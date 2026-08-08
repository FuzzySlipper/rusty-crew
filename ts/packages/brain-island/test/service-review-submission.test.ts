import assert from "node:assert/strict";
import test from "node:test";

import type {
  AgentRouteResolution,
  ReviewSubmissionRecord,
} from "@rusty-crew/contracts";
import {
  createServiceReviewSubmissionRuntime,
  parseExternalReviewSubmissionRequest,
  parseReviewProjectIds,
  reconcileReviewSubmissions,
  reviewerDispatchIdentity,
  selectReviewDenBinding,
  selectRoutedReviewRecord,
  submitExternalReview,
} from "../src/service-review-submission.js";

test("review dispatch identity changes only with resolved route authority", () => {
  const resolution = {
    address: "@reviewer",
    routable: true,
    route: {
      routeKey: "reviewer",
      label: "reviewer",
      enabled: true,
      target: {
        type: "managed_external",
        agentId: "reviewer-agent",
        bindingId: "reviewer-binding",
        bindingRevision: 14,
      },
      revision: 3,
      createdAt: "2026-08-08T09:00:00.000Z",
      updatedAt: "2026-08-08T09:00:00.000Z",
    },
    resolvedTarget: {
      agentId: "reviewer-agent",
      bindingId: "reviewer-binding",
      bindingRevision: 14,
      displayLabel: "reviewer",
      profileId: "reviewer",
      runtimeId: "codex",
      runtimeKind: "codex_app_server",
      sessionId: "reviewer-session",
    },
  } as AgentRouteResolution;

  assert.equal(
    reviewerDispatchIdentity("review-1", resolution),
    "review-1:route-3:binding-14",
  );
  assert.equal(
    reviewerDispatchIdentity("review-1", {
      ...resolution,
      route: { ...resolution.route!, revision: 4 },
    }),
    "review-1:route-4:binding-14",
  );
});

test("managed reviews prefer a submitter session Den binding", () => {
  const context = {
    runtimeConfig: {
      sessions: [
        { sessionId: "session-1", profileId: "profile-1", agentId: "agent-1" },
      ],
      mcpBindings: [
        {
          bindingId: "session-den",
          status: "active",
          profileId: "profile-1",
          agentId: "agent-1",
          sessionId: "session-1",
          serverNames: ["den"],
        },
        {
          bindingId: "service-den",
          status: "active",
          serverNames: ["den"],
        },
      ],
    },
    reviewDenBindingId: "service-den",
  } as never;

  assert.equal(
    selectReviewDenBinding(context, "session-1")?.bindingId,
    "session-den",
  );
});

test("managed reviews fall back to the configured service Den binding", () => {
  const context = {
    runtimeConfig: {
      sessions: [
        { sessionId: "session-1", profileId: "profile-1", agentId: "agent-1" },
      ],
      mcpBindings: [
        {
          bindingId: "service-den",
          status: "active",
          serverNames: ["den"],
        },
      ],
    },
    reviewDenBindingId: "service-den",
  } as never;

  assert.equal(
    selectReviewDenBinding(context, "session-1")?.bindingId,
    "service-den",
  );
  assert.equal(
    selectReviewDenBinding(context, "missing")?.bindingId,
    "service-den",
  );
});

test("managed reviews reject inactive or absent Den bindings", () => {
  const context = {
    runtimeConfig: {
      sessions: [],
      mcpBindings: [
        { bindingId: "service-den", status: "inactive", serverNames: ["den"] },
      ],
    },
    reviewDenBindingId: "service-den",
  } as never;

  assert.equal(selectReviewDenBinding(context, "missing"), undefined);
});

