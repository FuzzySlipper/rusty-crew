import type {
  AdminControlCommand,
  AdminControlExecutor,
  AdminControlOutcome,
} from "./admin-control-api.js";
import type { DelegatedResourceCleanupResult } from "./delegated-resource-cleanup.js";

export interface SchedulerAdminControlOptions {
  tick?: () => Promise<unknown> | unknown;
  runJob?: (jobId: string) => Promise<unknown> | unknown;
  pauseJob?: (jobId: string) => Promise<unknown> | unknown;
  resumeJob?: (jobId: string, nextDueAt?: string) => Promise<unknown> | unknown;
}

export interface BackgroundAdminControlOptions {
  scheduler?: SchedulerAdminControlOptions;
  cleanupDelegatedResources?: () =>
    | Promise<DelegatedResourceCleanupResult>
    | DelegatedResourceCleanupResult;
}

export function createBackgroundAdminControlExecutor(
  options: BackgroundAdminControlOptions,
): Pick<
  AdminControlExecutor,
  | "schedulerTick"
  | "schedulerRunJob"
  | "schedulerPauseJob"
  | "schedulerResumeJob"
  | "cleanupDelegatedResources"
> {
  return {
    schedulerTick: options.scheduler?.tick
      ? async () =>
          controlOutcome(
            "Scheduler tick completed.",
            await options.scheduler!.tick!(),
          )
      : undefined,
    schedulerRunJob: options.scheduler?.runJob
      ? async (command) => {
          const jobId = requiredTarget(command, "jobId");
          return controlOutcome(
            `Scheduler job ${jobId} run requested.`,
            await options.scheduler!.runJob!(jobId),
            { jobId },
          );
        }
      : undefined,
    schedulerPauseJob: options.scheduler?.pauseJob
      ? async (command) => {
          const jobId = requiredTarget(command, "jobId");
          return controlOutcome(
            `Scheduler job ${jobId} paused.`,
            await options.scheduler!.pauseJob!(jobId),
            { jobId },
          );
        }
      : undefined,
    schedulerResumeJob: options.scheduler?.resumeJob
      ? async (command) => {
          const jobId = requiredTarget(command, "jobId");
          const nextDueAt = stringBody(command, "nextDueAt");
          return controlOutcome(
            `Scheduler job ${jobId} resumed.`,
            await options.scheduler!.resumeJob!(jobId, nextDueAt),
            { jobId },
          );
        }
      : undefined,
    cleanupDelegatedResources: options.cleanupDelegatedResources
      ? async () => {
          const result = await options.cleanupDelegatedResources!();
          const archived =
            result.runtime.terminalArchived.length +
            result.runtime.orphanedArchived.length +
            result.runtime.expiredArchived.length;
          return controlOutcome(
            `Delegated resource cleanup archived ${archived} session(s).`,
            result,
          );
        }
      : undefined,
  };
}

function controlOutcome(
  summary: string,
  result: unknown,
  affectedIds?: Record<string, string | number>,
): AdminControlOutcome {
  return {
    status: "completed",
    summary,
    affectedIds,
    result,
  };
}

function requiredTarget(command: AdminControlCommand, key: string): string {
  const value = command.target[key];
  if (!value) throw new Error(`missing ${key}`);
  return value;
}

function stringBody(
  command: AdminControlCommand,
  key: string,
): string | undefined {
  const value = command.body[key];
  return typeof value === "string" && value.trim() ? value : undefined;
}
