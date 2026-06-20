//! Local coordination-state persistence.
//!
//! This store is for Rust-owned coordination state only. Den task, project, and
//! document data remains Den product data and is not mirrored here.

use rusqlite::{params, Connection, OptionalExtension};
use rusty_crew_core_protocol::{
    AdapterId, AgentId, AgentInstanceId, AgentInstanceRecord, AgentMessage, BrainEvent,
    CompletionPacket, CoreError, CoreErrorKind, CoreEvent, CoreEventKind, CoreResult,
    DelegatedCompletion, DelegatedFanOutGroup, DelegationLineage, DenRuntimeReference,
    DurableAgentKind, DurableAgentRecord, DurableIdentityStatus, FanOutFailurePolicy,
    FanOutGroupStatus, IsoTimestamp, ParentConsumptionPolicy, ProfileId, ProjectId, ResourceLimits,
    RunId, SessionConfig, SessionHandle, SessionId, SessionIdentityRecord, SessionKind,
    SessionState, SessionStatus, SourceSystemReference, TaskId, ToolCallMetadata, ToolProfile,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const DB_FILE_NAME: &str = "coordination.sqlite3";
const CURRENT_SCHEMA_VERSION: i64 = 14;
const MIN_SUPPORTED_SCHEMA_VERSION: i64 = 1;
const SQLITE_BUSY_TIMEOUT_MS: u64 = 5_000;
const SQLITE_WAL_AUTOCHECKPOINT_PAGES: u32 = 1_000;
const COUNTER_BRAIN_TURNS: &str = "brain_turns";
const COUNTER_WAKES: &str = "wakes";
const COUNTER_TOOL_CALLS: &str = "tool_calls";
const COUNTER_TOOL_ERRORS: &str = "tool_errors";
const COUNTER_DELEGATIONS_CREATED: &str = "delegations_created";
const COUNTER_DELEGATIONS_COMPLETED: &str = "delegations_completed";
const COUNTER_DELEGATIONS_FAILED: &str = "delegations_failed";
const COUNTER_DELEGATIONS_TIMED_OUT: &str = "delegations_timed_out";
const COUNTER_DELEGATIONS_CANCELLED: &str = "delegations_cancelled";
const COUNTER_MESSAGES: &str = "messages";
const COUNTER_COMPLETIONS: &str = "completions";
const COUNTER_QUEUE_EXPIRATIONS: &str = "queue_expirations";

struct SchemaMigration {
    version: i64,
    description: &'static str,
    apply: fn(&rusqlite::Transaction<'_>) -> CoreResult<()>,
}

const SCHEMA_MIGRATIONS: &[SchemaMigration] = &[
    SchemaMigration {
        version: 1,
        description: "create base coordination tables",
        apply: migrate_v1_create_base_tables,
    },
    SchemaMigration {
        version: 2,
        description: "add delegation and fan-out coordination columns",
        apply: migrate_v2_add_delegation_columns,
    },
    SchemaMigration {
        version: 3,
        description: "add durable agent, instance, and session identity tables",
        apply: migrate_v3_add_identity_tables,
    },
    SchemaMigration {
        version: 4,
        description: "add immutable session configuration snapshots",
        apply: migrate_v4_add_session_config_snapshots,
    },
    SchemaMigration {
        version: 5,
        description: "add event-log query projection indexes",
        apply: migrate_v5_add_event_projection_indexes,
    },
    SchemaMigration {
        version: 6,
        description: "add FTS runtime search index",
        apply: migrate_v6_add_runtime_search_index,
    },
    SchemaMigration {
        version: 7,
        description: "add durable runtime counters",
        apply: migrate_v7_add_runtime_counters,
    },
    SchemaMigration {
        version: 8,
        description: "add queued message retention state",
        apply: migrate_v8_add_queued_message_retention,
    },
    SchemaMigration {
        version: 9,
        description: "add scale guardrail indexes for runtime diagnostics",
        apply: migrate_v9_add_scale_guardrail_indexes,
    },
    SchemaMigration {
        version: 10,
        description: "add future legacy runtime import metadata",
        apply: migrate_v10_add_legacy_runtime_import_metadata,
    },
    SchemaMigration {
        version: 11,
        description: "add per-agent external channel and MCP bindings",
        apply: migrate_v11_add_external_bindings,
    },
    SchemaMigration {
        version: 12,
        description: "add tool call metadata audit column",
        apply: migrate_v12_add_tool_call_metadata,
    },
    SchemaMigration {
        version: 13,
        description: "add dense profile memory persistence",
        apply: migrate_v13_add_profile_memory,
    },
    SchemaMigration {
        version: 14,
        description: "add scheduler job and run persistence",
        apply: migrate_v14_add_scheduler_persistence,
    },
];

#[derive(Debug, Clone)]
pub struct CoordinationStore {
    conn: Arc<Mutex<Connection>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaMigrationRecord {
    pub version: i64,
    pub description: String,
    pub applied_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionConfigRecord {
    pub session_id: SessionId,
    pub profile_id: ProfileId,
    pub kind: SessionKind,
    pub resource_limits: ResourceLimits,
    pub tool_profile: ToolProfile,
    pub config: SessionConfig,
    pub created_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedEvent {
    pub sequence: u64,
    pub event: CoreEvent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryPage {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

impl QueryPage {
    fn bounded(self, default_limit: u32, max_limit: u32) -> (i64, i64) {
        (
            self.limit.unwrap_or(default_limit).clamp(1, max_limit) as i64,
            self.offset.unwrap_or(0) as i64,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SessionQuery {
    pub agent_id: Option<AgentId>,
    pub profile_id: Option<ProfileId>,
    pub kind: Option<SessionKind>,
    pub status: Option<SessionStatus>,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AgentInstanceQuery {
    pub agent_id: Option<AgentId>,
    pub profile_id: Option<ProfileId>,
    pub status: Option<DurableIdentityStatus>,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentMessageRecord {
    pub sequence: u64,
    pub message: AgentMessage,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AgentMessageQuery {
    pub agent_id: Option<AgentId>,
    pub correlation_id: Option<String>,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionPacketRecord {
    pub sequence: u64,
    pub packet: CompletionPacket,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompletionPacketQuery {
    pub session_id: Option<SessionId>,
    pub status: Option<rusty_crew_core_protocol::CompletionStatus>,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkerRunQuery {
    pub parent_session_id: Option<SessionId>,
    pub delegated_session_id: Option<SessionId>,
    pub profile_id: Option<ProfileId>,
    pub task_id: Option<TaskId>,
    pub status: Option<WorkerRunStatus>,
    pub terminal: Option<bool>,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RuntimeCounterQuery {
    pub scope: Option<RuntimeCounterScope>,
    pub counter_name: Option<String>,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileMemoryTarget {
    Profile,
    User(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileMemoryCaps {
    pub max_records_per_profile: u32,
    pub max_key_bytes: u32,
    pub max_content_bytes: u32,
}

impl Default for ProfileMemoryCaps {
    fn default() -> Self {
        Self {
            max_records_per_profile: 64,
            max_key_bytes: 128,
            max_content_bytes: 8 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileMemoryRecord {
    pub profile_id: ProfileId,
    pub target: ProfileMemoryTarget,
    pub key: String,
    pub content: String,
    pub metadata: JsonValue,
    pub revision: u64,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileMemoryWrite {
    pub profile_id: ProfileId,
    pub target: ProfileMemoryTarget,
    pub key: String,
    pub content: String,
    pub metadata: JsonValue,
    pub now: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileMemoryReplace {
    pub write: ProfileMemoryWrite,
    pub expected_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileMemoryDelete {
    pub profile_id: ProfileId,
    pub target: ProfileMemoryTarget,
    pub key: String,
    pub expected_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileMemoryQuery {
    pub profile_id: ProfileId,
    pub target: Option<ProfileMemoryTarget>,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeEventRecord {
    pub sequence: u64,
    pub kind: CoreEventKind,
    pub recorded_at: IsoTimestamp,
    pub event: CoreEvent,
    pub session_ids: Vec<SessionId>,
    pub agent_ids: Vec<AgentId>,
    pub instance_ids: Vec<AgentInstanceId>,
    pub correlation_ids: Vec<String>,
    pub source_wake_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RuntimeEventFilter {
    pub kind: Option<CoreEventKind>,
    pub session_id: Option<SessionId>,
    pub agent_id: Option<AgentId>,
    pub instance_id: Option<AgentInstanceId>,
    pub correlation_id: Option<String>,
    pub source_wake_id: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeSearchRowType {
    Message,
    QueueMessage,
    Session,
}

impl RuntimeSearchRowType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Message => "message",
            Self::QueueMessage => "queue_message",
            Self::Session => "session",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSearchFilter {
    pub query: String,
    pub row_type: Option<RuntimeSearchRowType>,
    pub session_id: Option<SessionId>,
    pub agent_id: Option<AgentId>,
    pub instance_id: Option<AgentInstanceId>,
    pub task_id: Option<TaskId>,
    pub event_kind: Option<CoreEventKind>,
    pub recorded_after: Option<IsoTimestamp>,
    pub recorded_before: Option<IsoTimestamp>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSearchResult {
    pub row_type: RuntimeSearchRowType,
    pub row_key: String,
    pub sequence: Option<u64>,
    pub session_id: Option<SessionId>,
    pub agent_id: Option<AgentId>,
    pub instance_id: Option<AgentInstanceId>,
    pub task_id: Option<TaskId>,
    pub event_kind: Option<CoreEventKind>,
    pub recorded_at: IsoTimestamp,
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueuedMessageState {
    Pending,
    Delivered,
    Expired,
    Discarded,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedMessageRecord {
    pub message_id: String,
    pub owner_session_id: Option<SessionId>,
    pub owner_agent_id: AgentId,
    pub message: AgentMessage,
    pub source_sequence: Option<u64>,
    pub enqueued_at: IsoTimestamp,
    pub expires_at: IsoTimestamp,
    pub ttl_ms: u32,
    pub delivery_attempts: u32,
    pub state: QueuedMessageState,
    pub terminal_at: Option<IsoTimestamp>,
    pub state_reason: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QueuedMessageFilter {
    pub state: Option<QueuedMessageState>,
    pub owner_session_id: Option<SessionId>,
    pub owner_agent_id: Option<AgentId>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduledJobStatus {
    Active,
    Paused,
    Archived,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduledRunStatus {
    Claimed,
    Completed,
    Skipped,
    Failed,
    Expired,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduledRunTrigger {
    Due,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledJobRecord {
    pub job_id: String,
    pub job_kind: String,
    pub target_session_id: Option<SessionId>,
    pub interval_ms: Option<u64>,
    pub next_due_at: Option<IsoTimestamp>,
    pub payload_json: JsonValue,
    pub status: ScheduledJobStatus,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
    pub paused_at: Option<IsoTimestamp>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScheduledJobQuery {
    pub status: Option<ScheduledJobStatus>,
    pub job_kind: Option<String>,
    pub due_at_or_before: Option<IsoTimestamp>,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledRunRecord {
    pub run_id: RunId,
    pub job_id: String,
    pub job_kind: String,
    pub target_session_id: Option<SessionId>,
    pub status: ScheduledRunStatus,
    pub trigger: ScheduledRunTrigger,
    pub scheduled_for: Option<IsoTimestamp>,
    pub claimed_at: IsoTimestamp,
    pub claim_deadline_at: IsoTimestamp,
    pub completed_at: Option<IsoTimestamp>,
    pub error: Option<String>,
    pub output_json: JsonValue,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScheduledRunQuery {
    pub job_id: Option<String>,
    pub status: Option<ScheduledRunStatus>,
    pub trigger: Option<ScheduledRunTrigger>,
    pub target_session_id: Option<SessionId>,
    pub stale_claim_deadline_before: Option<IsoTimestamp>,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RuntimeMaintenancePolicy {
    pub expire_queued_messages_at: Option<IsoTimestamp>,
    pub purge_terminal_queued_messages_before: Option<IsoTimestamp>,
    pub run_wal_checkpoint: bool,
    pub run_optimize: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeDatabaseSize {
    pub database_bytes: u64,
    pub page_count: u64,
    pub page_size_bytes: u64,
    pub freelist_pages: u64,
    pub freelist_bytes: u64,
    pub wal_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeMaintenanceReport {
    pub size_before: RuntimeDatabaseSize,
    pub size_after: RuntimeDatabaseSize,
    pub expired_queue_messages: u64,
    pub purged_terminal_queue_messages: u64,
    pub wal_checkpoint_ran: bool,
    pub optimize_ran: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeQueryPlanCheck {
    pub name: &'static str,
    pub uses_index: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeImportBatchRecord {
    pub import_batch_id: String,
    pub source_system: String,
    pub source_label: String,
    pub source_snapshot_ref: Option<String>,
    pub notes: Option<String>,
    pub imported_at: IsoTimestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeObjectKind {
    Agent,
    AgentInstance,
    Session,
    Profile,
    WorkerRun,
    Message,
    CompletionPacket,
    ToolCall,
    QueueMessage,
    ExternalArtifact,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, serde::Deserialize)]
pub struct RuntimeImportProvenance {
    pub profile_id: Option<ProfileId>,
    pub session_id: Option<SessionId>,
    pub agent_id: Option<AgentId>,
    pub externally_owned: bool,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyIdMappingRecord {
    pub import_batch_id: String,
    pub source: SourceSystemReference,
    pub legacy_kind: RuntimeObjectKind,
    pub rusty_kind: RuntimeObjectKind,
    pub rusty_id: String,
    pub provenance: RuntimeImportProvenance,
    pub created_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LegacyIdMappingQuery {
    pub import_batch_id: Option<String>,
    pub source_system: Option<String>,
    pub legacy_kind: Option<RuntimeObjectKind>,
    pub rusty_kind: Option<RuntimeObjectKind>,
    pub rusty_id: Option<String>,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalBindingStatus {
    Active,
    Degraded,
    Disconnected,
    Archived,
}

impl ExternalBindingStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Degraded => "degraded",
            Self::Disconnected => "disconnected",
            Self::Archived => "archived",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ExternalBindingProvenance {
    pub source_system: Option<String>,
    pub source_ref: Option<String>,
    pub externally_owned: bool,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelBindingRecord {
    pub binding_id: String,
    pub adapter_id: AdapterId,
    pub provider: String,
    pub agent_id: AgentId,
    pub instance_id: Option<AgentInstanceId>,
    pub session_id: Option<SessionId>,
    pub profile_id: ProfileId,
    pub external_channel_id: String,
    pub external_thread_id: Option<String>,
    pub external_user_id: Option<String>,
    pub provider_subscription_id: Option<String>,
    pub cursor: Option<String>,
    pub membership_state: Option<String>,
    pub presence_state: Option<String>,
    pub status: ExternalBindingStatus,
    pub degraded_reason: Option<String>,
    pub provenance: ExternalBindingProvenance,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChannelBindingQuery {
    pub agent_id: Option<AgentId>,
    pub instance_id: Option<AgentInstanceId>,
    pub session_id: Option<SessionId>,
    pub profile_id: Option<ProfileId>,
    pub adapter_id: Option<AdapterId>,
    pub provider: Option<String>,
    pub external_channel_id: Option<String>,
    pub status: Option<ExternalBindingStatus>,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct McpBindingDiagnostics {
    pub last_error: Option<String>,
    pub last_checked_at: Option<IsoTimestamp>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpBindingRecord {
    pub binding_id: String,
    pub adapter_id: AdapterId,
    pub agent_id: AgentId,
    pub instance_id: Option<AgentInstanceId>,
    pub session_id: Option<SessionId>,
    pub profile_id: ProfileId,
    pub server_names: Vec<String>,
    pub endpoint_ref: String,
    pub transport: String,
    pub tool_profile_key: String,
    pub discovered_tool_revision: Option<String>,
    pub status: ExternalBindingStatus,
    pub degraded_reason: Option<String>,
    pub diagnostics: McpBindingDiagnostics,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct McpBindingQuery {
    pub agent_id: Option<AgentId>,
    pub instance_id: Option<AgentInstanceId>,
    pub session_id: Option<SessionId>,
    pub profile_id: Option<ProfileId>,
    pub adapter_id: Option<AdapterId>,
    pub status: Option<ExternalBindingStatus>,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeCounterScope {
    Runtime,
    Agent(AgentId),
    Instance(AgentInstanceId),
    Session(SessionId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeCounterRecord {
    pub scope: RuntimeCounterScope,
    pub counter_name: String,
    pub value: u64,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeStateSummary {
    pub scope: RuntimeCounterScope,
    pub brain_turns: u64,
    pub wakes: u64,
    pub tool_calls: u64,
    pub tool_errors: u64,
    pub delegations_created: u64,
    pub delegations_completed: u64,
    pub delegations_failed: u64,
    pub delegations_timed_out: u64,
    pub delegations_cancelled: u64,
    pub messages: u64,
    pub completions: u64,
    pub queue_expirations: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCallRecord {
    pub sequence: u64,
    pub session_id: SessionId,
    pub wake_id: Option<String>,
    pub tool_name: String,
    pub phase: ToolCallPhase,
    pub is_error: Option<bool>,
    pub metadata: Option<ToolCallMetadata>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCallPhase {
    Started,
    Finished,
}

impl ToolCallPhase {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Finished => "finished",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticTable {
    Agents,
    AgentInstances,
    Sessions,
    SessionConfigs,
    SessionIdentity,
    EventHistory,
    EventAgentIndex,
    EventCorrelationIndex,
    EventInstanceIndex,
    EventSessionIndex,
    EventWakeIndex,
    RuntimeCounters,
    RuntimeSearch,
    QueuedMessages,
    RuntimeImportBatches,
    LegacyIdMappings,
    ProfileMemories,
    ScheduledJobs,
    ScheduledJobRuns,
    ChannelBindings,
    McpBindings,
    AgentMessages,
    CompletionPackets,
    WorkerRuns,
    ToolCallHistory,
}

impl DiagnosticTable {
    pub const ALL: &'static [Self] = &[
        Self::Agents,
        Self::AgentInstances,
        Self::Sessions,
        Self::SessionConfigs,
        Self::SessionIdentity,
        Self::EventHistory,
        Self::EventAgentIndex,
        Self::EventCorrelationIndex,
        Self::EventInstanceIndex,
        Self::EventSessionIndex,
        Self::EventWakeIndex,
        Self::RuntimeCounters,
        Self::RuntimeSearch,
        Self::QueuedMessages,
        Self::RuntimeImportBatches,
        Self::LegacyIdMappings,
        Self::ProfileMemories,
        Self::ScheduledJobs,
        Self::ScheduledJobRuns,
        Self::ChannelBindings,
        Self::McpBindings,
        Self::AgentMessages,
        Self::CompletionPackets,
        Self::WorkerRuns,
        Self::ToolCallHistory,
    ];

    fn parse(raw: &str) -> CoreResult<Self> {
        match raw {
            "agents" => Ok(Self::Agents),
            "agent_instances" => Ok(Self::AgentInstances),
            "sessions" => Ok(Self::Sessions),
            "session_configs" => Ok(Self::SessionConfigs),
            "session_identity" => Ok(Self::SessionIdentity),
            "event_history" => Ok(Self::EventHistory),
            "event_agent_index" => Ok(Self::EventAgentIndex),
            "event_correlation_index" => Ok(Self::EventCorrelationIndex),
            "event_instance_index" => Ok(Self::EventInstanceIndex),
            "event_session_index" => Ok(Self::EventSessionIndex),
            "event_wake_index" => Ok(Self::EventWakeIndex),
            "runtime_counters" => Ok(Self::RuntimeCounters),
            "runtime_search_fts" => Ok(Self::RuntimeSearch),
            "queued_messages" => Ok(Self::QueuedMessages),
            "runtime_import_batches" => Ok(Self::RuntimeImportBatches),
            "legacy_id_mappings" => Ok(Self::LegacyIdMappings),
            "profile_memories" => Ok(Self::ProfileMemories),
            "scheduled_jobs" => Ok(Self::ScheduledJobs),
            "scheduled_job_runs" => Ok(Self::ScheduledJobRuns),
            "channel_bindings" => Ok(Self::ChannelBindings),
            "mcp_bindings" => Ok(Self::McpBindings),
            "agent_messages" => Ok(Self::AgentMessages),
            "completion_packets" => Ok(Self::CompletionPackets),
            "worker_runs" => Ok(Self::WorkerRuns),
            "tool_call_history" => Ok(Self::ToolCallHistory),
            _ => Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("unsupported persistence table {raw}"),
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Agents => "agents",
            Self::AgentInstances => "agent_instances",
            Self::Sessions => "sessions",
            Self::SessionConfigs => "session_configs",
            Self::SessionIdentity => "session_identity",
            Self::EventHistory => "event_history",
            Self::EventAgentIndex => "event_agent_index",
            Self::EventCorrelationIndex => "event_correlation_index",
            Self::EventInstanceIndex => "event_instance_index",
            Self::EventSessionIndex => "event_session_index",
            Self::EventWakeIndex => "event_wake_index",
            Self::RuntimeCounters => "runtime_counters",
            Self::RuntimeSearch => "runtime_search_fts",
            Self::QueuedMessages => "queued_messages",
            Self::RuntimeImportBatches => "runtime_import_batches",
            Self::LegacyIdMappings => "legacy_id_mappings",
            Self::ProfileMemories => "profile_memories",
            Self::ScheduledJobs => "scheduled_jobs",
            Self::ScheduledJobRuns => "scheduled_job_runs",
            Self::ChannelBindings => "channel_bindings",
            Self::McpBindings => "mcp_bindings",
            Self::AgentMessages => "agent_messages",
            Self::CompletionPackets => "completion_packets",
            Self::WorkerRuns => "worker_runs",
            Self::ToolCallHistory => "tool_call_history",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerRunStatus {
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

impl WorkerRunStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::SessionCreated => "session_created",
            Self::WakeRequested => "wake_requested",
            Self::Running => "running",
            Self::CheckpointWaiting => "checkpoint_waiting",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Blocked => "blocked",
            Self::Exhausted => "exhausted",
            Self::Cancelled => "cancelled",
            Self::Expired => "expired",
        }
    }

    pub const fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed
                | Self::Failed
                | Self::Blocked
                | Self::Exhausted
                | Self::Cancelled
                | Self::Expired
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerRunRecord {
    pub run_id: RunId,
    pub parent_session_id: SessionId,
    pub delegated_session_id: Option<SessionId>,
    pub parent_agent_id: Option<AgentId>,
    pub profile_id: ProfileId,
    pub task_id: Option<TaskId>,
    pub status: WorkerRunStatus,
    pub created_at: IsoTimestamp,
    pub last_updated_at: IsoTimestamp,
    pub source_wake_id: String,
    pub source_action_index: u32,
    pub delegation_correlation_id: Option<String>,
    pub parent_consumption: ParentConsumptionPolicy,
    pub fan_out_group_id: Option<String>,
    pub fan_out_max_concurrency: Option<u32>,
    pub fan_out_failure_policy: FanOutFailurePolicy,
}

impl CoordinationStore {
    pub fn open(engine_data_dir: impl AsRef<Path>) -> CoreResult<Self> {
        fs::create_dir_all(engine_data_dir.as_ref())
            .map_err(|error| persistence_error("create coordination data directory", error))?;
        Self::open_file(engine_data_dir.as_ref().join(DB_FILE_NAME))
    }

    pub fn open_file(path: impl AsRef<Path>) -> CoreResult<Self> {
        let conn = Connection::open(path.as_ref())
            .map_err(|error| persistence_error("open sqlite", error))?;
        configure_connection(&conn)?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn save_session(&self, state: &SessionState) -> CoreResult<()> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start save session", error))?;
        save_session_state_in_tx(&tx, state)?;
        save_default_identity_for_session_in_tx(&tx, state)?;
        tx.commit()
            .map_err(|error| persistence_error("commit save session", error))?;
        Ok(())
    }

    pub fn save_session_with_config(
        &self,
        state: &SessionState,
        config: &SessionConfig,
    ) -> CoreResult<()> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start save session with config", error))?;
        save_session_state_in_tx(&tx, state)?;
        save_session_config_in_tx(&tx, config, &state.created_at)?;
        save_default_identity_for_session_in_tx(&tx, state)?;
        tx.commit()
            .map_err(|error| persistence_error("commit save session with config", error))?;
        Ok(())
    }

    pub fn upsert_agent_identity(&self, record: &DurableAgentRecord) -> CoreResult<()> {
        let conn = self.conn()?;
        save_agent_identity(&conn, record)
    }

    pub fn load_agent_identities(&self) -> CoreResult<Vec<DurableAgentRecord>> {
        let conn = self.conn()?;
        load_agent_identities(&conn)
    }

    pub fn upsert_agent_instance(&self, record: &AgentInstanceRecord) -> CoreResult<()> {
        let conn = self.conn()?;
        save_agent_instance(&conn, record)
    }

    pub fn load_agent_instances(&self) -> CoreResult<Vec<AgentInstanceRecord>> {
        let conn = self.conn()?;
        load_agent_instances(&conn)
    }

    pub fn query_agent_instances(
        &self,
        query: &AgentInstanceQuery,
    ) -> CoreResult<Vec<AgentInstanceRecord>> {
        let conn = self.conn()?;
        query_agent_instances(&conn, query)
    }

    pub fn upsert_session_identity(&self, record: &SessionIdentityRecord) -> CoreResult<()> {
        let conn = self.conn()?;
        save_session_identity(&conn, record)
    }

    pub fn load_session_identities(&self) -> CoreResult<Vec<SessionIdentityRecord>> {
        let conn = self.conn()?;
        load_session_identities(&conn)
    }

    pub fn load_session_configs(&self) -> CoreResult<Vec<SessionConfigRecord>> {
        let conn = self.conn()?;
        load_session_config_records(&conn)
    }

    pub fn list_profile_memory(
        &self,
        query: &ProfileMemoryQuery,
    ) -> CoreResult<Vec<ProfileMemoryRecord>> {
        let conn = self.conn()?;
        query_profile_memory(&conn, query)
    }

    pub fn get_profile_memory(
        &self,
        profile_id: &ProfileId,
        target: &ProfileMemoryTarget,
        key: &str,
    ) -> CoreResult<Option<ProfileMemoryRecord>> {
        validate_profile_memory_key(key, ProfileMemoryCaps::default().max_key_bytes)?;
        let conn = self.conn()?;
        get_profile_memory(&conn, profile_id, target, key)
    }

    pub fn add_profile_memory(
        &self,
        write: &ProfileMemoryWrite,
        caps: &ProfileMemoryCaps,
    ) -> CoreResult<ProfileMemoryRecord> {
        validate_profile_memory_write(write, caps)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start add profile memory", error))?;
        let count = count_profile_memory_for_profile(&tx, &write.profile_id)?;
        if count >= caps.max_records_per_profile as u64 {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "profile {} already has the maximum {} dense memory records",
                    write.profile_id, caps.max_records_per_profile
                ),
            ));
        }
        if get_profile_memory(&tx, &write.profile_id, &write.target, &write.key)?.is_some() {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!(
                    "profile memory {} for profile {} already exists",
                    write.key, write.profile_id
                ),
            ));
        }
        let record = insert_profile_memory_in_tx(&tx, write)?;
        tx.commit()
            .map_err(|error| persistence_error("commit add profile memory", error))?;
        Ok(record)
    }

    pub fn replace_profile_memory(
        &self,
        replace: &ProfileMemoryReplace,
        caps: &ProfileMemoryCaps,
    ) -> CoreResult<ProfileMemoryRecord> {
        validate_profile_memory_write(&replace.write, caps)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start replace profile memory", error))?;
        let existing = get_profile_memory(
            &tx,
            &replace.write.profile_id,
            &replace.write.target,
            &replace.write.key,
        )?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!(
                    "profile memory {} for profile {} not found",
                    replace.write.key, replace.write.profile_id
                ),
            )
        })?;
        if existing.revision != replace.expected_revision {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "profile memory revision mismatch for {}: expected {}, found {}",
                    replace.write.key, replace.expected_revision, existing.revision
                ),
            ));
        }
        let record = update_profile_memory_in_tx(&tx, &replace.write, existing.revision + 1)?;
        tx.commit()
            .map_err(|error| persistence_error("commit replace profile memory", error))?;
        Ok(record)
    }

    pub fn remove_profile_memory(
        &self,
        delete: &ProfileMemoryDelete,
    ) -> CoreResult<ProfileMemoryRecord> {
        validate_profile_memory_key(&delete.key, ProfileMemoryCaps::default().max_key_bytes)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start remove profile memory", error))?;
        let existing = get_profile_memory(&tx, &delete.profile_id, &delete.target, &delete.key)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    format!(
                        "profile memory {} for profile {} not found",
                        delete.key, delete.profile_id
                    ),
                )
            })?;
        if existing.revision != delete.expected_revision {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "profile memory revision mismatch for {}: expected {}, found {}",
                    delete.key, delete.expected_revision, existing.revision
                ),
            ));
        }
        let (target_type, target_id) =
            profile_memory_target_parts(&delete.profile_id, &delete.target);
        tx.execute(
            "DELETE FROM profile_memories
             WHERE profile_id = ?1
               AND target_type = ?2
               AND target_id = ?3
               AND memory_key = ?4",
            params![
                delete.profile_id.0.as_str(),
                target_type,
                target_id.as_str(),
                delete.key.as_str(),
            ],
        )
        .map_err(|error| persistence_error("remove profile memory", error))?;
        tx.commit()
            .map_err(|error| persistence_error("commit remove profile memory", error))?;
        Ok(existing)
    }

    pub fn save_queued_message(&self, record: &QueuedMessageRecord) -> CoreResult<()> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start save queued message", error))?;
        save_queued_message_in_tx(&tx, record)?;
        tx.commit()
            .map_err(|error| persistence_error("commit save queued message", error))?;
        Ok(())
    }

    pub fn expire_queued_messages_at(
        &self,
        now: &IsoTimestamp,
    ) -> CoreResult<Vec<QueuedMessageRecord>> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start expire queued messages", error))?;
        let expired = expire_queued_messages_in_tx(&tx, now)?;
        tx.commit()
            .map_err(|error| persistence_error("commit expire queued messages", error))?;
        Ok(expired)
    }

    pub fn load_queued_messages(
        &self,
        filter: &QueuedMessageFilter,
    ) -> CoreResult<Vec<QueuedMessageRecord>> {
        let conn = self.conn()?;
        load_queued_messages(&conn, filter)
    }

    pub fn upsert_scheduled_job(&self, record: &ScheduledJobRecord) -> CoreResult<()> {
        let conn = self.conn()?;
        save_scheduled_job(&conn, record)
    }

    pub fn load_scheduled_job(&self, job_id: &str) -> CoreResult<Option<ScheduledJobRecord>> {
        let conn = self.conn()?;
        load_scheduled_job(&conn, job_id)
    }

    pub fn query_scheduled_jobs(
        &self,
        query: &ScheduledJobQuery,
    ) -> CoreResult<Vec<ScheduledJobRecord>> {
        let conn = self.conn()?;
        query_scheduled_jobs(&conn, query)
    }

    pub fn pause_scheduled_job(&self, job_id: &str, now: &IsoTimestamp) -> CoreResult<()> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE scheduled_jobs
             SET status = 'paused', paused_at = ?2, updated_at = ?2
             WHERE job_id = ?1 AND status != 'archived'",
            params![job_id, now],
        )
        .map_err(|error| persistence_error("pause scheduled job", error))?;
        Ok(())
    }

    pub fn resume_scheduled_job(
        &self,
        job_id: &str,
        next_due_at: &IsoTimestamp,
        now: &IsoTimestamp,
    ) -> CoreResult<()> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE scheduled_jobs
             SET status = 'active', next_due_at = ?2, paused_at = NULL, updated_at = ?3
             WHERE job_id = ?1 AND status != 'archived'",
            params![job_id, next_due_at, now],
        )
        .map_err(|error| persistence_error("resume scheduled job", error))?;
        Ok(())
    }

    pub fn claim_scheduled_run(
        &self,
        run: &ScheduledRunRecord,
        next_due_at: Option<&IsoTimestamp>,
    ) -> CoreResult<()> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start claim scheduled run", error))?;
        save_scheduled_run_in_tx(&tx, run)?;
        if run.trigger == ScheduledRunTrigger::Due {
            tx.execute(
                "UPDATE scheduled_jobs
                 SET next_due_at = ?2, updated_at = ?3
                 WHERE job_id = ?1 AND status = 'active'",
                params![run.job_id.as_str(), next_due_at, run.updated_at.as_str()],
            )
            .map_err(|error| persistence_error("advance scheduled job", error))?;
        }
        tx.commit()
            .map_err(|error| persistence_error("commit claim scheduled run", error))?;
        Ok(())
    }

    pub fn complete_scheduled_run(
        &self,
        run_id: &RunId,
        status: ScheduledRunStatus,
        completed_at: &IsoTimestamp,
        output_json: &JsonValue,
        error: Option<&str>,
    ) -> CoreResult<()> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE scheduled_job_runs
             SET status = ?2,
                 completed_at = ?3,
                 updated_at = ?3,
                 output_json = ?4,
                 error = ?5
             WHERE run_id = ?1",
            params![
                run_id.0.as_str(),
                scheduled_run_status_as_str(status),
                completed_at,
                to_json_text(output_json)?,
                error,
            ],
        )
        .map_err(|error| persistence_error("complete scheduled run", error))?;
        Ok(())
    }

    pub fn query_scheduled_runs(
        &self,
        query: &ScheduledRunQuery,
    ) -> CoreResult<Vec<ScheduledRunRecord>> {
        let conn = self.conn()?;
        query_scheduled_runs(&conn, query)
    }

    pub fn expire_stale_scheduled_runs(
        &self,
        stale_before: &IsoTimestamp,
        now: &IsoTimestamp,
    ) -> CoreResult<Vec<ScheduledRunRecord>> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start expire stale scheduled runs", error))?;
        let stale = query_scheduled_runs(
            &tx,
            &ScheduledRunQuery {
                status: Some(ScheduledRunStatus::Claimed),
                stale_claim_deadline_before: Some(stale_before.clone()),
                page: None,
                ..ScheduledRunQuery::default()
            },
        )?;
        for run in &stale {
            tx.execute(
                "UPDATE scheduled_job_runs
                 SET status = 'expired',
                     completed_at = ?2,
                     updated_at = ?2,
                     error = 'claim deadline elapsed'
                 WHERE run_id = ?1 AND status = 'claimed'",
                params![run.run_id.0.as_str(), now],
            )
            .map_err(|error| persistence_error("expire stale scheduled run", error))?;
        }
        tx.commit()
            .map_err(|error| persistence_error("commit expire stale scheduled runs", error))?;
        Ok(stale)
    }

    pub fn database_size(&self) -> CoreResult<RuntimeDatabaseSize> {
        let conn = self.conn()?;
        database_size(&conn)
    }

    pub fn run_maintenance(
        &self,
        policy: &RuntimeMaintenancePolicy,
    ) -> CoreResult<RuntimeMaintenanceReport> {
        let size_before = self.database_size()?;
        let mut expired_queue_messages = 0;
        let mut purged_terminal_queue_messages = 0;
        {
            let mut conn = self.conn()?;
            let tx = conn
                .transaction()
                .map_err(|error| persistence_error("start runtime maintenance", error))?;
            if let Some(now) = &policy.expire_queued_messages_at {
                expired_queue_messages = expire_queued_messages_in_tx(&tx, now)?.len() as u64;
            }
            if let Some(cutoff) = &policy.purge_terminal_queued_messages_before {
                purged_terminal_queue_messages = purge_terminal_queued_messages_in_tx(&tx, cutoff)?;
            }
            tx.commit()
                .map_err(|error| persistence_error("commit runtime maintenance", error))?;

            if policy.run_optimize {
                conn.execute_batch("PRAGMA optimize;")
                    .map_err(|error| persistence_error("optimize sqlite", error))?;
            }
            if policy.run_wal_checkpoint {
                conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
                    .map_err(|error| persistence_error("checkpoint sqlite wal", error))?;
            }
        }

        let size_after = self.database_size()?;
        Ok(RuntimeMaintenanceReport {
            size_before,
            size_after,
            expired_queue_messages,
            purged_terminal_queue_messages,
            wal_checkpoint_ran: policy.run_wal_checkpoint,
            optimize_ran: policy.run_optimize,
        })
    }

    pub fn load_sessions(&self) -> CoreResult<Vec<SessionState>> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT
                    session_id,
                    handle,
                    agent_id,
                    profile_id,
                    kind_json,
                    delegation_json,
                    resource_limits_json,
                    tool_profile_json,
                    status_json,
                    brain_turn_count,
                    created_at,
                    last_active_at
                FROM sessions
                ORDER BY handle ASC",
            )
            .map_err(|error| persistence_error("prepare load sessions", error))?;

        let rows = stmt
            .query_map([], |row| {
                let kind_json: String = row.get(4)?;
                let delegation_json: Option<String> = row.get(5)?;
                let resource_limits_json: Option<String> = row.get(6)?;
                let tool_profile_json: Option<String> = row.get(7)?;
                let status_json: String = row.get(8)?;
                let kind = from_json_text::<SessionKind>(&kind_json).map_err(to_sql_error)?;
                let delegation = delegation_json
                    .as_deref()
                    .map(from_json_text::<DelegationLineage>)
                    .transpose()
                    .map_err(to_sql_error)?;
                let resource_limits = resource_limits_json
                    .as_deref()
                    .map(from_json_text::<ResourceLimits>)
                    .transpose()
                    .map_err(to_sql_error)?
                    .unwrap_or(ResourceLimits {
                        workdir: None,
                        max_duration_ms: None,
                        max_delegation_depth: None,
                    });
                let tool_profile = tool_profile_json
                    .as_deref()
                    .map(from_json_text::<ToolProfile>)
                    .transpose()
                    .map_err(to_sql_error)?
                    .unwrap_or(ToolProfile { tools: Vec::new() });
                let status = from_json_text::<SessionStatus>(&status_json).map_err(to_sql_error)?;
                Ok(SessionState {
                    session_id: SessionId(row.get(0)?),
                    handle: SessionHandle::new(row.get::<_, i64>(1)? as u64),
                    agent_id: rusty_crew_core_protocol::AgentId(row.get(2)?),
                    profile_id: ProfileId(row.get(3)?),
                    kind,
                    delegation,
                    resource_limits,
                    tool_profile,
                    status,
                    brain_turn_count: row.get::<_, i64>(9)? as u32,
                    created_at: row.get(10)?,
                    last_active_at: row.get(11)?,
                })
            })
            .map_err(|error| persistence_error("query sessions", error))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| persistence_error("load sessions", error))
    }

    pub fn query_sessions(&self, query: &SessionQuery) -> CoreResult<Vec<SessionState>> {
        let conn = self.conn()?;
        query_sessions(&conn, query)
    }

    pub fn query_agent_messages(
        &self,
        query: &AgentMessageQuery,
    ) -> CoreResult<Vec<AgentMessageRecord>> {
        let conn = self.conn()?;
        query_agent_messages(&conn, query)
    }

    pub fn query_completion_packets(
        &self,
        query: &CompletionPacketQuery,
    ) -> CoreResult<Vec<CompletionPacketRecord>> {
        let conn = self.conn()?;
        query_completion_packets(&conn, query)
    }

    pub fn save_event(&self, sequence: u64, event: &CoreEvent) -> CoreResult<()> {
        if !should_persist_event(event) {
            return Ok(());
        }

        let event_kind = format!("{:?}", CoreEventKind::of(event));
        let event_json = to_json_text(event)?;
        let conn = self.conn()?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|error| persistence_error("begin save event", error))?;
        let is_new_event = tx
            .query_row(
                "SELECT NOT EXISTS(SELECT 1 FROM event_history WHERE sequence = ?1)",
                params![sequence as i64],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| persistence_error("check existing event sequence", error))?
            != 0;
        tx.execute(
            "INSERT OR REPLACE INTO event_history (sequence, event_kind, event_json)
             VALUES (?1, ?2, ?3)",
            params![sequence as i64, event_kind, event_json],
        )
        .map_err(|error| persistence_error("save event history", error))?;
        save_event_indexes_in_tx(&tx, sequence, event)?;
        if is_new_event {
            increment_event_counters_in_tx(&tx, event)?;
        }
        let recorded_at = tx
            .query_row(
                "SELECT recorded_at FROM event_history WHERE sequence = ?1",
                params![sequence as i64],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| persistence_error("read event recorded timestamp", error))?;
        save_event_search_rows_in_tx(&tx, sequence, event, &recorded_at)?;

        match event {
            CoreEvent::AgentMessageRouted { message } => {
                let message_json = to_json_text(message)?;
                tx.execute(
                    "INSERT OR REPLACE INTO agent_messages (
                        sequence,
                        from_agent,
                        to_agent,
                        body,
                        correlation_id,
                        message_json
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        sequence as i64,
                        message.from.0,
                        message.to.0,
                        message.body,
                        message.correlation_id,
                        message_json,
                    ],
                )
                .map_err(|error| persistence_error("save message history", error))?;
            }
            CoreEvent::CompletionPacketDelivered { packet } => {
                self.save_completion_packet_in_tx(&tx, sequence, packet)?;
            }
            CoreEvent::BrainEventObserved {
                session_id,
                wake_id,
                event,
            } => {
                self.save_tool_call_in_tx(&tx, sequence, session_id, wake_id.as_deref(), event)?;
            }
            _ => {}
        }

        tx.commit()
            .map_err(|error| persistence_error("commit save event", error))?;
        Ok(())
    }

    pub fn load_event_history(&self) -> CoreResult<Vec<PersistedEvent>> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare("SELECT sequence, event_json FROM event_history ORDER BY sequence ASC")
            .map_err(|error| persistence_error("prepare load event history", error))?;

        let rows = stmt
            .query_map([], |row| {
                let event_json: String = row.get(1)?;
                let event = from_json_text::<CoreEvent>(&event_json).map_err(to_sql_error)?;
                Ok(PersistedEvent {
                    sequence: row.get::<_, i64>(0)? as u64,
                    event,
                })
            })
            .map_err(|error| persistence_error("query event history", error))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| persistence_error("load event history", error))
    }

    pub fn query_events(&self, filter: &RuntimeEventFilter) -> CoreResult<Vec<RuntimeEventRecord>> {
        let conn = self.conn()?;
        let kind = filter.kind.as_ref().map(|kind| format!("{kind:?}"));
        let session_id = filter.session_id.as_ref().map(|value| value.0.as_str());
        let agent_id = filter.agent_id.as_ref().map(|value| value.0.as_str());
        let instance_id = filter.instance_id.as_ref().map(|value| value.0.as_str());
        let correlation_id = filter.correlation_id.as_deref();
        let source_wake_id = filter.source_wake_id.as_deref();
        let limit = filter.limit.unwrap_or(1_000).max(1) as i64;
        let mut stmt = conn
            .prepare(
                "SELECT sequence, event_kind, recorded_at, event_json
                 FROM event_history
                 WHERE (?1 IS NULL OR event_kind = ?1)
                   AND (?2 IS NULL OR EXISTS (
                        SELECT 1 FROM event_session_index
                        WHERE event_session_index.sequence = event_history.sequence
                          AND event_session_index.session_id = ?2
                   ))
                   AND (?3 IS NULL OR EXISTS (
                        SELECT 1 FROM event_agent_index
                        WHERE event_agent_index.sequence = event_history.sequence
                          AND event_agent_index.agent_id = ?3
                   ))
                   AND (?4 IS NULL OR EXISTS (
                        SELECT 1 FROM event_instance_index
                        WHERE event_instance_index.sequence = event_history.sequence
                          AND event_instance_index.instance_id = ?4
                   ))
                   AND (?5 IS NULL OR EXISTS (
                        SELECT 1 FROM event_correlation_index
                        WHERE event_correlation_index.sequence = event_history.sequence
                          AND event_correlation_index.correlation_id = ?5
                   ))
                   AND (?6 IS NULL OR EXISTS (
                        SELECT 1 FROM event_wake_index
                        WHERE event_wake_index.sequence = event_history.sequence
                          AND event_wake_index.source_wake_id = ?6
                   ))
                 ORDER BY sequence ASC
                 LIMIT ?7",
            )
            .map_err(|error| persistence_error("prepare query events", error))?;
        let rows = stmt
            .query_map(
                params![
                    kind,
                    session_id,
                    agent_id,
                    instance_id,
                    correlation_id,
                    source_wake_id,
                    limit,
                ],
                |row| {
                    let sequence = row.get::<_, i64>(0)? as u64;
                    let event_json: String = row.get(3)?;
                    let event = from_json_text::<CoreEvent>(&event_json).map_err(to_sql_error)?;
                    Ok(RuntimeEventRecord {
                        sequence,
                        kind: CoreEventKind::of(&event),
                        recorded_at: row.get(2)?,
                        event,
                        session_ids: load_event_index_values(
                            &conn,
                            EventIndexProjection::Session,
                            sequence,
                        )?
                        .into_iter()
                        .map(SessionId)
                        .collect(),
                        agent_ids: load_event_index_values(
                            &conn,
                            EventIndexProjection::Agent,
                            sequence,
                        )?
                        .into_iter()
                        .map(AgentId)
                        .collect(),
                        instance_ids: load_event_index_values(
                            &conn,
                            EventIndexProjection::Instance,
                            sequence,
                        )?
                        .into_iter()
                        .map(AgentInstanceId)
                        .collect(),
                        correlation_ids: load_event_index_values(
                            &conn,
                            EventIndexProjection::Correlation,
                            sequence,
                        )?,
                        source_wake_ids: load_event_index_values(
                            &conn,
                            EventIndexProjection::Wake,
                            sequence,
                        )?,
                    })
                },
            )
            .map_err(|error| persistence_error("query events", error))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| persistence_error("load queried events", error))
    }

    pub fn search_runtime(
        &self,
        filter: &RuntimeSearchFilter,
    ) -> CoreResult<Vec<RuntimeSearchResult>> {
        if filter.query.trim().is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "runtime search query must be non-empty",
            ));
        }

        let conn = self.conn()?;
        let row_type = filter.row_type.map(RuntimeSearchRowType::as_str);
        let session_id = filter.session_id.as_ref().map(|value| value.0.as_str());
        let agent_id = filter.agent_id.as_ref().map(|value| value.0.as_str());
        let instance_id = filter.instance_id.as_ref().map(|value| value.0.as_str());
        let task_id = filter.task_id.as_ref().map(|value| value.0.as_str());
        let event_kind = filter.event_kind.as_ref().map(|kind| format!("{kind:?}"));
        let recorded_after = filter.recorded_after.as_deref();
        let recorded_before = filter.recorded_before.as_deref();
        let limit = filter.limit.unwrap_or(50).clamp(1, 200) as i64;
        let fts_query = quote_fts_query(filter.query.trim());
        let mut stmt = conn
            .prepare(
                "SELECT
                    row_type,
                    row_key,
                    sequence,
                    session_id,
                    agent_id,
                    instance_id,
                    task_id,
                    event_kind,
                    recorded_at,
                    title,
                    body
                 FROM runtime_search_fts
                 WHERE runtime_search_fts MATCH ?1
                   AND (?2 IS NULL OR row_type = ?2)
                   AND (?3 IS NULL OR session_id = ?3)
                   AND (?4 IS NULL OR agent_id = ?4)
                   AND (?5 IS NULL OR instance_id = ?5)
                   AND (?6 IS NULL OR task_id = ?6)
                   AND (?7 IS NULL OR event_kind = ?7)
                   AND (?8 IS NULL OR recorded_at >= ?8)
                   AND (?9 IS NULL OR recorded_at <= ?9)
                 ORDER BY rank
                 LIMIT ?10",
            )
            .map_err(|error| persistence_error("prepare runtime search", error))?;
        let rows = stmt
            .query_map(
                params![
                    fts_query,
                    row_type,
                    session_id,
                    agent_id,
                    instance_id,
                    task_id,
                    event_kind,
                    recorded_after,
                    recorded_before,
                    limit,
                ],
                row_to_runtime_search_result,
            )
            .map_err(|error| persistence_error("query runtime search", error))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| persistence_error("load runtime search results", error))
    }

    pub fn hot_query_plan_checks(&self) -> CoreResult<Vec<RuntimeQueryPlanCheck>> {
        let conn = self.conn()?;
        hot_query_plan_checks(&conn)
    }

    pub fn save_import_batch(&self, record: &RuntimeImportBatchRecord) -> CoreResult<()> {
        let conn = self.conn()?;
        save_import_batch(&conn, record)
    }

    pub fn load_import_batches(&self) -> CoreResult<Vec<RuntimeImportBatchRecord>> {
        let conn = self.conn()?;
        load_import_batches(&conn)
    }

    pub fn save_legacy_id_mapping(&self, record: &LegacyIdMappingRecord) -> CoreResult<()> {
        let conn = self.conn()?;
        save_legacy_id_mapping(&conn, record)
    }

    pub fn query_legacy_id_mappings(
        &self,
        query: &LegacyIdMappingQuery,
    ) -> CoreResult<Vec<LegacyIdMappingRecord>> {
        let conn = self.conn()?;
        query_legacy_id_mappings(&conn, query)
    }

    pub fn save_channel_binding(&self, record: &ChannelBindingRecord) -> CoreResult<()> {
        let conn = self.conn()?;
        save_channel_binding(&conn, record)
    }

    pub fn query_channel_bindings(
        &self,
        query: &ChannelBindingQuery,
    ) -> CoreResult<Vec<ChannelBindingRecord>> {
        let conn = self.conn()?;
        query_channel_bindings(&conn, query)
    }

    pub fn save_mcp_binding(&self, record: &McpBindingRecord) -> CoreResult<()> {
        let conn = self.conn()?;
        save_mcp_binding(&conn, record)
    }

    pub fn query_mcp_bindings(&self, query: &McpBindingQuery) -> CoreResult<Vec<McpBindingRecord>> {
        let conn = self.conn()?;
        query_mcp_bindings(&conn, query)
    }

    pub fn load_tool_call_history(&self) -> CoreResult<Vec<ToolCallRecord>> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT sequence, session_id, wake_id, tool_name, phase, is_error, metadata_json
                 FROM tool_call_history
                 ORDER BY sequence ASC",
            )
            .map_err(|error| persistence_error("prepare tool call history", error))?;

        let rows = stmt
            .query_map([], |row| {
                let phase: String = row.get(4)?;
                Ok(ToolCallRecord {
                    sequence: row.get::<_, i64>(0)? as u64,
                    session_id: SessionId(row.get(1)?),
                    wake_id: row.get(2)?,
                    tool_name: row.get(3)?,
                    phase: tool_call_phase_from_str(&phase)?,
                    is_error: row.get::<_, Option<i64>>(5)?.map(|value| value != 0),
                    metadata: row
                        .get::<_, Option<String>>(6)?
                        .map(|value| from_json_text::<ToolCallMetadata>(&value))
                        .transpose()
                        .map_err(to_sql_error)?,
                })
            })
            .map_err(|error| persistence_error("query tool call history", error))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| persistence_error("load tool call history", error))
    }

    pub fn save_worker_run_requested(&self, record: &WorkerRunRecord) -> CoreResult<()> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT OR REPLACE INTO worker_runs (
                run_id,
                session_id,
                delegated_session_id,
                parent_agent_id,
                profile_id,
                task_id,
                status,
                created_at,
                last_updated_at,
                source_wake_id,
                source_action_index,
                delegation_correlation_id,
                parent_consumption,
                fan_out_group_id,
                fan_out_max_concurrency,
                fan_out_failure_policy
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                record.run_id.0.as_str(),
                record.parent_session_id.0.as_str(),
                record
                    .delegated_session_id
                    .as_ref()
                    .map(|session_id| session_id.0.as_str()),
                record
                    .parent_agent_id
                    .as_ref()
                    .map(|agent_id| agent_id.0.as_str()),
                record.profile_id.0.as_str(),
                record.task_id.as_ref().map(|task_id| task_id.0.as_str()),
                record.status.as_str(),
                record.created_at.as_str(),
                record.last_updated_at.as_str(),
                record.source_wake_id.as_str(),
                record.source_action_index as i64,
                record.delegation_correlation_id.as_deref(),
                parent_consumption_policy_as_str(&record.parent_consumption),
                record.fan_out_group_id.as_deref(),
                record.fan_out_max_concurrency.map(|value| value as i64),
                fan_out_failure_policy_as_str(&record.fan_out_failure_policy),
            ],
        )
        .map_err(|error| persistence_error("save worker run", error))?;
        Ok(())
    }

    pub fn load_worker_run(&self, run_id: &RunId) -> CoreResult<Option<WorkerRunRecord>> {
        let conn = self.conn()?;
        conn.query_row(
            "SELECT
                run_id,
                session_id,
                delegated_session_id,
                parent_agent_id,
                profile_id,
                task_id,
                status,
                created_at,
                last_updated_at,
                source_wake_id,
                source_action_index,
                delegation_correlation_id,
                parent_consumption,
                fan_out_group_id,
                fan_out_max_concurrency,
                fan_out_failure_policy
             FROM worker_runs
             WHERE run_id = ?1",
            params![run_id.0.as_str()],
            row_to_worker_run,
        )
        .optional()
        .map_err(|error| persistence_error("load worker run", error))
    }

    pub fn load_worker_run_by_delegated_session(
        &self,
        delegated_session_id: &SessionId,
    ) -> CoreResult<Option<WorkerRunRecord>> {
        let conn = self.conn()?;
        conn.query_row(
            "SELECT
                run_id,
                session_id,
                delegated_session_id,
                parent_agent_id,
                profile_id,
                task_id,
                status,
                created_at,
                last_updated_at,
                source_wake_id,
                source_action_index,
                delegation_correlation_id,
                parent_consumption,
                fan_out_group_id,
                fan_out_max_concurrency,
                fan_out_failure_policy
             FROM worker_runs
             WHERE delegated_session_id = ?1",
            params![delegated_session_id.0.as_str()],
            row_to_worker_run,
        )
        .optional()
        .map_err(|error| persistence_error("load worker run by delegated session", error))
    }

    pub fn query_worker_runs(&self, query: &WorkerRunQuery) -> CoreResult<Vec<WorkerRunRecord>> {
        let conn = self.conn()?;
        query_worker_runs(&conn, query)
    }

    pub fn update_worker_run_status_by_delegated_session(
        &self,
        delegated_session_id: &SessionId,
        status: WorkerRunStatus,
        now: IsoTimestamp,
    ) -> CoreResult<()> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE worker_runs
             SET status = ?1, last_updated_at = ?2
             WHERE delegated_session_id = ?3",
            params![
                status.as_str(),
                now.as_str(),
                delegated_session_id.0.as_str()
            ],
        )
        .map_err(|error| persistence_error("update worker run status", error))?;
        Ok(())
    }

    pub fn update_worker_run_status(
        &self,
        run_id: &RunId,
        status: WorkerRunStatus,
        now: IsoTimestamp,
    ) -> CoreResult<()> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE worker_runs
             SET status = ?1, last_updated_at = ?2
             WHERE run_id = ?3",
            params![status.as_str(), now.as_str(), run_id.0.as_str()],
        )
        .map_err(|error| persistence_error("update worker run status by run id", error))?;
        Ok(())
    }

    pub fn delegated_completions_for_parent(
        &self,
        parent_session_id: &SessionId,
    ) -> CoreResult<Vec<DelegatedCompletion>> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT
                    worker_runs.run_id,
                    worker_runs.delegated_session_id,
                    worker_runs.task_id,
                    worker_runs.source_wake_id,
                    worker_runs.source_action_index,
                    worker_runs.delegation_correlation_id,
                    worker_runs.parent_consumption,
                    completion_packets.packet_json
                 FROM worker_runs
                 JOIN completion_packets
                    ON completion_packets.session_id = worker_runs.delegated_session_id
                 WHERE worker_runs.session_id = ?1
                 ORDER BY completion_packets.sequence ASC",
            )
            .map_err(|error| persistence_error("prepare delegated completions", error))?;

        let rows = stmt
            .query_map(params![parent_session_id.0.as_str()], |row| {
                let parent_consumption: String = row.get(6)?;
                let packet_json: String = row.get(7)?;
                let packet =
                    from_json_text::<CompletionPacket>(&packet_json).map_err(to_sql_error)?;
                Ok(DelegatedCompletion {
                    run_id: RunId(row.get(0)?),
                    child_session_id: SessionId(row.get(1)?),
                    requested_task_id: row.get::<_, Option<String>>(2)?.map(TaskId),
                    source_wake_id: row.get(3)?,
                    source_action_index: row.get::<_, i64>(4)? as u32,
                    correlation_id: row.get(5)?,
                    parent_consumption: parent_consumption_policy_from_str(&parent_consumption)?,
                    packet,
                })
            })
            .map_err(|error| persistence_error("query delegated completions", error))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| persistence_error("load delegated completions", error))
    }

    pub fn worker_runs_for_fan_out_group(
        &self,
        parent_session_id: &SessionId,
        group_id: &str,
    ) -> CoreResult<Vec<WorkerRunRecord>> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT
                    run_id,
                    session_id,
                    delegated_session_id,
                    parent_agent_id,
                    profile_id,
                    task_id,
                    status,
                    created_at,
                    last_updated_at,
                    source_wake_id,
                    source_action_index,
                    delegation_correlation_id,
                    parent_consumption,
                    fan_out_group_id,
                    fan_out_max_concurrency,
                    fan_out_failure_policy
                 FROM worker_runs
                 WHERE session_id = ?1 AND fan_out_group_id = ?2
                 ORDER BY source_wake_id ASC, source_action_index ASC",
            )
            .map_err(|error| persistence_error("prepare worker runs for fan-out group", error))?;

        let rows = stmt
            .query_map(
                params![parent_session_id.0.as_str(), group_id],
                row_to_worker_run,
            )
            .map_err(|error| persistence_error("query worker runs for fan-out group", error))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| persistence_error("load worker runs for fan-out group", error))
    }

    pub fn fan_out_groups_for_parent(
        &self,
        parent_session_id: &SessionId,
    ) -> CoreResult<Vec<DelegatedFanOutGroup>> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT
                    run_id,
                    session_id,
                    delegated_session_id,
                    parent_agent_id,
                    profile_id,
                    task_id,
                    status,
                    created_at,
                    last_updated_at,
                    source_wake_id,
                    source_action_index,
                    delegation_correlation_id,
                    parent_consumption,
                    fan_out_group_id,
                    fan_out_max_concurrency,
                    fan_out_failure_policy
                 FROM worker_runs
                 WHERE session_id = ?1 AND fan_out_group_id IS NOT NULL
                 ORDER BY fan_out_group_id ASC, source_wake_id ASC, source_action_index ASC",
            )
            .map_err(|error| persistence_error("prepare fan-out groups", error))?;

        let rows = stmt
            .query_map(params![parent_session_id.0.as_str()], row_to_worker_run)
            .map_err(|error| persistence_error("query fan-out groups", error))?;
        let runs = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| persistence_error("load fan-out group runs", error))?;

        Ok(aggregate_fan_out_groups(runs))
    }

    pub fn count_rows(&self, table: &str) -> CoreResult<u64> {
        let table = DiagnosticTable::parse(table)?;

        let conn = self.conn()?;
        let count = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM {}", table.as_str()),
                [],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| persistence_error("count rows", error))?
            .unwrap_or(0);
        Ok(count as u64)
    }

    pub fn runtime_counters(
        &self,
        scope: Option<&RuntimeCounterScope>,
    ) -> CoreResult<Vec<RuntimeCounterRecord>> {
        let conn = self.conn()?;
        load_runtime_counters(&conn, scope)
    }

    pub fn query_runtime_counters(
        &self,
        query: &RuntimeCounterQuery,
    ) -> CoreResult<Vec<RuntimeCounterRecord>> {
        let conn = self.conn()?;
        query_runtime_counters(&conn, query)
    }

    pub fn reset_runtime_counters(
        &self,
        query: &RuntimeCounterQuery,
        now: IsoTimestamp,
    ) -> CoreResult<u64> {
        let conn = self.conn()?;
        reset_runtime_counters(&conn, query, &now)
    }

    pub fn runtime_summary(&self, scope: &RuntimeCounterScope) -> CoreResult<RuntimeStateSummary> {
        let counters = self.runtime_counters(Some(scope))?;
        Ok(RuntimeStateSummary {
            scope: scope.clone(),
            brain_turns: counter_value(&counters, COUNTER_BRAIN_TURNS),
            wakes: counter_value(&counters, COUNTER_WAKES),
            tool_calls: counter_value(&counters, COUNTER_TOOL_CALLS),
            tool_errors: counter_value(&counters, COUNTER_TOOL_ERRORS),
            delegations_created: counter_value(&counters, COUNTER_DELEGATIONS_CREATED),
            delegations_completed: counter_value(&counters, COUNTER_DELEGATIONS_COMPLETED),
            delegations_failed: counter_value(&counters, COUNTER_DELEGATIONS_FAILED),
            delegations_timed_out: counter_value(&counters, COUNTER_DELEGATIONS_TIMED_OUT),
            delegations_cancelled: counter_value(&counters, COUNTER_DELEGATIONS_CANCELLED),
            messages: counter_value(&counters, COUNTER_MESSAGES),
            completions: counter_value(&counters, COUNTER_COMPLETIONS),
            queue_expirations: counter_value(&counters, COUNTER_QUEUE_EXPIRATIONS),
        })
    }

    pub fn schema_version(&self) -> CoreResult<i64> {
        let conn = self.conn()?;
        current_schema_version(&conn)
    }

    pub fn schema_migrations(&self) -> CoreResult<Vec<SchemaMigrationRecord>> {
        let conn = self.conn()?;
        load_schema_migration_records(&conn)
    }

    fn migrate(&self) -> CoreResult<()> {
        let mut conn = self.conn()?;
        prepare_migration_metadata(&conn)?;
        apply_schema_migrations(&mut conn, SCHEMA_MIGRATIONS)
    }

    fn save_completion_packet_in_tx(
        &self,
        tx: &rusqlite::Transaction<'_>,
        sequence: u64,
        packet: &CompletionPacket,
    ) -> CoreResult<()> {
        let packet_json = to_json_text(packet)?;
        let status_json = to_json_text(&packet.status)?;
        tx.execute(
            "INSERT OR REPLACE INTO completion_packets (
                sequence,
                session_id,
                status,
                summary,
                packet_json
            ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                sequence as i64,
                packet.session_id.0,
                status_json,
                packet.summary,
                packet_json,
            ],
        )
        .map_err(|error| persistence_error("save completion packet", error))?;
        Ok(())
    }

    fn save_tool_call_in_tx(
        &self,
        tx: &rusqlite::Transaction<'_>,
        sequence: u64,
        session_id: &SessionId,
        wake_id: Option<&str>,
        event: &BrainEvent,
    ) -> CoreResult<()> {
        let (tool_name, phase, is_error, metadata) = match event {
            BrainEvent::ToolCallStarted {
                tool_name,
                metadata,
            } => (tool_name, ToolCallPhase::Started, None, metadata),
            BrainEvent::ToolCallFinished {
                tool_name,
                is_error,
                metadata,
            } => (
                tool_name,
                ToolCallPhase::Finished,
                Some(*is_error),
                metadata,
            ),
            _ => return Ok(()),
        };
        let metadata_json = metadata.as_ref().map(to_json_text).transpose()?;
        tx.execute(
            "INSERT OR REPLACE INTO tool_call_history (
                sequence,
                session_id,
                wake_id,
                tool_name,
                phase,
                is_error,
                metadata_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                sequence as i64,
                session_id.0,
                wake_id,
                tool_name,
                phase.as_str(),
                is_error.map(|value| if value { 1_i64 } else { 0_i64 }),
                metadata_json,
            ],
        )
        .map_err(|error| persistence_error("save tool call history", error))?;
        Ok(())
    }

    fn conn(&self) -> CoreResult<std::sync::MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|_| CoreError::new(CoreErrorKind::InternalError, "sqlite lock poisoned"))
    }
}

