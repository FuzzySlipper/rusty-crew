import type { IncomingMessage, ServerResponse } from "node:http";
import { createHash, randomBytes } from "node:crypto";
import { createReadStream, existsSync } from "node:fs";
import { mkdir, readFile, rename, rm, stat, writeFile } from "node:fs/promises";
import {
  basename,
  dirname,
  extname,
  join,
  relative,
  resolve,
  sep,
} from "node:path";
import type {
  BrainEvent,
  BrainImplementationId,
  BrainModelConfig,
  AgentId,
  BrainImplementationHandle,
  ChannelBindingRecord,
  ChannelMembershipStatus,
  ChannelSubscriptionStatus,
  CompletionPacket,
  CoreEvent,
  EngineHandle,
  EngineStorageConfig,
  McpBindingRecord,
  ProfileId,
  ScheduledRunSummary,
  SessionId,
  SessionKind,
  SessionState,
  SubscriptionHandle,
  ToolCallMetadata,
} from "@rusty-crew/contracts";
import {
  loadNativeBridge,
  type NativeProfileMemoryRecord,
  type NativeBridgeModule,
  type NativeCreateProfilePlan,
  type NativeModelProviderRecord,
  type NativeModelProviderProtocol,
  type NativeModelProviderQuery,
  type NativeModelProviderStatus,
  type NativeModelProviderWrite,
  type NativeProfilePurgeReport,
  type NativeProfileRegistryLifecycleStatus,
  type NativeProfileRegistryRecord,
  type NativeProfileRegistryWrite,
  type NativeRoleplayChatLayersWrite,
  type NativeRoleplayLoreEntryPromotion,
  type NativeRoleplayLoreFactCapture,
  type NativeRoleplayLoreLayerWrite,
  type NativeRoleplayLoreQuery,
  type NativeRoleplayLoreReplace,
  type NativeRoleplayLoreWrite,
  type NativeSimpleKvRecord,
} from "@rusty-crew/native-bridge";
import type {
  ChannelBindingDiagnostics,
  DenSuccessorAgentIdentity,
  DenSuccessorConversationChannel,
  DenSuccessorConversationMembership,
  DenSuccessorDeliveryIntent,
  DenSuccessorGatewayClient,
  McpSurfaceManagerPort,
  TelegramChannelConnectorPort,
} from "./service-adapter-ports.js";
import {
  AgentActivityObservationProducer,
  type AgentActivityEventInput,
  type AgentActivityObservationEvent,
  type AgentActivityObservationSink,
  type AgentActivityWorkRef,
} from "./agent-activity-observation.js";
import {
  runtimeCoreEventObservationInput,
  type RuntimeObservationSessionIdentity,
} from "./runtime-core-event-observation.js";
import {
  deliveryIntentWakeDecision,
  normalizeChannelWakePolicy,
  type ChannelWakePolicy,
  type DeliveryIntentWakeDecision,
} from "./channel-wake-policy.js";
import {
  isCorrelatedReply,
  replyFromEvent,
  type CoordinationToolRuntime,
} from "./coordination-tools.js";
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
  createLocalToolProfileStore,
  LocalToolProfileError,
  type LocalToolProfileWrite,
} from "./local-tool-profiles.js";
import {
  handleAdminDiagnosticsRequest,
  type AdminDiagnosticsContext,
  type MemorySpaceDiagnosticsProjection,
  type AdminRouteResult,
} from "./admin-diagnostics-api.js";
import { handleAdminContextStrategiesRequest } from "./service-context-strategy-routes.js";
import {
  handleAdminMcpCatalogRequest,
  mcpServerCatalogEntries,
} from "./service-mcp-catalog-routes.js";
import { failure, successRoute } from "./service-route-results.js";
import { handleSchedulerReadRequest } from "./service-scheduler-routes.js";
import { handleAdminToolsCatalogRequest } from "./service-tool-catalog-routes.js";
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
  contextStrategyCatalog,
  contextStrategyDescriptor,
  contextStrategyPolicyFromPatch,
  contextStrategyPolicyFromUnknown,
  defaultContextStrategyPolicy,
  prepareContextStrategyRoleAssembly,
  type ContextStrategyPolicy,
} from "./context-strategy.js";
import {
  estimateContextUsage,
  textFragmentsFromPayload,
} from "./context-estimate.js";
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
  type ToolDiagnosticsProjection,
  type RuntimeSessionEffectiveDefaults,
  type RuntimePauseDiagnostics,
  type RuntimeResponsesWakeMetrics,
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
  type ProviderRequestDebugDetail,
  type ReorderMessageVariantsInput,
  type RemoveAttachmentInput,
  type RemoveDataBankScopeInput,
  type SessionContextUsageResult,
  type SelectActiveMessageVariantInput,
  type SelectActiveMessageVariantResult,
  type SelectActiveConversationBranchInput,
  type SelectActiveConversationBranchResult,
  type SendChatMessageResult,
  type ResolveConversationJumpInput,
  type SearchTranscriptInput,
  type ToolCallDebugDetail,
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
import { runStructuredCaptureProvider } from "./capture-producer-provider.js";
import { buildSessionActivityDigest } from "./session-activity-digest.js";
import {
  MemoryToolCallDebugStore,
  type ToolCallDebugStore,
} from "./tool-call-debug-store.js";
import {
  MemoryProviderRequestDebugStore,
  type ProviderRequestDebugStore,
} from "./provider-request-debug-store.js";
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
  type RustyCrewMcpServerConfig,
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
  type ServiceBrainWakeResultObservation,
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
import {
  buildBuiltInToolCatalog,
  defaultToolRegistry,
} from "./tool-registry.js";
import type { ServiceAdapterFactories } from "./service-adapter-ports.js";
import { ChatEventStore } from "./chat-event-store.js";

const CHAT_EVENT_RETENTION_LIMIT = 50_000;

export interface RustyCrewServiceAppOptions {
  env?: RustyCrewServiceEnv;
  config?: RustyCrewServiceConfig;
  bridge?: NativeBridgeModule;
  adapterFactories: ServiceAdapterFactories;
  toolCallDebugStore?: ToolCallDebugStore;
  now?: () => string;
}

export interface RustyCrewServiceApp {
  readonly config: RustyCrewServiceConfig;
  readonly bridge: NativeBridgeModule;
  readonly engine: EngineHandle;
  readonly adminHost: string;
  readonly adminPort: number;
  readonly url: string;
  handle(request: IncomingMessage, response: ServerResponse): void;
  stop(): Promise<void>;
}

interface ServiceState {
  readonly config: RustyCrewServiceConfig;
  readonly bridge: NativeBridgeModule;
  readonly engine: EngineHandle;
  readonly lock: RustyCrewServiceLock;
  readonly auditSink: ReturnType<typeof createMemoryAdminControlAuditSink>;
  readonly adapterFactories: ServiceAdapterFactories;
  runtimeConfig: RustyCrewRuntimeConfig;
  runtimeConfigApplyResult: RustyCrewRuntimeConfigApplyResult;
  denGatewayClient?: DenSuccessorGatewayClient;
  denGatewayStartupReport?: DenSuccessorGatewayStartupReport;
  denObservationSubscription?: SubscriptionHandle;
  telegramConnector?: TelegramChannelConnectorPort;
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
  readonly openAiOauthPendingLogins: Map<string, OpenAiOauthPendingLogin>;
  readonly channelProjectionFailures: ChannelProjectionFailureRecord[];
  profileChannelWakePolicies: Map<string, ChannelWakePolicy>;
  mcpManager: McpSurfaceManagerPort;
  readonly wakeSubscription: SubscriptionHandle;
  readonly timers: Set<NodeJS.Timeout>;
  readonly inFlightWakes: Set<SessionId>;
  readonly runtimePauses: Map<string, RuntimePauseRecord>;
  readonly claimedDeliveryIntentIds: Set<number>;
  readonly unmatchedDeliveryIntentIds: Set<number>;
  readonly directDispatchSessions: Set<SessionId>;
  readonly chatMessageReceipts: Map<string, SendChatMessageResult>;
  readonly chatEventStore: ChatEventStore;
  readonly chatEventsBySession: Map<SessionId, ChatEvent[]>;
  readonly chatSequencesBySession: Map<SessionId, number>;
  readonly chatSubscribersBySession: Map<SessionId, Set<ChatStreamSubscriber>>;
  readonly toolCallDebugStore: ToolCallDebugStore;
  readonly providerRequestDebugStore: ProviderRequestDebugStore;
  readonly responsesWakeMetrics: RuntimeResponsesWakeMetrics[];
  readonly suppressedWakeEvents: Map<SessionId, number>;
  readonly recentEvents: ServiceRecentEvent[];
  schedulerHeartbeat: ServiceSchedulerHeartbeatState;
  readonly now: () => string;
  nextWakeSequence: number;
  stopping: boolean;
}

interface RoleplayCharacterRecord {
  id: string;
  profileId: string;
  name: string;
  description: string;
  personality: string;
  scenario: string;
  firstMessage: string;
  alternateGreetings: string[];
  exampleMessages: string[];
  tags: string[];
  avatarUrl?: string;
  status: "active" | "archived";
  createdAt: string;
  updatedAt?: string;
}

interface RoleplaySessionMetadata {
  sessionId: string;
  profileId: string;
  displayName?: string;
  characterId?: string;
  activeLayerIds: string[];
  archived: boolean;
  createdAt: string;
  updatedAt: string;
}

interface RoleplayChatLayersBrowserWrite extends NativeRoleplayChatLayersWrite {
  chat_id: string;
  layers: Array<{ layer_id: string; priority: number; enabled: boolean }>;
}

type RoleplayNarratorTone =
  | "whimsical"
  | "dramatic"
  | "matter_of_fact"
  | "lush"
  | "wry";

type RoleplayNarratorPacing = "leisurely" | "balanced" | "rapid" | "breathless";

type RoleplayNarratorExplicitness =
  | "implied"
  | "suggestive"
  | "romantic"
  | "steamy";

type RoleplayNarratorMemoryDepth = "shallow" | "medium" | "deep";

interface BrowserRoleplayNarratorConfig {
  tone: RoleplayNarratorTone;
  pacing: RoleplayNarratorPacing;
  explicitness: RoleplayNarratorExplicitness;
  memoryDepth: RoleplayNarratorMemoryDepth;
  stylePrompt?: string;
  exemplar?: string;
  review: {
    enabled: boolean;
    maxReviewCycles: number;
  };
}

interface OpenAiOauthPendingLogin {
  pendingLoginId: string;
  providerAlias: string;
  issuer: string;
  clientId: string;
  redirectUri: string;
  scopes: string[];
  state: string;
  codeVerifier: string;
  codeChallenge: string;
  authorizationUrl: string;
  createdAt: string;
  expiresAt: string;
}

interface DenConversationChannelResolution {
  channelId: number;
  projectId: string;
  slug: string;
}

interface ServiceBackgroundReviewRuntime {
  enabled: boolean;
  recentFindings: number;
  lastCaptureProposalCount?: number;
  lastPersistedCaptureProposalCount?: number;
  lastSkippedReasons?: readonly string[];
  lastRunAt?: string;
  lastError?: string;
}

interface ServiceSchedulerHeartbeatState {
  enabled: boolean;
  intervalMs: number;
  running: boolean;
  lastStartedAt?: string;
  lastCompletedAt?: string;
  lastDurationMs?: number;
  lastSummary?: string;
  lastSkippedAt?: string;
  lastSkipReason?: string;
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

export async function createRustyCrewServiceApp(
  options: RustyCrewServiceAppOptions,
): Promise<RustyCrewServiceApp> {
  const serviceEnv = options.env ?? process.env;
  const config = options.config ?? loadRustyCrewServiceConfig(serviceEnv);

  ensureRustyCrewServiceDirectories(config);
  const lock = acquireRustyCrewServiceLock(config);
  const bridge = options.bridge ?? (await loadNativeBridge());
  let engine: EngineHandle | undefined;

  try {
    const runtimeConfig = await loadRustyCrewRuntimeConfig(config);
    const storage = runtimeConfig.storage ?? config.storage;
    engine = await bridge.initializeEngine({
      engineDataDir: config.paths.engineDataDir,
      clock: "system",
      defaultTurnBudget: 16,
      defaultIdleTimeoutMs: 30_000,
      storage: engineStorageConfig(storage, serviceEnv),
    });
    const profileChannelWakePolicies =
      await loadProfileChannelWakePolicies(runtimeConfig);
    const mcpManager = await createServiceMcpManager(
      runtimeConfig,
      options.adapterFactories,
    );
    const curator = createServiceCuratorRuntime({
      config,
      runtimeConfig,
      bridge,
      now: options.now ?? (() => new Date().toISOString()),
    });
    let liveState: ServiceState | undefined;
    const toolCallDebugStore =
      options.toolCallDebugStore ??
      new MemoryToolCallDebugStore({
        now: options.now,
      });
    const providerRequestDebugStore = new MemoryProviderRequestDebugStore({
      now: options.now,
    });
    const runtimeConfigApplyResult = await applyRustyCrewRuntimeConfig({
      serviceConfig: config,
      runtimeConfig,
      bridge,
      curatorExecutor: curator.executor,
      mcpSurfaceDiagnostics: mcpManager.diagnostics(),
      adapterFactories: options.adapterFactories,
      coordinationRuntime: createServiceCoordinationRuntime(() => liveState),
      toolCallDebugStore,
      providerRequestDebugStore,
      onBrainWakeResult: (observation) => {
        const state = liveState;
        if (state === undefined) return;
        recordResponsesWakeMetrics(state, observation);
      },
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
      adapterFactories: options.adapterFactories,
      runtimeConfig,
      runtimeConfigApplyResult,
      denGatewayClient:
        config.denSuccessorGateway === undefined
          ? undefined
          : options.adapterFactories.createDenSuccessorGatewayClient(
              config.denSuccessorGateway,
            ),
      denConversationChannelResolutionsByBindingId: new Map(),
      denConversationChannelIdsByExternalId: new Map(),
      denConversationMembershipsByBindingId: new Map(),
      dynamicDenChannelBindings: new Map(),
      openAiOauthPendingLogins: new Map(),
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
      chatEventStore: new ChatEventStore(
        join(config.paths.dataDir, "data", "chat-events"),
      ),
      chatEventsBySession: new Map(),
      chatSequencesBySession: new Map(),
      chatSubscribersBySession: new Map(),
      toolCallDebugStore,
      providerRequestDebugStore,
      responsesWakeMetrics: [],
      suppressedWakeEvents: new Map(),
      recentEvents: [],
      schedulerHeartbeat: {
        enabled: config.background.schedulerTickIntervalMs > 0,
        intervalMs: config.background.schedulerTickIntervalMs,
        running: false,
      },
      now: options.now ?? (() => new Date().toISOString()),
      nextWakeSequence: 0,
      stopping: false,
    };
    liveState = state;
    state.denGatewayStartupReport = await connectDenSuccessorGateway(state);
    await startDenObservationProjection(state);
    await ensureDenConversationChannels(state);
    await startTelegramConnector(state);
    startServiceBackgroundLoops(state);

    return {
      config,
      bridge,
      engine,
      adminHost: config.admin.host,
      adminPort: config.admin.port,
      url: `http://${config.admin.host}:${config.admin.port}`,
      handle: (request, response) => {
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
      },
      stop: () => stopService(state),
    };
  } catch (error) {
    if (engine !== undefined) {
      await bridge
        .shutdownEngine({ engine, drainTimeoutMs: 2_000 })
        .catch(() => undefined);
    }
    lock.release();
    throw error;
  }
}

function assertServiceStorageBootAllowed(
  storage: RustyCrewStorageConfig,
  context: string,
): void {
  if (
    storage.backend === "postgres" &&
    storage.postgres.bootMode !== "active"
  ) {
    throw new Error(
      `${context}: storage.backend=postgres requires storage.postgres.bootMode=active for full service startup; current mode is ${storage.postgres.bootMode}. This fails closed so the service cannot silently fall back to SQLite.`,
    );
  }
}

function engineStorageConfig(
  storage: RustyCrewStorageConfig,
  env: RustyCrewServiceEnv,
): EngineStorageConfig {
  if (storage.backend === "sqlite") {
    return { backend: "sqlite" };
  }
  if (storage.postgres.bootMode !== "active") {
    throw new Error(
      `storage.backend=postgres requires storage.postgres.bootMode=active for full service startup; current mode is ${storage.postgres.bootMode}`,
    );
  }
  const databaseUrl = (env as Record<string, string | undefined>)[
    storage.postgres.databaseUrlEnv
  ];
  if (databaseUrl === undefined || databaseUrl.trim() === "") {
    throw new Error(
      `storage.backend=postgres requires ${storage.postgres.databaseUrlEnv} to be set`,
    );
  }
  return {
    backend: "postgres",
    databaseUrl,
    schema: storage.postgres.schema,
    maxConnections: storage.postgres.maxConnections,
    statementTimeoutMs: storage.postgres.statementTimeoutMs,
  };
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
    isBrowserCorsRoute(url.pathname) &&
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
      : isRoleplayBrowserRoute(url.pathname)
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
        getToolCallDebugDetail: (input) =>
          rustyViewToolCallDebugDetail(state, input),
        getProviderRequestDebugDetail: (input) =>
          rustyViewProviderRequestDebugDetail(state, input),
        executeCommand: (input) => executeRustyViewChatCommand(state, input),
        contextUsage: (input) => rustyViewSessionContextUsage(state, input),
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
    return handleSchedulerReadRequest(
      {
        method: request.method ?? "GET",
        url,
        requestId: requestId(request),
      },
      {
        listScheduledJobs: (input) => state.bridge.listScheduledJobs(input),
        listScheduledRuns: (input) => state.bridge.listScheduledRuns(input),
      },
    );
  }

  if (url.pathname === "/v1/admin/mcp/catalog") {
    return handleAdminMcpCatalogRequest(
      { method: request.method ?? "GET", requestId: requestId(request) },
      { config: state.config, runtimeConfig: state.runtimeConfig },
    );
  }

  if (
    url.pathname === "/v1/admin/mcp/servers" ||
    url.pathname.startsWith("/v1/admin/mcp/servers/")
  ) {
    return handleAdminMcpServerRegistryRequest(request, state, url);
  }

  if (
    url.pathname === "/v1/admin/tools/catalog" ||
    url.pathname === "/v1/admin/tool-policy/catalog"
  ) {
    return handleAdminToolsCatalogRequest({
      method: request.method ?? "GET",
      requestId: requestId(request),
    });
  }

  if (url.pathname === "/v1/admin/context-strategies") {
    return handleAdminContextStrategiesRequest({
      method: request.method ?? "GET",
      requestId: requestId(request),
    });
  }

  if (
    url.pathname === "/v1/admin/local-tool-profiles" ||
    url.pathname.startsWith("/v1/admin/local-tool-profiles/")
  ) {
    return handleAdminLocalToolProfilesRequest(request, state, url);
  }

  if (isRoleplayBrowserRoute(url.pathname)) {
    return withChatCors(
      await handleAdminRoleplayRequest(request, state, url),
      request,
    );
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

  if (url.pathname.startsWith("/v1/admin/model-providers")) {
    const body =
      (request.method ?? "GET").toUpperCase() === "POST" ||
      (request.method ?? "GET").toUpperCase() === "PATCH"
        ? await readJsonBody(request)
        : undefined;
    return handleModelProviderAdminRequest(
      {
        method: request.method ?? "GET",
        url: url.toString(),
        body,
        requestId: requestId(request),
      },
      state,
    );
  }

  if (isProfileRegistryWriteRoute(url.pathname)) {
    const body =
      (request.method ?? "GET").toUpperCase() === "POST" ||
      (request.method ?? "GET").toUpperCase() === "PATCH"
        ? await readJsonBody(request)
        : undefined;
    return handleProfileRegistryWriteRequest(
      {
        method: request.method ?? "GET",
        url: url.toString(),
        body,
        requestId: requestId(request),
      },
      state,
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
      await buildDiagnosticsContext(state, {
        includeProfileRegistry: isProfileRegistryAdminRoute(url.pathname),
      }),
    );
  }

  return failure(404, requestId(request), {
    code: "not_found",
    reason_code: "unknown_service_route",
    message: `unknown service route ${url.pathname}`,
    retryable: false,
  });
}

async function handleAdminMcpServerRegistryRequest(
  request: IncomingMessage,
  state: ServiceState,
  url: URL,
): Promise<AdminRouteResult> {
  const requestIdValue = requestId(request);
  const method = (request.method ?? "GET").toUpperCase();
  const serverId = mcpServerIdFromPath(url.pathname);
  try {
    if (url.pathname === "/v1/admin/mcp/servers") {
      if (method === "GET") {
        return handleAdminMcpCatalogRequest(
          { method, requestId: requestIdValue },
          { config: state.config, runtimeConfig: state.runtimeConfig },
        );
      }
      if (method === "POST") {
        const body = recordBody(await readJsonBody(request));
        return upsertAdminMcpServer(state, requestIdValue, body, undefined);
      }
      return failure(405, requestIdValue, {
        code: "method_not_allowed",
        reason_code: "mcp_server_collection_method_not_allowed",
        message: "MCP server collection supports GET and POST",
        retryable: false,
      });
    }

    if (serverId === undefined) {
      return failure(404, requestIdValue, {
        code: "not_found",
        reason_code: "unknown_mcp_server_route",
        message: `unknown MCP server route ${url.pathname}`,
        retryable: false,
      });
    }

    if (method === "PUT" || method === "PATCH") {
      const body = recordBody(await readJsonBody(request));
      return upsertAdminMcpServer(state, requestIdValue, body, serverId);
    }

    if (method === "DELETE") {
      return deleteAdminMcpServer(state, requestIdValue, serverId);
    }

    return failure(405, requestIdValue, {
      code: "method_not_allowed",
      reason_code: "mcp_server_item_method_not_allowed",
      message: "MCP server item routes support PUT, PATCH, and DELETE",
      retryable: false,
    });
  } catch (error) {
    return failure(400, requestIdValue, {
      code: "invalid_input",
      reason_code: "invalid_mcp_server_write",
      message: errorMessage(error, "invalid MCP server registry write"),
      retryable: false,
    });
  }
}

async function upsertAdminMcpServer(
  state: ServiceState,
  requestIdValue: string,
  body: Record<string, unknown>,
  pathServerId: string | undefined,
): Promise<AdminRouteResult> {
  const server = mcpServerWriteFromBody(body, pathServerId);
  const runtimeConfigFile = await readRuntimeConfigFileForMutation(state);
  const servers = runtimeConfigFile.array("mcpServers");
  const existingIndex = servers.findIndex(
    (entry) => isRecord(entry) && optionalString(entry.id) === server.id,
  );
  const status = existingIndex >= 0 ? "updated" : "created";
  if (existingIndex >= 0) {
    servers[existingIndex] = server;
  } else {
    servers.push(server);
  }
  await writeJsonFileAtomic(
    state.config.paths.serviceConfigFile,
    runtimeConfigFile.value,
  );
  const applyResult = await applyServiceRuntimeConfigFromDisk(state, {
    createMissingSessions: false,
    eventType: "mcp_server_registry_updated",
    summaryPrefix: `MCP server ${server.id} ${status}`,
  });
  return successRoute(requestIdValue, {
    status,
    server,
    applyResult,
    catalog: mcpServerRegistryCatalog(state),
  });
}

async function deleteAdminMcpServer(
  state: ServiceState,
  requestIdValue: string,
  serverId: string,
): Promise<AdminRouteResult> {
  assertMcpServerId(serverId, "server id");
  const runtimeConfigFile = await readRuntimeConfigFileForMutation(state);
  const servers = runtimeConfigFile.array("mcpServers");
  const existingIndex = servers.findIndex(
    (entry) => isRecord(entry) && optionalString(entry.id) === serverId,
  );
  if (existingIndex < 0) {
    const envServer = state.config.mcp.servers.find(
      (server) => server.id === serverId,
    );
    return failure(envServer ? 409 : 404, requestIdValue, {
      code: envServer ? "failed_precondition" : "not_found",
      reason_code: envServer
        ? "mcp_server_env_seeded_not_runtime_managed"
        : "mcp_server_not_found",
      message: envServer
        ? `MCP server ${serverId} is seeded from service environment; create a runtime override to edit it or change service environment to remove it`
        : `MCP server ${serverId} was not found in runtime registry`,
      retryable: false,
    });
  }

  const envServer = state.config.mcp.servers.find(
    (server) => server.id === serverId,
  );
  const activeBindingCount = state.runtimeConfig.mcpBindings.filter(
    (binding) =>
      binding.status === "active" &&
      (binding.serverNames.includes(serverId) ||
        binding.endpointRef === `config://mcp/${serverId}`),
  ).length;
  if (activeBindingCount > 0 && envServer === undefined) {
    return failure(409, requestIdValue, {
      code: "failed_precondition",
      reason_code: "mcp_server_has_active_bindings",
      message: `MCP server ${serverId} has ${activeBindingCount} active binding(s); remove profile bindings before deleting it`,
      retryable: false,
    });
  }

  const [removed] = servers.splice(existingIndex, 1);
  await writeJsonFileAtomic(
    state.config.paths.serviceConfigFile,
    runtimeConfigFile.value,
  );
  const applyResult = await applyServiceRuntimeConfigFromDisk(state, {
    createMissingSessions: false,
    eventType: "mcp_server_registry_deleted",
    summaryPrefix: `MCP server ${serverId} deleted`,
  });
  return successRoute(requestIdValue, {
    status: "deleted",
    serverId,
    removed,
    applyResult,
    catalog: mcpServerRegistryCatalog(state),
  });
}

function mcpServerRegistryCatalog(state: ServiceState) {
  return mcpServerCatalogEntries({
    config: state.config,
    runtimeConfig: state.runtimeConfig,
  }).map((server) => ({
    id: server.id,
    label: server.label,
    baseUrl: server.baseUrl,
    transport: server.transport,
    requestTimeoutMs: server.requestTimeoutMs,
    source: server.source,
  }));
}

function mcpServerIdFromPath(pathname: string): string | undefined {
  const prefix = "/v1/admin/mcp/servers/";
  if (!pathname.startsWith(prefix)) return undefined;
  const rest = pathname.slice(prefix.length);
  if (!rest || rest.includes("/")) return undefined;
  return decodeURIComponent(rest);
}

function mcpServerWriteFromBody(
  body: Record<string, unknown>,
  pathServerId: string | undefined,
): RustyCrewMcpServerConfig {
  const id = pathServerId ?? optionalString(body.id ?? body.serverId);
  if (id === undefined) {
    throw new Error("MCP server id is required");
  }
  assertMcpServerId(id, "MCP server id");
  if (
    pathServerId !== undefined &&
    optionalString(body.id ?? body.serverId) !== undefined &&
    optionalString(body.id ?? body.serverId) !== pathServerId
  ) {
    throw new Error("MCP server body id must match path id");
  }

  const baseUrl = requiredString(body.baseUrl ?? body.base_url, "baseUrl");
  assertHttpUrl(baseUrl, "baseUrl");
  const requestTimeoutMs =
    body.requestTimeoutMs === undefined && body.request_timeout_ms === undefined
      ? undefined
      : positiveInteger(
          body.requestTimeoutMs ?? body.request_timeout_ms,
          "requestTimeoutMs",
        );
  return {
    id,
    label: optionalString(body.label),
    baseUrl,
    transport:
      optionalString(body.transport ?? body.transportKind) ?? "streamable_http",
    requestTimeoutMs,
    source: "runtime",
  };
}

function assertMcpServerId(value: string, fieldName: string): void {
  if (!/^[A-Za-z0-9_.:-]+$/.test(value)) {
    throw new Error(
      `${fieldName} may only contain letters, numbers, dot, underscore, colon, or dash`,
    );
  }
}

function assertHttpUrl(value: string, fieldName: string): void {
  try {
    const parsed = new URL(value);
    if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
      throw new Error("protocol must be http or https");
    }
  } catch (error) {
    throw new Error(`${fieldName} must be a valid HTTP(S) URL`, {
      cause: error,
    });
  }
}

function positiveInteger(value: unknown, fieldName: string): number {
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value <= 0) {
    throw new Error(`${fieldName} must be a positive integer`);
  }
  return value;
}

async function handleAdminLocalToolProfilesRequest(
  request: IncomingMessage,
  state: ServiceState,
  url: URL,
): Promise<AdminRouteResult> {
  const requestIdValue = requestId(request);
  const method = (request.method ?? "GET").toUpperCase();
  const store = createLocalToolProfileStore({
    bridge: state.bridge,
    now: state.now,
  });
  const profileId = localToolProfileIdFromPath(url.pathname);
  try {
    if (url.pathname === "/v1/admin/local-tool-profiles") {
      if (method === "GET") {
        return successRoute(requestIdValue, await store.list());
      }
      if (method === "POST") {
        const body = (await readJsonBody(request)) as LocalToolProfileWrite;
        return successRoute(requestIdValue, {
          profile: await store.create(body),
        });
      }
      return failure(405, requestIdValue, {
        code: "method_not_allowed",
        reason_code: "local_tool_profiles_method_not_allowed",
        message: "local tool profile collection supports GET and POST",
        retryable: false,
      });
    }

    if (profileId === undefined) {
      return failure(404, requestIdValue, {
        code: "not_found",
        reason_code: "unknown_local_tool_profile_route",
        message: `unknown local tool profile route ${url.pathname}`,
        retryable: false,
      });
    }

    if (method === "GET") {
      const profile = await store.get(profileId);
      if (profile === undefined) {
        return failure(404, requestIdValue, {
          code: "not_found",
          reason_code: "local_tool_profile_not_found",
          message: `local tool profile ${profileId} was not found`,
          retryable: false,
        });
      }
      return successRoute(requestIdValue, { profile });
    }

    if (method === "PATCH") {
      const body = (await readJsonBody(request)) as LocalToolProfileWrite;
      return successRoute(requestIdValue, {
        profile: await store.update(profileId, body),
      });
    }

    if (method === "DELETE") {
      return successRoute(requestIdValue, {
        profile: await store.delete(profileId),
        deleted: true,
      });
    }

    return failure(405, requestIdValue, {
      code: "method_not_allowed",
      reason_code: "local_tool_profile_method_not_allowed",
      message: "local tool profile item routes support GET, PATCH, and DELETE",
      retryable: false,
    });
  } catch (error) {
    if (error instanceof LocalToolProfileError) {
      return failure(error.statusCode, requestIdValue, {
        code: error.statusCode === 404 ? "not_found" : "invalid_input",
        reason_code: error.reasonCode,
        message: error.message,
        retryable: false,
      });
    }
    throw error;
  }
}

function localToolProfileIdFromPath(pathname: string): string | undefined {
  const prefix = "/v1/admin/local-tool-profiles/";
  if (!pathname.startsWith(prefix)) return undefined;
  const rest = pathname.slice(prefix.length);
  if (!rest || rest.includes("/")) return undefined;
  return decodeURIComponent(rest);
}

async function handleAdminRoleplayRequest(
  request: IncomingMessage,
  state: ServiceState,
  url: URL,
): Promise<AdminRouteResult> {
  const requestIdValue = requestId(request);
  const method = (request.method ?? "GET").toUpperCase();
  const profileLayersMatch = url.pathname.match(
    /^\/v1\/profile\/([^/]+)\/layers\/?$/,
  );
  if (profileLayersMatch) {
    return handleBrowserProfileLoreLayersRequest(
      request,
      state,
      url,
      decodeURIComponent(profileLayersMatch[1]),
    );
  }
  const parts = url.pathname.split("/").filter(Boolean);
  if (
    parts[0] === "v1" &&
    parts[1] === "admin" &&
    parts[2] === "roleplay" &&
    parts[3] === "profiles" &&
    parts[4]
  ) {
    const profileId = decodeURIComponent(parts[4]);
    if (parts[5] === "characters") {
      return handleRoleplayCharacterRequest(request, state, url, profileId);
    }
    if (parts[5] === "narrator-config") {
      return handleRoleplayNarratorConfigRequest(
        request,
        state,
        url,
        profileId,
      );
    }
  }
  if (url.pathname.startsWith("/v1/admin/roleplay/sessions")) {
    return handleRoleplaySessionRequest(request, state, url);
  }
  if (url.pathname.startsWith("/v1/admin/roleplay/lore/")) {
    return handleAdminRoleplayLoreRequest(request, state, url);
  }
  return failure(404, requestIdValue, {
    code: "not_found",
    reason_code: "unknown_roleplay_admin_route",
    message: `unknown roleplay route ${url.pathname}`,
    retryable: false,
  });
}

