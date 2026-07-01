export type BackgroundServiceHealth = "ok" | "degraded" | "blocked";

export interface SchedulerBackgroundDiagnostics {
  heartbeatEnabled?: boolean;
  heartbeatIntervalMs?: number;
  heartbeatRunning?: boolean;
  lastHeartbeatStartedAt?: string;
  lastHeartbeatCompletedAt?: string;
  lastHeartbeatDurationMs?: number;
  lastHeartbeatSummary?: string;
  lastHeartbeatSkippedAt?: string;
  lastHeartbeatSkipReason?: string;
  jobCount: number;
  activeJobs: number;
  pausedJobs: number;
  staleRuns: number;
  runningRuns?: number;
  failedRuns?: number;
  nextDueAt?: string;
  lastRunAt?: string;
  lastError?: string;
}

export interface CuratorBackgroundDiagnostics {
  status: "available" | "paused" | "degraded" | "unavailable";
  candidateCount?: number;
  mutationCount?: number;
  lastRunAt?: string;
  lastMutationAt?: string;
  lastError?: string;
}

export interface BackgroundReviewDiagnostics {
  enabled: boolean;
  recentFindings: number;
  lastCaptureProposalCount?: number;
  lastPersistedCaptureProposalCount?: number;
  lastSkippedReasons?: readonly string[];
  lastRunAt?: string;
  lastError?: string;
}

export interface CleanupBackgroundDiagnostics {
  lastRunAt?: string;
  terminalArchived?: number;
  orphanedArchived?: number;
  expiredArchived?: number;
  adapterReleased?: number;
  adapterDegraded?: number;
  lastError?: string;
}

export interface BackgroundServiceDiagnosticsInput {
  now: string;
  scheduler?: SchedulerBackgroundDiagnostics;
  curator?: CuratorBackgroundDiagnostics;
  backgroundReview?: BackgroundReviewDiagnostics;
  cleanup?: CleanupBackgroundDiagnostics;
}

export interface BackgroundServiceIssue {
  source: "scheduler" | "curator" | "background_review" | "cleanup";
  severity: Exclude<BackgroundServiceHealth, "ok">;
  reasonCode: string;
  message: string;
}

export interface BackgroundServiceDiagnosticsProjection {
  generatedAt: string;
  health: BackgroundServiceHealth;
  degraded: boolean;
  summary: {
    activeJobs: number;
    pausedJobs: number;
    staleRuns: number;
    curatorCandidates: number;
    curatorMutations: number;
    recentReviewFindings: number;
    cleanupArchived: number;
    adapterDegraded: number;
  };
  scheduler?: SchedulerBackgroundDiagnostics;
  curator?: CuratorBackgroundDiagnostics;
  backgroundReview?: BackgroundReviewDiagnostics;
  cleanup?: CleanupBackgroundDiagnostics;
  issues: BackgroundServiceIssue[];
}

export function buildBackgroundServiceDiagnosticsProjection(
  input: BackgroundServiceDiagnosticsInput,
): BackgroundServiceDiagnosticsProjection {
  const issues = [
    ...schedulerIssues(input.scheduler),
    ...curatorIssues(input.curator),
    ...backgroundReviewIssues(input.backgroundReview),
    ...cleanupIssues(input.cleanup),
    ...missingIssues(input),
  ];
  const health = summarizeBackgroundHealth(issues);
  const cleanupArchived =
    (input.cleanup?.terminalArchived ?? 0) +
    (input.cleanup?.orphanedArchived ?? 0) +
    (input.cleanup?.expiredArchived ?? 0);
  return {
    generatedAt: input.now,
    health,
    degraded: health !== "ok",
    summary: {
      activeJobs: input.scheduler?.activeJobs ?? 0,
      pausedJobs: input.scheduler?.pausedJobs ?? 0,
      staleRuns: input.scheduler?.staleRuns ?? 0,
      curatorCandidates: input.curator?.candidateCount ?? 0,
      curatorMutations: input.curator?.mutationCount ?? 0,
      recentReviewFindings: input.backgroundReview?.recentFindings ?? 0,
      cleanupArchived,
      adapterDegraded: input.cleanup?.adapterDegraded ?? 0,
    },
    scheduler: input.scheduler,
    curator: input.curator,
    backgroundReview: input.backgroundReview,
    cleanup: input.cleanup,
    issues,
  };
}

