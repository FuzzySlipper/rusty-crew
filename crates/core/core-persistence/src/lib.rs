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
mod sqlite_schema;

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

impl CoreCoordinationStore {
    pub fn open_storage(
        engine_data_dir: impl AsRef<Path>,
        storage: Option<&EngineStorageConfig>,
    ) -> CoreResult<Self> {
        match storage {
            None | Some(EngineStorageConfig::Sqlite) => Self::open_sqlite(engine_data_dir),
            Some(EngineStorageConfig::Postgres {
                database_url,
                schema,
                max_connections,
                ..
            }) => Self::open_postgres_with_options(database_url, schema, *max_connections),
        }
    }

    pub fn open_sqlite(engine_data_dir: impl AsRef<Path>) -> CoreResult<Self> {
        Ok(Self::Sqlite(CoordinationStore::open(engine_data_dir)?))
    }

    pub fn open_sqlite_file(path: impl AsRef<Path>) -> CoreResult<Self> {
        Ok(Self::Sqlite(CoordinationStore::open_file(path)?))
    }

    #[cfg(feature = "postgres")]
    pub fn open_postgres(database_url: &str, schema: &str) -> CoreResult<Self> {
        Self::open_postgres_with_options(database_url, schema, None)
    }

    #[cfg(feature = "postgres")]
    pub fn open_postgres_with_options(
        database_url: &str,
        schema: &str,
        max_connections: Option<u32>,
    ) -> CoreResult<Self> {
        Ok(Self::Postgres(Arc::new(
            postgres_backend::PostgresBackendStore::connect_with_pool_options(
                database_url,
                schema,
                max_connections,
            )?,
        )))
    }

    #[cfg(not(feature = "postgres"))]
    pub fn open_postgres(_database_url: &str, _schema: &str) -> CoreResult<Self> {
        Err(CoreError::new(
            CoreErrorKind::AdapterUnavailable,
            "PostgreSQL coordination backend is not compiled into this build",
        ))
    }

    #[cfg(not(feature = "postgres"))]
    pub fn open_postgres_with_options(
        _database_url: &str,
        _schema: &str,
        _max_connections: Option<u32>,
    ) -> CoreResult<Self> {
        Err(CoreError::new(
            CoreErrorKind::AdapterUnavailable,
            "PostgreSQL coordination backend is not compiled into this build",
        ))
    }

    pub fn backend(&self) -> CoreCoordinationStoreBackend {
        match self {
            Self::Sqlite(_) => CoreCoordinationStoreBackend::Sqlite,
            #[cfg(feature = "postgres")]
            Self::Postgres(_) => CoreCoordinationStoreBackend::Postgres,
        }
    }

    pub fn coordination(&self) -> CoordinationRepositorySet<'_> {
        CoordinationRepositorySet { store: self }
    }

    pub fn service_data(&self) -> ServiceDataRepositorySet<'_> {
        ServiceDataRepositorySet { store: self }
    }

    pub fn conversation(&self) -> ConversationRepositorySet<'_> {
        ConversationRepositorySet { store: self }
    }

    pub fn memory(&self) -> MemoryRepositorySet<'_> {
        MemoryRepositorySet { store: self }
    }

    pub fn module_data(&self) -> ModuleDataRepositorySet<'_> {
        ModuleDataRepositorySet { store: self }
    }

    pub fn admin(&self) -> StorageAdminRepositorySet<'_> {
        StorageAdminRepositorySet { store: self }
    }

    pub fn sqlite_compat_store(&self) -> &CoordinationStore {
        match self {
            Self::Sqlite(sqlite) => sqlite,
            #[cfg(feature = "postgres")]
            Self::Postgres(_) => {
                panic!("sqlite_compat_store called on PostgreSQL coordination backend")
            }
        }
    }

    pub fn save_session(&self, state: &SessionState) -> CoreResult<()> {
        match self {
            Self::Sqlite(sqlite) => sqlite.save_session(state),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.save_session(state),
        }
    }

    pub fn save_session_with_config(
        &self,
        state: &SessionState,
        config: &SessionConfig,
    ) -> CoreResult<()> {
        match self {
            Self::Sqlite(sqlite) => sqlite.save_session_with_config(state, config),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.save_session_with_config(state, config),
        }
    }

    pub fn load_sessions(&self) -> CoreResult<Vec<SessionState>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.load_sessions(),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.load_sessions(),
        }
    }

    pub fn save_event(&self, sequence: u64, event: &CoreEvent) -> CoreResult<()> {
        match self {
            Self::Sqlite(sqlite) => sqlite.save_event(sequence, event),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.save_event(sequence, event),
        }
    }

    pub fn load_event_history(&self) -> CoreResult<Vec<PersistedEvent>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.load_event_history(),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.load_event_history(),
        }
    }

    pub fn load_tool_call_history(&self) -> CoreResult<Vec<ToolCallRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.load_tool_call_history(),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.load_tool_call_history(),
        }
    }

    pub fn save_queued_message(&self, record: &QueuedMessageRecord) -> CoreResult<()> {
        match self {
            Self::Sqlite(sqlite) => sqlite.save_queued_message(record),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.save_queued_message(record),
        }
    }

    pub fn expire_queued_messages_at(
        &self,
        now: &IsoTimestamp,
    ) -> CoreResult<Vec<QueuedMessageRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.expire_queued_messages_at(now),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.expire_queued_messages_at(now),
        }
    }

    pub fn load_queued_messages(
        &self,
        filter: &QueuedMessageFilter,
    ) -> CoreResult<Vec<QueuedMessageRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.load_queued_messages(filter),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.load_queued_messages(filter),
        }
    }

    pub fn delegated_completions_for_parent(
        &self,
        parent_session_id: &SessionId,
    ) -> CoreResult<Vec<DelegatedCompletion>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.delegated_completions_for_parent(parent_session_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => {
                postgres.delegated_completions_for_parent(parent_session_id)
            }
        }
    }

    pub fn fan_out_groups_for_parent(
        &self,
        parent_session_id: &SessionId,
    ) -> CoreResult<Vec<DelegatedFanOutGroup>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.fan_out_groups_for_parent(parent_session_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => {
                let runs = postgres.query_worker_runs(&WorkerRunQuery {
                    parent_session_id: Some(parent_session_id.clone()),
                    ..Default::default()
                })?;
                Ok(repos::worker_runs::aggregate_fan_out_groups(
                    runs.into_iter()
                        .filter(|run| run.fan_out_group_id.is_some())
                        .collect(),
                ))
            }
        }
    }

    pub fn load_provider_wire_state_for_wake(
        &self,
        lookup: &ProviderWireStateWakeLookup,
    ) -> CoreResult<ProviderWireStateWakeResult> {
        match self {
            Self::Sqlite(sqlite) => sqlite.load_provider_wire_state_for_wake(lookup),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.load_provider_wire_state_for_wake(lookup),
        }
    }

    pub fn save_provider_wire_state(&self, write: &ProviderWireStateWrite) -> CoreResult<()> {
        match self {
            Self::Sqlite(sqlite) => sqlite.save_provider_wire_state(write).map(|_| ()),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.save_provider_wire_state(write).map(|_| ()),
        }
    }

    pub fn clear_provider_wire_state(
        &self,
        key: &ProviderWireStateKey,
        now: &IsoTimestamp,
        reason: ProviderWireStateInvalidationReason,
    ) -> CoreResult<()> {
        match self {
            Self::Sqlite(sqlite) => sqlite
                .clear_provider_wire_state(key, now, reason)
                .map(|_| ()),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres
                .clear_provider_wire_state(key, now, reason)
                .map(|_| ()),
        }
    }

    pub fn list_provider_wire_state_diagnostics(
        &self,
        limit: u32,
    ) -> CoreResult<Vec<ProviderWireStateDiagnostic>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.list_provider_wire_state_diagnostics(limit),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.list_provider_wire_state_diagnostics(limit),
        }
    }

    pub fn count_rows(&self, table: &str) -> CoreResult<u64> {
        match self {
            Self::Sqlite(sqlite) => sqlite.count_rows(table),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => {
                let table = DiagnosticTable::parse(table)?.as_str().to_string();
                postgres
                    .storage_diagnostics()?
                    .table_counts
                    .into_iter()
                    .find(|count| count.table == table)
                    .map(|count| count.rows)
                    .ok_or_else(|| {
                        CoreError::new(
                            CoreErrorKind::InvalidInput,
                            format!("unsupported PostgreSQL diagnostic table {table}"),
                        )
                    })
            }
        }
    }

    pub fn database_size(&self) -> CoreResult<RuntimeDatabaseSize> {
        match self {
            Self::Sqlite(sqlite) => sqlite.database_size(),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.database_size(),
        }
    }

    pub fn storage_diagnostics(&self) -> CoreResult<RuntimeStorageDiagnostics> {
        match self {
            Self::Sqlite(sqlite) => sqlite.storage_diagnostics(),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => {
                let diagnostics = postgres.storage_diagnostics()?;
                let size = postgres.database_size()?;
                let module_registry = self.storage_schema()?;
                Ok(RuntimeStorageDiagnostics {
                    backend: diagnostics.backend,
                    backend_label: diagnostics.backend_label,
                    schema_version: diagnostics.schema_version,
                    supported_schema_version: diagnostics.supported_schema_version,
                    migrations: diagnostics.migrations,
                    size,
                    table_counts: diagnostics.table_counts,
                    capabilities: diagnostics.capabilities,
                    repository_groups: diagnostics.repository_groups,
                    connection_health: diagnostics.connection_health,
                    module_registry,
                    index_checks: Vec::new(),
                    search_healthy: true,
                    pressure_signals: Vec::new(),
                    pressure: false,
                })
            }
        }
    }

    pub fn storage_schema(&self) -> CoreResult<RuntimeModuleSchemaRegistryDiagnostics> {
        match self {
            Self::Sqlite(sqlite) => sqlite.storage_schema(),
            #[cfg(feature = "postgres")]
            Self::Postgres(_) => {
                let registry = compiled_module_schema_registry();
                let installed = registry
                    .bundles()
                    .iter()
                    .map(|bundle| {
                        Ok(InstalledModuleSchemaRecord {
                            module_id: bundle.module_id.clone(),
                            installed_version: bundle.schema_version,
                            descriptor_fingerprint: bundle.descriptor_fingerprint()?,
                            installed_at: "postgres_active_migration".to_string(),
                            updated_at: "postgres_active_migration".to_string(),
                        })
                    })
                    .collect::<CoreResult<Vec<_>>>()?;
                module_schema_registry_diagnostics(
                    &registry,
                    &installed,
                    &postgres_module_schema_capabilities(),
                )
            }
        }
    }

    pub fn list_profile_registry_records(
        &self,
        query: &ProfileRegistryQuery,
    ) -> CoreResult<Vec<ProfileRegistryRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.list_profile_registry_records(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.list_profile_registry_records(query),
        }
    }

    pub fn create_profile_registry_record(
        &self,
        write: &ProfileRegistryWrite,
    ) -> CoreResult<ProfileRegistryRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.create_profile_registry_record(write),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.create_profile_registry_record(write),
        }
    }

    pub fn update_profile_registry_record(
        &self,
        update: &ProfileRegistryUpdate,
    ) -> CoreResult<ProfileRegistryRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.update_profile_registry_record(update),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.update_profile_registry_record(update),
        }
    }

    pub fn get_profile_registry_record(
        &self,
        profile_id: &ProfileId,
    ) -> CoreResult<Option<ProfileRegistryRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.get_profile_registry_record(profile_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.get_profile_registry_record(profile_id),
        }
    }

    pub fn purge_profile(&self, profile_id: &ProfileId) -> CoreResult<ProfilePurgeReport> {
        match self {
            Self::Sqlite(sqlite) => sqlite.purge_profile(profile_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.purge_profile(profile_id),
        }
    }

    pub fn upsert_model_provider(
        &self,
        write: &ModelProviderWrite,
    ) -> CoreResult<ModelProviderRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.upsert_model_provider(write),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.upsert_model_provider(write),
        }
    }

    pub fn get_model_provider(&self, alias: &str) -> CoreResult<Option<ModelProviderRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.get_model_provider(alias),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.get_model_provider(alias),
        }
    }

    pub fn get_model_provider_secret(&self, alias: &str) -> CoreResult<Option<String>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.get_model_provider_secret(alias),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.get_model_provider_secret(alias),
        }
    }

    pub fn list_model_providers(
        &self,
        query: &ModelProviderQuery,
    ) -> CoreResult<Vec<ModelProviderRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.list_model_providers(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.list_model_providers(query),
        }
    }

    pub fn add_roleplay_lore_record(
        &self,
        write: &RoleplayLoreWrite,
    ) -> CoreResult<RoleplayLoreRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.add_roleplay_lore_record(write),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.add_roleplay_lore_record(write),
        }
    }

    pub fn replace_roleplay_lore_record(
        &self,
        replace: &RoleplayLoreReplace,
    ) -> CoreResult<RoleplayLoreRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.replace_roleplay_lore_record(replace),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.replace_roleplay_lore_record(replace),
        }
    }

    pub fn supersede_roleplay_lore_record(
        &self,
        supersede: &RoleplayLoreSupersede,
    ) -> CoreResult<(RoleplayLoreRecord, RoleplayLoreRecord)> {
        match self {
            Self::Sqlite(sqlite) => sqlite.supersede_roleplay_lore_record(supersede),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.supersede_roleplay_lore_record(supersede),
        }
    }

    pub fn tombstone_roleplay_lore_record(
        &self,
        tombstone: &RoleplayLoreTombstone,
    ) -> CoreResult<RoleplayLoreRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.tombstone_roleplay_lore_record(tombstone),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.tombstone_roleplay_lore_record(tombstone),
        }
    }

    pub fn query_roleplay_lore_records(
        &self,
        query: &RoleplayLoreQuery,
    ) -> CoreResult<Vec<RoleplayLoreRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.query_roleplay_lore_records(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.query_roleplay_lore_records(query),
        }
    }

    pub fn get_roleplay_lore_record(
        &self,
        record_id: &str,
    ) -> CoreResult<Option<RoleplayLoreRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.get_roleplay_lore_record(record_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.get_roleplay_lore_record(record_id),
        }
    }

    pub fn roleplay_lore_provenance_events(
        &self,
        record_id: &str,
    ) -> CoreResult<Vec<RoleplayLoreProvenanceEvent>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.roleplay_lore_provenance_events(record_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.roleplay_lore_provenance_events(record_id),
        }
    }

    pub fn create_lore_layer(
        &self,
        write: &RoleplayLoreLayerWrite,
    ) -> CoreResult<RoleplayLoreLayerRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.create_lore_layer(write),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.create_lore_layer(write),
        }
    }

    pub fn get_lore_layer(&self, layer_id: &str) -> CoreResult<Option<RoleplayLoreLayerRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.get_lore_layer(layer_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.get_lore_layer(layer_id),
        }
    }

    pub fn list_lore_layers_by_profile(
        &self,
        profile_id: &str,
    ) -> CoreResult<Vec<RoleplayLoreLayerRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.list_lore_layers_by_profile(profile_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.list_lore_layers_by_profile(profile_id),
        }
    }

    pub fn update_lore_layer(
        &self,
        update: &RoleplayLoreLayerUpdate,
    ) -> CoreResult<RoleplayLoreLayerRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.update_lore_layer(update),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.update_lore_layer(update),
        }
    }

    pub fn archive_lore_layer(
        &self,
        archive: &RoleplayLoreLayerArchive,
    ) -> CoreResult<RoleplayLoreLayerRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.archive_lore_layer(archive),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.archive_lore_layer(archive),
        }
    }

    pub fn get_lore_layer_config(
        &self,
        layer_id: &str,
    ) -> CoreResult<Option<RoleplayLoreLayerConfigRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.get_lore_layer_config(layer_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.get_lore_layer_config(layer_id),
        }
    }

    pub fn set_lore_layer_config(
        &self,
        write: &RoleplayLoreLayerConfigWrite,
    ) -> CoreResult<RoleplayLoreLayerConfigRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.set_lore_layer_config(write),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.set_lore_layer_config(write),
        }
    }

    pub fn add_entry_to_layer(&self, link: &RoleplayLoreLayerEntryLink) -> CoreResult<()> {
        match self {
            Self::Sqlite(sqlite) => sqlite.add_entry_to_layer(link),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.add_entry_to_layer(link),
        }
    }

    pub fn capture_lore_fact(
        &self,
        capture: &RoleplayLoreFactCapture,
    ) -> CoreResult<RoleplayLoreLayerEntryJoin> {
        match self {
            Self::Sqlite(sqlite) => sqlite.capture_lore_fact(capture),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.capture_lore_fact(capture),
        }
    }

    pub fn promote_lore_entry(
        &self,
        promotion: &RoleplayLoreEntryPromotion,
    ) -> CoreResult<RoleplayLoreLayerEntryJoin> {
        match self {
            Self::Sqlite(sqlite) => sqlite.promote_lore_entry(promotion),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.promote_lore_entry(promotion),
        }
    }

    pub fn remove_entry_from_layer(&self, layer_id: &str, record_id: &str) -> CoreResult<()> {
        match self {
            Self::Sqlite(sqlite) => sqlite.remove_entry_from_layer(layer_id, record_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.remove_entry_from_layer(layer_id, record_id),
        }
    }

    pub fn set_entry_constant(
        &self,
        layer_id: &str,
        record_id: &str,
        is_constant: bool,
    ) -> CoreResult<()> {
        match self {
            Self::Sqlite(sqlite) => sqlite.set_entry_constant(layer_id, record_id, is_constant),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => {
                postgres.set_entry_constant(layer_id, record_id, is_constant)
            }
        }
    }

    pub fn list_entries_by_layer(
        &self,
        layer_id: &str,
    ) -> CoreResult<Vec<RoleplayLoreLayerEntryJoin>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.list_entries_by_layer(layer_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.list_entries_by_layer(layer_id),
        }
    }

    pub fn set_chat_layers(&self, write: &RoleplayChatLayersWrite) -> CoreResult<()> {
        match self {
            Self::Sqlite(sqlite) => sqlite.set_chat_layers(write),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.set_chat_layers(write),
        }
    }

    pub fn get_chat_layers(&self, chat_id: &str) -> CoreResult<Vec<RoleplayChatLayerRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.get_chat_layers(chat_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.get_chat_layers(chat_id),
        }
    }

    pub fn toggle_chat_layer(
        &self,
        chat_id: &str,
        layer_id: &str,
        enabled: bool,
    ) -> CoreResult<()> {
        match self {
            Self::Sqlite(sqlite) => sqlite.toggle_chat_layer(chat_id, layer_id, enabled),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.toggle_chat_layer(chat_id, layer_id, enabled),
        }
    }

    pub fn reorder_chat_layers(&self, chat_id: &str, layer_ids: &[String]) -> CoreResult<()> {
        match self {
            Self::Sqlite(sqlite) => sqlite.reorder_chat_layers(chat_id, layer_ids),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.reorder_chat_layers(chat_id, layer_ids),
        }
    }

    pub fn recall_lore(&self, query: &LoreRecallQuery) -> CoreResult<LoreRecallResult> {
        match self {
            Self::Sqlite(sqlite) => sqlite.recall_lore(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.recall_lore(query),
        }
    }

    pub fn list_recall_traces(
        &self,
        query: &LoreRecallTraceQuery,
    ) -> CoreResult<Vec<LoreRecallTraceRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.list_recall_traces(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.list_recall_traces(query),
        }
    }

    pub fn get_recall_trace(&self, trace_id: &str) -> CoreResult<Option<LoreRecallTraceRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.get_recall_trace(trace_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.get_recall_trace(trace_id),
        }
    }

    pub fn list_simple_kv(&self, query: &SimpleKvQuery) -> CoreResult<Vec<SimpleKvRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.list_simple_kv(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.list_simple_kv(query),
        }
    }

    pub fn put_simple_kv(&self, write: &SimpleKvWrite) -> CoreResult<SimpleKvRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.put_simple_kv(write),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.put_simple_kv(write),
        }
    }

    pub fn delete_simple_kv(&self, delete: &SimpleKvDelete) -> CoreResult<SimpleKvRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.delete_simple_kv(delete),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.delete_simple_kv(delete),
        }
    }

    pub fn run_maintenance(
        &self,
        policy: &RuntimeMaintenancePolicy,
    ) -> CoreResult<RuntimeMaintenanceReport> {
        match self {
            Self::Sqlite(sqlite) => sqlite.run_maintenance(policy),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.run_maintenance(policy),
        }
    }

    pub fn save_message_slot(&self, slot: &MessageSlotWrite) -> CoreResult<()> {
        match self {
            Self::Sqlite(sqlite) => sqlite.save_message_slot(slot),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.save_message_slot(slot),
        }
    }

    pub fn save_message_variant(
        &self,
        variant: &MessageVariantWrite,
    ) -> CoreResult<MessageVariantRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.save_message_variant(variant),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.save_message_variant(variant),
        }
    }

    pub fn query_message_slots(
        &self,
        query: &MessageSlotQuery,
    ) -> CoreResult<Vec<MessageSlotRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.query_message_slots(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.query_message_slots(query),
        }
    }

    pub fn query_message_variants(
        &self,
        query: &MessageVariantQuery,
    ) -> CoreResult<Vec<MessageVariantRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.query_message_variants(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.query_message_variants(query),
        }
    }

    pub fn select_active_message_variant(
        &self,
        request: &SelectActiveVariantRequest,
    ) -> CoreResult<SelectActiveVariantResult> {
        match self {
            Self::Sqlite(sqlite) => sqlite.select_active_message_variant(request),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.select_active_message_variant(request),
        }
    }

    pub fn delete_message_variant(
        &self,
        slot_id: &MessageSlotId,
        variant_id: &MessageVariantId,
        updated_at: &IsoTimestamp,
    ) -> CoreResult<MessageSlotRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.delete_message_variant(slot_id, variant_id, updated_at),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => {
                postgres.delete_message_variant(slot_id, variant_id, updated_at)
            }
        }
    }

    pub fn reorder_message_variants(
        &self,
        slot_id: &MessageSlotId,
        ordered_variant_ids: &[MessageVariantId],
        updated_at: &IsoTimestamp,
    ) -> CoreResult<Vec<MessageVariantRecord>> {
        match self {
            Self::Sqlite(sqlite) => {
                sqlite.reorder_message_variants(slot_id, ordered_variant_ids, updated_at)
            }
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => {
                postgres.reorder_message_variants(slot_id, ordered_variant_ids, updated_at)
            }
        }
    }

    pub fn save_conversation_branch(
        &self,
        branch: &ConversationBranchWrite,
    ) -> CoreResult<ConversationBranchRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.save_conversation_branch(branch),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.save_conversation_branch(branch),
        }
    }

    pub fn query_conversation_branches(
        &self,
        query: &ConversationBranchQuery,
    ) -> CoreResult<Vec<ConversationBranchRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.query_conversation_branches(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.query_conversation_branches(query),
        }
    }

    pub fn get_conversation_branch_state(
        &self,
        session_id: &SessionId,
        default_updated_at: &IsoTimestamp,
    ) -> CoreResult<ConversationBranchStateRecord> {
        match self {
            Self::Sqlite(sqlite) => {
                sqlite.get_conversation_branch_state(session_id, default_updated_at)
            }
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => {
                postgres.get_conversation_branch_state(session_id, default_updated_at)
            }
        }
    }

    pub fn select_active_conversation_branch(
        &self,
        request: &SelectActiveBranchRequest,
    ) -> CoreResult<SelectActiveBranchResult> {
        match self {
            Self::Sqlite(sqlite) => sqlite.select_active_conversation_branch(request),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.select_active_conversation_branch(request),
        }
    }

    pub fn update_conversation_branch_head(
        &self,
        request: &UpdateBranchHeadRequest,
    ) -> CoreResult<UpdateBranchHeadResult> {
        match self {
            Self::Sqlite(sqlite) => sqlite.update_conversation_branch_head(request),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.update_conversation_branch_head(request),
        }
    }

    pub fn save_conversation_snapshot(
        &self,
        snapshot: &ConversationSnapshotWrite,
    ) -> CoreResult<ConversationSnapshotRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.save_conversation_snapshot(snapshot),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.save_conversation_snapshot(snapshot),
        }
    }

    pub fn query_conversation_snapshots(
        &self,
        query: &ConversationSnapshotQuery,
    ) -> CoreResult<Vec<ConversationSnapshotRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.query_conversation_snapshots(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.query_conversation_snapshots(query),
        }
    }

    pub fn resolve_conversation_jump(
        &self,
        request: &ConversationJumpRequest,
    ) -> CoreResult<ConversationJumpResult> {
        match self {
            Self::Sqlite(sqlite) => sqlite.resolve_conversation_jump(request),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.resolve_conversation_jump(request),
        }
    }

    pub fn save_attachment(&self, attachment: &AttachmentWrite) -> CoreResult<AttachmentRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.save_attachment(attachment),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.save_attachment(attachment),
        }
    }

    pub fn query_attachments(&self, query: &AttachmentQuery) -> CoreResult<Vec<AttachmentRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.query_attachments(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.query_attachments(query),
        }
    }

    pub fn remove_attachment(
        &self,
        attachment_id: &AttachmentId,
        updated_at: &IsoTimestamp,
    ) -> CoreResult<AttachmentRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.remove_attachment(attachment_id, updated_at),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.remove_attachment(attachment_id, updated_at),
        }
    }

    pub fn save_data_bank_scope(
        &self,
        scope: &DataBankScopeWrite,
    ) -> CoreResult<DataBankScopeRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.save_data_bank_scope(scope),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.save_data_bank_scope(scope),
        }
    }

    pub fn query_data_bank_scopes(
        &self,
        query: &DataBankScopeQuery,
    ) -> CoreResult<Vec<DataBankScopeRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.query_data_bank_scopes(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.query_data_bank_scopes(query),
        }
    }

    pub fn remove_data_bank_scope(
        &self,
        scope_id: &DataBankScopeId,
        updated_at: &IsoTimestamp,
    ) -> CoreResult<DataBankScopeRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.remove_data_bank_scope(scope_id, updated_at),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.remove_data_bank_scope(scope_id, updated_at),
        }
    }

    pub fn list_profile_memory(
        &self,
        query: &ProfileMemoryQuery,
    ) -> CoreResult<Vec<ProfileMemoryRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.list_profile_memory(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.list_profile_memory(query),
        }
    }
    pub fn get_profile_memory(
        &self,
        profile_id: &ProfileId,
        target: &ProfileMemoryTarget,
        key: &str,
    ) -> CoreResult<Option<ProfileMemoryRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.get_profile_memory(profile_id, target, key),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.get_profile_memory(profile_id, target, key),
        }
    }
    pub fn add_profile_memory(
        &self,
        write: &ProfileMemoryWrite,
        caps: &ProfileMemoryCaps,
    ) -> CoreResult<ProfileMemoryRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.add_profile_memory(write, caps),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.add_profile_memory(write, caps),
        }
    }
    pub fn replace_profile_memory(
        &self,
        replace: &ProfileMemoryReplace,
        caps: &ProfileMemoryCaps,
    ) -> CoreResult<ProfileMemoryRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.replace_profile_memory(replace, caps),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.replace_profile_memory(replace, caps),
        }
    }
    pub fn remove_profile_memory(
        &self,
        delete: &ProfileMemoryDelete,
    ) -> CoreResult<ProfileMemoryRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.remove_profile_memory(delete),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.remove_profile_memory(delete),
        }
    }
    pub fn query_session_memory_records(
        &self,
        query: &SessionMemoryQuery,
    ) -> CoreResult<Vec<SessionMemoryRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.query_session_memory_records(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.query_session_memory_records(query),
        }
    }
    pub fn build_session_memory_prompt_context(
        &self,
        query: &BranchAwareSessionMemoryQuery,
    ) -> CoreResult<SessionMemoryPromptContext> {
        match self {
            Self::Sqlite(sqlite) => sqlite.build_session_memory_prompt_context(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.build_session_memory_prompt_context(query),
        }
    }
    pub fn list_memory_proposals(
        &self,
        query: &MemoryProposalQuery,
    ) -> CoreResult<Vec<MemoryProposalRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.list_memory_proposals(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.list_memory_proposals(query),
        }
    }
    pub fn save_session_activity_digest(
        &self,
        digest: &SessionActivityDigest,
    ) -> CoreResult<SessionActivityDigest> {
        match self {
            Self::Sqlite(sqlite) => sqlite.save_session_activity_digest(digest),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.save_session_activity_digest(digest),
        }
    }
    pub fn list_session_activity_digests(
        &self,
        query: &SessionActivityDigestQuery,
    ) -> CoreResult<Vec<SessionActivityDigest>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.list_session_activity_digests(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.list_session_activity_digests(query),
        }
    }
    pub fn save_memory_proposal(
        &self,
        proposal: &MemoryProposalEnvelope,
        descriptor: &MemorySpaceDescriptor,
        now: &IsoTimestamp,
    ) -> CoreResult<MemoryProposalRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.save_memory_proposal(proposal, descriptor, now),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.save_memory_proposal(proposal, descriptor, now),
        }
    }
    pub fn record_memory_governance_decision(
        &self,
        decision: &MemoryGovernanceDecisionInput,
        now: &IsoTimestamp,
    ) -> CoreResult<MemoryGovernanceDecisionRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.record_memory_governance_decision(decision, now),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.record_memory_governance_decision(decision, now),
        }
    }

    pub fn save_context_compaction_artifact(
        &self,
        artifact: &ContextCompactionArtifact,
    ) -> CoreResult<ContextCompactionArtifact> {
        match self {
            Self::Sqlite(sqlite) => sqlite.save_context_compaction_artifact(artifact),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.save_context_compaction_artifact(artifact),
        }
    }

    pub fn list_context_compaction_artifacts(
        &self,
        query: &ContextCompactionArtifactQuery,
    ) -> CoreResult<Vec<ContextCompactionArtifact>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.list_context_compaction_artifacts(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.list_context_compaction_artifacts(query),
        }
    }

    pub fn search_runtime(
        &self,
        filter: &RuntimeSearchFilter,
    ) -> CoreResult<Vec<RuntimeSearchResult>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.search_runtime(filter),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.search_runtime(filter),
        }
    }

    fn runtime_counter_repository(&self) -> &dyn RuntimeCounterRepository {
        match self {
            Self::Sqlite(sqlite) => sqlite,
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.as_ref(),
        }
    }

    pub fn query_runtime_counters(
        &self,
        query: &RuntimeCounterQuery,
    ) -> CoreResult<Vec<RuntimeCounterRecord>> {
        self.runtime_counter_repository()
            .query_runtime_counters(query)
    }

    pub fn runtime_summary(&self, scope: &RuntimeCounterScope) -> CoreResult<RuntimeStateSummary> {
        self.runtime_counter_repository().runtime_summary(scope)
    }

    pub fn reset_runtime_counters(
        &self,
        query: &RuntimeCounterQuery,
        now: IsoTimestamp,
    ) -> CoreResult<u64> {
        self.runtime_counter_repository()
            .reset_runtime_counters(query, now)
    }

    pub fn upsert_scheduled_job(&self, record: &ScheduledJobRecord) -> CoreResult<()> {
        match self {
            Self::Sqlite(sqlite) => sqlite.upsert_scheduled_job(record),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.upsert_scheduled_job(record),
        }
    }

    pub fn load_scheduled_job(&self, job_id: &str) -> CoreResult<Option<ScheduledJobRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.load_scheduled_job(job_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.load_scheduled_job(job_id),
        }
    }

    pub fn query_scheduled_jobs(
        &self,
        query: &ScheduledJobQuery,
    ) -> CoreResult<Vec<ScheduledJobRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.query_scheduled_jobs(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.query_scheduled_jobs(query),
        }
    }

    pub fn pause_scheduled_job(&self, job_id: &str, now: &IsoTimestamp) -> CoreResult<()> {
        match self {
            Self::Sqlite(sqlite) => sqlite.pause_scheduled_job(job_id, now),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.pause_scheduled_job(job_id, now),
        }
    }

    pub fn resume_scheduled_job(
        &self,
        job_id: &str,
        next_due_at: &IsoTimestamp,
        now: &IsoTimestamp,
    ) -> CoreResult<()> {
        match self {
            Self::Sqlite(sqlite) => sqlite.resume_scheduled_job(job_id, next_due_at, now),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.resume_scheduled_job(job_id, next_due_at, now),
        }
    }

    pub fn claim_scheduled_run(
        &self,
        run: &ScheduledRunRecord,
        next_due_at: Option<&IsoTimestamp>,
    ) -> CoreResult<()> {
        match self {
            Self::Sqlite(sqlite) => sqlite.claim_scheduled_run(run, next_due_at),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.claim_scheduled_run(run, next_due_at),
        }
    }

    pub fn complete_scheduled_run(
        &self,
        run_id: &RunId,
        status: ScheduledRunStatus,
        completed_at: &IsoTimestamp,
        output_json: &JsonValue,
        error: Option<&str>,
    ) -> CoreResult<()> {
        match self {
            Self::Sqlite(sqlite) => {
                sqlite.complete_scheduled_run(run_id, status, completed_at, output_json, error)
            }
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => {
                postgres.complete_scheduled_run(run_id, status, completed_at, output_json, error)
            }
        }
    }

    pub fn query_scheduled_runs(
        &self,
        query: &ScheduledRunQuery,
    ) -> CoreResult<Vec<ScheduledRunRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.query_scheduled_runs(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.query_scheduled_runs(query),
        }
    }

    pub fn expire_stale_scheduled_runs(
        &self,
        stale_before: &IsoTimestamp,
        now: &IsoTimestamp,
    ) -> CoreResult<Vec<ScheduledRunRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.expire_stale_scheduled_runs(stale_before, now),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.expire_stale_scheduled_runs(stale_before, now),
        }
    }

    pub fn save_worker_run_requested(&self, record: &WorkerRunRecord) -> CoreResult<()> {
        match self {
            Self::Sqlite(sqlite) => sqlite.save_worker_run_requested(record),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.save_worker_run_requested(record),
        }
    }

    pub fn load_worker_run(&self, run_id: &RunId) -> CoreResult<Option<WorkerRunRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.load_worker_run(run_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.load_worker_run(run_id),
        }
    }

    pub fn load_worker_run_by_delegated_session(
        &self,
        delegated_session_id: &SessionId,
    ) -> CoreResult<Option<WorkerRunRecord>> {
        match self {
            Self::Sqlite(sqlite) => {
                sqlite.load_worker_run_by_delegated_session(delegated_session_id)
            }
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => {
                postgres.load_worker_run_by_delegated_session(delegated_session_id)
            }
        }
    }

    pub fn update_worker_run_status_by_delegated_session(
        &self,
        delegated_session_id: &SessionId,
        status: WorkerRunStatus,
        now: IsoTimestamp,
    ) -> CoreResult<()> {
        match self {
            Self::Sqlite(sqlite) => sqlite.update_worker_run_status_by_delegated_session(
                delegated_session_id,
                status,
                now,
            ),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.update_worker_run_status_by_delegated_session(
                delegated_session_id,
                status,
                now,
            ),
        }
    }

    pub fn update_worker_run_status(
        &self,
        run_id: &RunId,
        status: WorkerRunStatus,
        now: IsoTimestamp,
    ) -> CoreResult<()> {
        match self {
            Self::Sqlite(sqlite) => sqlite.update_worker_run_status(run_id, status, now),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.update_worker_run_status(run_id, status, now),
        }
    }

    pub fn worker_runs_for_fan_out_group(
        &self,
        parent_session_id: &SessionId,
        group_id: &str,
    ) -> CoreResult<Vec<WorkerRunRecord>> {
        match self {
            Self::Sqlite(sqlite) => {
                sqlite.worker_runs_for_fan_out_group(parent_session_id, group_id)
            }
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => Ok(postgres
                .query_worker_runs(&WorkerRunQuery {
                    parent_session_id: Some(parent_session_id.clone()),
                    ..Default::default()
                })?
                .into_iter()
                .filter(|run| run.fan_out_group_id.as_deref() == Some(group_id))
                .collect()),
        }
    }

    pub fn upsert_worker_pool_member(&self, record: &WorkerPoolMemberRecord) -> CoreResult<()> {
        match self {
            Self::Sqlite(sqlite) => sqlite.upsert_worker_pool_member(record),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.upsert_worker_pool_member(record),
        }
    }

    pub fn heartbeat_worker_pool_member(
        &self,
        member_id: &str,
        status: WorkerPoolMemberStatus,
        now: &IsoTimestamp,
    ) -> CoreResult<bool> {
        match self {
            Self::Sqlite(sqlite) => sqlite.heartbeat_worker_pool_member(member_id, status, now),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => {
                postgres.heartbeat_worker_pool_member(member_id, status, now)
            }
        }
    }

    pub fn load_worker_pool_member(
        &self,
        member_id: &str,
    ) -> CoreResult<Option<WorkerPoolMemberRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.load_worker_pool_member(member_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.load_worker_pool_member(member_id),
        }
    }

    pub fn create_worker_pool_work_item(
        &self,
        record: &WorkerPoolWorkItemRecord,
    ) -> CoreResult<()> {
        match self {
            Self::Sqlite(sqlite) => sqlite.create_worker_pool_work_item(record),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.create_worker_pool_work_item(record),
        }
    }

    pub fn load_worker_pool_work_item(
        &self,
        work_item_id: &str,
    ) -> CoreResult<Option<WorkerPoolWorkItemRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.load_worker_pool_work_item(work_item_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.load_worker_pool_work_item(work_item_id),
        }
    }

    pub fn claim_next_worker_pool_work_item(
        &self,
        request: &WorkerPoolClaimRequest,
    ) -> CoreResult<Result<WorkerPoolClaimRecord, WorkerPoolNoCapacityReason>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.claim_next_worker_pool_work_item(request),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.claim_next_worker_pool_work_item(request),
        }
    }

    pub fn complete_worker_pool_work_item(
        &self,
        request: &WorkerPoolCompletionRequest,
    ) -> CoreResult<bool> {
        match self {
            Self::Sqlite(sqlite) => sqlite.complete_worker_pool_work_item(request),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.complete_worker_pool_work_item(request),
        }
    }

    pub fn expire_worker_pool_claims(
        &self,
        stale_before: &IsoTimestamp,
        now: &IsoTimestamp,
    ) -> CoreResult<Vec<WorkerPoolWorkItemRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.expire_worker_pool_claims(stale_before, now),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.expire_worker_pool_claims(stale_before, now),
        }
    }
}

