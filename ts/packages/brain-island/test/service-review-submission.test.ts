import assert from "node:assert/strict";
import test from "node:test";

import type {
  AgentRouteResolution,
  ReviewSubmissionRecord,
} from "@rusty-crew/contracts";
import {
  createServiceReviewSubmissionRuntime,
  denReviewRequestByteLength,
  parseExternalReviewRecoveryRequest,
  parseExternalReviewSubmissionRequest,
  reconcileReviewSubmissionNow,
  reconcileReviewSubmissions,
  reviewSubmissionRecoveryDiagnostics,
  reviewerDispatchIdentity,
  selectReviewDenBinding,
  selectRoutedReviewRecord,
  submitExternalReview,
} from "../src/service-review-submission.js";

function reviewServiceConfig(
  deploymentRole: "production" | "debug" = "production",
) {
  return {
    deploymentRole,
    reviewDenAuthority: {
      authorityId: "service-review-den",
      endpointRef: "config://mcp/den",
      serverName: "den",
      toolProfileKey: "direct",
      auditIdentity: "rusty-crew-review-service",
    },
  } as never;
}

test("Den finalization request sizing matches the 4096-byte boundary", () => {
  const request = {
    review_round_id: 4150,
    verdict: "changes_requested",
    decided_by: "@reviewer",
    notes: "",
  };
  const emptyBytes = denReviewRequestByteLength(request);
  const atLimit = {
    ...request,
    notes: "x".repeat(4_096 - emptyBytes),
  };
  assert.equal(denReviewRequestByteLength(atLimit), 4_096);
  assert.equal(
    denReviewRequestByteLength({ ...atLimit, notes: `${atLimit.notes}x` }),
    4_097,
  );
  assert.equal(
    denReviewRequestByteLength({ notes: "<>&\u2028\u2029" }),
    new TextEncoder().encode('{"notes":"\\u003c\\u003e\\u0026\\u2028\\u2029"}')
      .length,
  );
});

test("oversized managed review result is rejected before persistence", async () => {
  const pending = {
    ...scopedSubmissionRecord("rusty-crew", {
      type: "review_submission",
      submissionId: "review-oversized",
    }),
    submissionId: "review-oversized",
    phase: "reviewer_dispatched",
    reviewerSessionId: "reviewer-session",
    reviewRoundId: 4150,
    revision: 7,
  } as ReviewSubmissionRecord;
  const transitions: unknown[] = [];
  const runtime = createServiceReviewSubmissionRuntime(() => ({
    bridge: {
      listReviewSubmissions: async () => [pending],
      transitionReviewSubmission: async (request: unknown) => {
        transitions.push(request);
        throw new Error("oversized result must not transition");
      },
    } as never,
    runtimeConfig: {
      sessions: [
        {
          sessionId: "reviewer-session",
          agentId: "reviewer-agent",
          profileId: "reviewer",
        },
      ],
      mcpBindings: [],
      mcpServers: [],
    } as never,
    serviceConfig: { deploymentRole: "production" } as never,
    now: () => "2026-08-08T10:00:00.000Z",
    applyCoordinationDelivery: async (receipt: never) => receipt,
  }));

  const result = await runtime.complete({
    verdict: "changes_requested",
    taskId: 6662,
    commitSha: pending.commitSha,
    notes: "x".repeat(4_096),
    caller: { type: "review_submission", submissionId: pending.submissionId },
    reviewerSessionId: "reviewer-session",
  });

  assert.equal(result.ok, false);
  assert.equal(result.reasonCode, "review_result_too_large");
  assert.match(result.summary, /no review result was persisted/);
  assert.deepEqual(transitions, []);
});

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
  assert.equal(
    reviewerDispatchIdentity(
      "review-1",
      resolution,
      "review-delivery:original",
    ),
    "review-1:route-3:binding-14:recovery-390f042ab9e80e4e71441196a4a0ed205a2a1104991a355b31d0dfd9e4593a2b",
  );
  assert.equal(
    reviewerDispatchIdentity(
      "review-1",
      { ...resolution, route: { ...resolution.route!, revision: 99 } },
      "review-delivery:original",
    ),
    "review-1:route-99:binding-14:recovery-390f042ab9e80e4e71441196a4a0ed205a2a1104991a355b31d0dfd9e4593a2b",
  );
});

test("disabled reviewer route stops durable churn after one bounded generation", async () => {
  let nowMs = Date.parse("2026-08-11T10:00:00.000Z");
  let durable = {
    ...scopedSubmissionRecord("rusty-crew", {
      type: "external_cli" as const,
      clientId: "incident-regression",
      idempotencyKey: "disabled-route",
    }),
    phase: "reviewer_dispatch_pending" as const,
    revision: 4,
  } as ReviewSubmissionRecord;
  let transitions = 0;
  const context = {
    bridge: {
      listReviewSubmissions: async () => [durable],
      resolveAgentAddress: async () => ({
        address: "@reviewer",
        routable: false,
        reasonCode: "route_disabled",
        route: { revision: 9 },
      }),
      transitionReviewSubmission: async (request: {
        transition: { type: string; retryGeneration: string };
      }) => {
        assert.equal(request.transition.type, "reviewer_dispatch_failed");
        transitions += 1;
        const attempts =
          durable.reviewerDispatchGeneration ===
          request.transition.retryGeneration
            ? (durable.reviewerDispatchAttempts ?? 0) + 1
            : 1;
        durable = {
          ...durable,
          reviewerDispatchAttempts: attempts,
          reviewerDispatchGeneration: request.transition.retryGeneration,
          reviewerDispatchNextRetryAt:
            attempts >= 6
              ? null
              : new Date(nowMs + 30_000 * 2 ** (attempts - 1)).toISOString(),
          revision: durable.revision + 1,
        } as ReviewSubmissionRecord;
        return durable;
      },
    } as never,
    runtimeConfig: { sessions: [], mcpBindings: [], mcpServers: [] } as never,
    serviceConfig: reviewServiceConfig(),
    now: () => new Date(nowMs).toISOString(),
    callDenTool: activeTaskDenCall(() => durable),
    applyCoordinationDelivery: async (receipt: never) => receipt,
  };

  for (let sweep = 0; sweep < 40; sweep += 1) {
    await reconcileReviewSubmissions(context);
    nowMs += 60 * 60_000;
  }

  assert.equal(transitions, 6);
  assert.equal(durable.reviewerDispatchAttempts, 6);
  assert.equal(durable.reviewerDispatchNextRetryAt, null);
});

