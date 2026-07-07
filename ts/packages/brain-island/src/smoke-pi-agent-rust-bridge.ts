import assert from "node:assert/strict";
import type { BrainWakeStreamItem, SessionId } from "@rusty-crew/contracts";
import { loadNativeBridge } from "@rusty-crew/native-bridge";

const native = await loadNativeBridge();
const started = await native.startPiAgentBrain({
  wakeId: "pi-agent-rust-bridge-wake",
  sessionId: "pi-agent-rust-bridge-session" as SessionId,
  messages: [
    {
      role: "system",
      content: "You are a deterministic bridge smoke test brain.",
    },
    {
      role: "user",
      content: "Call the echo tool, then finish.",
    },
  ],
  tools: [
    {
      name: "echo_tool",
      description: "Echoes a deterministic bridge smoke result",
      inputSchema: { type: "object", properties: {} },
    },
  ],
  config: {
    model: "deepseek-flash",
    wakeTimeoutMs: 10_000,
    streamIdleTimeoutMs: 10_000,
  },
  client: { mode: "fake" },
});

const firstDrain = await waitForToolRequest(started.wakeId);
assert.equal(firstDrain.toolRequests.length, 1);
assert.equal(firstDrain.toolRequests[0]?.name, "echo_tool");
assert.equal(firstDrain.toolRequests[0]?.callId, "fake-pi-call");

await native.submitPiAgentToolOutput({
  wakeId: started.wakeId,
  callId: firstDrain.toolRequests[0]!.callId,
  output: "SENTINEL_PI_AGENT_TOOL_OUTPUT from TS bridge smoke",
  isError: false,
});

const stream = await drainUntilTerminal(started.wakeId);
const events = stream.flatMap((item) =>
  item.type === "event" ? [item.event.event] : [],
);
assert.ok(events.some((event) => event.type === "started"));
assert.ok(
  events.some(
    (event) =>
      event.type === "tool_call_started" && event.toolName === "echo_tool",
  ),
);
assert.ok(
  events.some(
    (event) =>
      event.type === "tool_call_finished" &&
      event.toolName === "echo_tool" &&
      event.isError === false,
  ),
);
assert.ok(
  events.some(
    (event) =>
      event.type === "text_delta" &&
      event.text.includes("pi-agent Rust bridge wake completed"),
  ),
);
assert.ok(events.some((event) => event.type === "finished"));
assert.equal(stream.at(-1)?.type, "actions");

console.log(
  JSON.stringify(
    {
      wakeId: started.wakeId,
      toolRequest: firstDrain.toolRequests[0],
      eventTypes: events.map((event) => event.type),
      terminal: stream.at(-1)?.type,
    },
    null,
    2,
  ),
);

async function waitForToolRequest(wakeId: string): Promise<{
  toolRequests: Awaited<
    ReturnType<typeof native.drainPiAgentBrainStream>
  >["toolRequests"];
}> {
  for (let attempt = 0; attempt < 80; attempt += 1) {
    const drained = await native.drainPiAgentBrainStream({
      wakeId,
      maxItems: 32,
    });
    if (drained.toolRequests.length > 0) {
      return { toolRequests: drained.toolRequests };
    }
    await delay(25);
  }
  throw new Error("timed out waiting for pi-agent tool request");
}

async function drainUntilTerminal(
  wakeId: string,
): Promise<BrainWakeStreamItem[]> {
  const stream: BrainWakeStreamItem[] = [];
  for (let attempt = 0; attempt < 80; attempt += 1) {
    const drained = await native.drainPiAgentBrainStream({
      wakeId,
      maxItems: 32,
    });
    stream.push(...drained.items);
    if (drained.error) {
      throw new Error(drained.error);
    }
    if (drained.terminal) {
      return stream;
    }
    await delay(25);
  }
  throw new Error("timed out waiting for pi-agent terminal stream");
}

async function delay(delayMs: number): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, delayMs));
}