async function handleBrowserProfileLoreLayersRequest(
  request: IncomingMessage,
  state: ServiceState,
  url: URL,
  profileId: string,
): Promise<AdminRouteResult> {
  if ((request.method ?? "GET").toUpperCase() === "GET") {
    return roleplayLoreLayerListResult(requestId(request), state, profileId);
  }
  if ((request.method ?? "GET").toUpperCase() === "POST") {
    const body = recordBody(await readJsonBody(request));
    const layer = await state.bridge.createLoreLayer(
      roleplayLoreLayerWriteFromBody(body, profileId, state.now()),
    );
    return successRoute(requestId(request), { layer });
  }
  return roleplayLoreMethodNotAllowed(
    requestId(request),
    "profile lore layer routes support GET and POST",
  );
}

async function handleAdminRoleplayLoreRequest(
  request: IncomingMessage,
  state: ServiceState,
  url: URL,
): Promise<AdminRouteResult> {
  const requestIdValue = requestId(request);
  const method = (request.method ?? "GET").toUpperCase();
  try {
    if (url.pathname === "/v1/admin/roleplay/lore/layers") {
      if (method === "GET") {
        const profileId = url.searchParams.get("profile_id");
        if (!profileId) {
          return failure(400, requestIdValue, {
            code: "invalid_input",
            reason_code: "roleplay_lore_profile_id_required",
            message: "profile_id query parameter is required",
            retryable: false,
          });
        }
        return roleplayLoreLayerListResult(requestIdValue, state, profileId);
      }
      if (method === "POST") {
        const body = recordBody(await readJsonBody(request));
        return successRoute(requestIdValue, {
          layer: await state.bridge.createLoreLayer(
            roleplayLoreLayerWriteFromBody(body, undefined, state.now()),
          ),
        });
      }
      return roleplayLoreMethodNotAllowed(
        requestIdValue,
        "roleplay lore layer collection supports GET and POST",
      );
    }

    if (url.pathname === "/v1/admin/roleplay/lore/entries/search") {
      if (method !== "GET") {
        return roleplayLoreMethodNotAllowed(
          requestIdValue,
          "roleplay lore entry search supports GET",
        );
      }
      return successRoute(
        requestIdValue,
        await roleplayLoreEntrySearchResult(state, url),
      );
    }

    if (url.pathname === "/v1/admin/roleplay/lore/entries") {
      if (method !== "POST") {
        return roleplayLoreMethodNotAllowed(
          requestIdValue,
          "roleplay lore entry collection supports POST",
        );
      }
      return roleplayLoreEntryCreateResult(
        requestIdValue,
        state,
        recordBody(await readJsonBody(request)),
      );
    }

    const entryMatch = url.pathname.match(
      /^\/v1\/admin\/roleplay\/lore\/entries\/([^/]+)\/?$/,
    );
    if (entryMatch) {
      const entryId = decodeURIComponent(entryMatch[1]);
      if (method === "GET") {
        return roleplayLoreEntryDetailResult(
          requestIdValue,
          state,
          entryId,
          url,
        );
      }
      if (method === "PATCH") {
        return roleplayLoreEntryPatchResult(
          requestIdValue,
          state,
          entryId,
          recordBody(await readJsonBody(request)),
          url,
        );
      }
      return roleplayLoreMethodNotAllowed(
        requestIdValue,
        "roleplay lore entry item supports GET and PATCH",
      );
    }

    const entryPromoteMatch = url.pathname.match(
      /^\/v1\/admin\/roleplay\/lore\/entries\/([^/]+)\/promote\/?$/,
    );
    if (entryPromoteMatch) {
      const entryId = decodeURIComponent(entryPromoteMatch[1]);
      if (method !== "POST") {
        return roleplayLoreMethodNotAllowed(
          requestIdValue,
          "roleplay lore entry promote supports POST",
        );
      }
      return roleplayLoreEntryPromoteResult(
        requestIdValue,
        state,
        entryId,
        recordBody(await readJsonBody(request)),
        url,
      );
    }

    const layerEntriesMatch = url.pathname.match(
      /^\/v1\/admin\/roleplay\/lore\/layers\/([^/]+)\/entries\/?$/,
    );
    if (layerEntriesMatch) {
      if (method !== "GET") {
        return roleplayLoreMethodNotAllowed(
          requestIdValue,
          "roleplay lore layer entries supports GET",
        );
      }
      const layerId = decodeURIComponent(layerEntriesMatch[1]);
      const entries = await state.bridge.listEntriesByLayer(layerId);
      return successRoute(requestIdValue, {
        layerId,
        entries,
        total: entries.length,
      });
    }

    const layerMatch = url.pathname.match(
      /^\/v1\/admin\/roleplay\/lore\/layers\/([^/]+)\/?$/,
    );
    if (layerMatch) {
      const layerId = decodeURIComponent(layerMatch[1]);
      if (method === "GET") {
        const layer = await state.bridge.getLoreLayer(layerId);
        if (layer === undefined) {
          return failure(404, requestIdValue, {
            code: "not_found",
            reason_code: "roleplay_lore_layer_not_found",
            message: `roleplay lore layer ${layerId} was not found`,
            retryable: false,
          });
        }
        const entries = await state.bridge.listEntriesByLayer(layerId);
        return successRoute(requestIdValue, {
          layer: withEntryCount(layer, entries.length),
          entryCount: entries.length,
        });
      }
      if (method === "PATCH") {
        const body = recordBody(await readJsonBody(request));
        return successRoute(requestIdValue, {
          layer: await state.bridge.updateLoreLayer(
            roleplayLoreLayerUpdateFromBody(body, layerId, state.now()),
          ),
        });
      }
      if (method === "DELETE") {
        return successRoute(requestIdValue, {
          layer: await state.bridge.archiveLoreLayer({
            layer_id: layerId,
            now: state.now(),
          }),
        });
      }
      return roleplayLoreMethodNotAllowed(
        requestIdValue,
        "roleplay lore layer item supports GET, PATCH, and DELETE",
      );
    }

    if (url.pathname === "/v1/admin/roleplay/lore/chat-layers/toggle") {
      if (method !== "POST") {
        return roleplayLoreMethodNotAllowed(
          requestIdValue,
          "roleplay lore chat layer toggle supports POST",
        );
      }
      const body = recordBody(await readJsonBody(request));
      const chatId = requiredRouteString(
        optionalString(body.chatId) ?? optionalString(body.chat_id),
        "chat_id",
      );
      const layerId = requiredRouteString(
        optionalString(body.layerId) ?? optionalString(body.layer_id),
        "layer_id",
      );
      const enabled =
        typeof body.enabled === "boolean"
          ? body.enabled
          : requiredRouteBoolean(undefined, "enabled");
      await state.bridge.toggleChatLayer({ chatId, layerId, enabled });
      return successRoute(requestIdValue, { chatId, layerId, enabled });
    }

    if (url.pathname === "/v1/admin/roleplay/lore/chat-layers/reorder") {
      if (method !== "POST") {
        return roleplayLoreMethodNotAllowed(
          requestIdValue,
          "roleplay lore chat layer reorder supports POST",
        );
      }
      const body = recordBody(await readJsonBody(request));
      const chatId = requiredRouteString(
        optionalString(body.chatId) ?? optionalString(body.chat_id),
        "chat_id",
      );
      const layerIds = stringArray(
        body.layerIds ?? body.layer_ids,
        "layer_ids",
      );
      await state.bridge.reorderChatLayers({ chatId, layerIds });
      return successRoute(requestIdValue, { chatId, layerIds });
    }

    if (url.pathname === "/v1/admin/roleplay/lore/chat-layers") {
      if (method === "GET") {
        const chatId = url.searchParams.get("chat_id");
        if (!chatId) {
          return failure(400, requestIdValue, {
            code: "invalid_input",
            reason_code: "roleplay_lore_chat_id_required",
            message: "chat_id query parameter is required",
            retryable: false,
          });
        }
        const layers = await state.bridge.getChatLayers(chatId);
        return successRoute(requestIdValue, {
          chatId,
          layers,
          activeLayerIds: layers
            .filter((layer) => layer.enabled !== false)
            .sort(
              (left, right) => Number(left.priority) - Number(right.priority),
            )
            .map((layer) => String(layer.layer_id)),
        });
      }
      if (method !== "POST") {
        return roleplayLoreMethodNotAllowed(
          requestIdValue,
          "roleplay lore chat layers supports GET and POST",
        );
      }
      const body = recordBody(await readJsonBody(request));
      const write = roleplayChatLayersWriteFromBody(body, state.now());
      await state.bridge.setChatLayers(write);
      await upsertRoleplaySessionMetadata(state, write.chat_id, {
        activeLayerIds: write.layers.map((layer) => layer.layer_id),
      });
      return successRoute(requestIdValue, {
        saved: true,
        chatId: write.chat_id,
      });
    }

    if (url.pathname === "/v1/admin/roleplay/lore/facts/capture") {
      if (method !== "POST") {
        return roleplayLoreMethodNotAllowed(
          requestIdValue,
          "roleplay lore fact capture supports POST",
        );
      }
      const body = (await readJsonBody(
        request,
      )) as NativeRoleplayLoreFactCapture;
      return successRoute(requestIdValue, {
        entry: await state.bridge.captureLoreFact(body),
      });
    }

    return failure(404, requestIdValue, {
      code: "not_found",
      reason_code: "unknown_roleplay_lore_admin_route",
      message: `unknown roleplay lore admin route ${url.pathname}`,
      retryable: false,
    });
  } catch (error) {
    return failure(400, requestIdValue, {
      code: "invalid_input",
      reason_code: "roleplay_lore_admin_failed",
      message: errorMessage(error, "roleplay lore admin request failed"),
      retryable: false,
    });
  }
}

function roleplayLoreMethodNotAllowed(
  requestIdValue: string,
  message: string,
): AdminRouteResult {
  return failure(405, requestIdValue, {
    code: "method_not_allowed",
    reason_code: "roleplay_lore_method_not_allowed",
    message,
    retryable: false,
  });
}

async function roleplayLoreLayerListResult(
  requestIdValue: string,
  state: ServiceState,
  profileId: string,
): Promise<AdminRouteResult> {
  const layers = await state.bridge.listLoreLayers(profileId);
  const counts = await Promise.all(
    layers.map(async (layer) => {
      const layerId = String(layer.layer_id);
      return [
        layerId,
        await state.bridge
          .listEntriesByLayer(layerId)
          .then((entries) => entries.length)
          .catch(() => 0),
      ] as const;
    }),
  );
  const entryCounts = Object.fromEntries(counts);
  return successRoute(requestIdValue, {
    profileId,
    layers: layers.map((layer) =>
      withEntryCount(layer, entryCounts[String(layer.layer_id)] ?? 0),
    ),
    entryCounts,
    total: layers.length,
  });
}

async function roleplayLoreEntrySearchResult(
  state: ServiceState,
  url: URL,
): Promise<Record<string, unknown>> {
  const params = url.searchParams;
  const profileId = optionalString(params.get("profile_id"));
  const chatId = optionalString(params.get("chat_id"));
  const explicitLayerIds = roleplayLoreSearchLayerIds(params);
  const layerScope = await roleplayLoreSearchLayerScope(state, {
    profileId,
    chatId,
    explicitLayerIds,
  });
  const page = roleplayLoreSearchPage(params);
  const query = roleplayLoreSearchQuery(params, page);
  const pageResult =
    layerScope.recordIds === undefined
      ? await queryUnscopedRoleplayLoreEntrySearchPage(state, query, page)
      : await queryLayerScopedRoleplayLoreEntrySearchPage(
          state,
          query,
          page,
          layerScope.recordIds,
        );
  return {
    query: {
      text:
        optionalString(params.get("q")) ?? optionalString(params.get("query")),
      profileId,
      chatId,
      layerIds: layerScope.layerIds,
      worldId: optionalString(params.get("world_id") ?? params.get("worldId")),
      entityId: optionalString(
        params.get("entity_id") ?? params.get("entityId"),
      ),
      canonStatus: optionalString(
        params.get("canon_status") ?? params.get("canonStatus"),
      ),
      visibility: optionalString(params.get("visibility")),
      shapeId: optionalString(params.get("shape_id") ?? params.get("shapeId")),
      includeSuperseded: optionalBooleanQuery(params, "include_superseded"),
      includeTombstoned: optionalBooleanQuery(params, "include_tombstoned"),
    },
    entries: pageResult.entries.map(browserSafeLoreEntry),
    layers: layerScope.layers.map((layer) => withEntryCount(layer, 0)),
    layerContext: {
      source: layerScope.source,
      profileId,
      chatId,
      layerIds: layerScope.layerIds,
      activeLayerIds: layerScope.activeLayerIds,
    },
    total: pageResult.total,
    totalExact: pageResult.totalExact,
    limit: page.limit,
    offset: page.offset,
    hasMore: pageResult.hasMore,
  };
}

async function roleplayLoreEntryCreateResult(
  requestIdValue: string,
  state: ServiceState,
  body: Record<string, unknown>,
): Promise<AdminRouteResult> {
  const layerId = requiredRouteString(
    optionalString(body.layer_id) ?? optionalString(body.layerId),
    "layer_id",
  );
  const layer = await state.bridge.getLoreLayer(layerId);
  if (layer === undefined) {
    return failure(404, requestIdValue, {
      code: "not_found",
      reason_code: "roleplay_lore_layer_not_found",
      message: `roleplay lore layer ${layerId} was not found`,
      retryable: false,
    });
  }
  if (layer.is_archived === true) {
    return failure(409, requestIdValue, {
      code: "conflict",
      reason_code: "roleplay_lore_layer_archived",
      message: `roleplay lore layer ${layerId} is archived`,
      retryable: false,
    });
  }
  if (layer.write_policy === "readonly") {
    return failure(409, requestIdValue, {
      code: "conflict",
      reason_code: "roleplay_lore_layer_readonly",
      message: `roleplay lore layer ${layerId} is readonly`,
      retryable: false,
    });
  }

  const write = roleplayLoreWriteFromBody(
    recordBody(body.write ?? body),
    undefined,
    state.now(),
    {
      defaultRecordId: `lore-${randomBytes(8).toString("hex")}`,
      defaultSource: "ui",
      defaultDurabilityRationale:
        "Manual roleplay lore entry created through browser admin API.",
    },
  );
  const entry = await state.bridge.addLoreEntry(write);
  await state.bridge.addEntryToLayer({
    layer_id: layerId,
    record_id: String(entry.record_id),
    is_constant:
      optionalBoolean(body.is_constant) ??
      optionalBoolean(body.isConstant) ??
      false,
    priority: Math.trunc(optionalNumber(body.priority) ?? 0),
    added_at: write.now,
  });
  return successRoute(requestIdValue, {
    ...(await roleplayLoreEntryDetailData(state, entry, {
      explicitLayerIds: [layerId],
    })),
    created: true,
  });
}

async function roleplayLoreEntryDetailResult(
  requestIdValue: string,
  state: ServiceState,
  entryId: string,
  url: URL,
): Promise<AdminRouteResult> {
  const entry = await state.bridge.getLoreEntry(entryId);
  if (entry === undefined) {
    return failure(404, requestIdValue, {
      code: "not_found",
      reason_code: "roleplay_lore_entry_not_found",
      message: `roleplay lore entry ${entryId} was not found`,
      retryable: false,
    });
  }
  return successRoute(
    requestIdValue,
    await roleplayLoreEntryDetailData(state, entry, {
      profileId: optionalString(url.searchParams.get("profile_id")),
      chatId: optionalString(url.searchParams.get("chat_id")),
      explicitLayerIds: roleplayLoreSearchLayerIds(url.searchParams),
    }),
  );
}

async function roleplayLoreEntryPatchResult(
  requestIdValue: string,
  state: ServiceState,
  entryId: string,
  body: Record<string, unknown>,
  url: URL,
): Promise<AdminRouteResult> {
  const existing = await state.bridge.getLoreEntry(entryId);
  if (existing === undefined) {
    return failure(404, requestIdValue, {
      code: "not_found",
      reason_code: "roleplay_lore_entry_not_found",
      message: `roleplay lore entry ${entryId} was not found`,
      retryable: false,
    });
  }
  const replace: NativeRoleplayLoreReplace = {
    write: roleplayLoreWriteFromBody(
      recordBody(body.write ?? body),
      existing,
      state.now(),
      {
        forcedRecordId: entryId,
        defaultSource: "ui",
        defaultDurabilityRationale:
          "Roleplay lore entry updated through browser admin API.",
      },
    ),
    expected_revision: requiredPositiveInteger(
      body.expected_revision ?? body.expectedRevision,
      "expected_revision",
    ),
  };
  const entry = await state.bridge.replaceLoreEntry(replace);
  return successRoute(
    requestIdValue,
    await roleplayLoreEntryDetailData(state, entry, {
      profileId: optionalString(url.searchParams.get("profile_id")),
      chatId: optionalString(url.searchParams.get("chat_id")),
      explicitLayerIds: roleplayLoreSearchLayerIds(url.searchParams),
    }),
  );
}

async function roleplayLoreEntryPromoteResult(
  requestIdValue: string,
  state: ServiceState,
  entryId: string,
  body: Record<string, unknown>,
  url: URL,
): Promise<AdminRouteResult> {
  const sourceEntry = await state.bridge.getLoreEntry(entryId);
  if (sourceEntry === undefined) {
    return failure(404, requestIdValue, {
      code: "not_found",
      reason_code: "roleplay_lore_entry_not_found",
      message: `roleplay lore entry ${entryId} was not found`,
      retryable: false,
    });
  }

  const targetLayerId = requiredRouteString(
    optionalString(body.targetLayerId) ?? optionalString(body.target_layer_id),
    "target_layer_id",
  );
  const targetLayer = await state.bridge.getLoreLayer(targetLayerId);
  if (targetLayer === undefined) {
    return failure(404, requestIdValue, {
      code: "not_found",
      reason_code: "roleplay_lore_target_layer_not_found",
      message: `roleplay lore target layer ${targetLayerId} was not found`,
      retryable: false,
    });
  }
  if (targetLayer.is_archived === true) {
    return failure(409, requestIdValue, {
      code: "conflict",
      reason_code: "roleplay_lore_target_layer_archived",
      message: `roleplay lore target layer ${targetLayerId} is archived`,
      retryable: false,
    });
  }
  if (targetLayer.write_policy === "readonly") {
    return failure(409, requestIdValue, {
      code: "conflict",
      reason_code: "roleplay_lore_target_layer_readonly",
      message: `roleplay lore target layer ${targetLayerId} is readonly`,
      retryable: false,
    });
  }

  const sourceLayerId = await roleplayLorePromotionSourceLayerId(
    requestIdValue,
    state,
    entryId,
    body,
    url,
  );
  if (typeof sourceLayerId !== "string") return sourceLayerId;

  const newRecordId =
    optionalString(body.newRecordId) ??
    optionalString(body.new_record_id) ??
    `lore-promoted-${randomBytes(8).toString("hex")}`;
  if ((await state.bridge.getLoreEntry(newRecordId)) !== undefined) {
    return failure(409, requestIdValue, {
      code: "conflict",
      reason_code: "roleplay_lore_promoted_entry_exists",
      message: `roleplay lore promoted entry ${newRecordId} already exists`,
      retryable: false,
    });
  }

  const now = optionalString(body.now) ?? state.now();
  const promotion: NativeRoleplayLoreEntryPromotion = {
    source_layer_id: sourceLayerId,
    source_record_id: entryId,
    target_layer_id: targetLayerId,
    new_record_id: newRecordId,
    is_constant:
      optionalBoolean(body.is_constant) ??
      optionalBoolean(body.isConstant) ??
      false,
    priority: Math.trunc(optionalNumber(body.priority) ?? 0),
    now,
  };
  await state.bridge.promoteLoreEntry(promotion);
  const promotedEntry = await state.bridge.getLoreEntry(newRecordId);
  if (promotedEntry === undefined) {
    return failure(500, requestIdValue, {
      code: "internal_error",
      reason_code: "roleplay_lore_promoted_entry_unreadable",
      message: `roleplay lore promoted entry ${newRecordId} was not readable`,
      retryable: false,
    });
  }
  return successRoute(requestIdValue, {
    ...(await roleplayLoreEntryDetailData(state, promotedEntry, {
      profileId: optionalString(url.searchParams.get("profile_id")),
      chatId: optionalString(url.searchParams.get("chat_id")),
      explicitLayerIds: [targetLayerId],
    })),
    promoted: true,
    source: {
      layerId: sourceLayerId,
      recordId: entryId,
      entry: browserSafeLoreEntry(sourceEntry),
    },
    target: {
      layerId: targetLayerId,
      recordId: newRecordId,
    },
  });
}

async function roleplayLorePromotionSourceLayerId(
  requestIdValue: string,
  state: ServiceState,
  entryId: string,
  body: Record<string, unknown>,
  url: URL,
): Promise<string | AdminRouteResult> {
  const explicitSourceLayerId =
    optionalString(body.sourceLayerId) ??
    optionalString(body.source_layer_id) ??
    optionalString(url.searchParams.get("source_layer_id")) ??
    optionalString(url.searchParams.get("sourceLayerId"));
  if (explicitSourceLayerId !== undefined) {
    return roleplayLorePromotionValidatedSourceLayerId(
      requestIdValue,
      state,
      entryId,
      explicitSourceLayerId,
    );
  }

  const candidateLayerIds = await roleplayLorePromotionCandidateLayerIds(
    state,
    body,
    url,
  );
  if (candidateLayerIds.length === 0) {
    return failure(400, requestIdValue, {
      code: "invalid_input",
      reason_code: "roleplay_lore_source_layer_required",
      message:
        "source_layer_id is required when profile_id, chat_id, or source layer scope is not provided",
      retryable: false,
    });
  }
  const containingLayerIds = (
    await Promise.all(
      candidateLayerIds.map(async (layerId) => {
        if ((await state.bridge.getLoreLayer(layerId)) === undefined) {
          return undefined;
        }
        const entries = await state.bridge.listEntriesByLayer(layerId);
        return entries.some((entry) => String(entry.record_id) === entryId)
          ? layerId
          : undefined;
      }),
    )
  ).filter((layerId): layerId is string => layerId !== undefined);
  if (containingLayerIds.length === 1) return containingLayerIds[0]!;
  if (containingLayerIds.length === 0) {
    return failure(404, requestIdValue, {
      code: "not_found",
      reason_code: "roleplay_lore_source_layer_not_found",
      message: `roleplay lore entry ${entryId} was not found in the provided source layer scope`,
      retryable: false,
    });
  }
  return failure(409, requestIdValue, {
    code: "conflict",
    reason_code: "roleplay_lore_source_layer_ambiguous",
    message: `roleplay lore entry ${entryId} exists in multiple source layers; source_layer_id is required`,
    retryable: false,
  });
}

async function roleplayLorePromotionValidatedSourceLayerId(
  requestIdValue: string,
  state: ServiceState,
  entryId: string,
  sourceLayerId: string,
): Promise<string | AdminRouteResult> {
  if ((await state.bridge.getLoreLayer(sourceLayerId)) === undefined) {
    return failure(404, requestIdValue, {
      code: "not_found",
      reason_code: "roleplay_lore_source_layer_not_found",
      message: `roleplay lore source layer ${sourceLayerId} was not found`,
      retryable: false,
    });
  }
  const entries = await state.bridge.listEntriesByLayer(sourceLayerId);
  if (!entries.some((entry) => String(entry.record_id) === entryId)) {
    return failure(404, requestIdValue, {
      code: "not_found",
      reason_code: "roleplay_lore_source_entry_not_in_layer",
      message: `roleplay lore entry ${entryId} was not found in source layer ${sourceLayerId}`,
      retryable: false,
    });
  }
  return sourceLayerId;
}

async function roleplayLorePromotionCandidateLayerIds(
  state: ServiceState,
  body: Record<string, unknown>,
  url: URL,
): Promise<string[]> {
  const explicitLayerIds = [
    ...stringListField(body, ["sourceLayerIds", "source_layer_ids"]),
    ...url.searchParams.getAll("source_layer_id"),
    ...url.searchParams.getAll("sourceLayerId"),
    ...url.searchParams.getAll("source_layer_ids"),
    ...url.searchParams.getAll("sourceLayerIds"),
  ]
    .flatMap((value) => value.split(","))
    .map((value) => value.trim())
    .filter((value) => value.length > 0);
  if (explicitLayerIds.length > 0) return [...new Set(explicitLayerIds)];

  const profileId =
    optionalString(body.profileId) ??
    optionalString(body.profile_id) ??
    optionalString(url.searchParams.get("profile_id")) ??
    optionalString(url.searchParams.get("profileId"));
  if (profileId !== undefined) {
    const layers = await state.bridge.listLoreLayers(profileId);
    return layers.map((layer) => String(layer.layer_id));
  }

  const chatId =
    optionalString(body.chatId) ??
    optionalString(body.chat_id) ??
    optionalString(url.searchParams.get("chat_id")) ??
    optionalString(url.searchParams.get("chatId"));
  if (chatId !== undefined) {
    const layers = await state.bridge.getChatLayers(chatId);
    return layers
      .filter((layer) => layer.enabled !== false)
      .sort((left, right) => Number(left.priority) - Number(right.priority))
      .map((layer) => String(layer.layer_id));
  }

  return [];
}

async function roleplayLoreEntryDetailData(
  state: ServiceState,
  entry: Record<string, unknown>,
  scope: {
    profileId?: string;
    chatId?: string;
    explicitLayerIds: readonly string[];
  },
): Promise<Record<string, unknown>> {
  const entryId = String(entry.record_id);
  const [provenance, layerContext, supersedes, supersededBy] =
    await Promise.all([
      state.bridge.loreEntryProvenanceEvents(entryId),
      roleplayLoreEntryLayerContext(state, entryId, scope),
      optionalString(entry.supersedes_record_id) === undefined
        ? Promise.resolve(undefined)
        : state.bridge.getLoreEntry(String(entry.supersedes_record_id)),
      optionalString(entry.superseded_by_record_id) === undefined
        ? Promise.resolve(undefined)
        : state.bridge.getLoreEntry(String(entry.superseded_by_record_id)),
    ]);
  return {
    entry: browserSafeLoreEntry(entry),
    provenance,
    supersession: {
      supersedesRecordId: optionalString(entry.supersedes_record_id),
      supersededByRecordId: optionalString(entry.superseded_by_record_id),
      supersedes: supersedes ? browserSafeLoreEntry(supersedes) : undefined,
      supersededBy: supersededBy
        ? browserSafeLoreEntry(supersededBy)
        : undefined,
    },
    layerEntries: layerContext.layerEntries,
    layers: layerContext.layers,
    layerContext: {
      source: layerContext.source,
      profileId: scope.profileId,
      chatId: scope.chatId,
      layerIds: layerContext.layerIds,
      activeLayerIds: layerContext.activeLayerIds,
    },
  };
}

async function roleplayLoreEntryLayerContext(
  state: ServiceState,
  entryId: string,
  scope: {
    profileId?: string;
    chatId?: string;
    explicitLayerIds: readonly string[];
  },
): Promise<{
  source: "explicit" | "chat" | "profile" | "all";
  layerIds: string[];
  activeLayerIds: string[];
  layers: Record<string, unknown>[];
  layerEntries: Record<string, unknown>[];
}> {
  const layerScope = await roleplayLoreSearchLayerScope(state, scope);
  const layerEntries =
    layerScope.layerIds.length === 0
      ? []
      : (
          await Promise.all(
            layerScope.layerIds.map((layerId) =>
              state.bridge.listEntriesByLayer(layerId),
            ),
          )
        )
          .flat()
          .filter((entry) => String(entry.record_id) === entryId);
  return {
    source: layerScope.source,
    layerIds: layerScope.layerIds,
    activeLayerIds: layerScope.activeLayerIds,
    layers: layerScope.layers,
    layerEntries,
  };
}

function roleplayLoreSearchQuery(
  params: URLSearchParams,
  page: { limit: number; offset: number },
): NativeRoleplayLoreQuery {
  return {
    world_id: optionalString(params.get("world_id") ?? params.get("worldId")),
    entity_id: optionalString(
      params.get("entity_id") ?? params.get("entityId"),
    ),
    canon_status: optionalString(
      params.get("canon_status") ?? params.get("canonStatus"),
    ),
    visibility: optionalString(params.get("visibility")),
    shape_id: optionalString(params.get("shape_id") ?? params.get("shapeId")),
    query:
      optionalString(params.get("q")) ?? optionalString(params.get("query")),
    include_superseded:
      optionalBooleanQuery(params, "include_superseded") ??
      optionalBooleanQuery(params, "includeSuperseded") ??
      false,
    include_tombstoned:
      optionalBooleanQuery(params, "include_tombstoned") ??
      optionalBooleanQuery(params, "includeTombstoned") ??
      false,
    page,
  };
}

async function queryUnscopedRoleplayLoreEntrySearchPage(
  state: ServiceState,
  query: NativeRoleplayLoreQuery,
  page: { limit: number; offset: number },
): Promise<{
  entries: Record<string, unknown>[];
  total: number;
  totalExact: boolean;
  hasMore: boolean;
}> {
  const rawEntries = await state.bridge.queryLoreEntries({
    ...query,
    page: {
      limit: page.limit + 1,
      offset: page.offset,
    },
  });
  const hasMore = rawEntries.length > page.limit;
  const entries = rawEntries.slice(0, page.limit);
  return {
    entries,
    total: page.offset + entries.length + (hasMore ? 1 : 0),
    totalExact: !hasMore,
    hasMore,
  };
}

async function queryLayerScopedRoleplayLoreEntrySearchPage(
  state: ServiceState,
  query: NativeRoleplayLoreQuery,
  page: { limit: number; offset: number },
  recordIds: Set<string>,
): Promise<{
  entries: Record<string, unknown>[];
  total: number;
  totalExact: boolean;
  hasMore: boolean;
}> {
  const rawEntries = await queryAllRoleplayLoreEntriesForLayerScopedSearch(
    state,
    query,
  );
  const filteredEntries = rawEntries.filter((entry) =>
    recordIds.has(String(entry.record_id)),
  );
  const entries = filteredEntries.slice(page.offset, page.offset + page.limit);
  return {
    entries,
    total: filteredEntries.length,
    totalExact: true,
    hasMore: page.offset + entries.length < filteredEntries.length,
  };
}

async function queryAllRoleplayLoreEntriesForLayerScopedSearch(
  state: ServiceState,
  baseQuery: NativeRoleplayLoreQuery,
): Promise<Record<string, unknown>[]> {
  const pageLimit = 1_000;
  const entries: Record<string, unknown>[] = [];
  for (let offset = 0; ; offset += pageLimit) {
    const pageEntries = await state.bridge.queryLoreEntries({
      ...baseQuery,
      page: {
        limit: pageLimit,
        offset,
      },
    });
    entries.push(...pageEntries);
    if (pageEntries.length < pageLimit) return entries;
  }
}

