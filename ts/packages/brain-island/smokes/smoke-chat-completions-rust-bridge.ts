import assert from "node:assert/strict";
import type { BrainWakeStreamItem, SessionId } from "@rusty-crew/contracts";
import { loadNativeBridge } from "@rusty-crew/native-bridge";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";

const native = await loadNativeBridge();
const started = await native.startBrainRun({
  moduleId: "chat-completions",
  providerInput: chatCompletionsWakeInput(
    "chat-completions-rust-bridge-wake",
    "primary",
  ),
});

const firstDrain = await waitForToolRequest(native, started.wakeId);
assert.equal(firstDrain.toolRequests.length, 1);
assert.equal(firstDrain.toolRequests[0]?.name, "echo_tool");
assert.equal(firstDrain.toolRequests[0]?.callId, "fake-chat-call");
const activeDiagnostics = await native.bufferedBrainRunDiagnostics();
assert.equal(activeDiagnostics.active_run_count, 1);
assert.equal(activeDiagnostics.runs[0]?.module_label, "chat-completions");
assert.equal(activeDiagnostics.runs[0]?.wake_id, started.wakeId);
assertNoBufferedPayloads(activeDiagnostics);

await native.submitBrainHostResult({
  moduleId: "chat-completions",
  wakeId: started.wakeId,
  callId: firstDrain.toolRequests[0]!.callId,
  output: "SENTINEL_CHAT_COMPLETIONS_TOOL_OUTPUT from TS bridge smoke",
  status: "succeeded",
  retryable: false,
});

const stream = [
  ...firstDrain.items,
  ...(await drainUntilTerminal(native, started.wakeId)),
];
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
      event.text.includes("chat-completions Rust bridge wake completed"),
  ),
);
assert.ok(
  events.some(
    (event) =>
      event.type === "reasoning_delta" &&
      event.text.includes("chat-completions Rust reasoning"),
  ),
);
assert.ok(events.some((event) => event.type === "finished"));
assert.equal(stream.at(-1)?.type, "actions");

const hostIsolation = await runSameWakeHostIsolationScenario();
const cleanup = await runExplicitCleanupScenario();
const singleDeniedContinuation = await runSingleDeniedContinuationScenario();
const repeatedFailureRecovery = await runRepeatedFailureRecoveryScenario();
const longContinuation = await runLongContinuationScenario();

console.log(
  JSON.stringify(
    {
      wakeId: started.wakeId,
      toolRequest: firstDrain.toolRequests[0],
      eventTypes: events.map((event) => event.type),
      terminal: stream.at(-1)?.type,
      hostIsolation,
      cleanup,
      singleDeniedContinuation,
      repeatedFailureRecovery,
      longContinuation,
    },
    null,
    2,
  ),
);

function chatCompletionsWakeInput(
  wakeId: string,
  sessionLabel: string,
  toolName = "echo_tool",
) {
  return {
    wakeId,
    sessionId:
      `chat-completions-rust-bridge-${sessionLabel}-session` as SessionId,
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
        name: toolName,
        description: "Echoes a deterministic bridge smoke result",
        inputSchema: { type: "object", properties: {} },
      },
    ],
    config: {
      model: "deepseek-flash",
      providerRequestTimeoutMs: 10_000,
      workQuantumToolRounds: 64,
    },
    client: { mode: "fake" as const },
  };
}

async function runLongContinuationScenario(): Promise<{
  submittedToolRounds: number;
  providerRequestCount: number;
  completedNormally: boolean;
}> {
  const host = await loadNativeBridge();
  const wakeId = "chat-completions-long-continuation-wake";
  await host.startBrainRun({
    moduleId: "chat-completions",
    providerInput: chatCompletionsWakeInput(
      wakeId,
      "long-continuation",
      "long_continuation_tool",
    ),
  });

  const submittedToolRounds = 12;
  for (let round = 1; round <= submittedToolRounds; round += 1) {
    const pending = await waitForToolRequest(host, wakeId);
    assert.equal(pending.toolRequests.length, 1);
    assert.equal(pending.toolRequests[0]?.name, "long_continuation_tool");
    assert.equal(
      pending.toolRequests[0]?.argumentsJson,
      JSON.stringify({ round }),
    );
    await host.submitBrainHostResult({
      moduleId: "chat-completions",
      wakeId,
      callId: pending.toolRequests[0]!.callId,
      output: `long continuation result ${round}`,
      status: "succeeded",
      retryable: false,
    });
  }

  const terminal = await drainTerminalReceipt(host, wakeId);
  const completedNormally = terminal.stream.at(-1)?.type === "actions";
  assert.equal(completedNormally, true);
  assert.equal(terminal.transportMetrics?.toolRoundCount, submittedToolRounds);
  assert.equal(
    terminal.transportMetrics?.providerRequestCount,
    submittedToolRounds + 1,
  );
  assert.equal(
    terminal.transportMetrics?.providerEventCounts["tool_call_finished"],
    submittedToolRounds,
  );
  assert.equal(
    terminal.transportMetrics?.providerEventCounts["finished"],
    submittedToolRounds + 1,
  );
  assert.equal(
    terminal.transportMetrics?.providerEventCounts["content_delta"],
    1,
  );
  assert.match(
    streamText(terminal.stream),
    /chat-completions long continuation completed/,
  );

  return {
    submittedToolRounds,
    providerRequestCount: terminal.transportMetrics?.providerRequestCount ?? 0,
    completedNormally,
  };
}