test("startup incident reconciliation retires twelve terminal tasks and dispatches one active review", async () => {
  let records = Array.from({ length: 13 }, (_, index) => ({
    ...scopedSubmissionRecord("rusty-crew", {
      type: "external_cli" as const,
      clientId: "incident-regression",
      idempotencyKey: `incident-${index}`,
    }),
    submissionId: `incident-review-${index}`,
    taskId: String(7_000 + index),
    phase: "reviewer_dispatch_pending" as const,
    revision: 3,
  })) as ReviewSubmissionRecord[];
  const terminalTransitions: string[] = [];
  const deliveries: Array<{
    deliveryId: string;
    createdAt: string;
    expiresAt: string;
  }> = [];
  const context = {
    bridge: {
      listReviewSubmissions: async () => records,
      transitionReviewSubmission: async (request: {
        submissionId: string;
        transition: { type: string; terminalReason?: string };
      }) => {
        const index = records.findIndex(
          (record) => record.submissionId === request.submissionId,
        );
        const current = records[index]!;
        if (request.transition.type === "den_task_terminal") {
          terminalTransitions.push(request.transition.terminalReason!);
          records[index] = {
            ...current,
            phase: "superseded",
            terminalReason: request.transition.terminalReason,
            revision: current.revision + 1,
          } as ReviewSubmissionRecord;
        } else if (request.transition.type === "reviewer_dispatched") {
          records[index] = {
            ...current,
            phase: "reviewer_dispatched",
            revision: current.revision + 1,
          } as ReviewSubmissionRecord;
        } else {
          assert.fail(`unexpected transition ${request.transition.type}`);
        }
        return records[index]!;
      },
      resolveAgentAddress: async () => ({
        address: "@reviewer",
        routable: true,
        route: { revision: 11 },
        resolvedTarget: {
          bindingRevision: 4,
          sessionId: "reviewer-session",
        },
      }),
      getAgentMessageDelivery: async () => undefined,
      deliverAgentMessage: async (request: {
        deliveryId: string;
        messageId: string;
        createdAt: string;
        expiresAt: string;
      }) => {
        deliveries.push(request);
        return {
          status: "accepted",
          request: { ...request, toSessionId: "reviewer-session" },
        };
      },
    } as never,
    runtimeConfig: { sessions: [], mcpBindings: [], mcpServers: [] } as never,
    serviceConfig: reviewServiceConfig(),
    now: () => "2026-08-11T11:00:00.000Z",
    callDenTool: async (
      _binding: unknown,
      toolName: string,
      args: Record<string, unknown>,
    ) => {
      if (toolName === "list_review_rounds") return { items: [] };
      if (toolName === "get_task") {
        const taskId = Number(args.task_id);
        return {
          id: taskId,
          project_id: "rusty-crew",
          status: taskId === 7_012 ? "in_progress" : "done",
        };
      }
      throw new Error(`unexpected Den tool ${toolName}`);
    },
    applyCoordinationDelivery: async (receipt: never) => receipt,
  };

  await reconcileReviewSubmissions(context);

  assert.equal(terminalTransitions.length, 12);
  assert.ok(
    terminalTransitions.every(
      (reason) => reason === "automatic_den_task_already_done",
    ),
  );
  assert.equal(deliveries.length, 1);
  assert.equal(
    Date.parse(deliveries[0]!.expiresAt) - Date.parse(deliveries[0]!.createdAt),
    24 * 60 * 60_000,
  );
  assert.equal(
    records.filter((record) => record.phase === "superseded").length,
    12,
  );
  assert.equal(records[12]!.phase, "reviewer_dispatched");
});

test("operator reconciliation is revision-guarded and recovery diagnostics are bounded", async () => {
  let durable = {
    ...scopedSubmissionRecord("rusty-crew", {
      type: "external_cli" as const,
      clientId: "operator",
      idempotencyKey: "reconcile-terminal",
    }),
    phase: "reviewer_dispatch_pending" as const,
    submissionId: `review-submission:${"c".repeat(64)}`,
    revision: 8,
  } as ReviewSubmissionRecord;
  let transitions = 0;
  const context = {
    bridge: {
      listReviewSubmissions: async ({
        pendingOnly,
      }: {
        pendingOnly: boolean;
      }) => (pendingOnly && durable.phase === "superseded" ? [] : [durable]),
      getReviewSubmission: async () => durable,
      transitionReviewSubmission: async (request: {
        transition: { type: string; terminalReason: string };
      }) => {
        transitions += 1;
        durable = {
          ...durable,
          phase: "superseded",
          terminalReason: request.transition.terminalReason,
          revision: durable.revision + 1,
        } as ReviewSubmissionRecord;
        return durable;
      },
    } as never,
    runtimeConfig: { sessions: [], mcpBindings: [], mcpServers: [] } as never,
    serviceConfig: reviewServiceConfig(),
    now: () => "2026-08-11T12:00:00.000Z",
    callDenTool: async (_binding: unknown, toolName: string) =>
      toolName === "list_review_rounds"
        ? { items: [] }
        : {
            id: Number(durable.taskId),
            project_id: durable.projectId,
            status: "done",
          },
    applyCoordinationDelivery: async (receipt: never) => receipt,
  };

  const reconciled = await reconcileReviewSubmissionNow(
    context,
    durable.submissionId,
    { expectedRevision: 8 },
  );
  assert.equal(reconciled.phase, "superseded");
  assert.equal(transitions, 1);
  await reconcileReviewSubmissionNow(context, durable.submissionId, {
    expectedRevision: 9,
  });
  assert.equal(transitions, 1);

  const diagnostics = await reviewSubmissionRecoveryDiagnostics(context);
  assert.equal(diagnostics.pendingSubmissionCount, 0);
  assert.equal(diagnostics.terminalReconciliations, 1);
  assert.equal(diagnostics.submissions.length, 1);
});

