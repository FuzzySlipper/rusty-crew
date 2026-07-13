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
