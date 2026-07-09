import assert from "node:assert/strict";
import type {
  AgentId,
  BrainEventEnvelope,
  BrainWakeStreamItem,
  ProfileId,
  SessionHandle,
  SessionId,
} from "@rusty-crew/contracts";
import {
  loadNativeBridge,
  type NativeBridgeModule,
} from "@rusty-crew/native-bridge";
import { Type } from "typebox";
import type { BrainTool } from "../src/brain-tool.js";
import { openAiResponsesBrainModule } from "../src/brain-module.js";
import type { BrainWakeInput } from "../src/index.js";
import type { LoadedProfileContext } from "../src/profile-loading.js";
import { MemoryToolCallDebugStore } from "../src/tool-call-debug-store.js";

const previousLiveMode = process.env.RUSTY_CREW_OPENAI_RESPONSES_LIVE;
process.env.RUSTY_CREW_OPENAI_RESPONSES_LIVE = "0";

try {
  const native = await loadNativeBridge();
  const observedEvents: BrainEventEnvelope[] = [];
  const toolProfile = {
    tools: [
      {
        name: "sentinel_tool",
        description: "Return a sentinel proving TS tool execution ran",
      },
    ],
  };
  const sentinelParameters = Type.Object({});
  const sentinelTool: BrainTool<typeof sentinelParameters> = {
    name: "sentinel_tool",
    label: "Sentinel",
    description: "Return a sentinel proving TS tool execution ran",
    parameters: sentinelParameters,
    execute: async () => ({
      content: [
        {
          type: "text",
          text: "SENTINEL_REAL_TOOL_OUTPUT from TypeScript BrainTool",
        },
      ],
      details: { bridge: "ts-real-tool" },
    }),
  };
  const bridge = {
    runOpenAiResponsesBrain: async () => {
      throw new Error("blocking Responses runner should not be used");
    },
    startOpenAiResponsesBrain: async (
      input: Parameters<NativeBridgeModule["startOpenAiResponsesBrain"]>[0],
    ) => {
      capturedInstructions = input.config.instructions;
      capturedWakeTimeoutMs = input.config.wakeTimeoutMs;
      return native.startOpenAiResponsesBrain(input);
    },
    drainOpenAiResponsesBrainStream:
      native.drainOpenAiResponsesBrainStream.bind(native),
    submitOpenAiResponsesToolOutput:
      native.submitOpenAiResponsesToolOutput.bind(native),
    submitBrainEvent: async (event: BrainEventEnvelope) => {
      observedEvents.push(event);
      return { accepted: true, sequence: observedEvents.length };
    },
  } as unknown as NativeBridgeModule;
  const toolCallDebugStore = new MemoryToolCallDebugStore({
    now: () => "2026-07-04T00:00:00.000Z",
  });
  let capturedInstructions: string | undefined;
  let capturedWakeTimeoutMs: number | undefined;

  const brain = await openAiResponsesBrainModule.createBrain({
    bridge,
    profile: {
      profile: {
        profileId: "responses-tool-bridge-profile",
        modelConfig: {
          provider: "openai",
          modelName: "gpt-5",
          api: "responses",
        },
        brain: {
          module: "openai-responses",
          strategy: "replay",
        },
        runtime: {
          maxTurnDurationMs: 45_000,
        },
      },
      skills: [],
      toolSelection: {
        source: "smoke",
        toolProfile,
      },
    } as unknown as LoadedProfileContext,
    runtimeConfig: {
      sessions: [
        {
          sessionId: "responses-tool-bridge-session",
          turnTimeoutMs: 12_000,
        },
      ],
    } as never,
    toolResolver: () => [sentinelTool],
    providerStateScope: {
      profileFingerprint: "profile-smoke",
      providerFingerprint: "provider-smoke",
    },
    toolCallDebugStore,
  });
  const result = await brain.wake(wakeInput(toolProfile));
  const providerStateText = JSON.stringify(result.providerState);

  assert.match(capturedInstructions ?? "", /System instruction marker/);
  assert.match(capturedInstructions ?? "", /Role inventory marker/);
  assert.match(capturedInstructions ?? "", /den_get_document/);
  assert.equal(capturedWakeTimeoutMs, 12_000);
  assert.match(providerStateText, /SENTINEL_REAL_TOOL_OUTPUT/);
  assert.doesNotMatch(providerStateText, /completed by Rust Responses bridge/);
  assert.doesNotMatch(providerStateText, /deterministic field scaffold/);
  const started = observedEvents.find(
    (event) =>
      event.event.type === "tool_call_started" &&
      event.event.toolName === "sentinel_tool",
  );
  assert.ok(started, "expected tool_call_started event for sentinel_tool");
  assert.ok(
    started.event.type === "tool_call_started",
    "expected tool_call_started event shape",
  );
  const debugDetailId = started.event.metadata?.debugDetailId;
  assert.ok(
    debugDetailId?.startsWith("tooldbg_"),
    "expected tool_call_started debug detail reference",
  );
  if (typeof debugDetailId !== "string") {
    throw new Error("expected string debug detail id");
  }
  const finished = observedEvents.find(
    (event) =>
      event.event.type === "tool_call_finished" &&
      event.event.toolName === "sentinel_tool" &&
      event.event.isError === false,
  );
  assert.ok(
    finished,
    "expected successful tool_call_finished event for sentinel_tool",
  );
  assert.ok(
    finished.event.type === "tool_call_finished",
    "expected tool_call_finished event shape",
  );
  assert.equal(
    finished.event.metadata?.debugDetailId,
    debugDetailId,
    "expected tool_call_finished to reference the same debug detail",
  );
  const debugRecord = toolCallDebugStore.get({
    sessionId: "responses-tool-bridge-session",
    debugDetailId,
  });
  assert.equal(debugRecord?.status, "completed");
  assert.deepEqual(debugRecord?.arguments.value, {
    rawArguments: {},
    preparedArguments: {},
  });
  assert.deepEqual(debugRecord?.final_result?.value, {
    content: [
      {
        type: "text",
        text: "SENTINEL_REAL_TOOL_OUTPUT from TypeScript BrainTool",
      },
    ],
    details: { bridge: "ts-real-tool" },
  });
  assert.ok(
    result.actions.some((action) => action.type === "deliver_completion"),
    "expected completion action from fake Responses provider",
  );
  const repeatedFailurePolicy = await runRepeatedFailurePolicyScenario();
  const singleDeniedContinuation = await runSingleDeniedContinuationScenario();

  console.log(
    JSON.stringify(
      {
        observedEventTypes: observedEvents.map((event) => event.event.type),
        actionTypes: result.actions.map((action) => action.type),
        streamItemCounts: result.brainStreamItemCounts,
        providerStateContainsRealToolOutput: providerStateText.includes(
          "SENTINEL_REAL_TOOL_OUTPUT",
        ),
        repeatedFailurePolicy,
        singleDeniedContinuation,
      },
      null,
      2,
    ),
  );
} finally {
  if (previousLiveMode === undefined) {
    delete process.env.RUSTY_CREW_OPENAI_RESPONSES_LIVE;
  } else {
    process.env.RUSTY_CREW_OPENAI_RESPONSES_LIVE = previousLiveMode;
  }
}

