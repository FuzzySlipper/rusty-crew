import assert from "node:assert/strict";
import {
  AgentActivityObservationProducer,
  createMemoryAgentActivityObservationSink,
  publishBackgroundGovernanceObservation,
} from "./index.js";

const sink = createMemoryAgentActivityObservationSink();
const producer = new AgentActivityObservationProducer({ sink, required: true });
const identity = {
  profile: "operator",
  instance_id: "background-governance",
  session_key: "background-session",
};

const schedulerStarted = await publishBackgroundGovernanceObservation({
  producer,
  identity,
  loopKind: "scheduler",
  phase: "started",
  summary: "Scheduler tick started.",
  workRef: { run_id: "scheduler-run-1" },
});
const curatorCompleted = await publishBackgroundGovernanceObservation({
  producer,
  identity,
  loopKind: "curator",
  phase: "completed",
  summary: "Curator scan completed.",
  resultRef: { document_slug: "curator-report" },
});
const adapterDegraded = await publishBackgroundGovernanceObservation({
  producer,
  identity,
  loopKind: "adapter_check",
  phase: "degraded",
  adapter: "mcp",
  summary: "MCP background check degraded.",
  reasonCode: "mcp_unavailable",
});

assert.equal(schedulerStarted?.status, "published");
assert.equal(curatorCompleted?.status, "published");
assert.equal(adapterDegraded?.status, "published");
assert.equal(sink.events.length, 3);
assert.equal(sink.events[0]?.event_type, "work_started");
assert.equal(sink.events[1]?.event_type, "work_completed");
assert.equal(
  sink.events[1]?.payload.result_ref?.document_slug,
  "curator-report",
);
assert.equal(sink.events[2]?.event_type, "adapter_degraded");
assert.equal(sink.events[2]?.payload.adapter, "mcp");

console.log("background governance observation smoke passed");
