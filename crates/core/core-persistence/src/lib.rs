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
#[cfg(test)]
mod sqlite_integration_tests;
mod sqlite_schema;
mod sqlite_simple_kv;
mod sqlite_store;

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

    pub fn validate_logical_storage_import(
        &self,
        bundle: &LogicalStorageExportBundle,
        dry_run: &LogicalStorageImportDryRun,
    ) -> CoreResult<LogicalStorageImportValidationReport> {
        let conn = self.conn()?;
        validate_logical_storage_import(&conn, bundle, dry_run)
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

    pub fn upsert_worker_pool_member(&self, record: &WorkerPoolMemberRecord) -> CoreResult<()> {
        let capabilities_json = to_json_text(&record.capabilities_json)?;
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO worker_pool_members (
                member_id,
                profile_id,
                agent_id,
                session_id,
                status,
                concurrency_limit,
                active_leases,
                capabilities_json,
                registered_at,
                last_heartbeat_at,
                updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ON CONFLICT(member_id) DO UPDATE SET
                profile_id = excluded.profile_id,
                agent_id = excluded.agent_id,
                session_id = excluded.session_id,
                status = excluded.status,
                concurrency_limit = excluded.concurrency_limit,
                active_leases = excluded.active_leases,
                capabilities_json = excluded.capabilities_json,
                last_heartbeat_at = excluded.last_heartbeat_at,
                updated_at = excluded.updated_at",
            params![
                record.member_id.as_str(),
                record.profile_id.0.as_str(),
                record.agent_id.as_ref().map(|agent_id| agent_id.0.as_str()),
                record
                    .session_id
                    .as_ref()
                    .map(|session_id| session_id.0.as_str()),
                record.status.as_str(),
                record.concurrency_limit as i64,
                record.active_leases as i64,
                capabilities_json,
                record.registered_at.as_str(),
                record.last_heartbeat_at.as_str(),
                record.updated_at.as_str(),
            ],
        )
        .map_err(|error| persistence_error("upsert worker pool member", error))?;
        Ok(())
    }

    pub fn heartbeat_worker_pool_member(
        &self,
        member_id: &str,
        status: WorkerPoolMemberStatus,
        now: &IsoTimestamp,
    ) -> CoreResult<bool> {
        let conn = self.conn()?;
        let rows = conn
            .execute(
                "UPDATE worker_pool_members
                 SET status = ?1, last_heartbeat_at = ?2, updated_at = ?2
                 WHERE member_id = ?3",
                params![status.as_str(), now.as_str(), member_id],
            )
            .map_err(|error| persistence_error("heartbeat worker pool member", error))?;
        Ok(rows > 0)
    }

    pub fn load_worker_pool_member(
        &self,
        member_id: &str,
    ) -> CoreResult<Option<WorkerPoolMemberRecord>> {
        let conn = self.conn()?;
        load_worker_pool_member_from_conn(&conn, member_id)
    }

    pub fn create_worker_pool_work_item(
        &self,
        record: &WorkerPoolWorkItemRecord,
    ) -> CoreResult<()> {
        let work_json = to_json_text(&record.work_json)?;
        let required_capabilities_json = to_json_text(&record.required_capabilities_json)?;
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO worker_pool_work_items (
                work_item_id,
                requested_profile_id,
                task_id,
                status,
                priority,
                work_json,
                required_capabilities_json,
                created_at,
                updated_at,
                claimed_by_member_id,
                lease_id,
                claim_token,
                claim_deadline_at,
                terminal_at,
                terminal_summary
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                record.work_item_id.as_str(),
                record
                    .requested_profile_id
                    .as_ref()
                    .map(|profile_id| profile_id.0.as_str()),
                record.task_id.as_ref().map(|task_id| task_id.0.as_str()),
                record.status.as_str(),
                record.priority as i64,
                work_json,
                required_capabilities_json,
                record.created_at.as_str(),
                record.updated_at.as_str(),
                record.claimed_by_member_id.as_deref(),
                record.lease_id.as_deref(),
                record.claim_token.as_deref(),
                record.claim_deadline_at.as_deref(),
                record.terminal_at.as_deref(),
                record.terminal_summary.as_deref(),
            ],
        )
        .map_err(|error| persistence_error("create worker pool work item", error))?;
        Ok(())
    }

    pub fn load_worker_pool_work_item(
        &self,
        work_item_id: &str,
    ) -> CoreResult<Option<WorkerPoolWorkItemRecord>> {
        let conn = self.conn()?;
        load_worker_pool_work_item_from_conn(&conn, work_item_id)
    }

    pub fn claim_next_worker_pool_work_item(
        &self,
        request: &WorkerPoolClaimRequest,
    ) -> CoreResult<Result<WorkerPoolClaimRecord, WorkerPoolNoCapacityReason>> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start worker pool claim transaction", error))?;

        let Some(member) = load_worker_pool_member_from_tx(&tx, &request.member_id)? else {
            return Ok(Err(WorkerPoolNoCapacityReason::MemberUnavailable));
        };
        if !member.status.can_claim() {
            return Ok(Err(WorkerPoolNoCapacityReason::MemberUnavailable));
        }
        if member.last_heartbeat_at < request.min_heartbeat_at {
            return Ok(Err(WorkerPoolNoCapacityReason::MemberHeartbeatStale));
        }
        if member.active_leases >= member.concurrency_limit {
            return Ok(Err(WorkerPoolNoCapacityReason::MemberAtCapacity));
        }

        let Some(mut work_item) = find_next_worker_pool_work_item_for_claim(&tx, &member)? else {
            return Ok(Err(WorkerPoolNoCapacityReason::NoPendingWork));
        };

        let rows = tx
            .execute(
                "UPDATE worker_pool_work_items
                 SET status = ?1,
                     claimed_by_member_id = ?2,
                     lease_id = ?3,
                     claim_token = ?4,
                     claim_deadline_at = ?5,
                     updated_at = ?6
                 WHERE work_item_id = ?7 AND status = ?8",
                params![
                    WorkerPoolWorkStatus::Claimed.as_str(),
                    member.member_id.as_str(),
                    request.lease_id.as_str(),
                    request.claim_token.as_str(),
                    request.claim_deadline_at.as_str(),
                    request.now.as_str(),
                    work_item.work_item_id.as_str(),
                    WorkerPoolWorkStatus::Pending.as_str(),
                ],
            )
            .map_err(|error| persistence_error("claim worker pool work item", error))?;
        if rows != 1 {
            return Ok(Err(WorkerPoolNoCapacityReason::NoPendingWork));
        }

        let lease = WorkerPoolLeaseRecord {
            lease_id: request.lease_id.clone(),
            work_item_id: work_item.work_item_id.clone(),
            member_id: member.member_id.clone(),
            claim_token: request.claim_token.clone(),
            status: WorkerPoolLeaseStatus::Active,
            claimed_at: request.now.clone(),
            claim_deadline_at: request.claim_deadline_at.clone(),
            terminal_at: None,
        };
        insert_worker_pool_lease(&tx, &lease)?;

        let next_active_leases = member.active_leases.saturating_add(1);
        let next_member_status = if next_active_leases >= member.concurrency_limit {
            WorkerPoolMemberStatus::Busy
        } else {
            member.status
        };
        tx.execute(
            "UPDATE worker_pool_members
             SET active_leases = ?1, status = ?2, updated_at = ?3
             WHERE member_id = ?4",
            params![
                next_active_leases as i64,
                next_member_status.as_str(),
                request.now.as_str(),
                member.member_id.as_str(),
            ],
        )
        .map_err(|error| persistence_error("update worker pool member claim count", error))?;
        insert_worker_pool_event(
            &tx,
            &work_item.work_item_id,
            Some(&lease.lease_id),
            Some(&member.member_id),
            "claimed",
            &serde_json::json!({
                "claim_deadline_at": request.claim_deadline_at,
                "profile_id": member.profile_id.0,
            }),
            &request.now,
        )?;
        tx.commit()
            .map_err(|error| persistence_error("commit worker pool claim", error))?;

        work_item.status = WorkerPoolWorkStatus::Claimed;
        work_item.updated_at = request.now.clone();
        work_item.claimed_by_member_id = Some(member.member_id.clone());
        work_item.lease_id = Some(lease.lease_id.clone());
        work_item.claim_token = Some(lease.claim_token.clone());
        work_item.claim_deadline_at = Some(lease.claim_deadline_at.clone());
        let mut updated_member = member;
        updated_member.active_leases = next_active_leases;
        updated_member.status = next_member_status;
        updated_member.updated_at = request.now.clone();

        Ok(Ok(WorkerPoolClaimRecord {
            member: updated_member,
            work_item,
            lease,
        }))
    }

    pub fn complete_worker_pool_work_item(
        &self,
        request: &WorkerPoolCompletionRequest,
    ) -> CoreResult<bool> {
        if !request.status.is_terminal() || request.status == WorkerPoolWorkStatus::Pending {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "worker pool completion must use a terminal work status",
            ));
        }

        let mut conn = self.conn()?;
        let tx = conn.transaction().map_err(|error| {
            persistence_error("start worker pool completion transaction", error)
        })?;
        let Some(lease) = load_worker_pool_lease_from_tx(&tx, &request.lease_id)? else {
            return Ok(false);
        };
        if lease.status != WorkerPoolLeaseStatus::Active || lease.claim_token != request.claim_token
        {
            return Ok(false);
        }
        let Some(work_item) = load_worker_pool_work_item_from_tx(&tx, &lease.work_item_id)? else {
            return Ok(false);
        };
        if work_item.status.is_terminal() {
            return Ok(false);
        }

        let lease_status = worker_pool_lease_status_for_work_status(request.status)?;
        let rows = tx
            .execute(
                "UPDATE worker_pool_work_items
                 SET status = ?1,
                     updated_at = ?2,
                     terminal_at = ?2,
                     terminal_summary = ?3
                 WHERE work_item_id = ?4
                   AND lease_id = ?5
                   AND claim_token = ?6
                   AND status IN (?7, ?8)",
                params![
                    request.status.as_str(),
                    request.now.as_str(),
                    request.summary.as_deref(),
                    work_item.work_item_id.as_str(),
                    lease.lease_id.as_str(),
                    request.claim_token.as_str(),
                    WorkerPoolWorkStatus::Claimed.as_str(),
                    WorkerPoolWorkStatus::Running.as_str(),
                ],
            )
            .map_err(|error| persistence_error("complete worker pool work item", error))?;
        if rows != 1 {
            return Ok(false);
        }

        tx.execute(
            "UPDATE worker_pool_leases
             SET status = ?1, terminal_at = ?2
             WHERE lease_id = ?3 AND status = ?4",
            params![
                lease_status.as_str(),
                request.now.as_str(),
                lease.lease_id.as_str(),
                WorkerPoolLeaseStatus::Active.as_str(),
            ],
        )
        .map_err(|error| persistence_error("complete worker pool lease", error))?;
        release_worker_pool_member_lease(&tx, &lease.member_id, &request.now)?;
        insert_worker_pool_event(
            &tx,
            &work_item.work_item_id,
            Some(&lease.lease_id),
            Some(&lease.member_id),
            request.status.as_str(),
            &serde_json::json!({ "summary": request.summary }),
            &request.now,
        )?;
        tx.commit()
            .map_err(|error| persistence_error("commit worker pool completion", error))?;
        Ok(true)
    }

    pub fn expire_worker_pool_claims(
        &self,
        stale_before: &IsoTimestamp,
        now: &IsoTimestamp,
    ) -> CoreResult<Vec<WorkerPoolWorkItemRecord>> {
        let mut conn = self.conn()?;
        let tx = conn.transaction().map_err(|error| {
            persistence_error("start worker pool claim expiry transaction", error)
        })?;
        let mut stmt = tx
            .prepare(
                "SELECT
                    work_item_id,
                    requested_profile_id,
                    task_id,
                    status,
                    priority,
                    work_json,
                    required_capabilities_json,
                    created_at,
                    updated_at,
                    claimed_by_member_id,
                    lease_id,
                    claim_token,
                    claim_deadline_at,
                    terminal_at,
                    terminal_summary
                 FROM worker_pool_work_items
                 WHERE status IN (?1, ?2)
                   AND claim_deadline_at IS NOT NULL
                   AND claim_deadline_at < ?3
                 ORDER BY claim_deadline_at ASC, work_item_id ASC",
            )
            .map_err(|error| persistence_error("prepare stale worker pool claims", error))?;
        let stale = stmt
            .query_map(
                params![
                    WorkerPoolWorkStatus::Claimed.as_str(),
                    WorkerPoolWorkStatus::Running.as_str(),
                    stale_before.as_str(),
                ],
                row_to_worker_pool_work_item,
            )
            .map_err(|error| persistence_error("query stale worker pool claims", error))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| persistence_error("load stale worker pool claims", error))?;
        drop(stmt);

        let mut expired = Vec::new();
        for mut item in stale {
            let rows = tx
                .execute(
                    "UPDATE worker_pool_work_items
                     SET status = ?1,
                         updated_at = ?2,
                         terminal_at = ?2,
                         terminal_summary = ?3
                     WHERE work_item_id = ?4
                       AND status IN (?5, ?6)",
                    params![
                        WorkerPoolWorkStatus::Expired.as_str(),
                        now.as_str(),
                        "worker pool claim expired",
                        item.work_item_id.as_str(),
                        WorkerPoolWorkStatus::Claimed.as_str(),
                        WorkerPoolWorkStatus::Running.as_str(),
                    ],
                )
                .map_err(|error| persistence_error("expire worker pool work item", error))?;
            if rows != 1 {
                continue;
            }
            if let Some(lease_id) = item.lease_id.as_deref() {
                tx.execute(
                    "UPDATE worker_pool_leases
                     SET status = ?1, terminal_at = ?2
                     WHERE lease_id = ?3 AND status = ?4",
                    params![
                        WorkerPoolLeaseStatus::Expired.as_str(),
                        now.as_str(),
                        lease_id,
                        WorkerPoolLeaseStatus::Active.as_str(),
                    ],
                )
                .map_err(|error| persistence_error("expire worker pool lease", error))?;
            }
            if let Some(member_id) = item.claimed_by_member_id.as_deref() {
                release_worker_pool_member_lease(&tx, member_id, now)?;
            }
            insert_worker_pool_event(
                &tx,
                &item.work_item_id,
                item.lease_id.as_deref(),
                item.claimed_by_member_id.as_deref(),
                WorkerPoolWorkStatus::Expired.as_str(),
                &serde_json::json!({ "reason": "claim_deadline_expired" }),
                now,
            )?;
            item.status = WorkerPoolWorkStatus::Expired;
            item.updated_at = now.clone();
            item.terminal_at = Some(now.clone());
            item.terminal_summary = Some("worker pool claim expired".to_string());
            expired.push(item);
        }

        tx.commit()
            .map_err(|error| persistence_error("commit worker pool expiry", error))?;
        Ok(expired)
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

