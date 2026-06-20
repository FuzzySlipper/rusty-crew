import assert from "node:assert/strict";
import {
  createSimulatedMcpTransportFactory,
  McpSurfaceManager,
} from "@rusty-crew/adapter-mcp";
import type {
  AdapterId,
  AgentId,
  McpBindingRecord,
  ProfileId,
  SessionHandle,
  SessionId,
  SessionState,
} from "@rusty-crew/contracts";
import {
  AgentActivityObservationProducer,
  buildAdapterDiagnosticsProjection,
  buildReadOnlySlashCommandResponse,
  buildRuntimeDiagnosticsProjection,
  createMemoryAdminControlAuditSink,
  createMemoryAgentActivityObservationSink,
  createMemoryNewSessionLifecycleAuditSink,
  createMemoryReloadMcpLifecycleAuditSink,
  createNewSessionLifecycleExecutor,
  createReloadMcpControlExecutor,
  handleAdminControlRequest,
  routeSlashCommand,
  type AdminControlResponse,
  type AdminRouteResult,
  type RuntimeCounterSummary,
  type SlashCommandRouteResult,
  type SlashCommandSession,
} from "./index.js";

const now = "2026-06-20T19:00:00.000Z";
const adapterId = "mcp-main" as AdapterId;
let currentSessionId = "session-alpha";
let currentMcpBinding = mcpBinding("mcp-alpha", currentSessionId);
let tick = 0;
const mcpManager = new McpSurfaceManager({
  transports: [createSimulatedMcpTransportFactory("stdio")],
  now: () => `2026-06-20T19:00:${String(tick++).padStart(2, "0")}.000Z`,
});
await mcpManager.connect(currentMcpBinding);
await mcpManager.connect(mcpBinding("mcp-beta", "session-beta"));

const adminAudit = createMemoryAdminControlAuditSink();
const newLifecycleAudit = createMemoryNewSessionLifecycleAuditSink();
const reloadAudit = createMemoryReloadMcpLifecycleAuditSink();
const observationSink = createMemoryAgentActivityObservationSink();
const observationProducer = new AgentActivityObservationProducer({
  sink: observationSink,
  required: true,
});
const projectedResponses: Array<{
  bindingId: string;
  commandName: string;
  title: string;
}> = [];
let llmPrompted = 0;

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

const newSessionExecutor = createNewSessionLifecycleExecutor({
  loadTemplate: () => ({
    agentId: "agent-alpha",
    profileId: "prime",
    kind: "full",
    channelBindingId: "binding-alpha",
    channelId: "crew-room",
  }),
  generateSessionId: () => "session-alpha-new",
  archiveSession: () => undefined,
  createSession: () => undefined,
  rebindChannel(input) {
    currentSessionId = input.newSessionId;
    currentMcpBinding = {
      ...currentMcpBinding,
      sessionId: input.newSessionId as SessionId,
    };
  },
  auditSink: newLifecycleAudit,
  observationProducer,
  observationIdentity({ template, sessionId }) {
    return {
      profile: template.profileId as ProfileId,
      instance_id: template.agentId as AgentId,
      session_key: sessionId as SessionId,
    };
  },
  now: () => now,
});
const reloadMcpExecutor = createReloadMcpControlExecutor({
  resolveBinding(sessionId) {
    return sessionId === currentSessionId ? currentMcpBinding : undefined;
  },
  manager: mcpManager,
  discoveryClient: {
    listTools: () => [
      { name: "stable", description: "Stable tool.", inputSchema: true },
      {
        name: "new_tool",
        description: "New tool.",
        inputSchema: { type: "object", properties: {} },
      },
    ],
  },
  catalogId: (binding) => `mcp:${binding.toolProfileKey}`,
  previousToolNames: () => ["den_old_tool", "den_stable"],
  inventoryRequest: (binding) => ({
    requestedToolsets: [`mcp:${binding.toolProfileKey}`],
  }),
  auditSink: reloadAudit,
  observationProducer,
  observationIdentity({ binding }) {
    return {
      profile: binding.profileId,
      instance_id: binding.agentId,
      session_key: binding.sessionId,
    };
  },
  now: () => `2026-06-20T19:01:${String(tick++).padStart(2, "0")}.000Z`,
});

for (const command of ["/help", "/status", "/session", "/new", "/reload-mcp"]) {
  await processSlashCommand(command);
}

assert.equal(llmPrompted, 0);
assert.deepEqual(
  projectedResponses.map((response) => response.commandName),
  ["help", "status", "session", "new", "reload-mcp"],
);
assert.equal(
  projectedResponses.every(
    (response) => response.bindingId === "binding-alpha",
  ),
  true,
);
assert.equal(currentSessionId, "session-alpha-new");
assert.deepEqual(
  newLifecycleAudit.events.map((event) => event.phase),
  [
    "template_loaded",
    "archive_started",
    "archived",
    "create_started",
    "created",
    "binding_rebind_started",
    "binding_rebound",
  ],
);
assert.deepEqual(
  reloadAudit.events.map((event) => event.phase),
  ["reload_started", "reloaded"],
);
assert.equal(mcpManager.diagnostics("mcp-beta")?.status, "active");
assert.equal(
  observationSink.events.some(
    (event) => event.event_type === "agent_session_stopped",
  ),
  true,
);
assert.equal(
  observationSink.events.some(
    (event) => event.event_type === "adapter_recovered",
  ),
  true,
);
assert.equal(adminAudit.events.length, 4);

