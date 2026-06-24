import type {
  BrainImplementationId,
  DelegatedSessionRuntimeStatus,
  ProfileId,
  SessionId,
  SessionState,
} from "@rusty-crew/contracts";
import type { AdapterDiagnosticsProjection } from "./adapter-diagnostics.js";
import type { ToolRegistryDiagnosticsReport } from "./tool-registry-diagnostics.js";

export type DiagnosticsHealth = "ok" | "degraded" | "blocked";

export type DiagnosticsReasonCode =
  | "ok"
  | "degraded_adapter"
  | "missing_binding"
  | "missing_canonical_identity"
  | "stale_session"
  | "queue_backlog"
  | "expired_queue_items"
  | "tool_registry_invalid"
  | "mcp_reload_failed"
  | "persistence_pressure"
  | "observation_unavailable"
  | "blocked_dependency"
  | "recent_runtime_error"
  | "diagnostics_missing";

export interface RuntimeCounterSummary {
  brainTurns: number;
  wakes: number;
  toolCalls: number;
  toolErrors: number;
  delegationsCreated: number;
  delegationsCompleted: number;
  delegationsFailed: number;
  delegationsTimedOut: number;
  delegationsCancelled: number;
  messages: number;
  completions: number;
  queueExpirations: number;
}

export interface QueueDiagnosticsInput {
  pending: number;
  expired: number;
  discarded?: number;
  delivered?: number;
  oldestPendingAgeMs?: number;
  maxPending?: number;
  maxOldestPendingAgeMs?: number;
}

export interface PersistenceDiagnosticsInput {
  schemaVersion?: number;
  migrationCount?: number;
  databaseBytes?: number;
  maxDatabaseBytes?: number;
  tableCounts?: Record<string, number>;
  tableCountThresholds?: Record<string, number>;
  searchHealthy?: boolean;
  lastError?: string;
}

export interface ObservationDiagnosticsInput {
  enabled: boolean;
  writerAvailable: boolean;
  lastError?: string;
}

export interface RuntimeDiagnosticError {
  source: string;
  message: string;
  reasonCode?: DiagnosticsReasonCode;
  observedAt: string;
  blocked?: boolean;
}

export interface RuntimeDiagnosticsInput {
  now: string;
  runtimeSummary?: RuntimeCounterSummary;
  sessions?: readonly SessionState[];
  sessionDefaults?: ReadonlyMap<SessionId, RuntimeSessionEffectiveDefaults>;
  delegatedSessions?: readonly DelegatedSessionRuntimeStatus[];
  queues?: QueueDiagnosticsInput;
  persistence?: PersistenceDiagnosticsInput;
  adapters?: AdapterDiagnosticsProjection;
  tools?: readonly ToolRegistryDiagnosticsReport[];
  observation?: ObservationDiagnosticsInput;
  brainModules?: readonly RuntimeBrainModuleDiagnostics[];
  recentErrors?: readonly RuntimeDiagnosticError[];
  staleSessionMs?: number;
}

export interface DiagnosticsIssue {
  code: DiagnosticsReasonCode;
  severity: Exclude<DiagnosticsHealth, "ok">;
  message: string;
  source: string;
  sessionId?: SessionId;
}

export interface RuntimeDiagnosticsProjection {
  generatedAt: string;
  health: DiagnosticsHealth;
  degraded: boolean;
  reasonCodes: DiagnosticsReasonCode[];
  summary: {
    sessions: number;
    activeSessions: number;
    idleSessions: number;
    archivedSessions: number;
    delegatedSessions: number;
    blockedDelegations: number;
    pendingQueueItems: number;
    expiredQueueItems: number;
    toolErrors: number;
    recentErrors: number;
  };
  runtime: {
    counters?: RuntimeCounterSummary;
    brainModules: RuntimeBrainModuleDiagnostics[];
    sessions: RuntimeSessionDiagnostics[];
    delegatedSessions: RuntimeDelegationDiagnostics[];
  };
  queues?: QueueDiagnosticsProjection;
  persistence?: PersistenceDiagnosticsProjection;
  adapters?: AdapterDiagnosticsProjection;
  tools: ToolDiagnosticsProjection[];
  observation?: ObservationDiagnosticsProjection;
  issues: DiagnosticsIssue[];
}

