export type Brand<T, Name extends string> = T & { readonly __brand: Name };

export type EngineHandle = Brand<number, "EngineHandle">;
export type SessionHandle = Brand<number, "SessionHandle">;
export type BrainImplementationHandle = Brand<
  number,
  "BrainImplementationHandle"
>;
export type PlatformAdapterHandle = Brand<number, "PlatformAdapterHandle">;
export type SubscriptionHandle = Brand<number, "SubscriptionHandle">;
export type RuntimeBufferHandle = Brand<number, "RuntimeBufferHandle">;

export type AgentId = Brand<string, "AgentId">;
export type SessionId = Brand<string, "SessionId">;
export type ProfileId = Brand<string, "ProfileId">;
export type ProjectId = Brand<string, "ProjectId">;
export type TaskId = Brand<string, "TaskId">;
export type RunId = Brand<string, "RunId">;
export type AdapterId = Brand<string, "AdapterId">;
export type BrainImplementationId = Brand<string, "BrainImplementationId">;

export interface Unit {}

export type ClockConfig = "system" | { fixed: string };

export interface EngineConfig {
  engineDataDir: string;
  clock: ClockConfig;
  defaultTurnBudget: number;
  defaultIdleTimeoutMs: number;
}

export interface ShutdownRequest {
  engine: EngineHandle;
  drainTimeoutMs: number;
}

export interface ShutdownSummary {
  archivedSessions: number;
  droppedSubscriptions: number;
}

export type SessionKind = "full" | "worker" | "delegated";
export type SessionStatus = "active" | "idle" | "archived";

export interface ToolDescriptor {
  name: string;
  description: string;
  inputSchema?: RuntimeBufferHandle;
}

export interface ToolProfile {
  tools: ToolDescriptor[];
}

export interface ResourceLimits {
  workdir?: string;
  maxDurationMs?: number;
  maxDelegationDepth?: number;
}

export interface DelegationLineage {
  parentSessionId: SessionId;
  parentAgentId: AgentId;
  sourceWakeId: string;
  sourceActionIndex: number;
  requestedTaskId?: TaskId;
  correlationId: string;
}

export interface SessionConfig {
  sessionId: SessionId;
  agentId: AgentId;
  profileId: ProfileId;
  kind: SessionKind;
  delegation?: DelegationLineage;
  resourceLimits: ResourceLimits;
  toolProfile: ToolProfile;
}

export interface SessionState extends SessionConfig {
  handle: SessionHandle;
  status: SessionStatus;
  brainTurnCount: number;
  createdAt: string;
  lastActiveAt: string;
}

export interface AgentMessage {
  from: AgentId;
  to: AgentId;
  body: string;
  correlationId?: string;
}

export type CoreEventKind =
  | "session_created"
  | "session_archived"
  | "agent_message_routed"
  | "delegation_lifecycle_observed"
  | "external_event_injected"
  | "den_data_updated"
  | "brain_wake_requested"
  | "brain_event_observed"
  | "brain_actions_accepted"
  | "completion_packet_delivered";

export interface EventSubscription {
  eventKinds: CoreEventKind[];
  sessionId?: SessionId;
  agentId?: AgentId;
  adapterId?: AdapterId;
}

export interface DenDataUpdate {
  projectId: ProjectId;
  entityKind: string;
  entityId: string;
  revision?: string;
}

export type ExternalEventPayload =
  | { type: "human_message"; from: string; text: string }
  | { type: "adapter_status"; status: string; detail?: string }
  | { type: "tool_catalog_changed"; catalogId: string }
  | { type: "raw_json"; json: string };

export interface ExternalEvent {
  adapterId: AdapterId;
  source: string;
  payload: ExternalEventPayload;
}

export type CompletionStatus = "completed" | "failed" | "blocked" | "exhausted";

export interface CompletionPacket {
  sessionId: SessionId;
  status: CompletionStatus;
  summary: string;
}

export type ParentConsumptionPolicy = "await_completion" | "observe_only";
export type FanOutFailurePolicy = "fail_fast" | "fail_soft";
export type DelegationLifecyclePhase =
  | "created"
  | "wake_requested"
  | "checkpoint_requested"
  | "completed"
  | "failed"
  | "blocked"
  | "exhausted"
  | "timed_out"
  | "cancelled";
export type DelegatedRunStatus =
  | "requested"
  | "session_created"
  | "wake_requested"
  | "running"
  | "checkpoint_waiting"
  | "completed"
  | "failed"
  | "blocked"
  | "exhausted"
  | "cancelled"
  | "expired";
export type FanOutGroupStatus =
  | "in_progress"
  | "completed"
  | "partial_failure"
  | "failed_fast";

export interface DelegatedCompletion {
  runId: RunId;
  childSessionId: SessionId;
  requestedTaskId?: TaskId;
  sourceWakeId: string;
  sourceActionIndex: number;
  correlationId?: string;
  parentConsumption: ParentConsumptionPolicy;
  packet: CompletionPacket;
}

export interface DelegationLifecycleEvent {
  parentSessionId: SessionId;
  delegatedSessionId: SessionId;
  runId?: RunId;
  phase: DelegationLifecyclePhase;
  detail?: string;
}

