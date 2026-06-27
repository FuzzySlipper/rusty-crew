import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

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
  PlatformAdapterHandle,
  PlatformAdapterRegistration,
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
  runOpenaiResponsesBrainJson(inputJson: string): string;
  providerStateDiagnostics(limit?: number): NativeProviderStateDiagnostic[];
  saveMessageSlotJson(inputJson: string): void;
  saveMessageVariantJson(inputJson: string): string;
  queryMessageSlotsJson(inputJson: string): string;
  queryMessageVariantsJson(inputJson: string): string;
  selectActiveMessageVariantJson(inputJson: string): string;
  deleteMessageVariantJson(inputJson: string): string;
  reorderMessageVariantsJson(inputJson: string): string;
  saveConversationBranchJson(inputJson: string): string;
  queryConversationBranchesJson(inputJson: string): string;
  getConversationBranchStateJson(inputJson: string): string;
  selectActiveConversationBranchJson(inputJson: string): string;
  updateConversationBranchHeadJson(inputJson: string): string;
  saveConversationSnapshotJson(inputJson: string): string;
  queryConversationSnapshotsJson(inputJson: string): string;
  resolveConversationJumpJson(inputJson: string): string;
  saveAttachmentJson(inputJson: string): string;
  queryAttachmentsJson(inputJson: string): string;
  removeAttachmentJson(inputJson: string): string;
  saveDataBankScopeJson(inputJson: string): string;
  queryDataBankScopesJson(inputJson: string): string;
  removeDataBankScopeJson(inputJson: string): string;
  addLoreEntryJson(inputJson: string): string;
  replaceLoreEntryJson(inputJson: string): string;
  supersedeLoreEntryJson(inputJson: string): string;
  tombstoneLoreEntryJson(inputJson: string): string;
  queryLoreEntriesJson(inputJson: string): string;
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
  validateRuntimeConfigDraftJson(inputJson: string): string;
  planRuntimeConfigJson(inputJson: string): string;
  planCreateProfileJson(inputJson: string): string;
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
  createProfileRegistryRecordJson(writeJson: string): string;
  updateProfileRegistryRecordJson(updateJson: string): string;
  listProfileRegistryRecordsJson(queryJson: string): string;
  getProfileRegistryRecordJson(profileId: string): string;
  upsertModelProviderJson(writeJson: string): string;
  listModelProvidersJson(queryJson: string): string;
  getModelProviderJson(alias: string): string;
  getModelProviderSecretJson(alias: string): string;
  runMaintenance(
    policy: NativeRuntimeMaintenancePolicy,
  ): NativeRuntimeMaintenanceReport;
  listMemorySpaceDescriptorsJson(): string;
  querySessionMemoryRecordsJson(inputJson: string): string;
  buildSessionMemoryPromptContextJson(inputJson: string): string;
  saveMemoryProposalJson(inputJson: string): string;
  listMemoryProposalsJson(inputJson: string): string;
  recordMemoryGovernanceDecisionJson(inputJson: string): string;
  listProfileMemory(
    query: NativeProfileMemoryQuery,
  ): NativeProfileMemoryRecord[];
  listSimpleKv(query: NativeSimpleKvQuery): NativeSimpleKvRecord[];
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
}

