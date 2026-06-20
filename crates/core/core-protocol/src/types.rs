use serde::{Deserialize, Serialize};
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
string_id!(SessionId);
string_id!(ProfileId);
string_id!(ProjectId);
string_id!(TaskId);
string_id!(RunId);
string_id!(AssignmentId);
string_id!(AdapterId);
string_id!(BrainImplementationId);

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
pub struct SessionConfig {
    pub session_id: SessionId,
    pub agent_id: AgentId,
    pub profile_id: ProfileId,
    pub kind: SessionKind,
    pub delegation: Option<DelegationLineage>,
    pub resource_limits: ResourceLimits,
    pub tool_profile: ToolProfile,
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
    pub status: SessionStatus,
    pub brain_turn_count: u32,
    pub created_at: IsoTimestamp,
    pub last_active_at: IsoTimestamp,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrainWakeRequest {
    pub brain: BrainImplementationHandle,
    pub session_id: SessionId,
    pub body_state: RuntimeBufferHandle,
    pub system_prompt: RuntimeBufferHandle,
    pub role_assembly: RuntimeBufferHandle,
    pub wake_id: String,
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
    TextDelta { text: String },
    ToolCallStarted { tool_name: String },
    ToolCallFinished { tool_name: String, is_error: bool },
    Finished,
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