async function runSingleDeniedContinuationScenario(): Promise<{
  providerContinued: boolean;
  toolResultWasError: boolean;
}> {
  const host = await loadNativeBridge();
  const wakeId = "chat-completions-single-denied-wake";
  await host.startBrainRun({
    moduleId: "chat-completions",
    providerInput: chatCompletionsWakeInput(
      wakeId,
      "single-denied",
      "denied_tool",
    ),
  });
  const pending = await waitForToolRequest(host, wakeId);
  await host.submitBrainHostResult({
    moduleId: "chat-completions",
    wakeId,
    callId: pending.toolRequests[0]!.callId,
    output: "manual review required",
    status: "denied",
    reasonCode: "memory_manual_review_required",
    retryable: false,
    action: "denied",
  });
  const stream = await drainUntilTerminal(host, wakeId);
  const events = stream.flatMap((item) =>
    item.type === "event" ? [item.event.event] : [],
  );
  const toolResultWasError = events.some(
    (event) => event.type === "tool_call_finished" && event.isError === true,
  );
  const providerContinued =
    stream.at(-1)?.type === "actions" &&
    events.some(
      (event) =>
        event.type === "text_delta" &&
        event.text.includes("chat-completions Rust bridge wake completed"),
    );
  assert.equal(toolResultWasError, true);
  assert.equal(providerContinued, true);
  return { providerContinued, toolResultWasError };
}

