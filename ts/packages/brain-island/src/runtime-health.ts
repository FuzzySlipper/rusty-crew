import type {
  DiagnosticsHealth,
  DiagnosticsIssue,
  DiagnosticsReasonCode,
  RuntimeDiagnosticsProjection,
} from "./runtime-diagnostics.js";

export type RuntimeHealthDomain = "internal" | "external";

export interface RuntimeHealthProbe {
  ok: boolean;
  generatedAt: string;
  health: DiagnosticsHealth;
  degraded: boolean;
  reasonCodes: DiagnosticsReasonCode[];
  summary: string;
}

export interface RuntimeReadinessProbe extends RuntimeHealthProbe {
  ready: boolean;
  blockingReasonCodes: DiagnosticsReasonCode[];
}

export interface RuntimeDegradedStatus {
  degraded: boolean;
  health: DiagnosticsHealth;
  reasonCodes: DiagnosticsReasonCode[];
  internal: RuntimeHealthDomainStatus;
  external: RuntimeHealthDomainStatus;
}

export interface RuntimeHealthDomainStatus {
  health: DiagnosticsHealth;
  degraded: boolean;
  reasonCodes: DiagnosticsReasonCode[];
  issues: DiagnosticsIssue[];
}

export interface RuntimeMetricSample {
  name: string;
  value: number;
  labels?: Record<string, string>;
}

export interface RuntimeHealthProjection {
  generatedAt: string;
  liveness: RuntimeHealthProbe;
  readiness: RuntimeReadinessProbe;
  degradedStatus: RuntimeDegradedStatus;
  metrics: RuntimeMetricSample[];
}

export function buildRuntimeHealthProjection(
  diagnostics: RuntimeDiagnosticsProjection,
): RuntimeHealthProjection {
  const internal = domainStatus(
    diagnostics.issues.filter((issue) => issueDomain(issue) === "internal"),
  );
  const external = domainStatus(
    diagnostics.issues.filter((issue) => issueDomain(issue) === "external"),
  );
  const blockingReasonCodes = internal.issues
    .filter(
      (issue) =>
        issue.severity === "blocked" || issue.code === "diagnostics_missing",
    )
    .map((issue) => issue.code);
  const ready = blockingReasonCodes.length === 0;

  return {
    generatedAt: diagnostics.generatedAt,
    liveness: {
      ok: true,
      generatedAt: diagnostics.generatedAt,
      health: "ok",
      degraded: false,
      reasonCodes: ["ok"],
      summary: "process is live",
    },
    readiness: {
      ok: ready,
      ready,
      generatedAt: diagnostics.generatedAt,
      health: ready ? internal.health : "blocked",
      degraded: diagnostics.degraded,
      reasonCodes:
        blockingReasonCodes.length > 0
          ? uniqueReasonCodes(blockingReasonCodes)
          : internal.reasonCodes,
      blockingReasonCodes: uniqueReasonCodes(blockingReasonCodes),
      summary: ready
        ? diagnostics.degraded
          ? "runtime is ready with degraded dependencies"
          : "runtime is ready"
        : "runtime is not ready",
    },
    degradedStatus: {
      degraded: diagnostics.degraded,
      health: diagnostics.health,
      reasonCodes: diagnostics.reasonCodes,
      internal,
      external,
    },
    metrics: runtimeMetrics(diagnostics, internal, external),
  };
}

export function issueDomain(issue: DiagnosticsIssue): RuntimeHealthDomain {
  if (issue.source.startsWith("adapters.") || issue.source === "observation") {
    return "external";
  }
  return "internal";
}

function domainStatus(
  issues: readonly DiagnosticsIssue[],
): RuntimeHealthDomainStatus {
  const health = summarizeHealth(issues);
  const reasonCodes = uniqueReasonCodes(issues.map((issue) => issue.code));
  return {
    health,
    degraded: health !== "ok",
    reasonCodes: reasonCodes.length > 0 ? reasonCodes : ["ok"],
    issues: [...issues],
  };
}

function runtimeMetrics(
  diagnostics: RuntimeDiagnosticsProjection,
  internal: RuntimeHealthDomainStatus,
  external: RuntimeHealthDomainStatus,
): RuntimeMetricSample[] {
  const counters = diagnostics.runtime.counters;
  return [
    metric("rusty_crew_runtime_health_degraded", diagnostics.degraded ? 1 : 0),
    metric(
      "rusty_crew_runtime_health_blocked",
      diagnostics.health === "blocked" ? 1 : 0,
    ),
    metric("rusty_crew_internal_health_degraded", internal.degraded ? 1 : 0),
    metric("rusty_crew_external_health_degraded", external.degraded ? 1 : 0),
    metric("rusty_crew_sessions_total", diagnostics.summary.sessions),
    metric("rusty_crew_sessions_active", diagnostics.summary.activeSessions),
    metric("rusty_crew_sessions_idle", diagnostics.summary.idleSessions),
    metric(
      "rusty_crew_sessions_archived",
      diagnostics.summary.archivedSessions,
    ),
    metric(
      "rusty_crew_delegations_total",
      diagnostics.summary.delegatedSessions,
    ),
    metric(
      "rusty_crew_delegations_blocked",
      diagnostics.summary.blockedDelegations,
    ),
    metric("rusty_crew_queue_pending", diagnostics.summary.pendingQueueItems),
    metric("rusty_crew_queue_expired", diagnostics.summary.expiredQueueItems),
    metric("rusty_crew_recent_errors", diagnostics.summary.recentErrors),
    metric("rusty_crew_tool_errors_total", diagnostics.summary.toolErrors),
    metric(
      "rusty_crew_adapter_channel_degraded_bindings",
      diagnostics.adapters?.channels.degradedBindings ?? 0,
    ),
    metric(
      "rusty_crew_adapter_mcp_degraded_surfaces",
      diagnostics.adapters?.mcp.degradedSurfaces ?? 0,
    ),
    metric(
      "rusty_crew_observation_writer_available",
      diagnostics.observation?.writerAvailable === false ? 0 : 1,
    ),
    ...(diagnostics.persistence?.databaseBytes !== undefined
      ? [
          metric(
            "rusty_crew_persistence_database_bytes",
            diagnostics.persistence.databaseBytes,
          ),
        ]
      : []),
    ...(counters
      ? [
          metric("rusty_crew_brain_turns_total", counters.brainTurns),
          metric("rusty_crew_wakes_total", counters.wakes),
          metric("rusty_crew_tool_calls_total", counters.toolCalls),
          metric("rusty_crew_messages_total", counters.messages),
          metric("rusty_crew_completions_total", counters.completions),
          metric(
            "rusty_crew_queue_expirations_total",
            counters.queueExpirations,
          ),
        ]
      : []),
  ];
}

function metric(name: string, value: number): RuntimeMetricSample {
  return { name, value };
}

function summarizeHealth(
  issues: readonly DiagnosticsIssue[],
): DiagnosticsHealth {
  if (issues.some((issue) => issue.severity === "blocked")) return "blocked";
  if (issues.some((issue) => issue.severity === "degraded")) return "degraded";
  return "ok";
}

function uniqueReasonCodes(
  reasonCodes: readonly DiagnosticsReasonCode[],
): DiagnosticsReasonCode[] {
  return [...new Set(reasonCodes)].sort();
}
