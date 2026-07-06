//! Local coordination-state persistence.
//!
//! This store is for Rust-owned coordination state only. Den task, project, and
//! document data remains Den product data and is not mirrored here.

mod contracts;
pub mod module_schema;
#[cfg(feature = "postgres")]
pub mod postgres_backend;
mod repos;
mod repositories;
mod sqlite_identity;
#[cfg(test)]
mod sqlite_integration_tests;
mod sqlite_provider_wire_state;
mod sqlite_runtime_import;
mod sqlite_runtime_search;
mod sqlite_schema;
mod sqlite_simple_kv;
mod sqlite_store;
mod sqlite_worker_pool;

pub use crate::contracts::*;
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
pub(crate) use crate::sqlite_identity::{
    durable_agent_kind_as_str, durable_agent_kind_from_session_kind, durable_agent_kind_from_str,
    durable_identity_status_as_str, durable_identity_status_from_str,
    durable_status_from_session_status, parent_consumption_policy_as_str,
    parent_consumption_policy_from_str, session_kind_as_str, session_kind_from_str,
    session_status_as_str, session_status_from_str, tool_call_phase_from_str,
};
pub(crate) use crate::sqlite_provider_wire_state::expire_provider_wire_states_in_tx;
pub(crate) use crate::sqlite_runtime_search::{
    dedupe_non_empty, insert_runtime_search_row, quote_fts_query, RuntimeSearchInsert,
};
pub(crate) use crate::sqlite_schema::*;
#[cfg(feature = "postgres")]
pub(crate) use crate::sqlite_simple_kv::{
    validate_simple_kv_identity, validate_simple_kv_query, validate_simple_kv_write,
};
pub use crate::sqlite_store::coordination_db_path;

