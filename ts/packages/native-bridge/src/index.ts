import { existsSync } from "node:fs";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

import {
  bridgeWireShapeFingerprint,
  manifestOperationNames,
} from "@rusty-crew/contracts";

import {
  validateBridgeJsonText,
  validateBridgeValue,
} from "./bridge-validation.js";
import {
  actionBatchReceiptSchema,
  brainActionBatchSchema,
  brainEventEnvelopeSchema,
  brainWakeRequestSchema,
  chatEventLogEventSchema,
  chatEventLogPageSchema,
  chatReadModelPageSchema,
  eventReceiptSchema,
  openAiResponsesBrainRunInputSchema,
  chatCompletionsBrainRunInputSchema,
  providerStateDiagnosticArraySchema,
  rawBodyStateSchema,
  rawBufferedBrainRunDrainSchema,
  rawChannelIngressRoutePlanInputSchema,
  rawChannelIngressRoutePlanSchema,
  rawDenProductIngressPolicyInputSchema,
  rawDenProductIngressPolicyPlanSchema,
  rawModelProviderRefreshImpactSchema,
  rawModelProviderRefreshPlanSchema,
  rawModelProviderRecordArraySchema,
  rawModelProviderRecordSchema,
  rawOpenAiResponsesBrainRunResultSchema,
  rawChatCompletionsBufferedDrainResultSchema,
  rawProfilePurgeReportSchema,
  rawProfileRegistryRecordArraySchema,
  rawProfileRegistryRecordSchema,
  rawSessionStateArraySchema,
} from "./bridge-validation-schemas.js";
import {
  fromCoreConfigWireRuntimeGraphPlan,
  toCoreConfigWireCreateProfilePlanInput,
  toCoreConfigWireRuntimeConfigValidationInput,
  toCoreConfigWireRuntimeGraphPlanInput,
} from "./generated/core-config-facade.js";
import { withGeneratedBridgeOutputValidation } from "./generated-binding-validation.js";
import { withDirectBridgeOutputValidation } from "./direct-binding-validation.js";
import type { NativeBridgeBinding } from "./generated/native-binding-surface.js";
import { executeNativeBrainWake } from "./brain-wake-execution.js";
import { createNativeBridgeMemoryMethods } from "./memory-wrappers.js";
import { createNativeBridgeCuratorMethods } from "./curator-wrappers.js";
import { createNativeBridgeSchedulerMethods } from "./scheduler-wrappers.js";
import { createNativeBridgeRoleplayMethods } from "./roleplay-wrappers.js";
import { createNativeBridgeRoleplayProposalMethods } from "./roleplay-proposal-wrappers.js";
import { createNativeBridgeRoleplayMechanicMethods } from "./roleplay-mechanic-wrappers.js";
import { toSessionState, type RawSessionState } from "./session-wire.js";
import { createNativeBridgeCrewSessionMethods } from "./crew-session-wrappers.js";
import { createNativeBridgeChatMethods } from "./chat-wrappers.js";
import { createNativeBridgeAdminMethods } from "./admin-wrappers.js";
import { createNativeBridgeBrainCatalogMethods } from "./brain-wrappers.js";
import { createNativeBridgeAgentCoordinationMethods } from "./agent-coordination-wrappers.js";
import { createNativeBridgeExternalRuntimeMethods } from "./external-runtime-wrappers.js";
import { createNativeBridgeExternalRuntimeCertificationMethods } from "./external-runtime-certification-wrappers.js";
import {
  assertCanonicalBrainRunModule,
  chatCompletionsTransportMetricsFromRaw,
  toNativeBrainAction,
  toNativeOpenAiResponsesBrainRunInput,
  toNativeChatCompletionsBrainRunInput,
  toNativeProviderStateInput,
  toBrainWakeStreamItem,
  toOpenAiResponsesBrainRunResult,
  toRawOpenAiResponsesBrainRunResult,
  toRawBrainWakeStreamItem,
  toBrainAction,
  toRawBrainWakeProviderStateOutput,
  toBrainWakeProviderStateOutput,
  toBufferedBrainRunDrainResult,
  toRawBufferedBrainRunDrainResult,
  type RawOpenAiResponsesBrainRunResult,
  type RawOpenAiResponsesCredentialSecretUpdate,
  type RawOpenAiOauthCredentialSummary,
  type RawOpenAiOauthCodeExchangeResult,
  type RawOpenAiResponsesBufferedStartResult,
  type RawChatCompletionsBufferedStartResult,
  type RawOpenAiResponsesBufferedDrainResult,
  type RawChatCompletionsBufferedDrainResult,
  type RawBufferedBrainRunDrainResult,
  type RawOpenAiResponsesBufferedCancellation,
  type RawOpenAiResponsesBufferedCancelResult,
  type RawBrainWakeStreamItem,
  type RawBrainAction,
  type RawBrainWakeProviderStateOutput,
} from "./brain-run-wire.js";
import { createNativeBridgeRuntimeConfigMethods } from "./runtime-config-wrappers.js";
import { createNativeBridgeRuntimeActivityMethods } from "./runtime-activity-wrappers.js";
import { createNativeBridgeReviewSubmissionMethods } from "./review-submission-wrappers.js";
import { createNativeBridgeProfileProviderMethods } from "./profile-provider-wrappers.js";
import { createNativeBridgeServiceCredentialMethods } from "./service-credential-wrappers.js";
import {
  toNativeModelProviderRecord,
  toNativeModelProviderRefreshImpact,
  toNativeProfileRegistryRecord,
  toRawModelProviderRecord,
  toRawModelProviderRefreshImpact,
  toRawProfileRegistryRecord,
  type RawModelProviderRecord,
  type RawModelProviderRefreshImpact,
  type RawProfileRegistryRecord,
} from "./profile-provider-wire.js";
import {
  toNativeBodyState,
  toBodyState,
  toNativeSessionState,
  toNativeAgentMessage,
  toNativeCoreEvent,
  toNativeBrainEventForJson,
  toNativeDelegatedCompletion,
  toDelegatedCompletion,
  toNativeDelegatedFanOutGroup,
  toDelegatedFanOutGroup,
  toNativeDenDataUpdate,
  toNativeExternalEvent,
  toNativeExternalEventPayload,
  toExternalEventPayload,
  encodeJson,
  toCoreEvent,
  toDelegatedSessionRuntimeStatus,
  toDelegatedResourceCleanupReport,
  toAgentMessage,
  toBrainEvent,
  toNativeBrainEvent,
  toToolCallMetadata,
  toRawToolCallMetadata,
  type RawCoreEvent,
  type RawDelegatedSessionRuntimeStatus,
  type RawDelegatedResourceCleanupReport,
  type RawBodyState,
  type RawAgentMessage,
  type RawToolProfile,
  type RawDelegatedCompletion,
  type RawDelegatedFanOutGroup,
  type RawBrainEvent,
  type RawToolCallPolicyMetadata,
  type RawToolCallMetadata,
} from "./event-body-wire.js";

export {
  coreConfigFacadeArtifact,
  fromCoreConfigWireRuntimeGraphPlan,
  toCoreConfigWireRuntimeGraphPlanInput,
} from "./generated/core-config-facade.js";

import type {
  ActionBatchReceipt,
  AdapterId,
  AgentId,
  AgentMessage,
  BrainAction,
  BrainActionBatch,
  BrainContinuationPayload,
  BrainEvent,
  BrainEventEnvelope,
  BrainImplementationHandle,
  BrainImplementationRegistration,
  BrainProviderStateScope,
  BrainWakeProviderStateOutput,
  BrainWakeProviderStateInput,
  BrainWakeFailure,
  BrainWakeRequest,
  BrainWakeStreamItem,
  BodyState,
  ChannelBindingRecord,
  CompletionPacket,
  ContextCompactionArtifact,
  ContextCompactionArtifactQuery,
  CoreEvent,
  DelegatedResourceCleanupReport,
  DelegatedSessionRuntimeStatus,
  DenDataUpdate,
  EngineConfig,
  EngineHandle,
  EventReceipt,
  EventSubscription,
  ExternalEvent,
  GitHubGateSuspendRequest,
  GitHubGateTerminalEvent,
  GitHubGateTerminalReceipt,
  GitHubGateWaitRecord,
  ManifestOperationName,
  MemoryGovernanceDecisionInput,
  MemoryGovernanceDecisionRecord,
  MemoryProposalEnvelope,
  MemoryProposalQuery,
  MemoryProposalRecord,
  MemorySpaceDescriptor,
  FanOutFailurePolicy,
  SessionActivityDigest,
  SessionActivityDigestQuery,
  PlatformAdapterHandle,
  PlatformAdapterRegistration,
  ParentConsumptionPolicy,
  ProfileId,
  ProviderStateAbsenceReason,
  ProjectId,
  ResourceLimits,
  RunId,
  RuntimeBufferHandle,
  RuntimeBufferView,
  ScheduledHostJobManualRunRequest,
  ScheduledHostJobRegistrationInput,
  ScheduledHostRunClaimQuery,
  ScheduledHostRunCompletionInput,
  ScheduledJobListQuery,
  ScheduledJobStatus,
  ScheduledJobSummary,
  ScheduledRunListQuery,
  ScheduledRunStatus,
  ScheduledRunSummary,
  ScheduledRunTrigger,
  SchedulerTickReport,
  SessionId,
  SessionState,
  ShutdownRequest,
  ShutdownSummary,
  SubscriptionHandle,
  TaskId,
  ToolCallMetadata,
  ToolProfile,
  Unit,
} from "@rusty-crew/contracts";

