import type {
  AgentMessage as RustyAgentMessage,
  BodyState,
  BrainAction,
  BrainEvent,
  BrainEventEnvelope,
  BrainImplementationHandle,
  BrainImplementationRegistration,
  CompletionPacket,
  SessionId,
} from "@rusty-crew/contracts";
import type {
  BrainWakeExecutor,
  NativeBridgeModule,
} from "@rusty-crew/native-bridge";
import { wakeBrainFromBridgeRequest } from "./bridge-wake.js";

export interface BrainRoleAssembly {
  instructions?: string;
  initialMessages?: RustyAgentMessage[];
}

export interface BrainWakeInput {
  wakeId: string;
  sessionId: SessionId;
  state: BodyState;
  systemPrompt: string;
  roleAssembly: BrainRoleAssembly;
}

export interface BrainWakeResult {
  events: BrainEventEnvelope[];
  actions: BrainAction[];
}

export interface BrainImplementation {
  wake(input: BrainWakeInput): Promise<BrainWakeResult>;
}

export function createBrainWakeExecutor(
  brain: BrainImplementation,
): BrainWakeExecutor {
  return {
    wake(request, buffers): Promise<BrainWakeResult> {
      return wakeBrainFromBridgeRequest(buffers, brain, request);
    },
  };
}

export function registerBrainImplementationRuntime(
  bridge: NativeBridgeModule,
  registration: BrainImplementationRegistration,
  brain: BrainImplementation,
): Promise<BrainImplementationHandle> {
  return bridge.registerBrainRuntime(
    registration,
    createBrainWakeExecutor(brain),
  );
}

export type BrainActionPlanner = (input: {
  wake: BrainWakeInput;
  events: BrainEventEnvelope[];
  toolActions?: readonly BrainAction[];
}) => Promise<BrainAction[]> | BrainAction[];

export function createLocalBrain(
  planner: BrainActionPlanner = defaultActionPlanner,
): BrainImplementation {
  return {
    async wake(input): Promise<BrainWakeResult> {
      const events = [
        envelope(input, { type: "started" }),
        envelope(input, {
          type: "text_delta",
          text: `local brain woke ${input.state.session.agentId}`,
        }),
        envelope(input, { type: "finished" }),
      ];

      return {
        events,
        actions: await planner({ wake: input, events }),
      };
    },
  };
}

export const createPlaceholderBrain = createLocalBrain;

function defaultActionPlanner({
  wake,
}: {
  wake: BrainWakeInput;
}): BrainAction[] {
  return [
    {
      type: "deliver_completion",
      packet: {
        sessionId: wake.sessionId,
        status: "completed",
        summary: "local brain smoke wake completed",
      } satisfies CompletionPacket,
    },
  ];
}

export function envelope(
  input: BrainWakeInput,
  event: BrainEvent,
): BrainEventEnvelope {
  return {
    wakeId: input.wakeId,
    sessionId: input.sessionId,
    event,
  };
}

