import type {
  LogicalTurnAttentionResolutionReceipt,
  LogicalTurnCancellationReceipt,
  LogicalTurnDiagnosticPage,
  LogicalTurnDiagnosticQuery,
  LogicalTurnResolutionAction,
} from "@rusty-crew/contracts";
import type { AdminRouteResult } from "./admin-diagnostics-api.js";
import { failure, successRoute } from "./service-route-results.js";

export interface LogicalTurnRouteRequest {
  method?: string;
  url: URL;
  body?: unknown;
  requestId: string;
  idempotencyKey?: string;
}

export interface LogicalTurnRouteContext {
  logicalTurnDiagnostics(
    query: LogicalTurnDiagnosticQuery,
  ): Promise<LogicalTurnDiagnosticPage>;
  resolveLogicalTurnAttention(input: {
    logicalTurnId: string;
    expectedRevision: number;
    action: LogicalTurnResolutionAction;
  }): Promise<LogicalTurnAttentionResolutionReceipt>;
  cancelLogicalTurn(input: {
    logicalTurnId: string;
    expectedRevision: number;
    idempotencyKey: string;
    reasonCode: string;
    summary: string;
    now: string;
  }): Promise<LogicalTurnCancellationReceipt>;
  appendChatLifecycleEvent?(input: {
    sessionId: string;
    kind:
      | "logical_turn_continuing"
      | "logical_turn_cancelling"
      | "logical_turn_cancelled";
    payload: Record<string, unknown>;
  }): Promise<void>;
  now(): string;
}

export function isLogicalTurnRoute(pathname: string): boolean {
  return (
    pathname === "/v1/admin/logical-turns" ||
    pathname.startsWith("/v1/admin/logical-turns/") ||
    /^\/v1\/chat\/sessions\/[^/]+\/logical-turns(?:\/|$)/.test(pathname)
  );
}

export async function handleLogicalTurnRoute(
  request: LogicalTurnRouteRequest,
  context: LogicalTurnRouteContext,
): Promise<AdminRouteResult> {
  const method = (request.method ?? "GET").toUpperCase();
  const route = parseLogicalTurnRoute(request.url);
  if (route === undefined) {
    return failure(404, request.requestId, {
      code: "not_found",
      reason_code: "unknown_logical_turn_route",
      message: `unknown logical-turn route ${request.url.pathname}`,
      retryable: false,
    });
  }

  try {
    if (route.action === "list") {
      if (method !== "GET") return methodNotAllowed(request.requestId);
      const logicalTurnId = stringParam(request.url, "logical_turn_id");
      const limit = positiveIntegerParam(request.url, "limit") ?? 100;
      return successRoute(
        request.requestId,
        await context.logicalTurnDiagnostics({
          ...(logicalTurnId === undefined ? {} : { logicalTurnId }),
          ...(route.sessionId === undefined
            ? stringParam(request.url, "session_id") === undefined
              ? {}
              : { sessionId: stringParam(request.url, "session_id") }
            : { sessionId: route.sessionId }),
          includeTerminal: boolParam(request.url, "include_terminal"),
          limit: Math.min(limit, 500),
        }),
      );
    }

    if (method !== "POST") return methodNotAllowed(request.requestId);
    const body = recordValue(request.body);
    if (body === undefined) {
      return invalidInput(
        request.requestId,
        "logical_turn_body_required",
        "logical-turn control body must be a JSON object",
      );
    }
    const expectedRevision = nonNegativeInteger(body.expectedRevision);
    if (expectedRevision === undefined) {
      return invalidInput(
        request.requestId,
        "logical_turn_revision_required",
        "expectedRevision must be a non-negative integer",
      );
    }
    await assertSessionOwnsLogicalTurn(context, route, request.requestId);

    if (route.action === "cancel") {
      const beforeCancel = await currentDiagnostic(
        context,
        route.logicalTurnId,
        route.sessionId,
      );
      const reasonCode = stringValue(body.reasonCode) ?? "operator_cancelled";
      const summary =
        stringValue(body.summary) ?? "operator cancelled the logical turn";
      const idempotencyKey =
        request.idempotencyKey ??
        stringValue(body.idempotencyKey) ??
        `logical-turn-cancel:${route.logicalTurnId}:${expectedRevision}`;
      const receipt = await context.cancelLogicalTurn({
        logicalTurnId: route.logicalTurnId,
        expectedRevision,
        idempotencyKey,
        reasonCode,
        summary,
        now: context.now(),
      });
      const diagnostic = await currentDiagnostic(
        context,
        route.logicalTurnId,
        receipt.record.sessionId,
      );
      if (
        diagnostic !== undefined &&
        beforeCancel?.operatorState !== "running"
      ) {
        const payload = diagnosticPayload(diagnostic);
        await context.appendChatLifecycleEvent?.({
          sessionId: diagnostic.sessionId,
          kind: "logical_turn_cancelling",
          payload: {
            ...payload,
            operator_state: "cancelling",
            reason_code: reasonCode,
            summary: "logical turn cancellation requested",
          },
        });
        await context.appendChatLifecycleEvent?.({
          sessionId: diagnostic.sessionId,
          kind: "logical_turn_cancelled",
          payload,
        });
      }
      return successRoute(request.requestId, receipt);
    }

    const action = resolutionAction(body.action);
    if (action === undefined) {
      return invalidInput(
        request.requestId,
        "invalid_logical_turn_resolution_action",
        "action must be retry_unchanged or retry_provider_operation",
      );
    }
    const receipt = await context.resolveLogicalTurnAttention({
      logicalTurnId: route.logicalTurnId,
      expectedRevision,
      action,
    });
    const diagnostic = await currentDiagnostic(
      context,
      route.logicalTurnId,
      receipt.record.sessionId,
    );
    if (diagnostic !== undefined) {
      await context.appendChatLifecycleEvent?.({
        sessionId: diagnostic.sessionId,
        kind: "logical_turn_continuing",
        payload: diagnosticPayload(diagnostic),
      });
    }
    return successRoute(request.requestId, receipt);
  } catch (error) {
    return logicalTurnFailure(request.requestId, error);
  }
}

