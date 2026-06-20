import { createHash } from "node:crypto";
import type {
  AgentMessage,
  BodyState,
  SessionId,
  SessionState,
  ToolDescriptor,
} from "@rusty-crew/contracts";
import type { AdapterDiagnosticsProjection } from "./adapter-diagnostics.js";
import type { AdminRecentEvent } from "./admin-diagnostics-api.js";
import type { BrainRoleAssembly } from "./index.js";
import type { LoadedProfileContext } from "./profile-loading.js";
import type { RuntimeDiagnosticsProjection } from "./runtime-diagnostics.js";
import {
  buildToolContextDiagnosticsReport,
  type TextSurfaceSummary,
  type ToolContextDiagnosticsReport,
} from "./tool-context-diagnostics.js";
import type {
  ProfileToolPolicy,
  SessionToolConstraints,
  ToolProfileSelection,
} from "./tool-profile-selection.js";
import type { ToolRegistryDiagnosticsReport } from "./tool-registry-diagnostics.js";

export type DirectDebugErrorCode =
  | "not_found"
  | "forbidden"
  | "invalid_input"
  | "failed_precondition"
  | "internal_error";

export type DirectDebugResult<T> =
  | {
      ok: true;
      data: T;
    }
  | {
      ok: false;
      error: {
        code: DirectDebugErrorCode;
        reasonCode: string;
        message: string;
        retryable: boolean;
      };
    };

export interface DirectDebugSessionSource {
  session: SessionState;
  bodyState?: BodyState;
  systemPrompt?: string;
  roleAssembly?: BrainRoleAssembly;
  profileContext?: LoadedProfileContext;
  toolDiagnostics?: ToolRegistryDiagnosticsReport;
  toolSelection?: ToolProfileSelection;
  toolPolicy?: ProfileToolPolicy;
  sessionConstraints?: SessionToolConstraints;
  toolContext?: ToolContextDiagnosticsReport;
}

export interface DirectDebugServiceContext {
  diagnostics: RuntimeDiagnosticsProjection;
  sessions: readonly DirectDebugSessionSource[];
  adapters?: AdapterDiagnosticsProjection;
  recentEvents?: readonly AdminRecentEvent[];
  allowDirectTurnInjection?: boolean;
  allowRawPromptText?: boolean;
  turnExecutor?: DirectDebugTurnExecutor;
  now?: () => string;
  maxPendingMessages?: number;
  maxRecentEvents?: number;
}

export interface DirectDebugInspectRequest {
  sessionId: SessionId | string;
  includePromptText?: boolean;
  includeMessageBodies?: boolean;
  maxPendingMessages?: number;
  maxRecentEvents?: number;
}

export interface DirectDebugSessionView {
  generatedAt: string;
  source: "direct_debug";
  session: DirectDebugSessionSummary;
  diagnostics?: DirectDebugRuntimeSummary;
  selectedTools: readonly ToolDescriptor[];
  toolContext?: ToolContextDiagnosticsReport;
  context: DirectDebugContextView;
  pendingMessages: readonly DirectDebugMessageSummary[];
  recentEvents: readonly DirectDebugRecentEventSummary[];
  controls: DirectDebugControlSummary;
}

export interface DirectDebugSessionSummary {
  sessionId: string;
  agentId: string;
  profileId: string;
  kind: string;
  status: string;
  brainTurnCount: number;
  createdAt: string;
  lastActiveAt: string;
  toolCount: number;
  workdir?: string;
}

export interface DirectDebugRuntimeSummary {
  stale: boolean;
  health: string;
  degraded: boolean;
  reasonCodes: readonly string[];
  issueMessages: readonly string[];
}

export interface DirectDebugContextView {
  rawPromptIncluded: boolean;
  rawPromptDeniedReason?: string;
  systemPrompt: TextSurfaceSummary & { text?: string };
  instructions: TextSurfaceSummary & { text?: string };
  sections: readonly string[];
  initialMessages: {
    count: number;
    totalChars: number;
  };
  skills: readonly {
    slug: string;
    title?: string;
    summary?: string;
    tags: readonly string[];
  }[];
}

