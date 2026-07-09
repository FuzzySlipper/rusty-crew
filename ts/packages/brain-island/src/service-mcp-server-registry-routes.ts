import {
  handleAdminMcpCatalogRequest,
  mcpServerCatalogEntries,
} from "./service-mcp-catalog-routes.js";
import type { AdminRouteResult } from "./admin-diagnostics-api.js";
import type { RustyCrewMcpServerConfig } from "./service-config.js";
import type { RustyCrewRuntimeConfig } from "./service-runtime-config.js";
import { failure, successRoute } from "./service-route-results.js";

export interface AdminMcpServerRegistryRouteRequest {
  method?: string;
  url: URL;
  requestId: string;
  body?: unknown;
}

export interface RuntimeConfigFileForMcpServerMutation {
  value: Record<string, unknown>;
  array(key: string): unknown[];
}

export interface AdminMcpServerRegistryRouteContext {
  config(): {
    mcp: {
      baseUrl?: string;
      servers: RustyCrewMcpServerConfig[];
    };
  };
  runtimeConfig(): Pick<
    RustyCrewRuntimeConfig,
    "mcpServers" | "mcpBindings"
  >;
  readRuntimeConfigFile(): Promise<RuntimeConfigFileForMcpServerMutation>;
  writeRuntimeConfigFile(value: Record<string, unknown>): Promise<void>;
  applyRuntimeConfigFromDisk(input: {
    createMissingSessions: boolean;
    eventType: string;
    summaryPrefix: string;
  }): Promise<unknown>;
  withRuntimeConfigMutation<T>(mutation: () => Promise<T>): Promise<T>;
}

export async function handleAdminMcpServerRegistryRequest(
  request: AdminMcpServerRegistryRouteRequest,
  context: AdminMcpServerRegistryRouteContext,
): Promise<AdminRouteResult> {
  const requestIdValue = request.requestId;
  const method = (request.method ?? "GET").toUpperCase();
  const serverId = mcpServerIdFromPath(request.url.pathname);
  try {
    if (request.url.pathname === "/v1/admin/mcp/servers") {
      if (method === "GET") {
        return handleAdminMcpCatalogRequest(
          { method, requestId: requestIdValue },
          { config: context.config(), runtimeConfig: context.runtimeConfig() },
        );
      }
      if (method === "POST") {
        return upsertAdminMcpServer(
          context,
          requestIdValue,
          recordBody(request.body),
          undefined,
        );
      }
      return failure(405, requestIdValue, {
        code: "method_not_allowed",
        reason_code: "mcp_server_collection_method_not_allowed",
        message: "MCP server collection supports GET and POST",
        retryable: false,
      });
    }

    if (serverId === undefined) {
      return failure(404, requestIdValue, {
        code: "not_found",
        reason_code: "unknown_mcp_server_route",
        message: `unknown MCP server route ${request.url.pathname}`,
        retryable: false,
      });
    }

    if (method === "PUT" || method === "PATCH") {
      return upsertAdminMcpServer(
        context,
        requestIdValue,
        recordBody(request.body),
        serverId,
      );
    }

    if (method === "DELETE") {
      return deleteAdminMcpServer(context, requestIdValue, serverId);
    }

    return failure(405, requestIdValue, {
      code: "method_not_allowed",
      reason_code: "mcp_server_item_method_not_allowed",
      message: "MCP server item routes support PUT, PATCH, and DELETE",
      retryable: false,
    });
  } catch (error) {
    return failure(400, requestIdValue, {
      code: "invalid_input",
      reason_code: "invalid_mcp_server_write",
      message: errorMessage(error, "invalid MCP server registry write"),
      retryable: false,
    });
  }
}

export function mcpServerWriteFromBody(
  body: Record<string, unknown>,
  pathServerId: string | undefined,
): RustyCrewMcpServerConfig {
  const id = pathServerId ?? optionalString(body.id ?? body.serverId);
  if (id === undefined) {
    throw new Error("MCP server id is required");
  }
  assertMcpServerId(id, "MCP server id");
  if (
    pathServerId !== undefined &&
    optionalString(body.id ?? body.serverId) !== undefined &&
    optionalString(body.id ?? body.serverId) !== pathServerId
  ) {
    throw new Error("MCP server body id must match path id");
  }

  const baseUrl = requiredString(body.baseUrl ?? body.base_url, "baseUrl");
  assertHttpUrl(baseUrl, "baseUrl");
  const requestTimeoutMs =
    body.requestTimeoutMs === undefined && body.request_timeout_ms === undefined
      ? undefined
      : positiveInteger(
          body.requestTimeoutMs ?? body.request_timeout_ms,
          "requestTimeoutMs",
        );
  return {
    id,
    label: optionalString(body.label),
    baseUrl,
    transport:
      optionalString(body.transport ?? body.transportKind) ?? "streamable_http",
    requestTimeoutMs,
    source: "runtime",
  };
}

async function upsertAdminMcpServer(
  context: AdminMcpServerRegistryRouteContext,
  requestIdValue: string,
  body: Record<string, unknown>,
  pathServerId: string | undefined,
): Promise<AdminRouteResult> {
  return context.withRuntimeConfigMutation(() =>
    upsertAdminMcpServerLocked(context, requestIdValue, body, pathServerId),
  );
}