export * from "./public-api.js";
import type {
  NativeSessionConfigInput,
  BridgeBufferClient,
  BrainWakeExecutionResult,
  NativeBrainRunModuleId,
  NativeBufferedBrainRunDrain,
  NativeBridgeRoundTripFixtureName,
  OpenAiResponsesTransportMetrics,
  OpenAiResponsesCredentialSecretUpdate,
  NativeOpenAiOauthCodeExchangeInput,
  NativeOpenAiOauthCredentialSummary,
  NativeOpenAiOauthExchangeError,
  NativeOpenAiOauthCodeExchangeResult,
  OpenAiResponsesBrainRunInput,
  OpenAiResponsesToolRequest,
  ChatCompletionsChatCompletionMessage,
  ChatCompletionsBrainRunInput,
  ChatCompletionsToolRequest,
  ChatCompletionsTransportMetrics,
  OpenAiResponsesBufferedCancellation,
  NativeBrainWakeProviderStateInput,
  BrainWakeExecutor,
  BrainWakeBufferInput,
  BrainWakeSessionBufferInput,
  NativeSessionStateSummary,
  NativeProfileMemoryCaps,
  NativeProfileMemoryRecord,
  NativeSessionMemoryRecord,
  NativeSessionMemoryQuery,
  NativeBranchAwareSessionMemoryQuery,
  NativeSessionMemoryPromptContext,
  NativeProfileRegistryLifecycleStatus,
  NativeProfileRegistrySourceAssetRef,
  NativeProfileRegistryDerivedRuntimeRef,
  NativeProfileRegistryImportExportMetadata,
  NativeProfileRegistryRecord,
  NativeProfileRegistryQuery,
  NativeProfilePurgeTableCount,
  NativeProfilePurgeReport,
  NativeModelProviderStatus,
  NativeModelProviderProtocol,
  NativeModelProviderCredentialKind,
  NativeModelProviderCredential,
  NativeModelProviderRecord,
  NativeModelProviderWrite,
  NativeModelProviderQuery,
  NativeModelProviderAffectedProfile,
  NativeModelProviderRefreshImpact,
  NativeModelProviderRefreshImpactRequest,
  NativeModelProviderRefreshMode,
  NativeModelProviderRefreshPlanRequest,
  NativeModelProviderRefreshProfileAction,
  NativeModelProviderRefreshPlan,
  NativeRoleplayLoreRecord,
  NativeRoleplayLoreWrite,
  NativeRoleplayLoreReplace,
  NativeRoleplayLoreSupersede,
  NativeRoleplayLoreTombstone,
  NativeRoleplayLoreQuery,
  NativeRoleplayLoreProvenanceEvent,
  NativeRoleplayLoreLayerRecord,
  NativeRoleplayLoreLayerWrite,
  NativeRoleplayLoreLayerUpdate,
  NativeRoleplayLoreLayerArchive,
  NativeRoleplayLoreLayerConfigRecord,
  NativeRoleplayLoreLayerConfigWrite,
  NativeRoleplayLoreLayerEntryLink,
  NativeRoleplayLoreLayerEntryJoin,
  NativeRoleplayLoreFactCapture,
  NativeRoleplayLoreEntryPromotion,
  NativeRoleplayChatLayersWrite,
  NativeRoleplayChatLayerRecord,
  NativeLoreRecallQuery,
  NativeLoreRecallResult,
  NativeLoreRecallTraceQuery,
  NativeLoreRecallTraceRecord,
  NativeProfileMemoryQuery,
  NativeSimpleKvQuery,
  NativeSimpleKvRecord,
  NativeSimpleKvWrite,
  NativeSimpleKvDelete,
  NativeProfileMemoryWrite,
  NativeProfileMemoryReplace,
  NativeProfileMemoryDelete,
  NativeRuntimeSearchQuery,
  NativeRuntimeSearchResult,
  NativeRuntimeCounterScopeType,
  NativeRuntimeCounterQuery,
  NativeRuntimeCounterRecord,
  NativeRuntimeCounterSummary,
  NativeRuntimeDatabaseSize,
  NativeSchemaMigrationRecord,
  NativeRuntimeStorageCapability,
  NativeRuntimeRepositoryBackendRequirement,
  NativeRuntimeRepositoryGroupDiagnostic,
  NativeRuntimeModuleCapabilityStatus,
  NativeRuntimeModuleLogicalStoreDiagnostic,
  NativeRuntimeModulePhysicalTableDiagnostic,
  NativeRuntimeModulePhysicalIndexDiagnostic,
  NativeRuntimeModuleRetentionDiagnostic,
  NativeRuntimeModuleNamedDiagnostic,
  NativeRuntimeModuleQueryCatalogDiagnostic,
  NativeRuntimeModuleTransferHookDiagnostic,
  NativeRuntimeInstalledModuleSchemaDiagnostic,
  NativeRuntimeModuleSchemaDiagnostic,
  NativeRuntimeModuleSchemaRegistryDiagnostics,
  NativeRuntimeStorageTableCount,
  NativeRuntimeQueryPlanCheck,
  NativeRuntimeStoragePressureSignal,
  NativeRuntimeStorageConnectionHealth,
  NativeRuntimeStorageDiagnostics,
  NativeBufferedBrainRunModuleDiagnostics,
  NativeBufferedBrainRunDiagnostic,
  NativeBufferedBrainRunDiagnostics,
  NativeBufferedBrainRunCleanupModuleReport,
  NativeBufferedBrainRunCleanupSummary,
  NativeRuntimeMaintenancePolicy,
  NativeSessionMemoryCompactionReport,
  NativeRuntimeMaintenanceReport,
  NativeRuntimeConfigDiagnosticSeverity,
  NativeExternalBindingStatus,
  NativeRuntimeConfigDiagnostic,
  NativeRuntimeConfigValidationResult,
  NativeToolMetadataPolicyValidationInput,
  NativeToolMetadataPolicyTool,
  NativeToolMetadataPolicyDiagnostic,
  NativeToolMetadataPolicyValidationResult,
  NativeLocalToolProfilePolicyValidationInput,
  NativeLocalToolProfilePolicyValidationIssue,
  NativeLocalToolProfilePolicyValidationResult,
  NativeExternalMemoryToolMode,
  NativeToolAvailabilityPlanInput,
  NativeToolAvailabilityOmission,
  NativeToolAvailabilityPlan,
  NativeLocalCodeResourcePolicyInput,
  NativeLocalCodeFilesystemScope,
  NativeLocalCodeExecutionMode,
  NativeLocalCodeToolResourcePolicy,
  NativeLocalCodeResourcePolicyPlan,
  NativeWebBrowserResourcePolicyInput,
  NativeWebBrowserResourcePolicyPlan,
  NativeWebResourcePolicyPlan,
  NativeBrowserResourcePolicyPlan,
  NativeRuntimeConfigPlan,
  NativeRuntimeConfigValidationInput,
  NativeRuntimeGraphPlanInput,
  NativeRuntimeGraphPlan,
  NativeRuntimeConfigDraft,
  NativeBrainConfigDraft,
  NativeSessionConfigDraft,
  NativeScheduledJobConfigDraft,
  NativeChannelBindingConfigDraft,
  NativeMcpBindingConfigDraft,
  NativeProfileRuntimeMetadata,
  NativeCreateProfilePlanInput,
  NativeNewSessionControlPlanInput,
  NativeDelegatedRoleLifecyclePlanInput,
  NativeDelegatedRoleLifecyclePlan,
  NativeNewSessionControlTemplate,
  NativeNewSessionControlPlan,
  NativeBrainProviderProtocol,
  NativeBrainProviderStateMode,
  NativeBrainHostCapability,
  NativeBrainProviderStatePolicy,
  NativeBrainStrategyDiagnostics,
  NativeBrainCatalogStrategy,
  NativeBrainCatalogModule,
  NativeBrainCatalog,
  NativeBrainSelectionRequest,
  NativeBrainSelectionPlan,
  NativeReloadMcpControlPlanInput,
  NativeReloadMcpControlPlan,
  NativeChannelIngressRouteDecision,
  NativeChannelIngressRouteMessage,
  NativeChannelIngressRoutePlanInput,
  NativeChannelIngressRouteRequest,
  NativeChannelIngressRoutePlan,
  NativeDenProductIngressPolicyInput,
  NativeDenProductIngressPolicyPlan,
  NativeCreateProfileRequest,
  NativeCreateProfileMcpBindingRequest,
  NativeProfileRegistryRuntimeMetadata,
  NativeCreateProfileSourceRequest,
  NativeProfileModelConfigSeed,
  NativeCreateProfilePlan,
  NativeProfileRegistryWrite,
  NativeProfileRegistryUpdate,
  NativeProfileRegistryMutationRequest,
  NativeProfileRegistryMutationPlan,
  NativeCreateProfileFileAssetAction,
  NativeCreateProfileDerivedRuntimeAction,
  NativeCreateProfileSeedMetadata,
  NativeQueuedMessageRecord,
  NativeProviderStateDiagnostic,
  NativeChatReadModelEvent,
  NativeChatReadModelPage,
  NativeChatEventLogEvent,
  NativeChatEventLogPage,
  NativeExactPage,
  NativeChatSessionReadFacts,
  NativeChatSessionSummaryPage,
  NativeChatSessionReadResult,
  NativeBridgeModule,
} from "./public-api.js";
import {
  mergeProviderStateDiagnostics,
  observeProviderStateFailure,
  observeProviderStateWake,
} from "./provider-state-diagnostics.js";

