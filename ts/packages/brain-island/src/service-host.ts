import { createServer, type IncomingMessage, type Server } from "node:http";
import { fileURLToPath } from "node:url";
import type { EngineHandle, SessionState } from "@rusty-crew/contracts";
import {
  loadNativeBridge,
  type NativeBridgeModule,
} from "@rusty-crew/native-bridge";
import {
  createMemoryAdminControlAuditSink,
  type AdminControlCommand,
  type AdminControlExecutor,
  handleAdminControlRequest,
} from "./admin-control-api.js";
import {
  handleAdminDiagnosticsRequest,
  type AdminDiagnosticsContext,
  type AdminRouteResult,
} from "./admin-diagnostics-api.js";
import {
  inspectDirectDebugSession,
  requestDirectDebugTurn,
  type DirectDebugResult,
  type DirectDebugServiceContext,
} from "./direct-debug-service.js";
import { loadProfileContext } from "./profile-loading.js";
import { buildRuntimeDiagnosticsProjection } from "./runtime-diagnostics.js";
import type { RuntimeHealthProjection } from "./runtime-health.js";
import {
  acquireRustyCrewServiceLock,
  ensureRustyCrewServiceDirectories,
  loadRustyCrewServiceConfig,
  type RustyCrewServiceConfig,
  type RustyCrewServiceEnv,
  type RustyCrewServiceLock,
} from "./service-config.js";
import {
  applyRustyCrewRuntimeConfig,
  loadRustyCrewRuntimeConfig,
  type RustyCrewRuntimeConfig,
  type RustyCrewRuntimeConfigApplyResult,
} from "./service-runtime-config.js";

export interface RustyCrewServiceHostOptions {
  env?: RustyCrewServiceEnv;
  config?: RustyCrewServiceConfig;
  bridge?: NativeBridgeModule;
  now?: () => string;
}

export interface RustyCrewServiceHost {
  readonly config: RustyCrewServiceConfig;
  readonly bridge: NativeBridgeModule;
  readonly engine: EngineHandle;
  readonly server: Server;
  readonly url: string;
  stop(): Promise<void>;
}

interface ServiceState {
  readonly config: RustyCrewServiceConfig;
  readonly bridge: NativeBridgeModule;
  readonly engine: EngineHandle;
  readonly lock: RustyCrewServiceLock;
  readonly auditSink: ReturnType<typeof createMemoryAdminControlAuditSink>;
  readonly runtimeConfig: RustyCrewRuntimeConfig;
  readonly runtimeConfigApplyResult: RustyCrewRuntimeConfigApplyResult;
  readonly now: () => string;
  stopping: boolean;
}

const CONTROL_ROUTE_PREFIX = "/v1/admin/control/";

export async function startRustyCrewServiceHost(
  options: RustyCrewServiceHostOptions = {},
): Promise<RustyCrewServiceHost> {
  const config = options.config ?? loadRustyCrewServiceConfig(options.env);
  if (!config.admin.token) {
    throw new Error("RUSTY_CREW_ADMIN_TOKEN is required to start admin HTTP");
  }

  ensureRustyCrewServiceDirectories(config);
  const lock = acquireRustyCrewServiceLock(config);
  const bridge = options.bridge ?? (await loadNativeBridge());
  let engine: EngineHandle | undefined;
  let server: Server | undefined;

  try {
    engine = await bridge.initializeEngine({
      engineDataDir: config.paths.engineDataDir,
      clock: "system",
      defaultTurnBudget: 16,
      defaultIdleTimeoutMs: 30_000,
    });
    const runtimeConfig = await loadRustyCrewRuntimeConfig(config);
    const runtimeConfigApplyResult = await applyRustyCrewRuntimeConfig({
      serviceConfig: config,
      runtimeConfig,
      bridge,
    });

    const state: ServiceState = {
      config,
      bridge,
      engine,
      lock,
      auditSink: createMemoryAdminControlAuditSink(),
      runtimeConfig,
      runtimeConfigApplyResult,
      now: options.now ?? (() => new Date().toISOString()),
      stopping: false,
    };
    server = createServer((request, response) => {
      void handleHttpRequest(request, state)
        .then((result) => writeJsonResponse(response, result))
        .catch((error) =>
          writeJsonResponse(
            response,
            failure(500, requestId(request), {
              code: "internal_error",
              reason_code: "service_host_error",
              message: errorMessage(error, "service host request failed"),
              retryable: false,
            }),
          ),
        );
    });

    await listen(server, config.admin.port, config.admin.host);

    return {
      config,
      bridge,
      engine,
      server,
      url: `http://${config.admin.host}:${config.admin.port}`,
      stop: () => stopService(state, server),
    };
  } catch (error) {
    if (server) {
      await closeServer(server).catch(() => undefined);
    }
    if (engine !== undefined) {
      await bridge
        .shutdownEngine({ engine, drainTimeoutMs: 2_000 })
        .catch(() => undefined);
    }
    lock.release();
    throw error;
  }
}

