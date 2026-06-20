import type { McpBindingRecord } from "@rusty-crew/contracts";
import type { McpToolDiscoveryClient } from "@rusty-crew/adapter-mcp";
import type { ToolInventoryRequest } from "./tool-registry.js";
import type { McpSurfaceManager } from "@rusty-crew/adapter-mcp";
import {
  adapterActivity,
  type AgentActivityObservationProducer,
  type AgentActivityPublishResult,
  type AgentObservationIdentity,
} from "./agent-activity-observation.js";
import type {
  AdminControlCommand,
  AdminControlExecutor,
  AdminControlOutcome,
} from "./admin-control-api.js";
import {
  reloadMcpSurface,
  type McpSurfaceReloadReport,
} from "./mcp-surface-reload.js";

export type ReloadMcpLifecyclePhase =
  | "reload_started"
  | "reloaded"
  | "degraded";

export interface ReloadMcpLifecycleAuditEvent {
  phase: ReloadMcpLifecyclePhase;
  sessionId: string;
  bindingId?: string;
  reason: string;
  observedAt: string;
  report?: McpSurfaceReloadReport;
}

export interface ReloadMcpLifecycleAuditSink {
  writeReloadMcpLifecycleAudit(
    event: ReloadMcpLifecycleAuditEvent,
  ): Promise<unknown> | unknown;
}

export interface ReloadMcpControlOptions {
  resolveBinding(
    sessionId: string,
    command: AdminControlCommand,
  ): Promise<McpBindingRecord | undefined> | McpBindingRecord | undefined;
  manager: McpSurfaceManager;
  discoveryClient: McpToolDiscoveryClient;
  catalogId(binding: McpBindingRecord, command: AdminControlCommand): string;
  previousToolNames?(
    binding: McpBindingRecord,
    command: AdminControlCommand,
  ): readonly string[];
  inventoryRequest?(
    binding: McpBindingRecord,
    command: AdminControlCommand,
  ): ToolInventoryRequest | undefined;
  auditSink?: ReloadMcpLifecycleAuditSink;
  observationProducer?: AgentActivityObservationProducer;
  observationIdentity?(input: {
    binding: McpBindingRecord;
    command: AdminControlCommand;
  }): AgentObservationIdentity;
  now?: () => string;
}

export interface MemoryReloadMcpLifecycleAuditSink extends ReloadMcpLifecycleAuditSink {
  readonly events: ReloadMcpLifecycleAuditEvent[];
}

export function createReloadMcpControlExecutor(
  options: ReloadMcpControlOptions,
): NonNullable<AdminControlExecutor["reloadMcp"]> {
  return async (command) => {
    const sessionId = command.target.sessionId;
    const reason = command.reason ?? "slash command /reload-mcp";
    if (!sessionId) {
      return failed(
        "missing_session_id",
        "Cannot reload MCP without a session target.",
      );
    }

    const binding = await options.resolveBinding(sessionId, command);
    if (!binding) {
      return failed(
        "mcp_binding_not_found",
        `No MCP binding found for ${sessionId}.`,
      );
    }
    if (binding.sessionId !== sessionId) {
      return failed(
        "mcp_binding_session_mismatch",
        "Resolved MCP binding does not belong to the requested session.",
      );
    }

    await audit(options, {
      phase: "reload_started",
      sessionId,
      bindingId: binding.bindingId,
      reason,
    });

    const report = await reloadMcpSurface({
      binding,
      manager: options.manager,
      discoveryClient: options.discoveryClient,
      catalogId: options.catalogId(binding, command),
      previousToolNames: options.previousToolNames?.(binding, command),
      inventoryRequest: options.inventoryRequest?.(binding, command),
      requestedBy: command.actor.operatorId,
      reason,
      now: options.now,
    });
    const phase = report.status === "reloaded" ? "reloaded" : "degraded";
    await audit(options, {
      phase,
      sessionId,
      bindingId: binding.bindingId,
      reason,
      report,
    });

    const observation = await publishReloadObservation(
      options,
      binding,
      command,
      report,
    );

    return {
      status: report.status === "reloaded" ? "completed" : "failed",
      summary:
        report.status === "reloaded"
          ? `Reloaded MCP surface ${binding.bindingId}.`
          : `MCP surface ${binding.bindingId} is degraded after reload.`,
      affectedIds: {
        sessionId,
        bindingId: binding.bindingId,
      },
      result: {
        bindingId: report.bindingId,
        sessionId: report.sessionId,
        profileId: report.profileId,
        status: report.status,
        oldToolCount: report.toolDiff.oldTools.length,
        newToolCount: report.toolDiff.newTools.length,
        addedTools: report.toolDiff.addedTools,
        removedTools: report.toolDiff.removedTools,
        unchangedTools: report.toolDiff.unchangedTools,
        collisionCount: report.collisionCount,
        discoveryIssueCount: report.discoveryIssueCount,
        optionalServerFailures: report.optionalServerFailures,
        durationMs: report.durationMs,
        reason: report.reason,
        observation: observation?.status,
      },
      reasonCode:
        report.status === "reloaded" ? "mcp_reloaded" : "mcp_reload_degraded",
    } satisfies AdminControlOutcome;
  };
}

export function createMemoryReloadMcpLifecycleAuditSink(): MemoryReloadMcpLifecycleAuditSink {
  const events: ReloadMcpLifecycleAuditEvent[] = [];
  return {
    events,
    writeReloadMcpLifecycleAudit(event) {
      events.push(event);
    },
  };
}

function failed(reasonCode: string, summary: string): AdminControlOutcome {
  return {
    status: "failed",
    summary,
    reasonCode,
  };
}

async function audit(
  options: ReloadMcpControlOptions,
  event: Omit<ReloadMcpLifecycleAuditEvent, "observedAt">,
): Promise<void> {
  await options.auditSink?.writeReloadMcpLifecycleAudit({
    ...event,
    observedAt: options.now?.() ?? new Date().toISOString(),
  });
}

async function publishReloadObservation(
  options: ReloadMcpControlOptions,
  binding: McpBindingRecord,
  command: AdminControlCommand,
  report: McpSurfaceReloadReport,
): Promise<AgentActivityPublishResult | undefined> {
  if (!options.observationProducer || !options.observationIdentity) {
    return undefined;
  }
  return options.observationProducer.publish(
    adapterActivity({
      eventType:
        report.status === "reloaded" ? "adapter_recovered" : "adapter_degraded",
      identity: options.observationIdentity({ binding, command }),
      adapter: "mcp",
      surface: "runtime",
      reasonCode:
        report.status === "reloaded" ? "mcp_reloaded" : "mcp_reload_degraded",
      summary:
        report.status === "reloaded"
          ? `Reloaded MCP surface ${binding.bindingId}.`
          : `MCP surface ${binding.bindingId} degraded after reload.`,
    }),
  );
}