export interface DirectDebugMessageSummary {
  from: string;
  to: string;
  correlationId?: string;
  bodyChars: number;
  bodySha256: string;
  bodyPreview?: string;
}

export interface DirectDebugRecentEventSummary {
  id: string | number;
  createdAt: string;
  source: string;
  eventType: string;
  severity?: string;
  summary: string;
}

export interface DirectDebugControlSummary {
  directTurnInjection: "available" | "disabled" | "missing_executor";
  reason: string;
}

export interface DirectDebugTurnRequest {
  sessionId: SessionId | string;
  actorId: string;
  body: string;
  reason?: string;
  requestId?: string;
  idempotencyKey?: string;
}

export interface DirectDebugTurnExecutorInput {
  source: "direct_debug";
  session: SessionState;
  actorId: string;
  body: string;
  reason?: string;
  requestId: string;
  idempotencyKey?: string;
  observedAt: string;
}

export interface DirectDebugTurnOutcome {
  status: "accepted" | "rejected";
  summary: string;
  wakeId?: string;
  messageId?: string;
  reasonCode?: string;
}

export interface DirectDebugTurnExecutor {
  submitDirectDebugTurn(
    input: DirectDebugTurnExecutorInput,
  ): Promise<DirectDebugTurnOutcome> | DirectDebugTurnOutcome;
}

export function inspectDirectDebugSession(
  request: DirectDebugInspectRequest,
  context: DirectDebugServiceContext,
): DirectDebugResult<DirectDebugSessionView> {
  const source = findSessionSource(context, request.sessionId);
  if (!source) {
    return failure("not_found", "debug_session_not_found", {
      message: `debug session ${String(request.sessionId)} was not found`,
      retryable: false,
    });
  }

  const toolContext = toolContextForSource(source, context);
  return {
    ok: true,
    data: {
      generatedAt: context.now?.() ?? new Date().toISOString(),
      source: "direct_debug",
      session: sessionSummary(source.session),
      diagnostics: runtimeSummary(source.session.sessionId, context),
      selectedTools: source.session.toolProfile.tools,
      toolContext,
      context: contextView(source, toolContext, request, context),
      pendingMessages: pendingMessages(source, request, context),
      recentEvents: recentEvents(source.session.sessionId, request, context),
      controls: controlSummary(context),
    },
  };
}

export async function requestDirectDebugTurn(
  request: DirectDebugTurnRequest,
  context: DirectDebugServiceContext,
): Promise<DirectDebugResult<DirectDebugTurnOutcome>> {
  const source = findSessionSource(context, request.sessionId);
  if (!source) {
    return failure("not_found", "debug_session_not_found", {
      message: `debug session ${String(request.sessionId)} was not found`,
      retryable: false,
    });
  }
  if (!context.allowDirectTurnInjection) {
    return failure("forbidden", "direct_turn_injection_disabled", {
      message: "direct debug turn injection is disabled",
      retryable: false,
    });
  }
  if (!context.turnExecutor) {
    return failure("failed_precondition", "direct_turn_executor_missing", {
      message: "direct debug turn executor is not configured",
      retryable: true,
    });
  }
  if (source.session.status === "archived") {
    return failure("failed_precondition", "debug_session_archived", {
      message: `session ${source.session.sessionId} is archived`,
      retryable: false,
    });
  }
  const body = request.body.trim();
  if (!body) {
    return failure("invalid_input", "empty_debug_turn", {
      message: "direct debug turn body is empty",
      retryable: false,
    });
  }

  try {
    const outcome = await context.turnExecutor.submitDirectDebugTurn({
      source: "direct_debug",
      session: source.session,
      actorId: request.actorId,
      body: redactText(body),
      reason: request.reason,
      requestId: request.requestId ?? "direct-debug-turn",
      idempotencyKey: request.idempotencyKey,
      observedAt: context.now?.() ?? new Date().toISOString(),
    });
    return { ok: true, data: outcome };
  } catch (error) {
    return failure("internal_error", "direct_turn_executor_failed", {
      message: errorMessage(error, "direct debug turn executor failed"),
      retryable: true,
    });
  }
}

