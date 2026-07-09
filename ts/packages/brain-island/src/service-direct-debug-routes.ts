import type { IncomingMessage } from "node:http";
import type { SessionId, SessionState } from "@rusty-crew/contracts";
import type { AdminRouteResult } from "./admin-diagnostics-api.js";
import {
  inspectDirectDebugSession,
  requestDirectDebugTurn,
  type DirectDebugResult,
  type DirectDebugServiceContext,
} from "./direct-debug-service.js";
import { failure, successRoute } from "./service-route-results.js";
import type { ProviderRequestDebugDetail } from "./rusty-view-chat-api.js";

export interface ServiceDirectDebugRouteContext {
  requestId(request: IncomingMessage): string;
  readJsonBody(request: IncomingMessage): Promise<unknown>;
  listSessions(): Promise<SessionState[]>;
  buildDirectDebugContext(): Promise<DirectDebugServiceContext>;
  emitContextCompactionDebugEvents(
    session: SessionState,
    input: {
      wakeId?: string;
      strategyId: string;
      estimateQuality: string;
      fillPercent?: number;
      compactAtPercent?: number;
      targetPercentAfterCompaction?: number;
      artifactId?: string;
      reasonCode?: string;
      fail: boolean;
    },
  ): Promise<unknown>;
  providerRequestDebugDetail(input: {
    session: SessionState;
    debugDetailId: string;
    requestId: string;
  }): Promise<ProviderRequestDebugDetail | undefined>;
}

export async function handleServiceDirectDebugRequest(
  request: IncomingMessage,
  url: URL,
  context: ServiceDirectDebugRouteContext,
): Promise<AdminRouteResult> {
  const parts = url.pathname.split("/").filter(Boolean);
  if (isDebugSessionSubroute(parts, "context-compaction-events", 5)) {
    if ((request.method ?? "GET").toUpperCase() !== "POST") {
      return failure(405, context.requestId(request), {
        code: "method_not_allowed",
        reason_code: "debug_context_compaction_events_requires_post",
        message: "context compaction debug event route only supports POST",
        retryable: false,
      });
    }
    const requestIdValue = context.requestId(request);
    const sessionId = decodeURIComponent(parts[3] ?? "") as SessionId;
    const session = await debugSessionById(
      context,
      sessionId,
      requestIdValue,
      "debug_context_compaction_session_not_found",
    );
    if (isRouteResult(session)) return session;
    const body = recordBody(await context.readJsonBody(request));
    const result = await context.emitContextCompactionDebugEvents(session, {
      wakeId: optionalString(body.wakeId) ?? optionalString(body.wake_id),
      strategyId:
        optionalString(body.strategyId) ??
        optionalString(body.strategy_id) ??
        "rolling_summary_compaction",
      estimateQuality:
        optionalString(body.estimateQuality) ??
        optionalString(body.estimate_quality) ??
        "approximate",
      fillPercent:
        optionalNumber(body.fillPercent) ?? optionalNumber(body.fill_percent),
      compactAtPercent:
        optionalNumber(body.compactAtPercent) ??
        optionalNumber(body.compact_at_percent),
      targetPercentAfterCompaction:
        optionalNumber(body.targetPercentAfterCompaction) ??
        optionalNumber(body.target_percent_after_compaction),
      artifactId:
        optionalString(body.artifactId) ?? optionalString(body.artifact_id),
      reasonCode:
        optionalString(body.reasonCode) ?? optionalString(body.reason_code),
      fail: body.fail === true,
    });
    return successRoute(requestIdValue, result);
  }

  if (isDebugSessionSubroute(parts, "context", 5)) {
    if ((request.method ?? "GET").toUpperCase() !== "GET") {
      return failure(405, context.requestId(request), {
        code: "method_not_allowed",
        reason_code: "debug_context_requires_get",
        message: "direct debug context route only supports GET",
        retryable: false,
      });
    }
    const result = inspectDirectDebugSession(
      {
        sessionId: decodeURIComponent(parts[3] ?? ""),
        includePromptText:
          url.searchParams.get("include_prompt_text") === "true",
        includeMessageBodies:
          url.searchParams.get("include_message_bodies") === "true",
        maxPendingMessages: optionalInteger(
          url.searchParams.get("max_pending_messages"),
        ),
        maxRecentEvents: optionalInteger(
          url.searchParams.get("max_recent_events"),
        ),
      },
      await context.buildDirectDebugContext(),
    );
    return directDebugResult(context.requestId(request), result);
  }

  if (isDebugSessionSubroute(parts, "provider-requests", 6)) {
    if ((request.method ?? "GET").toUpperCase() !== "GET") {
      return failure(405, context.requestId(request), {
        code: "method_not_allowed",
        reason_code: "debug_provider_request_requires_get",
        message: "direct provider request debug route only supports GET",
        retryable: false,
      });
    }
    const requestIdValue = context.requestId(request);
    const sessionId = decodeURIComponent(parts[3] ?? "") as SessionId;
    const session = await debugSessionById(
      context,
      sessionId,
      requestIdValue,
      "debug_provider_request_session_not_found",
    );
    if (isRouteResult(session)) return session;
    const debugDetailId = decodeURIComponent(parts[5] ?? "");
    const detail = await context.providerRequestDebugDetail({
      session,
      debugDetailId,
      requestId: requestIdValue,
    });
    if (!detail) {
      return failure(404, requestIdValue, {
        code: "not_found",
        reason_code: "debug_provider_request_not_found",
        message: `provider request debug detail ${debugDetailId} was not found`,
        retryable: false,
      });
    }
    return successRoute(requestIdValue, detail);
  }

  if (isDebugSessionSubroute(parts, "turn", 5)) {
    const body = recordBody(await context.readJsonBody(request));
    const result = await requestDirectDebugTurn(
      {
        ...body,
        sessionId: decodeURIComponent(parts[3] ?? ""),
      } as never,
      await context.buildDirectDebugContext(),
    );
    return directDebugResult(context.requestId(request), result);
  }

  return failure(404, context.requestId(request), {
    code: "not_found",
    reason_code: "unknown_debug_route",
    message: `unknown debug route ${url.pathname}`,
    retryable: false,
  });
}

