import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  ChannelBindingActivityTracker,
  providerRefsFromBinding,
} from "@rusty-crew/adapter-den";
import {
  createSimulatedMcpTransportFactory,
  McpSurfaceManager,
} from "@rusty-crew/adapter-mcp";
import type {
  AdapterId,
  AgentId,
  ChannelBindingRecord,
  McpBindingRecord,
  ProfileId,
  SessionId,
} from "@rusty-crew/contracts";
import { loadNativeBridge } from "@rusty-crew/native-bridge";
import {
  buildAdapterDiagnosticsProjection,
  type ChannelProjectionFailureRecord,
} from "./index.js";

const adapterId = "den-channel-main" as AdapterId;
const mcpAdapterId = "mcp-ts-main" as AdapterId;
const channelBindings: ChannelBindingRecord[] = [
  channelBinding("binding-alpha", "agent-alpha", "session-alpha", "prime"),
  channelBinding("binding-beta", "agent-beta", "session-beta", "review"),
];
const mcpBindings: McpBindingRecord[] = [
  mcpBinding("mcp-alpha", "agent-alpha", "session-alpha", "prime", "stdio"),
  mcpBinding("mcp-beta", "agent-beta", "session-beta", "review", "websocket"),
];

const activity = new ChannelBindingActivityTracker();
for (const binding of channelBindings) {
  activity.upsertMembership({
    bindingId: binding.bindingId,
    adapterId: binding.adapterId,
    providerRefs: providerRefsFromBinding(binding),
    externalUserId: binding.externalUserId ?? `${binding.agentId}-external`,
    agentId: binding.agentId,
    profileId: binding.profileId,
    status: "joined",
    observedAt: "2026-06-20T11:10:00.000Z",
  });
  activity.observePresence({
    bindingId: binding.bindingId,
    adapterId: binding.adapterId,
    providerRefs: providerRefsFromBinding(binding),
    agentId: binding.agentId,
    sessionId: binding.sessionId,
    status: "idle",
    observedAt: "2026-06-20T11:10:00.000Z",
    expiresAt: "2026-06-20T11:12:00.000Z",
  });
  activity.upsertSubscription({
    bindingId: binding.bindingId,
    adapterId: binding.adapterId,
    providerRefs: providerRefsFromBinding(binding),
    transportKind: "rust_event_subscription",
    status: "active",
    observedAt: "2026-06-20T11:10:01.000Z",
  });
}

const projectionFailures: ChannelProjectionFailureRecord[] = [
  {
    bindingId: "binding-beta",
    kind: "activity",
    degradedReason: "projection sink unavailable",
    observedAt: "2026-06-20T11:10:05.000Z",
  },
];

const mcpManager = new McpSurfaceManager({
  transports: [createSimulatedMcpTransportFactory("stdio")],
  now: () => "2026-06-20T11:10:10.000Z",
});
await mcpManager.connect(mcpBindings[0]!);
await mcpManager.connect(mcpBindings[1]!);

const diagnostics = buildAdapterDiagnosticsProjection({
  now: "2026-06-20T11:10:15.000Z",
  channelBindings,
  channelActivity: activity.diagnostics("2026-06-20T11:10:15.000Z"),
  channelProjectionFailures: projectionFailures,
  mcpBindings,
  mcpSurfaces: mcpManager.diagnostics(),
  mcpReloadHistory: [
    {
      bindingId: "mcp-beta",
      sessionId: "session-beta",
      profileId: "review" as ProfileId,
      status: "degraded",
      requestedBy: "smoke",
      reason: "prove degraded surface stays scoped",
      startedAt: "2026-06-20T11:10:02.000Z",
      finishedAt: "2026-06-20T11:10:03.000Z",
      durationMs: 1_000,
      discoveryIssueCount: 1,
      collisionCount: 1,
      optionalServerFailures: ["optional MCP server did not connect"],
      toolDiff: {
        oldTools: [],
        newTools: [],
        addedTools: [],
        removedTools: [],
        unchangedTools: [],
      },
      degradedReason: "MCP registry validation failed after reload",
    },
  ],
});

