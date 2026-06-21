import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  createCuratorGovernanceExecutor,
  curatorSkillSourceRef,
  MemoryCuratorGovernanceStore,
  runCuratorLifecycleTransitions,
  type CuratorCandidateBatch,
  type CuratorMutationCandidate,
} from "./index.js";

const root = mkdtempSync(join(tmpdir(), "rusty-crew-curator-lifecycle-"));
const skillsDir = join(root, "skills");
mkdirSync(skillsDir, { recursive: true });
writeSkill("archive-me", "Archive Me", "Archive body.");
writeSkill("reactive", "Reactive", "Reactive body.");
writeSkill("pinned", "Pinned", "Pinned body.");
writeFileSync(join(skillsDir, "pinned.pinned"), "operator pinned\n", "utf8");

const oldBatch: CuratorCandidateBatch = {
  batchId: "curator-lifecycle-batch",
  scopeType: "profile",
  scopeId: "prime",
  profileId: "prime",
  generatedAt: "2026-06-21T00:00:00.000Z",
  dryRun: true,
  candidateCount: 3,
  reportId: "curator-report:lifecycle",
  candidates: [],
  skippedReasons: [],
};
const archiveCandidate = await patchCandidate("archive-me");
const reactiveCandidate = await patchCandidate("reactive");
const pinnedCandidate = await patchCandidate("pinned");
const store = new MemoryCuratorGovernanceStore();
store.upsertBatch(
  {
    ...oldBatch,
    candidates: [archiveCandidate, reactiveCandidate, pinnedCandidate],
  },
  [archiveCandidate, reactiveCandidate, pinnedCandidate],
);

const first = await runCuratorLifecycleTransitions({
  store,
  skillsDir,
  now: "2026-06-22T00:00:00.000Z",
  policy: { staleAfterMs: 1_000, archiveAfterMs: 1_000 },
});
assert.equal(first.stale, 2);
assert.equal(first.pinnedSkipped, 1);
assert.deepEqual(
  first.transitions.map((transition) => transition.reasonCode).sort(),
  ["candidate_idle_stale", "candidate_idle_stale", "skill_pinned"],
);
assert.equal(
  store.getCandidate(archiveCandidate.candidateId)?.lifecycle?.state,
  "stale",
);

const executor = createCuratorGovernanceExecutor({
  skillsDir,
  store,
  now: () => new Date("2026-06-22T00:01:00.000Z"),
});
const reactivePreview = await executor({
  action: "preview_candidate",
  candidateId: reactiveCandidate.candidateId,
  dryRun: true,
  reason: "operator inspected stale candidate",
});
assert.equal(reactivePreview.status, "previewed");

const second = await runCuratorLifecycleTransitions({
  store,
  skillsDir,
  now: "2026-06-22T00:02:00.000Z",
  policy: { staleAfterMs: 1_000, archiveAfterMs: 1_000 },
});
assert.equal(second.reactivated, 1);
assert.equal(
  store.getCandidate(reactiveCandidate.candidateId)?.lifecycle?.state,
  "active",
);

const third = await runCuratorLifecycleTransitions({
  store,
  skillsDir,
  now: "2026-06-22T00:03:00.000Z",
  policy: { staleAfterMs: 1_000, archiveAfterMs: 1_000 },
});
assert.equal(third.archived, 1);
assert.equal(
  store.getCandidate(archiveCandidate.candidateId)?.lifecycle?.state,
  "archived",
);
assert.equal(
  store.getCandidate(pinnedCandidate.candidateId)?.lifecycle?.state,
  undefined,
);

await assert.rejects(
  () =>
    executor({
      action: "preview_candidate",
      candidateId: archiveCandidate.candidateId,
      dryRun: true,
      reason: "archived candidates do not resurrect",
    }),
  /curator_candidate_archived/,
);

console.log(
  JSON.stringify(
    {
      first: first.transitions.map((transition) => transition.to),
      second: second.transitions.map((transition) => transition.reasonCode),
      third: third.transitions.map((transition) => transition.reasonCode),
    },
    null,
    2,
  ),
);

async function patchCandidate(slug: string): Promise<CuratorMutationCandidate> {
  const sourceRef = await curatorSkillSourceRef(skillsDir, slug);
  return {
    candidateId: `curator:lifecycle:${slug}`,
    batchId: oldBatch.batchId,
    kind: "skill_patch",
    sourceRefs: [sourceRef],
    targetRef: `skill:${slug}`,
    summary: `Patch ${slug}.`,
    severity: "info",
    confidence: 0.9,
    proposedAction: "Patch skill body.",
    previewSummary: "Would patch skill body.",
    fingerprint: `fingerprint:${slug}`,
    status: "proposed",
    rollbackSupported: true,
    mutation: {
      type: "skill_patch",
      slug,
      oldString: `${titleFromSlug(slug)} body.`,
      newString: `${titleFromSlug(slug)} updated.`,
    },
  };
}

function writeSkill(slug: string, title: string, body: string): void {
  writeFileSync(
    join(skillsDir, `${slug}.md`),
    `---
title: ${title}
summary: Lifecycle smoke fixture.
---

${body}
`,
    "utf8",
  );
}

function titleFromSlug(slug: string): string {
  return slug
    .split("-")
    .map((part) => `${part[0]?.toUpperCase() ?? ""}${part.slice(1)}`)
    .join(" ");
}