fn validate_logical_storage_import(
    conn: &Connection,
    bundle: &LogicalStorageExportBundle,
    dry_run: &LogicalStorageImportDryRun,
) -> CoreResult<LogicalStorageImportValidationReport> {
    if dry_run.import_batch_id.trim().is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "logical import dry-run requires an import_batch_id",
        ));
    }
    if dry_run.target_backend.trim().is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "logical import dry-run requires a target_backend",
        ));
    }

    let mut issues = Vec::new();
    let mut accepted_records = 0_u64;
    let mut unsupported_records = 0_u64;
    let mut refused_records = 0_u64;
    let record_count = bundle
        .repositories
        .iter()
        .map(|repository| repository.records.len() as u64)
        .sum();
    let supported_capabilities = if dry_run.supported_capabilities.is_empty() {
        sqlite_storage_capabilities()
            .into_iter()
            .filter(|capability| capability.supported)
            .map(|capability| capability.name)
            .collect::<BTreeSet<_>>()
    } else {
        dry_run
            .supported_capabilities
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>()
    };
    let supported_repositories = dry_run
        .supported_repositories
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();

    if bundle.bundle_version != 1 {
        issues.push(logical_import_issue(
            LogicalStorageImportIssueSeverity::Error,
            "unsupported_bundle_version",
            None,
            None,
            format!(
                "logical storage bundle version {} is not supported",
                bundle.bundle_version
            ),
        ));
    }
    if bundle.export_id.trim().is_empty() {
        issues.push(logical_import_issue(
            LogicalStorageImportIssueSeverity::Error,
            "missing_export_id",
            None,
            None,
            "logical storage bundle requires an export_id",
        ));
    }

    let already_imported = import_batch_exists(conn, &dry_run.import_batch_id)?;
    if already_imported {
        issues.push(logical_import_issue(
            LogicalStorageImportIssueSeverity::Info,
            "import_batch_already_recorded",
            None,
            None,
            format!(
                "import batch {} is already recorded; validation is idempotent and will not apply records",
                dry_run.import_batch_id
            ),
        ));
    }

    for repository in &bundle.repositories {
        let repository_supported = supported_repositories.is_empty()
            || supported_repositories.contains(&repository.repository_id);
        let missing_capabilities = repository
            .required_capabilities
            .iter()
            .filter(|capability| !supported_capabilities.contains(*capability))
            .cloned()
            .collect::<Vec<_>>();

        if !repository_supported {
            unsupported_records += repository.records.len() as u64;
            issues.push(logical_import_issue(
                LogicalStorageImportIssueSeverity::Error,
                "unsupported_repository",
                Some(repository.repository_id.clone()),
                None,
                format!(
                    "target backend {} does not declare support for repository {}",
                    dry_run.target_backend, repository.repository_id
                ),
            ));
            continue;
        }

        if !missing_capabilities.is_empty() {
            unsupported_records += repository.records.len() as u64;
            issues.push(logical_import_issue(
                LogicalStorageImportIssueSeverity::Error,
                "missing_storage_capability",
                Some(repository.repository_id.clone()),
                None,
                format!(
                    "target backend {} is missing required capabilities: {}",
                    dry_run.target_backend,
                    missing_capabilities.join(", ")
                ),
            ));
            continue;
        }

        if repository.exported_count != repository.records.len() as u64 {
            issues.push(logical_import_issue(
                LogicalStorageImportIssueSeverity::Warning,
                "repository_count_mismatch",
                Some(repository.repository_id.clone()),
                None,
                format!(
                    "repository {} declared {} records but contains {} records",
                    repository.repository_id,
                    repository.exported_count,
                    repository.records.len()
                ),
            ));
        }

        for record in &repository.records {
            match validate_logical_storage_record(repository, record, &dry_run.validation_time) {
                Ok(()) => accepted_records += 1,
                Err(issue) => {
                    refused_records += 1;
                    issues.push(issue);
                }
            }
        }
    }

    Ok(LogicalStorageImportValidationReport {
        import_batch_id: dry_run.import_batch_id.clone(),
        dry_run: true,
        source_backend: bundle.source.backend.clone(),
        target_backend: dry_run.target_backend.clone(),
        repository_count: bundle.repositories.len() as u64,
        record_count,
        accepted_records,
        unsupported_records,
        refused_records,
        already_imported,
        issues,
    })
}

