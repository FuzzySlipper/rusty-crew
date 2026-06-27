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
  type NativeCreateProfilePlan,
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
  type MemorySpaceDiagnosticsProjection,
  type AdminRouteResult,
} from "./admin-diagnostics-api.js";
import { handleMemorySpaceAdminRequest } from "./memory-space-api.js";
import { handleStorageQueryRequest } from "./storage-query-catalog.js";
import { buildAdminProfileRegistryDiagnostics } from "./profile-registry-admin.js";
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
import {
  loadProfileConfig,
  loadProfileContext,
  parseProfileConfigDraft,
  type ProfileConfig,
  type SessionMemoryPromptConfig,
} from "./profile-loading.js";
import {
  buildProfileRoleAssembly,
  renderSessionMemoryContext,
} from "./profile-role-assembly.js";
import {
  planCreateProfileWithRust,
  planRuntimeConfigWithRust,
} from "./runtime-config-validation.js";
import {
  buildRuntimeDiagnosticsProjection,
  type RuntimeSessionEffectiveDefaults,
  type RuntimePauseDiagnostics,
  type StorageDiagnosticsProjection,
} from "./runtime-diagnostics.js";
import {
  handleRustyViewChatRequest,
  cursorSequence,
  type AttachmentMutationResult,
  type AttachmentPage,
  type AttachmentRecord,
  type ChatEvent,
  type ChatSendMessageInput,
  type ConversationBranchMutationResult,
  type ConversationBranchRecord,
  type ConversationBranchStateInput,
  type ConversationBranchStateRecord,
  type ConversationJumpResult,
  type ConversationSnapshotMutationResult,
  type ConversationSnapshotRecord,
  type ConversationTreeInput,
  type ConversationTreeProjection,
  type CreateAttachmentInput,
  type CreateConversationBranchInput,
  type CreateConversationSnapshotInput,
  type CreateDataBankScopeInput,
  type DataBankScopeMutationResult,
  type DataBankScopePage,
  type DataBankScopeRecord,
  type CreateMessageSlotInput,
  type CreateMessageVariantInput,
  type DeleteMessageVariantInput,
  type ExecuteChatCommandInput,
  type ExecuteChatCommandResult,
  type ListAttachmentsInput,
  type ListDataBankScopesInput,
  type ListMessageSlotsInput,
  type ListMessageVariantsInput,
  type MessageBlockDraft,
  type MessageSlotMutationResult,
  type MessageSlotPage,
  type MessageSlotRecord,
  type MessageVariantMutationResult,
  type MessageVariantPage,
  type MessageVariantRecord,
  type MessageVariantsReorderResult,
  type ReorderMessageVariantsInput,
  type RemoveAttachmentInput,
  type RemoveDataBankScopeInput,
  type SelectActiveMessageVariantInput,
  type SelectActiveMessageVariantResult,
  type SelectActiveConversationBranchInput,
  type SelectActiveConversationBranchResult,
  type SendChatMessageResult,
  type ResolveConversationJumpInput,
  type SearchTranscriptInput,
  type TranscriptSearchResult,
  type TranscriptSearchResultPage,
  type UpdateConversationBranchHeadInput,
  type UpdateConversationBranchHeadResult,
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
  type RustyCrewStorageConfig,
} from "./service-config.js";
import {
  applyRustyCrewRuntimeConfig,
  effectiveSessionDefaults,
  effectiveWakeTimeoutMs,
  loadRustyCrewRuntimeConfig,
  preflightRustyCrewRuntimeConfig,
  rebuildConfiguredBrainRuntime,
  registerConfiguredScheduledJobs,
  ensureConfiguredSessionForChannelBinding,
  type RustyCrewConfiguredSession,
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
  readonly runtimePauses: Map<string, RuntimePauseRecord>;
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

type RuntimePauseScope = "session" | "profile" | "agent";

interface RuntimePauseRecord {
  pauseId: string;
  scope: RuntimePauseScope;
  targetId: string;
  pausedBy: string;
  pausedAt: string;
  reason?: string;
  reasonCode?: string;
  affectedSessionIds: string[];
  inFlightWakeCount: number;
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
      runtimePauses: new Map(),
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
      await buildDiagnosticsContext(state, {
        includeProfileRegistry: isProfileRegistryAdminRoute(url.pathname),
      }),
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
        listMessageSlots: (input) => listRustyViewMessageSlots(state, input),
        searchTranscript: (input) => searchRustyViewTranscript(state, input),
        listMessageVariants: (input) =>
          listRustyViewMessageVariants(state, input),
        createMessageSlot: (input) => createRustyViewMessageSlot(state, input),
        createMessageVariant: (input) =>
          createRustyViewMessageVariant(state, input),
        deleteMessageVariant: (input) =>
          deleteRustyViewMessageVariant(state, input),
        reorderMessageVariants: (input) =>
          reorderRustyViewMessageVariants(state, input),
        selectActiveMessageVariant: (input) =>
          selectRustyViewActiveMessageVariant(state, input),
        conversationTree: (input) => rustyViewConversationTree(state, input),
        createConversationBranch: (input) =>
          createRustyViewConversationBranch(state, input),
        getConversationBranchState: (input) =>
          getRustyViewConversationBranchState(state, input),
        selectActiveConversationBranch: (input) =>
          selectRustyViewActiveConversationBranch(state, input),
        updateConversationBranchHead: (input) =>
          updateRustyViewConversationBranchHead(state, input),
        createConversationSnapshot: (input) =>
          createRustyViewConversationSnapshot(state, input),
        resolveConversationJump: (input) =>
          resolveRustyViewConversationJump(state, input),
        createAttachment: (input) => createRustyViewAttachment(state, input),
        listAttachments: (input) => listRustyViewAttachments(state, input),
        removeAttachment: (input) => removeRustyViewAttachment(state, input),
        createDataBankScope: (input) =>
          createRustyViewDataBankScope(state, input),
        listDataBankScopes: (input) =>
          listRustyViewDataBankScopes(state, input),
        removeDataBankScope: (input) =>
          removeRustyViewDataBankScope(state, input),
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

  if (url.pathname.startsWith("/v1/admin/storage/")) {
    const body =
      (request.method ?? "GET").toUpperCase() === "POST"
        ? await readJsonBody(request)
        : undefined;
    return handleStorageQueryRequest(
      {
        method: request.method ?? "GET",
        url: url.toString(),
        body,
        requestId: requestId(request),
      },
      { bridge: state.bridge },
    );
  }

  if (url.pathname.startsWith("/v1/admin/memory/")) {
    const body =
      (request.method ?? "GET").toUpperCase() === "POST"
        ? await readJsonBody(request)
        : undefined;
    return handleMemorySpaceAdminRequest(
      {
        method: request.method ?? "GET",
        url: url.toString(),
        body,
        requestId: requestId(request),
      },
      { bridge: state.bridge },
    );
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
  options: { includeProfileRegistry?: boolean } = {},
): Promise<AdminDiagnosticsContext> {
  const now = state.now();
  const [runtimeSummary, sessions, storage, providerStates, memorySpaces] =
    await Promise.all([
      state.bridge
        .runtimeSummary({ scopeType: "runtime" })
        .catch(() => undefined),
      state.bridge.listSessions().catch(() => []),
      state.bridge
        .storageDiagnostics()
        .then((diagnostics) =>
          storageDiagnosticsProjection(
            diagnostics,
            state.runtimeConfig.storage ?? state.config.storage,
          ),
        )
        .catch(() => undefined),
      state.bridge.providerStateDiagnostics().catch(() => []),
      buildMemorySpaceDiagnostics(state).catch(() => undefined),
    ]);
  const profileRegistry = options.includeProfileRegistry
    ? await buildAdminProfileRegistryDiagnostics({
        bridge: state.bridge,
        runtimeConfig: state.runtimeConfig,
        now,
      }).catch(() => undefined)
    : undefined;
  const sessionDefaults = await effectiveSessionDefaultsById(state, sessions);
  const diagnostics = buildRuntimeDiagnosticsProjection({
    now,
    runtimeSummary,
    sessions,
    sessionDefaults,
    delegatedSessions: [],
    brainModules: brainModuleDiagnostics(state),
    providerStates,
    adapters: buildServiceAdapterDiagnostics(state, now),
    persistence: {
      tableCounts: tableCountMap(storage),
      searchHealthy: storage?.searchHealthy ?? true,
      databaseBytes: storage?.size.databaseBytes,
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
    runtimePauses: runtimePauseDiagnostics(state, sessions),
  });
  return {
    diagnostics,
    storage,
    memorySpaces,
    profileRegistry,
    configValidation: await preflightRustyCrewRuntimeConfig({
      serviceConfig: state.config,
      bridge: state.bridge,
    }),
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

function isProfileRegistryAdminRoute(pathname: string): boolean {
  return (
    pathname === "/v1/admin/diagnostics/profiles" ||
    pathname === "/v1/admin/profiles/registry" ||
    pathname.startsWith("/v1/admin/profiles/registry/")
  );
}

async function buildMemorySpaceDiagnostics(
  state: ServiceState,
): Promise<MemorySpaceDiagnosticsProjection> {
  const descriptors = await state.bridge.listMemorySpaceDescriptors();
  const defaultCaps = {
    maxRecordsPerProfile: 64,
    maxKeyBytes: 128,
    maxContentBytes: 8 * 1024,
  };
  return {
    generatedAt: state.now(),
    items: descriptors.map((descriptor) => ({
      descriptor,
      compatibility:
        descriptor.space_id === "profile_dense"
          ? {
              spaceId: descriptor.space_id,
              status: "compatible",
              backingStore: "profile_memories",
              nativeMethods: [
                "listProfileMemory",
                "getProfileMemory",
                "addProfileMemory",
                "replaceProfileMemory",
                "removeProfileMemory",
              ],
              denseProfileMemoryCaps: defaultCaps,
              conflictBehavior: "expected_revision",
              promptInjectionBehavior:
                "renderDenseProfileMemoryContext injects dense records into profile role assembly when enabled",
              toolModeBehavior:
                "dense_profile_memory runs read_write only when profile tool selection includes the tool; otherwise read_only",
              notes: [
                "Descriptor projects the existing dense profile memory API without rewriting storage.",
                "Crew profile_dense memory is runtime-owned and distinct from Den memory.",
              ],
            }
          : descriptor.space_id === "session_memory"
            ? {
                spaceId: descriptor.space_id,
                status: "compatible",
                backingStore: "session_memory_records",
                nativeMethods: [
                  "querySessionMemoryRecords",
                  "buildSessionMemoryPromptContext",
                ],
                conflictBehavior: "expected_revision",
                promptInjectionBehavior:
                  "Rust buildSessionMemoryPromptContext selects bounded branch-aware records; TypeScript only renders the returned context when profile/session config enables it.",
                toolModeBehavior:
                  "session_memory is readable through memory-space tools and prompt assembly; write/governance apply paths are intentionally separate.",
                notes: [
                  "Active branch and ancestor records are eligible by default.",
                  "Sibling branch records are excluded from prompt context unless explicitly requested.",
                ],
              }
            : {
                spaceId: descriptor.space_id,
                status: "degraded",
                backingStore: "unknown",
                nativeMethods: [],
                conflictBehavior: "unknown",
                promptInjectionBehavior: "unknown",
                toolModeBehavior: "unknown",
                notes: [
                  "No compatibility projection is registered for this space.",
                ],
              },
    })),
  };
}

function tableCountMap(
  storage: StorageDiagnosticsProjection | undefined,
): Record<string, number> {
  return Object.fromEntries(
    (storage?.tableCounts ?? []).map((count) => [count.table, count.rows]),
  );
}

function storageDiagnosticsProjection(
  storage: StorageDiagnosticsProjection,
  config: RustyCrewStorageConfig,
): StorageDiagnosticsProjection {
  return {
    ...storage,
    configuredBackend: config.backend,
    implementationStatus: config.implementationStatus,
    sqlite: {
      path: config.sqlite.path,
      effectivePath: config.sqlite.effectivePath,
      wal: config.sqlite.wal,
      busyTimeoutMs: config.sqlite.busyTimeoutMs,
      deploymentClass: "embedded_local",
      singleServiceWriter: true,
    },
    postgres: {
      databaseUrlEnv: config.postgres.databaseUrlEnv,
      schema: config.postgres.schema,
      maxConnections: config.postgres.maxConnections,
      statementTimeoutMs: config.postgres.statementTimeoutMs,
      implementationStatus: "placeholder_unimplemented",
    },
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

async function buildSessionMemoryContextForWake(
  state: ServiceState,
  input: {
    session: Pick<SessionState, "sessionId" | "profileId">;
    configuredSession?: Pick<RustyCrewConfiguredSession, "sessionMemoryPrompt">;
    profileContext: Awaited<ReturnType<typeof loadProfileContext>>;
  },
): Promise<string | undefined> {
  const config = effectiveSessionMemoryPromptConfig(
    input.profileContext.profile.memoryConfig,
    input.configuredSession?.sessionMemoryPrompt,
  );
  if (!config.enabled) {
    return undefined;
  }
  let activeBranchId: string | null = null;
  try {
    const branchState = (await state.bridge.getConversationBranchState({
      session_id: input.session.sessionId,
      default_updated_at: state.now(),
    })) as ConversationBranchStateRecord;
    activeBranchId = branchState.active_branch_id ?? null;
  } catch (error) {
    recordServiceEvent(state, {
      source: "session_memory_prompt",
      eventType: "session_memory_prompt_branch_state_degraded",
      severity: "warning",
      summary: `session memory prompt for ${input.session.sessionId} could not read active branch: ${errorMessage(error, "unknown branch-state error")}`,
    });
  }
  try {
    const context = await state.bridge.buildSessionMemoryPromptContext({
      session_id: input.session.sessionId,
      active_branch_id: activeBranchId,
      include_ancestors: config.includeAncestors ?? true,
      include_siblings: config.includeSiblings ?? false,
      prompt_context_only: true,
      page: {
        limit: boundedSessionMemoryPromptLimit(config.maxRecords),
        offset: 0,
      },
    });
    return renderSessionMemoryContext(context);
  } catch (error) {
    recordServiceEvent(state, {
      source: "session_memory_prompt",
      eventType: "session_memory_prompt_context_degraded",
      severity: "warning",
      summary: `session memory prompt for ${input.session.sessionId} could not build context: ${errorMessage(error, "unknown prompt-context error")}`,
    });
    return undefined;
  }
}

function effectiveSessionMemoryPromptConfig(
  profileMemory: ProfileConfig["memoryConfig"] | undefined,
  sessionPrompt: SessionMemoryPromptConfig | undefined,
): Required<Pick<SessionMemoryPromptConfig, "enabled">> &
  Omit<SessionMemoryPromptConfig, "enabled"> {
  const profilePrompt = profileMemory?.sessionMemoryPrompt;
  const profileEnabled =
    profilePrompt?.enabled ??
    profileMemory?.sessionMemory ??
    (profileMemory?.enabled === true && profileMemory.sessionMemory !== false);
  return {
    enabled: sessionPrompt?.enabled ?? profileEnabled ?? false,
    maxRecords: sessionPrompt?.maxRecords ?? profilePrompt?.maxRecords,
    includeAncestors:
      sessionPrompt?.includeAncestors ?? profilePrompt?.includeAncestors,
    includeSiblings:
      sessionPrompt?.includeSiblings ?? profilePrompt?.includeSiblings,
  };
}

function boundedSessionMemoryPromptLimit(value: number | undefined): number {
  if (value === undefined || !Number.isFinite(value)) {
    return 12;
  }
  return Math.max(1, Math.min(32, Math.floor(value)));
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
  registryWrite?: NativeCreateProfilePlan["registryWrite"];
  fileAssetActions: NativeCreateProfilePlan["fileAssetActions"];
  derivedRuntimeActions: NativeCreateProfilePlan["derivedRuntimeActions"];
  applyResult: RustyCrewRuntimeConfigApplyResult;
}

interface DecommissionedServiceProfile {
  profileId: string;
  runtimeConfigPath: string;
  profilePath?: string;
  profileDirectoryPreserved: true;
  sessionsArchived: string[];
  removed: {
    brains: number;
    sessions: number;
    channelBindings: number;
    mcpBindings: number;
    scheduledJobs: number;
  };
  brainHandle: {
    action: "removed" | "already_absent";
    handle?: BrainImplementationHandle;
  };
  skipped: {
    profileDirectory: "preserved";
  };
  applyResult: RustyCrewRuntimeConfigApplyResult;
}

interface ProfileUpdatePlan {
  profileId: string;
  ok: boolean;
  profilePath: string;
  diagnostics: Array<{
    severity: "error" | "warning" | "info";
    code: string;
    path: string;
    message: string;
  }>;
  implications: {
    configReloadRequired: true;
    mcpRefreshRecommended: boolean;
    runtimeRebuildRecommended: boolean;
    profileDirectoryFiles: "json_profile_only";
  };
  runtimePlan?: unknown;
}

interface RuntimeConfigDraftPlan {
  ok: boolean;
  configPath: string;
  diagnostics: Array<{
    severity: "error" | "warning" | "info";
    code: string;
    path: string;
    message: string;
  }>;
  implications: {
    configReloadRequired: true;
    createMissingSessions: false;
    explicitChannelLifecycle: true;
    explicitSessionLifecycle: true;
  };
  runtimePlan?: unknown;
}

async function readServiceProfileConfig(
  state: ServiceState,
  command: AdminControlCommand,
): Promise<Record<string, unknown>> {
  const profileId = command.target.profileId;
  if (!profileId) throw new Error("profile id is required");
  const profilePath = safeProfileConfigPath(
    state.runtimeConfig.profilesDir,
    profileId,
  );
  if (profilePath === undefined) {
    throw new Error(`profile id ${profileId} is not a valid file profile id`);
  }
  const raw = JSON.parse(await readFile(profilePath, "utf8")) as unknown;
  if (!isRecord(raw)) {
    throw new Error(`profile ${profileId} config root must be an object`);
  }
  const loaded = await loadProfileConfig(
    state.runtimeConfig.profilesDir,
    profileId as ProfileId,
  );
  return {
    profileId,
    profilePath,
    profileConfig: raw,
    loaded,
    editable: {
      format: "json_profile",
      supportsSoulMarkdown: true,
      supportsMemoryMarkdown: true,
    },
  };
}

async function planServiceProfileUpdate(
  state: ServiceState,
  command: AdminControlCommand,
): Promise<ProfileUpdatePlan> {
  const profileId = command.target.profileId;
  if (!profileId) throw new Error("profile id is required");
  const profilePath = safeProfileConfigPath(
    state.runtimeConfig.profilesDir,
    profileId,
  );
  if (profilePath === undefined) {
    throw new Error(`profile id ${profileId} is not a valid file profile id`);
  }
  const draft = profileConfigDraftFromCommand(command, profileId);
  const diagnostics: ProfileUpdatePlan["diagnostics"] = [];
  let parsedDraft: ProfileConfig | undefined;
  try {
    parsedDraft = parseProfileConfigDraft({
      profilesDir: state.runtimeConfig.profilesDir,
      profileId: profileId as ProfileId,
      profileConfig: draft,
      soulMarkdown: optionalBodyString(command, "soulMarkdown"),
      memoryMarkdown: optionalBodyString(command, "memoryMarkdown"),
    });
  } catch (error) {
    diagnostics.push({
      severity: "error",
      code: "invalid_profile_config",
      path: `profiles.${profileId}`,
      message: errorMessage(error, "profile draft is invalid"),
    });
  }

  const currentProfile = await loadProfileConfig(
    state.runtimeConfig.profilesDir,
    profileId as ProfileId,
  ).catch(() => undefined);
  let runtimePlan: unknown;
  if (parsedDraft !== undefined) {
    const profiles = await loadRuntimeConfigProfilesReplacing(
      state,
      profileId,
      parsedDraft,
    );
    const plan = await planRuntimeConfigWithRust({
      bridge: state.bridge,
      runtimeConfig: state.runtimeConfig,
      profiles,
    });
    runtimePlan = plan;
    for (const diagnostic of plan.diagnostics) {
      diagnostics.push({
        severity: diagnostic.severity,
        code: diagnostic.code,
        path: diagnostic.path ?? "runtimeConfig",
        message: diagnostic.message,
      });
    }
  }

  return {
    profileId,
    ok: !diagnostics.some((diagnostic) => diagnostic.severity === "error"),
    profilePath,
    diagnostics,
    implications: {
      configReloadRequired: true,
      mcpRefreshRecommended: profileMcpChanged(currentProfile, parsedDraft),
      runtimeRebuildRecommended: profileRuntimeBrainChanged(
        currentProfile,
        parsedDraft,
      ),
      profileDirectoryFiles: "json_profile_only",
    },
    runtimePlan,
  };
}

async function applyServiceProfileUpdate(
  state: ServiceState,
  command: AdminControlCommand,
): Promise<
  ProfileUpdatePlan & { applyResult?: RustyCrewRuntimeConfigApplyResult }
> {
  const plan = await planServiceProfileUpdate(state, command);
  if (!plan.ok) return plan;
  const draft = profileConfigDraftFromCommand(command, plan.profileId);
  await writeJsonFileAtomic(plan.profilePath, draft);
  const applyResult = await applyServiceRuntimeConfigFromDisk(state, {
    createMissingSessions: false,
    eventType: "profile_config_updated",
    summaryPrefix: `Profile ${plan.profileId} updated`,
  });
  return { ...plan, applyResult };
}

async function planServiceRuntimeConfigDraft(
  state: ServiceState,
  command: AdminControlCommand,
): Promise<RuntimeConfigDraftPlan> {
  const runtimeConfig = runtimeConfigDraftFromCommand(state, command);
  const loaded = await loadRuntimeConfigProfilesForDraft(runtimeConfig);
  const diagnostics: RuntimeConfigDraftPlan["diagnostics"] =
    loaded.diagnostics.map((diagnostic) => ({
      severity: diagnostic.severity,
      code: diagnostic.code,
      path: diagnostic.path,
      message: diagnostic.message,
    }));
  let runtimePlan: unknown;
  if (!diagnostics.some((diagnostic) => diagnostic.severity === "error")) {
    const plan = await planRuntimeConfigWithRust({
      bridge: state.bridge,
      runtimeConfig,
      profiles: loaded.profiles,
    });
    runtimePlan = plan;
    for (const diagnostic of plan.diagnostics) {
      diagnostics.push({
        severity: diagnostic.severity,
        code: diagnostic.code,
        path: diagnostic.path ?? "runtimeConfig",
        message: diagnostic.message,
      });
    }
  }
  return {
    ok: !diagnostics.some((diagnostic) => diagnostic.severity === "error"),
    configPath: state.config.paths.serviceConfigFile,
    diagnostics,
    implications: {
      configReloadRequired: true,
      createMissingSessions: false,
      explicitChannelLifecycle: true,
      explicitSessionLifecycle: true,
    },
    runtimePlan,
  };
}

async function applyServiceRuntimeConfigDraft(
  state: ServiceState,
  command: AdminControlCommand,
): Promise<
  RuntimeConfigDraftPlan & { applyResult?: RustyCrewRuntimeConfigApplyResult }
> {
  const plan = await planServiceRuntimeConfigDraft(state, command);
  if (!plan.ok) return plan;
  const runtimeConfig = runtimeConfigDraftFromCommand(state, command);
  await writeJsonFileAtomic(
    state.config.paths.serviceConfigFile,
    runtimeConfig,
  );
  const applyResult = await applyServiceRuntimeConfigFromDisk(state, {
    createMissingSessions: false,
    eventType: "runtime_config_draft_applied",
    summaryPrefix: "Runtime config draft applied",
  });
  return { ...plan, applyResult };
}

async function decommissionServiceProfile(
  state: ServiceState,
  command: AdminControlCommand,
): Promise<DecommissionedServiceProfile> {
  const profileId = command.target.profileId;
  if (!profileId) throw new Error("profile id is required");
  if (optionalBodyBoolean(command, "deleteProfileDirectory") === true) {
    throw new Error(
      "deleteProfileDirectory is not supported by profile decommission; profile files are preserved",
    );
  }

  const configuredSessionIds = state.runtimeConfig.sessions
    .filter((session) => String(session.profileId) === profileId)
    .map((session) => String(session.sessionId));
  const activeSessions = await state.bridge.listSessions();
  const activeSessionIds = activeSessions
    .filter((session) => String(session.profileId) === profileId)
    .map((session) => String(session.sessionId));
  const sessionIds = [
    ...new Set([...configuredSessionIds, ...activeSessionIds]),
  ];
  const inFlightSessionIds = sessionIds.filter((sessionId) =>
    state.inFlightWakes.has(sessionId as SessionId),
  );
  if (inFlightSessionIds.length > 0) {
    throw new Error(
      `profile ${profileId} decommission blocked by in-flight wake(s): ${inFlightSessionIds.join(", ")}`,
    );
  }

  const sessionsArchived: string[] = [];
  for (const session of activeSessions) {
    if (
      String(session.profileId) !== profileId ||
      session.status === "archived"
    ) {
      continue;
    }
    await state.bridge.archiveSession(session.sessionId);
    sessionsArchived.push(String(session.sessionId));
  }

  const runtimeConfigFile = await readRuntimeConfigFileForMutation(state);
  const removed = {
    brains: removeRuntimeConfigEntries(
      runtimeConfigFile.array("brains"),
      (entry) =>
        runtimeEntryString(entry, "profileId", "profile_id") === profileId,
    ),
    sessions: removeRuntimeConfigEntries(
      runtimeConfigFile.array("sessions"),
      (entry) =>
        runtimeEntryString(entry, "profileId", "profile_id") === profileId,
    ),
    channelBindings: removeRuntimeConfigEntries(
      runtimeConfigFile.array("channelBindings"),
      (entry) =>
        runtimeEntryString(entry, "profileId", "profile_id") === profileId ||
        sessionIds.includes(
          runtimeEntryString(entry, "sessionId", "session_id") ?? "",
        ),
    ),
    mcpBindings: removeRuntimeConfigEntries(
      runtimeConfigFile.array("mcpBindings"),
      (entry) =>
        runtimeEntryString(entry, "profileId", "profile_id") === profileId ||
        sessionIds.includes(
          runtimeEntryString(entry, "sessionId", "session_id") ?? "",
        ),
    ),
    scheduledJobs: removeRuntimeConfigEntries(
      runtimeConfigFile.array("scheduledJobs"),
      (entry) =>
        sessionIds.includes(
          runtimeEntryString(entry, "targetSessionId", "target_session_id") ??
            "",
        ),
    ),
  };

  const profilePath = safeProfileConfigPath(
    state.runtimeConfig.profilesDir,
    profileId,
  );
  const matchedRuntimeConfig =
    removed.brains +
      removed.sessions +
      removed.channelBindings +
      removed.mcpBindings +
      removed.scheduledJobs >
    0;
  if (
    !matchedRuntimeConfig &&
    sessionsArchived.length === 0 &&
    (profilePath === undefined || !existsSync(profilePath))
  ) {
    throw new Error(`profile ${profileId} was not found`);
  }

  await writeJsonFileAtomic(
    state.config.paths.serviceConfigFile,
    runtimeConfigFile.value,
  );
  const applyResult = await applyServiceRuntimeConfigFromDisk(state, {
    createMissingSessions: false,
    eventType: "profile_decommissioned",
    summaryPrefix: `Profile ${profileId} decommissioned`,
  });
  const brainHandle = await unregisterServiceProfileBrain(state, profileId);
  return {
    profileId,
    runtimeConfigPath: state.config.paths.serviceConfigFile,
    ...(profilePath === undefined ? {} : { profilePath }),
    profileDirectoryPreserved: true,
    sessionsArchived,
    removed,
    brainHandle,
    skipped: {
      profileDirectory: "preserved",
    },
    applyResult,
  };
}

async function unregisterServiceProfileBrain(
  state: ServiceState,
  profileId: string,
): Promise<DecommissionedServiceProfile["brainHandle"]> {
  try {
    const handle = await state.bridge.unregisterBrainImplementationForProfile(
      profileId as ProfileId,
    );
    return { action: "removed", handle };
  } catch (error) {
    if (isNativeNotFoundError(error)) {
      return { action: "already_absent" };
    }
    throw error;
  }
}

function isNativeNotFoundError(error: unknown): boolean {
  const message = error instanceof Error ? error.message.toLowerCase() : "";
  return message.includes("notfound") || message.includes("not found");
}

async function createServiceProfile(
  state: ServiceState,
  command: AdminControlCommand,
): Promise<CreatedServiceProfile> {
  const profileId = requiredBodyString(command, "profileId");
  const displayName = optionalBodyString(command, "displayName");
  const profilePath = safeProfileConfigPath(
    state.runtimeConfig.profilesDir,
    profileId,
  );
  const runtimeConfigFile = await readRuntimeConfigFileForMutation(state);
  const profiles = await loadRuntimeConfigProfiles(state);
  const plan = await planCreateProfileWithRust({
    bridge: state.bridge,
    runtimeConfig: state.runtimeConfig,
    profiles,
    request: {
      profileId,
      ...(displayName === undefined ? {} : { displayName }),
      agentId: optionalBodyString(command, "agentId"),
      sessionId: optionalBodyString(command, "sessionId"),
      implementationId: optionalBodyString(command, "implementationId"),
      kind: createProfileKind(command),
      modelConfig: modelConfigFromBody(command.body.modelConfig),
      brain: profileBrainFromBody(
        command.body.brain ?? command.body.brainSelection,
      ),
      mcpToolProfile: optionalBodyString(command, "mcpToolProfile"),
      source: profileCreateSourceFromBody(command.body.source),
      now: state.now(),
      profileFileExists:
        profilePath === undefined ? false : existsSync(profilePath),
    },
  });
  assertCreateProfilePlan(plan);

  const profileSeed = plan.profileSeed;
  const runtimeBrain = plan.runtimeBrain;
  const runtimeSession = plan.runtimeSession;
  const profileMcpConfig = plan.profileMcpConfig;
  if (!profileSeed || !runtimeBrain || !runtimeSession || !profileMcpConfig) {
    throw new Error(
      "create-profile plan did not include required profile/runtime entries",
    );
  }
  const profileFileAction = plan.fileAssetActions.find(
    (action) => action.kind === "write_profile_json",
  );
  const plannedProfilePath = join(
    state.runtimeConfig.profilesDir,
    profileFileAction?.relativePath ?? `${profileSeed.profileId}.json`,
  );

  await mkdir(state.runtimeConfig.profilesDir, { recursive: true });
  await writeJsonFileAtomic(plannedProfilePath, {
    profileId: profileSeed.profileId,
    ...(profileSeed.displayName === undefined
      ? {}
      : { displayName: profileSeed.displayName }),
    modelConfig: profileSeed.modelConfig,
    brain: profileSeed.brain,
    mcpConfig: profileMcpConfig,
    skills: profileSeed.skillsMode,
  });

  runtimeConfigFile.array("brains").push(runtimeBrain);
  runtimeConfigFile.array("sessions").push(runtimeSession);
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
    profileId: profileSeed.profileId,
    ...(profileSeed.displayName === undefined
      ? {}
      : { displayName: profileSeed.displayName }),
    agentId: runtimeSession.agentId,
    sessionId: runtimeSession.sessionId,
    implementationId: runtimeBrain.implementationId,
    profilePath: plannedProfilePath,
    runtimeConfigPath: state.config.paths.serviceConfigFile,
    registryWrite: plan.registryWrite,
    fileAssetActions: plan.fileAssetActions,
    derivedRuntimeActions: plan.derivedRuntimeActions,
    applyResult,
  };
}

async function loadRuntimeConfigProfiles(
  state: ServiceState,
): Promise<ProfileConfig[]> {
  const profileIds = new Set<ProfileId>();
  for (const session of state.runtimeConfig.sessions) {
    profileIds.add(session.profileId);
  }
  const profiles: ProfileConfig[] = [];
  for (const profileId of profileIds) {
    profiles.push(
      await loadProfileConfig(state.runtimeConfig.profilesDir, profileId),
    );
  }
  return profiles;
}

function safeProfileConfigPath(
  profilesDir: string,
  profileId: string,
): string | undefined {
  if (!/^[A-Za-z0-9][A-Za-z0-9._:-]*$/.test(profileId)) {
    return undefined;
  }
  return join(profilesDir, `${profileId}.json`);
}

function createProfileKind(
  command: AdminControlCommand,
): "full" | "worker" | "delegated" | undefined {
  const kind = optionalBodyString(command, "kind");
  if (kind === undefined) {
    return undefined;
  }
  if (kind === "full" || kind === "worker" || kind === "delegated") {
    return kind;
  }
  throw new Error("profile session kind must be full, worker, or delegated");
}

function profileBrainFromBody(
  input: unknown,
): { module?: string; strategy?: string } | undefined {
  const brain = optionalRecord(input);
  if (!brain) {
    return undefined;
  }
  return compactRecord({
    module: optionalString(brain.module),
    strategy: optionalString(brain.strategy),
  }) as { module?: string; strategy?: string };
}

function profileCreateSourceFromBody(input: unknown):
  | {
      templateId?: string;
      sourceProfileId?: string;
      sourceBundlePath?: string;
    }
  | undefined {
  const source = optionalRecord(input);
  if (!source) {
    return undefined;
  }
  const result = compactRecord({
    templateId: optionalString(source.templateId),
    sourceProfileId: optionalString(source.sourceProfileId),
    sourceBundlePath: optionalString(source.sourceBundlePath),
  }) as {
    templateId?: string;
    sourceProfileId?: string;
    sourceBundlePath?: string;
  };
  return Object.keys(result).length > 0 ? result : undefined;
}

function assertCreateProfilePlan(plan: NativeCreateProfilePlan): void {
  const errors = plan.diagnostics.filter(
    (diagnostic) => diagnostic.severity === "error",
  );
  if (errors.length > 0) {
    const first = errors[0]!;
    const suffix =
      errors.length === 1
        ? ""
        : ` (${errors.length - 1} additional diagnostic${errors.length === 2 ? "" : "s"})`;
    throw new Error(
      `${first.path ? `${first.path}: ` : ""}${first.message}${suffix}`,
    );
  }
}

function modelConfigFromBody(input: unknown): BrainModelConfig | undefined {
  if (input === undefined) {
    return undefined;
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

function removeRuntimeConfigEntries(
  entries: unknown[],
  shouldRemove: (entry: Record<string, unknown>) => boolean,
): number {
  let removed = 0;
  for (let index = entries.length - 1; index >= 0; index -= 1) {
    const entry = entries[index];
    if (!isRecord(entry) || !shouldRemove(entry)) continue;
    entries.splice(index, 1);
    removed += 1;
  }
  return removed;
}

function runtimeEntryString(
  entry: Record<string, unknown>,
  camelKey: string,
  snakeKey: string,
): string | undefined {
  const value = entry[camelKey] ?? entry[snakeKey];
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function profileConfigDraftFromCommand(
  command: AdminControlCommand,
  profileId: string,
): Record<string, unknown> {
  const draft = optionalRecord(command.body.profileConfig);
  if (draft === undefined) {
    throw new Error("profileConfig object is required");
  }
  const next = structuredCloneRecord(draft);
  next.profileId = profileId;
  const soulMarkdown = optionalBodyString(command, "soulMarkdown");
  const memoryMarkdown = optionalBodyString(command, "memoryMarkdown");
  if (soulMarkdown !== undefined || memoryMarkdown !== undefined) {
    const prompt = optionalRecord(next.prompt);
    next.prompt = {
      ...(prompt ?? {}),
      ...(soulMarkdown === undefined ? {} : { soulMarkdown }),
      ...(memoryMarkdown === undefined ? {} : { memoryMarkdown }),
    };
  }
  return next;
}

function runtimeConfigDraftFromCommand(
  state: ServiceState,
  command: AdminControlCommand,
): RustyCrewRuntimeConfig {
  const draft = optionalRecord(command.body.runtimeConfig);
  if (draft === undefined) {
    throw new Error("runtimeConfig object is required");
  }
  return {
    profilesDir:
      optionalString(draft.profilesDir) ?? state.runtimeConfig.profilesDir,
    ...(optionalString(draft.skillsDir) === undefined
      ? {}
      : { skillsDir: optionalString(draft.skillsDir) }),
    brains: arrayValue(draft.brains).map((brain, index) =>
      runtimeConfigBrainDraft(brain, index),
    ),
    sessions: arrayValue(draft.sessions) as RustyCrewRuntimeConfig["sessions"],
    scheduledJobs: arrayValue(
      draft.scheduledJobs,
    ) as RustyCrewRuntimeConfig["scheduledJobs"],
    channelBindings: arrayValue(
      draft.channelBindings,
    ) as RustyCrewRuntimeConfig["channelBindings"],
    mcpBindings: arrayValue(
      draft.mcpBindings,
    ) as RustyCrewRuntimeConfig["mcpBindings"],
  };
}

function runtimeConfigBrainDraft(
  value: unknown,
  index: number,
): RustyCrewRuntimeConfig["brains"][number] {
  if (!isRecord(value)) {
    throw new Error(`runtimeConfig.brains[${index}] must be an object`);
  }
  const profileId = optionalString(value.profileId);
  if (profileId === undefined) {
    throw new Error(`runtimeConfig.brains[${index}].profileId is required`);
  }
  return {
    profileId: profileId as ProfileId,
    implementationId: (optionalString(value.implementationId) ??
      `${profileId}-brain`) as never,
  };
}

async function loadRuntimeConfigProfilesReplacing(
  state: ServiceState,
  profileId: string,
  replacement: ProfileConfig,
): Promise<ProfileConfig[]> {
  const profileIds = new Set<ProfileId>();
  for (const brain of state.runtimeConfig.brains) {
    profileIds.add(brain.profileId);
  }
  for (const session of state.runtimeConfig.sessions) {
    profileIds.add(session.profileId);
  }
  profileIds.add(profileId as ProfileId);
  const profiles: ProfileConfig[] = [];
  for (const candidateId of profileIds) {
    if (String(candidateId) === profileId) {
      profiles.push(replacement);
      continue;
    }
    profiles.push(
      await loadProfileConfig(state.runtimeConfig.profilesDir, candidateId),
    );
  }
  return profiles;
}

async function loadRuntimeConfigProfilesForDraft(
  runtimeConfig: RustyCrewRuntimeConfig,
): Promise<{
  profiles: ProfileConfig[];
  diagnostics: Array<{
    severity: "error";
    code: string;
    path: string;
    message: string;
  }>;
}> {
  const profileIds = new Set<ProfileId>();
  for (const brain of runtimeConfig.brains) profileIds.add(brain.profileId);
  for (const session of runtimeConfig.sessions)
    profileIds.add(session.profileId);
  const profiles: ProfileConfig[] = [];
  const diagnostics: Array<{
    severity: "error";
    code: string;
    path: string;
    message: string;
  }> = [];
  for (const profileId of profileIds) {
    try {
      profiles.push(
        await loadProfileConfig(runtimeConfig.profilesDir, profileId),
      );
    } catch (error) {
      diagnostics.push({
        severity: "error",
        code: "profile_metadata_load_failed",
        path: `profiles.${profileId}`,
        message: errorMessage(
          error,
          `profile ${profileId} could not be loaded`,
        ),
      });
    }
  }
  return { profiles, diagnostics };
}

function profileRuntimeBrainChanged(
  before: ProfileConfig | undefined,
  after: ProfileConfig | undefined,
): boolean {
  if (before === undefined || after === undefined) return false;
  return (
    JSON.stringify(before.modelConfig) !== JSON.stringify(after.modelConfig) ||
    JSON.stringify(before.brain ?? {}) !== JSON.stringify(after.brain ?? {})
  );
}

function profileMcpChanged(
  before: ProfileConfig | undefined,
  after: ProfileConfig | undefined,
): boolean {
  if (before === undefined || after === undefined) return false;
  return (
    JSON.stringify(before.mcpConfig ?? {}) !==
    JSON.stringify(after.mcpConfig ?? {})
  );
}

function structuredCloneRecord(
  record: Record<string, unknown>,
): Record<string, unknown> {
  return JSON.parse(JSON.stringify(record)) as Record<string, unknown>;
}

function arrayValue(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
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
    readProfileConfig: async (command) => {
      const result = await readServiceProfileConfig(state, command);
      return {
        status: "completed",
        summary: `profile ${result.profileId} read`,
        affectedIds: { profileId: String(result.profileId) },
        result,
      };
    },
    planProfileUpdate: async (command) => {
      const result = await planServiceProfileUpdate(state, command);
      return {
        status: result.ok ? "completed" : "failed",
        summary: result.ok
          ? `profile ${result.profileId} update plan is valid`
          : `profile ${result.profileId} update plan is invalid`,
        affectedIds: { profileId: result.profileId },
        result,
        reasonCode: result.ok ? undefined : "profile_update_plan_invalid",
      };
    },
    applyProfileUpdate: async (command) => {
      const result = await applyServiceProfileUpdate(state, command);
      return {
        status: result.ok ? "completed" : "failed",
        summary: result.ok
          ? `profile ${result.profileId} updated`
          : `profile ${result.profileId} update rejected`,
        affectedIds: { profileId: result.profileId },
        result,
        reasonCode: result.ok ? undefined : "profile_update_plan_invalid",
      };
    },
    decommissionProfile: async (command) => {
      const result = await decommissionServiceProfile(state, command);
      return {
        status: "completed",
        summary: `profile ${result.profileId} decommissioned`,
        affectedIds: {
          profileId: result.profileId,
          sessionsArchived: result.sessionsArchived.length,
          brainsRemoved: result.removed.brains,
          brainHandleRemoved: result.brainHandle.action === "removed" ? 1 : 0,
          sessionsRemoved: result.removed.sessions,
          channelBindingsRemoved: result.removed.channelBindings,
          mcpBindingsRemoved: result.removed.mcpBindings,
          scheduledJobsRemoved: result.removed.scheduledJobs,
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
    pauseRuntime: async (command) => pauseRuntimeTarget(state, command),
    resumeRuntime: async (command) => resumeRuntimeTarget(state, command),
    reloadMcp: createServiceReloadMcpExecutor(state),
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
    planRuntimeConfigUpdate: async (command) => {
      const result = await planServiceRuntimeConfigDraft(state, command);
      return {
        status: result.ok ? "completed" : "failed",
        summary: result.ok
          ? "runtime config draft plan is valid"
          : "runtime config draft plan is invalid",
        result,
        reasonCode: result.ok ? undefined : "runtime_config_draft_invalid",
      };
    },
    applyRuntimeConfigUpdate: async (command) => {
      const result = await applyServiceRuntimeConfigDraft(state, command);
      return {
        status: result.ok ? "completed" : "failed",
        summary: result.ok
          ? "runtime config draft applied"
          : "runtime config draft rejected",
        result,
        reasonCode: result.ok ? undefined : "runtime_config_draft_invalid",
      };
    },
    planRuntimeRebuild: async (command) => {
      const result = await planServiceRuntimeRebuild(state, command);
      return {
        status: "completed",
        summary: "runtime rebuild plan prepared",
        affectedIds: runtimeRebuildAffectedIds(result),
        result,
      };
    },
    applyRuntimeRebuild: async (command) => {
      const result = await applyServiceRuntimeRebuild(state, command);
      return {
        status: result.apply.status === "completed" ? "completed" : "failed",
        summary:
          result.apply.status === "completed"
            ? `runtime rebuild applied for profile ${result.profileId}`
            : `runtime rebuild blocked for profile ${result.profileId}`,
        affectedIds: runtimeRebuildAffectedIds(result),
        result,
        reasonCode:
          result.apply.status === "completed"
            ? undefined
            : result.apply.reasonCode,
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
        expireProviderWireStatesAt: optionalBodyString(
          command,
          "expireProviderWireStatesAt",
        ),
        compactSessionMemoryAt: optionalBodyString(
          command,
          "compactSessionMemoryAt",
        ),
        sessionMemoryMaxActiveRecordsPerScope: optionalNumber(
          command.body.sessionMemoryMaxActiveRecordsPerScope,
        ),
        sessionMemoryArchiveBatchSize: optionalNumber(
          command.body.sessionMemoryArchiveBatchSize,
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

async function pauseRuntimeTarget(
  state: ServiceState,
  command: AdminControlCommand,
): Promise<AdminControlResponse["outcome"]> {
  const target = await runtimePauseTarget(state, command, true);
  const key = runtimePauseKey(target.scope, target.targetId);
  const existing = state.runtimePauses.get(key);
  if (existing !== undefined) {
    return {
      status: "completed",
      summary: `runtime ${target.scope} ${target.targetId} was already paused`,
      affectedIds: runtimePauseAffectedIds(existing),
      result: runtimePauseRecordView(existing),
    };
  }

  const affectedSessionIds = await affectedRuntimePauseSessionIds(
    state,
    target,
  );
  if (affectedSessionIds.length === 0) {
    return {
      status: "failed",
      summary: `runtime ${target.scope} ${target.targetId} did not match any configured sessions`,
      reasonCode: "runtime_pause_target_not_found",
      affectedIds: { [runtimePauseTargetKey(target.scope)]: target.targetId },
    };
  }

  const record: RuntimePauseRecord = {
    pauseId: [
      "pause",
      target.scope,
      target.targetId.replace(/[^0-9A-Za-z_-]/g, "_"),
      Date.now(),
    ].join(":"),
    scope: target.scope,
    targetId: target.targetId,
    pausedBy: command.actor.operatorId,
    pausedAt: state.now(),
    reason: command.reason,
    reasonCode: command.reasonCode,
    affectedSessionIds,
    inFlightWakeCount: affectedSessionIds.filter((sessionId) =>
      state.inFlightWakes.has(sessionId as SessionId),
    ).length,
  };
  state.runtimePauses.set(key, record);
  recordServiceEvent(state, {
    source: "service-host",
    eventType: "runtime_target_paused",
    severity: "warning",
    summary: `Paused runtime ${record.scope} ${record.targetId}; ${record.affectedSessionIds.length} session(s) affected, ${record.inFlightWakeCount} wake(s) already in flight.`,
  });
  return {
    status: "completed",
    summary:
      record.inFlightWakeCount > 0
        ? `runtime ${record.scope} ${record.targetId} paused; ${record.inFlightWakeCount} in-flight wake(s) will finish before suppression fully takes effect`
        : `runtime ${record.scope} ${record.targetId} paused`,
    affectedIds: runtimePauseAffectedIds(record),
    result: runtimePauseRecordView(record),
  };
}

async function resumeRuntimeTarget(
  state: ServiceState,
  command: AdminControlCommand,
): Promise<AdminControlResponse["outcome"]> {
  const target = await runtimePauseTarget(state, command, false);
  const key = runtimePauseKey(target.scope, target.targetId);
  const record = state.runtimePauses.get(key);
  if (record === undefined) {
    return {
      status: "completed",
      summary: `runtime ${target.scope} ${target.targetId} was not paused`,
      affectedIds: { [runtimePauseTargetKey(target.scope)]: target.targetId },
      result: { paused: false, scope: target.scope, targetId: target.targetId },
    };
  }
  state.runtimePauses.delete(key);
  recordServiceEvent(state, {
    source: "service-host",
    eventType: "runtime_target_resumed",
    summary: `Resumed runtime ${record.scope} ${record.targetId}; ${record.affectedSessionIds.length} session(s) affected.`,
  });
  return {
    status: "completed",
    summary: `runtime ${record.scope} ${record.targetId} resumed`,
    affectedIds: runtimePauseAffectedIds(record),
    result: { ...runtimePauseRecordView(record), resumedAt: state.now() },
  };
}

async function runtimePauseTarget(
  state: ServiceState,
  command: AdminControlCommand,
  validateSession: boolean,
): Promise<{ scope: RuntimePauseScope; targetId: string }> {
  const scope = command.target.scope;
  if (scope !== "session" && scope !== "profile" && scope !== "agent") {
    throw new Error(
      "runtime pause target scope must be session, profile, or agent",
    );
  }
  const targetId =
    scope === "session"
      ? command.target.sessionId
      : scope === "profile"
        ? command.target.profileId
        : command.target.agentId;
  if (!targetId) {
    throw new Error(`runtime pause target ${scope} id is required`);
  }
  if (validateSession && scope === "session") {
    await serviceSessionById(state, targetId);
  }
  return { scope, targetId };
}

async function affectedRuntimePauseSessionIds(
  state: ServiceState,
  target: { scope: RuntimePauseScope; targetId: string },
): Promise<string[]> {
  const runtimeSessions = await state.bridge.listSessions().catch(() => []);
  const configured = state.runtimeConfig.sessions;
  const ids = new Set<string>();
  for (const session of [...configured, ...runtimeSessions]) {
    if (runtimePauseMatchesSession(target, session)) {
      ids.add(session.sessionId);
    }
  }
  return [...ids].sort();
}

function runtimePauseMatchesSession(
  target: { scope: RuntimePauseScope; targetId: string },
  session: Pick<SessionState, "sessionId" | "agentId" | "profileId">,
): boolean {
  if (target.scope === "session") return session.sessionId === target.targetId;
  if (target.scope === "profile") return session.profileId === target.targetId;
  return session.agentId === target.targetId;
}

function runtimePauseForSession(
  state: ServiceState,
  session: Pick<SessionState, "sessionId" | "agentId" | "profileId">,
): RuntimePauseRecord | undefined {
  return (
    state.runtimePauses.get(runtimePauseKey("session", session.sessionId)) ??
    state.runtimePauses.get(runtimePauseKey("profile", session.profileId)) ??
    state.runtimePauses.get(runtimePauseKey("agent", session.agentId))
  );
}

function runtimePauseKey(scope: RuntimePauseScope, targetId: string): string {
  return `${scope}:${targetId}`;
}

function runtimePauseTargetKey(scope: RuntimePauseScope): string {
  if (scope === "session") return "sessionId";
  if (scope === "profile") return "profileId";
  return "agentId";
}

function runtimePauseAffectedIds(
  record: RuntimePauseRecord,
): Record<string, string | number> {
  return {
    [runtimePauseTargetKey(record.scope)]: record.targetId,
    affectedSessions: record.affectedSessionIds.length,
    inFlightWakeCount: record.inFlightWakeCount,
  };
}

function runtimePauseRecordView(
  record: RuntimePauseRecord,
): RuntimePauseDiagnostics {
  return {
    pauseId: record.pauseId,
    scope: record.scope,
    targetId: record.targetId,
    pausedBy: record.pausedBy,
    pausedAt: record.pausedAt,
    reason: record.reason,
    reasonCode: record.reasonCode,
    affectedSessionIds: record.affectedSessionIds,
    inFlightWakeCount: record.inFlightWakeCount,
    cancellationSupported: false,
    limitation:
      "Current implementation suppresses new wakes and delivery claims; it does not interrupt an LLM/tool call already in flight.",
  };
}

function runtimePauseDiagnostics(
  state: ServiceState,
  sessions: readonly SessionState[],
): RuntimePauseDiagnostics[] {
  return [...state.runtimePauses.values()]
    .map((record) => ({
      ...record,
      affectedSessionIds: sessions
        .filter((session) =>
          runtimePauseMatchesSession(
            { scope: record.scope, targetId: record.targetId },
            session,
          ),
        )
        .map((session) => session.sessionId),
      inFlightWakeCount: sessions.filter(
        (session) =>
          runtimePauseMatchesSession(
            { scope: record.scope, targetId: record.targetId },
            session,
          ) && state.inFlightWakes.has(session.sessionId),
      ).length,
    }))
    .map(runtimePauseRecordView);
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

function createServiceReloadMcpExecutor(
  state: ServiceState,
): NonNullable<AdminControlExecutor["reloadMcp"]> {
  return createReloadMcpControlExecutor({
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
  });
}

async function refreshMcpBindingsAfterRuntimeRebuild(
  state: ServiceState,
  bindingIds: readonly string[],
  command: AdminControlCommand,
): Promise<ServiceRuntimeRebuildMcpRefreshResult> {
  const uniqueBindingIds = [...new Set(bindingIds)];
  const reloadMcp = createServiceReloadMcpExecutor(state);
  const results: ServiceRuntimeRebuildMcpRefreshResult["results"] = [];

  for (const bindingId of uniqueBindingIds) {
    const binding = state.runtimeConfig.mcpBindings.find(
      (candidate) => candidate.bindingId === bindingId,
    );
    if (binding?.sessionId === undefined) {
      results.push({
        bindingId,
        status: "missing",
        reasonCode: "mcp_binding_missing_after_rebuild",
        summary: `MCP binding ${bindingId} was not present after runtime rebuild.`,
      });
      continue;
    }

    const outcome = await reloadMcp({
      ...command,
      name: "reload_mcp",
      target: { sessionId: binding.sessionId },
      reason: command.reason ?? "runtime rebuild MCP refresh",
    });
    const status =
      outcome.status === "completed"
        ? ("refreshed" as const)
        : ("degraded" as const);
    results.push({
      bindingId,
      sessionId: binding.sessionId,
      status,
      reasonCode: outcome.reasonCode,
      summary: outcome.summary,
    });
  }

  const refreshedBindingIds = results
    .filter((result) => result.status === "refreshed")
    .map((result) => result.bindingId);
  const degradedBindingIds = results
    .filter((result) => result.status === "degraded")
    .map((result) => result.bindingId);
  const missingBindingIds = results
    .filter((result) => result.status === "missing")
    .map((result) => result.bindingId);

  return {
    action: "refresh_after_rebuild",
    bindingIds: uniqueBindingIds,
    refreshedBindingIds,
    degradedBindingIds,
    missingBindingIds,
    results,
  };
}

interface ServiceRuntimeRebuildMcpRefreshResult {
  action: "refresh_after_rebuild";
  bindingIds: string[];
  refreshedBindingIds: string[];
  degradedBindingIds: string[];
  missingBindingIds: string[];
  results: Array<{
    bindingId: string;
    sessionId?: string;
    status: "refreshed" | "degraded" | "missing";
    reasonCode?: string;
    summary: string;
  }>;
}

interface ServiceRuntimeRebuildPlan {
  scope: "session" | "profile";
  profileId: string;
  sessionIds: string[];
  applySupported: true;
  requiredAction: "brain_hot_swap_required";
  preservesSessionId: boolean;
  preservesHistory: boolean;
  replacementSession?: {
    mode: "derive_from_prior_session";
    explicitApplyRequired: true;
    oldSessionId: string;
    requestedNewSessionId?: string;
  };
  configReload: {
    implicit: false;
    requiredBeforeApply: boolean;
  };
  providerState: {
    action: "discard" | "migrate" | "unsupported";
    reason: string;
    migrationId?: string;
    clearedSessions?: number;
  };
  queuedMessages: {
    action:
      | "preserve_existing_queue_without_redelivery"
      | "start_replacement_session_with_empty_queue";
    ttlPolicy: "unchanged";
  };
  channelBindings: {
    action: "unchanged" | "move_to_replacement_session";
    bindingIds: string[];
  };
  mcp: {
    action: "refresh_after_rebuild";
    bindingIds: string[];
    refreshedBindingIds?: string[];
    degradedBindingIds?: string[];
    missingBindingIds?: string[];
    results?: ServiceRuntimeRebuildMcpRefreshResult["results"];
  };
  diagnostics: {
    brainModule?: string;
    profileConfigured: boolean;
    sessionsConfigured: number;
    sessionsActive: number;
  };
}

interface ServiceRuntimeRebuildApplyResult extends ServiceRuntimeRebuildPlan {
  apply:
    | {
        status: "completed";
        handle: BrainImplementationHandle;
        implementationId: BrainImplementationId;
        audited: true;
        replacementSession?: ServiceRuntimeReplacementSessionResult;
      }
    | {
        status: "blocked";
        reasonCode:
          | "runtime_rebuild_in_flight"
          | "provider_state_rebuild_unsupported"
          | "provider_state_migration_not_implemented";
        blockedSessionIds: string[];
      };
}

interface ServiceRuntimeReplacementSessionResult {
  oldSessionId: string;
  newSessionId: string;
  channelBindings: {
    action: "unchanged" | "move_to_replacement_session";
    bindingIds: string[];
  };
  mcpBindings: {
    action: "move_to_replacement_session";
    bindingIds: string[];
  };
  scheduledJobs: {
    action: "move_to_replacement_session";
    jobIds: string[];
  };
  queuedMessages: {
    action: "start_replacement_session_with_empty_queue";
    oldSessionQueuePreserved: true;
    expiredQueuedMessagesCopied: false;
  };
}

async function planServiceRuntimeRebuild(
  state: ServiceState,
  command: AdminControlCommand,
): Promise<ServiceRuntimeRebuildPlan> {
  const scope = command.target.scope;
  if (scope !== "session" && scope !== "profile") {
    throw new Error("runtime rebuild target scope must be session or profile");
  }

  const activeSessions = await state.bridge.listSessions();
  const configuredSessions = state.runtimeConfig.sessions;
  const replaceSessionIdentity = runtimeRebuildReplacesSessionIdentity(command);
  const configuredProfileIds = new Set(
    state.runtimeConfig.brains.map((brain) => String(brain.profileId)),
  );

  let profileId: string;
  let sessionIds: string[];
  if (scope === "session") {
    const sessionId = command.target.sessionId;
    if (!sessionId) throw new Error("runtime rebuild session id is required");
    const activeSession = activeSessions.find(
      (session) => session.sessionId === sessionId,
    );
    const configuredSession = configuredSessions.find(
      (session) => session.sessionId === sessionId,
    );
    profileId = activeSession?.profileId ?? configuredSession?.profileId ?? "";
    if (!profileId) throw new Error(`session ${sessionId} was not found`);
    sessionIds = [sessionId];
  } else {
    if (replaceSessionIdentity) {
      throw new Error(
        "replacement session rebuild is only supported for a single session target",
      );
    }
    profileId = command.target.profileId ?? "";
    if (!profileId) throw new Error("runtime rebuild profile id is required");
    if (!configuredProfileIds.has(profileId)) {
      throw new Error(`profile ${profileId} is not configured for a brain`);
    }
    sessionIds = [
      ...new Set(
        [
          ...activeSessions
            .filter((session) => session.profileId === profileId)
            .map((session) => session.sessionId),
          ...configuredSessions
            .filter((session) => session.profileId === profileId)
            .map((session) => session.sessionId),
        ].filter(Boolean),
      ),
    ];
  }

  const channelBindingIds = state.runtimeConfig.channelBindings
    .filter(
      (binding) =>
        binding.sessionId !== undefined &&
        sessionIds.includes(binding.sessionId),
    )
    .map((binding) => binding.bindingId);
  const mcpBindingIds = state.runtimeConfig.mcpBindings
    .filter(
      (binding) =>
        binding.sessionId !== undefined &&
        sessionIds.includes(binding.sessionId),
    )
    .map((binding) => binding.bindingId);
  const brainModule =
    state.runtimeConfigApplyResult.brainModulesByProfileId[profileId]?.moduleId;
  const brainDiagnostics =
    state.runtimeConfigApplyResult.brainDiagnosticsByProfileId[profileId];
  const providerStateRebuild = brainDiagnostics?.providerStateRebuild ?? {
    action: "unsupported" as const,
    reason:
      "brain module did not declare provider-state rebuild handling; fail closed",
  };

  return {
    scope,
    profileId,
    sessionIds,
    applySupported: true,
    requiredAction: "brain_hot_swap_required",
    preservesSessionId: !replaceSessionIdentity,
    preservesHistory: !replaceSessionIdentity,
    ...(replaceSessionIdentity
      ? {
          replacementSession: {
            mode: "derive_from_prior_session",
            explicitApplyRequired: true,
            oldSessionId: sessionIds[0] ?? "",
            requestedNewSessionId: optionalBodyString(command, "newSessionId"),
          },
        }
      : {}),
    configReload: {
      implicit: false,
      requiredBeforeApply: false,
    },
    providerState: {
      action: providerStateRebuild.action,
      reason: providerStateRebuild.reason,
      ...(providerStateRebuild.migrationId === undefined
        ? {}
        : { migrationId: providerStateRebuild.migrationId }),
    },
    queuedMessages: {
      action: replaceSessionIdentity
        ? "start_replacement_session_with_empty_queue"
        : "preserve_existing_queue_without_redelivery",
      ttlPolicy: "unchanged",
    },
    channelBindings: {
      action:
        replaceSessionIdentity &&
        replacementChannelBindingAction(command) === "move"
          ? "move_to_replacement_session"
          : "unchanged",
      bindingIds: channelBindingIds,
    },
    mcp: {
      action: "refresh_after_rebuild",
      bindingIds: mcpBindingIds,
    },
    diagnostics: {
      brainModule,
      profileConfigured: configuredProfileIds.has(profileId),
      sessionsConfigured: configuredSessions.filter(
        (session) => session.profileId === profileId,
      ).length,
      sessionsActive: activeSessions.filter(
        (session) => session.profileId === profileId,
      ).length,
    },
  };
}

async function applyServiceRuntimeRebuild(
  state: ServiceState,
  command: AdminControlCommand,
): Promise<ServiceRuntimeRebuildApplyResult> {
  const plan = await planServiceRuntimeRebuild(state, command);
  const activeProfileSessionIds = (await state.bridge.listSessions())
    .filter((session) => session.profileId === plan.profileId)
    .map((session) => session.sessionId);
  const blockedSessionIds = activeProfileSessionIds.filter((sessionId) =>
    state.inFlightWakes.has(sessionId),
  );
  if (plan.providerState.action === "unsupported") {
    recordServiceEvent(state, {
      source: "service-host",
      eventType: "runtime_rebuild_blocked",
      severity: "warning",
      summary: `Runtime rebuild for profile ${plan.profileId} blocked because provider-state handling is unsupported: ${plan.providerState.reason}.`,
    });
    return {
      ...plan,
      apply: {
        status: "blocked",
        reasonCode: "provider_state_rebuild_unsupported",
        blockedSessionIds: [],
      },
    };
  }
  if (plan.providerState.action === "migrate") {
    recordServiceEvent(state, {
      source: "service-host",
      eventType: "runtime_rebuild_blocked",
      severity: "warning",
      summary: `Runtime rebuild for profile ${plan.profileId} blocked because provider-state migration is not implemented: ${plan.providerState.reason}.`,
    });
    return {
      ...plan,
      apply: {
        status: "blocked",
        reasonCode: "provider_state_migration_not_implemented",
        blockedSessionIds: [],
      },
    };
  }
  if (blockedSessionIds.length > 0) {
    recordServiceEvent(state, {
      source: "service-host",
      eventType: "runtime_rebuild_blocked",
      severity: "warning",
      summary: `Runtime rebuild for profile ${plan.profileId} blocked by in-flight wake(s): ${blockedSessionIds.join(", ")}.`,
    });
    return {
      ...plan,
      apply: {
        status: "blocked",
        reasonCode: "runtime_rebuild_in_flight",
        blockedSessionIds,
      },
    };
  }

  if (runtimeRebuildReplacesSessionIdentity(command)) {
    return applyServiceRuntimeRebuildWithReplacementSession(
      state,
      command,
      plan,
    );
  }

  const previousBrain =
    state.runtimeConfigApplyResult.brainHandlesByProfileId[plan.profileId];
  let clearedSessions = 0;
  const providerStateMode =
    state.runtimeConfigApplyResult.brainDiagnosticsByProfileId[plan.profileId]
      ?.providerStateMode;
  if (
    previousBrain !== undefined &&
    plan.providerState.action === "discard" &&
    providerStateMode !== undefined &&
    providerStateMode !== "unused"
  ) {
    for (const sessionId of plan.sessionIds) {
      await state.bridge.clearBrainProviderState({
        brain: previousBrain,
        sessionId: sessionId as SessionId,
        wakeId: `runtime-rebuild-${Date.now()}-${sessionId}`,
      });
      clearedSessions += 1;
    }
  }

  const rebuild = await rebuildConfiguredBrainRuntime({
    serviceConfig: state.config,
    runtimeConfig: state.runtimeConfig,
    profileId: plan.profileId as ProfileId,
    bridge: state.bridge,
    curatorExecutor: state.curator.executor,
    mcpSurfaceDiagnostics: state.mcpManager.diagnostics(),
  });
  state.runtimeConfigApplyResult.brainHandlesByProfileId[plan.profileId] =
    rebuild.handle;
  state.runtimeConfigApplyResult.brainModulesByProfileId[plan.profileId] =
    rebuild.module;
  state.runtimeConfigApplyResult.brainDiagnosticsByProfileId[plan.profileId] =
    rebuild.diagnostics;
  recordServiceEvent(state, {
    source: "service-host",
    eventType: "runtime_rebuild_applied",
    summary: `Runtime rebuild applied for profile ${plan.profileId} with brain handle ${rebuild.handle}.`,
  });
  const mcpRefresh = await refreshMcpBindingsAfterRuntimeRebuild(
    state,
    plan.mcp.bindingIds,
    command,
  );

  return {
    ...plan,
    providerState: {
      ...plan.providerState,
      clearedSessions,
    },
    mcp: mcpRefresh,
    apply: {
      status: "completed",
      handle: rebuild.handle,
      implementationId: rebuild.implementationId,
      audited: true,
    },
  };
}

async function applyServiceRuntimeRebuildWithReplacementSession(
  state: ServiceState,
  command: AdminControlCommand,
  plan: ServiceRuntimeRebuildPlan,
): Promise<ServiceRuntimeRebuildApplyResult> {
  if (plan.scope !== "session") {
    throw new Error(
      "replacement session rebuild requires a session-scoped target",
    );
  }
  const oldSessionId = plan.sessionIds[0];
  if (!oldSessionId)
    throw new Error("replacement session rebuild requires a session id");
  const oldSession = await serviceSessionById(state, oldSessionId);
  if (oldSession.status === "archived") {
    throw new Error(`session ${oldSessionId} is already archived`);
  }
  const newSessionId =
    optionalBodyString(command, "newSessionId") ??
    replacementRuntimeSessionId(state, oldSession);
  if (newSessionId === oldSessionId) {
    throw new Error(
      "replacement session id must differ from the old session id",
    );
  }
  const existingSession = (await state.bridge.listSessions()).find(
    (session) => session.sessionId === newSessionId,
  );
  if (existingSession !== undefined) {
    throw new Error(`replacement session ${newSessionId} already exists`);
  }

  const previousBrain =
    state.runtimeConfigApplyResult.brainHandlesByProfileId[plan.profileId];
  const providerStateMode =
    state.runtimeConfigApplyResult.brainDiagnosticsByProfileId[plan.profileId]
      ?.providerStateMode;
  let clearedSessions = 0;
  if (
    previousBrain !== undefined &&
    plan.providerState.action === "discard" &&
    providerStateMode !== undefined &&
    providerStateMode !== "unused"
  ) {
    await state.bridge.clearBrainProviderState({
      brain: previousBrain,
      sessionId: oldSessionId as SessionId,
      wakeId: `runtime-rebuild-replace-${Date.now()}-${oldSessionId}`,
    });
    clearedSessions = 1;
  }

  const replacement = await replaceRuntimeSessionInConfig(
    state,
    oldSession,
    newSessionId,
    replacementChannelBindingAction(command),
  );
  await state.bridge.archiveSession(oldSessionId as SessionId);
  await applyServiceRuntimeConfigFromDisk(state, {
    createMissingSessions: true,
    eventType: "runtime_rebuild_replacement_session_created",
    summaryPrefix: `Runtime rebuild replaced session ${oldSessionId}`,
  });
  const rebuild = await rebuildConfiguredBrainRuntime({
    serviceConfig: state.config,
    runtimeConfig: state.runtimeConfig,
    profileId: plan.profileId as ProfileId,
    bridge: state.bridge,
    curatorExecutor: state.curator.executor,
    mcpSurfaceDiagnostics: state.mcpManager.diagnostics(),
  });
  state.runtimeConfigApplyResult.brainHandlesByProfileId[plan.profileId] =
    rebuild.handle;
  state.runtimeConfigApplyResult.brainModulesByProfileId[plan.profileId] =
    rebuild.module;
  state.runtimeConfigApplyResult.brainDiagnosticsByProfileId[plan.profileId] =
    rebuild.diagnostics;
  recordServiceEvent(state, {
    source: "service-host",
    eventType: "runtime_rebuild_replacement_session_applied",
    summary: `Runtime rebuild archived ${oldSessionId} and created replacement session ${newSessionId}.`,
  });
  const mcpRefresh = await refreshMcpBindingsAfterRuntimeRebuild(
    state,
    replacement.mcpBindings.bindingIds,
    command,
  );

  return {
    ...plan,
    sessionIds: [newSessionId],
    providerState: {
      ...plan.providerState,
      clearedSessions,
    },
    queuedMessages: {
      action: "start_replacement_session_with_empty_queue",
      ttlPolicy: "unchanged",
    },
    channelBindings: replacement.channelBindings,
    mcp: mcpRefresh,
    apply: {
      status: "completed",
      handle: rebuild.handle,
      implementationId: rebuild.implementationId,
      audited: true,
      replacementSession: {
        ...replacement,
        queuedMessages: {
          action: "start_replacement_session_with_empty_queue",
          oldSessionQueuePreserved: true,
          expiredQueuedMessagesCopied: false,
        },
      },
    },
    diagnostics: plan.diagnostics,
  };
}

function runtimeRebuildReplacesSessionIdentity(
  command: AdminControlCommand,
): boolean {
  const mode =
    optionalBodyString(command, "sessionIdentity") ??
    optionalBodyString(command, "sessionIdentityMode");
  if (mode === undefined || mode === "preserve") return false;
  if (mode === "replace") return true;
  throw new Error("sessionIdentity must be preserve or replace");
}

function replacementChannelBindingAction(
  command: AdminControlCommand,
): "move" | "unchanged" {
  const action =
    optionalBodyString(command, "channelBindingAction") ?? "unchanged";
  if (action === "move" || action === "unchanged") return action;
  throw new Error("channelBindingAction must be move or unchanged");
}

function replacementRuntimeSessionId(
  state: ServiceState,
  session: Pick<SessionState, "agentId" | "sessionId">,
): string {
  state.nextWakeSequence += 1;
  return [
    session.agentId,
    "session",
    state
      .now()
      .replace(/[^0-9A-Za-z]/g, "")
      .slice(0, 17),
    state.nextWakeSequence,
  ].join("-");
}

async function replaceRuntimeSessionInConfig(
  state: ServiceState,
  oldSession: SessionState,
  newSessionId: string,
  channelBindingAction: "move" | "unchanged",
): Promise<ServiceRuntimeReplacementSessionResult> {
  if (!/^[A-Za-z0-9][A-Za-z0-9._:-]*$/.test(newSessionId)) {
    throw new Error("replacement session id contains unsupported characters");
  }
  const runtimeConfigFile = await readRuntimeConfigFileForMutation(state);
  const sessions = runtimeConfigFile.array("sessions");
  const sessionEntry = sessions.find(
    (entry): entry is Record<string, unknown> =>
      isRecord(entry) &&
      runtimeEntryString(entry, "sessionId", "session_id") ===
        oldSession.sessionId,
  );
  if (sessionEntry === undefined) {
    sessions.push(runtimeConfigSessionEntryFromState(oldSession, newSessionId));
  } else {
    sessionEntry.sessionId = newSessionId;
    delete sessionEntry.session_id;
  }

  const channelBindingIds =
    channelBindingAction === "move"
      ? replaceRuntimeConfigSessionRefs(
          runtimeConfigFile.array("channelBindings"),
          oldSession.sessionId,
          newSessionId,
          "sessionId",
          "session_id",
          "bindingId",
          "binding_id",
        )
      : state.runtimeConfig.channelBindings
          .filter((binding) => binding.sessionId === oldSession.sessionId)
          .map((binding) => binding.bindingId);
  const mcpBindingIds = replaceRuntimeConfigSessionRefs(
    runtimeConfigFile.array("mcpBindings"),
    oldSession.sessionId,
    newSessionId,
    "sessionId",
    "session_id",
    "bindingId",
    "binding_id",
  );
  const scheduledJobIds = replaceRuntimeConfigSessionRefs(
    runtimeConfigFile.array("scheduledJobs"),
    oldSession.sessionId,
    newSessionId,
    "targetSessionId",
    "target_session_id",
    "id",
    "id",
  );

  await writeJsonFileAtomic(
    state.config.paths.serviceConfigFile,
    runtimeConfigFile.value,
  );
  return {
    oldSessionId: oldSession.sessionId,
    newSessionId,
    channelBindings: {
      action:
        channelBindingAction === "move"
          ? "move_to_replacement_session"
          : "unchanged",
      bindingIds: channelBindingIds,
    },
    mcpBindings: {
      action: "move_to_replacement_session",
      bindingIds: mcpBindingIds,
    },
    scheduledJobs: {
      action: "move_to_replacement_session",
      jobIds: scheduledJobIds,
    },
    queuedMessages: {
      action: "start_replacement_session_with_empty_queue",
      oldSessionQueuePreserved: true,
      expiredQueuedMessagesCopied: false,
    },
  };
}

function runtimeConfigSessionEntryFromState(
  session: SessionState,
  newSessionId: string,
): Record<string, unknown> {
  return compactRecord({
    sessionId: newSessionId,
    agentId: session.agentId,
    profileId: session.profileId,
    kind: session.kind,
    resourceLimits: compactRecord({
      workdir: session.resourceLimits.workdir,
      maxDurationMs: session.resourceLimits.maxDurationMs,
      maxDelegationDepth: session.resourceLimits.maxDelegationDepth,
    }),
    maxHistoryMessages: session.historyWindow?.maxMessages,
  });
}

function replaceRuntimeConfigSessionRefs(
  entries: unknown[],
  oldSessionId: string,
  newSessionId: string,
  sessionCamelKey: string,
  sessionSnakeKey: string,
  idCamelKey: string,
  idSnakeKey: string,
): string[] {
  const changedIds: string[] = [];
  for (const entry of entries) {
    if (!isRecord(entry)) continue;
    if (
      runtimeEntryString(entry, sessionCamelKey, sessionSnakeKey) !==
      oldSessionId
    ) {
      continue;
    }
    entry[sessionCamelKey] = newSessionId;
    if (sessionSnakeKey !== sessionCamelKey) delete entry[sessionSnakeKey];
    const id = runtimeEntryString(entry, idCamelKey, idSnakeKey);
    if (id !== undefined) changedIds.push(id);
  }
  return changedIds;
}

function runtimeRebuildAffectedIds(
  plan: ServiceRuntimeRebuildPlan,
): Record<string, string | number> {
  const affected: Record<string, string | number> = {
    profileId: plan.profileId,
    sessionCount: plan.sessionIds.length,
  };
  if (plan.sessionIds.length === 1) {
    affected.sessionId = plan.sessionIds[0] ?? "";
  }
  return affected;
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
    const pause = runtimePauseForSession(state, session);
    if (pause !== undefined) {
      state.claimedDeliveryIntentIds.add(intent.id);
      recordDynamicDenDeliveryChannel(state, intent, session, {
        channelId: channelIdFromDeliveryIntent(intent),
        sourceMessageId: intent.channel_message_id,
        wakePolicy: decision.wakePolicy,
        subscriptionStatus: "runtime_paused",
        lastError: runtimePauseSummary(pause, session.sessionId),
      });
      void rejectPausedDenDeliveryIntent(state, intent, session, pause).catch(
        (error) =>
          recordServiceEvent(state, {
            source: "den-successor-gateway",
            eventType: "den_delivery_intent_runtime_pause_reject_failed",
            severity: "error",
            summary: errorMessage(
              error,
              `Den Delivery intent ${intent.id} runtime pause reject failed`,
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

async function rejectPausedDenDeliveryIntent(
  state: ServiceState,
  intent: DenSuccessorDeliveryIntent,
  session: RustyCrewRuntimeConfig["sessions"][number],
  pause: RuntimePauseRecord,
): Promise<void> {
  if (state.denGatewayClient === undefined) return;
  const claimToken = `rusty-crew:${intent.id}:${Date.now()}`;
  const claimedBy = intent.target_identity;
  const summary = runtimePauseSummary(pause, session.sessionId);
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
      reason: "runtime_paused",
      summary,
      pause_id: pause.pauseId,
      pause_scope: pause.scope,
      pause_target_id: pause.targetId,
    },
  });
  recordServiceEvent(state, {
    source: "den-successor-gateway",
    eventType: "den_delivery_intent_runtime_paused",
    severity: "warning",
    summary: `Rejected Den Delivery intent ${intent.id} for ${session.agentId}: ${summary}.`,
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
  const slotId = stableChatRecordId("slot", messageId);
  const primaryVariantId = stableChatRecordId("variant", slotId);
  const now = state.now();
  const branch = await ensureDefaultConversationBranch(
    state,
    input.session,
    now,
  );
  await state.bridge.saveMessageSlot({
    slot_id: slotId,
    session_id: input.session.sessionId,
    primary_variant_id: primaryVariantId,
    active_variant_id: null,
    metadata_json: {
      source: "rusty_view_chat",
      correlation_id: correlationId,
      reason: input.reason,
    },
    created_at: now,
    updated_at: now,
  });
  await state.bridge.saveMessageVariant(
    messageVariantWrite({
      sessionId: input.session.sessionId,
      slotId,
      variantId: primaryVariantId,
      messageId,
      source: "primary",
      ordinal: 0,
      actor: input.actor,
      body: input.body,
      branchId: branch.branch_id,
      parentMessageId: branch.head_message_id ?? undefined,
      previousMessageId: branch.head_message_id ?? undefined,
      metadataJson: {
        source: "rusty_view_chat",
        correlation_id: correlationId,
        reason: input.reason,
      },
      now,
    }),
  );
  const inbound = appendChatEvent(state, input.session.sessionId, {
    kind: "message_created",
    payload: {
      message_id: messageId,
      slot_id: slotId,
      primary_variant_id: primaryVariantId,
      branch_id: branch.branch_id,
      parent_message_id: branch.head_message_id,
      previous_message_id: branch.head_message_id,
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
  await state.bridge.updateConversationBranchHead({
    branch_id: branch.branch_id,
    head_message_id: messageId,
    expected: { type: "any" },
    updated_at: state.now(),
  });
  const result: SendChatMessageResult = {
    status: wakeReport.status === "completed" ? "accepted" : "rejected",
    message_id: messageId,
    slot_id: slotId,
    primary_variant_id: primaryVariantId,
    wake_id: wakeReport.wakeId,
    correlation_id: correlationId,
    latest_cursor:
      latestChatCursor(state, input.session.sessionId) ?? inbound.event_id,
    reason_code: wakeReport.reasonCode,
  };
  rememberChatMessageReceipt(state, receiptKey, result);
  return result;
}

async function listRustyViewMessageSlots(
  state: ServiceState,
  input: ListMessageSlotsInput,
): Promise<MessageSlotPage> {
  const items = (await state.bridge.queryMessageSlots({
    session_id: input.session.sessionId,
    include_alternates: input.includeAlternates,
    page: { limit: input.limit, offset: input.offset },
  })) as MessageSlotRecord[];
  return {
    items,
    total: input.offset + items.length,
    limit: input.limit,
    offset: input.offset,
    ...(items.length >= input.limit
      ? { nextOffset: input.offset + items.length }
      : {}),
  };
}

async function searchRustyViewTranscript(
  state: ServiceState,
  input: SearchTranscriptInput,
): Promise<TranscriptSearchResultPage> {
  const sessions =
    input.scope === "current_session" && input.session
      ? [input.session]
      : (await state.bridge.listSessions()).filter(
          (session) =>
            (input.sessionId === undefined ||
              session.sessionId === input.sessionId) &&
            (input.profileId === undefined ||
              session.profileId === input.profileId),
        );
  const query = input.query.trim();
  const loweredQuery = query.toLowerCase();
  const results: TranscriptSearchResult[] = [];
  for (const session of sessions) {
    const slots = (await state.bridge.queryMessageSlots({
      session_id: session.sessionId,
      include_alternates: true,
      page: { limit: 500, offset: 0 },
    })) as MessageSlotRecord[];
    for (const slot of slots) {
      for (const variant of [slot.primary, ...slot.alternates]) {
        if (variant.status === "deleted") continue;
        const message = variant.message;
        if (input.role !== undefined && message.author_role !== input.role) {
          continue;
        }
        if (
          input.createdAfter !== undefined &&
          message.created_at < input.createdAfter
        ) {
          continue;
        }
        if (
          input.createdBefore !== undefined &&
          message.created_at > input.createdBefore
        ) {
          continue;
        }
        const matchIndex = message.body.toLowerCase().indexOf(loweredQuery);
        if (matchIndex < 0) continue;
        const snippet = transcriptSnippet(
          message.body,
          matchIndex,
          query.length,
        );
        results.push({
          result_id: stableChatRecordId(
            "search-result",
            `${session.sessionId}:${message.message_id}:${variant.variant_id}:${matchIndex}`,
          ),
          scope: input.scope,
          session_id: session.sessionId,
          slot_id: slot.slot_id,
          variant_id: variant.variant_id,
          message_id: message.message_id,
          branch_id: message.branch_id ?? null,
          author_role: message.author_role,
          created_at: message.created_at,
          snippet: snippet.text,
          highlights: [
            {
              start: snippet.highlightStart,
              end: snippet.highlightEnd,
            },
          ],
          jump: {
            session_id: session.sessionId,
            target: { type: "message", message_id: message.message_id },
            branch_id: message.branch_id ?? null,
            message_id: message.message_id,
            cursor: null,
            snapshot_id: null,
          },
          source: "rust_coordination",
        });
      }
    }
  }
  results.sort((left, right) =>
    left.created_at === right.created_at
      ? left.result_id.localeCompare(right.result_id)
      : left.created_at.localeCompare(right.created_at),
  );
  const items = results.slice(input.offset, input.offset + input.limit);
  return {
    items,
    total: results.length,
    limit: input.limit,
    offset: input.offset,
    ...(input.offset + items.length < results.length
      ? { nextOffset: input.offset + items.length }
      : {}),
    query,
    scope: input.scope,
    source: "rust_coordination",
  };
}

async function rustyViewConversationTree(
  state: ServiceState,
  input: ConversationTreeInput,
): Promise<ConversationTreeProjection> {
  const branches = (await state.bridge.queryConversationBranches({
    session_id: input.session.sessionId,
    page: { limit: input.limit, offset: input.offset },
  })) as ConversationBranchRecord[];
  const snapshots = input.includeSnapshots
    ? ((await state.bridge.queryConversationSnapshots({
        session_id: input.session.sessionId,
        page: { limit: input.limit, offset: input.offset },
      })) as ConversationSnapshotRecord[])
    : [];
  const branchState = await getRustyViewConversationBranchState(state, {
    session: input.session,
  });
  return {
    branches,
    snapshots,
    branch_state: branchState,
    active_branch_id: branchState.active_branch_id,
  };
}

async function getRustyViewConversationBranchState(
  state: ServiceState,
  input: ConversationBranchStateInput,
): Promise<ConversationBranchStateRecord> {
  return (await state.bridge.getConversationBranchState({
    session_id: input.session.sessionId,
    default_updated_at: state.now(),
  })) as ConversationBranchStateRecord;
}

async function createRustyViewConversationBranch(
  state: ServiceState,
  input: CreateConversationBranchInput,
): Promise<ConversationBranchMutationResult> {
  const now = state.now();
  const branchId =
    input.request.branch_id ??
    stableChatRecordId(
      "branch",
      `${input.session.sessionId}:${input.requestId}`,
    );
  const branch = (await state.bridge.saveConversationBranch({
    branch_id: branchId,
    session_id: input.session.sessionId,
    parent_branch_id: input.request.parent_branch_id ?? null,
    parent_message_id: input.request.parent_message_id ?? null,
    origin_message_id: input.request.origin_message_id ?? null,
    head_message_id: input.request.head_message_id ?? null,
    label: input.request.label ?? null,
    metadata_json: input.request.metadata_json ?? {},
    created_at: now,
    updated_at: now,
  })) as ConversationBranchRecord;
  const event = appendChatEvent(state, input.session.sessionId, {
    kind: "conversation_branch_created",
    payload: { branch },
  });
  return { status: "created", branch, latest_cursor: event.event_id };
}

async function selectRustyViewActiveConversationBranch(
  state: ServiceState,
  input: SelectActiveConversationBranchInput,
): Promise<SelectActiveConversationBranchResult> {
  const result = (await state.bridge.selectActiveConversationBranch({
    session_id: input.session.sessionId,
    active_branch_id: input.request.active_branch_id ?? null,
    expected: input.request.expected,
    updated_at: state.now(),
  })) as {
    state: ConversationBranchStateRecord;
    conflict?: { expected?: string | null; actual?: string | null } | null;
  };
  const status = result.conflict ? "conflict" : "selected";
  const event = appendChatEvent(state, input.session.sessionId, {
    kind: "conversation_active_branch_selected",
    payload: {
      active_branch_id: result.state.active_branch_id,
      conflict: result.conflict,
      state: result.state,
    },
  });
  return {
    status,
    state: result.state,
    ...(result.conflict ? { conflict: result.conflict } : {}),
    latest_cursor: event.event_id,
  };
}

async function updateRustyViewConversationBranchHead(
  state: ServiceState,
  input: UpdateConversationBranchHeadInput,
): Promise<UpdateConversationBranchHeadResult> {
  const result = (await state.bridge.updateConversationBranchHead({
    branch_id: input.branchId,
    head_message_id: input.request.head_message_id ?? null,
    expected: input.request.expected,
    updated_at: state.now(),
  })) as {
    branch: ConversationBranchRecord;
    conflict?: { expected?: string | null; actual?: string | null } | null;
  };
  const status = result.conflict ? "conflict" : "updated";
  const event = appendChatEvent(state, input.session.sessionId, {
    kind: "conversation_branch_head_updated",
    payload: {
      branch_id: input.branchId,
      head_message_id: result.branch.head_message_id,
      conflict: result.conflict,
      branch: result.branch,
    },
  });
  return {
    status,
    branch: result.branch,
    ...(result.conflict ? { conflict: result.conflict } : {}),
    latest_cursor: event.event_id,
  };
}

async function createRustyViewConversationSnapshot(
  state: ServiceState,
  input: CreateConversationSnapshotInput,
): Promise<ConversationSnapshotMutationResult> {
  const now = state.now();
  const snapshotId =
    input.request.snapshot_id ??
    stableChatRecordId(
      "snapshot",
      `${input.session.sessionId}:${input.requestId}`,
    );
  const snapshot = (await state.bridge.saveConversationSnapshot({
    snapshot_id: snapshotId,
    session_id: input.session.sessionId,
    branch_id: input.request.branch_id ?? null,
    message_id: input.request.message_id ?? null,
    cursor: input.request.cursor ?? null,
    label: input.request.label ?? null,
    summary: input.request.summary ?? null,
    source: input.request.source ?? "user",
    metadata_json: input.request.metadata_json ?? {},
    created_at: now,
    updated_at: now,
  })) as ConversationSnapshotRecord;
  const event = appendChatEvent(state, input.session.sessionId, {
    kind: "conversation_snapshot_created",
    payload: { snapshot },
  });
  return { status: "created", snapshot, latest_cursor: event.event_id };
}

async function resolveRustyViewConversationJump(
  state: ServiceState,
  input: ResolveConversationJumpInput,
): Promise<ConversationJumpResult> {
  return (await state.bridge.resolveConversationJump({
    session_id: input.session.sessionId,
    target: input.target,
  })) as ConversationJumpResult;
}

async function createRustyViewAttachment(
  state: ServiceState,
  input: CreateAttachmentInput,
): Promise<AttachmentMutationResult> {
  const now = state.now();
  const attachmentId =
    input.request.attachment_id ??
    stableChatRecordId(
      "attachment",
      `${input.session.sessionId}:${input.requestId}`,
    );
  const existing = await findRustyViewAttachment(
    state,
    input.session.sessionId,
    attachmentId,
  );
  const link = attachmentLinkRecord({
    attachmentId,
    sessionId: input.session.sessionId,
    messageId: input.request.message_id ?? null,
    blockId: input.request.block_id ?? null,
    scopeId: input.request.scope_id ?? null,
    metadataJson: input.request.link_metadata_json ?? {},
    createdAt: now,
  });
  const attachment = (await state.bridge.saveAttachment({
    attachment_id: attachmentId,
    session_id: input.session.sessionId,
    status: "active",
    filename: input.request.filename,
    mime_type: input.request.mime_type,
    byte_size: input.request.byte_size,
    storage_url: input.request.storage_url ?? null,
    download_url: input.request.download_url ?? null,
    thumbnail_url: input.request.thumbnail_url ?? null,
    extracted_text: input.request.extracted_text ?? null,
    extracted_text_truncated: input.request.extracted_text_truncated ?? false,
    metadata_json: input.request.metadata_json ?? {},
    created_at: existing?.created_at ?? now,
    updated_at: now,
    expires_at: input.request.expires_at ?? null,
    link: link.message_id || link.block_id || link.scope_id ? link : undefined,
  })) as AttachmentRecord;
  const event = appendChatEvent(state, input.session.sessionId, {
    kind: existing ? "attachment_updated" : "attachment_uploaded",
    payload: { attachment },
  });
  if (link.message_id || link.block_id || link.scope_id) {
    appendChatEvent(state, input.session.sessionId, {
      kind: "attachment_linked",
      payload: { attachment_id: attachmentId, link, attachment },
    });
  }
  return {
    status: existing
      ? "updated"
      : link.scope_id || link.message_id || link.block_id
        ? "linked"
        : "created",
    attachment,
    latest_cursor:
      latestChatCursor(state, input.session.sessionId) ?? event.event_id,
  };
}

async function listRustyViewAttachments(
  state: ServiceState,
  input: ListAttachmentsInput,
): Promise<AttachmentPage> {
  const items = (await state.bridge.queryAttachments({
    session_id: input.session.sessionId,
    message_id: input.messageId,
    scope_id: input.scopeId,
    include_removed: input.includeRemoved,
    page: { limit: input.limit, offset: input.offset },
  })) as AttachmentRecord[];
  return {
    items,
    total: input.offset + items.length,
    limit: input.limit,
    offset: input.offset,
    ...(items.length >= input.limit
      ? { nextOffset: input.offset + items.length }
      : {}),
  };
}

async function removeRustyViewAttachment(
  state: ServiceState,
  input: RemoveAttachmentInput,
): Promise<AttachmentMutationResult> {
  const removed = (await state.bridge.removeAttachment({
    attachment_id: input.attachmentId,
    updated_at: state.now(),
  })) as AttachmentRecord;
  if (removed.session_id !== input.session.sessionId) {
    throw new Error(
      `attachment ${input.attachmentId} was not found for ${input.session.sessionId}`,
    );
  }
  const event = appendChatEvent(state, input.session.sessionId, {
    kind: "attachment_removed",
    payload: { attachment_id: input.attachmentId, attachment: removed },
  });
  return {
    status: "removed",
    attachment: removed,
    latest_cursor: event.event_id,
  };
}

async function createRustyViewDataBankScope(
  state: ServiceState,
  input: CreateDataBankScopeInput,
): Promise<DataBankScopeMutationResult> {
  const now = state.now();
  const scopeId =
    input.request.scope_id ??
    stableChatRecordId(
      "scope",
      `${input.session.sessionId}:${input.requestId}`,
    );
  const existing = await findRustyViewDataBankScope(
    state,
    input.session.sessionId,
    scopeId,
  );
  const scope = (await state.bridge.saveDataBankScope({
    scope_id: scopeId,
    session_id: input.session.sessionId,
    status: "active",
    label: input.request.label ?? null,
    description: input.request.description ?? null,
    metadata_json: input.request.metadata_json ?? {},
    created_at: existing?.created_at ?? now,
    updated_at: now,
  })) as DataBankScopeRecord;
  const event = appendChatEvent(state, input.session.sessionId, {
    kind: "data_bank_scope_created",
    payload: { scope },
  });
  return {
    status: existing ? "updated" : "created",
    scope,
    latest_cursor: event.event_id,
  };
}

async function listRustyViewDataBankScopes(
  state: ServiceState,
  input: ListDataBankScopesInput,
): Promise<DataBankScopePage> {
  const items = (await state.bridge.queryDataBankScopes({
    session_id: input.session.sessionId,
    include_removed: input.includeRemoved,
    page: { limit: input.limit, offset: input.offset },
  })) as DataBankScopeRecord[];
  return {
    items,
    total: input.offset + items.length,
    limit: input.limit,
    offset: input.offset,
    ...(items.length >= input.limit
      ? { nextOffset: input.offset + items.length }
      : {}),
  };
}

async function removeRustyViewDataBankScope(
  state: ServiceState,
  input: RemoveDataBankScopeInput,
): Promise<DataBankScopeMutationResult> {
  const removed = (await state.bridge.removeDataBankScope({
    scope_id: input.scopeId,
    updated_at: state.now(),
  })) as DataBankScopeRecord;
  if (removed.session_id !== input.session.sessionId) {
    throw new Error(
      `data-bank scope ${input.scopeId} was not found for ${input.session.sessionId}`,
    );
  }
  const event = appendChatEvent(state, input.session.sessionId, {
    kind: "data_bank_scope_removed",
    payload: { scope_id: input.scopeId, scope: removed },
  });
  return { status: "removed", scope: removed, latest_cursor: event.event_id };
}

async function ensureDefaultConversationBranch(
  state: ServiceState,
  session: ChatSendMessageInput["session"],
  now: string,
): Promise<ConversationBranchRecord> {
  const branchId = stableChatRecordId("branch", `${session.sessionId}:default`);
  const existing = (await state.bridge.queryConversationBranches({
    session_id: session.sessionId,
    page: { limit: 500, offset: 0 },
  })) as ConversationBranchRecord[];
  const found = existing.find((branch) => branch.branch_id === branchId);
  if (found) return found;
  const branch = (await state.bridge.saveConversationBranch({
    branch_id: branchId,
    session_id: session.sessionId,
    parent_branch_id: null,
    parent_message_id: null,
    origin_message_id: null,
    head_message_id: null,
    label: "Default",
    metadata_json: { source: "rusty_view_chat_default" },
    created_at: now,
    updated_at: now,
  })) as ConversationBranchRecord;
  await state.bridge
    .selectActiveConversationBranch({
      session_id: session.sessionId,
      active_branch_id: branchId,
      expected: { type: "none" },
      updated_at: now,
    })
    .catch(() => undefined);
  return branch;
}

async function listRustyViewMessageVariants(
  state: ServiceState,
  input: ListMessageVariantsInput,
): Promise<MessageVariantPage> {
  await requireMessageSlotForSession(
    state,
    input.session.sessionId,
    input.slotId,
  );
  const items = (await state.bridge.queryMessageVariants({
    slot_id: input.slotId,
    include_deleted: false,
    page: { limit: input.limit, offset: input.offset },
  })) as MessageVariantRecord[];
  return {
    items,
    total: input.offset + items.length,
    limit: input.limit,
    offset: input.offset,
  };
}

async function createRustyViewMessageSlot(
  state: ServiceState,
  input: CreateMessageSlotInput,
): Promise<MessageSlotMutationResult> {
  const now = state.now();
  const slotId =
    input.request.slot_id ??
    stableChatRecordId("slot", `${input.session.sessionId}:${input.requestId}`);
  const variantId =
    input.request.primary_variant_id ?? stableChatRecordId("variant", slotId);
  await state.bridge.saveMessageSlot({
    slot_id: slotId,
    session_id: input.session.sessionId,
    primary_variant_id: variantId,
    active_variant_id: null,
    metadata_json: input.request.metadata_json ?? {},
    created_at: now,
    updated_at: now,
  });
  await state.bridge.saveMessageVariant(
    messageVariantWrite({
      sessionId: input.session.sessionId,
      slotId,
      variantId,
      messageId:
        input.request.message_id ?? stableChatRecordId("message", variantId),
      source: "primary",
      ordinal: 0,
      actor: input.request.actor,
      body: input.request.body,
      metadataJson: input.request.variant_metadata_json ?? {},
      blocks: input.request.blocks,
      now,
    }),
  );
  const slot = await requireMessageSlotForSession(
    state,
    input.session.sessionId,
    slotId,
    true,
  );
  const event = appendChatEvent(state, input.session.sessionId, {
    kind: "message_slot_created",
    payload: { slot },
  });
  return { status: "created", slot, latest_cursor: event.event_id };
}

async function createRustyViewMessageVariant(
  state: ServiceState,
  input: CreateMessageVariantInput,
): Promise<MessageVariantMutationResult> {
  const slot = await requireMessageSlotForSession(
    state,
    input.session.sessionId,
    input.slotId,
    true,
  );
  const now = state.now();
  const variantId =
    input.request.variant_id ??
    stableChatRecordId("variant", `${input.slotId}:${input.requestId}`);
  const ordinal = slot.alternates.length + 1;
  const variant = (await state.bridge.saveMessageVariant(
    messageVariantWrite({
      sessionId: input.session.sessionId,
      slotId: input.slotId,
      variantId,
      messageId:
        input.request.message_id ?? stableChatRecordId("message", variantId),
      source: "alternate",
      ordinal,
      actor: input.request.actor,
      body: input.request.body,
      metadataJson: input.request.metadata_json ?? {},
      blocks: input.request.blocks,
      now,
    }),
  )) as MessageVariantRecord;
  const event = appendChatEvent(state, input.session.sessionId, {
    kind: "message_variant_created",
    payload: { slot_id: input.slotId, variant },
  });
  return { status: "created", variant, latest_cursor: event.event_id };
}

async function deleteRustyViewMessageVariant(
  state: ServiceState,
  input: DeleteMessageVariantInput,
): Promise<MessageSlotMutationResult> {
  await requireMessageSlotForSession(
    state,
    input.session.sessionId,
    input.slotId,
  );
  const slot = (await state.bridge.deleteMessageVariant({
    slot_id: input.slotId,
    variant_id: input.variantId,
    updated_at: state.now(),
  })) as MessageSlotRecord;
  const event = appendChatEvent(state, input.session.sessionId, {
    kind: "message_variant_deleted",
    payload: { slot_id: input.slotId, variant_id: input.variantId, slot },
  });
  return { status: "deleted", slot, latest_cursor: event.event_id };
}

async function reorderRustyViewMessageVariants(
  state: ServiceState,
  input: ReorderMessageVariantsInput,
): Promise<MessageVariantsReorderResult> {
  await requireMessageSlotForSession(
    state,
    input.session.sessionId,
    input.slotId,
  );
  const variants = (await state.bridge.reorderMessageVariants({
    slot_id: input.slotId,
    ordered_variant_ids: input.orderedVariantIds,
    updated_at: state.now(),
  })) as MessageVariantRecord[];
  const event = appendChatEvent(state, input.session.sessionId, {
    kind: "message_variants_reordered",
    payload: {
      slot_id: input.slotId,
      ordered_variant_ids: input.orderedVariantIds,
      variants,
    },
  });
  return { status: "reordered", variants, latest_cursor: event.event_id };
}

async function selectRustyViewActiveMessageVariant(
  state: ServiceState,
  input: SelectActiveMessageVariantInput,
): Promise<SelectActiveMessageVariantResult> {
  await requireMessageSlotForSession(
    state,
    input.session.sessionId,
    input.slotId,
  );
  const result = (await state.bridge.selectActiveMessageVariant({
    slot_id: input.slotId,
    active_variant_id: input.request.active_variant_id ?? null,
    expected: input.request.expected,
    updated_at: state.now(),
  })) as {
    slot: MessageSlotRecord;
    conflict?: { expected?: string | null; actual?: string | null } | null;
  };
  const status = result.conflict ? "conflict" : "selected";
  const event = appendChatEvent(state, input.session.sessionId, {
    kind: "message_active_variant_selected",
    payload: {
      slot_id: input.slotId,
      active_variant_id: result.slot.active_variant_id,
      conflict: result.conflict,
      slot: result.slot,
    },
  });
  return {
    status,
    slot: result.slot,
    ...(result.conflict ? { conflict: result.conflict } : {}),
    latest_cursor: event.event_id,
  };
}

async function requireMessageSlotForSession(
  state: ServiceState,
  sessionId: SessionId,
  slotId: string,
  includeAlternates = false,
): Promise<MessageSlotRecord> {
  const slots = (await state.bridge.queryMessageSlots({
    session_id: sessionId,
    include_alternates: includeAlternates,
    page: { limit: 500, offset: 0 },
  })) as MessageSlotRecord[];
  const slot = slots.find((candidate) => candidate.slot_id === slotId);
  if (!slot) {
    throw new Error(`message slot ${slotId} was not found for ${sessionId}`);
  }
  return slot;
}

function messageVariantWrite(input: {
  sessionId: SessionId;
  slotId: string;
  variantId: string;
  messageId: string;
  source: "primary" | "alternate";
  ordinal: number;
  actor: { id: string; kind: "human" | "agent" | "system" };
  body: string;
  branchId?: string | null;
  parentMessageId?: string | null;
  previousMessageId?: string | null;
  metadataJson: unknown;
  blocks?: MessageBlockDraft[];
  now: string;
}): Record<string, unknown> {
  return {
    variant_id: input.variantId,
    slot_id: input.slotId,
    source: input.source,
    ordinal: input.ordinal,
    status: "active",
    message: {
      message_id: input.messageId,
      session_id: input.sessionId,
      branch_id: input.branchId ?? null,
      parent_message_id: input.parentMessageId ?? null,
      previous_message_id: input.previousMessageId ?? null,
      author_id: input.actor.id,
      author_role:
        input.actor.kind === "agent"
          ? "assistant"
          : input.actor.kind === "system"
            ? "system"
            : "user",
      status: "completed",
      body: input.body,
      metadata_json: input.metadataJson ?? {},
      created_at: input.now,
      blocks: messageBlockWrites(input.messageId, input.body, input.blocks),
    },
    metadata_json: input.metadataJson ?? {},
    created_at: input.now,
    updated_at: input.now,
  };
}

function messageBlockWrites(
  messageId: string,
  body: string,
  blocks: MessageBlockDraft[] | undefined,
): Array<Record<string, unknown>> {
  const source =
    blocks && blocks.length > 0
      ? blocks
      : [{ kind: "text", content_json: { text: body }, metadata_json: {} }];
  return source.map((block, index) => ({
    block_id: block.block_id ?? `${messageId}:block:${index + 1}`,
    ordinal: index,
    kind: block.kind,
    content_json: block.content_json,
    render_policy_json: block.render_policy_json,
    metadata_json: block.metadata_json ?? {},
  }));
}

function stableChatRecordId(prefix: string, raw: string): string {
  return `${prefix}:${raw.replace(/[^A-Za-z0-9._:-]+/g, "_").slice(0, 160)}`;
}

function transcriptSnippet(
  body: string,
  matchIndex: number,
  queryLength: number,
): { text: string; highlightStart: number; highlightEnd: number } {
  const radius = 80;
  const start = Math.max(0, matchIndex - radius);
  const end = Math.min(body.length, matchIndex + queryLength + radius);
  const prefix = start > 0 ? "..." : "";
  const suffix = end < body.length ? "..." : "";
  const text = `${prefix}${body.slice(start, end)}${suffix}`;
  const highlightStart = prefix.length + matchIndex - start;
  return {
    text,
    highlightStart,
    highlightEnd: highlightStart + queryLength,
  };
}

async function findRustyViewAttachment(
  state: ServiceState,
  sessionId: SessionId,
  attachmentId: string,
): Promise<AttachmentRecord | undefined> {
  const records = (await state.bridge.queryAttachments({
    session_id: sessionId,
    include_removed: true,
    page: { limit: 1000, offset: 0 },
  })) as AttachmentRecord[];
  return records.find((record) => record.attachment_id === attachmentId);
}

async function findRustyViewDataBankScope(
  state: ServiceState,
  sessionId: SessionId,
  scopeId: string,
): Promise<DataBankScopeRecord | undefined> {
  const records = (await state.bridge.queryDataBankScopes({
    session_id: sessionId,
    include_removed: true,
    page: { limit: 1000, offset: 0 },
  })) as DataBankScopeRecord[];
  return records.find((record) => record.scope_id === scopeId);
}

function attachmentLinkRecord(input: {
  attachmentId: string;
  sessionId: SessionId;
  messageId?: string | null;
  blockId?: string | null;
  scopeId?: string | null;
  metadataJson: unknown;
  createdAt: string;
}): AttachmentRecord["links"][number] {
  const target = [
    input.messageId ?? "no-message",
    input.blockId ?? "no-block",
    input.scopeId ?? "no-scope",
  ].join(":");
  return {
    link_id: stableChatRecordId(
      "attachment-link",
      `${input.attachmentId}:${target}`,
    ),
    attachment_id: input.attachmentId,
    session_id: input.sessionId,
    message_id: input.messageId ?? null,
    block_id: input.blockId ?? null,
    scope_id: input.scopeId ?? null,
    metadata_json: input.metadataJson,
    created_at: input.createdAt,
  };
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
  const session = (await state.bridge.listSessions().catch(() => [])).find(
    (candidate) => candidate.sessionId === input.sessionId,
  );
  const pause =
    session === undefined ? undefined : runtimePauseForSession(state, session);
  if (pause !== undefined) {
    return runtimePauseWakeReport(state, input.sessionId, pause);
  }
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
    const pause = runtimePauseForSession(state, session);
    if (pause !== undefined) {
      return runtimePauseWakeReport(state, sessionId, pause);
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
    const sessionMemoryContext = await buildSessionMemoryContextForWake(state, {
      session,
      configuredSession: configured,
      profileContext,
    });
    const role = buildProfileRoleAssembly(profileContext, {
      sessionMemoryContext,
    });
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

function runtimePauseWakeReport(
  state: ServiceState,
  sessionId: SessionId,
  pause: RuntimePauseRecord,
): ServiceWakeDispatchReport {
  return wakeDispatchSkipped(
    state,
    sessionId,
    "runtime_paused",
    runtimePauseSummary(pause, sessionId),
  );
}

function runtimePauseSummary(
  pause: RuntimePauseRecord,
  sessionId: string,
): string {
  const reason = pause.reason ? `: ${pause.reason}` : "";
  return `runtime wake for ${sessionId} is paused by ${pause.scope} ${pause.targetId}${reason}`;
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
