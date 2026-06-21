import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type {
  AgentId,
  CoreEvent,
  ProfileId,
  SessionId,
  TaskId,
} from "@rusty-crew/contracts";
import { loadNativeBridge } from "@rusty-crew/native-bridge";
import {
  AgentActivityObservationProducer,
  buildBackgroundServiceDiagnosticsProjection,
  publishBackgroundGovernanceObservation,
  runDelegatedResourceCleanup,
} from "./index.js";
import { createMemoryAgentActivityObservationSink } from "./test-support.js";

const fixedNow = "2026-06-21T00:00:00Z";
const engineDataDir = mkdtempSync(join(tmpdir(), "rusty-crew-scheduler-e2e-"));
const native = await loadNativeBridge();
let engine = await native.initializeEngine({
  engineDataDir,
  clock: { fixed: fixedNow },
  defaultTurnBudget: 3,
  defaultIdleTimeoutMs: 1_000,
});

try {
  const scheduledSessionId = "scheduler-target-session" as SessionId;
  await native.createSession({
    sessionId: scheduledSessionId,
    agentId: "prime" as AgentId,
    profileId: "prime-profile" as ProfileId,
    kind: "full",
  });

  const wakeSubscription = await native.subscribeEvents({
    eventKinds: ["brain_wake_requested"],
    sessionId: scheduledSessionId,
  });
  const job = await native.registerScheduledWakeJob({
    jobId: "wake-prime",
    targetSessionId: scheduledSessionId,
    intervalMs: 60_000,
    firstDueAt: "2026-06-20T00:00:00Z",
  });
  assert.equal(job.status, "active");
  assert.equal(job.nextDueAt, "2026-06-20T00:00:00Z");

  const tick = await native.runSchedulerTick();
  assert.equal(tick.staleRunsExpired, 0);
  assert.equal(tick.dueRunsClaimed, 1);
  assert.equal(tick.wakesRequested, 1);
  assert.equal(tick.runsCompleted, 1);

  const wakeEvents = await native.drainSubscriptionEvents(wakeSubscription, 4);
  const scheduledWake = wakeEvents.find(
    (event): event is Extract<CoreEvent, { type: "brain_wake_requested" }> =>
      event.type === "brain_wake_requested",
  );
  assert.equal(scheduledWake?.sessionId, scheduledSessionId);
  assert.equal(await native.diagnosticCountRows("scheduled_jobs"), 1);
  assert.equal(await native.diagnosticCountRows("scheduled_job_runs"), 1);

  const duplicateTick = await native.runSchedulerTick();
  assert.equal(duplicateTick.dueRunsClaimed, 0);
  assert.equal(await native.diagnosticCountRows("scheduled_job_runs"), 1);
  await native.unsubscribeEvents(wakeSubscription);

  const plannerSessionId = "cleanup-planner-session" as SessionId;
  const coderProfileId = "cleanup-coder-profile" as ProfileId;
  await native.createSession({
    sessionId: plannerSessionId,
    agentId: "cleanup-planner" as AgentId,
    profileId: "cleanup-planner-profile" as ProfileId,
    kind: "full",
  });
  await native.diagnosticSubmitBrainActionsJson(
    "cleanup-parent-wake",
    plannerSessionId,
    [
      {
        type: "request_delegation",
        profileId: coderProfileId,
        taskId: "2983" as TaskId,
        prompt: "Create a terminal delegated run for cleanup proof.",
        resourceLimits: {
          workdir: "/home/dev/rusty-crew",
          maxDurationMs: 30_000,
          maxDelegationDepth: 0,
        },
        timeoutMs: 30_000,
        priority: "normal",
        correlationId: "scheduler-cleanup-e2e",
      },
    ],
  );
  const delegatedSessionId =
    "cleanup-planner-session:delegated:cleanup-parent-wake:0" as SessionId;
  await native.diagnosticSubmitBrainActionsJson(
    "cleanup-child-wake",
    delegatedSessionId,
    [
      {
        type: "deliver_completion",
        packet: {
          sessionId: delegatedSessionId,
          status: "completed",
          summary: "cleanup target completed",
        },
      },
    ],
  );

  const observationSink = createMemoryAgentActivityObservationSink();
  const producer = new AgentActivityObservationProducer({
    sink: observationSink,
    required: true,
  });
  const cleanup = await runDelegatedResourceCleanup({
    runtime: native,
    observation: {
      producer,
      identity: {
        profile: "operator",
        instance_id: "scheduler-cleanup-e2e",
        session_key: "scheduler-cleanup",
      },
      workRef: { run_id: "cleanup:2983", task_id: "2983" },
    },
  });
  assert.deepEqual(cleanup.runtime.terminalArchived, [delegatedSessionId]);
  assert.equal(cleanup.runtime.orphanedArchived.length, 0);
  assert.equal(cleanup.runtime.expiredArchived.length, 0);
  assert.equal(cleanup.observation.started, "published");
  assert.equal(cleanup.observation.terminal, "published");
  const cleanedStatus = await native.delegatedSessionStatus(delegatedSessionId);
  assert.equal(cleanedStatus.session.status, "archived");
  assert.equal(cleanedStatus.runStatus, "completed");
  const adapterReleased = cleanup.adapters.reduce(
    (sum, adapter) => sum + adapter.released,
    0,
  );
  const adapterDegraded = cleanup.adapters.reduce(
    (sum, adapter) => sum + adapter.degraded,
    0,
  );

  const schedulerObservation = await publishBackgroundGovernanceObservation({
    producer,
    identity: {
      profile: "operator",
      instance_id: "scheduler-cleanup-e2e",
      session_key: "scheduler-cleanup",
    },
    loopKind: "scheduler",
    phase: "completed",
    summary: "Scheduler tick completed one scheduled wake.",
    workRef: { run_id: "scheduler:2983", task_id: "2983" },
  });
  assert.equal(schedulerObservation?.status, "published");

  const diagnostics = buildBackgroundServiceDiagnosticsProjection({
    now: fixedNow,
    scheduler: {
      jobCount: 1,
      activeJobs: 1,
      pausedJobs: 0,
      staleRuns: tick.staleRunsExpired,
      lastRunAt: fixedNow,
    },
    curator: {
      status: "available",
      candidateCount: 0,
      mutationCount: 0,
    },
    cleanup: {
      lastRunAt: cleanup.runtime.cleanedAt,
      terminalArchived: cleanup.runtime.terminalArchived.length,
      orphanedArchived: cleanup.runtime.orphanedArchived.length,
      expiredArchived: cleanup.runtime.expiredArchived.length,
      adapterReleased,
      adapterDegraded,
    },
  });
  assert.equal(diagnostics.health, "ok");
  assert.equal(diagnostics.summary.activeJobs, 1);
  assert.equal(diagnostics.summary.cleanupArchived, 1);
  assert.equal(observationSink.events.length, 3);

  await native.shutdownEngine({ engine, drainTimeoutMs: 1_000 });
  engine = await native.initializeEngine({
    engineDataDir,
    clock: { fixed: fixedNow },
    defaultTurnBudget: 3,
    defaultIdleTimeoutMs: 1_000,
  });
  const postRestartTick = await native.runSchedulerTick();
  assert.equal(postRestartTick.dueRunsClaimed, 0);
  assert.equal(postRestartTick.wakesRequested, 0);
  assert.equal(await native.diagnosticCountRows("scheduled_job_runs"), 1);
  const postRestartCleanup = await native.cleanupDelegatedResources();
  assert.deepEqual(postRestartCleanup.terminalArchived, []);
  assert.deepEqual(postRestartCleanup.orphanedArchived, []);
  assert.deepEqual(postRestartCleanup.expiredArchived, []);

  console.log(
    JSON.stringify(
      {
        scheduledJob: job.jobId,
        tick,
        cleanupArchived: cleanup.runtime.terminalArchived,
        diagnostics: diagnostics.summary,
        observations: observationSink.events.map((event) => event.event_type),
        postRestartTick,
      },
      null,
      2,
    ),
  );
} finally {
  await native.shutdownEngine({ engine, drainTimeoutMs: 1_000 });
  rmSync(engineDataDir, { force: true, recursive: true });
}
