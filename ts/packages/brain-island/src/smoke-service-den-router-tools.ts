import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type {
  AgentEvent as PiAgentEvent,
  AgentMessage as PiAgentMessage,
  AgentOptions as PiAgentOptions,
  AgentTool,
} from "@earendil-works/pi-agent-core";
import type {
  AgentId,
  BrainEvent,
  ProfileId,
  SessionId,
} from "@rusty-crew/contracts";
import { loadNativeBridge } from "@rusty-crew/native-bridge";
import { loadRustyCrewServiceConfig } from "./service-config.js";
import {
  applyRustyCrewRuntimeConfig,
  loadRustyCrewRuntimeConfig,
} from "./service-runtime-config.js";
import type { DenRouterAgentOptions } from "./den-router-agent.js";

const encoder = new TextEncoder();
const abortSignal = new AbortController().signal;
const root = mkdtempSync(
  join(tmpdir(), "rusty-crew-service-den-router-tools-"),
);
const native = await loadNativeBridge();
const originalFetch = globalThis.fetch;
const memoryRequests: Array<{
  url: string;
  authorization?: string;
  body: Record<string, unknown>;
}> = [];
const mcpRequests: Array<{
  url: string;
  method?: string;
  accept?: string;
  sessionId?: string;
  toolProfile?: string | null;
  toolName?: string;
  arguments?: unknown;
}> = [];
const requestedToolNames = [
  "browser_snapshot",
  "channel_readback",
  "counter_reset",
  "curator_execute",
  "den_memory_recall",
  "dense_profile_memory",
  "field_search",
  "git_status",
  "session_search",
  "skills_list",
  "todo",
  "web_search",
].sort();

class ToolCallingFakeAgent {
  private listener?: (event: PiAgentEvent, signal: AbortSignal) => void;

  constructor(
    private readonly options: PiAgentOptions,
    private readonly outputs: Record<string, string>,
    private readonly selectedToolNames: string[],
  ) {
    this.selectedToolNames.push(
      ...(this.options.initialState?.tools ?? []).map((tool) => tool.name),
    );
  }

  subscribe(
    listener: (event: PiAgentEvent, signal: AbortSignal) => void,
  ): () => void {
    this.listener = listener;
    return () => {
      this.listener = undefined;
    };
  }

  async prompt(
    _input: PiAgentMessage | PiAgentMessage[] | string,
  ): Promise<void> {
    await this.emit({ type: "agent_start" } as PiAgentEvent);
    await this.callTool("den_memory_recall", {
      prompt: "What memory guidance is relevant?",
    });
    await this.callTool("field_search", { query: "runner mcp tools" });
    await this.callTool("git_status", {});
    await this.emit({ type: "agent_end", messages: [] } as PiAgentEvent);
  }

  async waitForIdle(): Promise<void> {}

  clearAllQueues(): void {}

  private async callTool(
    name: string,
    params: Record<string, unknown>,
  ): Promise<void> {
    const tool = this.options.initialState?.tools?.find(
      (candidate) => candidate.name === name,
    );
    assert.ok(tool, `${name} should be callable`);
    await this.emit({
      type: "tool_execution_start",
      toolName: name,
    } as PiAgentEvent);
    try {
      const result = await (tool as AgentTool).execute(`${name}-call`, params);
      this.outputs[name] = result.content
        .flatMap((content) =>
          content.type === "text" && typeof content.text === "string"
            ? [content.text]
            : [],
        )
        .join("");
      await this.emit({
        type: "tool_execution_end",
        toolName: name,
        isError: false,
      } as PiAgentEvent);
    } catch (error) {
      await this.emit({
        type: "tool_execution_end",
        toolName: name,
        isError: true,
      } as PiAgentEvent);
      throw error;
    }
  }

  private async emit(event: PiAgentEvent): Promise<void> {
    this.listener?.(event, abortSignal);
  }
}

