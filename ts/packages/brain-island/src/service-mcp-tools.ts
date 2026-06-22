import {
  createMcpPiAgentTool,
  discoverMcpToolCandidates,
  type McpDiscoveryReport,
  type McpRegistryCandidate,
  type McpToolDiscoveryClient,
  type McpToolExecutionResult,
  type McpToolExecutor,
} from "@rusty-crew/adapter-mcp";
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
  integrateMcpToolsWithRegistry,
  type McpRegistryIntegrationReport,
} from "./mcp-tool-registry-integration.js";
import type { PiAgentToolResolver } from "./tool-session-selection.js";
import type { ToolRegistry } from "./tool-registry.js";

export type ServiceMcpToolDiscoveryClientFactory = (
  binding: McpBindingRecord,
) =>
  | McpToolDiscoveryClient
  | undefined
  | Promise<McpToolDiscoveryClient | undefined>;

export type ServiceMcpToolExecutorFactory = (
  binding: McpBindingRecord,
) => McpToolExecutor | undefined;

export interface ServiceMcpToolCatalogInput {
  runtimeConfig: {
    mcpBindings: readonly McpBindingRecord[];
  };
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
    )(binding);
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

  const reports = [...profiles.values()].map<ServiceMcpToolProfileReport>(
    (profile) => {
      const integration =
        profile.candidates.length === 0
          ? undefined
          : integrateMcpToolsWithRegistry({
              catalogId: `service:mcp:${profile.profileId}`,
              candidates: profile.candidates.map((item) => item.candidate),
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
  bridge?: Pick<NativeBridgeModule, "submitBrainEvent">;
  executorFactory?: ServiceMcpToolExecutorFactory;
}): PiAgentToolResolver {
  return ({ wake }) =>
    input.catalog
      .candidatesForSession(wake.state.session)
      .flatMap(({ binding, candidate }) => {
        const executor = (
          input.executorFactory ?? createDefaultMcpToolExecutor
        )(binding);
        if (!executor) return [];
        const decision = evaluateMcpResourceHooks({
          binding,
          candidate,
          toolProfile: wake.state.session.toolProfile as ToolProfile,
        });
        if (!decision.allowed) return [];
        const tool = createMcpPiAgentTool(binding, candidate, executor);
        return [
          {
            ...tool,
            execute: async (toolCallId, params, signal) => {
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
                const result = await tool.execute(toolCallId, params, signal);
                await input.bridge?.submitBrainEvent({
                  wakeId: wake.wakeId,
                  sessionId: wake.sessionId,
                  event: createMcpToolFinishedEvent({
                    binding,
                    toolName: candidate.name,
                    sourceToolName: candidate.source.sourceToolName,
                    catalogRevision: candidate.source.catalogRevision,
                    isError: false,
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
            },
          },
        ];
      });
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

function createDefaultMcpDiscoveryClient(
  binding: McpBindingRecord,
): McpToolDiscoveryClient | undefined {
  const endpoint = httpEndpoint(binding.endpointRef);
  if (!endpoint) return undefined;
  return {
    async listTools() {
      const response = await postJsonRpc(endpoint, "tools/list", {});
      const result = jsonRpcResult(response);
      const tools = resultRecord(result).tools;
      return Array.isArray(tools) ? tools : [];
    },
  };
}

function createDefaultMcpToolExecutor(
  binding: McpBindingRecord,
): McpToolExecutor | undefined {
  const endpoint = httpEndpoint(binding.endpointRef);
  if (!endpoint) return undefined;
  return {
    async callTool(input) {
      const response = await postJsonRpc(endpoint, "tools/call", {
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

async function postJsonRpc(
  endpoint: URL,
  method: string,
  params: Record<string, unknown>,
): Promise<unknown> {
  const response = await fetch(endpoint, {
    method: "POST",
    headers: {
      accept: "application/json",
      "content-type": "application/json",
    },
    body: JSON.stringify({
      jsonrpc: JSON_RPC_VERSION,
      id: `${Date.now()}:${method}`,
      method,
      params,
    }),
  });
  const body = await response.json();
  if (!response.ok) {
    throw new Error(`MCP ${method} failed with HTTP ${response.status}`);
  }
  return body;
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
    content.every(
      (item) =>
        item &&
        typeof item === "object" &&
        ((item as { type?: unknown }).type === "text" ||
          (item as { type?: unknown }).type === "image"),
    )
  );
}
