import type { McpBindingRecord } from "@rusty-crew/contracts";
import type { AdminRouteResult } from "./admin-diagnostics-api.js";
import type { RustyCrewMcpServerConfig } from "./service-config.js";
import type { RustyCrewRuntimeConfig } from "./service-runtime-config.js";
import { readOnlyMethod, successRoute } from "./service-route-results.js";

export interface AdminMcpCatalogRouteRequest {
  method?: string;
  requestId: string;
}

export interface AdminMcpCatalogRouteContext {
  config: {
    mcp: {
      baseUrl?: string;
      servers: RustyCrewMcpServerConfig[];
    };
  };
  runtimeConfig: Pick<RustyCrewRuntimeConfig, "mcpServers" | "mcpBindings">;
}

export async function handleAdminMcpCatalogRequest(
  request: AdminMcpCatalogRouteRequest,
  context: AdminMcpCatalogRouteContext,
): Promise<AdminRouteResult> {
  const methodFailure = readOnlyMethod(
    request.method,
    request.requestId,
    "mcp_catalog_read_only",
    "MCP catalog routes only support GET",
  );
  if (methodFailure) return methodFailure;

  const serverCatalog = mcpServerCatalogEntries(context);
  const serverIds = new Set(serverCatalog.map((server) => server.id));
  const compatibilityServerId = context.config.mcp.baseUrl
    ? context.config.mcp.servers[0]?.id
    : undefined;
  const bindings = context.runtimeConfig.mcpBindings.map((binding) =>
    mcpBindingCatalogEntry(binding, serverIds, compatibilityServerId),
  );
  const bindingCounts = new Map<string, number>();
  for (const binding of bindings) {
    if (!binding.resolvedServerId) continue;
    bindingCounts.set(
      binding.resolvedServerId,
      (bindingCounts.get(binding.resolvedServerId) ?? 0) + 1,
    );
  }
  const servers = serverCatalog.map((server) => ({
    id: server.id,
    label: server.label,
    baseUrl: server.baseUrl,
    transport: server.transport,
    requestTimeoutMs: server.requestTimeoutMs,
    source: server.source,
    configuredBindingCount: bindingCounts.get(server.id) ?? 0,
  }));
  const toolProfiles = [
    ...new Set(bindings.map((binding) => binding.toolProfileKey)),
  ].sort();
  return successRoute(request.requestId, {
    schemaVersion: 1,
    compatibilityBaseUrlConfigured: Boolean(context.config.mcp.baseUrl),
    servers,
    toolProfiles,
    bindings,
  });
}

export function mcpServerCatalogEntries(
  context: AdminMcpCatalogRouteContext,
): RustyCrewMcpServerConfig[] {
  const byId = new Map<string, RustyCrewMcpServerConfig>();
  for (const server of context.config.mcp.servers) {
    byId.set(server.id, server);
  }
  for (const server of context.runtimeConfig.mcpServers ?? []) {
    byId.set(server.id, server);
  }
  return [...byId.values()].sort((left, right) =>
    left.id.localeCompare(right.id),
  );
}

export function mcpServerIdFromEndpointRef(
  endpointRef: string,
): string | undefined {
  try {
    const url = new URL(endpointRef);
    if (url.protocol !== "config:" || url.hostname !== "mcp") {
      return undefined;
    }
    const serverId = decodeURIComponent(url.pathname.replace(/^\/+/, ""));
    return serverId.length > 0 ? serverId : undefined;
  } catch {
    return undefined;
  }
}

function mcpBindingCatalogEntry(
  binding: McpBindingRecord,
  serverIds: Set<string>,
  compatibilityServerId: string | undefined,
) {
  const endpointServerId = mcpServerIdFromEndpointRef(binding.endpointRef);
  const resolvedServerId =
    endpointServerId && serverIds.has(endpointServerId)
      ? endpointServerId
      : endpointServerId && compatibilityServerId
        ? compatibilityServerId
        : undefined;
  return {
    bindingId: binding.bindingId,
    adapterId: binding.adapterId,
    agentId: binding.agentId,
    sessionId: binding.sessionId,
    profileId: binding.profileId,
    endpointRef: binding.endpointRef,
    endpointServerId,
    resolvedServerId,
    transport: binding.transport,
    toolProfileKey: binding.toolProfileKey,
    serverNames: binding.serverNames,
    status: binding.status,
    degradedReason: binding.degradedReason,
  };
}
