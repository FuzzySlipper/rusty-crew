export type { Brand } from "./brands.js";
export * from "./generated/bridge-manifest.js";
export * from "./generated/core-protocol.js";

import type {
  ActionBatchReceipt,
  ActionRejection,
  AdapterId,
  AgentId,
  AgentInstanceId,
  AgentMessage,
  AgentMessageProjectionHint,
  BodyDeltaPolicy,
  BodyState,
  BrainAction,
  BrainActionBatch,
  BrainEvent,
  BrainEventEnvelope,
  BrainImplementationHandle,
  BrainImplementationId,
  BrainPhase,
  BrainProviderStateScope,
  BrainProviderStateStrategyMetadata,
  BrainProviderStatusLevel,
  BrainStrategyMetadata,
  BrainWakeAccepted,
  BrainWakeFailure,
  BrainWakeProviderStateInput,
  BrainWakeProviderStateOutput,
  BrainWakeProviderStateUpdate,
  BrainWakeRequest,
  BrainWakeStreamItem,
  CompletionPacket,
  CompletionStatus,
  ContextCompactionArtifact,
  ContextCompactionArtifactQuery,
  ConversationBranchId,
  CoreError,
  CoreErrorKind,
  CoreEvent,
  CoreEventKind,
  DelegatedCompletion,
  DelegatedFanOutGroup,
  DelegatedResourceCleanupReport,
  DelegatedRunStatus,
  DelegatedSessionRuntimeStatus,
  DelegationLifecycleEvent,
  DelegationLifecyclePhase,
  DelegationLineage,
  DelegationPriority,
  DeltaQueueOwner,
  DenDataUpdate,
  EngineHandle,
  EventReceipt,
  EventSubscription,
  ExternalEvent,
  ExternalEventPayload,
  FanOutFailurePolicy,
  FanOutGroupStatus,
  MemoryConflictPolicy,
  MemoryDiagnosticsPolicy,
  MemoryEvidenceKind,
  MemoryEvidenceRef,
  MemoryExportImportPolicy,
  MemoryFieldType,
  MemoryGovernanceDecisionInput,
  MemoryGovernanceDecisionKind,
  MemoryGovernanceDecisionRecord,
  MemoryGovernanceMode,
  MemoryIndexingPolicy,
  MemoryOperation,
  MemoryOperationPolicy,
  MemoryPromptPolicy,
  MemoryProposalEnvelope,
  MemoryProposalQuery,
  MemoryProposalRecord,
  MemoryProposalReviewStatus,
  MemoryProposalSource,
  MemoryProvenancePolicy,
  MemoryRecordFieldDescriptor,
  MemoryRecordShapeDescriptor,
  MemoryRecordShapeId,
  MemoryRecordShapeRef,
  MemoryRetentionPolicy,
  MemoryRetrievalStrategy,
  MemoryScope,
  MemoryScopeModel,
  MemoryScopeType,
  MemorySpaceDescriptor,
  MemorySpaceId,
  MemoryVisibilityModel,
  MemoryWritePolicy,
  MidTurnDeltaMode,
  ParentConsumptionPolicy,
  PlatformAdapterHandle,
  PlatformAdapterKind,
  PlatformAdapterRegistration,
  ProfileId,
  ProjectId,
  ProjectionRef,
  ProjectionVisibility,
  ProviderStateAbsenceReason,
  ProviderStateClearReason,
  ProviderStateMode,
  ResourceLimits,
  RunId,
  RuntimeBufferHandle,
  SessionActivityDigest,
  SessionActivityDigestQuery,
  SessionConfig,
  SessionHandle,
  SessionHistoryWindow,
  SessionId,
  SessionKind,
  SessionState,
  SessionStatus,
  SubscriptionHandle,
  TaskId,
  ToolCallMetadata,
  ToolCallPolicyMetadata,
  ToolCallSource,
  ToolDescriptor,
  ToolProfile,
  WorkerPoolCapacityFallbackPolicy,
  WorkerPoolCapacityRequest,
} from "./generated/core-protocol.js";

