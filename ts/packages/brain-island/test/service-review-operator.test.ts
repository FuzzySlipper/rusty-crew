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

test("config read rejects a mismatched expected deployment role", async () => {
  const result = await handleReviewOperatorRequest(
    {
      method: "GET",
      url: new URL(
        "http://crew/v1/admin/review-operator/config?expectedDeploymentRole=debug",
      ),
      requestId: "request-wrong-role",
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
  assert.match(
    JSON.stringify(result.body),
    /expected debug deployment but reached production/,
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
      listReviewSubmissions: async (query) =>
        query?.taskId === "6855" ? [] : [submission],
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
        limit: 25,
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
        limit: 25,
        offset: 0,
      };
    },
  });
  assert.equal(page.items[0]?.stableId, "den-task:rusty-view:6855");
  assert.equal(page.items[0]?.stage, "den_reviewable_not_submitted");
  assert.equal(page.items[1]?.stableId, "review-submission:abc");
  assert.equal(page.items[1]?.stage, "reviewer_delivery_retrying");
  assert.equal(page.items[1]?.latestGate?.status, "passed");
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

test("composed pipeline pages Den-only tasks and Crew submissions once", async () => {
  const submissions = ["s1", "s2"].map(
    (submissionId, index) =>
      ({
        submissionId,
        projectId: "rusty-view",
        taskId: String(7001 + index),
        phase: "replied",
        updatedAt: `2026-08-12T00:00:0${index}.000Z`,
      }) as ReviewSubmissionRecord,
  );
  const seen: string[] = [];
  let offset = 0;
  for (let pageNumber = 0; pageNumber < 3; pageNumber += 1) {
    const page = await composedReviewPipeline({
      bridge: {
        listReviewSubmissions: async (query) => {
          if (query?.taskId !== undefined && query.taskId !== null) {
            return submissions.filter(
              (submission) => submission.taskId === query.taskId,
            );
          }
          const pageOffset = query?.offset ?? 0;
          const pageLimit = query?.limit ?? submissions.length;
          return submissions.slice(pageOffset, pageOffset + pageLimit);
        },
      },
      runtimeConfig: { mcpServers: [] },
      mcpConfig: { requestTimeoutMs: 1_000, servers: [] },
      authority,
      deploymentRole: "production",
      projectId: "rusty-view",
      limit: 2,
      offset,
      callDenTool: async (_name, args) => {
        const denOffset = args.offset as number;
        return denOffset === 0
          ? {
              items: [{ task: { id: 7001, status: "review" } }],
              next_offset: 1,
            }
          : { items: [{ task: { id: 7003, status: "review" } }] };
      },
    });
    seen.push(...page.items.map((item) => item.stableId));
    if (page.nextOffset === undefined) break;
    offset = page.nextOffset;
  }
  assert.deepEqual(seen, ["s1", "den-task:rusty-view:7003", "s2"]);
});

test("concurrent config writes serialize revision validation", async () => {
  let activeAuthority = authority;
  let file: Record<string, unknown> = {};
  let tail = Promise.resolve();
  const withRuntimeConfigMutation = <T>(mutation: () => Promise<T>) => {
    const result = tail.then(mutation, mutation);
    tail = result.then(
      () => undefined,
      () => undefined,
    );
    return result;
  };
  const context = {
    deploymentRole: "production" as const,
    authority: () => activeAuthority,
    diagnostics: () => diagnostics,
    refreshDiagnostics: async () => diagnostics,
    resolveReviewer: async () => reviewerRoute,
    readRuntimeConfigFile: async () => ({ value: structuredClone(file) }),
    writeRuntimeConfigFile: async (value: Record<string, unknown>) => {
      file = structuredClone(value);
    },
    applyRuntimeConfigFromDisk: async () => {
      const configured = file.reviewDenAuthority as typeof authority;
      activeAuthority = { ...configured, bearerToken: authority.bearerToken };
      return {};
    },
    withRuntimeConfigMutation,
    pipeline: async () => ({
      projectId: "rusty-view",
      deploymentRole: "production" as const,
      limit: 50,
      offset: 0,
      items: [],
    }),
    promptReviewer: async () => ({ status: "accepted" }) as never,
  };
  const expectedConfigRevision = reviewOperatorConfigReadback({
    deploymentRole: "production",
    authority,
    diagnostics,
    reviewerRoute,
  }).configRevision;
  const write = (authorityId: string) =>
    handleReviewOperatorRequest(
      {
        method: "PATCH",
        url: new URL("http://crew/v1/admin/review-operator/config"),
        requestId: authorityId,
        body: {
          expectedConfigRevision,
          authorityId,
          endpointRef: "config://mcp/den",
          auditIdentity: "review-service",
        },
      },
      context,
    );
  const results = await Promise.all([write("winner-a"), write("winner-b")]);
  assert.deepEqual(
    results.map((result) => ("status" in result ? result.status : 0)).sort(),
    [200, 409],
  );
});

test("failed config apply restores the prior file and runtime", async () => {
  const prior = {
    reviewDenAuthority: {
      authorityId: authority.authorityId,
      endpointRef: authority.endpointRef,
      auditIdentity: authority.auditIdentity,
    },
  };
  let file: Record<string, unknown> = structuredClone(prior);
  let applies = 0;
  const result = await handleReviewOperatorRequest(
    {
      method: "PATCH",
      url: new URL("http://crew/v1/admin/review-operator/config"),
      requestId: "rollback",
      body: {
        expectedConfigRevision: reviewOperatorConfigReadback({
          deploymentRole: "production",
          authority,
          diagnostics,
          reviewerRoute,
        }).configRevision,
        authorityId: "replacement",
        endpointRef: "config://mcp/den",
        auditIdentity: "review-service",
      },
    },
    {
      deploymentRole: "production",
      authority: () => authority,
      diagnostics: () => diagnostics,
      refreshDiagnostics: async () => diagnostics,
      resolveReviewer: async () => reviewerRoute,
      readRuntimeConfigFile: async () => ({ value: structuredClone(file) }),
      writeRuntimeConfigFile: async (value) => {
        file = structuredClone(value);
      },
      applyRuntimeConfigFromDisk: async () => {
        applies += 1;
        if (applies === 1) throw new Error("apply failed");
        return {};
      },
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
  assert.equal("status" in result ? result.status : 0, 400);
  assert.deepEqual(file, prior);
  assert.equal(applies, 2);
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
