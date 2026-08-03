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

export type SubmitTaskForReviewParameters = Static<
  typeof submitTaskForReviewParameters
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

export interface ReviewSubmissionToolRuntime {
  submit(
    input: SubmitTaskForReviewParameters & {
      caller: AgentCoordinationCaller;
    },
  ): Promise<ReviewSubmissionToolReceipt>;
}

export function createReviewSubmissionToolResolver(
  runtime?: ReviewSubmissionToolRuntime,
): BrainToolResolver {
  return () => [submitTaskForReviewTool(runtime)];
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
