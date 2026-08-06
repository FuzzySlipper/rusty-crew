import { createHash } from "node:crypto";
import type {
  AgentCoordinationCaller,
  AgentMessageDeliveryReceipt,
  McpBindingRecord,
  ReviewFindingStatus,
  ReviewSubmissionRecord,
  SessionId,
} from "@rusty-crew/contracts";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";
import type { RustyCrewRuntimeConfig } from "./service-runtime-config.js";
import type { RustyCrewServiceConfig } from "./service-config.js";
import type {
  ReviewSubmissionToolReceipt,
  ReviewSubmissionToolRuntime,
  CompleteRoutedReviewParameters,
  CompleteRoutedReviewToolReceipt,
  SubmitTaskForReviewParameters,
} from "./review-submission-tools.js";
import {
  buildServiceMcpEndpointConfig,
  callConfiguredMcpTool,
} from "./service-mcp-tools.js";

export interface ServiceReviewSubmissionContext {
  readonly bridge: NativeBridgeModule;
  readonly reviewProjectIds: readonly string[];
  readonly reviewDenBindingId?: string;
  readonly runtimeConfig: RustyCrewRuntimeConfig;
  readonly serviceConfig: RustyCrewServiceConfig;
  readonly callDenTool?: (
    binding: McpBindingRecord,
    toolName: string,
    args: Record<string, unknown>,
  ) => Promise<unknown>;
  now(): string;
  applyCoordinationDelivery(
    receipt: AgentMessageDeliveryReceipt,
  ): Promise<AgentMessageDeliveryReceipt>;
}

export interface ExternalReviewSubmissionRequest {
  readonly projectId: string;
  readonly taskId: number;
  readonly repository: string;
  readonly commitSha: string;
  readonly ref: string;
  readonly requiredChecks: readonly string[];
  readonly baseCommit?: string;
  readonly reviewSummaryMd: string;
  readonly clientId: string;
  readonly idempotencyKey: string;
  readonly expectedDeploymentRole?: "production" | "debug";
}

export interface ExternalReviewSubmissionReceipt {
  readonly submissionId: string;
  readonly projectId: string;
  readonly taskId: number;
  readonly repository: string;
  readonly commitSha: string;
  readonly ref: string;
  readonly requiredChecks: string[];
  readonly baseCommit?: string;
  readonly reviewer: string;
  readonly clientId: string;
  readonly idempotencyKey: string;
  readonly deploymentRole: "production" | "debug";
  readonly phase: ReviewSubmissionRecord["phase"];
  readonly revision: number;
  readonly reviewRoundId?: number;
  readonly gateId?: number;
  readonly gateStatus?: string;
  readonly reviewFinalizationId?: number;
  readonly reviewPacketId?: number;
  readonly reviewPacketMessageId?: number;
  readonly reviewExactHeadCommit?: string;
  readonly reviewVerdict?: string;
  readonly reviewTaskStatus?: string;
  readonly terminalReason?: string;
  readonly lastAdapterError?: string;
}

const REVIEW_PROJECT_ID_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._-]*$/;

export function parseReviewProjectIds(raw: string | undefined): string[] {
  if (raw === undefined || raw.trim() === "") return [];
  const ids = [...new Set(raw.split(",").map((value) => value.trim()))];
  if (ids.some((id) => !REVIEW_PROJECT_ID_PATTERN.test(id))) {
    throw new Error(
      "RUSTY_CREW_REVIEW_PROJECT_IDS must contain only comma-separated Den project ids",
    );
  }
  return ids;
}

export async function submitExternalReview(
  context: ServiceReviewSubmissionContext,
  input: ExternalReviewSubmissionRequest,
): Promise<ExternalReviewSubmissionReceipt> {
  validateExternalReviewInput(context, input);
  const projectSelection = selectConfiguredReviewProject(
    context,
    input.projectId,
  );
  if (!projectSelection.ok) {
    throw new ReviewSubmissionAdapterError(
      projectSelection.reasonCode,
      projectSelection.reasonCode === "review_project_not_allowed"
        ? `Den project ${input.projectId} is not enabled for managed Rusty Crew reviews.`
        : "Rusty Crew managed review project integration is not configured.",
    );
  }
  const projectId = projectSelection.projectId;
  let record = await context.bridge.beginReviewSubmission({
    caller: {
      type: "external_cli",
      clientId: input.clientId,
      idempotencyKey: input.idempotencyKey,
    },
    projectId,
    taskId: String(input.taskId),
    repository: input.repository,
    commitSha: input.commitSha,
    gitRef: input.ref,
    requiredChecks: [...new Set(input.requiredChecks)],
    baseCommit: input.baseCommit,
    reviewSummaryMd: input.reviewSummaryMd,
    reviewer: "@reviewer",
    now: context.now(),
  });
  if (record.phase === "submitted" || record.phase === "den_handoff_recorded") {
    await advanceDenHandoff(context, record);
    record = await getReviewSubmissionRecord(context, record.submissionId);
  }
  return externalReviewReceipt(context, record);
}

export async function getExternalReviewStatus(
  context: ServiceReviewSubmissionContext,
  submissionId: string,
): Promise<ExternalReviewSubmissionReceipt> {
  const record = await getReviewSubmissionRecord(context, submissionId);
  if (record.caller.type !== "external_cli") {
    throw new ReviewSubmissionAdapterError(
      "external_review_submission_not_found",
      "The requested submission is not owned by the external review CLI.",
    );
  }
  return externalReviewReceipt(context, record);
}

