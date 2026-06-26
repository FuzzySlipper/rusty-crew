//! Local coordination-state persistence.
//!
//! This store is for Rust-owned coordination state only. Den task, project, and
//! document data remains Den product data and is not mirrored here.

pub mod module_schema;
#[cfg(feature = "postgres-proof")]
pub mod postgres_proof;
mod repositories;

pub use crate::module_schema::{
    RuntimeInstalledModuleSchemaDiagnostic, RuntimeModuleCapabilityStatus,
    RuntimeModuleLogicalStoreDiagnostic, RuntimeModuleNamedDiagnostic,
    RuntimeModulePhysicalIndexDiagnostic, RuntimeModulePhysicalTableDiagnostic,
    RuntimeModuleQueryCatalogDiagnostic, RuntimeModuleRetentionDiagnostic,
    RuntimeModuleSchemaDiagnostic, RuntimeModuleSchemaRegistryDiagnostics,
    RuntimeModuleTransferHookDiagnostic,
};
pub use crate::repositories::{
    RuntimeRepositoryBackendRequirement, RuntimeRepositoryGroupDiagnostic,
};

use crate::module_schema::{
    compiled_module_schema_registry, module_schema_registry_diagnostics,
    validate_version_progression, InstalledModuleSchemaRecord, ModuleId, ModuleSchemaBundle,
    ModuleSchemaCapability, ModuleSchemaRegistry,
};
use rusqlite::{params, Connection, OptionalExtension};
use rusty_crew_core_protocol::{
    AdapterId, AgentId, AgentInstanceId, AgentInstanceRecord, AgentMessage, AttachmentId,
    AttachmentLinkId, BrainEvent, CompletionPacket, ConversationBranchId, ConversationSnapshotId,
    CoreError, CoreErrorKind, CoreEvent, CoreEventKind, CoreResult, DataBankScopeId,
    DelegatedCompletion, DelegatedFanOutGroup, DelegationLineage, DenRuntimeReference,
    DurableAgentKind, DurableAgentRecord, DurableIdentityStatus, FanOutFailurePolicy,
    FanOutGroupStatus, IsoTimestamp, MemoryGovernanceDecisionInput, MemoryGovernanceDecisionKind,
    MemoryGovernanceDecisionRecord, MemoryGovernanceMode, MemoryOperation, MemoryProposalEnvelope,
    MemoryProposalQuery, MemoryProposalRecord, MemoryProposalReviewStatus, MemoryProposalSource,
    MemoryScopeType, MemorySpaceDescriptor, MessageBlockId, MessageId, MessageSlotId,
    MessageVariantId, ParentConsumptionPolicy, ProfileId, ProfileRegistryLifecycleStatus,
    ProfileRegistryLifecycleUpdate, ProfileRegistryRecord, ProfileRegistryWrite, ProjectId,
    ProviderStateAbsenceReason, ResourceLimits, RunId, SessionConfig, SessionHandle,
    SessionHistoryWindow, SessionId, SessionIdentityRecord, SessionKind, SessionState,
    SessionStatus, SourceSystemReference, TaskId, ToolCallMetadata, ToolProfile,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const DB_FILE_NAME: &str = "coordination.sqlite3";
const CURRENT_SCHEMA_VERSION: i64 = 22;
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
    SchemaMigration {
        version: 15,
        description: "add session history window persistence",
        apply: migrate_v15_add_session_history_window,
    },
    SchemaMigration {
        version: 16,
        description: "add provider wire-state persistence",
        apply: migrate_v16_add_provider_wire_state,
    },
    SchemaMigration {
        version: 17,
        description: "add message slot and variant persistence",
        apply: migrate_v17_add_message_slot_variants,
    },
    SchemaMigration {
        version: 18,
        description: "add conversation tree branches and snapshots",
        apply: migrate_v18_add_conversation_tree,
    },
    SchemaMigration {
        version: 19,
        description: "add generic chat attachments and data-bank scopes",
        apply: migrate_v19_add_chat_attachments,
    },
    SchemaMigration {
        version: 20,
        description: "add module schema installed-version registry",
        apply: migrate_v20_add_module_schema_registry,
    },
    SchemaMigration {
        version: 21,
        description: "add typed memory proposal governance storage",
        apply: migrate_v21_add_memory_proposal_governance,
    },
    SchemaMigration {
        version: 22,
        description: "add DB-backed active profile registry",
        apply: migrate_v22_add_profile_registry,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
pub struct ProfileRegistryQuery {
    pub lifecycle_status: Option<ProfileRegistryLifecycleStatus>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageVariantSource {
    Primary,
    Alternate,
}

impl MessageVariantSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Alternate => "alternate",
        }
    }

    fn parse(raw: &str) -> CoreResult<Self> {
        match raw {
            "primary" => Ok(Self::Primary),
            "alternate" => Ok(Self::Alternate),
            _ => Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("unsupported message variant source {raw}"),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageVariantStatus {
    Active,
    Deleted,
}

impl MessageVariantStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Deleted => "deleted",
        }
    }

    fn parse(raw: &str) -> CoreResult<Self> {
        match raw {
            "active" => Ok(Self::Active),
            "deleted" => Ok(Self::Deleted),
            _ => Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("unsupported message variant status {raw}"),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DurableMessageStatus {
    Created,
    Streaming,
    Completed,
    Failed,
    Deleted,
}

impl DurableMessageStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Streaming => "streaming",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Deleted => "deleted",
        }
    }

    fn parse(raw: &str) -> CoreResult<Self> {
        match raw {
            "created" => Ok(Self::Created),
            "streaming" => Ok(Self::Streaming),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "deleted" => Ok(Self::Deleted),
            _ => Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("unsupported durable message status {raw}"),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageBlockRecord {
    pub block_id: MessageBlockId,
    pub message_id: MessageId,
    pub ordinal: u32,
    pub kind: String,
    pub content_json: JsonValue,
    pub render_policy_json: Option<JsonValue>,
    pub metadata_json: JsonValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DurableMessageRecord {
    pub message_id: MessageId,
    pub session_id: SessionId,
    pub branch_id: Option<ConversationBranchId>,
    pub parent_message_id: Option<MessageId>,
    pub previous_message_id: Option<MessageId>,
    pub author_id: String,
    pub author_role: String,
    pub status: DurableMessageStatus,
    pub body: String,
    pub metadata_json: JsonValue,
    pub created_at: IsoTimestamp,
    pub blocks: Vec<MessageBlockRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageVariantRecord {
    pub variant_id: MessageVariantId,
    pub slot_id: MessageSlotId,
    pub source: MessageVariantSource,
    pub ordinal: u32,
    pub status: MessageVariantStatus,
    pub message: DurableMessageRecord,
    pub metadata_json: JsonValue,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageSlotRecord {
    pub slot_id: MessageSlotId,
    pub session_id: SessionId,
    pub primary_variant_id: MessageVariantId,
    pub active_variant_id: Option<MessageVariantId>,
    pub metadata_json: JsonValue,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
    pub version: u64,
    pub primary: MessageVariantRecord,
    pub alternates: Vec<MessageVariantRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageBlockWrite {
    pub block_id: MessageBlockId,
    pub ordinal: u32,
    pub kind: String,
    pub content_json: JsonValue,
    pub render_policy_json: Option<JsonValue>,
    pub metadata_json: JsonValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DurableMessageWrite {
    pub message_id: MessageId,
    pub session_id: SessionId,
    pub branch_id: Option<ConversationBranchId>,
    pub parent_message_id: Option<MessageId>,
    pub previous_message_id: Option<MessageId>,
    pub author_id: String,
    pub author_role: String,
    pub status: DurableMessageStatus,
    pub body: String,
    pub metadata_json: JsonValue,
    pub created_at: IsoTimestamp,
    pub blocks: Vec<MessageBlockWrite>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageVariantWrite {
    pub variant_id: MessageVariantId,
    pub slot_id: MessageSlotId,
    pub source: MessageVariantSource,
    pub ordinal: u32,
    pub status: MessageVariantStatus,
    pub message: DurableMessageWrite,
    pub metadata_json: JsonValue,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageSlotWrite {
    pub slot_id: MessageSlotId,
    pub session_id: SessionId,
    pub primary_variant_id: MessageVariantId,
    pub active_variant_id: Option<MessageVariantId>,
    pub metadata_json: JsonValue,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MessageSlotQuery {
    pub session_id: Option<SessionId>,
    pub include_alternates: bool,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MessageVariantQuery {
    pub slot_id: Option<MessageSlotId>,
    pub include_deleted: bool,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "variant_id", rename_all = "snake_case")]
pub enum ActiveVariantExpectation {
    Any,
    Primary,
    Variant(MessageVariantId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectActiveVariantRequest {
    pub slot_id: MessageSlotId,
    pub active_variant_id: Option<MessageVariantId>,
    pub expected: ActiveVariantExpectation,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectActiveVariantResult {
    pub slot: MessageSlotRecord,
    pub conflict: Option<ActiveVariantConflict>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveVariantConflict {
    pub expected: Option<MessageVariantId>,
    pub actual: Option<MessageVariantId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationBranchRecord {
    pub branch_id: ConversationBranchId,
    pub session_id: SessionId,
    pub parent_branch_id: Option<ConversationBranchId>,
    pub parent_message_id: Option<MessageId>,
    pub origin_message_id: Option<MessageId>,
    pub head_message_id: Option<MessageId>,
    pub label: Option<String>,
    pub metadata_json: JsonValue,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
    pub version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationBranchWrite {
    pub branch_id: ConversationBranchId,
    pub session_id: SessionId,
    pub parent_branch_id: Option<ConversationBranchId>,
    pub parent_message_id: Option<MessageId>,
    pub origin_message_id: Option<MessageId>,
    pub head_message_id: Option<MessageId>,
    pub label: Option<String>,
    pub metadata_json: JsonValue,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ConversationBranchQuery {
    pub session_id: Option<SessionId>,
    pub parent_branch_id: Option<ConversationBranchId>,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationBranchStateRecord {
    pub session_id: SessionId,
    pub active_branch_id: Option<ConversationBranchId>,
    pub updated_at: IsoTimestamp,
    pub version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "branch_id", rename_all = "snake_case")]
pub enum ActiveBranchExpectation {
    Any,
    None,
    Branch(ConversationBranchId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectActiveBranchRequest {
    pub session_id: SessionId,
    pub active_branch_id: Option<ConversationBranchId>,
    pub expected: ActiveBranchExpectation,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectActiveBranchResult {
    pub state: ConversationBranchStateRecord,
    pub conflict: Option<ActiveBranchConflict>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveBranchConflict {
    pub expected: Option<ConversationBranchId>,
    pub actual: Option<ConversationBranchId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "message_id", rename_all = "snake_case")]
pub enum BranchHeadExpectation {
    Any,
    None,
    Message(MessageId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateBranchHeadRequest {
    pub branch_id: ConversationBranchId,
    pub head_message_id: Option<MessageId>,
    pub expected: BranchHeadExpectation,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateBranchHeadResult {
    pub branch: ConversationBranchRecord,
    pub conflict: Option<BranchHeadConflict>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchHeadConflict {
    pub expected: Option<MessageId>,
    pub actual: Option<MessageId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationSnapshotSource {
    User,
    System,
    Import,
}

impl ConversationSnapshotSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::System => "system",
            Self::Import => "import",
        }
    }

    fn parse(raw: &str) -> CoreResult<Self> {
        match raw {
            "user" => Ok(Self::User),
            "system" => Ok(Self::System),
            "import" => Ok(Self::Import),
            _ => Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("unsupported conversation snapshot source {raw}"),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationSnapshotRecord {
    pub snapshot_id: ConversationSnapshotId,
    pub session_id: SessionId,
    pub branch_id: Option<ConversationBranchId>,
    pub message_id: Option<MessageId>,
    pub cursor: Option<String>,
    pub label: Option<String>,
    pub summary: Option<String>,
    pub source: ConversationSnapshotSource,
    pub metadata_json: JsonValue,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationSnapshotWrite {
    pub snapshot_id: ConversationSnapshotId,
    pub session_id: SessionId,
    pub branch_id: Option<ConversationBranchId>,
    pub message_id: Option<MessageId>,
    pub cursor: Option<String>,
    pub label: Option<String>,
    pub summary: Option<String>,
    pub source: ConversationSnapshotSource,
    pub metadata_json: JsonValue,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ConversationSnapshotQuery {
    pub session_id: Option<SessionId>,
    pub branch_id: Option<ConversationBranchId>,
    pub message_id: Option<MessageId>,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConversationJumpTarget {
    Message { message_id: MessageId },
    Branch { branch_id: ConversationBranchId },
    Snapshot { snapshot_id: ConversationSnapshotId },
    Cursor { cursor: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationJumpRequest {
    pub session_id: SessionId,
    pub target: ConversationJumpTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationJumpResult {
    pub session_id: SessionId,
    pub target: ConversationJumpTarget,
    pub branch_id: Option<ConversationBranchId>,
    pub message_id: Option<MessageId>,
    pub cursor: Option<String>,
    pub snapshot_id: Option<ConversationSnapshotId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentStatus {
    Active,
    Removed,
}

impl AttachmentStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Removed => "removed",
        }
    }

    fn parse(raw: &str) -> CoreResult<Self> {
        match raw {
            "active" => Ok(Self::Active),
            "removed" => Ok(Self::Removed),
            _ => Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("unsupported attachment status {raw}"),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentLinkRecord {
    pub link_id: AttachmentLinkId,
    pub attachment_id: AttachmentId,
    pub session_id: SessionId,
    pub message_id: Option<MessageId>,
    pub block_id: Option<MessageBlockId>,
    pub scope_id: Option<DataBankScopeId>,
    pub metadata_json: JsonValue,
    pub created_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentRecord {
    pub attachment_id: AttachmentId,
    pub session_id: SessionId,
    pub status: AttachmentStatus,
    pub filename: String,
    pub mime_type: String,
    pub byte_size: u64,
    pub storage_url: Option<String>,
    pub download_url: Option<String>,
    pub thumbnail_url: Option<String>,
    pub extracted_text: Option<String>,
    pub extracted_text_truncated: bool,
    pub metadata_json: JsonValue,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
    pub expires_at: Option<IsoTimestamp>,
    pub links: Vec<AttachmentLinkRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentWrite {
    pub attachment_id: AttachmentId,
    pub session_id: SessionId,
    pub status: AttachmentStatus,
    pub filename: String,
    pub mime_type: String,
    pub byte_size: u64,
    pub storage_url: Option<String>,
    pub download_url: Option<String>,
    pub thumbnail_url: Option<String>,
    pub extracted_text: Option<String>,
    pub extracted_text_truncated: bool,
    pub metadata_json: JsonValue,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
    pub expires_at: Option<IsoTimestamp>,
    pub link: Option<AttachmentLinkWrite>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentLinkWrite {
    pub link_id: AttachmentLinkId,
    pub attachment_id: AttachmentId,
    pub session_id: SessionId,
    pub message_id: Option<MessageId>,
    pub block_id: Option<MessageBlockId>,
    pub scope_id: Option<DataBankScopeId>,
    pub metadata_json: JsonValue,
    pub created_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AttachmentQuery {
    pub session_id: Option<SessionId>,
    pub message_id: Option<MessageId>,
    pub scope_id: Option<DataBankScopeId>,
    pub include_removed: bool,
    pub page: Option<QueryPage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataBankScopeStatus {
    Active,
    Removed,
}

impl DataBankScopeStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Removed => "removed",
        }
    }

    fn parse(raw: &str) -> CoreResult<Self> {
        match raw {
            "active" => Ok(Self::Active),
            "removed" => Ok(Self::Removed),
            _ => Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("unsupported data-bank scope status {raw}"),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataBankScopeRecord {
    pub scope_id: DataBankScopeId,
    pub session_id: SessionId,
    pub status: DataBankScopeStatus,
    pub label: Option<String>,
    pub description: Option<String>,
    pub metadata_json: JsonValue,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataBankScopeWrite {
    pub scope_id: DataBankScopeId,
    pub session_id: SessionId,
    pub status: DataBankScopeStatus,
    pub label: Option<String>,
    pub description: Option<String>,
    pub metadata_json: JsonValue,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DataBankScopeQuery {
    pub session_id: Option<SessionId>,
    pub include_removed: bool,
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
pub struct SimpleKvScope {
    pub scope_type: String,
    pub scope_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleKvRecord {
    pub scope: SimpleKvScope,
    pub key: String,
    pub value_json: JsonValue,
    pub revision: u64,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
    pub expires_at: Option<IsoTimestamp>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleKvWrite {
    pub scope: SimpleKvScope,
    pub key: String,
    pub value_json: JsonValue,
    pub now: IsoTimestamp,
    pub expires_at: Option<IsoTimestamp>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleKvCompareAndSwap {
    pub write: SimpleKvWrite,
    pub expected_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleKvDelete {
    pub scope: SimpleKvScope,
    pub key: String,
    pub expected_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleKvQuery {
    pub scope: SimpleKvScope,
    pub key_prefix: Option<String>,
    pub include_expired: bool,
    pub expired_only: bool,
    pub now: Option<IsoTimestamp>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderWireStateInvalidationReason {
    ProfileChanged,
    ProviderChanged,
    ModuleChanged,
    StrategyChanged,
    Expired,
    BrainRequestedClear,
    OperatorRequestedClear,
    Superseded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderWireStateKey {
    pub session_id: SessionId,
    pub module_id: String,
    pub strategy_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderWireStateRecord {
    pub row_id: i64,
    pub key: ProviderWireStateKey,
    pub profile_fingerprint: String,
    pub provider_fingerprint: String,
    pub payload_version: String,
    pub payload_json: JsonValue,
    pub payload_encoding: String,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
    pub expires_at: Option<IsoTimestamp>,
    pub last_wake_id: Option<String>,
    pub invalidated_at: Option<IsoTimestamp>,
    pub invalidation_reason: Option<ProviderWireStateInvalidationReason>,
}

impl ProviderWireStateRecord {
    pub fn is_current(&self) -> bool {
        self.invalidated_at.is_none() && self.invalidation_reason.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderWireStateDiagnostic {
    pub key: ProviderWireStateKey,
    pub payload_version: String,
    pub payload_bytes: u64,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
    pub expires_at: Option<IsoTimestamp>,
    pub last_wake_id: Option<String>,
    pub invalidated_at: Option<IsoTimestamp>,
    pub invalidation_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderWireStateWrite {
    pub key: ProviderWireStateKey,
    pub profile_fingerprint: String,
    pub provider_fingerprint: String,
    pub payload_version: String,
    pub payload_json: JsonValue,
    pub now: IsoTimestamp,
    pub expires_at: Option<IsoTimestamp>,
    pub last_wake_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderWireStateWakeLookup {
    pub key: ProviderWireStateKey,
    pub profile_fingerprint: String,
    pub provider_fingerprint: String,
    pub now: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderWireStateWakeResult {
    pub record: Option<ProviderWireStateRecord>,
    pub absence_reason: Option<ProviderStateAbsenceReason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RuntimeMaintenancePolicy {
    pub expire_queued_messages_at: Option<IsoTimestamp>,
    pub purge_terminal_queued_messages_before: Option<IsoTimestamp>,
    pub expire_provider_wire_states_at: Option<IsoTimestamp>,
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
pub struct RuntimeStorageCapability {
    pub name: String,
    pub supported: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeStorageTableCount {
    pub table: String,
    pub rows: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeStorageDiagnostics {
    pub backend: String,
    pub backend_label: String,
    pub schema_version: i64,
    pub supported_schema_version: i64,
    pub migrations: Vec<SchemaMigrationRecord>,
    pub size: RuntimeDatabaseSize,
    pub table_counts: Vec<RuntimeStorageTableCount>,
    pub capabilities: Vec<RuntimeStorageCapability>,
    pub repository_groups: Vec<RuntimeRepositoryGroupDiagnostic>,
    pub module_registry: RuntimeModuleSchemaRegistryDiagnostics,
    pub index_checks: Vec<RuntimeQueryPlanCheck>,
    pub search_healthy: bool,
    pub pressure: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeMaintenanceReport {
    pub size_before: RuntimeDatabaseSize,
    pub size_after: RuntimeDatabaseSize,
    pub expired_queue_messages: u64,
    pub purged_terminal_queue_messages: u64,
    pub expired_provider_wire_states: u64,
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
    ProfileRegistry,
    ProfileMemories,
    MemoryProposals,
    MemoryGovernanceDecisions,
    ScheduledJobs,
    ScheduledJobRuns,
    ProviderWireStates,
    MessageSlots,
    MessageVariants,
    Messages,
    MessageBlocks,
    ConversationBranches,
    ConversationBranchState,
    ConversationSnapshots,
    Attachments,
    AttachmentLinks,
    DataBankScopes,
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
        Self::ProfileRegistry,
        Self::ProfileMemories,
        Self::MemoryProposals,
        Self::MemoryGovernanceDecisions,
        Self::ScheduledJobs,
        Self::ScheduledJobRuns,
        Self::ProviderWireStates,
        Self::MessageSlots,
        Self::MessageVariants,
        Self::Messages,
        Self::MessageBlocks,
        Self::ConversationBranches,
        Self::ConversationBranchState,
        Self::ConversationSnapshots,
        Self::Attachments,
        Self::AttachmentLinks,
        Self::DataBankScopes,
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
            "profile_registry" => Ok(Self::ProfileRegistry),
            "profile_memories" => Ok(Self::ProfileMemories),
            "memory_proposals" => Ok(Self::MemoryProposals),
            "memory_governance_decisions" => Ok(Self::MemoryGovernanceDecisions),
            "scheduled_jobs" => Ok(Self::ScheduledJobs),
            "scheduled_job_runs" => Ok(Self::ScheduledJobRuns),
            "provider_wire_states" => Ok(Self::ProviderWireStates),
            "message_slots" => Ok(Self::MessageSlots),
            "message_variants" => Ok(Self::MessageVariants),
            "messages" => Ok(Self::Messages),
            "message_blocks" => Ok(Self::MessageBlocks),
            "conversation_branches" => Ok(Self::ConversationBranches),
            "conversation_branch_state" => Ok(Self::ConversationBranchState),
            "conversation_snapshots" => Ok(Self::ConversationSnapshots),
            "attachments" => Ok(Self::Attachments),
            "attachment_links" => Ok(Self::AttachmentLinks),
            "data_bank_scopes" => Ok(Self::DataBankScopes),
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
            Self::ProfileRegistry => "profile_registry",
            Self::ProfileMemories => "profile_memories",
            Self::MemoryProposals => "memory_proposals",
            Self::MemoryGovernanceDecisions => "memory_governance_decisions",
            Self::ScheduledJobs => "scheduled_jobs",
            Self::ScheduledJobRuns => "scheduled_job_runs",
            Self::ProviderWireStates => "provider_wire_states",
            Self::MessageSlots => "message_slots",
            Self::MessageVariants => "message_variants",
            Self::Messages => "messages",
            Self::MessageBlocks => "message_blocks",
            Self::ConversationBranches => "conversation_branches",
            Self::ConversationBranchState => "conversation_branch_state",
            Self::ConversationSnapshots => "conversation_snapshots",
            Self::Attachments => "attachments",
            Self::AttachmentLinks => "attachment_links",
            Self::DataBankScopes => "data_bank_scopes",
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

    pub fn create_profile_registry_record(
        &self,
        write: &ProfileRegistryWrite,
    ) -> CoreResult<ProfileRegistryRecord> {
        validate_profile_registry_write(write)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start create profile registry record", error))?;
        if get_profile_registry_record(&tx, &write.profile_id)?.is_some() {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!(
                    "profile registry record {} already exists",
                    write.profile_id
                ),
            ));
        }
        insert_profile_registry_record_in_tx(&tx, write)?;
        let record = get_profile_registry_record(&tx, &write.profile_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "created profile registry record was not readable",
            )
        })?;
        tx.commit()
            .map_err(|error| persistence_error("commit create profile registry record", error))?;
        Ok(record)
    }

    pub fn get_profile_registry_record(
        &self,
        profile_id: &ProfileId,
    ) -> CoreResult<Option<ProfileRegistryRecord>> {
        validate_profile_registry_id(profile_id)?;
        let conn = self.conn()?;
        get_profile_registry_record(&conn, profile_id)
    }

    pub fn list_profile_registry_records(
        &self,
        query: &ProfileRegistryQuery,
    ) -> CoreResult<Vec<ProfileRegistryRecord>> {
        let conn = self.conn()?;
        query_profile_registry_records(&conn, query)
    }

    pub fn update_profile_registry_lifecycle(
        &self,
        update: &ProfileRegistryLifecycleUpdate,
    ) -> CoreResult<ProfileRegistryRecord> {
        validate_profile_registry_id(&update.profile_id)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start update profile registry lifecycle", error))?;
        let existing = get_profile_registry_record(&tx, &update.profile_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("profile registry record {} not found", update.profile_id),
            )
        })?;
        if existing.revision != update.expected_revision {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "profile registry revision mismatch for {}: expected {}, found {}",
                    update.profile_id, update.expected_revision, existing.revision
                ),
            ));
        }
        update_profile_registry_lifecycle_in_tx(&tx, update, existing.revision + 1)?;
        let record = get_profile_registry_record(&tx, &update.profile_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "updated profile registry record was not readable",
            )
        })?;
        tx.commit().map_err(|error| {
            persistence_error("commit update profile registry lifecycle", error)
        })?;
        Ok(record)
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

    pub fn save_memory_proposal(
        &self,
        proposal: &MemoryProposalEnvelope,
        descriptor: &MemorySpaceDescriptor,
        now: &IsoTimestamp,
    ) -> CoreResult<MemoryProposalRecord> {
        validate_memory_proposal(proposal, descriptor)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start save memory proposal", error))?;
        if let Some(dedupe_key) = proposal
            .dedupe_key
            .as_ref()
            .filter(|value| !value.trim().is_empty())
        {
            if let Some(existing) =
                get_memory_proposal_by_dedupe(&tx, &proposal.space_id.0, dedupe_key)?
            {
                return Ok(existing);
            }
        }
        if get_memory_proposal_by_id(&tx, &proposal.proposal_id)?.is_some() {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!("memory proposal {} already exists", proposal.proposal_id),
            ));
        }
        insert_memory_proposal_in_tx(&tx, proposal, now)?;
        insert_memory_governance_decision_in_tx(
            &tx,
            &MemoryGovernanceDecisionInput {
                decision_id: format!("{}_routed", proposal.proposal_id),
                proposal_id: proposal.proposal_id.clone(),
                decision: MemoryGovernanceDecisionKind::RoutedToReview,
                actor: "rusty_crew_governance".to_string(),
                source: proposal.source,
                evidence_refs: proposal.evidence_refs.clone(),
                policy_mode: selected_governance_mode(proposal.governance_mode, proposal.source),
                confidence: Some(proposal.confidence),
                message: Some("typed memory proposals start in curator/manual review".to_string()),
                resulting_revision: None,
                decided_at: Some(now.clone()),
            },
        )?;
        let record = get_memory_proposal_by_id(&tx, &proposal.proposal_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "saved memory proposal was not readable",
            )
        })?;
        tx.commit()
            .map_err(|error| persistence_error("commit save memory proposal", error))?;
        Ok(record)
    }

    pub fn list_memory_proposals(
        &self,
        query: &MemoryProposalQuery,
    ) -> CoreResult<Vec<MemoryProposalRecord>> {
        let conn = self.conn()?;
        list_memory_proposals(&conn, query)
    }

    pub fn record_memory_governance_decision(
        &self,
        decision: &MemoryGovernanceDecisionInput,
        now: &IsoTimestamp,
    ) -> CoreResult<MemoryGovernanceDecisionRecord> {
        validate_memory_governance_decision(decision)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start memory governance decision", error))?;
        let proposal = get_memory_proposal_by_id(&tx, &decision.proposal_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("memory proposal {} not found", decision.proposal_id),
            )
        })?;
        validate_memory_governance_transition(proposal.status, decision.decision)?;
        let mut stored = decision.clone();
        if stored.decided_at.is_none() {
            stored.decided_at = Some(now.clone());
        }
        let record = insert_memory_governance_decision_in_tx(&tx, &stored)?;
        update_memory_proposal_review_state_in_tx(&tx, &record)?;
        tx.commit()
            .map_err(|error| persistence_error("commit memory governance decision", error))?;
        Ok(record)
    }

    pub fn get_simple_kv(
        &self,
        scope: &SimpleKvScope,
        key: &str,
        now: Option<&IsoTimestamp>,
    ) -> CoreResult<Option<SimpleKvRecord>> {
        validate_simple_kv_identity(scope, key)?;
        let conn = self.conn()?;
        get_simple_kv(&conn, scope, key, now)
    }

    pub fn list_simple_kv(&self, query: &SimpleKvQuery) -> CoreResult<Vec<SimpleKvRecord>> {
        validate_simple_kv_query(query)?;
        let conn = self.conn()?;
        list_simple_kv(&conn, query)
    }

    pub fn put_simple_kv(&self, write: &SimpleKvWrite) -> CoreResult<SimpleKvRecord> {
        validate_simple_kv_write(write)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start put simple kv", error))?;
        let record = put_simple_kv_in_tx(&tx, write)?;
        tx.commit()
            .map_err(|error| persistence_error("commit put simple kv", error))?;
        Ok(record)
    }

    pub fn compare_and_swap_simple_kv(
        &self,
        compare_and_swap: &SimpleKvCompareAndSwap,
    ) -> CoreResult<SimpleKvRecord> {
        validate_simple_kv_write(&compare_and_swap.write)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start compare-and-swap simple kv", error))?;
        let existing = get_simple_kv(
            &tx,
            &compare_and_swap.write.scope,
            &compare_and_swap.write.key,
            None,
        )?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!(
                    "simple_kv entry {}/{} not found",
                    compare_and_swap.write.scope.scope_id, compare_and_swap.write.key
                ),
            )
        })?;
        if existing.revision != compare_and_swap.expected_revision {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "simple_kv revision mismatch for {}/{}: expected {}, found {}",
                    compare_and_swap.write.scope.scope_id,
                    compare_and_swap.write.key,
                    compare_and_swap.expected_revision,
                    existing.revision
                ),
            ));
        }
        let record = update_simple_kv_in_tx(&tx, &compare_and_swap.write, existing.revision + 1)?;
        tx.commit()
            .map_err(|error| persistence_error("commit compare-and-swap simple kv", error))?;
        Ok(record)
    }

    pub fn delete_simple_kv(&self, delete: &SimpleKvDelete) -> CoreResult<SimpleKvRecord> {
        validate_simple_kv_identity(&delete.scope, &delete.key)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start delete simple kv", error))?;
        let existing = get_simple_kv(&tx, &delete.scope, &delete.key, None)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!(
                    "simple_kv entry {}/{} not found",
                    delete.scope.scope_id, delete.key
                ),
            )
        })?;
        if existing.revision != delete.expected_revision {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "simple_kv revision mismatch for {}/{}: expected {}, found {}",
                    delete.scope.scope_id, delete.key, delete.expected_revision, existing.revision
                ),
            ));
        }
        tx.execute(
            "DELETE FROM module_simple_kv_entries
             WHERE scope_type = ?1 AND scope_id = ?2 AND entry_key = ?3",
            params![
                delete.scope.scope_type.as_str(),
                delete.scope.scope_id.as_str(),
                delete.key.as_str()
            ],
        )
        .map_err(|error| persistence_error("delete simple kv", error))?;
        tx.commit()
            .map_err(|error| persistence_error("commit delete simple kv", error))?;
        Ok(existing)
    }

    pub fn expire_simple_kv(&self, now: &IsoTimestamp) -> CoreResult<u64> {
        let conn = self.conn()?;
        expire_simple_kv(&conn, now)
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

    pub fn save_provider_wire_state(
        &self,
        write: &ProviderWireStateWrite,
    ) -> CoreResult<ProviderWireStateRecord> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start save provider wire state", error))?;
        let record = save_provider_wire_state_in_tx(&tx, write)?;
        tx.commit()
            .map_err(|error| persistence_error("commit save provider wire state", error))?;
        Ok(record)
    }

    pub fn load_provider_wire_state_for_wake(
        &self,
        lookup: &ProviderWireStateWakeLookup,
    ) -> CoreResult<ProviderWireStateWakeResult> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start load provider wire state", error))?;
        let result = load_provider_wire_state_for_wake_in_tx(&tx, lookup)?;
        tx.commit()
            .map_err(|error| persistence_error("commit load provider wire state", error))?;
        Ok(result)
    }

    pub fn clear_provider_wire_state(
        &self,
        key: &ProviderWireStateKey,
        now: &IsoTimestamp,
        reason: ProviderWireStateInvalidationReason,
    ) -> CoreResult<Option<ProviderWireStateRecord>> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start clear provider wire state", error))?;
        let cleared = clear_provider_wire_state_in_tx(&tx, key, now, reason)?;
        tx.commit()
            .map_err(|error| persistence_error("commit clear provider wire state", error))?;
        Ok(cleared)
    }

    pub fn expire_provider_wire_states_at(
        &self,
        now: &IsoTimestamp,
    ) -> CoreResult<Vec<ProviderWireStateRecord>> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start expire provider wire states", error))?;
        let expired = expire_provider_wire_states_in_tx(&tx, now)?;
        tx.commit()
            .map_err(|error| persistence_error("commit expire provider wire states", error))?;
        Ok(expired)
    }

    pub fn list_provider_wire_state_diagnostics(
        &self,
        limit: u32,
    ) -> CoreResult<Vec<ProviderWireStateDiagnostic>> {
        let conn = self.conn()?;
        list_provider_wire_state_diagnostics(&conn, limit)
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

    pub fn storage_diagnostics(&self) -> CoreResult<RuntimeStorageDiagnostics> {
        let conn = self.conn()?;
        let size = database_size(&conn)?;
        let migrations = load_schema_migration_records(&conn)?;
        let schema_version = current_schema_version(&conn)?;
        let table_counts = DiagnosticTable::ALL
            .iter()
            .map(|table| {
                let rows = count_diagnostic_table_rows(&conn, *table)?;
                Ok(RuntimeStorageTableCount {
                    table: table.as_str().to_string(),
                    rows,
                })
            })
            .collect::<CoreResult<Vec<_>>>()?;
        let index_checks = hot_query_plan_checks(&conn)?;
        let search_healthy = sqlite_table_exists(&conn, "runtime_search_fts")?;
        let module_registry = storage_schema_for_registry(
            &conn,
            &compiled_module_schema_registry(),
            &sqlite_module_schema_capabilities(),
        )?;
        let pressure = size.wal_bytes > 64 * 1024 * 1024
            || (size.database_bytes > 0 && size.freelist_bytes > size.database_bytes / 4)
            || !index_checks.iter().all(|check| check.uses_index)
            || !search_healthy;
        Ok(RuntimeStorageDiagnostics {
            backend: "sqlite".to_string(),
            backend_label: "SQLite WAL".to_string(),
            schema_version,
            supported_schema_version: CURRENT_SCHEMA_VERSION,
            migrations,
            size,
            table_counts,
            capabilities: sqlite_storage_capabilities(),
            repository_groups: repositories::core_repository_group_diagnostics(),
            module_registry,
            index_checks,
            search_healthy,
            pressure,
        })
    }

    pub fn storage_schema(&self) -> CoreResult<RuntimeModuleSchemaRegistryDiagnostics> {
        let conn = self.conn()?;
        storage_schema_for_registry(
            &conn,
            &compiled_module_schema_registry(),
            &sqlite_module_schema_capabilities(),
        )
    }

    pub fn storage_schema_for_registry(
        &self,
        registry: &ModuleSchemaRegistry,
        supported_capabilities: &[ModuleSchemaCapability],
    ) -> CoreResult<RuntimeModuleSchemaRegistryDiagnostics> {
        let conn = self.conn()?;
        storage_schema_for_registry(&conn, registry, supported_capabilities)
    }

    pub fn run_maintenance(
        &self,
        policy: &RuntimeMaintenancePolicy,
    ) -> CoreResult<RuntimeMaintenanceReport> {
        let size_before = self.database_size()?;
        let mut expired_queue_messages = 0;
        let mut purged_terminal_queue_messages = 0;
        let mut expired_provider_wire_states = 0;
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
            if let Some(now) = &policy.expire_provider_wire_states_at {
                expired_provider_wire_states =
                    expire_provider_wire_states_in_tx(&tx, now)?.len() as u64;
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
            expired_provider_wire_states,
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
                    last_active_at,
                    history_window_json
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
                let history_window_json: Option<String> = row.get(12)?;
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
                let history_window = history_window_json
                    .as_deref()
                    .map(from_json_text::<SessionHistoryWindow>)
                    .transpose()
                    .map_err(to_sql_error)?;
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
                    history_window,
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

    pub fn save_message_slot(&self, slot: &MessageSlotWrite) -> CoreResult<()> {
        let conn = self.conn()?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|error| persistence_error("begin save message slot", error))?;
        save_message_slot_in_tx(&tx, slot)?;
        tx.commit()
            .map_err(|error| persistence_error("commit save message slot", error))?;
        Ok(())
    }

    pub fn save_message_variant(
        &self,
        variant: &MessageVariantWrite,
    ) -> CoreResult<MessageVariantRecord> {
        let conn = self.conn()?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|error| persistence_error("begin save message variant", error))?;
        save_message_variant_in_tx(&tx, variant)?;
        let record = load_message_variant_in_tx(&tx, &variant.variant_id)?;
        tx.commit()
            .map_err(|error| persistence_error("commit save message variant", error))?;
        Ok(record)
    }

    pub fn query_message_slots(
        &self,
        query: &MessageSlotQuery,
    ) -> CoreResult<Vec<MessageSlotRecord>> {
        let conn = self.conn()?;
        query_message_slots(&conn, query)
    }

    pub fn query_message_variants(
        &self,
        query: &MessageVariantQuery,
    ) -> CoreResult<Vec<MessageVariantRecord>> {
        let conn = self.conn()?;
        query_message_variants(&conn, query)
    }

    pub fn save_conversation_branch(
        &self,
        branch: &ConversationBranchWrite,
    ) -> CoreResult<ConversationBranchRecord> {
        let conn = self.conn()?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|error| persistence_error("begin save conversation branch", error))?;
        save_conversation_branch_in_tx(&tx, branch)?;
        let record = load_conversation_branch_in_tx(&tx, &branch.branch_id)?;
        tx.commit()
            .map_err(|error| persistence_error("commit save conversation branch", error))?;
        Ok(record)
    }

    pub fn query_conversation_branches(
        &self,
        query: &ConversationBranchQuery,
    ) -> CoreResult<Vec<ConversationBranchRecord>> {
        let conn = self.conn()?;
        query_conversation_branches(&conn, query)
    }

    pub fn get_conversation_branch_state(
        &self,
        session_id: &SessionId,
        default_updated_at: &IsoTimestamp,
    ) -> CoreResult<ConversationBranchStateRecord> {
        let conn = self.conn()?;
        load_conversation_branch_state(&conn, session_id, default_updated_at)
    }

    pub fn select_active_conversation_branch(
        &self,
        request: &SelectActiveBranchRequest,
    ) -> CoreResult<SelectActiveBranchResult> {
        let conn = self.conn()?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|error| persistence_error("begin select active branch", error))?;
        let current = current_active_branch_in_tx(&tx, &request.session_id)?;
        let expected = match &request.expected {
            ActiveBranchExpectation::Any => current.clone(),
            ActiveBranchExpectation::None => None,
            ActiveBranchExpectation::Branch(branch_id) => Some(branch_id.clone()),
        };
        if request.expected != ActiveBranchExpectation::Any && current != expected {
            let state = load_conversation_branch_state_in_tx(
                &tx,
                &request.session_id,
                &request.updated_at,
            )?;
            tx.commit()
                .map_err(|error| persistence_error("commit active branch conflict", error))?;
            return Ok(SelectActiveBranchResult {
                state,
                conflict: Some(ActiveBranchConflict {
                    expected,
                    actual: current,
                }),
            });
        }
        if let Some(branch_id) = &request.active_branch_id {
            ensure_branch_belongs_to_session_in_tx(&tx, &request.session_id, branch_id)?;
        }
        tx.execute(
            "INSERT INTO conversation_branch_state (
                session_id, active_branch_id, updated_at, version
             ) VALUES (?1, ?2, ?3, 0)
             ON CONFLICT(session_id) DO UPDATE SET
                active_branch_id = excluded.active_branch_id,
                updated_at = excluded.updated_at,
                version = conversation_branch_state.version + 1",
            params![
                request.session_id.0,
                request
                    .active_branch_id
                    .as_ref()
                    .map(|value| value.0.as_str()),
                request.updated_at,
            ],
        )
        .map_err(|error| persistence_error("select active conversation branch", error))?;
        let state =
            load_conversation_branch_state_in_tx(&tx, &request.session_id, &request.updated_at)?;
        tx.commit()
            .map_err(|error| persistence_error("commit select active branch", error))?;
        Ok(SelectActiveBranchResult {
            state,
            conflict: None,
        })
    }

    pub fn update_conversation_branch_head(
        &self,
        request: &UpdateBranchHeadRequest,
    ) -> CoreResult<UpdateBranchHeadResult> {
        let conn = self.conn()?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|error| persistence_error("begin update branch head", error))?;
        let current = current_branch_head_in_tx(&tx, &request.branch_id)?;
        let expected = match &request.expected {
            BranchHeadExpectation::Any => current.clone(),
            BranchHeadExpectation::None => None,
            BranchHeadExpectation::Message(message_id) => Some(message_id.clone()),
        };
        if request.expected != BranchHeadExpectation::Any && current != expected {
            let branch = load_conversation_branch_in_tx(&tx, &request.branch_id)?;
            tx.commit()
                .map_err(|error| persistence_error("commit branch head conflict", error))?;
            return Ok(UpdateBranchHeadResult {
                branch,
                conflict: Some(BranchHeadConflict {
                    expected,
                    actual: current,
                }),
            });
        }
        if let Some(message_id) = &request.head_message_id {
            ensure_message_exists_in_tx(&tx, message_id)?;
        }
        tx.execute(
            "UPDATE conversation_branches
             SET head_message_id = ?2,
                 updated_at = ?3,
                 version = version + 1
             WHERE branch_id = ?1",
            params![
                request.branch_id.0,
                request
                    .head_message_id
                    .as_ref()
                    .map(|value| value.0.as_str()),
                request.updated_at,
            ],
        )
        .map_err(|error| persistence_error("update conversation branch head", error))?;
        let branch = load_conversation_branch_in_tx(&tx, &request.branch_id)?;
        tx.commit()
            .map_err(|error| persistence_error("commit update branch head", error))?;
        Ok(UpdateBranchHeadResult {
            branch,
            conflict: None,
        })
    }

    pub fn save_conversation_snapshot(
        &self,
        snapshot: &ConversationSnapshotWrite,
    ) -> CoreResult<ConversationSnapshotRecord> {
        let conn = self.conn()?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|error| persistence_error("begin save conversation snapshot", error))?;
        save_conversation_snapshot_in_tx(&tx, snapshot)?;
        let record = load_conversation_snapshot_in_tx(&tx, &snapshot.snapshot_id)?;
        tx.commit()
            .map_err(|error| persistence_error("commit save conversation snapshot", error))?;
        Ok(record)
    }

    pub fn query_conversation_snapshots(
        &self,
        query: &ConversationSnapshotQuery,
    ) -> CoreResult<Vec<ConversationSnapshotRecord>> {
        let conn = self.conn()?;
        query_conversation_snapshots(&conn, query)
    }

    pub fn resolve_conversation_jump(
        &self,
        request: &ConversationJumpRequest,
    ) -> CoreResult<ConversationJumpResult> {
        let conn = self.conn()?;
        resolve_conversation_jump(&conn, request)
    }

    pub fn save_attachment(&self, attachment: &AttachmentWrite) -> CoreResult<AttachmentRecord> {
        let conn = self.conn()?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|error| persistence_error("begin save attachment", error))?;
        save_attachment_in_tx(&tx, attachment)?;
        let record = load_attachment_in_tx(&tx, &attachment.attachment_id)?;
        tx.commit()
            .map_err(|error| persistence_error("commit save attachment", error))?;
        Ok(record)
    }

    pub fn query_attachments(&self, query: &AttachmentQuery) -> CoreResult<Vec<AttachmentRecord>> {
        let conn = self.conn()?;
        query_attachments(&conn, query)
    }

    pub fn remove_attachment(
        &self,
        attachment_id: &AttachmentId,
        updated_at: &IsoTimestamp,
    ) -> CoreResult<AttachmentRecord> {
        let conn = self.conn()?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|error| persistence_error("begin remove attachment", error))?;
        tx.execute(
            "UPDATE attachments
             SET status = 'removed', updated_at = ?2
             WHERE attachment_id = ?1",
            params![attachment_id.0.as_str(), updated_at],
        )
        .map_err(|error| persistence_error("remove attachment", error))?;
        let record = load_attachment_in_tx(&tx, attachment_id)?;
        tx.commit()
            .map_err(|error| persistence_error("commit remove attachment", error))?;
        Ok(record)
    }

    pub fn save_data_bank_scope(
        &self,
        scope: &DataBankScopeWrite,
    ) -> CoreResult<DataBankScopeRecord> {
        let conn = self.conn()?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|error| persistence_error("begin save data-bank scope", error))?;
        save_data_bank_scope_in_tx(&tx, scope)?;
        let record = load_data_bank_scope_in_tx(&tx, &scope.scope_id)?;
        tx.commit()
            .map_err(|error| persistence_error("commit save data-bank scope", error))?;
        Ok(record)
    }

    pub fn query_data_bank_scopes(
        &self,
        query: &DataBankScopeQuery,
    ) -> CoreResult<Vec<DataBankScopeRecord>> {
        let conn = self.conn()?;
        query_data_bank_scopes(&conn, query)
    }

    pub fn remove_data_bank_scope(
        &self,
        scope_id: &DataBankScopeId,
        updated_at: &IsoTimestamp,
    ) -> CoreResult<DataBankScopeRecord> {
        let conn = self.conn()?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|error| persistence_error("begin remove data-bank scope", error))?;
        tx.execute(
            "UPDATE data_bank_scopes
             SET status = 'removed', updated_at = ?2
             WHERE scope_id = ?1",
            params![scope_id.0.as_str(), updated_at],
        )
        .map_err(|error| persistence_error("remove data-bank scope", error))?;
        let record = load_data_bank_scope_in_tx(&tx, scope_id)?;
        tx.commit()
            .map_err(|error| persistence_error("commit remove data-bank scope", error))?;
        Ok(record)
    }

    pub fn select_active_message_variant(
        &self,
        request: &SelectActiveVariantRequest,
    ) -> CoreResult<SelectActiveVariantResult> {
        let conn = self.conn()?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|error| persistence_error("begin select active message variant", error))?;
        let current = current_active_variant_in_tx(&tx, &request.slot_id)?;
        let expected = match &request.expected {
            ActiveVariantExpectation::Any => current.clone(),
            ActiveVariantExpectation::Primary => None,
            ActiveVariantExpectation::Variant(variant_id) => Some(variant_id.clone()),
        };
        if request.expected != ActiveVariantExpectation::Any && current != expected {
            let slot = load_message_slot_in_tx(&tx, &request.slot_id, true)?;
            tx.commit()
                .map_err(|error| persistence_error("commit active variant conflict", error))?;
            return Ok(SelectActiveVariantResult {
                slot,
                conflict: Some(ActiveVariantConflict {
                    expected,
                    actual: current,
                }),
            });
        }
        if let Some(variant_id) = &request.active_variant_id {
            ensure_variant_belongs_to_slot_in_tx(&tx, &request.slot_id, variant_id)?;
        }
        tx.execute(
            "UPDATE message_slots
             SET active_variant_id = ?2, updated_at = ?3, version = version + 1
             WHERE slot_id = ?1",
            params![
                request.slot_id.0,
                request
                    .active_variant_id
                    .as_ref()
                    .map(|value| value.0.as_str()),
                request.updated_at,
            ],
        )
        .map_err(|error| persistence_error("select active message variant", error))?;
        let slot = load_message_slot_in_tx(&tx, &request.slot_id, true)?;
        tx.commit()
            .map_err(|error| persistence_error("commit select active message variant", error))?;
        Ok(SelectActiveVariantResult {
            slot,
            conflict: None,
        })
    }

    pub fn delete_message_variant(
        &self,
        slot_id: &MessageSlotId,
        variant_id: &MessageVariantId,
        updated_at: &IsoTimestamp,
    ) -> CoreResult<MessageSlotRecord> {
        let conn = self.conn()?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|error| persistence_error("begin delete message variant", error))?;
        ensure_variant_belongs_to_slot_in_tx(&tx, slot_id, variant_id)?;
        tx.execute(
            "UPDATE message_variants
             SET status = 'deleted', updated_at = ?3
             WHERE slot_id = ?1 AND variant_id = ?2 AND source <> 'primary'",
            params![slot_id.0, variant_id.0, updated_at],
        )
        .map_err(|error| persistence_error("delete message variant", error))?;
        tx.execute(
            "UPDATE message_slots
             SET active_variant_id = CASE
                    WHEN active_variant_id = ?2 THEN NULL
                    ELSE active_variant_id
                 END,
                 updated_at = ?3,
                 version = version + 1
             WHERE slot_id = ?1",
            params![slot_id.0, variant_id.0, updated_at],
        )
        .map_err(|error| persistence_error("clear deleted active variant", error))?;
        let slot = load_message_slot_in_tx(&tx, slot_id, true)?;
        tx.commit()
            .map_err(|error| persistence_error("commit delete message variant", error))?;
        Ok(slot)
    }

    pub fn reorder_message_variants(
        &self,
        slot_id: &MessageSlotId,
        ordered_variant_ids: &[MessageVariantId],
        updated_at: &IsoTimestamp,
    ) -> CoreResult<Vec<MessageVariantRecord>> {
        let conn = self.conn()?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|error| persistence_error("begin reorder message variants", error))?;
        for (index, variant_id) in ordered_variant_ids.iter().enumerate() {
            ensure_variant_belongs_to_slot_in_tx(&tx, slot_id, variant_id)?;
            tx.execute(
                "UPDATE message_variants
                 SET ordinal = ?3, updated_at = ?4
                 WHERE slot_id = ?1 AND variant_id = ?2 AND source <> 'primary'",
                params![slot_id.0, variant_id.0, -((index + 1) as i64), updated_at],
            )
            .map_err(|error| persistence_error("stage reorder message variant", error))?;
        }
        for (index, variant_id) in ordered_variant_ids.iter().enumerate() {
            tx.execute(
                "UPDATE message_variants
                 SET ordinal = ?3, updated_at = ?4
                 WHERE slot_id = ?1 AND variant_id = ?2 AND source <> 'primary'",
                params![slot_id.0, variant_id.0, (index + 1) as i64, updated_at],
            )
            .map_err(|error| persistence_error("reorder message variant", error))?;
        }
        tx.execute(
            "UPDATE message_slots
             SET updated_at = ?2, version = version + 1
             WHERE slot_id = ?1",
            params![slot_id.0, updated_at],
        )
        .map_err(|error| persistence_error("touch reordered message slot", error))?;
        let variants = query_message_variants_in_tx(
            &tx,
            &MessageVariantQuery {
                slot_id: Some(slot_id.clone()),
                include_deleted: false,
                page: None,
            },
        )?;
        tx.commit()
            .map_err(|error| persistence_error("commit reorder message variants", error))?;
        Ok(variants)
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
        count_diagnostic_table_rows(&conn, table)
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

    pub fn installed_module_schemas(&self) -> CoreResult<Vec<InstalledModuleSchemaRecord>> {
        let conn = self.conn()?;
        load_installed_module_schema_records(&conn)
    }

    pub fn install_module_schema_registry(
        &self,
        registry: &ModuleSchemaRegistry,
        supported_capabilities: &[ModuleSchemaCapability],
        now: &IsoTimestamp,
    ) -> CoreResult<Vec<InstalledModuleSchemaRecord>> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start module schema registry install", error))?;
        let installed =
            install_module_schema_registry_in_tx(&tx, registry, supported_capabilities, now)?;
        tx.commit()
            .map_err(|error| persistence_error("commit module schema registry install", error))?;
        Ok(installed)
    }

    fn migrate(&self) -> CoreResult<()> {
        let mut conn = self.conn()?;
        prepare_migration_metadata(&conn)?;
        apply_schema_migrations(&mut conn, SCHEMA_MIGRATIONS)?;
        let now = "startup".to_string();
        let tx = conn.transaction().map_err(|error| {
            persistence_error("start compiled module schema registry install", error)
        })?;
        install_module_schema_registry_in_tx(
            &tx,
            &compiled_module_schema_registry(),
            &sqlite_module_schema_capabilities(),
            &now,
        )?;
        tx.commit().map_err(|error| {
            persistence_error("commit compiled module schema registry install", error)
        })?;
        Ok(())
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

fn sqlite_table_exists(conn: &Connection, table: &str) -> CoreResult<bool> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type IN ('table', 'view') AND name = ?1)",
        params![table],
        |row| row.get::<_, i64>(0),
    )
    .map(|value| value != 0)
    .map_err(|error| persistence_error("check sqlite table existence", error))
}

fn count_diagnostic_table_rows(conn: &Connection, table: DiagnosticTable) -> CoreResult<u64> {
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

fn sqlite_storage_capabilities() -> Vec<RuntimeStorageCapability> {
    [
        (
            "transactions",
            true,
            "single-node ACID transactions are supported",
        ),
        (
            "runtime_full_text_search",
            true,
            "runtime search is backed by the service search capability",
        ),
        (
            "json_metadata",
            true,
            "JSON metadata is stored as validated text blobs",
        ),
        (
            "concurrent_writers",
            false,
            "SQLite serializes writers; WAL improves readers but not write concurrency",
        ),
        (
            "online_migrations",
            false,
            "schema migrations run during service startup/open",
        ),
        (
            "advisory_locks",
            false,
            "SQLite backend has no database-native advisory lock capability",
        ),
        (
            "maintenance_checkpoint",
            true,
            "SQLite WAL checkpoint maintenance is available",
        ),
        (
            "maintenance_vacuum_or_optimize",
            true,
            "SQLite PRAGMA optimize maintenance is available",
        ),
        (
            "estimated_table_size",
            true,
            "SQLite table row counts and database/page size diagnostics are available",
        ),
        (
            "query_plan_diagnostics",
            true,
            "SQLite EXPLAIN QUERY PLAN checks are available for hot diagnostic queries",
        ),
        (
            "row_level_claims",
            false,
            "SQLite claims are scoped to a single service process rather than database row locks",
        ),
        (
            "listen_notify",
            false,
            "SQLite backend has no database-native LISTEN/NOTIFY capability",
        ),
        (
            "logical_export_import",
            false,
            "logical cross-backend export/import records are planned but not implemented",
        ),
    ]
    .into_iter()
    .map(|(name, supported, detail)| RuntimeStorageCapability {
        name: name.to_string(),
        supported,
        detail: detail.to_string(),
    })
    .collect()
}

fn sqlite_module_schema_capabilities() -> Vec<ModuleSchemaCapability> {
    vec![
        ModuleSchemaCapability::Transactions,
        ModuleSchemaCapability::FullTextSearch,
        ModuleSchemaCapability::JsonDocuments,
    ]
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
                | "profile_registry"
                | "profile_memories"
                | "provider_wire_states"
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

fn migrate_v15_add_session_history_window(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    add_missing_column(tx, "sessions", "history_window_json", "TEXT")
}

fn migrate_v16_add_provider_wire_state(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS provider_wire_states (
                row_id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                module_id TEXT NOT NULL,
                strategy_id TEXT NOT NULL,
                profile_fingerprint TEXT NOT NULL,
                provider_fingerprint TEXT NOT NULL,
                payload_version TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                payload_encoding TEXT NOT NULL DEFAULT 'json',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                expires_at TEXT,
                last_wake_id TEXT,
                invalidated_at TEXT,
                invalidation_reason TEXT
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_provider_wire_states_current
                ON provider_wire_states(session_id, module_id, strategy_id)
                WHERE invalidated_at IS NULL;
            CREATE INDEX IF NOT EXISTS idx_provider_wire_states_session_current
                ON provider_wire_states(session_id, invalidated_at);
            CREATE INDEX IF NOT EXISTS idx_provider_wire_states_expiry
                ON provider_wire_states(invalidated_at, expires_at);
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 16", error))
}

fn migrate_v17_add_message_slot_variants(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS message_slots (
                slot_id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                primary_variant_id TEXT NOT NULL,
                active_variant_id TEXT,
                metadata_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                version INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_message_slots_session_slot
                ON message_slots(session_id, slot_id);
            CREATE INDEX IF NOT EXISTS idx_message_slots_active_variant
                ON message_slots(active_variant_id);

            CREATE TABLE IF NOT EXISTS messages (
                message_id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                author_id TEXT NOT NULL,
                author_role TEXT NOT NULL,
                status TEXT NOT NULL,
                body TEXT NOT NULL,
                metadata_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_messages_session_created
                ON messages(session_id, created_at, message_id);

            CREATE TABLE IF NOT EXISTS message_blocks (
                block_id TEXT PRIMARY KEY,
                message_id TEXT NOT NULL,
                ordinal INTEGER NOT NULL,
                kind TEXT NOT NULL,
                content_json TEXT NOT NULL,
                render_policy_json TEXT,
                metadata_json TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_message_blocks_message_ordinal
                ON message_blocks(message_id, ordinal);

            CREATE TABLE IF NOT EXISTS message_variants (
                variant_id TEXT PRIMARY KEY,
                slot_id TEXT NOT NULL,
                source TEXT NOT NULL,
                ordinal INTEGER NOT NULL,
                status TEXT NOT NULL,
                message_id TEXT NOT NULL,
                metadata_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                FOREIGN KEY (slot_id) REFERENCES message_slots(slot_id),
                FOREIGN KEY (message_id) REFERENCES messages(message_id)
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_message_variants_slot_ordinal
                ON message_variants(slot_id, ordinal);
            CREATE INDEX IF NOT EXISTS idx_message_variants_slot_status
                ON message_variants(slot_id, status, ordinal);
            CREATE INDEX IF NOT EXISTS idx_message_variants_message
                ON message_variants(message_id);
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 17", error))
}

fn migrate_v18_add_conversation_tree(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    add_missing_column_tx(tx, "messages", "branch_id", "TEXT")?;
    add_missing_column_tx(tx, "messages", "parent_message_id", "TEXT")?;
    add_missing_column_tx(tx, "messages", "previous_message_id", "TEXT")?;
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS conversation_branches (
                branch_id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                parent_branch_id TEXT,
                parent_message_id TEXT,
                origin_message_id TEXT,
                head_message_id TEXT,
                label TEXT,
                metadata_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                version INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_conversation_branches_session_branch
                ON conversation_branches(session_id, branch_id);
            CREATE INDEX IF NOT EXISTS idx_conversation_branches_parent_branch
                ON conversation_branches(parent_branch_id);
            CREATE INDEX IF NOT EXISTS idx_conversation_branches_parent_message
                ON conversation_branches(parent_message_id);
            CREATE INDEX IF NOT EXISTS idx_conversation_branches_session_created
                ON conversation_branches(session_id, created_at, branch_id);

            CREATE TABLE IF NOT EXISTS conversation_branch_state (
                session_id TEXT PRIMARY KEY,
                active_branch_id TEXT,
                updated_at TEXT NOT NULL,
                version INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS conversation_snapshots (
                snapshot_id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                branch_id TEXT,
                message_id TEXT,
                cursor TEXT,
                label TEXT,
                summary TEXT,
                source TEXT NOT NULL,
                metadata_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_conversation_snapshots_session_message
                ON conversation_snapshots(session_id, message_id);
            CREATE INDEX IF NOT EXISTS idx_conversation_snapshots_session_branch
                ON conversation_snapshots(session_id, branch_id, created_at);
            CREATE INDEX IF NOT EXISTS idx_conversation_snapshots_session_created
                ON conversation_snapshots(session_id, created_at, snapshot_id);

            CREATE INDEX IF NOT EXISTS idx_messages_session_branch
                ON messages(session_id, branch_id);
            CREATE INDEX IF NOT EXISTS idx_messages_parent_message
                ON messages(parent_message_id);
            CREATE INDEX IF NOT EXISTS idx_messages_branch_created
                ON messages(branch_id, created_at, message_id);
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 18", error))
}

fn migrate_v19_add_chat_attachments(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS attachments (
                attachment_id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                status TEXT NOT NULL,
                filename TEXT NOT NULL,
                mime_type TEXT NOT NULL,
                byte_size INTEGER NOT NULL,
                storage_url TEXT,
                download_url TEXT,
                thumbnail_url TEXT,
                extracted_text TEXT,
                extracted_text_truncated INTEGER NOT NULL DEFAULT 0,
                metadata_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                expires_at TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_attachments_session_status
                ON attachments(session_id, status, created_at, attachment_id);
            CREATE INDEX IF NOT EXISTS idx_attachments_expiry
                ON attachments(expires_at);

            CREATE TABLE IF NOT EXISTS attachment_links (
                link_id TEXT PRIMARY KEY,
                attachment_id TEXT NOT NULL,
                session_id TEXT NOT NULL,
                message_id TEXT,
                block_id TEXT,
                scope_id TEXT,
                metadata_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY (attachment_id) REFERENCES attachments(attachment_id)
            );
            CREATE INDEX IF NOT EXISTS idx_attachment_links_attachment
                ON attachment_links(attachment_id, created_at, link_id);
            CREATE INDEX IF NOT EXISTS idx_attachment_links_session_message
                ON attachment_links(session_id, message_id);
            CREATE INDEX IF NOT EXISTS idx_attachment_links_session_scope
                ON attachment_links(session_id, scope_id);

            CREATE TABLE IF NOT EXISTS data_bank_scopes (
                scope_id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                status TEXT NOT NULL,
                label TEXT,
                description TEXT,
                metadata_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_data_bank_scopes_session_status
                ON data_bank_scopes(session_id, status, created_at, scope_id);
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 19", error))
}

fn migrate_v20_add_module_schema_registry(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS module_schema_versions (
                module_id TEXT PRIMARY KEY,
                installed_version INTEGER NOT NULL,
                descriptor_fingerprint TEXT NOT NULL,
                installed_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_module_schema_versions_version
                ON module_schema_versions(installed_version, module_id);
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 20", error))
}

fn migrate_v21_add_memory_proposal_governance(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS memory_proposals (
                proposal_id TEXT PRIMARY KEY,
                space_id TEXT NOT NULL,
                operation TEXT NOT NULL,
                scope_type TEXT NOT NULL,
                scope_id TEXT NOT NULL,
                shape_id TEXT NOT NULL,
                shape_version INTEGER NOT NULL,
                envelope_json TEXT NOT NULL,
                status TEXT NOT NULL,
                selected_governance_mode TEXT NOT NULL,
                source TEXT NOT NULL,
                dedupe_key TEXT,
                duplicate_of TEXT,
                resulting_revision INTEGER,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                decided_at TEXT,
                applied_at TEXT
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_memory_proposals_dedupe
                ON memory_proposals(space_id, dedupe_key)
                WHERE dedupe_key IS NOT NULL;
            CREATE INDEX IF NOT EXISTS idx_memory_proposals_status
                ON memory_proposals(status, updated_at DESC, proposal_id);
            CREATE INDEX IF NOT EXISTS idx_memory_proposals_space_status
                ON memory_proposals(space_id, status, updated_at DESC, proposal_id);

            CREATE TABLE IF NOT EXISTS memory_governance_decisions (
                decision_id TEXT PRIMARY KEY,
                proposal_id TEXT NOT NULL,
                decision TEXT NOT NULL,
                actor TEXT NOT NULL,
                source TEXT NOT NULL,
                evidence_refs_json TEXT NOT NULL,
                policy_mode TEXT NOT NULL,
                confidence REAL,
                message TEXT,
                resulting_revision INTEGER,
                decided_at TEXT NOT NULL,
                FOREIGN KEY (proposal_id) REFERENCES memory_proposals(proposal_id)
            );
            CREATE INDEX IF NOT EXISTS idx_memory_governance_decisions_proposal
                ON memory_governance_decisions(proposal_id, decided_at, decision_id);
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 21", error))
}

fn migrate_v22_add_profile_registry(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS profile_registry (
                profile_id TEXT PRIMARY KEY,
                lifecycle_status TEXT NOT NULL,
                display_name TEXT,
                summary TEXT,
                default_session_kind TEXT,
                agent_id TEXT,
                owner_id TEXT,
                active_runtime_settings_json TEXT NOT NULL,
                source_asset_refs_json TEXT NOT NULL,
                derived_runtime_refs_json TEXT NOT NULL,
                import_export_json TEXT NOT NULL,
                revision INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_profile_registry_lifecycle
                ON profile_registry(lifecycle_status, updated_at DESC, profile_id);
            CREATE INDEX IF NOT EXISTS idx_profile_registry_updated
                ON profile_registry(updated_at DESC, profile_id);
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 22", error))
}

fn apply_module_schema_migration_in_tx(
    tx: &rusqlite::Transaction<'_>,
    bundle: &ModuleSchemaBundle,
    installed_version: Option<u32>,
) -> CoreResult<()> {
    match bundle.module_id.as_str() {
        "simple_kv" => apply_simple_kv_module_schema_in_tx(tx, bundle, installed_version),
        module_id => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("module {module_id} has no registered migration implementation"),
        )),
    }
}

fn apply_simple_kv_module_schema_in_tx(
    tx: &rusqlite::Transaction<'_>,
    bundle: &ModuleSchemaBundle,
    _installed_version: Option<u32>,
) -> CoreResult<()> {
    if bundle.schema_version != 1 {
        return Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!(
                "simple_kv schema version {} has no migration implementation",
                bundle.schema_version
            ),
        ));
    }
    let table = bundle
        .tables
        .iter()
        .find(|table| table.table_name.as_str() == "entries")
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "simple_kv descriptor is missing entries table",
            )
        })?
        .physical_name(&bundle.module_id)?;
    let index = bundle
        .indexes
        .iter()
        .find(|index| {
            index.table_name.as_str() == "entries" && index.purpose.as_str() == "scope_key"
        })
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "simple_kv descriptor is missing scope_key index",
            )
        })?
        .physical_name(&bundle.module_id)?;
    let expiry_index = bundle
        .indexes
        .iter()
        .find(|index| {
            index.table_name.as_str() == "entries" && index.purpose.as_str() == "expires_at"
        })
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "simple_kv descriptor is missing expires_at index",
            )
        })?
        .physical_name(&bundle.module_id)?;
    let table = sqlite_identifier(&table)?;
    let index = sqlite_identifier(&index)?;
    let expiry_index = sqlite_identifier(&expiry_index)?;
    tx.execute_batch(&format!(
        "
            CREATE TABLE IF NOT EXISTS {table} (
                scope_type TEXT NOT NULL,
                scope_id TEXT NOT NULL,
                entry_key TEXT NOT NULL,
                value_json TEXT NOT NULL,
                revision INTEGER NOT NULL CHECK (revision > 0),
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                expires_at TEXT,
                PRIMARY KEY (scope_type, scope_id, entry_key)
            );
            CREATE UNIQUE INDEX IF NOT EXISTS {index}
                ON {table}(scope_type, scope_id, entry_key);
            CREATE INDEX IF NOT EXISTS {expiry_index}
                ON {table}(expires_at)
                WHERE expires_at IS NOT NULL;
            "
    ))
    .map_err(|error| persistence_error("apply simple_kv module schema", error))
}

fn sqlite_identifier(identifier: &str) -> CoreResult<String> {
    if identifier.is_empty()
        || !identifier.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
    {
        return Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("unsafe sqlite identifier {identifier:?}"),
        ));
    }
    Ok(identifier.to_string())
}

fn install_module_schema_registry_in_tx(
    tx: &rusqlite::Transaction<'_>,
    registry: &ModuleSchemaRegistry,
    supported_capabilities: &[ModuleSchemaCapability],
    now: &IsoTimestamp,
) -> CoreResult<Vec<InstalledModuleSchemaRecord>> {
    registry.validate()?;
    registry.validate_capabilities(supported_capabilities)?;

    let mut installed = Vec::new();
    for bundle in registry.bundles() {
        let module_id = bundle.module_id.as_str();
        let descriptor_fingerprint = bundle.descriptor_fingerprint()?;
        let existing = load_installed_module_schema_record(tx, module_id)?;
        if let Some(existing) = existing {
            validate_version_progression(Some(existing.installed_version), bundle.schema_version)?;
            apply_module_schema_migration_in_tx(tx, bundle, Some(existing.installed_version))?;
            if existing.installed_version == bundle.schema_version {
                if existing.descriptor_fingerprint != descriptor_fingerprint {
                    return Err(CoreError::new(
                        CoreErrorKind::ActionRejected,
                        format!(
                            "module {module_id} descriptor fingerprint changed without a schema version bump"
                        ),
                    ));
                }
                installed.push(existing);
                continue;
            }
        } else {
            validate_version_progression(None, bundle.schema_version)?;
            apply_module_schema_migration_in_tx(tx, bundle, None)?;
        }

        tx.execute(
            "INSERT INTO module_schema_versions (
                module_id,
                installed_version,
                descriptor_fingerprint,
                installed_at,
                updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?4)
            ON CONFLICT(module_id) DO UPDATE SET
                installed_version = excluded.installed_version,
                descriptor_fingerprint = excluded.descriptor_fingerprint,
                updated_at = excluded.updated_at",
            params![
                module_id,
                bundle.schema_version as i64,
                descriptor_fingerprint.as_str(),
                now.as_str(),
            ],
        )
        .map_err(|error| persistence_error("upsert module schema version", error))?;
        installed.push(
            load_installed_module_schema_record(tx, module_id)?.ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    format!("module {module_id} schema version missing after install"),
                )
            })?,
        );
    }

    Ok(installed)
}

fn load_installed_module_schema_records(
    conn: &Connection,
) -> CoreResult<Vec<InstalledModuleSchemaRecord>> {
    let mut stmt = conn
        .prepare(
            "SELECT module_id, installed_version, descriptor_fingerprint, installed_at, updated_at
             FROM module_schema_versions
             ORDER BY module_id ASC",
        )
        .map_err(|error| persistence_error("prepare installed module schema records", error))?;
    let rows = stmt
        .query_map([], row_to_installed_module_schema_record)
        .map_err(|error| persistence_error("query installed module schema records", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load installed module schema records", error))
}

fn storage_schema_for_registry(
    conn: &Connection,
    registry: &ModuleSchemaRegistry,
    supported_capabilities: &[ModuleSchemaCapability],
) -> CoreResult<RuntimeModuleSchemaRegistryDiagnostics> {
    let installed = load_installed_module_schema_records(conn)?;
    module_schema_registry_diagnostics(registry, &installed, supported_capabilities)
}

fn load_installed_module_schema_record(
    conn: &Connection,
    module_id: &str,
) -> CoreResult<Option<InstalledModuleSchemaRecord>> {
    conn.query_row(
        "SELECT module_id, installed_version, descriptor_fingerprint, installed_at, updated_at
         FROM module_schema_versions
         WHERE module_id = ?1",
        params![module_id],
        row_to_installed_module_schema_record,
    )
    .optional()
    .map_err(|error| persistence_error("load installed module schema record", error))
}

fn row_to_installed_module_schema_record(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<InstalledModuleSchemaRecord> {
    let raw_module_id: String = row.get(0)?;
    let installed_version: i64 = row.get(1)?;
    if installed_version <= 0 || installed_version > i64::from(u32::MAX) {
        return Err(rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Integer,
            Box::new(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("invalid installed module schema version {installed_version}"),
            )),
        ));
    }
    let module_id = ModuleId::new(raw_module_id).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(InstalledModuleSchemaRecord {
        module_id,
        installed_version: installed_version as u32,
        descriptor_fingerprint: row.get(2)?,
        installed_at: row.get(3)?,
        updated_at: row.get(4)?,
    })
}

fn save_provider_wire_state_in_tx(
    tx: &rusqlite::Transaction<'_>,
    write: &ProviderWireStateWrite,
) -> CoreResult<ProviderWireStateRecord> {
    validate_provider_wire_state_key(&write.key)?;
    let payload_json = to_json_text(&write.payload_json)?;
    invalidate_current_provider_wire_state_for_key_in_tx(
        tx,
        &write.key,
        &write.now,
        ProviderWireStateInvalidationReason::Superseded,
    )?;
    tx.execute(
        "INSERT INTO provider_wire_states (
            session_id,
            module_id,
            strategy_id,
            profile_fingerprint,
            provider_fingerprint,
            payload_version,
            payload_json,
            payload_encoding,
            created_at,
            updated_at,
            expires_at,
            last_wake_id,
            invalidated_at,
            invalidation_reason
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'json', ?8, ?8, ?9, ?10, NULL, NULL)",
        params![
            write.key.session_id.0.as_str(),
            write.key.module_id.as_str(),
            write.key.strategy_id.as_str(),
            write.profile_fingerprint.as_str(),
            write.provider_fingerprint.as_str(),
            write.payload_version.as_str(),
            payload_json,
            write.now.as_str(),
            write.expires_at.as_deref(),
            write.last_wake_id.as_deref(),
        ],
    )
    .map_err(|error| persistence_error("insert provider wire state", error))?;
    load_provider_wire_state_by_row_id(tx, tx.last_insert_rowid())
}

fn load_provider_wire_state_for_wake_in_tx(
    tx: &rusqlite::Transaction<'_>,
    lookup: &ProviderWireStateWakeLookup,
) -> CoreResult<ProviderWireStateWakeResult> {
    validate_provider_wire_state_key(&lookup.key)?;
    invalidate_provider_wire_states_for_session_except_in_tx(tx, &lookup.key, &lookup.now)?;
    let Some(record) = load_current_provider_wire_state_by_key(tx, &lookup.key)? else {
        return Ok(ProviderWireStateWakeResult {
            record: None,
            absence_reason: Some(ProviderStateAbsenceReason::Missing),
        });
    };
    if record
        .expires_at
        .as_ref()
        .is_some_and(|expires| expires <= &lookup.now)
    {
        invalidate_provider_wire_state_by_row_in_tx(
            tx,
            record.row_id,
            &lookup.now,
            ProviderWireStateInvalidationReason::Expired,
        )?;
        return Ok(ProviderWireStateWakeResult {
            record: None,
            absence_reason: Some(ProviderStateAbsenceReason::Expired),
        });
    }
    if record.profile_fingerprint != lookup.profile_fingerprint {
        invalidate_provider_wire_state_by_row_in_tx(
            tx,
            record.row_id,
            &lookup.now,
            ProviderWireStateInvalidationReason::ProfileChanged,
        )?;
        return Ok(ProviderWireStateWakeResult {
            record: None,
            absence_reason: Some(ProviderStateAbsenceReason::Invalidated),
        });
    }
    if record.provider_fingerprint != lookup.provider_fingerprint {
        invalidate_provider_wire_state_by_row_in_tx(
            tx,
            record.row_id,
            &lookup.now,
            ProviderWireStateInvalidationReason::ProviderChanged,
        )?;
        return Ok(ProviderWireStateWakeResult {
            record: None,
            absence_reason: Some(ProviderStateAbsenceReason::Invalidated),
        });
    }
    Ok(ProviderWireStateWakeResult {
        record: Some(record),
        absence_reason: None,
    })
}

fn clear_provider_wire_state_in_tx(
    tx: &rusqlite::Transaction<'_>,
    key: &ProviderWireStateKey,
    now: &IsoTimestamp,
    reason: ProviderWireStateInvalidationReason,
) -> CoreResult<Option<ProviderWireStateRecord>> {
    validate_provider_wire_state_key(key)?;
    let Some(record) = load_current_provider_wire_state_by_key(tx, key)? else {
        return Ok(None);
    };
    invalidate_provider_wire_state_by_row_in_tx(tx, record.row_id, now, reason)?;
    load_provider_wire_state_by_row_id(tx, record.row_id).map(Some)
}

fn expire_provider_wire_states_in_tx(
    tx: &rusqlite::Transaction<'_>,
    now: &IsoTimestamp,
) -> CoreResult<Vec<ProviderWireStateRecord>> {
    let expiring = load_expired_current_provider_wire_states(tx, now)?;
    for record in &expiring {
        invalidate_provider_wire_state_by_row_in_tx(
            tx,
            record.row_id,
            now,
            ProviderWireStateInvalidationReason::Expired,
        )?;
    }
    expiring
        .into_iter()
        .map(|record| load_provider_wire_state_by_row_id(tx, record.row_id))
        .collect()
}

fn invalidate_provider_wire_states_for_session_except_in_tx(
    tx: &rusqlite::Transaction<'_>,
    key: &ProviderWireStateKey,
    now: &IsoTimestamp,
) -> CoreResult<()> {
    tx.execute(
        "UPDATE provider_wire_states
         SET invalidated_at = ?4,
             updated_at = ?4,
             invalidation_reason = CASE
                 WHEN module_id != ?2 THEN 'module_changed'
                 ELSE 'strategy_changed'
             END
         WHERE session_id = ?1
           AND invalidated_at IS NULL
           AND (module_id != ?2 OR strategy_id != ?3)",
        params![
            key.session_id.0.as_str(),
            key.module_id.as_str(),
            key.strategy_id.as_str(),
            now.as_str(),
        ],
    )
    .map_err(|error| persistence_error("invalidate changed provider wire state", error))?;
    Ok(())
}

fn invalidate_current_provider_wire_state_for_key_in_tx(
    tx: &rusqlite::Transaction<'_>,
    key: &ProviderWireStateKey,
    now: &IsoTimestamp,
    reason: ProviderWireStateInvalidationReason,
) -> CoreResult<()> {
    tx.execute(
        "UPDATE provider_wire_states
         SET invalidated_at = ?4,
             updated_at = ?4,
             invalidation_reason = ?5
         WHERE session_id = ?1
           AND module_id = ?2
           AND strategy_id = ?3
           AND invalidated_at IS NULL",
        params![
            key.session_id.0.as_str(),
            key.module_id.as_str(),
            key.strategy_id.as_str(),
            now.as_str(),
            provider_wire_state_invalidation_reason_as_str(reason),
        ],
    )
    .map_err(|error| persistence_error("invalidate current provider wire state", error))?;
    Ok(())
}

fn invalidate_provider_wire_state_by_row_in_tx(
    tx: &rusqlite::Transaction<'_>,
    row_id: i64,
    now: &IsoTimestamp,
    reason: ProviderWireStateInvalidationReason,
) -> CoreResult<()> {
    tx.execute(
        "UPDATE provider_wire_states
         SET invalidated_at = ?2,
             updated_at = ?2,
             invalidation_reason = ?3
         WHERE row_id = ?1
           AND invalidated_at IS NULL",
        params![
            row_id,
            now.as_str(),
            provider_wire_state_invalidation_reason_as_str(reason),
        ],
    )
    .map_err(|error| persistence_error("invalidate provider wire state row", error))?;
    Ok(())
}

fn load_current_provider_wire_state_by_key(
    conn: &Connection,
    key: &ProviderWireStateKey,
) -> CoreResult<Option<ProviderWireStateRecord>> {
    conn.query_row(
        "SELECT
            row_id,
            session_id,
            module_id,
            strategy_id,
            profile_fingerprint,
            provider_fingerprint,
            payload_version,
            payload_json,
            payload_encoding,
            created_at,
            updated_at,
            expires_at,
            last_wake_id,
            invalidated_at,
            invalidation_reason
         FROM provider_wire_states
         WHERE session_id = ?1
           AND module_id = ?2
           AND strategy_id = ?3
           AND invalidated_at IS NULL
         LIMIT 1",
        params![
            key.session_id.0.as_str(),
            key.module_id.as_str(),
            key.strategy_id.as_str(),
        ],
        row_to_provider_wire_state_record,
    )
    .optional()
    .map_err(|error| persistence_error("load current provider wire state", error))
}

fn load_provider_wire_state_by_row_id(
    conn: &Connection,
    row_id: i64,
) -> CoreResult<ProviderWireStateRecord> {
    conn.query_row(
        "SELECT
            row_id,
            session_id,
            module_id,
            strategy_id,
            profile_fingerprint,
            provider_fingerprint,
            payload_version,
            payload_json,
            payload_encoding,
            created_at,
            updated_at,
            expires_at,
            last_wake_id,
            invalidated_at,
            invalidation_reason
         FROM provider_wire_states
         WHERE row_id = ?1",
        params![row_id],
        row_to_provider_wire_state_record,
    )
    .map_err(|error| persistence_error("load provider wire state by row id", error))
}

fn load_expired_current_provider_wire_states(
    conn: &Connection,
    now: &IsoTimestamp,
) -> CoreResult<Vec<ProviderWireStateRecord>> {
    let mut stmt = conn
        .prepare(
            "SELECT
                row_id,
                session_id,
                module_id,
                strategy_id,
                profile_fingerprint,
                provider_fingerprint,
                payload_version,
                payload_json,
                payload_encoding,
                created_at,
                updated_at,
                expires_at,
                last_wake_id,
                invalidated_at,
                invalidation_reason
             FROM provider_wire_states
             WHERE invalidated_at IS NULL
               AND expires_at IS NOT NULL
               AND expires_at <= ?1
             ORDER BY expires_at ASC, row_id ASC",
        )
        .map_err(|error| persistence_error("prepare expired provider wire state query", error))?;
    let rows = stmt
        .query_map(params![now.as_str()], row_to_provider_wire_state_record)
        .map_err(|error| persistence_error("query expired provider wire states", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load expired provider wire states", error))
}

fn list_provider_wire_state_diagnostics(
    conn: &Connection,
    limit: u32,
) -> CoreResult<Vec<ProviderWireStateDiagnostic>> {
    let mut stmt = conn
        .prepare(
            "SELECT
                session_id,
                module_id,
                strategy_id,
                payload_version,
                length(payload_json),
                created_at,
                updated_at,
                expires_at,
                last_wake_id,
                invalidated_at,
                invalidation_reason
             FROM provider_wire_states
             ORDER BY updated_at DESC, row_id DESC
             LIMIT ?1",
        )
        .map_err(|error| persistence_error("prepare provider wire state diagnostics", error))?;
    let rows = stmt
        .query_map(params![limit], row_to_provider_wire_state_diagnostic)
        .map_err(|error| persistence_error("query provider wire state diagnostics", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load provider wire state diagnostics", error))
}

fn row_to_provider_wire_state_record(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ProviderWireStateRecord> {
    let payload_json: String = row.get(7)?;
    let invalidation_reason = row
        .get::<_, Option<String>>(14)?
        .map(|raw| provider_wire_state_invalidation_reason_from_str(&raw))
        .transpose()?;
    Ok(ProviderWireStateRecord {
        row_id: row.get(0)?,
        key: ProviderWireStateKey {
            session_id: SessionId(row.get(1)?),
            module_id: row.get(2)?,
            strategy_id: row.get(3)?,
        },
        profile_fingerprint: row.get(4)?,
        provider_fingerprint: row.get(5)?,
        payload_version: row.get(6)?,
        payload_json: from_json_text(&payload_json).map_err(to_sql_error)?,
        payload_encoding: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        expires_at: row.get(11)?,
        last_wake_id: row.get(12)?,
        invalidated_at: row.get(13)?,
        invalidation_reason,
    })
}

fn row_to_provider_wire_state_diagnostic(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ProviderWireStateDiagnostic> {
    Ok(ProviderWireStateDiagnostic {
        key: ProviderWireStateKey {
            session_id: SessionId(row.get(0)?),
            module_id: row.get(1)?,
            strategy_id: row.get(2)?,
        },
        payload_version: row.get(3)?,
        payload_bytes: row.get::<_, u64>(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
        expires_at: row.get(7)?,
        last_wake_id: row.get(8)?,
        invalidated_at: row.get(9)?,
        invalidation_reason: row.get(10)?,
    })
}

fn validate_provider_wire_state_key(key: &ProviderWireStateKey) -> CoreResult<()> {
    if key.session_id.0.trim().is_empty()
        || key.module_id.trim().is_empty()
        || key.strategy_id.trim().is_empty()
    {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "provider wire state key requires session_id, module_id, and strategy_id",
        ));
    }
    Ok(())
}

fn provider_wire_state_invalidation_reason_as_str(
    reason: ProviderWireStateInvalidationReason,
) -> &'static str {
    match reason {
        ProviderWireStateInvalidationReason::ProfileChanged => "profile_changed",
        ProviderWireStateInvalidationReason::ProviderChanged => "provider_changed",
        ProviderWireStateInvalidationReason::ModuleChanged => "module_changed",
        ProviderWireStateInvalidationReason::StrategyChanged => "strategy_changed",
        ProviderWireStateInvalidationReason::Expired => "expired",
        ProviderWireStateInvalidationReason::BrainRequestedClear => "brain_requested_clear",
        ProviderWireStateInvalidationReason::OperatorRequestedClear => "operator_requested_clear",
        ProviderWireStateInvalidationReason::Superseded => "superseded",
    }
}

fn provider_wire_state_invalidation_reason_from_str(
    raw: &str,
) -> rusqlite::Result<ProviderWireStateInvalidationReason> {
    match raw {
        "profile_changed" => Ok(ProviderWireStateInvalidationReason::ProfileChanged),
        "provider_changed" => Ok(ProviderWireStateInvalidationReason::ProviderChanged),
        "module_changed" => Ok(ProviderWireStateInvalidationReason::ModuleChanged),
        "strategy_changed" => Ok(ProviderWireStateInvalidationReason::StrategyChanged),
        "expired" => Ok(ProviderWireStateInvalidationReason::Expired),
        "brain_requested_clear" => Ok(ProviderWireStateInvalidationReason::BrainRequestedClear),
        "operator_requested_clear" => {
            Ok(ProviderWireStateInvalidationReason::OperatorRequestedClear)
        }
        "superseded" => Ok(ProviderWireStateInvalidationReason::Superseded),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("unknown provider wire state invalidation reason {other}").into(),
        )),
    }
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
                last_active_at,
                history_window_json
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
    let history_window_json: Option<String> = row.get(12)?;
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
        history_window: history_window_json
            .as_deref()
            .map(from_json_text::<SessionHistoryWindow>)
            .transpose()
            .map_err(to_sql_error)?,
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

fn save_message_slot_in_tx(
    tx: &rusqlite::Transaction<'_>,
    slot: &MessageSlotWrite,
) -> CoreResult<()> {
    tx.execute(
        "INSERT INTO message_slots (
            slot_id,
            session_id,
            primary_variant_id,
            active_variant_id,
            metadata_json,
            created_at,
            updated_at,
            version
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)
         ON CONFLICT(slot_id) DO UPDATE SET
            session_id = excluded.session_id,
            primary_variant_id = excluded.primary_variant_id,
            active_variant_id = excluded.active_variant_id,
            metadata_json = excluded.metadata_json,
            updated_at = excluded.updated_at,
            version = message_slots.version + 1",
        params![
            slot.slot_id.0,
            slot.session_id.0,
            slot.primary_variant_id.0,
            slot.active_variant_id
                .as_ref()
                .map(|value| value.0.as_str()),
            to_json_text(&slot.metadata_json)?,
            slot.created_at,
            slot.updated_at,
        ],
    )
    .map_err(|error| persistence_error("save message slot", error))?;
    Ok(())
}

fn save_message_variant_in_tx(
    tx: &rusqlite::Transaction<'_>,
    variant: &MessageVariantWrite,
) -> CoreResult<()> {
    if variant.source == MessageVariantSource::Primary && variant.ordinal != 0 {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "primary message variant ordinal must be 0",
        ));
    }
    save_durable_message_in_tx(tx, &variant.message)?;
    tx.execute(
        "INSERT INTO message_variants (
            variant_id,
            slot_id,
            source,
            ordinal,
            status,
            message_id,
            metadata_json,
            created_at,
            updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(variant_id) DO UPDATE SET
            slot_id = excluded.slot_id,
            source = excluded.source,
            ordinal = excluded.ordinal,
            status = excluded.status,
            message_id = excluded.message_id,
            metadata_json = excluded.metadata_json,
            updated_at = excluded.updated_at",
        params![
            variant.variant_id.0,
            variant.slot_id.0,
            variant.source.as_str(),
            variant.ordinal as i64,
            variant.status.as_str(),
            variant.message.message_id.0,
            to_json_text(&variant.metadata_json)?,
            variant.created_at,
            variant.updated_at,
        ],
    )
    .map_err(|error| persistence_error("save message variant", error))?;
    Ok(())
}

fn save_durable_message_in_tx(
    tx: &rusqlite::Transaction<'_>,
    message: &DurableMessageWrite,
) -> CoreResult<()> {
    tx.execute(
        "INSERT INTO messages (
            message_id,
            session_id,
            branch_id,
            parent_message_id,
            previous_message_id,
            author_id,
            author_role,
            status,
            body,
            metadata_json,
            created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT(message_id) DO UPDATE SET
            session_id = excluded.session_id,
            branch_id = excluded.branch_id,
            parent_message_id = excluded.parent_message_id,
            previous_message_id = excluded.previous_message_id,
            author_id = excluded.author_id,
            author_role = excluded.author_role,
            status = excluded.status,
            body = excluded.body,
            metadata_json = excluded.metadata_json",
        params![
            message.message_id.0,
            message.session_id.0,
            message.branch_id.as_ref().map(|value| value.0.as_str()),
            message
                .parent_message_id
                .as_ref()
                .map(|value| value.0.as_str()),
            message
                .previous_message_id
                .as_ref()
                .map(|value| value.0.as_str()),
            message.author_id,
            message.author_role,
            message.status.as_str(),
            message.body,
            to_json_text(&message.metadata_json)?,
            message.created_at,
        ],
    )
    .map_err(|error| persistence_error("save durable message", error))?;
    tx.execute(
        "DELETE FROM message_blocks WHERE message_id = ?1",
        params![message.message_id.0],
    )
    .map_err(|error| persistence_error("replace message blocks", error))?;
    for block in &message.blocks {
        tx.execute(
            "INSERT INTO message_blocks (
                block_id,
                message_id,
                ordinal,
                kind,
                content_json,
                render_policy_json,
                metadata_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                block.block_id.0,
                message.message_id.0,
                block.ordinal as i64,
                block.kind,
                to_json_text(&block.content_json)?,
                block
                    .render_policy_json
                    .as_ref()
                    .map(to_json_text)
                    .transpose()?,
                to_json_text(&block.metadata_json)?,
            ],
        )
        .map_err(|error| persistence_error("save message block", error))?;
    }
    Ok(())
}

fn query_message_slots(
    conn: &Connection,
    query: &MessageSlotQuery,
) -> CoreResult<Vec<MessageSlotRecord>> {
    let session_id = query.session_id.as_ref().map(|value| value.0.as_str());
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let mut stmt = conn
        .prepare(
            "SELECT slot_id
             FROM message_slots
             WHERE (?1 IS NULL OR session_id = ?1)
             ORDER BY created_at ASC, slot_id ASC
             LIMIT ?2 OFFSET ?3",
        )
        .map_err(|error| persistence_error("prepare query message slots", error))?;
    let slot_ids = stmt
        .query_map(params![session_id, limit, offset], |row| {
            Ok(MessageSlotId::new(row.get::<_, String>(0)?))
        })
        .map_err(|error| persistence_error("query message slots", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load message slot ids", error))?;
    slot_ids
        .iter()
        .map(|slot_id| load_message_slot(conn, slot_id, query.include_alternates))
        .collect()
}

fn query_message_variants(
    conn: &Connection,
    query: &MessageVariantQuery,
) -> CoreResult<Vec<MessageVariantRecord>> {
    let slot_id = query.slot_id.as_ref().map(|value| value.0.as_str());
    let include_deleted = query.include_deleted;
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let mut stmt = conn
        .prepare(
            "SELECT variant_id
             FROM message_variants
             WHERE (?1 IS NULL OR slot_id = ?1)
               AND (?2 OR status <> 'deleted')
             ORDER BY slot_id ASC, ordinal ASC, variant_id ASC
             LIMIT ?3 OFFSET ?4",
        )
        .map_err(|error| persistence_error("prepare query message variants", error))?;
    let variant_ids = stmt
        .query_map(params![slot_id, include_deleted, limit, offset], |row| {
            Ok(MessageVariantId::new(row.get::<_, String>(0)?))
        })
        .map_err(|error| persistence_error("query message variants", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load message variant ids", error))?;
    variant_ids
        .iter()
        .map(|variant_id| load_message_variant(conn, variant_id))
        .collect()
}

fn query_message_variants_in_tx(
    tx: &rusqlite::Transaction<'_>,
    query: &MessageVariantQuery,
) -> CoreResult<Vec<MessageVariantRecord>> {
    let slot_id = query.slot_id.as_ref().map(|value| value.0.as_str());
    let include_deleted = query.include_deleted;
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let mut stmt = tx
        .prepare(
            "SELECT variant_id
             FROM message_variants
             WHERE (?1 IS NULL OR slot_id = ?1)
               AND (?2 OR status <> 'deleted')
             ORDER BY slot_id ASC, ordinal ASC, variant_id ASC
             LIMIT ?3 OFFSET ?4",
        )
        .map_err(|error| persistence_error("prepare query message variants", error))?;
    let variant_ids = stmt
        .query_map(params![slot_id, include_deleted, limit, offset], |row| {
            Ok(MessageVariantId::new(row.get::<_, String>(0)?))
        })
        .map_err(|error| persistence_error("query message variants", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load message variant ids", error))?;
    variant_ids
        .iter()
        .map(|variant_id| load_message_variant_in_tx(tx, variant_id))
        .collect()
}

fn save_conversation_branch_in_tx(
    tx: &rusqlite::Transaction<'_>,
    branch: &ConversationBranchWrite,
) -> CoreResult<()> {
    tx.execute(
        "INSERT INTO conversation_branches (
            branch_id,
            session_id,
            parent_branch_id,
            parent_message_id,
            origin_message_id,
            head_message_id,
            label,
            metadata_json,
            created_at,
            updated_at,
            version
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0)
         ON CONFLICT(branch_id) DO UPDATE SET
            session_id = excluded.session_id,
            parent_branch_id = excluded.parent_branch_id,
            parent_message_id = excluded.parent_message_id,
            origin_message_id = excluded.origin_message_id,
            head_message_id = excluded.head_message_id,
            label = excluded.label,
            metadata_json = excluded.metadata_json,
            updated_at = excluded.updated_at,
            version = conversation_branches.version + 1",
        params![
            branch.branch_id.0,
            branch.session_id.0,
            branch
                .parent_branch_id
                .as_ref()
                .map(|value| value.0.as_str()),
            branch
                .parent_message_id
                .as_ref()
                .map(|value| value.0.as_str()),
            branch
                .origin_message_id
                .as_ref()
                .map(|value| value.0.as_str()),
            branch
                .head_message_id
                .as_ref()
                .map(|value| value.0.as_str()),
            branch.label,
            to_json_text(&branch.metadata_json)?,
            branch.created_at,
            branch.updated_at,
        ],
    )
    .map_err(|error| persistence_error("save conversation branch", error))?;
    Ok(())
}

fn query_conversation_branches(
    conn: &Connection,
    query: &ConversationBranchQuery,
) -> CoreResult<Vec<ConversationBranchRecord>> {
    let session_id = query.session_id.as_ref().map(|value| value.0.as_str());
    let parent_branch_id = query
        .parent_branch_id
        .as_ref()
        .map(|value| value.0.as_str());
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let mut stmt = conn
        .prepare(
            "SELECT branch_id
             FROM conversation_branches
             WHERE (?1 IS NULL OR session_id = ?1)
               AND (?2 IS NULL OR parent_branch_id = ?2)
             ORDER BY created_at ASC, branch_id ASC
             LIMIT ?3 OFFSET ?4",
        )
        .map_err(|error| persistence_error("prepare query conversation branches", error))?;
    let branch_ids = stmt
        .query_map(
            params![session_id, parent_branch_id, limit, offset],
            |row| Ok(ConversationBranchId::new(row.get::<_, String>(0)?)),
        )
        .map_err(|error| persistence_error("query conversation branches", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load conversation branch ids", error))?;
    branch_ids
        .iter()
        .map(|branch_id| load_conversation_branch(conn, branch_id))
        .collect()
}

fn load_conversation_branch(
    conn: &Connection,
    branch_id: &ConversationBranchId,
) -> CoreResult<ConversationBranchRecord> {
    conn.query_row(
        "SELECT session_id, parent_branch_id, parent_message_id, origin_message_id,
                head_message_id, label, metadata_json, created_at, updated_at, version
         FROM conversation_branches
         WHERE branch_id = ?1",
        params![branch_id.0],
        |row| {
            Ok((
                SessionId::new(row.get::<_, String>(0)?),
                row.get::<_, Option<String>>(1)?
                    .map(ConversationBranchId::new),
                row.get::<_, Option<String>>(2)?.map(MessageId::new),
                row.get::<_, Option<String>>(3)?.map(MessageId::new),
                row.get::<_, Option<String>>(4)?.map(MessageId::new),
                row.get::<_, Option<String>>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, i64>(9)? as u64,
            ))
        },
    )
    .optional()
    .map_err(|error| persistence_error("load conversation branch", error))?
    .map(
        |(
            session_id,
            parent_branch_id,
            parent_message_id,
            origin_message_id,
            head_message_id,
            label,
            metadata_json,
            created_at,
            updated_at,
            version,
        )| {
            Ok(ConversationBranchRecord {
                branch_id: branch_id.clone(),
                session_id,
                parent_branch_id,
                parent_message_id,
                origin_message_id,
                head_message_id,
                label,
                metadata_json: parse_json_record(&metadata_json)?,
                created_at,
                updated_at,
                version,
            })
        },
    )
    .transpose()?
    .ok_or_else(|| {
        CoreError::new(
            CoreErrorKind::NotFound,
            format!("conversation branch {branch_id} not found"),
        )
    })
}

fn load_conversation_branch_in_tx(
    tx: &rusqlite::Transaction<'_>,
    branch_id: &ConversationBranchId,
) -> CoreResult<ConversationBranchRecord> {
    tx.query_row(
        "SELECT session_id, parent_branch_id, parent_message_id, origin_message_id,
                head_message_id, label, metadata_json, created_at, updated_at, version
         FROM conversation_branches
         WHERE branch_id = ?1",
        params![branch_id.0],
        |row| {
            Ok((
                SessionId::new(row.get::<_, String>(0)?),
                row.get::<_, Option<String>>(1)?
                    .map(ConversationBranchId::new),
                row.get::<_, Option<String>>(2)?.map(MessageId::new),
                row.get::<_, Option<String>>(3)?.map(MessageId::new),
                row.get::<_, Option<String>>(4)?.map(MessageId::new),
                row.get::<_, Option<String>>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, i64>(9)? as u64,
            ))
        },
    )
    .optional()
    .map_err(|error| persistence_error("load conversation branch in tx", error))?
    .map(
        |(
            session_id,
            parent_branch_id,
            parent_message_id,
            origin_message_id,
            head_message_id,
            label,
            metadata_json,
            created_at,
            updated_at,
            version,
        )| {
            Ok(ConversationBranchRecord {
                branch_id: branch_id.clone(),
                session_id,
                parent_branch_id,
                parent_message_id,
                origin_message_id,
                head_message_id,
                label,
                metadata_json: parse_json_record(&metadata_json)?,
                created_at,
                updated_at,
                version,
            })
        },
    )
    .transpose()?
    .ok_or_else(|| {
        CoreError::new(
            CoreErrorKind::NotFound,
            format!("conversation branch {branch_id} not found"),
        )
    })
}

fn current_active_branch_in_tx(
    tx: &rusqlite::Transaction<'_>,
    session_id: &SessionId,
) -> CoreResult<Option<ConversationBranchId>> {
    tx.query_row(
        "SELECT active_branch_id FROM conversation_branch_state WHERE session_id = ?1",
        params![session_id.0],
        |row| row.get::<_, Option<String>>(0),
    )
    .optional()
    .map_err(|error| persistence_error("load current active branch", error))
    .map(|value| value.flatten().map(ConversationBranchId::new))
}

fn load_conversation_branch_state(
    conn: &Connection,
    session_id: &SessionId,
    default_updated_at: &IsoTimestamp,
) -> CoreResult<ConversationBranchStateRecord> {
    Ok(conn
        .query_row(
            "SELECT active_branch_id, updated_at, version
             FROM conversation_branch_state
             WHERE session_id = ?1",
            params![session_id.0],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?
                        .map(ConversationBranchId::new),
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)? as u64,
                ))
            },
        )
        .optional()
        .map_err(|error| persistence_error("load conversation branch state", error))?
        .map(
            |(active_branch_id, updated_at, version)| ConversationBranchStateRecord {
                session_id: session_id.clone(),
                active_branch_id,
                updated_at,
                version,
            },
        )
        .unwrap_or_else(|| ConversationBranchStateRecord {
            session_id: session_id.clone(),
            active_branch_id: None,
            updated_at: default_updated_at.clone(),
            version: 0,
        }))
}

fn load_conversation_branch_state_in_tx(
    tx: &rusqlite::Transaction<'_>,
    session_id: &SessionId,
    default_updated_at: &IsoTimestamp,
) -> CoreResult<ConversationBranchStateRecord> {
    Ok(tx
        .query_row(
            "SELECT active_branch_id, updated_at, version
             FROM conversation_branch_state
             WHERE session_id = ?1",
            params![session_id.0],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?
                        .map(ConversationBranchId::new),
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)? as u64,
                ))
            },
        )
        .optional()
        .map_err(|error| persistence_error("load conversation branch state", error))?
        .map(
            |(active_branch_id, updated_at, version)| ConversationBranchStateRecord {
                session_id: session_id.clone(),
                active_branch_id,
                updated_at,
                version,
            },
        )
        .unwrap_or_else(|| ConversationBranchStateRecord {
            session_id: session_id.clone(),
            active_branch_id: None,
            updated_at: default_updated_at.clone(),
            version: 0,
        }))
}

fn current_branch_head_in_tx(
    tx: &rusqlite::Transaction<'_>,
    branch_id: &ConversationBranchId,
) -> CoreResult<Option<MessageId>> {
    tx.query_row(
        "SELECT head_message_id FROM conversation_branches WHERE branch_id = ?1",
        params![branch_id.0],
        |row| row.get::<_, Option<String>>(0),
    )
    .optional()
    .map_err(|error| persistence_error("load current branch head", error))?
    .map(|value| value.map(MessageId::new))
    .ok_or_else(|| {
        CoreError::new(
            CoreErrorKind::NotFound,
            format!("conversation branch {branch_id} not found"),
        )
    })
}

fn ensure_branch_belongs_to_session_in_tx(
    tx: &rusqlite::Transaction<'_>,
    session_id: &SessionId,
    branch_id: &ConversationBranchId,
) -> CoreResult<()> {
    let exists: bool = tx
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM conversation_branches
                WHERE session_id = ?1 AND branch_id = ?2
             )",
            params![session_id.0, branch_id.0],
            |row| row.get(0),
        )
        .map_err(|error| persistence_error("check branch session ownership", error))?;
    if exists {
        Ok(())
    } else {
        Err(CoreError::new(
            CoreErrorKind::NotFound,
            format!("conversation branch {branch_id} not found for session {session_id}"),
        ))
    }
}

fn ensure_message_exists_in_tx(
    tx: &rusqlite::Transaction<'_>,
    message_id: &MessageId,
) -> CoreResult<()> {
    let exists: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM messages WHERE message_id = ?1)",
            params![message_id.0],
            |row| row.get(0),
        )
        .map_err(|error| persistence_error("check durable message existence", error))?;
    if exists {
        Ok(())
    } else {
        Err(CoreError::new(
            CoreErrorKind::NotFound,
            format!("message {message_id} not found"),
        ))
    }
}

fn save_conversation_snapshot_in_tx(
    tx: &rusqlite::Transaction<'_>,
    snapshot: &ConversationSnapshotWrite,
) -> CoreResult<()> {
    tx.execute(
        "INSERT INTO conversation_snapshots (
            snapshot_id,
            session_id,
            branch_id,
            message_id,
            cursor,
            label,
            summary,
            source,
            metadata_json,
            created_at,
            updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT(snapshot_id) DO UPDATE SET
            session_id = excluded.session_id,
            branch_id = excluded.branch_id,
            message_id = excluded.message_id,
            cursor = excluded.cursor,
            label = excluded.label,
            summary = excluded.summary,
            source = excluded.source,
            metadata_json = excluded.metadata_json,
            updated_at = excluded.updated_at",
        params![
            snapshot.snapshot_id.0,
            snapshot.session_id.0,
            snapshot.branch_id.as_ref().map(|value| value.0.as_str()),
            snapshot.message_id.as_ref().map(|value| value.0.as_str()),
            snapshot.cursor,
            snapshot.label,
            snapshot.summary,
            snapshot.source.as_str(),
            to_json_text(&snapshot.metadata_json)?,
            snapshot.created_at,
            snapshot.updated_at,
        ],
    )
    .map_err(|error| persistence_error("save conversation snapshot", error))?;
    Ok(())
}

fn query_conversation_snapshots(
    conn: &Connection,
    query: &ConversationSnapshotQuery,
) -> CoreResult<Vec<ConversationSnapshotRecord>> {
    let session_id = query.session_id.as_ref().map(|value| value.0.as_str());
    let branch_id = query.branch_id.as_ref().map(|value| value.0.as_str());
    let message_id = query.message_id.as_ref().map(|value| value.0.as_str());
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let mut stmt = conn
        .prepare(
            "SELECT snapshot_id
             FROM conversation_snapshots
             WHERE (?1 IS NULL OR session_id = ?1)
               AND (?2 IS NULL OR branch_id = ?2)
               AND (?3 IS NULL OR message_id = ?3)
             ORDER BY created_at ASC, snapshot_id ASC
             LIMIT ?4 OFFSET ?5",
        )
        .map_err(|error| persistence_error("prepare query conversation snapshots", error))?;
    let snapshot_ids = stmt
        .query_map(
            params![session_id, branch_id, message_id, limit, offset],
            |row| Ok(ConversationSnapshotId::new(row.get::<_, String>(0)?)),
        )
        .map_err(|error| persistence_error("query conversation snapshots", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load conversation snapshot ids", error))?;
    snapshot_ids
        .iter()
        .map(|snapshot_id| load_conversation_snapshot(conn, snapshot_id))
        .collect()
}

fn load_conversation_snapshot(
    conn: &Connection,
    snapshot_id: &ConversationSnapshotId,
) -> CoreResult<ConversationSnapshotRecord> {
    conn.query_row(
        "SELECT session_id, branch_id, message_id, cursor, label, summary,
                source, metadata_json, created_at, updated_at
         FROM conversation_snapshots
         WHERE snapshot_id = ?1",
        params![snapshot_id.0],
        |row| {
            Ok((
                SessionId::new(row.get::<_, String>(0)?),
                row.get::<_, Option<String>>(1)?
                    .map(ConversationBranchId::new),
                row.get::<_, Option<String>>(2)?.map(MessageId::new),
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
            ))
        },
    )
    .optional()
    .map_err(|error| persistence_error("load conversation snapshot", error))?
    .map(
        |(
            session_id,
            branch_id,
            message_id,
            cursor,
            label,
            summary,
            source,
            metadata_json,
            created_at,
            updated_at,
        )| {
            Ok(ConversationSnapshotRecord {
                snapshot_id: snapshot_id.clone(),
                session_id,
                branch_id,
                message_id,
                cursor,
                label,
                summary,
                source: ConversationSnapshotSource::parse(&source)?,
                metadata_json: parse_json_record(&metadata_json)?,
                created_at,
                updated_at,
            })
        },
    )
    .transpose()?
    .ok_or_else(|| {
        CoreError::new(
            CoreErrorKind::NotFound,
            format!("conversation snapshot {snapshot_id} not found"),
        )
    })
}

fn load_conversation_snapshot_in_tx(
    tx: &rusqlite::Transaction<'_>,
    snapshot_id: &ConversationSnapshotId,
) -> CoreResult<ConversationSnapshotRecord> {
    tx.query_row(
        "SELECT session_id, branch_id, message_id, cursor, label, summary,
                source, metadata_json, created_at, updated_at
         FROM conversation_snapshots
         WHERE snapshot_id = ?1",
        params![snapshot_id.0],
        |row| {
            Ok((
                SessionId::new(row.get::<_, String>(0)?),
                row.get::<_, Option<String>>(1)?
                    .map(ConversationBranchId::new),
                row.get::<_, Option<String>>(2)?.map(MessageId::new),
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
            ))
        },
    )
    .optional()
    .map_err(|error| persistence_error("load conversation snapshot in tx", error))?
    .map(
        |(
            session_id,
            branch_id,
            message_id,
            cursor,
            label,
            summary,
            source,
            metadata_json,
            created_at,
            updated_at,
        )| {
            Ok(ConversationSnapshotRecord {
                snapshot_id: snapshot_id.clone(),
                session_id,
                branch_id,
                message_id,
                cursor,
                label,
                summary,
                source: ConversationSnapshotSource::parse(&source)?,
                metadata_json: parse_json_record(&metadata_json)?,
                created_at,
                updated_at,
            })
        },
    )
    .transpose()?
    .ok_or_else(|| {
        CoreError::new(
            CoreErrorKind::NotFound,
            format!("conversation snapshot {snapshot_id} not found"),
        )
    })
}

fn resolve_conversation_jump(
    conn: &Connection,
    request: &ConversationJumpRequest,
) -> CoreResult<ConversationJumpResult> {
    match &request.target {
        ConversationJumpTarget::Message { message_id } => {
            let message = load_durable_message(conn, message_id)?;
            if message.session_id != request.session_id {
                return Err(CoreError::new(
                    CoreErrorKind::NotFound,
                    format!(
                        "message {message_id} not found for session {}",
                        request.session_id
                    ),
                ));
            }
            Ok(ConversationJumpResult {
                session_id: request.session_id.clone(),
                target: request.target.clone(),
                branch_id: message.branch_id,
                message_id: Some(message_id.clone()),
                cursor: None,
                snapshot_id: None,
            })
        }
        ConversationJumpTarget::Branch { branch_id } => {
            let branch = load_conversation_branch(conn, branch_id)?;
            if branch.session_id != request.session_id {
                return Err(CoreError::new(
                    CoreErrorKind::NotFound,
                    format!(
                        "branch {branch_id} not found for session {}",
                        request.session_id
                    ),
                ));
            }
            Ok(ConversationJumpResult {
                session_id: request.session_id.clone(),
                target: request.target.clone(),
                branch_id: Some(branch.branch_id),
                message_id: branch
                    .head_message_id
                    .or(branch.origin_message_id)
                    .or(branch.parent_message_id),
                cursor: None,
                snapshot_id: None,
            })
        }
        ConversationJumpTarget::Snapshot { snapshot_id } => {
            let snapshot = load_conversation_snapshot(conn, snapshot_id)?;
            if snapshot.session_id != request.session_id {
                return Err(CoreError::new(
                    CoreErrorKind::NotFound,
                    format!(
                        "snapshot {snapshot_id} not found for session {}",
                        request.session_id
                    ),
                ));
            }
            Ok(ConversationJumpResult {
                session_id: request.session_id.clone(),
                target: request.target.clone(),
                branch_id: snapshot.branch_id,
                message_id: snapshot.message_id,
                cursor: snapshot.cursor,
                snapshot_id: Some(snapshot.snapshot_id),
            })
        }
        ConversationJumpTarget::Cursor { cursor } => Ok(ConversationJumpResult {
            session_id: request.session_id.clone(),
            target: request.target.clone(),
            branch_id: None,
            message_id: None,
            cursor: Some(cursor.clone()),
            snapshot_id: None,
        }),
    }
}

type AttachmentRow = (
    SessionId,
    String,
    String,
    String,
    i64,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    bool,
    String,
    String,
    String,
    Option<String>,
);

fn save_attachment_in_tx(
    tx: &rusqlite::Transaction<'_>,
    attachment: &AttachmentWrite,
) -> CoreResult<()> {
    tx.execute(
        "INSERT INTO attachments (
            attachment_id,
            session_id,
            status,
            filename,
            mime_type,
            byte_size,
            storage_url,
            download_url,
            thumbnail_url,
            extracted_text,
            extracted_text_truncated,
            metadata_json,
            created_at,
            updated_at,
            expires_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
         ON CONFLICT(attachment_id) DO UPDATE SET
            session_id = excluded.session_id,
            status = excluded.status,
            filename = excluded.filename,
            mime_type = excluded.mime_type,
            byte_size = excluded.byte_size,
            storage_url = excluded.storage_url,
            download_url = excluded.download_url,
            thumbnail_url = excluded.thumbnail_url,
            extracted_text = excluded.extracted_text,
            extracted_text_truncated = excluded.extracted_text_truncated,
            metadata_json = excluded.metadata_json,
            updated_at = excluded.updated_at,
            expires_at = excluded.expires_at",
        params![
            attachment.attachment_id.0,
            attachment.session_id.0,
            attachment.status.as_str(),
            attachment.filename,
            attachment.mime_type,
            attachment.byte_size as i64,
            attachment.storage_url,
            attachment.download_url,
            attachment.thumbnail_url,
            attachment.extracted_text,
            attachment.extracted_text_truncated,
            to_json_text(&attachment.metadata_json)?,
            attachment.created_at,
            attachment.updated_at,
            attachment.expires_at,
        ],
    )
    .map_err(|error| persistence_error("save attachment", error))?;
    if let Some(link) = &attachment.link {
        save_attachment_link_in_tx(tx, link)?;
    }
    Ok(())
}

fn save_attachment_link_in_tx(
    tx: &rusqlite::Transaction<'_>,
    link: &AttachmentLinkWrite,
) -> CoreResult<()> {
    tx.execute(
        "INSERT INTO attachment_links (
            link_id,
            attachment_id,
            session_id,
            message_id,
            block_id,
            scope_id,
            metadata_json,
            created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(link_id) DO UPDATE SET
            attachment_id = excluded.attachment_id,
            session_id = excluded.session_id,
            message_id = excluded.message_id,
            block_id = excluded.block_id,
            scope_id = excluded.scope_id,
            metadata_json = excluded.metadata_json",
        params![
            link.link_id.0,
            link.attachment_id.0,
            link.session_id.0,
            link.message_id.as_ref().map(|value| value.0.as_str()),
            link.block_id.as_ref().map(|value| value.0.as_str()),
            link.scope_id.as_ref().map(|value| value.0.as_str()),
            to_json_text(&link.metadata_json)?,
            link.created_at,
        ],
    )
    .map_err(|error| persistence_error("save attachment link", error))?;
    Ok(())
}

fn query_attachments(
    conn: &Connection,
    query: &AttachmentQuery,
) -> CoreResult<Vec<AttachmentRecord>> {
    let session_id = query.session_id.as_ref().map(|value| value.0.as_str());
    let message_id = query.message_id.as_ref().map(|value| value.0.as_str());
    let scope_id = query.scope_id.as_ref().map(|value| value.0.as_str());
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT a.attachment_id
             FROM attachments a
             LEFT JOIN attachment_links l ON l.attachment_id = a.attachment_id
             WHERE (?1 IS NULL OR a.session_id = ?1)
               AND (?2 OR a.status <> 'removed')
               AND (?3 IS NULL OR l.message_id = ?3)
               AND (?4 IS NULL OR l.scope_id = ?4)
             ORDER BY a.created_at ASC, a.attachment_id ASC
             LIMIT ?5 OFFSET ?6",
        )
        .map_err(|error| persistence_error("prepare query attachments", error))?;
    let attachment_ids = stmt
        .query_map(
            params![
                session_id,
                query.include_removed,
                message_id,
                scope_id,
                limit,
                offset,
            ],
            |row| Ok(AttachmentId::new(row.get::<_, String>(0)?)),
        )
        .map_err(|error| persistence_error("query attachments", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load attachment ids", error))?;
    attachment_ids
        .iter()
        .map(|attachment_id| load_attachment(conn, attachment_id))
        .collect()
}

fn load_attachment(
    conn: &Connection,
    attachment_id: &AttachmentId,
) -> CoreResult<AttachmentRecord> {
    let record = conn
        .query_row(
            "SELECT session_id, status, filename, mime_type, byte_size,
                    storage_url, download_url, thumbnail_url, extracted_text,
                    extracted_text_truncated, metadata_json, created_at, updated_at, expires_at
             FROM attachments
             WHERE attachment_id = ?1",
            params![attachment_id.0],
            |row| {
                Ok((
                    SessionId::new(row.get::<_, String>(0)?),
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, bool>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, String>(11)?,
                    row.get::<_, String>(12)?,
                    row.get::<_, Option<String>>(13)?,
                ))
            },
        )
        .optional()
        .map_err(|error| persistence_error("load attachment", error))?;
    attachment_record_from_row(conn, attachment_id, record)
}

fn load_attachment_in_tx(
    tx: &rusqlite::Transaction<'_>,
    attachment_id: &AttachmentId,
) -> CoreResult<AttachmentRecord> {
    let record = tx
        .query_row(
            "SELECT session_id, status, filename, mime_type, byte_size,
                    storage_url, download_url, thumbnail_url, extracted_text,
                    extracted_text_truncated, metadata_json, created_at, updated_at, expires_at
             FROM attachments
             WHERE attachment_id = ?1",
            params![attachment_id.0],
            |row| {
                Ok((
                    SessionId::new(row.get::<_, String>(0)?),
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, bool>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, String>(11)?,
                    row.get::<_, String>(12)?,
                    row.get::<_, Option<String>>(13)?,
                ))
            },
        )
        .optional()
        .map_err(|error| persistence_error("load attachment in tx", error))?;
    attachment_record_from_row(tx, attachment_id, record)
}

fn attachment_record_from_row(
    conn: &Connection,
    attachment_id: &AttachmentId,
    record: Option<AttachmentRow>,
) -> CoreResult<AttachmentRecord> {
    record
        .map(
            |(
                session_id,
                status,
                filename,
                mime_type,
                byte_size,
                storage_url,
                download_url,
                thumbnail_url,
                extracted_text,
                extracted_text_truncated,
                metadata_json,
                created_at,
                updated_at,
                expires_at,
            )| {
                Ok(AttachmentRecord {
                    attachment_id: attachment_id.clone(),
                    session_id,
                    status: AttachmentStatus::parse(&status)?,
                    filename,
                    mime_type,
                    byte_size: byte_size.max(0) as u64,
                    storage_url,
                    download_url,
                    thumbnail_url,
                    extracted_text,
                    extracted_text_truncated,
                    metadata_json: parse_json_record(&metadata_json)?,
                    created_at,
                    updated_at,
                    expires_at,
                    links: load_attachment_links(conn, attachment_id)?,
                })
            },
        )
        .transpose()?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("attachment {attachment_id} not found"),
            )
        })
}

fn load_attachment_links(
    conn: &Connection,
    attachment_id: &AttachmentId,
) -> CoreResult<Vec<AttachmentLinkRecord>> {
    let mut stmt = conn
        .prepare(
            "SELECT link_id, session_id, message_id, block_id, scope_id,
                    metadata_json, created_at
             FROM attachment_links
             WHERE attachment_id = ?1
             ORDER BY created_at ASC, link_id ASC",
        )
        .map_err(|error| persistence_error("prepare load attachment links", error))?;
    let links = stmt
        .query_map(params![attachment_id.0], |row| {
            Ok((
                AttachmentLinkId::new(row.get::<_, String>(0)?),
                SessionId::new(row.get::<_, String>(1)?),
                row.get::<_, Option<String>>(2)?.map(MessageId::new),
                row.get::<_, Option<String>>(3)?.map(MessageBlockId::new),
                row.get::<_, Option<String>>(4)?.map(DataBankScopeId::new),
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })
        .map_err(|error| persistence_error("query attachment links", error))?
        .map(|row| {
            let (link_id, session_id, message_id, block_id, scope_id, metadata_json, created_at) =
                row.map_err(|error| persistence_error("load attachment link", error))?;
            Ok(AttachmentLinkRecord {
                link_id,
                attachment_id: attachment_id.clone(),
                session_id,
                message_id,
                block_id,
                scope_id,
                metadata_json: parse_json_record(&metadata_json)?,
                created_at,
            })
        })
        .collect::<CoreResult<Vec<_>>>()?;
    Ok(links)
}

fn save_data_bank_scope_in_tx(
    tx: &rusqlite::Transaction<'_>,
    scope: &DataBankScopeWrite,
) -> CoreResult<()> {
    tx.execute(
        "INSERT INTO data_bank_scopes (
            scope_id,
            session_id,
            status,
            label,
            description,
            metadata_json,
            created_at,
            updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(scope_id) DO UPDATE SET
            session_id = excluded.session_id,
            status = excluded.status,
            label = excluded.label,
            description = excluded.description,
            metadata_json = excluded.metadata_json,
            updated_at = excluded.updated_at",
        params![
            scope.scope_id.0,
            scope.session_id.0,
            scope.status.as_str(),
            scope.label,
            scope.description,
            to_json_text(&scope.metadata_json)?,
            scope.created_at,
            scope.updated_at,
        ],
    )
    .map_err(|error| persistence_error("save data-bank scope", error))?;
    Ok(())
}

fn query_data_bank_scopes(
    conn: &Connection,
    query: &DataBankScopeQuery,
) -> CoreResult<Vec<DataBankScopeRecord>> {
    let session_id = query.session_id.as_ref().map(|value| value.0.as_str());
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let mut stmt = conn
        .prepare(
            "SELECT scope_id
             FROM data_bank_scopes
             WHERE (?1 IS NULL OR session_id = ?1)
               AND (?2 OR status <> 'removed')
             ORDER BY created_at ASC, scope_id ASC
             LIMIT ?3 OFFSET ?4",
        )
        .map_err(|error| persistence_error("prepare query data-bank scopes", error))?;
    let scope_ids = stmt
        .query_map(
            params![session_id, query.include_removed, limit, offset],
            |row| Ok(DataBankScopeId::new(row.get::<_, String>(0)?)),
        )
        .map_err(|error| persistence_error("query data-bank scopes", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load data-bank scope ids", error))?;
    scope_ids
        .iter()
        .map(|scope_id| load_data_bank_scope(conn, scope_id))
        .collect()
}

fn load_data_bank_scope(
    conn: &Connection,
    scope_id: &DataBankScopeId,
) -> CoreResult<DataBankScopeRecord> {
    conn.query_row(
        "SELECT session_id, status, label, description, metadata_json,
                created_at, updated_at
         FROM data_bank_scopes
         WHERE scope_id = ?1",
        params![scope_id.0],
        |row| {
            Ok((
                SessionId::new(row.get::<_, String>(0)?),
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        },
    )
    .optional()
    .map_err(|error| persistence_error("load data-bank scope", error))?
    .map(
        |(session_id, status, label, description, metadata_json, created_at, updated_at)| {
            Ok(DataBankScopeRecord {
                scope_id: scope_id.clone(),
                session_id,
                status: DataBankScopeStatus::parse(&status)?,
                label,
                description,
                metadata_json: parse_json_record(&metadata_json)?,
                created_at,
                updated_at,
            })
        },
    )
    .transpose()?
    .ok_or_else(|| {
        CoreError::new(
            CoreErrorKind::NotFound,
            format!("data-bank scope {scope_id} not found"),
        )
    })
}

fn load_data_bank_scope_in_tx(
    tx: &rusqlite::Transaction<'_>,
    scope_id: &DataBankScopeId,
) -> CoreResult<DataBankScopeRecord> {
    tx.query_row(
        "SELECT session_id, status, label, description, metadata_json,
                created_at, updated_at
         FROM data_bank_scopes
         WHERE scope_id = ?1",
        params![scope_id.0],
        |row| {
            Ok((
                SessionId::new(row.get::<_, String>(0)?),
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        },
    )
    .optional()
    .map_err(|error| persistence_error("load data-bank scope in tx", error))?
    .map(
        |(session_id, status, label, description, metadata_json, created_at, updated_at)| {
            Ok(DataBankScopeRecord {
                scope_id: scope_id.clone(),
                session_id,
                status: DataBankScopeStatus::parse(&status)?,
                label,
                description,
                metadata_json: parse_json_record(&metadata_json)?,
                created_at,
                updated_at,
            })
        },
    )
    .transpose()?
    .ok_or_else(|| {
        CoreError::new(
            CoreErrorKind::NotFound,
            format!("data-bank scope {scope_id} not found"),
        )
    })
}

fn load_message_slot(
    conn: &Connection,
    slot_id: &MessageSlotId,
    include_alternates: bool,
) -> CoreResult<MessageSlotRecord> {
    let (session_id, primary_variant_id, active_variant_id, metadata_json, created_at, updated_at, version) =
        conn.query_row(
            "SELECT session_id, primary_variant_id, active_variant_id, metadata_json, created_at, updated_at, version
             FROM message_slots
             WHERE slot_id = ?1",
            params![slot_id.0],
            |row| {
                Ok((
                    SessionId::new(row.get::<_, String>(0)?),
                    MessageVariantId::new(row.get::<_, String>(1)?),
                    row.get::<_, Option<String>>(2)?.map(MessageVariantId::new),
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, i64>(6)? as u64,
                ))
            },
        )
        .optional()
        .map_err(|error| persistence_error("load message slot", error))?
        .ok_or_else(|| CoreError::new(CoreErrorKind::NotFound, format!("message slot {slot_id} not found")))?;
    let primary = load_message_variant(conn, &primary_variant_id)?;
    let alternates = if include_alternates {
        query_message_variants(
            conn,
            &MessageVariantQuery {
                slot_id: Some(slot_id.clone()),
                include_deleted: false,
                page: None,
            },
        )?
        .into_iter()
        .filter(|variant| variant.source == MessageVariantSource::Alternate)
        .collect()
    } else {
        Vec::new()
    };
    Ok(MessageSlotRecord {
        slot_id: slot_id.clone(),
        session_id,
        primary_variant_id,
        active_variant_id,
        metadata_json: parse_json_record(&metadata_json)?,
        created_at,
        updated_at,
        version,
        primary,
        alternates,
    })
}

fn load_message_slot_in_tx(
    tx: &rusqlite::Transaction<'_>,
    slot_id: &MessageSlotId,
    include_alternates: bool,
) -> CoreResult<MessageSlotRecord> {
    let (session_id, primary_variant_id, active_variant_id, metadata_json, created_at, updated_at, version) =
        tx.query_row(
            "SELECT session_id, primary_variant_id, active_variant_id, metadata_json, created_at, updated_at, version
             FROM message_slots
             WHERE slot_id = ?1",
            params![slot_id.0],
            |row| {
                Ok((
                    SessionId::new(row.get::<_, String>(0)?),
                    MessageVariantId::new(row.get::<_, String>(1)?),
                    row.get::<_, Option<String>>(2)?.map(MessageVariantId::new),
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, i64>(6)? as u64,
                ))
            },
        )
        .optional()
        .map_err(|error| persistence_error("load message slot in tx", error))?
        .ok_or_else(|| CoreError::new(CoreErrorKind::NotFound, format!("message slot {slot_id} not found")))?;
    let primary = load_message_variant_in_tx(tx, &primary_variant_id)?;
    let alternates = if include_alternates {
        query_message_variants_in_tx(
            tx,
            &MessageVariantQuery {
                slot_id: Some(slot_id.clone()),
                include_deleted: false,
                page: None,
            },
        )?
        .into_iter()
        .filter(|variant| variant.source == MessageVariantSource::Alternate)
        .collect()
    } else {
        Vec::new()
    };
    Ok(MessageSlotRecord {
        slot_id: slot_id.clone(),
        session_id,
        primary_variant_id,
        active_variant_id,
        metadata_json: parse_json_record(&metadata_json)?,
        created_at,
        updated_at,
        version,
        primary,
        alternates,
    })
}

fn load_message_variant(
    conn: &Connection,
    variant_id: &MessageVariantId,
) -> CoreResult<MessageVariantRecord> {
    let row = conn
        .query_row(
            "SELECT slot_id, source, ordinal, status, message_id, metadata_json, created_at, updated_at
             FROM message_variants
             WHERE variant_id = ?1",
            params![variant_id.0],
            |row| {
                Ok((
                    MessageSlotId::new(row.get::<_, String>(0)?),
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)? as u32,
                    row.get::<_, String>(3)?,
                    MessageId::new(row.get::<_, String>(4)?),
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            },
        )
        .optional()
        .map_err(|error| persistence_error("load message variant", error))?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("message variant {variant_id} not found"),
            )
        })?;
    hydrate_message_variant(conn, variant_id, row)
}

fn load_message_variant_in_tx(
    tx: &rusqlite::Transaction<'_>,
    variant_id: &MessageVariantId,
) -> CoreResult<MessageVariantRecord> {
    let row = tx
        .query_row(
            "SELECT slot_id, source, ordinal, status, message_id, metadata_json, created_at, updated_at
             FROM message_variants
             WHERE variant_id = ?1",
            params![variant_id.0],
            |row| {
                Ok((
                    MessageSlotId::new(row.get::<_, String>(0)?),
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)? as u32,
                    row.get::<_, String>(3)?,
                    MessageId::new(row.get::<_, String>(4)?),
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            },
        )
        .optional()
        .map_err(|error| persistence_error("load message variant in tx", error))?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("message variant {variant_id} not found"),
            )
        })?;
    hydrate_message_variant_in_tx(tx, variant_id, row)
}

fn hydrate_message_variant(
    conn: &Connection,
    variant_id: &MessageVariantId,
    row: (
        MessageSlotId,
        String,
        u32,
        String,
        MessageId,
        String,
        IsoTimestamp,
        IsoTimestamp,
    ),
) -> CoreResult<MessageVariantRecord> {
    let (slot_id, source, ordinal, status, message_id, metadata_json, created_at, updated_at) = row;
    Ok(MessageVariantRecord {
        variant_id: variant_id.clone(),
        slot_id,
        source: MessageVariantSource::parse(&source)?,
        ordinal,
        status: MessageVariantStatus::parse(&status)?,
        message: load_durable_message(conn, &message_id)?,
        metadata_json: parse_json_record(&metadata_json)?,
        created_at,
        updated_at,
    })
}

fn hydrate_message_variant_in_tx(
    tx: &rusqlite::Transaction<'_>,
    variant_id: &MessageVariantId,
    row: (
        MessageSlotId,
        String,
        u32,
        String,
        MessageId,
        String,
        IsoTimestamp,
        IsoTimestamp,
    ),
) -> CoreResult<MessageVariantRecord> {
    let (slot_id, source, ordinal, status, message_id, metadata_json, created_at, updated_at) = row;
    Ok(MessageVariantRecord {
        variant_id: variant_id.clone(),
        slot_id,
        source: MessageVariantSource::parse(&source)?,
        ordinal,
        status: MessageVariantStatus::parse(&status)?,
        message: load_durable_message_in_tx(tx, &message_id)?,
        metadata_json: parse_json_record(&metadata_json)?,
        created_at,
        updated_at,
    })
}

fn load_durable_message(
    conn: &Connection,
    message_id: &MessageId,
) -> CoreResult<DurableMessageRecord> {
    let (
        session_id,
        branch_id,
        parent_message_id,
        previous_message_id,
        author_id,
        author_role,
        status,
        body,
        metadata_json,
        created_at,
    ) = conn
        .query_row(
            "SELECT session_id, branch_id, parent_message_id, previous_message_id,
                    author_id, author_role, status, body, metadata_json, created_at
             FROM messages
             WHERE message_id = ?1",
            params![message_id.0],
            |row| {
                Ok((
                    SessionId::new(row.get::<_, String>(0)?),
                    row.get::<_, Option<String>>(1)?
                        .map(ConversationBranchId::new),
                    row.get::<_, Option<String>>(2)?.map(MessageId::new),
                    row.get::<_, Option<String>>(3)?.map(MessageId::new),
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                ))
            },
        )
        .optional()
        .map_err(|error| persistence_error("load durable message", error))?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("message {message_id} not found"),
            )
        })?;
    Ok(DurableMessageRecord {
        message_id: message_id.clone(),
        session_id,
        branch_id,
        parent_message_id,
        previous_message_id,
        author_id,
        author_role,
        status: DurableMessageStatus::parse(&status)?,
        body,
        metadata_json: parse_json_record(&metadata_json)?,
        created_at,
        blocks: load_message_blocks(conn, message_id)?,
    })
}

fn load_durable_message_in_tx(
    tx: &rusqlite::Transaction<'_>,
    message_id: &MessageId,
) -> CoreResult<DurableMessageRecord> {
    let (
        session_id,
        branch_id,
        parent_message_id,
        previous_message_id,
        author_id,
        author_role,
        status,
        body,
        metadata_json,
        created_at,
    ) = tx
        .query_row(
            "SELECT session_id, branch_id, parent_message_id, previous_message_id,
                    author_id, author_role, status, body, metadata_json, created_at
             FROM messages
             WHERE message_id = ?1",
            params![message_id.0],
            |row| {
                Ok((
                    SessionId::new(row.get::<_, String>(0)?),
                    row.get::<_, Option<String>>(1)?
                        .map(ConversationBranchId::new),
                    row.get::<_, Option<String>>(2)?.map(MessageId::new),
                    row.get::<_, Option<String>>(3)?.map(MessageId::new),
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                ))
            },
        )
        .optional()
        .map_err(|error| persistence_error("load durable message in tx", error))?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("message {message_id} not found"),
            )
        })?;
    Ok(DurableMessageRecord {
        message_id: message_id.clone(),
        session_id,
        branch_id,
        parent_message_id,
        previous_message_id,
        author_id,
        author_role,
        status: DurableMessageStatus::parse(&status)?,
        body,
        metadata_json: parse_json_record(&metadata_json)?,
        created_at,
        blocks: load_message_blocks_in_tx(tx, message_id)?,
    })
}

fn load_message_blocks(
    conn: &Connection,
    message_id: &MessageId,
) -> CoreResult<Vec<MessageBlockRecord>> {
    let mut stmt = conn
        .prepare(
            "SELECT block_id, ordinal, kind, content_json, render_policy_json, metadata_json
             FROM message_blocks
             WHERE message_id = ?1
             ORDER BY ordinal ASC, block_id ASC",
        )
        .map_err(|error| persistence_error("prepare load message blocks", error))?;
    let rows = stmt
        .query_map(params![message_id.0], |row| {
            row_to_message_block(row, message_id)
        })
        .map_err(|error| persistence_error("query message blocks", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load message blocks", error))
}

fn load_message_blocks_in_tx(
    tx: &rusqlite::Transaction<'_>,
    message_id: &MessageId,
) -> CoreResult<Vec<MessageBlockRecord>> {
    let mut stmt = tx
        .prepare(
            "SELECT block_id, ordinal, kind, content_json, render_policy_json, metadata_json
             FROM message_blocks
             WHERE message_id = ?1
             ORDER BY ordinal ASC, block_id ASC",
        )
        .map_err(|error| persistence_error("prepare load message blocks in tx", error))?;
    let rows = stmt
        .query_map(params![message_id.0], |row| {
            row_to_message_block(row, message_id)
        })
        .map_err(|error| persistence_error("query message blocks in tx", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load message blocks in tx", error))
}

fn row_to_message_block(
    row: &rusqlite::Row<'_>,
    message_id: &MessageId,
) -> rusqlite::Result<MessageBlockRecord> {
    let content_json: String = row.get(3)?;
    let render_policy_json: Option<String> = row.get(4)?;
    let metadata_json: String = row.get(5)?;
    Ok(MessageBlockRecord {
        block_id: MessageBlockId::new(row.get::<_, String>(0)?),
        message_id: message_id.clone(),
        ordinal: row.get::<_, i64>(1)? as u32,
        kind: row.get(2)?,
        content_json: from_json_text(&content_json).map_err(to_sql_error)?,
        render_policy_json: render_policy_json
            .as_deref()
            .map(from_json_text)
            .transpose()
            .map_err(to_sql_error)?,
        metadata_json: from_json_text(&metadata_json).map_err(to_sql_error)?,
    })
}

fn current_active_variant_in_tx(
    tx: &rusqlite::Transaction<'_>,
    slot_id: &MessageSlotId,
) -> CoreResult<Option<MessageVariantId>> {
    tx.query_row(
        "SELECT active_variant_id FROM message_slots WHERE slot_id = ?1",
        params![slot_id.0],
        |row| row.get::<_, Option<String>>(0),
    )
    .optional()
    .map_err(|error| persistence_error("load active message variant", error))?
    .ok_or_else(|| {
        CoreError::new(
            CoreErrorKind::NotFound,
            format!("message slot {slot_id} not found"),
        )
    })
    .map(|value| value.map(MessageVariantId::new))
}

fn ensure_variant_belongs_to_slot_in_tx(
    tx: &rusqlite::Transaction<'_>,
    slot_id: &MessageSlotId,
    variant_id: &MessageVariantId,
) -> CoreResult<()> {
    let exists = tx
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM message_variants
                WHERE slot_id = ?1 AND variant_id = ?2 AND status <> 'deleted'
            )",
            params![slot_id.0, variant_id.0],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| persistence_error("check message variant slot", error))?
        != 0;
    if exists {
        Ok(())
    } else {
        Err(CoreError::new(
            CoreErrorKind::NotFound,
            format!("message variant {variant_id} not found in slot {slot_id}"),
        ))
    }
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
    let history_window_json = state
        .history_window
        .as_ref()
        .map(to_json_text)
        .transpose()?;
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
            last_active_at,
            history_window_json
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        ON CONFLICT(session_id) DO UPDATE SET
            handle = excluded.handle,
            agent_id = excluded.agent_id,
            profile_id = excluded.profile_id,
            kind_json = excluded.kind_json,
            delegation_json = excluded.delegation_json,
            resource_limits_json = excluded.resource_limits_json,
            tool_profile_json = excluded.tool_profile_json,
            history_window_json = excluded.history_window_json,
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
            history_window_json,
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

fn list_simple_kv(conn: &Connection, query: &SimpleKvQuery) -> CoreResult<Vec<SimpleKvRecord>> {
    validate_simple_kv_query(query)?;
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let now = query.now.as_deref();
    let key_prefix = query
        .key_prefix
        .as_ref()
        .map(|prefix| sqlite_like_prefix(prefix));
    let mut stmt = conn
        .prepare(
            "SELECT
                scope_type,
                scope_id,
                entry_key,
                value_json,
                revision,
                created_at,
                updated_at,
                expires_at
             FROM module_simple_kv_entries
             WHERE scope_type = ?1
               AND scope_id = ?2
               AND (?3 IS NULL OR entry_key LIKE ?3 ESCAPE '\\')
               AND (
                    (?4 AND expires_at IS NOT NULL AND ?5 IS NOT NULL AND expires_at <= ?5)
                    OR
                    (NOT ?4 AND (?6 OR expires_at IS NULL OR ?5 IS NULL OR expires_at > ?5))
               )
             ORDER BY entry_key ASC
             LIMIT ?7 OFFSET ?8",
        )
        .map_err(|error| persistence_error("prepare list simple kv", error))?;
    let rows = stmt
        .query_map(
            params![
                query.scope.scope_type.as_str(),
                query.scope.scope_id.as_str(),
                key_prefix.as_deref(),
                query.expired_only,
                now,
                query.include_expired,
                limit,
                offset
            ],
            row_to_simple_kv,
        )
        .map_err(|error| persistence_error("query simple kv", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load simple kv", error))
}

fn get_simple_kv(
    conn: &Connection,
    scope: &SimpleKvScope,
    key: &str,
    now: Option<&IsoTimestamp>,
) -> CoreResult<Option<SimpleKvRecord>> {
    validate_simple_kv_identity(scope, key)?;
    conn.query_row(
        "SELECT
            scope_type,
            scope_id,
            entry_key,
            value_json,
            revision,
            created_at,
            updated_at,
            expires_at
         FROM module_simple_kv_entries
         WHERE scope_type = ?1
           AND scope_id = ?2
           AND entry_key = ?3
           AND (expires_at IS NULL OR ?4 IS NULL OR expires_at > ?4)",
        params![scope.scope_type.as_str(), scope.scope_id.as_str(), key, now],
        row_to_simple_kv,
    )
    .optional()
    .map_err(|error| persistence_error("get simple kv", error))
}

fn put_simple_kv_in_tx(
    tx: &rusqlite::Transaction<'_>,
    write: &SimpleKvWrite,
) -> CoreResult<SimpleKvRecord> {
    let existing = get_simple_kv(tx, &write.scope, &write.key, None)?;
    match existing {
        Some(record) => update_simple_kv_in_tx(tx, write, record.revision + 1),
        None => insert_simple_kv_in_tx(tx, write),
    }
}

fn insert_simple_kv_in_tx(
    tx: &rusqlite::Transaction<'_>,
    write: &SimpleKvWrite,
) -> CoreResult<SimpleKvRecord> {
    let value_json = to_json_text(&write.value_json)?;
    tx.execute(
        "INSERT INTO module_simple_kv_entries (
            scope_type,
            scope_id,
            entry_key,
            value_json,
            revision,
            created_at,
            updated_at,
            expires_at
        ) VALUES (?1, ?2, ?3, ?4, 1, ?5, ?5, ?6)",
        params![
            write.scope.scope_type.as_str(),
            write.scope.scope_id.as_str(),
            write.key.as_str(),
            value_json,
            write.now.as_str(),
            write.expires_at.as_deref(),
        ],
    )
    .map_err(|error| persistence_error("insert simple kv", error))?;
    Ok(SimpleKvRecord {
        scope: write.scope.clone(),
        key: write.key.clone(),
        value_json: write.value_json.clone(),
        revision: 1,
        created_at: write.now.clone(),
        updated_at: write.now.clone(),
        expires_at: write.expires_at.clone(),
    })
}

fn update_simple_kv_in_tx(
    tx: &rusqlite::Transaction<'_>,
    write: &SimpleKvWrite,
    revision: u64,
) -> CoreResult<SimpleKvRecord> {
    let value_json = to_json_text(&write.value_json)?;
    let created_at = get_simple_kv(tx, &write.scope, &write.key, None)?
        .map(|record| record.created_at)
        .unwrap_or_else(|| write.now.clone());
    tx.execute(
        "UPDATE module_simple_kv_entries
         SET value_json = ?4,
             revision = ?5,
             updated_at = ?6,
             expires_at = ?7
         WHERE scope_type = ?1
           AND scope_id = ?2
           AND entry_key = ?3",
        params![
            write.scope.scope_type.as_str(),
            write.scope.scope_id.as_str(),
            write.key.as_str(),
            value_json,
            revision as i64,
            write.now.as_str(),
            write.expires_at.as_deref(),
        ],
    )
    .map_err(|error| persistence_error("update simple kv", error))?;
    Ok(SimpleKvRecord {
        scope: write.scope.clone(),
        key: write.key.clone(),
        value_json: write.value_json.clone(),
        revision,
        created_at,
        updated_at: write.now.clone(),
        expires_at: write.expires_at.clone(),
    })
}

fn expire_simple_kv(conn: &Connection, now: &IsoTimestamp) -> CoreResult<u64> {
    let changed = conn
        .execute(
            "DELETE FROM module_simple_kv_entries
             WHERE expires_at IS NOT NULL AND expires_at <= ?1",
            params![now.as_str()],
        )
        .map_err(|error| persistence_error("expire simple kv", error))?;
    Ok(changed as u64)
}

fn row_to_simple_kv(row: &rusqlite::Row<'_>) -> rusqlite::Result<SimpleKvRecord> {
    let value_json: String = row.get(3)?;
    let revision: i64 = row.get(4)?;
    if revision <= 0 {
        return Err(rusqlite::Error::FromSqlConversionFailure(
            4,
            rusqlite::types::Type::Integer,
            Box::new(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("invalid simple_kv revision {revision}"),
            )),
        ));
    }
    Ok(SimpleKvRecord {
        scope: SimpleKvScope {
            scope_type: row.get(0)?,
            scope_id: row.get(1)?,
        },
        key: row.get(2)?,
        value_json: from_json_text(&value_json).map_err(to_sql_error)?,
        revision: revision as u64,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
        expires_at: row.get(7)?,
    })
}

fn query_profile_registry_records(
    conn: &Connection,
    query: &ProfileRegistryQuery,
) -> CoreResult<Vec<ProfileRegistryRecord>> {
    let lifecycle_status = query
        .lifecycle_status
        .as_ref()
        .map(profile_registry_lifecycle_status_as_str);
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
                lifecycle_status,
                display_name,
                summary,
                default_session_kind,
                agent_id,
                owner_id,
                active_runtime_settings_json,
                source_asset_refs_json,
                derived_runtime_refs_json,
                import_export_json,
                revision,
                created_at,
                updated_at
             FROM profile_registry
             WHERE (?1 IS NULL OR lifecycle_status = ?1)
             ORDER BY updated_at DESC, profile_id ASC
             LIMIT ?2 OFFSET ?3",
        )
        .map_err(|error| persistence_error("prepare query profile registry", error))?;
    let rows = stmt
        .query_map(
            params![lifecycle_status, limit, offset],
            row_to_profile_registry_record,
        )
        .map_err(|error| persistence_error("query profile registry", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load profile registry records", error))
}

fn get_profile_registry_record(
    conn: &Connection,
    profile_id: &ProfileId,
) -> CoreResult<Option<ProfileRegistryRecord>> {
    conn.query_row(
        "SELECT
            profile_id,
            lifecycle_status,
            display_name,
            summary,
            default_session_kind,
            agent_id,
            owner_id,
            active_runtime_settings_json,
            source_asset_refs_json,
            derived_runtime_refs_json,
            import_export_json,
            revision,
            created_at,
            updated_at
         FROM profile_registry
         WHERE profile_id = ?1",
        params![profile_id.0.as_str()],
        row_to_profile_registry_record,
    )
    .optional()
    .map_err(|error| persistence_error("get profile registry record", error))
}

fn insert_profile_registry_record_in_tx(
    tx: &rusqlite::Transaction<'_>,
    write: &ProfileRegistryWrite,
) -> CoreResult<()> {
    tx.execute(
        "INSERT INTO profile_registry (
            profile_id,
            lifecycle_status,
            display_name,
            summary,
            default_session_kind,
            agent_id,
            owner_id,
            active_runtime_settings_json,
            source_asset_refs_json,
            derived_runtime_refs_json,
            import_export_json,
            revision,
            created_at,
            updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 1, ?12, ?12)",
        params![
            write.profile_id.0.as_str(),
            profile_registry_lifecycle_status_as_str(&write.lifecycle_status),
            write.display_name.as_deref(),
            write.summary.as_deref(),
            write.default_session_kind.as_ref().map(session_kind_as_str),
            write.agent_id.as_ref().map(|value| value.0.as_str()),
            write.owner_id.as_deref(),
            to_json_text(&write.active_runtime_settings_json)?,
            to_json_text(&write.source_asset_refs)?,
            to_json_text(&write.derived_runtime_refs)?,
            to_json_text(&write.import_export)?,
            write.now.as_str(),
        ],
    )
    .map_err(|error| persistence_error("insert profile registry record", error))?;
    Ok(())
}

fn update_profile_registry_lifecycle_in_tx(
    tx: &rusqlite::Transaction<'_>,
    update: &ProfileRegistryLifecycleUpdate,
    revision: u64,
) -> CoreResult<()> {
    tx.execute(
        "UPDATE profile_registry
         SET lifecycle_status = ?2,
             revision = ?3,
             updated_at = ?4
         WHERE profile_id = ?1",
        params![
            update.profile_id.0.as_str(),
            profile_registry_lifecycle_status_as_str(&update.lifecycle_status),
            revision as i64,
            update.now.as_str(),
        ],
    )
    .map_err(|error| persistence_error("update profile registry lifecycle", error))?;
    Ok(())
}

fn row_to_profile_registry_record(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ProfileRegistryRecord> {
    let lifecycle_status: String = row.get(1)?;
    let default_session_kind: Option<String> = row.get(4)?;
    let active_runtime_settings_json: String = row.get(7)?;
    let source_asset_refs_json: String = row.get(8)?;
    let derived_runtime_refs_json: String = row.get(9)?;
    let import_export_json: String = row.get(10)?;
    Ok(ProfileRegistryRecord {
        profile_id: ProfileId::new(row.get::<_, String>(0)?),
        lifecycle_status: profile_registry_lifecycle_status_from_str(&lifecycle_status)?,
        display_name: row.get(2)?,
        summary: row.get(3)?,
        default_session_kind: default_session_kind
            .as_deref()
            .map(session_kind_from_str)
            .transpose()?,
        agent_id: row.get::<_, Option<String>>(5)?.map(AgentId::new),
        owner_id: row.get(6)?,
        active_runtime_settings_json: from_json_text(&active_runtime_settings_json)
            .map_err(to_sql_error)?,
        source_asset_refs: from_json_text(&source_asset_refs_json).map_err(to_sql_error)?,
        derived_runtime_refs: from_json_text(&derived_runtime_refs_json).map_err(to_sql_error)?,
        import_export: from_json_text(&import_export_json).map_err(to_sql_error)?,
        revision: row.get::<_, i64>(11)? as u64,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

fn profile_registry_lifecycle_status_as_str(
    status: &ProfileRegistryLifecycleStatus,
) -> &'static str {
    match status {
        ProfileRegistryLifecycleStatus::Active => "active",
        ProfileRegistryLifecycleStatus::Paused => "paused",
        ProfileRegistryLifecycleStatus::Decommissioned => "decommissioned",
        ProfileRegistryLifecycleStatus::Archived => "archived",
    }
}

fn profile_registry_lifecycle_status_from_str(
    raw: &str,
) -> rusqlite::Result<ProfileRegistryLifecycleStatus> {
    match raw {
        "active" => Ok(ProfileRegistryLifecycleStatus::Active),
        "paused" => Ok(ProfileRegistryLifecycleStatus::Paused),
        "decommissioned" => Ok(ProfileRegistryLifecycleStatus::Decommissioned),
        "archived" => Ok(ProfileRegistryLifecycleStatus::Archived),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown profile registry lifecycle status {other}"),
            )),
        )),
    }
}

fn validate_profile_registry_write(write: &ProfileRegistryWrite) -> CoreResult<()> {
    validate_profile_registry_id(&write.profile_id)?;
    validate_optional_short_text(
        "profile registry display_name",
        write.display_name.as_deref(),
    )?;
    validate_optional_short_text("profile registry summary", write.summary.as_deref())?;
    if let Some(agent_id) = &write.agent_id {
        validate_registry_id_text("profile registry agent_id", &agent_id.0)?;
    }
    validate_optional_short_text("profile registry owner_id", write.owner_id.as_deref())?;
    for asset in &write.source_asset_refs {
        validate_registry_id_text("profile registry source asset kind", &asset.asset_kind)?;
        if asset.path.trim().is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "profile registry source asset path must be non-empty",
            ));
        }
    }
    for derived in &write.derived_runtime_refs {
        validate_registry_id_text("profile registry derived ref kind", &derived.ref_kind)?;
        if derived.ref_id.trim().is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "profile registry derived runtime ref id must be non-empty",
            ));
        }
    }
    Ok(())
}