fn validate_logical_storage_record(
    repository: &LogicalStorageRepositoryBundle,
    record: &LogicalStorageRecord,
    now: &IsoTimestamp,
) -> Result<(), LogicalStorageImportIssue> {
    if record.stable_id.trim().is_empty() {
        return Err(logical_import_issue(
            LogicalStorageImportIssueSeverity::Error,
            "missing_stable_id",
            Some(repository.repository_id.clone()),
            None,
            "logical import record requires a stable_id",
        ));
    }

    match &record.payload {
        LogicalStorageRecordPayload::QueueMessage(message) => {
            validate_logical_queue_message(repository, record, message.as_ref(), now)
        }
        LogicalStorageRecordPayload::TypedJson { .. } => Ok(()),
    }
}

fn validate_logical_queue_message(
    repository: &LogicalStorageRepositoryBundle,
    record: &LogicalStorageRecord,
    message: &LogicalQueuedMessageExportRecord,
    now: &IsoTimestamp,
) -> Result<(), LogicalStorageImportIssue> {
    if repository.repository_id != "queues_messages" {
        return Err(logical_import_issue(
            LogicalStorageImportIssueSeverity::Error,
            "queue_record_in_wrong_repository",
            Some(repository.repository_id.clone()),
            Some(record.stable_id.clone()),
            "queue message records must be grouped under queues_messages",
        ));
    }
    if message.state == QueuedMessageState::Pending && message.expires_at <= *now {
        return Err(logical_import_issue(
            LogicalStorageImportIssueSeverity::Error,
            "queue_pending_expired_would_resurrect",
            Some(repository.repository_id.clone()),
            Some(record.stable_id.clone()),
            "pending queue message is already expired at validation time and must not be imported as deliverable work",
        ));
    }
    if message.state == QueuedMessageState::Pending && message.terminal_at.is_some() {
        return Err(logical_import_issue(
            LogicalStorageImportIssueSeverity::Error,
            "queue_pending_has_terminal_at",
            Some(repository.repository_id.clone()),
            Some(record.stable_id.clone()),
            "pending queue message cannot carry terminal_at",
        ));
    }
    if message.state != QueuedMessageState::Pending && message.terminal_at.is_none() {
        return Err(logical_import_issue(
            LogicalStorageImportIssueSeverity::Error,
            "queue_terminal_missing_terminal_at",
            Some(repository.repository_id.clone()),
            Some(record.stable_id.clone()),
            "terminal queue message must preserve terminal_at so it cannot be resurrected",
        ));
    }
    Ok(())
}

