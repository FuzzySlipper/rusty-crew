import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { loadNativeBridge } from "@rusty-crew/native-bridge";
import {
  buildRuntimeDiagnosticsProjection,
  type RuntimeBrainModuleDiagnostics,
} from "../src/runtime-diagnostics.js";
import { loadRustyCrewServiceConfig } from "../src/service-config.js";
import {
  applyRustyCrewRuntimeConfig,
  loadRustyCrewRuntimeConfig,
} from "../src/service-runtime-config.js";
import { handleAdminBrainCatalogRequest } from "../src/service-brain-catalog-routes.js";
import { resolveBrainCatalogSelection } from "../src/brain-catalog.js";

const root = mkdtempSync(join(tmpdir(), "rusty-crew-brain-catalog-"));
const native = await loadNativeBridge();
process.env.RUSTY_CREW_TEST_OPENAI_API_KEY = "test-only";

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
    const catalog = await native.brainCatalog();
    assert.deepEqual(
      catalog.modules.map((module) => module.module_id),
      ["pi-agent", "openai-responses"],
    );
    await assert.rejects(
      () =>
        resolveBrainCatalogSelection(
          native,
          {
            modelConfig: {
              provider: "test",
              modelName: "test",
              api: "responses",
            },
          },
          {
            registrationId: "rusty-crew-ts-host",
            capabilities: ["execute_tool"],
          },
        ),
      /unregistered host capabilities/,
    );
    assert.equal(
      catalog.modules.some((module) => module.module_id === "local"),
      false,
    );
    const catalogRoute = await handleAdminBrainCatalogRequest(
      { method: "GET", requestId: "brain-catalog-smoke" },
      native,
    );
    assert.equal(catalogRoute.status, 200);
    assert.deepEqual(
      (catalogRoute.body as { data: typeof catalog }).data.modules.map(
        (module) => module.module_id,
      ),
      ["pi-agent", "openai-responses"],
    );
    const applyResult = await applyRustyCrewRuntimeConfig({
      serviceConfig,
      runtimeConfig,
      bridge: native,
    });

    assert.equal(
      applyResult.brainModulesByProfileId["pi-profile"]?.moduleId,
      "pi-agent",
    );
    assert.equal(
      applyResult.brainModulesByProfileId["responses-profile"]?.moduleId,
      "openai-responses",
    );
    assert.equal(
      applyResult.brainModulesByProfileId["narrator-profile"]?.moduleId,
      "pi-agent",
    );
    assert.equal(
      applyResult.brainModulesByProfileId["narrator-profile"]?.strategy,
      "roleplay_narrator",
    );
    assert.equal(
      applyResult.brainDiagnosticsByProfileId["pi-profile"]?.toolAdapterStatus,
      "native_neutral_tools",
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
          "pi-agent",
          "default",
          "default",
          "unused",
          "default-local-tools",
          "native_neutral_tools",
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
          "replay",
          "optional",
          "default-local-tools",
          "native_neutral_tools",
        ],
        [
          "narrator-profile",
          "narrator-brain",
          "pi-agent",
          "roleplay_narrator",
          "roleplay_narrator",
          "unused",
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

    console.log(
      JSON.stringify(
        {
          modules: diagnostics.runtime.brainModules,
          rustCatalogModules: catalog.modules.map((module) => module.module_id),
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
          {
            profileId: "responses-profile",
            implementationId: "responses-brain",
          },
          {
            profileId: "responses-chain-profile",
            implementationId: "responses-chain-brain",
          },
          {
            profileId: "narrator-profile",
            implementationId: "narrator-brain",
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
          {
            sessionId: "narrator-session",
            agentId: "narrator-agent",
            profileId: "narrator-profile",
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
          baseUrl: "http://127.0.0.1:1",
          maxOutputTokens: 256,
        },
        brain: {
          module: "pi-agent",
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
    join(profilesDir, "responses-profile.json"),
    JSON.stringify(
      {
        profileId: "responses-profile",
        modelConfig: {
          provider: "openai",
          modelName: "gpt-5",
          baseUrl: "http://127.0.0.1:1",
          apiKeyEnv: "RUSTY_CREW_TEST_OPENAI_API_KEY",
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
          baseUrl: "http://127.0.0.1:1",
          apiKeyEnv: "RUSTY_CREW_TEST_OPENAI_API_KEY",
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
  writeFileSync(
    join(profilesDir, "narrator-profile.json"),
    JSON.stringify(
      {
        profileId: "narrator-profile",
        modelConfig: {
          provider: "den-router",
          modelName: "fake-model",
          baseUrl: "http://127.0.0.1:1",
          maxOutputTokens: 256,
        },
        brain: {
          module: "pi-agent",
          strategy: "roleplay_narrator",
        },
        toolPolicy: {
          requestedTools: [],
        },
        roleplayNarrator: {
          tone: "wry",
          pacing: "balanced",
          explicitness: "romantic",
          memoryDepth: "medium",
          review: {
            enabled: false,
            maxReviewCycles: 1,
            checkGravityDrift: true,
            checkCharacterVoice: true,
            checkContinuity: true,
          },
        },
      },
      null,
      2,
    ),
  );
}