async function runRepeatedFailurePolicyScenario(): Promise<{
  submittedOutputs: number;
  providerStatusReported: boolean;
  debugRecordsCompleted: number;
}> {
  const observedEvents: BrainEventEnvelope[] = [];
  const submittedOutputs: Array<{
    wakeId: string;
    callId: string;
    output: string;
    isError: boolean;
  }> = [];
  const toolCallDebugStore = new MemoryToolCallDebugStore({
    now: () => "2026-07-04T00:00:00.000Z",
  });
  let drainCount = 0;
  const bridge = {
    runOpenAiResponsesBrain: async () => {
      throw new Error("blocking Responses runner should not be used");
    },
    startOpenAiResponsesBrain: async (
      input: Parameters<NativeBridgeModule["startOpenAiResponsesBrain"]>[0],
    ) => ({ wakeId: input.wakeId }),
    drainOpenAiResponsesBrainStream: async (
      input: Parameters<
        NativeBridgeModule["drainOpenAiResponsesBrainStream"]
      >[0],
    ) => {
      drainCount += 1;
      if (drainCount <= 2) {
        const callId = `repeated-failure-call-${drainCount}`;
        return {
          wakeId: input.wakeId,
          items: repeatedFailureToolEvents(input.wakeId, callId),
          toolRequests: [
            {
              wakeId: input.wakeId,
              callId,
              name: "memory",
              argumentsJson: "{}",
            },
          ],
          terminal: false,
        };
      }
      return {
        wakeId: input.wakeId,
        items: [],
        toolRequests: [],
        terminal: true,
      };
    },
    submitOpenAiResponsesToolOutput: async (
      input: Parameters<
        NativeBridgeModule["submitOpenAiResponsesToolOutput"]
      >[0],
    ) => {
      submittedOutputs.push(input);
      return { ok: true, wakeId: input.wakeId, callId: input.callId };
    },
    submitBrainEvent: async (event: BrainEventEnvelope) => {
      observedEvents.push(event);
      return { accepted: true, sequence: observedEvents.length };
    },
  } as unknown as NativeBridgeModule;
  const failureToolParameters = Type.Object({});
  const failureTool: BrainTool<typeof failureToolParameters> = {
    name: "memory",
    label: "Memory",
    description: "Synthetic repeated unavailable memory tool",
    parameters: failureToolParameters,
    execute: async () => ({
      content: [
        {
          type: "text",
          text: "MEMORY_TOOL_RESULT ok=false operation=search action=failed reason=memory_client_unavailable",
        },
      ],
      details: {
        ok: false,
        operation: "search",
        action: "failed",
        reasonCode: "memory_client_unavailable",
        retryable: true,
      },
    }),
  };
  const brain = await openAiResponsesBrainModule.createBrain({
    bridge,
    profile: {
      profile: {
        profileId: "responses-tool-failure-profile",
        modelConfig: {
          provider: "openai",
          modelName: "gpt-5",
          api: "responses",
        },
        brain: {
          module: "openai-responses",
          strategy: "replay",
        },
      },
      skills: [],
      toolSelection: {
        source: "smoke",
        toolProfile: {
          tools: [
            {
              name: "memory",
              description: "Synthetic repeated unavailable memory tool",
            },
          ],
        },
      },
    } as unknown as LoadedProfileContext,
    toolResolver: () => [failureTool],
    providerStateScope: {
      profileFingerprint: "profile-failure-smoke",
      providerFingerprint: "provider-failure-smoke",
    },
    toolCallDebugStore,
  });

  await assert.rejects(
    () => brain.wake(wakeInputForRepeatedFailure()),
    /Stopping assistant turn after repeated memory failure \(memory_client_unavailable\)/,
  );
  assert.equal(submittedOutputs.length, 2);
  assert.equal(
    submittedOutputs.every((output) => output.isError),
    true,
  );
  assert.match(submittedOutputs[0]?.output ?? "", /memory_client_unavailable/);
  const providerStatusReported = observedEvents.some(
    (event) =>
      event.event.type === "provider_status" &&
      event.event.level === "error" &&
      event.event.message.includes("memory_client_unavailable"),
  );
  assert.equal(providerStatusReported, true);
  const completedDebugRecords = observedEvents
    .filter(
      (event) =>
        event.event.type === "tool_call_finished" &&
        event.event.toolName === "memory",
    )
    .flatMap((event) =>
      event.event.type === "tool_call_finished" &&
      typeof event.event.metadata?.debugDetailId === "string"
        ? [event.event.metadata.debugDetailId]
        : [],
    )
    .map((debugDetailId) =>
      toolCallDebugStore.get({
        sessionId: "responses-repeated-failure-session",
        debugDetailId,
      }),
    );
  assert.equal(completedDebugRecords.length, 2);
  assert.equal(
    completedDebugRecords.every(
      (record) =>
        record?.status === "completed" &&
        JSON.stringify(record.final_result?.value).includes(
          "memory_client_unavailable",
        ),
    ),
    true,
  );
  return {
    submittedOutputs: submittedOutputs.length,
    providerStatusReported,
    debugRecordsCompleted: completedDebugRecords.length,
  };
}