fn import_batch_exists(conn: &Connection, import_batch_id: &str) -> CoreResult<bool> {
    conn.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM runtime_import_batches WHERE import_batch_id = ?1
        )",
        params![import_batch_id],
        |row| row.get::<_, i64>(0),
    )
    .map(|value| value != 0)
    .map_err(|error| persistence_error("check runtime import batch", error))
}

fn logical_import_issue(
    severity: LogicalStorageImportIssueSeverity,
    code: impl Into<String>,
    repository_id: Option<String>,
    record_id: Option<String>,
    message: impl Into<String>,
) -> LogicalStorageImportIssue {
    LogicalStorageImportIssue {
        severity,
        code: code.into(),
        repository_id,
        record_id,
        message: message.into(),
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

fn row_to_worker_pool_member(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkerPoolMemberRecord> {
    let status: String = row.get(4)?;
    let capabilities_json: String = row.get(7)?;
    Ok(WorkerPoolMemberRecord {
        member_id: row.get(0)?,
        profile_id: ProfileId(row.get(1)?),
        agent_id: row.get::<_, Option<String>>(2)?.map(AgentId),
        session_id: row.get::<_, Option<String>>(3)?.map(SessionId),
        status: worker_pool_member_status_from_str(&status)?,
        concurrency_limit: row.get::<_, i64>(5)? as u32,
        active_leases: row.get::<_, i64>(6)? as u32,
        capabilities_json: parse_json_record(&capabilities_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                7,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        registered_at: row.get(8)?,
        last_heartbeat_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn row_to_worker_pool_work_item(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<WorkerPoolWorkItemRecord> {
    let status: String = row.get(3)?;
    let work_json: String = row.get(5)?;
    let required_capabilities_json: String = row.get(6)?;
    Ok(WorkerPoolWorkItemRecord {
        work_item_id: row.get(0)?,
        requested_profile_id: row.get::<_, Option<String>>(1)?.map(ProfileId),
        task_id: row.get::<_, Option<String>>(2)?.map(TaskId),
        status: worker_pool_work_status_from_str(&status)?,
        priority: row.get::<_, i64>(4)? as i32,
        work_json: parse_json_record(&work_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                5,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        required_capabilities_json: parse_json_record(&required_capabilities_json).map_err(
            |error| {
                rusqlite::Error::FromSqlConversionFailure(
                    6,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            },
        )?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        claimed_by_member_id: row.get(9)?,
        lease_id: row.get(10)?,
        claim_token: row.get(11)?,
        claim_deadline_at: row.get(12)?,
        terminal_at: row.get(13)?,
        terminal_summary: row.get(14)?,
    })
}

fn row_to_worker_pool_lease(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkerPoolLeaseRecord> {
    let status: String = row.get(4)?;
    Ok(WorkerPoolLeaseRecord {
        lease_id: row.get(0)?,
        work_item_id: row.get(1)?,
        member_id: row.get(2)?,
        claim_token: row.get(3)?,
        status: worker_pool_lease_status_from_str(&status)?,
        claimed_at: row.get(5)?,
        claim_deadline_at: row.get(6)?,
        terminal_at: row.get(7)?,
    })
}

fn worker_pool_member_status_from_str(raw: &str) -> rusqlite::Result<WorkerPoolMemberStatus> {
    match raw {
        "available" => Ok(WorkerPoolMemberStatus::Available),
        "busy" => Ok(WorkerPoolMemberStatus::Busy),
        "offline" => Ok(WorkerPoolMemberStatus::Offline),
        "quarantined" => Ok(WorkerPoolMemberStatus::Quarantined),
        "retired" => Ok(WorkerPoolMemberStatus::Retired),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            4,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown worker pool member status {other}"),
            )),
        )),
    }
}

fn worker_pool_work_status_from_str(raw: &str) -> rusqlite::Result<WorkerPoolWorkStatus> {
    match raw {
        "pending" => Ok(WorkerPoolWorkStatus::Pending),
        "claimed" => Ok(WorkerPoolWorkStatus::Claimed),
        "running" => Ok(WorkerPoolWorkStatus::Running),
        "completed" => Ok(WorkerPoolWorkStatus::Completed),
        "failed" => Ok(WorkerPoolWorkStatus::Failed),
        "blocked" => Ok(WorkerPoolWorkStatus::Blocked),
        "exhausted" => Ok(WorkerPoolWorkStatus::Exhausted),
        "cancelled" => Ok(WorkerPoolWorkStatus::Cancelled),
        "expired" => Ok(WorkerPoolWorkStatus::Expired),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            3,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown worker pool work status {other}"),
            )),
        )),
    }
}

fn worker_pool_lease_status_from_str(raw: &str) -> rusqlite::Result<WorkerPoolLeaseStatus> {
    match raw {
        "active" => Ok(WorkerPoolLeaseStatus::Active),
        "completed" => Ok(WorkerPoolLeaseStatus::Completed),
        "failed" => Ok(WorkerPoolLeaseStatus::Failed),
        "blocked" => Ok(WorkerPoolLeaseStatus::Blocked),
        "exhausted" => Ok(WorkerPoolLeaseStatus::Exhausted),
        "cancelled" => Ok(WorkerPoolLeaseStatus::Cancelled),
        "expired" => Ok(WorkerPoolLeaseStatus::Expired),
        "released" => Ok(WorkerPoolLeaseStatus::Released),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            4,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown worker pool lease status {other}"),
            )),
        )),
    }
}

