import type { ScheduledRunSummary } from "@rusty-crew/contracts";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";
import type { AdminDiagnosticsContext } from "./admin-diagnostics-api.js";
import type {
  BackgroundReviewPayload,
  BackgroundReviewResult,
} from "./background-memory-skill-review.js";

export const RUNTIME_DIAGNOSTICS_SNAPSHOT_JOB_KIND =
  "runtime.diagnostics.snapshot";
export const RUNTIME_REVIEW_MEMORY_SKILLS_JOB_KIND =
  "runtime.review.memory_skills";

export const scheduledHostJobKinds = [
  RUNTIME_DIAGNOSTICS_SNAPSHOT_JOB_KIND,
  RUNTIME_REVIEW_MEMORY_SKILLS_JOB_KIND,
] as const;

export interface ScheduledHostExecutorReport {
  claimed: number;
  completed: number;
  failed: number;
  runs: ScheduledRunSummary[];
}

export interface ScheduledHostExecutorContext {
  bridge: Pick<
    NativeBridgeModule,
    "claimScheduledHostRuns" | "completeScheduledHostRun"
  >;
  limit?: number;
  diagnostics: () => Promise<AdminDiagnosticsContext>;
  jobPayload?(run: ScheduledRunSummary): unknown;
  backgroundReview?(
    run: ScheduledRunSummary,
    payload: BackgroundReviewPayload,
  ): Promise<BackgroundReviewResult>;
}

export async function runScheduledHostExecutors(
  context: ScheduledHostExecutorContext,
): Promise<ScheduledHostExecutorReport> {
  const runs = await context.bridge.claimScheduledHostRuns({
    supportedJobKinds: [...scheduledHostJobKinds],
    limit: context.limit ?? 5,
  });
  let completed = 0;
  let failed = 0;
  for (const run of runs) {
    const outcome = await executeScheduledHostRun(context, run);
    if (outcome === "completed") completed += 1;
    if (outcome === "failed") failed += 1;
  }
  return {
    claimed: runs.length,
    completed,
    failed,
    runs,
  };
}

export async function executeScheduledHostRun(
  context: Omit<ScheduledHostExecutorContext, "limit">,
  run: ScheduledRunSummary,
): Promise<"completed" | "failed"> {
  try {
    if (run.jobKind === RUNTIME_DIAGNOSTICS_SNAPSHOT_JOB_KIND) {
      await completeDiagnosticsSnapshot(context, run);
      return "completed";
    }
    if (run.jobKind === RUNTIME_REVIEW_MEMORY_SKILLS_JOB_KIND) {
      await completeBackgroundReview(context, run);
      return "completed";
    }
    throw new Error(`unsupported scheduled host job kind ${run.jobKind}`);
  } catch (error) {
    await context.bridge.completeScheduledHostRun({
      runId: run.runId,
      status: "failed",
      error: error instanceof Error ? error.message : String(error),
      output: {
        outcome: "failed",
        summary: "Diagnostics snapshot failed.",
        candidate_count: 0,
        changed_count: 0,
        result_refs: [],
        safe_error: error instanceof Error ? error.message : String(error),
      },
    });
    return "failed";
  }
}

async function completeDiagnosticsSnapshot(
  context: Omit<ScheduledHostExecutorContext, "limit">,
  run: ScheduledRunSummary,
): Promise<void> {
  const diagnostics = await context.diagnostics();
  const summary = diagnostics.diagnostics.summary;
  await context.bridge.completeScheduledHostRun({
    runId: run.runId,
    status: "completed",
    output: {
      outcome: "completed",
      summary: `Diagnostics snapshot captured ${summary.sessions} session(s), ${summary.pendingQueueItems} pending queue item(s), and ${summary.recentErrors} recent error(s).`,
      candidate_count: summary.sessions,
      changed_count: 0,
      result_refs: [
        {
          kind: "admin_route",
          path: "/v1/admin/diagnostics",
        },
      ],
      diagnostics: {
        generatedAt: diagnostics.diagnostics.generatedAt,
        health: diagnostics.diagnostics.health,
        summary,
      },
    },
  });
}

async function completeBackgroundReview(
  context: Omit<ScheduledHostExecutorContext, "limit">,
  run: ScheduledRunSummary,
): Promise<void> {
  if (!context.backgroundReview) {
    throw new Error("background review executor is not configured");
  }
  const result = await context.backgroundReview(
    run,
    backgroundReviewPayload(context.jobPayload?.(run)),
  );
  await context.bridge.completeScheduledHostRun({
    runId: run.runId,
    status: "completed",
    output: {
      outcome: "completed",
      summary: `Background ${result.reviewType} review for ${result.profileId} produced ${result.findingCount} finding(s).`,
      candidate_count: result.candidateCount,
      changed_count: 0,
      result_refs: [result.resultRef],
      review: {
        profileId: result.profileId,
        reviewType: result.reviewType,
        triggerSource: result.triggerSource,
        findingCount: result.findingCount,
        skippedCount: result.skippedCount,
        skippedReasons: result.skippedReasons,
        dryRun: result.dryRun,
        startedAt: result.startedAt,
        finishedAt: result.finishedAt,
      },
    },
  });
}

function backgroundReviewPayload(input: unknown): BackgroundReviewPayload {
  const raw = isRecord(input) ? input : {};
  const profileId =
    stringValue(raw.profileId) ??
    stringValue(raw.profile_id) ??
    "unknown-profile";
  return {
    schemaVersion:
      numberValue(raw.schemaVersion) === 1 ||
      numberValue(raw.schema_version) === 1
        ? 1
        : undefined,
    reviewType: reviewType(raw.reviewType ?? raw.review_type),
    profileId,
    triggerSource:
      stringValue(raw.triggerSource) ?? stringValue(raw.trigger_source),
    maxFindings: numberValue(raw.maxFindings) ?? numberValue(raw.max_findings),
    maxCandidates:
      numberValue(raw.maxCandidates) ?? numberValue(raw.max_candidates),
    maxTokens: numberValue(raw.maxTokens) ?? numberValue(raw.max_tokens),
    memoryNudgeInterval:
      numberValue(raw.memoryNudgeInterval) ??
      numberValue(raw.memory_nudge_interval),
    skillNudgeInterval:
      numberValue(raw.skillNudgeInterval) ??
      numberValue(raw.skill_nudge_interval),
    includeDenseProfileMemory:
      booleanValue(raw.includeDenseProfileMemory) ??
      booleanValue(raw.include_dense_profile_memory),
    includeDenMemoryDiagnostics:
      booleanValue(raw.includeDenMemoryDiagnostics) ??
      booleanValue(raw.include_den_memory_diagnostics),
    llmReviewEnabled:
      booleanValue(raw.llmReviewEnabled) ??
      booleanValue(raw.llm_review_enabled),
    dryRun: booleanValue(raw.dryRun) ?? booleanValue(raw.dry_run),
    reason: stringValue(raw.reason),
  };
}

function reviewType(input: unknown): BackgroundReviewPayload["reviewType"] {
  return input === "memory" || input === "skills" || input === "combined"
    ? input
    : "combined";
}

function isRecord(input: unknown): input is Record<string, unknown> {
  return input !== null && typeof input === "object" && !Array.isArray(input);
}

function stringValue(input: unknown): string | undefined {
  return typeof input === "string" && input.trim() ? input : undefined;
}

function numberValue(input: unknown): number | undefined {
  return typeof input === "number" && Number.isFinite(input)
    ? input
    : undefined;
}

function booleanValue(input: unknown): boolean | undefined {
  return typeof input === "boolean" ? input : undefined;
}
