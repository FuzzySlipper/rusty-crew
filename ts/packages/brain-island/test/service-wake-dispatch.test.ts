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