pub fn coordination_db_path(engine_data_dir: impl AsRef<Path>) -> PathBuf {
    engine_data_dir.as_ref().join(DB_FILE_NAME)
}

fn configure_connection(conn: &Connection) -> CoreResult<()> {
    conn.busy_timeout(Duration::from_millis(SQLITE_BUSY_TIMEOUT_MS))
        .map_err(|error| persistence_error("set sqlite busy timeout", error))?;
    conn.execute_batch(&format!(
        "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA foreign_keys = ON;
            PRAGMA temp_store = MEMORY;
            PRAGMA wal_autocheckpoint = {SQLITE_WAL_AUTOCHECKPOINT_PAGES};
            "
    ))
    .map_err(|error| persistence_error("configure sqlite connection", error))
}

fn prepare_migration_metadata(conn: &Connection) -> CoreResult<()> {
    conn.execute_batch(
        "
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                description TEXT NOT NULL DEFAULT '',
                applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            ",
    )
    .map_err(|error| persistence_error("prepare schema migration metadata", error))?;
    add_missing_column(
        conn,
        "schema_migrations",
        "description",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    reject_unsupported_unversioned_schema(conn)
}

fn database_size(conn: &Connection) -> CoreResult<RuntimeDatabaseSize> {
    let page_count = pragma_u64(conn, "page_count")?;
    let page_size_bytes = pragma_u64(conn, "page_size")?;
    let freelist_pages = pragma_u64(conn, "freelist_count")?;
    let database_bytes = page_count.saturating_mul(page_size_bytes);
    let freelist_bytes = freelist_pages.saturating_mul(page_size_bytes);
    let wal_bytes = database_path(conn)?
        .and_then(|path| fs::metadata(format!("{}-wal", path.display())).ok())
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    Ok(RuntimeDatabaseSize {
        database_bytes,
        page_count,
        page_size_bytes,
        freelist_pages,
        freelist_bytes,
        wal_bytes,
    })
}

fn pragma_u64(conn: &Connection, name: &str) -> CoreResult<u64> {
    let value = conn
        .query_row(&format!("PRAGMA {name}"), [], |row| row.get::<_, i64>(0))
        .map_err(|error| persistence_error("read sqlite pragma", error))?;
    Ok(value as u64)
}

fn database_path(conn: &Connection) -> CoreResult<Option<PathBuf>> {
    let path = conn
        .query_row("PRAGMA database_list", [], |row| row.get::<_, String>(2))
        .map_err(|error| persistence_error("read sqlite database path", error))?;
    if path.is_empty() {
        Ok(None)
    } else {
        Ok(Some(PathBuf::from(path)))
    }
}

fn hot_query_plan_checks(conn: &Connection) -> CoreResult<Vec<RuntimeQueryPlanCheck>> {
    const HOT_QUERIES: &[(&str, &str)] = &[
        (
            "pending_queue_by_agent",
            "SELECT message_id FROM queued_messages
             WHERE owner_agent_id = 'agent-alpha' AND state = 'pending'
             ORDER BY expires_at ASC LIMIT 10",
        ),
        (
            "worker_runs_by_parent_status",
            "SELECT run_id FROM worker_runs
             WHERE session_id = 'session-alpha' AND status = 'running'
             ORDER BY created_at ASC, run_id ASC LIMIT 10",
        ),
        (
            "messages_by_correlation",
            "SELECT sequence FROM agent_messages
             WHERE correlation_id = 'corr-alpha'
             ORDER BY sequence ASC LIMIT 10",
        ),
        (
            "completion_packets_by_session",
            "SELECT sequence FROM completion_packets
             WHERE session_id = 'session-alpha'
             ORDER BY sequence ASC LIMIT 10",
        ),
        (
            "event_session_lookup",
            "SELECT sequence FROM event_session_index
             WHERE session_id = 'session-alpha'
             ORDER BY sequence ASC LIMIT 10",
        ),
    ];

    HOT_QUERIES
        .iter()
        .map(|(name, sql)| query_plan_check(conn, name, sql))
        .collect()
}

fn query_plan_check(
    conn: &Connection,
    name: &'static str,
    sql: &str,
) -> CoreResult<RuntimeQueryPlanCheck> {
    let mut stmt = conn
        .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
        .map_err(|error| persistence_error("prepare hot query plan", error))?;
    let details = stmt
        .query_map([], |row| row.get::<_, String>(3))
        .map_err(|error| persistence_error("run hot query plan", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("read hot query plan", error))?;
    let detail = details.join(" | ");
    let uses_index = detail.contains("USING INDEX") || detail.contains("USING COVERING INDEX");
    Ok(RuntimeQueryPlanCheck {
        name,
        uses_index,
        detail,
    })
}

fn apply_schema_migrations(
    conn: &mut Connection,
    migrations: &[SchemaMigration],
) -> CoreResult<()> {
    validate_migration_catalog(migrations)?;
    let current_version = current_schema_version(conn)?;
    if current_version > CURRENT_SCHEMA_VERSION {
        return Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!(
                "sqlite schema version {current_version} is newer than supported version {CURRENT_SCHEMA_VERSION}"
            ),
        ));
    }
    if current_version > 0 && current_version < MIN_SUPPORTED_SCHEMA_VERSION {
        return Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!(
                "sqlite schema version {current_version} is older than supported version {MIN_SUPPORTED_SCHEMA_VERSION}"
            ),
        ));
    }

    for migration in migrations {
        if migration.version <= current_version {
            conn.execute(
                "UPDATE schema_migrations SET description = ?1 WHERE version = ?2",
                params![migration.description, migration.version],
            )
            .map_err(|error| persistence_error("refresh schema migration metadata", error))?;
            continue;
        }

        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start schema migration", error))?;
        (migration.apply)(&tx)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, description) VALUES (?1, ?2)",
            params![migration.version, migration.description],
        )
        .map_err(|error| persistence_error("record schema migration", error))?;
        tx.commit()
            .map_err(|error| persistence_error("commit schema migration", error))?;
    }
    Ok(())
}