fn validate_profile_registry_id(profile_id: &ProfileId) -> CoreResult<()> {
    validate_registry_id_text("profile registry profile_id", &profile_id.0)
}

fn validate_registry_id_text(label: &str, value: &str) -> CoreResult<()> {
    if value.is_empty() || value.len() > 128 {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("{label} must be 1-128 characters"),
        ));
    }
    if !value.chars().all(|character| {
        character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || character == '-'
            || character == '_'
            || character == ':'
    }) {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("{label} must use lowercase ASCII id characters"),
        ));
    }
    Ok(())
}

fn validate_optional_short_text(label: &str, value: Option<&str>) -> CoreResult<()> {
    if let Some(value) = value {
        if value.len() > 512 {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!("{label} must be at most 512 bytes"),
            ));
        }
    }
    Ok(())
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

fn validate_memory_proposal(
    proposal: &MemoryProposalEnvelope,
    descriptor: &MemorySpaceDescriptor,
) -> CoreResult<()> {
    proposal.validate_for_descriptor(descriptor)?;
    if proposal.space_id.as_str() == "profile_dense" {
        validate_profile_dense_memory_proposal(proposal)?;
    }
    Ok(())
}

fn validate_profile_dense_memory_proposal(proposal: &MemoryProposalEnvelope) -> CoreResult<()> {
    let content = proposal.content.as_object().ok_or_else(|| {
        CoreError::new(
            CoreErrorKind::InvalidInput,
            "profile_dense proposal content must be an object",
        )
    })?;
    let key = content
        .get("key")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    validate_profile_memory_key(key, ProfileMemoryCaps::default().max_key_bytes)?;
    if matches!(
        proposal.operation,
        MemoryOperation::Add | MemoryOperation::Replace | MemoryOperation::CandidateOnly
    ) {
        let body = content
            .get("content")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        if body.trim().is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "profile_dense proposal content.content must be non-empty",
            ));
        }
        if body.len() > ProfileMemoryCaps::default().max_content_bytes as usize {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "profile_dense proposal content exceeds {} bytes",
                    ProfileMemoryCaps::default().max_content_bytes
                ),
            ));
        }
    }
    Ok(())
}

