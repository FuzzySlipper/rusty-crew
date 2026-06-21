import assert from "node:assert/strict";
import type {
  AgentInstanceId,
  ProfileId,
  SessionId,
} from "@rusty-crew/contracts";
import {
  AgentActivityObservationProducer,
  createRuntimeActivityObserver,
  type AgentObservationIdentity,
} from "./index.js";
import { createMemoryAgentActivityObservationSink } from "./test-support.js";

const identity: AgentObservationIdentity = {
  profile: "prime" as ProfileId,
  instance_id: "agent-alpha" as AgentInstanceId,
  session_key: "session-alpha" as SessionId,
};
const sink = createMemoryAgentActivityObservationSink();
const observer = createRuntimeActivityObserver({
  producer: new AgentActivityObservationProducer({ sink, required: true }),
  identity,
  runtimeInstanceId: "runtime-alpha",
});

const started = await observer.session({
  eventType: "agent_session_started",
  summary: "Session started.",
});
assert.equal(started.status, "published");

const checkpoint = await observer.work({
  eventType: "work_checkpoint",
  summary: "Finished safe checkpoint.",
  workRef: { project_id: "rusty-crew", task_id: 2962 },
});
assert.equal(checkpoint.status, "published");

const tinyTool = await observer.tool({
  eventType: "tool_call_started",
  toolName: "read_file",
  summary: "Small read.",
});
assert.equal(tinyTool.status, "suppressed");
assert.equal(tinyTool.reasonCode, "low_signal_tool_call");

const riskyTool = await observer.tool({
  eventType: "tool_call_started",
  toolName: "terminal",
  summary: "Starting bounded terminal command.",
  longRunningOrRisky: true,
});
assert.equal(riskyTool.status, "published");

const failedTool = await observer.tool({
  eventType: "tool_call_failed",
  toolName: "mcp_apply_patch",
  summary: "Patch tool failed.",
  reasonCode: "tool_failed",
});
assert.equal(failedTool.status, "published");

const degraded = await observer.adapter({
  eventType: "adapter_degraded",
  adapter: "den-channels",
  surface: "channel",
  summary: "Channel projection degraded.",
  reasonCode: "projection_failed",
});
assert.equal(degraded.status, "published");

const recovered = await observer.adapter({
  eventType: "adapter_recovered",
  adapter: "den-channels",
  surface: "channel",
  summary: "Channel projection recovered.",
});
assert.equal(recovered.status, "published");

const missingRequired = await createRuntimeActivityObserver({
  producer: new AgentActivityObservationProducer({ required: true }),
  identity,
}).session({
  eventType: "agent_session_failed",
  summary: "Required observation sink missing.",
  reasonCode: "observation_unavailable",
});
assert.equal(missingRequired.status, "degraded");

assert.deepEqual(
  sink.events.map((event) => event.event_type),
  [
    "agent_session_started",
    "work_checkpoint",
    "tool_call_started",
    "tool_call_failed",
    "adapter_degraded",
    "adapter_recovered",
  ],
);
assert.equal(
  sink.events.every((event) => event.payload.kind === "agent_activity.v1"),
  true,
);
assert.equal(sink.events[0]?.runtime_instance_id, "runtime-alpha");
assert.equal(sink.events[2]?.payload.visibility, "debug");

console.log(
  JSON.stringify(
    {
      published: sink.events.length,
      suppressed: tinyTool.status,
      first: sink.events[0]?.event_type,
      riskyTool: sink.events[2]?.payload.tool_name,
      degraded: missingRequired.status,
    },
    null,
    2,
  ),
);
