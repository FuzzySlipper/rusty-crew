import assert from "node:assert/strict";
import test from "node:test";
import type { ReviewSubmissionRecord } from "@rusty-crew/contracts";
import {
  composedReviewPipeline,
  managedSubmissionStage,
  reviewOperatorConfigReadback,
} from "../src/service-review-operator.js";
import { handleReviewOperatorRequest } from "../src/service-review-operator-routes.js";
import { reviewConfig } from "../src/service-config.js";

const authority = {
  authorityId: "review-den",
  endpointRef: "config://mcp/den",
  serverName: "den" as const,
  toolProfileKey: "direct" as const,
  auditIdentity: "review-service",
  bearerToken: "must-not-leak",
};

const diagnostics = {
  authorityId: "review-den",
  auditIdentity: "review-service",
  serverName: "den" as const,
  status: "ready" as const,
  requiredTools: ["list_review_pipeline"] as never,
  missingTools: [],
  checkedAt: "2026-08-12T00:00:00.000Z",
  message: "ready",
};
const reviewerRoute = {
  address: "@reviewer",
  routable: true,
  resolvedTarget: {
    agentId: "reviewer",
    displayLabel: "Reviewer",
    profileId: "reviewer",
    runtimeKind: "codex_app_server" as const,
    sessionId: "reviewer-session",
  },
};

test("review config readback reports credential presence without secret material", () => {
  const readback = reviewOperatorConfigReadback({
    deploymentRole: "production",
    authority,
    diagnostics,
    reviewerRoute,
  });
  assert.deepEqual(readback.credential, {
    present: true,
    source: "service_environment",
  });
  assert.equal(JSON.stringify(readback).includes("must-not-leak"), false);
});

test("config diagnostics remain readable when @reviewer is absent", async () => {
  const result = await handleReviewOperatorRequest(
    {
      method: "GET",
      url: new URL("http://crew/v1/admin/review-operator/config"),
      requestId: "request-no-reviewer",
    },
    {
      deploymentRole: "debug",
      authority: () => authority,
      diagnostics: () => diagnostics,
      refreshDiagnostics: async () => diagnostics,
      resolveReviewer: async () => {
        throw new Error("NotFound: agent_route_not_found");
      },
      readRuntimeConfigFile: async () => ({ value: {} }),
      writeRuntimeConfigFile: async () => undefined,
      applyRuntimeConfigFromDisk: async () => ({}),
      withRuntimeConfigMutation: (mutation) => mutation(),
      pipeline: async () => ({
        projectId: "rusty-view",
        deploymentRole: "debug",
        limit: 50,
        offset: 0,
        items: [],
      }),
      promptReviewer: async () => ({ status: "accepted" }) as never,
    },
  );
  if ("kind" in result || typeof result.body === "string") {
    throw new Error("expected JSON admin route result");
  }
  assert.equal(result.status, 200);
  assert.deepEqual(
    (result.body as { data: { reviewerRoute: unknown } }).data.reviewerRoute,
    {
      address: "@reviewer",
      routable: false,
      reasonCode: "agent_route_not_found",
    },
  );
});

test("runtime review config preserves environment credentials and explicit disablement", () => {
  assert.equal(reviewConfig({ reviewDenAuthority: null }, authority), null);
  assert.deepEqual(
    reviewConfig(
      {
        reviewDenAuthority: {
          authorityId: "runtime-authority",
          endpointRef: "config://mcp/den",
          auditIdentity: "runtime-audit",
        },
      },
      authority,
    ),
    {
      authorityId: "runtime-authority",
      endpointRef: "config://mcp/den",
      serverName: "den",
      toolProfileKey: "direct",
      auditIdentity: "runtime-audit",
      bearerToken: "must-not-leak",
    },
  );
});

test("composed pipeline preserves Den state and Crew retry diagnostics", async () => {
  const submission = {
    submissionId: "review-submission:abc",
    projectId: "rusty-view",
    taskId: "6854",
    phase: "reviewer_dispatch_pending",
    reviewerDispatchAttempts: 2,
    reviewerDispatchNextRetryAt: "2026-08-12T00:01:00.000Z",
    updatedAt: "2026-08-12T00:00:30.000Z",
  } as ReviewSubmissionRecord;
  const page = await composedReviewPipeline({
    bridge: {
      listReviewSubmissions: async () => [submission],
    },
    runtimeConfig: { mcpServers: [] },
    mcpConfig: { requestTimeoutMs: 1_000, servers: [] },
    authority,
    deploymentRole: "production",
    projectId: "rusty-view",
    limit: 50,
    offset: 0,
    callDenTool: async (name, args) => {
      assert.equal(name, "list_review_pipeline");
      assert.deepEqual(args, {
        project_id: "rusty-view",
        limit: 50,
        offset: 0,
      });
      return {
        items: [
          {
            task: { id: 6854, project_id: "rusty-view", status: "review" },
            latest_round: { id: 77, status: "open" },
            latest_gate: { id: 88, status: "passed" },
          },
          {
            task: { id: 6855, project_id: "rusty-view", status: "review" },
            latest_round: null,
            latest_gate: null,
          },
        ],
        limit: 50,
        offset: 0,
      };
    },
  });
  assert.equal(page.items[0]?.stableId, "review-submission:abc");
  assert.equal(page.items[0]?.stage, "reviewer_delivery_retrying");
  assert.equal(page.items[0]?.latestGate?.status, "passed");
  assert.equal(page.items[1]?.stableId, "den-task:rusty-view:6855");
  assert.equal(page.items[1]?.stage, "den_reviewable_not_submitted");
});