export function assertExpectedDeploymentRole(
  context: ServiceReviewSubmissionContext,
  expectedDeploymentRole: string | undefined,
): void {
  if (
    expectedDeploymentRole !== undefined &&
    expectedDeploymentRole !== "production" &&
    expectedDeploymentRole !== "debug"
  ) {
    throw new ReviewSubmissionAdapterError(
      "invalid_external_review_submission",
      "expectedDeploymentRole must be production or debug.",
    );
  }
  if (
    expectedDeploymentRole !== undefined &&
    expectedDeploymentRole !== context.serviceConfig.deploymentRole
  ) {
    throw new ReviewSubmissionAdapterError(
      "deployment_role_mismatch",
      `Expected ${expectedDeploymentRole} service, connected to ${context.serviceConfig.deploymentRole}.`,
    );
  }
}

export function parseExternalReviewSubmissionRequest(
  body: unknown,
): ExternalReviewSubmissionRequest {
  if (typeof body !== "object" || body === null || Array.isArray(body)) {
    throw new ReviewSubmissionAdapterError(
      "invalid_external_review_submission",
      "External review submission body must be a JSON object.",
    );
  }
  const value = body as Record<string, unknown>;
  const taskId = value.taskId;
  const projectId = stringField(value, "projectId");
  const repository = stringField(value, "repository");
  const commitSha = stringField(value, "commitSha");
  const ref = stringField(value, "ref");
  const requiredChecks = value.requiredChecks;
  const baseCommit = optionalStringField(value, "baseCommit");
  const reviewSummaryMd = stringField(value, "reviewSummaryMd");
  const clientId = stringField(value, "clientId");
  const idempotencyKey = stringField(value, "idempotencyKey");
  const expectedDeploymentRole = optionalStringField(
    value,
    "expectedDeploymentRole",
  );
  if (Object.hasOwn(value, "reviewer")) {
    throw new ReviewSubmissionAdapterError(
      "invalid_external_review_submission",
      "External review submissions always route to @reviewer; reviewer cannot be overridden.",
    );
  }
  if (
    typeof taskId !== "number" ||
    !Array.isArray(requiredChecks) ||
    requiredChecks.some((check) => typeof check !== "string") ||
    (expectedDeploymentRole !== undefined &&
      expectedDeploymentRole !== "production" &&
      expectedDeploymentRole !== "debug")
  ) {
    throw new ReviewSubmissionAdapterError(
      "invalid_external_review_submission",
      "External review submission has an invalid task id, required checks, or deployment role.",
    );
  }
  return {
    projectId,
    taskId,
    repository,
    commitSha,
    ref,
    requiredChecks,
    ...(baseCommit === undefined ? {} : { baseCommit }),
    reviewSummaryMd,
    clientId,
    idempotencyKey,
    ...(expectedDeploymentRole === undefined ? {} : { expectedDeploymentRole }),
  };
}

function externalReviewReceipt(
  context: ServiceReviewSubmissionContext,
  record: ReviewSubmissionRecord,
): ExternalReviewSubmissionReceipt {
  if (record.caller.type !== "external_cli") {
    throw new ReviewSubmissionAdapterError(
      "external_review_submission_not_found",
      "The requested submission is not owned by the external review CLI.",
    );
  }
  return {
    submissionId: record.submissionId,
    projectId: String(record.projectId),
    taskId: Number(record.taskId),
    repository: record.repository,
    commitSha: record.commitSha,
    ref: record.gitRef,
    requiredChecks: [...record.requiredChecks],
    ...(record.baseCommit === undefined || record.baseCommit === null
      ? {}
      : { baseCommit: record.baseCommit }),
    reviewer: record.reviewer,
    clientId: record.caller.clientId,
    idempotencyKey: record.caller.idempotencyKey,
    deploymentRole: context.serviceConfig.deploymentRole,
    phase: record.phase,
    revision: record.revision,
    ...(record.reviewRoundId === undefined || record.reviewRoundId === null
      ? {}
      : { reviewRoundId: record.reviewRoundId }),
    ...(record.gateId === undefined || record.gateId === null
      ? {}
      : { gateId: record.gateId }),
    ...(record.gateStatus === undefined || record.gateStatus === null
      ? {}
      : { gateStatus: record.gateStatus }),
    ...(record.reviewFinalizationId === undefined ||
    record.reviewFinalizationId === null
      ? {}
      : { reviewFinalizationId: record.reviewFinalizationId }),
    ...(record.reviewPacketId === undefined || record.reviewPacketId === null
      ? {}
      : { reviewPacketId: record.reviewPacketId }),
    ...(record.reviewPacketMessageId === undefined ||
    record.reviewPacketMessageId === null
      ? {}
      : { reviewPacketMessageId: record.reviewPacketMessageId }),
    ...(record.reviewExactHeadCommit === undefined ||
    record.reviewExactHeadCommit === null
      ? {}
      : { reviewExactHeadCommit: record.reviewExactHeadCommit }),
    ...(record.reviewVerdict === undefined || record.reviewVerdict === null
      ? {}
      : { reviewVerdict: record.reviewVerdict }),
    ...(record.reviewTaskStatus === undefined ||
    record.reviewTaskStatus === null
      ? {}
      : { reviewTaskStatus: record.reviewTaskStatus }),
    ...(record.terminalReason === undefined || record.terminalReason === null
      ? {}
      : { terminalReason: record.terminalReason }),
    ...(record.lastAdapterError === undefined ||
    record.lastAdapterError === null
      ? {}
      : { lastAdapterError: record.lastAdapterError }),
  };
}