export interface Unit {}

export type ClockConfig = "system" | { fixed: string };

export interface EngineConfig {
  engineDataDir: string;
  clock: ClockConfig;
  defaultTurnBudget: number;
  defaultIdleTimeoutMs: number;
  storage?: EngineStorageConfig;
}

export type EngineStorageConfig =
  | {
      backend: "sqlite";
      filesystemWarningFreePercent?: number;
    }
  | {
      backend: "postgres";
      databaseUrl: string;
      schema: string;
      maxConnections?: number;
      statementTimeoutMs?: number;
      backingFilesystemPath?: string;
      filesystemWarningFreePercent?: number;
    };

export interface ShutdownRequest {
  engine: EngineHandle;
  drainTimeoutMs: number;
}

export interface ShutdownSummary {
  archivedSessions: number;
  droppedSubscriptions: number;
}

export type GitHubGateWaitPhase =
  | "waiting"
  | "wake_scheduled"
  | "consumed"
  | "cancelled";

export interface GitHubGateSuspendRequest {
  sessionId: SessionId;
  runId?: RunId;
  providerThreadId?: string;
  projectId: ProjectId;
  taskId: TaskId;
  gateId: number;
  commitSha: string;
  now: string;
}

export interface GitHubGateWaitRecord {
  sessionId: SessionId;
  runId?: RunId;
  providerThreadId?: string;
  projectId: ProjectId;
  taskId: TaskId;
  gateId: number;
  commitSha: string;
  phase: GitHubGateWaitPhase;
  terminalEventId?: number;
  createdAt: string;
  updatedAt: string;
}

export interface GitHubGateTerminalEvent {
  eventId: number;
  gateId: number;
  projectId: ProjectId;
  taskId: TaskId;
  commitSha: string;
  status: "passed" | "failed" | "timed_out" | "superseded";
  terminalReason:
    | "checks_passed"
    | "checks_failed"
    | "required_checks_missing"
    | "timeout"
    | "superseded";
  summary?: string;
  failureSummary?: string;
  completedAt: string;
}

export interface GitHubGateTerminalReceipt {
  eventId: number;
  cursor: number;
  duplicate: boolean;
  wakeScheduled: boolean;
  ignoredReason?: string;
  wait?: GitHubGateWaitRecord;
}

export type ChannelProvider = "den_channels" | "telegram" | "simulated";
export type ChannelVisibility = "conversation" | "task" | "debug" | "system";
export type ChannelSeverity = "info" | "success" | "warning" | "error";
export type ChannelDeliveryPolicy = "best_effort" | "must_ack" | "dry_run";
export type ExternalBindingStatus =
  | "active"
  | "degraded"
  | "disconnected"
  | "archived";
export type ChannelMembershipStatus = "joined" | "left" | "invited" | "unknown";
export type ChannelPresenceStatus = "online" | "idle" | "offline" | "unknown";
export type ChannelSubscriptionStatus =
  | "active"
  | "degraded"
  | "disconnected"
  | "paused"
  | "archived";
export type ChannelSubscriptionTransportKind =
  | "websocket"
  | "http_poll"
  | "webhook"
  | "simulation"
  | "rust_event_subscription";

export interface ChannelRuntimeIdentity {
  agentId?: AgentId;
  instanceId?: AgentInstanceId;
  sessionId?: SessionId;
  profileId?: ProfileId;
}

export interface ChannelProviderRefs {
  provider: ChannelProvider | string;
  externalChannelId: string;
  externalThreadId?: string;
  externalMessageId?: string;
  externalUserId?: string;
}

export interface ChannelAuthorRef {
  externalUserId: string;
  displayLabel?: string;
  username?: string;
  kind?: "human" | "bot" | "sender_chat";
  isBot?: boolean;
}

export interface ChannelAttachmentRef {
  ref: string;
  mediaType?: string;
  label?: string;
  attachmentId?: string;
  filename?: string;
  byteSize?: number;
  sha256?: string;
  contentUrl?: string;
  state?:
    | "pending"
    | "available"
    | "unsupported"
    | "oversized"
    | "expired"
    | "failed";
  reasonCode?: string;
}