async function handleHttpRequest(
  request: IncomingMessage,
  state: ServiceState,
): Promise<AdminRouteResult> {
  const url = new URL(request.url ?? "/", "http://rusty-crew.local");
  if (url.pathname === "/v1/admin/healthz") {
    return handleAdminDiagnosticsRequest(
      {
        method: request.method ?? "GET",
        url: url.toString(),
        requestId: requestId(request),
      },
      await buildDiagnosticsContext(state),
    );
  }

  if (!isAuthorized(request, state.config.admin.token)) {
    return failure(401, requestId(request), {
      code: "unauthorized",
      reason_code: "missing_or_invalid_bearer_token",
      message: "admin HTTP requires a valid bearer token",
      retryable: false,
    });
  }

  if (url.pathname.startsWith(CONTROL_ROUTE_PREFIX)) {
    const body = await readJsonBody(request);
    const result = await handleAdminControlRequest(
      {
        method: request.method ?? "POST",
        url: url.toString(),
        headers: headers(request),
        body,
        requestId: requestId(request),
      },
      {
        auth: {
          bearerToken: state.config.admin.token ?? "",
          operatorId: "local-operator",
        },
        auditSink: state.auditSink,
        executor: createServiceControlExecutor(state),
        now: state.now,
      },
    );
    return result;
  }

  if (url.pathname.startsWith("/v1/debug/")) {
    return handleDirectDebugRequest(request, url, state);
  }

  if (url.pathname.startsWith("/v1/admin/")) {
    return handleAdminDiagnosticsRequest(
      {
        method: request.method ?? "GET",
        url: url.toString(),
        requestId: requestId(request),
      },
      await buildDiagnosticsContext(state),
    );
  }

  return failure(404, requestId(request), {
    code: "not_found",
    reason_code: "unknown_service_route",
    message: `unknown service route ${url.pathname}`,
    retryable: false,
  });
}