test("reconciliation settles an exact-head Den round finalized outside Crew", async () => {
  const pending = {
    ...scopedSubmissionRecord("rusty-crew", {
      type: "external_cli",
      clientId: "test",
      idempotencyKey: "test",
    }),
    phase: "den_finalization_pending",
    reviewRoundId: 4089,
    reviewResultJson: JSON.stringify({ verdict: "looks_good" }),
    updatedAt: "2026-08-05T00:00:00.000Z",
  } as ReviewSubmissionRecord;
  const transitions: Array<Record<string, unknown>> = [];
  await reconcileReviewSubmissions({
    bridge: {
      listReviewSubmissions: async () => [pending],
      transitionReviewSubmission: async (request: Record<string, unknown>) => {
        transitions.push(request);
        return {
          ...pending,
          phase: "review_terminal",
          reviewVerdict: "looks_good",
        };
      },
    } as never,
    reviewProjectIds: ["rusty-crew"],
    reviewDenBindingId: "service-den",
    runtimeConfig: {
      sessions: [],
      mcpBindings: [
        {
          bindingId: "service-den",
          status: "active",
          serverNames: ["den"],
        },
      ],
      mcpServers: [],
    } as never,
    serviceConfig: { deploymentRole: "production" } as never,
    now: () => "2026-08-08T09:30:00.000Z",
    callDenTool: async (_binding: unknown, toolName: string) => {
      assert.equal(toolName, "list_review_rounds");
      return {
        project_id: "rusty-crew",
        items: [
          {
            id: 4089,
            project_id: "rusty-crew",
            head_commit: pending.commitSha,
            verdict: "looks_good",
          },
        ],
      };
    },
    applyCoordinationDelivery: async (receipt: never) => receipt,
  });

  assert.deepEqual(transitions, [
    {
      submissionId: pending.submissionId,
      expectedRevision: pending.revision,
      transition: {
        type: "den_already_finalized",
        reviewRoundId: 4089,
        exactHeadCommit: pending.commitSha,
        verdict: "looks_good",
        terminalReason: "den_round_already_finalized",
      },
      now: "2026-08-08T09:30:00.000Z",
    },
  ]);
});

function record(
  submissionId: string,
  taskId: string,
  commitSha: string,
  phase: ReviewSubmissionRecord["phase"],
): ReviewSubmissionRecord {
  return { submissionId, taskId, commitSha, phase } as ReviewSubmissionRecord;
}

function scopedSubmissionRecord(
  projectId: string,
  caller: ReviewSubmissionRecord["caller"],
): ReviewSubmissionRecord {
  return {
    submissionId: `review-${projectId}`,
    projectId,
    taskId: "6662",
    repository: "FuzzySlipper/rusty-crew",
    commitSha: "a".repeat(40),
    gitRef: "main",
    requiredChecks: ["Verify Offline"],
    baseCommit: "0".repeat(40),
    reviewSummaryMd: "Managed review.",
    reviewer: "@reviewer",
    submitterAgentId: "scope-test-agent",
    submitterSessionId: undefined,
    caller,
    phase: "gate_pending",
    reviewRoundId: 1,
    gateId: 2,
    revision: 1,
    updatedAt: "2026-08-05T00:00:00.000Z",
  } as ReviewSubmissionRecord;
}

function reviewScopeContext(
  reviewProjectIds: readonly string[],
  onBegin?: (request: { projectId: string }) => void,
) {
  return {
    bridge: {
      beginReviewSubmission: async (request: {
        projectId: string;
        caller: ReviewSubmissionRecord["caller"];
      }) => {
        onBegin?.(request);
        return scopedSubmissionRecord(request.projectId, request.caller);
      },
    } as never,
    reviewProjectIds,
    runtimeConfig: { sessions: [], mcpBindings: [], mcpServers: [] } as never,
    serviceConfig: { deploymentRole: "debug" } as never,
    now: () => "2026-08-05T00:01:00.000Z",
    applyCoordinationDelivery: async (receipt: never) => receipt,
  };
}