fn insert_memory_proposal_in_tx(
    tx: &rusqlite::Transaction<'_>,
    proposal: &MemoryProposalEnvelope,
    now: &IsoTimestamp,
) -> CoreResult<()> {
    let envelope_json = to_json_text(proposal)?;
    let status = MemoryProposalReviewStatus::PendingReview;
    let selected_governance_mode =
        selected_governance_mode(proposal.governance_mode, proposal.source);
    tx.execute(
        "INSERT INTO memory_proposals (
            proposal_id,
            space_id,
            operation,
            scope_type,
            scope_id,
            shape_id,
            shape_version,
            envelope_json,
            status,
            selected_governance_mode,
            source,
            dedupe_key,
            duplicate_of,
            resulting_revision,
            created_at,
            updated_at,
            decided_at,
            applied_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, NULL, NULL, ?13, ?13, NULL, NULL)",
        params![
            proposal.proposal_id.as_str(),
            proposal.space_id.0.as_str(),
            memory_operation_as_str(proposal.operation),
            memory_scope_type_as_str(proposal.scope.scope_type),
            proposal.scope.scope_id.as_str(),
            proposal.shape.shape_id.0.as_str(),
            proposal.shape.version as i64,
            envelope_json,
            memory_proposal_status_as_str(status),
            memory_governance_mode_as_str(selected_governance_mode),
            memory_proposal_source_as_str(proposal.source),
            proposal.dedupe_key.as_deref(),
            now.as_str(),
        ],
    )
    .map_err(|error| persistence_error("insert memory proposal", error))?;
    Ok(())
}

