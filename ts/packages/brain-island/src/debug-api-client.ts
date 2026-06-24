import type {
  AdminApiEnvelope,
  AdminPage,
  AdminRecentEvent,
} from "./admin-diagnostics-api.js";
import type {
  ChannelAdapterBindingDiagnostics,
  McpAdapterSurfaceDiagnostics,
} from "./adapter-diagnostics.js";
import type {
  DirectDebugSessionView,
  DirectDebugTurnOutcome,
  DirectDebugTurnRequest,
} from "./direct-debug-service.js";
import type {
  ObservationDiagnosticsProjection,
  RuntimeDiagnosticsProjection,
  RuntimeSessionDiagnostics,
  ToolDiagnosticsProjection,
} from "./runtime-diagnostics.js";
import type {
  RuntimeHealthProjection,
  RuntimeMetricSample,
  RuntimeReadinessProbe,
} from "./runtime-health.js";
import type { RuntimeConfigValidationPreflightReport } from "./service-runtime-config.js";

export type DebugApiFetch = typeof fetch;

export interface DebugApiClientOptions {
  baseUrl: string;
  bearerToken?: string;
  fetchImpl?: DebugApiFetch;
  timeoutMs?: number;
  retries?: number;
}

export interface DebugApiQuery extends Record<string, unknown> {
  limit?: number;
  offset?: number;
  status?: string;
  profileId?: string;
  invalid?: boolean;
}

export interface DirectDebugContextRequest {
  sessionId: string;
  includePromptText?: boolean;
  includeMessageBodies?: boolean;
  maxPendingMessages?: number;
  maxRecentEvents?: number;
}

export interface DebugDiagnosticsBundle {
  overview: RuntimeDiagnosticsProjection;
  health: RuntimeHealthProjection;
}

export class DebugApiClientError extends Error {
  constructor(
    readonly code: string,
    message: string,
    readonly options: {
      status?: number;
      reasonCode?: string;
      retryable: boolean;
      cause?: unknown;
    },
  ) {
    super(message);
    this.name = "DebugApiClientError";
  }
}

export interface DebugApiClient {
  diagnostics(): Promise<DebugDiagnosticsBundle>;
  overview(): Promise<
    Pick<
      RuntimeDiagnosticsProjection,
      "generatedAt" | "health" | "degraded" | "reasonCodes" | "summary"
    >
  >;
  ready(): Promise<RuntimeReadinessProbe>;
  sessions(
    query?: DebugApiQuery,
  ): Promise<AdminPage<RuntimeSessionDiagnostics>>;
  tools(query?: DebugApiQuery): Promise<AdminPage<ToolDiagnosticsProjection>>;
  mcpSurfaces(
    query?: DebugApiQuery,
  ): Promise<AdminPage<McpAdapterSurfaceDiagnostics>>;
  channelBindings(
    query?: DebugApiQuery,
  ): Promise<AdminPage<ChannelAdapterBindingDiagnostics>>;
  configValidation(): Promise<RuntimeConfigValidationPreflightReport | null>;
  observation(): Promise<ObservationDiagnosticsProjection | null>;
  metrics(query?: DebugApiQuery): Promise<AdminPage<RuntimeMetricSample>>;
  recentEvents(query?: DebugApiQuery): Promise<AdminPage<AdminRecentEvent>>;
  directDebugContext(
    request: DirectDebugContextRequest,
  ): Promise<DirectDebugSessionView>;
  requestDirectDebugTurn(
    request: DirectDebugTurnRequest,
  ): Promise<DirectDebugTurnOutcome>;
}

export function createDebugApiClient(
  options: DebugApiClientOptions,
): DebugApiClient {
  const transport = new DebugApiHttpTransport(options);
  return {
    diagnostics: () =>
      transport.get<DebugDiagnosticsBundle>("/v1/admin/diagnostics"),
    overview: () =>
      transport.get<
        Pick<
          RuntimeDiagnosticsProjection,
          "generatedAt" | "health" | "degraded" | "reasonCodes" | "summary"
        >
      >("/v1/admin/diagnostics/overview"),
    ready: () => transport.get<RuntimeReadinessProbe>("/v1/admin/readyz"),
    sessions: (query) =>
      transport.get<AdminPage<RuntimeSessionDiagnostics>>(
        "/v1/admin/diagnostics/sessions",
        query,
      ),
    tools: (query) =>
      transport.get<AdminPage<ToolDiagnosticsProjection>>(
        "/v1/admin/diagnostics/tools",
        query,
      ),
    mcpSurfaces: (query) =>
      transport.get<AdminPage<McpAdapterSurfaceDiagnostics>>(
        "/v1/admin/diagnostics/mcp",
        query,
      ),
    channelBindings: (query) =>
      transport.get<AdminPage<ChannelAdapterBindingDiagnostics>>(
        "/v1/admin/diagnostics/channels",
        query,
      ),
    configValidation: () =>
      transport.get<RuntimeConfigValidationPreflightReport | null>(
        "/v1/admin/diagnostics/config",
      ),
    observation: () =>
      transport.get<ObservationDiagnosticsProjection | null>(
        "/v1/admin/diagnostics/observation",
      ),
    metrics: (query) =>
      transport.get<AdminPage<RuntimeMetricSample>>(
        "/v1/admin/diagnostics/metrics",
        query,
      ),
    recentEvents: (query) =>
      transport.get<AdminPage<AdminRecentEvent>>(
        "/v1/admin/events/recent",
        query,
      ),
    directDebugContext: (request) =>
      transport.get<DirectDebugSessionView>(
        `/v1/debug/sessions/${encodeURIComponent(request.sessionId)}/context`,
        {
          include_prompt_text: request.includePromptText,
          include_message_bodies: request.includeMessageBodies,
          max_pending_messages: request.maxPendingMessages,
          max_recent_events: request.maxRecentEvents,
        },
      ),
    requestDirectDebugTurn: (request) =>
      transport.post<DirectDebugTurnOutcome>(
        `/v1/debug/sessions/${encodeURIComponent(String(request.sessionId))}/turn`,
        request,
      ),
  };
}