export interface RuntimeSessionDiagnostics {
  sessionId: SessionId;
  agentId: string;
  profileId: string;
  kind: string;
  status: SessionState["status"];
  toolCount: number;
  brainTurnCount: number;
  lastActiveAt: string;
  stale: boolean;
  effectiveDefaults?: RuntimeSessionEffectiveDefaults;
}

export interface RuntimeBrainModuleDiagnostics {
  profileId: ProfileId | string;
  implementationId: BrainImplementationId | string;
  moduleId: string;
  strategy?: string;
  effectiveStrategy?: string;
  providerStateMode?: string;
  selectedToolCount: number;
  selectedToolSource: string;
  toolAdapterStatus: string;
}

export interface RuntimeSessionEffectiveDefaults {
  ownerId?: string;
  maxHistoryMessages?: number;
  turnTimeoutMs?: number;
  wakeTimeoutMs?: number;
}

export interface RuntimeDelegationDiagnostics {
  sessionId: SessionId;
  parentSessionId?: SessionId;
  runId?: string;
  runStatus?: string;
  terminal: boolean;
  blocked: boolean;
}

export interface QueueDiagnosticsProjection extends QueueDiagnosticsInput {
  backlog: boolean;
}

export interface PersistenceDiagnosticsProjection extends PersistenceDiagnosticsInput {
  pressure: boolean;
}

export interface ToolDiagnosticsProjection {
  catalogId: string;
  registeredTools: number;
  selectedTools: number;
  validationErrors: number;
  validationWarnings: number;
  invalid: boolean;
}

export interface ObservationDiagnosticsProjection extends ObservationDiagnosticsInput {
  degraded: boolean;
}

export function buildRuntimeDiagnosticsProjection(
  input: RuntimeDiagnosticsInput,
): RuntimeDiagnosticsProjection {
  const staleSessionMs = input.staleSessionMs ?? 15 * 60 * 1000;
  const sessions = (input.sessions ?? []).map((session) =>
    sessionDiagnostics(
      session,
      input.now,
      staleSessionMs,
      input.sessionDefaults?.get(session.sessionId),
    ),
  );
  const delegatedSessions = (input.delegatedSessions ?? []).map(
    delegationDiagnostics,
  );
  const queues = input.queues ? queueDiagnostics(input.queues) : undefined;
  const persistence = input.persistence
    ? persistenceDiagnostics(input.persistence)
    : undefined;
  const tools = (input.tools ?? []).map(toolDiagnostics);
  const observation = input.observation
    ? observationDiagnostics(input.observation)
    : undefined;

  const issues = [
    ...sessionIssues(sessions),
    ...delegationIssues(delegatedSessions),
    ...queueIssues(queues),
    ...persistenceIssues(persistence),
    ...adapterIssues(input.adapters),
    ...toolIssues(tools),
    ...observationIssues(observation),
    ...runtimeErrorIssues(input.recentErrors ?? []),
    ...missingInputIssues(input),
  ];
  const health = summarizeHealth(issues);
  const reasonCodes = uniqueReasonCodes(issues);

  return {
    generatedAt: input.now,
    health,
    degraded: health !== "ok",
    reasonCodes: reasonCodes.length > 0 ? reasonCodes : ["ok"],
    summary: {
      sessions: sessions.length,
      activeSessions: sessions.filter((session) => session.status === "active")
        .length,
      idleSessions: sessions.filter((session) => session.status === "idle")
        .length,
      archivedSessions: sessions.filter(
        (session) => session.status === "archived",
      ).length,
      delegatedSessions: delegatedSessions.length,
      blockedDelegations: delegatedSessions.filter(
        (delegation) => delegation.blocked,
      ).length,
      pendingQueueItems: queues?.pending ?? 0,
      expiredQueueItems: queues?.expired ?? 0,
      toolErrors: input.runtimeSummary?.toolErrors ?? 0,
      recentErrors: input.recentErrors?.length ?? 0,
    },
    runtime: {
      counters: input.runtimeSummary,
      brainModules: [...(input.brainModules ?? [])],
      sessions,
      delegatedSessions,
    },
    queues,
    persistence,
    adapters: input.adapters,
    tools,
    observation,
    issues,
  };
}