fn get_memory_proposal_by_id(
    conn: &Connection,
    proposal_id: &str,
) -> CoreResult<Option<MemoryProposalRecord>> {
    conn.query_row(
        "SELECT envelope_json,
                status,
                selected_governance_mode,
                created_at,
                updated_at,
                decided_at,
                applied_at,
                resulting_revision,
                duplicate_of
         FROM memory_proposals
         WHERE proposal_id = ?1",
        params![proposal_id],
        row_to_memory_proposal,
    )
    .optional()
    .map_err(|error| persistence_error("get memory proposal", error))
}

fn get_memory_proposal_by_dedupe(
    conn: &Connection,
    space_id: &str,
    dedupe_key: &str,
) -> CoreResult<Option<MemoryProposalRecord>> {
    conn.query_row(
        "SELECT envelope_json,
                status,
                selected_governance_mode,
                created_at,
                updated_at,
                decided_at,
                applied_at,
                resulting_revision,
                duplicate_of
         FROM memory_proposals
         WHERE space_id = ?1 AND dedupe_key = ?2",
        params![space_id, dedupe_key],
        row_to_memory_proposal,
    )
    .optional()
    .map_err(|error| persistence_error("get memory proposal by dedupe", error))
}

fn list_memory_proposals(
    conn: &Connection,
    query: &MemoryProposalQuery,
) -> CoreResult<Vec<MemoryProposalRecord>> {
    let (limit, offset) = QueryPage {
        limit: query.limit,
        offset: query.offset,
    }
    .bounded(100, 1_000);
    let status = query.status.map(memory_proposal_status_as_str);
    let mut stmt = conn
        .prepare(
            "SELECT envelope_json,
                    status,
                    selected_governance_mode,
                    created_at,
                    updated_at,
                    decided_at,
                    applied_at,
                    resulting_revision,
                    duplicate_of
             FROM memory_proposals
             WHERE (?1 IS NULL OR space_id = ?1)
               AND (?2 IS NULL OR status = ?2)
               AND (?3 IS NULL OR dedupe_key = ?3)
             ORDER BY updated_at DESC, proposal_id ASC
             LIMIT ?4 OFFSET ?5",
        )
        .map_err(|error| persistence_error("prepare list memory proposals", error))?;
    let rows = stmt
        .query_map(
            params![
                query.space_id.as_ref().map(|space_id| space_id.0.as_str()),
                status,
                query.dedupe_key.as_deref(),
                limit,
                offset,
            ],
            row_to_memory_proposal,
        )
        .map_err(|error| persistence_error("query memory proposals", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load memory proposals", error))
}

fn row_to_memory_proposal(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryProposalRecord> {
    let envelope_json: String = row.get(0)?;
    let status: String = row.get(1)?;
    let governance: String = row.get(2)?;
    let resulting_revision: Option<i64> = row.get(7)?;
    Ok(MemoryProposalRecord {
        proposal: from_json_text(&envelope_json).map_err(to_sql_error)?,
        status: parse_memory_proposal_status(&status).map_err(to_sql_core_error)?,
        selected_governance_mode: parse_memory_governance_mode(&governance)
            .map_err(to_sql_core_error)?,
        created_at: row.get(3)?,
        updated_at: row.get(4)?,
        decided_at: row.get(5)?,
        applied_at: row.get(6)?,
        resulting_revision: resulting_revision.map(|value| value as u64),
        duplicate_of: row.get(8)?,
    })
}

fn validate_memory_governance_decision(decision: &MemoryGovernanceDecisionInput) -> CoreResult<()> {
    validate_identifier("memory governance decision id", &decision.decision_id)?;
    validate_identifier("memory governance proposal id", &decision.proposal_id)?;
    if decision.actor.trim().is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "memory governance actor must not be empty",
        ));
    }
    if let Some(confidence) = decision.confidence {
        if !(0.0..=1.0).contains(&confidence) || confidence.is_nan() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "memory governance confidence must be between 0 and 1",
            ));
        }
    }
    Ok(())
}

