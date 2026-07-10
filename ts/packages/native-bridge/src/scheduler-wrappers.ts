import type {
  RunId,
  ScheduledHostJobManualRunRequest,
  ScheduledHostJobRegistrationInput,
  ScheduledHostRunClaimQuery,
  ScheduledHostRunCompletionInput,
  ScheduledJobListQuery,
  ScheduledJobStatus,
  ScheduledJobSummary,
  ScheduledRunListQuery,
  ScheduledRunStatus,
  ScheduledRunSummary,
  ScheduledRunTrigger,
  SchedulerTickReport,
  SessionId,
  Unit,
} from "@rusty-crew/contracts";

import type { NativeBridgeBinding } from "./generated/native-binding-surface.js";

interface RawScheduledJobSummary {
  job_id: string;
  job_kind: string;
  target_session_id?: SessionId;
  interval_ms?: number;
  next_due_at?: string;
  status: ScheduledJobStatus;
  created_at: string;
  updated_at: string;
  paused_at?: string;
}

interface RawScheduledRunSummary {
  run_id: RunId;
  job_id: string;
  job_kind: string;
  target_session_id?: SessionId;
  status: ScheduledRunStatus;
  trigger: ScheduledRunTrigger;
  scheduled_for?: string;
  claimed_at: string;
  claim_deadline_at: string;
  completed_at?: string;
  error?: string;
  output?: unknown;
  created_at: string;
  updated_at: string;
}

interface RawSchedulerTickReport {
  stale_runs_expired: number;
  due_runs_claimed: number;
  wakes_requested: number;
  runs_completed: number;
  runs_skipped: number;
  runs_failed: number;
}

export function createNativeBridgeSchedulerMethods(
  binding: NativeBridgeBinding,
) {
  return {
    registerScheduledWakeJob: async (input: {
      jobId: string;
      targetSessionId: SessionId;
      intervalMs?: number;
      firstDueAt: string;
    }): Promise<ScheduledJobSummary> =>
      toScheduledJobSummary(
        JSON.parse(
          binding.registerScheduledWakeJobJson(
            input.jobId,
            input.targetSessionId,
            input.intervalMs,
            input.firstDueAt,
          ),
        ) as RawScheduledJobSummary,
      ),
    registerScheduledHostJob: async (
      input: ScheduledHostJobRegistrationInput,
    ): Promise<ScheduledJobSummary> =>
      toScheduledJobSummary(
        JSON.parse(
          binding.registerScheduledHostJobJson(
            input.jobId,
            input.jobKind,
            input.intervalMs,
            input.firstDueAt,
            JSON.stringify(input.payload ?? {}),
          ),
        ) as RawScheduledJobSummary,
      ),
    listScheduledJobs: async (
      query: ScheduledJobListQuery = {},
    ): Promise<ScheduledJobSummary[]> =>
      (
        JSON.parse(
          binding.listScheduledJobsJson(
            query.status,
            query.jobKind,
            query.limit,
            query.offset,
          ),
        ) as RawScheduledJobSummary[]
      ).map(toScheduledJobSummary),
    listScheduledRuns: async (
      query: ScheduledRunListQuery = {},
    ): Promise<ScheduledRunSummary[]> =>
      (
        JSON.parse(
          binding.listScheduledRunsJson(
            query.jobId,
            query.status,
            query.trigger,
            query.targetSessionId,
            query.limit,
            query.offset,
          ),
        ) as RawScheduledRunSummary[]
      ).map(toScheduledRunSummary),
    claimScheduledHostRuns: async (
      query: ScheduledHostRunClaimQuery,
    ): Promise<ScheduledRunSummary[]> =>
      (
        JSON.parse(
          binding.claimScheduledHostRunsJson(
            query.supportedJobKinds,
            query.limit,
          ),
        ) as RawScheduledRunSummary[]
      ).map(toScheduledRunSummary),
    requestScheduledHostJobRun: async (
      input: ScheduledHostJobManualRunRequest,
    ): Promise<ScheduledRunSummary | undefined> => {
      const raw = JSON.parse(
        binding.requestScheduledHostJobRunJson(
          input.jobId,
          input.supportedJobKinds,
        ),
      ) as RawScheduledRunSummary | null;
      return raw ? toScheduledRunSummary(raw) : undefined;
    },
    completeScheduledHostRun: async (
      input: ScheduledHostRunCompletionInput,
    ): Promise<Unit> => {
      binding.completeScheduledHostRun(
        input.runId,
        input.status,
        JSON.stringify(input.output ?? {}),
        input.error,
      );
      return {};
    },
    runSchedulerTick: async (): Promise<SchedulerTickReport> =>
      toSchedulerTickReport(
        JSON.parse(binding.runSchedulerTickJson()) as RawSchedulerTickReport,
      ),
    requestScheduledJobRun: async (
      jobId: string,
    ): Promise<ScheduledRunSummary | undefined> => {
      const raw = JSON.parse(
        binding.requestScheduledJobRunJson(jobId),
      ) as RawScheduledRunSummary | null;
      return raw ? toScheduledRunSummary(raw) : undefined;
    },
    pauseScheduledJob: async (jobId: string): Promise<Unit> => {
      binding.pauseScheduledJob(jobId);
      return {};
    },
    resumeScheduledJob: async (input: {
      jobId: string;
      nextDueAt: string;
    }): Promise<Unit> => {
      binding.resumeScheduledJob(input.jobId, input.nextDueAt);
      return {};
    },
  };
}

function toScheduledJobSummary(
  raw: RawScheduledJobSummary,
): ScheduledJobSummary {
  return {
    jobId: raw.job_id,
    jobKind: raw.job_kind,
    targetSessionId: raw.target_session_id,
    intervalMs: raw.interval_ms,
    nextDueAt: raw.next_due_at,
    status: raw.status,
    createdAt: raw.created_at,
    updatedAt: raw.updated_at,
    pausedAt: raw.paused_at,
  };
}

function toScheduledRunSummary(
  raw: RawScheduledRunSummary,
): ScheduledRunSummary {
  return {
    runId: raw.run_id,
    jobId: raw.job_id,
    jobKind: raw.job_kind,
    targetSessionId: raw.target_session_id,
    status: raw.status,
    trigger: raw.trigger,
    scheduledFor: raw.scheduled_for,
    claimedAt: raw.claimed_at,
    claimDeadlineAt: raw.claim_deadline_at,
    completedAt: raw.completed_at,
    error: raw.error,
    output: raw.output,
    createdAt: raw.created_at,
    updatedAt: raw.updated_at,
  };
}

function toSchedulerTickReport(
  raw: RawSchedulerTickReport,
): SchedulerTickReport {
  return {
    staleRunsExpired: raw.stale_runs_expired,
    dueRunsClaimed: raw.due_runs_claimed,
    wakesRequested: raw.wakes_requested,
    runsCompleted: raw.runs_completed,
    runsSkipped: raw.runs_skipped,
    runsFailed: raw.runs_failed,
  };
}
