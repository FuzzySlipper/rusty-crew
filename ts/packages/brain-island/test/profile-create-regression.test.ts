import assert from "node:assert/strict";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import type {
  AgentId,
  BrainImplementationId,
  ProfileId,
  SessionId,
} from "@rusty-crew/contracts";
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

test("profile create rolls back files, registry, and runtime after late activation failure", async () => {
  const dataDir = mkdtempSync(join(tmpdir(), "rusty-crew-profile-rollback-"));
  const profilesDir = join(dataDir, "profiles");
  const serviceConfigFile = join(dataDir, "service.json");
  const profileId = "activation-rollback";
  const profilePath = join(profilesDir, `${profileId}.json`);
  const bridge = await loadNativeBridge();
  const engine = await bridge.initializeEngine({
    engineDataDir: dataDir,
    clock: "system",
    defaultTurnBudget: 16,
    defaultIdleTimeoutMs: 30_000,
    storage: { backend: "sqlite" },
  });

  try {
    const now = "2026-07-16T05:30:00.000Z";
    await bridge.upsertModelProvider({
      alias: "missing-base-url",
      status: "active",
      protocol: "chat_completions",
      providerKind: "openai_compatible",
      modelId: "profile-create-rollback",
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
    const originalRuntimeConfig = `${JSON.stringify(runtimeConfig, null, 2)}\n`;
    await writeFile(serviceConfigFile, originalRuntimeConfig);

    let applyCount = 0;
    let createdImplementationId: BrainImplementationId | undefined;
    const forgottenSessionIds: string[] = [];
    const context: ServiceProfileAdminMutationContext = {
      bridge,
      runtimeConfig,
      serviceConfigFile,
      now: () => now,
      inFlightWakes: new Set<SessionId>(),
      applyRuntimeConfigFromDisk: async () => {
        applyCount += 1;
        if (applyCount > 1) return emptyApplyResult();

        const staged = JSON.parse(
          await readFile(serviceConfigFile, "utf8"),
        ) as Record<string, unknown>;
        const brain = requiredRecordArrayItem(staged, "brains");
        const session = requiredRecordArrayItem(staged, "sessions");
        assert.equal(existsSync(profilePath), true);
        assert.ok(await bridge.getProfileRegistryRecord(profileId));

        createdImplementationId = String(
          brain.implementationId,
        ) as BrainImplementationId;
        await bridge.registerBrainImplementation({
          implementationId: createdImplementationId,
          profileId: profileId as ProfileId,
          toolProfile: { tools: [] },
          modelConfig: {
            provider: "openai_compatible",
            modelName: "profile-create-rollback",
          },
        });
        await bridge.ensureConfiguredSession({
          sessionId: String(session.sessionId) as SessionId,
          agentId: String(session.agentId) as AgentId,
          profileId: profileId as ProfileId,
          kind: "full",
        });
        throw new Error(
          "rust-chat-completions live client requires modelConfig.baseUrl",
        );
      },
      archiveSession: async () => undefined,
      forgetPurgedSessions: (sessionIds) => {
        forgottenSessionIds.push(...sessionIds);
      },
    };

    await assert.rejects(
      createServiceProfile(context, {
        ...createProfileCommand(profileId, {}),
        body: {
          profileId,
          providerAlias: "missing-base-url",
          workspaceCwd: "/home/dev/rusty-crew",
        },
      }),
      /rust-chat-completions live client requires modelConfig\.baseUrl/,
    );

    assert.equal(applyCount, 2);
    assert.equal(
      await readFile(serviceConfigFile, "utf8"),
      originalRuntimeConfig,
    );
    assert.equal(existsSync(profilePath), false);
    assert.equal(await bridge.getProfileRegistryRecord(profileId), undefined);
    assert.equal(
      (await bridge.listSessions()).some(
        (session) => String(session.profileId) === profileId,
      ),
      false,
    );
    assert.equal(forgottenSessionIds.length, 1);
    assert.ok(createdImplementationId);
    await assert.rejects(
      bridge.unregisterBrainImplementationForProfile(profileId as ProfileId),
      /not.?found/i,
    );
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
      workspaceCwd: "/home/dev/rusty-crew",
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

function requiredRecordArrayItem(
  value: Record<string, unknown>,
  key: string,
): Record<string, unknown> {
  const items = value[key];
  assert.ok(Array.isArray(items));
  const item = items[0];
  assert.equal(typeof item, "object");
  assert.notEqual(item, null);
  return item as Record<string, unknown>;
}