fn validate_memory_governance_transition(
    current: MemoryProposalReviewStatus,
    decision: MemoryGovernanceDecisionKind,
) -> CoreResult<()> {
    let allowed = match (current, decision) {
        (_, MemoryGovernanceDecisionKind::RoutedToReview) => false,
        (MemoryProposalReviewStatus::PendingReview, MemoryGovernanceDecisionKind::Approved) => true,
        (MemoryProposalReviewStatus::PendingReview, MemoryGovernanceDecisionKind::Rejected) => true,
        (MemoryProposalReviewStatus::Approved, MemoryGovernanceDecisionKind::Applied) => true,
        _ => false,
    };
    if allowed {
        Ok(())
    } else {
        Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            format!(
                "memory governance decision {:?} is not allowed from {:?}",
                decision, current
            ),
        ))
    }
}

fn insert_memory_governance_decision_in_tx(
    tx: &rusqlite::Transaction<'_>,
    decision: &MemoryGovernanceDecisionInput,
) -> CoreResult<MemoryGovernanceDecisionRecord> {
    let decided_at = decision.decided_at.clone().ok_or_else(|| {
        CoreError::new(
            CoreErrorKind::InvalidInput,
            "memory governance decision requires decided_at",
        )
    })?;
    let evidence_refs_json = to_json_text(&decision.evidence_refs)?;
    tx.execute(
        "INSERT INTO memory_governance_decisions (
            decision_id,
            proposal_id,
            decision,
            actor,
            source,
            evidence_refs_json,
            policy_mode,
            confidence,
            message,
            resulting_revision,
            decided_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            decision.decision_id.as_str(),
            decision.proposal_id.as_str(),
            memory_governance_decision_as_str(decision.decision),
            decision.actor.as_str(),
            memory_proposal_source_as_str(decision.source),
            evidence_refs_json,
            memory_governance_mode_as_str(decision.policy_mode),
            decision.confidence.map(|value| value as f64),
            decision.message.as_deref(),
            decision.resulting_revision.map(|value| value as i64),
            decided_at.as_str(),
        ],
    )
    .map_err(|error| persistence_error("insert memory governance decision", error))?;
    Ok(MemoryGovernanceDecisionRecord {
        decision_id: decision.decision_id.clone(),
        proposal_id: decision.proposal_id.clone(),
        decision: decision.decision,
        actor: decision.actor.clone(),
        source: decision.source,
        evidence_refs: decision.evidence_refs.clone(),
        policy_mode: decision.policy_mode,
        confidence: decision.confidence,
        message: decision.message.clone(),
        resulting_revision: decision.resulting_revision,
        decided_at,
    })
}