async function runSingleDeniedContinuationScenario(): Promise<{
  submittedOutputs: number;
  outputWasProviderError: boolean;
  providerContinuedAfterDenial: boolean;
}> {
  const observedEvents: BrainEventEnvelope[] = [];
  const submittedOutputs: Array<{
    wakeId: string;
    callId: string;
    output: string;
    isError: boolean;
  }> = [];
  let drainCount = 0;
  const bridge = {
    runOpenAiResponsesBrain: async () => {
      throw new Error("blocking Responses runner should not be used");
    },
    startOpenAiResponsesBrain: async (
      input: Parameters<NativeBridgeModule["startOpenAiResponsesBrain"]>[0],
    ) => ({ wakeId: input.wakeId }),
    drainOpenAiResponsesBrainStream: async (
      input: Parameters<
        NativeBridgeModule["drainOpenAiResponsesBrainStream"]
      >[0],
    ) => {
      drainCount += 1;
      if (drainCount === 1) {
        return {
          wakeId: input.wakeId,
          items: toolEvents(
            input.wakeId,
            "responses-single-denial-session",
            "single-denial-call",
            "memory_store",
          ),
          toolRequests: [
            {
              wakeId: input.wakeId,
              callId: "single-denial-call",
              name: "memory_store",
              argumentsJson: "{}",
            },
          ],
          terminal: false,
        };
      }
      return {
        wakeId: input.wakeId,
        items: [
          {
            type: "event",
            event: {
              wakeId: input.wakeId,
              sessionId: "responses-single-denial-session" as SessionId,
              event: {
                type: "text_delta",
                text: "provider continued after single denial",
              },
            },
          },
        ],
        toolRequests: [],
        terminal: true,
      };
    },
    submitOpenAiResponsesToolOutput: async (
      input: Parameters<
        NativeBridgeModule["submitOpenAiResponsesToolOutput"]
      >[0],
    ) => {
      submittedOutputs.push(input);
      return { ok: true, wakeId: input.wakeId, callId: input.callId };
    },
    submitBrainEvent: async (event: BrainEventEnvelope) => {
      observedEvents.push(event);
      return { accepted: true, sequence: observedEvents.length };
    },
  } as unknown as NativeBridgeModule;
  const denialParameters = Type.Object({});
  const denialTool: BrainTool<typeof denialParameters> = {
    name: "memory_store",
    label: "Memory Store",
    description: "Synthetic manual-review memory denial",
    parameters: denialParameters,
    execute: async () => ({
      content: [
        {
          type: "text",
          text: "MEMORY_TOOL_RESULT ok=false operation=store action=denied reason=memory_manual_review_required",
        },
      ],
      details: {
        ok: false,
        operation: "store",
        action: "denied",
        reasonCode: "memory_manual_review_required",
        retryable: false,
      },
    }),
  };
  const brain = await openAiResponsesBrainModule.createBrain({
    bridge,
    profile: loadedProfileContext(
      "responses-single-denial-profile",
      "memory_store",
      "Synthetic manual-review memory denial",
    ),
    toolResolver: () => [denialTool],
    providerStateScope: {
      profileFingerprint: "profile-single-denial-smoke",
      providerFingerprint: "provider-single-denial-smoke",
    },
    toolCallDebugStore: new MemoryToolCallDebugStore({
      now: () => "2026-07-04T00:00:00.000Z",
    }),
  });

  await brain.wake(
    wakeInputForTool({
      wakeId: "responses-single-denial-wake",
      sessionId: "responses-single-denial-session",
      profileId: "responses-single-denial-profile",
      toolName: "memory_store",
      toolDescription: "Synthetic manual-review memory denial",
      body: "Try storing one memory, then explain the denial.",
    }),
  );
  assert.equal(submittedOutputs.length, 1);
  assert.equal(submittedOutputs[0]?.isError, true);
  assert.match(
    submittedOutputs[0]?.output ?? "",
    /memory_manual_review_required/,
  );
  assert.equal(
    observedEvents.some(
      (event) =>
        event.event.type === "provider_status" && event.event.level === "error",
    ),
    false,
  );
  const providerContinuedAfterDenial = observedEvents.some(
    (event) =>
      event.event.type === "text_delta" &&
      event.event.text.includes("provider continued after single denial"),
  );
  assert.equal(providerContinuedAfterDenial, true);
  return {
    submittedOutputs: submittedOutputs.length,
    outputWasProviderError: submittedOutputs[0]?.isError === true,
    providerContinuedAfterDenial,
  };
}