class DebugApiHttpTransport {
  private readonly baseUrl: URL;
  private readonly fetchImpl: DebugApiFetch;
  private readonly timeoutMs: number;
  private readonly retries: number;
  private readonly bearerToken?: string;

  constructor(options: DebugApiClientOptions) {
    this.baseUrl = new URL(options.baseUrl);
    this.fetchImpl = options.fetchImpl ?? fetch;
    this.timeoutMs = options.timeoutMs ?? 2_000;
    this.retries = options.retries ?? 1;
    this.bearerToken = options.bearerToken;
  }

  get<T>(path: string, query?: Record<string, unknown>): Promise<T> {
    return this.request<T>("GET", path, { query });
  }

  post<T>(path: string, body: unknown): Promise<T> {
    return this.request<T>("POST", path, { body });
  }

  private async request<T>(
    method: "GET" | "POST",
    path: string,
    options: { query?: Record<string, unknown>; body?: unknown } = {},
  ): Promise<T> {
    const attempts = method === "GET" ? this.retries + 1 : 1;
    let lastError: unknown;
    for (let attempt = 0; attempt < attempts; attempt += 1) {
      try {
        return await this.requestOnce<T>(method, path, options);
      } catch (error) {
        lastError = error;
        if (!shouldRetry(error, method, attempt, attempts)) {
          throw error;
        }
      }
    }
    throw normalizeClientError(lastError, "debug_api_request_failed");
  }

  private async requestOnce<T>(
    method: "GET" | "POST",
    path: string,
    options: { query?: Record<string, unknown>; body?: unknown },
  ): Promise<T> {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), this.timeoutMs);
    try {
      const response = await this.fetchImpl(this.url(path, options.query), {
        method,
        signal: controller.signal,
        headers: this.headers(options.body),
        body:
          options.body === undefined ? undefined : JSON.stringify(options.body),
      });
      return await decodeResponse<T>(response);
    } catch (error) {
      throw normalizeClientError(error, "debug_api_transport_failed");
    } finally {
      clearTimeout(timeout);
    }
  }

  private url(path: string, query?: Record<string, unknown>): string {
    const url = new URL(path, this.baseUrl);
    for (const [key, value] of Object.entries(query ?? {})) {
      if (value === undefined) {
        continue;
      }
      url.searchParams.set(key, String(value));
    }
    return url.toString();
  }

  private headers(body: unknown): Record<string, string> {
    return {
      accept: "application/json",
      ...(body === undefined ? {} : { "content-type": "application/json" }),
      ...(this.bearerToken
        ? { authorization: `Bearer ${this.bearerToken}` }
        : {}),
    };
  }
}

async function decodeResponse<T>(response: Response): Promise<T> {
  let body: unknown;
  try {
    body = await response.json();
  } catch (error) {
    throw new DebugApiClientError(
      "invalid_json",
      `debug API returned non-JSON response with status ${response.status}`,
      {
        status: response.status,
        retryable: response.status >= 500,
        cause: error,
      },
    );
  }

  const envelope = body as AdminApiEnvelope<T>;
  if (response.ok && envelope.ok === true) {
    return envelope.data;
  }
  if (envelope.ok === false) {
    throw new DebugApiClientError(envelope.error.code, envelope.error.message, {
      status: response.status,
      reasonCode: envelope.error.reason_code,
      retryable: envelope.error.retryable,
    });
  }
  throw new DebugApiClientError(
    "invalid_envelope",
    "debug API response did not match the expected envelope",
    {
      status: response.status,
      retryable: response.status >= 500,
    },
  );
}

function shouldRetry(
  error: unknown,
  method: "GET" | "POST",
  attempt: number,
  attempts: number,
): boolean {
  if (method !== "GET" || attempt + 1 >= attempts) {
    return false;
  }
  const normalized = normalizeClientError(error, "debug_api_retry_check");
  return normalized.options.retryable;
}

function normalizeClientError(
  error: unknown,
  code: string,
): DebugApiClientError {
  if (error instanceof DebugApiClientError) {
    return error;
  }
  const aborted =
    error instanceof Error &&
    (error.name === "AbortError" || error.message.includes("aborted"));
  return new DebugApiClientError(
    aborted ? "timeout" : code,
    error instanceof Error ? error.message : "debug API request failed",
    {
      retryable: true,
      cause: error,
    },
  );
}
