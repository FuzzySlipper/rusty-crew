import type { ReviewSubmissionRecord } from "@rusty-crew/contracts";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";
import type {
  RustyCrewDeploymentRole,
  RustyCrewMcpConfig,
  RustyCrewReviewDenAuthorityConfig,
} from "./service-config.js";
import type { RustyCrewRuntimeConfig } from "./service-runtime-config.js";
import {
  serviceReviewDenAuthority,
  type ReviewDenAuthorityDiagnostics,
} from "./service-review-den-authority.js";
import {
  buildServiceMcpEndpointConfig,
  callConfiguredMcpTool,
} from "./service-mcp-tools.js";

export interface ReviewOperatorConfigReadback {
  deploymentRole: RustyCrewDeploymentRole;
  authorityId?: string;
  endpointRef?: string;
  serverName: "den";
  toolProfileKey: "direct";
  auditIdentity?: string;
  credential: {
    present: boolean;
    source: "service_environment" | "none";
  };
  diagnostics: ReviewDenAuthorityDiagnostics;
}

export interface ReviewPipelinePage {
  projectId: string;
  deploymentRole: RustyCrewDeploymentRole;
  limit: number;
  offset: number;
  nextOffset?: number;
  denNextOffset?: number;
  items: ReviewPipelineItem[];
}

export interface ReviewPipelineItem {
  stableId: string;
  projectId: string;
  taskId: number;
  task?: Record<string, unknown>;
  latestRound: Record<string, unknown> | null;
  latestGate: Record<string, unknown> | null;
  submission?: ReviewSubmissionRecord;
  stage: string;
}

export function reviewOperatorConfigReadback(input: {
  deploymentRole: RustyCrewDeploymentRole;
  authority: RustyCrewReviewDenAuthorityConfig | undefined;
  diagnostics: ReviewDenAuthorityDiagnostics;
}): ReviewOperatorConfigReadback {
  const authority = input.authority;
  return {
    deploymentRole: input.deploymentRole,
    ...(authority === undefined
      ? {}
      : {
          authorityId: authority.authorityId,
          endpointRef: authority.endpointRef,
          auditIdentity: authority.auditIdentity,
        }),
    serverName: "den",
    toolProfileKey: "direct",
    credential: {
      present: authority?.bearerToken !== undefined,
      source:
        authority?.bearerToken === undefined ? "none" : "service_environment",
    },
    diagnostics: input.diagnostics,
  };
}

export async function composedReviewPipeline(input: {
  bridge: Pick<NativeBridgeModule, "listReviewSubmissions">;
  runtimeConfig: Pick<RustyCrewRuntimeConfig, "mcpServers">;
  mcpConfig: RustyCrewMcpConfig;
  authority: RustyCrewReviewDenAuthorityConfig | undefined;
  deploymentRole: RustyCrewDeploymentRole;
  projectId: string;
  limit: number;
  offset: number;
  callDenTool?: (
    name: string,
    args: Record<string, unknown>,
  ) => Promise<unknown>;
}): Promise<ReviewPipelinePage> {
  const authority = serviceReviewDenAuthority(input.authority);
  if (authority === undefined) {
    throw new Error(
      "Dedicated service review Den authority is not configured.",
    );
  }
  const denPayload = await (input.callDenTool?.("list_review_pipeline", {
    project_id: input.projectId,
    limit: input.limit,
    offset: input.offset,
  }) ??
    callReviewPipelineTool({
      authority,
      runtimeConfig: input.runtimeConfig,
      mcpConfig: input.mcpConfig,
      projectId: input.projectId,
      limit: input.limit,
      offset: input.offset,
    }));
  const denPage = reviewPipelinePageRecord(denPayload);
  const denItems = arrayValue(denPage.items);
  const submissions = (
    await input.bridge.listReviewSubmissions({
      pendingOnly: false,
    })
  )
    .filter((record) => record.projectId === input.projectId)
    .sort((left, right) => right.updatedAt.localeCompare(left.updatedAt));
  const submissionsByTask = new Map<number, ReviewSubmissionRecord[]>();
  for (const submission of submissions) {
    const taskId = Number(submission.taskId);
    if (!Number.isSafeInteger(taskId)) continue;
    const values = submissionsByTask.get(taskId) ?? [];
    values.push(submission);
    submissionsByTask.set(taskId, values);
  }

  const items: ReviewPipelineItem[] = [];
  const denTaskIds = new Set<number>();
  for (const rawItem of denItems) {
    if (!isRecord(rawItem) || !isRecord(rawItem.task)) continue;
    const taskId = numericValue(rawItem.task.id);
    if (taskId === undefined) continue;
    denTaskIds.add(taskId);
    const matches = submissionsByTask.get(taskId) ?? [];
    if (matches.length === 0) {
      items.push({
        stableId: `den-task:${input.projectId}:${taskId}`,
        projectId: input.projectId,
        taskId,
        task: rawItem.task,
        latestRound: recordOrNull(rawItem.latest_round),
        latestGate: recordOrNull(rawItem.latest_gate),
        stage: denOnlyStage(rawItem),
      });
      continue;
    }
    for (const submission of matches) {
      items.push({
        stableId: submission.submissionId,
        projectId: input.projectId,
        taskId,
        task: rawItem.task,
        latestRound: recordOrNull(rawItem.latest_round),
        latestGate: recordOrNull(rawItem.latest_gate),
        submission,
        stage: managedSubmissionStage(submission),
      });
    }
  }
  for (const submission of submissions) {
    const taskId = Number(submission.taskId);
    if (!Number.isSafeInteger(taskId) || denTaskIds.has(taskId)) continue;
    items.push({
      stableId: submission.submissionId,
      projectId: input.projectId,
      taskId,
      latestRound: null,
      latestGate: null,
      submission,
      stage: managedSubmissionStage(submission),
    });
  }

  const boundedItems = items.slice(0, input.limit);
  const denNextOffset = numericValue(denPage.next_offset);
  return {
    projectId: input.projectId,
    deploymentRole: input.deploymentRole,
    limit: input.limit,
    offset: input.offset,
    ...(items.length > boundedItems.length
      ? { nextOffset: input.offset + boundedItems.length }
      : denNextOffset === undefined
        ? {}
        : { nextOffset: denNextOffset }),
    ...(denNextOffset === undefined ? {} : { denNextOffset }),
    items: boundedItems,
  };
}

