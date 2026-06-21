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
  FileCuratorGovernanceStore,
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

const persistedStatePath = join(root, "curator-state", "governance.json");
const persistedSourceRef = await curatorSkillSourceRef(skillsDir, "managed");
const persistedCandidate: CuratorMutationCandidate = {
  ...patchCandidate,
  candidateId: "curator:batch-1:persisted-managed",
  fingerprint: "candidate-fingerprint-persisted",
  sourceRefs: [persistedSourceRef],
  mutation: {
    type: "skill_patch",
    slug: "managed",
    oldString: "Original body.",
    newString: "Persisted curated body.",
  },
};
const firstFileStore = new FileCuratorGovernanceStore(persistedStatePath);
firstFileStore.upsertCandidate(persistedCandidate);
const firstFileExecutor = createCuratorGovernanceExecutor({
  skillsDir,
  store: firstFileStore,
  now: () => new Date("2026-06-21T13:00:00.000Z"),
});
const persistedPreview = await firstFileExecutor({
  action: "preview_candidate",
  candidateId: persistedCandidate.candidateId,
  dryRun: true,
});
assert.equal(persistedPreview.status, "previewed");
const persistedApproval = await firstFileExecutor({
  action: "approve_candidate",
  candidateId: persistedCandidate.candidateId,
  reason: "persist approval across restart",
  dryRun: false,
});
assert.equal(persistedApproval.status, "approved");
assert.equal(existsSync(persistedStatePath), true);

const reloadedFileStore = new FileCuratorGovernanceStore(persistedStatePath);
assert.equal(
  reloadedFileStore.getCandidate(persistedCandidate.candidateId)?.status,
  "approved",
);
const reloadedExecutor = createCuratorGovernanceExecutor({
  skillsDir,
  store: reloadedFileStore,
  now: () => new Date("2026-06-21T13:01:00.000Z"),
});
const persistedApplied = await reloadedExecutor({
  action: "apply_candidate",
  candidateId: persistedCandidate.candidateId,
  reason: "apply after restart",
  dryRun: false,
});
assert.equal(persistedApplied.status, "applied");
assert.match(readFileSync(join(skillsDir, "managed.md"), "utf8"), /Persisted/);
assert.equal(reloadedFileStore.mutations.size, 1);

console.log("curator mutation smoke passed");