test("completed reviewer turn is redispatched once with a new durable identity", async () => {
  let durable = {
    ...scopedSubmissionRecord("rusty-crew", {
      type: "external_cli" as const,
      clientId: "external-review",
      idempotencyKey: "recover-review",
    }),
    submissionId: `review-submission:${"a".repeat(64)}`,
    phase: "reviewer_dispatched" as const,
    reviewerSessionId: "reviewer-session",
    dispatchMessageId: "review-message:original",
    dispatchDeliveryId: "review-delivery:original",
    revision: 5,
  } as ReviewSubmissionRecord;
  const deliveredIds: string[] = [];
  const transitionTypes: string[] = [];
  const resolution = {
    address: "@reviewer",
    routable: true,
    route: { revision: 4 },
    resolvedTarget: {
      bindingRevision: 5,
      sessionId: "reviewer-session",
    },
  } as AgentRouteResolution;
  const context = {
    bridge: {
      listReviewSubmissions: async () => [durable],
      listAgentMessageInbox: async () => [
        {
          status:
            durable.dispatchDeliveryId === "review-delivery:original"
              ? "awaiting_reply"
              : "in_progress",
          delivery: {
            request: { deliveryId: durable.dispatchDeliveryId },
          },
        },
      ],
      transitionReviewSubmission: async (request: {
        transition: {
          type: string;
          reviewerSessionId?: string;
          dispatchMessageId?: string;
          dispatchDeliveryId?: string;
        };
      }) => {
        transitionTypes.push(request.transition.type);
        durable = {
          ...durable,
          phase:
            request.transition.type === "reviewer_redispatch_pending"
              ? "reviewer_dispatch_pending"
              : "reviewer_dispatched",
          reviewerSessionId:
            request.transition.reviewerSessionId ?? durable.reviewerSessionId,
          dispatchMessageId:
            request.transition.dispatchMessageId ?? durable.dispatchMessageId,
          dispatchDeliveryId:
            request.transition.dispatchDeliveryId ?? durable.dispatchDeliveryId,
          revision: durable.revision + 1,
        } as ReviewSubmissionRecord;
        return durable;
      },
      resolveAgentAddress: async () => resolution,
      getAgentMessageDelivery: async () => undefined,
      deliverAgentMessage: async (request: {
        deliveryId: string;
        messageId: string;
      }) => {
        deliveredIds.push(request.deliveryId);
        return {
          status: "accepted",
          revision: 1,
          request: {
            ...request,
            toSessionId: "reviewer-session",
          },
        };
      },
    } as never,
    runtimeConfig: { sessions: [], mcpBindings: [], mcpServers: [] } as never,
    serviceConfig: reviewServiceConfig(),
    now: () => "2026-08-10T12:00:00.000Z",
    callDenTool: activeTaskDenCall(() => durable),
    applyCoordinationDelivery: async (receipt: never) => receipt,
  };

  await reconcileReviewSubmissions(context);
  await reconcileReviewSubmissions(context);
  await reconcileReviewSubmissions(context);

  assert.deepEqual(transitionTypes, [
    "reviewer_redispatch_pending",
    "reviewer_dispatched",
  ]);
  assert.equal(deliveredIds.length, 1);
  assert.match(deliveredIds[0]!, /:recovery-[0-9a-f]{64}$/);
  assert.notEqual(deliveredIds[0], "review-delivery:original");
});

test("ambiguous accepted recovery resumes one stable delivery after revision advances", async () => {
  let durable = {
    ...scopedSubmissionRecord("rusty-crew", {
      type: "external_cli" as const,
      clientId: "external-review",
      idempotencyKey: "ambiguous-recovery",
    }),
    submissionId: `review-submission:${"b".repeat(64)}`,
    phase: "reviewer_dispatch_pending" as const,
    reviewerSessionId: "reviewer-session",
    dispatchMessageId: "review-message:original",
    dispatchDeliveryId: "review-delivery:original",
    revision: 6,
  } as ReviewSubmissionRecord;
  let storedDelivery: Record<string, unknown> | undefined;
  let applyAttempts = 0;
  const deliveredIds: string[] = [];
  const lookedUpIds: string[] = [];
  const transitionTypes: string[] = [];
  const context = {
    bridge: {
      listReviewSubmissions: async () => [durable],
      resolveAgentAddress: async () => ({
        address: "@reviewer",
        routable: true,
        route: { revision: 4 },
        resolvedTarget: {
          bindingRevision: 5,
          sessionId: "reviewer-session",
        },
      }),
      getAgentMessageDelivery: async (deliveryId: string) => {
        lookedUpIds.push(deliveryId);
        return storedDelivery;
      },
      deliverAgentMessage: async (request: {
        deliveryId: string;
        messageId: string;
      }) => {
        deliveredIds.push(request.deliveryId);
        storedDelivery = {
          status: "accepted",
          revision: 1,
          request: { ...request, toSessionId: "reviewer-session" },
        };
        return storedDelivery;
      },
      transitionReviewSubmission: async (request: {
        transition: {
          type: string;
          reviewerSessionId?: string;
          dispatchMessageId?: string;
          dispatchDeliveryId?: string;
        };
      }) => {
        transitionTypes.push(request.transition.type);
        durable = {
          ...durable,
          phase:
            request.transition.type === "reviewer_dispatched"
              ? "reviewer_dispatched"
              : durable.phase,
          reviewerSessionId:
            request.transition.reviewerSessionId ?? durable.reviewerSessionId,
          dispatchMessageId:
            request.transition.dispatchMessageId ?? durable.dispatchMessageId,
          dispatchDeliveryId:
            request.transition.dispatchDeliveryId ?? durable.dispatchDeliveryId,
          revision: durable.revision + 1,
        } as ReviewSubmissionRecord;
        return durable;
      },
    } as never,
    runtimeConfig: { sessions: [], mcpBindings: [], mcpServers: [] } as never,
    serviceConfig: reviewServiceConfig(),
    now: () => "2026-08-10T12:00:00.000Z",
    callDenTool: activeTaskDenCall(() => durable),
    applyCoordinationDelivery: async (receipt: never) => {
      applyAttempts += 1;
      if (applyAttempts === 1) {
        throw new Error("ambiguous post-accept delivery failure");
      }
      return receipt;
    },
  };

  await reconcileReviewSubmissions(context);
  assert.equal(durable.phase, "reviewer_dispatch_pending");
  assert.equal(durable.revision, 7);
  await reconcileReviewSubmissions(context);

  assert.equal(durable.phase, "reviewer_dispatched");
  assert.deepEqual(transitionTypes, [
    "reviewer_dispatch_failed",
    "reviewer_dispatched",
  ]);
  assert.equal(deliveredIds.length, 1);
  assert.deepEqual(lookedUpIds, [deliveredIds[0], deliveredIds[0]]);
  assert.equal(applyAttempts, 2);
  assert.equal(durable.dispatchDeliveryId, deliveredIds[0]);
});

test("stale accepted reviewer turn with no native claim returns to dispatch pending", async () => {
  const pending = {
    ...scopedSubmissionRecord("rusty-crew", {
      type: "external_cli" as const,
      clientId: "external-review",
      idempotencyKey: "accepted-unclaimed",
    }),
    phase: "reviewer_dispatched" as const,
    reviewerSessionId: "reviewer-session",
    dispatchMessageId: "review-message:unclaimed",
    dispatchDeliveryId: "review-delivery:unclaimed",
    revision: 8,
  } as ReviewSubmissionRecord;
  const transitions: unknown[] = [];
  await reconcileReviewSubmissions({
    bridge: {
      listReviewSubmissions: async () => [pending],
      listAgentMessageInbox: async () => [
        {
          status: "in_progress",
          externalTurnRequestId: "turn-unclaimed",
          delivery: {
            request: { deliveryId: "review-delivery:unclaimed" },
          },
        },
      ],
      getExternalTurn: async () => ({
        phase: "accepted",
        nativeTurnId: undefined,
        updatedAt: "2026-08-10T11:58:00.000Z",
      }),
      transitionReviewSubmission: async (request: unknown) => {
        transitions.push(request);
        return {
          ...pending,
          phase: "reviewer_dispatch_pending",
          revision: pending.revision + 1,
        };
      },
    } as never,
    runtimeConfig: { sessions: [], mcpBindings: [], mcpServers: [] } as never,
    serviceConfig: reviewServiceConfig(),
    now: () => "2026-08-10T12:00:00.000Z",
    callDenTool: activeTaskDenCall(() => pending),
    applyCoordinationDelivery: async (receipt: never) => receipt,
  });

  assert.equal(transitions.length, 1);
  assert.equal(
    (transitions[0] as { transition: { type: string } }).transition.type,
    "reviewer_redispatch_pending",
  );
  assert.equal(
    (transitions[0] as { transition: { reasonCode: string } }).transition
      .reasonCode,
    "reviewer_turn_accepted_unclaimed",
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
    serviceConfig: reviewServiceConfig(),
  } as never;

  assert.equal(
    selectReviewDenBinding(context, "session-1")?.bindingId,
    "session-den",
  );
});

