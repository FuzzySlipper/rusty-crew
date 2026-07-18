import assert from "node:assert/strict";
import test from "node:test";
import type {
  AgentId,
  ProfileId,
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
  chatCompletionsMaxToolRounds,
  DEFAULT_CHAT_COMPLETIONS_MAX_TOOL_ROUNDS,
} from "../src/chat-completions-continuation-policy.js";
import { createChatCompletionsBrainHost } from "../src/chat-completions-host.js";
import type { BrainWakeInput } from "../src/index.js";

const variable = "RUSTY_CREW_CHAT_COMPLETIONS_MAX_TOOL_ROUNDS";

test("Chat Completions continuation policy uses the durable default", () => {
  assert.equal(
    chatCompletionsMaxToolRounds({}),
    DEFAULT_CHAT_COMPLETIONS_MAX_TOOL_ROUNDS,
  );
  assert.deepEqual(chatCompletionsContinuationDiagnostics("chat-completions"), {
    maxContinuationRounds: DEFAULT_CHAT_COMPLETIONS_MAX_TOOL_ROUNDS,
  });
  assert.deepEqual(
    chatCompletionsContinuationDiagnostics("openai-responses"),
    {},
  );
});

test("Chat Completions continuation policy accepts an explicit bounded limit", () => {
  assert.equal(chatCompletionsMaxToolRounds({ [variable]: " 96 " }), 96);
});

test("Chat Completions continuation policy rejects zero and unreasonable limits", () => {
  for (const value of ["0", "-1", "1.5", "513", "many"]) {
    assert.throws(
      () => chatCompletionsMaxToolRounds({ [variable]: value }),
      new RegExp(variable),
    );
  }
});

test("Chat Completions host sends the continuation budget to the native boundary", async () => {
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
        },
      },
      skills: [],
      toolSelection: { toolProfile: { tools: [] } },
    },
  } as unknown as BrainHostContext;
  const brain = createChatCompletionsBrainHost(context, { mode: "fake" });

  await brain.wake(continuationWake());

  assert.equal(
    capturedConfig?.maxToolRounds,
    DEFAULT_CHAT_COMPLETIONS_MAX_TOOL_ROUNDS,
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
