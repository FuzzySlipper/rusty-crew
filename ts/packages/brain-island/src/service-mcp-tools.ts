import {
  createMcpBrainTool,
  discoverMcpToolCandidates,
} from "./mcp-brain-tools.js";
import { brainToolResultIsUnsuccessful } from "./tool-execution-host.js";
import type {
  McpDiscoveryReport,
  McpRegistryCandidate,
  McpToolDiscoveryClient,
  McpToolExecutionResult,
  McpToolExecutor,
} from "./service-adapter-ports.js";
import type {
  McpBindingRecord,
  McpSurfaceDiagnostics,
  ProfileId,
  SessionState,
  ToolProfile,
} from "@rusty-crew/contracts";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";
import {
  createMcpToolFinishedEvent,
  createMcpToolStartedEvent,
  evaluateMcpResourceHooks,
} from "./mcp-tool-telemetry.js";
import {
  createBridgeToolMetadataPolicyValidator,
  integrateMcpToolsWithRegistry,
  type McpRegistryIntegrationReport,
} from "./mcp-tool-registry-integration.js";
import type { BrainToolResolver } from "./tool-session-selection.js";
import type { ToolRegistry } from "./tool-registry.js";

export type ServiceMcpToolDiscoveryClientFactory = (
  binding: McpBindingRecord,
  config?: ServiceMcpEndpointConfig,
) =>
  | McpToolDiscoveryClient
  | undefined
  | Promise<McpToolDiscoveryClient | undefined>;

export type ServiceMcpToolExecutorFactory = (
  binding: McpBindingRecord,
  config?: ServiceMcpEndpointConfig,
) => McpToolExecutor | undefined;

export interface ServiceMcpEndpointConfig {
  baseUrl?: string;
  requestTimeoutMs?: number;
  servers?: readonly ServiceMcpServerEndpointConfig[];
}

export interface ServiceMcpServerEndpointConfig {
  id: string;
  baseUrl: string;
  requestTimeoutMs?: number;
}

/** Endpoint identity shared by session bindings and service-owned callers. */
export interface ServiceMcpEndpointIdentity {
  bindingId: string;
  endpointRef: string;
  toolProfileKey: string;
}

export interface ServiceMcpToolCatalogInput {
  bridge: Pick<NativeBridgeModule, "validateToolMetadataPolicy">;
  runtimeConfig: {
    mcpServers?: readonly ServiceMcpServerEndpointConfig[];
    mcpBindings: readonly McpBindingRecord[];
  };
  mcpConfig?: ServiceMcpEndpointConfig;
  discoveryClientFactory?: ServiceMcpToolDiscoveryClientFactory;
  surfaceDiagnostics?: readonly McpSurfaceDiagnostics[];
}

export interface ServiceMcpToolCatalog {
  registryForProfile(profileId: ProfileId): ToolRegistry | undefined;
  toolsetsForProfile(profileId: ProfileId): string[];
  candidatesForSession(session: SessionState): ServiceMcpToolCandidate[];
  readonly reports: readonly ServiceMcpToolProfileReport[];
}

export interface ServiceMcpToolProfileReport {
  profileId: ProfileId;
  toolsets: readonly string[];
  discoveryReports: readonly McpDiscoveryReport[];
  integration?: McpRegistryIntegrationReport;
  unavailableBindings: readonly string[];
}

export interface ServiceMcpToolCandidate {
  binding: McpBindingRecord;
  candidate: McpRegistryCandidate;
}

interface ProfileMcpAccumulator {
  profileId: ProfileId;
  toolsets: Set<string>;
  candidates: ServiceMcpToolCandidate[];
  discoveryReports: McpDiscoveryReport[];
  unavailableBindings: string[];
}

const JSON_RPC_VERSION = "2.0";
const MCP_PROTOCOL_VERSION = "2024-11-05";