export interface OpenAiResponsesBrainRunInput {
  wakeId: string;
  sessionId: SessionId;
  bodyState: BodyState;
  providerState?: BrainWakeProviderStateInput;
  providerStateAbsence?: ProviderStateAbsenceReason;
  config: {
    model: string;
    instructions?: string;
    streamIdleTimeoutMs?: number;
  };
  client?:
    | { mode: "fake" }
    | { mode: "live"; baseUrl: string; apiKey?: string };
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

export type NativeModelProviderStatus = "active" | "disabled" | "archived";
export type NativeModelProviderProtocol = "responses" | "chat_completions";

export interface NativeModelProviderCredential {
  hasSecret: boolean;
  secretRef?: string;
  updatedAt?: string;
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
  moduleRegistry: NativeRuntimeModuleSchemaRegistryDiagnostics;
  indexChecks: NativeRuntimeQueryPlanCheck[];
  searchHealthy: boolean;
  pressureSignals: NativeRuntimeStoragePressureSignal[];
  pressure: boolean;
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
  mcpToolProfile?: string;
  source?: NativeCreateProfileSourceRequest;
  now?: string;
  profileFileExists?: boolean;
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

export interface NativeCreateProfileFileAssetAction {
  kind: "write_profile_json";
  profileId: string;
  relativePath: string;
  overwrite: boolean;
  metadataJson: unknown;
}

export interface NativeCreateProfileDerivedRuntimeAction {
  kind: "add_brain" | "add_session" | "add_profile_mcp_config";
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

export interface NativeBridgeModule {
  readonly manifestVersion: number;
  readonly operationNames: readonly ManifestOperationName[];
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
  wakeBrain(request: BrainWakeRequest): Promise<BrainWakeAccepted>;
  submitBrainEvent(event: BrainEventEnvelope): Promise<EventReceipt>;
  submitBrainActions(batch: BrainActionBatch): Promise<ActionBatchReceipt>;
  registerPlatformAdapter(
    registration: PlatformAdapterRegistration,
  ): Promise<PlatformAdapterHandle>;
  validateRuntimeConfigDraft(
    input: NativeRuntimeConfigValidationInput,
  ): Promise<NativeRuntimeConfigValidationResult>;
  planRuntimeConfig(
    input: NativeRuntimeConfigValidationInput,
  ): Promise<NativeRuntimeConfigPlan>;
  planCreateProfile(
    input: NativeCreateProfilePlanInput,
  ): Promise<NativeCreateProfilePlan>;
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
  listMemoryProposals(
    query: MemoryProposalQuery,
  ): Promise<MemoryProposalRecord[]>;
  recordMemoryGovernanceDecision(
    decision: MemoryGovernanceDecisionInput,
  ): Promise<MemoryGovernanceDecisionRecord>;
  saveMessageSlot(input: unknown): Promise<void>;
  saveMessageVariant(input: unknown): Promise<unknown>;
  queryMessageSlots(query: unknown): Promise<unknown[]>;
  queryMessageVariants(query: unknown): Promise<unknown[]>;
  selectActiveMessageVariant(input: unknown): Promise<unknown>;
  deleteMessageVariant(input: unknown): Promise<unknown>;
  reorderMessageVariants(input: unknown): Promise<unknown[]>;
  saveConversationBranch(input: unknown): Promise<unknown>;
  queryConversationBranches(query: unknown): Promise<unknown[]>;
  getConversationBranchState(input: unknown): Promise<unknown>;
  selectActiveConversationBranch(input: unknown): Promise<unknown>;
  updateConversationBranchHead(input: unknown): Promise<unknown>;
  saveConversationSnapshot(input: unknown): Promise<unknown>;
  queryConversationSnapshots(query: unknown): Promise<unknown[]>;
  resolveConversationJump(input: unknown): Promise<unknown>;
  saveAttachment(input: unknown): Promise<unknown>;
  queryAttachments(query: unknown): Promise<unknown[]>;
  removeAttachment(input: unknown): Promise<unknown>;
  saveDataBankScope(input: unknown): Promise<unknown>;
  queryDataBankScopes(query: unknown): Promise<unknown[]>;
  removeDataBankScope(input: unknown): Promise<unknown>;
  providerStateDiagnostics(
    limit?: number,
  ): Promise<NativeProviderStateDiagnostic[]>;
  runOpenAiResponsesBrain(
    input: OpenAiResponsesBrainRunInput,
  ): Promise<BrainWakeExecutionResult>;
  listProfileMemory(
    query: NativeProfileMemoryQuery,
  ): Promise<NativeProfileMemoryRecord[]>;
  listSimpleKv(query: NativeSimpleKvQuery): Promise<NativeSimpleKvRecord[]>;
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

export const nativeManifestOperationNames = [
  "initialize_engine",
  "shutdown_engine",
  "register_brain_implementation",
  "replace_brain_implementation",
  "unregister_brain_implementation_for_profile",
  "wake_brain",
  "submit_brain_event",
  "submit_brain_actions",
  "apply_brain_provider_state_output",
  "register_platform_adapter",
  "validate_runtime_config_draft",
  "plan_runtime_config",
  "plan_create_profile",
  "inject_external_event",
  "inject_den_data_update",
  "enqueue_body_follow_up_message",
  "archive_session",
  "ensure_configured_session",
  "register_scheduled_wake_job",
  "run_scheduler_tick",
  "request_scheduled_job_run",
  "pause_scheduled_job",
  "resume_scheduled_job",
  "cancel_delegated_session",
  "request_delegated_checkpoint",
  "drain_delegated_sessions",
  "cleanup_delegated_resources",
  "delegated_session_status",
  "list_sessions",
  "provider_state_diagnostics",
  "save_message_slot",
  "save_message_variant",
  "query_message_slots",
  "query_message_variants",
  "select_active_message_variant",
  "delete_message_variant",
  "reorder_message_variants",
  "save_conversation_branch",
  "query_conversation_branches",
  "get_conversation_branch_state",
  "select_active_conversation_branch",
  "update_conversation_branch_head",
  "save_conversation_snapshot",
  "query_conversation_snapshots",
  "resolve_conversation_jump",
  "save_attachment",
  "query_attachments",
  "remove_attachment",
  "save_data_bank_scope",
  "query_data_bank_scopes",
  "remove_data_bank_scope",
  "database_size",
  "storage_schema",
  "create_profile_registry_record",
  "update_profile_registry_record",
  "list_profile_registry_records",
  "get_profile_registry_record",
  "upsert_model_provider",
  "list_model_providers",
  "get_model_provider",
  "get_model_provider_secret",
  "create_lore_layer",
  "get_lore_layer",
  "list_lore_layers",
  "update_lore_layer",
  "archive_lore_layer",
  "set_chat_layers",
  "get_chat_layers",
  "toggle_chat_layer",
  "reorder_chat_layers",
  "add_lore_entry",
  "replace_lore_entry",
  "supersede_lore_entry",
  "tombstone_lore_entry",
  "query_lore_entries",
  "lore_entry_provenance_events",
  "add_entry_to_layer",
  "remove_entry_from_layer",
  "set_entry_constant",
  "list_entries_by_layer",
  "recall_lore",
  "capture_lore_fact",
  "promote_lore_entry",
  "get_lore_layer_config",
  "set_lore_layer_config",
  "list_recall_traces",
  "get_recall_trace",
  "list_simple_kv",
  "storage_diagnostics",
  "run_maintenance",
  "subscribe_events",
  "unsubscribe_events",
  "get_buffer",
  "release_buffer",
] as const satisfies readonly ManifestOperationName[];

export async function loadNativeBridge(): Promise<NativeBridgeModule> {
  const addon = loadNativeAddon();
  if (!addon) {
    return createUnavailableNativeBridge();
  }

  return createNativeBridgeModule(new addon.NativeBridgeBinding());
}

export function createUnavailableNativeBridge(): NativeBridgeModule {
  return {
    manifestVersion: 1,
    operationNames: nativeManifestOperationNames,
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
    validateRuntimeConfigDraft: unavailable("validate_runtime_config_draft"),
    planRuntimeConfig: unavailable("plan_runtime_config"),
    planCreateProfile: unavailable("plan_create_profile"),
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
    upsertModelProvider: unavailable("initialize_engine"),
    listModelProviders: unavailable("initialize_engine"),
    getModelProvider: unavailable("initialize_engine"),
    getModelProviderSecret: unavailable("initialize_engine"),
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
    listMemoryProposals: unavailable("initialize_engine"),
    recordMemoryGovernanceDecision: unavailable("initialize_engine"),
    saveMessageSlot: unavailable("save_message_slot"),
    saveMessageVariant: unavailable("save_message_variant"),
    queryMessageSlots: unavailable("query_message_slots"),
    queryMessageVariants: unavailable("query_message_variants"),
    selectActiveMessageVariant: unavailable("select_active_message_variant"),
    deleteMessageVariant: unavailable("delete_message_variant"),
    reorderMessageVariants: unavailable("reorder_message_variants"),
    saveConversationBranch: unavailable("save_conversation_branch"),
    queryConversationBranches: unavailable("query_conversation_branches"),
    getConversationBranchState: unavailable("get_conversation_branch_state"),
    selectActiveConversationBranch: unavailable(
      "select_active_conversation_branch",
    ),
    updateConversationBranchHead: unavailable(
      "update_conversation_branch_head",
    ),
    saveConversationSnapshot: unavailable("save_conversation_snapshot"),
    queryConversationSnapshots: unavailable("query_conversation_snapshots"),
    resolveConversationJump: unavailable("resolve_conversation_jump"),
    saveAttachment: unavailable("save_attachment"),
    queryAttachments: unavailable("query_attachments"),
    removeAttachment: unavailable("remove_attachment"),
    saveDataBankScope: unavailable("save_data_bank_scope"),
    queryDataBankScopes: unavailable("query_data_bank_scopes"),
    removeDataBankScope: unavailable("remove_data_bank_scope"),
    providerStateDiagnostics: unavailable("provider_state_diagnostics"),
    runOpenAiResponsesBrain: unavailable("wake_brain"),
    listProfileMemory: unavailable("initialize_engine"),
    getProfileMemory: unavailable("initialize_engine"),
    addProfileMemory: unavailable("initialize_engine"),
    replaceProfileMemory: unavailable("initialize_engine"),
    removeProfileMemory: unavailable("initialize_engine"),
    listSimpleKv: unavailable("initialize_engine"),
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
  operation: ManifestOperationName,
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

  try {
    const nativeRequire = createRequire(import.meta.url);
    const artifactPath = fileURLToPath(
      new URL(`../native/${artifactName}`, import.meta.url),
    );
    return nativeRequire(artifactPath) as NativeAddon;
  } catch {
    return undefined;
  }
}

function nativeArtifactName(): string | undefined {
  if (process.platform === "linux" && process.arch === "x64") {
    return "index.linux-x64-gnu.node";
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
    operationNames:
      binding.operationNames.length > 0
        ? (binding.operationNames as ManifestOperationName[])
        : nativeManifestOperationNames,
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
    wakeBrain: async (request) => {
      const executor = wakeExecutors.get(request.brain);
      if (!executor) {
        throw new Error(
          `brain implementation handle ${request.brain} is not registered in the TS runtime`,
        );
      }

      const result = await executor.wake(request, module);
      for (const item of brainWakeStreamItemsFromExecutionResult(
        request,
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
            request.brain,
            request.sessionId,
            request.wakeId,
            JSON.stringify(result.providerState),
          );
        } catch (error) {
          observeProviderStateFailure(
            providerStateObservations,
            request,
            brainRegistrations.get(request.brain),
            "save_failed",
          );
          await module.submitBrainEvent({
            wakeId: request.wakeId,
            sessionId: request.sessionId,
            event: {
              type: "provider_status",
              level: "degraded",
              message: `provider state save failed: ${errorMessage(error)}`,
            },
          });
        }
      }
      return { wakeId: request.wakeId, accepted: true };
    },
    submitBrainEvent: async (event) => {
      const nativeEvent = toNativeBrainEvent(event.event);
      return binding.submitBrainEvent(
        event.wakeId,
        event.sessionId,
        nativeEvent.eventType,
        nativeEvent.text,
        nativeEvent.toolName,
        nativeEvent.isError,
        nativeEvent.metadataJson,
      );
    },
    submitBrainActions: async (batch) => {
      const receipt = binding.submitBrainActionsJson(
        batch.wakeId,
        batch.sessionId,
        new TextEncoder().encode(
          JSON.stringify(batch.actions.map(toNativeBrainAction)),
        ),
      );
      return {
        wakeId: receipt.wakeId,
        acceptedActions: receipt.acceptedActions,
        rejectedActions: JSON.parse(
          receipt.rejectedActionsJson,
        ) as ActionBatchReceipt["rejectedActions"],
      };
    },
    registerPlatformAdapter: async (registration) =>
      binding.registerPlatformAdapter({
        adapterId: registration.adapterId,
        kind: registration.kind,
        displayName: registration.displayName,
      }) as PlatformAdapterHandle,
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
      (JSON.parse(binding.listSessionsJson()) as RawSessionState[]).map(
        toSessionState,
      ),
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
      observeProviderStateWake(
        providerStateObservations,
        request,
        brainRegistrations.get(input.brain),
      );
      return request;
    },
    diagnosticProjectBodyStateJson: async (sessionId) =>
      binding.projectBodyStateJson(sessionId),
    diagnosticSubmitBrainActionsJson: async (wakeId, sessionId, actions) => {
      const receipt = binding.submitBrainActionsJson(
        wakeId,
        sessionId,
        new TextEncoder().encode(
          JSON.stringify(actions.map(toNativeBrainAction)),
        ),
      );
      return {
        wakeId: receipt.wakeId,
        acceptedActions: receipt.acceptedActions,
        rejectedActions: JSON.parse(receipt.rejectedActionsJson) as [],
      };
    },
    diagnosticCountRows: async (table) => binding.countRows(table),
    databaseSize: async () => binding.databaseSize(),
    storageDiagnostics: async () => binding.storageDiagnostics(),
    storageSchema: async () => binding.storageSchema(),
    createProfileRegistryRecord: async (write) =>
      toNativeProfileRegistryRecord(
        JSON.parse(
          binding.createProfileRegistryRecordJson(
            JSON.stringify(toRawProfileRegistryWrite(write)),
          ),
        ) as RawProfileRegistryRecord,
      ),
    updateProfileRegistryRecord: async (update) =>
      toNativeProfileRegistryRecord(
        JSON.parse(
          binding.updateProfileRegistryRecordJson(
            JSON.stringify(toRawProfileRegistryUpdate(update)),
          ),
        ) as RawProfileRegistryRecord,
      ),
    listProfileRegistryRecords: async (query = {}) =>
      (
        JSON.parse(
          binding.listProfileRegistryRecordsJson(
            JSON.stringify(toRawProfileRegistryQuery(query)),
          ),
        ) as RawProfileRegistryRecord[]
      ).map(toNativeProfileRegistryRecord),
    getProfileRegistryRecord: async (profileId) => {
      const raw = JSON.parse(
        binding.getProfileRegistryRecordJson(profileId),
      ) as RawProfileRegistryRecord | null;
      return raw ? toNativeProfileRegistryRecord(raw) : undefined;
    },
    upsertModelProvider: async (write) =>
      toNativeModelProviderRecord(
        JSON.parse(
          binding.upsertModelProviderJson(
            JSON.stringify(toRawModelProviderWrite(write)),
          ),
        ) as RawModelProviderRecord,
      ),
    listModelProviders: async (query = {}) =>
      (
        JSON.parse(
          binding.listModelProvidersJson(
            JSON.stringify(toRawModelProviderQuery(query)),
          ),
        ) as RawModelProviderRecord[]
      ).map(toNativeModelProviderRecord),
    getModelProvider: async (alias) => {
      const raw = JSON.parse(
        binding.getModelProviderJson(alias),
      ) as RawModelProviderRecord | null;
      return raw ? toNativeModelProviderRecord(raw) : undefined;
    },
    getModelProviderSecret: async (alias) =>
      (JSON.parse(binding.getModelProviderSecretJson(alias)) as
        | string
        | null) ?? undefined,
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
    listMemoryProposals: async (query) =>
      JSON.parse(
        binding.listMemoryProposalsJson(JSON.stringify(query)),
      ) as MemoryProposalRecord[],
    recordMemoryGovernanceDecision: async (decision) =>
      JSON.parse(
        binding.recordMemoryGovernanceDecisionJson(JSON.stringify(decision)),
      ) as MemoryGovernanceDecisionRecord,
    saveMessageSlot: async (input) =>
      binding.saveMessageSlotJson(JSON.stringify(input)),
    saveMessageVariant: async (input) =>
      JSON.parse(
        binding.saveMessageVariantJson(JSON.stringify(input)),
      ) as unknown,
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
    queryAttachments: async (query) =>
      JSON.parse(
        binding.queryAttachmentsJson(JSON.stringify(query)),
      ) as unknown[],
    removeAttachment: async (input) =>
      JSON.parse(
        binding.removeAttachmentJson(JSON.stringify(input)),
      ) as unknown,
    saveDataBankScope: async (input) =>
      JSON.parse(
        binding.saveDataBankScopeJson(JSON.stringify(input)),
      ) as unknown,
    queryDataBankScopes: async (query) =>
      JSON.parse(
        binding.queryDataBankScopesJson(JSON.stringify(query)),
      ) as unknown[],
    removeDataBankScope: async (input) =>
      JSON.parse(
        binding.removeDataBankScopeJson(JSON.stringify(input)),
      ) as unknown,
    providerStateDiagnostics: async (limit = 100) => {
      const stored = binding
        .providerStateDiagnostics(limit)
        .map(toNativeProviderStateDiagnostic);
      return mergeProviderStateDiagnostics([
        ...providerStateObservations.values(),
        ...stored,
      ]).slice(0, limit);
    },
    runOpenAiResponsesBrain: async (input) => {
      const raw = JSON.parse(
        binding.runOpenaiResponsesBrainJson(
          JSON.stringify(toNativeOpenAiResponsesBrainRunInput(input)),
        ),
      ) as RawOpenAiResponsesBrainRunResult;
      return {
        stream: raw.stream.map(toBrainWakeStreamItem),
        events: [],
        actions: [],
        providerState: raw.provider_state
          ? toBrainWakeProviderStateOutput(raw.provider_state)
          : undefined,
      };
    },
    listProfileMemory: async (query) => binding.listProfileMemory(query),
    listSimpleKv: async (query) => binding.listSimpleKv(query),
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
  return {
    runtime_config: {
      profiles_dir: input.runtimeConfig.profilesDir,
      skills_dir: input.runtimeConfig.skillsDir,
      brains: input.runtimeConfig.brains.map((brain) => ({
        implementation_id: brain.implementationId,
        profile_id: brain.profileId,
      })),
      sessions: input.runtimeConfig.sessions.map((session) => ({
        session_id: session.sessionId,
        agent_id: session.agentId,
        profile_id: session.profileId,
        kind: session.kind,
        resource_limits: toNativeResourceLimits(session.resourceLimits),
        owner_id: session.ownerId,
        history_window: session.historyWindow
          ? {
              max_messages: session.historyWindow.maxMessages,
            }
          : undefined,
        max_history_messages: session.maxHistoryMessages,
        turn_timeout_ms: session.turnTimeoutMs,
      })),
      scheduled_jobs: input.runtimeConfig.scheduledJobs.map((job) => ({
        id: job.id,
        schedule: job.schedule,
        shape: job.shape,
        job_kind: job.jobKind,
        target_session_id: job.targetSessionId,
        script: job.script,
        delivery_channel_id: job.deliveryChannelId,
      })),
      channel_bindings: input.runtimeConfig.channelBindings.map((binding) => ({
        binding_id: binding.bindingId,
        adapter_id: binding.adapterId,
        provider: binding.provider,
        agent_id: binding.agentId,
        instance_id: binding.instanceId,
        session_id: binding.sessionId,
        profile_id: binding.profileId,
        external_channel_id: binding.externalChannelId,
        external_thread_id: binding.externalThreadId,
        external_user_id: binding.externalUserId,
        conversation_project_id: binding.conversationProjectId,
        conversation_channel_id: binding.conversationChannelId,
        provider_subscription_id: binding.providerSubscriptionId,
        status: binding.status,
      })),
      mcp_bindings: input.runtimeConfig.mcpBindings.map((binding) => ({
        binding_id: binding.bindingId,
        adapter_id: binding.adapterId,
        agent_id: binding.agentId,
        instance_id: binding.instanceId,
        session_id: binding.sessionId,
        profile_id: binding.profileId,
        server_names: binding.serverNames,
        endpoint_ref: binding.endpointRef,
        transport: binding.transport,
        tool_profile_key: binding.toolProfileKey,
        status: binding.status,
      })),
    },
    profiles: input.profiles.map((profile) => ({
      profile_id: profile.profileId,
      brain: profile.brain
        ? {
            module: profile.brain.module,
            strategy: profile.brain.strategy,
          }
        : undefined,
      runtime: profile.runtime
        ? {
            default_resource_limits: toNativeResourceLimits(
              profile.runtime.defaultResourceLimits,
            ),
            max_turn_duration_ms: profile.runtime.maxTurnDurationMs,
            max_tokens_per_turn: profile.runtime.maxTokensPerTurn,
          }
        : undefined,
      session_defaults: profile.sessionDefaults
        ? {
            owner_id: profile.sessionDefaults.ownerId,
            max_history_messages: profile.sessionDefaults.maxHistoryMessages,
            turn_timeout_ms: profile.sessionDefaults.turnTimeoutMs,
          }
        : undefined,
      mcp_config: profile.mcpConfig
        ? {
            binding_id: profile.mcpConfig.bindingId,
            endpoint_ref: profile.mcpConfig.endpointRef,
            server_names: profile.mcpConfig.serverNames,
            transport: profile.mcpConfig.transport,
            tool_profile: profile.mcpConfig.toolProfile,
          }
        : undefined,
      background_review: profile.backgroundReview
        ? {
            enabled: profile.backgroundReview.enabled,
            review_type: profile.backgroundReview.reviewType,
            schedule: profile.backgroundReview.schedule,
          }
        : undefined,
      channel_defaults: profile.channelDefaults
        ? {
            wake_policy: profile.channelDefaults.wakePolicy,
          }
        : undefined,
    })),
  };
}

function toNativeCreateProfilePlanInput(
  input: NativeCreateProfilePlanInput,
): unknown {
  const base = toNativeRuntimeConfigValidationInput({
    runtimeConfig: input.runtimeConfig,
    profiles: input.profiles,
  }) as Record<string, unknown>;
  return {
    ...base,
    profile_registry: input.profileRegistry?.map((record) => ({
      profile_id: record.profileId,
      lifecycle_status: record.lifecycleStatus,
      revision: record.revision,
    })),
    request: {
      profile_id: input.request.profileId,
      display_name: input.request.displayName,
      agent_id: input.request.agentId,
      session_id: input.request.sessionId,
      implementation_id: input.request.implementationId,
      kind: input.request.kind,
      provider_alias: input.request.providerAlias,
      model_config: input.request.modelConfig
        ? {
            provider: input.request.modelConfig.provider,
            model_name: input.request.modelConfig.modelName,
            base_url: input.request.modelConfig.baseUrl,
            api: input.request.modelConfig.api,
            api_key_env: input.request.modelConfig.apiKeyEnv,
            temperature_milli: input.request.modelConfig.temperatureMilli,
            max_output_tokens: input.request.modelConfig.maxOutputTokens,
          }
        : undefined,
      brain: input.request.brain
        ? {
            module: input.request.brain.module,
            strategy: input.request.brain.strategy,
          }
        : undefined,
      mcp_tool_profile: input.request.mcpToolProfile,
      source: input.request.source
        ? {
            template_id: input.request.source.templateId,
            source_profile_id: input.request.source.sourceProfileId,
            source_bundle_path: input.request.source.sourceBundlePath,
          }
        : undefined,
      now: input.request.now,
      profile_file_exists: input.request.profileFileExists ?? false,
    },
  };
}

function toNativeResourceLimits(limits: ResourceLimits | undefined): unknown {
  if (!limits) {
    return undefined;
  }
  return {
    workdir: limits.workdir,
    max_duration_ms: limits.maxDurationMs,
    max_delegation_depth: limits.maxDelegationDepth,
  };
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
    },
    metadataJson: record.metadata_json,
    revision: record.revision,
    createdAt: record.created_at,
    updatedAt: record.updated_at,
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
      tools: state.tool_profile?.tools ?? [],
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
  };
}

function toBrainEvent(event: RawBrainEvent): BrainEvent {
  switch (event.type) {
    case "started":
    case "finished":
      return event;
    case "text_delta":
      return { type: event.type, text: event.text };
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

interface RawCreateProfileFileAssetAction {
  kind: "write_profile_json";
  profile_id: string;
  relative_path: string;
  overwrite: boolean;
  metadata_json: unknown;
}

interface RawCreateProfileDerivedRuntimeAction {
  kind: "add_brain" | "add_session" | "add_profile_mcp_config";
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

interface RawModelProviderCredential {
  has_secret: boolean;
  secret_ref?: string | null;
  updated_at?: string | null;
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
  tool_profile?: SessionState["toolProfile"];
  history_window?: {
    max_messages?: number;
  };
  status: SessionState["status"];
  brain_turn_count: number;
  created_at: string;
  last_active_at: string;
}

interface RawAgentMessage {
  from: AgentId;
  to: AgentId;
  body: string;
  correlation_id?: string;
}

interface RawOpenAiResponsesBrainRunResult {
  stream: RawBrainWakeStreamItem[];
  provider_state?: RawBrainWakeProviderStateOutput;
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
  policy?: RawToolCallPolicyMetadata;
}
