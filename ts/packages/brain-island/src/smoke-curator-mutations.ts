import assert from "node:assert/strict";
import {
  existsSync,
  mkdtempSync,
  mkdirSync,
  readFileSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  createCuratorGovernanceExecutor,
  curatorSkillSourceRef,
  MemoryCuratorGovernanceStore,
  rollbackCuratorMutation,
  type CuratorMutationCandidate,
} from "./index.js";

const root = mkdtempSync(join(tmpdir(), "rusty-crew-curator-mutations-"));
const skillsDir = join(root, "skills");
mkdirSync(skillsDir, { recursive: true });
writeFileSync(
  join(skillsDir, "managed.md"),
  `---
title: Managed
summary: Existing managed skill.
tags:
  - smoke
---

Original body.
`,
);

const sourceRef = await curatorSkillSourceRef(skillsDir, "managed");
const patchCandidate: CuratorMutationCandidate = {
  candidateId: "curator:batch-1:patch-managed",
  batchId: "batch-1",
  kind: "skill_patch",
  sourceRefs: [sourceRef],
  targetRef: "skill:managed",
  summary: "Patch managed skill body.",
  severity: "warning",
  confidence: 0.9,
  proposedAction: "Replace original body with curated body.",
  previewSummary: "Would patch managed skill body.",
  fingerprint: "candidate-fingerprint-1",
  status: "proposed",
  rollbackSupported: true,
  mutation: {
    type: "skill_patch",
    slug: "managed",
    oldString: "Original body.",
    newString: "Curated body.",
  },
};

const store = new MemoryCuratorGovernanceStore();
store.upsertCandidate(patchCandidate);
const executor = createCuratorGovernanceExecutor({
  skillsDir,
  store,
  now: () => new Date("2026-06-21T12:00:00.000Z"),
});

const preview = await executor({
  action: "preview_candidate",
  candidateId: patchCandidate.candidateId,
  dryRun: true,
});
assert.equal(preview.status, "previewed");
assert.match(preview.summary, /changed=false/);
assert.match(readFileSync(join(skillsDir, "managed.md"), "utf8"), /Original/);

await assert.rejects(
  () =>
    executor({
      action: "apply_candidate",
      candidateId: patchCandidate.candidateId,
      reason: "should require approval",
      dryRun: false,
    }),
  /curator_candidate_not_approved/,
);

const approval = await executor({
  action: "approve_candidate",
  candidateId: patchCandidate.candidateId,
  reason: "smoke approved",
  dryRun: false,
});
assert.equal(approval.status, "approved");

const applied = await executor({
  action: "apply_candidate",
  candidateId: patchCandidate.candidateId,
  reason: "smoke apply",
  dryRun: false,
});
assert.equal(applied.status, "applied");
assert.match(readFileSync(join(skillsDir, "managed.md"), "utf8"), /Curated/);
assert.equal(store.mutations.size, 1);

const mutation = [...store.mutations.values()][0]!;
assert.equal(mutation.status, "applied");
assert.equal(existsSync(mutation.snapshot.skillSnapshotPath ?? ""), true);
assert.equal(
  mutation.changedPaths.includes(join(skillsDir, "managed.md")),
  true,
);

const rolledBack = await rollbackCuratorMutation(store, mutation.mutationId);
assert.equal(rolledBack.status, "rolled_back");
assert.match(readFileSync(join(skillsDir, "managed.md"), "utf8"), /Original/);

const staleCandidate: CuratorMutationCandidate = {
  ...patchCandidate,
  candidateId: "curator:batch-1:stale-managed",
  fingerprint: "candidate-fingerprint-2",
  sourceRefs: [{ ...sourceRef, hash: "stale-hash" }],
};
store.upsertCandidate(staleCandidate);
await assert.rejects(
  () =>
    executor({
      action: "approve_candidate",
      candidateId: staleCandidate.candidateId,
      reason: "stale candidate",
      dryRun: false,
    }),
  /curator_candidate_stale/,
);

console.log("curator mutation smoke passed");
