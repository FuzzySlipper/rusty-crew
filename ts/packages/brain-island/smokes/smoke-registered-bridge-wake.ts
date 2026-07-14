import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type {
  AgentEvent as ChatCompletionsEvent,
  AgentMessage as ChatCompletionsMessage,
} from "./support/chat-completions-test-harness.js";
import type {
  AdapterId,
  AgentId,
  BrainAction,
  BrainImplementationId,
  CompletionPacket,
  ProfileId,
  ProjectId,
  SessionId,
} from "@rusty-crew/contracts";
import { loadNativeBridge } from "@rusty-crew/native-bridge";
import { registerBrainHostRuntime } from "../src/index.js";
import { createChatCompletionsBrain } from "./support/chat-completions-test-harness.js";
import type {
  ChatCompletionsFactory,
  ChatCompletionsLike,
} from "./support/chat-completions-test-harness.js";

const encoder = new TextEncoder();
const abortSignal = new AbortController().signal;
const engineDataDir = mkdtempSync(
  join(tmpdir(), "rusty-crew-registered-wake-"),
);
const native = await loadNativeBridge();
const engine = await native.initializeEngine({
  engineDataDir,
  clock: { fixed: "2026-06-19T00:00:00Z" },
  defaultTurnBudget: 3,
  defaultIdleTimeoutMs: 1_000,
});

try {
  const sessionId = "registered-wake-session" as SessionId;
  const agentId = "registered-wake-agent" as AgentId;
  const profileId = "registered-wake-profile" as ProfileId;
  const wakeId = "registered-wake-1";
  const adapterId = "den" as AdapterId;

  const adapter = await native.registerPlatformAdapter({
    adapterId,
    kind: "den",
    displayName: "Den",
  });
  assert.equal(adapter, 1);
  await assert.rejects(
    () =>
      native.registerPlatformAdapter({
        adapterId,
        kind: "den",
        displayName: "Duplicate Den",
      }),
    /AlreadyExists/,
  );
  const denReceipt = await native.injectDenDataUpdate({
    projectId: "rusty-crew" as ProjectId,
    entityKind: "task",
    entityId: "2835",
    revision: "smoke",
  });
  assert.equal(denReceipt.accepted, true);

  await native.createSession({
    sessionId,
    agentId,
    profileId,
    kind: "worker",
  });
  const wakeSubscription = await native.subscribeEvents({
    eventKinds: ["brain_wake_requested"],
    sessionId,
  });
  await native.routeAgentMessage(
    "planner",
    agentId,
    "wake through the registered bridge runtime",
  );
  const wakeEvents = await native.drainSubscriptionEvents(wakeSubscription, 4);
  assert.equal(wakeEvents.length, 1);
  assert.deepEqual(wakeEvents[0], {
    type: "brain_wake_requested",
    sessionId,
  });
  await native.unsubscribeEvents(wakeSubscription);
  await assert.rejects(
    () => native.drainSubscriptionEvents(wakeSubscription, 1),
    /not registered/,
  );

  const brain = await registerBrainHostRuntime(
    native,
    {
      implementationId: "registered-local" as BrainImplementationId,
      profileId,
      toolProfile: { tools: [] },
      modelConfig: {
        provider: "local",
        modelName: "deterministic",
      },
    },
    createChatCompletionsBrain({
      createAgent: createStubChatCompletionsFactory,
      planActions: ({ wake }): BrainAction[] => [
        {
          type: "deliver_completion",
          packet: {
            sessionId: wake.sessionId,
            status: "completed",
            summary: "registered chat-completions bridge wake completed",
          } satisfies CompletionPacket,
        },
      ],
    }),
  );

  const request = await native.buildBrainWakeRequestForSession({
    brain,
    sessionId,
    systemPrompt: "You are a local registered bridge smoke brain.",
    roleAssemblyJson: encoder.encode(
      JSON.stringify({
        instructions:
          "Return the deterministic completion action from createLocalBrain.",
        initialMessages: [],
      }),
    ),
    wakeId,
  });

  const accepted = await native.wakeBrain(request);
  assert.deepEqual(accepted, { wakeId, accepted: true });
  assert.equal(await native.diagnosticCountRows("completion_packets"), 1);

  console.log(
    JSON.stringify(
      {
        wakeId,
        accepted: accepted.accepted,
        adapterHandle: adapter,
        denUpdateSequence: denReceipt.sequence,
        subscriptionEvents: wakeEvents.map((event) => event.type),
        completionPackets:
          await native.diagnosticCountRows("completion_packets"),
      },
      null,
      2,
    ),
  );
} finally {
  await native.shutdownEngine({ engine, drainTimeoutMs: 1_000 });
  rmSync(engineDataDir, { force: true, recursive: true });
}

function createStubChatCompletions(): ChatCompletionsLike {
  let listener: Parameters<ChatCompletionsLike["subscribe"]>[0] | undefined;

  return {
    subscribe(callback) {
      listener = callback;
      return () => {
        listener = undefined;
      };
    },
    async prompt(
      input: ChatCompletionsMessage | ChatCompletionsMessage[] | string,
    ) {
      const messages = Array.isArray(input) ? input : [input];
      await listener?.(
        { type: "agent_start" } as ChatCompletionsEvent,
        abortSignal,
      );
      listener?.(
        {
          type: "message_update",
          assistantMessageEvent: {
            type: "text_delta",
            delta: `stub chat-completions saw ${messages.length} message(s)`,
          },
          message: {
            role: "assistant",
            content: [
              {
                type: "text",
                text: `stub chat-completions saw ${messages.length} message(s)`,
              },
            ],
            timestamp: Date.now(),
          },
        } as ChatCompletionsEvent,
        abortSignal,
      );
      await listener?.(
        { type: "agent_end" } as ChatCompletionsEvent,
        abortSignal,
      );
    },
    async waitForIdle() {},
    clearAllQueues() {},
  };
}

function createStubChatCompletionsFactory(): ChatCompletionsLike {
  return createStubChatCompletions();
}
