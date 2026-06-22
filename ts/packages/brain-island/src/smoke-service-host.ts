import assert from "node:assert/strict";
import { existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { mkdtempSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type {
  AgentId,
  McpBindingRecord,
  ProfileId,
  SessionId,
} from "@rusty-crew/contracts";
import { createDebugApiClient } from "./debug-api-client.js";
import { startRustyCrewServiceHost } from "./service-host.js";

const root = mkdtempSync(join(tmpdir(), "rusty-crew-service-host-"));
const port = await openPort();
const token = "local-field-test-token";
writeRuntimeConfig(root);
let host = await startHost(root, port, token);

try {
  assert.equal(existsSync(join(root, "data", "engine")), true);
  assert.equal(existsSync(join(root, "run", "service.lock")), true);

  const adminPanel = await getText("/admin");
  assert.equal(adminPanel.status, 200);
  assert.match(adminPanel.body, /Rusty Crew Admin/);
  assert.match(adminPanel.body, /diagnostics/);

  const health = await get("/v1/admin/healthz");
  assert.equal(health.status, 200);
  assert.equal(health.body.ok, true);

  const unauthenticatedReady = await get("/v1/admin/readyz");
  assert.equal(unauthenticatedReady.status, 401);
  assert.equal(unauthenticatedReady.body.ok, false);

  const ready = await get("/v1/admin/readyz", token);
  assert.equal(ready.status, 200);
  assert.equal(ready.body.ok, true);

  const diagnostics = await get("/v1/admin/diagnostics", token);
  assert.equal(diagnostics.status, 200);
  assert.equal(diagnostics.body.ok, true);
  assert.equal(
    diagnostics.body.data.overview.persistence.tableCounts.sessions,
    1,
  );
  assert.equal(diagnostics.body.data.overview.summary.sessions, 1);
  assert.equal(diagnostics.body.data.overview.summary.idleSessions, 1);
  assert.equal(
    diagnostics.body.data.overview.runtime.sessions[0]?.toolCount,
    1,
  );
  assert.equal(
    typeof diagnostics.body.data.overview.persistence.databaseBytes,
    "number",
  );
  const configuredSessions = await host.bridge.listSessions();
  assert.equal(
    configuredSessions.find((session) => session.sessionId === "field-session")
      ?.resourceLimits.maxDurationMs,
    45_000,
  );

  const channels = await get("/v1/admin/diagnostics/channels", token);
  assert.equal(channels.status, 200);
  assert.equal(channels.body.data.total, 1);
  assert.equal(channels.body.data.items[0]?.bindingId, "field-channel");
  assert.equal(channels.body.data.items[0]?.status, "missing");

  const mcp = await get("/v1/admin/diagnostics/mcp", token);
  assert.equal(mcp.status, 200);
  assert.equal(mcp.body.data.total, 1);
  assert.equal(mcp.body.data.items[0]?.bindingId, "field-mcp");
  assert.equal(mcp.body.data.items[0]?.status, "active");

  const recentEvents = await get("/v1/admin/events/recent", token);
  assert.match(
    recentEvents.body.data.items[0]?.summary,
    /1 brains registered.*1 sessions created/,
  );

  const maintenance = await post("/v1/admin/control/maintenance", token, {
    reason: "smoke",
    runWalCheckpoint: true,
    runOptimize: true,
  });
  assert.equal(maintenance.status, 200);
  assert.equal(maintenance.body.ok, true);
  assert.equal(
    typeof maintenance.body.data.outcome.result.sizeBefore.databaseBytes,
    "number",
  );
  assert.equal(maintenance.body.data.outcome.result.walCheckpointRan, true);

  const created = await post("/v1/admin/control/sessions", token, {
    sessionId: "field-session-two",
    agentId: "field-agent",
    profileId: "field-profile",
    kind: "full",
    reason: "smoke",
  });
  assert.equal(created.status, 200);
  assert.equal(created.body.ok, true);
  assert.equal(
    created.body.data.outcome.affectedIds.sessionId,
    "field-session-two",
  );

  const afterCreate = await get("/v1/admin/diagnostics", token);
  assert.equal(afterCreate.body.data.overview.summary.sessions, 2);
  assert.equal(
    afterCreate.body.data.overview.persistence.tableCounts.sessions,
    2,
  );

  const schedulerTick = await post("/v1/admin/control/scheduler/tick", token, {
    reason: "smoke",
  });
  assert.equal(schedulerTick.status, 200);
  assert.equal(schedulerTick.body.ok, true);

  const curatorStatus = await post("/v1/admin/control/curator/status", token, {
    reason: "smoke",
  });
  assert.equal(curatorStatus.status, 200);
  assert.equal(curatorStatus.body.ok, true);
  assert.equal(curatorStatus.body.data.outcome.result.status, "available");

  const curatorRun = await post("/v1/admin/control/curator/run", token, {
    scopeType: "profile",
    scopeId: "field-profile",
    dryRun: true,
    reason: "service host smoke",
  });
  assert.equal(curatorRun.status, 200);
  assert.equal(curatorRun.body.ok, true);
  assert.match(
    curatorRun.body.data.outcome.result.summary,
    /scan produced [1-9]/,
  );

  const client = createDebugApiClient({
    baseUrl: `http://127.0.0.1:${port}`,
    bearerToken: token,
  });
  const debugContext = await client.directDebugContext({
    sessionId: "field-session",
    includePromptText: true,
  });
  assert.equal(debugContext.source, "direct_debug");
  assert.equal(debugContext.session.sessionId, "field-session");
  assert.equal(debugContext.selectedTools[0]?.name, "read_file");

  const beforeDirectTurn = await get("/v1/admin/diagnostics", token);
  const completionPacketsBeforeDirectTurn =
    beforeDirectTurn.body.data.overview.persistence.tableCounts
      .completion_packets;

  const directTurn = await client.requestDirectDebugTurn({
    sessionId: "field-session",
    actorId: "local-operator",
    body: "Exercise direct debug over the service host.",
  });
  assert.equal(directTurn.status, "accepted");
  assert.match(directTurn.summary, /local service brain wake completed/);
  assert.match(directTurn.wakeId ?? "", /^service-field-session-/);

  const afterDirectTurn = await get("/v1/admin/diagnostics", token);
  assert.equal(
    afterDirectTurn.body.data.overview.persistence.tableCounts
      .completion_packets,
    completionPacketsBeforeDirectTurn + 1,
  );
  await sleep(350);
  const afterDirectTurnSettled = await get("/v1/admin/diagnostics", token);
  assert.equal(
    afterDirectTurnSettled.body.data.overview.persistence.tableCounts
      .completion_packets,
    completionPacketsBeforeDirectTurn + 1,
  );

  const completionPacketsBeforeScheduledWake =
    afterDirectTurnSettled.body.data.overview.persistence.tableCounts
      .completion_packets;
  await host.bridge.registerScheduledWakeJob({
    jobId: "field-session-smoke-heartbeat",
    targetSessionId: "field-session" as SessionId,
    intervalMs: 60_000,
    firstDueAt: new Date(Date.now() - 1_000).toISOString(),
  });
  await waitUntil(async () => {
    return (await host.bridge.diagnosticCountRows("scheduled_job_runs")) > 0;
  }, "scheduled job was claimed by the service heartbeat");
  const schedulerJobs = await get(
    "/v1/admin/scheduler/jobs?status=active",
    token,
  );
  assert.equal(schedulerJobs.status, 200);
  assert.equal(
    schedulerJobs.body.data.jobs.some(
      (job: { jobId?: string }) =>
        job.jobId === "field-session-smoke-heartbeat",
    ),
    true,
  );
  const schedulerRuns = await get(
    "/v1/admin/scheduler/runs?jobId=field-session-smoke-heartbeat&limit=5",
    token,
  );
  assert.equal(schedulerRuns.status, 200);
  assert.equal(schedulerRuns.body.data.runs[0]?.status, "completed");
  await waitUntil(
    async () => {
      const scheduledWakeDiagnostics = await get(
        "/v1/admin/diagnostics",
        token,
      );
      return (
        scheduledWakeDiagnostics.body.data.overview.persistence.tableCounts
          .completion_packets > completionPacketsBeforeScheduledWake
      );
    },
    "scheduled wake was dispatched by the service heartbeat",
    async () => {
      const diagnostics = await get("/v1/admin/diagnostics", token);
      return JSON.stringify({
        runs: await host.bridge.diagnosticCountRows("scheduled_job_runs"),
        completions:
          diagnostics.body.data.overview.persistence.tableCounts
            .completion_packets,
        recentEvents: diagnostics.body.data.recentEvents,
      });
    },
  );

  await host.bridge.createSession({
    sessionId: "field-session-expiry" as SessionId,
    agentId: "expiry-agent" as AgentId,
    profileId: "missing-expiry-profile" as ProfileId,
    kind: "full",
  });
  const expiring = await host.bridge.enqueueBodyFollowUpMessage({
    sessionId: "field-session-expiry" as SessionId,
    from: "local-operator" as AgentId,
    body: "This queued message should expire under the heartbeat.",
  });
  assert.equal(expiring.state, "pending");
  await waitUntil(
    async () => {
      const expiryDiagnostics = await get("/v1/admin/diagnostics", token);
      return (
        expiryDiagnostics.body.data.overview.runtime.counters.queueExpirations >
        0
      );
    },
    "queued message expired under the service heartbeat",
    async () => {
      const diagnostics = await get("/v1/admin/diagnostics", token);
      return JSON.stringify({
        summary: diagnostics.body.data.overview.summary,
        counters: diagnostics.body.data.overview.runtime.counters,
      });
    },
    7_000,
  );

  await host.stop();
  host = await startHost(root, port, token);

  const restartedDiagnostics = await get("/v1/admin/diagnostics", token);
  assert.equal(restartedDiagnostics.body.data.overview.summary.sessions, 3);
  assert.equal(restartedDiagnostics.body.data.overview.summary.idleSessions, 1);
  assert.equal(
    restartedDiagnostics.body.data.overview.summary.archivedSessions,
    2,
  );
  assert.equal(
    restartedDiagnostics.body.data.overview.persistence.tableCounts.sessions,
    3,
  );

  const restartedContext = await client.directDebugContext({
    sessionId: "field-session",
  });
  assert.equal(restartedContext.session.sessionId, "field-session");
  assert.equal(restartedContext.session.status, "idle");
  const beforeRestartDirectTurn = await get("/v1/admin/diagnostics", token);
  const restartDirectTurn = await client.requestDirectDebugTurn({
    sessionId: "field-session",
    actorId: "local-operator",
    body: "Exercise direct debug after service restart.",
  });
  assert.equal(restartDirectTurn.status, "accepted");
  assert.match(restartDirectTurn.summary, /local service brain wake completed/);
  const afterRestartDirectTurn = await get("/v1/admin/diagnostics", token);
  assert.equal(
    afterRestartDirectTurn.body.data.overview.persistence.tableCounts
      .completion_packets,
    beforeRestartDirectTurn.body.data.overview.persistence.tableCounts
      .completion_packets + 1,
  );

  writeRuntimeConfig(root, { includeExtraMcpBinding: true });
  const reloadConfig = await post("/v1/admin/control/config/reload", token, {
    reason: "smoke config reload",
  });
  assert.equal(reloadConfig.status, 200);
  assert.equal(reloadConfig.body.ok, true);
  assert.equal(reloadConfig.body.data.outcome.result.sessionsAlreadyPresent, 1);
  assert.equal(reloadConfig.body.data.outcome.result.sessionsMissing, 0);
  const mcpAfterReload = await get("/v1/admin/diagnostics/mcp", token);
  assert.equal(mcpAfterReload.body.data.total, 2);
  assert.deepEqual(
    mcpAfterReload.body.data.items
      .map((item: { bindingId: string }) => item.bindingId)
      .sort(),
    ["field-mcp", "field-mcp-extra"],
  );

  await host.stop();

  const noAuthRoot = mkdtempSync(join(tmpdir(), "rusty-crew-service-noauth-"));
  const noAuthPort = await openPort();
  writeRuntimeConfig(noAuthRoot);
  const noAuthHost = await startNoAuthHost(noAuthRoot, noAuthPort);
  try {
    const noAuthPanel = await getText("/admin", noAuthPort);
    assert.equal(noAuthPanel.status, 200);
    assert.match(noAuthPanel.body, /tokenForm" class="token-row" hidden/);

    const noAuthReady = await get("/v1/admin/readyz", undefined, noAuthPort);
    assert.equal(noAuthReady.status, 200);
    assert.equal(noAuthReady.body.ok, true);

    const noAuthControl = await post(
      "/v1/admin/control/scheduler/tick",
      undefined,
      { reason: "smoke no-auth mode" },
      noAuthPort,
    );
    assert.equal(noAuthControl.status, 200);
    assert.equal(noAuthControl.body.ok, true);
  } finally {
    await noAuthHost.stop();
    rmSync(noAuthRoot, { recursive: true, force: true });
  }
} finally {
  await host.stop();
  assert.equal(existsSync(join(root, "run", "service.lock")), false);
  rmSync(root, { recursive: true, force: true });
}

console.log("service host smoke passed");

async function get(path: string, bearer?: string, requestPort = port) {
  const response = await fetch(`http://127.0.0.1:${requestPort}${path}`, {
    headers: bearer ? { authorization: `Bearer ${bearer}` } : undefined,
  });
  return {
    status: response.status,
    body: (await response.json()) as any,
  };
}

async function getText(path: string, requestPort = port) {
  const response = await fetch(`http://127.0.0.1:${requestPort}${path}`);
  return {
    status: response.status,
    contentType: response.headers.get("content-type"),
    body: await response.text(),
  };
}

async function post(
  path: string,
  bearer: string | undefined,
  body: unknown,
  requestPort = port,
) {
  const response = await fetch(`http://127.0.0.1:${requestPort}${path}`, {
    method: "POST",
    headers: {
      ...(bearer ? { authorization: `Bearer ${bearer}` } : {}),
      "content-type": "application/json",
    },
    body: JSON.stringify(body),
  });
  return {
    status: response.status,
    body: (await response.json()) as any,
  };
}

function openPort(): Promise<number> {
  return new Promise((resolveOpenPort, rejectOpenPort) => {
    const server = createServer();
    server.once("error", rejectOpenPort);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      if (!address || typeof address === "string") {
        server.close();
        rejectOpenPort(new Error("failed to discover open TCP port"));
        return;
      }
      const port = address.port;
      server.close((error) => {
        if (error) rejectOpenPort(error);
        else resolveOpenPort(port);
      });
    });
  });
}

async function startHost(root: string, port: number, token: string) {
  return startRustyCrewServiceHost({
    env: {
      RUSTY_CREW_DATA_DIR: root,
      RUSTY_CREW_ADMIN_HOST: "127.0.0.1",
      RUSTY_CREW_ADMIN_ALLOW_LAN: "false",
      RUSTY_CREW_ADMIN_PORT: String(port),
      RUSTY_CREW_ADMIN_TOKEN: token,
      RUSTY_CREW_SCHEDULER_TICK_INTERVAL_MS: "100",
      RUSTY_CREW_WAKE_DISPATCH_INTERVAL_MS: "50",
    },
  });
}

async function startNoAuthHost(root: string, port: number) {
  return startRustyCrewServiceHost({
    env: {
      RUSTY_CREW_DATA_DIR: root,
      RUSTY_CREW_ADMIN_HOST: "127.0.0.1",
      RUSTY_CREW_ADMIN_ALLOW_LAN: "false",
      RUSTY_CREW_ADMIN_PORT: String(port),
      RUSTY_CREW_ADMIN_AUTH_MODE: "none",
      RUSTY_CREW_SCHEDULER_TICK_INTERVAL_MS: "100",
      RUSTY_CREW_WAKE_DISPATCH_INTERVAL_MS: "50",
    },
  });
}

async function sleep(ms: number): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, ms));
}

