import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { MemoryProposalEnvelope } from "@rusty-crew/contracts";
import { loadNativeBridge } from "@rusty-crew/native-bridge";

const root = mkdtempSync(join(tmpdir(), "rusty-crew-memory-proposals-"));

try {
  const bridge = await loadNativeBridge();
  const engine = await bridge.initializeEngine({
    engineDataDir: root,
    clock: { fixed: "2026-06-26T00:00:00Z" },
    defaultTurnBudget: 4,
    defaultIdleTimeoutMs: 1_000,
  });

  const descriptors = await bridge.listMemorySpaceDescriptors();
  const profileDense = descriptors.find(
    (descriptor) => descriptor.space_id === "profile_dense",
  );
  assert.ok(profileDense, "profile_dense descriptor should exist");
  assert.ok(profileDense.operations.includes("candidate_only"));

  const proposal = profileDenseProposal("proposal_one", "profile_dense:style");
  const created = await bridge.saveMemoryProposal(proposal);
  assert.equal(created.proposal.proposal_id, "proposal_one");
  assert.equal(created.status, "pending_review");
  assert.equal(created.selected_governance_mode, "curator_route");

  const duplicate = await bridge.saveMemoryProposal(
    profileDenseProposal("proposal_two", "profile_dense:style"),
  );
  assert.equal(duplicate.proposal.proposal_id, "proposal_one");
  assert.equal(
    (await bridge.listMemoryProposals({ space_id: "profile_dense" as never }))
      .length,
    1,
  );

  await assert.rejects(
    bridge.saveMemoryProposal({
      ...proposal,
      proposal_id: "proposal_bad_space",
      space_id: "roleplay_lore" as never,
      dedupe_key: "profile_dense:bad_space",
    }),
  );
  await assert.rejects(
    bridge.saveMemoryProposal({
      ...proposal,
      proposal_id: "proposal_bad_scope",
      scope: { scope_type: "world", scope_id: "world-alpha" },
      dedupe_key: "profile_dense:bad_scope",
    }),
  );
  await assert.rejects(
    bridge.saveMemoryProposal({
      ...proposal,
      proposal_id: "proposal_bad_operation",
      operation: "merge",
      dedupe_key: "profile_dense:bad_operation",
    }),
  );

  const pending = await bridge.listMemoryProposals({
    status: "pending_review",
  });
  assert.equal(pending.length, 1);
  assert.equal(
    await bridge.getProfileMemory({
      profileId: "prime-profile",
      targetType: "profile",
      key: "style",
    }),
    undefined,
  );

  const approved = await bridge.recordMemoryGovernanceDecision({
    decision_id: "decision_approve",
    proposal_id: "proposal_one",
    decision: "approved",
    actor: "human_operator",
    source: "human",
    evidence_refs: [{ evidence_type: "ui", ref_id: "admin-review" }],
    policy_mode: "manual_review",
    confidence: 0.95,
  });
  assert.equal(approved.decision, "approved");

  const applied = await bridge.recordMemoryGovernanceDecision({
    decision_id: "decision_apply",
    proposal_id: "proposal_one",
    decision: "applied",
    actor: "curator",
    source: "human",
    evidence_refs: [{ evidence_type: "ui", ref_id: "admin-review" }],
    policy_mode: "manual_review",
    confidence: 0.97,
    resulting_revision: 7,
  });
  assert.equal(applied.resulting_revision, 7);

  const appliedRecords = await bridge.listMemoryProposals({
    status: "applied",
  });
  assert.equal(appliedRecords.length, 1);
  assert.equal(appliedRecords[0]?.resulting_revision, 7);
  assert.equal(
    await bridge.getProfileMemory({
      profileId: "prime-profile",
      targetType: "profile",
      key: "style",
    }),
    undefined,
  );

  await bridge.shutdownEngine({ engine, drainTimeoutMs: 1_000 });
  console.log("smoke-memory-proposals ok");
} finally {
  rmSync(root, { recursive: true, force: true });
}

function profileDenseProposal(
  proposalId: string,
  dedupeKey: string,
): MemoryProposalEnvelope {
  return {
    proposal_id: proposalId,
    space_id: "profile_dense" as never,
    operation: "candidate_only",
    scope: {
      scope_type: "profile",
      scope_id: "prime-profile",
    },
    shape: {
      shape_id: "profile_dense_item" as never,
      version: 1,
    },
    content: {
      key: "style",
      content: "prefers typed governance review",
    },
    evidence_refs: [
      {
        evidence_type: "wake",
        ref_id: "wake-alpha",
        label: "wake evidence",
      },
    ],
    confidence: 0.82,
    durability_rationale: "stable profile preference",
    governance_mode: "direct_write",
    source: "in_wake_tool",
    dedupe_key: dedupeKey,
  };
}
