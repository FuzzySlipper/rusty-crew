use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;
use thiserror::Error;

macro_rules! handle_type {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub u64);

        impl $name {
            pub const fn new(raw: u64) -> Self {
                Self(raw)
            }

            pub const fn get(self) -> u64 {
                self.0
            }
        }
    };
}

macro_rules! string_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            pub fn new(raw: impl Into<String>) -> Self {
                Self(raw.into())
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

handle_type!(EngineHandle);
handle_type!(SessionHandle);
handle_type!(BrainImplementationHandle);
handle_type!(PlatformAdapterHandle);
handle_type!(SubscriptionHandle);
handle_type!(RuntimeBufferHandle);

string_id!(AgentId);
string_id!(AgentInstanceId);
string_id!(SessionId);
string_id!(ProfileId);
string_id!(ProjectId);
string_id!(TaskId);
string_id!(RunId);
string_id!(AssignmentId);
string_id!(AdapterId);
string_id!(BrainImplementationId);
string_id!(MessageSlotId);
string_id!(MessageVariantId);
string_id!(MessageId);
string_id!(MessageBlockId);

pub type IsoTimestamp = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unit;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClockConfig {
    System,
    Fixed { at: IsoTimestamp },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineConfig {
    pub engine_data_dir: String,
    pub clock: ClockConfig,
    pub default_turn_budget: u32,
    pub default_idle_timeout_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShutdownRequest {
    pub engine: EngineHandle,
    pub drain_timeout_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShutdownSummary {
    pub engine: EngineHandle,
    pub archived_sessions: u32,
    pub dropped_subscriptions: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoreErrorKind {
    InvalidInput,
    NotFound,
    AlreadyExists,
    SessionExpired,
    TimeoutExpired,
    PersistenceFailure,
    AdapterUnavailable,
    BrainUnavailable,
    ActionRejected,
    InternalError,
}

#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize)]
#[error("{kind:?}: {message}")]
pub struct CoreError {
    pub kind: CoreErrorKind,
    pub message: String,
}

impl CoreError {
    pub fn new(kind: CoreErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

pub type CoreResult<T> = Result<T, CoreError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionKind {
    Full,
    Worker,
    Delegated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Active,
    Idle,
    Archived,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DurableAgentKind {
    Prime,
    Full,
    Delegated,
    WorkerPoolWorker,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DurableIdentityStatus {
    Active,
    Archived,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSystemReference {
    pub system: String,
    pub external_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DenRuntimeReference {
    pub project_id: Option<ProjectId>,
    pub task_id: Option<TaskId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DurableAgentRecord {
    pub agent_id: AgentId,
    pub display_label: String,
    pub profile_id: ProfileId,
    pub kind: DurableAgentKind,
    pub status: DurableIdentityStatus,
    pub source: Option<SourceSystemReference>,
    pub den: DenRuntimeReference,
    pub created_at: IsoTimestamp,
    pub archived_at: Option<IsoTimestamp>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentInstanceRecord {
    pub instance_id: AgentInstanceId,
    pub agent_id: AgentId,
    pub display_label: String,
    pub profile_id: ProfileId,
    pub status: DurableIdentityStatus,
    pub source: Option<SourceSystemReference>,
    pub den: DenRuntimeReference,
    pub created_at: IsoTimestamp,
    pub last_active_at: IsoTimestamp,
    pub archived_at: Option<IsoTimestamp>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionIdentityRecord {
    pub session_id: SessionId,
    pub instance_id: AgentInstanceId,
    pub agent_id: AgentId,
    pub profile_id: ProfileId,
    pub kind: SessionKind,
    pub status: SessionStatus,
    pub source: Option<SourceSystemReference>,
    pub den: DenRuntimeReference,
    pub created_at: IsoTimestamp,
    pub last_active_at: IsoTimestamp,
    pub archived_at: Option<IsoTimestamp>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub workdir: Option<String>,
    pub max_duration_ms: Option<u32>,
    pub max_delegation_depth: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelegationLineage {
    pub parent_session_id: SessionId,
    pub parent_agent_id: AgentId,
    pub source_wake_id: String,
    pub source_action_index: u32,
    pub requested_task_id: Option<TaskId>,
    pub correlation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DelegationPriority {
    Low,
    Normal,
    High,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParentConsumptionPolicy {
    AwaitCompletion,
    ObserveOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FanOutFailurePolicy {
    FailFast,
    FailSoft,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolDescriptor {
    pub name: String,
    pub description: String,
    pub input_schema: Option<RuntimeBufferHandle>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolProfile {
    pub tools: Vec<ToolDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallSource {
    Local,
    Mcp,
    Web,
    Browser,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallPolicyMetadata {
    pub allowed: Option<bool>,
    pub denial_reason: Option<String>,
    pub timeout_ms: Option<u32>,
    pub cancelled: Option<bool>,
    pub archive_cleanup: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallMetadata {
    pub source: ToolCallSource,
    pub adapter_id: Option<AdapterId>,
    pub binding_id: Option<String>,
    pub server_names: Vec<String>,
    pub profile_id: Option<ProfileId>,
    pub tool_profile_key: Option<String>,
    pub source_tool_name: Option<String>,
    pub catalog_revision: Option<String>,
    pub policy: Option<ToolCallPolicyMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionConfig {
    pub session_id: SessionId,
    pub agent_id: AgentId,
    pub profile_id: ProfileId,
    pub kind: SessionKind,
    pub delegation: Option<DelegationLineage>,
    pub resource_limits: ResourceLimits,
    pub tool_profile: ToolProfile,
    pub history_window: Option<SessionHistoryWindow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionState {
    pub handle: SessionHandle,
    pub session_id: SessionId,
    pub agent_id: AgentId,
    pub profile_id: ProfileId,
    pub kind: SessionKind,
    pub delegation: Option<DelegationLineage>,
    pub resource_limits: ResourceLimits,
    pub tool_profile: ToolProfile,
    pub history_window: Option<SessionHistoryWindow>,
    pub status: SessionStatus,
    pub brain_turn_count: u32,
    pub created_at: IsoTimestamp,
    pub last_active_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionHistoryWindow {
    pub max_messages: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentMessage {
    pub from: AgentId,
    pub to: AgentId,
    pub body: String,
    pub correlation_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSubscription {
    pub event_kinds: Vec<CoreEventKind>,
    pub session_id: Option<SessionId>,
    pub agent_id: Option<AgentId>,
    pub adapter_id: Option<AdapterId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoreEventKind {
    SessionCreated,
    SessionArchived,
    AgentMessageRouted,
    DelegationLifecycleObserved,
    ExternalEventInjected,
    DenDataUpdated,
    BrainWakeRequested,
    BrainEventObserved,
    BrainActionsAccepted,
    CompletionPacketDelivered,
}

impl CoreEventKind {
    pub const fn of(event: &CoreEvent) -> Self {
        match event {
            CoreEvent::SessionCreated { .. } => Self::SessionCreated,
            CoreEvent::SessionArchived { .. } => Self::SessionArchived,
            CoreEvent::AgentMessageRouted { .. } => Self::AgentMessageRouted,
            CoreEvent::DelegationLifecycleObserved { .. } => Self::DelegationLifecycleObserved,
            CoreEvent::ExternalEventInjected { .. } => Self::ExternalEventInjected,
            CoreEvent::DenDataUpdated { .. } => Self::DenDataUpdated,
            CoreEvent::BrainWakeRequested { .. } => Self::BrainWakeRequested,
            CoreEvent::BrainEventObserved { .. } => Self::BrainEventObserved,
            CoreEvent::BrainActionsAccepted { .. } => Self::BrainActionsAccepted,
            CoreEvent::CompletionPacketDelivered { .. } => Self::CompletionPacketDelivered,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DenDataUpdate {
    pub project_id: ProjectId,
    pub entity_kind: String,
    pub entity_id: String,
    pub revision: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalEvent {
    pub adapter_id: AdapterId,
    pub source: String,
    pub payload: ExternalEventPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExternalEventPayload {
    HumanMessage {
        from: String,
        text: String,
    },
    ChannelMessage(Box<ChannelMessageExternalPayload>),
    AdapterStatus {
        status: String,
        detail: Option<String>,
    },
    ToolCatalogChanged {
        catalog_id: String,
    },
    RawJson {
        json: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelMessageExternalPayload {
    pub binding_id: String,
    pub correlation_id: String,
    pub idempotency_key: String,
    pub provider: String,
    pub external_channel_id: String,
    pub external_thread_id: Option<String>,
    pub external_message_id: Option<String>,
    pub from: String,
    pub text: String,
    pub received_at: IsoTimestamp,
    pub expires_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionPacket {
    pub session_id: SessionId,
    pub status: CompletionStatus,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionStatus {
    Completed,
    Failed,
    Blocked,
    Exhausted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelegatedCompletion {
    pub run_id: RunId,
    pub child_session_id: SessionId,
    pub requested_task_id: Option<TaskId>,
    pub source_wake_id: String,
    pub source_action_index: u32,
    pub correlation_id: Option<String>,
    pub parent_consumption: ParentConsumptionPolicy,
    pub packet: CompletionPacket,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelegatedFanOutGroup {
    pub group_id: String,
    pub total: u32,
    pub pending: u32,
    pub completed: u32,
    pub failed: u32,
    pub blocked: u32,
    pub exhausted: u32,
    pub cancelled: u32,
    pub expired: u32,
    pub max_concurrency: Option<u32>,
    pub failure_policy: FanOutFailurePolicy,
    pub status: FanOutGroupStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FanOutGroupStatus {
    InProgress,
    Completed,
    PartialFailure,
    FailedFast,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DelegationLifecyclePhase {
    Created,
    WakeRequested,
    CheckpointRequested,
    Completed,
    Failed,
    Blocked,
    Exhausted,
    TimedOut,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelegationLifecycleEvent {
    pub parent_session_id: SessionId,
    pub delegated_session_id: SessionId,
    pub run_id: Option<RunId>,
    pub phase: DelegationLifecyclePhase,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DelegatedRunStatus {
    Requested,
    SessionCreated,
    WakeRequested,
    Running,
    CheckpointWaiting,
    Completed,
    Failed,
    Blocked,
    Exhausted,
    Cancelled,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelegatedSessionRuntimeStatus {
    pub session: SessionState,
    pub parent_session_id: Option<SessionId>,
    pub run_id: Option<RunId>,
    pub run_status: Option<DelegatedRunStatus>,
    pub terminal: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelegatedResourceCleanupReport {
    pub cleaned_at: IsoTimestamp,
    pub terminal_archived: Vec<SessionId>,
    pub orphaned_archived: Vec<SessionId>,
    pub expired_archived: Vec<SessionId>,
    pub resources_released: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CoreEvent {
    SessionCreated {
        state: Box<SessionState>,
    },
    SessionArchived {
        session_id: SessionId,
    },
    AgentMessageRouted {
        message: AgentMessage,
    },
    DelegationLifecycleObserved {
        lifecycle: DelegationLifecycleEvent,
    },
    ExternalEventInjected {
        event: ExternalEvent,
    },
    DenDataUpdated {
        update: DenDataUpdate,
    },
    BrainWakeRequested {
        session_id: SessionId,
    },
    BrainEventObserved {
        session_id: SessionId,
        wake_id: Option<String>,
        event: BrainEvent,
    },
    BrainActionsAccepted {
        session_id: SessionId,
        count: u32,
    },
    CompletionPacketDelivered {
        packet: CompletionPacket,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BodyState {
    pub session: SessionState,
    pub pending_messages: Vec<AgentMessage>,
    pub recent_events: Vec<CoreEvent>,
    pub child_completions: Vec<DelegatedCompletion>,
    pub fan_out_groups: Vec<DelegatedFanOutGroup>,
    pub delta_policy: BodyDeltaPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BodyDeltaPolicy {
    pub mode: MidTurnDeltaMode,
    pub queue_owner: DeltaQueueOwner,
    pub queued_message_ttl_ms: u32,
    pub max_queued_messages: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MidTurnDeltaMode {
    FrozenSnapshotNextWake,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeltaQueueOwner {
    Body,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrainWakeRequest {
    pub brain: BrainImplementationHandle,
    pub session_id: SessionId,
    pub body_state: RuntimeBufferHandle,
    pub system_prompt: RuntimeBufferHandle,
    pub role_assembly: RuntimeBufferHandle,
    pub wake_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_state: Option<BrainWakeProviderStateInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_state_absence: Option<ProviderStateAbsenceReason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrainWakeAccepted {
    pub wake_id: String,
    pub accepted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BrainEvent {
    Started,
    TextDelta {
        text: String,
    },
    ToolCallStarted {
        tool_name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        metadata: Option<ToolCallMetadata>,
    },
    ToolCallFinished {
        tool_name: String,
        is_error: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        metadata: Option<ToolCallMetadata>,
    },
    ProviderStatus {
        level: BrainProviderStatusLevel,
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        metadata_json: Option<String>,
    },
    Finished,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrainProviderStatusLevel {
    Info,
    Degraded,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrainEventEnvelope {
    pub wake_id: String,
    pub session_id: SessionId,
    pub event: BrainEvent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BrainAction {
    SendMessage {
        message: AgentMessage,
    },
    RequestDelegation {
        profile_id: ProfileId,
        task_id: Option<TaskId>,
        prompt: String,
        expected_output: Option<String>,
        resource_limits: Option<ResourceLimits>,
        timeout_ms: Option<u32>,
        priority: Option<DelegationPriority>,
        fan_out_group_id: Option<String>,
        fan_out_max_concurrency: Option<u32>,
        fan_out_failure_policy: Option<FanOutFailurePolicy>,
        correlation_id: Option<String>,
        parent_consumption: Option<ParentConsumptionPolicy>,
    },
    DeliverCompletion {
        packet: CompletionPacket,
    },
}

impl BrainAction {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::SendMessage { .. } => "send_message",
            Self::RequestDelegation { .. } => "request_delegation",
            Self::DeliverCompletion { .. } => "deliver_completion",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrainActionBatch {
    pub wake_id: String,
    pub session_id: SessionId,
    pub actions: Vec<BrainAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderStateMode {
    Unused,
    Optional,
    Required,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrainProviderStateStrategyMetadata {
    pub mode: ProviderStateMode,
}

impl BrainProviderStateStrategyMetadata {
    pub const fn unused() -> Self {
        Self {
            mode: ProviderStateMode::Unused,
        }
    }
}

impl Default for BrainProviderStateStrategyMetadata {
    fn default() -> Self {
        Self::unused()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrainStrategyMetadata {
    pub module_id: String,
    pub strategy_id: String,
    #[serde(default)]
    pub provider_state: BrainProviderStateStrategyMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrainProviderStateScope {
    pub profile_fingerprint: String,
    pub provider_fingerprint: String,
}

impl BrainStrategyMetadata {
    pub fn unused(module_id: impl Into<String>, strategy_id: impl Into<String>) -> Self {
        Self {
            module_id: module_id.into(),
            strategy_id: strategy_id.into(),
            provider_state: BrainProviderStateStrategyMetadata::unused(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderStateAbsenceReason {
    NotConfigured,
    Missing,
    Expired,
    Invalidated,
    ModuleDoesNotUseState,
    LoadFailed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrainWakeProviderStateInput {
    pub module_id: String,
    pub strategy_id: String,
    pub profile_fingerprint: String,
    pub provider_fingerprint: String,
    pub payload_version: String,
    pub payload: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<IsoTimestamp>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrainWakeProviderStateUpdate {
    pub module_id: String,
    pub strategy_id: String,
    pub profile_fingerprint: String,
    pub provider_fingerprint: String,
    pub payload_version: String,
    pub payload: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BrainWakeProviderStateOutput {
    Unchanged,
    Replace { state: BrainWakeProviderStateUpdate },
    Clear { reason: ProviderStateClearReason },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderStateClearReason {
    BrainRequestedClear,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrainWakeFailure {
    pub wake_id: String,
    pub session_id: SessionId,
    pub kind: CoreErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum BrainWakeStreamItem {
    Event { event: BrainEventEnvelope },
    Actions { batch: BrainActionBatch },
    WakeFailed { failure: BrainWakeFailure },
}

impl BrainWakeStreamItem {
    pub fn event(event: BrainEventEnvelope) -> Self {
        Self::Event { event }
    }

    pub fn actions(batch: BrainActionBatch) -> Self {
        Self::Actions { batch }
    }

    pub fn wake_failed(failure: BrainWakeFailure) -> Self {
        Self::WakeFailed { failure }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Actions { .. } | Self::WakeFailed { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionBatchReceipt {
    pub wake_id: String,
    pub accepted_actions: u32,
    pub rejected_actions: Vec<ActionRejection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionRejection {
    pub index: u32,
    pub kind: CoreErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventReceipt {
    pub accepted: bool,
    pub sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeBufferView {
    pub handle: RuntimeBufferHandle,
    pub media_type: String,
    pub byte_len: u64,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrainImplementationRegistration {
    pub implementation_id: BrainImplementationId,
    pub profile_id: ProfileId,
    pub tool_profile: ToolProfile,
    pub model_config: BrainModelConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy: Option<BrainStrategyMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_state_scope: Option<BrainProviderStateScope>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrainModelConfig {
    pub provider: String,
    pub model_name: String,
    pub temperature_milli: Option<u32>,
    pub max_output_tokens: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformAdapterRegistration {
    pub adapter_id: AdapterId,
    pub kind: PlatformAdapterKind,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformAdapterKind {
    Den,
    Telegram,
    Mcp,
    Tui,
    Cli,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_external_event_payload_keeps_flat_tagged_json_shape() {
        let payload =
            ExternalEventPayload::ChannelMessage(Box::new(ChannelMessageExternalPayload {
                binding_id: "binding-alpha".to_string(),
                correlation_id: "channel:binding-alpha:message-1".to_string(),
                idempotency_key: "den_channels:crew-room:thread-1:message-1".to_string(),
                provider: "den_channels".to_string(),
                external_channel_id: "crew-room".to_string(),
                external_thread_id: Some("thread-1".to_string()),
                external_message_id: Some("message-1".to_string()),
                from: "den-user-alpha".to_string(),
                text: "hello".to_string(),
                received_at: "2026-06-20T05:01:00.000Z".to_string(),
                expires_at: "2026-06-20T05:01:05.000Z".to_string(),
            }));

        let json = serde_json::to_value(&payload).expect("serialize payload");
        assert_eq!(json["type"], "channel_message");
        assert_eq!(json["binding_id"], "binding-alpha");
        assert_eq!(json["correlation_id"], "channel:binding-alpha:message-1");

        let round_trip: ExternalEventPayload =
            serde_json::from_value(json).expect("deserialize payload");
        assert_eq!(round_trip, payload);
    }

    #[test]
    fn brain_wake_stream_items_keep_flat_tagged_json_shape() {
        let item = BrainWakeStreamItem::event(BrainEventEnvelope {
            wake_id: "wake-1".to_string(),
            session_id: SessionId::new("session-1"),
            event: BrainEvent::ProviderStatus {
                level: BrainProviderStatusLevel::Degraded,
                message: "provider retrying".to_string(),
                metadata_json: Some("{\"attempt\":1}".to_string()),
            },
        });

        let json = serde_json::to_value(&item).expect("serialize stream item");
        assert_eq!(json["type"], "event");
        assert_eq!(json["event"]["event"]["type"], "provider_status");
        assert_eq!(json["event"]["event"]["level"], "degraded");
        assert!(!item.is_terminal());

        let round_trip: BrainWakeStreamItem =
            serde_json::from_value(json).expect("deserialize stream item");
        assert_eq!(round_trip, item);

        let terminal = BrainWakeStreamItem::actions(BrainActionBatch {
            wake_id: "wake-1".to_string(),
            session_id: SessionId::new("session-1"),
            actions: Vec::new(),
        });
        assert!(terminal.is_terminal());
    }
}
