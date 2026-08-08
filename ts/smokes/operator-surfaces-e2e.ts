import assert from "node:assert/strict";
import type {
  AgentId,
  AgentInstanceId,
  ProfileId,
  SessionHandle,
  SessionId,
  SessionState,
} from "@rusty-crew/contracts";
import {
  AgentActivityObservationProducer,
  apiCapabilityRegistry,
  buildRuntimeDiagnosticsProjection,
  buildRuntimeHealthProjection,
  buildToolRegistryDiagnostics,
  chatCommandRegistry,
  createDebugApiClient,
  createMemoryAdminControlAuditSink,
  createMemoryAgentActivityObservationSink,
  handleAdminControlRequest,
  handleAdminDiagnosticsRequest,
  handleStorageQueryRequest,
  inspectDirectDebugSession,
} from "@rusty-crew/brain-island";
import type {
  AdminApiEnvelope,
  AdminControlExecutor,
  AdminControlResponse,
  AdminRecentEvent,
  AdminRouteResult,
  DebugApiFetch,
  StorageQueryCatalog,
  StorageQueryContext,
} from "@rusty-crew/brain-island";
import {
  loadDebugTuiState,
  reduceDebugTuiState,
  renderDebugTui,
} from "@rusty-crew/adapter-tui";

const now = "2026-06-20T16:00:00.000Z";
const alpha = session("session-alpha", "agent-alpha", "prime", "active", 4);
const beta = session("session-beta", "agent-beta", "review", "idle", 1);
const diagnostics = buildRuntimeDiagnosticsProjection({
  now,
  runtimeSummary: {
    brainTurns: 5,
    wakes: 5,
    toolCalls: 3,
    toolErrors: 1,
    delegationsCreated: 2,
    delegationsCompleted: 1,
    delegationsFailed: 0,
    delegationsTimedOut: 0,
    delegationsCancelled: 0,
    messages: 8,
    completions: 2,
    queueExpirations: 1,
  },
  sessions: [alpha, beta],
  queues: {
    pending: 3,
    expired: 1,
    oldestPendingAgeMs: 2_500,
  },
  persistence: {
    schemaVersion: 4,
    migrationCount: 4,
    databaseBytes: 1_024 * 1_024,
    maxDatabaseBytes: 2_024 * 1_024,
    searchHealthy: true,
  },
  adapters: {
    generatedAt: now,
    degraded: true,
    channels: {
      totalBindings: 2,
      activeBindings: 1,
      degradedBindings: 1,
      droppedProjections: 1,
      lastProjectionError: "beta channel projection sink unavailable",
      bindings: [
        channel("channel-alpha", alpha, "active", "online"),
        {
          ...channel("channel-beta", beta, "degraded", "idle"),
          droppedProjections: 1,
          lastError: "beta channel projection sink unavailable",
        },
      ],
    },
    mcp: {
      totalSurfaces: 2,
      activeSurfaces: 1,
      degradedSurfaces: 1,
      collisionCount: 1,
      reloadCount: 1,
      surfaces: [
        mcp("mcp-alpha", alpha, "active", ["alpha-tools"], 0),
        {
          ...mcp("mcp-beta", beta, "degraded", ["review-tools"], 1),
          lastError: "review MCP optional server failed",
        },
      ],
    },
    issues: [
      "channel channel-beta: beta channel projection sink unavailable",
      "mcp mcp-beta: review MCP optional server failed",
    ],
  },
  tools: [
    buildToolRegistryDiagnostics({
      catalogId: "alpha-tools",
      inventoryRequest: {
        requestedToolsets: ["local_code_read"],
      },
    }),
    buildToolRegistryDiagnostics({
      catalogId: "beta-tools",
      inventoryRequest: {
        requestedToolsets: ["local_code_read"],
        requestedTools: ["missing_review_tool"],
      },
    }),
  ],
  observation: {
    enabled: true,
    writerAvailable: false,
    lastError: "observation writer offline",
  },
});
const health = buildRuntimeHealthProjection(diagnostics);
const storageQueryContext = {
  bridge: {
    async storageSchema() {
      return moduleRegistryFixture();
    },
    async storageDiagnostics() {
      throw new Error(
        "operator surface smoke only queries the storage catalog",
      );
    },
    async listProfileMemory() {
      throw new Error(
        "operator surface smoke only queries the storage catalog",
      );
    },
    async queryConversationBranches() {
      throw new Error(
        "operator surface smoke only queries the storage catalog",
      );
    },
    async queryRuntimeCounters() {
      throw new Error(
        "operator surface smoke only queries the storage catalog",
      );
    },
    async searchRuntime() {
      throw new Error(
        "operator surface smoke only queries the storage catalog",
      );
    },
    async listSimpleKv() {
      throw new Error(
        "operator surface smoke only queries the storage catalog",
      );
    },
  },
} satisfies StorageQueryContext;
const recentEvents: AdminRecentEvent[] = [
  {
    id: "event-alpha",
    createdAt: now,
    source: "runtime",
    eventType: "agent_session_resumed",
    summary: "Alpha resumed.",
    severity: "info",
    workRef: { sessionId: alpha.sessionId },
  },
  {
    id: "event-beta",
    createdAt: now,
    source: "runtime",
    eventType: "adapter_degraded",
    summary: "Beta channel degraded.",
    severity: "warning",
    workRef: { sessionId: beta.sessionId },
  },
];
const calls: string[] = [];
const fakeFetch: DebugApiFetch = async (input, init) => {
  const url = new URL(String(input));
  calls.push(`${init?.method ?? "GET"} ${url.pathname}`);
  if (url.pathname.startsWith("/v1/admin/storage/")) {
    const result = await handleStorageQueryRequest(
      {
        method: init?.method ?? "GET",
        url: `${url.pathname}${url.search}`,
        requestId: "operator-storage-e2e",
      },
      storageQueryContext,
    );
    return jsonResponse(result.body, result.status);
  }
  if (url.pathname.startsWith("/v1/admin/")) {
    const result = handleAdminDiagnosticsRequest(
      {
        method: init?.method ?? "GET",
        url: `${url.pathname}${url.search}`,
        requestId: "operator-e2e",
      },
      { diagnostics, health, recentEvents },
    );
    return jsonResponse(result.body, result.status);
  }
  if (url.pathname === `/v1/debug/sessions/${alpha.sessionId}/context`) {
    const result = inspectDirectDebugSession(
      {
        sessionId: alpha.sessionId,
        includeMessageBodies: true,
      },
      {
        diagnostics,
        sessions: [
          {
            session: alpha,
            bodyState: {
              session: alpha,
              pendingMessages: [
                {
                  from: "operator" as AgentId,
                  to: alpha.agentId,
                  body: "Check current state without channel delivery.",
                },
              ],
              recentEvents: [],
              childCompletions: [],
              fanOutGroups: [],
              deltaPolicy: {
                mode: "frozen_snapshot_next_wake",
                queueOwner: "body",
                queuedMessageTtlMs: 5_000,
                maxQueuedMessages: 10,
              },
            },
          },
        ],
        recentEvents,
        now: () => now,
      },
    );
    assert.equal(result.ok, true);
    return jsonResponse({
      ok: true,
      data: result.data,
      meta: { request_id: "operator-debug", schema_version: 1 },
    } satisfies AdminApiEnvelope<unknown>);
  }
  return jsonResponse(
    {
      ok: false,
      error: {
        code: "not_found",
        reason_code: "operator_e2e_missing",
        message: "operator e2e fake route missing",
        retryable: false,
      },
      meta: { request_id: "missing", schema_version: 1 },
    } satisfies AdminApiEnvelope<never>,
    404,
  );
};
const client = createDebugApiClient({
  baseUrl: "http://rusty-crew.local",
  bearerToken: "operator-token",
  fetchImpl: fakeFetch,
  retries: 0,
});

