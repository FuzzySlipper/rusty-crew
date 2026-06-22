import assert from "node:assert/strict";
import { mkdtempSync, rmSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { ProfileId } from "@rusty-crew/contracts";
import { loadProfileContext, ProfileLoadError } from "./index.js";

const root = mkdtempSync(join(tmpdir(), "rusty-crew-profile-loading-"));
const profilesDir = join(root, "profiles");
const skillsDir = join(root, "skills");
mkdirSync(profilesDir, { recursive: true });
mkdirSync(skillsDir, { recursive: true });

try {
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
        skills: ["repo-orientation"],
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
    context.profile.runtime?.defaultResourceLimits?.maxDelegationDepth,
    1,
  );
  assert.deepEqual(
    context.toolSelection.toolProfile.tools.map((tool) => tool.name),
    ["read_file", "search_files", "git_status", "git_diff"],
  );
  assert.equal(context.skills[0]?.title, "Repo Orientation");
  assert.deepEqual(context.skills[0]?.tags, ["repo", "architecture"]);
  assert.match(context.skills[0]?.bodyMarkdown ?? "", /Rusty Crew README/);

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
  api: openai-responses
  temperature: 0.2
  maxTokens: 4096
mcpConfig:
  toolProfile: runner
runtimeConfig:
  maxIterations: 100
  maxTokensPerTurn: 8192
  maxDurationMs: 900000
  maxTurnDurationMs: 180000
toolPolicy:
  mode: allow_all
memoryConfig:
  enabled: true
sessionDefaults:
  ownerId: "owner:den-k8plus:rusty-crew-runner"
  maxHistoryMessages: 200
  turnTimeoutMs: 1800000
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
  assert.equal(runner.profile.modelConfig.api, "openai-responses");
  assert.equal(runner.profile.modelConfig.temperatureMilli, 200);
  assert.equal(runner.profile.modelConfig.maxOutputTokens, 4096);
  assert.equal(runner.profile.runtime?.maxTurns, 100);
  assert.equal(
    runner.profile.runtime?.defaultResourceLimits?.maxDurationMs,
    900000,
  );
  assert.equal(runner.profile.runtime?.maxTokensPerTurn, 8192);
  assert.equal(runner.profile.runtime?.maxTurnDurationMs, 180000);
  assert.equal(runner.profile.mcpConfig?.toolProfile, "runner");
  assert.equal(runner.profile.memoryConfig?.enabled, true);
  assert.equal(
    runner.profile.sessionDefaults?.ownerId,
    "owner:den-k8plus:rusty-crew-runner",
  );
  assert.equal(runner.profile.sessionDefaults?.maxHistoryMessages, 200);
  assert.equal(runner.profile.sessionDefaults?.turnTimeoutMs, 1800000);
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
  rmSync(root, { recursive: true, force: true });
}