async function getReviewSubmissionRecord(
  context: ServiceReviewSubmissionContext,
  submissionId: string,
): Promise<ReviewSubmissionRecord> {
  if (!/^review-submission:[0-9a-f]{64}$/.test(submissionId)) {
    throw new ReviewSubmissionAdapterError(
      "external_review_submission_not_found",
      "Submission id must be a Rusty Crew review submission id.",
    );
  }
  const records = await context.bridge.listReviewSubmissions({
    submissionId,
    pendingOnly: false,
  });
  const record = records[0];
  if (record === undefined) {
    throw new ReviewSubmissionAdapterError(
      "external_review_submission_not_found",
      `Review submission ${submissionId} was not found.`,
    );
  }
  return record;
}

function validateExternalReviewInput(
  context: ServiceReviewSubmissionContext,
  input: ExternalReviewSubmissionRequest,
): void {
  if (
    !Number.isSafeInteger(input.taskId) ||
    input.taskId <= 0 ||
    !REVIEW_PROJECT_ID_PATTERN.test(input.projectId.trim()) ||
    input.repository.trim() === "" ||
    input.commitSha.trim() === "" ||
    input.ref.trim() === "" ||
    input.requiredChecks.length === 0 ||
    input.requiredChecks.some((check) => check.trim() === "") ||
    input.reviewSummaryMd.trim() === "" ||
    input.reviewSummaryMd.length > 64 * 1024 ||
    input.clientId.trim() === "" ||
    input.clientId.length > 128 ||
    input.idempotencyKey.trim() === "" ||
    input.idempotencyKey.length > 256
  ) {
    throw new ReviewSubmissionAdapterError(
      "invalid_external_review_submission",
      "External review submission fields are missing, empty, or exceed their bounds.",
    );
  }
  assertExpectedDeploymentRole(context, input.expectedDeploymentRole);
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
    async complete(input) {
      const context = getContext();
      if (context === undefined) {
        throw new Error("service review completion runtime is not ready");
      }
      return completeReview(context, input);
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
    } else if (record.phase === "gate_pending" && retryDue(record)) {
      await reconcilePendingGate(context, record);
    } else if (record.phase === "reviewer_dispatch_pending") {
      await dispatchReviewer(context, record);
    } else if (
      record.phase === "den_finalization_pending" ||
      record.phase === "den_finalized" ||
      record.phase === "reply_pending"
    ) {
      await resumeRoutedReview(context, record);
    }
  }
}