console.log(
  JSON.stringify(
    {
      projected: projectedResponses.map((response) => response.commandName),
      llmPrompted,
      currentSessionId,
      newLifecycleEvents: newLifecycleAudit.events.length,
      reloadEvents: reloadAudit.events.length,
      observationEvents: observationSink.events.length,
      betaMcpStatus: mcpManager.diagnostics("mcp-beta")?.status,
    },
    null,
    2,
  ),
);

async function processSlashCommand(text: string): Promise<void> {
  const session = slashSession(currentSessionId);
  const routed = routeSlashCommand({
    text,
    session,
    actor: { id: "human-alpha" },
    source: {
      adapterId: "den-channels",
      bindingId: "binding-alpha",
      channelId: "crew-room",
      messageId: `message-${text.slice(1)}`,
    },
  });

  if (routed.kind === "pass_through") {
    llmPrompted += 1;
    return;
  }

  if (
    routed.status === "ok" &&
    (routed.commandName === "help" ||
      routed.commandName === "status" ||
      routed.commandName === "session")
  ) {
    const response = buildReadOnlySlashCommandResponse(routed.commandName, {
      diagnostics: diagnosticsForSession(session),
      session,
    });
    projectResponse(routed, response.title);
    return;
  }

  if (routed.status === "ok" && routed.controlRequest) {
    const control = await handleAdminControlRequest(
      {
        method: "POST",
        url: controlUrl(routed.controlRequest.commandName, session.sessionId),
        headers: {
          authorization: "Bearer control-token",
          "x-rusty-crew-operator": "operator-alpha",
        },
        body: {
          reason: routed.controlRequest.reason,
          reasonCode: routed.controlRequest.reasonCode,
        },
      },
      {
        auth: { bearerToken: "control-token" },
        executor: {
          newSession: newSessionExecutor,
          reloadMcp: reloadMcpExecutor,
        },
        auditSink: adminAudit,
        observationProducer,
        observationIdentity: {
          profile: session.profileId,
          instance_id: session.agentId,
          session_key: session.sessionId,
        },
        now: () => now,
      },
    );
    const data = okData<AdminControlResponse>(control);
    assert.equal(data.outcome.status, "completed");
    projectResponse(routed, data.outcome.summary);
  }
}

function diagnosticsForSession(session: SlashCommandSession) {
  return buildRuntimeDiagnosticsProjection({
    now,
    runtimeSummary: counters,
    sessions: [
      runtimeSession(session.sessionId, session.agentId, session.profileId),
    ],
    delegatedSessions: [],
    queues: { pending: 0, expired: 0 },
    persistence: { searchHealthy: true },
    adapters: buildAdapterDiagnosticsProjection({
      now,
      channelBindings: [
        {
          bindingId: "binding-alpha",
          adapterId: "den-channel-main" as AdapterId,
          provider: "den_channels",
          agentId: session.agentId as AgentId,
          sessionId: session.sessionId as SessionId,
          profileId: session.profileId as ProfileId,
          externalChannelId: "crew-room",
          status: "active",
        },
      ],
      channelActivity: [
        {
          bindingId: "binding-alpha",
          adapterId: "den-channel-main" as AdapterId,
          membershipStatus: "joined",
          presenceStatus: "idle",
          subscriptionStatus: "active",
          stale: false,
        },
      ],
      mcpBindings: [currentMcpBinding],
      mcpSurfaces: [mcpManager.diagnostics(currentMcpBinding.bindingId)!],
    }),
    observation: { enabled: true, writerAvailable: true },
  });
}

function projectResponse(
  routed: Extract<SlashCommandRouteResult, { kind: "intercepted" }>,
  title: string,
): void {
  projectedResponses.push({
    bindingId: "binding-alpha",
    commandName: routed.commandName,
    title,
  });
}

function controlUrl(commandName: string, sessionId: string): string {
  if (commandName === "new_session") {
    return `/v1/admin/control/sessions/${sessionId}/new`;
  }
  if (commandName === "reload_mcp") {
    return `/v1/admin/control/mcp/${sessionId}/reload`;
  }
  throw new Error(`unsupported control ${commandName}`);
}

function slashSession(sessionId: string): SlashCommandSession {
  return {
    sessionId,
    agentId: "agent-alpha",
    profileId: "prime",
    kind: "full",
  };
}

function runtimeSession(
  sessionId: string,
  agentId: string,
  profileId: string,
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
    status: "active",
    brainTurnCount: 1,
    createdAt: "2026-06-20T18:00:00.000Z",
    lastActiveAt: now,
  };
}

function mcpBinding(bindingId: string, sessionId: string): McpBindingRecord {
  return {
    bindingId,
    adapterId,
    agentId: "agent-alpha" as AgentId,
    sessionId: sessionId as SessionId,
    profileId: "prime" as ProfileId,
    serverNames: ["den"],
    endpointRef: `config://mcp/${bindingId}`,
    transport: "stdio",
    toolProfileKey: "prime-mcp",
    status: "active",
    diagnostics: {},
  };
}

function okData<T>(result: AdminRouteResult): T {
  assert.equal(result.body.ok, true);
  if (!result.body.ok) throw new Error("expected admin route success");
  return result.body.data as T;
}