export interface NormalizedChannelInboundMessage {
  kind: "channel_inbound_message.v1";
  adapterId: AdapterId;
  bindingId: string;
  runtime: ChannelRuntimeIdentity;
  providerRefs: ChannelProviderRefs;
  author: ChannelAuthorRef;
  replyToExternalMessageId?: string;
  messageMutation?: "original" | "edited";
  body: string;
  summary?: string;
  attachments: ChannelAttachmentRef[];
  mentions: string[];
  receivedAt: string;
  ttlMs: number;
  expiresAt: string;
  cursor?: string;
  idempotencyKey: string;
  visibility: ChannelVisibility;
  provenance: Record<string, unknown>;
}

export interface NormalizedChannelOutboundMessage {
  kind: "channel_outbound_message.v1";
  adapterId: AdapterId;
  bindingId: string;
  runtime: ChannelRuntimeIdentity;
  providerRefs: ChannelProviderRefs;
  body: string;
  attachments?: ChannelAttachmentRef[];
  replyToExternalMessageId?: string;
  correlationId?: string;
  idempotencyKey: string;
  visibility: ChannelVisibility;
  deliveryPolicy: ChannelDeliveryPolicy;
  resultRef?: string;
  workRef?: string;
}

export interface NormalizedChannelActivityProjection {
  kind: "channel_activity_projection.v1";
  adapterId: AdapterId;
  bindingId: string;
  runtime: ChannelRuntimeIdentity;
  providerRefs: ChannelProviderRefs;
  eventType: string;
  summary: string;
  severity: ChannelSeverity;
  workRef?: string;
  resultRef?: string;
  workRefs?: WorkReference[];
  resultRefs?: ResultReference[];
  createdAt: string;
}

export type ChannelReadbackVisibilityFilter = ChannelVisibility | "any";

export type ChannelReadbackReasonCode =
  | "agent_context"
  | "operator_debug"
  | "incident_review";

export interface NormalizedChannelReadbackRequest {
  kind: "channel_readback_request.v1";
  adapterId?: AdapterId;
  bindingId: string;
  providerRefs?: Partial<ChannelProviderRefs>;
  requester: ChannelRuntimeIdentity;
  beforeExternalMessageId?: string;
  beforeCursor?: string;
  limit?: number;
  maxBodyChars?: number;
  visibility?: ChannelReadbackVisibilityFilter;
  includeExpired?: boolean;
  reasonCode: ChannelReadbackReasonCode;
}

export interface NormalizedChannelReadbackMessageSummary {
  providerRefs: ChannelProviderRefs;
  author: ChannelAuthorRef;
  bodySnippet: string;
  summary?: string;
  receivedAt: string;
  expiresAt: string;
  cursor?: string;
  visibility: ChannelVisibility;
  attachmentCount: number;
  truncated: boolean;
}

export interface ChannelReadbackCursorBoundaries {
  oldestCursor?: string;
  newestCursor?: string;
  beforeCursor?: string;
  beforeExternalMessageId?: string;
}

export interface NormalizedChannelReadbackResponse {
  kind: "channel_readback_response.v1";
  adapterId?: AdapterId;
  bindingId: string;
  providerRefs?: Partial<ChannelProviderRefs>;
  messages: NormalizedChannelReadbackMessageSummary[];
  cursorBoundaries: ChannelReadbackCursorBoundaries;
  truncated: boolean;
  provenance: Record<string, unknown>;
  errors?: string[];
  degradedReason?: string;
}

export type ReferenceSourceDomain =
  | "runtime"
  | "den"
  | "channel"
  | "mcp"
  | "artifact"
  | "git";

export type WorkReferenceKind =
  | "project"
  | "task"
  | "assignment"
  | "run"
  | "session"
  | "delegation_run"
  | "channel_binding"
  | "channel_message"
  | "mcp_surface";

