import {
  buildRuntimeHealthProjection,
  type RuntimeHealthProjection,
} from "./runtime-health.js";
import type {
  RuntimeDiagnosticsProjection,
  RuntimeSessionDiagnostics,
} from "./runtime-diagnostics.js";
import type { BackgroundServiceDiagnosticsProjection } from "./background-service-diagnostics.js";
import { apiCapabilityRegistry } from "./api-command-registry.js";
import type { RuntimeConfigValidationPreflightReport } from "./service-runtime-config.js";

export type AdminErrorCode =
  | "unauthorized"
  | "forbidden"
  | "method_not_allowed"
  | "not_found"
  | "invalid_input"
  | "failed_precondition"
  | "conflict"
  | "internal_error";

export interface AdminApiMeta {
  request_id: string;
  schema_version: 1;
}

export type AdminApiEnvelope<T> =
  | {
      ok: true;
      data: T;
      meta: AdminApiMeta;
    }
  | {
      ok: false;
      error: {
        code: AdminErrorCode;
        reason_code: string;
        message: string;
        retryable: boolean;
      };
      meta: AdminApiMeta;
    };

export interface AdminRouteResult<T = unknown> {
  status: number;
  headers: Record<string, string>;
  body: AdminApiEnvelope<T>;
}

export interface AdminDiagnosticsRouteRequest {
  method: string;
  url: string;
  requestId?: string;
}

export interface AdminRecentEvent {
  id: string | number;
  createdAt: string;
  source: string;
  eventType: string;
  summary: string;
  severity?: string;
  workRef?: Record<string, unknown>;
  resultRef?: Record<string, unknown>;
}

export interface AdminDiagnosticsContext {
  diagnostics: RuntimeDiagnosticsProjection;
  health?: RuntimeHealthProjection;
  recentEvents?: readonly AdminRecentEvent[];
  background?: BackgroundServiceDiagnosticsProjection;
  configValidation?: RuntimeConfigValidationPreflightReport;
}

export interface AdminPage<T> {
  items: T[];
  total: number;
  limit: number;
  offset: number;
  nextOffset?: number;
}

export interface AdminAgentDiagnostics {
  agentId: string;
  profileId: string;
  sessions: number;
  activeSessions: number;
  idleSessions: number;
  archivedSessions: number;
  staleSessions: number;
}

export function handleAdminDiagnosticsRequest(
  request: AdminDiagnosticsRouteRequest,
  context: AdminDiagnosticsContext,
): AdminRouteResult {
  const requestId = request.requestId ?? "admin-read";
  if (request.method.toUpperCase() !== "GET") {
    return failure(405, requestId, {
      code: "method_not_allowed",
      reason_code: "read_only_route",
      message: "read-only admin diagnostics routes only support GET",
      retryable: false,
    });
  }

  const url = parseAdminUrl(request.url);
  const health =
    context.health ?? buildRuntimeHealthProjection(context.diagnostics);

  switch (url.pathname) {
    case "/v1/admin/capabilities":
      return success(requestId, apiCapabilityRegistry());
    case "/v1/admin/healthz":
      return success(requestId, health.liveness);
    case "/v1/admin/readyz":
      return success(requestId, health.readiness);
    case "/v1/admin/diagnostics":
      return success(requestId, {
        overview: context.diagnostics,
        health,
      });
    case "/v1/admin/diagnostics/overview":
      return success(requestId, {
        generatedAt: context.diagnostics.generatedAt,
        health: context.diagnostics.health,
        degraded: context.diagnostics.degraded,
        reasonCodes: context.diagnostics.reasonCodes,
        summary: context.diagnostics.summary,
      });
    case "/v1/admin/diagnostics/sessions":
      return success(
        requestId,
        page(filterSessions(context.diagnostics.runtime.sessions, url), url),
      );
    case "/v1/admin/diagnostics/agents":
      return success(
        requestId,
        page(agentDiagnostics(context.diagnostics), url),
      );
    case "/v1/admin/diagnostics/delegations":
      return success(
        requestId,
        page(context.diagnostics.runtime.delegatedSessions, url),
      );
    case "/v1/admin/diagnostics/queues":
      return success(requestId, context.diagnostics.queues ?? null);
    case "/v1/admin/diagnostics/tools":
      return success(
        requestId,
        page(filterTools(context.diagnostics.tools, url), url),
      );
    case "/v1/admin/diagnostics/mcp":
      return success(
        requestId,
        page(
          filterByStatus(context.diagnostics.adapters?.mcp.surfaces ?? [], url),
          url,
        ),
      );
    case "/v1/admin/diagnostics/channels":
      return success(
        requestId,
        page(
          filterByStatus(
            context.diagnostics.adapters?.channels.bindings ?? [],
            url,
          ),
          url,
        ),
      );
    case "/v1/admin/diagnostics/persistence":
      return success(requestId, context.diagnostics.persistence ?? null);
    case "/v1/admin/diagnostics/provider-state":
      return success(
        requestId,
        context.diagnostics.runtime.brainModules.map((module) => ({
          profileId: module.profileId,
          implementationId: module.implementationId,
          moduleId: module.moduleId,
          strategyId: module.effectiveStrategy ?? module.strategy,
          providerStateMode: module.providerStateMode,
          providerState: module.providerState,
        })),
      );
    case "/v1/admin/diagnostics/observation":
      return success(requestId, context.diagnostics.observation ?? null);
    case "/v1/admin/diagnostics/background":
      return success(requestId, context.background ?? null);
    case "/v1/admin/diagnostics/config":
      return success(requestId, context.configValidation ?? null);
    case "/v1/admin/diagnostics/metrics":
      return success(requestId, page(health.metrics, url, 100, 250));
    case "/v1/admin/events/recent":
      return success(requestId, page(context.recentEvents ?? [], url));
    default:
      return failure(404, requestId, {
        code: "not_found",
        reason_code: "unknown_admin_diagnostics_route",
        message: `unknown read-only admin diagnostics route ${url.pathname}`,
        retryable: false,
      });
  }
}