const overview = handleAdminDiagnosticsRequest(
  { method: "GET", url: "/v1/admin/diagnostics/overview" },
  { diagnostics, health },
);
assert.equal(overview.status, 200);
assert.equal(okData<{ degraded: boolean }>(overview).degraded, true);
assert.equal(health.liveness.ok, true);
assert.equal(health.readiness.ready, true);

const capabilities = okEnvelope<ReturnType<typeof apiCapabilityRegistry>>(
  await fetchJson("/v1/admin/capabilities"),
);
assert.deepEqual(
  capabilities.slash_commands.map((command) => command.name),
  chatCommandRegistry().commands.map((command) => command.name),
);
assert.equal(
  capabilities.slash_commands.find((command) => command.name === "reload-mcp")
    ?.rust_plan_operation,
  "plan_reload_mcp_control",
);
assert.equal(
  capabilities.capabilities.find(
    (capability) => capability.id === "admin.control.mcp.reload",
  )?.rust_plan_operation,
  "plan_reload_mcp_control",
);
assert.ok(
  capabilities.capabilities.some(
    (capability) =>
      capability.path_template === "/v1/admin/storage/query-catalog",
  ),
  "capability registry should expose storage query catalog",
);

const storageCatalog = okEnvelope<StorageQueryCatalog>(
  await fetchJson("/v1/admin/storage/query-catalog"),
);
const simpleKvQuery = storageCatalog.items.find(
  (query) => query.id === "simple_kv.entries",
);
assert.ok(
  simpleKvQuery,
  "storage query catalog should expose simple_kv.entries",
);
assert.equal(simpleKvQuery.owner, "rust_coordination");
assert.equal(simpleKvQuery.module?.ownerCrate, "core_persistence");
assert.equal(simpleKvQuery.module?.rustQueryId, "list_entries_by_scope");
assert.equal(simpleKvQuery.readOnly, true);