impl CoordinationRepositorySet<'_> {
    pub fn save_session(&self, state: &SessionState) -> CoreResult<()> {
        self.store.save_session(state)
    }

    pub fn save_session_with_config(
        &self,
        state: &SessionState,
        config: &SessionConfig,
    ) -> CoreResult<()> {
        self.store.save_session_with_config(state, config)
    }

    pub fn load_sessions(&self) -> CoreResult<Vec<SessionState>> {
        self.store.load_sessions()
    }

    pub fn save_event(&self, sequence: u64, event: &CoreEvent) -> CoreResult<()> {
        self.store.save_event(sequence, event)
    }

    pub fn load_event_history(&self) -> CoreResult<Vec<PersistedEvent>> {
        self.store.load_event_history()
    }

    pub fn save_queued_message(&self, record: &QueuedMessageRecord) -> CoreResult<()> {
        self.store.save_queued_message(record)
    }

    pub fn expire_queued_messages_at(
        &self,
        now: &IsoTimestamp,
    ) -> CoreResult<Vec<QueuedMessageRecord>> {
        self.store.expire_queued_messages_at(now)
    }

    pub fn load_queued_messages(
        &self,
        filter: &QueuedMessageFilter,
    ) -> CoreResult<Vec<QueuedMessageRecord>> {
        self.store.load_queued_messages(filter)
    }

    pub fn upsert_scheduled_job(&self, record: &ScheduledJobRecord) -> CoreResult<()> {
        self.store.upsert_scheduled_job(record)
    }

    pub fn query_scheduled_jobs(
        &self,
        query: &ScheduledJobQuery,
    ) -> CoreResult<Vec<ScheduledJobRecord>> {
        self.store.query_scheduled_jobs(query)
    }

    pub fn load_scheduled_job(&self, job_id: &str) -> CoreResult<Option<ScheduledJobRecord>> {
        self.store.load_scheduled_job(job_id)
    }

    pub fn pause_scheduled_job(&self, job_id: &str, now: &IsoTimestamp) -> CoreResult<()> {
        self.store.pause_scheduled_job(job_id, now)
    }

    pub fn resume_scheduled_job(
        &self,
        job_id: &str,
        next_due_at: &IsoTimestamp,
        now: &IsoTimestamp,
    ) -> CoreResult<()> {
        self.store.resume_scheduled_job(job_id, next_due_at, now)
    }

    pub fn claim_scheduled_run(
        &self,
        run: &ScheduledRunRecord,
        next_due_at: Option<&IsoTimestamp>,
    ) -> CoreResult<()> {
        self.store.claim_scheduled_run(run, next_due_at)
    }

    pub fn complete_scheduled_run(
        &self,
        run_id: &RunId,
        status: ScheduledRunStatus,
        completed_at: &IsoTimestamp,
        output_json: &JsonValue,
        error: Option<&str>,
    ) -> CoreResult<()> {
        self.store
            .complete_scheduled_run(run_id, status, completed_at, output_json, error)
    }

    pub fn query_scheduled_runs(
        &self,
        query: &ScheduledRunQuery,
    ) -> CoreResult<Vec<ScheduledRunRecord>> {
        self.store.query_scheduled_runs(query)
    }

    pub fn expire_stale_scheduled_runs(
        &self,
        stale_before: &IsoTimestamp,
        now: &IsoTimestamp,
    ) -> CoreResult<Vec<ScheduledRunRecord>> {
        self.store.expire_stale_scheduled_runs(stale_before, now)
    }

    pub fn save_worker_run_requested(&self, record: &WorkerRunRecord) -> CoreResult<()> {
        self.store.save_worker_run_requested(record)
    }

    pub fn load_worker_run(&self, run_id: &RunId) -> CoreResult<Option<WorkerRunRecord>> {
        self.store.load_worker_run(run_id)
    }

    pub fn load_worker_run_by_delegated_session(
        &self,
        session_id: &SessionId,
    ) -> CoreResult<Option<WorkerRunRecord>> {
        self.store.load_worker_run_by_delegated_session(session_id)
    }

    pub fn update_worker_run_status_by_delegated_session(
        &self,
        session_id: &SessionId,
        status: WorkerRunStatus,
        now: IsoTimestamp,
    ) -> CoreResult<()> {
        self.store
            .update_worker_run_status_by_delegated_session(session_id, status, now)
    }
}