async function roleplayLoreSearchLayerScope(
  state: ServiceState,
  input: {
    profileId?: string;
    chatId?: string;
    explicitLayerIds: readonly string[];
  },
): Promise<{
  source: "explicit" | "chat" | "profile" | "all";
  layerIds: string[];
  activeLayerIds: string[];
  layers: Record<string, unknown>[];
  recordIds?: Set<string>;
}> {
  let source: "explicit" | "chat" | "profile" | "all" = "all";
  let layerIds = [...input.explicitLayerIds];
  let activeLayerIds: string[] = [];

  if (layerIds.length > 0) {
    source = "explicit";
    activeLayerIds = [...layerIds];
  } else if (input.chatId) {
    source = "chat";
    const chatLayers = await state.bridge.getChatLayers(input.chatId);
    layerIds = chatLayers
      .filter((layer) => layer.enabled !== false)
      .sort((left, right) => Number(left.priority) - Number(right.priority))
      .map((layer) => String(layer.layer_id));
    activeLayerIds = [...layerIds];
  } else if (input.profileId) {
    source = "profile";
    const profileLayers = await state.bridge.listLoreLayers(input.profileId);
    layerIds = profileLayers.map((layer) => String(layer.layer_id));
    activeLayerIds = [...layerIds];
  }

  const layers = (
    await Promise.all(
      layerIds.map((layerId) =>
        state.bridge.getLoreLayer(layerId).catch(() => undefined),
      ),
    )
  ).filter((layer): layer is Record<string, unknown> => layer !== undefined);

  if (layerIds.length === 0) {
    return { source, layerIds, activeLayerIds, layers };
  }

  const recordIds = new Set<string>();
  await Promise.all(
    layerIds.map(async (layerId) => {
      const entries = await state.bridge.listEntriesByLayer(layerId);
      for (const entry of entries) {
        recordIds.add(String(entry.record_id));
      }
    }),
  );
  return { source, layerIds, activeLayerIds, layers, recordIds };
}

function roleplayLoreSearchLayerIds(params: URLSearchParams): string[] {
  const values = [
    ...params.getAll("layer_id"),
    ...params.getAll("layerId"),
    ...params.getAll("layer_ids"),
    ...params.getAll("layerIds"),
  ];
  return [
    ...new Set(
      values
        .flatMap((value) => value.split(","))
        .map((value) => value.trim())
        .filter((value) => value.length > 0),
    ),
  ];
}

function roleplayLoreSearchPage(params: URLSearchParams): {
  limit: number;
  offset: number;
} {
  const limit = integerQueryParam(params, "limit", 50);
  const offset = integerQueryParam(params, "offset", 0);
  return {
    limit: Math.min(Math.max(limit, 1), 200),
    offset: Math.max(offset, 0),
  };
}

function integerQueryParam(
  params: URLSearchParams,
  name: string,
  fallback: number,
): number {
  const raw = params.get(name);
  if (raw === null || raw.trim().length === 0) return fallback;
  const value = Number(raw);
  if (!Number.isInteger(value)) {
    throw new Error(`${name} must be an integer`);
  }
  return value;
}

function optionalBooleanQuery(
  params: URLSearchParams,
  name: string,
): boolean | undefined {
  const raw = params.get(name);
  if (raw === null || raw.trim().length === 0) return undefined;
  if (raw === "true" || raw === "1") return true;
  if (raw === "false" || raw === "0") return false;
  throw new Error(`${name} must be true or false`);
}

function browserSafeLoreEntry(
  entry: Record<string, unknown>,
): Record<string, unknown> {
  return { ...entry };
}

function withEntryCount(
  layer: Record<string, unknown>,
  entryCount: number,
): Record<string, unknown> {
  return { ...layer, entry_count: entryCount, entryCount };
}

function roleplayLoreWriteFromBody(
  body: Record<string, unknown>,
  existing: Record<string, unknown> | undefined,
  now: string,
  options: {
    defaultRecordId?: string;
    forcedRecordId?: string;
    defaultSource: string;
    defaultDurabilityRationale: string;
  },
): NativeRoleplayLoreWrite {
  const recordId = requiredRouteString(
    options.forcedRecordId ??
      stringField(
        body,
        ["record_id", "recordId"],
        optionalString(existing?.record_id),
      ) ??
      options.defaultRecordId,
    "record_id",
  );
  const worldId = requiredRouteString(
    stringField(
      body,
      ["world_id", "worldId"],
      optionalString(existing?.world_id),
    ),
    "world_id",
  );
  const entityId = nullableStringField(
    body,
    ["entity_id", "entityId"],
    existing?.entity_id,
  );
  const title = requiredRouteString(
    stringField(body, ["title"], optionalString(existing?.title)),
    "title",
  );
  const loreBody = requiredRouteString(
    stringField(body, ["body"], optionalString(existing?.body)),
    "body",
  );
  const canonStatus = requiredRouteString(
    stringField(
      body,
      ["canon_status", "canonStatus"],
      optionalString(existing?.canon_status) ?? "draft",
    ),
    "canon_status",
  );
  const visibility = requiredRouteString(
    stringField(
      body,
      ["visibility"],
      optionalString(existing?.visibility) ?? "public",
    ),
    "visibility",
  );
  const content = roleplayLoreContentFromBody(body, existing, {
    worldId,
    entityId,
    title,
    body: loreBody,
    canonStatus,
    visibility,
  });
  return {
    record_id: recordId,
    world_id: worldId,
    entity_id: entityId,
    session_id: nullableStringField(
      body,
      ["session_id", "sessionId"],
      existing?.session_id,
    ),
    branch_id: nullableStringField(
      body,
      ["branch_id", "branchId"],
      existing?.branch_id,
    ),
    shape: roleplayLoreShapeFromBody(body, existing),
    canon_status: canonStatus,
    visibility,
    title,
    body: loreBody,
    content,
    evidence_refs: roleplayLoreEvidenceRefsFromBody(body, existing),
    source: stringField(body, ["source"], undefined) ?? options.defaultSource,
    confidence:
      optionalNumber(body.confidence) ??
      optionalNumber(existing?.confidence) ??
      1,
    durability_rationale:
      stringField(
        body,
        ["durability_rationale", "durabilityRationale"],
        optionalString(existing?.durability_rationale),
      ) ?? options.defaultDurabilityRationale,
    supersedes_record_id: nullableStringField(
      body,
      ["supersedes_record_id", "supersedesRecordId"],
      existing?.supersedes_record_id,
    ),
    now,
  };
}

function roleplayLoreShapeFromBody(
  body: Record<string, unknown>,
  existing: Record<string, unknown> | undefined,
): Record<string, unknown> {
  const bodyShape = recordBody(body.shape);
  const existingShape = recordBody(existing?.shape);
  return {
    shape_id:
      optionalString(bodyShape.shape_id) ??
      optionalString(bodyShape.shapeId) ??
      optionalString(body.shape_id) ??
      optionalString(body.shapeId) ??
      optionalString(existingShape.shape_id) ??
      "lore_entry",
    version:
      optionalNumber(bodyShape.version) ??
      optionalNumber(body.shape_version) ??
      optionalNumber(body.shapeVersion) ??
      optionalNumber(existingShape.version) ??
      1,
  };
}

function roleplayLoreContentFromBody(
  body: Record<string, unknown>,
  existing: Record<string, unknown> | undefined,
  fields: {
    worldId: string;
    entityId: string | null | undefined;
    title: string;
    body: string;
    canonStatus: string;
    visibility: string;
  },
): Record<string, unknown> {
  const rawContent = Object.hasOwn(body, "content")
    ? body.content
    : existing?.content;
  const content = isRecord(rawContent) ? { ...rawContent } : {};
  content.world_id = fields.worldId;
  if (fields.entityId === null || fields.entityId === undefined) {
    delete content.entity_id;
  } else {
    content.entity_id = fields.entityId;
  }
  content.title = fields.title;
  content.body = fields.body;
  content.canon_status = fields.canonStatus;
  content.visibility = fields.visibility;
  return content;
}

function roleplayLoreEvidenceRefsFromBody(
  body: Record<string, unknown>,
  existing: Record<string, unknown> | undefined,
): unknown[] {
  const value = Object.hasOwn(body, "evidence_refs")
    ? body.evidence_refs
    : Object.hasOwn(body, "evidenceRefs")
      ? body.evidenceRefs
      : existing?.evidence_refs;
  const refs = Array.isArray(value) ? value : [];
  return refs.length > 0
    ? refs
    : [
        {
          evidence_type: "ui",
          ref_id: "browser-admin",
          label: "Browser admin edit",
        },
      ];
}

function stringField(
  body: Record<string, unknown>,
  keys: readonly string[],
  fallback: string | undefined,
): string | undefined {
  for (const key of keys) {
    if (Object.hasOwn(body, key)) return optionalString(body[key]);
  }
  return fallback;
}

function stringListField(
  body: Record<string, unknown>,
  keys: readonly string[],
): string[] {
  for (const key of keys) {
    if (!Object.hasOwn(body, key)) continue;
    const value = body[key];
    if (Array.isArray(value)) {
      return value.map((item, index) =>
        requiredRouteString(optionalString(item), `${key}[${index}]`),
      );
    }
    const text = optionalString(value);
    return text === undefined ? [] : [text];
  }
  return [];
}

function nullableStringField(
  body: Record<string, unknown>,
  keys: readonly string[],
  fallback: unknown,
): string | null | undefined {
  for (const key of keys) {
    if (Object.hasOwn(body, key)) return optionalString(body[key]) ?? null;
  }
  return optionalString(fallback);
}

function roleplayLoreLayerWriteFromBody(
  body: Record<string, unknown>,
  pathProfileId: string | undefined,
  now: string,
): NativeRoleplayLoreLayerWrite {
  const layerId =
    optionalString(body.layer_id) ??
    optionalString(body.layerId) ??
    `layer-${randomBytes(6).toString("hex")}`;
  const profileId =
    pathProfileId ??
    optionalString(body.profile_id) ??
    optionalString(body.profileId);
  return {
    layer_id: requiredRouteString(layerId, "layer_id"),
    profile_id: requiredRouteString(profileId, "profile_id"),
    name: requiredRouteString(optionalString(body.name), "name"),
    description:
      optionalString(body.description) ??
      optionalString(body.summary) ??
      undefined,
    purpose:
      optionalString(body.purpose) ??
      optionalString(body.layerPurpose) ??
      "mixed",
    write_policy:
      optionalString(body.write_policy) ??
      optionalString(body.writePolicy) ??
      "manual",
    now,
  };
}

function roleplayLoreLayerUpdateFromBody(
  body: Record<string, unknown>,
  layerId: string,
  now: string,
): Record<string, unknown> {
  return {
    layer_id: layerId,
    ...(optionalString(body.name) === undefined
      ? {}
      : { name: optionalString(body.name) }),
    ...(Object.hasOwn(body, "description")
      ? { description: optionalString(body.description) ?? null }
      : {}),
    ...(optionalString(body.purpose) === undefined
      ? {}
      : { purpose: optionalString(body.purpose) }),
    ...(optionalString(body.write_policy) === undefined &&
    optionalString(body.writePolicy) === undefined
      ? {}
      : {
          write_policy:
            optionalString(body.write_policy) ??
            optionalString(body.writePolicy),
        }),
    now,
  };
}

function roleplayChatLayersWriteFromBody(
  body: Record<string, unknown>,
  now: string,
): RoleplayChatLayersBrowserWrite {
  const chatId = requiredRouteString(
    optionalString(body.chat_id) ??
      optionalString(body.chatId) ??
      optionalString(body.session_id) ??
      optionalString(body.sessionId),
    "chat_id",
  );
  const rawLayers = arrayValue(body.layers);
  const layerIds =
    rawLayers.length > 0
      ? rawLayers.map((layer, index) => {
          if (typeof layer === "string") {
            return { layer_id: layer, priority: index, enabled: true };
          }
          const record = recordBody(layer);
          return {
            layer_id: requiredRouteString(
              optionalString(record.layer_id) ?? optionalString(record.layerId),
              `layers[${index}].layer_id`,
            ),
            priority: optionalNumber(record.priority) ?? index,
            enabled: optionalBoolean(record.enabled) ?? true,
          };
        })
      : stringArray(body.layer_ids ?? body.layerIds, "layer_ids").map(
          (layerId, index) => ({
            layer_id: layerId,
            priority: index,
            enabled: true,
          }),
        );
  return { chat_id: chatId, layers: layerIds, now };
}

async function handleRoleplayCharacterRequest(
  request: IncomingMessage,
  state: ServiceState,
  url: URL,
  profileId: string,
): Promise<AdminRouteResult> {
  const requestIdValue = requestId(request);
  const method = (request.method ?? "GET").toUpperCase();
  const parts = url.pathname.split("/").filter(Boolean);
  const characterId =
    parts.length >= 7 ? decodeURIComponent(parts[6]) : undefined;
  try {
    if (characterId === undefined) {
      if (method === "GET") {
        const includeArchived =
          url.searchParams.get("include_archived") === "true";
        const characters = (
          await listRoleplayCharacters(state, profileId)
        ).filter(
          (character) => includeArchived || character.status !== "archived",
        );
        return successRoute(requestIdValue, {
          profileId,
          items: characters,
          total: characters.length,
        });
      }
      if (method === "POST") {
        const character = roleplayCharacterFromBody(
          recordBody(await readJsonBody(request)),
          profileId,
          state.now(),
        );
        await putRoleplayJson(state, roleplayCharacterScope(profileId), {
          key: roleplayCharacterKey(character.id),
          value: character,
        });
        return successRoute(requestIdValue, { character });
      }
      return roleplayLoreMethodNotAllowed(
        requestIdValue,
        "roleplay character collection supports GET and POST",
      );
    }

    if (method === "GET") {
      const character = await getRoleplayCharacter(
        state,
        profileId,
        characterId,
      );
      if (character === undefined) {
        return roleplayNotFound(
          requestIdValue,
          "roleplay_character_not_found",
          `roleplay character ${characterId} was not found`,
        );
      }
      return successRoute(requestIdValue, { character });
    }
    if (method === "PATCH") {
      const current = await requireRoleplayCharacter(
        state,
        profileId,
        characterId,
      );
      const character = mergeRoleplayCharacter(
        current,
        recordBody(await readJsonBody(request)),
        state.now(),
      );
      await putRoleplayJson(state, roleplayCharacterScope(profileId), {
        key: roleplayCharacterKey(character.id),
        value: character,
      });
      return successRoute(requestIdValue, { character });
    }
    if (method === "DELETE") {
      const current = await requireRoleplayCharacter(
        state,
        profileId,
        characterId,
      );
      const character = {
        ...current,
        status: "archived" as const,
        updatedAt: state.now(),
      };
      await putRoleplayJson(state, roleplayCharacterScope(profileId), {
        key: roleplayCharacterKey(character.id),
        value: character,
      });
      return successRoute(requestIdValue, { character });
    }
    return roleplayLoreMethodNotAllowed(
      requestIdValue,
      "roleplay character item supports GET, PATCH, and DELETE",
    );
  } catch (error) {
    return roleplayInputError(
      requestIdValue,
      "roleplay_character_request_failed",
      error,
    );
  }
}

async function handleRoleplaySessionRequest(
  request: IncomingMessage,
  state: ServiceState,
  url: URL,
): Promise<AdminRouteResult> {
  const requestIdValue = requestId(request);
  const method = (request.method ?? "GET").toUpperCase();
  const parts = url.pathname.split("/").filter(Boolean);
  const sessionId =
    parts.length >= 5 ? decodeURIComponent(parts[4]) : undefined;
  const action = parts.length >= 6 ? parts[5] : undefined;
  try {
    if (sessionId === undefined) {
      if (method === "GET") {
        const profileId = url.searchParams.get("profile_id") ?? undefined;
        return successRoute(requestIdValue, {
          items: await listRoleplaySessions(state, profileId),
        });
      }
      if (method === "POST") {
        return successRoute(requestIdValue, {
          session: await createRoleplaySession(
            state,
            recordBody(await readJsonBody(request)),
          ),
        });
      }
      return roleplayLoreMethodNotAllowed(
        requestIdValue,
        "roleplay session collection supports GET and POST",
      );
    }

    if (action === "archive" && method === "POST") {
      const archived = await archiveRoleplaySession(state, sessionId);
      return successRoute(requestIdValue, { session: archived });
    }
    if (action === "restore" && method === "POST") {
      const restored = await restoreRoleplaySession(state, sessionId);
      return successRoute(requestIdValue, { session: restored });
    }
    if (action !== undefined) {
      return roleplayNotFound(
        requestIdValue,
        "unknown_roleplay_session_action",
        `unknown roleplay session action ${action}`,
      );
    }
    if (method === "GET") {
      const session = await getRoleplaySessionSummary(state, sessionId);
      if (session === undefined) {
        return roleplayNotFound(
          requestIdValue,
          "roleplay_session_not_found",
          `roleplay session ${sessionId} was not found`,
        );
      }
      return successRoute(requestIdValue, { session });
    }
    if (method === "PATCH") {
      const session = await updateRoleplaySessionMetadata(
        state,
        sessionId,
        recordBody(await readJsonBody(request)),
      );
      return successRoute(requestIdValue, { session });
    }
    return roleplayLoreMethodNotAllowed(
      requestIdValue,
      "roleplay session item supports GET and PATCH",
    );
  } catch (error) {
    return roleplayInputError(
      requestIdValue,
      "roleplay_session_request_failed",
      error,
    );
  }
}

async function handleRoleplayNarratorConfigRequest(
  request: IncomingMessage,
  state: ServiceState,
  _url: URL,
  profileId: string,
): Promise<AdminRouteResult> {
  const requestIdValue = requestId(request);
  const method = (request.method ?? "GET").toUpperCase();
  try {
    if (method === "GET") {
      return successRoute(requestIdValue, {
        profileId,
        config: await readRoleplayNarratorConfig(state, profileId),
        applies: "next_wake",
      });
    }
    if (method === "PATCH" || method === "POST") {
      const config = await writeRoleplayNarratorConfig(
        state,
        profileId,
        recordBody(await readJsonBody(request)),
      );
      return successRoute(requestIdValue, {
        profileId,
        config,
        applies: "next_wake",
      });
    }
    return roleplayLoreMethodNotAllowed(
      requestIdValue,
      "roleplay narrator config supports GET, PATCH, and POST",
    );
  } catch (error) {
    return roleplayInputError(
      requestIdValue,
      "roleplay_narrator_config_request_failed",
      error,
    );
  }
}

function roleplayCharacterScope(profileId: string): {
  scopeType: string;
  scopeId: string;
} {
  return { scopeType: "roleplay_profile", scopeId: profileId };
}

function roleplaySessionScope(sessionId: string): {
  scopeType: string;
  scopeId: string;
} {
  return { scopeType: "roleplay_session", scopeId: sessionId };
}

function roleplayCharacterKey(characterId: string): string {
  return `character:${characterId}`;
}

function roleplaySessionMetadataKey(): string {
  return "metadata";
}

async function putRoleplayJson(
  state: ServiceState,
  scope: { scopeType: string; scopeId: string },
  input: { key: string; value: unknown },
): Promise<NativeSimpleKvRecord> {
  return state.bridge.putSimpleKv({
    ...scope,
    key: input.key,
    valueJson: JSON.stringify(input.value),
    now: state.now(),
  });
}

async function listRoleplayJson<T>(
  state: ServiceState,
  scope: { scopeType: string; scopeId: string },
  keyPrefix: string,
): Promise<T[]> {
  const records = await state.bridge.listSimpleKv({
    ...scope,
    keyPrefix,
    limit: 1_000,
    offset: 0,
  });
  return records.map((record) => JSON.parse(record.valueJson) as T);
}

async function getRoleplayJson<T>(
  state: ServiceState,
  scope: { scopeType: string; scopeId: string },
  key: string,
): Promise<T | undefined> {
  const records = await state.bridge.listSimpleKv({
    ...scope,
    keyPrefix: key,
    limit: 1,
    offset: 0,
  });
  const exact = records.find((record) => record.key === key);
  return exact === undefined ? undefined : (JSON.parse(exact.valueJson) as T);
}

function roleplayCharacterFromBody(
  body: Record<string, unknown>,
  profileId: string,
  now: string,
): RoleplayCharacterRecord {
  const id =
    optionalString(body.id) ??
    optionalString(body.character_id) ??
    optionalString(body.characterId) ??
    `character-${randomBytes(6).toString("hex")}`;
  return {
    id,
    profileId,
    name: requiredString(body.name, "name"),
    description: optionalString(body.description) ?? "",
    personality: optionalString(body.personality) ?? "",
    scenario: optionalString(body.scenario) ?? "",
    firstMessage:
      optionalString(body.firstMessage) ??
      optionalString(body.first_message) ??
      "",
    alternateGreetings: optionalStringArray(
      body.alternateGreetings ?? body.alternate_greetings,
      [],
      "alternateGreetings",
    ),
    exampleMessages: optionalStringArray(
      body.exampleMessages ?? body.example_messages,
      [],
      "exampleMessages",
    ),
    tags: optionalStringArray(body.tags, [], "tags"),
    ...((optionalString(body.avatarUrl) ?? optionalString(body.avatar_url))
      ? {
          avatarUrl:
            optionalString(body.avatarUrl) ?? optionalString(body.avatar_url),
        }
      : {}),
    status: "active",
    createdAt: now,
    updatedAt: now,
  };
}

function mergeRoleplayCharacter(
  current: RoleplayCharacterRecord,
  body: Record<string, unknown>,
  now: string,
): RoleplayCharacterRecord {
  return {
    ...current,
    ...(optionalString(body.name) === undefined
      ? {}
      : { name: optionalString(body.name) }),
    ...(Object.hasOwn(body, "description")
      ? { description: optionalString(body.description) ?? "" }
      : {}),
    ...(Object.hasOwn(body, "personality")
      ? { personality: optionalString(body.personality) ?? "" }
      : {}),
    ...(Object.hasOwn(body, "scenario")
      ? { scenario: optionalString(body.scenario) ?? "" }
      : {}),
    ...(Object.hasOwn(body, "firstMessage") ||
    Object.hasOwn(body, "first_message")
      ? {
          firstMessage:
            optionalString(body.firstMessage) ??
            optionalString(body.first_message) ??
            "",
        }
      : {}),
    ...(body.alternateGreetings !== undefined ||
    body.alternate_greetings !== undefined
      ? {
          alternateGreetings: optionalStringArray(
            body.alternateGreetings ?? body.alternate_greetings,
            [],
            "alternateGreetings",
          ),
        }
      : {}),
    ...(body.exampleMessages !== undefined ||
    body.example_messages !== undefined
      ? {
          exampleMessages: optionalStringArray(
            body.exampleMessages ?? body.example_messages,
            [],
            "exampleMessages",
          ),
        }
      : {}),
    ...(body.tags === undefined
      ? {}
      : { tags: optionalStringArray(body.tags, [], "tags") }),
    ...(body.avatarUrl !== undefined || body.avatar_url !== undefined
      ? {
          avatarUrl:
            optionalString(body.avatarUrl) ?? optionalString(body.avatar_url),
        }
      : {}),
    status:
      optionalString(body.status) === "archived" ? "archived" : current.status,
    updatedAt: now,
  };
}

async function listRoleplayCharacters(
  state: ServiceState,
  profileId: string,
): Promise<RoleplayCharacterRecord[]> {
  return listRoleplayJson<RoleplayCharacterRecord>(
    state,
    roleplayCharacterScope(profileId),
    "character:",
  );
}

async function getRoleplayCharacter(
  state: ServiceState,
  profileId: string,
  characterId: string,
): Promise<RoleplayCharacterRecord | undefined> {
  return getRoleplayJson<RoleplayCharacterRecord>(
    state,
    roleplayCharacterScope(profileId),
    roleplayCharacterKey(characterId),
  );
}

async function requireRoleplayCharacter(
  state: ServiceState,
  profileId: string,
  characterId: string,
): Promise<RoleplayCharacterRecord> {
  const character = await getRoleplayCharacter(state, profileId, characterId);
  if (character === undefined) {
    throw new Error(`roleplay character ${characterId} was not found`);
  }
  return character;
}

async function roleplaySessionMetadata(
  state: ServiceState,
  session: Pick<
    SessionState,
    "sessionId" | "profileId" | "createdAt" | "lastActiveAt" | "status"
  >,
): Promise<RoleplaySessionMetadata> {
  const stored = await getRoleplayJson<RoleplaySessionMetadata>(
    state,
    roleplaySessionScope(session.sessionId),
    roleplaySessionMetadataKey(),
  );
  return {
    sessionId: session.sessionId,
    profileId: session.profileId,
    activeLayerIds: [],
    archived: session.status === "archived",
    createdAt: session.createdAt,
    updatedAt: session.lastActiveAt,
    ...(stored ?? {}),
  };
}

async function upsertRoleplaySessionMetadata(
  state: ServiceState,
  sessionId: string,
  patch: Partial<RoleplaySessionMetadata>,
): Promise<RoleplaySessionMetadata> {
  const session = await serviceSessionById(state, sessionId);
  const current = await roleplaySessionMetadata(state, session);
  const next: RoleplaySessionMetadata = {
    ...current,
    ...patch,
    sessionId,
    profileId: patch.profileId ?? current.profileId,
    activeLayerIds: patch.activeLayerIds ?? current.activeLayerIds,
    updatedAt: state.now(),
  };
  await putRoleplayJson(state, roleplaySessionScope(sessionId), {
    key: roleplaySessionMetadataKey(),
    value: next,
  });
  return next;
}

async function roleplaySessionSummary(
  state: ServiceState,
  session: SessionState,
): Promise<Record<string, unknown>> {
  const metadata = await roleplaySessionMetadata(state, session);
  const character =
    metadata.characterId === undefined
      ? undefined
      : await getRoleplayCharacter(
          state,
          session.profileId,
          metadata.characterId,
        );
  const chatLayers = await state.bridge
    .getChatLayers(session.sessionId)
    .catch(() => []);
  const activeLayerIds =
    metadata.activeLayerIds.length > 0
      ? metadata.activeLayerIds
      : chatLayers
          .filter((layer) => layer.enabled !== false)
          .sort(
            (left, right) =>
              numberValue(left.priority) - numberValue(right.priority),
          )
          .map((layer) => String(layer.layer_id));
  const lastEvent = listChatEventsAfterCursor(state, session, undefined, 1).at(
    -1,
  );
  return {
    session_id: session.sessionId,
    profile_id: session.profileId,
    agent_id: session.agentId,
    status: session.status,
    display_name: metadata.displayName,
    character_id: metadata.characterId,
    character_name: character?.name,
    active_layer_ids: activeLayerIds,
    active_layer_count: activeLayerIds.length,
    last_message_preview: lastEventPreview(lastEvent),
    archived: metadata.archived || session.status === "archived",
    created_at: metadata.createdAt,
    updated_at: metadata.updatedAt,
    metadata,
  };
}

async function roleplayPromptContextForSession(
  state: ServiceState,
  session: Pick<
    SessionState,
    "sessionId" | "profileId" | "createdAt" | "lastActiveAt" | "status"
  >,
): Promise<string | undefined> {
  const metadata = await roleplaySessionMetadata(state, session).catch(
    () => undefined,
  );
  if (metadata === undefined) return undefined;
  const character =
    metadata.characterId === undefined
      ? undefined
      : await getRoleplayCharacter(
          state,
          session.profileId,
          metadata.characterId,
        ).catch(() => undefined);
  if (character === undefined && metadata.activeLayerIds.length === 0) {
    return undefined;
  }
  const lines = [
    "# Roleplay Session Context",
    metadata.displayName ? `Session: ${metadata.displayName}` : undefined,
    character ? `Selected character: ${character.name}` : undefined,
    character?.description
      ? `Description: ${character.description}`
      : undefined,
    character?.personality
      ? `Personality: ${character.personality}`
      : undefined,
    character?.scenario ? `Scenario: ${character.scenario}` : undefined,
    character?.firstMessage
      ? `First message: ${character.firstMessage}`
      : undefined,
    character && character.alternateGreetings.length > 0
      ? `Alternate greetings: ${character.alternateGreetings.join(" | ")}`
      : undefined,
    character && character.exampleMessages.length > 0
      ? `Example messages: ${character.exampleMessages.join(" | ")}`
      : undefined,
    metadata.activeLayerIds.length > 0
      ? `Active lore layers: ${metadata.activeLayerIds.join(", ")}`
      : undefined,
    "Use this roleplay context as session-scoped setup. Prefer current chat evidence if it conflicts with older character or lore metadata.",
  ];
  return lines.filter((line): line is string => Boolean(line)).join("\n");
}

async function listRoleplaySessions(
  state: ServiceState,
  profileId: string | undefined,
): Promise<Record<string, unknown>[]> {
  const sessions = (await state.bridge.listSessions()).filter(
    (session) => profileId === undefined || session.profileId === profileId,
  );
  return Promise.all(
    sessions.map((session) => roleplaySessionSummary(state, session)),
  );
}

async function getRoleplaySessionSummary(
  state: ServiceState,
  sessionId: string,
): Promise<Record<string, unknown> | undefined> {
  const session = (await state.bridge.listSessions()).find(
    (candidate) => candidate.sessionId === sessionId,
  );
  return session === undefined
    ? undefined
    : roleplaySessionSummary(state, session);
}

async function createRoleplaySession(
  state: ServiceState,
  body: Record<string, unknown>,
): Promise<Record<string, unknown>> {
  const profileId = requiredString(
    body.profileId ?? body.profile_id,
    "profileId",
  );
  const registry = await state.bridge.getProfileRegistryRecord(profileId);
  const agentId =
    optionalString(body.agentId) ??
    optionalString(body.agent_id) ??
    registry?.agentId ??
    profileId;
  const sessionId =
    optionalString(body.sessionId) ??
    optionalString(body.session_id) ??
    `${agentId}-rp-${state
      .now()
      .replace(/[^0-9A-Za-z]/g, "")
      .slice(0, 17)}-${randomBytes(3).toString("hex")}`;
  const activeLayerIds = optionalStringArray(
    body.activeLayerIds ?? body.active_layer_ids,
    [],
    "activeLayerIds",
  );
  const session = await state.bridge.createSession({
    sessionId,
    agentId,
    profileId,
    kind: "full",
    resourceLimits: {},
    toolProfile: { tools: [] },
  });
  const metadata: Partial<RoleplaySessionMetadata> = {
    profileId,
    displayName:
      optionalString(body.displayName) ?? optionalString(body.display_name),
    characterId:
      optionalString(body.characterId) ?? optionalString(body.character_id),
    activeLayerIds,
    archived: false,
    createdAt: state.now(),
  };
  await upsertRoleplaySessionMetadata(state, session.sessionId, metadata);
  if (activeLayerIds.length > 0) {
    await state.bridge.setChatLayers({
      chat_id: session.sessionId,
      layers: activeLayerIds.map((layerId, index) => ({
        layer_id: layerId,
        priority: index,
        enabled: true,
      })),
      now: state.now(),
    });
  }
  return (
    (await getRoleplaySessionSummary(state, session.sessionId)) ?? {
      session_id: session.sessionId,
      profile_id: session.profileId,
      agent_id: session.agentId,
      status: session.status,
    }
  );
}

async function updateRoleplaySessionMetadata(
  state: ServiceState,
  sessionId: string,
  body: Record<string, unknown>,
): Promise<Record<string, unknown>> {
  const patch: Partial<RoleplaySessionMetadata> = {};
  if (
    Object.hasOwn(body, "displayName") ||
    Object.hasOwn(body, "display_name")
  ) {
    patch.displayName =
      optionalString(body.displayName) ?? optionalString(body.display_name);
  }
  if (
    Object.hasOwn(body, "characterId") ||
    Object.hasOwn(body, "character_id")
  ) {
    patch.characterId =
      optionalString(body.characterId) ?? optionalString(body.character_id);
  }
  if (
    Object.hasOwn(body, "activeLayerIds") ||
    Object.hasOwn(body, "active_layer_ids")
  ) {
    patch.activeLayerIds = optionalStringArray(
      body.activeLayerIds ?? body.active_layer_ids,
      [],
      "activeLayerIds",
    );
    await state.bridge.setChatLayers({
      chat_id: sessionId,
      layers: patch.activeLayerIds.map((layerId, index) => ({
        layer_id: layerId,
        priority: index,
        enabled: true,
      })),
      now: state.now(),
    });
  }
  await upsertRoleplaySessionMetadata(state, sessionId, patch);
  const summary = await getRoleplaySessionSummary(state, sessionId);
  if (summary === undefined)
    throw new Error(`roleplay session ${sessionId} missing`);
  return summary;
}

