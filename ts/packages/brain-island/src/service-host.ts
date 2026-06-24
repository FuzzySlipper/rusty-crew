import {
  createServer,
  type IncomingMessage,
  type Server,
  type ServerResponse,
} from "node:http";
import { createReadStream, existsSync } from "node:fs";
import { mkdir, readFile, rename, stat, writeFile } from "node:fs/promises";
import {
  basename,
  dirname,
  extname,
  join,
  relative,
  resolve,
  sep,
} from "node:path";
import { fileURLToPath } from "node:url";
import type {
  BrainEvent,
  BrainImplementationId,
  BrainModelConfig,
  AgentId,
  BrainImplementationHandle,
  ChannelBindingRecord,
  ChannelMembershipStatus,
  ChannelSubscriptionStatus,
  CoreEvent,
  EngineHandle,
  McpBindingRecord,
  ProfileId,
  ScheduledJobStatus,
  ScheduledRunSummary,
  ScheduledRunStatus,
  ScheduledRunTrigger,
  SessionId,
  SessionKind,
  SessionState,
  SubscriptionHandle,
} from "@rusty-crew/contracts";
import {
  loadNativeBridge,
  type NativeProfileMemoryRecord,
  type NativeBridgeModule,
} from "@rusty-crew/native-bridge";
import {
  McpSurfaceManager,
  createSimulatedMcpTransportFactory,
} from "@rusty-crew/adapter-mcp";
import {
  createDenSuccessorGatewayClient,
  dispatchChannelMessageProjection,
  ingestChannelInboundMessage,
  projectAgentMessageToChannel,
  type DenSuccessorAgentIdentity,
  type DenSuccessorConversationChannel,
  type DenSuccessorConversationMembership,
  type DenSuccessorDeliveryIntent,
  type DenSuccessorGatewayClient,
  type ChannelBindingDiagnostics,
} from "@rusty-crew/adapter-den";
import {
  createTelegramAdapterRegistration,
  createTelegramBotApiHttpClient,
  FileTelegramUpdateOffsetStore,
  TelegramChannelConnector,
} from "@rusty-crew/adapter-telegram";
import {
  AgentActivityObservationProducer,
  type AgentActivityObservationEvent,
  type AgentActivityObservationSink,
  type AgentActivityWorkRef,
} from "./agent-activity-observation.js";
import {
  deliveryIntentWakeDecision,
  normalizeChannelWakePolicy,
  type ChannelWakePolicy,
  type DeliveryIntentWakeDecision,
} from "./channel-wake-policy.js";
import { resolveBrainModuleSelection } from "./brain-module.js";
import {
  createMemoryAdminControlAuditSink,
  type AdminControlCommand,
  type AdminControlExecutor,
  type AdminControlResponse,
  handleAdminControlRequest,
} from "./admin-control-api.js";
import { createNewSessionLifecycleExecutor } from "./new-session-lifecycle.js";
import { createReloadMcpControlExecutor } from "./reload-mcp-control.js";
import { createDefaultMcpDiscoveryClient } from "./service-mcp-tools.js";
import {
  handleAdminDiagnosticsRequest,
  type AdminDiagnosticsContext,
  type AdminRouteResult,
} from "./admin-diagnostics-api.js";
import {
  buildAdapterDiagnosticsProjection,
  type ChannelAdapterBindingDiagnostics,
  type ChannelProjectionFailureRecord,
  type AdapterDiagnosticsProjection,
} from "./adapter-diagnostics.js";
import { buildBackgroundServiceDiagnosticsProjection } from "./background-service-diagnostics.js";
import {
  runBackgroundMemorySkillReview,
  type BackgroundReviewPayload,
  type BackgroundReviewResult,
} from "./background-memory-skill-review.js";
import {
  inspectDirectDebugSession,
  requestDirectDebugTurn,
  type DirectDebugResult,
  type DirectDebugServiceContext,
} from "./direct-debug-service.js";
import { loadProfileConfig, loadProfileContext } from "./profile-loading.js";
import { buildProfileRoleAssembly } from "./profile-role-assembly.js";
import {
  buildRuntimeDiagnosticsProjection,
  type RuntimeSessionEffectiveDefaults,
} from "./runtime-diagnostics.js";
import {
  handleRustyViewChatRequest,
  cursorSequence,
  type ChatEvent,
  type ChatSendMessageInput,
  type ExecuteChatCommandInput,
  type ExecuteChatCommandResult,
  type SendChatMessageResult,
} from "./rusty-view-chat-api.js";
import { buildReadOnlySlashCommandResponse } from "./slash-command-responses.js";
import {
  routeSlashCommand,
  type SlashCommandSession,
} from "./slash-command-router.js";
import type { RuntimeHealthProjection } from "./runtime-health.js";
import {
  announceConfiguredSessionsToDenGateway,
  denGatewayStartupSummary,
  heartbeatConfiguredSessionsToDenRuntime,
  type DenSuccessorGatewayStartupReport,
} from "./den-successor-service.js";
import {
  createCuratorAdminControlExecutor,
  type CuratorAdminStatus,
} from "./curator-admin-control.js";
import {
  discoverCuratorCandidates,
  type CuratorCandidateBatch,
} from "./curator-candidates.js";
import { postTurnMaintenanceDecision } from "./post-turn-maintenance.js";
import {
  runCuratorLifecycleTransitions,
  type CuratorLifecycleReport,
} from "./curator-lifecycle.js";
import {
  listCuratorArchivedSkills,
  listCuratorPinnedSkills,
} from "./curator-skill-admin.js";
import {
  createCuratorGovernanceExecutor,
  FileCuratorGovernanceStore,
  MemoryCuratorGovernanceStore,
  rollbackCuratorMutation,
  type CuratorMutationCandidate,
} from "./curator-mutations.js";
import type {
  CuratorExecuteContext,
  CuratorExecuteRequest,
} from "./planning-tools.js";
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
  effectiveSessionDefaults,
  effectiveWakeTimeoutMs,
  loadRustyCrewRuntimeConfig,
  registerConfiguredScheduledJobs,
  ensureConfiguredSessionForChannelBinding,
  type RustyCrewRuntimeConfig,
  type RustyCrewRuntimeConfigApplyResult,
} from "./service-runtime-config.js";
import { createRuntimeActivityObserver } from "./runtime-activity-observer.js";
import {
  executeScheduledHostRun,
  runScheduledHostExecutors,
  scheduledHostJobKinds,
} from "./scheduled-host-executors.js";
import { buildToolRegistryDiagnostics } from "./tool-registry-diagnostics.js";
import { buildToolContextDiagnosticsReport } from "./tool-context-diagnostics.js";
import {
  effectiveTurnTimeoutMs,
  WakeDispatchTimeoutError,
  withWakeTimeout,
} from "./wake-timeout.js";

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
  telegramConnector?: TelegramChannelConnector;
  telegramOutboundSubscription?: SubscriptionHandle;
  readonly curator: ServiceCuratorRuntime;
  readonly backgroundReview: ServiceBackgroundReviewRuntime;
  readonly denConversationChannelResolutionsByBindingId: Map<
    string,
    DenConversationChannelResolution
  >;
  readonly denConversationChannelIdsByExternalId: Map<string, number>;
  readonly denConversationMembershipsByBindingId: Map<
    string,
    DenSuccessorConversationMembership
  >;
  readonly dynamicDenChannelBindings: Map<
    string,
    ChannelAdapterBindingDiagnostics
  >;
  readonly channelProjectionFailures: ChannelProjectionFailureRecord[];
  profileChannelWakePolicies: Map<string, ChannelWakePolicy>;
  mcpManager: McpSurfaceManager;
  readonly wakeSubscription: SubscriptionHandle;
  readonly timers: Set<NodeJS.Timeout>;
  readonly inFlightWakes: Set<SessionId>;
  readonly claimedDeliveryIntentIds: Set<number>;
  readonly unmatchedDeliveryIntentIds: Set<number>;
  readonly directDispatchSessions: Set<SessionId>;
  readonly chatMessageReceipts: Map<string, SendChatMessageResult>;
  readonly chatEventsBySession: Map<SessionId, ChatEvent[]>;
  readonly chatSequencesBySession: Map<SessionId, number>;
  readonly chatSubscribersBySession: Map<SessionId, Set<ChatStreamSubscriber>>;
  readonly suppressedWakeEvents: Map<SessionId, number>;
  readonly recentEvents: ServiceRecentEvent[];
  readonly now: () => string;
  nextWakeSequence: number;
  stopping: boolean;
}

interface DenConversationChannelResolution {
  channelId: number;
  projectId: string;
  slug: string;
}

interface ServiceBackgroundReviewRuntime {
  enabled: boolean;
  recentFindings: number;
  lastRunAt?: string;
  lastError?: string;
}

interface ServiceCuratorRuntime {
  readonly store: MemoryCuratorGovernanceStore;
  executor: NonNullable<CuratorExecuteContext["executor"]>;
  runtimeConfig: RustyCrewRuntimeConfig;
  lastRunAt?: string;
  lastError?: string;
  lastLifecycleRunAt?: string;
  lastLifecycleReport?: CuratorLifecycleReport;
}

interface ServiceRecentEvent {
  id: string;
  createdAt: string;
  source: string;
  eventType: string;
  summary: string;
  severity?: string;
}

interface RawServiceRouteResult {
  kind: "raw";
  write(response: ServerResponse): void;
}

