import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type {
  AgentEvent as PiAgentEvent,
  AgentMessage as PiAgentMessage,
  AgentOptions as PiAgentOptions,
} from "@earendil-works/pi-agent-core";
import type {
  BrainImplementationHandle,
  SessionId,
} from "@rusty-crew/contracts";
import { loadNativeBridge } from "@rusty-crew/native-bridge";
import {
  buildRuntimeDiagnosticsProjection,
  type RuntimeBrainModuleDiagnostics,
} from "./runtime-diagnostics.js";
import { loadRustyCrewServiceConfig } from "./service-config.js";
import {
  applyRustyCrewRuntimeConfig,
  loadRustyCrewRuntimeConfig,
} from "./service-runtime-config.js";

const encoder = new TextEncoder();
const abortSignal = new AbortController().signal;
const root = mkdtempSync(join(tmpdir(), "rusty-crew-brain-modules-"));
const native = await loadNativeBridge();

class FinalTextFakePiAgent {
  private listener?: (event: PiAgentEvent, signal: AbortSignal) => void;

  constructor(private readonly options: PiAgentOptions) {}

  subscribe(
    listener: (event: PiAgentEvent, signal: AbortSignal) => void,
  ): () => void {
    this.listener = listener;
    return () => {
      this.listener = undefined;
    };
  }

  async prompt(
    _input: PiAgentMessage | PiAgentMessage[] | string,
  ): Promise<void> {
    this.listener?.({ type: "agent_start" } as PiAgentEvent, abortSignal);
    this.listener?.(
      {
        type: "message_end",
        message: {
          role: "assistant",
          content: [
            {
              type: "text",
              text: `pi module ${this.options.sessionId} replied`,
            },
          ],
          api: "openai-completions",
          provider: "den-router",
          model: "fake-model",
          usage: {
            input: 0,
            output: 0,
            cacheRead: 0,
            cacheWrite: 0,
            totalTokens: 0,
            cost: {
              input: 0,
              output: 0,
              cacheRead: 0,
              cacheWrite: 0,
              total: 0,
            },
          },
          stopReason: "stop",
          timestamp: Date.now(),
        },
      } as PiAgentEvent,
      abortSignal,
    );
    this.listener?.(
      { type: "agent_end", messages: [] } as PiAgentEvent,
      abortSignal,
    );
  }

  async waitForIdle(): Promise<void> {}

  clearAllQueues(): void {}
}