async function archiveRoleplaySession(
  state: ServiceState,
  sessionId: string,
): Promise<Record<string, unknown>> {
  await state.bridge.archiveSession(sessionId as SessionId);
  await upsertRoleplaySessionMetadata(state, sessionId, { archived: true });
  const summary = await getRoleplaySessionSummary(state, sessionId);
  if (summary === undefined)
    throw new Error(`roleplay session ${sessionId} missing`);
  return summary;
}

async function restoreRoleplaySession(
  state: ServiceState,
  sessionId: string,
): Promise<Record<string, unknown>> {
  const existing = await serviceSessionById(state, sessionId);
  await state.bridge.ensureConfiguredSession({
    sessionId,
    agentId: existing.agentId,
    profileId: existing.profileId,
    kind: existing.kind as "full" | "worker" | "delegated",
    resourceLimits: compactRecord(
      existing.resourceLimits as unknown as Record<string, unknown>,
    ),
    toolProfile: existing.toolProfile,
    historyWindow: existing.historyWindow,
  });
  await upsertRoleplaySessionMetadata(state, sessionId, { archived: false });
  const summary = await getRoleplaySessionSummary(state, sessionId);
  if (summary === undefined)
    throw new Error(`roleplay session ${sessionId} missing`);
  return summary;
}

function lastEventPreview(event: ChatEvent | undefined): string | undefined {
  if (event === undefined) return undefined;
  const body =
    optionalString(event.payload.body) ??
    optionalString(event.payload.text) ??
    optionalString(event.payload.summary);
  return body === undefined ? undefined : body.slice(0, 180);
}

async function readRoleplayNarratorConfig(
  state: ServiceState,
  profileId: string,
): Promise<BrowserRoleplayNarratorConfig> {
  const profile = await loadProfileConfig(
    state.runtimeConfig.profilesDir,
    profileId as ProfileId,
  );
  return normalizeRoleplayNarratorConfig(profile.roleplayNarrator ?? {});
}

async function writeRoleplayNarratorConfig(
  state: ServiceState,
  profileId: string,
  body: Record<string, unknown>,
): Promise<BrowserRoleplayNarratorConfig> {
  const config = normalizeRoleplayNarratorConfig(body.config ?? body);
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
  await writeJsonFileAtomic(profilePath, {
    ...raw,
    profileId,
    roleplayNarrator: config,
  });
  await applyServiceRuntimeConfigFromDisk(state, {
    createMissingSessions: false,
    eventType: "roleplay_narrator_config_updated",
    summaryPrefix: `Roleplay narrator config for ${profileId} updated`,
  });
  return config;
}

function normalizeRoleplayNarratorConfig(
  input: unknown,
): BrowserRoleplayNarratorConfig {
  const raw = recordBody(input);
  const review = optionalRecord(raw.review) ?? {};
  const maxReviewCycles =
    optionalNumber(review.maxReviewCycles ?? review.max_review_cycles) ?? 1;
  if (
    !Number.isInteger(maxReviewCycles) ||
    maxReviewCycles < 0 ||
    maxReviewCycles > 8
  ) {
    throw new Error(
      "review.maxReviewCycles must be an integer between 0 and 8",
    );
  }
  return {
    tone: enumValue(
      raw.tone,
      ["whimsical", "dramatic", "matter_of_fact", "lush", "wry"],
      "tone",
      "lush",
    ),
    pacing: enumValue(
      raw.pacing,
      ["leisurely", "balanced", "rapid", "breathless"],
      "pacing",
      "balanced",
    ),
    explicitness: enumValue(
      raw.explicitness,
      ["implied", "suggestive", "romantic", "steamy"],
      "explicitness",
      "romantic",
    ),
    memoryDepth: enumValue(
      raw.memoryDepth ?? raw.memory_depth,
      ["shallow", "medium", "deep"],
      "memoryDepth",
      "medium",
    ),
    ...(Object.hasOwn(raw, "stylePrompt") || Object.hasOwn(raw, "style_prompt")
      ? {
          stylePrompt:
            optionalString(raw.stylePrompt ?? raw.style_prompt) ?? "",
        }
      : {}),
    ...(Object.hasOwn(raw, "exemplar") || Object.hasOwn(raw, "styleExemplar")
      ? { exemplar: optionalString(raw.exemplar ?? raw.styleExemplar) ?? "" }
      : {}),
    review: {
      enabled: optionalBoolean(review.enabled) ?? false,
      maxReviewCycles,
    },
  };
}

function enumValue<T extends string>(
  value: unknown,
  allowed: readonly T[],
  fieldName: string,
  fallback: T,
): T {
  if (value === undefined) return fallback;
  if (typeof value === "string" && allowed.includes(value as T)) {
    return value as T;
  }
  throw new Error(`${fieldName} must be one of ${allowed.join(", ")}`);
}

function numberValue(value: unknown): number {
  return typeof value === "number" && Number.isFinite(value) ? value : 0;
}

function roleplayNotFound(
  requestIdValue: string,
  reasonCode: string,
  message: string,
): AdminRouteResult {
  return failure(404, requestIdValue, {
    code: "not_found",
    reason_code: reasonCode,
    message,
    retryable: false,
  });
}

function roleplayInputError(
  requestIdValue: string,
  reasonCode: string,
  error: unknown,
): AdminRouteResult {
  return failure(400, requestIdValue, {
    code: "invalid_input",
    reason_code: reasonCode,
    message: errorMessage(error, "roleplay request failed"),
    retryable: false,
  });
}

function requiredRouteString(
  value: string | undefined,
  fieldName: string,
): string {
  if (!value) throw new Error(`${fieldName} is required`);
  return value;
}

function requiredRouteBoolean(
  value: boolean | undefined,
  fieldName: string,
): boolean {
  if (value === undefined) throw new Error(`${fieldName} is required`);
  return value;
}

function requiredPositiveInteger(value: unknown, fieldName: string): number {
  const parsed = typeof value === "number" ? value : Number(value);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) {
    throw new Error(`${fieldName} must be a positive integer`);
  }
  return parsed;
}

async function handleProfileRegistryWriteRequest(
  request: {
    method: string;
    url: string;
    body?: unknown;
    requestId: string;
  },
  state: ServiceState,
): Promise<AdminRouteResult> {
  const method = request.method.toUpperCase();
  if (method !== "POST" && method !== "PATCH") {
    return failure(405, request.requestId, {
      code: "method_not_allowed",
      reason_code: "profile_registry_write_requires_post_or_patch",
      message: "profile registry write routes support POST or PATCH",
      retryable: false,
    });
  }
  const route = parseProfileRegistryWriteRoute(new URL(request.url).pathname);
  if (route === undefined) {
    return failure(404, request.requestId, {
      code: "not_found",
      reason_code: "unknown_profile_registry_write_route",
      message: "unknown profile registry write route",
      retryable: false,
    });
  }
  let plan: ProfileRegistryWritePlan | ProfileRegistryRuntimeConfigPlan;
  try {
    plan =
      route.kind === "runtime-config"
        ? await planProfileRegistryRuntimeConfigWrite(
            state,
            route,
            request.body,
          )
        : await planProfileRegistryWrite(state, route, request.body);
  } catch (error) {
    if (
      error instanceof Error &&
      error.message.includes(
        "was not found; create or import a DB-backed profile",
      )
    ) {
      return failure(404, request.requestId, {
        code: "not_found",
        reason_code: "profile_registry_record_missing",
        message: error.message,
        retryable: false,
      });
    }
    throw error;
  }
  if (route.mode === "plan") return successRoute(request.requestId, plan);
  if (!plan.ok) return successRoute(request.requestId, plan);
  const updated = await state.bridge.updateProfileRegistryRecord({
    write: plan.nextWrite,
    expectedRevision: plan.expectedRevision,
  });
  const effects =
    route.kind === "lifecycle"
      ? await applyProfileRegistryLifecycleEffects(state, updated)
      : route.kind === "runtime-config"
        ? await applyProfileRegistryRuntimeConfigEffects(
            state,
            updated,
            plan as ProfileRegistryRuntimeConfigPlan,
          )
        : undefined;
  return successRoute(request.requestId, {
    ...plan,
    applied: true,
    record: updated,
    effects,
  });
}

interface ProfileRegistryWriteRoute {
  profileId: string;
  kind: "update" | "lifecycle" | "prompt" | "runtime-config";
  mode: "plan" | "apply";
}

interface ProfileRegistryWritePlan {
  ok: boolean;
  profileId: string;
  kind: "update" | "lifecycle" | "prompt";
  mode: "plan" | "apply";
  expectedRevision: number;
  current: NativeProfileRegistryRecord;
  next: NativeProfileRegistryRecord;
  nextWrite: NativeProfileRegistryWrite;
  diagnostics: Array<{
    severity: "error" | "warning" | "info";
    code: string;
    path: string;
    message: string;
  }>;
  implications: {
    registryRevisionWillIncrement: true;
    profileFilesUnchanged: true;
    serviceConfigUnchanged: true;
    runtimeRebuildRecommended: boolean;
    lifecycleEffects: "none" | "archive_active_sessions_and_unregister_brain";
  };
}

interface ProfileRegistryRuntimeConfigPlan {
  ok: boolean;
  profileId: string;
  mode: "plan" | "apply";
  expectedRevision: number;
  current: NativeProfileRegistryRecord;
  next: NativeProfileRegistryRecord;
  nextWrite: NativeProfileRegistryWrite;
  runtimeConfig: EditableProfileRuntimeConfig;
  diagnostics: ProfileRegistryWritePlan["diagnostics"];
  implications: {
    registryRevisionWillIncrement: true;
    profileFileWillChange: boolean;
    serviceConfigWillChange: boolean;
    configReloadRequired: true;
    runtimeRebuildRecommended: boolean;
    mcpRefreshRecommended: boolean;
  };
}

interface EditableProfileRuntimeConfig {
  providerAlias: string;
  brain?: { module?: string; strategy?: string };
  localToolProfileId?: string;
  toolPolicy?: {
    requestedToolsets?: string[];
    requestedTools?: string[];
    deniedTools?: string[];
    includeDeprecated?: boolean;
  };
  contextPolicy: ContextStrategyPolicy;
  mcpBindings: Array<{
    serverId: string;
    bindingId?: string;
    adapterId?: string;
    serverNames?: string[];
    transport?: string;
    toolProfileKey?: string;
  }>;
}

function parseProfileRegistryWriteRoute(
  pathname: string,
): ProfileRegistryWriteRoute | undefined {
  const parts = pathname.split("/").filter(Boolean);
  if (
    parts.length !== 7 ||
    parts[0] !== "v1" ||
    parts[1] !== "admin" ||
    parts[2] !== "profiles" ||
    parts[3] !== "registry"
  ) {
    return undefined;
  }
  const kind = parts[5];
  const mode = parts[6];
  if (
    (kind !== "update" &&
      kind !== "lifecycle" &&
      kind !== "prompt" &&
      kind !== "runtime-config") ||
    (mode !== "plan" && mode !== "apply")
  ) {
    return undefined;
  }
  return {
    profileId: decodeURIComponent(parts[4] ?? ""),
    kind: kind === "runtime-config" ? "runtime-config" : kind,
    mode,
  };
}

async function planProfileRegistryWrite(
  state: ServiceState,
  route: ProfileRegistryWriteRoute,
  body: unknown,
): Promise<ProfileRegistryWritePlan> {
  if (!isRecord(body)) {
    throw new Error("profile registry write body must be an object");
  }
  const current = await state.bridge.getProfileRegistryRecord(route.profileId);
  if (current === undefined) {
    throw new Error(
      `profile registry record ${route.profileId} was not found; create or import a DB-backed profile before registry mutation`,
    );
  }
  const expectedRevision = requiredRevision(body);
  const diagnostics: ProfileRegistryWritePlan["diagnostics"] = [];
  if (expectedRevision !== current.revision) {
    diagnostics.push({
      severity: "error",
      code: "profile_registry_revision_mismatch",
      path: "expectedRevision",
      message: `expected revision ${expectedRevision}, found ${current.revision}`,
    });
  }
  const next =
    route.kind === "lifecycle"
      ? nextProfileRegistryLifecycleRecord(current, body, state.now())
      : route.kind === "prompt"
        ? nextProfileRegistryPromptRecord(current, body, state.now())
        : nextProfileRegistryFieldRecord(current, body, state.now());
  const nextWrite = profileRegistryRecordToWrite(next, state.now());
  return {
    ok: !diagnostics.some((diagnostic) => diagnostic.severity === "error"),
    profileId: route.profileId,
    kind: route.kind as "update" | "lifecycle" | "prompt",
    mode: route.mode,
    expectedRevision,
    current,
    next,
    nextWrite,
    diagnostics,
    implications: {
      registryRevisionWillIncrement: true,
      profileFilesUnchanged: true,
      serviceConfigUnchanged: true,
      runtimeRebuildRecommended:
        route.kind === "lifecycle" ||
        JSON.stringify(current.activeRuntimeSettingsJson) !==
          JSON.stringify(next.activeRuntimeSettingsJson) ||
        current.defaultSessionKind !== next.defaultSessionKind ||
        current.agentId !== next.agentId ||
        current.promptSoulMarkdown !== next.promptSoulMarkdown ||
        current.promptMemoryMarkdown !== next.promptMemoryMarkdown,
      lifecycleEffects:
        route.kind === "lifecycle" && next.lifecycleStatus !== "active"
          ? "archive_active_sessions_and_unregister_brain"
          : "none",
    },
  };
}

async function planProfileRegistryRuntimeConfigWrite(
  state: ServiceState,
  route: ProfileRegistryWriteRoute,
  body: unknown,
): Promise<ProfileRegistryRuntimeConfigPlan> {
  if (!isRecord(body)) {
    throw new Error("profile registry runtime-config body must be an object");
  }
  const current = await state.bridge.getProfileRegistryRecord(route.profileId);
  if (current === undefined) {
    throw new Error(
      `profile registry record ${route.profileId} was not found; create or import a DB-backed profile before registry mutation`,
    );
  }
  const expectedRevision = requiredRevision(body);
  const diagnostics: ProfileRegistryRuntimeConfigPlan["diagnostics"] = [];
  if (expectedRevision !== current.revision) {
    diagnostics.push({
      severity: "error",
      code: "profile_registry_revision_mismatch",
      path: "expectedRevision",
      message: `expected revision ${expectedRevision}, found ${current.revision}`,
    });
  }

  const existing = await editableRuntimeConfigForProfile(state, current);
  const runtimeConfig = await editableRuntimeConfigFromBody(
    state,
    current,
    existing,
    body,
    diagnostics,
  );
  const next = nextProfileRegistryRuntimeConfigRecord(
    current,
    runtimeConfig,
    state.now(),
  );
  const nextWrite = profileRegistryRecordToWrite(next, state.now());
  return {
    ok: !diagnostics.some((diagnostic) => diagnostic.severity === "error"),
    profileId: route.profileId,
    mode: route.mode,
    expectedRevision,
    current,
    next,
    nextWrite,
    runtimeConfig,
    diagnostics,
    implications: {
      registryRevisionWillIncrement: true,
      profileFileWillChange:
        JSON.stringify(existing.profileFileRuntimeConfig) !==
        JSON.stringify(profileFileRuntimeConfig(runtimeConfig)),
      serviceConfigWillChange:
        JSON.stringify(existing.mcpBindings) !==
        JSON.stringify(runtimeConfig.mcpBindings),
      configReloadRequired: true,
      runtimeRebuildRecommended:
        existing.runtimeConfig.providerAlias !== runtimeConfig.providerAlias ||
        JSON.stringify(existing.runtimeConfig.brain ?? {}) !==
          JSON.stringify(runtimeConfig.brain ?? {}) ||
        JSON.stringify(existing.runtimeConfig.contextPolicy) !==
          JSON.stringify(runtimeConfig.contextPolicy),
      mcpRefreshRecommended:
        JSON.stringify(existing.mcpBindings) !==
        JSON.stringify(runtimeConfig.mcpBindings),
    },
  };
}

function nextProfileRegistryFieldRecord(
  current: NativeProfileRegistryRecord,
  body: Record<string, unknown>,
  now: string,
): NativeProfileRegistryRecord {
  return {
    ...current,
    displayName: bodyFieldString(body, "displayName", current.displayName),
    summary: bodyFieldString(body, "summary", current.summary),
    defaultSessionKind: bodySessionKind(
      body,
      "defaultSessionKind",
      current.defaultSessionKind,
    ),
    agentId: bodyFieldString(body, "agentId", current.agentId),
    ownerId: bodyFieldString(body, "ownerId", current.ownerId),
    activeRuntimeSettingsJson: Object.hasOwn(body, "activeRuntimeSettingsJson")
      ? (body.activeRuntimeSettingsJson ?? {})
      : current.activeRuntimeSettingsJson,
    updatedAt: now,
  };
}

function nextProfileRegistryRuntimeConfigRecord(
  current: NativeProfileRegistryRecord,
  runtimeConfig: EditableProfileRuntimeConfig,
  now: string,
): NativeProfileRegistryRecord {
  return {
    ...current,
    activeRuntimeSettingsJson: profileRuntimeSettingsJson(runtimeConfig),
    derivedRuntimeRefs: [
      ...current.derivedRuntimeRefs.filter(
        (ref) => ref.refKind !== "mcp_binding",
      ),
      ...runtimeConfig.mcpBindings.map((binding) => ({
        refKind: "mcp_binding",
        refId:
          binding.bindingId ??
          `${current.agentId ?? current.profileId}-mcp-${binding.serverId}`,
        status: "planned",
        updatedAt: now,
        metadataJson: {
          server_id: binding.serverId,
          server_names: binding.serverNames ?? [binding.serverId],
          endpoint_ref: `config://mcp/${binding.serverId}`,
          tool_profile_key: binding.toolProfileKey ?? current.profileId,
        },
      })),
    ],
    updatedAt: now,
  };
}

function nextProfileRegistryPromptRecord(
  current: NativeProfileRegistryRecord,
  body: Record<string, unknown>,
  now: string,
): NativeProfileRegistryRecord {
  return {
    ...current,
    promptSoulMarkdown: bodyMarkdownField(
      body,
      "soulMarkdown",
      "promptSoulMarkdown",
      current.promptSoulMarkdown,
    ),
    promptMemoryMarkdown: bodyMarkdownField(
      body,
      "memoryMarkdown",
      "promptMemoryMarkdown",
      current.promptMemoryMarkdown,
    ),
    updatedAt: now,
  };
}

function nextProfileRegistryLifecycleRecord(
  current: NativeProfileRegistryRecord,
  body: Record<string, unknown>,
  now: string,
): NativeProfileRegistryRecord {
  const lifecycleStatus = profileRegistryLifecycleStatusFromBody(
    body.lifecycleStatus ?? body.lifecycle_status,
  );
  return {
    ...current,
    lifecycleStatus,
    derivedRuntimeRefs: current.derivedRuntimeRefs.map((ref) => ({
      ...ref,
      status: derivedRuntimeRefStatusForLifecycle(lifecycleStatus),
      updatedAt: now,
    })),
    updatedAt: now,
  };
}

function profileRegistryRecordToWrite(
  record: NativeProfileRegistryRecord,
  now: string,
): NativeProfileRegistryWrite {
  return {
    profileId: record.profileId,
    lifecycleStatus: record.lifecycleStatus,
    displayName: record.displayName,
    summary: record.summary,
    defaultSessionKind: record.defaultSessionKind,
    agentId: record.agentId,
    ownerId: record.ownerId,
    promptSoulMarkdown: record.promptSoulMarkdown,
    promptMemoryMarkdown: record.promptMemoryMarkdown,
    activeRuntimeSettingsJson: record.activeRuntimeSettingsJson ?? {},
    sourceAssetRefs: record.sourceAssetRefs,
    derivedRuntimeRefs: record.derivedRuntimeRefs,
    importExport: record.importExport,
    now,
  };
}

async function editableRuntimeConfigForProfile(
  state: ServiceState,
  record: NativeProfileRegistryRecord,
): Promise<{
  runtimeConfig: EditableProfileRuntimeConfig;
  profileFileRuntimeConfig: ReturnType<typeof profileFileRuntimeConfig>;
  mcpBindings: EditableProfileRuntimeConfig["mcpBindings"];
}> {
  const profile = await loadProfileConfig(
    state.runtimeConfig.profilesDir,
    record.profileId as ProfileId,
  ).catch(() => undefined);
  const settings = optionalRecord(record.activeRuntimeSettingsJson) ?? {};
  const providerAlias =
    optionalString(settings.providerAlias) ??
    optionalString(settings.provider_alias) ??
    profile?.providerAlias ??
    "default";
  const mcpBindings = state.runtimeConfig.mcpBindings
    .filter((binding) => String(binding.profileId) === record.profileId)
    .map(editableMcpBindingFromRuntime);
  const runtimeConfig: EditableProfileRuntimeConfig = {
    providerAlias,
    brain:
      profile?.brain ??
      brainMetadataFromUnknown(settings.brain) ??
      defaultProfileBrainForModelProvider(
        (await state.bridge.getModelProvider(providerAlias)) ??
          ({
            providerKind: "local",
            protocol: "chat_completions",
          } as NativeModelProviderRecord),
      ),
    localToolProfileId:
      profile?.localToolProfileId ??
      optionalString(settings.localToolProfileId) ??
      optionalString(settings.local_tool_profile_id),
    toolPolicy:
      editableToolPolicy(profile?.toolPolicy) ??
      profileToolPolicyFromUnknown(settings.toolPolicy ?? settings.tool_policy),
    contextPolicy:
      profile?.contextPolicy ??
      contextStrategyPolicyFromUnknown(
        settings.contextPolicy ?? settings.context_policy,
      ),
    mcpBindings,
  };
  return {
    runtimeConfig,
    profileFileRuntimeConfig: profileFileRuntimeConfig(runtimeConfig),
    mcpBindings,
  };
}

async function editableRuntimeConfigFromBody(
  state: ServiceState,
  record: NativeProfileRegistryRecord,
  existing: Awaited<ReturnType<typeof editableRuntimeConfigForProfile>>,
  body: Record<string, unknown>,
  diagnostics: ProfileRegistryRuntimeConfigPlan["diagnostics"],
): Promise<EditableProfileRuntimeConfig> {
  const providerAlias = Object.hasOwn(body, "providerAlias")
    ? requiredString(body.providerAlias, "providerAlias")
    : existing.runtimeConfig.providerAlias;
  const modelProvider = await state.bridge.getModelProvider(providerAlias);
  if (modelProvider === undefined) {
    diagnostics.push({
      severity: "error",
      code: "model_provider_not_found",
      path: "providerAlias",
      message: `model provider alias ${providerAlias} was not found`,
    });
  } else if (modelProvider.status !== "active") {
    diagnostics.push({
      severity: "error",
      code: "model_provider_not_active",
      path: "providerAlias",
      message: `model provider alias ${providerAlias} is ${modelProvider.status}; active provider required`,
    });
  }

  const brain = Object.hasOwn(body, "brain")
    ? profileBrainFromBody(body.brain)
    : Object.hasOwn(body, "providerAlias") && modelProvider !== undefined
      ? defaultProfileBrainForModelProvider(modelProvider)
      : existing.runtimeConfig.brain;

  const localToolProfileId = Object.hasOwn(body, "localToolProfileId")
    ? optionalString(body.localToolProfileId)
    : existing.runtimeConfig.localToolProfileId;
  let toolPolicy = Object.hasOwn(body, "toolPolicy")
    ? (profileToolPolicyFromUnknown(body.toolPolicy) ?? {})
    : existing.runtimeConfig.toolPolicy;
  if (localToolProfileId !== undefined) {
    try {
      const localToolProfile = await createLocalToolProfileStore({
        bridge: state.bridge,
        now: state.now,
      }).resolve(localToolProfileId);
      toolPolicy = localToolProfile.toolPolicy;
    } catch (error) {
      diagnostics.push({
        severity: "error",
        code:
          error instanceof LocalToolProfileError
            ? error.reasonCode
            : "local_tool_profile_invalid",
        path: "localToolProfileId",
        message: errorMessage(
          error,
          `local tool profile ${localToolProfileId} is invalid`,
        ),
      });
    }
  } else {
    validateInlineToolPolicy(toolPolicy, diagnostics);
  }

  const mcpBindings = Object.hasOwn(body, "mcpBindings")
    ? editableMcpBindingsFromBody(body.mcpBindings)
    : existing.runtimeConfig.mcpBindings;
  const contextPolicy = Object.hasOwn(body, "contextPolicy")
    ? contextStrategyPolicyFromPatch(
        body.contextPolicy,
        existing.runtimeConfig.contextPolicy,
      )
    : {
        policy: existing.runtimeConfig.contextPolicy,
        diagnostics: [],
      };
  diagnostics.push(...contextPolicy.diagnostics);

  return {
    providerAlias,
    brain,
    localToolProfileId,
    toolPolicy,
    contextPolicy: contextPolicy.policy,
    mcpBindings: mcpBindings.map((binding, index) =>
      normalizedEditableMcpBinding(record, binding, index),
    ),
  };
}

function profileRuntimeSettingsJson(
  runtimeConfig: EditableProfileRuntimeConfig,
): Record<string, unknown> {
  return compactRecord({
    provider_alias: runtimeConfig.providerAlias,
    providerAlias: runtimeConfig.providerAlias,
    brain: runtimeConfig.brain,
    skills_mode: "all",
    localToolProfileId: runtimeConfig.localToolProfileId,
    toolPolicy: runtimeConfig.toolPolicy,
    contextPolicy: runtimeConfig.contextPolicy,
    mcp_bindings: runtimeConfig.mcpBindings.map((binding) => ({
      server_id: binding.serverId,
      binding_id: binding.bindingId,
      adapter_id: binding.adapterId,
      server_names: binding.serverNames ?? [binding.serverId],
      transport: binding.transport ?? "streamable_http",
      tool_profile_key: binding.toolProfileKey,
      endpoint_ref: `config://mcp/${binding.serverId}`,
    })),
    mcpBindings: runtimeConfig.mcpBindings,
    profile: profileFileRuntimeConfig(runtimeConfig),
  });
}

function profileFileRuntimeConfig(
  runtimeConfig: EditableProfileRuntimeConfig,
): Record<string, unknown> {
  return compactRecord({
    providerAlias: runtimeConfig.providerAlias,
    brain: runtimeConfig.brain,
    localToolProfileId: runtimeConfig.localToolProfileId,
    toolPolicy: runtimeConfig.toolPolicy,
    contextPolicy: runtimeConfig.contextPolicy,
  });
}

function applyEditableRuntimeConfigToProfileJson(
  profileConfig: Record<string, unknown>,
  runtimeConfig: EditableProfileRuntimeConfig,
): void {
  profileConfig.providerAlias = runtimeConfig.providerAlias;
  delete profileConfig.modelConfig;
  if (runtimeConfig.brain === undefined) {
    delete profileConfig.brain;
  } else {
    profileConfig.brain = runtimeConfig.brain;
  }
  if (runtimeConfig.localToolProfileId === undefined) {
    delete profileConfig.localToolProfileId;
  } else {
    profileConfig.localToolProfileId = runtimeConfig.localToolProfileId;
  }
  if (runtimeConfig.toolPolicy === undefined) {
    delete profileConfig.toolPolicy;
  } else {
    profileConfig.toolPolicy = runtimeConfig.toolPolicy;
  }
  profileConfig.contextPolicy = runtimeConfig.contextPolicy;
}

async function readProfileConfigJsonForMutation(
  profilePath: string,
  profileId: string,
): Promise<Record<string, unknown>> {
  let parsed: unknown;
  try {
    parsed = JSON.parse(await readFile(profilePath, "utf8"));
  } catch (error) {
    if (isNodeError(error) && error.code === "ENOENT") {
      parsed = { profileId };
    } else {
      throw error;
    }
  }
  if (!isRecord(parsed)) {
    throw new Error(`profile ${profileId} config root must be an object`);
  }
  parsed.profileId = profileId;
  return parsed;
}

function runtimeMcpBindingsForProfile(
  state: ServiceState,
  record: NativeProfileRegistryRecord,
  runtimeConfig: EditableProfileRuntimeConfig,
): Record<string, unknown>[] {
  const session = state.runtimeConfig.sessions.find(
    (candidate) => String(candidate.profileId) === record.profileId,
  );
  const agentId = String(
    record.agentId ?? session?.agentId ?? record.profileId,
  );
  return runtimeConfig.mcpBindings.map((binding, index) => ({
    bindingId: binding.bindingId ?? `${agentId}-mcp-${index + 1}`,
    adapterId: binding.adapterId ?? "mcp-ts-main",
    agentId,
    sessionId: String(session?.sessionId ?? `${record.profileId}-session`),
    profileId: record.profileId,
    serverNames: binding.serverNames ?? [binding.serverId],
    endpointRef: `config://mcp/${binding.serverId}`,
    transport: binding.transport ?? "streamable_http",
    toolProfileKey: binding.toolProfileKey ?? record.profileId,
    status: "active",
    diagnostics: {},
  }));
}

function editableMcpBindingFromRuntime(
  binding: McpBindingRecord,
): EditableProfileRuntimeConfig["mcpBindings"][number] {
  return {
    serverId:
      serverIdFromEndpointRef(binding.endpointRef) ??
      binding.serverNames[0] ??
      binding.bindingId,
    bindingId: binding.bindingId,
    adapterId: String(binding.adapterId),
    serverNames: binding.serverNames,
    transport: binding.transport,
    toolProfileKey: binding.toolProfileKey,
  };
}

function editableMcpBindingsFromBody(
  value: unknown,
): EditableProfileRuntimeConfig["mcpBindings"] {
  if (value === undefined || value === null) return [];
  if (!Array.isArray(value)) {
    throw new Error("mcpBindings must be an array when provided");
  }
  return value.map((item, index) => {
    if (!isRecord(item)) {
      throw new Error(`mcpBindings[${index}] must be an object`);
    }
    const serverId = optionalString(item.serverId);
    if (serverId === undefined) {
      throw new Error(`mcpBindings[${index}].serverId is required`);
    }
    return {
      serverId,
      bindingId: optionalString(item.bindingId),
      adapterId: optionalString(item.adapterId),
      serverNames:
        item.serverNames === undefined
          ? undefined
          : stringArray(item.serverNames, `mcpBindings[${index}].serverNames`),
      transport: optionalString(item.transport),
      toolProfileKey:
        optionalString(item.toolProfileKey) ?? optionalString(item.toolProfile),
    };
  });
}

function normalizedEditableMcpBinding(
  record: NativeProfileRegistryRecord,
  binding: EditableProfileRuntimeConfig["mcpBindings"][number],
  index: number,
): EditableProfileRuntimeConfig["mcpBindings"][number] {
  const agentId = String(record.agentId ?? record.profileId);
  return {
    ...binding,
    bindingId: binding.bindingId ?? `${agentId}-mcp-${index + 1}`,
    adapterId: binding.adapterId ?? "mcp-ts-main",
    serverNames: binding.serverNames ?? [binding.serverId],
    transport: binding.transport ?? "streamable_http",
    toolProfileKey: binding.toolProfileKey ?? record.profileId,
  };
}

function profileToolPolicyFromUnknown(
  value: unknown,
): EditableProfileRuntimeConfig["toolPolicy"] | undefined {
  const policy = optionalRecord(value);
  if (policy === undefined) return undefined;
  return {
    requestedToolsets:
      policy.requestedToolsets === undefined
        ? undefined
        : stringArray(policy.requestedToolsets, "toolPolicy.requestedToolsets"),
    requestedTools:
      policy.requestedTools === undefined
        ? undefined
        : stringArray(policy.requestedTools, "toolPolicy.requestedTools"),
    deniedTools:
      policy.deniedTools === undefined
        ? undefined
        : stringArray(policy.deniedTools, "toolPolicy.deniedTools"),
    includeDeprecated:
      typeof policy.includeDeprecated === "boolean"
        ? policy.includeDeprecated
        : undefined,
  };
}

