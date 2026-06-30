import assert from "node:assert/strict";
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
  buildAdapterDiagnosticsProjection,
  buildReadOnlySlashCommandResponse,
  buildRuntimeDiagnosticsProjection,
  type RuntimeCounterSummary,
  type SlashCommandSession,
} from "./index.js";

const now = "2026-06-20T18:00:00.000Z";
const sessionContext: SlashCommandSession = {
  sessionId: "session-alpha",
  agentId: "agent-alpha",
  profileId: "prime",
  kind: "full",
};
const counters: RuntimeCounterSummary = {
  brainTurns: 4,
  wakes: 4,
  toolCalls: 2,
  toolErrors: 1,
  delegationsCreated: 0,
  delegationsCompleted: 0,
  delegationsFailed: 0,
  delegationsTimedOut: 0,
  delegationsCancelled: 0,
  messages: 6,
  completions: 1,
  queueExpirations: 1,
};
const diagnostics = buildRuntimeDiagnosticsProjection({
  now,
  runtimeSummary: counters,
  sessions: [
    session("session-alpha", "agent-alpha", "prime", {
      status: "active",
      lastActiveAt: "2026-06-20T17:59:00.000Z",
    }),
  ],
  delegatedSessions: [],
  queues: { pending: 3, expired: 1 },
  persistence: { searchHealthy: true },
  adapters: buildAdapterDiagnosticsProjection({
    now,
    channelBindings: [
      {
        bindingId: "channel-alpha",
        adapterId: "den-channel-main" as AdapterId,
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
        adapterId: "den-channel-main" as AdapterId,
        membershipStatus: "joined",
        presenceStatus: "idle",
        subscriptionStatus: "active",
        stale: false,
      },
    ],
    mcpBindings: [mcpBinding()],
    mcpSurfaces: [
      {
        bindingId: "mcp-alpha",
        status: "active",
        transport: "stdio",
        serverNames: ["den"],
        endpointRef: "config://mcp/alpha",
        toolProfileKey: "prime-mcp",
        reconnectAttempts: 0,
        optional: false,
      },
    ],
  }),
  observation: { enabled: true, writerAvailable: false },
  recentErrors: [
    {
      source: "runtime.scheduler",
      message:
        "scheduler noticed a bounded warning that should be short enough for channels",
      observedAt: now,
    },
  ],
});

const help = buildReadOnlySlashCommandResponse("help", {
  diagnostics,
  session: sessionContext,
});
assert.equal(help.title, "Commands");
assert.equal(help.items?.includes("/status"), true);
assert.match(help.summary, /intercepted before the LLM/);

const status = buildReadOnlySlashCommandResponse("status", {
  diagnostics,
  session: sessionContext,
});
assert.equal(status.title, "Status");
assert.equal(status.fields?.ready, true);
assert.equal(status.fields?.pendingQueueItems, 3);
assert.equal(status.fields?.expiredQueueItems, 1);
assert.equal(status.fields?.mcpDegraded, 0);
assert.equal((status.items?.length ?? 0) <= 6, true);
assert.equal(
  status.items?.some((item) => item.includes("observation writer")),
  true,
);

const currentSession = buildReadOnlySlashCommandResponse("session", {
  diagnostics,
  session: sessionContext,
});
assert.equal(currentSession.title, "Session");
assert.equal(currentSession.fields?.sessionId, "session-alpha");
assert.equal(currentSession.fields?.channelPresence, "idle");
assert.equal(currentSession.fields?.mcpStatus, "active");
assert.equal(currentSession.fields?.tools, 1);

const model = buildReadOnlySlashCommandResponse("model", {
  diagnostics,
  session: sessionContext,
  modelContext: {
    session_id: "session-alpha",
    agent_id: "agent-alpha",
    profile_id: "prime",
    provider: {
      alias: "deepseek-flash",
      status: "active",
      protocol: "chat_completions",
      provider_kind: "openai-compatible",
      base_url_host: "127.0.0.1:18082",
      base_url_redacted: "http://127.0.0.1:18082",
      model_id: "gpt",
      context_window_tokens: 128_000,
      max_output_tokens: 4096,
      temperature: 0.5,
      reasoning_effort: "low",
      reasoning_format: "none",
      revision: 3,
    },
    brain: {
      module: "pi-agent-core",
      backend: "pi-agent-core",
    },
    tools: {
      local_tool_profile_id: "full-agent",
      tool_count: 3,
      requested_toolsets: ["local_code_read"],
      requested_tools: ["todo"],
      mcp_binding_count: 1,
      mcp_active_count: 1,
    },
    context: {
      estimate_quality: "approximate",
      estimate_method: "test",
      context_window_tokens: 128_000,
      estimated_prompt_tokens: 512,
      estimated_remaining_tokens: 127_488,
      max_output_tokens: 4096,
      sampled_event_count: 7,
      sampled_message_count: 2,
    },
    degraded: false,
    diagnostics: [],
  },
});
assert.equal(model.title, "Model");
assert.equal(model.fields?.providerAlias, "deepseek-flash");
assert.equal(model.fields?.modelId, "gpt");
assert.equal(model.fields?.estimatedPromptTokens, 512);
assert.equal(
  model.items?.some((item) => item.includes("provider endpoint")),
  true,
);

const missingSession = buildReadOnlySlashCommandResponse("session", {
  diagnostics,
  session: {
    ...sessionContext,
    sessionId: "session-missing",
  },
});
assert.equal(missingSession.fields?.status, "missing");
assert.match(missingSession.summary, /missing from diagnostics/);

const limitedHelp = buildReadOnlySlashCommandResponse("help", {
  diagnostics,
  session: sessionContext,
  options: { allowedCommands: ["help", "status"] },
});
assert.deepEqual(limitedHelp.items?.slice(0, 2), ["/help", "/status"]);
assert.equal(limitedHelp.items?.includes("/new"), false);

console.log(
  JSON.stringify(
    {
      helpItems: help.items?.length,
      ready: status.fields?.ready,
      issues: status.items?.length,
      channelPresence: currentSession.fields?.channelPresence,
      mcpStatus: currentSession.fields?.mcpStatus,
      modelProvider: model.fields?.providerAlias,
      missing: missingSession.fields?.status,
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
    handle: 1 as SessionHandle,
    sessionId: sessionId as SessionId,
    agentId: agentId as AgentId,
    profileId: profileId as ProfileId,
    kind: "full",
    resourceLimits: {},
    toolProfile: {
      tools: [{ name: "read_file", description: "Read a file." }],
    },
    status: options.status,
    brainTurnCount: 3,
    createdAt: "2026-06-20T17:00:00.000Z",
    lastActiveAt: options.lastActiveAt,
  };
}

function mcpBinding(): McpBindingRecord {
  return {
    bindingId: "mcp-alpha",
    adapterId: "mcp-main" as AdapterId,
    agentId: "agent-alpha" as AgentId,
    sessionId: "session-alpha" as SessionId,
    profileId: "prime" as ProfileId,
    serverNames: ["den"],
    endpointRef: "config://mcp/alpha",
    transport: "stdio",
    toolProfileKey: "prime-mcp",
    status: "active",
    diagnostics: {},
  };
}
