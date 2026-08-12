import { createHash } from "node:crypto";
import type {
  AgentRouteResolution,
  ReviewSubmissionRecord,
} from "@rusty-crew/contracts";
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
  configRevision: string;
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
  reviewerRoute: AgentRouteResolution;
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
  reviewerRoute: AgentRouteResolution;
}): ReviewOperatorConfigReadback {
  const authority = input.authority;
  return {
    configRevision: reviewOperatorConfigRevision(authority),
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
    reviewerRoute: input.reviewerRoute,
  };
}

export function reviewOperatorConfigRevision(
  authority: RustyCrewReviewDenAuthorityConfig | undefined,
): string {
  return createHash("sha256")
    .update(
      JSON.stringify(
        authority === undefined
          ? null
          : {
              authorityId: authority.authorityId,
              endpointRef: authority.endpointRef,
              auditIdentity: authority.auditIdentity,
            },
      ),
    )
    .digest("hex");
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
  const { denOffset, crewOffset } = decodePipelineOffset(input.offset);
  const denExhausted = denOffset === PIPELINE_OFFSET_EXHAUSTED;
  const crewExhausted = crewOffset === PIPELINE_OFFSET_EXHAUSTED;
  const denCapacity =
    input.limit === 1 ? (denExhausted ? 0 : 1) : Math.ceil(input.limit / 2);
  const crewCapacity =
    input.limit === 1
      ? denExhausted && !crewExhausted
        ? 1
        : 0
      : input.limit - denCapacity;
  const authority = serviceReviewDenAuthority(input.authority);
  if (authority === undefined) {
    throw new Error(
      "Dedicated service review Den authority is not configured.",
    );
  }
  const denPayload =
    denExhausted || denCapacity === 0
      ? { items: [] }
      : await (input.callDenTool?.("list_review_pipeline", {
          project_id: input.projectId,
          limit: denCapacity,
          offset: denOffset,
        }) ??
          callReviewPipelineTool({
            authority,
            runtimeConfig: input.runtimeConfig,
            mcpConfig: input.mcpConfig,
            projectId: input.projectId,
            limit: denCapacity,
            offset: denOffset,
          }));
  const denPage = reviewPipelinePageRecord(denPayload);
  const denItems = arrayValue(denPage.items);
  const crewPage =
    crewCapacity === 0 || crewExhausted
      ? []
      : await input.bridge.listReviewSubmissions({
          projectId: input.projectId,
          pendingOnly: false,
          limit: crewCapacity + 1,
          offset: crewOffset,
        });

  const items: ReviewPipelineItem[] = [];
  const denItemsByTask = new Map<number, Record<string, unknown>>();
  for (const rawItem of denItems) {
    if (!isRecord(rawItem) || !isRecord(rawItem.task)) continue;
    const taskId = numericValue(rawItem.task.id);
    if (taskId === undefined) continue;
    denItemsByTask.set(taskId, rawItem);
    const hasManagedSubmission =
      (
        await input.bridge.listReviewSubmissions({
          projectId: input.projectId,
          taskId: String(taskId),
          pendingOnly: false,
          limit: 1,
          offset: 0,
        })
      ).length > 0;
    if (!hasManagedSubmission) {
      items.push({
        stableId: `den-task:${input.projectId}:${taskId}`,
        projectId: input.projectId,
        taskId,
        task: rawItem.task,
        latestRound: recordOrNull(rawItem.latest_round),
        latestGate: recordOrNull(rawItem.latest_gate),
        stage: denOnlyStage(rawItem),
      });
    }
  }
  for (const submission of crewPage.slice(0, crewCapacity)) {
    const taskId = Number(submission.taskId);
    if (!Number.isSafeInteger(taskId)) continue;
    const denItem = denItemsByTask.get(taskId);
    items.push({
      stableId: submission.submissionId,
      projectId: input.projectId,
      taskId,
      ...(denItem !== undefined && isRecord(denItem.task)
        ? { task: denItem.task }
        : {}),
      latestRound:
        denItem === undefined ? null : recordOrNull(denItem.latest_round),
      latestGate:
        denItem === undefined ? null : recordOrNull(denItem.latest_gate),
      submission,
      stage: managedSubmissionStage(submission),
    });
  }

  const denNextOffset = numericValue(denPage.next_offset);
  const crewHasNext = crewPage.length > crewCapacity;
  const nextDenOffset = denExhausted
    ? PIPELINE_OFFSET_EXHAUSTED
    : denCapacity === 0
      ? denOffset
      : (denNextOffset ?? PIPELINE_OFFSET_EXHAUSTED);
  const nextCrewOffset = crewExhausted
    ? PIPELINE_OFFSET_EXHAUSTED
    : crewCapacity === 0
      ? crewOffset
      : crewHasNext
        ? crewOffset + crewCapacity
        : PIPELINE_OFFSET_EXHAUSTED;
  const hasNext =
    nextDenOffset !== PIPELINE_OFFSET_EXHAUSTED ||
    nextCrewOffset !== PIPELINE_OFFSET_EXHAUSTED;
  return {
    projectId: input.projectId,
    deploymentRole: input.deploymentRole,
    limit: input.limit,
    offset: input.offset,
    ...(hasNext
      ? { nextOffset: encodePipelineOffset(nextDenOffset, nextCrewOffset) }
      : {}),
    ...(denNextOffset === undefined ? {} : { denNextOffset }),
    items,
  };
}

const PIPELINE_OFFSET_RADIX = 1_000_000;
const PIPELINE_OFFSET_EXHAUSTED = PIPELINE_OFFSET_RADIX - 1;

function decodePipelineOffset(offset: number): {
  denOffset: number;
  crewOffset: number;
} {
  return {
    denOffset: Math.floor(offset / PIPELINE_OFFSET_RADIX),
    crewOffset: offset % PIPELINE_OFFSET_RADIX,
  };
}

function encodePipelineOffset(denOffset: number, crewOffset: number): number {
  if (
    denOffset > PIPELINE_OFFSET_EXHAUSTED ||
    crewOffset > PIPELINE_OFFSET_EXHAUSTED
  ) {
    throw new Error("review pipeline offset exceeds the bounded cursor range");
  }
  return denOffset * PIPELINE_OFFSET_RADIX + crewOffset;
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

export function managedSubmissionStage(record: ReviewSubmissionRecord): string {
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
