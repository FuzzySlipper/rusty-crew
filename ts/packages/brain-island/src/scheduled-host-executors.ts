import type { ScheduledRunSummary } from "@rusty-crew/contracts";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";
import type { AdminDiagnosticsContext } from "./admin-diagnostics-api.js";

export const RUNTIME_DIAGNOSTICS_SNAPSHOT_JOB_KIND =
  "runtime.diagnostics.snapshot";

export const scheduledHostJobKinds = [
  RUNTIME_DIAGNOSTICS_SNAPSHOT_JOB_KIND,
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
    if (run.jobKind !== RUNTIME_DIAGNOSTICS_SNAPSHOT_JOB_KIND) {
      throw new Error(`unsupported scheduled host job kind ${run.jobKind}`);
    }
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
    return "completed";
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
