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
  brainWakeAcceptedSchema,
  brainWakeRequestSchema,
  chatEventLogEventSchema,
  chatEventLogPageSchema,
  chatReadModelPageSchema,
  eventReceiptSchema,
  openAiResponsesBrainRunInputSchema,
  piAgentBrainRunInputSchema,
  providerStateDiagnosticArraySchema,
  rawBodyStateSchema,
  rawModelProviderRefreshImpactSchema,
  rawModelProviderRefreshPlanSchema,
  rawModelProviderRecordArraySchema,
  rawModelProviderRecordSchema,
  rawOpenAiResponsesBrainRunResultSchema,
  rawPiAgentBufferedDrainResultSchema,
  rawProfilePurgeReportSchema,
  rawProfileRegistryRecordArraySchema,
  rawProfileRegistryRecordSchema,
  rawSessionStateArraySchema,
} from "./bridge-validation-schemas.js";
import {
  toCoreConfigWireCreateProfilePlanInput,
  toCoreConfigWireRuntimeConfigValidationInput,
} from "./generated/core-config-facade.js";

export { coreConfigFacadeArtifact } from "./generated/core-config-facade.js";

import type {
  ActionBatchReceipt,
  AdapterId,
  AgentId,
  AgentMessage,
  BrainAction,
  BrainActionBatch,
  BrainEvent,
  BrainEventEnvelope,
  BrainImplementationHandle,
  BrainImplementationRegistration,
  BrainProviderStateScope,
  BrainWakeProviderStateOutput,
  BrainWakeProviderStateInput,
  BrainWakeAccepted,
  BrainWakeFailure,
  BrainWakeRequest,
  BrainWakeStreamItem,
  BodyState,
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
  ProviderStateMode,
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

interface NativeSessionConfigInput {
  sessionId: string;
  agentId: string;
  profileId: string;
  kind: "full" | "worker" | "delegated";
  resourceLimits?: ResourceLimits;
  toolProfile?: ToolProfile;
  historyWindow?: SessionState["historyWindow"];
}

interface NativeAddon {
  NativeBridgeBinding: new () => NativeBridgeBinding;
}

interface NativeBridgeBinding {
  readonly manifestVersion: number;
  readonly operationNames: string[];
  readonly wireShapeFingerprint: string;
  initializeEngine(config: {
    engineDataDir: string;
    fixedClock?: string;
    defaultTurnBudget: number;
    defaultIdleTimeoutMs: number;
    storageBackend?: string;
    postgresDatabaseUrl?: string;
    postgresSchema?: string;
    postgresMaxConnections?: number;
    postgresStatementTimeoutMs?: number;
  }): number;
  registerBrainImplementation(registration: {
    implementationId: string;
    profileId: string;
    toolProfile: {
      tools: Array<{
        name: string;
        description: string;
        inputSchema?: number;
      }>;
    };
    modelConfig: {
      provider: string;
      modelName: string;
      temperatureMilli?: number;
      maxOutputTokens?: number;
    };
    strategy?: {
      moduleId: string;
      strategyId: string;
      providerState: {
        mode: string;
      };
    };
    providerStateScope?: {
      profileFingerprint: string;
      providerFingerprint: string;
    };
  }): number;
  replaceBrainImplementation(registration: {
    implementationId: string;
    profileId: string;
    toolProfile: {
      tools: Array<{
        name: string;
        description: string;
        inputSchema?: number;
      }>;
    };
    modelConfig: {
      provider: string;
      modelName: string;
      temperatureMilli?: number;
      maxOutputTokens?: number;
    };
    strategy?: {
      moduleId: string;
      strategyId: string;
      providerState: {
        mode: string;
      };
    };
    providerStateScope?: {
      profileFingerprint: string;
      providerFingerprint: string;
    };
  }): number;
  unregisterBrainImplementationForProfile(profileId: string): number;
  applyBrainProviderStateOutputJson(
    brain: number,
    sessionId: string,
    wakeId: string,
    outputJson: string,
  ): void;
  runOpenaiResponsesBrainJson(inputJson: string): Promise<string>;
  exchangeOpenaiOauthCodeJson(inputJson: string): Promise<string>;
  startOpenaiResponsesBrainJson(inputJson: string): string;
  drainOpenaiResponsesBrainStreamJson(
    wakeId: string,
    maxItems?: number,
  ): string;
  submitOpenaiResponsesToolOutputJson(inputJson: string): string;
  cancelOpenaiResponsesBrainJson(inputJson: string): string;
  startPiAgentBrainJson(inputJson: string): string;
  drainPiAgentBrainStreamJson(wakeId: string, maxItems?: number): string;
  submitPiAgentToolOutputJson(inputJson: string): string;
  cancelPiAgentBrainJson(inputJson: string): string;
  providerStateDiagnostics(limit?: number): NativeProviderStateDiagnostic[];
  planRoleplayAssistantAlternativeJson(inputJson: string): string;
  planRoleplaySessionLifecycleJson(inputJson: string): string;
  planRoleplayChatLayerBindingJson(inputJson: string): string;
  normalizeRoleplayLoreSearchControlsJson(inputJson: string): string;
  readRoleplaySceneStateJson(inputJson: string): string;
  planRoleplaySceneStateUpdateJson(inputJson: string): string;
  buildRoleplayPromptContextJson(inputJson: string): string;
  roleplaySpeakerIdentityJson(inputJson: string): string;
  writeRoleplayCharacterJson(inputJson: string): string;
  mergeRoleplayCharacterJson(inputJson: string): string;
  writeRoleplayPlayerPersonaJson(inputJson: string): string;
  mergeRoleplayPlayerPersonaJson(inputJson: string): string;
  patchRoleplaySessionMetadataJson(inputJson: string): string;
  normalizeRoleplayNarratorConfigJson(inputJson: string): string;
  roleplayNarratorMandatoryExploreRequestsJson(inputJson: string): string;
  roleplayNarratorAutoCaptureRequestJson(inputJson: string): string;
  startRoleplayNarratorTurnJson(inputJson: string): string;
  nextRoleplayNarratorPhaseJson(inputJson: string): string;
  roleplayNarratorReviewRequestsRevision(feedback: string): boolean;
  saveMessageSlotJson(inputJson: string): void;
  saveMessageVariantJson(inputJson: string): string;
  createChatMessageSlotJson(inputJson: string): string;
  createChatMessageVariantJson(inputJson: string): string;
  queryMessageSlotsJson(inputJson: string): string;
  queryMessageVariantsJson(inputJson: string): string;
  chatReadModelPageJson(inputJson: string): string;
  appendChatEventJson(inputJson: string): string;
  queryChatEventsJson(inputJson: string): string;
  selectActiveMessageVariantJson(inputJson: string): string;
  selectActiveChatMessageVariantJson(inputJson: string): string;
  deleteChatMessageVariantJson(inputJson: string): string;
  reorderChatMessageVariantsJson(inputJson: string): string;
  deleteMessageVariantJson(inputJson: string): string;
  reorderMessageVariantsJson(inputJson: string): string;
  saveConversationBranchJson(inputJson: string): string;
  createChatConversationBranchJson(inputJson: string): string;
  ensureActiveChatConversationBranchJson(inputJson: string): string;
  queryConversationBranchesJson(inputJson: string): string;
  getConversationBranchStateJson(inputJson: string): string;
  selectActiveConversationBranchJson(inputJson: string): string;
  updateConversationBranchHeadJson(inputJson: string): string;
  saveConversationSnapshotJson(inputJson: string): string;
  createChatConversationSnapshotJson(inputJson: string): string;
  queryConversationSnapshotsJson(inputJson: string): string;
  resolveConversationJumpJson(inputJson: string): string;
  saveAttachmentJson(inputJson: string): string;
  createChatAttachmentJson(inputJson: string): string;
  queryAttachmentsJson(inputJson: string): string;
  removeAttachmentJson(inputJson: string): string;
  removeChatAttachmentJson(inputJson: string): string;
  saveDataBankScopeJson(inputJson: string): string;
  createChatDataBankScopeJson(inputJson: string): string;
  queryDataBankScopesJson(inputJson: string): string;
  removeDataBankScopeJson(inputJson: string): string;
  removeChatDataBankScopeJson(inputJson: string): string;
  addLoreEntryJson(inputJson: string): string;
  replaceLoreEntryJson(inputJson: string): string;
  supersedeLoreEntryJson(inputJson: string): string;
  tombstoneLoreEntryJson(inputJson: string): string;
  queryLoreEntriesJson(inputJson: string): string;
  getLoreEntryJson(recordId: string): string;
  loreEntryProvenanceEventsJson(recordId: string): string;
  createLoreLayerJson(inputJson: string): string;
  getLoreLayerJson(layerId: string): string;
  listLoreLayersJson(profileId: string): string;
  updateLoreLayerJson(inputJson: string): string;
  archiveLoreLayerJson(inputJson: string): string;
  getLoreLayerConfigJson(layerId: string): string;
  setLoreLayerConfigJson(inputJson: string): string;
  addEntryToLayerJson(inputJson: string): void;
  removeEntryFromLayerJson(inputJson: string): void;
  setEntryConstantJson(inputJson: string): void;
  listEntriesByLayerJson(layerId: string): string;
  captureLoreFactJson(inputJson: string): string;
  promoteLoreEntryJson(inputJson: string): string;
  setChatLayersJson(inputJson: string): void;
  getChatLayersJson(chatId: string): string;
  toggleChatLayerJson(inputJson: string): void;
  reorderChatLayersJson(inputJson: string): void;
  recallLoreJson(inputJson: string): string;
  listRecallTracesJson(inputJson: string): string;
  getRecallTraceJson(traceId: string): string;
  registerPlatformAdapter(registration: {
    adapterId: string;
    kind: string;
    displayName: string;
  }): number;
  validateToolMetadataPolicyJson(inputJson: string): string;
  validateLocalToolProfilePolicyJson(inputJson: string): string;
  validateRuntimeConfigDraftJson(inputJson: string): string;
  planRuntimeConfigJson(inputJson: string): string;
  planCreateProfileJson(inputJson: string): string;
  planProfileRegistryMutationJson(inputJson: string): string;
  shutdownEngine(
    engine: number,
    drainTimeoutMs: number,
  ): {
    archivedSessions: number;
    droppedSubscriptions: number;
  };
  submitBrainEvent(
    wakeId: string,
    sessionId: string,
    eventType: string,
    text?: string,
    toolName?: string,
    isError?: boolean,
    metadataJson?: string,
  ): { accepted: boolean; sequence: number };
  injectExternalEvent(eventJson: Uint8Array): {
    accepted: boolean;
    sequence: number;
  };
  injectDenDataUpdate(updateJson: Uint8Array): {
    accepted: boolean;
    sequence: number;
  };
  cancelDelegatedSession(delegatedSessionId: string): {
    handle: number;
    sessionId: string;
    agentId: string;
    profileId: string;
    kind: string;
    status: string;
  };
  requestDelegatedCheckpoint(
    parentSessionId: string,
    delegatedSessionId: string,
    reason: string,
  ): { accepted: boolean; sequence: number };
  drainDelegatedSessions(parentSessionId?: string): string[];
  cleanupDelegatedResourcesJson(): string;
  delegatedSessionStatusJson(delegatedSessionId: string): string;
  listSessionsJson(): string;
  submitBrainTextDelta(
    wakeId: string,
    sessionId: string,
    text: string,
  ): { accepted: boolean; sequence: number };
  createSession(config: {
    sessionId: string;
    agentId: string;
    profileId: string;
    kind: string;
    resourceLimits?: ResourceLimits;
    toolProfile?: ToolProfile;
    historyWindow?: SessionState["historyWindow"];
  }): {
    handle: number;
    sessionId: string;
    agentId: string;
    profileId: string;
    kind: string;
    status: string;
  };
  ensureConfiguredSession(config: {
    sessionId: string;
    agentId: string;
    profileId: string;
    kind: string;
    resourceLimits?: ResourceLimits;
    toolProfile?: ToolProfile;
    historyWindow?: SessionState["historyWindow"];
  }): {
    handle: number;
    sessionId: string;
    agentId: string;
    profileId: string;
    kind: string;
    status: string;
  };
  archiveSession(sessionId: string): {
    handle: number;
    sessionId: string;
    agentId: string;
    profileId: string;
    kind: string;
    status: string;
  };
  routeAgentMessage(
    from: string,
    to: string,
    body: string,
    correlationId?: string,
  ): { accepted: boolean; sequence: number };
  enqueueBodyFollowUpMessage(
    sessionId: string,
    from: string,
    body: string,
    correlationId: string | null,
  ): NativeQueuedMessageRecord;
  registerScheduledWakeJobJson(
    jobId: string,
    targetSessionId: string,
    intervalMs: number | undefined,
    firstDueAt: string,
  ): string;
  registerScheduledHostJobJson(
    jobId: string,
    jobKind: string,
    intervalMs: number | undefined,
    firstDueAt: string,
    payloadJson: string,
  ): string;
  listScheduledJobsJson(
    status?: ScheduledJobStatus,
    jobKind?: string,
    limit?: number,
    offset?: number,
  ): string;
  listScheduledRunsJson(
    jobId?: string,
    status?: ScheduledRunStatus,
    trigger?: ScheduledRunTrigger,
    targetSessionId?: SessionId,
    limit?: number,
    offset?: number,
  ): string;
  claimScheduledHostRunsJson(
    supportedJobKinds: string[],
    limit?: number,
  ): string;
  requestScheduledHostJobRunJson(
    jobId: string,
    supportedJobKinds: string[],
  ): string;
  completeScheduledHostRun(
    runId: string,
    status: ScheduledHostRunCompletionInput["status"],
    outputJson: string,
    error?: string,
  ): void;
  runSchedulerTickJson(): string;
  requestScheduledJobRunJson(jobId: string): string;
  pauseScheduledJob(jobId: string): void;
  resumeScheduledJob(jobId: string, nextDueAt: string): void;
  buildBrainWakeRequest(
    brain: number,
    sessionId: string,
    bodyStateJson: Uint8Array,
    systemPrompt: string,
    roleAssemblyJson: Uint8Array,
    wakeId: string,
  ): {
    bodyState: number;
    systemPrompt: number;
    roleAssembly: number;
    providerStateJson?: string;
    providerStateAbsence?: string;
  };
  buildBrainWakeRequestForSession(
    brain: number,
    sessionId: string,
    systemPrompt: string,
    roleAssemblyJson: Uint8Array,
    wakeId: string,
  ): {
    bodyState: number;
    systemPrompt: number;
    roleAssembly: number;
    providerStateJson?: string;
    providerStateAbsence?: string;
  };
  projectBodyStateJson(sessionId: string): Uint8Array;
  submitBrainActionsJson(
    wakeId: string,
    sessionId: string,
    actionsJson: Uint8Array,
  ): {
    wakeId: string;
    acceptedActions: number;
    rejectedActionsJson: string;
  };
  countRows(table: string): number;
  databaseSize(): NativeRuntimeDatabaseSize;
  storageDiagnostics(): NativeRuntimeStorageDiagnostics;
  storageSchema(): NativeRuntimeModuleSchemaRegistryDiagnostics;
  bufferedBrainRunDiagnosticsJson(): string;
  cleanupBufferedBrainRunsJson(reasonCode: string, summary: string): string;
  createProfileRegistryRecordJson(writeJson: string): string;
  updateProfileRegistryRecordJson(updateJson: string): string;
  listProfileRegistryRecordsJson(queryJson: string): string;
  getProfileRegistryRecordJson(profileId: string): string;
  purgeProfileJson(profileId: string): string;
  upsertModelProviderJson(writeJson: string): string;
  listModelProvidersJson(queryJson: string): string;
  getModelProviderJson(alias: string): string;
  getModelProviderSecretJson(alias: string): string;
  modelProviderRefreshImpactJson(requestJson: string): string;
  planModelProviderRefreshJson(requestJson: string): string;
  runMaintenance(
    policy: NativeRuntimeMaintenancePolicy,
  ): NativeRuntimeMaintenanceReport;
  listMemorySpaceDescriptorsJson(): string;
  querySessionMemoryRecordsJson(inputJson: string): string;
  buildSessionMemoryPromptContextJson(inputJson: string): string;
  saveMemoryProposalJson(inputJson: string): string;
  planCaptureMemoryProposalsJson(inputJson: string): string;
  listMemoryProposalsJson(inputJson: string): string;
  saveSessionActivityDigestJson(inputJson: string): string;
  listSessionActivityDigestsJson(inputJson: string): string;
  saveContextCompactionArtifactJson(inputJson: string): string;
  listContextCompactionArtifactsJson(inputJson: string): string;
  recordMemoryGovernanceDecisionJson(inputJson: string): string;
  listProfileMemory(
    query: NativeProfileMemoryQuery,
  ): NativeProfileMemoryRecord[];
  listSimpleKv(query: NativeSimpleKvQuery): NativeSimpleKvRecord[];
  putSimpleKv(write: NativeSimpleKvWrite): NativeSimpleKvRecord;
  deleteSimpleKv(input: NativeSimpleKvDelete): NativeSimpleKvRecord;
  getProfileMemory(
    profileId: string,
    targetType: string,
    targetId: string | undefined,
    key: string,
  ): NativeProfileMemoryRecord | undefined;
  addProfileMemory(write: NativeProfileMemoryWrite): NativeProfileMemoryRecord;
  replaceProfileMemory(
    replace: NativeProfileMemoryReplace,
  ): NativeProfileMemoryRecord;
  removeProfileMemory(
    remove: NativeProfileMemoryDelete,
  ): NativeProfileMemoryRecord;
  searchRuntime(query: NativeRuntimeSearchQuery): NativeRuntimeSearchResult[];
  queryRuntimeCounters(
    query: NativeRuntimeCounterQuery,
  ): NativeRuntimeCounterRecord[];
  runtimeSummary(
    scopeType: NativeRuntimeCounterScopeType,
    scopeId: string | undefined,
  ): NativeRuntimeCounterSummary;
  resetRuntimeCounters(query: NativeRuntimeCounterQuery): number;
  getBuffer(handle: number): {
    handle: number;
    mediaType: string;
    byteLen: number;
    bytes: Uint8Array;
  };
  releaseBuffer(handle: number): void;
  subscribeEvents(subscription: {
    eventKinds: string[];
    sessionId?: string;
    agentId?: string;
    adapterId?: string;
  }): number;
  unsubscribeEvents(handle: number): void;
  drainSubscriptionEvents(handle: number, maxEvents: number): string[];
}

export interface BridgeBufferClient {
  getBuffer(handle: RuntimeBufferHandle): Promise<RuntimeBufferView>;
  releaseBuffer(handle: RuntimeBufferHandle): Promise<Unit>;
}

export interface BrainWakeExecutionResult {
  events: BrainEventEnvelope[];
  actions: BrainAction[];
  providerState?: BrainWakeProviderStateOutput;
  stream?: BrainWakeStreamItem[];
  transportMetrics?: OpenAiResponsesTransportMetrics | PiAgentTransportMetrics;
  credentialSecretUpdate?: OpenAiResponsesCredentialSecretUpdate;
}

export type NativeBridgeRoundTripFixtureName =
  | "body_state_v1"
  | "list_sessions_v1"
  | "brain_wake_stream_result_v1"
  | "profile_registry_record_v1"
  | "model_provider_record_v1"
  | "model_provider_refresh_impact_v1"
  | "memory_space_descriptor_v1"
  | "memory_proposal_record_v1"
  | "memory_governance_decision_record_v1";

export interface OpenAiResponsesTransportMetrics {
  effectiveTransport: string;
  selectedStrategyId: string;
  effectiveStrategyId: string;
  fallbackReason?: string | null;
  providerRequestCount: number;
  continuationRoundCount: number;
  providerRequestPayloadBytes: number;
  providerRequestDebugSamples?: unknown[];
  providerEventCounts: Record<string, number>;
  firstTextDeltaLatencyMs?: number | null;
  totalTurnDurationMs: number;
}

export interface OpenAiResponsesCredentialSecretUpdate {
  providerAlias: string;
  secret: string;
}

export interface NativeOpenAiOauthCodeExchangeInput {
  issuer: string;
  clientId: string;
  redirectUri: string;
  code: string;
  codeVerifier: string;
  now?: string;
}

export interface NativeOpenAiOauthCredentialSummary {
  kind: NativeModelProviderCredentialKind;
  version: number;
  hasSecret: boolean;
  accountId?: string;
  email?: string;
  planType?: string;
  isFedrampAccount: boolean;
  accessTokenExpiresAt?: string;
}

export interface NativeOpenAiOauthExchangeError {
  code: string;
  reasonCode: string;
  status?: number;
  message: string;
  retryable: boolean;
}

export type NativeOpenAiOauthCodeExchangeResult =
  | {
      ok: true;
      secret: string;
      summary: NativeOpenAiOauthCredentialSummary;
    }
  | {
      ok: false;
      error: NativeOpenAiOauthExchangeError;
    };

export interface OpenAiResponsesBrainRunInput {
  wakeId: string;
  sessionId: SessionId;
  bodyState: BodyState;
  tools?: Array<{
    name: string;
    description: string;
    inputSchema: unknown;
  }>;
  providerState?: BrainWakeProviderStateInput;
  providerStateAbsence?: ProviderStateAbsenceReason;
  config: {
    model: string;
    instructions?: string;
    streamIdleTimeoutMs?: number;
    wakeTimeoutMs?: number;
  };
  client?:
    | { mode: "fake" }
    | {
        mode: "live";
        baseUrl: string;
        apiKey?: string;
        authKind?: "api_key" | "openai_oauth";
        providerAlias?: string;
        oauthCredentialSecret?: string;
      };
}

export interface OpenAiResponsesToolRequest {
  wakeId: string;
  callId: string;
  providerItemId?: string;
  name: string;
  argumentsJson: string;
}

export interface PiAgentChatCompletionMessage {
  role: "system" | "user" | "assistant" | "tool";
  content?: string;
  name?: string;
  toolCallId?: string;
  toolCalls?: unknown[];
}

export interface PiAgentBrainRunInput {
  wakeId: string;
  sessionId: SessionId;
  messages: PiAgentChatCompletionMessage[];
  tools?: Array<{
    name: string;
    description: string;
    inputSchema: unknown;
  }>;
  config: {
    model: string;
    streamIdleTimeoutMs?: number;
    wakeTimeoutMs?: number;
    temperatureMilli?: number;
    maxOutputTokens?: number;
    maxToolRounds?: number;
    repeatedToolCallLimit?: number;
    finalMessageFallbackText?: string;
  };
  client?:
    | { mode: "fake" }
    | {
        mode: "live";
        baseUrl: string;
        apiKey?: string;
      };
}

export interface PiAgentToolRequest {
  wakeId: string;
  callId: string;
  providerItemId?: string;
  name: string;
  argumentsJson: string;
}

export interface PiAgentTransportMetrics extends OpenAiResponsesTransportMetrics {
  toolRoundCount: number;
}

export interface OpenAiResponsesBufferedCancellation {
  reasonCode: string;
  summary: string;
  cancelledAt: string;
}

interface NativeBrainWakeProviderStateInput {
  module_id: string;
  strategy_id: string;
  profile_fingerprint: string;
  provider_fingerprint: string;
  payload_version: string;
  payload: unknown;
  expires_at?: string;
}

export interface BrainWakeExecutor {
  wake(
    request: BrainWakeRequest,
    buffers: BridgeBufferClient,
    options?: { signal?: AbortSignal },
  ): Promise<BrainWakeExecutionResult> | BrainWakeExecutionResult;
}

export function brainWakeStreamItemsFromExecutionResult(
  request: BrainWakeRequest,
  result: BrainWakeExecutionResult,
): BrainWakeStreamItem[] {
  if (result.stream !== undefined) {
    assertTerminalBrainWakeStream(request, result.stream);
    return result.stream;
  }

  return [
    ...result.events.map(
      (event): BrainWakeStreamItem => ({ type: "event", event }),
    ),
    {
      type: "actions",
      batch: {
        wakeId: request.wakeId,
        sessionId: request.sessionId,
        actions: result.actions,
      },
    },
  ];
}

function assertTerminalBrainWakeStream(
  request: BrainWakeRequest,
  stream: readonly BrainWakeStreamItem[],
): void {
  const terminal = stream.at(-1);
  if (terminal?.type !== "actions" && terminal?.type !== "wake_failed") {
    throw new Error(
      `brain wake ${request.wakeId} stream must end with actions or wake_failed`,
    );
  }
}

export interface BrainWakeBufferInput {
  brain: BrainImplementationHandle;
  sessionId: BrainWakeRequest["sessionId"];
  bodyStateJson: Uint8Array;
  systemPrompt: string;
  roleAssemblyJson: Uint8Array;
  wakeId: string;
}

export interface BrainWakeSessionBufferInput {
  brain: BrainImplementationHandle;
  sessionId: BrainWakeRequest["sessionId"];
  systemPrompt: string;
  roleAssemblyJson: Uint8Array;
  wakeId: string;
}

export interface NativeSessionStateSummary {
  handle: number;
  sessionId: string;
  agentId: string;
  profileId: string;
  kind: string;
  status: string;
}

export interface NativeProfileMemoryCaps {
  maxRecordsPerProfile?: number;
  maxKeyBytes?: number;
  maxContentBytes?: number;
}

export interface NativeProfileMemoryRecord {
  profileId: string;
  targetType: "profile" | "user";
  targetId: string;
  key: string;
  content: string;
  metadataJson: string;
  revision: number;
  createdAt: string;
  updatedAt: string;
}

export interface NativeSessionMemoryRecord {
  record_id: string;
  session_id: string;
  scope: { scope_type: string; scope_id: string };
  branch_id?: string | null;
  shape: { shape_id: string; version: number };
  status: "active" | "superseded" | "archived";
  revision: number;
  content: unknown;
  evidence_refs: unknown[];
  source: string;
  confidence: number;
  durability_rationale: string;
  supersedes_record_id?: string | null;
  superseded_by_record_id?: string | null;
  archived_at?: string | null;
  archive_reason?: string | null;
  created_at: string;
  updated_at: string;
}

export interface NativeSessionMemoryQuery {
  session_id?: string;
  branch_id?: string;
  scope_type?: string;
  shape_id?: string;
  include_superseded?: boolean;
  include_archived?: boolean;
  page?: { limit?: number; offset?: number };
}

export interface NativeBranchAwareSessionMemoryQuery {
  session_id: string;
  active_branch_id?: string | null;
  include_ancestors: boolean;
  include_siblings: boolean;
  shape_id?: string | null;
  prompt_context_only: boolean;
  page?: { limit?: number; offset?: number } | null;
}

export interface NativeSessionMemoryPromptContext {
  records: NativeSessionMemoryRecord[];
  diagnostics: {
    descriptor_id: string;
    descriptor_schema_version: number;
    session_id: string;
    active_branch_id?: string | null;
    selected_records: Array<{ record_id: string; shape_id: string }>;
    excluded_counts: {
      wrong_branch: number;
      sibling_branch: number;
      tool_only: number;
      archived: number;
      superseded: number;
      limit_exceeded: number;
      policy_disabled: number;
    };
    character_estimate: number;
    token_estimate: number;
    context_policy: "summary_context" | "tool_only";
  };
}

export type NativeProfileRegistryLifecycleStatus =
  | "active"
  | "paused"
  | "decommissioned"
  | "archived";

export interface NativeProfileRegistrySourceAssetRef {
  assetKind: string;
  path: string;
  contentHash?: string;
  lastSeenAt?: string;
  metadataJson: unknown;
}

export interface NativeProfileRegistryDerivedRuntimeRef {
  refKind: string;
  refId: string;
  status: string;
  updatedAt?: string;
  metadataJson: unknown;
}

export interface NativeProfileRegistryImportExportMetadata {
  importedFrom?: string;
  importedAt?: string;
  exportedTo?: string;
  exportedAt?: string;
  metadataJson: unknown;
}

export interface NativeProfileRegistryRecord {
  profileId: string;
  lifecycleStatus: NativeProfileRegistryLifecycleStatus;
  displayName?: string;
  summary?: string;
  defaultSessionKind?: "full" | "worker" | "delegated";
  agentId?: string;
  ownerId?: string;
  promptSoulMarkdown?: string;
  promptMemoryMarkdown?: string;
  activeRuntimeSettingsJson: unknown;
  sourceAssetRefs: NativeProfileRegistrySourceAssetRef[];
  derivedRuntimeRefs: NativeProfileRegistryDerivedRuntimeRef[];
  importExport: NativeProfileRegistryImportExportMetadata;
  revision: number;
  createdAt: string;
  updatedAt: string;
}

export interface NativeProfileRegistryQuery {
  lifecycleStatus?: NativeProfileRegistryLifecycleStatus;
  limit?: number;
  offset?: number;
}

export interface NativeProfilePurgeTableCount {
  table: string;
  rowsDeleted: number;
}

export interface NativeProfilePurgeReport {
  profileId: string;
  profileRegistryDeleted: boolean;
  sessionIds: string[];
  agentIds: string[];
  tableCounts: NativeProfilePurgeTableCount[];
  rowsDeleted: number;
}

export type NativeModelProviderStatus = "active" | "disabled" | "archived";
export type NativeModelProviderProtocol = "responses" | "chat_completions";
export type NativeModelProviderCredentialKind =
  | "api_key"
  | "openai_oauth"
  | "legacy_raw_api_key";

export interface NativeModelProviderCredential {
  hasSecret: boolean;
  secretRef?: string;
  updatedAt?: string;
  kind?: NativeModelProviderCredentialKind;
}

export interface NativeModelProviderRecord {
  alias: string;
  status: NativeModelProviderStatus;
  protocol: NativeModelProviderProtocol;
  providerKind: string;
  displayName?: string;
  description?: string;
  baseUrl?: string;
  modelId: string;
  contextWindowTokens?: number;
  maxOutputTokens?: number;
  temperatureMilli?: number;
  reasoningEffort?: string;
  reasoningFormat?: string;
  credential: NativeModelProviderCredential;
  metadataJson: unknown;
  revision: number;
  createdAt: string;
  updatedAt: string;
}

export interface NativeModelProviderWrite {
  alias: string;
  status: NativeModelProviderStatus;
  protocol: NativeModelProviderProtocol;
  providerKind: string;
  displayName?: string;
  description?: string;
  baseUrl?: string;
  modelId: string;
  contextWindowTokens?: number;
  maxOutputTokens?: number;
  temperatureMilli?: number;
  reasoningEffort?: string;
  reasoningFormat?: string;
  secret?: string;
  clearSecret?: boolean;
  metadataJson?: unknown;
  expectedRevision?: number;
  now: string;
}

export interface NativeModelProviderQuery {
  status?: NativeModelProviderStatus;
  aliasPrefix?: string;
  limit?: number;
  offset?: number;
}

export interface NativeModelProviderAffectedProfile {
  profileId: string;
  sessionIds: string[];
  configuredSessionIds: string[];
  activeSessionIds: string[];
}

export interface NativeModelProviderRefreshImpact {
  providerAlias: string;
  affectedProfiles: NativeModelProviderAffectedProfile[];
}

export interface NativeModelProviderRefreshImpactRequest {
  providerAlias: string;
}

export type NativeModelProviderRefreshMode = "none" | "plan" | "apply";

export interface NativeModelProviderRefreshPlanRequest {
  providerAlias: string;
  mode: NativeModelProviderRefreshMode;
}

export interface NativeModelProviderRefreshProfileAction {
  profileId: string;
  commandName: string;
  reason: string;
  plannedSummary: string;
  appliedSummary: string;
  blockedSummary: string;
  failureReasonCode: string;
}

export interface NativeModelProviderRefreshPlan {
  providerAlias: string;
  mode: NativeModelProviderRefreshMode;
  affectedProfiles: NativeModelProviderAffectedProfile[];
  actions: NativeModelProviderRefreshProfileAction[];
}

export type NativeRoleplayLoreRecord = Record<string, unknown>;
export type NativeRoleplayLoreWrite = Record<string, unknown>;
export type NativeRoleplayLoreReplace = Record<string, unknown>;
export type NativeRoleplayLoreSupersede = Record<string, unknown>;
export type NativeRoleplayLoreTombstone = Record<string, unknown>;
export type NativeRoleplayLoreQuery = Record<string, unknown>;
export type NativeRoleplayLoreProvenanceEvent = Record<string, unknown>;
export type NativeRoleplayLoreLayerRecord = Record<string, unknown>;
export type NativeRoleplayLoreLayerWrite = Record<string, unknown>;
export type NativeRoleplayLoreLayerUpdate = Record<string, unknown>;
export type NativeRoleplayLoreLayerArchive = Record<string, unknown>;
export type NativeRoleplayLoreLayerConfigRecord = Record<string, unknown>;
export type NativeRoleplayLoreLayerConfigWrite = Record<string, unknown>;
export type NativeRoleplayLoreLayerEntryLink = Record<string, unknown>;
export type NativeRoleplayLoreLayerEntryJoin = Record<string, unknown>;
export type NativeRoleplayLoreFactCapture = Record<string, unknown>;
export type NativeRoleplayLoreEntryPromotion = Record<string, unknown>;
export type NativeRoleplayChatLayersWrite = Record<string, unknown>;
export type NativeRoleplayChatLayerRecord = Record<string, unknown>;
export type NativeLoreRecallQuery = Record<string, unknown>;
export type NativeLoreRecallResult = Record<string, unknown>;
export type NativeLoreRecallTraceQuery = Record<string, unknown>;
export type NativeLoreRecallTraceRecord = Record<string, unknown>;

export interface NativeProfileMemoryQuery {
  profileId: string;
  targetType?: "profile" | "user";
  targetId?: string;
  limit?: number;
  offset?: number;
}

export interface NativeSimpleKvQuery {
  scopeType: string;
  scopeId: string;
  keyPrefix?: string;
  includeExpired?: boolean;
  expiredOnly?: boolean;
  now?: string;
  limit?: number;
  offset?: number;
}

export interface NativeSimpleKvRecord {
  scopeType: string;
  scopeId: string;
  key: string;
  valueJson: string;
  revision: number;
  createdAt: string;
  updatedAt: string;
  expiresAt?: string;
}

export interface NativeSimpleKvWrite {
  scopeType: string;
  scopeId: string;
  key: string;
  valueJson: string;
  now: string;
  expiresAt?: string;
}

export interface NativeSimpleKvDelete {
  scopeType: string;
  scopeId: string;
  key: string;
  expectedRevision: number;
}

export interface NativeProfileMemoryWrite {
  profileId: string;
  targetType: "profile" | "user";
  targetId?: string;
  key: string;
  content: string;
  metadataJson?: string;
  caps?: NativeProfileMemoryCaps;
}

export interface NativeProfileMemoryReplace {
  write: NativeProfileMemoryWrite;
  expectedRevision: number;
}

export interface NativeProfileMemoryDelete {
  profileId: string;
  targetType: "profile" | "user";
  targetId?: string;
  key: string;
  expectedRevision: number;
}

export interface NativeRuntimeSearchQuery {
  query: string;
  rowType?: "message" | "queue_message" | "session";
  sessionId?: string;
  agentId?: string;
  instanceId?: string;
  taskId?: string;
  eventKind?: string;
  recordedAfter?: string;
  recordedBefore?: string;
  limit?: number;
}

export interface NativeRuntimeSearchResult {
  rowType: "message" | "queue_message" | "session";
  rowKey: string;
  sequence?: number;
  sessionId?: string;
  agentId?: string;
  instanceId?: string;
  taskId?: string;
  eventKind?: string;
  recordedAt: string;
  title: string;
  body: string;
}

export type NativeRuntimeCounterScopeType =
  | "runtime"
  | "agent"
  | "instance"
  | "session";

export interface NativeRuntimeCounterQuery {
  scopeType?: NativeRuntimeCounterScopeType;
  scopeId?: string;
  counterName?: string;
  limit?: number;
  offset?: number;
}

export interface NativeRuntimeCounterRecord {
  scopeType: NativeRuntimeCounterScopeType;
  scopeId: string;
  counterName: string;
  value: number;
  updatedAt: string;
}

export interface NativeRuntimeCounterSummary {
  scopeType: NativeRuntimeCounterScopeType;
  scopeId: string;
  brainTurns: number;
  wakes: number;
  toolCalls: number;
  toolErrors: number;
  delegationsCreated: number;
  delegationsCompleted: number;
  delegationsFailed: number;
  delegationsTimedOut: number;
  delegationsCancelled: number;
  messages: number;
  completions: number;
  queueExpirations: number;
}

export interface NativeRuntimeDatabaseSize {
  databaseBytes: number;
  pageCount: number;
  pageSizeBytes: number;
  freelistPages: number;
  freelistBytes: number;
  walBytes: number;
}

export interface NativeSchemaMigrationRecord {
  version: number;
  description: string;
  appliedAt: string;
}

export interface NativeRuntimeStorageCapability {
  name: string;
  supported: boolean;
  detail: string;
}

export interface NativeRuntimeRepositoryBackendRequirement {
  capability: string;
  required: boolean;
  detail: string;
}

export interface NativeRuntimeRepositoryGroupDiagnostic {
  groupId: string;
  label: string;
  correctnessSensitive: boolean;
  backendRequirements: NativeRuntimeRepositoryBackendRequirement[];
  notes: string[];
}

export interface NativeRuntimeModuleCapabilityStatus {
  capability: string;
  required: boolean;
  supported: boolean;
  backendVariant?: string;
}

export interface NativeRuntimeModuleLogicalStoreDiagnostic {
  storeName: string;
  description: string;
}

export interface NativeRuntimeModulePhysicalTableDiagnostic {
  tableName: string;
  logicalStore: string;
  physicalTable: string;
  declaration: string;
}

export interface NativeRuntimeModulePhysicalIndexDiagnostic {
  tableName: string;
  purpose: string;
  physicalIndex: string;
  columns: string[];
  unique: boolean;
}

export interface NativeRuntimeModuleRetentionDiagnostic {
  storeName: string;
  policy: string;
  detail?: string;
}

export interface NativeRuntimeModuleNamedDiagnostic {
  name: string;
  description: string;
}

export interface NativeRuntimeModuleQueryCatalogDiagnostic {
  queryId: string;
  storeName: string;
  description: string;
  parameterSchemaId?: string;
}

export interface NativeRuntimeModuleTransferHookDiagnostic {
  hookName: string;
  formatVersion: number;
}

export interface NativeRuntimeInstalledModuleSchemaDiagnostic {
  moduleId: string;
  installedVersion: number;
  descriptorFingerprint: string;
  installedAt: string;
  updatedAt: string;
}

export interface NativeRuntimeModuleSchemaDiagnostic {
  moduleId: string;
  ownerCrate: string;
  ownerModule: string;
  descriptorVersion: number;
  installedVersion?: number;
  migrationStatus: string;
  descriptorFingerprint: string;
  installedDescriptorFingerprint?: string;
  installedAt?: string;
  updatedAt?: string;
  capabilityStatus: NativeRuntimeModuleCapabilityStatus[];
  logicalStores: NativeRuntimeModuleLogicalStoreDiagnostic[];
  physicalTables: NativeRuntimeModulePhysicalTableDiagnostic[];
  physicalIndexes: NativeRuntimeModulePhysicalIndexDiagnostic[];
  retention: NativeRuntimeModuleRetentionDiagnostic[];
  repositoryContracts: NativeRuntimeModuleNamedDiagnostic[];
  queryCatalogEntries: NativeRuntimeModuleQueryCatalogDiagnostic[];
  exportHooks: NativeRuntimeModuleTransferHookDiagnostic[];
  importHooks: NativeRuntimeModuleTransferHookDiagnostic[];
  migrationNotes: string[];
  degradedReasons: string[];
  blockedReasons: string[];
}

export interface NativeRuntimeModuleSchemaRegistryDiagnostics {
  source: string;
  backendCapabilities: string[];
  modules: NativeRuntimeModuleSchemaDiagnostic[];
  orphanInstalledModules: NativeRuntimeInstalledModuleSchemaDiagnostic[];
}

export interface NativeRuntimeStorageTableCount {
  table: string;
  rows: number;
}

export interface NativeRuntimeQueryPlanCheck {
  name: string;
  usesIndex: boolean;
  detail: string;
}

export interface NativeRuntimeStoragePressureSignal {
  name: string;
  active: boolean;
  severity: string;
  observedValue: number;
  thresholdValue?: number;
  detail: string;
}

export interface NativeRuntimeStorageConnectionHealth {
  backend: string;
  status: string;
  maxConnections: number;
  activeConnections: number;
  idleConnections: number;
  totalOpened: number;
  checkoutCount: number;
  checkoutReuseCount: number;
  reconnectAttempts: number;
  reconnectSuccesses: number;
  closedConnectionsDiscarded: number;
  lastError?: string;
}

export interface NativeRuntimeStorageDiagnostics {
  backend: string;
  backendLabel: string;
  schemaVersion: number;
  supportedSchemaVersion: number;
  migrations: NativeSchemaMigrationRecord[];
  size: NativeRuntimeDatabaseSize;
  tableCounts: NativeRuntimeStorageTableCount[];
  capabilities: NativeRuntimeStorageCapability[];
  repositoryGroups: NativeRuntimeRepositoryGroupDiagnostic[];
  connectionHealth: NativeRuntimeStorageConnectionHealth;
  moduleRegistry: NativeRuntimeModuleSchemaRegistryDiagnostics;
  indexChecks: NativeRuntimeQueryPlanCheck[];
  searchHealthy: boolean;
  pressureSignals: NativeRuntimeStoragePressureSignal[];
  pressure: boolean;
}

export interface NativeBufferedBrainRunModuleDiagnostics {
  module_label: string;
  active_run_count: number;
}

export interface NativeBufferedBrainRunDiagnostic {
  module_label: string;
  wake_id: string;
  queued_stream_item_count: number;
  pending_tool_request_count: number;
  submitted_tool_output_count: number;
  age_ms: number;
  wake_timeout_ms: number;
  terminal: boolean;
  cancelled: boolean;
  has_error: boolean;
  started_at: string;
  last_transition_at: string;
}

export interface NativeBufferedBrainRunDiagnostics {
  active_run_count: number;
  modules: NativeBufferedBrainRunModuleDiagnostics[];
  runs: NativeBufferedBrainRunDiagnostic[];
}

export interface NativeBufferedBrainRunCleanupModuleReport {
  module_label: string;
  active_runs: number;
  terminal_runs: number;
  cancelled_nonterminal_runs: number;
  removed_runs: number;
}

export interface NativeBufferedBrainRunCleanupSummary {
  active_runs: number;
  terminal_runs: number;
  cancelled_nonterminal_runs: number;
  removed_runs: number;
  modules: NativeBufferedBrainRunCleanupModuleReport[];
}

export interface NativeRuntimeMaintenancePolicy {
  expireQueuedMessagesAt?: string;
  purgeTerminalQueuedMessagesBefore?: string;
  expireProviderWireStatesAt?: string;
  compactSessionMemoryAt?: string;
  sessionMemoryMaxActiveRecordsPerScope?: number;
  sessionMemoryArchiveBatchSize?: number;
  runWalCheckpoint?: boolean;
  runOptimize?: boolean;
}

export interface NativeSessionMemoryCompactionReport {
  enabled: boolean;
  scopesInspected: number;
  retentionPressureScopes: number;
  scopesCompacted: number;
  sessionSummariesCreated: number;
  branchSummariesCreated: number;
  recordsArchived: number;
  recordsSuperseded: number;
  skippedScopes: number;
}

export interface NativeRuntimeMaintenanceReport {
  sizeBefore: NativeRuntimeDatabaseSize;
  sizeAfter: NativeRuntimeDatabaseSize;
  expiredQueueMessages: number;
  purgedTerminalQueueMessages: number;
  expiredProviderWireStates: number;
  sessionMemoryCompaction: NativeSessionMemoryCompactionReport;
  walCheckpointRan: boolean;
  optimizeRan: boolean;
}

export type NativeRuntimeConfigDiagnosticSeverity =
  | "error"
  | "warning"
  | "info";

export type NativeExternalBindingStatus =
  | "active"
  | "degraded"
  | "disconnected"
  | "archived";

export interface NativeRuntimeConfigDiagnostic {
  severity: NativeRuntimeConfigDiagnosticSeverity;
  code: string;
  path?: string;
  message: string;
}

export interface NativeRuntimeConfigValidationResult {
  diagnostics: NativeRuntimeConfigDiagnostic[];
}

export interface NativeToolMetadataPolicyValidationInput {
  tools: NativeToolMetadataPolicyTool[];
}

export interface NativeToolMetadataPolicyTool {
  name: string;
  description: string;
  aliases?: string[];
  category: string;
  toolsets: string[];
  surfaces: string[];
  safety: string[];
  output_shape: string;
  version: string;
  deprecated?: {
    reason: string;
    since: string;
    replacement?: string;
    sunset?: string;
  };
  replacement?: string;
  coexistence_note?: string;
  collision_notes?: string;
}

export interface NativeToolMetadataPolicyDiagnostic {
  severity: "error" | "warning" | "info";
  code: string;
  tool_name?: string;
  other_tool_name?: string;
  path?: string;
  message: string;
}

export interface NativeToolMetadataPolicyValidationResult {
  ok: boolean;
  diagnostics: NativeToolMetadataPolicyDiagnostic[];
}

export interface NativeLocalToolProfilePolicyValidationInput {
  profile: {
    id: string;
    enabled: boolean;
    system: boolean;
    readOnly: boolean;
    toolsets: string[];
    tools: string[];
  };
  catalog: {
    toolsets: string[];
    tools: string[];
  };
}

export interface NativeLocalToolProfilePolicyValidationIssue {
  reasonCode: string;
  path: string;
  message: string;
}

export interface NativeLocalToolProfilePolicyValidationResult {
  ok: boolean;
  issues: NativeLocalToolProfilePolicyValidationIssue[];
}

export interface NativeRuntimeConfigPlan {
  runtimeConfig: NativeRuntimeConfigDraft;
  diagnostics: NativeRuntimeConfigDiagnostic[];
  derivedScheduledJobs: NativeScheduledJobConfigDraft[];
  derivedMcpBindings: NativeMcpBindingConfigDraft[];
}

export interface NativeRuntimeConfigValidationInput {
  runtimeConfig: NativeRuntimeConfigDraft;
  profiles: NativeProfileRuntimeMetadata[];
}

export interface NativeRuntimeConfigDraft {
  profilesDir: string;
  skillsDir?: string;
  brains: NativeBrainConfigDraft[];
  sessions: NativeSessionConfigDraft[];
  scheduledJobs: NativeScheduledJobConfigDraft[];
  channelBindings: NativeChannelBindingConfigDraft[];
  mcpBindings: NativeMcpBindingConfigDraft[];
}

export interface NativeBrainConfigDraft {
  implementationId: string;
  profileId: string;
}

export interface NativeSessionConfigDraft {
  sessionId: string;
  agentId: string;
  profileId: string;
  kind: "full" | "worker" | "delegated";
  resourceLimits?: ResourceLimits;
  ownerId?: string;
  historyWindow?: SessionState["historyWindow"];
  maxHistoryMessages?: number;
  turnTimeoutMs?: number;
}

export interface NativeScheduledJobConfigDraft {
  id: string;
  schedule: string;
  shape: "host_job" | "session_wake" | "script_only" | "data_collection";
  jobKind?: string;
  targetSessionId?: string;
  script?: string;
  deliveryChannelId?: string;
}

export interface NativeChannelBindingConfigDraft {
  bindingId: string;
  adapterId: string;
  provider: string;
  agentId: string;
  instanceId?: string;
  sessionId?: string;
  profileId: string;
  externalChannelId: string;
  externalThreadId?: string;
  externalUserId?: string;
  conversationProjectId?: string;
  conversationChannelId?: number;
  providerSubscriptionId?: string;
  status: NativeExternalBindingStatus;
}

export interface NativeMcpBindingConfigDraft {
  bindingId: string;
  adapterId: string;
  agentId: string;
  instanceId?: string;
  sessionId?: string;
  profileId: string;
  serverNames: string[];
  endpointRef: string;
  transport: string;
  toolProfileKey: string;
  status: NativeExternalBindingStatus;
}

export interface NativeProfileRuntimeMetadata {
  profileId: string;
  brain?: {
    module?: string;
    strategy?: string;
  };
  runtime?: {
    defaultResourceLimits?: ResourceLimits;
    maxTurnDurationMs?: number;
    maxTokensPerTurn?: number;
  };
  sessionDefaults?: {
    ownerId?: string;
    maxHistoryMessages?: number;
    turnTimeoutMs?: number;
  };
  mcpConfig?: {
    bindingId?: string;
    endpointRef?: string;
    serverNames: string[];
    transport?: string;
    toolProfile?: string;
  };
  backgroundReview?: {
    enabled: boolean;
    reviewType?: "memory" | "skills" | "combined";
    schedule?: string;
  };
  channelDefaults?: {
    wakePolicy?: "subscription" | "manual" | "disabled";
  };
}

export interface NativeCreateProfilePlanInput {
  runtimeConfig: NativeRuntimeConfigDraft;
  profiles: NativeProfileRuntimeMetadata[];
  profileRegistry?: NativeProfileRegistryRuntimeMetadata[];
  request: NativeCreateProfileRequest;
}

export interface NativeCreateProfileRequest {
  profileId: string;
  displayName?: string;
  agentId?: string;
  sessionId?: string;
  implementationId?: string;
  kind?: "full" | "worker" | "delegated";
  providerAlias?: string;
  modelConfig?: NativeProfileModelConfigSeed;
  brain?: {
    module?: string;
    strategy?: string;
  };
  mcpBindings?: NativeCreateProfileMcpBindingRequest[];
  mcpToolProfile?: string;
  source?: NativeCreateProfileSourceRequest;
  now?: string;
  profileFileExists?: boolean;
}

export interface NativeCreateProfileMcpBindingRequest {
  serverId: string;
  bindingId?: string;
  adapterId?: string;
  serverNames?: string[];
  transport?: string;
  toolProfileKey?: string;
}

export interface NativeProfileRegistryRuntimeMetadata {
  profileId: string;
  lifecycleStatus?: NativeProfileRegistryLifecycleStatus;
  revision?: number;
}

export interface NativeCreateProfileSourceRequest {
  templateId?: string;
  sourceProfileId?: string;
  sourceBundlePath?: string;
}

export interface NativeProfileModelConfigSeed {
  provider: string;
  modelName: string;
  baseUrl?: string;
  api?: string;
  apiKeyEnv?: string;
  temperatureMilli?: number;
  maxOutputTokens?: number;
}

export interface NativeCreateProfilePlan {
  diagnostics: NativeRuntimeConfigDiagnostic[];
  registryWrite?: NativeProfileRegistryWrite;
  fileAssetActions: NativeCreateProfileFileAssetAction[];
  derivedRuntimeActions: NativeCreateProfileDerivedRuntimeAction[];
  profileSeed?: NativeCreateProfileSeedMetadata;
  runtimeBrain?: NativeBrainConfigDraft;
  runtimeSession?: NativeSessionConfigDraft;
  profileMcpConfig?: NativeProfileRuntimeMetadata["mcpConfig"];
  runtimeMcpBindings: NativeMcpBindingConfigDraft[];
}

export interface NativeProfileRegistryWrite {
  profileId: string;
  lifecycleStatus: NativeProfileRegistryLifecycleStatus;
  displayName?: string;
  summary?: string;
  defaultSessionKind?: "full" | "worker" | "delegated";
  agentId?: string;
  ownerId?: string;
  promptSoulMarkdown?: string;
  promptMemoryMarkdown?: string;
  activeRuntimeSettingsJson: unknown;
  sourceAssetRefs: NativeProfileRegistrySourceAssetRef[];
  derivedRuntimeRefs: NativeProfileRegistryDerivedRuntimeRef[];
  importExport: NativeProfileRegistryImportExportMetadata;
  now: string;
}

export interface NativeProfileRegistryUpdate {
  write: NativeProfileRegistryWrite;
  expectedRevision: number;
}

export interface NativeProfileRegistryMutationRequest {
  profileId: string;
  kind: "update" | "lifecycle" | "prompt";
  mode: "plan" | "apply";
  current: NativeProfileRegistryRecord;
  bodyJson: unknown;
  now: string;
}

export interface NativeProfileRegistryMutationPlan {
  ok: boolean;
  profileId: string;
  kind: "update" | "lifecycle" | "prompt";
  mode: "plan" | "apply";
  expectedRevision: number;
  current: NativeProfileRegistryRecord;
  next: NativeProfileRegistryRecord;
  nextWrite: NativeProfileRegistryWrite;
  diagnostics: NativeRuntimeConfigDiagnostic[];
  implications: {
    registryRevisionWillIncrement: boolean;
    profileFilesUnchanged: boolean;
    serviceConfigUnchanged: boolean;
    runtimeRebuildRecommended: boolean;
    lifecycleEffects: "none" | "archive_active_sessions_and_unregister_brain";
  };
}

export interface NativeCreateProfileFileAssetAction {
  kind: "write_profile_json";
  profileId: string;
  relativePath: string;
  overwrite: boolean;
  metadataJson: unknown;
}

export interface NativeCreateProfileDerivedRuntimeAction {
  kind:
    | "add_brain"
    | "add_session"
    | "add_profile_mcp_config"
    | "add_mcp_binding";
  refKind: string;
  refId: string;
  applyPhase: string;
  metadataJson: unknown;
}

export interface NativeCreateProfileSeedMetadata {
  profileId: string;
  displayName?: string;
  providerAlias: string;
  modelConfig: NativeProfileModelConfigSeed;
  brain: {
    module?: string;
    strategy?: string;
  };
  skillsMode: string;
}

export interface NativeQueuedMessageRecord {
  messageId: string;
  ownerSessionId?: string;
  ownerAgentId: string;
  fromAgent: string;
  toAgent: string;
  body: string;
  correlationId?: string;
  enqueuedAt: string;
  expiresAt: string;
  ttlMs: number;
  deliveryAttempts: number;
  state: "pending" | "delivered" | "expired" | "discarded" | "cancelled";
  terminalAt?: string;
  stateReason?: string;
}

export type NativeProviderStateStatus =
  | "unused"
  | "valid"
  | "missing"
  | "expired"
  | "invalidated"
  | "load_failed"
  | "save_failed";

export interface NativeProviderStateDiagnostic {
  sessionId: SessionId | string;
  moduleId: string;
  strategyId: string;
  status: NativeProviderStateStatus;
  payloadVersion?: string;
  payloadBytes?: number;
  createdAt?: string;
  updatedAt?: string;
  expiresAt?: string;
  lastWakeId?: string;
  invalidatedAt?: string;
  invalidationReason?: string;
}

export interface NativeChatReadModelEvent {
  event_id: string;
  session_id: string;
  sequence_id: number;
  created_at: string;
  kind: "message_created";
  payload: Record<string, unknown>;
}

export interface NativeChatReadModelPage {
  items: NativeChatReadModelEvent[];
  latest_cursor: string;
  has_more: boolean;
}

export interface NativeChatEventLogEvent {
  event_id: string;
  session_id: string;
  sequence_id: number;
  created_at: string;
  kind: string;
  payload: Record<string, unknown>;
}

export interface NativeChatEventLogPage {
  items: NativeChatEventLogEvent[];
  latest_cursor: string;
  has_more: boolean;
}

export interface NativeBridgeModule {
  readonly manifestVersion: number;
  readonly operationNames: readonly ManifestOperationName[];
  readonly wireShapeFingerprint: string;
  initializeEngine(config: EngineConfig): Promise<EngineHandle>;
  shutdownEngine(request: ShutdownRequest): Promise<ShutdownSummary>;
  registerBrainImplementation(
    registration: BrainImplementationRegistration,
  ): Promise<BrainImplementationHandle>;
  replaceBrainImplementation(
    registration: BrainImplementationRegistration,
  ): Promise<BrainImplementationHandle>;
  unregisterBrainImplementationForProfile(
    profileId: ProfileId,
  ): Promise<BrainImplementationHandle>;
  registerBrainRuntime(
    registration: BrainImplementationRegistration,
    executor: BrainWakeExecutor,
  ): Promise<BrainImplementationHandle>;
  replaceBrainRuntime(
    registration: BrainImplementationRegistration,
    executor: BrainWakeExecutor,
  ): Promise<BrainImplementationHandle>;
  clearBrainProviderState(input: {
    brain: BrainImplementationHandle;
    sessionId: SessionId;
    wakeId: string;
  }): Promise<Unit>;
  wakeBrain(
    request: BrainWakeRequest,
    options?: { signal?: AbortSignal },
  ): Promise<BrainWakeAccepted>;
  submitBrainEvent(event: BrainEventEnvelope): Promise<EventReceipt>;
  submitBrainActions(batch: BrainActionBatch): Promise<ActionBatchReceipt>;
  registerPlatformAdapter(
    registration: PlatformAdapterRegistration,
  ): Promise<PlatformAdapterHandle>;
  validateToolMetadataPolicy(
    input: NativeToolMetadataPolicyValidationInput,
  ): Promise<NativeToolMetadataPolicyValidationResult>;
  validateLocalToolProfilePolicy(
    input: NativeLocalToolProfilePolicyValidationInput,
  ): Promise<NativeLocalToolProfilePolicyValidationResult>;
  validateRuntimeConfigDraft(
    input: NativeRuntimeConfigValidationInput,
  ): Promise<NativeRuntimeConfigValidationResult>;
  planRuntimeConfig(
    input: NativeRuntimeConfigValidationInput,
  ): Promise<NativeRuntimeConfigPlan>;
  planCreateProfile(
    input: NativeCreateProfilePlanInput,
  ): Promise<NativeCreateProfilePlan>;
  planProfileRegistryMutation(
    input: NativeProfileRegistryMutationRequest,
  ): Promise<NativeProfileRegistryMutationPlan>;
  injectDenDataUpdate(update: DenDataUpdate): Promise<EventReceipt>;
  injectExternalEvent(event: ExternalEvent): Promise<EventReceipt>;
  cancelDelegatedSession(
    delegatedSessionId: SessionId,
  ): Promise<NativeSessionStateSummary>;
  requestDelegatedCheckpoint(input: {
    parentSessionId: SessionId;
    delegatedSessionId: SessionId;
    reason: string;
  }): Promise<EventReceipt>;
  drainDelegatedSessions(input?: {
    parentSessionId?: SessionId;
  }): Promise<SessionId[]>;
  cleanupDelegatedResources(): Promise<DelegatedResourceCleanupReport>;
  delegatedSessionStatus(
    delegatedSessionId: SessionId,
  ): Promise<DelegatedSessionRuntimeStatus>;
  listSessions(): Promise<SessionState[]>;
  subscribeEvents(subscription: EventSubscription): Promise<SubscriptionHandle>;
  unsubscribeEvents(handle: SubscriptionHandle): Promise<Unit>;
  drainSubscriptionEvents(
    handle: SubscriptionHandle,
    maxEvents?: number,
  ): Promise<CoreEvent[]>;
  /**
   * Startup/config setup surface. This creates a Rust session for a configured
   * agent; it is not a brain wake-loop diagnostic bypass.
   */
  createSession(
    config: NativeSessionConfigInput,
  ): Promise<NativeSessionStateSummary>;
  ensureConfiguredSession(
    config: NativeSessionConfigInput,
  ): Promise<NativeSessionStateSummary>;
  archiveSession(sessionId: SessionId): Promise<NativeSessionStateSummary>;
  /**
   * Internal agent-to-agent routing trigger. This publishes through
   * CoreEngine::route_agent_message and runs scheduler evaluation.
   */
  routeAgentMessage(
    from: string,
    to: string,
    body: string,
    correlationId?: string,
  ): Promise<EventReceipt>;
  enqueueBodyFollowUpMessage(input: {
    sessionId: SessionId;
    from: AgentId;
    body: string;
    correlationId?: string;
  }): Promise<NativeQueuedMessageRecord>;
  registerScheduledWakeJob(input: {
    jobId: string;
    targetSessionId: SessionId;
    intervalMs?: number;
    firstDueAt: string;
  }): Promise<ScheduledJobSummary>;
  registerScheduledHostJob(
    input: ScheduledHostJobRegistrationInput,
  ): Promise<ScheduledJobSummary>;
  listScheduledJobs(
    query?: ScheduledJobListQuery,
  ): Promise<ScheduledJobSummary[]>;
  listScheduledRuns(
    query?: ScheduledRunListQuery,
  ): Promise<ScheduledRunSummary[]>;
  claimScheduledHostRuns(
    query: ScheduledHostRunClaimQuery,
  ): Promise<ScheduledRunSummary[]>;
  requestScheduledHostJobRun(
    input: ScheduledHostJobManualRunRequest,
  ): Promise<ScheduledRunSummary | undefined>;
  completeScheduledHostRun(
    input: ScheduledHostRunCompletionInput,
  ): Promise<Unit>;
  runSchedulerTick(): Promise<SchedulerTickReport>;
  requestScheduledJobRun(
    jobId: string,
  ): Promise<ScheduledRunSummary | undefined>;
  pauseScheduledJob(jobId: string): Promise<Unit>;
  resumeScheduledJob(input: {
    jobId: string;
    nextDueAt: string;
  }): Promise<Unit>;
  /**
   * Runtime-local helper: projects body state in Rust and builds the three
   * runtime-buffer handles used by a registered brain wake.
   */
  buildBrainWakeRequest(input: BrainWakeBufferInput): Promise<BrainWakeRequest>;
  buildBrainWakeRequestForSession(
    input: BrainWakeSessionBufferInput,
  ): Promise<BrainWakeRequest>;
  diagnosticProjectBodyStateJson(sessionId: string): Promise<Uint8Array>;
  diagnosticSubmitBrainActionsJson(
    wakeId: string,
    sessionId: string,
    actions: BrainActionBatch["actions"],
  ): Promise<ActionBatchReceipt>;
  diagnosticCountRows(table: string): Promise<number>;
  databaseSize(): Promise<NativeRuntimeDatabaseSize>;
  storageDiagnostics(): Promise<NativeRuntimeStorageDiagnostics>;
  bufferedBrainRunDiagnostics(): Promise<NativeBufferedBrainRunDiagnostics>;
  cleanupBufferedBrainRuns(input: {
    reasonCode: string;
    summary: string;
  }): Promise<NativeBufferedBrainRunCleanupSummary>;
  storageSchema(): Promise<NativeRuntimeModuleSchemaRegistryDiagnostics>;
  createProfileRegistryRecord(
    write: NativeProfileRegistryWrite,
  ): Promise<NativeProfileRegistryRecord>;
  updateProfileRegistryRecord(
    update: NativeProfileRegistryUpdate,
  ): Promise<NativeProfileRegistryRecord>;
  listProfileRegistryRecords(
    query?: NativeProfileRegistryQuery,
  ): Promise<NativeProfileRegistryRecord[]>;
  getProfileRegistryRecord(
    profileId: string,
  ): Promise<NativeProfileRegistryRecord | undefined>;
  purgeProfile(profileId: string): Promise<NativeProfilePurgeReport>;
  upsertModelProvider(
    write: NativeModelProviderWrite,
  ): Promise<NativeModelProviderRecord>;
  listModelProviders(
    query?: NativeModelProviderQuery,
  ): Promise<NativeModelProviderRecord[]>;
  getModelProvider(
    alias: string,
  ): Promise<NativeModelProviderRecord | undefined>;
  getModelProviderSecret(alias: string): Promise<string | undefined>;
  modelProviderRefreshImpact(
    request: NativeModelProviderRefreshImpactRequest,
  ): Promise<NativeModelProviderRefreshImpact>;
  planModelProviderRefresh(
    request: NativeModelProviderRefreshPlanRequest,
  ): Promise<NativeModelProviderRefreshPlan>;
  createLoreLayer(
    write: NativeRoleplayLoreLayerWrite,
  ): Promise<NativeRoleplayLoreLayerRecord>;
  getLoreLayer(
    layerId: string,
  ): Promise<NativeRoleplayLoreLayerRecord | undefined>;
  listLoreLayers(profileId: string): Promise<NativeRoleplayLoreLayerRecord[]>;
  updateLoreLayer(
    update: NativeRoleplayLoreLayerUpdate,
  ): Promise<NativeRoleplayLoreLayerRecord>;
  archiveLoreLayer(
    archive: NativeRoleplayLoreLayerArchive,
  ): Promise<NativeRoleplayLoreLayerRecord>;
  setChatLayers(write: NativeRoleplayChatLayersWrite): Promise<void>;
  getChatLayers(chatId: string): Promise<NativeRoleplayChatLayerRecord[]>;
  toggleChatLayer(input: {
    chatId: string;
    layerId: string;
    enabled: boolean;
  }): Promise<void>;
  reorderChatLayers(input: {
    chatId: string;
    layerIds: string[];
  }): Promise<void>;
  addLoreEntry(
    write: NativeRoleplayLoreWrite,
  ): Promise<NativeRoleplayLoreRecord>;
  replaceLoreEntry(
    replace: NativeRoleplayLoreReplace,
  ): Promise<NativeRoleplayLoreRecord>;
  supersedeLoreEntry(
    supersede: NativeRoleplayLoreSupersede,
  ): Promise<[NativeRoleplayLoreRecord, NativeRoleplayLoreRecord]>;
  tombstoneLoreEntry(
    tombstone: NativeRoleplayLoreTombstone,
  ): Promise<NativeRoleplayLoreRecord>;
  queryLoreEntries(
    query: NativeRoleplayLoreQuery,
  ): Promise<NativeRoleplayLoreRecord[]>;
  getLoreEntry(recordId: string): Promise<NativeRoleplayLoreRecord | undefined>;
  loreEntryProvenanceEvents(
    recordId: string,
  ): Promise<NativeRoleplayLoreProvenanceEvent[]>;
  addEntryToLayer(link: NativeRoleplayLoreLayerEntryLink): Promise<void>;
  removeEntryFromLayer(input: {
    layerId: string;
    recordId: string;
  }): Promise<void>;
  setEntryConstant(input: {
    layerId: string;
    recordId: string;
    isConstant: boolean;
  }): Promise<void>;
  listEntriesByLayer(
    layerId: string,
  ): Promise<NativeRoleplayLoreLayerEntryJoin[]>;
  recallLore(query: NativeLoreRecallQuery): Promise<NativeLoreRecallResult>;
  captureLoreFact(
    capture: NativeRoleplayLoreFactCapture,
  ): Promise<NativeRoleplayLoreLayerEntryJoin>;
  promoteLoreEntry(
    promotion: NativeRoleplayLoreEntryPromotion,
  ): Promise<NativeRoleplayLoreLayerEntryJoin>;
  getLoreLayerConfig(
    layerId: string,
  ): Promise<NativeRoleplayLoreLayerConfigRecord | undefined>;
  setLoreLayerConfig(
    write: NativeRoleplayLoreLayerConfigWrite,
  ): Promise<NativeRoleplayLoreLayerConfigRecord>;
  listRecallTraces(
    query: NativeLoreRecallTraceQuery,
  ): Promise<NativeLoreRecallTraceRecord[]>;
  getRecallTrace(
    traceId: string,
  ): Promise<NativeLoreRecallTraceRecord | undefined>;
  runMaintenance(
    policy: NativeRuntimeMaintenancePolicy,
  ): Promise<NativeRuntimeMaintenanceReport>;
  listMemorySpaceDescriptors(): Promise<MemorySpaceDescriptor[]>;
  querySessionMemoryRecords(
    query: NativeSessionMemoryQuery,
  ): Promise<NativeSessionMemoryRecord[]>;
  buildSessionMemoryPromptContext(
    query: NativeBranchAwareSessionMemoryQuery,
  ): Promise<NativeSessionMemoryPromptContext>;
  saveMemoryProposal(
    proposal: MemoryProposalEnvelope,
  ): Promise<MemoryProposalRecord>;
  planCaptureMemoryProposals(input: unknown): Promise<unknown>;
  listMemoryProposals(
    query: MemoryProposalQuery,
  ): Promise<MemoryProposalRecord[]>;
  saveSessionActivityDigest(
    digest: SessionActivityDigest,
  ): Promise<SessionActivityDigest>;
  listSessionActivityDigests(
    query: SessionActivityDigestQuery,
  ): Promise<SessionActivityDigest[]>;
  saveContextCompactionArtifact(
    artifact: ContextCompactionArtifact,
  ): Promise<ContextCompactionArtifact>;
  listContextCompactionArtifacts(
    query: ContextCompactionArtifactQuery,
  ): Promise<ContextCompactionArtifact[]>;
  recordMemoryGovernanceDecision(
    decision: MemoryGovernanceDecisionInput,
  ): Promise<MemoryGovernanceDecisionRecord>;
  planRoleplayAssistantAlternative(input: unknown): Promise<unknown>;
  planRoleplaySessionLifecycle(input: unknown): Promise<unknown>;
  planRoleplayChatLayerBinding(input: unknown): Promise<unknown>;
  normalizeRoleplayLoreSearchControls(input: unknown): Promise<unknown>;
  readRoleplaySceneState(input: unknown): Promise<unknown>;
  planRoleplaySceneStateUpdate(input: unknown): Promise<unknown>;
  buildRoleplayPromptContext(input: unknown): Promise<unknown>;
  roleplaySpeakerIdentity(input: unknown): Promise<unknown>;
  writeRoleplayCharacter(input: unknown): Promise<unknown>;
  mergeRoleplayCharacter(input: unknown): Promise<unknown>;
  writeRoleplayPlayerPersona(input: unknown): Promise<unknown>;
  mergeRoleplayPlayerPersona(input: unknown): Promise<unknown>;
  patchRoleplaySessionMetadata(input: unknown): Promise<unknown>;
  normalizeRoleplayNarratorConfig(input: unknown): Promise<unknown>;
  roleplayNarratorMandatoryExploreRequests(input: unknown): Promise<unknown[]>;
  roleplayNarratorAutoCaptureRequest(
    input: unknown,
  ): Promise<unknown | undefined>;
  startRoleplayNarratorTurn(input: unknown): Promise<unknown>;
  nextRoleplayNarratorPhase(input: unknown): Promise<unknown>;
  roleplayNarratorReviewRequestsRevision(feedback: string): Promise<boolean>;
  saveMessageSlot(input: unknown): Promise<void>;
  saveMessageVariant(input: unknown): Promise<unknown>;
  createChatMessageSlot(input: unknown): Promise<unknown>;
  createChatMessageVariant(input: unknown): Promise<unknown>;
  chatReadModelPage(input: unknown): Promise<NativeChatReadModelPage>;
  appendChatEvent(input: unknown): Promise<NativeChatEventLogEvent>;
  queryChatEvents(input: unknown): Promise<NativeChatEventLogPage>;
  queryMessageSlots(query: unknown): Promise<unknown[]>;
  queryMessageVariants(query: unknown): Promise<unknown[]>;
  selectActiveMessageVariant(input: unknown): Promise<unknown>;
  selectActiveChatMessageVariant(input: unknown): Promise<unknown>;
  deleteChatMessageVariant(input: unknown): Promise<unknown>;
  reorderChatMessageVariants(input: unknown): Promise<unknown[]>;
  deleteMessageVariant(input: unknown): Promise<unknown>;
  reorderMessageVariants(input: unknown): Promise<unknown[]>;
  saveConversationBranch(input: unknown): Promise<unknown>;
  createChatConversationBranch(input: unknown): Promise<unknown>;
  ensureActiveChatConversationBranch(input: unknown): Promise<unknown>;
  queryConversationBranches(query: unknown): Promise<unknown[]>;
  getConversationBranchState(input: unknown): Promise<unknown>;
  selectActiveConversationBranch(input: unknown): Promise<unknown>;
  updateConversationBranchHead(input: unknown): Promise<unknown>;
  saveConversationSnapshot(input: unknown): Promise<unknown>;
  createChatConversationSnapshot(input: unknown): Promise<unknown>;
  queryConversationSnapshots(query: unknown): Promise<unknown[]>;
  resolveConversationJump(input: unknown): Promise<unknown>;
  saveAttachment(input: unknown): Promise<unknown>;
  createChatAttachment(input: unknown): Promise<unknown>;
  queryAttachments(query: unknown): Promise<unknown[]>;
  removeAttachment(input: unknown): Promise<unknown>;
  removeChatAttachment(input: unknown): Promise<unknown>;
  saveDataBankScope(input: unknown): Promise<unknown>;
  createChatDataBankScope(input: unknown): Promise<unknown>;
  queryDataBankScopes(query: unknown): Promise<unknown[]>;
  removeDataBankScope(input: unknown): Promise<unknown>;
  removeChatDataBankScope(input: unknown): Promise<unknown>;
  providerStateDiagnostics(
    limit?: number,
  ): Promise<NativeProviderStateDiagnostic[]>;
  runOpenAiResponsesBrain(
    input: OpenAiResponsesBrainRunInput,
  ): Promise<BrainWakeExecutionResult>;
  exchangeOpenAiOauthCode(
    input: NativeOpenAiOauthCodeExchangeInput,
  ): Promise<NativeOpenAiOauthCodeExchangeResult>;
  startOpenAiResponsesBrain(input: OpenAiResponsesBrainRunInput): Promise<{
    wakeId: string;
  }>;
  drainOpenAiResponsesBrainStream(input: {
    wakeId: string;
    maxItems?: number;
  }): Promise<{
    wakeId: string;
    items: BrainWakeStreamItem[];
    toolRequests: OpenAiResponsesToolRequest[];
    terminal: boolean;
    providerState?: BrainWakeProviderStateOutput;
    transportMetrics?: OpenAiResponsesTransportMetrics;
    credentialSecretUpdate?: OpenAiResponsesCredentialSecretUpdate;
    cancellation?: OpenAiResponsesBufferedCancellation;
    error?: string;
  }>;
  submitOpenAiResponsesToolOutput(input: {
    wakeId: string;
    callId: string;
    output: string;
    isError: boolean;
  }): Promise<{ ok: true; wakeId: string; callId: string }>;
  cancelOpenAiResponsesBrain(input: {
    wakeId: string;
    reasonCode: string;
    summary: string;
  }): Promise<{
    ok: true;
    wakeId: string;
    cancelled: boolean;
    terminal: boolean;
    cancellation?: OpenAiResponsesBufferedCancellation;
  }>;
  startPiAgentBrain(input: PiAgentBrainRunInput): Promise<{
    wakeId: string;
  }>;
  drainPiAgentBrainStream(input: {
    wakeId: string;
    maxItems?: number;
  }): Promise<{
    wakeId: string;
    items: BrainWakeStreamItem[];
    toolRequests: PiAgentToolRequest[];
    terminal: boolean;
    transportMetrics?: PiAgentTransportMetrics;
    cancellation?: OpenAiResponsesBufferedCancellation;
    error?: string;
  }>;
  submitPiAgentToolOutput(input: {
    wakeId: string;
    callId: string;
    output: string;
    isError: boolean;
  }): Promise<{ ok: true; wakeId: string; callId: string }>;
  cancelPiAgentBrain(input: {
    wakeId: string;
    reasonCode: string;
    summary: string;
  }): Promise<{
    ok: true;
    wakeId: string;
    cancelled: boolean;
    terminal: boolean;
    cancellation?: OpenAiResponsesBufferedCancellation;
  }>;
  listProfileMemory(
    query: NativeProfileMemoryQuery,
  ): Promise<NativeProfileMemoryRecord[]>;
  listSimpleKv(query: NativeSimpleKvQuery): Promise<NativeSimpleKvRecord[]>;
  putSimpleKv(write: NativeSimpleKvWrite): Promise<NativeSimpleKvRecord>;
  deleteSimpleKv(input: NativeSimpleKvDelete): Promise<NativeSimpleKvRecord>;
  getProfileMemory(input: {
    profileId: string;
    targetType: "profile" | "user";
    targetId?: string;
    key: string;
  }): Promise<NativeProfileMemoryRecord | undefined>;
  addProfileMemory(
    write: NativeProfileMemoryWrite,
  ): Promise<NativeProfileMemoryRecord>;
  replaceProfileMemory(
    replace: NativeProfileMemoryReplace,
  ): Promise<NativeProfileMemoryRecord>;
  removeProfileMemory(
    remove: NativeProfileMemoryDelete,
  ): Promise<NativeProfileMemoryRecord>;
  searchRuntime(
    query: NativeRuntimeSearchQuery,
  ): Promise<NativeRuntimeSearchResult[]>;
  queryRuntimeCounters(
    query: NativeRuntimeCounterQuery,
  ): Promise<NativeRuntimeCounterRecord[]>;
  runtimeSummary(input: {
    scopeType: NativeRuntimeCounterScopeType;
    scopeId?: string;
  }): Promise<NativeRuntimeCounterSummary>;
  resetRuntimeCounters(query: NativeRuntimeCounterQuery): Promise<number>;
  /** @deprecated Diagnostic helper. Use diagnosticProjectBodyStateJson. */
  projectBodyStateJson(sessionId: string): Promise<Uint8Array>;
  /** @deprecated Diagnostic helper. Use diagnosticSubmitBrainActionsJson. */
  submitBrainActionsJson(
    wakeId: string,
    sessionId: string,
    actions: BrainActionBatch["actions"],
  ): Promise<ActionBatchReceipt>;
  /** @deprecated Diagnostic helper. Use diagnosticCountRows. */
  countRows(table: string): Promise<number>;
  getBuffer(handle: RuntimeBufferHandle): Promise<RuntimeBufferView>;
  releaseBuffer(handle: RuntimeBufferHandle): Promise<Unit>;
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
    case "brain_wake_stream_result_v1":
      return toRawOpenAiResponsesBrainRunResult(
        toOpenAiResponsesBrainRunResult(
          input.value as RawOpenAiResponsesBrainRunResult,
        ) as BrainWakeExecutionResult & {
          transportMetrics?: OpenAiResponsesTransportMetrics;
        },
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

  const binding = new addon.NativeBridgeBinding();
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
    validateToolMetadataPolicy: unavailable("validate_tool_metadata_policy"),
    validateLocalToolProfilePolicy: unavailable(
      "validate_local_tool_profile_policy",
    ),
    validateRuntimeConfigDraft: unavailable("validate_runtime_config_draft"),
    planRuntimeConfig: unavailable("plan_runtime_config"),
    planCreateProfile: unavailable("plan_create_profile"),
    planProfileRegistryMutation: unavailable("plan_profile_registry_mutation"),
    injectExternalEvent: unavailable("inject_external_event"),
    injectDenDataUpdate: unavailable("inject_den_data_update"),
    enqueueBodyFollowUpMessage: unavailable("enqueue_body_follow_up_message"),
    archiveSession: unavailable("archive_session"),
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
    buildBrainWakeRequest: unavailable("wake_brain"),
    buildBrainWakeRequestForSession: unavailable("wake_brain"),
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
    modelProviderRefreshImpact: unavailable("initialize_engine"),
    planModelProviderRefresh: unavailable("initialize_engine"),
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
    listMemoryProposals: unavailable("initialize_engine"),
    saveSessionActivityDigest: unavailable("initialize_engine"),
    listSessionActivityDigests: unavailable("initialize_engine"),
    saveContextCompactionArtifact: unavailable("initialize_engine"),
    listContextCompactionArtifacts: unavailable("initialize_engine"),
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
    roleplayNarratorMandatoryExploreRequests: unavailable(
      "roleplay_narrator_mandatory_explore_requests",
    ),
    roleplayNarratorAutoCaptureRequest: unavailable(
      "roleplay_narrator_auto_capture_request",
    ),
    startRoleplayNarratorTurn: unavailable("start_roleplay_narrator_turn"),
    nextRoleplayNarratorPhase: unavailable("next_roleplay_narrator_phase"),
    roleplayNarratorReviewRequestsRevision: unavailable(
      "roleplay_narrator_review_requests_revision",
    ),
    saveMessageSlot: unavailable("save_message_slot"),
    saveMessageVariant: unavailable("save_message_variant"),
    createChatMessageSlot: unavailable("create_chat_message_slot"),
    createChatMessageVariant: unavailable("create_chat_message_variant"),
    chatReadModelPage: unavailable("chat_read_model_page"),
    appendChatEvent: unavailable("append_chat_event"),
    queryChatEvents: unavailable("query_chat_events"),
    queryMessageSlots: unavailable("query_message_slots"),
    queryMessageVariants: unavailable("query_message_variants"),
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
    removeAttachment: unavailable("remove_attachment"),
    removeChatAttachment: unavailable("remove_chat_attachment"),
    saveDataBankScope: unavailable("save_data_bank_scope"),
    createChatDataBankScope: unavailable("create_chat_data_bank_scope"),
    queryDataBankScopes: unavailable("query_data_bank_scopes"),
    removeDataBankScope: unavailable("remove_data_bank_scope"),
    removeChatDataBankScope: unavailable("remove_chat_data_bank_scope"),
    providerStateDiagnostics: unavailable("provider_state_diagnostics"),
    bufferedBrainRunDiagnostics: unavailable("buffered_brain_run_diagnostics"),
    cleanupBufferedBrainRuns: unavailable("cleanup_buffered_brain_runs"),
    runOpenAiResponsesBrain: unavailable("wake_brain"),
    exchangeOpenAiOauthCode: unavailable("wake_brain"),
    startOpenAiResponsesBrain: unavailable("wake_brain"),
    drainOpenAiResponsesBrainStream: unavailable("wake_brain"),
    submitOpenAiResponsesToolOutput: unavailable("wake_brain"),
    cancelOpenAiResponsesBrain: unavailable("wake_brain"),
    startPiAgentBrain: unavailable("wake_brain"),
    drainPiAgentBrainStream: unavailable("wake_brain"),
    submitPiAgentToolOutput: unavailable("wake_brain"),
    cancelPiAgentBrain: unavailable("wake_brain"),
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

function observeProviderStateWake(
  observations: Map<string, NativeProviderStateDiagnostic>,
  request: Pick<
    BrainWakeRequest,
    "sessionId" | "wakeId" | "providerState" | "providerStateAbsence"
  >,
  registration: BrainImplementationRegistration | undefined,
): void {
  const strategy = registration?.strategy;
  if (!strategy) return;
  const state = request.providerState;
  const status =
    state === undefined
      ? providerStateStatusFromAbsence(
          request.providerStateAbsence,
          strategy.providerState.mode,
        )
      : "valid";
  const diagnostic: NativeProviderStateDiagnostic = {
    sessionId: request.sessionId,
    moduleId: strategy.moduleId,
    strategyId: strategy.strategyId,
    status,
    lastWakeId: request.wakeId,
    ...(state === undefined
      ? {}
      : {
          payloadVersion: state.payloadVersion,
          payloadBytes: Buffer.byteLength(JSON.stringify(state.payload)),
          expiresAt: state.expiresAt,
        }),
  };
  observations.set(providerStateDiagnosticKey(diagnostic), diagnostic);
}

function observeProviderStateFailure(
  observations: Map<string, NativeProviderStateDiagnostic>,
  request: Pick<BrainWakeRequest, "sessionId" | "wakeId">,
  registration: BrainImplementationRegistration | undefined,
  status: Extract<NativeProviderStateStatus, "save_failed" | "load_failed">,
): void {
  const strategy = registration?.strategy;
  if (!strategy) return;
  const diagnostic: NativeProviderStateDiagnostic = {
    sessionId: request.sessionId,
    moduleId: strategy.moduleId,
    strategyId: strategy.strategyId,
    status,
    lastWakeId: request.wakeId,
  };
  observations.set(providerStateDiagnosticKey(diagnostic), diagnostic);
}

function providerStateStatusFromAbsence(
  absence: BrainWakeRequest["providerStateAbsence"] | undefined,
  mode: ProviderStateMode,
): NativeProviderStateStatus {
  if (mode === "unused" || absence === "module_does_not_use_state") {
    return "unused";
  }
  if (absence === "expired") return "expired";
  if (absence === "invalidated") return "invalidated";
  if (absence === "load_failed") return "load_failed";
  return "missing";
}

function toNativeProviderStateDiagnostic(
  raw: NativeProviderStateDiagnostic,
): NativeProviderStateDiagnostic {
  return {
    sessionId: raw.sessionId,
    moduleId: raw.moduleId,
    strategyId: raw.strategyId,
    status: raw.status,
    payloadVersion: raw.payloadVersion,
    payloadBytes: raw.payloadBytes,
    createdAt: raw.createdAt,
    updatedAt: raw.updatedAt,
    expiresAt: raw.expiresAt,
    lastWakeId: raw.lastWakeId,
    invalidatedAt: raw.invalidatedAt,
    invalidationReason: raw.invalidationReason,
  };
}

function mergeProviderStateDiagnostics(
  diagnostics: Iterable<NativeProviderStateDiagnostic>,
): NativeProviderStateDiagnostic[] {
  const byKey = new Map<string, NativeProviderStateDiagnostic>();
  for (const diagnostic of diagnostics) {
    const key = providerStateDiagnosticKey(diagnostic);
    const existing = byKey.get(key);
    if (
      existing === undefined ||
      providerStateDiagnosticPriority(diagnostic) >
        providerStateDiagnosticPriority(existing)
    ) {
      byKey.set(key, diagnostic);
    }
  }
  return [...byKey.values()];
}

function providerStateDiagnosticKey(
  diagnostic: Pick<
    NativeProviderStateDiagnostic,
    "sessionId" | "moduleId" | "strategyId"
  >,
): string {
  return `${diagnostic.sessionId}\u0000${diagnostic.moduleId}\u0000${diagnostic.strategyId}`;
}

function providerStateDiagnosticPriority(
  diagnostic: NativeProviderStateDiagnostic,
): number {
  switch (diagnostic.status) {
    case "save_failed":
      return 7;
    case "load_failed":
      return 6;
    case "invalidated":
      return diagnostic.invalidationReason === "superseded" ? 2 : 5;
    case "valid":
      return 4;
    case "expired":
      return 3;
    case "missing":
      return 2;
    case "unused":
      return 1;
  }
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
        inputSchema: tool.inputSchema,
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
        reason: "brain_requested_clear",
      };
      binding.applyBrainProviderStateOutputJson(
        input.brain,
        input.sessionId,
        input.wakeId,
        JSON.stringify(output),
      );
      return {};
    },
    wakeBrain: async (request, options) => {
      const validatedRequest = validateBridgeValue<BrainWakeRequest>({
        operation: "wake_brain",
        direction: "ts_to_rust",
        schema: brainWakeRequestSchema,
        value: request,
      });
      const executor = wakeExecutors.get(validatedRequest.brain);
      if (!executor) {
        throw new Error(
          `brain implementation handle ${validatedRequest.brain} is not registered in the TS runtime`,
        );
      }

      const result = await executor.wake(validatedRequest, module, options);
      for (const item of brainWakeStreamItemsFromExecutionResult(
        validatedRequest,
        result,
      )) {
        switch (item.type) {
          case "event":
            await module.submitBrainEvent(item.event);
            break;
          case "actions":
            await module.submitBrainActions(item.batch);
            break;
          case "wake_failed":
            throw new Error(
              `brain wake ${item.failure.wakeId} failed: ${item.failure.message}`,
            );
        }
      }
      if (result.providerState !== undefined) {
        try {
          binding.applyBrainProviderStateOutputJson(
            validatedRequest.brain,
            validatedRequest.sessionId,
            validatedRequest.wakeId,
            JSON.stringify(result.providerState),
          );
        } catch (error) {
          observeProviderStateFailure(
            providerStateObservations,
            validatedRequest,
            brainRegistrations.get(validatedRequest.brain),
            "save_failed",
          );
          await module.submitBrainEvent({
            wakeId: validatedRequest.wakeId,
            sessionId: validatedRequest.sessionId,
            event: {
              type: "provider_status",
              level: "degraded",
              message: `provider state save failed: ${errorMessage(error)}`,
            },
          });
        }
      }
      return validateBridgeValue<BrainWakeAccepted>({
        operation: "wake_brain",
        direction: "rust_to_ts",
        schema: brainWakeAcceptedSchema,
        value: { wakeId: validatedRequest.wakeId, accepted: true },
      });
    },
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
    validateToolMetadataPolicy: async (input) =>
      JSON.parse(
        binding.validateToolMetadataPolicyJson(JSON.stringify(input)),
      ) as NativeToolMetadataPolicyValidationResult,
    validateLocalToolProfilePolicy: async (input) =>
      JSON.parse(
        binding.validateLocalToolProfilePolicyJson(JSON.stringify(input)),
      ) as NativeLocalToolProfilePolicyValidationResult,
    validateRuntimeConfigDraft: async (input) =>
      JSON.parse(
        binding.validateRuntimeConfigDraftJson(
          JSON.stringify(toNativeRuntimeConfigValidationInput(input)),
        ),
      ) as NativeRuntimeConfigValidationResult,
    planRuntimeConfig: async (input) =>
      toNativeRuntimeConfigPlan(
        JSON.parse(
          binding.planRuntimeConfigJson(
            JSON.stringify(toNativeRuntimeConfigValidationInput(input)),
          ),
        ) as RawRuntimeConfigPlan,
      ),
    planCreateProfile: async (input) =>
      toNativeCreateProfilePlan(
        JSON.parse(
          binding.planCreateProfileJson(
            JSON.stringify(toNativeCreateProfilePlanInput(input)),
          ),
        ) as RawCreateProfilePlan,
      ),
    planProfileRegistryMutation: async (input) =>
      toNativeProfileRegistryMutationPlan(
        JSON.parse(
          binding.planProfileRegistryMutationJson(
            JSON.stringify(toRawProfileRegistryMutationRequest(input)),
          ),
        ) as RawProfileRegistryMutationPlan,
      ),
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
    createSession: async (config) => binding.createSession(config),
    ensureConfiguredSession: async (config) =>
      binding.ensureConfiguredSession(config),
    archiveSession: async (sessionId) => binding.archiveSession(sessionId),
    routeAgentMessage: async (from, to, body, correlationId) =>
      binding.routeAgentMessage(from, to, body, correlationId),
    enqueueBodyFollowUpMessage: async (input) =>
      binding.enqueueBodyFollowUpMessage(
        input.sessionId,
        input.from,
        input.body,
        input.correlationId ?? null,
      ),
    registerScheduledWakeJob: async (input) =>
      toScheduledJobSummary(
        JSON.parse(
          binding.registerScheduledWakeJobJson(
            input.jobId,
            input.targetSessionId,
            input.intervalMs,
            input.firstDueAt,
          ),
        ) as RawScheduledJobSummary,
      ),
    registerScheduledHostJob: async (input) =>
      toScheduledJobSummary(
        JSON.parse(
          binding.registerScheduledHostJobJson(
            input.jobId,
            input.jobKind,
            input.intervalMs,
            input.firstDueAt,
            JSON.stringify(input.payload ?? {}),
          ),
        ) as RawScheduledJobSummary,
      ),
    listScheduledJobs: async (query = {}) =>
      (
        JSON.parse(
          binding.listScheduledJobsJson(
            query.status,
            query.jobKind,
            query.limit,
            query.offset,
          ),
        ) as RawScheduledJobSummary[]
      ).map(toScheduledJobSummary),
    listScheduledRuns: async (query = {}) =>
      (
        JSON.parse(
          binding.listScheduledRunsJson(
            query.jobId,
            query.status,
            query.trigger,
            query.targetSessionId,
            query.limit,
            query.offset,
          ),
        ) as RawScheduledRunSummary[]
      ).map(toScheduledRunSummary),
    claimScheduledHostRuns: async (query) =>
      (
        JSON.parse(
          binding.claimScheduledHostRunsJson(
            query.supportedJobKinds,
            query.limit,
          ),
        ) as RawScheduledRunSummary[]
      ).map(toScheduledRunSummary),
    requestScheduledHostJobRun: async (input) => {
      const raw = JSON.parse(
        binding.requestScheduledHostJobRunJson(
          input.jobId,
          input.supportedJobKinds,
        ),
      ) as RawScheduledRunSummary | null;
      return raw ? toScheduledRunSummary(raw) : undefined;
    },
    completeScheduledHostRun: async (input) => {
      binding.completeScheduledHostRun(
        input.runId,
        input.status,
        JSON.stringify(input.output ?? {}),
        input.error,
      );
      return {};
    },
    runSchedulerTick: async () =>
      toSchedulerTickReport(
        JSON.parse(binding.runSchedulerTickJson()) as RawSchedulerTickReport,
      ),
    requestScheduledJobRun: async (jobId) => {
      const raw = JSON.parse(
        binding.requestScheduledJobRunJson(jobId),
      ) as RawScheduledRunSummary | null;
      return raw ? toScheduledRunSummary(raw) : undefined;
    },
    pauseScheduledJob: async (jobId) => {
      binding.pauseScheduledJob(jobId);
      return {};
    },
    resumeScheduledJob: async (input) => {
      binding.resumeScheduledJob(input.jobId, input.nextDueAt);
      return {};
    },
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
      const request = {
        brain: input.brain,
        sessionId: input.sessionId,
        bodyState: buffered.bodyState as RuntimeBufferHandle,
        systemPrompt: buffered.systemPrompt as RuntimeBufferHandle,
        roleAssembly: buffered.roleAssembly as RuntimeBufferHandle,
        wakeId: input.wakeId,
        ...providerStateFromBufferedWake(buffered),
      };
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
    diagnosticCountRows: async (table) => binding.countRows(table),
    databaseSize: async () => binding.databaseSize(),
    storageDiagnostics: async () => binding.storageDiagnostics(),
    bufferedBrainRunDiagnostics: async () =>
      JSON.parse(
        binding.bufferedBrainRunDiagnosticsJson(),
      ) as NativeBufferedBrainRunDiagnostics,
    cleanupBufferedBrainRuns: async (input) =>
      JSON.parse(
        binding.cleanupBufferedBrainRunsJson(input.reasonCode, input.summary),
      ) as NativeBufferedBrainRunCleanupSummary,
    storageSchema: async () => binding.storageSchema(),
    createProfileRegistryRecord: async (write) =>
      toNativeProfileRegistryRecord(
        validateBridgeValue<RawProfileRegistryRecord>({
          operation: "create_profile_registry_record",
          direction: "rust_to_ts",
          schema: rawProfileRegistryRecordSchema,
          value: JSON.parse(
            binding.createProfileRegistryRecordJson(
              JSON.stringify(toRawProfileRegistryWrite(write)),
            ),
          ),
        }),
      ),
    updateProfileRegistryRecord: async (update) =>
      toNativeProfileRegistryRecord(
        validateBridgeValue<RawProfileRegistryRecord>({
          operation: "update_profile_registry_record",
          direction: "rust_to_ts",
          schema: rawProfileRegistryRecordSchema,
          value: JSON.parse(
            binding.updateProfileRegistryRecordJson(
              JSON.stringify(toRawProfileRegistryUpdate(update)),
            ),
          ),
        }),
      ),
    listProfileRegistryRecords: async (query = {}) =>
      validateBridgeValue<RawProfileRegistryRecord[]>({
        operation: "list_profile_registry_records",
        direction: "rust_to_ts",
        schema: rawProfileRegistryRecordArraySchema,
        value: JSON.parse(
          binding.listProfileRegistryRecordsJson(
            JSON.stringify(toRawProfileRegistryQuery(query)),
          ),
        ),
      }).map(toNativeProfileRegistryRecord),
    getProfileRegistryRecord: async (profileId) => {
      const raw = JSON.parse(
        binding.getProfileRegistryRecordJson(profileId),
      ) as RawProfileRegistryRecord | null;
      return raw
        ? toNativeProfileRegistryRecord(
            validateBridgeValue<RawProfileRegistryRecord>({
              operation: "get_profile_registry_record",
              direction: "rust_to_ts",
              schema: rawProfileRegistryRecordSchema,
              value: raw,
            }),
          )
        : undefined;
    },
    purgeProfile: async (profileId) =>
      toNativeProfilePurgeReport(
        validateBridgeValue<RawProfilePurgeReport>({
          operation: "purge_profile",
          direction: "rust_to_ts",
          schema: rawProfilePurgeReportSchema,
          value: JSON.parse(binding.purgeProfileJson(profileId)),
        }),
      ),
    upsertModelProvider: async (write) =>
      toNativeModelProviderRecord(
        validateBridgeValue<RawModelProviderRecord>({
          operation: "upsert_model_provider",
          direction: "rust_to_ts",
          schema: rawModelProviderRecordSchema,
          value: JSON.parse(
            binding.upsertModelProviderJson(
              JSON.stringify(toRawModelProviderWrite(write)),
            ),
          ),
        }),
      ),
    listModelProviders: async (query = {}) =>
      validateBridgeValue<RawModelProviderRecord[]>({
        operation: "list_model_providers",
        direction: "rust_to_ts",
        schema: rawModelProviderRecordArraySchema,
        value: JSON.parse(
          binding.listModelProvidersJson(
            JSON.stringify(toRawModelProviderQuery(query)),
          ),
        ),
      }).map(toNativeModelProviderRecord),
    getModelProvider: async (alias) => {
      const raw = JSON.parse(
        binding.getModelProviderJson(alias),
      ) as RawModelProviderRecord | null;
      return raw
        ? toNativeModelProviderRecord(
            validateBridgeValue<RawModelProviderRecord>({
              operation: "get_model_provider",
              direction: "rust_to_ts",
              schema: rawModelProviderRecordSchema,
              value: raw,
            }),
          )
        : undefined;
    },
    getModelProviderSecret: async (alias) =>
      (JSON.parse(binding.getModelProviderSecretJson(alias)) as
        | string
        | null) ?? undefined,
    modelProviderRefreshImpact: async (request) =>
      toNativeModelProviderRefreshImpact(
        validateBridgeValue<RawModelProviderRefreshImpact>({
          operation: "model_provider_refresh_impact",
          direction: "rust_to_ts",
          schema: rawModelProviderRefreshImpactSchema,
          value: JSON.parse(
            binding.modelProviderRefreshImpactJson(
              JSON.stringify({ provider_alias: request.providerAlias }),
            ),
          ),
        }),
      ),
    planModelProviderRefresh: async (request) =>
      toNativeModelProviderRefreshPlan(
        validateBridgeValue<RawModelProviderRefreshPlan>({
          operation: "plan_model_provider_refresh",
          direction: "rust_to_ts",
          schema: rawModelProviderRefreshPlanSchema,
          value: JSON.parse(
            binding.planModelProviderRefreshJson(
              JSON.stringify({
                provider_alias: request.providerAlias,
                mode: request.mode,
              }),
            ),
          ),
        }),
      ),
    createLoreLayer: async (write) =>
      JSON.parse(
        binding.createLoreLayerJson(JSON.stringify(write)),
      ) as NativeRoleplayLoreLayerRecord,
    getLoreLayer: async (layerId) =>
      (JSON.parse(
        binding.getLoreLayerJson(layerId),
      ) as NativeRoleplayLoreLayerRecord | null) ?? undefined,
    listLoreLayers: async (profileId) =>
      JSON.parse(
        binding.listLoreLayersJson(profileId),
      ) as NativeRoleplayLoreLayerRecord[],
    updateLoreLayer: async (update) =>
      JSON.parse(
        binding.updateLoreLayerJson(JSON.stringify(update)),
      ) as NativeRoleplayLoreLayerRecord,
    archiveLoreLayer: async (archive) =>
      JSON.parse(
        binding.archiveLoreLayerJson(JSON.stringify(archive)),
      ) as NativeRoleplayLoreLayerRecord,
    setChatLayers: async (write) =>
      binding.setChatLayersJson(JSON.stringify(write)),
    getChatLayers: async (chatId) =>
      JSON.parse(
        binding.getChatLayersJson(chatId),
      ) as NativeRoleplayChatLayerRecord[],
    toggleChatLayer: async (input) =>
      binding.toggleChatLayerJson(
        JSON.stringify({
          chat_id: input.chatId,
          layer_id: input.layerId,
          enabled: input.enabled,
        }),
      ),
    reorderChatLayers: async (input) =>
      binding.reorderChatLayersJson(
        JSON.stringify({
          chat_id: input.chatId,
          layer_ids: input.layerIds,
        }),
      ),
    addLoreEntry: async (write) =>
      JSON.parse(
        binding.addLoreEntryJson(JSON.stringify(write)),
      ) as NativeRoleplayLoreRecord,
    replaceLoreEntry: async (replace) =>
      JSON.parse(
        binding.replaceLoreEntryJson(JSON.stringify(replace)),
      ) as NativeRoleplayLoreRecord,
    supersedeLoreEntry: async (supersede) =>
      JSON.parse(binding.supersedeLoreEntryJson(JSON.stringify(supersede))) as [
        NativeRoleplayLoreRecord,
        NativeRoleplayLoreRecord,
      ],
    tombstoneLoreEntry: async (tombstone) =>
      JSON.parse(
        binding.tombstoneLoreEntryJson(JSON.stringify(tombstone)),
      ) as NativeRoleplayLoreRecord,
    queryLoreEntries: async (query) =>
      JSON.parse(
        binding.queryLoreEntriesJson(JSON.stringify(query)),
      ) as NativeRoleplayLoreRecord[],
    getLoreEntry: async (recordId) =>
      (JSON.parse(
        binding.getLoreEntryJson(recordId),
      ) as NativeRoleplayLoreRecord | null) ?? undefined,
    loreEntryProvenanceEvents: async (recordId) =>
      JSON.parse(
        binding.loreEntryProvenanceEventsJson(recordId),
      ) as NativeRoleplayLoreProvenanceEvent[],
    addEntryToLayer: async (link) =>
      binding.addEntryToLayerJson(JSON.stringify(link)),
    removeEntryFromLayer: async (input) =>
      binding.removeEntryFromLayerJson(
        JSON.stringify({
          layer_id: input.layerId,
          record_id: input.recordId,
        }),
      ),
    setEntryConstant: async (input) =>
      binding.setEntryConstantJson(
        JSON.stringify({
          layer_id: input.layerId,
          record_id: input.recordId,
          is_constant: input.isConstant,
        }),
      ),
    listEntriesByLayer: async (layerId) =>
      JSON.parse(
        binding.listEntriesByLayerJson(layerId),
      ) as NativeRoleplayLoreLayerEntryJoin[],
    recallLore: async (query) =>
      JSON.parse(
        binding.recallLoreJson(JSON.stringify(query)),
      ) as NativeLoreRecallResult,
    captureLoreFact: async (capture) =>
      JSON.parse(
        binding.captureLoreFactJson(JSON.stringify(capture)),
      ) as NativeRoleplayLoreLayerEntryJoin,
    promoteLoreEntry: async (promotion) =>
      JSON.parse(
        binding.promoteLoreEntryJson(JSON.stringify(promotion)),
      ) as NativeRoleplayLoreLayerEntryJoin,
    getLoreLayerConfig: async (layerId) =>
      (JSON.parse(
        binding.getLoreLayerConfigJson(layerId),
      ) as NativeRoleplayLoreLayerConfigRecord | null) ?? undefined,
    setLoreLayerConfig: async (write) =>
      JSON.parse(
        binding.setLoreLayerConfigJson(JSON.stringify(write)),
      ) as NativeRoleplayLoreLayerConfigRecord,
    listRecallTraces: async (query) =>
      JSON.parse(
        binding.listRecallTracesJson(JSON.stringify(query)),
      ) as NativeLoreRecallTraceRecord[],
    getRecallTrace: async (traceId) =>
      (JSON.parse(
        binding.getRecallTraceJson(traceId),
      ) as NativeLoreRecallTraceRecord | null) ?? undefined,
    runMaintenance: async (policy) => binding.runMaintenance(policy),
    listMemorySpaceDescriptors: async () =>
      JSON.parse(
        binding.listMemorySpaceDescriptorsJson(),
      ) as MemorySpaceDescriptor[],
    querySessionMemoryRecords: async (query) =>
      JSON.parse(
        binding.querySessionMemoryRecordsJson(JSON.stringify(query)),
      ) as NativeSessionMemoryRecord[],
    buildSessionMemoryPromptContext: async (query) =>
      JSON.parse(
        binding.buildSessionMemoryPromptContextJson(JSON.stringify(query)),
      ) as NativeSessionMemoryPromptContext,
    saveMemoryProposal: async (proposal) =>
      JSON.parse(
        binding.saveMemoryProposalJson(JSON.stringify(proposal)),
      ) as MemoryProposalRecord,
    planCaptureMemoryProposals: async (input) =>
      JSON.parse(
        binding.planCaptureMemoryProposalsJson(JSON.stringify(input)),
      ) as unknown,
    listMemoryProposals: async (query) =>
      JSON.parse(
        binding.listMemoryProposalsJson(JSON.stringify(query)),
      ) as MemoryProposalRecord[],
    saveSessionActivityDigest: async (digest) =>
      JSON.parse(
        binding.saveSessionActivityDigestJson(JSON.stringify(digest)),
      ) as SessionActivityDigest,
    listSessionActivityDigests: async (query) =>
      JSON.parse(
        binding.listSessionActivityDigestsJson(JSON.stringify(query)),
      ) as SessionActivityDigest[],
    saveContextCompactionArtifact: async (artifact) =>
      JSON.parse(
        binding.saveContextCompactionArtifactJson(JSON.stringify(artifact)),
      ) as ContextCompactionArtifact,
    listContextCompactionArtifacts: async (query) =>
      JSON.parse(
        binding.listContextCompactionArtifactsJson(JSON.stringify(query)),
      ) as ContextCompactionArtifact[],
    recordMemoryGovernanceDecision: async (decision) =>
      JSON.parse(
        binding.recordMemoryGovernanceDecisionJson(JSON.stringify(decision)),
      ) as MemoryGovernanceDecisionRecord,
    planRoleplayAssistantAlternative: async (input) =>
      JSON.parse(
        binding.planRoleplayAssistantAlternativeJson(JSON.stringify(input)),
      ) as unknown,
    planRoleplaySessionLifecycle: async (input) =>
      JSON.parse(
        binding.planRoleplaySessionLifecycleJson(JSON.stringify(input)),
      ) as unknown,
    planRoleplayChatLayerBinding: async (input) =>
      JSON.parse(
        binding.planRoleplayChatLayerBindingJson(JSON.stringify(input)),
      ) as unknown,
    normalizeRoleplayLoreSearchControls: async (input) =>
      JSON.parse(
        binding.normalizeRoleplayLoreSearchControlsJson(JSON.stringify(input)),
      ) as unknown,
    readRoleplaySceneState: async (input) =>
      JSON.parse(
        binding.readRoleplaySceneStateJson(JSON.stringify(input)),
      ) as unknown,
    planRoleplaySceneStateUpdate: async (input) =>
      JSON.parse(
        binding.planRoleplaySceneStateUpdateJson(JSON.stringify(input)),
      ) as unknown,
    buildRoleplayPromptContext: async (input) =>
      JSON.parse(
        binding.buildRoleplayPromptContextJson(JSON.stringify(input)),
      ) as unknown,
    roleplaySpeakerIdentity: async (input) =>
      JSON.parse(
        binding.roleplaySpeakerIdentityJson(JSON.stringify(input)),
      ) as unknown,
    writeRoleplayCharacter: async (input) =>
      JSON.parse(
        binding.writeRoleplayCharacterJson(JSON.stringify(input)),
      ) as unknown,
    mergeRoleplayCharacter: async (input) =>
      JSON.parse(
        binding.mergeRoleplayCharacterJson(JSON.stringify(input)),
      ) as unknown,
    writeRoleplayPlayerPersona: async (input) =>
      JSON.parse(
        binding.writeRoleplayPlayerPersonaJson(JSON.stringify(input)),
      ) as unknown,
    mergeRoleplayPlayerPersona: async (input) =>
      JSON.parse(
        binding.mergeRoleplayPlayerPersonaJson(JSON.stringify(input)),
      ) as unknown,
    patchRoleplaySessionMetadata: async (input) =>
      JSON.parse(
        binding.patchRoleplaySessionMetadataJson(JSON.stringify(input)),
      ) as unknown,
    normalizeRoleplayNarratorConfig: async (input) =>
      JSON.parse(
        binding.normalizeRoleplayNarratorConfigJson(JSON.stringify(input)),
      ) as unknown,
    roleplayNarratorMandatoryExploreRequests: async (input) =>
      JSON.parse(
        binding.roleplayNarratorMandatoryExploreRequestsJson(
          JSON.stringify(input),
        ),
      ) as unknown[],
    roleplayNarratorAutoCaptureRequest: async (input) =>
      JSON.parse(
        binding.roleplayNarratorAutoCaptureRequestJson(JSON.stringify(input)),
      ) as unknown | undefined,
    startRoleplayNarratorTurn: async (input) =>
      JSON.parse(
        binding.startRoleplayNarratorTurnJson(JSON.stringify(input)),
      ) as unknown,
    nextRoleplayNarratorPhase: async (input) =>
      JSON.parse(
        binding.nextRoleplayNarratorPhaseJson(JSON.stringify(input)),
      ) as unknown,
    roleplayNarratorReviewRequestsRevision: async (feedback) =>
      binding.roleplayNarratorReviewRequestsRevision(feedback),
    saveMessageSlot: async (input) =>
      binding.saveMessageSlotJson(JSON.stringify(input)),
    saveMessageVariant: async (input) =>
      JSON.parse(
        binding.saveMessageVariantJson(JSON.stringify(input)),
      ) as unknown,
    createChatMessageSlot: async (input) =>
      JSON.parse(
        binding.createChatMessageSlotJson(JSON.stringify(input)),
      ) as unknown,
    createChatMessageVariant: async (input) =>
      JSON.parse(
        binding.createChatMessageVariantJson(JSON.stringify(input)),
      ) as unknown,
    chatReadModelPage: async (input) =>
      validateBridgeValue<NativeChatReadModelPage>({
        operation: "chat_read_model_page",
        direction: "rust_to_ts",
        schema: chatReadModelPageSchema,
        value: JSON.parse(
          binding.chatReadModelPageJson(JSON.stringify(input)),
        ) as unknown,
      }),
    appendChatEvent: async (input) =>
      validateBridgeValue<NativeChatEventLogEvent>({
        operation: "append_chat_event",
        direction: "rust_to_ts",
        schema: chatEventLogEventSchema,
        value: JSON.parse(
          binding.appendChatEventJson(JSON.stringify(input)),
        ) as unknown,
      }),
    queryChatEvents: async (input) =>
      validateBridgeValue<NativeChatEventLogPage>({
        operation: "query_chat_events",
        direction: "rust_to_ts",
        schema: chatEventLogPageSchema,
        value: JSON.parse(
          binding.queryChatEventsJson(JSON.stringify(input)),
        ) as unknown,
      }),
    queryMessageSlots: async (query) =>
      JSON.parse(
        binding.queryMessageSlotsJson(JSON.stringify(query)),
      ) as unknown[],
    queryMessageVariants: async (query) =>
      JSON.parse(
        binding.queryMessageVariantsJson(JSON.stringify(query)),
      ) as unknown[],
    selectActiveMessageVariant: async (input) =>
      JSON.parse(
        binding.selectActiveMessageVariantJson(JSON.stringify(input)),
      ) as unknown,
    selectActiveChatMessageVariant: async (input) =>
      JSON.parse(
        binding.selectActiveChatMessageVariantJson(JSON.stringify(input)),
      ) as unknown,
    deleteChatMessageVariant: async (input) =>
      JSON.parse(
        binding.deleteChatMessageVariantJson(JSON.stringify(input)),
      ) as unknown,
    reorderChatMessageVariants: async (input) =>
      JSON.parse(
        binding.reorderChatMessageVariantsJson(JSON.stringify(input)),
      ) as unknown[],
    deleteMessageVariant: async (input) =>
      JSON.parse(
        binding.deleteMessageVariantJson(JSON.stringify(input)),
      ) as unknown,
    reorderMessageVariants: async (input) =>
      JSON.parse(
        binding.reorderMessageVariantsJson(JSON.stringify(input)),
      ) as unknown[],
    saveConversationBranch: async (input) =>
      JSON.parse(
        binding.saveConversationBranchJson(JSON.stringify(input)),
      ) as unknown,
    createChatConversationBranch: async (input) =>
      JSON.parse(
        binding.createChatConversationBranchJson(JSON.stringify(input)),
      ) as unknown,
    ensureActiveChatConversationBranch: async (input) =>
      JSON.parse(
        binding.ensureActiveChatConversationBranchJson(JSON.stringify(input)),
      ) as unknown,
    queryConversationBranches: async (query) =>
      JSON.parse(
        binding.queryConversationBranchesJson(JSON.stringify(query)),
      ) as unknown[],
    getConversationBranchState: async (input) =>
      JSON.parse(
        binding.getConversationBranchStateJson(JSON.stringify(input)),
      ) as unknown,
    selectActiveConversationBranch: async (input) =>
      JSON.parse(
        binding.selectActiveConversationBranchJson(JSON.stringify(input)),
      ) as unknown,
    updateConversationBranchHead: async (input) =>
      JSON.parse(
        binding.updateConversationBranchHeadJson(JSON.stringify(input)),
      ) as unknown,
    saveConversationSnapshot: async (input) =>
      JSON.parse(
        binding.saveConversationSnapshotJson(JSON.stringify(input)),
      ) as unknown,
    createChatConversationSnapshot: async (input) =>
      JSON.parse(
        binding.createChatConversationSnapshotJson(JSON.stringify(input)),
      ) as unknown,
    queryConversationSnapshots: async (query) =>
      JSON.parse(
        binding.queryConversationSnapshotsJson(JSON.stringify(query)),
      ) as unknown[],
    resolveConversationJump: async (input) =>
      JSON.parse(
        binding.resolveConversationJumpJson(JSON.stringify(input)),
      ) as unknown,
    saveAttachment: async (input) =>
      JSON.parse(binding.saveAttachmentJson(JSON.stringify(input))) as unknown,
    createChatAttachment: async (input) =>
      JSON.parse(
        binding.createChatAttachmentJson(JSON.stringify(input)),
      ) as unknown,
    queryAttachments: async (query) =>
      JSON.parse(
        binding.queryAttachmentsJson(JSON.stringify(query)),
      ) as unknown[],
    removeAttachment: async (input) =>
      JSON.parse(
        binding.removeAttachmentJson(JSON.stringify(input)),
      ) as unknown,
    removeChatAttachment: async (input) =>
      JSON.parse(
        binding.removeChatAttachmentJson(JSON.stringify(input)),
      ) as unknown,
    saveDataBankScope: async (input) =>
      JSON.parse(
        binding.saveDataBankScopeJson(JSON.stringify(input)),
      ) as unknown,
    createChatDataBankScope: async (input) =>
      JSON.parse(
        binding.createChatDataBankScopeJson(JSON.stringify(input)),
      ) as unknown,
    queryDataBankScopes: async (query) =>
      JSON.parse(
        binding.queryDataBankScopesJson(JSON.stringify(query)),
      ) as unknown[],
    removeDataBankScope: async (input) =>
      JSON.parse(
        binding.removeDataBankScopeJson(JSON.stringify(input)),
      ) as unknown,
    removeChatDataBankScope: async (input) =>
      JSON.parse(
        binding.removeChatDataBankScopeJson(JSON.stringify(input)),
      ) as unknown,
    providerStateDiagnostics: async (limit = 100) => {
      const stored = binding
        .providerStateDiagnostics(limit)
        .map(toNativeProviderStateDiagnostic);
      return validateBridgeValue<NativeProviderStateDiagnostic[]>({
        operation: "provider_state_diagnostics",
        direction: "rust_to_ts",
        schema: providerStateDiagnosticArraySchema,
        value: mergeProviderStateDiagnostics([
          ...providerStateObservations.values(),
          ...stored,
        ]).slice(0, limit),
      });
    },
    runOpenAiResponsesBrain: async (input) => {
      const validatedInput = validateBridgeValue<OpenAiResponsesBrainRunInput>({
        operation: "run_openai_responses_brain",
        direction: "ts_to_rust",
        schema: openAiResponsesBrainRunInputSchema,
        value: input,
      });
      const raw = validateBridgeValue<RawOpenAiResponsesBrainRunResult>({
        operation: "run_openai_responses_brain",
        direction: "rust_to_ts",
        schema: rawOpenAiResponsesBrainRunResultSchema,
        value: JSON.parse(
          await binding.runOpenaiResponsesBrainJson(
            JSON.stringify(
              toNativeOpenAiResponsesBrainRunInput(validatedInput),
            ),
          ),
        ),
      });
      return toOpenAiResponsesBrainRunResult(raw);
    },
    exchangeOpenAiOauthCode: async (input) => {
      const raw = JSON.parse(
        await binding.exchangeOpenaiOauthCodeJson(
          JSON.stringify({
            issuer: input.issuer,
            clientId: input.clientId,
            redirectUri: input.redirectUri,
            code: input.code,
            codeVerifier: input.codeVerifier,
            now: input.now,
          }),
        ),
      ) as RawOpenAiOauthCodeExchangeResult;
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
    startOpenAiResponsesBrain: async (input) => {
      const raw = JSON.parse(
        binding.startOpenaiResponsesBrainJson(
          JSON.stringify(toNativeOpenAiResponsesBrainRunInput(input)),
        ),
      ) as RawOpenAiResponsesBufferedStartResult;
      return { wakeId: raw.wake_id };
    },
    drainOpenAiResponsesBrainStream: async (input) => {
      const raw = JSON.parse(
        binding.drainOpenaiResponsesBrainStreamJson(
          input.wakeId,
          input.maxItems,
        ),
      ) as RawOpenAiResponsesBufferedDrainResult;
      return {
        wakeId: raw.wake_id,
        items: raw.items.map(toBrainWakeStreamItem),
        toolRequests: (raw.tool_requests ?? []).map((request) => ({
          wakeId: raw.wake_id,
          callId: request.call_id,
          providerItemId: request.provider_item_id ?? undefined,
          name: request.name,
          argumentsJson: request.arguments_json,
        })),
        terminal: raw.terminal,
        providerState: raw.provider_state
          ? toBrainWakeProviderStateOutput(raw.provider_state)
          : undefined,
        transportMetrics: raw.transport_metrics,
        credentialSecretUpdate: raw.credential_secret_update
          ? {
              providerAlias: raw.credential_secret_update.provider_alias,
              secret: raw.credential_secret_update.secret,
            }
          : undefined,
        cancellation: raw.cancellation
          ? {
              reasonCode: raw.cancellation.reason_code,
              summary: raw.cancellation.summary,
              cancelledAt: raw.cancellation.cancelled_at,
            }
          : undefined,
        error: typeof raw.error === "string" ? raw.error : undefined,
      };
    },
    submitOpenAiResponsesToolOutput: async (input) => {
      const raw = JSON.parse(
        binding.submitOpenaiResponsesToolOutputJson(
          JSON.stringify({
            wakeId: input.wakeId,
            callId: input.callId,
            output: input.output,
            isError: input.isError,
          }),
        ),
      ) as {
        ok: true;
        wake_id: string;
        call_id: string;
      };
      return {
        ok: true,
        wakeId: raw.wake_id,
        callId: raw.call_id,
      };
    },
    cancelOpenAiResponsesBrain: async (input) => {
      const raw = JSON.parse(
        binding.cancelOpenaiResponsesBrainJson(
          JSON.stringify({
            wakeId: input.wakeId,
            reasonCode: input.reasonCode,
            summary: input.summary,
          }),
        ),
      ) as RawOpenAiResponsesBufferedCancelResult;
      return {
        ok: true,
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
    startPiAgentBrain: async (input) => {
      const validatedInput = validateBridgeValue<PiAgentBrainRunInput>({
        operation: "start_pi_agent_brain",
        direction: "ts_to_rust",
        schema: piAgentBrainRunInputSchema,
        value: input,
      });
      const raw = JSON.parse(
        binding.startPiAgentBrainJson(
          JSON.stringify(toNativePiAgentBrainRunInput(validatedInput)),
        ),
      ) as RawPiAgentBufferedStartResult;
      return { wakeId: raw.wake_id };
    },
    drainPiAgentBrainStream: async (input) => {
      const raw = validateBridgeValue<RawPiAgentBufferedDrainResult>({
        operation: "drain_pi_agent_brain_stream",
        direction: "rust_to_ts",
        schema: rawPiAgentBufferedDrainResultSchema,
        value: JSON.parse(
          binding.drainPiAgentBrainStreamJson(input.wakeId, input.maxItems),
        ),
      });
      return {
        wakeId: raw.wake_id,
        items: raw.items.map(toBrainWakeStreamItem),
        toolRequests: (raw.tool_requests ?? []).map((request) => ({
          wakeId: raw.wake_id,
          callId: request.call_id,
          providerItemId: request.provider_item_id ?? undefined,
          name: request.name,
          argumentsJson: request.arguments_json,
        })),
        terminal: raw.terminal,
        transportMetrics: raw.transport_metrics
          ? {
              effectiveTransport: "rust-pi-agent",
              selectedStrategyId: "default",
              effectiveStrategyId: "default",
              fallbackReason: null,
              providerRequestCount:
                raw.transport_metrics.provider_request_count,
              continuationRoundCount: raw.transport_metrics.tool_round_count,
              providerRequestPayloadBytes: 0,
              providerEventCounts: {},
              firstTextDeltaLatencyMs: null,
              totalTurnDurationMs: 0,
              toolRoundCount: raw.transport_metrics.tool_round_count,
            }
          : undefined,
        cancellation: raw.cancellation
          ? {
              reasonCode: raw.cancellation.reason_code,
              summary: raw.cancellation.summary,
              cancelledAt: raw.cancellation.cancelled_at,
            }
          : undefined,
        error: typeof raw.error === "string" ? raw.error : undefined,
      };
    },
    submitPiAgentToolOutput: async (input) => {
      const raw = JSON.parse(
        binding.submitPiAgentToolOutputJson(
          JSON.stringify({
            wakeId: input.wakeId,
            callId: input.callId,
            output: input.output,
            isError: input.isError,
          }),
        ),
      ) as {
        ok: true;
        wake_id: string;
        call_id: string;
      };
      return {
        ok: true,
        wakeId: raw.wake_id,
        callId: raw.call_id,
      };
    },
    cancelPiAgentBrain: async (input) => {
      const raw = JSON.parse(
        binding.cancelPiAgentBrainJson(
          JSON.stringify({
            wakeId: input.wakeId,
            reasonCode: input.reasonCode,
            summary: input.summary,
          }),
        ),
      ) as RawOpenAiResponsesBufferedCancelResult;
      return {
        ok: true,
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
    listProfileMemory: async (query) => binding.listProfileMemory(query),
    listSimpleKv: async (query) => binding.listSimpleKv(query),
    putSimpleKv: async (write) => binding.putSimpleKv(write),
    deleteSimpleKv: async (input) => binding.deleteSimpleKv(input),
    getProfileMemory: async (input) =>
      binding.getProfileMemory(
        input.profileId,
        input.targetType,
        input.targetId,
        input.key,
      ) ?? undefined,
    addProfileMemory: async (write) => binding.addProfileMemory(write),
    replaceProfileMemory: async (replace) =>
      binding.replaceProfileMemory(replace),
    removeProfileMemory: async (remove) => binding.removeProfileMemory(remove),
    searchRuntime: async (query) => binding.searchRuntime(query),
    queryRuntimeCounters: async (query) => binding.queryRuntimeCounters(query),
    runtimeSummary: async (input) =>
      binding.runtimeSummary(input.scopeType, input.scopeId),
    resetRuntimeCounters: async (query) => binding.resetRuntimeCounters(query),
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

function toNativeBrainAction(action: BrainAction): unknown {
  switch (action.type) {
    case "send_message":
      return {
        type: action.type,
        message: {
          from: action.message.from,
          to: action.message.to,
          body: action.message.body,
          correlation_id: action.message.correlationId,
        },
      };
    case "request_delegation":
      return {
        type: action.type,
        profile_id: action.profileId,
        task_id: action.taskId,
        prompt: action.prompt,
        expected_output: action.expectedOutput,
        resource_limits: action.resourceLimits
          ? {
              workdir: action.resourceLimits.workdir,
              max_duration_ms: action.resourceLimits.maxDurationMs,
              max_delegation_depth: action.resourceLimits.maxDelegationDepth,
            }
          : undefined,
        timeout_ms: action.timeoutMs,
        priority: action.priority,
        fan_out_group_id: action.fanOutGroupId,
        fan_out_max_concurrency: action.fanOutMaxConcurrency,
        fan_out_failure_policy: action.fanOutFailurePolicy,
        correlation_id: action.correlationId,
        parent_consumption: action.parentConsumption,
        capacity_request: action.capacityRequest
          ? {
              member_id: action.capacityRequest.memberId,
              claim_ttl_ms: action.capacityRequest.claimTtlMs,
              fallback_policy: action.capacityRequest.fallbackPolicy,
            }
          : undefined,
      };
    case "deliver_completion":
      return {
        type: action.type,
        packet: {
          session_id: action.packet.sessionId,
          status: action.packet.status,
          summary: action.packet.summary,
        },
      };
  }
}

function toNativeOpenAiResponsesBrainRunInput(
  input: OpenAiResponsesBrainRunInput,
): unknown {
  return {
    wakeId: input.wakeId,
    sessionId: input.sessionId,
    bodyState: toNativeBodyState(input.bodyState),
    tools: input.tools?.map((tool) => ({
      name: tool.name,
      description: tool.description,
      inputSchema: tool.inputSchema,
    })),
    providerState: input.providerState
      ? toNativeProviderStateInput(input.providerState)
      : undefined,
    providerStateAbsence: input.providerStateAbsence,
    config: input.config,
    client:
      input.client?.mode === "live"
        ? {
            mode: "live",
            base_url: input.client.baseUrl,
            api_key: input.client.apiKey,
            auth_kind: input.client.authKind,
            provider_alias: input.client.providerAlias,
            oauth_credential_secret: input.client.oauthCredentialSecret,
          }
        : { mode: "fake" },
  };
}

function toNativePiAgentBrainRunInput(input: PiAgentBrainRunInput): unknown {
  return {
    wakeId: input.wakeId,
    sessionId: input.sessionId,
    messages: input.messages.map((message) => ({
      role: message.role,
      content: message.content,
      name: message.name,
      tool_call_id: message.toolCallId,
      tool_calls: message.toolCalls,
    })),
    tools: input.tools?.map((tool) => ({
      name: tool.name,
      description: tool.description,
      inputSchema: tool.inputSchema,
    })),
    config: input.config,
    client:
      input.client?.mode === "live"
        ? {
            mode: "live",
            base_url: input.client.baseUrl,
            api_key: input.client.apiKey,
          }
        : { mode: "fake" },
  };
}

function toNativeBodyState(state: BodyState): unknown {
  return {
    session: toNativeSessionState(state.session),
    pending_messages: state.pendingMessages.map(toNativeAgentMessage),
    recent_events: state.recentEvents.map(toNativeCoreEvent),
    child_completions: state.childCompletions.map(toNativeDelegatedCompletion),
    fan_out_groups: state.fanOutGroups.map(toNativeDelegatedFanOutGroup),
    delta_policy: {
      mode: state.deltaPolicy.mode,
      queue_owner: state.deltaPolicy.queueOwner,
      queued_message_ttl_ms: state.deltaPolicy.queuedMessageTtlMs,
      max_queued_messages: state.deltaPolicy.maxQueuedMessages,
    },
  };
}

function toBodyState(state: RawBodyState): BodyState {
  return {
    session: toSessionState(state.session),
    pendingMessages: state.pending_messages.map(toAgentMessage),
    recentEvents: state.recent_events.map(toCoreEvent),
    childCompletions: state.child_completions.map(toDelegatedCompletion),
    fanOutGroups: state.fan_out_groups.map(toDelegatedFanOutGroup),
    deltaPolicy: {
      mode: state.delta_policy.mode,
      queueOwner: state.delta_policy.queue_owner,
      queuedMessageTtlMs: state.delta_policy.queued_message_ttl_ms,
      maxQueuedMessages: state.delta_policy.max_queued_messages,
    },
  };
}

function toNativeSessionState(session: SessionState): unknown {
  return {
    handle: session.handle,
    session_id: session.sessionId,
    agent_id: session.agentId,
    profile_id: session.profileId,
    kind: session.kind,
    delegation: session.delegation
      ? {
          parent_session_id: session.delegation.parentSessionId,
          parent_agent_id: session.delegation.parentAgentId,
          source_wake_id: session.delegation.sourceWakeId,
          source_action_index: session.delegation.sourceActionIndex,
          requested_task_id: session.delegation.requestedTaskId,
          correlation_id: session.delegation.correlationId,
        }
      : undefined,
    resource_limits: {
      workdir: session.resourceLimits.workdir,
      max_duration_ms: session.resourceLimits.maxDurationMs,
      max_delegation_depth: session.resourceLimits.maxDelegationDepth,
    },
    tool_profile: {
      tools: session.toolProfile.tools.map((tool) => ({
        name: tool.name,
        description: tool.description,
        input_schema: tool.inputSchema,
      })),
    },
    history_window: session.historyWindow
      ? { max_messages: session.historyWindow.maxMessages }
      : undefined,
    status: session.status,
    brain_turn_count: session.brainTurnCount,
    created_at: session.createdAt,
    last_active_at: session.lastActiveAt,
  };
}

function toNativeAgentMessage(message: AgentMessage): RawAgentMessage {
  return {
    from: message.from,
    to: message.to,
    body: message.body,
    correlation_id: message.correlationId,
    projection: message.projection
      ? {
          visibility: message.projection.visibility,
          target_ref: message.projection.targetRef,
          work_ref: message.projection.workRef,
          reason: message.projection.reason,
        }
      : undefined,
  };
}

function toNativeCoreEvent(event: CoreEvent): unknown {
  switch (event.type) {
    case "session_created":
      return { type: event.type, state: toNativeSessionState(event.state) };
    case "session_archived":
      return { type: event.type, session_id: event.sessionId };
    case "agent_message_routed":
      return { type: event.type, message: toNativeAgentMessage(event.message) };
    case "delegation_lifecycle_observed":
      return {
        type: event.type,
        lifecycle: {
          parent_session_id: event.lifecycle.parentSessionId,
          delegated_session_id: event.lifecycle.delegatedSessionId,
          run_id: event.lifecycle.runId,
          phase: event.lifecycle.phase,
          detail: event.lifecycle.detail,
        },
      };
    case "external_event_injected":
      return {
        type: event.type,
        event: toNativeExternalEvent(event.event),
      };
    case "den_data_updated":
      return { type: event.type, update: toNativeDenDataUpdate(event.update) };
    case "brain_wake_requested":
      return { type: event.type, session_id: event.sessionId };
    case "brain_event_observed":
      return {
        type: event.type,
        session_id: event.sessionId,
        wake_id: event.wakeId,
        event: toNativeBrainEventForJson(event.event),
      };
    case "brain_actions_accepted":
      return {
        type: event.type,
        session_id: event.sessionId,
        count: event.count,
      };
    case "completion_packet_delivered":
      return {
        type: event.type,
        packet: {
          session_id: event.packet.sessionId,
          status: event.packet.status,
          summary: event.packet.summary,
        },
      };
  }
}

function toNativeBrainEventForJson(event: BrainEvent): unknown {
  switch (event.type) {
    case "started":
    case "finished":
      return { type: event.type };
    case "text_delta":
      return { type: event.type, text: event.text };
    case "reasoning_delta":
      return {
        type: event.type,
        text: event.text,
        format: event.format,
      };
    case "phase_change":
      return {
        type: event.type,
        phase: event.phase,
        message: event.message,
      };
    case "tool_call_started":
      return {
        type: event.type,
        tool_name: event.toolName,
        metadata: event.metadata
          ? toRawToolCallMetadata(event.metadata)
          : undefined,
      };
    case "tool_call_finished":
      return {
        type: event.type,
        tool_name: event.toolName,
        is_error: event.isError,
        metadata: event.metadata
          ? toRawToolCallMetadata(event.metadata)
          : undefined,
      };
    case "provider_status":
      return {
        type: event.type,
        level: event.level,
        message: event.message,
        metadata_json: event.metadataJson,
      };
  }
}

function toNativeDelegatedCompletion(
  completion: BodyState["childCompletions"][number],
): unknown {
  return {
    run_id: completion.runId,
    child_session_id: completion.childSessionId,
    requested_task_id: completion.requestedTaskId,
    source_wake_id: completion.sourceWakeId,
    source_action_index: completion.sourceActionIndex,
    correlation_id: completion.correlationId,
    parent_consumption: completion.parentConsumption,
    packet: {
      session_id: completion.packet.sessionId,
      status: completion.packet.status,
      summary: completion.packet.summary,
    },
  };
}

function toDelegatedCompletion(
  completion: RawDelegatedCompletion,
): BodyState["childCompletions"][number] {
  return {
    runId: completion.run_id,
    childSessionId: completion.child_session_id,
    requestedTaskId: completion.requested_task_id,
    sourceWakeId: completion.source_wake_id,
    sourceActionIndex: completion.source_action_index,
    correlationId: completion.correlation_id,
    parentConsumption: completion.parent_consumption,
    packet: {
      sessionId: completion.packet.session_id,
      status: completion.packet.status,
      summary: completion.packet.summary,
    },
  };
}

function toNativeDelegatedFanOutGroup(
  group: BodyState["fanOutGroups"][number],
): unknown {
  return {
    group_id: group.groupId,
    total: group.total,
    pending: group.pending,
    completed: group.completed,
    failed: group.failed,
    blocked: group.blocked,
    exhausted: group.exhausted,
    cancelled: group.cancelled,
    expired: group.expired,
    max_concurrency: group.maxConcurrency,
    failure_policy: group.failurePolicy,
    status: group.status,
  };
}

function toDelegatedFanOutGroup(
  group: RawDelegatedFanOutGroup,
): BodyState["fanOutGroups"][number] {
  return {
    groupId: group.group_id,
    total: group.total,
    pending: group.pending,
    completed: group.completed,
    failed: group.failed,
    blocked: group.blocked,
    exhausted: group.exhausted,
    cancelled: group.cancelled,
    expired: group.expired,
    maxConcurrency: group.max_concurrency,
    failurePolicy: group.failure_policy,
    status: group.status,
  };
}

function toNativeProviderStateInput(
  state: BrainWakeProviderStateInput,
): NativeBrainWakeProviderStateInput {
  return {
    module_id: state.moduleId,
    strategy_id: state.strategyId,
    profile_fingerprint: state.profileFingerprint,
    provider_fingerprint: state.providerFingerprint,
    payload_version: state.payloadVersion,
    payload: state.payload,
    expires_at: state.expiresAt,
  };
}

function toNativeDenDataUpdate(update: DenDataUpdate): unknown {
  return {
    project_id: update.projectId,
    entity_kind: update.entityKind,
    entity_id: update.entityId,
    revision: update.revision,
  };
}

function toNativeExternalEvent(event: ExternalEvent): unknown {
  return {
    adapter_id: event.adapterId,
    source: event.source,
    payload: toNativeExternalEventPayload(event.payload),
  };
}

function toNativeExternalEventPayload(
  payload: ExternalEvent["payload"],
): unknown {
  switch (payload.type) {
    case "human_message":
      return payload;
    case "channel_message":
      return {
        type: payload.type,
        binding_id: payload.bindingId,
        correlation_id: payload.correlationId,
        idempotency_key: payload.idempotencyKey,
        provider: payload.provider,
        external_channel_id: payload.externalChannelId,
        external_thread_id: payload.externalThreadId,
        external_message_id: payload.externalMessageId,
        from: payload.from,
        text: payload.text,
        received_at: payload.receivedAt,
        expires_at: payload.expiresAt,
      };
    case "adapter_status":
      return payload;
    case "tool_catalog_changed":
      return {
        type: payload.type,
        catalog_id: payload.catalogId,
      };
    case "raw_json":
      return payload;
  }
}

function toExternalEventPayload(payload: unknown): ExternalEvent["payload"] {
  const raw = payload as Record<string, unknown>;
  switch (raw["type"]) {
    case "channel_message":
      return {
        type: "channel_message",
        bindingId: raw["binding_id"] as string,
        correlationId: raw["correlation_id"] as string,
        idempotencyKey: raw["idempotency_key"] as string,
        provider: raw["provider"] as string,
        externalChannelId: raw["external_channel_id"] as string,
        externalThreadId: raw["external_thread_id"] as string | undefined,
        externalMessageId: raw["external_message_id"] as string | undefined,
        from: raw["from"] as string,
        text: raw["text"] as string,
        receivedAt: raw["received_at"] as string,
        expiresAt: raw["expires_at"] as string,
      };
    case "tool_catalog_changed":
      return {
        type: "tool_catalog_changed",
        catalogId: raw["catalog_id"] as string,
      };
    default:
      return payload as ExternalEvent["payload"];
  }
}

function encodeJson(value: unknown): Uint8Array {
  return new TextEncoder().encode(JSON.stringify(value));
}

function toNativeRuntimeConfigValidationInput(
  input: NativeRuntimeConfigValidationInput,
): unknown {
  return toCoreConfigWireRuntimeConfigValidationInput(input);
}

function toNativeCreateProfilePlanInput(
  input: NativeCreateProfilePlanInput,
): unknown {
  return toCoreConfigWireCreateProfilePlanInput(input);
}

function toNativeCreateProfilePlan(
  plan: RawCreateProfilePlan,
): NativeCreateProfilePlan {
  return {
    diagnostics: plan.diagnostics,
    registryWrite: plan.registry_write
      ? toNativeProfileRegistryWrite(plan.registry_write)
      : undefined,
    fileAssetActions: (plan.file_asset_actions ?? []).map((action) => ({
      kind: action.kind,
      profileId: action.profile_id,
      relativePath: action.relative_path,
      overwrite: action.overwrite,
      metadataJson: action.metadata_json,
    })),
    derivedRuntimeActions: (plan.derived_runtime_actions ?? []).map(
      (action) => ({
        kind: action.kind,
        refKind: action.ref_kind,
        refId: action.ref_id,
        applyPhase: action.apply_phase,
        metadataJson: action.metadata_json,
      }),
    ),
    profileSeed: plan.profile_seed
      ? {
          profileId: plan.profile_seed.profile_id,
          displayName: plan.profile_seed.display_name ?? undefined,
          providerAlias: plan.profile_seed.provider_alias,
          modelConfig: toProfileModelConfigSeed(plan.profile_seed.model_config),
          brain: {
            module: plan.profile_seed.brain.module ?? undefined,
            strategy: plan.profile_seed.brain.strategy ?? undefined,
          },
          skillsMode: plan.profile_seed.skills_mode,
        }
      : undefined,
    runtimeBrain: plan.runtime_brain
      ? {
          implementationId: plan.runtime_brain.implementation_id,
          profileId: plan.runtime_brain.profile_id,
        }
      : undefined,
    runtimeSession: plan.runtime_session
      ? {
          sessionId: plan.runtime_session.session_id,
          agentId: plan.runtime_session.agent_id,
          profileId: plan.runtime_session.profile_id,
          kind: plan.runtime_session.kind,
          resourceLimits: toResourceLimits(
            plan.runtime_session.resource_limits,
          ),
          ownerId: plan.runtime_session.owner_id ?? undefined,
          historyWindow: plan.runtime_session.history_window
            ? {
                maxMessages:
                  plan.runtime_session.history_window.max_messages ?? undefined,
              }
            : undefined,
          maxHistoryMessages:
            plan.runtime_session.max_history_messages ?? undefined,
          turnTimeoutMs: plan.runtime_session.turn_timeout_ms ?? undefined,
        }
      : undefined,
    profileMcpConfig: plan.profile_mcp_config
      ? {
          bindingId: plan.profile_mcp_config.binding_id ?? undefined,
          endpointRef: plan.profile_mcp_config.endpoint_ref ?? undefined,
          serverNames: plan.profile_mcp_config.server_names,
          transport: plan.profile_mcp_config.transport ?? undefined,
          toolProfile: plan.profile_mcp_config.tool_profile ?? undefined,
        }
      : undefined,
    runtimeMcpBindings: (plan.runtime_mcp_bindings ?? []).map(
      toMcpBindingDraft,
    ),
  };
}

function toNativeProfileRegistryWrite(
  write: RawProfileRegistryWrite,
): NativeProfileRegistryWrite {
  return {
    profileId: write.profile_id,
    lifecycleStatus: write.lifecycle_status,
    displayName: write.display_name ?? undefined,
    summary: write.summary ?? undefined,
    defaultSessionKind: write.default_session_kind ?? undefined,
    agentId: write.agent_id ?? undefined,
    ownerId: write.owner_id ?? undefined,
    promptSoulMarkdown: write.prompt_soul_markdown ?? undefined,
    promptMemoryMarkdown: write.prompt_memory_markdown ?? undefined,
    activeRuntimeSettingsJson: write.active_runtime_settings_json,
    sourceAssetRefs: write.source_asset_refs.map(
      toNativeProfileRegistryAssetRef,
    ),
    derivedRuntimeRefs: write.derived_runtime_refs.map(
      toNativeProfileRegistryRuntimeRef,
    ),
    importExport: toNativeProfileRegistryImportExport(write.import_export),
    now: write.now,
  };
}

function toNativeRuntimeConfigPlan(
  plan: RawRuntimeConfigPlan,
): NativeRuntimeConfigPlan {
  return {
    runtimeConfig: toRuntimeConfigDraft(plan.runtime_config),
    diagnostics: plan.diagnostics,
    derivedScheduledJobs: plan.derived_scheduled_jobs.map(toScheduledJobDraft),
    derivedMcpBindings: plan.derived_mcp_bindings.map(toMcpBindingDraft),
  };
}

function toRawProfileRegistryQuery(
  query: NativeProfileRegistryQuery,
): RawProfileRegistryQuery {
  return {
    lifecycle_status: query.lifecycleStatus,
    limit: query.limit,
    offset: query.offset,
  };
}

function toRawProfileRegistryWrite(
  write: NativeProfileRegistryWrite,
): RawProfileRegistryWrite {
  return {
    profile_id: write.profileId,
    lifecycle_status: write.lifecycleStatus,
    display_name: write.displayName,
    summary: write.summary,
    default_session_kind: write.defaultSessionKind,
    agent_id: write.agentId,
    owner_id: write.ownerId,
    prompt_soul_markdown: write.promptSoulMarkdown,
    prompt_memory_markdown: write.promptMemoryMarkdown,
    active_runtime_settings_json: write.activeRuntimeSettingsJson,
    source_asset_refs: write.sourceAssetRefs.map(toRawProfileRegistryAssetRef),
    derived_runtime_refs: write.derivedRuntimeRefs.map(
      toRawProfileRegistryRuntimeRef,
    ),
    import_export: toRawProfileRegistryImportExport(write.importExport),
    now: write.now,
  };
}

function toRawProfileRegistryUpdate(
  update: NativeProfileRegistryUpdate,
): RawProfileRegistryUpdate {
  return {
    write: toRawProfileRegistryWrite(update.write),
    expected_revision: update.expectedRevision,
  };
}

function toRawProfileRegistryMutationRequest(
  request: NativeProfileRegistryMutationRequest,
): RawProfileRegistryMutationRequest {
  return {
    profile_id: request.profileId,
    kind: request.kind,
    mode: request.mode,
    current: toRawProfileRegistryRecord(request.current),
    body_json: request.bodyJson,
    now: request.now,
  };
}

function toNativeProfileRegistryMutationPlan(
  plan: RawProfileRegistryMutationPlan,
): NativeProfileRegistryMutationPlan {
  return {
    ok: plan.ok,
    profileId: plan.profile_id,
    kind: plan.kind,
    mode: plan.mode,
    expectedRevision: plan.expected_revision,
    current: toNativeProfileRegistryRecord(plan.current),
    next: toNativeProfileRegistryRecord(plan.next),
    nextWrite: toNativeProfileRegistryWrite(plan.next_write),
    diagnostics: plan.diagnostics.map((diagnostic) => ({
      ...diagnostic,
      path: diagnostic.path ?? "",
    })),
    implications: {
      registryRevisionWillIncrement:
        plan.implications.registry_revision_will_increment,
      profileFilesUnchanged: plan.implications.profile_files_unchanged,
      serviceConfigUnchanged: plan.implications.service_config_unchanged,
      runtimeRebuildRecommended: plan.implications.runtime_rebuild_recommended,
      lifecycleEffects: plan.implications.lifecycle_effects,
    },
  };
}

function toNativeProfileRegistryRecord(
  record: RawProfileRegistryRecord,
): NativeProfileRegistryRecord {
  return {
    profileId: record.profile_id,
    lifecycleStatus: record.lifecycle_status,
    displayName: record.display_name ?? undefined,
    summary: record.summary ?? undefined,
    defaultSessionKind: record.default_session_kind ?? undefined,
    agentId: record.agent_id ?? undefined,
    ownerId: record.owner_id ?? undefined,
    promptSoulMarkdown: record.prompt_soul_markdown ?? undefined,
    promptMemoryMarkdown: record.prompt_memory_markdown ?? undefined,
    activeRuntimeSettingsJson: record.active_runtime_settings_json,
    sourceAssetRefs: record.source_asset_refs.map(
      toNativeProfileRegistryAssetRef,
    ),
    derivedRuntimeRefs: record.derived_runtime_refs.map(
      toNativeProfileRegistryRuntimeRef,
    ),
    importExport: toNativeProfileRegistryImportExport(record.import_export),
    revision: record.revision,
    createdAt: record.created_at,
    updatedAt: record.updated_at,
  };
}

function toRawProfileRegistryRecord(
  record: NativeProfileRegistryRecord,
): RawProfileRegistryRecord {
  return {
    profile_id: record.profileId,
    lifecycle_status: record.lifecycleStatus,
    display_name: record.displayName,
    summary: record.summary,
    default_session_kind: record.defaultSessionKind,
    agent_id: record.agentId,
    owner_id: record.ownerId,
    prompt_soul_markdown: record.promptSoulMarkdown,
    prompt_memory_markdown: record.promptMemoryMarkdown,
    active_runtime_settings_json: record.activeRuntimeSettingsJson,
    source_asset_refs: record.sourceAssetRefs.map(toRawProfileRegistryAssetRef),
    derived_runtime_refs: record.derivedRuntimeRefs.map(
      toRawProfileRegistryRuntimeRef,
    ),
    import_export: toRawProfileRegistryImportExport(record.importExport),
    revision: record.revision,
    created_at: record.createdAt,
    updated_at: record.updatedAt,
  };
}

function toNativeProfilePurgeReport(
  report: RawProfilePurgeReport,
): NativeProfilePurgeReport {
  return {
    profileId: report.profile_id,
    profileRegistryDeleted: report.profile_registry_deleted,
    sessionIds: report.session_ids,
    agentIds: report.agent_ids,
    tableCounts: report.table_counts.map((count) => ({
      table: count.table,
      rowsDeleted: count.rows_deleted,
    })),
    rowsDeleted: report.rows_deleted,
  };
}

function toRawModelProviderQuery(
  query: NativeModelProviderQuery,
): RawModelProviderQuery {
  return {
    status: query.status,
    alias_prefix: query.aliasPrefix,
    limit: query.limit,
    offset: query.offset,
  };
}

function toRawModelProviderWrite(
  write: NativeModelProviderWrite,
): RawModelProviderWrite {
  return {
    alias: write.alias,
    status: write.status,
    protocol: write.protocol,
    provider_kind: write.providerKind,
    display_name: write.displayName,
    description: write.description,
    base_url: write.baseUrl,
    model_id: write.modelId,
    context_window_tokens: write.contextWindowTokens,
    max_output_tokens: write.maxOutputTokens,
    temperature_milli: write.temperatureMilli,
    reasoning_effort: write.reasoningEffort,
    reasoning_format: write.reasoningFormat,
    secret: write.secret,
    clear_secret: write.clearSecret ?? false,
    metadata_json: write.metadataJson ?? {},
    expected_revision: write.expectedRevision,
    now: write.now,
  };
}

function toNativeModelProviderRefreshImpact(
  impact: RawModelProviderRefreshImpact,
): NativeModelProviderRefreshImpact {
  return {
    providerAlias: impact.provider_alias,
    affectedProfiles: impact.affected_profiles.map((profile) => ({
      profileId: profile.profile_id,
      sessionIds: profile.session_ids,
      configuredSessionIds: profile.configured_session_ids,
      activeSessionIds: profile.active_session_ids,
    })),
  };
}

function toRawModelProviderRefreshImpact(
  impact: NativeModelProviderRefreshImpact,
): RawModelProviderRefreshImpact {
  return {
    provider_alias: impact.providerAlias,
    affected_profiles: impact.affectedProfiles.map((profile) => ({
      profile_id: profile.profileId,
      session_ids: profile.sessionIds,
      configured_session_ids: profile.configuredSessionIds,
      active_session_ids: profile.activeSessionIds,
    })),
  };
}

function toNativeModelProviderRefreshPlan(
  plan: RawModelProviderRefreshPlan,
): NativeModelProviderRefreshPlan {
  return {
    providerAlias: plan.provider_alias,
    mode: plan.mode,
    affectedProfiles: plan.affected_profiles.map((profile) => ({
      profileId: profile.profile_id,
      sessionIds: profile.session_ids,
      configuredSessionIds: profile.configured_session_ids,
      activeSessionIds: profile.active_session_ids,
    })),
    actions: plan.actions.map((action) => ({
      profileId: action.profile_id,
      commandName: action.command_name,
      reason: action.reason,
      plannedSummary: action.planned_summary,
      appliedSummary: action.applied_summary,
      blockedSummary: action.blocked_summary,
      failureReasonCode: action.failure_reason_code,
    })),
  };
}

function toNativeModelProviderRecord(
  record: RawModelProviderRecord,
): NativeModelProviderRecord {
  return {
    alias: record.alias,
    status: record.status,
    protocol: record.protocol,
    providerKind: record.provider_kind,
    displayName: record.display_name ?? undefined,
    description: record.description ?? undefined,
    baseUrl: record.base_url ?? undefined,
    modelId: record.model_id,
    contextWindowTokens: record.context_window_tokens ?? undefined,
    maxOutputTokens: record.max_output_tokens ?? undefined,
    temperatureMilli: record.temperature_milli ?? undefined,
    reasoningEffort: record.reasoning_effort ?? undefined,
    reasoningFormat: record.reasoning_format ?? undefined,
    credential: {
      hasSecret: record.credential.has_secret,
      secretRef: record.credential.secret_ref ?? undefined,
      updatedAt: record.credential.updated_at ?? undefined,
      kind: record.credential.kind ?? undefined,
    },
    metadataJson: record.metadata_json,
    revision: record.revision,
    createdAt: record.created_at,
    updatedAt: record.updated_at,
  };
}

function toRawModelProviderRecord(
  record: NativeModelProviderRecord,
): RawModelProviderRecord {
  return {
    alias: record.alias,
    status: record.status,
    protocol: record.protocol,
    provider_kind: record.providerKind,
    display_name: record.displayName,
    description: record.description,
    base_url: record.baseUrl,
    model_id: record.modelId,
    context_window_tokens: record.contextWindowTokens,
    max_output_tokens: record.maxOutputTokens,
    temperature_milli: record.temperatureMilli,
    reasoning_effort: record.reasoningEffort,
    reasoning_format: record.reasoningFormat,
    credential: {
      has_secret: record.credential.hasSecret,
      secret_ref: record.credential.secretRef,
      updated_at: record.credential.updatedAt,
      kind: record.credential.kind,
    },
    metadata_json: record.metadataJson,
    revision: record.revision,
    created_at: record.createdAt,
    updated_at: record.updatedAt,
  };
}

function toNativeProfileRegistryAssetRef(
  ref: RawProfileRegistrySourceAssetRef,
): NativeProfileRegistrySourceAssetRef {
  return {
    assetKind: ref.asset_kind,
    path: ref.path,
    contentHash: ref.content_hash ?? undefined,
    lastSeenAt: ref.last_seen_at ?? undefined,
    metadataJson: ref.metadata_json,
  };
}

function toRawProfileRegistryAssetRef(
  ref: NativeProfileRegistrySourceAssetRef,
): RawProfileRegistrySourceAssetRef {
  return {
    asset_kind: ref.assetKind,
    path: ref.path,
    content_hash: ref.contentHash,
    last_seen_at: ref.lastSeenAt,
    metadata_json: ref.metadataJson,
  };
}

function toNativeProfileRegistryRuntimeRef(
  ref: RawProfileRegistryDerivedRuntimeRef,
): NativeProfileRegistryDerivedRuntimeRef {
  return {
    refKind: ref.ref_kind,
    refId: ref.ref_id,
    status: ref.status,
    updatedAt: ref.updated_at ?? undefined,
    metadataJson: ref.metadata_json,
  };
}

function toRawProfileRegistryRuntimeRef(
  ref: NativeProfileRegistryDerivedRuntimeRef,
): RawProfileRegistryDerivedRuntimeRef {
  return {
    ref_kind: ref.refKind,
    ref_id: ref.refId,
    status: ref.status,
    updated_at: ref.updatedAt,
    metadata_json: ref.metadataJson,
  };
}

function toNativeProfileRegistryImportExport(
  metadata: RawProfileRegistryImportExportMetadata,
): NativeProfileRegistryImportExportMetadata {
  return {
    importedFrom: metadata.imported_from ?? undefined,
    importedAt: metadata.imported_at ?? undefined,
    exportedTo: metadata.exported_to ?? undefined,
    exportedAt: metadata.exported_at ?? undefined,
    metadataJson: metadata.metadata_json,
  };
}

function toRawProfileRegistryImportExport(
  metadata: NativeProfileRegistryImportExportMetadata,
): RawProfileRegistryImportExportMetadata {
  return {
    imported_from: metadata.importedFrom,
    imported_at: metadata.importedAt,
    exported_to: metadata.exportedTo,
    exported_at: metadata.exportedAt,
    metadata_json: metadata.metadataJson,
  };
}

function toRuntimeConfigDraft(
  draft: RawRuntimeConfigDraft,
): NativeRuntimeConfigDraft {
  return {
    profilesDir: draft.profiles_dir,
    skillsDir: draft.skills_dir ?? undefined,
    brains: draft.brains.map((brain) => ({
      implementationId: brain.implementation_id,
      profileId: brain.profile_id,
    })),
    sessions: draft.sessions.map((session) => ({
      sessionId: session.session_id,
      agentId: session.agent_id,
      profileId: session.profile_id,
      kind: session.kind,
      resourceLimits: toResourceLimits(session.resource_limits),
      ownerId: session.owner_id ?? undefined,
      historyWindow: session.history_window
        ? {
            maxMessages: session.history_window.max_messages ?? undefined,
          }
        : undefined,
      maxHistoryMessages: session.max_history_messages ?? undefined,
      turnTimeoutMs: session.turn_timeout_ms ?? undefined,
    })),
    scheduledJobs: draft.scheduled_jobs.map(toScheduledJobDraft),
    channelBindings: draft.channel_bindings.map((binding) => ({
      bindingId: binding.binding_id,
      adapterId: binding.adapter_id,
      provider: binding.provider,
      agentId: binding.agent_id,
      instanceId: binding.instance_id ?? undefined,
      sessionId: binding.session_id,
      profileId: binding.profile_id,
      externalChannelId: binding.external_channel_id,
      externalThreadId: binding.external_thread_id ?? undefined,
      externalUserId: binding.external_user_id ?? undefined,
      conversationProjectId: binding.conversation_project_id ?? undefined,
      conversationChannelId: binding.conversation_channel_id ?? undefined,
      providerSubscriptionId: binding.provider_subscription_id ?? undefined,
      status: binding.status,
    })),
    mcpBindings: draft.mcp_bindings.map(toMcpBindingDraft),
  };
}

function toScheduledJobDraft(
  job: RawScheduledJobConfigDraft,
): NativeScheduledJobConfigDraft {
  return {
    id: job.id,
    schedule: job.schedule,
    shape: job.shape,
    jobKind: job.job_kind ?? undefined,
    targetSessionId: job.target_session_id ?? undefined,
    script: job.script ?? undefined,
    deliveryChannelId: job.delivery_channel_id ?? undefined,
  };
}

function toMcpBindingDraft(
  binding: RawMcpBindingConfigDraft,
): NativeMcpBindingConfigDraft {
  return {
    bindingId: binding.binding_id,
    adapterId: binding.adapter_id,
    agentId: binding.agent_id,
    instanceId: binding.instance_id ?? undefined,
    sessionId: binding.session_id ?? undefined,
    profileId: binding.profile_id,
    serverNames: binding.server_names,
    endpointRef: binding.endpoint_ref,
    transport: binding.transport,
    toolProfileKey: binding.tool_profile_key,
    status: binding.status,
  };
}

function toProfileModelConfigSeed(
  modelConfig: RawProfileModelConfigSeed,
): NativeProfileModelConfigSeed {
  return {
    provider: modelConfig.provider,
    modelName: modelConfig.model_name,
    baseUrl: modelConfig.base_url,
    api: modelConfig.api,
    apiKeyEnv: modelConfig.api_key_env,
    temperatureMilli: modelConfig.temperature_milli,
    maxOutputTokens: modelConfig.max_output_tokens,
  };
}

function toResourceLimits(
  limits: RawResourceLimits | undefined,
): ResourceLimits | undefined {
  if (!limits) {
    return undefined;
  }
  return {
    workdir: limits.workdir ?? undefined,
    maxDurationMs: limits.max_duration_ms ?? undefined,
    maxDelegationDepth: limits.max_delegation_depth ?? undefined,
  };
}

function toCoreEvent(event: RawCoreEvent): CoreEvent {
  switch (event.type) {
    case "session_created":
      return { type: event.type, state: toSessionState(event.state) };
    case "session_archived":
      return { type: event.type, sessionId: event.session_id };
    case "agent_message_routed":
      return { type: event.type, message: toAgentMessage(event.message) };
    case "delegation_lifecycle_observed":
      return {
        type: event.type,
        lifecycle: toDelegationLifecycleEvent(event.lifecycle),
      };
    case "external_event_injected":
      return {
        type: event.type,
        event: {
          adapterId: event.event.adapter_id,
          source: event.event.source,
          payload: toExternalEventPayload(event.event.payload),
        },
      };
    case "den_data_updated":
      return {
        type: event.type,
        update: {
          projectId: event.update.project_id,
          entityKind: event.update.entity_kind,
          entityId: event.update.entity_id,
          revision: event.update.revision,
        },
      };
    case "brain_wake_requested":
      return { type: event.type, sessionId: event.session_id };
    case "brain_event_observed":
      return {
        type: event.type,
        sessionId: event.session_id,
        wakeId: event.wake_id,
        event: toBrainEvent(event.event),
      };
    case "brain_actions_accepted":
      return {
        type: event.type,
        sessionId: event.session_id,
        count: event.count,
      };
    case "completion_packet_delivered":
      return {
        type: event.type,
        packet: {
          sessionId: event.packet.session_id,
          status: event.packet.status,
          summary: event.packet.summary,
        },
      };
  }
}

function toDelegationLifecycleEvent(
  lifecycle: RawDelegationLifecycleEvent,
): Extract<CoreEvent, { type: "delegation_lifecycle_observed" }>["lifecycle"] {
  return {
    parentSessionId: lifecycle.parent_session_id,
    delegatedSessionId: lifecycle.delegated_session_id,
    runId: lifecycle.run_id,
    phase: lifecycle.phase,
    detail: lifecycle.detail,
  };
}

function toDelegatedSessionRuntimeStatus(
  status: RawDelegatedSessionRuntimeStatus,
): DelegatedSessionRuntimeStatus {
  return {
    session: toSessionState(status.session),
    parentSessionId: status.parent_session_id,
    runId: status.run_id,
    runStatus: status.run_status,
    terminal: status.terminal,
  };
}

function toDelegatedResourceCleanupReport(
  report: RawDelegatedResourceCleanupReport,
): DelegatedResourceCleanupReport {
  return {
    cleanedAt: report.cleaned_at,
    terminalArchived: report.terminal_archived,
    orphanedArchived: report.orphaned_archived,
    expiredArchived: report.expired_archived,
    resourcesReleased: report.resources_released,
  };
}

function toScheduledJobSummary(
  raw: RawScheduledJobSummary,
): ScheduledJobSummary {
  return {
    jobId: raw.job_id,
    jobKind: raw.job_kind,
    targetSessionId: raw.target_session_id,
    intervalMs: raw.interval_ms,
    nextDueAt: raw.next_due_at,
    status: raw.status,
    createdAt: raw.created_at,
    updatedAt: raw.updated_at,
    pausedAt: raw.paused_at,
  };
}

function toScheduledRunSummary(
  raw: RawScheduledRunSummary,
): ScheduledRunSummary {
  return {
    runId: raw.run_id,
    jobId: raw.job_id,
    jobKind: raw.job_kind,
    targetSessionId: raw.target_session_id,
    status: raw.status,
    trigger: raw.trigger,
    scheduledFor: raw.scheduled_for,
    claimedAt: raw.claimed_at,
    claimDeadlineAt: raw.claim_deadline_at,
    completedAt: raw.completed_at,
    error: raw.error,
    output: raw.output,
    createdAt: raw.created_at,
    updatedAt: raw.updated_at,
  };
}

function toSchedulerTickReport(
  raw: RawSchedulerTickReport,
): SchedulerTickReport {
  return {
    staleRunsExpired: raw.stale_runs_expired,
    dueRunsClaimed: raw.due_runs_claimed,
    wakesRequested: raw.wakes_requested,
    runsCompleted: raw.runs_completed,
    runsSkipped: raw.runs_skipped,
    runsFailed: raw.runs_failed,
  };
}

function toSessionState(state: RawSessionState): SessionState {
  return {
    handle: state.handle as SessionState["handle"],
    sessionId: state.session_id,
    agentId: state.agent_id,
    profileId: state.profile_id,
    kind: state.kind,
    delegation: state.delegation
      ? {
          parentSessionId: state.delegation.parent_session_id,
          parentAgentId: state.delegation.parent_agent_id,
          sourceWakeId: state.delegation.source_wake_id,
          sourceActionIndex: state.delegation.source_action_index,
          requestedTaskId: state.delegation.requested_task_id,
          correlationId: state.delegation.correlation_id,
        }
      : undefined,
    resourceLimits: {
      workdir: state.resource_limits?.workdir,
      maxDurationMs: state.resource_limits?.max_duration_ms,
      maxDelegationDepth: state.resource_limits?.max_delegation_depth,
    },
    toolProfile: {
      tools:
        state.tool_profile?.tools.map((tool) => ({
          name: tool.name,
          description: tool.description,
          inputSchema:
            typeof tool.input_schema === "number"
              ? (tool.input_schema as SessionState["toolProfile"]["tools"][number]["inputSchema"])
              : undefined,
        })) ?? [],
    },
    historyWindow: state.history_window
      ? {
          maxMessages: state.history_window.max_messages,
        }
      : undefined,
    status: state.status,
    brainTurnCount: state.brain_turn_count,
    createdAt: state.created_at,
    lastActiveAt: state.last_active_at,
  };
}

function toAgentMessage(message: RawAgentMessage): AgentMessage {
  return {
    from: message.from,
    to: message.to,
    body: message.body,
    correlationId: message.correlation_id,
    projection: message.projection
      ? {
          visibility: message.projection.visibility,
          targetRef: message.projection.target_ref,
          workRef: message.projection.work_ref,
          reason: message.projection.reason,
        }
      : undefined,
  };
}

function toBrainEvent(event: RawBrainEvent): BrainEvent {
  switch (event.type) {
    case "started":
    case "finished":
      return event;
    case "text_delta":
      return { type: event.type, text: event.text };
    case "reasoning_delta":
      return {
        type: event.type,
        text: event.text,
        format: event.format,
      };
    case "phase_change":
      return {
        type: event.type,
        phase: event.phase,
        message: event.message,
      };
    case "tool_call_started":
      return {
        type: event.type,
        toolName: event.tool_name,
        metadata: event.metadata
          ? toToolCallMetadata(event.metadata)
          : undefined,
      };
    case "tool_call_finished":
      return {
        type: event.type,
        toolName: event.tool_name,
        isError: event.is_error,
        metadata: event.metadata
          ? toToolCallMetadata(event.metadata)
          : undefined,
      };
    case "provider_status":
      return {
        type: event.type,
        level: event.level,
        message: event.message,
        metadataJson: event.metadata_json,
      };
  }
}

function toBrainWakeStreamItem(
  item: RawBrainWakeStreamItem,
): BrainWakeStreamItem {
  switch (item.type) {
    case "event":
      return {
        type: "event",
        event: {
          wakeId: item.event.wake_id,
          sessionId: item.event.session_id,
          event: toBrainEvent(item.event.event),
        },
      };
    case "actions":
      return {
        type: "actions",
        batch: {
          wakeId: item.batch.wake_id,
          sessionId: item.batch.session_id,
          actions: item.batch.actions.map(toBrainAction),
        },
      };
    case "wake_failed":
      return {
        type: "wake_failed",
        failure: {
          wakeId: item.failure.wake_id,
          sessionId: item.failure.session_id,
          kind: item.failure.kind as BrainWakeFailure["kind"],
          message: item.failure.message,
        },
      };
  }
}

function toOpenAiResponsesBrainRunResult(
  raw: RawOpenAiResponsesBrainRunResult,
): BrainWakeExecutionResult {
  return {
    stream: raw.stream.map(toBrainWakeStreamItem),
    events: [],
    actions: [],
    providerState: raw.provider_state
      ? toBrainWakeProviderStateOutput(raw.provider_state)
      : undefined,
    transportMetrics: raw.transport_metrics,
    credentialSecretUpdate: raw.credential_secret_update
      ? {
          providerAlias: raw.credential_secret_update.provider_alias,
          secret: raw.credential_secret_update.secret,
        }
      : undefined,
  };
}

function toRawOpenAiResponsesBrainRunResult(
  result: BrainWakeExecutionResult & {
    transportMetrics?: OpenAiResponsesTransportMetrics;
  },
): RawOpenAiResponsesBrainRunResult {
  return {
    stream: (
      result.stream ?? [
        ...result.events.map(
          (event): BrainWakeStreamItem => ({
            type: "event",
            event,
          }),
        ),
        ...(result.actions.length > 0
          ? [
              {
                type: "actions" as const,
                batch: {
                  wakeId: result.events[0]?.wakeId ?? "unknown-wake",
                  sessionId: result.events[0]?.sessionId ?? "unknown-session",
                  actions: result.actions,
                },
              },
            ]
          : []),
      ]
    ).map(toRawBrainWakeStreamItem),
    provider_state: result.providerState
      ? toRawBrainWakeProviderStateOutput(result.providerState)
      : undefined,
    transport_metrics: result.transportMetrics,
    credential_secret_update: result.credentialSecretUpdate
      ? {
          provider_alias: result.credentialSecretUpdate.providerAlias,
          secret: result.credentialSecretUpdate.secret,
        }
      : undefined,
  };
}

function toRawBrainWakeStreamItem(
  item: BrainWakeStreamItem,
): RawBrainWakeStreamItem {
  switch (item.type) {
    case "event":
      return {
        type: "event",
        event: {
          wake_id: item.event.wakeId,
          session_id: item.event.sessionId,
          event: toNativeBrainEventForJson(item.event.event) as RawBrainEvent,
        },
      };
    case "actions":
      return {
        type: "actions",
        batch: {
          wake_id: item.batch.wakeId,
          session_id: item.batch.sessionId,
          actions: item.batch.actions.map(
            (action) => toNativeBrainAction(action) as RawBrainAction,
          ),
        },
      };
    case "wake_failed":
      return {
        type: "wake_failed",
        failure: {
          wake_id: item.failure.wakeId,
          session_id: item.failure.sessionId,
          kind: item.failure.kind,
          message: item.failure.message,
        },
      };
  }
}

function toBrainAction(action: RawBrainAction): BrainAction {
  switch (action.type) {
    case "send_message":
      return {
        type: action.type,
        message: toAgentMessage(action.message),
      };
    case "request_delegation":
      return {
        type: action.type,
        profileId: action.profile_id,
        taskId: action.task_id,
        prompt: action.prompt,
        expectedOutput: action.expected_output,
        resourceLimits: action.resource_limits
          ? {
              workdir: action.resource_limits.workdir,
              maxDurationMs: action.resource_limits.max_duration_ms,
              maxDelegationDepth: action.resource_limits.max_delegation_depth,
            }
          : undefined,
        timeoutMs: action.timeout_ms,
        priority: action.priority,
        fanOutGroupId: action.fan_out_group_id,
        fanOutMaxConcurrency: action.fan_out_max_concurrency,
        fanOutFailurePolicy: action.fan_out_failure_policy,
        correlationId: action.correlation_id,
        parentConsumption: action.parent_consumption,
        capacityRequest: action.capacity_request
          ? {
              memberId: action.capacity_request.member_id,
              claimTtlMs: action.capacity_request.claim_ttl_ms,
              fallbackPolicy: action.capacity_request.fallback_policy,
            }
          : undefined,
      };
    case "deliver_completion":
      return {
        type: action.type,
        packet: {
          sessionId: action.packet.session_id,
          status: action.packet.status,
          summary: action.packet.summary,
        },
      };
  }
}

function toRawBrainWakeProviderStateOutput(
  output: BrainWakeProviderStateOutput,
): RawBrainWakeProviderStateOutput {
  switch (output.type) {
    case "unchanged":
      return { type: "unchanged" };
    case "replace":
      return {
        type: "replace",
        state: {
          module_id: output.state.moduleId,
          strategy_id: output.state.strategyId,
          profile_fingerprint: output.state.profileFingerprint,
          provider_fingerprint: output.state.providerFingerprint,
          payload_version: output.state.payloadVersion,
          payload: output.state.payload,
          ttl_ms: output.state.ttlMs,
        },
      };
    case "clear":
      return { type: "clear", reason: output.reason };
  }
}

function toBrainWakeProviderStateOutput(
  output: RawBrainWakeProviderStateOutput,
): BrainWakeProviderStateOutput {
  switch (output.type) {
    case "unchanged":
      return { type: "unchanged" };
    case "replace":
      return {
        type: "replace",
        state: {
          moduleId: output.state.module_id,
          strategyId: output.state.strategy_id,
          profileFingerprint: output.state.profile_fingerprint,
          providerFingerprint: output.state.provider_fingerprint,
          payloadVersion: output.state.payload_version,
          payload: output.state.payload,
          ttlMs: output.state.ttl_ms,
        },
      };
    case "clear":
      return { type: "clear", reason: output.reason };
  }
}

function toNativeBrainEvent(event: BrainEvent): {
  eventType: string;
  text?: string;
  toolName?: string;
  isError?: boolean;
  metadataJson?: string;
} {
  switch (event.type) {
    case "started":
      return { eventType: event.type };
    case "text_delta":
      return { eventType: event.type, text: event.text };
    case "reasoning_delta":
      return {
        eventType: event.type,
        text: event.text,
        toolName: event.format,
      };
    case "phase_change":
      return {
        eventType: event.type,
        text: event.message,
        toolName: event.phase,
      };
    case "tool_call_started":
      return {
        eventType: event.type,
        toolName: event.toolName,
        metadataJson: event.metadata
          ? JSON.stringify(toRawToolCallMetadata(event.metadata))
          : undefined,
      };
    case "tool_call_finished":
      return {
        eventType: event.type,
        toolName: event.toolName,
        isError: event.isError,
        metadataJson: event.metadata
          ? JSON.stringify(toRawToolCallMetadata(event.metadata))
          : undefined,
      };
    case "provider_status":
      return {
        eventType: event.type,
        text: event.message,
        toolName: event.level,
        metadataJson: event.metadataJson,
      };
    case "finished":
      return { eventType: event.type };
  }
}

function toToolCallMetadata(metadata: RawToolCallMetadata): ToolCallMetadata {
  return {
    source: metadata.source,
    adapterId: metadata.adapter_id as ToolCallMetadata["adapterId"],
    bindingId: metadata.binding_id,
    serverNames: metadata.server_names,
    profileId: metadata.profile_id as ToolCallMetadata["profileId"],
    toolProfileKey: metadata.tool_profile_key,
    sourceToolName: metadata.source_tool_name,
    catalogRevision: metadata.catalog_revision,
    debugDetailId: metadata.debug_detail_id,
    policy: metadata.policy
      ? {
          allowed: metadata.policy.allowed,
          denialReason: metadata.policy.denial_reason,
          timeoutMs: metadata.policy.timeout_ms,
          cancelled: metadata.policy.cancelled,
          archiveCleanup: metadata.policy.archive_cleanup,
        }
      : undefined,
  };
}

function toRawToolCallMetadata(
  metadata: ToolCallMetadata,
): RawToolCallMetadata {
  return {
    source: metadata.source,
    adapter_id: metadata.adapterId,
    binding_id: metadata.bindingId,
    server_names: metadata.serverNames ?? [],
    profile_id: metadata.profileId,
    tool_profile_key: metadata.toolProfileKey,
    source_tool_name: metadata.sourceToolName,
    catalog_revision: metadata.catalogRevision,
    debug_detail_id: metadata.debugDetailId,
    policy: metadata.policy
      ? {
          allowed: metadata.policy.allowed,
          denial_reason: metadata.policy.denialReason,
          timeout_ms: metadata.policy.timeoutMs,
          cancelled: metadata.policy.cancelled,
          archive_cleanup: metadata.policy.archiveCleanup,
        }
      : undefined,
  };
}

type RawCoreEvent =
  | { type: "session_created"; state: RawSessionState }
  | { type: "session_archived"; session_id: SessionId }
  | { type: "agent_message_routed"; message: RawAgentMessage }
  | {
      type: "delegation_lifecycle_observed";
      lifecycle: RawDelegationLifecycleEvent;
    }
  | {
      type: "external_event_injected";
      event: {
        adapter_id: AdapterId;
        source: string;
        payload: unknown;
      };
    }
  | {
      type: "den_data_updated";
      update: {
        project_id: ProjectId;
        entity_kind: string;
        entity_id: string;
        revision?: string;
      };
    }
  | { type: "brain_wake_requested"; session_id: SessionId }
  | {
      type: "brain_event_observed";
      session_id: SessionId;
      wake_id?: string;
      event: RawBrainEvent;
    }
  | {
      type: "brain_actions_accepted";
      session_id: SessionId;
      count: number;
    }
  | {
      type: "completion_packet_delivered";
      packet: {
        session_id: SessionId;
        status: Extract<
          CoreEvent,
          { type: "completion_packet_delivered" }
        >["packet"]["status"];
        summary: string;
      };
    };

interface RawDelegationLifecycleEvent {
  parent_session_id: SessionId;
  delegated_session_id: SessionId;
  run_id?: RunId;
  phase: Extract<
    CoreEvent,
    { type: "delegation_lifecycle_observed" }
  >["lifecycle"]["phase"];
  detail?: string;
}

interface RawDelegatedSessionRuntimeStatus {
  session: RawSessionState;
  parent_session_id?: SessionId;
  run_id?: RunId;
  run_status?: DelegatedSessionRuntimeStatus["runStatus"];
  terminal: boolean;
}

interface RawDelegatedResourceCleanupReport {
  cleaned_at: string;
  terminal_archived: SessionId[];
  orphaned_archived: SessionId[];
  expired_archived: SessionId[];
  resources_released: number;
}

interface RawCreateProfilePlan {
  diagnostics: NativeRuntimeConfigDiagnostic[];
  registry_write?: RawProfileRegistryWrite;
  file_asset_actions: RawCreateProfileFileAssetAction[];
  derived_runtime_actions: RawCreateProfileDerivedRuntimeAction[];
  profile_seed?: {
    profile_id: string;
    display_name?: string;
    provider_alias: string;
    model_config: RawProfileModelConfigSeed;
    brain: {
      module?: string;
      strategy?: string;
    };
    skills_mode: string;
  };
  runtime_brain?: {
    implementation_id: string;
    profile_id: string;
  };
  runtime_session?: {
    session_id: string;
    agent_id: string;
    profile_id: string;
    kind: "full" | "worker" | "delegated";
    resource_limits?: RawResourceLimits;
    owner_id?: string;
    history_window?: {
      max_messages?: number;
    };
    max_history_messages?: number;
    turn_timeout_ms?: number;
  };
  profile_mcp_config?: {
    binding_id?: string;
    endpoint_ref?: string;
    server_names: string[];
    transport?: string;
    tool_profile?: string;
  };
  runtime_mcp_bindings?: RawMcpBindingConfigDraft[];
}

interface RawProfileRegistryWrite {
  profile_id: string;
  lifecycle_status: NativeProfileRegistryLifecycleStatus;
  display_name?: string;
  summary?: string;
  default_session_kind?: "full" | "worker" | "delegated";
  agent_id?: string;
  owner_id?: string;
  prompt_soul_markdown?: string;
  prompt_memory_markdown?: string;
  active_runtime_settings_json: unknown;
  source_asset_refs: RawProfileRegistrySourceAssetRef[];
  derived_runtime_refs: RawProfileRegistryDerivedRuntimeRef[];
  import_export: RawProfileRegistryImportExportMetadata;
  now: string;
}

interface RawProfileRegistryUpdate {
  write: RawProfileRegistryWrite;
  expected_revision: number;
}

interface RawProfileRegistryMutationRequest {
  profile_id: string;
  kind: "update" | "lifecycle" | "prompt";
  mode: "plan" | "apply";
  current: RawProfileRegistryRecord;
  body_json: unknown;
  now: string;
}

interface RawProfileRegistryMutationPlan {
  ok: boolean;
  profile_id: string;
  kind: "update" | "lifecycle" | "prompt";
  mode: "plan" | "apply";
  expected_revision: number;
  current: RawProfileRegistryRecord;
  next: RawProfileRegistryRecord;
  next_write: RawProfileRegistryWrite;
  diagnostics: NativeRuntimeConfigDiagnostic[];
  implications: {
    registry_revision_will_increment: boolean;
    profile_files_unchanged: boolean;
    service_config_unchanged: boolean;
    runtime_rebuild_recommended: boolean;
    lifecycle_effects: "none" | "archive_active_sessions_and_unregister_brain";
  };
}

interface RawCreateProfileFileAssetAction {
  kind: "write_profile_json";
  profile_id: string;
  relative_path: string;
  overwrite: boolean;
  metadata_json: unknown;
}

interface RawCreateProfileDerivedRuntimeAction {
  kind:
    | "add_brain"
    | "add_session"
    | "add_profile_mcp_config"
    | "add_mcp_binding";
  ref_kind: string;
  ref_id: string;
  apply_phase: string;
  metadata_json: unknown;
}

interface RawRuntimeConfigPlan {
  runtime_config: RawRuntimeConfigDraft;
  diagnostics: NativeRuntimeConfigDiagnostic[];
  derived_scheduled_jobs: RawScheduledJobConfigDraft[];
  derived_mcp_bindings: RawMcpBindingConfigDraft[];
}

interface RawProfileRegistryQuery {
  lifecycle_status?: NativeProfileRegistryLifecycleStatus;
  limit?: number;
  offset?: number;
}

interface RawProfileRegistrySourceAssetRef {
  asset_kind: string;
  path: string;
  content_hash?: string | null;
  last_seen_at?: string | null;
  metadata_json: unknown;
}

interface RawProfileRegistryDerivedRuntimeRef {
  ref_kind: string;
  ref_id: string;
  status: string;
  updated_at?: string | null;
  metadata_json: unknown;
}

interface RawProfileRegistryImportExportMetadata {
  imported_from?: string | null;
  imported_at?: string | null;
  exported_to?: string | null;
  exported_at?: string | null;
  metadata_json: unknown;
}

interface RawProfileRegistryRecord {
  profile_id: string;
  lifecycle_status: NativeProfileRegistryLifecycleStatus;
  display_name?: string | null;
  summary?: string | null;
  default_session_kind?: "full" | "worker" | "delegated" | null;
  agent_id?: string | null;
  owner_id?: string | null;
  prompt_soul_markdown?: string | null;
  prompt_memory_markdown?: string | null;
  active_runtime_settings_json: unknown;
  source_asset_refs: RawProfileRegistrySourceAssetRef[];
  derived_runtime_refs: RawProfileRegistryDerivedRuntimeRef[];
  import_export: RawProfileRegistryImportExportMetadata;
  revision: number;
  created_at: string;
  updated_at: string;
}

interface RawProfilePurgeReport {
  profile_id: string;
  profile_registry_deleted: boolean;
  session_ids: string[];
  agent_ids: string[];
  table_counts: Array<{
    table: string;
    rows_deleted: number;
  }>;
  rows_deleted: number;
}

interface RawModelProviderCredential {
  has_secret: boolean;
  secret_ref?: string | null;
  updated_at?: string | null;
  kind?: NativeModelProviderCredentialKind | null;
}

interface RawModelProviderRecord {
  alias: string;
  status: NativeModelProviderStatus;
  protocol: NativeModelProviderProtocol;
  provider_kind: string;
  display_name?: string | null;
  description?: string | null;
  base_url?: string | null;
  model_id: string;
  context_window_tokens?: number | null;
  max_output_tokens?: number | null;
  temperature_milli?: number | null;
  reasoning_effort?: string | null;
  reasoning_format?: string | null;
  credential: RawModelProviderCredential;
  metadata_json: unknown;
  revision: number;
  created_at: string;
  updated_at: string;
}

interface RawModelProviderWrite {
  alias: string;
  status: NativeModelProviderStatus;
  protocol: NativeModelProviderProtocol;
  provider_kind: string;
  display_name?: string;
  description?: string;
  base_url?: string;
  model_id: string;
  context_window_tokens?: number;
  max_output_tokens?: number;
  temperature_milli?: number;
  reasoning_effort?: string;
  reasoning_format?: string;
  secret?: string;
  clear_secret: boolean;
  metadata_json: unknown;
  expected_revision?: number;
  now: string;
}

interface RawModelProviderQuery {
  status?: NativeModelProviderStatus;
  alias_prefix?: string;
  limit?: number;
  offset?: number;
}

interface RawModelProviderAffectedProfile {
  profile_id: string;
  session_ids: string[];
  configured_session_ids: string[];
  active_session_ids: string[];
}

interface RawModelProviderRefreshImpact {
  provider_alias: string;
  affected_profiles: RawModelProviderAffectedProfile[];
}

interface RawModelProviderRefreshProfileAction {
  profile_id: string;
  command_name: string;
  reason: string;
  planned_summary: string;
  applied_summary: string;
  blocked_summary: string;
  failure_reason_code: string;
}

interface RawModelProviderRefreshPlan {
  provider_alias: string;
  mode: NativeModelProviderRefreshMode;
  affected_profiles: RawModelProviderAffectedProfile[];
  actions: RawModelProviderRefreshProfileAction[];
}

interface RawRuntimeConfigDraft {
  profiles_dir: string;
  skills_dir?: string;
  brains: Array<{
    implementation_id: string;
    profile_id: string;
  }>;
  sessions: RawSessionConfigDraft[];
  scheduled_jobs: RawScheduledJobConfigDraft[];
  channel_bindings: RawChannelBindingConfigDraft[];
  mcp_bindings: RawMcpBindingConfigDraft[];
}

interface RawSessionConfigDraft {
  session_id: string;
  agent_id: string;
  profile_id: string;
  kind: "full" | "worker" | "delegated";
  resource_limits?: RawResourceLimits;
  owner_id?: string;
  history_window?: {
    max_messages?: number;
  };
  max_history_messages?: number;
  turn_timeout_ms?: number;
}

interface RawScheduledJobConfigDraft {
  id: string;
  schedule: string;
  shape: "host_job" | "session_wake" | "script_only" | "data_collection";
  job_kind?: string;
  target_session_id?: string;
  script?: string;
  delivery_channel_id?: string;
}

interface RawChannelBindingConfigDraft {
  binding_id: string;
  adapter_id: string;
  provider: string;
  agent_id: string;
  instance_id?: string;
  session_id?: string;
  profile_id: string;
  external_channel_id: string;
  external_thread_id?: string;
  external_user_id?: string;
  conversation_project_id?: string;
  conversation_channel_id?: number;
  provider_subscription_id?: string;
  status: NativeExternalBindingStatus;
}

interface RawMcpBindingConfigDraft {
  binding_id: string;
  adapter_id: string;
  agent_id: string;
  instance_id?: string;
  session_id?: string;
  profile_id: string;
  server_names: string[];
  endpoint_ref: string;
  transport: string;
  tool_profile_key: string;
  status: NativeExternalBindingStatus;
}

interface RawProfileModelConfigSeed {
  provider: string;
  model_name: string;
  base_url?: string;
  api?: string;
  api_key_env?: string;
  temperature_milli?: number;
  max_output_tokens?: number;
}

interface RawResourceLimits {
  workdir?: string;
  max_duration_ms?: number;
  max_delegation_depth?: number;
}

interface RawScheduledJobSummary {
  job_id: string;
  job_kind: string;
  target_session_id?: SessionId;
  interval_ms?: number;
  next_due_at?: string;
  status: ScheduledJobStatus;
  created_at: string;
  updated_at: string;
  paused_at?: string;
}

interface RawScheduledRunSummary {
  run_id: RunId;
  job_id: string;
  job_kind: string;
  target_session_id?: SessionId;
  status: ScheduledRunStatus;
  trigger: ScheduledRunTrigger;
  scheduled_for?: string;
  claimed_at: string;
  claim_deadline_at: string;
  completed_at?: string;
  error?: string;
  output?: unknown;
  created_at: string;
  updated_at: string;
}

interface RawSchedulerTickReport {
  stale_runs_expired: number;
  due_runs_claimed: number;
  wakes_requested: number;
  runs_completed: number;
  runs_skipped: number;
  runs_failed: number;
}

interface RawSessionState {
  handle: number;
  session_id: SessionId;
  agent_id: AgentId;
  profile_id: ProfileId;
  kind: SessionState["kind"];
  delegation?: {
    parent_session_id: SessionId;
    parent_agent_id: AgentId;
    source_wake_id: string;
    source_action_index: number;
    requested_task_id?: TaskId;
    correlation_id: string;
  };
  resource_limits?: {
    workdir?: string;
    max_duration_ms?: number;
    max_delegation_depth?: number;
  };
  tool_profile?: RawToolProfile;
  history_window?: {
    max_messages?: number;
  };
  status: SessionState["status"];
  brain_turn_count: number;
  created_at: string;
  last_active_at: string;
}

interface RawBodyState {
  session: RawSessionState;
  pending_messages: RawAgentMessage[];
  recent_events: RawCoreEvent[];
  child_completions: RawDelegatedCompletion[];
  fan_out_groups: RawDelegatedFanOutGroup[];
  delta_policy: {
    mode: BodyState["deltaPolicy"]["mode"];
    queue_owner: BodyState["deltaPolicy"]["queueOwner"];
    queued_message_ttl_ms: number;
    max_queued_messages: number;
  };
}

interface RawAgentMessage {
  from: AgentId;
  to: AgentId;
  body: string;
  correlation_id?: string;
  projection?: {
    visibility: "observation" | "user_visible";
    target_ref?: {
      system: string;
      kind: string;
      id: string;
    };
    work_ref?: {
      system: string;
      kind: string;
      id: string;
    };
    reason?: string;
  };
}

interface RawToolProfile {
  tools: Array<{
    name: string;
    description: string;
    input_schema?: number;
  }>;
}

interface RawDelegatedCompletion {
  run_id: RunId;
  child_session_id: SessionId;
  requested_task_id?: TaskId;
  source_wake_id: string;
  source_action_index: number;
  correlation_id?: string;
  parent_consumption: ParentConsumptionPolicy;
  packet: {
    session_id: SessionId;
    status: CompletionPacket["status"];
    summary: string;
  };
}

interface RawDelegatedFanOutGroup {
  group_id: string;
  total: number;
  pending: number;
  completed: number;
  failed: number;
  blocked: number;
  exhausted: number;
  cancelled: number;
  expired: number;
  max_concurrency?: number;
  failure_policy: FanOutFailurePolicy;
  status: BodyState["fanOutGroups"][number]["status"];
}

interface RawOpenAiResponsesBrainRunResult {
  stream: RawBrainWakeStreamItem[];
  provider_state?: RawBrainWakeProviderStateOutput;
  transport_metrics?: OpenAiResponsesTransportMetrics;
  credential_secret_update?: RawOpenAiResponsesCredentialSecretUpdate;
}

interface RawOpenAiResponsesCredentialSecretUpdate {
  provider_alias: string;
  secret: string;
}

interface RawOpenAiOauthCredentialSummary {
  kind: NativeModelProviderCredentialKind;
  version: number;
  has_secret: boolean;
  account_id?: string | null;
  email?: string | null;
  plan_type?: string | null;
  is_fedramp_account: boolean;
  access_token_expires_at?: string | null;
}

type RawOpenAiOauthCodeExchangeResult =
  | {
      ok: true;
      secret: string;
      summary: RawOpenAiOauthCredentialSummary;
    }
  | {
      ok: false;
      error: NativeOpenAiOauthExchangeError;
    };

interface RawOpenAiResponsesBufferedStartResult {
  wake_id: string;
}

interface RawPiAgentBufferedStartResult {
  wake_id: string;
}

interface RawOpenAiResponsesBufferedDrainResult {
  wake_id: string;
  items: RawBrainWakeStreamItem[];
  tool_requests?: Array<{
    call_id: string;
    provider_item_id?: string | null;
    name: string;
    arguments_json: string;
  }>;
  terminal: boolean;
  provider_state?: RawBrainWakeProviderStateOutput;
  transport_metrics?: OpenAiResponsesTransportMetrics;
  credential_secret_update?: RawOpenAiResponsesCredentialSecretUpdate;
  error?: string | null;
  cancellation?: RawOpenAiResponsesBufferedCancellation | null;
}

interface RawPiAgentBufferedDrainResult {
  wake_id: string;
  items: RawBrainWakeStreamItem[];
  tool_requests?: Array<{
    call_id: string;
    provider_item_id?: string | null;
    name: string;
    arguments_json: string;
  }>;
  terminal: boolean;
  transport_metrics?: {
    provider_request_count: number;
    tool_round_count: number;
  };
  error?: string | null;
  cancellation?: RawOpenAiResponsesBufferedCancellation | null;
}

interface RawOpenAiResponsesBufferedCancellation {
  reason_code: string;
  summary: string;
  cancelled_at: string;
}

interface RawOpenAiResponsesBufferedCancelResult {
  ok: true;
  wake_id: string;
  cancelled: boolean;
  terminal: boolean;
  cancellation?: RawOpenAiResponsesBufferedCancellation | null;
}

type RawBrainWakeStreamItem =
  | {
      type: "event";
      event: {
        wake_id: string;
        session_id: SessionId;
        event: RawBrainEvent;
      };
    }
  | {
      type: "actions";
      batch: {
        wake_id: string;
        session_id: SessionId;
        actions: RawBrainAction[];
      };
    }
  | {
      type: "wake_failed";
      failure: {
        wake_id: string;
        session_id: SessionId;
        kind: string;
        message: string;
      };
    };

type RawBrainAction =
  | {
      type: "send_message";
      message: RawAgentMessage;
    }
  | {
      type: "request_delegation";
      profile_id: ProfileId;
      task_id?: TaskId;
      prompt: string;
      expected_output?: string;
      resource_limits?: RawResourceLimits;
      timeout_ms?: number;
      priority?: Extract<
        BrainAction,
        { type: "request_delegation" }
      >["priority"];
      fan_out_group_id?: string;
      fan_out_max_concurrency?: number;
      fan_out_failure_policy?: Extract<
        BrainAction,
        { type: "request_delegation" }
      >["fanOutFailurePolicy"];
      correlation_id?: string;
      parent_consumption?: Extract<
        BrainAction,
        { type: "request_delegation" }
      >["parentConsumption"];
      capacity_request?: {
        member_id: string;
        claim_ttl_ms?: number;
        fallback_policy?: "reject_on_no_capacity" | "direct_on_no_capacity";
      };
    }
  | {
      type: "deliver_completion";
      packet: {
        session_id: SessionId;
        status: CompletionPacket["status"];
        summary: string;
      };
    };

type RawBrainWakeProviderStateOutput =
  | { type: "unchanged" }
  | {
      type: "replace";
      state: NativeBrainWakeProviderStateInput & { ttl_ms?: number };
    }
  | { type: "clear"; reason: "brain_requested_clear" };

type RawBrainEvent =
  | { type: "started" }
  | { type: "text_delta"; text: string }
  | { type: "reasoning_delta"; text: string; format?: string }
  | {
      type: "phase_change";
      phase: "idle" | "exploring" | "composing" | "reviewing";
      message?: string;
    }
  | {
      type: "tool_call_started";
      tool_name: string;
      metadata?: RawToolCallMetadata;
    }
  | {
      type: "tool_call_finished";
      tool_name: string;
      is_error: boolean;
      metadata?: RawToolCallMetadata;
    }
  | {
      type: "provider_status";
      level: "info" | "degraded" | "error";
      message: string;
      metadata_json?: string;
    }
  | { type: "finished" };

interface RawToolCallPolicyMetadata {
  allowed?: boolean;
  denial_reason?: string;
  timeout_ms?: number;
  cancelled?: boolean;
  archive_cleanup?: boolean;
}

interface RawToolCallMetadata {
  source: ToolCallMetadata["source"];
  adapter_id?: string;
  binding_id?: string;
  server_names: string[];
  profile_id?: string;
  tool_profile_key?: string;
  source_tool_name?: string;
  catalog_revision?: string;
  debug_detail_id?: string;
  policy?: RawToolCallPolicyMetadata;
}
