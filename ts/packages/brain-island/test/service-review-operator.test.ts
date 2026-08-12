import assert from "node:assert/strict";
import test from "node:test";
import type { ReviewSubmissionRecord } from "@rusty-crew/contracts";
import {
  composedReviewPipeline,
  managedSubmissionStage,
  reviewOperatorConfigReadback,
  staleReviewTasks,
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

test("stale review discovery returns only old gate-passed unsubmitted work", async () => {
  const old = "2026-08-12T00:00:00.000Z";
  const recent = "2026-08-12T00:59:00.000Z";
  const round = (taskId: number, extra: Record<string, unknown> = {}) => ({
    id: taskId + 100,
    task_id: taskId,
    head_commit: `head-${taskId}`,
    requested_at: old,
    ...extra,
  });
  const gate = (taskId: number, extra: Record<string, unknown> = {}) => ({
    id: taskId + 200,
    task_id: taskId,
    commit_sha: `head-${taskId}`,
    status: "passed",
    completed_at: old,
    updated_at: old,
    ...extra,
  });
  const item = (
    projectId: string,
    taskId: number,
    extra: Record<string, unknown> = {},
  ) => ({
    task: {
      id: taskId,
      project_id: projectId,
      status: "review",
      updated_at: old,
    },
    latest_round: round(taskId),
    latest_gate: gate(taskId),
    ...extra,
  });
  const calls: Array<{ name: string; args: Record<string, unknown> }> = [];
  const result = await staleReviewTasks({
    bridge: {
      listReviewSubmissions: async (query) =>
        query?.taskId === "7"
          ? [
              {
                submissionId: "active-7",
                projectId: "alpha",
                taskId: "7",
                commitSha: "head-7",
                phase: "reviewer_dispatched",
              } as ReviewSubmissionRecord,
            ]
          : query?.taskId === "1"
            ? [
                {
                  submissionId: "historical-1",
                  projectId: "alpha",
                  taskId: "1",
                  commitSha: "older-head",
                  phase: "review_terminal",
                } as ReviewSubmissionRecord,
              ]
            : [],
    },
    runtimeConfig: { mcpServers: [] },
    mcpConfig: { requestTimeoutMs: 1_000, servers: [] },
    authority,
    deploymentRole: "production",
    projectIds: [],
    staleMs: 300_000,
    now: "2026-08-12T01:00:00.000Z",
    callDenTool: async (name, args) => {
      calls.push({ name, args });
      if (name === "list_projects") {
        return { items: [{ id: "beta" }, { id: "alpha" }] };
      }
      assert.equal(name, "list_review_pipeline");
      if (args.project_id === "beta") {
        return { items: [item("beta", 9)] };
      }
      if (args.offset === 0) {
        return {
          items: [
            item("alpha", 1),
            item("alpha", 2, {
              latest_gate: gate(2, { updated_at: recent }),
            }),
            item("alpha", 3, {
              latest_gate: gate(3, { status: "pending" }),
            }),
            item("alpha", 4, {
              latest_gate: gate(4, { status: "failed" }),
            }),
          ],
          next_offset: 4,
        };
      }
      assert.equal(args.offset, 4);
      return {
        items: [
          item("alpha", 5, {
            latest_round: round(5, { verdict: "looks_good" }),
          }),
          item("alpha", 6, { latest_gate: null }),
          item("alpha", 7),
          item("alpha", 8, {
            latest_gate: gate(8, { commit_sha: "wrong-head" }),
          }),
        ],
      };
    },
  });

  assert.deepEqual(result, [
    { projectId: "alpha", taskId: 1 },
    { projectId: "beta", taskId: 9 },
  ]);
  assert.equal(calls[0]?.name, "list_projects");
  assert.ok(calls.some((call) => call.args.offset === 4));
});

test("stale review discovery honors explicit project filters", async () => {
  const result = await staleReviewTasks({
    bridge: { listReviewSubmissions: async () => [] },
    runtimeConfig: { mcpServers: [] },
    mcpConfig: { requestTimeoutMs: 1_000, servers: [] },
    authority,
    deploymentRole: "production",
    projectIds: ["beta", "beta"],
    staleMs: 0,
    now: "2026-08-12T01:00:00.000Z",
    callDenTool: async (name, args) => {
      assert.equal(name, "list_review_pipeline");
      assert.equal(args.project_id, "beta");
      return { items: [] };
    },
  });
  assert.deepEqual(result, []);
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

test("limit-one pipeline drains Den and Crew without dropping either authority", async () => {
  const collect = async (
    denTaskIds: number[],
    submissions: ReviewSubmissionRecord[],
  ): Promise<string[]> => {
    const seen: string[] = [];
    let offset = 0;
    for (let pageNumber = 0; pageNumber < 10; pageNumber += 1) {
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
        limit: 1,
        offset,
        callDenTool: async (_name, args) => {
          const denOffset = args.offset as number;
          const taskId = denTaskIds[denOffset];
          return {
            items:
              taskId === undefined
                ? []
                : [{ task: { id: taskId, status: "review" } }],
            ...(denOffset + 1 < denTaskIds.length
              ? { next_offset: denOffset + 1 }
              : {}),
          };
        },
      });
      assert.ok(page.items.length <= 1);
      seen.push(...page.items.map((item) => item.stableId));
      if (page.nextOffset === undefined) return seen;
      offset = page.nextOffset;
    }
    throw new Error("limit-one pipeline did not terminate");
  };
  const crew = ["s1", "s2"].map(
    (submissionId, index) =>
      ({
        submissionId,
        projectId: "rusty-view",
        taskId: String(7101 + index),
        phase: "replied",
      }) as ReviewSubmissionRecord,
  );

  assert.deepEqual(await collect([], crew), ["s1", "s2"]);
  assert.deepEqual(await collect([7201, 7202], []), [
    "den-task:rusty-view:7201",
    "den-task:rusty-view:7202",
  ]);
  assert.deepEqual(await collect([7301], crew.slice(0, 1)), [
    "den-task:rusty-view:7301",
    "s1",
  ]);
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

test("stale review route accepts repeatable projects and a stale duration", async () => {
  let observed: unknown;
  const result = await handleReviewOperatorRequest(
    {
      method: "GET",
      url: new URL(
        "http://crew/v1/admin/review-operator/stale-review-tasks?projectId=beta&projectId=alpha&staleMs=60000&expectedDeploymentRole=production",
      ),
      requestId: "stale-review-tasks",
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
        projectId: "alpha",
        deploymentRole: "production",
        limit: 50,
        offset: 0,
        items: [],
      }),
      staleTasks: async (input) => {
        observed = input;
        return [{ projectId: "alpha", taskId: 42 }];
      },
      promptReviewer: async () => ({ status: "accepted" }) as never,
    },
  );
  if ("kind" in result || typeof result.body === "string") {
    throw new Error("expected JSON admin route result");
  }
  assert.deepEqual(observed, {
    projectIds: ["beta", "alpha"],
    staleMs: 60_000,
  });
  assert.deepEqual((result.body as { data: unknown }).data, [
    { projectId: "alpha", taskId: 42 },
  ]);
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