test("managed pipeline stages preserve every operator-visible transition", () => {
  const stage = (
    phase: ReviewSubmissionRecord["phase"],
    extra: Partial<ReviewSubmissionRecord> = {},
  ) =>
    managedSubmissionStage({
      submissionId: `review-submission:${phase}`,
      projectId: "rusty-view",
      taskId: "6854",
      phase,
      ...extra,
    } as ReviewSubmissionRecord);

  assert.equal(stage("submitted"), "managed_submission_accepted");
  assert.equal(stage("den_handoff_recorded"), "managed_submission_accepted");
  assert.equal(stage("gate_pending"), "github_gate_pending");
  assert.equal(
    stage("gate_failed", { gateStatus: "timed_out" }),
    "github_gate_timed_out",
  );
  assert.equal(stage("reviewer_dispatch_pending"), "reviewer_delivery_queued");
  assert.equal(
    stage("reviewer_dispatch_pending", { reviewerDispatchAttempts: 2 }),
    "reviewer_delivery_retrying",
  );
  assert.equal(stage("reviewer_dispatched"), "reviewer_dispatched");
  assert.equal(stage("den_finalization_pending"), "den_finalization_pending");
  assert.equal(stage("den_finalized"), "review_complete_reply_pending");
  assert.equal(stage("reply_pending"), "review_complete_reply_pending");
  assert.equal(stage("replied"), "review_complete_replied");
  assert.equal(stage("reply_terminal"), "reply_terminal");
  assert.equal(stage("review_terminal"), "review_terminal");
  assert.equal(stage("superseded"), "superseded");
});

test("manual reviewer route returns receipt-backed exact command", async () => {
  let promptInput: unknown;
  const result = await handleReviewOperatorRequest(
    {
      method: "POST",
      url: new URL(
        "http://crew/v1/admin/review-operator/tasks/6854/prompt-reviewer",
      ),
      requestId: "request-1",
      body: {
        ttlMs: 30_000,
        correlationId: "correlation-1",
        idempotencyKey: "idem-1",
        expectedDeploymentRole: "production",
      },
    },
    {
      deploymentRole: "production",
      authority: () => authority,
      diagnostics: () => diagnostics,
      refreshDiagnostics: async () => diagnostics,
      resolveReviewer: async () => reviewerRoute,
      readRuntimeConfigFile: async () => ({ value: {} }),
      writeRuntimeConfigFile: async () => undefined,
      applyRuntimeConfigFromDisk: async () => ({}),
      withRuntimeConfigMutation: (mutation) => mutation(),
      pipeline: async () => ({
        projectId: "rusty-view",
        deploymentRole: "production",
        limit: 50,
        offset: 0,
        items: [],
      }),
      promptReviewer: async (input) => {
        promptInput = input;
        return { status: "accepted" } as never;
      },
    },
  );
  if ("kind" in result || typeof result.body === "string") {
    throw new Error("expected JSON admin route result");
  }
  assert.deepEqual(promptInput, {
    taskId: 6854,
    ttlMs: 30_000,
    correlationId: "correlation-1",
    idempotencyKey: "idem-1",
  });
  assert.equal(
    (result.body as { data: { command: string; target: string } }).data.command,
    "review 6854",
  );
  assert.equal(
    (result.body as { data: { command: string; target: string } }).data.target,
    "@reviewer",
  );
});

test("config write rejects browser-supplied credentials", async () => {
  const result = await handleReviewOperatorRequest(
    {
      method: "PATCH",
      url: new URL("http://crew/v1/admin/review-operator/config"),
      requestId: "request-2",
      body: {
        expectedConfigRevision: "wrong-on-purpose",
        authorityId: "a",
        endpointRef: "config://mcp/den",
        bearerToken: "secret",
      },
    },
    {
      deploymentRole: "production",
      authority: () => authority,
      diagnostics: () => diagnostics,
      refreshDiagnostics: async () => diagnostics,
      resolveReviewer: async () => reviewerRoute,
      readRuntimeConfigFile: async () => ({ value: {} }),
      writeRuntimeConfigFile: async () => undefined,
      applyRuntimeConfigFromDisk: async () => ({}),
      withRuntimeConfigMutation: (mutation) => mutation(),
      pipeline: async () => ({
        projectId: "rusty-view",
        deploymentRole: "production",
        limit: 50,
        offset: 0,
        items: [],
      }),
      promptReviewer: async () => ({ status: "accepted" }) as never,
    },
  );
  if ("kind" in result || typeof result.body === "string") {
    throw new Error("expected JSON admin route result");
  }
  assert.equal(result.status, 400);
  assert.match(JSON.stringify(result.body), /server-managed/);
  assert.equal(JSON.stringify(result.body).includes("secret"), false);
});