interface NativeAddon {
  NativeBridgeBinding: new () => NativeBridgeBinding;
}

interface RawGitHubGateWaitRecord {
  session_id: string;
  run_id?: string | null;
  provider_thread_id?: string | null;
  project_id: string;
  task_id: string;
  gate_id: number;
  commit_sha: string;
  phase: GitHubGateWaitRecord["phase"];
  terminal_event_id?: number | null;
  created_at: string;
  updated_at: string;
}

interface RawGitHubGateTerminalReceipt {
  event_id: number;
  cursor: number;
  duplicate: boolean;
  wake_scheduled: boolean;
  ignored_reason?: string | null;
  wait?: RawGitHubGateWaitRecord | null;
}

function toRawGitHubGateSuspendRequest(input: GitHubGateSuspendRequest) {
  return {
    session_id: input.sessionId,
    ...(input.runId === undefined ? {} : { run_id: input.runId }),
    ...(input.providerThreadId === undefined
      ? {}
      : { provider_thread_id: input.providerThreadId }),
    project_id: input.projectId,
    task_id: input.taskId,
    gate_id: input.gateId,
    commit_sha: input.commitSha,
    now: input.now,
  };
}

function toRawGitHubGateTerminalEvent(input: GitHubGateTerminalEvent) {
  return {
    event_id: input.eventId,
    gate_id: input.gateId,
    project_id: input.projectId,
    task_id: input.taskId,
    commit_sha: input.commitSha,
    status: input.status,
    terminal_reason: input.terminalReason,
    ...(input.summary === undefined ? {} : { summary: input.summary }),
    ...(input.failureSummary === undefined
      ? {}
      : { failure_summary: input.failureSummary }),
    completed_at: input.completedAt,
  };
}

function fromRawGitHubGateWaitRecord(
  raw: RawGitHubGateWaitRecord,
): GitHubGateWaitRecord {
  return {
    sessionId: raw.session_id as SessionId,
    ...(raw.run_id == null ? {} : { runId: raw.run_id as RunId }),
    ...(raw.provider_thread_id == null
      ? {}
      : { providerThreadId: raw.provider_thread_id }),
    projectId: raw.project_id as ProjectId,
    taskId: raw.task_id as TaskId,
    gateId: raw.gate_id,
    commitSha: raw.commit_sha,
    phase: raw.phase,
    ...(raw.terminal_event_id == null
      ? {}
      : { terminalEventId: raw.terminal_event_id }),
    createdAt: raw.created_at,
    updatedAt: raw.updated_at,
  };
}

function fromRawGitHubGateTerminalReceipt(
  raw: RawGitHubGateTerminalReceipt,
): GitHubGateTerminalReceipt {
  return {
    eventId: raw.event_id,
    cursor: raw.cursor,
    duplicate: raw.duplicate,
    wakeScheduled: raw.wake_scheduled,
    ...(raw.ignored_reason == null
      ? {}
      : { ignoredReason: raw.ignored_reason }),
    ...(raw.wait == null
      ? {}
      : { wait: fromRawGitHubGateWaitRecord(raw.wait) }),
  };
}

export const nativeManifestOperationNames = manifestOperationNames;
export const nativeManifestVersion = 1;
export const nativeWireShapeFingerprint = bridgeWireShapeFingerprint;

export function roundTripNativeBridgeFixture(input: {
  name: NativeBridgeRoundTripFixtureName;
  value: unknown;
}): unknown {
  switch (input.name) {
    case "body_state_v1":
      return toNativeBodyState(toBodyState(input.value as RawBodyState));
    case "list_sessions_v1":
      return (input.value as RawSessionState[])
        .map(toSessionState)
        .map(toNativeSessionState);
    case "buffered_brain_run_drain_v1":
      return toRawBufferedBrainRunDrainResult(
        toBufferedBrainRunDrainResult(
          input.value as RawBufferedBrainRunDrainResult,
        ),
      );
    case "profile_registry_record_v1":
      return toRawProfileRegistryRecord(
        toNativeProfileRegistryRecord(input.value as RawProfileRegistryRecord),
      );
    case "model_provider_record_v1":
      return toRawModelProviderRecord(
        toNativeModelProviderRecord(input.value as RawModelProviderRecord),
      );
    case "model_provider_refresh_impact_v1":
      return toRawModelProviderRefreshImpact(
        toNativeModelProviderRefreshImpact(
          input.value as RawModelProviderRefreshImpact,
        ),
      );
    case "memory_space_descriptor_v1":
    case "memory_proposal_record_v1":
    case "memory_governance_decision_record_v1":
    case "session_activity_digest_v1":
    case "context_compaction_artifact_v1":
      return input.value;
  }
}

export class NativeBridgeContractError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "NativeBridgeContractError";
  }
}

export class NativeBridgeLoadError extends Error {
  constructor(message: string, options?: { cause?: unknown }) {
    super(message, options);
    this.name = "NativeBridgeLoadError";
  }
}

export async function loadNativeBridge(): Promise<NativeBridgeModule> {
  const addon = loadNativeAddon();
  if (!addon) {
    return createUnavailableNativeBridge();
  }

  const binding = withDirectBridgeOutputValidation(
    withGeneratedBridgeOutputValidation(new addon.NativeBridgeBinding()),
  );
  assertNativeBridgeContract(binding);
  return createNativeBridgeModule(binding);
}

