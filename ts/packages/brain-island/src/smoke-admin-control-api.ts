import assert from "node:assert/strict";
import type {
  AgentInstanceId,
  ProfileId,
  SessionId,
} from "@rusty-crew/contracts";
import {
  AgentActivityObservationProducer,
  createCuratorAdminControlExecutor,
  createMemoryAdminControlAuditSink,
  createMemoryAgentActivityObservationSink,
  handleAdminControlRequest,
  type AdminControlExecutor,
  type AdminControlResponse,
  type AdminRouteResult,
} from "./index.js";

const auditSink = createMemoryAdminControlAuditSink();
const observationSink = createMemoryAgentActivityObservationSink();
const observationProducer = new AgentActivityObservationProducer({
  sink: observationSink,
  required: true,
});
const curatorRequests: unknown[] = [];
const executor: AdminControlExecutor = {
  ...createCuratorAdminControlExecutor({
    curatorExecutor(request) {
      curatorRequests.push(request);
      return {
        receiptId: `curator-${curatorRequests.length}`,
        status:
          request.action === "approve_candidate"
            ? "approved"
            : request.action === "apply_candidate"
              ? "applied"
              : request.action === "preview_candidate"
                ? "previewed"
                : "requested",
        candidateId: request.candidateId,
        auditRef: `audit-${curatorRequests.length}`,
        summary: `Curator ${request.action} completed.`,
      };
    },
    status: () => ({
      status: "available",
      candidateCount: 1,
      mutationCount: 0,
    }),
    rollbackMutation: (mutationId) => ({
      mutationId,
      candidateId: "candidate-alpha",
      action: "skill_patch",
      actorId: "operator-alpha",
      reason: "rollback smoke",
      appliedAt: "2026-06-20T15:00:00.000Z",
      status: "rolled_back",
      rollbackRef: `curator-rollback:${mutationId}`,
      snapshot: {
        snapshotId: "snapshot-alpha",
        snapshotDir: "/tmp/snapshot-alpha",
        createdAt: "2026-06-20T15:00:00.000Z",
        skillPath: "/tmp/skill.md",
        skillExisted: true,
      },
      changedPaths: ["/tmp/skill.md"],
    }),
  }),
  archiveSession(command) {
    return {
      status: "completed",
      summary: `Archived ${command.target.sessionId}.`,
      affectedIds: { sessionId: command.target.sessionId ?? "" },
    };
  },
  cancelDelegation(command) {
    return {
      status: "completed",
      summary: `Cancelled ${command.target.sessionId}.`,
      affectedIds: { delegatedSessionId: command.target.sessionId ?? "" },
      result: { bearerToken: "must-not-leak" },
    };
  },
  reloadMcp() {
    throw new Error("reload failed");
  },
};

const context = {
  auth: { bearerToken: "control-token", operatorId: "operator-alpha" },
  executor,
  auditSink,
  observationProducer,
  observationIdentity: {
    profile: "operator" as ProfileId,
    instance_id: "rusty-crew-admin" as AgentInstanceId,
    session_key: "admin-session" as SessionId,
  },
  now: () => "2026-06-20T15:00:00.000Z",
};

const unauthorized = await handleAdminControlRequest(
  { method: "POST", url: "/v1/admin/control/sessions/session-alpha/archive" },
  context,
);
assert.equal(unauthorized.status, 401);
assert.equal(unauthorized.body.ok, false);

const unsupported = await handleAdminControlRequest(
  {
    method: "POST",
    url: "/v1/admin/control/sessions/session-alpha/new",
    headers: authHeaders(),
  },
  context,
);
assert.equal(unsupported.status, 412);
assert.equal(unsupported.body.ok, false);
assert.equal(auditSink.events.length, 0);

const archive = await handleAdminControlRequest(
  {
    method: "POST",
    url: "/v1/admin/control/sessions/session-alpha/archive",
    headers: authHeaders({ "idempotency-key": "idem-1" }),
    body: {
      reason: "operator requested reset",
      reasonCode: "operator_reset",
      denRefs: { project_id: "rusty-crew", task_id: 2953 },
    },
    requestId: "req-archive",
  },
  context,
);
assert.equal(archive.status, 200);
const archiveData = okData<AdminControlResponse>(archive);
assert.equal(archiveData.command.name, "archive_session");
assert.equal(archiveData.command.actor.operatorId, "operator-alpha");
assert.equal(archiveData.command.idempotencyKey, "idem-1");
assert.equal(archiveData.outcome.status, "completed");
assert.equal(archiveData.observation.started, "published");
assert.equal(archiveData.observation.terminal, "published");
assert.equal(auditSink.events.length, 2);
assert.equal(auditSink.events[0]?.phase, "started");
assert.equal(auditSink.events[1]?.phase, "completed");
assert.equal(observationSink.events.length, 2);
assert.equal(observationSink.events[0]?.event_type, "admin_command_started");
assert.equal(observationSink.events[1]?.event_type, "admin_command_completed");

