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
import { loadNativeBridge } from "@rusty-crew/native-bridge";
import {
  createCuratorGovernanceExecutor,
  curatorSkillSourceRef,
  FileCuratorGovernanceStore,
  MemoryCuratorGovernanceStore,
  NativeCuratorGovernanceStore,
  rollbackCuratorMutation,
  type CuratorGovernancePlanner,
  type CuratorMutationCandidate,
} from "../src/index.js";

const root = mkdtempSync(join(tmpdir(), "rusty-crew-curator-mutations-"));
const bridge = await loadNativeBridge();
const engine = await bridge.initializeEngine({
  engineDataDir: join(root, "engine"),
  clock: { fixed: "2026-06-21T12:00:00.000Z" },
  defaultTurnBudget: 4,
  defaultIdleTimeoutMs: 1_000,
});
const planner: CuratorGovernancePlanner = (input) =>
  bridge.planCuratorGovernanceTransition(
    input,
  ) as ReturnType<CuratorGovernancePlanner>;
const skillsDir = join(root, "skills");
const nativeSnapshotRoot = join(root, "native-snapshots");
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
  planner,
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
  planner,
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
  planner,
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

writeFileSync(
  join(skillsDir, "native-managed.md"),
  `---
title: Native Managed
summary: Native-backed managed skill.
---

Native original body.
`,
);
const nativeSourceRef = await curatorSkillSourceRef(
  skillsDir,
  "native-managed",
);
const nativeCandidate: CuratorMutationCandidate = {
  ...patchCandidate,
  candidateId: "curator:batch-1:native-managed",
  fingerprint: "candidate-fingerprint-native",
  sourceRefs: [nativeSourceRef],
  targetRef: "skill:native-managed",
  mutation: {
    type: "skill_patch",
    slug: "native-managed",
    oldString: "Native original body.",
    newString: "Native persisted body.",
  },
};
const projectedReceiptIds: string[] = [];
let failActivityProjection = false;
const nativeStore = await NativeCuratorGovernanceStore.load({
  bridge,
  now: "2026-06-21T14:00:00.000Z",
  skillsDir,
  snapshotRoot: nativeSnapshotRoot,
  publishActivity: async (receipt) => {
    projectedReceiptIds.push(receipt.receiptId);
    if (failActivityProjection) {
      throw new Error("synthetic observation outage");
    }
  },
});
nativeStore.upsertCandidate(nativeCandidate);
await nativeStore.persist();
assert.equal(projectedReceiptIds.length, 1);
const nativeExecutor = createCuratorGovernanceExecutor({
  skillsDir,
  store: nativeStore,
  snapshotDir: nativeSnapshotRoot,
  now: () => new Date("2026-06-21T14:00:00.000Z"),
  planner,
});
await assert.rejects(
  nativeExecutor({
    action: "apply_candidate",
    candidateId: nativeCandidate.candidateId,
    reason: "must reject unapproved mutation",
    dryRun: false,
  }),
  /curator_candidate_not_approved/,
);
const rejectedAudit = (await bridge.listCuratorAuditReceipts({
  candidate_id: nativeCandidate.candidateId,
  page: { limit: 20, offset: 0 },
})) as {
  items: Array<{ activityKind: string; reasonCode?: string }>;
};
assert.equal(rejectedAudit.items.at(-1)?.activityKind, "mutation_failed");
assert.equal(
  rejectedAudit.items.at(-1)?.reasonCode,
  "curator_candidate_not_approved",
);
failActivityProjection = true;
const nativeApproval = await nativeExecutor({
  action: "approve_candidate",
  candidateId: nativeCandidate.candidateId,
  reason: "persist approval through rust storage",
  dryRun: false,
});
assert.equal(nativeApproval.status, "approved");
assert.equal(nativeStore.activityProjectionFailures.length, 1);
assert.match(
  nativeStore.activityProjectionFailures[0]!.message,
  /synthetic observation outage/,
);

const reloadedNativeStore = await NativeCuratorGovernanceStore.load({
  bridge,
  now: "2026-06-21T14:01:00.000Z",
  skillsDir,
  snapshotRoot: nativeSnapshotRoot,
});
assert.equal(
  reloadedNativeStore.getCandidate(nativeCandidate.candidateId)?.status,
  "approved",
);
const reloadedNativeExecutor = createCuratorGovernanceExecutor({
  skillsDir,
  store: reloadedNativeStore,
  snapshotDir: nativeSnapshotRoot,
  now: () => new Date("2026-06-21T14:01:00.000Z"),
  planner,
});
const nativeApplied = await reloadedNativeExecutor({
  action: "apply_candidate",
  candidateId: nativeCandidate.candidateId,
  reason: "apply native-backed approval",
  dryRun: false,
});
assert.equal(nativeApplied.status, "applied");
assert.match(
  readFileSync(join(skillsDir, "native-managed.md"), "utf8"),
  /Native persisted body/,
);
assert.equal(reloadedNativeStore.mutations.size, 1);

const appliedReload = await NativeCuratorGovernanceStore.load({
  bridge,
  now: "2026-06-21T14:02:00.000Z",
  skillsDir,
  snapshotRoot: nativeSnapshotRoot,
});
assert.equal(appliedReload.mutations.values().next().value?.status, "applied");
await rollbackCuratorMutation(
  appliedReload,
  appliedReload.mutations.keys().next().value!,
);
const rollbackReload = await NativeCuratorGovernanceStore.load({
  bridge,
  now: "2026-06-21T14:03:00.000Z",
  skillsDir,
  snapshotRoot: nativeSnapshotRoot,
});
assert.equal(
  rollbackReload.mutations.values().next().value?.status,
  "rolled_back",
);

await bridge.shutdownEngine({ engine, drainTimeoutMs: 1_000 });
console.log("curator mutation smoke passed");
