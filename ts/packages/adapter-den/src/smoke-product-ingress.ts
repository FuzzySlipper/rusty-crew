import assert from "node:assert/strict";
import type {
  DenDataUpdate,
  EventReceipt,
  ProjectId,
} from "@rusty-crew/contracts";
import { ingestDenProductReference, toDenProductDataUpdate } from "./index.js";

const updates: DenDataUpdate[] = [];
let sequence = 0;
const ingress = {
  injectDenDataUpdate(update: DenDataUpdate): EventReceipt {
    updates.push(update);
    sequence += 1;
    return { accepted: true, sequence };
  },
};

const assignmentInput = {
  projectId: "rusty-crew" as ProjectId,
  entityKind: "assignment",
  entityId: "assignment-3054",
  revision: "rev-1",
  label: "Product context only",
  provenance: {
    source: "smoke",
    providerToken: "should-not-leak",
    rawPrompt: "should-not-leak",
  },
};

assert.deepEqual(toDenProductDataUpdate(assignmentInput), {
  projectId: "rusty-crew",
  entityKind: "assignment",
  entityId: "assignment-3054",
  revision: "rev-1",
});

const accepted = await ingestDenProductReference(assignmentInput, ingress);
assert.equal(accepted.status, "accepted");
assert.equal(accepted.workRef.sourceDomain, "den");
assert.equal(accepted.workRef.refKind, "assignment");
assert.equal(accepted.provenance.providerToken, "[redacted]");
assert.equal(accepted.provenance.rawPrompt, "[redacted]");
assert.equal(updates.length, 1);

const denied = await ingestDenProductReference(
  {
    projectId: "rusty-crew" as ProjectId,
    entityKind: "assignment",
    entityId: "assignment-3054",
    operation: "claim",
  },
  ingress,
);
assert.equal(denied.status, "denied");
assert.equal(denied.reasonCode, "adapter_lifecycle_operation_denied");
assert.equal(updates.length, 1);

const degraded = await ingestDenProductReference(
  {
    projectId: "rusty-crew" as ProjectId,
    entityKind: "task",
    entityId: "3054",
  },
  {
    injectDenDataUpdate(): EventReceipt {
      throw new Error("simulated Rust ingress outage");
    },
  },
);
assert.equal(degraded.status, "degraded");
assert.equal(degraded.reasonCode, "den_product_update_failed");
assert.equal(degraded.workRef.refKind, "task");

console.log(
  JSON.stringify(
    {
      accepted: {
        status: accepted.status,
        workRef: `${accepted.workRef.sourceDomain}:${accepted.workRef.refKind}:${accepted.workRef.id}`,
        sequence: accepted.receipt.sequence,
      },
      denied: {
        status: denied.status,
        reasonCode: denied.reasonCode,
      },
      degraded: {
        status: degraded.status,
        reasonCode: degraded.reasonCode,
      },
      updates: updates.length,
    },
    null,
    2,
  ),
);