export async function buildServiceMcpToolCatalog(
  input: ServiceMcpToolCatalogInput,
): Promise<ServiceMcpToolCatalog> {
  const profiles = new Map<ProfileId, ProfileMcpAccumulator>();
  const surfaceStatus = new Map(
    (input.surfaceDiagnostics ?? []).map((diagnostic) => [
      diagnostic.bindingId,
      diagnostic.status,
    ]),
  );

  for (const binding of input.runtimeConfig.mcpBindings) {
    if (binding.status !== "active") continue;
    if (
      surfaceStatus.size > 0 &&
      surfaceStatus.get(binding.bindingId) !== "active"
    ) {
      continue;
    }

    const profile = profileAccumulator(profiles, binding.profileId);
    profile.toolsets.add(`mcp:${binding.toolProfileKey}`);

    const discoveryClient = await (
      input.discoveryClientFactory ?? createDefaultMcpDiscoveryClient
    )(
      binding,
      buildServiceMcpEndpointConfig({
        mcpConfig: input.mcpConfig,
        mcpServers: input.runtimeConfig.mcpServers,
      }),
    );
    if (!discoveryClient) {
      profile.unavailableBindings.push(binding.bindingId);
      continue;
    }

    try {
      const discovery = await discoverMcpToolCandidates(
        binding,
        discoveryClient,
      );
      profile.discoveryReports.push(discovery);
      for (const candidate of discovery.candidates) {
        profile.candidates.push({ binding, candidate });
      }
    } catch {
      profile.unavailableBindings.push(binding.bindingId);
    }
  }

  const reports = await Promise.all(
    [...profiles.values()].map(
      async (profile): Promise<ServiceMcpToolProfileReport> => {
        const integration =
          profile.candidates.length === 0
            ? undefined
            : await integrateMcpToolsWithRegistry({
                catalogId: `service:mcp:${profile.profileId}`,
                candidates: profileIntegrationCandidates(profile.candidates),
                metadataPolicyValidator:
                  createBridgeToolMetadataPolicyValidator(input.bridge),
                inventoryRequest: {
                  requestedToolsets: [...profile.toolsets],
                },
              });
        return {
          profileId: profile.profileId,
          toolsets: [...profile.toolsets].sort(),
          discoveryReports: profile.discoveryReports,
          integration,
          unavailableBindings: [...profile.unavailableBindings].sort(),
        };
      },
    ),
  );

  return {
    registryForProfile(profileId) {
      const report = reports.find((item) => item.profileId === profileId);
      if (!report?.integration?.validation.ok) return undefined;
      return report.integration.registry;
    },
    toolsetsForProfile(profileId) {
      return (
        reports
          .find((item) => item.profileId === profileId)
          ?.toolsets.slice() ?? []
      );
    },
    candidatesForSession(session) {
      const profile = profiles.get(session.profileId);
      if (!profile) return [];
      return profile.candidates.filter(({ binding, candidate }) => {
        if (!matchesSession(binding, session)) return false;
        const report = reports.find(
          (item) => item.profileId === profile.profileId,
        );
        if (report?.integration && !report.integration.validation.ok) {
          return false;
        }
        return Boolean(candidate);
      });
    },
    reports,
  };
}

