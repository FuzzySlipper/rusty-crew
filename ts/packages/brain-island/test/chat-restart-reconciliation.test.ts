import assert from "node:assert/strict";
import test from "node:test";
import type { SessionId } from "@rusty-crew/contracts";
import type { ChatEvent } from "../src/rusty-view-chat-api.js";
import { interruptedTurnRepair } from "../src/service-chat-restart-reconciliation.js";

const sessionId = "session-restart" as SessionId;

test("repairs a user message interrupted before the brain emitted started", () => {
  const repair = interruptedTurnRepair(
    [event(7, "message_created", { role: "user", body: "hello" })],
    sessionId,
  );

  assert.equal(repair?.wakeId, "restart-interrupted:session-restart:7");
  assert.deepEqual(
    repair?.events.map((item) => item.kind),
    ["assistant_message_completed", "assistant_turn_finished"],
  );
  assert.equal(
    repair?.events[0]?.payload.reason_code,
    "service_restart_interrupted",
  );
});

test("repairs a streamed wake with its durable wake id", () => {
  const repair = interruptedTurnRepair(
    [
      event(1, "message_created", { role: "user" }),
      event(2, "assistant_turn_started", { wake_id: "wake-7" }),
      event(3, "tool_call_started", {
        wake_id: "wake-7",
        tool_name: "read_file",
      }),
    ],
    sessionId,
  );

  assert.equal(repair?.wakeId, "wake-7");
  assert.equal(repair?.events.length, 2);
});

test("only appends the missing half of a terminal pair", () => {
  const repair = interruptedTurnRepair(
    [
      event(1, "assistant_turn_started", { wake_id: "wake-8" }),
      event(2, "assistant_message_completed", {
        wake_id: "wake-8",
        status: "failed",
      }),
    ],
    sessionId,
  );

  assert.deepEqual(
    repair?.events.map((item) => item.kind),
    ["assistant_turn_finished"],
  );
});

test("leaves a completed turn unchanged", () => {
  const repair = interruptedTurnRepair(
    [
      event(1, "message_created", { role: "user" }),
      event(2, "assistant_turn_started", { wake_id: "wake-9" }),
      event(3, "assistant_message_completed", {
        wake_id: "wake-9",
        status: "completed",
      }),
      event(4, "assistant_turn_finished", { wake_id: "wake-9" }),
      event(5, "unknown", { source_event_type: "brain_actions_accepted" }),
    ],
    sessionId,
  );

  assert.equal(repair, undefined);
});

function event(
  sequence: number,
  kind: ChatEvent["kind"],
  payload: Record<string, unknown>,
): ChatEvent {
  return {
    event_id: `${sessionId}:${sequence}`,
    session_id: sessionId,
    sequence_id: sequence,
    created_at: "2026-07-22T00:00:00.000Z",
    kind,
    payload,
  };
}