type LogicalTurnDiagnostic = LogicalTurnDiagnosticPage["items"][number];

async function currentDiagnostic(
  context: LogicalTurnRouteContext,
  logicalTurnId: string,
  sessionId?: string,
): Promise<LogicalTurnDiagnostic | undefined> {
  const page = await context.logicalTurnDiagnostics({
    logicalTurnId,
    ...(sessionId === undefined ? {} : { sessionId }),
    includeTerminal: true,
    limit: 1,
  });
  return page.items[0];
}

function diagnosticPayload(
  diagnostic: LogicalTurnDiagnostic,
): Record<string, unknown> {
  return {
    logical_turn_id: diagnostic.logicalTurnId,
    continuation_id: diagnostic.currentContinuationId,
    continuation_count: diagnostic.continuationCount,
    execution_epoch_id: diagnostic.activeExecutionEpochId,
    wake_id: diagnostic.sourceWakeId,
    phase: diagnostic.phase,
    operator_state: diagnostic.operatorState,
    progress_classification: diagnostic.progressClassification,
    provider_request_total: diagnostic.providerRequestTotal,
    tool_round_total: diagnostic.toolRoundTotal,
    last_progress_at: diagnostic.lastProgressAt,
    last_liveness_at: diagnostic.lastLivenessAt,
    reason_code: diagnostic.reasonCode,
    summary: diagnostic.summary,
    logical_turn_revision: diagnostic.revision,
  };
}

type ParsedLogicalTurnRoute =
  | { action: "list"; sessionId?: string }
  | {
      action: "cancel" | "resolve";
      sessionId?: string;
      logicalTurnId: string;
    };