const sessions = await client.sessions({ limit: 10 });
assert.equal(sessions.total, 2);
const degradedMcp = await client.mcpSurfaces({ status: "degraded" });
assert.equal(degradedMcp.items[0]?.bindingId, "mcp-beta");
const channels = await client.channelBindings({ status: "degraded" });
assert.equal(channels.items[0]?.bindingId, "channel-beta");

const tui = await loadDebugTuiState(client, {
  activeSessionId: alpha.sessionId,
  includeDirectDebugContext: true,
});
const tuiText = renderDebugTui(tui, { width: 120, height: 22 });
assert.match(tuiText, /Rusty Crew Debug \[DEGRADED\]/);
assert.match(tuiText, /sessions: active=1 idle=1 archived=0/);
assert.match(tuiText, /observation writer offline/);
const contextTui = renderDebugTui(
  { ...tui, activeTab: "context" },
  { width: 120, height: 12 },
);
assert.match(contextTui, /session-alpha/);
assert.match(contextTui, /direct turns: disabled/);
assert.equal(reduceDebugTuiState(tui, "q").quitRequested, true);

const auditSink = createMemoryAdminControlAuditSink();
const observationSink = createMemoryAgentActivityObservationSink();
const observationProducer = new AgentActivityObservationProducer({
  sink: observationSink,
  required: true,
});
const executor: AdminControlExecutor = {
  archiveSession(command) {
    return {
      status: "completed",
      summary: `Archived ${command.target.sessionId}.`,
      affectedIds: { sessionId: command.target.sessionId ?? "" },
    };
  },
};
const unauthorized = await handleAdminControlRequest(
  {
    method: "POST",
    url: "/v1/admin/control/sessions/session-alpha/archive",
  },
  controlContext(executor, auditSink, observationProducer),
);
assert.equal(unauthorized.status, 401);
const archive = await handleAdminControlRequest(
  {
    method: "POST",
    url: "/v1/admin/control/sessions/session-alpha/archive",
    headers: { authorization: "Bearer control-token" },
    body: {
      reason: "operator e2e archive",
      reasonCode: "operator_e2e",
    },
  },
  controlContext(executor, auditSink, observationProducer),
);
assert.equal(archive.status, 200);
assert.equal(okData<AdminControlResponse>(archive).outcome.status, "completed");
assert.equal(auditSink.events.length, 2);
assert.equal(observationSink.events.length, 2);
assert.equal(observationSink.events[0]?.event_type, "admin_command_started");

console.log(
  JSON.stringify(
    {
      sessions: sessions.total,
      degradedChannels: channels.total,
      degradedMcp: degradedMcp.total,
      health: diagnostics.health,
      ready: health.readiness.ready,
      tuiTabs: tui.tabs.length,
      controlAuditEvents: auditSink.events.length,
      controlObservationEvents: observationSink.events.length,
      fakeFetchCalls: calls.length,
      capabilityCommands: capabilities.slash_commands.length,
      storageQueries: storageCatalog.items.length,
    },
    null,
    2,
  ),
);

function session(
  sessionId: string,
  agentId: string,
  profileId: string,
  status: SessionState["status"],
  turns: number,
): SessionState {
  return {
    handle: sessionId.length as SessionHandle,
    sessionId: sessionId as SessionId,
    agentId: agentId as AgentId,
    profileId: profileId as ProfileId,
    kind: "full",
    workspace: {
      cwd: "/home/dev/rusty-crew",
      revision: 1,
      updatedAt: "2026-06-20T15:00:00.000Z",
    },
    resourceLimits: {},
    toolProfile: {
      tools: [{ name: "read_file", description: "Read a file." }],
    },
    status,
    brainTurnCount: turns,
    createdAt: "2026-06-20T15:00:00.000Z",
    lastActiveAt: "2026-06-20T15:59:00.000Z",
  };
}