fn worker_pool_lease_status_for_work_status(
    status: WorkerPoolWorkStatus,
) -> CoreResult<WorkerPoolLeaseStatus> {
    match status {
        WorkerPoolWorkStatus::Completed => Ok(WorkerPoolLeaseStatus::Completed),
        WorkerPoolWorkStatus::Failed => Ok(WorkerPoolLeaseStatus::Failed),
        WorkerPoolWorkStatus::Blocked => Ok(WorkerPoolLeaseStatus::Blocked),
        WorkerPoolWorkStatus::Exhausted => Ok(WorkerPoolLeaseStatus::Exhausted),
        WorkerPoolWorkStatus::Cancelled => Ok(WorkerPoolLeaseStatus::Cancelled),
        WorkerPoolWorkStatus::Expired => Ok(WorkerPoolLeaseStatus::Expired),
        WorkerPoolWorkStatus::Pending
        | WorkerPoolWorkStatus::Claimed
        | WorkerPoolWorkStatus::Running => Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "worker pool lease terminal status requires terminal work status",
        )),
    }
}

fn load_worker_pool_member_from_conn(
    conn: &Connection,
    member_id: &str,
) -> CoreResult<Option<WorkerPoolMemberRecord>> {
    conn.query_row(
        "SELECT
            member_id,
            profile_id,
            agent_id,
            session_id,
            status,
            concurrency_limit,
            active_leases,
            capabilities_json,
            registered_at,
            last_heartbeat_at,
            updated_at
         FROM worker_pool_members
         WHERE member_id = ?1",
        params![member_id],
        row_to_worker_pool_member,
    )
    .optional()
    .map_err(|error| persistence_error("load worker pool member", error))
}