function repeatedFailureToolEvents(
  wakeId: string,
  callId: string,
): BrainWakeStreamItem[] {
  return toolEvents(
    wakeId,
    "responses-repeated-failure-session",
    callId,
    "memory",
  );
}

function toolEvents(
  wakeId: string,
  sessionId: string,
  callId: string,
  toolName: string,
): BrainWakeStreamItem[] {
  return [
    {
      type: "event",
      event: {
        wakeId,
        sessionId: sessionId as SessionId,
        event: {
          type: "tool_call_started",
          toolName,
          metadata: {
            source: "local",
            serverNames: [],
            sourceToolName: callId,
          },
        },
      },
    },
    {
      type: "event",
      event: {
        wakeId,
        sessionId: sessionId as SessionId,
        event: {
          type: "tool_call_finished",
          toolName,
          isError: false,
          metadata: {
            source: "local",
            serverNames: [],
            sourceToolName: callId,
          },
        },
      },
    },
  ];
}

function loadedProfileContext(
  profileId: string,
  toolName: string,
  toolDescription: string,
): LoadedProfileContext {
  return {
    profile: {
      profileId,
      modelConfig: {
        provider: "openai",
        modelName: "gpt-5",
        api: "responses",
      },
      brain: {
        module: "openai-responses",
        strategy: "replay",
      },
    },
    skills: [],
    toolSelection: {
      source: "smoke",
      toolProfile: {
        tools: [
          {
            name: toolName,
            description: toolDescription,
          },
        ],
      },
    },
  } as unknown as LoadedProfileContext;
}