function findSessionSource(
  context: DirectDebugServiceContext,
  sessionId: SessionId | string,
): DirectDebugSessionSource | undefined {
  return context.sessions.find(
    (source) => source.session.sessionId === sessionId,
  );
}

function toolContextForSource(
  source: DirectDebugSessionSource,
  context: DirectDebugServiceContext,
): ToolContextDiagnosticsReport | undefined {
  if (source.toolContext) {
    return source.toolContext;
  }
  if (!source.toolDiagnostics) {
    return undefined;
  }
  return buildToolContextDiagnosticsReport({
    now: context.now?.() ?? new Date().toISOString(),
    session: {
      sessionId: source.session.sessionId,
      agentId: source.session.agentId,
      profileId: source.session.profileId,
      kind: source.session.kind,
    },
    toolDiagnostics: source.toolDiagnostics,
    toolSelection: source.toolSelection,
    profileContext: source.profileContext,
    toolPolicy: source.toolPolicy,
    sessionConstraints: source.sessionConstraints,
    roleAssembly: source.roleAssembly,
    systemPrompt: source.systemPrompt,
    resourceLimits: source.session.resourceLimits,
    adapters: context.adapters,
  });
}

function sessionSummary(session: SessionState): DirectDebugSessionSummary {
  return {
    sessionId: session.sessionId,
    agentId: session.agentId,
    profileId: session.profileId,
    kind: session.kind,
    status: session.status,
    brainTurnCount: session.brainTurnCount,
    createdAt: session.createdAt,
    lastActiveAt: session.lastActiveAt,
    toolCount: session.toolProfile.tools.length,
    workdir: session.resourceLimits.workdir,
  };
}

function runtimeSummary(
  sessionId: SessionId,
  context: DirectDebugServiceContext,
): DirectDebugRuntimeSummary | undefined {
  const session = context.diagnostics.runtime.sessions.find(
    (candidate) => candidate.sessionId === sessionId,
  );
  if (!session) {
    return undefined;
  }
  return {
    stale: session.stale,
    health: context.diagnostics.health,
    degraded: context.diagnostics.degraded,
    reasonCodes: context.diagnostics.reasonCodes,
    issueMessages: context.diagnostics.issues
      .filter((issue) => issue.sessionId === sessionId || !issue.sessionId)
      .map((issue) => redactText(issue.message))
      .slice(0, 8),
  };
}

function contextView(
  source: DirectDebugSessionSource,
  toolContext: ToolContextDiagnosticsReport | undefined,
  request: DirectDebugInspectRequest,
  service: DirectDebugServiceContext,
): DirectDebugContextView {
  const rawPromptAllowed =
    request.includePromptText === true && service.allowRawPromptText === true;
  const instructions = source.roleAssembly?.instructions;
  return {
    rawPromptIncluded: rawPromptAllowed,
    rawPromptDeniedReason: rawPromptAllowed
      ? undefined
      : "raw prompt text is disabled for direct debug by default",
    systemPrompt: {
      ...textSummary(source.systemPrompt),
      text: rawPromptAllowed
        ? redactText(source.systemPrompt ?? "")
        : undefined,
    },
    instructions: {
      ...textSummary(instructions),
      text: rawPromptAllowed ? redactText(instructions ?? "") : undefined,
    },
    sections:
      toolContext?.context.sections ??
      sectionHeadings(source.roleAssembly?.instructions),
    initialMessages: {
      count: source.roleAssembly?.initialMessages?.length ?? 0,
      totalChars:
        source.roleAssembly?.initialMessages?.reduce(
          (sum, message) => sum + JSON.stringify(message).length,
          0,
        ) ?? 0,
    },
    skills:
      toolContext?.context.skills.map((skill) => ({
        slug: skill.slug,
        title: skill.title,
        summary: skill.summary,
        tags: skill.tags,
      })) ??
      source.profileContext?.skills.map((skill) => ({
        slug: skill.slug,
        title: skill.title,
        summary: skill.summary,
        tags: skill.tags,
      })) ??
      [],
  };
}