fn validate_migration_catalog(migrations: &[SchemaMigration]) -> CoreResult<()> {
    for (index, migration) in migrations.iter().enumerate() {
        let expected = (index as i64) + 1;
        if migration.version != expected {
            return Err(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!(
                    "invalid schema migration catalog: expected version {expected}, found {}",
                    migration.version
                ),
            ));
        }
    }
    Ok(())
}

fn current_schema_version(conn: &Connection) -> CoreResult<i64> {
    conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get::<_, i64>(0),
    )
    .map_err(|error| persistence_error("read schema version", error))
}

fn load_schema_migration_records(conn: &Connection) -> CoreResult<Vec<SchemaMigrationRecord>> {
    let mut stmt = conn
        .prepare(
            "SELECT version, description, applied_at
             FROM schema_migrations
             ORDER BY version ASC",
        )
        .map_err(|error| persistence_error("prepare schema migration records", error))?;
    let rows = stmt
        .query_map([], |row| {
            Ok(SchemaMigrationRecord {
                version: row.get(0)?,
                description: row.get(1)?,
                applied_at: row.get(2)?,
            })
        })
        .map_err(|error| persistence_error("query schema migration records", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load schema migration records", error))
}

fn reject_unsupported_unversioned_schema(conn: &Connection) -> CoreResult<()> {
    let has_runtime_tables = runtime_table_names(conn)?.iter().any(|table| {
        matches!(
            table.as_str(),
            "sessions"
                | "agents"
                | "agent_instances"
                | "session_configs"
                | "session_identity"
                | "event_history"
                | "agent_messages"
                | "worker_runs"
                | "completion_packets"
                | "tool_call_history"
                | "runtime_counters"
                | "queued_messages"
                | "runtime_search_fts"
                | "runtime_import_batches"
                | "legacy_id_mappings"
                | "profile_memories"
                | "channel_bindings"
                | "mcp_bindings"
        )
    });
    let has_migration_records = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM schema_migrations)",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| persistence_error("check schema migration records", error))?
        != 0;

    if has_runtime_tables && !has_migration_records {
        return Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            "unsupported unversioned sqlite coordination schema",
        ));
    }
    Ok(())
}

fn runtime_table_names(conn: &Connection) -> CoreResult<Vec<String>> {
    let mut stmt = conn
        .prepare(
            "SELECT name FROM sqlite_schema
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
             ORDER BY name ASC",
        )
        .map_err(|error| persistence_error("prepare sqlite table names", error))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| persistence_error("query sqlite table names", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("read sqlite table names", error))
}