async function waitUntil(
  predicate: () => Promise<boolean>,
  description: string,
  details?: () => Promise<string>,
  timeoutMs = 3_000,
): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (await predicate()) return;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  const detailText = details ? `: ${await details()}` : "";
  assert.fail(`timed out waiting for ${description}${detailText}`);
}

function writeRuntimeConfig(
  root: string,
  options: { includeExtraMcpBinding?: boolean } = {},
): void {
  const configDir = join(root, "config");
  const profilesDir = join(configDir, "profiles");
  const skillsDir = join(configDir, "skills");
  mkdirSync(profilesDir, { recursive: true });
  mkdirSync(skillsDir, { recursive: true });
  const runtimeConfig = {
    profilesDir,
    skillsDir,
    brains: [{ profileId: "field-profile" }],
    sessions: [
      {
        sessionId: "field-session",
        agentId: "field-agent",
        profileId: "field-profile",
        kind: "full",
      },
    ],
    channelBindings: [
      {
        bindingId: "field-channel",
        adapterId: "den-channel-main",
        provider: "den_channels",
        agentId: "field-agent",
        sessionId: "field-session",
        profileId: "field-profile",
        externalChannelId: "field-room",
        externalThreadId: "field-thread",
        externalUserId: "field-agent-external",
        status: "active",
      },
    ],
    mcpBindings: [] as McpBindingRecord[],
  };
  if (options.includeExtraMcpBinding) {
    runtimeConfig.mcpBindings.push({
      bindingId: "field-mcp-extra",
      adapterId: "mcp-ts-extra" as never,
      agentId: "field-agent" as AgentId,
      sessionId: "field-session" as SessionId,
      profileId: "field-profile" as ProfileId,
      serverNames: ["field-extra"],
      endpointRef: "config://mcp/field-extra",
      transport: "stdio",
      toolProfileKey: "field-profile-mcp-extra",
      status: "active",
      diagnostics: {},
    });
  }
  writeFileSync(
    join(configDir, "service.json"),
    JSON.stringify(runtimeConfig, null, 2),
  );
  writeFileSync(
    join(profilesDir, "field-profile.json"),
    JSON.stringify(
      {
        profileId: "field-profile",
        modelConfig: {
          provider: "local",
          modelName: "deterministic",
        },
        runtime: {
          defaultResourceLimits: {
            maxDurationMs: 45_000,
          },
        },
        mcpConfig: {
          bindingId: "field-mcp",
          serverNames: ["field"],
          endpointRef: "config://mcp/field",
          toolProfile: "field-profile-mcp",
        },
        toolPolicy: {
          requestedTools: ["read_file"],
        },
        skills: "all",
      },
      null,
      2,
    ),
  );
  writeFileSync(
    join(skillsDir, "field-review.md"),
    `---
title: Field Review
summary: Review field service behavior.
tags:
  - smoke
---

Use this skill for stable field review behavior.
`,
  );
  writeFileSync(
    join(skillsDir, "field-review-copy.md"),
    `---
title: Field Review
tags:
  - smoke
---

TODO: move temporary project progress out of skills.
`,
  );
}
