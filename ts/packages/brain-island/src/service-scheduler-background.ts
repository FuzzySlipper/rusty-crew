import type { NativeBridgeModule } from "@rusty-crew/native-bridge";
import type {
  CuratorLifecyclePlanner,
  CuratorLifecycleReport,
} from "./curator-lifecycle.js";
import { runCuratorLifecycleTransitions } from "./curator-lifecycle.js";
import type { MemoryCuratorGovernanceStore } from "./curator-mutations.js";
import {
  registerConfiguredScheduledJobs,
  type RustyCrewRuntimeConfig,
} from "./service-runtime-config.js";
import { runScheduledHostExecutors } from "./scheduled-host-executors.js";

export interface SchedulerServiceEvent {
  source: string;
  eventType: string;
  summary: string;
  severity?: "info" | "warning" | "error";
}

export interface SchedulerHeartbeatState {
  enabled: boolean;
  intervalMs: number;
  running: boolean;
  lastStartedAt?: string;
  lastCompletedAt?: string;
  lastDurationMs?: number;
  lastSummary?: string;
  lastSkippedAt?: string;
  lastSkipReason?: string;
  lastError?: string;
}

export interface SchedulerCuratorRuntime {
  readonly store: MemoryCuratorGovernanceStore;
  runtimeConfig: RustyCrewRuntimeConfig;
  lastLifecycleRunAt?: string;
  lastLifecycleReport?: CuratorLifecycleReport;
}

export interface SchedulerBackgroundContext {
  bridge: NativeBridgeModule;
  get runtimeConfig(): RustyCrewRuntimeConfig;
  schedulerHeartbeat: SchedulerHeartbeatState;
  curator: SchedulerCuratorRuntime;
  now(): string;
  isStopping(): boolean;
  curatorSkillsDir(runtimeConfig: RustyCrewRuntimeConfig): string;
  scheduledHostExecutorContext(): Parameters<
    typeof runScheduledHostExecutors
  >[0];
  recordEvent(event: SchedulerServiceEvent): void;
  reconcileDeferredRuntimeActivitySettlements(): Promise<number>;
}

export async function runSchedulerHeartbeat(
  context: SchedulerBackgroundContext,
): Promise<void> {
  if (context.isStopping()) return;
  if (context.schedulerHeartbeat.running) {
    context.schedulerHeartbeat.lastSkippedAt = context.now();
    context.schedulerHeartbeat.lastSkipReason =
      "previous scheduler heartbeat is still running";
    context.recordEvent({
      source: "service-host",
      eventType: "scheduler_heartbeat_skipped",
      severity: "warning",
      summary:
        "Scheduler heartbeat skipped because the previous tick is still running.",
    });
    return;
  }
  const startedAt = context.now();
  const startedMonotonic = Date.now();
  context.schedulerHeartbeat.running = true;
  context.schedulerHeartbeat.lastStartedAt = startedAt;
  context.schedulerHeartbeat.lastSkipReason = undefined;
  try {
    const runtimeActivitySettlements =
      await context.reconcileDeferredRuntimeActivitySettlements();
    const tick = await context.bridge.runSchedulerTick();
    const hostRuns = await runScheduledHostExecutors({
      ...context.scheduledHostExecutorContext(),
    });
    const scheduledJobs = await registerConfiguredScheduledJobs({
      bridge: context.bridge,
      runtimeConfig: context.runtimeConfig,
      now: context.now,
    });
    const curatorLifecycle =
      await runServiceCuratorLifecycleTransitions(context);
    const maintenanceAt = context.now();
    const retention = context.runtimeConfig.storage?.externalEventRetention;
    const maintenance = await context.bridge.runMaintenance({
      expireQueuedMessagesAt: maintenanceAt,
      ...(retention?.enabled === true &&
      retention.ageDays !== undefined &&
      retention.terminalTurnBatchSize !== undefined
        ? {
            compactTerminalExternalRuntimeEventsBefore: new Date(
              Date.parse(maintenanceAt) - retention.ageDays * 86_400_000,
            ).toISOString(),
            externalRuntimeEventRetentionAt: maintenanceAt,
            externalRuntimeEventTerminalTurnBatchSize:
              retention.terminalTurnBatchSize,
          }
        : {}),
    });
    const summary = `Scheduler heartbeat: ${tick.wakesRequested} wakes requested, ${tick.runsCompleted} wake runs completed, ${hostRuns.completed} host runs completed, ${scheduledJobs.registered} configured jobs reconciled, ${runtimeActivitySettlements} deferred runtime activity settlements reconciled, ${curatorLifecycle.transitions.length} curator lifecycle transitions, ${maintenance.expiredQueueMessages} queued messages expired, ${maintenance.externalRuntimeEventRetention.eventsDeleted} external runtime events compacted.`;
    context.schedulerHeartbeat.lastCompletedAt = context.now();
    context.schedulerHeartbeat.lastDurationMs = Date.now() - startedMonotonic;
    context.schedulerHeartbeat.lastSummary = summary;
    context.schedulerHeartbeat.lastError = undefined;
    if (
      tick.wakesRequested > 0 ||
      tick.runsCompleted > 0 ||
      tick.runsFailed > 0 ||
      hostRuns.claimed > 0 ||
      scheduledJobs.registered > 0 ||
      runtimeActivitySettlements > 0 ||
      curatorLifecycle.transitions.length > 0 ||
      maintenance.expiredQueueMessages > 0 ||
      maintenance.externalRuntimeEventRetention.eventsDeleted > 0
    ) {
      context.recordEvent({
        source: "service-host",
        eventType: "scheduler_heartbeat",
        summary,
      });
    }
  } catch (error) {
    recordSchedulerHeartbeatFailure(context, error);
    throw error;
  } finally {
    context.schedulerHeartbeat.running = false;
  }
}

export function recordSchedulerHeartbeatFailure(
  context: SchedulerBackgroundContext,
  error: unknown,
): void {
  const summary = errorMessage(error, "scheduler heartbeat failed");
  context.schedulerHeartbeat.lastCompletedAt = context.now();
  context.schedulerHeartbeat.lastError = summary;
  context.schedulerHeartbeat.lastSummary = summary;
  context.recordEvent({
    source: "service-host",
    eventType: "scheduler_heartbeat_failed",
    severity: "error",
    summary,
  });
}

export async function runServiceCuratorLifecycleTransitions(
  context: SchedulerBackgroundContext,
): Promise<CuratorLifecycleReport> {
  const report = await runCuratorLifecycleTransitions({
    store: context.curator.store,
    skillsDir: context.curatorSkillsDir(context.curator.runtimeConfig),
    now: context.now(),
    planner: (request) =>
      context.bridge.planCuratorLifecycleTransition(
        request,
      ) as ReturnType<CuratorLifecyclePlanner>,
  });
  context.curator.lastLifecycleRunAt = report.checkedAt;
  context.curator.lastLifecycleReport = report;
  return report;
}

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback;
}
