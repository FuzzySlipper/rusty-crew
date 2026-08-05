import { Type, type Static } from "typebox";
import type { AgentCoordinationCaller } from "@rusty-crew/contracts";
import type { BrainTool, BrainToolResult } from "./brain-tool.js";
import type { BrainToolResolver } from "./tool-session-selection.js";

const submitTaskForReviewParameters = Type.Object(
  {
    taskId: Type.Integer({ minimum: 1 }),
    repository: Type.String({ minLength: 3 }),
    commitSha: Type.String({ pattern: "^[0-9a-fA-F]{40}$" }),
    ref: Type.String({ minLength: 1 }),
    requiredChecks: Type.Array(Type.String({ minLength: 1 }), {
      minItems: 1,
    }),
    baseCommit: Type.Optional(Type.String({ pattern: "^[0-9a-fA-F]{40}$" })),
    reviewSummaryMd: Type.String({ minLength: 1 }),
    reviewer: Type.Optional(Type.String({ pattern: "^@[A-Za-z0-9._-]+$" })),
  },
  { additionalProperties: false },
);

const reviewFindingResolution = Type.Object(
  {
    findingId: Type.Integer({ minimum: 1 }),
    status: Type.String({ minLength: 1 }),
    verificationNote: Type.String({ minLength: 1 }),
  },
  { additionalProperties: false },
);

const reviewNewFinding = Type.Object(
  {
    category: Type.String({ minLength: 1 }),
    summary: Type.String({ minLength: 1 }),
    notes: Type.Optional(Type.String()),
    fileReferences: Type.Optional(Type.Array(Type.String({ minLength: 1 }))),
    testCommands: Type.Optional(Type.Array(Type.String({ minLength: 1 }))),
  },
  { additionalProperties: false },
);

const completeRoutedReviewParameters = Type.Object(
  {
    verdict: Type.Union([
      Type.Literal("looks_good"),
      Type.Literal("changes_requested"),
    ]),
    taskId: Type.Optional(Type.Integer({ minimum: 1 })),
    commitSha: Type.Optional(Type.String({ pattern: "^[0-9a-fA-F]{40}$" })),
    notes: Type.Optional(Type.String({ maxLength: 4096 })),
    evidence: Type.Optional(
      Type.Array(Type.String({ minLength: 1, maxLength: 512 }), {
        maxItems: 32,
      }),
    ),
    priorFindingResolutions: Type.Optional(Type.Array(reviewFindingResolution)),
    newFindings: Type.Optional(Type.Array(reviewNewFinding)),
  },
  { additionalProperties: false },
);

export type SubmitTaskForReviewParameters = Static<
  typeof submitTaskForReviewParameters
>;
export type CompleteRoutedReviewParameters = Static<
  typeof completeRoutedReviewParameters
>;

export interface ReviewSubmissionToolReceipt {
  ok: boolean;
  submissionId?: string;
  phase?: string;
  taskId: number;
  commitSha: string;
  reasonCode?: string;
  summary: string;
}

export interface CompleteRoutedReviewToolReceipt {
  ok: boolean;
  submissionId?: string;
  taskId?: number;
  commitSha?: string;
  reviewRoundId?: number;
  finalizationId?: number;
  packetId?: number;
  packetMessageId?: number;
  exactHeadCommit?: string;
  verdict?: string;
  findingStatuses?: Array<{ findingId: number; status: string }>;
  taskStatus?: string;
  replyMessageId?: string;
  replyStatus?: string;
  reasonCode?: string;
  summary: string;
}

export interface ReviewSubmissionToolRuntime {
  submit(
    input: SubmitTaskForReviewParameters & {
      caller: AgentCoordinationCaller;
    },
  ): Promise<ReviewSubmissionToolReceipt>;
  complete(
    input: CompleteRoutedReviewParameters & {
      caller: AgentCoordinationCaller;
      reviewerSessionId: string;
      correlationId?: string;
    },
  ): Promise<CompleteRoutedReviewToolReceipt>;
}