fn migrate_v1_create_base_tables(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "

            CREATE TABLE IF NOT EXISTS sessions (
                session_id TEXT PRIMARY KEY,
                handle INTEGER NOT NULL UNIQUE,
                agent_id TEXT NOT NULL,
                profile_id TEXT NOT NULL,
                kind_json TEXT NOT NULL,
                status_json TEXT NOT NULL,
                brain_turn_count INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                last_active_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS event_history (
                sequence INTEGER PRIMARY KEY,
                event_kind TEXT NOT NULL,
                event_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS agent_messages (
                sequence INTEGER PRIMARY KEY,
                from_agent TEXT NOT NULL,
                to_agent TEXT NOT NULL,
                body TEXT NOT NULL,
                correlation_id TEXT,
                message_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS worker_runs (
                run_id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                profile_id TEXT NOT NULL,
                task_id TEXT,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                last_updated_at TEXT NOT NULL,
                source_wake_id TEXT NOT NULL,
                source_action_index INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS completion_packets (
                sequence INTEGER PRIMARY KEY,
                session_id TEXT NOT NULL,
                status TEXT NOT NULL,
                summary TEXT NOT NULL,
                packet_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS tool_call_history (
                sequence INTEGER PRIMARY KEY,
                session_id TEXT NOT NULL,
                wake_id TEXT,
                tool_name TEXT NOT NULL,
                phase TEXT NOT NULL,
                is_error INTEGER,
                metadata_json TEXT
            );
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 1", error))
}

fn migrate_v2_add_delegation_columns(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    add_missing_column(tx, "sessions", "delegation_json", "TEXT")?;
    add_missing_column(tx, "sessions", "resource_limits_json", "TEXT")?;
    add_missing_column(tx, "sessions", "tool_profile_json", "TEXT")?;
    add_missing_column(tx, "worker_runs", "delegated_session_id", "TEXT")?;
    add_missing_column(tx, "worker_runs", "parent_agent_id", "TEXT")?;
    add_missing_column(tx, "worker_runs", "delegation_correlation_id", "TEXT")?;
    add_missing_column(
        tx,
        "worker_runs",
        "parent_consumption",
        "TEXT NOT NULL DEFAULT 'await_completion'",
    )?;
    add_missing_column(tx, "worker_runs", "fan_out_group_id", "TEXT")?;
    add_missing_column(tx, "worker_runs", "fan_out_max_concurrency", "INTEGER")?;
    add_missing_column(
        tx,
        "worker_runs",
        "fan_out_failure_policy",
        "TEXT NOT NULL DEFAULT 'fail_soft'",
    )
}

fn migrate_v3_add_identity_tables(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS agents (
                agent_id TEXT PRIMARY KEY,
                display_label TEXT NOT NULL,
                profile_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                status TEXT NOT NULL,
                source_system TEXT,
                source_external_id TEXT,
                den_project_id TEXT,
                den_task_id TEXT,
                created_at TEXT NOT NULL,
                archived_at TEXT
            );

            CREATE TABLE IF NOT EXISTS agent_instances (
                instance_id TEXT PRIMARY KEY,
                agent_id TEXT NOT NULL,
                display_label TEXT NOT NULL,
                profile_id TEXT NOT NULL,
                status TEXT NOT NULL,
                source_system TEXT,
                source_external_id TEXT,
                den_project_id TEXT,
                den_task_id TEXT,
                created_at TEXT NOT NULL,
                last_active_at TEXT NOT NULL,
                archived_at TEXT
            );

            CREATE TABLE IF NOT EXISTS session_identity (
                session_id TEXT PRIMARY KEY,
                instance_id TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                profile_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                status TEXT NOT NULL,
                source_system TEXT,
                source_external_id TEXT,
                den_project_id TEXT,
                den_task_id TEXT,
                created_at TEXT NOT NULL,
                last_active_at TEXT NOT NULL,
                archived_at TEXT
            );
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 3", error))
}

fn migrate_v4_add_session_config_snapshots(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS session_configs (
                session_id TEXT PRIMARY KEY,
                profile_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                resource_limits_json TEXT NOT NULL,
                tool_profile_json TEXT NOT NULL,
                config_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 4", error))
}

fn migrate_v5_add_event_projection_indexes(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    add_missing_column(
        tx,
        "event_history",
        "recorded_at",
        "TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP",
    )?;
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS event_session_index (
                sequence INTEGER NOT NULL,
                session_id TEXT NOT NULL,
                PRIMARY KEY (sequence, session_id)
            );
            CREATE INDEX IF NOT EXISTS idx_event_session_index_session
                ON event_session_index(session_id, sequence);

            CREATE TABLE IF NOT EXISTS event_agent_index (
                sequence INTEGER NOT NULL,
                agent_id TEXT NOT NULL,
                PRIMARY KEY (sequence, agent_id)
            );
            CREATE INDEX IF NOT EXISTS idx_event_agent_index_agent
                ON event_agent_index(agent_id, sequence);

            CREATE TABLE IF NOT EXISTS event_instance_index (
                sequence INTEGER NOT NULL,
                instance_id TEXT NOT NULL,
                PRIMARY KEY (sequence, instance_id)
            );
            CREATE INDEX IF NOT EXISTS idx_event_instance_index_instance
                ON event_instance_index(instance_id, sequence);

            CREATE TABLE IF NOT EXISTS event_correlation_index (
                sequence INTEGER NOT NULL,
                correlation_id TEXT NOT NULL,
                PRIMARY KEY (sequence, correlation_id)
            );
            CREATE INDEX IF NOT EXISTS idx_event_correlation_index_correlation
                ON event_correlation_index(correlation_id, sequence);

            CREATE TABLE IF NOT EXISTS event_wake_index (
                sequence INTEGER NOT NULL,
                source_wake_id TEXT NOT NULL,
                PRIMARY KEY (sequence, source_wake_id)
            );
            CREATE INDEX IF NOT EXISTS idx_event_wake_index_wake
                ON event_wake_index(source_wake_id, sequence);
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 5", error))
}

fn migrate_v6_add_runtime_search_index(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE VIRTUAL TABLE IF NOT EXISTS runtime_search_fts USING fts5(
                row_type UNINDEXED,
                row_key UNINDEXED,
                sequence UNINDEXED,
                session_id UNINDEXED,
                agent_id UNINDEXED,
                instance_id UNINDEXED,
                task_id UNINDEXED,
                event_kind UNINDEXED,
                recorded_at UNINDEXED,
                title,
                body
            );
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 6", error))
}

fn migrate_v7_add_runtime_counters(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS runtime_counters (
                scope_type TEXT NOT NULL,
                scope_id TEXT NOT NULL,
                counter_name TEXT NOT NULL,
                value INTEGER NOT NULL,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (scope_type, scope_id, counter_name)
            );
            CREATE INDEX IF NOT EXISTS idx_runtime_counters_scope
                ON runtime_counters(scope_type, scope_id);
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 7", error))
}

fn migrate_v8_add_queued_message_retention(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS queued_messages (
                message_id TEXT PRIMARY KEY,
                owner_session_id TEXT,
                owner_agent_id TEXT NOT NULL,
                from_agent TEXT NOT NULL,
                to_agent TEXT NOT NULL,
                body TEXT NOT NULL,
                correlation_id TEXT,
                source_sequence INTEGER,
                enqueued_at TEXT NOT NULL,
                expires_at TEXT NOT NULL,
                ttl_ms INTEGER NOT NULL,
                delivery_attempts INTEGER NOT NULL DEFAULT 0,
                state TEXT NOT NULL,
                terminal_at TEXT,
                state_reason TEXT,
                message_json TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_queued_messages_state_expiry
                ON queued_messages(state, expires_at);
            CREATE INDEX IF NOT EXISTS idx_queued_messages_owner_agent
                ON queued_messages(owner_agent_id, state, expires_at);
            CREATE INDEX IF NOT EXISTS idx_queued_messages_owner_session
                ON queued_messages(owner_session_id, state, expires_at);
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 8", error))
}

fn migrate_v9_add_scale_guardrail_indexes(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE INDEX IF NOT EXISTS idx_sessions_agent_profile_handle
                ON sessions(agent_id, profile_id, handle);
            CREATE INDEX IF NOT EXISTS idx_sessions_profile_handle
                ON sessions(profile_id, handle);
            CREATE INDEX IF NOT EXISTS idx_agent_instances_agent_status
                ON agent_instances(agent_id, status, instance_id);
            CREATE INDEX IF NOT EXISTS idx_agent_messages_from_sequence
                ON agent_messages(from_agent, sequence);
            CREATE INDEX IF NOT EXISTS idx_agent_messages_to_sequence
                ON agent_messages(to_agent, sequence);
            CREATE INDEX IF NOT EXISTS idx_agent_messages_correlation_sequence
                ON agent_messages(correlation_id, sequence);
            CREATE INDEX IF NOT EXISTS idx_completion_packets_session_sequence
                ON completion_packets(session_id, sequence);
            CREATE INDEX IF NOT EXISTS idx_worker_runs_parent_status_created
                ON worker_runs(session_id, status, created_at, run_id);
            CREATE INDEX IF NOT EXISTS idx_worker_runs_delegated_session
                ON worker_runs(delegated_session_id);
            CREATE INDEX IF NOT EXISTS idx_worker_runs_profile_task_created
                ON worker_runs(profile_id, task_id, created_at, run_id);
            CREATE INDEX IF NOT EXISTS idx_tool_call_history_session_sequence
                ON tool_call_history(session_id, sequence);
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 9", error))
}

fn migrate_v10_add_legacy_runtime_import_metadata(
    tx: &rusqlite::Transaction<'_>,
) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS runtime_import_batches (
                import_batch_id TEXT PRIMARY KEY,
                source_system TEXT NOT NULL,
                source_label TEXT NOT NULL,
                source_snapshot_ref TEXT,
                notes TEXT,
                imported_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_runtime_import_batches_source
                ON runtime_import_batches(source_system, imported_at);

            CREATE TABLE IF NOT EXISTS legacy_id_mappings (
                import_batch_id TEXT NOT NULL,
                source_system TEXT NOT NULL,
                legacy_kind TEXT NOT NULL,
                legacy_id TEXT NOT NULL,
                rusty_kind TEXT NOT NULL,
                rusty_id TEXT NOT NULL,
                provenance_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                PRIMARY KEY (source_system, legacy_kind, legacy_id),
                FOREIGN KEY (import_batch_id)
                    REFERENCES runtime_import_batches(import_batch_id)
            );
            CREATE INDEX IF NOT EXISTS idx_legacy_id_mappings_batch
                ON legacy_id_mappings(import_batch_id, legacy_kind);
            CREATE INDEX IF NOT EXISTS idx_legacy_id_mappings_rusty
                ON legacy_id_mappings(rusty_kind, rusty_id);
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 10", error))
}

fn migrate_v11_add_external_bindings(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS channel_bindings (
                binding_id TEXT PRIMARY KEY,
                adapter_id TEXT NOT NULL,
                provider TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                instance_id TEXT,
                session_id TEXT,
                profile_id TEXT NOT NULL,
                external_channel_id TEXT NOT NULL,
                external_thread_id TEXT,
                external_user_id TEXT,
                provider_subscription_id TEXT,
                cursor TEXT,
                membership_state TEXT,
                presence_state TEXT,
                status TEXT NOT NULL,
                degraded_reason TEXT,
                provenance_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_channel_bindings_agent_provider
                ON channel_bindings(agent_id, provider, status);
            CREATE INDEX IF NOT EXISTS idx_channel_bindings_profile_agent
                ON channel_bindings(profile_id, agent_id, status);
            CREATE INDEX IF NOT EXISTS idx_channel_bindings_session
                ON channel_bindings(session_id, status);
            CREATE INDEX IF NOT EXISTS idx_channel_bindings_external
                ON channel_bindings(provider, external_channel_id, external_thread_id);

            CREATE TABLE IF NOT EXISTS mcp_bindings (
                binding_id TEXT PRIMARY KEY,
                adapter_id TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                instance_id TEXT,
                session_id TEXT,
                profile_id TEXT NOT NULL,
                server_names_json TEXT NOT NULL,
                endpoint_ref TEXT NOT NULL,
                transport TEXT NOT NULL,
                tool_profile_key TEXT NOT NULL,
                discovered_tool_revision TEXT,
                status TEXT NOT NULL,
                degraded_reason TEXT,
                diagnostics_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_mcp_bindings_agent_profile
                ON mcp_bindings(agent_id, profile_id, status);
            CREATE INDEX IF NOT EXISTS idx_mcp_bindings_session
                ON mcp_bindings(session_id, status);
            CREATE INDEX IF NOT EXISTS idx_mcp_bindings_adapter
                ON mcp_bindings(adapter_id, status);
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 11", error))
}

fn migrate_v12_add_tool_call_metadata(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    add_missing_column(tx, "tool_call_history", "metadata_json", "TEXT")
}

fn migrate_v13_add_profile_memory(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS profile_memories (
                profile_id TEXT NOT NULL,
                target_type TEXT NOT NULL,
                target_id TEXT NOT NULL,
                memory_key TEXT NOT NULL,
                content TEXT NOT NULL,
                metadata_json TEXT NOT NULL,
                revision INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (profile_id, target_type, target_id, memory_key)
            );
            CREATE INDEX IF NOT EXISTS idx_profile_memories_profile_updated
                ON profile_memories(profile_id, updated_at DESC);
            CREATE INDEX IF NOT EXISTS idx_profile_memories_target
                ON profile_memories(profile_id, target_type, target_id, memory_key);
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 13", error))
}

fn migrate_v14_add_scheduler_persistence(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS scheduled_jobs (
                job_id TEXT PRIMARY KEY,
                job_kind TEXT NOT NULL,
                target_session_id TEXT,
                interval_ms INTEGER,
                next_due_at TEXT,
                payload_json TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                paused_at TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_scheduled_jobs_due
                ON scheduled_jobs(status, next_due_at, job_id);
            CREATE INDEX IF NOT EXISTS idx_scheduled_jobs_kind_status
                ON scheduled_jobs(job_kind, status, job_id);

            CREATE TABLE IF NOT EXISTS scheduled_job_runs (
                run_id TEXT PRIMARY KEY,
                job_id TEXT NOT NULL,
                job_kind TEXT NOT NULL,
                target_session_id TEXT,
                status TEXT NOT NULL,
                trigger_kind TEXT NOT NULL,
                scheduled_for TEXT,
                claimed_at TEXT NOT NULL,
                claim_deadline_at TEXT NOT NULL,
                completed_at TEXT,
                error TEXT,
                output_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                FOREIGN KEY (job_id) REFERENCES scheduled_jobs(job_id)
            );
            CREATE INDEX IF NOT EXISTS idx_scheduled_job_runs_job_created
                ON scheduled_job_runs(job_id, created_at, run_id);
            CREATE INDEX IF NOT EXISTS idx_scheduled_job_runs_status_deadline
                ON scheduled_job_runs(status, claim_deadline_at, run_id);
            CREATE INDEX IF NOT EXISTS idx_scheduled_job_runs_target
                ON scheduled_job_runs(target_session_id, status, created_at);
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 14", error))
}

fn save_queued_message_in_tx(
    tx: &rusqlite::Transaction<'_>,
    record: &QueuedMessageRecord,
) -> CoreResult<()> {
    let message_json = to_json_text(&record.message)?;
    tx.execute(
        "INSERT INTO queued_messages (
            message_id,
            owner_session_id,
            owner_agent_id,
            from_agent,
            to_agent,
            body,
            correlation_id,
            source_sequence,
            enqueued_at,
            expires_at,
            ttl_ms,
            delivery_attempts,
            state,
            terminal_at,
            state_reason,
            message_json
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
        ON CONFLICT(message_id) DO UPDATE SET
            owner_session_id = excluded.owner_session_id,
            owner_agent_id = excluded.owner_agent_id,
            from_agent = excluded.from_agent,
            to_agent = excluded.to_agent,
            body = excluded.body,
            correlation_id = excluded.correlation_id,
            source_sequence = excluded.source_sequence,
            expires_at = excluded.expires_at,
            ttl_ms = excluded.ttl_ms,
            delivery_attempts = excluded.delivery_attempts,
            state = excluded.state,
            terminal_at = excluded.terminal_at,
            state_reason = excluded.state_reason,
            message_json = excluded.message_json",
        params![
            record.message_id,
            record
                .owner_session_id
                .as_ref()
                .map(|value| value.0.as_str()),
            record.owner_agent_id.0,
            record.message.from.0,
            record.message.to.0,
            record.message.body,
            record.message.correlation_id,
            record.source_sequence.map(|value| value as i64),
            record.enqueued_at,
            record.expires_at,
            record.ttl_ms as i64,
            record.delivery_attempts as i64,
            queued_message_state_as_str(record.state),
            record.terminal_at,
            record.state_reason,
            message_json,
        ],
    )
    .map_err(|error| persistence_error("save queued message", error))?;
    save_queued_message_search_row_in_tx(tx, record)
}

fn expire_queued_messages_in_tx(
    tx: &rusqlite::Transaction<'_>,
    now: &IsoTimestamp,
) -> CoreResult<Vec<QueuedMessageRecord>> {
    let expiring = load_queued_messages_in_tx(
        tx,
        &QueuedMessageFilter {
            state: Some(QueuedMessageState::Pending),
            owner_session_id: None,
            owner_agent_id: None,
            limit: None,
        },
    )?
    .into_iter()
    .filter(|message| message.expires_at <= *now)
    .collect::<Vec<_>>();

    for mut message in expiring.clone() {
        message.state = QueuedMessageState::Expired;
        message.terminal_at = Some(now.clone());
        message.state_reason = Some("ttl_expired".to_string());
        save_queued_message_in_tx(tx, &message)?;
        increment_counter_for_scopes_in_tx(
            tx,
            queued_message_counter_scopes(&message),
            COUNTER_QUEUE_EXPIRATIONS,
            1,
        )?;
    }
    Ok(expiring
        .into_iter()
        .map(|mut message| {
            message.state = QueuedMessageState::Expired;
            message.terminal_at = Some(now.clone());
            message.state_reason = Some("ttl_expired".to_string());
            message
        })
        .collect())
}

fn purge_terminal_queued_messages_in_tx(
    tx: &rusqlite::Transaction<'_>,
    cutoff: &IsoTimestamp,
) -> CoreResult<u64> {
    tx.execute(
        "DELETE FROM runtime_search_fts
         WHERE row_type = 'queue_message'
           AND row_key IN (
               SELECT message_id FROM queued_messages
               WHERE state IN ('delivered', 'expired', 'discarded', 'cancelled')
                 AND terminal_at IS NOT NULL
                 AND terminal_at < ?1
           )",
        params![cutoff],
    )
    .map_err(|error| persistence_error("delete purged queue search rows", error))?;
    let purged = tx
        .execute(
            "DELETE FROM queued_messages
             WHERE state IN ('delivered', 'expired', 'discarded', 'cancelled')
               AND terminal_at IS NOT NULL
               AND terminal_at < ?1",
            params![cutoff],
        )
        .map_err(|error| persistence_error("purge terminal queued messages", error))?;
    Ok(purged as u64)
}

fn load_queued_messages(
    conn: &Connection,
    filter: &QueuedMessageFilter,
) -> CoreResult<Vec<QueuedMessageRecord>> {
    load_queued_messages_with_conn(conn, filter)
}

fn load_queued_messages_in_tx(
    tx: &rusqlite::Transaction<'_>,
    filter: &QueuedMessageFilter,
) -> CoreResult<Vec<QueuedMessageRecord>> {
    load_queued_messages_with_conn(tx, filter)
}

fn load_queued_messages_with_conn(
    conn: &Connection,
    filter: &QueuedMessageFilter,
) -> CoreResult<Vec<QueuedMessageRecord>> {
    let state = filter.state.map(queued_message_state_as_str);
    let owner_session_id = filter
        .owner_session_id
        .as_ref()
        .map(|value| value.0.as_str());
    let owner_agent_id = filter.owner_agent_id.as_ref().map(|value| value.0.as_str());
    let limit = filter.limit.unwrap_or(1_000).clamp(1, 10_000) as i64;
    let mut stmt = conn
        .prepare(
            "SELECT
                message_id,
                owner_session_id,
                owner_agent_id,
                from_agent,
                to_agent,
                body,
                correlation_id,
                source_sequence,
                enqueued_at,
                expires_at,
                ttl_ms,
                delivery_attempts,
                state,
                terminal_at,
                state_reason,
                message_json
             FROM queued_messages
             WHERE (?1 IS NULL OR state = ?1)
               AND (?2 IS NULL OR owner_session_id = ?2)
               AND (?3 IS NULL OR owner_agent_id = ?3)
             ORDER BY enqueued_at ASC, message_id ASC
             LIMIT ?4",
        )
        .map_err(|error| persistence_error("prepare queued message query", error))?;
    let rows = stmt
        .query_map(
            params![state, owner_session_id, owner_agent_id, limit],
            row_to_queued_message,
        )
        .map_err(|error| persistence_error("query queued messages", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load queued messages", error))
}

fn row_to_queued_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<QueuedMessageRecord> {
    let message_json: String = row.get(15)?;
    let state: String = row.get(12)?;
    Ok(QueuedMessageRecord {
        message_id: row.get(0)?,
        owner_session_id: row.get::<_, Option<String>>(1)?.map(SessionId),
        owner_agent_id: AgentId(row.get(2)?),
        message: from_json_text(&message_json).map_err(to_sql_error)?,
        source_sequence: row.get::<_, Option<i64>>(7)?.map(|value| value as u64),
        enqueued_at: row.get(8)?,
        expires_at: row.get(9)?,
        ttl_ms: row.get::<_, i64>(10)? as u32,
        delivery_attempts: row.get::<_, i64>(11)? as u32,
        state: queued_message_state_from_str(&state)?,
        terminal_at: row.get(13)?,
        state_reason: row.get(14)?,
    })
}

fn save_scheduled_job(conn: &Connection, record: &ScheduledJobRecord) -> CoreResult<()> {
    conn.execute(
        "INSERT OR REPLACE INTO scheduled_jobs (
            job_id,
            job_kind,
            target_session_id,
            interval_ms,
            next_due_at,
            payload_json,
            status,
            created_at,
            updated_at,
            paused_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            record.job_id.as_str(),
            record.job_kind.as_str(),
            record
                .target_session_id
                .as_ref()
                .map(|session_id| session_id.0.as_str()),
            record.interval_ms.map(|value| value as i64),
            record.next_due_at.as_deref(),
            to_json_text(&record.payload_json)?,
            scheduled_job_status_as_str(record.status),
            record.created_at.as_str(),
            record.updated_at.as_str(),
            record.paused_at.as_deref(),
        ],
    )
    .map_err(|error| persistence_error("save scheduled job", error))?;
    Ok(())
}

fn load_scheduled_job(conn: &Connection, job_id: &str) -> CoreResult<Option<ScheduledJobRecord>> {
    conn.query_row(
        "SELECT
            job_id,
            job_kind,
            target_session_id,
            interval_ms,
            next_due_at,
            payload_json,
            status,
            created_at,
            updated_at,
            paused_at
         FROM scheduled_jobs
         WHERE job_id = ?1",
        params![job_id],
        row_to_scheduled_job,
    )
    .optional()
    .map_err(|error| persistence_error("load scheduled job", error))
}

fn query_scheduled_jobs(
    conn: &Connection,
    query: &ScheduledJobQuery,
) -> CoreResult<Vec<ScheduledJobRecord>> {
    let status = query.status.map(scheduled_job_status_as_str);
    let job_kind = query.job_kind.as_deref();
    let due_at_or_before = query.due_at_or_before.as_deref();
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let mut stmt = conn
        .prepare(
            "SELECT
                job_id,
                job_kind,
                target_session_id,
                interval_ms,
                next_due_at,
                payload_json,
                status,
                created_at,
                updated_at,
                paused_at
             FROM scheduled_jobs
             WHERE (?1 IS NULL OR status = ?1)
               AND (?2 IS NULL OR job_kind = ?2)
               AND (?3 IS NULL OR (next_due_at IS NOT NULL AND next_due_at <= ?3))
             ORDER BY COALESCE(next_due_at, created_at) ASC, job_id ASC
             LIMIT ?4 OFFSET ?5",
        )
        .map_err(|error| persistence_error("prepare scheduled jobs query", error))?;
    let rows = stmt
        .query_map(
            params![status, job_kind, due_at_or_before, limit, offset],
            row_to_scheduled_job,
        )
        .map_err(|error| persistence_error("query scheduled jobs", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load scheduled jobs", error))
}

fn row_to_scheduled_job(row: &rusqlite::Row<'_>) -> rusqlite::Result<ScheduledJobRecord> {
    let payload_json: String = row.get(5)?;
    let status: String = row.get(6)?;
    Ok(ScheduledJobRecord {
        job_id: row.get(0)?,
        job_kind: row.get(1)?,
        target_session_id: row.get::<_, Option<String>>(2)?.map(SessionId),
        interval_ms: row.get::<_, Option<i64>>(3)?.map(|value| value as u64),
        next_due_at: row.get(4)?,
        payload_json: from_json_text(&payload_json).map_err(to_sql_error)?,
        status: scheduled_job_status_from_str(&status)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        paused_at: row.get(9)?,
    })
}

fn save_scheduled_run_in_tx(
    tx: &rusqlite::Transaction<'_>,
    run: &ScheduledRunRecord,
) -> CoreResult<()> {
    tx.execute(
        "INSERT INTO scheduled_job_runs (
            run_id,
            job_id,
            job_kind,
            target_session_id,
            status,
            trigger_kind,
            scheduled_for,
            claimed_at,
            claim_deadline_at,
            completed_at,
            error,
            output_json,
            created_at,
            updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        params![
            run.run_id.0.as_str(),
            run.job_id.as_str(),
            run.job_kind.as_str(),
            run.target_session_id
                .as_ref()
                .map(|session_id| session_id.0.as_str()),
            scheduled_run_status_as_str(run.status),
            scheduled_run_trigger_as_str(run.trigger),
            run.scheduled_for.as_deref(),
            run.claimed_at.as_str(),
            run.claim_deadline_at.as_str(),
            run.completed_at.as_deref(),
            run.error.as_deref(),
            to_json_text(&run.output_json)?,
            run.created_at.as_str(),
            run.updated_at.as_str(),
        ],
    )
    .map_err(|error| persistence_error("save scheduled run", error))?;
    Ok(())
}

fn query_scheduled_runs(
    conn: &Connection,
    query: &ScheduledRunQuery,
) -> CoreResult<Vec<ScheduledRunRecord>> {
    let job_id = query.job_id.as_deref();
    let status = query.status.map(scheduled_run_status_as_str);
    let trigger = query.trigger.map(scheduled_run_trigger_as_str);
    let target_session_id = query
        .target_session_id
        .as_ref()
        .map(|session_id| session_id.0.as_str());
    let stale_before = query.stale_claim_deadline_before.as_deref();
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let mut stmt = conn
        .prepare(
            "SELECT
                run_id,
                job_id,
                job_kind,
                target_session_id,
                status,
                trigger_kind,
                scheduled_for,
                claimed_at,
                claim_deadline_at,
                completed_at,
                error,
                output_json,
                created_at,
                updated_at
             FROM scheduled_job_runs
             WHERE (?1 IS NULL OR job_id = ?1)
               AND (?2 IS NULL OR status = ?2)
               AND (?3 IS NULL OR trigger_kind = ?3)
               AND (?4 IS NULL OR target_session_id = ?4)
               AND (?5 IS NULL OR claim_deadline_at < ?5)
             ORDER BY created_at ASC, run_id ASC
             LIMIT ?6 OFFSET ?7",
        )
        .map_err(|error| persistence_error("prepare scheduled runs query", error))?;
    let rows = stmt
        .query_map(
            params![
                job_id,
                status,
                trigger,
                target_session_id,
                stale_before,
                limit,
                offset,
            ],
            row_to_scheduled_run,
        )
        .map_err(|error| persistence_error("query scheduled runs", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load scheduled runs", error))
}

fn row_to_scheduled_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<ScheduledRunRecord> {
    let status: String = row.get(4)?;
    let trigger: String = row.get(5)?;
    let output_json: String = row.get(11)?;
    Ok(ScheduledRunRecord {
        run_id: RunId(row.get(0)?),
        job_id: row.get(1)?,
        job_kind: row.get(2)?,
        target_session_id: row.get::<_, Option<String>>(3)?.map(SessionId),
        status: scheduled_run_status_from_str(&status)?,
        trigger: scheduled_run_trigger_from_str(&trigger)?,
        scheduled_for: row.get(6)?,
        claimed_at: row.get(7)?,
        claim_deadline_at: row.get(8)?,
        completed_at: row.get(9)?,
        error: row.get(10)?,
        output_json: from_json_text(&output_json).map_err(to_sql_error)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

fn scheduled_job_status_as_str(status: ScheduledJobStatus) -> &'static str {
    match status {
        ScheduledJobStatus::Active => "active",
        ScheduledJobStatus::Paused => "paused",
        ScheduledJobStatus::Archived => "archived",
    }
}

fn scheduled_job_status_from_str(raw: &str) -> rusqlite::Result<ScheduledJobStatus> {
    match raw {
        "active" => Ok(ScheduledJobStatus::Active),
        "paused" => Ok(ScheduledJobStatus::Paused),
        "archived" => Ok(ScheduledJobStatus::Archived),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::<dyn std::error::Error + Send + Sync>::from(format!(
                "unknown scheduled job status {other}",
            )),
        )),
    }
}

fn scheduled_run_status_as_str(status: ScheduledRunStatus) -> &'static str {
    match status {
        ScheduledRunStatus::Claimed => "claimed",
        ScheduledRunStatus::Completed => "completed",
        ScheduledRunStatus::Skipped => "skipped",
        ScheduledRunStatus::Failed => "failed",
        ScheduledRunStatus::Expired => "expired",
        ScheduledRunStatus::Cancelled => "cancelled",
    }
}

fn scheduled_run_status_from_str(raw: &str) -> rusqlite::Result<ScheduledRunStatus> {
    match raw {
        "claimed" => Ok(ScheduledRunStatus::Claimed),
        "completed" => Ok(ScheduledRunStatus::Completed),
        "skipped" => Ok(ScheduledRunStatus::Skipped),
        "failed" => Ok(ScheduledRunStatus::Failed),
        "expired" => Ok(ScheduledRunStatus::Expired),
        "cancelled" => Ok(ScheduledRunStatus::Cancelled),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::<dyn std::error::Error + Send + Sync>::from(format!(
                "unknown scheduled run status {other}",
            )),
        )),
    }
}

fn scheduled_run_trigger_as_str(trigger: ScheduledRunTrigger) -> &'static str {
    match trigger {
        ScheduledRunTrigger::Due => "due",
        ScheduledRunTrigger::Manual => "manual",
    }
}

fn scheduled_run_trigger_from_str(raw: &str) -> rusqlite::Result<ScheduledRunTrigger> {
    match raw {
        "due" => Ok(ScheduledRunTrigger::Due),
        "manual" => Ok(ScheduledRunTrigger::Manual),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::<dyn std::error::Error + Send + Sync>::from(format!(
                "unknown scheduled run trigger {other}",
            )),
        )),
    }
}

fn save_queued_message_search_row_in_tx(
    tx: &rusqlite::Transaction<'_>,
    record: &QueuedMessageRecord,
) -> CoreResult<()> {
    tx.execute(
        "DELETE FROM runtime_search_fts WHERE row_type = ?1 AND row_key = ?2",
        params!["queue_message", record.message_id],
    )
    .map_err(|error| persistence_error("delete queued message search row", error))?;
    insert_runtime_search_row(
        tx,
        &RuntimeSearchInsert {
            row_type: RuntimeSearchRowType::QueueMessage,
            row_key: record.message_id.clone(),
            sequence: record.source_sequence,
            session_id: record
                .owner_session_id
                .as_ref()
                .map(|value| value.0.clone()),
            agent_id: Some(record.owner_agent_id.0.clone()),
            instance_id: record
                .owner_session_id
                .as_ref()
                .map(|value| AgentInstanceId::new(format!("instance:{value}")).0),
            task_id: None,
            event_kind: Some(CoreEventKind::AgentMessageRouted),
            recorded_at: record.enqueued_at.clone(),
            title: format!(
                "queued message {}",
                queued_message_state_as_str(record.state)
            ),
            body: record.message.body.clone(),
        },
    )
}

fn queued_message_counter_scopes(message: &QueuedMessageRecord) -> Vec<RuntimeCounterScope> {
    let mut scopes = vec![
        RuntimeCounterScope::Runtime,
        RuntimeCounterScope::Agent(message.owner_agent_id.clone()),
    ];
    if let Some(session_id) = &message.owner_session_id {
        scopes.push(RuntimeCounterScope::Session(session_id.clone()));
        scopes.push(RuntimeCounterScope::Instance(AgentInstanceId::new(
            format!("instance:{session_id}"),
        )));
    }
    scopes
}

fn queued_message_state_as_str(state: QueuedMessageState) -> &'static str {
    match state {
        QueuedMessageState::Pending => "pending",
        QueuedMessageState::Delivered => "delivered",
        QueuedMessageState::Expired => "expired",
        QueuedMessageState::Discarded => "discarded",
        QueuedMessageState::Cancelled => "cancelled",
    }
}

fn queued_message_state_from_str(raw: &str) -> rusqlite::Result<QueuedMessageState> {
    match raw {
        "pending" => Ok(QueuedMessageState::Pending),
        "delivered" => Ok(QueuedMessageState::Delivered),
        "expired" => Ok(QueuedMessageState::Expired),
        "discarded" => Ok(QueuedMessageState::Discarded),
        "cancelled" => Ok(QueuedMessageState::Cancelled),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            12,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown queued message state {other}"),
            )),
        )),
    }
}

fn query_sessions(conn: &Connection, query: &SessionQuery) -> CoreResult<Vec<SessionState>> {
    let kind_json = query.kind.as_ref().map(to_json_text).transpose()?;
    let status_json = query.status.as_ref().map(to_json_text).transpose()?;
    let agent_id = query.agent_id.as_ref().map(|value| value.0.as_str());
    let profile_id = query.profile_id.as_ref().map(|value| value.0.as_str());
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let mut stmt = conn
        .prepare(
            "SELECT
                session_id,
                handle,
                agent_id,
                profile_id,
                kind_json,
                delegation_json,
                resource_limits_json,
                tool_profile_json,
                status_json,
                brain_turn_count,
                created_at,
                last_active_at
             FROM sessions
             WHERE (?1 IS NULL OR agent_id = ?1)
               AND (?2 IS NULL OR profile_id = ?2)
               AND (?3 IS NULL OR kind_json = ?3)
               AND (?4 IS NULL OR status_json = ?4)
             ORDER BY handle ASC
             LIMIT ?5 OFFSET ?6",
        )
        .map_err(|error| persistence_error("prepare query sessions", error))?;
    let rows = stmt
        .query_map(
            params![agent_id, profile_id, kind_json, status_json, limit, offset],
            row_to_session_state,
        )
        .map_err(|error| persistence_error("query sessions", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load queried sessions", error))
}

fn row_to_session_state(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionState> {
    let kind_json: String = row.get(4)?;
    let delegation_json: Option<String> = row.get(5)?;
    let resource_limits_json: Option<String> = row.get(6)?;
    let tool_profile_json: Option<String> = row.get(7)?;
    let status_json: String = row.get(8)?;
    Ok(SessionState {
        session_id: SessionId(row.get(0)?),
        handle: SessionHandle::new(row.get::<_, i64>(1)? as u64),
        agent_id: AgentId(row.get(2)?),
        profile_id: ProfileId(row.get(3)?),
        kind: from_json_text::<SessionKind>(&kind_json).map_err(to_sql_error)?,
        delegation: delegation_json
            .as_deref()
            .map(from_json_text::<DelegationLineage>)
            .transpose()
            .map_err(to_sql_error)?,
        resource_limits: resource_limits_json
            .as_deref()
            .map(from_json_text::<ResourceLimits>)
            .transpose()
            .map_err(to_sql_error)?
            .unwrap_or(ResourceLimits {
                workdir: None,
                max_duration_ms: None,
                max_delegation_depth: None,
            }),
        tool_profile: tool_profile_json
            .as_deref()
            .map(from_json_text::<ToolProfile>)
            .transpose()
            .map_err(to_sql_error)?
            .unwrap_or(ToolProfile { tools: Vec::new() }),
        status: from_json_text::<SessionStatus>(&status_json).map_err(to_sql_error)?,
        brain_turn_count: row.get::<_, i64>(9)? as u32,
        created_at: row.get(10)?,
        last_active_at: row.get(11)?,
    })
}

fn query_agent_instances(
    conn: &Connection,
    query: &AgentInstanceQuery,
) -> CoreResult<Vec<AgentInstanceRecord>> {
    let agent_id = query.agent_id.as_ref().map(|value| value.0.as_str());
    let profile_id = query.profile_id.as_ref().map(|value| value.0.as_str());
    let status = query.status.as_ref().map(durable_identity_status_as_str);
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let mut stmt = conn
        .prepare(
            "SELECT
                instance_id,
                agent_id,
                display_label,
                profile_id,
                status,
                source_system,
                source_external_id,
                den_project_id,
                den_task_id,
                created_at,
                last_active_at,
                archived_at
             FROM agent_instances
             WHERE (?1 IS NULL OR agent_id = ?1)
               AND (?2 IS NULL OR profile_id = ?2)
               AND (?3 IS NULL OR status = ?3)
             ORDER BY instance_id ASC
             LIMIT ?4 OFFSET ?5",
        )
        .map_err(|error| persistence_error("prepare query agent instances", error))?;
    let rows = stmt
        .query_map(
            params![agent_id, profile_id, status, limit, offset],
            row_to_agent_instance,
        )
        .map_err(|error| persistence_error("query agent instances", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load queried agent instances", error))
}

fn query_agent_messages(
    conn: &Connection,
    query: &AgentMessageQuery,
) -> CoreResult<Vec<AgentMessageRecord>> {
    let agent_id = query.agent_id.as_ref().map(|value| value.0.as_str());
    let correlation_id = query.correlation_id.as_deref();
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let mut stmt = conn
        .prepare(
            "SELECT sequence, message_json
             FROM agent_messages
             WHERE (?1 IS NULL OR from_agent = ?1 OR to_agent = ?1)
               AND (?2 IS NULL OR correlation_id = ?2)
             ORDER BY sequence ASC
             LIMIT ?3 OFFSET ?4",
        )
        .map_err(|error| persistence_error("prepare query agent messages", error))?;
    let rows = stmt
        .query_map(params![agent_id, correlation_id, limit, offset], |row| {
            let message_json: String = row.get(1)?;
            Ok(AgentMessageRecord {
                sequence: row.get::<_, i64>(0)? as u64,
                message: from_json_text(&message_json).map_err(to_sql_error)?,
            })
        })
        .map_err(|error| persistence_error("query agent messages", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load queried agent messages", error))
}

fn query_completion_packets(
    conn: &Connection,
    query: &CompletionPacketQuery,
) -> CoreResult<Vec<CompletionPacketRecord>> {
    let session_id = query.session_id.as_ref().map(|value| value.0.as_str());
    let status_json = query.status.as_ref().map(to_json_text).transpose()?;
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let mut stmt = conn
        .prepare(
            "SELECT sequence, packet_json
             FROM completion_packets
             WHERE (?1 IS NULL OR session_id = ?1)
               AND (?2 IS NULL OR status = ?2)
             ORDER BY sequence ASC
             LIMIT ?3 OFFSET ?4",
        )
        .map_err(|error| persistence_error("prepare query completion packets", error))?;
    let rows = stmt
        .query_map(params![session_id, status_json, limit, offset], |row| {
            let packet_json: String = row.get(1)?;
            Ok(CompletionPacketRecord {
                sequence: row.get::<_, i64>(0)? as u64,
                packet: from_json_text(&packet_json).map_err(to_sql_error)?,
            })
        })
        .map_err(|error| persistence_error("query completion packets", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load queried completion packets", error))
}

fn query_worker_runs(
    conn: &Connection,
    query: &WorkerRunQuery,
) -> CoreResult<Vec<WorkerRunRecord>> {
    let parent_session_id = query
        .parent_session_id
        .as_ref()
        .map(|value| value.0.as_str());
    let delegated_session_id = query
        .delegated_session_id
        .as_ref()
        .map(|value| value.0.as_str());
    let profile_id = query.profile_id.as_ref().map(|value| value.0.as_str());
    let task_id = query.task_id.as_ref().map(|value| value.0.as_str());
    let status = query.status.as_ref().map(WorkerRunStatus::as_str);
    let terminal = query
        .terminal
        .map(|value| if value { 1_i64 } else { 0_i64 });
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let mut stmt = conn
        .prepare(
            "SELECT
                run_id,
                session_id,
                delegated_session_id,
                parent_agent_id,
                profile_id,
                task_id,
                status,
                created_at,
                last_updated_at,
                source_wake_id,
                source_action_index,
                delegation_correlation_id,
                parent_consumption,
                fan_out_group_id,
                fan_out_max_concurrency,
                fan_out_failure_policy
             FROM worker_runs
             WHERE (?1 IS NULL OR session_id = ?1)
               AND (?2 IS NULL OR delegated_session_id = ?2)
               AND (?3 IS NULL OR profile_id = ?3)
               AND (?4 IS NULL OR task_id = ?4)
               AND (?5 IS NULL OR status = ?5)
               AND (
                   ?6 IS NULL
                   OR (?6 = 1 AND status IN ('completed', 'failed', 'blocked', 'exhausted', 'cancelled', 'expired'))
                   OR (?6 = 0 AND status NOT IN ('completed', 'failed', 'blocked', 'exhausted', 'cancelled', 'expired'))
               )
             ORDER BY created_at ASC, run_id ASC
             LIMIT ?7 OFFSET ?8",
        )
        .map_err(|error| persistence_error("prepare query worker runs", error))?;
    let rows = stmt
        .query_map(
            params![
                parent_session_id,
                delegated_session_id,
                profile_id,
                task_id,
                status,
                terminal,
                limit,
                offset,
            ],
            row_to_worker_run,
        )
        .map_err(|error| persistence_error("query worker runs", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load queried worker runs", error))
}

fn query_runtime_counters(
    conn: &Connection,
    query: &RuntimeCounterQuery,
) -> CoreResult<Vec<RuntimeCounterRecord>> {
    let scope_parts = query.scope.as_ref().map(runtime_counter_scope_parts);
    let scope_type = scope_parts.as_ref().map(|(scope_type, _)| *scope_type);
    let scope_id = scope_parts.as_ref().map(|(_, scope_id)| scope_id.as_str());
    let counter_name = query.counter_name.as_deref();
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(200, 5_000);
    let mut stmt = conn
        .prepare(
            "SELECT scope_type, scope_id, counter_name, value, updated_at
             FROM runtime_counters
             WHERE (?1 IS NULL OR scope_type = ?1)
               AND (?2 IS NULL OR scope_id = ?2)
               AND (?3 IS NULL OR counter_name = ?3)
             ORDER BY scope_type ASC, scope_id ASC, counter_name ASC
             LIMIT ?4 OFFSET ?5",
        )
        .map_err(|error| persistence_error("prepare query runtime counters", error))?;
    let rows = stmt
        .query_map(
            params![scope_type, scope_id, counter_name, limit, offset],
            row_to_runtime_counter,
        )
        .map_err(|error| persistence_error("query runtime counters", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load queried runtime counters", error))
}

fn reset_runtime_counters(
    conn: &Connection,
    query: &RuntimeCounterQuery,
    now: &IsoTimestamp,
) -> CoreResult<u64> {
    let scope_parts = query.scope.as_ref().map(runtime_counter_scope_parts);
    let scope_type = scope_parts.as_ref().map(|(scope_type, _)| *scope_type);
    let scope_id = scope_parts.as_ref().map(|(_, scope_id)| scope_id.as_str());
    let counter_name = query.counter_name.as_deref();
    let changed = conn
        .execute(
            "UPDATE runtime_counters
             SET value = 0, updated_at = ?4
             WHERE (?1 IS NULL OR scope_type = ?1)
               AND (?2 IS NULL OR scope_id = ?2)
               AND (?3 IS NULL OR counter_name = ?3)",
            params![scope_type, scope_id, counter_name, now],
        )
        .map_err(|error| persistence_error("reset runtime counters", error))?;
    Ok(changed as u64)
}

fn save_import_batch(conn: &Connection, record: &RuntimeImportBatchRecord) -> CoreResult<()> {
    conn.execute(
        "INSERT INTO runtime_import_batches (
            import_batch_id,
            source_system,
            source_label,
            source_snapshot_ref,
            notes,
            imported_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ON CONFLICT(import_batch_id) DO UPDATE SET
            source_system = excluded.source_system,
            source_label = excluded.source_label,
            source_snapshot_ref = excluded.source_snapshot_ref,
            notes = excluded.notes",
        params![
            record.import_batch_id,
            record.source_system,
            record.source_label,
            record.source_snapshot_ref,
            record.notes,
            record.imported_at,
        ],
    )
    .map_err(|error| persistence_error("save runtime import batch", error))?;
    Ok(())
}

fn load_import_batches(conn: &Connection) -> CoreResult<Vec<RuntimeImportBatchRecord>> {
    let mut stmt = conn
        .prepare(
            "SELECT
                import_batch_id,
                source_system,
                source_label,
                source_snapshot_ref,
                notes,
                imported_at
             FROM runtime_import_batches
             ORDER BY imported_at ASC, import_batch_id ASC",
        )
        .map_err(|error| persistence_error("prepare load runtime import batches", error))?;
    let rows = stmt
        .query_map([], row_to_import_batch)
        .map_err(|error| persistence_error("query runtime import batches", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load runtime import batches", error))
}

fn save_legacy_id_mapping(conn: &Connection, record: &LegacyIdMappingRecord) -> CoreResult<()> {
    let provenance_json = to_json_text(&record.provenance)?;
    conn.execute(
        "INSERT INTO legacy_id_mappings (
            import_batch_id,
            source_system,
            legacy_kind,
            legacy_id,
            rusty_kind,
            rusty_id,
            provenance_json,
            created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        ON CONFLICT(source_system, legacy_kind, legacy_id) DO UPDATE SET
            import_batch_id = excluded.import_batch_id,
            rusty_kind = excluded.rusty_kind,
            rusty_id = excluded.rusty_id,
            provenance_json = excluded.provenance_json",
        params![
            record.import_batch_id,
            record.source.system,
            runtime_object_kind_as_str(record.legacy_kind),
            record.source.external_id,
            runtime_object_kind_as_str(record.rusty_kind),
            record.rusty_id,
            provenance_json,
            record.created_at,
        ],
    )
    .map_err(|error| persistence_error("save legacy id mapping", error))?;
    Ok(())
}

fn query_legacy_id_mappings(
    conn: &Connection,
    query: &LegacyIdMappingQuery,
) -> CoreResult<Vec<LegacyIdMappingRecord>> {
    let import_batch_id = query.import_batch_id.as_deref();
    let source_system = query.source_system.as_deref();
    let legacy_kind = query.legacy_kind.map(runtime_object_kind_as_str);
    let rusty_kind = query.rusty_kind.map(runtime_object_kind_as_str);
    let rusty_id = query.rusty_id.as_deref();
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let mut stmt = conn
        .prepare(
            "SELECT
                import_batch_id,
                source_system,
                legacy_kind,
                legacy_id,
                rusty_kind,
                rusty_id,
                provenance_json,
                created_at
             FROM legacy_id_mappings
             WHERE (?1 IS NULL OR import_batch_id = ?1)
               AND (?2 IS NULL OR source_system = ?2)
               AND (?3 IS NULL OR legacy_kind = ?3)
               AND (?4 IS NULL OR rusty_kind = ?4)
               AND (?5 IS NULL OR rusty_id = ?5)
             ORDER BY created_at ASC, source_system ASC, legacy_kind ASC, legacy_id ASC
             LIMIT ?6 OFFSET ?7",
        )
        .map_err(|error| persistence_error("prepare query legacy id mappings", error))?;
    let rows = stmt
        .query_map(
            params![
                import_batch_id,
                source_system,
                legacy_kind,
                rusty_kind,
                rusty_id,
                limit,
                offset,
            ],
            row_to_legacy_id_mapping,
        )
        .map_err(|error| persistence_error("query legacy id mappings", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load legacy id mappings", error))
}

fn save_channel_binding(conn: &Connection, record: &ChannelBindingRecord) -> CoreResult<()> {
    let provenance_json = to_json_text(&record.provenance)?;
    conn.execute(
        "INSERT INTO channel_bindings (
            binding_id,
            adapter_id,
            provider,
            agent_id,
            instance_id,
            session_id,
            profile_id,
            external_channel_id,
            external_thread_id,
            external_user_id,
            provider_subscription_id,
            cursor,
            membership_state,
            presence_state,
            status,
            degraded_reason,
            provenance_json,
            created_at,
            updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)
        ON CONFLICT(binding_id) DO UPDATE SET
            adapter_id = excluded.adapter_id,
            provider = excluded.provider,
            agent_id = excluded.agent_id,
            instance_id = excluded.instance_id,
            session_id = excluded.session_id,
            profile_id = excluded.profile_id,
            external_channel_id = excluded.external_channel_id,
            external_thread_id = excluded.external_thread_id,
            external_user_id = excluded.external_user_id,
            provider_subscription_id = excluded.provider_subscription_id,
            cursor = excluded.cursor,
            membership_state = excluded.membership_state,
            presence_state = excluded.presence_state,
            status = excluded.status,
            degraded_reason = excluded.degraded_reason,
            provenance_json = excluded.provenance_json,
            updated_at = excluded.updated_at",
        params![
            record.binding_id,
            record.adapter_id.0,
            record.provider,
            record.agent_id.0,
            record.instance_id.as_ref().map(|value| value.0.as_str()),
            record.session_id.as_ref().map(|value| value.0.as_str()),
            record.profile_id.0,
            record.external_channel_id,
            record.external_thread_id,
            record.external_user_id,
            record.provider_subscription_id,
            record.cursor,
            record.membership_state,
            record.presence_state,
            record.status.as_str(),
            record.degraded_reason,
            provenance_json,
            record.created_at,
            record.updated_at,
        ],
    )
    .map_err(|error| persistence_error("save channel binding", error))?;
    Ok(())
}

fn query_channel_bindings(
    conn: &Connection,
    query: &ChannelBindingQuery,
) -> CoreResult<Vec<ChannelBindingRecord>> {
    let agent_id = query.agent_id.as_ref().map(|value| value.0.as_str());
    let instance_id = query.instance_id.as_ref().map(|value| value.0.as_str());
    let session_id = query.session_id.as_ref().map(|value| value.0.as_str());
    let profile_id = query.profile_id.as_ref().map(|value| value.0.as_str());
    let adapter_id = query.adapter_id.as_ref().map(|value| value.0.as_str());
    let provider = query.provider.as_deref();
    let external_channel_id = query.external_channel_id.as_deref();
    let status = query.status.map(ExternalBindingStatus::as_str);
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let mut stmt = conn
        .prepare(
            "SELECT
                binding_id,
                adapter_id,
                provider,
                agent_id,
                instance_id,
                session_id,
                profile_id,
                external_channel_id,
                external_thread_id,
                external_user_id,
                provider_subscription_id,
                cursor,
                membership_state,
                presence_state,
                status,
                degraded_reason,
                provenance_json,
                created_at,
                updated_at
             FROM channel_bindings
             WHERE (?1 IS NULL OR agent_id = ?1)
               AND (?2 IS NULL OR instance_id = ?2)
               AND (?3 IS NULL OR session_id = ?3)
               AND (?4 IS NULL OR profile_id = ?4)
               AND (?5 IS NULL OR adapter_id = ?5)
               AND (?6 IS NULL OR provider = ?6)
               AND (?7 IS NULL OR external_channel_id = ?7)
               AND (?8 IS NULL OR status = ?8)
             ORDER BY provider ASC, external_channel_id ASC, binding_id ASC
             LIMIT ?9 OFFSET ?10",
        )
        .map_err(|error| persistence_error("prepare channel binding query", error))?;
    let rows = stmt
        .query_map(
            params![
                agent_id,
                instance_id,
                session_id,
                profile_id,
                adapter_id,
                provider,
                external_channel_id,
                status,
                limit,
                offset,
            ],
            row_to_channel_binding,
        )
        .map_err(|error| persistence_error("query channel bindings", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load channel bindings", error))
}

fn row_to_channel_binding(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChannelBindingRecord> {
    let status: String = row.get(14)?;
    let provenance_json: String = row.get(16)?;
    Ok(ChannelBindingRecord {
        binding_id: row.get(0)?,
        adapter_id: AdapterId(row.get(1)?),
        provider: row.get(2)?,
        agent_id: AgentId(row.get(3)?),
        instance_id: row.get::<_, Option<String>>(4)?.map(AgentInstanceId),
        session_id: row.get::<_, Option<String>>(5)?.map(SessionId),
        profile_id: ProfileId(row.get(6)?),
        external_channel_id: row.get(7)?,
        external_thread_id: row.get(8)?,
        external_user_id: row.get(9)?,
        provider_subscription_id: row.get(10)?,
        cursor: row.get(11)?,
        membership_state: row.get(12)?,
        presence_state: row.get(13)?,
        status: external_binding_status_from_str(&status)?,
        degraded_reason: row.get(15)?,
        provenance: from_json_text(&provenance_json).map_err(to_sql_error)?,
        created_at: row.get(17)?,
        updated_at: row.get(18)?,
    })
}

fn save_mcp_binding(conn: &Connection, record: &McpBindingRecord) -> CoreResult<()> {
    let server_names_json = to_json_text(&record.server_names)?;
    let diagnostics_json = to_json_text(&record.diagnostics)?;
    conn.execute(
        "INSERT INTO mcp_bindings (
            binding_id,
            adapter_id,
            agent_id,
            instance_id,
            session_id,
            profile_id,
            server_names_json,
            endpoint_ref,
            transport,
            tool_profile_key,
            discovered_tool_revision,
            status,
            degraded_reason,
            diagnostics_json,
            created_at,
            updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
        ON CONFLICT(binding_id) DO UPDATE SET
            adapter_id = excluded.adapter_id,
            agent_id = excluded.agent_id,
            instance_id = excluded.instance_id,
            session_id = excluded.session_id,
            profile_id = excluded.profile_id,
            server_names_json = excluded.server_names_json,
            endpoint_ref = excluded.endpoint_ref,
            transport = excluded.transport,
            tool_profile_key = excluded.tool_profile_key,
            discovered_tool_revision = excluded.discovered_tool_revision,
            status = excluded.status,
            degraded_reason = excluded.degraded_reason,
            diagnostics_json = excluded.diagnostics_json,
            updated_at = excluded.updated_at",
        params![
            record.binding_id,
            record.adapter_id.0,
            record.agent_id.0,
            record.instance_id.as_ref().map(|value| value.0.as_str()),
            record.session_id.as_ref().map(|value| value.0.as_str()),
            record.profile_id.0,
            server_names_json,
            record.endpoint_ref,
            record.transport,
            record.tool_profile_key,
            record.discovered_tool_revision,
            record.status.as_str(),
            record.degraded_reason,
            diagnostics_json,
            record.created_at,
            record.updated_at,
        ],
    )
    .map_err(|error| persistence_error("save MCP binding", error))?;
    Ok(())
}

fn query_mcp_bindings(
    conn: &Connection,
    query: &McpBindingQuery,
) -> CoreResult<Vec<McpBindingRecord>> {
    let agent_id = query.agent_id.as_ref().map(|value| value.0.as_str());
    let instance_id = query.instance_id.as_ref().map(|value| value.0.as_str());
    let session_id = query.session_id.as_ref().map(|value| value.0.as_str());
    let profile_id = query.profile_id.as_ref().map(|value| value.0.as_str());
    let adapter_id = query.adapter_id.as_ref().map(|value| value.0.as_str());
    let status = query.status.map(ExternalBindingStatus::as_str);
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let mut stmt = conn
        .prepare(
            "SELECT
                binding_id,
                adapter_id,
                agent_id,
                instance_id,
                session_id,
                profile_id,
                server_names_json,
                endpoint_ref,
                transport,
                tool_profile_key,
                discovered_tool_revision,
                status,
                degraded_reason,
                diagnostics_json,
                created_at,
                updated_at
             FROM mcp_bindings
             WHERE (?1 IS NULL OR agent_id = ?1)
               AND (?2 IS NULL OR instance_id = ?2)
               AND (?3 IS NULL OR session_id = ?3)
               AND (?4 IS NULL OR profile_id = ?4)
               AND (?5 IS NULL OR adapter_id = ?5)
               AND (?6 IS NULL OR status = ?6)
             ORDER BY agent_id ASC, profile_id ASC, binding_id ASC
             LIMIT ?7 OFFSET ?8",
        )
        .map_err(|error| persistence_error("prepare MCP binding query", error))?;
    let rows = stmt
        .query_map(
            params![
                agent_id,
                instance_id,
                session_id,
                profile_id,
                adapter_id,
                status,
                limit,
                offset,
            ],
            row_to_mcp_binding,
        )
        .map_err(|error| persistence_error("query MCP bindings", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load MCP bindings", error))
}

fn row_to_mcp_binding(row: &rusqlite::Row<'_>) -> rusqlite::Result<McpBindingRecord> {
    let server_names_json: String = row.get(6)?;
    let status: String = row.get(11)?;
    let diagnostics_json: String = row.get(13)?;
    Ok(McpBindingRecord {
        binding_id: row.get(0)?,
        adapter_id: AdapterId(row.get(1)?),
        agent_id: AgentId(row.get(2)?),
        instance_id: row.get::<_, Option<String>>(3)?.map(AgentInstanceId),
        session_id: row.get::<_, Option<String>>(4)?.map(SessionId),
        profile_id: ProfileId(row.get(5)?),
        server_names: from_json_text(&server_names_json).map_err(to_sql_error)?,
        endpoint_ref: row.get(7)?,
        transport: row.get(8)?,
        tool_profile_key: row.get(9)?,
        discovered_tool_revision: row.get(10)?,
        status: external_binding_status_from_str(&status)?,
        degraded_reason: row.get(12)?,
        diagnostics: from_json_text(&diagnostics_json).map_err(to_sql_error)?,
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
    })
}

fn external_binding_status_from_str(raw: &str) -> rusqlite::Result<ExternalBindingStatus> {
    match raw {
        "active" => Ok(ExternalBindingStatus::Active),
        "degraded" => Ok(ExternalBindingStatus::Degraded),
        "disconnected" => Ok(ExternalBindingStatus::Disconnected),
        "archived" => Ok(ExternalBindingStatus::Archived),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown external binding status {other}"),
            )),
        )),
    }
}

fn row_to_import_batch(row: &rusqlite::Row<'_>) -> rusqlite::Result<RuntimeImportBatchRecord> {
    Ok(RuntimeImportBatchRecord {
        import_batch_id: row.get(0)?,
        source_system: row.get(1)?,
        source_label: row.get(2)?,
        source_snapshot_ref: row.get(3)?,
        notes: row.get(4)?,
        imported_at: row.get(5)?,
    })
}

fn row_to_legacy_id_mapping(row: &rusqlite::Row<'_>) -> rusqlite::Result<LegacyIdMappingRecord> {
    let legacy_kind: String = row.get(2)?;
    let rusty_kind: String = row.get(4)?;
    let provenance_json: String = row.get(6)?;
    Ok(LegacyIdMappingRecord {
        import_batch_id: row.get(0)?,
        source: SourceSystemReference {
            system: row.get(1)?,
            external_id: row.get(3)?,
        },
        legacy_kind: runtime_object_kind_from_str(&legacy_kind)?,
        rusty_kind: runtime_object_kind_from_str(&rusty_kind)?,
        rusty_id: row.get(5)?,
        provenance: from_json_text(&provenance_json).map_err(to_sql_error)?,
        created_at: row.get(7)?,
    })
}

fn runtime_object_kind_as_str(kind: RuntimeObjectKind) -> &'static str {
    match kind {
        RuntimeObjectKind::Agent => "agent",
        RuntimeObjectKind::AgentInstance => "agent_instance",
        RuntimeObjectKind::Session => "session",
        RuntimeObjectKind::Profile => "profile",
        RuntimeObjectKind::WorkerRun => "worker_run",
        RuntimeObjectKind::Message => "message",
        RuntimeObjectKind::CompletionPacket => "completion_packet",
        RuntimeObjectKind::ToolCall => "tool_call",
        RuntimeObjectKind::QueueMessage => "queue_message",
        RuntimeObjectKind::ExternalArtifact => "external_artifact",
    }
}

fn runtime_object_kind_from_str(raw: &str) -> rusqlite::Result<RuntimeObjectKind> {
    match raw {
        "agent" => Ok(RuntimeObjectKind::Agent),
        "agent_instance" => Ok(RuntimeObjectKind::AgentInstance),
        "session" => Ok(RuntimeObjectKind::Session),
        "profile" => Ok(RuntimeObjectKind::Profile),
        "worker_run" => Ok(RuntimeObjectKind::WorkerRun),
        "message" => Ok(RuntimeObjectKind::Message),
        "completion_packet" => Ok(RuntimeObjectKind::CompletionPacket),
        "tool_call" => Ok(RuntimeObjectKind::ToolCall),
        "queue_message" => Ok(RuntimeObjectKind::QueueMessage),
        "external_artifact" => Ok(RuntimeObjectKind::ExternalArtifact),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown runtime object kind {other}"),
            )),
        )),
    }
}

fn load_runtime_counters(
    conn: &Connection,
    scope: Option<&RuntimeCounterScope>,
) -> CoreResult<Vec<RuntimeCounterRecord>> {
    if let Some(scope) = scope {
        let (scope_type, scope_id) = runtime_counter_scope_parts(scope);
        let mut stmt = conn
            .prepare(
                "SELECT scope_type, scope_id, counter_name, value, updated_at
                 FROM runtime_counters
                 WHERE scope_type = ?1 AND scope_id = ?2
                 ORDER BY counter_name ASC",
            )
            .map_err(|error| persistence_error("prepare scoped runtime counters", error))?;
        let rows = stmt
            .query_map(params![scope_type, scope_id], row_to_runtime_counter)
            .map_err(|error| persistence_error("query scoped runtime counters", error))?;
        return rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| persistence_error("load scoped runtime counters", error));
    }

    let mut stmt = conn
        .prepare(
            "SELECT scope_type, scope_id, counter_name, value, updated_at
             FROM runtime_counters
             ORDER BY scope_type ASC, scope_id ASC, counter_name ASC",
        )
        .map_err(|error| persistence_error("prepare runtime counters", error))?;
    let rows = stmt
        .query_map([], row_to_runtime_counter)
        .map_err(|error| persistence_error("query runtime counters", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load runtime counters", error))
}

fn row_to_runtime_counter(row: &rusqlite::Row<'_>) -> rusqlite::Result<RuntimeCounterRecord> {
    let scope_type: String = row.get(0)?;
    let scope_id: String = row.get(1)?;
    Ok(RuntimeCounterRecord {
        scope: runtime_counter_scope_from_parts(&scope_type, &scope_id)?,
        counter_name: row.get(2)?,
        value: row.get::<_, i64>(3)? as u64,
        updated_at: row.get(4)?,
    })
}

fn counter_value(counters: &[RuntimeCounterRecord], name: &str) -> u64 {
    counters
        .iter()
        .find(|counter| counter.counter_name == name)
        .map_or(0, |counter| counter.value)
}

fn increment_counter_in_tx(
    tx: &rusqlite::Transaction<'_>,
    scope: &RuntimeCounterScope,
    counter_name: &str,
    amount: u64,
) -> CoreResult<()> {
    if amount == 0 {
        return Ok(());
    }

    let (scope_type, scope_id) = runtime_counter_scope_parts(scope);
    tx.execute(
        "INSERT INTO runtime_counters (
            scope_type,
            scope_id,
            counter_name,
            value
        ) VALUES (?1, ?2, ?3, ?4)
        ON CONFLICT(scope_type, scope_id, counter_name) DO UPDATE SET
            value = value + excluded.value,
            updated_at = CURRENT_TIMESTAMP",
        params![scope_type, scope_id, counter_name, amount as i64],
    )
    .map_err(|error| persistence_error("increment runtime counter", error))?;
    Ok(())
}

fn increment_counter_for_scopes_in_tx(
    tx: &rusqlite::Transaction<'_>,
    scopes: Vec<RuntimeCounterScope>,
    counter_name: &str,
    amount: u64,
) -> CoreResult<()> {
    for scope in dedupe_counter_scopes(scopes) {
        increment_counter_in_tx(tx, &scope, counter_name, amount)?;
    }
    Ok(())
}

fn increment_event_counters_in_tx(
    tx: &rusqlite::Transaction<'_>,
    event: &CoreEvent,
) -> CoreResult<()> {
    for (counter_name, amount) in event_counter_deltas(event) {
        increment_counter_for_scopes_in_tx(tx, event_counter_scopes(event), counter_name, amount)?;
    }
    Ok(())
}

fn event_counter_deltas(event: &CoreEvent) -> Vec<(&'static str, u64)> {
    match event {
        CoreEvent::AgentMessageRouted { .. } => vec![(COUNTER_MESSAGES, 1)],
        CoreEvent::BrainWakeRequested { .. } => vec![(COUNTER_WAKES, 1)],
        CoreEvent::BrainActionsAccepted { count, .. } => {
            vec![
                (COUNTER_BRAIN_TURNS, 1),
                ("accepted_actions", *count as u64),
            ]
        }
        CoreEvent::BrainEventObserved { event, .. } => match event {
            BrainEvent::ToolCallStarted { .. } => vec![(COUNTER_TOOL_CALLS, 1)],
            BrainEvent::ToolCallFinished { is_error: true, .. } => vec![(COUNTER_TOOL_ERRORS, 1)],
            _ => Vec::new(),
        },
        CoreEvent::DelegationLifecycleObserved { lifecycle } => match lifecycle.phase {
            rusty_crew_core_protocol::DelegationLifecyclePhase::Created => {
                vec![(COUNTER_DELEGATIONS_CREATED, 1)]
            }
            rusty_crew_core_protocol::DelegationLifecyclePhase::Completed => {
                vec![(COUNTER_DELEGATIONS_COMPLETED, 1)]
            }
            rusty_crew_core_protocol::DelegationLifecyclePhase::Failed
            | rusty_crew_core_protocol::DelegationLifecyclePhase::Blocked
            | rusty_crew_core_protocol::DelegationLifecyclePhase::Exhausted => {
                vec![(COUNTER_DELEGATIONS_FAILED, 1)]
            }
            rusty_crew_core_protocol::DelegationLifecyclePhase::TimedOut => {
                vec![(COUNTER_DELEGATIONS_TIMED_OUT, 1)]
            }
            rusty_crew_core_protocol::DelegationLifecyclePhase::Cancelled => {
                vec![(COUNTER_DELEGATIONS_CANCELLED, 1)]
            }
            rusty_crew_core_protocol::DelegationLifecyclePhase::WakeRequested
            | rusty_crew_core_protocol::DelegationLifecyclePhase::CheckpointRequested => Vec::new(),
        },
        CoreEvent::CompletionPacketDelivered { .. } => vec![(COUNTER_COMPLETIONS, 1)],
        CoreEvent::SessionCreated { .. }
        | CoreEvent::SessionArchived { .. }
        | CoreEvent::ExternalEventInjected { .. }
        | CoreEvent::DenDataUpdated { .. } => Vec::new(),
    }
}

fn event_counter_scopes(event: &CoreEvent) -> Vec<RuntimeCounterScope> {
    let mut scopes = vec![RuntimeCounterScope::Runtime];
    scopes.extend(
        event_agent_ids(event)
            .into_iter()
            .map(RuntimeCounterScope::Agent),
    );
    let session_ids = event_session_ids(event);
    scopes.extend(
        session_ids
            .iter()
            .cloned()
            .map(RuntimeCounterScope::Session),
    );
    scopes.extend(session_ids.into_iter().map(|session_id| {
        RuntimeCounterScope::Instance(AgentInstanceId::new(format!("instance:{session_id}")))
    }));
    scopes
}

fn runtime_counter_scope_parts(scope: &RuntimeCounterScope) -> (&'static str, String) {
    match scope {
        RuntimeCounterScope::Runtime => ("runtime", "_global".to_string()),
        RuntimeCounterScope::Agent(agent_id) => ("agent", agent_id.0.clone()),
        RuntimeCounterScope::Instance(instance_id) => ("instance", instance_id.0.clone()),
        RuntimeCounterScope::Session(session_id) => ("session", session_id.0.clone()),
    }
}

fn runtime_counter_scope_from_parts(
    scope_type: &str,
    scope_id: &str,
) -> rusqlite::Result<RuntimeCounterScope> {
    match scope_type {
        "runtime" if scope_id == "_global" => Ok(RuntimeCounterScope::Runtime),
        "agent" => Ok(RuntimeCounterScope::Agent(AgentId::new(scope_id))),
        "instance" => Ok(RuntimeCounterScope::Instance(AgentInstanceId::new(
            scope_id,
        ))),
        "session" => Ok(RuntimeCounterScope::Session(SessionId::new(scope_id))),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown runtime counter scope {other}:{scope_id}"),
            )),
        )),
    }
}

fn dedupe_counter_scopes(scopes: Vec<RuntimeCounterScope>) -> Vec<RuntimeCounterScope> {
    let mut deduped = Vec::new();
    for scope in scopes {
        if deduped.contains(&scope) {
            continue;
        }
        deduped.push(scope);
    }
    deduped
}

fn save_event_indexes_in_tx(
    tx: &rusqlite::Transaction<'_>,
    sequence: u64,
    event: &CoreEvent,
) -> CoreResult<()> {
    let session_ids = event_session_ids(event);
    replace_event_index_values(
        tx,
        EventIndexProjection::Session,
        sequence,
        session_ids.iter().map(|id| id.0.clone()).collect(),
    )?;
    replace_event_index_values(
        tx,
        EventIndexProjection::Agent,
        sequence,
        event_agent_ids(event).into_iter().map(|id| id.0).collect(),
    )?;
    replace_event_index_values(
        tx,
        EventIndexProjection::Instance,
        sequence,
        session_ids
            .into_iter()
            .map(|id| AgentInstanceId::new(format!("instance:{id}")).0)
            .collect(),
    )?;
    replace_event_index_values(
        tx,
        EventIndexProjection::Correlation,
        sequence,
        event_correlation_ids(event),
    )?;
    replace_event_index_values(
        tx,
        EventIndexProjection::Wake,
        sequence,
        event_source_wake_ids(event),
    )
}

#[derive(Debug, Clone, Copy)]
enum EventIndexProjection {
    Session,
    Agent,
    Instance,
    Correlation,
    Wake,
}

impl EventIndexProjection {
    fn delete_sql(self) -> &'static str {
        match self {
            Self::Session => "DELETE FROM event_session_index WHERE sequence = ?1",
            Self::Agent => "DELETE FROM event_agent_index WHERE sequence = ?1",
            Self::Instance => "DELETE FROM event_instance_index WHERE sequence = ?1",
            Self::Correlation => "DELETE FROM event_correlation_index WHERE sequence = ?1",
            Self::Wake => "DELETE FROM event_wake_index WHERE sequence = ?1",
        }
    }

    fn insert_sql(self) -> &'static str {
        match self {
            Self::Session => {
                "INSERT OR IGNORE INTO event_session_index (sequence, session_id) VALUES (?1, ?2)"
            }
            Self::Agent => {
                "INSERT OR IGNORE INTO event_agent_index (sequence, agent_id) VALUES (?1, ?2)"
            }
            Self::Instance => {
                "INSERT OR IGNORE INTO event_instance_index (sequence, instance_id) VALUES (?1, ?2)"
            }
            Self::Correlation => {
                "INSERT OR IGNORE INTO event_correlation_index (sequence, correlation_id) VALUES (?1, ?2)"
            }
            Self::Wake => {
                "INSERT OR IGNORE INTO event_wake_index (sequence, source_wake_id) VALUES (?1, ?2)"
            }
        }
    }

    fn select_sql(self) -> &'static str {
        match self {
            Self::Session => {
                "SELECT session_id FROM event_session_index WHERE sequence = ?1 ORDER BY session_id ASC"
            }
            Self::Agent => {
                "SELECT agent_id FROM event_agent_index WHERE sequence = ?1 ORDER BY agent_id ASC"
            }
            Self::Instance => {
                "SELECT instance_id FROM event_instance_index WHERE sequence = ?1 ORDER BY instance_id ASC"
            }
            Self::Correlation => {
                "SELECT correlation_id FROM event_correlation_index WHERE sequence = ?1 ORDER BY correlation_id ASC"
            }
            Self::Wake => {
                "SELECT source_wake_id FROM event_wake_index WHERE sequence = ?1 ORDER BY source_wake_id ASC"
            }
        }
    }
}

fn replace_event_index_values(
    tx: &rusqlite::Transaction<'_>,
    projection: EventIndexProjection,
    sequence: u64,
    values: Vec<String>,
) -> CoreResult<()> {
    tx.execute(projection.delete_sql(), params![sequence as i64])
        .map_err(|error| persistence_error("delete event index values", error))?;
    for value in dedupe_non_empty(values) {
        tx.execute(projection.insert_sql(), params![sequence as i64, value])
            .map_err(|error| persistence_error("insert event index value", error))?;
    }
    Ok(())
}

fn load_event_index_values(
    conn: &Connection,
    projection: EventIndexProjection,
    sequence: u64,
) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(projection.select_sql())?;
    let rows = stmt.query_map(params![sequence as i64], |row| row.get::<_, String>(0))?;
    rows.collect::<Result<Vec<_>, _>>()
}

fn dedupe_non_empty(values: Vec<String>) -> Vec<String> {
    let mut deduped = Vec::new();
    for value in values {
        if value.trim().is_empty() || deduped.contains(&value) {
            continue;
        }
        deduped.push(value);
    }
    deduped
}

fn event_session_ids(event: &CoreEvent) -> Vec<SessionId> {
    match event {
        CoreEvent::SessionCreated { state } => vec![state.session_id.clone()],
        CoreEvent::SessionArchived { session_id } => vec![session_id.clone()],
        CoreEvent::DelegationLifecycleObserved { lifecycle } => vec![
            lifecycle.parent_session_id.clone(),
            lifecycle.delegated_session_id.clone(),
        ],
        CoreEvent::BrainWakeRequested { session_id }
        | CoreEvent::BrainEventObserved { session_id, .. }
        | CoreEvent::BrainActionsAccepted { session_id, .. } => vec![session_id.clone()],
        CoreEvent::CompletionPacketDelivered { packet } => vec![packet.session_id.clone()],
        CoreEvent::AgentMessageRouted { .. }
        | CoreEvent::ExternalEventInjected { .. }
        | CoreEvent::DenDataUpdated { .. } => Vec::new(),
    }
}

fn event_agent_ids(event: &CoreEvent) -> Vec<AgentId> {
    match event {
        CoreEvent::SessionCreated { state } => vec![state.agent_id.clone()],
        CoreEvent::AgentMessageRouted { message } => vec![message.from.clone(), message.to.clone()],
        CoreEvent::SessionArchived { .. }
        | CoreEvent::DelegationLifecycleObserved { .. }
        | CoreEvent::ExternalEventInjected { .. }
        | CoreEvent::DenDataUpdated { .. }
        | CoreEvent::BrainWakeRequested { .. }
        | CoreEvent::BrainEventObserved { .. }
        | CoreEvent::BrainActionsAccepted { .. }
        | CoreEvent::CompletionPacketDelivered { .. } => Vec::new(),
    }
}

fn event_correlation_ids(event: &CoreEvent) -> Vec<String> {
    match event {
        CoreEvent::SessionCreated { state } => state
            .delegation
            .as_ref()
            .map(|lineage| lineage.correlation_id.clone())
            .into_iter()
            .collect(),
        CoreEvent::AgentMessageRouted { message } => {
            message.correlation_id.clone().into_iter().collect()
        }
        CoreEvent::SessionArchived { .. }
        | CoreEvent::DelegationLifecycleObserved { .. }
        | CoreEvent::ExternalEventInjected { .. }
        | CoreEvent::DenDataUpdated { .. }
        | CoreEvent::BrainWakeRequested { .. }
        | CoreEvent::BrainEventObserved { .. }
        | CoreEvent::BrainActionsAccepted { .. }
        | CoreEvent::CompletionPacketDelivered { .. } => Vec::new(),
    }
}

fn event_source_wake_ids(event: &CoreEvent) -> Vec<String> {
    match event {
        CoreEvent::SessionCreated { state } => state
            .delegation
            .as_ref()
            .map(|lineage| lineage.source_wake_id.clone())
            .into_iter()
            .collect(),
        CoreEvent::BrainEventObserved {
            wake_id: Some(wake_id),
            ..
        } => vec![wake_id.clone()],
        CoreEvent::SessionArchived { .. }
        | CoreEvent::AgentMessageRouted { .. }
        | CoreEvent::DelegationLifecycleObserved { .. }
        | CoreEvent::ExternalEventInjected { .. }
        | CoreEvent::DenDataUpdated { .. }
        | CoreEvent::BrainWakeRequested { .. }
        | CoreEvent::BrainEventObserved { wake_id: None, .. }
        | CoreEvent::BrainActionsAccepted { .. }
        | CoreEvent::CompletionPacketDelivered { .. } => Vec::new(),
    }
}

fn save_event_search_rows_in_tx(
    tx: &rusqlite::Transaction<'_>,
    sequence: u64,
    event: &CoreEvent,
    recorded_at: &IsoTimestamp,
) -> CoreResult<()> {
    tx.execute(
        "DELETE FROM runtime_search_fts WHERE row_type = ?1 AND sequence = ?2",
        params!["message", sequence as i64],
    )
    .map_err(|error| persistence_error("delete event search rows", error))?;

    if let CoreEvent::AgentMessageRouted { message } = event {
        for agent_id in dedupe_non_empty(vec![message.from.0.clone(), message.to.0.clone()]) {
            insert_runtime_search_row(
                tx,
                &RuntimeSearchInsert {
                    row_type: RuntimeSearchRowType::Message,
                    row_key: format!("message:{sequence}:{agent_id}"),
                    sequence: Some(sequence),
                    session_id: None,
                    agent_id: Some(agent_id),
                    instance_id: None,
                    task_id: None,
                    event_kind: Some(CoreEventKind::AgentMessageRouted),
                    recorded_at: recorded_at.clone(),
                    title: "agent message".to_string(),
                    body: message.body.clone(),
                },
            )?;
        }
    }
    Ok(())
}

fn save_session_config_search_row_in_tx(
    tx: &rusqlite::Transaction<'_>,
    config: &SessionConfig,
    created_at: &IsoTimestamp,
) -> CoreResult<()> {
    let task_id = config
        .delegation
        .as_ref()
        .and_then(|lineage| lineage.requested_task_id.clone());
    let tool_names = config
        .tool_profile
        .tools
        .iter()
        .map(|tool| tool.name.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let body = format!(
        "agent {} profile {} kind {} workdir {} tools {}",
        config.agent_id,
        config.profile_id,
        session_kind_as_str(&config.kind),
        config.resource_limits.workdir.as_deref().unwrap_or(""),
        tool_names
    );
    tx.execute(
        "DELETE FROM runtime_search_fts WHERE row_type = ?1 AND row_key = ?2",
        params!["session", config.session_id.0.as_str()],
    )
    .map_err(|error| persistence_error("delete session config search row", error))?;
    insert_runtime_search_row(
        tx,
        &RuntimeSearchInsert {
            row_type: RuntimeSearchRowType::Session,
            row_key: config.session_id.0.clone(),
            sequence: None,
            session_id: Some(config.session_id.0.clone()),
            agent_id: Some(config.agent_id.0.clone()),
            instance_id: Some(AgentInstanceId::new(format!("instance:{}", config.session_id)).0),
            task_id: task_id.map(|value| value.0),
            event_kind: Some(CoreEventKind::SessionCreated),
            recorded_at: created_at.clone(),
            title: format!("session {}", config.session_id),
            body,
        },
    )
}

struct RuntimeSearchInsert {
    row_type: RuntimeSearchRowType,
    row_key: String,
    sequence: Option<u64>,
    session_id: Option<String>,
    agent_id: Option<String>,
    instance_id: Option<String>,
    task_id: Option<String>,
    event_kind: Option<CoreEventKind>,
    recorded_at: IsoTimestamp,
    title: String,
    body: String,
}

fn insert_runtime_search_row(
    tx: &rusqlite::Transaction<'_>,
    row: &RuntimeSearchInsert,
) -> CoreResult<()> {
    let event_kind = row.event_kind.as_ref().map(|kind| format!("{kind:?}"));
    tx.execute(
        "INSERT INTO runtime_search_fts (
            row_type,
            row_key,
            sequence,
            session_id,
            agent_id,
            instance_id,
            task_id,
            event_kind,
            recorded_at,
            title,
            body
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            row.row_type.as_str(),
            row.row_key,
            row.sequence.map(|value| value as i64),
            row.session_id,
            row.agent_id,
            row.instance_id,
            row.task_id,
            event_kind,
            row.recorded_at,
            row.title,
            row.body,
        ],
    )
    .map_err(|error| persistence_error("insert runtime search row", error))?;
    Ok(())
}

fn row_to_runtime_search_result(row: &rusqlite::Row<'_>) -> rusqlite::Result<RuntimeSearchResult> {
    let row_type: String = row.get(0)?;
    let sequence = row.get::<_, Option<i64>>(2)?.map(|value| value as u64);
    let event_kind = row
        .get::<_, Option<String>>(7)?
        .as_deref()
        .map(core_event_kind_from_debug_str)
        .transpose()?;
    Ok(RuntimeSearchResult {
        row_type: runtime_search_row_type_from_str(&row_type)?,
        row_key: row.get(1)?,
        sequence,
        session_id: row.get::<_, Option<String>>(3)?.map(SessionId),
        agent_id: row.get::<_, Option<String>>(4)?.map(AgentId),
        instance_id: row.get::<_, Option<String>>(5)?.map(AgentInstanceId),
        task_id: row.get::<_, Option<String>>(6)?.map(TaskId),
        event_kind,
        recorded_at: row.get(8)?,
        title: row.get(9)?,
        body: row.get(10)?,
    })
}

fn runtime_search_row_type_from_str(raw: &str) -> rusqlite::Result<RuntimeSearchRowType> {
    match raw {
        "message" => Ok(RuntimeSearchRowType::Message),
        "queue_message" => Ok(RuntimeSearchRowType::QueueMessage),
        "session" => Ok(RuntimeSearchRowType::Session),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown runtime search row type {other}"),
            )),
        )),
    }
}

fn quote_fts_query(query: &str) -> String {
    format!("\"{}\"", query.replace('"', "\"\""))
}

fn core_event_kind_from_debug_str(raw: &str) -> rusqlite::Result<CoreEventKind> {
    match raw {
        "SessionCreated" => Ok(CoreEventKind::SessionCreated),
        "SessionArchived" => Ok(CoreEventKind::SessionArchived),
        "AgentMessageRouted" => Ok(CoreEventKind::AgentMessageRouted),
        "DelegationLifecycleObserved" => Ok(CoreEventKind::DelegationLifecycleObserved),
        "ExternalEventInjected" => Ok(CoreEventKind::ExternalEventInjected),
        "DenDataUpdated" => Ok(CoreEventKind::DenDataUpdated),
        "BrainWakeRequested" => Ok(CoreEventKind::BrainWakeRequested),
        "BrainEventObserved" => Ok(CoreEventKind::BrainEventObserved),
        "BrainActionsAccepted" => Ok(CoreEventKind::BrainActionsAccepted),
        "CompletionPacketDelivered" => Ok(CoreEventKind::CompletionPacketDelivered),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            7,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown core event kind {other}"),
            )),
        )),
    }
}