function channel(
  bindingId: string,
  state: SessionState,
  status: "active" | "degraded",
  presenceStatus: string,
) {
  return {
    bindingId,
    adapterId: "den-channel-main",
    agentId: state.agentId,
    sessionId: state.sessionId,
    profileId: state.profileId,
    provider: "den_channels",
    status,
    membershipStatus: "joined",
    presenceStatus,
    subscriptionStatus: status === "active" ? "active" : "degraded",
    stalePresence: false,
    droppedProjections: 0,
  };
}

function mcp(
  bindingId: string,
  state: SessionState,
  status: "active" | "degraded",
  serverNames: string[],
  collisionCount: number,
) {
  return {
    bindingId,
    adapterId: "mcp-main",
    agentId: state.agentId,
    sessionId: state.sessionId,
    profileId: state.profileId,
    status,
    transport: "stdio",
    serverNames,
    toolProfileKey: `${state.profileId}:${serverNames.join("-")}`,
    reconnectAttempts: status === "active" ? 0 : 2,
    collisionCount,
    discoveryIssueCount: status === "active" ? 0 : 1,
    optionalServerFailures:
      status === "active" ? [] : ["optional server failed"],
  };
}

function controlContext(
  executor: AdminControlExecutor,
  auditSink: ReturnType<typeof createMemoryAdminControlAuditSink>,
  observationProducer: AgentActivityObservationProducer,
) {
  return {
    auth: { bearerToken: "control-token", operatorId: "operator-e2e" },
    executor,
    auditSink,
    observationProducer,
    observationIdentity: {
      profile: "operator" as ProfileId,
      instance_id: "rusty-crew-admin" as AgentInstanceId,
      session_key: "admin-session" as SessionId,
    },
    now: () => now,
  };
}

function fetchJson(pathname: string): Promise<AdminApiEnvelope<unknown>> {
  return fakeFetch(`http://rusty-crew.local${pathname}`, {
    method: "GET",
    headers: { authorization: "Bearer operator-token" },
  }).then((response) => response.json() as Promise<AdminApiEnvelope<unknown>>);
}

function okEnvelope<T>(body: AdminApiEnvelope<unknown>): T {
  if (!body.ok) {
    assert.fail(
      `expected ok envelope, got ${body.error.reason_code}: ${body.error.message}`,
    );
  }
  return body.data as T;
}

function okData<T>(result: AdminRouteResult): T {
  assert.equal(result.body.ok, true);
  if (!result.body.ok) throw new Error("expected admin route success");
  return result.body.data as T;
}

function moduleRegistryFixture(): Awaited<
  ReturnType<StorageQueryContext["bridge"]["storageSchema"]>
> {
  return {
    source: "compiled_module_schema_registry",
    backendCapabilities: ["transactions", "json_documents"],
    modules: [
      {
        moduleId: "simple_kv",
        ownerCrate: "core_persistence",
        ownerModule: "simple_kv",
        descriptorVersion: 1,
        installedVersion: 1,
        migrationStatus: "installed",
        descriptorFingerprint: "fnv1a64:operator",
        installedDescriptorFingerprint: "fnv1a64:operator",
        installedAt: "2026-06-20T15:00:00.000Z",
        updatedAt: "2026-06-20T15:00:00.000Z",
        capabilityStatus: [
          {
            capability: "transactions",
            required: true,
            supported: true,
            backendVariant: "sqlite",
          },
          {
            capability: "json_documents",
            required: false,
            supported: true,
            backendVariant: "sqlite",
          },
        ],
        logicalStores: [
          { storeName: "entries", description: "Simple entries" },
        ],
        physicalTables: [
          {
            tableName: "entries",
            logicalStore: "entries",
            physicalTable: "module_simple_kv_entries",
            declaration: "owned",
          },
        ],
        physicalIndexes: [],
        retention: [],
        repositoryContracts: [],
        queryCatalogEntries: [
          {
            queryId: "list_entries_by_scope",
            storeName: "entries",
            description: "List simple key/value entries for a scope",
            parameterSchemaId: "simple_kv_scope_query",
          },
        ],
        exportHooks: [],
        importHooks: [],
        migrationNotes: [],
        degradedReasons: [],
        blockedReasons: [],
      },
    ],
    orphanInstalledModules: [],
  };
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}
