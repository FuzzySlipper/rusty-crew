import type {
  AgentMessageDeliveryReceipt,
  McpBindingRecord,
  ReviewSubmissionRecord,
  SessionId,
} from "@rusty-crew/contracts";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";
import type { RustyCrewRuntimeConfig } from "./service-runtime-config.js";
import type { RustyCrewServiceConfig } from "./service-config.js";
import type {
  ReviewSubmissionToolReceipt,
  ReviewSubmissionToolRuntime,
  SubmitTaskForReviewParameters,
} from "./review-submission-tools.js";
import {
  buildServiceMcpEndpointConfig,
  callConfiguredMcpTool,
} from "./service-mcp-tools.js";

export interface ServiceReviewSubmissionContext {
  readonly bridge: NativeBridgeModule;
  readonly projectId?: string;
  readonly runtimeConfig: RustyCrewRuntimeConfig;
  readonly serviceConfig: RustyCrewServiceConfig;
  now(): string;
  applyCoordinationDelivery(
    receipt: AgentMessageDeliveryReceipt,
  ): Promise<AgentMessageDeliveryReceipt>;
}

export function createServiceReviewSubmissionRuntime(
  getContext: () => ServiceReviewSubmissionContext | undefined,
): ReviewSubmissionToolRuntime {
  return {
    async submit(input) {
      const context = getContext();
      if (context === undefined) {
        throw new Error("service review submission runtime is not ready");
      }
      return submitReview(context, input);
    },
  };
}

export async function reconcileReviewSubmissions(
  context: ServiceReviewSubmissionContext,
): Promise<void> {
  const pending = await context.bridge.listReviewSubmissions({
    pendingOnly: true,
  });
  for (const record of pending) {
    if (
      (record.phase === "submitted" ||
        record.phase === "den_handoff_recorded") &&
      retryDue(record)
    ) {
      await advanceDenHandoff(context, record);
    } else if (record.phase === "gate_failed") {
      await settleFailedGate(context, record);
    } else if (record.phase === "reviewer_dispatch_pending") {
      await dispatchReviewer(context, record);
    }
  }
}

async function submitReview(
  context: ServiceReviewSubmissionContext,
  input: SubmitTaskForReviewParameters & {
    caller: import("@rusty-crew/contracts").AgentCoordinationCaller;
  },
): Promise<ReviewSubmissionToolReceipt> {
  const projectId = context.projectId?.trim();
  if (!projectId) {
    return rejected(input, "review_project_not_configured");
  }
  let record = await context.bridge.beginReviewSubmission({
    caller: input.caller,
    projectId,
    taskId: String(input.taskId),
    repository: input.repository,
    commitSha: input.commitSha,
    gitRef: input.ref,
    requiredChecks: [...new Set(input.requiredChecks)],
    baseCommit: input.baseCommit,
    reviewSummaryMd: input.reviewSummaryMd,
    reviewer: input.reviewer ?? "@reviewer",
    now: context.now(),
  });
  if (record.phase !== "submitted" && record.phase !== "den_handoff_recorded") {
    return accepted(record);
  }

  return advanceDenHandoff(context, record);
}

async function advanceDenHandoff(
  context: ServiceReviewSubmissionContext,
  initial: ReviewSubmissionRecord,
): Promise<ReviewSubmissionToolReceipt> {
  let record = initial;
  const binding = denBinding(context, record.submitterSessionId);
  if (binding === undefined) {
    record = await recordAdapterFailure(
      context,
      record,
      "den_mcp_binding_unavailable",
      "Submitting session has no active Den MCP binding.",
    );
    return failed(record, "den_mcp_binding_unavailable");
  }

  try {
    if (record.phase === "submitted") {
      const rounds = await reviewRounds(
        context,
        binding,
        Number(record.taskId),
      );
      const existingRound = exactHeadRound(rounds, record.commitSha);
      const baseCommit =
        record.baseCommit ?? priorReviewedHead(rounds, record.commitSha);
      if (baseCommit === undefined) {
        throw new ReviewSubmissionAdapterError(
          "review_base_commit_required",
          "Initial review submission requires baseCommit; rereviews derive it from the prior round.",
        );
      }
      const reviewRoundId =
        existingRound ??
        requiredNumericId(
          await denCall(context, binding, "request_review", {
            task_id: Number(record.taskId),
            requested_by: record.submitterAgentId,
            branch: record.gitRef,
            base_branch: record.gitRef,
            base_commit: baseCommit,
            head_commit: record.commitSha,
            tests_run: JSON.stringify([]),
            notes: record.reviewSummaryMd,
          }),
          ["review_round_id", "reviewRoundId", "id"],
        );
      record = await context.bridge.transitionReviewSubmission({
        submissionId: record.submissionId,
        expectedRevision: record.revision,
        transition: { type: "den_handoff_recorded", reviewRoundId },
        now: context.now(),
      });
    }
    const gate = await denCall(context, binding, "watch_github_checks", {
      task_id: Number(record.taskId),
      repository: record.repository,
      commit_sha: record.commitSha,
      ref: record.gitRef,
      required_checks: record.requiredChecks,
      requested_by: record.submitterAgentId,
      session_key: record.submitterSessionId,
      agent_profile: record.submitterAgentId,
    });
    const gateId = requiredNumericId(gate, ["gate_id", "gateId", "id"]);
    record = await context.bridge.transitionReviewSubmission({
      submissionId: record.submissionId,
      expectedRevision: record.revision,
      transition: { type: "gate_registered", gateId },
      now: context.now(),
    });
    return accepted(record);
  } catch (error) {
    const reasonCode =
      error instanceof ReviewSubmissionAdapterError
        ? error.reasonCode
        : "review_submission_adapter_failed";
    record = await recordAdapterFailure(
      context,
      record,
      reasonCode,
      errorMessage(error),
    );
    return failed(record, reasonCode);
  }
}

