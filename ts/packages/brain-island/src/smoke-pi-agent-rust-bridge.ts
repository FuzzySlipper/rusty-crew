import assert from "node:assert/strict";
import type { BrainWakeStreamItem, SessionId } from "@rusty-crew/contracts";
import { loadNativeBridge } from "@rusty-crew/native-bridge";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";

const native = await loadNativeBridge();
const started = await native.startPiAgentBrain(
  piAgentWakeInput("pi-agent-rust-bridge-wake", "primary"),
);

const firstDrain = await waitForToolRequest(native, started.wakeId);
assert.equal(firstDrain.toolRequests.length, 1);
assert.equal(firstDrain.toolRequests[0]?.name, "echo_tool");
assert.equal(firstDrain.toolRequests[0]?.callId, "fake-pi-call");

await native.submitPiAgentToolOutput({
  wakeId: started.wakeId,
  callId: firstDrain.toolRequests[0]!.callId,
  output: "SENTINEL_PI_AGENT_TOOL_OUTPUT from TS bridge smoke",
  isError: false,
});

const stream = await drainUntilTerminal(native, started.wakeId);
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

const hostIsolation = await runSameWakeHostIsolationScenario();

console.log(
  JSON.stringify(
    {
      wakeId: started.wakeId,
      toolRequest: firstDrain.toolRequests[0],
      eventTypes: events.map((event) => event.type),
      terminal: stream.at(-1)?.type,
      hostIsolation,
    },
    null,
    2,
  ),
);

function piAgentWakeInput(wakeId: string, sessionLabel: string) {
  return {
    wakeId,
    sessionId: `pi-agent-rust-bridge-${sessionLabel}-session` as SessionId,
    messages: [
      {
        role: "system" as const,
        content: "You are a deterministic bridge smoke test brain.",
      },
      {
        role: "user" as const,
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
    client: { mode: "fake" as const },
  };
}

async function runSameWakeHostIsolationScenario(): Promise<{
  sharedWakeId: string;
  firstHostTerminal: boolean;
  secondHostUnaffectedByFirstOutput: boolean;
  secondHostCancelled: boolean;
  terminalCleanupConfirmed: boolean;
}> {
  const firstHost = await loadNativeBridge();
  const secondHost = await loadNativeBridge();
  const sharedWakeId = "pi-agent-shared-host-wake";

  const firstStarted = await firstHost.startPiAgentBrain(
    piAgentWakeInput(sharedWakeId, "host-one"),
  );
  const secondStarted = await secondHost.startPiAgentBrain(
    piAgentWakeInput(sharedWakeId, "host-two"),
  );
  assert.equal(firstStarted.wakeId, sharedWakeId);
  assert.equal(secondStarted.wakeId, sharedWakeId);

  const firstDrain = await waitForToolRequest(firstHost, sharedWakeId);
  const secondDrain = await waitForToolRequest(secondHost, sharedWakeId);
  assert.equal(firstDrain.toolRequests[0]?.callId, "fake-pi-call");
  assert.equal(secondDrain.toolRequests[0]?.callId, "fake-pi-call");

  await firstHost.submitPiAgentToolOutput({
    wakeId: sharedWakeId,
    callId: firstDrain.toolRequests[0]!.callId,
    output: "FIRST_HOST_OUTPUT_ONLY",
    isError: false,
  });
  const firstStream = await drainUntilTerminal(firstHost, sharedWakeId);

  const secondAfterFirstOutput = await secondHost.drainPiAgentBrainStream({
    wakeId: sharedWakeId,
    maxItems: 32,
  });
  assert.equal(secondAfterFirstOutput.terminal, false);
  assert.equal(secondAfterFirstOutput.items.length, 0);
  assert.equal(secondAfterFirstOutput.toolRequests.length, 0);

  const cancellation = await secondHost.cancelPiAgentBrain({
    wakeId: sharedWakeId,
    reasonCode: "host_isolation_smoke",
    summary: "second host cleanup after same-wake isolation smoke",
  });
  const secondStream = await drainUntilTerminal(secondHost, sharedWakeId, {
    allowTerminalError: true,
  });
  const secondEvents = streamText(secondStream);
  assert.equal(cancellation.cancellation?.reasonCode, "host_isolation_smoke");
  assert.ok(
    secondEvents.includes("host_isolation_smoke") ||
      cancellation.cancellation?.summary.includes("second host cleanup"),
  );

  await assert.rejects(
    () => firstHost.drainPiAgentBrainStream({ wakeId: sharedWakeId }),
    /pi-agent buffered wake pi-agent-shared-host-wake was not found/,
  );
  await assert.rejects(
    () => secondHost.drainPiAgentBrainStream({ wakeId: sharedWakeId }),
    /pi-agent buffered wake pi-agent-shared-host-wake was not found/,
  );

  return {
    sharedWakeId,
    firstHostTerminal: firstStream.at(-1)?.type === "actions",
    secondHostUnaffectedByFirstOutput:
      secondAfterFirstOutput.terminal === false &&
      secondAfterFirstOutput.items.length === 0 &&
      secondAfterFirstOutput.toolRequests.length === 0,
    secondHostCancelled:
      cancellation.cancellation?.reasonCode === "host_isolation_smoke",
    terminalCleanupConfirmed: true,
  };
}

function streamText(stream: BrainWakeStreamItem[]): string {
  return stream
    .flatMap((item) =>
      item.type === "event" && item.event.event.type === "text_delta"
        ? [item.event.event.text]
        : [],
    )
    .join("");
}

async function waitForToolRequest(
  nativeBridge: NativeBridgeModule,
  wakeId: string,
): Promise<{
  toolRequests: Awaited<
    ReturnType<NativeBridgeModule["drainPiAgentBrainStream"]>
  >["toolRequests"];
}> {
  for (let attempt = 0; attempt < 80; attempt += 1) {
    const drained = await nativeBridge.drainPiAgentBrainStream({
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
  nativeBridge: NativeBridgeModule,
  wakeId: string,
  options: { allowTerminalError?: boolean } = {},
): Promise<BrainWakeStreamItem[]> {
  const stream: BrainWakeStreamItem[] = [];
  for (let attempt = 0; attempt < 80; attempt += 1) {
    const drained = await nativeBridge.drainPiAgentBrainStream({
      wakeId,
      maxItems: 32,
    });
    stream.push(...drained.items);
    if (drained.error && !(options.allowTerminalError && drained.terminal)) {
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