function isDebugSessionSubroute(
  parts: string[],
  subroute: string,
  length: number,
): boolean {
  return (
    parts.length === length &&
    parts[0] === "v1" &&
    parts[1] === "debug" &&
    parts[2] === "sessions" &&
    parts[4] === subroute
  );
}

async function debugSessionById(
  context: ServiceDirectDebugRouteContext,
  sessionId: SessionId,
  requestIdValue: string,
  reasonCode: string,
): Promise<SessionState | AdminRouteResult> {
  const sessions = await context.listSessions();
  const session = sessions.find(
    (candidate) => candidate.sessionId === sessionId,
  );
  if (session) return session;
  return failure(404, requestIdValue, {
    code: "not_found",
    reason_code: reasonCode,
    message: `debug session ${sessionId} was not found`,
    retryable: false,
  });
}

function directDebugResult<T>(
  requestIdValue: string,
  result: DirectDebugResult<T>,
): AdminRouteResult<T> {
  if (result.ok) {
    return {
      status: 200,
      headers: { "content-type": "application/json" },
      body: {
        ok: true,
        data: result.data,
        meta: { request_id: requestIdValue, schema_version: 1 },
      },
    };
  }
  return failure(directDebugStatus(result.error.code), requestIdValue, {
    code: result.error.code,
    reason_code: result.error.reasonCode,
    message: result.error.message,
    retryable: result.error.retryable,
  }) as AdminRouteResult<T>;
}

function directDebugStatus(
  code:
    | "not_found"
    | "forbidden"
    | "invalid_input"
    | "failed_precondition"
    | "internal_error",
): number {
  switch (code) {
    case "not_found":
      return 404;
    case "forbidden":
      return 403;
    case "invalid_input":
      return 400;
    case "failed_precondition":
      return 412;
    case "internal_error":
      return 500;
  }
}

function isRouteResult(value: unknown): value is AdminRouteResult {
  return (
    typeof value === "object" &&
    value !== null &&
    "status" in value &&
    "headers" in value &&
    "body" in value
  );
}

function recordBody(value: unknown): Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function optionalNumber(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value)
    ? value
    : undefined;
}

function optionalInteger(value: string | null): number | undefined {
  if (value === null || value.trim() === "") return undefined;
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) && parsed >= 0 ? parsed : undefined;
}
