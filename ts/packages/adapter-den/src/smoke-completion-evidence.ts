import assert from "node:assert/strict";
import type {
  AdapterId,
  AgentId,
  ChannelBindingRecord,
  NormalizedChannelActivityProjection,
  ProfileId,
  ProjectId,
  RunId,
  SessionId,
} from "@rusty-crew/contracts";
import {
  completionPacketResultRef,
  denProductWorkRef,
  dispatchCompletionEvidenceProjection,
  projectCompletionEvidenceToChannelActivity,
  runtimeSessionWorkRef,
  toDenChannelsActivityRequest,
} from "./index.js";

const binding: ChannelBindingRecord = {
  bindingId: "binding-alpha",
  adapterId: "den-adapter" as AdapterId,
  provider: "den_channels",
  agentId: "agent-alpha" as AgentId,
  sessionId: "session-alpha" as SessionId,
  profileId: "profile-alpha" as ProfileId,
  externalChannelId: "channel-alpha",
  externalThreadId: "thread-alpha",
  status: "active",
};

const completionEvent = {
  type: "completion_packet_delivered" as const,
  packet: {
    sessionId: "session-alpha" as SessionId,
    status: "completed" as const,
    summary:
      "Completed the projection work. This summary is intentionally long enough to prove evidence summaries are bounded before they reach provider surfaces.",
  },
};

const activity = projectCompletionEvidenceToChannelActivity({
  binding,
  event: completionEvent,
  workRefs: [
    denProductWorkRef({
      refKind: "task",
      id: "3053",
      projectId: "rusty-crew" as ProjectId,
    }),
    runtimeSessionWorkRef("session-alpha"),
  ],
  resultRefs: [completionPacketResultRef(completionEvent.packet)],
  now: "2026-06-20T09:00:00Z",
  maxSummaryChars: 72,
});

assert.equal(activity.kind, "channel_activity_projection.v1");
assert.equal(activity.severity, "success");
assert.equal(activity.workRef, "den:task:3053");
assert.equal(
  activity.resultRef,
  "runtime:completion_packet:session-alpha:completed",
);
assert.equal(activity.workRefs?.length, 2);
assert.equal(activity.resultRefs?.length, 1);
assert.match(activity.summary, /\[truncated\]$/);
assert.equal(
  toDenChannelsActivityRequest(activity).metadata.resultRef,
  "runtime:completion_packet:session-alpha:completed",
);

let runtimeCompletionAccepted = true;
const projectedActivities: NormalizedChannelActivityProjection[] = [];
let failNext = true;
const sink = {
  sendMessage(): void {
    throw new Error("message projection should not be used");
  },
  sendActivity(projection: NormalizedChannelActivityProjection): void {
    if (failNext) {
      failNext = false;
      throw new Error("simulated Den evidence outage");
    }
    projectedActivities.push(projection);
  },
};

const dropped = await dispatchCompletionEvidenceProjection(sink, {
  binding,
  event: completionEvent,
  workRefs: activity.workRefs,
  resultRefs: activity.resultRefs,
  now: "2026-06-20T09:00:01Z",
});
assert.equal(dropped.dispatch.accepted, false);
assert.equal(runtimeCompletionAccepted, true);
assert.equal(projectedActivities.length, 0);

const accepted = await dispatchCompletionEvidenceProjection(sink, {
  binding,
  event: {
    type: "delegation_lifecycle_observed",
    lifecycle: {
      parentSessionId: "session-alpha" as SessionId,
      delegatedSessionId: "session-alpha:delegate:1" as SessionId,
      runId: "run-3053" as RunId,
      phase: "completed",
    },
  },
  workRefs: [denProductWorkRef({ refKind: "assignment", id: "assignment-1" })],
  now: "2026-06-20T09:00:02Z",
});
assert.equal(accepted.dispatch.accepted, true);
assert.equal(projectedActivities.length, 1);
assert.equal(projectedActivities[0]?.workRef, "den:assignment:assignment-1");
assert.equal(
  projectedActivities[0]?.resultRef,
  "runtime:runtime_event:delegation:session-alpha:delegate:1:completed",
);

runtimeCompletionAccepted = runtimeCompletionAccepted && true;

console.log(
  JSON.stringify(
    {
      completionResultRef: activity.resultRef,
      completionWorkRefs: activity.workRefs?.map(
        (ref) => `${ref.sourceDomain}:${ref.refKind}:${ref.id}`,
      ),
      dropped: dropped.dispatch,
      accepted: accepted.dispatch,
      runtimeCompletionAccepted,
    },
    null,
    2,
  ),
);