async function runRepeatedFailureRecoveryScenario(): Promise<{
  submittedFailureCount: number;
  providerContinued: boolean;
  completionEmitted: boolean;
}> {
  const host = await loadNativeBridge();
  const wakeId = "chat-completions-repeated-failure-wake";
  await host.startBrainRun({
    moduleId: "chat-completions",
    providerInput: chatCompletionsWakeInput(
      wakeId,
      "repeated-failure",
      "repeat_failure_tool",
    ),
  });
  for (let index = 1; index <= 2; index += 1) {
    const pending = await waitForToolRequest(host, wakeId);
    await host.submitBrainHostResult({
      moduleId: "chat-completions",
      wakeId,
      callId: pending.toolRequests[0]!.callId,
      output: "memory client unavailable",
      status: "failed",
      reasonCode: "memory_client_unavailable",
      retryable: true,
      action: "failed",
    });
  }
  const terminal = await drainTerminalReceipt(host, wakeId);
  const completionEmitted = terminal.stream.some(
    (item) => item.type === "actions",
  );
  const providerContinued = streamText(terminal.stream).includes(
    "recovered after repeated tool failure guidance",
  );
  assert.equal(providerContinued, true);
  assert.equal(completionEmitted, true);
  return {
    submittedFailureCount: 2,
    providerContinued,
    completionEmitted,
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
  const sharedWakeId = "chat-completions-shared-host-wake";

  const firstStarted = await firstHost.startBrainRun({
    moduleId: "chat-completions",
    providerInput: chatCompletionsWakeInput(sharedWakeId, "host-one"),
  });
  const secondStarted = await secondHost.startBrainRun({
    moduleId: "chat-completions",
    providerInput: chatCompletionsWakeInput(sharedWakeId, "host-two"),
  });
  assert.equal(firstStarted.wakeId, sharedWakeId);
  assert.equal(secondStarted.wakeId, sharedWakeId);

  const firstDrain = await waitForToolRequest(firstHost, sharedWakeId);
  const secondDrain = await waitForToolRequest(secondHost, sharedWakeId);
  assert.equal(firstDrain.toolRequests[0]?.callId, "fake-chat-call");
  assert.equal(secondDrain.toolRequests[0]?.callId, "fake-chat-call");

  await firstHost.submitBrainHostResult({
    moduleId: "chat-completions",
    wakeId: sharedWakeId,
    callId: firstDrain.toolRequests[0]!.callId,
    output: "FIRST_HOST_OUTPUT_ONLY",
    status: "succeeded",
    retryable: false,
  });
  const firstStream = await drainUntilTerminal(firstHost, sharedWakeId);

  const secondAfterFirstOutput = await secondHost.drainBrainRun({
    moduleId: "chat-completions",
    wakeId: sharedWakeId,
    maxItems: 32,
  });
  assert.equal(secondAfterFirstOutput.terminal, false);
  assert.equal(secondAfterFirstOutput.items.length, 0);
  assert.equal(secondAfterFirstOutput.toolRequests.length, 0);

  const cancellation = await secondHost.cancelBrainRun({
    moduleId: "chat-completions",
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
    () =>
      firstHost.drainBrainRun({
        moduleId: "chat-completions",
        wakeId: sharedWakeId,
      }),
    /chat-completions buffered wake chat-completions-shared-host-wake was not found/,
  );
  await assert.rejects(
    () =>
      secondHost.drainBrainRun({
        moduleId: "chat-completions",
        wakeId: sharedWakeId,
      }),
    /chat-completions buffered wake chat-completions-shared-host-wake was not found/,
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

async function runExplicitCleanupScenario(): Promise<{
  activeBeforeCleanup: number;
  cancelledNonterminalRuns: number;
  removedRuns: number;
  activeAfterCleanup: number;
}> {
  const cleanupHost = await loadNativeBridge();
  const cleanupWakeId = "chat-completions-cleanup-host-wake";
  await cleanupHost.startBrainRun({
    moduleId: "chat-completions",
    providerInput: chatCompletionsWakeInput(cleanupWakeId, "cleanup-host"),
  });
  await waitForToolRequest(cleanupHost, cleanupWakeId);
  const beforeCleanup = await cleanupHost.bufferedBrainRunDiagnostics();
  assert.equal(beforeCleanup.active_run_count, 1);
  assert.equal(beforeCleanup.runs[0]?.pending_tool_request_count, 1);
  assertNoBufferedPayloads(beforeCleanup);

  const cleanup = await cleanupHost.cleanupBufferedBrainRuns({
    reasonCode: "smoke_cleanup",
    summary: "chat-completions bridge smoke cleanup",
  });
  assert.equal(cleanup.active_runs, 1);
  assert.equal(cleanup.cancelled_nonterminal_runs, 1);
  assert.equal(cleanup.removed_runs, 1);

  const afterCleanup = await cleanupHost.bufferedBrainRunDiagnostics();
  assert.equal(afterCleanup.active_run_count, 0);
  await assert.rejects(
    () =>
      cleanupHost.drainBrainRun({
        moduleId: "chat-completions",
        wakeId: cleanupWakeId,
      }),
    /chat-completions buffered wake chat-completions-cleanup-host-wake was not found/,
  );

  return {
    activeBeforeCleanup: beforeCleanup.active_run_count,
    cancelledNonterminalRuns: cleanup.cancelled_nonterminal_runs,
    removedRuns: cleanup.removed_runs,
    activeAfterCleanup: afterCleanup.active_run_count,
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

function assertNoBufferedPayloads(value: unknown): void {
  const serialized = JSON.stringify(value);
  assert.ok(!serialized.includes("argumentsJson"));
  assert.ok(!serialized.includes("arguments_json"));
  assert.ok(!serialized.includes("SENTINEL_CHAT_COMPLETIONS_TOOL_OUTPUT"));
  assert.ok(!serialized.includes("FIRST_HOST_OUTPUT_ONLY"));
}

async function waitForToolRequest(
  nativeBridge: NativeBridgeModule,
  wakeId: string,
): Promise<{
  items: BrainWakeStreamItem[];
  toolRequests: Awaited<
    ReturnType<NativeBridgeModule["drainBrainRun"]>
  >["toolRequests"];
}> {
  const items: BrainWakeStreamItem[] = [];
  for (let attempt = 0; attempt < 80; attempt += 1) {
    const drained = await nativeBridge.drainBrainRun({
      moduleId: "chat-completions",
      wakeId,
      maxItems: 32,
    });
    items.push(...drained.items);
    if (drained.toolRequests.length > 0) {
      return { items, toolRequests: drained.toolRequests };
    }
    await delay(25);
  }
  throw new Error("timed out waiting for chat-completions tool request");
}

async function drainUntilTerminal(
  nativeBridge: NativeBridgeModule,
  wakeId: string,
  options: { allowTerminalError?: boolean } = {},
): Promise<BrainWakeStreamItem[]> {
  return (await drainTerminalReceipt(nativeBridge, wakeId, options)).stream;
}

async function drainTerminalReceipt(
  nativeBridge: NativeBridgeModule,
  wakeId: string,
  options: { allowTerminalError?: boolean } = {},
): Promise<{
  stream: BrainWakeStreamItem[];
  error?: string;
  transportMetrics?: Awaited<
    ReturnType<NativeBridgeModule["drainBrainRun"]>
  >["transportMetrics"];
}> {
  const stream: BrainWakeStreamItem[] = [];
  for (let attempt = 0; attempt < 80; attempt += 1) {
    const drained = await nativeBridge.drainBrainRun({
      moduleId: "chat-completions",
      wakeId,
      maxItems: 32,
    });
    stream.push(...drained.items);
    if (drained.error && !(options.allowTerminalError && drained.terminal)) {
      throw new Error(drained.error);
    }
    if (drained.terminal) {
      return {
        stream,
        ...(drained.error === undefined ? {} : { error: drained.error }),
        ...(drained.transportMetrics === undefined
          ? {}
          : { transportMetrics: drained.transportMetrics }),
      };
    }
    await delay(25);
  }
  throw new Error("timed out waiting for chat-completions terminal stream");
}

async function delay(delayMs: number): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, delayMs));
}
