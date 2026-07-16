import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import type { SessionId } from "@rusty-crew/contracts";
import { loadNativeBridge } from "@rusty-crew/native-bridge";

import type { AdminControlCommand } from "../src/admin-control-api.js";
import {
  createServiceProfile,
  type ServiceProfileAdminMutationContext,
} from "../src/service-profile-admin-mutations.js";
import type {
  RustyCrewRuntimeConfig,
  RustyCrewRuntimeConfigApplyResult,
} from "../src/service-runtime-config.js";

test("profile create persists exact prompts through the public service mutation", async () => {
  const dataDir = mkdtempSync(join(tmpdir(), "rusty-crew-profile-create-"));
  const profilesDir = join(dataDir, "profiles");
  const serviceConfigFile = join(dataDir, "service.json");
  const bridge = await loadNativeBridge();
  const engine = await bridge.initializeEngine({
    engineDataDir: dataDir,
    clock: "system",
    defaultTurnBudget: 16,
    defaultIdleTimeoutMs: 30_000,
    storage: { backend: "sqlite" },
  });

  try {
    const now = "2026-07-16T05:00:00.000Z";
    await bridge.upsertModelProvider({
      alias: "default",
      status: "active",
      protocol: "chat_completions",
      providerKind: "openai_compatible",
      modelId: "profile-create-regression",
      clearSecret: false,
      metadataJson: {},
      now,
    });

    const runtimeConfig: RustyCrewRuntimeConfig = {
      profilesDir,
      brains: [],
      sessions: [],
      scheduledJobs: [],
      channelBindings: [],
      mcpBindings: [],
    };
    await writeFile(
      serviceConfigFile,
      `${JSON.stringify(runtimeConfig, null, 2)}\n`,
    );
    const context: ServiceProfileAdminMutationContext = {
      bridge,
      runtimeConfig,
      serviceConfigFile,
      now: () => now,
      inFlightWakes: new Set<SessionId>(),
      applyRuntimeConfigFromDisk: async () => emptyApplyResult(),
      archiveSession: async () => undefined,
      forgetPurgedSessions: () => undefined,
    };
    const soulMarkdown =
      "# Exact soul\n\n  Preserve leading spaces.\n\nPreserve final newline.\n";
    const memoryMarkdown = "# Exact memory\n\n- one\n- two\n";

    await createServiceProfile(
      context,
      createProfileCommand("prompt-round-trip", {
        soulMarkdown,
        memoryMarkdown,
      }),
    );
    const persisted =
      await bridge.getProfileRegistryRecord("prompt-round-trip");
    assert.ok(persisted);
    assert.equal(persisted.promptSoulMarkdown, soulMarkdown);
    assert.equal(persisted.promptMemoryMarkdown, memoryMarkdown);

    await createServiceProfile(
      context,
      createProfileCommand("prompt-omitted", {}),
    );
    const omitted = await bridge.getProfileRegistryRecord("prompt-omitted");
    assert.ok(omitted);
    assert.equal(omitted.promptSoulMarkdown, undefined);
    assert.equal(omitted.promptMemoryMarkdown, undefined);
  } finally {
    await bridge.shutdownEngine({ engine, drainTimeoutMs: 5_000 });
    rmSync(dataDir, { recursive: true, force: true });
  }
});

function createProfileCommand(
  profileId: string,
  prompts: { soulMarkdown?: string; memoryMarkdown?: string },
): AdminControlCommand {
  return {
    name: "create_profile",
    target: {},
    actor: { operatorId: "profile-create-regression" },
    requestId: `request-${profileId}`,
    body: {
      profileId,
      providerAlias: "default",
      ...prompts,
    },
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