export function createServiceMcpToolResolver(input: {
  catalog: ServiceMcpToolCatalog;
  bridge?: Pick<
    NativeBridgeModule,
    "submitBrainEvent" | "suspendForGitHubGate"
  >;
  mcpConfig?: ServiceMcpEndpointConfig;
  executorFactory?: ServiceMcpToolExecutorFactory;
}): BrainToolResolver {
  return ({ wake }) =>
    input.catalog
      .candidatesForSession(wake.state.session)
      .flatMap(({ binding, candidate }) => {
        const executor = (
          input.executorFactory ?? createDefaultMcpToolExecutor
        )(binding, input.mcpConfig);
        if (!executor) return [];
        const decision = evaluateMcpResourceHooks({
          binding,
          candidate,
          toolProfile: wake.state.session.toolProfile as ToolProfile,
        });
        if (!decision.allowed) return [];
        const tool = createMcpBrainTool(binding, candidate, executor);
        const execute = async (
          toolCallId: string,
          params: unknown,
          signal: AbortSignal | undefined,
          contextWake = wake,
        ) => {
          await input.bridge?.submitBrainEvent({
            wakeId: wake.wakeId,
            sessionId: wake.sessionId,
            event: createMcpToolStartedEvent({
              binding,
              toolName: candidate.name,
              sourceToolName: candidate.source.sourceToolName,
              catalogRevision: candidate.source.catalogRevision,
            }),
          });
          try {
            let result = await tool.execute(toolCallId, params, signal);
            if (
              input.bridge !== undefined &&
              isGitHubGateWatch(candidate.source.sourceToolName)
            ) {
              const gate = pendingGitHubGate(result.details);
              if (gate !== undefined) {
                await input.bridge.suspendForGitHubGate({
                  sessionId: contextWake.sessionId,
                  runId: contextWake.wakeId as never,
                  ...(providerThreadIdentity(contextWake.providerState) ===
                  undefined
                    ? {}
                    : {
                        providerThreadId: providerThreadIdentity(
                          contextWake.providerState,
                        ),
                      }),
                  projectId: gate.projectId as never,
                  taskId: String(gate.taskId) as never,
                  gateId: gate.gateId,
                  commitSha: gate.commitSha,
                  now: new Date().toISOString(),
                });
                result = {
                  ...result,
                  turnDisposition: "suspend_external",
                };
              }
            }
            await input.bridge?.submitBrainEvent({
              wakeId: wake.wakeId,
              sessionId: wake.sessionId,
              event: createMcpToolFinishedEvent({
                binding,
                toolName: candidate.name,
                sourceToolName: candidate.source.sourceToolName,
                catalogRevision: candidate.source.catalogRevision,
                isError: brainToolResultIsUnsuccessful(result),
                allowed: true,
              }),
            });
            return result;
          } catch (error) {
            await input.bridge?.submitBrainEvent({
              wakeId: wake.wakeId,
              sessionId: wake.sessionId,
              event: createMcpToolFinishedEvent({
                binding,
                toolName: candidate.name,
                sourceToolName: candidate.source.sourceToolName,
                catalogRevision: candidate.source.catalogRevision,
                isError: true,
                allowed: true,
              }),
            });
            throw error;
          }
        };
        return [
          {
            ...tool,
            execute: (toolCallId, params, signal) =>
              execute(toolCallId, params, signal, wake),
            executeWithContext: async (params, context) =>
              execute(context.callId, params, context.signal, context.wake),
          },
        ];
      });
}

function isGitHubGateWatch(toolName: string): boolean {
  return (
    toolName === "watch_github_checks" || toolName === "await_github_checks"
  );
}

function pendingGitHubGate(
  details: unknown,
):
  | { gateId: number; projectId: string; taskId: number; commitSha: string }
  | undefined {
  if (!isRecord(details)) return undefined;
  const structured = isRecord(details.structuredContent)
    ? details.structuredContent
    : details;
  if (
    structured.status !== "pending" ||
    typeof structured.id !== "number" ||
    typeof structured.project_id !== "string" ||
    typeof structured.task_id !== "number" ||
    typeof structured.commit_sha !== "string"
  ) {
    return undefined;
  }
  return {
    gateId: structured.id,
    projectId: structured.project_id,
    taskId: structured.task_id,
    commitSha: structured.commit_sha,
  };
}