async function settleFailedGate(
  context: ServiceReviewSubmissionContext,
  record: ReviewSubmissionRecord,
): Promise<void> {
  const binding = denBinding(context, record.submitterSessionId);
  if (binding === undefined) return;
  try {
    await denCall(context, binding, "update_task", {
      task_id: Number(record.taskId),
      status: "in_progress",
      agent: record.submitterAgentId,
    });
    await context.bridge.transitionReviewSubmission({
      submissionId: record.submissionId,
      expectedRevision: record.revision,
      transition: {
        type: "gate_failure_settled",
        terminalReason: record.terminalReason ?? "github_gate_failed",
      },
      now: context.now(),
    });
  } catch (error) {
    await recordAdapterFailure(
      context,
      record,
      "den_task_reset_failed",
      errorMessage(error),
    );
  }
}

async function dispatchReviewer(
  context: ServiceReviewSubmissionContext,
  record: ReviewSubmissionRecord,
): Promise<void> {
  const identity = record.submissionId.replaceAll(":", "-");
  try {
    const initial = await context.bridge.deliverAgentMessage({
      caller: {
        type: "review_submission",
        submissionId: record.submissionId,
      },
      deliveryId: `review-delivery:${identity}`,
      idempotencyKey: `review-delivery:${identity}`,
      messageId: `review-message:${identity}`,
      toAddress: record.reviewer,
      inputKind: "routed_agent_message",
      body: reviewerRequestBody(record),
      correlationId: `review:${record.taskId}:${record.commitSha}`,
      requireWake: true,
      createdAt: context.now(),
      expiresAt: new Date(Date.now() + 24 * 60 * 60_000).toISOString(),
    });
    const receipt = await context.applyCoordinationDelivery(initial);
    if (receipt.status !== "accepted") {
      throw new ReviewSubmissionAdapterError(
        receipt.reasonCode ?? "reviewer_dispatch_rejected",
        `Reviewer dispatch to ${record.reviewer} was ${receipt.status}.`,
      );
    }
    await context.bridge.transitionReviewSubmission({
      submissionId: record.submissionId,
      expectedRevision: record.revision,
      transition: {
        type: "reviewer_dispatched",
        reviewerSessionId: requiredSessionId(receipt),
        dispatchMessageId: receipt.request.messageId,
        dispatchDeliveryId: receipt.request.deliveryId,
      },
      now: context.now(),
    });
  } catch (error) {
    await recordAdapterFailure(
      context,
      record,
      error instanceof ReviewSubmissionAdapterError
        ? error.reasonCode
        : "reviewer_dispatch_failed",
      errorMessage(error),
    );
  }
}

function denBinding(
  context: ServiceReviewSubmissionContext,
  sessionId: string,
): McpBindingRecord | undefined {
  const session = context.runtimeConfig.sessions.find(
    (candidate) => candidate.sessionId === sessionId,
  );
  if (session === undefined) return undefined;
  return context.runtimeConfig.mcpBindings.find(
    (binding) =>
      binding.status === "active" &&
      binding.profileId === session.profileId &&
      binding.agentId === session.agentId &&
      (binding.sessionId === undefined || binding.sessionId === sessionId) &&
      binding.serverNames.includes("den"),
  );
}

async function denCall(
  context: ServiceReviewSubmissionContext,
  binding: McpBindingRecord,
  toolName: string,
  args: Record<string, unknown>,
): Promise<unknown> {
  const result = await callConfiguredMcpTool({
    binding,
    config: buildServiceMcpEndpointConfig({
      mcpConfig: context.serviceConfig.mcp,
      mcpServers: context.runtimeConfig.mcpServers,
    }),
    toolName,
    arguments: args,
  });
  if (result.isError) {
    const content =
      typeof result.content === "string"
        ? result.content
        : result.content
            .map((part) =>
              part.type === "text" ? part.text : `[${part.type}]`,
            )
            .join("\n");
    throw new Error(`Den ${toolName} failed: ${content}`);
  }
  return result.details;
}