function parseAdminUrl(url: string): URL {
  return new URL(url, "http://rusty-crew.local");
}

function success<T>(requestId: string, data: T): AdminRouteResult<T> {
  return {
    status: 200,
    headers: { "content-type": "application/json" },
    body: {
      ok: true,
      data: redactAdminData(data),
      meta: meta(requestId),
    },
  };
}

function failure(
  status: number,
  requestId: string,
  error: Extract<AdminApiEnvelope<never>, { ok: false }>["error"],
): AdminRouteResult {
  return {
    status,
    headers: { "content-type": "application/json" },
    body: {
      ok: false,
      error,
      meta: meta(requestId),
    },
  };
}

function meta(requestId: string): AdminApiMeta {
  return {
    request_id: requestId,
    schema_version: 1,
  };
}

function page<T>(
  items: readonly T[],
  url: URL,
  defaultLimit = 50,
  maxLimit = 100,
): AdminPage<T> {
  const limit = boundedInteger(
    url.searchParams.get("limit"),
    defaultLimit,
    maxLimit,
  );
  const offset = boundedInteger(
    url.searchParams.get("offset"),
    0,
    Number.MAX_SAFE_INTEGER,
  );
  const pageItems = items.slice(offset, offset + limit);
  const nextOffset =
    offset + pageItems.length < items.length
      ? offset + pageItems.length
      : undefined;
  return {
    items: pageItems,
    total: items.length,
    limit,
    offset,
    nextOffset,
  };
}

function boundedInteger(
  value: string | null,
  fallback: number,
  max: number,
): number {
  if (value === null) return fallback;
  const parsed = Number.parseInt(value, 10);
  if (!Number.isFinite(parsed) || parsed < 0) return fallback;
  return Math.min(parsed, max);
}

function filterSessions(
  sessions: readonly RuntimeSessionDiagnostics[],
  url: URL,
): RuntimeSessionDiagnostics[] {
  const status = url.searchParams.get("status");
  const agentId = url.searchParams.get("agent_id");
  const profileId = url.searchParams.get("profile_id");
  return sessions.filter(
    (session) =>
      (status === null || session.status === status) &&
      (agentId === null || session.agentId === agentId) &&
      (profileId === null || session.profileId === profileId),
  );
}

function agentDiagnostics(
  diagnostics: RuntimeDiagnosticsProjection,
): AdminAgentDiagnostics[] {
  const agents = new Map<string, AdminAgentDiagnostics>();
  for (const session of diagnostics.runtime.sessions) {
    const key = `${session.agentId}\0${session.profileId}`;
    const existing =
      agents.get(key) ??
      ({
        agentId: session.agentId,
        profileId: session.profileId,
        sessions: 0,
        activeSessions: 0,
        idleSessions: 0,
        archivedSessions: 0,
        staleSessions: 0,
      } satisfies AdminAgentDiagnostics);
    existing.sessions += 1;
    if (session.status === "active") existing.activeSessions += 1;
    if (session.status === "idle") existing.idleSessions += 1;
    if (session.status === "archived") existing.archivedSessions += 1;
    if (session.stale) existing.staleSessions += 1;
    agents.set(key, existing);
  }
  return [...agents.values()].sort((left, right) =>
    left.agentId.localeCompare(right.agentId),
  );
}

function filterTools<T extends { catalogId?: string; invalid?: boolean }>(
  tools: readonly T[],
  url: URL,
): T[] {
  const catalogId = url.searchParams.get("catalog_id");
  const invalid = url.searchParams.get("invalid");
  return tools.filter(
    (tool) =>
      (catalogId === null || tool.catalogId === catalogId) &&
      (invalid === null || String(Boolean(tool.invalid)) === invalid),
  );
}

function filterByStatus<T extends { status?: string }>(
  items: readonly T[],
  url: URL,
): T[] {
  const status = url.searchParams.get("status");
  return items.filter((item) => status === null || item.status === status);
}

function redactAdminData<T>(data: T): T {
  return redactValue(data) as T;
}

function redactValue(value: unknown): unknown {
  if (typeof value === "string") return value.slice(0, 2_048);
  if (Array.isArray(value)) return value.map(redactValue);
  if (value === null || typeof value !== "object") return value;

  const output: Record<string, unknown> = {};
  for (const [key, nested] of Object.entries(value)) {
    if (isSecretLikeKey(key)) {
      output[key] = "[redacted]";
    } else {
      output[key] = redactValue(nested);
    }
  }
  return output;
}

function isSecretLikeKey(key: string): boolean {
  return /authorization|bearer|credential|password|secret|token|api[_-]?key/i.test(
    key,
  );
}