try {
  writeRuntimeConfig(root);
  const serviceConfig = loadRustyCrewServiceConfig({
    RUSTY_CREW_DATA_DIR: root,
    RUSTY_CREW_ADMIN_AUTH_MODE: "none",
  });
  const runtimeConfig = await loadRustyCrewRuntimeConfig(serviceConfig);
  const engine = await native.initializeEngine({
    engineDataDir: serviceConfig.paths.engineDataDir,
    clock: { fixed: "2026-06-23T19:00:00Z" },
    defaultTurnBudget: 8,
    defaultIdleTimeoutMs: 1_000,
  });
  try {
    const denRouterOptions: unknown[] = [];
    const applyResult = await applyRustyCrewRuntimeConfig({
      serviceConfig,
      runtimeConfig,
      bridge: native,
      createDenRouterAgentFactory: async (options) => {
        denRouterOptions.push(options);
        return (agentOptions) => new FinalTextFakePiAgent(agentOptions);
      },
    });

    assert.equal(
      applyResult.brainModulesByProfileId["pi-profile"]?.moduleId,
      "pi-agent-core",
    );
    assert.equal(
      applyResult.brainModulesByProfileId["local-profile"]?.moduleId,
      "local",
    );
    assert.equal(
      applyResult.brainModulesByProfileId["responses-profile"]?.moduleId,
      "openai-responses",
    );
    assert.equal(
      applyResult.brainDiagnosticsByProfileId["pi-profile"]?.toolAdapterStatus,
      "neutral_tools_adapted_to_pi",
    );
    assert.equal(
      applyResult.brainDiagnosticsByProfileId["pi-profile"]?.selectedToolSource,
      "default-local-tools",
    );
    assert.ok(
      (applyResult.brainDiagnosticsByProfileId["pi-profile"]
        ?.selectedToolCount ?? 0) > 0,
      "pi module diagnostics should report selected tools",
    );
    assert.equal(
      applyResult.brainDiagnosticsByProfileId["local-profile"]
        ?.toolAdapterStatus,
      "tools_not_used",
    );
    assert.equal(
      applyResult.brainDiagnosticsByProfileId["responses-profile"]
        ?.providerStateMode,
      "optional",
    );
    assert.equal(
      applyResult.brainDiagnosticsByProfileId["responses-profile"]
        ?.providerStateRebuild?.action,
      "discard",
    );
    assert.equal(
      applyResult.brainDiagnosticsByProfileId["responses-profile"]
        ?.toolAdapterStatus,
      "native_neutral_tools",
    );
    const chainDiagnostics =
      applyResult.brainDiagnosticsByProfileId["responses-chain-profile"]
        ?.strategyDiagnostics;
    assert.equal(
      chainDiagnostics?.selectedStrategyId,
      "previous-response-chain",
    );
    assert.equal(chainDiagnostics?.effectiveStrategyId, "replay");
    assert.equal(chainDiagnostics?.replayFallbackUsed, true);
    assert.equal(chainDiagnostics?.fallbackReason, "normal_invalidation");
    assert.deepEqual(chainDiagnostics?.fallbackReasonCatalog, [
      "no_predecessor_state",
      "request_fingerprint_mismatch",
      "profile_fingerprint_mismatch",
      "provider_fingerprint_mismatch",
      "predecessor_rejected_by_provider",
      "provider_state_expired",
      "provider_state_load_failed",
      "input_not_append_only",
      "normal_invalidation",
    ]);
    assert.equal(denRouterOptions.length, 1);

    const diagnostics = buildRuntimeDiagnosticsProjection({
      now: "2026-06-23T19:00:00Z",
      sessions: await native.listSessions(),
      brainModules: brainModuleDiagnostics(runtimeConfig, applyResult),
    });
    assert.deepEqual(
      diagnostics.runtime.brainModules.map((module) => [
        module.profileId,
        module.implementationId,
        module.moduleId,
        module.strategy,
        module.effectiveStrategy,
        module.providerStateMode,
        module.selectedToolSource,
        module.toolAdapterStatus,
      ]),
      [
        [
          "pi-profile",
          "pi-brain",
          "pi-agent-core",
          "default",
          "default",
          "unused",
          "default-local-tools",
          "neutral_tools_adapted_to_pi",
        ],
        [
          "local-profile",
          "local-brain",
          "local",
          undefined,
          "default",
          "unused",
          "default-local-tools",
          "tools_not_used",
        ],
        [
          "responses-profile",
          "responses-brain",
          "openai-responses",
          "replay",
          "replay",
          "optional",
          "default-local-tools",
          "native_neutral_tools",
        ],
        [
          "responses-chain-profile",
          "responses-chain-brain",
          "openai-responses",
          "previous-response-chain",
          "previous-response-chain",
          "optional",
          "default-local-tools",
          "native_neutral_tools",
        ],
      ],
    );
    assert.equal(
      diagnostics.runtime.brainModules.find(
        (module) => module.profileId === "responses-chain-profile",
      )?.strategyDiagnostics?.effectiveStrategyId,
      "replay",
    );

    const brainEvents = await native.subscribeEvents({
      eventKinds: ["brain_event_observed"],
    });
    const piResult = await wakeSession(
      applyResult.brainHandlesByProfileId["pi-profile"],
      "pi-session" as SessionId,
      "wake-pi-module",
    );
    const localResult = await wakeSession(
      applyResult.brainHandlesByProfileId["local-profile"],
      "local-session" as SessionId,
      "wake-local-module",
    );
    const responsesResult = await wakeSession(
      applyResult.brainHandlesByProfileId["responses-profile"],
      "responses-session" as SessionId,
      "wake-responses-module",
    );

    assert.deepEqual(piResult, { wakeId: "wake-pi-module", accepted: true });
    assert.deepEqual(localResult, {
      wakeId: "wake-local-module",
      accepted: true,
    });
    assert.deepEqual(responsesResult, {
      wakeId: "wake-responses-module",
      accepted: true,
    });
    assert.equal(await native.countRows("completion_packets"), 3);

    const observedEvents = await native.drainSubscriptionEvents(
      brainEvents,
      10,
    );
    await native.unsubscribeEvents(brainEvents);
    const piText = observedEvents
      .flatMap((event) =>
        event.type === "brain_event_observed" &&
        event.wakeId === "wake-pi-module" &&
        event.event.type === "text_delta"
          ? [event.event.text]
          : [],
      )
      .join("");
    assert.match(piText, /pi module pi-session replied/);

    console.log(
      JSON.stringify(
        {
          modules: diagnostics.runtime.brainModules,
          piText,
          completionPackets: await native.countRows("completion_packets"),
        },
        null,
        2,
      ),
    );
  } finally {
    await native.shutdownEngine({ engine, drainTimeoutMs: 1_000 });
  }
} finally {
  rmSync(root, { recursive: true, force: true });
}