fn save_session_state_in_tx(
    tx: &rusqlite::Transaction<'_>,
    state: &SessionState,
) -> CoreResult<()> {
    let kind_json = to_json_text(&state.kind)?;
    let status_json = to_json_text(&state.status)?;
    let resource_limits_json = to_json_text(&state.resource_limits)?;
    let tool_profile_json = to_json_text(&state.tool_profile)?;
    let delegation_json = state.delegation.as_ref().map(to_json_text).transpose()?;
    tx.execute(
        "INSERT INTO sessions (
            session_id,
            handle,
            agent_id,
            profile_id,
            kind_json,
            delegation_json,
            resource_limits_json,
            tool_profile_json,
            status_json,
            brain_turn_count,
            created_at,
            last_active_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
        ON CONFLICT(session_id) DO UPDATE SET
            handle = excluded.handle,
            agent_id = excluded.agent_id,
            profile_id = excluded.profile_id,
            kind_json = excluded.kind_json,
            delegation_json = excluded.delegation_json,
            resource_limits_json = excluded.resource_limits_json,
            tool_profile_json = excluded.tool_profile_json,
            status_json = excluded.status_json,
            brain_turn_count = excluded.brain_turn_count,
            last_active_at = excluded.last_active_at",
        params![
            state.session_id.0,
            state.handle.get() as i64,
            state.agent_id.0,
            state.profile_id.0,
            kind_json,
            delegation_json,
            resource_limits_json,
            tool_profile_json,
            status_json,
            state.brain_turn_count as i64,
            state.created_at,
            state.last_active_at,
        ],
    )
    .map_err(|error| persistence_error("save session", error))?;
    Ok(())
}