function sessionDiagnostics(
  session: SessionState,
  now: string,
  staleSessionMs: number,
  effectiveDefaults: RuntimeSessionEffectiveDefaults | undefined,
): RuntimeSessionDiagnostics {
  const ageMs = Date.parse(now) - Date.parse(session.lastActiveAt);
  return {
    sessionId: session.sessionId,
    agentId: session.agentId,
    profileId: session.profileId,
    kind: session.kind,
    status: session.status,
    toolCount: session.toolProfile.tools.length,
    brainTurnCount: session.brainTurnCount,
    lastActiveAt: session.lastActiveAt,
    stale:
      session.status !== "archived" &&
      Number.isFinite(ageMs) &&
      ageMs > staleSessionMs,
    effectiveDefaults,
  };
}

function delegationDiagnostics(
  status: DelegatedSessionRuntimeStatus,
): RuntimeDelegationDiagnostics {
  const blocked =
    status.runStatus === "blocked" ||
    status.runStatus === "failed" ||
    status.runStatus === "exhausted" ||
    status.runStatus === "expired";
  return {
    sessionId: status.session.sessionId,
    parentSessionId: status.parentSessionId,
    runId: status.runId,
    runStatus: status.runStatus,
    terminal: status.terminal,
    blocked,
  };
}

function queueDiagnostics(
  input: QueueDiagnosticsInput,
): QueueDiagnosticsProjection {
  return {
    ...input,
    backlog:
      input.pending > (input.maxPending ?? 32) ||
      (input.oldestPendingAgeMs ?? 0) > (input.maxOldestPendingAgeMs ?? 60_000),
  };
}

function persistenceDiagnostics(
  input: PersistenceDiagnosticsInput,
): PersistenceDiagnosticsProjection {
  const databasePressure =
    input.databaseBytes !== undefined &&
    input.maxDatabaseBytes !== undefined &&
    input.databaseBytes > input.maxDatabaseBytes;
  const tablePressure = Object.entries(input.tableCounts ?? {}).some(
    ([table, count]) => {
      const threshold = input.tableCountThresholds?.[table];
      return threshold !== undefined && count > threshold;
    },
  );
  return {
    ...input,
    pressure:
      databasePressure ||
      tablePressure ||
      input.searchHealthy === false ||
      input.lastError !== undefined,
  };
}

function toolDiagnostics(
  report: ToolRegistryDiagnosticsReport,
): ToolDiagnosticsProjection {
  return {
    catalogId: report.catalogId,
    registeredTools: report.summary.registeredTools,
    selectedTools: report.summary.selectedTools,
    validationErrors: report.summary.validationErrors,
    validationWarnings: report.summary.validationWarnings,
    invalid: !report.validation.ok || report.summary.validationErrors > 0,
  };
}

function observationDiagnostics(
  input: ObservationDiagnosticsInput,
): ObservationDiagnosticsProjection {
  return {
    ...input,
    degraded:
      input.enabled && (!input.writerAvailable || Boolean(input.lastError)),
  };
}

function sessionIssues(
  sessions: readonly RuntimeSessionDiagnostics[],
): DiagnosticsIssue[] {
  return sessions.flatMap((session) =>
    session.stale
      ? [
          {
            code: "stale_session" as const,
            severity: "degraded" as const,
            source: "runtime.sessions",
            sessionId: session.sessionId,
            message: `session ${session.sessionId} has not been active since ${session.lastActiveAt}`,
          },
        ]
      : [],
  );
}

function delegationIssues(
  delegations: readonly RuntimeDelegationDiagnostics[],
): DiagnosticsIssue[] {
  return delegations.flatMap((delegation) =>
    delegation.blocked
      ? [
          {
            code: "blocked_dependency" as const,
            severity: "blocked" as const,
            source: "runtime.delegations",
            sessionId: delegation.sessionId,
            message: `delegated session ${delegation.sessionId} is ${delegation.runStatus}`,
          },
        ]
      : [],
  );
}