const cancel = await handleAdminControlRequest(
  {
    method: "POST",
    url: "/v1/admin/control/delegations/delegated-alpha/cancel",
    headers: authHeaders({ "x-rusty-crew-operator": "operator-beta" }),
  },
  context,
);
assert.equal(cancel.status, 200);
const cancelData = okData<AdminControlResponse>(cancel);
assert.equal(cancelData.command.actor.operatorId, "operator-beta");
assert.equal(
  (cancelData.outcome.result as { bearerToken?: string }).bearerToken,
  "[redacted]",
);

const invalidCheckpoint = await handleAdminControlRequest(
  {
    method: "POST",
    url: "/v1/admin/control/delegations/delegated-alpha/checkpoint",
    headers: authHeaders(),
  },
  context,
);
assert.equal(invalidCheckpoint.status, 400);
assert.equal(invalidCheckpoint.body.ok, false);

const reloadFailure = await handleAdminControlRequest(
  {
    method: "POST",
    url: "/v1/admin/control/mcp/session-alpha/reload",
    headers: authHeaders(),
  },
  context,
);
assert.equal(reloadFailure.status, 500);
const reloadFailureData = okData<AdminControlResponse>(reloadFailure);
assert.equal(reloadFailureData.outcome.status, "failed");
assert.equal(reloadFailureData.outcome.reasonCode, "control_executor_failed");

const curatorStatus = await handleAdminControlRequest(
  {
    method: "POST",
    url: "/v1/admin/control/curator/status",
    headers: authHeaders(),
  },
  context,
);
assert.equal(curatorStatus.status, 200);
const curatorStatusData = okData<AdminControlResponse>(curatorStatus);
assert.equal(curatorStatusData.command.name, "curator_status");
assert.equal(
  (curatorStatusData.outcome.result as { status?: string }).status,
  "available",
);

const curatorRun = await handleAdminControlRequest(
  {
    method: "POST",
    url: "/v1/admin/control/curator/run",
    headers: authHeaders(),
    body: { scopeType: "profile", scopeId: "prime", dryRun: true },
  },
  context,
);
assert.equal(curatorRun.status, 200);
assert.equal(
  (curatorRequests.at(-1) as { action?: string; scopeId?: string }).action,
  "request_scan",
);
assert.equal(
  (curatorRequests.at(-1) as { action?: string; scopeId?: string }).scopeId,
  "prime",
);

const curatorApply = await handleAdminControlRequest(
  {
    method: "POST",
    url: "/v1/admin/control/curator/candidates/candidate-alpha/apply",
    headers: authHeaders(),
    body: { reason: "operator apply", dryRun: false },
  },
  context,
);
assert.equal(curatorApply.status, 200);
const curatorApplyData = okData<AdminControlResponse>(curatorApply);
assert.equal(curatorApplyData.command.name, "curator_apply_candidate");
assert.equal(
  (curatorRequests.at(-1) as { action?: string; dryRun?: boolean }).action,
  "apply_candidate",
);
assert.equal(
  (curatorRequests.at(-1) as { action?: string; dryRun?: boolean }).dryRun,
  false,
);

const curatorRollback = await handleAdminControlRequest(
  {
    method: "POST",
    url: "/v1/admin/control/curator/mutations/mutation-alpha/rollback",
    headers: authHeaders(),
  },
  context,
);
assert.equal(curatorRollback.status, 200);
const curatorRollbackData = okData<AdminControlResponse>(curatorRollback);
assert.equal(curatorRollbackData.command.name, "curator_rollback_mutation");
assert.equal(
  curatorRollbackData.outcome.affectedIds?.mutationId,
  "mutation-alpha",
);

const auditUnavailable = createMemoryAdminControlAuditSink();
auditUnavailable.failNext();
const auditFailure = await handleAdminControlRequest(
  {
    method: "POST",
    url: "/v1/admin/control/sessions/session-alpha/archive",
    headers: authHeaders(),
  },
  { ...context, auditSink: auditUnavailable },
);
assert.equal(auditFailure.status, 412);
assert.equal(auditFailure.body.ok, false);

const wrongMethod = await handleAdminControlRequest(
  {
    method: "GET",
    url: "/v1/admin/control/sessions/session-alpha/archive",
    headers: authHeaders(),
  },
  context,
);
assert.equal(wrongMethod.status, 405);
assert.equal(wrongMethod.body.ok, false);

console.log(
  JSON.stringify(
    {
      archive: archiveData.outcome.status,
      auditEvents: auditSink.events.length,
      observationEvents: observationSink.events.length,
      cancelActor: cancelData.command.actor.operatorId,
      invalidCheckpoint: invalidCheckpoint.status,
      reloadFailure: reloadFailureData.outcome.reasonCode,
      curatorApply: curatorApplyData.outcome.status,
      curatorRollback: curatorRollbackData.outcome.status,
      auditFailure: auditFailure.status,
    },
    null,
    2,
  ),
);

function authHeaders(
  extra: Record<string, string> = {},
): Record<string, string> {
  return {
    authorization: "Bearer control-token",
    ...extra,
  };
}

function okData<T>(result: AdminRouteResult): T {
  assert.equal(result.body.ok, true);
  if (!result.body.ok) throw new Error("expected admin route success");
  return result.body.data as T;
}
