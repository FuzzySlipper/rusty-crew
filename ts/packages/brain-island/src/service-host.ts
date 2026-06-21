import { createServer, type IncomingMessage, type Server } from "node:http";
import { fileURLToPath } from "node:url";
import type {
  BrainImplementationHandle,
  ChannelBindingRecord,
  CoreEvent,
  EngineHandle,
  SessionId,
  SessionState,
  SubscriptionHandle,
} from "@rusty-crew/contracts";
import {
  loadNativeBridge,
  type NativeBridgeModule,
} from "@rusty-crew/native-bridge";
import {
  McpSurfaceManager,
  createSimulatedMcpTransportFactory,
} from "@rusty-crew/adapter-mcp";
import {
  createDenSuccessorGatewayClient,
  type DenSuccessorAgentIdentity,
  type DenSuccessorDeliveryIntent,
  type DenSuccessorGatewayClient,
} from "@rusty-crew/adapter-den";
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
  buildAdapterDiagnosticsProjection,
  type AdapterDiagnosticsProjection,
} from "./adapter-diagnostics.js";
import {
  inspectDirectDebugSession,
  requestDirectDebugTurn,
  type DirectDebugResult,
  type DirectDebugServiceContext,
} from "./direct-debug-service.js";
import { loadProfileContext } from "./profile-loading.js";
import { buildProfileRoleAssembly } from "./profile-role-assembly.js";
import { buildRuntimeDiagnosticsProjection } from "./runtime-diagnostics.js";
import type { RuntimeHealthProjection } from "./runtime-health.js";
import {
  announceConfiguredSessionsToDenGateway,
  denGatewayStartupSummary,
  heartbeatConfiguredSessionsToDenRuntime,
  type DenSuccessorGatewayStartupReport,
} from "./den-successor-service.js";
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
  runtimeConfig: RustyCrewRuntimeConfig;
  runtimeConfigApplyResult: RustyCrewRuntimeConfigApplyResult;
  denGatewayClient?: DenSuccessorGatewayClient;
  denGatewayStartupReport?: DenSuccessorGatewayStartupReport;
  readonly denConversationChannelIdsByExternalId: Map<string, number>;
  mcpManager: McpSurfaceManager;
  readonly wakeSubscription: SubscriptionHandle;
  readonly timers: Set<NodeJS.Timeout>;
  readonly inFlightWakes: Set<SessionId>;
  readonly claimedDeliveryIntentIds: Set<number>;
  readonly directDispatchSessions: Set<SessionId>;
  readonly suppressedWakeEvents: Map<SessionId, number>;
  readonly recentEvents: ServiceRecentEvent[];
  readonly now: () => string;
  nextWakeSequence: number;
  stopping: boolean;
}

interface ServiceRecentEvent {
  id: string;
  createdAt: string;
  source: string;
  eventType: string;
  summary: string;
  severity?: string;
}

type ServiceRouteResult =
  | AdminRouteResult
  | {
      status: number;
      headers: Record<string, string>;
      body: string;
    };

const CONTROL_ROUTE_PREFIX = "/v1/admin/control/";
const DEV_NO_AUTH_CONTROL_TOKEN = "__rusty_crew_dev_no_auth__";

