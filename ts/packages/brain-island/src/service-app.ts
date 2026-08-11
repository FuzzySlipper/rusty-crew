import type { IncomingMessage, ServerResponse } from "node:http";
import { join } from "node:path";
import type {
  AdapterId,
  BrainEvent,
  BrainModelConfig,
  AgentId,
  AgentInstanceId,
  ChannelBindingRecord,
  CoreEvent,
  EngineHandle,
  EngineStorageConfig,
  McpBindingRecord,
  ProfileId,
  ResourceLimits,
  ScheduledRunSummary,
  SessionId,
  SessionKind,
  SessionState,
  SessionWorkspaceUpdateRecord,
  SubscriptionHandle,
} from "@rusty-crew/contracts";
import {
  loadNativeBridge,
  type NativeBridgeModule,
  type NativeModelProviderRecord,
  type NativeModelProviderWrite,
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
  DenConversationChannelResolution,
  DenSuccessorAgentIdentity,
  DenSuccessorConversationMembership,
  DenSuccessorDeliveryIntent,
  DenSuccessorGatewayClient,
  McpSurfaceManagerPort,
  TelegramChannelConnectorPort,
} from "./service-adapter-ports.js";
import {
  deliveryIntentWakeDecision,
  normalizeChannelWakePolicy,
  type ChannelWakePolicy,
  type DeliveryIntentWakeDecision,
} from "./channel-wake-policy.js";
import type { CoordinationToolRuntime } from "./coordination-tools.js";
import { resolveRawDeliveryTarget } from "./coordination-addressing.js";
import {
  createMemoryAdminControlAuditSink,
  type AdminControlCommand,
  type AdminControlCommandName,
  type AdminControlExecutor,
  type AdminControlResponse,
  handleAdminControlRequest,
} from "./admin-control-api.js";
import { createNewSessionLifecycleExecutor } from "./new-session-lifecycle.js";
import { createReloadMcpControlExecutor } from "./reload-mcp-control.js";
import { createBridgeToolMetadataPolicyValidator } from "./mcp-tool-registry-integration.js";
import { createDefaultMcpDiscoveryClient } from "./service-mcp-tools.js";
import { createLocalToolProfileStore } from "./local-tool-profiles.js";
import {
  handleAdminDiagnosticsRequest,
  type AdminDiagnosticsContext,
  type MemorySpaceDiagnosticsProjection,
} from "./admin-diagnostics-api.js";
import { buildMemorySurfaceCatalog } from "./memory-surface-diagnostics.js";
import { builtInSkillCatalogDiagnostics } from "./built-in-skills.js";
import {
  isNativeReasoningEffort,
  resolveReasoningEffort,
} from "./reasoning-effort-policy.js";
import { handleAdminContextStrategiesRequest } from "./service-context-strategy-routes.js";
import { handleAdminBrainCatalogRequest } from "./service-brain-catalog-routes.js";
import { handleAdminMcpCatalogRequest } from "./service-mcp-catalog-routes.js";
import { handleAdminMcpServerRegistryRequest } from "./service-mcp-server-registry-routes.js";
import {
  failure,
  successRoute,
  type ServiceRouteResult,
} from "./service-route-results.js";
import {
  chatCorsHeaders,
  chatCorsPreflightResponse,
  controlBearerToken,
  controlHeaders,
  headers,
  isAuthorized,
  optionalInteger,
  readJsonBody,
  requestId,
  withChatCors,
  writeJsonResponse,
} from "./service-http-route-helpers.js";
import { handleSchedulerReadRequest } from "./service-scheduler-routes.js";
import { handleAdminToolsCatalogRequest } from "./service-tool-catalog-routes.js";
import { handleAdminLocalToolProfilesRequest } from "./service-local-tool-profile-routes.js";
import { handleLogicalTurnRoute } from "./service-logical-turn-routes.js";
import type { ServiceBackgroundLoopPort } from "./service-background-loops.js";
import { handleMemorySpaceAdminRequest } from "./memory-space-api.js";
import {
  handleAdminRoleplayRequest,
  isRoleplayBrowserRoute,
  roleplayPromptContextForSession,
  roleplaySpeakerIdentitySnapshotForMessage,
  type RoleplayAssistantAlternativeGenerationInput,
  type RoleplayAssistantAlternativeGenerationResult,
  type RoleplayRouteContext,
} from "./service-roleplay-routes.js";
import {
  handleModelProviderAdminRequest,
  type ModelProviderRefreshMode,
  type ModelProviderWriteRefreshResult,
} from "./service-model-provider-routes.js";
import { handleServiceCredentialAdminRequest } from "./service-credential-admin-routes.js";
import { handleTelegramDiplomatAdminRequest } from "./telegram-diplomat-admin-routes.js";
import type { OpenAiOauthPendingLogin } from "./service-openai-oauth-routes.js";
import { DeferredRuntimeActivitySettlementQueue } from "./runtime-activity-settlement.js";
import {
  handleProfileRegistryWriteRequest,
  isProfileRegistryWriteRoute,
  type ProfileRegistryWriteRoute,
} from "./service-profile-registry-routes.js";
import {
  applyProfileRegistryRuntimeConfigEffects as applyProfileRegistryRuntimeConfigEffectsFromModule,
  planProfileRegistryRuntimeConfigWrite as planProfileRegistryRuntimeConfigWriteFromModule,
  planProfileRegistryWrite as planProfileRegistryWriteFromModule,
  type ProfileRegistryRuntimeConfigPlan as ExtractedProfileRegistryRuntimeConfigPlan,
} from "./service-profile-runtime-mutations.js";
import {
  applyServiceProfileUpdate as applyServiceProfileUpdateFromModule,
  applyServiceRuntimeConfigDraft as applyServiceRuntimeConfigDraftFromModule,
  createServiceProfile as createServiceProfileFromModule,
  defaultProfileBrainForModelProvider,
  decommissionServiceProfile as decommissionServiceProfileFromModule,
  deleteServiceProfile as deleteServiceProfileFromModule,
  planServiceProfileUpdate as planServiceProfileUpdateFromModule,
  planServiceRuntimeConfigDraft as planServiceRuntimeConfigDraftFromModule,
  readRuntimeConfigFileForMutation as readRuntimeConfigFileForMutationFromModule,
  readServiceProfileConfig as readServiceProfileConfigFromModule,
  unregisterServiceProfileBrain as unregisterServiceProfileBrainFromModule,
  writeJsonFileAtomic as writeJsonFileAtomicFromModule,
  planRuntimeConfigFileValue,
  type DecommissionedServiceProfile,
  type ServiceProfileAdminMutationContext,
} from "./service-profile-admin-mutations.js";
import {
  archiveCrewSession,
  createFreshCrewSession,
  CrewSessionLifecycleError,
  switchCrewSessionWorkspace,
  type CrewSessionLifecycleContext,
} from "./service-crew-session-lifecycle.js";
import {
  applyServiceRuntimeRebuild as applyServiceRuntimeRebuildFromModule,
  commitRuntimeSessionReplacementInConfig as commitRuntimeSessionReplacementInConfigFromModule,
  planRuntimeSessionReplacementInConfig as planRuntimeSessionReplacementInConfigFromModule,
  planServiceRuntimeRebuild as planServiceRuntimeRebuildFromModule,
  replaceRuntimeSessionInConfig as replaceRuntimeSessionInConfigFromModule,
  runtimeRebuildAffectedIds,
  type ServiceRuntimeRebuildApplyResult,
  type ServiceRuntimeRebuildMcpRefreshResult,
  type ServiceRuntimeRebuildPlan,
  type ServiceRuntimeReplacementConfigPlan,
  type ServiceRuntimeReplacementSessionResult,
} from "./service-runtime-rebuild.js";
import { handleStorageQueryRequest } from "./storage-query-catalog.js";
import { buildAdminProfileRegistryDiagnostics } from "./profile-registry-admin.js";
import { effectiveToolSelectionForResourceLimits } from "./tool-profile-selection.js";
import {
  buildAdapterDiagnosticsProjection,
  type ChannelAdapterBindingDiagnostics,
  type ChannelProjectionFailureRecord,
  type AdapterDiagnosticsProjection,
} from "./adapter-diagnostics.js";
import {
  activeDenChannelBindings,
  connectDenSuccessorGateway as connectDenSuccessorGatewayFromModule,
  denConversationChannelActivityDiagnostics as denConversationChannelActivityDiagnosticsFromModule,
  drainTelegramOutboundMessages as drainTelegramOutboundMessagesFromModule,
  ensureDenConversationChannels as ensureDenConversationChannelsFromModule,
  recordDynamicDenDeliveryChannel as recordDynamicDenDeliveryChannelFromModule,
  restartTelegramConnector as restartTelegramConnectorFromModule,
  projectTelegramDiplomatWakeReplies,
  startDenObservationProjection as startDenObservationProjectionFromModule,
  startTelegramConnector as startTelegramConnectorFromModule,
  stopTelegramConnector as stopTelegramConnectorFromModule,
  telegramChannelActivityDiagnostics as telegramChannelActivityDiagnosticsFromModule,
  type ServiceAdapterLifecycleContext,
} from "./service-adapter-lifecycle.js";
import { buildBackgroundServiceDiagnosticsProjection } from "./background-service-diagnostics.js";
import type { DirectDebugServiceContext } from "./direct-debug-service.js";
import { handleServiceDirectDebugRequest } from "./service-direct-debug-routes.js";
import {
  contextStrategyCatalog,
  contextStrategyDescriptor,
  contextStrategyPolicyFromUnknown,
  defaultContextStrategyPolicy,
  prepareContextStrategyRoleAssembly,
  type ContextStrategyPolicy,
} from "./context-strategy.js";
import {
  loadProfileConfig,
  loadProfileContext,
  loadProfileCuratorDiscoveryContext,
  type ProfileConfig,
  type SessionMemoryPromptConfig,
} from "./profile-loading.js";
import { loadServiceProfileContext } from "./service-profile-context.js";
import {
  buildProfileRoleAssembly,
  renderSessionMemoryContext,
} from "./profile-role-assembly.js";
import {
  buildRuntimeDiagnosticsProjection,
  type ToolDiagnosticsProjection,
  type RuntimeSessionEffectiveDefaults,
  type RuntimePauseDiagnostics,
  type RuntimeResponsesWakeMetrics,
  type StorageDiagnosticsProjection,
} from "./runtime-diagnostics.js";
import {
  type ChatEvent,
  type ConversationBranchStateRecord,
} from "./rusty-view-chat-api.js";
import {
  handleRustyViewChatRouteRequest,
  isChatRoute,
  type ChatStreamSubscriber,
} from "./service-chat-stream-routes.js";
import {
  appendChatEvent as appendChatEventFromModule,
  chatSubscribers as chatSubscribersFromModule,
  listChatEventsAfterCursor as listChatEventsAfterCursorFromModule,
  nativeChatEventToChatEvent,
  type ChatEventLogContext,
} from "./service-chat-event-log.js";
import { reconcileInterruptedChatTurns } from "./service-chat-restart-reconciliation.js";
import {
  createRustyViewAttachment,
  createRustyViewConversationBranch,
  createRustyViewConversationSnapshot,
  createRustyViewDataBankScope,
  createRustyViewMessageSlot,
  createRustyViewMessageVariant,
  deleteRustyViewMessageVariant,
  generateRoleplayAssistantAlternativeViaWake,
  getRustyViewConversationBranchState,
  listRustyViewAttachments,
  listRustyViewDataBankScopes,
  listRustyViewMessageSlots,
  listRustyViewMessageVariants,
  queryRustyViewChatSessionSummaries,
  readRustyViewChatSession,
  removeRustyViewAttachment,
  removeRustyViewDataBankScope,
  reorderRustyViewMessageVariants,
  resolveRustyViewConversationJump,
  rustyViewConversationTree,
  rustyViewProviderRequestDebugDetail,
  rustyViewSessionContextUsage,
  rustyViewToolCallDebugDetail,
  searchRustyViewTranscript,
  selectRustyViewActiveConversationBranch,
  selectRustyViewActiveMessageVariant,
  submitRustyViewChatMessage,
  updateRustyViewConversationBranchHead,
  type RustyViewChatOperationsContext,
} from "./service-rusty-view-chat-operations.js";
import {
  isBrowserCorsRoute,
  matchServiceApiRoute,
} from "./service-route-table.js";
import { ServiceExternalRuntimeController } from "./service-external-runtime.js";
import { ToolMediaAttachmentStore } from "./tool-media-attachments.js";
import { handleServiceImageGenerationRequest } from "./service-image-generation-routes.js";
import { createImageGenerationRuntime } from "./image-generation.js";
import { handleExternalRuntimeRequest } from "./service-external-runtime-routes.js";
import { handleCoordinationOperatorRequest } from "./service-coordination-operator-routes.js";
import {
  controlUrlForSlashCommand,
  executeRustyViewChatCommand,
  type RustyViewSlashCommandContext,
} from "./service-rusty-view-chat-commands.js";
import {
  buildRuntimeHealthProjection,
  type RuntimeHealthProjection,
} from "./runtime-health.js";
import {
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
import {
  runCuratorLifecycleTransitions,
  type CuratorLifecyclePlanner,
  type CuratorLifecycleReport,
} from "./curator-lifecycle.js";
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
  MemoryCuratorGovernanceStore,
  NativeCuratorGovernanceStore,
  rollbackCuratorMutation,
  type CuratorGovernancePlanner,
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
import type { ExternalMemoryReadiness } from "./external-memory-readiness.js";
import { createServiceExternalMemoryReadiness } from "./service-external-memory-readiness.js";
import {
  executeScheduledHostRun,
  runScheduledHostExecutors,
  scheduledHostJobKinds,
} from "./scheduled-host-executors.js";
import {
  recordSchedulerHeartbeatFailure as recordSchedulerHeartbeatFailureFromModule,
  runSchedulerHeartbeat as runSchedulerHeartbeatFromModule,
  runServiceCuratorLifecycleTransitions as runServiceCuratorLifecycleTransitionsFromModule,
  type SchedulerBackgroundContext,
} from "./service-scheduler-background.js";
import {
  drainAndDispatchWakes as drainAndDispatchWakesFromModule,
  suppressNextWakeEvent as suppressNextWakeEventFromModule,
  type WakeEventDrainContext,
} from "./service-wake-event-drain.js";
import {
  buildBuiltInToolCatalog,
  defaultToolRegistry,
} from "./tool-registry.js";
import type { ServiceAdapterFactories } from "./service-adapter-ports.js";
import {
  closeAllServiceBrowserSessionsForLifecycle,
  closeServiceBrowserSessionForLifecycle,
  createServiceBrowserResources,
  type ServiceBrowserResources,
} from "./service-browser-resources.js";
import {
  appendCoreEventsToChatLog,
  completionPacketProjectionMetadata,
  dispatchWake as dispatchWakeFromModule,
  observeWakeEvents,
  runtimePauseSummary,
  runtimePauseWakeReport as runtimePauseWakeReportFromModule,
  type ServiceWakeDispatchContext,
  type ServiceWakeDispatchReport,
  type ServiceWakeObservationContext,
  type ServiceWakeSource,
  type WakeProfileContext,
} from "./service-wake-dispatch.js";
import {
  createDenGatewayObservationSink,
  persistSessionActivityDigest as persistSessionActivityDigestFromModule,
  publishWakeToolActivity as publishWakeToolActivityFromModule,
  runPostTurnMaintenance as runPostTurnMaintenanceFromModule,
  type ServiceWakeMaintenanceContext,
} from "./service-wake-maintenance.js";
import {
  scheduledHostExecutorContext as scheduledHostExecutorContextFromModule,
  type ServiceBackgroundReviewContext,
} from "./service-background-review.js";
import { AgentActivityObservationProducer } from "./agent-activity-observation.js";
import {
  publishCuratorActivityObservation,
  type CuratorActivityReceipt,
} from "./curator-observation.js";
import {
  createServiceReviewSubmissionRuntime,
  assertExpectedDeploymentRole,
  getExternalReviewStatus,
  parseExternalReviewRecoveryRequest,
  parseExternalReviewSubmissionRequest,
  recoverExternalReviewDispatch,
  reconcileReviewSubmissions,
  ReviewSubmissionAdapterError,
  submitExternalReview,
  type ServiceReviewSubmissionContext,
} from "./service-review-submission.js";
import {
  validateServiceReviewDenAuthority,
  type ReviewDenAuthorityDiagnostics,
} from "./service-review-den-authority.js";
import { runManualContextCompaction } from "./manual-compaction.js";

export {
  isManualCompactionDuplicate,
  manualCompactionArtifactEffectiveFingerprint,
  manualCompactionEffectiveFingerprint,
} from "./manual-compaction.js";

export interface RustyCrewServiceAppOptions {
  env?: RustyCrewServiceEnv;
  config?: RustyCrewServiceConfig;
  bridge?: NativeBridgeModule;
  adapterFactories: ServiceAdapterFactories;
  toolCallDebugStore?: ToolCallDebugStore;
  browserResources?: ServiceBrowserResources;
  now?: () => string;
}

export interface RustyCrewServiceApp {
  readonly config: RustyCrewServiceConfig;
  readonly bridge: NativeBridgeModule;
  readonly engine: EngineHandle;
  readonly backgroundLoops: ServiceBackgroundLoopPort;
  readonly adminHost: string;
  readonly adminPort: number;
  readonly url: string;
  handle(request: IncomingMessage, response: ServerResponse): void;
  stop(): Promise<void>;
}

interface ServiceState {
  readonly config: RustyCrewServiceConfig;
  reviewDenAuthorityDiagnostics: ReviewDenAuthorityDiagnostics;
  readonly bridge: NativeBridgeModule;
  readonly engine: EngineHandle;
  readonly lock: RustyCrewServiceLock;
  readonly runtimeConfigMutationQueue: AsyncMutationQueue;
  readonly auditSink: ReturnType<typeof createMemoryAdminControlAuditSink>;
  readonly adapterFactories: ServiceAdapterFactories;
  readonly externalMemoryReadiness: ExternalMemoryReadiness;
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
  readonly telegramDiplomatPendingReplies: ServiceAdapterLifecycleContext["telegramDiplomatPendingReplies"];
  telegramDiplomatReplyProjectionRunning: boolean;
  profileChannelWakePolicies: Map<string, ChannelWakePolicy>;
  mcpManager: McpSurfaceManagerPort;
  readonly wakeSubscription: SubscriptionHandle;
  readonly timers: Set<NodeJS.Timeout>;
  readonly inFlightWakes: Set<SessionId>;
  readonly deferredWakeSessions: Set<SessionId>;
  readonly deferredRuntimeActivitySettlements: DeferredRuntimeActivitySettlementQueue;
  readonly runtimePauses: Map<string, RuntimePauseRecord>;
  readonly claimedDeliveryIntentIds: Set<number>;
  readonly unmatchedDeliveryIntentIds: Set<number>;
  readonly directDispatchSessions: Set<SessionId>;
  readonly chatSubscribersBySession: Map<SessionId, Set<ChatStreamSubscriber>>;
  readonly toolCallDebugStore: ToolCallDebugStore;
  readonly providerRequestDebugStore: ProviderRequestDebugStore;
  readonly toolMediaAttachments: ToolMediaAttachmentStore;
  readonly browserResources: ServiceBrowserResources;
  readonly externalRuntimeController: ServiceExternalRuntimeController;
  readonly diagnosticsContextCache: Map<
    string,
    { readonly expiresAt: number; readonly value: AdminDiagnosticsContext }
  >;
  readonly diagnosticsContextInFlight: Map<
    string,
    Promise<AdminDiagnosticsContext>
  >;
  readonly responsesWakeMetrics: RuntimeResponsesWakeMetrics[];
  readonly suppressedWakeEvents: Map<SessionId, number>;
  readonly recentEvents: ServiceRecentEvent[];
  schedulerHeartbeat: ServiceSchedulerHeartbeatState;
  readonly now: () => string;
  nextWakeSequence: number;
  stopping: boolean;
}

interface AsyncMutationQueue {
  tail: Promise<void>;
}

function createAsyncMutationQueue(): AsyncMutationQueue {
  return { tail: Promise.resolve() };
}

async function withAsyncMutationQueue<T>(
  queue: AsyncMutationQueue,
  operation: () => Promise<T>,
): Promise<T> {
  const previous = queue.tail;
  let release!: () => void;
  queue.tail = new Promise<void>((resolve) => {
    release = resolve;
  });
  await previous.catch(() => undefined);
  try {
    return await operation();
  } finally {
    release();
  }
}

function roleplayRouteContext(state: ServiceState): RoleplayRouteContext {
  return {
    bridge: state.bridge,
    runtimeConfig: { profilesDir: state.runtimeConfig.profilesDir },
    now: state.now,
    applyServiceRuntimeConfigFromDisk: (options) =>
      applyServiceRuntimeConfigFromDisk(state, options),
    rebuildBrainRuntime: async (profileId) => {
      await rebuildServiceBrainRuntime(state, profileId);
    },
    serviceSessionById: (sessionId) => serviceSessionById(state, sessionId),
    listChatEventsAfterCursor: (session, afterCursor, limit) =>
      listChatEventsAfterCursorFromModule(
        chatEventLogContext(state),
        session,
        afterCursor,
        limit,
      ),
    generateRoleplayAssistantAlternative: (input) =>
      generateRoleplayAssistantAlternativeViaWake(
        rustyViewChatOperationsContext(state),
        input,
      ),
  };
}

function profileRuntimeMutationContext(state: ServiceState) {
  return {
    bridge: state.bridge,
    get runtimeConfig() {
      return state.runtimeConfig;
    },
    serviceConfigFile: state.config.paths.serviceConfigFile,
    now: state.now,
    applyRuntimeConfigFromDisk: (options: {
      createMissingSessions: boolean;
      eventType: string;
      summaryPrefix: string;
    }) => applyServiceRuntimeConfigFromDisk(state, options),
    rebuildBrainRuntime: async (profileId: string) => {
      await rebuildServiceBrainRuntime(state, profileId as ProfileId);
    },
  };
}

function profileAdminMutationContext(
  state: ServiceState,
): ServiceProfileAdminMutationContext {
  return {
    bridge: state.bridge,
    runtimeConfig: state.runtimeConfig,
    serviceConfigFile: state.config.paths.serviceConfigFile,
    now: state.now,
    inFlightWakes: state.inFlightWakes,
    applyRuntimeConfigFromDisk: (options) =>
      applyServiceRuntimeConfigFromDisk(state, options),
    archiveSession: (sessionId) => archiveServiceSession(state, sessionId),
    forgetPurgedSessions: (sessionIds) => {
      for (const sessionId of sessionIds) {
        state.directDispatchSessions.delete(sessionId as SessionId);
        state.chatSubscribersBySession.delete(sessionId as SessionId);
        state.suppressedWakeEvents.delete(sessionId as SessionId);
      }
    },
  };
}

function crewSessionLifecycleContext(
  state: ServiceState,
): CrewSessionLifecycleContext {
  return {
    bridge: state.bridge,
    runtimeConfig: state.runtimeConfig,
    serviceConfigFile: state.config.paths.serviceConfigFile,
    inFlightWakes: state.inFlightWakes,
    now: state.now,
    readRuntimeConfigFile: () =>
      readRuntimeConfigFileForMutationFromModule(
        profileAdminMutationContext(state),
      ),
    validateRuntimeConfigFile: (value) =>
      planRuntimeConfigFileValue(
        profileAdminMutationContext(state),
        isRecord(value) ? value : {},
      ),
    writeRuntimeConfigFile: (value) =>
      writeJsonFileAtomicFromModule(
        state.config.paths.serviceConfigFile,
        value,
      ),
    applyRuntimeConfigFromDisk: (options) =>
      applyServiceRuntimeConfigFromDisk(state, options),
    sessionById: (sessionId) => serviceSessionById(state, sessionId),
    appendChatEvent: (sessionId, event) =>
      appendChatEventFromModule(chatEventLogContext(state), sessionId, event),
  };
}

function runtimeRebuildContext(state: ServiceState) {
  return {
    bridge: state.bridge,
    get runtimeConfig() {
      return state.runtimeConfig;
    },
    get runtimeConfigApplyResult() {
      return state.runtimeConfigApplyResult;
    },
    inFlightWakes: state.inFlightWakes,
    now: state.now,
    nextReplacementSessionId: (
      session: Pick<SessionState, "agentId" | "sessionId">,
    ) => {
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
    },
    readRuntimeConfigFile: () =>
      readRuntimeConfigFileForMutationFromModule(
        profileAdminMutationContext(state),
      ),
    validateRuntimeConfigFile: (value: unknown) =>
      planRuntimeConfigFileValue(
        profileAdminMutationContext(state),
        isRecord(value) ? value : {},
      ),
    writeRuntimeConfigFile: (value: unknown) =>
      writeJsonFileAtomicFromModule(
        state.config.paths.serviceConfigFile,
        value,
      ),
    serviceSessionById: (sessionId: string) =>
      serviceSessionById(state, sessionId),
    archiveSession: (sessionId: SessionId) =>
      archiveServiceSession(state, sessionId),
    applyRuntimeConfigFromDisk: (options: {
      createMissingSessions: boolean;
      eventType: string;
      summaryPrefix: string;
    }) => applyServiceRuntimeConfigFromDisk(state, options),
    rebuildBrainRuntime: (profileId: ProfileId) =>
      rebuildServiceBrainRuntime(state, profileId),
    refreshMcpBindingsAfterRuntimeRebuild: (
      bindingIds: readonly string[],
      command: AdminControlCommand,
    ) => refreshMcpBindingsAfterRuntimeRebuild(state, bindingIds, command),
    recordEvent: (event: {
      source: string;
      eventType: string;
      summary: string;
      severity?: "info" | "warning" | "error";
      workRef?: Record<string, unknown>;
      resultRef?: Record<string, unknown>;
    }) => recordServiceEvent(state, event),
    recordDurableTransition: async (sessionId: string, transition: unknown) => {
      await appendChatEventFromModule(
        chatEventLogContext(state),
        sessionId as SessionId,
        {
          kind: "runtime_rebuild_transition",
          payload: isRecord(transition) ? transition : {},
        },
      );
    },
  };
}

function adapterLifecycleContext(
  state: ServiceState,
): ServiceAdapterLifecycleContext {
  return {
    config: state.config,
    bridge: state.bridge,
    adapterFactories: state.adapterFactories,
    get runtimeConfig() {
      return state.runtimeConfig;
    },
    get denGatewayClient() {
      return state.denGatewayClient;
    },
    get denObservationSubscription() {
      return state.denObservationSubscription;
    },
    set denObservationSubscription(subscription) {
      state.denObservationSubscription = subscription;
    },
    get telegramConnector() {
      return state.telegramConnector;
    },
    set telegramConnector(connector) {
      state.telegramConnector = connector;
    },
    get telegramOutboundSubscription() {
      return state.telegramOutboundSubscription;
    },
    set telegramOutboundSubscription(subscription) {
      state.telegramOutboundSubscription = subscription;
    },
    timers: state.timers,
    denConversationChannelResolutionsByBindingId:
      state.denConversationChannelResolutionsByBindingId,
    denConversationChannelIdsByExternalId:
      state.denConversationChannelIdsByExternalId,
    denConversationMembershipsByBindingId:
      state.denConversationMembershipsByBindingId,
    dynamicDenChannelBindings: state.dynamicDenChannelBindings,
    channelProjectionFailures: state.channelProjectionFailures,
    telegramDiplomatPendingReplies: state.telegramDiplomatPendingReplies,
    get telegramDiplomatReplyProjectionRunning() {
      return state.telegramDiplomatReplyProjectionRunning;
    },
    set telegramDiplomatReplyProjectionRunning(running) {
      state.telegramDiplomatReplyProjectionRunning = running;
    },
    now: state.now,
    isStopping: () => state.stopping,
    recordEvent: (event) => recordServiceEvent(state, event),
    drainSubscriptionEventsUntilIdle: (subscription) =>
      drainSubscriptionEventsUntilIdle(state.bridge, subscription),
    createObservationSink: (client) => createDenGatewayObservationSink(client),
    ensureSessionForChannelBinding: ({ binding }) =>
      ensureConfiguredSessionForChannelBinding({
        bridge: state.bridge,
        runtimeConfig: state.runtimeConfig,
        binding,
      }),
    channelWakePolicyForSession: (session) =>
      channelWakePolicyForSession(state, session),
    persistTelegramMedia: (input) =>
      state.toolMediaAttachments.persistExternalChannelAttachment({
        ...input,
        provider: "telegram",
      }),
  };
}

function chatEventLogContext(state: ServiceState): ChatEventLogContext {
  return {
    bridge: state.bridge,
    chatSubscribersBySession: state.chatSubscribersBySession,
    now: state.now,
  };
}

function rustyViewChatOperationsContext(
  state: ServiceState,
): RustyViewChatOperationsContext {
  return {
    bridge: state.bridge,
    get runtimeConfig() {
      return state.runtimeConfig;
    },
    toolCallDebugStore: state.toolCallDebugStore,
    providerRequestDebugStore: state.providerRequestDebugStore,
    toolMediaAttachments: state.toolMediaAttachments,
    now: state.now,
    appendChatEvent: (sessionId, event) =>
      appendChatEventFromModule(chatEventLogContext(state), sessionId, event),
    listChatEventsAfterCursor: (session, cursor, limit) =>
      listChatEventsAfterCursorFromModule(
        chatEventLogContext(state),
        session,
        cursor,
        limit,
      ),
    roleplayRouteContext: () => roleplayRouteContext(state),
    submitServiceTurn: (input) => submitServiceTurn(state, input),
    resolveModelProviderForBrain: (alias) =>
      resolveModelProviderForBrain(state.bridge, alias),
  };
}

async function listProjectedServiceSessions(
  state: ServiceState,
): Promise<SessionState[]> {
  return state.bridge.listSessions();
}

function rustyViewSlashCommandContext(
  state: ServiceState,
): RustyViewSlashCommandContext {
  return {
    appendChatEvent: (sessionId, event) =>
      appendChatEventFromModule(chatEventLogContext(state), sessionId, event),
    buildDiagnosticsContext: () => buildDiagnosticsContext(state),
    sessionContextUsage: (input) =>
      rustyViewSessionContextUsage(
        rustyViewChatOperationsContext(state),
        input,
      ),
    executeControlCommand: async (input) => {
      const control = await handleAdminControlRequest(
        {
          method: "POST",
          url: controlUrlForSlashCommand(input.commandName, input.sessionId),
          headers: {
            authorization: `Bearer ${controlBearerToken(state.config)}`,
            "x-rusty-crew-operator": input.actorId,
          },
          body: input.body,
          requestId: input.requestId,
        },
        {
          auth: {
            bearerToken: controlBearerToken(state.config),
            operatorId: input.actorId,
          },
          auditSink: state.auditSink,
          executor: createServiceControlExecutor(state),
          now: state.now,
        },
      );
      if (control.body.ok) {
        const data = control.body.data as AdminControlResponse;
        return { controlStatus: control.status, outcome: data.outcome };
      }
      return {
        controlStatus: control.status,
        outcome: {
          status: "failed",
          summary: control.body.error.message,
          reasonCode: control.body.error.reason_code,
        },
      };
    },
  };
}

function wakeEventDrainContext(
  state: ServiceState,
  source: ServiceWakeSource,
  observationContext?: ServiceWakeObservationContext,
): WakeEventDrainContext<ServiceWakeDispatchReport> {
  return {
    bridge: state.bridge,
    wakeSubscription: state.wakeSubscription,
    suppressedWakeEvents: state.suppressedWakeEvents,
    dispatchWake: (event) =>
      dispatchWake(state, event, source, observationContext),
  };
}

function schedulerBackgroundContext(
  state: ServiceState,
): SchedulerBackgroundContext {
  return {
    bridge: state.bridge,
    get runtimeConfig() {
      return state.runtimeConfig;
    },
    schedulerHeartbeat: state.schedulerHeartbeat,
    curator: state.curator,
    now: state.now,
    isStopping: () => state.stopping,
    curatorSkillsDir,
    scheduledHostExecutorContext: () => scheduledHostExecutorContext(state),
    reconcileDeferredRuntimeActivitySettlements: () =>
      reconcileDeferredRuntimeActivitySettlements(state),
    recordEvent: (event) => recordServiceEvent(state, event),
  };
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
  readonly store: NativeCuratorGovernanceStore;
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
  workRef?: Record<string, unknown>;
  resultRef?: Record<string, unknown>;
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
    let liveState: ServiceState | undefined;
    const toolMediaAttachments = new ToolMediaAttachmentStore({
      artifactDir: config.paths.artifactDir,
      bridge,
      now: options.now ?? (() => new Date().toISOString()),
      appendChatEvent: async (sessionId, event) => {
        if (liveState === undefined) {
          throw new Error("service state is not ready for tool media events");
        }
        return appendChatEventFromModule(
          chatEventLogContext(liveState),
          sessionId,
          event,
        );
      },
    });
    const curator = await createServiceCuratorRuntime({
      config,
      runtimeConfig,
      bridge,
      now: options.now ?? (() => new Date().toISOString()),
      publishActivity: async (receipt) => {
        if (liveState) {
          await publishServiceCuratorActivity(liveState, receipt);
        }
      },
    });
    const toolCallDebugStore =
      options.toolCallDebugStore ??
      new MemoryToolCallDebugStore({
        now: options.now,
      });
    const providerRequestDebugStore = new MemoryProviderRequestDebugStore({
      now: options.now,
    });
    const browserResources =
      options.browserResources ??
      createServiceBrowserResources({
        resourcePolicy: await bridge.planWebBrowserResourcePolicy({}),
        bridge,
      });
    const externalRuntimeController = new ServiceExternalRuntimeController({
      bridge,
      mediaCaptureSink: toolMediaAttachments,
      documentCaptureSink: toolMediaAttachments,
      resolveInputImage: (sessionId, storageUrl) =>
        toolMediaAttachments.resolveExternalInputImage(sessionId, storageUrl),
      now: () => new Date((options.now ?? (() => new Date().toISOString()))()),
      onCoordinationDelivery: async (receipt) => {
        const state = liveState;
        if (state === undefined) return receipt;
        const settled =
          await state.externalRuntimeController.applyCoordinationDelivery(
            receipt,
          );
        await drainAndDispatchWakesFromModule(
          wakeEventDrainContext(state, "external_runtime"),
        ).catch((error) =>
          recordServiceEvent(state, {
            source: "external-runtime-controller",
            eventType: "wake_dispatch_failed",
            severity: "error",
            summary: errorMessage(
              error,
              "external runtime wake dispatch failed",
            ),
          }),
        );
        return settled;
      },
      onReviewSubmission: async (input) => {
        const state = liveState;
        if (state === undefined) {
          throw new Error("service review submission runtime is not ready");
        }
        return createServiceReviewSubmissionRuntime(() =>
          reviewSubmissionContext(state),
        ).submit(input);
      },
      onReviewCompletion: async (input) => {
        const state = liveState;
        if (state === undefined) {
          throw new Error("service review completion runtime is not ready");
        }
        return createServiceReviewSubmissionRuntime(() =>
          reviewSubmissionContext(state),
        ).complete(input);
      },
    });
    const externalMemoryReadiness = createServiceExternalMemoryReadiness(
      config,
      options.adapterFactories,
    );
    const runtimeConfigApplyResult = await applyRustyCrewRuntimeConfig({
      serviceConfig: config,
      runtimeConfig,
      bridge,
      curatorExecutor: curator.executor,
      mcpSurfaceDiagnostics: mcpManager.diagnostics(),
      adapterFactories: options.adapterFactories,
      externalMemoryReadiness,
      coordinationRuntime: createServiceCoordinationRuntime(() => liveState),
      reviewSubmissionRuntime: createServiceReviewSubmissionRuntime(() =>
        liveState === undefined
          ? undefined
          : reviewSubmissionContext(liveState),
      ),
      toolCallDebugStore,
      providerRequestDebugStore,
      browserResources,
      toolMediaSink: toolMediaAttachments,
      narratorImageContextResolver: toolMediaAttachments,
      onBrainWakeResult: (observation) => {
        const state = liveState;
        if (state === undefined) return;
        recordResponsesWakeMetrics(state, observation);
      },
    });
    const wakeSubscription = await bridge.subscribeEvents({
      eventKinds: ["brain_wake_requested", "session_archived"],
    });

    const state: ServiceState = {
      config,
      reviewDenAuthorityDiagnostics: {
        serverName: "den",
        status: "unconfigured",
        requiredTools: [],
        missingTools: [],
        checkedAt: (options.now ?? (() => new Date().toISOString()))(),
        message: "Dedicated service review Den authority has not been checked.",
      },
      bridge,
      engine,
      lock,
      runtimeConfigMutationQueue: createAsyncMutationQueue(),
      auditSink: createMemoryAdminControlAuditSink(),
      adapterFactories: options.adapterFactories,
      externalMemoryReadiness,
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
      telegramDiplomatPendingReplies: new Map(),
      telegramDiplomatReplyProjectionRunning: false,
      profileChannelWakePolicies,
      curator,
      backgroundReview: createServiceBackgroundReviewRuntime(runtimeConfig),
      mcpManager,
      wakeSubscription,
      timers: new Set(),
      inFlightWakes: new Set(),
      deferredWakeSessions: new Set(),
      deferredRuntimeActivitySettlements:
        new DeferredRuntimeActivitySettlementQueue(),
      runtimePauses: new Map(),
      claimedDeliveryIntentIds: new Set(),
      unmatchedDeliveryIntentIds: new Set(),
      directDispatchSessions: new Set(),
      chatSubscribersBySession: new Map(),
      toolCallDebugStore,
      providerRequestDebugStore,
      toolMediaAttachments,
      browserResources,
      externalRuntimeController,
      diagnosticsContextCache: new Map(),
      diagnosticsContextInFlight: new Map(),
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
    await refreshReviewDenAuthorityDiagnostics(state);
    const chatRestartReconciliation = await reconcileInterruptedChatTurns({
      bridge: state.bridge,
      now: state.now,
    });
    if (chatRestartReconciliation.sessionsReconciled.length > 0) {
      recordServiceEvent(state, {
        source: "service-host",
        eventType: "interrupted_chat_turns_reconciled",
        severity: "warning",
        summary: `Reconciled ${chatRestartReconciliation.sessionsReconciled.length} interrupted chat turn(s) after service restart.`,
        resultRef: {
          sessionIds: chatRestartReconciliation.sessionsReconciled,
          eventsAppended: chatRestartReconciliation.eventsAppended,
        },
      });
    }
    const requeuedLogicalTurns =
      await state.bridge.requeueLogicalTurnContinuations();
    if (requeuedLogicalTurns > 0) {
      recordServiceEvent(state, {
        source: "service-host",
        eventType: "logical_turn_continuations_requeued",
        summary: `Requeued ${requeuedLogicalTurns} durable logical turn continuation(s) after event subscription startup.`,
      });
    }
    await state.externalRuntimeController.start();
    state.denGatewayStartupReport = await connectDenSuccessorGatewayFromModule(
      adapterLifecycleContext(state),
    );
    await startDenObservationProjectionFromModule(
      adapterLifecycleContext(state),
    );
    await ensureDenConversationChannelsFromModule(
      adapterLifecycleContext(state),
    );
    await startTelegramConnectorFromModule(adapterLifecycleContext(state));
    await reconcileReviewSubmissions(reviewSubmissionContext(state));
    const backgroundLoops: ServiceBackgroundLoopPort = {
      intervals: {
        schedulerTickIntervalMs:
          state.config.background.schedulerTickIntervalMs,
        wakeDispatchIntervalMs: state.config.background.wakeDispatchIntervalMs,
        denRuntimeHeartbeatIntervalMs:
          state.config.background.denRuntimeHeartbeatIntervalMs,
        denDeliveryPollIntervalMs:
          state.config.background.denDeliveryPollIntervalMs,
        telegramOutboundDrainIntervalMs: state.config.telegram.pollIntervalMs,
        externalRuntimeControllerTickIntervalMs: 5_000,
      },
      denGatewayAvailable: state.denGatewayClient !== undefined,
      telegramConnectorAvailable: state.telegramConnector !== undefined,
      callbacks: {
        runSchedulerHeartbeat: () =>
          runSchedulerHeartbeatFromModule(schedulerBackgroundContext(state)),
        recordSchedulerHeartbeatFailure: (error) =>
          recordSchedulerHeartbeatFailureFromModule(
            schedulerBackgroundContext(state),
            error,
          ),
        drainAndDispatchWakes: async () => {
          const reports = await drainAndDispatchWakesFromModule(
            wakeEventDrainContext(state, "background"),
          );
          await projectTelegramDiplomatWakeReplies(
            adapterLifecycleContext(state),
            reports,
          );
          return reports;
        },
        heartbeatDenRuntimeInstances: () => heartbeatDenRuntimeInstances(state),
        pollDenDeliveryIntents: () => pollDenDeliveryIntents(state),
        drainTelegramOutboundMessages: () =>
          drainTelegramOutboundMessagesFromModule(
            adapterLifecycleContext(state),
          ),
        tickExternalRuntimeController: async () => {
          await state.externalRuntimeController.tick();
          await reconcileReviewSubmissions(reviewSubmissionContext(state));
        },
        recordFailure: (failureRecord) =>
          recordServiceEvent(state, failureRecord),
        errorMessage,
      },
    };

    return {
      config,
      bridge,
      engine,
      backgroundLoops,
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
    return {
      backend: "sqlite",
      filesystemWarningFreePercent: storage.filesystemWarningFreePercent,
    };
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
    backingFilesystemPath: storage.postgres.backingFilesystemPath,
    filesystemWarningFreePercent: storage.filesystemWarningFreePercent,
  };
}

async function handleHttpRequest(
  request: IncomingMessage,
  state: ServiceState,
): Promise<ServiceRouteResult> {
  const url = new URL(request.url ?? "/", "http://rusty-crew.local");
  const beforeAuthRoute = matchServiceApiRoute(url.pathname, "before_auth");
  if (beforeAuthRoute?.id === "admin.healthz") {
    return handleAdminDiagnosticsRequest(
      {
        method: request.method ?? "GET",
        url: url.toString(),
        requestId: requestId(request),
      },
      await buildDiagnosticsContext(state, {
        includeProfileRegistry: isProfileRegistryAdminRoute(url.pathname),
        includeStorageDiagnostics: includeStorageDiagnosticsForAdminPath(
          url.pathname,
        ),
        curatorUrl: isCuratorAdminReadRoute(url.pathname) ? url : undefined,
      }),
    );
  }

  const corsRoute = matchServiceApiRoute(url.pathname, "cors_preflight");
  if (
    corsRoute?.id === "browser.cors" &&
    (request.method ?? "GET").toUpperCase() === "OPTIONS"
  ) {
    return chatCorsPreflightResponse(request);
  }

  if (!isAuthorized(request, state.config.admin.token, state.config)) {
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
        : isBrowserCorsRoute(url.pathname)
          ? withChatCors(unauthorized, request)
          : unauthorized;
  }

  const route = matchServiceApiRoute(url.pathname, "after_auth");

  if (url.pathname === "/v1/admin/diagnostics/review-submissions") {
    if ((request.method ?? "GET").toUpperCase() !== "GET") {
      return failure(405, requestId(request), {
        code: "method_not_allowed",
        reason_code: "review_submission_method_not_allowed",
        message: "review submission diagnostics support GET only",
        retryable: false,
      });
    }
    return successRoute(
      requestId(request),
      await state.bridge.listReviewSubmissions({ pendingOnly: false }),
    );
  }

  if (url.pathname === "/v1/admin/diagnostics/review-den-authority") {
    if ((request.method ?? "GET").toUpperCase() !== "GET") {
      return failure(405, requestId(request), {
        code: "method_not_allowed",
        reason_code: "review_den_authority_method_not_allowed",
        message: "review Den authority diagnostics support GET only",
        retryable: false,
      });
    }
    return successRoute(
      requestId(request),
      state.reviewDenAuthorityDiagnostics,
    );
  }

  if (url.pathname === "/v1/admin/diagnostics/review-submission-scope") {
    if ((request.method ?? "GET").toUpperCase() !== "GET") {
      return failure(405, requestId(request), {
        code: "method_not_allowed",
        reason_code: "review_submission_scope_method_not_allowed",
        message: "review submission scope diagnostics support GET only",
        retryable: false,
      });
    }
    const submissions = await state.bridge.listReviewSubmissions({
      pendingOnly: false,
    });
    return successRoute(requestId(request), {
      management: "rusty_crew_managed",
      projectScope: "caller_supplied_den_project",
      directDenReviews: "not_tracked_by_rusty_crew",
      submissions: submissions.map((record) => ({
        submissionId: record.submissionId,
        projectId: String(record.projectId),
        taskId: Number(record.taskId),
        phase: record.phase,
        callerType: record.caller.type,
      })),
    });
  }

  if (route?.id === "admin.review_submissions.external") {
    const method = (request.method ?? "GET").toUpperCase();
    try {
      if (
        method === "POST" &&
        url.pathname === "/v1/admin/review-submissions"
      ) {
        const input = parseExternalReviewSubmissionRequest(
          await readJsonBody(request),
        );
        const receipt = await submitExternalReview(
          reviewSubmissionContext(state),
          input,
        );
        return successRoute(requestId(request), receipt);
      }
      if (
        method === "POST" &&
        url.pathname.endsWith("/recover") &&
        url.pathname.startsWith("/v1/admin/review-submissions/")
      ) {
        const encodedSubmissionId = url.pathname.slice(
          "/v1/admin/review-submissions/".length,
          -"/recover".length,
        );
        const submissionId = decodeURIComponent(encodedSubmissionId);
        if (!submissionId) {
          return failure(404, requestId(request), {
            code: "not_found",
            reason_code: "external_review_submission_not_found",
            message: "A review submission id is required.",
            retryable: false,
          });
        }
        const input = parseExternalReviewRecoveryRequest(
          await readJsonBody(request),
        );
        return successRoute(
          requestId(request),
          await recoverExternalReviewDispatch(
            reviewSubmissionContext(state),
            submissionId,
            input,
          ),
        );
      }
      if (
        method === "GET" &&
        url.pathname.startsWith("/v1/admin/review-submissions/")
      ) {
        assertExpectedDeploymentRole(
          reviewSubmissionContext(state),
          url.searchParams.get("expectedDeploymentRole") ?? undefined,
        );
        const submissionId = decodeURIComponent(
          url.pathname.slice("/v1/admin/review-submissions/".length),
        );
        if (!submissionId) {
          return failure(404, requestId(request), {
            code: "not_found",
            reason_code: "external_review_submission_not_found",
            message: "A review submission id is required.",
            retryable: false,
          });
        }
        return successRoute(
          requestId(request),
          await getExternalReviewStatus(
            reviewSubmissionContext(state),
            submissionId,
          ),
        );
      }
      return failure(405, requestId(request), {
        code: "method_not_allowed",
        reason_code: "external_review_submission_method_not_allowed",
        message:
          "External review submissions support POST collection, GET by submission id, and POST /{submissionId}/recover.",
        retryable: false,
      });
    } catch (error) {
      return externalReviewApiFailure(requestId(request), error);
    }
  }

  if (route?.id === "logical_turns") {
    const result = await handleLogicalTurnRoute(
      {
        method: request.method,
        url,
        body:
          (request.method ?? "GET").toUpperCase() === "POST"
            ? await readJsonBody(request)
            : undefined,
        requestId: requestId(request),
        idempotencyKey: headers(request)["idempotency-key"],
      },
      {
        logicalTurnDiagnostics: (query) =>
          state.bridge.logicalTurnDiagnostics(query),
        resolveLogicalTurnAttention: (input) =>
          state.bridge.resolveLogicalTurnAttention(input),
        cancelLogicalTurn: (input) => state.bridge.cancelLogicalTurn(input),
        appendChatLifecycleEvent: async ({ sessionId, kind, payload }) => {
          await appendChatEventFromModule(
            chatEventLogContext(state),
            sessionId as SessionId,
            {
              kind,
              payload,
            },
          );
        },
        now: state.now,
      },
    );
    return isChatRoute(url.pathname) ? withChatCors(result, request) : result;
  }

  if (route?.id === "admin.control") {
    const body = await readJsonBody(request);
    const result = await handleAdminControlRequest(
      {
        method: request.method ?? "POST",
        url: url.toString(),
        headers: controlHeaders(request, state.config),
        body,
        requestId: requestId(request),
      },
      {
        auth: {
          bearerToken: controlBearerToken(state.config),
          operatorId: "local-operator",
        },
        auditSink: state.auditSink,
        executor: createServiceControlExecutor(state),
        now: state.now,
      },
    );
    return result;
  }

  if (route?.id === "chat") {
    let chatEffectiveDefaults:
      | Promise<Map<SessionId, RuntimeSessionEffectiveDefaults>>
      | undefined;
    const effectiveDefaultsForChatSession = async (session: SessionState) => {
      chatEffectiveDefaults ??= state.bridge
        .listSessions()
        .then((sessions) => effectiveSessionDefaultsById(state, sessions));
      return chatEffectiveDefaults.then((defaults) => {
        const value = defaults.get(session.sessionId);
        return value === undefined ? undefined : { ...value };
      });
    };
    const chatOperations = rustyViewChatOperationsContext(state);
    return handleRustyViewChatRouteRequest(request, url, {
      stream: {
        listSessions: () => listProjectedServiceSessions(state),
        streamReplayEvents: (session, cursor, streamUrl) =>
          streamReplayEvents(chatOperations, session, cursor, streamUrl),
        subscribersForSession: (sessionId) =>
          chatSubscribersFromModule(chatEventLogContext(state), sessionId),
        deleteSubscribersForSession: (sessionId) =>
          state.chatSubscribersBySession.delete(sessionId),
        timers: state.timers,
        corsHeaders: (corsRequest) => chatCorsHeaders(corsRequest),
        readAttachmentContent: (sessionId, attachmentId) =>
          state.toolMediaAttachments.readContent(sessionId, attachmentId),
        uploadAttachmentContent: (input) =>
          state.toolMediaAttachments.persistUploadedImage(input),
      },
      chat: {
        listSessions: () => listProjectedServiceSessions(state),
        createSession: (input) =>
          withAsyncMutationQueue(state.runtimeConfigMutationQueue, () =>
            createFreshCrewSession(crewSessionLifecycleContext(state), {
              idempotencyKey: input.idempotencyKey,
              profileId: input.profileId as ProfileId,
              expectedProfileRevision: input.expectedProfileRevision,
              workspaceCwd: input.workspaceCwd,
              requestedAt: state.now(),
            }),
          ),
        effectiveSessionDefaults: effectiveDefaultsForChatSession,
        querySessionSummaries: (input) =>
          queryRustyViewChatSessionSummaries(chatOperations, input),
        readSession: (input) => readRustyViewChatSession(chatOperations, input),
        getToolCallDebugDetail: (input) =>
          rustyViewToolCallDebugDetail(chatOperations, input),
        getProviderRequestDebugDetail: (input) =>
          rustyViewProviderRequestDebugDetail(chatOperations, input),
        executeCommand: (input) =>
          executeRustyViewChatCommand(
            rustyViewSlashCommandContext(state),
            input,
          ),
        contextUsage: (input) =>
          rustyViewSessionContextUsage(chatOperations, input),
        manualContextCompaction: (input) =>
          runManualContextCompaction(
            {
              bridge: state.bridge,
              dispatch: wakeDispatchContext(state),
            },
            input,
          ),
        sendMessage: (input) =>
          submitRustyViewChatMessage(chatOperations, input),
        listMessageSlots: (input) =>
          listRustyViewMessageSlots(chatOperations, input),
        searchTranscript: (input) =>
          searchRustyViewTranscript(chatOperations, input),
        listMessageVariants: (input) =>
          listRustyViewMessageVariants(chatOperations, input),
        createMessageSlot: (input) =>
          createRustyViewMessageSlot(chatOperations, input),
        createMessageVariant: (input) =>
          createRustyViewMessageVariant(chatOperations, input),
        deleteMessageVariant: (input) =>
          deleteRustyViewMessageVariant(chatOperations, input),
        reorderMessageVariants: (input) =>
          reorderRustyViewMessageVariants(chatOperations, input),
        selectActiveMessageVariant: (input) =>
          selectRustyViewActiveMessageVariant(chatOperations, input),
        conversationTree: (input) =>
          rustyViewConversationTree(chatOperations, input),
        createConversationBranch: (input) =>
          createRustyViewConversationBranch(chatOperations, input),
        getConversationBranchState: (input) =>
          getRustyViewConversationBranchState(chatOperations, input),
        selectActiveConversationBranch: (input) =>
          selectRustyViewActiveConversationBranch(chatOperations, input),
        updateConversationBranchHead: (input) =>
          updateRustyViewConversationBranchHead(chatOperations, input),
        createConversationSnapshot: (input) =>
          createRustyViewConversationSnapshot(chatOperations, input),
        resolveConversationJump: (input) =>
          resolveRustyViewConversationJump(chatOperations, input),
        createAttachment: (input) =>
          createRustyViewAttachment(chatOperations, input),
        listAttachments: (input) =>
          listRustyViewAttachments(chatOperations, input),
        removeAttachment: (input) =>
          removeRustyViewAttachment(chatOperations, input),
        createDataBankScope: (input) =>
          createRustyViewDataBankScope(chatOperations, input),
        listDataBankScopes: (input) =>
          listRustyViewDataBankScopes(chatOperations, input),
        removeDataBankScope: (input) =>
          removeRustyViewDataBankScope(chatOperations, input),
        now: state.now,
      },
      readJsonBody,
      requestId,
      headers,
    });
  }

  if (route?.id === "admin.image_generation") {
    const method = (request.method ?? "GET").toUpperCase();
    return handleServiceImageGenerationRequest(
      {
        method,
        url,
        body: method === "POST" ? await readJsonBody(request) : undefined,
        requestId: requestId(request),
      },
      {
        runtime: () =>
          createImageGenerationRuntime(
            state.runtimeConfig.imageGeneration ?? {
              providers: [],
              presets: [],
            },
          ),
        listSessions: () => state.bridge.listSessions(),
        toolMediaAttachments: state.toolMediaAttachments,
      },
    );
  }

  if (route?.id === "external_runtime") {
    return withChatCors(
      await handleExternalRuntimeRequest(request, url, {
        bridge: state.bridge,
        controller: state.externalRuntimeController,
        startInterval: (callback, intervalMs) => {
          const timer = setInterval(callback, intervalMs);
          state.timers.add(timer);
          return timer;
        },
        stopInterval: (timer) => {
          clearInterval(timer);
          state.timers.delete(timer);
        },
        now: state.now,
        requestId,
        readJsonBody,
        corsHeaders: chatCorsHeaders,
      }),
      request,
    );
  }

  if (route?.id === "coordination_operator") {
    return handleCoordinationOperatorRequest(request, url, {
      bridge: state.bridge,
      deploymentRole: state.config.deploymentRole,
      now: state.now,
      requestId,
      readJsonBody,
      settleDelivery: (receipt) =>
        state.externalRuntimeController.applyCoordinationDelivery(receipt),
    });
  }

  if (route?.id === "debug") {
    return handleServiceDirectDebugRequest(request, url, {
      requestId,
      readJsonBody,
      listSessions: () => listProjectedServiceSessions(state),
      buildDirectDebugContext: () => buildDirectDebugContext(state),
      emitContextCompactionDebugEvents: (session, input) =>
        emitContextCompactionDebugEvents(state, session, input),
      providerRequestDebugDetail: (input) =>
        rustyViewProviderRequestDebugDetail(
          rustyViewChatOperationsContext(state),
          input,
        ),
    });
  }

  if (route?.id === "admin.scheduler") {
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

  if (route?.id === "admin.mcp.catalog") {
    return handleAdminMcpCatalogRequest(
      { method: request.method ?? "GET", requestId: requestId(request) },
      { config: state.config, runtimeConfig: state.runtimeConfig },
    );
  }

  if (route?.id === "admin.mcp.servers") {
    const method = request.method ?? "GET";
    const normalizedMethod = method.toUpperCase();
    return handleAdminMcpServerRegistryRequest(
      {
        method,
        url,
        requestId: requestId(request),
        body: ["POST", "PUT", "PATCH"].includes(normalizedMethod)
          ? await readJsonBody(request)
          : undefined,
      },
      {
        config: () => state.config,
        runtimeConfig: () => state.runtimeConfig,
        readRuntimeConfigFile: () =>
          readRuntimeConfigFileForMutationFromModule(
            profileAdminMutationContext(state),
          ),
        writeRuntimeConfigFile: (value) =>
          writeJsonFileAtomicFromModule(
            state.config.paths.serviceConfigFile,
            value,
          ),
        applyRuntimeConfigFromDisk: (input) =>
          applyServiceRuntimeConfigFromDisk(state, input),
        withRuntimeConfigMutation: (mutation) =>
          withAsyncMutationQueue(state.runtimeConfigMutationQueue, mutation),
      },
    );
  }

  if (route?.id === "admin.tools.catalog") {
    return handleAdminToolsCatalogRequest({
      method: request.method ?? "GET",
      requestId: requestId(request),
    });
  }

  if (route?.id === "admin.brain_catalog") {
    return handleAdminBrainCatalogRequest(
      {
        method: request.method ?? "GET",
        requestId: requestId(request),
      },
      state.bridge,
    );
  }

  if (route?.id === "admin.context_strategies") {
    return handleAdminContextStrategiesRequest({
      method: request.method ?? "GET",
      requestId: requestId(request),
    });
  }

  if (route?.id === "admin.local_tool_profiles") {
    return handleAdminLocalToolProfilesRequest(
      {
        method: request.method ?? "GET",
        requestId: requestId(request),
        url,
        readBody: () => readJsonBody(request),
      },
      {
        store: createLocalToolProfileStore({
          bridge: state.bridge,
          now: state.now,
        }),
      },
    );
  }

  if (route?.id === "roleplay") {
    return withChatCors(
      await handleAdminRoleplayRequest(
        request,
        roleplayRouteContext(state),
        url,
      ),
      request,
    );
  }

  if (route?.id === "admin.storage") {
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

  if (route?.id === "admin.model_providers") {
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
      {
        listModelProviders: (query) => state.bridge.listModelProviders(query),
        getModelProvider: (alias) => state.bridge.getModelProvider(alias),
        upsertModelProvider: (write) => state.bridge.upsertModelProvider(write),
        getServiceCredential: (credentialId) =>
          state.bridge.getServiceCredential(credentialId),
        upsertServiceCredential: (write) =>
          state.bridge.upsertServiceCredential(write),
        linkModelProviderCredential: (link) =>
          state.bridge.linkModelProviderCredential(link),
        unlinkModelProviderCredential: (unlink) =>
          state.bridge.unlinkModelProviderCredential(unlink),
        exchangeOpenAiOauthCode: (input) =>
          state.bridge.exchangeOpenAiOauthCode(input),
        openAiOauth: state.config.openAiOauth,
        pendingLogins: state.openAiOauthPendingLogins,
        now: state.now,
        refreshAfterWrite: (input) =>
          modelProviderRefreshAfterWrite({
            state,
            requestId: input.requestId,
            provider: input.provider,
            refreshMode: input.refreshMode,
          }),
      },
    );
  }

  if (route?.id === "admin.service_credentials") {
    const method = (request.method ?? "GET").toUpperCase();
    const body =
      method === "POST" || method === "PATCH"
        ? await readJsonBody(request)
        : undefined;
    return handleServiceCredentialAdminRequest(
      {
        method,
        url: url.toString(),
        body,
        requestId: requestId(request),
      },
      {
        listServiceCredentials: (query) =>
          state.bridge.listServiceCredentials(query),
        getServiceCredential: (credentialId) =>
          state.bridge.getServiceCredential(credentialId),
        upsertServiceCredential: (write) =>
          state.bridge.upsertServiceCredential(write),
        deleteServiceCredential: (deleteRequest) =>
          state.bridge.deleteServiceCredential(deleteRequest),
        listModelProviders: (query) => state.bridge.listModelProviders(query),
        getModelProvider: (alias) => state.bridge.getModelProvider(alias),
        linkModelProviderCredential: (link) =>
          state.bridge.linkModelProviderCredential(link),
        unlinkModelProviderCredential: (unlink) =>
          state.bridge.unlinkModelProviderCredential(unlink),
        exchangeOpenAiOauthCode: (input) =>
          state.bridge.exchangeOpenAiOauthCode(input),
        openAiOauth: state.config.openAiOauth,
        pendingLogins: state.openAiOauthPendingLogins,
        now: state.now,
      },
    );
  }

  if (route?.id === "admin.telegram_diplomat") {
    const method = (request.method ?? "GET").toUpperCase();
    const body = method === "POST" ? await readJsonBody(request) : undefined;
    return handleTelegramDiplomatAdminRequest(
      {
        method,
        url: url.toString(),
        body,
        requestId: requestId(request),
      },
      {
        bridge: state.bridge,
        config: state.config.telegram,
        connector: () => state.telegramConnector,
        restartConnector: () =>
          restartTelegramConnectorFromModule(adapterLifecycleContext(state)),
        now: state.now,
      },
    );
  }

  if (route?.id === "admin.profile_registry.write") {
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
      {
        planRegistryWrite: (route, bodyValue) =>
          planProfileRegistryWriteFromModule(
            profileRuntimeMutationContext(state),
            route,
            bodyValue,
          ),
        planRuntimeConfigWrite: (route, bodyValue) =>
          planProfileRegistryRuntimeConfigWriteFromModule(
            profileRuntimeMutationContext(state),
            route,
            bodyValue,
          ),
        updateProfileRegistryRecord: (input) =>
          state.bridge.updateProfileRegistryRecord(input),
        applyLifecycleEffects: (record) =>
          withAsyncMutationQueue(state.runtimeConfigMutationQueue, () =>
            applyProfileRegistryLifecycleEffects(state, record),
          ),
        applyPromptEffects: (record) =>
          withAsyncMutationQueue(state.runtimeConfigMutationQueue, () =>
            state.externalRuntimeController.profileInstructionStatus(
              record.profileId,
            ),
          ),
        applyRuntimeConfigEffects: (record, plan) =>
          withAsyncMutationQueue(state.runtimeConfigMutationQueue, () =>
            applyProfileRegistryRuntimeConfigEffectsFromModule(
              profileRuntimeMutationContext(state),
              record,
              plan as ExtractedProfileRegistryRuntimeConfigPlan,
            ),
          ),
      },
    );
  }

  if (route?.id === "admin.memory") {
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

  if (route?.id === "admin.diagnostics") {
    return handleAdminDiagnosticsRequest(
      {
        method: request.method ?? "GET",
        url: url.toString(),
        requestId: requestId(request),
      },
      await buildDiagnosticsContext(state, {
        includeProfileRegistry: isProfileRegistryAdminRoute(url.pathname),
        includeStorageDiagnostics: includeStorageDiagnosticsForAdminPath(
          url.pathname,
        ),
        curatorUrl: isCuratorAdminReadRoute(url.pathname) ? url : undefined,
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

type ProfileRuntimeToolPolicy = {
  requestedToolsets?: string[];
  requestedTools?: string[];
  deniedTools?: string[];
  includeDeprecated?: boolean;
};

function profileToolPolicyFromUnknown(
  value: unknown,
): ProfileRuntimeToolPolicy | undefined {
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

type ProfileRuntimeBrainMetadata = { module?: string; strategy?: string };

function brainMetadataFromUnknown(
  value: unknown,
): ProfileRuntimeBrainMetadata | undefined {
  const brain = optionalRecord(value);
  if (brain === undefined) return undefined;
  return compactRecord({
    module: optionalString(brain.module),
    strategy: optionalString(brain.strategy),
  }) as ProfileRuntimeBrainMetadata;
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
    await archiveServiceSession(state, session.sessionId);
    sessionsArchived.push(String(session.sessionId));
  }
  const brainHandle = await unregisterServiceProfileBrainFromModule(
    profileAdminMutationContext(state),
    record.profileId,
  );
  return { sessionsArchived, brainHandle };
}

async function modelProviderRefreshAfterWrite(input: {
  state: ServiceState;
  requestId: string;
  provider: NativeModelProviderRecord;
  refreshMode: ModelProviderRefreshMode;
}): Promise<ModelProviderWriteRefreshResult> {
  const plan = await input.state.bridge.planModelProviderRefresh({
    providerAlias: input.provider.alias,
    mode: input.refreshMode,
  });
  const outcomes: ModelProviderWriteRefreshResult["refresh"]["outcomes"] = [];
  if (plan.mode !== "none") {
    for (const action of plan.actions) {
      const command: AdminControlCommand = {
        name: modelProviderRefreshCommandName(action.commandName),
        target: { scope: "profile", profileId: action.profileId },
        actor: { operatorId: "model-provider-admin" },
        requestId: input.requestId,
        reason: action.reason,
        body: {},
      };
      try {
        const outcome =
          plan.mode === "apply"
            ? await applyServiceRuntimeRebuild(input.state, command)
            : await planServiceRuntimeRebuild(input.state, command);
        const applyOutcome =
          plan.mode === "apply"
            ? (outcome as ServiceRuntimeRebuildApplyResult)
            : undefined;
        const applyStatus = applyOutcome?.apply.status;
        outcomes.push({
          profileId: action.profileId,
          status:
            plan.mode === "plan"
              ? "planned"
              : applyStatus === "completed"
                ? "applied"
                : "blocked",
          summary:
            plan.mode === "plan"
              ? action.plannedSummary
              : applyStatus === "completed"
                ? action.appliedSummary
                : action.blockedSummary,
          reasonCode:
            applyOutcome?.apply.status === "blocked"
              ? applyOutcome.apply.reasonCode
              : undefined,
          result: outcome,
        });
      } catch (error) {
        outcomes.push({
          profileId: action.profileId,
          status: "failed",
          summary: errorMessage(
            error,
            `runtime rebuild failed for profile ${action.profileId}`,
          ),
          reasonCode: action.failureReasonCode,
        });
      }
    }
  }

  return {
    refresh: {
      mode: plan.mode,
      affectedProfiles: plan.affectedProfiles,
      outcomes,
    },
  };
}

function modelProviderRefreshCommandName(
  value: string,
): AdminControlCommandName {
  if (value === "plan_runtime_rebuild" || value === "apply_runtime_rebuild") {
    return value;
  }
  throw new Error(`unknown model-provider refresh command ${value}`);
}

const DIAGNOSTICS_CONTEXT_CACHE_TTL_MS = 1_000;

async function buildDiagnosticsContext(
  state: ServiceState,
  options: {
    includeProfileRegistry?: boolean;
    includeStorageDiagnostics?: boolean;
    curatorUrl?: URL;
  } = {},
): Promise<AdminDiagnosticsContext> {
  // Profile registry revisions are optimistic-concurrency authority. Returning
  // even a one-second-old diagnostics snapshot can make the next valid session
  // creation impossible after a sibling advances the profile revision.
  if (options.includeProfileRegistry === true) {
    return buildDiagnosticsContextUncached(state, options);
  }
  const key = diagnosticsContextCacheKey(options);
  const cached = state.diagnosticsContextCache.get(key);
  if (cached !== undefined && cached.expiresAt > Date.now()) {
    return cached.value;
  }
  const inFlight = state.diagnosticsContextInFlight.get(key);
  if (inFlight !== undefined) return inFlight;

  const build = buildDiagnosticsContextUncached(state, options);
  state.diagnosticsContextInFlight.set(key, build);
  try {
    const value = await build;
    state.diagnosticsContextCache.set(key, {
      expiresAt: Date.now() + DIAGNOSTICS_CONTEXT_CACHE_TTL_MS,
      value,
    });
    return value;
  } finally {
    if (state.diagnosticsContextInFlight.get(key) === build) {
      state.diagnosticsContextInFlight.delete(key);
    }
  }
}

function diagnosticsContextCacheKey(options: {
  includeProfileRegistry?: boolean;
  includeStorageDiagnostics?: boolean;
  curatorUrl?: URL;
}): string {
  return JSON.stringify({
    includeProfileRegistry: options.includeProfileRegistry === true,
    includeStorageDiagnostics: options.includeStorageDiagnostics !== false,
    curatorUrl: options.curatorUrl?.pathname ?? null,
  });
}

function includeStorageDiagnosticsForAdminPath(pathname: string): boolean {
  return (
    pathname === "/v1/admin/diagnostics" ||
    pathname === "/v1/admin/diagnostics/persistence" ||
    pathname === "/v1/admin/diagnostics/storage"
  );
}

async function buildDiagnosticsContextUncached(
  state: ServiceState,
  options: {
    includeProfileRegistry?: boolean;
    includeStorageDiagnostics?: boolean;
    curatorUrl?: URL;
  } = {},
): Promise<AdminDiagnosticsContext> {
  const now = state.now();
  const [
    runtimeSummary,
    sessions,
    storage,
    providerStates,
    bufferedBrainRuns,
    runtimeActivities,
    memorySpaces,
  ] = await Promise.all([
    state.bridge
      .runtimeSummary({ scopeType: "runtime" })
      .catch(() => undefined),
    listProjectedServiceSessions(state).catch(() => []),
    options.includeStorageDiagnostics === false
      ? Promise.resolve(undefined)
      : state.bridge
          .storageDiagnostics()
          .then((diagnostics) =>
            storageDiagnosticsProjection(
              diagnostics,
              state.runtimeConfig.storage ?? state.config.storage,
            ),
          )
          .catch(() => undefined),
    state.bridge.providerStateDiagnostics().catch(() => []),
    state.bridge.bufferedBrainRunDiagnostics().catch(() => undefined),
    state.bridge.runtimeActivityCensus({}).catch(() => undefined),
    buildMemorySpaceDiagnostics(state).catch(() => undefined),
  ]);
  const profileRegistry = options.includeProfileRegistry
    ? await buildAdminProfileRegistryDiagnostics({
        bridge: state.bridge,
        runtimeConfig: state.runtimeConfig,
        now,
      }).catch(() => undefined)
    : undefined;
  const curatorReadback = options.curatorUrl
    ? await buildCuratorAdminReadback(state, options.curatorUrl)
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
    ...(bufferedBrainRuns === undefined ? {} : { bufferedBrainRuns }),
    ...(runtimeActivities === undefined ? {} : { runtimeActivities }),
    adapters: buildServiceAdapterDiagnostics(state, now),
    tools: buildSelectedToolDiagnostics(state, sessions),
    persistence: {
      tableCounts: tableCountMap(storage),
      searchHealthy: storage?.searchHealthy ?? true,
      databaseBytes: storage?.size.databaseBytes,
    },
    recentErrors: [
      ...(state.reviewDenAuthorityDiagnostics.status === "ready"
        ? []
        : [
            {
              source: "service-review-den-authority",
              message: state.reviewDenAuthorityDiagnostics.message,
              reasonCode: "blocked_dependency" as const,
              observedAt: state.reviewDenAuthorityDiagnostics.checkedAt,
              blocked: true,
            },
          ]),
      ...(state.stopping
        ? [
            {
              source: "service-host",
              message: "service shutdown is in progress",
              reasonCode: "blocked_dependency" as const,
              observedAt: now,
            },
          ]
        : []),
    ],
    runtimePauses: runtimePauseDiagnostics(state, sessions),
  });
  const health = buildRuntimeHealthProjection(diagnostics, {
    sourceRevision: state.config.sourceRevision,
  });
  const memorySurfaces = buildMemorySurfaceCatalog({
    now,
    dataDir: state.config.paths.dataDir,
    profilesDir: state.runtimeConfig.profilesDir,
    ...(state.runtimeConfig.skillsDir === undefined
      ? {}
      : { skillsDir: state.runtimeConfig.skillsDir }),
    memorySpaceDescriptors:
      memorySpaces?.items.map((item) => item.descriptor) ?? [],
    storageSearchHealthy: storage?.searchHealthy ?? false,
    externalMemory: state.externalMemoryReadiness.current(),
    mcpSurfaces: state.mcpManager.diagnostics(),
    denPlanningToolNames: [
      ...new Set(
        sessions.flatMap((session) =>
          session.toolProfile.tools
            .map((tool) => tool.name)
            .filter((toolName) => toolName.startsWith("den_")),
        ),
      ),
    ],
  });
  return {
    diagnostics,
    health,
    storage,
    memorySpaces,
    memorySurfaces,
    builtInSkills: builtInSkillCatalogDiagnostics(),
    profileRegistry,
    curatorCandidates: curatorReadback?.candidates,
    curatorMutations: curatorReadback?.mutations,
    curatorAuditReceipts: curatorReadback?.auditReceipts,
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

function isCuratorAdminReadRoute(pathname: string): boolean {
  return pathname.startsWith("/v1/admin/curator/");
}

async function buildCuratorAdminReadback(
  state: ServiceState,
  url: URL,
): Promise<{
  candidates?: unknown;
  mutations?: unknown;
  auditReceipts?: unknown;
}> {
  const page = {
    limit: optionalUnsignedInteger(url.searchParams.get("limit")),
    offset: optionalUnsignedInteger(url.searchParams.get("offset")),
  };
  switch (url.pathname) {
    case "/v1/admin/curator/candidates":
      return {
        candidates: await state.bridge.listCuratorCandidates({
          profile_id: optionalQueryValue(url, "profile_id"),
          session_id: optionalQueryValue(url, "session_id"),
          status: optionalQueryValue(url, "status"),
          lifecycle_state: optionalQueryValue(url, "lifecycle_state"),
          page,
        }),
      };
    case "/v1/admin/curator/mutations":
      return {
        mutations: await state.bridge.listCuratorMutations({
          candidate_id: optionalQueryValue(url, "candidate_id"),
          status: optionalQueryValue(url, "status"),
          page,
        }),
      };
    case "/v1/admin/curator/audit-receipts":
      return {
        auditReceipts: await state.bridge.listCuratorAuditReceipts({
          profile_id: optionalQueryValue(url, "profile_id"),
          session_id: optionalQueryValue(url, "session_id"),
          candidate_id: optionalQueryValue(url, "candidate_id"),
          mutation_id: optionalQueryValue(url, "mutation_id"),
          activity_kind: optionalQueryValue(url, "activity_kind"),
          page,
        }),
      };
    default:
      return {};
  }
}

function optionalQueryValue(url: URL, name: string): string | undefined {
  const value = url.searchParams.get(name)?.trim();
  return value ? value : undefined;
}

function optionalUnsignedInteger(value: string | null): number | undefined {
  if (value === null || value.trim() === "") return undefined;
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) && parsed >= 0 ? parsed : undefined;
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
            : descriptor.space_id === "roleplay_lore"
              ? {
                  spaceId: descriptor.space_id,
                  status: "compatible",
                  backingStore: "roleplay_lore module tables",
                  nativeMethods: [
                    "queryRoleplayLoreRecords",
                    "recallLore",
                    "addRoleplayLoreRecord",
                    "replaceRoleplayLoreRecord",
                    "supersedeRoleplayLoreRecord",
                    "tombstoneRoleplayLoreRecord",
                  ],
                  conflictBehavior: "expected_revision",
                  promptInjectionBehavior:
                    "Lore remains domain-specific and enters narrator/model context only through typed lore recall and explicit roleplay assembly.",
                  toolModeBehavior:
                    "Roleplay lore read/write tools follow the descriptor's canon-aware governance, provenance, and expected-revision policies.",
                  notes: [
                    "Roleplay lore is Crew-owned domain memory, not a generic memory blob.",
                    "Runtime search and Den planning tools remain separate surfaces.",
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
        ? "Scheduler stale-run expiry uses PostgreSQL row-level claim semantics in the backend slice."
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
        "Runtime search has a PostgreSQL backend slice behind typed APIs; backend tsquery syntax is not exposed through admin or tool routes.",
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
      "PostgreSQL runtime search is unavailable for full service boot until the backend repository is wired behind the service backend.",
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
  const chatMetrics = "promptCachingPolicy" in metrics ? metrics : undefined;
  state.responsesWakeMetrics.unshift({
    profileId: observation.profileId,
    sessionId: observation.sessionId,
    wakeId: observation.wakeId,
    observedAt: state.now(),
    effectiveTransport: metrics.effectiveTransport,
    ...(metrics.providerDialect === undefined
      ? {}
      : { providerDialect: metrics.providerDialect }),
    selectedStrategyId: metrics.selectedStrategyId,
    effectiveStrategyId: metrics.effectiveStrategyId,
    fallbackReason: metrics.fallbackReason,
    providerRequestCount: metrics.providerRequestCount,
    continuationRoundCount: metrics.continuationRoundCount,
    providerRequestPayloadBytes: metrics.providerRequestPayloadBytes,
    providerEventCounts: metrics.providerEventCounts,
    ...(metrics.inputTokens === undefined
      ? {}
      : { inputTokens: metrics.inputTokens }),
    ...(metrics.cachedInputTokens === undefined
      ? {}
      : { cachedInputTokens: metrics.cachedInputTokens }),
    ...(metrics.outputTokens === undefined
      ? {}
      : { outputTokens: metrics.outputTokens }),
    ...(metrics.reasoningOutputTokens === undefined
      ? {}
      : { reasoningOutputTokens: metrics.reasoningOutputTokens }),
    ...(metrics.totalTokens === undefined
      ? {}
      : { totalTokens: metrics.totalTokens }),
    ...(chatMetrics?.promptCachingPolicy === undefined
      ? {}
      : { promptCachingPolicy: chatMetrics.promptCachingPolicy }),
    ...(chatMetrics?.openrouterSessionId === undefined
      ? {}
      : { openrouterSessionId: chatMetrics.openrouterSessionId }),
    ...(chatMetrics?.promptTokens === undefined
      ? {}
      : { promptTokens: chatMetrics.promptTokens }),
    ...(chatMetrics?.cachedPromptTokens === undefined
      ? {}
      : { cachedPromptTokens: chatMetrics.cachedPromptTokens }),
    ...(chatMetrics?.cacheWritePromptTokens === undefined
      ? {}
      : { cacheWritePromptTokens: chatMetrics.cacheWritePromptTokens }),
    brainEventCounts,
    brainStreamItemCounts,
    streamRetentionMetrics: observation.result.streamRetentionMetrics,
    firstTextDeltaLatencyMs: metrics.firstTextDeltaLatencyMs,
    totalTurnDurationMs: metrics.totalTurnDurationMs,
    terminalFailureReasonCode: metrics.terminalFailureReasonCode,
    terminalFailureSource: metrics.terminalFailureSource,
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
          effectiveSessionDefaults(configured ?? {}, profile),
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
      ...telegramChannelActivityDiagnosticsFromModule(
        adapterLifecycleContext(state),
        now,
      ),
      ...denConversationChannelActivityDiagnosticsFromModule(
        adapterLifecycleContext(state),
      ),
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

async function createServiceCuratorRuntime(input: {
  config: RustyCrewServiceConfig;
  runtimeConfig: RustyCrewRuntimeConfig;
  bridge: NativeBridgeModule;
  now: () => string;
  publishActivity?: (receipt: CuratorActivityReceipt) => Promise<void>;
}): Promise<ServiceCuratorRuntime> {
  const skillsDir = curatorSkillsDir(input.runtimeConfig);
  const snapshotRoot = join(input.config.paths.backupDir, "curator-snapshots");
  const store = await NativeCuratorGovernanceStore.load({
    bridge: input.bridge,
    now: input.now(),
    skillsDir,
    snapshotRoot,
    publishActivity: input.publishActivity,
  });
  const runtime: ServiceCuratorRuntime = {
    store,
    runtimeConfig: input.runtimeConfig,
    executor: async () => {
      throw new Error("curator executor not initialized");
    },
  };
  runtime.executor = createCuratorGovernanceExecutor({
    skillsDir,
    store,
    snapshotDir: snapshotRoot,
    now: () => new Date(input.now()),
    planner: (request) =>
      input.bridge.planCuratorGovernanceTransition(
        request,
      ) as ReturnType<CuratorGovernancePlanner>,
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

async function publishServiceCuratorActivity(
  state: ServiceState,
  receipt: CuratorActivityReceipt,
): Promise<void> {
  if (state.denGatewayClient === undefined) return;
  const result = await publishCuratorActivityObservation({
    producer: new AgentActivityObservationProducer({
      sink: createDenGatewayObservationSink(state.denGatewayClient),
      required: true,
    }),
    receipt,
  });
  recordServiceEvent(state, {
    source: "curator",
    eventType: `curator_${receipt.activityKind}`,
    summary: receipt.summary,
    severity: result.status === "degraded" ? "warning" : "info",
    workRef: {
      receiptId: receipt.receiptId,
      sequence: receipt.sequence,
      correlationId: receipt.correlationId,
    },
    resultRef: {
      artifactPath: `curator://receipt/${receipt.receiptId}`,
    },
  });
  if (result.status === "degraded") {
    throw new Error(result.message);
  }
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
  const profile = await loadProfileCuratorDiscoveryContext({
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
    status:
      state.curator.lastError ||
      state.curator.store.activityProjectionFailures.length > 0
        ? "degraded"
        : "available",
    candidateCount: state.curator.store.candidates.size,
    mutationCount: state.curator.store.mutations.size,
    pinnedSkillCount: pinnedSkills.length,
    archivedSkillCount: archivedSkills.length,
    lastRunAt: state.curator.lastRunAt,
    lastError: state.curator.lastError,
    activityProjectionFailureCount:
      state.curator.store.activityProjectionFailures.length,
    lastActivityReceiptId: state.curator.store.lastActivityReceipt?.receiptId,
    lastActivitySequence: state.curator.store.lastActivityReceipt?.sequence,
    lifecycle: state.curator.lastLifecycleReport,
  };
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
    existingBrainModulesByProfileId:
      state.runtimeConfigApplyResult.brainModulesByProfileId,
    existingBrainDiagnosticsByProfileId:
      state.runtimeConfigApplyResult.brainDiagnosticsByProfileId,
    createMissingSessions: options.createMissingSessions,
    curatorExecutor: state.curator.executor,
    mcpSurfaceDiagnostics: nextMcpManager.diagnostics(),
    adapterFactories: state.adapterFactories,
    externalMemoryReadiness: state.externalMemoryReadiness,
    coordinationRuntime: createServiceCoordinationRuntime(() => state),
    reviewSubmissionRuntime: createServiceReviewSubmissionRuntime(() =>
      reviewSubmissionContext(state),
    ),
    toolCallDebugStore: state.toolCallDebugStore,
    providerRequestDebugStore: state.providerRequestDebugStore,
    browserResources: state.browserResources,
    toolMediaSink: state.toolMediaAttachments,
    narratorImageContextResolver: state.toolMediaAttachments,
    onBrainWakeResult: (observation) =>
      recordResponsesWakeMetrics(state, observation),
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
  await ensureDenConversationChannelsFromModule(adapterLifecycleContext(state));
  await restartTelegramConnectorFromModule(adapterLifecycleContext(state));
  recordServiceEvent(state, {
    source: "service-host",
    eventType: options.eventType,
    summary: runtimeConfigApplySummary(options.summaryPrefix, nextApplyResult),
  });
  return nextApplyResult;
}

async function rebuildServiceBrainRuntime(
  state: ServiceState,
  profileId: ProfileId,
) {
  const rebuild = await rebuildConfiguredBrainRuntime({
    serviceConfig: state.config,
    runtimeConfig: state.runtimeConfig,
    profileId,
    bridge: state.bridge,
    curatorExecutor: state.curator.executor,
    mcpSurfaceDiagnostics: state.mcpManager.diagnostics(),
    adapterFactories: state.adapterFactories,
    externalMemoryReadiness: state.externalMemoryReadiness,
    coordinationRuntime: createServiceCoordinationRuntime(() => state),
    toolCallDebugStore: state.toolCallDebugStore,
    providerRequestDebugStore: state.providerRequestDebugStore,
    browserResources: state.browserResources,
    toolMediaSink: state.toolMediaAttachments,
    narratorImageContextResolver: state.toolMediaAttachments,
    onBrainWakeResult: (observation) =>
      recordResponsesWakeMetrics(state, observation),
  });
  state.runtimeConfigApplyResult.brainHandlesByProfileId[profileId] =
    rebuild.handle;
  state.runtimeConfigApplyResult.brainModulesByProfileId[profileId] =
    rebuild.module;
  state.runtimeConfigApplyResult.brainDiagnosticsByProfileId[profileId] =
    rebuild.diagnostics;
  return rebuild;
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
          const profileContext = await loadServiceProfileContext({
            bridge: state.bridge,
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
            toolSelection: effectiveToolSelectionForResourceLimits(
              profileContext.toolSelection,
              session.resourceLimits,
            ),
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
          suppressNextWakeEventFromModule(
            wakeEventDrainContext(state, "direct_debug"),
            input.session.sessionId,
          );
          wakeReport = await dispatchWake(
            state,
            {
              type: "brain_wake_requested",
              sessionId: input.session.sessionId,
            },
            "direct_debug",
          );
          await drainAndDispatchWakesFromModule(
            wakeEventDrainContext(state, "direct_debug"),
          );
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
    inferenceOverrides: {},
    status: "active",
    brainTurnCount: 0,
    createdAt: now,
    lastActiveAt: now,
  }));
}

function createServiceControlExecutor(
  state: ServiceState,
): AdminControlExecutor {
  const withRuntimeConfigMutation = <T>(operation: () => Promise<T>) =>
    withAsyncMutationQueue(state.runtimeConfigMutationQueue, operation);
  return {
    ...createCuratorAdminControlExecutor({
      curatorExecutor: state.curator.executor,
      rollbackMutation: (mutationId) =>
        rollbackCuratorMutation(state.curator.store, mutationId),
      status: () => curatorStatus(state),
      skillsDir: curatorSkillsDir(state.curator.runtimeConfig),
    }),
    createProfile: async (command) => {
      const result = await withRuntimeConfigMutation(() =>
        createServiceProfileFromModule(
          profileAdminMutationContext(state),
          command,
        ),
      );
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
      const result = await readServiceProfileConfigFromModule(
        profileAdminMutationContext(state),
        command,
      );
      return {
        status: "completed",
        summary: `profile ${result.profileId} read`,
        affectedIds: { profileId: String(result.profileId) },
        result,
      };
    },
    planProfileUpdate: async (command) => {
      const result = await planServiceProfileUpdateFromModule(
        profileAdminMutationContext(state),
        command,
      );
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
      const result = await withRuntimeConfigMutation(() =>
        applyServiceProfileUpdateFromModule(
          profileAdminMutationContext(state),
          command,
        ),
      );
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
      const result = await withRuntimeConfigMutation(() =>
        decommissionServiceProfileFromModule(
          profileAdminMutationContext(state),
          command,
        ),
      );
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
      const result = await withRuntimeConfigMutation(() =>
        deleteServiceProfileFromModule(
          profileAdminMutationContext(state),
          command,
        ),
      );
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
      const workspaceCwd = requiredBodyString(command, "workspaceCwd");
      const kind = optionalBodyString(command, "kind") ?? "full";
      if (kind !== "full" && kind !== "worker" && kind !== "delegated") {
        throw new Error("session kind must be full, worker, or delegated");
      }
      const profileSession = (await state.bridge.listSessions()).find(
        (candidate) =>
          candidate.profileId === profileId && candidate.status !== "archived",
      );
      const session = await state.bridge.createSession({
        sessionId,
        agentId,
        profileId,
        kind,
        workspace: {
          cwd: workspaceCwd,
          revision: 1,
          updatedAt: state.now(),
        },
        resourceLimits: createSessionResourceLimits(command),
        ...(profileSession === undefined
          ? {}
          : { toolProfile: profileSession.toolProfile }),
      });
      return {
        status: "completed",
        summary: `session ${session.sessionId} created`,
        affectedIds: { sessionId: session.sessionId },
        result: session,
      };
    },
    switchSessionWorkspace: async (command) => {
      const sessionId = command.target.sessionId as SessionId | undefined;
      if (sessionId === undefined) {
        throw new Error("session_workspace_session_id_required");
      }
      const cwd = requiredBodyString(command, "cwd");
      const expectedRevision = command.body.expectedRevision;
      if (
        !Number.isSafeInteger(expectedRevision) ||
        Number(expectedRevision) < 1
      ) {
        throw new Error("expectedRevision must be a positive integer");
      }
      let result: SessionWorkspaceUpdateRecord;
      try {
        ({ update: result } = await withRuntimeConfigMutation(() =>
          switchCrewSessionWorkspace(crewSessionLifecycleContext(state), {
            sessionId,
            cwd,
            expectedRevision: Number(expectedRevision),
          }),
        ));
      } catch (error) {
        if (
          error instanceof CrewSessionLifecycleError &&
          error.partialOutcome !== undefined
        ) {
          const affectedIds: Record<string, string | number> = { sessionId };
          return {
            status: "failed",
            summary: error.message,
            reasonCode: error.reasonCode,
            affectedIds,
            result: error.partialOutcome,
          };
        }
        throw error;
      }
      return {
        status: "completed",
        summary: `session ${sessionId} workspace is ${result.current.cwd}`,
        affectedIds: {
          sessionId,
          workspaceRevision: result.current.revision,
        },
        result,
      };
    },
    archiveSession: async (command) => {
      const sessionId = command.target.sessionId as SessionId | undefined;
      if (sessionId === undefined) {
        throw new Error("crew_session_archive_session_id_required");
      }
      const result = await withRuntimeConfigMutation(() =>
        archiveCrewSession(crewSessionLifecycleContext(state), {
          sessionId,
          ...(command.body.chatCommandName === "archive"
            ? {
                commandName: "archive",
                requestId: command.requestId,
                actorId: command.actor.operatorId,
              }
            : {}),
        }),
      );
      return {
        status: "completed",
        summary: `Archived session ${sessionId}.`,
        affectedIds: { sessionId },
        result,
      };
    },
    newSession: (() => {
      const pendingRuntimeConfigReplacements = new Map<
        string,
        { oldSession: SessionState; plan: ServiceRuntimeReplacementConfigPlan }
      >();
      const executor = createNewSessionLifecycleExecutor({
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
        planNewSessionControl: (input) =>
          state.bridge.planNewSessionControl(input),
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
          await archiveServiceSession(state, sessionId as SessionId);
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
            if (replacement.mcpBindings.bindingIds.length > 0) {
              const rebuild = await applyServiceRuntimeRebuild(state, {
                ...command,
                name: "apply_runtime_rebuild",
                target: {
                  scope: "profile",
                  profileId: oldSession.profileId,
                },
                reason:
                  command.reason ??
                  "New session moved MCP bindings; refreshing live brain catalog",
                body: {
                  ...command.body,
                  skipSessionReplacement: true,
                },
              });
              recordServiceEvent(state, {
                source: "service-host",
                eventType:
                  rebuild.apply.status === "completed"
                    ? "new_session_brain_catalog_rebuilt"
                    : "new_session_brain_catalog_rebuild_blocked",
                severity:
                  rebuild.apply.status === "completed" ? undefined : "warning",
                summary:
                  rebuild.apply.status === "completed"
                    ? `New session rebuilt brain catalog for profile ${oldSession.profileId} after moving MCP bindings.`
                    : `New session could not rebuild brain catalog for profile ${oldSession.profileId}: ${rebuild.apply.reasonCode}.`,
              });
            }
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
      return (command) =>
        withRuntimeConfigMutation(async () => executor(command));
    })(),
    setSessionEffort: async (command) => {
      const raw = command.body.reasoningEffort;
      if (
        raw !== null &&
        (typeof raw !== "string" || !isNativeReasoningEffort(raw))
      ) {
        throw new Error(
          "reasoningEffort must be one of none, minimal, low, medium, high, xhigh, or null",
        );
      }
      const sessionSummary = await state.bridge.setSessionReasoningEffort(
        command.target.sessionId as SessionId,
        raw === null ? undefined : raw,
      );
      const session = (await state.bridge.listSessions()).find(
        (candidate) => candidate.sessionId === sessionSummary.sessionId,
      );
      const contextUsage =
        session === undefined
          ? undefined
          : await rustyViewSessionContextUsage(
              rustyViewChatOperationsContext(state),
              { session, requestId: command.requestId },
            ).catch(() => undefined);
      const reasoningEffort = resolveReasoningEffort(
        session?.inferenceOverrides?.reasoningEffort ??
          sessionSummary.reasoningEffort,
        contextUsage?.provider.provider_reasoning_effort,
      );
      const resolved = reasoningEffort.value ?? "provider default";
      return {
        status: "completed",
        summary:
          raw === null
            ? `session ${sessionSummary.sessionId} now uses ${resolved} reasoning effort (${reasoningEffort.source})`
            : `session ${sessionSummary.sessionId} reasoning effort set to ${resolved} (${reasoningEffort.source})`,
        affectedIds: { sessionId: sessionSummary.sessionId },
        result: {
          session: sessionSummary,
          reasoningEffort: reasoningEffort.value ?? null,
          reasoningEffortSource: reasoningEffort.source,
          providerReasoningEffort:
            contextUsage?.provider.provider_reasoning_effort ?? null,
          sessionReasoningEffortOverride:
            session?.inferenceOverrides?.reasoningEffort ??
            sessionSummary.reasoningEffort ??
            null,
        },
      };
    },
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
      const result = await withRuntimeConfigMutation(() =>
        reloadServiceRuntimeConfig(state),
      );
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
      const result = await planServiceRuntimeConfigDraftFromModule(
        profileAdminMutationContext(state),
        command,
      );
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
      const result = await withRuntimeConfigMutation(() =>
        applyServiceRuntimeConfigDraftFromModule(
          profileAdminMutationContext(state),
          command,
        ),
      );
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
      const result = await withRuntimeConfigMutation(() =>
        applyServiceRuntimeRebuild(state, command),
      );
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
        await runServiceCuratorLifecycleTransitionsFromModule(
          schedulerBackgroundContext(state),
        );
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
        compactTerminalExternalRuntimeEventsBefore: optionalBodyString(
          command,
          "compactTerminalExternalRuntimeEventsBefore",
        ),
        externalRuntimeEventRetentionAt: optionalBodyString(
          command,
          "externalRuntimeEventRetentionAt",
        ),
        externalRuntimeEventTerminalTurnBatchSize: optionalNumber(
          command.body.externalRuntimeEventTerminalTurnBatchSize,
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
    planReloadMcpControl: (input) => state.bridge.planReloadMcpControl(input),
    manager: state.mcpManager,
    discoveryClient: {
      listTools: () => [],
    },
    discoveryClientForBinding: (binding) =>
      createDefaultMcpDiscoveryClient(binding, state.config.mcp),
    metadataPolicyValidator: createBridgeToolMetadataPolicyValidator(
      state.bridge,
    ),
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

async function planServiceRuntimeRebuild(
  state: ServiceState,
  command: AdminControlCommand,
): Promise<ServiceRuntimeRebuildPlan> {
  return planServiceRuntimeRebuildFromModule(
    runtimeRebuildContext(state),
    command,
  );
}

async function applyServiceRuntimeRebuild(
  state: ServiceState,
  command: AdminControlCommand,
): Promise<ServiceRuntimeRebuildApplyResult> {
  return applyServiceRuntimeRebuildFromModule(
    runtimeRebuildContext(state),
    command,
  );
}

async function replaceRuntimeSessionInConfig(
  state: ServiceState,
  oldSession: SessionState,
  newSessionId: string,
  channelBindingAction: "move" | "unchanged",
): Promise<ServiceRuntimeReplacementSessionResult> {
  return replaceRuntimeSessionInConfigFromModule(
    runtimeRebuildContext(state),
    oldSession,
    newSessionId,
    channelBindingAction,
  );
}

async function planRuntimeSessionReplacementInConfig(
  state: ServiceState,
  oldSession: SessionState,
  newSessionId: string,
  channelBindingAction: "move" | "unchanged",
): Promise<ServiceRuntimeReplacementConfigPlan> {
  return planRuntimeSessionReplacementInConfigFromModule(
    runtimeRebuildContext(state),
    oldSession,
    newSessionId,
    channelBindingAction,
  );
}

async function commitRuntimeSessionReplacementInConfig(
  state: ServiceState,
  oldSession: SessionState,
  plan: ServiceRuntimeReplacementConfigPlan,
): Promise<ServiceRuntimeReplacementSessionResult> {
  return commitRuntimeSessionReplacementInConfigFromModule(
    runtimeRebuildContext(state),
    oldSession,
    plan,
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

function createSessionResourceLimits(
  command: AdminControlCommand,
): ResourceLimits | undefined {
  const value = command.body.resourceLimits;
  if (value === undefined) return undefined;
  if (!isRecord(value)) {
    throw new Error("control body field resourceLimits must be an object");
  }
  const maxDurationMs = resourceLimitInteger(value, "maxDurationMs");
  const maxDelegationDepth = resourceLimitInteger(value, "maxDelegationDepth");
  return {
    ...(maxDurationMs === undefined ? {} : { maxDurationMs }),
    ...(maxDelegationDepth === undefined ? {} : { maxDelegationDepth }),
  };
}

function resourceLimitInteger(
  value: Record<string, unknown>,
  key: string,
): number | undefined {
  const candidate = value[key];
  if (candidate === undefined) return undefined;
  if (
    typeof candidate !== "number" ||
    !Number.isInteger(candidate) ||
    candidate < 0 ||
    candidate > 0xffff_ffff
  ) {
    throw new Error(`resourceLimits.${key} must be an unsigned 32-bit integer`);
  }
  return candidate;
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
      recordDynamicDenDeliveryChannelFromModule(
        adapterLifecycleContext(state),
        intent,
        session,
        {
          channelId: channelIdFromDeliveryIntent(intent),
          sourceMessageId: intent.channel_message_id,
          wakePolicy: decision.wakePolicy,
          subscriptionStatus: "manual",
        },
      );
      recordServiceEvent(state, {
        source: "den-successor-gateway",
        eventType: "den_delivery_intent_manual",
        summary: `Left Den Delivery intent ${intent.id} pending for manual wake policy on ${session.agentId}; Gateway TTL remains authoritative.`,
      });
      continue;
    }
    if (decision.action === "reject") {
      state.claimedDeliveryIntentIds.add(intent.id);
      recordDynamicDenDeliveryChannelFromModule(
        adapterLifecycleContext(state),
        intent,
        session,
        {
          channelId: channelIdFromDeliveryIntent(intent),
          sourceMessageId: intent.channel_message_id,
          wakePolicy: decision.wakePolicy,
          subscriptionStatus: "disabled",
        },
      );
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
      recordDynamicDenDeliveryChannelFromModule(
        adapterLifecycleContext(state),
        intent,
        session,
        {
          channelId: channelIdFromDeliveryIntent(intent),
          sourceMessageId: intent.channel_message_id,
          wakePolicy: decision.wakePolicy,
          subscriptionStatus: "runtime_paused",
          lastError: runtimePauseSummary(pause, session.sessionId),
        },
      );
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
    recordDynamicDenDeliveryChannelFromModule(
      adapterLifecycleContext(state),
      intent,
      session,
      deliveryBody,
    );

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

async function emitContextCompactionDebugEvents(
  state: ServiceState,
  session: SessionState,
  input: ContextCompactionDebugEventInput,
): Promise<{ events: ChatEvent[]; latest_cursor: string }> {
  const basePayload = contextDebugPayload(session.sessionId, input);
  const events = [
    await appendChatEventFromModule(
      chatEventLogContext(state),
      session.sessionId,
      {
        kind: "context_status",
        payload: {
          ...basePayload,
          status: input.fail ? "will_fail" : "ready",
        },
      },
    ),
    await appendChatEventFromModule(
      chatEventLogContext(state),
      session.sessionId,
      {
        kind: "context_compaction_started",
        payload: {
          ...basePayload,
          status: "started",
        },
      },
    ),
  ];
  events.push(
    await appendChatEventFromModule(
      chatEventLogContext(state),
      session.sessionId,
      {
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
      },
    ),
  );
  return {
    events,
    latest_cursor: events.at(-1)?.event_id ?? "",
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

async function streamReplayEvents(
  context: RustyViewChatOperationsContext,
  session: SessionState,
  cursor: string | undefined,
  url: URL,
): Promise<readonly ChatEvent[]> {
  const limit = optionalInteger(url.searchParams.get("limit")) ?? 500;
  const after = chatCursorSequence(cursor, session.sessionId);
  const read = await readRustyViewChatSession(context, {
    sessionId: session.sessionId,
    cursor,
    limit: Math.min(Math.max(limit, 1), 1_000),
    includeAlternates: false,
  });
  const events = read.events;
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

function chatCursorSequence(
  cursor: string | undefined,
  sessionId: string,
): number {
  if (!cursor) return 0;
  const prefix = `${sessionId}:`;
  if (!cursor.startsWith(prefix)) return 0;
  const sequence = Number(cursor.slice(prefix.length));
  return Number.isSafeInteger(sequence) && sequence >= 0 ? sequence : 0;
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
    appendChatEvents?: boolean;
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
    suppressNextWakeEventFromModule(
      wakeEventDrainContext(state, input.source, input.observationContext),
      input.sessionId,
    );
    const wakeReport = await dispatchWake(
      state,
      {
        type: "brain_wake_requested",
        sessionId: input.sessionId,
      },
      input.source,
      input.observationContext,
      { appendChatEvents: input.appendChatEvents },
    );
    await drainAndDispatchWakesFromModule(
      wakeEventDrainContext(state, input.source, input.observationContext),
    );
    return wakeReport;
  } finally {
    state.directDispatchSessions.delete(input.sessionId);
  }
}

function createServiceCoordinationRuntime(
  getState: () => ServiceState | undefined,
): CoordinationToolRuntime {
  const runtime: CoordinationToolRuntime = {
    async listAgents() {
      const state = getState();
      if (state === undefined) {
        throw new Error("service coordination runtime is not ready");
      }
      return state.bridge.listAgentDirectory();
    },
    async listRoutes() {
      const state = getState();
      if (state === undefined) {
        throw new Error("service coordination runtime is not ready");
      }
      return state.bridge.listAgentRouteResolutions();
    },
    async routeMessage(input) {
      const state = getState();
      if (state === undefined) {
        throw new Error("service coordination runtime is not ready");
      }
      const createdAt = new Date().toISOString();
      const ttlMs = Math.min(
        Math.max((input.ttlSeconds ?? 300) * 1_000, 1_000),
        24 * 60 * 60_000,
      );
      const identity = `${input.fromSessionId}:${input.wakeId}:${input.toolCallId}`;
      const initialReceipt = await state.bridge.deliverAgentMessage({
        caller: {
          type: "direct_brain",
          sessionId: input.fromSessionId as SessionId,
          wakeId: input.wakeId,
          toolCallId: input.toolCallId,
        },
        deliveryId: `delivery:${identity}`,
        idempotencyKey: `delivery:${identity}`,
        messageId: `message:${identity}`,
        toAddress: input.toAddress,
        inputKind: "routed_agent_message",
        body: input.body,
        ...(input.correlationId === undefined
          ? {}
          : { correlationId: input.correlationId }),
        requireWake: input.requireWake ?? true,
        createdAt,
        expiresAt: new Date(Date.parse(createdAt) + ttlMs).toISOString(),
      });
      const receipt =
        await state.externalRuntimeController.applyCoordinationDelivery(
          initialReceipt,
        );
      const activation = receipt.activation;
      const resolvedTarget = receipt.request.routing?.resolvedTarget;
      const rawTarget =
        resolvedTarget == null
          ? resolveRawDeliveryTarget(
              receipt,
              await state.bridge.listAgentDirectory().catch(() => []),
            )
          : undefined;
      const runtimeKind =
        resolvedTarget?.runtimeKind ?? rawTarget?.runtimeKind ?? "unresolved";
      return {
        accepted: receipt.status === "accepted",
        sequence: receipt.sequence ?? undefined,
        destination: {
          requestedAddress: receipt.request.requestedAddress,
          addressKind:
            receipt.request.routing === undefined ||
            receipt.request.routing === null
              ? "raw_agent"
              : "curated_route",
          agentId: receipt.request.toAgentId,
          sessionId: receipt.request.toSessionId ?? undefined,
          runtimeKind,
          activation: activation?.type ?? "none",
        },
        wake:
          activation?.type === "rejected" ||
          receipt.status === "rejected" ||
          receipt.status === "expired"
            ? {
                status: "failed",
                summary: `message delivery to ${input.toAddress} ${receipt.status}`,
                reasonCode:
                  activation?.type === "rejected"
                    ? activation.reasonCode
                    : (receipt.reasonCode ?? undefined),
              }
            : activation?.type === "queued_for_next_turn"
              ? {
                  status: "skipped",
                  summary: `message queued for ${input.toAddress}'s next external turn`,
                  reasonCode: "external_turn_active",
                }
              : activation?.type === "external_turn_steer_requested"
                ? {
                    status: "completed",
                    summary: `message steered into ${input.toAddress}'s active external turn`,
                  }
                : {
                    status: "completed",
                    summary:
                      activation?.type === "external_turn_requested"
                        ? `external turn requested for ${input.toAddress}`
                        : `direct wake requested for ${input.toAddress}`,
                  },
      };
    },
    async replyMessage(input) {
      const state = getState();
      if (state === undefined) {
        throw new Error("service coordination runtime is not ready");
      }
      const createdAt = new Date().toISOString();
      const ttlMs = Math.min(
        Math.max((input.ttlSeconds ?? 300) * 1_000, 1_000),
        24 * 60 * 60_000,
      );
      const identity = `${input.fromSessionId}:${input.wakeId}:${input.toolCallId}`;
      const initialReceipt = await state.bridge.replyAgentMessage({
        caller: {
          type: "direct_brain",
          sessionId: input.fromSessionId as SessionId,
          wakeId: input.wakeId,
          toolCallId: input.toolCallId,
        },
        deliveryId: `reply-delivery:${identity}`,
        idempotencyKey: `reply-delivery:${identity}`,
        messageId: `reply-message:${identity}`,
        inReplyToMessageId: input.messageId,
        body: input.body,
        createdAt,
        expiresAt: new Date(Date.parse(createdAt) + ttlMs).toISOString(),
      });
      const receipt =
        await state.externalRuntimeController.applyCoordinationDelivery(
          initialReceipt,
        );
      return {
        accepted: receipt.status === "accepted",
        sequence: receipt.sequence ?? undefined,
        wake: {
          status:
            receipt.status === "accepted"
              ? ("completed" as const)
              : ("failed" as const),
          summary:
            receipt.status === "accepted"
              ? `reply accepted for message ${input.messageId}`
              : `reply to message ${input.messageId} ${receipt.status}`,
          reasonCode: receipt.reasonCode ?? undefined,
        },
      };
    },
    async roundTrip(input) {
      const state = getState();
      if (state === undefined) {
        throw new Error("service coordination runtime is not ready");
      }
      const createdAt = new Date().toISOString();
      const identity = `${input.fromSessionId}:${input.wakeId}:${input.toolCallId}`;
      const started = await state.bridge.beginAgentRound({
        caller: {
          type: "direct_brain",
          sessionId: input.fromSessionId as SessionId,
          wakeId: input.wakeId,
          toolCallId: input.toolCallId,
        },
        roundId: `round:${identity}`,
        idempotencyKey: `round:${identity}`,
        messageId: `round-message:${identity}`,
        toAddress: input.toAddress,
        body: input.body,
        correlationId: input.correlationId,
        createdAt,
        expiresAt: new Date(Date.now() + input.timeoutMs).toISOString(),
      });
      while (true) {
        const round = await state.bridge.getAgentRound(started.round.roundId);
        if (round === undefined || round.status === "expired") {
          return {
            accepted: started.delivery.status === "accepted",
            sequence: started.delivery.sequence ?? undefined,
            timedOut: true,
          };
        }
        if (round.status === "replied") {
          const outcome = round.outcome as
            | {
                from?: string;
                to?: string;
                body?: string;
                correlationId?: string;
              }
            | undefined;
          return {
            accepted: true,
            sequence: started.delivery.sequence ?? undefined,
            reply: {
              from: outcome?.from ?? input.toAddress,
              to: outcome?.to ?? input.fromAgentId,
              body: outcome?.body ?? "",
              correlationId: outcome?.correlationId ?? input.correlationId,
            },
          };
        }
        if (round.status === "failed" || round.status === "cancelled") {
          return {
            accepted: false,
            sequence: started.delivery.sequence ?? undefined,
          };
        }
        await new Promise<void>((resolve) => setTimeout(resolve, 25));
      }
    },
  };
  return runtime;
}

function reviewSubmissionContext(
  state: ServiceState,
): ServiceReviewSubmissionContext {
  return {
    bridge: state.bridge,
    runtimeConfig: state.runtimeConfig,
    serviceConfig: state.config,
    now: state.now,
    validateServiceDenAuthority: () =>
      refreshReviewDenAuthorityDiagnostics(state),
    applyCoordinationDelivery: (receipt) =>
      state.externalRuntimeController.applyCoordinationDelivery(receipt),
  };
}

async function refreshReviewDenAuthorityDiagnostics(
  state: ServiceState,
): Promise<ReviewDenAuthorityDiagnostics> {
  const diagnostics = await validateServiceReviewDenAuthority({
    authority: state.config.reviewDenAuthority,
    mcpConfig: state.config.mcp,
    now: state.now,
  });
  state.reviewDenAuthorityDiagnostics = diagnostics;
  return diagnostics;
}

function externalReviewApiFailure(
  requestIdValue: string,
  error: unknown,
): ServiceRouteResult {
  const reasonCode =
    error instanceof ReviewSubmissionAdapterError
      ? error.reasonCode
      : "external_review_submission_failed";
  const message = error instanceof Error ? error.message : String(error);
  const status =
    reasonCode === "external_review_submission_not_found"
      ? 404
      : reasonCode === "deployment_role_mismatch" ||
          reasonCode === "review_submission_duplicate_payload_mismatch" ||
          reasonCode === "external_review_recovery_revision_conflict" ||
          reasonCode === "external_review_recovery_not_applicable"
        ? 409
        : reasonCode.startsWith("invalid_")
          ? 400
          : 500;
  return failure(status, requestIdValue, {
    code:
      status === 404
        ? "not_found"
        : status === 409
          ? "conflict"
          : status === 400
            ? "invalid_input"
            : "internal_error",
    reason_code: reasonCode,
    message,
    retryable: status >= 500,
  });
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

function scheduledHostExecutorContext(
  state: ServiceState,
): Parameters<typeof runScheduledHostExecutors>[0] {
  return scheduledHostExecutorContextFromModule(backgroundReviewContext(state));
}

function backgroundReviewContext(
  state: ServiceState,
): ServiceBackgroundReviewContext {
  return {
    bridge: state.bridge,
    get runtimeConfig() {
      return state.runtimeConfig;
    },
    diagnostics: () => buildDiagnosticsContext(state),
    loadProfileContext: (profileId) =>
      loadServiceProfileContext({
        bridge: state.bridge,
        profilesDir: state.runtimeConfig.profilesDir,
        skillsDir: state.runtimeConfig.skillsDir,
        profileId,
        modelProviderResolver: (alias) =>
          resolveModelProviderForBrain(state.bridge, alias),
      }),
    buildAdapterDiagnostics: (now) =>
      buildServiceAdapterDiagnostics(state, now),
    denMemoryConfigured: () => Boolean(state.config.denMemory.baseUrl),
    now: state.now,
    updateBackgroundReviewState: (update) => {
      if ("lastRunAt" in update)
        state.backgroundReview.lastRunAt = update.lastRunAt;
      if ("lastError" in update)
        state.backgroundReview.lastError = update.lastError;
      if ("recentFindings" in update) {
        state.backgroundReview.recentFindings = update.recentFindings ?? 0;
      }
      if ("lastCaptureProposalCount" in update) {
        state.backgroundReview.lastCaptureProposalCount =
          update.lastCaptureProposalCount;
      }
      if ("lastPersistedCaptureProposalCount" in update) {
        state.backgroundReview.lastPersistedCaptureProposalCount =
          update.lastPersistedCaptureProposalCount;
      }
      if ("lastSkippedReasons" in update) {
        state.backgroundReview.lastSkippedReasons = update.lastSkippedReasons;
      }
    },
    recordEvent: (event) => recordServiceEvent(state, event),
  };
}

async function persistSessionActivityDigest(input: {
  state: ServiceState;
  session: SessionState;
  wakeId: string;
  source: ServiceWakeSource;
  observedEvents: readonly CoreEvent[];
  completionSummary?: string;
}): Promise<void> {
  return persistSessionActivityDigestFromModule({
    context: wakeMaintenanceContext(input.state),
    session: input.session,
    wakeId: input.wakeId,
    source: input.source,
    observedEvents: input.observedEvents,
    completionSummary: input.completionSummary,
  });
}

async function runPostTurnMaintenance(input: {
  state: ServiceState;
  session: SessionState;
  profileContext: WakeProfileContext;
  wakeId: string;
  source: ServiceWakeSource;
  observedEvents: readonly CoreEvent[];
  completionSummary?: string;
}): Promise<void> {
  return runPostTurnMaintenanceFromModule({
    context: wakeMaintenanceContext(input.state),
    session: input.session,
    profileContext: input.profileContext,
    wakeId: input.wakeId,
    source: input.source,
    observedEvents: input.observedEvents,
    completionSummary: input.completionSummary,
  });
}

async function archiveServiceSession(
  state: ServiceState,
  sessionId: SessionId,
): Promise<void> {
  await state.bridge.archiveSession(sessionId);
  await closeBrowserSessionForServiceLifecycle(state, sessionId);
}

async function closeBrowserSessionForServiceLifecycle(
  state: ServiceState,
  sessionId: SessionId,
): Promise<void> {
  const cleanup = await closeServiceBrowserSessionForLifecycle({
    resources: state.browserResources,
    sessionId,
    reason: "session_archived",
  });
  if (!cleanup.closed) return;
  recordServiceEvent(state, {
    source: "browser-session-manager",
    eventType: "browser_session_closed",
    summary: `Closed browser session for archived runtime session ${sessionId}.`,
    resultRef: {
      sessionId,
      reason: cleanup.reason,
    },
  });
}

async function dispatchWake(
  state: ServiceState,
  event: Extract<CoreEvent, { type: "brain_wake_requested" }>,
  source: ServiceWakeSource,
  observationContext?: ServiceWakeObservationContext,
  options: { appendChatEvents?: boolean } = {},
): Promise<ServiceWakeDispatchReport> {
  return dispatchWakeFromModule(
    wakeDispatchContext(state),
    event,
    source,
    observationContext,
    options,
  );
}

function wakeDispatchContext(state: ServiceState): ServiceWakeDispatchContext {
  return {
    bridge: state.bridge,
    inFlightWakes: state.inFlightWakes,
    deferredWakeSessions: state.deferredWakeSessions,
    toolCallDebugStore: state.toolCallDebugStore,
    brainForProfile: (profileId) =>
      state.runtimeConfigApplyResult.brainHandlesByProfileId[profileId],
    configuredSessionForRuntimeSession: (session) =>
      configuredSessionForRuntimeSession(state.runtimeConfig, session),
    loadProfileContext: (profileId) =>
      loadServiceProfileContext({
        bridge: state.bridge,
        profilesDir: state.runtimeConfig.profilesDir,
        skillsDir: state.runtimeConfig.skillsDir,
        profileId,
        modelProviderResolver: (alias) =>
          resolveModelProviderForBrain(state.bridge, alias),
      }),
    nextWakeId: (session) => nextWakeId(state, session),
    prepareContextStrategy: (input) =>
      prepareContextStrategyForWake(state, input),
    roleplayPromptContextForSession: (session) =>
      roleplayPromptContextForSession(roleplayRouteContext(state), session),
    appendChatEvent: (sessionId, event) =>
      appendChatEventFromModule(chatEventLogContext(state), sessionId, event),
    listChatEventsAfterCursor: (session, cursor, limit) =>
      listChatEventsAfterCursorFromModule(
        chatEventLogContext(state),
        session,
        cursor,
        limit,
      ),
    publishWakeToolActivity: (input) =>
      publishWakeToolActivity({ state, ...input }),
    runPostTurnMaintenance: (input) =>
      runPostTurnMaintenance({ state, ...input }),
    persistSessionActivityDigest: (input) =>
      persistSessionActivityDigest({ state, ...input }),
    runtimePauseForSession: (session) => runtimePauseForSession(state, session),
    deferRuntimeActivitySettlement: (settlement) => {
      state.deferredRuntimeActivitySettlements.defer(settlement);
    },
    recordEvent: (event) => recordServiceEvent(state, event),
    now: state.now,
  };
}

async function reconcileDeferredRuntimeActivitySettlements(
  state: ServiceState,
): Promise<number> {
  const report = await state.deferredRuntimeActivitySettlements.reconcile(
    state.bridge,
  );
  for (const wakeId of report.reconciledWakeIds) {
    recordServiceEvent(state, {
      source: "service-host",
      eventType: "runtime_activity_wake_settlement_reconciled",
      severity: "warning",
      summary: `Reconciled deferred runtime activity settlement for wake ${wakeId}.`,
    });
  }
  if (report.failure !== undefined) {
    recordServiceEvent(state, {
      source: "service-host",
      eventType: "runtime_activity_wake_settlement_retry_failed",
      severity: "warning",
      summary: errorMessage(
        report.failure.error,
        `deferred runtime activity settlement retry failed for wake ${report.failure.wakeId}`,
      ),
    });
  }
  return report.reconciledWakeIds.length;
}

function wakeMaintenanceContext(
  state: ServiceState,
): ServiceWakeMaintenanceContext {
  return {
    get denGatewayClient() {
      return state.denGatewayClient;
    },
    now: state.now,
    saveSessionActivityDigest: async (digest) => {
      await state.bridge.saveSessionActivityDigest(digest);
    },
    upsertCuratorBatch: async (batch, mutations) => {
      state.curator.store.upsertBatch(batch, mutations);
      await state.curator.store.persist();
    },
    setCuratorLastRunAt: (value) => {
      state.curator.lastRunAt = value;
    },
    mutationForCuratorCandidate: (candidate) =>
      mutationForServiceCuratorCandidate(candidate),
    recordEvent: (event) => recordServiceEvent(state, event),
  };
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

async function publishWakeToolActivity(input: {
  state: ServiceState;
  session: SessionState;
  wakeId: string;
  events: readonly CoreEvent[];
  observationContext?: ServiceWakeObservationContext;
}): Promise<void> {
  return publishWakeToolActivityFromModule({
    context: wakeMaintenanceContext(input.state),
    session: input.session,
    wakeId: input.wakeId,
    events: input.events,
    observationContext: input.observationContext,
  });
}

function runtimePauseWakeReport(
  state: ServiceState,
  sessionId: SessionId,
  pause: RuntimePauseRecord,
): ServiceWakeDispatchReport {
  return runtimePauseWakeReportFromModule(
    wakeDispatchContext(state),
    sessionId,
    pause,
  );
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
    await stopTelegramConnectorFromModule(adapterLifecycleContext(state));
    if (state.denObservationSubscription !== undefined) {
      await state.bridge
        .unsubscribeEvents(state.denObservationSubscription)
        .catch(() => undefined);
      state.denObservationSubscription = undefined;
    }
    await state.bridge
      .unsubscribeEvents(state.wakeSubscription)
      .catch(() => undefined);
    await state.mcpManager.shutdown();
    await state.externalRuntimeController.stop();
    const browserCleanup = await closeAllServiceBrowserSessionsForLifecycle({
      resources: state.browserResources,
      reason: "service_shutdown",
    });
    if (browserCleanup.closed > 0) {
      recordServiceEvent(state, {
        source: "browser-session-manager",
        eventType: "browser_sessions_closed",
        summary: `Service shutdown closed ${browserCleanup.closed} browser session(s).`,
        resultRef: {
          closed: browserCleanup.closed,
          reasons: browserCleanup.reasons,
        },
      });
    }
    const bufferedCleanup = await state.bridge
      .cleanupBufferedBrainRuns({
        reasonCode: "service_shutdown",
        summary: "service shutdown cleaned up active buffered brain runs",
      })
      .catch(() => undefined);
    if (bufferedCleanup !== undefined && bufferedCleanup.removed_runs > 0) {
      recordServiceEvent(state, {
        source: "native-bridge",
        eventType: "buffered_brain_run_cleanup",
        severity:
          bufferedCleanup.cancelled_nonterminal_runs > 0 ? "warning" : "info",
        summary: `Cleaned up ${bufferedCleanup.removed_runs} buffered brain run(s); cancelled ${bufferedCleanup.cancelled_nonterminal_runs} nonterminal run(s).`,
        resultRef: {
          activeRuns: bufferedCleanup.active_runs,
          terminalRuns: bufferedCleanup.terminal_runs,
          removedRuns: bufferedCleanup.removed_runs,
          cancelledNonterminalRuns: bufferedCleanup.cancelled_nonterminal_runs,
        },
      });
    }
    await state.bridge.shutdownEngine({
      engine: state.engine,
      drainTimeoutMs: 5_000,
    });
  } finally {
    state.lock.release();
  }
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
    contextWindowTokens: provider.contextWindowTokens,
    temperatureMilli: provider.temperatureMilli,
    maxOutputTokens: provider.maxOutputTokens,
    reasoningEffort: provider.reasoningEffort,
    reasoningFormat: provider.reasoningFormat,
    responsesDialect: provider.responsesDialect,
    chatCompletionsDialect: provider.chatCompletionsDialect,
    thinkingMode: provider.thinkingMode,
    reasoningHistory: provider.reasoningHistory,
    reasoningBudgetTokens: provider.reasoningBudgetTokens,
    promptCaching: provider.promptCaching,
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

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback;
}