type ServiceRouteResult =
  | AdminRouteResult
  | RawServiceRouteResult
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
    const profileChannelWakePolicies =
      await loadProfileChannelWakePolicies(runtimeConfig);
    const mcpManager = await createServiceMcpManager(runtimeConfig);
    const curator = createServiceCuratorRuntime({
      config,
      runtimeConfig,
      bridge,
      now: options.now ?? (() => new Date().toISOString()),
    });
    const runtimeConfigApplyResult = await applyRustyCrewRuntimeConfig({
      serviceConfig: config,
      runtimeConfig,
      bridge,
      curatorExecutor: curator.executor,
      mcpSurfaceDiagnostics: mcpManager.diagnostics(),
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
      denConversationChannelResolutionsByBindingId: new Map(),
      denConversationChannelIdsByExternalId: new Map(),
      denConversationMembershipsByBindingId: new Map(),
      dynamicDenChannelBindings: new Map(),
      channelProjectionFailures: [],
      profileChannelWakePolicies,
      curator,
      backgroundReview: createServiceBackgroundReviewRuntime(runtimeConfig),
      mcpManager,
      wakeSubscription,
      timers: new Set(),
      inFlightWakes: new Set(),
      claimedDeliveryIntentIds: new Set(),
      unmatchedDeliveryIntentIds: new Set(),
      directDispatchSessions: new Set(),
      chatMessageReceipts: new Map(),
      chatEventsBySession: new Map(),
      chatSequencesBySession: new Map(),
      chatSubscribersBySession: new Map(),
      suppressedWakeEvents: new Map(),
      recentEvents: [],
      now: options.now ?? (() => new Date().toISOString()),
      nextWakeSequence: 0,
      stopping: false,
    };
    state.denGatewayStartupReport = await connectDenSuccessorGateway(state);
    await ensureDenConversationChannels(state);
    await startTelegramConnector(state);
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
  if (isAdminPanelRoute(url.pathname, staticServingEnabled(state))) {
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

  if (
    isChatRoute(url.pathname) &&
    (request.method ?? "GET").toUpperCase() === "OPTIONS"
  ) {
    return chatCorsPreflightResponse(request);
  }

  if (!url.pathname.startsWith("/v1/") && staticServingEnabled(state)) {
    return handleStaticSiteRequest(request, url, state);
  }

  if (!isAuthorized(request, state.config.admin.token, state)) {
    const unauthorized = failure(401, requestId(request), {
      code: "unauthorized",
      reason_code: "missing_or_invalid_bearer_token",
      message: "admin HTTP requires a valid bearer token",
      retryable: false,
    });
    return isChatRoute(url.pathname)
      ? withChatCors(unauthorized, request)
      : unauthorized;
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

  if (isChatRoute(url.pathname)) {
    const streamResult = await handleRustyViewChatStreamRequest(
      request,
      url,
      state,
    );
    if (streamResult !== undefined) return withChatCors(streamResult, request);
    const body =
      (request.method ?? "GET").toUpperCase() === "POST"
        ? await readJsonBody(request)
        : undefined;
    const result = await handleRustyViewChatRequest(
      {
        method: request.method ?? "GET",
        url: url.toString(),
        headers: headers(request),
        body,
        requestId: requestId(request),
      },
      {
        listSessions: () => state.bridge.listSessions(),
        projectBodyStateJson: (sessionId) =>
          state.bridge.projectBodyStateJson(sessionId),
        listChatEvents: (session, cursor, limit) =>
          listChatEventsAfterCursor(state, session, cursor, limit),
        executeCommand: (input) => executeRustyViewChatCommand(state, input),
        sendMessage: (input) => submitRustyViewChatMessage(state, input),
        now: state.now,
      },
    );
    return withChatCors(result, request);
  }

  if (url.pathname.startsWith("/v1/debug/")) {
    return handleDirectDebugRequest(request, url, state);
  }

  if (url.pathname.startsWith("/v1/admin/scheduler/")) {
    return handleSchedulerReadRequest(request, url, state);
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

async function handleSchedulerReadRequest(
  request: IncomingMessage,
  url: URL,
  state: ServiceState,
): Promise<AdminRouteResult> {
  const requestIdValue = requestId(request);
  if ((request.method ?? "GET").toUpperCase() !== "GET") {
    return failure(405, requestIdValue, {
      code: "method_not_allowed",
      reason_code: "read_only_route",
      message: "scheduler diagnostics routes only support GET",
      retryable: false,
    });
  }

  if (url.pathname === "/v1/admin/scheduler/jobs") {
    const status = scheduledJobStatusParam(url.searchParams.get("status"));
    if (status === "invalid") {
      return invalidSchedulerFilter(requestIdValue, "status");
    }
    const jobKind = stringParam(url, "jobKind");
    const jobs = await state.bridge.listScheduledJobs({
      ...(status === undefined ? {} : { status }),
      ...(jobKind === undefined ? {} : { jobKind }),
      ...pageParams(url),
    });
    return successRoute(requestIdValue, { jobs });
  }

  if (url.pathname === "/v1/admin/scheduler/runs") {
    const status = scheduledRunStatusParam(url.searchParams.get("status"));
    if (status === "invalid") {
      return invalidSchedulerFilter(requestIdValue, "status");
    }
    const trigger = scheduledRunTriggerParam(url.searchParams.get("trigger"));
    if (trigger === "invalid") {
      return invalidSchedulerFilter(requestIdValue, "trigger");
    }
    const jobId = stringParam(url, "jobId");
    const targetSessionId = stringParam(url, "targetSessionId");
    const runs = await state.bridge.listScheduledRuns({
      ...(jobId === undefined ? {} : { jobId }),
      ...(status === undefined ? {} : { status }),
      ...(trigger === undefined ? {} : { trigger }),
      ...(targetSessionId === undefined
        ? {}
        : { targetSessionId: targetSessionId as never }),
      ...pageParams(url),
    });
    return successRoute(requestIdValue, { runs });
  }

  return failure(404, requestIdValue, {
    code: "not_found",
    reason_code: "unknown_scheduler_diagnostics_route",
    message: `unknown scheduler diagnostics route ${url.pathname}`,
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

async function handleRustyViewChatStreamRequest(
  request: IncomingMessage,
  url: URL,
  state: ServiceState,
): Promise<ServiceRouteResult | undefined> {
  const parts = url.pathname.split("/").filter(Boolean);
  if (
    parts.length !== 5 ||
    parts[0] !== "v1" ||
    parts[1] !== "chat" ||
    parts[2] !== "sessions" ||
    parts[4] !== "stream"
  ) {
    return undefined;
  }

  const requestIdValue = requestId(request);
  if ((request.method ?? "GET").toUpperCase() !== "GET") {
    return failure(405, requestIdValue, {
      code: "method_not_allowed",
      reason_code: "chat_stream_requires_get",
      message: "Rusty View chat stream routes only support GET",
      retryable: false,
    });
  }

  const sessionId = decodeURIComponent(parts[3] ?? "") as SessionId;
  const sessions = await state.bridge.listSessions();
  const session = sessions.find(
    (candidate) => candidate.sessionId === sessionId,
  );
  if (!session) {
    return failure(404, requestIdValue, {
      code: "not_found",
      reason_code: "chat_session_not_found",
      message: `chat session ${sessionId} was not found`,
      retryable: false,
    });
  }

  const cursor =
    stringHeader(request, "last-event-id") ?? stringParam(url, "cursor");
  const replay = streamReplayEvents(state, session, cursor, url);
  const closeAfterReplay =
    url.searchParams.get("once") === "true" ||
    url.searchParams.get("close_after_replay") === "true";
  return {
    kind: "raw",
    write(response) {
      writeRustyViewChatSseStream({
        state,
        session,
        replay,
        closeAfterReplay,
        request,
        response,
      });
    },
  };
}

function writeRustyViewChatSseStream(input: {
  state: ServiceState;
  session: SessionState;
  replay: readonly ChatEvent[];
  closeAfterReplay: boolean;
  request: IncomingMessage;
  response: ServerResponse;
}): void {
  const { state, session, response } = input;
  response.writeHead(200, {
    "content-type": "text/event-stream; charset=utf-8",
    "cache-control": "no-cache, no-transform",
    connection: "keep-alive",
    "x-accel-buffering": "no",
    ...chatCorsHeaders(input.request),
  });
  for (const event of input.replay) {
    writeSseEvent(response, event);
  }
  if (input.closeAfterReplay) {
    response.end();
    return;
  }

  const subscriber: ChatStreamSubscriber = {
    write(event) {
      writeSseEvent(response, event);
    },
  };
  const subscribers = chatSubscribers(state, session.sessionId);
  subscribers.add(subscriber);
  const heartbeat = setInterval(() => {
    if (!response.destroyed) response.write(": keep-alive\n\n");
  }, 15_000);
  state.timers.add(heartbeat);

  const cleanup = () => {
    clearInterval(heartbeat);
    state.timers.delete(heartbeat);
    subscribers.delete(subscriber);
    if (subscribers.size === 0) {
      state.chatSubscribersBySession.delete(session.sessionId);
    }
  };
  response.on("close", cleanup);
  response.on("error", cleanup);
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
  const sessionDefaults = await effectiveSessionDefaultsById(state, sessions);
  const diagnostics = buildRuntimeDiagnosticsProjection({
    now,
    runtimeSummary,
    sessions,
    sessionDefaults,
    delegatedSessions: [],
    brainModules: brainModuleDiagnostics(state),
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
    background: await buildServiceBackgroundDiagnostics(state, now),
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

function brainModuleDiagnostics(
  state: ServiceState,
): NonNullable<
  Parameters<typeof buildRuntimeDiagnosticsProjection>[0]["brainModules"]
> {
  return state.runtimeConfig.brains.map((brain) => {
    const diagnostics =
      state.runtimeConfigApplyResult.brainDiagnosticsByProfileId[
        brain.profileId
      ];
    if (diagnostics) return diagnostics;
    const selection =
      state.runtimeConfigApplyResult.brainModulesByProfileId[brain.profileId];
    return {
      profileId: brain.profileId,
      implementationId: brain.implementationId,
      moduleId: selection?.moduleId ?? "unknown",
      ...(selection?.strategy === undefined
        ? {}
        : { strategy: selection.strategy }),
      selectedToolCount: 0,
      selectedToolSource: "unknown",
      toolAdapterStatus: "unknown",
    };
  });
}

async function effectiveSessionDefaultsById(
  state: ServiceState,
  sessions: readonly SessionState[],
): Promise<Map<SessionId, RuntimeSessionEffectiveDefaults>> {
  const entries = await Promise.all(
    sessions.map(async (session) => {
      const configured = configuredSessionForRuntimeSession(
        state.runtimeConfig,
        session,
      );
      try {
        const profile = await loadProfileConfig(
          state.runtimeConfig.profilesDir,
          session.profileId,
        );
        return [
          session.sessionId,
          {
            ...effectiveSessionDefaults(configured ?? {}, profile),
            wakeTimeoutMs: effectiveWakeTimeoutMs({
              session: configured,
              profile,
            }),
          },
        ] as const;
      } catch {
        return [
          session.sessionId,
          effectiveSessionDefaults(configured ?? {}, {}),
        ] as const;
      }
    }),
  );
  return new Map(entries);
}

function configuredSessionForRuntimeSession(
  runtimeConfig: RustyCrewRuntimeConfig,
  session: Pick<SessionState, "sessionId" | "profileId">,
): RustyCrewRuntimeConfig["sessions"][number] | undefined {
  return runtimeConfig.sessions.find(
    (configured) =>
      configured.sessionId === session.sessionId &&
      configured.profileId === session.profileId,
  );
}

async function buildServiceBackgroundDiagnostics(
  state: ServiceState,
  now: string,
): Promise<ReturnType<typeof buildBackgroundServiceDiagnosticsProjection>> {
  const [jobs, runs] = await Promise.all([
    state.bridge.listScheduledJobs({ limit: 100 }).catch(() => []),
    state.bridge.listScheduledRuns({ limit: 100 }).catch(() => []),
  ]);
  const activeJobs = jobs.filter((job) => job.status === "active");
  const pausedJobs = jobs.filter((job) => job.status === "paused");
  const failedRuns = runs.filter((run) => run.status === "failed");
  const runningRuns = runs.filter((run) => run.status === "claimed");
  const lastRun = latestCompletedOrFailedRun(runs);
  const reviewJobs = jobs.filter(
    (job) => job.jobKind === "runtime.review.memory_skills",
  );
  return buildBackgroundServiceDiagnosticsProjection({
    now,
    scheduler: {
      jobCount: jobs.length,
      activeJobs: activeJobs.length,
      pausedJobs: pausedJobs.length,
      staleRuns: 0,
      runningRuns: runningRuns.length,
      failedRuns: failedRuns.length,
      nextDueAt: earliestDueAt(activeJobs),
      lastRunAt: lastRun?.completedAt,
      lastError: failedRuns[0]?.error,
    },
    curator: {
      status: "available",
      candidateCount: state.curator.store.candidates.size,
      lastRunAt: state.curator.lastRunAt,
      lastError: state.curator.lastError,
    },
    backgroundReview: {
      enabled: state.backgroundReview.enabled || reviewJobs.length > 0,
      recentFindings: state.backgroundReview.recentFindings,
      lastRunAt: state.backgroundReview.lastRunAt,
      lastError: state.backgroundReview.lastError,
    },
    cleanup: {},
  });
}

function runtimeConfigApplySummary(
  prefix: string,
  result: RustyCrewRuntimeConfigApplyResult,
): string {
  return `${prefix}: ${result.brainsRegistered} brains registered, ${result.brainsAlreadyPresent} brains already present, ${result.sessionsCreated} sessions created, ${result.sessionsAlreadyPresent} sessions already present, ${result.sessionsReactivated} sessions reactivated, ${result.sessionsMissing} configured sessions missing, ${result.scheduledJobsRegistered} scheduled jobs registered.`;
}

function buildServiceAdapterDiagnostics(
  state: ServiceState,
  now: string,
): AdapterDiagnosticsProjection | undefined {
  if (
    state.runtimeConfig.channelBindings.length === 0 &&
    state.dynamicDenChannelBindings.size === 0 &&
    state.runtimeConfig.mcpBindings.length === 0
  ) {
    return undefined;
  }
  return buildAdapterDiagnosticsProjection({
    now,
    channelBindings: state.runtimeConfig.channelBindings,
    dynamicChannelBindings: [...state.dynamicDenChannelBindings.values()],
    channelActivity: [
      ...telegramChannelActivityDiagnostics(state, now),
      ...denConversationChannelActivityDiagnostics(state),
    ],
    channelProjectionFailures: state.channelProjectionFailures,
    channelWakePolicies: channelWakePoliciesByBinding(state),
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

async function loadProfileChannelWakePolicies(
  runtimeConfig: RustyCrewRuntimeConfig,
): Promise<Map<string, ChannelWakePolicy>> {
  const policies = new Map<string, ChannelWakePolicy>();
  const profileIds = [
    ...new Set(runtimeConfig.sessions.map((session) => session.profileId)),
  ];
  for (const profileId of profileIds) {
    const profile = await loadProfileConfig(
      runtimeConfig.profilesDir,
      profileId,
    );
    policies.set(
      profileId,
      normalizeChannelWakePolicy(profile.channelDefaults?.wakePolicy),
    );
  }
  return policies;
}

function channelWakePolicyForSession(
  state: ServiceState,
  session: RustyCrewRuntimeConfig["sessions"][number],
): ChannelWakePolicy {
  return (
    state.profileChannelWakePolicies.get(session.profileId) ?? "subscription"
  );
}

function channelWakePoliciesByBinding(
  state: ServiceState,
): Record<string, ChannelWakePolicy> {
  const policies: Record<string, ChannelWakePolicy> = {};
  for (const binding of state.runtimeConfig.channelBindings) {
    policies[binding.bindingId] =
      state.profileChannelWakePolicies.get(binding.profileId) ?? "subscription";
  }
  return policies;
}

function createServiceCuratorRuntime(input: {
  config: RustyCrewServiceConfig;
  runtimeConfig: RustyCrewRuntimeConfig;
  bridge: NativeBridgeModule;
  now: () => string;
}): ServiceCuratorRuntime {
  const store = new FileCuratorGovernanceStore(
    join(input.config.paths.dataDir, "data", "curator-governance.json"),
  );
  const runtime: ServiceCuratorRuntime = {
    store,
    runtimeConfig: input.runtimeConfig,
    executor: async () => {
      throw new Error("curator executor not initialized");
    },
  };
  runtime.executor = createCuratorGovernanceExecutor({
    skillsDir: curatorSkillsDir(input.runtimeConfig),
    store,
    snapshotDir: join(input.config.paths.backupDir, "curator-snapshots"),
    now: () => new Date(input.now()),
    scan: async (request) => {
      try {
        const batch = await scanServiceCuratorCandidates(
          {
            ...input,
            runtimeConfig: runtime.runtimeConfig,
            store,
          },
          request,
        );
        runtime.lastRunAt = input.now();
        runtime.lastError = undefined;
        return batch;
      } catch (error) {
        runtime.lastError = errorMessage(error, "curator scan failed");
        throw error;
      }
    },
  });
  return runtime;
}

function createServiceBackgroundReviewRuntime(
  runtimeConfig: RustyCrewRuntimeConfig,
): ServiceBackgroundReviewRuntime {
  return {
    enabled: runtimeConfig.scheduledJobs.some(
      (job) => job.jobKind === "runtime.review.memory_skills",
    ),
    recentFindings: 0,
  };
}

function earliestDueAt(
  jobs: readonly { nextDueAt?: string }[],
): string | undefined {
  return jobs
    .flatMap((job) => (job.nextDueAt ? [job.nextDueAt] : []))
    .sort()[0];
}

function latestCompletedOrFailedRun(
  runs: readonly ScheduledRunSummary[],
): ScheduledRunSummary | undefined {
  return [...runs]
    .filter((run) => run.completedAt)
    .sort((left, right) =>
      (right.completedAt ?? "").localeCompare(left.completedAt ?? ""),
    )[0];
}

async function scanServiceCuratorCandidates(
  input: {
    runtimeConfig: RustyCrewRuntimeConfig;
    bridge: NativeBridgeModule;
    store: MemoryCuratorGovernanceStore;
    now: () => string;
  },
  request: CuratorExecuteRequest,
): Promise<CuratorCandidateBatch> {
  const profileId = curatorProfileId(input.runtimeConfig, request);
  const profile = await loadProfileContext({
    profilesDir: input.runtimeConfig.profilesDir,
    skillsDir: input.runtimeConfig.skillsDir,
    profileId,
  });
  const denseProfileMemory = await input.bridge
    .listProfileMemory({ profileId })
    .catch(() => []);
  const batch = discoverCuratorCandidates({
    batchId: [
      "curator",
      request.scopeType ?? "profile",
      request.scopeId ?? profileId,
      input.now().replace(/[^0-9A-Za-z]/g, ""),
    ].join(":"),
    now: input.now(),
    scopeType: request.scopeType ?? "profile",
    scopeId: request.scopeId ?? profileId,
    profileId,
    skills: profile.skills,
    expectedSkillSlugs:
      profile.profile.skillsMode === "all" ? [] : profile.profile.skills,
    denseProfileMemory: denseProfileMemory.map((record) => ({
      profileId: record.profileId,
      key: record.key,
      content: record.content,
      revision: record.revision,
      updatedAt: record.updatedAt,
      metadata: record.metadataJson,
    })),
    dryRun: request.dryRun,
  });
  input.store.upsertBatch(
    batch,
    batch.candidates.flatMap((candidate) =>
      mutationForServiceCuratorCandidate(candidate),
    ),
  );
  return batch;
}

function curatorProfileId(
  runtimeConfig: RustyCrewRuntimeConfig,
  request: CuratorExecuteRequest,
): ProfileId {
  if (request.profileId) return request.profileId as ProfileId;
  if (request.scopeType === "profile" && request.scopeId) {
    return request.scopeId as ProfileId;
  }
  if (request.scopeType === "session" && request.scopeId) {
    const session = runtimeConfig.sessions.find(
      (candidate) => candidate.sessionId === request.scopeId,
    );
    if (session) return session.profileId;
  }
  const profileId =
    runtimeConfig.brains[0]?.profileId ?? runtimeConfig.sessions[0]?.profileId;
  if (!profileId) {
    throw new Error("curator scan requires a configured profile");
  }
  return profileId;
}

function mutationForServiceCuratorCandidate(
  candidate: CuratorCandidateBatch["candidates"][number],
): CuratorMutationCandidate[] {
  const slug = skillSlugFromTarget(candidate.targetRef);
  if (!slug) return [];
  if (candidate.kind === "skill_create") {
    return [
      {
        ...candidate,
        mutation: {
          type: "skill_create",
          slug,
          content: skillCreateDraft(slug, candidate.summary),
        },
      },
    ];
  }
  if (candidate.kind === "skill_archive") {
    return [
      {
        ...candidate,
        mutation: {
          type: "skill_archive",
          slug,
          absorbedInto: "curator",
        },
      },
    ];
  }
  return [];
}

function skillSlugFromTarget(targetRef: string): string | undefined {
  return targetRef.startsWith("skill:")
    ? targetRef.slice("skill:".length)
    : undefined;
}

function skillCreateDraft(slug: string, summary: string): string {
  return [
    "---",
    `title: ${titleFromSlug(slug)}`,
    `summary: ${summary.replace(/\n/g, " ")}`,
    "tags:",
    "  - curated",
    "---",
    "",
    "Describe when to use this skill and the exact workflow it should guide.",
    "",
  ].join("\n");
}

function titleFromSlug(slug: string): string {
  return slug
    .split(/[-_]/g)
    .filter(Boolean)
    .map((part) => `${part[0]?.toUpperCase() ?? ""}${part.slice(1)}`)
    .join(" ");
}

function curatorSkillsDir(runtimeConfig: RustyCrewRuntimeConfig): string {
  return runtimeConfig.skillsDir ?? join(runtimeConfig.profilesDir, "skills");
}

async function curatorStatus(state: ServiceState): Promise<CuratorAdminStatus> {
  const skillsDir = curatorSkillsDir(state.curator.runtimeConfig);
  const [pinnedSkills, archivedSkills] = await Promise.all([
    listCuratorPinnedSkills(skillsDir),
    listCuratorArchivedSkills(skillsDir),
  ]);
  return {
    status: state.curator.lastError ? "degraded" : "available",
    candidateCount: state.curator.store.candidates.size,
    mutationCount: state.curator.store.mutations.size,
    pinnedSkillCount: pinnedSkills.length,
    archivedSkillCount: archivedSkills.length,
    lastRunAt: state.curator.lastRunAt,
    lastError: state.curator.lastError,
    lifecycle: state.curator.lastLifecycleReport,
  };
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
    state.denConversationChannelResolutionsByBindingId.clear();
    state.denConversationChannelIdsByExternalId.clear();
    state.denConversationMembershipsByBindingId.clear();
    return;
  }

  try {
    const channelsByProjectId = new Map<
      string,
      Map<string, DenSuccessorConversationChannel>
    >();
    const nextResolutions = new Map<string, DenConversationChannelResolution>();
    const nextChannelIds = new Map<string, number>();
    let created = 0;
    for (const binding of bindings) {
      const projectId = conversationProjectIdForBinding(state, binding);
      const slug = binding.externalChannelId;
      if (binding.conversationChannelId !== undefined) {
        nextResolutions.set(binding.bindingId, {
          channelId: binding.conversationChannelId,
          projectId,
          slug,
        });
        nextChannelIds.set(
          conversationExternalChannelKey(projectId, slug),
          binding.conversationChannelId,
        );
        continue;
      }
      let channelsBySlug = channelsByProjectId.get(projectId);
      if (channelsBySlug === undefined) {
        const channels = await state.denGatewayClient.listConversationChannels({
          projectId,
          limit: 100,
        });
        channelsBySlug = new Map(
          channels.map((channel) => [channel.slug, channel]),
        );
        channelsByProjectId.set(projectId, channelsBySlug);
      }
      const existing = channelsBySlug.get(slug);
      if (existing !== undefined) {
        nextResolutions.set(binding.bindingId, {
          channelId: existing.id,
          projectId,
          slug: existing.slug,
        });
        nextChannelIds.set(
          conversationExternalChannelKey(projectId, slug),
          existing.id,
        );
        continue;
      }
      const channel = await state.denGatewayClient.createConversationChannel({
        slug,
        display_name: displayNameForConversationBinding(binding),
        kind: "agent_channel",
        project_id: projectId,
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
      nextResolutions.set(binding.bindingId, {
        channelId: channel.id,
        projectId,
        slug: channel.slug,
      });
      nextChannelIds.set(
        conversationExternalChannelKey(projectId, slug),
        channel.id,
      );
    }
    state.denConversationChannelResolutionsByBindingId.clear();
    for (const [bindingId, resolution] of nextResolutions) {
      state.denConversationChannelResolutionsByBindingId.set(
        bindingId,
        resolution,
      );
    }
    state.denConversationChannelIdsByExternalId.clear();
    for (const [externalChannelKey, channelId] of nextChannelIds) {
      state.denConversationChannelIdsByExternalId.set(
        externalChannelKey,
        channelId,
      );
    }
    await refreshDenConversationMemberships(state, bindings, nextResolutions);
    recordServiceEvent(state, {
      source: "den-successor-gateway",
      eventType: "den_conversation_channels_resolved",
      summary: `Resolved ${nextResolutions.size} Den Conversation channel binding(s), created ${created}.`,
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

async function refreshDenConversationMemberships(
  state: ServiceState,
  bindings: readonly ChannelBindingRecord[],
  resolutionsByBindingId: ReadonlyMap<string, DenConversationChannelResolution>,
): Promise<void> {
  if (state.denGatewayClient === undefined) return;
  try {
    const projectIds = [
      ...new Set(
        bindings.map((binding) =>
          conversationProjectIdForBinding(state, binding),
        ),
      ),
    ];
    const memberships = (
      await Promise.all(
        projectIds.map((projectId) =>
          state.denGatewayClient!.listConversationMemberships({
            projectId,
            includeLeft: true,
            limit: Math.max(100, bindings.length * 2),
          }),
        ),
      )
    ).flat();
    const membershipByChannelAndMember = new Map<
      string,
      DenSuccessorConversationMembership
    >();
    for (const membership of memberships) {
      const key = conversationMembershipKey(
        membership.channel_id,
        membership.member_identity,
      );
      const existing = membershipByChannelAndMember.get(key);
      if (preferConversationMembership(membership, existing)) {
        membershipByChannelAndMember.set(key, membership);
      }
    }
    state.denConversationMembershipsByBindingId.clear();
    for (const binding of bindings) {
      const resolution = resolutionsByBindingId.get(binding.bindingId);
      if (resolution === undefined) continue;
      const membership = membershipByChannelAndMember.get(
        conversationMembershipKey(resolution.channelId, binding.agentId),
      );
      if (membership !== undefined) {
        state.denConversationMembershipsByBindingId.set(
          binding.bindingId,
          membership,
        );
      }
    }
  } catch (error) {
    state.denConversationMembershipsByBindingId.clear();
    recordServiceEvent(state, {
      source: "den-successor-gateway",
      eventType: "den_conversation_memberships_degraded",
      severity: "warning",
      summary: errorMessage(
        error,
        "Den Conversation membership resolution failed",
      ),
    });
  }
}

function conversationMembershipKey(
  channelId: number,
  memberIdentity: string,
): string {
  return `${channelId}:${memberIdentity}`;
}

function preferConversationMembership(
  candidate: DenSuccessorConversationMembership,
  existing: DenSuccessorConversationMembership | undefined,
): boolean {
  if (existing === undefined) return true;
  return (
    conversationMembershipRank(candidate.membership_status) >
    conversationMembershipRank(existing.membership_status)
  );
}

function conversationMembershipRank(status: string): number {
  switch (status) {
    case "active":
      return 3;
    case "invited":
      return 2;
    case "left":
      return 1;
    default:
      return 0;
  }
}

function conversationProjectIdForBinding(
  state: ServiceState,
  binding: ChannelBindingRecord,
): string {
  return (
    binding.conversationProjectId?.trim() ??
    state.config.denConversationProjectId
  );
}

function conversationExternalChannelKey(
  projectId: string,
  slug: string,
): string {
  return `${projectId}:${slug}`;
}

function displayNameForConversationBinding(
  binding: ChannelBindingRecord,
): string {
  return `${binding.agentId} (${binding.externalChannelId})`;
}

async function startTelegramConnector(state: ServiceState): Promise<void> {
  if (!state.config.telegram.enabled) return;
  const token = state.config.telegram.botToken;
  if (!token) return;
  const adapterId = state.config.telegram.adapterId as never;
  try {
    await state.bridge.registerPlatformAdapter(
      createTelegramAdapterRegistration(adapterId),
    );
  } catch (error) {
    recordServiceEvent(state, {
      source: "telegram",
      eventType: "telegram_adapter_registration_degraded",
      severity: "warning",
      summary: errorMessage(error, "Telegram adapter registration failed"),
    });
  }

  const connector = new TelegramChannelConnector({
    adapterId,
    bot: createTelegramBotApiHttpClient({
      token,
      baseUrl: state.config.telegram.apiBaseUrl,
      timeoutMs:
        Math.max(1, state.config.telegram.pollTimeoutSeconds) * 1_000 + 5_000,
    }),
    offsetStore: new FileTelegramUpdateOffsetStore(
      join(
        state.config.paths.dataDir,
        "data",
        "telegram",
        `${state.config.telegram.adapterId}-offset.json`,
      ),
    ),
    bindings: () =>
      activeTelegramChannelBindings(
        state.runtimeConfig.channelBindings,
        state.config.telegram.adapterId,
      ),
    ttlMs: state.config.telegram.messageTtlMs,
    pollIntervalMs: state.config.telegram.pollIntervalMs,
    pollTimeoutSeconds: state.config.telegram.pollTimeoutSeconds,
    updateLimit: state.config.telegram.updateLimit,
    now: state.now,
    ingest: async (message) =>
      ingestChannelInboundMessage(message, {
        bridge: {
          injectExternalEvent: (event) =>
            state.bridge.injectExternalEvent(event),
          routeAgentMessage: (agentMessage) =>
            state.bridge.routeAgentMessage(
              agentMessage.from,
              agentMessage.to,
              agentMessage.body,
              agentMessage.correlationId,
            ),
        },
        bindings: state.runtimeConfig.channelBindings,
        ensureSessionForRoute: ({ binding }) =>
          ensureConfiguredSessionForChannelBinding({
            bridge: state.bridge,
            runtimeConfig: state.runtimeConfig,
            binding,
          }),
        now: state.now(),
      }),
  });
  const outboundSubscription = await state.bridge.subscribeEvents({
    eventKinds: ["agent_message_routed"],
  });
  state.telegramConnector = connector;
  state.telegramOutboundSubscription = outboundSubscription;
  await connector.start();
  recordServiceEvent(state, {
    source: "telegram",
    eventType: "telegram_connector_started",
    summary: `Telegram connector started with ${connector.diagnostics().bindingCount} active binding(s).`,
  });
}

async function restartTelegramConnector(state: ServiceState): Promise<void> {
  await stopTelegramConnector(state);
  await startTelegramConnector(state);
}

async function stopTelegramConnector(state: ServiceState): Promise<void> {
  state.telegramConnector?.stop();
  state.telegramConnector = undefined;
  const subscription = state.telegramOutboundSubscription;
  state.telegramOutboundSubscription = undefined;
  if (subscription !== undefined) {
    await state.bridge.unsubscribeEvents(subscription).catch(() => undefined);
  }
}

function activeTelegramChannelBindings(
  bindings: readonly ChannelBindingRecord[],
  adapterId: string,
): ChannelBindingRecord[] {
  return bindings.filter(
    (binding) =>
      binding.status === "active" &&
      binding.provider === "telegram" &&
      binding.adapterId === adapterId,
  );
}

async function drainTelegramOutboundMessages(
  state: ServiceState,
): Promise<void> {
  const connector = state.telegramConnector;
  const subscription = state.telegramOutboundSubscription;
  if (state.stopping || connector === undefined || subscription === undefined) {
    return;
  }
  const events = await state.bridge.drainSubscriptionEvents(subscription, 128);
  for (const event of events) {
    if (event.type !== "agent_message_routed") continue;
    const projection = projectAgentMessageToChannel(
      event.message,
      activeTelegramChannelBindings(
        state.runtimeConfig.channelBindings,
        state.config.telegram.adapterId,
      ),
      { now: state.now() },
    );
    if (projection.status === "projected") {
      const dispatch = await dispatchChannelMessageProjection(
        {
          sendMessage: (message) => connector.sendOutbound(message),
          sendActivity: () => undefined,
        },
        projection.message,
      );
      if (!dispatch.accepted) {
        recordChannelProjectionFailure(
          state,
          projection.binding.bindingId,
          dispatch.kind,
          dispatch.degradedReason,
        );
      }
      continue;
    }
    if (projection.status !== "not_channel_target") {
      recordChannelProjectionFailure(
        state,
        projection.candidates[0]?.bindingId ?? "telegram:unresolved",
        "message",
        projection.reason,
      );
    }
  }
}

function recordChannelProjectionFailure(
  state: ServiceState,
  bindingId: string,
  kind: ChannelProjectionFailureRecord["kind"],
  degradedReason: string,
): void {
  state.channelProjectionFailures.push({
    bindingId,
    kind,
    degradedReason,
    observedAt: state.now(),
  });
  state.channelProjectionFailures.splice(
    0,
    Math.max(0, state.channelProjectionFailures.length - 100),
  );
  recordServiceEvent(state, {
    source: "telegram",
    eventType: "telegram_projection_degraded",
    severity: "warning",
    summary: `${bindingId}: ${degradedReason}`,
  });
}

function telegramChannelActivityDiagnostics(
  state: ServiceState,
  now: string,
): ChannelBindingDiagnostics[] {
  const connector = state.telegramConnector;
  const diagnostics = connector?.diagnostics();
  return activeTelegramChannelBindings(
    state.runtimeConfig.channelBindings,
    state.config.telegram.adapterId,
  ).map((binding) => ({
    bindingId: binding.bindingId,
    adapterId: binding.adapterId,
    membershipStatus: "joined",
    presenceStatus: connector === undefined ? "offline" : "online",
    subscriptionStatus:
      connector === undefined
        ? "disconnected"
        : diagnostics?.lastError
          ? "degraded"
          : "active",
    degradedReason:
      connector === undefined
        ? state.config.telegram.enabled
          ? "telegram connector is not running"
          : "telegram connector is disabled"
        : diagnostics?.lastError,
    stale:
      connector === undefined ||
      (diagnostics?.lastPollAt === undefined
        ? false
        : Date.parse(now) - Date.parse(diagnostics.lastPollAt) >
          Math.max(30_000, state.config.telegram.pollIntervalMs * 5)),
  }));
}

function denConversationChannelActivityDiagnostics(
  state: ServiceState,
): ChannelBindingDiagnostics[] {
  return activeDenChannelBindings(state.runtimeConfig.channelBindings).map(
    (binding) => {
      const resolution = state.denConversationChannelResolutionsByBindingId.get(
        binding.bindingId,
      );
      const channelId = resolution?.channelId;
      const membership = state.denConversationMembershipsByBindingId.get(
        binding.bindingId,
      );
      const membershipStatus =
        membership === undefined
          ? "missing"
          : denConversationMembershipStatus(membership.membership_status);
      const subscriptionStatus = denConversationSubscriptionStatus(membership);
      const resolved = channelId !== undefined;
      return {
        bindingId: binding.bindingId,
        adapterId: binding.adapterId,
        conversationProjectId:
          resolution?.projectId ??
          conversationProjectIdForBinding(state, binding),
        conversationChannelId: channelId,
        membershipStatus,
        presenceStatus:
          membershipStatus === "joined"
            ? "online"
            : resolved
              ? "offline"
              : "missing",
        subscriptionStatus,
        degradedReason: denConversationDiagnosticReason({
          resolved,
          membership,
          membershipStatus,
          subscriptionStatus,
        }),
        stale: false,
      };
    },
  );
}

function denConversationMembershipStatus(
  status: string,
): ChannelMembershipStatus {
  switch (status) {
    case "active":
      return "joined";
    case "left":
      return "left";
    case "invited":
      return "invited";
    default:
      return "unknown";
  }
}

function denConversationSubscriptionStatus(
  membership: DenSuccessorConversationMembership | undefined,
): ChannelSubscriptionStatus | "missing" {
  if (membership === undefined) return "missing";
  if (membership.membership_status === "left") return "archived";
  if (membership.membership_status !== "active") return "degraded";
  return membership.wake_policy === "never" ? "paused" : "active";
}

function denConversationDiagnosticReason(input: {
  resolved: boolean;
  membership: DenSuccessorConversationMembership | undefined;
  membershipStatus: ChannelMembershipStatus | "missing";
  subscriptionStatus: ChannelSubscriptionStatus | "missing";
}): string | undefined {
  if (!input.resolved) return "Den Conversation channel is not resolved";
  if (input.membership === undefined) {
    return "Den Conversation membership is missing";
  }
  if (input.membershipStatus !== "joined") {
    return `Den Conversation membership is ${input.membership.membership_status}`;
  }
  if (input.subscriptionStatus !== "active") {
    return `Den Conversation wake policy is ${input.membership.wake_policy}`;
  }
  return undefined;
}

async function reloadServiceRuntimeConfig(
  state: ServiceState,
): Promise<RustyCrewRuntimeConfigApplyResult> {
  return applyServiceRuntimeConfigFromDisk(state, {
    createMissingSessions: false,
    eventType: "runtime_config_reloaded",
    summaryPrefix: "Runtime config reloaded",
  });
}

async function applyServiceRuntimeConfigFromDisk(
  state: ServiceState,
  options: {
    createMissingSessions: boolean;
    eventType: string;
    summaryPrefix: string;
  },
): Promise<RustyCrewRuntimeConfigApplyResult> {
  const nextRuntimeConfig = await loadRustyCrewRuntimeConfig(state.config);
  const nextProfileChannelWakePolicies =
    await loadProfileChannelWakePolicies(nextRuntimeConfig);
  const nextMcpManager = await createServiceMcpManager(nextRuntimeConfig);
  const nextApplyResult = await applyRustyCrewRuntimeConfig({
    serviceConfig: state.config,
    runtimeConfig: nextRuntimeConfig,
    bridge: state.bridge,
    existingBrainHandlesByProfileId:
      state.runtimeConfigApplyResult.brainHandlesByProfileId,
    createMissingSessions: options.createMissingSessions,
    curatorExecutor: state.curator.executor,
    mcpSurfaceDiagnostics: nextMcpManager.diagnostics(),
  });
  const previousMcpManager = state.mcpManager;
  state.runtimeConfig = nextRuntimeConfig;
  state.profileChannelWakePolicies = nextProfileChannelWakePolicies;
  state.runtimeConfigApplyResult = nextApplyResult;
  state.curator.runtimeConfig = nextRuntimeConfig;
  state.backgroundReview.enabled =
    createServiceBackgroundReviewRuntime(nextRuntimeConfig).enabled;
  state.mcpManager = nextMcpManager;
  await previousMcpManager.shutdown();
  await ensureDenConversationChannels(state);
  await restartTelegramConnector(state);
  recordServiceEvent(state, {
    source: "service-host",
    eventType: options.eventType,
    summary: runtimeConfigApplySummary(options.summaryPrefix, nextApplyResult),
  });
  return nextApplyResult;
}

interface CreatedServiceProfile {
  profileId: string;
  displayName?: string;
  agentId: string;
  sessionId: string;
  implementationId: string;
  profilePath: string;
  runtimeConfigPath: string;
  applyResult: RustyCrewRuntimeConfigApplyResult;
}

async function createServiceProfile(
  state: ServiceState,
  command: AdminControlCommand,
): Promise<CreatedServiceProfile> {
  const profileId = validateProfileComponent(
    requiredBodyString(command, "profileId"),
    "profileId",
  );
  const displayName = optionalBodyString(command, "displayName");
  const agentId = validateProfileComponent(
    optionalBodyString(command, "agentId") ?? profileId,
    "agentId",
  );
  const sessionId = validateProfileComponent(
    optionalBodyString(command, "sessionId") ?? `${agentId}-session`,
    "sessionId",
  );
  const implementationId = validateProfileComponent(
    optionalBodyString(command, "implementationId") ?? `${profileId}-brain`,
    "implementationId",
  );
  const kind = optionalBodyString(command, "kind") ?? "full";
  if (kind !== "full" && kind !== "worker" && kind !== "delegated") {
    throw new Error("profile session kind must be full, worker, or delegated");
  }
  const modelConfig = modelConfigFromBody(command.body.modelConfig);
  const mcpToolProfile =
    optionalBodyString(command, "mcpToolProfile") ?? profileId;
  const profilePath = join(
    state.runtimeConfig.profilesDir,
    `${profileId}.json`,
  );
  if (existsSync(profilePath)) {
    throw new Error(`profile ${profileId} already exists`);
  }

  const runtimeConfigFile = await readRuntimeConfigFileForMutation(state);
  const brains = runtimeConfigFile.array("brains");
  const sessions = runtimeConfigFile.array("sessions");
  if (
    brains.some(
      (item) =>
        isRecord(item) &&
        (item.profileId === profileId ||
          item.implementationId === implementationId),
    )
  ) {
    throw new Error(`runtime config already has a brain for ${profileId}`);
  }
  if (
    sessions.some(
      (item) =>
        isRecord(item) &&
        (item.profileId === profileId ||
          item.sessionId === sessionId ||
          item.agentId === agentId),
    )
  ) {
    throw new Error(`runtime config already has a session for ${profileId}`);
  }

  await mkdir(state.runtimeConfig.profilesDir, { recursive: true });
  await writeJsonFileAtomic(profilePath, {
    profileId,
    ...(displayName === undefined ? {} : { displayName }),
    modelConfig,
    brain: {
      module: resolveBrainModuleSelection({ modelConfig }).moduleId,
    },
    mcpConfig: {
      bindingId: `${agentId}-mcp`,
      serverNames: [agentId],
      endpointRef: `config://mcp/${agentId}`,
      toolProfile: mcpToolProfile,
    },
    skills: "all",
  });

  brains.push({ profileId, implementationId });
  sessions.push({
    sessionId,
    agentId,
    profileId,
    kind: kind as SessionKind,
  });
  await writeJsonFileAtomic(
    state.config.paths.serviceConfigFile,
    runtimeConfigFile.value,
  );

  const applyResult = await applyServiceRuntimeConfigFromDisk(state, {
    createMissingSessions: true,
    eventType: "profile_created",
    summaryPrefix: `Profile ${profileId} created`,
  });
  return {
    profileId,
    ...(displayName === undefined ? {} : { displayName }),
    agentId,
    sessionId,
    implementationId,
    profilePath,
    runtimeConfigPath: state.config.paths.serviceConfigFile,
    applyResult,
  };
}

function validateProfileComponent(value: string, field: string): string {
  if (!/^[A-Za-z0-9][A-Za-z0-9_-]{0,79}$/.test(value)) {
    throw new Error(
      `${field} must start with a letter or number and contain only letters, numbers, underscore, or hyphen`,
    );
  }
  return value;
}

function modelConfigFromBody(input: unknown): BrainModelConfig {
  if (input === undefined) {
    return { provider: "local", modelName: "deterministic" };
  }
  if (!isRecord(input)) {
    throw new Error("modelConfig must be an object when provided");
  }
  const provider = optionalString(input.provider) ?? "local";
  const modelName = optionalString(input.modelName) ?? "deterministic";
  return compactRecord({
    provider,
    modelName,
    baseUrl: optionalString(input.baseUrl),
    api: optionalString(input.api),
    apiKeyEnv: optionalString(input.apiKeyEnv),
    temperatureMilli: optionalNumber(input.temperatureMilli),
    maxOutputTokens: optionalNumber(input.maxOutputTokens),
  }) as unknown as BrainModelConfig;
}

interface RuntimeConfigFileForMutation {
  value: Record<string, unknown>;
  array(key: string): unknown[];
}

async function readRuntimeConfigFileForMutation(
  state: ServiceState,
): Promise<RuntimeConfigFileForMutation> {
  let parsed: unknown;
  try {
    parsed = JSON.parse(
      await readFile(state.config.paths.serviceConfigFile, "utf8"),
    );
  } catch (error) {
    if (isNodeError(error) && error.code === "ENOENT") {
      parsed = {};
    } else {
      throw error;
    }
  }
  if (!isRecord(parsed)) {
    throw new Error("service runtime config root must be an object");
  }
  if (parsed.profilesDir === undefined) {
    parsed.profilesDir = state.runtimeConfig.profilesDir;
  }
  if (
    state.runtimeConfig.skillsDir !== undefined &&
    parsed.skillsDir === undefined
  ) {
    parsed.skillsDir = state.runtimeConfig.skillsDir;
  }
  return {
    value: parsed,
    array(key) {
      const existing = parsed[key];
      if (existing === undefined) {
        const created: unknown[] = [];
        parsed[key] = created;
        return created;
      }
      if (!Array.isArray(existing)) {
        throw new Error(`runtime config ${key} must be an array`);
      }
      return existing;
    },
  };
}

async function writeJsonFileAtomic(
  path: string,
  value: unknown,
): Promise<void> {
  await mkdir(dirname(path), { recursive: true });
  const tmpPath = `${path}.${process.pid}.${Date.now()}.tmp`;
  await writeFile(tmpPath, `${JSON.stringify(value, null, 2)}\n`);
  await rename(tmpPath, path);
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
    ...createCuratorAdminControlExecutor({
      curatorExecutor: state.curator.executor,
      rollbackMutation: (mutationId) =>
        rollbackCuratorMutation(state.curator.store, mutationId),
      status: () => curatorStatus(state),
      skillsDir: curatorSkillsDir(state.curator.runtimeConfig),
    }),
    createProfile: async (command) => {
      const result = await createServiceProfile(state, command);
      return {
        status: "completed",
        summary: `profile ${result.profileId} created with session ${result.sessionId}`,
        affectedIds: {
          profileId: result.profileId,
          agentId: result.agentId,
          sessionId: result.sessionId,
          implementationId: result.implementationId,
        },
        result,
      };
    },
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
    newSession: createNewSessionLifecycleExecutor({
      loadTemplate: async (currentSessionId) => {
        const session = await serviceSessionById(state, currentSessionId);
        const channelBinding = channelBindingForSession(
          state,
          currentSessionId,
        );
        return {
          agentId: session.agentId,
          profileId: session.profileId,
          kind: session.kind,
          channelBindingId: channelBinding?.bindingId,
          channelId: channelBinding?.externalChannelId,
          toolProfileKey: mcpBindingForSession(state, currentSessionId)
            ?.toolProfileKey,
          sessionConfig: {
            resourceLimits: session.resourceLimits,
            toolProfile: session.toolProfile,
            historyWindow: session.historyWindow,
          },
        };
      },
      generateSessionId: (template) => {
        state.nextWakeSequence += 1;
        return [
          template.agentId,
          "session",
          state
            .now()
            .replace(/[^0-9A-Za-z]/g, "")
            .slice(0, 17),
          state.nextWakeSequence,
        ].join("-");
      },
      archiveSession: async ({ sessionId }) => {
        await state.bridge.archiveSession(sessionId as SessionId);
      },
      createSession: async ({ sessionId, template }) => {
        const sessionConfig = optionalRecord(template.sessionConfig) ?? {};
        await state.bridge.createSession({
          sessionId,
          agentId: template.agentId,
          profileId: template.profileId,
          kind: template.kind,
          resourceLimits: compactRecord(
            optionalRecord(sessionConfig.resourceLimits) ?? {},
          ),
          toolProfile:
            optionalRecord(sessionConfig.toolProfile) === undefined
              ? undefined
              : (sessionConfig.toolProfile as never),
          historyWindow:
            optionalRecord(sessionConfig.historyWindow) === undefined
              ? undefined
              : (compactRecord(sessionConfig.historyWindow as never) as never),
        });
      },
      auditSink: {
        writeNewSessionLifecycleAudit(event) {
          recordServiceEvent(state, {
            source: "service-host",
            eventType: `new_session_${event.phase}`,
            summary: `New-session lifecycle ${event.phase} for ${event.oldSessionId}.`,
          });
        },
      },
      now: state.now,
    }),
    reloadMcp: createReloadMcpControlExecutor({
      resolveBinding: (sessionId) => mcpBindingForSession(state, sessionId),
      manager: state.mcpManager,
      discoveryClient: {
        listTools: () => [],
      },
      discoveryClientForBinding: (binding) =>
        createDefaultMcpDiscoveryClient(binding, state.config.mcp),
      catalogId: (binding) => `mcp:${binding.toolProfileKey}`,
      previousToolNames: () => [],
      inventoryRequest: (binding) => ({
        requestedToolsets: [`mcp:${binding.toolProfileKey}`],
      }),
      auditSink: {
        writeReloadMcpLifecycleAudit(event) {
          recordServiceEvent(state, {
            source: "service-host",
            eventType: `reload_mcp_${event.phase}`,
            severity: event.phase === "degraded" ? "warning" : undefined,
            summary: `Reload MCP lifecycle ${event.phase} for ${event.sessionId}.`,
          });
        },
      },
      now: state.now,
    }),
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
      const curatorLifecycle =
        await runServiceCuratorLifecycleTransitions(state);
      return {
        status: "completed",
        summary: "scheduler tick completed",
        result: { scheduler: report, curatorLifecycle },
      };
    },
    schedulerRunJob: async (command) => {
      const job = (await state.bridge.listScheduledJobs({ limit: 100 })).find(
        (candidate) => candidate.jobId === command.target.jobId,
      );
      if (job && scheduledHostJobKinds.includes(job.jobKind as never)) {
        const run = await state.bridge.requestScheduledHostJobRun({
          jobId: command.target.jobId,
          supportedJobKinds: [...scheduledHostJobKinds],
        });
        if (!run) {
          return {
            status: "completed",
            summary: `scheduled host job ${command.target.jobId} was not found`,
            result: null,
          };
        }
        const outcome = await executeScheduledHostRun(
          scheduledHostExecutorContext(state),
          run,
        );
        const affectedIds: Record<string, string | number> = {
          jobId: command.target.jobId,
          runId: run.runId,
        };
        return {
          status: outcome === "completed" ? "completed" : "failed",
          summary: `scheduled host job ${command.target.jobId} ${outcome}`,
          affectedIds,
          result: run,
        };
      }
      const run = await state.bridge.requestScheduledJobRun(
        command.target.jobId,
      );
      if (!run) {
        return {
          status: "completed",
          summary: `scheduled job ${command.target.jobId} was not due or not found`,
          result: null,
        };
      }
      const affectedIds: Record<string, string | number> = {
        jobId: command.target.jobId,
      };
      return {
        status: "completed",
        summary: `scheduled job ${command.target.jobId} run requested`,
        affectedIds,
        result: run,
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

async function serviceSessionById(
  state: ServiceState,
  sessionId: string,
): Promise<SessionState> {
  const session = (await state.bridge.listSessions()).find(
    (candidate) => candidate.sessionId === sessionId,
  );
  if (!session) {
    throw new Error(`session ${sessionId} was not found`);
  }
  return session;
}

function channelBindingForSession(
  state: ServiceState,
  sessionId: string,
): ChannelBindingRecord | undefined {
  return state.runtimeConfig.channelBindings.find(
    (binding) => binding.sessionId === sessionId,
  );
}

function mcpBindingForSession(
  state: ServiceState,
  sessionId: string,
): McpBindingRecord | undefined {
  return state.runtimeConfig.mcpBindings.find(
    (binding) => binding.sessionId === sessionId,
  );
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

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function optionalNumber(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value)
    ? value
    : undefined;
}

function isNodeError(error: unknown): error is NodeJS.ErrnoException {
  return error instanceof Error && "code" in error;
}

interface ServiceWakeDispatchReport {
  sessionId: SessionId;
  wakeId?: string;
  status: "completed" | "skipped" | "failed";
  summary: string;
  reasonCode?: string;
}

interface ChatStreamSubscriber {
  write(event: ChatEvent): void;
}

interface ServiceWakeObservationContext {
  deliveryIntentId?: number;
  channelId?: number;
  channelMessageId?: number;
}

type ServiceWakeSource = "background" | "direct_debug" | "delivery" | "chat";

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

  if (state.telegramConnector !== undefined) {
    const timer = setInterval(
      () => {
        void drainTelegramOutboundMessages(state).catch((error) =>
          recordServiceEvent(state, {
            source: "telegram",
            eventType: "telegram_outbound_drain_failed",
            severity: "error",
            summary: errorMessage(error, "Telegram outbound drain failed"),
          }),
        );
      },
      Math.max(250, state.config.telegram.pollIntervalMs),
    );
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
    if (session === undefined) {
      recordUnmatchedDeliveryIntent(state, intent);
      continue;
    }
    const decision = deliveryIntentWakeDecision({
      wakePolicy: channelWakePolicyForSession(state, session),
      expiresAt: intent.expires_at,
      now: state.now(),
    });
    if (decision.action === "skip_expired") {
      state.claimedDeliveryIntentIds.add(intent.id);
      recordServiceEvent(state, {
        source: "den-successor-gateway",
        eventType: "den_delivery_intent_expired",
        severity: "warning",
        summary: `Skipped expired Den Delivery intent ${intent.id} for ${intent.target_identity.profile}.`,
      });
      continue;
    }
    if (decision.action === "manual_wait") {
      state.claimedDeliveryIntentIds.add(intent.id);
      recordDynamicDenDeliveryChannel(state, intent, session, {
        channelId: channelIdFromDeliveryIntent(intent),
        sourceMessageId: intent.channel_message_id,
        wakePolicy: decision.wakePolicy,
        subscriptionStatus: "manual",
      });
      recordServiceEvent(state, {
        source: "den-successor-gateway",
        eventType: "den_delivery_intent_manual",
        summary: `Left Den Delivery intent ${intent.id} pending for manual wake policy on ${session.agentId}; Gateway TTL remains authoritative.`,
      });
      continue;
    }
    if (decision.action === "reject") {
      state.claimedDeliveryIntentIds.add(intent.id);
      recordDynamicDenDeliveryChannel(state, intent, session, {
        channelId: channelIdFromDeliveryIntent(intent),
        sourceMessageId: intent.channel_message_id,
        wakePolicy: decision.wakePolicy,
        subscriptionStatus: "disabled",
      });
      void rejectDenDeliveryIntent(state, intent, session, decision).catch(
        (error) =>
          recordServiceEvent(state, {
            source: "den-successor-gateway",
            eventType: "den_delivery_intent_reject_failed",
            severity: "error",
            summary: errorMessage(
              error,
              `Den Delivery intent ${intent.id} reject failed`,
            ),
          }),
      );
      continue;
    }
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
  const claimedBy = intent.target_identity;
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
    recordDynamicDenDeliveryChannel(state, intent, session, deliveryBody);

    const wakeReport = await submitServiceTurn(state, {
      sessionId: session.sessionId,
      from: "den-delivery",
      body: deliveryBody.body,
      correlationId: `delivery:${intent.id}:${intent.idempotency_key}`,
      source: "delivery",
      observationContext: {
        deliveryIntentId: intent.id,
        channelId: deliveryBody.channelId,
        channelMessageId: deliveryBody.sourceMessageId,
      },
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

function recordUnmatchedDeliveryIntent(
  state: ServiceState,
  intent: DenSuccessorDeliveryIntent,
): void {
  if (state.unmatchedDeliveryIntentIds.has(intent.id)) return;
  state.unmatchedDeliveryIntentIds.add(intent.id);
  recordServiceEvent(state, {
    source: "den-successor-gateway",
    eventType: "den_delivery_intent_unmatched",
    severity: "warning",
    summary: `Pending Den Delivery intent ${intent.id} for ${deliveryIdentitySummary(intent.target_identity)} does not match any configured Rusty Crew session.`,
  });
}

async function rejectDenDeliveryIntent(
  state: ServiceState,
  intent: DenSuccessorDeliveryIntent,
  session: RustyCrewRuntimeConfig["sessions"][number],
  decision: Extract<DeliveryIntentWakeDecision, { action: "reject" }>,
): Promise<void> {
  if (state.denGatewayClient === undefined) return;
  const claimToken = `rusty-crew:${intent.id}:${Date.now()}`;
  const claimedBy = intent.target_identity;
  await state.denGatewayClient.claimDeliveryIntent({
    id: intent.id,
    claimToken,
    claimedBy,
  });
  await state.denGatewayClient.reportDeliveryIntentEvent({
    id: intent.id,
    claimToken,
    eventType: "failed",
    payload: {
      source: "rusty-crew",
      session_id: session.sessionId,
      reason: decision.reasonCode,
      summary: decision.summary,
    },
  });
  recordServiceEvent(state, {
    source: "den-successor-gateway",
    eventType: "den_delivery_intent_rejected",
    summary: `Rejected Den Delivery intent ${intent.id} for ${session.agentId}: ${decision.summary}.`,
  });
}

async function submitRustyViewChatMessage(
  state: ServiceState,
  input: ChatSendMessageInput,
): Promise<SendChatMessageResult> {
  const receiptKey = `${input.session.sessionId}:${input.idempotencyKey}`;
  const existing = state.chatMessageReceipts.get(receiptKey);
  if (existing !== undefined) {
    return { ...existing, status: "duplicate" };
  }
  const messageId = input.clientMessageId ?? `chat:${input.idempotencyKey}`;
  const correlationId = `chat:${input.idempotencyKey}`;
  const inbound = appendChatEvent(state, input.session.sessionId, {
    kind: "message_created",
    payload: {
      message_id: messageId,
      role: input.actor.kind === "agent" ? "assistant" : "user",
      actor: input.actor,
      body: input.body,
      correlation_id: correlationId,
      reason: input.reason,
    },
  });
  const wakeReport = await submitServiceTurn(state, {
    sessionId: input.session.sessionId,
    from: input.actor.id,
    body: input.body,
    correlationId,
    source: "chat",
  });
  const result: SendChatMessageResult = {
    status: wakeReport.status === "completed" ? "accepted" : "rejected",
    message_id: messageId,
    wake_id: wakeReport.wakeId,
    correlation_id: correlationId,
    latest_cursor:
      latestChatCursor(state, input.session.sessionId) ?? inbound.event_id,
    reason_code: wakeReport.reasonCode,
  };
  rememberChatMessageReceipt(state, receiptKey, result);
  return result;
}

async function executeRustyViewChatCommand(
  state: ServiceState,
  input: ExecuteChatCommandInput,
): Promise<ExecuteChatCommandResult> {
  const started = appendChatEvent(state, input.session.sessionId, {
    kind: "command_started",
    payload: {
      command: input.command,
      actor: input.actor,
      request_id: input.requestId,
    },
  });
  const routed = routeSlashCommand({
    text: input.command,
    session: slashCommandSession(input.session),
    actor: {
      id: input.actor.id,
      displayName: input.actor.display_name,
    },
    options: {
      primeProfiles: [input.session.profileId],
      allowNonPrimeReadCommands: true,
    },
  });
  if (routed.kind === "pass_through") {
    return completeChatCommand(state, input.session.sessionId, {
      status: "rejected",
      command_name: "unknown",
      summary:
        "Only slash commands can be executed through the chat command API.",
      latest_cursor: started.event_id,
      reason_code: "not_a_slash_command",
    });
  }
  if (routed.status !== "ok") {
    return completeChatCommand(state, input.session.sessionId, {
      status: "rejected",
      command_name: routed.commandName,
      summary: routed.response.summary,
      latest_cursor: started.event_id,
      reason_code:
        routed.status === "denied" ? "slash_command_denied" : "unknown_command",
      response: routed.response,
    });
  }
  if (
    routed.commandName === "help" ||
    routed.commandName === "status" ||
    routed.commandName === "session"
  ) {
    const diagnosticsContext = await buildDiagnosticsContext(state);
    const response = buildReadOnlySlashCommandResponse(routed.commandName, {
      diagnostics: diagnosticsContext.diagnostics,
      session: slashCommandSession(input.session),
      options: {
        primeProfiles: [input.session.profileId],
        allowNonPrimeReadCommands: true,
      },
    });
    return completeChatCommand(state, input.session.sessionId, {
      status: "completed",
      command_name: routed.commandName,
      summary: response.summary,
      latest_cursor: started.event_id,
      response,
    });
  }
  if (routed.controlRequest) {
    const control = await handleAdminControlRequest(
      {
        method: "POST",
        url: controlUrlForSlashCommand(
          routed.controlRequest.commandName,
          input.session.sessionId,
        ),
        headers: {
          authorization: `Bearer ${controlBearerToken(state)}`,
          "x-rusty-crew-operator": input.actor.id,
        },
        body: {
          ...routed.controlRequest.body,
          reason: routed.controlRequest.reason,
          reasonCode: routed.controlRequest.reasonCode,
        },
        requestId: input.requestId,
      },
      {
        auth: {
          bearerToken: controlBearerToken(state),
          operatorId: input.actor.id,
        },
        auditSink: state.auditSink,
        executor: createServiceControlExecutor(state),
        now: state.now,
      },
    );
    const result: Pick<AdminControlResponse, "outcome"> = control.body.ok
      ? (control.body.data as AdminControlResponse)
      : {
          outcome: {
            status: "failed" as const,
            summary: control.body.error.message,
            reasonCode: control.body.error.reason_code,
          },
        };
    const outcome = result.outcome;
    const affected = outcome.affectedIds ?? {};
    return completeChatCommand(state, input.session.sessionId, {
      status: outcome.status === "completed" ? "completed" : "failed",
      command_name: routed.commandName,
      summary: outcome.summary,
      latest_cursor: started.event_id,
      old_session_id: stringRecordValue(affected, "oldSessionId"),
      new_session_id: stringRecordValue(affected, "newSessionId"),
      reason_code: outcome.reasonCode,
      response: { outcome, control_status: control.status },
    });
  }
  return completeChatCommand(state, input.session.sessionId, {
    status: "failed",
    command_name: routed.commandName,
    summary: "Slash command did not produce an executable action.",
    latest_cursor: started.event_id,
    reason_code: "missing_command_action",
  });
}

function completeChatCommand(
  state: ServiceState,
  sessionId: SessionId,
  result: ExecuteChatCommandResult,
): ExecuteChatCommandResult {
  const completed = appendChatEvent(state, sessionId, {
    kind:
      result.status === "completed" ? "command_completed" : "command_failed",
    payload: { ...result },
  });
  return {
    ...result,
    latest_cursor: completed.event_id,
  };
}

function slashCommandSession(session: SessionState): SlashCommandSession {
  return {
    sessionId: session.sessionId,
    agentId: session.agentId,
    profileId: session.profileId,
    kind: session.kind,
  };
}

function controlUrlForSlashCommand(
  commandName: string,
  sessionId: SessionId,
): string {
  if (commandName === "new_session") {
    return `/v1/admin/control/sessions/${sessionId}/new`;
  }
  if (commandName === "reload_mcp") {
    return `/v1/admin/control/mcp/${sessionId}/reload`;
  }
  return `/v1/admin/control/unsupported/${commandName}`;
}

function stringRecordValue(
  record: Record<string, string | number>,
  key: string,
): string | undefined {
  const value = record[key];
  return typeof value === "string" ? value : undefined;
}

function rememberChatMessageReceipt(
  state: ServiceState,
  key: string,
  result: SendChatMessageResult,
): void {
  state.chatMessageReceipts.set(key, result);
  if (state.chatMessageReceipts.size <= 500) return;
  const first = state.chatMessageReceipts.keys().next().value;
  if (typeof first === "string") {
    state.chatMessageReceipts.delete(first);
  }
}

function appendCoreEventsToChatLog(
  state: ServiceState,
  session: SessionState,
  events: readonly CoreEvent[],
): void {
  for (const event of events) {
    if (
      event.type === "brain_event_observed" &&
      event.sessionId === session.sessionId
    ) {
      appendBrainEventToChatLog(state, session, event.wakeId, event.event);
    } else if (
      event.type === "completion_packet_delivered" &&
      event.packet.sessionId === session.sessionId
    ) {
      appendChatEvent(state, session.sessionId, {
        kind: "assistant_message_completed",
        payload: {
          status: event.packet.status,
          summary: event.packet.summary,
        },
      });
    } else if (
      event.type === "brain_actions_accepted" &&
      event.sessionId === session.sessionId
    ) {
      appendChatEvent(state, session.sessionId, {
        kind: "unknown",
        payload: {
          source_event_type: event.type,
          accepted_action_count: event.count,
        },
      });
    }
  }
}

function appendBrainEventToChatLog(
  state: ServiceState,
  session: SessionState,
  wakeId: string | undefined,
  event: BrainEvent,
): void {
  switch (event.type) {
    case "started":
      appendChatEvent(state, session.sessionId, {
        kind: "assistant_turn_started",
        payload: { wake_id: wakeId },
      });
      return;
    case "text_delta":
      appendChatEvent(state, session.sessionId, {
        kind: "assistant_text_delta",
        payload: { wake_id: wakeId, text: event.text },
      });
      return;
    case "tool_call_started":
      appendChatEvent(state, session.sessionId, {
        kind: "tool_call_started",
        payload: {
          wake_id: wakeId,
          tool_name: event.toolName,
          metadata: event.metadata,
        },
      });
      return;
    case "tool_call_finished":
      appendChatEvent(state, session.sessionId, {
        kind: event.isError ? "tool_call_failed" : "tool_call_completed",
        payload: {
          wake_id: wakeId,
          tool_name: event.toolName,
          is_error: event.isError,
          metadata: event.metadata,
        },
      });
      return;
    case "finished":
      appendChatEvent(state, session.sessionId, {
        kind: "assistant_turn_finished",
        payload: { wake_id: wakeId },
      });
      return;
  }
}

function ensureChatWakeTerminalEvents(
  state: ServiceState,
  session: SessionState,
  wakeId: string,
  events: readonly CoreEvent[],
  fallback: { summary?: string },
): void {
  const wakeEvents = events.filter(
    (event) =>
      (event.type === "brain_event_observed" &&
        event.sessionId === session.sessionId &&
        (event.wakeId === undefined || event.wakeId === wakeId)) ||
      (event.type === "completion_packet_delivered" &&
        event.packet.sessionId === session.sessionId),
  );
  const hasAssistantTurn = wakeEvents.some(
    (event) =>
      event.type === "brain_event_observed" &&
      (event.event.type === "started" ||
        event.event.type === "text_delta" ||
        event.event.type === "tool_call_started" ||
        event.event.type === "tool_call_finished"),
  );
  if (!hasAssistantTurn) return;

  const hasCompletion = wakeEvents.some(
    (event) => event.type === "completion_packet_delivered",
  );
  const hasFinished = wakeEvents.some(
    (event) =>
      event.type === "brain_event_observed" && event.event.type === "finished",
  );

  if (!hasCompletion && fallback.summary?.trim()) {
    appendChatEvent(state, session.sessionId, {
      kind: "assistant_message_completed",
      payload: {
        status: "completed",
        summary: fallback.summary.trim(),
        wake_id: wakeId,
        source: "terminal_fallback",
      },
    });
  }
  if (!hasFinished) {
    appendChatEvent(state, session.sessionId, {
      kind: "assistant_turn_finished",
      payload: { wake_id: wakeId, source: "terminal_fallback" },
    });
  }
}

function appendChatEvent(
  state: ServiceState,
  sessionId: SessionId,
  event: Pick<ChatEvent, "kind" | "payload">,
): ChatEvent {
  const sequence = (state.chatSequencesBySession.get(sessionId) ?? 0) + 1;
  state.chatSequencesBySession.set(sessionId, sequence);
  const chatEvent: ChatEvent = {
    event_id: `${sessionId}:${sequence}`,
    session_id: sessionId,
    sequence_id: sequence,
    created_at: state.now(),
    kind: event.kind,
    payload: event.payload,
  };
  const events = state.chatEventsBySession.get(sessionId) ?? [];
  events.push(chatEvent);
  if (events.length > 1_000) {
    events.splice(0, events.length - 1_000);
  }
  state.chatEventsBySession.set(sessionId, events);
  const subscribers = state.chatSubscribersBySession.get(sessionId);
  if (subscribers !== undefined) {
    for (const subscriber of subscribers) {
      subscriber.write(chatEvent);
    }
  }
  return chatEvent;
}

function listChatEventsAfterCursor(
  state: ServiceState,
  session: SessionState,
  cursor: string | undefined,
  limit: number,
): readonly ChatEvent[] {
  const after = cursorSequence(cursor, session.sessionId);
  return (state.chatEventsBySession.get(session.sessionId) ?? [])
    .filter((event) => event.sequence_id > after)
    .slice(0, limit);
}

function streamReplayEvents(
  state: ServiceState,
  session: SessionState,
  cursor: string | undefined,
  url: URL,
): readonly ChatEvent[] {
  const limit = optionalInteger(url.searchParams.get("limit")) ?? 500;
  const after = cursorSequence(cursor, session.sessionId);
  const events = listChatEventsAfterCursor(
    state,
    session,
    cursor,
    Math.min(Math.max(limit, 1), 1_000),
  );
  if (after > 0) return events;
  return [
    {
      event_id: `${session.sessionId}:0`,
      session_id: session.sessionId,
      sequence_id: 0,
      created_at: session.lastActiveAt,
      kind: "session_snapshot",
      payload: {
        session_id: session.sessionId,
        agent_id: session.agentId,
        profile_id: session.profileId,
        status: session.status,
      },
    },
    ...events,
  ];
}

function latestChatCursor(
  state: ServiceState,
  sessionId: SessionId,
): string | undefined {
  return state.chatEventsBySession.get(sessionId)?.at(-1)?.event_id;
}

function chatSubscribers(
  state: ServiceState,
  sessionId: SessionId,
): Set<ChatStreamSubscriber> {
  const existing = state.chatSubscribersBySession.get(sessionId);
  if (existing !== undefined) return existing;
  const subscribers = new Set<ChatStreamSubscriber>();
  state.chatSubscribersBySession.set(sessionId, subscribers);
  return subscribers;
}

function writeSseEvent(response: ServerResponse, event: ChatEvent): void {
  if (response.destroyed) return;
  response.write(`id: ${event.event_id}\n`);
  response.write(`event: ${event.kind}\n`);
  response.write(`data: ${JSON.stringify(event)}\n\n`);
}

async function submitServiceTurn(
  state: ServiceState,
  input: {
    sessionId: SessionId;
    from: string;
    body: string;
    correlationId: string;
    source: Exclude<ServiceWakeSource, "background">;
    observationContext?: ServiceWakeObservationContext;
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
      input.observationContext,
    );
    suppressNextWakeEvent(state, input.sessionId);
    await drainAndDispatchWakes(state, input.source);
    return wakeReport;
  } finally {
    state.directDispatchSessions.delete(input.sessionId);
  }
}

function configuredSessionForDeliveryIntent(
  state: ServiceState,
  intent: DenSuccessorDeliveryIntent,
): RustyCrewRuntimeConfig["sessions"][number] | undefined {
  return state.runtimeConfig.sessions.find((session) => {
    const identity = deliveryIdentityForSession(session);
    return (
      intent.target_identity.profile === identity.profile &&
      intent.target_identity.instance_id === identity.instance_id &&
      (intent.target_identity.session_key === undefined ||
        intent.target_identity.session_key === identity.session_key)
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

function deliveryIdentitySummary(identity: DenSuccessorAgentIdentity): string {
  const sessionKey =
    identity.session_key === undefined
      ? ""
      : ` session ${identity.session_key}`;
  return `${identity.profile}/${identity.instance_id}${sessionKey}`;
}

async function deliveryIntentBody(
  state: ServiceState,
  intent: DenSuccessorDeliveryIntent,
  session: RustyCrewRuntimeConfig["sessions"][number],
): Promise<{ body: string; channelId?: number; sourceMessageId?: number }> {
  const sourceBody = bodyFromWakeSourceRef(intent.source_ref);
  const sourceMessageId =
    messageIdFromWakeSourceRef(intent.source_ref) ?? intent.channel_message_id;
  const channelId =
    channelIdFromDeliveryIntent(intent) ??
    channelIdForConfiguredSession(state, session);
  if (sourceBody !== undefined) {
    return {
      body: sourceBody,
      channelId,
      sourceMessageId,
    };
  }
  if (
    state.denGatewayClient !== undefined &&
    sourceMessageId !== undefined &&
    channelId !== undefined
  ) {
    const messages = await state.denGatewayClient.listConversationMessages({
      channelId,
      afterId: Math.max(0, sourceMessageId - 1),
      limit: 5,
    });
    const message = messages.find(
      (candidate) => candidate.id === sourceMessageId,
    );
    if (message !== undefined) {
      return {
        body: message.body,
        channelId: message.channel_id,
        sourceMessageId: message.id,
      };
    }
  }
  return { body: "", channelId, sourceMessageId };
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
  return state.denConversationChannelResolutionsByBindingId.get(
    binding.bindingId,
  )?.channelId;
}

function bodyFromWakeSourceRef(
  sourceRef: string | undefined,
): string | undefined {
  if (!sourceRef?.trim()) return undefined;
  const parsed = parseWakeSourceRef(sourceRef);
  if (parsed === undefined) return undefined;
  const body = parsed.searchParams.get("body");
  return body?.trim() ? body : undefined;
}

function channelIdFromDeliveryIntent(
  intent: DenSuccessorDeliveryIntent,
): number | undefined {
  const sourceChannelId = channelIdFromWakeSourceRef(intent.source_ref);
  if (sourceChannelId !== undefined) return sourceChannelId;
  const [, channelPart] = intent.idempotency_key.split(":");
  const raw = channelPart?.startsWith("ch")
    ? channelPart.slice(2)
    : channelPart;
  if (!raw || !/^[0-9]+$/.test(raw)) return undefined;
  const parsed = Number(raw);
  return Number.isSafeInteger(parsed) ? parsed : undefined;
}

function channelIdFromWakeSourceRef(
  sourceRef: string | undefined,
): number | undefined {
  if (!sourceRef?.trim()) return undefined;
  const parsed = parseWakeSourceRef(sourceRef);
  if (parsed === undefined) return undefined;
  for (const key of ["channel_id", "conversation_channel_id"]) {
    const value = parsed.searchParams.get(key);
    if (value !== null && /^[0-9]+$/.test(value)) {
      const channelId = Number(value);
      if (Number.isSafeInteger(channelId)) return channelId;
    }
  }
  return parseConversationSourceRef(parsed)?.channelId;
}

function messageIdFromWakeSourceRef(
  sourceRef: string | undefined,
): number | undefined {
  if (!sourceRef?.trim()) return undefined;
  const parsed = parseWakeSourceRef(sourceRef);
  if (parsed === undefined) return undefined;
  return parseConversationSourceRef(parsed)?.messageId;
}

function parseWakeSourceRef(sourceRef: string): URL | undefined {
  try {
    return new URL(sourceRef, "http://rusty-crew.local");
  } catch {
    return undefined;
  }
}

function parseConversationSourceRef(
  sourceRef: URL,
): { channelId: number; messageId?: number } | undefined {
  const match = sourceRef.pathname.match(
    /^\/(?:api\/)?v1\/conversation\/channels\/([0-9]+)(?:\/messages(?:\/([0-9]+))?)?$/,
  );
  if (match === null) return undefined;
  const channelId = Number(match[1]);
  if (!Number.isSafeInteger(channelId)) return undefined;
  const messageId =
    match[2] !== undefined && /^[0-9]+$/.test(match[2])
      ? Number(match[2])
      : undefined;
  if (messageId !== undefined && !Number.isSafeInteger(messageId)) {
    return undefined;
  }
  return { channelId, messageId };
}

function recordDynamicDenDeliveryChannel(
  state: ServiceState,
  intent: DenSuccessorDeliveryIntent,
  session: RustyCrewRuntimeConfig["sessions"][number],
  deliveryBody: {
    channelId?: number;
    sourceMessageId?: number;
    wakePolicy?: ChannelWakePolicy;
    subscriptionStatus?: string;
    lastError?: string;
  },
): void {
  if (deliveryBody.channelId === undefined) return;
  const bindingId = `gateway-delivery:${session.sessionId}:${deliveryBody.channelId}`;
  state.dynamicDenChannelBindings.set(bindingId, {
    bindingId,
    bindingSource: "gateway_delivery",
    adapterId: "den-successor-gateway",
    agentId: session.agentId,
    sessionId: session.sessionId,
    profileId: session.profileId,
    provider: "den_successor_gateway",
    externalChannelId: `conversation:${deliveryBody.channelId}`,
    conversationChannelId: deliveryBody.channelId,
    sourceMessageId: deliveryBody.sourceMessageId,
    deliveryIntentId: intent.id,
    lastObservedAt: state.now(),
    wakePolicy:
      deliveryBody.wakePolicy ?? channelWakePolicyForSession(state, session),
    status: "active",
    membershipStatus: "dynamic",
    presenceStatus: "delivery_intent",
    subscriptionStatus: deliveryBody.subscriptionStatus ?? "active",
    stalePresence: false,
    droppedProjections: 0,
    lastError: deliveryBody.lastError,
  });
}

function scheduledHostExecutorContext(
  state: ServiceState,
): Parameters<typeof runScheduledHostExecutors>[0] {
  return {
    bridge: state.bridge,
    diagnostics: () => buildDiagnosticsContext(state),
    jobPayload: (run) => configuredScheduledJobPayload(state, run.jobId),
    backgroundReview: (run, payload) =>
      runServiceBackgroundReview(state, run, payload),
  };
}

function configuredScheduledJobPayload(
  state: ServiceState,
  jobId: string,
): unknown {
  return state.runtimeConfig.scheduledJobs.find((job) => job.id === jobId)
    ?.payload;
}

async function runServiceBackgroundReview(
  state: ServiceState,
  run: ScheduledRunSummary,
  payload: BackgroundReviewPayload,
): Promise<BackgroundReviewResult> {
  try {
    const now = state.now();
    const profileId = String(payload.profileId);
    const profileContext = await loadProfileContext({
      profilesDir: state.runtimeConfig.profilesDir,
      skillsDir: state.runtimeConfig.skillsDir,
      profileId: profileId as ProfileId,
    });
    const sessions = await state.bridge.listSessions().catch(() => []);
    const session =
      sessions.find((candidate) => candidate.profileId === profileId) ??
      configuredSessionForProfile(state.runtimeConfig, profileId);
    if (!session) {
      throw new Error(`no configured session found for profile ${profileId}`);
    }
    const denseProfileMemory =
      payload.includeDenseProfileMemory === false
        ? []
        : await state.bridge
            .listProfileMemory({
              profileId,
              limit: payload.maxCandidates ?? 100,
            })
            .catch(() => []);
    const role = buildProfileRoleAssembly(profileContext, {
      includeSkillBodies: false,
    });
    const toolDiagnostics = buildToolRegistryDiagnostics({
      catalogId: profileContext.toolSelection.catalogId,
      inventoryRequest: {
        requestedTools: profileContext.toolSelection.toolProfile.tools.map(
          (tool) => tool.name,
        ),
      },
    });
    const diagnostics = buildToolContextDiagnosticsReport({
      now,
      session: {
        sessionId: session.sessionId,
        agentId: session.agentId,
        profileId: session.profileId,
        kind: session.kind,
      },
      toolDiagnostics,
      toolSelection: profileContext.toolSelection,
      profileContext,
      toolPolicy: profileContext.profile.toolPolicy,
      roleAssembly: role.roleAssembly,
      systemPrompt: role.systemPrompt,
      resourceLimits: session.resourceLimits,
      adapters: buildServiceAdapterDiagnostics(state, now),
      memorySkillsPlanning: {
        denMemory: {
          configured: Boolean(state.config.denMemory.baseUrl),
          clientAvailable: Boolean(state.config.denMemory.baseUrl),
          mode: "metadata",
          endpointConfigured: Boolean(state.config.denMemory.baseUrl),
        },
        skills: {
          rootConfigured: Boolean(state.runtimeConfig.skillsDir),
          rootReadable: true,
          profileSkillCount: profileContext.profile.skills?.length ?? 0,
          loadedSkillCount: profileContext.skills.length,
          missingSkillCount: Math.max(
            0,
            (profileContext.profile.skills?.length ?? 0) -
              profileContext.skills.length,
          ),
          invalidSkillCount: 0,
        },
        denseProfileMemory: {
          clientAvailable: true,
          recordCount: denseProfileMemory.length,
        },
        sessionSearch: { available: true },
        todo: { available: true },
        counters: { available: true, resetAllowed: false },
      },
    });
    const result = await runBackgroundMemorySkillReview({
      runId: String(run.runId),
      now,
      payload,
      diagnostics,
      skills: profileContext.skills,
      denseProfileMemory: denseProfileMemory.map(toBackgroundMemoryRecord),
    });
    state.backgroundReview.lastRunAt = result.finishedAt;
    state.backgroundReview.lastError = undefined;
    state.backgroundReview.recentFindings = result.findingCount;
    recordServiceEvent(state, {
      source: "background-review",
      eventType: "memory_skills_review_completed",
      summary: `Background ${result.reviewType} review for ${result.profileId} produced ${result.findingCount} finding(s).`,
    });
    return result;
  } catch (error) {
    state.backgroundReview.lastError = errorMessage(
      error,
      "background review failed",
    );
    recordServiceEvent(state, {
      source: "background-review",
      eventType: "memory_skills_review_failed",
      summary: state.backgroundReview.lastError,
      severity: "warning",
    });
    throw error;
  }
}

async function runPostTurnMaintenance(input: {
  state: ServiceState;
  session: SessionState;
  profileContext: Awaited<ReturnType<typeof loadProfileContext>>;
  wakeId: string;
  source: ServiceWakeSource;
  observedEvents: readonly CoreEvent[];
  completionSummary?: string;
}): Promise<void> {
  const decision = postTurnMaintenanceDecision({
    profileId: input.session.profileId,
    wakeId: input.wakeId,
    source: input.source,
    backgroundReviewEnabled:
      input.profileContext.profile.backgroundReview?.enabled ?? false,
    events: input.observedEvents,
    completionSummary: input.completionSummary,
  });
  if (decision.action === "noop") {
    recordServiceEvent(input.state, {
      source: "post-turn-maintenance",
      eventType: "post_turn_auto_maintenance_noop",
      summary: `${decision.summary} for wake ${input.wakeId}.`,
    });
    return;
  }

  const batch = discoverCuratorCandidates({
    batchId: [
      "post-turn",
      input.session.profileId,
      input.wakeId.replace(/[^0-9A-Za-z_-]/g, ""),
    ].join(":"),
    now: input.state.now(),
    scopeType: "profile",
    scopeId: input.session.profileId,
    profileId: input.session.profileId,
    skills: input.profileContext.skills,
    expectedSkillSlugs:
      input.profileContext.profile.skillsMode === "all"
        ? []
        : input.profileContext.profile.skills,
    observedBehavior: [decision.evidence],
    maxCandidates: 1,
    dryRun: true,
  });
  input.state.curator.store.upsertBatch(
    batch,
    batch.candidates.flatMap((candidate) =>
      mutationForServiceCuratorCandidate(candidate),
    ),
  );
  input.state.curator.lastRunAt = input.state.now();
  recordServiceEvent(input.state, {
    source: "post-turn-maintenance",
    eventType:
      batch.candidateCount > 0
        ? "post_turn_curator_candidate_created"
        : "post_turn_auto_maintenance_noop",
    summary:
      batch.candidateCount > 0
        ? `Post-turn maintenance proposed ${batch.candidateCount} curator candidate(s) for wake ${input.wakeId}.`
        : `Post-turn maintenance observed reusable behavior for wake ${input.wakeId}, but no new candidate was needed.`,
  });
}

function configuredSessionForProfile(
  runtimeConfig: RustyCrewRuntimeConfig,
  profileId: string,
): RustyCrewRuntimeConfig["sessions"][number] | undefined {
  return runtimeConfig.sessions.find(
    (session) => session.profileId === profileId,
  );
}

function toBackgroundMemoryRecord(record: NativeProfileMemoryRecord) {
  return {
    profileId: record.profileId,
    key: record.key,
    content: record.content,
    revision: record.revision,
    updatedAt: record.updatedAt,
    metadata: parseJson(record.metadataJson),
  };
}

function parseJson(value: string): unknown {
  try {
    return JSON.parse(value) as unknown;
  } catch {
    return {};
  }
}

async function runSchedulerHeartbeat(state: ServiceState): Promise<void> {
  if (state.stopping) return;
  const tick = await state.bridge.runSchedulerTick();
  const hostRuns = await runScheduledHostExecutors({
    ...scheduledHostExecutorContext(state),
  });
  const scheduledJobs = await registerConfiguredScheduledJobs({
    bridge: state.bridge,
    runtimeConfig: state.runtimeConfig,
    now: state.now,
  });
  const curatorLifecycle = await runServiceCuratorLifecycleTransitions(state);
  const maintenance = await state.bridge.runMaintenance({
    expireQueuedMessagesAt: state.now(),
  });
  if (
    tick.wakesRequested > 0 ||
    tick.runsCompleted > 0 ||
    tick.runsFailed > 0 ||
    hostRuns.claimed > 0 ||
    scheduledJobs.registered > 0 ||
    curatorLifecycle.transitions.length > 0 ||
    maintenance.expiredQueueMessages > 0
  ) {
    recordServiceEvent(state, {
      source: "service-host",
      eventType: "scheduler_heartbeat",
      summary: `Scheduler heartbeat: ${tick.wakesRequested} wakes requested, ${tick.runsCompleted} wake runs completed, ${hostRuns.completed} host runs completed, ${scheduledJobs.registered} configured jobs reconciled, ${curatorLifecycle.transitions.length} curator lifecycle transitions, ${maintenance.expiredQueueMessages} queued messages expired.`,
    });
  }
}

async function runServiceCuratorLifecycleTransitions(
  state: ServiceState,
): Promise<CuratorLifecycleReport> {
  const report = await runCuratorLifecycleTransitions({
    store: state.curator.store,
    skillsDir: curatorSkillsDir(state.curator.runtimeConfig),
    now: state.now(),
  });
  state.curator.lastLifecycleRunAt = report.checkedAt;
  state.curator.lastLifecycleReport = report;
  return report;
}

async function drainAndDispatchWakes(
  state: ServiceState,
  source: ServiceWakeSource,
  observationContext?: ServiceWakeObservationContext,
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
    reports.push(await dispatchWake(state, event, source, observationContext));
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
  source: ServiceWakeSource,
  observationContext?: ServiceWakeObservationContext,
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
    const configured = configuredSessionForRuntimeSession(
      state.runtimeConfig,
      session,
    );
    const role = buildProfileRoleAssembly(profileContext);
    const turnTimeoutMs = effectiveTurnTimeoutMs(
      effectiveWakeTimeoutMs({
        session: configured,
        profile: profileContext.profile,
      }),
    );
    const observed = await withWakeTimeout(
      observeWakeEvents(
        state,
        sessionId,
        async () => {
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
        },
        (events) => appendCoreEventsToChatLog(state, session, events),
      ),
      {
        wakeId,
        sessionId,
        timeoutMs: turnTimeoutMs,
      },
    );
    await publishWakeToolActivity({
      state,
      session,
      wakeId,
      events: observed.events,
      observationContext,
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
    if (report.status === "completed") {
      ensureChatWakeTerminalEvents(state, session, wakeId, observed.events, {
        summary: completionSummary ?? report.summary,
      });
    }
    recordServiceEvent(state, {
      source: "service-host",
      eventType: "brain_wake_dispatched",
      severity: accepted.accepted ? undefined : "error",
      summary: `${report.summary} (${source}).`,
    });
    if (report.status === "completed") {
      await runPostTurnMaintenance({
        state,
        session,
        profileContext,
        wakeId,
        source,
        observedEvents: observed.events,
        completionSummary: report.summary,
      });
    }
    return report;
  } catch (error) {
    if (error instanceof WakeDispatchTimeoutError) {
      const report: ServiceWakeDispatchReport = {
        sessionId,
        wakeId: error.wakeId,
        status: "failed",
        summary: `wake ${error.wakeId} timed out after ${error.timeoutMs}ms`,
        reasonCode: "wake_timeout",
      };
      recordServiceEvent(state, {
        source: "service-host",
        eventType: "brain_wake_timeout",
        severity: "error",
        summary: `${report.summary} (${source}).`,
      });
      return report;
    }
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
  onEvents?: (events: readonly CoreEvent[]) => void,
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
    const events: CoreEvent[] = [];
    let callbackSettled = false;
    const callbackResult = callback()
      .then((value) => ({ ok: true as const, value }))
      .catch((error: unknown) => ({ ok: false as const, error }))
      .finally(() => {
        callbackSettled = true;
      });

    while (!callbackSettled) {
      await delay(25);
      const chunk = await drainSubscriptionEventsUntilIdle(
        state.bridge,
        subscription,
      );
      if (chunk.length > 0) {
        events.push(...chunk);
        onEvents?.(chunk);
      }
    }

    const result = await callbackResult;
    if (!result.ok) throw result.error;

    const finalEvents = await drainSubscriptionEventsUntilIdle(
      state.bridge,
      subscription,
    );
    if (finalEvents.length > 0) {
      events.push(...finalEvents);
      onEvents?.(finalEvents);
    }
    return { accepted: result.value, events };
  } finally {
    await state.bridge.unsubscribeEvents(subscription).catch(() => undefined);
  }
}

async function drainSubscriptionEventsUntilIdle(
  bridge: Pick<NativeBridgeModule, "drainSubscriptionEvents">,
  subscription: SubscriptionHandle,
): Promise<CoreEvent[]> {
  const chunkSize = 128;
  const maxEvents = 2_048;
  const events: CoreEvent[] = [];
  while (events.length < maxEvents) {
    const chunk = await bridge.drainSubscriptionEvents(subscription, chunkSize);
    events.push(...chunk);
    if (chunk.length < chunkSize) break;
  }
  return events;
}

async function delay(ms: number): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, ms));
}

async function publishWakeToolActivity(input: {
  state: ServiceState;
  session: SessionState;
  wakeId: string;
  events: readonly CoreEvent[];
  observationContext?: ServiceWakeObservationContext;
}): Promise<void> {
  if (input.state.denGatewayClient === undefined) return;
  const toolEvents = input.events.filter((event): event is ObservedToolEvent =>
    isObservedToolEvent(event, input.wakeId),
  );
  if (toolEvents.length === 0) return;

  const observer = createRuntimeActivityObserver({
    producer: new AgentActivityObservationProducer({
      sink: createDenGatewayObservationSink(input.state.denGatewayClient),
      required: true,
    }),
    identity: observationIdentityForSession(input.session),
    runtimeInstanceId: runtimeInstanceIdForSession(input.session),
  });
  const workRef = toolActivityWorkRef({
    sessionId: input.session.sessionId,
    wakeId: input.wakeId,
    observationContext: input.observationContext,
  });
  let degraded = 0;
  for (const event of toolEvents) {
    const toolEvent = event.event;
    const result = await observer.tool({
      eventType:
        toolEvent.type === "tool_call_started"
          ? "tool_call_started"
          : toolEvent.isError
            ? "tool_call_failed"
            : "tool_call_completed",
      toolName: toolEvent.toolName,
      adapter: "rusty-crew",
      visibility:
        input.observationContext?.channelId === undefined
          ? undefined
          : "channel",
      summary:
        toolEvent.type === "tool_call_started"
          ? `Tool ${toolEvent.toolName} started.`
          : toolEvent.isError
            ? `Tool ${toolEvent.toolName} failed.`
            : `Tool ${toolEvent.toolName} completed.`,
      longRunningOrRisky: true,
      workRef,
      resultRef:
        toolEvent.type === "tool_call_finished"
          ? {
              artifact_path: `runtime://tool/${toolEvent.toolName}/${input.wakeId}`,
            }
          : undefined,
      reasonCode:
        toolEvent.type === "tool_call_finished" && toolEvent.isError
          ? "tool_call_failed"
          : undefined,
    });
    if (result.status === "degraded") degraded += 1;
  }
  if (degraded > 0) {
    recordServiceEvent(input.state, {
      source: "den-successor-gateway",
      eventType: "den_observation_tool_activity_degraded",
      severity: "warning",
      summary: `Publishing ${degraded} tool Observation event(s) degraded for wake ${input.wakeId}.`,
    });
  }
}

type ObservedToolEvent = Extract<
  CoreEvent,
  { type: "brain_event_observed" }
> & {
  event: Extract<
    BrainEvent,
    { type: "tool_call_started" | "tool_call_finished" }
  >;
};

function isObservedToolEvent(
  event: CoreEvent,
  wakeId: string,
): event is ObservedToolEvent {
  return (
    event.type === "brain_event_observed" &&
    (event.wakeId === undefined || event.wakeId === wakeId) &&
    (event.event.type === "tool_call_started" ||
      event.event.type === "tool_call_finished")
  );
}

function createDenGatewayObservationSink(
  client: DenSuccessorGatewayClient,
): AgentActivityObservationSink {
  return {
    writeAgentActivity(event: AgentActivityObservationEvent): Promise<unknown> {
      return client.createObservationActivityEvent({
        source_domain: event.source_domain,
        event_type: event.event_type,
        agent_identity: event.agent_identity,
        runtime_instance_id: event.runtime_instance_id,
        payload: event.payload as unknown as Record<string, unknown>,
      });
    },
  };
}

function observationIdentityForSession(
  session: SessionState,
): DenSuccessorAgentIdentity {
  return {
    profile: session.profileId,
    instance_id: runtimeInstanceIdForSession(session),
    session_key: session.sessionId,
  };
}

function runtimeInstanceIdForSession(
  session: Pick<SessionState, "agentId">,
): string {
  return `${session.agentId}@rusty-crew`;
}

function toolActivityWorkRef(input: {
  sessionId: SessionId;
  wakeId: string;
  observationContext?: ServiceWakeObservationContext;
}): AgentActivityWorkRef {
  const deliveryIntentId = input.observationContext?.deliveryIntentId;
  return {
    session_id: input.sessionId,
    run_id:
      deliveryIntentId === undefined
        ? `wake:${input.wakeId}`
        : `delivery_intent:${deliveryIntentId};wake:${input.wakeId}`,
    channel_id: input.observationContext?.channelId,
    channel_message_id: input.observationContext?.channelMessageId,
  };
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

  const text = mergeTextParts(
    events.flatMap((event) =>
      event.type === "brain_event_observed" && event.event.type === "text_delta"
        ? [event.event.text]
        : [],
    ),
  ).trim();
  return text ? truncate(text, 480) : undefined;
}

function mergeTextParts(parts: readonly string[]): string {
  return parts
    .filter((part) => part.length > 0)
    .reduce((merged, part) => {
      if (!merged) return part;
      if (part.startsWith(merged)) return part;
      if (merged.endsWith(part)) return merged;
      return `${merged}${part}`;
    }, "");
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
    await stopTelegramConnector(state);
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
  response: ServerResponse,
  result: ServiceRouteResult,
): void {
  if (isRawServiceRouteResult(result)) {
    result.write(response);
    return;
  }
  for (const [name, value] of Object.entries(result.headers)) {
    response.setHeader(name, value);
  }
  response.statusCode = result.status;
  response.end(
    typeof result.body === "string" ? result.body : JSON.stringify(result.body),
  );
}

function isChatRoute(pathname: string): boolean {
  return pathname === "/v1/chat" || pathname.startsWith("/v1/chat/");
}

function chatCorsPreflightResponse(
  request: IncomingMessage,
): ServiceRouteResult {
  return {
    status: 204,
    headers: chatCorsHeaders(request),
    body: "",
  };
}

function withChatCors<T extends ServiceRouteResult>(
  result: T,
  request: IncomingMessage,
): T {
  if (isRawServiceRouteResult(result)) return result;
  return {
    ...result,
    headers: {
      ...result.headers,
      ...chatCorsHeaders(request),
    },
  };
}

function chatCorsHeaders(request: IncomingMessage): Record<string, string> {
  const origin = stringHeader(request, "origin") ?? "*";
  return {
    "access-control-allow-origin": origin,
    "access-control-allow-methods": "GET,POST,OPTIONS",
    "access-control-allow-headers":
      "authorization,content-type,idempotency-key,last-event-id,x-request-id",
    "access-control-expose-headers": "content-type",
    "access-control-max-age": "600",
    vary: origin === "*" ? "Origin" : "Origin",
  };
}

function isRawServiceRouteResult(
  result: ServiceRouteResult,
): result is RawServiceRouteResult {
  return "kind" in result && result.kind === "raw";
}

function isAdminPanelRoute(pathname: string, staticEnabled: boolean): boolean {
  return (
    pathname === "/admin" ||
    pathname === "/admin/" ||
    (!staticEnabled && pathname === "/")
  );
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

function staticServingEnabled(state: ServiceState): boolean {
  const root = effectiveStaticSiteRoot(state);
  return root !== undefined && existsSync(root);
}

async function handleStaticSiteRequest(
  request: IncomingMessage,
  url: URL,
  state: ServiceState,
): Promise<ServiceRouteResult> {
  if ((request.method ?? "GET").toUpperCase() !== "GET") {
    return failure(405, requestId(request), {
      code: "method_not_allowed",
      reason_code: "static_method_not_allowed",
      message: "static files only support GET",
      retryable: false,
    });
  }
  const root = effectiveStaticSiteRoot(state);
  if (root === undefined) {
    return failure(404, requestId(request), {
      code: "not_found",
      reason_code: "static_site_disabled",
      message: "static site serving is not configured",
      retryable: false,
    });
  }
  const candidate = resolveStaticSitePath(root, url.pathname);
  if (!candidate.ok) {
    return failure(403, requestId(request), {
      code: "forbidden",
      reason_code: candidate.reasonCode,
      message: candidate.message,
      retryable: false,
    });
  }

  const filePath = await existingStaticFile(candidate.path);
  if (filePath !== undefined) return staticFileResponse(root, filePath);

  const indexPath = resolve(root, "index.html");
  if (await isReadableFile(indexPath))
    return staticFileResponse(root, indexPath);

  return failure(404, requestId(request), {
    code: "not_found",
    reason_code: "static_index_missing",
    message: `static site index.html was not found in ${root}`,
    retryable: false,
  });
}

function effectiveStaticSiteRoot(state: ServiceState): string | undefined {
  return (
    state.config.paths.staticDir ?? join(state.config.paths.dataDir, "site")
  );
}

function resolveStaticSitePath(
  root: string,
  pathname: string,
):
  | { ok: true; path: string }
  | { ok: false; reasonCode: string; message: string } {
  let decodedSegments: string[];
  try {
    decodedSegments = pathname
      .split("/")
      .filter(Boolean)
      .map((segment) => decodeURIComponent(segment));
  } catch {
    return {
      ok: false,
      reasonCode: "static_path_invalid",
      message: "static path contains invalid percent encoding",
    };
  }

  if (
    decodedSegments.some(
      (segment) =>
        segment === "." || segment === ".." || segment.startsWith("."),
    )
  ) {
    return {
      ok: false,
      reasonCode: "static_path_forbidden",
      message: "static path contains a forbidden segment",
    };
  }

  const resolvedRoot = resolve(root);
  const resolvedPath =
    decodedSegments.length === 0
      ? resolve(resolvedRoot, "index.html")
      : resolve(resolvedRoot, ...decodedSegments);
  const relativePath = relative(resolvedRoot, resolvedPath);
  if (
    relativePath === ".." ||
    relativePath.startsWith(`..${sep}`) ||
    resolve(resolvedPath) === resolvedRoot
  ) {
    return {
      ok: false,
      reasonCode: "static_path_traversal",
      message: "static path escapes the configured static directory",
    };
  }
  return { ok: true, path: resolvedPath };
}

async function existingStaticFile(path: string): Promise<string | undefined> {
  if (await isReadableFile(path)) return path;
  const indexPath = resolve(path, "index.html");
  return (await isReadableFile(indexPath)) ? indexPath : undefined;
}

async function isReadableFile(path: string): Promise<boolean> {
  try {
    return (await stat(path)).isFile();
  } catch {
    return false;
  }
}

function staticFileResponse(
  root: string,
  filePath: string,
): RawServiceRouteResult {
  return {
    kind: "raw",
    write(response) {
      response.statusCode = 200;
      response.setHeader("content-type", staticContentType(filePath));
      response.setHeader("cache-control", staticCacheControl(root, filePath));
      createReadStream(filePath).pipe(response);
    },
  };
}

function staticContentType(path: string): string {
  switch (extname(path).toLowerCase()) {
    case ".html":
      return "text/html; charset=utf-8";
    case ".js":
      return "application/javascript; charset=utf-8";
    case ".css":
      return "text/css; charset=utf-8";
    case ".json":
      return "application/json; charset=utf-8";
    case ".svg":
      return "image/svg+xml";
    case ".ico":
      return "image/x-icon";
    case ".png":
      return "image/png";
    case ".woff2":
      return "font/woff2";
    default:
      return "application/octet-stream";
  }
}

function staticCacheControl(root: string, filePath: string): string {
  const relativePath = relative(root, filePath);
  if (relativePath === "index.html" || basename(filePath) === "index.html") {
    return "no-cache";
  }
  return /-[a-z0-9]{16,}\./i.test(basename(filePath))
    ? "public, max-age=31536000, immutable"
    : "no-cache";
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

function successRoute<T>(requestIdValue: string, data: T): AdminRouteResult<T> {
  return {
    status: 200,
    headers: { "content-type": "application/json" },
    body: {
      ok: true,
      data,
      meta: { request_id: requestIdValue, schema_version: 1 },
    },
  };
}

function invalidSchedulerFilter(
  requestIdValue: string,
  key: string,
): AdminRouteResult {
  return failure(400, requestIdValue, {
    code: "invalid_input",
    reason_code: "invalid_scheduler_filter",
    message: `invalid scheduler ${key} filter`,
    retryable: false,
  });
}

function scheduledJobStatusParam(
  value: string | null,
): ScheduledJobStatus | "invalid" | undefined {
  if (value === null || value.trim() === "") return undefined;
  return value === "active" || value === "paused" || value === "archived"
    ? value
    : "invalid";
}

function scheduledRunStatusParam(
  value: string | null,
): ScheduledRunStatus | "invalid" | undefined {
  if (value === null || value.trim() === "") return undefined;
  return value === "claimed" ||
    value === "completed" ||
    value === "skipped" ||
    value === "failed" ||
    value === "expired" ||
    value === "cancelled"
    ? value
    : "invalid";
}

function scheduledRunTriggerParam(
  value: string | null,
): ScheduledRunTrigger | "invalid" | undefined {
  if (value === null || value.trim() === "") return undefined;
  return value === "due" || value === "manual" ? value : "invalid";
}

function stringParam(url: URL, key: string): string | undefined {
  const value = url.searchParams.get(key);
  return value === null || value.trim() === "" ? undefined : value;
}

function pageParams(url: URL): { limit?: number; offset?: number } {
  const limit = optionalInteger(url.searchParams.get("limit"));
  const offset = optionalInteger(url.searchParams.get("offset"));
  return {
    ...(limit === undefined ? {} : { limit }),
    ...(offset === undefined ? {} : { offset }),
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

function optionalRecord(value: unknown): Record<string, unknown> | undefined {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : undefined;
}

function compactRecord(
  value: Record<string, unknown>,
): Record<string, unknown> {
  return Object.fromEntries(
    Object.entries(value).filter(
      ([, entry]) => entry !== null && entry !== undefined,
    ),
  );
}

function requestId(request: IncomingMessage): string {
  const value = request.headers["x-request-id"];
  return typeof value === "string" && value.trim()
    ? value.trim()
    : `req_${Date.now()}`;
}

function stringHeader(
  request: IncomingMessage,
  name: string,
): string | undefined {
  const value = request.headers[name.toLowerCase()];
  if (Array.isArray(value)) return value.find((candidate) => candidate.trim());
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
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
