import assert from "node:assert/strict";
import {
  AgentActivityObservationProducer,
  createMemoryAgentActivityObservationSink,
} from "../src/agent-activity-observation.js";
import { publishCuratorActivityObservation } from "../src/curator-observation.js";

const sink = createMemoryAgentActivityObservationSink();
const producer = new AgentActivityObservationProducer({
  sink,
  required: true,
});
const published = await publishCuratorActivityObservation({
  producer,
  receipt: {
    sequence: 7,
    receiptId: "receipt-7",
    correlationId: "curator:batch-7",
    profileId: "profile-7",
    sessionId: "session-7",
    candidateId: "candidate-7",
    activityKind: "mutation_applied",
    outcome: "accepted",
    summary: "Applied candidate 7",
    occurredAt: "2026-07-10T00:00:00Z",
  },
});
assert.equal(published.status, "published");
assert.equal(sink.events[0]?.event_type, "work_completed");
assert.equal(
  sink.events[0]?.payload.result_ref?.artifact_path,
  "curator://receipt/receipt-7",
);
assert.equal(sink.events[0]?.payload.work_ref?.run_id, "curator:batch-7");

sink.failNext(new Error("observation offline"));
const degraded = await publishCuratorActivityObservation({
  producer,
  receipt: {
    sequence: 8,
    receiptId: "receipt-8",
    correlationId: "curator:batch-7",
    profileId: "profile-7",
    candidateId: "candidate-8",
    activityKind: "mutation_failed",
    outcome: "failed",
    reasonCode: "curator_filesystem_mutation_failed",
    summary: "Candidate 8 failed",
    occurredAt: "2026-07-10T00:01:00Z",
  },
});
assert.equal(degraded.status, "degraded");
assert.match(degraded.message, /observation offline/);
assert.equal(sink.events.length, 1);

console.log("curator observation smoke passed");
