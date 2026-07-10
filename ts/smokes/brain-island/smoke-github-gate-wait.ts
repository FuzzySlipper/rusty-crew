import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { ReviewGitHubGateEventConsumer } from "@rusty-crew/adapter-den";
import type {
  AgentId,
  CoreEvent,
  ProfileId,
  ProjectId,
  RunId,
  SessionId,
  TaskId,
} from "@rusty-crew/contracts";
import { loadNativeBridge } from "@rusty-crew/native-bridge";

const reviewUrl = new URL(
  process.env.RUSTY_CREW_REVIEW_URL ?? "http://127.0.0.1:18096",
);
const sessionId = "github-gate-live-smoke" as SessionId;
const gateId = 903;
const mode = process.argv[2];

if (mode === "seed") {
  await seedWait(process.argv[3]!);
  // Deliberately omit graceful engine shutdown to model service-process loss.
  process.exit(0);
}

const root = mkdtempSync(join(tmpdir(), "rusty-crew-github-gate-wait-"));
try {
  const seed = spawnSync(
    process.execPath,
    ["--import", "tsx", fileURLToPath(import.meta.url), "seed", root],
    { encoding: "utf8", env: process.env },
  );
  assert.equal(seed.status, 0, seed.stderr || seed.stdout);

  const native = await loadNativeBridge();
  const engine = await native.initializeEngine({
    engineDataDir: root,
    clock: { fixed: "2026-07-09T18:31:00-07:00" },
    defaultTurnBudget: 4,
    defaultIdleTimeoutMs: 1_000,
  });
  const wakeSubscription = await native.subscribeEvents({
    eventKinds: ["brain_wake_requested"],
    sessionId,
  });
  const consumer = new ReviewGitHubGateEventConsumer({
    baseUrl: reviewUrl,
    projectId: "den-services",
    bridge: native,
    waitMs: 0,
  });
  assert.equal(await consumer.hydrate(), 1);
  assert.equal(consumer.status().cursor, 6);
  assert.equal(
    (await native.gitHubGateWait(sessionId))?.phase,
    "wake_scheduled",
  );
  const events = await native.drainSubscriptionEvents(wakeSubscription, 8);
  const wake = events.find(
    (event): event is Extract<CoreEvent, { type: "brain_wake_requested" }> =>
      event.type === "brain_wake_requested",
  );
  assert.equal(wake?.sessionId, sessionId);
  await native.unsubscribeEvents(wakeSubscription);
  await native.shutdownEngine({ engine, drainTimeoutMs: 1_000 });

  console.log(
    JSON.stringify(
      {
        reviewUrl: reviewUrl.toString(),
        gateId,
        terminalEventId: 6,
        wakeSessionId: wake?.sessionId,
        recoveredWakeCount: 1,
        durableCursor: consumer.status().cursor,
      },
      null,
      2,
    ),
  );
} finally {
  rmSync(root, { force: true, recursive: true });
}

async function seedWait(root: string): Promise<void> {
  const native = await loadNativeBridge();
  await native.initializeEngine({
    engineDataDir: root,
    clock: { fixed: "2026-07-09T18:30:00-07:00" },
    defaultTurnBudget: 4,
    defaultIdleTimeoutMs: 1_000,
  });
  await native.createSession({
    sessionId,
    agentId: "github-gate-smoke" as AgentId,
    profileId: "github-gate-smoke" as ProfileId,
    kind: "full",
  });
  await native.suspendForGitHubGate({
    sessionId,
    runId: "github-gate-live-run" as RunId,
    projectId: "den-services" as ProjectId,
    taskId: "5499" as TaskId,
    gateId,
    commitSha: "b2a2941122f61a015f0bcdc7e539776a9d13fb3d",
    now: "2026-07-09T18:30:00-07:00",
  });
  const consumer = new ReviewGitHubGateEventConsumer({
    baseUrl: reviewUrl,
    projectId: "den-services",
    bridge: native,
    waitMs: 0,
  });
  assert.equal(await consumer.hydrate(), 0);
  const receipt = (await consumer.pollOnce()).find(
    (candidate) => candidate.eventId === 6,
  );
  assert.equal(receipt?.wakeScheduled, true);
  assert.equal(receipt?.ignoredReason, undefined);
  assert.equal(consumer.status().cursor, 6);
}