fn update_memory_proposal_review_state_in_tx(
    tx: &rusqlite::Transaction<'_>,
    decision: &MemoryGovernanceDecisionRecord,
) -> CoreResult<()> {
    let next_status = match decision.decision {
        MemoryGovernanceDecisionKind::RoutedToReview => MemoryProposalReviewStatus::PendingReview,
        MemoryGovernanceDecisionKind::Approved => MemoryProposalReviewStatus::Approved,
        MemoryGovernanceDecisionKind::Rejected => MemoryProposalReviewStatus::Rejected,
        MemoryGovernanceDecisionKind::Applied => MemoryProposalReviewStatus::Applied,
    };
    tx.execute(
        "UPDATE memory_proposals
         SET status = ?2,
             updated_at = ?3,
             decided_at = CASE WHEN ?4 IS NULL THEN decided_at ELSE ?4 END,
             applied_at = CASE WHEN ?5 IS NULL THEN applied_at ELSE ?5 END,
             resulting_revision = CASE WHEN ?6 IS NULL THEN resulting_revision ELSE ?6 END
         WHERE proposal_id = ?1",
        params![
            decision.proposal_id.as_str(),
            memory_proposal_status_as_str(next_status),
            decision.decided_at.as_str(),
            if matches!(
                decision.decision,
                MemoryGovernanceDecisionKind::Approved | MemoryGovernanceDecisionKind::Rejected
            ) {
                Some(decision.decided_at.as_str())
            } else {
                None
            },
            if decision.decision == MemoryGovernanceDecisionKind::Applied {
                Some(decision.decided_at.as_str())
            } else {
                None
            },
            decision.resulting_revision.map(|value| value as i64),
        ],
    )
    .map_err(|error| persistence_error("update memory proposal review state", error))?;
    Ok(())
}

fn selected_governance_mode(
    requested: MemoryGovernanceMode,
    source: MemoryProposalSource,
) -> MemoryGovernanceMode {
    match (source, requested) {
        (
            MemoryProposalSource::InWakeTool | MemoryProposalSource::CaptureProducer,
            MemoryGovernanceMode::DirectWrite | MemoryGovernanceMode::AutoApplyThreshold,
        ) => MemoryGovernanceMode::CuratorRoute,
        _ => requested,
    }
}

fn memory_proposal_status_as_str(status: MemoryProposalReviewStatus) -> &'static str {
    match status {
        MemoryProposalReviewStatus::PendingReview => "pending_review",
        MemoryProposalReviewStatus::Approved => "approved",
        MemoryProposalReviewStatus::Rejected => "rejected",
        MemoryProposalReviewStatus::Applied => "applied",
    }
}

fn parse_memory_proposal_status(raw: &str) -> CoreResult<MemoryProposalReviewStatus> {
    match raw {
        "pending_review" => Ok(MemoryProposalReviewStatus::PendingReview),
        "approved" => Ok(MemoryProposalReviewStatus::Approved),
        "rejected" => Ok(MemoryProposalReviewStatus::Rejected),
        "applied" => Ok(MemoryProposalReviewStatus::Applied),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("invalid memory proposal status {other}"),
        )),
    }
}

fn memory_governance_decision_as_str(decision: MemoryGovernanceDecisionKind) -> &'static str {
    match decision {
        MemoryGovernanceDecisionKind::RoutedToReview => "routed_to_review",
        MemoryGovernanceDecisionKind::Approved => "approved",
        MemoryGovernanceDecisionKind::Rejected => "rejected",
        MemoryGovernanceDecisionKind::Applied => "applied",
    }
}

fn memory_governance_mode_as_str(mode: MemoryGovernanceMode) -> &'static str {
    match mode {
        MemoryGovernanceMode::ReadOnly => "read_only",
        MemoryGovernanceMode::DirectWrite => "direct_write",
        MemoryGovernanceMode::Candidate => "candidate",
        MemoryGovernanceMode::ManualReview => "manual_review",
        MemoryGovernanceMode::CuratorRoute => "curator_route",
        MemoryGovernanceMode::AutoApplyThreshold => "auto_apply_threshold",
    }
}

fn parse_memory_governance_mode(raw: &str) -> CoreResult<MemoryGovernanceMode> {
    match raw {
        "read_only" => Ok(MemoryGovernanceMode::ReadOnly),
        "direct_write" => Ok(MemoryGovernanceMode::DirectWrite),
        "candidate" => Ok(MemoryGovernanceMode::Candidate),
        "manual_review" => Ok(MemoryGovernanceMode::ManualReview),
        "curator_route" => Ok(MemoryGovernanceMode::CuratorRoute),
        "auto_apply_threshold" => Ok(MemoryGovernanceMode::AutoApplyThreshold),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("invalid memory governance mode {other}"),
        )),
    }
}

fn memory_operation_as_str(operation: MemoryOperation) -> &'static str {
    match operation {
        MemoryOperation::Read => "read",
        MemoryOperation::List => "list",
        MemoryOperation::Add => "add",
        MemoryOperation::Replace => "replace",
        MemoryOperation::Merge => "merge",
        MemoryOperation::Supersede => "supersede",
        MemoryOperation::Remove => "remove",
        MemoryOperation::Archive => "archive",
        MemoryOperation::CandidateOnly => "candidate_only",
    }
}

fn memory_scope_type_as_str(scope_type: MemoryScopeType) -> &'static str {
    match scope_type {
        MemoryScopeType::Profile => "profile",
        MemoryScopeType::User => "user",
        MemoryScopeType::Session => "session",
        MemoryScopeType::ConversationBranch => "conversation_branch",
        MemoryScopeType::World => "world",
        MemoryScopeType::Entity => "entity",
        MemoryScopeType::Project => "project",
    }
}

fn memory_proposal_source_as_str(source: MemoryProposalSource) -> &'static str {
    match source {
        MemoryProposalSource::InWakeTool => "in_wake_tool",
        MemoryProposalSource::CaptureProducer => "capture_producer",
        MemoryProposalSource::Ui => "ui",
        MemoryProposalSource::Import => "import",
        MemoryProposalSource::Migration => "migration",
        MemoryProposalSource::Human => "human",
        MemoryProposalSource::DenMemoryImport => "den_memory_import",
    }
}

fn validate_identifier(label: &str, value: &str) -> CoreResult<()> {
    if value.is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("{label} must not be empty"),
        ));
    }
    if value.len() > 64 {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("{label} must be at most 64 characters"),
        ));
    }
    let mut previous_underscore = false;
    for (index, ch) in value.chars().enumerate() {
        let valid = ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_';
        if !valid {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("{label} must use lowercase snake_case ASCII identifiers"),
            ));
        }
        if index == 0 && !ch.is_ascii_lowercase() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("{label} must start with a lowercase letter"),
            ));
        }
        if ch == '_' && (index == 0 || previous_underscore) {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("{label} must not contain leading or repeated underscores"),
            ));
        }
        previous_underscore = ch == '_';
    }
    if value.ends_with('_') {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("{label} must not end with an underscore"),
        ));
    }
    Ok(())
}

fn to_sql_core_error(error: CoreError) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}

fn validate_simple_kv_write(write: &SimpleKvWrite) -> CoreResult<()> {
    validate_simple_kv_identity(&write.scope, &write.key)
}

fn validate_simple_kv_query(query: &SimpleKvQuery) -> CoreResult<()> {
    validate_simple_kv_scope(&query.scope)?;
    if let Some(prefix) = &query.key_prefix {
        validate_simple_kv_part("key_prefix", prefix, 256)?;
    }
    if query.expired_only && query.now.is_none() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "simple_kv expired-only queries require now",
        ));
    }
    Ok(())
}

fn validate_simple_kv_identity(scope: &SimpleKvScope, key: &str) -> CoreResult<()> {
    validate_simple_kv_scope(scope)?;
    validate_simple_kv_part("key", key, 256)
}

fn validate_simple_kv_scope(scope: &SimpleKvScope) -> CoreResult<()> {
    validate_simple_kv_part("scope_type", &scope.scope_type, 64)?;
    validate_simple_kv_part("scope_id", &scope.scope_id, 256)
}

fn validate_simple_kv_part(label: &str, value: &str, max_bytes: usize) -> CoreResult<()> {
    if value.trim().is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("simple_kv {label} must be non-empty"),
        ));
    }
    if value.len() > max_bytes {
        return Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            format!("simple_kv {label} exceeds {max_bytes} bytes"),
        ));
    }
    if value.contains('\0') {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("simple_kv {label} must not contain NUL bytes"),
        ));
    }
    Ok(())
}

fn sqlite_like_prefix(prefix: &str) -> String {
    let mut escaped = String::new();
    for character in prefix.chars() {
        match character {
            '%' | '_' | '\\' => {
                escaped.push('\\');
                escaped.push(character);
            }
            _ => escaped.push(character),
        }
    }
    escaped.push('%');
    escaped
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

fn add_missing_column_tx(
    tx: &rusqlite::Transaction<'_>,
    table: &str,
    column: &str,
    definition: &str,
) -> CoreResult<()> {
    let mut stmt = tx
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| persistence_error("prepare table info in tx", error))?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| persistence_error("query table info in tx", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("read table info in tx", error))?;

    if columns.iter().any(|existing| existing == column) {
        return Ok(());
    }

    tx.execute(
        &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
        [],
    )
    .map_err(|error| persistence_error("add missing sqlite column in tx", error))?;
    Ok(())
}

fn to_json_text<T: Serialize>(value: &T) -> CoreResult<String> {
    serde_json::to_string(value)
        .map_err(|error| persistence_error("serialize coordination record", error))
}

fn from_json_text<T: DeserializeOwned>(value: &str) -> Result<T, serde_json::Error> {
    serde_json::from_str(value)
}