export async function startRustyCrewServiceHost(
  options: RustyCrewServiceHostOptions = {},
): Promise<RustyCrewServiceHost> {
  const config = options.config ?? loadRustyCrewServiceConfig(options.env);

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
    const wakeSubscription = await bridge.subscribeEvents({
      eventKinds: ["brain_wake_requested"],
    });

    const state: ServiceState = {
      config,
      bridge,
      engine,
      lock,
      auditSink: createMemoryAdminControlAuditSink(),
      runtimeConfig,
      runtimeConfigApplyResult,
      denGatewayClient:
        config.denSuccessorGateway === undefined
          ? undefined
          : createDenSuccessorGatewayClient(config.denSuccessorGateway),
      denConversationChannelIdsByExternalId: new Map(),
      mcpManager: await createServiceMcpManager(runtimeConfig),
      wakeSubscription,
      timers: new Set(),
      inFlightWakes: new Set(),
      claimedDeliveryIntentIds: new Set(),
      directDispatchSessions: new Set(),
      suppressedWakeEvents: new Map(),
      recentEvents: [],
      now: options.now ?? (() => new Date().toISOString()),
      nextWakeSequence: 0,
      stopping: false,
    };
    state.denGatewayStartupReport = await connectDenSuccessorGateway(state);
    await ensureDenConversationChannels(state);
    startServiceBackgroundLoops(state);
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
): Promise<ServiceRouteResult> {
  const url = new URL(request.url ?? "/", "http://rusty-crew.local");
  if (isAdminPanelRoute(url.pathname)) {
    return htmlResponse(adminPanelHtml(configRequiresAuth(state.config)));
  }

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

  if (!isAuthorized(request, state.config.admin.token, state)) {
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
        headers: controlHeaders(request, state),
        body,
        requestId: requestId(request),
      },
      {
        auth: {
          bearerToken: controlBearerToken(state),
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
  const [runtimeSummary, sessions, tableCounts, databaseSize] =
    await Promise.all([
      state.bridge
        .runtimeSummary({ scopeType: "runtime" })
        .catch(() => undefined),
      state.bridge.listSessions().catch(() => []),
      collectTableCounts(state.bridge),
      state.bridge.databaseSize().catch(() => undefined),
    ]);
  const diagnostics = buildRuntimeDiagnosticsProjection({
    now,
    runtimeSummary,
    sessions,
    delegatedSessions: [],
    adapters: buildServiceAdapterDiagnostics(state, now),
    persistence: {
      tableCounts,
      searchHealthy: true,
      databaseBytes: databaseSize?.databaseBytes,
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
        summary: runtimeConfigApplySummary(
          "Runtime config applied",
          state.runtimeConfigApplyResult,
        ),
      },
      ...state.recentEvents,
    ],
  };
}

function runtimeConfigApplySummary(
  prefix: string,
  result: RustyCrewRuntimeConfigApplyResult,
): string {
  return `${prefix}: ${result.brainsRegistered} brains registered, ${result.brainsAlreadyPresent} brains already present, ${result.sessionsCreated} sessions created, ${result.sessionsAlreadyPresent} sessions already present, ${result.sessionsReactivated} sessions reactivated, ${result.sessionsMissing} configured sessions missing.`;
}

function buildServiceAdapterDiagnostics(
  state: ServiceState,
  now: string,
): AdapterDiagnosticsProjection | undefined {
  if (
    state.runtimeConfig.channelBindings.length === 0 &&
    state.runtimeConfig.mcpBindings.length === 0
  ) {
    return undefined;
  }
  return buildAdapterDiagnosticsProjection({
    now,
    channelBindings: state.runtimeConfig.channelBindings,
    mcpBindings: state.runtimeConfig.mcpBindings,
    mcpSurfaces: state.mcpManager.diagnostics(),
  });
}

async function createServiceMcpManager(
  runtimeConfig: RustyCrewRuntimeConfig,
): Promise<McpSurfaceManager> {
  const manager = new McpSurfaceManager({
    transports: [
      createSimulatedMcpTransportFactory("stdio"),
      createSimulatedMcpTransportFactory("streamable_http"),
      createSimulatedMcpTransportFactory("websocket"),
    ],
  });
  for (const binding of runtimeConfig.mcpBindings) {
    await manager.connect(binding);
  }
  return manager;
}

async function connectDenSuccessorGateway(
  state: ServiceState,
): Promise<DenSuccessorGatewayStartupReport | undefined> {
  if (state.config.denSuccessorGateway === undefined) {
    return undefined;
  }
  if (state.denGatewayClient === undefined) {
    return undefined;
  }
  let report: DenSuccessorGatewayStartupReport;
  try {
    report = await announceConfiguredSessionsToDenGateway({
      client: state.denGatewayClient,
      sessions: state.runtimeConfig.sessions,
      now: state.now(),
    });
  } catch (error) {
    report = {
      enabled: true,
      sessionsAnnounced: 0,
      runtimeInstancesRegistered: 0,
      runtimeInstancesHeartbeated: 0,
      failures: [
        errorMessage(error, "Den successor Gateway connection failed"),
      ],
    };
  }
  recordServiceEvent(state, {
    source: "den-successor-gateway",
    eventType:
      report.failures.length === 0
        ? "den_successor_gateway_connected"
        : "den_successor_gateway_degraded",
    summary: denGatewayStartupSummary(report),
    severity: report.failures.length === 0 ? "info" : "warning",
  });
  return report;
}

async function ensureDenConversationChannels(
  state: ServiceState,
): Promise<void> {
  if (state.denGatewayClient === undefined) return;
  const bindings = activeDenChannelBindings(
    state.runtimeConfig.channelBindings,
  );
  if (bindings.length === 0) {
    state.denConversationChannelIdsByExternalId.clear();
    return;
  }

  try {
    const channels = await state.denGatewayClient.listConversationChannels({
      projectId: state.config.denConversationProjectId,
      limit: 100,
    });
    const channelsBySlug = new Map(
      channels.map((channel) => [channel.slug, channel]),
    );
    const nextChannelIds = new Map<string, number>();
    let created = 0;
    for (const binding of bindings) {
      const existing = channelsBySlug.get(binding.externalChannelId);
      if (existing !== undefined) {
        nextChannelIds.set(binding.externalChannelId, existing.id);
        continue;
      }
      const channel = await state.denGatewayClient.createConversationChannel({
        slug: binding.externalChannelId,
        display_name: displayNameForConversationBinding(binding),
        kind: "agent_channel",
        project_id: state.config.denConversationProjectId,
        created_by: "rusty-crew",
        visibility: "normal",
        settings: {
          adapter_id: binding.adapterId,
          binding_id: binding.bindingId,
          provider: binding.provider,
          profile_id: binding.profileId,
          agent_id: binding.agentId,
        },
      });
      created += 1;
      channelsBySlug.set(channel.slug, channel);
      nextChannelIds.set(binding.externalChannelId, channel.id);
    }
    state.denConversationChannelIdsByExternalId.clear();
    for (const [externalChannelId, channelId] of nextChannelIds) {
      state.denConversationChannelIdsByExternalId.set(
        externalChannelId,
        channelId,
      );
    }
    recordServiceEvent(state, {
      source: "den-successor-gateway",
      eventType: "den_conversation_channels_resolved",
      summary: `Resolved ${nextChannelIds.size} Den Conversation channel(s), created ${created}.`,
    });
  } catch (error) {
    recordServiceEvent(state, {
      source: "den-successor-gateway",
      eventType: "den_conversation_channels_degraded",
      severity: "warning",
      summary: errorMessage(
        error,
        "Den Conversation channel resolution failed",
      ),
    });
  }
}

function activeDenChannelBindings(
  bindings: readonly ChannelBindingRecord[],
): ChannelBindingRecord[] {
  return bindings.filter(
    (binding) =>
      binding.status === "active" &&
      binding.provider === "den_channels" &&
      binding.externalChannelId.trim(),
  );
}

function displayNameForConversationBinding(
  binding: ChannelBindingRecord,
): string {
  return `${binding.agentId} (${binding.externalChannelId})`;
}

async function reloadServiceRuntimeConfig(
  state: ServiceState,
): Promise<RustyCrewRuntimeConfigApplyResult> {
  const nextRuntimeConfig = await loadRustyCrewRuntimeConfig(state.config);
  const nextApplyResult = await applyRustyCrewRuntimeConfig({
    serviceConfig: state.config,
    runtimeConfig: nextRuntimeConfig,
    bridge: state.bridge,
    existingBrainHandlesByProfileId:
      state.runtimeConfigApplyResult.brainHandlesByProfileId,
    createMissingSessions: false,
  });
  const nextMcpManager = await createServiceMcpManager(nextRuntimeConfig);
  const previousMcpManager = state.mcpManager;
  state.runtimeConfig = nextRuntimeConfig;
  state.runtimeConfigApplyResult = nextApplyResult;
  state.mcpManager = nextMcpManager;
  await previousMcpManager.shutdown();
  await ensureDenConversationChannels(state);
  recordServiceEvent(state, {
    source: "service-host",
    eventType: "runtime_config_reloaded",
    summary: runtimeConfigApplySummary(
      "Runtime config reloaded",
      nextApplyResult,
    ),
  });
  return nextApplyResult;
}

async function buildDirectDebugContext(
  state: ServiceState,
): Promise<DirectDebugServiceContext> {
  const diagnosticsContext = await buildDiagnosticsContext(state);
  const runtimeSessions = await state.bridge.listSessions().catch(() => []);
  const debugSessions =
    runtimeSessions.length > 0
      ? runtimeSessions
      : configuredDebugSessionFallback(state);
  const sessions = (
    await Promise.all(
      debugSessions.map(async (session) => {
        try {
          const profileContext = await loadProfileContext({
            profilesDir: state.runtimeConfig.profilesDir,
            skillsDir: state.runtimeConfig.skillsDir,
            profileId: session.profileId,
          });
          return {
            session: {
              ...session,
              toolProfile:
                session.toolProfile.tools.length > 0
                  ? session.toolProfile
                  : profileContext.toolSelection.toolProfile,
            },
            profileContext,
            toolSelection: profileContext.toolSelection,
            systemPrompt: profileContext.profile.prompt?.system,
            roleAssembly: {
              instructions:
                profileContext.profile.prompt?.instructions?.join("\n\n"),
              initialMessages: [],
            },
          };
        } catch (error) {
          if (session.status === "archived") return undefined;
          throw error;
        }
      }),
    )
  ).filter((session): session is NonNullable<typeof session> =>
    Boolean(session),
  );
  return {
    diagnostics: diagnosticsContext.diagnostics,
    sessions,
    adapters: diagnosticsContext.diagnostics.adapters,
    recentEvents: diagnosticsContext.recentEvents,
    allowDirectTurnInjection: true,
    now: state.now,
    turnExecutor: {
      submitDirectDebugTurn: async (input) => {
        state.directDispatchSessions.add(input.session.sessionId);
        try {
          let wakeReport: ServiceWakeDispatchReport | undefined;
          const queued = await state.bridge.enqueueBodyFollowUpMessage({
            sessionId: input.session.sessionId,
            from: input.actorId as never,
            body: input.body,
            correlationId: input.idempotencyKey,
          });
          wakeReport = await dispatchWake(
            state,
            {
              type: "brain_wake_requested",
              sessionId: input.session.sessionId,
            },
            "direct_debug",
          );
          suppressNextWakeEvent(state, input.session.sessionId);
          await drainAndDispatchWakes(state, "direct_debug");
          return {
            status: "accepted",
            summary: wakeReport
              ? wakeReport.summary
              : "direct debug turn accepted",
            wakeId: wakeReport?.wakeId,
            reasonCode: wakeReport?.reasonCode,
            messageId: queued.messageId,
          };
        } finally {
          state.directDispatchSessions.delete(input.session.sessionId);
        }
      },
    },
  };
}

function configuredDebugSessionFallback(state: ServiceState): SessionState[] {
  const now = state.now();
  return state.runtimeConfig.sessions.map((configured, index) => ({
    handle: index as never,
    sessionId: configured.sessionId,
    agentId: configured.agentId,
    profileId: configured.profileId,
    kind: configured.kind,
    resourceLimits: {},
    toolProfile: { tools: [] },
    status: "active",
    brainTurnCount: 0,
    createdAt: now,
    lastActiveAt: now,
  }));
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
    reloadConfig: async () => {
      const result = await reloadServiceRuntimeConfig(state);
      return {
        status: "completed",
        summary: runtimeConfigApplySummary("runtime config reloaded", result),
        affectedIds: {
          brainsRegistered: result.brainsRegistered,
          sessionsCreated: result.sessionsCreated,
          sessionsReactivated: result.sessionsReactivated,
          sessionsMissing: result.sessionsMissing,
        },
        result,
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
    runMaintenance: async (command) => {
      const report = await state.bridge.runMaintenance({
        expireQueuedMessagesAt: optionalBodyString(
          command,
          "expireQueuedMessagesAt",
        ),
        purgeTerminalQueuedMessagesBefore: optionalBodyString(
          command,
          "purgeTerminalQueuedMessagesBefore",
        ),
        runWalCheckpoint: optionalBodyBoolean(command, "runWalCheckpoint"),
        runOptimize: optionalBodyBoolean(command, "runOptimize"),
      });
      return {
        status: "completed",
        summary: "runtime maintenance completed",
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

function optionalBodyBoolean(
  command: AdminControlCommand,
  key: string,
): boolean | undefined {
  const value = command.body[key];
  return typeof value === "boolean" ? value : undefined;
}

interface ServiceWakeDispatchReport {
  sessionId: SessionId;
  wakeId?: string;
  status: "completed" | "skipped" | "failed";
  summary: string;
  reasonCode?: string;
}

function startServiceBackgroundLoops(state: ServiceState): void {
  if (state.config.background.schedulerTickIntervalMs > 0) {
    const timer = setInterval(() => {
      void runSchedulerHeartbeat(state).catch((error) =>
        recordServiceEvent(state, {
          source: "service-host",
          eventType: "scheduler_heartbeat_failed",
          severity: "error",
          summary: errorMessage(error, "scheduler heartbeat failed"),
        }),
      );
    }, state.config.background.schedulerTickIntervalMs);
    state.timers.add(timer);
  }

  if (state.config.background.wakeDispatchIntervalMs > 0) {
    const timer = setInterval(() => {
      void drainAndDispatchWakes(state, "background").catch((error) =>
        recordServiceEvent(state, {
          source: "service-host",
          eventType: "wake_dispatch_failed",
          severity: "error",
          summary: errorMessage(error, "wake dispatch failed"),
        }),
      );
    }, state.config.background.wakeDispatchIntervalMs);
    state.timers.add(timer);
  }

  if (
    state.denGatewayClient !== undefined &&
    state.config.background.denRuntimeHeartbeatIntervalMs > 0
  ) {
    const timer = setInterval(() => {
      void heartbeatDenRuntimeInstances(state).catch((error) =>
        recordServiceEvent(state, {
          source: "den-successor-gateway",
          eventType: "den_runtime_heartbeat_failed",
          severity: "error",
          summary: errorMessage(error, "Den Runtime heartbeat failed"),
        }),
      );
    }, state.config.background.denRuntimeHeartbeatIntervalMs);
    state.timers.add(timer);
  }

  if (
    state.denGatewayClient !== undefined &&
    state.config.background.denDeliveryPollIntervalMs > 0
  ) {
    const timer = setInterval(() => {
      void pollDenDeliveryIntents(state).catch((error) =>
        recordServiceEvent(state, {
          source: "den-successor-gateway",
          eventType: "den_delivery_poll_failed",
          severity: "error",
          summary: errorMessage(error, "Den Delivery poll failed"),
        }),
      );
    }, state.config.background.denDeliveryPollIntervalMs);
    state.timers.add(timer);
  }
}

async function heartbeatDenRuntimeInstances(
  state: ServiceState,
): Promise<void> {
  if (state.stopping || state.denGatewayClient === undefined) return;
  const report = await heartbeatConfiguredSessionsToDenRuntime({
    client: state.denGatewayClient,
    sessions: state.runtimeConfig.sessions,
  });
  if (report.failures.length > 0) {
    recordServiceEvent(state, {
      source: "den-successor-gateway",
      eventType: "den_runtime_heartbeat_degraded",
      severity: "warning",
      summary: `Den Runtime heartbeat: ${report.heartbeated} session(s) heartbeated, ${report.failures.length} failure(s): ${report.failures.join("; ")}`,
    });
  }
}

async function pollDenDeliveryIntents(state: ServiceState): Promise<void> {
  if (state.stopping || state.denGatewayClient === undefined) return;
  const intents = await state.denGatewayClient.listDeliveryIntents("pending");
  for (const intent of intents) {
    if (state.claimedDeliveryIntentIds.has(intent.id)) continue;
    const session = configuredSessionForDeliveryIntent(state, intent);
    if (session === undefined) continue;
    state.claimedDeliveryIntentIds.add(intent.id);
    void processDenDeliveryIntent(state, intent, session).catch((error) =>
      recordServiceEvent(state, {
        source: "den-successor-gateway",
        eventType: "den_delivery_intent_failed",
        severity: "error",
        summary: errorMessage(error, `Den Delivery intent ${intent.id} failed`),
      }),
    );
  }
}

async function processDenDeliveryIntent(
  state: ServiceState,
  intent: DenSuccessorDeliveryIntent,
  session: RustyCrewRuntimeConfig["sessions"][number],
): Promise<void> {
  if (state.denGatewayClient === undefined) return;
  const claimToken = `rusty-crew:${intent.id}:${Date.now()}`;
  const claimedBy = claimIdentityForDeliveryIntent(intent);
  let claimed = false;
  try {
    await state.denGatewayClient.claimDeliveryIntent({
      id: intent.id,
      claimToken,
      claimedBy,
    });
    claimed = true;
    await state.denGatewayClient.reportDeliveryIntentEvent({
      id: intent.id,
      claimToken,
      eventType: "running",
      payload: { source: "rusty-crew", session_id: session.sessionId },
    });

    const deliveryBody = await deliveryIntentBody(state, intent, session);
    if (!deliveryBody.body.trim()) {
      throw new Error(
        "Delivery intent has no body in source_ref or channel message",
      );
    }

    const wakeReport = await submitServiceTurn(state, {
      sessionId: session.sessionId,
      from: "den-delivery",
      body: deliveryBody.body,
      correlationId: `delivery:${intent.id}:${intent.idempotency_key}`,
      source: "delivery",
    });
    if (wakeReport.status !== "completed") {
      throw new Error(wakeReport.summary);
    }

    if (deliveryBody.channelId !== undefined) {
      await state.denGatewayClient.appendConversationMessage({
        channelId: deliveryBody.channelId,
        idempotencyKey: `rusty-crew-delivery:${intent.id}:completion`,
        message: {
          sender_type: "agent",
          sender_identity: session.agentId,
          body: wakeReport.summary,
          message_kind: "message",
          source_kind: "rusty-crew",
          source_id: String(intent.id),
          profile_identity: session.profileId,
          agent_instance_id: claimedBy.instance_id,
          session_id: session.sessionId,
          metadata: {
            delivery_intent_id: intent.id,
            delivery_idempotency_key: intent.idempotency_key,
            source_message_id: intent.channel_message_id,
            wake_id: wakeReport.wakeId,
          },
          dedupe_key: `rusty-crew-delivery:${intent.id}:completion`,
        },
      });
    }

    await state.denGatewayClient.reportDeliveryIntentEvent({
      id: intent.id,
      claimToken,
      eventType: "completed",
      payload: {
        source: "rusty-crew",
        session_id: session.sessionId,
        summary: wakeReport.summary,
        wake_id: wakeReport.wakeId,
      },
    });
    recordServiceEvent(state, {
      source: "den-successor-gateway",
      eventType: "den_delivery_intent_completed",
      summary: `Den Delivery intent ${intent.id} completed for ${session.agentId}.`,
    });
  } catch (error) {
    if (claimed && state.denGatewayClient !== undefined) {
      await state.denGatewayClient
        .reportDeliveryIntentEvent({
          id: intent.id,
          claimToken,
          eventType: "failed",
          payload: {
            source: "rusty-crew",
            session_id: session.sessionId,
            reason: errorMessage(error, "Delivery intent failed"),
          },
        })
        .catch(() => undefined);
    }
    throw error;
  }
}

async function submitServiceTurn(
  state: ServiceState,
  input: {
    sessionId: SessionId;
    from: string;
    body: string;
    correlationId: string;
    source: "delivery" | "direct_debug";
  },
): Promise<ServiceWakeDispatchReport> {
  state.directDispatchSessions.add(input.sessionId);
  try {
    await state.bridge.enqueueBodyFollowUpMessage({
      sessionId: input.sessionId,
      from: input.from as never,
      body: input.body,
      correlationId: input.correlationId,
    });
    const wakeReport = await dispatchWake(
      state,
      {
        type: "brain_wake_requested",
        sessionId: input.sessionId,
      },
      input.source,
    );
    suppressNextWakeEvent(state, input.sessionId);
    await drainAndDispatchWakes(state, input.source);
    return wakeReport;
  } finally {
    state.directDispatchSessions.delete(input.sessionId);
  }
}

function claimIdentityForDeliveryIntent(
  intent: DenSuccessorDeliveryIntent,
): DenSuccessorAgentIdentity {
  return {
    profile: intent.target_identity.profile,
    instance_id: intent.target_identity.instance_id,
  };
}

function configuredSessionForDeliveryIntent(
  state: ServiceState,
  intent: DenSuccessorDeliveryIntent,
): RustyCrewRuntimeConfig["sessions"][number] | undefined {
  if (intent.target_identity.session_key !== undefined) return undefined;
  return state.runtimeConfig.sessions.find((session) => {
    const identity = deliveryIdentityForSession(session);
    return (
      intent.target_identity.profile === identity.profile &&
      intent.target_identity.instance_id === identity.instance_id
    );
  });
}

function deliveryIdentityForSession(
  session: RustyCrewRuntimeConfig["sessions"][number],
): DenSuccessorAgentIdentity {
  return {
    profile: session.profileId,
    instance_id: `${session.agentId}@rusty-crew`,
    session_key: session.sessionId,
  };
}

async function deliveryIntentBody(
  state: ServiceState,
  intent: DenSuccessorDeliveryIntent,
  session: RustyCrewRuntimeConfig["sessions"][number],
): Promise<{ body: string; channelId?: number; sourceMessageId?: number }> {
  const sourceBody = bodyFromWakeSourceRef(intent.source_ref);
  const channelId =
    channelIdFromDeliveryIntent(intent) ??
    channelIdForConfiguredSession(state, session);
  if (sourceBody !== undefined) {
    return {
      body: sourceBody,
      channelId,
      sourceMessageId: intent.channel_message_id,
    };
  }
  if (
    state.denGatewayClient !== undefined &&
    intent.channel_message_id !== undefined &&
    channelId !== undefined
  ) {
    const messages = await state.denGatewayClient.listConversationMessages({
      channelId,
      limit: 50,
    });
    const message = messages.find(
      (candidate) => candidate.id === intent.channel_message_id,
    );
    if (message !== undefined) {
      return {
        body: message.body,
        channelId: message.channel_id,
        sourceMessageId: message.id,
      };
    }
  }
  return { body: "", channelId, sourceMessageId: intent.channel_message_id };
}

function channelIdForConfiguredSession(
  state: ServiceState,
  session: RustyCrewRuntimeConfig["sessions"][number],
): number | undefined {
  const binding = activeDenChannelBindings(
    state.runtimeConfig.channelBindings,
  ).find(
    (candidate) =>
      candidate.agentId === session.agentId &&
      candidate.profileId === session.profileId &&
      (candidate.sessionId === undefined ||
        candidate.sessionId === session.sessionId),
  );
  if (binding === undefined) return undefined;
  return state.denConversationChannelIdsByExternalId.get(
    binding.externalChannelId,
  );
}

function bodyFromWakeSourceRef(
  sourceRef: string | undefined,
): string | undefined {
  if (!sourceRef?.trim()) return undefined;
  try {
    const parsed = new URL(sourceRef);
    const body = parsed.searchParams.get("body");
    return body?.trim() ? body : undefined;
  } catch {
    return undefined;
  }
}

function channelIdFromDeliveryIntent(
  intent: DenSuccessorDeliveryIntent,
): number | undefined {
  const [, channelPart] = intent.idempotency_key.split(":");
  const raw = channelPart?.startsWith("ch")
    ? channelPart.slice(2)
    : channelPart;
  if (!raw || !/^[0-9]+$/.test(raw)) return undefined;
  const parsed = Number(raw);
  return Number.isSafeInteger(parsed) ? parsed : undefined;
}

async function runSchedulerHeartbeat(state: ServiceState): Promise<void> {
  if (state.stopping) return;
  const tick = await state.bridge.runSchedulerTick();
  const maintenance = await state.bridge.runMaintenance({
    expireQueuedMessagesAt: state.now(),
  });
  if (
    tick.wakesRequested > 0 ||
    tick.runsCompleted > 0 ||
    tick.runsFailed > 0 ||
    maintenance.expiredQueueMessages > 0
  ) {
    recordServiceEvent(state, {
      source: "service-host",
      eventType: "scheduler_heartbeat",
      summary: `Scheduler heartbeat: ${tick.wakesRequested} wakes requested, ${tick.runsCompleted} runs completed, ${maintenance.expiredQueueMessages} queued messages expired.`,
    });
  }
}

async function drainAndDispatchWakes(
  state: ServiceState,
  source: "background" | "direct_debug" | "delivery",
): Promise<ServiceWakeDispatchReport[]> {
  if (state.stopping) return [];
  const events = await state.bridge.drainSubscriptionEvents(
    state.wakeSubscription,
    32,
  );
  const reports: ServiceWakeDispatchReport[] = [];
  for (const event of events) {
    if (event.type !== "brain_wake_requested") continue;
    if (consumeSuppressedWakeEvent(state, event.sessionId)) continue;
    if (
      source === "background" &&
      state.directDispatchSessions.has(event.sessionId)
    ) {
      continue;
    }
    reports.push(await dispatchWake(state, event, source));
  }
  return reports;
}

function suppressNextWakeEvent(
  state: ServiceState,
  sessionId: SessionId,
): void {
  state.suppressedWakeEvents.set(
    sessionId,
    (state.suppressedWakeEvents.get(sessionId) ?? 0) + 1,
  );
}

function consumeSuppressedWakeEvent(
  state: ServiceState,
  sessionId: SessionId,
): boolean {
  const count = state.suppressedWakeEvents.get(sessionId) ?? 0;
  if (count <= 0) return false;
  if (count === 1) state.suppressedWakeEvents.delete(sessionId);
  else state.suppressedWakeEvents.set(sessionId, count - 1);
  return true;
}

async function dispatchWake(
  state: ServiceState,
  event: Extract<CoreEvent, { type: "brain_wake_requested" }>,
  source: "background" | "direct_debug" | "delivery",
): Promise<ServiceWakeDispatchReport> {
  const sessionId = event.sessionId;
  if (state.inFlightWakes.has(sessionId)) {
    return {
      sessionId,
      status: "skipped",
      summary: `wake for ${sessionId} skipped because one is already in flight`,
      reasonCode: "wake_already_in_flight",
    };
  }

  state.inFlightWakes.add(sessionId);
  try {
    const session = (await state.bridge.listSessions()).find(
      (candidate) => candidate.sessionId === sessionId,
    );
    if (!session) {
      return wakeDispatchSkipped(
        state,
        sessionId,
        "wake_session_missing",
        `wake for ${sessionId} skipped because the session is missing`,
      );
    }
    if (session.status === "archived") {
      return wakeDispatchSkipped(
        state,
        sessionId,
        "wake_session_archived",
        `wake for ${sessionId} skipped because the session is archived`,
      );
    }

    const brain = brainForProfile(state, session.profileId);
    if (brain === undefined) {
      return wakeDispatchSkipped(
        state,
        sessionId,
        "wake_brain_missing",
        `wake for ${sessionId} skipped because profile ${session.profileId} has no registered brain`,
      );
    }

    const wakeId = nextWakeId(state, session);
    const profileContext = await loadProfileContext({
      profilesDir: state.runtimeConfig.profilesDir,
      skillsDir: state.runtimeConfig.skillsDir,
      profileId: session.profileId,
    });
    const role = buildProfileRoleAssembly(profileContext);
    const observed = await observeWakeEvents(state, sessionId, async () => {
      const request = await state.bridge.buildBrainWakeRequestForSession({
        brain,
        sessionId,
        systemPrompt: role.systemPrompt,
        roleAssemblyJson: new TextEncoder().encode(
          JSON.stringify(role.roleAssembly),
        ),
        wakeId,
      });
      return state.bridge.wakeBrain(request);
    });
    const accepted = observed.accepted;
    const completionSummary = wakeCompletionSummary(observed.events);
    const report: ServiceWakeDispatchReport = {
      sessionId,
      wakeId,
      status: accepted.accepted ? "completed" : "failed",
      summary:
        completionSummary ??
        (accepted.accepted
          ? `wake ${wakeId} completed for ${session.agentId}`
          : `wake ${wakeId} was rejected for ${session.agentId}`),
      reasonCode: accepted.accepted ? undefined : "wake_rejected",
    };
    recordServiceEvent(state, {
      source: "service-host",
      eventType: "brain_wake_dispatched",
      severity: accepted.accepted ? undefined : "error",
      summary: `${report.summary} (${source}).`,
    });
    return report;
  } catch (error) {
    const report: ServiceWakeDispatchReport = {
      sessionId,
      status: "failed",
      summary: errorMessage(error, `wake for ${sessionId} failed`),
      reasonCode: "wake_dispatch_failed",
    };
    recordServiceEvent(state, {
      source: "service-host",
      eventType: "brain_wake_failed",
      severity: "error",
      summary: report.summary,
    });
    return report;
  } finally {
    state.inFlightWakes.delete(sessionId);
  }
}

async function observeWakeEvents<T>(
  state: ServiceState,
  sessionId: SessionId,
  callback: () => Promise<T>,
): Promise<{ accepted: T; events: CoreEvent[] }> {
  const subscription = await state.bridge.subscribeEvents({
    eventKinds: [
      "brain_event_observed",
      "brain_actions_accepted",
      "completion_packet_delivered",
    ],
    sessionId,
  });
  try {
    const accepted = await callback();
    const events = await state.bridge.drainSubscriptionEvents(subscription, 64);
    return { accepted, events };
  } finally {
    await state.bridge.unsubscribeEvents(subscription).catch(() => undefined);
  }
}

function wakeCompletionSummary(
  events: readonly CoreEvent[],
): string | undefined {
  const packet = events
    .filter(
      (
        event,
      ): event is Extract<CoreEvent, { type: "completion_packet_delivered" }> =>
        event.type === "completion_packet_delivered",
    )
    .at(-1);
  if (packet?.packet.summary.trim()) {
    return packet.packet.summary.trim();
  }

  const text = events
    .flatMap((event) =>
      event.type === "brain_event_observed" && event.event.type === "text_delta"
        ? [event.event.text]
        : [],
    )
    .join("")
    .trim();
  return text ? truncate(text, 480) : undefined;
}

function truncate(value: string, maxChars: number): string {
  return value.length <= maxChars ? value : `${value.slice(0, maxChars)}...`;
}

function wakeDispatchSkipped(
  state: ServiceState,
  sessionId: SessionId,
  reasonCode: string,
  summary: string,
): ServiceWakeDispatchReport {
  recordServiceEvent(state, {
    source: "service-host",
    eventType: "brain_wake_skipped",
    severity: "warning",
    summary,
  });
  return { sessionId, status: "skipped", summary, reasonCode };
}

function brainForProfile(
  state: ServiceState,
  profileId: string,
): BrainImplementationHandle | undefined {
  return state.runtimeConfigApplyResult.brainHandlesByProfileId[profileId];
}

function nextWakeId(state: ServiceState, session: SessionState): string {
  state.nextWakeSequence += 1;
  return `service-${session.sessionId}-${Date.now()}-${state.nextWakeSequence}`;
}

function recordServiceEvent(
  state: ServiceState,
  event: Omit<ServiceRecentEvent, "id" | "createdAt">,
): void {
  const createdAt = state.now();
  state.recentEvents.unshift({
    id: `service-event-${Date.now()}-${state.recentEvents.length}`,
    createdAt,
    ...event,
  });
  state.recentEvents.splice(50);
}

async function stopService(
  state: ServiceState,
  server?: Server,
): Promise<void> {
  if (state.stopping) return;
  state.stopping = true;
  for (const timer of state.timers) clearInterval(timer);
  state.timers.clear();
  if (server) await closeServer(server);
  try {
    await state.bridge
      .unsubscribeEvents(state.wakeSubscription)
      .catch(() => undefined);
    await state.mcpManager.shutdown();
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
  result: ServiceRouteResult,
): void {
  for (const [name, value] of Object.entries(result.headers)) {
    response.setHeader(name, value);
  }
  response.statusCode = result.status;
  response.end(
    typeof result.body === "string" ? result.body : JSON.stringify(result.body),
  );
}

function isAdminPanelRoute(pathname: string): boolean {
  return pathname === "/" || pathname === "/admin" || pathname === "/admin/";
}

function htmlResponse(body: string): ServiceRouteResult {
  return {
    status: 200,
    headers: {
      "content-type": "text/html; charset=utf-8",
      "cache-control": "no-store",
    },
    body,
  };
}

function adminPanelHtml(authRequired: boolean): string {
  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Rusty Crew Admin</title>
  <style>
    :root {
      color-scheme: light;
      --bg: #f6f7f9;
      --panel: #ffffff;
      --panel-strong: #eef2f6;
      --text: #17202a;
      --muted: #607083;
      --border: #d7dee7;
      --good: #147a4a;
      --warn: #9c5a00;
      --bad: #b42318;
      --accent: #2457a6;
    }

    * {
      box-sizing: border-box;
    }

    body {
      margin: 0;
      background: var(--bg);
      color: var(--text);
      font-family: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      font-size: 14px;
      line-height: 1.45;
    }

    header {
      background: var(--panel);
      border-bottom: 1px solid var(--border);
    }

    .shell {
      width: min(1180px, calc(100% - 32px));
      margin: 0 auto;
    }

    .topbar {
      display: grid;
      grid-template-columns: minmax(180px, 1fr) minmax(280px, 520px);
      gap: 20px;
      align-items: center;
      padding: 22px 0;
    }

    h1 {
      margin: 0;
      font-size: 24px;
      font-weight: 700;
      letter-spacing: 0;
    }

    .subtitle {
      margin: 4px 0 0;
      color: var(--muted);
    }

    .token-row {
      display: grid;
      grid-template-columns: minmax(0, 1fr) auto;
      gap: 8px;
    }

    input {
      min-width: 0;
      height: 38px;
      border: 1px solid var(--border);
      border-radius: 6px;
      padding: 0 10px;
      font: inherit;
      background: #fff;
      color: var(--text);
    }

    button {
      height: 38px;
      border: 1px solid #1f4f95;
      border-radius: 6px;
      padding: 0 14px;
      background: var(--accent);
      color: #fff;
      font: inherit;
      font-weight: 650;
      cursor: pointer;
    }

    button.secondary {
      border-color: var(--border);
      background: #fff;
      color: var(--text);
    }

    main {
      padding: 20px 0 32px;
    }

    .status-line {
      display: flex;
      flex-wrap: wrap;
      gap: 8px;
      margin-bottom: 14px;
      color: var(--muted);
    }

    .pill {
      display: inline-flex;
      align-items: center;
      min-height: 26px;
      border: 1px solid var(--border);
      border-radius: 999px;
      padding: 3px 10px;
      background: var(--panel);
      color: var(--muted);
      font-size: 13px;
    }

    .pill.good {
      border-color: #9fd7b8;
      color: var(--good);
      background: #eefaf3;
    }

    .pill.warn {
      border-color: #f0c982;
      color: var(--warn);
      background: #fff7e8;
    }

    .pill.bad {
      border-color: #f1a39d;
      color: var(--bad);
      background: #fff1f0;
    }

    .grid {
      display: grid;
      grid-template-columns: repeat(12, 1fr);
      gap: 12px;
    }

    .panel {
      grid-column: span 6;
      background: var(--panel);
      border: 1px solid var(--border);
      border-radius: 8px;
      overflow: hidden;
    }

    .panel.wide {
      grid-column: span 12;
    }

    .panel h2 {
      margin: 0;
      padding: 12px 14px;
      border-bottom: 1px solid var(--border);
      background: var(--panel-strong);
      font-size: 15px;
      letter-spacing: 0;
    }

    .panel-body {
      padding: 12px 14px;
    }

    .metrics {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(130px, 1fr));
      gap: 8px;
    }

    .metric {
      min-height: 72px;
      border: 1px solid var(--border);
      border-radius: 6px;
      padding: 10px;
      background: #fbfcfd;
    }

    .metric span {
      display: block;
      color: var(--muted);
      font-size: 12px;
    }

    .metric strong {
      display: block;
      margin-top: 4px;
      font-size: 20px;
      font-weight: 720;
    }

    table {
      width: 100%;
      border-collapse: collapse;
      table-layout: fixed;
    }

    th,
    td {
      border-bottom: 1px solid var(--border);
      padding: 8px 6px;
      text-align: left;
      vertical-align: top;
      overflow-wrap: anywhere;
    }

    th {
      color: var(--muted);
      font-size: 12px;
      font-weight: 700;
    }

    tr:last-child td {
      border-bottom: 0;
    }

    .empty,
    .error {
      color: var(--muted);
      padding: 12px 0;
    }

    .error {
      color: var(--bad);
    }

    pre {
      max-height: 280px;
      overflow: auto;
      margin: 0;
      padding: 10px;
      border: 1px solid var(--border);
      border-radius: 6px;
      background: #101820;
      color: #e8eef5;
      font-size: 12px;
      line-height: 1.5;
      white-space: pre-wrap;
    }

    @media (max-width: 800px) {
      .topbar,
      .token-row {
        grid-template-columns: 1fr;
      }

      .panel {
        grid-column: span 12;
      }

      button {
        width: 100%;
      }
    }
  </style>
</head>
<body>
  <header>
    <div class="shell topbar">
      <div>
        <h1>Rusty Crew Admin</h1>
        <p class="subtitle">Service diagnostics for the local field-test runtime</p>
      </div>
      <form id="tokenForm" class="token-row"${authRequired ? "" : " hidden"}>
        <input id="tokenInput" name="token" type="password" autocomplete="current-password" placeholder="Admin bearer token">
        <button type="submit">Refresh</button>
        <button id="clearToken" class="secondary" type="button">Clear</button>
      </form>
    </div>
  </header>
  <main class="shell">
    <div class="status-line" id="statusLine"></div>
    <section class="grid">
      <article class="panel wide">
        <h2>Overview</h2>
        <div class="panel-body">
          <div class="metrics" id="overviewMetrics"></div>
        </div>
      </article>
      <article class="panel">
        <h2>Persistence</h2>
        <div class="panel-body" id="persistencePanel"></div>
      </article>
      <article class="panel">
        <h2>Queues And Health</h2>
        <div class="panel-body" id="healthPanel"></div>
      </article>
      <article class="panel">
        <h2>Channels</h2>
        <div class="panel-body" id="channelsPanel"></div>
      </article>
      <article class="panel">
        <h2>MCP</h2>
        <div class="panel-body" id="mcpPanel"></div>
      </article>
      <article class="panel wide">
        <h2>Recent Events</h2>
        <div class="panel-body" id="eventsPanel"></div>
      </article>
      <article class="panel wide">
        <h2>Raw Diagnostics</h2>
        <div class="panel-body"><pre id="rawPanel">Waiting for diagnostics...</pre></div>
      </article>
    </section>
  </main>
  <script>
    (function () {
      var authRequired = ${authRequired ? "true" : "false"};
      var tokenInput = document.getElementById("tokenInput");
      var statusLine = document.getElementById("statusLine");
      var savedToken = authRequired ? (localStorage.getItem("rustyCrewAdminToken") || "") : "";
      tokenInput.value = savedToken;

      document.getElementById("tokenForm").addEventListener("submit", function (event) {
        event.preventDefault();
        var token = tokenInput.value.trim();
        if (token) localStorage.setItem("rustyCrewAdminToken", token);
        refresh();
      });

      document.getElementById("clearToken").addEventListener("click", function () {
        localStorage.removeItem("rustyCrewAdminToken");
        tokenInput.value = "";
        refresh();
      });

      function headers() {
        var token = tokenInput.value.trim();
        return authRequired && token ? { authorization: "Bearer " + token } : {};
      }

      async function api(path, auth) {
        var response = await fetch(path, { headers: auth ? headers() : {} });
        var body = await response.json();
        if (!response.ok || !body.ok) {
          var message = body.error ? body.error.message : response.statusText;
          throw new Error(message || ("request failed: " + response.status));
        }
        return body.data;
      }

      function pill(text, kind) {
        var span = document.createElement("span");
        span.className = "pill " + (kind || "");
        span.textContent = text;
        return span;
      }

      function setStatus(items) {
        statusLine.replaceChildren.apply(statusLine, items);
      }

      function metric(label, value) {
        var node = document.createElement("div");
        node.className = "metric";
        var labelNode = document.createElement("span");
        labelNode.textContent = label;
        var valueNode = document.createElement("strong");
        valueNode.textContent = value === undefined || value === null ? "n/a" : String(value);
        node.append(labelNode, valueNode);
        return node;
      }

      function renderMetrics(id, entries) {
        var target = document.getElementById(id);
        target.replaceChildren.apply(target, entries.map(function (entry) {
          return metric(entry[0], entry[1]);
        }));
      }

      function renderObjectTable(id, data) {
        var target = document.getElementById(id);
        if (!data) {
          target.innerHTML = '<div class="empty">No data reported.</div>';
          return;
        }
        var table = document.createElement("table");
        Object.keys(data).sort().forEach(function (key) {
          var row = document.createElement("tr");
          var name = document.createElement("th");
          var value = document.createElement("td");
          name.textContent = key;
          value.textContent = typeof data[key] === "object" ? JSON.stringify(data[key]) : String(data[key]);
          row.append(name, value);
          table.append(row);
        });
        target.replaceChildren(table);
      }

      function renderItemsTable(id, items, columns) {
        var target = document.getElementById(id);
        if (!items || items.length === 0) {
          target.innerHTML = '<div class="empty">No records reported.</div>';
          return;
        }
        var table = document.createElement("table");
        var head = document.createElement("tr");
        columns.forEach(function (column) {
          var th = document.createElement("th");
          th.textContent = column.label;
          head.append(th);
        });
        table.append(head);
        items.forEach(function (item) {
          var row = document.createElement("tr");
          columns.forEach(function (column) {
            var td = document.createElement("td");
            var value = column.value(item);
            td.textContent = value === undefined || value === null || value === "" ? "n/a" : String(value);
            row.append(td);
          });
          table.append(row);
        });
        target.replaceChildren(table);
      }

      function setPanelError(id, error) {
        document.getElementById(id).innerHTML = '<div class="error">' + escapeHtml(error.message || String(error)) + '</div>';
      }

      function escapeHtml(value) {
        return String(value).replace(/[&<>"']/g, function (char) {
          return { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[char];
        });
      }

      async function refresh() {
        var token = tokenInput.value.trim();
        setStatus([pill("Loading", "warn")]);
        try {
          var health = await api("/v1/admin/healthz", false);
          var statusPills = [
            pill("Liveness: " + health.status, health.status === "live" ? "good" : "bad")
          ];
          if (authRequired && !token) {
            setStatus(statusPills.concat([pill("Enter token for diagnostics", "warn")]));
            return;
          }

          var results = await Promise.allSettled([
            api("/v1/admin/readyz", true),
            api("/v1/admin/diagnostics", true),
            api("/v1/admin/diagnostics/persistence", true),
            api("/v1/admin/diagnostics/channels", true),
            api("/v1/admin/diagnostics/mcp", true),
            api("/v1/admin/events/recent", true)
          ]);

          var ready = unwrap(results[0]);
          var diagnostics = unwrap(results[1]);
          var persistence = unwrap(results[2]);
          var channels = unwrap(results[3]);
          var mcp = unwrap(results[4]);
          var events = unwrap(results[5]);
          var overview = diagnostics.overview || {};
          var summary = overview.summary || {};

          statusPills.push(pill("Readiness: " + ready.status, ready.status === "ready" ? "good" : "warn"));
          statusPills.push(pill("Generated: " + (overview.generatedAt || "n/a")));
          if (overview.degraded) statusPills.push(pill("Degraded", "warn"));
          setStatus(statusPills);

          renderMetrics("overviewMetrics", [
            ["Sessions", summary.sessions],
            ["Active", summary.activeSessions],
            ["Idle", summary.idleSessions],
            ["Queued", summary.queueDepth],
            ["Agents", summary.agents],
            ["Tools", summary.tools],
            ["Recent errors", summary.recentErrors]
          ]);

          renderObjectTable("persistencePanel", Object.assign({}, persistence, {
            tableCounts: JSON.stringify((persistence && persistence.tableCounts) || {})
          }));

          renderObjectTable("healthPanel", {
            runtimeHealth: overview.health,
            degraded: overview.degraded,
            reasonCodes: (overview.reasonCodes || []).join(", ") || "none",
            queues: overview.queues ? JSON.stringify(overview.queues) : "none"
          });

          renderItemsTable("channelsPanel", channels.items || [], [
            { label: "Binding", value: function (item) { return item.bindingId; } },
            { label: "Agent", value: function (item) { return item.agentId; } },
            { label: "Status", value: function (item) { return item.status; } },
            { label: "Channel", value: function (item) { return item.externalChannelId; } }
          ]);

          renderItemsTable("mcpPanel", mcp.items || [], [
            { label: "Binding", value: function (item) { return item.bindingId; } },
            { label: "Agent", value: function (item) { return item.agentId; } },
            { label: "Status", value: function (item) { return item.status; } },
            { label: "Servers", value: function (item) { return (item.serverNames || []).join(", "); } }
          ]);

          renderItemsTable("eventsPanel", events.items || [], [
            { label: "Time", value: function (item) { return item.createdAt; } },
            { label: "Source", value: function (item) { return item.source; } },
            { label: "Type", value: function (item) { return item.eventType; } },
            { label: "Summary", value: function (item) { return item.summary; } }
          ]);

          document.getElementById("rawPanel").textContent = JSON.stringify(diagnostics, null, 2);
        } catch (error) {
          setStatus([pill("Diagnostics error", "bad"), pill(error.message || String(error), "bad")]);
          setPanelError("healthPanel", error);
        }
      }

      function unwrap(result) {
        if (result.status === "fulfilled") return result.value;
        throw result.reason;
      }

      refresh();
      setInterval(refresh, 15000);
    }());
  </script>
</body>
</html>`;
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

function controlHeaders(
  request: IncomingMessage,
  state: ServiceState,
): Record<string, string | undefined> {
  const result = headers(request);
  if (!configRequiresAuth(state.config)) {
    result.authorization = `Bearer ${DEV_NO_AUTH_CONTROL_TOKEN}`;
  }
  return result;
}

function controlBearerToken(state: ServiceState): string {
  return configRequiresAuth(state.config)
    ? (state.config.admin.token ?? "")
    : DEV_NO_AUTH_CONTROL_TOKEN;
}

function configRequiresAuth(config: RustyCrewServiceConfig): boolean {
  return config.admin.authMode !== "none";
}

function isAuthorized(
  request: IncomingMessage,
  token: string | undefined,
  state?: ServiceState,
): boolean {
  if (state && !configRequiresAuth(state.config)) return true;
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
