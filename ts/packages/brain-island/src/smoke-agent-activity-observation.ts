import assert from "node:assert/strict";
import type {
  AgentId,
  AgentInstanceId,
  ProfileId,
  SessionId,
} from "@rusty-crew/contracts";
import {
  adapterActivity,
  adminCommandActivity,
  AgentActivityObservationProducer,
  createAgentActivityObservationEvent,
  sessionActivity,
  toolActivity,
  workActivity,
  type AgentObservationIdentity,
} from "./index.js";
import { createMemoryAgentActivityObservationSink } from "./test-support.js";

const identity: AgentObservationIdentity = {
  profile: "prime" as ProfileId,
  instance_id: "prime@local" as AgentInstanceId,
  session_key: "session-alpha" as SessionId,
};
const sink = createMemoryAgentActivityObservationSink();
const producer = new AgentActivityObservationProducer({ sink, required: true });

const started = await producer.publish(
  sessionActivity({
    eventType: "agent_session_started",
    identity,
    summary: "Prime session is ready for work.",
  }),
);
assert.equal(started.status, "published");
assert.equal(started.event.source_domain, "runtime");
assert.equal(started.event.payload.kind, "agent_activity.v1");
assert.equal(started.event.payload.schema_version, 1);
assert.equal(started.event.payload.session_key, "session-alpha");
assert.equal(started.event.payload.severity, "info");

const checkpoint = await producer.publish(
  workActivity({
    eventType: "work_checkpoint",
    identity,
    summary: "Completed planning pass.",
    workRef: {
      project_id: "rusty-crew",
      task_id: 2949,
      run_id: "run-2949",
      session_id: "session-alpha" as SessionId,
    },
  }),
);
assert.equal(checkpoint.status, "published");
assert.equal(checkpoint.event.payload.visibility, "task");
assert.equal(checkpoint.event.payload.work_ref?.task_id, 2949);

const toolCompleted = await producer.publish(
  toolActivity({
    eventType: "tool_call_completed",
    identity,
    toolName: "mcp_search",
    summary: "MCP search completed.",
    resultRef: { message_id: 16005 },
  }),
);
assert.equal(toolCompleted.status, "published");
assert.equal(toolCompleted.event.payload.tool_name, "mcp_search");
assert.equal(toolCompleted.event.payload.severity, "success");

const adapterDegraded = await producer.publish(
  adapterActivity({
    eventType: "adapter_degraded",
    identity,
    adapter: "den-channels",
    surface: "channel",
    reasonCode: "subscription_cursor_stale",
    summary: "Den Channels subscription cursor is stale.",
  }),
);
assert.equal(adapterDegraded.status, "published");
assert.equal(
  adapterDegraded.event.payload.reason_code,
  "subscription_cursor_stale",
);
assert.equal(adapterDegraded.event.payload.severity, "warning");

const adminDone = await producer.publish(
  adminCommandActivity({
    eventType: "admin_command_completed",
    identity,
    commandName: "/status",
    summary: "Status command completed.",
    resultRef: { message_id: 16006 },
  }),
);
assert.equal(adminDone.status, "published");
assert.equal(adminDone.event.payload.work_ref?.run_id, "command:/status");

const longSummary = createAgentActivityObservationEvent({
  eventType: "work_started",
  identity: {
    profile: "review" as ProfileId,
    instance_id: "agent-review" as AgentId,
  },
  summary: "x".repeat(260),
  workRef: { project_id: "rusty-crew", task_id: 2949 },
});
assert.equal(longSummary.payload.summary.length, 240);
assert.match(longSummary.payload.summary, /\.\.\.$/);

sink.failNext(new Error("observation endpoint unavailable"));
const failed = await producer.publish(
  workActivity({
    eventType: "work_failed",
    identity,
    summary: "Observation write failure should degrade visibly.",
    workRef: { project_id: "rusty-crew", task_id: 2949 },
    reasonCode: "observation_write_failed",
  }),
);
assert.equal(failed.status, "degraded");
assert.equal(failed.reasonCode, "observation_unavailable");

const missingRequired = await new AgentActivityObservationProducer({
  required: true,
}).publish(
  sessionActivity({
    eventType: "agent_session_idle",
    identity,
    summary: "Idle event without configured sink.",
  }),
);
assert.equal(missingRequired.status, "degraded");

const missingOptional = await new AgentActivityObservationProducer().publish(
  sessionActivity({
    eventType: "agent_session_idle",
    identity,
    summary: "Optional observation sink missing.",
  }),
);
assert.equal(missingOptional.status, "skipped");

assert.equal(sink.events.length, 5);

console.log(
  JSON.stringify(
    {
      published: sink.events.length,
      firstType: sink.events[0]?.event_type,
      toolSeverity: toolCompleted.event.payload.severity,
      failedStatus: failed.status,
      missingRequired: missingRequired.status,
      missingOptional: missingOptional.status,
    },
    null,
    2,
  ),
);