function editableToolPolicy(
  policy: ProfileConfig["toolPolicy"],
): EditableProfileRuntimeConfig["toolPolicy"] | undefined {
  if (policy === undefined) return undefined;
  return {
    requestedToolsets:
      policy.requestedToolsets === undefined
        ? undefined
        : [...policy.requestedToolsets],
    requestedTools:
      policy.requestedTools === undefined
        ? undefined
        : [...policy.requestedTools],
    deniedTools:
      policy.deniedTools === undefined ? undefined : [...policy.deniedTools],
    includeDeprecated: policy.includeDeprecated,
  };
}

function validateInlineToolPolicy(
  policy: EditableProfileRuntimeConfig["toolPolicy"],
  diagnostics: ProfileRegistryRuntimeConfigPlan["diagnostics"],
): void {
  const catalog = buildBuiltInToolCatalog();
  const validToolsets = new Set(catalog.toolsets.map((toolset) => toolset.id));
  const validTools = new Set(catalog.tools.map((tool) => tool.name));
  for (const toolset of policy?.requestedToolsets ?? []) {
    if (toolset.startsWith("mcp:")) {
      diagnostics.push({
        severity: "error",
        code: "inline_tool_policy_rejects_mcp_toolset",
        path: "toolPolicy.requestedToolsets",
        message: `inline tool policy cannot reference dynamic MCP toolset ${toolset}`,
      });
    } else if (!validToolsets.has(toolset)) {
      diagnostics.push({
        severity: "error",
        code: "inline_tool_policy_unknown_toolset",
        path: "toolPolicy.requestedToolsets",
        message: `inline tool policy references unknown built-in toolset ${toolset}`,
      });
    }
  }
  for (const tool of policy?.requestedTools ?? []) {
    if (!validTools.has(tool)) {
      diagnostics.push({
        severity: "error",
        code: "inline_tool_policy_unknown_tool",
        path: "toolPolicy.requestedTools",
        message: `inline tool policy references unknown built-in tool ${tool}`,
      });
    }
  }
}

function brainMetadataFromUnknown(
  value: unknown,
): EditableProfileRuntimeConfig["brain"] | undefined {
  const brain = optionalRecord(value);
  if (brain === undefined) return undefined;
  return compactRecord({
    module: optionalString(brain.module),
    strategy: optionalString(brain.strategy),
  }) as EditableProfileRuntimeConfig["brain"];
}

function serverIdFromEndpointRef(
  value: string | undefined,
): string | undefined {
  const prefix = "config://mcp/";
  return value?.startsWith(prefix) ? value.slice(prefix.length) : undefined;
}

async function applyProfileRegistryLifecycleEffects(
  state: ServiceState,
  record: NativeProfileRegistryRecord,
): Promise<{
  sessionsArchived: string[];
  brainHandle: DecommissionedServiceProfile["brainHandle"];
}> {
  if (record.lifecycleStatus === "active") {
    return { sessionsArchived: [], brainHandle: { action: "already_absent" } };
  }
  const sessions = await state.bridge.listSessions();
  const profileSessions = sessions.filter(
    (session) =>
      String(session.profileId) === record.profileId &&
      session.status !== "archived",
  );
  const inFlightSessionIds = profileSessions
    .map((session) => String(session.sessionId))
    .filter((sessionId) => state.inFlightWakes.has(sessionId as SessionId));
  if (inFlightSessionIds.length > 0) {
    throw new Error(
      `profile ${record.profileId} lifecycle transition blocked by in-flight wake(s): ${inFlightSessionIds.join(", ")}`,
    );
  }
  const sessionsArchived: string[] = [];
  for (const session of profileSessions) {
    await state.bridge.archiveSession(session.sessionId);
    sessionsArchived.push(String(session.sessionId));
  }
  const brainHandle = await unregisterServiceProfileBrain(
    state,
    record.profileId,
  );
  return { sessionsArchived, brainHandle };
}

async function applyProfileRegistryRuntimeConfigEffects(
  state: ServiceState,
  record: NativeProfileRegistryRecord,
  plan: ProfileRegistryRuntimeConfigPlan,
): Promise<{
  profilePath: string;
  runtimeConfigPath: string;
  mcpBindings: { removed: number; added: number };
  applyResult: RustyCrewRuntimeConfigApplyResult;
}> {
  const profilePath = safeProfileConfigPath(
    state.runtimeConfig.profilesDir,
    record.profileId,
  );
  if (profilePath === undefined) {
    throw new Error(
      `profile id ${record.profileId} is not a valid file profile id`,
    );
  }
  const profileConfig = await readProfileConfigJsonForMutation(
    profilePath,
    record.profileId,
  );
  applyEditableRuntimeConfigToProfileJson(profileConfig, plan.runtimeConfig);
  await writeJsonFileAtomic(profilePath, profileConfig);

  const runtimeConfigFile = await readRuntimeConfigFileForMutation(state);
  const mcpBindings = runtimeConfigFile.array("mcpBindings");
  const removed = removeRuntimeConfigEntries(
    mcpBindings,
    (entry) =>
      runtimeEntryString(entry, "profileId", "profile_id") === record.profileId,
  );
  const runtimeMcpBindings = runtimeMcpBindingsForProfile(
    state,
    record,
    plan.runtimeConfig,
  );
  mcpBindings.push(...runtimeMcpBindings);
  await writeJsonFileAtomic(
    state.config.paths.serviceConfigFile,
    runtimeConfigFile.value,
  );

  const applyResult = await applyServiceRuntimeConfigFromDisk(state, {
    createMissingSessions: false,
    eventType: "profile_runtime_config_updated",
    summaryPrefix: `Profile ${record.profileId} runtime config updated`,
  });
  return {
    profilePath,
    runtimeConfigPath: state.config.paths.serviceConfigFile,
    mcpBindings: { removed, added: runtimeMcpBindings.length },
    applyResult,
  };
}

function requiredRevision(body: Record<string, unknown>): number {
  const value = body.expectedRevision ?? body.expected_revision;
  if (!Number.isSafeInteger(value) || Number(value) < 1) {
    throw new Error(
      "expectedRevision is required and must be a positive integer",
    );
  }
  return Number(value);
}

function bodyFieldString(
  body: Record<string, unknown>,
  key: string,
  current: string | undefined,
): string | undefined {
  if (!Object.hasOwn(body, key)) return current;
  const value = body[key];
  if (value === null) return undefined;
  if (typeof value === "string") return value.trim() ? value.trim() : undefined;
  throw new Error(`${key} must be a string or null`);
}

function bodyMarkdownField(
  body: Record<string, unknown>,
  camelKey: string,
  registryKey: string,
  current: string | undefined,
): string | undefined {
  const key = Object.hasOwn(body, camelKey)
    ? camelKey
    : Object.hasOwn(body, registryKey)
      ? registryKey
      : undefined;
  if (key === undefined) return current;
  const value = body[key];
  if (value === null) return undefined;
  if (typeof value === "string") return value;
  throw new Error(`${camelKey} must be a string or null`);
}

function bodySessionKind(
  body: Record<string, unknown>,
  key: string,
  current: "full" | "worker" | "delegated" | undefined,
): "full" | "worker" | "delegated" | undefined {
  if (!Object.hasOwn(body, key)) return current;
  const value = body[key];
  if (value === null) return undefined;
  if (value === "full" || value === "worker" || value === "delegated") {
    return value;
  }
  throw new Error(`${key} must be full, worker, delegated, or null`);
}

function profileRegistryLifecycleStatusFromBody(
  value: unknown,
): NativeProfileRegistryLifecycleStatus {
  if (
    value === "active" ||
    value === "paused" ||
    value === "decommissioned" ||
    value === "archived"
  ) {
    return value;
  }
  throw new Error(
    "lifecycleStatus must be active, paused, decommissioned, or archived",
  );
}

function derivedRuntimeRefStatusForLifecycle(
  status: NativeProfileRegistryLifecycleStatus,
): string {
  if (status === "active") return "active";
  if (status === "paused") return "paused";
  return "disabled";
}

async function handleModelProviderAdminRequest(
  request: {
    method: string;
    url: string;
    body?: unknown;
    requestId: string;
  },
  state: ServiceState,
): Promise<AdminRouteResult> {
  const url = new URL(request.url);
  const method = request.method.toUpperCase();
  const relativePath = url.pathname.replace(
    /^\/v1\/admin\/model-providers\/?/,
    "",
  );
  const segments = relativePath
    .split("/")
    .filter((segment) => segment.length > 0)
    .map((segment) => decodeURIComponent(segment));
  const alias = segments[0] ?? "";

  if (alias && segments[1] === "oauth" && segments[2] === "openai") {
    return handleOpenAiOauthProviderAdminRequest(
      {
        method,
        alias,
        action: segments[3] ?? "status",
        body: request.body,
        requestId: request.requestId,
      },
      state,
    );
  }

  if (method === "GET" && !alias) {
    const status = modelProviderStatusParam(url.searchParams.get("status"));
    if (status === "invalid") {
      return failure(400, request.requestId, {
        code: "invalid_input",
        reason_code: "invalid_model_provider_status",
        message: "invalid model provider status filter",
        retryable: false,
      });
    }
    const query: NativeModelProviderQuery = {
      status,
      aliasPrefix: stringParam(url, "aliasPrefix"),
      limit: numberParam(url, "limit"),
      offset: numberParam(url, "offset"),
    };
    const items = await state.bridge.listModelProviders(query);
    return successRoute(request.requestId, {
      items: items.map(modelProviderApiRecord),
      total: items.length,
      limit: query.limit ?? 100,
      offset: query.offset ?? 0,
    });
  }

  if (method === "GET" && alias) {
    const provider = await state.bridge.getModelProvider(alias);
    if (!provider) {
      return failure(404, request.requestId, {
        code: "not_found",
        reason_code: "model_provider_not_found",
        message: `model provider ${alias} was not found`,
        retryable: false,
      });
    }
    return successRoute(request.requestId, modelProviderApiRecord(provider));
  }

  if ((method === "POST" && !alias) || (method === "PATCH" && alias)) {
    const write = modelProviderWriteFromBody(
      request.body,
      alias || undefined,
      state.now(),
    );
    let provider: NativeModelProviderRecord;
    try {
      provider = await state.bridge.upsertModelProvider(write);
    } catch (error) {
      const mismatch = modelProviderRevisionMismatch(error);
      if (mismatch !== undefined && mismatch.alias === write.alias) {
        const currentProvider = await state.bridge.getModelProvider(
          write.alias,
        );
        return modelProviderRevisionConflictRoute(
          request.requestId,
          mismatch,
          currentProvider,
        );
      }
      throw error;
    }
    const refresh = await modelProviderRefreshAfterWrite({
      state,
      requestId: request.requestId,
      provider,
      refreshMode: modelProviderRefreshMode(url, request.body),
    });
    return successRoute(request.requestId, {
      provider: modelProviderApiRecord(provider),
      ...refresh,
    });
  }

  return failure(405, request.requestId, {
    code: "method_not_allowed",
    reason_code: "model_provider_method_not_allowed",
    message:
      "model provider routes support GET list/get, POST create/upsert, and PATCH update",
    retryable: false,
  });
}

async function handleOpenAiOauthProviderAdminRequest(
  request: {
    method: string;
    alias: string;
    action: string;
    body?: unknown;
    requestId: string;
  },
  state: ServiceState,
): Promise<AdminRouteResult> {
  const provider = await state.bridge.getModelProvider(request.alias);
  if (!provider) {
    return failure(404, request.requestId, {
      code: "not_found",
      reason_code: "model_provider_not_found",
      message: `model provider ${request.alias} was not found`,
      retryable: false,
    });
  }

  if (request.action === "status" && request.method === "GET") {
    return successRoute(
      request.requestId,
      openAiOauthProviderStatus(provider, state),
    );
  }

  if (request.action === "start" && request.method === "POST") {
    const body = optionalRecord(request.body) ?? {};
    const requestedRedirectUri = optionalString(
      body.redirectUri ?? body.redirect_uri,
    );
    if (
      requestedRedirectUri !== undefined &&
      requestedRedirectUri !== state.config.openAiOauth.redirectUri &&
      !state.config.openAiOauth.allowRedirectUriOverride
    ) {
      return failure(400, request.requestId, {
        code: "invalid_input",
        reason_code: "openai_oauth_unregistered_redirect_uri",
        message:
          "OpenAI OAuth redirectUri override is disabled; use the configured registered redirectUri from status/start response",
        retryable: false,
      });
    }
    const pending = startOpenAiOauthLogin(provider, state, body);
    state.openAiOauthPendingLogins.set(pending.pendingLoginId, pending);
    return successRoute(request.requestId, {
      provider: modelProviderApiRecord(provider),
      loginConfig: openAiOauthLoginConfig(state),
      pendingLogin: redactedOpenAiOauthPendingLogin(pending),
    });
  }

  if (request.action === "complete" && request.method === "POST") {
    const body = requiredRecord(
      request.body,
      "OpenAI OAuth complete request body",
    );
    const callbackUrl = optionalString(
      body.callbackUrl ??
        body.callback_url ??
        body.authorizationResponseUrl ??
        body.authorization_response_url,
    );
    let callback: ReturnType<typeof parseOpenAiOauthCallbackUrl> | undefined;
    try {
      callback = callbackUrl
        ? parseOpenAiOauthCallbackUrl(callbackUrl)
        : undefined;
    } catch {
      return failure(400, request.requestId, {
        code: "invalid_input",
        reason_code: "openai_oauth_invalid_callback_url",
        message:
          "OpenAI OAuth callbackUrl must be a full callback URL or query string containing code and state",
        retryable: false,
      });
    }
    if (callback?.error !== undefined) {
      return failure(400, request.requestId, {
        code: "invalid_input",
        reason_code: "openai_oauth_callback_error",
        message: `OpenAI OAuth callback returned error ${callback.error}`,
        retryable: false,
      });
    }
    const stateValue = requiredString(
      body.state ?? callback?.state,
      "OpenAI OAuth state",
    );
    const pendingLoginId = optionalString(
      body.pendingLoginId ?? body.pending_login_id,
    );
    const pending =
      pendingLoginId !== undefined
        ? state.openAiOauthPendingLogins.get(pendingLoginId)
        : findOpenAiOauthPendingLoginByState(provider.alias, stateValue, state);
    if (!pending || pending.providerAlias !== provider.alias) {
      return failure(404, request.requestId, {
        code: "not_found",
        reason_code: "openai_oauth_pending_login_not_found",
        message: "OpenAI OAuth pending login was not found",
        retryable: false,
      });
    }
    if (stateValue !== pending.state) {
      return failure(400, request.requestId, {
        code: "invalid_input",
        reason_code: "openai_oauth_state_mismatch",
        message: "OpenAI OAuth callback state did not match the pending login",
        retryable: false,
      });
    }
    const fakeTokenResponse =
      body.fakeTokenResponse ?? body.fake_token_response;
    let completionMode: "real" | "test";
    let credentialSecret: Record<string, unknown> | string;
    let oauthSummary: unknown;
    if (fakeTokenResponse !== undefined) {
      if (
        optionalBoolean(body.testMode ?? body.test_mode) !== true &&
        optionalBoolean(body.allowFakeTokenResponse) !== true
      ) {
        return failure(400, request.requestId, {
          code: "invalid_input",
          reason_code: "openai_oauth_test_mode_required",
          message:
            "OpenAI OAuth fakeTokenResponse completion requires explicit testMode=true",
          retryable: false,
        });
      }
      completionMode = "test";
      credentialSecret = openAiOauthCredentialSecretFromFakeCompletion(
        pending,
        body,
        state.now(),
      );
    } else {
      completionMode = "real";
      const code = requiredString(
        body.code ?? callback?.code,
        "OpenAI OAuth code",
      );
      const exchange = await state.bridge.exchangeOpenAiOauthCode({
        issuer: pending.issuer,
        clientId: pending.clientId,
        redirectUri: pending.redirectUri,
        code,
        codeVerifier: pending.codeVerifier,
        now: state.now(),
      });
      if (!exchange.ok) {
        return failure(
          exchange.error.retryable ? 502 : 400,
          request.requestId,
          {
            code: exchange.error.retryable ? "internal_error" : "invalid_input",
            reason_code: exchange.error.reasonCode,
            message: exchange.error.message,
            retryable: exchange.error.retryable,
          },
        );
      }
      credentialSecret = exchange.secret;
      oauthSummary = exchange.summary;
    }
    const updated = await upsertModelProviderCredentialSecret({
      state,
      provider,
      credentialSecret,
      expectedRevision: optionalNumber(
        body.expectedRevision ?? body.expected_revision,
      ),
      now: state.now(),
    });
    state.openAiOauthPendingLogins.delete(pending.pendingLoginId);
    return successRoute(request.requestId, {
      provider: modelProviderApiRecord(updated),
      credential: updated.credential,
      completionMode,
      oauthSummary,
      pendingLoginId: pending.pendingLoginId,
    });
  }

  if (request.action === "clear" && request.method === "POST") {
    const body = optionalRecord(request.body) ?? {};
    const updated = await clearModelProviderCredential({
      state,
      provider,
      expectedRevision: optionalNumber(
        body.expectedRevision ?? body.expected_revision,
      ),
      now: state.now(),
    });
    for (const [pendingLoginId, pending] of state.openAiOauthPendingLogins) {
      if (pending.providerAlias === provider.alias) {
        state.openAiOauthPendingLogins.delete(pendingLoginId);
      }
    }
    return successRoute(request.requestId, {
      provider: modelProviderApiRecord(updated),
      credential: updated.credential,
    });
  }

  return failure(405, request.requestId, {
    code: "method_not_allowed",
    reason_code: "openai_oauth_provider_method_not_allowed",
    message:
      "OpenAI OAuth provider routes support GET status and POST start/complete/clear",
    retryable: false,
  });
}

function openAiOauthProviderStatus(
  provider: NativeModelProviderRecord,
  state: ServiceState,
): {
  provider: NativeModelProviderRecord & { temperature?: number };
  credential: NativeModelProviderRecord["credential"];
  pendingLogins: Array<ReturnType<typeof redactedOpenAiOauthPendingLogin>>;
  loginConfig: ReturnType<typeof openAiOauthLoginConfig>;
} {
  return {
    provider: modelProviderApiRecord(provider),
    credential: provider.credential,
    loginConfig: openAiOauthLoginConfig(state),
    pendingLogins: [...state.openAiOauthPendingLogins.values()]
      .filter((pending) => pending.providerAlias === provider.alias)
      .map(redactedOpenAiOauthPendingLogin),
  };
}

function startOpenAiOauthLogin(
  provider: NativeModelProviderRecord,
  state: ServiceState,
  body: Record<string, unknown>,
): OpenAiOauthPendingLogin {
  const issuer = optionalString(body.issuer) ?? state.config.openAiOauth.issuer;
  const clientId =
    optionalString(body.clientId ?? body.client_id) ??
    state.config.openAiOauth.clientId;
  const redirectUri =
    optionalString(body.redirectUri ?? body.redirect_uri) ??
    state.config.openAiOauth.redirectUri;
  const scopes = optionalStringArray(
    body.scopes,
    [
      "openid",
      "profile",
      "email",
      "offline_access",
      "api.connectors.read",
      "api.connectors.invoke",
    ],
    "OpenAI OAuth scopes",
  );
  const stateValue = randomBase64Url(32);
  const codeVerifier = randomBase64Url(64);
  const codeChallenge = base64Url(
    createHash("sha256").update(codeVerifier).digest(),
  );
  const createdAt = state.now();
  const expiresAt = addMilliseconds(createdAt, 10 * 60 * 1000);
  const pendingLoginId = `openai-oauth:${provider.alias}:${randomBase64Url(18)}`;
  const authorizationUrl = openAiOauthAuthorizationUrl({
    issuer,
    clientId,
    redirectUri,
    scopes,
    state: stateValue,
    codeChallenge,
    allowedWorkspaceIds: optionalStringArray(
      body.allowedWorkspaceIds ?? body.allowed_workspace_ids,
      [],
      "OpenAI OAuth allowedWorkspaceIds",
    ),
    originator:
      optionalString(body.originator) ?? state.config.openAiOauth.originator,
  });
  return {
    pendingLoginId,
    providerAlias: provider.alias,
    issuer,
    clientId,
    redirectUri,
    scopes,
    state: stateValue,
    codeVerifier,
    codeChallenge,
    authorizationUrl,
    createdAt,
    expiresAt,
  };
}

function openAiOauthLoginConfig(state: ServiceState): {
  issuer: string;
  clientId: string;
  redirectUri: string;
  redirectUriOverrideAllowed: boolean;
  redirectUriMode: "fixed_registered" | "operator_configured";
  callbackUrlCompletionAccepted: boolean;
  callbackUrlCompletionField: "callbackUrl";
  pendingLoginIdRequiredForCallbackUrl: boolean;
  remoteOperatorFlow: "paste_callback_url";
} {
  return {
    issuer: state.config.openAiOauth.issuer,
    clientId: state.config.openAiOauth.clientId,
    redirectUri: state.config.openAiOauth.redirectUri,
    redirectUriOverrideAllowed:
      state.config.openAiOauth.allowRedirectUriOverride,
    redirectUriMode: state.config.openAiOauth.allowRedirectUriOverride
      ? "operator_configured"
      : "fixed_registered",
    callbackUrlCompletionAccepted: true,
    callbackUrlCompletionField: "callbackUrl",
    pendingLoginIdRequiredForCallbackUrl: false,
    remoteOperatorFlow: "paste_callback_url",
  };
}

function findOpenAiOauthPendingLoginByState(
  providerAlias: string,
  stateValue: string,
  serviceState: ServiceState,
): OpenAiOauthPendingLogin | undefined {
  for (const pending of serviceState.openAiOauthPendingLogins.values()) {
    if (
      pending.providerAlias === providerAlias &&
      pending.state === stateValue
    ) {
      return pending;
    }
  }
  return undefined;
}

function parseOpenAiOauthCallbackUrl(value: string): {
  code?: string;
  state?: string;
  error?: string;
  errorDescription?: string;
} {
  const trimmed = value.trim();
  const url = new URL(
    trimmed.startsWith("?")
      ? `http://localhost:1455/auth/callback${trimmed}`
      : trimmed,
  );
  return {
    code: optionalString(url.searchParams.get("code")),
    state: optionalString(url.searchParams.get("state")),
    error: optionalString(url.searchParams.get("error")),
    errorDescription: optionalString(url.searchParams.get("error_description")),
  };
}

function redactedOpenAiOauthPendingLogin(pending: OpenAiOauthPendingLogin): {
  pendingLoginId: string;
  providerAlias: string;
  issuer: string;
  clientId: string;
  redirectUri: string;
  scopes: string[];
  codeChallenge: string;
  authorizationUrl: string;
  createdAt: string;
  expiresAt: string;
} {
  return {
    pendingLoginId: pending.pendingLoginId,
    providerAlias: pending.providerAlias,
    issuer: pending.issuer,
    clientId: pending.clientId,
    redirectUri: pending.redirectUri,
    scopes: pending.scopes,
    codeChallenge: pending.codeChallenge,
    authorizationUrl: pending.authorizationUrl,
    createdAt: pending.createdAt,
    expiresAt: pending.expiresAt,
  };
}

function openAiOauthAuthorizationUrl(input: {
  issuer: string;
  clientId: string;
  redirectUri: string;
  scopes: string[];
  state: string;
  codeChallenge: string;
  allowedWorkspaceIds: string[];
  originator: string;
}): string {
  const url = new URL("/oauth/authorize", input.issuer);
  url.searchParams.set("response_type", "code");
  url.searchParams.set("client_id", input.clientId);
  url.searchParams.set("redirect_uri", input.redirectUri);
  url.searchParams.set("scope", input.scopes.join(" "));
  url.searchParams.set("code_challenge", input.codeChallenge);
  url.searchParams.set("code_challenge_method", "S256");
  url.searchParams.set("id_token_add_organizations", "true");
  url.searchParams.set("codex_cli_simplified_flow", "true");
  url.searchParams.set("state", input.state);
  url.searchParams.set("originator", input.originator);
  if (input.allowedWorkspaceIds.length > 0) {
    url.searchParams.set(
      "allowed_workspace_id",
      input.allowedWorkspaceIds.join(","),
    );
  }
  return url.toString();
}

function openAiOauthCredentialSecretFromFakeCompletion(
  pending: OpenAiOauthPendingLogin,
  body: Record<string, unknown>,
  now: string,
): Record<string, unknown> {
  const fake = requiredRecord(
    body.fakeTokenResponse ?? body.fake_token_response,
    "OpenAI OAuth fakeTokenResponse",
  );
  return {
    kind: "openai_oauth",
    version: 1,
    issuer: pending.issuer,
    client_id: pending.clientId,
    id_token: requiredString(fake.idToken ?? fake.id_token, "fake idToken"),
    access_token: requiredString(
      fake.accessToken ?? fake.access_token,
      "fake accessToken",
    ),
    refresh_token: requiredString(
      fake.refreshToken ?? fake.refresh_token,
      "fake refreshToken",
    ),
    exchanged_api_token: optionalString(
      fake.exchangedApiToken ?? fake.exchanged_api_token,
    ),
    last_refresh_at:
      optionalString(fake.lastRefreshAt ?? fake.last_refresh_at) ?? now,
    account_id: optionalString(fake.accountId ?? fake.account_id),
    email: optionalString(fake.email),
    plan_type: optionalString(fake.planType ?? fake.plan_type),
    is_fedramp_account:
      optionalBoolean(fake.isFedrampAccount ?? fake.is_fedramp_account) ??
      false,
    access_token_expires_at: optionalString(
      fake.accessTokenExpiresAt ?? fake.access_token_expires_at,
    ),
  };
}

async function upsertModelProviderCredentialSecret(input: {
  state: ServiceState;
  provider: NativeModelProviderRecord;
  credentialSecret: Record<string, unknown> | string;
  expectedRevision: number | undefined;
  now: string;
}): Promise<NativeModelProviderRecord> {
  return input.state.bridge.upsertModelProvider({
    ...modelProviderWriteFromRecord(input.provider, input.now),
    secret:
      typeof input.credentialSecret === "string"
        ? input.credentialSecret
        : JSON.stringify(input.credentialSecret),
    expectedRevision: input.expectedRevision ?? input.provider.revision,
  });
}

async function clearModelProviderCredential(input: {
  state: ServiceState;
  provider: NativeModelProviderRecord;
  expectedRevision: number | undefined;
  now: string;
}): Promise<NativeModelProviderRecord> {
  return input.state.bridge.upsertModelProvider({
    ...modelProviderWriteFromRecord(input.provider, input.now),
    clearSecret: true,
    expectedRevision: input.expectedRevision ?? input.provider.revision,
  });
}

function modelProviderWriteFromRecord(
  provider: NativeModelProviderRecord,
  now: string,
): NativeModelProviderWrite {
  return {
    alias: provider.alias,
    status: provider.status,
    protocol: provider.protocol,
    providerKind: provider.providerKind,
    displayName: provider.displayName,
    description: provider.description,
    baseUrl: provider.baseUrl,
    modelId: provider.modelId,
    contextWindowTokens: provider.contextWindowTokens,
    maxOutputTokens: provider.maxOutputTokens,
    temperatureMilli: provider.temperatureMilli,
    reasoningEffort: provider.reasoningEffort,
    reasoningFormat: provider.reasoningFormat,
    clearSecret: false,
    metadataJson: provider.metadataJson,
    now,
  };
}

type ModelProviderRefreshMode = "none" | "plan" | "apply";

interface ModelProviderRevisionMismatch {
  alias: string;
  expected: number;
  found: number;
}

function modelProviderRevisionMismatch(
  error: unknown,
): ModelProviderRevisionMismatch | undefined {
  const message = errorMessage(error, "");
  const match =
    /model provider ([^ ]+) revision mismatch: expected (\d+), found (\d+)/.exec(
      message,
    );
  if (match === null) {
    return undefined;
  }
  return {
    alias: match[1] ?? "",
    expected: Number(match[2]),
    found: Number(match[3]),
  };
}

function modelProviderRevisionConflictRoute(
  requestIdValue: string,
  mismatch: ModelProviderRevisionMismatch,
  currentProvider: NativeModelProviderRecord | undefined,
): AdminRouteResult {
  return {
    status: 409,
    headers: { "content-type": "application/json" },
    body: {
      ok: false,
      error: {
        code: "conflict",
        reason_code: "model_provider_revision_mismatch",
        message: `model provider ${mismatch.alias} revision mismatch: expected ${mismatch.expected}, found ${mismatch.found}`,
        retryable: false,
      },
      data: {
        provider:
          currentProvider === undefined
            ? undefined
            : modelProviderApiRecord(currentProvider),
        expectedRevision: mismatch.expected,
        currentRevision: mismatch.found,
      },
      meta: { request_id: requestIdValue, schema_version: 1 },
    } as AdminRouteResult["body"],
  };
}

function modelProviderApiRecord(
  provider: NativeModelProviderRecord,
): NativeModelProviderRecord & { temperature?: number } {
  if (provider.temperatureMilli === undefined) {
    return provider;
  }
  return {
    ...provider,
    temperature: provider.temperatureMilli / 1_000,
  };
}

interface ModelProviderWriteRefreshResult {
  refresh: {
    mode: ModelProviderRefreshMode;
    affectedProfiles: Array<{
      profileId: string;
      sessionIds: string[];
      configuredSessionIds: string[];
      activeSessionIds: string[];
    }>;
    outcomes: Array<{
      profileId: string;
      status: "planned" | "applied" | "blocked" | "failed";
      summary: string;
      reasonCode?: string;
      result?: unknown;
    }>;
  };
}

async function modelProviderRefreshAfterWrite(input: {
  state: ServiceState;
  requestId: string;
  provider: NativeModelProviderRecord;
  refreshMode: ModelProviderRefreshMode;
}): Promise<ModelProviderWriteRefreshResult> {
  const affectedProfiles = await modelProviderAffectedProfiles(
    input.state,
    input.provider.alias,
  );
  const outcomes: ModelProviderWriteRefreshResult["refresh"]["outcomes"] = [];
  if (input.refreshMode !== "none") {
    for (const affected of affectedProfiles) {
      const command: AdminControlCommand = {
        name:
          input.refreshMode === "apply"
            ? "apply_runtime_rebuild"
            : "plan_runtime_rebuild",
        target: { scope: "profile", profileId: affected.profileId },
        actor: { operatorId: "model-provider-admin" },
        requestId: input.requestId,
        reason: `model provider ${input.provider.alias} updated`,
        body: {},
      };
      try {
        const outcome =
          input.refreshMode === "apply"
            ? await applyServiceRuntimeRebuild(input.state, command)
            : await planServiceRuntimeRebuild(input.state, command);
        const applyOutcome =
          input.refreshMode === "apply"
            ? (outcome as ServiceRuntimeRebuildApplyResult)
            : undefined;
        const applyStatus = applyOutcome?.apply.status;
        outcomes.push({
          profileId: affected.profileId,
          status:
            input.refreshMode === "plan"
              ? "planned"
              : applyStatus === "completed"
                ? "applied"
                : "blocked",
          summary:
            input.refreshMode === "plan"
              ? `runtime rebuild plan prepared for profile ${affected.profileId}`
              : applyStatus === "completed"
                ? `runtime rebuild applied for profile ${affected.profileId}`
                : `runtime rebuild blocked for profile ${affected.profileId}`,
          reasonCode:
            applyOutcome?.apply.status === "blocked"
              ? applyOutcome.apply.reasonCode
              : undefined,
          result: outcome,
        });
      } catch (error) {
        outcomes.push({
          profileId: affected.profileId,
          status: "failed",
          summary: errorMessage(
            error,
            `runtime rebuild failed for profile ${affected.profileId}`,
          ),
          reasonCode: "model_provider_refresh_failed",
        });
      }
    }
  }

  return {
    refresh: {
      mode: input.refreshMode,
      affectedProfiles,
      outcomes,
    },
  };
}

