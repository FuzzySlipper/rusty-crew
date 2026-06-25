import assert from "node:assert/strict";
import type {
  AgentId,
  ProfileId,
  SessionHandle,
  SessionId,
  SessionState,
} from "@rusty-crew/contracts";
import {
  buildAdapterDiagnosticsProjection,
  buildBackgroundServiceDiagnosticsProjection,
  buildRuntimeDiagnosticsProjection,
  buildToolRegistryDiagnostics,
  handleAdminDiagnosticsRequest,
  type AdminPage,
  type AdminRecentEvent,
  type AdminRouteResult,
  type RuntimeReadinessProbe,
  type RuntimeCounterSummary,
} from "./index.js";

const now = "2026-06-20T14:00:00.000Z";
const counters: RuntimeCounterSummary = {
  brainTurns: 4,
  wakes: 4,
  toolCalls: 2,
  toolErrors: 1,
  delegationsCreated: 1,
  delegationsCompleted: 0,
  delegationsFailed: 0,
  delegationsTimedOut: 0,
  delegationsCancelled: 0,
  messages: 5,
  completions: 1,
  queueExpirations: 1,
};
const diagnostics = buildRuntimeDiagnosticsProjection({
  now,
  runtimeSummary: counters,
  sessions: [
    session("session-alpha", "agent-alpha", "prime", "active"),
    session("session-beta", "agent-beta", "review", "idle"),
    session("session-gamma", "agent-alpha", "prime", "archived"),
  ],
  delegatedSessions: [],
  queues: { pending: 2, expired: 1 },
  persistence: {
    schemaVersion: 3,
    migrationCount: 3,
    databaseBytes: 512,
    maxDatabaseBytes: 10_000,
    searchHealthy: true,
  },
  adapters: buildAdapterDiagnosticsProjection({
    now,
    channelBindings: [
      {
        bindingId: "channel-alpha",
        adapterId: "den-channel-main" as never,
        provider: "den_channels",
        agentId: "agent-alpha" as AgentId,
        sessionId: "session-alpha" as SessionId,
        profileId: "prime" as ProfileId,
        externalChannelId: "room-alpha",
        status: "active",
      },
      {
        bindingId: "channel-beta",
        adapterId: "den-channel-main" as never,
        provider: "den_channels",
        agentId: "agent-beta" as AgentId,
        sessionId: "session-beta" as SessionId,
        profileId: "review" as ProfileId,
        externalChannelId: "room-beta",
        status: "degraded",
      },
    ],
    mcpBindings: [],
    mcpSurfaces: [
      {
        bindingId: "mcp-alpha",
        status: "active",
        transport: "stdio",
        serverNames: ["alpha"],
        endpointRef: "config://mcp/alpha",
        toolProfileKey: "prime-mcp",
        reconnectAttempts: 0,
        optional: false,
      },
    ],
  }),
  tools: [
    buildToolRegistryDiagnostics({ catalogId: "default-local-tools" }),
    buildToolRegistryDiagnostics({
      catalogId: "broken-tools",
      entries: [tool("read_file", "first"), tool("read_file", "second")],
    }),
  ],
  observation: { enabled: true, writerAvailable: true },
  brainModules: [
    {
      profileId: "prime" as ProfileId,
      implementationId: "prime-brain" as never,
      moduleId: "openai-responses",
      effectiveStrategy: "replay",
      providerStateMode: "optional",
      selectedToolCount: 1,
      selectedToolSource: "default-local-tools",
      toolAdapterStatus: "native_neutral_tools",
    },
  ],
  providerStates: [
    {
      sessionId: "session-alpha" as SessionId,
      moduleId: "openai-responses",
      strategyId: "replay",
      status: "valid",
      payloadVersion: "provider-owned-v1",
      payloadBytes: 42,
      lastWakeId: "wake-alpha",
    },
  ],
});
const background = buildBackgroundServiceDiagnosticsProjection({
  now,
  scheduler: {
    jobCount: 3,
    activeJobs: 2,
    pausedJobs: 1,
    staleRuns: 1,
    nextDueAt: "2026-06-20T14:05:00.000Z",
  },
  curator: {
    status: "available",
    candidateCount: 4,
    mutationCount: 1,
  },
  backgroundReview: {
    enabled: true,
    recentFindings: 2,
  },
  cleanup: {
    lastRunAt: "2026-06-20T13:55:00.000Z",
    terminalArchived: 1,
    orphanedArchived: 0,
    expiredArchived: 1,
    adapterReleased: 1,
    adapterDegraded: 0,
  },
});