async function callReviewPipelineTool(input: {
  authority: NonNullable<ReturnType<typeof serviceReviewDenAuthority>>;
  runtimeConfig: Pick<RustyCrewRuntimeConfig, "mcpServers">;
  mcpConfig: RustyCrewMcpConfig;
  projectId: string;
  limit: number;
  offset: number;
}): Promise<unknown> {
  const result = await callConfiguredMcpTool({
    binding: input.authority.binding,
    config: buildServiceMcpEndpointConfig({
      mcpConfig: input.mcpConfig,
      mcpServers: input.runtimeConfig.mcpServers,
    }),
    toolName: "list_review_pipeline",
    arguments: {
      project_id: input.projectId,
      limit: input.limit,
      offset: input.offset,
    },
    ...(input.authority.config.bearerToken === undefined
      ? {}
      : { bearerToken: input.authority.config.bearerToken }),
    clientName: input.authority.auditIdentity,
  });
  if (result.isError) {
    throw new Error("Den list_review_pipeline returned an error");
  }
  return result.details;
}

function reviewPipelinePageRecord(value: unknown): Record<string, unknown> {
  if (isRecord(value) && isRecord(value.structuredContent)) {
    return value.structuredContent;
  }
  if (isRecord(value) && Array.isArray(value.items)) return value;
  if (isRecord(value) && Array.isArray(value.content)) {
    for (const part of value.content) {
      if (!isRecord(part) || typeof part.text !== "string") continue;
      try {
        const parsed: unknown = JSON.parse(part.text);
        if (isRecord(parsed)) return parsed;
      } catch {
        // Keep looking for a structured result.
      }
    }
  }
  throw new Error("Den list_review_pipeline returned an invalid page");
}

function managedSubmissionStage(record: ReviewSubmissionRecord): string {
  switch (record.phase) {
    case "submitted":
    case "den_handoff_recorded":
      return "managed_submission_accepted";
    case "gate_pending":
      return "github_gate_pending";
    case "gate_failed":
      return `github_gate_${record.gateStatus ?? "failed"}`;
    case "reviewer_dispatch_pending":
      return (record.reviewerDispatchAttempts ?? 0) > 0
        ? "reviewer_delivery_retrying"
        : "reviewer_delivery_queued";
    case "reviewer_dispatched":
      return "reviewer_dispatched";
    case "den_finalization_pending":
      return "den_finalization_pending";
    case "den_finalized":
    case "reply_pending":
      return "review_complete_reply_pending";
    case "replied":
      return "review_complete_replied";
    case "reply_terminal":
    case "review_terminal":
    case "superseded":
      return record.phase;
  }
}

function denOnlyStage(item: Record<string, unknown>): string {
  const gate = recordOrNull(item.latest_gate);
  if (gate !== null && typeof gate.status === "string") {
    return `den_gate_${gate.status}`;
  }
  if (recordOrNull(item.latest_round) !== null) return "den_review_round_open";
  return "den_reviewable_not_submitted";
}

function recordOrNull(value: unknown): Record<string, unknown> | null {
  return isRecord(value) ? value : null;
}

function arrayValue(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function numericValue(value: unknown): number | undefined {
  return typeof value === "number" && Number.isSafeInteger(value)
    ? value
    : undefined;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
