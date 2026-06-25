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

    const invalidProfile = await post(
      "/v1/admin/control/profiles",
      undefined,
      { profileId: "../bad" },
      noAuthPort,
    );
    assert.equal(invalidProfile.status, 500);
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
      displayName?: string;
    };
    assert.equal(createdProfileConfig.displayName, "Field Created Profile");
    assert.equal(createdProfileConfig.brain?.module, "local");
    assert.equal(
      createdProfileConfig.mcpConfig?.toolProfile,
      "field-created-profile",
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
    assert.equal(profileUpdateApply.body.data.outcome.result.ok, true);
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
      noAuthAfterProfile.body.data.overview.adapters.mcp.totalSurfaces,
      2,
    );

    const duplicateProfile = await post(
      "/v1/admin/control/profiles",
      undefined,
      { profileId: "field-created-profile" },
      noAuthPort,
    );
    assert.equal(duplicateProfile.status, 500);
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