const overview = handleAdminDiagnosticsRequest(
  {
    method: "GET",
    url: "/v1/admin/diagnostics/overview",
    requestId: "req-overview",
  },
  { diagnostics },
);
assert.equal(overview.status, 200);
assert.equal(overview.body.ok, true);
assert.equal(overview.body.meta.request_id, "req-overview");
assert.equal(
  okData<{ summary: { sessions: number } }>(overview).summary.sessions,
  3,
);

const ready = handleAdminDiagnosticsRequest(
  { method: "GET", url: "/v1/admin/readyz" },
  { diagnostics },
);
assert.equal(ready.status, 200);
assert.equal(okData<RuntimeReadinessProbe>(ready).ready, false);

const sessions = handleAdminDiagnosticsRequest(
  {
    method: "GET",
    url: "/v1/admin/diagnostics/sessions?profile_id=prime&limit=1",
  },
  { diagnostics },
);
assert.equal(sessions.status, 200);
const sessionPage = okData<AdminPage<{ sessionId: string }>>(sessions);
assert.equal(sessionPage.total, 2);
assert.equal(sessionPage.items.length, 1);
assert.equal(sessionPage.nextOffset, 1);

const agents = handleAdminDiagnosticsRequest(
  { method: "GET", url: "/v1/admin/diagnostics/agents" },
  { diagnostics },
);
const agentPage =
  okData<AdminPage<{ agentId: string; sessions: number }>>(agents);
assert.equal(agentPage.items[0]?.agentId, "agent-alpha");
assert.equal(agentPage.items[0]?.sessions, 2);

const channels = handleAdminDiagnosticsRequest(
  {
    method: "GET",
    url: "/v1/admin/diagnostics/channels?status=degraded",
  },
  { diagnostics },
);
const channelPage = okData<AdminPage<{ bindingId: string }>>(channels);
assert.equal(channelPage.items.length, 1);
assert.equal(channelPage.items[0]?.bindingId, "channel-beta");

const tools = handleAdminDiagnosticsRequest(
  {
    method: "GET",
    url: "/v1/admin/diagnostics/tools?invalid=true",
  },
  { diagnostics },
);
const toolPage = okData<AdminPage<{ catalogId: string }>>(tools);
assert.equal(toolPage.items[0]?.catalogId, "broken-tools");

const redacted = handleAdminDiagnosticsRequest(
  { method: "GET", url: "/v1/admin/events/recent" },
  {
    diagnostics,
    recentEvents: [
      {
        id: 1,
        createdAt: now,
        source: "runtime",
        eventType: "admin_command_started",
        summary: "admin command started",
        token: "secret-token",
      } as unknown as AdminRecentEvent,
    ],
  },
);
const eventPage = okData<AdminPage<{ token: string }>>(redacted);
assert.equal(eventPage.items[0]?.token, "[redacted]");

const metrics = handleAdminDiagnosticsRequest(
  { method: "GET", url: "/v1/admin/diagnostics/metrics?limit=500" },
  { diagnostics },
);
const metricPage = okData<AdminPage<unknown>>(metrics);
assert.equal(metricPage.limit, 250);

const providerState = handleAdminDiagnosticsRequest(
  { method: "GET", url: "/v1/admin/diagnostics/provider-state" },
  { diagnostics },
);
assert.equal(providerState.status, 200);
assert.equal(
  okData<Array<{ providerState?: { status: string } }>>(providerState)[0]
    ?.providerState?.status,
  "valid",
);

const backgroundRoute = handleAdminDiagnosticsRequest(
  { method: "GET", url: "/v1/admin/diagnostics/background" },
  { diagnostics, background },
);
assert.equal(backgroundRoute.status, 200);
assert.equal(
  okData<{ summary: { activeJobs: number; cleanupArchived: number } }>(
    backgroundRoute,
  ).summary.activeJobs,
  2,
);