async function modelProviderAffectedProfiles(
  state: ServiceState,
  alias: string,
): Promise<ModelProviderWriteRefreshResult["refresh"]["affectedProfiles"]> {
  const profileIds = new Set<ProfileId>();
  for (const brain of state.runtimeConfig.brains)
    profileIds.add(brain.profileId);
  for (const session of state.runtimeConfig.sessions)
    profileIds.add(session.profileId);
  const activeSessions = await state.bridge.listSessions().catch(() => []);
  for (const session of activeSessions) profileIds.add(session.profileId);

  const affected: ModelProviderWriteRefreshResult["refresh"]["affectedProfiles"] =
    [];
  for (const profileId of [...profileIds].sort()) {
    const profile = await loadProfileConfig(
      state.runtimeConfig.profilesDir,
      profileId,
    ).catch(() => undefined);
    if (profile?.providerAlias !== alias) continue;
    const configuredSessionIds = state.runtimeConfig.sessions
      .filter((session) => session.profileId === profileId)
      .map((session) => String(session.sessionId));
    const activeSessionIds = activeSessions
      .filter((session) => session.profileId === profileId)
      .map((session) => String(session.sessionId));
    affected.push({
      profileId: String(profileId),
      sessionIds: [...new Set([...configuredSessionIds, ...activeSessionIds])],
      configuredSessionIds,
      activeSessionIds,
    });
  }
  return affected;
}