fn load_worker_pool_member_from_tx(
    tx: &rusqlite::Transaction<'_>,
    member_id: &str,
) -> CoreResult<Option<WorkerPoolMemberRecord>> {
    tx.query_row(
        "SELECT
            member_id,
            profile_id,
            agent_id,
            session_id,
            status,
            concurrency_limit,
            active_leases,
            capabilities_json,
            registered_at,
            last_heartbeat_at,
            updated_at
         FROM worker_pool_members
         WHERE member_id = ?1",
        params![member_id],
        row_to_worker_pool_member,
    )
    .optional()
    .map_err(|error| persistence_error("load worker pool member in tx", error))
}

fn load_worker_pool_work_item_from_conn(
    conn: &Connection,
    work_item_id: &str,
) -> CoreResult<Option<WorkerPoolWorkItemRecord>> {
    conn.query_row(
        WORKER_POOL_WORK_ITEM_SELECT_BY_ID,
        params![work_item_id],
        row_to_worker_pool_work_item,
    )
    .optional()
    .map_err(|error| persistence_error("load worker pool work item", error))
}

fn load_worker_pool_work_item_from_tx(
    tx: &rusqlite::Transaction<'_>,
    work_item_id: &str,
) -> CoreResult<Option<WorkerPoolWorkItemRecord>> {
    tx.query_row(
        WORKER_POOL_WORK_ITEM_SELECT_BY_ID,
        params![work_item_id],
        row_to_worker_pool_work_item,
    )
    .optional()
    .map_err(|error| persistence_error("load worker pool work item in tx", error))
}

