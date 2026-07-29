import assert from "node:assert/strict";
import test from "node:test";

import type { CoreEvent, SessionState } from "@rusty-crew/contracts";

import {
  appendCoreEventsToChatLog,
  type ServiceWakeDispatchContext,
} from "../src/service-wake-dispatch.js";

test("chat terminal events keep completion before turn finished", async () => {
  const appended: Array<{ kind: string; payload: unknown }> = [];
  const context = {
    appendChatEvent: async (
      _sessionId: string,
      event: { kind: string; payload: unknown },
    ) => {
      appended.push(event);
      return event;
    },
  } as unknown as ServiceWakeDispatchContext;
  const session = {
    sessionId: "session-1",
  } as SessionState;
  const wakeId = "wake-1";

  await appendCoreEventsToChatLog(context, session, wakeId, [
    {
      type: "brain_event_observed",
      sessionId: session.sessionId,
      wakeId,
      event: { type: "finished" },
    } as CoreEvent,
  ]);

  assert.equal(appended.length, 0);

  await appendCoreEventsToChatLog(context, session, wakeId, [
    {
      type: "completion_packet_delivered",
      packet: {
        sessionId: session.sessionId,
        status: "completed",
        summary: "wake completed",
      },
    } as CoreEvent,
  ]);

  assert.deepEqual(
    appended.map((event) => event.kind),
    ["assistant_message_completed", "assistant_turn_finished"],
  );
  assert.deepEqual(appended[0]?.payload, {
    status: "completed",
    summary: "wake completed",
    wake_id: wakeId,
  });
  assert.deepEqual(appended[1]?.payload, { wake_id: wakeId });
});

test("logical turn yields project as continuing rather than terminal chat events", async () => {
  const appended: Array<{ kind: string; payload: unknown }> = [];
  const context = {
    appendChatEvent: async (
      _sessionId: string,
      event: { kind: string; payload: unknown },
    ) => {
      appended.push(event);
      return event;
    },
  } as unknown as ServiceWakeDispatchContext;
  const session = { sessionId: "session-1" } as SessionState;

  await appendCoreEventsToChatLog(context, session, "dispatch-wake", [
    {
      type: "logical_turn_lifecycle_observed",
      lifecycle: {
        projectionId: "projection-1",
        logicalTurnId: "turn-1",
        sessionId: session.sessionId,
        wakeId: "source-wake",
        continuationId: "continuation-2",
        kind: "continuation_yielded",
        phase: "yielded",
        progress: {
          semanticRevision: 2,
          committedProviderOperations: 1,
          committedToolOperations: 0,
          committedProjectionCursor: 1,
          assistantContentBytes: 128,
          acceptedActionCount: 0,
          delegatedCompletionCount: 0,
          stateFingerprint: "progress-2",
          lastLivenessAt: "2026-07-29T00:00:00Z",
          lastSemanticProgressAt: "2026-07-29T00:00:00Z",
          consecutiveNoProgressSamples: 0,
        },
        reasonCode: "work_quantum_reached",
        summary: "turn will continue",
        occurredAt: "2026-07-29T00:00:00Z",
        logicalTurnRevision: 3,
      },
    },
  ]);

  assert.deepEqual(
    appended.map((event) => event.kind),
    ["logical_turn_continuing"],
  );
  assert.equal((appended[0]?.payload as { phase?: string }).phase, "yielded");
});