async function submitReview(
  context: ServiceReviewSubmissionContext,
  input: SubmitTaskForReviewParameters & {
    caller: import("@rusty-crew/contracts").AgentCoordinationCaller;
  },
): Promise<ReviewSubmissionToolReceipt> {
  const projectSelection = selectConfiguredReviewProject(
    context,
    input.projectId,
  );
  if (!projectSelection.ok) {
    return rejected(input, projectSelection.reasonCode);
  }
  const projectId = projectSelection.projectId;
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

async function completeReview(
  context: ServiceReviewSubmissionContext,
  input: CompleteRoutedReviewParameters & {
    caller: AgentCoordinationCaller;
    reviewerSessionId: string;
    correlationId?: string;
  },
): Promise<CompleteRoutedReviewToolReceipt> {
  if ((input.taskId === undefined) !== (input.commitSha === undefined)) {
    return {
      ok: false,
      reasonCode: "review_target_incomplete",
      summary:
        "Explicit review selection requires both taskId and the exact commitSha from the routed review envelope.",
    };
  }
  const records = await context.bridge.listReviewSubmissions({
    pendingOnly: false,
    reviewerSessionId: input.reviewerSessionId,
    ...(input.taskId === undefined ? {} : { taskId: String(input.taskId) }),
  });
  const selection =
    input.taskId === undefined
      ? selectRoutedReviewRecord(records, input.correlationId)
      : selectRoutedReviewRecord(records, input.correlationId, {
          taskId: input.taskId,
          commitSha: input.commitSha as string,
        });
  const result = canonicalReviewResult(input);
  if (selection.notFound) {
    return {
      ok: false,
      reasonCode: "review_target_not_found",
      summary: `No Rusty Crew managed review submission for task #${input.taskId} at ${input.commitSha} is attached to this reviewer session. This usually means the review was requested directly through Den or send_agent_message; finalize that Den round with finalize_review, or resubmit through submit_task_for_review/the external Rusty Crew review CLI for managed closeout.`,
    };
  }
  if (selection.ambiguous) {
    return {
      ok: false,
      reasonCode: "multiple_active_review_requests",
      summary:
        "More than one active routed review is bound to this reviewer session; completion is refused until the wake context is unambiguous.",
    };
  }
  const eligible = selection.record;
  if (eligible === undefined) {
    return {
      ok: false,
      reasonCode: "review_context_required",
      summary:
        "No active routed review is bound to this reviewer wake. Use the managed reviewer wake that carried the review request.",
    };
  }
  if (eligible.phase === "reply_terminal") {
    return {
      ok: false,
      submissionId: eligible.submissionId,
      taskId: Number(eligible.taskId),
      commitSha: eligible.commitSha,
      reasonCode: eligible.replyReasonCode ?? "review_reply_terminal",
      summary:
        "Den finalization is durable, but this routed requester has a terminal reply outcome; no replacement requester was selected.",
    };
  }
  if (eligible.phase === "replied") {
    if (eligible.reviewResultDigest === result.digest) {
      return completedReceipt(eligible);
    }
    return {
      ok: false,
      submissionId: eligible.submissionId,
      taskId: Number(eligible.taskId),
      commitSha: eligible.commitSha,
      reasonCode: "review_result_conflict",
      summary:
        "This routed review is already finalized with a different structured result; no second Den finalization or reply was sent.",
    };
  }
  if (
    eligible.reviewResultDigest !== undefined &&
    eligible.reviewResultDigest !== null &&
    eligible.reviewResultDigest !== result.digest
  ) {
    return {
      ok: false,
      submissionId: eligible.submissionId,
      taskId: Number(eligible.taskId),
      commitSha: eligible.commitSha,
      reasonCode: "review_result_conflict",
      summary:
        "This routed review already has a different structured result; no alternate Den finalization or reply was sent.",
    };
  }
  return resumeRoutedReview(context, eligible, result);
}

const ROUTED_REVIEW_PHASES = new Set<ReviewSubmissionRecord["phase"]>([
  "reviewer_dispatched",
  "den_finalization_pending",
  "den_finalized",
  "reply_pending",
  "replied",
  "reply_terminal",
]);

const TERMINAL_ROUTED_REVIEW_PHASES = new Set<ReviewSubmissionRecord["phase"]>([
  "replied",
  "reply_terminal",
]);

export function selectRoutedReviewRecord(
  records: readonly ReviewSubmissionRecord[],
  correlationId?: string,
  explicitTarget?: {
    readonly taskId: number;
    readonly commitSha: string;
  },
): {
  readonly record?: ReviewSubmissionRecord;
  readonly ambiguous: boolean;
  readonly notFound?: boolean;
} {
  const eligible = records.filter((record) =>
    ROUTED_REVIEW_PHASES.has(record.phase),
  );
  if (explicitTarget !== undefined) {
    const candidates = eligible.filter(
      (record) =>
        record.taskId === String(explicitTarget.taskId) &&
        record.commitSha.toLowerCase() ===
          explicitTarget.commitSha.toLowerCase(),
    );
    return {
      ...(candidates.length === 1 ? { record: candidates[0] } : {}),
      ambiguous: candidates.length > 1,
      ...(candidates.length === 0 ? { notFound: true } : {}),
    };
  }
  const correlated =
    correlationId === undefined
      ? eligible
      : eligible.filter(
          (record) =>
            correlationId === `review:${record.taskId}:${record.commitSha}`,
        );
  const active = correlated.filter(
    (record) => !TERMINAL_ROUTED_REVIEW_PHASES.has(record.phase),
  );
  const candidates =
    correlationId === undefined && active.length > 0 ? active : correlated;
  return {
    ...(candidates.length === 1 ? { record: candidates[0] } : {}),
    ambiguous: candidates.length > 1,
  };
}

interface CanonicalReviewResult {
  verdict: "looks_good" | "changes_requested";
  notes?: string;
  evidence: string[];
  priorFindingResolutions: NonNullable<
    CompleteRoutedReviewParameters["priorFindingResolutions"]
  >;
  newFindings: NonNullable<CompleteRoutedReviewParameters["newFindings"]>;
  digest: string;
  json: string;
}

function canonicalReviewResult(
  input: CompleteRoutedReviewParameters,
): CanonicalReviewResult {
  const normalized = {
    verdict: input.verdict,
    ...(input.notes?.trim() ? { notes: input.notes.trim() } : {}),
    evidence: [...(input.evidence ?? [])]
      .map((item) => item.trim())
      .filter(Boolean),
    priorFindingResolutions: [...(input.priorFindingResolutions ?? [])]
      .map((finding) => ({
        findingId: finding.findingId,
        status: finding.status.trim(),
        verificationNote: finding.verificationNote.trim(),
      }))
      .sort((left, right) => left.findingId - right.findingId),
    newFindings: [...(input.newFindings ?? [])]
      .map((finding) => ({
        category: finding.category.trim(),
        summary: finding.summary.trim(),
        ...(finding.notes?.trim() ? { notes: finding.notes.trim() } : {}),
        ...(finding.fileReferences?.length
          ? { fileReferences: [...finding.fileReferences].sort() }
          : {}),
        ...(finding.testCommands?.length
          ? { testCommands: [...finding.testCommands].sort() }
          : {}),
      }))
      .sort((left, right) =>
        JSON.stringify(left).localeCompare(JSON.stringify(right)),
      ),
  };
  const json = JSON.stringify(normalized);
  return {
    verdict: normalized.verdict,
    notes:
      normalized.notes === undefined && normalized.evidence.length === 0
        ? undefined
        : [normalized.notes, ...normalized.evidence]
            .filter((item): item is string => item !== undefined)
            .join("\n"),
    evidence: normalized.evidence,
    priorFindingResolutions: normalized.priorFindingResolutions,
    newFindings: normalized.newFindings,
    digest: createHash("sha256").update(json).digest("hex"),
    json,
  };
}

async function resumeRoutedReview(
  context: ServiceReviewSubmissionContext,
  initial: ReviewSubmissionRecord,
  suppliedResult?: CanonicalReviewResult,
): Promise<CompleteRoutedReviewToolReceipt> {
  let record = initial;
  try {
    if (record.phase === "reviewer_dispatched") {
      if (suppliedResult === undefined) {
        return {
          ok: false,
          submissionId: record.submissionId,
          taskId: Number(record.taskId),
          commitSha: record.commitSha,
          reasonCode: "review_result_required",
          summary:
            "A structured review result is required for this routed review.",
        };
      }
      record = await context.bridge.transitionReviewSubmission({
        submissionId: record.submissionId,
        expectedRevision: record.revision,
        transition: {
          type: "den_finalization_pending",
          resultDigest: suppliedResult.digest,
          resultJson: suppliedResult.json,
        },
        now: context.now(),
      });
    }

    if (record.phase === "den_finalization_pending") {
      const result = parseStoredReviewResult(record.reviewResultJson);
      const binding = denBinding(context, record.submitterSessionId);
      if (binding === undefined) {
        return completionFailed(
          context,
          record,
          "den_mcp_binding_unavailable",
          isExternalCliSubmission(record)
            ? "The configured service Den MCP binding is unavailable."
            : "Submitting session has no active Den MCP binding.",
        );
      }
      const payload = await denCall(context, binding, "finalize_review", {
        review_round_id: record.reviewRoundId,
        verdict: result.verdict,
        decided_by: reviewerAgentId(context, record) ?? record.reviewer,
        ...(result.notes === undefined ? {} : { notes: result.notes }),
        prior_finding_resolutions: result.priorFindingResolutions.map(
          (finding) => ({
            finding_id: finding.findingId,
            status: finding.status,
            verification_note: finding.verificationNote,
          }),
        ),
        new_findings: result.newFindings.map((finding) => ({
          category: finding.category,
          summary: finding.summary,
          ...(finding.notes === undefined ? {} : { notes: finding.notes }),
          ...(finding.fileReferences === undefined
            ? {}
            : { file_references: finding.fileReferences }),
          ...(finding.testCommands === undefined
            ? {}
            : { test_commands: finding.testCommands }),
        })),
      });
      const receipt = parseFinalizationReceipt(payload, record);
      record = await context.bridge.transitionReviewSubmission({
        submissionId: record.submissionId,
        expectedRevision: record.revision,
        transition: {
          type: "den_finalized",
          finalizationId: receipt.finalizationId,
          packetId: receipt.packetId,
          packetMessageId: receipt.packetMessageId,
          exactHeadCommit: receipt.exactHeadCommit,
          verdict: receipt.verdict,
          findingStatuses: receipt.findingStatuses,
          taskStatus: receipt.taskStatus,
          materialDigest: receipt.materialDigest,
        },
        now: context.now(),
      });
    }

    if (record.phase === "den_finalized" && isExternalCliSubmission(record)) {
      record = await context.bridge.transitionReviewSubmission({
        submissionId: record.submissionId,
        expectedRevision: record.revision,
        transition: {
          type: "review_terminal",
          terminalReason: "external_cli_review_complete",
        },
        now: context.now(),
      });
    }

    if (record.phase === "den_finalized") {
      record = await context.bridge.transitionReviewSubmission({
        submissionId: record.submissionId,
        expectedRevision: record.revision,
        transition: { type: "reply_pending" },
        now: context.now(),
      });
    }

    if (record.phase === "reply_pending") {
      return deliverReviewReceipt(context, record);
    }
    return completedReceipt(record);
  } catch (error) {
    const reasonCode =
      error instanceof ReviewSubmissionAdapterError
        ? error.reasonCode
        : "review_completion_failed";
    return completionFailed(context, record, reasonCode, errorMessage(error));
  }
}

function parseStoredReviewResult(
  json: string | null | undefined,
): CanonicalReviewResult {
  if (json === undefined || json === null) {
    throw new ReviewSubmissionAdapterError(
      "review_result_missing",
      "Durable routed review result is missing.",
    );
  }
  let parsed: CompleteRoutedReviewParameters;
  try {
    parsed = JSON.parse(json) as CompleteRoutedReviewParameters;
  } catch {
    throw new ReviewSubmissionAdapterError(
      "review_result_corrupt",
      "Durable routed review result is not valid JSON.",
    );
  }
  return canonicalReviewResult(parsed);
}

function parseFinalizationReceipt(
  payload: unknown,
  record: ReviewSubmissionRecord,
): {
  finalizationId: number;
  packetId: number;
  packetMessageId: number;
  exactHeadCommit: string;
  verdict: string;
  findingStatuses: ReviewFindingStatus[];
  taskStatus: string;
  materialDigest?: string;
} {
  assertDenProjectScope(payload, String(record.projectId), "finalize_review");
  const response = allObjects(payload).find(
    (value) =>
      numericValue(value, ["id", "finalization_id"]) !== undefined &&
      numericValue(value, ["packet_id", "packetId"]) !== undefined &&
      stringValue(value, ["verdict"]) !== undefined,
  );
  if (response === undefined) {
    throw new ReviewSubmissionAdapterError(
      "den_finalization_receipt_invalid",
      "Den finalize_review omitted its compact completion receipt.",
    );
  }
  const packetMessageId = numericValue(response, [
    "packet_message_id",
    "packetMessageId",
    "message_id",
    "messageId",
  ]);
  if (packetMessageId === undefined) {
    throw new ReviewSubmissionAdapterError(
      "den_finalization_receipt_invalid",
      "Den finalization receipt omitted its packet message id.",
    );
  }
  const exactHeadCommit = stringValue(response, [
    "exact_head_commit",
    "exactHeadCommit",
  ]);
  const verdict = stringValue(response, ["verdict"]);
  const taskStatus = stringValue(response, [
    "resulting_task_status",
    "target_task_status",
    "task_status",
  ]);
  if (
    exactHeadCommit === undefined ||
    verdict === undefined ||
    taskStatus === undefined
  ) {
    throw new ReviewSubmissionAdapterError(
      "den_finalization_receipt_invalid",
      "Den finalization receipt omitted exact head, verdict, or task status.",
    );
  }
  const findingStatuses = Array.isArray(response.finding_statuses)
    ? response.finding_statuses.flatMap((value): ReviewFindingStatus[] => {
        if (typeof value !== "object" || value === null) return [];
        const finding = value as Record<string, unknown>;
        const findingId = numericValue(finding, ["finding_id", "findingId"]);
        const status = stringValue(finding, ["status"]);
        return findingId === undefined || status === undefined
          ? []
          : [{ findingId, status }];
      })
    : [];
  return {
    finalizationId: requiredNumericId(response, ["id", "finalization_id"]),
    packetId: requiredNumericId(response, ["packet_id", "packetId"]),
    packetMessageId,
    exactHeadCommit,
    verdict,
    findingStatuses,
    taskStatus,
    materialDigest: stringValue(response, [
      "material_digest",
      "materialDigest",
    ]),
  };
}

async function deliverReviewReceipt(
  context: ServiceReviewSubmissionContext,
  record: ReviewSubmissionRecord,
): Promise<CompleteRoutedReviewToolReceipt> {
  const dispatchMessageId = record.dispatchMessageId;
  if (dispatchMessageId === undefined || dispatchMessageId === null) {
    return completionFailed(
      context,
      record,
      "review_dispatch_message_missing",
      "The routed review has no requester message to reply to.",
    );
  }
  const identity = record.submissionId.replaceAll(":", "-");
  const initial = await context.bridge.replyAgentMessage({
    caller: { type: "review_submission", submissionId: record.submissionId },
    deliveryId: `review-reply-delivery:${identity}`,
    idempotencyKey: `review-reply-delivery:${identity}`,
    messageId: `review-reply-message:${identity}`,
    inReplyToMessageId: dispatchMessageId,
    body: reviewReceiptBody(record),
    createdAt: context.now(),
    expiresAt: new Date(Date.now() + 24 * 60 * 60_000).toISOString(),
  });
  const receipt = await context.applyCoordinationDelivery(initial);
  if (receipt.status !== "accepted") {
    const reasonCode = receipt.reasonCode ?? "reviewer_reply_delivery_failed";
    try {
      await context.bridge.transitionReviewSubmission({
        submissionId: record.submissionId,
        expectedRevision: record.revision,
        transition: { type: "reply_terminal", reasonCode },
        now: context.now(),
      });
    } catch {}
    return completionFailed(
      context,
      record,
      reasonCode,
      `Review finalization succeeded, but the requester reply was ${receipt.status}.`,
    );
  }
  const settled = await context.bridge.transitionReviewSubmission({
    submissionId: record.submissionId,
    expectedRevision: record.revision,
    transition: {
      type: "reply_sent",
      replyMessageId: receipt.request.messageId,
      replyDeliveryId: receipt.request.deliveryId,
      replyStatus: receipt.status,
    },
    now: context.now(),
  });
  return completedReceipt(settled);
}

function reviewReceiptBody(record: ReviewSubmissionRecord): string {
  const findingStatuses = record.reviewFindingStatuses ?? [];
  const findings = findingStatuses.length
    ? findingStatuses
        .map((finding) => `#${finding.findingId}=${finding.status}`)
        .join(", ")
    : "none";
  return [
    `REVIEW COMPLETE — Den finalization ${record.reviewFinalizationId ?? "?"}.`,
    `Task #${record.taskId}; verdict=${record.reviewVerdict ?? "unknown"}; task=${record.reviewTaskStatus ?? "unknown"}.`,
    `Exact SHA ${record.reviewExactHeadCommit ?? record.commitSha}.`,
    `Findings: ${findings}. Packet/message ${record.reviewPacketId ?? "?"}/${record.reviewPacketMessageId ?? "?"}.`,
  ].join("\n");
}

function completedReceipt(
  record: ReviewSubmissionRecord,
): CompleteRoutedReviewToolReceipt {
  return boundCompletionReceipt({
    ok: true,
    submissionId: record.submissionId,
    taskId: Number(record.taskId),
    commitSha: record.commitSha,
    reviewRoundId: record.reviewRoundId ?? undefined,
    finalizationId: record.reviewFinalizationId ?? undefined,
    packetId: record.reviewPacketId ?? undefined,
    packetMessageId: record.reviewPacketMessageId ?? undefined,
    exactHeadCommit: record.reviewExactHeadCommit ?? undefined,
    verdict: record.reviewVerdict ?? undefined,
    findingStatuses: record.reviewFindingStatuses ?? [],
    taskStatus: record.reviewTaskStatus ?? undefined,
    replyMessageId: record.replyMessageId ?? undefined,
    replyStatus: record.replyStatus ?? undefined,
    summary: `Den finalized task #${record.taskId} at ${record.reviewExactHeadCommit ?? record.commitSha} with verdict ${record.reviewVerdict ?? "unknown"}; reply=${record.replyStatus ?? record.phase}.`,
  });
}

async function completionFailed(
  context: ServiceReviewSubmissionContext,
  record: ReviewSubmissionRecord,
  reasonCode: string,
  summary: string,
): Promise<CompleteRoutedReviewToolReceipt> {
  await recordAdapterFailure(context, record, reasonCode, summary).catch(
    () => {},
  );
  return boundCompletionReceipt({
    ok: false,
    submissionId: record.submissionId,
    taskId: Number(record.taskId),
    commitSha: record.commitSha,
    reasonCode,
    summary,
  });
}

function boundCompletionReceipt(
  receipt: CompleteRoutedReviewToolReceipt,
): CompleteRoutedReviewToolReceipt {
  let bounded = { ...receipt };
  while (
    serializedBytes(bounded) > 2048 &&
    (bounded.findingStatuses?.length ?? 0) > 0
  ) {
    bounded = {
      ...bounded,
      findingStatuses: bounded.findingStatuses?.slice(0, -1),
    };
  }
  if (serializedBytes(bounded) > 2048) {
    bounded = {
      ...bounded,
      summary: bounded.summary.slice(0, 256),
    };
  }
  return bounded;
}

function serializedBytes(value: unknown): number {
  return new TextEncoder().encode(JSON.stringify(value)).byteLength;
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
      isExternalCliSubmission(record)
        ? "The configured service Den MCP binding is unavailable."
        : "Submitting session has no active Den MCP binding.",
    );
    return failed(record, "den_mcp_binding_unavailable");
  }

  try {
    if (record.phase === "submitted") {
      const rounds = await reviewRounds(
        context,
        binding,
        Number(record.taskId),
        String(record.projectId),
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
            notes: reviewRequestNotes(record),
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
      ...(record.submitterSessionId === undefined
        ? {}
        : { session_key: record.submitterSessionId }),
      agent_profile: record.submitterAgentId,
    });
    assertDenProjectScope(
      gate,
      String(record.projectId),
      "watch_github_checks",
    );
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

async function reconcilePendingGate(
  context: ServiceReviewSubmissionContext,
  record: ReviewSubmissionRecord,
): Promise<void> {
  const binding = denBinding(context, record.submitterSessionId);
  if (binding === undefined || record.gateId === undefined) return;
  try {
    const gate = await denCall(context, binding, "get_github_check_gate", {
      task_id: Number(record.taskId),
      commit_sha: record.commitSha,
    });
    const state = existingGateState(gate, record);
    if (state.status === "pending") return;
    await context.bridge.transitionReviewSubmission({
      submissionId: record.submissionId,
      expectedRevision: record.revision,
      transition: {
        type: "gate_terminal",
        gateStatus: state.status,
        terminalReason: state.terminalReason,
      },
      now: context.now(),
    });
  } catch (error) {
    await recordAdapterFailure(
      context,
      record,
      error instanceof ReviewSubmissionAdapterError
        ? error.reasonCode
        : "github_gate_reconciliation_failed",
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
  sessionId?: string | null,
): McpBindingRecord | undefined {
  if (sessionId !== undefined && sessionId !== null) {
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
  const configuredBindingId = context.reviewDenBindingId?.trim();
  if (!configuredBindingId) return undefined;
  return context.runtimeConfig.mcpBindings.find(
    (binding) =>
      binding.status === "active" &&
      binding.bindingId === configuredBindingId &&
      binding.serverNames.includes("den"),
  );
}

async function denCall(
  context: ServiceReviewSubmissionContext,
  binding: McpBindingRecord,
  toolName: string,
  args: Record<string, unknown>,
): Promise<unknown> {
  if (context.callDenTool !== undefined) {
    return context.callDenTool(binding, toolName, args);
  }
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

type ExistingGateStatus =
  | "pending"
  | "passed"
  | "failed"
  | "timed_out"
  | "superseded";

function existingGateState(
  payload: unknown,
  record: ReviewSubmissionRecord,
): { status: ExistingGateStatus; terminalReason: string } {
  const gate = allObjects(payload).find((candidate) => {
    const gateId = numericValue(candidate, ["id", "gate_id", "gateId"]);
    const taskId = numericValue(candidate, ["task_id", "taskId"]);
    const commitSha = stringValue(candidate, ["commit_sha", "commitSha"]);
    return (
      gateId === record.gateId &&
      taskId === Number(record.taskId) &&
      commitSha?.toLowerCase() === record.commitSha.toLowerCase()
    );
  });
  if (gate === undefined) {
    throw new ReviewSubmissionAdapterError(
      "github_gate_scope_mismatch",
      `Den did not return gate ${record.gateId} for task #${record.taskId} at ${record.commitSha}.`,
    );
  }
  const status = stringValue(gate, ["status"]);
  if (
    status !== "pending" &&
    status !== "passed" &&
    status !== "failed" &&
    status !== "timed_out" &&
    status !== "superseded"
  ) {
    throw new ReviewSubmissionAdapterError(
      "github_gate_status_invalid",
      `Den returned an unsupported GitHub gate status for gate ${record.gateId}.`,
    );
  }
  return {
    status,
    terminalReason:
      stringValue(gate, ["terminal_reason", "terminalReason"]) ??
      (status === "passed" ? "checks_passed" : "github_gate_failed"),
  };
}

async function reviewRounds(
  context: ServiceReviewSubmissionContext,
  binding: McpBindingRecord,
  taskId: number,
  projectId: string,
): Promise<Record<string, unknown>[]> {
  const payload = await denCall(context, binding, "list_review_rounds", {
    task_id: taskId,
  });
  assertDenProjectScope(payload, projectId, "list_review_rounds");
  return allObjects(payload);
}

function reviewRequestNotes(record: ReviewSubmissionRecord): string {
  return [
    `Rusty Crew managed review project: ${record.projectId}.`,
    "The task id is the Den project-scoped review identity; do not substitute a direct or unrelated project review.",
    "",
    record.reviewSummaryMd,
  ].join("\n");
}

function assertDenProjectScope(
  payload: unknown,
  projectId: string,
  operation: string,
): void {
  const explicitProjects = allObjects(payload).flatMap((value) => {
    const candidate = value.project_id ?? value.projectId;
    return typeof candidate === "string" && candidate.trim() !== ""
      ? [candidate.trim()]
      : [];
  });
  const mismatch = explicitProjects.find(
    (candidate) => candidate !== projectId,
  );
  if (mismatch === undefined) return;
  throw new ReviewSubmissionAdapterError(
    "review_project_scope_mismatch",
    `${operation} returned project ${mismatch}, expected ${projectId}.`,
  );
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
    `Rusty Crew managed review submission: ${record.submissionId}.`,
    `Review Den task #${record.taskId} at exact SHA ${record.commitSha}.`,
    `Den project: ${record.projectId}. This is a Rusty Crew managed review submission; direct Den reviews are not attached to this workflow.`,
    `Repository: ${record.repository}`,
    `Ref: ${record.gitRef}`,
    `Review round: ${record.reviewRoundId ?? "unknown"}`,
    `If this reviewer session has multiple queued reviews, call complete_routed_review with taskId ${record.taskId} and commitSha ${record.commitSha} to select this review explicitly.`,
    "",
    record.reviewSummaryMd,
    "",
    isExternalCliSubmission(record)
      ? "Record findings and the verdict in Den, then call complete_routed_review. This was submitted by an external CLI; do not attempt a requester reply."
      : "Record findings and the verdict in Den, then call complete_routed_review. Crew sends the one receipt-based reply to the requester.",
  ].join("\n");
}

function isExternalCliSubmission(record: ReviewSubmissionRecord): boolean {
  return record.caller.type === "external_cli";
}

function accepted(record: ReviewSubmissionRecord): ReviewSubmissionToolReceipt {
  return {
    ok: true,
    submissionId: record.submissionId,
    phase: record.phase,
    projectId: String(record.projectId),
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
    projectId: String(record.projectId),
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
    projectId: input.projectId,
    taskId: input.taskId,
    commitSha: input.commitSha,
    reasonCode,
    summary:
      reasonCode === "review_project_not_allowed"
        ? `Den project ${input.projectId} is not enabled for managed Rusty Crew reviews.`
        : "Rusty Crew managed review project integration is not configured.",
  };
}

type ReviewProjectSelection =
  | { readonly ok: true; readonly projectId: string }
  | {
      readonly ok: false;
      readonly reasonCode:
        | "review_project_not_configured"
        | "review_project_not_allowed";
    };

function selectConfiguredReviewProject(
  context: ServiceReviewSubmissionContext,
  requestedProjectId: string,
): ReviewProjectSelection {
  const projectId = requestedProjectId.trim();
  if (context.reviewProjectIds.length === 0) {
    return { ok: false, reasonCode: "review_project_not_configured" };
  }
  if (!context.reviewProjectIds.includes(projectId)) {
    return { ok: false, reasonCode: "review_project_not_allowed" };
  }
  return { ok: true, projectId };
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

function numericValue(
  value: Record<string, unknown>,
  keys: string[],
): number | undefined {
  for (const key of keys) {
    const candidate = value[key];
    if (typeof candidate === "number" && Number.isSafeInteger(candidate)) {
      return candidate;
    }
  }
  return undefined;
}

function reviewerAgentId(
  context: ServiceReviewSubmissionContext,
  record: ReviewSubmissionRecord,
): string | undefined {
  const reviewerSessionId = record.reviewerSessionId;
  if (reviewerSessionId === undefined) return undefined;
  return context.runtimeConfig.sessions.find(
    (session) => session.sessionId === reviewerSessionId,
  )?.agentId;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function retryDue(record: ReviewSubmissionRecord): boolean {
  if (record.lastAdapterError === undefined) return true;
  return Date.now() - Date.parse(record.updatedAt) >= 30_000;
}

export class ReviewSubmissionAdapterError extends Error {
  constructor(
    readonly reasonCode: string,
    message: string,
  ) {
    super(message);
  }
}

function stringField(value: Record<string, unknown>, key: string): string {
  const candidate = value[key];
  return typeof candidate === "string" ? candidate : "";
}

function optionalStringField(
  value: Record<string, unknown>,
  key: string,
): string | undefined {
  const candidate = value[key];
  return candidate === undefined || candidate === null
    ? undefined
    : typeof candidate === "string"
      ? candidate
      : "";
}