test("managed reviews fall back to the dedicated service Den authority", () => {
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
    serviceConfig: reviewServiceConfig(),
  } as never;

  assert.equal(
    selectReviewDenBinding(context, "session-1")?.bindingId,
    "service-review-den",
  );
  assert.equal(
    selectReviewDenBinding(context, "missing")?.bindingId,
    "service-review-den",
  );
});

test("managed reviews remain available with zero sessions and bindings", () => {
  const context = {
    runtimeConfig: {
      sessions: [],
      mcpBindings: [
        { bindingId: "service-den", status: "inactive", serverNames: ["den"] },
      ],
    },
    serviceConfig: reviewServiceConfig(),
  } as never;

  assert.equal(
    selectReviewDenBinding(context, "missing")?.bindingId,
    "service-review-den",
  );
});

test("reconciliation settles the exact current Den round finalized outside Crew", async () => {
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
    serviceConfig: reviewServiceConfig(),
    now: () => "2026-08-08T09:30:00.000Z",
    callDenTool: async (_binding: unknown, toolName: string) => {
      if (toolName === "get_task") {
        return {
          id: Number(pending.taskId),
          project_id: pending.projectId,
          status: "done",
        };
      }
      assert.equal(toolName, "list_review_rounds");
      return {
        items: [
          {
            id: 4089,
            project_id: "rusty-crew",
            task_id: Number(pending.taskId),
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
        taskStatus: "done",
        terminalReason: "den_round_already_finalized",
      },
      now: "2026-08-08T09:30:00.000Z",
    },
  ]);
});

test("restart reconciliation records an independently finalized round exactly once", async () => {
  let durable = {
    ...scopedSubmissionRecord("rusty-crew", {
      type: "external_cli" as const,
      clientId: "test",
      idempotencyKey: "restart-exact-once",
    }),
    phase: "den_finalization_pending" as const,
    reviewRoundId: 4089,
    reviewResultJson: JSON.stringify({ verdict: "looks_good" }),
  } as ReviewSubmissionRecord;
  let transitions = 0;
  let denReads = 0;
  const context = {
    bridge: {
      listReviewSubmissions: async ({
        pendingOnly,
      }: {
        pendingOnly: boolean;
      }) =>
        pendingOnly && durable.phase === "review_terminal" ? [] : [durable],
      transitionReviewSubmission: async (request: {
        transition: { type: string; verdict?: string };
      }) => {
        transitions += 1;
        durable = {
          ...durable,
          phase: "review_terminal",
          reviewVerdict: request.transition.verdict,
          revision: durable.revision + 1,
        } as ReviewSubmissionRecord;
        return durable;
      },
    } as never,
    runtimeConfig: { sessions: [], mcpBindings: [], mcpServers: [] } as never,
    serviceConfig: reviewServiceConfig(),
    now: () => "2026-08-08T09:30:00.000Z",
    callDenTool: async (_authority: unknown, toolName: string) => {
      denReads += 1;
      if (toolName === "get_task") {
        return {
          id: Number(durable.taskId),
          project_id: durable.projectId,
          status: "done",
        };
      }
      assert.equal(toolName, "list_review_rounds");
      return {
        items: [
          {
            id: 4089,
            project_id: "rusty-crew",
            task_id: Number(durable.taskId),
            verdict: "looks_good",
          },
        ],
      };
    },
    applyCoordinationDelivery: async (receipt: never) => receipt,
  };

  await reconcileReviewSubmissions(context);
  await reconcileReviewSubmissions(context);
  assert.equal(denReads, 2);
  assert.equal(transitions, 1);
  assert.equal(durable.phase, "review_terminal");
});

test("pointer-first finalization receipt is verified against current Den round and task", async () => {
  let durable = {
    ...scopedSubmissionRecord("rusty-crew", {
      type: "external_cli" as const,
      clientId: "test",
      idempotencyKey: "pointer-first-finalization",
    }),
    phase: "den_finalization_pending" as const,
    reviewRoundId: 4743,
    reviewResultJson: JSON.stringify({ verdict: "looks_good" }),
    reviewerSessionId: "reviewer-session",
  } as ReviewSubmissionRecord;
  let finalized = false;
  const calls: string[] = [];
  const transitions: Array<Record<string, unknown>> = [];
  const runtime = createServiceReviewSubmissionRuntime(() => ({
    bridge: {
      listReviewSubmissions: async () => [durable],
      transitionReviewSubmission: async (request: {
        transition: Record<string, unknown>;
      }) => {
        transitions.push(request.transition);
        if (request.transition.type === "den_finalized") {
          durable = {
            ...durable,
            phase: "den_finalized",
            reviewFinalizationId: request.transition.finalizationId,
            reviewPacketId: request.transition.packetId,
            reviewPacketMessageId: request.transition.packetMessageId,
            reviewExactHeadCommit: request.transition.exactHeadCommit,
            reviewVerdict: request.transition.verdict,
            reviewTaskStatus: request.transition.taskStatus,
            revision: durable.revision + 1,
          } as ReviewSubmissionRecord;
        } else if (request.transition.type === "review_terminal") {
          durable = {
            ...durable,
            phase: "review_terminal",
            revision: durable.revision + 1,
          } as ReviewSubmissionRecord;
        } else {
          assert.fail(`unexpected transition ${request.transition.type}`);
        }
        return durable;
      },
    } as never,
    runtimeConfig: { sessions: [], mcpBindings: [], mcpServers: [] } as never,
    serviceConfig: reviewServiceConfig(),
    now: () => "2026-08-15T01:42:59.669Z",
    callDenTool: async (_authority: unknown, toolName: string) => {
      calls.push(toolName);
      if (toolName === "list_review_rounds") {
        return {
          items: [
            {
              id: 4743,
              project_id: durable.projectId,
              task_id: Number(durable.taskId),
              verdict: finalized ? "looks_good" : null,
            },
          ],
        };
      }
      if (toolName === "finalize_review") {
        finalized = true;
        return {
          schema: "den_review.review_completion_receipt.v1",
          schema_version: 1,
          id: 901,
          project_id: durable.projectId,
          task_id: Number(durable.taskId),
          review_round_id: 4743,
          verdict: "looks_good",
          state: "complete",
          packet_id: 902,
          packet_message_id: 903,
          resulting_task_status: "done",
        };
      }
      if (toolName === "get_task") {
        return {
          id: Number(durable.taskId),
          project_id: durable.projectId,
          status: "done",
        };
      }
      assert.fail(`unexpected Den tool ${toolName}`);
    },
    applyCoordinationDelivery: async (receipt: never) => receipt,
  }));

  const result = await runtime.complete({
    verdict: "looks_good",
    taskId: Number(durable.taskId),
    commitSha: durable.commitSha,
    caller: { type: "review_submission", submissionId: durable.submissionId },
    reviewerSessionId: "reviewer-session",
  });

  assert.equal(result.ok, true);
  assert.equal(result.exactHeadCommit, durable.commitSha);
  assert.equal(result.taskStatus, "done");
  assert.deepEqual(calls, [
    "list_review_rounds",
    "finalize_review",
    "list_review_rounds",
    "get_task",
  ]);
  assert.deepEqual(
    transitions.map((transition) => transition.type),
    ["den_finalized", "review_terminal"],
  );
  assert.equal("exact_head_commit" in transitions[0]!, false);
  assert.equal(transitions[0]!.exactHeadCommit, durable.commitSha);
});