fn save_session_config_in_tx(
    tx: &rusqlite::Transaction<'_>,
    config: &SessionConfig,
    created_at: &IsoTimestamp,
) -> CoreResult<()> {
    let resource_limits_json = to_json_text(&config.resource_limits)?;
    let tool_profile_json = to_json_text(&config.tool_profile)?;
    let config_json = to_json_text(config)?;
    tx.execute(
        "INSERT INTO session_configs (
            session_id,
            profile_id,
            kind,
            resource_limits_json,
            tool_profile_json,
            config_json,
            created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        ON CONFLICT(session_id) DO NOTHING",
        params![
            config.session_id.0,
            config.profile_id.0,
            session_kind_as_str(&config.kind),
            resource_limits_json,
            tool_profile_json,
            config_json,
            created_at,
        ],
    )
    .map_err(|error| persistence_error("save session config", error))?;
    save_session_config_search_row_in_tx(tx, config, created_at)?;
    Ok(())
}

fn load_session_config_records(conn: &Connection) -> CoreResult<Vec<SessionConfigRecord>> {
    let mut stmt = conn
        .prepare(
            "SELECT
                session_id,
                profile_id,
                kind,
                resource_limits_json,
                tool_profile_json,
                config_json,
                created_at
             FROM session_configs
             ORDER BY session_id ASC",
        )
        .map_err(|error| persistence_error("prepare load session configs", error))?;
    let rows = stmt
        .query_map([], row_to_session_config_record)
        .map_err(|error| persistence_error("query session configs", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load session configs", error))
}

fn query_profile_memory(
    conn: &Connection,
    query: &ProfileMemoryQuery,
) -> CoreResult<Vec<ProfileMemoryRecord>> {
    let target_parts = query
        .target
        .as_ref()
        .map(|target| profile_memory_target_parts(&query.profile_id, target));
    let target_type = target_parts.as_ref().map(|(target_type, _)| *target_type);
    let target_id = target_parts
        .as_ref()
        .map(|(_, target_id)| target_id.as_str());
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let mut stmt = conn
        .prepare(
            "SELECT
                profile_id,
                target_type,
                target_id,
                memory_key,
                content,
                metadata_json,
                revision,
                created_at,
                updated_at
             FROM profile_memories
             WHERE profile_id = ?1
               AND (?2 IS NULL OR target_type = ?2)
               AND (?3 IS NULL OR target_id = ?3)
             ORDER BY updated_at DESC, memory_key ASC
             LIMIT ?4 OFFSET ?5",
        )
        .map_err(|error| persistence_error("prepare query profile memory", error))?;
    let rows = stmt
        .query_map(
            params![
                query.profile_id.0.as_str(),
                target_type,
                target_id,
                limit,
                offset
            ],
            row_to_profile_memory,
        )
        .map_err(|error| persistence_error("query profile memory", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load profile memory", error))
}

fn get_profile_memory(
    conn: &Connection,
    profile_id: &ProfileId,
    target: &ProfileMemoryTarget,
    key: &str,
) -> CoreResult<Option<ProfileMemoryRecord>> {
    let (target_type, target_id) = profile_memory_target_parts(profile_id, target);
    conn.query_row(
        "SELECT
            profile_id,
            target_type,
            target_id,
            memory_key,
            content,
            metadata_json,
            revision,
            created_at,
            updated_at
         FROM profile_memories
         WHERE profile_id = ?1
           AND target_type = ?2
           AND target_id = ?3
           AND memory_key = ?4",
        params![profile_id.0.as_str(), target_type, target_id.as_str(), key,],
        row_to_profile_memory,
    )
    .optional()
    .map_err(|error| persistence_error("get profile memory", error))
}

fn insert_profile_memory_in_tx(
    tx: &rusqlite::Transaction<'_>,
    write: &ProfileMemoryWrite,
) -> CoreResult<ProfileMemoryRecord> {
    let (target_type, target_id) = profile_memory_target_parts(&write.profile_id, &write.target);
    let metadata_json = to_json_text(&write.metadata)?;
    tx.execute(
        "INSERT INTO profile_memories (
            profile_id,
            target_type,
            target_id,
            memory_key,
            content,
            metadata_json,
            revision,
            created_at,
            updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?7)",
        params![
            write.profile_id.0.as_str(),
            target_type,
            target_id.as_str(),
            write.key.as_str(),
            write.content.as_str(),
            metadata_json,
            write.now.as_str(),
        ],
    )
    .map_err(|error| persistence_error("insert profile memory", error))?;
    Ok(ProfileMemoryRecord {
        profile_id: write.profile_id.clone(),
        target: write.target.clone(),
        key: write.key.clone(),
        content: write.content.clone(),
        metadata: write.metadata.clone(),
        revision: 1,
        created_at: write.now.clone(),
        updated_at: write.now.clone(),
    })
}

fn update_profile_memory_in_tx(
    tx: &rusqlite::Transaction<'_>,
    write: &ProfileMemoryWrite,
    revision: u64,
) -> CoreResult<ProfileMemoryRecord> {
    let (target_type, target_id) = profile_memory_target_parts(&write.profile_id, &write.target);
    let metadata_json = to_json_text(&write.metadata)?;
    tx.execute(
        "UPDATE profile_memories
         SET content = ?5,
             metadata_json = ?6,
             revision = ?7,
             updated_at = ?8
         WHERE profile_id = ?1
           AND target_type = ?2
           AND target_id = ?3
           AND memory_key = ?4",
        params![
            write.profile_id.0.as_str(),
            target_type,
            target_id.as_str(),
            write.key.as_str(),
            write.content.as_str(),
            metadata_json,
            revision as i64,
            write.now.as_str(),
        ],
    )
    .map_err(|error| persistence_error("update profile memory", error))?;
    Ok(ProfileMemoryRecord {
        profile_id: write.profile_id.clone(),
        target: write.target.clone(),
        key: write.key.clone(),
        content: write.content.clone(),
        metadata: write.metadata.clone(),
        revision,
        created_at: get_profile_memory(tx, &write.profile_id, &write.target, &write.key)?
            .map(|record| record.created_at)
            .unwrap_or_else(|| write.now.clone()),
        updated_at: write.now.clone(),
    })
}

fn count_profile_memory_for_profile(conn: &Connection, profile_id: &ProfileId) -> CoreResult<u64> {
    let count = conn
        .query_row(
            "SELECT COUNT(*) FROM profile_memories WHERE profile_id = ?1",
            params![profile_id.0.as_str()],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| persistence_error("count profile memory", error))?;
    Ok(count as u64)
}

fn row_to_profile_memory(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProfileMemoryRecord> {
    let profile_id = ProfileId(row.get(0)?);
    let target_type: String = row.get(1)?;
    let target_id: String = row.get(2)?;
    let metadata_json: String = row.get(5)?;
    Ok(ProfileMemoryRecord {
        profile_id: profile_id.clone(),
        target: profile_memory_target_from_parts(&profile_id, &target_type, target_id)?,
        key: row.get(3)?,
        content: row.get(4)?,
        metadata: from_json_text(&metadata_json).map_err(to_sql_error)?,
        revision: row.get::<_, i64>(6)? as u64,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

fn profile_memory_target_parts(
    profile_id: &ProfileId,
    target: &ProfileMemoryTarget,
) -> (&'static str, String) {
    match target {
        ProfileMemoryTarget::Profile => ("profile", profile_id.0.clone()),
        ProfileMemoryTarget::User(user_id) => ("user", user_id.clone()),
    }
}

fn profile_memory_target_from_parts(
    profile_id: &ProfileId,
    target_type: &str,
    target_id: String,
) -> rusqlite::Result<ProfileMemoryTarget> {
    match target_type {
        "profile" if target_id == profile_id.0 => Ok(ProfileMemoryTarget::Profile),
        "user" if !target_id.is_empty() => Ok(ProfileMemoryTarget::User(target_id)),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("invalid profile memory target {other}/{target_id}"),
            )),
        )),
    }
}

fn validate_profile_memory_write(
    write: &ProfileMemoryWrite,
    caps: &ProfileMemoryCaps,
) -> CoreResult<()> {
    validate_profile_memory_key(&write.key, caps.max_key_bytes)?;
    if write.content.len() > caps.max_content_bytes as usize {
        return Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            format!(
                "profile memory content exceeds {} bytes",
                caps.max_content_bytes
            ),
        ));
    }
    if let ProfileMemoryTarget::User(user_id) = &write.target {
        if user_id.trim().is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "profile memory user target must be non-empty",
            ));
        }
    }
    Ok(())
}

fn validate_profile_memory_key(key: &str, max_key_bytes: u32) -> CoreResult<()> {
    if key.trim().is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "profile memory key must be non-empty",
        ));
    }
    if key.len() > max_key_bytes as usize {
        return Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            format!("profile memory key exceeds {max_key_bytes} bytes"),
        ));
    }
    Ok(())
}

fn row_to_session_config_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionConfigRecord> {
    let resource_limits_json: String = row.get(3)?;
    let tool_profile_json: String = row.get(4)?;
    let config_json: String = row.get(5)?;
    Ok(SessionConfigRecord {
        session_id: SessionId(row.get(0)?),
        profile_id: ProfileId(row.get(1)?),
        kind: session_kind_from_str(&row.get::<_, String>(2)?)?,
        resource_limits: from_json_text(&resource_limits_json).map_err(to_sql_error)?,
        tool_profile: from_json_text(&tool_profile_json).map_err(to_sql_error)?,
        config: from_json_text(&config_json).map_err(to_sql_error)?,
        created_at: row.get(6)?,
    })
}

fn save_default_identity_for_session_in_tx(
    tx: &rusqlite::Transaction<'_>,
    state: &SessionState,
) -> CoreResult<()> {
    let den = DenRuntimeReference {
        project_id: None,
        task_id: state
            .delegation
            .as_ref()
            .and_then(|lineage| lineage.requested_task_id.clone()),
    };
    let archived_at = if state.status == SessionStatus::Archived {
        Some(state.last_active_at.clone())
    } else {
        None
    };
    let status = durable_status_from_session_status(&state.status);
    let instance_id = AgentInstanceId::new(format!("instance:{}", state.session_id));

    save_agent_identity(
        tx,
        &DurableAgentRecord {
            agent_id: state.agent_id.clone(),
            display_label: state.agent_id.to_string(),
            profile_id: state.profile_id.clone(),
            kind: durable_agent_kind_from_session_kind(&state.kind),
            status: status.clone(),
            source: None,
            den: den.clone(),
            created_at: state.created_at.clone(),
            archived_at: archived_at.clone(),
        },
    )?;
    save_agent_instance(
        tx,
        &AgentInstanceRecord {
            instance_id: instance_id.clone(),
            agent_id: state.agent_id.clone(),
            display_label: state.session_id.to_string(),
            profile_id: state.profile_id.clone(),
            status: status.clone(),
            source: None,
            den: den.clone(),
            created_at: state.created_at.clone(),
            last_active_at: state.last_active_at.clone(),
            archived_at: archived_at.clone(),
        },
    )?;
    save_session_identity(
        tx,
        &SessionIdentityRecord {
            session_id: state.session_id.clone(),
            instance_id,
            agent_id: state.agent_id.clone(),
            profile_id: state.profile_id.clone(),
            kind: state.kind.clone(),
            status: state.status.clone(),
            source: None,
            den,
            created_at: state.created_at.clone(),
            last_active_at: state.last_active_at.clone(),
            archived_at,
        },
    )
}

fn save_agent_identity(conn: &Connection, record: &DurableAgentRecord) -> CoreResult<()> {
    let source_system = record.source.as_ref().map(|source| source.system.as_str());
    let source_external_id = record
        .source
        .as_ref()
        .map(|source| source.external_id.as_str());
    let den_project_id = record
        .den
        .project_id
        .as_ref()
        .map(|project_id| project_id.0.as_str());
    let den_task_id = record
        .den
        .task_id
        .as_ref()
        .map(|task_id| task_id.0.as_str());
    conn.execute(
        "INSERT INTO agents (
            agent_id,
            display_label,
            profile_id,
            kind,
            status,
            source_system,
            source_external_id,
            den_project_id,
            den_task_id,
            created_at,
            archived_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
        ON CONFLICT(agent_id) DO UPDATE SET
            display_label = excluded.display_label,
            profile_id = excluded.profile_id,
            kind = excluded.kind,
            status = excluded.status,
            source_system = excluded.source_system,
            source_external_id = excluded.source_external_id,
            den_project_id = excluded.den_project_id,
            den_task_id = excluded.den_task_id,
            archived_at = excluded.archived_at",
        params![
            record.agent_id.0,
            record.display_label,
            record.profile_id.0,
            durable_agent_kind_as_str(&record.kind),
            durable_identity_status_as_str(&record.status),
            source_system,
            source_external_id,
            den_project_id,
            den_task_id,
            record.created_at,
            record.archived_at,
        ],
    )
    .map_err(|error| persistence_error("save agent identity", error))?;
    Ok(())
}

fn load_agent_identities(conn: &Connection) -> CoreResult<Vec<DurableAgentRecord>> {
    let mut stmt = conn
        .prepare(
            "SELECT
                agent_id,
                display_label,
                profile_id,
                kind,
                status,
                source_system,
                source_external_id,
                den_project_id,
                den_task_id,
                created_at,
                archived_at
             FROM agents
             ORDER BY agent_id ASC",
        )
        .map_err(|error| persistence_error("prepare load agent identities", error))?;
    let rows = stmt
        .query_map([], row_to_agent_identity)
        .map_err(|error| persistence_error("query agent identities", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load agent identities", error))
}

fn save_agent_instance(conn: &Connection, record: &AgentInstanceRecord) -> CoreResult<()> {
    let source_system = record.source.as_ref().map(|source| source.system.as_str());
    let source_external_id = record
        .source
        .as_ref()
        .map(|source| source.external_id.as_str());
    let den_project_id = record
        .den
        .project_id
        .as_ref()
        .map(|project_id| project_id.0.as_str());
    let den_task_id = record
        .den
        .task_id
        .as_ref()
        .map(|task_id| task_id.0.as_str());
    conn.execute(
        "INSERT INTO agent_instances (
            instance_id,
            agent_id,
            display_label,
            profile_id,
            status,
            source_system,
            source_external_id,
            den_project_id,
            den_task_id,
            created_at,
            last_active_at,
            archived_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
        ON CONFLICT(instance_id) DO UPDATE SET
            agent_id = excluded.agent_id,
            display_label = excluded.display_label,
            profile_id = excluded.profile_id,
            status = excluded.status,
            source_system = excluded.source_system,
            source_external_id = excluded.source_external_id,
            den_project_id = excluded.den_project_id,
            den_task_id = excluded.den_task_id,
            last_active_at = excluded.last_active_at,
            archived_at = excluded.archived_at",
        params![
            record.instance_id.0,
            record.agent_id.0,
            record.display_label,
            record.profile_id.0,
            durable_identity_status_as_str(&record.status),
            source_system,
            source_external_id,
            den_project_id,
            den_task_id,
            record.created_at,
            record.last_active_at,
            record.archived_at,
        ],
    )
    .map_err(|error| persistence_error("save agent instance", error))?;
    Ok(())
}

fn load_agent_instances(conn: &Connection) -> CoreResult<Vec<AgentInstanceRecord>> {
    let mut stmt = conn
        .prepare(
            "SELECT
                instance_id,
                agent_id,
                display_label,
                profile_id,
                status,
                source_system,
                source_external_id,
                den_project_id,
                den_task_id,
                created_at,
                last_active_at,
                archived_at
             FROM agent_instances
             ORDER BY instance_id ASC",
        )
        .map_err(|error| persistence_error("prepare load agent instances", error))?;
    let rows = stmt
        .query_map([], row_to_agent_instance)
        .map_err(|error| persistence_error("query agent instances", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load agent instances", error))
}

fn save_session_identity(conn: &Connection, record: &SessionIdentityRecord) -> CoreResult<()> {
    let source_system = record.source.as_ref().map(|source| source.system.as_str());
    let source_external_id = record
        .source
        .as_ref()
        .map(|source| source.external_id.as_str());
    let den_project_id = record
        .den
        .project_id
        .as_ref()
        .map(|project_id| project_id.0.as_str());
    let den_task_id = record
        .den
        .task_id
        .as_ref()
        .map(|task_id| task_id.0.as_str());
    conn.execute(
        "INSERT INTO session_identity (
            session_id,
            instance_id,
            agent_id,
            profile_id,
            kind,
            status,
            source_system,
            source_external_id,
            den_project_id,
            den_task_id,
            created_at,
            last_active_at,
            archived_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        ON CONFLICT(session_id) DO UPDATE SET
            instance_id = excluded.instance_id,
            agent_id = excluded.agent_id,
            profile_id = excluded.profile_id,
            kind = excluded.kind,
            status = excluded.status,
            source_system = excluded.source_system,
            source_external_id = excluded.source_external_id,
            den_project_id = excluded.den_project_id,
            den_task_id = excluded.den_task_id,
            last_active_at = excluded.last_active_at,
            archived_at = excluded.archived_at",
        params![
            record.session_id.0,
            record.instance_id.0,
            record.agent_id.0,
            record.profile_id.0,
            session_kind_as_str(&record.kind),
            session_status_as_str(&record.status),
            source_system,
            source_external_id,
            den_project_id,
            den_task_id,
            record.created_at,
            record.last_active_at,
            record.archived_at,
        ],
    )
    .map_err(|error| persistence_error("save session identity", error))?;
    Ok(())
}

fn load_session_identities(conn: &Connection) -> CoreResult<Vec<SessionIdentityRecord>> {
    let mut stmt = conn
        .prepare(
            "SELECT
                session_id,
                instance_id,
                agent_id,
                profile_id,
                kind,
                status,
                source_system,
                source_external_id,
                den_project_id,
                den_task_id,
                created_at,
                last_active_at,
                archived_at
             FROM session_identity
             ORDER BY session_id ASC",
        )
        .map_err(|error| persistence_error("prepare load session identities", error))?;
    let rows = stmt
        .query_map([], row_to_session_identity)
        .map_err(|error| persistence_error("query session identities", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load session identities", error))
}

fn row_to_agent_identity(row: &rusqlite::Row<'_>) -> rusqlite::Result<DurableAgentRecord> {
    Ok(DurableAgentRecord {
        agent_id: AgentId(row.get(0)?),
        display_label: row.get(1)?,
        profile_id: ProfileId(row.get(2)?),
        kind: durable_agent_kind_from_str(&row.get::<_, String>(3)?)?,
        status: durable_identity_status_from_str(&row.get::<_, String>(4)?)?,
        source: source_reference(row.get(5)?, row.get(6)?),
        den: den_reference(row.get(7)?, row.get(8)?),
        created_at: row.get(9)?,
        archived_at: row.get(10)?,
    })
}

fn row_to_agent_instance(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentInstanceRecord> {
    Ok(AgentInstanceRecord {
        instance_id: AgentInstanceId(row.get(0)?),
        agent_id: AgentId(row.get(1)?),
        display_label: row.get(2)?,
        profile_id: ProfileId(row.get(3)?),
        status: durable_identity_status_from_str(&row.get::<_, String>(4)?)?,
        source: source_reference(row.get(5)?, row.get(6)?),
        den: den_reference(row.get(7)?, row.get(8)?),
        created_at: row.get(9)?,
        last_active_at: row.get(10)?,
        archived_at: row.get(11)?,
    })
}

fn row_to_session_identity(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionIdentityRecord> {
    Ok(SessionIdentityRecord {
        session_id: SessionId(row.get(0)?),
        instance_id: AgentInstanceId(row.get(1)?),
        agent_id: AgentId(row.get(2)?),
        profile_id: ProfileId(row.get(3)?),
        kind: session_kind_from_str(&row.get::<_, String>(4)?)?,
        status: session_status_from_str(&row.get::<_, String>(5)?)?,
        source: source_reference(row.get(6)?, row.get(7)?),
        den: den_reference(row.get(8)?, row.get(9)?),
        created_at: row.get(10)?,
        last_active_at: row.get(11)?,
        archived_at: row.get(12)?,
    })
}

fn source_reference(
    system: Option<String>,
    external_id: Option<String>,
) -> Option<SourceSystemReference> {
    system
        .zip(external_id)
        .map(|(system, external_id)| SourceSystemReference {
            system,
            external_id,
        })
}

fn den_reference(project_id: Option<String>, task_id: Option<String>) -> DenRuntimeReference {
    DenRuntimeReference {
        project_id: project_id.map(ProjectId),
        task_id: task_id.map(TaskId),
    }
}

fn durable_agent_kind_from_session_kind(kind: &SessionKind) -> DurableAgentKind {
    match kind {
        SessionKind::Full => DurableAgentKind::Full,
        SessionKind::Worker => DurableAgentKind::WorkerPoolWorker,
        SessionKind::Delegated => DurableAgentKind::Delegated,
    }
}

fn durable_status_from_session_status(status: &SessionStatus) -> DurableIdentityStatus {
    match status {
        SessionStatus::Active | SessionStatus::Idle => DurableIdentityStatus::Active,
        SessionStatus::Archived => DurableIdentityStatus::Archived,
    }
}

fn durable_agent_kind_as_str(kind: &DurableAgentKind) -> &'static str {
    match kind {
        DurableAgentKind::Prime => "prime",
        DurableAgentKind::Full => "full",
        DurableAgentKind::Delegated => "delegated",
        DurableAgentKind::WorkerPoolWorker => "worker_pool_worker",
    }
}

fn durable_agent_kind_from_str(raw: &str) -> rusqlite::Result<DurableAgentKind> {
    match raw {
        "prime" => Ok(DurableAgentKind::Prime),
        "full" => Ok(DurableAgentKind::Full),
        "delegated" => Ok(DurableAgentKind::Delegated),
        "worker_pool_worker" => Ok(DurableAgentKind::WorkerPoolWorker),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            3,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown durable agent kind {other}"),
            )),
        )),
    }
}

fn durable_identity_status_as_str(status: &DurableIdentityStatus) -> &'static str {
    match status {
        DurableIdentityStatus::Active => "active",
        DurableIdentityStatus::Archived => "archived",
    }
}

fn durable_identity_status_from_str(raw: &str) -> rusqlite::Result<DurableIdentityStatus> {
    match raw {
        "active" => Ok(DurableIdentityStatus::Active),
        "archived" => Ok(DurableIdentityStatus::Archived),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            4,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown durable identity status {other}"),
            )),
        )),
    }
}

fn session_kind_as_str(kind: &SessionKind) -> &'static str {
    match kind {
        SessionKind::Full => "full",
        SessionKind::Worker => "worker",
        SessionKind::Delegated => "delegated",
    }
}

fn session_kind_from_str(raw: &str) -> rusqlite::Result<SessionKind> {
    match raw {
        "full" => Ok(SessionKind::Full),
        "worker" => Ok(SessionKind::Worker),
        "delegated" => Ok(SessionKind::Delegated),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            4,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown session kind {other}"),
            )),
        )),
    }
}

fn session_status_as_str(status: &SessionStatus) -> &'static str {
    match status {
        SessionStatus::Active => "active",
        SessionStatus::Idle => "idle",
        SessionStatus::Archived => "archived",
    }
}

fn session_status_from_str(raw: &str) -> rusqlite::Result<SessionStatus> {
    match raw {
        "active" => Ok(SessionStatus::Active),
        "idle" => Ok(SessionStatus::Idle),
        "archived" => Ok(SessionStatus::Archived),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            5,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown session status {other}"),
            )),
        )),
    }
}

fn should_persist_event(event: &CoreEvent) -> bool {
    !matches!(
        event,
        CoreEvent::DenDataUpdated { .. } | CoreEvent::ExternalEventInjected { .. }
    )
}

fn row_to_worker_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkerRunRecord> {
    let status: String = row.get(6)?;
    let fan_out_failure_policy: String = row.get(15)?;
    Ok(WorkerRunRecord {
        run_id: RunId(row.get(0)?),
        parent_session_id: SessionId(row.get(1)?),
        delegated_session_id: row.get::<_, Option<String>>(2)?.map(SessionId),
        parent_agent_id: row.get::<_, Option<String>>(3)?.map(AgentId),
        profile_id: ProfileId(row.get(4)?),
        task_id: row.get::<_, Option<String>>(5)?.map(TaskId),
        status: worker_run_status_from_str(&status)?,
        created_at: row.get(7)?,
        last_updated_at: row.get(8)?,
        source_wake_id: row.get(9)?,
        source_action_index: row.get::<_, i64>(10)? as u32,
        delegation_correlation_id: row.get(11)?,
        parent_consumption: parent_consumption_policy_from_str(&row.get::<_, String>(12)?)?,
        fan_out_group_id: row.get(13)?,
        fan_out_max_concurrency: row.get::<_, Option<i64>>(14)?.map(|value| value as u32),
        fan_out_failure_policy: fan_out_failure_policy_from_str(&fan_out_failure_policy)?,
    })
}

fn worker_run_status_from_str(raw: &str) -> rusqlite::Result<WorkerRunStatus> {
    match raw {
        "requested" => Ok(WorkerRunStatus::Requested),
        "session_created" => Ok(WorkerRunStatus::SessionCreated),
        "wake_requested" => Ok(WorkerRunStatus::WakeRequested),
        "running" => Ok(WorkerRunStatus::Running),
        "checkpoint_waiting" => Ok(WorkerRunStatus::CheckpointWaiting),
        "completed" => Ok(WorkerRunStatus::Completed),
        "failed" => Ok(WorkerRunStatus::Failed),
        "blocked" => Ok(WorkerRunStatus::Blocked),
        "exhausted" => Ok(WorkerRunStatus::Exhausted),
        "cancelled" => Ok(WorkerRunStatus::Cancelled),
        "expired" => Ok(WorkerRunStatus::Expired),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            6,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown worker run status {other}"),
            )),
        )),
    }
}

fn tool_call_phase_from_str(raw: &str) -> rusqlite::Result<ToolCallPhase> {
    match raw {
        "started" => Ok(ToolCallPhase::Started),
        "finished" => Ok(ToolCallPhase::Finished),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            4,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unsupported tool call phase {other}"),
            )),
        )),
    }
}

fn parent_consumption_policy_as_str(policy: &ParentConsumptionPolicy) -> &'static str {
    match policy {
        ParentConsumptionPolicy::AwaitCompletion => "await_completion",
        ParentConsumptionPolicy::ObserveOnly => "observe_only",
    }
}

fn parent_consumption_policy_from_str(raw: &str) -> rusqlite::Result<ParentConsumptionPolicy> {
    match raw {
        "await_completion" => Ok(ParentConsumptionPolicy::AwaitCompletion),
        "observe_only" => Ok(ParentConsumptionPolicy::ObserveOnly),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            12,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown parent consumption policy {other}"),
            )),
        )),
    }
}

fn fan_out_failure_policy_as_str(policy: &FanOutFailurePolicy) -> &'static str {
    match policy {
        FanOutFailurePolicy::FailFast => "fail_fast",
        FanOutFailurePolicy::FailSoft => "fail_soft",
    }
}

fn fan_out_failure_policy_from_str(raw: &str) -> rusqlite::Result<FanOutFailurePolicy> {
    match raw {
        "fail_fast" => Ok(FanOutFailurePolicy::FailFast),
        "fail_soft" => Ok(FanOutFailurePolicy::FailSoft),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            15,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown fan-out failure policy {other}"),
            )),
        )),
    }
}

fn aggregate_fan_out_groups(mut runs: Vec<WorkerRunRecord>) -> Vec<DelegatedFanOutGroup> {
    runs.sort_by(|left, right| {
        left.fan_out_group_id
            .cmp(&right.fan_out_group_id)
            .then_with(|| left.source_wake_id.cmp(&right.source_wake_id))
            .then_with(|| left.source_action_index.cmp(&right.source_action_index))
    });

    let mut groups = Vec::new();
    let mut index = 0;
    while index < runs.len() {
        let Some(group_id) = runs[index].fan_out_group_id.clone() else {
            index += 1;
            continue;
        };
        let mut group_runs = Vec::new();
        while index < runs.len() && runs[index].fan_out_group_id.as_deref() == Some(&group_id) {
            group_runs.push(runs[index].clone());
            index += 1;
        }
        groups.push(aggregate_fan_out_group(group_id, &group_runs));
    }
    groups
}

fn aggregate_fan_out_group(group_id: String, runs: &[WorkerRunRecord]) -> DelegatedFanOutGroup {
    let mut group = DelegatedFanOutGroup {
        group_id,
        total: runs.len() as u32,
        pending: 0,
        completed: 0,
        failed: 0,
        blocked: 0,
        exhausted: 0,
        cancelled: 0,
        expired: 0,
        max_concurrency: runs.iter().find_map(|run| run.fan_out_max_concurrency),
        failure_policy: runs
            .iter()
            .find(|run| run.fan_out_failure_policy == FanOutFailurePolicy::FailFast)
            .map(|run| run.fan_out_failure_policy.clone())
            .unwrap_or(FanOutFailurePolicy::FailSoft),
        status: FanOutGroupStatus::InProgress,
    };

    for run in runs {
        match run.status {
            WorkerRunStatus::Requested
            | WorkerRunStatus::SessionCreated
            | WorkerRunStatus::WakeRequested
            | WorkerRunStatus::Running
            | WorkerRunStatus::CheckpointWaiting => group.pending += 1,
            WorkerRunStatus::Completed => group.completed += 1,
            WorkerRunStatus::Failed => group.failed += 1,
            WorkerRunStatus::Blocked => group.blocked += 1,
            WorkerRunStatus::Exhausted => group.exhausted += 1,
            WorkerRunStatus::Cancelled => group.cancelled += 1,
            WorkerRunStatus::Expired => group.expired += 1,
        }
    }

    let non_success =
        group.failed + group.blocked + group.exhausted + group.cancelled + group.expired;
    group.status = if group.pending > 0 {
        if group.failure_policy == FanOutFailurePolicy::FailFast && non_success > 0 {
            FanOutGroupStatus::FailedFast
        } else {
            FanOutGroupStatus::InProgress
        }
    } else if non_success == 0 {
        FanOutGroupStatus::Completed
    } else if group.failure_policy == FanOutFailurePolicy::FailFast {
        FanOutGroupStatus::FailedFast
    } else {
        FanOutGroupStatus::PartialFailure
    };

    group
}