export type {
  PiAgentBrainOptions,
  PiAgentFactory,
  PiAgentLike,
} from "./pi-agent-brain.js";
export { createPiAgentBrain } from "./pi-agent-brain.js";
export {
  acquireRustyCrewServiceLock,
  ensureRustyCrewServiceDirectories,
  loadRustyCrewServiceConfig,
  RUSTY_CREW_DEFAULT_ADMIN_HOST,
  RUSTY_CREW_DEFAULT_ADMIN_PORT,
  RUSTY_CREW_DEFAULT_DATA_DIR,
  validateRustyCrewServiceConfig,
} from "./service-config.js";
export type {
  RustyCrewBackgroundConfig,
  RustyCrewAdminConfig,
  RustyCrewServiceConfig,
  RustyCrewServiceEnv,
  RustyCrewServiceLock,
  RustyCrewServicePaths,
} from "./service-config.js";
export { startRustyCrewServiceHost } from "./service-host.js";
export type {
  RustyCrewServiceHost,
  RustyCrewServiceHostOptions,
} from "./service-host.js";
export {
  combineResolvers,
  resolveToolSession,
} from "./tool-session-selection.js";
export type {
  PiAgentToolResolver,
  ToolSessionSelection,
  ToolSessionSelectionInput,
  ToolSessionSelectionItem,
  ToolSessionSelectionStatus,
} from "./tool-session-selection.js";
export type { BridgeBufferClient } from "./bridge-wake.js";
export { wakeBrainFromBridgeRequest } from "./bridge-wake.js";
export {
  BodyControlledDeltaQueue,
  defaultBodyDeltaPolicy,
} from "./mid-turn-delta.js";
export type { DrainResult, QueuedMidTurnMessage } from "./mid-turn-delta.js";
export {
  adapterActivity,
  adminCommandActivity,
  AgentActivityObservationProducer,
  createAgentActivityObservationEvent,
  createMemoryAgentActivityObservationSink,
  sessionActivity,
  toolActivity,
  workActivity,
} from "./agent-activity-observation.js";
export type {
  AgentActivityEventInput,
  AgentActivityEventType,
  AgentActivityObservationEvent,
  AgentActivityObservationSink,
  AgentActivityPayload,
  AgentActivityPublishResult,
  AgentActivityResultRef,
  AgentActivitySeverity,
  AgentActivityVisibility,
  AgentActivityWorkRef,
  AgentObservationIdentity,
  MemoryAgentActivityObservationSink,
  ObservationSourceDomain,
} from "./agent-activity-observation.js";
export {
  createRuntimeActivityObserver,
  RuntimeActivityObserver,
} from "./runtime-activity-observer.js";
export type {
  RuntimeActivityObserverOptions,
  RuntimeActivityResult,
  RuntimeAdapterActivityInput,
  RuntimeSessionActivityInput,
  RuntimeToolActivityInput,
  RuntimeWorkActivityInput,
} from "./runtime-activity-observer.js";
export { publishBackgroundGovernanceObservation } from "./background-governance-observation.js";
export type {
  BackgroundGovernanceLoopKind,
  BackgroundGovernanceObservationInput,
  BackgroundGovernancePhase,
} from "./background-governance-observation.js";
export { buildBackgroundServiceDiagnosticsProjection } from "./background-service-diagnostics.js";
export type {
  BackgroundReviewDiagnostics,
  BackgroundServiceDiagnosticsInput,
  BackgroundServiceDiagnosticsProjection,
  BackgroundServiceHealth,
  BackgroundServiceIssue,
  CleanupBackgroundDiagnostics,
  CuratorBackgroundDiagnostics,
  SchedulerBackgroundDiagnostics,
} from "./background-service-diagnostics.js";
export { createBackgroundAdminControlExecutor } from "./background-admin-control.js";
export type {
  BackgroundAdminControlOptions,
  SchedulerAdminControlOptions,
} from "./background-admin-control.js";
export {
  createDenRouterPiAgentFactory,
  resolveDenRouterModel,
} from "./den-router-agent.js";
export type {
  DenRouterAgentOptions,
  DenRouterModelSelection,
} from "./den-router-agent.js";
export {
  buildDelegatedRoleAssembly,
  normalizeDelegatedRole,
} from "./delegated-role-assembly.js";
export type {
  BuildDelegatedRoleAssemblyInput,
  DelegatedProfileData,
  DelegatedRole,
  DelegatedRoleInput,
  DelegationRoleContext,
} from "./delegated-role-assembly.js";
export {
  CronExpression,
  CronExpressionError,
  nextCronDueAt,
} from "./cron-expression.js";
export type { CronFieldRange, CronNextOptions } from "./cron-expression.js";
export { parseCronArgs, runRustyCrewCronCli } from "./cron-cli.js";
export type { CronCliCommand, CronCliOptions } from "./cron-cli.js";
export {
  executeScheduledHostRun,
  RUNTIME_DIAGNOSTICS_SNAPSHOT_JOB_KIND,
  runScheduledHostExecutors,
  scheduledHostJobKinds,
} from "./scheduled-host-executors.js";
export type {
  ScheduledHostExecutorContext,
  ScheduledHostExecutorReport,
} from "./scheduled-host-executors.js";
export {
  gitDiffTool,
  gitStatusTool,
  readFileTool,
  resolveLocalCodeTools,
  searchFilesTool,
  terminalTool,
  writeFileTool,
} from "./local-code-tools.js";
export type {
  LocalToolContext,
  LocalToolProcessResult,
} from "./local-code-tools.js";
export {
  delegationTools,
  fanOutSubagentsTool,
  findRelevantPathsTool,
  resolveDelegationTools,
  scoutCodebaseTool,
  spawnSubagentTool,
  summarizeFilesTool,
} from "./delegation-tools.js";
export type {
  DelegationToolContext,
  DelegationToolDetails,
} from "./delegation-tools.js";
export {
  createDenMemoryToolResolver,
  denMemoryProposeTool,
  denMemoryReadTool,
  denMemoryRecallTool,
  denMemorySearchTool,
  denMemoryStoreTool,
  resolveDenMemoryTools,
} from "./den-memory-tools.js";
export type {
  DenMemoryPolicyMode,
  DenMemoryToolContext,
  DenMemoryToolDetails,
  DenMemoryToolPolicy,
} from "./den-memory-tools.js";
export {
  assertSafePublicUrl,
  createWebSearchProvider,
  createWebToolResolver,
  resolveWebTools,
  webExtractTool,
  webSearchTool,
} from "./web-tools.js";
export type {
  ResolveHostAddresses,
  ResolvedAddress,
  WebExtractResult,
  WebExtractToolContext,
  WebExtractToolDetails,
  WebNetworkPolicy,
  WebSearchProvider,
  WebSearchResult,
  WebSearchToolContext,
  WebSearchToolDetails,
} from "./web-tools.js";
export {
  createSkillsToolResolver,
  resolveSkillsTools,
  skillManageTool,
  skillsListTool,
  skillViewTool,
} from "./skills-tools.js";
export type {
  SkillManageAction,
  SkillManagementResult,
  SkillManageMode,
  SkillListItem,
  SkillsToolContext,
  SkillsToolDetails,
} from "./skills-tools.js";
export {
  createDenseProfileMemoryToolResolver,
  denseProfileMemoryTool,
} from "./dense-profile-memory-tool.js";
export type {
  DenseProfileMemoryAction,
  DenseProfileMemoryMode,
  DenseProfileMemoryToolContext,
  DenseProfileMemoryToolDetails,
} from "./dense-profile-memory-tool.js";
export {
  channelReadbackTool,
  curatorExecuteTool,
  counterResetTool,
  MemorySessionTodoStore,
  renderSessionTodoContext,
  sessionSearchTool,
  todoTool,
} from "./planning-tools.js";
export type {
  ChannelReadbackClient,
  ChannelReadbackToolContext,
  ChannelReadbackToolDetails,
  CounterResetToolContext,
  CounterResetToolDetails,
  CounterResetTriggerType,
  CuratorExecuteAction,
  CuratorExecuteContext,
  CuratorExecuteReceipt,
  CuratorExecuteRequest,
  CuratorExecuteToolDetails,
  CuratorExecutionStatus,
  CuratorScopeType,
  MemorySessionTodoStoreOptions,
  SessionSearchResult,
  SessionSearchToolContext,
  SessionSearchToolDetails,
  SessionTodoState,
  SessionTodoStore,
  TodoItem,
  TodoStatus,
  TodoToolContext,
  TodoToolDetails,
} from "./planning-tools.js";
export { patchTool } from "./patch-tool.js";
export {
  buildToolRegistryDiagnostics,
  formatToolRegistryDiagnosticsMarkdown,
} from "./tool-registry-diagnostics.js";
export type {
  ToolRegistryDiagnosticTool,
  ToolRegistryDiagnosticsInput,
  ToolRegistryDiagnosticsReport,
  ToolRegistryDiagnosticsSummary,
} from "./tool-registry-diagnostics.js";
export {
  integrateMcpToolsWithRegistry,
  mcpCandidateToRegistryEntry,
} from "./mcp-tool-registry-integration.js";
export type {
  McpNameCollisionPolicy,
  McpRegistryIntegrationInput,
  McpRegistryIntegrationReport,
  McpToolRegistryEntry,
} from "./mcp-tool-registry-integration.js";
export { reloadMcpSurface } from "./mcp-surface-reload.js";
export type {
  McpSurfaceReloadInput,
  McpSurfaceReloadReport,
  McpToolDiff,
} from "./mcp-surface-reload.js";
export {
  createMcpToolCallMetadata,
  createMcpToolFinishedEvent,
  createMcpToolStartedEvent,
  evaluateMcpResourceHooks,
} from "./mcp-tool-telemetry.js";
export type {
  McpResourceDenialReason,
  McpResourceHookDecision,
  McpResourceHookInput,
  McpToolTelemetryInput,
} from "./mcp-tool-telemetry.js";
export {
  createWebBrowserToolCallMetadata,
  createWebBrowserToolFinishedEvent,
  createWebBrowserToolStartedEvent,
  evaluateWebBrowserResourceHooks,
  webBrowserToolSource,
} from "./web-browser-tool-telemetry.js";
export type {
  WebBrowserResourceDenialReason,
  WebBrowserResourceHookDecision,
  WebBrowserResourceHookInput,
  WebBrowserToolTelemetryInput,
} from "./web-browser-tool-telemetry.js";
export {
  buildWebBrowserDiagnostics,
  cleanupWebBrowserCapabilities,
} from "./web-browser-diagnostics.js";
export type {
  BrowserCapabilityDiagnostics,
  WebBrowserDiagnostics,
  WebBrowserDiagnosticsInput,
  WebDiagnosticsInput,
  WebProviderDiagnostics,
} from "./web-browser-diagnostics.js";
export { buildAdapterDiagnosticsProjection } from "./adapter-diagnostics.js";
export type {
  AdapterDiagnosticsInput,
  AdapterDiagnosticsProjection,
  AdapterHealthStatus,
  ChannelAdapterBindingDiagnostics,
  ChannelProjectionFailureRecord,
  McpAdapterSurfaceDiagnostics,
} from "./adapter-diagnostics.js";
export { buildRuntimeDiagnosticsProjection } from "./runtime-diagnostics.js";
export type {
  DiagnosticsHealth,
  DiagnosticsIssue,
  DiagnosticsReasonCode,
  ObservationDiagnosticsInput,
  ObservationDiagnosticsProjection,
  PersistenceDiagnosticsInput,
  PersistenceDiagnosticsProjection,
  QueueDiagnosticsInput,
  QueueDiagnosticsProjection,
  RuntimeCounterSummary,
  RuntimeDelegationDiagnostics,
  RuntimeDiagnosticError,
  RuntimeDiagnosticsInput,
  RuntimeDiagnosticsProjection,
  RuntimeSessionDiagnostics,
  ToolDiagnosticsProjection,
} from "./runtime-diagnostics.js";
export { buildRuntimeHealthProjection, issueDomain } from "./runtime-health.js";
export type {
  RuntimeDegradedStatus,
  RuntimeHealthDomain,
  RuntimeHealthDomainStatus,
  RuntimeHealthProbe,
  RuntimeHealthProjection,
  RuntimeMetricSample,
  RuntimeReadinessProbe,
} from "./runtime-health.js";
export {
  buildToolContextDiagnosticsReport,
  formatToolContextDiagnosticsMarkdown,
} from "./tool-context-diagnostics.js";
export {
  discoverCuratorCandidates,
  renderCuratorCandidateReport,
} from "./curator-candidates.js";
export type {
  CuratorCandidate,
  CuratorCandidateBatch,
  CuratorCandidateDiscoveryInput,
  CuratorCandidateKind,
  CuratorObservedBehaviorEvidence,
  CuratorCandidateSourceRef,
  CuratorCandidateStatus,
} from "./curator-candidates.js";
export { runCuratorLifecycleTransitions } from "./curator-lifecycle.js";
export type {
  CuratorLifecyclePolicy,
  CuratorLifecycleReport,
  CuratorLifecycleTransition,
} from "./curator-lifecycle.js";
export {
  listCuratorArchivedSkills,
  listCuratorPinnedSkills,
  pinCuratorSkill,
  restoreCuratorArchivedSkill,
  unpinCuratorSkill,
} from "./curator-skill-admin.js";
export type {
  CuratorArchivedSkill,
  CuratorPinnedSkill,
  CuratorSkillPinResult,
  CuratorSkillRestoreResult,
  CuratorSkillUnpinResult,
} from "./curator-skill-admin.js";
export {
  createCuratorGovernanceExecutor,
  curatorSkillSourceRef,
  executeCuratorGovernanceRequest,
  FileCuratorGovernanceStore,
  MemoryCuratorGovernanceStore,
  rollbackCuratorMutation,
} from "./curator-mutations.js";
export type {
  CuratorApprovalRecord,
  CuratorCandidateLifecycle,
  CuratorCandidateLifecycleState,
  CuratorGovernanceStoreSnapshot,
  CuratorGovernanceExecutorOptions,
  CuratorMutationCandidate,
  CuratorMutationOperation,
  CuratorMutationRecord,
  CuratorMutationStatus,
  CuratorStoredCandidate,
  CuratorStoredCandidateStatus,
  CuratorSnapshotRef,
} from "./curator-mutations.js";
export { createCuratorAdminControlExecutor } from "./curator-admin-control.js";
export type {
  CuratorAdminControlOptions,
  CuratorAdminStatus,
} from "./curator-admin-control.js";
export { runDelegatedResourceCleanup } from "./delegated-resource-cleanup.js";
export type {
  AdapterCleanupResult,
  DelegatedResourceAdapterCleanup,
  DelegatedResourceCleanupInput,
  DelegatedResourceCleanupResult,
} from "./delegated-resource-cleanup.js";
export { runBackgroundMemorySkillReview } from "./background-memory-skill-review.js";
export type {
  BackgroundReviewCandidateKind,
  BackgroundReviewDenseMemoryRecord,
  BackgroundReviewFinding,
  BackgroundReviewPayload,
  BackgroundReviewResult,
  BackgroundReviewResultRef,
  BackgroundReviewRunnerInput,
  BackgroundReviewSeverity,
  BackgroundReviewSourceRef,
  BackgroundReviewType,
} from "./background-memory-skill-review.js";
export type {
  TextSurfaceSummary,
  ToolContextAdapterSummary,
  ToolContextAssemblySummary,
  ToolContextDiagnosticTool,
  ToolContextDiagnosticsInput,
  ToolContextDiagnosticsIssue,
  ToolContextDiagnosticsReport,
  ToolContextDiagnosticsSession,
  ToolContextMemorySkillsPlanningInput,
  ToolContextMemorySkillsPlanningSummary,
  ToolContextDiagnosticsSummary,
  ToolContextPolicySummary,
  ToolContextReasonCode,
  ToolContextResourceSummary,
  ToolContextSkillSummary,
  ToolContextToolStatus,
  DenMemoryDiagnosticsInput,
  DenseProfileMemoryDiagnosticsInput,
  RuntimeCounterDiagnosticsInput,
  SessionSearchDiagnosticsInput,
  SkillRootDiagnosticsInput,
  TodoDiagnosticsInput,
} from "./tool-context-diagnostics.js";
export {
  inspectDirectDebugSession,
  requestDirectDebugTurn,
} from "./direct-debug-service.js";
export type {
  DirectDebugContextView,
  DirectDebugControlSummary,
  DirectDebugErrorCode,
  DirectDebugInspectRequest,
  DirectDebugMessageSummary,
  DirectDebugRecentEventSummary,
  DirectDebugResult,
  DirectDebugRuntimeSummary,
  DirectDebugServiceContext,
  DirectDebugSessionSource,
  DirectDebugSessionSummary,
  DirectDebugSessionView,
  DirectDebugTurnExecutor,
  DirectDebugTurnExecutorInput,
  DirectDebugTurnOutcome,
  DirectDebugTurnRequest,
} from "./direct-debug-service.js";
export {
  createDebugApiClient,
  DebugApiClientError,
} from "./debug-api-client.js";
export type {
  DebugApiClient,
  DebugApiClientOptions,
  DebugApiFetch,
  DebugApiQuery,
  DebugDiagnosticsBundle,
  DirectDebugContextRequest,
} from "./debug-api-client.js";
export {
  BrowserSessionManager,
  createChromiumBrowserLauncher,
} from "./browser-session-manager.js";
export type {
  BrowserCleanupSummary,
  BrowserCloseReason,
  BrowserLaunchInput,
  BrowserLauncher,
  BrowserLaunchResult,
  BrowserManagerDiagnostics,
  BrowserManagerOptions,
  BrowserOpenInput,
  BrowserProcessHandle,
  BrowserRefEntry,
  BrowserResolvedRef,
  BrowserSessionDiagnostics,
  BrowserSessionHandle,
  BrowserSessionLimits,
  BrowserSessionState,
  BrowserSnapshot,
  CdpConnection,
} from "./browser-session-manager.js";
export {
  browserBackTool,
  browserClickTool,
  browserConsoleTool,
  browserNavigateTool,
  browserPressTool,
  browserScrollTool,
  browserSnapshotTool,
  browserTypeTool,
  browserVisionTool,
  createBrowserToolResolver,
  MemoryBrowserScreenshotStore,
  resolveBrowserTools,
} from "./browser-tools.js";
export type {
  BrowserActionDetails,
  BrowserConsoleDetails,
  BrowserConsoleExpression,
  BrowserScreenshotArtifact,
  BrowserScreenshotStore,
  BrowserSnapshotDetails,
  BrowserToolContext,
  BrowserToolResolverContext,
  BrowserVisionDetails,
} from "./browser-tools.js";
export { handleAdminDiagnosticsRequest } from "./admin-diagnostics-api.js";
export type {
  AdminAgentDiagnostics,
  AdminApiEnvelope,
  AdminApiMeta,
  AdminDiagnosticsContext,
  AdminDiagnosticsRouteRequest,
  AdminErrorCode,
  AdminPage,
  AdminRecentEvent,
  AdminRouteResult,
} from "./admin-diagnostics-api.js";
export {
  createMemoryAdminControlAuditSink,
  handleAdminControlRequest,
} from "./admin-control-api.js";
export type {
  AdminControlActor,
  AdminControlAuditEvent,
  AdminControlAuditSink,
  AdminControlAuthConfig,
  AdminControlCommand,
  AdminControlCommandName,
  AdminControlContext,
  AdminControlExecutor,
  AdminControlOutcome,
  AdminControlResponse,
  AdminControlRouteRequest,
  AdminControlStatus,
  MemoryAdminControlAuditSink,
} from "./admin-control-api.js";
export { routeSlashCommand } from "./slash-command-router.js";
export type {
  SlashCommandActor,
  SlashCommandControlRequest,
  SlashCommandInput,
  SlashCommandName,
  SlashCommandResponse,
  SlashCommandRouteResult,
  SlashCommandRouterOptions,
  SlashCommandSession,
  SlashCommandStatus,
} from "./slash-command-router.js";
export { buildReadOnlySlashCommandResponse } from "./slash-command-responses.js";
export type { SlashCommandResponseContext } from "./slash-command-responses.js";
export { createNewSessionLifecycleExecutor } from "./new-session-lifecycle.js";
export type {
  NewSessionLifecycleAuditEvent,
  NewSessionLifecycleAuditSink,
  NewSessionLifecycleOptions,
  NewSessionLifecyclePhase,
  NewSessionTemplate,
} from "./new-session-lifecycle.js";
export { createReloadMcpControlExecutor } from "./reload-mcp-control.js";
export type {
  ReloadMcpControlOptions,
  ReloadMcpLifecycleAuditEvent,
  ReloadMcpLifecycleAuditSink,
  ReloadMcpLifecyclePhase,
} from "./reload-mcp-control.js";
export {
  assertValidToolRegistry,
  buildToolInventory,
  createToolRegistry,
  defaultToolRegistry,
  toToolDescriptor,
  validateToolRegistry,
  ToolRegistry,
} from "./tool-registry.js";
export type {
  ToolCategory,
  ToolDeprecation,
  ToolInventory,
  ToolInventoryItem,
  ToolInventoryRequest,
  ToolInventoryStatus,
  ToolRegistryEntry,
  ToolRegistryValidation,
  ToolRegistryValidationIssue,
  ToolSafetyFlag,
  ToolSurface,
} from "./tool-registry.js";
export {
  buildBrainRegistrationFromToolProfile,
  createToolCatalogChangedPayload,
  selectToolProfile,
} from "./tool-profile-selection.js";
export type {
  BrainRegistrationFromToolProfileInput,
  ProfileToolPolicy,
  SessionToolConstraints,
  ToolProfileSelection,
  ToolProfileSelectionInput,
} from "./tool-profile-selection.js";
export {
  loadProfileConfig,
  loadProfileContext,
  loadSkill,
  ProfileLoadError,
} from "./profile-loading.js";
export type {
  LoadedProfileContext,
  LoadedSkill,
  LoadProfileContextInput,
  ProfileConfig,
  ProfileLoadErrorCode,
  ProfilePromptFragments,
  ProfileRuntimeConfig,
} from "./profile-loading.js";
export {
  buildProfileRoleAssembly,
  renderDenMemoryContext,
  renderDenseProfileMemoryContext,
  renderPlanningContext,
} from "./profile-role-assembly.js";
export type {
  BuildProfileRoleAssemblyOptions,
  DenMemoryPromptContext,
  DenMemoryPromptMode,
  DenseProfileMemoryPromptRecord,
  PlanningPromptContext,
  ProfileRoleAssemblyResult,
  RenderDenseProfileMemoryContextOptions,
} from "./profile-role-assembly.js";
