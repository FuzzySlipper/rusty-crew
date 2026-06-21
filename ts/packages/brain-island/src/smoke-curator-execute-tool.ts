import assert from "node:assert/strict";
import { curatorExecuteTool } from "./index.js";
import type { CuratorExecuteRequest } from "./index.js";

const missingExecutor = await curatorExecuteTool({}).execute("missing", {
  action: "request_scan",
  scopeType: "profile",
  scopeId: "profile-alpha",
});
assert.equal(missingExecutor.details.ok, false);
assert.equal(
  missingExecutor.details.reasonCode,
  "curator_executor_unavailable",
);

const calls: CuratorExecuteRequest[] = [];
const tool = curatorExecuteTool({
  actorId: "agent-alpha",
  sessionId: "session-alpha",
  profileId: "profile-alpha",
  allowedActions: [
    "request_scan",
    "preview_candidate",
    "approve_candidate",
    "apply_candidate",
  ],
  executor(request) {
    calls.push(request);
    const status =
      request.action === "request_scan"
        ? "requested"
        : request.action === "preview_candidate"
          ? "previewed"
          : request.action === "approve_candidate"
            ? "approved"
            : "applied";
    return {
      receiptId: `curator-receipt-${calls.length}`,
      status,
      candidateId: request.candidateId,
      auditRef: `audit:${calls.length}`,
      observationRef: `observation:${calls.length}`,
      summary: `${request.action} accepted`,
    };
  },
});

const missingScope = await tool.execute("missing-scope", {
  action: "request_scan",
});
assert.equal(missingScope.details.ok, false);
assert.equal(missingScope.details.reasonCode, "curator_scope_required");

const scan = await tool.execute("scan", {
  action: "request_scan",
  scopeType: "profile",
  scopeId: "profile-alpha",
  reason: "curator smoke",
});
assert.equal(scan.details.ok, true);
assert.equal(scan.details.status, "requested");
assert.equal(scan.details.dryRun, true);
assert.equal(calls[0]?.actorId, "agent-alpha");
assert.equal(calls[0]?.scopeType, "profile");

const missingCandidate = await tool.execute("missing-candidate", {
  action: "preview_candidate",
});
assert.equal(missingCandidate.details.ok, false);
assert.equal(
  missingCandidate.details.reasonCode,
  "curator_candidate_id_required",
);

const preview = await tool.execute("preview", {
  action: "preview_candidate",
  candidateId: "candidate-1",
});
assert.equal(preview.details.ok, true);
assert.equal(preview.details.status, "previewed");
assert.equal(preview.details.candidateId, "candidate-1");

const approveWithoutReason = await tool.execute("approve-no-reason", {
  action: "approve_candidate",
  candidateId: "candidate-1",
});
assert.equal(approveWithoutReason.details.ok, false);
assert.equal(
  approveWithoutReason.details.reasonCode,
  "curator_reason_required",
);

const applyWithoutConfirmation = await tool.execute("apply-no-confirm", {
  action: "apply_candidate",
  candidateId: "candidate-1",
  dryRun: false,
  reason: "approved by smoke",
});
assert.equal(applyWithoutConfirmation.details.ok, false);
assert.equal(
  applyWithoutConfirmation.details.reasonCode,
  "curator_apply_confirmation_required",
);

const applied = await tool.execute("apply", {
  action: "apply_candidate",
  candidateId: "candidate-1",
  dryRun: false,
  reason: "approved by smoke",
  confirm: true,
});
assert.equal(applied.details.ok, true);
assert.equal(applied.details.status, "applied");
assert.equal(applied.details.receipt?.auditRef, "audit:3");
assert.equal(calls.at(-1)?.dryRun, false);

const deniedAction = await curatorExecuteTool({
  allowedActions: ["request_scan"],
  executor() {
    throw new Error("should not execute");
  },
}).execute("denied-action", {
  action: "preview_candidate",
  candidateId: "candidate-2",
});
assert.equal(deniedAction.details.ok, false);
assert.equal(deniedAction.details.reasonCode, "curator_action_not_allowed");

console.log(
  JSON.stringify(
    {
      missingExecutor: missingExecutor.details.reasonCode,
      scanReceipt: scan.details.receipt?.receiptId,
      previewStatus: preview.details.status,
      applyReceipt: applied.details.receipt?.receiptId,
      executorCalls: calls.length,
    },
    null,
    2,
  ),
);