fn add_missing_column(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> CoreResult<()> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| persistence_error("prepare table info", error))?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| persistence_error("query table info", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("read table info", error))?;

    if columns.iter().any(|existing| existing == column) {
        return Ok(());
    }

    conn.execute(
        &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
        [],
    )
    .map_err(|error| persistence_error("add missing sqlite column", error))?;
    Ok(())
}

fn to_json_text<T: Serialize>(value: &T) -> CoreResult<String> {
    serde_json::to_string(value)
        .map_err(|error| persistence_error("serialize coordination record", error))
}

fn from_json_text<T: DeserializeOwned>(value: &str) -> Result<T, serde_json::Error> {
    serde_json::from_str(value)
}

fn to_sql_error(error: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}

fn persistence_error(context: &str, error: impl std::error::Error) -> CoreError {
    CoreError::new(
        CoreErrorKind::PersistenceFailure,
        format!("{context}: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_crew_core_protocol::{AgentMessage, ToolDescriptor};
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn diagnostic_table_names_are_whitelisted() {
        for table in DiagnosticTable::ALL {
            assert_eq!(DiagnosticTable::parse(table.as_str()).unwrap(), *table);
        }
        let error = DiagnosticTable::parse("sessions; DROP TABLE sessions").unwrap_err();
        assert_eq!(error.kind, CoreErrorKind::InvalidInput);
    }

    #[test]
    fn fresh_database_applies_all_schema_migrations() {
        let db_path = temp_db_path("fresh-schema");
        let store = CoordinationStore::open_file(&db_path).unwrap();

        assert_eq!(store.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
        assert_eq!(
            store.schema_migrations().unwrap().len(),
            SCHEMA_MIGRATIONS.len()
        );
        assert_eq!(store.count_rows("sessions").unwrap(), 0);

        remove_temp_db(&db_path);
    }

    #[test]
    fn version_one_database_migrates_to_current_schema() {
        let db_path = temp_db_path("migrated-schema");
        {
            let mut conn = Connection::open(&db_path).unwrap();
            prepare_migration_metadata(&conn).unwrap();
            apply_schema_migrations(&mut conn, &SCHEMA_MIGRATIONS[..1]).unwrap();
        }

        let store = CoordinationStore::open_file(&db_path).unwrap();

        assert_eq!(store.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
        assert!(table_has_column(&db_path, "sessions", "tool_profile_json"));
        assert!(table_has_column(
            &db_path,
            "worker_runs",
            "fan_out_failure_policy"
        ));
        assert!(table_exists(&db_path, "agents"));
        assert!(table_exists(&db_path, "agent_instances"));
        assert!(table_exists(&db_path, "session_configs"));
        assert!(table_exists(&db_path, "session_identity"));
        assert!(table_exists(&db_path, "event_session_index"));
        assert!(table_exists(&db_path, "event_agent_index"));
        assert!(table_exists(&db_path, "runtime_search_fts"));
        assert!(table_exists(&db_path, "runtime_counters"));
        assert!(table_exists(&db_path, "queued_messages"));
        assert!(table_exists(&db_path, "runtime_import_batches"));
        assert!(table_exists(&db_path, "legacy_id_mappings"));
        assert!(table_exists(&db_path, "profile_memories"));
        assert!(table_exists(&db_path, "scheduled_jobs"));
        assert!(table_exists(&db_path, "scheduled_job_runs"));
        assert!(table_exists(&db_path, "channel_bindings"));
        assert!(table_exists(&db_path, "mcp_bindings"));
        assert!(index_exists(
            &db_path,
            "idx_worker_runs_parent_status_created"
        ));
        assert!(index_exists(
            &db_path,
            "idx_profile_memories_profile_updated"
        ));
        assert!(index_exists(&db_path, "idx_scheduled_jobs_due"));
        assert!(index_exists(
            &db_path,
            "idx_scheduled_job_runs_status_deadline"
        ));
        assert!(index_exists(&db_path, "idx_channel_bindings_external"));
        assert!(index_exists(&db_path, "idx_mcp_bindings_agent_profile"));

        remove_temp_db(&db_path);
    }

    #[test]
    fn legacy_import_metadata_maps_pi_crew_and_hermes_ids_without_runtime_coupling() {
        let db_path = temp_db_path("legacy-import-metadata");
        let store = CoordinationStore::open_file(&db_path).unwrap();

        store
            .save_import_batch(&RuntimeImportBatchRecord {
                import_batch_id: "import-pi-crew-001".to_string(),
                source_system: "pi-crew".to_string(),
                source_label: "pi-crew production snapshot".to_string(),
                source_snapshot_ref: Some("/backup/pi-crew/2026-06-20.sqlite3".to_string()),
                notes: Some("worker-pool history imported as provenance only".to_string()),
                imported_at: "2026-06-20T03:00:00Z".to_string(),
            })
            .unwrap();
        store
            .save_import_batch(&RuntimeImportBatchRecord {
                import_batch_id: "import-hermes-001".to_string(),
                source_system: "hermes".to_string(),
                source_label: "Hermes profile sqlite exports".to_string(),
                source_snapshot_ref: Some("/backup/hermes/profiles".to_string()),
                notes: Some("one sqlite source per profile".to_string()),
                imported_at: "2026-06-20T03:05:00Z".to_string(),
            })
            .unwrap();

        store
            .save_legacy_id_mapping(&LegacyIdMappingRecord {
                import_batch_id: "import-pi-crew-001".to_string(),
                source: SourceSystemReference {
                    system: "pi-crew".to_string(),
                    external_id: "worker-run:abc123".to_string(),
                },
                legacy_kind: RuntimeObjectKind::WorkerRun,
                rusty_kind: RuntimeObjectKind::WorkerRun,
                rusty_id: "run-rusty-001".to_string(),
                provenance: RuntimeImportProvenance {
                    profile_id: Some(ProfileId::new("coder-profile")),
                    session_id: Some(SessionId::new("session-rusty-001")),
                    agent_id: Some(AgentId::new("agent-rusty")),
                    externally_owned: false,
                    notes: Some("pi-crew worker-pool run mapped to delegated run".to_string()),
                },
                created_at: "2026-06-20T03:10:00Z".to_string(),
            })
            .unwrap();
        store
            .save_legacy_id_mapping(&LegacyIdMappingRecord {
                import_batch_id: "import-hermes-001".to_string(),
                source: SourceSystemReference {
                    system: "hermes".to_string(),
                    external_id: "profile-db:/home/dev/.hermes/profiles/alpha.sqlite3".to_string(),
                },
                legacy_kind: RuntimeObjectKind::ExternalArtifact,
                rusty_kind: RuntimeObjectKind::Profile,
                rusty_id: "profile-alpha".to_string(),
                provenance: RuntimeImportProvenance {
                    profile_id: Some(ProfileId::new("profile-alpha")),
                    session_id: None,
                    agent_id: None,
                    externally_owned: true,
                    notes: Some("Hermes source database remains external".to_string()),
                },
                created_at: "2026-06-20T03:11:00Z".to_string(),
            })
            .unwrap();

        assert_eq!(store.load_import_batches().unwrap().len(), 2);
        let pi_crew_mapping = store
            .query_legacy_id_mappings(&LegacyIdMappingQuery {
                source_system: Some("pi-crew".to_string()),
                legacy_kind: Some(RuntimeObjectKind::WorkerRun),
                ..LegacyIdMappingQuery::default()
            })
            .unwrap();
        assert_eq!(pi_crew_mapping.len(), 1);
        assert_eq!(pi_crew_mapping[0].rusty_id, "run-rusty-001");
        assert!(!pi_crew_mapping[0].provenance.externally_owned);

        let hermes_mapping = store
            .query_legacy_id_mappings(&LegacyIdMappingQuery {
                rusty_kind: Some(RuntimeObjectKind::Profile),
                rusty_id: Some("profile-alpha".to_string()),
                ..LegacyIdMappingQuery::default()
            })
            .unwrap();
        assert_eq!(hermes_mapping.len(), 1);
        assert_eq!(hermes_mapping[0].source.system, "hermes");
        assert!(hermes_mapping[0].provenance.externally_owned);
        assert_eq!(store.count_rows("runtime_import_batches").unwrap(), 2);
        assert_eq!(store.count_rows("legacy_id_mappings").unwrap(), 2);

        remove_temp_db(&db_path);
    }

    #[test]
    fn external_bindings_are_scoped_per_agent_without_secret_material() {
        let db_path = temp_db_path("external-bindings");
        let store = CoordinationStore::open_file(&db_path).unwrap();

        let base_provenance = ExternalBindingProvenance {
            source_system: Some("den-channels".to_string()),
            source_ref: Some("den-channel:crew-room".to_string()),
            externally_owned: true,
            notes: Some("provider secret remains in adapter config".to_string()),
        };
        let alpha_channel = ChannelBindingRecord {
            binding_id: "channel-alpha".to_string(),
            adapter_id: AdapterId::new("den-channels-main"),
            provider: "den_channels".to_string(),
            agent_id: AgentId::new("agent-alpha"),
            instance_id: Some(AgentInstanceId::new("instance-alpha")),
            session_id: Some(SessionId::new("session-alpha")),
            profile_id: ProfileId::new("prime-profile"),
            external_channel_id: "crew-room".to_string(),
            external_thread_id: Some("thread-42".to_string()),
            external_user_id: Some("den-user-alpha".to_string()),
            provider_subscription_id: Some("sub-alpha".to_string()),
            cursor: Some("cursor-alpha".to_string()),
            membership_state: Some("joined".to_string()),
            presence_state: Some("online".to_string()),
            status: ExternalBindingStatus::Active,
            degraded_reason: None,
            provenance: base_provenance.clone(),
            created_at: "2026-06-20T04:00:00Z".to_string(),
            updated_at: "2026-06-20T04:01:00Z".to_string(),
        };
        let beta_channel = ChannelBindingRecord {
            binding_id: "channel-beta".to_string(),
            agent_id: AgentId::new("agent-beta"),
            instance_id: Some(AgentInstanceId::new("instance-beta")),
            session_id: Some(SessionId::new("session-beta")),
            profile_id: ProfileId::new("review-profile"),
            provider_subscription_id: Some("sub-beta".to_string()),
            cursor: Some("cursor-beta".to_string()),
            presence_state: Some("idle".to_string()),
            updated_at: "2026-06-20T04:02:00Z".to_string(),
            ..alpha_channel.clone()
        };

        store.save_channel_binding(&alpha_channel).unwrap();
        store.save_channel_binding(&beta_channel).unwrap();

        let shared_channel = store
            .query_channel_bindings(&ChannelBindingQuery {
                provider: Some("den_channels".to_string()),
                external_channel_id: Some("crew-room".to_string()),
                ..ChannelBindingQuery::default()
            })
            .unwrap();
        let alpha_only = store
            .query_channel_bindings(&ChannelBindingQuery {
                agent_id: Some(AgentId::new("agent-alpha")),
                status: Some(ExternalBindingStatus::Active),
                ..ChannelBindingQuery::default()
            })
            .unwrap();

        assert_eq!(shared_channel.len(), 2);
        assert_eq!(alpha_only.len(), 1);
        assert_eq!(
            alpha_only[0].provider_subscription_id.as_deref(),
            Some("sub-alpha")
        );
        assert_eq!(alpha_only[0].cursor.as_deref(), Some("cursor-alpha"));
        assert_eq!(alpha_only[0].profile_id, ProfileId::new("prime-profile"));

        store
            .save_mcp_binding(&McpBindingRecord {
                binding_id: "mcp-alpha".to_string(),
                adapter_id: AdapterId::new("mcp-ts-main"),
                agent_id: AgentId::new("agent-alpha"),
                instance_id: Some(AgentInstanceId::new("instance-alpha")),
                session_id: Some(SessionId::new("session-alpha")),
                profile_id: ProfileId::new("prime-profile"),
                server_names: vec!["den".to_string(), "filesystem".to_string()],
                endpoint_ref: "config://mcp/alpha".to_string(),
                transport: "stdio".to_string(),
                tool_profile_key: "tool-profile-alpha".to_string(),
                discovered_tool_revision: Some("rev-alpha".to_string()),
                status: ExternalBindingStatus::Active,
                degraded_reason: None,
                diagnostics: McpBindingDiagnostics {
                    last_error: None,
                    last_checked_at: Some("2026-06-20T04:05:00Z".to_string()),
                    notes: Some("no secret fields".to_string()),
                },
                created_at: "2026-06-20T04:00:00Z".to_string(),
                updated_at: "2026-06-20T04:05:00Z".to_string(),
            })
            .unwrap();
        store
            .save_mcp_binding(&McpBindingRecord {
                binding_id: "mcp-beta".to_string(),
                adapter_id: AdapterId::new("mcp-ts-main"),
                agent_id: AgentId::new("agent-beta"),
                instance_id: Some(AgentInstanceId::new("instance-beta")),
                session_id: Some(SessionId::new("session-beta")),
                profile_id: ProfileId::new("review-profile"),
                server_names: vec!["den".to_string()],
                endpoint_ref: "config://mcp/beta".to_string(),
                transport: "stdio".to_string(),
                tool_profile_key: "tool-profile-beta".to_string(),
                discovered_tool_revision: Some("rev-beta".to_string()),
                status: ExternalBindingStatus::Degraded,
                degraded_reason: Some("tool discovery stale".to_string()),
                diagnostics: McpBindingDiagnostics {
                    last_error: Some("catalog revision mismatch".to_string()),
                    last_checked_at: Some("2026-06-20T04:06:00Z".to_string()),
                    notes: None,
                },
                created_at: "2026-06-20T04:00:00Z".to_string(),
                updated_at: "2026-06-20T04:06:00Z".to_string(),
            })
            .unwrap();

        let alpha_mcp = store
            .query_mcp_bindings(&McpBindingQuery {
                session_id: Some(SessionId::new("session-alpha")),
                ..McpBindingQuery::default()
            })
            .unwrap();
        let degraded = store
            .query_mcp_bindings(&McpBindingQuery {
                status: Some(ExternalBindingStatus::Degraded),
                ..McpBindingQuery::default()
            })
            .unwrap();

        assert_eq!(alpha_mcp.len(), 1);
        assert_eq!(
            alpha_mcp[0].server_names,
            vec!["den".to_string(), "filesystem".to_string()]
        );
        assert_eq!(alpha_mcp[0].endpoint_ref, "config://mcp/alpha");
        assert_eq!(alpha_mcp[0].tool_profile_key, "tool-profile-alpha");
        assert!(!alpha_mcp[0].endpoint_ref.contains("secret"));
        assert_eq!(degraded.len(), 1);
        assert_eq!(degraded[0].agent_id, AgentId::new("agent-beta"));
        assert_eq!(
            degraded[0].diagnostics.last_error.as_deref(),
            Some("catalog revision mismatch")
        );
        assert_eq!(store.count_rows("channel_bindings").unwrap(), 2);
        assert_eq!(store.count_rows("mcp_bindings").unwrap(), 2);

        remove_temp_db(&db_path);
    }

    #[test]
    fn profile_memory_supports_caps_revisions_and_profile_isolation() {
        let db_path = temp_db_path("profile-memory");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        let caps = ProfileMemoryCaps {
            max_records_per_profile: 2,
            max_key_bytes: 32,
            max_content_bytes: 64,
        };

        let added = store
            .add_profile_memory(
                &ProfileMemoryWrite {
                    profile_id: ProfileId::new("prime-profile"),
                    target: ProfileMemoryTarget::Profile,
                    key: "style".to_string(),
                    content: "prefers concise handoffs".to_string(),
                    metadata: serde_json::json!({"source": "smoke"}),
                    now: "2026-06-20T05:00:00Z".to_string(),
                },
                &caps,
            )
            .unwrap();
        assert_eq!(added.revision, 1);
        assert_eq!(added.target, ProfileMemoryTarget::Profile);

        let replaced = store
            .replace_profile_memory(
                &ProfileMemoryReplace {
                    expected_revision: added.revision,
                    write: ProfileMemoryWrite {
                        profile_id: ProfileId::new("prime-profile"),
                        target: ProfileMemoryTarget::Profile,
                        key: "style".to_string(),
                        content: "prefers concise handoffs with citations".to_string(),
                        metadata: serde_json::json!({"source": "replacement"}),
                        now: "2026-06-20T05:01:00Z".to_string(),
                    },
                },
                &caps,
            )
            .unwrap();
        assert_eq!(replaced.revision, 2);
        assert_eq!(replaced.created_at, "2026-06-20T05:00:00Z");
        assert_eq!(replaced.updated_at, "2026-06-20T05:01:00Z");

        let stale_replace = store
            .replace_profile_memory(
                &ProfileMemoryReplace {
                    expected_revision: 1,
                    write: ProfileMemoryWrite {
                        now: "2026-06-20T05:02:00Z".to_string(),
                        ..replaced_write("prime-profile", ProfileMemoryTarget::Profile, "style")
                    },
                },
                &caps,
            )
            .unwrap_err();
        assert_eq!(stale_replace.kind, CoreErrorKind::ActionRejected);

        store
            .add_profile_memory(
                &ProfileMemoryWrite {
                    profile_id: ProfileId::new("prime-profile"),
                    target: ProfileMemoryTarget::User("den-user-alpha".to_string()),
                    key: "salutation".to_string(),
                    content: "likes direct updates".to_string(),
                    metadata: serde_json::json!({"scope": "user"}),
                    now: "2026-06-20T05:03:00Z".to_string(),
                },
                &caps,
            )
            .unwrap();
        let cap_error = store
            .add_profile_memory(
                &ProfileMemoryWrite {
                    profile_id: ProfileId::new("prime-profile"),
                    target: ProfileMemoryTarget::Profile,
                    key: "third".to_string(),
                    content: "would exceed cap".to_string(),
                    metadata: serde_json::json!({}),
                    now: "2026-06-20T05:04:00Z".to_string(),
                },
                &caps,
            )
            .unwrap_err();
        assert_eq!(cap_error.kind, CoreErrorKind::ActionRejected);

        store
            .add_profile_memory(
                &ProfileMemoryWrite {
                    profile_id: ProfileId::new("review-profile"),
                    target: ProfileMemoryTarget::Profile,
                    key: "style".to_string(),
                    content: "prefers detailed risk notes".to_string(),
                    metadata: serde_json::json!({}),
                    now: "2026-06-20T05:05:00Z".to_string(),
                },
                &caps,
            )
            .unwrap();

        let prime_rows = store
            .list_profile_memory(&ProfileMemoryQuery {
                profile_id: ProfileId::new("prime-profile"),
                target: None,
                page: None,
            })
            .unwrap();
        assert_eq!(prime_rows.len(), 2);
        assert!(prime_rows
            .iter()
            .all(|row| row.profile_id == ProfileId::new("prime-profile")));

        let profile_style = store
            .get_profile_memory(
                &ProfileId::new("prime-profile"),
                &ProfileMemoryTarget::Profile,
                "style",
            )
            .unwrap()
            .unwrap();
        let user_style = store
            .get_profile_memory(
                &ProfileId::new("prime-profile"),
                &ProfileMemoryTarget::User("den-user-alpha".to_string()),
                "salutation",
            )
            .unwrap()
            .unwrap();
        assert_ne!(profile_style.target, user_style.target);

        let stale_delete = store
            .remove_profile_memory(&ProfileMemoryDelete {
                profile_id: ProfileId::new("prime-profile"),
                target: ProfileMemoryTarget::Profile,
                key: "style".to_string(),
                expected_revision: 1,
            })
            .unwrap_err();
        assert_eq!(stale_delete.kind, CoreErrorKind::ActionRejected);

        let removed = store
            .remove_profile_memory(&ProfileMemoryDelete {
                profile_id: ProfileId::new("prime-profile"),
                target: ProfileMemoryTarget::Profile,
                key: "style".to_string(),
                expected_revision: 2,
            })
            .unwrap();
        assert_eq!(removed.key, "style");
        assert!(store
            .get_profile_memory(
                &ProfileId::new("prime-profile"),
                &ProfileMemoryTarget::Profile,
                "style"
            )
            .unwrap()
            .is_none());

        let too_large = store
            .add_profile_memory(
                &ProfileMemoryWrite {
                    profile_id: ProfileId::new("review-profile"),
                    target: ProfileMemoryTarget::Profile,
                    key: "large".to_string(),
                    content: "x".repeat(65),
                    metadata: serde_json::json!({}),
                    now: "2026-06-20T05:06:00Z".to_string(),
                },
                &caps,
            )
            .unwrap_err();
        assert_eq!(too_large.kind, CoreErrorKind::ActionRejected);

        remove_temp_db(&db_path);
    }

    #[test]
    fn future_schema_version_fails_closed() {
        let db_path = temp_db_path("future-schema");
        {
            let conn = Connection::open(&db_path).unwrap();
            prepare_migration_metadata(&conn).unwrap();
            conn.execute(
                "INSERT INTO schema_migrations (version, description) VALUES (?1, ?2)",
                params![CURRENT_SCHEMA_VERSION + 1, "future migration"],
            )
            .unwrap();
        }

        let error = CoordinationStore::open_file(&db_path).unwrap_err();

        assert_eq!(error.kind, CoreErrorKind::PersistenceFailure);
        assert!(error.message.contains("newer than supported"));

        remove_temp_db(&db_path);
    }

    #[test]
    fn scheduled_jobs_claim_runs_and_reconcile_stale_claims() {
        let db_path = temp_db_path("scheduled-jobs");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        store
            .upsert_scheduled_job(&ScheduledJobRecord {
                job_id: "wake-prime".to_string(),
                job_kind: "runtime.wake.session".to_string(),
                target_session_id: Some(SessionId::new("prime-session")),
                interval_ms: Some(60_000),
                next_due_at: Some("2026-06-20T06:00:00Z".to_string()),
                payload_json: serde_json::json!({"reason": "scheduled"}),
                status: ScheduledJobStatus::Active,
                created_at: "2026-06-20T05:59:00Z".to_string(),
                updated_at: "2026-06-20T05:59:00Z".to_string(),
                paused_at: None,
            })
            .unwrap();

        let due = store
            .query_scheduled_jobs(&ScheduledJobQuery {
                status: Some(ScheduledJobStatus::Active),
                due_at_or_before: Some("2026-06-20T06:00:00Z".to_string()),
                ..ScheduledJobQuery::default()
            })
            .unwrap();
        assert_eq!(due.len(), 1);

        let run = ScheduledRunRecord {
            run_id: RunId::new("scheduled:wake-prime:1"),
            job_id: "wake-prime".to_string(),
            job_kind: "runtime.wake.session".to_string(),
            target_session_id: Some(SessionId::new("prime-session")),
            status: ScheduledRunStatus::Claimed,
            trigger: ScheduledRunTrigger::Due,
            scheduled_for: Some("2026-06-20T06:00:00Z".to_string()),
            claimed_at: "2026-06-20T06:00:00Z".to_string(),
            claim_deadline_at: "2026-06-20T06:00:30Z".to_string(),
            completed_at: None,
            error: None,
            output_json: serde_json::json!({}),
            created_at: "2026-06-20T06:00:00Z".to_string(),
            updated_at: "2026-06-20T06:00:00Z".to_string(),
        };
        store
            .claim_scheduled_run(&run, Some(&"2026-06-20T06:01:00Z".to_string()))
            .unwrap();
        assert_eq!(
            store
                .load_scheduled_job("wake-prime")
                .unwrap()
                .unwrap()
                .next_due_at,
            Some("2026-06-20T06:01:00Z".to_string())
        );

        store
            .complete_scheduled_run(
                &run.run_id,
                ScheduledRunStatus::Completed,
                &"2026-06-20T06:00:01Z".to_string(),
                &serde_json::json!({"wake_requested": true}),
                None,
            )
            .unwrap();
        let completed = store
            .query_scheduled_runs(&ScheduledRunQuery {
                status: Some(ScheduledRunStatus::Completed),
                ..ScheduledRunQuery::default()
            })
            .unwrap();
        assert_eq!(completed.len(), 1);

        store
            .claim_scheduled_run(
                &ScheduledRunRecord {
                    run_id: RunId::new("scheduled:wake-prime:2"),
                    status: ScheduledRunStatus::Claimed,
                    trigger: ScheduledRunTrigger::Manual,
                    claimed_at: "2026-06-20T06:02:00Z".to_string(),
                    claim_deadline_at: "2026-06-20T06:02:05Z".to_string(),
                    created_at: "2026-06-20T06:02:00Z".to_string(),
                    updated_at: "2026-06-20T06:02:00Z".to_string(),
                    scheduled_for: None,
                    completed_at: None,
                    error: None,
                    output_json: serde_json::json!({}),
                    ..run.clone()
                },
                None,
            )
            .unwrap();
        let expired = store
            .expire_stale_scheduled_runs(
                &"2026-06-20T06:02:06Z".to_string(),
                &"2026-06-20T06:02:06Z".to_string(),
            )
            .unwrap();
        assert_eq!(expired.len(), 1);
        assert_eq!(
            store
                .query_scheduled_runs(&ScheduledRunQuery {
                    status: Some(ScheduledRunStatus::Expired),
                    ..ScheduledRunQuery::default()
                })
                .unwrap()
                .len(),
            1
        );

        store
            .pause_scheduled_job("wake-prime", &"2026-06-20T06:03:00Z".to_string())
            .unwrap();
        assert_eq!(
            store
                .load_scheduled_job("wake-prime")
                .unwrap()
                .unwrap()
                .status,
            ScheduledJobStatus::Paused
        );
        store
            .resume_scheduled_job(
                "wake-prime",
                &"2026-06-20T06:04:00Z".to_string(),
                &"2026-06-20T06:03:30Z".to_string(),
            )
            .unwrap();
        assert_eq!(
            store
                .load_scheduled_job("wake-prime")
                .unwrap()
                .unwrap()
                .next_due_at,
            Some("2026-06-20T06:04:00Z".to_string())
        );

        remove_temp_db(&db_path);
    }

    #[test]
    fn failed_schema_migration_rolls_back_partial_ddl() {
        fn create_then_fail(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
            tx.execute_batch("CREATE TABLE partial_migration_marker (id INTEGER PRIMARY KEY);")
                .map_err(|error| persistence_error("create partial migration marker", error))?;
            Err(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "intentional migration failure",
            ))
        }

        let db_path = temp_db_path("rollback-schema");
        let mut conn = Connection::open(&db_path).unwrap();
        prepare_migration_metadata(&conn).unwrap();
        let failing_migrations = [SchemaMigration {
            version: 1,
            description: "create table then fail",
            apply: create_then_fail,
        }];

        let error = apply_schema_migrations(&mut conn, &failing_migrations).unwrap_err();

        assert_eq!(error.kind, CoreErrorKind::PersistenceFailure);
        assert!(!table_exists(&db_path, "partial_migration_marker"));
        assert_eq!(current_schema_version(&conn).unwrap(), 0);

        drop(conn);
        remove_temp_db(&db_path);
    }

    #[test]
    fn saving_session_projects_durable_identity_records() {
        let db_path = temp_db_path("session-identity");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        store.save_session(&sample_session_state()).unwrap();

        let agents = store.load_agent_identities().unwrap();
        let instances = store.load_agent_instances().unwrap();
        let sessions = store.load_session_identities().unwrap();

        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].agent_id, AgentId::new("agent-alpha"));
        assert_eq!(agents[0].kind, DurableAgentKind::Full);
        assert_eq!(instances.len(), 1);
        assert_eq!(
            instances[0].instance_id,
            AgentInstanceId::new("instance:session-alpha")
        );
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, SessionId::new("session-alpha"));
        assert_eq!(
            sessions[0].instance_id,
            AgentInstanceId::new("instance:session-alpha")
        );

        remove_temp_db(&db_path);
    }

    #[test]
    fn explicit_identity_records_round_trip_source_and_den_references() {
        let db_path = temp_db_path("explicit-identity");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        let den = DenRuntimeReference {
            project_id: Some(ProjectId::new("pi-crew")),
            task_id: Some(TaskId::new("123")),
        };
        let source = Some(SourceSystemReference {
            system: "hermes".to_string(),
            external_id: "hermes-agent-1".to_string(),
        });

        store
            .upsert_agent_identity(&DurableAgentRecord {
                agent_id: AgentId::new("agent-imported"),
                display_label: "Imported Agent".to_string(),
                profile_id: ProfileId::new("prime-profile"),
                kind: DurableAgentKind::Prime,
                status: DurableIdentityStatus::Active,
                source: source.clone(),
                den: den.clone(),
                created_at: "2026-06-20T01:00:00Z".to_string(),
                archived_at: None,
            })
            .unwrap();
        store
            .upsert_agent_instance(&AgentInstanceRecord {
                instance_id: AgentInstanceId::new("instance-imported"),
                agent_id: AgentId::new("agent-imported"),
                display_label: "Imported Agent / main".to_string(),
                profile_id: ProfileId::new("prime-profile"),
                status: DurableIdentityStatus::Active,
                source: source.clone(),
                den: den.clone(),
                created_at: "2026-06-20T01:00:00Z".to_string(),
                last_active_at: "2026-06-20T01:05:00Z".to_string(),
                archived_at: None,
            })
            .unwrap();
        store
            .upsert_session_identity(&SessionIdentityRecord {
                session_id: SessionId::new("session-imported"),
                instance_id: AgentInstanceId::new("instance-imported"),
                agent_id: AgentId::new("agent-imported"),
                profile_id: ProfileId::new("prime-profile"),
                kind: SessionKind::Full,
                status: SessionStatus::Active,
                source,
                den,
                created_at: "2026-06-20T01:00:00Z".to_string(),
                last_active_at: "2026-06-20T01:05:00Z".to_string(),
                archived_at: None,
            })
            .unwrap();

        let agent = store.load_agent_identities().unwrap().remove(0);
        let instance = store.load_agent_instances().unwrap().remove(0);
        let session = store.load_session_identities().unwrap().remove(0);

        assert_eq!(agent.kind, DurableAgentKind::Prime);
        assert_eq!(
            agent.source.unwrap().external_id,
            "hermes-agent-1".to_string()
        );
        assert_eq!(instance.den.project_id, Some(ProjectId::new("pi-crew")));
        assert_eq!(session.den.task_id, Some(TaskId::new("123")));

        remove_temp_db(&db_path);
    }

    #[test]
    fn session_config_snapshot_is_immutable_creation_context() {
        let db_path = temp_db_path("session-config");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        let config = sample_session_config();
        let mut state = sample_session_state();
        store.save_session_with_config(&state, &config).unwrap();

        state.resource_limits.max_duration_ms = Some(10);
        state.tool_profile.tools.clear();
        state.last_active_at = "2026-06-20T00:10:00Z".to_string();
        store.save_session(&state).unwrap();

        let live_state = store.load_sessions().unwrap().remove(0);
        let config_snapshot = store.load_session_configs().unwrap().remove(0);

        assert_eq!(live_state.resource_limits.max_duration_ms, Some(10));
        assert_eq!(live_state.tool_profile.tools.len(), 0);
        assert_eq!(
            config_snapshot.resource_limits.max_duration_ms,
            Some(60_000)
        );
        assert_eq!(config_snapshot.tool_profile.tools.len(), 1);
        assert_eq!(
            config_snapshot.config.resource_limits.max_delegation_depth,
            Some(4)
        );
        assert_eq!(config_snapshot.created_at, state.created_at);

        remove_temp_db(&db_path);
    }

    #[test]
    fn event_log_projection_indexes_support_typed_queries() {
        let db_path = temp_db_path("event-projections");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        let session = sample_session_state();

        store
            .save_event(
                1,
                &CoreEvent::SessionCreated {
                    state: Box::new(session.clone()),
                },
            )
            .unwrap();
        store
            .save_event(
                2,
                &CoreEvent::AgentMessageRouted {
                    message: AgentMessage {
                        from: AgentId::new("agent-alpha"),
                        to: AgentId::new("agent-beta"),
                        body: "hello".to_string(),
                        correlation_id: Some("corr-1".to_string()),
                    },
                },
            )
            .unwrap();
        store
            .save_event(
                3,
                &CoreEvent::BrainEventObserved {
                    session_id: session.session_id.clone(),
                    wake_id: Some("wake-1".to_string()),
                    event: BrainEvent::Started,
                },
            )
            .unwrap();

        let by_session = store
            .query_events(&RuntimeEventFilter {
                session_id: Some(SessionId::new("session-alpha")),
                ..RuntimeEventFilter::default()
            })
            .unwrap();
        let by_agent = store
            .query_events(&RuntimeEventFilter {
                agent_id: Some(AgentId::new("agent-beta")),
                ..RuntimeEventFilter::default()
            })
            .unwrap();
        let by_correlation = store
            .query_events(&RuntimeEventFilter {
                correlation_id: Some("corr-1".to_string()),
                ..RuntimeEventFilter::default()
            })
            .unwrap();
        let by_wake = store
            .query_events(&RuntimeEventFilter {
                source_wake_id: Some("wake-1".to_string()),
                ..RuntimeEventFilter::default()
            })
            .unwrap();

        assert_eq!(by_session.len(), 2);
        assert_eq!(
            by_session[0].session_ids,
            vec![SessionId::new("session-alpha")]
        );
        assert_eq!(
            by_session[0].instance_ids,
            vec![AgentInstanceId::new("instance:session-alpha")]
        );
        assert_eq!(by_agent.len(), 1);
        assert_eq!(by_agent[0].agent_ids.len(), 2);
        assert_eq!(by_correlation.len(), 1);
        assert_eq!(by_correlation[0].correlation_ids, vec!["corr-1"]);
        assert_eq!(by_wake.len(), 1);
        assert_eq!(by_wake[0].source_wake_ids, vec!["wake-1"]);
        assert_eq!(store.count_rows("event_session_index").unwrap(), 2);

        remove_temp_db(&db_path);
    }

    #[test]
    fn runtime_search_indexes_messages_and_session_configs() {
        let db_path = temp_db_path("runtime-search");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        let config = sample_session_config();
        let state = sample_session_state();
        store.save_session_with_config(&state, &config).unwrap();
        store
            .save_event(
                1,
                &CoreEvent::AgentMessageRouted {
                    message: AgentMessage {
                        from: AgentId::new("agent-alpha"),
                        to: AgentId::new("agent-beta"),
                        body: "hello nebula".to_string(),
                        correlation_id: Some("corr-search".to_string()),
                    },
                },
            )
            .unwrap();

        let sessions = store
            .search_runtime(&RuntimeSearchFilter {
                query: "tools".to_string(),
                row_type: Some(RuntimeSearchRowType::Session),
                session_id: Some(SessionId::new("session-alpha")),
                agent_id: None,
                instance_id: None,
                task_id: None,
                event_kind: None,
                recorded_after: None,
                recorded_before: None,
                limit: Some(10),
            })
            .unwrap();
        let messages = store
            .search_runtime(&RuntimeSearchFilter {
                query: "nebula".to_string(),
                row_type: Some(RuntimeSearchRowType::Message),
                session_id: None,
                agent_id: Some(AgentId::new("agent-beta")),
                instance_id: None,
                task_id: None,
                event_kind: Some(CoreEventKind::AgentMessageRouted),
                recorded_after: None,
                recorded_before: None,
                limit: Some(10),
            })
            .unwrap();

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].row_type, RuntimeSearchRowType::Session);
        assert_eq!(
            sessions[0].session_id,
            Some(SessionId::new("session-alpha"))
        );
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].row_type, RuntimeSearchRowType::Message);
        assert_eq!(messages[0].agent_id, Some(AgentId::new("agent-beta")));
        assert_eq!(messages[0].sequence, Some(1));
        assert!(store
            .search_runtime(&RuntimeSearchFilter {
                query: "pi-crew".to_string(),
                row_type: None,
                session_id: None,
                agent_id: None,
                instance_id: None,
                task_id: None,
                event_kind: None,
                recorded_after: None,
                recorded_before: None,
                limit: Some(10),
            })
            .unwrap()
            .is_empty());

        remove_temp_db(&db_path);
    }

    #[test]
    fn runtime_counters_increment_by_scope_without_scanning_history() {
        let db_path = temp_db_path("runtime-counters");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        let session = sample_session_state();
        let delegated_session_id = SessionId::new("delegated-alpha");

        store
            .save_event(
                1,
                &CoreEvent::BrainWakeRequested {
                    session_id: session.session_id.clone(),
                },
            )
            .unwrap();
        store
            .save_event(
                2,
                &CoreEvent::BrainActionsAccepted {
                    session_id: session.session_id.clone(),
                    count: 2,
                },
            )
            .unwrap();
        store
            .save_event(
                3,
                &CoreEvent::BrainEventObserved {
                    session_id: session.session_id.clone(),
                    wake_id: Some("wake-tools".to_string()),
                    event: BrainEvent::ToolCallStarted {
                        tool_name: "read_file".to_string(),
                        metadata: None,
                    },
                },
            )
            .unwrap();
        store
            .save_event(
                4,
                &CoreEvent::BrainEventObserved {
                    session_id: session.session_id.clone(),
                    wake_id: Some("wake-tools".to_string()),
                    event: BrainEvent::ToolCallFinished {
                        tool_name: "read_file".to_string(),
                        is_error: true,
                        metadata: None,
                    },
                },
            )
            .unwrap();
        store
            .save_event(
                5,
                &CoreEvent::AgentMessageRouted {
                    message: AgentMessage {
                        from: AgentId::new("agent-alpha"),
                        to: AgentId::new("agent-beta"),
                        body: "counter message".to_string(),
                        correlation_id: None,
                    },
                },
            )
            .unwrap();
        store
            .save_event(
                6,
                &CoreEvent::DelegationLifecycleObserved {
                    lifecycle: rusty_crew_core_protocol::DelegationLifecycleEvent {
                        parent_session_id: session.session_id.clone(),
                        delegated_session_id: delegated_session_id.clone(),
                        run_id: Some(RunId::new("wake-tools:0")),
                        phase: rusty_crew_core_protocol::DelegationLifecyclePhase::Created,
                        detail: None,
                    },
                },
            )
            .unwrap();
        store
            .save_event(
                7,
                &CoreEvent::DelegationLifecycleObserved {
                    lifecycle: rusty_crew_core_protocol::DelegationLifecycleEvent {
                        parent_session_id: session.session_id.clone(),
                        delegated_session_id,
                        run_id: Some(RunId::new("wake-tools:0")),
                        phase: rusty_crew_core_protocol::DelegationLifecyclePhase::TimedOut,
                        detail: None,
                    },
                },
            )
            .unwrap();
        store
            .save_event(
                8,
                &CoreEvent::CompletionPacketDelivered {
                    packet: CompletionPacket {
                        session_id: session.session_id.clone(),
                        status: rusty_crew_core_protocol::CompletionStatus::Completed,
                        summary: "done".to_string(),
                    },
                },
            )
            .unwrap();

        // Re-saving the same sequence replaces projections but must not inflate counters.
        store
            .save_event(
                8,
                &CoreEvent::CompletionPacketDelivered {
                    packet: CompletionPacket {
                        session_id: session.session_id.clone(),
                        status: rusty_crew_core_protocol::CompletionStatus::Completed,
                        summary: "done again".to_string(),
                    },
                },
            )
            .unwrap();

        let runtime = store
            .runtime_summary(&RuntimeCounterScope::Runtime)
            .unwrap();
        let session_summary = store
            .runtime_summary(&RuntimeCounterScope::Session(SessionId::new(
                "session-alpha",
            )))
            .unwrap();
        let agent_summary = store
            .runtime_summary(&RuntimeCounterScope::Agent(AgentId::new("agent-beta")))
            .unwrap();

        assert_eq!(runtime.wakes, 1);
        assert_eq!(runtime.brain_turns, 1);
        assert_eq!(runtime.tool_calls, 1);
        assert_eq!(runtime.tool_errors, 1);
        assert_eq!(runtime.messages, 1);
        assert_eq!(runtime.delegations_created, 1);
        assert_eq!(runtime.delegations_timed_out, 1);
        assert_eq!(runtime.completions, 1);
        assert_eq!(session_summary.wakes, 1);
        assert_eq!(session_summary.completions, 1);
        assert_eq!(agent_summary.messages, 1);
        assert_eq!(store.count_rows("runtime_counters").unwrap(), 31);

        remove_temp_db(&db_path);
    }

    #[test]
    fn runtime_counter_reset_zeroes_selected_derived_rows() {
        let db_path = temp_db_path("runtime-counter-reset");
        let store = CoordinationStore::open_file(&db_path).unwrap();

        store
            .save_event(
                1,
                &CoreEvent::AgentMessageRouted {
                    message: AgentMessage {
                        from: AgentId::new("agent-alpha"),
                        to: AgentId::new("agent-beta"),
                        body: "reset this derived projection".to_string(),
                        correlation_id: None,
                    },
                },
            )
            .unwrap();

        let reset = store
            .reset_runtime_counters(
                &RuntimeCounterQuery {
                    scope: Some(RuntimeCounterScope::Runtime),
                    counter_name: Some(COUNTER_MESSAGES.to_string()),
                    page: None,
                },
                "2026-06-20T08:00:00Z".to_string(),
            )
            .unwrap();
        let runtime = store
            .runtime_summary(&RuntimeCounterScope::Runtime)
            .unwrap();
        let agent_beta = store
            .runtime_summary(&RuntimeCounterScope::Agent(AgentId::new("agent-beta")))
            .unwrap();

        assert_eq!(reset, 1);
        assert_eq!(runtime.messages, 0);
        assert_eq!(agent_beta.messages, 1);
        assert_eq!(
            store
                .query_runtime_counters(&RuntimeCounterQuery {
                    scope: Some(RuntimeCounterScope::Runtime),
                    counter_name: Some(COUNTER_MESSAGES.to_string()),
                    page: None,
                })
                .unwrap()[0]
                .updated_at,
            "2026-06-20T08:00:00Z"
        );

        remove_temp_db(&db_path);
    }

    #[test]
    fn queued_message_expiry_is_queryable_without_redelivery() {
        let db_path = temp_db_path("queued-messages");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        let record = QueuedMessageRecord {
            message_id: "queue-1".to_string(),
            owner_session_id: Some(SessionId::new("session-alpha")),
            owner_agent_id: AgentId::new("agent-alpha"),
            message: AgentMessage {
                from: AgentId::new("operator"),
                to: AgentId::new("agent-alpha"),
                body: "time boxed queue work".to_string(),
                correlation_id: Some("queue-corr".to_string()),
            },
            source_sequence: Some(42),
            enqueued_at: "2026-06-20T00:00:00Z".to_string(),
            expires_at: "2026-06-20T00:00:05Z".to_string(),
            ttl_ms: 5_000,
            delivery_attempts: 0,
            state: QueuedMessageState::Pending,
            terminal_at: None,
            state_reason: None,
        };

        store.save_queued_message(&record).unwrap();
        assert_eq!(
            store
                .load_queued_messages(&QueuedMessageFilter {
                    state: Some(QueuedMessageState::Pending),
                    owner_session_id: Some(SessionId::new("session-alpha")),
                    owner_agent_id: None,
                    limit: None,
                })
                .unwrap()
                .len(),
            1
        );
        assert!(store
            .expire_queued_messages_at(&"2026-06-20T00:00:04Z".to_string())
            .unwrap()
            .is_empty());

        let expired = store
            .expire_queued_messages_at(&"2026-06-20T00:00:06Z".to_string())
            .unwrap();

        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].state, QueuedMessageState::Expired);
        assert!(store
            .load_queued_messages(&QueuedMessageFilter {
                state: Some(QueuedMessageState::Pending),
                owner_session_id: Some(SessionId::new("session-alpha")),
                owner_agent_id: None,
                limit: None,
            })
            .unwrap()
            .is_empty());
        let expired_query = store
            .load_queued_messages(&QueuedMessageFilter {
                state: Some(QueuedMessageState::Expired),
                owner_session_id: None,
                owner_agent_id: Some(AgentId::new("agent-alpha")),
                limit: None,
            })
            .unwrap();
        assert_eq!(expired_query.len(), 1);
        assert_eq!(
            expired_query[0].state_reason.as_deref(),
            Some("ttl_expired")
        );
        assert_eq!(
            store
                .runtime_summary(&RuntimeCounterScope::Session(SessionId::new(
                    "session-alpha"
                )))
                .unwrap()
                .queue_expirations,
            1
        );
        let search = store
            .search_runtime(&RuntimeSearchFilter {
                query: "queue".to_string(),
                row_type: Some(RuntimeSearchRowType::QueueMessage),
                session_id: Some(SessionId::new("session-alpha")),
                agent_id: Some(AgentId::new("agent-alpha")),
                instance_id: None,
                task_id: None,
                event_kind: None,
                recorded_after: None,
                recorded_before: None,
                limit: Some(10),
            })
            .unwrap();
        assert_eq!(search.len(), 1);
        assert_eq!(search[0].row_type, RuntimeSearchRowType::QueueMessage);
        assert_eq!(store.count_rows("queued_messages").unwrap(), 1);

        remove_temp_db(&db_path);
    }

    #[test]
    fn runtime_state_query_apis_filter_and_page_without_raw_sql() {
        let db_path = temp_db_path("runtime-query-api");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        let alpha_config = sample_session_config();
        let alpha = sample_session_state();
        let beta_config = SessionConfig {
            session_id: SessionId::new("session-beta"),
            agent_id: AgentId::new("agent-beta"),
            profile_id: ProfileId::new("review-profile"),
            kind: SessionKind::Worker,
            delegation: None,
            resource_limits: sample_resource_limits(),
            tool_profile: sample_tool_profile(),
        };
        let beta = SessionState {
            handle: SessionHandle::new(2),
            session_id: beta_config.session_id.clone(),
            agent_id: beta_config.agent_id.clone(),
            profile_id: beta_config.profile_id.clone(),
            kind: beta_config.kind.clone(),
            delegation: None,
            resource_limits: beta_config.resource_limits.clone(),
            tool_profile: beta_config.tool_profile.clone(),
            status: SessionStatus::Idle,
            brain_turn_count: 0,
            created_at: "2026-06-20T00:01:00Z".to_string(),
            last_active_at: "2026-06-20T00:01:00Z".to_string(),
        };

        store
            .save_session_with_config(&alpha, &alpha_config)
            .unwrap();
        store.save_session_with_config(&beta, &beta_config).unwrap();
        store
            .save_worker_run_requested(&WorkerRunRecord {
                run_id: RunId::new("alpha-wake:0"),
                parent_session_id: alpha.session_id.clone(),
                delegated_session_id: Some(SessionId::new("delegated-alpha")),
                parent_agent_id: Some(alpha.agent_id.clone()),
                profile_id: ProfileId::new("coder-profile"),
                task_id: Some(TaskId::new("2876")),
                status: WorkerRunStatus::Requested,
                created_at: "2026-06-20T00:02:00Z".to_string(),
                last_updated_at: "2026-06-20T00:02:00Z".to_string(),
                source_wake_id: "alpha-wake".to_string(),
                source_action_index: 0,
                delegation_correlation_id: Some("query-run".to_string()),
                parent_consumption: ParentConsumptionPolicy::AwaitCompletion,
                fan_out_group_id: None,
                fan_out_max_concurrency: None,
                fan_out_failure_policy: FanOutFailurePolicy::FailSoft,
            })
            .unwrap();
        store
            .save_event(
                1,
                &CoreEvent::AgentMessageRouted {
                    message: AgentMessage {
                        from: alpha.agent_id.clone(),
                        to: beta.agent_id.clone(),
                        body: "first query message".to_string(),
                        correlation_id: Some("query-corr".to_string()),
                    },
                },
            )
            .unwrap();
        store
            .save_event(
                2,
                &CoreEvent::AgentMessageRouted {
                    message: AgentMessage {
                        from: beta.agent_id.clone(),
                        to: alpha.agent_id.clone(),
                        body: "second query message".to_string(),
                        correlation_id: Some("query-corr".to_string()),
                    },
                },
            )
            .unwrap();
        store
            .save_event(
                3,
                &CoreEvent::CompletionPacketDelivered {
                    packet: CompletionPacket {
                        session_id: alpha.session_id.clone(),
                        status: rusty_crew_core_protocol::CompletionStatus::Completed,
                        summary: "query completion".to_string(),
                    },
                },
            )
            .unwrap();
        store
            .save_event(
                4,
                &CoreEvent::BrainWakeRequested {
                    session_id: alpha.session_id.clone(),
                },
            )
            .unwrap();

        assert_eq!(
            store
                .query_sessions(&SessionQuery {
                    kind: Some(SessionKind::Full),
                    page: Some(QueryPage {
                        limit: Some(10),
                        offset: Some(0),
                    }),
                    ..SessionQuery::default()
                })
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            store
                .query_agent_instances(&AgentInstanceQuery {
                    agent_id: Some(AgentId::new("agent-beta")),
                    ..AgentInstanceQuery::default()
                })
                .unwrap()[0]
                .instance_id,
            AgentInstanceId::new("instance:session-beta")
        );
        assert_eq!(
            store
                .query_agent_messages(&AgentMessageQuery {
                    agent_id: Some(AgentId::new("agent-alpha")),
                    correlation_id: Some("query-corr".to_string()),
                    page: Some(QueryPage {
                        limit: Some(1),
                        offset: Some(1),
                    }),
                })
                .unwrap()[0]
                .sequence,
            2
        );
        assert_eq!(
            store
                .query_completion_packets(&CompletionPacketQuery {
                    session_id: Some(SessionId::new("session-alpha")),
                    status: Some(rusty_crew_core_protocol::CompletionStatus::Completed),
                    page: None,
                })
                .unwrap()[0]
                .packet
                .summary,
            "query completion"
        );
        assert_eq!(
            store
                .query_worker_runs(&WorkerRunQuery {
                    parent_session_id: Some(SessionId::new("session-alpha")),
                    terminal: Some(false),
                    ..WorkerRunQuery::default()
                })
                .unwrap()[0]
                .run_id,
            RunId::new("alpha-wake:0")
        );
        assert_eq!(
            store
                .query_runtime_counters(&RuntimeCounterQuery {
                    scope: Some(RuntimeCounterScope::Runtime),
                    counter_name: Some(COUNTER_MESSAGES.to_string()),
                    page: None,
                })
                .unwrap()[0]
                .value,
            2
        );

        remove_temp_db(&db_path);
    }

    #[test]
    fn maintenance_guardrails_cover_queue_retention_size_and_hot_indexes() {
        let db_path = temp_db_path("maintenance-guardrails");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        let mut sequence = 1_u64;
        for index in 0..30 {
            let session_id = SessionId::new(format!("session-{index:02}"));
            let agent_id = AgentId::new(format!("agent-{index:02}"));
            let profile_id = ProfileId::new(format!("profile-{}", index % 3));
            let config = SessionConfig {
                session_id: session_id.clone(),
                agent_id: agent_id.clone(),
                profile_id: profile_id.clone(),
                kind: SessionKind::Full,
                delegation: None,
                resource_limits: sample_resource_limits(),
                tool_profile: sample_tool_profile(),
            };
            store
                .save_session_with_config(
                    &SessionState {
                        handle: SessionHandle::new((index + 1) as u64),
                        session_id: session_id.clone(),
                        agent_id: agent_id.clone(),
                        profile_id,
                        kind: SessionKind::Full,
                        delegation: None,
                        resource_limits: sample_resource_limits(),
                        tool_profile: sample_tool_profile(),
                        status: SessionStatus::Idle,
                        brain_turn_count: 0,
                        created_at: format!("2026-06-20T00:{index:02}:00Z"),
                        last_active_at: format!("2026-06-20T00:{index:02}:00Z"),
                    },
                    &config,
                )
                .unwrap();
            store
                .save_worker_run_requested(&WorkerRunRecord {
                    run_id: RunId::new(format!("run-{index:02}")),
                    parent_session_id: session_id.clone(),
                    delegated_session_id: Some(SessionId::new(format!("delegated-{index:02}"))),
                    parent_agent_id: Some(agent_id.clone()),
                    profile_id: ProfileId::new("delegated-profile"),
                    task_id: Some(TaskId::new(format!("task-{index:02}"))),
                    status: WorkerRunStatus::Running,
                    created_at: format!("2026-06-20T01:{index:02}:00Z"),
                    last_updated_at: format!("2026-06-20T01:{index:02}:00Z"),
                    source_wake_id: format!("wake-{index:02}"),
                    source_action_index: index,
                    delegation_correlation_id: Some("scale-corr".to_string()),
                    parent_consumption: ParentConsumptionPolicy::AwaitCompletion,
                    fan_out_group_id: Some("scale-group".to_string()),
                    fan_out_max_concurrency: Some(4),
                    fan_out_failure_policy: FanOutFailurePolicy::FailSoft,
                })
                .unwrap();

            for message_index in 0..12 {
                store
                    .save_event(
                        sequence,
                        &CoreEvent::AgentMessageRouted {
                            message: AgentMessage {
                                from: agent_id.clone(),
                                to: AgentId::new(format!("agent-{:02}", (index + 1) % 30)),
                                body: format!("scale message {index}-{message_index}"),
                                correlation_id: Some("corr-alpha".to_string()),
                            },
                        },
                    )
                    .unwrap();
                sequence += 1;
            }
        }

        for index in 0..5 {
            store
                .save_queued_message(&QueuedMessageRecord {
                    message_id: format!("expired-queue-{index}"),
                    owner_session_id: Some(SessionId::new("session-00")),
                    owner_agent_id: AgentId::new("agent-00"),
                    message: AgentMessage {
                        from: AgentId::new("operator"),
                        to: AgentId::new("agent-00"),
                        body: format!("expired queue message {index}"),
                        correlation_id: Some("queue-scale".to_string()),
                    },
                    source_sequence: Some(sequence + index as u64),
                    enqueued_at: "2026-06-20T02:00:00Z".to_string(),
                    expires_at: "2026-06-20T02:00:01Z".to_string(),
                    ttl_ms: 1_000,
                    delivery_attempts: 0,
                    state: QueuedMessageState::Pending,
                    terminal_at: None,
                    state_reason: None,
                })
                .unwrap();
        }
        store
            .save_queued_message(&QueuedMessageRecord {
                message_id: "future-queue".to_string(),
                owner_session_id: Some(SessionId::new("session-00")),
                owner_agent_id: AgentId::new("agent-00"),
                message: AgentMessage {
                    from: AgentId::new("operator"),
                    to: AgentId::new("agent-00"),
                    body: "fresh queue message".to_string(),
                    correlation_id: Some("queue-scale".to_string()),
                },
                source_sequence: Some(sequence + 10),
                enqueued_at: "2026-06-20T02:00:00Z".to_string(),
                expires_at: "2026-06-20T02:10:00Z".to_string(),
                ttl_ms: 600_000,
                delivery_attempts: 0,
                state: QueuedMessageState::Pending,
                terminal_at: None,
                state_reason: None,
            })
            .unwrap();

        let report = store
            .run_maintenance(&RuntimeMaintenancePolicy {
                expire_queued_messages_at: Some("2026-06-20T02:00:02Z".to_string()),
                purge_terminal_queued_messages_before: Some("2026-06-20T02:00:03Z".to_string()),
                run_wal_checkpoint: true,
                run_optimize: true,
            })
            .unwrap();

        assert_eq!(report.expired_queue_messages, 5);
        assert_eq!(report.purged_terminal_queue_messages, 5);
        assert!(report.optimize_ran);
        assert!(report.wal_checkpoint_ran);
        assert!(report.size_before.page_size_bytes > 0);
        assert!(report.size_after.database_bytes > 0);
        assert_eq!(store.count_rows("queued_messages").unwrap(), 1);
        assert_eq!(
            store
                .load_queued_messages(&QueuedMessageFilter {
                    state: Some(QueuedMessageState::Pending),
                    owner_session_id: None,
                    owner_agent_id: Some(AgentId::new("agent-00")),
                    limit: None,
                })
                .unwrap()[0]
                .message_id,
            "future-queue"
        );
        assert_eq!(
            store
                .search_runtime(&RuntimeSearchFilter {
                    query: "expired queue message".to_string(),
                    row_type: Some(RuntimeSearchRowType::QueueMessage),
                    session_id: Some(SessionId::new("session-00")),
                    agent_id: Some(AgentId::new("agent-00")),
                    instance_id: None,
                    task_id: None,
                    event_kind: None,
                    recorded_after: None,
                    recorded_before: None,
                    limit: Some(10),
                })
                .unwrap()
                .len(),
            0
        );
        let checks = store.hot_query_plan_checks().unwrap();
        assert!(
            checks.iter().all(|check| check.uses_index),
            "hot query plan lost index coverage: {checks:?}"
        );

        remove_temp_db(&db_path);
    }

    #[test]
    fn sqlite_and_sql_literals_do_not_leak_outside_persistence_crate() {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = find_workspace_root(manifest_dir);
        let mut violations = Vec::new();
        scan_source_tree(workspace_root, workspace_root, &mut violations);

        assert!(
            violations.is_empty(),
            "persistence backend leaked outside core-persistence:\n{}",
            violations.join("\n")
        );
    }

    fn find_workspace_root(start: &Path) -> &Path {
        start
            .ancestors()
            .find(|candidate| {
                fs::read_to_string(candidate.join("Cargo.toml"))
                    .is_ok_and(|content| content.lines().any(|line| line.trim() == "[workspace]"))
            })
            .expect("workspace Cargo.toml")
    }

    fn scan_source_tree(workspace_root: &Path, root: &Path, violations: &mut Vec<String>) {
        for entry in fs::read_dir(root).expect("scan root") {
            let entry = entry.expect("read dir entry");
            let path = entry.path();
            let file_name = entry.file_name();
            let file_name = file_name.to_string_lossy();
            if file_name == "target" || file_name == "node_modules" || file_name == ".git" {
                continue;
            }
            if path.is_dir() {
                scan_source_tree(workspace_root, &path, violations);
                continue;
            }
            if !matches!(
                path.extension().and_then(|extension| extension.to_str()),
                Some("rs" | "ts")
            ) {
                continue;
            }
            if path.starts_with(workspace_root.join("crates/core/core-persistence")) {
                continue;
            }
            let content = fs::read_to_string(&path).expect("read source file");
            if contains_persistence_backend_detail(&content) {
                violations.push(
                    path.strip_prefix(workspace_root)
                        .unwrap_or(&path)
                        .display()
                        .to_string(),
                );
            }
        }
    }

    fn contains_persistence_backend_detail(content: &str) -> bool {
        const NEEDLES: &[&str] = &[
            "rusqlite",
            "CREATE TABLE",
            "ALTER TABLE",
            "PRAGMA ",
            "SELECT ",
            "INSERT ",
            "UPDATE ",
            "DELETE ",
        ];
        NEEDLES.iter().any(|needle| content.contains(needle))
    }

    fn temp_db_path(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "rusty-crew-{label}-{}-{nanos}.sqlite3",
            std::process::id()
        ))
    }

    fn table_has_column(db_path: &Path, table: &str, column: &str) -> bool {
        let conn = Connection::open(db_path).unwrap();
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .unwrap();
        let columns = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        columns.iter().any(|existing| existing == column)
    }

    fn table_exists(db_path: &Path, table: &str) -> bool {
        let conn = Connection::open(db_path).unwrap();
        conn.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = ?1
            )",
            params![table],
            |row| row.get::<_, i64>(0),
        )
        .unwrap()
            != 0
    }

    fn index_exists(db_path: &Path, index: &str) -> bool {
        let conn = Connection::open(db_path).unwrap();
        conn.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM sqlite_schema WHERE type = 'index' AND name = ?1
            )",
            params![index],
            |row| row.get::<_, i64>(0),
        )
        .unwrap()
            != 0
    }

    fn remove_temp_db(db_path: &Path) {
        let _ = fs::remove_file(db_path);
        let _ = fs::remove_file(format!("{}-wal", db_path.display()));
        let _ = fs::remove_file(format!("{}-shm", db_path.display()));
    }

    fn sample_session_state() -> SessionState {
        SessionState {
            handle: SessionHandle::new(1),
            session_id: SessionId::new("session-alpha"),
            agent_id: AgentId::new("agent-alpha"),
            profile_id: ProfileId::new("full-profile"),
            kind: SessionKind::Full,
            delegation: None,
            resource_limits: sample_resource_limits(),
            tool_profile: sample_tool_profile(),
            status: SessionStatus::Idle,
            brain_turn_count: 0,
            created_at: "2026-06-20T00:00:00Z".to_string(),
            last_active_at: "2026-06-20T00:00:00Z".to_string(),
        }
    }

    fn replaced_write(
        profile_id: &str,
        target: ProfileMemoryTarget,
        key: &str,
    ) -> ProfileMemoryWrite {
        ProfileMemoryWrite {
            profile_id: ProfileId::new(profile_id),
            target,
            key: key.to_string(),
            content: "stale write should be rejected".to_string(),
            metadata: serde_json::json!({}),
            now: "2026-06-20T05:02:00Z".to_string(),
        }
    }

    fn sample_session_config() -> SessionConfig {
        SessionConfig {
            session_id: SessionId::new("session-alpha"),
            agent_id: AgentId::new("agent-alpha"),
            profile_id: ProfileId::new("full-profile"),
            kind: SessionKind::Full,
            delegation: None,
            resource_limits: sample_resource_limits(),
            tool_profile: sample_tool_profile(),
        }
    }

    fn sample_resource_limits() -> ResourceLimits {
        ResourceLimits {
            workdir: Some("/tmp/rusty-crew-test".to_string()),
            max_duration_ms: Some(60_000),
            max_delegation_depth: Some(4),
        }
    }

    fn sample_tool_profile() -> ToolProfile {
        ToolProfile {
            tools: vec![ToolDescriptor {
                name: "apply_patch".to_string(),
                description: "Apply a source patch".to_string(),
                input_schema: None,
            }],
        }
    }
}