impl ServiceDataRepositorySet<'_> {
    pub fn list_profile_registry_records(
        &self,
        query: &ProfileRegistryQuery,
    ) -> CoreResult<Vec<ProfileRegistryRecord>> {
        self.store.list_profile_registry_records(query)
    }

    pub fn create_profile_registry_record(
        &self,
        write: &ProfileRegistryWrite,
    ) -> CoreResult<ProfileRegistryRecord> {
        self.store.create_profile_registry_record(write)
    }

    pub fn update_profile_registry_record(
        &self,
        update: &ProfileRegistryUpdate,
    ) -> CoreResult<ProfileRegistryRecord> {
        self.store.update_profile_registry_record(update)
    }

    pub fn get_profile_registry_record(
        &self,
        profile_id: &ProfileId,
    ) -> CoreResult<Option<ProfileRegistryRecord>> {
        self.store.get_profile_registry_record(profile_id)
    }

    pub fn purge_profile(&self, profile_id: &ProfileId) -> CoreResult<ProfilePurgeReport> {
        self.store.purge_profile(profile_id)
    }

    pub fn upsert_model_provider(
        &self,
        write: &ModelProviderWrite,
    ) -> CoreResult<ModelProviderRecord> {
        self.store.upsert_model_provider(write)
    }

    pub fn get_model_provider(&self, alias: &str) -> CoreResult<Option<ModelProviderRecord>> {
        self.store.get_model_provider(alias)
    }

    pub fn get_model_provider_secret(&self, alias: &str) -> CoreResult<Option<String>> {
        self.store.get_model_provider_secret(alias)
    }

    pub fn list_model_providers(
        &self,
        query: &ModelProviderQuery,
    ) -> CoreResult<Vec<ModelProviderRecord>> {
        self.store.list_model_providers(query)
    }
}

impl ConversationRepositorySet<'_> {
    pub fn save_message_slot(&self, slot: &MessageSlotWrite) -> CoreResult<()> {
        self.store.save_message_slot(slot)
    }

    pub fn save_message_variant(
        &self,
        variant: &MessageVariantWrite,
    ) -> CoreResult<MessageVariantRecord> {
        self.store.save_message_variant(variant)
    }

    pub fn query_message_slots(
        &self,
        query: &MessageSlotQuery,
    ) -> CoreResult<Vec<MessageSlotRecord>> {
        self.store.query_message_slots(query)
    }

    pub fn query_message_variants(
        &self,
        query: &MessageVariantQuery,
    ) -> CoreResult<Vec<MessageVariantRecord>> {
        self.store.query_message_variants(query)
    }

    pub fn save_conversation_branch(
        &self,
        branch: &ConversationBranchWrite,
    ) -> CoreResult<ConversationBranchRecord> {
        self.store.save_conversation_branch(branch)
    }

    pub fn query_conversation_branches(
        &self,
        query: &ConversationBranchQuery,
    ) -> CoreResult<Vec<ConversationBranchRecord>> {
        self.store.query_conversation_branches(query)
    }

    pub fn get_conversation_branch_state(
        &self,
        session_id: &SessionId,
        default_updated_at: &IsoTimestamp,
    ) -> CoreResult<ConversationBranchStateRecord> {
        self.store
            .get_conversation_branch_state(session_id, default_updated_at)
    }

    pub fn select_active_conversation_branch(
        &self,
        request: &SelectActiveBranchRequest,
    ) -> CoreResult<SelectActiveBranchResult> {
        self.store.select_active_conversation_branch(request)
    }

    pub fn update_conversation_branch_head(
        &self,
        request: &UpdateBranchHeadRequest,
    ) -> CoreResult<UpdateBranchHeadResult> {
        self.store.update_conversation_branch_head(request)
    }

    pub fn save_conversation_snapshot(
        &self,
        snapshot: &ConversationSnapshotWrite,
    ) -> CoreResult<ConversationSnapshotRecord> {
        self.store.save_conversation_snapshot(snapshot)
    }

    pub fn query_conversation_snapshots(
        &self,
        query: &ConversationSnapshotQuery,
    ) -> CoreResult<Vec<ConversationSnapshotRecord>> {
        self.store.query_conversation_snapshots(query)
    }

    pub fn resolve_conversation_jump(
        &self,
        request: &ConversationJumpRequest,
    ) -> CoreResult<ConversationJumpResult> {
        self.store.resolve_conversation_jump(request)
    }

    pub fn save_attachment(&self, attachment: &AttachmentWrite) -> CoreResult<AttachmentRecord> {
        self.store.save_attachment(attachment)
    }

    pub fn query_attachments(&self, query: &AttachmentQuery) -> CoreResult<Vec<AttachmentRecord>> {
        self.store.query_attachments(query)
    }

    pub fn remove_attachment(
        &self,
        attachment_id: &AttachmentId,
        updated_at: &IsoTimestamp,
    ) -> CoreResult<AttachmentRecord> {
        self.store.remove_attachment(attachment_id, updated_at)
    }

    pub fn save_data_bank_scope(
        &self,
        scope: &DataBankScopeWrite,
    ) -> CoreResult<DataBankScopeRecord> {
        self.store.save_data_bank_scope(scope)
    }

    pub fn query_data_bank_scopes(
        &self,
        query: &DataBankScopeQuery,
    ) -> CoreResult<Vec<DataBankScopeRecord>> {
        self.store.query_data_bank_scopes(query)
    }

    pub fn remove_data_bank_scope(
        &self,
        scope_id: &DataBankScopeId,
        updated_at: &IsoTimestamp,
    ) -> CoreResult<DataBankScopeRecord> {
        self.store.remove_data_bank_scope(scope_id, updated_at)
    }

    pub fn select_active_message_variant(
        &self,
        request: &SelectActiveVariantRequest,
    ) -> CoreResult<SelectActiveVariantResult> {
        self.store.select_active_message_variant(request)
    }

    pub fn delete_message_variant(
        &self,
        slot_id: &MessageSlotId,
        variant_id: &MessageVariantId,
        updated_at: &IsoTimestamp,
    ) -> CoreResult<MessageSlotRecord> {
        self.store
            .delete_message_variant(slot_id, variant_id, updated_at)
    }

    pub fn reorder_message_variants(
        &self,
        slot_id: &MessageSlotId,
        ordered_variant_ids: &[MessageVariantId],
        updated_at: &IsoTimestamp,
    ) -> CoreResult<Vec<MessageVariantRecord>> {
        self.store
            .reorder_message_variants(slot_id, ordered_variant_ids, updated_at)
    }

    pub fn save_context_compaction_artifact(
        &self,
        artifact: &ContextCompactionArtifact,
    ) -> CoreResult<ContextCompactionArtifact> {
        self.store.save_context_compaction_artifact(artifact)
    }

    pub fn list_context_compaction_artifacts(
        &self,
        query: &ContextCompactionArtifactQuery,
    ) -> CoreResult<Vec<ContextCompactionArtifact>> {
        self.store.list_context_compaction_artifacts(query)
    }
}

impl MemoryRepositorySet<'_> {
    pub fn list_profile_memory(
        &self,
        query: &ProfileMemoryQuery,
    ) -> CoreResult<Vec<ProfileMemoryRecord>> {
        self.store.list_profile_memory(query)
    }

    pub fn add_profile_memory(
        &self,
        write: &ProfileMemoryWrite,
        caps: &ProfileMemoryCaps,
    ) -> CoreResult<ProfileMemoryRecord> {
        self.store.add_profile_memory(write, caps)
    }

    pub fn query_session_memory_records(
        &self,
        query: &SessionMemoryQuery,
    ) -> CoreResult<Vec<SessionMemoryRecord>> {
        self.store.query_session_memory_records(query)
    }

    pub fn build_session_memory_prompt_context(
        &self,
        query: &BranchAwareSessionMemoryQuery,
    ) -> CoreResult<SessionMemoryPromptContext> {
        self.store.build_session_memory_prompt_context(query)
    }

    pub fn save_memory_proposal(
        &self,
        envelope: &MemoryProposalEnvelope,
        descriptor: &MemorySpaceDescriptor,
        now: &IsoTimestamp,
    ) -> CoreResult<MemoryProposalRecord> {
        self.store.save_memory_proposal(envelope, descriptor, now)
    }

    pub fn list_memory_proposals(
        &self,
        query: &MemoryProposalQuery,
    ) -> CoreResult<Vec<MemoryProposalRecord>> {
        self.store.list_memory_proposals(query)
    }

    pub fn save_session_activity_digest(
        &self,
        digest: &SessionActivityDigest,
    ) -> CoreResult<SessionActivityDigest> {
        self.store.save_session_activity_digest(digest)
    }

    pub fn list_session_activity_digests(
        &self,
        query: &SessionActivityDigestQuery,
    ) -> CoreResult<Vec<SessionActivityDigest>> {
        self.store.list_session_activity_digests(query)
    }

    pub fn record_memory_governance_decision(
        &self,
        decision: &MemoryGovernanceDecisionInput,
        now: &IsoTimestamp,
    ) -> CoreResult<MemoryGovernanceDecisionRecord> {
        self.store.record_memory_governance_decision(decision, now)
    }

    pub fn get_profile_memory(
        &self,
        profile_id: &ProfileId,
        target: &ProfileMemoryTarget,
        key: &str,
    ) -> CoreResult<Option<ProfileMemoryRecord>> {
        self.store.get_profile_memory(profile_id, target, key)
    }

    pub fn add_roleplay_lore_record(
        &self,
        write: &RoleplayLoreWrite,
    ) -> CoreResult<RoleplayLoreRecord> {
        self.store.add_roleplay_lore_record(write)
    }

    pub fn replace_roleplay_lore_record(
        &self,
        replace: &RoleplayLoreReplace,
    ) -> CoreResult<RoleplayLoreRecord> {
        self.store.replace_roleplay_lore_record(replace)
    }

    pub fn query_roleplay_lore_records(
        &self,
        query: &RoleplayLoreQuery,
    ) -> CoreResult<Vec<RoleplayLoreRecord>> {
        self.store.query_roleplay_lore_records(query)
    }

    pub fn get_roleplay_lore_record(
        &self,
        record_id: &str,
    ) -> CoreResult<Option<RoleplayLoreRecord>> {
        self.store.get_roleplay_lore_record(record_id)
    }

    pub fn replace_profile_memory(
        &self,
        replace: &ProfileMemoryReplace,
        caps: &ProfileMemoryCaps,
    ) -> CoreResult<ProfileMemoryRecord> {
        self.store.replace_profile_memory(replace, caps)
    }

    pub fn remove_profile_memory(
        &self,
        delete: &ProfileMemoryDelete,
    ) -> CoreResult<ProfileMemoryRecord> {
        self.store.remove_profile_memory(delete)
    }
}

impl ModuleDataRepositorySet<'_> {
    pub fn list_simple_kv(&self, query: &SimpleKvQuery) -> CoreResult<Vec<SimpleKvRecord>> {
        self.store.list_simple_kv(query)
    }

    pub fn put_simple_kv(&self, write: &SimpleKvWrite) -> CoreResult<SimpleKvRecord> {
        self.store.put_simple_kv(write)
    }

    pub fn delete_simple_kv(&self, delete: &SimpleKvDelete) -> CoreResult<SimpleKvRecord> {
        self.store.delete_simple_kv(delete)
    }
}

impl StorageAdminRepositorySet<'_> {
    pub fn count_rows(&self, table: &str) -> CoreResult<u64> {
        self.store.count_rows(table)
    }

    pub fn database_size(&self) -> CoreResult<RuntimeDatabaseSize> {
        self.store.database_size()
    }

    pub fn storage_diagnostics(&self) -> CoreResult<RuntimeStorageDiagnostics> {
        self.store.storage_diagnostics()
    }

    pub fn storage_schema(&self) -> CoreResult<RuntimeModuleSchemaRegistryDiagnostics> {
        self.store.storage_schema()
    }

    pub fn run_maintenance(
        &self,
        policy: &RuntimeMaintenancePolicy,
    ) -> CoreResult<RuntimeMaintenanceReport> {
        self.store.run_maintenance(policy)
    }

    pub fn search_runtime(
        &self,
        filter: &RuntimeSearchFilter,
    ) -> CoreResult<Vec<RuntimeSearchResult>> {
        self.store.search_runtime(filter)
    }

    pub fn query_runtime_counters(
        &self,
        query: &RuntimeCounterQuery,
    ) -> CoreResult<Vec<RuntimeCounterRecord>> {
        self.store.query_runtime_counters(query)
    }

    pub fn runtime_summary(&self, scope: &RuntimeCounterScope) -> CoreResult<RuntimeStateSummary> {
        self.store.runtime_summary(scope)
    }

    pub fn reset_runtime_counters(
        &self,
        query: &RuntimeCounterQuery,
        now: IsoTimestamp,
    ) -> CoreResult<u64> {
        self.store.reset_runtime_counters(query, now)
    }
}

#[cfg(test)]
impl std::ops::Deref for CoreCoordinationStore {
    type Target = CoordinationStore;

    fn deref(&self) -> &Self::Target {
        self.sqlite_compat_store()
    }
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
        let pressure_signals =
            sqlite_storage_pressure_signals(&size, &table_counts, &index_checks, search_healthy);
        let pressure = pressure_signals.iter().any(|signal| signal.active);
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
            connection_health: RuntimeStorageConnectionHealth {
                backend: "sqlite".to_string(),
                status: "healthy".to_string(),
                max_connections: 1,
                active_connections: 0,
                idle_connections: 1,
                total_opened: 1,
                checkout_count: 0,
                checkout_reuse_count: 0,
                reconnect_attempts: 0,
                reconnect_successes: 0,
                closed_connections_discarded: 0,
                last_error: None,
            },
            module_registry,
            index_checks,
            search_healthy,
            pressure_signals,
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
        let mut session_memory_compaction = SessionMemoryCompactionReport::default();
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
            if let Some(now) = &policy.compact_session_memory_at {
                session_memory_compaction = compact_session_memory_records_in_tx(&tx, policy, now)?;
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
            session_memory_compaction,
            wal_checkpoint_ran: policy.run_wal_checkpoint,
            optimize_ran: policy.run_optimize,
        })
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

    pub fn count_rows(&self, table: &str) -> CoreResult<u64> {
        let table = DiagnosticTable::parse(table)?;

        let conn = self.conn()?;
        count_diagnostic_table_rows(&conn, table)
    }

    pub fn runtime_counters(
        &self,
        scope: Option<&RuntimeCounterScope>,
    ) -> CoreResult<Vec<RuntimeCounterRecord>> {
        RuntimeCounterRepository::runtime_counters(self, scope)
    }

    pub fn query_runtime_counters(
        &self,
        query: &RuntimeCounterQuery,
    ) -> CoreResult<Vec<RuntimeCounterRecord>> {
        RuntimeCounterRepository::query_runtime_counters(self, query)
    }

    pub fn reset_runtime_counters(
        &self,
        query: &RuntimeCounterQuery,
        now: IsoTimestamp,
    ) -> CoreResult<u64> {
        RuntimeCounterRepository::reset_runtime_counters(self, query, now)
    }

    pub fn runtime_summary(&self, scope: &RuntimeCounterScope) -> CoreResult<RuntimeStateSummary> {
        RuntimeCounterRepository::runtime_summary(self, scope)
    }

    pub fn schema_version(&self) -> CoreResult<i64> {
        let conn = self.conn()?;
        current_schema_version(&conn)
    }

    pub fn schema_migrations(&self) -> CoreResult<Vec<SchemaMigrationRecord>> {
        let conn = self.conn()?;
        load_schema_migration_records(&conn)
    }
}

impl RuntimeCounterRepository for CoordinationStore {
    #[cfg(test)]
    fn record_runtime_counter_delta(
        &self,
        scope: &RuntimeCounterScope,
        counter_name: &str,
        amount: u64,
        now: &IsoTimestamp,
    ) -> CoreResult<()> {
        let mut conn = self.conn()?;
        repos::runtime_counters::record_runtime_counter_delta(
            &mut conn,
            scope,
            counter_name,
            amount,
            now,
        )
    }

    fn runtime_counters(
        &self,
        scope: Option<&RuntimeCounterScope>,
    ) -> CoreResult<Vec<RuntimeCounterRecord>> {
        let conn = self.conn()?;
        load_runtime_counters(&conn, scope)
    }

    fn query_runtime_counters(
        &self,
        query: &RuntimeCounterQuery,
    ) -> CoreResult<Vec<RuntimeCounterRecord>> {
        let conn = self.conn()?;
        query_runtime_counters(&conn, query)
    }

    fn reset_runtime_counters(
        &self,
        query: &RuntimeCounterQuery,
        now: IsoTimestamp,
    ) -> CoreResult<u64> {
        let conn = self.conn()?;
        reset_runtime_counters(&conn, query, &now)
    }
}