assert.equal(diagnostics.degraded, true);
assert.equal(diagnostics.channels.totalBindings, 2);
assert.equal(diagnostics.channels.activeBindings, 1);
assert.equal(diagnostics.channels.degradedBindings, 1);
assert.equal(diagnostics.channels.droppedProjections, 1);
assert.equal(
  diagnostics.channels.bindings.find(
    (item) => item.bindingId === "binding-alpha",
  )?.status,
  "active",
);
assert.equal(
  diagnostics.channels.bindings.find(
    (item) => item.bindingId === "binding-beta",
  )?.lastError,
  "projection sink unavailable",
);
assert.equal(diagnostics.mcp.totalSurfaces, 2);
assert.equal(diagnostics.mcp.activeSurfaces, 1);
assert.equal(diagnostics.mcp.degradedSurfaces, 1);
assert.equal(diagnostics.mcp.collisionCount, 1);
assert.equal(
  diagnostics.mcp.surfaces.find((item) => item.bindingId === "mcp-alpha")
    ?.toolProfileKey,
  "prime-mcp",
);
assert.equal(
  diagnostics.mcp.surfaces.find((item) => item.bindingId === "mcp-beta")
    ?.status,
  "degraded",
);

const engineDataDir = mkdtempSync(
  join(tmpdir(), "rusty-crew-adapter-diagnostics-engine-"),
);
const native = await loadNativeBridge();
const engine = await native.initializeEngine({
  engineDataDir,
  clock: { fixed: "2026-06-20T11:11:00Z" },
  defaultTurnBudget: 3,
  defaultIdleTimeoutMs: 1_000,
});

try {
  await native.createSession({
    sessionId: "session-alpha",
    agentId: "agent-alpha",
    profileId: "prime",
    kind: "full",
  });
  await native.createSession({
    sessionId: "session-beta",
    agentId: "agent-beta",
    profileId: "review",
    kind: "full",
  });

  const routeReceipt = await native.routeAgentMessage(
    "agent-alpha",
    "agent-beta",
    "internal routing continues after adapter degradation",
  );
  assert.equal(routeReceipt.accepted, true);
  assert.equal(await native.diagnosticCountRows("agent_messages"), 1);
} finally {
  await native.shutdownEngine({ engine, drainTimeoutMs: 1_000 });
  rmSync(engineDataDir, { force: true, recursive: true });
}

console.log(
  JSON.stringify(
    {
      channelBindings: diagnostics.channels.totalBindings,
      channelDegraded: diagnostics.channels.degradedBindings,
      mcpSurfaces: diagnostics.mcp.totalSurfaces,
      mcpDegraded: diagnostics.mcp.degradedSurfaces,
      issues: diagnostics.issues.length,
    },
    null,
    2,
  ),
);

function channelBinding(
  bindingId: string,
  agentId: string,
  sessionId: string,
  profileId: string,
): ChannelBindingRecord {
  return {
    bindingId,
    adapterId,
    provider: "den_channels",
    agentId: agentId as AgentId,
    sessionId: sessionId as SessionId,
    profileId: profileId as ProfileId,
    externalChannelId: "crew-room",
    externalThreadId: "thread-1",
    externalUserId: `${agentId}-external`,
    status: "active",
  };
}

function mcpBinding(
  bindingId: string,
  agentId: string,
  sessionId: string,
  profileId: string,
  transport: McpBindingRecord["transport"],
): McpBindingRecord {
  return {
    bindingId,
    adapterId: mcpAdapterId,
    agentId: agentId as AgentId,
    sessionId: sessionId as SessionId,
    profileId: profileId as ProfileId,
    serverNames: [profileId],
    endpointRef: `config://mcp/${profileId}`,
    transport,
    toolProfileKey: `${profileId}-mcp`,
    discoveredToolRevision: `${profileId}-rev`,
    status: "active",
    diagnostics: {},
  };
}
