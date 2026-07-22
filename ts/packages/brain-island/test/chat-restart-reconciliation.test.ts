import assert from "node:assert/strict";
import test from "node:test";
import type { SessionId } from "@rusty-crew/contracts";
import type { ChatEvent } from "../src/rusty-view-chat-api.js";
import {
  interruptedTurnRepair,
  reconcileInterruptedChatTurns,
} from "../src/service-chat-restart-reconciliation.js";

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

test("restart reconciliation ignores pending-message read-model fallback", async () => {
  const appended: Array<{ kind: string; payload: Record<string, unknown> }> =
    [];
  const bridge = reconciliationBridge(
    "pending_messages",
    [
      event(1, "message_created", {
        role: "user",
        source: "pending_body_state",
      }),
    ],
    appended,
  );

  const report = await reconcileInterruptedChatTurns({
    bridge,
    now: () => "2026-07-22T00:00:01.000Z",
  });

  assert.equal(report.sessionsScanned, 1);
  assert.deepEqual(report.sessionsReconciled, []);
  assert.equal(report.eventsAppended, 0);
  assert.deepEqual(appended, []);
});

test("restart reconciliation repairs only authoritative event-log turns", async () => {
  const appended: Array<{ kind: string; payload: Record<string, unknown> }> =
    [];
  const bridge = reconciliationBridge(
    "event_log",
    [
      event(1, "message_created", { role: "user" }),
      event(2, "assistant_turn_started", { wake_id: "wake-event-log" }),
      event(3, "assistant_message_completed", {
        wake_id: "wake-event-log",
        status: "failed",
      }),
    ],
    appended,
  );

  const report = await reconcileInterruptedChatTurns({
    bridge,
    now: () => "2026-07-22T00:00:01.000Z",
  });

  assert.deepEqual(report.sessionsReconciled, [sessionId]);
  assert.equal(report.eventsAppended, 1);
  assert.equal(appended[0]?.kind, "assistant_turn_finished");
  assert.equal(appended[0]?.payload.wake_id, "wake-event-log");
});

function reconciliationBridge(
  source: "event_log" | "pending_messages",
  events: ChatEvent[],
  appended: Array<{ kind: string; payload: Record<string, unknown> }>,
): Parameters<typeof reconcileInterruptedChatTurns>[0]["bridge"] {
  return {
    async queryChatSessionSummaries() {
      return {
        page: {
          items: [
            {
              session: { sessionId, status: "idle" },
              message_count: events.length,
              latest_cursor: `${sessionId}:${events.length}`,
              source,
            },
          ],
          total: 1,
          limit: 500,
          offset: 0,
          next_offset: null,
        },
      } as never;
    },
    async readChatSession() {
      return {
        session: { sessionId, status: "idle" },
        events,
        latest_cursor: `${sessionId}:${events.length}`,
        has_more: false,
        has_more_before: false,
        total: events.length,
        message_count: events.length,
        source,
        message_slots: {
          items: [],
          total: 0,
          limit: 1_000,
          offset: 0,
          next_offset: null,
        },
      } as never;
    },
    async appendChatEvent(input) {
      const event = input as {
        kind: string;
        payload: Record<string, unknown>;
      };
      appended.push({ kind: event.kind, payload: event.payload });
      return {} as never;
    },
  };
}

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