impl CoordinationStore {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repos::runtime_counters::COUNTER_MESSAGES;
    use rusty_crew_core_protocol::{
        AgentMessage, MemoryConflictPolicy, MemoryDiagnosticsPolicy, MemoryEvidenceKind,
        MemoryEvidenceRef, MemoryExportImportPolicy, MemoryFieldType, MemoryIndexingPolicy,
        MemoryOperationPolicy, MemoryPromptPolicy, MemoryProvenancePolicy,
        MemoryRecordFieldDescriptor, MemoryRecordShapeDescriptor, MemoryRecordShapeId,
        MemoryRecordShapeRef, MemoryRetentionPolicy, MemoryRetrievalStrategy, MemoryScope,
        MemoryScopeModel, MemorySpaceId, MemoryVisibilityModel, MemoryWritePolicy,
        ModelProviderCredentialKind, ProfileRegistryDerivedRuntimeRef,
        ProfileRegistryImportExportMetadata, ProfileRegistrySourceAssetRef, ToolDescriptor,
        MODEL_PROVIDER_SECRET_ENVELOPE_VERSION,
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

        struct SqliteFacadeRepositoryConformance;

        impl RepositoryConformanceBackend for SqliteFacadeRepositoryConformance {
            fn with_store<F>(&self, label: &str, test: F)
            where
                F: FnOnce(&CoordinationStore),
            {
                let db_path = temp_db_path(&format!("sqlite-facade-conformance-{label}"));
                let store = CoreCoordinationStore::open_sqlite_file(&db_path).unwrap();
                assert_eq!(store.backend(), CoreCoordinationStoreBackend::Sqlite);
                test(store.sqlite_compat_store());
                remove_temp_db(&db_path);
            }
        }

        #[test]
        fn sqlite_satisfies_repository_conformance_suite() {
            run_repository_conformance_suite(&SqliteRepositoryConformance);
        }

        #[test]
        fn sqlite_facade_satisfies_repository_conformance_suite() {
            run_repository_conformance_suite(&SqliteFacadeRepositoryConformance);
        }

        #[test]
        fn sqlite_store_facades_expose_distinct_concern_boundaries() {
            let db_path = temp_db_path("sqlite-store-facades");
            let store = CoreCoordinationStore::open_sqlite_file(&db_path).unwrap();

            let state = sample_session_state();
            let config = sample_session_config();
            store
                .coordination()
                .save_session_with_config(&state, &config)
                .unwrap();
            assert_eq!(store.coordination().load_sessions().unwrap().len(), 1);

            let profile = store
                .service_data()
                .create_profile_registry_record(&profile_registry_write("facade-profile"))
                .unwrap();
            assert_eq!(profile.profile_id, ProfileId::new("facade-profile"));

            let scope = SimpleKvScope {
                scope_type: "profile".to_string(),
                scope_id: "facade-profile".to_string(),
            };
            store
                .module_data()
                .put_simple_kv(&SimpleKvWrite {
                    scope: scope.clone(),
                    key: "checkpoint".to_string(),
                    value_json: json!({"ok": true}),
                    now: "2026-07-02T00:00:00Z".to_string(),
                    expires_at: None,
                })
                .unwrap();
            assert_eq!(
                store
                    .module_data()
                    .list_simple_kv(&SimpleKvQuery {
                        scope,
                        key_prefix: Some("check".to_string()),
                        include_expired: false,
                        expired_only: false,
                        now: Some("2026-07-02T00:01:00Z".to_string()),
                        page: Some(page()),
                    })
                    .unwrap()
                    .len(),
                1
            );

            store
                .memory()
                .add_roleplay_lore_record(&roleplay_lore_write(
                    "facade-lore",
                    "facade-world",
                    None,
                    "Facade Lore",
                    "Facade memory/lore boundary survives restart.",
                    "2026-07-02T00:00:00Z",
                ))
                .unwrap();
            assert_eq!(
                store
                    .memory()
                    .query_roleplay_lore_records(&RoleplayLoreQuery {
                        world_id: Some("facade-world".to_string()),
                        ..RoleplayLoreQuery::default()
                    })
                    .unwrap()
                    .len(),
                1
            );

            assert!(store.admin().database_size().unwrap().database_bytes > 0);

            drop(store);
            let reopened = CoreCoordinationStore::open_sqlite_file(&db_path).unwrap();
            assert_eq!(reopened.coordination().load_sessions().unwrap().len(), 1);
            assert!(reopened
                .service_data()
                .get_profile_registry_record(&ProfileId::new("facade-profile"))
                .unwrap()
                .is_some());

            remove_temp_db(&db_path);
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
            model_provider_secret_envelope_contract(backend);
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
                                projection: None,
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
                        projection: None,
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
                                projection: None,
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
                                projection: None,
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
                            projection: None,
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

        fn model_provider_secret_envelope_contract<B: RepositoryConformanceBackend>(backend: &B) {
            backend.with_store("model-provider-secret-envelope", |store| {
                let api_key = store
                    .upsert_model_provider(&model_provider_write(
                        "deepseek-flash",
                        ModelProviderProtocol::ChatCompletions,
                        "deepseek",
                        "deepseek-chat",
                        Some("sk-legacy-api-key"),
                    ))
                    .unwrap();
                assert_eq!(
                    api_key.credential.kind,
                    Some(ModelProviderCredentialKind::ApiKey)
                );
                let stored_api_key = store
                    .get_model_provider_secret("deepseek-flash")
                    .unwrap()
                    .expect("stored API key secret");
                assert_ne!(stored_api_key, "sk-legacy-api-key");
                let api_key_envelope =
                    ModelProviderSecretEnvelope::from_storage_text(&stored_api_key).unwrap();
                assert_eq!(api_key_envelope.api_key_value(), Some("sk-legacy-api-key"));

                let oauth_secret = ModelProviderSecretEnvelope::OpenAiOauth {
                    version: MODEL_PROVIDER_SECRET_ENVELOPE_VERSION,
                    issuer: "https://auth.openai.com".to_string(),
                    client_id: "app-client".to_string(),
                    id_token: "id.jwt.token".to_string(),
                    access_token: "access.jwt.token".to_string(),
                    refresh_token: "refresh-token".to_string(),
                    exchanged_api_token: Some("exchanged-token".to_string()),
                    last_refresh_at: Some("2026-07-02T00:00:00Z".to_string()),
                    account_id: Some("account-1".to_string()),
                    email: Some("agent@example.test".to_string()),
                    plan_type: Some("pro".to_string()),
                    is_fedramp_account: false,
                    access_token_expires_at: Some("2026-07-02T01:00:00Z".to_string()),
                }
                .to_storage_text()
                .unwrap();
                let oauth = store
                    .upsert_model_provider(&model_provider_write(
                        "gpt-oauth",
                        ModelProviderProtocol::Responses,
                        "openai",
                        "gpt-5",
                        Some(&oauth_secret),
                    ))
                    .unwrap();
                assert_eq!(
                    oauth.credential.kind,
                    Some(ModelProviderCredentialKind::OpenAiOauth)
                );
                let stored_oauth = store
                    .get_model_provider_secret("gpt-oauth")
                    .unwrap()
                    .expect("stored OAuth secret");
                let oauth_envelope =
                    ModelProviderSecretEnvelope::from_storage_text(&stored_oauth).unwrap();
                assert_eq!(
                    oauth_envelope.kind(),
                    ModelProviderCredentialKind::OpenAiOauth
                );
                assert!(!serde_json::to_string(&oauth.credential)
                    .unwrap()
                    .contains("refresh-token"));
            });
        }
    }

    #[test]
    fn sqlite_small_roleplay_deployment_storage_proof() {
        let data_dir = temp_data_dir("small-roleplay-storage");
        let store = CoordinationStore::open(&data_dir).unwrap();
        let session_id = SessionId::new("session-alpha");
        let profile_id = ProfileId::new("full-profile");
        let now = "2026-06-26T00:00:00Z".to_string();

        store
            .create_profile_registry_record(&profile_registry_write("full-profile"))
            .unwrap();
        store
            .save_session_with_config(&sample_session_state(), &sample_session_config())
            .unwrap();

        let branch_id = ConversationBranchId::new("branch-roleplay-root");
        let root_message_id = MessageId::new("message-roleplay-root");
        store
            .save_conversation_branch(&ConversationBranchWrite {
                branch_id: branch_id.clone(),
                session_id: session_id.clone(),
                parent_branch_id: None,
                parent_message_id: None,
                origin_message_id: None,
                head_message_id: Some(root_message_id.clone()),
                label: Some("Roleplay Root".to_string()),
                metadata_json: json!({"deployment": "small_sqlite"}),
                created_at: now.clone(),
                updated_at: now.clone(),
            })
            .unwrap();

        let slot_id = MessageSlotId::new("slot-roleplay-root");
        let variant_id = MessageVariantId::new("variant-roleplay-primary");
        store
            .save_message_slot(&MessageSlotWrite {
                slot_id: slot_id.clone(),
                session_id: session_id.clone(),
                primary_variant_id: variant_id.clone(),
                active_variant_id: None,
                metadata_json: json!({"kind": "roleplay_turn"}),
                created_at: now.clone(),
                updated_at: now.clone(),
            })
            .unwrap();
        let mut variant = variant_write(
            &slot_id,
            &variant_id,
            MessageVariantSource::Primary,
            0,
            &root_message_id.0,
            "The moonlit tavern keeps a private lore ledger.",
        );
        variant.message.session_id = session_id.clone();
        variant.message.branch_id = Some(branch_id.clone());
        store.save_message_variant(&variant).unwrap();

        store
            .add_profile_memory(
                &ProfileMemoryWrite {
                    profile_id: profile_id.clone(),
                    target: ProfileMemoryTarget::User("player-1".to_string()),
                    key: "tone".to_string(),
                    content: "prefers slow-burn mystery with grounded sensory detail".to_string(),
                    metadata: json!({"source": "roleplay_smoke"}),
                    now: "2026-06-26T00:01:00Z".to_string(),
                },
                &ProfileMemoryCaps::default(),
            )
            .unwrap();

        store
            .save_event(
                1,
                &CoreEvent::AgentMessageRouted {
                    message: AgentMessage {
                        from: AgentId::new("player-1"),
                        to: AgentId::new("agent-alpha"),
                        body: "roleplay search needle: ask about the tavern ledger".to_string(),
                        correlation_id: Some("roleplay-search".to_string()),
                        projection: None,
                    },
                },
            )
            .unwrap();

        store
            .save_provider_wire_state(&sample_provider_wire_state_write(
                ProviderWireStateWriteFixture {
                    key: sample_provider_wire_state_key(),
                    profile_fingerprint: "profile:roleplay:v1",
                    provider_fingerprint: "provider:gpt:v1",
                    payload_version: "responses:v1",
                    payload_json: json!({"response_id": "resp_roleplay_root"}),
                    now: "2026-06-26T00:02:00Z",
                    expires_at: Some("2026-06-26T06:00:00Z"),
                    last_wake_id: Some("wake-roleplay"),
                },
            ))
            .unwrap();

        store
            .upsert_scheduled_job(&ScheduledJobRecord {
                job_id: "roleplay-maintenance".to_string(),
                job_kind: "maintenance".to_string(),
                target_session_id: Some(session_id.clone()),
                interval_ms: Some(300_000),
                next_due_at: Some("2026-06-26T00:05:00Z".to_string()),
                payload_json: json!({"mode": "small_sqlite"}),
                status: ScheduledJobStatus::Active,
                created_at: now.clone(),
                updated_at: now.clone(),
                paused_at: None,
            })
            .unwrap();

        let sessions = store.load_sessions().unwrap();
        let branches = store
            .query_conversation_branches(&ConversationBranchQuery {
                session_id: Some(session_id.clone()),
                parent_branch_id: None,
                page: None,
            })
            .unwrap();
        let slots = store
            .query_message_slots(&MessageSlotQuery {
                session_id: Some(session_id.clone()),
                include_alternates: false,
                page: None,
            })
            .unwrap();
        let memories = store
            .list_profile_memory(&ProfileMemoryQuery {
                profile_id,
                target: Some(ProfileMemoryTarget::User("player-1".to_string())),
                page: None,
            })
            .unwrap();
        let search = store
            .search_runtime(&RuntimeSearchFilter {
                query: "tavern".to_string(),
                row_type: Some(RuntimeSearchRowType::Message),
                session_id: None,
                agent_id: Some(AgentId::new("agent-alpha")),
                instance_id: None,
                task_id: None,
                event_kind: Some(CoreEventKind::AgentMessageRouted),
                recorded_after: None,
                recorded_before: None,
                limit: Some(10),
            })
            .unwrap();
        let provider = store
            .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
                key: sample_provider_wire_state_key(),
                profile_fingerprint: "profile:roleplay:v1".to_string(),
                provider_fingerprint: "provider:gpt:v1".to_string(),
                now: "2026-06-26T00:03:00Z".to_string(),
            })
            .unwrap();
        let scheduled = store
            .query_scheduled_jobs(&ScheduledJobQuery {
                status: Some(ScheduledJobStatus::Active),
                job_kind: Some("maintenance".to_string()),
                due_at_or_before: Some("2026-06-26T00:05:00Z".to_string()),
                page: None,
            })
            .unwrap();
        let before_maintenance = store.storage_diagnostics().unwrap();
        let maintenance = store
            .run_maintenance(&RuntimeMaintenancePolicy {
                run_wal_checkpoint: true,
                run_optimize: true,
                ..RuntimeMaintenancePolicy::default()
            })
            .unwrap();
        let after_maintenance = store.storage_diagnostics().unwrap();

        assert_eq!(sessions.len(), 1);
        assert_eq!(branches.len(), 1);
        assert_eq!(slots[0].primary.message.body, variant.message.body);
        assert_eq!(memories.len(), 1);
        assert_eq!(search.len(), 1);
        assert!(provider.record.unwrap().is_current());
        assert_eq!(scheduled.len(), 1);
        assert_eq!(before_maintenance.backend, "sqlite");
        assert!(before_maintenance.search_healthy);
        assert!(before_maintenance
            .capabilities
            .iter()
            .any(|capability| capability.name == "maintenance_checkpoint" && capability.supported));
        assert!(before_maintenance
            .capabilities
            .iter()
            .any(
                |capability| capability.name == "maintenance_vacuum_or_optimize"
                    && capability.supported
            ));
        assert!(before_maintenance
            .repository_groups
            .iter()
            .any(|group| group.group_id == "conversations_attachments"));
        assert!(before_maintenance
            .repository_groups
            .iter()
            .any(|group| group.group_id == "profile_memory"));
        assert!(maintenance.wal_checkpoint_ran);
        assert!(maintenance.optimize_ran);
        assert!(after_maintenance.size.wal_bytes < 64 * 1024 * 1024);

