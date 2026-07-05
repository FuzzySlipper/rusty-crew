import assert from "node:assert/strict";
import test from "node:test";

import { buildChatWakeFailureSummaryFromEvents } from "../src/chat-wake-failure-summary.js";
import type { ChatEvent } from "../src/rusty-view-chat-api.js";

test("chat wake failure summary reports partial text, tool state, and unsuccessful completed tools", () => {
  const events: ChatEvent[] = [
    chatEvent("assistant_text_delta", { text: "Half" }, 1),
    chatEvent("assistant_text_delta", { text: "Half done" }, 2),
    chatEvent(
      "tool_call_started",
      { tool_call_id: "tool-1", tool_name: "memory_read" },
      3,
    ),
    chatEvent(
      "tool_call_completed",
      {
        tool_call_id: "tool-1",
        tool_name: "memory_read",
        debug_detail_id: "debug-1",
      },
      4,
    ),
    chatEvent(
      "tool_call_started",
      { tool_call_id: "tool-2", tool_name: "den_docs_read" },
      5,
    ),
    chatEvent(
      "tool_call_failed",
      { tool_call_id: "tool-3", tool_name: "web_fetch" },
      6,
    ),
    chatEvent("assistant_reasoning_delta", { text: "checking" }, 7),
    chatEvent(
      "provider_status",
      { level: "warning", message: "provider stream stalled" },
      8,
    ),
  ];

  const summary = buildChatWakeFailureSummaryFromEvents({
    failureSummary: "provider stream idle timeout",
    events,
    sessionId: "session-a",
    toolDebugLookup: {
      get: ({ debugDetailId }) =>
        debugDetailId === "debug-1"
          ? {
              tool_name: "memory_read",
              final_result: {
                value: {
                  details: {
                    ok: false,
                    reasonCode: "memory_client_unavailable",
                  },
                },
              },
            }
          : undefined,
    },
  });

  assert.match(
    summary,
    /Assistant turn failed before it could finish: provider stream idle timeout/,
  );
  assert.match(summary, /Partial response before failure: Half done/);
  assert.match(summary, /Failed tool calls: web_fetch\./);
  assert.match(
    summary,
    /Tool calls reporting unsuccessful results: memory_read \(memory_client_unavailable\)\./,
  );
  assert.match(summary, /Completed tool calls before failure: 1\./);
  assert.match(summary, /Tool calls still in flight: den_docs_read\./);
  assert.match(summary, /Reasoning updates before failure: 1\./);
  assert.match(
    summary,
    /Recent provider status: warning: provider stream stalled\./,
  );
});

test("chat wake failure summary returns the base failure when no matching events exist", () => {
  assert.equal(
    buildChatWakeFailureSummaryFromEvents({
      failureSummary: "  ",
      events: [],
      sessionId: "session-a",
      toolDebugLookup: { get: () => undefined },
    }),
    "assistant turn failed",
  );
});

function chatEvent(
  kind: ChatEvent["kind"],
  payload: Record<string, unknown>,
  sequenceId: number,
): ChatEvent {
  return {
    event_id: `event-${sequenceId}`,
    session_id: "session-a",
    sequence_id: sequenceId,
    created_at: "2026-07-05T00:00:00Z",
    kind,
    payload,
  };
}
