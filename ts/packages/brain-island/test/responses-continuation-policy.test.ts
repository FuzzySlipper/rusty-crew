import assert from "node:assert/strict";
import test from "node:test";
import type {
  AgentId,
  ProfileId,
  SessionHandle,
  SessionId,
} from "@rusty-crew/contracts";
import type {
  NativeBridgeModule,
  OpenAiResponsesBrainRunInput,
} from "@rusty-crew/native-bridge";

import type { BrainHostContext } from "../src/brain-host-context.js";
import { createOpenAiResponsesBrainHost } from "../src/openai-responses-host.js";
import {
  DEFAULT_RESPONSES_NO_PROGRESS_ATTENTION_THRESHOLD,
  DEFAULT_RESPONSES_WORK_QUANTUM_CONTINUATION_ROUNDS,
  responsesContinuationDiagnostics,
  responsesNoProgressAttentionThreshold,
  responsesWorkQuantumContinuationRounds,
} from "../src/responses-continuation-policy.js";
import type { BrainWakeInput } from "../src/index.js";

const variable = "RUSTY_CREW_OPENAI_RESPONSES_WORK_QUANTUM_CONTINUATION_ROUNDS";
const noProgressVariable =
  "RUSTY_CREW_OPENAI_RESPONSES_NO_PROGRESS_ATTENTION_THRESHOLD";

test("Responses continuation policy uses the durable default", () => {
  assert.equal(
    responsesWorkQuantumContinuationRounds({}),
    DEFAULT_RESPONSES_WORK_QUANTUM_CONTINUATION_ROUNDS,
  );
  assert.deepEqual(responsesContinuationDiagnostics("openai-responses"), {
    workQuantumContinuationRounds:
      DEFAULT_RESPONSES_WORK_QUANTUM_CONTINUATION_ROUNDS,
    noProgressAttentionThreshold:
      DEFAULT_RESPONSES_NO_PROGRESS_ATTENTION_THRESHOLD,
  });
});

test("Responses no-progress policy is explicit and configurable", () => {
  assert.equal(
    responsesNoProgressAttentionThreshold({}),
    DEFAULT_RESPONSES_NO_PROGRESS_ATTENTION_THRESHOLD,
  );
  assert.equal(
    responsesNoProgressAttentionThreshold({ [noProgressVariable]: " 8 " }),
    8,
  );
  for (const value of ["0", "1", "1.5", "many"]) {
    assert.throws(
      () =>
        responsesNoProgressAttentionThreshold({
          [noProgressVariable]: value,
        }),
      new RegExp(noProgressVariable),
    );
  }
});

test("Responses continuation policy accepts an explicit work quantum", () => {
  assert.equal(
    responsesWorkQuantumContinuationRounds({ [variable]: " 96 " }),
    96,
  );
  assert.equal(
    responsesWorkQuantumContinuationRounds({ [variable]: "1000000" }),
    1_000_000,
  );
});

test("Responses continuation policy rejects non-positive and invalid quanta", () => {
  for (const value of ["0", "-1", "1.5", "many"]) {
    assert.throws(
      () => responsesWorkQuantumContinuationRounds({ [variable]: value }),
      new RegExp(variable),
    );
  }
});

test("Responses host forwards strategy quantum and continuation to Rust", async () => {
  let captured: OpenAiResponsesBrainRunInput | undefined;
  const bridge = {
    startBrainRun: async (
      input: Parameters<NativeBridgeModule["startBrainRun"]>[0],
    ) => {
      if (input.moduleId !== "openai-responses") {
        throw new Error(`unexpected module ${input.moduleId}`);
      }
      captured = input.providerInput;
      return { moduleId: input.moduleId, wakeId: input.providerInput.wakeId };
    },
    drainBrainRun: async () => ({
      items: [],
      toolRequests: [],
      terminal: true,
      yielded: true,
      continuationState: continuationState,
    }),
  } as unknown as NativeBridgeModule;
  const context = {
    bridge,
    profile: {
      profile: {
        profileId: "responses-continuation-profile" as ProfileId,
        modelConfig: {
          provider: "test",
          modelName: "test-model",
          api: "responses",
          responsesDialect: "openai_stateful",
          contextWindowTokens: 32_000,
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
  const brain = await createOpenAiResponsesBrainHost(
    context,
    { mode: "fake" },
    "previous-response-chain",
  );

  const result = await brain.wake(responsesWake());

  assert.equal(
    captured?.config.workQuantumContinuationRounds,
    DEFAULT_RESPONSES_WORK_QUANTUM_CONTINUATION_ROUNDS,
  );
  assert.equal(captured?.config.strategyId, "previous-response-chain");
  assert.equal(
    captured?.config.noProgressAttentionThreshold,
    DEFAULT_RESPONSES_NO_PROGRESS_ATTENTION_THRESHOLD,
  );
  assert.deepEqual(captured?.config.contextCompaction, {
    enabled: true,
    autoCompactionEnabled: true,
    strategyId: "rolling_summary_compaction",
    contextWindowTokens: 32_000,
    compactAtPercent: 80,
    targetPercentAfterCompaction: 55,
  });
  assert.deepEqual(captured?.continuationState, continuationState);
  assert.equal(result.outcome, "yielded");
  assert.deepEqual(result.continuationState, continuationState);
});

test("Responses narrator host forwards the versioned Roleplay adapter context", async () => {
  let captured: OpenAiResponsesBrainRunInput | undefined;
  const bridge = {
    startBrainRun: async (
      input: Parameters<NativeBridgeModule["startBrainRun"]>[0],
    ) => {
      if (input.moduleId !== "openai-responses") {
        throw new Error(`unexpected module ${input.moduleId}`);
      }
      captured = input.providerInput;
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
        profileId: "roleplay-responses-profile" as ProfileId,
        brain: { strategy: "roleplay_narrator" },
        modelConfig: {
          provider: "test",
          modelName: "test-model",
          api: "responses",
          responsesDialect: "openai_stateless",
          contextWindowTokens: 32_000,
        },
        contextPolicy: {
          enabled: true,
          strategyId: "roleplay_scene_aware_compaction",
          autoCompactionEnabled: true,
          compactAtPercent: 70,
          targetPercentAfterCompaction: 45,
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
  const brain = await createOpenAiResponsesBrainHost(context, { mode: "fake" });

  await brain.wake({ ...responsesWake(), continuationState: undefined });

  assert.deepEqual(captured?.compactionDomainContext, {
    schemaVersion: 1,
    retentionTiers: [],
    directorsNotes: [],
    extractionRequests: [],
  });
  assert.equal(
    captured?.config.contextCompaction?.strategyId,
    "roleplay_scene_aware_compaction",
  );
});

const continuationState = {
  moduleId: "openai-responses",
  payloadVersion: "openai-responses-continuation-v1",
  payloadFingerprint: "checkpoint-fingerprint",
  payload: { cursor: 1 },
};

function responsesWake(): BrainWakeInput {
  const sessionId = "responses-continuation-session" as SessionId;
  return {
    wakeId: "responses-continuation-wake",
    sessionId,
    systemPrompt: "system",
    roleAssembly: {},
    continuationState,
    state: {
      session: {
        handle: 1 as SessionHandle,
        sessionId,
        agentId: "responses-continuation-agent" as AgentId,
        profileId: "responses-continuation-profile" as ProfileId,
        kind: "full",
        resourceLimits: { maxDelegationDepth: 0 },
        toolProfile: { tools: [] },
        status: "idle",
        brainTurnCount: 0,
        createdAt: "2026-07-29T00:00:00Z",
        lastActiveAt: "2026-07-29T00:00:00Z",
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