function pendingMessages(
  source: DirectDebugSessionSource,
  request: DirectDebugInspectRequest,
  context: DirectDebugServiceContext,
): DirectDebugMessageSummary[] {
  const limit = request.maxPendingMessages ?? context.maxPendingMessages ?? 20;
  return (source.bodyState?.pendingMessages ?? [])
    .slice(0, limit)
    .map((message) =>
      messageSummary(message, request.includeMessageBodies === true),
    );
}

function messageSummary(
  message: AgentMessage,
  includeBody: boolean,
): DirectDebugMessageSummary {
  return {
    from: message.from,
    to: message.to,
    correlationId: message.correlationId,
    bodyChars: message.body.length,
    bodySha256: sha256(message.body),
    bodyPreview: includeBody
      ? boundedPreview(redactText(message.body))
      : undefined,
  };
}

function recentEvents(
  sessionId: SessionId,
  request: DirectDebugInspectRequest,
  context: DirectDebugServiceContext,
): DirectDebugRecentEventSummary[] {
  const limit = request.maxRecentEvents ?? context.maxRecentEvents ?? 20;
  return (context.recentEvents ?? [])
    .filter((event) =>
      event.workRef && typeof event.workRef === "object"
        ? event.workRef["sessionId"] === sessionId
        : true,
    )
    .slice(0, limit)
    .map((event) => ({
      id: event.id,
      createdAt: event.createdAt,
      source: event.source,
      eventType: event.eventType,
      severity: event.severity,
      summary: boundedPreview(redactText(event.summary), 180),
    }));
}

function controlSummary(
  context: DirectDebugServiceContext,
): DirectDebugControlSummary {
  if (!context.allowDirectTurnInjection) {
    return {
      directTurnInjection: "disabled",
      reason: "direct debug inspect mode is read-only",
    };
  }
  if (!context.turnExecutor) {
    return {
      directTurnInjection: "missing_executor",
      reason: "direct debug turn executor is not configured",
    };
  }
  return {
    directTurnInjection: "available",
    reason: "direct debug turns route through the configured control executor",
  };
}

function textSummary(text: string | undefined): TextSurfaceSummary {
  return {
    present: Boolean(text),
    chars: text?.length ?? 0,
    lines: text ? text.split(/\r?\n/).length : 0,
    sha256: text ? sha256(text) : undefined,
  };
}

function sectionHeadings(text: string | undefined): readonly string[] {
  return (text ?? "")
    .split(/\r?\n/)
    .filter((line) => line.startsWith("#"))
    .map((line) => line.replace(/^#+\s*/, "").trim())
    .filter(Boolean)
    .slice(0, 20);
}

function sha256(value: string): string {
  return createHash("sha256").update(value).digest("hex");
}

function boundedPreview(text: string, maxChars = 240): string {
  return text.length > maxChars ? `${text.slice(0, maxChars - 3)}...` : text;
}

function redactText(text: string): string {
  return text
    .replace(
      /(api[_-]?key|token|password|secret)\s*[:=]\s*([^\s,;]+)/gi,
      "$1=[redacted]",
    )
    .replace(/Bearer\s+[A-Za-z0-9._~+/-]+=*/g, "Bearer [redacted]");
}

function failure<T>(
  code: DirectDebugErrorCode,
  reasonCode: string,
  options: { message: string; retryable: boolean },
): DirectDebugResult<T> {
  return {
    ok: false,
    error: {
      code,
      reasonCode,
      message: options.message,
      retryable: options.retryable,
    },
  };
}

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback;
}
