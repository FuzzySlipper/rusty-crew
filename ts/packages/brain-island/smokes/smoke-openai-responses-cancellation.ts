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
import { openAiResponsesBrainModule } from "../src/brain-module.js";
import type { BrainWakeInput } from "../src/index.js";
import type { LoadedProfileContext } from "../src/profile-loading.js";

const previousLiveMode = process.env.RUSTY_CREW_OPENAI_RESPONSES_LIVE;
const previousFakeDelay = process.env.RUSTY_CREW_OPENAI_RESPONSES_FAKE_DELAY_MS;

process.env.RUSTY_CREW_OPENAI_RESPONSES_LIVE = "0";
process.env.RUSTY_CREW_OPENAI_RESPONSES_FAKE_DELAY_MS = "200";

try {
  const native = await loadNativeBridge();
  const submittedEvents: BrainEventEnvelope[] = [];
  const bridge = {
    runOpenAiResponsesBrain: async () => {
      throw new Error("blocking Responses runner should not be used");
    },
    startOpenAiResponsesBrain: native.startOpenAiResponsesBrain.bind(native),
    drainOpenAiResponsesBrainStream:
      native.drainOpenAiResponsesBrainStream.bind(native),
    submitOpenAiResponsesToolOutput:
      native.submitOpenAiResponsesToolOutput.bind(native),
    cancelOpenAiResponsesBrain: native.cancelOpenAiResponsesBrain.bind(native),
    submitBrainEvent: async (event: BrainEventEnvelope) => {
      submittedEvents.push(event);
      return { accepted: true, sequence: submittedEvents.length };
    },
  } as unknown as NativeBridgeModule;

  const brain = await openAiResponsesBrainModule.createBrain({
    bridge,
    profile: {
      profile: {
        profileId: "responses-cancel-profile",
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
          maxTurnDurationMs: 60_000,
        },
      },
      skills: [],
      toolSelection: {
        source: "smoke",
        toolProfile: { tools: [] },
      },
    } as unknown as LoadedProfileContext,
    providerStateScope: {
      profileFingerprint: "profile-cancel-smoke",
      providerFingerprint: "provider-cancel-smoke",
    },
  });

  const controller = new AbortController();
  const wakePromise = brain.wake(wakeInput(), { signal: controller.signal });
  setTimeout(() => controller.abort(), 25);

  await assert.rejects(
    () => wakePromise,
    /cancelled by service wake timeout policy/,
  );

  await new Promise((resolve) => setTimeout(resolve, 300));
  assert.deepEqual(
    submittedEvents.map((event) => event.event.type),
    [],
    "cancelled fake Responses wake should not append late stream events",
  );

  console.log(
    JSON.stringify(
      {
        status: "cancelled",
        submittedEventCount: submittedEvents.length,
        noLateEventAppendDrift: submittedEvents.length === 0,
      },
      null,
      2,
    ),
  );
} finally {
  restoreEnv("RUSTY_CREW_OPENAI_RESPONSES_LIVE", previousLiveMode);
  restoreEnv("RUSTY_CREW_OPENAI_RESPONSES_FAKE_DELAY_MS", previousFakeDelay);
}

function wakeInput(): BrainWakeInput {
  return {
    wakeId: "responses-cancel-wake",
    sessionId: "responses-cancel-session" as SessionId,
    systemPrompt: "Return a short acknowledgement.",
    roleAssembly: {},
    state: {
      session: {
        handle: 1 as SessionHandle,
        sessionId: "responses-cancel-session" as SessionId,
        agentId: "responses-cancel-agent" as AgentId,
        profileId: "responses-cancel-profile" as ProfileId,
        kind: "full",
        resourceLimits: {},
        toolProfile: { tools: [] },
        status: "idle",
        brainTurnCount: 0,
        createdAt: "2026-07-07T00:00:00Z",
        lastActiveAt: "2026-07-07T00:00:00Z",
      },
      pendingMessages: [
        {
          from: "tester" as AgentId,
          to: "responses-cancel-agent" as AgentId,
          body: "Please answer slowly enough for cancellation.",
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

function restoreEnv(name: string, previous: string | undefined): void {
  if (previous === undefined) {
    delete process.env[name];
  } else {
    process.env[name] = previous;
  }
}