fn parse_json_record<T: DeserializeOwned>(value: &str) -> CoreResult<T> {
    from_json_text(value)
        .map_err(|error| persistence_error("deserialize coordination record", error))
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
    use rusty_crew_core_protocol::{
        AgentMessage, MemoryConflictPolicy, MemoryDiagnosticsPolicy, MemoryEvidenceKind,
        MemoryEvidenceRef, MemoryExportImportPolicy, MemoryFieldType, MemoryIndexingPolicy,
        MemoryOperationPolicy, MemoryPromptPolicy, MemoryProvenancePolicy,
        MemoryRecordFieldDescriptor, MemoryRecordShapeDescriptor, MemoryRecordShapeId,
        MemoryRecordShapeRef, MemoryRetentionPolicy, MemoryRetrievalStrategy, MemoryScope,
        MemoryScopeModel, MemorySpaceId, MemoryVisibilityModel, MemoryWritePolicy,
        ProfileRegistryDerivedRuntimeRef, ProfileRegistryImportExportMetadata,
        ProfileRegistrySourceAssetRef, ToolDescriptor,
    };
    use serde_json::json;
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    mod repository_conformance {
        use super::*;

        trait RepositoryConformanceBackend {
            fn with_store<F>(&self, label: &str, test: F)
            where
                F: FnOnce(&CoordinationStore);
        }

        struct SqliteRepositoryConformance;

        impl RepositoryConformanceBackend for SqliteRepositoryConformance {
            fn with_store<F>(&self, label: &str, test: F)
            where
                F: FnOnce(&CoordinationStore),
            {
                let db_path = temp_db_path(&format!("sqlite-conformance-{label}"));
                let store = CoordinationStore::open_file(&db_path).unwrap();
                test(&store);
                remove_temp_db(&db_path);
            }
        }

        #[test]
        fn sqlite_satisfies_repository_conformance_suite() {
            run_repository_conformance_suite(&SqliteRepositoryConformance);
        }

        fn run_repository_conformance_suite<B: RepositoryConformanceBackend>(backend: &B) {
            session_persistence_contract(backend);
            event_ordering_projection_contract(backend);
            queued_message_ttl_no_resurrection_contract(backend);
            scheduler_claim_and_expiry_contract(backend);
            runtime_counters_contract(backend);
            dense_profile_memory_revision_contract(backend);
            runtime_search_contract(backend);
            conversation_branch_message_contract(backend);
            provider_wire_state_expiry_contract(backend);
        }

        fn page() -> QueryPage {
            QueryPage {
                limit: Some(10),
                offset: Some(0),
            }
        }

        fn session_persistence_contract<B: RepositoryConformanceBackend>(backend: &B) {
            backend.with_store("session-persistence", |store| {
                let state = sample_session_state();
                let config = sample_session_config();
                store.save_session_with_config(&state, &config).unwrap();

                let sessions = store
                    .query_sessions(&SessionQuery {
                        agent_id: Some(AgentId::new("agent-alpha")),
                        profile_id: Some(ProfileId::new("full-profile")),
                        kind: Some(SessionKind::Full),
                        status: Some(SessionStatus::Idle),
                        page: Some(page()),
                    })
                    .unwrap();
                let configs = store.load_session_configs().unwrap();
                let identities = store.load_session_identities().unwrap();

                assert_eq!(sessions.len(), 1);
                assert_eq!(sessions[0].session_id, SessionId::new("session-alpha"));
                assert_eq!(configs.len(), 1);
                assert_eq!(
                    configs[0].config.resource_limits.max_duration_ms,
                    Some(60_000)
                );
                assert_eq!(configs[0].tool_profile.tools[0].name, "apply_patch");
                assert_eq!(identities.len(), 1);
                assert_eq!(
                    identities[0].instance_id,
                    AgentInstanceId::new("instance:session-alpha")
                );
            });
        }

        fn event_ordering_projection_contract<B: RepositoryConformanceBackend>(backend: &B) {
            backend.with_store("event-ordering-projections", |store| {
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
                                body: "projected conformance message".to_string(),
                                correlation_id: Some("conformance-corr".to_string()),
                            },
                        },
                    )
                    .unwrap();
                store
                    .save_event(
                        3,
                        &CoreEvent::BrainEventObserved {
                            session_id: session.session_id.clone(),
                            wake_id: Some("wake-conformance".to_string()),
                            event: BrainEvent::Started,
                        },
                    )
                    .unwrap();

                let all = store
                    .query_events(&RuntimeEventFilter {
                        limit: Some(10),
                        ..RuntimeEventFilter::default()
                    })
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
                        correlation_id: Some("conformance-corr".to_string()),
                        ..RuntimeEventFilter::default()
                    })
                    .unwrap();
                let by_wake = store
                    .query_events(&RuntimeEventFilter {
                        source_wake_id: Some("wake-conformance".to_string()),
                        ..RuntimeEventFilter::default()
                    })
                    .unwrap();

                assert_eq!(
                    all.iter().map(|event| event.sequence).collect::<Vec<_>>(),
                    vec![1, 2, 3]
                );
                assert_eq!(by_session.len(), 2);
                assert_eq!(by_agent.len(), 1);
                assert_eq!(by_agent[0].agent_ids.len(), 2);
                assert_eq!(by_correlation[0].sequence, 2);
                assert_eq!(by_wake[0].source_wake_ids, vec!["wake-conformance"]);
            });
        }

        fn queued_message_ttl_no_resurrection_contract<B: RepositoryConformanceBackend>(
            backend: &B,
        ) {
            backend.with_store("queue-ttl-no-resurrection", |store| {
                let record = QueuedMessageRecord {
                    message_id: "queue-conformance-1".to_string(),
                    owner_session_id: Some(SessionId::new("session-alpha")),
                    owner_agent_id: AgentId::new("agent-alpha"),
                    message: AgentMessage {
                        from: AgentId::new("operator"),
                        to: AgentId::new("agent-alpha"),
                        body: "ttl bounded conformance queue".to_string(),
                        correlation_id: Some("queue-conformance".to_string()),
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
                assert_eq!(pending_queue_messages(store).len(), 1);
                assert!(store
                    .expire_queued_messages_at(&"2026-06-20T00:00:04Z".to_string())
                    .unwrap()
                    .is_empty());
                assert_eq!(pending_queue_messages(store).len(), 1);

                let expired = store
                    .expire_queued_messages_at(&"2026-06-20T00:00:06Z".to_string())
                    .unwrap();
                assert_eq!(expired.len(), 1);
                assert_eq!(expired[0].state, QueuedMessageState::Expired);
                assert_eq!(expired[0].state_reason.as_deref(), Some("ttl_expired"));
                assert!(pending_queue_messages(store).is_empty());

                let expired_query = store
                    .load_queued_messages(&QueuedMessageFilter {
                        state: Some(QueuedMessageState::Expired),
                        owner_session_id: Some(SessionId::new("session-alpha")),
                        owner_agent_id: Some(AgentId::new("agent-alpha")),
                        limit: Some(10),
                    })
                    .unwrap();
                assert_eq!(expired_query.len(), 1);
                assert!(store
                    .expire_queued_messages_at(&"2026-06-20T00:00:10Z".to_string())
                    .unwrap()
                    .is_empty());
                assert!(pending_queue_messages(store).is_empty());
                assert_eq!(
                    store
                        .runtime_summary(&RuntimeCounterScope::Session(SessionId::new(
                            "session-alpha"
                        )))
                        .unwrap()
                        .queue_expirations,
                    1
                );
            });
        }

        fn pending_queue_messages(store: &CoordinationStore) -> Vec<QueuedMessageRecord> {
            store
                .load_queued_messages(&QueuedMessageFilter {
                    state: Some(QueuedMessageState::Pending),
                    owner_session_id: Some(SessionId::new("session-alpha")),
                    owner_agent_id: Some(AgentId::new("agent-alpha")),
                    limit: Some(10),
                })
                .unwrap()
        }

        fn scheduler_claim_and_expiry_contract<B: RepositoryConformanceBackend>(backend: &B) {
            backend.with_store("scheduler-claim-expiry", |store| {
                store
                    .upsert_scheduled_job(&ScheduledJobRecord {
                        job_id: "conformance-wake".to_string(),
                        job_kind: "wake".to_string(),
                        target_session_id: Some(SessionId::new("session-alpha")),
                        interval_ms: Some(60_000),
                        next_due_at: Some("2026-06-20T06:00:00Z".to_string()),
                        payload_json: json!({"reason": "conformance"}),
                        status: ScheduledJobStatus::Active,
                        created_at: "2026-06-20T05:59:00Z".to_string(),
                        updated_at: "2026-06-20T05:59:00Z".to_string(),
                        paused_at: None,
                    })
                    .unwrap();

                let due = store
                    .query_scheduled_jobs(&ScheduledJobQuery {
                        status: Some(ScheduledJobStatus::Active),
                        job_kind: Some("wake".to_string()),
                        due_at_or_before: Some("2026-06-20T06:00:00Z".to_string()),
                        page: Some(page()),
                    })
                    .unwrap();
                assert_eq!(due.len(), 1);

                let claimed = ScheduledRunRecord {
                    run_id: RunId::new("scheduled:conformance-wake:1"),
                    job_id: "conformance-wake".to_string(),
                    job_kind: "wake".to_string(),
                    target_session_id: Some(SessionId::new("session-alpha")),
                    status: ScheduledRunStatus::Claimed,
                    trigger: ScheduledRunTrigger::Due,
                    scheduled_for: Some("2026-06-20T06:00:00Z".to_string()),
                    claimed_at: "2026-06-20T06:00:01Z".to_string(),
                    claim_deadline_at: "2026-06-20T06:01:00Z".to_string(),
                    completed_at: None,
                    error: None,
                    output_json: json!({}),
                    created_at: "2026-06-20T06:00:01Z".to_string(),
                    updated_at: "2026-06-20T06:00:01Z".to_string(),
                };
                store
                    .claim_scheduled_run(&claimed, Some(&"2026-06-20T06:05:00Z".to_string()))
                    .unwrap();
                assert_eq!(
                    store
                        .load_scheduled_job("conformance-wake")
                        .unwrap()
                        .unwrap()
                        .next_due_at,
                    Some("2026-06-20T06:05:00Z".to_string())
                );
                store
                    .complete_scheduled_run(
                        &RunId::new("scheduled:conformance-wake:1"),
                        ScheduledRunStatus::Completed,
                        &"2026-06-20T06:00:30Z".to_string(),
                        &json!({"woke": true}),
                        None,
                    )
                    .unwrap();
                assert_eq!(
                    scheduled_runs(store, Some(ScheduledRunStatus::Completed)).len(),
                    1
                );

                let stale = ScheduledRunRecord {
                    run_id: RunId::new("scheduled:conformance-wake:2"),
                    job_id: "conformance-wake".to_string(),
                    job_kind: "wake".to_string(),
                    target_session_id: Some(SessionId::new("session-alpha")),
                    status: ScheduledRunStatus::Claimed,
                    trigger: ScheduledRunTrigger::Manual,
                    scheduled_for: None,
                    claimed_at: "2026-06-20T06:01:00Z".to_string(),
                    claim_deadline_at: "2026-06-20T06:02:00Z".to_string(),
                    completed_at: None,
                    error: None,
                    output_json: json!({}),
                    created_at: "2026-06-20T06:01:00Z".to_string(),
                    updated_at: "2026-06-20T06:01:00Z".to_string(),
                };
                store.claim_scheduled_run(&stale, None).unwrap();
                let expired = store
                    .expire_stale_scheduled_runs(
                        &"2026-06-20T06:02:01Z".to_string(),
                        &"2026-06-20T06:03:00Z".to_string(),
                    )
                    .unwrap();
                assert_eq!(expired.len(), 1);
                assert_eq!(
                    expired[0].run_id,
                    RunId::new("scheduled:conformance-wake:2")
                );
                assert_eq!(
                    scheduled_runs(store, Some(ScheduledRunStatus::Expired))[0]
                        .error
                        .as_deref(),
                    Some("claim deadline elapsed")
                );
            });
        }

        fn scheduled_runs(
            store: &CoordinationStore,
            status: Option<ScheduledRunStatus>,
        ) -> Vec<ScheduledRunRecord> {
            store
                .query_scheduled_runs(&ScheduledRunQuery {
                    job_id: Some("conformance-wake".to_string()),
                    status,
                    trigger: None,
                    target_session_id: None,
                    stale_claim_deadline_before: None,
                    page: Some(page()),
                })
                .unwrap()
        }

        fn runtime_counters_contract<B: RepositoryConformanceBackend>(backend: &B) {
            backend.with_store("runtime-counters", |store| {
                store
                    .save_event(
                        1,
                        &CoreEvent::BrainWakeRequested {
                            session_id: SessionId::new("session-alpha"),
                        },
                    )
                    .unwrap();
                store
                    .save_event(
                        2,
                        &CoreEvent::BrainActionsAccepted {
                            session_id: SessionId::new("session-alpha"),
                            count: 2,
                        },
                    )
                    .unwrap();
                store
                    .save_event(
                        3,
                        &CoreEvent::AgentMessageRouted {
                            message: AgentMessage {
                                from: AgentId::new("agent-alpha"),
                                to: AgentId::new("agent-beta"),
                                body: "counter conformance message".to_string(),
                                correlation_id: None,
                            },
                        },
                    )
                    .unwrap();

                let runtime = store
                    .runtime_summary(&RuntimeCounterScope::Runtime)
                    .unwrap();
                let session = store
                    .runtime_summary(&RuntimeCounterScope::Session(SessionId::new(
                        "session-alpha",
                    )))
                    .unwrap();
                let message_counter = store
                    .query_runtime_counters(&RuntimeCounterQuery {
                        scope: Some(RuntimeCounterScope::Runtime),
                        counter_name: Some(COUNTER_MESSAGES.to_string()),
                        page: Some(page()),
                    })
                    .unwrap();

                assert_eq!(runtime.wakes, 1);
                assert_eq!(runtime.brain_turns, 1);
                assert_eq!(runtime.messages, 1);
                assert_eq!(session.wakes, 1);
                assert_eq!(message_counter[0].value, 1);
            });
        }

        fn dense_profile_memory_revision_contract<B: RepositoryConformanceBackend>(backend: &B) {
            backend.with_store("profile-memory-revisions", |store| {
                let profile_id = ProfileId::new("profile-conformance");
                let target = ProfileMemoryTarget::Profile;
                let added = store
                    .add_profile_memory(
                        &ProfileMemoryWrite {
                            profile_id: profile_id.clone(),
                            target: target.clone(),
                            key: "tone".to_string(),
                            content: "prefers direct conformance checks".to_string(),
                            metadata: json!({"source": "test"}),
                            now: "2026-06-20T05:00:00Z".to_string(),
                        },
                        &ProfileMemoryCaps::default(),
                    )
                    .unwrap();
                assert_eq!(added.revision, 1);

                let replaced = store
                    .replace_profile_memory(
                        &ProfileMemoryReplace {
                            write: ProfileMemoryWrite {
                                profile_id: profile_id.clone(),
                                target: target.clone(),
                                key: "tone".to_string(),
                                content: "prefers backend-neutral repository checks".to_string(),
                                metadata: json!({"source": "replace"}),
                                now: "2026-06-20T05:01:00Z".to_string(),
                            },
                            expected_revision: 1,
                        },
                        &ProfileMemoryCaps::default(),
                    )
                    .unwrap();
                assert_eq!(replaced.revision, 2);
                assert!(store
                    .replace_profile_memory(
                        &ProfileMemoryReplace {
                            write: replaced_write("profile-conformance", target.clone(), "tone"),
                            expected_revision: 1,
                        },
                        &ProfileMemoryCaps::default(),
                    )
                    .is_err());
                assert_eq!(
                    store
                        .get_profile_memory(&profile_id, &target, "tone")
                        .unwrap()
                        .unwrap()
                        .content,
                    "prefers backend-neutral repository checks"
                );
                assert_eq!(
                    store
                        .list_profile_memory(&ProfileMemoryQuery {
                            profile_id,
                            target: Some(target),
                            page: Some(page()),
                        })
                        .unwrap()
                        .len(),
                    1
                );
            });
        }

        fn runtime_search_contract<B: RepositoryConformanceBackend>(backend: &B) {
            backend.with_store("runtime-search", |store| {
                store
                    .save_session_with_config(&sample_session_state(), &sample_session_config())
                    .unwrap();
                store
                    .save_event(
                        1,
                        &CoreEvent::AgentMessageRouted {
                            message: AgentMessage {
                                from: AgentId::new("agent-alpha"),
                                to: AgentId::new("agent-beta"),
                                body: "needle event search".to_string(),
                                correlation_id: Some("search-conformance".to_string()),
                            },
                        },
                    )
                    .unwrap();
                store
                    .save_queued_message(&QueuedMessageRecord {
                        message_id: "queue-search-conformance".to_string(),
                        owner_session_id: Some(SessionId::new("session-alpha")),
                        owner_agent_id: AgentId::new("agent-alpha"),
                        message: AgentMessage {
                            from: AgentId::new("operator"),
                            to: AgentId::new("agent-alpha"),
                            body: "needle queue search".to_string(),
                            correlation_id: None,
                        },
                        source_sequence: Some(1),
                        enqueued_at: "2026-06-20T00:00:00Z".to_string(),
                        expires_at: "2026-06-20T00:05:00Z".to_string(),
                        ttl_ms: 300_000,
                        delivery_attempts: 0,
                        state: QueuedMessageState::Pending,
                        terminal_at: None,
                        state_reason: None,
                    })
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
                        query: "needle".to_string(),
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
                let queued = store
                    .search_runtime(&RuntimeSearchFilter {
                        query: "needle".to_string(),
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

                assert_eq!(sessions.len(), 1);
                assert_eq!(sessions[0].row_type, RuntimeSearchRowType::Session);
                assert_eq!(messages.len(), 1);
                assert_eq!(messages[0].sequence, Some(1));
                assert_eq!(queued.len(), 1);
                assert_eq!(queued[0].row_key, "queue-search-conformance");
            });
        }

        fn conversation_branch_message_contract<B: RepositoryConformanceBackend>(backend: &B) {
            backend.with_store("conversation-branch-message", |store| {
                let now = "2026-06-25T04:00:00Z".to_string();
                let session_id = SessionId::new("session-1");
                let root_branch = ConversationBranchId::new("branch-conformance-root");
                let slot_id = MessageSlotId::new("slot-conformance");
                let primary_variant_id = MessageVariantId::new("variant-conformance-primary");
                let root_message_id = MessageId::new("message-conformance-root");
                store
                    .save_conversation_branch(&ConversationBranchWrite {
                        branch_id: root_branch.clone(),
                        session_id: session_id.clone(),
                        parent_branch_id: None,
                        parent_message_id: None,
                        origin_message_id: None,
                        head_message_id: Some(root_message_id.clone()),
                        label: Some("Root".to_string()),
                        metadata_json: json!({"kind": "conformance"}),
                        created_at: now.clone(),
                        updated_at: now.clone(),
                    })
                    .unwrap();
                store
                    .save_message_slot(&MessageSlotWrite {
                        slot_id: slot_id.clone(),
                        session_id: session_id.clone(),
                        primary_variant_id: primary_variant_id.clone(),
                        active_variant_id: None,
                        metadata_json: json!({"origin": "conformance"}),
                        created_at: now.clone(),
                        updated_at: now.clone(),
                    })
                    .unwrap();
                let mut variant = variant_write(
                    &slot_id,
                    &primary_variant_id,
                    MessageVariantSource::Primary,
                    0,
                    &root_message_id.0,
                    "root conformance body",
                );
                variant.message.branch_id = Some(root_branch.clone());
                store.save_message_variant(&variant).unwrap();

                let branches = store
                    .query_conversation_branches(&ConversationBranchQuery {
                        session_id: Some(session_id.clone()),
                        parent_branch_id: None,
                        page: Some(page()),
                    })
                    .unwrap();
                let slots = store
                    .query_message_slots(&MessageSlotQuery {
                        session_id: Some(session_id.clone()),
                        include_alternates: false,
                        page: Some(page()),
                    })
                    .unwrap();
                let selected = store
                    .select_active_conversation_branch(&SelectActiveBranchRequest {
                        session_id: session_id.clone(),
                        active_branch_id: Some(root_branch.clone()),
                        expected: ActiveBranchExpectation::None,
                        updated_at: "2026-06-25T04:01:00Z".to_string(),
                    })
                    .unwrap();
                let updated = store
                    .update_conversation_branch_head(&UpdateBranchHeadRequest {
                        branch_id: root_branch.clone(),
                        head_message_id: Some(root_message_id.clone()),
                        expected: BranchHeadExpectation::Message(root_message_id.clone()),
                        updated_at: "2026-06-25T04:02:00Z".to_string(),
                    })
                    .unwrap();
                let jump = store
                    .resolve_conversation_jump(&ConversationJumpRequest {
                        session_id,
                        target: ConversationJumpTarget::Message {
                            message_id: root_message_id.clone(),
                        },
                    })
                    .unwrap();

                assert_eq!(branches.len(), 1);
                assert_eq!(slots.len(), 1);
                assert_eq!(slots[0].primary.message.body, "root conformance body");
                assert!(selected.conflict.is_none());
                assert_eq!(selected.state.active_branch_id, Some(root_branch.clone()));
                assert!(updated.conflict.is_none());
                assert_eq!(jump.branch_id, Some(root_branch));
            });
        }

        fn provider_wire_state_expiry_contract<B: RepositoryConformanceBackend>(backend: &B) {
            backend.with_store("provider-wire-state-expiry", |store| {
                let key = sample_provider_wire_state_key();
                store
                    .save_provider_wire_state(&sample_provider_wire_state_write(
                        ProviderWireStateWriteFixture {
                            key: key.clone(),
                            profile_fingerprint: "profile:v1",
                            provider_fingerprint: "provider:v1",
                            payload_version: "responses:v1",
                            payload_json: json!({"response_id": "resp_conformance"}),
                            now: "2026-06-20T00:00:00Z",
                            expires_at: Some("2026-06-20T00:00:05Z"),
                            last_wake_id: Some("wake-conformance"),
                        },
                    ))
                    .unwrap();
                let current = store
                    .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
                        key: key.clone(),
                        profile_fingerprint: "profile:v1".to_string(),
                        provider_fingerprint: "provider:v1".to_string(),
                        now: "2026-06-20T00:00:04Z".to_string(),
                    })
                    .unwrap();
                assert!(current.record.unwrap().is_current());

                let expired_lookup = store
                    .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
                        key: key.clone(),
                        profile_fingerprint: "profile:v1".to_string(),
                        provider_fingerprint: "provider:v1".to_string(),
                        now: "2026-06-20T00:00:06Z".to_string(),
                    })
                    .unwrap();
                assert!(expired_lookup.record.is_none());
                assert_eq!(
                    expired_lookup.absence_reason,
                    Some(ProviderStateAbsenceReason::Expired)
                );

                store
                    .save_provider_wire_state(&sample_provider_wire_state_write(
                        ProviderWireStateWriteFixture {
                            key: key.clone(),
                            profile_fingerprint: "profile:v1",
                            provider_fingerprint: "provider:v1",
                            payload_version: "responses:v2",
                            payload_json: json!({"response_id": "resp_maintenance"}),
                            now: "2026-06-20T00:00:07Z",
                            expires_at: Some("2026-06-20T00:00:08Z"),
                            last_wake_id: Some("wake-maintenance"),
                        },
                    ))
                    .unwrap();
                let expired = store
                    .expire_provider_wire_states_at(&"2026-06-20T00:00:09Z".to_string())
                    .unwrap();
                assert_eq!(expired.len(), 1);
                assert_eq!(
                    expired[0].invalidation_reason,
                    Some(ProviderWireStateInvalidationReason::Expired)
                );
                assert!(store
                    .expire_provider_wire_states_at(&"2026-06-20T00:00:10Z".to_string())
                    .unwrap()
                    .is_empty());
            });
        }
    }

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
        assert!(table_exists(&db_path, "module_simple_kv_entries"));
        assert!(table_exists(&db_path, "profile_registry"));
        assert!(index_exists(
            &db_path,
            "idx_module_simple_kv_entries_scope_key"
        ));
        assert!(index_exists(&db_path, "idx_profile_registry_lifecycle"));
        assert!(index_exists(
            &db_path,
            "idx_module_simple_kv_entries_expires_at"
        ));
        let installed = store.installed_module_schemas().unwrap();
        assert_eq!(installed.len(), 1);
        assert_eq!(installed[0].module_id.as_str(), "simple_kv");
        assert_eq!(installed[0].installed_version, 1);

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
        assert!(table_exists(&db_path, "provider_wire_states"));
        assert!(table_exists(&db_path, "message_slots"));
        assert!(table_exists(&db_path, "message_variants"));
        assert!(table_exists(&db_path, "messages"));
        assert!(table_exists(&db_path, "message_blocks"));
        assert!(table_exists(&db_path, "channel_bindings"));
        assert!(table_exists(&db_path, "mcp_bindings"));
        assert!(table_exists(&db_path, "module_schema_versions"));
        assert!(table_exists(&db_path, "memory_proposals"));
        assert!(table_exists(&db_path, "memory_governance_decisions"));
        assert!(table_exists(&db_path, "profile_registry"));
        assert!(table_exists(&db_path, "module_simple_kv_entries"));
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
        assert!(index_exists(&db_path, "idx_provider_wire_states_current"));
        assert!(index_exists(&db_path, "idx_channel_bindings_external"));
        assert!(index_exists(&db_path, "idx_mcp_bindings_agent_profile"));
        assert!(index_exists(
            &db_path,
            "idx_module_simple_kv_entries_scope_key"
        ));
        assert!(index_exists(
            &db_path,
            "idx_module_simple_kv_entries_expires_at"
        ));
        assert!(index_exists(&db_path, "idx_memory_proposals_dedupe"));
        assert!(index_exists(
            &db_path,
            "idx_memory_governance_decisions_proposal"
        ));
        assert!(index_exists(&db_path, "idx_profile_registry_lifecycle"));

        remove_temp_db(&db_path);
    }

    #[test]
    fn module_schema_registry_tracks_fresh_install_and_existing_descriptor() {
        let db_path = temp_db_path("module-schema-fresh");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        let registry =
            ModuleSchemaRegistry::new(vec![simple_kv_schema_bundle(1).unwrap()]).unwrap();

        let installed = store
            .install_module_schema_registry(
                &registry,
                &[ModuleSchemaCapability::Transactions],
                &"2026-06-26T00:00:00Z".to_string(),
            )
            .unwrap();
        assert_eq!(installed.len(), 1);
        assert_eq!(installed[0].module_id.as_str(), "simple_kv");
        assert_eq!(installed[0].installed_version, 1);

        let second = store
            .install_module_schema_registry(
                &registry,
                &[ModuleSchemaCapability::Transactions],
                &"2026-06-26T00:01:00Z".to_string(),
            )
            .unwrap();
        assert_eq!(second, installed);
        assert_eq!(store.installed_module_schemas().unwrap(), installed);

        remove_temp_db(&db_path);
    }

    #[test]
    fn module_schema_registry_rejects_upgrade_without_migration_implementation() {
        let db_path = temp_db_path("module-schema-upgrade");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        let v1 = ModuleSchemaRegistry::new(vec![simple_kv_schema_bundle(1).unwrap()]).unwrap();
        let v2 = ModuleSchemaRegistry::new(vec![simple_kv_schema_bundle(2).unwrap()]).unwrap();

        store
            .install_module_schema_registry(
                &v1,
                &[ModuleSchemaCapability::Transactions],
                &"2026-06-26T00:00:00Z".to_string(),
            )
            .unwrap();
        let error = store
            .install_module_schema_registry(
                &v2,
                &[ModuleSchemaCapability::Transactions],
                &"2026-06-26T00:02:00Z".to_string(),
            )
            .unwrap_err();

        assert_eq!(error.kind, CoreErrorKind::PersistenceFailure);
        assert!(error.message.contains("no migration implementation"));

        remove_temp_db(&db_path);
    }

    #[test]
    fn module_schema_registry_rejects_same_version_fingerprint_change() {
        let db_path = temp_db_path("module-schema-fingerprint");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        let v1 = ModuleSchemaRegistry::new(vec![simple_kv_schema_bundle(1).unwrap()]).unwrap();
        let mut changed_bundle = simple_kv_schema_bundle(1).unwrap();
        changed_bundle.migration_notes = vec!["same version but changed descriptor".to_string()];
        let changed = ModuleSchemaRegistry::new(vec![changed_bundle]).unwrap();

        store
            .install_module_schema_registry(
                &v1,
                &[ModuleSchemaCapability::Transactions],
                &"2026-06-26T00:00:00Z".to_string(),
            )
            .unwrap();
        let error = store
            .install_module_schema_registry(
                &changed,
                &[ModuleSchemaCapability::Transactions],
                &"2026-06-26T00:01:00Z".to_string(),
            )
            .unwrap_err();

        assert_eq!(error.kind, CoreErrorKind::ActionRejected);
        assert!(error.message.contains("fingerprint changed"));

        remove_temp_db(&db_path);
    }

    #[test]
    fn module_schema_registry_rejects_missing_required_capability() {
        let db_path = temp_db_path("module-schema-capability");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        let registry =
            ModuleSchemaRegistry::new(vec![simple_kv_schema_bundle(1).unwrap()]).unwrap();

        let error = store
            .install_module_schema_registry(&registry, &[], &"2026-06-26T00:00:00Z".to_string())
            .unwrap_err();

        assert_eq!(error.kind, CoreErrorKind::InvalidInput);
        assert!(error
            .message
            .contains("requires unsupported storage capability"));

        remove_temp_db(&db_path);
    }

    #[test]
    fn module_schema_registry_rejects_invalid_installed_state() {
        let db_path = temp_db_path("module-schema-invalid-state");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute(
                "INSERT INTO module_schema_versions (
                    module_id,
                    installed_version,
                    descriptor_fingerprint,
                    installed_at,
                    updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?4)",
                params!["old_module", 0_i64, "bad", "2026-06-26T00:00:00Z"],
            )
            .unwrap();
        }

        let error = store.installed_module_schemas().unwrap_err();
        assert_eq!(error.kind, CoreErrorKind::PersistenceFailure);
        assert!(error
            .message
            .contains("invalid installed module schema version"));

        remove_temp_db(&db_path);
    }

    #[test]
    fn simple_kv_repository_round_trips_revisions_and_expiry() {
        let db_path = temp_db_path("simple-kv-repository");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        let scope = SimpleKvScope {
            scope_type: "profile".to_string(),
            scope_id: "rusty-crew-runner".to_string(),
        };

        let first = store
            .put_simple_kv(&SimpleKvWrite {
                scope: scope.clone(),
                key: "tone".to_string(),
                value_json: json!({"style": "steady"}),
                now: "2026-06-26T00:00:00Z".to_string(),
                expires_at: None,
            })
            .unwrap();
        assert_eq!(first.revision, 1);
        assert_eq!(first.value_json, json!({"style": "steady"}));

        let fetched = store
            .get_simple_kv(&scope, "tone", Some(&"2026-06-26T00:01:00Z".to_string()))
            .unwrap()
            .unwrap();
        assert_eq!(fetched, first);

        let second = store
            .put_simple_kv(&SimpleKvWrite {
                scope: scope.clone(),
                key: "tone".to_string(),
                value_json: json!({"style": "direct"}),
                now: "2026-06-26T00:02:00Z".to_string(),
                expires_at: Some("2026-06-26T01:00:00Z".to_string()),
            })
            .unwrap();
        assert_eq!(second.revision, 2);
        assert_eq!(second.created_at, first.created_at);
        assert_eq!(second.value_json, json!({"style": "direct"}));

        let stale = store
            .compare_and_swap_simple_kv(&SimpleKvCompareAndSwap {
                write: SimpleKvWrite {
                    scope: scope.clone(),
                    key: "tone".to_string(),
                    value_json: json!({"style": "stale"}),
                    now: "2026-06-26T00:03:00Z".to_string(),
                    expires_at: None,
                },
                expected_revision: 1,
            })
            .unwrap_err();
        assert_eq!(stale.kind, CoreErrorKind::ActionRejected);

        let third = store
            .compare_and_swap_simple_kv(&SimpleKvCompareAndSwap {
                write: SimpleKvWrite {
                    scope: scope.clone(),
                    key: "tone".to_string(),
                    value_json: json!({"style": "precise"}),
                    now: "2026-06-26T00:04:00Z".to_string(),
                    expires_at: Some("2026-06-26T00:05:00Z".to_string()),
                },
                expected_revision: 2,
            })
            .unwrap();
        assert_eq!(third.revision, 3);

        store
            .put_simple_kv(&SimpleKvWrite {
                scope: scope.clone(),
                key: "working_set".to_string(),
                value_json: json!(["a", "b"]),
                now: "2026-06-26T00:04:30Z".to_string(),
                expires_at: None,
            })
            .unwrap();

        let visible = store
            .list_simple_kv(&SimpleKvQuery {
                scope: scope.clone(),
                key_prefix: None,
                include_expired: false,
                expired_only: false,
                now: Some("2026-06-26T00:04:45Z".to_string()),
                page: None,
            })
            .unwrap();
        assert_eq!(
            visible
                .iter()
                .map(|record| record.key.as_str())
                .collect::<Vec<_>>(),
            vec!["tone", "working_set"]
        );
        let prefixed = store
            .list_simple_kv(&SimpleKvQuery {
                scope: scope.clone(),
                key_prefix: Some("work".to_string()),
                include_expired: false,
                expired_only: false,
                now: Some("2026-06-26T00:04:45Z".to_string()),
                page: None,
            })
            .unwrap();
        assert_eq!(prefixed.len(), 1);
        assert_eq!(prefixed[0].key, "working_set");

        assert!(store
            .get_simple_kv(&scope, "tone", Some(&"2026-06-26T00:05:01Z".to_string()))
            .unwrap()
            .is_none());
        let with_expired = store
            .list_simple_kv(&SimpleKvQuery {
                scope: scope.clone(),
                key_prefix: None,
                include_expired: true,
                expired_only: false,
                now: Some("2026-06-26T00:05:01Z".to_string()),
                page: None,
            })
            .unwrap();
        assert_eq!(with_expired.len(), 2);
        let expired_only = store
            .list_simple_kv(&SimpleKvQuery {
                scope: scope.clone(),
                key_prefix: None,
                include_expired: true,
                expired_only: true,
                now: Some("2026-06-26T00:05:01Z".to_string()),
                page: None,
            })
            .unwrap();
        assert_eq!(expired_only.len(), 1);
        assert_eq!(expired_only[0].key, "tone");

        assert_eq!(
            store
                .delete_simple_kv(&SimpleKvDelete {
                    scope: scope.clone(),
                    key: "working_set".to_string(),
                    expected_revision: 1,
                })
                .unwrap()
                .key,
            "working_set"
        );
        assert_eq!(
            store
                .expire_simple_kv(&"2026-06-26T00:05:01Z".to_string())
                .unwrap(),
            1
        );
        assert!(store
            .list_simple_kv(&SimpleKvQuery {
                scope,
                key_prefix: None,
                include_expired: true,
                expired_only: false,
                now: None,
                page: None,
            })
            .unwrap()
            .is_empty());

        remove_temp_db(&db_path);
    }

    #[test]
    fn storage_schema_diagnostics_project_installed_module_registry() {
        let db_path = temp_db_path("module-schema-diagnostics");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        let registry =
            ModuleSchemaRegistry::new(vec![simple_kv_schema_bundle(1).unwrap()]).unwrap();

        store
            .install_module_schema_registry(
                &registry,
                &[
                    ModuleSchemaCapability::Transactions,
                    ModuleSchemaCapability::JsonDocuments,
                ],
                &"2026-06-26T00:00:00Z".to_string(),
            )
            .unwrap();

        let diagnostics = store
            .storage_schema_for_registry(
                &registry,
                &[
                    ModuleSchemaCapability::Transactions,
                    ModuleSchemaCapability::JsonDocuments,
                ],
            )
            .unwrap();

        assert_eq!(diagnostics.modules.len(), 1);
        let module = &diagnostics.modules[0];
        assert_eq!(module.module_id, "simple_kv");
        assert_eq!(module.migration_status, "installed");
        assert_eq!(module.installed_version, Some(1));
        assert_eq!(module.logical_stores[0].store_name, "entries");
        assert_eq!(
            module.physical_tables[0].physical_table,
            "module_simple_kv_entries"
        );
        assert!(module.blocked_reasons.is_empty());
        assert!(module.degraded_reasons.is_empty());

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
    fn profile_registry_supports_lifecycle_revisions_and_asset_refs() {
        let db_path = temp_db_path("profile-registry");
        let store = CoordinationStore::open_file(&db_path).unwrap();

        let created = store
            .create_profile_registry_record(&profile_registry_write("runner-profile"))
            .unwrap();
        assert_eq!(created.profile_id, ProfileId::new("runner-profile"));
        assert_eq!(
            created.lifecycle_status,
            ProfileRegistryLifecycleStatus::Active
        );
        assert_eq!(created.revision, 1);
        assert_eq!(created.display_name.as_deref(), Some("Runner Profile"));
        assert_eq!(created.default_session_kind, Some(SessionKind::Full));
        assert_eq!(created.source_asset_refs.len(), 2);
        assert_eq!(created.source_asset_refs[0].asset_kind, "profile_yaml");
        assert_eq!(
            created.source_asset_refs[0].path,
            "/home/agents/rusty-crew/config/profiles/runner-profile/profile.yaml"
        );
        assert_eq!(created.derived_runtime_refs[0].ref_kind, "session");

        let loaded = store
            .get_profile_registry_record(&ProfileId::new("runner-profile"))
            .unwrap()
            .unwrap();
        assert_eq!(loaded.source_asset_refs, created.source_asset_refs);
        assert_eq!(loaded.import_export.imported_from.as_deref(), Some("file"));

        let duplicate = store
            .create_profile_registry_record(&profile_registry_write("runner-profile"))
            .unwrap_err();
        assert_eq!(duplicate.kind, CoreErrorKind::AlreadyExists);

        store
            .create_profile_registry_record(&ProfileRegistryWrite {
                lifecycle_status: ProfileRegistryLifecycleStatus::Paused,
                display_name: Some("Paused Profile".to_string()),
                now: "2026-06-26T02:00:00Z".to_string(),
                ..profile_registry_write("paused-profile")
            })
            .unwrap();

        let active = store
            .list_profile_registry_records(&ProfileRegistryQuery {
                lifecycle_status: Some(ProfileRegistryLifecycleStatus::Active),
                page: None,
            })
            .unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].profile_id, ProfileId::new("runner-profile"));

        let paused = store
            .update_profile_registry_lifecycle(&ProfileRegistryLifecycleUpdate {
                profile_id: ProfileId::new("runner-profile"),
                lifecycle_status: ProfileRegistryLifecycleStatus::Paused,
                expected_revision: created.revision,
                now: "2026-06-26T03:00:00Z".to_string(),
            })
            .unwrap();
        assert_eq!(
            paused.lifecycle_status,
            ProfileRegistryLifecycleStatus::Paused
        );
        assert_eq!(paused.revision, 2);
        assert_eq!(paused.created_at, "2026-06-26T01:00:00Z");
        assert_eq!(paused.updated_at, "2026-06-26T03:00:00Z");

        let stale = store
            .update_profile_registry_lifecycle(&ProfileRegistryLifecycleUpdate {
                profile_id: ProfileId::new("runner-profile"),
                lifecycle_status: ProfileRegistryLifecycleStatus::Archived,
                expected_revision: 1,
                now: "2026-06-26T04:00:00Z".to_string(),
            })
            .unwrap_err();
        assert_eq!(stale.kind, CoreErrorKind::ActionRejected);

        let invalid_id = store
            .create_profile_registry_record(&profile_registry_write("../bad"))
            .unwrap_err();
        assert_eq!(invalid_id.kind, CoreErrorKind::InvalidInput);

        assert_eq!(store.count_rows("profile_registry").unwrap(), 2);
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
    fn memory_proposals_persist_governance_state_without_direct_mutation() {
        let db_path = temp_db_path("memory-proposals");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        let descriptor = profile_dense_memory_space_descriptor();
        let proposal = profile_dense_memory_proposal("proposal_one", "profile_dense:style");

        let created = store
            .save_memory_proposal(&proposal, &descriptor, &"2026-06-26T00:00:00Z".to_string())
            .unwrap();
        assert_eq!(created.proposal.proposal_id, "proposal_one");
        assert_eq!(created.status, MemoryProposalReviewStatus::PendingReview);
        assert_eq!(
            created.selected_governance_mode,
            MemoryGovernanceMode::CuratorRoute
        );
        assert!(store
            .get_profile_memory(
                &ProfileId::new("prime-profile"),
                &ProfileMemoryTarget::Profile,
                "style"
            )
            .unwrap()
            .is_none());

        let duplicate = store
            .save_memory_proposal(
                &profile_dense_memory_proposal("proposal_two", "profile_dense:style"),
                &descriptor,
                &"2026-06-26T00:01:00Z".to_string(),
            )
            .unwrap();
        assert_eq!(duplicate.proposal.proposal_id, "proposal_one");
        assert_eq!(store.count_rows("memory_proposals").unwrap(), 1);

        let pending = store
            .list_memory_proposals(&MemoryProposalQuery {
                space_id: Some(MemorySpaceId::unchecked("profile_dense")),
                status: Some(MemoryProposalReviewStatus::PendingReview),
                dedupe_key: None,
                limit: None,
                offset: None,
            })
            .unwrap();
        assert_eq!(pending.len(), 1);

        let bad_space = store
            .save_memory_proposal(
                &MemoryProposalEnvelope {
                    space_id: MemorySpaceId::unchecked("roleplay_lore"),
                    ..profile_dense_memory_proposal("proposal_bad_space", "profile_dense:bad")
                },
                &descriptor,
                &"2026-06-26T00:02:00Z".to_string(),
            )
            .unwrap_err();
        assert_eq!(bad_space.kind, CoreErrorKind::InvalidInput);

        let bad_scope = store
            .save_memory_proposal(
                &MemoryProposalEnvelope {
                    proposal_id: "proposal_bad_scope".to_string(),
                    scope: MemoryScope {
                        scope_type: MemoryScopeType::World,
                        scope_id: "world-alpha".to_string(),
                    },
                    dedupe_key: Some("profile_dense:bad_scope".to_string()),
                    ..proposal.clone()
                },
                &descriptor,
                &"2026-06-26T00:03:00Z".to_string(),
            )
            .unwrap_err();
        assert_eq!(bad_scope.kind, CoreErrorKind::InvalidInput);

        let bad_operation = store
            .save_memory_proposal(
                &MemoryProposalEnvelope {
                    proposal_id: "proposal_bad_operation".to_string(),
                    operation: MemoryOperation::Merge,
                    dedupe_key: Some("profile_dense:bad_operation".to_string()),
                    ..proposal.clone()
                },
                &descriptor,
                &"2026-06-26T00:04:00Z".to_string(),
            )
            .unwrap_err();
        assert_eq!(bad_operation.kind, CoreErrorKind::InvalidInput);

        let approved = store
            .record_memory_governance_decision(
                &MemoryGovernanceDecisionInput {
                    decision_id: "decision_approve".to_string(),
                    proposal_id: "proposal_one".to_string(),
                    decision: MemoryGovernanceDecisionKind::Approved,
                    actor: "human_operator".to_string(),
                    source: MemoryProposalSource::Human,
                    evidence_refs: proposal.evidence_refs.clone(),
                    policy_mode: MemoryGovernanceMode::ManualReview,
                    confidence: Some(0.95),
                    message: Some("approved for later apply".to_string()),
                    resulting_revision: None,
                    decided_at: None,
                },
                &"2026-06-26T00:05:00Z".to_string(),
            )
            .unwrap();
        assert_eq!(approved.decision, MemoryGovernanceDecisionKind::Approved);

        let applied = store
            .record_memory_governance_decision(
                &MemoryGovernanceDecisionInput {
                    decision_id: "decision_apply".to_string(),
                    proposal_id: "proposal_one".to_string(),
                    decision: MemoryGovernanceDecisionKind::Applied,
                    actor: "curator".to_string(),
                    source: MemoryProposalSource::Human,
                    evidence_refs: proposal.evidence_refs.clone(),
                    policy_mode: MemoryGovernanceMode::ManualReview,
                    confidence: Some(0.97),
                    message: Some("compatibility projection only".to_string()),
                    resulting_revision: Some(7),
                    decided_at: None,
                },
                &"2026-06-26T00:06:00Z".to_string(),
            )
            .unwrap();
        assert_eq!(applied.resulting_revision, Some(7));

        let records = store
            .list_memory_proposals(&MemoryProposalQuery {
                space_id: None,
                status: Some(MemoryProposalReviewStatus::Applied),
                dedupe_key: None,
                limit: None,
                offset: None,
            })
            .unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].status, MemoryProposalReviewStatus::Applied);
        assert_eq!(records[0].resulting_revision, Some(7));
        assert!(store
            .get_profile_memory(
                &ProfileId::new("prime-profile"),
                &ProfileMemoryTarget::Profile,
                "style"
            )
            .unwrap()
            .is_none());

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
    fn provider_wire_state_replaces_current_record_and_preserves_payload_version() {
        let db_path = temp_db_path("provider-wire-replace");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        let key = sample_provider_wire_state_key();

        store
            .save_provider_wire_state(&sample_provider_wire_state_write(
                ProviderWireStateWriteFixture {
                    key: key.clone(),
                    profile_fingerprint: "profile-fp-1",
                    provider_fingerprint: "provider-fp-1",
                    payload_version: "provider-owned-v1",
                    payload_json: serde_json::json!({"response_id": "resp-1"}),
                    now: "2026-06-20T00:00:00Z",
                    expires_at: Some("2026-06-20T06:00:00Z"),
                    last_wake_id: Some("wake-1"),
                },
            ))
            .unwrap();
        store
            .save_provider_wire_state(&sample_provider_wire_state_write(
                ProviderWireStateWriteFixture {
                    key: key.clone(),
                    profile_fingerprint: "profile-fp-1",
                    provider_fingerprint: "provider-fp-1",
                    payload_version: "provider-owned-v9000",
                    payload_json: serde_json::json!({"response_id": "resp-2"}),
                    now: "2026-06-20T00:01:00Z",
                    expires_at: Some("2026-06-20T06:01:00Z"),
                    last_wake_id: Some("wake-2"),
                },
            ))
            .unwrap();

        assert_eq!(store.count_rows("provider_wire_states").unwrap(), 2);
        let loaded = store
            .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
                key,
                profile_fingerprint: "profile-fp-1".to_string(),
                provider_fingerprint: "provider-fp-1".to_string(),
                now: "2026-06-20T00:02:00Z".to_string(),
            })
            .unwrap();
        let record = loaded.record.unwrap();
        assert_eq!(loaded.absence_reason, None);
        assert_eq!(record.payload_version, "provider-owned-v9000");
        assert_eq!(
            record.payload_json,
            serde_json::json!({"response_id": "resp-2"})
        );
        assert_eq!(record.last_wake_id.as_deref(), Some("wake-2"));
        assert!(record.is_current());

        remove_temp_db(&db_path);
    }

    #[test]
    fn provider_wire_state_withholds_expired_and_fingerprint_stale_records() {
        let db_path = temp_db_path("provider-wire-invalidation");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        let key = sample_provider_wire_state_key();

        store
            .save_provider_wire_state(&sample_provider_wire_state_write(
                ProviderWireStateWriteFixture {
                    key: key.clone(),
                    profile_fingerprint: "profile-fp-1",
                    provider_fingerprint: "provider-fp-1",
                    payload_version: "provider-owned-v1",
                    payload_json: serde_json::json!({"response_id": "expired"}),
                    now: "2026-06-20T00:00:00Z",
                    expires_at: Some("2026-06-20T00:05:00Z"),
                    last_wake_id: Some("wake-expired"),
                },
            ))
            .unwrap();
        let expired = store
            .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
                key: key.clone(),
                profile_fingerprint: "profile-fp-1".to_string(),
                provider_fingerprint: "provider-fp-1".to_string(),
                now: "2026-06-20T00:05:00Z".to_string(),
            })
            .unwrap();
        assert_eq!(expired.record, None);
        assert_eq!(
            expired.absence_reason,
            Some(ProviderStateAbsenceReason::Expired)
        );

        store
            .save_provider_wire_state(&sample_provider_wire_state_write(
                ProviderWireStateWriteFixture {
                    key: key.clone(),
                    profile_fingerprint: "profile-fp-1",
                    provider_fingerprint: "provider-fp-1",
                    payload_version: "provider-owned-v2",
                    payload_json: serde_json::json!({"response_id": "profile-stale"}),
                    now: "2026-06-20T00:06:00Z",
                    expires_at: Some("2026-06-20T06:00:00Z"),
                    last_wake_id: Some("wake-profile-stale"),
                },
            ))
            .unwrap();
        let profile_stale = store
            .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
                key: key.clone(),
                profile_fingerprint: "profile-fp-2".to_string(),
                provider_fingerprint: "provider-fp-1".to_string(),
                now: "2026-06-20T00:07:00Z".to_string(),
            })
            .unwrap();
        assert_eq!(profile_stale.record, None);
        assert_eq!(
            profile_stale.absence_reason,
            Some(ProviderStateAbsenceReason::Invalidated)
        );

        store
            .save_provider_wire_state(&sample_provider_wire_state_write(
                ProviderWireStateWriteFixture {
                    key: key.clone(),
                    profile_fingerprint: "profile-fp-2",
                    provider_fingerprint: "provider-fp-1",
                    payload_version: "provider-owned-v3",
                    payload_json: serde_json::json!({"response_id": "provider-stale"}),
                    now: "2026-06-20T00:08:00Z",
                    expires_at: Some("2026-06-20T06:00:00Z"),
                    last_wake_id: Some("wake-provider-stale"),
                },
            ))
            .unwrap();
        let provider_stale = store
            .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
                key,
                profile_fingerprint: "profile-fp-2".to_string(),
                provider_fingerprint: "provider-fp-2".to_string(),
                now: "2026-06-20T00:09:00Z".to_string(),
            })
            .unwrap();
        assert_eq!(provider_stale.record, None);
        assert_eq!(
            provider_stale.absence_reason,
            Some(ProviderStateAbsenceReason::Invalidated)
        );

        remove_temp_db(&db_path);
    }

    #[test]
    fn provider_wire_state_clear_and_strategy_change_remove_current_state() {
        let db_path = temp_db_path("provider-wire-clear");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        let key = sample_provider_wire_state_key();

        store
            .save_provider_wire_state(&sample_provider_wire_state_write(
                ProviderWireStateWriteFixture {
                    key: key.clone(),
                    profile_fingerprint: "profile-fp",
                    provider_fingerprint: "provider-fp",
                    payload_version: "provider-owned-v1",
                    payload_json: serde_json::json!({"response_id": "clear-me"}),
                    now: "2026-06-20T00:00:00Z",
                    expires_at: Some("2026-06-20T06:00:00Z"),
                    last_wake_id: Some("wake-clear"),
                },
            ))
            .unwrap();
        let cleared = store
            .clear_provider_wire_state(
                &key,
                &"2026-06-20T00:01:00Z".to_string(),
                ProviderWireStateInvalidationReason::BrainRequestedClear,
            )
            .unwrap()
            .unwrap();
        assert_eq!(
            cleared.invalidation_reason,
            Some(ProviderWireStateInvalidationReason::BrainRequestedClear)
        );
        let after_clear = store
            .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
                key: key.clone(),
                profile_fingerprint: "profile-fp".to_string(),
                provider_fingerprint: "provider-fp".to_string(),
                now: "2026-06-20T00:02:00Z".to_string(),
            })
            .unwrap();
        assert_eq!(after_clear.record, None);
        assert_eq!(
            after_clear.absence_reason,
            Some(ProviderStateAbsenceReason::Missing)
        );

        store
            .save_provider_wire_state(&sample_provider_wire_state_write(
                ProviderWireStateWriteFixture {
                    key: key.clone(),
                    profile_fingerprint: "profile-fp",
                    provider_fingerprint: "provider-fp",
                    payload_version: "provider-owned-v2",
                    payload_json: serde_json::json!({"response_id": "old-strategy"}),
                    now: "2026-06-20T00:03:00Z",
                    expires_at: Some("2026-06-20T06:00:00Z"),
                    last_wake_id: Some("wake-old-strategy"),
                },
            ))
            .unwrap();
        let changed_key = ProviderWireStateKey {
            strategy_id: "replay-v2".to_string(),
            ..key.clone()
        };
        let changed = store
            .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
                key: changed_key,
                profile_fingerprint: "profile-fp".to_string(),
                provider_fingerprint: "provider-fp".to_string(),
                now: "2026-06-20T00:04:00Z".to_string(),
            })
            .unwrap();
        assert_eq!(changed.record, None);
        assert_eq!(
            changed.absence_reason,
            Some(ProviderStateAbsenceReason::Missing)
        );
        let old_key_after_strategy_change = store
            .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
                key,
                profile_fingerprint: "profile-fp".to_string(),
                provider_fingerprint: "provider-fp".to_string(),
                now: "2026-06-20T00:05:00Z".to_string(),
            })
            .unwrap();
        assert_eq!(old_key_after_strategy_change.record, None);

        remove_temp_db(&db_path);
    }

    #[test]
    fn provider_wire_state_maintenance_marks_expired_current_records() {
        let db_path = temp_db_path("provider-wire-maintenance");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        let key = sample_provider_wire_state_key();

        store
            .save_provider_wire_state(&sample_provider_wire_state_write(
                ProviderWireStateWriteFixture {
                    key: key.clone(),
                    profile_fingerprint: "profile-fp",
                    provider_fingerprint: "provider-fp",
                    payload_version: "provider-owned-v1",
                    payload_json: serde_json::json!({"response_id": "expire-me"}),
                    now: "2026-06-20T00:00:00Z",
                    expires_at: Some("2026-06-20T00:05:00Z"),
                    last_wake_id: Some("wake-expire-me"),
                },
            ))
            .unwrap();
        let report = store
            .run_maintenance(&RuntimeMaintenancePolicy {
                expire_provider_wire_states_at: Some("2026-06-20T00:05:01Z".to_string()),
                ..RuntimeMaintenancePolicy::default()
            })
            .unwrap();
        assert_eq!(report.expired_provider_wire_states, 1);
        let after_expiry = store
            .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
                key,
                profile_fingerprint: "profile-fp".to_string(),
                provider_fingerprint: "provider-fp".to_string(),
                now: "2026-06-20T00:05:02Z".to_string(),
            })
            .unwrap();
        assert_eq!(after_expiry.record, None);
        assert_eq!(
            after_expiry.absence_reason,
            Some(ProviderStateAbsenceReason::Missing)
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
            history_window: None,
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
            history_window: beta_config.history_window.clone(),
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
    fn message_slots_persist_variants_and_active_selection_conflicts() {
        let db_path = temp_db_path("message-slots");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        let now = "2026-06-25T03:00:00Z".to_string();
        let slot_id = MessageSlotId::new("slot-1");
        let primary_variant_id = MessageVariantId::new("variant-primary");
        store
            .save_message_slot(&MessageSlotWrite {
                slot_id: slot_id.clone(),
                session_id: SessionId::new("session-1"),
                primary_variant_id: primary_variant_id.clone(),
                active_variant_id: None,
                metadata_json: json!({"origin": "test"}),
                created_at: now.clone(),
                updated_at: now.clone(),
            })
            .unwrap();
        store
            .save_message_variant(&variant_write(
                &slot_id,
                &primary_variant_id,
                MessageVariantSource::Primary,
                0,
                "message-primary",
                "primary body",
            ))
            .unwrap();
        store
            .save_message_variant(&variant_write(
                &slot_id,
                &MessageVariantId::new("variant-a"),
                MessageVariantSource::Alternate,
                1,
                "message-a",
                "alternate a",
            ))
            .unwrap();
        store
            .save_message_variant(&variant_write(
                &slot_id,
                &MessageVariantId::new("variant-b"),
                MessageVariantSource::Alternate,
                2,
                "message-b",
                "alternate b",
            ))
            .unwrap();

        let lazy = store
            .query_message_slots(&MessageSlotQuery {
                session_id: Some(SessionId::new("session-1")),
                include_alternates: false,
                page: None,
            })
            .unwrap();
        assert_eq!(lazy.len(), 1);
        assert_eq!(lazy[0].primary.message.body, "primary body");
        assert!(lazy[0].alternates.is_empty());

        let variants = store
            .query_message_variants(&MessageVariantQuery {
                slot_id: Some(slot_id.clone()),
                include_deleted: false,
                page: None,
            })
            .unwrap();
        assert_eq!(
            variants
                .iter()
                .map(|variant| variant.variant_id.0.as_str())
                .collect::<Vec<_>>(),
            vec!["variant-primary", "variant-a", "variant-b"]
        );
        assert_eq!(variants[0].message.blocks[0].kind, "text");

        let selected = store
            .select_active_message_variant(&SelectActiveVariantRequest {
                slot_id: slot_id.clone(),
                active_variant_id: Some(MessageVariantId::new("variant-a")),
                expected: ActiveVariantExpectation::Primary,
                updated_at: "2026-06-25T03:01:00Z".to_string(),
            })
            .unwrap();
        assert!(selected.conflict.is_none());
        assert_eq!(
            selected.slot.active_variant_id,
            Some(MessageVariantId::new("variant-a"))
        );

        let conflict = store
            .select_active_message_variant(&SelectActiveVariantRequest {
                slot_id: slot_id.clone(),
                active_variant_id: Some(MessageVariantId::new("variant-b")),
                expected: ActiveVariantExpectation::Primary,
                updated_at: "2026-06-25T03:02:00Z".to_string(),
            })
            .unwrap();
        assert_eq!(
            conflict.conflict.unwrap().actual,
            Some(MessageVariantId::new("variant-a"))
        );

        store
            .reorder_message_variants(
                &slot_id,
                &[
                    MessageVariantId::new("variant-b"),
                    MessageVariantId::new("variant-a"),
                ],
                &"2026-06-25T03:03:00Z".to_string(),
            )
            .unwrap();
        let reordered = store
            .query_message_variants(&MessageVariantQuery {
                slot_id: Some(slot_id.clone()),
                include_deleted: false,
                page: None,
            })
            .unwrap();
        assert_eq!(
            reordered
                .iter()
                .map(|variant| variant.variant_id.0.as_str())
                .collect::<Vec<_>>(),
            vec!["variant-primary", "variant-b", "variant-a"]
        );

        let deleted = store
            .delete_message_variant(
                &slot_id,
                &MessageVariantId::new("variant-a"),
                &"2026-06-25T03:04:00Z".to_string(),
            )
            .unwrap();
        assert_eq!(deleted.active_variant_id, None);
        assert_eq!(deleted.alternates.len(), 1);
        assert_eq!(
            deleted.alternates[0].variant_id,
            MessageVariantId::new("variant-b")
        );

        remove_temp_db(&db_path);
    }

    #[test]
    fn conversation_tree_branches_snapshots_and_jump_targets_persist() {
        let db_path = temp_db_path("conversation-tree");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        let now = "2026-06-25T04:00:00Z".to_string();
        let session_id = SessionId::new("session-1");
        let root_branch = ConversationBranchId::new("branch-root");
        let child_branch = ConversationBranchId::new("branch-child");
        let slot_id = MessageSlotId::new("slot-tree");
        let primary_variant_id = MessageVariantId::new("variant-tree-primary");
        let root_message_id = MessageId::new("message-root");
        let child_message_id = MessageId::new("message-child");

        store
            .save_conversation_branch(&ConversationBranchWrite {
                branch_id: root_branch.clone(),
                session_id: session_id.clone(),
                parent_branch_id: None,
                parent_message_id: None,
                origin_message_id: None,
                head_message_id: Some(root_message_id.clone()),
                label: Some("Root".to_string()),
                metadata_json: json!({"kind": "default"}),
                created_at: now.clone(),
                updated_at: now.clone(),
            })
            .unwrap();
        store
            .save_message_slot(&MessageSlotWrite {
                slot_id: slot_id.clone(),
                session_id: session_id.clone(),
                primary_variant_id: primary_variant_id.clone(),
                active_variant_id: None,
                metadata_json: json!({}),
                created_at: now.clone(),
                updated_at: now.clone(),
            })
            .unwrap();
        let mut variant = variant_write(
            &slot_id,
            &primary_variant_id,
            MessageVariantSource::Primary,
            0,
            &root_message_id.0,
            "root body",
        );
        variant.message.branch_id = Some(root_branch.clone());
        store.save_message_variant(&variant).unwrap();

        store
            .save_conversation_branch(&ConversationBranchWrite {
                branch_id: child_branch.clone(),
                session_id: session_id.clone(),
                parent_branch_id: Some(root_branch.clone()),
                parent_message_id: Some(root_message_id.clone()),
                origin_message_id: Some(root_message_id.clone()),
                head_message_id: Some(child_message_id.clone()),
                label: Some("Alternative".to_string()),
                metadata_json: json!({"reason": "alternate"}),
                created_at: "2026-06-25T04:01:00Z".to_string(),
                updated_at: "2026-06-25T04:01:00Z".to_string(),
            })
            .unwrap();

        let branches = store
            .query_conversation_branches(&ConversationBranchQuery {
                session_id: Some(session_id.clone()),
                parent_branch_id: None,
                page: None,
            })
            .unwrap();
        assert_eq!(branches.len(), 2);
        assert_eq!(branches[1].parent_branch_id, Some(root_branch.clone()));

        let selected = store
            .select_active_conversation_branch(&SelectActiveBranchRequest {
                session_id: session_id.clone(),
                active_branch_id: Some(child_branch.clone()),
                expected: ActiveBranchExpectation::None,
                updated_at: "2026-06-25T04:02:00Z".to_string(),
            })
            .unwrap();
        assert!(selected.conflict.is_none());
        assert_eq!(selected.state.active_branch_id, Some(child_branch.clone()));

        let conflict = store
            .select_active_conversation_branch(&SelectActiveBranchRequest {
                session_id: session_id.clone(),
                active_branch_id: Some(root_branch.clone()),
                expected: ActiveBranchExpectation::None,
                updated_at: "2026-06-25T04:03:00Z".to_string(),
            })
            .unwrap();
        assert_eq!(
            conflict.conflict.unwrap().actual,
            Some(child_branch.clone())
        );

        let head_conflict = store
            .update_conversation_branch_head(&UpdateBranchHeadRequest {
                branch_id: child_branch.clone(),
                head_message_id: Some(root_message_id.clone()),
                expected: BranchHeadExpectation::None,
                updated_at: "2026-06-25T04:04:00Z".to_string(),
            })
            .unwrap();
        assert_eq!(
            head_conflict.conflict.unwrap().actual,
            Some(child_message_id.clone())
        );

        let snapshot = store
            .save_conversation_snapshot(&ConversationSnapshotWrite {
                snapshot_id: ConversationSnapshotId::new("snapshot-1"),
                session_id: session_id.clone(),
                branch_id: Some(child_branch.clone()),
                message_id: Some(root_message_id.clone()),
                cursor: Some("session-1:42".to_string()),
                label: Some("Before alternate".to_string()),
                summary: Some("Checkpoint summary".to_string()),
                source: ConversationSnapshotSource::User,
                metadata_json: json!({"from": "test"}),
                created_at: "2026-06-25T04:05:00Z".to_string(),
                updated_at: "2026-06-25T04:05:00Z".to_string(),
            })
            .unwrap();
        assert_eq!(snapshot.branch_id, Some(child_branch.clone()));

        let snapshots = store
            .query_conversation_snapshots(&ConversationSnapshotQuery {
                session_id: Some(session_id.clone()),
                branch_id: None,
                message_id: Some(root_message_id.clone()),
                page: None,
            })
            .unwrap();
        assert_eq!(snapshots.len(), 1);

        let branch_jump = store
            .resolve_conversation_jump(&ConversationJumpRequest {
                session_id: session_id.clone(),
                target: ConversationJumpTarget::Branch {
                    branch_id: child_branch.clone(),
                },
            })
            .unwrap();
        assert_eq!(branch_jump.message_id, Some(child_message_id.clone()));

        let snapshot_jump = store
            .resolve_conversation_jump(&ConversationJumpRequest {
                session_id: session_id.clone(),
                target: ConversationJumpTarget::Snapshot {
                    snapshot_id: ConversationSnapshotId::new("snapshot-1"),
                },
            })
            .unwrap();
        assert_eq!(snapshot_jump.cursor, Some("session-1:42".to_string()));

        let message_jump = store
            .resolve_conversation_jump(&ConversationJumpRequest {
                session_id,
                target: ConversationJumpTarget::Message {
                    message_id: root_message_id,
                },
            })
            .unwrap();
        assert_eq!(message_jump.branch_id, Some(root_branch));

        remove_temp_db(&db_path);
    }

    #[test]
    fn attachments_and_data_bank_scopes_persist_across_reopen() {
        let db_path = temp_db_path("attachments-data-bank");
        let session_id = SessionId::new("session-attachment");
        let scope_id = DataBankScopeId::new("scope-reference");
        let attachment_id = AttachmentId::new("attachment-guide");
        let message_id = MessageId::new("message-guide");

        {
            let store = CoordinationStore::open_file(&db_path).unwrap();
            store
                .save_data_bank_scope(&DataBankScopeWrite {
                    scope_id: scope_id.clone(),
                    session_id: session_id.clone(),
                    status: DataBankScopeStatus::Active,
                    label: Some("Reference".to_string()),
                    description: Some("Reusable files".to_string()),
                    metadata_json: json!({"source": "test"}),
                    created_at: "2026-06-25T05:00:00Z".to_string(),
                    updated_at: "2026-06-25T05:00:00Z".to_string(),
                })
                .unwrap();
            let saved = store
                .save_attachment(&AttachmentWrite {
                    attachment_id: attachment_id.clone(),
                    session_id: session_id.clone(),
                    status: AttachmentStatus::Active,
                    filename: "guide.txt".to_string(),
                    mime_type: "text/plain".to_string(),
                    byte_size: 42,
                    storage_url: None,
                    download_url: Some("/download/guide".to_string()),
                    thumbnail_url: None,
                    extracted_text: Some("hello attachment".to_string()),
                    extracted_text_truncated: false,
                    metadata_json: json!({"kind": "reference"}),
                    created_at: "2026-06-25T05:01:00Z".to_string(),
                    updated_at: "2026-06-25T05:01:00Z".to_string(),
                    expires_at: None,
                    link: Some(AttachmentLinkWrite {
                        link_id: AttachmentLinkId::new("attachment-link-guide"),
                        attachment_id: attachment_id.clone(),
                        session_id: session_id.clone(),
                        message_id: Some(message_id.clone()),
                        block_id: None,
                        scope_id: Some(scope_id.clone()),
                        metadata_json: json!({"linked_by": "test"}),
                        created_at: "2026-06-25T05:01:00Z".to_string(),
                    }),
                })
                .unwrap();
            assert_eq!(saved.links.len(), 1);
        }

        let store = CoordinationStore::open_file(&db_path).unwrap();
        let by_message = store
            .query_attachments(&AttachmentQuery {
                session_id: Some(session_id.clone()),
                message_id: Some(message_id),
                scope_id: None,
                include_removed: false,
                page: None,
            })
            .unwrap();
        assert_eq!(by_message.len(), 1);
        assert_eq!(&by_message[0].attachment_id, &attachment_id);
        assert_eq!(by_message[0].links[0].scope_id, Some(scope_id.clone()));

        let by_scope = store
            .query_attachments(&AttachmentQuery {
                session_id: Some(session_id.clone()),
                message_id: None,
                scope_id: Some(scope_id.clone()),
                include_removed: false,
                page: None,
            })
            .unwrap();
        assert_eq!(by_scope.len(), 1);

        let scopes = store
            .query_data_bank_scopes(&DataBankScopeQuery {
                session_id: Some(session_id.clone()),
                include_removed: false,
                page: None,
            })
            .unwrap();
        assert_eq!(scopes.len(), 1);
        assert_eq!(&scopes[0].scope_id, &scope_id);

        let removed_attachment = store
            .remove_attachment(
                &AttachmentId::new("attachment-guide"),
                &"2026-06-25T05:02:00Z".to_string(),
            )
            .unwrap();
        assert_eq!(removed_attachment.status, AttachmentStatus::Removed);
        let active_after_remove = store
            .query_attachments(&AttachmentQuery {
                session_id: Some(session_id.clone()),
                include_removed: false,
                ..AttachmentQuery::default()
            })
            .unwrap();
        assert!(active_after_remove.is_empty());
        let removed_scope = store
            .remove_data_bank_scope(&scope_id, &"2026-06-25T05:03:00Z".to_string())
            .unwrap();
        assert_eq!(removed_scope.status, DataBankScopeStatus::Removed);

        let removed_records = store
            .query_attachments(&AttachmentQuery {
                session_id: Some(session_id.clone()),
                include_removed: true,
                ..AttachmentQuery::default()
            })
            .unwrap();
        assert_eq!(removed_records.len(), 1);
        let removed_scopes = store
            .query_data_bank_scopes(&DataBankScopeQuery {
                session_id: Some(session_id),
                include_removed: true,
                page: None,
            })
            .unwrap();
        assert_eq!(removed_scopes.len(), 1);

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
                history_window: None,
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
                        history_window: None,
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
                expire_provider_wire_states_at: None,
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

    fn sample_provider_wire_state_key() -> ProviderWireStateKey {
        ProviderWireStateKey {
            session_id: SessionId::new("session-alpha"),
            module_id: "openai-responses".to_string(),
            strategy_id: "replay".to_string(),
        }
    }

    fn simple_kv_schema_bundle(version: u32) -> CoreResult<ModuleSchemaBundle> {
        let mut bundle = crate::module_schema::simple_kv_schema_bundle();
        bundle.schema_version = version;
        if version != 1 {
            bundle
                .migration_notes
                .push(format!("test schema version {version}"));
        }
        Ok(bundle)
    }

    struct ProviderWireStateWriteFixture<'a> {
        key: ProviderWireStateKey,
        profile_fingerprint: &'a str,
        provider_fingerprint: &'a str,
        payload_version: &'a str,
        payload_json: JsonValue,
        now: &'a str,
        expires_at: Option<&'a str>,
        last_wake_id: Option<&'a str>,
    }

    fn sample_provider_wire_state_write(
        input: ProviderWireStateWriteFixture<'_>,
    ) -> ProviderWireStateWrite {
        ProviderWireStateWrite {
            key: input.key,
            profile_fingerprint: input.profile_fingerprint.to_string(),
            provider_fingerprint: input.provider_fingerprint.to_string(),
            payload_version: input.payload_version.to_string(),
            payload_json: input.payload_json,
            now: input.now.to_string(),
            expires_at: input.expires_at.map(ToString::to_string),
            last_wake_id: input.last_wake_id.map(ToString::to_string),
        }
    }

    fn variant_write(
        slot_id: &MessageSlotId,
        variant_id: &MessageVariantId,
        source: MessageVariantSource,
        ordinal: u32,
        message_id: &str,
        body: &str,
    ) -> MessageVariantWrite {
        MessageVariantWrite {
            variant_id: variant_id.clone(),
            slot_id: slot_id.clone(),
            source,
            ordinal,
            status: MessageVariantStatus::Active,
            message: DurableMessageWrite {
                message_id: MessageId::new(message_id),
                session_id: SessionId::new("session-1"),
                branch_id: None,
                parent_message_id: None,
                previous_message_id: None,
                author_id: "agent-alpha".to_string(),
                author_role: "assistant".to_string(),
                status: DurableMessageStatus::Completed,
                body: body.to_string(),
                metadata_json: json!({"provider": "fixture"}),
                created_at: "2026-06-25T03:00:00Z".to_string(),
                blocks: vec![MessageBlockWrite {
                    block_id: MessageBlockId::new(format!("{message_id}:block-1")),
                    ordinal: 0,
                    kind: "text".to_string(),
                    content_json: json!({"text": body}),
                    render_policy_json: None,
                    metadata_json: json!({}),
                }],
            },
            metadata_json: json!({}),
            created_at: "2026-06-25T03:00:00Z".to_string(),
            updated_at: "2026-06-25T03:00:00Z".to_string(),
        }
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
            history_window: None,
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

    fn profile_registry_write(profile_id: &str) -> ProfileRegistryWrite {
        ProfileRegistryWrite {
            profile_id: ProfileId::new(profile_id),
            lifecycle_status: ProfileRegistryLifecycleStatus::Active,
            display_name: Some("Runner Profile".to_string()),
            summary: Some("Test registry-backed runner profile.".to_string()),
            default_session_kind: Some(SessionKind::Full),
            agent_id: Some(AgentId::new("runner-agent")),
            owner_id: Some("operator".to_string()),
            active_runtime_settings_json: json!({
                "brainModule": "pi_agent_core",
                "model": "gpt"
            }),
            source_asset_refs: vec![
                ProfileRegistrySourceAssetRef {
                    asset_kind: "profile_yaml".to_string(),
                    path: format!(
                        "/home/agents/rusty-crew/config/profiles/{profile_id}/profile.yaml"
                    ),
                    content_hash: Some("sha256:profile".to_string()),
                    last_seen_at: Some("2026-06-26T00:59:00Z".to_string()),
                    metadata_json: json!({"source": "file"}),
                },
                ProfileRegistrySourceAssetRef {
                    asset_kind: "soul_md".to_string(),
                    path: format!("/home/agents/rusty-crew/config/profiles/{profile_id}/soul.md"),
                    content_hash: Some("sha256:soul".to_string()),
                    last_seen_at: Some("2026-06-26T00:59:00Z".to_string()),
                    metadata_json: json!({"source": "file"}),
                },
            ],
            derived_runtime_refs: vec![ProfileRegistryDerivedRuntimeRef {
                ref_kind: "session".to_string(),
                ref_id: "session-runner".to_string(),
                status: "planned".to_string(),
                updated_at: Some("2026-06-26T00:59:00Z".to_string()),
                metadata_json: json!({"derived": true}),
            }],
            import_export: ProfileRegistryImportExportMetadata {
                imported_from: Some("file".to_string()),
                imported_at: Some("2026-06-26T01:00:00Z".to_string()),
                exported_to: None,
                exported_at: None,
                metadata_json: json!({"compatibility": "file_loader"}),
            },
            now: "2026-06-26T01:00:00Z".to_string(),
        }
    }

    fn profile_dense_memory_proposal(
        proposal_id: &str,
        dedupe_key: &str,
    ) -> MemoryProposalEnvelope {
        MemoryProposalEnvelope {
            proposal_id: proposal_id.to_string(),
            space_id: MemorySpaceId::unchecked("profile_dense"),
            operation: MemoryOperation::CandidateOnly,
            scope: MemoryScope {
                scope_type: MemoryScopeType::Profile,
                scope_id: "prime-profile".to_string(),
            },
            shape: MemoryRecordShapeRef {
                shape_id: MemoryRecordShapeId::unchecked("profile_dense_item"),
                version: 1,
            },
            content: json!({
                "key": "style",
                "content": "prefers typed governance review"
            }),
            evidence_refs: vec![MemoryEvidenceRef {
                evidence_type: MemoryEvidenceKind::Wake,
                ref_id: "wake-alpha".to_string(),
                label: Some("wake evidence".to_string()),
            }],
            confidence: 0.82,
            durability_rationale: Some("stable profile preference".to_string()),
            governance_mode: MemoryGovernanceMode::DirectWrite,
            source: MemoryProposalSource::InWakeTool,
            dedupe_key: Some(dedupe_key.to_string()),
            created_at: None,
        }
    }

    fn profile_dense_memory_space_descriptor() -> MemorySpaceDescriptor {
        MemorySpaceDescriptor {
            space_id: MemorySpaceId::unchecked("profile_dense"),
            schema_version: 1,
            module_id: Some("runtime_memory".to_string()),
            description: "Compact stable Crew profile memory.".to_string(),
            record_shapes: vec![MemoryRecordShapeDescriptor {
                shape_id: MemoryRecordShapeId::unchecked("profile_dense_item"),
                version: 1,
                description: "Keyed profile or user memory item.".to_string(),
                fields: vec![
                    memory_field("key", MemoryFieldType::String, true),
                    memory_field("content", MemoryFieldType::Markdown, true),
                    memory_field("metadata_json", MemoryFieldType::Json, false),
                    memory_field("revision", MemoryFieldType::Integer, true),
                    memory_field("created_at", MemoryFieldType::Timestamp, true),
                    memory_field("updated_at", MemoryFieldType::Timestamp, true),
                ],
            }],
            scope_model: MemoryScopeModel {
                allowed_scopes: vec![MemoryScopeType::Profile, MemoryScopeType::User],
                primary_scope: MemoryScopeType::Profile,
            },
            visibility_model: MemoryVisibilityModel::ProfileLocal,
            retrieval_strategies: vec![
                MemoryRetrievalStrategy::DirectLookup,
                MemoryRetrievalStrategy::QuerySearch,
            ],
            indexing: MemoryIndexingPolicy {
                required_capabilities: vec![
                    "profile_target_key_lookup".to_string(),
                    "expected_revision_conflicts".to_string(),
                ],
                optional_capabilities: vec![],
            },
            prompt_policy: MemoryPromptPolicy::SummaryContext,
            write_policy: MemoryWritePolicy {
                default_mode: MemoryGovernanceMode::Candidate,
                operation_policies: vec![
                    memory_operation_policy(MemoryOperation::Add, false),
                    memory_operation_policy(MemoryOperation::Replace, true),
                    memory_operation_policy(MemoryOperation::Remove, true),
                    memory_operation_policy(MemoryOperation::CandidateOnly, false),
                ],
            },
            operations: vec![
                MemoryOperation::Read,
                MemoryOperation::List,
                MemoryOperation::Add,
                MemoryOperation::Replace,
                MemoryOperation::Remove,
                MemoryOperation::CandidateOnly,
            ],
            provenance_policy: MemoryProvenancePolicy {
                required_evidence: vec![MemoryEvidenceKind::Wake],
                source_required: false,
                rationale_required: false,
            },
            retention_policy: MemoryRetentionPolicy::ManualOnly,
            conflict_policy: MemoryConflictPolicy::ExpectedRevision,
            diagnostics: MemoryDiagnosticsPolicy {
                expose_catalog: true,
                expose_record_counts: true,
                expose_policy_decisions: true,
            },
            export_import: MemoryExportImportPolicy {
                export_supported: true,
                import_supported: true,
                import_governance_mode: MemoryGovernanceMode::ManualReview,
            },
        }
    }

    fn memory_field(
        field_name: &str,
        field_type: MemoryFieldType,
        required: bool,
    ) -> MemoryRecordFieldDescriptor {
        MemoryRecordFieldDescriptor {
            field_name: field_name.to_string(),
            field_type,
            required,
            description: format!("{field_name} field"),
        }
    }

    fn memory_operation_policy(
        operation: MemoryOperation,
        requires_expected_revision: bool,
    ) -> MemoryOperationPolicy {
        MemoryOperationPolicy {
            operation,
            governance_mode: MemoryGovernanceMode::Candidate,
            requires_expected_revision,
            min_confidence: None,
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
            history_window: None,
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