export interface DelegatedSessionRuntimeStatus {
  session: SessionState;
  parentSessionId?: SessionId;
  runId?: RunId;
  runStatus?: DelegatedRunStatus;
  terminal: boolean;
}

export interface DelegatedFanOutGroup {
  groupId: string;
  total: number;
  pending: number;
  completed: number;
  failed: number;
  blocked: number;
  exhausted: number;
  cancelled: number;
  expired: number;
  maxConcurrency?: number;
  failurePolicy: FanOutFailurePolicy;
  status: FanOutGroupStatus;
}

export type BrainEvent =
  | { type: "started" }
  | { type: "text_delta"; text: string }
  | { type: "tool_call_started"; toolName: string }
  | { type: "tool_call_finished"; toolName: string; isError: boolean }
  | { type: "finished" };

export type BrainAction =
  | { type: "send_message"; message: AgentMessage }
  | {
      type: "request_delegation";
      profileId: ProfileId;
      taskId?: TaskId;
      prompt: string;
      expectedOutput?: string;
      resourceLimits?: ResourceLimits;
      timeoutMs?: number;
      priority?: "low" | "normal" | "high";
      fanOutGroupId?: string;
      fanOutMaxConcurrency?: number;
      fanOutFailurePolicy?: FanOutFailurePolicy;
      correlationId?: string;
      parentConsumption?: ParentConsumptionPolicy;
    }
  | { type: "deliver_completion"; packet: CompletionPacket };

export interface BrainWakeRequest {
  brain: BrainImplementationHandle;
  sessionId: SessionId;
  bodyState: RuntimeBufferHandle;
  systemPrompt: RuntimeBufferHandle;
  roleAssembly: RuntimeBufferHandle;
  wakeId: string;
}

export interface BrainWakeAccepted {
  wakeId: string;
  accepted: boolean;
}

export interface BrainEventEnvelope {
  wakeId: string;
  sessionId: SessionId;
  event: BrainEvent;
}

export interface BrainActionBatch {
  wakeId: string;
  sessionId: SessionId;
  actions: BrainAction[];
}

export interface ActionRejection {
  index: number;
  kind: CoreErrorKind;
  message: string;
}

export interface ActionBatchReceipt {
  wakeId: string;
  acceptedActions: number;
  rejectedActions: ActionRejection[];
}

export interface EventReceipt {
  accepted: boolean;
  sequence: number;
}

export interface RuntimeBufferView {
  handle: RuntimeBufferHandle;
  mediaType: string;
  byteLen: number;
  bytes: Uint8Array;
}

export interface BodyDeltaPolicy {
  mode: "frozen_snapshot_next_wake";
  queueOwner: "body";
  queuedMessageTtlMs: number;
  maxQueuedMessages: number;
}

export interface BodyState {
  session: SessionState;
  pendingMessages: AgentMessage[];
  recentEvents: CoreEvent[];
  childCompletions: DelegatedCompletion[];
  fanOutGroups: DelegatedFanOutGroup[];
  deltaPolicy: BodyDeltaPolicy;
}

export type CoreEvent =
  | { type: "session_created"; state: SessionState }
  | { type: "session_archived"; sessionId: SessionId }
  | { type: "agent_message_routed"; message: AgentMessage }
  | {
      type: "delegation_lifecycle_observed";
      lifecycle: DelegationLifecycleEvent;
    }
  | { type: "external_event_injected"; event: ExternalEvent }
  | { type: "den_data_updated"; update: DenDataUpdate }
  | { type: "brain_wake_requested"; sessionId: SessionId }
  | {
      type: "brain_event_observed";
      sessionId: SessionId;
      wakeId?: string;
      event: BrainEvent;
    }
  | { type: "brain_actions_accepted"; sessionId: SessionId; count: number }
  | { type: "completion_packet_delivered"; packet: CompletionPacket };

export type CoreErrorKind =
  | "invalid_input"
  | "not_found"
  | "already_exists"
  | "session_expired"
  | "timeout_expired"
  | "persistence_failure"
  | "adapter_unavailable"
  | "brain_unavailable"
  | "action_rejected"
  | "internal_error";

export interface CoreError {
  kind: CoreErrorKind;
  message: string;
}

export interface BrainModelConfig {
  provider: string;
  modelName: string;
  temperatureMilli?: number;
  maxOutputTokens?: number;
}

export interface BrainImplementationRegistration {
  implementationId: BrainImplementationId;
  profileId: ProfileId;
  toolProfile: ToolProfile;
  modelConfig: BrainModelConfig;
}

export type PlatformAdapterKind = "den" | "telegram" | "mcp" | "tui" | "cli";

export interface PlatformAdapterRegistration {
  adapterId: AdapterId;
  kind: PlatformAdapterKind;
  displayName: string;
}

export const manifestOperationNames = [
  "initialize_engine",
  "shutdown_engine",
  "register_brain_implementation",
  "wake_brain",
  "submit_brain_event",
  "submit_brain_actions",
  "register_platform_adapter",
  "inject_external_event",
  "inject_den_data_update",
  "cancel_delegated_session",
  "request_delegated_checkpoint",
  "drain_delegated_sessions",
  "delegated_session_status",
  "subscribe_events",
  "unsubscribe_events",
  "get_buffer",
  "release_buffer",
] as const;

export type ManifestOperationName = (typeof manifestOperationNames)[number];
