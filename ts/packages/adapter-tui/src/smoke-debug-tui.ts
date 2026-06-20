import assert from "node:assert/strict";
import {
  loadDebugTuiState,
  reduceDebugTuiState,
  renderDebugTui,
} from "./index.js";
import type {
  DebugTuiApiClient,
  DebugTuiDirectDebugContext,
  DebugTuiPage,
  DebugTuiRecentEvent,
} from "./index.js";

const session = {
  sessionId: "tui-session",
  agentId: "tui-agent",
  profileId: "tui-profile",
  kind: "full",
  resourceLimits: {
    workdir: "/home/dev/rusty-crew",
  },
  toolProfile: {
    tools: [
      {
        name: "read_file",
        description: "Read files.",
      },
    ],
  },
  status: "active",
  brainTurnCount: 5,
  createdAt: "2026-06-20T00:00:00Z",
  lastActiveAt: "2026-06-20T00:02:00Z",
};
const diagnostics = {
  generatedAt: "2026-06-20T00:03:00Z",
  health: "degraded",
  degraded: true,
  reasonCodes: ["observation_unavailable"],
  summary: {
    activeSessions: 1,
    idleSessions: 0,
    archivedSessions: 0,
    delegatedSessions: 0,
    blockedDelegations: 0,
    pendingQueueItems: 2,
    expiredQueueItems: 1,
  },
  queues: {
    pending: 2,
    expired: 1,
    oldestPendingAgeMs: 1200,
  },
  persistence: {
    schemaVersion: 1,
    databaseBytes: 1024,
    searchHealthy: true,
  },
  observation: {
    enabled: true,
    writerAvailable: false,
    degraded: true,
    lastError: "writer offline",
  },
  issues: [
    {
      code: "observation_unavailable",
      severity: "degraded",
      message: "observation writer offline",
    },
  ],
} as const;
const health = {
  readiness: {
    ready: false,
  },
  metrics: [
    {
      name: "rusty_crew_runtime_health_degraded",
      value: 1,
    },
  ],
};
const directContext = {
  generatedAt: "2026-06-20T00:03:00Z",
  source: "direct_debug",
  session: {
    sessionId: session.sessionId,
    agentId: session.agentId,
    profileId: session.profileId,
    kind: session.kind,
    status: session.status,
    brainTurnCount: session.brainTurnCount,
    createdAt: session.createdAt,
    lastActiveAt: session.lastActiveAt,
    toolCount: session.toolProfile.tools.length,
    workdir: session.resourceLimits.workdir,
  },
  selectedTools: session.toolProfile.tools,
  context: {
    rawPromptIncluded: false,
    rawPromptDeniedReason: "raw prompt text disabled",
    systemPrompt: {
      present: true,
      chars: 42,
      lines: 1,
      sha256: "abc",
    },
    instructions: {
      present: true,
      chars: 84,
      lines: 3,
      sha256: "def",
    },
    sections: ["Profile", "Tool Inventory"],
    initialMessages: {
      count: 0,
      totalChars: 0,
    },
    skills: [],
  },
  pendingMessages: [],
  recentEvents: [],
  controls: {
    directTurnInjection: "disabled",
    reason: "read only",
  },
} satisfies DebugTuiDirectDebugContext;
const client = {
  async diagnostics() {
    return {
      overview: diagnostics,
      health,
    };
  },
  async sessions() {
    return page([
      {
        sessionId: session.sessionId,
        agentId: session.agentId,
        profileId: session.profileId,
        kind: session.kind,
        status: session.status,
        toolCount: session.toolProfile.tools.length,
        brainTurnCount: session.brainTurnCount,
        lastActiveAt: session.lastActiveAt,
        stale: false,
      },
    ]);
  },
  async tools() {
    return page([
      {
        catalogId: "default",
        registeredTools: 7,
        selectedTools: 3,
        validationErrors: 0,
        validationWarnings: 0,
        invalid: false,
      },
    ]);
  },
  async mcpSurfaces() {
    return page([
      {
        bindingId: "mcp-tui",
        adapterId: "mcp",
        agentId: session.agentId,
        sessionId: session.sessionId,
        profileId: session.profileId,
        status: "degraded",
        transport: "stdio",
        serverNames: ["tools"],
        toolProfileKey: "tui-profile:tools",
        reconnectAttempts: 1,
        collisionCount: 0,
        discoveryIssueCount: 1,
        optionalServerFailures: ["tools"],
        lastError: "server offline",
      },
    ]);
  },
  async channelBindings() {
    return page([
      {
        bindingId: "channel-tui",
        adapterId: "den",
        agentId: session.agentId,
        sessionId: session.sessionId,
        profileId: session.profileId,
        provider: "den",
        status: "active",
        membershipStatus: "joined",
        presenceStatus: "online",
        subscriptionStatus: "active",
        stalePresence: false,
        droppedProjections: 0,
      },
    ]);
  },
  async observation() {
    return diagnostics.observation ?? null;
  },
  async metrics() {
    return page(health.metrics);
  },
  async recentEvents() {
    return page([
      {
        id: "event-tui",
        createdAt: "2026-06-20T00:03:00Z",
        source: "smoke",
        eventType: "debug.event",
        summary: "Rendered TUI.",
      },
    ] satisfies DebugTuiRecentEvent[]);
  },
  async directDebugContext() {
    return directContext;
  },
} satisfies DebugTuiApiClient;

const loaded = await loadDebugTuiState(client, {
  activeSessionId: session.sessionId,
  includeDirectDebugContext: true,
});
assert.equal(loaded.readOnly, true);
assert.equal(loaded.status, "degraded");
assert.equal(loaded.tabs.includes("context"), true);

const overview = renderDebugTui(loaded, { width: 100, height: 18 });
assert.match(overview, /Rusty Crew Debug \[DEGRADED\]/);
assert.match(overview, /queues: pending=2 expired=1/);
assert.match(overview, /observation writer offline/);

const sessions = reduceDebugTuiState(loaded, "ArrowRight");
assert.equal(sessions.activeTab, "sessions");
const sessionRender = renderDebugTui(sessions, { width: 100, height: 12 });
assert.match(sessionRender, /tui-session/);

const contextState = {
  ...loaded,
  activeTab: "context" as const,
};
const contextRender = renderDebugTui(contextState, { width: 100, height: 12 });
assert.match(contextRender, /raw=false/);
assert.doesNotMatch(contextRender, /system prompt/i);

const quit = reduceDebugTuiState(loaded, "q");
const refresh = reduceDebugTuiState(loaded, "r");
assert.equal(quit.quitRequested, true);
assert.equal(refresh.refreshRequested, true);

console.log(
  JSON.stringify(
    {
      status: loaded.status,
      tabs: loaded.tabs,
      overviewLines: overview.split("\n").length,
      sessionsTab: sessions.activeTab,
      contextRendered: /direct turns: disabled/.test(contextRender),
    },
    null,
    2,
  ),
);

function page<T>(items: T[]): DebugTuiPage<T> {
  return {
    items,
    total: items.length,
    limit: 100,
    offset: 0,
  };
}
