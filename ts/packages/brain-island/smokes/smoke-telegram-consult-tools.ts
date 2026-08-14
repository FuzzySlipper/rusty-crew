import assert from "node:assert/strict";
import { requestTelegramConsultTool } from "../src/telegram-consult-tools.js";
import { defaultToolRegistry } from "../src/tool-registry.js";

const calls: unknown[] = [];
const tool = requestTelegramConsultTool({
  async request(input) {
    calls.push(input);
    return {
      ok: true,
      consultId: "telegram-consult-1",
      status: "sent",
      bindingId: "diplomat-binding",
      externalMessageIds: ["123"],
      summary: "Telegram consult sent to the bound remote operator.",
    };
  },
});

assert.ok(tool.executeWithContext);
const result = await tool.executeWithContext!(
  {
    message: "The network state is ambiguous. Should I inspect the router?",
    category: "network_trouble",
  },
  {
    wake: {
      state: {
        session: { agentId: "diplomat" },
        pendingMessages: [{ inputKind: "operator" }],
      },
    } as never,
    wakeId: "wake-1",
    sessionId: "diplomat-session",
    callId: "call-1",
    signal: new AbortController().signal,
  },
);
assert.equal(result.details.status, "sent");
assert.deepEqual(calls, [
  {
    message: "The network state is ambiguous. Should I inspect the router?",
    category: "network_trouble",
    caller: {
      type: "direct_brain",
      sessionId: "diplomat-session",
      wakeId: "wake-1",
      toolCallId: "call-1",
    },
    originatingWakeKind: "operator",
  },
]);

const ordinary = defaultToolRegistry.buildInventory({
  requestedToolsets: ["full_agent"],
});
assert.equal(
  ordinary.selectedTools.some(
    (candidate) => candidate.name === "request_telegram_consult",
  ),
  false,
);
const diplomat = defaultToolRegistry.buildInventory({
  requestedToolsets: ["telegram_diplomat"],
});
assert.deepEqual(
  diplomat.selectedTools.map((candidate) => candidate.name),
  ["request_telegram_consult"],
);

console.log("Telegram consult tool smoke passed");