export function createUnavailableNativeBridge(): NativeBridgeModule {
  return {
    manifestVersion: nativeManifestVersion,
    operationNames: nativeManifestOperationNames,
    wireShapeFingerprint: nativeWireShapeFingerprint,
    initializeEngine: unavailable("initialize_engine"),
    shutdownEngine: unavailable("shutdown_engine"),
    registerBrainImplementation: unavailable("register_brain_implementation"),
    replaceBrainImplementation: unavailable("replace_brain_implementation"),
    unregisterBrainImplementationForProfile: unavailable(
      "unregister_brain_implementation_for_profile",
    ),
    registerBrainRuntime: unavailable("register_brain_implementation"),
    replaceBrainRuntime: unavailable("replace_brain_implementation"),
    clearBrainProviderState: unavailable("apply_brain_provider_state_output"),
    wakeBrain: unavailable("wake_brain"),
    submitBrainEvent: unavailable("submit_brain_event"),
    submitBrainActions: unavailable("submit_brain_actions"),
    registerPlatformAdapter: unavailable("register_platform_adapter"),
    brainCatalog: unavailable("brain_catalog"),
    planBrainSelection: unavailable("plan_brain_selection"),
    validateToolMetadataPolicy: unavailable("validate_tool_metadata_policy"),
    validateLocalToolProfilePolicy: unavailable(
      "validate_local_tool_profile_policy",
    ),
    planToolAvailability: unavailable("plan_tool_availability"),
    planLocalCodeResourcePolicy: unavailable("plan_local_code_resource_policy"),
    planWebBrowserResourcePolicy: unavailable(
      "plan_web_browser_resource_policy",
    ),
    validateRuntimeConfigDraft: unavailable("validate_runtime_config_draft"),
    planRuntimeConfig: unavailable("plan_runtime_config"),
    planRuntimeGraph: unavailable("plan_runtime_graph"),
    planCreateProfile: unavailable("plan_create_profile"),
    planProfileRegistryMutation: unavailable("plan_profile_registry_mutation"),
    planNewSessionControl: unavailable("plan_new_session_control"),
    planReloadMcpControl: unavailable("plan_reload_mcp_control"),
    planDelegatedRoleLifecycle: unavailable("plan_delegated_role_lifecycle"),
    planChannelIngressRoute: unavailable("plan_channel_ingress_route"),
    planDenProductIngressPolicy: unavailable("plan_den_product_ingress_policy"),
    injectExternalEvent: unavailable("inject_external_event"),
    injectDenDataUpdate: unavailable("inject_den_data_update"),
    listAgentRouteResolutions: unavailable("list_agent_route_resolutions"),
    getAgentRouteResolution: unavailable("get_agent_route_resolution"),
    resolveAgentAddress: unavailable("resolve_agent_address"),
    putAgentRoute: unavailable("put_agent_route"),
    deleteAgentRoute: unavailable("delete_agent_route"),
    enqueueBodyFollowUpMessage: unavailable("enqueue_body_follow_up_message"),
    createCrewAgentSession: unavailable("create_crew_agent_session"),
    archiveSession: unavailable("archive_session"),
    setSessionReasoningEffort: unavailable("set_session_reasoning_effort"),
    ensureConfiguredSession: unavailable("ensure_configured_session"),
    registerScheduledWakeJob: unavailable("register_scheduled_wake_job"),
    registerScheduledHostJob: unavailable("register_scheduled_host_job"),
    listScheduledJobs: unavailable("list_scheduled_jobs"),
    listScheduledRuns: unavailable("list_scheduled_runs"),
    claimScheduledHostRuns: unavailable("claim_scheduled_host_runs"),
    requestScheduledHostJobRun: unavailable("request_scheduled_host_job_run"),
    completeScheduledHostRun: unavailable("complete_scheduled_host_run"),
    runSchedulerTick: unavailable("run_scheduler_tick"),
    requestScheduledJobRun: unavailable("request_scheduled_job_run"),
    pauseScheduledJob: unavailable("pause_scheduled_job"),
    resumeScheduledJob: unavailable("resume_scheduled_job"),
    cancelDelegatedSession: unavailable("cancel_delegated_session"),
    requestDelegatedCheckpoint: unavailable("request_delegated_checkpoint"),
    drainDelegatedSessions: unavailable("drain_delegated_sessions"),
    cleanupDelegatedResources: unavailable("cleanup_delegated_resources"),
    delegatedSessionStatus: unavailable("delegated_session_status"),
    listSessions: unavailable("list_sessions"),
    subscribeEvents: unavailable("subscribe_events"),
    unsubscribeEvents: unavailable("unsubscribe_events"),
    drainSubscriptionEvents: unavailable("subscribe_events"),
    createSession: unavailable("initialize_engine"),
    routeAgentMessage: unavailable("inject_external_event"),
    listAgentDirectory: unavailable("list_agent_directory"),
    deliverAgentMessage: unavailable("deliver_agent_message"),
    replyAgentMessage: unavailable("reply_agent_message"),
    listAgentMessageInbox: unavailable("list_agent_message_inbox"),
    listAgentMessageTraffic: unavailable("list_agent_message_traffic"),
    beginAgentRound: unavailable("begin_agent_round"),
    getAgentRound: unavailable("get_agent_round"),
    getAgentMessageDelivery: unavailable("get_agent_message_delivery"),
    completeAgentMessageDelivery: unavailable(
      "complete_agent_message_delivery",
    ),
    registerExternalRuntime: unavailable("register_external_runtime"),
    authorizeExternalRuntimeHandshake: unavailable(
      "authorize_external_runtime_handshake",
    ),
    recordExternalRuntimeState: unavailable("record_external_runtime_state"),
    listExternalRuntimes: unavailable("list_external_runtimes"),
    getExternalRuntime: unavailable("get_external_runtime"),
    certifyExternalRuntime: unavailable("certify_external_runtime"),
    invalidateExternalRuntimeCertification: unavailable(
      "invalidate_external_runtime_certification",
    ),
    listExternalRuntimeCertifications: unavailable(
      "list_external_runtime_certifications",
    ),
    getExternalRuntimeCertification: unavailable(
      "get_external_runtime_certification",
    ),
    acquireExternalController: unavailable("acquire_external_controller"),
    releaseExternalController: unavailable("release_external_controller"),
    bindExternalAgent: unavailable("bind_external_agent"),
    restoreExternalAgentBinding: unavailable("restore_external_agent_binding"),
    updateExternalBindingMetadata: unavailable(
      "update_external_binding_metadata",
    ),
    listExternalBindings: unavailable("list_external_bindings"),
    getExternalBinding: unavailable("get_external_binding"),
    prepareExternalAgentSessionCreation: unavailable(
      "prepare_external_agent_session_creation",
    ),
    markExternalAgentSessionNativeStarting: unavailable(
      "mark_external_agent_session_native_starting",
    ),
    completeExternalAgentSessionCreation: unavailable(
      "complete_external_agent_session_creation",
    ),
    recordExternalAgentSessionCreationFailure: unavailable(
      "record_external_agent_session_creation_failure",
    ),
    getExternalTurn: unavailable("get_external_turn"),
    listExternalTurnsForNativeThread: unavailable(
      "list_external_turns_for_native_thread",
    ),
    listActiveExternalTurns: unavailable("list_active_external_turns"),
    expireExternalTurnDispatches: unavailable(
      "expire_external_turn_dispatches",
    ),
    transitionExternalTurn: unavailable("transition_external_turn"),
    submitExternalControl: unavailable("submit_external_control"),
    completeExternalControl: unavailable("complete_external_control"),
    recordExternalInteraction: unavailable("record_external_interaction"),
    resolveExternalInteraction: unavailable("resolve_external_interaction"),
    terminalizeExternalInteraction: unavailable(
      "terminalize_external_interaction",
    ),
    listPendingExternalInteractions: unavailable(
      "list_pending_external_interactions",
    ),
    recordExternalRuntimeEvent: unavailable("record_external_runtime_event"),
    queryExternalRuntimeEvents: unavailable("query_external_runtime_events"),
    buildBrainWakeRequest: unavailable("wake_brain"),
    buildBrainWakeRequestForSession: unavailable("wake_brain"),
    logicalTurnDiagnostics: unavailable("logical_turn_diagnostics"),
    requeueLogicalTurnContinuations: unavailable(
      "requeue_logical_turn_continuations",
    ),
    resolveLogicalTurnAttention: unavailable("resolve_logical_turn_attention"),
    cancelLogicalTurn: unavailable("cancel_logical_turn"),
    diagnosticProjectBodyStateJson: unavailable("wake_brain"),
    diagnosticSubmitBrainActionsJson: unavailable("submit_brain_actions"),
    diagnosticCountRows: unavailable("initialize_engine"),
    databaseSize: unavailable("initialize_engine"),
    storageDiagnostics: unavailable("initialize_engine"),
    storageSchema: unavailable("initialize_engine"),
    createProfileRegistryRecord: unavailable("initialize_engine"),
    updateProfileRegistryRecord: unavailable("initialize_engine"),
    listProfileRegistryRecords: unavailable("initialize_engine"),
    getProfileRegistryRecord: unavailable("initialize_engine"),
    purgeProfile: unavailable("initialize_engine"),
    upsertModelProvider: unavailable("initialize_engine"),
    listModelProviders: unavailable("initialize_engine"),
    getModelProvider: unavailable("initialize_engine"),
    getModelProviderSecret: unavailable("initialize_engine"),
    upsertServiceCredential: unavailable("initialize_engine"),
    listServiceCredentials: unavailable("initialize_engine"),
    getServiceCredential: unavailable("initialize_engine"),
    getServiceCredentialSecret: unavailable("initialize_engine"),
    deleteServiceCredential: unavailable("initialize_engine"),
    linkModelProviderCredential: unavailable("initialize_engine"),
    unlinkModelProviderCredential: unavailable("initialize_engine"),
    modelProviderRefreshImpact: unavailable("initialize_engine"),
    planModelProviderRefresh: unavailable("initialize_engine"),
    putRoleplayCharacter: unavailable("initialize_engine"),
    getRoleplayCharacter: unavailable("initialize_engine"),
    listRoleplayCharacters: unavailable("initialize_engine"),
    putRoleplayPlayerPersona: unavailable("initialize_engine"),
    getRoleplayPlayerPersona: unavailable("initialize_engine"),
    listRoleplayPlayerPersonas: unavailable("initialize_engine"),
    putRoleplaySessionMetadata: unavailable("initialize_engine"),
    getRoleplaySessionMetadata: unavailable("initialize_engine"),
    listRoleplaySessionMetadata: unavailable("initialize_engine"),
    applyRoleplaySessionProjection: unavailable("initialize_engine"),
    putRoleplayImport: unavailable("initialize_engine"),
    getRoleplayImport: unavailable("initialize_engine"),
    listRoleplayImports: unavailable("initialize_engine"),
    createRoleplayMechanicProposal: unavailable("initialize_engine"),
    getRoleplayMechanicProposal: unavailable("initialize_engine"),
    listRoleplayMechanicProposals: unavailable("initialize_engine"),
    decideRoleplayMechanicProposal: unavailable("initialize_engine"),
    applyRoleplayMechanicProposal: unavailable("initialize_engine"),
    createRoleplayMechanicSessionAssociation: unavailable("initialize_engine"),
    getRoleplayMechanicSessionAssociation: unavailable("initialize_engine"),
    listRoleplayMechanicSessionAssociations: unavailable("initialize_engine"),
    updateRoleplayMechanicSessionAttachment: unavailable("initialize_engine"),
    createRoleplayMechanicDiagnostic: unavailable("initialize_engine"),
    getRoleplayMechanicDiagnostic: unavailable("initialize_engine"),
    listRoleplayMechanicDiagnostics: unavailable("initialize_engine"),
    updateRoleplayMechanicDiagnosticOutcome: unavailable("initialize_engine"),
    createLoreLayer: unavailable("initialize_engine"),
    getLoreLayer: unavailable("initialize_engine"),
    listLoreLayers: unavailable("initialize_engine"),
    updateLoreLayer: unavailable("initialize_engine"),
    archiveLoreLayer: unavailable("initialize_engine"),
    setChatLayers: unavailable("initialize_engine"),
    getChatLayers: unavailable("initialize_engine"),
    toggleChatLayer: unavailable("initialize_engine"),
    reorderChatLayers: unavailable("initialize_engine"),
    addLoreEntry: unavailable("initialize_engine"),
    replaceLoreEntry: unavailable("initialize_engine"),
    supersedeLoreEntry: unavailable("initialize_engine"),
    tombstoneLoreEntry: unavailable("initialize_engine"),
    queryLoreEntries: unavailable("initialize_engine"),
    getLoreEntry: unavailable("initialize_engine"),
    loreEntryProvenanceEvents: unavailable("initialize_engine"),
    addEntryToLayer: unavailable("initialize_engine"),
    removeEntryFromLayer: unavailable("initialize_engine"),
    setEntryConstant: unavailable("initialize_engine"),
    listEntriesByLayer: unavailable("initialize_engine"),
    recallLore: unavailable("initialize_engine"),
    captureLoreFact: unavailable("initialize_engine"),
    promoteLoreEntry: unavailable("initialize_engine"),
    getLoreLayerConfig: unavailable("initialize_engine"),
    setLoreLayerConfig: unavailable("initialize_engine"),
    listRecallTraces: unavailable("initialize_engine"),
    getRecallTrace: unavailable("initialize_engine"),
    runMaintenance: unavailable("initialize_engine"),
    listMemorySpaceDescriptors: unavailable("initialize_engine"),
    querySessionMemoryRecords: unavailable("initialize_engine"),
    buildSessionMemoryPromptContext: unavailable("initialize_engine"),
    saveMemoryProposal: unavailable("initialize_engine"),
    planCaptureMemoryProposals: unavailable("initialize_engine"),
    planCuratorGovernanceTransition: unavailable("initialize_engine"),
    applyCuratorGovernanceWrite: unavailable("initialize_engine"),
    getCuratorCandidate: unavailable("initialize_engine"),
    listCuratorCandidates: unavailable("initialize_engine"),
    getCuratorMutation: unavailable("initialize_engine"),
    listCuratorMutations: unavailable("initialize_engine"),
    listCuratorAuditReceipts: unavailable("initialize_engine"),
    planCuratorLifecycleTransition: unavailable("initialize_engine"),
    planBackgroundMemoryAutoMutations: unavailable("initialize_engine"),
    listMemoryProposals: unavailable("initialize_engine"),
    saveSessionActivityDigest: unavailable("initialize_engine"),
    listSessionActivityDigests: unavailable("initialize_engine"),
    saveContextCompactionArtifact: unavailable("initialize_engine"),
    listContextCompactionArtifacts: unavailable("initialize_engine"),
    manualContextCompaction: unavailable("initialize_engine"),
    recordMemoryGovernanceDecision: unavailable("initialize_engine"),
    planRoleplayAssistantAlternative: unavailable(
      "plan_roleplay_assistant_alternative",
    ),
    planRoleplaySessionLifecycle: unavailable(
      "plan_roleplay_session_lifecycle",
    ),
    planRoleplayChatLayerBinding: unavailable(
      "plan_roleplay_chat_layer_binding",
    ),
    normalizeRoleplayLoreSearchControls: unavailable(
      "normalize_roleplay_lore_search_controls",
    ),
    readRoleplaySceneState: unavailable("read_roleplay_scene_state"),
    planRoleplaySceneStateUpdate: unavailable(
      "plan_roleplay_scene_state_update",
    ),
    buildRoleplayPromptContext: unavailable("build_roleplay_prompt_context"),
    roleplaySpeakerIdentity: unavailable("roleplay_speaker_identity"),
    writeRoleplayCharacter: unavailable("write_roleplay_character"),
    mergeRoleplayCharacter: unavailable("merge_roleplay_character"),
    writeRoleplayPlayerPersona: unavailable("write_roleplay_player_persona"),
    mergeRoleplayPlayerPersona: unavailable("merge_roleplay_player_persona"),
    patchRoleplaySessionMetadata: unavailable(
      "patch_roleplay_session_metadata",
    ),
    normalizeRoleplayNarratorConfig: unavailable(
      "normalize_roleplay_narrator_config",
    ),
    planRoleplayMechanicProfile: unavailable("plan_roleplay_mechanic_profile"),
    startRoleplayNarratorTurn: unavailable("start_roleplay_narrator_turn"),
    advanceRoleplayNarratorTurn: unavailable("advance_roleplay_narrator_turn"),
    saveMessageSlot: unavailable("save_message_slot"),
    saveMessageVariant: unavailable("save_message_variant"),
    createChatMessageSlot: unavailable("create_chat_message_slot"),
    createChatMessageVariant: unavailable("create_chat_message_variant"),
    applyRoleplayAlternative: unavailable("apply_roleplay_alternative"),
    chatReadModelPage: unavailable("chat_read_model_page"),
    readChatSession: unavailable("read_chat_session"),
    queryChatSessionSummaries: unavailable("query_chat_session_summaries"),
    appendChatEvent: unavailable("append_chat_event"),
    queryChatEvents: unavailable("query_chat_events"),
    queryMessageSlots: unavailable("query_message_slots"),
    queryMessageSlotsPage: unavailable("query_message_slots_page"),
    queryMessageVariants: unavailable("query_message_variants"),
    queryMessageVariantsPage: unavailable("query_message_variants_page"),
    selectActiveMessageVariant: unavailable("select_active_message_variant"),
    selectActiveChatMessageVariant: unavailable(
      "select_active_chat_message_variant",
    ),
    deleteChatMessageVariant: unavailable("delete_chat_message_variant"),
    reorderChatMessageVariants: unavailable("reorder_chat_message_variants"),
    deleteMessageVariant: unavailable("delete_message_variant"),
    reorderMessageVariants: unavailable("reorder_message_variants"),
    saveConversationBranch: unavailable("save_conversation_branch"),
    createChatConversationBranch: unavailable(
      "create_chat_conversation_branch",
    ),
    ensureActiveChatConversationBranch: unavailable(
      "ensure_active_chat_conversation_branch",
    ),
    queryConversationBranches: unavailable("query_conversation_branches"),
    readConversationTree: unavailable("read_conversation_tree"),
    searchChatTranscript: unavailable("search_chat_transcript"),
    getConversationBranchState: unavailable("get_conversation_branch_state"),
    selectActiveConversationBranch: unavailable(
      "select_active_conversation_branch",
    ),
    updateConversationBranchHead: unavailable(
      "update_conversation_branch_head",
    ),
    saveConversationSnapshot: unavailable("save_conversation_snapshot"),
    createChatConversationSnapshot: unavailable(
      "create_chat_conversation_snapshot",
    ),
    queryConversationSnapshots: unavailable("query_conversation_snapshots"),
    resolveConversationJump: unavailable("resolve_conversation_jump"),
    saveAttachment: unavailable("save_attachment"),
    createChatAttachment: unavailable("create_chat_attachment"),
    queryAttachments: unavailable("query_attachments"),
    queryAttachmentsPage: unavailable("query_attachments_page"),
    removeAttachment: unavailable("remove_attachment"),
    removeChatAttachment: unavailable("remove_chat_attachment"),
    saveDataBankScope: unavailable("save_data_bank_scope"),
    createChatDataBankScope: unavailable("create_chat_data_bank_scope"),
    queryDataBankScopes: unavailable("query_data_bank_scopes"),
    queryDataBankScopesPage: unavailable("query_data_bank_scopes_page"),
    removeDataBankScope: unavailable("remove_data_bank_scope"),
    removeChatDataBankScope: unavailable("remove_chat_data_bank_scope"),
    providerStateDiagnostics: unavailable("provider_state_diagnostics"),
    beginRuntimeActivity: unavailable("begin_runtime_activity"),
    progressRuntimeActivity: unavailable("progress_runtime_activity"),
    finishRuntimeActivity: unavailable("finish_runtime_activity"),
    settleRuntimeActivityWake: unavailable("settle_runtime_activity_wake"),
    runtimeActivityCensus: unavailable("runtime_activity_census"),
    bufferedBrainRunDiagnostics: unavailable("buffered_brain_run_diagnostics"),
    cleanupBufferedBrainRuns: unavailable("cleanup_buffered_brain_runs"),
    suspendForGitHubGate: unavailable("suspend_for_github_gate"),
    consumeGitHubGateTerminalEvent: unavailable(
      "consume_github_gate_terminal_event",
    ),
    recoverGitHubGateWakes: unavailable("recover_github_gate_wakes"),
    gitHubGateWait: unavailable("github_gate_wait"),
    gitHubGateEventCursor: unavailable("github_gate_event_cursor"),
    beginReviewSubmission: unavailable("begin_review_submission"),
    transitionReviewSubmission: unavailable("transition_review_submission"),
    listReviewSubmissions: unavailable("list_review_submissions"),
    exchangeOpenAiOauthCode: unavailable("wake_brain"),
    startBrainRun: unavailable("start_brain_run"),
    drainBrainRun: unavailable("drain_brain_run"),
    submitBrainHostResult: unavailable("submit_brain_host_result"),
    cancelBrainRun: unavailable("cancel_brain_run"),
    listProfileMemory: unavailable("initialize_engine"),
    getProfileMemory: unavailable("initialize_engine"),
    addProfileMemory: unavailable("initialize_engine"),
    replaceProfileMemory: unavailable("initialize_engine"),
    removeProfileMemory: unavailable("initialize_engine"),
    listSimpleKv: unavailable("initialize_engine"),
    putSimpleKv: unavailable("initialize_engine"),
    deleteSimpleKv: unavailable("initialize_engine"),
    searchRuntime: unavailable("initialize_engine"),
    queryRuntimeCounters: unavailable("initialize_engine"),
    runtimeSummary: unavailable("initialize_engine"),
    resetRuntimeCounters: unavailable("initialize_engine"),
    projectBodyStateJson: unavailable("wake_brain"),
    submitBrainActionsJson: unavailable("submit_brain_actions"),
    countRows: unavailable("initialize_engine"),
    getBuffer: unavailable("get_buffer"),
    releaseBuffer: unavailable("release_buffer"),
  };
}

