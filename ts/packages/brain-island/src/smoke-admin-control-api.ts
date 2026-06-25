import assert from "node:assert/strict";
import type {
  AgentInstanceId,
  ProfileId,
  SessionId,
} from "@rusty-crew/contracts";
import {
  AgentActivityObservationProducer,
  createBackgroundAdminControlExecutor,
  createCuratorAdminControlExecutor,
  handleAdminControlRequest,
  type AdminControlExecutor,
  type AdminControlResponse,
  type AdminRouteResult,
} from "./index.js";
import {
  createMemoryAdminControlAuditSink,
  createMemoryAgentActivityObservationSink,
} from "./test-support.js";

const auditSink = createMemoryAdminControlAuditSink();
const observationSink = createMemoryAgentActivityObservationSink();
const observationProducer = new AgentActivityObservationProducer({
  sink: observationSink,
  required: true,
});
const curatorRequests: unknown[] = [];
const schedulerCalls: string[] = [];
const executor: AdminControlExecutor = {
  ...createBackgroundAdminControlExecutor({
    scheduler: {
      tick: () => {
        schedulerCalls.push("tick");
        return { dueRunsClaimed: 1, runsCompleted: 1 };
      },
      runJob: (jobId) => {
        schedulerCalls.push(`run:${jobId}`);
        return { jobId, status: "completed" };
      },
      pauseJob: (jobId) => {
        schedulerCalls.push(`pause:${jobId}`);
        return { jobId, status: "paused" };
      },
      resumeJob: (jobId, nextDueAt) => {
        schedulerCalls.push(`resume:${jobId}:${nextDueAt ?? ""}`);
        return { jobId, nextDueAt, status: "active" };
      },
    },
    cleanupDelegatedResources: () => ({
      runtime: {
        cleanedAt: "2026-06-20T15:00:00.000Z",
        terminalArchived: ["delegated-terminal" as SessionId],
        orphanedArchived: [],
        expiredArchived: ["delegated-expired" as SessionId],
        resourcesReleased: 0,
      },
      adapters: [],
      observation: {},
    }),
  }),
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
  readProfileConfig(command) {
    return {
      status: "completed",
      summary: `Read ${command.target.profileId}.`,
      affectedIds: { profileId: command.target.profileId ?? "" },
      result: { profileId: command.target.profileId, profileConfig: {} },
    };
  },
  planProfileUpdate(command) {
    return {
      status: "completed",
      summary: `Planned ${command.target.profileId}.`,
      affectedIds: { profileId: command.target.profileId ?? "" },
      result: { ok: true, profileId: command.target.profileId },
    };
  },
  applyProfileUpdate(command) {
    return {
      status: "completed",
      summary: `Updated ${command.target.profileId}.`,
      affectedIds: { profileId: command.target.profileId ?? "" },
      result: { ok: true, profileId: command.target.profileId },
    };
  },
  archiveSession(command) {
    return {
      status: "completed",
      summary: `Archived ${command.target.sessionId}.`,
      affectedIds: { sessionId: command.target.sessionId ?? "" },
    };
  },
  decommissionProfile(command) {
    return {
      status: "completed",
      summary: `Decommissioned ${command.target.profileId}.`,
      affectedIds: {
        profileId: command.target.profileId ?? "",
        sessionsArchived: 1,
      },
      result: {
        profileId: command.target.profileId,
        profileDirectoryPreserved: true,
        sessionsArchived: ["session-alpha"],
        removed: {
          brains: 1,
          sessions: 1,
          channelBindings: 1,
          mcpBindings: 1,
          scheduledJobs: 1,
        },
      },
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
  reloadConfig() {
    return {
      status: "completed",
      summary: "Runtime config reloaded.",
      affectedIds: { sessionsReactivated: 1 },
    };
  },
  planRuntimeConfigUpdate() {
    return {
      status: "completed",
      summary: "Runtime config draft valid.",
      result: { ok: true },
    };
  },
  applyRuntimeConfigUpdate() {
    return {
      status: "completed",
      summary: "Runtime config draft applied.",
      result: { ok: true },
    };
  },
  planRuntimeRebuild(command) {
    return {
      status: "completed",
      summary: "runtime rebuild plan prepared",
      affectedIds: {
        profileId: command.target.profileId ?? "prime",
        sessionId: command.target.sessionId ?? "session-alpha",
      },
      result: {
        scope: command.target.scope,
        profileId: command.target.profileId ?? "prime",
        sessionIds: [command.target.sessionId ?? "session-alpha"],
        applySupported: true,
        requiredAction: "brain_hot_swap_required",
        preservesSessionId: true,
        preservesHistory: true,
        queuedMessages: {
          action: "preserve_existing_queue_without_redelivery",
        },
      },
    };
  },
  applyRuntimeRebuild(command) {
    return {
      status: "completed",
      summary: "runtime rebuild applied",
      affectedIds: {
        profileId: command.target.profileId ?? "prime",
        sessionId: command.target.sessionId ?? "session-alpha",
      },
      result: {
        scope: command.target.scope,
        profileId: command.target.profileId ?? "prime",
        sessionIds: [command.target.sessionId ?? "session-alpha"],
        applySupported: true,
        apply: {
          status: "completed",
          handle: 1,
          implementationId: "prime-brain",
          audited: true,
        },
      },
    };
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

const reloadConfig = await handleAdminControlRequest(
  {
    method: "POST",
    url: "/v1/admin/control/config/reload",
    headers: authHeaders(),
    body: { reason: "operator edited service config" },
  },
  context,
);
assert.equal(reloadConfig.status, 200);
const reloadConfigData = okData<AdminControlResponse>(reloadConfig);
assert.equal(reloadConfigData.command.name, "reload_config");
assert.equal(reloadConfigData.outcome.status, "completed");

const readProfile = await handleAdminControlRequest(
  {
    method: "POST",
    url: "/v1/admin/control/profiles/prime/read",
    headers: authHeaders(),
  },
  context,
);
assert.equal(readProfile.status, 200);
const readProfileData = okData<AdminControlResponse>(readProfile);
assert.equal(readProfileData.command.name, "read_profile_config");
assert.equal(readProfileData.command.target.profileId, "prime");

const planProfileUpdate = await handleAdminControlRequest(
  {
    method: "POST",
    url: "/v1/admin/control/profiles/prime/update/plan",
    headers: authHeaders(),
    body: { profileConfig: { profileId: "prime" } },
  },
  context,
);
assert.equal(planProfileUpdate.status, 200);
const planProfileUpdateData = okData<AdminControlResponse>(planProfileUpdate);
assert.equal(planProfileUpdateData.command.name, "plan_profile_update");

const applyProfileUpdate = await handleAdminControlRequest(
  {
    method: "POST",
    url: "/v1/admin/control/profiles/prime/update/apply",
    headers: authHeaders(),
    body: { profileConfig: { profileId: "prime" } },
  },
  context,
);
assert.equal(applyProfileUpdate.status, 200);
const applyProfileUpdateData = okData<AdminControlResponse>(applyProfileUpdate);
assert.equal(applyProfileUpdateData.command.name, "apply_profile_update");

const planRuntimeConfigUpdate = await handleAdminControlRequest(
  {
    method: "POST",
    url: "/v1/admin/control/config/draft/plan",
    headers: authHeaders(),
    body: { runtimeConfig: {} },
  },
  context,
);
assert.equal(planRuntimeConfigUpdate.status, 200);
const planRuntimeConfigUpdateData = okData<AdminControlResponse>(
  planRuntimeConfigUpdate,
);
assert.equal(
  planRuntimeConfigUpdateData.command.name,
  "plan_runtime_config_update",
);

const applyRuntimeConfigUpdate = await handleAdminControlRequest(
  {
    method: "POST",
    url: "/v1/admin/control/config/draft/apply",
    headers: authHeaders(),
    body: { runtimeConfig: {} },
  },
  context,
);
assert.equal(applyRuntimeConfigUpdate.status, 200);
const applyRuntimeConfigUpdateData = okData<AdminControlResponse>(
  applyRuntimeConfigUpdate,
);
assert.equal(
  applyRuntimeConfigUpdateData.command.name,
  "apply_runtime_config_update",
);

const decommissionProfile = await handleAdminControlRequest(
  {
    method: "POST",
    url: "/v1/admin/control/profiles/prime/decommission",
    headers: authHeaders(),
    body: { reason: "operator removed profile" },
  },
  context,
);
assert.equal(decommissionProfile.status, 200);
const decommissionProfileData =
  okData<AdminControlResponse>(decommissionProfile);
assert.equal(decommissionProfileData.command.name, "decommission_profile");
assert.equal(decommissionProfileData.command.target.profileId, "prime");
assert.equal(decommissionProfileData.outcome.status, "completed");
assert.equal(
  (
    decommissionProfileData.outcome.result as {
      profileDirectoryPreserved?: boolean;
    }
  ).profileDirectoryPreserved,
  true,
);

const rebuildSessionPlan = await handleAdminControlRequest(
  {
    method: "POST",
    url: "/v1/admin/control/sessions/session-alpha/rebuild-runtime/plan",
    headers: authHeaders(),
  },
  context,
);
assert.equal(rebuildSessionPlan.status, 200);
const rebuildSessionPlanData = okData<AdminControlResponse>(rebuildSessionPlan);
assert.equal(rebuildSessionPlanData.command.name, "plan_runtime_rebuild");
assert.equal(rebuildSessionPlanData.command.target.scope, "session");
assert.equal(rebuildSessionPlanData.command.target.sessionId, "session-alpha");
assert.equal(rebuildSessionPlanData.outcome.status, "completed");
assert.equal(
  (rebuildSessionPlanData.outcome.result as { preservesSessionId?: boolean })
    .preservesSessionId,
  true,
);

const rebuildProfileApply = await handleAdminControlRequest(
  {
    method: "POST",
    url: "/v1/admin/control/profiles/prime/rebuild-brain/apply",
    headers: authHeaders(),
  },
  context,
);
assert.equal(rebuildProfileApply.status, 200);
const rebuildProfileApplyData =
  okData<AdminControlResponse>(rebuildProfileApply);
assert.equal(rebuildProfileApplyData.command.name, "apply_runtime_rebuild");
assert.equal(rebuildProfileApplyData.command.target.scope, "profile");
assert.equal(rebuildProfileApplyData.command.target.profileId, "prime");
assert.equal(rebuildProfileApplyData.outcome.status, "completed");
assert.equal(
  (
    rebuildProfileApplyData.outcome.result as {
      apply?: { status?: string; audited?: boolean };
    }
  ).apply?.audited,
  true,
);

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

const schedulerTick = await handleAdminControlRequest(
  {
    method: "POST",
    url: "/v1/admin/control/scheduler/tick",
    headers: authHeaders(),
  },
  context,
);
assert.equal(schedulerTick.status, 200);
assert.equal(schedulerCalls.at(-1), "tick");

const schedulerRun = await handleAdminControlRequest(
  {
    method: "POST",
    url: "/v1/admin/control/scheduler/jobs/wake-prime/run",
    headers: authHeaders(),
  },
  context,
);
assert.equal(schedulerRun.status, 200);
assert.equal(schedulerCalls.at(-1), "run:wake-prime");

const schedulerResume = await handleAdminControlRequest(
  {
    method: "POST",
    url: "/v1/admin/control/scheduler/jobs/wake-prime/resume",
    headers: authHeaders(),
    body: { nextDueAt: "2026-06-20T15:05:00.000Z" },
  },
  context,
);
assert.equal(schedulerResume.status, 200);
assert.equal(
  schedulerCalls.at(-1),
  "resume:wake-prime:2026-06-20T15:05:00.000Z",
);

const cleanupRun = await handleAdminControlRequest(
  {
    method: "POST",
    url: "/v1/admin/control/cleanup/delegated/run",
    headers: authHeaders(),
  },
  context,
);
assert.equal(cleanupRun.status, 200);
const cleanupRunData = okData<AdminControlResponse>(cleanupRun);
assert.equal(cleanupRunData.command.name, "cleanup_delegated_resources");
assert.equal(cleanupRunData.outcome.status, "completed");

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
      schedulerRun: okData<AdminControlResponse>(schedulerRun).outcome.status,
      cleanupRun: cleanupRunData.outcome.status,
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