async function wakeSession(
  brain: BrainImplementationHandle | undefined,
  sessionId: SessionId,
  wakeId: string,
) {
  assert.ok(brain, `${String(sessionId)} brain should be registered`);
  const request = await native.buildBrainWakeRequestForSession({
    brain,
    sessionId,
    systemPrompt: "Test module selection.",
    roleAssemblyJson: encoder.encode(
      JSON.stringify({ instructions: "Reply once." }),
    ),
    wakeId,
  });
  return native.wakeBrain(request);
}

function brainModuleDiagnostics(
  runtimeConfig: Awaited<ReturnType<typeof loadRustyCrewRuntimeConfig>>,
  applyResult: Awaited<ReturnType<typeof applyRustyCrewRuntimeConfig>>,
): RuntimeBrainModuleDiagnostics[] {
  return runtimeConfig.brains.map((brain) => {
    const selection = applyResult.brainModulesByProfileId[brain.profileId];
    return {
      profileId: brain.profileId,
      implementationId: brain.implementationId,
      moduleId: selection?.moduleId ?? "unknown",
      ...(selection?.strategy === undefined
        ? {}
        : { strategy: selection.strategy }),
      effectiveStrategy:
        applyResult.brainDiagnosticsByProfileId[brain.profileId]
          ?.effectiveStrategy,
      providerStateMode:
        applyResult.brainDiagnosticsByProfileId[brain.profileId]
          ?.providerStateMode,
      providerStateRebuild:
        applyResult.brainDiagnosticsByProfileId[brain.profileId]
          ?.providerStateRebuild,
      strategyDiagnostics:
        applyResult.brainDiagnosticsByProfileId[brain.profileId]
          ?.strategyDiagnostics,
      selectedToolCount:
        applyResult.brainDiagnosticsByProfileId[brain.profileId]
          ?.selectedToolCount ?? 0,
      selectedToolSource:
        applyResult.brainDiagnosticsByProfileId[brain.profileId]
          ?.selectedToolSource ?? "unknown",
      toolAdapterStatus:
        applyResult.brainDiagnosticsByProfileId[brain.profileId]
          ?.toolAdapterStatus ?? "unknown",
    };
  });
}

function writeRuntimeConfig(dataDir: string): void {
  const configDir = join(dataDir, "config");
  const profilesDir = join(configDir, "profiles");
  mkdirSync(profilesDir, { recursive: true });
  writeFileSync(
    join(configDir, "service.json"),
    JSON.stringify(
      {
        profilesDir,
        brains: [
          { profileId: "pi-profile", implementationId: "pi-brain" },
          { profileId: "local-profile", implementationId: "local-brain" },
          {
            profileId: "responses-profile",
            implementationId: "responses-brain",
          },
          {
            profileId: "responses-chain-profile",
            implementationId: "responses-chain-brain",
          },
        ],
        sessions: [
          {
            sessionId: "pi-session",
            agentId: "pi-agent",
            profileId: "pi-profile",
            kind: "full",
          },
          {
            sessionId: "local-session",
            agentId: "local-agent",
            profileId: "local-profile",
            kind: "full",
          },
          {
            sessionId: "responses-session",
            agentId: "responses-agent",
            profileId: "responses-profile",
            kind: "full",
          },
          {
            sessionId: "responses-chain-session",
            agentId: "responses-chain-agent",
            profileId: "responses-chain-profile",
            kind: "full",
          },
        ],
      },
      null,
      2,
    ),
  );
  writeFileSync(
    join(profilesDir, "pi-profile.json"),
    JSON.stringify(
      {
        profileId: "pi-profile",
        modelConfig: {
          provider: "den-router",
          modelName: "fake-model",
          maxOutputTokens: 256,
        },
        brain: {
          module: "pi-agent-core",
          strategy: "default",
        },
        toolPolicy: {
          requestedTools: ["git_status"],
        },
      },
      null,
      2,
    ),
  );
  writeFileSync(
    join(profilesDir, "local-profile.json"),
    JSON.stringify(
      {
        profileId: "local-profile",
        modelConfig: {
          provider: "local",
          modelName: "deterministic",
        },
        brain: {
          module: "local",
        },
      },
      null,
      2,
    ),
  );
  writeFileSync(
    join(profilesDir, "responses-profile.json"),
    JSON.stringify(
      {
        profileId: "responses-profile",
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
      null,
      2,
    ),
  );
  writeFileSync(
    join(profilesDir, "responses-chain-profile.json"),
    JSON.stringify(
      {
        profileId: "responses-chain-profile",
        modelConfig: {
          provider: "openai",
          modelName: "gpt-5",
          api: "responses",
        },
        brain: {
          module: "openai-responses",
          strategy: "previous-response-chain",
        },
      },
      null,
      2,
    ),
  );
}