        remove_temp_dir(&data_dir);
    }

    #[test]
    fn roleplay_lore_layers_configs_entries_and_chat_links_round_trip() {
        let db_path = temp_db_path("roleplay-lore-layers");
        let store = CoordinationStore::open_file(&db_path).unwrap();

        let world_layer = store
            .create_lore_layer(&RoleplayLoreLayerWrite {
                layer_id: "layer-world".to_string(),
                profile_id: "profile-narrator".to_string(),
                name: "World Details".to_string(),
                description: Some("Durable world facts.".to_string()),
                purpose: RoleplayLoreLayerPurpose::World,
                write_policy: RoleplayLoreLayerWritePolicy::Manual,
                now: "2026-06-27T01:00:00Z".to_string(),
            })
            .unwrap();
        assert_eq!(world_layer.purpose, RoleplayLoreLayerPurpose::World);

        store
            .create_lore_layer(&RoleplayLoreLayerWrite {
                layer_id: "layer-story".to_string(),
                profile_id: "profile-narrator".to_string(),
                name: "Current Story".to_string(),
                description: None,
                purpose: RoleplayLoreLayerPurpose::Story,
                write_policy: RoleplayLoreLayerWritePolicy::AutoCapture,
                now: "2026-06-27T01:01:00Z".to_string(),
            })
            .unwrap();

        let updated = store
            .update_lore_layer(&RoleplayLoreLayerUpdate {
                layer_id: "layer-world".to_string(),
                name: Some("World Bible".to_string()),
                description: Some(None),
                purpose: Some(RoleplayLoreLayerPurpose::Mixed),
                write_policy: Some(RoleplayLoreLayerWritePolicy::Readonly),
                now: "2026-06-27T01:02:00Z".to_string(),
            })
            .unwrap();
        assert_eq!(updated.name, "World Bible");
        assert_eq!(updated.description, None);
        assert_eq!(updated.write_policy, RoleplayLoreLayerWritePolicy::Readonly);

        let config = store
            .set_lore_layer_config(&RoleplayLoreLayerConfigWrite {
                config_id: "config-world".to_string(),
                layer_id: "layer-world".to_string(),
                fts_weight: 1.25,
                subject_weight: 1.0,
                canon_weight: 0.75,
                tag_boost_weight: 0.5,
                recency_weight: 0.1,
                default_token_budget: 3200,
                constant_token_reserve: 400,
                min_relevance_score: 0.25,
                max_constants: 7,
                now: "2026-06-27T01:03:00Z".to_string(),
            })
            .unwrap();
        assert_eq!(config.max_constants, 7);
        assert_eq!(
            store
                .get_lore_layer_config("layer-world")
                .unwrap()
                .unwrap()
                .default_token_budget,
            3200
        );

        store
            .add_roleplay_lore_record(&roleplay_lore_write(
                "lore-tide-calendar",
                "world-moonlit",
                Some("entity-clockmaker"),
                "Tide Calendar",
                "The tide calendar opens the moon gate.",
                "2026-06-27T01:04:00Z",
            ))
            .unwrap();
        store
            .add_roleplay_lore_record(&roleplay_lore_write(
                "lore-brass-needle",
                "world-moonlit",
                Some("entity-clockmaker"),
                "Brass Needle",
                "The brass needle points to hidden observatory doors.",
                "2026-06-27T01:05:00Z",
            ))
            .unwrap();

        store
            .add_entry_to_layer(&RoleplayLoreLayerEntryLink {
                layer_id: "layer-world".to_string(),
                record_id: "lore-tide-calendar".to_string(),
                is_constant: false,
                priority: 10,
                added_at: "2026-06-27T01:06:00Z".to_string(),
            })
            .unwrap();
        store
            .add_entry_to_layer(&RoleplayLoreLayerEntryLink {
                layer_id: "layer-world".to_string(),
                record_id: "lore-brass-needle".to_string(),
                is_constant: true,
                priority: 0,
                added_at: "2026-06-27T01:07:00Z".to_string(),
            })
            .unwrap();

        let entries = store.list_entries_by_layer("layer-world").unwrap();
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.record_id.as_str())
                .collect::<Vec<_>>(),
            vec!["lore-brass-needle", "lore-tide-calendar"]
        );
        assert!(entries[0].is_constant);
        store
            .set_entry_constant("layer-world", "lore-tide-calendar", true)
            .unwrap();
        assert!(store
            .list_entries_by_layer("layer-world")
            .unwrap()
            .iter()
            .any(|entry| entry.record_id == "lore-tide-calendar" && entry.is_constant));
        store
            .remove_entry_from_layer("layer-world", "lore-brass-needle")
            .unwrap();
        assert_eq!(store.list_entries_by_layer("layer-world").unwrap().len(), 1);

        let mut captured_write = roleplay_lore_write(
            "lore-captured-orchard",
            "world-moonlit",
            Some("entity-clockmaker"),
            "Silver Orchard",
            "The silver orchard blooms after the clockmaker sings.",
            "2026-06-27T01:07:30Z",
        );
        captured_write.source = MemoryProposalSource::CaptureProducer;
        let captured = store
            .capture_lore_fact(&RoleplayLoreFactCapture {
                layer_id: "layer-story".to_string(),
                write: captured_write,
                is_constant: false,
                priority: 4,
                capture_reason: Some("observed in chat turn".to_string()),
            })
            .unwrap();
        assert_eq!(captured.layer_id, "layer-story");
        assert_eq!(captured.record.record_id, "lore-captured-orchard");
        assert_eq!(
            captured.record.source,
            MemoryProposalSource::CaptureProducer
        );
        assert_eq!(
            store
                .roleplay_lore_provenance_events("lore-captured-orchard")
                .unwrap()[0]
                .note
                .as_deref(),
            Some("observed in chat turn")
        );
        let mut invalid_capture = roleplay_lore_write(
            "lore-invalid-capture-target",
            "world-moonlit",
            None,
            "Invalid Capture",
            "This should not enter a manual layer.",
            "2026-06-27T01:07:31Z",
        );
        invalid_capture.source = MemoryProposalSource::CaptureProducer;
        assert!(store
            .capture_lore_fact(&RoleplayLoreFactCapture {
                layer_id: "layer-world".to_string(),
                write: invalid_capture,
                is_constant: false,
                priority: 0,
                capture_reason: None,
            })
            .is_err());

        assert!(store
            .promote_lore_entry(&RoleplayLoreEntryPromotion {
                source_layer_id: "layer-story".to_string(),
                source_record_id: "lore-captured-orchard".to_string(),
                target_layer_id: "layer-world".to_string(),
                new_record_id: "lore-promoted-orchard".to_string(),
                is_constant: false,
                priority: 2,
                now: "2026-06-27T01:07:40Z".to_string(),
            })
            .is_err());
        store
            .update_lore_layer(&RoleplayLoreLayerUpdate {
                layer_id: "layer-world".to_string(),
                name: None,
                description: None,
                purpose: None,
                write_policy: Some(RoleplayLoreLayerWritePolicy::Manual),
                now: "2026-06-27T01:07:41Z".to_string(),
            })
            .unwrap();
        let promoted = store
            .promote_lore_entry(&RoleplayLoreEntryPromotion {
                source_layer_id: "layer-story".to_string(),
                source_record_id: "lore-captured-orchard".to_string(),
                target_layer_id: "layer-world".to_string(),
                new_record_id: "lore-promoted-orchard".to_string(),
                is_constant: false,
                priority: 2,
                now: "2026-06-27T01:07:42Z".to_string(),
            })
            .unwrap();
        assert_eq!(promoted.layer_id, "layer-world");
        assert_eq!(promoted.record.record_id, "lore-promoted-orchard");
        assert_eq!(promoted.record.title, "Silver Orchard");
        assert_eq!(
            promoted.record.supersedes_record_id.as_deref(),
            Some("lore-captured-orchard")
        );
        let promoted_source = store
            .get_roleplay_lore_record("lore-captured-orchard")
            .unwrap()
            .unwrap();
        assert_eq!(promoted_source.status, RoleplayLoreRecordStatus::Superseded);
        assert_eq!(
            promoted_source.superseded_by_record_id.as_deref(),
            Some("lore-promoted-orchard")
        );
        assert_eq!(
            store
                .roleplay_lore_provenance_events("lore-promoted-orchard")
                .unwrap()[0]
                .note
                .as_deref(),
            Some("promoted from layer-story:lore-captured-orchard")
        );

        store
            .set_chat_layers(&RoleplayChatLayersWrite {
                chat_id: "chat-moonlit".to_string(),
                layers: vec![
                    RoleplayChatLayerLink {
                        layer_id: "layer-story".to_string(),
                        priority: 0,
                        enabled: true,
                    },
                    RoleplayChatLayerLink {
                        layer_id: "layer-world".to_string(),
                        priority: 1,
                        enabled: true,
                    },
                ],
                now: "2026-06-27T01:08:00Z".to_string(),
            })
            .unwrap();
        assert_eq!(
            store
                .get_chat_layers("chat-moonlit")
                .unwrap()
                .iter()
                .map(|layer| layer.layer_id.as_str())
                .collect::<Vec<_>>(),
            vec!["layer-story", "layer-world"]
        );
        store
            .toggle_chat_layer("chat-moonlit", "layer-world", false)
            .unwrap();
        assert!(
            !store
                .get_chat_layers("chat-moonlit")
                .unwrap()
                .iter()
                .find(|layer| layer.layer_id == "layer-world")
                .unwrap()
                .enabled
        );
        store
            .reorder_chat_layers(
                "chat-moonlit",
                &["layer-world".to_string(), "layer-story".to_string()],
            )
            .unwrap();
        store
            .toggle_chat_layer("chat-moonlit", "layer-world", true)
            .unwrap();
        assert_eq!(
            store
                .get_chat_layers("chat-moonlit")
                .unwrap()
                .iter()
                .map(|layer| layer.layer_id.as_str())
                .collect::<Vec<_>>(),
            vec!["layer-world", "layer-story"]
        );

        let recall = store
            .recall_lore(&LoreRecallQuery {
                chat_id: "chat-moonlit".to_string(),
                session_id: Some(SessionId::new("session-moonlit")),
                query_text: Some("moon gate tide".to_string()),
                active_subjects: vec!["entity-clockmaker".to_string()],
                excluded_subjects: Vec::new(),
                token_budget: Some(120),
                trace_id: Some("trace-moonlit-1".to_string()),
                record_trace: true,
                now: "2026-06-27T01:08:30Z".to_string(),
            })
            .unwrap();
        assert_eq!(recall.entries.len(), 1);
        assert_eq!(recall.entries[0].record.record_id, "lore-tide-calendar");
        assert!(recall.tokens_consumed > 0);
        assert_eq!(recall.trace.as_ref().unwrap().trace_id, "trace-moonlit-1");
        assert_eq!(
            store
                .count_rows("module_roleplay_lore_recall_traces")
                .unwrap(),
            1
        );
        let traces = store
            .list_recall_traces(&LoreRecallTraceQuery {
                session_id: Some(SessionId::new("session-moonlit")),
                chat_id: None,
                page: None,
            })
            .unwrap();
        assert_eq!(traces.len(), 1);
        assert_eq!(traces[0].trace_id, "trace-moonlit-1");
        let trace = store.get_recall_trace("trace-moonlit-1").unwrap().unwrap();
        assert_eq!(trace.entries_returned, 1);
        assert_eq!(trace.tokens_consumed, recall.tokens_consumed);

        store
            .archive_lore_layer(&RoleplayLoreLayerArchive {
                layer_id: "layer-story".to_string(),
                now: "2026-06-27T01:09:00Z".to_string(),
            })
            .unwrap();
        assert_eq!(
            store
                .list_lore_layers_by_profile("profile-narrator")
                .unwrap()
                .iter()
                .map(|layer| layer.layer_id.as_str())
                .collect::<Vec<_>>(),
            vec!["layer-world"]
        );

        remove_temp_db(&db_path);
    }

    #[test]
    fn sqlite_scale_fixture_reports_backend_move_pressure_without_resurrection() {
        let data_dir = temp_data_dir("scale-backend-pressure");
        let store = CoordinationStore::open(&data_dir).unwrap();
        let now = "2026-06-26T02:00:00Z".to_string();
        let mut sequence = 1_u64;

        for index in 0..36 {
            let session_id = SessionId::new(format!("scale-session-{index:02}"));
            let agent_id = AgentId::new(format!("scale-agent-{index:02}"));
            let profile_id = ProfileId::new(format!("scale-profile-{index:02}"));
            store
                .create_profile_registry_record(&profile_registry_write(&profile_id.0))
                .unwrap();
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
                        profile_id: profile_id.clone(),
                        kind: SessionKind::Full,
                        delegation: None,
                        resource_limits: sample_resource_limits(),
                        tool_profile: sample_tool_profile(),
                        history_window: None,
                        status: SessionStatus::Idle,
                        brain_turn_count: 0,
                        created_at: now.clone(),
                        last_active_at: now.clone(),
                    },
                    &config,
                )
                .unwrap();
            for memory_index in 0..2 {
                store
                    .add_profile_memory(
                        &ProfileMemoryWrite {
                            profile_id: profile_id.clone(),
                            target: ProfileMemoryTarget::User(format!("player-{memory_index}")),
                            key: format!("lore-seed-{memory_index}"),
                            content: format!(
                                "scale lore memory {index}-{memory_index}: persistent roleplay fact"
                            ),
                            metadata: json!({"fixture": "scale_backend_pressure"}),
                            now: now.clone(),
                        },
                        &ProfileMemoryCaps::default(),
                    )
                    .unwrap();
            }
        }

        let session_id = SessionId::new("scale-session-00");
        let branch_id = ConversationBranchId::new("scale-branch-root");
        store
            .save_conversation_branch(&ConversationBranchWrite {
                branch_id: branch_id.clone(),
                session_id: session_id.clone(),
                parent_branch_id: None,
                parent_message_id: None,
                origin_message_id: None,
                head_message_id: Some(MessageId::new("scale-message-069")),
                label: Some("Scale transcript root".to_string()),
                metadata_json: json!({"fixture": "scale_backend_pressure"}),
                created_at: now.clone(),
                updated_at: now.clone(),
            })
            .unwrap();
        for turn in 0..70 {
            let slot_id = MessageSlotId::new(format!("scale-slot-{turn:03}"));
            let variant_id = MessageVariantId::new(format!("scale-variant-{turn:03}"));
            let message_id = format!("scale-message-{turn:03}");
            store
                .save_message_slot(&MessageSlotWrite {
                    slot_id: slot_id.clone(),
                    session_id: session_id.clone(),
                    primary_variant_id: variant_id.clone(),
                    active_variant_id: None,
                    metadata_json: json!({"turn": turn}),
                    created_at: now.clone(),
                    updated_at: now.clone(),
                })
                .unwrap();
            let mut variant = variant_write(
                &slot_id,
                &variant_id,
                MessageVariantSource::Primary,
                0,
                &message_id,
                &format!("scale transcript turn {turn}: roleplay lore and search pressure needle"),
            );
            variant.message.session_id = session_id.clone();
            variant.message.branch_id = Some(branch_id.clone());
            store.save_message_variant(&variant).unwrap();
            store
                .save_event(
                    sequence,
                    &CoreEvent::AgentMessageRouted {
                        message: AgentMessage {
                            from: AgentId::new(format!("scale-agent-{:02}", turn % 36)),
                            to: AgentId::new(format!("scale-agent-{:02}", (turn + 1) % 36)),
                            body: format!("scale search row {turn}: roleplay lore needle"),
                            correlation_id: Some("scale-pressure".to_string()),
                            projection: None,
                        },
                    },
                )
                .unwrap();
            sequence += 1;
        }

        for index in 0..34 {
            store
                .upsert_scheduled_job(&ScheduledJobRecord {
                    job_id: format!("scale-job-{index:02}"),
                    job_kind: "maintenance".to_string(),
                    target_session_id: Some(SessionId::new(format!(
                        "scale-session-{:02}",
                        index % 36
                    ))),
                    interval_ms: Some(300_000),
                    next_due_at: Some("2026-06-26T02:05:00Z".to_string()),
                    payload_json: json!({"fixture": "scale_backend_pressure", "index": index}),
                    status: ScheduledJobStatus::Active,
                    created_at: now.clone(),
                    updated_at: now.clone(),
                    paused_at: None,
                })
                .unwrap();
            store
                .save_provider_wire_state(&sample_provider_wire_state_write(
                    ProviderWireStateWriteFixture {
                        key: ProviderWireStateKey {
                            session_id: SessionId::new(format!("scale-session-{:02}", index % 36)),
                            module_id: "openai-responses".to_string(),
                            strategy_id: format!("scale-wire-{index:02}"),
                        },
                        profile_fingerprint: "profile:scale:v1",
                        provider_fingerprint: "provider:gpt:v1",
                        payload_version: "responses:v1",
                        payload_json: json!({"response_id": format!("resp_scale_{index:02}")}),
                        now: "2026-06-26T02:01:00Z",
                        expires_at: Some("2026-06-27T02:01:00Z"),
                        last_wake_id: Some("wake-scale"),
                    },
                ))
                .unwrap();
        }

        for index in 0..40 {
            let expires_at = if index < 5 {
                "2026-06-26T02:00:01Z"
            } else {
                "2026-06-26T03:00:00Z"
            };
            store
                .save_queued_message(&QueuedMessageRecord {
                    message_id: format!("scale-queue-{index:02}"),
                    owner_session_id: Some(session_id.clone()),
                    owner_agent_id: AgentId::new("scale-agent-00"),
                    message: AgentMessage {
                        from: AgentId::new("operator"),
                        to: AgentId::new("scale-agent-00"),
                        body: format!("scale queued message {index}"),
                        correlation_id: Some("scale-queue".to_string()),
                        projection: None,
                    },
                    source_sequence: Some(sequence + index as u64),
                    enqueued_at: "2026-06-26T02:00:00Z".to_string(),
                    expires_at: expires_at.to_string(),
                    ttl_ms: if index < 5 { 1_000 } else { 3_600_000 },
                    delivery_attempts: 0,
                    state: QueuedMessageState::Pending,
                    terminal_at: None,
                    state_reason: None,
                })
                .unwrap();
        }

        let before_maintenance = store.storage_diagnostics().unwrap();
        assert!(before_maintenance.pressure);
        assert_active_storage_signal(&before_maintenance, "active_agent_count");
        assert_active_storage_signal(&before_maintenance, "conversation_transcript_growth");
        assert_active_storage_signal(&before_maintenance, "memory_lore_growth");
        assert_active_storage_signal(&before_maintenance, "runtime_search_growth");
        assert_active_storage_signal(&before_maintenance, "queued_message_retention");
        assert_active_storage_signal(&before_maintenance, "scheduler_row_growth");
        assert_active_storage_signal(&before_maintenance, "provider_wire_state_growth");
        assert_inactive_storage_signal(&before_maintenance, "single_service_writer_assumption");

        let report = store
            .run_maintenance(&RuntimeMaintenancePolicy {
                expire_queued_messages_at: Some("2026-06-26T02:00:02Z".to_string()),
                purge_terminal_queued_messages_before: Some("2026-06-26T02:00:03Z".to_string()),
                run_wal_checkpoint: true,
                run_optimize: true,
                ..RuntimeMaintenancePolicy::default()
            })
            .unwrap();
        assert_eq!(report.expired_queue_messages, 5);
        assert_eq!(report.purged_terminal_queue_messages, 5);
        assert_eq!(store.count_rows("queued_messages").unwrap(), 35);

        let pending = store
            .load_queued_messages(&QueuedMessageFilter {
                state: Some(QueuedMessageState::Pending),
                owner_session_id: Some(session_id.clone()),
                owner_agent_id: Some(AgentId::new("scale-agent-00")),
                limit: None,
            })
            .unwrap();
        assert_eq!(pending.len(), 35);
        assert!(pending.iter().all(|message| !matches!(
            message.message_id.as_str(),
            "scale-queue-00"
                | "scale-queue-01"
                | "scale-queue-02"
                | "scale-queue-03"
                | "scale-queue-04"
        )));
        assert_eq!(
            store
                .search_runtime(&RuntimeSearchFilter {
                    query: "scale queued message 0".to_string(),
                    row_type: Some(RuntimeSearchRowType::QueueMessage),
                    session_id: Some(session_id),
                    agent_id: Some(AgentId::new("scale-agent-00")),
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

        remove_temp_dir(&data_dir);
    }

    #[test]
    fn roleplay_lore_fts_triggers_track_record_changes() {
        let db_path = temp_db_path("roleplay-lore-fts");
        let _store = CoordinationStore::open_file(&db_path).unwrap();
        let conn = Connection::open(&db_path).unwrap();
        conn.execute(
            "INSERT INTO module_roleplay_lore_records (
                record_id,
                world_id,
                entity_id,
                session_id,
                branch_id,
                shape_id,
                shape_version,
                canon_status,
                visibility,
                status,
                revision,
                title,
                body,
                content_json,
                evidence_refs_json,
                source,
                confidence,
                durability_rationale,
                supersedes_record_id,
                superseded_by_record_id,
                tombstoned_at,
                tombstone_reason,
                created_at,
                updated_at
            ) VALUES (
                'lore-observatory',
                'world-moonlit',
                'entity-clockmaker',
                NULL,
                NULL,
                'lore_entry',
                1,
                'canon',
                'public',
                'active',
                1,
                'Observatory Door',
                'The observatory door opens at eclipse tide.',
                '{\"tags\":[\"observatory\",\"eclipse\"]}',
                '[]',
                'test',
                0.9,
                'schema test',
                NULL,
                NULL,
                NULL,
                NULL,
                '2026-06-27T00:00:00Z',
                '2026-06-27T00:00:00Z'
            )",
            [],
        )
        .unwrap();
        assert_eq!(roleplay_lore_fts_matches(&conn, "observatory"), 1);

        conn.execute(
            "UPDATE module_roleplay_lore_records
             SET title = 'Moon Gate',
                 body = 'The moon gate opens only when the brass needle turns.',
                 content_json = '{\"tags\":[\"moon\",\"brass\"]}',
                 updated_at = '2026-06-27T00:01:00Z'
             WHERE record_id = 'lore-observatory'",
            [],
        )
        .unwrap();
        assert_eq!(roleplay_lore_fts_matches(&conn, "observatory"), 0);
        assert_eq!(roleplay_lore_fts_matches(&conn, "moon"), 1);

        conn.execute(
            "DELETE FROM module_roleplay_lore_records WHERE record_id = 'lore-observatory'",
            [],
        )
        .unwrap();
        assert_eq!(roleplay_lore_fts_matches(&conn, "moon"), 0);

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
    fn logical_storage_import_dry_run_validates_capabilities_and_idempotency_without_writes() {
        let db_path = temp_db_path("logical-import-dry-run");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        let bundle = logical_import_bundle(vec![LogicalStorageRepositoryBundle {
            repository_id: "runtime_counters".to_string(),
            schema_version: 1,
            required_capabilities: vec!["transactions".to_string()],
            exported_count: 1,
            checksum: Some("sha256:runtime-counters".to_string()),
            records: vec![LogicalStorageRecord {
                stable_id: "runtime-counter:brain_turns".to_string(),
                record_version: 1,
                exported_at: "2026-06-26T10:00:00Z".to_string(),
                payload: LogicalStorageRecordPayload::TypedJson {
                    object_kind: "runtime_counter".to_string(),
                    payload_json: json!({
                        "scope_type": "runtime",
                        "counter_name": "brain_turns",
                        "value": 7
                    }),
                },
            }],
        }]);
        let dry_run = LogicalStorageImportDryRun {
            import_batch_id: "dry-run-batch-1".to_string(),
            target_backend: "sqlite".to_string(),
            validation_time: "2026-06-26T10:01:00Z".to_string(),
            supported_capabilities: vec!["transactions".to_string()],
            supported_repositories: vec!["runtime_counters".to_string()],
        };

        let report = store
            .validate_logical_storage_import(&bundle, &dry_run)
            .unwrap();
        assert_eq!(report.record_count, 1);
        assert_eq!(report.accepted_records, 1);
        assert_eq!(report.unsupported_records, 0);
        assert_eq!(report.refused_records, 0);
        assert!(report.can_apply());
        assert_eq!(store.count_rows("runtime_import_batches").unwrap(), 0);

        store
            .save_import_batch(&RuntimeImportBatchRecord {
                import_batch_id: "dry-run-batch-1".to_string(),
                source_system: "logical-export".to_string(),
                source_label: "already imported".to_string(),
                source_snapshot_ref: Some("logical://bundle/export-1".to_string()),
                notes: None,
                imported_at: "2026-06-26T10:02:00Z".to_string(),
            })
            .unwrap();
        let idempotent = store
            .validate_logical_storage_import(&bundle, &dry_run)
            .unwrap();
        assert!(idempotent.already_imported);
        assert!(!idempotent.can_apply());
        assert!(idempotent
            .issues
            .iter()
            .any(|issue| issue.code == "import_batch_already_recorded"));

        remove_temp_db(&db_path);
    }

    #[test]
    fn logical_storage_import_dry_run_refuses_queue_resurrection_risks() {
        let db_path = temp_db_path("logical-import-queue-safety");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        let bundle = logical_import_bundle(vec![LogicalStorageRepositoryBundle {
            repository_id: "queues_messages".to_string(),
            schema_version: 1,
            required_capabilities: vec!["transactions".to_string()],
            exported_count: 2,
            checksum: None,
            records: vec![
                LogicalStorageRecord {
                    stable_id: "queue:fresh".to_string(),
                    record_version: 1,
                    exported_at: "2026-06-26T10:00:00Z".to_string(),
                    payload: LogicalStorageRecordPayload::QueueMessage(Box::new(
                        logical_queue_message(
                            "queue-fresh",
                            QueuedMessageState::Pending,
                            "2026-06-26T10:05:00Z",
                            None,
                        ),
                    )),
                },
                LogicalStorageRecord {
                    stable_id: "queue:stale".to_string(),
                    record_version: 1,
                    exported_at: "2026-06-26T10:00:00Z".to_string(),
                    payload: LogicalStorageRecordPayload::QueueMessage(Box::new(
                        logical_queue_message(
                            "queue-stale",
                            QueuedMessageState::Pending,
                            "2026-06-26T09:59:00Z",
                            None,
                        ),
                    )),
                },
            ],
        }]);
        let report = store
            .validate_logical_storage_import(
                &bundle,
                &LogicalStorageImportDryRun {
                    import_batch_id: "queue-dry-run".to_string(),
                    target_backend: "postgres".to_string(),
                    validation_time: "2026-06-26T10:01:00Z".to_string(),
                    supported_capabilities: vec!["transactions".to_string()],
                    supported_repositories: vec!["queues_messages".to_string()],
                },
            )
            .unwrap();

        assert_eq!(report.accepted_records, 1);
        assert_eq!(report.refused_records, 1);
        assert!(!report.can_apply());
        assert!(report.issues.iter().any(|issue| {
            issue.code == "queue_pending_expired_would_resurrect"
                && issue.record_id.as_deref() == Some("queue:stale")
        }));
        assert_eq!(store.count_rows("queued_messages").unwrap(), 0);

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
        assert_eq!(
            loaded.prompt_soul_markdown.as_deref(),
            Some("You are a registry-backed runner.")
        );
        assert_eq!(
            loaded.prompt_memory_markdown.as_deref(),
            Some("Static deployment-safe memory.")
        );

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
    fn profile_purge_removes_registry_sessions_and_profile_owned_readbacks() {
        let db_path = temp_db_path("profile-purge");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        let config = sample_session_config();
        let state = sample_session_state();

        store
            .create_profile_registry_record(&profile_registry_write("full-profile"))
            .unwrap();
        store
            .create_profile_registry_record(&profile_registry_write("other-profile"))
            .unwrap();
        store.save_session_with_config(&state, &config).unwrap();
        store
            .add_profile_memory(
                &ProfileMemoryWrite {
                    profile_id: ProfileId::new("full-profile"),
                    target: ProfileMemoryTarget::Profile,
                    key: "style".to_string(),
                    content: "delete me".to_string(),
                    metadata: serde_json::json!({"source": "profile_purge_test"}),
                    now: "2026-06-20T05:00:00Z".to_string(),
                },
                &ProfileMemoryCaps::default(),
            )
            .unwrap();
        store
            .save_event(
                1,
                &CoreEvent::SessionCreated {
                    state: Box::new(state.clone()),
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
                        body: "profile purge message".to_string(),
                        correlation_id: Some("corr-profile-purge".to_string()),
                        projection: None,
                    },
                },
            )
            .unwrap();
        let slot_id = MessageSlotId::new("slot-profile-purge");
        let variant_id = MessageVariantId::new("variant-profile-purge");
        store
            .save_message_slot(&MessageSlotWrite {
                slot_id: slot_id.clone(),
                session_id: state.session_id.clone(),
                primary_variant_id: variant_id.clone(),
                active_variant_id: None,
                metadata_json: json!({"test": "profile_purge"}),
                created_at: "2026-06-25T03:00:00Z".to_string(),
                updated_at: "2026-06-25T03:00:00Z".to_string(),
            })
            .unwrap();
        let mut variant = variant_write(
            &slot_id,
            &variant_id,
            MessageVariantSource::Primary,
            0,
            "message-profile-purge",
            "visible transcript residue",
        );
        variant.message.session_id = state.session_id.clone();
        store.save_message_variant(&variant).unwrap();

        assert_eq!(store.count_rows("sessions").unwrap(), 1);
        assert_eq!(store.count_rows("profile_registry").unwrap(), 2);
        assert_eq!(store.count_rows("message_slots").unwrap(), 1);
        assert_eq!(store.count_rows("profile_memories").unwrap(), 1);

        let report = store
            .purge_profile(&ProfileId::new("full-profile"))
            .unwrap();
        assert!(report.profile_registry_deleted);
        assert_eq!(report.profile_id, ProfileId::new("full-profile"));
        assert_eq!(report.session_ids, vec![SessionId::new("session-alpha")]);
        assert!(report.agent_ids.contains(&AgentId::new("agent-alpha")));
        assert!(report.rows_deleted > 0);

        assert!(store
            .get_profile_registry_record(&ProfileId::new("full-profile"))
            .unwrap()
            .is_none());
        assert!(store
            .get_profile_registry_record(&ProfileId::new("other-profile"))
            .unwrap()
            .is_some());
        assert_eq!(store.count_rows("sessions").unwrap(), 0);
        assert_eq!(store.count_rows("session_configs").unwrap(), 0);
        assert_eq!(store.count_rows("event_history").unwrap(), 0);
        assert_eq!(store.count_rows("event_session_index").unwrap(), 0);
        assert_eq!(store.count_rows("event_agent_index").unwrap(), 0);
        assert_eq!(store.count_rows("agent_messages").unwrap(), 0);
        assert_eq!(store.count_rows("message_slots").unwrap(), 0);
        assert_eq!(store.count_rows("message_variants").unwrap(), 0);
        assert_eq!(store.count_rows("messages").unwrap(), 0);
        assert_eq!(store.count_rows("message_blocks").unwrap(), 0);
        assert_eq!(store.count_rows("profile_memories").unwrap(), 0);
        assert_eq!(store.count_rows("profile_registry").unwrap(), 1);

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
    fn session_memory_round_trips_and_isolates_by_session() {
        let db_path = temp_db_path("session-memory-basic");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        store.save_session(&sample_session_state()).unwrap();
        let mut other_session = sample_session_state();
        other_session.session_id = SessionId::new("session-beta");
        other_session.agent_id = AgentId::new("agent-beta");
        other_session.handle = SessionHandle::new(2);
        store.save_session(&other_session).unwrap();

        let added = store
            .add_session_memory_record(&session_fact_memory_write(
                "session-fact-one",
                &SessionId::new("session-alpha"),
                "2026-06-26T01:00:00Z",
            ))
            .unwrap();

        assert_eq!(added.revision, 1);
        assert_eq!(added.status, SessionMemoryRecordStatus::Active);
        assert_eq!(added.scope.scope_type, MemoryScopeType::Session);
        assert_eq!(added.shape.shape_id.as_str(), "session_fact");
        assert_eq!(
            added.content["content"],
            "The user prefers slow-burn pacing."
        );

        let alpha_rows = store
            .query_session_memory_records(&SessionMemoryQuery {
                session_id: Some(SessionId::new("session-alpha")),
                shape_id: Some("session_fact".to_string()),
                ..SessionMemoryQuery::default()
            })
            .unwrap();
        assert_eq!(alpha_rows, vec![added.clone()]);

        let beta_rows = store
            .query_session_memory_records(&SessionMemoryQuery {
                session_id: Some(SessionId::new("session-beta")),
                ..SessionMemoryQuery::default()
            })
            .unwrap();
        assert!(beta_rows.is_empty());

        let invalid_shape = store
            .add_session_memory_record(&SessionMemoryRecordWrite {
                shape: MemoryRecordShapeRef {
                    shape_id: MemoryRecordShapeId::unchecked("transcript_message"),
                    version: 1,
                },
                ..session_fact_memory_write(
                    "session-fact-two",
                    &SessionId::new("session-alpha"),
                    "2026-06-26T01:01:00Z",
                )
            })
            .unwrap_err();
        assert_eq!(invalid_shape.kind, CoreErrorKind::InvalidInput);

        remove_temp_db(&db_path);
    }

    #[test]
    fn session_memory_validates_branch_membership() {
        let db_path = temp_db_path("session-memory-branch");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        store.save_session(&sample_session_state()).unwrap();
        let mut other_session = sample_session_state();
        other_session.session_id = SessionId::new("session-beta");
        other_session.agent_id = AgentId::new("agent-beta");
        other_session.handle = SessionHandle::new(2);
        store.save_session(&other_session).unwrap();
        store
            .save_conversation_branch(&ConversationBranchWrite {
                branch_id: ConversationBranchId::new("branch-alpha"),
                session_id: SessionId::new("session-alpha"),
                parent_branch_id: None,
                parent_message_id: None,
                origin_message_id: Some(MessageId::new("message-root")),
                head_message_id: Some(MessageId::new("message-alpha")),
                label: Some("Branch alpha".to_string()),
                metadata_json: json!({"fixture": true}),
                created_at: "2026-06-26T01:00:00Z".to_string(),
                updated_at: "2026-06-26T01:00:00Z".to_string(),
            })
            .unwrap();

        let missing_branch_id = store
            .add_session_memory_record(&SessionMemoryRecordWrite {
                branch_id: None,
                ..branch_summary_memory_write(
                    "branch-summary-missing",
                    &SessionId::new("session-alpha"),
                    &ConversationBranchId::new("branch-alpha"),
                    "2026-06-26T01:01:00Z",
                )
            })
            .unwrap_err();
        assert_eq!(missing_branch_id.kind, CoreErrorKind::InvalidInput);

        let wrong_session = store
            .add_session_memory_record(&branch_summary_memory_write(
                "branch-summary-wrong-session",
                &SessionId::new("session-beta"),
                &ConversationBranchId::new("branch-alpha"),
                "2026-06-26T01:02:00Z",
            ))
            .unwrap_err();
        assert_eq!(wrong_session.kind, CoreErrorKind::InvalidInput);

        let added = store
            .add_session_memory_record(&branch_summary_memory_write(
                "branch-summary-one",
                &SessionId::new("session-alpha"),
                &ConversationBranchId::new("branch-alpha"),
                "2026-06-26T01:03:00Z",
            ))
            .unwrap();
        assert_eq!(
            added.branch_id,
            Some(ConversationBranchId::new("branch-alpha"))
        );

        let branch_rows = store
            .query_session_memory_records(&SessionMemoryQuery {
                branch_id: Some(ConversationBranchId::new("branch-alpha")),
                ..SessionMemoryQuery::default()
            })
            .unwrap();
        assert_eq!(branch_rows, vec![added]);

        remove_temp_db(&db_path);
    }

    #[test]
    fn session_memory_replace_supersede_and_archive_enforce_revisions() {
        let db_path = temp_db_path("session-memory-revisions");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        store.save_session(&sample_session_state()).unwrap();

        let added = store
            .add_session_memory_record(&session_fact_memory_write(
                "session-fact-one",
                &SessionId::new("session-alpha"),
                "2026-06-26T01:00:00Z",
            ))
            .unwrap();
        let replaced = store
            .replace_session_memory_record(&SessionMemoryReplace {
                record_id: added.record_id.clone(),
                expected_revision: added.revision,
                content: session_fact_content(
                    "session-fact-one",
                    "The user prefers slow-burn pacing with explicit clues.",
                    "2026-06-26T01:01:00Z",
                ),
                evidence_refs: session_memory_evidence("wake-replace"),
                source: MemoryProposalSource::Human,
                confidence: 0.95,
                durability_rationale: "Human correction refined the fact.".to_string(),
                now: "2026-06-26T01:01:00Z".to_string(),
            })
            .unwrap();
        assert_eq!(replaced.revision, 2);
        assert_eq!(
            replaced.content["content"],
            "The user prefers slow-burn pacing with explicit clues."
        );

        let stale_replace = store
            .replace_session_memory_record(&SessionMemoryReplace {
                expected_revision: 1,
                now: "2026-06-26T01:02:00Z".to_string(),
                ..replace_session_fact_input("session-fact-one")
            })
            .unwrap_err();
        assert_eq!(stale_replace.kind, CoreErrorKind::ActionRejected);

        let (old_record, new_record) = store
            .supersede_session_memory_record(&SessionMemorySupersede {
                record_id: "session-fact-one".to_string(),
                expected_revision: replaced.revision,
                replacement: SessionMemoryRecordWrite {
                    supersedes_record_id: Some("session-fact-one".to_string()),
                    content: session_fact_content(
                        "session-fact-two",
                        "The user prefers mystery pacing with explicit clue checkpoints.",
                        "2026-06-26T01:03:00Z",
                    ),
                    ..session_fact_memory_write(
                        "session-fact-two",
                        &SessionId::new("session-alpha"),
                        "2026-06-26T01:03:00Z",
                    )
                },
            })
            .unwrap();
        assert_eq!(old_record.status, SessionMemoryRecordStatus::Superseded);
        assert_eq!(
            old_record.superseded_by_record_id.as_deref(),
            Some("session-fact-two")
        );
        assert_eq!(old_record.revision, 3);
        assert_eq!(new_record.status, SessionMemoryRecordStatus::Active);
        assert_eq!(
            new_record.supersedes_record_id.as_deref(),
            Some("session-fact-one")
        );

        let archived = store
            .archive_session_memory_record(&SessionMemoryArchive {
                record_id: "session-fact-two".to_string(),
                expected_revision: new_record.revision,
                reason: Some("Compacted into a later summary".to_string()),
                now: "2026-06-26T01:04:00Z".to_string(),
            })
            .unwrap();
        assert_eq!(archived.status, SessionMemoryRecordStatus::Archived);
        assert_eq!(archived.revision, 2);

        let stale_archive = store
            .archive_session_memory_record(&SessionMemoryArchive {
                record_id: "session-fact-two".to_string(),
                expected_revision: 1,
                reason: None,
                now: "2026-06-26T01:05:00Z".to_string(),
            })
            .unwrap_err();
        assert_eq!(stale_archive.kind, CoreErrorKind::ActionRejected);

        let active_rows = store
            .query_session_memory_records(&SessionMemoryQuery {
                session_id: Some(SessionId::new("session-alpha")),
                ..SessionMemoryQuery::default()
            })
            .unwrap();
        assert!(active_rows.is_empty());

        let history_rows = store
            .query_session_memory_records(&SessionMemoryQuery {
                session_id: Some(SessionId::new("session-alpha")),
                include_superseded: true,
                include_archived: true,
                ..SessionMemoryQuery::default()
            })
            .unwrap();
        assert_eq!(history_rows.len(), 2);

        remove_temp_db(&db_path);
    }

    #[test]
    fn session_memory_compaction_archives_records_without_touching_message_history() {
        let db_path = temp_db_path("session-memory-compaction");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        store.save_session(&sample_session_state()).unwrap();
        let session_id = SessionId::new("session-alpha");
        let slot_id = MessageSlotId::new("slot-compaction");
        let variant_id = MessageVariantId::new("variant-compaction");
        store
            .save_message_slot(&MessageSlotWrite {
                slot_id: slot_id.clone(),
                session_id: session_id.clone(),
                primary_variant_id: variant_id.clone(),
                active_variant_id: None,
                metadata_json: json!({"fixture": "compaction"}),
                created_at: "2026-06-26T01:00:00Z".to_string(),
                updated_at: "2026-06-26T01:00:00Z".to_string(),
            })
            .unwrap();
        store
            .save_message_variant(&variant_write(
                &slot_id,
                &variant_id,
                MessageVariantSource::Primary,
                0,
                "message-compaction",
                "raw message history must survive compaction",
            ))
            .unwrap();

        for index in 0..4 {
            store
                .add_session_memory_record(&session_fact_memory_write(
                    &format!("session-fact-{index}"),
                    &session_id,
                    &format!("2026-06-26T01:0{index}:00Z"),
                ))
                .unwrap();
        }
        let slots_before = store.count_rows("message_slots").unwrap();
        let variants_before = store.count_rows("message_variants").unwrap();

        let report = store
            .run_maintenance(&RuntimeMaintenancePolicy {
                compact_session_memory_at: Some("2026-06-26T02:00:00Z".to_string()),
                session_memory_max_active_records_per_scope: Some(2),
                session_memory_archive_batch_size: Some(2),
                ..RuntimeMaintenancePolicy::default()
            })
            .unwrap();

        assert!(report.session_memory_compaction.enabled);
        assert_eq!(report.session_memory_compaction.scopes_inspected, 1);
        assert_eq!(
            report.session_memory_compaction.retention_pressure_scopes,
            1
        );
        assert_eq!(report.session_memory_compaction.scopes_compacted, 1);
        assert_eq!(
            report.session_memory_compaction.session_summaries_created,
            1
        );
        assert_eq!(report.session_memory_compaction.records_archived, 2);
        assert_eq!(report.session_memory_compaction.records_superseded, 0);
        assert_eq!(store.count_rows("message_slots").unwrap(), slots_before);
        assert_eq!(
            store.count_rows("message_variants").unwrap(),
            variants_before
        );

        let rows = store
            .query_session_memory_records(&SessionMemoryQuery {
                session_id: Some(session_id),
                include_archived: true,
                ..SessionMemoryQuery::default()
            })
            .unwrap();
        let summary = rows
            .iter()
            .find(|record| record.shape.shape_id.as_str() == "session_summary")
            .expect("summary record");
        assert_eq!(summary.status, SessionMemoryRecordStatus::Active);
        assert_eq!(
            summary.content["metadata_json"]["generated_by"],
            "runtime_maintenance"
        );
        let archived: Vec<_> = rows
            .iter()
            .filter(|record| record.status == SessionMemoryRecordStatus::Archived)
            .collect();
        assert_eq!(archived.len(), 2);
        assert!(archived.iter().all(|record| record
            .archive_reason
            .as_deref()
            .unwrap_or_default()
            .contains(summary.record_id.as_str())));

        remove_temp_db(&db_path);
    }

    #[test]
    fn session_memory_compaction_writes_branch_summary_for_branch_scopes() {
        let db_path = temp_db_path("session-memory-branch-compaction");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        store.save_session(&sample_session_state()).unwrap();
        save_branch_tree(&store);
        let session_id = SessionId::new("session-alpha");
        let branch_id = ConversationBranchId::new("branch-active");

        for index in 0..3 {
            store
                .add_session_memory_record(&branch_user_choice_memory_write(
                    &format!("branch-choice-{index}"),
                    &session_id,
                    &branch_id,
                    &format!("2026-06-26T01:1{index}:00Z"),
                ))
                .unwrap();
        }

        let report = store
            .run_maintenance(&RuntimeMaintenancePolicy {
                compact_session_memory_at: Some("2026-06-26T02:10:00Z".to_string()),
                session_memory_max_active_records_per_scope: Some(1),
                session_memory_archive_batch_size: Some(2),
                ..RuntimeMaintenancePolicy::default()
            })
            .unwrap();

        assert_eq!(report.session_memory_compaction.scopes_compacted, 1);
        assert_eq!(report.session_memory_compaction.branch_summaries_created, 1);
        assert_eq!(report.session_memory_compaction.records_archived, 2);
        let rows = store
            .query_session_memory_records(&SessionMemoryQuery {
                session_id: Some(session_id),
                branch_id: Some(branch_id.clone()),
                include_archived: true,
                ..SessionMemoryQuery::default()
            })
            .unwrap();
        let summary = rows
            .iter()
            .find(|record| record.shape.shape_id.as_str() == "branch_summary")
            .expect("branch summary");
        assert_eq!(
            summary.scope.scope_type,
            MemoryScopeType::ConversationBranch
        );
        assert_eq!(summary.branch_id, Some(branch_id.clone()));
        assert_eq!(summary.content["branch_id"], branch_id.0);
        assert_eq!(summary.content["head_message_id"], "branch-active:head");

        remove_temp_db(&db_path);
    }

    #[test]
    fn branch_aware_session_memory_orders_active_ancestor_then_session() {
        let db_path = temp_db_path("session-memory-branch-aware-order");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        store.save_session(&sample_session_state()).unwrap();
        save_branch_tree(&store);

        store
            .add_session_memory_record(&branch_summary_memory_write(
                "memory-root-branch",
                &SessionId::new("session-alpha"),
                &ConversationBranchId::new("branch-root"),
                "2026-06-26T01:01:00Z",
            ))
            .unwrap();
        store
            .add_session_memory_record(&branch_summary_memory_write(
                "memory-active-branch",
                &SessionId::new("session-alpha"),
                &ConversationBranchId::new("branch-active"),
                "2026-06-26T01:02:00Z",
            ))
            .unwrap();
        store
            .add_session_memory_record(&session_fact_memory_write(
                "memory-session",
                &SessionId::new("session-alpha"),
                "2026-06-26T01:03:00Z",
            ))
            .unwrap();

        let context = store
            .build_session_memory_prompt_context(&BranchAwareSessionMemoryQuery {
                session_id: SessionId::new("session-alpha"),
                active_branch_id: Some(ConversationBranchId::new("branch-active")),
                include_ancestors: true,
                include_siblings: false,
                shape_id: None,
                prompt_context_only: true,
                page: None,
            })
            .unwrap();

        assert_eq!(
            context
                .records
                .iter()
                .map(|record| record.record_id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "memory-active-branch",
                "memory-root-branch",
                "memory-session"
            ]
        );
        assert_eq!(
            context.diagnostics.selected_records[0].record_id,
            "memory-active-branch"
        );
        assert_eq!(context.diagnostics.excluded_counts.sibling_branch, 0);
        assert!(context.diagnostics.character_estimate > 0);
        assert!(context.diagnostics.token_estimate > 0);

        remove_temp_db(&db_path);
    }

    #[test]
    fn branch_aware_session_memory_excludes_siblings_by_default() {
        let db_path = temp_db_path("session-memory-branch-aware-siblings");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        store.save_session(&sample_session_state()).unwrap();
        save_branch_tree(&store);

        for (record_id, branch_id, now) in [
            ("memory-root-branch", "branch-root", "2026-06-26T01:01:00Z"),
            (
                "memory-active-branch",
                "branch-active",
                "2026-06-26T01:02:00Z",
            ),
            (
                "memory-sibling-branch",
                "branch-sibling",
                "2026-06-26T01:03:00Z",
            ),
        ] {
            store
                .add_session_memory_record(&branch_summary_memory_write(
                    record_id,
                    &SessionId::new("session-alpha"),
                    &ConversationBranchId::new(branch_id),
                    now,
                ))
                .unwrap();
        }

        let default_context = store
            .build_session_memory_prompt_context(&BranchAwareSessionMemoryQuery {
                session_id: SessionId::new("session-alpha"),
                active_branch_id: Some(ConversationBranchId::new("branch-active")),
                include_ancestors: true,
                include_siblings: false,
                shape_id: None,
                prompt_context_only: true,
                page: None,
            })
            .unwrap();
        assert_eq!(
            default_context
                .records
                .iter()
                .map(|record| record.record_id.as_str())
                .collect::<Vec<_>>(),
            vec!["memory-active-branch", "memory-root-branch"]
        );
        assert_eq!(
            default_context.diagnostics.excluded_counts.sibling_branch,
            1
        );

        let sibling_context = store
            .build_session_memory_prompt_context(&BranchAwareSessionMemoryQuery {
                include_siblings: true,
                ..BranchAwareSessionMemoryQuery {
                    session_id: SessionId::new("session-alpha"),
                    active_branch_id: Some(ConversationBranchId::new("branch-active")),
                    include_ancestors: true,
                    include_siblings: false,
                    shape_id: None,
                    prompt_context_only: true,
                    page: None,
                }
            })
            .unwrap();
        assert_eq!(
            sibling_context
                .records
                .iter()
                .map(|record| record.record_id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "memory-active-branch",
                "memory-root-branch",
                "memory-sibling-branch"
            ]
        );
        assert_eq!(
            sibling_context.diagnostics.excluded_counts.sibling_branch,
            0
        );

        remove_temp_db(&db_path);
    }

    #[test]
    fn prompt_context_reports_policy_status_and_limit_exclusions() {
        let db_path = temp_db_path("session-memory-prompt-diagnostics");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        store.save_session(&sample_session_state()).unwrap();

        store
            .add_session_memory_record(&session_fact_memory_write(
                "memory-selected",
                &SessionId::new("session-alpha"),
                "2026-06-26T01:00:00Z",
            ))
            .unwrap();
        let archived = store
            .add_session_memory_record(&session_fact_memory_write(
                "memory-archived",
                &SessionId::new("session-alpha"),
                "2026-06-26T01:01:00Z",
            ))
            .unwrap();
        store
            .archive_session_memory_record(&SessionMemoryArchive {
                record_id: archived.record_id,
                expected_revision: archived.revision,
                reason: Some("No longer useful".to_string()),
                now: "2026-06-26T01:02:00Z".to_string(),
            })
            .unwrap();
        let superseded = store
            .add_session_memory_record(&session_fact_memory_write(
                "memory-superseded",
                &SessionId::new("session-alpha"),
                "2026-06-26T01:03:00Z",
            ))
            .unwrap();
        store
            .supersede_session_memory_record(&SessionMemorySupersede {
                record_id: superseded.record_id,
                expected_revision: superseded.revision,
                replacement: SessionMemoryRecordWrite {
                    supersedes_record_id: Some("memory-superseded".to_string()),
                    content: session_fact_content(
                        "memory-replacement",
                        "Replacement fact remains selectable.",
                        "2026-06-26T01:04:00Z",
                    ),
                    ..session_fact_memory_write(
                        "memory-replacement",
                        &SessionId::new("session-alpha"),
                        "2026-06-26T01:04:00Z",
                    )
                },
            })
            .unwrap();
        store
            .add_session_memory_record(&SessionMemoryRecordWrite {
                content: {
                    let mut content = session_fact_content(
                        "memory-tool-only",
                        "Tool-only diagnostic detail.",
                        "2026-06-26T01:05:00Z",
                    );
                    content["metadata_json"] = json!({"prompt_policy": "tool_only"});
                    content
                },
                ..session_fact_memory_write(
                    "memory-tool-only",
                    &SessionId::new("session-alpha"),
                    "2026-06-26T01:05:00Z",
                )
            })
            .unwrap();
        store
            .add_session_memory_record(&SessionMemoryRecordWrite {
                content: {
                    let mut content = session_fact_content(
                        "memory-policy-disabled",
                        "Never prompt detail.",
                        "2026-06-26T01:06:00Z",
                    );
                    content["metadata_json"] = json!({"prompt_policy": "never_prompt"});
                    content
                },
                ..session_fact_memory_write(
                    "memory-policy-disabled",
                    &SessionId::new("session-alpha"),
                    "2026-06-26T01:06:00Z",
                )
            })
            .unwrap();

        let context = store
            .build_session_memory_prompt_context(&BranchAwareSessionMemoryQuery {
                session_id: SessionId::new("session-alpha"),
                active_branch_id: None,
                include_ancestors: false,
                include_siblings: false,
                shape_id: None,
                prompt_context_only: true,
                page: Some(QueryPage {
                    limit: Some(1),
                    offset: None,
                }),
            })
            .unwrap();

        assert_eq!(context.records.len(), 1);
        assert_eq!(
            context.diagnostics.context_policy,
            SessionMemoryPromptContextPolicy::SummaryContext
        );
        assert_eq!(context.diagnostics.excluded_counts.archived, 1);
        assert_eq!(context.diagnostics.excluded_counts.superseded, 1);
        assert_eq!(context.diagnostics.excluded_counts.tool_only, 1);
        assert_eq!(context.diagnostics.excluded_counts.policy_disabled, 1);
        assert_eq!(context.diagnostics.excluded_counts.limit_exceeded, 1);
        assert_eq!(context.diagnostics.selected_records.len(), 1);

        let history = store
            .build_session_memory_prompt_context(&BranchAwareSessionMemoryQuery {
                prompt_context_only: false,
                page: None,
                ..BranchAwareSessionMemoryQuery {
                    session_id: SessionId::new("session-alpha"),
                    active_branch_id: None,
                    include_ancestors: false,
                    include_siblings: false,
                    shape_id: None,
                    prompt_context_only: true,
                    page: None,
                }
            })
            .unwrap();
        assert_eq!(
            history.diagnostics.context_policy,
            SessionMemoryPromptContextPolicy::ToolOnly
        );
        assert!(history.records.len() > context.records.len());

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
    fn session_activity_digests_save_and_list_by_profile_session_and_wake() {
        let db_path = temp_db_path("session-activity-digests");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        let digest = SessionActivityDigest {
            digest_id: "sad_alpha".to_string(),
            profile_id: ProfileId::new("prime-profile"),
            session_id: SessionId::new("session-alpha"),
            wake_id: "wake-alpha".to_string(),
            source: "direct_debug".to_string(),
            summary_text: "Wake wake-alpha from direct_debug.".to_string(),
            event_counts_json: json!({"brain_event_observed.text_delta": 1}),
            tool_calls_json: json!([{"tool_name": "shell", "status": "failed"}]),
            signals_json: json!([{"signal_type": "tool_failure"}]),
            completion_summary: Some("wake completed".to_string()),
            allowed_capture_spaces: vec![MemorySpaceId::unchecked("profile_dense")],
            created_at: "2026-06-27T12:00:00Z".to_string(),
            retention_until: Some("2026-07-04T12:00:00Z".to_string()),
            reviewed_at: None,
        };

        let saved = store.save_session_activity_digest(&digest).unwrap();
        assert_eq!(saved.digest_id, "sad_alpha");
        assert_eq!(store.count_rows("session_activity_digests").unwrap(), 1);

        let duplicate = SessionActivityDigest {
            summary_text: "Updated deterministic digest.".to_string(),
            ..digest.clone()
        };
        let saved_duplicate = store.save_session_activity_digest(&duplicate).unwrap();
        assert_eq!(
            saved_duplicate.summary_text,
            "Updated deterministic digest."
        );
        assert_eq!(store.count_rows("session_activity_digests").unwrap(), 1);

        let by_profile = store
            .list_session_activity_digests(&SessionActivityDigestQuery {
                profile_id: Some(ProfileId::new("prime-profile")),
                session_id: None,
                wake_id: None,
                include_reviewed: false,
                limit: None,
                offset: None,
            })
            .unwrap();
        assert_eq!(by_profile.len(), 1);
        assert_eq!(by_profile[0].wake_id, "wake-alpha");

        let by_session_wake = store
            .list_session_activity_digests(&SessionActivityDigestQuery {
                profile_id: None,
                session_id: Some(SessionId::new("session-alpha")),
                wake_id: Some("wake-alpha".to_string()),
                include_reviewed: false,
                limit: Some(10),
                offset: Some(0),
            })
            .unwrap();
        assert_eq!(by_session_wake.len(), 1);
        assert_eq!(
            by_session_wake[0].allowed_capture_spaces[0].as_str(),
            "profile_dense"
        );

        remove_temp_db(&db_path);
    }

    #[test]
    fn applied_session_memory_proposals_create_and_update_records() {
        let db_path = temp_db_path("session-memory-proposal-apply");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        store.save_session(&sample_session_state()).unwrap();
        let descriptor = session_memory_space_descriptor();
        let add_proposal = session_memory_record_proposal(
            "session_memory_proposal_add",
            MemoryOperation::Add,
            session_fact_content(
                "session-fact-proposal",
                "User chose the sqlite-first deployment path.",
                "2026-06-26T02:00:00Z",
            ),
        );

        let created = store
            .save_memory_proposal(
                &add_proposal,
                &descriptor,
                &"2026-06-26T02:00:00Z".to_string(),
            )
            .unwrap();
        assert_eq!(created.status, MemoryProposalReviewStatus::PendingReview);
        assert!(store
            .query_session_memory_records(&SessionMemoryQuery {
                session_id: Some(SessionId::new("session-alpha")),
                ..SessionMemoryQuery::default()
            })
            .unwrap()
            .is_empty());
        assert_eq!(store.count_rows("message_slots").unwrap(), 0);
        assert_eq!(store.count_rows("profile_memories").unwrap(), 0);

        store
            .record_memory_governance_decision(
                &MemoryGovernanceDecisionInput {
                    decision_id: "session_memory_decision_approve".to_string(),
                    proposal_id: "session_memory_proposal_add".to_string(),
                    decision: MemoryGovernanceDecisionKind::Approved,
                    actor: "human_operator".to_string(),
                    source: MemoryProposalSource::Human,
                    evidence_refs: session_memory_evidence("ui-review"),
                    policy_mode: MemoryGovernanceMode::ManualReview,
                    confidence: Some(0.95),
                    message: Some("approved session memory add".to_string()),
                    resulting_revision: None,
                    decided_at: None,
                },
                &"2026-06-26T02:01:00Z".to_string(),
            )
            .unwrap();
        assert!(store
            .query_session_memory_records(&SessionMemoryQuery {
                session_id: Some(SessionId::new("session-alpha")),
                ..SessionMemoryQuery::default()
            })
            .unwrap()
            .is_empty());

        let applied = store
            .record_memory_governance_decision(
                &MemoryGovernanceDecisionInput {
                    decision_id: "session_memory_decision_apply".to_string(),
                    proposal_id: "session_memory_proposal_add".to_string(),
                    decision: MemoryGovernanceDecisionKind::Applied,
                    actor: "curator".to_string(),
                    source: MemoryProposalSource::Human,
                    evidence_refs: session_memory_evidence("ui-apply"),
                    policy_mode: MemoryGovernanceMode::ManualReview,
                    confidence: Some(0.97),
                    message: Some("apply session memory add".to_string()),
                    resulting_revision: None,
                    decided_at: None,
                },
                &"2026-06-26T02:02:00Z".to_string(),
            )
            .unwrap();
        assert_eq!(applied.resulting_revision, Some(1));
        let records = store
            .query_session_memory_records(&SessionMemoryQuery {
                session_id: Some(SessionId::new("session-alpha")),
                ..SessionMemoryQuery::default()
            })
            .unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].record_id, "session-fact-proposal");
        assert_eq!(records[0].revision, 1);
        assert_eq!(records[0].source, MemoryProposalSource::CaptureProducer);
        assert_eq!(
            records[0].durability_rationale,
            "Session proposal should survive future wakes."
        );
        assert_eq!(records[0].evidence_refs, add_proposal.evidence_refs);
        assert_eq!(store.count_rows("message_slots").unwrap(), 0);
        assert_eq!(store.count_rows("profile_memories").unwrap(), 0);

        let replace_proposal = session_memory_record_proposal(
            "session_memory_proposal_replace",
            MemoryOperation::Replace,
            {
                let mut content = session_fact_content(
                    "session-fact-proposal",
                    "User chose sqlite-first deployment before Postgres shakedown.",
                    "2026-06-26T02:03:00Z",
                );
                content["expected_revision"] = json!(1);
                content
            },
        );
        store
            .save_memory_proposal(
                &replace_proposal,
                &descriptor,
                &"2026-06-26T02:03:00Z".to_string(),
            )
            .unwrap();
        store
            .record_memory_governance_decision(
                &MemoryGovernanceDecisionInput {
                    decision_id: "session_memory_replace_approve".to_string(),
                    proposal_id: "session_memory_proposal_replace".to_string(),
                    decision: MemoryGovernanceDecisionKind::Approved,
                    actor: "human_operator".to_string(),
                    source: MemoryProposalSource::Human,
                    evidence_refs: session_memory_evidence("ui-review-replace"),
                    policy_mode: MemoryGovernanceMode::ManualReview,
                    confidence: Some(0.94),
                    message: Some("approved session memory replace".to_string()),
                    resulting_revision: None,
                    decided_at: None,
                },
                &"2026-06-26T02:04:00Z".to_string(),
            )
            .unwrap();
        let replaced = store
            .record_memory_governance_decision(
                &MemoryGovernanceDecisionInput {
                    decision_id: "session_memory_replace_apply".to_string(),
                    proposal_id: "session_memory_proposal_replace".to_string(),
                    decision: MemoryGovernanceDecisionKind::Applied,
                    actor: "curator".to_string(),
                    source: MemoryProposalSource::Human,
                    evidence_refs: session_memory_evidence("ui-apply-replace"),
                    policy_mode: MemoryGovernanceMode::ManualReview,
                    confidence: Some(0.96),
                    message: Some("apply session memory replace".to_string()),
                    resulting_revision: None,
                    decided_at: None,
                },
                &"2026-06-26T02:05:00Z".to_string(),
            )
            .unwrap();
        assert_eq!(replaced.resulting_revision, Some(2));
        let replaced_record = store
            .query_session_memory_records(&SessionMemoryQuery {
                session_id: Some(SessionId::new("session-alpha")),
                ..SessionMemoryQuery::default()
            })
            .unwrap()
            .pop()
            .expect("updated session memory record");
        assert_eq!(replaced_record.revision, 2);
        assert_eq!(
            replaced_record.content["content"],
            "User chose sqlite-first deployment before Postgres shakedown."
        );
        assert_eq!(
            replaced_record.evidence_refs,
            replace_proposal.evidence_refs
        );
        assert_eq!(store.count_rows("message_slots").unwrap(), 0);
        assert_eq!(store.count_rows("profile_memories").unwrap(), 0);

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
                        projection: None,
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
                        projection: None,
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
                        projection: None,
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
                        projection: None,
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
                projection: None,
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
                worker_pool_work_item_id: None,
                worker_pool_lease_id: None,
                worker_pool_member_id: None,
                worker_pool_claim_token: None,
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
                        projection: None,
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
                        projection: None,
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
    fn context_compaction_artifacts_preserve_raw_message_history() {
        let db_path = temp_db_path("context-compaction-artifacts");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        let session_id = SessionId::new("session-1");
        let slot_id = MessageSlotId::new("slot-context");
        let variant_id = MessageVariantId::new("variant-context-primary");
        store
            .save_message_slot(&MessageSlotWrite {
                slot_id: slot_id.clone(),
                session_id: session_id.clone(),
                primary_variant_id: variant_id.clone(),
                active_variant_id: None,
                metadata_json: json!({"fixture": "context_compaction"}),
                created_at: "2026-06-30T00:00:00Z".to_string(),
                updated_at: "2026-06-30T00:00:00Z".to_string(),
            })
            .unwrap();
        store
            .save_message_variant(&variant_write(
                &slot_id,
                &variant_id,
                MessageVariantSource::Primary,
                0,
                "message-context",
                "raw context compaction source text",
            ))
            .unwrap();
        let slots_before = store.count_rows("message_slots").unwrap();
        let variants_before = store.count_rows("message_variants").unwrap();

        let artifact = ContextCompactionArtifact {
            artifact_id: "artifact_context_one".to_string(),
            session_id: session_id.clone(),
            branch_id: None,
            strategy_id: "rolling_summary_compaction".to_string(),
            source_refs_json: json!({
                "message_slot_ids": [slot_id.0.as_str()],
                "message_variant_ids": [variant_id.0.as_str()],
                "cursor_range": {"from": "session-1:0", "to": "session-1:1"}
            }),
            provider_metadata_json: json!({
                "provider_alias": "deepseek-flash",
                "model_id": "deepseek-chat"
            }),
            estimate_before_json: json!({
                "estimator_id": "fallback_chars_words_v1",
                "estimated_prompt_tokens": 85000
            }),
            estimate_after_json: Some(json!({
                "estimated_prompt_tokens": 24000
            })),
            summary_text: "The conversation discussed durable compaction provenance.".to_string(),
            enters_future_context: true,
            context_policy: "summary_context".to_string(),
            metadata_json: json!({"created_by": "test"}),
            created_at: "2026-06-30T00:01:00Z".to_string(),
            updated_at: "2026-06-30T00:01:00Z".to_string(),
        };
        let saved = store.save_context_compaction_artifact(&artifact).unwrap();
        assert_eq!(saved.artifact_id, "artifact_context_one");
        assert_eq!(saved.strategy_id, "rolling_summary_compaction");

        let latest = store
            .list_context_compaction_artifacts(&ContextCompactionArtifactQuery {
                session_id: Some(session_id.clone()),
                branch_id: None,
                strategy_id: Some("rolling_summary_compaction".to_string()),
                enters_future_context: Some(true),
                latest_only: true,
                limit: None,
                offset: None,
            })
            .unwrap();
        assert_eq!(latest, vec![artifact]);
        assert_eq!(store.count_rows("message_slots").unwrap(), slots_before);
        assert_eq!(
            store.count_rows("message_variants").unwrap(),
            variants_before
        );
        let slots_after = store
            .query_message_slots(&MessageSlotQuery {
                session_id: Some(session_id),
                include_alternates: false,
                page: None,
            })
            .unwrap();
        assert_eq!(
            slots_after[0].primary.message.body,
            "raw context compaction source text"
        );
        drop(store);

        let reopened = CoordinationStore::open_file(&db_path).unwrap();
        let reopened_artifacts = reopened
            .list_context_compaction_artifacts(&ContextCompactionArtifactQuery {
                session_id: Some(SessionId::new("session-1")),
                branch_id: None,
                strategy_id: None,
                enters_future_context: None,
                latest_only: true,
                limit: None,
                offset: None,
            })
            .unwrap();
        assert_eq!(reopened_artifacts.len(), 1);
        assert_eq!(reopened_artifacts[0].artifact_id, "artifact_context_one");
        let reopened_slots = reopened
            .query_message_slots(&MessageSlotQuery {
                session_id: Some(SessionId::new("session-1")),
                include_alternates: false,
                page: None,
            })
            .unwrap();
        assert_eq!(
            reopened_slots[0].primary.message.body,
            "raw context compaction source text"
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
                ..AttachmentQuery::default()
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
                ..AttachmentQuery::default()
            })
            .unwrap();
        assert_eq!(by_scope.len(), 1);

        let scopes = store
            .query_data_bank_scopes(&DataBankScopeQuery {
                session_id: Some(session_id.clone()),
                include_removed: false,
                ..DataBankScopeQuery::default()
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
                ..DataBankScopeQuery::default()
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
                    worker_pool_work_item_id: None,
                    worker_pool_lease_id: None,
                    worker_pool_member_id: None,
                    worker_pool_claim_token: None,
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
                                projection: None,
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
                        projection: None,
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
                    projection: None,
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
                ..RuntimeMaintenancePolicy::default()
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

    #[test]
    fn worker_pool_member_registration_claim_and_completion_round_trip() {
        let db_path = temp_db_path("worker-pool-round-trip");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        store
            .upsert_worker_pool_member(&sample_worker_pool_member(
                "member-a",
                "worker-profile",
                WorkerPoolMemberStatus::Available,
                1,
                0,
                "2026-06-30T00:00:00Z",
            ))
            .unwrap();
        assert!(store
            .heartbeat_worker_pool_member(
                "member-a",
                WorkerPoolMemberStatus::Available,
                &"2026-06-30T00:00:10Z".to_string(),
            )
            .unwrap());
        store
            .create_worker_pool_work_item(&sample_worker_pool_work_item(
                "work-b",
                Some("worker-profile"),
                20,
                "2026-06-30T00:00:11Z",
            ))
            .unwrap();
        store
            .create_worker_pool_work_item(&sample_worker_pool_work_item(
                "work-a",
                Some("worker-profile"),
                10,
                "2026-06-30T00:00:12Z",
            ))
            .unwrap();

        let claim = store
            .claim_next_worker_pool_work_item(&WorkerPoolClaimRequest {
                member_id: "member-a".to_string(),
                lease_id: "lease-a".to_string(),
                claim_token: "token-a".to_string(),
                now: "2026-06-30T00:00:13Z".to_string(),
                claim_deadline_at: "2026-06-30T00:10:00Z".to_string(),
                min_heartbeat_at: "2026-06-30T00:00:00Z".to_string(),
            })
            .unwrap()
            .unwrap();
        assert_eq!(claim.work_item.work_item_id, "work-a");
        assert_eq!(claim.work_item.status, WorkerPoolWorkStatus::Claimed);
        assert_eq!(claim.member.active_leases, 1);
        assert_eq!(claim.member.status, WorkerPoolMemberStatus::Busy);

        let no_capacity = store
            .claim_next_worker_pool_work_item(&WorkerPoolClaimRequest {
                member_id: "member-a".to_string(),
                lease_id: "lease-b".to_string(),
                claim_token: "token-b".to_string(),
                now: "2026-06-30T00:00:14Z".to_string(),
                claim_deadline_at: "2026-06-30T00:10:00Z".to_string(),
                min_heartbeat_at: "2026-06-30T00:00:00Z".to_string(),
            })
            .unwrap()
            .unwrap_err();
        assert_eq!(no_capacity, WorkerPoolNoCapacityReason::MemberAtCapacity);

        assert!(store
            .complete_worker_pool_work_item(&WorkerPoolCompletionRequest {
                lease_id: "lease-a".to_string(),
                claim_token: "token-a".to_string(),
                status: WorkerPoolWorkStatus::Completed,
                now: "2026-06-30T00:00:15Z".to_string(),
                summary: Some("done".to_string()),
            })
            .unwrap());
        let member = store.load_worker_pool_member("member-a").unwrap().unwrap();
        assert_eq!(member.active_leases, 0);
        assert_eq!(member.status, WorkerPoolMemberStatus::Available);
        let work = store.load_worker_pool_work_item("work-a").unwrap().unwrap();
        assert_eq!(work.status, WorkerPoolWorkStatus::Completed);
        assert_eq!(work.terminal_summary.as_deref(), Some("done"));

        remove_temp_db(&db_path);
    }

    #[test]
    fn worker_pool_stale_member_cannot_claim() {
        let db_path = temp_db_path("worker-pool-stale-member");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        store
            .upsert_worker_pool_member(&sample_worker_pool_member(
                "member-a",
                "worker-profile",
                WorkerPoolMemberStatus::Available,
                1,
                0,
                "2026-06-30T00:00:00Z",
            ))
            .unwrap();
        store
            .create_worker_pool_work_item(&sample_worker_pool_work_item(
                "work-a",
                Some("worker-profile"),
                10,
                "2026-06-30T00:00:01Z",
            ))
            .unwrap();

        let reason = store
            .claim_next_worker_pool_work_item(&WorkerPoolClaimRequest {
                member_id: "member-a".to_string(),
                lease_id: "lease-a".to_string(),
                claim_token: "token-a".to_string(),
                now: "2026-06-30T00:00:02Z".to_string(),
                claim_deadline_at: "2026-06-30T00:10:00Z".to_string(),
                min_heartbeat_at: "2026-06-30T00:00:01Z".to_string(),
            })
            .unwrap()
            .unwrap_err();
        assert_eq!(reason, WorkerPoolNoCapacityReason::MemberHeartbeatStale);
        assert_eq!(
            store
                .load_worker_pool_work_item("work-a")
                .unwrap()
                .unwrap()
                .status,
            WorkerPoolWorkStatus::Pending
        );

        remove_temp_db(&db_path);
    }

    #[test]
    fn worker_pool_claim_token_fences_terminal_completion() {
        let db_path = temp_db_path("worker-pool-token-fence");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        store
            .upsert_worker_pool_member(&sample_worker_pool_member(
                "member-a",
                "worker-profile",
                WorkerPoolMemberStatus::Available,
                1,
                0,
                "2026-06-30T00:00:00Z",
            ))
            .unwrap();
        store
            .create_worker_pool_work_item(&sample_worker_pool_work_item(
                "work-a",
                Some("worker-profile"),
                10,
                "2026-06-30T00:00:01Z",
            ))
            .unwrap();
        store
            .claim_next_worker_pool_work_item(&WorkerPoolClaimRequest {
                member_id: "member-a".to_string(),
                lease_id: "lease-a".to_string(),
                claim_token: "token-a".to_string(),
                now: "2026-06-30T00:00:02Z".to_string(),
                claim_deadline_at: "2026-06-30T00:10:00Z".to_string(),
                min_heartbeat_at: "2026-06-30T00:00:00Z".to_string(),
            })
            .unwrap()
            .unwrap();

        assert!(!store
            .complete_worker_pool_work_item(&WorkerPoolCompletionRequest {
                lease_id: "lease-a".to_string(),
                claim_token: "wrong-token".to_string(),
                status: WorkerPoolWorkStatus::Completed,
                now: "2026-06-30T00:00:03Z".to_string(),
                summary: None,
            })
            .unwrap());
        assert!(store
            .complete_worker_pool_work_item(&WorkerPoolCompletionRequest {
                lease_id: "lease-a".to_string(),
                claim_token: "token-a".to_string(),
                status: WorkerPoolWorkStatus::Completed,
                now: "2026-06-30T00:00:04Z".to_string(),
                summary: Some("done".to_string()),
            })
            .unwrap());
        assert!(!store
            .complete_worker_pool_work_item(&WorkerPoolCompletionRequest {
                lease_id: "lease-a".to_string(),
                claim_token: "token-a".to_string(),
                status: WorkerPoolWorkStatus::Failed,
                now: "2026-06-30T00:00:05Z".to_string(),
                summary: Some("too late".to_string()),
            })
            .unwrap());
        assert_eq!(
            store
                .load_worker_pool_work_item("work-a")
                .unwrap()
                .unwrap()
                .status,
            WorkerPoolWorkStatus::Completed
        );

        remove_temp_db(&db_path);
    }

    #[test]
    fn worker_pool_expired_claims_are_terminal_not_resurrected() {
        let db_path = temp_db_path("worker-pool-expiry");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        store
            .upsert_worker_pool_member(&sample_worker_pool_member(
                "member-a",
                "worker-profile",
                WorkerPoolMemberStatus::Available,
                1,
                0,
                "2026-06-30T00:00:00Z",
            ))
            .unwrap();
        store
            .create_worker_pool_work_item(&sample_worker_pool_work_item(
                "work-a",
                Some("worker-profile"),
                10,
                "2026-06-30T00:00:01Z",
            ))
            .unwrap();
        store
            .claim_next_worker_pool_work_item(&WorkerPoolClaimRequest {
                member_id: "member-a".to_string(),
                lease_id: "lease-a".to_string(),
                claim_token: "token-a".to_string(),
                now: "2026-06-30T00:00:02Z".to_string(),
                claim_deadline_at: "2026-06-30T00:00:03Z".to_string(),
                min_heartbeat_at: "2026-06-30T00:00:00Z".to_string(),
            })
            .unwrap()
            .unwrap();

        let expired = store
            .expire_worker_pool_claims(
                &"2026-06-30T00:00:04Z".to_string(),
                &"2026-06-30T00:00:05Z".to_string(),
            )
            .unwrap();
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].status, WorkerPoolWorkStatus::Expired);
        assert!(!store
            .complete_worker_pool_work_item(&WorkerPoolCompletionRequest {
                lease_id: "lease-a".to_string(),
                claim_token: "token-a".to_string(),
                status: WorkerPoolWorkStatus::Completed,
                now: "2026-06-30T00:00:06Z".to_string(),
                summary: Some("too late".to_string()),
            })
            .unwrap());
        assert_eq!(
            store
                .load_worker_pool_work_item("work-a")
                .unwrap()
                .unwrap()
                .status,
            WorkerPoolWorkStatus::Expired
        );
        assert_eq!(
            store
                .load_worker_pool_member("member-a")
                .unwrap()
                .unwrap()
                .active_leases,
            0
        );

        remove_temp_db(&db_path);
    }

    fn sample_worker_pool_member(
        member_id: &str,
        profile_id: &str,
        status: WorkerPoolMemberStatus,
        concurrency_limit: u32,
        active_leases: u32,
        now: &str,
    ) -> WorkerPoolMemberRecord {
        WorkerPoolMemberRecord {
            member_id: member_id.to_string(),
            profile_id: ProfileId(profile_id.to_string()),
            agent_id: Some(AgentId(format!("{member_id}-agent"))),
            session_id: Some(SessionId(format!("{member_id}-session"))),
            status,
            concurrency_limit,
            active_leases,
            capabilities_json: json!({"skills": ["review"]}),
            registered_at: now.to_string(),
            last_heartbeat_at: now.to_string(),
            updated_at: now.to_string(),
        }
    }

    fn sample_worker_pool_work_item(
        work_item_id: &str,
        requested_profile_id: Option<&str>,
        priority: i32,
        now: &str,
    ) -> WorkerPoolWorkItemRecord {
        WorkerPoolWorkItemRecord {
            work_item_id: work_item_id.to_string(),
            requested_profile_id: requested_profile_id.map(|value| ProfileId(value.to_string())),
            task_id: Some(TaskId(format!("task-{work_item_id}"))),
            status: WorkerPoolWorkStatus::Pending,
            priority,
            work_json: json!({"handoff_markdown": "Please review this slice."}),
            required_capabilities_json: json!({"skills": ["review"]}),
            created_at: now.to_string(),
            updated_at: now.to_string(),
            claimed_by_member_id: None,
            lease_id: None,
            claim_token: None,
            claim_deadline_at: None,
            terminal_at: None,
            terminal_summary: None,
        }
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

    fn temp_data_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("rusty-crew-{label}-{}-{nanos}", std::process::id()))
    }

    fn roleplay_lore_fts_matches(conn: &Connection, query: &str) -> i64 {
        conn.query_row(
            "SELECT count(*)
             FROM module_roleplay_lore_records_fts
             WHERE module_roleplay_lore_records_fts MATCH ?1",
            params![query],
            |row| row.get::<_, i64>(0),
        )
        .unwrap()
    }

    fn remove_temp_db(db_path: &Path) {
        let _ = fs::remove_file(db_path);
        let _ = fs::remove_file(format!("{}-wal", db_path.display()));
        let _ = fs::remove_file(format!("{}-shm", db_path.display()));
    }

    fn remove_temp_dir(path: &Path) {
        let _ = fs::remove_dir_all(path);
    }

    fn logical_import_bundle(
        repositories: Vec<LogicalStorageRepositoryBundle>,
    ) -> LogicalStorageExportBundle {
        LogicalStorageExportBundle {
            bundle_version: 1,
            export_id: "export-1".to_string(),
            exported_at: "2026-06-26T10:00:00Z".to_string(),
            service_version: Some("test".to_string()),
            source: LogicalStorageExportSource {
                backend: "sqlite".to_string(),
                backend_label: "SQLite".to_string(),
                source_instance_id: Some("test-instance".to_string()),
                snapshot_ref: Some("logical://export-1".to_string()),
            },
            schema_version: CURRENT_SCHEMA_VERSION,
            module_versions: vec![LogicalStorageModuleVersion {
                module_id: "simple_kv".to_string(),
                schema_version: 1,
                descriptor_fingerprint: Some("test-fingerprint".to_string()),
            }],
            capability_snapshot: vec![LogicalStorageCapabilitySnapshot {
                name: "transactions".to_string(),
                supported: true,
                detail: Some("test capability".to_string()),
            }],
            repositories,
            legacy_id_mappings: vec![LogicalStorageLegacyIdMapping {
                source_system: "legacy-test".to_string(),
                legacy_kind: RuntimeObjectKind::ExternalArtifact,
                legacy_id: "legacy-1".to_string(),
                rusty_kind: RuntimeObjectKind::ExternalArtifact,
                rusty_id: "rusty-1".to_string(),
                provenance: RuntimeImportProvenance::default(),
            }],
            profile_asset_refs: vec![LogicalStorageProfileAssetRef {
                profile_id: ProfileId::new("rusty-crew-runner"),
                asset_kind: "soul".to_string(),
                asset_ref: "profiles/rusty-crew-runner/soul.md".to_string(),
                checksum: None,
                bundled: false,
            }],
        }
    }

    fn logical_queue_message(
        message_id: &str,
        state: QueuedMessageState,
        expires_at: &str,
        terminal_at: Option<&str>,
    ) -> LogicalQueuedMessageExportRecord {
        LogicalQueuedMessageExportRecord {
            message_id: message_id.to_string(),
            owner_session_id: Some(SessionId::new("session-alpha")),
            owner_agent_id: AgentId::new("agent-alpha"),
            message: AgentMessage {
                from: AgentId::new("operator"),
                to: AgentId::new("agent-alpha"),
                body: format!("logical import queue {message_id}"),
                correlation_id: Some("logical-import-queue".to_string()),
                projection: None,
            },
            source_sequence: Some(7),
            enqueued_at: "2026-06-26T09:58:00Z".to_string(),
            expires_at: expires_at.to_string(),
            ttl_ms: 5_000,
            delivery_attempts: 0,
            state,
            terminal_at: terminal_at.map(str::to_string),
            state_reason: None,
        }
    }

    fn assert_active_storage_signal(diagnostics: &RuntimeStorageDiagnostics, signal_name: &str) {
        let signal = diagnostics
            .pressure_signals
            .iter()
            .find(|signal| signal.name == signal_name)
            .unwrap_or_else(|| panic!("missing storage pressure signal {signal_name}"));
        assert!(
            signal.active,
            "expected active storage pressure signal {signal_name}: {signal:?}"
        );
    }

    fn assert_inactive_storage_signal(diagnostics: &RuntimeStorageDiagnostics, signal_name: &str) {
        let signal = diagnostics
            .pressure_signals
            .iter()
            .find(|signal| signal.name == signal_name)
            .unwrap_or_else(|| panic!("missing storage pressure signal {signal_name}"));
        assert!(
            !signal.active,
            "expected inactive storage pressure signal {signal_name}: {signal:?}"
        );
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

    fn session_fact_memory_write(
        record_id: &str,
        session_id: &SessionId,
        now: &str,
    ) -> SessionMemoryRecordWrite {
        SessionMemoryRecordWrite {
            record_id: record_id.to_string(),
            session_id: session_id.clone(),
            scope: MemoryScope {
                scope_type: MemoryScopeType::Session,
                scope_id: session_id.0.clone(),
            },
            branch_id: None,
            shape: MemoryRecordShapeRef {
                shape_id: MemoryRecordShapeId::unchecked("session_fact"),
                version: 1,
            },
            content: session_fact_content(record_id, "The user prefers slow-burn pacing.", now),
            evidence_refs: session_memory_evidence("wake-add"),
            source: MemoryProposalSource::CaptureProducer,
            confidence: 0.9,
            durability_rationale:
                "Session fact should survive future wakes without duplicating transcript text."
                    .to_string(),
            supersedes_record_id: None,
            now: now.to_string(),
        }
    }

    fn replace_session_fact_input(record_id: &str) -> SessionMemoryReplace {
        SessionMemoryReplace {
            record_id: record_id.to_string(),
            expected_revision: 1,
            content: session_fact_content(
                record_id,
                "Stale replacement should be rejected.",
                "2026-06-26T01:02:00Z",
            ),
            evidence_refs: session_memory_evidence("wake-stale"),
            source: MemoryProposalSource::Human,
            confidence: 0.8,
            durability_rationale: "Testing stale revision behavior.".to_string(),
            now: "2026-06-26T01:02:00Z".to_string(),
        }
    }

    fn session_fact_content(record_id: &str, content: &str, now: &str) -> JsonValue {
        json!({
            "record_id": record_id,
            "content": content,
            "fact_kind": "preference",
            "confidence": 0.9,
            "source_summary": "Observed during a session wake.",
            "created_at": now,
            "updated_at": now
        })
    }

    fn session_memory_record_proposal(
        proposal_id: &str,
        operation: MemoryOperation,
        content: JsonValue,
    ) -> MemoryProposalEnvelope {
        MemoryProposalEnvelope {
            proposal_id: proposal_id.to_string(),
            space_id: MemorySpaceId::unchecked("session_memory"),
            operation,
            scope: MemoryScope {
                scope_type: MemoryScopeType::Session,
                scope_id: "session-alpha".to_string(),
            },
            shape: MemoryRecordShapeRef {
                shape_id: MemoryRecordShapeId::unchecked("session_fact"),
                version: 1,
            },
            content,
            evidence_refs: session_memory_evidence("wake-proposal"),
            confidence: 0.86,
            durability_rationale: Some("Session proposal should survive future wakes.".to_string()),
            governance_mode: MemoryGovernanceMode::ManualReview,
            source: MemoryProposalSource::CaptureProducer,
            dedupe_key: Some(format!("session_memory:{proposal_id}")),
            created_at: None,
        }
    }

    fn branch_summary_memory_write(
        record_id: &str,
        session_id: &SessionId,
        branch_id: &ConversationBranchId,
        now: &str,
    ) -> SessionMemoryRecordWrite {
        SessionMemoryRecordWrite {
            record_id: record_id.to_string(),
            session_id: session_id.clone(),
            scope: MemoryScope {
                scope_type: MemoryScopeType::ConversationBranch,
                scope_id: branch_id.0.clone(),
            },
            branch_id: Some(branch_id.clone()),
            shape: MemoryRecordShapeRef {
                shape_id: MemoryRecordShapeId::unchecked("branch_summary"),
                version: 1,
            },
            content: json!({
                "record_id": record_id,
                "summary": "The branch followed the quiet clue trail.",
                "branch_id": branch_id.0,
                "head_message_id": "message-alpha",
                "coverage_start": "message-root",
                "coverage_end": "message-alpha",
                "created_at": now,
                "updated_at": now
            }),
            evidence_refs: session_memory_evidence("wake-branch"),
            source: MemoryProposalSource::CaptureProducer,
            confidence: 0.87,
            durability_rationale: "Branch summary should survive branch navigation.".to_string(),
            supersedes_record_id: None,
            now: now.to_string(),
        }
    }

    fn branch_user_choice_memory_write(
        record_id: &str,
        session_id: &SessionId,
        branch_id: &ConversationBranchId,
        now: &str,
    ) -> SessionMemoryRecordWrite {
        SessionMemoryRecordWrite {
            record_id: record_id.to_string(),
            session_id: session_id.clone(),
            scope: MemoryScope {
                scope_type: MemoryScopeType::ConversationBranch,
                scope_id: branch_id.0.clone(),
            },
            branch_id: Some(branch_id.clone()),
            shape: MemoryRecordShapeRef {
                shape_id: MemoryRecordShapeId::unchecked("user_choice"),
                version: 1,
            },
            content: json!({
                "record_id": record_id,
                "choice": "The user kept the active branch.",
                "choice_kind": "branch_direction",
                "chosen_at": now,
                "status": "active",
                "created_at": now,
                "updated_at": now
            }),
            evidence_refs: session_memory_evidence("wake-branch-choice"),
            source: MemoryProposalSource::CaptureProducer,
            confidence: 0.84,
            durability_rationale: "Branch choice should survive branch navigation.".to_string(),
            supersedes_record_id: None,
            now: now.to_string(),
        }
    }

    fn save_branch_tree(store: &CoordinationStore) {
        for (branch_id, parent_branch_id, now) in [
            ("branch-root", None, "2026-06-26T01:00:00Z"),
            ("branch-active", Some("branch-root"), "2026-06-26T01:01:00Z"),
            (
                "branch-sibling",
                Some("branch-root"),
                "2026-06-26T01:02:00Z",
            ),
        ] {
            store
                .save_conversation_branch(&ConversationBranchWrite {
                    branch_id: ConversationBranchId::new(branch_id),
                    session_id: SessionId::new("session-alpha"),
                    parent_branch_id: parent_branch_id.map(ConversationBranchId::new),
                    parent_message_id: None,
                    origin_message_id: Some(MessageId::new(format!("{branch_id}:origin"))),
                    head_message_id: Some(MessageId::new(format!("{branch_id}:head"))),
                    label: Some(branch_id.to_string()),
                    metadata_json: json!({"fixture": true}),
                    created_at: now.to_string(),
                    updated_at: now.to_string(),
                })
                .unwrap();
        }
    }

    fn session_memory_evidence(ref_id: &str) -> Vec<MemoryEvidenceRef> {
        vec![MemoryEvidenceRef {
            evidence_type: MemoryEvidenceKind::Wake,
            ref_id: ref_id.to_string(),
            label: Some("Test wake".to_string()),
        }]
    }

    fn roleplay_lore_write(
        record_id: &str,
        world_id: &str,
        entity_id: Option<&str>,
        title: &str,
        body: &str,
        now: &str,
    ) -> RoleplayLoreWrite {
        RoleplayLoreWrite {
            record_id: record_id.to_string(),
            world_id: world_id.to_string(),
            entity_id: entity_id.map(ToOwned::to_owned),
            session_id: None,
            branch_id: None,
            shape: MemoryRecordShapeRef {
                shape_id: MemoryRecordShapeId::unchecked("lore_entry"),
                version: 1,
            },
            canon_status: RoleplayLoreCanonStatus::Canon,
            visibility: RoleplayLoreVisibility::Public,
            title: title.to_string(),
            body: body.to_string(),
            content: json!({
                "world_id": world_id,
                "entity_id": entity_id,
                "title": title,
                "body": body,
                "canon_status": "canon",
                "visibility": "public",
                "metadata_json": {"fixture": "roleplay_lore_layers"}
            }),
            evidence_refs: session_memory_evidence("wake-roleplay-lore"),
            source: MemoryProposalSource::Human,
            confidence: 0.92,
            durability_rationale: "Roleplay lore fixture should survive recall.".to_string(),
            supersedes_record_id: None,
            now: now.to_string(),
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
            prompt_soul_markdown: Some("You are a registry-backed runner.".to_string()),
            prompt_memory_markdown: Some("Static deployment-safe memory.".to_string()),
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

    fn model_provider_write(
        alias: &str,
        protocol: ModelProviderProtocol,
        provider_kind: &str,
        model_id: &str,
        secret: Option<&str>,
    ) -> ModelProviderWrite {
        ModelProviderWrite {
            alias: alias.to_string(),
            status: ModelProviderStatus::Active,
            protocol,
            provider_kind: provider_kind.to_string(),
            display_name: Some(alias.to_string()),
            description: None,
            base_url: Some("http://127.0.0.1:18082".to_string()),
            model_id: model_id.to_string(),
            context_window_tokens: Some(128_000),
            max_output_tokens: Some(4_096),
            temperature_milli: Some(500),
            reasoning_effort: None,
            reasoning_format: None,
            secret: secret.map(ToString::to_string),
            clear_secret: false,
            metadata_json: json!({"fixture": "model_provider_secret_envelope"}),
            expected_revision: None,
            now: "2026-07-02T00:00:00Z".to_string(),
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