function wakeInputForRepeatedFailure(): BrainWakeInput {
  return wakeInputForTool({
    wakeId: "responses-repeated-failure-wake",
    sessionId: "responses-repeated-failure-session",
    profileId: "responses-tool-failure-profile",
    toolName: "memory",
    toolDescription: "Synthetic repeated unavailable memory tool",
    body: "Search memory repeatedly until available.",
  });
}

function wakeInputForTool(input: {
  wakeId: string;
  sessionId: string;
  profileId: string;
  toolName: string;
  toolDescription: string;
  body: string;
}): BrainWakeInput {
  const toolProfile = {
    tools: [
      {
        name: input.toolName,
        description: input.toolDescription,
      },
    ],
  };
  const base = wakeInput(toolProfile);
  return {
    ...base,
    wakeId: input.wakeId,
    sessionId: input.sessionId as SessionId,
    roleAssembly: {
      instructions: `Use the ${input.toolName} tool to answer the user.`,
    },
    state: {
      ...base.state,
      session: {
        ...base.state.session,
        sessionId: input.sessionId as SessionId,
        profileId: input.profileId as ProfileId,
        toolProfile,
      },
      pendingMessages: [
        {
          from: "tester" as AgentId,
          to: "responses-tool-bridge-agent" as AgentId,
          body: input.body,
        },
      ],
    },
  };
}

function wakeInput(toolProfile: {
  tools: Array<{ name: string; description: string }>;
}): BrainWakeInput {
  return {
    wakeId: "responses-tool-bridge-wake",
    sessionId: "responses-tool-bridge-session" as SessionId,
    systemPrompt: "System instruction marker: use the sentinel tool once.",
    roleAssembly: {
      instructions:
        "Role inventory marker: MCP document tools include den_get_document.",
    },
    state: {
      session: {
        handle: 1 as SessionHandle,
        sessionId: "responses-tool-bridge-session" as SessionId,
        agentId: "responses-tool-bridge-agent" as AgentId,
        profileId: "responses-tool-bridge-profile" as ProfileId,
        kind: "full",
        resourceLimits: {},
        toolProfile,
        status: "idle",
        brainTurnCount: 0,
        createdAt: "2026-07-04T00:00:00Z",
        lastActiveAt: "2026-07-04T00:00:00Z",
      },
      pendingMessages: [
        {
          from: "tester" as AgentId,
          to: "responses-tool-bridge-agent" as AgentId,
          body: "Please use sentinel_tool.",
        },
      ],
      recentEvents: [],
      childCompletions: [],
      fanOutGroups: [],
      deltaPolicy: {
        mode: "frozen_snapshot_next_wake",
        queueOwner: "body",
        queuedMessageTtlMs: 5_000,
        maxQueuedMessages: 32,
      },
    },
  };
}