test("a durable result survives authority loss after dispatch and reconciles after restoration", async () => {
  let authorityAvailable = false;
  let durable = {
    ...scopedSubmissionRecord("rusty-crew", {
      type: "external_cli" as const,
      clientId: "test",
      idempotencyKey: "authority-restoration",
    }),
    phase: "den_finalization_pending" as const,
    reviewRoundId: 4089,
    reviewResultJson: JSON.stringify({ verdict: "looks_good" }),
  } as ReviewSubmissionRecord;
  const transitionTypes: string[] = [];
  const context = {
    bridge: {
      listReviewSubmissions: async () =>
        durable.phase === "review_terminal" ? [] : [durable],
      transitionReviewSubmission: async (request: {
        transition: { type: string; verdict?: string };
      }) => {
        transitionTypes.push(request.transition.type);
        durable = {
          ...durable,
          ...(request.transition.type === "den_already_finalized"
            ? {
                phase: "review_terminal" as const,
                reviewVerdict: request.transition.verdict,
              }
            : {}),
          revision: durable.revision + 1,
          updatedAt: "2026-08-05T00:00:00.000Z",
        } as ReviewSubmissionRecord;
        return durable;
      },
    } as never,
    runtimeConfig: { sessions: [], mcpBindings: [], mcpServers: [] } as never,
    serviceConfig: reviewServiceConfig(),
    now: () => "2026-08-08T09:30:00.000Z",
    callDenTool: async (_authority: unknown, toolName: string) => {
      if (!authorityAvailable) throw new Error("service authority unavailable");
      if (toolName === "get_task") {
        return {
          id: Number(durable.taskId),
          project_id: durable.projectId,
          status: "done",
        };
      }
      return {
        items: [
          {
            id: 4089,
            project_id: "rusty-crew",
            task_id: Number(durable.taskId),
            verdict: "looks_good",
          },
        ],
      };
    },
    applyCoordinationDelivery: async (receipt: never) => receipt,
  };

  await reconcileReviewSubmissions(context);
  assert.equal(durable.phase, "den_finalization_pending");
  assert.equal(
    durable.reviewResultJson,
    JSON.stringify({ verdict: "looks_good" }),
  );
  authorityAvailable = true;
  await reconcileReviewSubmissions(context);
  assert.deepEqual(transitionTypes, [
    "adapter_failed",
    "den_already_finalized",
  ]);
  assert.equal(durable.phase, "review_terminal");
  assert.equal(durable.reviewVerdict, "looks_good");
});