fn find_next_worker_pool_work_item_for_claim(
    tx: &rusqlite::Transaction<'_>,
    member: &WorkerPoolMemberRecord,
) -> CoreResult<Option<WorkerPoolWorkItemRecord>> {
    tx.query_row(
        "SELECT
            work_item_id,
            requested_profile_id,
            task_id,
            status,
            priority,
            work_json,
            required_capabilities_json,
            created_at,
            updated_at,
            claimed_by_member_id,
            lease_id,
            claim_token,
            claim_deadline_at,
            terminal_at,
            terminal_summary
         FROM worker_pool_work_items
         WHERE status = ?1
           AND (requested_profile_id IS NULL OR requested_profile_id = ?2)
         ORDER BY priority ASC, created_at ASC, work_item_id ASC
         LIMIT 1",
        params![
            WorkerPoolWorkStatus::Pending.as_str(),
            member.profile_id.0.as_str()
        ],
        row_to_worker_pool_work_item,
    )
    .optional()
    .map_err(|error| persistence_error("find next worker pool work item", error))
}

fn load_worker_pool_lease_from_tx(
    tx: &rusqlite::Transaction<'_>,
    lease_id: &str,
) -> CoreResult<Option<WorkerPoolLeaseRecord>> {
    tx.query_row(
        "SELECT
            lease_id,
            work_item_id,
            member_id,
            claim_token,
            status,
            claimed_at,
            claim_deadline_at,
            terminal_at
         FROM worker_pool_leases
         WHERE lease_id = ?1",
        params![lease_id],
        row_to_worker_pool_lease,
    )
    .optional()
    .map_err(|error| persistence_error("load worker pool lease", error))
}

