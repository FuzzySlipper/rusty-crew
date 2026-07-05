import assert from "node:assert/strict";
import type {
  AgentId,
  BrainEventEnvelope,
  ProfileId,
  SessionHandle,
  SessionId,
} from "@rusty-crew/contracts";
import {
  loadNativeBridge,
  type NativeBridgeModule,
} from "@rusty-crew/native-bridge";
import { Type } from "typebox";
import type { BrainTool } from "./brain-tool.js";
import { openAiResponsesBrainModule } from "./brain-module.js";
import type { BrainWakeInput } from "./index.js";
import type { LoadedProfileContext } from "./profile-loading.js";
import { MemoryToolCallDebugStore } from "./tool-call-debug-store.js";

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
      },
      skills: [],
      toolSelection: {
        source: "smoke",
        toolProfile,
      },
    } as unknown as LoadedProfileContext,
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

  console.log(
    JSON.stringify(
      {
        observedEventTypes: observedEvents.map((event) => event.event.type),
        actionTypes: result.actions.map((action) => action.type),
        streamItemCounts: result.brainStreamItemCounts,
        providerStateContainsRealToolOutput: providerStateText.includes(
          "SENTINEL_REAL_TOOL_OUTPUT",
        ),
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