export function createReviewSubmissionToolResolver(
  runtime?: ReviewSubmissionToolRuntime,
): BrainToolResolver {
  return () => [
    submitTaskForReviewTool(runtime),
    completeRoutedReviewTool(runtime),
  ];
}

export function submitTaskForReviewTool(
  runtime?: ReviewSubmissionToolRuntime,
): BrainTool<
  typeof submitTaskForReviewParameters,
  ReviewSubmissionToolReceipt
> {
  return {
    name: "submit_task_for_review",
    label: "Submit task for review",
    description:
      "Use this tool for normal Den task review submission. It durably records the exact commit, runs required GitHub checks without model polling, and sends the passing result to the reviewer. Lower-level Den review and GitHub-gate tools are infrastructure and direct/unmanaged-session fallbacks.",
    parameters: submitTaskForReviewParameters,
    executeWithContext: async (params, context) => {
      if (runtime === undefined) {
        return result({
          ok: false,
          taskId: params.taskId,
          commitSha: params.commitSha,
          reasonCode: "review_submission_runtime_unavailable",
          summary: "Rusty Crew review submission runtime is unavailable.",
        });
      }
      const receipt = await runtime.submit({
        ...params,
        reviewer: params.reviewer ?? "@reviewer",
        caller: {
          type: "direct_brain",
          sessionId: context.sessionId,
          wakeId: context.wakeId,
          toolCallId: context.callId,
        },
      });
      return result(receipt, receipt.ok ? "complete_turn" : undefined);
    },
    execute: async (_callId, params) =>
      result({
        ok: false,
        taskId: params.taskId,
        commitSha: params.commitSha,
        reasonCode: "tool_context_required",
        summary: "submit_task_for_review requires trusted wake context.",
      }),
  };
}

export function completeRoutedReviewTool(
  runtime?: ReviewSubmissionToolRuntime,
): BrainTool<
  typeof completeRoutedReviewParameters,
  CompleteRoutedReviewToolReceipt
> {
  return {
    name: "complete_routed_review",
    label: "Complete routed review",
    description:
      "Complete the currently routed Den review using a structured verdict, finding resolutions, new findings, and notes. If queued reviews share this reviewer session, include taskId and commitSha from the review envelope to explicitly select the target. Den finalizes the authoritative round; Rusty Crew then sends exactly one receipt-based reply to the requester.",
    parameters: completeRoutedReviewParameters,
    executeWithContext: async (params, context) => {
      if (runtime === undefined) {
        return completeResult({
          ok: false,
          reasonCode: "review_submission_runtime_unavailable",
          summary: "Rusty Crew review completion runtime is unavailable.",
        });
      }
      const receipt = await runtime.complete({
        ...params,
        caller: {
          type: "review_submission",
          submissionId: "context-resolved",
        },
        reviewerSessionId: context.sessionId,
        correlationId:
          context.wake.state.pendingMessages.find((message) =>
            message.correlationId?.startsWith("review:"),
          )?.correlationId ?? undefined,
      });
      return completeResult(receipt, receipt.ok ? "complete_turn" : undefined);
    },
    execute: async (_callId, _params) =>
      completeResult({
        ok: false,
        reasonCode: "tool_context_required",
        summary: "complete_routed_review requires trusted wake context.",
      }),
  };
}

function result(
  receipt: ReviewSubmissionToolReceipt,
  turnDisposition?: "complete_turn",
): BrainToolResult<ReviewSubmissionToolReceipt> {
  return {
    content: [{ type: "text", text: receipt.summary }],
    details: receipt,
    ...(turnDisposition === undefined ? {} : { turnDisposition }),
  };
}

function completeResult(
  receipt: CompleteRoutedReviewToolReceipt,
  turnDisposition?: "complete_turn",
): BrainToolResult<CompleteRoutedReviewToolReceipt> {
  return {
    content: [{ type: "text", text: receipt.summary }],
    details: receipt,
    ...(turnDisposition === undefined ? {} : { turnDisposition }),
  };
}
