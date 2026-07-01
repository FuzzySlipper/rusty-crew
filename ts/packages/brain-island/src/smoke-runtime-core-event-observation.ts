import assert from "node:assert/strict";
import type {
  AgentId,
  CoreEvent,
  ProfileId,
  ProjectId,
  SessionHandle,
  SessionId,
  SessionState,
} from "@rusty-crew/contracts";
import { runtimeCoreEventObservationInput } from "./runtime-core-event-observation.js";

const sessionId = "runner-session" as SessionId;
const agentId = "rusty-crew-runner" as AgentId;
const profileId = "rusty-crew-runner" as ProfileId;

const session: SessionState = {
  handle: 1 as SessionHandle,
  sessionId,
  agentId,
  profileId,
  kind: "full",
  resourceLimits: {},
  toolProfile: {
    tools: [],
  },
  status: "active",
  brainTurnCount: 0,
  createdAt: "2026-07-01T00:00:00.000Z",
  lastActiveAt: "2026-07-01T00:00:00.000Z",
};

const started = runtimeCoreEventObservationInput({
  type: "session_created",
  state: session,
});
assert.equal(started?.eventType, "agent_session_started");
assert.equal(started?.identity.profile, "rusty-crew-runner");
assert.equal(started?.sessionKey, "runner-session");

const privateMessage = runtimeCoreEventObservationInput({
  type: "agent_message_routed",
  message: {
    from: agentId,
    to: "asha-runner" as AgentId,
    body: "internal note",
  },
});
assert.equal(privateMessage, undefined);

const projectedMessage = runtimeCoreEventObservationInput({
  type: "agent_message_routed",
  message: {
    from: agentId,
    to: "operator" as AgentId,
    body: "ready for review",
    correlationId: "review-1",
    projection: {
      visibility: "user_visible",
      targetRef: {
        system: "den",
        kind: "project",
        id: "rusty-crew",
      },
      workRef: {
        system: "den",
        kind: "task",
        id: "3873",
      },
      reason: "Operator-visible projection proof.",
    },
  },
});
assert.equal(projectedMessage?.eventType, "work_checkpoint");
assert.equal(projectedMessage?.visibility, "channel");
assert.equal(projectedMessage?.workRef?.project_id, "rusty-crew");
assert.equal(projectedMessage?.workRef?.task_id, "3873");
assert.equal(projectedMessage?.resultRef?.message_id, "review-1");

const completion = runtimeCoreEventObservationInput(
  {
    type: "completion_packet_delivered",
    packet: {
      sessionId,
      status: "completed",
      summary: "Delegated work completed.",
    },
  },
  (sessionId) =>
    sessionId === "runner-session"
      ? {
          sessionId: session.sessionId,
          agentId: session.agentId,
          profileId: session.profileId,
        }
      : undefined,
);
assert.equal(completion?.eventType, "work_completed");
assert.equal(completion?.identity.instance_id, "rusty-crew-runner@rusty-crew");
assert.equal(completion?.workRef?.session_id, "runner-session");

const suppressedCompletion = runtimeCoreEventObservationInput(
  {
    type: "completion_packet_delivered",
    packet: {
      sessionId,
      status: "completed",
      summary: "Should not project.",
    },
  },
  {
    lookupSession: (candidate) =>
      candidate === sessionId
        ? {
            sessionId: session.sessionId,
            agentId: session.agentId,
            profileId: session.profileId,
            kind: session.kind,
          }
        : undefined,
    filters: [
      { eventKind: "completion_packet_delivered", completionStatus: "failed" },
    ],
  },
);
assert.equal(suppressedCompletion, undefined);

const fullOnlyWake = runtimeCoreEventObservationInput(
  {
    type: "brain_wake_requested",
    sessionId,
  },
  {
    lookupSession: (candidate) =>
      candidate === sessionId
        ? {
            sessionId: session.sessionId,
            agentId: session.agentId,
            profileId: session.profileId,
            kind: session.kind,
          }
        : undefined,
    filters: [{ eventKind: "brain_wake_requested", sessionKind: "full" }],
  },
);
assert.equal(fullOnlyWake?.eventType, "model_turn_started");

const noMessageFilter = runtimeCoreEventObservationInput(
  {
    type: "agent_message_routed",
    message: {
      from: agentId,
      to: "operator" as AgentId,
      body: "ready for review",
      projection: {
        visibility: "user_visible",
        reason: "Operator-visible projection proof.",
      },
    },
  },
  { filters: [{ eventKind: "completion_packet_delivered" }] },
);
assert.equal(noMessageFilter, undefined);

const ignored: CoreEvent = {
  type: "den_data_updated",
  update: {
    projectId: "rusty-crew" as ProjectId,
    entityKind: "task",
    entityId: "3873",
  },
};
assert.equal(runtimeCoreEventObservationInput(ignored), undefined);

console.log(
  JSON.stringify({
    ok: true,
    projectedMessage: projectedMessage?.workRef,
    completion: completion?.eventType,
  }),
);
