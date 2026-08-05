import assert from "node:assert/strict";
import test from "node:test";

import type { ReviewSubmissionRecord } from "@rusty-crew/contracts";
import {
  parseExternalReviewSubmissionRequest,
  selectRoutedReviewRecord,
} from "../src/service-review-submission.js";

function record(
  submissionId: string,
  taskId: string,
  commitSha: string,
  phase: ReviewSubmissionRecord["phase"],
): ReviewSubmissionRecord {
  return { submissionId, taskId, commitSha, phase } as ReviewSubmissionRecord;
}

test("reused reviewer sessions select the new active review over old terminal work", () => {
  const oldSha = "a".repeat(40);
  const newSha = "b".repeat(40);
  const oldReview = record("review-old", "6600", oldSha, "replied");
  const newReview = record("review-new", "6601", newSha, "reviewer_dispatched");

  const current = selectRoutedReviewRecord([oldReview, newReview]);
  assert.equal(current.ambiguous, false);
  assert.equal(current.record?.submissionId, "review-new");

  const retry = selectRoutedReviewRecord(
    [oldReview, newReview],
    `review:6600:${oldSha}`,
  );
  assert.equal(retry.ambiguous, false);
  assert.equal(retry.record?.submissionId, "review-old");

  const afterRestart = selectRoutedReviewRecord(
    [oldReview, newReview],
    `review:6600:${oldSha}`,
  );
  assert.equal(afterRestart.record?.submissionId, "review-old");
});

test("multiple terminal reviews remain ambiguous without persisted routed context", () => {
  const first = record("review-first", "6600", "a".repeat(40), "replied");
  const second = record(
    "review-second",
    "6601",
    "b".repeat(40),
    "reply_terminal",
  );
  const result = selectRoutedReviewRecord([first, second]);
  assert.equal(result.record, undefined);
  assert.equal(result.ambiguous, true);
});

test("external review submission parser does not expose a reviewer override", () => {
  const input = {
    taskId: 6644,
    repository: "earendil-works/rusty-crew",
    commitSha: "a".repeat(40),
    ref: "main",
    requiredChecks: ["Verify Offline", "Verify Postgres Backend"],
    baseCommit: "0".repeat(40),
    reviewSummaryMd: "Ready for exact-SHA review.",
    clientId: "external-agent",
    idempotencyKey: "6644-a",
    expectedDeploymentRole: "debug",
    reviewer: "@untrusted-recipient",
  };
  assert.throws(() => parseExternalReviewSubmissionRequest(input));
  const { reviewer: _reviewer, ...withoutReviewer } = input;
  const request = parseExternalReviewSubmissionRequest(withoutReviewer);
  assert.equal(request.taskId, 6644);
  assert.equal(request.clientId, "external-agent");
  assert.equal("reviewer" in request, false);
});
