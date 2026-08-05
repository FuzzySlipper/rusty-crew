import assert from "node:assert/strict";
import test from "node:test";

import type { ReviewSubmissionRecord } from "@rusty-crew/contracts";
import {
  createServiceReviewSubmissionRuntime,
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

test("explicit task and SHA select a queued review over stale wake correlation", () => {
  const firstSha = "a".repeat(40);
  const secondSha = "b".repeat(40);
  const first = record("review-first", "6600", firstSha, "reviewer_dispatched");
  const second = record(
    "review-second",
    "6601",
    secondSha,
    "reviewer_dispatched",
  );

  const selected = selectRoutedReviewRecord(
    [first, second],
    `review:6600:${firstSha}`,
    { taskId: 6601, commitSha: secondSha },
  );
  assert.equal(selected.ambiguous, false);
  assert.equal(selected.notFound, undefined);
  assert.equal(selected.record?.submissionId, "review-second");
});

test("explicit review target cannot cross-select a different task or SHA", () => {
  const sha = "a".repeat(40);
  const review = record("review-one", "6600", sha, "reviewer_dispatched");

  const wrongTask = selectRoutedReviewRecord([review], undefined, {
    taskId: 6601,
    commitSha: sha,
  });
  assert.equal(wrongTask.record, undefined);
  assert.equal(wrongTask.ambiguous, false);
  assert.equal(wrongTask.notFound, true);

  const wrongSha = selectRoutedReviewRecord([review], undefined, {
    taskId: 6600,
    commitSha: "b".repeat(40),
  });
  assert.equal(wrongSha.record, undefined);
  assert.equal(wrongSha.ambiguous, false);
  assert.equal(wrongSha.notFound, true);
});

test("managed closeout uses the explicit target within a reused reviewer session", async () => {
  const firstSha = "a".repeat(40);
  const secondSha = "b".repeat(40);
  const first = record("review-first", "6600", firstSha, "reply_terminal");
  const second = record("review-second", "6601", secondSha, "reply_terminal");
  const runtime = createServiceReviewSubmissionRuntime(() => ({
    bridge: {
      listReviewSubmissions: async () => [first, second],
    } as never,
    projectId: "rusty-crew",
    runtimeConfig: { sessions: [] } as never,
    serviceConfig: { deploymentRole: "production" } as never,
    now: () => "2026-08-05T00:00:00.000Z",
    applyCoordinationDelivery: async (receipt) => receipt,
  }));

  const result = await runtime.complete({
    verdict: "looks_good",
    taskId: 6601,
    commitSha: secondSha,
    caller: { type: "review_submission", submissionId: "context-resolved" },
    reviewerSessionId: "reviewer-session",
  });
  assert.equal(result.submissionId, "review-second");
  assert.equal(result.taskId, 6601);
  assert.equal(result.commitSha, secondSha);
  assert.equal(result.reasonCode, "review_reply_terminal");
});

test("external review submission parser does not expose a reviewer override", () => {
  const input = {
    taskId: 6644,
    repository: "FuzzySlipper/rusty-crew",
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