function directReviewInput(projectId: string) {
  return {
    projectId,
    taskId: 6662,
    repository: "FuzzySlipper/rusty-crew",
    commitSha: "a".repeat(40),
    ref: "main",
    requiredChecks: ["Verify Offline"],
    baseCommit: "0".repeat(40),
    reviewSummaryMd: "Managed review.",
    caller: {
      type: "direct_brain" as const,
      sessionId: "scope-test-session",
      wakeId: "scope-test-wake",
      toolCallId: "scope-test-tool",
    },
  };
}

function gatePendingRecord(): ReviewSubmissionRecord {
  return {
    submissionId: "review-gate-pending",
    projectId: "den-services",
    taskId: "6663",
    repository: "FuzzySlipper/rusty-crew",
    commitSha: "c".repeat(40),
    gitRef: "main",
    requiredChecks: ["Verify Offline"],
    reviewSummaryMd: "Managed review.",
    reviewer: "@reviewer",
    submitterAgentId: "codex",
    caller: { type: "external_cli", clientId: "test", idempotencyKey: "test" },
    phase: "gate_pending",
    gateId: 2719,
    revision: 3,
    updatedAt: "2026-08-05T00:00:00.000Z",
  } as ReviewSubmissionRecord;
}

function gateReconciliationContext(
  gate: Record<string, unknown>,
  transitions: unknown[],
) {
  const pending = gatePendingRecord();
  return {
    bridge: {
      listReviewSubmissions: async () => [pending],
      transitionReviewSubmission: async (request: unknown) => {
        transitions.push(request);
        return pending;
      },
    } as never,
    reviewProjectIds: ["den-services"],
    reviewDenBindingId: "den-review",
    runtimeConfig: {
      sessions: [],
      mcpBindings: [
        {
          bindingId: "den-review",
          status: "active",
          serverNames: ["den"],
        },
      ],
      mcpServers: [],
    } as never,
    serviceConfig: { deploymentRole: "production" } as never,
    now: () => "2026-08-05T00:01:00.000Z",
    callDenTool: async (_binding: unknown, toolName: string) => {
      assert.equal(toolName, "get_github_check_gate");
      return gate;
    },
    applyCoordinationDelivery: async (receipt: never) => receipt,
  };
}

test("managed gate reconciliation advances an exact-SHA passed gate", async () => {
  const transitions: unknown[] = [];
  const pending = gatePendingRecord();
  await reconcileReviewSubmissions(
    gateReconciliationContext(
      {
        id: pending.gateId,
        project_id: pending.projectId,
        task_id: Number(pending.taskId),
        commit_sha: pending.commitSha,
        status: "passed",
        terminal_reason: "checks_passed",
      },
      transitions,
    ),
  );
  assert.deepEqual(transitions[0], {
    submissionId: pending.submissionId,
    expectedRevision: pending.revision,
    transition: {
      type: "gate_terminal",
      gateStatus: "passed",
      terminalReason: "checks_passed",
    },
    now: "2026-08-05T00:01:00.000Z",
  });
});

test("review project configuration deduplicates valid ids and rejects malformed ids", () => {
  assert.deepEqual(
    parseReviewProjectIds("rusty-crew, den-services, rusty-crew"),
    ["rusty-crew", "den-services"],
  );
  assert.throws(
    () => parseReviewProjectIds("rusty-crew,../other-project"),
    /comma-separated Den project ids/,
  );
});

test("managed review submission carries an explicitly allowed cross-project scope", async () => {
  let selectedProject: string | undefined;
  const runtime = createServiceReviewSubmissionRuntime(() =>
    reviewScopeContext(["rusty-crew", "den-services"], (request) => {
      selectedProject = request.projectId;
    }),
  );

  const result = await runtime.submit(directReviewInput("den-services"));

  assert.equal(selectedProject, "den-services");
  assert.equal(result.ok, true);
  assert.equal(result.projectId, "den-services");
});

