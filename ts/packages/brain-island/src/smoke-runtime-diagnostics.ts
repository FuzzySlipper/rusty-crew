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
  buildRuntimeDiagnosticsProjection,
  buildToolRegistryDiagnostics,
  type RuntimeCounterSummary,
} from "./index.js";

const now = "2026-06-20T12:30:00.000Z";
const runtimeSummary: RuntimeCounterSummary = {
  brainTurns: 8,
  wakes: 9,
  toolCalls: 3,
  toolErrors: 1,
  delegationsCreated: 2,
  delegationsCompleted: 1,
  delegationsFailed: 1,
  delegationsTimedOut: 0,
  delegationsCancelled: 0,
  messages: 4,
  completions: 2,
  queueExpirations: 1,
};
const activeSession = session("session-alpha", "agent-alpha", "prime", {
  status: "active",
  lastActiveAt: "2026-06-20T12:29:50.000Z",
});
const staleSession = session("session-beta", "agent-beta", "review", {
  status: "idle",
  lastActiveAt: "2026-06-20T11:00:00.000Z",
});
const adapterDiagnostics = buildAdapterDiagnosticsProjection({
  now,
  channelBindings: [
    {
      bindingId: "channel-alpha",
      adapterId: "den-channel-main" as never,
      provider: "den_channels",
      agentId: "agent-alpha" as AgentId,
      sessionId: "session-alpha" as SessionId,
      profileId: "prime" as ProfileId,
      externalChannelId: "crew-room",
      status: "active",
    },
  ],
  channelActivity: [
    {
      bindingId: "channel-alpha",
      adapterId: "den-channel-main" as never,
      membershipStatus: "joined",
      presenceStatus: "idle",
      subscriptionStatus: "active",
      stale: false,
    },
  ],
  mcpBindings: [
    {
      bindingId: "mcp-beta",
      adapterId: "mcp-main" as never,
      agentId: "agent-beta" as AgentId,
      sessionId: "session-beta" as SessionId,
      profileId: "review" as ProfileId,
      serverNames: ["beta"],
      endpointRef: "config://mcp/beta",
      transport: "stdio",
      toolProfileKey: "review-mcp",
      status: "active",
      diagnostics: {},
    },
  ],
  mcpSurfaces: [
    {
      bindingId: "mcp-beta",
      status: "degraded",
      transport: "stdio",
      serverNames: ["beta"],
      endpointRef: "config://mcp/beta",
      toolProfileKey: "review-mcp",
      reconnectAttempts: 2,
      optional: false,
      lastError: "MCP reload failed",
    },
  ],
});
const validTools = buildToolRegistryDiagnostics({
  catalogId: "default-local-tools",
});
const invalidTools = buildToolRegistryDiagnostics({
  catalogId: "broken-tools",
  entries: [
    {
      name: "read_file",
      description: "Duplicate tool.",
      category: "local",
      toolsets: ["local_code_read"],
      surfaces: ["brain"],
      safety: ["read_only"],
      outputShape: "duplicate",
      version: "1.0.0",
    },
    {
      name: "read_file",
      description: "Duplicate tool.",
      category: "local",
      toolsets: ["local_code_read"],
      surfaces: ["brain"],
      safety: ["read_only"],
      outputShape: "duplicate",
      version: "1.0.0",
    },
  ],
});

const degraded = buildRuntimeDiagnosticsProjection({
  now,
  runtimeSummary,
  sessions: [activeSession, staleSession],
  delegatedSessions: [
    {
      session: staleSession,
      parentSessionId: "session-alpha" as SessionId,
      runId: "run-beta" as never,
      runStatus: "blocked",
      terminal: false,
    },
  ],
  queues: {
    pending: 40,
    expired: 2,
    oldestPendingAgeMs: 120_000,
    maxPending: 32,
    maxOldestPendingAgeMs: 60_000,
  },
  persistence: {
    schemaVersion: 12,
    migrationCount: 12,
    databaseBytes: 2_000,
    maxDatabaseBytes: 1_000,
    tableCounts: { events: 100 },
    tableCountThresholds: { events: 50 },
    searchHealthy: true,
  },
  adapters: adapterDiagnostics,
  tools: [validTools, invalidTools],
  observation: {
    enabled: true,
    writerAvailable: false,
    lastError: "observation sink unavailable",
  },
  recentErrors: [
    {
      source: "runtime.scheduler",
      message: "scheduler checkpoint failed",
      observedAt: now,
    },
  ],
});

assert.equal(degraded.health, "blocked");
assert.equal(degraded.degraded, true);
assert.equal(degraded.summary.sessions, 2);
assert.equal(degraded.summary.blockedDelegations, 1);
assert.equal(degraded.summary.pendingQueueItems, 40);
assert.equal(degraded.summary.toolErrors, 1);
assert.equal(degraded.runtime.sessions[1]?.stale, true);
assert.equal(degraded.queues?.backlog, true);
assert.equal(degraded.persistence?.pressure, true);
assert.equal(
  degraded.tools.find((tool) => tool.catalogId === "broken-tools")?.invalid,
  true,
);
assert.equal(degraded.observation?.degraded, true);
assert.deepEqual(
  [
    "blocked_dependency",
    "expired_queue_items",
    "mcp_reload_failed",
    "observation_unavailable",
    "persistence_pressure",
    "queue_backlog",
    "recent_runtime_error",
    "stale_session",
    "tool_registry_invalid",
  ].every((code) => degraded.reasonCodes.includes(code as never)),
  true,
);

const healthy = buildRuntimeDiagnosticsProjection({
  now,
  runtimeSummary: { ...runtimeSummary, toolErrors: 0, queueExpirations: 0 },
  sessions: [activeSession],
  delegatedSessions: [],
  queues: { pending: 0, expired: 0 },
  persistence: {
    schemaVersion: 12,
    migrationCount: 12,
    databaseBytes: 200,
    maxDatabaseBytes: 1_000,
    searchHealthy: true,
  },
  tools: [validTools],
  observation: { enabled: false, writerAvailable: false },
});

assert.equal(healthy.health, "ok");
assert.deepEqual(healthy.reasonCodes, ["ok"]);

const missingInputs = buildRuntimeDiagnosticsProjection({ now });
assert.equal(missingInputs.health, "degraded");
assert.equal(missingInputs.reasonCodes.includes("diagnostics_missing"), true);

console.log(
  JSON.stringify(
    {
      health: degraded.health,
      reasonCodes: degraded.reasonCodes,
      healthy: healthy.health,
      missing: missingInputs.reasonCodes,
    },
    null,
    2,
  ),
);

function session(
  sessionId: string,
  agentId: string,
  profileId: string,
  options: Pick<SessionState, "status" | "lastActiveAt">,
): SessionState {
  return {
    handle: Number(sessionId.endsWith("alpha") ? 1 : 2) as SessionHandle,
    sessionId: sessionId as SessionId,
    agentId: agentId as AgentId,
    profileId: profileId as ProfileId,
    kind: "full",
    resourceLimits: {},
    toolProfile: {
      tools: [
        {
          name: "read_file",
          description: "Read a file.",
        },
      ],
    },
    status: options.status,
    brainTurnCount: 1,
    createdAt: "2026-06-20T10:00:00.000Z",
    lastActiveAt: options.lastActiveAt,
  };
}
