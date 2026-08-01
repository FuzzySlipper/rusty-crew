import assert from "node:assert/strict";
import test from "node:test";
import type {
  AgentId,
  BrainWakeProviderStateInput,
  ProfileId,
  RunId,
  SessionHandle,
  SessionId,
} from "@rusty-crew/contracts";
import type {
  ChatCompletionsBrainRunInput,
  NativeBridgeModule,
} from "@rusty-crew/native-bridge";

import type { BrainHostContext } from "../src/brain-host-context.js";
import {
  chatCompletionsContinuationDiagnostics,
  chatCompletionsNoProgressAttentionThreshold,
  chatCompletionsWorkQuantumToolRounds,
  DEFAULT_CHAT_COMPLETIONS_NO_PROGRESS_ATTENTION_THRESHOLD,
  DEFAULT_CHAT_COMPLETIONS_WORK_QUANTUM_TOOL_ROUNDS,
} from "../src/chat-completions-continuation-policy.js";
import { createChatCompletionsBrainHost } from "../src/chat-completions-host.js";
import type { BrainWakeInput } from "../src/index.js";

const variable = "RUSTY_CREW_CHAT_COMPLETIONS_WORK_QUANTUM_TOOL_ROUNDS";
const noProgressVariable =
  "RUSTY_CREW_CHAT_COMPLETIONS_NO_PROGRESS_ATTENTION_THRESHOLD";

test("Chat Completions continuation policy uses the scheduling default", () => {
  assert.equal(
    chatCompletionsWorkQuantumToolRounds({}),
    DEFAULT_CHAT_COMPLETIONS_WORK_QUANTUM_TOOL_ROUNDS,
  );
  assert.deepEqual(chatCompletionsContinuationDiagnostics("chat-completions"), {
    workQuantumToolRounds: DEFAULT_CHAT_COMPLETIONS_WORK_QUANTUM_TOOL_ROUNDS,
    noProgressAttentionThreshold:
      DEFAULT_CHAT_COMPLETIONS_NO_PROGRESS_ATTENTION_THRESHOLD,
  });
  assert.deepEqual(
    chatCompletionsContinuationDiagnostics("openai-responses"),
    {},
  );
});

test("Chat Completions no-progress policy is explicit and configurable", () => {
  assert.equal(
    chatCompletionsNoProgressAttentionThreshold({}),
    DEFAULT_CHAT_COMPLETIONS_NO_PROGRESS_ATTENTION_THRESHOLD,
  );
  assert.equal(
    chatCompletionsNoProgressAttentionThreshold({
      [noProgressVariable]: " 9 ",
    }),
    9,
  );
  for (const value of ["0", "1", "1.5", "many"]) {
    assert.throws(
      () =>
        chatCompletionsNoProgressAttentionThreshold({
          [noProgressVariable]: value,
        }),
      new RegExp(noProgressVariable),
    );
  }
});

test("Chat Completions continuation policy accepts a large scheduling quantum", () => {
  assert.equal(
    chatCompletionsWorkQuantumToolRounds({ [variable]: " 100000 " }),
    100_000,
  );
});

test("Chat Completions continuation policy rejects non-positive or invalid quanta", () => {
  for (const value of ["0", "-1", "1.5", "9007199254740992", "many"]) {
    assert.throws(
      () => chatCompletionsWorkQuantumToolRounds({ [variable]: value }),
      new RegExp(variable),
    );
  }
});

