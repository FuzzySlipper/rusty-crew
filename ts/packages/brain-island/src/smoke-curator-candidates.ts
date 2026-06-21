import { strict as assert } from "node:assert";
import {
  discoverCuratorCandidates,
  renderCuratorCandidateReport,
  type CuratorCandidateKind,
} from "./index.js";
import type { BackgroundReviewDenseMemoryRecord } from "./background-memory-skill-review.js";
import type { LoadedSkill } from "./profile-loading.js";

const skills: LoadedSkill[] = [
  {
    slug: "coding-a",
    title: "Coding",
    tags: ["code"],
    bodyMarkdown: "Temporary TODO progress should move to task tracking.",
    sourcePath: "/profiles/skills/coding-a/SKILL.md",
  },
  {
    slug: "coding-b",
    title: "Coding",
    summary: "Second coding skill.",
    tags: ["code"],
    bodyMarkdown: "Stable coding guidance.",
    sourcePath: "/profiles/skills/coding-b/SKILL.md",
  },
  {
    slug: "large-skill",
    title: "Large Skill",
    summary: "Oversized fixture.",
    tags: ["large"],
    bodyMarkdown: "x".repeat(33_000),
    sourcePath: "/profiles/skills/large-skill/SKILL.md",
  },
];

const denseProfileMemory: BackgroundReviewDenseMemoryRecord[] = [
  {
    profileId: "prime",
    key: "handoff",
    content: "Remember a temporary follow-up from an old task.",
    revision: 1,
  },
  {
    profileId: "prime",
    key: "handoff",
    content: "Duplicate stable handoff note.",
    revision: 2,
  },
];

const batch = discoverCuratorCandidates({
  batchId: "batch-1",
  now: "2026-06-21T00:00:00.000Z",
  scopeType: "profile",
  scopeId: "prime",
  profileId: "prime",
  skills,
  expectedSkillSlugs: ["coding-a", "missing-skill"],
  denseProfileMemory,
});

assert.equal(batch.batchId, "batch-1");
assert.equal(batch.dryRun, true);
assert.equal(batch.candidateCount, batch.candidates.length);
assert.ok(batch.reportId.startsWith("curator-report:batch-1:"));

const kinds = new Set<CuratorCandidateKind>(
  batch.candidates.map((candidate) => candidate.kind),
);
assert.ok(kinds.has("skill_patch"));
assert.ok(kinds.has("skill_create"));
assert.ok(kinds.has("diagnostics_only"));
assert.ok(kinds.has("dense_memory_prune"));
assert.ok(kinds.has("dense_memory_merge"));

const candidateIds = batch.candidates.map((candidate) => candidate.candidateId);
assert.equal(candidateIds.length, new Set(candidateIds).size);
assert.ok(candidateIds.every((id) => id.startsWith("curator:batch-1:")));
assert.ok(
  batch.candidates.every((candidate) => candidate.status === "proposed"),
);

const repeated = discoverCuratorCandidates({
  batchId: "batch-1",
  now: "2026-06-21T00:00:00.000Z",
  scopeType: "profile",
  scopeId: "prime",
  profileId: "prime",
  skills,
  expectedSkillSlugs: ["coding-a", "missing-skill"],
  denseProfileMemory,
});
assert.deepEqual(
  repeated.candidates.map((candidate) => candidate.candidateId),
  candidateIds,
);

const limited = discoverCuratorCandidates({
  batchId: "batch-limited",
  now: "2026-06-21T00:00:00.000Z",
  scopeType: "profile",
  scopeId: "prime",
  skills,
  maxCandidates: 2,
});
assert.equal(limited.candidateCount, 2);
assert.deepEqual(limited.skippedReasons, ["candidate_limit_reached"]);

const report = renderCuratorCandidateReport(batch);
assert.match(report, /# Curator Candidate Report/);
assert.match(report, /Scope: profile:prime/);
assert.match(report, /skill_create/);
assert.match(report, /dense_memory_merge/);

console.log("curator candidate discovery smoke passed");