test("managed review submission rejects an unconfigured project before Rust persistence", async () => {
  let beginCalled = false;
  const runtime = createServiceReviewSubmissionRuntime(() =>
    reviewScopeContext(["rusty-crew"], () => {
      beginCalled = true;
    }),
  );

  const result = await runtime.submit(directReviewInput("den-services"));

  assert.equal(beginCalled, false);
  assert.equal(result.ok, false);
  assert.equal(result.projectId, "den-services");
  assert.equal(result.reasonCode, "review_project_not_allowed");
});

test("managed review submission reports missing project integration distinctly", async () => {
  const runtime = createServiceReviewSubmissionRuntime(() =>
    reviewScopeContext([]),
  );

  const result = await runtime.submit(directReviewInput("rusty-crew"));

  assert.equal(result.ok, false);
  assert.equal(result.reasonCode, "review_project_not_configured");
});

test("external review receipt preserves an allowed project scope", async () => {
  const receipt = await submitExternalReview(
    reviewScopeContext(["rusty-crew", "den-services"]),
    {
      projectId: "den-services",
      taskId: 6662,
      repository: "FuzzySlipper/rusty-crew",
      commitSha: "a".repeat(40),
      ref: "main",
      requiredChecks: ["Verify Offline"],
      baseCommit: "0".repeat(40),
      reviewSummaryMd: "Managed review.",
      clientId: "scope-test-cli",
      idempotencyKey: "scope-test-6662",
      expectedDeploymentRole: "debug",
    },
  );

  assert.equal(receipt.projectId, "den-services");
  assert.equal(receipt.phase, "gate_pending");
});

test("managed gate reconciliation routes a failed gate to terminal failure", async () => {
  const transitions: unknown[] = [];
  const pending = gatePendingRecord();
  await reconcileReviewSubmissions(
    gateReconciliationContext(
      {
        id: pending.gateId,
        project_id: pending.projectId,
        task_id: Number(pending.taskId),
        commit_sha: pending.commitSha,
        status: "failed",
        terminal_reason: "checks_failed",
      },
      transitions,
    ),
  );
  assert.equal(
    (transitions[0] as { transition: { type: string } }).transition.type,
    "gate_terminal",
  );
  assert.equal(
    (transitions[0] as { transition: { terminalReason: string } }).transition
      .terminalReason,
    "checks_failed",
  );
});

test("managed gate reconciliation leaves a pending exact-SHA gate untouched", async () => {
  const transitions: unknown[] = [];
  const pending = gatePendingRecord();
  await reconcileReviewSubmissions(
    gateReconciliationContext(
      {
        id: pending.gateId,
        project_id: pending.projectId,
        task_id: Number(pending.taskId),
        commit_sha: pending.commitSha,
        status: "pending",
      },
      transitions,
    ),
  );
  assert.deepEqual(transitions, []);
});

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
    reviewProjectIds: ["rusty-crew"],
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

test("managed closeout explains when a direct Den review has no Crew attachment", async () => {
  const runtime = createServiceReviewSubmissionRuntime(() => ({
    bridge: {
      listReviewSubmissions: async () => [],
    } as never,
    reviewProjectIds: ["rusty-crew"],
    runtimeConfig: { sessions: [] } as never,
    serviceConfig: { deploymentRole: "production" } as never,
    now: () => "2026-08-05T00:00:00.000Z",
    applyCoordinationDelivery: async (receipt) => receipt,
  }));

  const result = await runtime.complete({
    verdict: "looks_good",
    taskId: 6651,
    commitSha: "c".repeat(40),
    caller: { type: "review_submission", submissionId: "context-resolved" },
    reviewerSessionId: "reviewer-session",
  });

  assert.equal(result.ok, false);
  assert.equal(result.reasonCode, "review_target_not_found");
  assert.match(result.summary, /requested directly through Den/);
  assert.match(result.summary, /finalize_review/);
});

test("external review submission parser does not expose a reviewer override", () => {
  const input = {
    projectId: "rusty-crew",
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