export type ResultReferenceKind =
  | "completion_packet"
  | "runtime_event"
  | "scheduler_run"
  | "curator_candidate_batch"
  | "den_message"
  | "den_document"
  | "den_task"
  | "observation_event"
  | "diagnostics_bundle"
  | "artifact"
  | "commit"
  | "channel_message";

export interface WorkReference {
  kind: "work_ref.v1";
  sourceDomain: ReferenceSourceDomain;
  refKind: WorkReferenceKind | string;
  id: string;
  projectId?: ProjectId | string;
  label?: string;
  externalUrl?: string;
}

export interface ResultReference {
  kind: "result_ref.v1";
  sourceDomain: ReferenceSourceDomain;
  refKind: ResultReferenceKind | string;
  id: string;
  label?: string;
  contentHash?: string;
  externalUrl?: string;
}

export interface DenRouterMetadataProjection {
  kind: "den_router_metadata_projection.v1";
  adapterId: AdapterId;
  bindingId: string;
  runtime: ChannelRuntimeIdentity;
  providerRefs?: Partial<ChannelProviderRefs>;
  workRefs: WorkReference[];
  resultRefs?: ResultReference[];
  toolProfileKey?: string;
  mcpSurfaceRefs?: string[];
  status: ExternalBindingStatus;
  degradedReason?: string;
  observedAt: string;
  provenance: Record<string, unknown>;
}

export interface ChannelBindingRecord {
  bindingId: string;
  adapterId: AdapterId;
  provider: ChannelProvider | string;
  agentId: AgentId;
  instanceId?: AgentInstanceId;
  sessionId?: SessionId;
  profileId: ProfileId;
  externalChannelId: string;
  externalThreadId?: string;
  externalUserId?: string;
  conversationProjectId?: string;
  conversationChannelId?: number;
  providerSubscriptionId?: string;
  cursor?: string;
  membershipState?: string;
  presenceState?: string;
  status: ExternalBindingStatus;
  degradedReason?: string;
  createdAt?: string;
  updatedAt?: string;
}

export interface ChannelMembershipRecord {
  kind: "channel_membership.v1";
  bindingId: string;
  adapterId: AdapterId;
  providerRefs: ChannelProviderRefs;
  externalUserId: string;
  displayLabel?: string;
  agentId?: AgentId;
  profileId?: ProfileId;
  roleLabels: string[];
  status: ChannelMembershipStatus;
  observedAt: string;
  provenance: Record<string, unknown>;
}

export interface ChannelPresenceRecord {
  kind: "channel_presence.v1";
  bindingId: string;
  adapterId: AdapterId;
  providerRefs: ChannelProviderRefs;
  externalUserId?: string;
  agentId?: AgentId;
  sessionId?: SessionId;
  status: ChannelPresenceStatus;
  observedAt: string;
  expiresAt?: string;
  provenance: Record<string, unknown>;
}

export interface ChannelSubscriptionRecord {
  kind: "channel_subscription.v1";
  bindingId: string;
  adapterId: AdapterId;
  providerRefs: ChannelProviderRefs;
  transportKind: ChannelSubscriptionTransportKind;
  providerSubscriptionId?: string;
  rustSubscriptionHandle?: SubscriptionHandle;
  cursor?: string;
  status: ChannelSubscriptionStatus;
  lastConnectedAt?: string;
  lastSeenAt?: string;
  lastErrorAt?: string;
  degradedReason?: string;
  provenance: Record<string, unknown>;
}

export type McpTransportKind = "stdio" | "streamable_http" | "websocket";
export type McpSurfaceStatus =
  | "disconnected"
  | "connecting"
  | "active"
  | "degraded"
  | "archived";

export interface McpBindingDiagnostics {
  lastError?: string;
  lastCheckedAt?: string;
  notes?: string;
}

export interface McpBindingRecord {
  bindingId: string;
  adapterId: AdapterId;
  agentId: AgentId;
  instanceId?: AgentInstanceId;
  sessionId?: SessionId;
  profileId: ProfileId;
  serverNames: string[];
  endpointRef: string;
  transport: McpTransportKind | string;
  toolProfileKey: string;
  discoveredToolRevision?: string;
  status: ExternalBindingStatus;
  degradedReason?: string;
  diagnostics: McpBindingDiagnostics;
  createdAt?: string;
  updatedAt?: string;
}