async function reviewRounds(
  context: ServiceReviewSubmissionContext,
  binding: McpBindingRecord,
  taskId: number,
): Promise<Record<string, unknown>[]> {
  const payload = await denCall(context, binding, "list_review_rounds", {
    task_id: taskId,
  });
  return allObjects(payload);
}

function priorReviewedHead(
  rounds: Record<string, unknown>[],
  currentCommitSha: string,
): string | undefined {
  return rounds
    .map((value) => stringValue(value, ["head_commit", "headCommit"]))
    .filter(
      (value): value is string =>
        value !== undefined &&
        /^[0-9a-fA-F]{40}$/.test(value) &&
        value.toLowerCase() !== currentCommitSha.toLowerCase(),
    )
    .at(-1);
}

function exactHeadRound(
  rounds: Record<string, unknown>[],
  commitSha: string,
): number | undefined {
  for (const value of [...rounds].reverse()) {
    const head = stringValue(value, ["head_commit", "headCommit"]);
    if (head?.toLowerCase() !== commitSha.toLowerCase()) continue;
    for (const key of ["review_round_id", "reviewRoundId", "id"]) {
      const id = value[key];
      if (typeof id === "number" && Number.isSafeInteger(id) && id > 0) {
        return id;
      }
    }
  }
  return undefined;
}

async function recordAdapterFailure(
  context: ServiceReviewSubmissionContext,
  record: ReviewSubmissionRecord,
  reasonCode: string,
  summary: string,
): Promise<ReviewSubmissionRecord> {
  return context.bridge.transitionReviewSubmission({
    submissionId: record.submissionId,
    expectedRevision: record.revision,
    transition: { type: "adapter_failed", reasonCode, summary },
    now: context.now(),
  });
}

function reviewerRequestBody(record: ReviewSubmissionRecord): string {
  return [
    `Review Den task #${record.taskId} at exact SHA ${record.commitSha}.`,
    `Repository: ${record.repository}`,
    `Ref: ${record.gitRef}`,
    `Review round: ${record.reviewRoundId ?? "unknown"}`,
    "",
    record.reviewSummaryMd,
    "",
    "Record findings and the verdict in Den, then reply once to this routed message.",
  ].join("\n");
}

function accepted(record: ReviewSubmissionRecord): ReviewSubmissionToolReceipt {
  return {
    ok: true,
    submissionId: record.submissionId,
    phase: record.phase,
    taskId: Number(record.taskId),
    commitSha: record.commitSha,
    summary: `Task #${record.taskId} review submission accepted at ${record.commitSha}; phase=${record.phase}. GitHub checks continue durably without holding this model turn.`,
  };
}

function failed(
  record: ReviewSubmissionRecord,
  reasonCode: string,
): ReviewSubmissionToolReceipt {
  return {
    ok: false,
    submissionId: record.submissionId,
    phase: record.phase,
    taskId: Number(record.taskId),
    commitSha: record.commitSha,
    reasonCode,
    summary: record.lastAdapterError ?? "Review submission failed.",
  };
}

function rejected(
  input: SubmitTaskForReviewParameters,
  reasonCode: string,
): ReviewSubmissionToolReceipt {
  return {
    ok: false,
    taskId: input.taskId,
    commitSha: input.commitSha,
    reasonCode,
    summary: "Rusty Crew Review project integration is not configured.",
  };
}

function requiredNumericId(value: unknown, keys: string[]): number {
  for (const object of allObjects(value)) {
    for (const key of keys) {
      const candidate = object[key];
      if (typeof candidate === "number" && Number.isSafeInteger(candidate)) {
        return candidate;
      }
    }
  }
  throw new Error(`Den response omitted ${keys[0]}.`);
}

function requiredSessionId(receipt: AgentMessageDeliveryReceipt): SessionId {
  const sessionId = receipt.request.toSessionId;
  if (sessionId === undefined || sessionId === null) {
    throw new Error("Reviewer dispatch did not resolve an exact session.");
  }
  return sessionId as SessionId;
}

function allObjects(value: unknown): Record<string, unknown>[] {
  const found: Record<string, unknown>[] = [];
  const visit = (candidate: unknown): void => {
    if (Array.isArray(candidate)) {
      candidate.forEach(visit);
      return;
    }
    if (typeof candidate !== "object" || candidate === null) {
      if (typeof candidate === "string") {
        try {
          visit(JSON.parse(candidate));
        } catch {}
      }
      return;
    }
    const record = candidate as Record<string, unknown>;
    found.push(record);
    Object.values(record).forEach(visit);
  };
  visit(value);
  return found;
}

function stringValue(
  value: Record<string, unknown>,
  keys: string[],
): string | undefined {
  for (const key of keys) {
    if (typeof value[key] === "string") return value[key];
  }
  return undefined;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function retryDue(record: ReviewSubmissionRecord): boolean {
  if (record.lastAdapterError === undefined) return true;
  return Date.now() - Date.parse(record.updatedAt) >= 30_000;
}

class ReviewSubmissionAdapterError extends Error {
  constructor(
    readonly reasonCode: string,
    message: string,
  ) {
    super(message);
  }
}
