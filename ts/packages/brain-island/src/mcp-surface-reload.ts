import type {
  McpBindingRecord,
  McpSurfaceDiagnostics,
} from "@rusty-crew/contracts";
import type { McpToolDiscoveryClient } from "@rusty-crew/adapter-mcp";
import {
  discoverMcpToolCandidates,
  type McpSurfaceManager,
} from "@rusty-crew/adapter-mcp";
import type { ToolInventoryRequest } from "./tool-registry.js";
import {
  integrateMcpToolsWithRegistry,
  type McpRegistryIntegrationReport,
} from "./mcp-tool-registry-integration.js";

export interface McpSurfaceReloadInput {
  binding: McpBindingRecord;
  manager: McpSurfaceManager;
  discoveryClient: McpToolDiscoveryClient;
  catalogId: string;
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
  const diagnostics = input.manager.diagnostics(input.binding.bindingId);

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
  const registry = integrateMcpToolsWithRegistry({
    catalogId: input.catalogId,
    candidates: discovery.candidates,
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