export interface McpSurfaceIdentity {
  bindingId: string;
  adapterId: AdapterId;
  agentId: AgentId;
  instanceId?: AgentInstanceId;
  sessionId?: SessionId;
  profileId: ProfileId;
  serverNames: string[];
  toolProfileKey: string;
}

export interface McpSurfaceDiagnostics {
  bindingId: string;
  status: McpSurfaceStatus;
  transport: McpTransportKind | string;
  serverNames: string[];
  endpointRef: string;
  toolProfileKey: string;
  connectedAt?: string;
  lastCheckedAt?: string;
  lastError?: string;
  reconnectAttempts: number;
  optional: boolean;
}

export interface RuntimeBufferView {
  handle: RuntimeBufferHandle;
  mediaType: string;
  byteLen: number;
  bytes: Uint8Array;
}

export interface BrainModelConfig {
  /** Stable normalized selection identity. Absent only for compatibility imports. */
  modelConfigId?: string;
  modelConfigRevision?: number;
  endpointId?: string;
  endpointRevision?: number;
  credentialId?: string;
  credentialRevision?: number;
  authScheme?: "none" | "bearer_api_key" | "openai_codex_oauth";
  promptCacheTransport?: "none" | "openrouter_anthropic";
  provider: string;
  modelName: string;
  baseUrl?: string;
  api?: string;
  apiKeyEnv?: string;
  credentialKind?: "api_key" | "openai_oauth" | "legacy_raw_api_key";
  contextWindowTokens?: number;
  temperatureMilli?: number;
  maxOutputTokens?: number;
  reasoningEffort?: string;
  reasoningFormat?: string;
  responsesDialect?:
    | "openai_stateful"
    | "openai_stateless"
    | "generic_stateless"
    | "deepseek"
    | "meta";
  chatCompletionsDialect?: "standard" | "kimi" | "glm" | "qwen" | "deepseek";
  thinkingMode?: "provider_default" | "enabled" | "disabled";
  reasoningHistory?:
    | "provider_default"
    | "discard"
    | "preserve_all"
    | "tool_calls_only";
  reasoningBudgetTokens?: number;
  promptCaching?: "disabled" | "automatic_5m" | "automatic_1h";
  narratorImageInput?: {
    supported: boolean;
    maxImages: number;
    maxImageBytes: number;
    maxTotalBytes: number;
    reasonCode?: string;
  };
}

export interface BrainImplementationRegistration {
  implementationId: BrainImplementationId;
  profileId: ProfileId;
  toolProfile: ToolProfile;
  modelConfig: BrainModelConfig;
  strategy?: BrainStrategyMetadata;
  providerStateScope?: BrainProviderStateScope;
}

export type ScheduledJobStatus = "active" | "paused" | "archived";
export type ScheduledRunStatus =
  | "claimed"
  | "completed"
  | "skipped"
  | "failed"
  | "expired"
  | "cancelled";
export type ScheduledRunTrigger = "due" | "manual";

export interface ScheduledJobSummary {
  jobId: string;
  jobKind: string;
  targetSessionId?: SessionId;
  intervalMs?: number;
  nextDueAt?: string;
  status: ScheduledJobStatus;
  createdAt: string;
  updatedAt: string;
  pausedAt?: string;
}

export interface ScheduledRunSummary {
  runId: RunId;
  jobId: string;
  jobKind: string;
  targetSessionId?: SessionId;
  status: ScheduledRunStatus;
  trigger: ScheduledRunTrigger;
  scheduledFor?: string;
  claimedAt: string;
  claimDeadlineAt: string;
  completedAt?: string;
  error?: string;
  output?: unknown;
  createdAt: string;
  updatedAt: string;
}

export interface ScheduledJobListQuery {
  status?: ScheduledJobStatus;
  jobKind?: string;
  limit?: number;
  offset?: number;
}