test("cross-project reconciliation ignores Den wrapper project metadata and keeps task identity authoritative", async () => {
  const pending = scopedSubmissionRecord("rusty-view", {
    type: "external_cli",
    clientId: "test",
    idempotencyKey: "test-newer-round",
  });
  const submitted = {
    ...pending,
    phase: "submitted",
    reviewRoundId: undefined,
    gateId: undefined,
  } as ReviewSubmissionRecord;
  const transitions: Array<Record<string, unknown>> = [];
  const denCalls: string[] = [];
  await reconcileReviewSubmissions({
    bridge: {
      listReviewSubmissions: async () => [submitted],
      transitionReviewSubmission: async (request: Record<string, unknown>) => {
        transitions.push(request);
        return {
          ...submitted,
          phase: "gate_pending",
          reviewRoundId: 4090,
          revision: submitted.revision + 1,
        };
      },
    } as never,
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
    serviceConfig: reviewServiceConfig(),
    now: () => "2026-08-08T09:30:00.000Z",
    callDenTool: async (_binding: unknown, toolName: string) => {
      denCalls.push(toolName);
      if (toolName === "list_review_rounds") {
        return {
          project_id: "rusty-crew",
          items: [
            {
              id: 4089,
              project_id: "rusty-crew",
              head_commit: submitted.commitSha,
              verdict: "looks_good",
            },
            {
              id: 4090,
              project_id: "rusty-crew",
              head_commit: submitted.commitSha,
              verdict: null,
            },
          ],
        };
      }
      if (toolName === "get_task") {
        return {
          id: Number(submitted.taskId),
          project_id: submitted.projectId,
          status: "in_progress",
        };
      }
      if (toolName === "watch_github_checks") {
        return { gate: { id: 2738, status: "pending" } };
      }
      assert.fail(`unexpected Den tool ${toolName}`);
    },
    applyCoordinationDelivery: async (receipt: never) => receipt,
  });

  assert.deepEqual(denCalls, [
    "list_review_rounds",
    "get_task",
    "list_review_rounds",
    "watch_github_checks",
  ]);
  assert.equal(
    transitions.some(
      (request) =>
        (request.transition as Record<string, unknown>).type ===
        "adapter_failed",
    ),
    false,
  );
  assert.equal(
    (transitions[0]?.transition as Record<string, unknown>)?.type,
    "den_handoff_recorded",
  );
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

function activeTaskDenCall(
  getRecord: () => ReviewSubmissionRecord,
  fallback?: (toolName: string) => unknown | Promise<unknown>,
) {
  return async (_binding: unknown, toolName: string): Promise<unknown> => {
    const current = getRecord();
    if (toolName === "list_review_rounds") {
      return { project_id: current.projectId, items: [] };
    }
    if (toolName === "get_task") {
      return {
        id: Number(current.taskId),
        project_id: current.projectId,
        status: "in_progress",
      };
    }
    if (fallback !== undefined) return fallback(toolName);
    throw new Error(`unexpected Den tool ${toolName}`);
  };
}

function reviewScopeContext(
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
    runtimeConfig: { sessions: [], mcpBindings: [], mcpServers: [] } as never,
    serviceConfig: reviewServiceConfig("debug"),
    validateServiceDenAuthority: async () => ({
      authorityId: "service-review-den",
      auditIdentity: "rusty-crew-review-service",
      serverName: "den" as const,
      status: "ready" as const,
      requiredTools: [],
      missingTools: [],
      checkedAt: "2026-08-05T00:01:00.000Z",
      message: "ready",
    }),
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
    serviceConfig: reviewServiceConfig(),
    now: () => "2026-08-05T00:01:00.000Z",
    callDenTool: async (_binding: unknown, toolName: string) => {
      if (toolName === "list_review_rounds") return { items: [] };
      if (toolName === "get_task") {
        return {
          id: Number(pending.taskId),
          project_id: pending.projectId,
          status: "in_progress",
        };
      }
      if (toolName === "get_github_check_gate") return gate;
      assert.fail(`unexpected Den tool ${toolName}`);
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

test("managed review submission forwards any caller-supplied Den project", async () => {
  let selectedProject: string | undefined;
  const runtime = createServiceReviewSubmissionRuntime(() =>
    reviewScopeContext((request) => {
      selectedProject = request.projectId;
    }),
  );

  const result = await runtime.submit(directReviewInput("rusty-engine-demo"));

  assert.equal(selectedProject, "rusty-engine-demo");
  assert.equal(result.ok, true);
  assert.equal(result.projectId, "rusty-engine-demo");
});

test("external review receipt preserves the caller-supplied project scope", async () => {
  const receipt = await submitExternalReview(reviewScopeContext(), {
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
  });

  assert.equal(receipt.projectId, "den-services");
  assert.equal(receipt.phase, "gate_pending");
});

test("checkless external review advances from Den handoff without registering a gate", async () => {
  const pending = {
    ...scopedSubmissionRecord("den-services", {
      type: "external_cli",
      clientId: "checkless-test",
      idempotencyKey: "checkless-test-6797",
    }),
    requiredChecks: [],
    phase: "den_handoff_recorded",
    gateId: undefined,
    reviewRoundId: 679701,
    revision: 2,
  } as ReviewSubmissionRecord;
  const transitions: Array<Record<string, unknown>> = [];
  const denCalls: string[] = [];

  await reconcileReviewSubmissions({
    bridge: {
      listReviewSubmissions: async () => [pending],
      transitionReviewSubmission: async (request: Record<string, unknown>) => {
        transitions.push(request);
        return {
          ...pending,
          phase: "reviewer_dispatch_pending",
          gateStatus: "passed",
          terminalReason: "no_required_checks",
          revision: pending.revision + 1,
        } as ReviewSubmissionRecord;
      },
    } as never,
    runtimeConfig: { sessions: [], mcpBindings: [], mcpServers: [] } as never,
    serviceConfig: reviewServiceConfig(),
    now: () => "2026-08-11T05:00:00.000Z",
    callDenTool: async (_authority: unknown, toolName: string) => {
      denCalls.push(toolName);
      if (toolName === "list_review_rounds") return { items: [] };
      if (toolName === "get_task") {
        return {
          id: Number(pending.taskId),
          project_id: pending.projectId,
          status: "in_progress",
        };
      }
      throw new Error(`unexpected Den tool ${toolName}`);
    },
    applyCoordinationDelivery: async (receipt: never) => receipt,
  });

  assert.deepEqual(denCalls, ["list_review_rounds", "get_task"]);
  assert.deepEqual(transitions, [
    {
      submissionId: pending.submissionId,
      expectedRevision: pending.revision,
      transition: {
        type: "gate_terminal",
        gateStatus: "passed",
        terminalReason: "no_required_checks",
      },
      now: "2026-08-11T05:00:00.000Z",
    },
  ]);
});

test("external review admission stops before durable dispatch when service authority is invalid", async () => {
  let began = false;
  const context = reviewScopeContext(() => {
    began = true;
  });
  await assert.rejects(
    submitExternalReview(
      {
        ...context,
        validateServiceDenAuthority: async () => ({
          authorityId: "service-review-den",
          auditIdentity: "rusty-crew-review-service",
          serverName: "den" as const,
          status: "missing_tools" as const,
          requiredTools: [],
          missingTools: ["finalize_review"],
          checkedAt: "2026-08-05T00:01:00.000Z",
          message: "finalize_review is missing",
        }),
      },
      {
        projectId: "den-services",
        taskId: 6662,
        repository: "FuzzySlipper/rusty-crew",
        commitSha: "a".repeat(40),
        ref: "main",
        requiredChecks: ["Verify Offline"],
        baseCommit: "0".repeat(40),
        reviewSummaryMd: "Managed review.",
        clientId: "external-test",
        idempotencyKey: "invalid-authority",
        expectedDeploymentRole: "debug",
      },
    ),
    /finalize_review is missing/,
  );
  assert.equal(began, false);
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

test("enabled operator bypass advances a pending gate without calling Den", async () => {
  const pending = gatePendingRecord();
  const transitions: Array<Record<string, unknown>> = [];
  await reconcileReviewSubmissions({
    bridge: {
      listReviewSubmissions: async () => [pending],
      transitionReviewSubmission: async (request: Record<string, unknown>) => {
        transitions.push(request);
        return {
          ...pending,
          phase: "reviewer_dispatch_pending",
          gateStatus: "passed",
          terminalReason: "operator_bypass_github_gate",
          revision: pending.revision + 1,
        } as ReviewSubmissionRecord;
      },
    } as never,
    runtimeConfig: {
      sessions: [],
      mcpBindings: [],
      mcpServers: [],
      reviewGithubGateBypass: {
        enabled: true,
        reason: "GitHub Actions unavailable",
        configRevision: "bypass-config-1",
        deploymentRole: "production",
      },
    } as never,
    serviceConfig: reviewServiceConfig(),
    now: () => "2026-08-18T05:10:00.000Z",
    callDenTool: async () => {
      assert.fail("bypassing an existing pending gate must not call Den");
    },
    applyCoordinationDelivery: async (receipt: never) => receipt,
  });
  assert.deepEqual(transitions, [
    {
      submissionId: pending.submissionId,
      expectedRevision: pending.revision,
      transition: {
        type: "gate_bypassed",
        reason: "GitHub Actions unavailable",
        configRevision: "bypass-config-1",
        deploymentRole: "production",
      },
      now: "2026-08-18T05:10:00.000Z",
    },
  ]);
});

test("new managed submission preserves checks but skips GitHub after Den handoff", async () => {
  const submitted = {
    ...scopedSubmissionRecord("den-services", {
      type: "external_cli",
      clientId: "bypass-new",
      idempotencyKey: "bypass-new-7084",
    }),
    phase: "submitted",
    reviewRoundId: undefined,
    gateId: undefined,
    revision: 1,
  } as ReviewSubmissionRecord;
  let current = submitted;
  const transitions: Array<Record<string, unknown>> = [];
  const denCalls: string[] = [];
  await reconcileReviewSubmissions({
    bridge: {
      listReviewSubmissions: async () => [current],
      transitionReviewSubmission: async (request: Record<string, unknown>) => {
        transitions.push(request);
        const transition = request.transition as Record<string, unknown>;
        current = {
          ...current,
          phase:
            transition.type === "den_handoff_recorded"
              ? "den_handoff_recorded"
              : "reviewer_dispatch_pending",
          reviewRoundId:
            transition.type === "den_handoff_recorded" ? 708401 : 708401,
          gateStatus:
            transition.type === "gate_bypassed" ? "passed" : undefined,
          terminalReason:
            transition.type === "gate_bypassed"
              ? "operator_bypass_github_gate"
              : undefined,
          revision: current.revision + 1,
        } as ReviewSubmissionRecord;
        return current;
      },
    } as never,
    runtimeConfig: {
      sessions: [],
      mcpBindings: [],
      mcpServers: [],
      reviewGithubGateBypass: {
        enabled: true,
        reason: "GitHub outage",
        configRevision: "bypass-config-2",
        deploymentRole: "debug",
      },
    } as never,
    serviceConfig: reviewServiceConfig("debug"),
    now: () => "2026-08-18T05:20:00.000Z",
    callDenTool: async (_binding: unknown, toolName: string) => {
      denCalls.push(toolName);
      if (toolName === "list_review_rounds") return { items: [] };
      if (toolName === "get_task") {
        return {
          id: Number(submitted.taskId),
          project_id: submitted.projectId,
          status: "in_progress",
        };
      }
      if (toolName === "request_review") return { review_round_id: 708401 };
      assert.fail(`unexpected Den tool ${toolName}`);
    },
    applyCoordinationDelivery: async (receipt: never) => receipt,
  });
  assert.equal(denCalls.includes("watch_github_checks"), false);
  assert.deepEqual(
    transitions.map(
      (request) => (request.transition as Record<string, unknown>).type,
    ),
    ["den_handoff_recorded", "gate_bypassed"],
  );
  assert.deepEqual(current.requiredChecks, submitted.requiredChecks);
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

test("duplicate submissions for one Den round select one deterministic workflow", () => {
  const sha = "a".repeat(40);
  const first = {
    ...record("review-first", "6655", sha, "reviewer_dispatched"),
    reviewRoundId: 4279,
    updatedAt: "2026-08-09T17:05:57.988Z",
  } as ReviewSubmissionRecord;
  const duplicate = {
    ...record("review-duplicate", "6655", sha, "reviewer_dispatched"),
    reviewRoundId: 4279,
    updatedAt: "2026-08-09T17:05:58.060Z",
  } as ReviewSubmissionRecord;

  const selected = selectRoutedReviewRecord(
    [first, duplicate],
    `review:6655:${sha}`,
    { taskId: 6655, commitSha: sha },
  );
  assert.equal(selected.ambiguous, false);
  assert.equal(selected.record?.submissionId, "review-duplicate");
});

test("persisted duplicate review progress wins over a queued duplicate", () => {
  const sha = "b".repeat(40);
  const queued = {
    ...record("review-queued", "6656", sha, "reviewer_dispatched"),
    reviewRoundId: 4278,
    updatedAt: "2026-08-09T17:06:00.000Z",
  } as ReviewSubmissionRecord;
  const persisted = {
    ...record("review-persisted", "6656", sha, "den_finalization_pending"),
    reviewRoundId: 4278,
    updatedAt: "2026-08-09T17:05:00.000Z",
  } as ReviewSubmissionRecord;

  const selected = selectRoutedReviewRecord([queued, persisted], undefined, {
    taskId: 6656,
    commitSha: sha,
  });
  assert.equal(selected.ambiguous, false);
  assert.equal(selected.record?.submissionId, "review-persisted");
});

test("same task and SHA remain ambiguous when Den round identity differs", () => {
  const sha = "c".repeat(40);
  const first = {
    ...record("review-first", "6657", sha, "reviewer_dispatched"),
    reviewRoundId: 4275,
  } as ReviewSubmissionRecord;
  const second = {
    ...record("review-second", "6657", sha, "reviewer_dispatched"),
    reviewRoundId: 4280,
  } as ReviewSubmissionRecord;

  const selected = selectRoutedReviewRecord([first, second], undefined, {
    taskId: 6657,
    commitSha: sha,
  });
  assert.equal(selected.record, undefined);
  assert.equal(selected.ambiguous, true);
});

test("trusted dispatch identity selects the exact rereview when task and SHA repeat", () => {
  const sha = "d".repeat(40);
  const oldReview = {
    ...record("review-old", "6658", sha, "review_terminal"),
    reviewRoundId: 4275,
    dispatchMessageId: "review-message:review-old",
  } as ReviewSubmissionRecord;
  const currentReview = {
    ...record("review-current", "6658", sha, "reviewer_dispatched"),
    reviewRoundId: 4280,
    dispatchMessageId: "review-message:review-current",
  } as ReviewSubmissionRecord;

  const selected = selectRoutedReviewRecord(
    [oldReview, currentReview],
    `review:6658:${sha}`,
    { taskId: 6658, commitSha: sha },
    "review-message:review-current",
  );
  assert.equal(selected.ambiguous, false);
  assert.equal(selected.notFound, undefined);
  assert.equal(selected.record?.submissionId, "review-current");
});

test("trusted dispatch identity fails closed when the explicit target disagrees", () => {
  const sha = "e".repeat(40);
  const review = {
    ...record("review-one", "6659", sha, "reviewer_dispatched"),
    reviewRoundId: 4281,
    dispatchMessageId: "review-message:review-one",
  } as ReviewSubmissionRecord;

  const selected = selectRoutedReviewRecord(
    [review],
    `review:6659:${sha}`,
    { taskId: 6660, commitSha: sha },
    "review-message:review-one",
  );
  assert.equal(selected.record, undefined);
  assert.equal(selected.ambiguous, false);
  assert.equal(selected.notFound, true);
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

test("managed closeout uses trusted dispatch identity across same-SHA rereview rounds", async () => {
  const sha = "f".repeat(40);
  const oldReview = {
    ...record("review-old", "6602", sha, "reply_terminal"),
    reviewRoundId: 4275,
    dispatchMessageId: "review-message:review-old",
  } as ReviewSubmissionRecord;
  const currentReview = {
    ...record("review-current", "6602", sha, "reply_terminal"),
    reviewRoundId: 4280,
    dispatchMessageId: "review-message:review-current",
  } as ReviewSubmissionRecord;
  const runtime = createServiceReviewSubmissionRuntime(() => ({
    bridge: {
      listReviewSubmissions: async () => [oldReview, currentReview],
    } as never,
    runtimeConfig: { sessions: [] } as never,
    serviceConfig: { deploymentRole: "production" } as never,
    now: () => "2026-08-05T00:00:00.000Z",
    applyCoordinationDelivery: async (receipt) => receipt,
  }));

  const result = await runtime.complete({
    verdict: "looks_good",
    taskId: 6602,
    commitSha: sha,
    caller: { type: "review_submission", submissionId: "context-resolved" },
    reviewerSessionId: "reviewer-session",
    dispatchMessageId: "review-message:review-current",
  });
  assert.equal(result.submissionId, "review-current");
  assert.equal(result.reasonCode, "review_reply_terminal");
});

test("completed duplicate review replays the durable Den receipt", async () => {
  const sha = "d".repeat(40);
  const queued = {
    ...record("review-queued", "6658", sha, "reviewer_dispatched"),
    reviewRoundId: 4274,
    updatedAt: "2026-08-09T17:06:00.000Z",
  } as ReviewSubmissionRecord;
  const completed = {
    ...record("review-completed", "6658", sha, "review_terminal"),
    reviewRoundId: 4274,
    reviewFinalizationId: 910,
    reviewPacketId: 911,
    reviewPacketMessageId: 912,
    reviewExactHeadCommit: sha,
    reviewVerdict: "looks_good",
    reviewTaskStatus: "done",
    updatedAt: "2026-08-09T17:05:00.000Z",
  } as ReviewSubmissionRecord;
  const runtime = createServiceReviewSubmissionRuntime(() => ({
    bridge: {
      listReviewSubmissions: async () => [queued, completed],
    } as never,
    runtimeConfig: { sessions: [] } as never,
    serviceConfig: { deploymentRole: "production" } as never,
    now: () => "2026-08-09T17:07:00.000Z",
    applyCoordinationDelivery: async (receipt) => receipt,
  }));

  const result = await runtime.complete({
    verdict: "looks_good",
    taskId: 6658,
    commitSha: sha,
    caller: { type: "review_submission", submissionId: "context-resolved" },
    reviewerSessionId: "reviewer-session",
  });
  assert.equal(result.ok, true);
  assert.equal(result.submissionId, "review-completed");
  assert.equal(result.reviewRoundId, 4274);
  assert.equal(result.finalizationId, 910);
  assert.equal(result.packetId, 911);
  assert.equal(result.packetMessageId, 912);
  assert.equal(result.verdict, "looks_good");
  assert.equal(result.taskStatus, "done");
});

test("persisted duplicate result reconciles a finalized Den round without finalizing twice", async () => {
  const sha = "e".repeat(40);
  const persisted = {
    ...scopedSubmissionRecord("rusty-view", {
      type: "external_cli",
      clientId: "external-review",
      idempotencyKey: "first",
    }),
    submissionId: "review-persisted",
    taskId: "6656",
    commitSha: sha,
    phase: "den_finalization_pending",
    reviewRoundId: 4278,
    reviewerSessionId: "reviewer-session",
    reviewResultJson: JSON.stringify({ verdict: "looks_good" }),
    updatedAt: "2026-08-09T17:05:00.000Z",
  } as ReviewSubmissionRecord;
  const duplicate = {
    ...persisted,
    submissionId: "review-queued-duplicate",
    caller: {
      type: "external_cli",
      clientId: "external-review",
      idempotencyKey: "second",
    },
    phase: "reviewer_dispatched",
    reviewResultJson: undefined,
    updatedAt: "2026-08-09T17:06:00.000Z",
  } as ReviewSubmissionRecord;
  const denCalls: string[] = [];
  const transitions: string[] = [];
  const runtime = createServiceReviewSubmissionRuntime(() => ({
    bridge: {
      listReviewSubmissions: async () => [duplicate, persisted],
      transitionReviewSubmission: async (request: {
        submissionId: string;
        transition: { type: string };
      }) => {
        assert.equal(request.submissionId, persisted.submissionId);
        transitions.push(request.transition.type);
        assert.equal(request.transition.type, "den_already_finalized");
        return {
          ...persisted,
          phase: "review_terminal",
          reviewExactHeadCommit: sha,
          reviewVerdict: "looks_good",
          revision: persisted.revision + 1,
        } as ReviewSubmissionRecord;
      },
    } as never,
    runtimeConfig: { sessions: [], mcpBindings: [], mcpServers: [] } as never,
    serviceConfig: reviewServiceConfig(),
    now: () => "2026-08-09T17:07:00.000Z",
    callDenTool: async (_binding: unknown, toolName: string) => {
      denCalls.push(toolName);
      if (toolName === "get_task") {
        return {
          id: Number(persisted.taskId),
          project_id: persisted.projectId,
          status: "done",
        };
      }
      assert.equal(toolName, "list_review_rounds");
      return {
        items: [
          {
            id: 4278,
            project_id: persisted.projectId,
            task_id: Number(persisted.taskId),
            verdict: "looks_good",
          },
        ],
      };
    },
    applyCoordinationDelivery: async (receipt: never) => receipt,
  }));

  const result = await runtime.complete({
    verdict: "looks_good",
    taskId: 6656,
    commitSha: sha,
    caller: { type: "review_submission", submissionId: "context-resolved" },
    reviewerSessionId: "reviewer-session",
  });
  assert.equal(result.ok, true);
  assert.equal(result.submissionId, persisted.submissionId);
  assert.equal(result.reviewRoundId, 4278);
  assert.equal(result.verdict, "looks_good");
  assert.deepEqual(denCalls, ["list_review_rounds", "get_task"]);
  assert.deepEqual(transitions, ["den_already_finalized"]);
});

test("managed closeout explains when a direct Den review has no Crew attachment", async () => {
  const runtime = createServiceReviewSubmissionRuntime(() => ({
    bridge: {
      listReviewSubmissions: async () => [],
    } as never,
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

test("external review submission parser accepts an explicit checkless payload", () => {
  const request = parseExternalReviewSubmissionRequest({
    projectId: "den-services",
    taskId: 6797,
    repository: "FuzzySlipper/den-services",
    commitSha: "a".repeat(40),
    ref: "main",
    requiredChecks: [],
    baseCommit: "0".repeat(40),
    reviewSummaryMd: "Ready for managed review without a GitHub gate.",
    clientId: "external-agent",
    idempotencyKey: "6797-a",
    expectedDeploymentRole: "production",
  });
  assert.deepEqual(request.requiredChecks, []);
});

test("external recovery parser requires optimistic concurrency", () => {
  assert.deepEqual(
    parseExternalReviewRecoveryRequest({
      expectedRevision: 7,
      expectedDeploymentRole: "production",
    }),
    { expectedRevision: 7, expectedDeploymentRole: "production" },
  );
  assert.throws(() => parseExternalReviewRecoveryRequest({}));
  assert.throws(() =>
    parseExternalReviewRecoveryRequest({ expectedRevision: -1 }),
  );
});
