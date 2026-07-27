import assert from "node:assert/strict";
import test from "node:test";

import { mapNotification } from "../src/event-mapper.js";

for (const [itemType, expectedKind] of [
  ["commandExecution", "command_activity"],
  ["fileChange", "file_activity"],
  ["mcpToolCall", "mcp_activity"],
  ["dynamicToolCall", "dynamic_tool_activity"],
  ["contextCompaction", "compaction"],
] as const) {
  test(`generic item lifecycle projects ${itemType} as ${expectedKind}`, () => {
    const event = mapNotification(
      {
        method: "item/completed",
        params: {
          threadId: "thread-1",
          turnId: "turn-1",
          item: { id: "item-1", type: itemType },
        },
      },
      1,
      16_384,
      true,
    );
    assert.equal(event.kind, expectedKind);
    assert.equal(event.itemId, "item-1");
    assert.equal(event.payload.nativeMethod, "item/completed");
    assert.equal("threadId" in event.payload, false);
    assert.equal("turnId" in event.payload, false);
    assert.equal("item" in event.payload, false);
  });
}

test("error notifications preserve bounded durable diagnostics", () => {
  const event = mapNotification(
    {
      method: "error",
      params: {
        threadId: "thread-1",
        turnId: "turn-1",
        error: {
          message: "response stream disconnected",
          codexErrorInfo: {
            responseStreamDisconnected: { httpStatusCode: 502 },
          },
          additionalDetails: "upstream closed before final answer",
        },
        willRetry: false,
      },
    },
    9,
    16_384,
    true,
  );

  assert.equal(event.kind, "runtime_warning");
  assert.equal(event.payload.message, "response stream disconnected");
  assert.deepEqual(event.payload.error, {
    message: "response stream disconnected",
    code: "responseStreamDisconnected",
    additionalDetails: "upstream closed before final answer",
    willRetry: false,
  });
});

test("failed turn completion preserves its embedded error", () => {
  const event = mapNotification(
    {
      method: "turn/completed",
      params: {
        threadId: "thread-1",
        turn: {
          id: "turn-1",
          status: "failed",
          error: {
            message: "provider rejected the request",
            codexErrorInfo: "badRequest",
            additionalDetails: null,
          },
        },
      },
    },
    10,
    16_384,
    true,
  );

  assert.equal(event.kind, "turn_lifecycle");
  assert.equal(event.payload.status, "failed");
  assert.deepEqual(event.payload.error, {
    message: "provider rejected the request",
    code: "badRequest",
    additionalDetails: null,
    willRetry: false,
  });
});