function schedulerIssues(
  scheduler: SchedulerBackgroundDiagnostics | undefined,
): BackgroundServiceIssue[] {
  if (!scheduler) return [];
  return [
    ...(scheduler.staleRuns > 0
      ? [
          {
            source: "scheduler" as const,
            severity: "degraded" as const,
            reasonCode: "scheduler_stale_runs",
            message: `${scheduler.staleRuns} scheduler run(s) are stale.`,
          },
        ]
      : []),
    ...(scheduler.lastError
      ? [
          {
            source: "scheduler" as const,
            severity: "blocked" as const,
            reasonCode: "scheduler_error",
            message: scheduler.lastError,
          },
        ]
      : []),
  ];
}

function curatorIssues(
  curator: CuratorBackgroundDiagnostics | undefined,
): BackgroundServiceIssue[] {
  if (!curator) return [];
  if (curator.status === "available" || curator.status === "paused") {
    return curator.lastError
      ? [
          {
            source: "curator",
            severity: "degraded",
            reasonCode: "curator_error",
            message: curator.lastError,
          },
        ]
      : [];
  }
  return [
    {
      source: "curator",
      severity: curator.status === "unavailable" ? "blocked" : "degraded",
      reasonCode: `curator_${curator.status}`,
      message: curator.lastError ?? `Curator is ${curator.status}.`,
    },
  ];
}

function backgroundReviewIssues(
  review: BackgroundReviewDiagnostics | undefined,
): BackgroundServiceIssue[] {
  if (!review) return [];
  return review.lastError
    ? [
        {
          source: "background_review",
          severity: "degraded",
          reasonCode: "background_review_error",
          message: review.lastError,
        },
      ]
    : [];
}

function cleanupIssues(
  cleanup: CleanupBackgroundDiagnostics | undefined,
): BackgroundServiceIssue[] {
  if (!cleanup) return [];
  return [
    ...(cleanup.adapterDegraded && cleanup.adapterDegraded > 0
      ? [
          {
            source: "cleanup" as const,
            severity: "degraded" as const,
            reasonCode: "cleanup_adapter_degraded",
            message: `${cleanup.adapterDegraded} adapter cleanup(s) degraded.`,
          },
        ]
      : []),
    ...(cleanup.lastError
      ? [
          {
            source: "cleanup" as const,
            severity: "degraded" as const,
            reasonCode: "cleanup_error",
            message: cleanup.lastError,
          },
        ]
      : []),
  ];
}

function missingIssues(
  input: BackgroundServiceDiagnosticsInput,
): BackgroundServiceIssue[] {
  const issues: BackgroundServiceIssue[] = [];
  if (!input.scheduler) {
    issues.push({
      source: "scheduler",
      severity: "degraded",
      reasonCode: "scheduler_diagnostics_missing",
      message: "Scheduler diagnostics are not configured.",
    });
  }
  if (!input.curator) {
    issues.push({
      source: "curator",
      severity: "degraded",
      reasonCode: "curator_diagnostics_missing",
      message: "Curator diagnostics are not configured.",
    });
  }
  return issues;
}

function summarizeBackgroundHealth(
  issues: readonly BackgroundServiceIssue[],
): BackgroundServiceHealth {
  if (issues.some((issue) => issue.severity === "blocked")) return "blocked";
  if (issues.some((issue) => issue.severity === "degraded")) return "degraded";
  return "ok";
}