function unavailable<Args extends unknown[], Result>(
  operation: string,
): (...args: Args) => Promise<Result> {
  return async () => {
    throw new Error(`native bridge operation ${operation} is unavailable`);
  };
}

function providerStateFromBufferedWake(buffered: {
  providerStateJson?: string;
  providerStateAbsence?: string;
}): Pick<BrainWakeRequest, "providerState" | "providerStateAbsence"> {
  const providerState =
    buffered.providerStateJson === undefined
      ? undefined
      : providerStateInputFromNativeJson(buffered.providerStateJson);
  return {
    ...(providerState === undefined ? {} : { providerState }),
    ...(buffered.providerStateAbsence === undefined
      ? {}
      : {
          providerStateAbsence:
            buffered.providerStateAbsence as BrainWakeRequest["providerStateAbsence"],
        }),
  };
}

function continuationStateFromBufferedWake(buffered: {
  continuationStateJson?: string;
}): Pick<BrainWakeRequest, "continuationState"> {
  if (buffered.continuationStateJson === undefined) return {};
  return {
    continuationState: JSON.parse(
      buffered.continuationStateJson,
    ) as BrainContinuationPayload,
  };
}

function providerStateInputFromNativeJson(
  raw: string,
): BrainWakeProviderStateInput {
  const parsed = JSON.parse(raw) as NativeBrainWakeProviderStateInput;
  return {
    moduleId: parsed.module_id,
    strategyId: parsed.strategy_id,
    profileFingerprint: parsed.profile_fingerprint,
    providerFingerprint: parsed.provider_fingerprint,
    payloadVersion: parsed.payload_version,
    payload: parsed.payload,
    ...(parsed.expires_at === undefined
      ? {}
      : { expiresAt: parsed.expires_at }),
  };
}