async function handleDirectDebugRequest(
  request: IncomingMessage,
  url: URL,
  state: ServiceState,
): Promise<AdminRouteResult> {
  const parts = url.pathname.split("/").filter(Boolean);
  if (
    parts.length === 5 &&
    parts[0] === "v1" &&
    parts[1] === "debug" &&
    parts[2] === "sessions" &&
    parts[4] === "context"
  ) {
    if ((request.method ?? "GET").toUpperCase() !== "GET") {
      return failure(405, requestId(request), {
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
      await buildDirectDebugContext(state),
    );
    return directDebugResult(requestId(request), result);
  }

  if (
    parts.length === 5 &&
    parts[0] === "v1" &&
    parts[1] === "debug" &&
    parts[2] === "sessions" &&
    parts[4] === "turn"
  ) {
    const body = recordBody(await readJsonBody(request));
    const result = await requestDirectDebugTurn(
      {
        ...body,
        sessionId: decodeURIComponent(parts[3] ?? ""),
      } as never,
      await buildDirectDebugContext(state),
    );
    return directDebugResult(requestId(request), result);
  }

  return failure(404, requestId(request), {
    code: "not_found",
    reason_code: "unknown_debug_route",
    message: `unknown debug route ${url.pathname}`,
    retryable: false,
  });
}

async function buildDiagnosticsContext(
  state: ServiceState,
): Promise<AdminDiagnosticsContext> {
  const now = state.now();
  const [runtimeSummary, tableCounts] = await Promise.all([
    state.bridge
      .runtimeSummary({ scopeType: "runtime" })
      .catch(() => undefined),
    collectTableCounts(state.bridge),
  ]);
  const diagnostics = buildRuntimeDiagnosticsProjection({
    now,
    runtimeSummary,
    sessions: [],
    delegatedSessions: [],
    persistence: {
      tableCounts,
      searchHealthy: true,
    },
    recentErrors: state.stopping
      ? [
          {
            source: "service-host",
            message: "service shutdown is in progress",
            reasonCode: "blocked_dependency",
            observedAt: now,
          },
        ]
      : [],
  });
  return {
    diagnostics,
    recentEvents: [
      {
        id: "service-runtime-config",
        createdAt: now,
        source: "service-host",
        eventType: "runtime_config_applied",
        summary: `Runtime config applied: ${state.runtimeConfigApplyResult.brainsRegistered} brains registered, ${state.runtimeConfigApplyResult.sessionsCreated} sessions created.`,
      },
    ],
  };
}

async function buildDirectDebugContext(
  state: ServiceState,
): Promise<DirectDebugServiceContext> {
  const diagnosticsContext = await buildDiagnosticsContext(state);
  const sessions = await Promise.all(
    state.runtimeConfig.sessions.map(async (configured, index) => {
      const profileContext = await loadProfileContext({
        profilesDir: state.runtimeConfig.profilesDir,
        skillsDir: state.runtimeConfig.skillsDir,
        profileId: configured.profileId,
      });
      const now = state.now();
      const session: SessionState = {
        handle: index as never,
        sessionId: configured.sessionId,
        agentId: configured.agentId,
        profileId: configured.profileId,
        kind: configured.kind,
        resourceLimits:
          profileContext.profile.runtime?.defaultResourceLimits ?? {},
        toolProfile: profileContext.toolSelection.toolProfile,
        status: "active",
        brainTurnCount: 0,
        createdAt: now,
        lastActiveAt: now,
      };
      return {
        session,
        profileContext,
        toolSelection: profileContext.toolSelection,
        systemPrompt: profileContext.profile.prompt?.system,
        roleAssembly: {
          instructions:
            profileContext.profile.prompt?.instructions?.join("\n\n"),
          initialMessages: [],
        },
      };
    }),
  );
  return {
    diagnostics: diagnosticsContext.diagnostics,
    sessions,
    recentEvents: diagnosticsContext.recentEvents,
    allowDirectTurnInjection: true,
    now: state.now,
    turnExecutor: {
      submitDirectDebugTurn: async (input) => {
        const receipt = await state.bridge.routeAgentMessage(
          input.actorId,
          input.session.agentId,
          input.body,
          input.idempotencyKey,
        );
        const status = receipt.accepted ? "accepted" : "rejected";
        return {
          status,
          summary: receipt.accepted
            ? "direct debug turn accepted"
            : "direct debug turn rejected",
          messageId: String(receipt.sequence),
        };
      },
    },
  };
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

function createServiceControlExecutor(
  state: ServiceState,
): AdminControlExecutor {
  return {
    createSession: async (command) => {
      const sessionId = requiredBodyString(command, "sessionId");
      const agentId = requiredBodyString(command, "agentId");
      const profileId = requiredBodyString(command, "profileId");
      const kind = optionalBodyString(command, "kind") ?? "full";
      if (kind !== "full" && kind !== "worker" && kind !== "delegated") {
        throw new Error("session kind must be full, worker, or delegated");
      }
      const session = await state.bridge.createSession({
        sessionId,
        agentId,
        profileId,
        kind,
      });
      return {
        status: "completed",
        summary: `session ${session.sessionId} created`,
        affectedIds: { sessionId: session.sessionId },
        result: session,
      };
    },
    cancelDelegation: async (command) => {
      const session = await state.bridge.cancelDelegatedSession(
        command.target.sessionId as never,
      );
      return {
        status: "completed",
        summary: `delegated session ${session.sessionId} cancelled`,
        affectedIds: { sessionId: session.sessionId },
        result: session,
      };
    },
    requestDelegatedCheckpoint: async (command) => {
      const receipt = await state.bridge.requestDelegatedCheckpoint({
        parentSessionId: command.target.parentSessionId as never,
        delegatedSessionId: command.target.sessionId as never,
        reason: command.reason ?? "admin requested checkpoint",
      });
      return {
        status: "completed",
        summary: `checkpoint requested for delegated session ${command.target.sessionId}`,
        affectedIds: { sequence: receipt.sequence },
        result: receipt,
      };
    },
    schedulerTick: async () => {
      const report = await state.bridge.runSchedulerTick();
      return {
        status: "completed",
        summary: "scheduler tick completed",
        result: report,
      };
    },
    schedulerRunJob: async (command) => {
      const run = await state.bridge.requestScheduledJobRun(
        command.target.jobId,
      );
      return {
        status: "completed",
        summary: run
          ? `scheduled job ${command.target.jobId} run requested`
          : `scheduled job ${command.target.jobId} was not due or not found`,
        affectedIds: run ? { jobId: command.target.jobId } : undefined,
        result: run ?? null,
      };
    },
    schedulerPauseJob: async (command) => {
      await state.bridge.pauseScheduledJob(command.target.jobId);
      return {
        status: "completed",
        summary: `scheduled job ${command.target.jobId} paused`,
        affectedIds: { jobId: command.target.jobId },
      };
    },
    schedulerResumeJob: async (command) => {
      const nextDueAt = requiredBodyString(command, "nextDueAt");
      await state.bridge.resumeScheduledJob({
        jobId: command.target.jobId,
        nextDueAt,
      });
      return {
        status: "completed",
        summary: `scheduled job ${command.target.jobId} resumed`,
        affectedIds: { jobId: command.target.jobId },
      };
    },
    cleanupDelegatedResources: async () => {
      const report = await state.bridge.cleanupDelegatedResources();
      return {
        status: "completed",
        summary: "delegated resource cleanup completed",
        result: report,
      };
    },
    shutdown: async () => {
      setTimeout(() => {
        void stopService(state).catch(() => undefined);
      }, 0);
      return {
        status: "completed",
        summary: "shutdown requested",
        affectedIds: { engine: Number(state.engine) },
      };
    },
  };
}

async function collectTableCounts(
  bridge: NativeBridgeModule,
): Promise<Record<string, number>> {
  const tables = [
    "sessions",
    "agent_messages",
    "queued_messages",
    "completion_packets",
    "worker_runs",
    "tool_call_history",
  ];
  const counts: Record<string, number> = {};
  for (const table of tables) {
    try {
      counts[table] = await bridge.diagnosticCountRows(table);
    } catch {
      counts[table] = 0;
    }
  }
  return counts;
}

function requiredBodyString(command: AdminControlCommand, key: string): string {
  const value = optionalBodyString(command, key);
  if (!value) throw new Error(`control body field ${key} is required`);
  return value;
}

function optionalBodyString(
  command: AdminControlCommand,
  key: string,
): string | undefined {
  const value = command.body[key];
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

async function stopService(
  state: ServiceState,
  server?: Server,
): Promise<void> {
  if (state.stopping) return;
  state.stopping = true;
  if (server) await closeServer(server);
  try {
    await state.bridge.shutdownEngine({
      engine: state.engine,
      drainTimeoutMs: 5_000,
    });
  } finally {
    state.lock.release();
  }
}

function listen(server: Server, port: number, host: string): Promise<void> {
  return new Promise((resolveListen, rejectListen) => {
    const onError = (error: Error) => {
      server.off("listening", onListening);
      rejectListen(error);
    };
    const onListening = () => {
      server.off("error", onError);
      resolveListen();
    };
    server.once("error", onError);
    server.once("listening", onListening);
    server.listen(port, host);
  });
}

function closeServer(server: Server): Promise<void> {
  return new Promise((resolveClose, rejectClose) => {
    if (!server.listening) {
      resolveClose();
      return;
    }
    server.close((error) => (error ? rejectClose(error) : resolveClose()));
  });
}

function writeJsonResponse(
  response: import("node:http").ServerResponse,
  result: AdminRouteResult,
): void {
  for (const [name, value] of Object.entries(result.headers)) {
    response.setHeader(name, value);
  }
  response.statusCode = result.status;
  response.end(JSON.stringify(result.body));
}

function failure(
  status: number,
  requestIdValue: string,
  error: {
    code:
      | "unauthorized"
      | "forbidden"
      | "method_not_allowed"
      | "not_found"
      | "invalid_input"
      | "failed_precondition"
      | "conflict"
      | "internal_error";
    reason_code: string;
    message: string;
    retryable: boolean;
  },
): AdminRouteResult {
  return {
    status,
    headers: { "content-type": "application/json" },
    body: {
      ok: false,
      error,
      meta: { request_id: requestIdValue, schema_version: 1 },
    },
  };
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

function optionalInteger(value: string | null): number | undefined {
  if (value === null || value.trim() === "") return undefined;
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) && parsed >= 0 ? parsed : undefined;
}

function recordBody(value: unknown): Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

function requestId(request: IncomingMessage): string {
  const value = request.headers["x-request-id"];
  return typeof value === "string" && value.trim()
    ? value.trim()
    : `req_${Date.now()}`;
}

function headers(request: IncomingMessage): Record<string, string | undefined> {
  const result: Record<string, string | undefined> = {};
  for (const [name, value] of Object.entries(request.headers)) {
    result[name] = Array.isArray(value) ? value.join(", ") : value;
  }
  return result;
}

function isAuthorized(
  request: IncomingMessage,
  token: string | undefined,
): boolean {
  return Boolean(token) && request.headers.authorization === `Bearer ${token}`;
}

async function readJsonBody(request: IncomingMessage): Promise<unknown> {
  const chunks: Buffer[] = [];
  for await (const chunk of request) {
    chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
    if (Buffer.concat(chunks).byteLength > 1024 * 1024) {
      throw new Error("admin request body exceeds 1 MiB");
    }
  }
  const raw = Buffer.concat(chunks).toString("utf8").trim();
  if (!raw) return {};
  return JSON.parse(raw) as unknown;
}

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback;
}

async function main(): Promise<void> {
  const host = await startRustyCrewServiceHost();
  console.log(`rusty-crew service listening on ${host.url}`);
  const shutdown = () => {
    void host.stop().finally(() => process.exit(0));
  };
  process.once("SIGINT", shutdown);
  process.once("SIGTERM", shutdown);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  void main().catch((error) => {
    console.error(errorMessage(error, "rusty-crew service failed"));
    process.exit(1);
  });
}
