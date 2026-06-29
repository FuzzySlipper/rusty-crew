import assert from "node:assert/strict";
import {
  existsSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { mkdtempSync } from "node:fs";
import { connect, createServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type {
  AgentId,
  EngineConfig,
  McpBindingRecord,
  ProfileId,
  SessionId,
} from "@rusty-crew/contracts";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";
import { createDebugApiClient } from "./debug-api-client.js";
import { startRustyCrewServiceHost } from "./service-host.js";

const blockedPostgresRoot = mkdtempSync(
  join(tmpdir(), "rusty-crew-service-host-postgres-blocked-"),
);
try {
  let initializeCalled = false;
  const blockedPostgresPort = await openPort();
  await assert.rejects(
    () =>
      startRustyCrewServiceHost({
        env: {
          RUSTY_CREW_DATA_DIR: blockedPostgresRoot,
          RUSTY_CREW_ADMIN_HOST: "127.0.0.1",
          RUSTY_CREW_ADMIN_ALLOW_LAN: "false",
          RUSTY_CREW_ADMIN_PORT: String(blockedPostgresPort),
          RUSTY_CREW_ADMIN_AUTH_MODE: "none",
          RUSTY_CREW_STORAGE_BACKEND: "postgres",
          RUSTY_CREW_POSTGRES_DATABASE_URL_ENV: "RUSTY_CREW_DATABASE_URL",
          RUSTY_CREW_POSTGRES_BOOT_MODE: "active",
        },
        bridge: {
          manifestVersion: 1,
          operationNames: [],
          initializeEngine: async () => {
            initializeCalled = true;
            throw new Error("initializeEngine should not be called");
          },
        } as unknown as NativeBridgeModule,
      }),
    /storage\.backend=postgres requires RUSTY_CREW_DATABASE_URL to be set/,
  );
  assert.equal(initializeCalled, false);

  initializeCalled = false;
  let capturedStorageBackend: string | undefined;
  await assert.rejects(
    () =>
      startRustyCrewServiceHost({
        env: {
          RUSTY_CREW_DATA_DIR: blockedPostgresRoot,
          RUSTY_CREW_ADMIN_HOST: "127.0.0.1",
          RUSTY_CREW_ADMIN_ALLOW_LAN: "false",
          RUSTY_CREW_ADMIN_PORT: String(blockedPostgresPort),
          RUSTY_CREW_ADMIN_AUTH_MODE: "none",
          RUSTY_CREW_STORAGE_BACKEND: "postgres",
          RUSTY_CREW_DATABASE_URL:
            "postgres://rusty_crew:local@127.0.0.1:5432/rusty_crew",
          RUSTY_CREW_POSTGRES_DATABASE_URL_ENV: "RUSTY_CREW_DATABASE_URL",
          RUSTY_CREW_POSTGRES_BOOT_MODE: "active",
        },
        bridge: {
          manifestVersion: 1,
          operationNames: [],
          initializeEngine: async (config: EngineConfig) => {
            initializeCalled = true;
            capturedStorageBackend = config.storage?.backend;
            throw new Error("postgres init sentinel");
          },
        } as unknown as NativeBridgeModule,
      }),
    /postgres init sentinel/,
  );
  assert.equal(initializeCalled, true);
  assert.equal(capturedStorageBackend, "postgres");
} finally {
  rmSync(blockedPostgresRoot, { recursive: true, force: true });
}

const root = mkdtempSync(join(tmpdir(), "rusty-crew-service-host-"));
const port = await openPort();
const token = "local-field-test-token";
writeRuntimeConfig(root);
writeStaticSite(root);
let host = await startHost(root, port, token);

try {
  assert.equal(existsSync(join(root, "data", "engine")), true);
  assert.equal(existsSync(join(root, "run", "service.lock")), true);

  const staticRoot = await getText("/");
  assert.equal(staticRoot.status, 200);
  assert.match(staticRoot.contentType ?? "", /text\/html/);
  assert.equal(staticRoot.cacheControl, "no-cache");
  assert.match(staticRoot.body, /Rusty View Smoke/);

  const staticAsset = await getText("/main-1234567890abcdef.js");
  assert.equal(staticAsset.status, 200);
  assert.match(staticAsset.contentType ?? "", /application\/javascript/);
  assert.equal(staticAsset.cacheControl, "public, max-age=31536000, immutable");
  assert.match(staticAsset.body, /rusty-view-smoke/);

  const staticStyle = await getText("/styles.css");
  assert.equal(staticStyle.status, 200);
  assert.match(staticStyle.contentType ?? "", /text\/css/);
  assert.equal(staticStyle.cacheControl, "no-cache");

  const spaFallback = await getText("/sessions/field-session");
  assert.equal(spaFallback.status, 200);
  assert.match(spaFallback.body, /Rusty View Smoke/);

  const traversal = await rawHttpGet("/%2e%2e/%2e%2e/etc/passwd");
  assert.match(traversal, /^HTTP\/1\.1 200/);
  assert.match(traversal, /Rusty View Smoke/);
  assert.doesNotMatch(traversal, /root:/);

  const dotfile = await getText("/.env");
  assert.equal(dotfile.status, 403);
  assert.match(dotfile.body, /forbidden segment/);

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

  const chatSessions = await get("/v1/chat/sessions", token);
  assert.equal(chatSessions.status, 200);
  assert.equal(chatSessions.body.ok, true);
  assert.equal(chatSessions.body.data.items.length, 1);

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
  const configValidation = await get("/v1/admin/diagnostics/config", token);
  assert.equal(configValidation.status, 200);
  assert.equal(configValidation.body.ok, true);
  assert.equal(configValidation.body.data.ok, true);
  assert.equal(configValidation.body.data.summary.errors, 0);
  assert.equal(
    configValidation.body.data.derived.scheduledJobs[0]?.id,
    "background-review-field-profile",
  );
  assert.equal(
    configValidation.body.data.derived.mcpBindings[0]?.bindingId,
    "field-mcp",
  );
  assert.equal(
    configValidation.body.data.derived.sessionDefaultsApplied[0]?.sessionId,
    "field-session",
  );
  const configValidationJson = JSON.stringify(configValidation.body.data);
  assert.equal(
    /soulMarkdown|memoryMarkdown|apiKeyEnv/.test(configValidationJson),
    false,
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
  assert.equal(channels.body.data.items[0]?.status, "degraded");
  assert.equal(
    channels.body.data.items[0]?.lastError,
    "Den Conversation channel is not resolved",
  );

  const mcp = await get("/v1/admin/diagnostics/mcp", token);
  assert.equal(mcp.status, 200);
  assert.equal(mcp.body.data.total, 1);
  assert.equal(mcp.body.data.items[0]?.bindingId, "field-mcp");
  assert.equal(mcp.body.data.items[0]?.status, "active");
  const mcpCatalog = await get("/v1/admin/mcp/servers", token);
  assert.equal(mcpCatalog.status, 200);
  assert.equal(mcpCatalog.body.ok, true);
  assert.deepEqual(
    mcpCatalog.body.data.servers.map(
      (server: {
        id: string;
        baseUrl: string;
        configuredBindingCount: number;
      }) => ({
        id: server.id,
        baseUrl: server.baseUrl,
        configuredBindingCount: server.configuredBindingCount,
      }),
    ),
    [
      {
        id: "field",
        baseUrl: "http://mcp.local/mcp",
        configuredBindingCount: 1,
      },
    ],
  );
  assert.deepEqual(mcpCatalog.body.data.toolProfiles, ["field-profile-mcp"]);
  assert.equal(mcpCatalog.body.data.bindings[0]?.endpointServerId, "field");
  assert.equal(mcpCatalog.body.data.bindings[0]?.resolvedServerId, "field");

  const toolsCatalog = await get("/v1/admin/tools/catalog", token);
  assert.equal(toolsCatalog.status, 200);
  assert.equal(toolsCatalog.body.ok, true);
  assert.equal(toolsCatalog.body.data.schemaVersion, 1);
  assert.equal(toolsCatalog.body.data.catalogId, "default-local-tools");
  for (const toolset of [
    "local_code_read",
    "web_research",
    "memory_profile",
    "skills_read",
    "planning_session",
  ]) {
    assert.ok(
      toolsCatalog.body.data.toolsets.some(
        (entry: { id: string }) => entry.id === toolset,
      ),
      `missing built-in tool catalog toolset ${toolset}`,
    );
  }
  assert.ok(
    toolsCatalog.body.data.tools.some(
      (entry: { name: string; description: string }) =>
        entry.name === "todo" && entry.description.length > 0,
    ),
    "missing built-in todo tool catalog metadata",
  );
  assert.equal(
    toolsCatalog.body.data.toolsets.some((entry: { id: string }) =>
      entry.id.startsWith("mcp:"),
    ),
    false,
  );

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

  await host.bridge.registerScheduledHostJob({
    jobId: "field-diagnostics-snapshot",
    jobKind: "runtime.diagnostics.snapshot",
    firstDueAt: new Date(Date.now() - 1_000).toISOString(),
    payload: { schema_version: 1 },
  });
  await waitUntil(async () => {
    const runs = await get(
      "/v1/admin/scheduler/runs?jobId=field-diagnostics-snapshot&limit=5",
      token,
    );
    return runs.body.data.runs.some(
      (run: { status?: string; output?: { outcome?: string } }) =>
        run.status === "completed" && run.output?.outcome === "completed",
    );
  }, "scheduled host diagnostics job completed by the service heartbeat");
  const manualHostRun = await post(
    "/v1/admin/control/scheduler/jobs/field-diagnostics-snapshot/run",
    token,
    { reason: "manual host diagnostics proof" },
  );
  assert.equal(manualHostRun.status, 200);
  assert.equal(manualHostRun.body.data.outcome.status, "completed");
  assert.match(
    manualHostRun.body.data.outcome.summary,
    /scheduled host job field-diagnostics-snapshot completed/,
  );
  const hostRuns = await get(
    "/v1/admin/scheduler/runs?jobId=field-diagnostics-snapshot&limit=5",
    token,
  );
  assert.equal(
    hostRuns.body.data.runs.filter(
      (run: { status?: string }) => run.status === "completed",
    ).length >= 2,
    true,
  );
  const manualReviewRun = await post(
    "/v1/admin/control/scheduler/jobs/background-review-field-profile/run",
    token,
    { reason: "manual background review proof" },
  );
  assert.equal(manualReviewRun.status, 200);
  assert.equal(manualReviewRun.body.data.outcome.status, "completed");
  assert.match(
    manualReviewRun.body.data.outcome.summary,
    /scheduled host job background-review-field-profile completed/,
  );
  const backgroundDiagnostics = await get(
    "/v1/admin/diagnostics/background",
    token,
  );
  assert.equal(backgroundDiagnostics.status, 200);
  assert.equal(backgroundDiagnostics.body.data.backgroundReview.enabled, true);
  assert.equal(
    backgroundDiagnostics.body.data.backgroundReview.recentFindings > 0,
    true,
  );
  assert.equal(
    typeof backgroundDiagnostics.body.data.backgroundReview.lastRunAt,
    "string",
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

  const configPath = join(root, "config", "service.json");
  const invalidConfig = JSON.parse(readFileSync(configPath, "utf8")) as {
    channelBindings?: Array<{ agentId?: string }>;
  };
  if (invalidConfig.channelBindings?.[0]) {
    invalidConfig.channelBindings[0].agentId = "wrong-field-agent";
  }
  writeFileSync(configPath, JSON.stringify(invalidConfig, null, 2));
  const invalidConfigValidation = await get(
    "/v1/admin/diagnostics/config",
    token,
  );
  assert.equal(invalidConfigValidation.status, 200);
  assert.equal(invalidConfigValidation.body.data.ok, false);
  assert.equal(
    invalidConfigValidation.body.data.diagnostics[0]?.code,
    "binding_session_mismatch",
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

  const staleLockRoot = mkdtempSync(
    join(tmpdir(), "rusty-crew-service-stale-lock-"),
  );
  const staleLockPort = await openPort();
  writeRuntimeConfig(staleLockRoot);
  mkdirSync(join(staleLockRoot, "run"), { recursive: true });
  writeFileSync(
    join(staleLockRoot, "run", "service.lock"),
    JSON.stringify(
      {
        pid: 999_999_999,
        createdAt: "2026-06-27T00:00:00.000Z",
      },
      null,
      2,
    ),
  );
  const staleLockHost = await startNoAuthHost(staleLockRoot, staleLockPort);
  try {
    const staleLockReady = await get(
      "/v1/admin/readyz",
      undefined,
      staleLockPort,
    );
    assert.equal(staleLockReady.status, 200);
    assert.equal(staleLockReady.body.ok, true);
  } finally {
    await staleLockHost.stop();
    assert.equal(existsSync(join(staleLockRoot, "run", "service.lock")), false);
    rmSync(staleLockRoot, { recursive: true, force: true });
  }

  const noAuthRoot = mkdtempSync(join(tmpdir(), "rusty-crew-service-noauth-"));
  const noAuthPort = await openPort();
  writeRuntimeConfig(noAuthRoot);
  const noAuthHost = await startNoAuthHost(noAuthRoot, noAuthPort);
  try {
    const noStaticRoot = await getText("/", noAuthPort);
    assert.equal(noStaticRoot.status, 200);
    assert.match(noStaticRoot.body, /Rusty Crew Admin/);

    const noAuthPanel = await getText("/admin", noAuthPort);
    assert.equal(noAuthPanel.status, 200);
    assert.match(noAuthPanel.body, /tokenForm" class="token-row" hidden/);

    const noAuthReady = await get("/v1/admin/readyz", undefined, noAuthPort);
    assert.equal(noAuthReady.status, 200);
    assert.equal(noAuthReady.body.ok, true);

    const defaultProvider = await post(
      "/v1/admin/model-providers",
      undefined,
      {
        alias: "default",
        displayName: "Default Local",
        protocol: "chat_completions",
        providerKind: "local",
        modelId: "deterministic",
        contextWindowTokens: 8192,
        maxOutputTokens: 512,
        temperature: 0.5,
      },
      noAuthPort,
    );
    assert.equal(defaultProvider.status, 200);
    assert.equal(defaultProvider.body.data.provider.alias, "default");
    assert.equal(defaultProvider.body.data.provider.temperatureMilli, 500);
    assert.equal(defaultProvider.body.data.provider.temperature, 0.5);
    assert.equal(
      defaultProvider.body.data.provider.credential.hasSecret,
      false,
    );
    assert.equal(defaultProvider.body.data.refresh.mode, "none");

    const alternateProvider = await post(
      "/v1/admin/model-providers",
      undefined,
      {
        alias: "alternate",
        displayName: "Alternate Local",
        protocol: "chat_completions",
        providerKind: "local",
        modelId: "deterministic",
        temperatureMilli: 0.5,
        apiKey: "alternate-secret-smoke",
      },
      noAuthPort,
    );
    assert.equal(alternateProvider.status, 200);
    assert.equal(alternateProvider.body.data.provider.alias, "alternate");
    assert.equal(alternateProvider.body.data.provider.temperatureMilli, 500);
    assert.equal(alternateProvider.body.data.provider.temperature, 0.5);
    assert.equal(
      alternateProvider.body.data.provider.credential.hasSecret,
      true,
    );
    assert.doesNotMatch(
      JSON.stringify(alternateProvider.body),
      /alternate-secret-smoke/,
    );
    const alternateRevision = alternateProvider.body.data.provider.revision;
    const updatedAlternateProvider = await patch(
      "/v1/admin/model-providers/alternate",
      undefined,
      {
        alias: "alternate",
        displayName: "Alternate Local Updated",
        protocol: "chat_completions",
        providerKind: "local",
        modelId: "deterministic",
        temperature: 0.75,
        expectedRevision: alternateRevision,
      },
      noAuthPort,
    );
    assert.equal(updatedAlternateProvider.status, 200);
    assert.equal(
      updatedAlternateProvider.body.data.provider.revision,
      alternateRevision + 1,
    );
    assert.equal(
      updatedAlternateProvider.body.data.provider.temperatureMilli,
      750,
    );
    assert.equal(updatedAlternateProvider.body.data.provider.temperature, 0.75);

    const customChatProvider = await post(
      "/v1/admin/model-providers",
      undefined,
      {
        alias: "custom-chat",
        displayName: "Custom Chat",
        protocol: "chat_completions",
        providerKind: "custom",
        modelId: "deterministic",
      },
      noAuthPort,
    );
    assert.equal(customChatProvider.status, 200);

    const staleAlternateProvider = await patch(
      "/v1/admin/model-providers/alternate",
      undefined,
      {
        alias: "alternate",
        displayName: "Alternate Local Stale",
        protocol: "chat_completions",
        providerKind: "local",
        modelId: "deterministic",
        temperature: 0.9,
        expectedRevision: alternateRevision,
      },
      noAuthPort,
    );
    assert.equal(staleAlternateProvider.status, 409);
    assert.equal(staleAlternateProvider.body.ok, false);
    assert.equal(staleAlternateProvider.body.error.code, "conflict");
    assert.equal(
      staleAlternateProvider.body.error.reason_code,
      "model_provider_revision_mismatch",
    );
    assert.equal(
      staleAlternateProvider.body.data.provider.revision,
      alternateRevision + 1,
    );

    const providers = await get(
      "/v1/admin/model-providers",
      undefined,
      noAuthPort,
    );
    assert.deepEqual(
      providers.body.data.items.map((item: { alias: string }) => item.alias),
      ["custom-chat", "alternate", "default"],
    );

    const localToolProfiles = await get(
      "/v1/admin/local-tool-profiles",
      undefined,
      noAuthPort,
    );
    assert.equal(localToolProfiles.status, 200);
    assert.equal(localToolProfiles.body.ok, true);
    assert.ok(
      localToolProfiles.body.data.items.some(
        (item: { id: string; system: boolean; readOnly: boolean }) =>
          item.id === "code_read" && item.system && item.readOnly,
      ),
      "missing seeded code_read local tool profile",
    );

    const customToolProfile = await post(
      "/v1/admin/local-tool-profiles",
      undefined,
      {
        id: "field_custom",
        displayName: "Field Custom",
        description: "Custom smoke local tools.",
        toolsets: ["local_code_read"],
        tools: ["todo"],
      },
      noAuthPort,
    );
    assert.equal(customToolProfile.status, 200);
    assert.equal(customToolProfile.body.ok, true);
    assert.equal(customToolProfile.body.data.profile.id, "field_custom");
    assert.equal(customToolProfile.body.data.profile.revision, 1);

    const invalidToolProfile = await post(
      "/v1/admin/local-tool-profiles",
      undefined,
      {
        id: "bad_mcp",
        displayName: "Bad MCP",
        toolsets: ["mcp:planner"],
      },
      noAuthPort,
    );
    assert.equal(invalidToolProfile.status, 400);
    assert.equal(invalidToolProfile.body.ok, false);
    assert.equal(
      invalidToolProfile.body.error.reason_code,
      "local_tool_profile_rejects_mcp_toolset",
    );

    const updatedToolProfile = await patch(
      "/v1/admin/local-tool-profiles/field_custom",
      undefined,
      {
        expectedRevision: customToolProfile.body.data.profile.revision,
        displayName: "Field Custom Updated",
        toolsets: ["local_code_read", "skills_read"],
        tools: ["todo"],
      },
      noAuthPort,
    );
    assert.equal(updatedToolProfile.status, 200);
    assert.equal(updatedToolProfile.body.data.profile.revision, 2);
    assert.deepEqual(updatedToolProfile.body.data.profile.toolsets, [
      "local_code_read",
      "skills_read",
    ]);

    const deletedToolProfile = await del(
      "/v1/admin/local-tool-profiles/field_custom",
      undefined,
      noAuthPort,
    );
    assert.equal(deletedToolProfile.status, 200);
    assert.equal(deletedToolProfile.body.data.deleted, true);

    const invalidProfile = await post(
      "/v1/admin/control/profiles",
      undefined,
      { profileId: "../bad" },
      noAuthPort,
    );
    assert.equal(invalidProfile.status, 200);
    assert.equal(invalidProfile.body.data.outcome.status, "failed");
    assert.match(
      invalidProfile.body.data.outcome.summary,
      /profileId must start/,
    );

    const createdProfile = await post(
      "/v1/admin/control/profiles",
      undefined,
      {
        profileId: "field-created-profile",
        displayName: "Field Created Profile",
        providerAlias: "alternate",
        mcpBindings: [
          {
            serverId: "field",
            toolProfileKey: "field-created-profile",
          },
          {
            serverId: "field-extra",
            bindingId: "field-created-profile-extra-mcp",
            adapterId: "mcp-ts-extra",
            serverNames: ["field-extra"],
            toolProfileKey: "field-created-profile-extra",
          },
        ],
        localToolProfileId: "code_read",
      },
      noAuthPort,
    );
    assert.equal(createdProfile.status, 200);
    assert.equal(createdProfile.body.ok, true);
    assert.equal(
      createdProfile.body.data.outcome.result.profileId,
      "field-created-profile",
    );
    assert.equal(
      createdProfile.body.data.outcome.result.sessionId,
      "field-created-profile-session",
    );
    assert.equal(
      createdProfile.body.data.outcome.result.registryWrite.profileId,
      "field-created-profile",
    );
    assert.equal(
      createdProfile.body.data.outcome.result.fileAssetActions[0].kind,
      "write_profile_json",
    );
    assert.deepEqual(
      createdProfile.body.data.outcome.result.derivedRuntimeActions.map(
        (action: { refKind: string; refId: string }) => [
          action.refKind,
          action.refId,
        ],
      ),
      [
        ["brain", "field-created-profile-brain"],
        ["session", "field-created-profile-session"],
        ["mcp_binding", "field-created-profile-mcp-1"],
        ["mcp_binding", "field-created-profile-extra-mcp"],
      ],
    );
    assert.equal(
      existsSync(
        join(noAuthRoot, "config", "profiles", "field-created-profile.json"),
      ),
      true,
    );
    const createdProfileConfig = JSON.parse(
      readFileSync(
        join(noAuthRoot, "config", "profiles", "field-created-profile.json"),
        "utf8",
      ),
    ) as {
      brain?: { module?: string };
      mcpConfig?: { toolProfile?: string };
      localToolProfileId?: string;
      toolPolicy?: {
        requestedToolsets?: string[];
        requestedTools?: string[];
      };
      displayName?: string;
      providerAlias?: string;
      modelConfig?: unknown;
    };
    assert.equal(createdProfileConfig.displayName, "Field Created Profile");
    assert.equal(createdProfileConfig.providerAlias, "alternate");
    assert.equal(createdProfileConfig.modelConfig, undefined);
    assert.equal(createdProfileConfig.brain?.module, "local");
    assert.equal(createdProfileConfig.mcpConfig, undefined);
    assert.equal(createdProfileConfig.localToolProfileId, "code_read");
    assert.deepEqual(createdProfileConfig.toolPolicy?.requestedToolsets, [
      "local_code_read",
    ]);
    assert.deepEqual(createdProfileConfig.toolPolicy?.requestedTools, []);

    const customChatProfile = await post(
      "/v1/admin/control/profiles",
      undefined,
      {
        profileId: "field-custom-chat-profile",
        displayName: "Field Custom Chat Profile",
        providerAlias: "custom-chat",
      },
      noAuthPort,
    );
    assert.equal(customChatProfile.status, 200);
    const customChatProfileConfig = JSON.parse(
      readFileSync(
        join(
          noAuthRoot,
          "config",
          "profiles",
          "field-custom-chat-profile.json",
        ),
        "utf8",
      ),
    ) as {
      brain?: { module?: string };
      providerAlias?: string;
    };
    assert.equal(customChatProfileConfig.providerAlias, "custom-chat");
    assert.equal(customChatProfileConfig.brain?.module, "pi-agent-core");
    const decommissionCustomChatProfile = await post(
      "/v1/admin/control/profiles/field-custom-chat-profile/decommission",
      undefined,
      { reason: "service host smoke custom chat cleanup" },
      noAuthPort,
    );
    assert.equal(decommissionCustomChatProfile.status, 200);
    assert.equal(decommissionCustomChatProfile.body.ok, true);
    const runtimeConfigAfterProfileCreate = JSON.parse(
      readFileSync(join(noAuthRoot, "config", "service.json"), "utf8"),
    ) as {
      mcpBindings?: Array<{
        bindingId: string;
        endpointRef: string;
        toolProfileKey: string;
      }>;
    };
    assert.deepEqual(
      runtimeConfigAfterProfileCreate.mcpBindings
        ?.filter((binding) =>
          binding.bindingId.startsWith("field-created-profile"),
        )
        .map((binding) => ({
          bindingId: binding.bindingId,
          endpointRef: binding.endpointRef,
          toolProfileKey: binding.toolProfileKey,
        })),
      [
        {
          bindingId: "field-created-profile-mcp-1",
          endpointRef: "config://mcp/field",
          toolProfileKey: "field-created-profile",
        },
        {
          bindingId: "field-created-profile-extra-mcp",
          endpointRef: "config://mcp/field-extra",
          toolProfileKey: "field-created-profile-extra",
        },
      ],
    );
    const readProfile = await post(
      "/v1/admin/control/profiles/field-created-profile/read",
      undefined,
      {},
      noAuthPort,
    );
    assert.equal(readProfile.status, 200);
    assert.equal(
      readProfile.body.data.outcome.result.profileId,
      "field-created-profile",
    );

    const createdRegistry = await get(
      "/v1/admin/profiles/registry/field-created-profile",
      undefined,
      noAuthPort,
    );
    assert.equal(createdRegistry.status, 200);
    assert.equal(createdRegistry.body.data.source, "registry");
    const registryRevision = createdRegistry.body.data.revision as number;
    const registryUpdatePlan = await post(
      "/v1/admin/profiles/registry/field-created-profile/update/plan",
      undefined,
      {
        expectedRevision: registryRevision,
        displayName: "Registry Field Created Profile",
        summary: "Registry-owned summary updated through admin API.",
        ownerId: "registry-owner",
      },
      noAuthPort,
    );
    assert.equal(registryUpdatePlan.status, 200);
    assert.equal(registryUpdatePlan.body.data.ok, true);
    assert.equal(
      registryUpdatePlan.body.data.next.displayName,
      "Registry Field Created Profile",
    );
    const registryUpdateApply = await post(
      "/v1/admin/profiles/registry/field-created-profile/update/apply",
      undefined,
      {
        expectedRevision: registryRevision,
        displayName: "Registry Field Created Profile",
        summary: "Registry-owned summary updated through admin API.",
        ownerId: "registry-owner",
      },
      noAuthPort,
    );
    assert.equal(registryUpdateApply.status, 200);
    assert.equal(registryUpdateApply.body.data.ok, true);
    assert.equal(
      registryUpdateApply.body.data.record.displayName,
      "Registry Field Created Profile",
    );
    assert.equal(
      registryUpdateApply.body.data.record.revision,
      registryRevision + 1,
    );
    const registryPromptApply = await post(
      "/v1/admin/profiles/registry/field-created-profile/prompt/apply",
      undefined,
      {
        expectedRevision: registryRevision + 1,
        soulMarkdown: "Registry DB soul edited through Rusty View.",
        memoryMarkdown: "Registry DB memory edited through Rusty View.",
      },
      noAuthPort,
    );
    assert.equal(registryPromptApply.status, 200);
    assert.equal(registryPromptApply.body.data.ok, true);
    assert.equal(
      registryPromptApply.body.data.record.promptSoulMarkdown,
      "Registry DB soul edited through Rusty View.",
    );
    assert.equal(
      registryPromptApply.body.data.record.promptMemoryMarkdown,
      "Registry DB memory edited through Rusty View.",
    );
    const registryMismatch = await post(
      "/v1/admin/profiles/registry/field-created-profile/update/apply",
      undefined,
      {
        expectedRevision: registryRevision,
        displayName: "Should Not Apply",
      },
      noAuthPort,
    );
    assert.equal(registryMismatch.status, 200);
    assert.equal(registryMismatch.body.data.ok, false);
    assert.equal(
      registryMismatch.body.data.diagnostics[0]?.code,
      "profile_registry_revision_mismatch",
    );
    const fallbackRegistryUpdate = await post(
      "/v1/admin/profiles/registry/field-profile/update/plan",
      undefined,
      {
        expectedRevision: 1,
        displayName: "File Fallback Should Import First",
      },
      noAuthPort,
    );
    assert.equal(fallbackRegistryUpdate.status, 404);
    assert.equal(
      fallbackRegistryUpdate.body.error.reason_code,
      "profile_registry_requires_import",
    );

    const updatedProfileConfig = {
      ...createdProfileConfig,
      profileId: "field-created-profile",
      displayName: "Field Created Profile Updated",
    };
    const profileUpdatePlan = await post(
      "/v1/admin/control/profiles/field-created-profile/update/plan",
      undefined,
      {
        profileConfig: updatedProfileConfig,
        soulMarkdown: "A profile soul edited through Rusty View.",
      },
      noAuthPort,
    );
    assert.equal(profileUpdatePlan.status, 200);
    assert.equal(profileUpdatePlan.body.data.outcome.result.ok, true);
    assert.equal(
      profileUpdatePlan.body.data.outcome.result.implications
        .configReloadRequired,
      true,
    );
    const profileUpdateApply = await post(
      "/v1/admin/control/profiles/field-created-profile/update/apply",
      undefined,
      {
        profileConfig: updatedProfileConfig,
        soulMarkdown: "A profile soul edited through Rusty View.",
      },
      noAuthPort,
    );
    assert.equal(profileUpdateApply.status, 200);
    assert.equal(
      profileUpdateApply.body.data.outcome.result?.ok,
      true,
      JSON.stringify(profileUpdateApply.body),
    );
    const profileAfterUpdate = JSON.parse(
      readFileSync(
        join(noAuthRoot, "config", "profiles", "field-created-profile.json"),
        "utf8",
      ),
    ) as {
      displayName?: string;
      prompt?: { soulMarkdown?: string };
    };
    assert.equal(
      profileAfterUpdate.displayName,
      "Field Created Profile Updated",
    );
    assert.equal(
      profileAfterUpdate.prompt?.soulMarkdown,
      "A profile soul edited through Rusty View.",
    );

    const runtimeDraft = JSON.parse(
      readFileSync(join(noAuthRoot, "config", "service.json"), "utf8"),
    ) as Record<string, unknown>;
    const runtimeDraftPlan = await post(
      "/v1/admin/control/config/draft/plan",
      undefined,
      { runtimeConfig: runtimeDraft },
      noAuthPort,
    );
    assert.equal(
      runtimeDraftPlan.status,
      200,
      JSON.stringify(runtimeDraftPlan.body),
    );
    assert.equal(runtimeDraftPlan.body.data.outcome.result.ok, true);
    const runtimeDraftApply = await post(
      "/v1/admin/control/config/draft/apply",
      undefined,
      { runtimeConfig: runtimeDraft },
      noAuthPort,
    );
    assert.equal(
      runtimeDraftApply.status,
      200,
      JSON.stringify(runtimeDraftApply.body),
    );
    assert.equal(runtimeDraftApply.body.data.outcome.result.ok, true);

    const noAuthAfterProfile = await get(
      "/v1/admin/diagnostics",
      undefined,
      noAuthPort,
    );
    assert.equal(noAuthAfterProfile.body.data.overview.summary.sessions, 2);
    assert.deepEqual(
      noAuthAfterProfile.body.data.overview.runtime.brainModules.map(
        (module: { profileId: string; moduleId: string }) => [
          module.profileId,
          module.moduleId,
        ],
      ),
      [
        ["field-profile", "local"],
        ["field-created-profile", "local"],
      ],
    );
    assert.equal(
      noAuthAfterProfile.body.data.overview.runtime.brainModules.find(
        (module: { profileId: string }) =>
          module.profileId === "field-created-profile",
      )?.providerAlias,
      "alternate",
    );
    assert.equal(
      noAuthAfterProfile.body.data.overview.runtime.brainModules.find(
        (module: { profileId: string }) =>
          module.profileId === "field-created-profile",
      )?.modelProvider.modelId,
      "deterministic",
    );
    assert.equal(
      process.env.RUSTY_CREW_MODEL_PROVIDER_SECRET_ALTERNATE,
      "alternate-secret-smoke",
    );
    assert.equal(
      noAuthAfterProfile.body.data.overview.adapters.mcp.totalSurfaces,
      3,
    );

    const refreshPlan = await patch(
      "/v1/admin/model-providers/alternate?refresh=plan",
      undefined,
      {
        displayName: "Alternate Local Updated",
        protocol: "chat_completions",
        providerKind: "local",
        modelId: "deterministic-updated",
      },
      noAuthPort,
    );
    assert.equal(refreshPlan.status, 200);
    assert.equal(refreshPlan.body.data.refresh.mode, "plan");
    assert.deepEqual(
      refreshPlan.body.data.refresh.affectedProfiles.map(
        (profile: { profileId: string }) => profile.profileId,
      ),
      ["field-created-profile"],
    );
    assert.equal(refreshPlan.body.data.refresh.outcomes[0]?.status, "planned");

    const disabledRefresh = await patch(
      "/v1/admin/model-providers/alternate?refresh=apply",
      undefined,
      {
        status: "disabled",
        displayName: "Alternate Local Disabled",
        protocol: "chat_completions",
        providerKind: "local",
        modelId: "deterministic-updated",
      },
      noAuthPort,
    );
    assert.equal(disabledRefresh.status, 200);
    assert.equal(disabledRefresh.body.data.provider.status, "disabled");
    assert.equal(
      disabledRefresh.body.data.refresh.affectedProfiles[0]?.profileId,
      "field-created-profile",
    );
    assert.equal(
      disabledRefresh.body.data.refresh.outcomes[0]?.status,
      "failed",
    );
    assert.match(
      disabledRefresh.body.data.refresh.outcomes[0]?.summary,
      /active provider required/,
    );
    const reenabledAlternate = await patch(
      "/v1/admin/model-providers/alternate",
      undefined,
      {
        status: "active",
        displayName: "Alternate Local Reenabled",
        protocol: "chat_completions",
        providerKind: "local",
        modelId: "deterministic-updated",
      },
      noAuthPort,
    );
    assert.equal(reenabledAlternate.status, 200);
    assert.equal(reenabledAlternate.body.data.provider.status, "active");

    const lifecycleProfile = await post(
      "/v1/admin/control/profiles",
      undefined,
      {
        profileId: "field-lifecycle-profile",
        displayName: "Field Lifecycle Profile",
        providerAlias: "default",
      },
      noAuthPort,
    );
    assert.equal(
      lifecycleProfile.status,
      200,
      JSON.stringify(lifecycleProfile.body),
    );
    const lifecycleRegistry = await get(
      "/v1/admin/profiles/registry/field-lifecycle-profile",
      undefined,
      noAuthPort,
    );
    assert.equal(lifecycleRegistry.status, 200);
    const lifecycleApply = await post(
      "/v1/admin/profiles/registry/field-lifecycle-profile/lifecycle/apply",
      undefined,
      {
        expectedRevision: lifecycleRegistry.body.data.revision,
        lifecycleStatus: "decommissioned",
      },
      noAuthPort,
    );
    assert.equal(lifecycleApply.status, 200);
    assert.equal(lifecycleApply.body.data.ok, true);
    assert.equal(
      lifecycleApply.body.data.record.lifecycleStatus,
      "decommissioned",
    );
    assert.equal(
      lifecycleApply.body.data.record.derivedRuntimeRefs.every(
        (ref: { status: string }) => ref.status === "disabled",
      ),
      true,
    );
    assert.deepEqual(lifecycleApply.body.data.effects.sessionsArchived, [
      "field-lifecycle-profile-session",
    ]);
    const lifecycleSession = (await noAuthHost.bridge.listSessions()).find(
      (session) => session.sessionId === "field-lifecycle-profile-session",
    );
    assert.equal(lifecycleSession?.status, "archived");
    const lifecycleReactivate = await post(
      "/v1/admin/profiles/registry/field-lifecycle-profile/lifecycle/apply",
      undefined,
      {
        expectedRevision: lifecycleApply.body.data.record.revision,
        lifecycleStatus: "active",
      },
      noAuthPort,
    );
    assert.equal(lifecycleReactivate.status, 200);
    assert.equal(
      lifecycleReactivate.body.data.record.lifecycleStatus,
      "active",
    );
    await post(
      "/v1/admin/control/profiles/field-lifecycle-profile/decommission",
      undefined,
      { reason: "service host smoke lifecycle cleanup" },
      noAuthPort,
    );

    const duplicateProfile = await post(
      "/v1/admin/control/profiles",
      undefined,
      { profileId: "field-created-profile" },
      noAuthPort,
    );
    assert.equal(duplicateProfile.status, 200);
    assert.equal(duplicateProfile.body.data.outcome.status, "failed");
    assert.match(duplicateProfile.body.data.outcome.summary, /already exists/);

    const decommissionProfile = await post(
      "/v1/admin/control/profiles/field-created-profile/decommission",
      undefined,
      { reason: "service host smoke profile cleanup" },
      noAuthPort,
    );
    assert.equal(decommissionProfile.status, 200);
    assert.equal(decommissionProfile.body.ok, true);
    assert.equal(
      decommissionProfile.body.data.outcome.result.profileDirectoryPreserved,
      true,
    );
    assert.equal(
      decommissionProfile.body.data.outcome.result.sessionsArchived[0],
      "field-created-profile-session",
    );
    assert.equal(
      decommissionProfile.body.data.outcome.result.brainHandle.action,
      "removed",
    );
    assert.equal(
      typeof decommissionProfile.body.data.outcome.result.brainHandle.handle,
      "number",
    );
    assert.equal(
      existsSync(
        join(noAuthRoot, "config", "profiles", "field-created-profile.json"),
      ),
      true,
    );
    const noAuthAfterDecommission = await get(
      "/v1/admin/diagnostics",
      undefined,
      noAuthPort,
    );
    assert.deepEqual(
      noAuthAfterDecommission.body.data.overview.runtime.brainModules.map(
        (module: { profileId: string; moduleId: string }) => [
          module.profileId,
          module.moduleId,
        ],
      ),
      [["field-profile", "local"]],
    );
    assert.equal(
      noAuthAfterDecommission.body.data.overview.adapters.mcp.totalSurfaces,
      1,
    );
    const noAuthSessionsAfterDecommission =
      await noAuthHost.bridge.listSessions();
    assert.equal(
      noAuthSessionsAfterDecommission.find(
        (session) => session.sessionId === "field-created-profile-session",
      )?.status,
      "archived",
    );
    const noAuthRuntimeConfigAfterDecommission = JSON.parse(
      readFileSync(join(noAuthRoot, "config", "service.json"), "utf8"),
    ) as {
      brains?: Array<{ profileId?: string }>;
      sessions?: Array<{ profileId?: string }>;
    };
    assert.equal(
      noAuthRuntimeConfigAfterDecommission.brains?.some(
        (brain) => brain.profileId === "field-created-profile",
      ),
      false,
    );
    assert.equal(
      noAuthRuntimeConfigAfterDecommission.sessions?.some(
        (session) => session.profileId === "field-created-profile",
      ),
      false,
    );

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
    cacheControl: response.headers.get("cache-control"),
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

async function patch(
  path: string,
  bearer: string | undefined,
  body: unknown,
  requestPort = port,
) {
  const response = await fetch(`http://127.0.0.1:${requestPort}${path}`, {
    method: "PATCH",
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

async function del(
  path: string,
  bearer: string | undefined,
  requestPort = port,
) {
  const response = await fetch(`http://127.0.0.1:${requestPort}${path}`, {
    method: "DELETE",
    headers: {
      ...(bearer ? { authorization: `Bearer ${bearer}` } : {}),
    },
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

async function rawHttpGet(path: string, requestPort = port): Promise<string> {
  return new Promise((resolveRaw, rejectRaw) => {
    const socket = connect(requestPort, "127.0.0.1");
    let data = "";
    socket.setEncoding("utf8");
    socket.once("error", rejectRaw);
    socket.on("data", (chunk) => {
      data += chunk;
    });
    socket.on("end", () => resolveRaw(data));
    socket.once("connect", () => {
      socket.write(`GET ${path} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n`);
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
    mcpServers: [
      {
        id: "field",
        label: "Field MCP",
        baseUrl: "http://mcp.local/mcp",
        transport: "streamable_http",
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
        backgroundReview: {
          enabled: true,
          reviewType: "combined",
          schedule: "0 3 * * *",
          maxFindings: 10,
          maxCandidates: 25,
          dryRun: true,
        },
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

function writeStaticSite(root: string): void {
  const siteDir = join(root, "site");
  mkdirSync(siteDir, { recursive: true });
  writeFileSync(
    join(siteDir, "index.html"),
    `<!doctype html>
<html>
  <head>
    <title>Rusty View Smoke</title>
    <script type="module" src="/main-1234567890abcdef.js"></script>
    <link rel="stylesheet" href="/styles.css">
  </head>
  <body>Rusty View Smoke</body>
</html>
`,
  );
  writeFileSync(
    join(siteDir, "main-1234567890abcdef.js"),
    `globalThis.__rustyViewSmoke = "rusty-view-smoke";\n`,
  );
  writeFileSync(join(siteDir, "styles.css"), `body { color: black; }\n`);
  writeFileSync(join(siteDir, ".env"), "hidden=true\n");
}