function queueIssues(
  queues: QueueDiagnosticsProjection | undefined,
): DiagnosticsIssue[] {
  if (!queues) return [];
  return [
    ...(queues.backlog
      ? [
          {
            code: "queue_backlog" as const,
            severity: "degraded" as const,
            source: "runtime.queues",
            message: `${queues.pending} queued messages are pending`,
          },
        ]
      : []),
    ...(queues.expired > 0
      ? [
          {
            code: "expired_queue_items" as const,
            severity: "degraded" as const,
            source: "runtime.queues",
            message: `${queues.expired} queued messages expired`,
          },
        ]
      : []),
  ];
}

function persistenceIssues(
  persistence: PersistenceDiagnosticsProjection | undefined,
): DiagnosticsIssue[] {
  return persistence?.pressure
    ? [
        {
          code: "persistence_pressure",
          severity: "degraded",
          source: "runtime.persistence",
          message: persistence.lastError ?? "persistence thresholds exceeded",
        },
      ]
    : [];
}

function adapterIssues(
  adapters: AdapterDiagnosticsProjection | undefined,
): DiagnosticsIssue[] {
  if (!adapters) return [];
  return [
    ...(adapters.channels.degradedBindings > 0
      ? [
          {
            code: "degraded_adapter" as const,
            severity: "degraded" as const,
            source: "adapters.channels",
            message: `${adapters.channels.degradedBindings} channel bindings are degraded`,
          },
        ]
      : []),
    ...(adapters.mcp.degradedSurfaces > 0
      ? [
          {
            code: "mcp_reload_failed" as const,
            severity: "degraded" as const,
            source: "adapters.mcp",
            message: `${adapters.mcp.degradedSurfaces} MCP surfaces are degraded`,
          },
        ]
      : []),
    ...(adapters.channels.bindings.some(
      (binding) => binding.status === "missing",
    )
      ? [
          {
            code: "missing_binding" as const,
            severity: "degraded" as const,
            source: "adapters.channels",
            message: "one or more channel binding diagnostics are missing",
          },
        ]
      : []),
  ];
}

function toolIssues(
  tools: readonly ToolDiagnosticsProjection[],
): DiagnosticsIssue[] {
  return tools.flatMap((tool) =>
    tool.invalid
      ? [
          {
            code: "tool_registry_invalid" as const,
            severity: "blocked" as const,
            source: `tools.${tool.catalogId}`,
            message: `tool catalog ${tool.catalogId} has ${tool.validationErrors} validation errors`,
          },
        ]
      : [],
  );
}

function observationIssues(
  observation: ObservationDiagnosticsProjection | undefined,
): DiagnosticsIssue[] {
  return observation?.degraded
    ? [
        {
          code: "observation_unavailable",
          severity: "degraded",
          source: "observation",
          message: observation.lastError ?? "observation writer is unavailable",
        },
      ]
    : [];
}

function runtimeErrorIssues(
  errors: readonly RuntimeDiagnosticError[],
): DiagnosticsIssue[] {
  return errors.map((error) => ({
    code: error.reasonCode ?? "recent_runtime_error",
    severity: error.blocked ? "blocked" : "degraded",
    source: error.source,
    message: error.message,
  }));
}

function missingInputIssues(
  input: RuntimeDiagnosticsInput,
): DiagnosticsIssue[] {
  return [
    ...(input.sessions === undefined
      ? [
          {
            code: "diagnostics_missing" as const,
            severity: "degraded" as const,
            source: "runtime.sessions",
            message: "session diagnostics were not supplied",
          },
        ]
      : []),
    ...(input.runtimeSummary === undefined
      ? [
          {
            code: "diagnostics_missing" as const,
            severity: "degraded" as const,
            source: "runtime.counters",
            message: "runtime counter summary was not supplied",
          },
        ]
      : []),
  ];
}

function summarizeHealth(
  issues: readonly DiagnosticsIssue[],
): DiagnosticsHealth {
  if (issues.some((issue) => issue.severity === "blocked")) return "blocked";
  if (issues.some((issue) => issue.severity === "degraded")) return "degraded";
  return "ok";
}

function uniqueReasonCodes(
  issues: readonly DiagnosticsIssue[],
): DiagnosticsReasonCode[] {
  return [...new Set(issues.map((issue) => issue.code))].sort();
}