try {
  globalThis.fetch = (async (input, init) => {
    const url = new URL(String(input));
    const body =
      typeof init?.body === "string"
        ? (JSON.parse(init.body) as Record<string, unknown>)
        : {};
    if (url.hostname === "den-memory.local") {
      memoryRequests.push({
        url: url.toString(),
        authorization: header(init?.headers, "authorization"),
        body,
      });
      assert.equal(url.pathname, "/memory/recall");
      return new Response(
        JSON.stringify({
          ok: true,
          data: {
            memories: [
              {
                id: "service-memory-1",
                summary: "Den memory is available through service config.",
              },
            ],
            total: 1,
          },
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      );
    }
    if (url.hostname === "mcp.local") {
      const method = typeof body.method === "string" ? body.method : undefined;
      const params = isRecord(body.params) ? body.params : {};
      const toolName =
        isRecord(params) && typeof params.name === "string"
          ? params.name
          : undefined;
      mcpRequests.push({
        url: url.toString(),
        method,
        accept: header(init?.headers, "accept"),
        sessionId: header(init?.headers, "mcp-session-id"),
        toolProfile: url.searchParams.get("tool_profile"),
        toolName,
        arguments: isRecord(params) ? params.arguments : undefined,
      });
      if (method === "initialize") {
        return sseJsonRpcResponse(
          body.id,
          {
            protocolVersion: "2024-11-05",
            capabilities: {
              tools: {
                listChanged: true,
              },
            },
            serverInfo: {
              name: "fake-mcp",
              version: "0.1.0",
            },
          },
          "fake-mcp-session",
        );
      }
      if (method === "notifications/initialized") {
        assert.equal(
          header(init?.headers, "mcp-session-id"),
          "fake-mcp-session",
        );
        return new Response(null, { status: 202 });
      }
      if (header(init?.headers, "mcp-session-id") !== "fake-mcp-session") {
        return new Response(
          "A new session can only be created by an initialize request. Include a valid Mcp-Session-Id header for non-initialize requests.",
          { status: 400, headers: { "content-type": "text/plain" } },
        );
      }
      if (method === "tools/list") {
        return jsonRpcResponse(body.id, {
          tools: [
            {
              name: "search",
              description: "Search field MCP profile resources.",
              inputSchema: {
                type: "object",
                properties: {
                  query: { type: "string", minLength: 1 },
                },
                required: ["query"],
              },
            },
          ],
        });
      }
      if (method === "tools/call") {
        return jsonRpcResponse(body.id, {
          content: `mcp:field-mcp:${toolName}`,
          details: {
            sourceToolName: toolName,
          },
        });
      }
    }
    throw new Error(`unexpected fetch to ${url.toString()}`);
  }) satisfies typeof fetch;

  writeRuntimeConfig(root);
  const serviceConfig = loadRustyCrewServiceConfig({
    RUSTY_CREW_DATA_DIR: root,
    RUSTY_CREW_ADMIN_AUTH_MODE: "none",
    RUSTY_CREW_DEN_MEMORY_BASE_URL: "http://den-memory.local",
    RUSTY_CREW_DEN_MEMORY_TOKEN: "memory-token",
    RUSTY_CREW_DEN_MEMORY_RECALL_PATH: "/memory/recall",
    RUSTY_CREW_MCP_REQUEST_TIMEOUT_MS: "10000",
  });
  const runtimeConfig = await loadRustyCrewRuntimeConfig(serviceConfig);
  const engine = await native.initializeEngine({
    engineDataDir: serviceConfig.paths.engineDataDir,
    clock: { fixed: "2026-06-21T12:20:00Z" },
    defaultTurnBudget: 8,
    defaultIdleTimeoutMs: 1_000,
  });
  const outputs: Record<string, string> = {};
  const selectedToolNames: string[] = [];
  const denRouterOptions: DenRouterAgentOptions[] = [];
  try {
    const applyResult = await applyRustyCrewRuntimeConfig({
      serviceConfig,
      runtimeConfig,
      bridge: native,
      createDenRouterAgentFactory: async (options) => {
        denRouterOptions.push(options ?? {});
        return (agentOptions) =>
          new ToolCallingFakeAgent(agentOptions, outputs, selectedToolNames);
      },
    });
    const brain = applyResult.brainHandlesByProfileId["field-profile"];
    assert.ok(brain, "field-profile brain should be registered");
    assert.deepEqual(
      {
        moduleId:
          applyResult.brainDiagnosticsByProfileId["field-profile"]?.moduleId,
        selectedToolCount:
          applyResult.brainDiagnosticsByProfileId["field-profile"]
            ?.selectedToolCount,
        selectedToolSource:
          applyResult.brainDiagnosticsByProfileId["field-profile"]
            ?.selectedToolSource,
        toolAdapterStatus:
          applyResult.brainDiagnosticsByProfileId["field-profile"]
            ?.toolAdapterStatus,
      },
      {
        moduleId: "pi-agent-core",
        selectedToolCount: requestedToolNames.length,
        selectedToolSource: "service:mcp:field-profile",
        toolAdapterStatus: "neutral_tools_adapted_to_pi",
      },
    );

    const subscription = await native.subscribeEvents({
      eventKinds: ["brain_event_observed"],
      sessionId: "field-session" as SessionId,
    });
    const request = await native.buildBrainWakeRequestForSession({
      brain,
      sessionId: "field-session" as SessionId,
      systemPrompt: "Use the configured tools.",
      roleAssemblyJson: encoder.encode(
        JSON.stringify({
          instructions: "Call git_status and summarize the result.",
        }),
      ),
      wakeId: "service-den-router-tools-wake",
    });
    await native.wakeBrain(request);
    const events = await native.drainSubscriptionEvents(subscription, 16);
    await native.unsubscribeEvents(subscription);

    assert.equal(denRouterOptions[0]?.maxTokens, 128);
    assert.deepEqual([...selectedToolNames].sort(), requestedToolNames);
    assert.match(outputs.git_status ?? "", /git status --short/);
    assert.equal(outputs.field_search, "mcp:field-mcp:search");
    assert.deepEqual(
      mcpRequests
        .filter((request) => request.sessionId === "fake-mcp-session")
        .filter(
          (request) =>
            request.method === "tools/list" || request.method === "tools/call",
        )
        .map((request) => ({
          method: request.method,
          toolProfile: request.toolProfile,
          toolName: request.toolName,
          arguments: request.arguments,
        })),
      [
        {
          method: "tools/list",
          toolProfile: "field-profile-mcp",
          toolName: undefined,
          arguments: undefined,
        },
        {
          method: "tools/call",
          toolProfile: "field-profile-mcp",
          toolName: "search",
          arguments: { query: "runner mcp tools" },
        },
      ],
    );
    assert.deepEqual(
      mcpRequests
        .filter((request) => request.sessionId === "fake-mcp-session")
        .filter(
          (request) =>
            request.method === "tools/list" || request.method === "tools/call",
        )
        .map((request) => request.url),
      [
        "http://mcp.local/mcp?tool_profile=field-profile-mcp",
        "http://mcp.local/mcp?tool_profile=field-profile-mcp",
      ],
    );
    assert.deepEqual(
      mcpRequests
        .filter((request) => request.method === "tools/call")
        .filter((request) => request.sessionId === "fake-mcp-session")
        .map((request) => ({
          bindingId: "field-mcp",
          toolName: request.toolName,
          arguments: request.arguments,
        })),
      [
        {
          bindingId: "field-mcp",
          toolName: "search",
          arguments: { query: "runner mcp tools" },
        },
      ],
    );
    assert.deepEqual(
      mcpRequests.map((request) => ({
        method: request.method,
        sessionId: request.sessionId,
        accept: request.accept,
      })),
      [
        {
          method: "tools/list",
          sessionId: undefined,
          accept: "application/json, text/event-stream",
        },
        {
          method: "initialize",
          sessionId: undefined,
          accept: "application/json, text/event-stream",
        },
        {
          method: "notifications/initialized",
          sessionId: "fake-mcp-session",
          accept: "application/json, text/event-stream",
        },
        {
          method: "tools/list",
          sessionId: "fake-mcp-session",
          accept: "application/json, text/event-stream",
        },
        {
          method: "tools/call",
          sessionId: undefined,
          accept: "application/json, text/event-stream",
        },
        {
          method: "initialize",
          sessionId: undefined,
          accept: "application/json, text/event-stream",
        },
        {
          method: "notifications/initialized",
          sessionId: "fake-mcp-session",
          accept: "application/json, text/event-stream",
        },
        {
          method: "tools/call",
          sessionId: "fake-mcp-session",
          accept: "application/json, text/event-stream",
        },
      ],
    );
    assert.match(
      outputs.den_memory_recall ?? "",
      /Den memory is available through service config/,
    );
    assert.equal(memoryRequests.length, 1);
    assert.equal(memoryRequests[0]?.authorization, "Bearer memory-token");
    assert.equal(
      memoryRequests[0]?.body["prompt"],
      "What memory guidance is relevant?",
    );
    assert.equal(await native.diagnosticCountRows("tool_call_history"), 8);
    const mcpTelemetry = events.filter(
      (event) =>
        event.type === "brain_event_observed" &&
        isToolCallEvent(event.event) &&
        event.event.metadata?.source === "mcp",
    );
    assert.deepEqual(
      mcpTelemetry.map((event) =>
        event.type === "brain_event_observed" && isToolCallEvent(event.event)
          ? event.event.type
          : undefined,
      ),
      ["tool_call_started", "tool_call_finished"],
    );
    assert.deepEqual(
      mcpTelemetry.map((event) =>
        event.type === "brain_event_observed" && isToolCallEvent(event.event)
          ? event.event.metadata?.bindingId
          : undefined,
      ),
      ["field-mcp", "field-mcp"],
    );

    console.log(
      JSON.stringify(
        {
          selectedToolNames,
          toolHistoryRows:
            await native.diagnosticCountRows("tool_call_history"),
          memoryRequests: memoryRequests.length,
          mcpRequests,
          gitStatusFirstLine: outputs.git_status?.split("\n")[0],
        },
        null,
        2,
      ),
    );
  } finally {
    await native.shutdownEngine({ engine, drainTimeoutMs: 1_000 });
  }
} finally {
  globalThis.fetch = originalFetch;
  rmSync(root, { force: true, recursive: true });
}

function writeRuntimeConfig(targetRoot: string): void {
  const configDir = join(targetRoot, "config");
  const profilesDir = join(configDir, "profiles");
  mkdirSync(profilesDir, { recursive: true });
  writeFileSync(
    join(configDir, "service.json"),
    JSON.stringify(
      {
        profilesDir,
        brains: [{ profileId: "field-profile" }],
        sessions: [
          {
            sessionId: "field-session",
            agentId: "field-agent",
            profileId: "field-profile",
            kind: "full",
          },
        ],
        channelBindings: [],
        mcpServers: [
          {
            id: "field",
            label: "Field MCP",
            baseUrl: "http://mcp.local/mcp",
            transport: "streamable_http",
            requestTimeoutMs: 10000,
          },
        ],
        mcpBindings: [
          {
            bindingId: "field-mcp",
            adapterId: "mcp-ts-main",
            agentId: "field-agent",
            sessionId: "field-session",
            profileId: "field-profile",
            serverNames: ["field"],
            endpointRef: "config://mcp/field",
            transport: "stdio",
            toolProfileKey: "field-profile-mcp",
            status: "active",
            diagnostics: {},
          },
        ],
      },
      null,
      2,
    ),
  );
  writeFileSync(
    join(profilesDir, "field-profile.json"),
    JSON.stringify(
      {
        profileId: "field-profile",
        modelConfig: {
          provider: "den-router",
          modelName: "fake-router-model",
          maxTokens: 512,
        },
        runtimeConfig: {
          maxTokensPerTurn: 128,
        },
        toolPolicy: {
          requestedTools: requestedToolNames,
        },
        mcpConfig: {
          toolProfile: "field-profile-mcp",
        },
      },
      null,
      2,
    ),
  );
}

function header(
  headers: HeadersInit | undefined,
  name: string,
): string | undefined {
  if (!headers) return undefined;
  if (headers instanceof Headers) return headers.get(name) ?? undefined;
  if (Array.isArray(headers)) {
    return headers.find(([key]) => key.toLowerCase() === name)?.[1];
  }
  return Object.entries(headers).find(
    ([key]) => key.toLowerCase() === name,
  )?.[1];
}

function jsonRpcResponse(id: unknown, result: unknown): Response {
  return new Response(
    JSON.stringify({
      jsonrpc: "2.0",
      id,
      result,
    }),
    { status: 200, headers: { "content-type": "application/json" } },
  );
}

function sseJsonRpcResponse(
  id: unknown,
  result: unknown,
  sessionId: string,
): Response {
  return new Response(
    `event: message\ndata: ${JSON.stringify({
      jsonrpc: "2.0",
      id,
      result,
    })}\n\n`,
    {
      status: 200,
      headers: {
        "content-type": "text/event-stream",
        "mcp-session-id": sessionId,
      },
    },
  );
}

function isRecord(input: unknown): input is Record<string, unknown> {
  return Boolean(input && typeof input === "object" && !Array.isArray(input));
}

function isToolCallEvent(
  event: BrainEvent,
): event is Extract<
  BrainEvent,
  { type: "tool_call_started" | "tool_call_finished" }
> {
  return (
    event.type === "tool_call_started" || event.type === "tool_call_finished"
  );
}