function toNativeProviderStateDiagnostic(
  raw: NativeProviderStateDiagnostic,
): NativeProviderStateDiagnostic {
  return {
    sessionId: raw.sessionId,
    moduleId: raw.moduleId,
    strategyId: raw.strategyId,
    recordId: raw.recordId,
    profileFingerprint: raw.profileFingerprint,
    providerFingerprint: raw.providerFingerprint,
    status: raw.status,
    payloadVersion: raw.payloadVersion,
    payloadBytes: raw.payloadBytes,
    createdAt: raw.createdAt,
    updatedAt: raw.updatedAt,
    expiresAt: raw.expiresAt,
    lastWakeId: raw.lastWakeId,
    invalidatedAt: raw.invalidatedAt,
    invalidationReason: raw.invalidationReason,
    isCurrent: raw.isCurrent,
  };
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function loadNativeAddon(): NativeAddon | undefined {
  const artifactName = nativeArtifactName();
  if (!artifactName) {
    return undefined;
  }

  const artifactPath = fileURLToPath(
    new URL(`../native/${artifactName}`, import.meta.url),
  );
  if (!existsSync(artifactPath)) {
    return undefined;
  }

  try {
    const nativeRequire = createRequire(import.meta.url);
    return nativeRequire(artifactPath) as NativeAddon;
  } catch (error) {
    throw new NativeBridgeLoadError(
      [
        `native bridge addon ${artifactName} exists but failed to load`,
        `path: ${artifactPath}`,
        `error: ${errorMessage(error)}`,
        "rebuild the native bridge binary with npm run build:native",
      ].join("; "),
      { cause: error },
    );
  }
}

function nativeArtifactName(): string | undefined {
  if (process.platform === "linux" && process.arch === "x64") {
    return "index.linux-x64-gnu.node";
  }

  return undefined;
}

function assertNativeBridgeContract(binding: NativeBridgeBinding): void {
  if (binding.manifestVersion !== nativeManifestVersion) {
    throw new NativeBridgeContractError(
      `native bridge manifest version mismatch: expected ${nativeManifestVersion}, got ${binding.manifestVersion}; rebuild the native bridge binary with npm run build:native`,
    );
  }

  const actual = [...binding.operationNames];
  const expected = [...nativeManifestOperationNames];
  if (!arraysEqual(actual, expected)) {
    const actualSet = new Set(actual);
    const expectedSet = new Set<string>(expected);
    const missing = expected.filter((name) => !actualSet.has(name));
    const extra = actual.filter((name) => !expectedSet.has(name));
    const firstDiff = firstArrayDifference(actual, expected);
    const firstDiffDetail =
      firstDiff === undefined
        ? "none"
        : `index ${firstDiff}: native has ${actual[firstDiff] ?? "<missing>"}, contracts expect ${expected[firstDiff] ?? "<missing>"}`;

    throw new NativeBridgeContractError(
      [
        "native bridge operation inventory mismatch; rebuild the native bridge binary with npm run build:native",
        `first difference: ${firstDiffDetail}`,
        `missing from native: ${missing.length === 0 ? "[]" : JSON.stringify(missing)}`,
        `extra in native: ${extra.length === 0 ? "[]" : JSON.stringify(extra)}`,
      ].join("; "),
    );
  }

  if (binding.wireShapeFingerprint !== nativeWireShapeFingerprint) {
    throw new NativeBridgeContractError(
      `native bridge wire-shape fingerprint mismatch: expected ${nativeWireShapeFingerprint}, got ${binding.wireShapeFingerprint}; run npm run codegen:bridge-fingerprint and npm run build:native, then commit the regenerated fingerprint and native bridge binary`,
    );
  }
}

function arraysEqual(
  left: readonly string[],
  right: readonly string[],
): boolean {
  return (
    left.length === right.length &&
    left.every((value, index) => value === right[index])
  );
}

function firstArrayDifference(
  left: readonly string[],
  right: readonly string[],
): number | undefined {
  const length = Math.max(left.length, right.length);
  for (let index = 0; index < length; index += 1) {
    if (left[index] !== right[index]) return index;
  }
  return undefined;
}

function createNativeBridgeModule(
  binding: NativeBridgeBinding,
): NativeBridgeModule {
  const wakeExecutors = new Map<BrainImplementationHandle, BrainWakeExecutor>();
  const brainRegistrations = new Map<
    BrainImplementationHandle,
    BrainImplementationRegistration
  >();
  const providerStateObservations = new Map<
    string,
    NativeProviderStateDiagnostic
  >();
  const nativeBrainRegistration = (
    registration: BrainImplementationRegistration,
  ) => ({
    implementationId: registration.implementationId,
    profileId: registration.profileId,
    toolProfile: {
      tools: registration.toolProfile.tools.map((tool) => ({
        name: tool.name,
        description: tool.description,
        inputSchema: tool.inputSchema ?? undefined,
      })),
    },
    modelConfig: {
      provider: registration.modelConfig.provider,
      modelName: registration.modelConfig.modelName,
      temperatureMilli: registration.modelConfig.temperatureMilli,
      maxOutputTokens: registration.modelConfig.maxOutputTokens,
    },
    strategy: registration.strategy
      ? {
          moduleId: registration.strategy.moduleId,
          strategyId: registration.strategy.strategyId,
          providerState: {
            mode: registration.strategy.providerState.mode,
          },
        }
      : undefined,
    providerStateScope: registration.providerStateScope
      ? {
          profileFingerprint:
            registration.providerStateScope.profileFingerprint,
          providerFingerprint:
            registration.providerStateScope.providerFingerprint,
        }
      : undefined,
  });
  const nativeSessionConfig = (config: NativeSessionConfigInput) => ({
    ...config,
    resourceLimits: config.resourceLimits
      ? {
          workdir: config.resourceLimits.workdir ?? undefined,
          maxDurationMs: config.resourceLimits.maxDurationMs ?? undefined,
          maxDelegationDepth:
            config.resourceLimits.maxDelegationDepth ?? undefined,
        }
      : undefined,
    toolProfile: config.toolProfile
      ? {
          tools: config.toolProfile.tools.map((tool) => ({
            ...tool,
            inputSchema: tool.inputSchema ?? undefined,
          })),
        }
      : undefined,
    historyWindow: config.historyWindow
      ? { maxMessages: config.historyWindow.maxMessages ?? undefined }
      : undefined,
  });
  const module: NativeBridgeModule = {
    manifestVersion: binding.manifestVersion,
    operationNames: binding.operationNames as ManifestOperationName[],
    wireShapeFingerprint: binding.wireShapeFingerprint,
    initializeEngine: async (config) =>
      binding.initializeEngine({
        engineDataDir: config.engineDataDir,
        fixedClock: config.clock === "system" ? undefined : config.clock.fixed,
        defaultTurnBudget: config.defaultTurnBudget,
        defaultIdleTimeoutMs: config.defaultIdleTimeoutMs,
        storageBackend: config.storage?.backend,
        postgresDatabaseUrl:
          config.storage?.backend === "postgres"
            ? config.storage.databaseUrl
            : undefined,
        postgresSchema:
          config.storage?.backend === "postgres"
            ? config.storage.schema
            : undefined,
        postgresMaxConnections:
          config.storage?.backend === "postgres"
            ? config.storage.maxConnections
            : undefined,
        postgresStatementTimeoutMs:
          config.storage?.backend === "postgres"
            ? config.storage.statementTimeoutMs
            : undefined,
        backingFilesystemPath:
          config.storage?.backend === "postgres"
            ? config.storage.backingFilesystemPath
            : undefined,
        filesystemWarningFreePercent:
          config.storage?.filesystemWarningFreePercent,
      }) as EngineHandle,
    shutdownEngine: async (request) =>
      binding.shutdownEngine(request.engine, request.drainTimeoutMs),
    registerBrainImplementation: async (registration) => {
      const handle = binding.registerBrainImplementation(
        nativeBrainRegistration(registration),
      ) as BrainImplementationHandle;
      brainRegistrations.set(handle, registration);
      return handle;
    },
    replaceBrainImplementation: async (registration) => {
      const handle = binding.replaceBrainImplementation(
        nativeBrainRegistration(registration),
      ) as BrainImplementationHandle;
      brainRegistrations.set(handle, registration);
      return handle;
    },
    unregisterBrainImplementationForProfile: async (profileId) => {
      const handle = binding.unregisterBrainImplementationForProfile(
        profileId,
      ) as BrainImplementationHandle;
      brainRegistrations.delete(handle);
      wakeExecutors.delete(handle);
      return handle;
    },
    registerBrainRuntime: async (registration, executor) => {
      const handle = await module.registerBrainImplementation(registration);
      wakeExecutors.set(handle, executor);
      return handle;
    },
    replaceBrainRuntime: async (registration, executor) => {
      const handle = await module.replaceBrainImplementation(registration);
      wakeExecutors.set(handle, executor);
      return handle;
    },
    clearBrainProviderState: async (input) => {
      const output: BrainWakeProviderStateOutput = {
        type: "clear",
        reason: input.reason ?? "brain_requested_clear",
      };
      binding.applyBrainProviderStateOutputJson(
        input.brain,
        input.sessionId,
        input.wakeId,
        JSON.stringify(output),
      );
      return {};
    },
    wakeBrain: (request, options) =>
      executeNativeBrainWake(
        {
          binding,
          module,
          wakeExecutors,
          brainRegistrations,
          observeProviderStateSaveFailure: (wake, registration, status) =>
            observeProviderStateFailure(
              providerStateObservations,
              wake,
              registration,
              status,
            ),
        },
        request,
        options,
      ),
    submitBrainEvent: async (event) => {
      const validatedEvent = validateBridgeValue<BrainEventEnvelope>({
        operation: "submit_brain_event",
        direction: "ts_to_rust",
        schema: brainEventEnvelopeSchema,
        value: event,
      });
      const nativeEvent = toNativeBrainEvent(validatedEvent.event);
      return validateBridgeValue<EventReceipt>({
        operation: "submit_brain_event",
        direction: "rust_to_ts",
        schema: eventReceiptSchema,
        value: binding.submitBrainEvent(
          validatedEvent.wakeId,
          validatedEvent.sessionId,
          nativeEvent.eventType,
          nativeEvent.text,
          nativeEvent.toolName,
          nativeEvent.isError,
          nativeEvent.metadataJson,
        ),
      });
    },
    submitBrainActions: async (batch) => {
      const validatedBatch = validateBridgeValue<BrainActionBatch>({
        operation: "submit_brain_actions",
        direction: "ts_to_rust",
        schema: brainActionBatchSchema,
        value: batch,
      });
      const receipt = binding.submitBrainActionsJson(
        validatedBatch.wakeId,
        validatedBatch.sessionId,
        new TextEncoder().encode(
          JSON.stringify(validatedBatch.actions.map(toNativeBrainAction)),
        ),
      );
      return validateBridgeValue<ActionBatchReceipt>({
        operation: "submit_brain_actions",
        direction: "rust_to_ts",
        schema: actionBatchReceiptSchema,
        value: {
          wakeId: receipt.wakeId,
          acceptedActions: receipt.acceptedActions,
          rejectedActions: JSON.parse(
            receipt.rejectedActionsJson,
          ) as ActionBatchReceipt["rejectedActions"],
        },
      });
    },
    registerPlatformAdapter: async (registration) =>
      binding.registerPlatformAdapter({
        adapterId: registration.adapterId,
        kind: registration.kind,
        displayName: registration.displayName,
      }) as PlatformAdapterHandle,
    ...createNativeBridgeBrainCatalogMethods(binding),
    ...createNativeBridgeRuntimeConfigMethods(binding),
    injectExternalEvent: async (event) =>
      binding.injectExternalEvent(encodeJson(toNativeExternalEvent(event))),
    injectDenDataUpdate: async (update) =>
      binding.injectDenDataUpdate(encodeJson(toNativeDenDataUpdate(update))),
    cancelDelegatedSession: async (delegatedSessionId) =>
      binding.cancelDelegatedSession(delegatedSessionId),
    requestDelegatedCheckpoint: async (input) =>
      binding.requestDelegatedCheckpoint(
        input.parentSessionId,
        input.delegatedSessionId,
        input.reason,
      ),
    drainDelegatedSessions: async (input) =>
      binding.drainDelegatedSessions(input?.parentSessionId) as SessionId[],
    cleanupDelegatedResources: async () =>
      toDelegatedResourceCleanupReport(
        JSON.parse(
          binding.cleanupDelegatedResourcesJson(),
        ) as RawDelegatedResourceCleanupReport,
      ),
    delegatedSessionStatus: async (delegatedSessionId) =>
      toDelegatedSessionRuntimeStatus(
        JSON.parse(
          binding.delegatedSessionStatusJson(delegatedSessionId),
        ) as RawDelegatedSessionRuntimeStatus,
      ),
    listSessions: async () =>
      validateBridgeValue<RawSessionState[]>({
        operation: "list_sessions",
        direction: "rust_to_ts",
        schema: rawSessionStateArraySchema,
        value: JSON.parse(binding.listSessionsJson()),
      }).map(toSessionState),
    subscribeEvents: async (subscription) =>
      binding.subscribeEvents({
        eventKinds: subscription.eventKinds,
        sessionId: subscription.sessionId,
        agentId: subscription.agentId,
        adapterId: subscription.adapterId,
      }) as SubscriptionHandle,
    unsubscribeEvents: async (handle) => {
      binding.unsubscribeEvents(handle);
      return {};
    },
    drainSubscriptionEvents: async (handle, maxEvents = 32) =>
      binding
        .drainSubscriptionEvents(handle, maxEvents)
        .map((eventJson) => toCoreEvent(JSON.parse(eventJson) as RawCoreEvent)),
    createSession: async (config) =>
      binding.createSession(nativeSessionConfig(config)),
    ...createNativeBridgeCrewSessionMethods(binding),
    ensureConfiguredSession: async (config) =>
      binding.ensureConfiguredSession(nativeSessionConfig(config)),
    archiveSession: async (sessionId) => binding.archiveSession(sessionId),
    setSessionReasoningEffort: async (sessionId, reasoningEffort) =>
      binding.setSessionReasoningEffort(sessionId, reasoningEffort),
    ...createNativeBridgeAgentCoordinationMethods(binding),
    ...createNativeBridgeExternalRuntimeMethods(binding),
    ...createNativeBridgeExternalRuntimeCertificationMethods(binding),
    enqueueBodyFollowUpMessage: async (input) =>
      binding.enqueueBodyFollowUpMessage(
        input.sessionId,
        input.from,
        input.body,
        input.correlationId ?? null,
      ) as unknown as NativeQueuedMessageRecord,
    ...createNativeBridgeSchedulerMethods(binding),
    buildBrainWakeRequest: async (input) => {
      const buffered = binding.buildBrainWakeRequest(
        input.brain,
        input.sessionId,
        input.bodyStateJson,
        input.systemPrompt,
        input.roleAssemblyJson,
        input.wakeId,
      );
      const request = {
        brain: input.brain,
        sessionId: input.sessionId as BrainWakeRequest["sessionId"],
        bodyState: buffered.bodyState as RuntimeBufferHandle,
        systemPrompt: buffered.systemPrompt as RuntimeBufferHandle,
        roleAssembly: buffered.roleAssembly as RuntimeBufferHandle,
        wakeId: input.wakeId,
        ...continuationStateFromBufferedWake(buffered),
        ...providerStateFromBufferedWake(buffered),
      };
      validateBridgeValue<BrainWakeRequest>({
        operation: "build_brain_wake_request",
        direction: "rust_to_ts",
        schema: brainWakeRequestSchema,
        value: request,
      });
      observeProviderStateWake(
        providerStateObservations,
        request,
        brainRegistrations.get(input.brain),
      );
      return request;
    },
    buildBrainWakeRequestForSession: async (input) => {
      const buffered = binding.buildBrainWakeRequestForSession(
        input.brain,
        input.sessionId,
        input.systemPrompt,
        input.roleAssemblyJson,
        input.wakeId,
      );
      const request: BrainWakeRequest & Record<string, unknown> = {
        brain: input.brain,
        sessionId: input.sessionId,
        bodyState: buffered.bodyState as RuntimeBufferHandle,
        systemPrompt: buffered.systemPrompt as RuntimeBufferHandle,
        roleAssembly: buffered.roleAssembly as RuntimeBufferHandle,
        wakeId: input.wakeId,
        ...continuationStateFromBufferedWake(buffered),
        ...providerStateFromBufferedWake(buffered),
        ...(input.compactionIntent !== undefined
          ? {
              compactionIntent: input.compactionIntent,
            }
          : {}),
      } as unknown as BrainWakeRequest;
      validateBridgeValue<BrainWakeRequest>({
        operation: "build_brain_wake_request_for_session",
        direction: "rust_to_ts",
        schema: brainWakeRequestSchema,
        value: request,
      });
      observeProviderStateWake(
        providerStateObservations,
        request,
        brainRegistrations.get(input.brain),
      );
      return request;
    },
    logicalTurnDiagnostics: async (query) =>
      JSON.parse(
        binding.logicalTurnDiagnosticsJson(JSON.stringify(query)),
      ) as Awaited<ReturnType<NativeBridgeModule["logicalTurnDiagnostics"]>>,
    requeueLogicalTurnContinuations: async () =>
      binding.requeueLogicalTurnContinuations(),
    resolveLogicalTurnAttention: async (input) =>
      JSON.parse(
        binding.resolveLogicalTurnAttentionJson(JSON.stringify(input)),
      ) as Awaited<
        ReturnType<NativeBridgeModule["resolveLogicalTurnAttention"]>
      >,
    cancelLogicalTurn: async (input) =>
      JSON.parse(
        binding.cancelLogicalTurnJson(JSON.stringify(input)),
      ) as Awaited<ReturnType<NativeBridgeModule["cancelLogicalTurn"]>>,
    diagnosticProjectBodyStateJson: async (sessionId) => {
      const bytes = binding.projectBodyStateJson(sessionId);
      validateBridgeJsonText({
        operation: "project_body_state",
        direction: "rust_to_ts",
        schema: rawBodyStateSchema,
        text: new TextDecoder().decode(bytes),
      });
      return bytes;
    },
    diagnosticSubmitBrainActionsJson: async (wakeId, sessionId, actions) => {
      validateBridgeValue<BrainActionBatch>({
        operation: "diagnostic_submit_brain_actions_json",
        direction: "ts_to_rust",
        schema: brainActionBatchSchema,
        value: { wakeId, sessionId, actions },
      });
      const receipt = binding.submitBrainActionsJson(
        wakeId,
        sessionId,
        new TextEncoder().encode(
          JSON.stringify(actions.map(toNativeBrainAction)),
        ),
      );
      return validateBridgeValue<ActionBatchReceipt>({
        operation: "diagnostic_submit_brain_actions_json",
        direction: "rust_to_ts",
        schema: actionBatchReceiptSchema,
        value: {
          wakeId: receipt.wakeId,
          acceptedActions: receipt.acceptedActions,
          rejectedActions: JSON.parse(receipt.rejectedActionsJson) as [],
        },
      });
    },
    ...createNativeBridgeAdminMethods(binding),
    ...createNativeBridgeRuntimeActivityMethods(binding),
    bufferedBrainRunDiagnostics: async () =>
      JSON.parse(
        binding.bufferedBrainRunDiagnosticsJson(),
      ) as NativeBufferedBrainRunDiagnostics,
    cleanupBufferedBrainRuns: async (input) =>
      JSON.parse(
        binding.cleanupBufferedBrainRunsJson(input.reasonCode, input.summary),
      ) as NativeBufferedBrainRunCleanupSummary,
    suspendForGitHubGate: async (input) =>
      fromRawGitHubGateWaitRecord(
        JSON.parse(
          binding.suspendForGithubGateJson(
            JSON.stringify(toRawGitHubGateSuspendRequest(input)),
          ),
        ) as RawGitHubGateWaitRecord,
      ),
    consumeGitHubGateTerminalEvent: async (input) =>
      fromRawGitHubGateTerminalReceipt(
        JSON.parse(
          binding.consumeGithubGateTerminalEventJson(
            JSON.stringify(toRawGitHubGateTerminalEvent(input)),
          ),
        ) as RawGitHubGateTerminalReceipt,
      ),
    recoverGitHubGateWakes: async () => binding.recoverGithubGateWakes(),
    gitHubGateWait: async (sessionId) => {
      const raw = JSON.parse(
        binding.githubGateWaitJson(sessionId),
      ) as RawGitHubGateWaitRecord | null;
      return raw === null ? undefined : fromRawGitHubGateWaitRecord(raw);
    },
    gitHubGateEventCursor: async () => binding.githubGateEventCursor(),
    ...createNativeBridgeReviewSubmissionMethods(binding),
    ...createNativeBridgeProfileProviderMethods(binding),
    ...createNativeBridgeServiceCredentialMethods(binding),
    ...createNativeBridgeRoleplayMethods(binding),
    ...createNativeBridgeRoleplayProposalMethods(binding),
    ...createNativeBridgeRoleplayMechanicMethods(binding),
    ...createNativeBridgeMemoryMethods(binding),
    ...createNativeBridgeCuratorMethods(binding),
    ...createNativeBridgeChatMethods(binding),
    providerStateDiagnostics: async (limit = 100) => {
      const stored = binding
        .providerStateDiagnostics(limit)
        .map((raw) =>
          toNativeProviderStateDiagnostic(
            raw as unknown as NativeProviderStateDiagnostic,
          ),
        );
      return validateBridgeValue<NativeProviderStateDiagnostic[]>({
        operation: "provider_state_diagnostics",
        direction: "rust_to_ts",
        schema: providerStateDiagnosticArraySchema,
        value: mergeProviderStateDiagnostics(
          stored,
          providerStateObservations.values(),
        ).slice(0, limit),
      });
    },
    exchangeOpenAiOauthCode: async (input) => {
      const result = await binding.exchangeOpenaiOauthCodeJson(
        JSON.stringify({
          issuer: input.issuer,
          clientId: input.clientId,
          redirectUri: input.redirectUri,
          code: input.code,
          codeVerifier: input.codeVerifier,
          now: input.now,
        }),
      );
      if (typeof result !== "string") {
        throw new TypeError("OpenAI OAuth native result must be JSON text");
      }
      const raw = JSON.parse(result) as RawOpenAiOauthCodeExchangeResult;
      if (!raw.ok) {
        return raw;
      }
      return {
        ok: true,
        secret: raw.secret,
        summary: {
          kind: raw.summary.kind,
          version: raw.summary.version,
          hasSecret: raw.summary.has_secret,
          accountId: raw.summary.account_id ?? undefined,
          email: raw.summary.email ?? undefined,
          planType: raw.summary.plan_type ?? undefined,
          isFedrampAccount: raw.summary.is_fedramp_account,
          accessTokenExpiresAt:
            raw.summary.access_token_expires_at ?? undefined,
        },
      };
    },
    startBrainRun: async (input) => {
      const providerInput =
        input.moduleId === "chat-completions"
          ? toNativeChatCompletionsBrainRunInput(
              validateBridgeValue<ChatCompletionsBrainRunInput>({
                operation: "start_brain_run",
                direction: "ts_to_rust",
                schema: chatCompletionsBrainRunInputSchema,
                value: input.providerInput,
              }),
            )
          : toNativeOpenAiResponsesBrainRunInput(input.providerInput);
      const raw = JSON.parse(
        binding.startBrainRunJson(
          input.moduleId,
          JSON.stringify(providerInput),
        ),
      ) as RawOpenAiResponsesBufferedStartResult & { module_id: string };
      return {
        moduleId: assertCanonicalBrainRunModule(raw.module_id),
        wakeId: raw.wake_id,
      };
    },
    drainBrainRun: async (input) => {
      const raw = validateBridgeValue<RawBufferedBrainRunDrainResult>({
        operation: "drain_brain_run",
        direction: "rust_to_ts",
        schema: rawBufferedBrainRunDrainSchema,
        value: JSON.parse(
          binding.drainBrainRunJson(
            input.moduleId,
            input.wakeId,
            input.maxItems,
          ),
        ),
      });
      return toBufferedBrainRunDrainResult(raw);
    },
    submitBrainHostResult: async (input) => {
      const raw = JSON.parse(
        binding.submitBrainHostResultJson(
          input.moduleId,
          JSON.stringify({
            wakeId: input.wakeId,
            callId: input.callId,
            output: input.output,
            status: input.status,
            retryable: input.retryable,
            ...(input.reasonCode === undefined
              ? {}
              : { reasonCode: input.reasonCode }),
            ...(input.action === undefined ? {} : { action: input.action }),
            ...(input.summary === undefined ? {} : { summary: input.summary }),
            ...(input.debugDetailId === undefined
              ? {}
              : { debugDetailId: input.debugDetailId }),
            ...(input.stateFingerprint === undefined
              ? {}
              : { stateFingerprint: input.stateFingerprint }),
            ...(input.turnDisposition === undefined
              ? {}
              : { turnDisposition: input.turnDisposition }),
          }),
        ),
      ) as { module_id: string; wake_id: string; call_id: string };
      return {
        moduleId: assertCanonicalBrainRunModule(raw.module_id),
        wakeId: raw.wake_id,
        callId: raw.call_id,
      };
    },
    cancelBrainRun: async (input) => {
      const raw = JSON.parse(
        binding.cancelBrainRunJson(
          input.moduleId,
          JSON.stringify({
            wakeId: input.wakeId,
            reasonCode: input.reasonCode,
            summary: input.summary,
          }),
        ),
      ) as RawOpenAiResponsesBufferedCancelResult & { module_id: string };
      return {
        moduleId: assertCanonicalBrainRunModule(raw.module_id),
        wakeId: raw.wake_id,
        cancelled: raw.cancelled,
        terminal: raw.terminal,
        cancellation: raw.cancellation
          ? {
              reasonCode: raw.cancellation.reason_code,
              summary: raw.cancellation.summary,
              cancelledAt: raw.cancellation.cancelled_at,
            }
          : undefined,
      };
    },
    projectBodyStateJson: async (sessionId) =>
      module.diagnosticProjectBodyStateJson(sessionId),
    submitBrainActionsJson: async (wakeId, sessionId, actions) =>
      module.diagnosticSubmitBrainActionsJson(wakeId, sessionId, actions),
    countRows: async (table) => module.diagnosticCountRows(table),
    getBuffer: async (handle) => {
      const view = binding.getBuffer(handle);
      return {
        ...view,
        handle: view.handle as RuntimeBufferHandle,
      };
    },
    releaseBuffer: async (handle) => {
      binding.releaseBuffer(handle);
      return {};
    },
  };

  return module;
}