async function upsertAdminMcpServerLocked(
  context: AdminMcpServerRegistryRouteContext,
  requestIdValue: string,
  body: Record<string, unknown>,
  pathServerId: string | undefined,
): Promise<AdminRouteResult> {
  const server = mcpServerWriteFromBody(body, pathServerId);
  const runtimeConfigFile = await context.readRuntimeConfigFile();
  const servers = runtimeConfigFile.array("mcpServers");
  const existingIndex = servers.findIndex(
    (entry) => isRecord(entry) && optionalString(entry.id) === server.id,
  );
  const status = existingIndex >= 0 ? "updated" : "created";
  if (existingIndex >= 0) {
    servers[existingIndex] = server;
  } else {
    servers.push(server);
  }
  await context.writeRuntimeConfigFile(runtimeConfigFile.value);
  const applyResult = await context.applyRuntimeConfigFromDisk({
    createMissingSessions: false,
    eventType: "mcp_server_registry_updated",
    summaryPrefix: `MCP server ${server.id} ${status}`,
  });
  return successRoute(requestIdValue, {
    status,
    server,
    applyResult,
    catalog: mcpServerRegistryCatalog(context),
  });
}

async function deleteAdminMcpServer(
  context: AdminMcpServerRegistryRouteContext,
  requestIdValue: string,
  serverId: string,
): Promise<AdminRouteResult> {
  return context.withRuntimeConfigMutation(() =>
    deleteAdminMcpServerLocked(context, requestIdValue, serverId),
  );
}

async function deleteAdminMcpServerLocked(
  context: AdminMcpServerRegistryRouteContext,
  requestIdValue: string,
  serverId: string,
): Promise<AdminRouteResult> {
  assertMcpServerId(serverId, "server id");
  const runtimeConfigFile = await context.readRuntimeConfigFile();
  const servers = runtimeConfigFile.array("mcpServers");
  const existingIndex = servers.findIndex(
    (entry) => isRecord(entry) && optionalString(entry.id) === serverId,
  );
  const envServer = context
    .config()
    .mcp.servers.find((server) => server.id === serverId);
  if (existingIndex < 0) {
    return failure(envServer ? 409 : 404, requestIdValue, {
      code: envServer ? "failed_precondition" : "not_found",
      reason_code: envServer
        ? "mcp_server_env_seeded_not_runtime_managed"
        : "mcp_server_not_found",
      message: envServer
        ? `MCP server ${serverId} is seeded from service environment; create a runtime override to edit it or change service environment to remove it`
        : `MCP server ${serverId} was not found in runtime registry`,
      retryable: false,
    });
  }

  const activeBindingCount = context
    .runtimeConfig()
    .mcpBindings.filter(
      (binding) =>
        binding.status === "active" &&
        (binding.serverNames.includes(serverId) ||
          binding.endpointRef === `config://mcp/${serverId}`),
    ).length;
  if (activeBindingCount > 0 && envServer === undefined) {
    return failure(409, requestIdValue, {
      code: "failed_precondition",
      reason_code: "mcp_server_has_active_bindings",
      message: `MCP server ${serverId} has ${activeBindingCount} active binding(s); remove profile bindings before deleting it`,
      retryable: false,
    });
  }

  const [removed] = servers.splice(existingIndex, 1);
  await context.writeRuntimeConfigFile(runtimeConfigFile.value);
  const applyResult = await context.applyRuntimeConfigFromDisk({
    createMissingSessions: false,
    eventType: "mcp_server_registry_deleted",
    summaryPrefix: `MCP server ${serverId} deleted`,
  });
  return successRoute(requestIdValue, {
    status: "deleted",
    serverId,
    removed,
    applyResult,
    catalog: mcpServerRegistryCatalog(context),
  });
}

function mcpServerRegistryCatalog(context: AdminMcpServerRegistryRouteContext) {
  return mcpServerCatalogEntries({
    config: context.config(),
    runtimeConfig: context.runtimeConfig(),
  }).map((server) => ({
    id: server.id,
    label: server.label,
    baseUrl: server.baseUrl,
    transport: server.transport,
    requestTimeoutMs: server.requestTimeoutMs,
    source: server.source,
  }));
}

function mcpServerIdFromPath(pathname: string): string | undefined {
  const prefix = "/v1/admin/mcp/servers/";
  if (!pathname.startsWith(prefix)) return undefined;
  const rest = pathname.slice(prefix.length);
  if (!rest || rest.includes("/")) return undefined;
  return decodeURIComponent(rest);
}

function assertMcpServerId(value: string, fieldName: string): void {
  if (!/^[A-Za-z0-9_.:-]+$/.test(value)) {
    throw new Error(
      `${fieldName} may only contain letters, numbers, dot, underscore, colon, or dash`,
    );
  }
}

function assertHttpUrl(value: string, fieldName: string): void {
  try {
    const parsed = new URL(value);
    if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
      throw new Error("protocol must be http or https");
    }
  } catch (error) {
    throw new Error(`${fieldName} must be a valid HTTP(S) URL`, {
      cause: error,
    });
  }
}

function positiveInteger(value: unknown, fieldName: string): number {
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value <= 0) {
    throw new Error(`${fieldName} must be a positive integer`);
  }
  return value;
}

function recordBody(value: unknown): Record<string, unknown> {
  return isRecord(value) ? value : {};
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function requiredString(value: unknown, fieldName: string): string {
  const text = optionalString(value);
  if (!text) throw new Error(`${fieldName} is required`);
  return text;
}

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error && error.message ? error.message : fallback;
}