fn insert_worker_pool_lease(
    tx: &rusqlite::Transaction<'_>,
    lease: &WorkerPoolLeaseRecord,
) -> CoreResult<()> {
    tx.execute(
        "INSERT INTO worker_pool_leases (
            lease_id,
            work_item_id,
            member_id,
            claim_token,
            status,
            claimed_at,
            claim_deadline_at,
            terminal_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            lease.lease_id.as_str(),
            lease.work_item_id.as_str(),
            lease.member_id.as_str(),
            lease.claim_token.as_str(),
            lease.status.as_str(),
            lease.claimed_at.as_str(),
            lease.claim_deadline_at.as_str(),
            lease.terminal_at.as_deref(),
        ],
    )
    .map_err(|error| persistence_error("insert worker pool lease", error))?;
    Ok(())
}

fn release_worker_pool_member_lease(
    tx: &rusqlite::Transaction<'_>,
    member_id: &str,
    now: &IsoTimestamp,
) -> CoreResult<()> {
    tx.execute(
        "UPDATE worker_pool_members
         SET active_leases = CASE WHEN active_leases > 0 THEN active_leases - 1 ELSE 0 END,
             status = CASE
                WHEN status IN (?1, ?2) THEN ?1
                ELSE status
             END,
             updated_at = ?3
         WHERE member_id = ?4",
        params![
            WorkerPoolMemberStatus::Available.as_str(),
            WorkerPoolMemberStatus::Busy.as_str(),
            now.as_str(),
            member_id,
        ],
    )
    .map_err(|error| persistence_error("release worker pool member lease", error))?;
    Ok(())
}

fn insert_worker_pool_event(
    tx: &rusqlite::Transaction<'_>,
    work_item_id: &str,
    lease_id: Option<&str>,
    member_id: Option<&str>,
    event_type: &str,
    event_json: &JsonValue,
    recorded_at: &IsoTimestamp,
) -> CoreResult<()> {
    let event_json = to_json_text(event_json)?;
    tx.execute(
        "INSERT INTO worker_pool_events (
            work_item_id,
            lease_id,
            member_id,
            event_type,
            event_json,
            recorded_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            work_item_id,
            lease_id,
            member_id,
            event_type,
            event_json,
            recorded_at.as_str(),
        ],
    )
    .map_err(|error| persistence_error("insert worker pool event", error))?;
    Ok(())
}

const WORKER_POOL_WORK_ITEM_SELECT_BY_ID: &str = "SELECT
    work_item_id,
    requested_profile_id,
    task_id,
    status,
    priority,
    work_json,
    required_capabilities_json,
    created_at,
    updated_at,
    claimed_by_member_id,
    lease_id,
    claim_token,
    claim_deadline_at,
    terminal_at,
    terminal_summary
 FROM worker_pool_work_items
 WHERE work_item_id = ?1";

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
