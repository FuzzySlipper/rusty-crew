import assert from "node:assert/strict";
import type {
  AgentId,
  ProfileId,
  SessionHandle,
  SessionId,
  SessionState,
} from "@rusty-crew/contracts";
import {
  buildRuntimeDiagnosticsProjection,
  buildRuntimeHealthProjection,
  buildToolRegistryDiagnostics,
  issueDomain,
  type RuntimeCounterSummary,
} from "./index.js";

const now = "2026-06-20T13:00:00.000Z";
const counters: RuntimeCounterSummary = {
  brainTurns: 2,
  wakes: 2,
  toolCalls: 1,
  toolErrors: 0,
  delegationsCreated: 0,
  delegationsCompleted: 0,
  delegationsFailed: 0,
  delegationsTimedOut: 0,
  delegationsCancelled: 0,
  messages: 3,
  completions: 1,
  queueExpirations: 0,
};

const diagnostics = buildRuntimeDiagnosticsProjection({
  now,
  runtimeSummary: counters,
  sessions: [session("session-alpha", "agent-alpha", "prime", "active")],
  delegatedSessions: [],
  queues: { pending: 1, expired: 0 },
  persistence: {
    schemaVersion: 1,
    migrationCount: 1,
    databaseBytes: 128,
    maxDatabaseBytes: 1024,
    searchHealthy: true,
  },
  tools: [buildToolRegistryDiagnostics({ catalogId: "default-local-tools" })],
  observation: {
    enabled: true,
    writerAvailable: false,
    lastError: "observation endpoint unavailable",
  },
});
const projection = buildRuntimeHealthProjection(diagnostics);

assert.equal(projection.liveness.ok, true);
assert.equal(projection.liveness.health, "ok");
assert.equal(projection.readiness.ok, true);
assert.equal(projection.readiness.ready, true);
assert.equal(projection.readiness.degraded, true);
assert.deepEqual(projection.readiness.blockingReasonCodes, []);
assert.equal(projection.degradedStatus.external.health, "degraded");
assert.equal(projection.degradedStatus.internal.health, "ok");
assert.equal(
  projection.degradedStatus.external.reasonCodes.includes(
    "observation_unavailable",
  ),
  true,
);
assert.equal(
  projection.metrics.find(
    (metric) => metric.name === "rusty_crew_observation_writer_available",
  )?.value,
  0,
);

const missingDiagnostics = buildRuntimeHealthProjection(
  buildRuntimeDiagnosticsProjection({ now }),
);
assert.equal(missingDiagnostics.liveness.ok, true);
assert.equal(missingDiagnostics.readiness.ok, false);
assert.equal(
  missingDiagnostics.readiness.blockingReasonCodes.includes(
    "diagnostics_missing",
  ),
  true,
);

const invalidTools = buildRuntimeHealthProjection(
  buildRuntimeDiagnosticsProjection({
    now,
    runtimeSummary: counters,
    sessions: [session("session-alpha", "agent-alpha", "prime", "active")],
    delegatedSessions: [],
    tools: [
      buildToolRegistryDiagnostics({
        catalogId: "broken-tools",
        entries: [tool("read_file", "first"), tool("read_file", "second")],
      }),
    ],
  }),
);
assert.equal(invalidTools.readiness.ok, false);
assert.equal(invalidTools.degradedStatus.internal.health, "blocked");
assert.equal(
  invalidTools.readiness.blockingReasonCodes.includes("tool_registry_invalid"),
  true,
);

assert.equal(
  issueDomain({
    code: "degraded_adapter",
    severity: "degraded",
    source: "adapters.channels",
    message: "channel degraded",
  }),
  "external",
);

console.log(
  JSON.stringify(
    {
      liveness: projection.liveness.ok,
      readiness: projection.readiness.ok,
      degraded: projection.degradedStatus.reasonCodes,
      metrics: projection.metrics.length,
      missingReady: missingDiagnostics.readiness.ok,
      invalidToolsReady: invalidTools.readiness.ok,
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
): SessionState {
  return {
    handle: 1 as SessionHandle,
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
    lastActiveAt: "2026-06-20T12:59:50.000Z",
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
