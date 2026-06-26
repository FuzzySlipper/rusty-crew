import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { ProfileId } from "@rusty-crew/contracts";
import { loadNativeBridge } from "@rusty-crew/native-bridge";
import {
  captureProposalToMemoryProposal,
  runBackgroundMemorySkillReview,
  type LegacyDenseMemoryCaptureProposal,
  type TypedCaptureMemoryProposal,
} from "./index.js";

const root = mkdtempSync(join(tmpdir(), "rusty-crew-capture-proposals-"));
const profileId = "prime-profile" as ProfileId;

try {
  const legacyAdd: LegacyDenseMemoryCaptureProposal = {
    id: "legacy_dense_add",
    kind: "dense_memory_add",
    summary: "Remember operator prefers compact review summaries.",
    confidence: 0.84,
    durabilityRationale: "The preference applies across future sessions.",
    memoryKey: "review_style",
    memoryContent: "Prefers compact review summaries.",
    evidenceRefs: [
      {
        eventType: "user_correction",
        wakeId: "wake-alpha",
        summary: "User corrected review output style.",
      },
    ],
  };
  const addProposal = captureProposalToMemoryProposal({
    runId: "capture-run-1",
    profileId,
    proposal: legacyAdd,
  });
  assert.equal(addProposal.space_id, "profile_dense");
  assert.equal(addProposal.operation, "add");
  assert.equal(addProposal.source, "capture_producer");
  assert.equal(addProposal.governance_mode, "curator_route");
  assert.equal(addProposal.scope.scope_type, "profile");
  assert.equal(addProposal.shape.shape_id, "profile_dense_item");
  assert.equal(
    addProposal.evidence_refs.some(
      (ref) => ref.evidence_type === "user_correction",
    ),
    true,
  );

  const legacyReplace: LegacyDenseMemoryCaptureProposal = {
    id: "legacy_dense_replace",
    kind: "dense_memory_replace",
    summary: "Replace stale host location.",
    confidence: 0.91,
    durabilityRationale: "User corrected a stale infrastructure fact.",
    memoryKey: "den_host",
    memoryContent: "Den Core database lives on den-srv.",
    replacesKey: "den_host",
    expectedRevision: 3,
    evidenceRefs: [{ eventType: "wake", wakeId: "wake-beta" }],
  };
  assert.equal(
    captureProposalToMemoryProposal({
      runId: "capture-run-1",
      profileId,
      proposal: legacyReplace,
    }).operation,
    "replace",
  );

  const legacyRemove: LegacyDenseMemoryCaptureProposal = {
    id: "legacy_dense_remove",
    kind: "dense_memory_remove",
    summary: "Remove stale one-off service status.",
    confidence: 0.8,
    durabilityRationale: "Current service status is ephemeral.",
    memoryKey: "service_running_now",
    expectedRevision: 1,
    evidenceRefs: [{ eventType: "wake", wakeId: "wake-gamma" }],
  };
  assert.equal(
    captureProposalToMemoryProposal({
      runId: "capture-run-1",
      profileId,
      proposal: legacyRemove,
    }).operation,
    "remove",
  );

  const sessionProposal: TypedCaptureMemoryProposal = {
    id: "session_summary_candidate",
    summary: "Remember branch decision.",
    space_id: "session_memory",
    operation: "merge",
    scope: { scope_type: "session", scope_id: "session-alpha" },
    shape: { shape_id: "session_fact" as never, version: 1 },
    content: { content: "User chose the sqlite-first deployment path." },
    evidence_refs: [{ eventType: "wake", wakeId: "wake-delta" }],
    confidence: 0.73,
    durability_rationale: "The decision should persist across later wakes.",
    governance_policy: "manual_review",
    dedupe_key: "session_memory:session-alpha:sqlite-first",
  };
  const sessionEnvelope = captureProposalToMemoryProposal({
    runId: "capture-run-1",
    profileId,
    proposal: sessionProposal,
  });
  assert.equal(sessionEnvelope.space_id, "session_memory");
  assert.equal(sessionEnvelope.operation, "merge");
  assert.equal(sessionEnvelope.governance_mode, "manual_review");

  const review = await runBackgroundMemorySkillReview({
    runId: "capture-run-1",
    now: "2026-06-26T01:00:00.000Z",
    payload: {
      reviewType: "memory",
      profileId,
      llmReviewEnabled: true,
      maxFindings: 10,
      dryRun: true,
    },
    captureProposals: [legacyAdd],
  });
  const captureFinding = review.findings.find(
    (finding) => finding.candidateKind === "llm_review",
  );
  assert.ok(captureFinding);
  assert.equal(captureFinding.memoryProposal?.space_id, "profile_dense");
  assert.equal(captureFinding.memoryProposal?.operation, "add");
  assert.equal(
    review.skippedReasons.includes("llm_review_requires_provider_path"),
    true,
  );

  const bridge = await loadNativeBridge();
  const engine = await bridge.initializeEngine({
    engineDataDir: root,
    clock: { fixed: "2026-06-26T01:00:00Z" },
    defaultTurnBudget: 4,
    defaultIdleTimeoutMs: 1_000,
  });
  const stored = await bridge.saveMemoryProposal(addProposal);
  assert.equal(stored.proposal.space_id, "profile_dense");
  assert.equal(stored.proposal.operation, "add");
  assert.equal(stored.status, "pending_review");
  assert.equal(stored.selected_governance_mode, "curator_route");
  assert.equal(
    await bridge.getProfileMemory({
      profileId,
      targetType: "profile",
      key: "review_style",
    }),
    undefined,
  );
  await bridge.shutdownEngine({ engine, drainTimeoutMs: 1_000 });
  console.log("smoke-capture-memory-proposals ok");
} finally {
  rmSync(root, { recursive: true, force: true });
}