export interface ScheduledRunListQuery {
  jobId?: string;
  status?: ScheduledRunStatus;
  trigger?: ScheduledRunTrigger;
  targetSessionId?: SessionId;
  limit?: number;
  offset?: number;
}

export interface ScheduledHostJobRegistrationInput {
  jobId: string;
  jobKind: string;
  intervalMs?: number;
  firstDueAt: string;
  payload?: unknown;
}

export interface ScheduledHostRunClaimQuery {
  supportedJobKinds: string[];
  limit?: number;
}

export interface ScheduledHostJobManualRunRequest {
  jobId: string;
  supportedJobKinds: string[];
}

export type ScheduledHostRunCompletionStatus =
  | "completed"
  | "skipped"
  | "failed"
  | "cancelled";

export interface ScheduledHostRunCompletionInput {
  runId: RunId;
  status: ScheduledHostRunCompletionStatus;
  output?: unknown;
  error?: string;
}

export interface SchedulerTickReport {
  staleRunsExpired: number;
  dueRunsClaimed: number;
  wakesRequested: number;
  runsCompleted: number;
  runsSkipped: number;
  runsFailed: number;
}

const mutableMemoryOperations = new Set<MemoryOperation>([
  "add",
  "replace",
  "merge",
  "supersede",
  "remove",
  "archive",
  "candidate_only",
]);

export function validateMemorySpaceDescriptor(
  descriptor: MemorySpaceDescriptor,
): string[] {
  const errors: string[] = [];
  validateIdentifier("memory space id", descriptor.space_id, errors);
  if (
    !Number.isInteger(descriptor.schema_version) ||
    descriptor.schema_version < 1
  ) {
    errors.push("memory space schema_version must be greater than zero");
  }
  if (descriptor.module_id != null) {
    validateIdentifier("memory module id", descriptor.module_id, errors);
  }
  if (descriptor.record_shapes.length === 0) {
    errors.push("memory space must declare at least one record shape");
  }
  for (const shape of descriptor.record_shapes) {
    validateIdentifier("memory record shape id", shape.shape_id, errors);
    if (!Number.isInteger(shape.version) || shape.version < 1) {
      errors.push(
        `memory record shape ${shape.shape_id} version must be greater than zero`,
      );
    }
    if (shape.fields.length === 0) {
      errors.push(
        `memory record shape ${shape.shape_id} must declare at least one field`,
      );
    }
    for (const field of shape.fields) {
      validateIdentifier("memory record field name", field.field_name, errors);
    }
  }
  if (descriptor.scope_model.allowed_scopes.length === 0) {
    errors.push("memory space must allow at least one scope type");
  }
  if (
    !descriptor.scope_model.allowed_scopes.includes(
      descriptor.scope_model.primary_scope,
    )
  ) {
    errors.push("memory space primary_scope must be in allowed_scopes");
  }
  if (descriptor.retrieval_strategies.length === 0) {
    errors.push("memory space must declare at least one retrieval strategy");
  }
  if (descriptor.operations.length === 0) {
    errors.push("memory space must declare at least one operation");
  }
  for (const policy of descriptor.write_policy.operation_policies) {
    if (!descriptor.operations.includes(policy.operation)) {
      errors.push(
        `memory operation policy references unsupported operation ${policy.operation}`,
      );
    }
    if (policy.min_confidence != null) {
      validateConfidence(policy.min_confidence, errors);
    }
  }
  return errors;
}

export function assertValidMemorySpaceDescriptor(
  descriptor: MemorySpaceDescriptor,
): void {
  const errors = validateMemorySpaceDescriptor(descriptor);
  if (errors.length > 0) {
    throw new Error(errors.join("; "));
  }
}

