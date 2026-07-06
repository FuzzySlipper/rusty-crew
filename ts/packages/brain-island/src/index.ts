export type {
  BrainActionPlanner,
  BrainImplementation,
  BrainRoleAssembly,
  BrainWakeInput,
  BrainWakeResult,
} from "./local-brain.js";
export {
  createBrainWakeExecutor,
  createLocalBrain,
  createPlaceholderBrain,
  envelope,
  registerBrainImplementationRuntime,
} from "./local-brain.js";
export * from "./package-surface/service.js";
export * from "./package-surface/observation.js";
export * from "./package-surface/diagnostics.js";
export * from "./package-surface/admin.js";
export * from "./package-surface/background.js";
export * from "./package-surface/debug.js";

export type {
  PiAgentBrainOptions,
  PiAgentFactory,
  PiAgentLike,
} from "./pi-agent-brain.js";
export { createPiAgentBrain } from "./pi-agent-brain.js";
export type {
  ToolCallDebugRecord,
  ToolCallDebugStore,
} from "./tool-call-debug-store.js";
export {
  MemoryToolCallDebugStore,
  localToolCallMetadata,
  withToolCallDebugReference,
} from "./tool-call-debug-store.js";
export type {
  ProviderRequestDebugRecord,
  ProviderRequestDebugStore,
} from "./provider-request-debug-store.js";
export { MemoryProviderRequestDebugStore } from "./provider-request-debug-store.js";
export {
  combineResolvers,
  resolveToolSession,
} from "./tool-session-selection.js";
export type {
  BrainToolResolver,
  ToolSessionSelection,
  ToolSessionSelectionInput,
  ToolSessionSelectionItem,
  ToolSessionSelectionStatus,
} from "./tool-session-selection.js";
export type {
  BrainTool,
  BrainToolContent,
  BrainToolContext,
  BrainToolExecutionMode,
  BrainToolResult,
  BrainToolUpdateCallback,
} from "./brain-tool.js";
export {
  createBrainModuleRegistry,
  defaultBrainModules,
  localBrainModule,
  piAgentCoreBrainModule,
  resolveBrainModuleSelection,
} from "./brain-module.js";
export type {
  BrainModule,
  BrainModuleContext,
  BrainModuleDiagnosticsMetadata,
  BrainModuleId,
  BrainModuleRegistry,
  BrainModuleSelection,
  BrainModuleToolAdapterStatus,
} from "./brain-module.js";
export type { BridgeBufferClient } from "./bridge-wake.js";
export { wakeBrainFromBridgeRequest } from "./bridge-wake.js";
export {
  BodyControlledDeltaQueue,
  defaultBodyDeltaPolicy,
} from "./mid-turn-delta.js";
export type { DrainResult, QueuedMidTurnMessage } from "./mid-turn-delta.js";
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
  gitDiffTool,
  gitStatusTool,
  defaultLocalToolWorkdir,
  readFileTool,
  resolveToolPath,
  resolveLocalCodeTools,
  searchFilesTool,
  terminalTool,
  workerPatchTool,
  workerWriteTool,
  writeFileTool,
} from "./local-code-tools.js";
export type {
  LocalToolContext,
  LocalToolProcessResult,
} from "./local-code-tools.js";
export {
  delegationTools,
  fanOutSubagentsTool,
  fanOutSubagentsMarkdownTool,
  findRelevantPathsTool,
  resolveDelegationTools,
  scoutCodebaseTool,
  spawnSubagentMarkdownTool,
  spawnSubagentTool,
  summarizeFilesTool,
} from "./delegation-tools.js";
export type {
  DelegationToolContext,
  DelegationToolDetails,
} from "./delegation-tools.js";
export {
  completionTools,
  deliverCompletionMarkdownTool,
  resolveCompletionTools,
} from "./completion-tools.js";
export type {
  CompletionToolContext,
  CompletionToolDetails,
} from "./completion-tools.js";
export {
  agentRoundTool,
  coordinationTools,
  createCoordinationToolResolver,
  isCorrelatedReply,
  replyFromEvent,
  resolveCoordinationTools,
  sendAgentMessageTool,
} from "./coordination-tools.js";
export type {
  AgentMessageRouteResult,
  AgentRoundResult,
  CoordinationToolContext,
  CoordinationToolDetails,
  CoordinationToolRuntime,
} from "./coordination-tools.js";
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
  captureLoreFactTool,
  createLoreMemoryToolResolver,
  getLoreLayerConfigTool,
  listLoreLayersTool,
  manageLoreLayersTool,
  promoteLoreEntryTool,
  recallLoreTool,
  resolveLoreMemoryTools,
  searchLoreTool,
} from "./lore-memory-tool.js";
export type {
  LoreMemoryToolContext,
  LoreMemoryToolDetails,
  LoreMemoryToolOperation,
} from "./lore-memory-tool.js";
export {
  createSceneStateToolResolver,
  getSceneStateTool,
  resolveSceneStateTools,
  updateSceneStateTool,
} from "./scene-state-tool.js";
export type {
  RoleplaySceneState,
  SceneStateToolContext,
  SceneStateToolDetails,
} from "./scene-state-tool.js";
export {
  createRoleplayNarratorBrain,
  createRoleplayNarratorBrain as createTwoPhaseRoleplayNarratorBrain,
} from "./narrator-brain.js";
export type { RoleplayNarratorBrainOptions } from "./narrator-brain.js";
export {
  channelReadbackTool,
  curatorExecuteTool,
  counterResetTool,
  FileSessionTodoStore,
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
  FileSessionTodoStoreOptions,
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
export {
  captureProposalToMemoryProposal,
  isLegacyDenseMemoryCaptureProposal,
  legacyDenseCaptureProposalToMemoryProposal,
  typedCaptureProposalToMemoryProposal,
} from "./capture-memory-proposals.js";
export type {
  CaptureProducerEvidenceRef,
  CaptureProducerOutput,
  CaptureTargetSpaceId,
  LegacyDenseMemoryCaptureKind,
  LegacyDenseMemoryCaptureProposal,
  TypedCaptureMemoryProposal,
} from "./capture-memory-proposals.js";
export {
  buildSessionActivityDigest,
  sessionActivityDigestId,
} from "./session-activity-digest.js";
export type {
  BuildSessionActivityDigestInput,
  SessionActivitySignalDigest,
  SessionActivityToolCallDigest,
} from "./session-activity-digest.js";
export {
  normalizeCaptureProviderOutput,
  runStructuredCaptureProvider,
} from "./capture-producer-provider.js";
export type {
  CaptureProducerProviderInput,
  CaptureProducerProviderResult,
  CaptureProviderJsonTransport,
} from "./capture-producer-provider.js";
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
export {
  assertValidToolRegistry,
  buildToolInventory,
  createToolRegistry,
  defaultToolRegistry,
  defaultToolExecutableBindings,
  defaultToolRegistryMetadata,
  toToolDescriptor,
  validateToolRegistry,
  ToolRegistry,
} from "./tool-registry.js";
export type {
  ToolCategory,
  ToolDeprecation,
  ToolExecutableBinding,
  ToolInventory,
  ToolInventoryItem,
  ToolInventoryRequest,
  ToolInventoryStatus,
  ToolRegistryMetadata,
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
  loadProfileConfigWithSource,
  loadProfileContext,
  loadSkill,
  ProfileLoadError,
} from "./profile-loading.js";
export type {
  LoadedProfileContext,
  LoadedProfileConfigSource,
  LoadedSkill,
  LoadProfileContextInput,
  ProfileConfig,
  ProfileConfigSourceFormat,
  ProfileLoadErrorCode,
  ProfilePromptFragments,
  ProfileRuntimeConfig,
} from "./profile-loading.js";
export { buildProfileRegistryImportPlan } from "./profile-registry-import.js";
export type {
  BuildProfileRegistryImportPlanInput,
  ProfileRegistryDerivedRuntimeRefDraft,
  ProfileRegistryImportExportMetadataDraft,
  ProfileRegistryImportMode,
  ProfileRegistryImportPlan,
  ProfileRegistryLifecycleStatus,
  ProfileRegistrySourceAssetRefDraft,
  ProfileRegistryWriteDraft,
} from "./profile-registry-import.js";
export {
  buildAdminProfileRegistryDiagnostics,
  filterAdminProfileRegistryRecords,
} from "./profile-registry-admin.js";
export type {
  AdminProfileAssetStatus,
  AdminProfileRegistryAssetStatus,
  AdminProfileRegistryDiagnostics,
  AdminProfileRegistryRecord,
  AdminProfileRegistrySource,
} from "./profile-registry-admin.js";
export {
  buildProfileBundleExportPlan,
  ProfileBundleExportPlanError,
} from "./profile-registry-export.js";
export type {
  BuildProfileBundleExportPlanInput,
  ProfileBundleExportEntry,
  ProfileBundleExportEntryKind,
  ProfileBundleExportPlan,
  ProfileBundleExportSource,
} from "./profile-registry-export.js";
export {
  planCreateProfileWithRust,
  planRuntimeConfigWithRust,
  runtimeConfigValidationInput,
  validateRuntimeConfigWithRust,
} from "./runtime-config-validation.js";
export {
  buildProfileRoleAssembly,
  renderDenMemoryContext,
  renderDenseProfileMemoryContext,
  renderSessionMemoryContext,
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
  RenderSessionMemoryContextOptions,
} from "./profile-role-assembly.js";
export {
  createMemorySpaceToolResolver,
  handleMemorySpaceAdminRequest,
  memorySpaceCatalogTool,
  memorySpaceReadTool,
} from "./memory-space-api.js";
export type {
  MemorySpaceCatalogResult,
  MemorySpaceReadContext,
  MemorySpaceRecordListResult,
  MemorySpaceRecordQuery,
  MemorySpaceRecordReadResult,
  MemorySpaceToolDetails,
} from "./memory-space-api.js";
export {
  contextStrategyCatalog,
  contextStrategyDescriptor,
  contextStrategyPolicyFromPatch,
  contextStrategyPolicyFromUnknown,
  defaultContextStrategyPolicy,
  prepareContextStrategyRoleAssembly,
} from "./context-strategy.js";
export type {
  ContextDebugVisibility,
  ContextStrategyCatalog,
  ContextStrategyDescriptor,
  ContextStrategyId,
  ContextStrategyPolicy,
  ContextStrategyPolicyDiagnostic,
  ContextStrategyRoleAssemblyPreparation,
} from "./context-strategy.js";
export {
  contextTokenBudget,
  estimateApproximateTokens,
  estimateContextUsage,
  textFragmentsFromPayload,
} from "./context-estimate.js";
export type {
  ContextBudgetProvider,
  ContextEstimateInput,
  ContextEstimateQuality,
  ContextTokenBudget,
  ContextUsageEstimate,
} from "./context-estimate.js";
export {
  contextFillPercent,
  evaluateContextCompactionTrigger,
} from "./context-compaction-trigger.js";
export type {
  ContextCompactionAttemptRef,
  ContextCompactionAttemptStatus,
  ContextCompactionDecisionStatus,
  ContextCompactionTriggerDecision,
  ContextCompactionTriggerInput,
} from "./context-compaction-trigger.js";