test("Chat Completions host sends the work quantum to the native boundary", async () => {
  let capturedConfig: ChatCompletionsBrainRunInput["config"] | undefined;
  const bridge = {
    startBrainRun: async (
      input: Parameters<NativeBridgeModule["startBrainRun"]>[0],
    ) => {
      if (input.moduleId !== "chat-completions") {
        throw new Error(`unexpected module ${input.moduleId}`);
      }
      capturedConfig = input.providerInput.config;
      return { moduleId: input.moduleId, wakeId: input.providerInput.wakeId };
    },
    drainBrainRun: async () => ({
      items: [],
      toolRequests: [],
      terminal: true,
    }),
  } as unknown as NativeBridgeModule;
  const context = {
    bridge,
    profile: {
      profile: {
        profileId: "continuation-profile" as ProfileId,
        modelConfig: {
          provider: "test",
          modelName: "test-model",
          api: "openai-completions",
          contextWindowTokens: 16_000,
        },
        contextPolicy: {
          enabled: true,
          strategyId: "rolling_summary_compaction",
          autoCompactionEnabled: true,
          compactAtPercent: 80,
          targetPercentAfterCompaction: 55,
          maxContextPercentForWake: 95,
          debugVisibility: "status",
          includeDebugEventsInModelContext: false,
          strategyConfig: {},
        },
      },
      skills: [],
      toolSelection: { toolProfile: { tools: [] } },
    },
  } as unknown as BrainHostContext;
  const brain = createChatCompletionsBrainHost(context, { mode: "fake" });

  await brain.wake(continuationWake());

  assert.equal(
    capturedConfig?.workQuantumToolRounds,
    DEFAULT_CHAT_COMPLETIONS_WORK_QUANTUM_TOOL_ROUNDS,
  );
  assert.equal(
    capturedConfig?.noProgressAttentionThreshold,
    DEFAULT_CHAT_COMPLETIONS_NO_PROGRESS_ATTENTION_THRESHOLD,
  );
  assert.deepEqual(capturedConfig?.contextCompaction, {
    enabled: true,
    autoCompactionEnabled: true,
    strategyId: "rolling_summary_compaction",
    contextWindowTokens: 16_000,
    compactAtPercent: 80,
    targetPercentAfterCompaction: 55,
  });
});

test("Chat Completions host sends role bootstrap messages only before provider state exists", async () => {
  const capturedMessages: ChatCompletionsBrainRunInput["messages"][] = [];
  const providerState = {
    type: "replace" as const,
    state: {
      moduleId: "chat-completions",
      strategyId: "default",
      profileFingerprint: "profile-fingerprint",
      providerFingerprint: "provider-fingerprint",
      payloadVersion: "chat-completions-history-v1",
      payload: { messages: [] },
      ttlMs: 60_000,
    },
  };
  const bridge = {
    startBrainRun: async (
      input: Parameters<NativeBridgeModule["startBrainRun"]>[0],
    ) => {
      if (input.moduleId !== "chat-completions") {
        throw new Error(`unexpected module ${input.moduleId}`);
      }
      capturedMessages.push(input.providerInput.messages);
      return { moduleId: input.moduleId, wakeId: input.providerInput.wakeId };
    },
    drainBrainRun: async () => ({
      items: [],
      toolRequests: [],
      terminal: true,
      providerState,
    }),
  } as unknown as NativeBridgeModule;
  const context = {
    bridge,
    profile: {
      profile: {
        profileId: "history-profile" as ProfileId,
        modelConfig: {
          provider: "test",
          modelName: "test-model",
          api: "openai-completions",
          chatCompletionsDialect: "kimi",
          thinkingMode: "enabled",
          reasoningHistory: "preserve_all",
          maxOutputTokens: 16_000,
        },
      },
      skills: [],
      toolSelection: { toolProfile: { tools: [] } },
    },
  } as unknown as BrainHostContext;
  const brain = createChatCompletionsBrainHost(context, { mode: "fake" });
  const firstWake = historyWake("history-wake-1", "first question");

  const first = await brain.wake(firstWake);
  assert.equal(first.providerState?.type, "replace");
  const state =
    first.providerState?.type === "replace"
      ? first.providerState.state
      : undefined;
  assert.ok(state);
  const restored: BrainWakeProviderStateInput = {
    moduleId: state.moduleId,
    strategyId: state.strategyId,
    profileFingerprint: state.profileFingerprint,
    providerFingerprint: state.providerFingerprint,
    payloadVersion: state.payloadVersion,
    payload: state.payload,
  };
  await brain.wake({
    ...historyWake("history-wake-2", "second question"),
    providerState: restored,
  });

  assert.deepEqual(
    capturedMessages.map((messages) =>
      messages.map((message) => message.content),
    ),
    [
      [
        "system\n\nrole instructions",
        "bootstrap role message",
        "first question",
      ],
      ["system\n\nrole instructions", "second question"],
    ],
  );
});

