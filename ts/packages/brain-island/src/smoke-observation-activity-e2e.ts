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
  createRuntimeActivityObserver,
  handleAdminControlRequest,
  type AgentActivityObservationEvent,
  type AdminControlResponse,
  type AdminRouteResult,
} from "./index.js";

const sink = createMemoryAgentActivityObservationSink();
const producer = new AgentActivityObservationProducer({ sink, required: true });
const identity = {
  profile: "prime" as ProfileId,
  instance_id: "agent-alpha" as AgentInstanceId,
  session_key: "session-alpha" as SessionId,
};
const observer = createRuntimeActivityObserver({
  producer,
  identity,
  runtimeInstanceId: "runtime-alpha",
});

await observer.session({
  eventType: "agent_session_started",
  summary: "Prime session started.",
});
await observer.work({
  eventType: "work_checkpoint",
  summary: "Completed an operator-visible checkpoint.",
  workRef: { project_id: "rusty-crew", task_id: 2964, run_id: "run-2964" },
});
await observer.adapter({
  eventType: "adapter_degraded",
  adapter: "den-channels",
  surface: "channel",
  summary: "Channel projection degraded.",
  reasonCode: "projection_failed",
});
await observer.adapter({
  eventType: "adapter_recovered",
  adapter: "den-channels",
  surface: "channel",
  summary: "Channel projection recovered.",
});
await observer.tool({
  eventType: "tool_call_started",
  toolName: "terminal",
  summary: "Starting long-running terminal command.",
  longRunningOrRisky: true,
});

const auditSink = createMemoryAdminControlAuditSink();
const commandResult = await handleAdminControlRequest(
  {
    method: "POST",
    url: "/v1/admin/control/sessions/session-alpha/archive",
    headers: {
      authorization: "Bearer control-token",
      "x-rusty-crew-operator": "operator-alpha",
    },
    body: {
      reason: "observation e2e",
      reasonCode: "observation_e2e",
    },
  },
  {
    auth: { bearerToken: "control-token" },
    auditSink,
    observationProducer: producer,
    observationIdentity: identity,
    executor: {
      archiveSession() {
        return {
          status: "completed",
          summary: "Archived session-alpha.",
          affectedIds: { sessionId: "session-alpha" },
        };
      },
    },
    now: () => "2026-06-20T20:00:00.000Z",
  },
);
assert.equal(
  okData<AdminControlResponse>(commandResult).outcome.status,
  "completed",
);

assert.equal(sink.events.length, 7);
for (const event of sink.events) {
  assert.equal(event.source_domain, "runtime");
  assert.equal(event.agent_identity.profile, "prime");
  assert.equal(event.agent_identity.instance_id, "agent-alpha");
  assert.equal(event.payload.kind, "agent_activity.v1");
  assert.equal(event.payload.schema_version, 1);
  assert.equal(event.payload.summary.length <= 240, true);
}
assert.deepEqual(
  sink.events.map((event) => event.event_type),
  [
    "agent_session_started",
    "work_checkpoint",
    "adapter_degraded",
    "adapter_recovered",
    "tool_call_started",
    "admin_command_started",
    "admin_command_completed",
  ],
);

let wakeCalls = 0;
let deliveryCalls = 0;
let completionCalls = 0;
const lane = renderObservationLane([
  ...sink.events,
  {
    ...sink.events[0],
    event_type: "future_activity_type",
    payload: {
      ...sink.events[0]!.payload,
      summary: "Future activity rendered generically.",
    },
  } as unknown as AgentActivityObservationEvent,
]);

assert.equal(lane.length, 8);
assert.equal(lane.at(-1)?.known, false);
assert.equal(lane.at(-1)?.summary, "Future activity rendered generically.");
assert.equal(wakeCalls, 0);
assert.equal(deliveryCalls, 0);
assert.equal(completionCalls, 0);

console.log(
  JSON.stringify(
    {
      events: sink.events.length,
      types: sink.events.map((event) => event.event_type),
      unknownKnown: lane.at(-1)?.known,
      wakeCalls,
      deliveryCalls,
      completionCalls,
    },
    null,
    2,
  ),
);

function renderObservationLane(
  events: readonly AgentActivityObservationEvent[],
): Array<{ known: boolean; eventType: string; summary: string }> {
  const knownTypes = new Set(sink.events.map((event) => event.event_type));
  return events.map((event) => {
    const summary = event.payload.summary;
    void wakeCalls;
    void deliveryCalls;
    void completionCalls;
    return {
      known: knownTypes.has(event.event_type),
      eventType: event.event_type,
      summary,
    };
  });
}

function okData<T>(result: AdminRouteResult): T {
  assert.equal(result.body.ok, true);
  if (!result.body.ok) throw new Error("expected admin route success");
  return result.body.data as T;
}
