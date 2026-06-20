import assert from "node:assert/strict";
import type {
  AgentInstanceId,
  ProfileId,
  SessionId,
} from "@rusty-crew/contracts";
import {
  AgentActivityObservationProducer,
  createMemoryAdminControlAuditSink,
  createMemoryAgentActivityObservationSink,
  createMemoryNewSessionLifecycleAuditSink,
  createNewSessionLifecycleExecutor,
  handleAdminControlRequest,
  type AdminControlResponse,
  type AdminRouteResult,
  type NewSessionTemplate,
} from "./index.js";

const order: string[] = [];
const lifecycleAudit = createMemoryNewSessionLifecycleAuditSink();
const adminAudit = createMemoryAdminControlAuditSink();
const observationSink = createMemoryAgentActivityObservationSink();
const observationProducer = new AgentActivityObservationProducer({
  sink: observationSink,
  required: true,
});
const template: NewSessionTemplate = {
  agentId: "agent-alpha",
  profileId: "prime",
  kind: "full",
  channelBindingId: "binding-alpha",
  channelId: "crew-room",
  toolProfileKey: "prime-tools",
};

const newSession = createNewSessionLifecycleExecutor({
  loadTemplate(sessionId) {
    order.push(`load:${sessionId}`);
    return template;
  },
  generateSessionId() {
    order.push("generate");
    return "session-alpha-new";
  },
  archiveSession(input) {
    order.push(`archive:${input.sessionId}:${input.reasonCode}`);
  },
  createSession(input) {
    order.push(`create:${input.sessionId}:${input.template.agentId}`);
  },
  rebindChannel(input) {
    order.push(
      `rebind:${input.template.channelBindingId}:${input.oldSessionId}->${input.newSessionId}`,
    );
  },
  auditSink: lifecycleAudit,
  observationProducer,
  observationIdentity({ template: currentTemplate, sessionId }) {
    return {
      profile: currentTemplate.profileId as ProfileId,
      instance_id: currentTemplate.agentId as AgentInstanceId,
      session_key: sessionId as SessionId,
    };
  },
  now: () => "2026-06-20T16:00:00.000Z",
});

const routeResult = await handleAdminControlRequest(
  {
    method: "POST",
    url: "/v1/admin/control/sessions/session-alpha/new",
    headers: {
      authorization: "Bearer control-token",
      "x-rusty-crew-operator": "operator-alpha",
    },
    body: {
      reason: "fresh planning context",
      reasonCode: "slash_command_new",
    },
  },
  {
    auth: { bearerToken: "control-token" },
    executor: { newSession },
    auditSink: adminAudit,
    now: () => "2026-06-20T16:00:00.000Z",
  },
);

assert.equal(routeResult.status, 200);
const data = okData<AdminControlResponse>(routeResult);
assert.equal(data.outcome.status, "completed");
assert.equal(data.outcome.affectedIds?.oldSessionId, "session-alpha");
assert.equal(data.outcome.affectedIds?.newSessionId, "session-alpha-new");
assert.equal(
  (data.outcome.result as { reattachedChannelBinding?: boolean })
    .reattachedChannelBinding,
  true,
);
assert.deepEqual(order, [
  "load:session-alpha",
  "generate",
  "archive:session-alpha:slash_command_new",
  "create:session-alpha-new:agent-alpha",
  "rebind:binding-alpha:session-alpha->session-alpha-new",
]);
assert.deepEqual(
  lifecycleAudit.events.map((event) => event.phase),
  [
    "template_loaded",
    "archive_started",
    "archived",
    "create_started",
    "created",
    "binding_rebind_started",
    "binding_rebound",
  ],
);
assert.deepEqual(
  observationSink.events.map((event) => event.event_type),
  ["agent_session_stopped", "agent_session_started"],
);
assert.equal(
  observationSink.events[0]?.agent_identity.session_key,
  "session-alpha",
);
assert.equal(
  observationSink.events[1]?.agent_identity.session_key,
  "session-alpha-new",
);

const duplicateId = await createNewSessionLifecycleExecutor({
  loadTemplate: () => template,
  generateSessionId: () => "session-alpha",
  archiveSession: () => {
    throw new Error("should not archive");
  },
  createSession: () => {
    throw new Error("should not create");
  },
})({
  name: "new_session",
  target: { sessionId: "session-alpha" },
  actor: { operatorId: "operator-alpha" },
  requestId: "req-new",
  reason: "duplicate",
  reasonCode: "slash_command_new",
  body: {},
});
assert.equal(duplicateId.status, "failed");
assert.equal(duplicateId.reasonCode, "new_session_identity_not_distinct");

const missingRebind = await createNewSessionLifecycleExecutor({
  loadTemplate: () => template,
  generateSessionId: () => "session-beta",
  archiveSession: () => {
    throw new Error("should not archive without rebind");
  },
  createSession: () => {
    throw new Error("should not create without rebind");
  },
})({
  name: "new_session",
  target: { sessionId: "session-alpha" },
  actor: { operatorId: "operator-alpha" },
  requestId: "req-new",
  reason: "missing rebind",
  reasonCode: "slash_command_new",
  body: {},
});
assert.equal(missingRebind.status, "failed");
assert.equal(missingRebind.reasonCode, "missing_channel_rebind");

console.log(
  JSON.stringify(
    {
      status: data.outcome.status,
      oldSessionId: data.outcome.affectedIds?.oldSessionId,
      newSessionId: data.outcome.affectedIds?.newSessionId,
      lifecycleSteps: lifecycleAudit.events.length,
      observationEvents: observationSink.events.length,
      duplicateId: duplicateId.reasonCode,
      missingRebind: missingRebind.reasonCode,
    },
    null,
    2,
  ),
);

function okData<T>(result: AdminRouteResult): T {
  assert.equal(result.body.ok, true);
  if (!result.body.ok) throw new Error("expected admin route success");
  return result.body.data as T;
}