use crate::module_schema::{
    compiled_module_schema_registry, module_schema_registry_diagnostics,
    validate_version_progression, InstalledModuleSchemaRecord, ModuleId, ModuleSchemaBundle,
    ModuleSchemaCapability, ModuleSchemaRegistry,
};
use rusqlite::{params, Connection, OptionalExtension};
use rusty_crew_core_protocol::{
    session_memory_space_descriptor, AdapterId, AgentId, AgentInstanceId, AgentInstanceRecord,
    AgentMessage, AttachmentId, AttachmentLinkId, BrainEvent, CompletionPacket,
    ContextCompactionArtifact, ContextCompactionArtifactQuery, ConversationBranchId,
    ConversationSnapshotId, CoreError, CoreErrorKind, CoreEvent, CoreEventKind, CoreResult,
    DataBankScopeId, DelegatedCompletion, DelegatedFanOutGroup, DelegationLineage,
    DenRuntimeReference, DurableAgentKind, DurableAgentRecord, DurableIdentityStatus,
    EngineStorageConfig, FanOutFailurePolicy, FanOutGroupStatus, IsoTimestamp,
    MemoryConflictPolicy, MemoryDiagnosticsPolicy, MemoryEvidenceKind, MemoryEvidenceRef,
    MemoryExportImportPolicy, MemoryFieldType, MemoryGovernanceDecisionInput,
    MemoryGovernanceDecisionKind, MemoryGovernanceDecisionRecord, MemoryGovernanceMode,
    MemoryIndexingPolicy, MemoryOperation, MemoryOperationPolicy, MemoryPromptPolicy,
    MemoryProposalEnvelope, MemoryProposalQuery, MemoryProposalRecord, MemoryProposalReviewStatus,
    MemoryProposalSource, MemoryProvenancePolicy, MemoryRecordFieldDescriptor,
    MemoryRecordShapeDescriptor, MemoryRecordShapeId, MemoryRecordShapeRef, MemoryRetentionPolicy,
    MemoryRetrievalStrategy, MemoryScope, MemoryScopeModel, MemoryScopeType, MemorySpaceDescriptor,
    MemorySpaceId, MemoryVisibilityModel, MemoryWritePolicy, MessageBlockId, MessageId,
    MessageSlotId, MessageVariantId, ModelProviderCredential, ModelProviderProtocol,
    ModelProviderQuery, ModelProviderRecord, ModelProviderSecretEnvelope, ModelProviderStatus,
    ModelProviderWrite, ParentConsumptionPolicy, ProfileId, ProfilePurgeReport,
    ProfilePurgeTableCount, ProfileRegistryLifecycleStatus, ProfileRegistryLifecycleUpdate,
    ProfileRegistryRecord, ProfileRegistryUpdate, ProfileRegistryWrite, ProjectId,
    ProviderStateAbsenceReason, ResourceLimits, RunId, SessionActivityDigest,
    SessionActivityDigestQuery, SessionConfig, SessionHandle, SessionHistoryWindow, SessionId,
    SessionIdentityRecord, SessionKind, SessionState, SessionStatus, SourceSystemReference, TaskId,
    ToolCallMetadata, ToolProfile,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[cfg(feature = "postgres")]
pub(crate) use crate::sqlite_provider_wire_state::validate_provider_wire_state_key;
pub(crate) use repos::conversations::load_conversation_branch;
pub(crate) use repos::events::{event_agent_ids, event_session_ids};
pub(crate) use repos::memory::compact_session_memory_records_in_tx;
#[cfg(feature = "postgres")]
pub(crate) use repos::memory::{
    profile_memory_target_parts, validate_memory_proposal, validate_profile_memory_key,
    validate_profile_memory_write, validate_session_memory_content,
    validate_session_memory_record_id, validate_session_memory_shape,
    validate_session_memory_write,
};
pub(crate) use repos::queued_messages::{
    expire_queued_messages_in_tx, load_queued_messages, purge_terminal_queued_messages_in_tx,
    save_queued_message_in_tx,
};
#[cfg(feature = "postgres")]
pub(crate) use repos::roleplay_lore::{
    default_lore_layer_config, estimate_lore_tokens, excluded_subject_match,
    lore_recall_config_snapshot, normalized_optional_text, parse_roleplay_lore_canon_status,
    parse_roleplay_lore_layer_purpose, parse_roleplay_lore_layer_write_policy,
    parse_roleplay_lore_record_status, parse_roleplay_lore_visibility,
    roleplay_lore_canon_status_as_str, roleplay_lore_layer_purpose_as_str,
    roleplay_lore_layer_write_policy_as_str, roleplay_lore_memory_space_descriptor,
    roleplay_lore_record_status_as_str, roleplay_lore_visibility_as_str, score_lore_recall_entry,
    validate_lore_recall_query, validate_lore_recall_trace_query,
    validate_roleplay_chat_layers_write, validate_roleplay_lore_entry_promotion,
    validate_roleplay_lore_fact_capture, validate_roleplay_lore_identifier,
    validate_roleplay_lore_layer_config_write, validate_roleplay_lore_layer_entry_link,
    validate_roleplay_lore_layer_update, validate_roleplay_lore_layer_write,
    validate_roleplay_lore_record_id, validate_roleplay_lore_write, validate_unique_roleplay_ids,
};
pub(crate) use repos::runtime_counters::{
    increment_counter_for_scopes_in_tx, increment_event_counters_in_tx, load_runtime_counters,
    query_runtime_counters, reset_runtime_counters, RuntimeCounterRepository,
    COUNTER_QUEUE_EXPIRATIONS,
};
#[cfg(feature = "postgres")]
pub(crate) use repos::service_config::{
    validate_model_provider_alias, validate_model_provider_write, validate_profile_registry_id,
    validate_profile_registry_write,
};

const DB_FILE_NAME: &str = "coordination.sqlite3";

#[derive(Debug, Clone)]
pub struct CoordinationStore {
    conn: Arc<Mutex<Connection>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreCoordinationStoreBackend {
    Sqlite,
    Postgres,
}

#[derive(Debug, Clone)]
pub enum CoreCoordinationStore {
    Sqlite(CoordinationStore),
    #[cfg(feature = "postgres")]
    Postgres(Arc<postgres_backend::PostgresBackendStore>),
}

impl CoordinationStore {
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
}

impl CoordinationStore {
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
}

fn branch_head_message_id_in_tx(
    tx: &rusqlite::Transaction<'_>,
    branch_id: &str,
) -> CoreResult<String> {
    tx.query_row(
        "SELECT COALESCE(head_message_id, origin_message_id, branch_id)
         FROM conversation_branches
         WHERE branch_id = ?1",
        params![branch_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(|error| persistence_error("load branch head for session memory compaction", error))?
    .ok_or_else(|| {
        CoreError::new(
            CoreErrorKind::NotFound,
            format!("branch {branch_id} not found for session memory compaction"),
        )
    })
}

fn session_exists_in_tx(
    tx: &rusqlite::Transaction<'_>,
    session_id: &SessionId,
) -> CoreResult<bool> {
    tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM sessions WHERE session_id = ?1)",
        params![session_id.0.as_str()],
        |row| row.get::<_, i64>(0),
    )
    .map(|value| value != 0)
    .map_err(|error| persistence_error("check session exists", error))
}

fn session_id_for_conversation_branch_in_tx(
    tx: &rusqlite::Transaction<'_>,
    branch_id: &ConversationBranchId,
) -> CoreResult<Option<SessionId>> {
    tx.query_row(
        "SELECT session_id FROM conversation_branches WHERE branch_id = ?1",
        params![branch_id.0.as_str()],
        |row| Ok(SessionId::new(row.get::<_, String>(0)?)),
    )
    .optional()
    .map_err(|error| persistence_error("load session id for conversation branch", error))
}

fn validate_memory_confidence(value: f32) -> CoreResult<()> {
    if !(0.0..=1.0).contains(&value) || value.is_nan() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "memory confidence must be between 0 and 1",
        ));
    }
    Ok(())
}

