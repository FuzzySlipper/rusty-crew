import type {
  McpBindingRecord,
  McpSurfaceDiagnostics,
} from "@rusty-crew/contracts";
import { discoverMcpToolCandidates } from "./mcp-brain-tools.js";
import type {
  McpSurfaceManagerPort,
  McpToolDiscoveryClient,
} from "./service-adapter-ports.js";
import type { ToolInventoryRequest } from "./tool-registry.js";
import {
  integrateMcpToolsWithRegistry,
  type McpRegistryIntegrationReport,
  type PortableToolMetadataPolicyValidator,
} from "./mcp-tool-registry-integration.js";

export interface McpSurfaceReloadInput {
  binding: McpBindingRecord;
  manager: McpSurfaceManagerPort;
  discoveryClient: McpToolDiscoveryClient;
  catalogId: string;
  metadataPolicyValidator: PortableToolMetadataPolicyValidator;
  previousToolNames?: readonly string[];
  inventoryRequest?: ToolInventoryRequest;
  requestedBy: string;
  reason: string;
  now?: () => string;
}

export interface McpToolDiff {
  oldTools: string[];
  newTools: string[];
  addedTools: string[];
  removedTools: string[];
  unchangedTools: string[];
}

export interface McpSurfaceReloadReport {
  bindingId: string;
  sessionId?: string;
  profileId: string;
  status: "reloaded" | "degraded";
  requestedBy: string;
  reason: string;
  startedAt: string;
  finishedAt: string;
  durationMs: number;
  diagnostics?: McpSurfaceDiagnostics;
  discoveryIssueCount: number;
  collisionCount: number;
  optionalServerFailures: string[];
  toolDiff: McpToolDiff;
  registry?: McpRegistryIntegrationReport;
  degradedReason?: string;
}

export async function reloadMcpSurface(
  input: McpSurfaceReloadInput,
): Promise<McpSurfaceReloadReport> {
  const now = input.now ?? (() => new Date().toISOString());
  const startedAt = now();
  const startedMs = Date.parse(startedAt);
  const oldTools = [...(input.previousToolNames ?? [])].sort();
  const connect = await input.manager.reload(input.binding);
  const diagnostics =
    input.manager.diagnostics(input.binding.bindingId) ??
    fallbackMcpDiagnostics(input.binding, connect.degradedReason);

  if (connect.status !== "active") {
    const finishedAt = now();
    return {
      bindingId: input.binding.bindingId,
      sessionId: input.binding.sessionId,
      profileId: input.binding.profileId,
      status: "degraded",
      requestedBy: input.requestedBy,
      reason: input.reason,
      startedAt,
      finishedAt,
      durationMs: durationMs(startedMs, finishedAt),
      diagnostics,
      discoveryIssueCount: 0,
      collisionCount: 0,
      optionalServerFailures: connect.optional
        ? [connect.degradedReason ?? "optional MCP surface failed to reload"]
        : [],
      toolDiff: diffTools(oldTools, []),
      degradedReason: connect.degradedReason,
    };
  }

  const discovery = await discoverMcpToolCandidates(
    input.binding,
    input.discoveryClient,
  );
  const registry = await integrateMcpToolsWithRegistry({
    catalogId: input.catalogId,
    candidates: discovery.candidates,
    metadataPolicyValidator: input.metadataPolicyValidator,
    inventoryRequest: input.inventoryRequest,
  });
  const newTools =
    registry.inventory?.selectedTools.map((entry) => entry.name) ?? [];
  const collisionCount = registry.validation.issues.filter(
    (issue) =>
      issue.code === "duplicate_name" || issue.code === "capability_collision",
  ).length;
  const finishedAt = now();

  return {
    bindingId: input.binding.bindingId,
    sessionId: input.binding.sessionId,
    profileId: input.binding.profileId,
    status: registry.validation.ok ? "reloaded" : "degraded",
    requestedBy: input.requestedBy,
    reason: input.reason,
    startedAt,
    finishedAt,
    durationMs: durationMs(startedMs, finishedAt),
    diagnostics,
    discoveryIssueCount: discovery.issues.length,
    collisionCount,
    optionalServerFailures: [],
    toolDiff: diffTools(oldTools, newTools),
    registry,
    degradedReason: registry.validation.ok
      ? undefined
      : "MCP registry validation failed after reload",
  };
}

function fallbackMcpDiagnostics(
  binding: McpBindingRecord,
  lastError: string | undefined,
): McpSurfaceDiagnostics {
  return {
    bindingId: binding.bindingId,
    status: "degraded",
    transport: binding.transport,
    serverNames: [...binding.serverNames],
    endpointRef: binding.endpointRef,
    toolProfileKey: binding.toolProfileKey,
    reconnectAttempts: 0,
    optional: binding.diagnostics.notes?.includes("optional") ?? false,
    lastError,
  };
}

function diffTools(
  oldTools: readonly string[],
  newTools: readonly string[],
): McpToolDiff {
  const oldSet = new Set(oldTools);
  const newSet = new Set(newTools);
  return {
    oldTools: [...oldSet].sort(),
    newTools: [...newSet].sort(),
    addedTools: [...newSet].filter((tool) => !oldSet.has(tool)).sort(),
    removedTools: [...oldSet].filter((tool) => !newSet.has(tool)).sort(),
    unchangedTools: [...newSet].filter((tool) => oldSet.has(tool)).sort(),
  };
}

function durationMs(startedMs: number, finishedAt: string): number {
  const finishedMs = Date.parse(finishedAt);
  return Number.isFinite(startedMs) && Number.isFinite(finishedMs)
    ? Math.max(0, finishedMs - startedMs)
    : 0;
}