function providerThreadIdentity(
  providerState:
    | {
        moduleId: string;
        strategyId: string;
        providerFingerprint: string;
        payload: unknown;
      }
    | undefined,
): string | undefined {
  if (providerState === undefined) return undefined;
  if (isRecord(providerState.payload)) {
    for (const key of [
      "responseId",
      "response_id",
      "threadId",
      "thread_id",
      "conversationId",
      "conversation_id",
    ]) {
      const value = providerState.payload[key];
      if (typeof value === "string" && value.trim() !== "") return value;
    }
  }
  return `${providerState.moduleId}:${providerState.strategyId}:${providerState.providerFingerprint}`;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function profileIntegrationCandidates(
  candidates: readonly ServiceMcpToolCandidate[],
): McpRegistryCandidate[] {
  const unique = new Map<string, McpRegistryCandidate>();
  for (const { binding, candidate } of candidates) {
    const templateBindingId = materializedBindingTemplateId(binding.bindingId);
    const signature = JSON.stringify({
      templateBindingId,
      candidate: {
        ...candidate,
        source: {
          ...candidate.source,
          bindingId: templateBindingId,
        },
      },
    });
    if (!unique.has(signature)) unique.set(signature, candidate);
  }
  return [...unique.values()];
}

function materializedBindingTemplateId(bindingId: string): string {
  const delimiter = "--session--";
  const index = bindingId.lastIndexOf(delimiter);
  return index < 1 ? bindingId : bindingId.slice(0, index);
}

function profileAccumulator(
  profiles: Map<ProfileId, ProfileMcpAccumulator>,
  profileId: ProfileId,
): ProfileMcpAccumulator {
  const existing = profiles.get(profileId);
  if (existing) return existing;
  const next: ProfileMcpAccumulator = {
    profileId,
    toolsets: new Set(),
    candidates: [],
    discoveryReports: [],
    unavailableBindings: [],
  };
  profiles.set(profileId, next);
  return next;
}

function matchesSession(
  binding: McpBindingRecord,
  session: SessionState,
): boolean {
  if (binding.profileId !== session.profileId) return false;
  if (binding.agentId !== session.agentId) return false;
  return (
    binding.sessionId === undefined || binding.sessionId === session.sessionId
  );
}

export function createDefaultMcpDiscoveryClient(
  binding: McpBindingRecord,
  config?: ServiceMcpEndpointConfig,
): McpToolDiscoveryClient | undefined {
  const endpoint = endpointForBinding(binding, config);
  if (!endpoint) return undefined;
  const discoveryProfile = binding.serverNames.includes("den")
    ? "managed-runtime"
    : undefined;
  const client = new DefaultMcpHttpClient(
    endpoint.url,
    endpoint.timeoutMs,
    discoveryProfile,
  );
  return {
    async listTools() {
      const response = await client.request(
        "tools/list",
        discoveryProfile === undefined ? {} : { toolProfile: discoveryProfile },
      );
      const result = jsonRpcResult(response);
      const tools = resultRecord(result).tools;
      return Array.isArray(tools) ? tools : [];
    },
  };
}

function createDefaultMcpToolExecutor(
  binding: McpBindingRecord,
  config?: ServiceMcpEndpointConfig,
): McpToolExecutor | undefined {
  const endpoint = endpointForBinding(binding, config);
  if (!endpoint) return undefined;
  const client = new DefaultMcpHttpClient(endpoint.url, endpoint.timeoutMs);
  return {
    async callTool(input) {
      const response = await client.request("tools/call", {
        name: input.toolName,
        arguments: input.arguments,
      });
      const result = jsonRpcResult(response);
      const record = resultRecord(result);
      return {
        content: mcpResultContent(record.content, result),
        details: record,
        isError: record.isError === true,
      } satisfies McpToolExecutionResult;
    },
  };
}

export async function callConfiguredMcpTool(input: {
  binding: ServiceMcpEndpointIdentity;
  config?: ServiceMcpEndpointConfig;
  toolName: string;
  arguments: Record<string, unknown>;
  bearerToken?: string;
  clientName?: string;
}): Promise<McpToolExecutionResult> {
  const endpoint = endpointForBinding(input.binding, input.config);
  if (endpoint === undefined) {
    throw new Error(`MCP endpoint unavailable for ${input.binding.bindingId}`);
  }
  const client = new DefaultMcpHttpClient(
    endpoint.url,
    endpoint.timeoutMs,
    undefined,
    input.bearerToken,
    input.clientName,
  );
  const response = await client.request("tools/call", {
    name: input.toolName,
    arguments: input.arguments,
  });
  const result = jsonRpcResult(response);
  const record = resultRecord(result);
  return {
    content: mcpResultContent(record.content, result),
    details: record,
    isError: record.isError === true,
  };
}

export async function listConfiguredMcpTools(input: {
  binding: ServiceMcpEndpointIdentity;
  config?: ServiceMcpEndpointConfig;
  bearerToken?: string;
  clientName?: string;
}): Promise<unknown[]> {
  const endpoint = endpointForBinding(input.binding, input.config);
  if (endpoint === undefined) {
    throw new Error(`MCP endpoint unavailable for ${input.binding.bindingId}`);
  }
  const client = new DefaultMcpHttpClient(
    endpoint.url,
    endpoint.timeoutMs,
    input.binding.toolProfileKey,
    input.bearerToken,
    input.clientName,
  );
  const response = await client.request("tools/list", {
    toolProfile: input.binding.toolProfileKey,
  });
  const tools = resultRecord(jsonRpcResult(response)).tools;
  return Array.isArray(tools) ? tools : [];
}

function endpointForBinding(
  binding: ServiceMcpEndpointIdentity,
  config: ServiceMcpEndpointConfig | undefined,
): { url: URL; timeoutMs: number | undefined } | undefined {
  const direct = httpEndpoint(binding.endpointRef);
  if (direct) {
    return { url: direct, timeoutMs: config?.requestTimeoutMs };
  }
  const configured = configuredMcpEndpoint(binding, config);
  if (configured) {
    return configured;
  }
  return undefined;
}

function httpEndpoint(endpointRef: string): URL | undefined {
  try {
    const url = new URL(endpointRef);
    return url.protocol === "http:" || url.protocol === "https:"
      ? url
      : undefined;
  } catch {
    return undefined;
  }
}

function configuredMcpEndpoint(
  binding: ServiceMcpEndpointIdentity,
  config: ServiceMcpEndpointConfig | undefined,
): { url: URL; timeoutMs: number | undefined } | undefined {
  try {
    const endpointRef = new URL(binding.endpointRef);
    if (endpointRef.protocol !== "config:" || endpointRef.hostname !== "mcp") {
      return undefined;
    }
    const serverId = decodeURIComponent(
      endpointRef.pathname.replace(/^\/+/, ""),
    );
    const server = config?.servers?.find((candidate) => {
      return candidate.id === serverId;
    });
    if (!server?.baseUrl) return undefined;
    const url = new URL(server.baseUrl);
    url.searchParams.set("tool_profile", binding.toolProfileKey);
    return {
      url,
      timeoutMs: server?.requestTimeoutMs ?? config?.requestTimeoutMs,
    };
  } catch {
    return undefined;
  }
}

export function buildServiceMcpEndpointConfig(input: {
  mcpConfig?: ServiceMcpEndpointConfig;
  mcpServers?: readonly ServiceMcpServerEndpointConfig[];
}): ServiceMcpEndpointConfig | undefined {
  const servers = [
    ...(input.mcpConfig?.servers ?? []),
    ...(input.mcpServers ?? []),
  ];
  if (!input.mcpConfig && servers.length === 0) return undefined;
  return {
    ...input.mcpConfig,
    servers: dedupeMcpServers(servers),
  };
}

function dedupeMcpServers(
  servers: readonly ServiceMcpServerEndpointConfig[],
): ServiceMcpServerEndpointConfig[] {
  const byId = new Map<string, ServiceMcpServerEndpointConfig>();
  for (const server of servers) {
    byId.set(server.id, server);
  }
  return [...byId.values()];
}

class DefaultMcpHttpClient {
  private sessionId: string | undefined;

  constructor(
    private readonly endpoint: URL,
    private readonly timeoutMs: number | undefined,
    private readonly discoveryProfile?: string,
    private readonly bearerToken?: string,
    private readonly clientName = "rusty-crew",
  ) {}

  async request(
    method: string,
    params: Record<string, unknown>,
  ): Promise<unknown> {
    if (this.sessionId) {
      return (
        await postJsonRpc({
          endpoint: this.endpoint,
          method,
          params,
          sessionId: this.sessionId,
          timeoutMs: this.timeoutMs,
          bearerToken: this.bearerToken,
        })
      ).body;
    }

    try {
      const response = await postJsonRpc({
        endpoint: this.endpoint,
        method,
        params,
        timeoutMs: this.timeoutMs,
        bearerToken: this.bearerToken,
      });
      this.sessionId = response.sessionId ?? this.sessionId;
      return response.body;
    } catch (error) {
      if (!requiresInitializedMcpSession(error)) throw error;
    }

    await this.initialize();
    return (
      await postJsonRpc({
        endpoint: this.endpoint,
        method,
        params,
        sessionId: this.sessionId,
        timeoutMs: this.timeoutMs,
        bearerToken: this.bearerToken,
      })
    ).body;
  }

  private async initialize(): Promise<void> {
    const response = await postJsonRpc({
      endpoint: this.endpoint,
      method: "initialize",
      params: {
        protocolVersion: MCP_PROTOCOL_VERSION,
        capabilities: {},
        clientInfo: {
          name: this.clientName,
          version: "0.1.0",
        },
        ...(this.discoveryProfile === undefined
          ? {}
          : { toolProfile: this.discoveryProfile }),
      },
      timeoutMs: this.timeoutMs,
      bearerToken: this.bearerToken,
    });
    if (!response.sessionId) {
      throw new Error("MCP initialize response did not include a session id");
    }
    this.sessionId = response.sessionId;
    await postJsonRpc({
      endpoint: this.endpoint,
      method: "notifications/initialized",
      params: {},
      sessionId: this.sessionId,
      timeoutMs: this.timeoutMs,
      expectResponse: false,
      bearerToken: this.bearerToken,
    });
  }
}

interface JsonRpcPostInput {
  endpoint: URL;
  method: string;
  params: Record<string, unknown>;
  timeoutMs: number | undefined;
  sessionId?: string;
  expectResponse?: boolean;
  bearerToken?: string;
}

interface JsonRpcPostResponse {
  body: unknown;
  sessionId?: string;
}

class McpHttpError extends Error {
  constructor(
    message: string,
    readonly status: number,
    readonly bodyText: string,
  ) {
    super(message);
  }
}

async function postJsonRpc(
  input: JsonRpcPostInput,
): Promise<JsonRpcPostResponse> {
  const controller = new AbortController();
  const timeout =
    input.timeoutMs === undefined
      ? undefined
      : setTimeout(() => controller.abort(), input.timeoutMs);
  try {
    const headers: Record<string, string> = {
      accept: "application/json, text/event-stream",
      "content-type": "application/json",
    };
    if (input.sessionId) {
      headers["Mcp-Session-Id"] = input.sessionId;
    }
    if (input.bearerToken) {
      headers.authorization = `Bearer ${input.bearerToken}`;
    }
    const response = await fetch(input.endpoint, {
      method: "POST",
      signal: controller.signal,
      headers,
      body: JSON.stringify({
        jsonrpc: JSON_RPC_VERSION,
        ...(input.expectResponse === false
          ? {}
          : { id: `${Date.now()}:${input.method}` }),
        method: input.method,
        params: input.params,
      }),
    });
    const bodyText = await response.text();
    if (!response.ok) {
      throw new McpHttpError(
        `MCP ${input.method} failed with HTTP ${response.status}`,
        response.status,
        bodyText,
      );
    }
    return {
      body:
        input.expectResponse === false
          ? undefined
          : parseMcpResponseBody(bodyText, response.headers),
      sessionId: response.headers.get("mcp-session-id") ?? undefined,
    };
  } finally {
    if (timeout !== undefined) {
      clearTimeout(timeout);
    }
  }
}

function requiresInitializedMcpSession(error: unknown): boolean {
  if (!(error instanceof McpHttpError)) return false;
  return (
    error.status === 400 &&
    /session|initialize|Mcp-Session-Id/i.test(error.bodyText)
  );
}

function parseMcpResponseBody(bodyText: string, headers: Headers): unknown {
  if (bodyText.trim().length === 0) return undefined;
  const contentType = headers.get("content-type") ?? "";
  if (contentType.includes("text/event-stream")) {
    return parseServerSentJson(bodyText);
  }
  return JSON.parse(bodyText);
}

function parseServerSentJson(bodyText: string): unknown {
  for (const eventBlock of bodyText.split(/\r?\n\r?\n/)) {
    const data = eventBlock
      .split(/\r?\n/)
      .filter((line) => line.startsWith("data:"))
      .map((line) => line.slice("data:".length).trimStart())
      .join("\n")
      .trim();
    if (data.length > 0) {
      return JSON.parse(data);
    }
  }
  throw new Error("MCP SSE response did not include a data event");
}

function jsonRpcResult(response: unknown): unknown {
  const record = resultRecord(response);
  if (record.error) {
    throw new Error(`MCP JSON-RPC error: ${JSON.stringify(record.error)}`);
  }
  return "result" in record ? record.result : response;
}

function resultRecord(input: unknown): Record<string, unknown> {
  return input && typeof input === "object" && !Array.isArray(input)
    ? (input as Record<string, unknown>)
    : {};
}

function mcpResultContent(
  content: unknown,
  fallback: unknown,
): McpToolExecutionResult["content"] {
  if (typeof content === "string") return content;
  if (isMcpContentArray(content)) return content;
  return JSON.stringify(fallback);
}

function isMcpContentArray(
  content: unknown,
): content is McpToolExecutionResult["content"] {
  return (
    Array.isArray(content) &&
    content.every((item) => {
      if (!item || typeof item !== "object") return false;
      const value = item as Record<string, unknown>;
      return (
        (value.type === "text" && typeof value.text === "string") ||
        (value.type === "image" &&
          typeof value.data === "string" &&
          typeof value.mimeType === "string")
      );
    })
  );
}
