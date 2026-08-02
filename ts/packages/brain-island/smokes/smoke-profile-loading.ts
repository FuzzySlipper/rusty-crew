import assert from "node:assert/strict";
import { mkdtempSync, rmSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { ProfileId } from "@rusty-crew/contracts";
import {
  coreConfigFacadeArtifact,
  loadNativeBridge,
} from "@rusty-crew/native-bridge";
import {
  loadProfileContext,
  profilePromptAssetConfigPaths,
  profileRuntimeGraphWireFieldPaths,
  ProfileLoadError,
  resolveBrainCatalogSelection,
} from "../src/index.js";

const root = mkdtempSync(join(tmpdir(), "rusty-crew-profile-loading-"));
const profilesDir = join(root, "profiles");
const skillsDir = join(root, "skills");
const native = await loadNativeBridge();
const engine = await native.initializeEngine({
  engineDataDir: join(root, "engine"),
  clock: { fixed: "2026-07-08T00:00:00Z" },
  defaultTurnBudget: 4,
  defaultIdleTimeoutMs: 1_000,
});
mkdirSync(profilesDir, { recursive: true });
mkdirSync(skillsDir, { recursive: true });

try {
  const runtimeValidationFields: ReadonlySet<string> = new Set(
    coreConfigFacadeArtifact.wire_field_inventory.RuntimeConfigValidationInput,
  );
  const profilePlanFields: ReadonlySet<string> = new Set(
    coreConfigFacadeArtifact.wire_field_inventory.CreateProfilePlanInput,
  );
  const missingRuntimeValidationFields =
    profileRuntimeGraphWireFieldPaths.filter(
      (field) => !runtimeValidationFields.has(field),
    );
  const missingProfilePlanFields = profileRuntimeGraphWireFieldPaths.filter(
    (field) => !profilePlanFields.has(field),
  );
  assert.deepEqual(missingRuntimeValidationFields, []);
  assert.deepEqual(missingProfilePlanFields, []);
  for (const assetField of profilePromptAssetConfigPaths) {
    assert.equal(runtimeValidationFields.has(assetField), false);
    assert.equal(profilePlanFields.has(assetField), false);
  }

  writeFileSync(
    join(profilesDir, "prime-coder.json"),
    JSON.stringify(
      {
        profileId: "prime-coder",
        displayName: "Prime Coder",
        modelConfig: {
          provider: "den-router",
          modelName: "local-deterministic",
          maxOutputTokens: 2048,
        },
        runtime: {
          maxTurns: 3,
          defaultResourceLimits: {
            workdir: "/home/dev/rusty-crew",
            maxDurationMs: 30_000,
            maxDelegationDepth: 1,
          },
        },
        toolPolicy: {
          requestedToolsets: ["local_code_read", "local_code_write"],
          deniedTools: ["terminal"],
        },
        prompt: {
          system: "You are a Rusty Crew prime coder.",
          instructions: [
            "Prefer direct work and bounded subagent delegation.",
            "Use selected local-code tools only.",
          ],
        },
        skills: ["repo-orientation", "rusty-crew"],
      },
      null,
      2,
    ),
  );
  writeFileSync(
    join(skillsDir, "repo-orientation.md"),
    `---
title: Repo Orientation
summary: Read the local architecture docs first.
tags:
  - repo
  - architecture
---

Start with the Rusty Crew README and tool registry note.
`,
  );
  writeFileSync(
    join(skillsDir, "rusty-crew.md"),
    "This filesystem collision must never enter profile prompt assembly.",
  );
  mkdirSync(join(skillsDir, "autonomous-ai-agents", "codex"), {
    recursive: true,
  });
  writeFileSync(
    join(skillsDir, "autonomous-ai-agents", "codex", "SKILL.md"),
    `---
name: codex
description: Delegate coding work through Codex CLI.
tags:
  - coding
  - delegation
---

Use Codex for bounded coding delegation when context isolation helps.
`,
  );
  writeFileSync(
    join(profilesDir, "nested-skill-profile.json"),
    JSON.stringify(
      {
        profileId: "nested-skill-profile",
        modelConfig: {
          provider: "den-router",
          modelName: "local-deterministic",
        },
        skills: ["codex"],
      },
      null,
      2,
    ),
  );
  writeFileSync(
    join(profilesDir, "memory-profile.json"),
    JSON.stringify(
      {
        profileId: "memory-profile",
        modelConfig: {
          provider: "den-router",
          modelName: "local-deterministic",
        },
        toolPolicy: {
          requestedTools: [
            "memory_recall",
            "memory_read",
            "memory_search",
            "memory_store",
            "memory_propose",
          ],
        },
      },
      null,
      2,
    ),
  );

  const context = await loadProfileContext({
    profilesDir,
    skillsDir,
    profileId: "prime-coder" as ProfileId,
    session: {
      readOnly: true,
    },
  });

  assert.equal(context.profile.displayName, "Prime Coder");
  assert.equal(context.profile.modelConfig.provider, "den-router");
  assert.equal(
    (await resolveBrainCatalogSelection(native, context.profile)).selection
      .moduleId,
    "chat-completions",
  );
  assert.equal(
    context.profile.runtime?.defaultResourceLimits?.maxDelegationDepth,
    1,
  );
  assert.deepEqual(
    context.toolSelection.toolProfile.tools.map((tool) => tool.name),
    ["read_file", "search_files", "git_status", "git_diff", "rusty_crew_help"],
  );
  assert.deepEqual(
    context.skills.map((skill) => skill.slug),
    ["repo-orientation"],
  );
  assert.equal(context.skills[0]?.title, "Repo Orientation");
  assert.deepEqual(context.skills[0]?.tags, ["repo", "architecture"]);
  assert.match(context.skills[0]?.bodyMarkdown ?? "", /Rusty Crew README/);

  const missingMemory = await loadProfileContext({
    profilesDir,
    skillsDir,
    profileId: "memory-profile" as ProfileId,
    toolAvailabilityPlanner: (request) => native.planToolAvailability(request),
    externalMemoryAvailability: {
      configured: false,
      clientAvailable: false,
      mode: "metadata",
      lastError: "den memory baseUrl is not configured",
    },
  });
  assert.deepEqual(
    missingMemory.toolSelection.toolProfile.tools.map((tool) => tool.name),
    ["rusty_crew_help"],
  );
  assert.equal(
    missingMemory.toolSelection.inventory.items.find(
      (item) => item.name === "memory_search",
    )?.status,
    "resource_denied",
  );
  assert.match(
    missingMemory.toolSelection.inventory.items.find(
      (item) => item.name === "memory_search",
    )?.reasons[0] ?? "",
    /memory_external_dependency_missing/,
  );

  const metadataMemory = await loadProfileContext({
    profilesDir,
    skillsDir,
    profileId: "memory-profile" as ProfileId,
    toolAvailabilityPlanner: (request) => native.planToolAvailability(request),
    externalMemoryAvailability: {
      configured: true,
      clientAvailable: true,
      mode: "metadata",
    },
  });
  assert.deepEqual(
    metadataMemory.toolSelection.toolProfile.tools.map((tool) => tool.name),
    ["memory_recall", "memory_read", "memory_search", "rusty_crew_help"],
  );
  assert.equal(
    metadataMemory.toolSelection.inventory.items.find(
      (item) => item.name === "memory_store",
    )?.status,
    "resource_denied",
  );

  const nestedSkill = await loadProfileContext({
    profilesDir,
    skillsDir,
    profileId: "nested-skill-profile" as ProfileId,
  });
  assert.equal(nestedSkill.skills[0]?.slug, "codex");
  assert.equal(nestedSkill.skills[0]?.title, "codex");
  assert.equal(
    nestedSkill.skills[0]?.summary,
    "Delegate coding work through Codex CLI.",
  );
  assert.match(nestedSkill.skills[0]?.bodyMarkdown ?? "", /bounded coding/);

  const runnerDir = join(profilesDir, "rusty-crew-runner");
  mkdirSync(runnerDir, { recursive: true });
  writeFileSync(
    join(runnerDir, "profile.yaml"),
    `name: "Rusty Crew Runner"
displayName: "Rusty Crew Runner"
profileIdentity: rusty-crew-runner
skills: []
modelConfig:
  provider: den-router
  model: deepseek-flash
  baseUrl: http://127.0.0.1:18082/v1
  apiKeyEnv: DEN_ROUTER_API_KEY
  api: openai-completions
  temperature: 0.2
  maxTokens: 4096
brain:
  module: chat-completions-core
  strategy: default
mcpConfig:
  toolProfile: runner
runtimeConfig:
  maxIterations: 100
  maxTokensPerTurn: 8192
  maxDurationMs: 900000
toolPolicy:
  mode: allow_all
memoryConfig:
  enabled: true
sessionDefaults:
  ownerId: "owner:den-k8plus:rusty-crew-runner"
  maxHistoryMessages: 200
channelDefaults:
  wakePolicy: subscription
backgroundReview:
  enabled: true
  reviewType: combined
  schedule: "0 3 * * *"
  memoryNudgeInterval: 2
  skillNudgeInterval: 2
  maxTokens: 5000
  maxFindings: 12
  maxCandidates: 50
`,
  );
  writeFileSync(
    join(runnerDir, "soul.md"),
    "You are Rusty Crew Runner.\n\nHandle implementation work.",
  );
  writeFileSync(join(runnerDir, "memory.md"), "Piper is the project lead.");
  const runner = await loadProfileContext({
    profilesDir,
    profileId: "rusty-crew-runner" as ProfileId,
  });

  assert.equal(runner.profile.displayName, "Rusty Crew Runner");
  assert.equal(runner.profile.modelConfig.modelName, "deepseek-flash");
  assert.equal(runner.profile.modelConfig.baseUrl, "http://127.0.0.1:18082/v1");
  assert.equal(runner.profile.modelConfig.apiKeyEnv, "DEN_ROUTER_API_KEY");
  assert.equal(runner.profile.modelConfig.api, "openai-completions");
  assert.equal(runner.profile.brain?.module, "chat-completions-core");
  assert.equal(runner.profile.brain?.strategy, "default");
  assert.equal(runner.profile.modelConfig.temperatureMilli, 200);
  assert.equal(runner.profile.modelConfig.maxOutputTokens, 4096);
  assert.equal(runner.profile.runtime?.maxTurns, 100);
  assert.equal(
    runner.profile.runtime?.defaultResourceLimits?.maxDurationMs,
    900000,
  );
  assert.equal(runner.profile.runtime?.maxTokensPerTurn, 8192);
  assert.equal(runner.profile.mcpConfig?.toolProfile, "runner");
  assert.equal(runner.profile.memoryConfig?.enabled, true);
  assert.equal(
    runner.profile.sessionDefaults?.ownerId,
    "owner:den-k8plus:rusty-crew-runner",
  );
  assert.equal(runner.profile.sessionDefaults?.maxHistoryMessages, 200);
  assert.equal(runner.profile.channelDefaults?.wakePolicy, "subscription");
  assert.equal(runner.profile.backgroundReview?.enabled, true);
  assert.equal(runner.profile.backgroundReview?.reviewType, "combined");
  assert.equal(runner.profile.backgroundReview?.schedule, "0 3 * * *");
  assert.equal(runner.profile.backgroundReview?.memoryNudgeInterval, 2);
  assert.equal(runner.profile.backgroundReview?.skillNudgeInterval, 2);
  assert.equal(runner.profile.backgroundReview?.maxTokens, 5000);
  assert.equal(runner.profile.backgroundReview?.maxFindings, 12);
  assert.equal(runner.profile.backgroundReview?.maxCandidates, 50);
  assert.match(
    runner.profile.prompt?.soulMarkdown ?? "",
    /implementation work/,
  );
  assert.match(runner.profile.prompt?.memoryMarkdown ?? "", /Piper/);
  assert.equal(
    runner.toolSelection.toolProfile.tools.some(
      (tool) => tool.name === "git_status",
    ),
    true,
  );
  assert.equal(
    runner.toolSelection.toolProfile.tools.some(
      (tool) => tool.name === "skill_manage",
    ),
    true,
  );

  writeFileSync(
    join(profilesDir, "roleplay-narrator.json"),
    JSON.stringify(
      {
        profileId: "roleplay-narrator",
        providerAlias: "deepseek_flash",
        brain: {
          module: "chat-completions",
          strategy: "roleplay_narrator",
        },
        toolPolicy: {
          requestedToolsets: [
            "roleplay_lore_read",
            "roleplay_lore_write",
            "roleplay_scene_state",
          ],
        },
        roleplayNarrator: {
          tone: "dramatic",
          explicitness: "romantic",
          pacing: "leisurely",
          memoryDepth: "deep",
          stylePrompt:
            "Use clipped sensory beats before major emotional turns.",
          exemplar: "The rain softened every edge of the room.",
          review: {
            enabled: true,
            maxReviewCycles: 2,
            checkGravityDrift: false,
          },
        },
      },
      null,
      2,
    ),
  );
  const narrator = await loadProfileContext({
    profilesDir,
    profileId: "roleplay-narrator" as ProfileId,
    modelProviderResolver: async () => ({
      provider: "den-router",
      modelName: "deepseek-flash",
      temperatureMilli: 700,
    }),
  });
  assert.equal(narrator.profile.brain?.strategy, "roleplay_narrator");
  assert.equal(narrator.profile.roleplayNarrator?.tone, "dramatic");
  assert.equal(narrator.profile.roleplayNarrator?.explicitness, "romantic");
  assert.equal(narrator.profile.roleplayNarrator?.pacing, "leisurely");
  assert.equal(narrator.profile.roleplayNarrator?.memoryDepth, "deep");
  assert.equal(
    narrator.profile.roleplayNarrator?.stylePrompt,
    "Use clipped sensory beats before major emotional turns.",
  );
  assert.equal(
    narrator.profile.roleplayNarrator?.exemplar,
    "The rain softened every edge of the room.",
  );
  assert.equal(narrator.profile.roleplayNarrator?.review.enabled, true);
  assert.equal(narrator.profile.roleplayNarrator?.review.maxReviewCycles, 2);
  assert.equal(
    narrator.profile.roleplayNarrator?.review.checkGravityDrift,
    false,
  );
  assert.deepEqual(
    narrator.toolSelection.toolProfile.tools.map((tool) => tool.name).sort(),
    [
      "capture_lore_fact",
      "get_lore_layer_config",
      "get_scene_state",
      "list_lore_layers",
      "promote_lore_entry",
      "recall_lore",
      "rusty_crew_help",
      "search_lore",
      "update_scene_state",
    ],
  );

  const skillAllDir = join(profilesDir, "skill-all-profile");
  mkdirSync(join(skillAllDir, "skills", "local-skill-smoke"), {
    recursive: true,
  });
  writeFileSync(
    join(skillAllDir, "profile.yaml"),
    `name: "Skill All Profile"
profileIdentity: skill-all-profile
skills: all
modelConfig:
  provider: den-router
  model: local-deterministic
`,
  );
  writeFileSync(
    join(skillAllDir, "skills", "local-skill-smoke", "SKILL.md"),
    `---
name: local-skill-smoke
description: Profile-local smoke skill.
---

Use the profile-local skill source.
`,
  );
  const skillAll = await loadProfileContext({
    profilesDir,
    skillsDir,
    profileId: "skill-all-profile" as ProfileId,
  });
  assert.deepEqual(skillAll.skills.map((skill) => skill.slug).sort(), [
    "codex",
    "local-skill-smoke",
    "repo-orientation",
  ]);
  assert.match(
    skillAll.skills.find((skill) => skill.slug === "local-skill-smoke")
      ?.bodyMarkdown ?? "",
    /profile-local/,
  );

  await assert.rejects(
    () =>
      loadProfileContext({
        profilesDir,
        skillsDir,
        profileId: "missing-profile" as ProfileId,
      }),
    (error) =>
      error instanceof ProfileLoadError && error.code === "profile_not_found",
  );

  writeFileSync(
    join(profilesDir, "bad-profile.json"),
    JSON.stringify({ profileId: "bad-profile" }),
  );
  await assert.rejects(
    () =>
      loadProfileContext({
        profilesDir,
        skillsDir,
        profileId: "bad-profile" as ProfileId,
      }),
    (error) =>
      error instanceof ProfileLoadError &&
      error.code === "invalid_profile_config",
  );

  writeFileSync(
    join(profilesDir, "bad-wake-policy.json"),
    JSON.stringify({
      profileId: "bad-wake-policy",
      modelConfig: {
        provider: "den-router",
        modelName: "local-deterministic",
      },
      channelDefaults: {
        wakePolicy: "sometimes",
      },
    }),
  );
  await assert.rejects(
    () =>
      loadProfileContext({
        profilesDir,
        skillsDir,
        profileId: "bad-wake-policy" as ProfileId,
      }),
    (error) =>
      error instanceof ProfileLoadError &&
      error.code === "invalid_profile_config" &&
      /channelDefaults\.wakePolicy/.test(error.message),
  );

  writeFileSync(
    join(profilesDir, "bad-context-policy.json"),
    JSON.stringify({
      profileId: "bad-context-policy",
      modelConfig: {
        provider: "den-router",
        modelName: "local-deterministic",
      },
      contextPolicy: {
        strategyId: "mystery_strategy",
      },
    }),
  );
  await assert.rejects(
    () =>
      loadProfileContext({
        profilesDir,
        skillsDir,
        profileId: "bad-context-policy" as ProfileId,
      }),
    (error) =>
      error instanceof ProfileLoadError &&
      error.code === "invalid_profile_config" &&
      /unknown context strategy mystery_strategy/.test(error.message),
  );

  console.log(
    JSON.stringify(
      {
        profileId: context.profile.profileId,
        skills: context.skills.map((skill) => skill.slug),
        selectedTools: context.toolSelection.toolProfile.tools.map(
          (tool) => tool.name,
        ),
      },
      null,
      2,
    ),
  );
} finally {
  await native.shutdownEngine({ engine, drainTimeoutMs: 1_000 });
  rmSync(root, { recursive: true, force: true });
}
