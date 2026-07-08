import type { McpBindingRecord } from "@rusty-crew/contracts";
import type {
  NativeReloadMcpControlPlan,
  NativeReloadMcpControlPlanInput,
} from "@rusty-crew/native-bridge";
import type { ToolInventoryRequest } from "./tool-registry.js";
import type {
  McpSurfaceManagerPort,
  McpToolDiscoveryClient,
} from "./service-adapter-ports.js";
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
import type { PortableToolMetadataPolicyValidator } from "./mcp-tool-registry-integration.js";

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
  planReloadMcpControl(
    input: NativeReloadMcpControlPlanInput,
  ): Promise<NativeReloadMcpControlPlan> | NativeReloadMcpControlPlan;
  manager: McpSurfaceManagerPort;
  discoveryClient: McpToolDiscoveryClient;
  discoveryClientForBinding?(
    binding: McpBindingRecord,
    command: AdminControlCommand,
  ): McpToolDiscoveryClient | undefined;
  metadataPolicyValidator: PortableToolMetadataPolicyValidator;
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
  afterReload?(input: {
    binding: McpBindingRecord;
    command: AdminControlCommand;
    report: McpSurfaceReloadReport;
    outcome: AdminControlOutcome;
  }):
    | Promise<AdminControlOutcome | undefined>
    | AdminControlOutcome
    | undefined;
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
    const plan = await options.planReloadMcpControl({
      command: {
        commandKind: command.name,
        targetSessionId: sessionId,
        requestId: command.requestId,
        idempotencyKey: command.idempotencyKey,
        operatorReason: reason,
        operatorReasonCode: command.reasonCode,
      },
      binding: binding ? reloadPlanBinding(binding) : undefined,
      reloadHandlerAvailable: true,
    });
    if (!plan.accepted) {
      return failed(
        plan.denial?.reasonCode ?? "reload_mcp_plan_denied",
        plan.denial?.summary ?? "Rust reload-MCP planner denied the control.",
      );
    }
    if (
      !plan.actions.some((action) => action.action === "reload_mcp_surface")
    ) {
      return failed(
        "reload_mcp_plan_missing_action",
        "Rust reload-MCP planner accepted without a reload action.",
      );
    }
    if (!binding) {
      return failed(
        "reload_mcp_plan_missing_binding",
        "Rust reload-MCP planner accepted without an executable binding.",
      );
    }
    if (binding.sessionId !== plan.target.sessionId) {
      return failed(
        "reload_mcp_plan_session_mismatch",
        "Rust reload-MCP planner target does not match the executable binding.",
      );
    }
    if (binding.bindingId !== plan.target.bindingId) {
      return failed(
        "reload_mcp_plan_binding_mismatch",
        "Rust reload-MCP planner binding target does not match the executable binding.",
      );
    }

    await audit(options, {
      phase: "reload_started",
      sessionId,
      bindingId: binding.bindingId,
      reason,
    });

    const discoveryClient =
      options.discoveryClientForBinding?.(binding, command) ??
      options.discoveryClient;
    const report = await reloadMcpSurface({
      binding,
      manager: options.manager,
      discoveryClient,
      catalogId: options.catalogId(binding, command),
      metadataPolicyValidator: options.metadataPolicyValidator,
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

    const outcome = {
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
    if (outcome.status === "completed") {
      const followUp = await options.afterReload?.({
        binding,
        command,
        report,
        outcome,
      });
      return followUp ?? outcome;
    }
    return outcome;
  };
}

function reloadPlanBinding(
  binding: McpBindingRecord,
): NativeReloadMcpControlPlanInput["binding"] | undefined {
  if (!binding.sessionId) {
    return undefined;
  }
  return {
    bindingId: binding.bindingId,
    sessionId: binding.sessionId,
    profileId: binding.profileId,
    ...(binding.toolProfileKey
      ? { toolProfileKey: binding.toolProfileKey }
      : {}),
    ...(binding.endpointRef ? { endpointRef: binding.endpointRef } : {}),
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