fn validate_non_negative_finite(label: &str, value: f32) -> CoreResult<()> {
    if !value.is_finite() || value < 0.0 {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("{label} must be a non-negative finite number"),
        ));
    }
    Ok(())
}

fn sql_bool(value: i64) -> bool {
    value != 0
}

fn sqlite_like_contains(value: &str) -> String {
    format!("%{}%", escape_sqlite_like(value))
}

fn escape_sqlite_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
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

fn parse_memory_scope_type(raw: &str) -> CoreResult<MemoryScopeType> {
    match raw {
        "profile" => Ok(MemoryScopeType::Profile),
        "user" => Ok(MemoryScopeType::User),
        "session" => Ok(MemoryScopeType::Session),
        "conversation_branch" => Ok(MemoryScopeType::ConversationBranch),
        "world" => Ok(MemoryScopeType::World),
        "entity" => Ok(MemoryScopeType::Entity),
        "project" => Ok(MemoryScopeType::Project),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("invalid memory scope type {other}"),
        )),
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

fn parse_memory_proposal_source(raw: &str) -> CoreResult<MemoryProposalSource> {
    match raw {
        "in_wake_tool" => Ok(MemoryProposalSource::InWakeTool),
        "capture_producer" => Ok(MemoryProposalSource::CaptureProducer),
        "ui" => Ok(MemoryProposalSource::Ui),
        "import" => Ok(MemoryProposalSource::Import),
        "migration" => Ok(MemoryProposalSource::Migration),
        "human" => Ok(MemoryProposalSource::Human),
        "den_memory_import" => Ok(MemoryProposalSource::DenMemoryImport),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("invalid memory proposal source {other}"),
        )),
    }
}

fn session_memory_status_as_str(status: SessionMemoryRecordStatus) -> &'static str {
    match status {
        SessionMemoryRecordStatus::Active => "active",
        SessionMemoryRecordStatus::Superseded => "superseded",
        SessionMemoryRecordStatus::Archived => "archived",
    }
}

fn parse_session_memory_status(raw: &str) -> CoreResult<SessionMemoryRecordStatus> {
    match raw {
        "active" => Ok(SessionMemoryRecordStatus::Active),
        "superseded" => Ok(SessionMemoryRecordStatus::Superseded),
        "archived" => Ok(SessionMemoryRecordStatus::Archived),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("invalid session memory status {other}"),
        )),
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
