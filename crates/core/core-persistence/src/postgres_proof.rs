//! Narrow PostgreSQL proof slice for the runtime counter repository.
//!
//! This module is intentionally not the full `CoordinationStore` backend. It
//! exists to prove connection, migration, typed API parity, and diagnostics for
//! one low-risk repository before correctness-sensitive coordination state moves
//! beyond SQLite.

use crate::{
    counter_value, from_json_text, profile_memory_target_parts, repositories,
    roleplay_lore_canon_status_as_str, roleplay_lore_memory_space_descriptor,
    roleplay_lore_record_status_as_str, roleplay_lore_visibility_as_str, to_json_text,
    validate_profile_memory_key, validate_profile_memory_write, validate_provider_wire_state_key,
    validate_roleplay_lore_record_id, validate_roleplay_lore_write, validate_simple_kv_identity,
    validate_simple_kv_query, validate_simple_kv_write, ActiveBranchConflict,
    ActiveBranchExpectation, ActiveVariantConflict, ActiveVariantExpectation, AdapterId, AgentId,
    AgentInstanceId, AttachmentId, AttachmentLinkId, AttachmentLinkRecord, AttachmentLinkWrite,
    AttachmentQuery, AttachmentRecord, AttachmentStatus, AttachmentWrite,
    BranchAwareSessionMemoryQuery, BranchHeadConflict, BranchHeadExpectation, ChannelBindingQuery,
    ChannelBindingRecord, CompletionPacketQuery, CompletionPacketRecord, ConversationBranchId,
    ConversationBranchQuery, ConversationBranchRecord, ConversationBranchStateRecord,
    ConversationBranchWrite, ConversationJumpRequest, ConversationJumpResult,
    ConversationJumpTarget, ConversationSnapshotId, ConversationSnapshotQuery,
    ConversationSnapshotRecord, ConversationSnapshotSource, ConversationSnapshotWrite, CoreError,
    CoreErrorKind, CoreEvent, CoreEventKind, CoreResult, DataBankScopeId, DataBankScopeQuery,
    DataBankScopeRecord, DataBankScopeStatus, DataBankScopeWrite, DelegatedCompletion,
    DenRuntimeReference, DurableAgentKind, DurableAgentRecord, DurableIdentityStatus,
    DurableMessageRecord, DurableMessageStatus, DurableMessageWrite, ExternalBindingStatus,
    IsoTimestamp, McpBindingQuery, McpBindingRecord, MessageBlockRecord, MessageId, MessageSlotId,
    MessageSlotQuery, MessageSlotRecord, MessageSlotWrite, MessageVariantId, MessageVariantQuery,
    MessageVariantRecord, MessageVariantSource, MessageVariantStatus, MessageVariantWrite,
    ModelProviderCredential, ModelProviderProtocol, ModelProviderQuery, ModelProviderRecord,
    ModelProviderStatus, ModelProviderWrite, PersistedEvent, ProfileId, ProfileMemoryCaps,
    ProfileMemoryDelete, ProfileMemoryQuery, ProfileMemoryRecord, ProfileMemoryReplace,
    ProfileMemoryTarget, ProfileMemoryWrite, ProfileRegistryLifecycleStatus, ProfileRegistryQuery,
    ProfileRegistryRecord, ProfileRegistryUpdate, ProfileRegistryWrite, ProviderStateAbsenceReason,
    ProviderWireStateDiagnostic, ProviderWireStateInvalidationReason, ProviderWireStateKey,
    ProviderWireStateRecord, ProviderWireStateWakeLookup, ProviderWireStateWakeResult,
    ProviderWireStateWrite, QueryPage, QueuedMessageFilter, QueuedMessageRecord,
    QueuedMessageState, RoleplayLoreProvenanceEvent, RoleplayLoreQuery, RoleplayLoreRecord,
    RoleplayLoreRecordStatus, RoleplayLoreReplace, RoleplayLoreSupersede, RoleplayLoreTombstone,
    RoleplayLoreWrite, RunId, RuntimeCounterQuery, RuntimeCounterRecord, RuntimeCounterScope,
    RuntimeDatabaseSize, RuntimeEventFilter, RuntimeEventRecord, RuntimeMaintenancePolicy,
    RuntimeMaintenanceReport, RuntimeRepositoryGroupDiagnostic, RuntimeSearchFilter,
    RuntimeSearchResult, RuntimeSearchRowType, RuntimeStateSummary, RuntimeStorageCapability,
    RuntimeStorageTableCount, ScheduledJobQuery, ScheduledJobRecord, ScheduledJobStatus,
    ScheduledRunQuery, ScheduledRunRecord, ScheduledRunStatus, ScheduledRunTrigger,
    SelectActiveBranchRequest, SelectActiveBranchResult, SelectActiveVariantRequest,
    SelectActiveVariantResult, SessionConfig, SessionConfigRecord, SessionId,
    SessionIdentityRecord, SessionKind, SessionMemoryArchive, SessionMemoryCompactionReport,
    SessionMemoryPromptContext, SessionMemoryPromptContextPolicy, SessionMemoryPromptDiagnostics,
    SessionMemoryPromptExcludedCounts, SessionMemoryQuery, SessionMemoryRecord,
    SessionMemoryRecordStatus, SessionMemoryRecordWrite, SessionMemoryReplace,
    SessionMemorySelectedRecordDiagnostic, SessionMemorySupersede, SessionState, SessionStatus,
    SimpleKvCompareAndSwap, SimpleKvDelete, SimpleKvQuery, SimpleKvRecord, SimpleKvScope,
    SimpleKvWrite, TaskId, ToolCallPhase, ToolCallRecord, UpdateBranchHeadRequest,
    UpdateBranchHeadResult, WorkerRunQuery, WorkerRunRecord, WorkerRunStatus, COUNTER_BRAIN_TURNS,
    COUNTER_COMPLETIONS, COUNTER_DELEGATIONS_CANCELLED, COUNTER_DELEGATIONS_COMPLETED,
    COUNTER_DELEGATIONS_CREATED, COUNTER_DELEGATIONS_FAILED, COUNTER_DELEGATIONS_TIMED_OUT,
    COUNTER_MESSAGES, COUNTER_QUEUE_EXPIRATIONS, COUNTER_TOOL_CALLS, COUNTER_TOOL_ERRORS,
    COUNTER_WAKES,
};
use postgres::{Client, GenericClient, NoTls, Row, Transaction};
use rusty_crew_core_protocol::{
    session_memory_space_descriptor, BrainEvent, CompletionPacket, CompletionStatus,
    FanOutFailurePolicy, MemoryConflictPolicy, MemoryDiagnosticsPolicy, MemoryEvidenceKind,
    MemoryEvidenceRef, MemoryExportImportPolicy, MemoryFieldType, MemoryGovernanceDecisionInput,
    MemoryGovernanceDecisionKind, MemoryGovernanceDecisionRecord, MemoryGovernanceMode,
    MemoryIndexingPolicy, MemoryOperation, MemoryOperationPolicy, MemoryPromptPolicy,
    MemoryProposalEnvelope, MemoryProposalQuery, MemoryProposalRecord, MemoryProposalReviewStatus,
    MemoryProvenancePolicy, MemoryRecordFieldDescriptor, MemoryRecordShapeDescriptor,
    MemoryRecordShapeId, MemoryRecordShapeRef, MemoryRetentionPolicy, MemoryRetrievalStrategy,
    MemoryScope, MemoryScopeModel, MemoryScopeType, MemorySpaceDescriptor, MemorySpaceId,
    MemoryVisibilityModel, MemoryWritePolicy, ParentConsumptionPolicy,
};
use std::sync::{Mutex, MutexGuard};

const POSTGRES_PROOF_SCHEMA_VERSION: i64 = 13;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostgresRuntimeCounterProofConfig {
    pub database_url_env: String,
    pub schema: String,
}

impl Default for PostgresRuntimeCounterProofConfig {
    fn default() -> Self {
        Self {
            database_url_env: "RUSTY_CREW_DATABASE_URL".to_string(),
            schema: "rusty_crew".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostgresRuntimeCounterProofDiagnostics {
    pub backend: String,
    pub backend_label: String,
    pub schema: String,
    pub proof_repository: String,
    pub schema_version: i64,
    pub table_counts: Vec<RuntimeStorageTableCount>,
    pub capabilities: Vec<RuntimeStorageCapability>,
    pub repository_groups: Vec<RuntimeRepositoryGroupDiagnostic>,
}

pub struct PostgresRuntimeCounterProofStore {
    schema: String,
    client: Mutex<Client>,
}

impl std::fmt::Debug for PostgresRuntimeCounterProofStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PostgresRuntimeCounterProofStore")
            .field("schema", &self.schema)
            .finish_non_exhaustive()
    }
}

impl PostgresRuntimeCounterProofStore {
    pub fn connect_from_env(config: &PostgresRuntimeCounterProofConfig) -> CoreResult<Self> {
        let database_url = std::env::var(&config.database_url_env).map_err(|error| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!(
                    "load PostgreSQL proof database URL from env {}: {error}",
                    config.database_url_env
                ),
            )
        })?;
        Self::connect(&database_url, &config.schema)
    }

    pub fn connect(database_url: &str, schema: &str) -> CoreResult<Self> {
        validate_postgres_identifier("postgres schema", schema)?;
        let client = Client::connect(database_url, NoTls)
            .map_err(|error| postgres_error("connect PostgreSQL runtime counter proof", error))?;
        let store = Self {
            schema: schema.to_string(),
            client: Mutex::new(client),
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn save_session(&self, state: &SessionState) -> CoreResult<()> {
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start save PostgreSQL session", error))?;
        self.save_session_in_tx(&mut tx, state)?;
        tx.commit()
            .map_err(|error| postgres_error("commit save PostgreSQL session", error))?;
        Ok(())
    }

    pub fn save_session_with_config(
        &self,
        state: &SessionState,
        config: &SessionConfig,
    ) -> CoreResult<()> {
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start save PostgreSQL session with config", error))?;
        self.save_session_in_tx(&mut tx, state)?;
        self.save_session_config_in_tx(&mut tx, config, &state.created_at)?;
        tx.commit()
            .map_err(|error| postgres_error("commit save PostgreSQL session with config", error))?;
        Ok(())
    }

    pub fn load_sessions(&self) -> CoreResult<Vec<SessionState>> {
        let schema = self.quoted_schema();
        let rows = self
            .client()?
            .query(
                &format!("SELECT state_json FROM {schema}.sessions ORDER BY session_id ASC"),
                &[],
            )
            .map_err(|error| postgres_error("load PostgreSQL sessions", error))?;
        rows.into_iter()
            .map(|row| {
                let state_json: String = row.get(0);
                parse_postgres_json(&state_json, "session state_json")
            })
            .collect()
    }

    pub fn load_agent_identities(&self) -> CoreResult<Vec<DurableAgentRecord>> {
        self.load_json_records("agent_identities", "record_json", "agent_id")
    }

    pub fn load_agent_instances(
        &self,
    ) -> CoreResult<Vec<rusty_crew_core_protocol::AgentInstanceRecord>> {
        self.load_json_records("agent_instances", "record_json", "instance_id")
    }

    pub fn load_session_identities(&self) -> CoreResult<Vec<SessionIdentityRecord>> {
        self.load_json_records("session_identities", "record_json", "session_id")
    }

    pub fn load_session_configs(&self) -> CoreResult<Vec<SessionConfigRecord>> {
        let schema = self.quoted_schema();
        let rows = self
            .client()?
            .query(
                &format!(
                    "SELECT session_id, profile_id, kind, record_json, created_at
                     FROM {schema}.session_configs
                     ORDER BY session_id ASC"
                ),
                &[],
            )
            .map_err(|error| postgres_error("load PostgreSQL session configs", error))?;
        rows.into_iter()
            .map(|row| {
                let session_id: String = row.get(0);
                let profile_id: String = row.get(1);
                let config_json: String = row.get(3);
                let config: SessionConfig =
                    parse_postgres_json(&config_json, "session config_json")?;
                Ok(SessionConfigRecord {
                    session_id: SessionId(session_id),
                    profile_id: ProfileId(profile_id),
                    kind: config.kind.clone(),
                    resource_limits: config.resource_limits.clone(),
                    tool_profile: config.tool_profile.clone(),
                    config,
                    created_at: row.get(4),
                })
            })
            .collect()
    }

    pub fn create_profile_registry_record(
        &self,
        write: &ProfileRegistryWrite,
    ) -> CoreResult<ProfileRegistryRecord> {
        crate::validate_profile_registry_write(write)?;
        let schema = self.quoted_schema();
        let record = ProfileRegistryRecord {
            profile_id: write.profile_id.clone(),
            lifecycle_status: write.lifecycle_status,
            display_name: write.display_name.clone(),
            summary: write.summary.clone(),
            default_session_kind: write.default_session_kind.clone(),
            agent_id: write.agent_id.clone(),
            owner_id: write.owner_id.clone(),
            active_runtime_settings_json: write.active_runtime_settings_json.clone(),
            source_asset_refs: write.source_asset_refs.clone(),
            derived_runtime_refs: write.derived_runtime_refs.clone(),
            import_export: write.import_export.clone(),
            revision: 1,
            created_at: write.now.clone(),
            updated_at: write.now.clone(),
        };
        let record_json = to_json_text(&record)?;
        let lifecycle_status =
            profile_registry_lifecycle_status_as_str(record.lifecycle_status).to_string();
        self.client()?
            .execute(
                &format!(
                    "INSERT INTO {schema}.profile_registry (
                        profile_id,
                        lifecycle_status,
                        record_json,
                        created_at,
                        updated_at
                     ) VALUES ($1, $2, $3, $4, $5)"
                ),
                &[
                    &record.profile_id.0,
                    &lifecycle_status,
                    &record_json,
                    &record.created_at,
                    &record.updated_at,
                ],
            )
            .map_err(|error| postgres_error("create PostgreSQL profile registry record", error))?;
        Ok(record)
    }

    pub fn update_profile_registry_record(
        &self,
        update: &ProfileRegistryUpdate,
    ) -> CoreResult<ProfileRegistryRecord> {
        crate::validate_profile_registry_write(&update.write)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start PostgreSQL profile registry record update", error)
        })?;
        let existing = tx
            .query_opt(
                &format!(
                    "SELECT record_json
                     FROM {schema}.profile_registry
                     WHERE profile_id = $1"
                ),
                &[&update.write.profile_id.0],
            )
            .map_err(|error| postgres_error("load PostgreSQL profile registry record", error))?
            .map(|row| {
                let record_json: String = row.get(0);
                parse_postgres_json::<ProfileRegistryRecord>(
                    &record_json,
                    "profile registry record_json",
                )
            })
            .transpose()?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    format!(
                        "profile registry record {} not found",
                        update.write.profile_id
                    ),
                )
            })?;
        if existing.revision != update.expected_revision {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "profile registry record {} revision mismatch: expected {}, found {}",
                    update.write.profile_id, update.expected_revision, existing.revision
                ),
            ));
        }
        let record = ProfileRegistryRecord {
            profile_id: update.write.profile_id.clone(),
            lifecycle_status: update.write.lifecycle_status,
            display_name: update.write.display_name.clone(),
            summary: update.write.summary.clone(),
            default_session_kind: update.write.default_session_kind.clone(),
            agent_id: update.write.agent_id.clone(),
            owner_id: update.write.owner_id.clone(),
            active_runtime_settings_json: update.write.active_runtime_settings_json.clone(),
            source_asset_refs: update.write.source_asset_refs.clone(),
            derived_runtime_refs: update.write.derived_runtime_refs.clone(),
            import_export: update.write.import_export.clone(),
            revision: existing.revision + 1,
            created_at: existing.created_at,
            updated_at: update.write.now.clone(),
        };
        let record_json = to_json_text(&record)?;
        let lifecycle_status =
            profile_registry_lifecycle_status_as_str(record.lifecycle_status).to_string();
        tx.execute(
            &format!(
                "UPDATE {schema}.profile_registry
                 SET lifecycle_status = $2,
                     record_json = $3,
                     updated_at = $4
                 WHERE profile_id = $1"
            ),
            &[
                &record.profile_id.0,
                &lifecycle_status,
                &record_json,
                &record.updated_at,
            ],
        )
        .map_err(|error| postgres_error("update PostgreSQL profile registry record", error))?;
        tx.commit().map_err(|error| {
            postgres_error("commit PostgreSQL profile registry record update", error)
        })?;
        Ok(record)
    }

    pub fn get_profile_registry_record(
        &self,
        profile_id: &ProfileId,
    ) -> CoreResult<Option<ProfileRegistryRecord>> {
        crate::validate_profile_registry_id(profile_id)?;
        let schema = self.quoted_schema();
        let row = self
            .client()?
            .query_opt(
                &format!(
                    "SELECT record_json
                     FROM {schema}.profile_registry
                     WHERE profile_id = $1"
                ),
                &[&profile_id.0],
            )
            .map_err(|error| postgres_error("get PostgreSQL profile registry record", error))?;
        row.map(|row| {
            let record_json: String = row.get(0);
            parse_postgres_json(&record_json, "profile registry record_json")
        })
        .transpose()
    }

    pub fn list_profile_registry_records(
        &self,
        query: &ProfileRegistryQuery,
    ) -> CoreResult<Vec<ProfileRegistryRecord>> {
        let schema = self.quoted_schema();
        let lifecycle_status = query
            .lifecycle_status
            .map(profile_registry_lifecycle_status_as_str)
            .map(str::to_string);
        let (limit, offset) = query
            .page
            .unwrap_or(QueryPage {
                limit: None,
                offset: None,
            })
            .bounded(100, 1_000);
        let rows = self
            .client()?
            .query(
                &format!(
                    "SELECT record_json
                     FROM {schema}.profile_registry
                     WHERE ($1::TEXT IS NULL OR lifecycle_status = $1)
                     ORDER BY profile_id ASC
                     LIMIT $2 OFFSET $3"
                ),
                &[&lifecycle_status, &limit, &offset],
            )
            .map_err(|error| postgres_error("list PostgreSQL profile registry records", error))?;
        rows.iter()
            .map(|row| {
                let record_json: String = row.get(0);
                parse_postgres_json(&record_json, "profile registry record_json")
            })
            .collect()
    }

    pub fn upsert_model_provider(
        &self,
        write: &ModelProviderWrite,
    ) -> CoreResult<ModelProviderRecord> {
        crate::validate_model_provider_write(write)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start PostgreSQL model provider upsert", error))?;
        let existing = get_model_provider_in_tx(&mut tx, &schema, &write.alias)?;
        if let (Some(expected), Some(record)) = (write.expected_revision, existing.as_ref()) {
            if record.revision != expected {
                return Err(CoreError::new(
                    CoreErrorKind::ActionRejected,
                    format!(
                        "model provider {} revision mismatch: expected {}, found {}",
                        write.alias, expected, record.revision
                    ),
                ));
            }
        }
        upsert_model_provider_in_tx(&mut tx, &schema, write, existing.as_ref())?;
        let record =
            get_model_provider_in_tx(&mut tx, &schema, &write.alias)?.ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "upserted PostgreSQL model provider was not readable",
                )
            })?;
        tx.commit()
            .map_err(|error| postgres_error("commit PostgreSQL model provider upsert", error))?;
        Ok(record)
    }

    pub fn get_model_provider(&self, alias: &str) -> CoreResult<Option<ModelProviderRecord>> {
        crate::validate_model_provider_alias(alias)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        get_model_provider_in_client(&mut *client, &schema, alias)
    }

    pub fn get_model_provider_secret(&self, alias: &str) -> CoreResult<Option<String>> {
        crate::validate_model_provider_alias(alias)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        get_model_provider_secret_in_client(&mut *client, &schema, alias)
    }

    pub fn list_model_providers(
        &self,
        query: &ModelProviderQuery,
    ) -> CoreResult<Vec<ModelProviderRecord>> {
        let schema = self.quoted_schema();
        let status = query
            .status
            .map(model_provider_status_as_str)
            .map(str::to_string);
        let alias_prefix = query
            .alias_prefix
            .as_deref()
            .map(|value| format!("{value}%"));
        let limit = query.limit.unwrap_or(100).clamp(1, 1_000) as i64;
        let offset = query.offset.unwrap_or(0) as i64;
        let rows = self
            .client()?
            .query(
                &format!(
                    "SELECT provider_json, secret_ciphertext, secret_updated_at
                     FROM {schema}.model_providers
                     WHERE ($1::TEXT IS NULL OR status = $1)
                       AND ($2::TEXT IS NULL OR alias LIKE $2)
                     ORDER BY updated_at DESC, alias ASC
                     LIMIT $3 OFFSET $4"
                ),
                &[&status, &alias_prefix, &limit, &offset],
            )
            .map_err(|error| postgres_error("list PostgreSQL model providers", error))?;
        rows.iter().map(row_to_model_provider).collect()
    }

    pub fn save_channel_binding(&self, record: &ChannelBindingRecord) -> CoreResult<()> {
        let schema = self.quoted_schema();
        let status = external_binding_status_as_str(record.status).to_string();
        let provenance_json = to_json_text(&record.provenance)?;
        self.client()?
            .execute(
                &format!(
                    "INSERT INTO {schema}.channel_bindings (
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
                     ) VALUES (
                        $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                        $11, $12, $13, $14, $15, $16, $17, $18, $19
                     )
                     ON CONFLICT(binding_id) DO UPDATE SET
                        adapter_id = EXCLUDED.adapter_id,
                        provider = EXCLUDED.provider,
                        agent_id = EXCLUDED.agent_id,
                        instance_id = EXCLUDED.instance_id,
                        session_id = EXCLUDED.session_id,
                        profile_id = EXCLUDED.profile_id,
                        external_channel_id = EXCLUDED.external_channel_id,
                        external_thread_id = EXCLUDED.external_thread_id,
                        external_user_id = EXCLUDED.external_user_id,
                        provider_subscription_id = EXCLUDED.provider_subscription_id,
                        cursor = EXCLUDED.cursor,
                        membership_state = EXCLUDED.membership_state,
                        presence_state = EXCLUDED.presence_state,
                        status = EXCLUDED.status,
                        degraded_reason = EXCLUDED.degraded_reason,
                        provenance_json = EXCLUDED.provenance_json,
                        updated_at = EXCLUDED.updated_at"
                ),
                &[
                    &record.binding_id,
                    &record.adapter_id.0,
                    &record.provider,
                    &record.agent_id.0,
                    &record.instance_id.as_ref().map(|value| value.0.as_str()),
                    &record.session_id.as_ref().map(|value| value.0.as_str()),
                    &record.profile_id.0,
                    &record.external_channel_id,
                    &record.external_thread_id,
                    &record.external_user_id,
                    &record.provider_subscription_id,
                    &record.cursor,
                    &record.membership_state,
                    &record.presence_state,
                    &status,
                    &record.degraded_reason,
                    &provenance_json,
                    &record.created_at,
                    &record.updated_at,
                ],
            )
            .map_err(|error| postgres_error("save PostgreSQL channel binding", error))?;
        Ok(())
    }

    pub fn query_channel_bindings(
        &self,
        query: &ChannelBindingQuery,
    ) -> CoreResult<Vec<ChannelBindingRecord>> {
        let schema = self.quoted_schema();
        let agent_id = query.agent_id.as_ref().map(|value| value.0.as_str());
        let instance_id = query.instance_id.as_ref().map(|value| value.0.as_str());
        let session_id = query.session_id.as_ref().map(|value| value.0.as_str());
        let profile_id = query.profile_id.as_ref().map(|value| value.0.as_str());
        let adapter_id = query.adapter_id.as_ref().map(|value| value.0.as_str());
        let provider = query.provider.as_deref();
        let external_channel_id = query.external_channel_id.as_deref();
        let status = query.status.map(external_binding_status_as_str);
        let (limit, offset) = query
            .page
            .unwrap_or(QueryPage {
                limit: None,
                offset: None,
            })
            .bounded(100, 1_000);
        let rows = self
            .client()?
            .query(
                &format!(
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
                     FROM {schema}.channel_bindings
                     WHERE ($1::TEXT IS NULL OR agent_id = $1)
                       AND ($2::TEXT IS NULL OR instance_id = $2)
                       AND ($3::TEXT IS NULL OR session_id = $3)
                       AND ($4::TEXT IS NULL OR profile_id = $4)
                       AND ($5::TEXT IS NULL OR adapter_id = $5)
                       AND ($6::TEXT IS NULL OR provider = $6)
                       AND ($7::TEXT IS NULL OR external_channel_id = $7)
                       AND ($8::TEXT IS NULL OR status = $8)
                     ORDER BY provider ASC, external_channel_id ASC, binding_id ASC
                     LIMIT $9 OFFSET $10"
                ),
                &[
                    &agent_id,
                    &instance_id,
                    &session_id,
                    &profile_id,
                    &adapter_id,
                    &provider,
                    &external_channel_id,
                    &status,
                    &limit,
                    &offset,
                ],
            )
            .map_err(|error| postgres_error("query PostgreSQL channel bindings", error))?;
        rows.iter().map(row_to_channel_binding_record).collect()
    }

    pub fn save_mcp_binding(&self, record: &McpBindingRecord) -> CoreResult<()> {
        let schema = self.quoted_schema();
        let server_names_json = to_json_text(&record.server_names)?;
        let diagnostics_json = to_json_text(&record.diagnostics)?;
        let status = external_binding_status_as_str(record.status).to_string();
        self.client()?
            .execute(
                &format!(
                    "INSERT INTO {schema}.mcp_bindings (
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
                     ) VALUES (
                        $1, $2, $3, $4, $5, $6, $7, $8,
                        $9, $10, $11, $12, $13, $14, $15, $16
                     )
                     ON CONFLICT(binding_id) DO UPDATE SET
                        adapter_id = EXCLUDED.adapter_id,
                        agent_id = EXCLUDED.agent_id,
                        instance_id = EXCLUDED.instance_id,
                        session_id = EXCLUDED.session_id,
                        profile_id = EXCLUDED.profile_id,
                        server_names_json = EXCLUDED.server_names_json,
                        endpoint_ref = EXCLUDED.endpoint_ref,
                        transport = EXCLUDED.transport,
                        tool_profile_key = EXCLUDED.tool_profile_key,
                        discovered_tool_revision = EXCLUDED.discovered_tool_revision,
                        status = EXCLUDED.status,
                        degraded_reason = EXCLUDED.degraded_reason,
                        diagnostics_json = EXCLUDED.diagnostics_json,
                        updated_at = EXCLUDED.updated_at"
                ),
                &[
                    &record.binding_id,
                    &record.adapter_id.0,
                    &record.agent_id.0,
                    &record.instance_id.as_ref().map(|value| value.0.as_str()),
                    &record.session_id.as_ref().map(|value| value.0.as_str()),
                    &record.profile_id.0,
                    &server_names_json,
                    &record.endpoint_ref,
                    &record.transport,
                    &record.tool_profile_key,
                    &record.discovered_tool_revision,
                    &status,
                    &record.degraded_reason,
                    &diagnostics_json,
                    &record.created_at,
                    &record.updated_at,
                ],
            )
            .map_err(|error| postgres_error("save PostgreSQL MCP binding", error))?;
        Ok(())
    }

    pub fn query_mcp_bindings(&self, query: &McpBindingQuery) -> CoreResult<Vec<McpBindingRecord>> {
        let schema = self.quoted_schema();
        let agent_id = query.agent_id.as_ref().map(|value| value.0.as_str());
        let instance_id = query.instance_id.as_ref().map(|value| value.0.as_str());
        let session_id = query.session_id.as_ref().map(|value| value.0.as_str());
        let profile_id = query.profile_id.as_ref().map(|value| value.0.as_str());
        let adapter_id = query.adapter_id.as_ref().map(|value| value.0.as_str());
        let status = query.status.map(external_binding_status_as_str);
        let (limit, offset) = query
            .page
            .unwrap_or(QueryPage {
                limit: None,
                offset: None,
            })
            .bounded(100, 1_000);
        let rows = self
            .client()?
            .query(
                &format!(
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
                     FROM {schema}.mcp_bindings
                     WHERE ($1::TEXT IS NULL OR agent_id = $1)
                       AND ($2::TEXT IS NULL OR instance_id = $2)
                       AND ($3::TEXT IS NULL OR session_id = $3)
                       AND ($4::TEXT IS NULL OR profile_id = $4)
                       AND ($5::TEXT IS NULL OR adapter_id = $5)
                       AND ($6::TEXT IS NULL OR status = $6)
                     ORDER BY agent_id ASC, profile_id ASC, binding_id ASC
                     LIMIT $7 OFFSET $8"
                ),
                &[
                    &agent_id,
                    &instance_id,
                    &session_id,
                    &profile_id,
                    &adapter_id,
                    &status,
                    &limit,
                    &offset,
                ],
            )
            .map_err(|error| postgres_error("query PostgreSQL MCP bindings", error))?;
        rows.iter().map(row_to_mcp_binding_record).collect()
    }

    pub fn save_event(&self, sequence: u64, event: &CoreEvent) -> CoreResult<()> {
        if !postgres_should_persist_event(event) {
            return Ok(());
        }
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start save PostgreSQL event", error))?;
        let schema = self.quoted_schema();
        let event_kind = format!("{:?}", CoreEventKind::of(event));
        let event_json = to_json_text(event)?;
        let is_new = tx
            .query_one(
                &format!(
                    "SELECT NOT EXISTS(
                        SELECT 1 FROM {schema}.event_history WHERE sequence = $1
                     )"
                ),
                &[&(sequence as i64)],
            )
            .map_err(|error| postgres_error("check PostgreSQL event existence", error))?
            .get::<_, bool>(0);
        tx.execute(
            &format!(
                "INSERT INTO {schema}.event_history (sequence, event_kind, event_json)
                 VALUES ($1, $2, $3)
                 ON CONFLICT(sequence) DO UPDATE SET
                    event_kind = EXCLUDED.event_kind,
                    event_json = EXCLUDED.event_json"
            ),
            &[&(sequence as i64), &event_kind, &event_json],
        )
        .map_err(|error| postgres_error("save PostgreSQL event history", error))?;
        self.replace_event_indexes_in_tx(&mut tx, sequence, event)?;
        if let CoreEvent::CompletionPacketDelivered { packet } = event {
            save_completion_packet_in_tx(&mut tx, &schema, sequence, packet)?;
        }
        if let CoreEvent::BrainEventObserved {
            session_id,
            wake_id,
            event,
        } = event
        {
            save_tool_call_in_tx(
                &mut tx,
                &schema,
                sequence,
                session_id,
                wake_id.as_deref(),
                event,
            )?;
        }
        if is_new {
            for scope in postgres_event_counter_scopes(event) {
                for (counter_name, amount) in postgres_event_counter_deltas(event) {
                    self.increment_counter_in_tx(&mut tx, &scope, counter_name, amount)?;
                }
            }
        }
        tx.commit()
            .map_err(|error| postgres_error("commit save PostgreSQL event", error))?;
        Ok(())
    }

    pub fn load_event_history(&self) -> CoreResult<Vec<PersistedEvent>> {
        let schema = self.quoted_schema();
        let rows = self
            .client()?
            .query(
                &format!(
                    "SELECT sequence, event_json
                     FROM {schema}.event_history
                     ORDER BY sequence ASC"
                ),
                &[],
            )
            .map_err(|error| postgres_error("load PostgreSQL event history", error))?;
        rows.into_iter()
            .map(|row| {
                let sequence: i64 = row.get(0);
                let event_json: String = row.get(1);
                Ok(PersistedEvent {
                    sequence: sequence as u64,
                    event: parse_postgres_json(&event_json, "event event_json")?,
                })
            })
            .collect()
    }

    pub fn query_events(&self, filter: &RuntimeEventFilter) -> CoreResult<Vec<RuntimeEventRecord>> {
        let schema = self.quoted_schema();
        let kind = filter.kind.as_ref().map(|kind| format!("{kind:?}"));
        let session_id = filter.session_id.as_ref().map(|value| value.0.as_str());
        let agent_id = filter.agent_id.as_ref().map(|value| value.0.as_str());
        let instance_id = filter.instance_id.as_ref().map(|value| value.0.as_str());
        let correlation_id = filter.correlation_id.as_deref();
        let source_wake_id = filter.source_wake_id.as_deref();
        let limit = filter.limit.unwrap_or(1_000).max(1) as i64;
        let rows = self
            .client()?
            .query(
                &format!(
                    "SELECT sequence, event_kind, recorded_at, event_json
                     FROM {schema}.event_history
                     WHERE ($1::TEXT IS NULL OR event_kind = $1)
                       AND ($2::TEXT IS NULL OR EXISTS (
                            SELECT 1 FROM {schema}.event_index
                            WHERE event_index.sequence = event_history.sequence
                              AND projection = 'session'
                              AND value = $2
                       ))
                       AND ($3::TEXT IS NULL OR EXISTS (
                            SELECT 1 FROM {schema}.event_index
                            WHERE event_index.sequence = event_history.sequence
                              AND projection = 'agent'
                              AND value = $3
                       ))
                       AND ($4::TEXT IS NULL OR EXISTS (
                            SELECT 1 FROM {schema}.event_index
                            WHERE event_index.sequence = event_history.sequence
                              AND projection = 'instance'
                              AND value = $4
                       ))
                       AND ($5::TEXT IS NULL OR EXISTS (
                            SELECT 1 FROM {schema}.event_index
                            WHERE event_index.sequence = event_history.sequence
                              AND projection = 'correlation'
                              AND value = $5
                       ))
                       AND ($6::TEXT IS NULL OR EXISTS (
                            SELECT 1 FROM {schema}.event_index
                            WHERE event_index.sequence = event_history.sequence
                              AND projection = 'wake'
                              AND value = $6
                       ))
                     ORDER BY sequence ASC
                     LIMIT $7"
                ),
                &[
                    &kind,
                    &session_id,
                    &agent_id,
                    &instance_id,
                    &correlation_id,
                    &source_wake_id,
                    &limit,
                ],
            )
            .map_err(|error| postgres_error("query PostgreSQL events", error))?;
        rows.into_iter()
            .map(|row| self.row_to_event_record(row))
            .collect()
    }

    pub fn save_queued_message(&self, record: &QueuedMessageRecord) -> CoreResult<()> {
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start save PostgreSQL queued message", error))?;
        self.save_queued_message_in_tx(&mut tx, record)?;
        tx.commit()
            .map_err(|error| postgres_error("commit save PostgreSQL queued message", error))?;
        Ok(())
    }

    pub fn expire_queued_messages_at(
        &self,
        now: &IsoTimestamp,
    ) -> CoreResult<Vec<QueuedMessageRecord>> {
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start expire PostgreSQL queued messages", error))?;
        let expired = self.expire_queued_messages_in_tx(&mut tx, now)?;
        tx.commit()
            .map_err(|error| postgres_error("commit expire PostgreSQL queued messages", error))?;
        Ok(expired)
    }

    pub fn load_queued_messages(
        &self,
        filter: &QueuedMessageFilter,
    ) -> CoreResult<Vec<QueuedMessageRecord>> {
        let mut client = self.client()?;
        self.load_queued_messages_in_tx(&mut *client, filter)
    }

    pub fn load_tool_call_history(&self) -> CoreResult<Vec<ToolCallRecord>> {
        let schema = self.quoted_schema();
        let rows = self
            .client()?
            .query(
                &format!(
                    "SELECT sequence,
                            session_id,
                            wake_id,
                            tool_name,
                            phase,
                            is_error,
                            metadata_json
                     FROM {schema}.tool_call_history
                     ORDER BY sequence ASC"
                ),
                &[],
            )
            .map_err(|error| postgres_error("load PostgreSQL tool call history", error))?;
        rows.iter().map(row_to_tool_call_record).collect()
    }

    pub fn upsert_scheduled_job(&self, record: &ScheduledJobRecord) -> CoreResult<()> {
        let schema = self.quoted_schema();
        let payload_json = to_json_text(&record.payload_json)?;
        let status = postgres_scheduled_job_status_as_str(record.status).to_string();
        self.client()?
            .execute(
                &format!(
                    "INSERT INTO {schema}.scheduled_jobs (
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
                     ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                     ON CONFLICT(job_id) DO UPDATE SET
                        job_kind = EXCLUDED.job_kind,
                        target_session_id = EXCLUDED.target_session_id,
                        interval_ms = EXCLUDED.interval_ms,
                        next_due_at = EXCLUDED.next_due_at,
                        payload_json = EXCLUDED.payload_json,
                        status = EXCLUDED.status,
                        updated_at = EXCLUDED.updated_at,
                        paused_at = EXCLUDED.paused_at"
                ),
                &[
                    &record.job_id,
                    &record.job_kind,
                    &record
                        .target_session_id
                        .as_ref()
                        .map(|session_id| session_id.0.as_str()),
                    &record.interval_ms.map(|value| value as i64),
                    &record.next_due_at,
                    &payload_json,
                    &status,
                    &record.created_at,
                    &record.updated_at,
                    &record.paused_at,
                ],
            )
            .map_err(|error| postgres_error("upsert PostgreSQL scheduled job", error))?;
        Ok(())
    }

    pub fn load_scheduled_job(&self, job_id: &str) -> CoreResult<Option<ScheduledJobRecord>> {
        let schema = self.quoted_schema();
        let row = self
            .client()?
            .query_opt(
                &format!(
                    "SELECT job_id,
                            job_kind,
                            target_session_id,
                            interval_ms,
                            next_due_at,
                            payload_json,
                            status,
                            created_at,
                            updated_at,
                            paused_at
                     FROM {schema}.scheduled_jobs
                     WHERE job_id = $1"
                ),
                &[&job_id],
            )
            .map_err(|error| postgres_error("load PostgreSQL scheduled job", error))?;
        row.as_ref().map(row_to_scheduled_job).transpose()
    }

    pub fn query_scheduled_jobs(
        &self,
        query: &ScheduledJobQuery,
    ) -> CoreResult<Vec<ScheduledJobRecord>> {
        let schema = self.quoted_schema();
        let status = query
            .status
            .map(postgres_scheduled_job_status_as_str)
            .map(str::to_string);
        let job_kind = query.job_kind.as_deref();
        let due_at_or_before = query.due_at_or_before.as_deref();
        let (limit, offset) = query
            .page
            .unwrap_or(QueryPage {
                limit: None,
                offset: None,
            })
            .bounded(100, 1_000);
        let rows = self
            .client()?
            .query(
                &format!(
                    "SELECT job_id,
                            job_kind,
                            target_session_id,
                            interval_ms,
                            next_due_at,
                            payload_json,
                            status,
                            created_at,
                            updated_at,
                            paused_at
                     FROM {schema}.scheduled_jobs
                     WHERE ($1::TEXT IS NULL OR status = $1)
                       AND ($2::TEXT IS NULL OR job_kind = $2)
                       AND ($3::TEXT IS NULL OR (next_due_at IS NOT NULL AND next_due_at <= $3))
                     ORDER BY COALESCE(next_due_at, created_at) ASC, job_id ASC
                     LIMIT $4 OFFSET $5"
                ),
                &[&status, &job_kind, &due_at_or_before, &limit, &offset],
            )
            .map_err(|error| postgres_error("query PostgreSQL scheduled jobs", error))?;
        rows.iter().map(row_to_scheduled_job).collect()
    }

    pub fn pause_scheduled_job(&self, job_id: &str, now: &IsoTimestamp) -> CoreResult<()> {
        let schema = self.quoted_schema();
        self.client()?
            .execute(
                &format!(
                    "UPDATE {schema}.scheduled_jobs
                     SET status = 'paused',
                         paused_at = $2,
                         updated_at = $2
                     WHERE job_id = $1
                       AND status <> 'archived'"
                ),
                &[&job_id, now],
            )
            .map_err(|error| postgres_error("pause PostgreSQL scheduled job", error))?;
        Ok(())
    }

    pub fn resume_scheduled_job(
        &self,
        job_id: &str,
        next_due_at: &IsoTimestamp,
        now: &IsoTimestamp,
    ) -> CoreResult<()> {
        let schema = self.quoted_schema();
        self.client()?
            .execute(
                &format!(
                    "UPDATE {schema}.scheduled_jobs
                     SET status = 'active',
                         next_due_at = $2,
                         paused_at = NULL,
                         updated_at = $3
                     WHERE job_id = $1
                       AND status <> 'archived'"
                ),
                &[&job_id, next_due_at, now],
            )
            .map_err(|error| postgres_error("resume PostgreSQL scheduled job", error))?;
        Ok(())
    }

    pub fn claim_scheduled_run(
        &self,
        run: &ScheduledRunRecord,
        next_due_at: Option<&IsoTimestamp>,
    ) -> CoreResult<()> {
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start claim PostgreSQL scheduled run", error))?;
        let schema = self.quoted_schema();
        save_scheduled_run_in_tx(&mut tx, &schema, run)?;
        if run.trigger == ScheduledRunTrigger::Due {
            tx.execute(
                &format!(
                    "UPDATE {schema}.scheduled_jobs
                     SET next_due_at = $2,
                         updated_at = $3
                     WHERE job_id = $1
                       AND status = 'active'"
                ),
                &[&run.job_id, &next_due_at, &run.updated_at],
            )
            .map_err(|error| postgres_error("advance PostgreSQL scheduled job", error))?;
        }
        tx.commit()
            .map_err(|error| postgres_error("commit claim PostgreSQL scheduled run", error))?;
        Ok(())
    }

    pub fn complete_scheduled_run(
        &self,
        run_id: &RunId,
        status: ScheduledRunStatus,
        completed_at: &IsoTimestamp,
        output_json: &serde_json::Value,
        error: Option<&str>,
    ) -> CoreResult<()> {
        let schema = self.quoted_schema();
        let status = postgres_scheduled_run_status_as_str(status).to_string();
        let output_json = to_json_text(output_json)?;
        self.client()?
            .execute(
                &format!(
                    "UPDATE {schema}.scheduled_job_runs
                     SET status = $2,
                         completed_at = $3,
                         updated_at = $3,
                         output_json = $4,
                         error = $5
                     WHERE run_id = $1"
                ),
                &[&run_id.0, &status, completed_at, &output_json, &error],
            )
            .map_err(|error| postgres_error("complete PostgreSQL scheduled run", error))?;
        Ok(())
    }

    pub fn query_scheduled_runs(
        &self,
        query: &ScheduledRunQuery,
    ) -> CoreResult<Vec<ScheduledRunRecord>> {
        let schema = self.quoted_schema();
        let job_id = query.job_id.as_deref();
        let status = query
            .status
            .map(postgres_scheduled_run_status_as_str)
            .map(str::to_string);
        let trigger = query
            .trigger
            .map(postgres_scheduled_run_trigger_as_str)
            .map(str::to_string);
        let target_session_id = query
            .target_session_id
            .as_ref()
            .map(|value| value.0.as_str());
        let stale_before = query.stale_claim_deadline_before.as_deref();
        let (limit, offset) = query
            .page
            .unwrap_or(QueryPage {
                limit: None,
                offset: None,
            })
            .bounded(100, 1_000);
        let rows = self
            .client()?
            .query(
                &format!(
                    "SELECT run_id,
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
                     FROM {schema}.scheduled_job_runs
                     WHERE ($1::TEXT IS NULL OR job_id = $1)
                       AND ($2::TEXT IS NULL OR status = $2)
                       AND ($3::TEXT IS NULL OR trigger_kind = $3)
                       AND ($4::TEXT IS NULL OR target_session_id = $4)
                       AND ($5::TEXT IS NULL OR claim_deadline_at < $5)
                     ORDER BY created_at ASC, run_id ASC
                     LIMIT $6 OFFSET $7"
                ),
                &[
                    &job_id,
                    &status,
                    &trigger,
                    &target_session_id,
                    &stale_before,
                    &limit,
                    &offset,
                ],
            )
            .map_err(|error| postgres_error("query PostgreSQL scheduled runs", error))?;
        rows.iter().map(row_to_scheduled_run).collect()
    }

    pub fn expire_stale_scheduled_runs(
        &self,
        stale_before: &IsoTimestamp,
        now: &IsoTimestamp,
    ) -> CoreResult<Vec<ScheduledRunRecord>> {
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start expire stale PostgreSQL scheduled runs", error)
        })?;
        let schema = self.quoted_schema();
        let rows = tx
            .query(
                &format!(
                    "SELECT run_id,
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
                     FROM {schema}.scheduled_job_runs
                     WHERE status = 'claimed'
                       AND claim_deadline_at < $1
                     ORDER BY created_at ASC, run_id ASC
                     FOR UPDATE SKIP LOCKED"
                ),
                &[stale_before],
            )
            .map_err(|error| postgres_error("query stale PostgreSQL scheduled runs", error))?;
        let stale = rows
            .iter()
            .map(row_to_scheduled_run)
            .collect::<CoreResult<Vec<_>>>()?;
        for run in &stale {
            tx.execute(
                &format!(
                    "UPDATE {schema}.scheduled_job_runs
                     SET status = 'expired',
                         completed_at = $2,
                         updated_at = $2,
                         error = 'claim deadline elapsed'
                     WHERE run_id = $1
                       AND status = 'claimed'"
                ),
                &[&run.run_id.0, now],
            )
            .map_err(|error| postgres_error("expire stale PostgreSQL scheduled run", error))?;
        }
        tx.commit().map_err(|error| {
            postgres_error("commit expire stale PostgreSQL scheduled runs", error)
        })?;
        Ok(stale)
    }

    pub fn query_completion_packets(
        &self,
        query: &CompletionPacketQuery,
    ) -> CoreResult<Vec<CompletionPacketRecord>> {
        let schema = self.quoted_schema();
        let session_id = query.session_id.as_ref().map(|value| value.0.as_str());
        let status = query
            .status
            .as_ref()
            .map(postgres_completion_status_as_str)
            .map(str::to_string);
        let (limit, offset) = query
            .page
            .unwrap_or(QueryPage {
                limit: None,
                offset: None,
            })
            .bounded(100, 1_000);
        let rows = self
            .client()?
            .query(
                &format!(
                    "SELECT sequence, packet_json
                     FROM {schema}.completion_packets
                     WHERE ($1::TEXT IS NULL OR session_id = $1)
                       AND ($2::TEXT IS NULL OR status = $2)
                     ORDER BY sequence ASC
                     LIMIT $3 OFFSET $4"
                ),
                &[&session_id, &status, &limit, &offset],
            )
            .map_err(|error| postgres_error("query PostgreSQL completion packets", error))?;
        rows.iter().map(row_to_completion_packet_record).collect()
    }

    pub fn save_worker_run_requested(&self, record: &WorkerRunRecord) -> CoreResult<()> {
        let schema = self.quoted_schema();
        save_worker_run_in_client(&mut *self.client()?, &schema, record)
    }

    pub fn load_worker_run(&self, run_id: &RunId) -> CoreResult<Option<WorkerRunRecord>> {
        let schema = self.quoted_schema();
        let row = self
            .client()?
            .query_opt(
                &format!("{WORKER_RUN_SELECT} FROM {schema}.worker_runs WHERE run_id = $1"),
                &[&run_id.0],
            )
            .map_err(|error| postgres_error("load PostgreSQL worker run", error))?;
        row.as_ref().map(row_to_worker_run).transpose()
    }

    pub fn load_worker_run_by_delegated_session(
        &self,
        delegated_session_id: &SessionId,
    ) -> CoreResult<Option<WorkerRunRecord>> {
        let schema = self.quoted_schema();
        let row = self
            .client()?
            .query_opt(
                &format!(
                    "{WORKER_RUN_SELECT} FROM {schema}.worker_runs WHERE delegated_session_id = $1"
                ),
                &[&delegated_session_id.0],
            )
            .map_err(|error| {
                postgres_error("load PostgreSQL worker run by delegated session", error)
            })?;
        row.as_ref().map(row_to_worker_run).transpose()
    }

    pub fn query_worker_runs(&self, query: &WorkerRunQuery) -> CoreResult<Vec<WorkerRunRecord>> {
        let schema = self.quoted_schema();
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
        let status = query
            .status
            .as_ref()
            .map(worker_run_status_as_str)
            .map(str::to_string);
        let terminal = query.terminal;
        let (limit, offset) = query
            .page
            .unwrap_or(QueryPage {
                limit: None,
                offset: None,
            })
            .bounded(100, 1_000);
        let rows = self
            .client()?
            .query(
                &format!(
                    "{WORKER_RUN_SELECT}
                     FROM {schema}.worker_runs
                     WHERE ($1::TEXT IS NULL OR session_id = $1)
                       AND ($2::TEXT IS NULL OR delegated_session_id = $2)
                       AND ($3::TEXT IS NULL OR profile_id = $3)
                       AND ($4::TEXT IS NULL OR task_id = $4)
                       AND ($5::TEXT IS NULL OR status = $5)
                       AND (
                           $6::BOOLEAN IS NULL
                           OR ($6 AND status IN ('completed', 'failed', 'blocked', 'exhausted', 'cancelled', 'expired'))
                           OR (NOT $6 AND status NOT IN ('completed', 'failed', 'blocked', 'exhausted', 'cancelled', 'expired'))
                       )
                     ORDER BY created_at ASC, run_id ASC
                     LIMIT $7 OFFSET $8"
                ),
                &[
                    &parent_session_id,
                    &delegated_session_id,
                    &profile_id,
                    &task_id,
                    &status,
                    &terminal,
                    &limit,
                    &offset,
                ],
            )
            .map_err(|error| postgres_error("query PostgreSQL worker runs", error))?;
        rows.iter().map(row_to_worker_run).collect()
    }

    pub fn update_worker_run_status_by_delegated_session(
        &self,
        delegated_session_id: &SessionId,
        status: WorkerRunStatus,
        now: IsoTimestamp,
    ) -> CoreResult<()> {
        let schema = self.quoted_schema();
        let status = worker_run_status_as_str(&status).to_string();
        self.client()?
            .execute(
                &format!(
                    "UPDATE {schema}.worker_runs
                     SET status = $1,
                         last_updated_at = $2
                     WHERE delegated_session_id = $3"
                ),
                &[&status, &now, &delegated_session_id.0],
            )
            .map_err(|error| postgres_error("update PostgreSQL worker run status", error))?;
        Ok(())
    }

    pub fn update_worker_run_status(
        &self,
        run_id: &RunId,
        status: WorkerRunStatus,
        now: IsoTimestamp,
    ) -> CoreResult<()> {
        let schema = self.quoted_schema();
        let status = worker_run_status_as_str(&status).to_string();
        self.client()?
            .execute(
                &format!(
                    "UPDATE {schema}.worker_runs
                     SET status = $1,
                         last_updated_at = $2
                     WHERE run_id = $3"
                ),
                &[&status, &now, &run_id.0],
            )
            .map_err(|error| {
                postgres_error("update PostgreSQL worker run status by run id", error)
            })?;
        Ok(())
    }

    pub fn delegated_completions_for_parent(
        &self,
        parent_session_id: &SessionId,
    ) -> CoreResult<Vec<DelegatedCompletion>> {
        let schema = self.quoted_schema();
        let rows = self
            .client()?
            .query(
                &format!(
                    "SELECT worker_runs.run_id,
                            worker_runs.delegated_session_id,
                            worker_runs.task_id,
                            worker_runs.source_wake_id,
                            worker_runs.source_action_index,
                            worker_runs.delegation_correlation_id,
                            worker_runs.parent_consumption,
                            completion_packets.packet_json
                     FROM {schema}.worker_runs
                     JOIN {schema}.completion_packets
                       ON completion_packets.session_id = worker_runs.delegated_session_id
                     WHERE worker_runs.session_id = $1
                     ORDER BY completion_packets.sequence ASC"
                ),
                &[&parent_session_id.0],
            )
            .map_err(|error| postgres_error("query PostgreSQL delegated completions", error))?;
        rows.iter().map(row_to_delegated_completion).collect()
    }

    pub fn increment_counter(
        &self,
        scope: &RuntimeCounterScope,
        counter_name: &str,
        amount: u64,
        now: &IsoTimestamp,
    ) -> CoreResult<()> {
        if amount == 0 {
            return Ok(());
        }
        validate_counter_amount(amount)?;
        let (scope_type, scope_id) = runtime_counter_scope_parts(scope);
        let schema = self.quoted_schema();
        self.client()?
            .execute(
                &format!(
                    "INSERT INTO {schema}.runtime_counters (
                    scope_type,
                    scope_id,
                    counter_name,
                    value,
                    updated_at
                ) VALUES ($1, $2, $3, $4, $5)
                ON CONFLICT(scope_type, scope_id, counter_name) DO UPDATE SET
                    value = runtime_counters.value + EXCLUDED.value,
                    updated_at = EXCLUDED.updated_at"
                ),
                &[&scope_type, &scope_id, &counter_name, &(amount as i64), now],
            )
            .map_err(|error| postgres_error("increment PostgreSQL runtime counter", error))?;
        Ok(())
    }

    pub fn runtime_counters(
        &self,
        scope: Option<&RuntimeCounterScope>,
    ) -> CoreResult<Vec<RuntimeCounterRecord>> {
        self.query_runtime_counters(&RuntimeCounterQuery {
            scope: scope.cloned(),
            counter_name: None,
            page: None,
        })
    }

    pub fn query_runtime_counters(
        &self,
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
        let schema = self.quoted_schema();
        let rows = self
            .client()?
            .query(
                &format!(
                    "SELECT scope_type, scope_id, counter_name, value, updated_at
                     FROM {schema}.runtime_counters
                     WHERE ($1::text IS NULL OR scope_type = $1)
                       AND ($2::text IS NULL OR scope_id = $2)
                       AND ($3::text IS NULL OR counter_name = $3)
                     ORDER BY scope_type ASC, scope_id ASC, counter_name ASC
                     LIMIT $4 OFFSET $5"
                ),
                &[&scope_type, &scope_id, &counter_name, &limit, &offset],
            )
            .map_err(|error| postgres_error("query PostgreSQL runtime counters", error))?;
        rows.iter().map(row_to_runtime_counter).collect()
    }

    pub fn reset_runtime_counters(
        &self,
        query: &RuntimeCounterQuery,
        now: IsoTimestamp,
    ) -> CoreResult<u64> {
        let scope_parts = query.scope.as_ref().map(runtime_counter_scope_parts);
        let scope_type = scope_parts.as_ref().map(|(scope_type, _)| *scope_type);
        let scope_id = scope_parts.as_ref().map(|(_, scope_id)| scope_id.as_str());
        let counter_name = query.counter_name.as_deref();
        let schema = self.quoted_schema();
        self.client()?
            .execute(
                &format!(
                    "UPDATE {schema}.runtime_counters
                     SET value = 0, updated_at = $4
                     WHERE ($1::text IS NULL OR scope_type = $1)
                       AND ($2::text IS NULL OR scope_id = $2)
                       AND ($3::text IS NULL OR counter_name = $3)"
                ),
                &[&scope_type, &scope_id, &counter_name, &now],
            )
            .map_err(|error| postgres_error("reset PostgreSQL runtime counters", error))
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

    pub fn database_size(&self) -> CoreResult<RuntimeDatabaseSize> {
        let row = self
            .client()?
            .query_one("SELECT pg_database_size(current_database())::BIGINT", &[])
            .map_err(|error| postgres_error("read PostgreSQL database size", error))?;
        let database_bytes: i64 = row.get(0);
        Ok(RuntimeDatabaseSize {
            database_bytes: database_bytes.max(0) as u64,
            page_count: 0,
            page_size_bytes: 0,
            freelist_pages: 0,
            freelist_bytes: 0,
            wal_bytes: 0,
        })
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
            let mut client = self.client()?;
            let mut tx = client
                .transaction()
                .map_err(|error| postgres_error("start PostgreSQL runtime maintenance", error))?;
            if let Some(now) = &policy.expire_queued_messages_at {
                let expired = self.expire_queued_messages_in_tx(&mut tx, now)?;
                expired_queue_messages = expired.len() as u64;
            }
            if let Some(cutoff) = &policy.purge_terminal_queued_messages_before {
                purged_terminal_queue_messages =
                    purge_terminal_queued_messages_in_tx(&mut tx, &self.quoted_schema(), cutoff)?;
            }
            if let Some(now) = &policy.expire_provider_wire_states_at {
                let schema = self.quoted_schema();
                let expiring = load_expired_current_provider_wire_states(&mut tx, &schema, now)?;
                for record in &expiring {
                    invalidate_provider_wire_state_by_row_in_tx(
                        &mut tx,
                        &schema,
                        record.row_id,
                        now,
                        ProviderWireStateInvalidationReason::Expired,
                    )?;
                }
                expired_provider_wire_states = expiring.len() as u64;
            }
            tx.commit()
                .map_err(|error| postgres_error("commit PostgreSQL runtime maintenance", error))?;
        }
        let size_after = self.database_size()?;
        Ok(RuntimeMaintenanceReport {
            size_before,
            size_after,
            expired_queue_messages,
            purged_terminal_queue_messages,
            expired_provider_wire_states,
            session_memory_compaction: SessionMemoryCompactionReport::default(),
            wal_checkpoint_ran: false,
            optimize_ran: false,
        })
    }

    pub fn get_simple_kv(
        &self,
        scope: &SimpleKvScope,
        key: &str,
        now: Option<&IsoTimestamp>,
    ) -> CoreResult<Option<SimpleKvRecord>> {
        validate_simple_kv_identity(scope, key)?;
        let schema = self.quoted_schema();
        let row = self
            .client()?
            .query_opt(
                &format!(
                    "SELECT scope_type, scope_id, entry_key, value_json, revision,
                            created_at, updated_at, expires_at
                     FROM {schema}.module_simple_kv_entries
                     WHERE scope_type = $1
                       AND scope_id = $2
                       AND entry_key = $3
                       AND (expires_at IS NULL OR $4::text IS NULL OR expires_at > $4)"
                ),
                &[&scope.scope_type, &scope.scope_id, &key, &now],
            )
            .map_err(|error| postgres_error("get PostgreSQL simple_kv entry", error))?;
        row.as_ref().map(row_to_simple_kv).transpose()
    }

    pub fn list_simple_kv(&self, query: &SimpleKvQuery) -> CoreResult<Vec<SimpleKvRecord>> {
        validate_simple_kv_query(query)?;
        let (limit, offset) = query
            .page
            .unwrap_or(QueryPage {
                limit: None,
                offset: None,
            })
            .bounded(100, 1_000);
        let key_prefix = query
            .key_prefix
            .as_ref()
            .map(|prefix| postgres_like_prefix(prefix));
        let schema = self.quoted_schema();
        let rows = self
            .client()?
            .query(
                &format!(
                    "SELECT scope_type, scope_id, entry_key, value_json, revision,
                            created_at, updated_at, expires_at
                     FROM {schema}.module_simple_kv_entries
                     WHERE scope_type = $1
                       AND scope_id = $2
                       AND ($3::text IS NULL OR entry_key LIKE $3 ESCAPE '\\')
                       AND (
                            ($4 AND expires_at IS NOT NULL AND $5::text IS NOT NULL AND expires_at <= $5)
                            OR
                            (NOT $4 AND ($6 OR expires_at IS NULL OR $5::text IS NULL OR expires_at > $5))
                       )
                     ORDER BY entry_key ASC
                     LIMIT $7 OFFSET $8"
                ),
                &[
                    &query.scope.scope_type,
                    &query.scope.scope_id,
                    &key_prefix,
                    &query.expired_only,
                    &query.now,
                    &query.include_expired,
                    &limit,
                    &offset,
                ],
            )
            .map_err(|error| postgres_error("list PostgreSQL simple_kv entries", error))?;
        rows.iter().map(row_to_simple_kv).collect()
    }

    pub fn put_simple_kv(&self, write: &SimpleKvWrite) -> CoreResult<SimpleKvRecord> {
        validate_simple_kv_write(write)?;
        let existing = self.get_simple_kv(&write.scope, &write.key, None)?;
        match existing {
            Some(existing) => self.update_simple_kv(write, existing.revision + 1),
            None => self.insert_simple_kv(write),
        }
    }

    pub fn compare_and_swap_simple_kv(
        &self,
        compare_and_swap: &SimpleKvCompareAndSwap,
    ) -> CoreResult<SimpleKvRecord> {
        validate_simple_kv_write(&compare_and_swap.write)?;
        let existing = self
            .get_simple_kv(
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
        self.update_simple_kv(&compare_and_swap.write, existing.revision + 1)
    }

    pub fn delete_simple_kv(&self, delete: &SimpleKvDelete) -> CoreResult<SimpleKvRecord> {
        validate_simple_kv_identity(&delete.scope, &delete.key)?;
        let existing = self
            .get_simple_kv(&delete.scope, &delete.key, None)?
            .ok_or_else(|| {
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
        let schema = self.quoted_schema();
        self.client()?
            .execute(
                &format!(
                    "DELETE FROM {schema}.module_simple_kv_entries
                     WHERE scope_type = $1 AND scope_id = $2 AND entry_key = $3"
                ),
                &[
                    &delete.scope.scope_type,
                    &delete.scope.scope_id,
                    &delete.key,
                ],
            )
            .map_err(|error| postgres_error("delete PostgreSQL simple_kv entry", error))?;
        Ok(existing)
    }

    pub fn expire_simple_kv(&self, now: &IsoTimestamp) -> CoreResult<u64> {
        let schema = self.quoted_schema();
        let changed = self
            .client()?
            .execute(
                &format!(
                    "DELETE FROM {schema}.module_simple_kv_entries
                     WHERE expires_at IS NOT NULL AND expires_at <= $1"
                ),
                &[now],
            )
            .map_err(|error| postgres_error("expire PostgreSQL simple_kv entries", error))?;
        Ok(changed)
    }

    pub fn storage_diagnostics(&self) -> CoreResult<PostgresRuntimeCounterProofDiagnostics> {
        Ok(PostgresRuntimeCounterProofDiagnostics {
            backend: "postgres".to_string(),
            backend_label: "PostgreSQL runtime-counter proof slice".to_string(),
            schema: self.schema.clone(),
            proof_repository:
                "sessions,events,queued_messages,scheduled_jobs,worker_runs,completion_packets,tool_call_history,runtime_counters,module_simple_kv_entries,runtime_search,provider_wire_states,model_providers,conversations,attachments,data_bank_scopes,profile_memory,roleplay_lore"
                    .to_string(),
            schema_version: self.schema_version()?,
            table_counts: vec![
                RuntimeStorageTableCount {
                    table: "sessions".to_string(),
                    rows: self.table_rows("sessions")?,
                },
                RuntimeStorageTableCount {
                    table: "session_configs".to_string(),
                    rows: self.table_rows("session_configs")?,
                },
                RuntimeStorageTableCount {
                    table: "session_identities".to_string(),
                    rows: self.table_rows("session_identities")?,
                },
                RuntimeStorageTableCount {
                    table: "event_history".to_string(),
                    rows: self.table_rows("event_history")?,
                },
                RuntimeStorageTableCount {
                    table: "event_index".to_string(),
                    rows: self.table_rows("event_index")?,
                },
                RuntimeStorageTableCount {
                    table: "queued_messages".to_string(),
                    rows: self.table_rows("queued_messages")?,
                },
                RuntimeStorageTableCount {
                    table: "scheduled_jobs".to_string(),
                    rows: self.table_rows("scheduled_jobs")?,
                },
                RuntimeStorageTableCount {
                    table: "scheduled_job_runs".to_string(),
                    rows: self.table_rows("scheduled_job_runs")?,
                },
                RuntimeStorageTableCount {
                    table: "worker_runs".to_string(),
                    rows: self.table_rows("worker_runs")?,
                },
                RuntimeStorageTableCount {
                    table: "completion_packets".to_string(),
                    rows: self.table_rows("completion_packets")?,
                },
                RuntimeStorageTableCount {
                    table: "tool_call_history".to_string(),
                    rows: self.table_rows("tool_call_history")?,
                },
                RuntimeStorageTableCount {
                    table: "runtime_counters".to_string(),
                    rows: self.runtime_counter_rows()?,
                },
                RuntimeStorageTableCount {
                    table: "module_simple_kv_entries".to_string(),
                    rows: self.simple_kv_rows()?,
                },
                RuntimeStorageTableCount {
                    table: "runtime_search_entries".to_string(),
                    rows: self.runtime_search_rows()?,
                },
                RuntimeStorageTableCount {
                    table: "provider_wire_states".to_string(),
                    rows: self.provider_wire_state_rows()?,
                },
                RuntimeStorageTableCount {
                    table: "message_slots".to_string(),
                    rows: self.table_rows("message_slots")?,
                },
                RuntimeStorageTableCount {
                    table: "message_variants".to_string(),
                    rows: self.table_rows("message_variants")?,
                },
                RuntimeStorageTableCount {
                    table: "messages".to_string(),
                    rows: self.table_rows("messages")?,
                },
                RuntimeStorageTableCount {
                    table: "conversation_branches".to_string(),
                    rows: self.table_rows("conversation_branches")?,
                },
                RuntimeStorageTableCount {
                    table: "conversation_snapshots".to_string(),
                    rows: self.table_rows("conversation_snapshots")?,
                },
                RuntimeStorageTableCount {
                    table: "attachments".to_string(),
                    rows: self.table_rows("attachments")?,
                },
                RuntimeStorageTableCount {
                    table: "attachment_links".to_string(),
                    rows: self.table_rows("attachment_links")?,
                },
                RuntimeStorageTableCount {
                    table: "data_bank_scopes".to_string(),
                    rows: self.table_rows("data_bank_scopes")?,
                },
                RuntimeStorageTableCount {
                    table: "profile_memories".to_string(),
                    rows: self.table_rows("profile_memories")?,
                },
                RuntimeStorageTableCount {
                    table: "profile_registry".to_string(),
                    rows: self.table_rows("profile_registry")?,
                },
                RuntimeStorageTableCount {
                    table: "model_providers".to_string(),
                    rows: self.table_rows("model_providers")?,
                },
                RuntimeStorageTableCount {
                    table: "channel_bindings".to_string(),
                    rows: self.table_rows("channel_bindings")?,
                },
                RuntimeStorageTableCount {
                    table: "mcp_bindings".to_string(),
                    rows: self.table_rows("mcp_bindings")?,
                },
                RuntimeStorageTableCount {
                    table: "session_memory_records".to_string(),
                    rows: self.table_rows("session_memory_records")?,
                },
                RuntimeStorageTableCount {
                    table: "memory_proposals".to_string(),
                    rows: self.table_rows("memory_proposals")?,
                },
                RuntimeStorageTableCount {
                    table: "memory_governance_decisions".to_string(),
                    rows: self.table_rows("memory_governance_decisions")?,
                },
                RuntimeStorageTableCount {
                    table: "module_roleplay_lore_records".to_string(),
                    rows: self.table_rows("module_roleplay_lore_records")?,
                },
                RuntimeStorageTableCount {
                    table: "module_roleplay_lore_provenance_events".to_string(),
                    rows: self.table_rows("module_roleplay_lore_provenance_events")?,
                },
            ],
            capabilities: postgres_proof_capabilities(),
            repository_groups: postgres_proof_repository_groups(),
        })
    }

    pub fn memory_space_descriptors(&self) -> Vec<MemorySpaceDescriptor> {
        vec![
            profile_dense_memory_space_descriptor(),
            roleplay_lore_memory_space_descriptor(),
        ]
    }

    pub fn list_profile_memory(
        &self,
        query: &ProfileMemoryQuery,
    ) -> CoreResult<Vec<ProfileMemoryRecord>> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        query_profile_memory(&mut *client, &schema, query)
    }

    pub fn get_profile_memory(
        &self,
        profile_id: &ProfileId,
        target: &ProfileMemoryTarget,
        key: &str,
    ) -> CoreResult<Option<ProfileMemoryRecord>> {
        validate_profile_memory_key(key, ProfileMemoryCaps::default().max_key_bytes)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        get_profile_memory(&mut *client, &schema, profile_id, target, key)
    }

    pub fn add_profile_memory(
        &self,
        write: &ProfileMemoryWrite,
        caps: &ProfileMemoryCaps,
    ) -> CoreResult<ProfileMemoryRecord> {
        validate_profile_memory_write(write, caps)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start add PostgreSQL profile memory", error))?;
        let count = count_profile_memory_for_profile(&mut tx, &schema, &write.profile_id)?;
        if count >= u64::from(caps.max_records_per_profile) {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "profile {} already has the maximum {} dense memory records",
                    write.profile_id, caps.max_records_per_profile
                ),
            ));
        }
        if get_profile_memory(
            &mut tx,
            &schema,
            &write.profile_id,
            &write.target,
            &write.key,
        )?
        .is_some()
        {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!(
                    "profile memory {} for profile {} already exists",
                    write.key, write.profile_id
                ),
            ));
        }
        let record = insert_profile_memory_in_tx(&mut tx, &schema, write)?;
        tx.commit()
            .map_err(|error| postgres_error("commit add PostgreSQL profile memory", error))?;
        Ok(record)
    }

    pub fn replace_profile_memory(
        &self,
        replace: &ProfileMemoryReplace,
        caps: &ProfileMemoryCaps,
    ) -> CoreResult<ProfileMemoryRecord> {
        validate_profile_memory_write(&replace.write, caps)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start replace PostgreSQL profile memory", error))?;
        let existing = get_profile_memory(
            &mut tx,
            &schema,
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
        let record = update_profile_memory_in_tx(
            &mut tx,
            &schema,
            &replace.write,
            existing.revision + 1,
            &existing.created_at,
        )?;
        tx.commit()
            .map_err(|error| postgres_error("commit replace PostgreSQL profile memory", error))?;
        Ok(record)
    }

    pub fn remove_profile_memory(
        &self,
        delete: &ProfileMemoryDelete,
    ) -> CoreResult<ProfileMemoryRecord> {
        validate_profile_memory_key(&delete.key, ProfileMemoryCaps::default().max_key_bytes)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start remove PostgreSQL profile memory", error))?;
        let existing = get_profile_memory(
            &mut tx,
            &schema,
            &delete.profile_id,
            &delete.target,
            &delete.key,
        )?
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
            &format!(
                "DELETE FROM {schema}.profile_memories
                 WHERE profile_id = $1
                   AND target_type = $2
                   AND target_id = $3
                   AND memory_key = $4"
            ),
            &[&delete.profile_id.0, &target_type, &target_id, &delete.key],
        )
        .map_err(|error| postgres_error("remove PostgreSQL profile memory", error))?;
        tx.commit()
            .map_err(|error| postgres_error("commit remove PostgreSQL profile memory", error))?;
        Ok(existing)
    }

    pub fn add_session_memory_record(
        &self,
        write: &SessionMemoryRecordWrite,
    ) -> CoreResult<SessionMemoryRecord> {
        crate::validate_session_memory_write(write)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start add PostgreSQL session memory record", error))?;
        validate_postgres_session_memory_scope(
            &mut tx,
            &schema,
            &write.session_id,
            &write.scope,
            &write.branch_id,
        )?;
        if get_session_memory_record_in_tx(&mut tx, &schema, &write.record_id)?.is_some() {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!("session memory record {} already exists", write.record_id),
            ));
        }
        insert_session_memory_record_in_tx(&mut tx, &schema, write)?;
        let record = get_session_memory_record_in_tx(&mut tx, &schema, &write.record_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "created PostgreSQL session memory record was not readable",
                )
            })?;
        tx.commit().map_err(|error| {
            postgres_error("commit add PostgreSQL session memory record", error)
        })?;
        Ok(record)
    }

    pub fn replace_session_memory_record(
        &self,
        replace: &SessionMemoryReplace,
    ) -> CoreResult<SessionMemoryRecord> {
        validate_postgres_session_memory_revision_input(
            &replace.record_id,
            replace.expected_revision,
            &replace.evidence_refs,
            replace.confidence,
            &replace.durability_rationale,
        )?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start replace PostgreSQL session memory record", error)
        })?;
        let existing = active_session_memory_record_for_update(
            &mut tx,
            &schema,
            &replace.record_id,
            replace.expected_revision,
        )?;
        crate::validate_session_memory_content(&existing.shape, &replace.content)?;
        update_session_memory_record_content_in_tx(
            &mut tx,
            &schema,
            replace,
            existing.revision + 1,
        )?;
        let record = get_session_memory_record_in_tx(&mut tx, &schema, &replace.record_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "replaced PostgreSQL session memory record was not readable",
                )
            })?;
        tx.commit().map_err(|error| {
            postgres_error("commit replace PostgreSQL session memory record", error)
        })?;
        Ok(record)
    }

    pub fn supersede_session_memory_record(
        &self,
        supersede: &SessionMemorySupersede,
    ) -> CoreResult<(SessionMemoryRecord, SessionMemoryRecord)> {
        crate::validate_session_memory_write(&supersede.replacement)?;
        if supersede.replacement.supersedes_record_id.as_deref()
            != Some(supersede.record_id.as_str())
        {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "session memory replacement must reference the superseded record",
            ));
        }
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start supersede PostgreSQL session memory record", error)
        })?;
        let existing = active_session_memory_record_for_update(
            &mut tx,
            &schema,
            &supersede.record_id,
            supersede.expected_revision,
        )?;
        validate_postgres_session_memory_scope(
            &mut tx,
            &schema,
            &supersede.replacement.session_id,
            &supersede.replacement.scope,
            &supersede.replacement.branch_id,
        )?;
        if existing.session_id != supersede.replacement.session_id {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "session memory replacement must stay in the same session",
            ));
        }
        if get_session_memory_record_in_tx(&mut tx, &schema, &supersede.replacement.record_id)?
            .is_some()
        {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!(
                    "session memory replacement {} already exists",
                    supersede.replacement.record_id
                ),
            ));
        }
        insert_session_memory_record_in_tx(&mut tx, &schema, &supersede.replacement)?;
        mark_session_memory_superseded_in_tx(
            &mut tx,
            &schema,
            &existing.record_id,
            &supersede.replacement.record_id,
            existing.revision + 1,
            &supersede.replacement.now,
        )?;
        let old_record = get_session_memory_record_in_tx(&mut tx, &schema, &existing.record_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "superseded PostgreSQL session memory record was not readable",
                )
            })?;
        let new_record =
            get_session_memory_record_in_tx(&mut tx, &schema, &supersede.replacement.record_id)?
                .ok_or_else(|| {
                    CoreError::new(
                        CoreErrorKind::PersistenceFailure,
                        "replacement PostgreSQL session memory record was not readable",
                    )
                })?;
        tx.commit().map_err(|error| {
            postgres_error("commit supersede PostgreSQL session memory record", error)
        })?;
        Ok((old_record, new_record))
    }

    pub fn archive_session_memory_record(
        &self,
        archive: &SessionMemoryArchive,
    ) -> CoreResult<SessionMemoryRecord> {
        crate::validate_session_memory_record_id(&archive.record_id)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start archive PostgreSQL session memory record", error)
        })?;
        let existing = active_session_memory_record_for_update(
            &mut tx,
            &schema,
            &archive.record_id,
            archive.expected_revision,
        )?;
        archive_session_memory_record_in_tx(&mut tx, &schema, archive, existing.revision + 1)?;
        let record = get_session_memory_record_in_tx(&mut tx, &schema, &archive.record_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "archived PostgreSQL session memory record was not readable",
                )
            })?;
        tx.commit().map_err(|error| {
            postgres_error("commit archive PostgreSQL session memory record", error)
        })?;
        Ok(record)
    }

    pub fn query_session_memory_records(
        &self,
        query: &SessionMemoryQuery,
    ) -> CoreResult<Vec<SessionMemoryRecord>> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        query_session_memory_records(&mut *client, &schema, query)
    }

    pub fn build_session_memory_prompt_context(
        &self,
        query: &BranchAwareSessionMemoryQuery,
    ) -> CoreResult<SessionMemoryPromptContext> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        select_branch_aware_session_memory(&mut *client, &schema, query)
    }

    pub fn save_memory_proposal(
        &self,
        proposal: &MemoryProposalEnvelope,
        descriptor: &MemorySpaceDescriptor,
        now: &IsoTimestamp,
    ) -> CoreResult<MemoryProposalRecord> {
        crate::validate_memory_proposal(proposal, descriptor)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start save PostgreSQL memory proposal", error))?;
        if let Some(dedupe_key) = proposal
            .dedupe_key
            .as_ref()
            .filter(|value| !value.trim().is_empty())
        {
            if let Some(existing) =
                get_memory_proposal_by_dedupe(&mut tx, &schema, &proposal.space_id.0, dedupe_key)?
            {
                tx.commit().map_err(|error| {
                    postgres_error("commit PostgreSQL duplicate memory proposal", error)
                })?;
                return Ok(existing);
            }
        }
        if get_memory_proposal_by_id(&mut tx, &schema, &proposal.proposal_id)?.is_some() {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!("memory proposal {} already exists", proposal.proposal_id),
            ));
        }
        let record = MemoryProposalRecord {
            proposal: proposal.clone(),
            status: MemoryProposalReviewStatus::PendingReview,
            selected_governance_mode: selected_governance_mode(proposal.governance_mode),
            created_at: now.clone(),
            updated_at: now.clone(),
            decided_at: None,
            applied_at: None,
            resulting_revision: None,
            duplicate_of: None,
        };
        insert_memory_proposal_record_in_tx(&mut tx, &schema, &record)?;
        insert_memory_governance_decision_in_tx(
            &mut tx,
            &schema,
            &MemoryGovernanceDecisionRecord {
                decision_id: format!("{}_routed", proposal.proposal_id),
                proposal_id: proposal.proposal_id.clone(),
                decision: MemoryGovernanceDecisionKind::RoutedToReview,
                actor: "rusty_crew_governance".to_string(),
                source: proposal.source,
                evidence_refs: proposal.evidence_refs.clone(),
                policy_mode: record.selected_governance_mode,
                confidence: Some(proposal.confidence),
                message: Some("typed memory proposals start in curator/manual review".to_string()),
                resulting_revision: None,
                decided_at: now.clone(),
            },
        )?;
        let saved = get_memory_proposal_by_id(&mut tx, &schema, &proposal.proposal_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "saved PostgreSQL memory proposal was not readable",
                )
            })?;
        tx.commit()
            .map_err(|error| postgres_error("commit save PostgreSQL memory proposal", error))?;
        Ok(saved)
    }

    pub fn list_memory_proposals(
        &self,
        query: &MemoryProposalQuery,
    ) -> CoreResult<Vec<MemoryProposalRecord>> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        list_memory_proposals(&mut *client, &schema, query)
    }

    pub fn record_memory_governance_decision(
        &self,
        decision: &MemoryGovernanceDecisionInput,
        now: &IsoTimestamp,
    ) -> CoreResult<MemoryGovernanceDecisionRecord> {
        validate_postgres_memory_governance_decision(decision)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start PostgreSQL memory governance decision", error)
        })?;
        let mut proposal = get_memory_proposal_by_id(&mut tx, &schema, &decision.proposal_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    format!("memory proposal {} not found", decision.proposal_id),
                )
            })?;
        validate_postgres_memory_governance_transition(proposal.status, decision.decision)?;
        let resulting_revision = if decision.decision == MemoryGovernanceDecisionKind::Applied
            && proposal.proposal.space_id.as_str() == "session_memory"
        {
            Some(apply_session_memory_proposal_in_tx(
                &mut tx,
                &schema,
                &proposal.proposal,
                now,
            )?)
        } else {
            decision.resulting_revision
        };
        let decided_at = decision.decided_at.clone().unwrap_or_else(|| now.clone());
        let record = MemoryGovernanceDecisionRecord {
            decision_id: decision.decision_id.clone(),
            proposal_id: decision.proposal_id.clone(),
            decision: decision.decision,
            actor: decision.actor.clone(),
            source: decision.source,
            evidence_refs: decision.evidence_refs.clone(),
            policy_mode: decision.policy_mode,
            confidence: decision.confidence,
            message: decision.message.clone(),
            resulting_revision,
            decided_at,
        };
        insert_memory_governance_decision_in_tx(&mut tx, &schema, &record)?;
        update_memory_proposal_review_state(&mut proposal, &record);
        update_memory_proposal_record_in_tx(&mut tx, &schema, &proposal)?;
        tx.commit().map_err(|error| {
            postgres_error("commit PostgreSQL memory governance decision", error)
        })?;
        Ok(record)
    }

    pub fn add_roleplay_lore_record(
        &self,
        write: &RoleplayLoreWrite,
    ) -> CoreResult<RoleplayLoreRecord> {
        validate_roleplay_lore_write(write)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start add PostgreSQL roleplay lore", error))?;
        if get_roleplay_lore_record(&mut tx, &schema, &write.record_id)?.is_some() {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!("roleplay lore record {} already exists", write.record_id),
            ));
        }
        insert_roleplay_lore_record(&mut tx, &schema, write)?;
        insert_roleplay_lore_provenance_event(
            &mut tx,
            &schema,
            &RoleplayLoreProvenanceEvent {
                event_id: format!("{}:created", write.record_id),
                record_id: write.record_id.clone(),
                world_id: write.world_id.clone(),
                evidence_refs: write.evidence_refs.clone(),
                source: write.source,
                actor: crate::memory_proposal_source_as_str(write.source).to_string(),
                note: Some("created roleplay lore record".to_string()),
                created_at: write.now.clone(),
            },
        )?;
        let record =
            get_roleplay_lore_record(&mut tx, &schema, &write.record_id)?.ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "created PostgreSQL roleplay lore record was not readable",
                )
            })?;
        tx.commit()
            .map_err(|error| postgres_error("commit add PostgreSQL roleplay lore", error))?;
        Ok(record)
    }

    pub fn replace_roleplay_lore_record(
        &self,
        replace: &RoleplayLoreReplace,
    ) -> CoreResult<RoleplayLoreRecord> {
        validate_roleplay_lore_write(&replace.write)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start replace PostgreSQL roleplay lore", error))?;
        let existing = active_roleplay_lore_record_for_update(
            &mut tx,
            &schema,
            &replace.write.record_id,
            replace.expected_revision,
        )?;
        update_roleplay_lore_record(&mut tx, &schema, replace, existing.revision + 1)?;
        insert_roleplay_lore_provenance_event(
            &mut tx,
            &schema,
            &RoleplayLoreProvenanceEvent {
                event_id: format!(
                    "{}:revision:{}",
                    replace.write.record_id,
                    existing.revision + 1
                ),
                record_id: replace.write.record_id.clone(),
                world_id: replace.write.world_id.clone(),
                evidence_refs: replace.write.evidence_refs.clone(),
                source: replace.write.source,
                actor: crate::memory_proposal_source_as_str(replace.write.source).to_string(),
                note: Some("replaced roleplay lore record".to_string()),
                created_at: replace.write.now.clone(),
            },
        )?;
        let record = get_roleplay_lore_record(&mut tx, &schema, &replace.write.record_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "replaced PostgreSQL roleplay lore record was not readable",
                )
            })?;
        tx.commit()
            .map_err(|error| postgres_error("commit replace PostgreSQL roleplay lore", error))?;
        Ok(record)
    }

    pub fn supersede_roleplay_lore_record(
        &self,
        supersede: &RoleplayLoreSupersede,
    ) -> CoreResult<(RoleplayLoreRecord, RoleplayLoreRecord)> {
        validate_roleplay_lore_write(&supersede.replacement)?;
        if supersede.replacement.supersedes_record_id.as_deref()
            != Some(supersede.record_id.as_str())
        {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "roleplay lore replacement must reference the superseded record",
            ));
        }
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start supersede PostgreSQL roleplay lore", error))?;
        let existing = active_roleplay_lore_record_for_update(
            &mut tx,
            &schema,
            &supersede.record_id,
            supersede.expected_revision,
        )?;
        if existing.world_id != supersede.replacement.world_id {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "roleplay lore replacement must stay in the same world",
            ));
        }
        if get_roleplay_lore_record(&mut tx, &schema, &supersede.replacement.record_id)?.is_some() {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!(
                    "roleplay lore replacement {} already exists",
                    supersede.replacement.record_id
                ),
            ));
        }
        insert_roleplay_lore_record(&mut tx, &schema, &supersede.replacement)?;
        mark_roleplay_lore_superseded(
            &mut tx,
            &schema,
            &existing.record_id,
            &supersede.replacement.record_id,
            existing.revision + 1,
            &supersede.replacement.now,
        )?;
        insert_roleplay_lore_provenance_event(
            &mut tx,
            &schema,
            &RoleplayLoreProvenanceEvent {
                event_id: format!(
                    "{}:superseded_by:{}",
                    existing.record_id, supersede.replacement.record_id
                ),
                record_id: existing.record_id.clone(),
                world_id: existing.world_id.clone(),
                evidence_refs: supersede.replacement.evidence_refs.clone(),
                source: supersede.replacement.source,
                actor: crate::memory_proposal_source_as_str(supersede.replacement.source)
                    .to_string(),
                note: Some(format!("superseded by {}", supersede.replacement.record_id)),
                created_at: supersede.replacement.now.clone(),
            },
        )?;
        let old_record = get_roleplay_lore_record(&mut tx, &schema, &existing.record_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "superseded PostgreSQL roleplay lore record was not readable",
                )
            })?;
        let new_record =
            get_roleplay_lore_record(&mut tx, &schema, &supersede.replacement.record_id)?
                .ok_or_else(|| {
                    CoreError::new(
                        CoreErrorKind::PersistenceFailure,
                        "replacement PostgreSQL roleplay lore record was not readable",
                    )
                })?;
        tx.commit()
            .map_err(|error| postgres_error("commit supersede PostgreSQL roleplay lore", error))?;
        Ok((old_record, new_record))
    }

    pub fn tombstone_roleplay_lore_record(
        &self,
        tombstone: &RoleplayLoreTombstone,
    ) -> CoreResult<RoleplayLoreRecord> {
        validate_roleplay_lore_record_id(&tombstone.record_id)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start tombstone PostgreSQL roleplay lore", error))?;
        let existing = active_roleplay_lore_record_for_update(
            &mut tx,
            &schema,
            &tombstone.record_id,
            tombstone.expected_revision,
        )?;
        tombstone_roleplay_lore_record(&mut tx, &schema, tombstone, existing.revision + 1)?;
        insert_roleplay_lore_provenance_event(
            &mut tx,
            &schema,
            &RoleplayLoreProvenanceEvent {
                event_id: format!(
                    "{}:tombstoned:{}",
                    tombstone.record_id,
                    existing.revision + 1
                ),
                record_id: tombstone.record_id.clone(),
                world_id: existing.world_id,
                evidence_refs: existing.evidence_refs,
                source: existing.source,
                actor: "rusty_crew_storage".to_string(),
                note: tombstone.reason.clone(),
                created_at: tombstone.now.clone(),
            },
        )?;
        let record =
            get_roleplay_lore_record(&mut tx, &schema, &tombstone.record_id)?.ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "tombstoned PostgreSQL roleplay lore record was not readable",
                )
            })?;
        tx.commit()
            .map_err(|error| postgres_error("commit tombstone PostgreSQL roleplay lore", error))?;
        Ok(record)
    }

    pub fn query_roleplay_lore_records(
        &self,
        query: &RoleplayLoreQuery,
    ) -> CoreResult<Vec<RoleplayLoreRecord>> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        query_roleplay_lore_records(&mut *client, &schema, query)
    }

    pub fn roleplay_lore_provenance_events(
        &self,
        record_id: &str,
    ) -> CoreResult<Vec<RoleplayLoreProvenanceEvent>> {
        validate_roleplay_lore_record_id(record_id)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        roleplay_lore_provenance_events(&mut *client, &schema, record_id)
    }

    pub fn save_attachment(&self, attachment: &AttachmentWrite) -> CoreResult<AttachmentRecord> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start save PostgreSQL attachment", error))?;
        save_attachment_in_tx(&mut tx, &schema, attachment)?;
        let record = load_attachment(&mut tx, &schema, &attachment.attachment_id)?;
        tx.commit()
            .map_err(|error| postgres_error("commit save PostgreSQL attachment", error))?;
        Ok(record)
    }

    pub fn query_attachments(&self, query: &AttachmentQuery) -> CoreResult<Vec<AttachmentRecord>> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        query_attachments(&mut *client, &schema, query)
    }

    pub fn remove_attachment(
        &self,
        attachment_id: &AttachmentId,
        updated_at: &IsoTimestamp,
    ) -> CoreResult<AttachmentRecord> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start remove PostgreSQL attachment", error))?;
        tx.execute(
            &format!(
                "UPDATE {schema}.attachments
                 SET status = 'removed',
                     updated_at = $2
                 WHERE attachment_id = $1"
            ),
            &[&attachment_id.0, updated_at],
        )
        .map_err(|error| postgres_error("remove PostgreSQL attachment", error))?;
        let record = load_attachment(&mut tx, &schema, attachment_id)?;
        tx.commit()
            .map_err(|error| postgres_error("commit remove PostgreSQL attachment", error))?;
        Ok(record)
    }

    pub fn save_data_bank_scope(
        &self,
        scope: &DataBankScopeWrite,
    ) -> CoreResult<DataBankScopeRecord> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start save PostgreSQL data-bank scope", error))?;
        save_data_bank_scope_in_tx(&mut tx, &schema, scope)?;
        let record = load_data_bank_scope(&mut tx, &schema, &scope.scope_id)?;
        tx.commit()
            .map_err(|error| postgres_error("commit save PostgreSQL data-bank scope", error))?;
        Ok(record)
    }

    pub fn query_data_bank_scopes(
        &self,
        query: &DataBankScopeQuery,
    ) -> CoreResult<Vec<DataBankScopeRecord>> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        query_data_bank_scopes(&mut *client, &schema, query)
    }

    pub fn remove_data_bank_scope(
        &self,
        scope_id: &DataBankScopeId,
        updated_at: &IsoTimestamp,
    ) -> CoreResult<DataBankScopeRecord> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start remove PostgreSQL data-bank scope", error))?;
        tx.execute(
            &format!(
                "UPDATE {schema}.data_bank_scopes
                 SET status = 'removed',
                     updated_at = $2
                 WHERE scope_id = $1"
            ),
            &[&scope_id.0, updated_at],
        )
        .map_err(|error| postgres_error("remove PostgreSQL data-bank scope", error))?;
        let record = load_data_bank_scope(&mut tx, &schema, scope_id)?;
        tx.commit()
            .map_err(|error| postgres_error("commit remove PostgreSQL data-bank scope", error))?;
        Ok(record)
    }

    pub fn save_message_slot(&self, slot: &MessageSlotWrite) -> CoreResult<()> {
        let metadata_json = to_json_text(&slot.metadata_json)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start save PostgreSQL message slot", error))?;
        tx.execute(
            &format!(
                "INSERT INTO {schema}.message_slots (
                    slot_id,
                    session_id,
                    primary_variant_id,
                    active_variant_id,
                    metadata_json,
                    created_at,
                    updated_at,
                    version
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, 0)
                 ON CONFLICT(slot_id) DO UPDATE SET
                    session_id = EXCLUDED.session_id,
                    primary_variant_id = EXCLUDED.primary_variant_id,
                    active_variant_id = EXCLUDED.active_variant_id,
                    metadata_json = EXCLUDED.metadata_json,
                    updated_at = EXCLUDED.updated_at,
                    version = message_slots.version + 1"
            ),
            &[
                &slot.slot_id.0,
                &slot.session_id.0,
                &slot.primary_variant_id.0,
                &slot
                    .active_variant_id
                    .as_ref()
                    .map(|value| value.0.as_str()),
                &metadata_json,
                &slot.created_at,
                &slot.updated_at,
            ],
        )
        .map_err(|error| postgres_error("save PostgreSQL message slot", error))?;
        tx.commit()
            .map_err(|error| postgres_error("commit save PostgreSQL message slot", error))?;
        Ok(())
    }

    pub fn save_message_variant(
        &self,
        variant: &MessageVariantWrite,
    ) -> CoreResult<MessageVariantRecord> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start save PostgreSQL message variant", error))?;
        save_message_variant_in_tx(&mut tx, &schema, variant)?;
        let record = load_message_variant_in_tx(&mut tx, &schema, &variant.variant_id)?;
        tx.commit()
            .map_err(|error| postgres_error("commit save PostgreSQL message variant", error))?;
        Ok(record)
    }

    pub fn query_message_slots(
        &self,
        query: &MessageSlotQuery,
    ) -> CoreResult<Vec<MessageSlotRecord>> {
        let schema = self.quoted_schema();
        let session_id = query.session_id.as_ref().map(|value| value.0.as_str());
        let (limit, offset) = query
            .page
            .unwrap_or(QueryPage {
                limit: None,
                offset: None,
            })
            .bounded(100, 1_000);
        let mut client = self.client()?;
        let rows = client
            .query(
                &format!(
                    "SELECT slot_id
                     FROM {schema}.message_slots
                     WHERE ($1::text IS NULL OR session_id = $1)
                     ORDER BY created_at ASC, slot_id ASC
                     LIMIT $2 OFFSET $3"
                ),
                &[&session_id, &limit, &offset],
            )
            .map_err(|error| postgres_error("query PostgreSQL message slots", error))?;
        rows.iter()
            .map(|row| MessageSlotId::new(row.get::<_, String>(0)))
            .map(|slot_id| {
                load_message_slot(&mut *client, &schema, &slot_id, query.include_alternates)
            })
            .collect()
    }

    pub fn query_message_variants(
        &self,
        query: &MessageVariantQuery,
    ) -> CoreResult<Vec<MessageVariantRecord>> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        query_message_variants(&mut *client, &schema, query)
    }

    pub fn select_active_message_variant(
        &self,
        request: &SelectActiveVariantRequest,
    ) -> CoreResult<SelectActiveVariantResult> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start select PostgreSQL active message variant", error)
        })?;
        let current = current_active_variant_in_tx(&mut tx, &schema, &request.slot_id)?;
        let expected = match &request.expected {
            ActiveVariantExpectation::Any => current.clone(),
            ActiveVariantExpectation::Primary => None,
            ActiveVariantExpectation::Variant(variant_id) => Some(variant_id.clone()),
        };
        if request.expected != ActiveVariantExpectation::Any && current != expected {
            let slot = load_message_slot_in_tx(&mut tx, &schema, &request.slot_id, true)?;
            tx.commit().map_err(|error| {
                postgres_error("commit PostgreSQL active variant conflict", error)
            })?;
            return Ok(SelectActiveVariantResult {
                slot,
                conflict: Some(ActiveVariantConflict {
                    expected,
                    actual: current,
                }),
            });
        }
        if let Some(variant_id) = &request.active_variant_id {
            ensure_variant_belongs_to_slot_in_tx(&mut tx, &schema, &request.slot_id, variant_id)?;
        }
        tx.execute(
            &format!(
                "UPDATE {schema}.message_slots
                 SET active_variant_id = $2,
                     updated_at = $3,
                     version = version + 1
                 WHERE slot_id = $1"
            ),
            &[
                &request.slot_id.0,
                &request
                    .active_variant_id
                    .as_ref()
                    .map(|value| value.0.as_str()),
                &request.updated_at,
            ],
        )
        .map_err(|error| postgres_error("select PostgreSQL active message variant", error))?;
        let slot = load_message_slot_in_tx(&mut tx, &schema, &request.slot_id, true)?;
        tx.commit().map_err(|error| {
            postgres_error("commit select PostgreSQL active message variant", error)
        })?;
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
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start delete PostgreSQL message variant", error))?;
        ensure_variant_belongs_to_slot_in_tx(&mut tx, &schema, slot_id, variant_id)?;
        tx.execute(
            &format!(
                "UPDATE {schema}.message_variants
                 SET status = 'deleted',
                     updated_at = $3
                 WHERE slot_id = $1
                   AND variant_id = $2
                   AND source <> 'primary'"
            ),
            &[&slot_id.0, &variant_id.0, updated_at],
        )
        .map_err(|error| postgres_error("delete PostgreSQL message variant", error))?;
        tx.execute(
            &format!(
                "UPDATE {schema}.message_slots
                 SET active_variant_id = CASE
                        WHEN active_variant_id = $2 THEN NULL
                        ELSE active_variant_id
                     END,
                     updated_at = $3,
                     version = version + 1
                 WHERE slot_id = $1"
            ),
            &[&slot_id.0, &variant_id.0, updated_at],
        )
        .map_err(|error| postgres_error("clear PostgreSQL deleted active variant", error))?;
        let slot = load_message_slot_in_tx(&mut tx, &schema, slot_id, true)?;
        tx.commit()
            .map_err(|error| postgres_error("commit delete PostgreSQL message variant", error))?;
        Ok(slot)
    }

    pub fn reorder_message_variants(
        &self,
        slot_id: &MessageSlotId,
        ordered_variant_ids: &[MessageVariantId],
        updated_at: &IsoTimestamp,
    ) -> CoreResult<Vec<MessageVariantRecord>> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start reorder PostgreSQL message variants", error))?;
        for (index, variant_id) in ordered_variant_ids.iter().enumerate() {
            ensure_variant_belongs_to_slot_in_tx(&mut tx, &schema, slot_id, variant_id)?;
            tx.execute(
                &format!(
                    "UPDATE {schema}.message_variants
                     SET ordinal = $3,
                         updated_at = $4
                     WHERE slot_id = $1
                       AND variant_id = $2
                       AND source <> 'primary'"
                ),
                &[
                    &slot_id.0,
                    &variant_id.0,
                    &(-((index + 1) as i64)),
                    updated_at,
                ],
            )
            .map_err(|error| postgres_error("stage PostgreSQL message variant reorder", error))?;
        }
        for (index, variant_id) in ordered_variant_ids.iter().enumerate() {
            tx.execute(
                &format!(
                    "UPDATE {schema}.message_variants
                     SET ordinal = $3,
                         updated_at = $4
                     WHERE slot_id = $1
                       AND variant_id = $2
                       AND source <> 'primary'"
                ),
                &[&slot_id.0, &variant_id.0, &((index + 1) as i64), updated_at],
            )
            .map_err(|error| postgres_error("reorder PostgreSQL message variant", error))?;
        }
        tx.execute(
            &format!(
                "UPDATE {schema}.message_slots
                 SET updated_at = $2,
                     version = version + 1
                 WHERE slot_id = $1"
            ),
            &[&slot_id.0, updated_at],
        )
        .map_err(|error| postgres_error("touch PostgreSQL reordered message slot", error))?;
        let variants = query_message_variants(
            &mut tx,
            &schema,
            &MessageVariantQuery {
                slot_id: Some(slot_id.clone()),
                include_deleted: false,
                page: None,
            },
        )?;
        tx.commit()
            .map_err(|error| postgres_error("commit reorder PostgreSQL message variants", error))?;
        Ok(variants)
    }

    pub fn save_conversation_branch(
        &self,
        branch: &ConversationBranchWrite,
    ) -> CoreResult<ConversationBranchRecord> {
        let metadata_json = to_json_text(&branch.metadata_json)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start save PostgreSQL conversation branch", error))?;
        tx.execute(
            &format!(
                "INSERT INTO {schema}.conversation_branches (
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
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 0)
                 ON CONFLICT(branch_id) DO UPDATE SET
                    session_id = EXCLUDED.session_id,
                    parent_branch_id = EXCLUDED.parent_branch_id,
                    parent_message_id = EXCLUDED.parent_message_id,
                    origin_message_id = EXCLUDED.origin_message_id,
                    head_message_id = EXCLUDED.head_message_id,
                    label = EXCLUDED.label,
                    metadata_json = EXCLUDED.metadata_json,
                    updated_at = EXCLUDED.updated_at,
                    version = conversation_branches.version + 1"
            ),
            &[
                &branch.branch_id.0,
                &branch.session_id.0,
                &branch
                    .parent_branch_id
                    .as_ref()
                    .map(|value| value.0.as_str()),
                &branch
                    .parent_message_id
                    .as_ref()
                    .map(|value| value.0.as_str()),
                &branch
                    .origin_message_id
                    .as_ref()
                    .map(|value| value.0.as_str()),
                &branch
                    .head_message_id
                    .as_ref()
                    .map(|value| value.0.as_str()),
                &branch.label,
                &metadata_json,
                &branch.created_at,
                &branch.updated_at,
            ],
        )
        .map_err(|error| postgres_error("save PostgreSQL conversation branch", error))?;
        let record = load_conversation_branch_in_tx(&mut tx, &schema, &branch.branch_id)?;
        tx.commit()
            .map_err(|error| postgres_error("commit save PostgreSQL conversation branch", error))?;
        Ok(record)
    }

    pub fn query_conversation_branches(
        &self,
        query: &ConversationBranchQuery,
    ) -> CoreResult<Vec<ConversationBranchRecord>> {
        let schema = self.quoted_schema();
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
        let mut client = self.client()?;
        let rows = client
            .query(
                &format!(
                    "SELECT branch_id
                     FROM {schema}.conversation_branches
                     WHERE ($1::text IS NULL OR session_id = $1)
                       AND ($2::text IS NULL OR parent_branch_id = $2)
                     ORDER BY created_at ASC, branch_id ASC
                     LIMIT $3 OFFSET $4"
                ),
                &[&session_id, &parent_branch_id, &limit, &offset],
            )
            .map_err(|error| postgres_error("query PostgreSQL conversation branches", error))?;
        rows.iter()
            .map(|row| ConversationBranchId::new(row.get::<_, String>(0)))
            .map(|branch_id| load_conversation_branch(&mut *client, &schema, &branch_id))
            .collect()
    }

    pub fn get_conversation_branch_state(
        &self,
        session_id: &SessionId,
        default_updated_at: &IsoTimestamp,
    ) -> CoreResult<ConversationBranchStateRecord> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        load_conversation_branch_state(&mut *client, &schema, session_id, default_updated_at)
    }

    pub fn select_active_conversation_branch(
        &self,
        request: &SelectActiveBranchRequest,
    ) -> CoreResult<SelectActiveBranchResult> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start select PostgreSQL active conversation branch", error)
        })?;
        let current = current_active_branch_in_tx(&mut tx, &schema, &request.session_id)?;
        let expected = match &request.expected {
            ActiveBranchExpectation::Any => current.clone(),
            ActiveBranchExpectation::None => None,
            ActiveBranchExpectation::Branch(branch_id) => Some(branch_id.clone()),
        };
        if request.expected != ActiveBranchExpectation::Any && current != expected {
            let state = load_conversation_branch_state_in_tx(
                &mut tx,
                &schema,
                &request.session_id,
                &request.updated_at,
            )?;
            tx.commit().map_err(|error| {
                postgres_error("commit PostgreSQL active branch conflict", error)
            })?;
            return Ok(SelectActiveBranchResult {
                state,
                conflict: Some(ActiveBranchConflict {
                    expected,
                    actual: current,
                }),
            });
        }
        if let Some(branch_id) = &request.active_branch_id {
            ensure_branch_belongs_to_session_in_tx(
                &mut tx,
                &schema,
                &request.session_id,
                branch_id,
            )?;
        }
        let changed = if current.is_none() {
            tx.execute(
                &format!(
                    "INSERT INTO {schema}.conversation_branch_state (
                        session_id,
                        active_branch_id,
                        updated_at,
                        version
                     ) VALUES ($1, $2, $3, 0)
                     ON CONFLICT(session_id) DO NOTHING"
                ),
                &[
                    &request.session_id.0,
                    &request
                        .active_branch_id
                        .as_ref()
                        .map(|value| value.0.as_str()),
                    &request.updated_at,
                ],
            )
        } else {
            tx.execute(
                &format!(
                    "UPDATE {schema}.conversation_branch_state
                     SET active_branch_id = $2,
                         updated_at = $3,
                         version = version + 1
                     WHERE session_id = $1"
                ),
                &[
                    &request.session_id.0,
                    &request
                        .active_branch_id
                        .as_ref()
                        .map(|value| value.0.as_str()),
                    &request.updated_at,
                ],
            )
        }
        .map_err(|error| postgres_error("select PostgreSQL active branch", error))?;
        if changed == 0 {
            let state = load_conversation_branch_state_in_tx(
                &mut tx,
                &schema,
                &request.session_id,
                &request.updated_at,
            )?;
            let actual = state.active_branch_id.clone();
            tx.commit().map_err(|error| {
                postgres_error("commit PostgreSQL active branch insert conflict", error)
            })?;
            return Ok(SelectActiveBranchResult {
                state,
                conflict: Some(ActiveBranchConflict { expected, actual }),
            });
        }
        let state = load_conversation_branch_state_in_tx(
            &mut tx,
            &schema,
            &request.session_id,
            &request.updated_at,
        )?;
        tx.commit().map_err(|error| {
            postgres_error("commit select PostgreSQL active conversation branch", error)
        })?;
        Ok(SelectActiveBranchResult {
            state,
            conflict: None,
        })
    }

    pub fn update_conversation_branch_head(
        &self,
        request: &UpdateBranchHeadRequest,
    ) -> CoreResult<UpdateBranchHeadResult> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start update PostgreSQL branch head", error))?;
        let current = current_branch_head_in_tx(&mut tx, &schema, &request.branch_id)?;
        let expected = match &request.expected {
            BranchHeadExpectation::Any => current.clone(),
            BranchHeadExpectation::None => None,
            BranchHeadExpectation::Message(message_id) => Some(message_id.clone()),
        };
        if request.expected != BranchHeadExpectation::Any && current != expected {
            let branch = load_conversation_branch_in_tx(&mut tx, &schema, &request.branch_id)?;
            tx.commit()
                .map_err(|error| postgres_error("commit PostgreSQL branch head conflict", error))?;
            return Ok(UpdateBranchHeadResult {
                branch,
                conflict: Some(BranchHeadConflict {
                    expected,
                    actual: current,
                }),
            });
        }
        if let Some(message_id) = &request.head_message_id {
            ensure_message_exists_in_tx(&mut tx, &schema, message_id)?;
        }
        tx.execute(
            &format!(
                "UPDATE {schema}.conversation_branches
                 SET head_message_id = $2,
                     updated_at = $3,
                     version = version + 1
                 WHERE branch_id = $1"
            ),
            &[
                &request.branch_id.0,
                &request
                    .head_message_id
                    .as_ref()
                    .map(|value| value.0.as_str()),
                &request.updated_at,
            ],
        )
        .map_err(|error| postgres_error("update PostgreSQL branch head", error))?;
        let branch = load_conversation_branch_in_tx(&mut tx, &schema, &request.branch_id)?;
        tx.commit()
            .map_err(|error| postgres_error("commit update PostgreSQL branch head", error))?;
        Ok(UpdateBranchHeadResult {
            branch,
            conflict: None,
        })
    }

    pub fn save_conversation_snapshot(
        &self,
        snapshot: &ConversationSnapshotWrite,
    ) -> CoreResult<ConversationSnapshotRecord> {
        let metadata_json = to_json_text(&snapshot.metadata_json)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start save PostgreSQL conversation snapshot", error)
        })?;
        let source = conversation_snapshot_source_as_str(snapshot.source);
        tx.execute(
            &format!(
                "INSERT INTO {schema}.conversation_snapshots (
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
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                 ON CONFLICT(snapshot_id) DO UPDATE SET
                    session_id = EXCLUDED.session_id,
                    branch_id = EXCLUDED.branch_id,
                    message_id = EXCLUDED.message_id,
                    cursor = EXCLUDED.cursor,
                    label = EXCLUDED.label,
                    summary = EXCLUDED.summary,
                    source = EXCLUDED.source,
                    metadata_json = EXCLUDED.metadata_json,
                    updated_at = EXCLUDED.updated_at"
            ),
            &[
                &snapshot.snapshot_id.0,
                &snapshot.session_id.0,
                &snapshot.branch_id.as_ref().map(|value| value.0.as_str()),
                &snapshot.message_id.as_ref().map(|value| value.0.as_str()),
                &snapshot.cursor,
                &snapshot.label,
                &snapshot.summary,
                &source,
                &metadata_json,
                &snapshot.created_at,
                &snapshot.updated_at,
            ],
        )
        .map_err(|error| postgres_error("save PostgreSQL conversation snapshot", error))?;
        let record = load_conversation_snapshot_in_tx(&mut tx, &schema, &snapshot.snapshot_id)?;
        tx.commit().map_err(|error| {
            postgres_error("commit save PostgreSQL conversation snapshot", error)
        })?;
        Ok(record)
    }

    pub fn query_conversation_snapshots(
        &self,
        query: &ConversationSnapshotQuery,
    ) -> CoreResult<Vec<ConversationSnapshotRecord>> {
        let schema = self.quoted_schema();
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
        let mut client = self.client()?;
        let rows = client
            .query(
                &format!(
                    "SELECT snapshot_id
                     FROM {schema}.conversation_snapshots
                     WHERE ($1::text IS NULL OR session_id = $1)
                       AND ($2::text IS NULL OR branch_id = $2)
                       AND ($3::text IS NULL OR message_id = $3)
                     ORDER BY created_at ASC, snapshot_id ASC
                     LIMIT $4 OFFSET $5"
                ),
                &[&session_id, &branch_id, &message_id, &limit, &offset],
            )
            .map_err(|error| postgres_error("query PostgreSQL conversation snapshots", error))?;
        rows.iter()
            .map(|row| ConversationSnapshotId::new(row.get::<_, String>(0)))
            .map(|snapshot_id| load_conversation_snapshot(&mut *client, &schema, &snapshot_id))
            .collect()
    }

    pub fn resolve_conversation_jump(
        &self,
        request: &ConversationJumpRequest,
    ) -> CoreResult<ConversationJumpResult> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        resolve_conversation_jump(&mut *client, &schema, request)
    }

    pub fn save_provider_wire_state(
        &self,
        write: &ProviderWireStateWrite,
    ) -> CoreResult<ProviderWireStateRecord> {
        validate_provider_wire_state_key(&write.key)?;
        let payload_json = to_json_text(&write.payload_json)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start save PostgreSQL provider wire state", error))?;
        invalidate_current_provider_wire_state_for_key_in_tx(
            &mut tx,
            &schema,
            &write.key,
            &write.now,
            ProviderWireStateInvalidationReason::Superseded,
        )?;
        let row = tx
            .query_one(
                &format!(
                    "INSERT INTO {schema}.provider_wire_states (
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
                     ) VALUES ($1, $2, $3, $4, $5, $6, $7, 'json', $8, $8, $9, $10, NULL, NULL)
                     RETURNING row_id,
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
                        invalidation_reason"
                ),
                &[
                    &write.key.session_id.0,
                    &write.key.module_id,
                    &write.key.strategy_id,
                    &write.profile_fingerprint,
                    &write.provider_fingerprint,
                    &write.payload_version,
                    &payload_json,
                    &write.now,
                    &write.expires_at,
                    &write.last_wake_id,
                ],
            )
            .map_err(|error| postgres_error("insert PostgreSQL provider wire state", error))?;
        let record = row_to_provider_wire_state_record(&row)?;
        tx.commit()
            .map_err(|error| postgres_error("commit save PostgreSQL provider wire state", error))?;
        Ok(record)
    }

    pub fn load_provider_wire_state_for_wake(
        &self,
        lookup: &ProviderWireStateWakeLookup,
    ) -> CoreResult<ProviderWireStateWakeResult> {
        validate_provider_wire_state_key(&lookup.key)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start load PostgreSQL provider wire state", error))?;
        invalidate_provider_wire_states_for_session_except_in_tx(
            &mut tx,
            &schema,
            &lookup.key,
            &lookup.now,
        )?;
        let Some(record) = load_current_provider_wire_state_by_key(&mut tx, &schema, &lookup.key)?
        else {
            tx.commit().map_err(|error| {
                postgres_error(
                    "commit missing PostgreSQL provider wire state lookup",
                    error,
                )
            })?;
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
                &mut tx,
                &schema,
                record.row_id,
                &lookup.now,
                ProviderWireStateInvalidationReason::Expired,
            )?;
            tx.commit().map_err(|error| {
                postgres_error(
                    "commit expired PostgreSQL provider wire state lookup",
                    error,
                )
            })?;
            return Ok(ProviderWireStateWakeResult {
                record: None,
                absence_reason: Some(ProviderStateAbsenceReason::Expired),
            });
        }
        if record.profile_fingerprint != lookup.profile_fingerprint {
            invalidate_provider_wire_state_by_row_in_tx(
                &mut tx,
                &schema,
                record.row_id,
                &lookup.now,
                ProviderWireStateInvalidationReason::ProfileChanged,
            )?;
            tx.commit().map_err(|error| {
                postgres_error(
                    "commit profile-invalidated PostgreSQL provider wire state lookup",
                    error,
                )
            })?;
            return Ok(ProviderWireStateWakeResult {
                record: None,
                absence_reason: Some(ProviderStateAbsenceReason::Invalidated),
            });
        }
        if record.provider_fingerprint != lookup.provider_fingerprint {
            invalidate_provider_wire_state_by_row_in_tx(
                &mut tx,
                &schema,
                record.row_id,
                &lookup.now,
                ProviderWireStateInvalidationReason::ProviderChanged,
            )?;
            tx.commit().map_err(|error| {
                postgres_error(
                    "commit provider-invalidated PostgreSQL provider wire state lookup",
                    error,
                )
            })?;
            return Ok(ProviderWireStateWakeResult {
                record: None,
                absence_reason: Some(ProviderStateAbsenceReason::Invalidated),
            });
        }
        tx.commit().map_err(|error| {
            postgres_error("commit PostgreSQL provider wire state lookup", error)
        })?;
        Ok(ProviderWireStateWakeResult {
            record: Some(record),
            absence_reason: None,
        })
    }

    pub fn clear_provider_wire_state(
        &self,
        key: &ProviderWireStateKey,
        now: &IsoTimestamp,
        reason: ProviderWireStateInvalidationReason,
    ) -> CoreResult<Option<ProviderWireStateRecord>> {
        validate_provider_wire_state_key(key)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start clear PostgreSQL provider wire state", error))?;
        let Some(record) = load_current_provider_wire_state_by_key(&mut tx, &schema, key)? else {
            tx.commit().map_err(|error| {
                postgres_error("commit missing PostgreSQL provider wire state clear", error)
            })?;
            return Ok(None);
        };
        invalidate_provider_wire_state_by_row_in_tx(&mut tx, &schema, record.row_id, now, reason)?;
        let cleared = load_provider_wire_state_by_row_id(&mut tx, &schema, record.row_id)?;
        tx.commit().map_err(|error| {
            postgres_error("commit clear PostgreSQL provider wire state", error)
        })?;
        Ok(Some(cleared))
    }

    pub fn expire_provider_wire_states_at(
        &self,
        now: &IsoTimestamp,
    ) -> CoreResult<Vec<ProviderWireStateRecord>> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start expire PostgreSQL provider wire states", error)
        })?;
        let expiring = load_expired_current_provider_wire_states(&mut tx, &schema, now)?;
        for record in &expiring {
            invalidate_provider_wire_state_by_row_in_tx(
                &mut tx,
                &schema,
                record.row_id,
                now,
                ProviderWireStateInvalidationReason::Expired,
            )?;
        }
        let expired = expiring
            .iter()
            .map(|record| load_provider_wire_state_by_row_id(&mut tx, &schema, record.row_id))
            .collect::<CoreResult<Vec<_>>>()?;
        tx.commit().map_err(|error| {
            postgres_error("commit expire PostgreSQL provider wire states", error)
        })?;
        Ok(expired)
    }

    pub fn list_provider_wire_state_diagnostics(
        &self,
        limit: u32,
    ) -> CoreResult<Vec<ProviderWireStateDiagnostic>> {
        let schema = self.quoted_schema();
        let limit = i64::from(limit);
        let rows = self
            .client()?
            .query(
                &format!(
                    "SELECT
                        session_id,
                        module_id,
                        strategy_id,
                        payload_version,
                        octet_length(payload_json)::bigint,
                        created_at,
                        updated_at,
                        expires_at,
                        last_wake_id,
                        invalidated_at,
                        invalidation_reason
                     FROM {schema}.provider_wire_states
                     ORDER BY updated_at DESC, row_id DESC
                     LIMIT $1"
                ),
                &[&limit],
            )
            .map_err(|error| postgres_error("list PostgreSQL provider wire diagnostics", error))?;
        rows.iter()
            .map(row_to_provider_wire_state_diagnostic)
            .collect()
    }

    pub fn upsert_runtime_search_entry(&self, entry: &RuntimeSearchResult) -> CoreResult<()> {
        let schema = self.quoted_schema();
        let row_type = runtime_search_row_type_as_str(entry.row_type);
        let event_kind = entry.event_kind.as_ref().map(|kind| format!("{kind:?}"));
        self.client()?
            .execute(
                &format!(
                    "INSERT INTO {schema}.runtime_search_entries (
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
                     ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                     ON CONFLICT(row_type, row_key) DO UPDATE SET
                        sequence = EXCLUDED.sequence,
                        session_id = EXCLUDED.session_id,
                        agent_id = EXCLUDED.agent_id,
                        instance_id = EXCLUDED.instance_id,
                        task_id = EXCLUDED.task_id,
                        event_kind = EXCLUDED.event_kind,
                        recorded_at = EXCLUDED.recorded_at,
                        title = EXCLUDED.title,
                        body = EXCLUDED.body"
                ),
                &[
                    &row_type,
                    &entry.row_key,
                    &entry.sequence.map(|value| value as i64),
                    &entry.session_id.as_ref().map(|value| value.0.as_str()),
                    &entry.agent_id.as_ref().map(|value| value.0.as_str()),
                    &entry.instance_id.as_ref().map(|value| value.0.as_str()),
                    &entry.task_id.as_ref().map(|value| value.0.as_str()),
                    &event_kind,
                    &entry.recorded_at,
                    &entry.title,
                    &entry.body,
                ],
            )
            .map_err(|error| postgres_error("upsert PostgreSQL runtime search entry", error))?;
        Ok(())
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
        let row_type = filter.row_type.map(runtime_search_row_type_as_str);
        let session_id = filter.session_id.as_ref().map(|value| value.0.as_str());
        let agent_id = filter.agent_id.as_ref().map(|value| value.0.as_str());
        let instance_id = filter.instance_id.as_ref().map(|value| value.0.as_str());
        let task_id = filter.task_id.as_ref().map(|value| value.0.as_str());
        let event_kind = filter.event_kind.as_ref().map(|kind| format!("{kind:?}"));
        let recorded_after = filter.recorded_after.as_deref();
        let recorded_before = filter.recorded_before.as_deref();
        let limit = filter.limit.unwrap_or(50).clamp(1, 200) as i64;
        let query = filter.query.trim();
        let schema = self.quoted_schema();
        let rows = self
            .client()?
            .query(
                &format!(
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
                     FROM {schema}.runtime_search_entries
                     WHERE search_vector @@ plainto_tsquery('simple', $1)
                       AND ($2::text IS NULL OR row_type = $2)
                       AND ($3::text IS NULL OR session_id = $3)
                       AND ($4::text IS NULL OR agent_id = $4)
                       AND ($5::text IS NULL OR instance_id = $5)
                       AND ($6::text IS NULL OR task_id = $6)
                       AND ($7::text IS NULL OR event_kind = $7)
                       AND ($8::text IS NULL OR recorded_at >= $8)
                       AND ($9::text IS NULL OR recorded_at <= $9)
                     ORDER BY
                       ts_rank(search_vector, plainto_tsquery('simple', $1)) DESC,
                       recorded_at ASC,
                       row_type ASC,
                       row_key ASC
                     LIMIT $10"
                ),
                &[
                    &query,
                    &row_type,
                    &session_id,
                    &agent_id,
                    &instance_id,
                    &task_id,
                    &event_kind,
                    &recorded_after,
                    &recorded_before,
                    &limit,
                ],
            )
            .map_err(|error| postgres_error("query PostgreSQL runtime search", error))?;
        rows.iter().map(row_to_runtime_search_result).collect()
    }

    #[cfg(test)]
    fn drop_schema_for_test(&self) -> CoreResult<()> {
        let schema = self.quoted_schema();
        self.client()?
            .batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
            .map_err(|error| postgres_error("drop PostgreSQL proof schema", error))
    }

    fn migrate(&self) -> CoreResult<()> {
        let schema = self.quoted_schema();
        self.client()?
            .batch_execute(&format!(
                "CREATE SCHEMA IF NOT EXISTS {schema};
                 CREATE TABLE IF NOT EXISTS {schema}.rusty_crew_storage_metadata (
                    metadata_key TEXT PRIMARY KEY,
                    metadata_value TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                 );
                 INSERT INTO {schema}.rusty_crew_storage_metadata (
                    metadata_key,
                    metadata_value,
                    updated_at
                 ) VALUES (
                    'runtime_counter_proof_schema_version',
                    '{POSTGRES_PROOF_SCHEMA_VERSION}',
                    '2026-06-26T00:00:00Z'
                 )
                 ON CONFLICT(metadata_key) DO UPDATE SET
                    metadata_value = EXCLUDED.metadata_value,
                    updated_at = EXCLUDED.updated_at;
                 CREATE TABLE IF NOT EXISTS {schema}.runtime_counters (
                    scope_type TEXT NOT NULL,
                    scope_id TEXT NOT NULL,
                    counter_name TEXT NOT NULL,
                    value BIGINT NOT NULL DEFAULT 0,
                    updated_at TEXT NOT NULL,
                    PRIMARY KEY(scope_type, scope_id, counter_name)
                 );
                 CREATE INDEX IF NOT EXISTS runtime_counters_scope_idx
                    ON {schema}.runtime_counters(scope_type, scope_id);
                 CREATE TABLE IF NOT EXISTS {schema}.sessions (
                    session_id TEXT PRIMARY KEY,
                    handle BIGINT NOT NULL,
                    agent_id TEXT NOT NULL,
                    profile_id TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    status TEXT NOT NULL,
                    state_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    last_active_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS sessions_agent_profile_idx
                    ON {schema}.sessions(agent_id, profile_id, kind, status, session_id);
                 CREATE TABLE IF NOT EXISTS {schema}.session_configs (
                    session_id TEXT PRIMARY KEY,
                    profile_id TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    record_json TEXT NOT NULL,
                    created_at TEXT NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS {schema}.profile_registry (
                    profile_id TEXT PRIMARY KEY,
                    lifecycle_status TEXT NOT NULL,
                    record_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS profile_registry_status_idx
                    ON {schema}.profile_registry(lifecycle_status, profile_id);
                 CREATE TABLE IF NOT EXISTS {schema}.model_providers (
                    alias TEXT PRIMARY KEY,
                    status TEXT NOT NULL,
                    protocol TEXT NOT NULL,
                    provider_json TEXT NOT NULL,
                    secret_ciphertext TEXT,
                    secret_updated_at TEXT,
                    revision BIGINT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS model_providers_status_idx
                    ON {schema}.model_providers(status, updated_at DESC, alias);
                 CREATE INDEX IF NOT EXISTS model_providers_protocol_idx
                    ON {schema}.model_providers(protocol, alias);
                 CREATE TABLE IF NOT EXISTS {schema}.channel_bindings (
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
                 CREATE INDEX IF NOT EXISTS channel_bindings_agent_provider_idx
                    ON {schema}.channel_bindings(agent_id, provider, status);
                 CREATE INDEX IF NOT EXISTS channel_bindings_profile_agent_idx
                    ON {schema}.channel_bindings(profile_id, agent_id, status);
                 CREATE INDEX IF NOT EXISTS channel_bindings_session_idx
                    ON {schema}.channel_bindings(session_id, status);
                 CREATE INDEX IF NOT EXISTS channel_bindings_external_idx
                    ON {schema}.channel_bindings(provider, external_channel_id, external_thread_id);
                 CREATE TABLE IF NOT EXISTS {schema}.mcp_bindings (
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
                 CREATE INDEX IF NOT EXISTS mcp_bindings_agent_profile_idx
                    ON {schema}.mcp_bindings(agent_id, profile_id, status);
                 CREATE INDEX IF NOT EXISTS mcp_bindings_session_idx
                    ON {schema}.mcp_bindings(session_id, status);
                 CREATE INDEX IF NOT EXISTS mcp_bindings_adapter_idx
                    ON {schema}.mcp_bindings(adapter_id, status);
                 CREATE TABLE IF NOT EXISTS {schema}.agent_identities (
                    agent_id TEXT PRIMARY KEY,
                    profile_id TEXT NOT NULL,
                    status TEXT NOT NULL,
                    record_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    archived_at TEXT
                 );
                 CREATE TABLE IF NOT EXISTS {schema}.agent_instances (
                    instance_id TEXT PRIMARY KEY,
                    agent_id TEXT NOT NULL,
                    profile_id TEXT NOT NULL,
                    status TEXT NOT NULL,
                    record_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    last_active_at TEXT NOT NULL,
                    archived_at TEXT
                 );
                 CREATE INDEX IF NOT EXISTS agent_instances_agent_idx
                    ON {schema}.agent_instances(agent_id, status, last_active_at DESC);
                 CREATE TABLE IF NOT EXISTS {schema}.session_identities (
                    session_id TEXT PRIMARY KEY,
                    instance_id TEXT NOT NULL,
                    agent_id TEXT NOT NULL,
                    profile_id TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    status TEXT NOT NULL,
                    record_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    last_active_at TEXT NOT NULL,
                    archived_at TEXT
                 );
                 CREATE TABLE IF NOT EXISTS {schema}.event_history (
                    sequence BIGINT PRIMARY KEY,
                    event_kind TEXT NOT NULL,
                    recorded_at TEXT NOT NULL DEFAULT to_char(
                        CURRENT_TIMESTAMP AT TIME ZONE 'UTC',
                        'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"'
                    ),
                    event_json TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS event_history_kind_idx
                    ON {schema}.event_history(event_kind, sequence);
                 CREATE TABLE IF NOT EXISTS {schema}.event_index (
                    sequence BIGINT NOT NULL REFERENCES {schema}.event_history(sequence) ON DELETE CASCADE,
                    projection TEXT NOT NULL,
                    value TEXT NOT NULL,
                    PRIMARY KEY(sequence, projection, value)
                 );
                 CREATE INDEX IF NOT EXISTS event_index_lookup_idx
                    ON {schema}.event_index(projection, value, sequence);
                 CREATE TABLE IF NOT EXISTS {schema}.queued_messages (
                    message_id TEXT PRIMARY KEY,
                    owner_session_id TEXT,
                    owner_agent_id TEXT NOT NULL,
                    from_agent TEXT NOT NULL,
                    to_agent TEXT NOT NULL,
                    body TEXT NOT NULL,
                    correlation_id TEXT,
                    source_sequence BIGINT,
                    enqueued_at TEXT NOT NULL,
                    expires_at TEXT NOT NULL,
                    ttl_ms BIGINT NOT NULL,
                    delivery_attempts BIGINT NOT NULL,
                    state TEXT NOT NULL,
                    terminal_at TEXT,
                    state_reason TEXT,
                    message_json TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS queued_messages_state_expiry_idx
                    ON {schema}.queued_messages(state, expires_at);
                 CREATE INDEX IF NOT EXISTS queued_messages_owner_agent_idx
                    ON {schema}.queued_messages(owner_agent_id, state, expires_at);
                 CREATE INDEX IF NOT EXISTS queued_messages_owner_session_idx
                    ON {schema}.queued_messages(owner_session_id, state, expires_at);
                 CREATE TABLE IF NOT EXISTS {schema}.scheduled_jobs (
                    job_id TEXT PRIMARY KEY,
                    job_kind TEXT NOT NULL,
                    target_session_id TEXT,
                    interval_ms BIGINT,
                    next_due_at TEXT,
                    payload_json TEXT NOT NULL,
                    status TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    paused_at TEXT
                 );
                 CREATE INDEX IF NOT EXISTS scheduled_jobs_due_idx
                    ON {schema}.scheduled_jobs(status, next_due_at, job_id);
                 CREATE INDEX IF NOT EXISTS scheduled_jobs_kind_due_idx
                    ON {schema}.scheduled_jobs(job_kind, status, next_due_at, job_id);
                 CREATE TABLE IF NOT EXISTS {schema}.scheduled_job_runs (
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
                    updated_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS scheduled_job_runs_job_status_idx
                    ON {schema}.scheduled_job_runs(job_id, status, created_at, run_id);
                 CREATE INDEX IF NOT EXISTS scheduled_job_runs_claim_idx
                    ON {schema}.scheduled_job_runs(status, claim_deadline_at, run_id);
                 CREATE INDEX IF NOT EXISTS scheduled_job_runs_session_idx
                    ON {schema}.scheduled_job_runs(target_session_id, status, created_at, run_id);
                 CREATE TABLE IF NOT EXISTS {schema}.worker_runs (
                    run_id TEXT PRIMARY KEY,
                    session_id TEXT NOT NULL,
                    delegated_session_id TEXT,
                    parent_agent_id TEXT,
                    profile_id TEXT NOT NULL,
                    task_id TEXT,
                    status TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    last_updated_at TEXT NOT NULL,
                    source_wake_id TEXT NOT NULL,
                    source_action_index BIGINT NOT NULL,
                    delegation_correlation_id TEXT,
                    parent_consumption TEXT NOT NULL,
                    fan_out_group_id TEXT,
                    fan_out_max_concurrency BIGINT,
                    fan_out_failure_policy TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS worker_runs_parent_status_created_idx
                    ON {schema}.worker_runs(session_id, status, created_at, run_id);
                 CREATE INDEX IF NOT EXISTS worker_runs_delegated_session_idx
                    ON {schema}.worker_runs(delegated_session_id);
                 CREATE INDEX IF NOT EXISTS worker_runs_profile_task_created_idx
                    ON {schema}.worker_runs(profile_id, task_id, created_at, run_id);
                 CREATE TABLE IF NOT EXISTS {schema}.completion_packets (
                    sequence BIGINT PRIMARY KEY,
                    session_id TEXT NOT NULL,
                    status TEXT NOT NULL,
                    summary TEXT NOT NULL,
                    packet_json TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS completion_packets_session_sequence_idx
                    ON {schema}.completion_packets(session_id, sequence);
                 CREATE TABLE IF NOT EXISTS {schema}.tool_call_history (
                    sequence BIGINT PRIMARY KEY,
                    session_id TEXT NOT NULL,
                    wake_id TEXT,
                    tool_name TEXT NOT NULL,
                    phase TEXT NOT NULL,
                    is_error BOOLEAN,
                    metadata_json TEXT
                 );
                 CREATE INDEX IF NOT EXISTS tool_call_history_session_sequence_idx
                    ON {schema}.tool_call_history(session_id, sequence);
                 CREATE TABLE IF NOT EXISTS {schema}.module_simple_kv_entries (
                    scope_type TEXT NOT NULL,
                    scope_id TEXT NOT NULL,
                    entry_key TEXT NOT NULL,
                    value_json TEXT NOT NULL,
                    revision BIGINT NOT NULL CHECK (revision > 0),
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    expires_at TEXT,
                    PRIMARY KEY(scope_type, scope_id, entry_key)
                 );
                 CREATE INDEX IF NOT EXISTS module_simple_kv_entries_scope_key_idx
                    ON {schema}.module_simple_kv_entries(scope_type, scope_id, entry_key);
                 CREATE INDEX IF NOT EXISTS module_simple_kv_entries_expires_at_idx
                    ON {schema}.module_simple_kv_entries(expires_at);
                 CREATE TABLE IF NOT EXISTS {schema}.session_memory_records (
                    record_id TEXT PRIMARY KEY,
                    session_id TEXT NOT NULL,
                    scope_type TEXT NOT NULL,
                    scope_id TEXT NOT NULL,
                    branch_id TEXT,
                    shape_id TEXT NOT NULL,
                    status TEXT NOT NULL,
                    revision BIGINT NOT NULL CHECK (revision > 0),
                    record_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS session_memory_session_scope_idx
                    ON {schema}.session_memory_records(session_id, scope_type, scope_id, status, updated_at DESC);
                 CREATE INDEX IF NOT EXISTS session_memory_branch_idx
                    ON {schema}.session_memory_records(branch_id, status, updated_at DESC);
                 CREATE INDEX IF NOT EXISTS session_memory_shape_idx
                    ON {schema}.session_memory_records(shape_id, status, updated_at DESC);
                 CREATE TABLE IF NOT EXISTS {schema}.memory_proposals (
                    proposal_id TEXT PRIMARY KEY,
                    space_id TEXT NOT NULL,
                    status TEXT NOT NULL,
                    dedupe_key TEXT,
                    record_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                 );
                 CREATE UNIQUE INDEX IF NOT EXISTS memory_proposals_dedupe_idx
                    ON {schema}.memory_proposals(space_id, dedupe_key)
                    WHERE dedupe_key IS NOT NULL;
                 CREATE INDEX IF NOT EXISTS memory_proposals_status_idx
                    ON {schema}.memory_proposals(space_id, status, updated_at DESC, proposal_id);
                 CREATE TABLE IF NOT EXISTS {schema}.memory_governance_decisions (
                    decision_id TEXT PRIMARY KEY,
                    proposal_id TEXT NOT NULL REFERENCES {schema}.memory_proposals(proposal_id),
                    decision TEXT NOT NULL,
                    record_json TEXT NOT NULL,
                    decided_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS memory_governance_decisions_proposal_idx
                    ON {schema}.memory_governance_decisions(proposal_id, decided_at, decision_id);
                 CREATE TABLE IF NOT EXISTS {schema}.runtime_search_entries (
                    row_type TEXT NOT NULL,
                    row_key TEXT NOT NULL,
                    sequence BIGINT,
                    session_id TEXT,
                    agent_id TEXT,
                    instance_id TEXT,
                    task_id TEXT,
                    event_kind TEXT,
                    recorded_at TEXT NOT NULL,
                    title TEXT NOT NULL,
                    body TEXT NOT NULL,
                    search_vector TSVECTOR GENERATED ALWAYS AS (
                        to_tsvector('simple', coalesce(title, '') || ' ' || coalesce(body, ''))
                    ) STORED,
                    PRIMARY KEY(row_type, row_key)
                 );
                 CREATE INDEX IF NOT EXISTS runtime_search_entries_vector_idx
                    ON {schema}.runtime_search_entries USING GIN(search_vector);
                 CREATE INDEX IF NOT EXISTS runtime_search_entries_metadata_idx
                    ON {schema}.runtime_search_entries(
                        row_type,
                        session_id,
                        agent_id,
                        instance_id,
                        task_id,
                        event_kind,
                        recorded_at
                    );
                 CREATE TABLE IF NOT EXISTS {schema}.provider_wire_states (
                    row_id BIGSERIAL PRIMARY KEY,
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
                 CREATE UNIQUE INDEX IF NOT EXISTS provider_wire_states_current_idx
                    ON {schema}.provider_wire_states(session_id, module_id, strategy_id)
                    WHERE invalidated_at IS NULL;
                 CREATE INDEX IF NOT EXISTS provider_wire_states_session_current_idx
                    ON {schema}.provider_wire_states(session_id, invalidated_at);
                 CREATE INDEX IF NOT EXISTS provider_wire_states_expiry_idx
                    ON {schema}.provider_wire_states(invalidated_at, expires_at);
                 CREATE INDEX IF NOT EXISTS provider_wire_states_updated_idx
                    ON {schema}.provider_wire_states(updated_at DESC, row_id DESC);
                 CREATE TABLE IF NOT EXISTS {schema}.message_slots (
                    slot_id TEXT PRIMARY KEY,
                    session_id TEXT NOT NULL,
                    primary_variant_id TEXT NOT NULL,
                    active_variant_id TEXT,
                    metadata_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    version BIGINT NOT NULL DEFAULT 0
                 );
                 CREATE INDEX IF NOT EXISTS message_slots_session_slot_idx
                    ON {schema}.message_slots(session_id, slot_id);
                 CREATE INDEX IF NOT EXISTS message_slots_active_variant_idx
                    ON {schema}.message_slots(active_variant_id);
                 CREATE TABLE IF NOT EXISTS {schema}.messages (
                    message_id TEXT PRIMARY KEY,
                    session_id TEXT NOT NULL,
                    branch_id TEXT,
                    parent_message_id TEXT,
                    previous_message_id TEXT,
                    author_id TEXT NOT NULL,
                    author_role TEXT NOT NULL,
                    status TEXT NOT NULL,
                    body TEXT NOT NULL,
                    metadata_json TEXT NOT NULL,
                    created_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS messages_session_created_idx
                    ON {schema}.messages(session_id, created_at, message_id);
                 CREATE INDEX IF NOT EXISTS messages_session_branch_idx
                    ON {schema}.messages(session_id, branch_id);
                 CREATE INDEX IF NOT EXISTS messages_parent_message_idx
                    ON {schema}.messages(parent_message_id);
                 CREATE TABLE IF NOT EXISTS {schema}.message_blocks (
                    block_id TEXT PRIMARY KEY,
                    message_id TEXT NOT NULL REFERENCES {schema}.messages(message_id),
                    ordinal BIGINT NOT NULL,
                    kind TEXT NOT NULL,
                    content_json TEXT NOT NULL,
                    render_policy_json TEXT,
                    metadata_json TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS message_blocks_message_ordinal_idx
                    ON {schema}.message_blocks(message_id, ordinal);
                 CREATE TABLE IF NOT EXISTS {schema}.message_variants (
                    variant_id TEXT PRIMARY KEY,
                    slot_id TEXT NOT NULL REFERENCES {schema}.message_slots(slot_id),
                    source TEXT NOT NULL,
                    ordinal BIGINT NOT NULL,
                    status TEXT NOT NULL,
                    message_id TEXT NOT NULL REFERENCES {schema}.messages(message_id),
                    metadata_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                 );
                 CREATE UNIQUE INDEX IF NOT EXISTS message_variants_slot_ordinal_idx
                    ON {schema}.message_variants(slot_id, ordinal);
                 CREATE INDEX IF NOT EXISTS message_variants_slot_status_idx
                    ON {schema}.message_variants(slot_id, status, ordinal);
                 CREATE INDEX IF NOT EXISTS message_variants_message_idx
                    ON {schema}.message_variants(message_id);
                 CREATE TABLE IF NOT EXISTS {schema}.conversation_branches (
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
                    version BIGINT NOT NULL DEFAULT 0
                 );
                 CREATE INDEX IF NOT EXISTS conversation_branches_session_branch_idx
                    ON {schema}.conversation_branches(session_id, branch_id);
                 CREATE INDEX IF NOT EXISTS conversation_branches_parent_branch_idx
                    ON {schema}.conversation_branches(parent_branch_id);
                 CREATE INDEX IF NOT EXISTS conversation_branches_parent_message_idx
                    ON {schema}.conversation_branches(parent_message_id);
                 CREATE INDEX IF NOT EXISTS conversation_branches_session_created_idx
                    ON {schema}.conversation_branches(session_id, created_at, branch_id);
                 CREATE TABLE IF NOT EXISTS {schema}.conversation_branch_state (
                    session_id TEXT PRIMARY KEY,
                    active_branch_id TEXT,
                    updated_at TEXT NOT NULL,
                    version BIGINT NOT NULL DEFAULT 0
                 );
                 CREATE TABLE IF NOT EXISTS {schema}.conversation_snapshots (
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
                 CREATE INDEX IF NOT EXISTS conversation_snapshots_session_message_idx
                    ON {schema}.conversation_snapshots(session_id, message_id);
                 CREATE INDEX IF NOT EXISTS conversation_snapshots_session_branch_idx
                    ON {schema}.conversation_snapshots(session_id, branch_id, created_at);
                 CREATE INDEX IF NOT EXISTS conversation_snapshots_session_created_idx
                    ON {schema}.conversation_snapshots(session_id, created_at, snapshot_id);
                 CREATE TABLE IF NOT EXISTS {schema}.attachments (
                    attachment_id TEXT PRIMARY KEY,
                    session_id TEXT NOT NULL,
                    status TEXT NOT NULL,
                    filename TEXT NOT NULL,
                    mime_type TEXT NOT NULL,
                    byte_size BIGINT NOT NULL,
                    storage_url TEXT,
                    download_url TEXT,
                    thumbnail_url TEXT,
                    extracted_text TEXT,
                    extracted_text_truncated BOOLEAN NOT NULL DEFAULT FALSE,
                    metadata_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    expires_at TEXT
                 );
                 CREATE INDEX IF NOT EXISTS attachments_session_status_idx
                    ON {schema}.attachments(session_id, status, created_at, attachment_id);
                 CREATE INDEX IF NOT EXISTS attachments_expiry_idx
                    ON {schema}.attachments(expires_at);
                 CREATE TABLE IF NOT EXISTS {schema}.attachment_links (
                    link_id TEXT PRIMARY KEY,
                    attachment_id TEXT NOT NULL REFERENCES {schema}.attachments(attachment_id),
                    session_id TEXT NOT NULL,
                    message_id TEXT,
                    block_id TEXT,
                    scope_id TEXT,
                    metadata_json TEXT NOT NULL,
                    created_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS attachment_links_attachment_idx
                    ON {schema}.attachment_links(attachment_id, created_at, link_id);
                 CREATE INDEX IF NOT EXISTS attachment_links_session_message_idx
                    ON {schema}.attachment_links(session_id, message_id);
                 CREATE INDEX IF NOT EXISTS attachment_links_session_block_idx
                    ON {schema}.attachment_links(session_id, block_id);
                 CREATE INDEX IF NOT EXISTS attachment_links_session_scope_idx
                    ON {schema}.attachment_links(session_id, scope_id);
                 CREATE TABLE IF NOT EXISTS {schema}.data_bank_scopes (
                    scope_id TEXT PRIMARY KEY,
                    session_id TEXT NOT NULL,
                    status TEXT NOT NULL,
                    label TEXT,
                    description TEXT,
                    metadata_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS data_bank_scopes_session_status_idx
                    ON {schema}.data_bank_scopes(session_id, status, created_at, scope_id);
                 CREATE TABLE IF NOT EXISTS {schema}.profile_memories (
                    profile_id TEXT NOT NULL,
                    target_type TEXT NOT NULL,
                    target_id TEXT NOT NULL,
                    memory_key TEXT NOT NULL,
                    content TEXT NOT NULL,
                    metadata_json JSONB NOT NULL,
                    revision BIGINT NOT NULL CHECK (revision > 0),
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    PRIMARY KEY(profile_id, target_type, target_id, memory_key)
                 );
                 CREATE INDEX IF NOT EXISTS profile_memories_profile_updated_idx
                    ON {schema}.profile_memories(profile_id, updated_at DESC);
                 CREATE INDEX IF NOT EXISTS profile_memories_target_idx
                    ON {schema}.profile_memories(profile_id, target_type, target_id, memory_key);
                 CREATE TABLE IF NOT EXISTS {schema}.module_roleplay_lore_records (
                    record_id TEXT PRIMARY KEY,
                    world_id TEXT NOT NULL,
                    entity_id TEXT,
                    session_id TEXT,
                    branch_id TEXT,
                    shape_id TEXT NOT NULL,
                    shape_version BIGINT NOT NULL,
                    canon_status TEXT NOT NULL,
                    visibility TEXT NOT NULL,
                    status TEXT NOT NULL,
                    revision BIGINT NOT NULL CHECK (revision > 0),
                    title TEXT NOT NULL,
                    body TEXT NOT NULL,
                    content_json JSONB NOT NULL,
                    evidence_refs_json JSONB NOT NULL,
                    source TEXT NOT NULL,
                    confidence DOUBLE PRECISION NOT NULL,
                    durability_rationale TEXT NOT NULL,
                    supersedes_record_id TEXT,
                    superseded_by_record_id TEXT,
                    tombstoned_at TEXT,
                    tombstone_reason TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    search_vector TSVECTOR GENERATED ALWAYS AS (
                        to_tsvector('simple', coalesce(title, '') || ' ' || coalesce(body, ''))
                    ) STORED
                 );
                 CREATE INDEX IF NOT EXISTS roleplay_lore_world_status_updated_idx
                    ON {schema}.module_roleplay_lore_records(world_id, status, updated_at DESC, record_id);
                 CREATE INDEX IF NOT EXISTS roleplay_lore_entity_idx
                    ON {schema}.module_roleplay_lore_records(world_id, entity_id, canon_status, visibility, updated_at DESC, record_id);
                 CREATE INDEX IF NOT EXISTS roleplay_lore_shape_idx
                    ON {schema}.module_roleplay_lore_records(shape_id, shape_version, updated_at DESC, record_id);
                 CREATE INDEX IF NOT EXISTS roleplay_lore_supersedes_idx
                    ON {schema}.module_roleplay_lore_records(supersedes_record_id)
                    WHERE supersedes_record_id IS NOT NULL;
                 CREATE INDEX IF NOT EXISTS roleplay_lore_search_vector_idx
                    ON {schema}.module_roleplay_lore_records USING GIN(search_vector);
                 CREATE TABLE IF NOT EXISTS {schema}.module_roleplay_lore_provenance_events (
                    event_id TEXT PRIMARY KEY,
                    record_id TEXT NOT NULL REFERENCES {schema}.module_roleplay_lore_records(record_id),
                    world_id TEXT NOT NULL,
                    evidence_refs_json JSONB NOT NULL,
                    source TEXT NOT NULL,
                    actor TEXT NOT NULL,
                    note TEXT,
                    created_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS roleplay_lore_provenance_record_idx
                    ON {schema}.module_roleplay_lore_provenance_events(record_id, created_at, event_id);
                 CREATE INDEX IF NOT EXISTS roleplay_lore_provenance_world_idx
                    ON {schema}.module_roleplay_lore_provenance_events(world_id, created_at, event_id);"
            ))
            .map_err(|error| postgres_error("migrate PostgreSQL runtime counter proof", error))
    }

    fn insert_simple_kv(&self, write: &SimpleKvWrite) -> CoreResult<SimpleKvRecord> {
        let value_json = to_json_text(&write.value_json)?;
        let schema = self.quoted_schema();
        self.client()?
            .execute(
                &format!(
                    "INSERT INTO {schema}.module_simple_kv_entries (
                        scope_type,
                        scope_id,
                        entry_key,
                        value_json,
                        revision,
                        created_at,
                        updated_at,
                        expires_at
                     ) VALUES ($1, $2, $3, $4, 1, $5, $5, $6)"
                ),
                &[
                    &write.scope.scope_type,
                    &write.scope.scope_id,
                    &write.key,
                    &value_json,
                    &write.now,
                    &write.expires_at,
                ],
            )
            .map_err(|error| postgres_error("insert PostgreSQL simple_kv entry", error))?;
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

    fn save_session_in_tx(&self, tx: &mut Transaction<'_>, state: &SessionState) -> CoreResult<()> {
        let schema = self.quoted_schema();
        let state_json = to_json_text(state)?;
        let kind = postgres_session_kind_as_str(&state.kind).to_string();
        let status = postgres_session_status_as_str(&state.status).to_string();
        tx.execute(
            &format!(
                "INSERT INTO {schema}.sessions (
                    session_id,
                    handle,
                    agent_id,
                    profile_id,
                    kind,
                    status,
                    state_json,
                    created_at,
                    last_active_at
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                 ON CONFLICT(session_id) DO UPDATE SET
                    handle = EXCLUDED.handle,
                    agent_id = EXCLUDED.agent_id,
                    profile_id = EXCLUDED.profile_id,
                    kind = EXCLUDED.kind,
                    status = EXCLUDED.status,
                    state_json = EXCLUDED.state_json,
                    last_active_at = EXCLUDED.last_active_at"
            ),
            &[
                &state.session_id.0,
                &(state.handle.get() as i64),
                &state.agent_id.0,
                &state.profile_id.0,
                &kind,
                &status,
                &state_json,
                &state.created_at,
                &state.last_active_at,
            ],
        )
        .map_err(|error| postgres_error("save PostgreSQL session", error))?;
        let (agent, instance, session) = postgres_default_identity_for_session(state);
        self.save_agent_identity_in_tx(tx, &agent)?;
        self.save_agent_instance_in_tx(tx, &instance)?;
        self.save_session_identity_in_tx(tx, &session)?;
        Ok(())
    }

    fn save_session_config_in_tx(
        &self,
        tx: &mut Transaction<'_>,
        config: &SessionConfig,
        created_at: &IsoTimestamp,
    ) -> CoreResult<()> {
        let schema = self.quoted_schema();
        let config_json = to_json_text(config)?;
        let kind = postgres_session_kind_as_str(&config.kind).to_string();
        tx.execute(
            &format!(
                "INSERT INTO {schema}.session_configs (
                    session_id,
                    profile_id,
                    kind,
                    record_json,
                    created_at
                 ) VALUES ($1, $2, $3, $4, $5)
                 ON CONFLICT(session_id) DO NOTHING"
            ),
            &[
                &config.session_id.0,
                &config.profile_id.0,
                &kind,
                &config_json,
                created_at,
            ],
        )
        .map_err(|error| postgres_error("save PostgreSQL session config", error))?;
        Ok(())
    }

    fn save_agent_identity_in_tx(
        &self,
        tx: &mut Transaction<'_>,
        record: &DurableAgentRecord,
    ) -> CoreResult<()> {
        let schema = self.quoted_schema();
        let record_json = to_json_text(record)?;
        let status = postgres_durable_status_as_str(&record.status).to_string();
        tx.execute(
            &format!(
                "INSERT INTO {schema}.agent_identities (
                    agent_id,
                    profile_id,
                    status,
                    record_json,
                    created_at,
                    archived_at
                 ) VALUES ($1, $2, $3, $4, $5, $6)
                 ON CONFLICT(agent_id) DO UPDATE SET
                    profile_id = EXCLUDED.profile_id,
                    status = EXCLUDED.status,
                    record_json = EXCLUDED.record_json,
                    archived_at = EXCLUDED.archived_at"
            ),
            &[
                &record.agent_id.0,
                &record.profile_id.0,
                &status,
                &record_json,
                &record.created_at,
                &record.archived_at,
            ],
        )
        .map_err(|error| postgres_error("save PostgreSQL agent identity", error))?;
        Ok(())
    }

    fn save_agent_instance_in_tx(
        &self,
        tx: &mut Transaction<'_>,
        record: &rusty_crew_core_protocol::AgentInstanceRecord,
    ) -> CoreResult<()> {
        let schema = self.quoted_schema();
        let record_json = to_json_text(record)?;
        let status = postgres_durable_status_as_str(&record.status).to_string();
        tx.execute(
            &format!(
                "INSERT INTO {schema}.agent_instances (
                    instance_id,
                    agent_id,
                    profile_id,
                    status,
                    record_json,
                    created_at,
                    last_active_at,
                    archived_at
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                 ON CONFLICT(instance_id) DO UPDATE SET
                    agent_id = EXCLUDED.agent_id,
                    profile_id = EXCLUDED.profile_id,
                    status = EXCLUDED.status,
                    record_json = EXCLUDED.record_json,
                    last_active_at = EXCLUDED.last_active_at,
                    archived_at = EXCLUDED.archived_at"
            ),
            &[
                &record.instance_id.0,
                &record.agent_id.0,
                &record.profile_id.0,
                &status,
                &record_json,
                &record.created_at,
                &record.last_active_at,
                &record.archived_at,
            ],
        )
        .map_err(|error| postgres_error("save PostgreSQL agent instance", error))?;
        Ok(())
    }

    fn save_session_identity_in_tx(
        &self,
        tx: &mut Transaction<'_>,
        record: &SessionIdentityRecord,
    ) -> CoreResult<()> {
        let schema = self.quoted_schema();
        let record_json = to_json_text(record)?;
        let kind = postgres_session_kind_as_str(&record.kind).to_string();
        let status = postgres_session_status_as_str(&record.status).to_string();
        tx.execute(
            &format!(
                "INSERT INTO {schema}.session_identities (
                    session_id,
                    instance_id,
                    agent_id,
                    profile_id,
                    kind,
                    status,
                    record_json,
                    created_at,
                    last_active_at,
                    archived_at
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                 ON CONFLICT(session_id) DO UPDATE SET
                    instance_id = EXCLUDED.instance_id,
                    agent_id = EXCLUDED.agent_id,
                    profile_id = EXCLUDED.profile_id,
                    kind = EXCLUDED.kind,
                    status = EXCLUDED.status,
                    record_json = EXCLUDED.record_json,
                    last_active_at = EXCLUDED.last_active_at,
                    archived_at = EXCLUDED.archived_at"
            ),
            &[
                &record.session_id.0,
                &record.instance_id.0,
                &record.agent_id.0,
                &record.profile_id.0,
                &kind,
                &status,
                &record_json,
                &record.created_at,
                &record.last_active_at,
                &record.archived_at,
            ],
        )
        .map_err(|error| postgres_error("save PostgreSQL session identity", error))?;
        Ok(())
    }

    fn load_json_records<T>(&self, table: &str, column: &str, order_by: &str) -> CoreResult<Vec<T>>
    where
        T: serde::de::DeserializeOwned,
    {
        validate_postgres_identifier("postgres table", table)?;
        validate_postgres_identifier("postgres column", column)?;
        validate_postgres_identifier("postgres order column", order_by)?;
        let schema = self.quoted_schema();
        let table = quote_postgres_identifier(table);
        let column = quote_postgres_identifier(column);
        let order_by = quote_postgres_identifier(order_by);
        let rows = self
            .client()?
            .query(
                &format!("SELECT {column} FROM {schema}.{table} ORDER BY {order_by} ASC"),
                &[],
            )
            .map_err(|error| postgres_error("load PostgreSQL JSON records", error))?;
        rows.into_iter()
            .map(|row| {
                let record_json: String = row.get(0);
                parse_postgres_json(&record_json, "record_json")
            })
            .collect()
    }

    fn increment_counter_in_tx(
        &self,
        tx: &mut impl GenericClient,
        scope: &RuntimeCounterScope,
        counter_name: &str,
        amount: u64,
    ) -> CoreResult<()> {
        if amount == 0 {
            return Ok(());
        }
        validate_counter_amount(amount)?;
        let (scope_type, scope_id) = runtime_counter_scope_parts(scope);
        let schema = self.quoted_schema();
        let now = postgres_now_iso(tx)?;
        tx.execute(
            &format!(
                "INSERT INTO {schema}.runtime_counters (
                    scope_type,
                    scope_id,
                    counter_name,
                    value,
                    updated_at
                ) VALUES ($1, $2, $3, $4, $5)
                ON CONFLICT(scope_type, scope_id, counter_name) DO UPDATE SET
                    value = runtime_counters.value + EXCLUDED.value,
                    updated_at = EXCLUDED.updated_at"
            ),
            &[
                &scope_type,
                &scope_id,
                &counter_name,
                &(amount as i64),
                &now,
            ],
        )
        .map_err(|error| postgres_error("increment PostgreSQL runtime counter in tx", error))?;
        Ok(())
    }

    fn replace_event_indexes_in_tx(
        &self,
        tx: &mut Transaction<'_>,
        sequence: u64,
        event: &CoreEvent,
    ) -> CoreResult<()> {
        self.replace_event_index_values_in_tx(
            tx,
            sequence,
            "session",
            postgres_event_session_ids(event)
                .into_iter()
                .map(|value| value.0)
                .collect(),
        )?;
        self.replace_event_index_values_in_tx(
            tx,
            sequence,
            "agent",
            postgres_event_agent_ids(event)
                .into_iter()
                .map(|value| value.0)
                .collect(),
        )?;
        self.replace_event_index_values_in_tx(
            tx,
            sequence,
            "instance",
            postgres_event_session_ids(event)
                .into_iter()
                .map(|value| AgentInstanceId::new(format!("instance:{value}")).0)
                .collect(),
        )?;
        self.replace_event_index_values_in_tx(
            tx,
            sequence,
            "correlation",
            postgres_event_correlation_ids(event),
        )?;
        self.replace_event_index_values_in_tx(
            tx,
            sequence,
            "wake",
            postgres_event_source_wake_ids(event),
        )
    }

    fn replace_event_index_values_in_tx(
        &self,
        tx: &mut Transaction<'_>,
        sequence: u64,
        projection: &str,
        values: Vec<String>,
    ) -> CoreResult<()> {
        let schema = self.quoted_schema();
        tx.execute(
            &format!(
                "DELETE FROM {schema}.event_index
                 WHERE sequence = $1 AND projection = $2"
            ),
            &[&(sequence as i64), &projection],
        )
        .map_err(|error| postgres_error("delete PostgreSQL event index values", error))?;
        for value in postgres_dedupe_non_empty(values) {
            tx.execute(
                &format!(
                    "INSERT INTO {schema}.event_index (sequence, projection, value)
                     VALUES ($1, $2, $3)
                     ON CONFLICT DO NOTHING"
                ),
                &[&(sequence as i64), &projection, &value],
            )
            .map_err(|error| postgres_error("insert PostgreSQL event index value", error))?;
        }
        Ok(())
    }

    fn row_to_event_record(&self, row: Row) -> CoreResult<RuntimeEventRecord> {
        let sequence = row.get::<_, i64>(0) as u64;
        let event_json: String = row.get(3);
        let event = parse_postgres_json::<CoreEvent>(&event_json, "event event_json")?;
        Ok(RuntimeEventRecord {
            sequence,
            kind: CoreEventKind::of(&event),
            recorded_at: row.get(2),
            event,
            session_ids: self
                .event_index_values(sequence, "session")?
                .into_iter()
                .map(SessionId)
                .collect(),
            agent_ids: self
                .event_index_values(sequence, "agent")?
                .into_iter()
                .map(AgentId)
                .collect(),
            instance_ids: self
                .event_index_values(sequence, "instance")?
                .into_iter()
                .map(AgentInstanceId)
                .collect(),
            correlation_ids: self.event_index_values(sequence, "correlation")?,
            source_wake_ids: self.event_index_values(sequence, "wake")?,
        })
    }

    fn event_index_values(&self, sequence: u64, projection: &str) -> CoreResult<Vec<String>> {
        let schema = self.quoted_schema();
        let rows = self
            .client()?
            .query(
                &format!(
                    "SELECT value
                     FROM {schema}.event_index
                     WHERE sequence = $1 AND projection = $2
                     ORDER BY value ASC"
                ),
                &[&(sequence as i64), &projection],
            )
            .map_err(|error| postgres_error("load PostgreSQL event index values", error))?;
        Ok(rows.into_iter().map(|row| row.get(0)).collect())
    }

    fn save_queued_message_in_tx(
        &self,
        tx: &mut impl GenericClient,
        record: &QueuedMessageRecord,
    ) -> CoreResult<()> {
        let schema = self.quoted_schema();
        let message_json = to_json_text(&record.message)?;
        let state = postgres_queued_message_state_as_str(record.state).to_string();
        tx.execute(
            &format!(
                "INSERT INTO {schema}.queued_messages (
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
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
                 ON CONFLICT(message_id) DO UPDATE SET
                    owner_session_id = EXCLUDED.owner_session_id,
                    owner_agent_id = EXCLUDED.owner_agent_id,
                    from_agent = EXCLUDED.from_agent,
                    to_agent = EXCLUDED.to_agent,
                    body = EXCLUDED.body,
                    correlation_id = EXCLUDED.correlation_id,
                    source_sequence = EXCLUDED.source_sequence,
                    expires_at = EXCLUDED.expires_at,
                    ttl_ms = EXCLUDED.ttl_ms,
                    delivery_attempts = EXCLUDED.delivery_attempts,
                    state = EXCLUDED.state,
                    terminal_at = EXCLUDED.terminal_at,
                    state_reason = EXCLUDED.state_reason,
                    message_json = EXCLUDED.message_json"
            ),
            &[
                &record.message_id,
                &record
                    .owner_session_id
                    .as_ref()
                    .map(|value| value.0.as_str()),
                &record.owner_agent_id.0,
                &record.message.from.0,
                &record.message.to.0,
                &record.message.body,
                &record.message.correlation_id,
                &record.source_sequence.map(|value| value as i64),
                &record.enqueued_at,
                &record.expires_at,
                &(record.ttl_ms as i64),
                &(record.delivery_attempts as i64),
                &state,
                &record.terminal_at,
                &record.state_reason,
                &message_json,
            ],
        )
        .map_err(|error| postgres_error("save PostgreSQL queued message", error))?;
        Ok(())
    }

    fn load_queued_messages_in_tx(
        &self,
        client: &mut impl GenericClient,
        filter: &QueuedMessageFilter,
    ) -> CoreResult<Vec<QueuedMessageRecord>> {
        let schema = self.quoted_schema();
        let state = filter
            .state
            .map(postgres_queued_message_state_as_str)
            .map(str::to_string);
        let owner_session_id = filter
            .owner_session_id
            .as_ref()
            .map(|value| value.0.as_str());
        let owner_agent_id = filter.owner_agent_id.as_ref().map(|value| value.0.as_str());
        let limit = filter.limit.unwrap_or(1_000).clamp(1, 10_000) as i64;
        let rows = client
            .query(
                &format!(
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
                     FROM {schema}.queued_messages
                     WHERE ($1::TEXT IS NULL OR state = $1)
                       AND ($2::TEXT IS NULL OR owner_session_id = $2)
                       AND ($3::TEXT IS NULL OR owner_agent_id = $3)
                     ORDER BY enqueued_at ASC, message_id ASC
                     LIMIT $4"
                ),
                &[&state, &owner_session_id, &owner_agent_id, &limit],
            )
            .map_err(|error| postgres_error("load PostgreSQL queued messages", error))?;
        rows.into_iter().map(row_to_queued_message).collect()
    }

    fn expire_queued_messages_in_tx(
        &self,
        tx: &mut impl GenericClient,
        now: &IsoTimestamp,
    ) -> CoreResult<Vec<QueuedMessageRecord>> {
        let expiring = self
            .load_queued_messages_in_tx(
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
        let mut expired = Vec::new();
        for mut message in expiring {
            message.state = QueuedMessageState::Expired;
            message.terminal_at = Some(now.clone());
            message.state_reason = Some("ttl_expired".to_string());
            self.save_queued_message_in_tx(tx, &message)?;
            for scope in postgres_queued_message_counter_scopes(&message) {
                self.increment_counter_in_tx(tx, &scope, COUNTER_QUEUE_EXPIRATIONS, 1)?;
            }
            expired.push(message);
        }
        Ok(expired)
    }

    fn update_simple_kv(&self, write: &SimpleKvWrite, revision: u64) -> CoreResult<SimpleKvRecord> {
        validate_counter_amount(revision)?;
        let existing = self.get_simple_kv(&write.scope, &write.key, None)?;
        let created_at = existing
            .as_ref()
            .map(|record| record.created_at.clone())
            .unwrap_or_else(|| write.now.clone());
        let value_json = to_json_text(&write.value_json)?;
        let schema = self.quoted_schema();
        self.client()?
            .execute(
                &format!(
                    "UPDATE {schema}.module_simple_kv_entries
                     SET value_json = $4,
                         revision = $5,
                         updated_at = $6,
                         expires_at = $7
                     WHERE scope_type = $1
                       AND scope_id = $2
                       AND entry_key = $3"
                ),
                &[
                    &write.scope.scope_type,
                    &write.scope.scope_id,
                    &write.key,
                    &value_json,
                    &(revision as i64),
                    &write.now,
                    &write.expires_at,
                ],
            )
            .map_err(|error| postgres_error("update PostgreSQL simple_kv entry", error))?;
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

    fn schema_version(&self) -> CoreResult<i64> {
        let schema = self.quoted_schema();
        let row = self
            .client()?
            .query_one(
                &format!(
                    "SELECT metadata_value
                     FROM {schema}.rusty_crew_storage_metadata
                     WHERE metadata_key = 'runtime_counter_proof_schema_version'"
                ),
                &[],
            )
            .map_err(|error| postgres_error("load PostgreSQL proof schema version", error))?;
        let raw: String = row.get(0);
        raw.parse::<i64>().map_err(|error| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("parse PostgreSQL proof schema version: {error}"),
            )
        })
    }

    fn runtime_counter_rows(&self) -> CoreResult<u64> {
        let schema = self.quoted_schema();
        let row = self
            .client()?
            .query_one(
                &format!("SELECT COUNT(*) FROM {schema}.runtime_counters"),
                &[],
            )
            .map_err(|error| postgres_error("count PostgreSQL runtime counters", error))?;
        let rows: i64 = row.get(0);
        Ok(rows as u64)
    }

    fn simple_kv_rows(&self) -> CoreResult<u64> {
        let schema = self.quoted_schema();
        let row = self
            .client()?
            .query_one(
                &format!("SELECT COUNT(*) FROM {schema}.module_simple_kv_entries"),
                &[],
            )
            .map_err(|error| postgres_error("count PostgreSQL simple_kv entries", error))?;
        let rows: i64 = row.get(0);
        Ok(rows as u64)
    }

    fn runtime_search_rows(&self) -> CoreResult<u64> {
        let schema = self.quoted_schema();
        let row = self
            .client()?
            .query_one(
                &format!("SELECT COUNT(*) FROM {schema}.runtime_search_entries"),
                &[],
            )
            .map_err(|error| postgres_error("count PostgreSQL runtime search entries", error))?;
        let rows: i64 = row.get(0);
        Ok(rows as u64)
    }

    fn provider_wire_state_rows(&self) -> CoreResult<u64> {
        self.table_rows("provider_wire_states")
    }

    fn table_rows(&self, table: &str) -> CoreResult<u64> {
        validate_postgres_identifier("postgres table", table)?;
        let schema = self.quoted_schema();
        let table = quote_postgres_identifier(table);
        let row = self
            .client()?
            .query_one(&format!("SELECT COUNT(*) FROM {schema}.{table}"), &[])
            .map_err(|error| postgres_error("count PostgreSQL proof table rows", error))?;
        let rows: i64 = row.get(0);
        Ok(rows as u64)
    }

    fn quoted_schema(&self) -> String {
        quote_postgres_identifier(&self.schema)
    }

    fn client(&self) -> CoreResult<MutexGuard<'_, Client>> {
        self.client.lock().map_err(|_| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "PostgreSQL proof connection mutex poisoned",
            )
        })
    }
}

fn postgres_proof_capabilities() -> Vec<RuntimeStorageCapability> {
    [
        (
            "transactions",
            true,
            "PostgreSQL transactions are available for the proof repository",
        ),
        (
            "json_metadata",
            true,
            "PostgreSQL can store JSON metadata, though runtime counters do not need it",
        ),
        (
            "concurrent_writers",
            true,
            "PostgreSQL supports concurrent writers for this proof repository",
        ),
        (
            "estimated_table_size",
            true,
            "the proof slice exposes row counts for proof-owned tables",
        ),
        (
            "row_level_claims",
            true,
            "PostgreSQL scheduler expiry uses FOR UPDATE SKIP LOCKED row-level claim semantics",
        ),
        (
            "runtime_full_text_search",
            true,
            "PostgreSQL runtime search proof uses tsvector behind the typed RuntimeSearchFilter API",
        ),
        (
            "logical_export_import",
            false,
            "logical cross-backend export/import remains future work",
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

fn postgres_proof_repository_groups() -> Vec<RuntimeRepositoryGroupDiagnostic> {
    repositories::core_repository_group_diagnostics()
        .into_iter()
        .map(|mut group| {
            if group.group_id == "storage_admin" {
                group.notes.insert(
                    0,
                    "PostgreSQL proof status: implemented for proof-owned migrations and storage diagnostics only.".to_string(),
                );
            } else if group.group_id == "sessions_identities" {
                group.notes.insert(
                    0,
                    "PostgreSQL proof status: implemented for session/config/identity hydration conformance; not yet wired as the full service backend.".to_string(),
                );
            } else if group.group_id == "events_projections" {
                group.notes.insert(
                    0,
                    "PostgreSQL proof status: implemented for event history and typed event-index query conformance; not yet wired as the full service backend.".to_string(),
                );
            } else if group.group_id == "queues_messages" {
                group.notes.insert(
                    0,
                    "PostgreSQL proof status: implemented for queued-message TTL and no-resurrection conformance; not yet wired as the full service backend.".to_string(),
                );
            } else if group.group_id == "scheduler_jobs" {
                group.notes.insert(
                    0,
                    "PostgreSQL proof status: implemented for scheduled jobs, scheduled run claim/completion, stale-run expiry, and row-level claim conformance; not yet wired as the full service backend.".to_string(),
                );
            } else if group.group_id == "worker_runs_completions" {
                group.notes.insert(
                    0,
                    "PostgreSQL proof status: implemented for worker run lifecycle, delegated completion lookup, completion packet persistence, and terminal-status conformance; not yet wired as the full service backend.".to_string(),
                );
            } else if group.group_id == "module_schema_registry" {
                group.notes.insert(
                    0,
                    "PostgreSQL proof status: implemented only for the simple_kv module-owned data table, not the full module registry.".to_string(),
                );
            } else if group.group_id == "runtime_counters" {
                group.notes.insert(
                    0,
                    "PostgreSQL proof status: implemented for the runtime-counter proof repository.".to_string(),
                );
            } else if group.group_id == "runtime_search" {
                group.notes.insert(
                    0,
                    "PostgreSQL proof status: implemented for runtime search entries through the typed search API; not yet wired as the full service backend.".to_string(),
                );
            } else if group.group_id == "provider_state" {
                group.notes.insert(
                    0,
                    "PostgreSQL proof status: implemented for provider wire-state conformance through the typed provider-state API; not yet wired as the full service backend.".to_string(),
                );
            } else if group.group_id == "conversations_attachments" {
                group.notes.insert(
                    0,
                    "PostgreSQL proof status: implemented for the conversation transcript proof surface and attachment/data-bank proof surface; not yet wired as the full service backend.".to_string(),
                );
            } else if group.group_id == "profile_memory" {
                group.notes.insert(
                    0,
                    "PostgreSQL proof status: implemented for profile_dense descriptor projection and dense profile memory conformance; not yet wired as the full service backend.".to_string(),
                );
            } else {
                group.notes.insert(
                    0,
                    "PostgreSQL proof status: unsupported; full service boot must fail closed before using this repository group.".to_string(),
                );
            }
            group
        })
        .collect()
}

fn postgres_session_kind_as_str(kind: &SessionKind) -> &'static str {
    match kind {
        SessionKind::Full => "full",
        SessionKind::Worker => "worker",
        SessionKind::Delegated => "delegated",
    }
}

fn postgres_session_status_as_str(status: &SessionStatus) -> &'static str {
    match status {
        SessionStatus::Active => "active",
        SessionStatus::Idle => "idle",
        SessionStatus::Archived => "archived",
    }
}

fn postgres_durable_status_as_str(status: &DurableIdentityStatus) -> &'static str {
    match status {
        DurableIdentityStatus::Active => "active",
        DurableIdentityStatus::Archived => "archived",
    }
}

fn postgres_durable_agent_kind_from_session_kind(kind: &SessionKind) -> DurableAgentKind {
    match kind {
        SessionKind::Full => DurableAgentKind::Full,
        SessionKind::Worker => DurableAgentKind::WorkerPoolWorker,
        SessionKind::Delegated => DurableAgentKind::Delegated,
    }
}

fn postgres_durable_status_from_session_status(status: &SessionStatus) -> DurableIdentityStatus {
    match status {
        SessionStatus::Active | SessionStatus::Idle => DurableIdentityStatus::Active,
        SessionStatus::Archived => DurableIdentityStatus::Archived,
    }
}

fn postgres_default_identity_for_session(
    state: &SessionState,
) -> (
    DurableAgentRecord,
    rusty_crew_core_protocol::AgentInstanceRecord,
    SessionIdentityRecord,
) {
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
    let status = postgres_durable_status_from_session_status(&state.status);
    let instance_id = AgentInstanceId::new(format!("instance:{}", state.session_id));
    (
        DurableAgentRecord {
            agent_id: state.agent_id.clone(),
            display_label: state.agent_id.to_string(),
            profile_id: state.profile_id.clone(),
            kind: postgres_durable_agent_kind_from_session_kind(&state.kind),
            status: status.clone(),
            source: None,
            den: den.clone(),
            created_at: state.created_at.clone(),
            archived_at: archived_at.clone(),
        },
        rusty_crew_core_protocol::AgentInstanceRecord {
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
        SessionIdentityRecord {
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

fn postgres_should_persist_event(event: &CoreEvent) -> bool {
    !matches!(
        event,
        CoreEvent::DenDataUpdated { .. } | CoreEvent::ExternalEventInjected { .. }
    )
}

fn postgres_event_session_ids(event: &CoreEvent) -> Vec<SessionId> {
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

fn postgres_event_agent_ids(event: &CoreEvent) -> Vec<AgentId> {
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

fn postgres_event_correlation_ids(event: &CoreEvent) -> Vec<String> {
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

fn postgres_event_source_wake_ids(event: &CoreEvent) -> Vec<String> {
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

fn postgres_event_counter_deltas(event: &CoreEvent) -> Vec<(&'static str, u64)> {
    match event {
        CoreEvent::BrainWakeRequested { .. } => vec![(COUNTER_WAKES, 1)],
        CoreEvent::AgentMessageRouted { .. } => vec![(COUNTER_MESSAGES, 1)],
        CoreEvent::BrainActionsAccepted { count, .. } => {
            vec![
                (COUNTER_BRAIN_TURNS, 1),
                ("accepted_actions", u64::from(*count)),
            ]
        }
        CoreEvent::BrainEventObserved { event, .. } => match event {
            BrainEvent::ToolCallStarted { .. } => vec![(COUNTER_TOOL_CALLS, 1)],
            BrainEvent::ToolCallFinished { is_error: true, .. } => {
                vec![(COUNTER_TOOL_ERRORS, 1)]
            }
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
        CoreEvent::SessionArchived { .. }
        | CoreEvent::SessionCreated { .. }
        | CoreEvent::ExternalEventInjected { .. }
        | CoreEvent::DenDataUpdated { .. } => Vec::new(),
    }
}

fn postgres_event_counter_scopes(event: &CoreEvent) -> Vec<RuntimeCounterScope> {
    let mut scopes = vec![RuntimeCounterScope::Runtime];
    for agent_id in postgres_event_agent_ids(event) {
        scopes.push(RuntimeCounterScope::Agent(agent_id));
    }
    let session_ids = postgres_event_session_ids(event);
    for session_id in &session_ids {
        scopes.push(RuntimeCounterScope::Session(session_id.clone()));
    }
    for session_id in session_ids {
        scopes.push(RuntimeCounterScope::Instance(AgentInstanceId::new(
            format!("instance:{session_id}"),
        )));
    }
    scopes
}

fn postgres_queued_message_state_as_str(state: QueuedMessageState) -> &'static str {
    match state {
        QueuedMessageState::Pending => "pending",
        QueuedMessageState::Delivered => "delivered",
        QueuedMessageState::Expired => "expired",
        QueuedMessageState::Discarded => "discarded",
        QueuedMessageState::Cancelled => "cancelled",
    }
}

fn postgres_queued_message_state_from_str(raw: &str) -> CoreResult<QueuedMessageState> {
    match raw {
        "pending" => Ok(QueuedMessageState::Pending),
        "delivered" => Ok(QueuedMessageState::Delivered),
        "expired" => Ok(QueuedMessageState::Expired),
        "discarded" => Ok(QueuedMessageState::Discarded),
        "cancelled" => Ok(QueuedMessageState::Cancelled),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("unknown PostgreSQL queued message state {other}"),
        )),
    }
}

fn postgres_queued_message_counter_scopes(
    message: &QueuedMessageRecord,
) -> Vec<RuntimeCounterScope> {
    let mut scopes = vec![RuntimeCounterScope::Runtime];
    if let Some(session_id) = &message.owner_session_id {
        scopes.push(RuntimeCounterScope::Session(session_id.clone()));
    }
    scopes
}

fn postgres_dedupe_non_empty(values: Vec<String>) -> Vec<String> {
    let mut deduped = Vec::new();
    for value in values {
        if value.trim().is_empty() || deduped.contains(&value) {
            continue;
        }
        deduped.push(value);
    }
    deduped
}

fn postgres_now_iso(client: &mut impl GenericClient) -> CoreResult<IsoTimestamp> {
    let row = client
        .query_one(
            "SELECT to_char(
                CURRENT_TIMESTAMP AT TIME ZONE 'UTC',
                'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"'
             )",
            &[],
        )
        .map_err(|error| postgres_error("read PostgreSQL current timestamp", error))?;
    Ok(row.get(0))
}

fn row_to_queued_message(row: Row) -> CoreResult<QueuedMessageRecord> {
    let message_json: String = row.get(15);
    let state: String = row.get(12);
    Ok(QueuedMessageRecord {
        message_id: row.get(0),
        owner_session_id: row.get::<_, Option<String>>(1).map(SessionId),
        owner_agent_id: AgentId(row.get(2)),
        message: parse_postgres_json(&message_json, "queued message_json")?,
        source_sequence: row.get::<_, Option<i64>>(7).map(|value| value as u64),
        enqueued_at: row.get(8),
        expires_at: row.get(9),
        ttl_ms: row.get::<_, i64>(10) as u32,
        delivery_attempts: row.get::<_, i64>(11) as u32,
        state: postgres_queued_message_state_from_str(&state)?,
        terminal_at: row.get(13),
        state_reason: row.get(14),
    })
}

const WORKER_RUN_SELECT: &str = "SELECT
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
    fan_out_failure_policy";

fn save_completion_packet_in_tx(
    tx: &mut impl GenericClient,
    schema: &str,
    sequence: u64,
    packet: &CompletionPacket,
) -> CoreResult<()> {
    let status = postgres_completion_status_as_str(&packet.status).to_string();
    let packet_json = to_json_text(packet)?;
    tx.execute(
        &format!(
            "INSERT INTO {schema}.completion_packets (
                sequence,
                session_id,
                status,
                summary,
                packet_json
             ) VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT(sequence) DO UPDATE SET
                session_id = EXCLUDED.session_id,
                status = EXCLUDED.status,
                summary = EXCLUDED.summary,
                packet_json = EXCLUDED.packet_json"
        ),
        &[
            &(sequence as i64),
            &packet.session_id.0,
            &status,
            &packet.summary,
            &packet_json,
        ],
    )
    .map_err(|error| postgres_error("save PostgreSQL completion packet", error))?;
    Ok(())
}

fn save_tool_call_in_tx(
    tx: &mut impl GenericClient,
    schema: &str,
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
    let phase = tool_call_phase_as_str(phase).to_string();
    let metadata_json = metadata.as_ref().map(to_json_text).transpose()?;
    tx.execute(
        &format!(
            "INSERT INTO {schema}.tool_call_history (
                sequence,
                session_id,
                wake_id,
                tool_name,
                phase,
                is_error,
                metadata_json
             ) VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT(sequence) DO UPDATE SET
                session_id = EXCLUDED.session_id,
                wake_id = EXCLUDED.wake_id,
                tool_name = EXCLUDED.tool_name,
                phase = EXCLUDED.phase,
                is_error = EXCLUDED.is_error,
                metadata_json = EXCLUDED.metadata_json"
        ),
        &[
            &(sequence as i64),
            &session_id.0,
            &wake_id,
            tool_name,
            &phase,
            &is_error,
            &metadata_json,
        ],
    )
    .map_err(|error| postgres_error("save PostgreSQL tool call history", error))?;
    Ok(())
}

fn row_to_tool_call_record(row: &Row) -> CoreResult<ToolCallRecord> {
    let sequence: i64 = row.get(0);
    let phase: String = row.get(4);
    let metadata_json: Option<String> = row.get(6);
    Ok(ToolCallRecord {
        sequence: sequence as u64,
        session_id: SessionId::new(row.get::<_, String>(1)),
        wake_id: row.get(2),
        tool_name: row.get(3),
        phase: tool_call_phase_from_str(&phase)?,
        is_error: row.get(5),
        metadata: metadata_json
            .as_deref()
            .map(|value| parse_postgres_json(value, "tool call metadata_json"))
            .transpose()?,
    })
}

fn tool_call_phase_as_str(phase: ToolCallPhase) -> &'static str {
    match phase {
        ToolCallPhase::Started => "started",
        ToolCallPhase::Finished => "finished",
    }
}

fn tool_call_phase_from_str(raw: &str) -> CoreResult<ToolCallPhase> {
    match raw {
        "started" => Ok(ToolCallPhase::Started),
        "finished" => Ok(ToolCallPhase::Finished),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("unknown PostgreSQL tool call phase {other}"),
        )),
    }
}

fn purge_terminal_queued_messages_in_tx(
    tx: &mut impl GenericClient,
    schema: &str,
    cutoff: &IsoTimestamp,
) -> CoreResult<u64> {
    tx.execute(
        &format!(
            "DELETE FROM {schema}.queued_messages
             WHERE state IN ('delivered', 'expired', 'discarded', 'cancelled')
               AND terminal_at IS NOT NULL
               AND terminal_at < $1"
        ),
        &[cutoff],
    )
    .map_err(|error| postgres_error("purge PostgreSQL terminal queued messages", error))
}

fn row_to_completion_packet_record(row: &Row) -> CoreResult<CompletionPacketRecord> {
    let sequence: i64 = row.get(0);
    let packet_json: String = row.get(1);
    Ok(CompletionPacketRecord {
        sequence: sequence as u64,
        packet: parse_postgres_json(&packet_json, "completion packet_json")?,
    })
}

fn save_worker_run_in_client(
    client: &mut impl GenericClient,
    schema: &str,
    record: &WorkerRunRecord,
) -> CoreResult<()> {
    let status = worker_run_status_as_str(&record.status).to_string();
    let parent_consumption = parent_consumption_policy_as_str(&record.parent_consumption);
    let fan_out_failure_policy = fan_out_failure_policy_as_str(&record.fan_out_failure_policy);
    client
        .execute(
            &format!(
                "INSERT INTO {schema}.worker_runs (
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
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
                 ON CONFLICT(run_id) DO UPDATE SET
                    session_id = EXCLUDED.session_id,
                    delegated_session_id = EXCLUDED.delegated_session_id,
                    parent_agent_id = EXCLUDED.parent_agent_id,
                    profile_id = EXCLUDED.profile_id,
                    task_id = EXCLUDED.task_id,
                    status = EXCLUDED.status,
                    last_updated_at = EXCLUDED.last_updated_at,
                    source_wake_id = EXCLUDED.source_wake_id,
                    source_action_index = EXCLUDED.source_action_index,
                    delegation_correlation_id = EXCLUDED.delegation_correlation_id,
                    parent_consumption = EXCLUDED.parent_consumption,
                    fan_out_group_id = EXCLUDED.fan_out_group_id,
                    fan_out_max_concurrency = EXCLUDED.fan_out_max_concurrency,
                    fan_out_failure_policy = EXCLUDED.fan_out_failure_policy"
            ),
            &[
                &record.run_id.0,
                &record.parent_session_id.0,
                &record
                    .delegated_session_id
                    .as_ref()
                    .map(|value| value.0.as_str()),
                &record
                    .parent_agent_id
                    .as_ref()
                    .map(|value| value.0.as_str()),
                &record.profile_id.0,
                &record.task_id.as_ref().map(|value| value.0.as_str()),
                &status,
                &record.created_at,
                &record.last_updated_at,
                &record.source_wake_id,
                &(record.source_action_index as i64),
                &record.delegation_correlation_id,
                &parent_consumption,
                &record.fan_out_group_id,
                &record.fan_out_max_concurrency.map(|value| value as i64),
                &fan_out_failure_policy,
            ],
        )
        .map_err(|error| postgres_error("save PostgreSQL worker run", error))?;
    Ok(())
}

fn row_to_worker_run(row: &Row) -> CoreResult<WorkerRunRecord> {
    let status: String = row.get(6);
    let parent_consumption: String = row.get(12);
    let fan_out_failure_policy: String = row.get(15);
    Ok(WorkerRunRecord {
        run_id: RunId::new(row.get::<_, String>(0)),
        parent_session_id: SessionId::new(row.get::<_, String>(1)),
        delegated_session_id: row.get::<_, Option<String>>(2).map(SessionId::new),
        parent_agent_id: row.get::<_, Option<String>>(3).map(AgentId::new),
        profile_id: ProfileId::new(row.get::<_, String>(4)),
        task_id: row.get::<_, Option<String>>(5).map(TaskId::new),
        status: worker_run_status_from_str(&status)?,
        created_at: row.get(7),
        last_updated_at: row.get(8),
        source_wake_id: row.get(9),
        source_action_index: row.get::<_, i64>(10) as u32,
        delegation_correlation_id: row.get(11),
        parent_consumption: parent_consumption_policy_from_str(&parent_consumption)?,
        fan_out_group_id: row.get(13),
        fan_out_max_concurrency: row.get::<_, Option<i64>>(14).map(|value| value as u32),
        fan_out_failure_policy: fan_out_failure_policy_from_str(&fan_out_failure_policy)?,
    })
}

fn row_to_delegated_completion(row: &Row) -> CoreResult<DelegatedCompletion> {
    let child_session_id = row.get::<_, Option<String>>(1).ok_or_else(|| {
        CoreError::new(
            CoreErrorKind::PersistenceFailure,
            "delegated completion row is missing delegated_session_id",
        )
    })?;
    let parent_consumption: String = row.get(6);
    let packet_json: String = row.get(7);
    Ok(DelegatedCompletion {
        run_id: RunId::new(row.get::<_, String>(0)),
        child_session_id: SessionId::new(child_session_id),
        requested_task_id: row.get::<_, Option<String>>(2).map(TaskId::new),
        source_wake_id: row.get(3),
        source_action_index: row.get::<_, i64>(4) as u32,
        correlation_id: row.get(5),
        parent_consumption: parent_consumption_policy_from_str(&parent_consumption)?,
        packet: parse_postgres_json(&packet_json, "delegated completion packet_json")?,
    })
}

fn postgres_completion_status_as_str(status: &CompletionStatus) -> &'static str {
    match status {
        CompletionStatus::Completed => "completed",
        CompletionStatus::Failed => "failed",
        CompletionStatus::Blocked => "blocked",
        CompletionStatus::Exhausted => "exhausted",
    }
}

fn worker_run_status_as_str(status: &WorkerRunStatus) -> &'static str {
    match status {
        WorkerRunStatus::Requested => "requested",
        WorkerRunStatus::SessionCreated => "session_created",
        WorkerRunStatus::WakeRequested => "wake_requested",
        WorkerRunStatus::Running => "running",
        WorkerRunStatus::CheckpointWaiting => "checkpoint_waiting",
        WorkerRunStatus::Completed => "completed",
        WorkerRunStatus::Failed => "failed",
        WorkerRunStatus::Blocked => "blocked",
        WorkerRunStatus::Exhausted => "exhausted",
        WorkerRunStatus::Cancelled => "cancelled",
        WorkerRunStatus::Expired => "expired",
    }
}

fn worker_run_status_from_str(raw: &str) -> CoreResult<WorkerRunStatus> {
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
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("unknown PostgreSQL worker run status {other}"),
        )),
    }
}

fn parent_consumption_policy_as_str(policy: &ParentConsumptionPolicy) -> &'static str {
    match policy {
        ParentConsumptionPolicy::AwaitCompletion => "await_completion",
        ParentConsumptionPolicy::ObserveOnly => "observe_only",
    }
}

fn parent_consumption_policy_from_str(raw: &str) -> CoreResult<ParentConsumptionPolicy> {
    match raw {
        "await_completion" => Ok(ParentConsumptionPolicy::AwaitCompletion),
        "observe_only" => Ok(ParentConsumptionPolicy::ObserveOnly),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("unknown PostgreSQL parent consumption policy {other}"),
        )),
    }
}

fn fan_out_failure_policy_as_str(policy: &FanOutFailurePolicy) -> &'static str {
    match policy {
        FanOutFailurePolicy::FailFast => "fail_fast",
        FanOutFailurePolicy::FailSoft => "fail_soft",
    }
}

fn fan_out_failure_policy_from_str(raw: &str) -> CoreResult<FanOutFailurePolicy> {
    match raw {
        "fail_fast" => Ok(FanOutFailurePolicy::FailFast),
        "fail_soft" => Ok(FanOutFailurePolicy::FailSoft),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("unknown PostgreSQL fan-out failure policy {other}"),
        )),
    }
}

fn save_scheduled_run_in_tx(
    tx: &mut impl GenericClient,
    schema: &str,
    run: &ScheduledRunRecord,
) -> CoreResult<()> {
    let status = postgres_scheduled_run_status_as_str(run.status).to_string();
    let trigger = postgres_scheduled_run_trigger_as_str(run.trigger).to_string();
    let output_json = to_json_text(&run.output_json)?;
    tx.execute(
        &format!(
            "INSERT INTO {schema}.scheduled_job_runs (
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
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)"
        ),
        &[
            &run.run_id.0,
            &run.job_id,
            &run.job_kind,
            &run.target_session_id.as_ref().map(|value| value.0.as_str()),
            &status,
            &trigger,
            &run.scheduled_for,
            &run.claimed_at,
            &run.claim_deadline_at,
            &run.completed_at,
            &run.error,
            &output_json,
            &run.created_at,
            &run.updated_at,
        ],
    )
    .map_err(|error| postgres_error("save PostgreSQL scheduled run", error))?;
    Ok(())
}

fn row_to_scheduled_job(row: &Row) -> CoreResult<ScheduledJobRecord> {
    let interval_ms = row.get::<_, Option<i64>>(3).map(|value| value as u64);
    let payload_json: String = row.get(5);
    let status: String = row.get(6);
    Ok(ScheduledJobRecord {
        job_id: row.get(0),
        job_kind: row.get(1),
        target_session_id: row.get::<_, Option<String>>(2).map(SessionId),
        interval_ms,
        next_due_at: row.get(4),
        payload_json: parse_postgres_json(&payload_json, "scheduled job payload_json")?,
        status: postgres_scheduled_job_status_from_str(&status)?,
        created_at: row.get(7),
        updated_at: row.get(8),
        paused_at: row.get(9),
    })
}

fn row_to_scheduled_run(row: &Row) -> CoreResult<ScheduledRunRecord> {
    let status: String = row.get(4);
    let trigger: String = row.get(5);
    let output_json: String = row.get(11);
    Ok(ScheduledRunRecord {
        run_id: RunId::new(row.get::<_, String>(0)),
        job_id: row.get(1),
        job_kind: row.get(2),
        target_session_id: row.get::<_, Option<String>>(3).map(SessionId),
        status: postgres_scheduled_run_status_from_str(&status)?,
        trigger: postgres_scheduled_run_trigger_from_str(&trigger)?,
        scheduled_for: row.get(6),
        claimed_at: row.get(7),
        claim_deadline_at: row.get(8),
        completed_at: row.get(9),
        error: row.get(10),
        output_json: parse_postgres_json(&output_json, "scheduled run output_json")?,
        created_at: row.get(12),
        updated_at: row.get(13),
    })
}

fn postgres_scheduled_job_status_as_str(status: ScheduledJobStatus) -> &'static str {
    match status {
        ScheduledJobStatus::Active => "active",
        ScheduledJobStatus::Paused => "paused",
        ScheduledJobStatus::Archived => "archived",
    }
}

fn postgres_scheduled_job_status_from_str(raw: &str) -> CoreResult<ScheduledJobStatus> {
    match raw {
        "active" => Ok(ScheduledJobStatus::Active),
        "paused" => Ok(ScheduledJobStatus::Paused),
        "archived" => Ok(ScheduledJobStatus::Archived),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("unknown PostgreSQL scheduled job status {other}"),
        )),
    }
}

fn postgres_scheduled_run_status_as_str(status: ScheduledRunStatus) -> &'static str {
    match status {
        ScheduledRunStatus::Claimed => "claimed",
        ScheduledRunStatus::Completed => "completed",
        ScheduledRunStatus::Skipped => "skipped",
        ScheduledRunStatus::Failed => "failed",
        ScheduledRunStatus::Expired => "expired",
        ScheduledRunStatus::Cancelled => "cancelled",
    }
}

fn postgres_scheduled_run_status_from_str(raw: &str) -> CoreResult<ScheduledRunStatus> {
    match raw {
        "claimed" => Ok(ScheduledRunStatus::Claimed),
        "completed" => Ok(ScheduledRunStatus::Completed),
        "skipped" => Ok(ScheduledRunStatus::Skipped),
        "failed" => Ok(ScheduledRunStatus::Failed),
        "expired" => Ok(ScheduledRunStatus::Expired),
        "cancelled" => Ok(ScheduledRunStatus::Cancelled),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("unknown PostgreSQL scheduled run status {other}"),
        )),
    }
}

fn postgres_scheduled_run_trigger_as_str(trigger: ScheduledRunTrigger) -> &'static str {
    match trigger {
        ScheduledRunTrigger::Due => "due",
        ScheduledRunTrigger::Manual => "manual",
    }
}

fn postgres_scheduled_run_trigger_from_str(raw: &str) -> CoreResult<ScheduledRunTrigger> {
    match raw {
        "due" => Ok(ScheduledRunTrigger::Due),
        "manual" => Ok(ScheduledRunTrigger::Manual),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("unknown PostgreSQL scheduled run trigger {other}"),
        )),
    }
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
) -> CoreResult<RuntimeCounterScope> {
    match scope_type {
        "runtime" if scope_id == "_global" => Ok(RuntimeCounterScope::Runtime),
        "agent" => Ok(RuntimeCounterScope::Agent(crate::AgentId::new(scope_id))),
        "instance" => Ok(RuntimeCounterScope::Instance(crate::AgentInstanceId::new(
            scope_id,
        ))),
        "session" => Ok(RuntimeCounterScope::Session(crate::SessionId::new(
            scope_id,
        ))),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("unknown runtime counter scope {other}:{scope_id}"),
        )),
    }
}

fn row_to_runtime_counter(row: &Row) -> CoreResult<RuntimeCounterRecord> {
    let scope_type: String = row.get(0);
    let scope_id: String = row.get(1);
    let value: i64 = row.get(3);
    if value < 0 {
        return Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("runtime counter value must not be negative: {value}"),
        ));
    }
    Ok(RuntimeCounterRecord {
        scope: runtime_counter_scope_from_parts(&scope_type, &scope_id)?,
        counter_name: row.get(2),
        value: value as u64,
        updated_at: row.get(4),
    })
}

fn row_to_simple_kv(row: &Row) -> CoreResult<SimpleKvRecord> {
    let value_json: String = row.get(3);
    let revision: i64 = row.get(4);
    if revision <= 0 {
        return Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("invalid simple_kv revision {revision}"),
        ));
    }
    Ok(SimpleKvRecord {
        scope: SimpleKvScope {
            scope_type: row.get(0),
            scope_id: row.get(1),
        },
        key: row.get(2),
        value_json: from_json_text(&value_json).map_err(|error| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("parse PostgreSQL simple_kv value_json: {error}"),
            )
        })?,
        revision: revision as u64,
        created_at: row.get(5),
        updated_at: row.get(6),
        expires_at: row.get(7),
    })
}

fn row_to_runtime_search_result(row: &Row) -> CoreResult<RuntimeSearchResult> {
    let row_type: String = row.get(0);
    let sequence: Option<i64> = row.get(2);
    let event_kind: Option<String> = row.get(7);
    Ok(RuntimeSearchResult {
        row_type: runtime_search_row_type_from_str(&row_type)?,
        row_key: row.get(1),
        sequence: sequence.map(|value| value as u64),
        session_id: row.get::<_, Option<String>>(3).map(crate::SessionId),
        agent_id: row.get::<_, Option<String>>(4).map(crate::AgentId),
        instance_id: row.get::<_, Option<String>>(5).map(crate::AgentInstanceId),
        task_id: row.get::<_, Option<String>>(6).map(crate::TaskId),
        event_kind: event_kind
            .as_deref()
            .map(core_event_kind_from_debug_str)
            .transpose()?,
        recorded_at: row.get(8),
        title: row.get(9),
        body: row.get(10),
    })
}

fn query_profile_memory<C: GenericClient>(
    conn: &mut C,
    schema: &str,
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
    let rows = conn
        .query(
            &format!(
                "SELECT profile_id,
                        target_type,
                        target_id,
                        memory_key,
                        content,
                        metadata_json::text,
                        revision,
                        created_at,
                        updated_at
                 FROM {schema}.profile_memories
                 WHERE profile_id = $1
                   AND ($2::text IS NULL OR target_type = $2)
                   AND ($3::text IS NULL OR target_id = $3)
                 ORDER BY updated_at DESC, memory_key ASC
                 LIMIT $4 OFFSET $5"
            ),
            &[
                &query.profile_id.0,
                &target_type,
                &target_id,
                &limit,
                &offset,
            ],
        )
        .map_err(|error| postgres_error("query PostgreSQL profile memory", error))?;
    rows.iter().map(row_to_profile_memory).collect()
}

fn get_profile_memory<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    profile_id: &ProfileId,
    target: &ProfileMemoryTarget,
    key: &str,
) -> CoreResult<Option<ProfileMemoryRecord>> {
    let (target_type, target_id) = profile_memory_target_parts(profile_id, target);
    let row = conn
        .query_opt(
            &format!(
                "SELECT profile_id,
                        target_type,
                        target_id,
                        memory_key,
                        content,
                        metadata_json::text,
                        revision,
                        created_at,
                        updated_at
                 FROM {schema}.profile_memories
                 WHERE profile_id = $1
                   AND target_type = $2
                   AND target_id = $3
                   AND memory_key = $4"
            ),
            &[&profile_id.0, &target_type, &target_id, &key],
        )
        .map_err(|error| postgres_error("get PostgreSQL profile memory", error))?;
    row.as_ref().map(row_to_profile_memory).transpose()
}

fn insert_profile_memory_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    write: &ProfileMemoryWrite,
) -> CoreResult<ProfileMemoryRecord> {
    let (target_type, target_id) = profile_memory_target_parts(&write.profile_id, &write.target);
    let metadata_json = to_json_text(&write.metadata)?;
    tx.execute(
        &format!(
            "INSERT INTO {schema}.profile_memories (
                profile_id,
                target_type,
                target_id,
                memory_key,
                content,
                metadata_json,
                revision,
                created_at,
                updated_at
             ) VALUES ($1, $2, $3, $4, $5, ($6::text)::jsonb, 1, $7, $7)"
        ),
        &[
            &write.profile_id.0,
            &target_type,
            &target_id,
            &write.key,
            &write.content,
            &metadata_json,
            &write.now,
        ],
    )
    .map_err(|error| postgres_error("insert PostgreSQL profile memory", error))?;
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
    tx: &mut Transaction<'_>,
    schema: &str,
    write: &ProfileMemoryWrite,
    revision: u64,
    created_at: &IsoTimestamp,
) -> CoreResult<ProfileMemoryRecord> {
    validate_counter_amount(revision)?;
    let (target_type, target_id) = profile_memory_target_parts(&write.profile_id, &write.target);
    let metadata_json = to_json_text(&write.metadata)?;
    tx.execute(
        &format!(
            "UPDATE {schema}.profile_memories
             SET content = $5,
                 metadata_json = ($6::text)::jsonb,
                 revision = $7,
                 updated_at = $8
             WHERE profile_id = $1
               AND target_type = $2
               AND target_id = $3
               AND memory_key = $4"
        ),
        &[
            &write.profile_id.0,
            &target_type,
            &target_id,
            &write.key,
            &write.content,
            &metadata_json,
            &(revision as i64),
            &write.now,
        ],
    )
    .map_err(|error| postgres_error("update PostgreSQL profile memory", error))?;
    Ok(ProfileMemoryRecord {
        profile_id: write.profile_id.clone(),
        target: write.target.clone(),
        key: write.key.clone(),
        content: write.content.clone(),
        metadata: write.metadata.clone(),
        revision,
        created_at: created_at.clone(),
        updated_at: write.now.clone(),
    })
}

fn count_profile_memory_for_profile<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    profile_id: &ProfileId,
) -> CoreResult<u64> {
    let row = conn
        .query_one(
            &format!("SELECT COUNT(*) FROM {schema}.profile_memories WHERE profile_id = $1"),
            &[&profile_id.0],
        )
        .map_err(|error| postgres_error("count PostgreSQL profile memory", error))?;
    let count: i64 = row.get(0);
    Ok(count as u64)
}

fn row_to_profile_memory(row: &Row) -> CoreResult<ProfileMemoryRecord> {
    let profile_id = ProfileId::new(row.get::<_, String>(0));
    let target_type: String = row.get(1);
    let target_id: String = row.get(2);
    let revision: i64 = row.get(6);
    if revision <= 0 {
        return Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("invalid PostgreSQL profile memory revision {revision}"),
        ));
    }
    let metadata_json: String = row.get(5);
    Ok(ProfileMemoryRecord {
        profile_id: profile_id.clone(),
        target: profile_memory_target_from_parts(&profile_id, &target_type, target_id)?,
        key: row.get(3),
        content: row.get(4),
        metadata: parse_postgres_json(&metadata_json, "profile memory metadata_json")?,
        revision: revision as u64,
        created_at: row.get(7),
        updated_at: row.get(8),
    })
}

fn profile_memory_target_from_parts(
    profile_id: &ProfileId,
    target_type: &str,
    target_id: String,
) -> CoreResult<ProfileMemoryTarget> {
    match target_type {
        "profile" if target_id == profile_id.0 => Ok(ProfileMemoryTarget::Profile),
        "user" if !target_id.is_empty() => Ok(ProfileMemoryTarget::User(target_id)),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("invalid profile memory target {other}/{target_id}"),
        )),
    }
}

fn save_attachment_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    attachment: &AttachmentWrite,
) -> CoreResult<()> {
    let metadata_json = to_json_text(&attachment.metadata_json)?;
    let status = attachment_status_as_str(attachment.status);
    tx.execute(
        &format!(
            "INSERT INTO {schema}.attachments (
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
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
             ON CONFLICT(attachment_id) DO UPDATE SET
                session_id = EXCLUDED.session_id,
                status = EXCLUDED.status,
                filename = EXCLUDED.filename,
                mime_type = EXCLUDED.mime_type,
                byte_size = EXCLUDED.byte_size,
                storage_url = EXCLUDED.storage_url,
                download_url = EXCLUDED.download_url,
                thumbnail_url = EXCLUDED.thumbnail_url,
                extracted_text = EXCLUDED.extracted_text,
                extracted_text_truncated = EXCLUDED.extracted_text_truncated,
                metadata_json = EXCLUDED.metadata_json,
                updated_at = EXCLUDED.updated_at,
                expires_at = EXCLUDED.expires_at"
        ),
        &[
            &attachment.attachment_id.0,
            &attachment.session_id.0,
            &status,
            &attachment.filename,
            &attachment.mime_type,
            &(attachment.byte_size as i64),
            &attachment.storage_url,
            &attachment.download_url,
            &attachment.thumbnail_url,
            &attachment.extracted_text,
            &attachment.extracted_text_truncated,
            &metadata_json,
            &attachment.created_at,
            &attachment.updated_at,
            &attachment.expires_at,
        ],
    )
    .map_err(|error| postgres_error("save PostgreSQL attachment", error))?;
    if let Some(link) = &attachment.link {
        save_attachment_link_in_tx(tx, schema, link)?;
    }
    Ok(())
}

fn save_attachment_link_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    link: &AttachmentLinkWrite,
) -> CoreResult<()> {
    let metadata_json = to_json_text(&link.metadata_json)?;
    tx.execute(
        &format!(
            "INSERT INTO {schema}.attachment_links (
                link_id,
                attachment_id,
                session_id,
                message_id,
                block_id,
                scope_id,
                metadata_json,
                created_at
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT(link_id) DO UPDATE SET
                attachment_id = EXCLUDED.attachment_id,
                session_id = EXCLUDED.session_id,
                message_id = EXCLUDED.message_id,
                block_id = EXCLUDED.block_id,
                scope_id = EXCLUDED.scope_id,
                metadata_json = EXCLUDED.metadata_json"
        ),
        &[
            &link.link_id.0,
            &link.attachment_id.0,
            &link.session_id.0,
            &link.message_id.as_ref().map(|value| value.0.as_str()),
            &link.block_id.as_ref().map(|value| value.0.as_str()),
            &link.scope_id.as_ref().map(|value| value.0.as_str()),
            &metadata_json,
            &link.created_at,
        ],
    )
    .map_err(|error| postgres_error("save PostgreSQL attachment link", error))?;
    Ok(())
}

fn query_attachments<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    query: &AttachmentQuery,
) -> CoreResult<Vec<AttachmentRecord>> {
    let session_id = query.session_id.as_ref().map(|value| value.0.as_str());
    let message_id = query.message_id.as_ref().map(|value| value.0.as_str());
    let block_id = query.block_id.as_ref().map(|value| value.0.as_str());
    let scope_id = query.scope_id.as_ref().map(|value| value.0.as_str());
    let status = query.status.map(attachment_status_as_str);
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let rows = conn
        .query(
            &format!(
                "SELECT DISTINCT a.attachment_id, a.created_at
                 FROM {schema}.attachments a
                 LEFT JOIN {schema}.attachment_links l ON l.attachment_id = a.attachment_id
                 WHERE ($1::text IS NULL OR a.session_id = $1)
                   AND ($2 OR a.status <> 'removed')
                   AND ($3::text IS NULL OR l.message_id = $3)
                   AND ($4::text IS NULL OR l.scope_id = $4)
                   AND ($5::text IS NULL OR l.block_id = $5)
                   AND ($6::text IS NULL OR a.status = $6)
                   AND (
                        ($7 AND a.expires_at IS NOT NULL AND $8::text IS NOT NULL AND a.expires_at <= $8)
                        OR
                        (NOT $7 AND ($9 OR a.expires_at IS NULL OR $8::text IS NULL OR a.expires_at > $8))
                   )
                 ORDER BY a.created_at ASC, a.attachment_id ASC
                 LIMIT $10 OFFSET $11"
            ),
            &[
                &session_id,
                &query.include_removed,
                &message_id,
                &scope_id,
                &block_id,
                &status,
                &query.expired_only,
                &query.now,
                &query.include_expired,
                &limit,
                &offset,
            ],
        )
        .map_err(|error| postgres_error("query PostgreSQL attachments", error))?;
    rows.iter()
        .map(|row| AttachmentId::new(row.get::<_, String>(0)))
        .map(|attachment_id| load_attachment(conn, schema, &attachment_id))
        .collect()
}

fn load_attachment<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    attachment_id: &AttachmentId,
) -> CoreResult<AttachmentRecord> {
    let row = conn
        .query_opt(
            &format!(
                "SELECT session_id,
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
                 FROM {schema}.attachments
                 WHERE attachment_id = $1"
            ),
            &[&attachment_id.0],
        )
        .map_err(|error| postgres_error("load PostgreSQL attachment", error))?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("attachment {attachment_id} not found"),
            )
        })?;
    row_to_attachment(conn, schema, attachment_id, &row)
}

fn row_to_attachment<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    attachment_id: &AttachmentId,
    row: &Row,
) -> CoreResult<AttachmentRecord> {
    let status: String = row.get(1);
    let byte_size: i64 = row.get(4);
    if byte_size < 0 {
        return Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("invalid PostgreSQL attachment byte_size {byte_size}"),
        ));
    }
    let metadata_json: String = row.get(10);
    Ok(AttachmentRecord {
        attachment_id: attachment_id.clone(),
        session_id: SessionId::new(row.get::<_, String>(0)),
        status: attachment_status_from_str(&status)?,
        filename: row.get(2),
        mime_type: row.get(3),
        byte_size: byte_size as u64,
        storage_url: row.get(5),
        download_url: row.get(6),
        thumbnail_url: row.get(7),
        extracted_text: row.get(8),
        extracted_text_truncated: row.get(9),
        metadata_json: parse_postgres_json(&metadata_json, "attachment metadata_json")?,
        created_at: row.get(11),
        updated_at: row.get(12),
        expires_at: row.get(13),
        links: load_attachment_links(conn, schema, attachment_id)?,
    })
}

fn load_attachment_links<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    attachment_id: &AttachmentId,
) -> CoreResult<Vec<AttachmentLinkRecord>> {
    let rows = conn
        .query(
            &format!(
                "SELECT link_id,
                        session_id,
                        message_id,
                        block_id,
                        scope_id,
                        metadata_json,
                        created_at
                 FROM {schema}.attachment_links
                 WHERE attachment_id = $1
                 ORDER BY created_at ASC, link_id ASC"
            ),
            &[&attachment_id.0],
        )
        .map_err(|error| postgres_error("load PostgreSQL attachment links", error))?;
    rows.iter()
        .map(|row| row_to_attachment_link(row, attachment_id))
        .collect()
}

fn row_to_attachment_link(
    row: &Row,
    attachment_id: &AttachmentId,
) -> CoreResult<AttachmentLinkRecord> {
    let metadata_json: String = row.get(5);
    Ok(AttachmentLinkRecord {
        link_id: AttachmentLinkId::new(row.get::<_, String>(0)),
        attachment_id: attachment_id.clone(),
        session_id: SessionId::new(row.get::<_, String>(1)),
        message_id: row.get::<_, Option<String>>(2).map(MessageId::new),
        block_id: row
            .get::<_, Option<String>>(3)
            .map(crate::MessageBlockId::new),
        scope_id: row.get::<_, Option<String>>(4).map(DataBankScopeId::new),
        metadata_json: parse_postgres_json(&metadata_json, "attachment link metadata_json")?,
        created_at: row.get(6),
    })
}

fn save_data_bank_scope_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    scope: &DataBankScopeWrite,
) -> CoreResult<()> {
    let metadata_json = to_json_text(&scope.metadata_json)?;
    let status = data_bank_scope_status_as_str(scope.status);
    tx.execute(
        &format!(
            "INSERT INTO {schema}.data_bank_scopes (
                scope_id,
                session_id,
                status,
                label,
                description,
                metadata_json,
                created_at,
                updated_at
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT(scope_id) DO UPDATE SET
                session_id = EXCLUDED.session_id,
                status = EXCLUDED.status,
                label = EXCLUDED.label,
                description = EXCLUDED.description,
                metadata_json = EXCLUDED.metadata_json,
                updated_at = EXCLUDED.updated_at"
        ),
        &[
            &scope.scope_id.0,
            &scope.session_id.0,
            &status,
            &scope.label,
            &scope.description,
            &metadata_json,
            &scope.created_at,
            &scope.updated_at,
        ],
    )
    .map_err(|error| postgres_error("save PostgreSQL data-bank scope", error))?;
    Ok(())
}

fn query_data_bank_scopes<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    query: &DataBankScopeQuery,
) -> CoreResult<Vec<DataBankScopeRecord>> {
    let session_id = query.session_id.as_ref().map(|value| value.0.as_str());
    let status = query.status.map(data_bank_scope_status_as_str);
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let rows = conn
        .query(
            &format!(
                "SELECT scope_id
                 FROM {schema}.data_bank_scopes
                 WHERE ($1::text IS NULL OR session_id = $1)
                   AND ($2 OR status <> 'removed')
                   AND ($3::text IS NULL OR status = $3)
                 ORDER BY created_at ASC, scope_id ASC
                 LIMIT $4 OFFSET $5"
            ),
            &[
                &session_id,
                &query.include_removed,
                &status,
                &limit,
                &offset,
            ],
        )
        .map_err(|error| postgres_error("query PostgreSQL data-bank scopes", error))?;
    rows.iter()
        .map(|row| DataBankScopeId::new(row.get::<_, String>(0)))
        .map(|scope_id| load_data_bank_scope(conn, schema, &scope_id))
        .collect()
}

fn load_data_bank_scope<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    scope_id: &DataBankScopeId,
) -> CoreResult<DataBankScopeRecord> {
    let row = conn
        .query_opt(
            &format!(
                "SELECT session_id,
                        status,
                        label,
                        description,
                        metadata_json,
                        created_at,
                        updated_at
                 FROM {schema}.data_bank_scopes
                 WHERE scope_id = $1"
            ),
            &[&scope_id.0],
        )
        .map_err(|error| postgres_error("load PostgreSQL data-bank scope", error))?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("data-bank scope {scope_id} not found"),
            )
        })?;
    row_to_data_bank_scope(scope_id, &row)
}

fn row_to_data_bank_scope(
    scope_id: &DataBankScopeId,
    row: &Row,
) -> CoreResult<DataBankScopeRecord> {
    let status: String = row.get(1);
    let metadata_json: String = row.get(4);
    Ok(DataBankScopeRecord {
        scope_id: scope_id.clone(),
        session_id: SessionId::new(row.get::<_, String>(0)),
        status: data_bank_scope_status_from_str(&status)?,
        label: row.get(2),
        description: row.get(3),
        metadata_json: parse_postgres_json(&metadata_json, "data-bank scope metadata_json")?,
        created_at: row.get(5),
        updated_at: row.get(6),
    })
}

fn save_message_variant_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    variant: &MessageVariantWrite,
) -> CoreResult<()> {
    if variant.source == MessageVariantSource::Primary && variant.ordinal != 0 {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "primary message variant ordinal must be 0",
        ));
    }
    save_durable_message_in_tx(tx, schema, &variant.message)?;
    let metadata_json = to_json_text(&variant.metadata_json)?;
    let source = message_variant_source_as_str(variant.source);
    let status = message_variant_status_as_str(variant.status);
    tx.execute(
        &format!(
            "INSERT INTO {schema}.message_variants (
                variant_id,
                slot_id,
                source,
                ordinal,
                status,
                message_id,
                metadata_json,
                created_at,
                updated_at
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT(variant_id) DO UPDATE SET
                slot_id = EXCLUDED.slot_id,
                source = EXCLUDED.source,
                ordinal = EXCLUDED.ordinal,
                status = EXCLUDED.status,
                message_id = EXCLUDED.message_id,
                metadata_json = EXCLUDED.metadata_json,
                updated_at = EXCLUDED.updated_at"
        ),
        &[
            &variant.variant_id.0,
            &variant.slot_id.0,
            &source,
            &(variant.ordinal as i64),
            &status,
            &variant.message.message_id.0,
            &metadata_json,
            &variant.created_at,
            &variant.updated_at,
        ],
    )
    .map_err(|error| postgres_error("save PostgreSQL message variant", error))?;
    Ok(())
}

fn save_durable_message_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    message: &DurableMessageWrite,
) -> CoreResult<()> {
    let metadata_json = to_json_text(&message.metadata_json)?;
    let status = durable_message_status_as_str(message.status);
    tx.execute(
        &format!(
            "INSERT INTO {schema}.messages (
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
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
             ON CONFLICT(message_id) DO UPDATE SET
                session_id = EXCLUDED.session_id,
                branch_id = EXCLUDED.branch_id,
                parent_message_id = EXCLUDED.parent_message_id,
                previous_message_id = EXCLUDED.previous_message_id,
                author_id = EXCLUDED.author_id,
                author_role = EXCLUDED.author_role,
                status = EXCLUDED.status,
                body = EXCLUDED.body,
                metadata_json = EXCLUDED.metadata_json"
        ),
        &[
            &message.message_id.0,
            &message.session_id.0,
            &message.branch_id.as_ref().map(|value| value.0.as_str()),
            &message
                .parent_message_id
                .as_ref()
                .map(|value| value.0.as_str()),
            &message
                .previous_message_id
                .as_ref()
                .map(|value| value.0.as_str()),
            &message.author_id,
            &message.author_role,
            &status,
            &message.body,
            &metadata_json,
            &message.created_at,
        ],
    )
    .map_err(|error| postgres_error("save PostgreSQL durable message", error))?;
    tx.execute(
        &format!("DELETE FROM {schema}.message_blocks WHERE message_id = $1"),
        &[&message.message_id.0],
    )
    .map_err(|error| postgres_error("replace PostgreSQL message blocks", error))?;
    for block in &message.blocks {
        let content_json = to_json_text(&block.content_json)?;
        let render_policy_json = block
            .render_policy_json
            .as_ref()
            .map(to_json_text)
            .transpose()?;
        let metadata_json = to_json_text(&block.metadata_json)?;
        tx.execute(
            &format!(
                "INSERT INTO {schema}.message_blocks (
                    block_id,
                    message_id,
                    ordinal,
                    kind,
                    content_json,
                    render_policy_json,
                    metadata_json
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7)"
            ),
            &[
                &block.block_id.0,
                &message.message_id.0,
                &(block.ordinal as i64),
                &block.kind,
                &content_json,
                &render_policy_json,
                &metadata_json,
            ],
        )
        .map_err(|error| postgres_error("save PostgreSQL message block", error))?;
    }
    Ok(())
}

fn query_message_variants<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    query: &MessageVariantQuery,
) -> CoreResult<Vec<MessageVariantRecord>> {
    let slot_id = query.slot_id.as_ref().map(|value| value.0.as_str());
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let rows = conn
        .query(
            &format!(
                "SELECT variant_id
                 FROM {schema}.message_variants
                 WHERE ($1::text IS NULL OR slot_id = $1)
                   AND ($2 OR status <> 'deleted')
                 ORDER BY slot_id ASC, ordinal ASC, variant_id ASC
                 LIMIT $3 OFFSET $4"
            ),
            &[&slot_id, &query.include_deleted, &limit, &offset],
        )
        .map_err(|error| postgres_error("query PostgreSQL message variants", error))?;
    rows.iter()
        .map(|row| MessageVariantId::new(row.get::<_, String>(0)))
        .map(|variant_id| load_message_variant(conn, schema, &variant_id))
        .collect()
}

fn load_message_slot<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    slot_id: &MessageSlotId,
    include_alternates: bool,
) -> CoreResult<MessageSlotRecord> {
    let row = conn
        .query_opt(
            &format!(
                "SELECT session_id,
                        primary_variant_id,
                        active_variant_id,
                        metadata_json,
                        created_at,
                        updated_at,
                        version
                 FROM {schema}.message_slots
                 WHERE slot_id = $1"
            ),
            &[&slot_id.0],
        )
        .map_err(|error| postgres_error("load PostgreSQL message slot", error))?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("message slot {slot_id} not found"),
            )
        })?;
    row_to_message_slot(conn, schema, slot_id, include_alternates, &row)
}

fn load_message_slot_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    slot_id: &MessageSlotId,
    include_alternates: bool,
) -> CoreResult<MessageSlotRecord> {
    load_message_slot(tx, schema, slot_id, include_alternates)
}

fn row_to_message_slot<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    slot_id: &MessageSlotId,
    include_alternates: bool,
    row: &Row,
) -> CoreResult<MessageSlotRecord> {
    let metadata_json: String = row.get(3);
    let version: i64 = row.get(6);
    if version < 0 {
        return Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("invalid PostgreSQL message slot version {version}"),
        ));
    }
    let primary_variant_id = MessageVariantId::new(row.get::<_, String>(1));
    let primary = load_message_variant(conn, schema, &primary_variant_id)?;
    let alternates = if include_alternates {
        query_message_variants(
            conn,
            schema,
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
        session_id: SessionId::new(row.get::<_, String>(0)),
        primary_variant_id,
        active_variant_id: row.get::<_, Option<String>>(2).map(MessageVariantId::new),
        metadata_json: parse_postgres_json(&metadata_json, "message slot metadata_json")?,
        created_at: row.get(4),
        updated_at: row.get(5),
        version: version as u64,
        primary,
        alternates,
    })
}

fn load_message_variant<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    variant_id: &MessageVariantId,
) -> CoreResult<MessageVariantRecord> {
    let row = conn
        .query_opt(
            &format!(
                "SELECT slot_id,
                        source,
                        ordinal,
                        status,
                        message_id,
                        metadata_json,
                        created_at,
                        updated_at
                 FROM {schema}.message_variants
                 WHERE variant_id = $1"
            ),
            &[&variant_id.0],
        )
        .map_err(|error| postgres_error("load PostgreSQL message variant", error))?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("message variant {variant_id} not found"),
            )
        })?;
    row_to_message_variant(conn, schema, variant_id, &row)
}

fn load_message_variant_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    variant_id: &MessageVariantId,
) -> CoreResult<MessageVariantRecord> {
    load_message_variant(tx, schema, variant_id)
}

fn row_to_message_variant<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    variant_id: &MessageVariantId,
    row: &Row,
) -> CoreResult<MessageVariantRecord> {
    let ordinal: i64 = row.get(2);
    if ordinal < 0 || ordinal > u32::MAX as i64 {
        return Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("invalid PostgreSQL message variant ordinal {ordinal}"),
        ));
    }
    let source: String = row.get(1);
    let status: String = row.get(3);
    let message_id = MessageId::new(row.get::<_, String>(4));
    let metadata_json: String = row.get(5);
    Ok(MessageVariantRecord {
        variant_id: variant_id.clone(),
        slot_id: MessageSlotId::new(row.get::<_, String>(0)),
        source: message_variant_source_from_str(&source)?,
        ordinal: ordinal as u32,
        status: message_variant_status_from_str(&status)?,
        message: load_durable_message(conn, schema, &message_id)?,
        metadata_json: parse_postgres_json(&metadata_json, "message variant metadata_json")?,
        created_at: row.get(6),
        updated_at: row.get(7),
    })
}

fn load_durable_message<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    message_id: &MessageId,
) -> CoreResult<DurableMessageRecord> {
    let row = conn
        .query_opt(
            &format!(
                "SELECT session_id,
                        branch_id,
                        parent_message_id,
                        previous_message_id,
                        author_id,
                        author_role,
                        status,
                        body,
                        metadata_json,
                        created_at
                 FROM {schema}.messages
                 WHERE message_id = $1"
            ),
            &[&message_id.0],
        )
        .map_err(|error| postgres_error("load PostgreSQL durable message", error))?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("message {message_id} not found"),
            )
        })?;
    row_to_durable_message(conn, schema, message_id, &row)
}

fn row_to_durable_message<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    message_id: &MessageId,
    row: &Row,
) -> CoreResult<DurableMessageRecord> {
    let status: String = row.get(6);
    let metadata_json: String = row.get(8);
    Ok(DurableMessageRecord {
        message_id: message_id.clone(),
        session_id: SessionId::new(row.get::<_, String>(0)),
        branch_id: row
            .get::<_, Option<String>>(1)
            .map(ConversationBranchId::new),
        parent_message_id: row.get::<_, Option<String>>(2).map(MessageId::new),
        previous_message_id: row.get::<_, Option<String>>(3).map(MessageId::new),
        author_id: row.get(4),
        author_role: row.get(5),
        status: durable_message_status_from_str(&status)?,
        body: row.get(7),
        metadata_json: parse_postgres_json(&metadata_json, "durable message metadata_json")?,
        created_at: row.get(9),
        blocks: load_message_blocks(conn, schema, message_id)?,
    })
}

fn load_message_blocks<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    message_id: &MessageId,
) -> CoreResult<Vec<MessageBlockRecord>> {
    let rows = conn
        .query(
            &format!(
                "SELECT block_id,
                        ordinal,
                        kind,
                        content_json,
                        render_policy_json,
                        metadata_json
                 FROM {schema}.message_blocks
                 WHERE message_id = $1
                 ORDER BY ordinal ASC, block_id ASC"
            ),
            &[&message_id.0],
        )
        .map_err(|error| postgres_error("load PostgreSQL message blocks", error))?;
    rows.iter()
        .map(|row| row_to_message_block(row, message_id))
        .collect()
}

fn row_to_message_block(row: &Row, message_id: &MessageId) -> CoreResult<MessageBlockRecord> {
    let ordinal: i64 = row.get(1);
    if ordinal < 0 || ordinal > u32::MAX as i64 {
        return Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("invalid PostgreSQL message block ordinal {ordinal}"),
        ));
    }
    let content_json: String = row.get(3);
    let render_policy_json: Option<String> = row.get(4);
    let metadata_json: String = row.get(5);
    Ok(MessageBlockRecord {
        block_id: crate::MessageBlockId::new(row.get::<_, String>(0)),
        message_id: message_id.clone(),
        ordinal: ordinal as u32,
        kind: row.get(2),
        content_json: parse_postgres_json(&content_json, "message block content_json")?,
        render_policy_json: render_policy_json
            .as_deref()
            .map(|value| parse_postgres_json(value, "message block render_policy_json"))
            .transpose()?,
        metadata_json: parse_postgres_json(&metadata_json, "message block metadata_json")?,
    })
}

fn load_conversation_branch<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    branch_id: &ConversationBranchId,
) -> CoreResult<ConversationBranchRecord> {
    let row = conn
        .query_opt(
            &format!(
                "SELECT session_id,
                        parent_branch_id,
                        parent_message_id,
                        origin_message_id,
                        head_message_id,
                        label,
                        metadata_json,
                        created_at,
                        updated_at,
                        version
                 FROM {schema}.conversation_branches
                 WHERE branch_id = $1"
            ),
            &[&branch_id.0],
        )
        .map_err(|error| postgres_error("load PostgreSQL conversation branch", error))?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("conversation branch {branch_id} not found"),
            )
        })?;
    row_to_conversation_branch(branch_id, &row)
}

fn load_conversation_branch_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    branch_id: &ConversationBranchId,
) -> CoreResult<ConversationBranchRecord> {
    load_conversation_branch(tx, schema, branch_id)
}

fn row_to_conversation_branch(
    branch_id: &ConversationBranchId,
    row: &Row,
) -> CoreResult<ConversationBranchRecord> {
    let metadata_json: String = row.get(6);
    let version: i64 = row.get(9);
    if version < 0 {
        return Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("invalid PostgreSQL conversation branch version {version}"),
        ));
    }
    Ok(ConversationBranchRecord {
        branch_id: branch_id.clone(),
        session_id: SessionId::new(row.get::<_, String>(0)),
        parent_branch_id: row
            .get::<_, Option<String>>(1)
            .map(ConversationBranchId::new),
        parent_message_id: row.get::<_, Option<String>>(2).map(MessageId::new),
        origin_message_id: row.get::<_, Option<String>>(3).map(MessageId::new),
        head_message_id: row.get::<_, Option<String>>(4).map(MessageId::new),
        label: row.get(5),
        metadata_json: parse_postgres_json(&metadata_json, "conversation branch metadata_json")?,
        created_at: row.get(7),
        updated_at: row.get(8),
        version: version as u64,
    })
}

fn current_active_branch_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    session_id: &SessionId,
) -> CoreResult<Option<ConversationBranchId>> {
    let row = tx
        .query_opt(
            &format!(
                "SELECT active_branch_id
                 FROM {schema}.conversation_branch_state
                 WHERE session_id = $1
                 FOR UPDATE"
            ),
            &[&session_id.0],
        )
        .map_err(|error| postgres_error("load PostgreSQL current active branch", error))?;
    Ok(row
        .as_ref()
        .and_then(|row| row.get::<_, Option<String>>(0))
        .map(ConversationBranchId::new))
}

fn load_conversation_branch_state<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    session_id: &SessionId,
    default_updated_at: &IsoTimestamp,
) -> CoreResult<ConversationBranchStateRecord> {
    let row = conn
        .query_opt(
            &format!(
                "SELECT active_branch_id, updated_at, version
                 FROM {schema}.conversation_branch_state
                 WHERE session_id = $1"
            ),
            &[&session_id.0],
        )
        .map_err(|error| postgres_error("load PostgreSQL conversation branch state", error))?;
    row.as_ref()
        .map(|row| row_to_conversation_branch_state(session_id, row))
        .transpose()
        .map(|state| {
            state.unwrap_or_else(|| ConversationBranchStateRecord {
                session_id: session_id.clone(),
                active_branch_id: None,
                updated_at: default_updated_at.clone(),
                version: 0,
            })
        })
}

fn load_conversation_branch_state_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    session_id: &SessionId,
    default_updated_at: &IsoTimestamp,
) -> CoreResult<ConversationBranchStateRecord> {
    load_conversation_branch_state(tx, schema, session_id, default_updated_at)
}

fn row_to_conversation_branch_state(
    session_id: &SessionId,
    row: &Row,
) -> CoreResult<ConversationBranchStateRecord> {
    let version: i64 = row.get(2);
    if version < 0 {
        return Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("invalid PostgreSQL conversation branch state version {version}"),
        ));
    }
    Ok(ConversationBranchStateRecord {
        session_id: session_id.clone(),
        active_branch_id: row
            .get::<_, Option<String>>(0)
            .map(ConversationBranchId::new),
        updated_at: row.get(1),
        version: version as u64,
    })
}

fn current_branch_head_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    branch_id: &ConversationBranchId,
) -> CoreResult<Option<MessageId>> {
    let row = tx
        .query_opt(
            &format!(
                "SELECT head_message_id
                 FROM {schema}.conversation_branches
                 WHERE branch_id = $1
                 FOR UPDATE"
            ),
            &[&branch_id.0],
        )
        .map_err(|error| postgres_error("load PostgreSQL current branch head", error))?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("conversation branch {branch_id} not found"),
            )
        })?;
    Ok(row.get::<_, Option<String>>(0).map(MessageId::new))
}

fn ensure_branch_belongs_to_session_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    session_id: &SessionId,
    branch_id: &ConversationBranchId,
) -> CoreResult<()> {
    let row = tx
        .query_one(
            &format!(
                "SELECT EXISTS(
                    SELECT 1 FROM {schema}.conversation_branches
                    WHERE session_id = $1 AND branch_id = $2
                )"
            ),
            &[&session_id.0, &branch_id.0],
        )
        .map_err(|error| postgres_error("check PostgreSQL branch session ownership", error))?;
    if row.get::<_, bool>(0) {
        Ok(())
    } else {
        Err(CoreError::new(
            CoreErrorKind::NotFound,
            format!("conversation branch {branch_id} not found for session {session_id}"),
        ))
    }
}

fn ensure_message_exists_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    message_id: &MessageId,
) -> CoreResult<()> {
    let row = tx
        .query_one(
            &format!(
                "SELECT EXISTS(
                    SELECT 1 FROM {schema}.messages
                    WHERE message_id = $1
                )"
            ),
            &[&message_id.0],
        )
        .map_err(|error| postgres_error("check PostgreSQL durable message existence", error))?;
    if row.get::<_, bool>(0) {
        Ok(())
    } else {
        Err(CoreError::new(
            CoreErrorKind::NotFound,
            format!("message {message_id} not found"),
        ))
    }
}

fn load_conversation_snapshot<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    snapshot_id: &ConversationSnapshotId,
) -> CoreResult<ConversationSnapshotRecord> {
    let row = conn
        .query_opt(
            &format!(
                "SELECT session_id,
                        branch_id,
                        message_id,
                        cursor,
                        label,
                        summary,
                        source,
                        metadata_json,
                        created_at,
                        updated_at
                 FROM {schema}.conversation_snapshots
                 WHERE snapshot_id = $1"
            ),
            &[&snapshot_id.0],
        )
        .map_err(|error| postgres_error("load PostgreSQL conversation snapshot", error))?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("conversation snapshot {snapshot_id} not found"),
            )
        })?;
    row_to_conversation_snapshot(snapshot_id, &row)
}

fn load_conversation_snapshot_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    snapshot_id: &ConversationSnapshotId,
) -> CoreResult<ConversationSnapshotRecord> {
    load_conversation_snapshot(tx, schema, snapshot_id)
}

fn row_to_conversation_snapshot(
    snapshot_id: &ConversationSnapshotId,
    row: &Row,
) -> CoreResult<ConversationSnapshotRecord> {
    let source: String = row.get(6);
    let metadata_json: String = row.get(7);
    Ok(ConversationSnapshotRecord {
        snapshot_id: snapshot_id.clone(),
        session_id: SessionId::new(row.get::<_, String>(0)),
        branch_id: row
            .get::<_, Option<String>>(1)
            .map(ConversationBranchId::new),
        message_id: row.get::<_, Option<String>>(2).map(MessageId::new),
        cursor: row.get(3),
        label: row.get(4),
        summary: row.get(5),
        source: conversation_snapshot_source_from_str(&source)?,
        metadata_json: parse_postgres_json(&metadata_json, "conversation snapshot metadata_json")?,
        created_at: row.get(8),
        updated_at: row.get(9),
    })
}

fn resolve_conversation_jump<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    request: &ConversationJumpRequest,
) -> CoreResult<ConversationJumpResult> {
    match &request.target {
        ConversationJumpTarget::Message { message_id } => {
            let message = load_durable_message(conn, schema, message_id)?;
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
            let branch = load_conversation_branch(conn, schema, branch_id)?;
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
            let snapshot = load_conversation_snapshot(conn, schema, snapshot_id)?;
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

fn current_active_variant_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    slot_id: &MessageSlotId,
) -> CoreResult<Option<MessageVariantId>> {
    let row = tx
        .query_opt(
            &format!(
                "SELECT active_variant_id
                 FROM {schema}.message_slots
                 WHERE slot_id = $1
                 FOR UPDATE"
            ),
            &[&slot_id.0],
        )
        .map_err(|error| postgres_error("load PostgreSQL active message variant", error))?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("message slot {slot_id} not found"),
            )
        })?;
    Ok(row.get::<_, Option<String>>(0).map(MessageVariantId::new))
}

fn ensure_variant_belongs_to_slot_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    slot_id: &MessageSlotId,
    variant_id: &MessageVariantId,
) -> CoreResult<()> {
    let row = tx
        .query_one(
            &format!(
                "SELECT EXISTS(
                    SELECT 1 FROM {schema}.message_variants
                    WHERE slot_id = $1 AND variant_id = $2 AND status <> 'deleted'
                )"
            ),
            &[&slot_id.0, &variant_id.0],
        )
        .map_err(|error| postgres_error("check PostgreSQL message variant slot", error))?;
    if row.get::<_, bool>(0) {
        Ok(())
    } else {
        Err(CoreError::new(
            CoreErrorKind::NotFound,
            format!("message variant {variant_id} not found in slot {slot_id}"),
        ))
    }
}

fn insert_session_memory_record_in_tx<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    write: &SessionMemoryRecordWrite,
) -> CoreResult<()> {
    let record = SessionMemoryRecord {
        record_id: write.record_id.clone(),
        session_id: write.session_id.clone(),
        scope: write.scope.clone(),
        branch_id: write.branch_id.clone(),
        shape: write.shape.clone(),
        status: SessionMemoryRecordStatus::Active,
        revision: 1,
        content: write.content.clone(),
        evidence_refs: write.evidence_refs.clone(),
        source: write.source,
        confidence: write.confidence,
        durability_rationale: write.durability_rationale.clone(),
        supersedes_record_id: write.supersedes_record_id.clone(),
        superseded_by_record_id: None,
        archived_at: None,
        archive_reason: None,
        created_at: write.now.clone(),
        updated_at: write.now.clone(),
    };
    upsert_session_memory_record_in_tx(conn, schema, &record)
}

fn upsert_session_memory_record_in_tx<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    record: &SessionMemoryRecord,
) -> CoreResult<()> {
    let record_json = to_json_text(record)?;
    let scope_type = memory_scope_type_as_str(record.scope.scope_type).to_string();
    let status = session_memory_status_as_str(record.status).to_string();
    conn.execute(
        &format!(
            "INSERT INTO {schema}.session_memory_records (
                record_id,
                session_id,
                scope_type,
                scope_id,
                branch_id,
                shape_id,
                status,
                revision,
                record_json,
                created_at,
                updated_at
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
             ON CONFLICT(record_id) DO UPDATE SET
                session_id = EXCLUDED.session_id,
                scope_type = EXCLUDED.scope_type,
                scope_id = EXCLUDED.scope_id,
                branch_id = EXCLUDED.branch_id,
                shape_id = EXCLUDED.shape_id,
                status = EXCLUDED.status,
                revision = EXCLUDED.revision,
                record_json = EXCLUDED.record_json,
                updated_at = EXCLUDED.updated_at"
        ),
        &[
            &record.record_id,
            &record.session_id.0,
            &scope_type,
            &record.scope.scope_id,
            &record.branch_id.as_ref().map(|value| value.0.as_str()),
            &record.shape.shape_id.0,
            &status,
            &(record.revision as i64),
            &record_json,
            &record.created_at,
            &record.updated_at,
        ],
    )
    .map_err(|error| postgres_error("upsert PostgreSQL session memory record", error))?;
    Ok(())
}

fn get_session_memory_record_in_tx<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    record_id: &str,
) -> CoreResult<Option<SessionMemoryRecord>> {
    let row = conn
        .query_opt(
            &format!(
                "SELECT record_json
                 FROM {schema}.session_memory_records
                 WHERE record_id = $1"
            ),
            &[&record_id],
        )
        .map_err(|error| postgres_error("get PostgreSQL session memory record", error))?;
    row.map(|row| {
        let record_json: String = row.get(0);
        parse_postgres_json(&record_json, "session memory record_json")
    })
    .transpose()
}

fn active_session_memory_record_for_update(
    tx: &mut Transaction<'_>,
    schema: &str,
    record_id: &str,
    expected_revision: u64,
) -> CoreResult<SessionMemoryRecord> {
    crate::validate_session_memory_record_id(record_id)?;
    if expected_revision == 0 {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "session memory expected_revision must be greater than zero",
        ));
    }
    let row = tx
        .query_opt(
            &format!(
                "SELECT record_json
                 FROM {schema}.session_memory_records
                 WHERE record_id = $1
                 FOR UPDATE"
            ),
            &[&record_id],
        )
        .map_err(|error| postgres_error("lock PostgreSQL session memory record", error))?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("session memory record {record_id} not found"),
            )
        })?;
    let record_json: String = row.get(0);
    let record: SessionMemoryRecord =
        parse_postgres_json(&record_json, "session memory record_json")?;
    if record.status != SessionMemoryRecordStatus::Active {
        return Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            format!("session memory record {record_id} is not active"),
        ));
    }
    if record.revision != expected_revision {
        return Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            format!(
                "session memory revision mismatch for {record_id}: expected {expected_revision}, found {}",
                record.revision
            ),
        ));
    }
    Ok(record)
}

fn update_session_memory_record_content_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    replace: &SessionMemoryReplace,
    revision: u64,
) -> CoreResult<()> {
    let mut record =
        get_session_memory_record_in_tx(tx, schema, &replace.record_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("session memory record {} not found", replace.record_id),
            )
        })?;
    record.content = replace.content.clone();
    record.evidence_refs = replace.evidence_refs.clone();
    record.source = replace.source;
    record.confidence = replace.confidence;
    record.durability_rationale = replace.durability_rationale.clone();
    record.revision = revision;
    record.updated_at = replace.now.clone();
    upsert_session_memory_record_in_tx(tx, schema, &record)
}

fn mark_session_memory_superseded_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    record_id: &str,
    replacement_record_id: &str,
    revision: u64,
    now: &IsoTimestamp,
) -> CoreResult<()> {
    let mut record = get_session_memory_record_in_tx(tx, schema, record_id)?.ok_or_else(|| {
        CoreError::new(
            CoreErrorKind::NotFound,
            format!("session memory record {record_id} not found"),
        )
    })?;
    record.status = SessionMemoryRecordStatus::Superseded;
    record.superseded_by_record_id = Some(replacement_record_id.to_string());
    record.revision = revision;
    record.updated_at = now.clone();
    upsert_session_memory_record_in_tx(tx, schema, &record)
}

fn archive_session_memory_record_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    archive: &SessionMemoryArchive,
    revision: u64,
) -> CoreResult<()> {
    let mut record =
        get_session_memory_record_in_tx(tx, schema, &archive.record_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("session memory record {} not found", archive.record_id),
            )
        })?;
    record.status = SessionMemoryRecordStatus::Archived;
    record.archived_at = Some(archive.now.clone());
    record.archive_reason = archive.reason.clone();
    record.revision = revision;
    record.updated_at = archive.now.clone();
    upsert_session_memory_record_in_tx(tx, schema, &record)
}

fn query_session_memory_records<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    query: &SessionMemoryQuery,
) -> CoreResult<Vec<SessionMemoryRecord>> {
    let session_id = query.session_id.as_ref().map(|value| value.0.as_str());
    let branch_id = query.branch_id.as_ref().map(|value| value.0.as_str());
    let scope_type = query
        .scope_type
        .map(memory_scope_type_as_str)
        .map(str::to_string);
    let shape_id = query.shape_id.as_deref();
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let rows = conn
        .query(
            &format!(
                "SELECT record_json
                 FROM {schema}.session_memory_records
                 WHERE ($1::TEXT IS NULL OR session_id = $1)
                   AND ($2::TEXT IS NULL OR branch_id = $2)
                   AND ($3::TEXT IS NULL OR scope_type = $3)
                   AND ($4::TEXT IS NULL OR shape_id = $4)
                   AND ($5 OR status <> 'superseded')
                   AND ($6 OR status <> 'archived')
                 ORDER BY updated_at DESC, record_id ASC
                 LIMIT $7 OFFSET $8"
            ),
            &[
                &session_id,
                &branch_id,
                &scope_type,
                &shape_id,
                &query.include_superseded,
                &query.include_archived,
                &limit,
                &offset,
            ],
        )
        .map_err(|error| postgres_error("query PostgreSQL session memory records", error))?;
    rows.iter()
        .map(|row| {
            let record_json: String = row.get(0);
            parse_postgres_json(&record_json, "session memory record_json")
        })
        .collect()
}

fn select_branch_aware_session_memory<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    query: &BranchAwareSessionMemoryQuery,
) -> CoreResult<SessionMemoryPromptContext> {
    let descriptor = session_memory_space_descriptor();
    let ancestor_distances =
        load_branch_ancestor_distances(conn, schema, &query.session_id, &query.active_branch_id)?;
    let mut records = query_session_memory_records(
        conn,
        schema,
        &SessionMemoryQuery {
            session_id: Some(query.session_id.clone()),
            shape_id: query.shape_id.clone(),
            include_superseded: true,
            include_archived: true,
            page: None,
            ..SessionMemoryQuery::default()
        },
    )?;
    records.sort_by(|left, right| {
        let left_key = session_memory_sort_key(left, query, &ancestor_distances);
        let right_key = session_memory_sort_key(right, query, &ancestor_distances);
        left_key
            .cmp(&right_key)
            .then_with(|| right.updated_at.cmp(&left.updated_at))
            .then_with(|| left.record_id.cmp(&right.record_id))
    });

    let mut excluded_counts = SessionMemoryPromptExcludedCounts::default();
    let mut candidates = Vec::new();
    for record in records {
        if let Some(reason) = session_memory_exclusion_reason(&record, query, &ancestor_distances) {
            increment_session_memory_excluded_count(&mut excluded_counts, reason);
            continue;
        }
        candidates.push(record);
    }

    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let selected = candidates
        .iter()
        .skip(offset as usize)
        .take(limit as usize)
        .cloned()
        .collect::<Vec<_>>();
    excluded_counts.limit_exceeded = candidates.len().saturating_sub(selected.len()) as u64;
    let character_estimate = selected
        .iter()
        .map(session_memory_record_character_estimate)
        .sum::<u64>();
    let token_estimate = character_estimate.div_ceil(4);
    let selected_records = selected
        .iter()
        .map(|record| SessionMemorySelectedRecordDiagnostic {
            record_id: record.record_id.clone(),
            shape_id: record.shape.shape_id.0.clone(),
        })
        .collect();
    Ok(SessionMemoryPromptContext {
        records: selected,
        diagnostics: SessionMemoryPromptDiagnostics {
            descriptor_id: descriptor.space_id.0,
            descriptor_schema_version: descriptor.schema_version,
            session_id: query.session_id.clone(),
            active_branch_id: query.active_branch_id.clone(),
            selected_records,
            excluded_counts,
            character_estimate,
            token_estimate,
            context_policy: if query.prompt_context_only {
                SessionMemoryPromptContextPolicy::SummaryContext
            } else {
                SessionMemoryPromptContextPolicy::ToolOnly
            },
        },
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionMemoryPromptExclusionReason {
    WrongBranch,
    SiblingBranch,
    ToolOnly,
    Archived,
    Superseded,
    PolicyDisabled,
}

fn session_memory_exclusion_reason(
    record: &SessionMemoryRecord,
    query: &BranchAwareSessionMemoryQuery,
    ancestor_distances: &[(ConversationBranchId, u32)],
) -> Option<SessionMemoryPromptExclusionReason> {
    match record.scope.scope_type {
        MemoryScopeType::Session => {}
        MemoryScopeType::ConversationBranch => {
            let Some(record_branch_id) = &record.branch_id else {
                return Some(SessionMemoryPromptExclusionReason::WrongBranch);
            };
            if query.active_branch_id.as_ref() == Some(record_branch_id) {
            } else if ancestor_distances
                .iter()
                .any(|(branch_id, _)| branch_id == record_branch_id)
            {
                if !query.include_ancestors {
                    return Some(SessionMemoryPromptExclusionReason::WrongBranch);
                }
            } else if query.include_siblings {
            } else if query.active_branch_id.is_some() {
                return Some(SessionMemoryPromptExclusionReason::SiblingBranch);
            } else {
                return Some(SessionMemoryPromptExclusionReason::WrongBranch);
            }
        }
        _ => return Some(SessionMemoryPromptExclusionReason::WrongBranch),
    }
    if query.prompt_context_only {
        match record.status {
            SessionMemoryRecordStatus::Archived => {
                return Some(SessionMemoryPromptExclusionReason::Archived);
            }
            SessionMemoryRecordStatus::Superseded => {
                return Some(SessionMemoryPromptExclusionReason::Superseded);
            }
            SessionMemoryRecordStatus::Active => {}
        }
        if session_memory_policy_disabled(record) {
            return Some(SessionMemoryPromptExclusionReason::PolicyDisabled);
        }
        if session_memory_tool_only(record) {
            return Some(SessionMemoryPromptExclusionReason::ToolOnly);
        }
    }
    None
}

fn increment_session_memory_excluded_count(
    counts: &mut SessionMemoryPromptExcludedCounts,
    reason: SessionMemoryPromptExclusionReason,
) {
    match reason {
        SessionMemoryPromptExclusionReason::WrongBranch => counts.wrong_branch += 1,
        SessionMemoryPromptExclusionReason::SiblingBranch => counts.sibling_branch += 1,
        SessionMemoryPromptExclusionReason::ToolOnly => counts.tool_only += 1,
        SessionMemoryPromptExclusionReason::Archived => counts.archived += 1,
        SessionMemoryPromptExclusionReason::Superseded => counts.superseded += 1,
        SessionMemoryPromptExclusionReason::PolicyDisabled => counts.policy_disabled += 1,
    }
}

fn session_memory_sort_key(
    record: &SessionMemoryRecord,
    query: &BranchAwareSessionMemoryQuery,
    ancestor_distances: &[(ConversationBranchId, u32)],
) -> (u8, u32, u8) {
    let shape_priority = session_memory_shape_prompt_priority(record.shape.shape_id.as_str());
    match record.scope.scope_type {
        MemoryScopeType::ConversationBranch => {
            if query.active_branch_id.as_ref() == record.branch_id.as_ref() {
                (0, 0, shape_priority)
            } else if let Some((_, distance)) =
                record.branch_id.as_ref().and_then(|record_branch| {
                    ancestor_distances
                        .iter()
                        .find(|(branch_id, _)| branch_id == record_branch)
                })
            {
                (1, *distance, shape_priority)
            } else {
                (3, u32::MAX, shape_priority)
            }
        }
        MemoryScopeType::Session => (2, 0, shape_priority),
        _ => (4, u32::MAX, shape_priority),
    }
}

fn session_memory_shape_prompt_priority(shape_id: &str) -> u8 {
    match shape_id {
        "branch_summary" | "session_summary" => 0,
        "user_choice" => 1,
        "session_fact" => 2,
        _ => 3,
    }
}

fn load_branch_ancestor_distances<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    session_id: &SessionId,
    active_branch_id: &Option<ConversationBranchId>,
) -> CoreResult<Vec<(ConversationBranchId, u32)>> {
    let Some(active_branch_id) = active_branch_id else {
        return Ok(Vec::new());
    };
    let active_branch = load_conversation_branch(conn, schema, active_branch_id)?;
    if active_branch.session_id != *session_id {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "active_branch_id does not belong to session_id",
        ));
    }
    let mut ancestors = Vec::new();
    let mut parent = active_branch.parent_branch_id;
    let mut distance = 1;
    while let Some(parent_branch_id) = parent {
        let branch = load_conversation_branch(conn, schema, &parent_branch_id)?;
        if branch.session_id != *session_id {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "conversation branch ancestry crosses session boundary",
            ));
        }
        parent = branch.parent_branch_id.clone();
        ancestors.push((branch.branch_id, distance));
        distance += 1;
    }
    Ok(ancestors)
}

fn session_memory_tool_only(record: &SessionMemoryRecord) -> bool {
    session_memory_json_policy_flag(&record.content, "tool_only")
        || session_memory_json_policy_eq(&record.content, "prompt_policy", "tool_only")
        || record
            .content
            .get("metadata_json")
            .map(|metadata| {
                session_memory_json_policy_flag(metadata, "tool_only")
                    || session_memory_json_policy_eq(metadata, "prompt_policy", "tool_only")
            })
            .unwrap_or(false)
}

fn session_memory_policy_disabled(record: &SessionMemoryRecord) -> bool {
    session_memory_json_policy_flag(&record.content, "prompt_disabled")
        || session_memory_json_policy_eq(&record.content, "prompt_policy", "never_prompt")
        || record
            .content
            .get("metadata_json")
            .map(|metadata| {
                session_memory_json_policy_flag(metadata, "prompt_disabled")
                    || session_memory_json_policy_eq(metadata, "prompt_policy", "never_prompt")
            })
            .unwrap_or(false)
}

fn session_memory_json_policy_flag(value: &serde_json::Value, key: &str) -> bool {
    value
        .get(key)
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

fn session_memory_json_policy_eq(value: &serde_json::Value, key: &str, expected: &str) -> bool {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(|actual| actual == expected)
        .unwrap_or(false)
}

fn session_memory_record_character_estimate(record: &SessionMemoryRecord) -> u64 {
    to_json_text(&record.content)
        .map(|value| value.len() as u64)
        .unwrap_or(0)
}

fn validate_postgres_session_memory_revision_input(
    record_id: &str,
    expected_revision: u64,
    evidence_refs: &[MemoryEvidenceRef],
    confidence: f32,
    durability_rationale: &str,
) -> CoreResult<()> {
    crate::validate_session_memory_record_id(record_id)?;
    if expected_revision == 0 {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "session memory expected_revision must be greater than zero",
        ));
    }
    if evidence_refs.is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "session memory evidence_refs must not be empty",
        ));
    }
    if !(0.0..=1.0).contains(&confidence) || confidence.is_nan() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "session memory confidence must be between 0 and 1",
        ));
    }
    if durability_rationale.trim().is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "session memory durability_rationale must not be empty",
        ));
    }
    Ok(())
}

fn validate_postgres_session_memory_scope<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    session_id: &SessionId,
    scope: &MemoryScope,
    branch_id: &Option<ConversationBranchId>,
) -> CoreResult<()> {
    match scope.scope_type {
        MemoryScopeType::Session => {
            if scope.scope_id != session_id.0 {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "session memory session scope_id must match session_id",
                ));
            }
            if branch_id.is_some() {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "session-scoped memory must not set branch_id",
                ));
            }
        }
        MemoryScopeType::ConversationBranch => {
            let Some(branch_id) = branch_id else {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "conversation_branch memory requires branch_id",
                ));
            };
            if branch_id.0 != scope.scope_id {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "conversation_branch memory scope_id must match branch_id",
                ));
            }
            ensure_branch_belongs_to_session_generic(conn, schema, session_id, branch_id)?;
        }
        _ => {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "session memory scope must be session or conversation_branch",
            ));
        }
    }
    Ok(())
}

fn ensure_branch_belongs_to_session_generic<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    session_id: &SessionId,
    branch_id: &ConversationBranchId,
) -> CoreResult<()> {
    let row = conn
        .query_one(
            &format!(
                "SELECT EXISTS(
                    SELECT 1 FROM {schema}.conversation_branches
                    WHERE session_id = $1 AND branch_id = $2
                )"
            ),
            &[&session_id.0, &branch_id.0],
        )
        .map_err(|error| postgres_error("check PostgreSQL branch session ownership", error))?;
    if row.get::<_, bool>(0) {
        Ok(())
    } else {
        Err(CoreError::new(
            CoreErrorKind::NotFound,
            format!("conversation branch {branch_id} not found for session {session_id}"),
        ))
    }
}

fn get_memory_proposal_by_id<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    proposal_id: &str,
) -> CoreResult<Option<MemoryProposalRecord>> {
    let row = conn
        .query_opt(
            &format!(
                "SELECT record_json
                 FROM {schema}.memory_proposals
                 WHERE proposal_id = $1"
            ),
            &[&proposal_id],
        )
        .map_err(|error| postgres_error("get PostgreSQL memory proposal", error))?;
    row.map(|row| {
        let record_json: String = row.get(0);
        parse_postgres_json(&record_json, "memory proposal record_json")
    })
    .transpose()
}

fn get_memory_proposal_by_dedupe<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    space_id: &str,
    dedupe_key: &str,
) -> CoreResult<Option<MemoryProposalRecord>> {
    let row = conn
        .query_opt(
            &format!(
                "SELECT record_json
                 FROM {schema}.memory_proposals
                 WHERE space_id = $1 AND dedupe_key = $2"
            ),
            &[&space_id, &dedupe_key],
        )
        .map_err(|error| postgres_error("get PostgreSQL memory proposal by dedupe", error))?;
    row.map(|row| {
        let record_json: String = row.get(0);
        parse_postgres_json(&record_json, "memory proposal record_json")
    })
    .transpose()
}

fn insert_memory_proposal_record_in_tx<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    record: &MemoryProposalRecord,
) -> CoreResult<()> {
    let record_json = to_json_text(record)?;
    let status = memory_proposal_status_as_str(record.status).to_string();
    conn.execute(
        &format!(
            "INSERT INTO {schema}.memory_proposals (
                proposal_id,
                space_id,
                status,
                dedupe_key,
                record_json,
                created_at,
                updated_at
             ) VALUES ($1, $2, $3, $4, $5, $6, $7)"
        ),
        &[
            &record.proposal.proposal_id,
            &record.proposal.space_id.0,
            &status,
            &record.proposal.dedupe_key,
            &record_json,
            &record.created_at,
            &record.updated_at,
        ],
    )
    .map_err(|error| postgres_error("insert PostgreSQL memory proposal", error))?;
    Ok(())
}

fn update_memory_proposal_record_in_tx<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    record: &MemoryProposalRecord,
) -> CoreResult<()> {
    let record_json = to_json_text(record)?;
    let status = memory_proposal_status_as_str(record.status).to_string();
    conn.execute(
        &format!(
            "UPDATE {schema}.memory_proposals
             SET status = $2,
                 record_json = $3,
                 updated_at = $4
             WHERE proposal_id = $1"
        ),
        &[
            &record.proposal.proposal_id,
            &status,
            &record_json,
            &record.updated_at,
        ],
    )
    .map_err(|error| postgres_error("update PostgreSQL memory proposal", error))?;
    Ok(())
}

fn list_memory_proposals<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    query: &MemoryProposalQuery,
) -> CoreResult<Vec<MemoryProposalRecord>> {
    let space_id = query.space_id.as_ref().map(|space_id| space_id.0.as_str());
    let status = query
        .status
        .map(memory_proposal_status_as_str)
        .map(str::to_string);
    let dedupe_key = query.dedupe_key.as_deref();
    let (limit, offset) = QueryPage {
        limit: query.limit,
        offset: query.offset,
    }
    .bounded(100, 1_000);
    let rows = conn
        .query(
            &format!(
                "SELECT record_json
                 FROM {schema}.memory_proposals
                 WHERE ($1::TEXT IS NULL OR space_id = $1)
                   AND ($2::TEXT IS NULL OR status = $2)
                   AND ($3::TEXT IS NULL OR dedupe_key = $3)
                 ORDER BY updated_at DESC, proposal_id ASC
                 LIMIT $4 OFFSET $5"
            ),
            &[&space_id, &status, &dedupe_key, &limit, &offset],
        )
        .map_err(|error| postgres_error("list PostgreSQL memory proposals", error))?;
    rows.iter()
        .map(|row| {
            let record_json: String = row.get(0);
            parse_postgres_json(&record_json, "memory proposal record_json")
        })
        .collect()
}

fn insert_memory_governance_decision_in_tx<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    record: &MemoryGovernanceDecisionRecord,
) -> CoreResult<()> {
    let record_json = to_json_text(record)?;
    let decision = memory_governance_decision_as_str(record.decision).to_string();
    conn.execute(
        &format!(
            "INSERT INTO {schema}.memory_governance_decisions (
                decision_id,
                proposal_id,
                decision,
                record_json,
                decided_at
             ) VALUES ($1, $2, $3, $4, $5)"
        ),
        &[
            &record.decision_id,
            &record.proposal_id,
            &decision,
            &record_json,
            &record.decided_at,
        ],
    )
    .map_err(|error| postgres_error("insert PostgreSQL memory governance decision", error))?;
    Ok(())
}

fn update_memory_proposal_review_state(
    proposal: &mut MemoryProposalRecord,
    decision: &MemoryGovernanceDecisionRecord,
) {
    proposal.status = match decision.decision {
        MemoryGovernanceDecisionKind::RoutedToReview => MemoryProposalReviewStatus::PendingReview,
        MemoryGovernanceDecisionKind::Approved => MemoryProposalReviewStatus::Approved,
        MemoryGovernanceDecisionKind::Rejected => MemoryProposalReviewStatus::Rejected,
        MemoryGovernanceDecisionKind::Applied => MemoryProposalReviewStatus::Applied,
    };
    proposal.updated_at = decision.decided_at.clone();
    if matches!(
        decision.decision,
        MemoryGovernanceDecisionKind::Approved | MemoryGovernanceDecisionKind::Rejected
    ) {
        proposal.decided_at = Some(decision.decided_at.clone());
    }
    if decision.decision == MemoryGovernanceDecisionKind::Applied {
        proposal.applied_at = Some(decision.decided_at.clone());
    }
    if decision.resulting_revision.is_some() {
        proposal.resulting_revision = decision.resulting_revision;
    }
}

fn apply_session_memory_proposal_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    proposal: &MemoryProposalEnvelope,
    now: &IsoTimestamp,
) -> CoreResult<u64> {
    if proposal.space_id.as_str() != "session_memory" {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "only session_memory proposals can be applied to session memory records",
        ));
    }
    match proposal.operation {
        MemoryOperation::Add => {
            let write = session_memory_write_from_proposal(tx, schema, proposal, now)?;
            crate::validate_session_memory_write(&write)?;
            validate_postgres_session_memory_scope(
                tx,
                schema,
                &write.session_id,
                &write.scope,
                &write.branch_id,
            )?;
            if get_session_memory_record_in_tx(tx, schema, &write.record_id)?.is_some() {
                return Err(CoreError::new(
                    CoreErrorKind::AlreadyExists,
                    format!("session memory record {} already exists", write.record_id),
                ));
            }
            insert_session_memory_record_in_tx(tx, schema, &write)?;
            Ok(1)
        }
        MemoryOperation::Replace | MemoryOperation::Merge => {
            let record_id = session_memory_proposal_record_id(proposal)?;
            let expected_revision = session_memory_proposal_expected_revision(proposal)?;
            let durability_rationale = session_memory_proposal_rationale(proposal)?;
            validate_postgres_session_memory_revision_input(
                &record_id,
                expected_revision,
                &proposal.evidence_refs,
                proposal.confidence,
                durability_rationale,
            )?;
            let existing =
                active_session_memory_record_for_update(tx, schema, &record_id, expected_revision)?;
            crate::validate_session_memory_shape(&proposal.shape)?;
            crate::validate_session_memory_content(&proposal.shape, &proposal.content)?;
            validate_postgres_session_memory_scope(
                tx,
                schema,
                &existing.session_id,
                &proposal.scope,
                &existing.branch_id,
            )?;
            let next_revision = existing.revision + 1;
            update_session_memory_record_content_in_tx(
                tx,
                schema,
                &SessionMemoryReplace {
                    record_id,
                    expected_revision,
                    content: proposal.content.clone(),
                    evidence_refs: proposal.evidence_refs.clone(),
                    source: proposal.source,
                    confidence: proposal.confidence,
                    durability_rationale: durability_rationale.to_string(),
                    now: now.clone(),
                },
                next_revision,
            )?;
            Ok(next_revision)
        }
        MemoryOperation::Supersede => {
            let record_id = session_memory_proposal_supersedes_record_id(proposal)?;
            let expected_revision = session_memory_proposal_expected_revision(proposal)?;
            let replacement = session_memory_write_from_proposal(tx, schema, proposal, now)?;
            if replacement.supersedes_record_id.as_deref() != Some(record_id.as_str()) {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "session memory supersede proposal must set content.supersedes_record_id",
                ));
            }
            crate::validate_session_memory_write(&replacement)?;
            validate_postgres_session_memory_scope(
                tx,
                schema,
                &replacement.session_id,
                &replacement.scope,
                &replacement.branch_id,
            )?;
            let existing =
                active_session_memory_record_for_update(tx, schema, &record_id, expected_revision)?;
            validate_postgres_session_memory_scope(
                tx,
                schema,
                &existing.session_id,
                &existing.scope,
                &existing.branch_id,
            )?;
            if get_session_memory_record_in_tx(tx, schema, &replacement.record_id)?.is_some() {
                return Err(CoreError::new(
                    CoreErrorKind::AlreadyExists,
                    format!(
                        "session memory replacement record {} already exists",
                        replacement.record_id
                    ),
                ));
            }
            insert_session_memory_record_in_tx(tx, schema, &replacement)?;
            mark_session_memory_superseded_in_tx(
                tx,
                schema,
                &existing.record_id,
                &replacement.record_id,
                existing.revision + 1,
                now,
            )?;
            Ok(1)
        }
        MemoryOperation::Archive => {
            let record_id = session_memory_proposal_record_id(proposal)?;
            let expected_revision = session_memory_proposal_expected_revision(proposal)?;
            let existing =
                active_session_memory_record_for_update(tx, schema, &record_id, expected_revision)?;
            validate_postgres_session_memory_scope(
                tx,
                schema,
                &existing.session_id,
                &proposal.scope,
                &existing.branch_id,
            )?;
            let next_revision = existing.revision + 1;
            archive_session_memory_record_in_tx(
                tx,
                schema,
                &SessionMemoryArchive {
                    record_id,
                    expected_revision,
                    reason: session_memory_proposal_archive_reason(proposal),
                    now: now.clone(),
                },
                next_revision,
            )?;
            Ok(next_revision)
        }
        _ => Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            format!(
                "session memory proposal operation {:?} cannot be applied",
                proposal.operation
            ),
        )),
    }
}

fn session_memory_write_from_proposal(
    tx: &mut Transaction<'_>,
    schema: &str,
    proposal: &MemoryProposalEnvelope,
    now: &IsoTimestamp,
) -> CoreResult<SessionMemoryRecordWrite> {
    let record_id = session_memory_proposal_record_id(proposal)?;
    let session_id = session_id_for_session_memory_proposal(tx, schema, proposal)?;
    let branch_id = match proposal.scope.scope_type {
        MemoryScopeType::Session => None,
        MemoryScopeType::ConversationBranch => {
            Some(ConversationBranchId::new(proposal.scope.scope_id.clone()))
        }
        _ => {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "session memory proposal scope must be session or conversation_branch",
            ));
        }
    };
    Ok(SessionMemoryRecordWrite {
        record_id,
        session_id,
        scope: proposal.scope.clone(),
        branch_id,
        shape: proposal.shape.clone(),
        content: proposal.content.clone(),
        evidence_refs: proposal.evidence_refs.clone(),
        source: proposal.source,
        confidence: proposal.confidence,
        durability_rationale: session_memory_proposal_rationale(proposal)?.to_string(),
        supersedes_record_id: session_memory_proposal_supersedes_record_id(proposal).ok(),
        now: now.clone(),
    })
}

fn session_id_for_session_memory_proposal(
    tx: &mut Transaction<'_>,
    schema: &str,
    proposal: &MemoryProposalEnvelope,
) -> CoreResult<SessionId> {
    match proposal.scope.scope_type {
        MemoryScopeType::Session => Ok(SessionId::new(proposal.scope.scope_id.clone())),
        MemoryScopeType::ConversationBranch => {
            let branch_id = ConversationBranchId::new(proposal.scope.scope_id.clone());
            Ok(load_conversation_branch(tx, schema, &branch_id)?.session_id)
        }
        _ => Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "session memory proposal scope must be session or conversation_branch",
        )),
    }
}

fn validate_postgres_memory_governance_decision(
    decision: &MemoryGovernanceDecisionInput,
) -> CoreResult<()> {
    validate_postgres_identifier("memory governance decision id", &decision.decision_id)?;
    validate_postgres_identifier("memory governance proposal id", &decision.proposal_id)?;
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

fn validate_postgres_memory_governance_transition(
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

fn session_memory_proposal_record_id(proposal: &MemoryProposalEnvelope) -> CoreResult<String> {
    let record_id = proposal
        .content
        .get("record_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::InvalidInput,
                "session memory proposal content.record_id is required",
            )
        })?;
    crate::validate_session_memory_record_id(record_id)?;
    Ok(record_id.to_string())
}

fn session_memory_proposal_expected_revision(proposal: &MemoryProposalEnvelope) -> CoreResult<u64> {
    proposal
        .content
        .get("expected_revision")
        .and_then(serde_json::Value::as_u64)
        .filter(|revision| *revision > 0)
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::InvalidInput,
                "session memory proposal content.expected_revision must be greater than zero",
            )
        })
}

fn session_memory_proposal_supersedes_record_id(
    proposal: &MemoryProposalEnvelope,
) -> CoreResult<String> {
    let record_id = proposal
        .content
        .get("supersedes_record_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::InvalidInput,
                "session memory supersede proposal requires content.supersedes_record_id",
            )
        })?;
    crate::validate_session_memory_record_id(record_id)?;
    Ok(record_id.to_string())
}

fn session_memory_proposal_archive_reason(proposal: &MemoryProposalEnvelope) -> Option<String> {
    proposal
        .content
        .get("archive_reason")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn session_memory_proposal_rationale(proposal: &MemoryProposalEnvelope) -> CoreResult<&str> {
    proposal
        .durability_rationale
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::InvalidInput,
                "session memory proposal durability_rationale is required",
            )
        })
}

fn selected_governance_mode(requested: MemoryGovernanceMode) -> MemoryGovernanceMode {
    requested
}

fn memory_proposal_status_as_str(status: MemoryProposalReviewStatus) -> &'static str {
    match status {
        MemoryProposalReviewStatus::PendingReview => "pending_review",
        MemoryProposalReviewStatus::Approved => "approved",
        MemoryProposalReviewStatus::Rejected => "rejected",
        MemoryProposalReviewStatus::Applied => "applied",
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

fn session_memory_status_as_str(status: SessionMemoryRecordStatus) -> &'static str {
    match status {
        SessionMemoryRecordStatus::Active => "active",
        SessionMemoryRecordStatus::Superseded => "superseded",
        SessionMemoryRecordStatus::Archived => "archived",
    }
}

fn profile_registry_lifecycle_status_as_str(
    status: ProfileRegistryLifecycleStatus,
) -> &'static str {
    match status {
        ProfileRegistryLifecycleStatus::Active => "active",
        ProfileRegistryLifecycleStatus::Paused => "paused",
        ProfileRegistryLifecycleStatus::Decommissioned => "decommissioned",
        ProfileRegistryLifecycleStatus::Archived => "archived",
    }
}

fn get_model_provider_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    alias: &str,
) -> CoreResult<Option<ModelProviderRecord>> {
    get_model_provider_in_client(tx, schema, alias)
}

fn get_model_provider_in_client<C: GenericClient>(
    client: &mut C,
    schema: &str,
    alias: &str,
) -> CoreResult<Option<ModelProviderRecord>> {
    let row = client
        .query_opt(
            &format!(
                "SELECT provider_json, secret_ciphertext, secret_updated_at
                 FROM {schema}.model_providers
                 WHERE alias = $1"
            ),
            &[&alias],
        )
        .map_err(|error| postgres_error("get PostgreSQL model provider", error))?;
    row.as_ref().map(row_to_model_provider).transpose()
}

fn get_model_provider_secret_in_client<C: GenericClient>(
    client: &mut C,
    schema: &str,
    alias: &str,
) -> CoreResult<Option<String>> {
    client
        .query_opt(
            &format!(
                "SELECT secret_ciphertext
                 FROM {schema}.model_providers
                 WHERE alias = $1"
            ),
            &[&alias],
        )
        .map_err(|error| postgres_error("get PostgreSQL model provider secret", error))
        .map(|row| row.and_then(|row| row.get(0)))
}

fn upsert_model_provider_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    write: &ModelProviderWrite,
    existing: Option<&ModelProviderRecord>,
) -> CoreResult<()> {
    let revision = existing.map_or(1, |record| record.revision + 1);
    let created_at = existing
        .map(|record| record.created_at.clone())
        .unwrap_or_else(|| write.now.clone());
    let current_secret: Option<String> = if write.clear_secret || write.secret.is_some() {
        None
    } else {
        tx.query_opt(
            &format!(
                "SELECT secret_ciphertext
                 FROM {schema}.model_providers
                 WHERE alias = $1"
            ),
            &[&write.alias],
        )
        .map_err(|error| postgres_error("load preserved PostgreSQL model provider secret", error))?
        .and_then(|row| row.get(0))
    };
    let secret_ciphertext = if write.clear_secret {
        None
    } else {
        write.secret.clone().or(current_secret)
    };
    let secret_updated_at = if write.clear_secret {
        None
    } else if write.secret.is_some() {
        Some(write.now.clone())
    } else {
        existing.and_then(|record| record.credential.updated_at.clone())
    };
    let record = ModelProviderRecord {
        alias: write.alias.clone(),
        status: write.status,
        protocol: write.protocol,
        provider_kind: write.provider_kind.clone(),
        display_name: write.display_name.clone(),
        description: write.description.clone(),
        base_url: write.base_url.clone(),
        model_id: write.model_id.clone(),
        context_window_tokens: write.context_window_tokens,
        max_output_tokens: write.max_output_tokens,
        temperature_milli: write.temperature_milli,
        reasoning_effort: write.reasoning_effort.clone(),
        reasoning_format: write.reasoning_format.clone(),
        credential: ModelProviderCredential {
            has_secret: secret_ciphertext.is_some(),
            secret_ref: secret_ciphertext
                .as_ref()
                .map(|_| format!("db://model_providers/{}/secret", write.alias)),
            updated_at: secret_updated_at.clone(),
        },
        metadata_json: write.metadata_json.clone(),
        revision,
        created_at,
        updated_at: write.now.clone(),
    };
    let provider_json = to_json_text(&record)?;
    tx.execute(
        &format!(
            "INSERT INTO {schema}.model_providers (
                alias,
                status,
                protocol,
                provider_json,
                secret_ciphertext,
                secret_updated_at,
                revision,
                created_at,
                updated_at
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT(alias) DO UPDATE SET
                status = excluded.status,
                protocol = excluded.protocol,
                provider_json = excluded.provider_json,
                secret_ciphertext = excluded.secret_ciphertext,
                secret_updated_at = excluded.secret_updated_at,
                revision = excluded.revision,
                updated_at = excluded.updated_at"
        ),
        &[
            &record.alias,
            &model_provider_status_as_str(record.status).to_string(),
            &model_provider_protocol_as_str(record.protocol).to_string(),
            &provider_json,
            &secret_ciphertext,
            &secret_updated_at,
            &(record.revision as i64),
            &record.created_at,
            &record.updated_at,
        ],
    )
    .map_err(|error| postgres_error("upsert PostgreSQL model provider", error))?;
    Ok(())
}

fn row_to_model_provider(row: &Row) -> CoreResult<ModelProviderRecord> {
    let provider_json: String = row.get(0);
    let secret_ciphertext: Option<String> = row.get(1);
    let secret_updated_at: Option<String> = row.get(2);
    let mut record: ModelProviderRecord =
        parse_postgres_json(&provider_json, "model provider provider_json")?;
    record.credential = ModelProviderCredential {
        has_secret: secret_ciphertext.is_some(),
        secret_ref: secret_ciphertext
            .as_ref()
            .map(|_| format!("db://model_providers/{}/secret", record.alias)),
        updated_at: secret_updated_at,
    };
    Ok(record)
}

fn model_provider_status_as_str(status: ModelProviderStatus) -> &'static str {
    match status {
        ModelProviderStatus::Active => "active",
        ModelProviderStatus::Disabled => "disabled",
        ModelProviderStatus::Archived => "archived",
    }
}

fn model_provider_protocol_as_str(protocol: ModelProviderProtocol) -> &'static str {
    match protocol {
        ModelProviderProtocol::Responses => "responses",
        ModelProviderProtocol::ChatCompletions => "chat_completions",
    }
}

fn row_to_channel_binding_record(row: &Row) -> CoreResult<ChannelBindingRecord> {
    let status: String = row.get(14);
    let provenance_json: String = row.get(16);
    Ok(ChannelBindingRecord {
        binding_id: row.get(0),
        adapter_id: AdapterId(row.get(1)),
        provider: row.get(2),
        agent_id: AgentId(row.get(3)),
        instance_id: row.get::<_, Option<String>>(4).map(AgentInstanceId),
        session_id: row.get::<_, Option<String>>(5).map(SessionId),
        profile_id: ProfileId(row.get(6)),
        external_channel_id: row.get(7),
        external_thread_id: row.get(8),
        external_user_id: row.get(9),
        provider_subscription_id: row.get(10),
        cursor: row.get(11),
        membership_state: row.get(12),
        presence_state: row.get(13),
        status: external_binding_status_from_str(&status)?,
        degraded_reason: row.get(15),
        provenance: parse_postgres_json(&provenance_json, "channel binding provenance_json")?,
        created_at: row.get(17),
        updated_at: row.get(18),
    })
}

fn row_to_mcp_binding_record(row: &Row) -> CoreResult<McpBindingRecord> {
    let server_names_json: String = row.get(6);
    let status: String = row.get(11);
    let diagnostics_json: String = row.get(13);
    Ok(McpBindingRecord {
        binding_id: row.get(0),
        adapter_id: AdapterId(row.get(1)),
        agent_id: AgentId(row.get(2)),
        instance_id: row.get::<_, Option<String>>(3).map(AgentInstanceId),
        session_id: row.get::<_, Option<String>>(4).map(SessionId),
        profile_id: ProfileId(row.get(5)),
        server_names: parse_postgres_json(&server_names_json, "MCP binding server_names_json")?,
        endpoint_ref: row.get(7),
        transport: row.get(8),
        tool_profile_key: row.get(9),
        discovered_tool_revision: row.get(10),
        status: external_binding_status_from_str(&status)?,
        degraded_reason: row.get(12),
        diagnostics: parse_postgres_json(&diagnostics_json, "MCP binding diagnostics_json")?,
        created_at: row.get(14),
        updated_at: row.get(15),
    })
}

fn external_binding_status_as_str(status: ExternalBindingStatus) -> &'static str {
    match status {
        ExternalBindingStatus::Active => "active",
        ExternalBindingStatus::Degraded => "degraded",
        ExternalBindingStatus::Disconnected => "disconnected",
        ExternalBindingStatus::Archived => "archived",
    }
}

fn external_binding_status_from_str(raw: &str) -> CoreResult<ExternalBindingStatus> {
    match raw {
        "active" => Ok(ExternalBindingStatus::Active),
        "degraded" => Ok(ExternalBindingStatus::Degraded),
        "disconnected" => Ok(ExternalBindingStatus::Disconnected),
        "archived" => Ok(ExternalBindingStatus::Archived),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("unknown external binding status {other}"),
        )),
    }
}

fn row_to_provider_wire_state_record(row: &Row) -> CoreResult<ProviderWireStateRecord> {
    let payload_json: String = row.get(7);
    let invalidation_reason = row
        .get::<_, Option<String>>(14)
        .as_deref()
        .map(provider_wire_state_invalidation_reason_from_str)
        .transpose()?;
    Ok(ProviderWireStateRecord {
        row_id: row.get(0),
        key: ProviderWireStateKey {
            session_id: crate::SessionId(row.get(1)),
            module_id: row.get(2),
            strategy_id: row.get(3),
        },
        profile_fingerprint: row.get(4),
        provider_fingerprint: row.get(5),
        payload_version: row.get(6),
        payload_json: from_json_text(&payload_json).map_err(|error| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("parse PostgreSQL provider wire payload_json: {error}"),
            )
        })?,
        payload_encoding: row.get(8),
        created_at: row.get(9),
        updated_at: row.get(10),
        expires_at: row.get(11),
        last_wake_id: row.get(12),
        invalidated_at: row.get(13),
        invalidation_reason,
    })
}

fn row_to_provider_wire_state_diagnostic(row: &Row) -> CoreResult<ProviderWireStateDiagnostic> {
    let payload_bytes: i64 = row.get(4);
    if payload_bytes < 0 {
        return Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("invalid PostgreSQL provider wire payload byte count {payload_bytes}"),
        ));
    }
    Ok(ProviderWireStateDiagnostic {
        key: ProviderWireStateKey {
            session_id: crate::SessionId(row.get(0)),
            module_id: row.get(1),
            strategy_id: row.get(2),
        },
        payload_version: row.get(3),
        payload_bytes: payload_bytes as u64,
        created_at: row.get(5),
        updated_at: row.get(6),
        expires_at: row.get(7),
        last_wake_id: row.get(8),
        invalidated_at: row.get(9),
        invalidation_reason: row.get(10),
    })
}

fn invalidate_provider_wire_states_for_session_except_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    key: &ProviderWireStateKey,
    now: &IsoTimestamp,
) -> CoreResult<()> {
    tx.execute(
        &format!(
            "UPDATE {schema}.provider_wire_states
             SET invalidated_at = $4,
                 updated_at = $4,
                 invalidation_reason = CASE
                     WHEN module_id != $2 THEN 'module_changed'
                     ELSE 'strategy_changed'
                 END
             WHERE session_id = $1
               AND invalidated_at IS NULL
               AND (module_id != $2 OR strategy_id != $3)"
        ),
        &[&key.session_id.0, &key.module_id, &key.strategy_id, now],
    )
    .map_err(|error| postgres_error("invalidate changed PostgreSQL provider wire states", error))?;
    Ok(())
}

fn invalidate_current_provider_wire_state_for_key_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    key: &ProviderWireStateKey,
    now: &IsoTimestamp,
    reason: ProviderWireStateInvalidationReason,
) -> CoreResult<()> {
    let reason = provider_wire_state_invalidation_reason_as_str(reason);
    tx.execute(
        &format!(
            "UPDATE {schema}.provider_wire_states
             SET invalidated_at = $4,
                 updated_at = $4,
                 invalidation_reason = $5
             WHERE session_id = $1
               AND module_id = $2
               AND strategy_id = $3
               AND invalidated_at IS NULL"
        ),
        &[
            &key.session_id.0,
            &key.module_id,
            &key.strategy_id,
            now,
            &reason,
        ],
    )
    .map_err(|error| postgres_error("invalidate current PostgreSQL provider wire state", error))?;
    Ok(())
}

fn invalidate_provider_wire_state_by_row_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    row_id: i64,
    now: &IsoTimestamp,
    reason: ProviderWireStateInvalidationReason,
) -> CoreResult<()> {
    let reason = provider_wire_state_invalidation_reason_as_str(reason);
    tx.execute(
        &format!(
            "UPDATE {schema}.provider_wire_states
             SET invalidated_at = $2,
                 updated_at = $2,
                 invalidation_reason = $3
             WHERE row_id = $1
               AND invalidated_at IS NULL"
        ),
        &[&row_id, now, &reason],
    )
    .map_err(|error| postgres_error("invalidate PostgreSQL provider wire state row", error))?;
    Ok(())
}

fn load_current_provider_wire_state_by_key(
    tx: &mut Transaction<'_>,
    schema: &str,
    key: &ProviderWireStateKey,
) -> CoreResult<Option<ProviderWireStateRecord>> {
    let row = tx
        .query_opt(
            &format!(
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
                 FROM {schema}.provider_wire_states
                 WHERE session_id = $1
                   AND module_id = $2
                   AND strategy_id = $3
                   AND invalidated_at IS NULL
                 LIMIT 1"
            ),
            &[&key.session_id.0, &key.module_id, &key.strategy_id],
        )
        .map_err(|error| postgres_error("load current PostgreSQL provider wire state", error))?;
    row.as_ref()
        .map(row_to_provider_wire_state_record)
        .transpose()
}

fn load_provider_wire_state_by_row_id(
    tx: &mut Transaction<'_>,
    schema: &str,
    row_id: i64,
) -> CoreResult<ProviderWireStateRecord> {
    let row = tx
        .query_one(
            &format!(
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
                 FROM {schema}.provider_wire_states
                 WHERE row_id = $1"
            ),
            &[&row_id],
        )
        .map_err(|error| postgres_error("load PostgreSQL provider wire state by row id", error))?;
    row_to_provider_wire_state_record(&row)
}

fn load_expired_current_provider_wire_states(
    tx: &mut Transaction<'_>,
    schema: &str,
    now: &IsoTimestamp,
) -> CoreResult<Vec<ProviderWireStateRecord>> {
    let rows = tx
        .query(
            &format!(
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
                 FROM {schema}.provider_wire_states
                 WHERE invalidated_at IS NULL
                   AND expires_at IS NOT NULL
                   AND expires_at <= $1
                 ORDER BY expires_at ASC, row_id ASC"
            ),
            &[now],
        )
        .map_err(|error| {
            postgres_error(
                "load expired current PostgreSQL provider wire states",
                error,
            )
        })?;
    rows.iter().map(row_to_provider_wire_state_record).collect()
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
) -> CoreResult<ProviderWireStateInvalidationReason> {
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
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("unknown provider wire state invalidation reason {other}"),
        )),
    }
}

fn message_variant_source_as_str(source: MessageVariantSource) -> &'static str {
    match source {
        MessageVariantSource::Primary => "primary",
        MessageVariantSource::Alternate => "alternate",
    }
}

fn message_variant_source_from_str(raw: &str) -> CoreResult<MessageVariantSource> {
    match raw {
        "primary" => Ok(MessageVariantSource::Primary),
        "alternate" => Ok(MessageVariantSource::Alternate),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("unknown message variant source {other}"),
        )),
    }
}

fn message_variant_status_as_str(status: MessageVariantStatus) -> &'static str {
    match status {
        MessageVariantStatus::Active => "active",
        MessageVariantStatus::Deleted => "deleted",
    }
}

fn message_variant_status_from_str(raw: &str) -> CoreResult<MessageVariantStatus> {
    match raw {
        "active" => Ok(MessageVariantStatus::Active),
        "deleted" => Ok(MessageVariantStatus::Deleted),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("unknown message variant status {other}"),
        )),
    }
}

fn durable_message_status_as_str(status: DurableMessageStatus) -> &'static str {
    match status {
        DurableMessageStatus::Created => "created",
        DurableMessageStatus::Streaming => "streaming",
        DurableMessageStatus::Completed => "completed",
        DurableMessageStatus::Failed => "failed",
        DurableMessageStatus::Deleted => "deleted",
    }
}

fn durable_message_status_from_str(raw: &str) -> CoreResult<DurableMessageStatus> {
    match raw {
        "created" => Ok(DurableMessageStatus::Created),
        "streaming" => Ok(DurableMessageStatus::Streaming),
        "completed" => Ok(DurableMessageStatus::Completed),
        "failed" => Ok(DurableMessageStatus::Failed),
        "deleted" => Ok(DurableMessageStatus::Deleted),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("unknown durable message status {other}"),
        )),
    }
}

fn conversation_snapshot_source_as_str(source: ConversationSnapshotSource) -> &'static str {
    match source {
        ConversationSnapshotSource::User => "user",
        ConversationSnapshotSource::System => "system",
        ConversationSnapshotSource::Import => "import",
    }
}

fn conversation_snapshot_source_from_str(raw: &str) -> CoreResult<ConversationSnapshotSource> {
    match raw {
        "user" => Ok(ConversationSnapshotSource::User),
        "system" => Ok(ConversationSnapshotSource::System),
        "import" => Ok(ConversationSnapshotSource::Import),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("unknown conversation snapshot source {other}"),
        )),
    }
}

fn insert_roleplay_lore_record<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    write: &RoleplayLoreWrite,
) -> CoreResult<()> {
    let content_json = to_json_text(&write.content)?;
    let evidence_refs_json = to_json_text(&write.evidence_refs)?;
    conn.execute(
        &format!(
            "INSERT INTO {schema}.module_roleplay_lore_records (
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
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 1, $11, $12, ($13::text)::jsonb, ($14::text)::jsonb, $15, $16, $17, $18, NULL, NULL, NULL, $19, $19)"
        ),
        &[
            &write.record_id,
            &write.world_id,
            &write.entity_id,
            &write.session_id.as_ref().map(|value| value.0.as_str()),
            &write.branch_id.as_ref().map(|value| value.0.as_str()),
            &write.shape.shape_id.0,
            &(write.shape.version as i64),
            &roleplay_lore_canon_status_as_str(write.canon_status),
            &roleplay_lore_visibility_as_str(write.visibility),
            &roleplay_lore_record_status_as_str(RoleplayLoreRecordStatus::Active),
            &write.title,
            &write.body,
            &content_json,
            &evidence_refs_json,
            &crate::memory_proposal_source_as_str(write.source),
            &(write.confidence as f64),
            &write.durability_rationale,
            &write.supersedes_record_id,
            &write.now,
        ],
    )
    .map_err(|error| postgres_error("insert PostgreSQL roleplay lore record", error))?;
    Ok(())
}

fn update_roleplay_lore_record<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    replace: &RoleplayLoreReplace,
    next_revision: u64,
) -> CoreResult<()> {
    let content_json = to_json_text(&replace.write.content)?;
    let evidence_refs_json = to_json_text(&replace.write.evidence_refs)?;
    conn.execute(
        &format!(
            "UPDATE {schema}.module_roleplay_lore_records
             SET world_id = $2,
                 entity_id = $3,
                 session_id = $4,
                 branch_id = $5,
                 shape_id = $6,
                 shape_version = $7,
                 canon_status = $8,
                 visibility = $9,
                 revision = $10,
                 title = $11,
                 body = $12,
                 content_json = ($13::text)::jsonb,
                 evidence_refs_json = ($14::text)::jsonb,
                 source = $15,
                 confidence = $16,
                 durability_rationale = $17,
                 updated_at = $18
             WHERE record_id = $1"
        ),
        &[
            &replace.write.record_id,
            &replace.write.world_id,
            &replace.write.entity_id,
            &replace
                .write
                .session_id
                .as_ref()
                .map(|value| value.0.as_str()),
            &replace
                .write
                .branch_id
                .as_ref()
                .map(|value| value.0.as_str()),
            &replace.write.shape.shape_id.0,
            &(replace.write.shape.version as i64),
            &roleplay_lore_canon_status_as_str(replace.write.canon_status),
            &roleplay_lore_visibility_as_str(replace.write.visibility),
            &(next_revision as i64),
            &replace.write.title,
            &replace.write.body,
            &content_json,
            &evidence_refs_json,
            &crate::memory_proposal_source_as_str(replace.write.source),
            &(replace.write.confidence as f64),
            &replace.write.durability_rationale,
            &replace.write.now,
        ],
    )
    .map_err(|error| postgres_error("update PostgreSQL roleplay lore record", error))?;
    Ok(())
}

fn mark_roleplay_lore_superseded<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    record_id: &str,
    replacement_record_id: &str,
    next_revision: u64,
    now: &IsoTimestamp,
) -> CoreResult<()> {
    conn.execute(
        &format!(
            "UPDATE {schema}.module_roleplay_lore_records
             SET status = $2,
                 revision = $3,
                 superseded_by_record_id = $4,
                 updated_at = $5
             WHERE record_id = $1"
        ),
        &[
            &record_id,
            &roleplay_lore_record_status_as_str(RoleplayLoreRecordStatus::Superseded),
            &(next_revision as i64),
            &replacement_record_id,
            &now,
        ],
    )
    .map_err(|error| postgres_error("mark PostgreSQL roleplay lore superseded", error))?;
    Ok(())
}

fn tombstone_roleplay_lore_record<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    tombstone: &RoleplayLoreTombstone,
    next_revision: u64,
) -> CoreResult<()> {
    conn.execute(
        &format!(
            "UPDATE {schema}.module_roleplay_lore_records
             SET status = $2,
                 revision = $3,
                 tombstoned_at = $4,
                 tombstone_reason = $5,
                 updated_at = $4
             WHERE record_id = $1"
        ),
        &[
            &tombstone.record_id,
            &roleplay_lore_record_status_as_str(RoleplayLoreRecordStatus::Tombstoned),
            &(next_revision as i64),
            &tombstone.now,
            &tombstone.reason,
        ],
    )
    .map_err(|error| postgres_error("tombstone PostgreSQL roleplay lore record", error))?;
    Ok(())
}

fn active_roleplay_lore_record_for_update(
    tx: &mut Transaction<'_>,
    schema: &str,
    record_id: &str,
    expected_revision: u64,
) -> CoreResult<RoleplayLoreRecord> {
    validate_roleplay_lore_record_id(record_id)?;
    if expected_revision == 0 {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "roleplay lore expected_revision must be greater than zero",
        ));
    }
    let record = get_roleplay_lore_record_for_update(tx, schema, record_id)?.ok_or_else(|| {
        CoreError::new(
            CoreErrorKind::NotFound,
            format!("roleplay lore record {record_id} not found"),
        )
    })?;
    if record.status != RoleplayLoreRecordStatus::Active {
        return Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            format!("roleplay lore record {record_id} is not active"),
        ));
    }
    if record.revision != expected_revision {
        return Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            format!(
                "roleplay lore revision mismatch for {record_id}: expected {}, found {}",
                expected_revision, record.revision
            ),
        ));
    }
    Ok(record)
}

fn get_roleplay_lore_record_for_update(
    tx: &mut Transaction<'_>,
    schema: &str,
    record_id: &str,
) -> CoreResult<Option<RoleplayLoreRecord>> {
    let row = tx
        .query_opt(
            &format!(
                "SELECT record_id,
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
                        content_json::text,
                        evidence_refs_json::text,
                        source,
                        confidence,
                        durability_rationale,
                        supersedes_record_id,
                        superseded_by_record_id,
                        tombstoned_at,
                        tombstone_reason,
                        created_at,
                        updated_at
                 FROM {schema}.module_roleplay_lore_records
                 WHERE record_id = $1
                 FOR UPDATE"
            ),
            &[&record_id],
        )
        .map_err(|error| postgres_error("load PostgreSQL roleplay lore record", error))?;
    row.as_ref().map(row_to_roleplay_lore_record).transpose()
}

fn get_roleplay_lore_record<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    record_id: &str,
) -> CoreResult<Option<RoleplayLoreRecord>> {
    let row = conn
        .query_opt(
            &format!(
                "SELECT record_id,
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
                        content_json::text,
                        evidence_refs_json::text,
                        source,
                        confidence,
                        durability_rationale,
                        supersedes_record_id,
                        superseded_by_record_id,
                        tombstoned_at,
                        tombstone_reason,
                        created_at,
                        updated_at
                 FROM {schema}.module_roleplay_lore_records
                 WHERE record_id = $1"
            ),
            &[&record_id],
        )
        .map_err(|error| postgres_error("load PostgreSQL roleplay lore record", error))?;
    row.as_ref().map(row_to_roleplay_lore_record).transpose()
}

fn query_roleplay_lore_records<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    query: &RoleplayLoreQuery,
) -> CoreResult<Vec<RoleplayLoreRecord>> {
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let canon_status = query.canon_status.map(roleplay_lore_canon_status_as_str);
    let visibility = query.visibility.map(roleplay_lore_visibility_as_str);
    let provenance_like = query
        .provenance_ref_id
        .as_ref()
        .map(|value| postgres_like_contains(value));
    let rows = conn
        .query(
            &format!(
                "SELECT record_id,
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
                        content_json::text,
                        evidence_refs_json::text,
                        source,
                        confidence,
                        durability_rationale,
                        supersedes_record_id,
                        superseded_by_record_id,
                        tombstoned_at,
                        tombstone_reason,
                        created_at,
                        updated_at
                 FROM {schema}.module_roleplay_lore_records
                 WHERE ($1::text IS NULL OR world_id = $1)
                   AND ($2::text IS NULL OR entity_id = $2)
                   AND ($3::text IS NULL OR canon_status = $3)
                   AND ($4::text IS NULL OR visibility = $4)
                   AND ($5::text IS NULL OR shape_id = $5)
                   AND ($6::boolean OR status != 'superseded')
                   AND ($7::boolean OR status != 'tombstoned')
                   AND ($8::text IS NULL OR search_vector @@ plainto_tsquery('simple', $8))
                   AND (
                        $9::text IS NULL OR EXISTS (
                            SELECT 1
                            FROM {schema}.module_roleplay_lore_provenance_events p
                            WHERE p.record_id = module_roleplay_lore_records.record_id
                              AND p.evidence_refs_json::text LIKE $10 ESCAPE '\\'
                        )
                   )
                 ORDER BY updated_at DESC, record_id ASC
                 LIMIT $11 OFFSET $12"
            ),
            &[
                &query.world_id,
                &query.entity_id,
                &canon_status,
                &visibility,
                &query.shape_id,
                &query.include_superseded,
                &query.include_tombstoned,
                &query.query,
                &query.provenance_ref_id,
                &provenance_like,
                &limit,
                &offset,
            ],
        )
        .map_err(|error| postgres_error("query PostgreSQL roleplay lore records", error))?;
    rows.iter().map(row_to_roleplay_lore_record).collect()
}

fn insert_roleplay_lore_provenance_event<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    event: &RoleplayLoreProvenanceEvent,
) -> CoreResult<()> {
    let evidence_refs_json = to_json_text(&event.evidence_refs)?;
    conn.execute(
        &format!(
            "INSERT INTO {schema}.module_roleplay_lore_provenance_events (
                event_id,
                record_id,
                world_id,
                evidence_refs_json,
                source,
                actor,
                note,
                created_at
             ) VALUES ($1, $2, $3, ($4::text)::jsonb, $5, $6, $7, $8)"
        ),
        &[
            &event.event_id,
            &event.record_id,
            &event.world_id,
            &evidence_refs_json,
            &crate::memory_proposal_source_as_str(event.source),
            &event.actor,
            &event.note,
            &event.created_at,
        ],
    )
    .map_err(|error| postgres_error("insert PostgreSQL roleplay lore provenance event", error))?;
    Ok(())
}

fn roleplay_lore_provenance_events<C: GenericClient>(
    conn: &mut C,
    schema: &str,
    record_id: &str,
) -> CoreResult<Vec<RoleplayLoreProvenanceEvent>> {
    let rows = conn
        .query(
            &format!(
                "SELECT event_id,
                        record_id,
                        world_id,
                        evidence_refs_json::text,
                        source,
                        actor,
                        note,
                        created_at
                 FROM {schema}.module_roleplay_lore_provenance_events
                 WHERE record_id = $1
                 ORDER BY created_at ASC, event_id ASC"
            ),
            &[&record_id],
        )
        .map_err(|error| {
            postgres_error("query PostgreSQL roleplay lore provenance events", error)
        })?;
    rows.iter()
        .map(row_to_roleplay_lore_provenance_event)
        .collect()
}

fn row_to_roleplay_lore_record(row: &Row) -> CoreResult<RoleplayLoreRecord> {
    let shape_version: i64 = row.get(6);
    let revision: i64 = row.get(10);
    let content_json: String = row.get(13);
    let evidence_refs_json: String = row.get(14);
    let source: String = row.get(15);
    let canon_status: String = row.get(7);
    let visibility: String = row.get(8);
    let status: String = row.get(9);
    Ok(RoleplayLoreRecord {
        record_id: row.get(0),
        world_id: row.get(1),
        entity_id: row.get(2),
        session_id: row.get::<_, Option<String>>(3).map(SessionId::new),
        branch_id: row
            .get::<_, Option<String>>(4)
            .map(ConversationBranchId::new),
        shape: MemoryRecordShapeRef {
            shape_id: MemoryRecordShapeId::new(row.get::<_, String>(5))?,
            version: shape_version as u32,
        },
        canon_status: crate::parse_roleplay_lore_canon_status(&canon_status)?,
        visibility: crate::parse_roleplay_lore_visibility(&visibility)?,
        status: crate::parse_roleplay_lore_record_status(&status)?,
        revision: revision as u64,
        title: row.get(11),
        body: row.get(12),
        content: parse_postgres_json(&content_json, "roleplay lore content_json")?,
        evidence_refs: from_json_text(&evidence_refs_json).map_err(|error| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("parse PostgreSQL roleplay lore evidence_refs_json: {error}"),
            )
        })?,
        source: crate::parse_memory_proposal_source(&source)?,
        confidence: row.get::<_, f64>(16) as f32,
        durability_rationale: row.get(17),
        supersedes_record_id: row.get(18),
        superseded_by_record_id: row.get(19),
        tombstoned_at: row.get(20),
        tombstone_reason: row.get(21),
        created_at: row.get(22),
        updated_at: row.get(23),
    })
}

fn row_to_roleplay_lore_provenance_event(row: &Row) -> CoreResult<RoleplayLoreProvenanceEvent> {
    let evidence_refs_json: String = row.get(3);
    let source: String = row.get(4);
    Ok(RoleplayLoreProvenanceEvent {
        event_id: row.get(0),
        record_id: row.get(1),
        world_id: row.get(2),
        evidence_refs: from_json_text(&evidence_refs_json).map_err(|error| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("parse PostgreSQL roleplay lore provenance evidence_refs_json: {error}"),
            )
        })?,
        source: crate::parse_memory_proposal_source(&source)?,
        actor: row.get(5),
        note: row.get(6),
        created_at: row.get(7),
    })
}

fn parse_postgres_json<T>(value: &str, label: &str) -> CoreResult<T>
where
    T: serde::de::DeserializeOwned,
{
    from_json_text(value).map_err(|error| {
        CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("parse PostgreSQL {label}: {error}"),
        )
    })
}

fn profile_dense_memory_space_descriptor() -> MemorySpaceDescriptor {
    MemorySpaceDescriptor {
        space_id: MemorySpaceId::unchecked("profile_dense"),
        schema_version: 1,
        module_id: Some("runtime_memory".to_string()),
        description: "Compact stable Crew profile memory; not Den memory.".to_string(),
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
        description: format!("profile_dense {field_name} field"),
    }
}

fn memory_operation_policy(
    operation: MemoryOperation,
    expected_revision_required: bool,
) -> MemoryOperationPolicy {
    MemoryOperationPolicy {
        operation,
        governance_mode: MemoryGovernanceMode::Candidate,
        requires_expected_revision: expected_revision_required,
        min_confidence: None,
    }
}

fn attachment_status_as_str(status: AttachmentStatus) -> &'static str {
    match status {
        AttachmentStatus::Active => "active",
        AttachmentStatus::Removed => "removed",
    }
}

fn attachment_status_from_str(raw: &str) -> CoreResult<AttachmentStatus> {
    match raw {
        "active" => Ok(AttachmentStatus::Active),
        "removed" => Ok(AttachmentStatus::Removed),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("unknown attachment status {other}"),
        )),
    }
}

fn data_bank_scope_status_as_str(status: DataBankScopeStatus) -> &'static str {
    match status {
        DataBankScopeStatus::Active => "active",
        DataBankScopeStatus::Removed => "removed",
    }
}

fn data_bank_scope_status_from_str(raw: &str) -> CoreResult<DataBankScopeStatus> {
    match raw {
        "active" => Ok(DataBankScopeStatus::Active),
        "removed" => Ok(DataBankScopeStatus::Removed),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("unknown data-bank scope status {other}"),
        )),
    }
}

fn runtime_search_row_type_as_str(row_type: RuntimeSearchRowType) -> &'static str {
    match row_type {
        RuntimeSearchRowType::Message => "message",
        RuntimeSearchRowType::QueueMessage => "queue_message",
        RuntimeSearchRowType::Session => "session",
    }
}

fn runtime_search_row_type_from_str(raw: &str) -> CoreResult<RuntimeSearchRowType> {
    match raw {
        "message" => Ok(RuntimeSearchRowType::Message),
        "queue_message" => Ok(RuntimeSearchRowType::QueueMessage),
        "session" => Ok(RuntimeSearchRowType::Session),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("unknown runtime search row type {other}"),
        )),
    }
}

fn core_event_kind_from_debug_str(raw: &str) -> CoreResult<crate::CoreEventKind> {
    match raw {
        "AgentMessageRouted" => Ok(crate::CoreEventKind::AgentMessageRouted),
        "SessionCreated" => Ok(crate::CoreEventKind::SessionCreated),
        "SessionArchived" => Ok(crate::CoreEventKind::SessionArchived),
        "BrainWakeRequested" => Ok(crate::CoreEventKind::BrainWakeRequested),
        "BrainEventObserved" => Ok(crate::CoreEventKind::BrainEventObserved),
        "BrainActionsAccepted" => Ok(crate::CoreEventKind::BrainActionsAccepted),
        "DelegationLifecycleObserved" => Ok(crate::CoreEventKind::DelegationLifecycleObserved),
        "CompletionPacketDelivered" => Ok(crate::CoreEventKind::CompletionPacketDelivered),
        "ExternalEventInjected" => Ok(crate::CoreEventKind::ExternalEventInjected),
        "DenDataUpdated" => Ok(crate::CoreEventKind::DenDataUpdated),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("unknown runtime search event kind {other}"),
        )),
    }
}

fn postgres_like_prefix(prefix: &str) -> String {
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

fn postgres_like_contains(value: &str) -> String {
    let mut escaped = String::from("%");
    for character in value.chars() {
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

fn validate_counter_amount(amount: u64) -> CoreResult<()> {
    if amount > i64::MAX as u64 {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "runtime counter increment exceeds PostgreSQL BIGINT range",
        ));
    }
    Ok(())
}

fn validate_postgres_identifier(label: &str, value: &str) -> CoreResult<()> {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("{label} must not be empty"),
        ));
    };
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("{label} must start with an ASCII letter or underscore"),
        ));
    }
    if !chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric()) {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("{label} must contain only ASCII letters, digits, or underscores"),
        ));
    }
    Ok(())
}

fn quote_postgres_identifier(identifier: &str) -> String {
    format!("\"{identifier}\"")
}

fn postgres_error(context: &str, error: postgres::Error) -> CoreError {
    CoreError::new(
        CoreErrorKind::PersistenceFailure,
        format!("{context}: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CoordinationStore, ExternalBindingProvenance, McpBindingDiagnostics, MessageBlockWrite,
        RoleplayLoreCanonStatus, RoleplayLoreVisibility, COUNTER_MESSAGES, COUNTER_WAKES,
    };
    use rusty_crew_core_protocol::{
        AgentMessage, BrainEvent, MemoryEvidenceRef, MemoryProposalSource,
        ProfileRegistryImportExportMetadata, ResourceLimits, SessionHandle, ToolCallMetadata,
        ToolCallSource, ToolDescriptor, ToolProfile,
    };
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    trait SimpleKvConformanceStore {
        fn put_simple_kv(&self, write: &SimpleKvWrite) -> CoreResult<SimpleKvRecord>;
        fn compare_and_swap_simple_kv(
            &self,
            compare_and_swap: &SimpleKvCompareAndSwap,
        ) -> CoreResult<SimpleKvRecord>;
        fn get_simple_kv(
            &self,
            scope: &SimpleKvScope,
            key: &str,
            now: Option<&IsoTimestamp>,
        ) -> CoreResult<Option<SimpleKvRecord>>;
        fn list_simple_kv(&self, query: &SimpleKvQuery) -> CoreResult<Vec<SimpleKvRecord>>;
        fn delete_simple_kv(&self, delete: &SimpleKvDelete) -> CoreResult<SimpleKvRecord>;
        fn expire_simple_kv(&self, now: &IsoTimestamp) -> CoreResult<u64>;
    }

    trait SessionEventConformanceStore {
        fn save_session_with_config(
            &self,
            state: &SessionState,
            config: &SessionConfig,
        ) -> CoreResult<()>;
        fn load_sessions(&self) -> CoreResult<Vec<SessionState>>;
        fn load_session_configs(&self) -> CoreResult<Vec<SessionConfigRecord>>;
        fn load_session_identities(&self) -> CoreResult<Vec<SessionIdentityRecord>>;
        fn save_event(&self, sequence: u64, event: &CoreEvent) -> CoreResult<()>;
        fn load_event_history(&self) -> CoreResult<Vec<PersistedEvent>>;
        fn query_events(&self, filter: &RuntimeEventFilter) -> CoreResult<Vec<RuntimeEventRecord>>;
    }

    trait QueueConformanceStore {
        fn save_queued_message(&self, record: &QueuedMessageRecord) -> CoreResult<()>;
        fn expire_queued_messages_at(
            &self,
            now: &IsoTimestamp,
        ) -> CoreResult<Vec<QueuedMessageRecord>>;
        fn load_queued_messages(
            &self,
            filter: &QueuedMessageFilter,
        ) -> CoreResult<Vec<QueuedMessageRecord>>;
        fn runtime_summary(&self, scope: &RuntimeCounterScope) -> CoreResult<RuntimeStateSummary>;
    }

    trait SchedulerConformanceStore {
        fn upsert_scheduled_job(&self, record: &ScheduledJobRecord) -> CoreResult<()>;
        fn load_scheduled_job(&self, job_id: &str) -> CoreResult<Option<ScheduledJobRecord>>;
        fn query_scheduled_jobs(
            &self,
            query: &ScheduledJobQuery,
        ) -> CoreResult<Vec<ScheduledJobRecord>>;
        fn claim_scheduled_run(
            &self,
            run: &ScheduledRunRecord,
            next_due_at: Option<&IsoTimestamp>,
        ) -> CoreResult<()>;
        fn complete_scheduled_run(
            &self,
            run_id: &RunId,
            status: ScheduledRunStatus,
            completed_at: &IsoTimestamp,
            output_json: &serde_json::Value,
            error: Option<&str>,
        ) -> CoreResult<()>;
        fn query_scheduled_runs(
            &self,
            query: &ScheduledRunQuery,
        ) -> CoreResult<Vec<ScheduledRunRecord>>;
        fn expire_stale_scheduled_runs(
            &self,
            stale_before: &IsoTimestamp,
            now: &IsoTimestamp,
        ) -> CoreResult<Vec<ScheduledRunRecord>>;
    }

    trait WorkerLifecycleConformanceStore {
        fn save_worker_run_requested(&self, record: &WorkerRunRecord) -> CoreResult<()>;
        fn load_worker_run(&self, run_id: &RunId) -> CoreResult<Option<WorkerRunRecord>>;
        fn load_worker_run_by_delegated_session(
            &self,
            delegated_session_id: &SessionId,
        ) -> CoreResult<Option<WorkerRunRecord>>;
        fn query_worker_runs(&self, query: &WorkerRunQuery) -> CoreResult<Vec<WorkerRunRecord>>;
        fn update_worker_run_status_by_delegated_session(
            &self,
            delegated_session_id: &SessionId,
            status: WorkerRunStatus,
            now: IsoTimestamp,
        ) -> CoreResult<()>;
        fn update_worker_run_status(
            &self,
            run_id: &RunId,
            status: WorkerRunStatus,
            now: IsoTimestamp,
        ) -> CoreResult<()>;
        fn save_event(&self, sequence: u64, event: &CoreEvent) -> CoreResult<()>;
        fn query_completion_packets(
            &self,
            query: &CompletionPacketQuery,
        ) -> CoreResult<Vec<CompletionPacketRecord>>;
        fn delegated_completions_for_parent(
            &self,
            parent_session_id: &SessionId,
        ) -> CoreResult<Vec<DelegatedCompletion>>;
    }

    trait TelemetryMaintenanceConformanceStore {
        fn save_session_with_config(
            &self,
            state: &SessionState,
            config: &SessionConfig,
        ) -> CoreResult<()>;
        fn save_event(&self, sequence: u64, event: &CoreEvent) -> CoreResult<()>;
        fn runtime_summary(&self, scope: &RuntimeCounterScope) -> CoreResult<RuntimeStateSummary>;
        fn load_tool_call_history(&self) -> CoreResult<Vec<ToolCallRecord>>;
        fn save_queued_message(&self, record: &QueuedMessageRecord) -> CoreResult<()>;
        fn load_queued_messages(
            &self,
            filter: &QueuedMessageFilter,
        ) -> CoreResult<Vec<QueuedMessageRecord>>;
        fn save_provider_wire_state(
            &self,
            write: &ProviderWireStateWrite,
        ) -> CoreResult<ProviderWireStateRecord>;
        fn run_maintenance(
            &self,
            policy: &RuntimeMaintenancePolicy,
        ) -> CoreResult<RuntimeMaintenanceReport>;
    }

    trait ProviderWireStateConformanceStore {
        fn save_provider_wire_state(
            &self,
            write: &ProviderWireStateWrite,
        ) -> CoreResult<ProviderWireStateRecord>;
        fn load_provider_wire_state_for_wake(
            &self,
            lookup: &ProviderWireStateWakeLookup,
        ) -> CoreResult<ProviderWireStateWakeResult>;
        fn clear_provider_wire_state(
            &self,
            key: &ProviderWireStateKey,
            now: &IsoTimestamp,
            reason: ProviderWireStateInvalidationReason,
        ) -> CoreResult<Option<ProviderWireStateRecord>>;
        fn expire_provider_wire_states_at(
            &self,
            now: &IsoTimestamp,
        ) -> CoreResult<Vec<ProviderWireStateRecord>>;
        fn list_provider_wire_state_diagnostics(
            &self,
            limit: u32,
        ) -> CoreResult<Vec<ProviderWireStateDiagnostic>>;
    }

    trait ConversationConformanceStore {
        fn save_message_slot(&self, slot: &MessageSlotWrite) -> CoreResult<()>;
        fn save_message_variant(
            &self,
            variant: &MessageVariantWrite,
        ) -> CoreResult<MessageVariantRecord>;
        fn query_message_slots(
            &self,
            query: &MessageSlotQuery,
        ) -> CoreResult<Vec<MessageSlotRecord>>;
        fn query_message_variants(
            &self,
            query: &MessageVariantQuery,
        ) -> CoreResult<Vec<MessageVariantRecord>>;
        fn select_active_message_variant(
            &self,
            request: &SelectActiveVariantRequest,
        ) -> CoreResult<SelectActiveVariantResult>;
        fn save_conversation_branch(
            &self,
            branch: &ConversationBranchWrite,
        ) -> CoreResult<ConversationBranchRecord>;
        fn query_conversation_branches(
            &self,
            query: &ConversationBranchQuery,
        ) -> CoreResult<Vec<ConversationBranchRecord>>;
        fn get_conversation_branch_state(
            &self,
            session_id: &SessionId,
            default_updated_at: &IsoTimestamp,
        ) -> CoreResult<ConversationBranchStateRecord>;
        fn select_active_conversation_branch(
            &self,
            request: &SelectActiveBranchRequest,
        ) -> CoreResult<SelectActiveBranchResult>;
        fn update_conversation_branch_head(
            &self,
            request: &UpdateBranchHeadRequest,
        ) -> CoreResult<UpdateBranchHeadResult>;
        fn save_conversation_snapshot(
            &self,
            snapshot: &ConversationSnapshotWrite,
        ) -> CoreResult<ConversationSnapshotRecord>;
        fn query_conversation_snapshots(
            &self,
            query: &ConversationSnapshotQuery,
        ) -> CoreResult<Vec<ConversationSnapshotRecord>>;
        fn resolve_conversation_jump(
            &self,
            request: &ConversationJumpRequest,
        ) -> CoreResult<ConversationJumpResult>;
    }

    trait AttachmentDataBankConformanceStore {
        fn save_attachment(&self, attachment: &AttachmentWrite) -> CoreResult<AttachmentRecord>;
        fn query_attachments(&self, query: &AttachmentQuery) -> CoreResult<Vec<AttachmentRecord>>;
        fn remove_attachment(
            &self,
            attachment_id: &AttachmentId,
            updated_at: &IsoTimestamp,
        ) -> CoreResult<AttachmentRecord>;
        fn save_data_bank_scope(
            &self,
            scope: &DataBankScopeWrite,
        ) -> CoreResult<DataBankScopeRecord>;
        fn query_data_bank_scopes(
            &self,
            query: &DataBankScopeQuery,
        ) -> CoreResult<Vec<DataBankScopeRecord>>;
        fn remove_data_bank_scope(
            &self,
            scope_id: &DataBankScopeId,
            updated_at: &IsoTimestamp,
        ) -> CoreResult<DataBankScopeRecord>;
    }

    trait ProfileMemoryConformanceStore {
        fn memory_space_descriptors(&self) -> Vec<MemorySpaceDescriptor>;
        fn list_profile_memory(
            &self,
            query: &ProfileMemoryQuery,
        ) -> CoreResult<Vec<ProfileMemoryRecord>>;
        fn get_profile_memory(
            &self,
            profile_id: &ProfileId,
            target: &ProfileMemoryTarget,
            key: &str,
        ) -> CoreResult<Option<ProfileMemoryRecord>>;
        fn add_profile_memory(
            &self,
            write: &ProfileMemoryWrite,
            caps: &ProfileMemoryCaps,
        ) -> CoreResult<ProfileMemoryRecord>;
        fn replace_profile_memory(
            &self,
            replace: &ProfileMemoryReplace,
            caps: &ProfileMemoryCaps,
        ) -> CoreResult<ProfileMemoryRecord>;
        fn remove_profile_memory(
            &self,
            delete: &ProfileMemoryDelete,
        ) -> CoreResult<ProfileMemoryRecord>;
    }

    trait RoleplayLoreConformanceStore {
        fn memory_space_descriptors(&self) -> Vec<MemorySpaceDescriptor>;
        fn add_roleplay_lore_record(
            &self,
            write: &RoleplayLoreWrite,
        ) -> CoreResult<RoleplayLoreRecord>;
        fn replace_roleplay_lore_record(
            &self,
            replace: &RoleplayLoreReplace,
        ) -> CoreResult<RoleplayLoreRecord>;
        fn supersede_roleplay_lore_record(
            &self,
            supersede: &RoleplayLoreSupersede,
        ) -> CoreResult<(RoleplayLoreRecord, RoleplayLoreRecord)>;
        fn tombstone_roleplay_lore_record(
            &self,
            tombstone: &RoleplayLoreTombstone,
        ) -> CoreResult<RoleplayLoreRecord>;
        fn query_roleplay_lore_records(
            &self,
            query: &RoleplayLoreQuery,
        ) -> CoreResult<Vec<RoleplayLoreRecord>>;
        fn roleplay_lore_provenance_events(
            &self,
            record_id: &str,
        ) -> CoreResult<Vec<RoleplayLoreProvenanceEvent>>;
    }

    impl SimpleKvConformanceStore for CoordinationStore {
        fn put_simple_kv(&self, write: &SimpleKvWrite) -> CoreResult<SimpleKvRecord> {
            CoordinationStore::put_simple_kv(self, write)
        }

        fn compare_and_swap_simple_kv(
            &self,
            compare_and_swap: &SimpleKvCompareAndSwap,
        ) -> CoreResult<SimpleKvRecord> {
            CoordinationStore::compare_and_swap_simple_kv(self, compare_and_swap)
        }

        fn get_simple_kv(
            &self,
            scope: &SimpleKvScope,
            key: &str,
            now: Option<&IsoTimestamp>,
        ) -> CoreResult<Option<SimpleKvRecord>> {
            CoordinationStore::get_simple_kv(self, scope, key, now)
        }

        fn list_simple_kv(&self, query: &SimpleKvQuery) -> CoreResult<Vec<SimpleKvRecord>> {
            CoordinationStore::list_simple_kv(self, query)
        }

        fn delete_simple_kv(&self, delete: &SimpleKvDelete) -> CoreResult<SimpleKvRecord> {
            CoordinationStore::delete_simple_kv(self, delete)
        }

        fn expire_simple_kv(&self, now: &IsoTimestamp) -> CoreResult<u64> {
            CoordinationStore::expire_simple_kv(self, now)
        }
    }

    impl SessionEventConformanceStore for CoordinationStore {
        fn save_session_with_config(
            &self,
            state: &SessionState,
            config: &SessionConfig,
        ) -> CoreResult<()> {
            CoordinationStore::save_session_with_config(self, state, config)
        }

        fn load_sessions(&self) -> CoreResult<Vec<SessionState>> {
            CoordinationStore::load_sessions(self)
        }

        fn load_session_configs(&self) -> CoreResult<Vec<SessionConfigRecord>> {
            CoordinationStore::load_session_configs(self)
        }

        fn load_session_identities(&self) -> CoreResult<Vec<SessionIdentityRecord>> {
            CoordinationStore::load_session_identities(self)
        }

        fn save_event(&self, sequence: u64, event: &CoreEvent) -> CoreResult<()> {
            CoordinationStore::save_event(self, sequence, event)
        }

        fn load_event_history(&self) -> CoreResult<Vec<PersistedEvent>> {
            CoordinationStore::load_event_history(self)
        }

        fn query_events(&self, filter: &RuntimeEventFilter) -> CoreResult<Vec<RuntimeEventRecord>> {
            CoordinationStore::query_events(self, filter)
        }
    }

    impl QueueConformanceStore for CoordinationStore {
        fn save_queued_message(&self, record: &QueuedMessageRecord) -> CoreResult<()> {
            CoordinationStore::save_queued_message(self, record)
        }

        fn expire_queued_messages_at(
            &self,
            now: &IsoTimestamp,
        ) -> CoreResult<Vec<QueuedMessageRecord>> {
            CoordinationStore::expire_queued_messages_at(self, now)
        }

        fn load_queued_messages(
            &self,
            filter: &QueuedMessageFilter,
        ) -> CoreResult<Vec<QueuedMessageRecord>> {
            CoordinationStore::load_queued_messages(self, filter)
        }

        fn runtime_summary(&self, scope: &RuntimeCounterScope) -> CoreResult<RuntimeStateSummary> {
            CoordinationStore::runtime_summary(self, scope)
        }
    }

    impl SchedulerConformanceStore for CoordinationStore {
        fn upsert_scheduled_job(&self, record: &ScheduledJobRecord) -> CoreResult<()> {
            CoordinationStore::upsert_scheduled_job(self, record)
        }

        fn load_scheduled_job(&self, job_id: &str) -> CoreResult<Option<ScheduledJobRecord>> {
            CoordinationStore::load_scheduled_job(self, job_id)
        }

        fn query_scheduled_jobs(
            &self,
            query: &ScheduledJobQuery,
        ) -> CoreResult<Vec<ScheduledJobRecord>> {
            CoordinationStore::query_scheduled_jobs(self, query)
        }

        fn claim_scheduled_run(
            &self,
            run: &ScheduledRunRecord,
            next_due_at: Option<&IsoTimestamp>,
        ) -> CoreResult<()> {
            CoordinationStore::claim_scheduled_run(self, run, next_due_at)
        }

        fn complete_scheduled_run(
            &self,
            run_id: &RunId,
            status: ScheduledRunStatus,
            completed_at: &IsoTimestamp,
            output_json: &serde_json::Value,
            error: Option<&str>,
        ) -> CoreResult<()> {
            CoordinationStore::complete_scheduled_run(
                self,
                run_id,
                status,
                completed_at,
                output_json,
                error,
            )
        }

        fn query_scheduled_runs(
            &self,
            query: &ScheduledRunQuery,
        ) -> CoreResult<Vec<ScheduledRunRecord>> {
            CoordinationStore::query_scheduled_runs(self, query)
        }

        fn expire_stale_scheduled_runs(
            &self,
            stale_before: &IsoTimestamp,
            now: &IsoTimestamp,
        ) -> CoreResult<Vec<ScheduledRunRecord>> {
            CoordinationStore::expire_stale_scheduled_runs(self, stale_before, now)
        }
    }

    impl WorkerLifecycleConformanceStore for CoordinationStore {
        fn save_worker_run_requested(&self, record: &WorkerRunRecord) -> CoreResult<()> {
            CoordinationStore::save_worker_run_requested(self, record)
        }

        fn load_worker_run(&self, run_id: &RunId) -> CoreResult<Option<WorkerRunRecord>> {
            CoordinationStore::load_worker_run(self, run_id)
        }

        fn load_worker_run_by_delegated_session(
            &self,
            delegated_session_id: &SessionId,
        ) -> CoreResult<Option<WorkerRunRecord>> {
            CoordinationStore::load_worker_run_by_delegated_session(self, delegated_session_id)
        }

        fn query_worker_runs(&self, query: &WorkerRunQuery) -> CoreResult<Vec<WorkerRunRecord>> {
            CoordinationStore::query_worker_runs(self, query)
        }

        fn update_worker_run_status_by_delegated_session(
            &self,
            delegated_session_id: &SessionId,
            status: WorkerRunStatus,
            now: IsoTimestamp,
        ) -> CoreResult<()> {
            CoordinationStore::update_worker_run_status_by_delegated_session(
                self,
                delegated_session_id,
                status,
                now,
            )
        }

        fn update_worker_run_status(
            &self,
            run_id: &RunId,
            status: WorkerRunStatus,
            now: IsoTimestamp,
        ) -> CoreResult<()> {
            CoordinationStore::update_worker_run_status(self, run_id, status, now)
        }

        fn save_event(&self, sequence: u64, event: &CoreEvent) -> CoreResult<()> {
            CoordinationStore::save_event(self, sequence, event)
        }

        fn query_completion_packets(
            &self,
            query: &CompletionPacketQuery,
        ) -> CoreResult<Vec<CompletionPacketRecord>> {
            CoordinationStore::query_completion_packets(self, query)
        }

        fn delegated_completions_for_parent(
            &self,
            parent_session_id: &SessionId,
        ) -> CoreResult<Vec<DelegatedCompletion>> {
            CoordinationStore::delegated_completions_for_parent(self, parent_session_id)
        }
    }

    impl TelemetryMaintenanceConformanceStore for CoordinationStore {
        fn save_session_with_config(
            &self,
            state: &SessionState,
            config: &SessionConfig,
        ) -> CoreResult<()> {
            CoordinationStore::save_session_with_config(self, state, config)
        }

        fn save_event(&self, sequence: u64, event: &CoreEvent) -> CoreResult<()> {
            CoordinationStore::save_event(self, sequence, event)
        }

        fn runtime_summary(&self, scope: &RuntimeCounterScope) -> CoreResult<RuntimeStateSummary> {
            CoordinationStore::runtime_summary(self, scope)
        }

        fn load_tool_call_history(&self) -> CoreResult<Vec<ToolCallRecord>> {
            CoordinationStore::load_tool_call_history(self)
        }

        fn save_queued_message(&self, record: &QueuedMessageRecord) -> CoreResult<()> {
            CoordinationStore::save_queued_message(self, record)
        }

        fn load_queued_messages(
            &self,
            filter: &QueuedMessageFilter,
        ) -> CoreResult<Vec<QueuedMessageRecord>> {
            CoordinationStore::load_queued_messages(self, filter)
        }

        fn save_provider_wire_state(
            &self,
            write: &ProviderWireStateWrite,
        ) -> CoreResult<ProviderWireStateRecord> {
            CoordinationStore::save_provider_wire_state(self, write)
        }

        fn run_maintenance(
            &self,
            policy: &RuntimeMaintenancePolicy,
        ) -> CoreResult<RuntimeMaintenanceReport> {
            CoordinationStore::run_maintenance(self, policy)
        }
    }

    impl ProviderWireStateConformanceStore for CoordinationStore {
        fn save_provider_wire_state(
            &self,
            write: &ProviderWireStateWrite,
        ) -> CoreResult<ProviderWireStateRecord> {
            CoordinationStore::save_provider_wire_state(self, write)
        }

        fn load_provider_wire_state_for_wake(
            &self,
            lookup: &ProviderWireStateWakeLookup,
        ) -> CoreResult<ProviderWireStateWakeResult> {
            CoordinationStore::load_provider_wire_state_for_wake(self, lookup)
        }

        fn clear_provider_wire_state(
            &self,
            key: &ProviderWireStateKey,
            now: &IsoTimestamp,
            reason: ProviderWireStateInvalidationReason,
        ) -> CoreResult<Option<ProviderWireStateRecord>> {
            CoordinationStore::clear_provider_wire_state(self, key, now, reason)
        }

        fn expire_provider_wire_states_at(
            &self,
            now: &IsoTimestamp,
        ) -> CoreResult<Vec<ProviderWireStateRecord>> {
            CoordinationStore::expire_provider_wire_states_at(self, now)
        }

        fn list_provider_wire_state_diagnostics(
            &self,
            limit: u32,
        ) -> CoreResult<Vec<ProviderWireStateDiagnostic>> {
            CoordinationStore::list_provider_wire_state_diagnostics(self, limit)
        }
    }

    impl ConversationConformanceStore for CoordinationStore {
        fn save_message_slot(&self, slot: &MessageSlotWrite) -> CoreResult<()> {
            CoordinationStore::save_message_slot(self, slot)
        }

        fn save_message_variant(
            &self,
            variant: &MessageVariantWrite,
        ) -> CoreResult<MessageVariantRecord> {
            CoordinationStore::save_message_variant(self, variant)
        }

        fn query_message_slots(
            &self,
            query: &MessageSlotQuery,
        ) -> CoreResult<Vec<MessageSlotRecord>> {
            CoordinationStore::query_message_slots(self, query)
        }

        fn query_message_variants(
            &self,
            query: &MessageVariantQuery,
        ) -> CoreResult<Vec<MessageVariantRecord>> {
            CoordinationStore::query_message_variants(self, query)
        }

        fn select_active_message_variant(
            &self,
            request: &SelectActiveVariantRequest,
        ) -> CoreResult<SelectActiveVariantResult> {
            CoordinationStore::select_active_message_variant(self, request)
        }

        fn save_conversation_branch(
            &self,
            branch: &ConversationBranchWrite,
        ) -> CoreResult<ConversationBranchRecord> {
            CoordinationStore::save_conversation_branch(self, branch)
        }

        fn query_conversation_branches(
            &self,
            query: &ConversationBranchQuery,
        ) -> CoreResult<Vec<ConversationBranchRecord>> {
            CoordinationStore::query_conversation_branches(self, query)
        }

        fn get_conversation_branch_state(
            &self,
            session_id: &SessionId,
            default_updated_at: &IsoTimestamp,
        ) -> CoreResult<ConversationBranchStateRecord> {
            CoordinationStore::get_conversation_branch_state(self, session_id, default_updated_at)
        }

        fn select_active_conversation_branch(
            &self,
            request: &SelectActiveBranchRequest,
        ) -> CoreResult<SelectActiveBranchResult> {
            CoordinationStore::select_active_conversation_branch(self, request)
        }

        fn update_conversation_branch_head(
            &self,
            request: &UpdateBranchHeadRequest,
        ) -> CoreResult<UpdateBranchHeadResult> {
            CoordinationStore::update_conversation_branch_head(self, request)
        }

        fn save_conversation_snapshot(
            &self,
            snapshot: &ConversationSnapshotWrite,
        ) -> CoreResult<ConversationSnapshotRecord> {
            CoordinationStore::save_conversation_snapshot(self, snapshot)
        }

        fn query_conversation_snapshots(
            &self,
            query: &ConversationSnapshotQuery,
        ) -> CoreResult<Vec<ConversationSnapshotRecord>> {
            CoordinationStore::query_conversation_snapshots(self, query)
        }

        fn resolve_conversation_jump(
            &self,
            request: &ConversationJumpRequest,
        ) -> CoreResult<ConversationJumpResult> {
            CoordinationStore::resolve_conversation_jump(self, request)
        }
    }

    impl AttachmentDataBankConformanceStore for CoordinationStore {
        fn save_attachment(&self, attachment: &AttachmentWrite) -> CoreResult<AttachmentRecord> {
            CoordinationStore::save_attachment(self, attachment)
        }

        fn query_attachments(&self, query: &AttachmentQuery) -> CoreResult<Vec<AttachmentRecord>> {
            CoordinationStore::query_attachments(self, query)
        }

        fn remove_attachment(
            &self,
            attachment_id: &AttachmentId,
            updated_at: &IsoTimestamp,
        ) -> CoreResult<AttachmentRecord> {
            CoordinationStore::remove_attachment(self, attachment_id, updated_at)
        }

        fn save_data_bank_scope(
            &self,
            scope: &DataBankScopeWrite,
        ) -> CoreResult<DataBankScopeRecord> {
            CoordinationStore::save_data_bank_scope(self, scope)
        }

        fn query_data_bank_scopes(
            &self,
            query: &DataBankScopeQuery,
        ) -> CoreResult<Vec<DataBankScopeRecord>> {
            CoordinationStore::query_data_bank_scopes(self, query)
        }

        fn remove_data_bank_scope(
            &self,
            scope_id: &DataBankScopeId,
            updated_at: &IsoTimestamp,
        ) -> CoreResult<DataBankScopeRecord> {
            CoordinationStore::remove_data_bank_scope(self, scope_id, updated_at)
        }
    }

    impl ProfileMemoryConformanceStore for CoordinationStore {
        fn memory_space_descriptors(&self) -> Vec<MemorySpaceDescriptor> {
            vec![profile_dense_memory_space_descriptor()]
        }

        fn list_profile_memory(
            &self,
            query: &ProfileMemoryQuery,
        ) -> CoreResult<Vec<ProfileMemoryRecord>> {
            CoordinationStore::list_profile_memory(self, query)
        }

        fn get_profile_memory(
            &self,
            profile_id: &ProfileId,
            target: &ProfileMemoryTarget,
            key: &str,
        ) -> CoreResult<Option<ProfileMemoryRecord>> {
            CoordinationStore::get_profile_memory(self, profile_id, target, key)
        }

        fn add_profile_memory(
            &self,
            write: &ProfileMemoryWrite,
            caps: &ProfileMemoryCaps,
        ) -> CoreResult<ProfileMemoryRecord> {
            CoordinationStore::add_profile_memory(self, write, caps)
        }

        fn replace_profile_memory(
            &self,
            replace: &ProfileMemoryReplace,
            caps: &ProfileMemoryCaps,
        ) -> CoreResult<ProfileMemoryRecord> {
            CoordinationStore::replace_profile_memory(self, replace, caps)
        }

        fn remove_profile_memory(
            &self,
            delete: &ProfileMemoryDelete,
        ) -> CoreResult<ProfileMemoryRecord> {
            CoordinationStore::remove_profile_memory(self, delete)
        }
    }

    impl RoleplayLoreConformanceStore for CoordinationStore {
        fn memory_space_descriptors(&self) -> Vec<MemorySpaceDescriptor> {
            vec![roleplay_lore_memory_space_descriptor()]
        }

        fn add_roleplay_lore_record(
            &self,
            write: &RoleplayLoreWrite,
        ) -> CoreResult<RoleplayLoreRecord> {
            CoordinationStore::add_roleplay_lore_record(self, write)
        }

        fn replace_roleplay_lore_record(
            &self,
            replace: &RoleplayLoreReplace,
        ) -> CoreResult<RoleplayLoreRecord> {
            CoordinationStore::replace_roleplay_lore_record(self, replace)
        }

        fn supersede_roleplay_lore_record(
            &self,
            supersede: &RoleplayLoreSupersede,
        ) -> CoreResult<(RoleplayLoreRecord, RoleplayLoreRecord)> {
            CoordinationStore::supersede_roleplay_lore_record(self, supersede)
        }

        fn tombstone_roleplay_lore_record(
            &self,
            tombstone: &RoleplayLoreTombstone,
        ) -> CoreResult<RoleplayLoreRecord> {
            CoordinationStore::tombstone_roleplay_lore_record(self, tombstone)
        }

        fn query_roleplay_lore_records(
            &self,
            query: &RoleplayLoreQuery,
        ) -> CoreResult<Vec<RoleplayLoreRecord>> {
            CoordinationStore::query_roleplay_lore_records(self, query)
        }

        fn roleplay_lore_provenance_events(
            &self,
            record_id: &str,
        ) -> CoreResult<Vec<RoleplayLoreProvenanceEvent>> {
            CoordinationStore::roleplay_lore_provenance_events(self, record_id)
        }
    }

    impl SimpleKvConformanceStore for PostgresRuntimeCounterProofStore {
        fn put_simple_kv(&self, write: &SimpleKvWrite) -> CoreResult<SimpleKvRecord> {
            PostgresRuntimeCounterProofStore::put_simple_kv(self, write)
        }

        fn compare_and_swap_simple_kv(
            &self,
            compare_and_swap: &SimpleKvCompareAndSwap,
        ) -> CoreResult<SimpleKvRecord> {
            PostgresRuntimeCounterProofStore::compare_and_swap_simple_kv(self, compare_and_swap)
        }

        fn get_simple_kv(
            &self,
            scope: &SimpleKvScope,
            key: &str,
            now: Option<&IsoTimestamp>,
        ) -> CoreResult<Option<SimpleKvRecord>> {
            PostgresRuntimeCounterProofStore::get_simple_kv(self, scope, key, now)
        }

        fn list_simple_kv(&self, query: &SimpleKvQuery) -> CoreResult<Vec<SimpleKvRecord>> {
            PostgresRuntimeCounterProofStore::list_simple_kv(self, query)
        }

        fn delete_simple_kv(&self, delete: &SimpleKvDelete) -> CoreResult<SimpleKvRecord> {
            PostgresRuntimeCounterProofStore::delete_simple_kv(self, delete)
        }

        fn expire_simple_kv(&self, now: &IsoTimestamp) -> CoreResult<u64> {
            PostgresRuntimeCounterProofStore::expire_simple_kv(self, now)
        }
    }

    impl SessionEventConformanceStore for PostgresRuntimeCounterProofStore {
        fn save_session_with_config(
            &self,
            state: &SessionState,
            config: &SessionConfig,
        ) -> CoreResult<()> {
            PostgresRuntimeCounterProofStore::save_session_with_config(self, state, config)
        }

        fn load_sessions(&self) -> CoreResult<Vec<SessionState>> {
            PostgresRuntimeCounterProofStore::load_sessions(self)
        }

        fn load_session_configs(&self) -> CoreResult<Vec<SessionConfigRecord>> {
            PostgresRuntimeCounterProofStore::load_session_configs(self)
        }

        fn load_session_identities(&self) -> CoreResult<Vec<SessionIdentityRecord>> {
            PostgresRuntimeCounterProofStore::load_session_identities(self)
        }

        fn save_event(&self, sequence: u64, event: &CoreEvent) -> CoreResult<()> {
            PostgresRuntimeCounterProofStore::save_event(self, sequence, event)
        }

        fn load_event_history(&self) -> CoreResult<Vec<PersistedEvent>> {
            PostgresRuntimeCounterProofStore::load_event_history(self)
        }

        fn query_events(&self, filter: &RuntimeEventFilter) -> CoreResult<Vec<RuntimeEventRecord>> {
            PostgresRuntimeCounterProofStore::query_events(self, filter)
        }
    }

    impl QueueConformanceStore for PostgresRuntimeCounterProofStore {
        fn save_queued_message(&self, record: &QueuedMessageRecord) -> CoreResult<()> {
            PostgresRuntimeCounterProofStore::save_queued_message(self, record)
        }

        fn expire_queued_messages_at(
            &self,
            now: &IsoTimestamp,
        ) -> CoreResult<Vec<QueuedMessageRecord>> {
            PostgresRuntimeCounterProofStore::expire_queued_messages_at(self, now)
        }

        fn load_queued_messages(
            &self,
            filter: &QueuedMessageFilter,
        ) -> CoreResult<Vec<QueuedMessageRecord>> {
            PostgresRuntimeCounterProofStore::load_queued_messages(self, filter)
        }

        fn runtime_summary(&self, scope: &RuntimeCounterScope) -> CoreResult<RuntimeStateSummary> {
            PostgresRuntimeCounterProofStore::runtime_summary(self, scope)
        }
    }

    impl SchedulerConformanceStore for PostgresRuntimeCounterProofStore {
        fn upsert_scheduled_job(&self, record: &ScheduledJobRecord) -> CoreResult<()> {
            PostgresRuntimeCounterProofStore::upsert_scheduled_job(self, record)
        }

        fn load_scheduled_job(&self, job_id: &str) -> CoreResult<Option<ScheduledJobRecord>> {
            PostgresRuntimeCounterProofStore::load_scheduled_job(self, job_id)
        }

        fn query_scheduled_jobs(
            &self,
            query: &ScheduledJobQuery,
        ) -> CoreResult<Vec<ScheduledJobRecord>> {
            PostgresRuntimeCounterProofStore::query_scheduled_jobs(self, query)
        }

        fn claim_scheduled_run(
            &self,
            run: &ScheduledRunRecord,
            next_due_at: Option<&IsoTimestamp>,
        ) -> CoreResult<()> {
            PostgresRuntimeCounterProofStore::claim_scheduled_run(self, run, next_due_at)
        }

        fn complete_scheduled_run(
            &self,
            run_id: &RunId,
            status: ScheduledRunStatus,
            completed_at: &IsoTimestamp,
            output_json: &serde_json::Value,
            error: Option<&str>,
        ) -> CoreResult<()> {
            PostgresRuntimeCounterProofStore::complete_scheduled_run(
                self,
                run_id,
                status,
                completed_at,
                output_json,
                error,
            )
        }

        fn query_scheduled_runs(
            &self,
            query: &ScheduledRunQuery,
        ) -> CoreResult<Vec<ScheduledRunRecord>> {
            PostgresRuntimeCounterProofStore::query_scheduled_runs(self, query)
        }

        fn expire_stale_scheduled_runs(
            &self,
            stale_before: &IsoTimestamp,
            now: &IsoTimestamp,
        ) -> CoreResult<Vec<ScheduledRunRecord>> {
            PostgresRuntimeCounterProofStore::expire_stale_scheduled_runs(self, stale_before, now)
        }
    }

    impl WorkerLifecycleConformanceStore for PostgresRuntimeCounterProofStore {
        fn save_worker_run_requested(&self, record: &WorkerRunRecord) -> CoreResult<()> {
            PostgresRuntimeCounterProofStore::save_worker_run_requested(self, record)
        }

        fn load_worker_run(&self, run_id: &RunId) -> CoreResult<Option<WorkerRunRecord>> {
            PostgresRuntimeCounterProofStore::load_worker_run(self, run_id)
        }

        fn load_worker_run_by_delegated_session(
            &self,
            delegated_session_id: &SessionId,
        ) -> CoreResult<Option<WorkerRunRecord>> {
            PostgresRuntimeCounterProofStore::load_worker_run_by_delegated_session(
                self,
                delegated_session_id,
            )
        }

        fn query_worker_runs(&self, query: &WorkerRunQuery) -> CoreResult<Vec<WorkerRunRecord>> {
            PostgresRuntimeCounterProofStore::query_worker_runs(self, query)
        }

        fn update_worker_run_status_by_delegated_session(
            &self,
            delegated_session_id: &SessionId,
            status: WorkerRunStatus,
            now: IsoTimestamp,
        ) -> CoreResult<()> {
            PostgresRuntimeCounterProofStore::update_worker_run_status_by_delegated_session(
                self,
                delegated_session_id,
                status,
                now,
            )
        }

        fn update_worker_run_status(
            &self,
            run_id: &RunId,
            status: WorkerRunStatus,
            now: IsoTimestamp,
        ) -> CoreResult<()> {
            PostgresRuntimeCounterProofStore::update_worker_run_status(self, run_id, status, now)
        }

        fn save_event(&self, sequence: u64, event: &CoreEvent) -> CoreResult<()> {
            PostgresRuntimeCounterProofStore::save_event(self, sequence, event)
        }

        fn query_completion_packets(
            &self,
            query: &CompletionPacketQuery,
        ) -> CoreResult<Vec<CompletionPacketRecord>> {
            PostgresRuntimeCounterProofStore::query_completion_packets(self, query)
        }

        fn delegated_completions_for_parent(
            &self,
            parent_session_id: &SessionId,
        ) -> CoreResult<Vec<DelegatedCompletion>> {
            PostgresRuntimeCounterProofStore::delegated_completions_for_parent(
                self,
                parent_session_id,
            )
        }
    }

    impl TelemetryMaintenanceConformanceStore for PostgresRuntimeCounterProofStore {
        fn save_session_with_config(
            &self,
            state: &SessionState,
            config: &SessionConfig,
        ) -> CoreResult<()> {
            PostgresRuntimeCounterProofStore::save_session_with_config(self, state, config)
        }

        fn save_event(&self, sequence: u64, event: &CoreEvent) -> CoreResult<()> {
            PostgresRuntimeCounterProofStore::save_event(self, sequence, event)
        }

        fn runtime_summary(&self, scope: &RuntimeCounterScope) -> CoreResult<RuntimeStateSummary> {
            PostgresRuntimeCounterProofStore::runtime_summary(self, scope)
        }

        fn load_tool_call_history(&self) -> CoreResult<Vec<ToolCallRecord>> {
            PostgresRuntimeCounterProofStore::load_tool_call_history(self)
        }

        fn save_queued_message(&self, record: &QueuedMessageRecord) -> CoreResult<()> {
            PostgresRuntimeCounterProofStore::save_queued_message(self, record)
        }

        fn load_queued_messages(
            &self,
            filter: &QueuedMessageFilter,
        ) -> CoreResult<Vec<QueuedMessageRecord>> {
            PostgresRuntimeCounterProofStore::load_queued_messages(self, filter)
        }

        fn save_provider_wire_state(
            &self,
            write: &ProviderWireStateWrite,
        ) -> CoreResult<ProviderWireStateRecord> {
            PostgresRuntimeCounterProofStore::save_provider_wire_state(self, write)
        }

        fn run_maintenance(
            &self,
            policy: &RuntimeMaintenancePolicy,
        ) -> CoreResult<RuntimeMaintenanceReport> {
            PostgresRuntimeCounterProofStore::run_maintenance(self, policy)
        }
    }

    impl ProviderWireStateConformanceStore for PostgresRuntimeCounterProofStore {
        fn save_provider_wire_state(
            &self,
            write: &ProviderWireStateWrite,
        ) -> CoreResult<ProviderWireStateRecord> {
            PostgresRuntimeCounterProofStore::save_provider_wire_state(self, write)
        }

        fn load_provider_wire_state_for_wake(
            &self,
            lookup: &ProviderWireStateWakeLookup,
        ) -> CoreResult<ProviderWireStateWakeResult> {
            PostgresRuntimeCounterProofStore::load_provider_wire_state_for_wake(self, lookup)
        }

        fn clear_provider_wire_state(
            &self,
            key: &ProviderWireStateKey,
            now: &IsoTimestamp,
            reason: ProviderWireStateInvalidationReason,
        ) -> CoreResult<Option<ProviderWireStateRecord>> {
            PostgresRuntimeCounterProofStore::clear_provider_wire_state(self, key, now, reason)
        }

        fn expire_provider_wire_states_at(
            &self,
            now: &IsoTimestamp,
        ) -> CoreResult<Vec<ProviderWireStateRecord>> {
            PostgresRuntimeCounterProofStore::expire_provider_wire_states_at(self, now)
        }

        fn list_provider_wire_state_diagnostics(
            &self,
            limit: u32,
        ) -> CoreResult<Vec<ProviderWireStateDiagnostic>> {
            PostgresRuntimeCounterProofStore::list_provider_wire_state_diagnostics(self, limit)
        }
    }

    impl ConversationConformanceStore for PostgresRuntimeCounterProofStore {
        fn save_message_slot(&self, slot: &MessageSlotWrite) -> CoreResult<()> {
            PostgresRuntimeCounterProofStore::save_message_slot(self, slot)
        }

        fn save_message_variant(
            &self,
            variant: &MessageVariantWrite,
        ) -> CoreResult<MessageVariantRecord> {
            PostgresRuntimeCounterProofStore::save_message_variant(self, variant)
        }

        fn query_message_slots(
            &self,
            query: &MessageSlotQuery,
        ) -> CoreResult<Vec<MessageSlotRecord>> {
            PostgresRuntimeCounterProofStore::query_message_slots(self, query)
        }

        fn query_message_variants(
            &self,
            query: &MessageVariantQuery,
        ) -> CoreResult<Vec<MessageVariantRecord>> {
            PostgresRuntimeCounterProofStore::query_message_variants(self, query)
        }

        fn select_active_message_variant(
            &self,
            request: &SelectActiveVariantRequest,
        ) -> CoreResult<SelectActiveVariantResult> {
            PostgresRuntimeCounterProofStore::select_active_message_variant(self, request)
        }

        fn save_conversation_branch(
            &self,
            branch: &ConversationBranchWrite,
        ) -> CoreResult<ConversationBranchRecord> {
            PostgresRuntimeCounterProofStore::save_conversation_branch(self, branch)
        }

        fn query_conversation_branches(
            &self,
            query: &ConversationBranchQuery,
        ) -> CoreResult<Vec<ConversationBranchRecord>> {
            PostgresRuntimeCounterProofStore::query_conversation_branches(self, query)
        }

        fn get_conversation_branch_state(
            &self,
            session_id: &SessionId,
            default_updated_at: &IsoTimestamp,
        ) -> CoreResult<ConversationBranchStateRecord> {
            PostgresRuntimeCounterProofStore::get_conversation_branch_state(
                self,
                session_id,
                default_updated_at,
            )
        }

        fn select_active_conversation_branch(
            &self,
            request: &SelectActiveBranchRequest,
        ) -> CoreResult<SelectActiveBranchResult> {
            PostgresRuntimeCounterProofStore::select_active_conversation_branch(self, request)
        }

        fn update_conversation_branch_head(
            &self,
            request: &UpdateBranchHeadRequest,
        ) -> CoreResult<UpdateBranchHeadResult> {
            PostgresRuntimeCounterProofStore::update_conversation_branch_head(self, request)
        }

        fn save_conversation_snapshot(
            &self,
            snapshot: &ConversationSnapshotWrite,
        ) -> CoreResult<ConversationSnapshotRecord> {
            PostgresRuntimeCounterProofStore::save_conversation_snapshot(self, snapshot)
        }

        fn query_conversation_snapshots(
            &self,
            query: &ConversationSnapshotQuery,
        ) -> CoreResult<Vec<ConversationSnapshotRecord>> {
            PostgresRuntimeCounterProofStore::query_conversation_snapshots(self, query)
        }

        fn resolve_conversation_jump(
            &self,
            request: &ConversationJumpRequest,
        ) -> CoreResult<ConversationJumpResult> {
            PostgresRuntimeCounterProofStore::resolve_conversation_jump(self, request)
        }
    }

    impl AttachmentDataBankConformanceStore for PostgresRuntimeCounterProofStore {
        fn save_attachment(&self, attachment: &AttachmentWrite) -> CoreResult<AttachmentRecord> {
            PostgresRuntimeCounterProofStore::save_attachment(self, attachment)
        }

        fn query_attachments(&self, query: &AttachmentQuery) -> CoreResult<Vec<AttachmentRecord>> {
            PostgresRuntimeCounterProofStore::query_attachments(self, query)
        }

        fn remove_attachment(
            &self,
            attachment_id: &AttachmentId,
            updated_at: &IsoTimestamp,
        ) -> CoreResult<AttachmentRecord> {
            PostgresRuntimeCounterProofStore::remove_attachment(self, attachment_id, updated_at)
        }

        fn save_data_bank_scope(
            &self,
            scope: &DataBankScopeWrite,
        ) -> CoreResult<DataBankScopeRecord> {
            PostgresRuntimeCounterProofStore::save_data_bank_scope(self, scope)
        }

        fn query_data_bank_scopes(
            &self,
            query: &DataBankScopeQuery,
        ) -> CoreResult<Vec<DataBankScopeRecord>> {
            PostgresRuntimeCounterProofStore::query_data_bank_scopes(self, query)
        }

        fn remove_data_bank_scope(
            &self,
            scope_id: &DataBankScopeId,
            updated_at: &IsoTimestamp,
        ) -> CoreResult<DataBankScopeRecord> {
            PostgresRuntimeCounterProofStore::remove_data_bank_scope(self, scope_id, updated_at)
        }
    }

    impl ProfileMemoryConformanceStore for PostgresRuntimeCounterProofStore {
        fn memory_space_descriptors(&self) -> Vec<MemorySpaceDescriptor> {
            PostgresRuntimeCounterProofStore::memory_space_descriptors(self)
        }

        fn list_profile_memory(
            &self,
            query: &ProfileMemoryQuery,
        ) -> CoreResult<Vec<ProfileMemoryRecord>> {
            PostgresRuntimeCounterProofStore::list_profile_memory(self, query)
        }

        fn get_profile_memory(
            &self,
            profile_id: &ProfileId,
            target: &ProfileMemoryTarget,
            key: &str,
        ) -> CoreResult<Option<ProfileMemoryRecord>> {
            PostgresRuntimeCounterProofStore::get_profile_memory(self, profile_id, target, key)
        }

        fn add_profile_memory(
            &self,
            write: &ProfileMemoryWrite,
            caps: &ProfileMemoryCaps,
        ) -> CoreResult<ProfileMemoryRecord> {
            PostgresRuntimeCounterProofStore::add_profile_memory(self, write, caps)
        }

        fn replace_profile_memory(
            &self,
            replace: &ProfileMemoryReplace,
            caps: &ProfileMemoryCaps,
        ) -> CoreResult<ProfileMemoryRecord> {
            PostgresRuntimeCounterProofStore::replace_profile_memory(self, replace, caps)
        }

        fn remove_profile_memory(
            &self,
            delete: &ProfileMemoryDelete,
        ) -> CoreResult<ProfileMemoryRecord> {
            PostgresRuntimeCounterProofStore::remove_profile_memory(self, delete)
        }
    }

    impl RoleplayLoreConformanceStore for PostgresRuntimeCounterProofStore {
        fn memory_space_descriptors(&self) -> Vec<MemorySpaceDescriptor> {
            PostgresRuntimeCounterProofStore::memory_space_descriptors(self)
        }

        fn add_roleplay_lore_record(
            &self,
            write: &RoleplayLoreWrite,
        ) -> CoreResult<RoleplayLoreRecord> {
            PostgresRuntimeCounterProofStore::add_roleplay_lore_record(self, write)
        }

        fn replace_roleplay_lore_record(
            &self,
            replace: &RoleplayLoreReplace,
        ) -> CoreResult<RoleplayLoreRecord> {
            PostgresRuntimeCounterProofStore::replace_roleplay_lore_record(self, replace)
        }

        fn supersede_roleplay_lore_record(
            &self,
            supersede: &RoleplayLoreSupersede,
        ) -> CoreResult<(RoleplayLoreRecord, RoleplayLoreRecord)> {
            PostgresRuntimeCounterProofStore::supersede_roleplay_lore_record(self, supersede)
        }

        fn tombstone_roleplay_lore_record(
            &self,
            tombstone: &RoleplayLoreTombstone,
        ) -> CoreResult<RoleplayLoreRecord> {
            PostgresRuntimeCounterProofStore::tombstone_roleplay_lore_record(self, tombstone)
        }

        fn query_roleplay_lore_records(
            &self,
            query: &RoleplayLoreQuery,
        ) -> CoreResult<Vec<RoleplayLoreRecord>> {
            PostgresRuntimeCounterProofStore::query_roleplay_lore_records(self, query)
        }

        fn roleplay_lore_provenance_events(
            &self,
            record_id: &str,
        ) -> CoreResult<Vec<RoleplayLoreProvenanceEvent>> {
            PostgresRuntimeCounterProofStore::roleplay_lore_provenance_events(self, record_id)
        }
    }

    #[test]
    fn validates_schema_identifiers_before_connecting() {
        let error = match PostgresRuntimeCounterProofStore::connect(
            "postgres://example.invalid/db",
            "bad-schema",
        ) {
            Ok(_) => panic!("invalid schema unexpectedly connected"),
            Err(error) => error,
        };
        assert_eq!(error.kind, CoreErrorKind::InvalidInput);
    }

    #[test]
    fn postgres_proof_repository_groups_mark_unsupported_service_repositories() {
        let groups = postgres_proof_repository_groups();
        assert!(groups.iter().any(
            |group| group.group_id == "storage_admin" && group.notes[0].contains("implemented")
        ));
        assert!(groups
            .iter()
            .any(|group| group.group_id == "sessions_identities"
                && group.notes[0].contains("session/config/identity")));
        assert!(groups
            .iter()
            .any(|group| group.group_id == "events_projections"
                && group.notes[0].contains("event history")));
        assert!(groups
            .iter()
            .any(|group| group.group_id == "runtime_counters"
                && group.notes[0].contains("implemented")));
        assert!(groups
            .iter()
            .any(|group| group.group_id == "queues_messages"
                && group.notes[0].contains("queued-message TTL")));
        assert!(groups.iter().any(|group| group.group_id == "scheduler_jobs"
            && group.notes[0].contains("row-level claim conformance")));
        assert!(groups
            .iter()
            .any(|group| group.group_id == "worker_runs_completions"
                && group.notes[0].contains("terminal-status conformance")));
        assert!(groups
            .iter()
            .any(|group| group.group_id == "module_schema_registry"
                && group.notes[0].contains("simple_kv")));
        assert!(groups.iter().any(|group| group.group_id == "runtime_search"
            && group.notes[0].contains("runtime search entries")));
        assert!(groups.iter().any(|group| group.group_id == "provider_state"
            && group.notes[0].contains("provider wire-state conformance")));
        assert!(groups
            .iter()
            .any(|group| group.group_id == "conversations_attachments"
                && group.notes[0].contains("conversation transcript")
                && group.notes[0].contains("attachment/data-bank")));
        assert!(groups.iter().any(|group| group.group_id == "profile_memory"
            && group.notes[0].contains("profile_dense")
            && group.notes[0].contains("conformance")));
    }

    #[test]
    fn sqlite_session_event_conformance_matches_postgres_proof_contract() {
        let db_path = temp_sqlite_path("sqlite-session-event-conformance");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        session_event_conformance(&store);
        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn sqlite_queue_conformance_matches_postgres_proof_contract() {
        let db_path = temp_sqlite_path("sqlite-queue-conformance");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        queue_conformance(&store);
        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn sqlite_scheduler_conformance_matches_postgres_proof_contract() {
        let db_path = temp_sqlite_path("sqlite-scheduler-conformance");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        scheduler_conformance(&store);
        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn sqlite_worker_lifecycle_conformance_matches_postgres_proof_contract() {
        let db_path = temp_sqlite_path("sqlite-worker-lifecycle-conformance");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        worker_lifecycle_conformance(&store);
        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn sqlite_telemetry_maintenance_conformance_matches_postgres_proof_contract() {
        let db_path = temp_sqlite_path("sqlite-telemetry-maintenance-conformance");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        telemetry_maintenance_conformance(&store);
        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn sqlite_simple_kv_conformance_matches_postgres_proof_contract() {
        let db_path = temp_sqlite_path("sqlite-simple-kv-conformance");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        simple_kv_conformance(&store);
        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn sqlite_provider_wire_state_conformance_matches_postgres_proof_contract() {
        let db_path = temp_sqlite_path("sqlite-provider-wire-state-conformance");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        provider_wire_state_conformance(&store);
        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn sqlite_conversation_conformance_matches_postgres_proof_contract() {
        let db_path = temp_sqlite_path("sqlite-conversation-conformance");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        conversation_conformance(&store);
        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn sqlite_attachment_data_bank_conformance_matches_postgres_proof_contract() {
        let db_path = temp_sqlite_path("sqlite-attachment-data-bank-conformance");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        attachment_data_bank_conformance(&store);
        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn sqlite_profile_memory_conformance_matches_postgres_proof_contract() {
        let db_path = temp_sqlite_path("sqlite-profile-memory-conformance");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        profile_memory_conformance(&store);
        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn sqlite_roleplay_lore_conformance_matches_postgres_proof_contract() {
        let db_path = temp_sqlite_path("sqlite-roleplay-lore-conformance");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        roleplay_lore_conformance(&store);
        let _ = fs::remove_file(db_path);
    }

    #[test]
    #[ignore = "requires local PostgreSQL dev database env; source /home/system/database/rusty-crew-postgres.env or set RUSTY_CREW_DATABASE_URL"]
    fn postgres_session_event_proof_matches_sqlite_conformance_contract() {
        let Some(database_url) = postgres_test_database_url() else {
            eprintln!("skipping PostgreSQL proof; no database URL env is set");
            return;
        };
        let schema = unique_schema("rusty_crew_session_event_proof");
        let store = PostgresRuntimeCounterProofStore::connect(&database_url, &schema).unwrap();
        session_event_conformance(&store);
        store.drop_schema_for_test().unwrap();
    }

    #[test]
    #[ignore = "requires local PostgreSQL dev database env; source /home/system/database/rusty-crew-postgres.env or set RUSTY_CREW_DATABASE_URL"]
    fn postgres_queue_proof_matches_sqlite_conformance_contract() {
        let Some(database_url) = postgres_test_database_url() else {
            eprintln!("skipping PostgreSQL proof; no database URL env is set");
            return;
        };
        let schema = unique_schema("rusty_crew_queue_proof");
        let store = PostgresRuntimeCounterProofStore::connect(&database_url, &schema).unwrap();
        queue_conformance(&store);
        store.drop_schema_for_test().unwrap();
    }

    #[test]
    #[ignore = "requires local PostgreSQL dev database env; source /home/system/database/rusty-crew-postgres.env or set RUSTY_CREW_DATABASE_URL"]
    fn postgres_scheduler_proof_matches_sqlite_conformance_contract() {
        let Some(database_url) = postgres_test_database_url() else {
            eprintln!("skipping PostgreSQL scheduler proof; no database URL env is set");
            return;
        };
        let schema = unique_schema("rusty_crew_scheduler_proof");
        let store = PostgresRuntimeCounterProofStore::connect(&database_url, &schema).unwrap();
        scheduler_conformance(&store);

        let diagnostics = store.storage_diagnostics().unwrap();
        assert_eq!(
            diagnostics
                .table_counts
                .iter()
                .find(|count| count.table == "scheduled_jobs")
                .map(|count| count.rows),
            Some(1)
        );
        assert_eq!(
            diagnostics
                .table_counts
                .iter()
                .find(|count| count.table == "scheduled_job_runs")
                .map(|count| count.rows),
            Some(2)
        );
        assert!(diagnostics
            .capabilities
            .iter()
            .any(|capability| { capability.name == "row_level_claims" && capability.supported }));

        store.drop_schema_for_test().unwrap();
    }

    #[test]
    #[ignore = "requires local PostgreSQL dev database env; source /home/system/database/rusty-crew-postgres.env or set RUSTY_CREW_DATABASE_URL"]
    fn postgres_profile_registry_and_session_memory_governance_are_implemented() {
        let Some(database_url) = postgres_test_database_url() else {
            eprintln!("skipping PostgreSQL memory proof; no database URL env is set");
            return;
        };
        let schema = unique_schema("rusty_crew_profile_memory_governance_proof");
        let store = PostgresRuntimeCounterProofStore::connect(&database_url, &schema).unwrap();

        let created = store
            .create_profile_registry_record(&ProfileRegistryWrite {
                profile_id: ProfileId::new("runner_profile"),
                lifecycle_status: ProfileRegistryLifecycleStatus::Active,
                display_name: Some("Runner Profile".to_string()),
                summary: Some("PostgreSQL proof profile".to_string()),
                default_session_kind: Some(SessionKind::Full),
                agent_id: Some(AgentId::new("runner_agent")),
                owner_id: Some("patch".to_string()),
                active_runtime_settings_json: json!({"model": "gpt"}),
                source_asset_refs: Vec::new(),
                derived_runtime_refs: Vec::new(),
                import_export: ProfileRegistryImportExportMetadata {
                    imported_from: None,
                    imported_at: None,
                    exported_to: None,
                    exported_at: None,
                    metadata_json: json!({}),
                },
                now: "2026-06-27T01:00:00Z".to_string(),
            })
            .unwrap();
        assert_eq!(created.revision, 1);
        assert_eq!(
            store
                .list_profile_registry_records(&ProfileRegistryQuery {
                    lifecycle_status: Some(ProfileRegistryLifecycleStatus::Active),
                    page: None,
                })
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            store
                .get_profile_registry_record(&ProfileId::new("runner_profile"))
                .unwrap()
                .unwrap()
                .agent_id,
            Some(AgentId::new("runner_agent"))
        );

        let descriptor = session_memory_space_descriptor();
        let proposal = MemoryProposalEnvelope {
            proposal_id: "session_memory_add_proposal".to_string(),
            space_id: descriptor.space_id.clone(),
            operation: MemoryOperation::Add,
            scope: MemoryScope {
                scope_type: MemoryScopeType::Session,
                scope_id: "session-memory-proof".to_string(),
            },
            shape: MemoryRecordShapeRef {
                shape_id: MemoryRecordShapeId::unchecked("session_fact"),
                version: 1,
            },
            content: json!({
                "record_id": "session_fact_one",
                "content": "Postgres memory proposals can apply.",
                "fact_kind": "proof",
                "confidence": 0.91,
                "source_summary": "ignored proof test",
                "created_at": "2026-06-27T01:01:00Z",
                "updated_at": "2026-06-27T01:01:00Z"
            }),
            evidence_refs: vec![MemoryEvidenceRef {
                evidence_type: MemoryEvidenceKind::Wake,
                ref_id: "wake-proof".to_string(),
                label: Some("wake proof".to_string()),
            }],
            confidence: 0.91,
            durability_rationale: Some(
                "The user explicitly asked to preserve this proof fact.".to_string(),
            ),
            governance_mode: MemoryGovernanceMode::ManualReview,
            source: MemoryProposalSource::Human,
            dedupe_key: Some("session_fact_one".to_string()),
            created_at: Some("2026-06-27T01:01:00Z".to_string()),
        };
        let saved = store
            .save_memory_proposal(&proposal, &descriptor, &"2026-06-27T01:01:00Z".to_string())
            .unwrap();
        assert_eq!(saved.status, MemoryProposalReviewStatus::PendingReview);
        store
            .record_memory_governance_decision(
                &MemoryGovernanceDecisionInput {
                    decision_id: "approve_session_memory_add".to_string(),
                    proposal_id: proposal.proposal_id.clone(),
                    decision: MemoryGovernanceDecisionKind::Approved,
                    actor: "curator".to_string(),
                    source: MemoryProposalSource::Human,
                    evidence_refs: proposal.evidence_refs.clone(),
                    policy_mode: MemoryGovernanceMode::ManualReview,
                    confidence: Some(0.95),
                    message: Some("approve proof add".to_string()),
                    resulting_revision: None,
                    decided_at: None,
                },
                &"2026-06-27T01:02:00Z".to_string(),
            )
            .unwrap();
        let applied = store
            .record_memory_governance_decision(
                &MemoryGovernanceDecisionInput {
                    decision_id: "apply_session_memory_add".to_string(),
                    proposal_id: proposal.proposal_id.clone(),
                    decision: MemoryGovernanceDecisionKind::Applied,
                    actor: "curator".to_string(),
                    source: MemoryProposalSource::Human,
                    evidence_refs: proposal.evidence_refs.clone(),
                    policy_mode: MemoryGovernanceMode::ManualReview,
                    confidence: Some(0.95),
                    message: Some("apply proof add".to_string()),
                    resulting_revision: None,
                    decided_at: None,
                },
                &"2026-06-27T01:03:00Z".to_string(),
            )
            .unwrap();
        assert_eq!(applied.resulting_revision, Some(1));

        let records = store
            .query_session_memory_records(&SessionMemoryQuery {
                session_id: Some(SessionId::new("session-memory-proof")),
                ..SessionMemoryQuery::default()
            })
            .unwrap();
        assert_eq!(records.len(), 1);
        let context = store
            .build_session_memory_prompt_context(&BranchAwareSessionMemoryQuery {
                session_id: SessionId::new("session-memory-proof"),
                active_branch_id: None,
                include_ancestors: true,
                include_siblings: false,
                shape_id: None,
                prompt_context_only: true,
                page: None,
            })
            .unwrap();
        assert_eq!(context.records.len(), 1);
        assert_eq!(
            context.diagnostics.token_estimate,
            context.diagnostics.character_estimate.div_ceil(4)
        );

        let diagnostics = store.storage_diagnostics().unwrap();
        assert!(diagnostics
            .table_counts
            .iter()
            .any(|count| count.table == "profile_registry" && count.rows == 1));
        assert!(diagnostics
            .table_counts
            .iter()
            .any(|count| count.table == "memory_proposals" && count.rows == 1));
        assert!(diagnostics
            .table_counts
            .iter()
            .any(|count| count.table == "session_memory_records" && count.rows == 1));

        store.drop_schema_for_test().unwrap();
    }

    #[test]
    #[ignore = "requires local PostgreSQL dev database env; source /home/system/database/rusty-crew-postgres.env or set RUSTY_CREW_DATABASE_URL"]
    fn postgres_external_bindings_are_scoped_per_agent_without_secret_material() {
        let Some(database_url) = postgres_test_database_url() else {
            eprintln!("skipping PostgreSQL bindings proof; no database URL env is set");
            return;
        };
        let schema = unique_schema("rusty_crew_bindings_proof");
        let store = PostgresRuntimeCounterProofStore::connect(&database_url, &schema).unwrap();

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
            created_at: "2026-06-27T03:00:00Z".to_string(),
            updated_at: "2026-06-27T03:01:00Z".to_string(),
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
            updated_at: "2026-06-27T03:02:00Z".to_string(),
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
        assert!(!alpha_only[0]
            .provenance
            .notes
            .as_deref()
            .unwrap_or_default()
            .contains("token"));

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
                    last_checked_at: Some("2026-06-27T03:05:00Z".to_string()),
                    notes: Some("no secret fields".to_string()),
                },
                created_at: "2026-06-27T03:00:00Z".to_string(),
                updated_at: "2026-06-27T03:05:00Z".to_string(),
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
                    last_checked_at: Some("2026-06-27T03:06:00Z".to_string()),
                    notes: None,
                },
                created_at: "2026-06-27T03:00:00Z".to_string(),
                updated_at: "2026-06-27T03:06:00Z".to_string(),
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
        assert!(!alpha_mcp[0].endpoint_ref.contains("secret"));
        assert_eq!(degraded.len(), 1);
        assert_eq!(degraded[0].agent_id, AgentId::new("agent-beta"));
        assert_eq!(
            degraded[0].diagnostics.last_error.as_deref(),
            Some("catalog revision mismatch")
        );

        let diagnostics = store.storage_diagnostics().unwrap();
        assert!(diagnostics
            .table_counts
            .iter()
            .any(|count| count.table == "channel_bindings" && count.rows == 2));
        assert!(diagnostics
            .table_counts
            .iter()
            .any(|count| count.table == "mcp_bindings" && count.rows == 2));

        store.drop_schema_for_test().unwrap();
    }

    #[test]
    #[ignore = "requires local PostgreSQL dev database env; source /home/system/database/rusty-crew-postgres.env or set RUSTY_CREW_DATABASE_URL"]
    fn postgres_message_variant_reorder_and_delete_are_implemented() {
        let Some(database_url) = postgres_test_database_url() else {
            eprintln!("skipping PostgreSQL message variant proof; no database URL env is set");
            return;
        };
        let schema = unique_schema("rusty_crew_variant_mutation_proof");
        let store = PostgresRuntimeCounterProofStore::connect(&database_url, &schema).unwrap();
        seed_conversation_base_fixture(&store, "session-conversation", "slot-conversation");

        let reordered = store
            .reorder_message_variants(
                &MessageSlotId::new("slot-conversation"),
                &[
                    MessageVariantId::new("variant-conversation-b"),
                    MessageVariantId::new("variant-conversation-a"),
                ],
                &"2026-06-27T02:00:00Z".to_string(),
            )
            .unwrap();
        assert_eq!(
            reordered
                .iter()
                .map(|variant| variant.variant_id.0.as_str())
                .collect::<Vec<_>>(),
            vec![
                "variant-conversation-primary",
                "variant-conversation-b",
                "variant-conversation-a"
            ]
        );

        store
            .select_active_message_variant(&SelectActiveVariantRequest {
                slot_id: MessageSlotId::new("slot-conversation"),
                active_variant_id: Some(MessageVariantId::new("variant-conversation-a")),
                expected: ActiveVariantExpectation::Primary,
                updated_at: "2026-06-27T02:01:00Z".to_string(),
            })
            .unwrap();
        let slot = store
            .delete_message_variant(
                &MessageSlotId::new("slot-conversation"),
                &MessageVariantId::new("variant-conversation-a"),
                &"2026-06-27T02:02:00Z".to_string(),
            )
            .unwrap();
        assert_eq!(slot.active_variant_id, None);
        assert_eq!(slot.alternates.len(), 1);
        assert_eq!(
            slot.alternates[0].variant_id,
            MessageVariantId::new("variant-conversation-b")
        );

        store.drop_schema_for_test().unwrap();
    }

    #[test]
    #[ignore = "requires local PostgreSQL dev database env; source /home/system/database/rusty-crew-postgres.env or set RUSTY_CREW_DATABASE_URL"]
    fn postgres_worker_lifecycle_proof_matches_sqlite_conformance_contract() {
        let Some(database_url) = postgres_test_database_url() else {
            eprintln!("skipping PostgreSQL worker lifecycle proof; no database URL env is set");
            return;
        };
        let schema = unique_schema("rusty_crew_worker_lifecycle_proof");
        let store = PostgresRuntimeCounterProofStore::connect(&database_url, &schema).unwrap();
        worker_lifecycle_conformance(&store);

        let diagnostics = store.storage_diagnostics().unwrap();
        assert_eq!(
            diagnostics
                .table_counts
                .iter()
                .find(|count| count.table == "worker_runs")
                .map(|count| count.rows),
            Some(2)
        );
        assert_eq!(
            diagnostics
                .table_counts
                .iter()
                .find(|count| count.table == "completion_packets")
                .map(|count| count.rows),
            Some(1)
        );
        assert!(diagnostics.repository_groups.iter().any(|group| {
            group.group_id == "worker_runs_completions"
                && group.notes[0].contains("terminal-status conformance")
        }));

        store.drop_schema_for_test().unwrap();
    }

    #[test]
    #[ignore = "requires local PostgreSQL dev database env; source /home/system/database/rusty-crew-postgres.env or set RUSTY_CREW_DATABASE_URL"]
    fn postgres_telemetry_maintenance_proof_matches_sqlite_conformance_contract() {
        let Some(database_url) = postgres_test_database_url() else {
            eprintln!(
                "skipping PostgreSQL telemetry/maintenance proof; no database URL env is set"
            );
            return;
        };
        let schema = unique_schema("rusty_crew_telemetry_maintenance_proof");
        let store = PostgresRuntimeCounterProofStore::connect(&database_url, &schema).unwrap();
        telemetry_maintenance_conformance(&store);

        let diagnostics = store.storage_diagnostics().unwrap();
        assert_eq!(diagnostics.backend, "postgres");
        assert_eq!(diagnostics.schema_version, POSTGRES_PROOF_SCHEMA_VERSION);
        assert!(diagnostics
            .table_counts
            .iter()
            .any(|count| count.table == "tool_call_history" && count.rows == 2));
        assert!(diagnostics
            .table_counts
            .iter()
            .any(|count| count.table == "queued_messages" && count.rows == 0));

        store.drop_schema_for_test().unwrap();
    }

    #[test]
    #[ignore = "requires local PostgreSQL dev database env; source /home/system/database/rusty-crew-postgres.env or set RUSTY_CREW_DATABASE_URL"]
    fn postgres_runtime_counter_proof_matches_typed_counter_contract() {
        let Some(database_url) = postgres_test_database_url() else {
            eprintln!("skipping PostgreSQL proof; no database URL env is set");
            return;
        };
        let schema = unique_schema("rusty_crew_counter_proof");
        let store = PostgresRuntimeCounterProofStore::connect(&database_url, &schema).unwrap();

        store
            .increment_counter(
                &RuntimeCounterScope::Runtime,
                COUNTER_MESSAGES,
                2,
                &"2026-06-26T00:00:00Z".to_string(),
            )
            .unwrap();
        store
            .increment_counter(
                &RuntimeCounterScope::Session(crate::SessionId::new("session-alpha")),
                COUNTER_WAKES,
                1,
                &"2026-06-26T00:00:01Z".to_string(),
            )
            .unwrap();

        let runtime_summary = store
            .runtime_summary(&RuntimeCounterScope::Runtime)
            .unwrap();
        let session_summary = store
            .runtime_summary(&RuntimeCounterScope::Session(crate::SessionId::new(
                "session-alpha",
            )))
            .unwrap();
        assert_eq!(runtime_summary.messages, 2);
        assert_eq!(session_summary.wakes, 1);

        let messages = store
            .query_runtime_counters(&RuntimeCounterQuery {
                scope: Some(RuntimeCounterScope::Runtime),
                counter_name: Some(COUNTER_MESSAGES.to_string()),
                page: None,
            })
            .unwrap();
        assert_eq!(messages[0].value, 2);
        assert_eq!(
            store
                .reset_runtime_counters(
                    &RuntimeCounterQuery {
                        scope: Some(RuntimeCounterScope::Runtime),
                        counter_name: Some(COUNTER_MESSAGES.to_string()),
                        page: None,
                    },
                    "2026-06-26T00:00:02Z".to_string(),
                )
                .unwrap(),
            1
        );
        assert_eq!(
            store
                .runtime_summary(&RuntimeCounterScope::Runtime)
                .unwrap()
                .messages,
            0
        );

        let diagnostics = store.storage_diagnostics().unwrap();
        assert_eq!(diagnostics.backend, "postgres");
        assert_eq!(diagnostics.schema_version, POSTGRES_PROOF_SCHEMA_VERSION);
        assert_eq!(diagnostics.repository_groups[0].group_id, "storage_admin");
        assert_eq!(
            diagnostics
                .table_counts
                .iter()
                .find(|count| count.table == "runtime_counters")
                .map(|count| count.rows),
            Some(2)
        );

        store.drop_schema_for_test().unwrap();
    }

    #[test]
    #[ignore = "requires local PostgreSQL dev database env; source /home/system/database/rusty-crew-postgres.env or set RUSTY_CREW_DATABASE_URL"]
    fn postgres_simple_kv_proof_matches_sqlite_conformance_contract() {
        let Some(database_url) = postgres_test_database_url() else {
            eprintln!("skipping PostgreSQL simple_kv proof; no database URL env is set");
            return;
        };
        let schema = unique_schema("rusty_crew_simple_kv_proof");
        let store = PostgresRuntimeCounterProofStore::connect(&database_url, &schema).unwrap();
        simple_kv_conformance(&store);

        let diagnostics = store.storage_diagnostics().unwrap();
        assert_eq!(
            diagnostics
                .table_counts
                .iter()
                .find(|count| count.table == "module_simple_kv_entries")
                .map(|count| count.rows),
            Some(0)
        );
        assert!(diagnostics.repository_groups.iter().any(|group| {
            group.group_id == "module_schema_registry" && group.notes[0].contains("simple_kv")
        }));

        store.drop_schema_for_test().unwrap();
    }

    #[test]
    #[ignore = "requires local PostgreSQL dev database env; source /home/system/database/rusty-crew-postgres.env or set RUSTY_CREW_DATABASE_URL"]
    fn postgres_runtime_search_proof_matches_typed_search_contract() {
        let Some(database_url) = postgres_test_database_url() else {
            eprintln!("skipping PostgreSQL runtime search proof; no database URL env is set");
            return;
        };
        let schema = unique_schema("rusty_crew_runtime_search_proof");
        let store = PostgresRuntimeCounterProofStore::connect(&database_url, &schema).unwrap();
        runtime_search_conformance(&store);

        let diagnostics = store.storage_diagnostics().unwrap();
        assert_eq!(
            diagnostics
                .table_counts
                .iter()
                .find(|count| count.table == "runtime_search_entries")
                .map(|count| count.rows),
            Some(5)
        );
        assert!(diagnostics.capabilities.iter().any(|capability| {
            capability.name == "runtime_full_text_search" && capability.supported
        }));
        assert!(diagnostics.repository_groups.iter().any(|group| {
            group.group_id == "runtime_search" && group.notes[0].contains("implemented")
        }));

        store.drop_schema_for_test().unwrap();
    }

    #[test]
    #[ignore = "requires local PostgreSQL dev database env; source /home/system/database/rusty-crew-postgres.env or set RUSTY_CREW_DATABASE_URL"]
    fn postgres_provider_wire_state_proof_matches_sqlite_conformance_contract() {
        let Some(database_url) = postgres_test_database_url() else {
            eprintln!("skipping PostgreSQL provider wire-state proof; no database URL env is set");
            return;
        };
        let schema = unique_schema("rusty_crew_provider_state_proof");
        let store = PostgresRuntimeCounterProofStore::connect(&database_url, &schema).unwrap();
        provider_wire_state_conformance(&store);

        let diagnostics = store.storage_diagnostics().unwrap();
        assert_eq!(
            diagnostics
                .table_counts
                .iter()
                .find(|count| count.table == "provider_wire_states")
                .map(|count| count.rows),
            Some(9)
        );
        assert!(diagnostics.repository_groups.iter().any(|group| {
            group.group_id == "provider_state" && group.notes[0].contains("implemented")
        }));

        store.drop_schema_for_test().unwrap();
    }

    #[test]
    #[ignore = "requires local PostgreSQL dev database env; source /home/system/database/rusty-crew-postgres.env or set RUSTY_CREW_DATABASE_URL"]
    fn postgres_conversation_proof_matches_sqlite_conformance_contract() {
        let Some(database_url) = postgres_test_database_url() else {
            eprintln!("skipping PostgreSQL conversation proof; no database URL env is set");
            return;
        };
        let schema = unique_schema("rusty_crew_conversation_proof");
        let store = PostgresRuntimeCounterProofStore::connect(&database_url, &schema).unwrap();
        conversation_conformance(&store);

        let diagnostics = store.storage_diagnostics().unwrap();
        assert_eq!(
            diagnostics
                .table_counts
                .iter()
                .find(|count| count.table == "conversation_branches")
                .map(|count| count.rows),
            Some(2)
        );
        assert_eq!(
            diagnostics
                .table_counts
                .iter()
                .find(|count| count.table == "message_variants")
                .map(|count| count.rows),
            Some(3)
        );
        assert!(diagnostics.repository_groups.iter().any(|group| {
            group.group_id == "conversations_attachments"
                && group.notes[0].contains("conversation transcript")
                && group.notes[0].contains("attachment/data-bank")
        }));

        store.drop_schema_for_test().unwrap();
    }

    #[test]
    #[ignore = "requires local PostgreSQL dev database env; source /home/system/database/rusty-crew-postgres.env or set RUSTY_CREW_DATABASE_URL"]
    fn postgres_attachment_data_bank_proof_matches_sqlite_conformance_contract() {
        let Some(database_url) = postgres_test_database_url() else {
            eprintln!("skipping PostgreSQL attachment/data-bank proof; no database URL env is set");
            return;
        };
        let schema = unique_schema("rusty_crew_attachment_data_bank_proof");
        let store = PostgresRuntimeCounterProofStore::connect(&database_url, &schema).unwrap();
        attachment_data_bank_conformance(&store);

        let diagnostics = store.storage_diagnostics().unwrap();
        assert_eq!(
            diagnostics
                .table_counts
                .iter()
                .find(|count| count.table == "attachments")
                .map(|count| count.rows),
            Some(3)
        );
        assert_eq!(
            diagnostics
                .table_counts
                .iter()
                .find(|count| count.table == "attachment_links")
                .map(|count| count.rows),
            Some(3)
        );
        assert_eq!(
            diagnostics
                .table_counts
                .iter()
                .find(|count| count.table == "data_bank_scopes")
                .map(|count| count.rows),
            Some(2)
        );
        assert!(diagnostics.repository_groups.iter().any(|group| {
            group.group_id == "conversations_attachments"
                && group.notes[0].contains("attachment/data-bank")
        }));

        store.drop_schema_for_test().unwrap();
    }

    #[test]
    #[ignore = "requires local PostgreSQL dev database env; source /home/system/database/rusty-crew-postgres.env or set RUSTY_CREW_DATABASE_URL"]
    fn postgres_profile_memory_proof_matches_sqlite_conformance_contract() {
        let Some(database_url) = postgres_test_database_url() else {
            eprintln!("skipping PostgreSQL profile memory proof; no database URL env is set");
            return;
        };
        let schema = unique_schema("rusty_crew_profile_memory_proof");
        let store = PostgresRuntimeCounterProofStore::connect(&database_url, &schema).unwrap();
        profile_memory_conformance(&store);

        let diagnostics = store.storage_diagnostics().unwrap();
        assert_eq!(
            diagnostics
                .table_counts
                .iter()
                .find(|count| count.table == "profile_memories")
                .map(|count| count.rows),
            Some(2)
        );
        assert!(diagnostics.repository_groups.iter().any(|group| {
            group.group_id == "profile_memory"
                && group.notes[0].contains("profile_dense")
                && group.notes[0].contains("conformance")
        }));

        store.drop_schema_for_test().unwrap();
    }

    #[test]
    #[ignore = "requires local PostgreSQL dev database env; source /home/system/database/rusty-crew-postgres.env or set RUSTY_CREW_DATABASE_URL"]
    fn postgres_roleplay_lore_proof_matches_sqlite_conformance_contract() {
        let Some(database_url) = postgres_test_database_url() else {
            eprintln!("skipping PostgreSQL roleplay lore proof; no database URL env is set");
            return;
        };
        let schema = unique_schema("rusty_crew_roleplay_lore_proof");
        let store = PostgresRuntimeCounterProofStore::connect(&database_url, &schema).unwrap();
        roleplay_lore_conformance(&store);

        let diagnostics = store.storage_diagnostics().unwrap();
        assert_eq!(
            diagnostics
                .table_counts
                .iter()
                .find(|count| count.table == "module_roleplay_lore_records")
                .map(|count| count.rows),
            Some(5)
        );
        assert_eq!(
            diagnostics
                .table_counts
                .iter()
                .find(|count| count.table == "module_roleplay_lore_provenance_events")
                .map(|count| count.rows),
            Some(7)
        );

        store.drop_schema_for_test().unwrap();
    }

    #[test]
    #[ignore = "requires local PostgreSQL dev database env; source /home/system/database/rusty-crew-postgres.env or set RUSTY_CREW_DATABASE_URL"]
    fn postgres_conversation_conflicts_apply_once_across_connections() {
        let Some(database_url) = postgres_test_database_url() else {
            eprintln!(
                "skipping PostgreSQL conversation conflict proof; no database URL env is set"
            );
            return;
        };
        let schema = unique_schema("rusty_crew_conversation_conflict_proof");
        let setup = PostgresRuntimeCounterProofStore::connect(&database_url, &schema).unwrap();
        seed_conversation_conflict_fixture(&setup);

        let first = PostgresRuntimeCounterProofStore::connect(&database_url, &schema).unwrap();
        let second = PostgresRuntimeCounterProofStore::connect(&database_url, &schema).unwrap();

        let active_branch_first = first
            .select_active_conversation_branch(&SelectActiveBranchRequest {
                session_id: SessionId::new("session-conflict"),
                active_branch_id: Some(ConversationBranchId::new("branch-conflict-a")),
                expected: ActiveBranchExpectation::None,
                updated_at: "2026-06-26T03:01:00Z".to_string(),
            })
            .unwrap();
        let active_branch_second = second
            .select_active_conversation_branch(&SelectActiveBranchRequest {
                session_id: SessionId::new("session-conflict"),
                active_branch_id: Some(ConversationBranchId::new("branch-conflict-b")),
                expected: ActiveBranchExpectation::None,
                updated_at: "2026-06-26T03:01:01Z".to_string(),
            })
            .unwrap();
        assert!(active_branch_first.conflict.is_none());
        assert_eq!(
            active_branch_second.conflict.unwrap().actual,
            Some(ConversationBranchId::new("branch-conflict-a"))
        );

        let head_first = first
            .update_conversation_branch_head(&UpdateBranchHeadRequest {
                branch_id: ConversationBranchId::new("branch-conflict-a"),
                head_message_id: Some(MessageId::new("message-conflict-a")),
                expected: BranchHeadExpectation::None,
                updated_at: "2026-06-26T03:02:00Z".to_string(),
            })
            .unwrap();
        let head_second = second
            .update_conversation_branch_head(&UpdateBranchHeadRequest {
                branch_id: ConversationBranchId::new("branch-conflict-a"),
                head_message_id: Some(MessageId::new("message-conflict-b")),
                expected: BranchHeadExpectation::None,
                updated_at: "2026-06-26T03:02:01Z".to_string(),
            })
            .unwrap();
        assert!(head_first.conflict.is_none());
        assert_eq!(
            head_second.conflict.unwrap().actual,
            Some(MessageId::new("message-conflict-a"))
        );

        let variant_first = first
            .select_active_message_variant(&SelectActiveVariantRequest {
                slot_id: MessageSlotId::new("slot-conflict"),
                active_variant_id: Some(MessageVariantId::new("variant-conflict-a")),
                expected: ActiveVariantExpectation::Primary,
                updated_at: "2026-06-26T03:03:00Z".to_string(),
            })
            .unwrap();
        let variant_second = second
            .select_active_message_variant(&SelectActiveVariantRequest {
                slot_id: MessageSlotId::new("slot-conflict"),
                active_variant_id: Some(MessageVariantId::new("variant-conflict-b")),
                expected: ActiveVariantExpectation::Primary,
                updated_at: "2026-06-26T03:03:01Z".to_string(),
            })
            .unwrap();
        assert!(variant_first.conflict.is_none());
        assert_eq!(
            variant_second.conflict.unwrap().actual,
            Some(MessageVariantId::new("variant-conflict-a"))
        );

        setup.drop_schema_for_test().unwrap();
    }

    fn session_event_conformance(store: &dyn SessionEventConformanceStore) {
        let session = proof_session_state();
        let config = proof_session_config();
        store.save_session_with_config(&session, &config).unwrap();

        let sessions = store.load_sessions().unwrap();
        let configs = store.load_session_configs().unwrap();
        let identities = store.load_session_identities().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, SessionId::new("session-alpha"));
        assert_eq!(sessions[0].tool_profile.tools[0].name, "apply_patch");
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
                    session_id: SessionId::new("session-alpha"),
                    wake_id: Some("wake-conformance".to_string()),
                    event: BrainEvent::Started,
                },
            )
            .unwrap();

        let history = store.load_event_history().unwrap();
        assert_eq!(
            history
                .iter()
                .map(|event| event.sequence)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
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

        assert_eq!(all.len(), 3);
        assert_eq!(by_session.len(), 2);
        assert_eq!(by_agent.len(), 1);
        assert_eq!(by_agent[0].agent_ids.len(), 2);
        assert_eq!(by_correlation[0].sequence, 2);
        assert_eq!(by_wake[0].source_wake_ids, vec!["wake-conformance"]);
    }

    fn proof_session_state() -> SessionState {
        SessionState {
            handle: SessionHandle::new(1),
            session_id: SessionId::new("session-alpha"),
            agent_id: AgentId::new("agent-alpha"),
            profile_id: ProfileId::new("full-profile"),
            kind: SessionKind::Full,
            delegation: None,
            resource_limits: proof_resource_limits(),
            tool_profile: proof_tool_profile(),
            history_window: None,
            status: SessionStatus::Idle,
            brain_turn_count: 0,
            created_at: "2026-06-20T00:00:00Z".to_string(),
            last_active_at: "2026-06-20T00:00:00Z".to_string(),
        }
    }

    fn proof_session_config() -> SessionConfig {
        SessionConfig {
            session_id: SessionId::new("session-alpha"),
            agent_id: AgentId::new("agent-alpha"),
            profile_id: ProfileId::new("full-profile"),
            kind: SessionKind::Full,
            delegation: None,
            resource_limits: proof_resource_limits(),
            tool_profile: proof_tool_profile(),
            history_window: None,
        }
    }

    fn proof_resource_limits() -> ResourceLimits {
        ResourceLimits {
            workdir: Some("/tmp/rusty-crew-test".to_string()),
            max_duration_ms: Some(60_000),
            max_delegation_depth: Some(4),
        }
    }

    fn proof_tool_profile() -> ToolProfile {
        ToolProfile {
            tools: vec![ToolDescriptor {
                name: "apply_patch".to_string(),
                description: "Apply a source patch".to_string(),
                input_schema: None,
            }],
        }
    }

    fn page() -> QueryPage {
        QueryPage {
            limit: Some(100),
            offset: Some(0),
        }
    }

    fn queue_conformance(store: &dyn QueueConformanceStore) {
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
    }

    fn pending_queue_messages(store: &dyn QueueConformanceStore) -> Vec<QueuedMessageRecord> {
        store
            .load_queued_messages(&QueuedMessageFilter {
                state: Some(QueuedMessageState::Pending),
                owner_session_id: Some(SessionId::new("session-alpha")),
                owner_agent_id: Some(AgentId::new("agent-alpha")),
                limit: Some(10),
            })
            .unwrap()
    }

    fn scheduler_conformance(store: &dyn SchedulerConformanceStore) {
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
        let completed = scheduled_runs(store, Some(ScheduledRunStatus::Completed));
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].output_json, json!({"woke": true}));

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
        assert!(store
            .expire_stale_scheduled_runs(
                &"2026-06-20T06:10:00Z".to_string(),
                &"2026-06-20T06:11:00Z".to_string(),
            )
            .unwrap()
            .is_empty());
    }

    fn scheduled_runs(
        store: &dyn SchedulerConformanceStore,
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

    fn worker_lifecycle_conformance(store: &dyn WorkerLifecycleConformanceStore) {
        let parent_session_id = SessionId::new("parent-session");
        let delegated_session_id = SessionId::new("delegated-alpha");
        let run_id = RunId::new("worker-run-alpha");
        let sibling_run_id = RunId::new("worker-run-beta");
        store
            .save_worker_run_requested(&WorkerRunRecord {
                run_id: run_id.clone(),
                parent_session_id: parent_session_id.clone(),
                delegated_session_id: Some(delegated_session_id.clone()),
                parent_agent_id: Some(AgentId::new("agent-parent")),
                profile_id: ProfileId::new("delegated-profile"),
                task_id: Some(TaskId::new("task-alpha")),
                status: WorkerRunStatus::Requested,
                created_at: "2026-06-20T07:00:00Z".to_string(),
                last_updated_at: "2026-06-20T07:00:00Z".to_string(),
                source_wake_id: "wake-alpha".to_string(),
                source_action_index: 2,
                delegation_correlation_id: Some("corr-alpha".to_string()),
                parent_consumption: ParentConsumptionPolicy::AwaitCompletion,
                fan_out_group_id: Some("group-alpha".to_string()),
                fan_out_max_concurrency: Some(2),
                fan_out_failure_policy: FanOutFailurePolicy::FailFast,
            })
            .unwrap();
        store
            .save_worker_run_requested(&WorkerRunRecord {
                run_id: sibling_run_id.clone(),
                parent_session_id: parent_session_id.clone(),
                delegated_session_id: Some(SessionId::new("delegated-beta")),
                parent_agent_id: Some(AgentId::new("agent-parent")),
                profile_id: ProfileId::new("delegated-profile"),
                task_id: Some(TaskId::new("task-beta")),
                status: WorkerRunStatus::Running,
                created_at: "2026-06-20T07:00:01Z".to_string(),
                last_updated_at: "2026-06-20T07:00:01Z".to_string(),
                source_wake_id: "wake-alpha".to_string(),
                source_action_index: 3,
                delegation_correlation_id: Some("corr-beta".to_string()),
                parent_consumption: ParentConsumptionPolicy::ObserveOnly,
                fan_out_group_id: Some("group-alpha".to_string()),
                fan_out_max_concurrency: Some(2),
                fan_out_failure_policy: FanOutFailurePolicy::FailSoft,
            })
            .unwrap();

        assert_eq!(
            store.load_worker_run(&run_id).unwrap().unwrap().status,
            WorkerRunStatus::Requested
        );
        store
            .update_worker_run_status_by_delegated_session(
                &delegated_session_id,
                WorkerRunStatus::Running,
                "2026-06-20T07:00:10Z".to_string(),
            )
            .unwrap();
        assert_eq!(
            store
                .load_worker_run_by_delegated_session(&delegated_session_id)
                .unwrap()
                .unwrap()
                .status,
            WorkerRunStatus::Running
        );
        store
            .update_worker_run_status(
                &run_id,
                WorkerRunStatus::Completed,
                "2026-06-20T07:01:00Z".to_string(),
            )
            .unwrap();
        assert_eq!(
            store
                .query_worker_runs(&WorkerRunQuery {
                    parent_session_id: Some(parent_session_id.clone()),
                    terminal: Some(true),
                    page: Some(page()),
                    ..WorkerRunQuery::default()
                })
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            store
                .query_worker_runs(&WorkerRunQuery {
                    parent_session_id: Some(parent_session_id.clone()),
                    terminal: Some(false),
                    page: Some(page()),
                    ..WorkerRunQuery::default()
                })
                .unwrap()
                .len(),
            1
        );

        store
            .save_event(
                10,
                &CoreEvent::CompletionPacketDelivered {
                    packet: CompletionPacket {
                        session_id: delegated_session_id.clone(),
                        status: CompletionStatus::Completed,
                        summary: "worker completed".to_string(),
                    },
                },
            )
            .unwrap();
        let completions = store
            .query_completion_packets(&CompletionPacketQuery {
                session_id: Some(delegated_session_id.clone()),
                status: Some(CompletionStatus::Completed),
                page: Some(page()),
            })
            .unwrap();
        assert_eq!(completions.len(), 1);
        assert_eq!(completions[0].sequence, 10);
        assert_eq!(completions[0].packet.summary, "worker completed");

        let delegated = store
            .delegated_completions_for_parent(&parent_session_id)
            .unwrap();
        assert_eq!(delegated.len(), 1);
        assert_eq!(delegated[0].run_id, run_id);
        assert_eq!(delegated[0].child_session_id, delegated_session_id);
        assert_eq!(
            delegated[0].parent_consumption,
            ParentConsumptionPolicy::AwaitCompletion
        );
    }

    fn telemetry_maintenance_conformance(store: &dyn TelemetryMaintenanceConformanceStore) {
        let session = proof_session_state();
        let config = proof_session_config();
        store.save_session_with_config(&session, &config).unwrap();

        let metadata = ToolCallMetadata {
            source: ToolCallSource::Mcp,
            adapter_id: None,
            binding_id: Some("binding-alpha".to_string()),
            server_names: vec!["den".to_string()],
            profile_id: Some(ProfileId::new("full-profile")),
            tool_profile_key: Some("planner".to_string()),
            source_tool_name: Some("den.get_task".to_string()),
            catalog_revision: Some("rev-1".to_string()),
            policy: None,
        };

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
                        tool_name: "den.get_task".to_string(),
                        metadata: Some(metadata.clone()),
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
                        tool_name: "den.get_task".to_string(),
                        is_error: true,
                        metadata: Some(metadata),
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
                        delegated_session_id: SessionId::new("delegated-alpha"),
                        run_id: Some(RunId::new("wake-tools:0")),
                        phase: rusty_crew_core_protocol::DelegationLifecyclePhase::Created,
                        detail: None,
                    },
                },
            )
            .unwrap();

        let tool_calls = store.load_tool_call_history().unwrap();
        assert_eq!(tool_calls.len(), 2);
        assert_eq!(tool_calls[0].phase, ToolCallPhase::Started);
        assert_eq!(tool_calls[1].phase, ToolCallPhase::Finished);
        assert_eq!(tool_calls[1].is_error, Some(true));
        assert_eq!(
            tool_calls[0].metadata.as_ref().unwrap().server_names,
            vec!["den"]
        );

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
        assert_eq!(session_summary.tool_calls, 1);
        assert_eq!(agent_summary.messages, 1);

        store
            .save_queued_message(&QueuedMessageRecord {
                message_id: "maintenance-queue".to_string(),
                owner_session_id: Some(SessionId::new("session-alpha")),
                owner_agent_id: AgentId::new("agent-alpha"),
                message: AgentMessage {
                    from: AgentId::new("operator"),
                    to: AgentId::new("agent-alpha"),
                    body: "expire me".to_string(),
                    correlation_id: Some("maintenance".to_string()),
                },
                source_sequence: Some(42),
                enqueued_at: "2026-06-20T08:00:00Z".to_string(),
                expires_at: "2026-06-20T08:00:05Z".to_string(),
                ttl_ms: 5_000,
                delivery_attempts: 0,
                state: QueuedMessageState::Pending,
                terminal_at: None,
                state_reason: None,
            })
            .unwrap();
        store
            .save_provider_wire_state(&ProviderWireStateWrite {
                key: provider_wire_state_key("session-alpha", "openai-responses", "replay"),
                profile_fingerprint: "profile-v1".to_string(),
                provider_fingerprint: "provider-v1".to_string(),
                payload_version: "v1".to_string(),
                payload_json: json!({"response_id": "maintenance"}),
                now: "2026-06-20T08:00:00Z".to_string(),
                expires_at: Some("2026-06-20T08:00:05Z".to_string()),
                last_wake_id: Some("wake-maintenance".to_string()),
            })
            .unwrap();

        let report = store
            .run_maintenance(&RuntimeMaintenancePolicy {
                expire_queued_messages_at: Some("2026-06-20T08:00:06Z".to_string()),
                purge_terminal_queued_messages_before: Some("2026-06-20T08:00:07Z".to_string()),
                expire_provider_wire_states_at: Some("2026-06-20T08:00:06Z".to_string()),
                run_wal_checkpoint: true,
                run_optimize: true,
                ..RuntimeMaintenancePolicy::default()
            })
            .unwrap();
        assert_eq!(report.expired_queue_messages, 1);
        assert_eq!(report.purged_terminal_queue_messages, 1);
        assert_eq!(report.expired_provider_wire_states, 1);
        assert!(store
            .load_queued_messages(&QueuedMessageFilter {
                state: Some(QueuedMessageState::Expired),
                owner_session_id: Some(SessionId::new("session-alpha")),
                owner_agent_id: Some(AgentId::new("agent-alpha")),
                limit: Some(10),
            })
            .unwrap()
            .is_empty());
    }

    fn simple_kv_conformance(store: &dyn SimpleKvConformanceStore) {
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
    }

    fn runtime_search_conformance(store: &PostgresRuntimeCounterProofStore) {
        for entry in runtime_search_fixture() {
            store.upsert_runtime_search_entry(&entry).unwrap();
        }

        let session = store
            .search_runtime(&RuntimeSearchFilter {
                query: "tools".to_string(),
                row_type: Some(RuntimeSearchRowType::Session),
                session_id: Some(crate::SessionId::new("session-alpha")),
                agent_id: None,
                instance_id: None,
                task_id: None,
                event_kind: Some(crate::CoreEventKind::SessionCreated),
                recorded_after: None,
                recorded_before: None,
                limit: Some(10),
            })
            .unwrap();
        assert_eq!(session.len(), 1);
        assert_eq!(session[0].row_type, RuntimeSearchRowType::Session);

        let beta_message = store
            .search_runtime(&RuntimeSearchFilter {
                query: "needle".to_string(),
                row_type: Some(RuntimeSearchRowType::Message),
                session_id: None,
                agent_id: Some(crate::AgentId::new("agent-beta")),
                instance_id: None,
                task_id: None,
                event_kind: Some(crate::CoreEventKind::AgentMessageRouted),
                recorded_after: Some("2026-06-26T00:00:00Z".to_string()),
                recorded_before: Some("2026-06-26T00:10:00Z".to_string()),
                limit: Some(10),
            })
            .unwrap();
        assert_eq!(beta_message.len(), 1);
        assert_eq!(beta_message[0].row_key, "message:1:agent-beta");
        assert_eq!(beta_message[0].sequence, Some(1));

        let queued = store
            .search_runtime(&RuntimeSearchFilter {
                query: "needle".to_string(),
                row_type: Some(RuntimeSearchRowType::QueueMessage),
                session_id: Some(crate::SessionId::new("session-alpha")),
                agent_id: Some(crate::AgentId::new("agent-alpha")),
                instance_id: None,
                task_id: None,
                event_kind: None,
                recorded_after: None,
                recorded_before: None,
                limit: Some(10),
            })
            .unwrap();
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].row_key, "queue-search-conformance");

        let stable_ties = store
            .search_runtime(&RuntimeSearchFilter {
                query: "tie".to_string(),
                row_type: Some(RuntimeSearchRowType::Message),
                session_id: None,
                agent_id: Some(crate::AgentId::new("agent-alpha")),
                instance_id: None,
                task_id: None,
                event_kind: Some(crate::CoreEventKind::AgentMessageRouted),
                recorded_after: None,
                recorded_before: None,
                limit: Some(1),
            })
            .unwrap();
        assert_eq!(stable_ties.len(), 1);
        assert_eq!(stable_ties[0].row_key, "message:2:agent-alpha");

        let empty_query = store
            .search_runtime(&RuntimeSearchFilter {
                query: "   ".to_string(),
                row_type: None,
                session_id: None,
                agent_id: None,
                instance_id: None,
                task_id: None,
                event_kind: None,
                recorded_after: None,
                recorded_before: None,
                limit: None,
            })
            .unwrap_err();
        assert_eq!(empty_query.kind, CoreErrorKind::InvalidInput);
    }

    fn conversation_conformance(store: &dyn ConversationConformanceStore) {
        seed_conversation_base_fixture(store, "session-conversation", "slot-conversation");

        let lazy_slots = store
            .query_message_slots(&MessageSlotQuery {
                session_id: Some(SessionId::new("session-conversation")),
                include_alternates: false,
                page: Some(QueryPage {
                    limit: Some(1),
                    offset: Some(0),
                }),
            })
            .unwrap();
        assert_eq!(lazy_slots.len(), 1);
        assert_eq!(lazy_slots[0].primary.message.body, "root body");
        assert!(lazy_slots[0].alternates.is_empty());

        let eager_slots = store
            .query_message_slots(&MessageSlotQuery {
                session_id: Some(SessionId::new("session-conversation")),
                include_alternates: true,
                page: None,
            })
            .unwrap();
        assert_eq!(eager_slots[0].alternates.len(), 2);
        assert_eq!(eager_slots[0].primary.message.blocks[0].kind, "text");

        let variants = store
            .query_message_variants(&MessageVariantQuery {
                slot_id: Some(MessageSlotId::new("slot-conversation")),
                include_deleted: false,
                page: None,
            })
            .unwrap();
        assert_eq!(
            variants
                .iter()
                .map(|variant| variant.variant_id.0.as_str())
                .collect::<Vec<_>>(),
            vec![
                "variant-conversation-primary",
                "variant-conversation-a",
                "variant-conversation-b"
            ]
        );

        let selected_variant = store
            .select_active_message_variant(&SelectActiveVariantRequest {
                slot_id: MessageSlotId::new("slot-conversation"),
                active_variant_id: Some(MessageVariantId::new("variant-conversation-a")),
                expected: ActiveVariantExpectation::Primary,
                updated_at: "2026-06-26T02:02:00Z".to_string(),
            })
            .unwrap();
        assert!(selected_variant.conflict.is_none());
        assert_eq!(
            selected_variant.slot.active_variant_id,
            Some(MessageVariantId::new("variant-conversation-a"))
        );
        let variant_conflict = store
            .select_active_message_variant(&SelectActiveVariantRequest {
                slot_id: MessageSlotId::new("slot-conversation"),
                active_variant_id: Some(MessageVariantId::new("variant-conversation-b")),
                expected: ActiveVariantExpectation::Primary,
                updated_at: "2026-06-26T02:02:01Z".to_string(),
            })
            .unwrap();
        assert_eq!(
            variant_conflict.conflict.unwrap().actual,
            Some(MessageVariantId::new("variant-conversation-a"))
        );

        let branches = store
            .query_conversation_branches(&ConversationBranchQuery {
                session_id: Some(SessionId::new("session-conversation")),
                parent_branch_id: None,
                page: None,
            })
            .unwrap();
        assert_eq!(
            branches
                .iter()
                .map(|branch| branch.branch_id.0.as_str())
                .collect::<Vec<_>>(),
            vec!["branch-conversation-root", "branch-conversation-child"]
        );
        let paged_branches = store
            .query_conversation_branches(&ConversationBranchQuery {
                session_id: Some(SessionId::new("session-conversation")),
                parent_branch_id: None,
                page: Some(QueryPage {
                    limit: Some(1),
                    offset: Some(1),
                }),
            })
            .unwrap();
        assert_eq!(
            paged_branches[0].branch_id,
            ConversationBranchId::new("branch-conversation-child")
        );

        let default_state = store
            .get_conversation_branch_state(
                &SessionId::new("session-conversation"),
                &"2026-06-26T02:00:00Z".to_string(),
            )
            .unwrap();
        assert_eq!(default_state.active_branch_id, None);
        assert_eq!(default_state.version, 0);

        let selected_branch = store
            .select_active_conversation_branch(&SelectActiveBranchRequest {
                session_id: SessionId::new("session-conversation"),
                active_branch_id: Some(ConversationBranchId::new("branch-conversation-child")),
                expected: ActiveBranchExpectation::None,
                updated_at: "2026-06-26T02:03:00Z".to_string(),
            })
            .unwrap();
        assert!(selected_branch.conflict.is_none());
        assert_eq!(
            selected_branch.state.active_branch_id,
            Some(ConversationBranchId::new("branch-conversation-child"))
        );
        let branch_conflict = store
            .select_active_conversation_branch(&SelectActiveBranchRequest {
                session_id: SessionId::new("session-conversation"),
                active_branch_id: Some(ConversationBranchId::new("branch-conversation-root")),
                expected: ActiveBranchExpectation::None,
                updated_at: "2026-06-26T02:03:01Z".to_string(),
            })
            .unwrap();
        assert_eq!(
            branch_conflict.conflict.unwrap().actual,
            Some(ConversationBranchId::new("branch-conversation-child"))
        );

        let head_conflict = store
            .update_conversation_branch_head(&UpdateBranchHeadRequest {
                branch_id: ConversationBranchId::new("branch-conversation-child"),
                head_message_id: Some(MessageId::new("message-conversation-root")),
                expected: BranchHeadExpectation::None,
                updated_at: "2026-06-26T02:04:00Z".to_string(),
            })
            .unwrap();
        assert_eq!(
            head_conflict.conflict.unwrap().actual,
            Some(MessageId::new("message-conversation-child"))
        );
        let head_updated = store
            .update_conversation_branch_head(&UpdateBranchHeadRequest {
                branch_id: ConversationBranchId::new("branch-conversation-root"),
                head_message_id: Some(MessageId::new("message-conversation-a")),
                expected: BranchHeadExpectation::Message(MessageId::new(
                    "message-conversation-root",
                )),
                updated_at: "2026-06-26T02:04:01Z".to_string(),
            })
            .unwrap();
        assert!(head_updated.conflict.is_none());
        assert_eq!(
            head_updated.branch.head_message_id,
            Some(MessageId::new("message-conversation-a"))
        );

        let snapshot = store
            .save_conversation_snapshot(&ConversationSnapshotWrite {
                snapshot_id: ConversationSnapshotId::new("snapshot-conversation"),
                session_id: SessionId::new("session-conversation"),
                branch_id: Some(ConversationBranchId::new("branch-conversation-child")),
                message_id: Some(MessageId::new("message-conversation-root")),
                cursor: Some("session-conversation:42".to_string()),
                label: Some("Checkpoint".to_string()),
                summary: Some("Conversation checkpoint".to_string()),
                source: ConversationSnapshotSource::User,
                metadata_json: json!({"proof": "conversation"}),
                created_at: "2026-06-26T02:05:00Z".to_string(),
                updated_at: "2026-06-26T02:05:00Z".to_string(),
            })
            .unwrap();
        assert_eq!(
            snapshot.branch_id,
            Some(ConversationBranchId::new("branch-conversation-child"))
        );
        let snapshots = store
            .query_conversation_snapshots(&ConversationSnapshotQuery {
                session_id: Some(SessionId::new("session-conversation")),
                branch_id: None,
                message_id: Some(MessageId::new("message-conversation-root")),
                page: None,
            })
            .unwrap();
        assert_eq!(snapshots.len(), 1);

        let branch_jump = store
            .resolve_conversation_jump(&ConversationJumpRequest {
                session_id: SessionId::new("session-conversation"),
                target: ConversationJumpTarget::Branch {
                    branch_id: ConversationBranchId::new("branch-conversation-child"),
                },
            })
            .unwrap();
        assert_eq!(
            branch_jump.message_id,
            Some(MessageId::new("message-conversation-child"))
        );
        let snapshot_jump = store
            .resolve_conversation_jump(&ConversationJumpRequest {
                session_id: SessionId::new("session-conversation"),
                target: ConversationJumpTarget::Snapshot {
                    snapshot_id: ConversationSnapshotId::new("snapshot-conversation"),
                },
            })
            .unwrap();
        assert_eq!(
            snapshot_jump.cursor,
            Some("session-conversation:42".to_string())
        );
        let message_jump = store
            .resolve_conversation_jump(&ConversationJumpRequest {
                session_id: SessionId::new("session-conversation"),
                target: ConversationJumpTarget::Message {
                    message_id: MessageId::new("message-conversation-root"),
                },
            })
            .unwrap();
        assert_eq!(
            message_jump.branch_id,
            Some(ConversationBranchId::new("branch-conversation-root"))
        );
        let cursor_jump = store
            .resolve_conversation_jump(&ConversationJumpRequest {
                session_id: SessionId::new("session-conversation"),
                target: ConversationJumpTarget::Cursor {
                    cursor: "manual-cursor".to_string(),
                },
            })
            .unwrap();
        assert_eq!(cursor_jump.cursor, Some("manual-cursor".to_string()));
    }

    fn seed_conversation_base_fixture(
        store: &dyn ConversationConformanceStore,
        session_id: &str,
        slot_id: &str,
    ) {
        let session = SessionId::new(session_id);
        let root_branch = ConversationBranchId::new("branch-conversation-root");
        let child_branch = ConversationBranchId::new("branch-conversation-child");
        let slot = MessageSlotId::new(slot_id);
        let primary_variant = MessageVariantId::new("variant-conversation-primary");
        store
            .save_conversation_branch(&ConversationBranchWrite {
                branch_id: root_branch.clone(),
                session_id: session.clone(),
                parent_branch_id: None,
                parent_message_id: None,
                origin_message_id: None,
                head_message_id: Some(MessageId::new("message-conversation-root")),
                label: Some("Root".to_string()),
                metadata_json: json!({"kind": "root"}),
                created_at: "2026-06-26T02:00:00Z".to_string(),
                updated_at: "2026-06-26T02:00:00Z".to_string(),
            })
            .unwrap();
        store
            .save_message_slot(&MessageSlotWrite {
                slot_id: slot.clone(),
                session_id: session.clone(),
                primary_variant_id: primary_variant.clone(),
                active_variant_id: None,
                metadata_json: json!({"slot": "proof"}),
                created_at: "2026-06-26T02:00:00Z".to_string(),
                updated_at: "2026-06-26T02:00:00Z".to_string(),
            })
            .unwrap();
        let mut primary = conversation_variant_write(
            &slot,
            &primary_variant,
            MessageVariantSource::Primary,
            0,
            "message-conversation-root",
            "root body",
        );
        primary.message.session_id = session.clone();
        primary.message.branch_id = Some(root_branch.clone());
        store.save_message_variant(&primary).unwrap();
        let mut alternate_a = conversation_variant_write(
            &slot,
            &MessageVariantId::new("variant-conversation-a"),
            MessageVariantSource::Alternate,
            1,
            "message-conversation-a",
            "alternate a",
        );
        alternate_a.message.session_id = session.clone();
        alternate_a.message.branch_id = Some(root_branch.clone());
        alternate_a.message.parent_message_id = Some(MessageId::new("message-conversation-root"));
        store.save_message_variant(&alternate_a).unwrap();
        let mut alternate_b = conversation_variant_write(
            &slot,
            &MessageVariantId::new("variant-conversation-b"),
            MessageVariantSource::Alternate,
            2,
            "message-conversation-b",
            "alternate b",
        );
        alternate_b.message.session_id = session.clone();
        alternate_b.message.branch_id = Some(root_branch.clone());
        alternate_b.message.parent_message_id = Some(MessageId::new("message-conversation-root"));
        store.save_message_variant(&alternate_b).unwrap();
        store
            .save_conversation_branch(&ConversationBranchWrite {
                branch_id: child_branch,
                session_id: session,
                parent_branch_id: Some(root_branch),
                parent_message_id: Some(MessageId::new("message-conversation-root")),
                origin_message_id: Some(MessageId::new("message-conversation-root")),
                head_message_id: Some(MessageId::new("message-conversation-child")),
                label: Some("Child".to_string()),
                metadata_json: json!({"kind": "child"}),
                created_at: "2026-06-26T02:01:00Z".to_string(),
                updated_at: "2026-06-26T02:01:00Z".to_string(),
            })
            .unwrap();
    }

    fn seed_conversation_conflict_fixture(store: &PostgresRuntimeCounterProofStore) {
        let session = SessionId::new("session-conflict");
        for branch_id in ["branch-conflict-a", "branch-conflict-b"] {
            store
                .save_conversation_branch(&ConversationBranchWrite {
                    branch_id: ConversationBranchId::new(branch_id),
                    session_id: session.clone(),
                    parent_branch_id: None,
                    parent_message_id: None,
                    origin_message_id: None,
                    head_message_id: None,
                    label: Some(branch_id.to_string()),
                    metadata_json: json!({"conflict": true}),
                    created_at: "2026-06-26T03:00:00Z".to_string(),
                    updated_at: "2026-06-26T03:00:00Z".to_string(),
                })
                .unwrap();
        }
        let slot = MessageSlotId::new("slot-conflict");
        let primary_variant = MessageVariantId::new("variant-conflict-primary");
        store
            .save_message_slot(&MessageSlotWrite {
                slot_id: slot.clone(),
                session_id: session.clone(),
                primary_variant_id: primary_variant.clone(),
                active_variant_id: None,
                metadata_json: json!({"conflict": true}),
                created_at: "2026-06-26T03:00:00Z".to_string(),
                updated_at: "2026-06-26T03:00:00Z".to_string(),
            })
            .unwrap();
        let variants = [
            (
                primary_variant,
                MessageVariantSource::Primary,
                0,
                "message-conflict-primary",
                "primary conflict body",
            ),
            (
                MessageVariantId::new("variant-conflict-a"),
                MessageVariantSource::Alternate,
                1,
                "message-conflict-a",
                "alternate conflict a",
            ),
            (
                MessageVariantId::new("variant-conflict-b"),
                MessageVariantSource::Alternate,
                2,
                "message-conflict-b",
                "alternate conflict b",
            ),
        ];
        for (variant_id, source, ordinal, message_id, body) in variants {
            let mut variant =
                conversation_variant_write(&slot, &variant_id, source, ordinal, message_id, body);
            variant.message.session_id = session.clone();
            store.save_message_variant(&variant).unwrap();
        }
    }

    fn conversation_variant_write(
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
                session_id: SessionId::new("session-conversation"),
                branch_id: None,
                parent_message_id: None,
                previous_message_id: None,
                author_id: "agent-proof".to_string(),
                author_role: "assistant".to_string(),
                status: DurableMessageStatus::Completed,
                body: body.to_string(),
                metadata_json: json!({"provider": "proof"}),
                created_at: "2026-06-26T02:00:00Z".to_string(),
                blocks: vec![MessageBlockWrite {
                    block_id: crate::MessageBlockId::new(format!("{message_id}:block-1")),
                    ordinal: 0,
                    kind: "text".to_string(),
                    content_json: json!({"text": body}),
                    render_policy_json: None,
                    metadata_json: json!({}),
                }],
            },
            metadata_json: json!({"variant": variant_id.0}),
            created_at: "2026-06-26T02:00:00Z".to_string(),
            updated_at: "2026-06-26T02:00:00Z".to_string(),
        }
    }

    fn attachment_data_bank_conformance(store: &dyn AttachmentDataBankConformanceStore) {
        let session = SessionId::new("session-attachments");
        let other_session = SessionId::new("session-attachments-other");
        let scope = DataBankScopeId::new("scope-reference");
        let removed_scope = DataBankScopeId::new("scope-removed");
        let message = MessageId::new("message-reference");
        let block = crate::MessageBlockId::new("block-reference");

        store
            .save_data_bank_scope(&DataBankScopeWrite {
                scope_id: scope.clone(),
                session_id: session.clone(),
                status: DataBankScopeStatus::Active,
                label: Some("Reference".to_string()),
                description: Some("Reusable reference files".to_string()),
                metadata_json: json!({"kind": "reference"}),
                created_at: "2026-06-26T04:00:00Z".to_string(),
                updated_at: "2026-06-26T04:00:00Z".to_string(),
            })
            .unwrap();
        store
            .save_data_bank_scope(&DataBankScopeWrite {
                scope_id: removed_scope.clone(),
                session_id: session.clone(),
                status: DataBankScopeStatus::Active,
                label: Some("Removed".to_string()),
                description: None,
                metadata_json: json!({"kind": "temporary"}),
                created_at: "2026-06-26T04:00:01Z".to_string(),
                updated_at: "2026-06-26T04:00:01Z".to_string(),
            })
            .unwrap();

        let saved = store
            .save_attachment(&AttachmentWrite {
                attachment_id: AttachmentId::new("attachment-reference"),
                session_id: session.clone(),
                status: AttachmentStatus::Active,
                filename: "reference.txt".to_string(),
                mime_type: "text/plain".to_string(),
                byte_size: 42,
                storage_url: Some("file:///store/reference.txt".to_string()),
                download_url: Some("/attachments/reference".to_string()),
                thumbnail_url: None,
                extracted_text: Some("bounded reference text".to_string()),
                extracted_text_truncated: true,
                metadata_json: json!({"source": "conformance"}),
                created_at: "2026-06-26T04:01:00Z".to_string(),
                updated_at: "2026-06-26T04:01:00Z".to_string(),
                expires_at: Some("2026-06-26T05:00:00Z".to_string()),
                link: Some(AttachmentLinkWrite {
                    link_id: AttachmentLinkId::new("attachment-link-reference"),
                    attachment_id: AttachmentId::new("attachment-reference"),
                    session_id: session.clone(),
                    message_id: Some(message.clone()),
                    block_id: Some(block.clone()),
                    scope_id: Some(scope.clone()),
                    metadata_json: json!({"linked_by": "conformance"}),
                    created_at: "2026-06-26T04:01:00Z".to_string(),
                }),
            })
            .unwrap();
        assert_eq!(saved.links.len(), 1);
        assert_eq!(saved.links[0].message_id, Some(message.clone()));
        assert_eq!(saved.links[0].block_id, Some(block.clone()));
        assert_eq!(saved.links[0].scope_id, Some(scope.clone()));
        assert!(saved.extracted_text_truncated);

        store
            .save_attachment(&AttachmentWrite {
                attachment_id: AttachmentId::new("attachment-expired"),
                session_id: session.clone(),
                status: AttachmentStatus::Active,
                filename: "expired.txt".to_string(),
                mime_type: "text/plain".to_string(),
                byte_size: 7,
                storage_url: None,
                download_url: None,
                thumbnail_url: None,
                extracted_text: Some("expired".to_string()),
                extracted_text_truncated: false,
                metadata_json: json!({"source": "expired"}),
                created_at: "2026-06-26T04:02:00Z".to_string(),
                updated_at: "2026-06-26T04:02:00Z".to_string(),
                expires_at: Some("2026-06-26T04:30:00Z".to_string()),
                link: Some(AttachmentLinkWrite {
                    link_id: AttachmentLinkId::new("attachment-link-expired"),
                    attachment_id: AttachmentId::new("attachment-expired"),
                    session_id: session.clone(),
                    message_id: None,
                    block_id: None,
                    scope_id: Some(scope.clone()),
                    metadata_json: json!({"linked_by": "expiry"}),
                    created_at: "2026-06-26T04:02:00Z".to_string(),
                }),
            })
            .unwrap();

        store
            .save_attachment(&AttachmentWrite {
                attachment_id: AttachmentId::new("attachment-other-session"),
                session_id: other_session,
                status: AttachmentStatus::Active,
                filename: "other.txt".to_string(),
                mime_type: "text/plain".to_string(),
                byte_size: 3,
                storage_url: None,
                download_url: None,
                thumbnail_url: None,
                extracted_text: None,
                extracted_text_truncated: false,
                metadata_json: json!({}),
                created_at: "2026-06-26T04:03:00Z".to_string(),
                updated_at: "2026-06-26T04:03:00Z".to_string(),
                expires_at: None,
                link: Some(AttachmentLinkWrite {
                    link_id: AttachmentLinkId::new("attachment-link-other-session"),
                    attachment_id: AttachmentId::new("attachment-other-session"),
                    session_id: SessionId::new("session-attachments-other"),
                    message_id: None,
                    block_id: None,
                    scope_id: None,
                    metadata_json: json!({}),
                    created_at: "2026-06-26T04:03:00Z".to_string(),
                }),
            })
            .unwrap();

        let by_message = store
            .query_attachments(&AttachmentQuery {
                session_id: Some(session.clone()),
                message_id: Some(message.clone()),
                now: Some("2026-06-26T04:10:00Z".to_string()),
                ..AttachmentQuery::default()
            })
            .unwrap();
        assert_eq!(by_message.len(), 1);
        assert_eq!(
            by_message[0].attachment_id,
            AttachmentId::new("attachment-reference")
        );

        let by_block = store
            .query_attachments(&AttachmentQuery {
                session_id: Some(session.clone()),
                block_id: Some(block),
                now: Some("2026-06-26T04:10:00Z".to_string()),
                ..AttachmentQuery::default()
            })
            .unwrap();
        assert_eq!(by_block.len(), 1);
        assert_eq!(
            by_block[0].links[0].metadata_json["linked_by"],
            "conformance"
        );

        let by_scope_before_expiry = store
            .query_attachments(&AttachmentQuery {
                session_id: Some(session.clone()),
                scope_id: Some(scope.clone()),
                now: Some("2026-06-26T04:10:00Z".to_string()),
                ..AttachmentQuery::default()
            })
            .unwrap();
        assert_eq!(
            by_scope_before_expiry
                .iter()
                .map(|record| record.attachment_id.0.as_str())
                .collect::<Vec<_>>(),
            vec!["attachment-reference", "attachment-expired"]
        );

        let by_scope_after_expiry = store
            .query_attachments(&AttachmentQuery {
                session_id: Some(session.clone()),
                scope_id: Some(scope.clone()),
                now: Some("2026-06-26T04:31:00Z".to_string()),
                ..AttachmentQuery::default()
            })
            .unwrap();
        assert_eq!(
            by_scope_after_expiry
                .iter()
                .map(|record| record.attachment_id.0.as_str())
                .collect::<Vec<_>>(),
            vec!["attachment-reference"]
        );

        let expired_only = store
            .query_attachments(&AttachmentQuery {
                session_id: Some(session.clone()),
                scope_id: Some(scope.clone()),
                expired_only: true,
                now: Some("2026-06-26T04:31:00Z".to_string()),
                ..AttachmentQuery::default()
            })
            .unwrap();
        assert_eq!(expired_only.len(), 1);
        assert_eq!(
            expired_only[0].attachment_id,
            AttachmentId::new("attachment-expired")
        );

        let include_expired = store
            .query_attachments(&AttachmentQuery {
                session_id: Some(session.clone()),
                scope_id: Some(scope),
                include_expired: true,
                now: Some("2026-06-26T04:31:00Z".to_string()),
                ..AttachmentQuery::default()
            })
            .unwrap();
        assert_eq!(include_expired.len(), 2);

        let removed_attachment = store
            .remove_attachment(
                &AttachmentId::new("attachment-reference"),
                &"2026-06-26T04:40:00Z".to_string(),
            )
            .unwrap();
        assert_eq!(removed_attachment.status, AttachmentStatus::Removed);
        assert_eq!(removed_attachment.links.len(), 1);

        let active_after_remove = store
            .query_attachments(&AttachmentQuery {
                session_id: Some(session.clone()),
                include_expired: true,
                now: Some("2026-06-26T04:41:00Z".to_string()),
                ..AttachmentQuery::default()
            })
            .unwrap();
        assert_eq!(
            active_after_remove
                .iter()
                .map(|record| record.attachment_id.0.as_str())
                .collect::<Vec<_>>(),
            vec!["attachment-expired"]
        );

        let with_removed = store
            .query_attachments(&AttachmentQuery {
                session_id: Some(session.clone()),
                status: Some(AttachmentStatus::Removed),
                include_removed: true,
                include_expired: true,
                now: Some("2026-06-26T04:41:00Z".to_string()),
                ..AttachmentQuery::default()
            })
            .unwrap();
        assert_eq!(with_removed.len(), 1);
        assert_eq!(
            with_removed[0].attachment_id,
            AttachmentId::new("attachment-reference")
        );

        let scopes = store
            .query_data_bank_scopes(&DataBankScopeQuery {
                session_id: Some(session.clone()),
                status: Some(DataBankScopeStatus::Active),
                include_removed: false,
                page: None,
            })
            .unwrap();
        assert_eq!(
            scopes
                .iter()
                .map(|scope| scope.scope_id.0.as_str())
                .collect::<Vec<_>>(),
            vec!["scope-reference", "scope-removed"]
        );
        let removed = store
            .remove_data_bank_scope(&removed_scope, &"2026-06-26T04:45:00Z".to_string())
            .unwrap();
        assert_eq!(removed.status, DataBankScopeStatus::Removed);
        let active_scopes = store
            .query_data_bank_scopes(&DataBankScopeQuery {
                session_id: Some(session.clone()),
                status: Some(DataBankScopeStatus::Active),
                include_removed: false,
                page: None,
            })
            .unwrap();
        assert_eq!(active_scopes.len(), 1);
        assert_eq!(
            active_scopes[0].scope_id,
            DataBankScopeId::new("scope-reference")
        );
        let all_scopes = store
            .query_data_bank_scopes(&DataBankScopeQuery {
                session_id: Some(session),
                status: Some(DataBankScopeStatus::Removed),
                include_removed: true,
                page: None,
            })
            .unwrap();
        assert_eq!(all_scopes.len(), 1);
    }

    fn profile_memory_conformance(store: &dyn ProfileMemoryConformanceStore) {
        let descriptors = store.memory_space_descriptors();
        let descriptor = descriptors
            .iter()
            .find(|descriptor| descriptor.space_id.as_str() == "profile_dense")
            .expect("profile_dense descriptor missing");
        descriptor.validate().unwrap();
        assert_eq!(descriptor.schema_version, 1);
        assert_eq!(
            descriptor.scope_model.allowed_scopes,
            vec![MemoryScopeType::Profile, MemoryScopeType::User]
        );
        assert_eq!(
            descriptor.conflict_policy,
            MemoryConflictPolicy::ExpectedRevision
        );
        assert!(descriptor
            .operations
            .contains(&MemoryOperation::CandidateOnly));

        let profile = ProfileId::new("profile-memory-proof");
        let other_profile = ProfileId::new("profile-memory-other");
        let caps = ProfileMemoryCaps {
            max_records_per_profile: 2,
            max_key_bytes: 32,
            max_content_bytes: 80,
        };
        let target = ProfileMemoryTarget::Profile;
        let user_target = ProfileMemoryTarget::User("user-alpha".to_string());

        let added = store
            .add_profile_memory(
                &ProfileMemoryWrite {
                    profile_id: profile.clone(),
                    target: target.clone(),
                    key: "tone".to_string(),
                    content: "prefers typed repository checks".to_string(),
                    metadata: json!({"source": "conformance", "tags": ["profile_dense"]}),
                    now: "2026-06-26T05:00:00Z".to_string(),
                },
                &caps,
            )
            .unwrap();
        assert_eq!(added.revision, 1);
        assert_eq!(added.target, ProfileMemoryTarget::Profile);
        assert_eq!(added.metadata["tags"][0], "profile_dense");

        let duplicate = store
            .add_profile_memory(
                &ProfileMemoryWrite {
                    profile_id: profile.clone(),
                    target: target.clone(),
                    key: "tone".to_string(),
                    content: "duplicate".to_string(),
                    metadata: json!({}),
                    now: "2026-06-26T05:00:30Z".to_string(),
                },
                &caps,
            )
            .unwrap_err();
        assert_eq!(duplicate.kind, CoreErrorKind::AlreadyExists);

        let replaced = store
            .replace_profile_memory(
                &ProfileMemoryReplace {
                    expected_revision: 1,
                    write: ProfileMemoryWrite {
                        profile_id: profile.clone(),
                        target: target.clone(),
                        key: "tone".to_string(),
                        content: "prefers backend-neutral memory contracts".to_string(),
                        metadata: json!({"source": "replace"}),
                        now: "2026-06-26T05:01:00Z".to_string(),
                    },
                },
                &caps,
            )
            .unwrap();
        assert_eq!(replaced.revision, 2);
        assert_eq!(replaced.created_at, "2026-06-26T05:00:00Z");
        assert_eq!(replaced.updated_at, "2026-06-26T05:01:00Z");

        let stale_replace = store
            .replace_profile_memory(
                &ProfileMemoryReplace {
                    expected_revision: 1,
                    write: ProfileMemoryWrite {
                        profile_id: profile.clone(),
                        target: target.clone(),
                        key: "tone".to_string(),
                        content: "stale".to_string(),
                        metadata: json!({}),
                        now: "2026-06-26T05:02:00Z".to_string(),
                    },
                },
                &caps,
            )
            .unwrap_err();
        assert_eq!(stale_replace.kind, CoreErrorKind::ActionRejected);

        store
            .add_profile_memory(
                &ProfileMemoryWrite {
                    profile_id: profile.clone(),
                    target: user_target.clone(),
                    key: "salutation".to_string(),
                    content: "likes direct status updates".to_string(),
                    metadata: json!({"scope": "user"}),
                    now: "2026-06-26T05:03:00Z".to_string(),
                },
                &caps,
            )
            .unwrap();

        let cap_error = store
            .add_profile_memory(
                &ProfileMemoryWrite {
                    profile_id: profile.clone(),
                    target: ProfileMemoryTarget::Profile,
                    key: "third".to_string(),
                    content: "would exceed cap".to_string(),
                    metadata: json!({}),
                    now: "2026-06-26T05:04:00Z".to_string(),
                },
                &caps,
            )
            .unwrap_err();
        assert_eq!(cap_error.kind, CoreErrorKind::ActionRejected);

        let too_large = store
            .add_profile_memory(
                &ProfileMemoryWrite {
                    profile_id: other_profile.clone(),
                    target: ProfileMemoryTarget::Profile,
                    key: "large".to_string(),
                    content: "x".repeat(81),
                    metadata: json!({}),
                    now: "2026-06-26T05:05:00Z".to_string(),
                },
                &caps,
            )
            .unwrap_err();
        assert_eq!(too_large.kind, CoreErrorKind::ActionRejected);

        store
            .add_profile_memory(
                &ProfileMemoryWrite {
                    profile_id: other_profile.clone(),
                    target: ProfileMemoryTarget::Profile,
                    key: "tone".to_string(),
                    content: "kept separate".to_string(),
                    metadata: json!({}),
                    now: "2026-06-26T05:06:00Z".to_string(),
                },
                &caps,
            )
            .unwrap();

        let all_profile_rows = store
            .list_profile_memory(&ProfileMemoryQuery {
                profile_id: profile.clone(),
                target: None,
                page: None,
            })
            .unwrap();
        assert_eq!(
            all_profile_rows
                .iter()
                .map(|record| record.key.as_str())
                .collect::<Vec<_>>(),
            vec!["salutation", "tone"]
        );
        assert!(all_profile_rows
            .iter()
            .all(|record| record.profile_id == profile));

        let profile_rows = store
            .list_profile_memory(&ProfileMemoryQuery {
                profile_id: profile.clone(),
                target: Some(ProfileMemoryTarget::Profile),
                page: None,
            })
            .unwrap();
        assert_eq!(profile_rows.len(), 1);
        assert_eq!(profile_rows[0].key, "tone");

        let user_rows = store
            .list_profile_memory(&ProfileMemoryQuery {
                profile_id: profile.clone(),
                target: Some(user_target.clone()),
                page: Some(QueryPage {
                    limit: Some(1),
                    offset: Some(0),
                }),
            })
            .unwrap();
        assert_eq!(user_rows.len(), 1);
        assert_eq!(user_rows[0].target, user_target);

        let fetched = store
            .get_profile_memory(&profile, &target, "tone")
            .unwrap()
            .unwrap();
        assert_eq!(fetched.content, "prefers backend-neutral memory contracts");
        assert_eq!(fetched.revision, 2);

        let stale_delete = store
            .remove_profile_memory(&ProfileMemoryDelete {
                profile_id: profile.clone(),
                target: target.clone(),
                key: "tone".to_string(),
                expected_revision: 1,
            })
            .unwrap_err();
        assert_eq!(stale_delete.kind, CoreErrorKind::ActionRejected);

        let removed = store
            .remove_profile_memory(&ProfileMemoryDelete {
                profile_id: profile.clone(),
                target,
                key: "tone".to_string(),
                expected_revision: 2,
            })
            .unwrap();
        assert_eq!(removed.key, "tone");
        assert!(store
            .get_profile_memory(&profile, &ProfileMemoryTarget::Profile, "tone")
            .unwrap()
            .is_none());

        assert_eq!(
            store
                .list_profile_memory(&ProfileMemoryQuery {
                    profile_id: profile,
                    target: None,
                    page: None,
                })
                .unwrap()
                .len(),
            1
        );
    }

    fn roleplay_lore_conformance(store: &dyn RoleplayLoreConformanceStore) {
        let descriptors = store.memory_space_descriptors();
        let descriptor = descriptors
            .iter()
            .find(|descriptor| descriptor.space_id.as_str() == "roleplay_lore")
            .expect("roleplay_lore descriptor missing");
        descriptor.validate().unwrap();
        assert_eq!(descriptor.module_id.as_deref(), Some("roleplay_lore"));
        assert_eq!(descriptor.scope_model.primary_scope, MemoryScopeType::World);
        assert!(descriptor
            .record_shapes
            .iter()
            .any(|shape| shape.shape_id.as_str() == "world"));
        assert!(descriptor
            .record_shapes
            .iter()
            .any(|shape| shape.shape_id.as_str() == "entity"));
        assert!(descriptor
            .record_shapes
            .iter()
            .any(|shape| shape.shape_id.as_str() == "lore_entry"));
        assert!(descriptor
            .record_shapes
            .iter()
            .any(|shape| shape.shape_id.as_str() == "relationship"));
        assert!(descriptor
            .record_shapes
            .iter()
            .any(|shape| shape.shape_id.as_str() == "timeline_event"));

        let world = roleplay_lore_write(RoleplayLoreWriteFixture {
            record_id: "world-moonlit",
            world_id: "world-moonlit",
            entity_id: None,
            shape_id: "world",
            title: "Moonlit Expanse",
            body: "A foggy port world with a secret tide calendar.",
            canon_status: RoleplayLoreCanonStatus::Canon,
            visibility: RoleplayLoreVisibility::Public,
            evidence_ref_id: "wake-world",
            now: "2026-06-26T07:00:00Z",
            supersedes_record_id: None,
        });
        let world = store.add_roleplay_lore_record(&world).unwrap();
        assert_eq!(world.revision, 1);
        assert_eq!(world.shape.shape_id.as_str(), "world");

        let entity = store
            .add_roleplay_lore_record(&roleplay_lore_write(RoleplayLoreWriteFixture {
                record_id: "entity-clockmaker",
                world_id: "world-moonlit",
                entity_id: Some("entity-clockmaker"),
                shape_id: "entity",
                title: "The Clockmaker",
                body: "A public canon entity who repairs brass moons.",
                canon_status: RoleplayLoreCanonStatus::Canon,
                visibility: RoleplayLoreVisibility::Public,
                evidence_ref_id: "wake-entity",
                now: "2026-06-26T07:01:00Z",
                supersedes_record_id: None,
            }))
            .unwrap();
        assert_eq!(entity.entity_id.as_deref(), Some("entity-clockmaker"));

        let lore = store
            .add_roleplay_lore_record(&roleplay_lore_write(RoleplayLoreWriteFixture {
                record_id: "lore-tide-calendar",
                world_id: "world-moonlit",
                entity_id: Some("entity-clockmaker"),
                shape_id: "lore_entry",
                title: "Tide Calendar",
                body: "The tide calendar needle unlocks the observatory door.",
                canon_status: RoleplayLoreCanonStatus::Canon,
                visibility: RoleplayLoreVisibility::Public,
                evidence_ref_id: "wake-lore",
                now: "2026-06-26T07:02:00Z",
                supersedes_record_id: None,
            }))
            .unwrap();
        assert_eq!(lore.revision, 1);

        store
            .add_roleplay_lore_record(&roleplay_lore_write(RoleplayLoreWriteFixture {
                record_id: "relationship-apprentice",
                world_id: "world-moonlit",
                entity_id: Some("entity-clockmaker"),
                shape_id: "relationship",
                title: "Apprentice Bond",
                body: "The Clockmaker protects a private apprentice.",
                canon_status: RoleplayLoreCanonStatus::Draft,
                visibility: RoleplayLoreVisibility::Private,
                evidence_ref_id: "wake-relationship",
                now: "2026-06-26T07:03:00Z",
                supersedes_record_id: None,
            }))
            .unwrap();

        let world_canon = store
            .query_roleplay_lore_records(&RoleplayLoreQuery {
                world_id: Some("world-moonlit".to_string()),
                canon_status: Some(RoleplayLoreCanonStatus::Canon),
                visibility: Some(RoleplayLoreVisibility::Public),
                page: Some(QueryPage {
                    limit: Some(10),
                    offset: Some(0),
                }),
                ..RoleplayLoreQuery::default()
            })
            .unwrap();
        assert_eq!(
            world_canon
                .iter()
                .map(|record| record.record_id.as_str())
                .collect::<Vec<_>>(),
            vec!["lore-tide-calendar", "entity-clockmaker", "world-moonlit"]
        );

        let entity_lore = store
            .query_roleplay_lore_records(&RoleplayLoreQuery {
                world_id: Some("world-moonlit".to_string()),
                entity_id: Some("entity-clockmaker".to_string()),
                shape_id: Some("lore_entry".to_string()),
                ..RoleplayLoreQuery::default()
            })
            .unwrap();
        assert_eq!(entity_lore.len(), 1);
        assert_eq!(entity_lore[0].record_id, "lore-tide-calendar");

        let searched = store
            .query_roleplay_lore_records(&RoleplayLoreQuery {
                query: Some("observatory".to_string()),
                ..RoleplayLoreQuery::default()
            })
            .unwrap();
        assert_eq!(searched.len(), 1);
        assert_eq!(searched[0].record_id, "lore-tide-calendar");

        let provenance = store
            .roleplay_lore_provenance_events("lore-tide-calendar")
            .unwrap();
        assert_eq!(provenance.len(), 1);
        assert_eq!(provenance[0].evidence_refs[0].ref_id, "wake-lore");
        let by_provenance = store
            .query_roleplay_lore_records(&RoleplayLoreQuery {
                provenance_ref_id: Some("wake-lore".to_string()),
                ..RoleplayLoreQuery::default()
            })
            .unwrap();
        assert_eq!(by_provenance.len(), 1);
        assert_eq!(by_provenance[0].record_id, "lore-tide-calendar");

        let stale_replace = store
            .replace_roleplay_lore_record(&RoleplayLoreReplace {
                expected_revision: 2,
                write: roleplay_lore_write(RoleplayLoreWriteFixture {
                    record_id: "lore-tide-calendar",
                    world_id: "world-moonlit",
                    entity_id: Some("entity-clockmaker"),
                    shape_id: "lore_entry",
                    title: "Tide Calendar",
                    body: "stale",
                    canon_status: RoleplayLoreCanonStatus::Canon,
                    visibility: RoleplayLoreVisibility::Public,
                    evidence_ref_id: "wake-stale",
                    now: "2026-06-26T07:04:00Z",
                    supersedes_record_id: None,
                }),
            })
            .unwrap_err();
        assert_eq!(stale_replace.kind, CoreErrorKind::ActionRejected);

        let replaced = store
            .replace_roleplay_lore_record(&RoleplayLoreReplace {
                expected_revision: 1,
                write: roleplay_lore_write(RoleplayLoreWriteFixture {
                    record_id: "lore-tide-calendar",
                    world_id: "world-moonlit",
                    entity_id: Some("entity-clockmaker"),
                    shape_id: "lore_entry",
                    title: "Tide Calendar",
                    body: "The tide calendar needle unlocks the observatory and the moon gate.",
                    canon_status: RoleplayLoreCanonStatus::Canon,
                    visibility: RoleplayLoreVisibility::Public,
                    evidence_ref_id: "wake-replace",
                    now: "2026-06-26T07:04:00Z",
                    supersedes_record_id: None,
                }),
            })
            .unwrap();
        assert_eq!(replaced.revision, 2);
        assert!(replaced.body.contains("moon gate"));

        let (old_lore, new_lore) = store
            .supersede_roleplay_lore_record(&RoleplayLoreSupersede {
                record_id: "lore-tide-calendar".to_string(),
                expected_revision: 2,
                replacement: roleplay_lore_write(RoleplayLoreWriteFixture {
                    record_id: "lore-tide-calendar-v2",
                    world_id: "world-moonlit",
                    entity_id: Some("entity-clockmaker"),
                    shape_id: "lore_entry",
                    title: "Tide Calendar Revised",
                    body: "The revised calendar opens the moon gate only during eclipse tide.",
                    canon_status: RoleplayLoreCanonStatus::Canon,
                    visibility: RoleplayLoreVisibility::Public,
                    evidence_ref_id: "wake-supersede",
                    now: "2026-06-26T07:05:00Z",
                    supersedes_record_id: Some("lore-tide-calendar"),
                }),
            })
            .unwrap();
        assert_eq!(old_lore.status, RoleplayLoreRecordStatus::Superseded);
        assert_eq!(
            old_lore.superseded_by_record_id.as_deref(),
            Some("lore-tide-calendar-v2")
        );
        assert_eq!(
            new_lore.supersedes_record_id.as_deref(),
            Some("lore-tide-calendar")
        );

        let active_after_supersede = store
            .query_roleplay_lore_records(&RoleplayLoreQuery {
                query: Some("eclipse".to_string()),
                ..RoleplayLoreQuery::default()
            })
            .unwrap();
        assert_eq!(active_after_supersede.len(), 1);
        assert_eq!(active_after_supersede[0].record_id, "lore-tide-calendar-v2");
        let with_superseded = store
            .query_roleplay_lore_records(&RoleplayLoreQuery {
                query: Some("moon gate".to_string()),
                include_superseded: true,
                ..RoleplayLoreQuery::default()
            })
            .unwrap();
        assert_eq!(with_superseded.len(), 2);

        let tombstoned = store
            .tombstone_roleplay_lore_record(&RoleplayLoreTombstone {
                record_id: "lore-tide-calendar-v2".to_string(),
                expected_revision: 1,
                reason: Some("merged into world bible".to_string()),
                now: "2026-06-26T07:06:00Z".to_string(),
            })
            .unwrap();
        assert_eq!(tombstoned.status, RoleplayLoreRecordStatus::Tombstoned);
        assert_eq!(tombstoned.revision, 2);

        let active_after_tombstone = store
            .query_roleplay_lore_records(&RoleplayLoreQuery {
                query: Some("eclipse".to_string()),
                ..RoleplayLoreQuery::default()
            })
            .unwrap();
        assert!(active_after_tombstone.is_empty());
        let with_tombstoned = store
            .query_roleplay_lore_records(&RoleplayLoreQuery {
                query: Some("moon gate".to_string()),
                include_superseded: true,
                include_tombstoned: true,
                ..RoleplayLoreQuery::default()
            })
            .unwrap();
        assert_eq!(with_tombstoned.len(), 2);
    }

    fn provider_wire_state_conformance(store: &dyn ProviderWireStateConformanceStore) {
        let key = provider_wire_state_key("session-alpha", "openai-responses", "replay");
        let first = store
            .save_provider_wire_state(&provider_wire_state_write(ProviderWireStateWriteFixture {
                key: key.clone(),
                profile_fingerprint: "profile:v1",
                provider_fingerprint: "provider:v1",
                payload_version: "responses:v1",
                payload_json: json!({"response_id": "resp_1"}),
                now: "2026-06-26T00:00:00Z",
                expires_at: Some("2026-06-26T01:00:00Z"),
                last_wake_id: Some("wake-1"),
            }))
            .unwrap();
        assert!(first.is_current());
        assert_eq!(first.payload_encoding, "json");

        let second = store
            .save_provider_wire_state(&provider_wire_state_write(ProviderWireStateWriteFixture {
                key: key.clone(),
                profile_fingerprint: "profile:v1",
                provider_fingerprint: "provider:v1",
                payload_version: "responses:v2",
                payload_json: json!({"response_id": "resp_2", "large": "x".repeat(128)}),
                now: "2026-06-26T00:01:00Z",
                expires_at: Some("2026-06-26T01:01:00Z"),
                last_wake_id: Some("wake-2"),
            }))
            .unwrap();
        assert!(second.row_id > first.row_id);

        let current = store
            .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
                key: key.clone(),
                profile_fingerprint: "profile:v1".to_string(),
                provider_fingerprint: "provider:v1".to_string(),
                now: "2026-06-26T00:02:00Z".to_string(),
            })
            .unwrap();
        let record = current.record.unwrap();
        assert_eq!(current.absence_reason, None);
        assert_eq!(record.payload_version, "responses:v2");
        assert_eq!(record.payload_json["response_id"], "resp_2");
        assert_eq!(record.last_wake_id.as_deref(), Some("wake-2"));

        let diagnostics = store.list_provider_wire_state_diagnostics(10).unwrap();
        assert_eq!(diagnostics.len(), 2);
        assert_eq!(diagnostics[0].key, key);
        assert_eq!(diagnostics[0].payload_version, "responses:v2");
        assert!(diagnostics[0].payload_bytes > 128);
        assert_eq!(
            diagnostics[1].invalidation_reason.as_deref(),
            Some("superseded")
        );

        let expired_key = provider_wire_state_key("session-expired", "openai-responses", "replay");
        store
            .save_provider_wire_state(&provider_wire_state_write(ProviderWireStateWriteFixture {
                key: expired_key.clone(),
                profile_fingerprint: "profile:v1",
                provider_fingerprint: "provider:v1",
                payload_version: "responses:v1",
                payload_json: json!({"response_id": "expired"}),
                now: "2026-06-26T00:03:00Z",
                expires_at: Some("2026-06-26T00:04:00Z"),
                last_wake_id: Some("wake-expired"),
            }))
            .unwrap();
        let expired = store
            .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
                key: expired_key.clone(),
                profile_fingerprint: "profile:v1".to_string(),
                provider_fingerprint: "provider:v1".to_string(),
                now: "2026-06-26T00:04:00Z".to_string(),
            })
            .unwrap();
        assert_eq!(expired.record, None);
        assert_eq!(
            expired.absence_reason,
            Some(ProviderStateAbsenceReason::Expired)
        );

        let profile_key = provider_wire_state_key("session-profile", "openai-responses", "replay");
        store
            .save_provider_wire_state(&provider_wire_state_write(ProviderWireStateWriteFixture {
                key: profile_key.clone(),
                profile_fingerprint: "profile:v1",
                provider_fingerprint: "provider:v1",
                payload_version: "responses:v1",
                payload_json: json!({"response_id": "profile_stale"}),
                now: "2026-06-26T00:05:00Z",
                expires_at: Some("2026-06-26T01:00:00Z"),
                last_wake_id: None,
            }))
            .unwrap();
        let profile_stale = store
            .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
                key: profile_key,
                profile_fingerprint: "profile:v2".to_string(),
                provider_fingerprint: "provider:v1".to_string(),
                now: "2026-06-26T00:06:00Z".to_string(),
            })
            .unwrap();
        assert_eq!(profile_stale.record, None);
        assert_eq!(
            profile_stale.absence_reason,
            Some(ProviderStateAbsenceReason::Invalidated)
        );

        let provider_key =
            provider_wire_state_key("session-provider", "openai-responses", "replay");
        store
            .save_provider_wire_state(&provider_wire_state_write(ProviderWireStateWriteFixture {
                key: provider_key.clone(),
                profile_fingerprint: "profile:v1",
                provider_fingerprint: "provider:v1",
                payload_version: "responses:v1",
                payload_json: json!({"response_id": "provider_stale"}),
                now: "2026-06-26T00:07:00Z",
                expires_at: Some("2026-06-26T01:00:00Z"),
                last_wake_id: None,
            }))
            .unwrap();
        let provider_stale = store
            .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
                key: provider_key,
                profile_fingerprint: "profile:v1".to_string(),
                provider_fingerprint: "provider:v2".to_string(),
                now: "2026-06-26T00:08:00Z".to_string(),
            })
            .unwrap();
        assert_eq!(provider_stale.record, None);
        assert_eq!(
            provider_stale.absence_reason,
            Some(ProviderStateAbsenceReason::Invalidated)
        );

        let clear_key = provider_wire_state_key("session-clear", "openai-responses", "replay");
        store
            .save_provider_wire_state(&provider_wire_state_write(ProviderWireStateWriteFixture {
                key: clear_key.clone(),
                profile_fingerprint: "profile:v1",
                provider_fingerprint: "provider:v1",
                payload_version: "responses:v1",
                payload_json: json!({"response_id": "clear"}),
                now: "2026-06-26T00:09:00Z",
                expires_at: Some("2026-06-26T01:00:00Z"),
                last_wake_id: None,
            }))
            .unwrap();
        let cleared = store
            .clear_provider_wire_state(
                &clear_key,
                &"2026-06-26T00:10:00Z".to_string(),
                ProviderWireStateInvalidationReason::BrainRequestedClear,
            )
            .unwrap()
            .unwrap();
        assert_eq!(
            cleared.invalidation_reason,
            Some(ProviderWireStateInvalidationReason::BrainRequestedClear)
        );
        assert_eq!(
            store
                .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
                    key: clear_key,
                    profile_fingerprint: "profile:v1".to_string(),
                    provider_fingerprint: "provider:v1".to_string(),
                    now: "2026-06-26T00:11:00Z".to_string(),
                })
                .unwrap()
                .absence_reason,
            Some(ProviderStateAbsenceReason::Missing)
        );

        let strategy_key =
            provider_wire_state_key("session-strategy", "openai-responses", "replay-v1");
        store
            .save_provider_wire_state(&provider_wire_state_write(ProviderWireStateWriteFixture {
                key: strategy_key.clone(),
                profile_fingerprint: "profile:v1",
                provider_fingerprint: "provider:v1",
                payload_version: "responses:v1",
                payload_json: json!({"response_id": "strategy"}),
                now: "2026-06-26T00:12:00Z",
                expires_at: Some("2026-06-26T01:00:00Z"),
                last_wake_id: None,
            }))
            .unwrap();
        let changed_strategy =
            provider_wire_state_key("session-strategy", "openai-responses", "replay-v2");
        assert_eq!(
            store
                .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
                    key: changed_strategy,
                    profile_fingerprint: "profile:v1".to_string(),
                    provider_fingerprint: "provider:v1".to_string(),
                    now: "2026-06-26T00:13:00Z".to_string(),
                })
                .unwrap()
                .absence_reason,
            Some(ProviderStateAbsenceReason::Missing)
        );
        assert!(store
            .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
                key: strategy_key,
                profile_fingerprint: "profile:v1".to_string(),
                provider_fingerprint: "provider:v1".to_string(),
                now: "2026-06-26T00:14:00Z".to_string(),
            })
            .unwrap()
            .record
            .is_none());

        let module_key = provider_wire_state_key("session-module", "openai-responses", "replay");
        store
            .save_provider_wire_state(&provider_wire_state_write(ProviderWireStateWriteFixture {
                key: module_key.clone(),
                profile_fingerprint: "profile:v1",
                provider_fingerprint: "provider:v1",
                payload_version: "responses:v1",
                payload_json: json!({"response_id": "module"}),
                now: "2026-06-26T00:15:00Z",
                expires_at: Some("2026-06-26T01:00:00Z"),
                last_wake_id: None,
            }))
            .unwrap();
        let changed_module =
            provider_wire_state_key("session-module", "anthropic-messages", "replay");
        assert_eq!(
            store
                .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
                    key: changed_module,
                    profile_fingerprint: "profile:v1".to_string(),
                    provider_fingerprint: "provider:v1".to_string(),
                    now: "2026-06-26T00:16:00Z".to_string(),
                })
                .unwrap()
                .absence_reason,
            Some(ProviderStateAbsenceReason::Missing)
        );
        assert!(store
            .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
                key: module_key,
                profile_fingerprint: "profile:v1".to_string(),
                provider_fingerprint: "provider:v1".to_string(),
                now: "2026-06-26T00:17:00Z".to_string(),
            })
            .unwrap()
            .record
            .is_none());

        let maintenance_key =
            provider_wire_state_key("session-maintenance", "openai-responses", "replay");
        store
            .save_provider_wire_state(&provider_wire_state_write(ProviderWireStateWriteFixture {
                key: maintenance_key,
                profile_fingerprint: "profile:v1",
                provider_fingerprint: "provider:v1",
                payload_version: "responses:v1",
                payload_json: json!({"response_id": "maintenance"}),
                now: "2026-06-26T00:18:00Z",
                expires_at: Some("2026-06-26T00:19:00Z"),
                last_wake_id: Some("wake-maintenance"),
            }))
            .unwrap();
        assert!(store
            .expire_provider_wire_states_at(&"2026-06-26T00:18:30Z".to_string())
            .unwrap()
            .is_empty());
        let expired = store
            .expire_provider_wire_states_at(&"2026-06-26T00:19:00Z".to_string())
            .unwrap();
        assert_eq!(expired.len(), 1);
        assert_eq!(
            expired[0].invalidation_reason,
            Some(ProviderWireStateInvalidationReason::Expired)
        );
    }

    fn provider_wire_state_key(
        session_id: &str,
        module_id: &str,
        strategy_id: &str,
    ) -> ProviderWireStateKey {
        ProviderWireStateKey {
            session_id: crate::SessionId::new(session_id),
            module_id: module_id.to_string(),
            strategy_id: strategy_id.to_string(),
        }
    }

    struct RoleplayLoreWriteFixture<'a> {
        record_id: &'a str,
        world_id: &'a str,
        entity_id: Option<&'a str>,
        shape_id: &'a str,
        title: &'a str,
        body: &'a str,
        canon_status: RoleplayLoreCanonStatus,
        visibility: RoleplayLoreVisibility,
        evidence_ref_id: &'a str,
        now: &'a str,
        supersedes_record_id: Option<&'a str>,
    }

    fn roleplay_lore_write(input: RoleplayLoreWriteFixture<'_>) -> RoleplayLoreWrite {
        RoleplayLoreWrite {
            record_id: input.record_id.to_string(),
            world_id: input.world_id.to_string(),
            entity_id: input.entity_id.map(ToString::to_string),
            session_id: Some(SessionId::new("session-roleplay")),
            branch_id: Some(ConversationBranchId::new("branch-roleplay-main")),
            shape: MemoryRecordShapeRef {
                shape_id: MemoryRecordShapeId::unchecked(input.shape_id),
                version: 1,
            },
            canon_status: input.canon_status,
            visibility: input.visibility,
            title: input.title.to_string(),
            body: input.body.to_string(),
            content: json!({
                "record_id": input.record_id,
                "world_id": input.world_id,
                "entity_id": input.entity_id,
                "title": input.title,
                "body": input.body,
                "canon_status": roleplay_lore_canon_status_as_str(input.canon_status),
                "visibility": roleplay_lore_visibility_as_str(input.visibility),
                "relationship_kind": "protects",
                "target_entity_id": "entity-apprentice",
                "event_time": "third moon",
                "metadata_json": {"fixture": "roleplay_lore"}
            }),
            evidence_refs: vec![MemoryEvidenceRef {
                evidence_type: MemoryEvidenceKind::Wake,
                ref_id: input.evidence_ref_id.to_string(),
                label: Some("roleplay lore evidence".to_string()),
            }],
            source: MemoryProposalSource::Human,
            confidence: 0.91,
            durability_rationale: "roleplay lore conformance fact should survive restarts"
                .to_string(),
            supersedes_record_id: input.supersedes_record_id.map(ToString::to_string),
            now: input.now.to_string(),
        }
    }

    struct ProviderWireStateWriteFixture<'a> {
        key: ProviderWireStateKey,
        profile_fingerprint: &'a str,
        provider_fingerprint: &'a str,
        payload_version: &'a str,
        payload_json: serde_json::Value,
        now: &'a str,
        expires_at: Option<&'a str>,
        last_wake_id: Option<&'a str>,
    }

    fn provider_wire_state_write(
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

    fn runtime_search_fixture() -> Vec<RuntimeSearchResult> {
        vec![
            RuntimeSearchResult {
                row_type: RuntimeSearchRowType::Session,
                row_key: "session-alpha".to_string(),
                sequence: None,
                session_id: Some(crate::SessionId::new("session-alpha")),
                agent_id: Some(crate::AgentId::new("agent-alpha")),
                instance_id: Some(crate::AgentInstanceId::new("instance:session-alpha")),
                task_id: Some(crate::TaskId::new("task-search")),
                event_kind: Some(crate::CoreEventKind::SessionCreated),
                recorded_at: "2026-06-26T00:00:00Z".to_string(),
                title: "session session-alpha".to_string(),
                body: "agent agent-alpha profile runner kind full tools shell den".to_string(),
            },
            RuntimeSearchResult {
                row_type: RuntimeSearchRowType::Message,
                row_key: "message:1:agent-beta".to_string(),
                sequence: Some(1),
                session_id: None,
                agent_id: Some(crate::AgentId::new("agent-beta")),
                instance_id: None,
                task_id: None,
                event_kind: Some(crate::CoreEventKind::AgentMessageRouted),
                recorded_at: "2026-06-26T00:01:00Z".to_string(),
                title: "agent message".to_string(),
                body: "needle event search beta".to_string(),
            },
            RuntimeSearchResult {
                row_type: RuntimeSearchRowType::QueueMessage,
                row_key: "queue-search-conformance".to_string(),
                sequence: Some(1),
                session_id: Some(crate::SessionId::new("session-alpha")),
                agent_id: Some(crate::AgentId::new("agent-alpha")),
                instance_id: Some(crate::AgentInstanceId::new("instance:session-alpha")),
                task_id: None,
                event_kind: Some(crate::CoreEventKind::AgentMessageRouted),
                recorded_at: "2026-06-26T00:02:00Z".to_string(),
                title: "queued message pending".to_string(),
                body: "needle queue search".to_string(),
            },
            RuntimeSearchResult {
                row_type: RuntimeSearchRowType::Message,
                row_key: "message:2:agent-alpha".to_string(),
                sequence: Some(2),
                session_id: None,
                agent_id: Some(crate::AgentId::new("agent-alpha")),
                instance_id: None,
                task_id: None,
                event_kind: Some(crate::CoreEventKind::AgentMessageRouted),
                recorded_at: "2026-06-26T00:03:00Z".to_string(),
                title: "agent message".to_string(),
                body: "tie search alpha".to_string(),
            },
            RuntimeSearchResult {
                row_type: RuntimeSearchRowType::Message,
                row_key: "message:3:agent-alpha".to_string(),
                sequence: Some(3),
                session_id: None,
                agent_id: Some(crate::AgentId::new("agent-alpha")),
                instance_id: None,
                task_id: None,
                event_kind: Some(crate::CoreEventKind::AgentMessageRouted),
                recorded_at: "2026-06-26T00:04:00Z".to_string(),
                title: "agent message".to_string(),
                body: "tie search alpha".to_string(),
            },
        ]
    }

    fn postgres_test_database_url() -> Option<String> {
        for key in [
            "RUSTY_CREW_POSTGRES_PROOF_DATABASE_URL",
            "RUSTY_CREW_DATABASE_URL",
            "RUSTY_CREW_APP_DATABASE_URL",
        ] {
            if let Ok(value) = std::env::var(key) {
                if !value.trim().is_empty() {
                    return Some(value);
                }
            }
        }
        None
    }

    fn unique_schema(prefix: &str) -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after unix epoch")
            .as_nanos();
        format!("{prefix}_{}_{}", std::process::id(), nanos)
    }

    fn temp_sqlite_path(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("rusty_crew_{label}_{nanos}.sqlite3"))
    }
}
