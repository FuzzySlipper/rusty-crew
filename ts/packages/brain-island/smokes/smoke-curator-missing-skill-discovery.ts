import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import type { ProfileId } from "@rusty-crew/contracts";
import {
  discoverCuratorCandidates,
  loadProfileContext,
  loadProfileCuratorDiscoveryContext,
  ProfileLoadError,
} from "../src/index.js";

const root = mkdtempSync(join(tmpdir(), "rusty-crew-curator-missing-skill-"));
const profilesDir = join(root, "profiles");
const skillsDir = join(root, "skills");
mkdirSync(profilesDir, { recursive: true });
mkdirSync(skillsDir, { recursive: true });
const profileId = "curator-missing-skill" as ProfileId;
writeFileSync(
  join(profilesDir, `${profileId}.json`),
  JSON.stringify({
    profileId,
    providerAlias: "unused-by-curator-discovery",
    skills: ["healthy-skill", "missing-skill"],
  }),
);
writeFileSync(
  join(skillsDir, "healthy-skill.md"),
  [
    "---",
    "title: Healthy Skill",
    "summary: Evidence that valid skills continue through curator discovery.",
    "---",
    "",
    "TODO: replace temporary guidance with a stable workflow.",
  ].join("\n"),
);

const discovery = await loadProfileCuratorDiscoveryContext({
  profilesDir,
  skillsDir,
  profileId,
});
assert.deepEqual(
  discovery.skills.map((skill) => skill.slug),
  ["healthy-skill"],
);
assert.deepEqual(discovery.missingSkillSlugs, ["missing-skill"]);

const batch = discoverCuratorCandidates({
  batchId: "curator-missing-skill-discovery",
  now: "2026-07-11T00:00:00.000Z",
  scopeType: "profile",
  scopeId: profileId,
  profileId,
  skills: discovery.skills,
  expectedSkillSlugs: discovery.profile.skills,
});
const missing = batch.candidates.find(
  (candidate) => candidate.targetRef === "skill:missing-skill",
);
assert.equal(missing?.kind, "skill_create");
assert.equal(missing?.severity, "warning");
assert.ok(
  batch.candidates.some(
    (candidate) => candidate.targetRef === "skill:healthy-skill",
  ),
  "valid skill evidence should still be scanned",
);

await assert.rejects(
  loadProfileContext({
    profilesDir,
    skillsDir,
    profileId,
    modelProviderResolver: async () => ({
      provider: "test",
      modelName: "test",
    }),
  }),
  (error: unknown) =>
    error instanceof ProfileLoadError && error.code === "skill_not_found",
);

console.log(
  JSON.stringify({
    profileId,
    loadedSkills: discovery.skills.map((skill) => skill.slug),
    missingSkillSlugs: discovery.missingSkillSlugs,
    missingCandidateId: missing?.candidateId,
    ordinaryRuntimeLoadFailedClosed: true,
  }),
);