function parseLogicalTurnRoute(url: URL): ParsedLogicalTurnRoute | undefined {
  const parts = url.pathname.split("/").filter(Boolean).map(decodeURIComponent);
  if (parts[0] !== "v1") return undefined;
  if (parts[1] === "admin" && parts[2] === "logical-turns") {
    if (parts.length === 3) return { action: "list" };
    if (
      parts.length === 5 &&
      (parts[4] === "cancel" || parts[4] === "resolve")
    ) {
      return { action: parts[4], logicalTurnId: parts[3] ?? "" };
    }
  }
  if (
    parts[1] === "chat" &&
    parts[2] === "sessions" &&
    parts[4] === "logical-turns"
  ) {
    const sessionId = parts[3] ?? "";
    if (parts.length === 5) return { action: "list", sessionId };
    if (
      parts.length === 7 &&
      (parts[6] === "cancel" || parts[6] === "resolve")
    ) {
      return {
        action: parts[6],
        sessionId,
        logicalTurnId: parts[5] ?? "",
      };
    }
  }
  return undefined;
}

async function assertSessionOwnsLogicalTurn(
  context: LogicalTurnRouteContext,
  route: Extract<ParsedLogicalTurnRoute, { logicalTurnId: string }>,
  requestId: string,
): Promise<void> {
  if (route.sessionId === undefined) return;
  const page = await context.logicalTurnDiagnostics({
    logicalTurnId: route.logicalTurnId,
    sessionId: route.sessionId,
    includeTerminal: true,
    limit: 1,
  });
  if (page.items.length === 0) {
    throw new LogicalTurnRouteError(
      404,
      "logical_turn_not_found",
      `logical turn ${route.logicalTurnId} was not found for session ${route.sessionId}`,
      requestId,
    );
  }
}

class LogicalTurnRouteError extends Error {
  constructor(
    readonly status: number,
    readonly reasonCode: string,
    message: string,
    readonly requestId: string,
  ) {
    super(message);
  }
}

function logicalTurnFailure(
  requestId: string,
  error: unknown,
): AdminRouteResult {
  if (error instanceof LogicalTurnRouteError) {
    return failure(error.status, error.requestId, {
      code: error.status === 404 ? "not_found" : "failed_precondition",
      reason_code: error.reasonCode,
      message: error.message,
      retryable: false,
    });
  }
  const message = error instanceof Error ? error.message : String(error);
  const revisionConflict = /revision mismatch/i.test(message);
  const notFound = /not found/i.test(message);
  return failure(revisionConflict ? 409 : notFound ? 404 : 400, requestId, {
    code: revisionConflict
      ? "conflict"
      : notFound
        ? "not_found"
        : "invalid_input",
    reason_code: revisionConflict
      ? "logical_turn_revision_mismatch"
      : notFound
        ? "logical_turn_not_found"
        : "logical_turn_control_rejected",
    message,
    retryable: revisionConflict,
  });
}

function methodNotAllowed(requestId: string): AdminRouteResult {
  return failure(405, requestId, {
    code: "method_not_allowed",
    reason_code: "logical_turn_method_not_allowed",
    message: "logical-turn list uses GET and logical-turn controls use POST",
    retryable: false,
  });
}

function invalidInput(
  requestId: string,
  reasonCode: string,
  message: string,
): AdminRouteResult {
  return failure(400, requestId, {
    code: "invalid_input",
    reason_code: reasonCode,
    message,
    retryable: false,
  });
}

function recordValue(value: unknown): Record<string, unknown> | undefined {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : undefined;
}

function stringValue(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function stringParam(url: URL, key: string): string | undefined {
  return stringValue(url.searchParams.get(key));
}

function nonNegativeInteger(value: unknown): number | undefined {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0
    ? value
    : undefined;
}

function positiveIntegerParam(url: URL, key: string): number | undefined {
  const value = Number(url.searchParams.get(key));
  return Number.isSafeInteger(value) && value > 0 ? value : undefined;
}

function boolParam(url: URL, key: string): boolean {
  return url.searchParams.get(key) === "true";
}

function resolutionAction(
  value: unknown,
): LogicalTurnResolutionAction | undefined {
  return value === "retry_unchanged" || value === "retry_provider_operation"
    ? value
    : undefined;
}
