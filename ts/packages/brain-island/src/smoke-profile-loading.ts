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
