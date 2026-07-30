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

test("runtime config effects rebuild the active brain when the plan requires it", async () => {
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
        providerAlias: "provider-next",
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
      },
    } as unknown as ProfileRegistryRuntimeConfigPlan;

    const result = await applyProfileRegistryRuntimeConfigEffects(
      context,
      record,
      plan,
    );

    assert.deepEqual(calls, ["apply", "rebuild:profile-alpha"]);
    assert.equal(result.brainRebuilt, true);
    const saved = JSON.parse(await readFile(profilePath, "utf8")) as {
      providerAlias?: string;
      contextPolicy?: { strategyId?: string };
    };
    assert.equal(saved.providerAlias, "provider-next");
    assert.equal(saved.contextPolicy?.strategyId, "rolling_summary_compaction");
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