function modelProviderRefreshMode(
  url: URL,
  body: unknown,
): ModelProviderRefreshMode {
  const raw =
    url.searchParams.get("refresh") ??
    (isRecord(body) ? optionalString(body.refresh) : undefined) ??
    "none";
  if (raw === "none" || raw === "plan" || raw === "apply") return raw;
  throw new Error("model provider refresh must be none, plan, or apply");
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
    parts[4] === "context-compaction-events"
  ) {
    if ((request.method ?? "GET").toUpperCase() !== "POST") {
      return failure(405, requestId(request), {
        code: "method_not_allowed",
        reason_code: "debug_context_compaction_events_requires_post",
        message: "context compaction debug event route only supports POST",
        retryable: false,
      });
    }
    const requestIdValue = requestId(request);
    const sessionId = decodeURIComponent(parts[3] ?? "") as SessionId;
    const sessions = await state.bridge.listSessions();
    const session = sessions.find(
      (candidate) => candidate.sessionId === sessionId,
    );
    if (!session) {
      return failure(404, requestIdValue, {
        code: "not_found",
        reason_code: "debug_context_compaction_session_not_found",
        message: `debug session ${sessionId} was not found`,
        retryable: false,
      });
    }
    const body = recordBody(await readJsonBody(request));
    const result = emitContextCompactionDebugEvents(state, session, {
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
    parts.length === 6 &&
    parts[0] === "v1" &&
    parts[1] === "debug" &&
    parts[2] === "sessions" &&
    parts[4] === "provider-requests"
  ) {
    if ((request.method ?? "GET").toUpperCase() !== "GET") {
      return failure(405, requestId(request), {
        code: "method_not_allowed",
        reason_code: "debug_provider_request_requires_get",
        message: "direct provider request debug route only supports GET",
        retryable: false,
      });
    }
    const requestIdValue = requestId(request);
    const sessionId = decodeURIComponent(parts[3] ?? "") as SessionId;
    const debugDetailId = decodeURIComponent(parts[5] ?? "");
    const sessions = await state.bridge.listSessions();
    const session = sessions.find(
      (candidate) => candidate.sessionId === sessionId,
    );
    if (!session) {
      return failure(404, requestIdValue, {
        code: "not_found",
        reason_code: "debug_provider_request_session_not_found",
        message: `debug session ${sessionId} was not found`,
        retryable: false,
      });
    }
    const detail = await rustyViewProviderRequestDebugDetail(state, {
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
  response.write(": connected\n\n");
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
    responsesWakeMetrics: state.responsesWakeMetrics,
    adapters: buildServiceAdapterDiagnostics(state, now),
    tools: buildSelectedToolDiagnostics(state, sessions),
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

function buildSelectedToolDiagnostics(
  state: ServiceState,
  sessions: readonly SessionState[],
): ToolDiagnosticsProjection[] {
  return sessions.flatMap((session) => {
    const activeMcpBinding = state.runtimeConfig.mcpBindings.find(
      (binding) =>
        (binding.status === undefined || binding.status === "active") &&
        (binding.sessionId === session.sessionId ||
          binding.profileId === session.profileId),
    );
    return session.toolProfile.tools.map((tool) => {
      const localEntry = defaultToolRegistry.resolve(tool.name);
      const source = localEntry ? "local" : "mcp";
      const catalogId =
        source === "mcp" && activeMcpBinding
          ? `mcp:${activeMcpBinding.toolProfileKey}`
          : `session:${session.sessionId}`;
      return {
        catalogId,
        sessionId: session.sessionId,
        agentId: session.agentId,
        profileId: session.profileId,
        toolName: tool.name,
        description: tool.description,
        source,
        adapterId: source === "mcp" ? activeMcpBinding?.adapterId : undefined,
        bindingId: source === "mcp" ? activeMcpBinding?.bindingId : undefined,
        serverNames:
          source === "mcp" ? [...(activeMcpBinding?.serverNames ?? [])] : [],
        endpointRef:
          source === "mcp" ? activeMcpBinding?.endpointRef : undefined,
        toolProfileKey:
          source === "mcp" ? activeMcpBinding?.toolProfileKey : undefined,
        sourceToolName: tool.name,
        catalogRevision: source === "mcp" ? catalogId : "default-local-tools",
        schemaStatus: tool.inputSchema ? "present" : "missing",
        category: localEntry?.category ?? source,
        toolsets: localEntry ? [...localEntry.toolsets] : [],
        safety: localEntry ? [...localEntry.safety] : [],
        outputShape: localEntry?.outputShape,
        registeredTools: 1,
        selectedTools: 1,
        validationErrors: 0,
        validationWarnings: 0,
        invalid: false,
      } satisfies ToolDiagnosticsProjection;
    });
  });
}

function isProfileRegistryAdminRoute(pathname: string): boolean {
  return (
    pathname === "/v1/admin/diagnostics/profiles" ||
    pathname === "/v1/admin/profiles/registry" ||
    pathname.startsWith("/v1/admin/profiles/registry/")
  );
}

function isProfileRegistryWriteRoute(pathname: string): boolean {
  return (
    pathname.startsWith("/v1/admin/profiles/registry/") &&
    (pathname.endsWith("/update/plan") ||
      pathname.endsWith("/update/apply") ||
      pathname.endsWith("/lifecycle/plan") ||
      pathname.endsWith("/lifecycle/apply") ||
      pathname.endsWith("/prompt/plan") ||
      pathname.endsWith("/prompt/apply") ||
      pathname.endsWith("/runtime-config/plan") ||
      pathname.endsWith("/runtime-config/apply"))
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
    activeCoordinationBackend: storage.backend,
    selectorStatus:
      config.backend === "sqlite"
        ? "active"
        : config.postgres.bootMode === "active"
          ? "active"
          : config.postgres.bootMode === "proof_admin"
            ? "proof_admin_only"
            : "blocked",
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
      bootMode: config.postgres.bootMode,
      maxConnections: config.postgres.maxConnections,
      statementTimeoutMs: config.postgres.statementTimeoutMs,
      implementationStatus:
        config.postgres.bootMode === "active"
          ? "active"
          : config.postgres.bootMode === "proof_admin"
            ? "proof_admin_only"
            : "blocked_unimplemented",
      productionReadiness: postgresProductionReadiness(
        config.postgres.bootMode,
        storage.repositoryGroups,
      ),
      capabilities: postgresStorageCapabilities(config.postgres.bootMode),
      search: postgresSearchDiagnostics(config.postgres.bootMode),
      repositoryGroups: postgresRepositoryGroupDiagnostics(
        storage.repositoryGroups,
      ),
      moduleOwnedStores: postgresModuleOwnedStoreDiagnostics(),
    },
  };
}

function postgresStorageCapabilities(
  bootMode: RustyCrewStorageConfig["postgres"]["bootMode"],
): NonNullable<StorageDiagnosticsProjection["postgres"]>["capabilities"] {
  const postgresConnected = bootMode === "proof_admin" || bootMode === "active";
  return [
    {
      name: "transactions",
      supported: postgresConnected,
      detail: postgresConnected
        ? "PostgreSQL transactions are available for covered repository groups."
        : "PostgreSQL service boot is blocked until active mode is selected.",
    },
    {
      name: "json_metadata",
      supported: postgresConnected,
      detail:
        "PostgreSQL JSON metadata is available for covered repository groups.",
    },
    {
      name: "concurrent_writers",
      supported: postgresConnected,
      detail:
        "PostgreSQL supports concurrent writers; covered repository groups use the Postgres backend facade.",
    },
    {
      name: "row_level_claims",
      supported: postgresConnected,
      detail: postgresConnected
        ? "Scheduler stale-run expiry uses PostgreSQL row-level claim semantics in the proof slice."
        : "Row-level claim support requires active PostgreSQL service mode.",
    },
    {
      name: "runtime_full_text_search",
      supported: postgresConnected,
      detail:
        "PostgreSQL runtime search is available through typed runtime-search service APIs.",
    },
    {
      name: "logical_export_import",
      supported: false,
      detail:
        "Logical cross-backend export/import remains future work; raw migration is not the green path.",
    },
  ];
}

function postgresRepositoryGroupDiagnostics(
  groups: StorageDiagnosticsProjection["repositoryGroups"],
): NonNullable<StorageDiagnosticsProjection["postgres"]>["repositoryGroups"] {
  const implementedDetails = {
    storage_admin: {
      status: "active_storage_admin",
      detail:
        "Implemented for active backend selector projection, env-var references, and storage-admin diagnostics.",
    },
    sessions_identities: {
      status: "active_sessions_identities",
      detail:
        "Implemented by the active Rust PostgreSQL backend for session/config/identity hydration.",
    },
    events_projections: {
      status: "active_events_projections",
      detail:
        "Implemented by the active Rust PostgreSQL backend for event history, event indexing, completion packets, and tool telemetry.",
    },
    queues_messages: {
      status: "active_queues_messages",
      detail:
        "Implemented by the active Rust PostgreSQL backend for queued-message TTL, no-resurrection behavior, and maintenance purging.",
    },
    scheduler_jobs: {
      status: "active_scheduler_jobs",
      detail:
        "Implemented by the active Rust PostgreSQL backend for scheduled jobs, pause/resume, scheduled run claim/completion, and stale-run row-level expiry.",
    },
    worker_runs_completions: {
      status: "active_worker_runs_completions",
      detail:
        "Implemented by the active Rust PostgreSQL backend for worker lifecycle, terminal-status queries, completion packet persistence, and delegated completion lookup.",
    },
    runtime_counters: {
      status: "active_runtime_counter",
      detail:
        "Implemented by the active Rust PostgreSQL backend for typed runtime counter APIs.",
    },
    runtime_search: {
      status: "active_runtime_search",
      detail:
        "Implemented by the active Rust PostgreSQL backend for typed runtime-search APIs.",
    },
    provider_state: {
      status: "active_provider_state",
      detail:
        "Implemented by the active Rust PostgreSQL backend for typed provider wire-state APIs.",
    },
    conversations_attachments: {
      status: "active_conversations_attachments",
      detail:
        "Implemented by the active Rust PostgreSQL backend for conversation transcripts, variants, branches, attachments, and data-bank scopes.",
    },
    profile_memory: {
      status: "active_profile_memory",
      detail:
        "Implemented by the active Rust PostgreSQL backend for profile registry, profile_dense memory, session memory query/prompt context, and memory proposal governance.",
    },
    bindings: {
      status: "active_bindings",
      detail:
        "Implemented by the active Rust PostgreSQL backend for profile/session-scoped channel and MCP binding records without adapter secret material.",
    },
    profile_registry: {
      status: "active_profile_registry",
      detail:
        "Implemented by the active Rust PostgreSQL backend for official create-profile registry records, lifecycle status, runtime refs, and import/export metadata.",
    },
    module_schema_registry: {
      status: "active_module_schema_registry",
      detail:
        "Implemented by the active Rust PostgreSQL backend with compiled module registry diagnostics, supported capability projection, simple_kv, session_memory, and roleplay_lore stores.",
    },
    import_export: {
      status: "active_import_export_fresh_deployment",
      detail:
        "Accepted for active fresh PostgreSQL deployment: raw SQLite-to-Postgres migration is intentionally not the green path, and logical transfer remains disabled until its own implementation task.",
    },
  } as const;
  return groups.map((group) => {
    const implemented =
      group.groupId in implementedDetails
        ? implementedDetails[group.groupId as keyof typeof implementedDetails]
        : undefined;
    if (implemented) {
      return {
        groupId: group.groupId,
        label: group.label,
        correctnessSensitive: group.correctnessSensitive,
        coverageStatus: "implemented",
        implementationStatus: implemented.status,
        detail: implemented.detail,
      };
    }
    return {
      groupId: group.groupId,
      label: group.label,
      correctnessSensitive: group.correctnessSensitive,
      coverageStatus: "unsupported",
      implementationStatus: "unsupported",
      detail:
        "Unsupported for PostgreSQL service boot; using this group must fail closed until repository coverage exists.",
    };
  });
}

function postgresSearchDiagnostics(
  bootMode: RustyCrewStorageConfig["postgres"]["bootMode"],
): NonNullable<StorageDiagnosticsProjection["postgres"]>["search"] {
  if (bootMode === "proof_admin") {
    return {
      backend: "postgres_tsvector",
      status: "proof",
      degraded: false,
      detail:
        "Runtime search has a PostgreSQL proof slice behind typed APIs; backend tsquery syntax is not exposed through admin or tool routes.",
    };
  }
  if (bootMode === "active") {
    return {
      backend: "postgres_tsvector",
      status: "implemented",
      degraded: false,
      detail:
        "Runtime search is wired through the PostgreSQL backend facade for typed service queries.",
    };
  }
  return {
    backend: "postgres_tsvector",
    status: "unsupported",
    degraded: true,
    detail:
      "PostgreSQL runtime search is unavailable for full service boot until the proof repository is wired behind the service backend.",
  };
}

function postgresProductionReadiness(
  bootMode: RustyCrewStorageConfig["postgres"]["bootMode"],
  groups: StorageDiagnosticsProjection["repositoryGroups"],
): NonNullable<
  StorageDiagnosticsProjection["postgres"]
>["productionReadiness"] {
  const projectedGroups = postgresRepositoryGroupDiagnostics(groups);
  const blockers = projectedGroups
    .filter(
      (group) =>
        group.correctnessSensitive && group.coverageStatus !== "implemented",
    )
    .map((group) => ({
      groupId: group.groupId,
      status: group.coverageStatus,
      detail: group.detail,
    }));
  for (const store of postgresModuleOwnedStoreDiagnostics()) {
    if (store.coverageStatus !== "implemented") {
      blockers.push({
        groupId: store.storeId,
        status: store.coverageStatus,
        detail: store.detail,
      });
    }
  }
  const ready = bootMode === "active" && blockers.length === 0;
  const reasonCodes = ready
    ? ["postgres_active_ready"]
    : [
        bootMode === "active"
          ? "postgres_active_with_repository_gaps"
          : bootMode === "proof_admin"
            ? "postgres_proof_admin_only"
            : "postgres_full_service_boot_blocked",
        ...blockers.map(
          (blocker) =>
            `postgres_repository_${blocker.status}:${blocker.groupId}`,
        ),
      ];
  return {
    ready,
    status: ready
      ? "ready"
      : bootMode === "active"
        ? "degraded"
        : bootMode === "proof_admin"
          ? "proof_admin_only"
          : "blocked_unimplemented",
    reasonCodes,
    blockers,
    detail: ready
      ? "PostgreSQL is the active coordination backend and every correctness-sensitive repository group is implemented for this deployment mode."
      : bootMode === "active"
        ? "PostgreSQL is the active coordination backend, but readiness remains fail-closed until every correctness-sensitive repository group is implemented."
        : bootMode === "proof_admin"
          ? "PostgreSQL is available only for bounded proof/admin diagnostics; full service startup requires active mode."
          : "PostgreSQL full service boot is blocked until active mode is selected and required repository groups are implemented or explicitly unsupported for a selected deployment mode.",
  };
}

function postgresModuleOwnedStoreDiagnostics(): NonNullable<
  StorageDiagnosticsProjection["postgres"]
>["moduleOwnedStores"] {
  return [
    {
      storeId: "typed_memory_spaces",
      label: "Typed Memory Spaces",
      coverageStatus: "implemented",
      detail:
        "Implemented for Rust-owned profile_dense and session_memory typed memory spaces on the active PostgreSQL backend.",
    },
    {
      storeId: "roleplay_lore",
      label: "Roleplay Lore",
      coverageStatus: "implemented",
      detail:
        "Implemented by the active PostgreSQL backend with typed world/entity/lore/timeline/provenance records.",
    },
  ];
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

function recordResponsesWakeMetrics(
  state: ServiceState,
  observation: ServiceBrainWakeResultObservation,
): void {
  const metrics = observation.result.transportMetrics;
  if (metrics === undefined) return;
  const brainEventCounts =
    observation.result.brainEventCounts ?? countBrainEvents(observation.result);
  const brainStreamItemCounts =
    observation.result.brainStreamItemCounts ??
    countBrainStreamItems(observation.result);
  state.responsesWakeMetrics.unshift({
    profileId: observation.profileId,
    sessionId: observation.sessionId,
    wakeId: observation.wakeId,
    observedAt: state.now(),
    effectiveTransport: metrics.effectiveTransport,
    selectedStrategyId: metrics.selectedStrategyId,
    effectiveStrategyId: metrics.effectiveStrategyId,
    fallbackReason: metrics.fallbackReason,
    providerRequestCount: metrics.providerRequestCount,
    continuationRoundCount: metrics.continuationRoundCount,
    providerRequestPayloadBytes: metrics.providerRequestPayloadBytes,
    providerEventCounts: metrics.providerEventCounts,
    brainEventCounts,
    brainStreamItemCounts,
    firstTextDeltaLatencyMs: metrics.firstTextDeltaLatencyMs,
    totalTurnDurationMs: metrics.totalTurnDurationMs,
  });
  state.responsesWakeMetrics.splice(50);
}

function countBrainEvents(
  result: ServiceBrainWakeResultObservation["result"],
): Record<string, number> {
  const counts: Record<string, number> = {};
  for (const event of result.events) {
    incrementRecordCount(counts, event.event.type);
  }
  for (const item of result.stream ?? []) {
    if (item.type === "event") {
      incrementRecordCount(counts, item.event.event.type);
    }
  }
  return counts;
}

function countBrainStreamItems(
  result: ServiceBrainWakeResultObservation["result"],
): Record<string, number> {
  const counts: Record<string, number> = {};
  for (const item of result.stream ?? []) {
    incrementRecordCount(counts, item.type);
  }
  if (result.events.length > 0) incrementRecordCount(counts, "event");
  if (result.actions.length > 0) incrementRecordCount(counts, "actions");
  return counts;
}

function incrementRecordCount(
  counts: Record<string, number>,
  key: string,
): void {
  counts[key] = (counts[key] ?? 0) + 1;
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

async function prepareContextStrategyForWake(
  state: ServiceState,
  input: {
    session: Pick<SessionState, "sessionId" | "profileId">;
    configuredSession?: Pick<
      RustyCrewConfiguredSession,
      "sessionMemoryPrompt" | "contextPolicy"
    >;
    profileContext: Awaited<ReturnType<typeof loadProfileContext>>;
  },
): Promise<{
  policy: ContextStrategyPolicy;
  additionalInstructions: string[];
  sessionMemoryContext?: string;
}> {
  const policy =
    input.configuredSession?.contextPolicy ??
    input.profileContext.profile.contextPolicy ??
    defaultContextStrategyPolicy();
  const descriptor = contextStrategyDescriptor(policy.strategyId);
  if (!policy.enabled || descriptor === undefined) {
    return { policy, additionalInstructions: [] };
  }
  const rolePreparation = prepareContextStrategyRoleAssembly(policy);
  // First implementation keeps current wake behavior behind the strategy seam.
  // Future strategies can replace this helper without changing dispatch.
  const sessionMemoryContext = await buildSessionMemoryContextForWake(state, {
    session: input.session,
    configuredSession: input.configuredSession,
    profileContext: input.profileContext,
  });
  return {
    policy,
    additionalInstructions: rolePreparation.additionalInstructions,
    sessionMemoryContext,
  };
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
      heartbeatEnabled: state.schedulerHeartbeat.enabled,
      heartbeatIntervalMs: state.schedulerHeartbeat.intervalMs,
      heartbeatRunning: state.schedulerHeartbeat.running,
      lastHeartbeatStartedAt: state.schedulerHeartbeat.lastStartedAt,
      lastHeartbeatCompletedAt: state.schedulerHeartbeat.lastCompletedAt,
      lastHeartbeatDurationMs: state.schedulerHeartbeat.lastDurationMs,
      lastHeartbeatSummary: state.schedulerHeartbeat.lastSummary,
      lastHeartbeatSkippedAt: state.schedulerHeartbeat.lastSkippedAt,
      lastHeartbeatSkipReason: state.schedulerHeartbeat.lastSkipReason,
      jobCount: jobs.length,
      activeJobs: activeJobs.length,
      pausedJobs: pausedJobs.length,
      staleRuns: 0,
      runningRuns: runningRuns.length,
      failedRuns: failedRuns.length,
      nextDueAt: earliestDueAt(activeJobs),
      lastRunAt: lastRun?.completedAt,
      lastError: state.schedulerHeartbeat.lastError ?? failedRuns[0]?.error,
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
      lastCaptureProposalCount: state.backgroundReview.lastCaptureProposalCount,
      lastPersistedCaptureProposalCount:
        state.backgroundReview.lastPersistedCaptureProposalCount,
      lastSkippedReasons: state.backgroundReview.lastSkippedReasons,
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
  adapterFactories: Pick<
    ServiceAdapterFactories,
    "createMcpSurfaceManager" | "createSimulatedMcpTransportFactory"
  >,
): Promise<McpSurfaceManagerPort> {
  const manager = adapterFactories.createMcpSurfaceManager({
    transports: [
      adapterFactories.createSimulatedMcpTransportFactory("stdio"),
      adapterFactories.createSimulatedMcpTransportFactory("streamable_http"),
      adapterFactories.createSimulatedMcpTransportFactory("websocket"),
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
    modelProviderResolver: (alias) =>
      resolveModelProviderForBrain(input.bridge, alias),
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

async function startDenObservationProjection(
  state: ServiceState,
): Promise<void> {
  if (state.denGatewayClient === undefined) return;
  const subscription = await state.bridge.subscribeEvents({
    eventKinds: [
      "session_created",
      "session_archived",
      "agent_message_routed",
      "delegation_lifecycle_observed",
      "brain_wake_requested",
      "brain_actions_accepted",
      "completion_packet_delivered",
    ],
  });
  state.denObservationSubscription = subscription;
  const timer = setInterval(() => {
    void drainDenObservationProjection(state).catch((error) =>
      recordServiceEvent(state, {
        source: "den-successor-gateway",
        eventType: "den_observation_projection_degraded",
        severity: "warning",
        summary: errorMessage(error, "Den Observation projection failed"),
      }),
    );
  }, 1_000);
  state.timers.add(timer);
  recordServiceEvent(state, {
    source: "den-successor-gateway",
    eventType: "den_observation_projection_started",
    summary:
      "Den Observation projection subscribed to Rusty Crew runtime events.",
  });
}

async function drainDenObservationProjection(
  state: ServiceState,
): Promise<void> {
  const subscription = state.denObservationSubscription;
  if (subscription === undefined || state.denGatewayClient === undefined)
    return;
  const events = await drainSubscriptionEventsUntilIdle(
    state.bridge,
    subscription,
  );
  if (events.length === 0) return;

  const sessionLookup = await runtimeObservationSessionLookup(state);
  const producer = new AgentActivityObservationProducer({
    sink: createDenGatewayObservationSink(state.denGatewayClient),
    required: true,
  });
  let projected = 0;
  let degraded = 0;
  for (const event of events) {
    const input: AgentActivityEventInput | undefined =
      runtimeCoreEventObservationInput(event, {
        lookupSession: sessionLookup,
        filters: state.runtimeConfig.denObservation?.eventFilters,
      });
    if (input === undefined) continue;
    const result = await producer.publish(input);
    if (result.status === "published") {
      projected += 1;
    } else if (result.status === "degraded") {
      degraded += 1;
    }
  }
  if (projected > 0) {
    recordServiceEvent(state, {
      source: "den-successor-gateway",
      eventType: "den_observation_projection_published",
      summary: `Published ${projected} Den Observation runtime event(s).`,
    });
  }
  if (degraded > 0) {
    recordServiceEvent(state, {
      source: "den-successor-gateway",
      eventType: "den_observation_projection_degraded",
      severity: "warning",
      summary: `Publishing ${degraded} Den Observation runtime event(s) degraded.`,
    });
  }
}

async function runtimeObservationSessionLookup(
  state: ServiceState,
): Promise<
  (
    sessionId: SessionId | string,
  ) => RuntimeObservationSessionIdentity | undefined
> {
  const sessions = await state.bridge.listSessions().catch(() => []);
  const byId = new Map<string, RuntimeObservationSessionIdentity>();
  for (const session of sessions) {
    byId.set(session.sessionId, {
      sessionId: session.sessionId,
      agentId: session.agentId,
      profileId: session.profileId,
      kind: session.kind,
    });
  }
  for (const session of state.runtimeConfig.sessions) {
    if (!byId.has(session.sessionId)) {
      byId.set(session.sessionId, {
        sessionId: session.sessionId,
        agentId: session.agentId,
        profileId: session.profileId,
        kind: session.kind,
      });
    }
  }
  return (sessionId) => byId.get(String(sessionId));
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
      state.adapterFactories.createTelegramAdapterRegistration(adapterId),
    );
  } catch (error) {
    recordServiceEvent(state, {
      source: "telegram",
      eventType: "telegram_adapter_registration_degraded",
      severity: "warning",
      summary: errorMessage(error, "Telegram adapter registration failed"),
    });
  }

  const connector = state.adapterFactories.createTelegramConnector({
    adapterId,
    botToken: token,
    apiBaseUrl: state.config.telegram.apiBaseUrl,
    offsetStorePath: join(
      state.config.paths.dataDir,
      "data",
      "telegram",
      `${state.config.telegram.adapterId}-offset.json`,
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
    onInbound: async (message) => {
      await state.adapterFactories.ingestChannelInboundMessage(message, {
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
      });
    },
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
    const projection = state.adapterFactories.projectAgentMessageToChannel(
      event.message,
      activeTelegramChannelBindings(
        state.runtimeConfig.channelBindings,
        state.config.telegram.adapterId,
      ),
      { now: state.now() },
    );
    if (projection.status === "projected") {
      const dispatch =
        await state.adapterFactories.dispatchChannelMessageProjection(
          {
            sendMessage: async (message) => {
              await connector.sendOutbound(message);
            },
            sendActivity: async () => undefined,
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
  assertServiceStorageBootAllowed(
    nextRuntimeConfig.storage ?? state.config.storage,
    "runtime config reload",
  );
  const nextProfileChannelWakePolicies =
    await loadProfileChannelWakePolicies(nextRuntimeConfig);
  const nextMcpManager = await createServiceMcpManager(
    nextRuntimeConfig,
    state.adapterFactories,
  );
  const nextApplyResult = await applyRustyCrewRuntimeConfig({
    serviceConfig: state.config,
    runtimeConfig: nextRuntimeConfig,
    bridge: state.bridge,
    existingBrainHandlesByProfileId:
      state.runtimeConfigApplyResult.brainHandlesByProfileId,
    createMissingSessions: options.createMissingSessions,
    curatorExecutor: state.curator.executor,
    mcpSurfaceDiagnostics: nextMcpManager.diagnostics(),
    adapterFactories: state.adapterFactories,
    coordinationRuntime: createServiceCoordinationRuntime(() => state),
    toolCallDebugStore: state.toolCallDebugStore,
    providerRequestDebugStore: state.providerRequestDebugStore,
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
  registryRecord?: Awaited<
    ReturnType<ServiceState["bridge"]["createProfileRegistryRecord"]>
  >;
  localToolProfileId?: string;
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

interface DeletedServiceProfile {
  profileId: string;
  runtimeConfigPath: string;
  profilePath?: string;
  profileDirectoryDeleted: boolean;
  sessionsDeleted: string[];
  removed: DecommissionedServiceProfile["removed"];
  brainHandle: DecommissionedServiceProfile["brainHandle"];
  storagePurge: NativeProfilePurgeReport;
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
  return planRuntimeConfigValue(state, runtimeConfig);
}

async function planRuntimeConfigFileValue(
  state: ServiceState,
  value: Record<string, unknown>,
): Promise<RuntimeConfigDraftPlan> {
  return planRuntimeConfigValue(
    state,
    runtimeConfigDraftFromFileValue(state, value),
  );
}

async function planRuntimeConfigValue(
  state: ServiceState,
  runtimeConfig: RustyCrewRuntimeConfig,
): Promise<RuntimeConfigDraftPlan> {
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

function assertRuntimeConfigDraftPlanOk(plan: RuntimeConfigDraftPlan): void {
  const errors = plan.diagnostics.filter(
    (diagnostic) => diagnostic.severity === "error",
  );
  if (errors.length === 0) return;
  const first = errors[0]!;
  const suffix =
    errors.length === 1
      ? ""
      : ` (${errors.length - 1} additional diagnostic${errors.length === 2 ? "" : "s"})`;
  throw new Error(
    `${first.path ? `${first.path}: ` : ""}${first.message}${suffix}`,
  );
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

async function deleteServiceProfile(
  state: ServiceState,
  command: AdminControlCommand,
): Promise<DeletedServiceProfile> {
  const profileId = command.target.profileId;
  if (!profileId) throw new Error("profile id is required");
  const confirmProfileId = requiredBodyString(command, "confirmProfileId");
  if (confirmProfileId !== profileId) {
    throw new Error(
      `profile delete confirmation mismatch: expected ${profileId}`,
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
      `profile ${profileId} delete blocked by in-flight wake(s): ${inFlightSessionIds.join(", ")}`,
    );
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
  const registryRecord = await state.bridge.getProfileRegistryRecord(profileId);
  const matchedRuntimeConfig =
    removed.brains +
      removed.sessions +
      removed.channelBindings +
      removed.mcpBindings +
      removed.scheduledJobs >
    0;
  if (
    !matchedRuntimeConfig &&
    sessionIds.length === 0 &&
    registryRecord === undefined &&
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
    eventType: "profile_deleted",
    summaryPrefix: `Profile ${profileId} deleted`,
  });
  const brainHandle = await unregisterServiceProfileBrain(state, profileId);

  let profileDirectoryDeleted = false;
  if (profilePath !== undefined && existsSync(profilePath)) {
    await rm(profilePath, { recursive: true, force: true });
    profileDirectoryDeleted = true;
  }

  const storagePurge = await state.bridge.purgeProfile(profileId);
  const purgedSessionIds = new Set([
    ...sessionIds,
    ...storagePurge.sessionIds.map(String),
  ]);
  for (const sessionId of purgedSessionIds) {
    state.directDispatchSessions.delete(sessionId as SessionId);
    state.chatSubscribersBySession.delete(sessionId as SessionId);
    state.chatEventsBySession.delete(sessionId as SessionId);
    state.chatSequencesBySession.delete(sessionId as SessionId);
    state.suppressedWakeEvents.delete(sessionId as SessionId);
  }

  return {
    profileId,
    runtimeConfigPath: state.config.paths.serviceConfigFile,
    ...(profilePath === undefined ? {} : { profilePath }),
    profileDirectoryDeleted,
    sessionsDeleted: [...purgedSessionIds].sort(),
    removed,
    brainHandle,
    storagePurge,
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
  const providerAlias =
    optionalBodyString(command, "providerAlias") ?? "default";
  const modelProvider = await state.bridge.getModelProvider(providerAlias);
  if (modelProvider === undefined) {
    throw new Error(`model provider alias ${providerAlias} was not found`);
  }
  if (modelProvider.status !== "active") {
    throw new Error(
      `model provider alias ${providerAlias} is ${modelProvider.status}; active provider required`,
    );
  }
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
      providerAlias,
      brain:
        profileBrainFromBody(
          command.body.brain ?? command.body.brainSelection,
        ) ?? defaultProfileBrainForModelProvider(modelProvider),
      mcpBindings: createProfileMcpBindingsFromBody(command.body.mcpBindings),
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
  if (!profileSeed || !runtimeBrain || !runtimeSession) {
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
  const localToolProfileId = optionalBodyString(command, "localToolProfileId");
  const localToolProfile =
    localToolProfileId === undefined
      ? undefined
      : await createLocalToolProfileStore({
          bridge: state.bridge,
          now: state.now,
        }).resolve(localToolProfileId);
  const registryRuntimeSettings =
    plan.registryWrite === undefined
      ? {}
      : (optionalRecord(plan.registryWrite.activeRuntimeSettingsJson) ?? {});
  const registryWrite =
    plan.registryWrite === undefined
      ? undefined
      : {
          ...plan.registryWrite,
          activeRuntimeSettingsJson: {
            ...registryRuntimeSettings,
            ...(localToolProfile === undefined
              ? {}
              : {
                  localToolProfileId: localToolProfile.id,
                  toolPolicy: localToolProfile.toolPolicy,
                  profile: {
                    ...(optionalRecord(registryRuntimeSettings.profile) ?? {}),
                    localToolProfileId: localToolProfile.id,
                    toolPolicy: localToolProfile.toolPolicy,
                  },
                }),
          },
        };
  const registryRecord = registryWrite
    ? await state.bridge.createProfileRegistryRecord(registryWrite)
    : undefined;

  await mkdir(state.runtimeConfig.profilesDir, { recursive: true });
  await writeJsonFileAtomic(plannedProfilePath, {
    profileId: profileSeed.profileId,
    ...(profileSeed.displayName === undefined
      ? {}
      : { displayName: profileSeed.displayName }),
    providerAlias: profileSeed.providerAlias,
    brain: profileSeed.brain,
    ...(profileMcpConfig === undefined ? {} : { mcpConfig: profileMcpConfig }),
    ...(localToolProfile === undefined
      ? {}
      : {
          localToolProfileId: localToolProfile.id,
          toolPolicy: localToolProfile.toolPolicy,
        }),
    skills: profileSeed.skillsMode,
  });

  runtimeConfigFile.array("brains").push(runtimeBrain);
  runtimeConfigFile.array("sessions").push(runtimeSession);
  runtimeConfigFile.array("mcpBindings").push(...plan.runtimeMcpBindings);
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
    registryWrite,
    registryRecord,
    localToolProfileId: localToolProfile?.id,
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
    profiles.push(await loadProfileConfigWithRegistryPrompt(state, profileId));
  }
  return profiles;
}

async function loadProfileConfigWithRegistryPrompt(
  state: ServiceState,
  profileId: ProfileId,
): Promise<ProfileConfig> {
  const profile = await loadProfileConfig(
    state.runtimeConfig.profilesDir,
    profileId,
  );
  const record = await state.bridge
    .getProfileRegistryRecord(String(profileId))
    .catch(() => undefined);
  if (record === undefined) return profile;
  return {
    ...profile,
    prompt: {
      ...(profile.prompt ?? {}),
      soulMarkdown: record.promptSoulMarkdown,
      memoryMarkdown: record.promptMemoryMarkdown,
    },
  };
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

function defaultProfileBrainForModelProvider(
  provider: NativeModelProviderRecord,
): { module?: string; strategy?: string } {
  if (provider.protocol === "responses") {
    return { module: "openai-responses" };
  }
  if (provider.providerKind === "local") {
    return { module: "local" };
  }
  return { module: "pi-agent-core" };
}

function createProfileMcpBindingsFromBody(input: unknown):
  | Array<{
      serverId: string;
      bindingId?: string;
      adapterId?: string;
      serverNames?: string[];
      transport?: string;
      toolProfileKey?: string;
    }>
  | undefined {
  if (input === undefined || input === null) return undefined;
  if (!Array.isArray(input)) {
    throw new Error("mcpBindings must be an array when provided");
  }
  return input.map((item, index) => {
    if (!isRecord(item)) {
      throw new Error(`mcpBindings[${index}] must be an object`);
    }
    const serverId = optionalString(item.serverId);
    if (serverId === undefined) {
      throw new Error(`mcpBindings[${index}].serverId is required`);
    }
    return compactRecord({
      serverId,
      bindingId: optionalString(item.bindingId),
      adapterId: optionalString(item.adapterId),
      serverNames:
        item.serverNames === undefined
          ? undefined
          : stringArray(item.serverNames, `mcpBindings[${index}].serverNames`),
      transport: optionalString(item.transport),
      toolProfileKey:
        optionalString(item.toolProfileKey) ?? optionalString(item.toolProfile),
    }) as {
      serverId: string;
      bindingId?: string;
      adapterId?: string;
      serverNames?: string[];
      transport?: string;
      toolProfileKey?: string;
    };
  });
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
    mcpServers: Object.hasOwn(draft, "mcpServers")
      ? arrayValue(draft.mcpServers).map((server) =>
          runtimeConfigMcpServerDraft(server),
        )
      : state.runtimeConfig.mcpServers,
    mcpBindings: arrayValue(
      draft.mcpBindings,
    ) as RustyCrewRuntimeConfig["mcpBindings"],
  };
}

function runtimeConfigDraftFromFileValue(
  state: ServiceState,
  draft: Record<string, unknown>,
): RustyCrewRuntimeConfig {
  return {
    profilesDir:
      optionalString(draft.profilesDir) ?? state.runtimeConfig.profilesDir,
    ...(optionalString(draft.skillsDir) === undefined
      ? {}
      : { skillsDir: optionalString(draft.skillsDir) }),
    storage: state.runtimeConfig.storage,
    denObservation: state.runtimeConfig.denObservation,
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
    mcpServers: Object.hasOwn(draft, "mcpServers")
      ? arrayValue(draft.mcpServers).map((server) =>
          runtimeConfigMcpServerDraft(server),
        )
      : state.runtimeConfig.mcpServers,
    mcpBindings: arrayValue(
      draft.mcpBindings,
    ) as RustyCrewRuntimeConfig["mcpBindings"],
  };
}

function runtimeConfigMcpServerDraft(value: unknown): RustyCrewMcpServerConfig {
  if (!isRecord(value)) {
    throw new Error("runtimeConfig.mcpServers entries must be objects");
  }
  return mcpServerWriteFromBody(value, undefined);
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
      await loadProfileConfigWithRegistryPrompt(state, candidateId),
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
    before.providerAlias !== after.providerAlias ||
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
            modelProviderResolver: (alias) =>
              resolveModelProviderForBrain(state.bridge, alias),
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
            from: (input.actorId || "direct-debug-operator") as never,
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
    deleteProfile: async (command) => {
      const result = await deleteServiceProfile(state, command);
      return {
        status: "completed",
        summary: `profile ${result.profileId} deleted`,
        affectedIds: {
          profileId: result.profileId,
          sessionsDeleted: result.sessionsDeleted.length,
          rowsDeleted: result.storagePurge.rowsDeleted,
          profileDirectoryDeleted: result.profileDirectoryDeleted ? 1 : 0,
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
    newSession: (() => {
      const pendingRuntimeConfigReplacements = new Map<
        string,
        { oldSession: SessionState; plan: ServiceRuntimeReplacementConfigPlan }
      >();
      return createNewSessionLifecycleExecutor({
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
        archiveSession: async ({ sessionId, newSessionId }) => {
          const oldSession = await serviceSessionById(state, sessionId);
          const plan = await planRuntimeSessionReplacementInConfig(
            state,
            oldSession,
            newSessionId,
            "move",
          );
          pendingRuntimeConfigReplacements.set(newSessionId, {
            oldSession,
            plan,
          });
          await state.bridge.archiveSession(sessionId as SessionId);
        },
        createSession: async ({ sessionId, template, command }) => {
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
                : (compactRecord(
                    sessionConfig.historyWindow as never,
                  ) as never),
          });
          const oldSessionId = command.target.sessionId;
          if (oldSessionId !== undefined) {
            const pending =
              pendingRuntimeConfigReplacements.get(sessionId) ?? undefined;
            const oldSession =
              pending?.oldSession ??
              (await serviceSessionById(state, oldSessionId));
            const replacement =
              pending === undefined
                ? await replaceRuntimeSessionInConfig(
                    state,
                    oldSession,
                    sessionId,
                    "move",
                  )
                : await commitRuntimeSessionReplacementInConfig(
                    state,
                    oldSession,
                    pending.plan,
                  );
            pendingRuntimeConfigReplacements.delete(sessionId);
            await applyServiceRuntimeConfigFromDisk(state, {
              createMissingSessions: false,
              eventType: "new_session_runtime_config_moved",
              summaryPrefix: `New session moved runtime config from ${oldSessionId}`,
            });
            recordServiceEvent(state, {
              source: "service-host",
              eventType: "new_session_runtime_config_moved",
              summary: `New session moved runtime config from ${oldSessionId} to ${sessionId}.`,
            });
            recordServiceEvent(state, {
              source: "service-host",
              eventType: "new_session_runtime_config_bindings_moved",
              summary: `New session moved ${replacement.channelBindings.bindingIds.length} channel binding(s), ${replacement.mcpBindings.bindingIds.length} MCP binding(s), and ${replacement.scheduledJobs.jobIds.length} scheduled job(s).`,
            });
          }
        },
        rebindChannel: () => undefined,
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
      });
    })(),
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
    afterReload: async ({ binding, command, outcome }) => {
      if (optionalBodyBoolean(command, "skipBrainRebuildAfterMcpReload")) {
        return outcome;
      }
      const sessionId = command.target.sessionId;
      if (sessionId === undefined) {
        return {
          ...outcome,
          status: "failed",
          summary: `${outcome.summary} Brain rebuild was skipped because the MCP reload had no session target.`,
          reasonCode: "mcp_reload_brain_rebuild_session_missing",
        };
      }

      const rebuild = await applyServiceRuntimeRebuild(state, {
        ...command,
        name: "apply_runtime_rebuild",
        target: { scope: "session", sessionId },
        reason: command.reason ?? "MCP reload refreshed live brain catalog",
        body: {
          ...command.body,
          skipBrainRebuildAfterMcpReload: true,
        },
      });
      const rebuildCompleted = rebuild.apply.status === "completed";
      return {
        status: rebuildCompleted ? "completed" : "failed",
        summary: rebuildCompleted
          ? `${outcome.summary} Rebuilt brain runtime for profile ${binding.profileId}.`
          : `${outcome.summary} Brain rebuild required but was blocked for profile ${binding.profileId}.`,
        affectedIds: {
          ...(outcome.affectedIds ?? {}),
          profileId: binding.profileId,
          sessionId,
        },
        result: {
          reload: outcome.result,
          rebuild,
          followUpAction: rebuildCompleted
            ? "none"
            : "retry_runtime_rebuild_when_unblocked",
        },
        reasonCode: rebuildCompleted
          ? outcome.reasonCode
          : rebuild.apply.status === "blocked"
            ? rebuild.apply.reasonCode
            : "mcp_reload_brain_rebuild_failed",
      };
    },
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
      body: {
        ...command.body,
        skipBrainRebuildAfterMcpReload: true,
      },
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
  profileRegistry?: ServiceRuntimeReplacementSessionResult["profileRegistry"];
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
  profileRegistry: {
    action: "update_session_refs" | "record_missing" | "unchanged";
    updatedProfileId?: string;
    updatedRefIds: string[];
  };
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

interface ServiceRuntimeReplacementConfigPlan {
  oldSessionId: string;
  newSessionId: string;
  runtimeConfigFile: RuntimeConfigFileForMutation;
  validation: RuntimeConfigDraftPlan;
  channelBindings: ServiceRuntimeReplacementSessionResult["channelBindings"];
  mcpBindings: ServiceRuntimeReplacementSessionResult["mcpBindings"];
  scheduledJobs: ServiceRuntimeReplacementSessionResult["scheduledJobs"];
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
    coordinationRuntime: createServiceCoordinationRuntime(() => state),
    toolCallDebugStore: state.toolCallDebugStore,
    providerRequestDebugStore: state.providerRequestDebugStore,
    onBrainWakeResult: (observation) =>
      recordResponsesWakeMetrics(state, observation),
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
    coordinationRuntime: createServiceCoordinationRuntime(() => state),
    toolCallDebugStore: state.toolCallDebugStore,
    providerRequestDebugStore: state.providerRequestDebugStore,
    onBrainWakeResult: (observation) =>
      recordResponsesWakeMetrics(state, observation),
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
    profileRegistry: replacement.profileRegistry,
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
  const plan = await planRuntimeSessionReplacementInConfig(
    state,
    oldSession,
    newSessionId,
    channelBindingAction,
  );
  return commitRuntimeSessionReplacementInConfig(state, oldSession, plan);
}

async function planRuntimeSessionReplacementInConfig(
  state: ServiceState,
  oldSession: SessionState,
  newSessionId: string,
  channelBindingAction: "move" | "unchanged",
): Promise<ServiceRuntimeReplacementConfigPlan> {
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

  const validation = await planRuntimeConfigFileValue(
    state,
    runtimeConfigFile.value,
  );
  assertRuntimeConfigDraftPlanOk(validation);

  return {
    oldSessionId: oldSession.sessionId,
    newSessionId,
    runtimeConfigFile,
    validation,
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
  };
}

async function commitRuntimeSessionReplacementInConfig(
  state: ServiceState,
  oldSession: SessionState,
  plan: ServiceRuntimeReplacementConfigPlan,
): Promise<ServiceRuntimeReplacementSessionResult> {
  await writeJsonFileAtomic(
    state.config.paths.serviceConfigFile,
    plan.runtimeConfigFile.value,
  );
  const profileRegistry = await replaceProfileRegistrySessionRefs(
    state,
    oldSession,
    plan.newSessionId,
  );
  return {
    oldSessionId: oldSession.sessionId,
    newSessionId: plan.newSessionId,
    profileRegistry,
    channelBindings: plan.channelBindings,
    mcpBindings: plan.mcpBindings,
    scheduledJobs: plan.scheduledJobs,
    queuedMessages: {
      action: "start_replacement_session_with_empty_queue",
      oldSessionQueuePreserved: true,
      expiredQueuedMessagesCopied: false,
    },
  };
}

async function replaceProfileRegistrySessionRefs(
  state: ServiceState,
  oldSession: SessionState,
  newSessionId: string,
): Promise<ServiceRuntimeReplacementSessionResult["profileRegistry"]> {
  const record = await state.bridge.getProfileRegistryRecord(
    oldSession.profileId,
  );
  if (record === undefined) {
    return { action: "record_missing", updatedRefIds: [] };
  }

  const now = state.now();
  const updatedRefIds: string[] = [];
  const derivedRuntimeRefs = record.derivedRuntimeRefs.map((ref) => {
    if (ref.refKind !== "session" || ref.refId !== oldSession.sessionId) {
      return ref;
    }
    updatedRefIds.push(ref.refId);
    return {
      ...ref,
      refId: newSessionId,
      updatedAt: now,
      metadataJson: replaceRuntimeRefSessionMetadata(
        ref.metadataJson,
        newSessionId,
      ),
    };
  });

  if (updatedRefIds.length === 0) {
    return {
      action: "unchanged",
      updatedProfileId: record.profileId,
      updatedRefIds: [],
    };
  }

  await state.bridge.updateProfileRegistryRecord({
    write: profileRegistryRecordToWrite(
      {
        ...record,
        derivedRuntimeRefs,
        updatedAt: now,
      },
      now,
    ),
    expectedRevision: record.revision,
  });

  return {
    action: "update_session_refs",
    updatedProfileId: record.profileId,
    updatedRefIds,
  };
}

function replaceRuntimeRefSessionMetadata(
  metadata: unknown,
  newSessionId: string,
): unknown {
  if (!isRecord(metadata)) return metadata;
  const next = { ...metadata };
  if (next.session_id !== undefined) next.session_id = newSessionId;
  if (next.sessionId !== undefined) next.sessionId = newSessionId;
  return next;
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

function stringArray(value: unknown, fieldName: string): string[] {
  if (!Array.isArray(value)) {
    throw new Error(`${fieldName} must be an array`);
  }
  return value.map((item, index) => {
    const text = optionalString(item);
    if (text === undefined) {
      throw new Error(`${fieldName}[${index}] must be a non-empty string`);
    }
    return text;
  });
}

function optionalStringArray(
  value: unknown,
  fallback: string[],
  fieldName: string,
): string[] {
  return value === undefined ? fallback : stringArray(value, fieldName);
}

function requiredString(value: unknown, fieldName: string): string {
  const text = optionalString(value);
  if (!text) throw new Error(`${fieldName} is required`);
  return text;
}

function optionalBoolean(value: unknown): boolean | undefined {
  return typeof value === "boolean" ? value : undefined;
}

function optionalNumber(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value)
    ? value
    : undefined;
}

function optionalTemperatureMilli(
  body: Record<string, unknown>,
): number | undefined {
  const temperatureMilli = optionalNumber(body.temperatureMilli);
  if (temperatureMilli !== undefined) {
    if (Number.isInteger(temperatureMilli)) {
      return temperatureMilli;
    }
    if (temperatureMilli >= 0 && temperatureMilli <= 10) {
      return Math.round(temperatureMilli * 1_000);
    }
    throw new Error(
      "model provider temperatureMilli must be an integer millivalue; use temperature for decimal temperatures",
    );
  }

  const temperature = optionalNumber(body.temperature);
  if (temperature === undefined) {
    return undefined;
  }
  if (temperature < 0) {
    throw new Error("model provider temperature must be non-negative");
  }
  return Math.round(temperature * 1_000);
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
  completionPacket?: CompletionPacket;
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
      void runSchedulerHeartbeat(state).catch((error) => {
        recordSchedulerHeartbeatFailure(state, error);
      });
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
            kind: "rusty_crew_completion_projection.v1",
            delivery_intent_id: intent.id,
            delivery_idempotency_key: intent.idempotency_key,
            source_message_id: deliveryBody.sourceMessageId,
            wake_id: wakeReport.wakeId,
            completion_packet: completionPacketProjectionMetadata(
              wakeReport.completionPacket,
            ),
            work_ref: {
              source_domain: "runtime",
              ref_kind: "session",
              id: session.sessionId,
              delivery_intent_id: intent.id,
              channel_id: deliveryBody.channelId,
              channel_message_id: deliveryBody.sourceMessageId,
            },
            result_ref:
              wakeReport.completionPacket === undefined
                ? undefined
                : {
                    source_domain: "runtime",
                    ref_kind: "completion_packet",
                    id: `${wakeReport.completionPacket.sessionId}:${wakeReport.completionPacket.status}`,
                    label: `completion ${wakeReport.completionPacket.status} for ${wakeReport.completionPacket.sessionId}`,
                  },
            runtime_refs: {
              session_id: session.sessionId,
              profile_id: session.profileId,
              agent_id: session.agentId,
              instance_id: claimedBy.instance_id,
            },
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
        completion_packet: completionPacketProjectionMetadata(
          wakeReport.completionPacket,
        ),
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
    summary: wakeReport.summary,
    reason_code: wakeReport.reasonCode,
  };
  rememberChatMessageReceipt(state, receiptKey, result);
  return result;
}

async function rustyViewSessionContextUsage(
  state: ServiceState,
  input: { session: SessionState; requestId: string },
): Promise<SessionContextUsageResult> {
  const diagnostics: SessionContextUsageResult["diagnostics"] = [];
  const registryRecord = await state.bridge
    .getProfileRegistryRecord(input.session.profileId)
    .catch((error) => {
      diagnostics.push({
        severity: "warning",
        code: "profile_registry_read_failed",
        message: errorMessage(error, "profile registry read failed"),
      });
      return undefined;
    });
  if (registryRecord === undefined) {
    diagnostics.push({
      severity: "warning",
      code: "profile_registry_record_missing",
      message:
        "profile registry record is missing; model diagnostics may be incomplete until the profile is created through the DB-backed profile API",
    });
  }

  const settings =
    optionalRecord(registryRecord?.activeRuntimeSettingsJson) ?? {};
  const providerAlias =
    optionalString(settings.providerAlias) ??
    optionalString(settings.provider_alias) ??
    "default";
  const provider = await state.bridge
    .getModelProvider(providerAlias)
    .catch((error) => {
      diagnostics.push({
        severity: "warning",
        code: "model_provider_read_failed",
        message: errorMessage(error, "model provider read failed"),
      });
      return undefined;
    });
  if (provider === undefined) {
    diagnostics.push({
      severity: "error",
      code: "model_provider_missing",
      message: `model provider alias ${providerAlias} was not found`,
    });
  } else if (provider.status !== "active") {
    diagnostics.push({
      severity: "warning",
      code: "model_provider_not_active",
      message: `model provider alias ${providerAlias} is ${provider.status}`,
    });
  }

  const brain =
    brainMetadataFromUnknown(settings.brain) ??
    (provider === undefined
      ? undefined
      : defaultProfileBrainForModelProvider(provider));
  const toolPolicy = profileToolPolicyFromUnknown(
    settings.toolPolicy ?? settings.tool_policy,
  );
  const contextPolicy = contextStrategyPolicyFromUnknown(
    settings.contextPolicy ?? settings.context_policy,
  );
  const localToolProfileId =
    optionalString(settings.localToolProfileId) ??
    optionalString(settings.local_tool_profile_id);
  const mcpBindings = state.runtimeConfig.mcpBindings.filter(
    (binding) =>
      String(binding.profileId) === input.session.profileId ||
      String(binding.sessionId) === input.session.sessionId,
  );
  const activeMcpBindings = mcpBindings.filter(
    (binding) => binding.status === undefined || binding.status === "active",
  );
  const sampledEvents =
    state.chatEventsBySession.get(input.session.sessionId) ?? [];
  const sampledMessageCount = sampledEvents.filter(
    (event) =>
      event.kind === "message_created" ||
      event.kind === "assistant_message_completed",
  ).length;
  const contextUsage = estimateContextUsage({
    provider,
    textFragments: sampledEvents.flatMap((event) =>
      textFragmentsFromPayload(event.payload),
    ),
    sampledEventCount: sampledEvents.length,
    sampledMessageCount,
  });
  if (contextUsage.budget.contextWindowTokens === undefined) {
    diagnostics.push({
      severity: "info",
      code: "context_window_unknown",
      message: "model provider does not declare contextWindowTokens",
    });
  }
  const latestCompactionArtifact = await state.bridge
    .listContextCompactionArtifacts({
      session_id: input.session.sessionId,
      latest_only: true,
      limit: 1,
      offset: 0,
    })
    .then((artifacts) => artifacts[0])
    .catch((error) => {
      diagnostics.push({
        severity: "warning",
        code: "context_compaction_artifact_read_failed",
        message: errorMessage(error, "context compaction artifact read failed"),
      });
      return undefined;
    });
  const redactedUrl = redactedProviderUrl(provider?.baseUrl);
  return {
    session_id: input.session.sessionId,
    agent_id: input.session.agentId,
    profile_id: input.session.profileId,
    provider: {
      alias: providerAlias,
      status: provider?.status ?? "missing",
      protocol: provider?.protocol,
      provider_kind: provider?.providerKind,
      display_name: provider?.displayName,
      base_url_host: redactedUrl.host,
      base_url_redacted: redactedUrl.redacted,
      model_id: provider?.modelId,
      context_window_tokens: contextUsage.budget.contextWindowTokens,
      max_output_tokens: contextUsage.budget.maxOutputTokens,
      temperature:
        provider?.temperatureMilli === undefined
          ? undefined
          : provider.temperatureMilli / 1_000,
      reasoning_effort: provider?.reasoningEffort,
      reasoning_format: provider?.reasoningFormat,
      revision: provider?.revision,
    },
    brain: {
      module: brain?.module,
      strategy: brain?.strategy,
      backend: brain?.module ?? providerBrainBackend(provider),
    },
    context_strategy: {
      strategy_id: contextPolicy.strategyId,
      enabled: contextPolicy.enabled,
      auto_compaction_enabled: contextPolicy.autoCompactionEnabled,
      compact_at_percent: contextPolicy.compactAtPercent,
      target_percent_after_compaction:
        contextPolicy.targetPercentAfterCompaction,
      max_context_percent_for_wake: contextPolicy.maxContextPercentForWake,
      debug_visibility: contextPolicy.debugVisibility,
      include_debug_events_in_model_context:
        contextPolicy.includeDebugEventsInModelContext,
    },
    tools: {
      local_tool_profile_id: localToolProfileId,
      tool_count: input.session.toolProfile.tools.length,
      requested_toolsets:
        toolPolicy?.requestedToolsets === undefined
          ? undefined
          : [...toolPolicy.requestedToolsets],
      requested_tools:
        toolPolicy?.requestedTools === undefined
          ? undefined
          : [...toolPolicy.requestedTools],
      mcp_binding_count: mcpBindings.length,
      mcp_active_count: activeMcpBindings.length,
    },
    context: {
      estimate_quality: contextUsage.estimateQuality,
      estimate_method: contextUsage.estimateMethod,
      estimator_id: contextUsage.estimatorId,
      context_window_tokens: contextUsage.budget.contextWindowTokens,
      estimated_prompt_tokens: contextUsage.estimatedPromptTokens,
      estimated_remaining_tokens: contextUsage.estimatedRemainingTokens,
      max_output_tokens: contextUsage.budget.maxOutputTokens,
      reserved_response_tokens: contextUsage.budget.reservedResponseTokens,
      safety_margin_tokens: contextUsage.budget.safetyMarginTokens,
      usable_input_tokens: contextUsage.budget.usableInputTokens,
      sampled_event_count: contextUsage.sampledEventCount,
      sampled_message_count: contextUsage.sampledMessageCount,
    },
    latest_compaction_artifact:
      latestCompactionArtifact === undefined
        ? undefined
        : {
            artifact_id: latestCompactionArtifact.artifact_id,
            strategy_id: latestCompactionArtifact.strategy_id,
            branch_id: latestCompactionArtifact.branch_id,
            enters_future_context:
              latestCompactionArtifact.enters_future_context,
            context_policy: latestCompactionArtifact.context_policy,
            created_at: latestCompactionArtifact.created_at,
            updated_at: latestCompactionArtifact.updated_at,
            estimate_before_json: latestCompactionArtifact.estimate_before_json,
            estimate_after_json: latestCompactionArtifact.estimate_after_json,
          },
    degraded: diagnostics.some((diagnostic) => diagnostic.severity !== "info"),
    diagnostics,
  };
}

async function rustyViewToolCallDebugDetail(
  state: ServiceState,
  input: { session: SessionState; debugDetailId: string; requestId: string },
): Promise<ToolCallDebugDetail | undefined> {
  const record = state.toolCallDebugStore.get({
    sessionId: input.session.sessionId,
    debugDetailId: input.debugDetailId,
  });
  if (!record) return undefined;
  return {
    debug_detail_id: record.debug_detail_id,
    tool_call_id: record.tool_call_id,
    session_id: record.session_id,
    wake_id: record.wake_id,
    tool_name: record.tool_name,
    status: record.status,
    arguments: record.arguments,
    partial_updates: record.partial_updates,
    final_result: record.final_result,
    error: record.error,
    source_metadata: record.source_metadata,
    started_at: record.started_at,
    updated_at: record.updated_at,
    expires_at: record.expires_at,
    limits: { ...record.limits },
  };
}

async function rustyViewProviderRequestDebugDetail(
  state: ServiceState,
  input: { session: SessionState; debugDetailId: string; requestId: string },
): Promise<ProviderRequestDebugDetail | undefined> {
  const record = state.providerRequestDebugStore.get({
    sessionId: input.session.sessionId,
    debugDetailId: input.debugDetailId,
  });
  if (!record) return undefined;
  return {
    debug_detail_id: record.debug_detail_id,
    session_id: record.session_id,
    wake_id: record.wake_id,
    provider: record.provider,
    request: record.request,
    request_sha256: record.request_sha256,
    request_json_chars: record.request_json_chars,
    recorded_at: record.recorded_at,
    expires_at: record.expires_at,
    limits: { ...record.limits },
  };
}

function providerBrainBackend(
  provider: NativeModelProviderRecord | undefined,
): string {
  if (provider === undefined) return "unknown";
  return provider.protocol === "responses"
    ? "openai-responses"
    : "pi-agent-core";
}

function redactedProviderUrl(baseUrl: string | undefined): {
  host?: string;
  redacted?: string;
} {
  if (baseUrl === undefined || baseUrl.trim() === "") return {};
  try {
    const parsed = new URL(baseUrl);
    return { host: parsed.host, redacted: parsed.origin };
  } catch {
    return { redacted: "invalid-url" };
  }
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
    include_expired: false,
    expired_only: false,
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
    include_expired: true,
    expired_only: false,
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
    routed.commandName === "session" ||
    routed.commandName === "model"
  ) {
    const diagnosticsContext = await buildDiagnosticsContext(state);
    const modelContext =
      routed.commandName === "model"
        ? await rustyViewSessionContextUsage(state, {
            session: input.session,
            requestId: input.requestId,
          })
        : undefined;
    const response = buildReadOnlySlashCommandResponse(routed.commandName, {
      diagnostics: diagnosticsContext.diagnostics,
      session: slashCommandSession(input.session),
      modelContext,
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
  wakeId: string,
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
          wake_id: wakeId,
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
    case "reasoning_delta":
      appendChatEvent(state, session.sessionId, {
        kind: "assistant_reasoning_delta",
        payload: {
          wake_id: wakeId,
          text: event.text,
          visibility: "reasoning",
          ...(event.format === undefined ? {} : { format: event.format }),
        },
      });
      return;
    case "phase_change":
      appendChatEvent(state, session.sessionId, {
        kind: "phase_change",
        payload: {
          wake_id: wakeId,
          phase: event.phase,
          ...(event.message === undefined ? {} : { message: event.message }),
        },
      });
      return;
    case "provider_status":
      appendChatEvent(state, session.sessionId, {
        kind: "provider_status",
        payload: {
          wake_id: wakeId,
          level: event.level,
          message: event.message,
          ...(event.metadataJson === undefined
            ? {}
            : { metadata_json: event.metadataJson }),
        },
      });
      return;
    case "tool_call_started":
      appendChatEvent(state, session.sessionId, {
        kind: "tool_call_started",
        payload: {
          wake_id: wakeId,
          tool_call_id: chatToolCallId(wakeId, event.toolName, event.metadata),
          tool_name: event.toolName,
          debug_detail_id: event.metadata?.debugDetailId,
          metadata: event.metadata,
        },
      });
      return;
    case "tool_call_finished":
      appendChatEvent(state, session.sessionId, {
        kind: event.isError ? "tool_call_failed" : "tool_call_completed",
        payload: {
          wake_id: wakeId,
          tool_call_id: chatToolCallId(wakeId, event.toolName, event.metadata),
          tool_name: event.toolName,
          is_error: event.isError,
          debug_detail_id: event.metadata?.debugDetailId,
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

function chatToolCallId(
  wakeId: string | undefined,
  toolName: string,
  metadata: ToolCallMetadata | undefined,
): string {
  if (metadata?.debugDetailId) return metadata.debugDetailId;
  return [
    wakeId ?? "wake",
    metadata?.source ?? "tool",
    metadata?.bindingId ?? "local",
    metadata?.sourceToolName ?? toolName,
  ]
    .map((part) => part.replace(/[^A-Za-z0-9_.:-]+/g, "_"))
    .join(":");
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
        event.event.type === "reasoning_delta" ||
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

  ensureChatWakeTerminalEventsFromChatLog(state, session, wakeId, {
    status: "completed",
    summary: fallback.summary,
    source: "terminal_fallback",
    requireCompletion: !hasCompletion,
    requireFinished: !hasFinished,
  });
}

function ensureChatWakeTerminalEventsFromChatLog(
  state: ServiceState,
  session: SessionState,
  wakeId: string,
  input: {
    status: "completed" | "failed";
    summary?: string;
    reasonCode?: string;
    source: string;
    requireCompletion?: boolean;
    requireFinished?: boolean;
  },
): void {
  const events = state.chatEventsBySession.get(session.sessionId) ?? [];
  const wakeEvents = events.filter((event) => {
    const payload = event.payload;
    return isRecord(payload) && payload.wake_id === wakeId;
  });
  const hasAssistantTurn = wakeEvents.some((event) =>
    [
      "assistant_turn_started",
      "assistant_text_delta",
      "assistant_reasoning_delta",
      "tool_call_started",
      "tool_call_completed",
      "tool_call_failed",
    ].includes(event.kind),
  );
  if (!hasAssistantTurn) return;

  const needsCompletion =
    input.requireCompletion !== false &&
    !wakeEvents.some((event) => event.kind === "assistant_message_completed");
  const needsFinished =
    input.requireFinished !== false &&
    !wakeEvents.some((event) => event.kind === "assistant_turn_finished");
  const summary = input.summary?.trim();
  if (needsCompletion && summary) {
    appendChatEvent(state, session.sessionId, {
      kind: "assistant_message_completed",
      payload: {
        status: input.status,
        summary,
        wake_id: wakeId,
        source: input.source,
        ...(input.reasonCode === undefined
          ? {}
          : { reason_code: input.reasonCode }),
      },
    });
  }
  if (needsFinished) {
    appendChatEvent(state, session.sessionId, {
      kind: "assistant_turn_finished",
      payload: {
        wake_id: wakeId,
        source: input.source,
        status: input.status,
        ...(input.reasonCode === undefined
          ? {}
          : { reason_code: input.reasonCode }),
      },
    });
  }
}

function buildChatWakeFailureSummary(
  state: ServiceState,
  session: SessionState | undefined,
  wakeId: string | undefined,
  failureSummary: string,
): string {
  const base = failureSummary.trim() || "assistant turn failed";
  if (!session || !wakeId) return base;

  const events = (
    state.chatEventsBySession.get(session.sessionId) ?? []
  ).filter((event) => {
    const payload = event.payload;
    return isRecord(payload) && payload.wake_id === wakeId;
  });
  if (events.length === 0) return base;

  const text = mergeTextParts(
    events.flatMap((event) => {
      if (event.kind !== "assistant_text_delta") return [];
      const payload = event.payload;
      return isRecord(payload) && typeof payload.text === "string"
        ? [payload.text]
        : [];
    }),
  ).trim();
  const reasoningDeltaCount = events.filter(
    (event) => event.kind === "assistant_reasoning_delta",
  ).length;
  const startedTools = toolCallsForChatEvents(events, "tool_call_started");
  const completedTools = toolCallsForChatEvents(events, "tool_call_completed");
  const failedTools = toolCallsForChatEvents(events, "tool_call_failed");
  const unsuccessfulCompletedTools = unsuccessfulCompletedToolSummaries(
    state,
    session,
    events,
  );
  const inFlightTools = [...startedTools].filter(
    ([toolCallId]) =>
      !completedTools.has(toolCallId) && !failedTools.has(toolCallId),
  );
  const providerStatuses = events.flatMap((event) => {
    if (event.kind !== "provider_status") return [];
    const payload = event.payload;
    if (!isRecord(payload) || typeof payload.message !== "string") return [];
    return [`${String(payload.level ?? "info")}: ${payload.message}`];
  });

  const lines = [`Assistant turn failed before it could finish: ${base}`];
  if (text) {
    lines.push(`Partial response before failure: ${truncate(text, 360)}`);
  }
  if (failedTools.size > 0) {
    lines.push(`Failed tool calls: ${formatToolCallMap(failedTools)}.`);
  }
  if (unsuccessfulCompletedTools.length > 0) {
    lines.push(
      `Tool calls reporting unsuccessful results: ${unsuccessfulCompletedTools
        .slice(0, 5)
        .join("; ")}.`,
    );
  }
  if (completedTools.size > 0) {
    lines.push(`Completed tool calls before failure: ${completedTools.size}.`);
  }
  if (inFlightTools.length > 0) {
    lines.push(
      `Tool calls still in flight: ${inFlightTools
        .slice(0, 5)
        .map(([, toolName]) => toolName)
        .join(", ")}.`,
    );
  }
  if (reasoningDeltaCount > 0) {
    lines.push(`Reasoning updates before failure: ${reasoningDeltaCount}.`);
  }
  if (providerStatuses.length > 0) {
    lines.push(
      `Recent provider status: ${providerStatuses.slice(-3).join("; ")}.`,
    );
  }
  return truncate(lines.join("\n"), 1_500);
}

function toolCallsForChatEvents(
  events: readonly ChatEvent[],
  kind: ChatEvent["kind"],
): Map<string, string> {
  const calls = new Map<string, string>();
  for (const event of events) {
    if (event.kind !== kind) continue;
    const payload = event.payload;
    if (!isRecord(payload) || typeof payload.tool_name !== "string") {
      continue;
    }
    const toolCallId =
      typeof payload.tool_call_id === "string"
        ? payload.tool_call_id
        : `${kind}:${calls.size}:${payload.tool_name}`;
    calls.set(toolCallId, payload.tool_name);
  }
  return calls;
}

function unsuccessfulCompletedToolSummaries(
  state: ServiceState,
  session: SessionState,
  events: readonly ChatEvent[],
): string[] {
  return events.flatMap((event) => {
    if (event.kind !== "tool_call_completed") return [];
    const payload = event.payload;
    if (!isRecord(payload)) return [];
    const debugDetailId =
      typeof payload.debug_detail_id === "string"
        ? payload.debug_detail_id
        : undefined;
    if (!debugDetailId) return [];
    const record = state.toolCallDebugStore.get({
      sessionId: session.sessionId,
      debugDetailId,
    });
    const detail = unsuccessfulToolDetail(record?.final_result?.value);
    if (!detail) return [];
    const toolName =
      typeof payload.tool_name === "string"
        ? payload.tool_name
        : record?.tool_name;
    return [toolName ? `${toolName} (${detail})` : detail];
  });
}

function unsuccessfulToolDetail(value: unknown): string | undefined {
  if (!isRecord(value)) return undefined;
  const details = isRecord(value.details) ? value.details : undefined;
  if (!details) return undefined;
  if (details.ok !== false && details.action !== "failed") return undefined;
  if (typeof details.reasonCode === "string") return details.reasonCode;
  if (typeof details.action === "string") return details.action;
  return "ok=false";
}

function formatToolCallMap(calls: Map<string, string>): string {
  return [...calls.values()].slice(0, 5).join(", ");
}

function appendChatEvent(
  state: ServiceState,
  sessionId: SessionId,
  event: Pick<ChatEvent, "kind" | "payload">,
): ChatEvent {
  const sequence =
    Math.max(
      state.chatSequencesBySession.get(sessionId) ?? 0,
      state.chatEventStore.latestSequence(sessionId) ?? 0,
    ) + 1;
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
  if (events.length > CHAT_EVENT_RETENTION_LIMIT) {
    events.splice(0, events.length - CHAT_EVENT_RETENTION_LIMIT);
  }
  state.chatEventsBySession.set(sessionId, events);
  state.chatEventStore.append(chatEvent);
  const subscribers = state.chatSubscribersBySession.get(sessionId);
  if (subscribers !== undefined) {
    for (const subscriber of subscribers) {
      subscriber.write(chatEvent);
    }
  }
  return chatEvent;
}

interface ContextCompactionDebugEventInput {
  wakeId?: string;
  strategyId: string;
  estimateQuality: string;
  fillPercent?: number;
  compactAtPercent?: number;
  targetPercentAfterCompaction?: number;
  artifactId?: string;
  reasonCode?: string;
  fail: boolean;
}

function emitContextCompactionDebugEvents(
  state: ServiceState,
  session: SessionState,
  input: ContextCompactionDebugEventInput,
): { events: ChatEvent[]; latest_cursor: string } {
  const basePayload = contextDebugPayload(session.sessionId, input);
  const events = [
    appendChatEvent(state, session.sessionId, {
      kind: "context_status",
      payload: {
        ...basePayload,
        status: input.fail ? "will_fail" : "ready",
      },
    }),
    appendChatEvent(state, session.sessionId, {
      kind: "context_compaction_started",
      payload: {
        ...basePayload,
        status: "started",
      },
    }),
  ];
  events.push(
    appendChatEvent(state, session.sessionId, {
      kind: input.fail
        ? "context_compaction_failed"
        : "context_compaction_completed",
      payload: {
        ...basePayload,
        status: input.fail ? "failed" : "completed",
        reason_code: input.fail
          ? (input.reasonCode ?? "debug_context_compaction_failed")
          : input.reasonCode,
      },
    }),
  );
  return {
    events,
    latest_cursor:
      events.at(-1)?.event_id ??
      latestChatCursor(state, session.sessionId) ??
      "",
  };
}

function contextDebugPayload(
  sessionId: SessionId,
  input: ContextCompactionDebugEventInput,
): Record<string, unknown> {
  return {
    session_id: sessionId,
    wake_id: input.wakeId,
    strategy_id: input.strategyId,
    estimate_quality: safeEstimateQuality(input.estimateQuality),
    fill_percent: boundedPercent(input.fillPercent),
    compact_at_percent: boundedPercent(input.compactAtPercent),
    target_percent_after_compaction: boundedPercent(
      input.targetPercentAfterCompaction,
    ),
    artifact_id: input.artifactId,
    ui_debug: true,
    model_facing: false,
  };
}

function safeEstimateQuality(
  raw: string,
): "exact" | "approximate" | "unavailable" {
  return raw === "exact" || raw === "unavailable" ? raw : "approximate";
}

function boundedPercent(value: number | undefined): number | undefined {
  if (value === undefined || !Number.isFinite(value)) return undefined;
  return Math.max(0, Math.min(100, Math.trunc(value)));
}

function listChatEventsAfterCursor(
  state: ServiceState,
  session: SessionState,
  cursor: string | undefined,
  limit: number,
): readonly ChatEvent[] {
  if (limit <= 0) return [];
  const events = state.chatEventsBySession.get(session.sessionId) ?? [];
  const storedEvents = state.chatEventStore.listAfterCursor(
    session.sessionId,
    cursor,
    limit,
  );
  if (storedEvents.length > 0) {
    const mergedEvents = mergeChatEventPages(
      storedEvents,
      events,
      session.sessionId,
      cursor,
    );
    return cursor === undefined
      ? mergedEvents.slice(Math.max(0, mergedEvents.length - limit))
      : mergedEvents.slice(0, limit);
  }
  if (cursor === undefined)
    return events.slice(Math.max(0, events.length - limit));
  const after = cursorSequence(cursor, session.sessionId);
  return events.filter((event) => event.sequence_id > after).slice(0, limit);
}

function mergeChatEventPages(
  storedEvents: readonly ChatEvent[],
  memoryEvents: readonly ChatEvent[],
  sessionId: SessionId,
  cursor: string | undefined,
): readonly ChatEvent[] {
  const after = cursorSequence(cursor, sessionId);
  const eventsBySequence = new Map<number, ChatEvent>();
  for (const event of [...storedEvents, ...memoryEvents]) {
    if (event.session_id !== sessionId || event.sequence_id <= after) continue;
    eventsBySequence.set(event.sequence_id, event);
  }
  return [...eventsBySequence.values()].sort(
    (left, right) => left.sequence_id - right.sequence_id,
  );
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
  if (after > 0 || cursor === undefined) return events;
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
  const latestSequence = Math.max(
    state.chatEventStore.latestSequence(sessionId) ?? 0,
    state.chatEventsBySession.get(sessionId)?.at(-1)?.sequence_id ?? 0,
  );
  return latestSequence > 0 ? `${sessionId}:${latestSequence}` : undefined;
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

function createServiceCoordinationRuntime(
  getState: () => ServiceState | undefined,
): CoordinationToolRuntime {
  const runtime: CoordinationToolRuntime = {
    async routeMessage(input) {
      const state = getState();
      if (state === undefined) {
        throw new Error("service coordination runtime is not ready");
      }
      const receipt = await state.bridge.routeAgentMessage(
        input.fromAgentId,
        input.toAgentId,
        input.body,
        input.correlationId,
      );
      const targetSession = (await state.bridge.listSessions()).find(
        (candidate) => candidate.agentId === input.toAgentId,
      );
      if (targetSession === undefined) {
        return {
          accepted: receipt.accepted,
          sequence: receipt.sequence,
          wake: {
            status: "skipped",
            summary: `message routed to ${input.toAgentId}; no target session found to wake`,
            reasonCode: "target_session_missing",
          },
        };
      }
      if (input.requireWake === false) {
        return {
          accepted: receipt.accepted,
          sequence: receipt.sequence,
          wake: {
            status: "skipped",
            summary: `message routed to ${input.toAgentId}; wake not requested`,
            reasonCode: "wake_not_requested",
          },
        };
      }
      const pause = runtimePauseForSession(state, targetSession);
      if (pause !== undefined) {
        return {
          accepted: receipt.accepted,
          sequence: receipt.sequence,
          wake: runtimePauseWakeReport(state, targetSession.sessionId, pause),
        };
      }
      state.directDispatchSessions.add(targetSession.sessionId);
      try {
        const wake = await dispatchWake(
          state,
          {
            type: "brain_wake_requested",
            sessionId: targetSession.sessionId,
          },
          "direct_debug",
        );
        suppressNextWakeEvent(state, targetSession.sessionId);
        await drainAndDispatchWakes(state, "direct_debug");
        return {
          accepted: receipt.accepted,
          sequence: receipt.sequence,
          wake,
        };
      } finally {
        state.directDispatchSessions.delete(targetSession.sessionId);
      }
    },
    async roundTrip(input) {
      const state = getState();
      if (state === undefined) {
        throw new Error("service coordination runtime is not ready");
      }
      const subscription = await state.bridge.subscribeEvents({
        eventKinds: ["agent_message_routed"],
      });
      try {
        const routed = await runtime.routeMessage({
          fromAgentId: input.fromAgentId,
          toAgentId: input.toAgentId,
          body: input.body,
          correlationId: input.correlationId,
          requireWake: true,
        });
        const deadline = Date.now() + input.timeoutMs;
        while (Date.now() < deadline) {
          const events = await state.bridge.drainSubscriptionEvents(
            subscription,
            32,
          );
          const replyEvent = events.find((event) =>
            isCorrelatedReply(event, input),
          );
          if (replyEvent !== undefined) {
            return {
              ...routed,
              reply: replyFromEvent(replyEvent),
            };
          }
          await drainAndDispatchWakes(state, "direct_debug");
          await delay(25);
        }
        return {
          ...routed,
          timedOut: true,
        };
      } finally {
        await state.bridge
          .unsubscribeEvents(subscription)
          .catch(() => undefined);
      }
    },
  };
  return runtime;
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
        body: message.body ?? "",
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
      modelProviderResolver: (alias) =>
        resolveModelProviderForBrain(state.bridge, alias),
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
    const sessionActivityDigests = await state.bridge
      .listSessionActivityDigests({
        profile_id: profileId as ProfileId,
        include_reviewed: false,
        limit: payload.maxCandidates ?? 100,
        offset: 0,
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
      sessionActivityDigests,
      captureProvider: (captureInput) =>
        runStructuredCaptureProvider({
          ...captureInput,
          bridge: state.bridge,
        }),
    });
    const persistedCaptureProposalCount =
      await persistBackgroundReviewProposals(state, result);
    state.backgroundReview.lastRunAt = result.finishedAt;
    state.backgroundReview.lastError = undefined;
    state.backgroundReview.recentFindings = result.findingCount;
    state.backgroundReview.lastCaptureProposalCount = result.findings.filter(
      (finding) => finding.memoryProposal !== undefined,
    ).length;
    state.backgroundReview.lastPersistedCaptureProposalCount =
      persistedCaptureProposalCount;
    state.backgroundReview.lastSkippedReasons = result.skippedReasons;
    recordServiceEvent(state, {
      source: "background-review",
      eventType: "memory_skills_review_completed",
      summary: `Background ${result.reviewType} review for ${result.profileId} produced ${result.findingCount} finding(s) and persisted ${persistedCaptureProposalCount} capture proposal(s).`,
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

async function persistBackgroundReviewProposals(
  state: ServiceState,
  result: BackgroundReviewResult,
): Promise<number> {
  if (result.dryRun) return 0;
  let persisted = 0;
  for (const finding of result.findings) {
    if (finding.memoryProposal === undefined) continue;
    try {
      await state.bridge.saveMemoryProposal(finding.memoryProposal);
      persisted += 1;
    } catch (error) {
      recordServiceEvent(state, {
        source: "background-review",
        eventType: "capture_proposal_persist_failed",
        severity: "warning",
        summary: errorMessage(error, "capture proposal persist failed"),
      });
    }
  }
  return persisted;
}

async function persistSessionActivityDigest(input: {
  state: ServiceState;
  session: SessionState;
  wakeId: string;
  source: ServiceWakeSource;
  observedEvents: readonly CoreEvent[];
  completionSummary?: string;
}): Promise<void> {
  try {
    const digest = buildSessionActivityDigest({
      profileId: input.session.profileId,
      sessionId: input.session.sessionId,
      wakeId: input.wakeId,
      source: input.source,
      events: input.observedEvents,
      completionSummary: input.completionSummary,
      now: input.state.now(),
    });
    await input.state.bridge.saveSessionActivityDigest(digest);
    recordServiceEvent(input.state, {
      source: "session-activity-digest",
      eventType: "session_activity_digest_saved",
      summary: `Saved activity digest ${digest.digest_id} for wake ${input.wakeId}.`,
    });
  } catch (error) {
    recordServiceEvent(input.state, {
      source: "session-activity-digest",
      eventType: "session_activity_digest_save_failed",
      severity: "warning",
      summary: errorMessage(error, "session activity digest save failed"),
    });
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
  if (state.schedulerHeartbeat.running) {
    state.schedulerHeartbeat.lastSkippedAt = state.now();
    state.schedulerHeartbeat.lastSkipReason =
      "previous scheduler heartbeat is still running";
    recordServiceEvent(state, {
      source: "service-host",
      eventType: "scheduler_heartbeat_skipped",
      severity: "warning",
      summary:
        "Scheduler heartbeat skipped because the previous tick is still running.",
    });
    return;
  }
  const startedAt = state.now();
  const startedMonotonic = Date.now();
  state.schedulerHeartbeat.running = true;
  state.schedulerHeartbeat.lastStartedAt = startedAt;
  state.schedulerHeartbeat.lastSkipReason = undefined;
  try {
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
    const summary = `Scheduler heartbeat: ${tick.wakesRequested} wakes requested, ${tick.runsCompleted} wake runs completed, ${hostRuns.completed} host runs completed, ${scheduledJobs.registered} configured jobs reconciled, ${curatorLifecycle.transitions.length} curator lifecycle transitions, ${maintenance.expiredQueueMessages} queued messages expired.`;
    state.schedulerHeartbeat.lastCompletedAt = state.now();
    state.schedulerHeartbeat.lastDurationMs = Date.now() - startedMonotonic;
    state.schedulerHeartbeat.lastSummary = summary;
    state.schedulerHeartbeat.lastError = undefined;
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
        summary,
      });
    }
  } finally {
    state.schedulerHeartbeat.running = false;
  }
}

function recordSchedulerHeartbeatFailure(
  state: ServiceState,
  error: unknown,
): void {
  const summary = errorMessage(error, "scheduler heartbeat failed");
  state.schedulerHeartbeat.lastCompletedAt = state.now();
  state.schedulerHeartbeat.lastError = summary;
  state.schedulerHeartbeat.lastSummary = summary;
  recordServiceEvent(state, {
    source: "service-host",
    eventType: "scheduler_heartbeat_failed",
    severity: "error",
    summary,
  });
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
  let activeWake:
    | {
        session: SessionState;
        wakeId: string;
      }
    | undefined;
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
    activeWake = { session, wakeId };
    const profileContext = await loadProfileContext({
      profilesDir: state.runtimeConfig.profilesDir,
      skillsDir: state.runtimeConfig.skillsDir,
      profileId: session.profileId,
      modelProviderResolver: (alias) =>
        resolveModelProviderForBrain(state.bridge, alias),
    });
    const configured = configuredSessionForRuntimeSession(
      state.runtimeConfig,
      session,
    );
    const contextStrategy = await prepareContextStrategyForWake(state, {
      session,
      configuredSession: configured,
      profileContext,
    });
    const roleplayContext = await roleplayPromptContextForSession(
      state,
      session,
    );
    const role = buildProfileRoleAssembly(profileContext, {
      sessionMemoryContext: contextStrategy.sessionMemoryContext,
      additionalInstructions: [
        ...contextStrategy.additionalInstructions,
        ...(roleplayContext === undefined ? [] : [roleplayContext]),
      ],
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
        (events) => appendCoreEventsToChatLog(state, session, wakeId, events),
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
    const completionPacket = wakeCompletionPacket(observed.events);
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
      completionPacket,
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
      await persistSessionActivityDigest({
        state,
        session,
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
        summary: buildChatWakeFailureSummary(
          state,
          activeWake?.session,
          error.wakeId,
          `wake ${error.wakeId} timed out after ${error.timeoutMs}ms`,
        ),
        reasonCode: "wake_timeout",
      };
      if (activeWake !== undefined) {
        ensureChatWakeTerminalEventsFromChatLog(
          state,
          activeWake.session,
          error.wakeId,
          {
            status: "failed",
            summary: report.summary,
            reasonCode: report.reasonCode,
            source: "wake_timeout",
          },
        );
      }
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
      wakeId: activeWake?.wakeId,
      status: "failed",
      summary: buildChatWakeFailureSummary(
        state,
        activeWake?.session,
        activeWake?.wakeId,
        errorMessage(error, `wake for ${sessionId} failed`),
      ),
      reasonCode: "wake_dispatch_failed",
    };
    if (activeWake !== undefined) {
      ensureChatWakeTerminalEventsFromChatLog(
        state,
        activeWake.session,
        activeWake.wakeId,
        {
          status: "failed",
          summary: report.summary,
          reasonCode: report.reasonCode,
          source: "wake_dispatch_failed",
        },
      );
    }
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
  const maxEvents = 65_536;
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
  const packet = wakeCompletionPacket(events);
  if (packet?.summary.trim()) {
    return packet.summary.trim();
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

function wakeCompletionPacket(
  events: readonly CoreEvent[],
): CompletionPacket | undefined {
  return events
    .filter(
      (
        event,
      ): event is Extract<CoreEvent, { type: "completion_packet_delivered" }> =>
        event.type === "completion_packet_delivered",
    )
    .at(-1)?.packet;
}

function completionPacketProjectionMetadata(
  packet: CompletionPacket | undefined,
): Record<string, unknown> | undefined {
  if (packet === undefined) return undefined;
  return {
    kind: "completion_packet.v1",
    session_id: packet.sessionId,
    status: packet.status,
    summary: packet.summary,
  };
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

async function stopService(state: ServiceState): Promise<void> {
  if (state.stopping) return;
  state.stopping = true;
  for (const timer of state.timers) clearInterval(timer);
  state.timers.clear();
  try {
    await stopTelegramConnector(state);
    if (state.denObservationSubscription !== undefined) {
      await state.bridge
        .unsubscribeEvents(state.denObservationSubscription)
        .catch(() => undefined);
      state.denObservationSubscription = undefined;
    }
    await state.bridge
      .unsubscribeEvents(state.wakeSubscription)
      .catch(() => undefined);
    await state.chatEventStore.flush();
    await state.mcpManager.shutdown();
    await state.bridge.shutdownEngine({
      engine: state.engine,
      drainTimeoutMs: 5_000,
    });
  } finally {
    state.lock.release();
  }
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

function isRoleplayBrowserRoute(pathname: string): boolean {
  return (
    pathname.startsWith("/v1/admin/roleplay/") ||
    /^\/v1\/profile\/[^/]+\/layers\/?$/.test(pathname)
  );
}

function isBrowserCorsRoute(pathname: string): boolean {
  return isChatRoute(pathname) || isRoleplayBrowserRoute(pathname);
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
    "access-control-allow-methods": "GET,POST,PATCH,DELETE,OPTIONS",
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

function stringParam(url: URL, key: string): string | undefined {
  const value = url.searchParams.get(key);
  return value === null || value.trim() === "" ? undefined : value;
}

function numberParam(url: URL, key: string): number | undefined {
  const value = stringParam(url, key);
  if (value === undefined) return undefined;
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) && parsed >= 0 ? parsed : undefined;
}

function modelProviderStatusParam(
  value: string | null,
): NativeModelProviderStatus | "invalid" | undefined {
  if (value === null || value.trim() === "") return undefined;
  return value === "active" || value === "disabled" || value === "archived"
    ? value
    : "invalid";
}

function modelProviderProtocolFromBody(
  value: unknown,
): NativeModelProviderProtocol {
  const protocol = optionalString(value) ?? "chat_completions";
  if (protocol === "responses" || protocol === "chat_completions") {
    return protocol;
  }
  throw new Error(
    "model provider protocol must be responses or chat_completions",
  );
}

function modelProviderSecretFromBody(
  body: Record<string, unknown>,
): string | undefined {
  const credentialSecret = body.credentialSecret ?? body.credential_secret;
  if (credentialSecret !== undefined) {
    return JSON.stringify(
      modelProviderCredentialSecretEnvelope(credentialSecret),
    );
  }
  return optionalString(body.secret ?? body.apiKey);
}

function modelProviderCredentialSecretEnvelope(
  value: unknown,
): Record<string, unknown> {
  if (!isRecord(value)) {
    throw new Error("model provider credentialSecret must be an object");
  }
  const kind = requiredString(
    value.kind,
    "model provider credentialSecret.kind",
  );
  const version = optionalNumber(value.version) ?? 1;
  if (version !== 1) {
    throw new Error("model provider credentialSecret.version must be 1");
  }
  if (kind === "api_key") {
    return {
      kind,
      version,
      value: requiredString(
        value.value ?? value.apiKey ?? value.api_key,
        "model provider credentialSecret.value",
      ),
    };
  }
  if (kind === "openai_oauth") {
    return {
      kind,
      version,
      issuer: requiredString(
        value.issuer,
        "model provider credentialSecret.issuer",
      ),
      client_id: requiredString(
        value.clientId ?? value.client_id,
        "model provider credentialSecret.clientId",
      ),
      id_token: requiredString(
        value.idToken ?? value.id_token,
        "model provider credentialSecret.idToken",
      ),
      access_token: requiredString(
        value.accessToken ?? value.access_token,
        "model provider credentialSecret.accessToken",
      ),
      refresh_token: requiredString(
        value.refreshToken ?? value.refresh_token,
        "model provider credentialSecret.refreshToken",
      ),
      exchanged_api_token: optionalString(
        value.exchangedApiToken ?? value.exchanged_api_token,
      ),
      last_refresh_at: optionalString(
        value.lastRefreshAt ?? value.last_refresh_at,
      ),
      account_id: optionalString(value.accountId ?? value.account_id),
      email: optionalString(value.email),
      plan_type: optionalString(value.planType ?? value.plan_type),
      is_fedramp_account:
        optionalBoolean(value.isFedrampAccount ?? value.is_fedramp_account) ??
        false,
      access_token_expires_at: optionalString(
        value.accessTokenExpiresAt ?? value.access_token_expires_at,
      ),
    };
  }
  throw new Error(
    "model provider credentialSecret.kind must be api_key or openai_oauth",
  );
}

function modelProviderWriteFromBody(
  body: unknown,
  pathAlias: string | undefined,
  now: string,
): NativeModelProviderWrite {
  if (!isRecord(body)) {
    throw new Error("model provider request body must be an object");
  }
  const alias = pathAlias ?? requiredString(body.alias, "model provider alias");
  const status = modelProviderStatusParam(optionalString(body.status) ?? null);
  if (status === "invalid") {
    throw new Error(
      "model provider status must be active, disabled, or archived",
    );
  }
  return {
    alias,
    status: status ?? "active",
    protocol: modelProviderProtocolFromBody(body.protocol),
    providerKind: optionalString(body.providerKind) ?? "custom",
    displayName: optionalString(body.displayName),
    description: optionalString(body.description),
    baseUrl: optionalString(body.baseUrl),
    modelId: requiredString(
      body.modelId ?? body.model,
      "model provider modelId",
    ),
    contextWindowTokens: optionalNumber(body.contextWindowTokens),
    maxOutputTokens: optionalNumber(body.maxOutputTokens),
    temperatureMilli: optionalTemperatureMilli(body),
    reasoningEffort: optionalString(body.reasoningEffort),
    reasoningFormat: optionalString(body.reasoningFormat),
    secret: modelProviderSecretFromBody(body),
    clearSecret: optionalBoolean(body.clearSecret),
    metadataJson: isRecord(body.metadataJson) ? body.metadataJson : {},
    expectedRevision: optionalNumber(body.expectedRevision),
    now,
  };
}

async function resolveModelProviderForBrain(
  bridge: NativeBridgeModule,
  alias: string,
): Promise<BrainModelConfig> {
  const provider = await bridge.getModelProvider(alias);
  if (provider === undefined) {
    throw new Error(`model provider alias ${alias} was not found`);
  }
  if (provider.status !== "active") {
    throw new Error(
      `model provider alias ${alias} is ${provider.status}; active provider required`,
    );
  }
  const secret = provider.credential.hasSecret
    ? await bridge.getModelProviderSecret(alias)
    : undefined;
  return modelProviderToBrainModelConfig(provider, secret);
}

function modelProviderToBrainModelConfig(
  provider: NativeModelProviderRecord,
  secret: string | undefined,
): BrainModelConfig {
  const apiKey = modelProviderApiKeySecret(provider, secret);
  const credentialKind =
    provider.credential.kind ??
    (apiKey === undefined ? undefined : "legacy_raw_api_key");
  const apiKeyEnv =
    apiKey === undefined
      ? undefined
      : modelProviderSecretEnvName(provider.alias);
  if (apiKeyEnv !== undefined) {
    process.env[apiKeyEnv] = apiKey;
  }
  return {
    provider: provider.providerKind,
    modelName: provider.modelId,
    baseUrl: provider.baseUrl,
    api:
      provider.protocol === "responses"
        ? "openai-responses"
        : "openai-completions",
    apiKeyEnv,
    credentialKind,
    temperatureMilli: provider.temperatureMilli,
    maxOutputTokens: provider.maxOutputTokens,
  };
}

function modelProviderApiKeySecret(
  provider: NativeModelProviderRecord,
  secret: string | undefined,
): string | undefined {
  if (secret === undefined) {
    return undefined;
  }
  const trimmed = secret.trim();
  if (!trimmed.startsWith("{")) {
    return secret;
  }
  const envelope = JSON.parse(trimmed) as unknown;
  if (!isRecord(envelope)) {
    throw new Error(
      `model provider ${provider.alias} secret envelope is invalid`,
    );
  }
  if (envelope.kind === "api_key" && typeof envelope.value === "string") {
    return envelope.value;
  }
  if (envelope.kind === "openai_oauth") {
    return undefined;
  }
  throw new Error(
    `model provider ${provider.alias} secret envelope kind is unsupported`,
  );
}

function modelProviderSecretEnvName(alias: string): string {
  return `RUSTY_CREW_MODEL_PROVIDER_SECRET_${alias
    .toUpperCase()
    .replace(/[^A-Z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "")}`;
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

function requiredRecord(
  value: unknown,
  fieldName: string,
): Record<string, unknown> {
  const record = optionalRecord(value);
  if (record === undefined) {
    throw new Error(`${fieldName} must be an object`);
  }
  return record;
}

function randomBase64Url(byteLength: number): string {
  return base64Url(randomBytes(byteLength));
}

function base64Url(bytes: Buffer): string {
  return bytes
    .toString("base64")
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/g, "");
}

function addMilliseconds(isoTimestamp: string, milliseconds: number): string {
  const parsed = Date.parse(isoTimestamp);
  const base = Number.isFinite(parsed) ? parsed : Date.now();
  return new Date(base + milliseconds).toISOString();
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