export function validateMemoryProposalEnvelope(
  proposal: MemoryProposalEnvelope,
  descriptor: MemorySpaceDescriptor,
): string[] {
  const errors = validateMemorySpaceDescriptor(descriptor);
  validateIdentifier("memory proposal id", proposal.proposal_id, errors);
  if (proposal.space_id !== descriptor.space_id) {
    errors.push("memory proposal space_id does not match descriptor");
  }
  if (!mutableMemoryOperations.has(proposal.operation)) {
    errors.push("memory proposal operation must mutate memory");
  }
  if (!descriptor.operations.includes(proposal.operation)) {
    errors.push("memory proposal operation is not supported by descriptor");
  }
  validateScopeId(proposal.scope.scope_id, errors);
  if (
    !descriptor.scope_model.allowed_scopes.includes(proposal.scope.scope_type)
  ) {
    errors.push("memory proposal scope_type is not supported by descriptor");
  }
  validateIdentifier("memory record shape id", proposal.shape.shape_id, errors);
  if (!Number.isInteger(proposal.shape.version) || proposal.shape.version < 1) {
    errors.push("memory proposal shape version must be greater than zero");
  }
  if (
    !descriptor.record_shapes.some(
      (shape) =>
        shape.shape_id === proposal.shape.shape_id &&
        shape.version === proposal.shape.version,
    )
  ) {
    errors.push("memory proposal shape is not declared by descriptor");
  }
  validateConfidence(proposal.confidence, errors);
  for (const evidence of proposal.evidence_refs) {
    if (evidence.ref_id.trim().length === 0) {
      errors.push("memory proposal evidence ref_id must not be empty");
    }
  }
  for (const required of descriptor.provenance_policy.required_evidence) {
    if (
      !proposal.evidence_refs.some(
        (evidence) => evidence.evidence_type === required,
      )
    ) {
      errors.push(`memory proposal missing required evidence ${required}`);
    }
  }
  if (
    descriptor.provenance_policy.rationale_required &&
    (proposal.durability_rationale?.trim().length ?? 0) === 0
  ) {
    errors.push("memory proposal durability_rationale is required");
  }
  return errors;
}

export function validateSessionActivityDigest(
  digest: SessionActivityDigest,
): string[] {
  const errors: string[] = [];
  validateIdentifier("session activity digest id", digest.digest_id, errors);
  validateScopeId(digest.wake_id, errors);
  if (digest.source.trim().length === 0) {
    errors.push("session activity digest source must not be empty");
  }
  if (digest.summary_text.trim().length === 0) {
    errors.push("session activity digest summary_text must not be empty");
  }
  for (const space of digest.allowed_capture_spaces) {
    validateIdentifier(
      "session activity digest allowed capture space",
      space,
      errors,
    );
  }
  return errors;
}

export function assertValidSessionActivityDigest(
  digest: SessionActivityDigest,
): void {
  const errors = validateSessionActivityDigest(digest);
  if (errors.length > 0) {
    throw new Error(errors.join("; "));
  }
}

export function assertValidMemoryProposalEnvelope(
  proposal: MemoryProposalEnvelope,
  descriptor: MemorySpaceDescriptor,
): void {
  const errors = validateMemoryProposalEnvelope(proposal, descriptor);
  if (errors.length > 0) {
    throw new Error(errors.join("; "));
  }
}

function validateIdentifier(
  label: string,
  value: string,
  errors: string[],
): void {
  if (value.length === 0) {
    errors.push(`${label} must not be empty`);
    return;
  }
  if (value.length > 64) {
    errors.push(`${label} must be at most 64 characters`);
  }
  if (!/^[a-z][a-z0-9_]*[a-z0-9]$/.test(value) || value.includes("__")) {
    errors.push(`${label} must use lowercase snake_case ASCII identifiers`);
  }
}

function validateScopeId(value: string, errors: string[]): void {
  if (value.trim().length === 0) {
    errors.push("memory scope_id must not be empty");
  }
  if (value.length > 256) {
    errors.push("memory scope_id must be at most 256 characters");
  }
  if (value.includes("\0")) {
    errors.push("memory scope_id must not contain NUL");
  }
}

function validateConfidence(value: number, errors: string[]): void {
  if (!Number.isFinite(value) || value < 0 || value > 1) {
    errors.push("memory confidence must be between 0 and 1");
  }
}