test("Chat Completions host injects each delegated completion once", async () => {
  const capturedMessages: ChatCompletionsBrainRunInput["messages"][] = [];
  const bridge = {
    startBrainRun: async (
      input: Parameters<NativeBridgeModule["startBrainRun"]>[0],
    ) => {
      if (input.moduleId !== "chat-completions") {
        throw new Error(`unexpected module ${input.moduleId}`);
      }
      capturedMessages.push(input.providerInput.messages);
      return { moduleId: input.moduleId, wakeId: input.providerInput.wakeId };
    },
    drainBrainRun: async () => ({
      items: [],
      toolRequests: [],
      terminal: true,
    }),
  } as unknown as NativeBridgeModule;
  const context = {
    bridge,
    profile: {
      profile: {
        profileId: "continuation-profile" as ProfileId,
        modelConfig: {
          provider: "test",
          modelName: "test-model",
          api: "openai-completions",
        },
      },
      skills: [],
      toolSelection: { toolProfile: { tools: [] } },
    },
  } as unknown as BrainHostContext;
  const brain = createChatCompletionsBrainHost(context, { mode: "fake" });
  const completionMessage = [
    "[Rusty Crew delegated completion]",
    "run_id: run-1",
    "child_session_id: child-1",
    "status: completed",
    "correlation_id: scout-1",
    "summary:",
    "scout evidence",
  ].join("\n");
  const wake = continuationWake();
  wake.state.childCompletions = [
    {
      runId: "run-1" as RunId,
      childSessionId: "child-1" as SessionId,
      sourceWakeId: "parent-wake",
      sourceActionIndex: 0,
      correlationId: "scout-1",
      parentConsumption: "await_completion",
      packet: {
        sessionId: "child-1" as SessionId,
        status: "completed",
        summary: "scout evidence",
      },
    },
  ];

  await brain.wake(wake);
  await brain.wake({
    ...wake,
    wakeId: "continuation-wake-2",
    providerState: {
      moduleId: "chat-completions",
      strategyId: "default",
      profileFingerprint: "profile-fingerprint",
      providerFingerprint: "provider-fingerprint",
      payloadVersion: "chat-completions-history-v1",
      payload: {
        messages: [{ role: "user", content: completionMessage }],
      },
    },
  });

  assert.equal(
    capturedMessages[0]?.filter(
      (message) => message.content === completionMessage,
    ).length,
    1,
  );
  assert.equal(
    capturedMessages[1]?.filter(
      (message) => message.content === completionMessage,
    ).length,
    0,
  );
});

function continuationWake(): BrainWakeInput {
  const sessionId = "continuation-session" as SessionId;
  return {
    wakeId: "continuation-wake",
    sessionId,
    systemPrompt: "system",
    roleAssembly: {},
    state: {
      session: {
        handle: 1 as SessionHandle,
        sessionId,
        agentId: "continuation-agent" as AgentId,
        profileId: "continuation-profile" as ProfileId,
        kind: "full",
        resourceLimits: { maxDelegationDepth: 0 },
        toolProfile: { tools: [] },
        status: "idle",
        brainTurnCount: 0,
        createdAt: "2026-07-18T00:00:00Z",
        lastActiveAt: "2026-07-18T00:00:00Z",
      },
      pendingMessages: [],
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

function historyWake(wakeId: string, pendingBody: string): BrainWakeInput {
  const wake = continuationWake();
  return {
    ...wake,
    wakeId,
    systemPrompt: "system",
    roleAssembly: {
      instructions: "role instructions",
      initialMessages: [
        {
          from: "history-operator" as AgentId,
          to: "continuation-agent" as AgentId,
          body: "bootstrap role message",
        },
      ],
    },
    state: {
      ...wake.state,
      pendingMessages: [
        {
          from: "history-operator" as AgentId,
          to: "continuation-agent" as AgentId,
          body: pendingBody,
        },
      ],
    },
  };
}
