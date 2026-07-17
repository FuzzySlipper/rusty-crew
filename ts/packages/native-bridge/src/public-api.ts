import type {
  ActionBatchReceipt,
  AdapterId,
  AgentDirectoryEntry,
  AgentId,
  AgentCorrelatedRound,
  AgentMessageCommand,
  AgentMessageDeliveryCompletion,
  AgentMessageDeliveryReceipt,
  AgentMessageInboxItem,
  AgentMessageInboxQuery,
  AgentMessageReplyCommand,
  AgentRoundCommand,
  AgentRoundStartReceipt,
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

import type { NativeExternalRuntimeBridgeMethods } from "./external-runtime-public-api.js";
import type {
  NativeModelProviderCredentialKind,
  NativeModelProviderCredentialLink,
  NativeModelProviderCredentialLinkResult,
  NativeModelProviderCredentialUnlink,
  NativeModelProviderQuery,
  NativeModelProviderRecord,
  NativeModelProviderRefreshImpact,
  NativeModelProviderRefreshImpactRequest,
  NativeModelProviderRefreshPlan,
  NativeModelProviderRefreshPlanRequest,
  NativeModelProviderWrite,
  NativeServiceCredentialQuery,
  NativeServiceCredentialRecord,
  NativeServiceCredentialDelete,
  NativeServiceCredentialWrite,
} from "./model-provider-public-api.js";

export type {
  NativeModelProviderAffectedProfile,
  NativeModelProviderCredential,
  NativeModelProviderCredentialKind,
  NativeModelProviderCredentialLink,
  NativeModelProviderCredentialLinkResult,
  NativeModelProviderCredentialUnlink,
  NativeModelProviderProtocol,
  NativeModelProviderQuery,
  NativeModelProviderRecord,
  NativeModelProviderRefreshImpact,
  NativeModelProviderRefreshImpactRequest,
  NativeModelProviderRefreshMode,
  NativeModelProviderRefreshPlan,
  NativeModelProviderRefreshPlanRequest,
  NativeModelProviderRefreshProfileAction,
  NativeModelProviderStatus,
  NativeModelProviderWrite,
  NativeServiceCredentialQuery,
  NativeServiceCredentialRecord,
  NativeServiceCredentialDelete,
  NativeServiceCredentialWrite,
} from "./model-provider-public-api.js";

export interface NativeSessionConfigInput {
  sessionId: string;
  agentId: string;
  profileId: string;
  kind: "full" | "worker" | "delegated";
  resourceLimits?: ResourceLimits;
  toolProfile?: ToolProfile;
  historyWindow?: SessionState["historyWindow"];
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
  transportMetrics?:
    | OpenAiResponsesTransportMetrics
    | ChatCompletionsTransportMetrics;
  credentialSecretUpdate?: OpenAiResponsesCredentialSecretUpdate;
}

export type NativeBrainRunModuleId = "chat-completions" | "openai-responses";

export interface NativeBufferedBrainRunDrain {
  moduleId: NativeBrainRunModuleId;
  wakeId: string;
  items: BrainWakeStreamItem[];
  toolRequests: Array<{
    wakeId: string;
    callId: string;
    providerItemId?: string;
    name: string;
    argumentsJson: string;
  }>;
  terminal: boolean;
  providerState?: BrainWakeProviderStateOutput;
  transportMetrics?:
    | OpenAiResponsesTransportMetrics
    | ChatCompletionsTransportMetrics;
  credentialSecretUpdate?: OpenAiResponsesCredentialSecretUpdate;
  cancellation?: OpenAiResponsesBufferedCancellation;
  error?: string;
}

export type NativeBridgeRoundTripFixtureName =
  | "body_state_v1"
  | "list_sessions_v1"
  | "buffered_brain_run_drain_v1"
  | "profile_registry_record_v1"
  | "model_provider_record_v1"
  | "model_provider_refresh_impact_v1"
  | "memory_space_descriptor_v1"
  | "memory_proposal_record_v1"
  | "memory_governance_decision_record_v1"
  | "session_activity_digest_v1"
  | "context_compaction_artifact_v1";

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
    reasoningEffort?: string;
    maxOutputTokens?: number;
    providerRequestTimeoutMs?: number;
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

export interface ChatCompletionsChatCompletionMessage {
  role: "system" | "user" | "assistant" | "tool";
  content?: string;
  name?: string;
  toolCallId?: string;
  toolCalls?: unknown[];
}

export interface ChatCompletionsBrainRunInput {
  wakeId: string;
  sessionId: SessionId;
  messages: ChatCompletionsChatCompletionMessage[];
  tools?: Array<{
    name: string;
    description: string;
    inputSchema: unknown;
  }>;
  config: {
    model: string;
    providerRequestTimeoutMs?: number;
    wakeTimeoutMs?: number;
    temperatureMilli?: number;
    reasoningEffort?: string;
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

export interface ChatCompletionsToolRequest {
  wakeId: string;
  callId: string;
  providerItemId?: string;
  name: string;
  argumentsJson: string;
}

export interface ChatCompletionsTransportMetrics extends OpenAiResponsesTransportMetrics {
  toolRoundCount: number;
}

export interface OpenAiResponsesBufferedCancellation {
  reasonCode: string;
  summary: string;
  cancelledAt: string;
}

export interface NativeBrainWakeProviderStateInput {
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
  reasoningEffort?: string;
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

export type NativeExternalMemoryToolMode =
  | "off"
  | "metadata"
  | "candidate"
  | "manual"
  | "permissive";

export interface NativeToolAvailabilityPlanInput {
  selectedTools: string[];
  denMemory: {
    configured: boolean;
    clientAvailable: boolean;
    mode: NativeExternalMemoryToolMode;
    lastError?: string;
  };
}

export interface NativeToolAvailabilityOmission {
  toolName: string;
  reasonCode: string;
  message: string;
}

export interface NativeToolAvailabilityPlan {
  selectedTools: string[];
  omittedTools: NativeToolAvailabilityOmission[];
  diagnostics: NativeToolAvailabilityOmission[];
}

export interface NativeLocalCodeResourcePolicyInput {
  resourceLimits?: {
    workdir?: string;
    maxDurationMs?: number;
  };
}

export type NativeLocalCodeFilesystemScope = "unrestricted" | "workdir";
export type NativeLocalCodeExecutionMode = "parallel" | "sequential";

export interface NativeLocalCodeToolResourcePolicy {
  toolName: string;
  filesystemScope: NativeLocalCodeFilesystemScope;
  writesFiles: boolean;
  executesProcess: boolean;
  executionMode: NativeLocalCodeExecutionMode;
  outputShape: string;
}

export interface NativeLocalCodeResourcePolicyPlan {
  workdir: string;
  maxDurationMs?: number;
  commandTimeoutMs: number;
  maxReadBytes: number;
  maxSearchFileBytes: number;
  maxCommandOutputBytes: number;
  tools: NativeLocalCodeToolResourcePolicy[];
  denialReasonCodes: string[];
}

export interface NativeWebBrowserResourcePolicyInput {
  web?: Partial<NativeWebResourcePolicyPlan> & {
    allowedNonstandardPorts?: number[];
  };
  browser?: Partial<NativeBrowserResourcePolicyPlan>;
}

export interface NativeWebBrowserResourcePolicyPlan {
  web: NativeWebResourcePolicyPlan;
  browser: NativeBrowserResourcePolicyPlan;
  denialReasonCodes: string[];
}

export interface NativeWebResourcePolicyPlan {
  searchDefaultLimit: number;
  searchMaxResults: number;
  maxExtractUrls: number;
  maxExtractChars: number;
  maxExtractBytes: number;
  maxRedirects: number;
  allowPrivateNet: boolean;
  allowedNonstandardPorts: number[];
}

export interface NativeBrowserResourcePolicyPlan {
  maxServiceSessions: number;
  maxSessionsPerAgent: number;
  maxSessionsPerProfile?: number;
  idleTimeoutMs: number;
  hardLifetimeMs: number;
  startupTimeoutMs: number;
  cdpCallTimeoutMs: number;
  pageLoadTimeoutMs: number;
  maxRefs: number;
  consoleRingSize: number;
  maxScreenshotBytes: number;
  allowPrivateNet: boolean;
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

export interface NativeRuntimeGraphPlanInput {
  hostFacts: {
    configDir: string;
    engineDataDir: string;
    defaultWorkdir?: string;
    postgresDatabaseUrlEnvPresent: boolean;
  };
  serviceDefaults: {
    wakeTimeout?: {
      mode: "disabled" | "default";
      defaultMs?: number;
    };
    storage?: {
      backend: "sqlite" | "postgres";
      sqlite?: {
        path?: string;
        wal?: boolean;
        busyTimeoutMs?: number;
      };
      postgres?: {
        databaseUrlEnv?: string;
        schema?: string;
        bootMode?: "blocked" | "proof_admin" | "active";
        maxConnections?: number;
        statementTimeoutMs?: number;
      };
    };
  };
  runtimeConfig: Record<string, unknown>;
  profiles: Record<string, unknown>[];
}

export interface NativeRuntimeGraphPlan {
  accepted: boolean;
  sourceRevision: string;
  runtimeConfig: {
    profilesDir: string;
    skillsDir?: string;
    storage: {
      backend: "sqlite" | "postgres";
      implementationStatus:
        | "active"
        | "proof_admin_only"
        | "blocked_unimplemented";
      sqlite: {
        path: string;
        effectivePath: string;
        wal: boolean;
        busyTimeoutMs: number;
      };
      postgres: {
        databaseUrlEnv: string;
        schema: string;
        bootMode: "blocked" | "proof_admin" | "active";
        maxConnections: number;
        statementTimeoutMs: number;
      };
    };
    wakeTimeout: { mode: "disabled" | "default"; defaultMs?: number };
    brains: NativeBrainConfigDraft[];
    sessions: Array<
      NativeSessionConfigDraft & {
        resourceLimits: ResourceLimits;
        effectiveWakeTimeoutMs?: number;
        wakeTimeoutSource:
          | "disabled"
          | "session"
          | "profile_runtime"
          | "profile_session_default"
          | "service_default";
        localToolProfileId?: string;
        contextPolicyProfileId?: string;
        sessionMemoryPromptProfileId?: string;
      }
    >;
    scheduledJobs: Array<NativeScheduledJobConfigDraft & { payload?: unknown }>;
    channelBindings: NativeChannelBindingConfigDraft[];
    mcpBindings: NativeMcpBindingConfigDraft[];
  };
  derived: Array<{
    kind: "scheduled_job" | "mcp_binding";
    id: string;
    source: string;
  }>;
  defaultsApplied: Array<{
    path: string;
    source:
      | "canonical_profile_default"
      | "service_default"
      | "host_default_workdir"
      | "profile_runtime_default"
      | "profile_session_default";
  }>;
  diagnostics: NativeRuntimeConfigDiagnostic[];
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
  contextPolicy?: {
    enabled: boolean;
    strategyId: string;
    autoCompactionEnabled: boolean;
    compactAtPercent: number;
    targetPercentAfterCompaction: number;
    maxContextPercentForWake: number;
    debugVisibility: string;
    includeDebugEventsInModelContext: boolean;
    strategyConfig: Record<string, unknown>;
  };
}

export interface NativeCreateProfilePlanInput {
  runtimeConfig: NativeRuntimeConfigDraft;
  profiles: NativeProfileRuntimeMetadata[];
  profileRegistry?: NativeProfileRegistryRuntimeMetadata[];
  request: NativeCreateProfileRequest;
}

export interface NativeNewSessionControlPlanInput {
  command: {
    commandKind: string;
    targetSessionId?: string;
    requestId?: string;
    idempotencyKey?: string;
    operatorReason?: string;
    operatorReasonCode?: string;
  };
  template?: NativeNewSessionControlTemplate;
  generatedSessionId?: string;
  rebindHandlerAvailable?: boolean;
}

export interface NativeDelegatedRoleLifecyclePlanInput {
  parentSession: {
    sessionId: string;
    agentId: string;
    kind: "full" | "worker" | "delegated";
    resourceLimits?: ResourceLimits;
  };
  delegatedSessionId: string;
  delegatedAgentId: string;
  profileId: string;
  toolProfileKey?: string;
  requestedResourceLimits?: ResourceLimits;
  sourceWakeId: string;
  sourceActionIndex: number;
  taskId?: string;
  correlationId?: string;
}

export interface NativeDelegatedRoleLifecyclePlan {
  accepted: boolean;
  reasonCode: string;
  diagnostics: NativeRuntimeConfigDiagnostic[];
  sessionId: string;
  agentId: string;
  parentSessionId: string;
  parentAgentId: string;
  profileId: string;
  kind: "delegated";
  resourceLimits: ResourceLimits;
  toolProfileKey?: string;
  sourceWakeId: string;
  sourceActionIndex: number;
  taskId?: string;
  correlationId: string;
}

export interface NativeNewSessionControlTemplate {
  agentId: string;
  profileId: string;
  kind: "full" | "worker" | "delegated";
  channelBindingId?: string;
  channelId?: string;
  toolProfileKey?: string;
}

export interface NativeNewSessionControlPlan {
  accepted: boolean;
  commandKind: string;
  target: {
    oldSessionId?: string;
    newSessionId?: string;
    agentId?: string;
    profileId?: string;
    channelBindingId?: string;
    channelId?: string;
    toolProfileKey?: string;
  };
  idempotencyKey?: string;
  operatorReason: string;
  reasonCode: string;
  denial?: {
    reasonCode: string;
    summary: string;
  };
  preconditions: Array<{
    code: string;
    status: "satisfied" | "failed";
    summary: string;
  }>;
  actions: Array<{
    action: "archive_session" | "create_session" | "rebind_channel";
    sessionId?: string;
    oldSessionId?: string;
    newSessionId?: string;
    reasonCode: string;
  }>;
}

export type NativeBrainProviderProtocol = "chat_completions" | "responses";
export type NativeBrainProviderStateMode = "unused" | "optional" | "required";
export type NativeBrainHostCapability =
  | "execute_tool"
  | "project_debug_reference"
  | "project_event";

export interface NativeBrainProviderStatePolicy {
  mode: NativeBrainProviderStateMode;
  rebuild: {
    action: "discard" | "migrate" | "unsupported";
    reason: string;
    migration_id?: string;
  };
}

export interface NativeBrainStrategyDiagnostics {
  selected_strategy_id: string;
  effective_strategy_id: string;
  replay_fallback_used: boolean;
  fallback_reason?: string;
  fallback_reason_catalog?: string[];
}

export interface NativeBrainCatalogStrategy {
  strategy_id: string;
  provider_state: NativeBrainProviderStatePolicy;
  profile_fingerprint_options?: unknown;
  provider_fingerprint_options?: unknown;
  diagnostics: NativeBrainStrategyDiagnostics;
}

export interface NativeBrainCatalogModule {
  module_id: string;
  display_name: string;
  provider_protocols: NativeBrainProviderProtocol[];
  default_strategy_id: string;
  strategies: NativeBrainCatalogStrategy[];
  required_host_capabilities: NativeBrainHostCapability[];
}

export interface NativeBrainCatalog {
  revision: number;
  modules: NativeBrainCatalogModule[];
}

export interface NativeBrainSelectionRequest {
  configuredModuleId?: string;
  configuredStrategyId?: string;
  providerProtocol: NativeBrainProviderProtocol;
  providerKind: string;
  roleplayNarratorEnabled?: boolean;
}

export interface NativeBrainSelectionPlan {
  catalog_revision: number;
  module_id: string;
  selected_strategy_id: string;
  effective_strategy_id: string;
  provider_state_policy: NativeBrainProviderStatePolicy;
  profile_fingerprint_options?: unknown;
  provider_fingerprint_options?: unknown;
  strategy_diagnostics: NativeBrainStrategyDiagnostics;
  required_host_capabilities: NativeBrainHostCapability[];
}

export interface NativeReloadMcpControlPlanInput {
  command: {
    commandKind: string;
    targetSessionId?: string;
    requestId?: string;
    idempotencyKey?: string;
    operatorReason?: string;
    operatorReasonCode?: string;
  };
  binding?: {
    bindingId: string;
    sessionId: string;
    profileId: string;
    toolProfileKey?: string;
    endpointRef?: string;
  };
  reloadHandlerAvailable?: boolean;
}

export interface NativeReloadMcpControlPlan {
  accepted: boolean;
  commandKind: string;
  target: {
    sessionId?: string;
    bindingId?: string;
    profileId?: string;
    toolProfileKey?: string;
    endpointRef?: string;
  };
  idempotencyKey?: string;
  operatorReason: string;
  reasonCode: string;
  denial?: {
    reasonCode: string;
    summary: string;
  };
  preconditions: Array<{
    code: string;
    status: "satisfied" | "failed";
    summary: string;
  }>;
  actions: Array<{
    action: "reload_mcp_surface";
    sessionId: string;
    bindingId: string;
    reasonCode: string;
  }>;
}

export type NativeChannelIngressRouteDecision =
  | "routed"
  | "no_binding"
  | "inactive_binding"
  | "ambiguous"
  | "expired"
  | "duplicate"
  | "denied";

export interface NativeChannelIngressRouteMessage {
  adapterId: AdapterId;
  bindingId: string;
  provider: string;
  externalChannelId: string;
  externalThreadId?: string;
  externalUserId: string;
  body: string;
  mentions: string[];
  expiresAt: string;
  idempotencyKey: string;
  runtimeAgentId?: AgentId;
}

export interface NativeChannelIngressRoutePlanInput {
  message: NativeChannelIngressRouteMessage;
  bindings: NativeChannelBindingConfigDraft[];
  mentionAliases?: Record<string, AgentId>;
  systemAgentId?: AgentId;
  now?: string;
  seenIdempotencyKeys?: string[];
}

export interface NativeChannelIngressRouteRequest {
  from: AgentId;
  to: AgentId;
  body: string;
  correlationId: string;
  bindingId: string;
  sessionId?: SessionId;
}

export interface NativeChannelIngressRoutePlan {
  status: NativeChannelIngressRouteDecision;
  reasonCode: string;
  reason: string;
  correlationId?: string;
  binding?: NativeChannelBindingConfigDraft;
  candidates: NativeChannelBindingConfigDraft[];
  route?: NativeChannelIngressRouteRequest;
}

export interface NativeDenProductIngressPolicyInput {
  operation: string;
  entityKind: string;
  entityId: string;
  projectId?: string;
}

export interface NativeDenProductIngressPolicyPlan {
  status: "allowed" | "denied";
  operation: string;
  reasonCode: string;
  reason: string;
  lifecycleOperation: boolean;
}

export interface NativeCreateProfileRequest {
  profileId: string;
  displayName?: string;
  soulMarkdown?: string;
  memoryMarkdown?: string;
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
  total: number;
  source: "event_log" | "message_slots" | "pending_messages" | "empty";
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
  total: number;
  message_count: number;
  has_more_before: boolean;
}

export interface NativeExactPage<T> {
  items: T[];
  total: number;
  limit: number;
  offset: number;
  next_offset?: number | null;
}

export interface NativeChatSessionReadFacts {
  session: SessionState;
  message_count: number;
  latest_cursor: string;
  source: NativeChatReadModelPage["source"];
}

export interface NativeChatSessionSummaryPage {
  page: NativeExactPage<NativeChatSessionReadFacts>;
}

export interface NativeChatSessionReadResult {
  session: SessionState;
  events: NativeChatEventLogEvent[];
  latest_cursor: string;
  has_more: boolean;
  has_more_before: boolean;
  total: number;
  message_count: number;
  source: NativeChatReadModelPage["source"];
  message_slots: NativeExactPage<unknown>;
}

export interface NativeBridgeModule extends NativeExternalRuntimeBridgeMethods {
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
  brainCatalog(): Promise<NativeBrainCatalog>;
  planBrainSelection(
    input: NativeBrainSelectionRequest,
  ): Promise<NativeBrainSelectionPlan>;
  validateToolMetadataPolicy(
    input: NativeToolMetadataPolicyValidationInput,
  ): Promise<NativeToolMetadataPolicyValidationResult>;
  validateLocalToolProfilePolicy(
    input: NativeLocalToolProfilePolicyValidationInput,
  ): Promise<NativeLocalToolProfilePolicyValidationResult>;
  planToolAvailability(
    input: NativeToolAvailabilityPlanInput,
  ): Promise<NativeToolAvailabilityPlan>;
  planLocalCodeResourcePolicy(
    input: NativeLocalCodeResourcePolicyInput,
  ): Promise<NativeLocalCodeResourcePolicyPlan>;
  planWebBrowserResourcePolicy(
    input: NativeWebBrowserResourcePolicyInput,
  ): Promise<NativeWebBrowserResourcePolicyPlan>;
  validateRuntimeConfigDraft(
    input: NativeRuntimeConfigValidationInput,
  ): Promise<NativeRuntimeConfigValidationResult>;
  planRuntimeConfig(
    input: NativeRuntimeConfigValidationInput,
  ): Promise<NativeRuntimeConfigPlan>;
  planRuntimeGraph(
    input: NativeRuntimeGraphPlanInput,
  ): Promise<NativeRuntimeGraphPlan>;
  planCreateProfile(
    input: NativeCreateProfilePlanInput,
  ): Promise<NativeCreateProfilePlan>;
  planProfileRegistryMutation(
    input: NativeProfileRegistryMutationRequest,
  ): Promise<NativeProfileRegistryMutationPlan>;
  planNewSessionControl(
    input: NativeNewSessionControlPlanInput,
  ): Promise<NativeNewSessionControlPlan>;
  planReloadMcpControl(
    input: NativeReloadMcpControlPlanInput,
  ): Promise<NativeReloadMcpControlPlan>;
  planDelegatedRoleLifecycle(
    input: NativeDelegatedRoleLifecyclePlanInput,
  ): Promise<NativeDelegatedRoleLifecyclePlan>;
  planChannelIngressRoute(
    input: NativeChannelIngressRoutePlanInput,
  ): Promise<NativeChannelIngressRoutePlan>;
  planDenProductIngressPolicy(
    input: NativeDenProductIngressPolicyInput,
  ): Promise<NativeDenProductIngressPolicyPlan>;
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
  setSessionReasoningEffort(
    sessionId: SessionId,
    reasoningEffort?: string,
  ): Promise<NativeSessionStateSummary>;
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
  deliverAgentMessage(
    command: AgentMessageCommand,
  ): Promise<AgentMessageDeliveryReceipt>;
  replyAgentMessage(
    command: AgentMessageReplyCommand,
  ): Promise<AgentMessageDeliveryReceipt>;
  listAgentMessageInbox(
    query: AgentMessageInboxQuery,
  ): Promise<AgentMessageInboxItem[]>;
  listAgentDirectory(): Promise<AgentDirectoryEntry[]>;
  beginAgentRound(command: AgentRoundCommand): Promise<AgentRoundStartReceipt>;
  getAgentRound(roundId: string): Promise<AgentCorrelatedRound | undefined>;
  getAgentMessageDelivery(
    deliveryId: string,
  ): Promise<AgentMessageDeliveryReceipt | undefined>;
  completeAgentMessageDelivery(
    completion: AgentMessageDeliveryCompletion,
  ): Promise<AgentMessageDeliveryReceipt>;
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
  suspendForGitHubGate(
    input: GitHubGateSuspendRequest,
  ): Promise<GitHubGateWaitRecord>;
  consumeGitHubGateTerminalEvent(
    input: GitHubGateTerminalEvent,
  ): Promise<GitHubGateTerminalReceipt>;
  recoverGitHubGateWakes(): Promise<number>;
  gitHubGateWait(
    sessionId: SessionId,
  ): Promise<GitHubGateWaitRecord | undefined>;
  gitHubGateEventCursor(): Promise<number>;
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
  upsertServiceCredential(
    write: NativeServiceCredentialWrite,
  ): Promise<NativeServiceCredentialRecord>;
  listServiceCredentials(
    query?: NativeServiceCredentialQuery,
  ): Promise<NativeServiceCredentialRecord[]>;
  getServiceCredential(
    credentialId: string,
  ): Promise<NativeServiceCredentialRecord | undefined>;
  getServiceCredentialSecret(credentialId: string): Promise<string | undefined>;
  deleteServiceCredential(
    deleteRequest: NativeServiceCredentialDelete,
  ): Promise<NativeServiceCredentialRecord>;
  linkModelProviderCredential(
    link: NativeModelProviderCredentialLink,
  ): Promise<NativeModelProviderCredentialLinkResult>;
  unlinkModelProviderCredential(
    unlink: NativeModelProviderCredentialUnlink,
  ): Promise<NativeModelProviderRecord>;
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
  putRoleplayCharacter(write: unknown): Promise<unknown>;
  getRoleplayCharacter(id: string): Promise<unknown | undefined>;
  listRoleplayCharacters(query: unknown): Promise<unknown[]>;
  putRoleplayPlayerPersona(write: unknown): Promise<unknown>;
  getRoleplayPlayerPersona(id: string): Promise<unknown | undefined>;
  listRoleplayPlayerPersonas(query: unknown): Promise<unknown[]>;
  putRoleplaySessionMetadata(write: unknown): Promise<unknown>;
  getRoleplaySessionMetadata(id: string): Promise<unknown | undefined>;
  listRoleplaySessionMetadata(query: unknown): Promise<unknown[]>;
  applyRoleplaySessionProjection(write: unknown): Promise<unknown>;
  putRoleplayImport(write: unknown): Promise<unknown>;
  getRoleplayImport(id: string): Promise<unknown | undefined>;
  listRoleplayImports(query: unknown): Promise<unknown[]>;
  createRoleplayMechanicProposal(create: unknown): Promise<unknown>;
  getRoleplayMechanicProposal(proposalId: string): Promise<unknown | undefined>;
  listRoleplayMechanicProposals(query: unknown): Promise<unknown[]>;
  decideRoleplayMechanicProposal(decision: unknown): Promise<unknown>;
  applyRoleplayMechanicProposal(apply: unknown): Promise<unknown>;
  createRoleplayMechanicSessionAssociation(create: unknown): Promise<unknown>;
  getRoleplayMechanicSessionAssociation(
    sessionId: string,
  ): Promise<unknown | undefined>;
  listRoleplayMechanicSessionAssociations(query: unknown): Promise<unknown[]>;
  updateRoleplayMechanicSessionAttachment(update: unknown): Promise<unknown>;
  createRoleplayMechanicDiagnostic(create: unknown): Promise<unknown>;
  getRoleplayMechanicDiagnostic(
    diagnosticId: string,
  ): Promise<unknown | undefined>;
  listRoleplayMechanicDiagnostics(query: unknown): Promise<unknown[]>;
  updateRoleplayMechanicDiagnosticOutcome(update: unknown): Promise<unknown>;
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
  planCuratorGovernanceTransition(input: unknown): Promise<unknown>;
  applyCuratorGovernanceWrite(input: unknown): Promise<unknown>;
  getCuratorCandidate(candidateId: string): Promise<unknown | undefined>;
  listCuratorCandidates(query: unknown): Promise<unknown>;
  getCuratorMutation(mutationId: string): Promise<unknown | undefined>;
  listCuratorMutations(query: unknown): Promise<unknown>;
  listCuratorAuditReceipts(query: unknown): Promise<unknown>;
  planCuratorLifecycleTransition(input: unknown): Promise<unknown>;
  planBackgroundMemoryAutoMutations(input: unknown): Promise<unknown>;
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
  planRoleplayMechanicProfile(input: unknown): Promise<unknown>;
  startRoleplayNarratorTurn(input: unknown): Promise<unknown>;
  advanceRoleplayNarratorTurn(input: unknown): Promise<unknown>;
  saveMessageSlot(input: unknown): Promise<void>;
  saveMessageVariant(input: unknown): Promise<unknown>;
  createChatMessageSlot(input: unknown): Promise<unknown>;
  createChatMessageVariant(input: unknown): Promise<unknown>;
  applyRoleplayAlternative(input: unknown): Promise<unknown>;
  chatReadModelPage(input: unknown): Promise<NativeChatReadModelPage>;
  readChatSession(input: unknown): Promise<NativeChatSessionReadResult>;
  queryChatSessionSummaries(
    input: unknown,
  ): Promise<NativeChatSessionSummaryPage>;
  appendChatEvent(input: unknown): Promise<NativeChatEventLogEvent>;
  queryChatEvents(input: unknown): Promise<NativeChatEventLogPage>;
  queryMessageSlots(query: unknown): Promise<unknown[]>;
  queryMessageSlotsPage(query: unknown): Promise<NativeExactPage<unknown>>;
  queryMessageVariants(query: unknown): Promise<unknown[]>;
  queryMessageVariantsPage(query: unknown): Promise<NativeExactPage<unknown>>;
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
  readConversationTree(query: unknown): Promise<unknown>;
  searchChatTranscript(query: unknown): Promise<unknown>;
  resolveConversationJump(input: unknown): Promise<unknown>;
  saveAttachment(input: unknown): Promise<unknown>;
  createChatAttachment(input: unknown): Promise<unknown>;
  queryAttachments(query: unknown): Promise<unknown[]>;
  queryAttachmentsPage(query: unknown): Promise<NativeExactPage<unknown>>;
  removeAttachment(input: unknown): Promise<unknown>;
  removeChatAttachment(input: unknown): Promise<unknown>;
  saveDataBankScope(input: unknown): Promise<unknown>;
  createChatDataBankScope(input: unknown): Promise<unknown>;
  queryDataBankScopes(query: unknown): Promise<unknown[]>;
  queryDataBankScopesPage(query: unknown): Promise<NativeExactPage<unknown>>;
  removeDataBankScope(input: unknown): Promise<unknown>;
  removeChatDataBankScope(input: unknown): Promise<unknown>;
  providerStateDiagnostics(
    limit?: number,
  ): Promise<NativeProviderStateDiagnostic[]>;
  exchangeOpenAiOauthCode(
    input: NativeOpenAiOauthCodeExchangeInput,
  ): Promise<NativeOpenAiOauthCodeExchangeResult>;
  startBrainRun(
    input:
      | {
          moduleId: "chat-completions";
          providerInput: ChatCompletionsBrainRunInput;
        }
      | {
          moduleId: "openai-responses";
          providerInput: OpenAiResponsesBrainRunInput;
        },
  ): Promise<{ moduleId: NativeBrainRunModuleId; wakeId: string }>;
  drainBrainRun(input: {
    moduleId: "chat-completions" | "openai-responses";
    wakeId: string;
    maxItems?: number;
  }): Promise<NativeBufferedBrainRunDrain>;
  submitBrainHostResult(input: {
    moduleId: "chat-completions" | "openai-responses";
    wakeId: string;
    callId: string;
    output: string;
    status: "succeeded" | "denied" | "failed";
    reasonCode?: string;
    retryable: boolean;
    action?: string;
    summary?: string;
    debugDetailId?: string;
  }): Promise<{
    moduleId: "chat-completions" | "openai-responses";
    wakeId: string;
    callId: string;
  }>;
  cancelBrainRun(input: {
    moduleId: "chat-completions" | "openai-responses";
    wakeId: string;
    reasonCode: string;
    summary: string;
  }): Promise<{
    moduleId: "chat-completions" | "openai-responses";
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
