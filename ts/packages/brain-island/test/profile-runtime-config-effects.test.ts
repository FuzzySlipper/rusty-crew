import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import type { NativeProfileRegistryRecord } from "@rusty-crew/native-bridge";
import {
  applyProfileRegistryRuntimeConfigEffects,
  type ProfileRegistryRuntimeConfigMutationContext,
  type ProfileRegistryRuntimeConfigPlan,
} from "../src/service-profile-runtime-mutations.js";
import type {
  RustyCrewRuntimeConfig,
  RustyCrewRuntimeConfigApplyResult,
} from "../src/service-runtime-config.js";

test("runtime config effects refresh repaired MCP bindings and rebuild the active brain", async () => {
  const root = await mkdtemp(join(tmpdir(), "rusty-crew-profile-runtime-"));
  const profilesDir = join(root, "profiles");
  const profilePath = join(profilesDir, "profile-alpha.json");
  const serviceConfigFile = join(root, "service.json");
  const calls: string[] = [];
  try {
    await writeFile(
      serviceConfigFile,
      `${JSON.stringify({ profilesDir, mcpBindings: [] }, null, 2)}\n`,
    );
    const runtimeConfig = {
      profilesDir,
      brains: [],
      sessions: [],
      scheduledJobs: [],
      channelBindings: [],
      mcpBindings: [],
    } satisfies RustyCrewRuntimeConfig;
    const context = {
      runtimeConfig,
      serviceConfigFile,
      now: () => "2026-07-30T00:00:00.000Z",
      async applyRuntimeConfigFromDisk() {
        calls.push("apply");
        return emptyApplyResult();
      },
      async rebuildBrainRuntime(profileId: string) {
        calls.push(`rebuild:${profileId}`);
      },
      async reconcileProfileMcpBindings() {
        return {
          desiredCount: 1,
          activeSessionCount: 1,
          materializedCount: 1,
          removedBindingIds: [],
          changed: true,
          diagnostics: [],
        };
      },
      async refreshExternalProfileMcpTools() {
        calls.push("refresh:profile-alpha");
        return { refreshed: [] };
      },
    } as unknown as ProfileRegistryRuntimeConfigMutationContext;
    const record = profileRecord();
    const plan = {
      ok: true,
      profileId: record.profileId,
      mode: "apply",
      expectedRevision: record.revision,
      current: record,
      next: record,
      nextWrite: {} as never,
      runtimeConfig: {
        modelConfigId: "model-next",
        externalMessageDeliveryPolicy: "serial_next_turn",
        brain: { module: "chat-completions" },
        contextPolicy: {
          enabled: true,
          strategyId: "rolling_summary_compaction",
          autoCompactionEnabled: true,
          compactAtPercent: 20,
          targetPercentAfterCompaction: 10,
          maxContextPercentForWake: 95,
          debugVisibility: "status",
          includeDebugEventsInModelContext: false,
          strategyConfig: {},
        },
        mcpBindings: [],
      },
      diagnostics: [],
      implications: {
        registryRevisionWillIncrement: true,
        profileFileWillChange: true,
        serviceConfigWillChange: false,
        configReloadRequired: true,
        runtimeRebuildRecommended: true,
        mcpRefreshRecommended: false,
        externalBindingRebuildRecommended: true,
      },
    } as unknown as ProfileRegistryRuntimeConfigPlan;

    const result = await applyProfileRegistryRuntimeConfigEffects(
      context,
      record,
      plan,
    );

    assert.deepEqual(calls, [
      "apply",
      "refresh:profile-alpha",
      "rebuild:profile-alpha",
    ]);
    assert.equal(result.brainRebuilt, true);
    const saved = JSON.parse(await readFile(profilePath, "utf8")) as {
      modelConfigId?: string;
      externalMessageDeliveryPolicy?: string;
      contextPolicy?: { strategyId?: string };
    };
    assert.equal(saved.modelConfigId, "model-next");
    assert.equal(saved.externalMessageDeliveryPolicy, "serial_next_turn");
    assert.equal(saved.contextPolicy?.strategyId, "rolling_summary_compaction");
    assert.equal(result.externalBindingRebuildRecommended, true);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

function profileRecord(): NativeProfileRegistryRecord {
  return {
    profileId: "profile-alpha",
    lifecycleStatus: "active",
    displayName: "Profile Alpha",
    defaultSessionKind: "full",
    agentId: "profile-alpha-agent",
    activeRuntimeSettingsJson: {},
    sourceAssetRefs: [],
    derivedRuntimeRefs: [],
    importExport: { metadataJson: {} },
    revision: 2,
    createdAt: "2026-07-30T00:00:00.000Z",
    updatedAt: "2026-07-30T00:00:00.000Z",
  };
}

function emptyApplyResult(): RustyCrewRuntimeConfigApplyResult {
  return {
    brainsRegistered: 0,
    brainsAlreadyPresent: 0,
    sessionsCreated: 0,
    sessionsAlreadyPresent: 0,
    sessionsReactivated: 0,
    sessionsMissing: 0,
    scheduledJobsRegistered: 0,
    brainHandlesByProfileId: {},
    brainModulesByProfileId: {},
    brainDiagnosticsByProfileId: {},
  };
}