const configRoute = handleAdminDiagnosticsRequest(
  { method: "GET", url: "/v1/admin/diagnostics/config" },
  {
    diagnostics,
    configValidation: {
      ok: false,
      configPath: "/tmp/rusty-crew/config/service.json",
      profilesDir: "/tmp/rusty-crew/config/profiles",
      diagnostics: [
        {
          severity: "error",
          code: "binding_session_mismatch",
          path: "channelBindings[0].sessionId",
          message: "binding target session mismatch",
        },
      ],
      summary: {
        diagnostics: 1,
        errors: 1,
        warnings: 0,
        brains: 1,
        sessions: 1,
        scheduledJobs: 2,
        channelBindings: 1,
        mcpBindings: 1,
        derivedScheduledJobs: 1,
        derivedMcpBindings: 1,
        sessionDefaultsApplied: 1,
      },
      derived: {
        scheduledJobs: [
          {
            id: "background-review-prime",
            shape: "host_job",
            jobKind: "runtime.review.memory_skills",
          },
        ],
        mcpBindings: [
          {
            bindingId: "agent-alpha-mcp",
            agentId: "agent-alpha",
            sessionId: "session-alpha",
            profileId: "prime",
            transport: "stdio",
            toolProfileKey: "prime",
            serverNames: ["agent-alpha"],
          },
        ],
        sessionDefaultsApplied: [
          {
            sessionId: "session-alpha",
            ownerId: true,
            resourceLimits: false,
            maxHistoryMessages: true,
            turnTimeoutMs: true,
          },
        ],
      },
    },
  },
);
assert.equal(configRoute.status, 200);
assert.equal(
  okData<{ ok: boolean; summary: { errors: number } }>(configRoute).ok,
  false,
);
assert.equal(
  okData<{ diagnostics: Array<{ code: string }> }>(configRoute).diagnostics[0]
    ?.code,
  "binding_session_mismatch",
);
assert.equal(
  okData<{ summary: { activeJobs: number; cleanupArchived: number } }>(
    backgroundRoute,
  ).summary.cleanupArchived,
  2,
);

const wrongMethod = handleAdminDiagnosticsRequest(
  { method: "POST", url: "/v1/admin/diagnostics" },
  { diagnostics },
);
assert.equal(wrongMethod.status, 405);
assert.equal(wrongMethod.body.ok, false);

const missing = handleAdminDiagnosticsRequest(
  { method: "GET", url: "/v1/admin/nope" },
  { diagnostics },
);
assert.equal(missing.status, 404);
assert.equal(missing.body.ok, false);

console.log(
  JSON.stringify(
    {
      overview: overview.status,
      ready: okData<RuntimeReadinessProbe>(ready).ready,
      sessions: sessionPage.total,
      channels: channelPage.total,
      redacted: eventPage.items[0]?.token,
      metricsLimit: metricPage.limit,
      backgroundHealth: okData<{ health: string }>(backgroundRoute).health,
    },
    null,
    2,
  ),
);

function okData<T>(result: AdminRouteResult): T {
  assert.equal(result.body.ok, true);
  if (!result.body.ok) throw new Error("expected admin route success");
  return result.body.data as T;
}

function session(
  sessionId: string,
  agentId: string,
  profileId: string,
  status: SessionState["status"],
): SessionState {
  return {
    handle: Number(sessionId.length) as SessionHandle,
    sessionId: sessionId as SessionId,
    agentId: agentId as AgentId,
    profileId: profileId as ProfileId,
    kind: "full",
    resourceLimits: {},
    toolProfile: {
      tools: [{ name: "read_file", description: "Read a file." }],
    },
    status,
    brainTurnCount: 1,
    createdAt: "2026-06-20T12:00:00.000Z",
    lastActiveAt: "2026-06-20T13:59:00.000Z",
  };
}

function tool(name: string, implementationModule: string) {
  return {
    name,
    description: "Duplicate tool.",
    category: "local" as const,
    toolsets: ["local_code_read"],
    implementationModule,
    surfaces: ["brain" as const],
    safety: ["read_only" as const],
    outputShape: "duplicate",
    version: "1.0.0",
    inventoryTest: "smoke",
  };
}
