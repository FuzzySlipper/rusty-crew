import assert from "node:assert/strict";
import type {
  AgentId,
  MemorySpaceDescriptor,
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
  type AdminProfileRegistryDiagnostics,
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
      strategy: "previous-response-chain",
      effectiveStrategy: "replay",
      strategyDiagnostics: {
        selectedStrategyId: "previous-response-chain",
        effectiveStrategyId: "replay",
        replayFallbackUsed: true,
        fallbackReason: "provider_state_expired",
        fallbackReasonCatalog: [
          "no_predecessor_state",
          "request_fingerprint_mismatch",
          "profile_fingerprint_mismatch",
          "provider_fingerprint_mismatch",
          "predecessor_rejected_by_provider",
          "provider_state_expired",
          "provider_state_load_failed",
          "input_not_append_only",
          "normal_invalidation",
        ],
      },
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
const storage = {
  backend: "sqlite",
  backendLabel: "SQLite WAL",
  schemaVersion: 19,
  supportedSchemaVersion: 19,
  migrations: [
    {
      version: 19,
      description: "storage diagnostics smoke",
      appliedAt: now,
    },
  ],
  size: {
    databaseBytes: 4096,
    pageCount: 1,
    pageSizeBytes: 4096,
    freelistPages: 0,
    freelistBytes: 0,
    walBytes: 0,
  },
  tableCounts: [
    { table: "sessions", rows: 3 },
    { table: "profile_memories", rows: 2 },
  ],
  capabilities: [
    {
      name: "transactions",
      supported: true,
      detail: "single-node ACID transactions are supported",
    },
    {
      name: "concurrent_writers",
      supported: false,
      detail: "SQLite serializes writers",
    },
  ],
  indexChecks: [
    {
      name: "pending_queue_by_agent",
      usesIndex: true,
      detail: "SEARCH queued_messages USING INDEX",
    },
  ],
  searchHealthy: true,
  pressure: false,
};

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

const storageRoute = handleAdminDiagnosticsRequest(
  { method: "GET", url: "/v1/admin/diagnostics/storage" },
  { diagnostics, storage },
);
assert.equal(storageRoute.status, 200);
const storageData = okData<{
  backend: string;
  capabilities: Array<{ name: string; supported: boolean }>;
  tableCounts: Array<{ table: string; rows: number }>;
}>(storageRoute);
assert.equal(storageData.backend, "sqlite");
assert.equal(
  storageData.capabilities.find(
    (capability) => capability.name === "transactions",
  )?.supported,
  true,
);
assert.equal(
  storageData.tableCounts.find((count) => count.table === "sessions")?.rows,
  3,
);

const profileRegistry: AdminProfileRegistryDiagnostics = {
  generatedAt: now,
  registryCount: 2,
  fileFallbackCount: 1,
  driftCount: 1,
  missingAssetCount: 1,
  diagnostics: [
    {
      severity: "warning",
      code: "profile_registry_asset_drift",
      path: "profiles.prime.assets.soul_md",
      message: "profile registry asset fingerprint changed",
    },
  ],
  records: [
    {
      source: "registry",
      profileId: "prime",
      lifecycleStatus: "active",
      displayName: "Prime",
      revision: 3,
      activeRuntimeRefs: [
        {
          refKind: "session",
          refId: "session-alpha",
          status: "active",
          metadataJson: {},
        },
      ],
      sourceAssetRefs: [
        {
          assetKind: "soul_md",
          path: "/profiles/prime/soul.md",
          contentHash: "sha256:old",
          metadataJson: {},
        },
      ],
      sourceAssetStatuses: [
        {
          assetKind: "soul_md",
          path: "/profiles/prime/soul.md",
          contentHash: "sha256:old",
          currentContentHash: "sha256:new",
          status: "changed",
        },
      ],
      diagnostics: [
        {
          severity: "warning",
          code: "profile_registry_asset_drift",
          path: "profiles.prime.assets.soul_md",
          message: "profile registry asset fingerprint changed",
        },
      ],
      fallbackStatus: "registry_authoritative",
    },
    {
      source: "registry",
      profileId: "archived-profile",
      lifecycleStatus: "archived",
      displayName: "Archived Profile",
      revision: 7,
      activeRuntimeRefs: [],
      sourceAssetRefs: [],
      sourceAssetStatuses: [],
      diagnostics: [],
      fallbackStatus: "registry_authoritative",
    },
    {
      source: "registry",
      profileId: "decommissioned-profile",
      lifecycleStatus: "decommissioned",
      displayName: "Decommissioned Profile",
      revision: 5,
      activeRuntimeRefs: [],
      sourceAssetRefs: [],
      sourceAssetStatuses: [
        {
          assetKind: "profile_yaml",
          path: "/profiles/decommissioned-profile/profile.yaml",
          contentHash: "sha256:gone",
          status: "missing",
        },
      ],
      diagnostics: [
        {
          severity: "error",
          code: "profile_registry_asset_missing",
          path: "profiles.decommissioned-profile.assets.profile_yaml",
          message: "profile registry asset is missing",
        },
      ],
      fallbackStatus: "registry_authoritative",
    },
    {
      source: "file_fallback",
      profileId: "file-only",
      lifecycleStatus: "paused",
      displayName: "File Only",
      activeRuntimeRefs: [],
      sourceAssetRefs: [
        {
          assetKind: "profile_json",
          path: "/profiles/file-only.json",
          contentHash: "sha256:file",
          metadataJson: {},
        },
      ],
      sourceAssetStatuses: [
        {
          assetKind: "profile_json",
          path: "/profiles/file-only.json",
          contentHash: "sha256:file",
          currentContentHash: "sha256:file",
          status: "tracked",
        },
      ],
      diagnostics: [
        {
          severity: "info",
          code: "file_backed_profile_fallback",
          path: "profiles.file-only",
          message: "profile is currently file backed",
        },
      ],
      fallbackStatus: "file_backed_fallback",
    },
  ],
};

const profileDiagnosticsRoute = handleAdminDiagnosticsRequest(
  { method: "GET", url: "/v1/admin/diagnostics/profiles" },
  { diagnostics, profileRegistry },
);
assert.equal(profileDiagnosticsRoute.status, 200);
assert.equal(
  okData<{ driftCount: number; fileFallbackCount: number }>(
    profileDiagnosticsRoute,
  ).driftCount,
  1,
);

const profileRegistryList = handleAdminDiagnosticsRequest(
  {
    method: "GET",
    url: "/v1/admin/profiles/registry?lifecycle_status=archived",
  },
  { diagnostics, profileRegistry },
);
const profileRegistryPage =
  okData<AdminPage<{ profileId: string; lifecycleStatus: string }>>(
    profileRegistryList,
  );
assert.equal(profileRegistryPage.total, 1);
assert.equal(profileRegistryPage.items[0]?.profileId, "archived-profile");

const fallbackProfiles = handleAdminDiagnosticsRequest(
  { method: "GET", url: "/v1/admin/profiles/registry?source=file_fallback" },
  { diagnostics, profileRegistry },
);
assert.equal(
  okData<AdminPage<{ profileId: string }>>(fallbackProfiles).items[0]
    ?.profileId,
  "file-only",
);

const decommissionedProfile = handleAdminDiagnosticsRequest(
  {
    method: "GET",
    url: "/v1/admin/profiles/registry/decommissioned-profile",
  },
  { diagnostics, profileRegistry },
);
assert.equal(decommissionedProfile.status, 200);
assert.equal(
  okData<{
    lifecycleStatus: string;
    sourceAssetStatuses: Array<{ status: string }>;
  }>(decommissionedProfile).sourceAssetStatuses[0]?.status,
  "missing",
);

const missingProfile = handleAdminDiagnosticsRequest(
  { method: "GET", url: "/v1/admin/profiles/registry/missing-profile" },
  { diagnostics, profileRegistry },
);
assert.equal(missingProfile.status, 404);
assert.equal(missingProfile.body.ok, false);

const memorySpaces = handleAdminDiagnosticsRequest(
  { method: "GET", url: "/v1/admin/diagnostics/memory-spaces" },
  {
    diagnostics,
    memorySpaces: {
      generatedAt: now,
      items: [
        {
          descriptor: profileDenseDescriptor(),
          compatibility: {
            spaceId: "profile_dense",
            status: "compatible",
            backingStore: "profile_memories",
            nativeMethods: [
              "listProfileMemory",
              "getProfileMemory",
              "addProfileMemory",
              "replaceProfileMemory",
              "removeProfileMemory",
            ],
            denseProfileMemoryCaps: {
              maxRecordsPerProfile: 64,
              maxKeyBytes: 128,
              maxContentBytes: 8192,
            },
            conflictBehavior: "expected_revision",
            promptInjectionBehavior: "summary_context",
            toolModeBehavior: "read_write when selected, otherwise read_only",
            notes: ["existing dense profile memory API compatibility wrapper"],
          },
        },
      ],
    },
  },
);
assert.equal(memorySpaces.status, 200);
const memorySpaceData = okData<{
  items: Array<{
    descriptor: MemorySpaceDescriptor;
    compatibility: { backingStore: string; conflictBehavior: string };
  }>;
}>(memorySpaces);
assert.equal(memorySpaceData.items[0]?.descriptor.space_id, "profile_dense");
assert.deepEqual(memorySpaceData.items[0]?.descriptor.operations, [
  "read",
  "list",
  "add",
  "replace",
  "remove",
]);
assert.equal(
  memorySpaceData.items[0]?.compatibility.backingStore,
  "profile_memories",
);
assert.equal(
  memorySpaceData.items[0]?.compatibility.conflictBehavior,
  "expected_revision",
);

const rootDiagnostics = handleAdminDiagnosticsRequest(
  { method: "GET", url: "/v1/admin/diagnostics" },
  { diagnostics },
);
const diagnosticsText = JSON.stringify(
  okData<{ overview: unknown }>(rootDiagnostics),
);
assert.match(diagnosticsText, /previous-response-chain/);
assert.match(diagnosticsText, /provider_state_expired/);
assert.doesNotMatch(diagnosticsText, /responseId|rawJson|encrypted_content/);

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
      memorySpaces: memorySpaceData.items.length,
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

function profileDenseDescriptor(): MemorySpaceDescriptor {
  return {
    space_id: "profile_dense" as never,
    schema_version: 1,
    module_id: "runtime_memory",
    description: "Existing dense profile memory.",
    record_shapes: [
      {
        shape_id: "profile_dense_item" as never,
        version: 1,
        description: "Dense profile memory record.",
        fields: [
          {
            field_name: "key",
            field_type: "string",
            required: true,
            description: "Memory key.",
          },
          {
            field_name: "content",
            field_type: "markdown",
            required: true,
            description: "Memory content.",
          },
          {
            field_name: "revision",
            field_type: "integer",
            required: true,
            description: "Expected revision token.",
          },
        ],
      },
    ],
    scope_model: {
      allowed_scopes: ["profile", "user"],
      primary_scope: "profile",
    },
    visibility_model: "profile_local",
    retrieval_strategies: ["direct_lookup", "query_search"],
    indexing: {
      required_capabilities: [
        "profile_target_key_lookup",
        "expected_revision_conflicts",
      ],
      optional_capabilities: [
        "profile_scoped_listing",
        "cap_max_records_per_profile_64",
        "cap_max_key_bytes_128",
        "cap_max_content_bytes_8192",
      ],
    },
    prompt_policy: "summary_context",
    write_policy: {
      default_mode: "direct_write",
      operation_policies: [
        {
          operation: "add",
          governance_mode: "direct_write",
          requires_expected_revision: false,
        },
        {
          operation: "replace",
          governance_mode: "direct_write",
          requires_expected_revision: true,
        },
        {
          operation: "remove",
          governance_mode: "direct_write",
          requires_expected_revision: true,
        },
      ],
    },
    operations: ["read", "list", "add", "replace", "remove"],
    provenance_policy: {
      required_evidence: ["wake"],
      source_required: false,
      rationale_required: false,
    },
    retention_policy: "manual_only",
    conflict_policy: "expected_revision",
    diagnostics: {
      expose_catalog: true,
      expose_record_counts: true,
      expose_policy_decisions: true,
    },
    export_import: {
      export_supported: true,
      import_supported: true,
      import_governance_mode: "manual_review",
    },
  };
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
