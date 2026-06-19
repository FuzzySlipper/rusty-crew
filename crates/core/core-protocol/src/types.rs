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

pub type IsoTimestamp = String;

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
pub struct ToolDescriptor {
    pub name: String,
    pub description: String,
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
    pub payload_json: String,
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
        state: SessionState,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrainWakeRequest {
    pub session_id: SessionId,
    pub state: BodyState,
    pub system_prompt: RuntimeBufferHandle,
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
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BrainAction {
    SendMessage {
        message: AgentMessage,
    },
    RequestDelegation {
        profile_id: ProfileId,
        task_id: Option<TaskId>,
        prompt: String,
    },
    DeliverCompletion {
        packet: CompletionPacket,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrainActionBatch {
    pub session_id: SessionId,
    pub actions: Vec<BrainAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformAdapterRegistration {
    pub adapter_id: AdapterId,
    pub kind: PlatformAdapterKind,
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
