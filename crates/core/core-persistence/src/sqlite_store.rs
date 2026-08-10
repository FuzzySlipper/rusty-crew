//! SQLite store facade, runtime-admin API, and coordination-store construction.
//!
//! This module keeps the public `CoreCoordinationStore` and `CoordinationStore`
//! facade implementations out of the crate entrypoint. Domain SQL helpers remain
//! in their existing modules or in `lib.rs` until the repository-domain split.

use super::*;

impl CoreCoordinationStore {
    pub fn open_storage(
        engine_data_dir: impl AsRef<Path>,
        storage: Option<&EngineStorageConfig>,
    ) -> CoreResult<Self> {
        match storage {
            None => Self::open_sqlite(engine_data_dir),
            Some(EngineStorageConfig::Sqlite {
                filesystem_warning_free_percent,
            }) => Ok(Self::Sqlite(CoordinationStore::open_with_diagnostics(
                engine_data_dir,
                *filesystem_warning_free_percent,
            )?)),
            Some(EngineStorageConfig::Postgres {
                database_url,
                schema,
                max_connections,
                backing_filesystem_path,
                filesystem_warning_free_percent,
                ..
            }) => Self::open_postgres_with_storage_options(
                database_url,
                schema,
                *max_connections,
                backing_filesystem_path.clone(),
                *filesystem_warning_free_percent,
            ),
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

    #[cfg(feature = "postgres")]
    pub fn open_postgres_with_storage_options(
        database_url: &str,
        schema: &str,
        max_connections: Option<u32>,
        backing_filesystem_path: Option<String>,
        filesystem_warning_free_percent: Option<u32>,
    ) -> CoreResult<Self> {
        Ok(Self::Postgres(Arc::new(
            postgres_backend::PostgresBackendStore::connect_with_storage_options(
                database_url,
                schema,
                max_connections,
                backing_filesystem_path,
                filesystem_warning_free_percent,
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

    #[cfg(not(feature = "postgres"))]
    pub fn open_postgres_with_storage_options(
        _database_url: &str,
        _schema: &str,
        _max_connections: Option<u32>,
        _backing_filesystem_path: Option<String>,
        _filesystem_warning_free_percent: Option<u32>,
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

    pub fn chat_events(&self) -> ChatEventRepositorySet<'_> {
        ChatEventRepositorySet { store: self }
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

    pub fn load_session_configs(&self) -> CoreResult<Vec<SessionConfigRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.load_session_configs(),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.load_session_configs(),
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

    pub fn get_runtime_activity(
        &self,
        activity_id: &RuntimeActivityId,
    ) -> CoreResult<Option<RuntimeActivityRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.get_runtime_activity(activity_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.get_runtime_activity(activity_id),
        }
    }

    pub fn get_review_submission(
        &self,
        submission_id: &str,
    ) -> CoreResult<Option<ReviewSubmissionRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.get_review_submission(submission_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.get_review_submission(submission_id),
        }
    }

    pub fn list_review_submissions(&self) -> CoreResult<Vec<ReviewSubmissionRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.list_review_submissions(),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.list_review_submissions(),
        }
    }

    pub fn insert_review_submission(
        &self,
        record: &ReviewSubmissionRecord,
    ) -> CoreResult<ReviewSubmissionRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.insert_review_submission(record),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.insert_review_submission(record),
        }
    }

    pub fn update_review_submission(
        &self,
        record: &ReviewSubmissionRecord,
        expected_revision: u64,
    ) -> CoreResult<ReviewSubmissionRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.update_review_submission(record, expected_revision),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => {
                postgres.update_review_submission(record, expected_revision)
            }
        }
    }

    pub fn get_install_diplomat_binding(
        &self,
        binding_id: &str,
    ) -> CoreResult<Option<InstallDiplomatBindingRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.get_install_diplomat_binding(binding_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.get_install_diplomat_binding(binding_id),
        }
    }

    pub fn list_install_diplomat_bindings(
        &self,
        query: &InstallDiplomatBindingQuery,
    ) -> CoreResult<Vec<InstallDiplomatBindingRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.list_install_diplomat_bindings(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.list_install_diplomat_bindings(query),
        }
    }

    pub fn insert_install_diplomat_binding(
        &self,
        record: &InstallDiplomatBindingRecord,
    ) -> CoreResult<InstallDiplomatBindingRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.insert_install_diplomat_binding(record),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.insert_install_diplomat_binding(record),
        }
    }

    pub fn update_install_diplomat_binding(
        &self,
        record: &InstallDiplomatBindingRecord,
        expected_revision: u64,
    ) -> CoreResult<InstallDiplomatBindingRecord> {
        match self {
            Self::Sqlite(sqlite) => {
                sqlite.update_install_diplomat_binding(record, expected_revision)
            }
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => {
                postgres.update_install_diplomat_binding(record, expected_revision)
            }
        }
    }

    pub fn get_telegram_diplomat_interaction(
        &self,
        interaction_id: &str,
    ) -> CoreResult<Option<TelegramDiplomatInteractionRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.get_telegram_diplomat_interaction(interaction_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.get_telegram_diplomat_interaction(interaction_id),
        }
    }

    pub fn put_telegram_diplomat_interaction(
        &self,
        record: &TelegramDiplomatInteractionRecord,
        expected_revision: Option<u64>,
    ) -> CoreResult<TelegramDiplomatInteractionRecord> {
        match self {
            Self::Sqlite(sqlite) => {
                sqlite.put_telegram_diplomat_interaction(record, expected_revision)
            }
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => {
                postgres.put_telegram_diplomat_interaction(record, expected_revision)
            }
        }
    }

    pub fn list_telegram_diplomat_interactions(
        &self,
        binding_id: &str,
    ) -> CoreResult<Vec<TelegramDiplomatInteractionRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.list_telegram_diplomat_interactions(binding_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.list_telegram_diplomat_interactions(binding_id),
        }
    }

    pub fn insert_runtime_activity(
        &self,
        record: &RuntimeActivityRecord,
    ) -> CoreResult<RuntimeActivityRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.insert_runtime_activity(record),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.insert_runtime_activity(record),
        }
    }

    pub fn update_runtime_activity(
        &self,
        record: &RuntimeActivityRecord,
        expected_revision: u64,
    ) -> CoreResult<RuntimeActivityRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.update_runtime_activity(record, expected_revision),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.update_runtime_activity(record, expected_revision),
        }
    }

    pub fn list_runtime_activities(
        &self,
        status: Option<RuntimeActivityStatus>,
        limit: Option<u32>,
    ) -> CoreResult<Vec<RuntimeActivityRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.list_runtime_activities(status, limit),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.list_runtime_activities(status, limit),
        }
    }

    pub fn list_runtime_activities_for_session(
        &self,
        session_id: &SessionId,
        limit: Option<u32>,
    ) -> CoreResult<Vec<RuntimeActivityRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.list_runtime_activities_for_session(session_id, limit),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => {
                postgres.list_runtime_activities_for_session(session_id, limit)
            }
        }
    }

    pub fn interrupt_runtime_activities_from_other_instances(
        &self,
        current_service_instance_id: &str,
        now: &IsoTimestamp,
    ) -> CoreResult<Vec<RuntimeActivityRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.interrupt_runtime_activities_from_other_instances(
                current_service_instance_id,
                now,
            ),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.interrupt_runtime_activities_from_other_instances(
                current_service_instance_id,
                now,
            ),
        }
    }

    pub fn insert_logical_turn_admission(
        &self,
        write: &LogicalTurnAdmissionWrite,
    ) -> CoreResult<LogicalTurnAdmission> {
        match self {
            Self::Sqlite(sqlite) => sqlite.insert_logical_turn_admission(write),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.insert_logical_turn_admission(write),
        }
    }

    pub fn get_logical_turn(
        &self,
        logical_turn_id: &LogicalTurnId,
    ) -> CoreResult<Option<LogicalTurnRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.get_logical_turn(logical_turn_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.get_logical_turn(logical_turn_id),
        }
    }

    pub fn list_logical_turns(
        &self,
        query: &LogicalTurnDiagnosticQuery,
    ) -> CoreResult<Vec<LogicalTurnRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.list_logical_turns(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.list_logical_turns(query),
        }
    }

    pub fn get_logical_turn_checkpoint(
        &self,
        continuation_id: &ContinuationId,
    ) -> CoreResult<Option<LogicalTurnCheckpoint>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.get_logical_turn_checkpoint(continuation_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.get_logical_turn_checkpoint(continuation_id),
        }
    }

    pub fn load_logical_turn_content(&self, content_ref: &str) -> CoreResult<Option<Vec<u8>>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.load_logical_turn_content(content_ref),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.load_logical_turn_content(content_ref),
        }
    }

    pub fn claim_logical_turn(
        &self,
        request: &LogicalTurnClaimRequest,
    ) -> CoreResult<LogicalTurnContinuationClaim> {
        match self {
            Self::Sqlite(sqlite) => sqlite.claim_logical_turn(request),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.claim_logical_turn(request),
        }
    }

    pub fn yield_logical_turn(
        &self,
        request: &LogicalTurnYieldRequest,
    ) -> CoreResult<LogicalTurnYieldReceipt> {
        match self {
            Self::Sqlite(sqlite) => sqlite.yield_logical_turn(request),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.yield_logical_turn(request),
        }
    }

    pub fn require_logical_turn_attention(
        &self,
        request: &LogicalTurnAttentionRequest,
    ) -> CoreResult<LogicalTurnAttentionReceipt> {
        match self {
            Self::Sqlite(sqlite) => sqlite.require_logical_turn_attention(request),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.require_logical_turn_attention(request),
        }
    }

    pub fn resolve_logical_turn_attention(
        &self,
        request: &LogicalTurnAttentionResolutionRequest,
    ) -> CoreResult<LogicalTurnAttentionResolutionReceipt> {
        match self {
            Self::Sqlite(sqlite) => sqlite.resolve_logical_turn_attention(request),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.resolve_logical_turn_attention(request),
        }
    }

    pub fn complete_logical_turn(
        &self,
        request: &LogicalTurnCompletionRequest,
    ) -> CoreResult<LogicalTurnRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.complete_logical_turn(request),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.complete_logical_turn(request),
        }
    }

    pub fn cancel_logical_turn(
        &self,
        request: &LogicalTurnCancelRequest,
    ) -> CoreResult<LogicalTurnCancellationReceipt> {
        match self {
            Self::Sqlite(sqlite) => sqlite.cancel_logical_turn(request),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.cancel_logical_turn(request),
        }
    }

    pub fn insert_logical_turn_operation(
        &self,
        operation: &LogicalTurnOperationRecord,
    ) -> CoreResult<LogicalTurnOperationRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.insert_logical_turn_operation(operation),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.insert_logical_turn_operation(operation),
        }
    }

    pub fn lease_logical_turn_operation(
        &self,
        request: &LogicalTurnOperationLeaseRequest,
    ) -> CoreResult<LogicalTurnOperationRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.lease_logical_turn_operation(request),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.lease_logical_turn_operation(request),
        }
    }

    pub fn complete_logical_turn_operation(
        &self,
        request: &LogicalTurnOperationCompletionRequest,
    ) -> CoreResult<LogicalTurnOperationRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.complete_logical_turn_operation(request),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.complete_logical_turn_operation(request),
        }
    }

    pub fn update_logical_turn_operation(
        &self,
        operation: &LogicalTurnOperationRecord,
        expected_revision: u64,
    ) -> CoreResult<LogicalTurnOperationRecord> {
        match self {
            Self::Sqlite(sqlite) => {
                sqlite.update_logical_turn_operation(operation, expected_revision)
            }
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => {
                postgres.update_logical_turn_operation(operation, expected_revision)
            }
        }
    }

    pub fn list_logical_turn_operations(
        &self,
        logical_turn_id: &LogicalTurnId,
    ) -> CoreResult<Vec<LogicalTurnOperationRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.list_logical_turn_operations(logical_turn_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.list_logical_turn_operations(logical_turn_id),
        }
    }

    pub fn list_logical_turn_tickets(&self) -> CoreResult<Vec<LogicalTurnContinuationTicket>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.list_logical_turn_tickets(),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.list_logical_turn_tickets(),
        }
    }

    pub fn list_pending_logical_turn_outbox(&self) -> CoreResult<Vec<LogicalTurnOutboxRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.list_pending_logical_turn_outbox(),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.list_pending_logical_turn_outbox(),
        }
    }

    pub fn mark_logical_turn_outbox_delivered(
        &self,
        projection_id: &str,
        delivered_at: &IsoTimestamp,
    ) -> CoreResult<()> {
        match self {
            Self::Sqlite(sqlite) => {
                sqlite.mark_logical_turn_outbox_delivered(projection_id, delivered_at)
            }
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => {
                postgres.mark_logical_turn_outbox_delivered(projection_id, delivered_at)
            }
        }
    }

    pub fn hydrate_logical_turns(
        &self,
        now: &IsoTimestamp,
    ) -> CoreResult<LogicalTurnHydrationReport> {
        match self {
            Self::Sqlite(sqlite) => sqlite.hydrate_logical_turns(now),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.hydrate_logical_turns(now),
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

    pub fn put_external_runtime_registration(
        &self,
        record: &ExternalRuntimeRegistration,
        expected_revision: Option<u64>,
    ) -> CoreResult<ExternalRuntimeRegistration> {
        match self {
            Self::Sqlite(store) => {
                store.put_external_runtime_registration(record, expected_revision)
            }
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => {
                store.put_external_runtime_registration(record, expected_revision)
            }
        }
    }

    pub fn get_external_runtime_registration(
        &self,
        runtime_id: &ExternalRuntimeId,
    ) -> CoreResult<Option<ExternalRuntimeRegistration>> {
        match self {
            Self::Sqlite(store) => store.get_external_runtime_registration(runtime_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.get_external_runtime_registration(runtime_id),
        }
    }

    pub fn list_external_runtime_registrations(
        &self,
    ) -> CoreResult<Vec<ExternalRuntimeRegistration>> {
        match self {
            Self::Sqlite(store) => store.list_external_runtime_registrations(),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.list_external_runtime_registrations(),
        }
    }

    pub fn record_external_runtime_certification(
        &self,
        record: &ExternalRuntimeCertificationRecord,
    ) -> CoreResult<ExternalRuntimeCertificationRecord> {
        match self {
            Self::Sqlite(store) => store.record_external_runtime_certification(record),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.record_external_runtime_certification(record),
        }
    }

    pub fn put_external_runtime_probe_evidence(
        &self,
        evidence: &ExternalRuntimeProbeEvidenceRecord,
    ) -> CoreResult<()> {
        match self {
            Self::Sqlite(store) => store.put_external_runtime_probe_evidence(evidence),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.put_external_runtime_probe_evidence(evidence),
        }
    }

    pub fn get_external_runtime_probe_evidence(
        &self,
        runtime_id: &ExternalRuntimeId,
    ) -> CoreResult<Option<ExternalRuntimeProbeEvidenceRecord>> {
        match self {
            Self::Sqlite(store) => store.get_external_runtime_probe_evidence(runtime_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.get_external_runtime_probe_evidence(runtime_id),
        }
    }

    pub fn get_external_runtime_certification(
        &self,
        certification_id: &str,
    ) -> CoreResult<Option<ExternalRuntimeCertificationRecord>> {
        match self {
            Self::Sqlite(store) => store.get_external_runtime_certification(certification_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.get_external_runtime_certification(certification_id),
        }
    }

    pub fn list_external_runtime_certifications(
        &self,
    ) -> CoreResult<Vec<ExternalRuntimeCertificationRecord>> {
        match self {
            Self::Sqlite(store) => store.list_external_runtime_certifications(),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.list_external_runtime_certifications(),
        }
    }

    pub fn find_active_external_runtime_certification(
        &self,
        runtime_kind: &ExternalRuntimeKind,
        observed_cli_version: &str,
        consumed_contract_revision: &str,
        probe_suite_revision: &str,
    ) -> CoreResult<Option<ExternalRuntimeCertificationRecord>> {
        match self {
            Self::Sqlite(store) => store.find_active_external_runtime_certification(
                runtime_kind,
                observed_cli_version,
                consumed_contract_revision,
                probe_suite_revision,
            ),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.find_active_external_runtime_certification(
                runtime_kind,
                observed_cli_version,
                consumed_contract_revision,
                probe_suite_revision,
            ),
        }
    }

    pub fn invalidate_external_runtime_certification(
        &self,
        invalidation: &ExternalRuntimeCertificationInvalidation,
    ) -> CoreResult<ExternalRuntimeCertificationRecord> {
        match self {
            Self::Sqlite(store) => store.invalidate_external_runtime_certification(invalidation),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.invalidate_external_runtime_certification(invalidation),
        }
    }

    pub fn acquire_external_controller_lease(
        &self,
        candidate: &ExternalControllerLease,
        now: &IsoTimestamp,
    ) -> CoreResult<ExternalControllerLease> {
        match self {
            Self::Sqlite(store) => store.acquire_external_controller_lease(candidate, now),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.acquire_external_controller_lease(candidate, now),
        }
    }

    pub fn release_external_controller_lease(
        &self,
        runtime_id: &ExternalRuntimeId,
        holder_instance_id: &str,
        generation: u64,
        now: &IsoTimestamp,
    ) -> CoreResult<ExternalControllerLease> {
        match self {
            Self::Sqlite(store) => store.release_external_controller_lease(
                runtime_id,
                holder_instance_id,
                generation,
                now,
            ),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.release_external_controller_lease(
                runtime_id,
                holder_instance_id,
                generation,
                now,
            ),
        }
    }

    pub fn get_external_controller_lease(
        &self,
        runtime_id: &ExternalRuntimeId,
    ) -> CoreResult<Option<ExternalControllerLease>> {
        match self {
            Self::Sqlite(store) => store.get_external_controller_lease(runtime_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.get_external_controller_lease(runtime_id),
        }
    }

    pub fn put_external_agent_binding(
        &self,
        record: &ExternalAgentBinding,
        expected_revision: Option<u64>,
    ) -> CoreResult<ExternalAgentBinding> {
        match self {
            Self::Sqlite(store) => store.put_external_agent_binding(record, expected_revision),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.put_external_agent_binding(record, expected_revision),
        }
    }

    pub fn get_external_binding_for_agent(
        &self,
        agent_id: &AgentId,
    ) -> CoreResult<Option<ExternalAgentBinding>> {
        match self {
            Self::Sqlite(store) => store.get_external_binding_for_agent(agent_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.get_external_binding_for_agent(agent_id),
        }
    }

    pub fn get_external_agent_binding(
        &self,
        binding_id: &ExternalBindingId,
    ) -> CoreResult<Option<ExternalAgentBinding>> {
        match self {
            Self::Sqlite(store) => store.get_external_agent_binding(binding_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.get_external_agent_binding(binding_id),
        }
    }

    pub fn list_external_agent_bindings(&self) -> CoreResult<Vec<ExternalAgentBinding>> {
        match self {
            Self::Sqlite(store) => store.list_external_agent_bindings(),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.list_external_agent_bindings(),
        }
    }

    pub fn create_external_agent_session_creation(
        &self,
        record: &ExternalAgentSessionCreationRecord,
    ) -> CoreResult<ExternalAgentSessionCreationRecord> {
        match self {
            Self::Sqlite(store) => store.create_external_agent_session_creation(record),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.create_external_agent_session_creation(record),
        }
    }

    pub fn get_external_agent_session_creation(
        &self,
        creation_id: &ExternalAgentSessionCreationId,
    ) -> CoreResult<Option<ExternalAgentSessionCreationRecord>> {
        match self {
            Self::Sqlite(store) => store.get_external_agent_session_creation(creation_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.get_external_agent_session_creation(creation_id),
        }
    }

    pub fn update_external_agent_session_creation(
        &self,
        next: &ExternalAgentSessionCreationRecord,
        expected_revision: u64,
    ) -> CoreResult<ExternalAgentSessionCreationRecord> {
        match self {
            Self::Sqlite(store) => {
                store.update_external_agent_session_creation(next, expected_revision)
            }
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => {
                store.update_external_agent_session_creation(next, expected_revision)
            }
        }
    }

    pub fn create_external_turn(
        &self,
        record: &ExternalTurnCorrelation,
    ) -> CoreResult<ExternalTurnCorrelation> {
        match self {
            Self::Sqlite(store) => store.create_external_turn(record),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.create_external_turn(record),
        }
    }

    pub fn promote_queued_message_to_external_turn(
        &self,
        queued_message_id: &str,
        now: &IsoTimestamp,
        record: &ExternalTurnCorrelation,
    ) -> CoreResult<Option<ExternalTurnCorrelation>> {
        match self {
            Self::Sqlite(store) => {
                store.promote_queued_message_to_external_turn(queued_message_id, now, record)
            }
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => {
                store.promote_queued_message_to_external_turn(queued_message_id, now, record)
            }
        }
    }

    pub fn update_external_turn(
        &self,
        next: &ExternalTurnCorrelation,
        expected_revision: u64,
    ) -> CoreResult<ExternalTurnCorrelation> {
        match self {
            Self::Sqlite(store) => store.update_external_turn(next, expected_revision),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.update_external_turn(next, expected_revision),
        }
    }

    pub fn get_external_turn(
        &self,
        request_id: &ExternalTurnRequestId,
    ) -> CoreResult<Option<ExternalTurnCorrelation>> {
        match self {
            Self::Sqlite(store) => store.get_external_turn(request_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.get_external_turn(request_id),
        }
    }

    pub fn list_external_turns_for_native_thread(
        &self,
        runtime_id: &ExternalRuntimeId,
        native_thread_id: &str,
    ) -> CoreResult<Vec<ExternalTurnCorrelation>> {
        match self {
            Self::Sqlite(store) => {
                store.list_external_turns_for_native_thread(runtime_id, native_thread_id)
            }
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => {
                store.list_external_turns_for_native_thread(runtime_id, native_thread_id)
            }
        }
    }

    pub fn list_nonterminal_external_turns(&self) -> CoreResult<Vec<ExternalTurnCorrelation>> {
        match self {
            Self::Sqlite(store) => store.list_nonterminal_external_turns(),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.list_nonterminal_external_turns(),
        }
    }

    pub fn put_external_control_receipt(
        &self,
        receipt: &ExternalControlReceipt,
    ) -> CoreResult<ExternalControlReceipt> {
        match self {
            Self::Sqlite(store) => store.put_external_control_receipt(receipt),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.put_external_control_receipt(receipt),
        }
    }

    pub fn get_external_control_receipt(
        &self,
        control_id: &ExternalControlId,
    ) -> CoreResult<Option<ExternalControlReceipt>> {
        match self {
            Self::Sqlite(store) => store.get_external_control_receipt(control_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.get_external_control_receipt(control_id),
        }
    }

    pub fn update_external_control_receipt(
        &self,
        next: &ExternalControlReceipt,
        expected_revision: u64,
    ) -> CoreResult<ExternalControlReceipt> {
        match self {
            Self::Sqlite(store) => store.update_external_control_receipt(next, expected_revision),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.update_external_control_receipt(next, expected_revision),
        }
    }

    pub fn put_external_interaction(
        &self,
        record: &ExternalInteractionRecord,
    ) -> CoreResult<ExternalInteractionRecord> {
        match self {
            Self::Sqlite(store) => store.put_external_interaction(record),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.put_external_interaction(record),
        }
    }

    pub fn update_external_interaction(
        &self,
        next: &ExternalInteractionRecord,
        expected_revision: u64,
    ) -> CoreResult<ExternalInteractionRecord> {
        match self {
            Self::Sqlite(store) => store.update_external_interaction(next, expected_revision),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.update_external_interaction(next, expected_revision),
        }
    }

    pub fn list_pending_external_interactions(&self) -> CoreResult<Vec<ExternalInteractionRecord>> {
        match self {
            Self::Sqlite(store) => store.list_pending_external_interactions(),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.list_pending_external_interactions(),
        }
    }

    pub fn append_external_runtime_event(
        &self,
        event: &NormalizedExternalRuntimeEvent,
    ) -> CoreResult<()> {
        match self {
            Self::Sqlite(store) => store.append_external_runtime_event(event),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.append_external_runtime_event(event),
        }
    }

    pub fn append_external_runtime_event_allocated(
        &self,
        input: &ExternalRuntimeEventInput,
    ) -> CoreResult<NormalizedExternalRuntimeEvent> {
        match self {
            Self::Sqlite(store) => store.append_external_runtime_event_allocated(input),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.append_external_runtime_event_allocated(input),
        }
    }

    pub fn query_external_runtime_events(
        &self,
        runtime_id: &ExternalRuntimeId,
        after_sequence: u64,
        limit: u32,
    ) -> CoreResult<Vec<NormalizedExternalRuntimeEvent>> {
        match self {
            Self::Sqlite(store) => {
                store.query_external_runtime_events(runtime_id, after_sequence, limit)
            }
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => {
                store.query_external_runtime_events(runtime_id, after_sequence, limit)
            }
        }
    }

    pub fn query_external_runtime_thread_events(
        &self,
        runtime_id: &ExternalRuntimeId,
        native_thread_id: &str,
        after_sequence: u64,
        limit: u32,
    ) -> CoreResult<Vec<NormalizedExternalRuntimeEvent>> {
        match self {
            Self::Sqlite(store) => store.query_external_runtime_thread_events(
                runtime_id,
                native_thread_id,
                after_sequence,
                limit,
            ),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.query_external_runtime_thread_events(
                runtime_id,
                native_thread_id,
                after_sequence,
                limit,
            ),
        }
    }

    pub fn query_external_runtime_event_tail(
        &self,
        runtime_id: &ExternalRuntimeId,
        limit: u32,
    ) -> CoreResult<Vec<NormalizedExternalRuntimeEvent>> {
        match self {
            Self::Sqlite(store) => store.query_external_runtime_event_tail(runtime_id, limit),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.query_external_runtime_event_tail(runtime_id, limit),
        }
    }

    pub fn create_agent_correlated_round(
        &self,
        record: &AgentCorrelatedRound,
    ) -> CoreResult<AgentCorrelatedRound> {
        match self {
            Self::Sqlite(store) => store.create_agent_correlated_round(record),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.create_agent_correlated_round(record),
        }
    }

    pub fn create_agent_message_delivery(
        &self,
        record: &AgentMessageDeliveryReceipt,
    ) -> CoreResult<AgentMessageDeliveryReceipt> {
        match self {
            Self::Sqlite(store) => store.create_agent_message_delivery(record),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.create_agent_message_delivery(record),
        }
    }

    pub fn update_agent_message_delivery(
        &self,
        next: &AgentMessageDeliveryReceipt,
        expected_revision: u64,
    ) -> CoreResult<AgentMessageDeliveryReceipt> {
        match self {
            Self::Sqlite(store) => store.update_agent_message_delivery(next, expected_revision),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.update_agent_message_delivery(next, expected_revision),
        }
    }

    pub fn get_agent_correlated_round(
        &self,
        round_id: &AgentRoundId,
    ) -> CoreResult<Option<AgentCorrelatedRound>> {
        match self {
            Self::Sqlite(store) => store.get_agent_correlated_round(round_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.get_agent_correlated_round(round_id),
        }
    }

    pub fn get_agent_message_delivery(
        &self,
        delivery_id: &rusty_crew_core_protocol::AgentMessageDeliveryId,
    ) -> CoreResult<Option<AgentMessageDeliveryReceipt>> {
        match self {
            Self::Sqlite(store) => store.get_agent_message_delivery(delivery_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.get_agent_message_delivery(delivery_id),
        }
    }

    pub fn get_agent_message_delivery_by_message_id(
        &self,
        message_id: &str,
    ) -> CoreResult<Option<AgentMessageDeliveryReceipt>> {
        match self {
            Self::Sqlite(store) => store.get_agent_message_delivery_by_message_id(message_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.get_agent_message_delivery_by_message_id(message_id),
        }
    }

    pub fn get_agent_message_reply(
        &self,
        message_id: &str,
    ) -> CoreResult<Option<AgentMessageDeliveryReceipt>> {
        match self {
            Self::Sqlite(store) => store.get_agent_message_reply(message_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.get_agent_message_reply(message_id),
        }
    }

    pub fn list_agent_message_inbox_deliveries(
        &self,
        query: &rusty_crew_core_protocol::AgentMessageInboxQuery,
        limit: u32,
    ) -> CoreResult<Vec<AgentMessageDeliveryReceipt>> {
        match self {
            Self::Sqlite(store) => store.list_agent_message_inbox_deliveries(query, limit),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.list_agent_message_inbox_deliveries(query, limit),
        }
    }

    pub fn list_agent_message_traffic_deliveries(
        &self,
        query: &rusty_crew_core_protocol::AgentMessageInboxQuery,
        limit: u32,
    ) -> CoreResult<Vec<AgentMessageDeliveryReceipt>> {
        match self {
            Self::Sqlite(store) => store.list_agent_message_traffic_deliveries(query, limit),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.list_agent_message_traffic_deliveries(query, limit),
        }
    }

    pub fn list_pending_agent_message_deliveries(
        &self,
    ) -> CoreResult<Vec<AgentMessageDeliveryReceipt>> {
        match self {
            Self::Sqlite(store) => store.list_pending_agent_message_deliveries(),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.list_pending_agent_message_deliveries(),
        }
    }

    pub fn put_agent_route(&self, write: &AgentRouteWrite) -> CoreResult<AgentRouteRecord> {
        match self {
            Self::Sqlite(store) => store.put_agent_route(write),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.put_agent_route(write),
        }
    }

    pub fn get_agent_route(
        &self,
        route_key: &AgentRouteKey,
    ) -> CoreResult<Option<AgentRouteRecord>> {
        match self {
            Self::Sqlite(store) => store.get_agent_route(route_key),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.get_agent_route(route_key),
        }
    }

    pub fn list_agent_routes(&self) -> CoreResult<Vec<AgentRouteRecord>> {
        match self {
            Self::Sqlite(store) => store.list_agent_routes(),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.list_agent_routes(),
        }
    }

    pub fn get_latest_agent_route_delivery(
        &self,
        route_key: &AgentRouteKey,
    ) -> CoreResult<Option<AgentMessageDeliveryReceipt>> {
        match self {
            Self::Sqlite(store) => store.get_latest_agent_route_delivery(route_key),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.get_latest_agent_route_delivery(route_key),
        }
    }

    pub fn delete_agent_route(&self, delete: &AgentRouteDelete) -> CoreResult<AgentRouteRecord> {
        match self {
            Self::Sqlite(store) => store.delete_agent_route(delete),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.delete_agent_route(delete),
        }
    }

    pub fn update_agent_correlated_round(
        &self,
        next: &AgentCorrelatedRound,
        expected_revision: u64,
    ) -> CoreResult<AgentCorrelatedRound> {
        match self {
            Self::Sqlite(store) => store.update_agent_correlated_round(next, expected_revision),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.update_agent_correlated_round(next, expected_revision),
        }
    }

    pub fn list_pending_agent_rounds(&self) -> CoreResult<Vec<AgentCorrelatedRound>> {
        match self {
            Self::Sqlite(store) => store.list_pending_agent_rounds(),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.list_pending_agent_rounds(),
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

    pub fn record_provider_state_compatibility_plan(
        &self,
        row_id: i64,
        current: &ProviderStateCompatibilitySnapshot,
        plan: &ProviderStateCompatibilityPlan,
        now: &IsoTimestamp,
    ) -> CoreResult<()> {
        match self {
            Self::Sqlite(sqlite) => {
                sqlite.record_provider_state_compatibility_plan(row_id, current, plan, now)
            }
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => {
                postgres.record_provider_state_compatibility_plan(row_id, current, plan, now)
            }
        }
    }

    pub fn count_rows(&self, table: &str) -> CoreResult<u64> {
        match self {
            Self::Sqlite(sqlite) => sqlite.count_rows(table),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => {
                let table = DiagnosticTable::parse(table)?.as_str().to_string();
                postgres.exact_table_rows(&table)
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
                let pressure_signals = diagnostics
                    .filesystem_headroom
                    .free_percent
                    .map(|free_percent| {
                        vec![RuntimeStoragePressureSignal {
                            name: "filesystem_free_percent".to_string(),
                            active: diagnostics.filesystem_headroom.warning_active,
                            severity: if diagnostics.filesystem_headroom.warning_active {
                                "warning".to_string()
                            } else {
                                "info".to_string()
                            },
                            observed_value: free_percent as u64,
                            threshold_value: diagnostics
                                .filesystem_headroom
                                .warning_free_percent
                                .map(u64::from),
                            detail: diagnostics.filesystem_headroom.detail.clone(),
                        }]
                    })
                    .unwrap_or_default();
                let pressure = pressure_signals.iter().any(|signal| signal.active);
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
                    pressure_signals,
                    pressure,
                    external_runtime_events: diagnostics.external_runtime_events,
                    filesystem_headroom: diagnostics.filesystem_headroom,
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

    pub fn upsert_service_credential(
        &self,
        write: &ServiceCredentialWrite,
    ) -> CoreResult<ServiceCredentialRecord> {
        match self {
            Self::Sqlite(store) => store.upsert_service_credential(write),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.upsert_service_credential(write),
        }
    }

    pub fn get_service_credential(
        &self,
        credential_id: &str,
    ) -> CoreResult<Option<ServiceCredentialRecord>> {
        match self {
            Self::Sqlite(store) => store.get_service_credential(credential_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.get_service_credential(credential_id),
        }
    }

    pub fn get_service_credential_secret(&self, credential_id: &str) -> CoreResult<Option<String>> {
        match self {
            Self::Sqlite(store) => store.get_service_credential_secret(credential_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.get_service_credential_secret(credential_id),
        }
    }

    pub fn delete_service_credential(
        &self,
        delete: &ServiceCredentialDelete,
    ) -> CoreResult<ServiceCredentialRecord> {
        match self {
            Self::Sqlite(store) => store.delete_service_credential(delete),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.delete_service_credential(delete),
        }
    }

    pub fn list_service_credentials(
        &self,
        query: &ServiceCredentialQuery,
    ) -> CoreResult<Vec<ServiceCredentialRecord>> {
        match self {
            Self::Sqlite(store) => store.list_service_credentials(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.list_service_credentials(query),
        }
    }

    pub fn link_model_provider_credential(
        &self,
        link: &ModelProviderCredentialLink,
    ) -> CoreResult<ModelProviderCredentialLinkResult> {
        match self {
            Self::Sqlite(store) => store.link_model_provider_credential(link),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.link_model_provider_credential(link),
        }
    }

    pub fn unlink_model_provider_credential(
        &self,
        unlink: &ModelProviderCredentialUnlink,
    ) -> CoreResult<ModelProviderRecord> {
        match self {
            Self::Sqlite(store) => store.unlink_model_provider_credential(unlink),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.unlink_model_provider_credential(unlink),
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

    pub fn apply_curator_governance_write(
        &self,
        write: &CuratorGovernanceWrite,
    ) -> CoreResult<CuratorGovernanceWriteResult> {
        match self {
            Self::Sqlite(store) => store.apply_curator_governance_write(write),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.apply_curator_governance_write(write),
        }
    }

    pub fn get_curator_candidate(
        &self,
        candidate_id: &str,
    ) -> CoreResult<Option<CuratorCandidateRecord>> {
        match self {
            Self::Sqlite(store) => store.get_curator_candidate(candidate_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.get_curator_candidate(candidate_id),
        }
    }

    pub fn list_curator_candidates(
        &self,
        query: &CuratorCandidateQuery,
    ) -> CoreResult<ExactPage<CuratorCandidateRecord>> {
        match self {
            Self::Sqlite(store) => store.list_curator_candidates(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.list_curator_candidates(query),
        }
    }

    pub fn get_curator_mutation(
        &self,
        mutation_id: &str,
    ) -> CoreResult<Option<CuratorMutationRecord>> {
        match self {
            Self::Sqlite(store) => store.get_curator_mutation(mutation_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.get_curator_mutation(mutation_id),
        }
    }

    pub fn list_curator_mutations(
        &self,
        query: &CuratorMutationQuery,
    ) -> CoreResult<ExactPage<CuratorMutationRecord>> {
        match self {
            Self::Sqlite(store) => store.list_curator_mutations(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.list_curator_mutations(query),
        }
    }

    pub fn list_curator_audit_receipts(
        &self,
        query: &CuratorAuditQuery,
    ) -> CoreResult<ExactPage<CuratorAuditReceiptRecord>> {
        match self {
            Self::Sqlite(store) => store.list_curator_audit_receipts(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.list_curator_audit_receipts(query),
        }
    }

    pub fn purge_curator_profile(&self, profile_id: &str) -> CoreResult<CuratorPurgeReport> {
        match self {
            Self::Sqlite(store) => store.purge_curator_profile(profile_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.purge_curator_profile(profile_id),
        }
    }

    pub fn purge_curator_session(&self, session_id: &str) -> CoreResult<CuratorPurgeReport> {
        match self {
            Self::Sqlite(store) => store.purge_curator_session(session_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.purge_curator_session(session_id),
        }
    }

    pub fn put_roleplay_character(
        &self,
        write: &RoleplayCharacterWrite,
    ) -> CoreResult<RoleplayCharacterRecord> {
        match self {
            Self::Sqlite(store) => store.put_roleplay_character(write),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.put_roleplay_character(write),
        }
    }
    pub fn get_roleplay_character(&self, id: &str) -> CoreResult<Option<RoleplayCharacterRecord>> {
        match self {
            Self::Sqlite(store) => store.get_roleplay_character(id),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.get_roleplay_character(id),
        }
    }
    pub fn list_roleplay_characters(
        &self,
        query: &RoleplayCharacterQuery,
    ) -> CoreResult<Vec<RoleplayCharacterRecord>> {
        match self {
            Self::Sqlite(store) => store.list_roleplay_characters(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.list_roleplay_characters(query),
        }
    }
    pub fn put_roleplay_player_persona(
        &self,
        write: &RoleplayPlayerPersonaWrite,
    ) -> CoreResult<RoleplayPlayerPersonaRecord> {
        match self {
            Self::Sqlite(store) => store.put_roleplay_player_persona(write),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.put_roleplay_player_persona(write),
        }
    }
    pub fn get_roleplay_player_persona(
        &self,
        id: &str,
    ) -> CoreResult<Option<RoleplayPlayerPersonaRecord>> {
        match self {
            Self::Sqlite(store) => store.get_roleplay_player_persona(id),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.get_roleplay_player_persona(id),
        }
    }
    pub fn list_roleplay_player_personas(
        &self,
        query: &RoleplayPlayerPersonaQuery,
    ) -> CoreResult<Vec<RoleplayPlayerPersonaRecord>> {
        match self {
            Self::Sqlite(store) => store.list_roleplay_player_personas(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.list_roleplay_player_personas(query),
        }
    }
    pub fn put_roleplay_session_metadata(
        &self,
        write: &RoleplaySessionMetadataWrite,
    ) -> CoreResult<RoleplaySessionMetadataRecord> {
        match self {
            Self::Sqlite(store) => store.put_roleplay_session_metadata(write),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.put_roleplay_session_metadata(write),
        }
    }
    pub fn get_roleplay_session_metadata(
        &self,
        id: &str,
    ) -> CoreResult<Option<RoleplaySessionMetadataRecord>> {
        match self {
            Self::Sqlite(store) => store.get_roleplay_session_metadata(id),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.get_roleplay_session_metadata(id),
        }
    }
    pub fn list_roleplay_session_metadata(
        &self,
        query: &RoleplaySessionMetadataQuery,
    ) -> CoreResult<Vec<RoleplaySessionMetadataRecord>> {
        match self {
            Self::Sqlite(store) => store.list_roleplay_session_metadata(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.list_roleplay_session_metadata(query),
        }
    }
    pub fn apply_roleplay_session_projection(
        &self,
        write: &RoleplaySessionProjectionWrite,
    ) -> CoreResult<RoleplaySessionProjectionRecord> {
        match self {
            Self::Sqlite(store) => store.apply_roleplay_session_projection(write),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.apply_roleplay_session_projection(write),
        }
    }
    pub fn put_roleplay_import(
        &self,
        write: &RoleplayImportWrite,
    ) -> CoreResult<RoleplayImportRecord> {
        match self {
            Self::Sqlite(store) => store.put_roleplay_import(write),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.put_roleplay_import(write),
        }
    }
    pub fn get_roleplay_import(&self, id: &str) -> CoreResult<Option<RoleplayImportRecord>> {
        match self {
            Self::Sqlite(store) => store.get_roleplay_import(id),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.get_roleplay_import(id),
        }
    }
    pub fn list_roleplay_imports(
        &self,
        query: &RoleplayImportQuery,
    ) -> CoreResult<Vec<RoleplayImportRecord>> {
        match self {
            Self::Sqlite(store) => store.list_roleplay_imports(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.list_roleplay_imports(query),
        }
    }

    pub fn create_roleplay_mechanic_proposal(
        &self,
        persist: &RoleplayMechanicProposalPersist,
    ) -> CoreResult<RoleplayMechanicProposalRecord> {
        match self {
            Self::Sqlite(store) => store.create_roleplay_mechanic_proposal(persist),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.create_roleplay_mechanic_proposal(persist),
        }
    }

    pub fn get_roleplay_mechanic_proposal(
        &self,
        proposal_id: &str,
    ) -> CoreResult<Option<RoleplayMechanicProposalRecord>> {
        match self {
            Self::Sqlite(store) => store.get_roleplay_mechanic_proposal(proposal_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.get_roleplay_mechanic_proposal(proposal_id),
        }
    }

    pub fn list_roleplay_mechanic_proposals(
        &self,
        query: &RoleplayMechanicProposalQuery,
    ) -> CoreResult<Vec<RoleplayMechanicProposalRecord>> {
        match self {
            Self::Sqlite(store) => store.list_roleplay_mechanic_proposals(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.list_roleplay_mechanic_proposals(query),
        }
    }

    pub fn decide_roleplay_mechanic_proposal(
        &self,
        decision: &RoleplayMechanicProposalDecision,
    ) -> CoreResult<RoleplayMechanicProposalRecord> {
        match self {
            Self::Sqlite(store) => store.decide_roleplay_mechanic_proposal(decision),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.decide_roleplay_mechanic_proposal(decision),
        }
    }

    pub fn record_roleplay_mechanic_proposal_apply(
        &self,
        outcome: &RoleplayMechanicProposalApplyOutcome,
    ) -> CoreResult<RoleplayMechanicProposalRecord> {
        match self {
            Self::Sqlite(store) => store.record_roleplay_mechanic_proposal_apply(outcome),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.record_roleplay_mechanic_proposal_apply(outcome),
        }
    }

    pub fn put_roleplay_mechanic_session_association(
        &self,
        write: &RoleplayMechanicSessionAssociationWrite,
    ) -> CoreResult<RoleplayMechanicSessionAssociationRecord> {
        match self {
            Self::Sqlite(store) => store.put_roleplay_mechanic_session_association(write),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.put_roleplay_mechanic_session_association(write),
        }
    }

    pub fn get_roleplay_mechanic_session_association(
        &self,
        mechanic_session_id: &SessionId,
    ) -> CoreResult<Option<RoleplayMechanicSessionAssociationRecord>> {
        match self {
            Self::Sqlite(store) => {
                store.get_roleplay_mechanic_session_association(mechanic_session_id)
            }
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => {
                store.get_roleplay_mechanic_session_association(mechanic_session_id)
            }
        }
    }

    pub fn list_roleplay_mechanic_session_associations(
        &self,
        query: &RoleplayMechanicSessionAssociationQuery,
    ) -> CoreResult<Vec<RoleplayMechanicSessionAssociationRecord>> {
        match self {
            Self::Sqlite(store) => store.list_roleplay_mechanic_session_associations(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.list_roleplay_mechanic_session_associations(query),
        }
    }

    pub fn create_roleplay_mechanic_diagnostic(
        &self,
        record: &RoleplayMechanicDiagnosticRecord,
    ) -> CoreResult<RoleplayMechanicDiagnosticRecord> {
        match self {
            Self::Sqlite(store) => store.create_roleplay_mechanic_diagnostic(record),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.create_roleplay_mechanic_diagnostic(record),
        }
    }

    pub fn get_roleplay_mechanic_diagnostic(
        &self,
        diagnostic_id: &str,
    ) -> CoreResult<Option<RoleplayMechanicDiagnosticRecord>> {
        match self {
            Self::Sqlite(store) => store.get_roleplay_mechanic_diagnostic(diagnostic_id),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.get_roleplay_mechanic_diagnostic(diagnostic_id),
        }
    }

    pub fn list_roleplay_mechanic_diagnostics(
        &self,
        query: &RoleplayMechanicDiagnosticQuery,
    ) -> CoreResult<Vec<RoleplayMechanicDiagnosticRecord>> {
        match self {
            Self::Sqlite(store) => store.list_roleplay_mechanic_diagnostics(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.list_roleplay_mechanic_diagnostics(query),
        }
    }

    pub fn update_roleplay_mechanic_diagnostic_outcome(
        &self,
        update: &RoleplayMechanicDiagnosticOutcomeUpdate,
    ) -> CoreResult<RoleplayMechanicDiagnosticRecord> {
        match self {
            Self::Sqlite(store) => store.update_roleplay_mechanic_diagnostic_outcome(update),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.update_roleplay_mechanic_diagnostic_outcome(update),
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

    pub fn create_chat_message_slot(
        &self,
        request: &CreateChatMessageSlotRequest,
    ) -> CoreResult<CreateChatMessageSlotResult> {
        match self {
            Self::Sqlite(sqlite) => sqlite.create_chat_message_slot(request),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.create_chat_message_slot(request),
        }
    }

    pub fn prune_chat_message_ingest_receipts(&self, now: &str) -> CoreResult<u64> {
        match self {
            Self::Sqlite(sqlite) => sqlite.prune_chat_message_ingest_receipts(now),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.prune_chat_message_ingest_receipts(now),
        }
    }

    pub fn purge_chat_message_ingest_receipts_for_session(
        &self,
        session_id: &SessionId,
    ) -> CoreResult<u64> {
        match self {
            Self::Sqlite(sqlite) => {
                sqlite.purge_chat_message_ingest_receipts_for_session(session_id)
            }
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => {
                postgres.purge_chat_message_ingest_receipts_for_session(session_id)
            }
        }
    }

    pub fn create_chat_message_variant(
        &self,
        request: &CreateChatMessageVariantRequest,
    ) -> CoreResult<CreateChatMessageVariantResult> {
        match self {
            Self::Sqlite(sqlite) => sqlite.create_chat_message_variant(request),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.create_chat_message_variant(request),
        }
    }
    pub fn apply_roleplay_alternative(
        &self,
        request: &ApplyRoleplayAlternativeRequest,
    ) -> CoreResult<ApplyRoleplayAlternativeResult> {
        match self {
            Self::Sqlite(store) => store.apply_roleplay_alternative(request),
            #[cfg(feature = "postgres")]
            Self::Postgres(store) => store.apply_roleplay_alternative(request),
        }
    }

    pub fn delete_chat_message_variant(
        &self,
        request: &DeleteChatMessageVariantRequest,
    ) -> CoreResult<MessageSlotRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.delete_chat_message_variant(request),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.delete_chat_message_variant(request),
        }
    }

    pub fn reorder_chat_message_variants(
        &self,
        request: &ReorderChatMessageVariantsRequest,
    ) -> CoreResult<Vec<MessageVariantRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.reorder_chat_message_variants(request),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.reorder_chat_message_variants(request),
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

    pub fn query_message_slots_page(
        &self,
        query: &MessageSlotQuery,
    ) -> CoreResult<ExactPage<MessageSlotRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.query_message_slots_page(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.query_message_slots_page(query),
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

    pub fn query_message_variants_page(
        &self,
        query: &SessionMessageVariantPageQuery,
    ) -> CoreResult<ExactPage<MessageVariantRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.query_message_variants_page(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.query_message_variants_page(query),
        }
    }

    pub fn append_chat_event(&self, event: &ChatEventLogAppend) -> CoreResult<ChatEventLogEvent> {
        match self {
            Self::Sqlite(sqlite) => sqlite.append_chat_event(event),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.append_chat_event(event),
        }
    }

    pub fn query_chat_events(&self, query: &ChatEventLogQuery) -> CoreResult<ChatEventLogPage> {
        match self {
            Self::Sqlite(sqlite) => sqlite.query_chat_events(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.query_chat_events(query),
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

    pub fn create_chat_conversation_branch(
        &self,
        request: &CreateChatConversationBranchRequest,
    ) -> CoreResult<ConversationBranchRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.create_chat_conversation_branch(request),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.create_chat_conversation_branch(request),
        }
    }

    pub fn ensure_active_chat_conversation_branch(
        &self,
        request: &EnsureActiveChatConversationBranchRequest,
    ) -> CoreResult<EnsureActiveChatConversationBranchResult> {
        match self {
            Self::Sqlite(sqlite) => sqlite.ensure_active_chat_conversation_branch(request),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.ensure_active_chat_conversation_branch(request),
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

    pub fn create_chat_conversation_snapshot(
        &self,
        request: &CreateChatConversationSnapshotRequest,
    ) -> CoreResult<CreateChatConversationSnapshotResult> {
        match self {
            Self::Sqlite(sqlite) => sqlite.create_chat_conversation_snapshot(request),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.create_chat_conversation_snapshot(request),
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

    pub fn read_conversation_tree(
        &self,
        query: &ConversationTreeReadQuery,
    ) -> CoreResult<ConversationTreeReadResult> {
        match self {
            Self::Sqlite(sqlite) => sqlite.read_conversation_tree(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.read_conversation_tree(query),
        }
    }

    pub fn search_chat_transcript(
        &self,
        query: &ChatTranscriptSearchQuery,
    ) -> CoreResult<ChatTranscriptSearchPage> {
        match self {
            Self::Sqlite(sqlite) => sqlite.search_chat_transcript(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.search_chat_transcript(query),
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

    pub fn create_chat_attachment(
        &self,
        request: &CreateChatAttachmentRequest,
    ) -> CoreResult<CreateChatAttachmentResult> {
        match self {
            Self::Sqlite(sqlite) => sqlite.create_chat_attachment(request),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.create_chat_attachment(request),
        }
    }

    pub fn query_attachments(&self, query: &AttachmentQuery) -> CoreResult<Vec<AttachmentRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.query_attachments(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.query_attachments(query),
        }
    }

    pub fn query_attachments_page(
        &self,
        query: &AttachmentQuery,
    ) -> CoreResult<ExactPage<AttachmentRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.query_attachments_page(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.query_attachments_page(query),
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

    pub fn remove_chat_attachment(
        &self,
        request: &RemoveChatAttachmentRequest,
    ) -> CoreResult<AttachmentRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.remove_chat_attachment(request),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.remove_chat_attachment(request),
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

    pub fn create_chat_data_bank_scope(
        &self,
        request: &CreateChatDataBankScopeRequest,
    ) -> CoreResult<CreateChatDataBankScopeResult> {
        match self {
            Self::Sqlite(sqlite) => sqlite.create_chat_data_bank_scope(request),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.create_chat_data_bank_scope(request),
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

    pub fn query_data_bank_scopes_page(
        &self,
        query: &DataBankScopeQuery,
    ) -> CoreResult<ExactPage<DataBankScopeRecord>> {
        match self {
            Self::Sqlite(sqlite) => sqlite.query_data_bank_scopes_page(query),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.query_data_bank_scopes_page(query),
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

    pub fn remove_chat_data_bank_scope(
        &self,
        request: &RemoveChatDataBankScopeRequest,
    ) -> CoreResult<DataBankScopeRecord> {
        match self {
            Self::Sqlite(sqlite) => sqlite.remove_chat_data_bank_scope(request),
            #[cfg(feature = "postgres")]
            Self::Postgres(postgres) => postgres.remove_chat_data_bank_scope(request),
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

    pub fn upsert_service_credential(
        &self,
        write: &ServiceCredentialWrite,
    ) -> CoreResult<ServiceCredentialRecord> {
        self.store.upsert_service_credential(write)
    }

    pub fn get_service_credential(
        &self,
        credential_id: &str,
    ) -> CoreResult<Option<ServiceCredentialRecord>> {
        self.store.get_service_credential(credential_id)
    }

    pub fn get_service_credential_secret(&self, credential_id: &str) -> CoreResult<Option<String>> {
        self.store.get_service_credential_secret(credential_id)
    }

    pub fn delete_service_credential(
        &self,
        delete: &ServiceCredentialDelete,
    ) -> CoreResult<ServiceCredentialRecord> {
        self.store.delete_service_credential(delete)
    }

    pub fn list_service_credentials(
        &self,
        query: &ServiceCredentialQuery,
    ) -> CoreResult<Vec<ServiceCredentialRecord>> {
        self.store.list_service_credentials(query)
    }

    pub fn link_model_provider_credential(
        &self,
        link: &ModelProviderCredentialLink,
    ) -> CoreResult<ModelProviderCredentialLinkResult> {
        self.store.link_model_provider_credential(link)
    }

    pub fn unlink_model_provider_credential(
        &self,
        unlink: &ModelProviderCredentialUnlink,
    ) -> CoreResult<ModelProviderRecord> {
        self.store.unlink_model_provider_credential(unlink)
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

    pub fn create_chat_message_slot(
        &self,
        request: &CreateChatMessageSlotRequest,
    ) -> CoreResult<CreateChatMessageSlotResult> {
        self.store.create_chat_message_slot(request)
    }

    pub fn prune_chat_message_ingest_receipts(&self, now: &str) -> CoreResult<u64> {
        self.store.prune_chat_message_ingest_receipts(now)
    }

    pub fn purge_chat_message_ingest_receipts_for_session(
        &self,
        session_id: &SessionId,
    ) -> CoreResult<u64> {
        self.store
            .purge_chat_message_ingest_receipts_for_session(session_id)
    }

    pub fn create_chat_message_variant(
        &self,
        request: &CreateChatMessageVariantRequest,
    ) -> CoreResult<CreateChatMessageVariantResult> {
        self.store.create_chat_message_variant(request)
    }

    pub fn delete_chat_message_variant(
        &self,
        request: &DeleteChatMessageVariantRequest,
    ) -> CoreResult<MessageSlotRecord> {
        self.store.delete_chat_message_variant(request)
    }

    pub fn reorder_chat_message_variants(
        &self,
        request: &ReorderChatMessageVariantsRequest,
    ) -> CoreResult<Vec<MessageVariantRecord>> {
        self.store.reorder_chat_message_variants(request)
    }

    pub fn query_message_slots(
        &self,
        query: &MessageSlotQuery,
    ) -> CoreResult<Vec<MessageSlotRecord>> {
        self.store.query_message_slots(query)
    }

    pub fn query_message_slots_page(
        &self,
        query: &MessageSlotQuery,
    ) -> CoreResult<ExactPage<MessageSlotRecord>> {
        self.store.query_message_slots_page(query)
    }

    pub fn query_message_variants(
        &self,
        query: &MessageVariantQuery,
    ) -> CoreResult<Vec<MessageVariantRecord>> {
        self.store.query_message_variants(query)
    }

    pub fn query_message_variants_page(
        &self,
        query: &SessionMessageVariantPageQuery,
    ) -> CoreResult<ExactPage<MessageVariantRecord>> {
        self.store.query_message_variants_page(query)
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

    pub fn create_chat_conversation_branch(
        &self,
        request: &CreateChatConversationBranchRequest,
    ) -> CoreResult<ConversationBranchRecord> {
        self.store.create_chat_conversation_branch(request)
    }

    pub fn ensure_active_chat_conversation_branch(
        &self,
        request: &EnsureActiveChatConversationBranchRequest,
    ) -> CoreResult<EnsureActiveChatConversationBranchResult> {
        self.store.ensure_active_chat_conversation_branch(request)
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

    pub fn create_chat_conversation_snapshot(
        &self,
        request: &CreateChatConversationSnapshotRequest,
    ) -> CoreResult<CreateChatConversationSnapshotResult> {
        self.store.create_chat_conversation_snapshot(request)
    }

    pub fn query_conversation_snapshots(
        &self,
        query: &ConversationSnapshotQuery,
    ) -> CoreResult<Vec<ConversationSnapshotRecord>> {
        self.store.query_conversation_snapshots(query)
    }

    pub fn read_conversation_tree(
        &self,
        query: &ConversationTreeReadQuery,
    ) -> CoreResult<ConversationTreeReadResult> {
        self.store.read_conversation_tree(query)
    }

    pub fn search_chat_transcript(
        &self,
        query: &ChatTranscriptSearchQuery,
    ) -> CoreResult<ChatTranscriptSearchPage> {
        self.store.search_chat_transcript(query)
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

    pub fn create_chat_attachment(
        &self,
        request: &CreateChatAttachmentRequest,
    ) -> CoreResult<CreateChatAttachmentResult> {
        self.store.create_chat_attachment(request)
    }

    pub fn query_attachments(&self, query: &AttachmentQuery) -> CoreResult<Vec<AttachmentRecord>> {
        self.store.query_attachments(query)
    }

    pub fn query_attachments_page(
        &self,
        query: &AttachmentQuery,
    ) -> CoreResult<ExactPage<AttachmentRecord>> {
        self.store.query_attachments_page(query)
    }

    pub fn remove_attachment(
        &self,
        attachment_id: &AttachmentId,
        updated_at: &IsoTimestamp,
    ) -> CoreResult<AttachmentRecord> {
        self.store.remove_attachment(attachment_id, updated_at)
    }

    pub fn remove_chat_attachment(
        &self,
        request: &RemoveChatAttachmentRequest,
    ) -> CoreResult<AttachmentRecord> {
        self.store.remove_chat_attachment(request)
    }

    pub fn save_data_bank_scope(
        &self,
        scope: &DataBankScopeWrite,
    ) -> CoreResult<DataBankScopeRecord> {
        self.store.save_data_bank_scope(scope)
    }

    pub fn create_chat_data_bank_scope(
        &self,
        request: &CreateChatDataBankScopeRequest,
    ) -> CoreResult<CreateChatDataBankScopeResult> {
        self.store.create_chat_data_bank_scope(request)
    }

    pub fn query_data_bank_scopes(
        &self,
        query: &DataBankScopeQuery,
    ) -> CoreResult<Vec<DataBankScopeRecord>> {
        self.store.query_data_bank_scopes(query)
    }

    pub fn query_data_bank_scopes_page(
        &self,
        query: &DataBankScopeQuery,
    ) -> CoreResult<ExactPage<DataBankScopeRecord>> {
        self.store.query_data_bank_scopes_page(query)
    }

    pub fn remove_data_bank_scope(
        &self,
        scope_id: &DataBankScopeId,
        updated_at: &IsoTimestamp,
    ) -> CoreResult<DataBankScopeRecord> {
        self.store.remove_data_bank_scope(scope_id, updated_at)
    }

    pub fn remove_chat_data_bank_scope(
        &self,
        request: &RemoveChatDataBankScopeRequest,
    ) -> CoreResult<DataBankScopeRecord> {
        self.store.remove_chat_data_bank_scope(request)
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

impl ChatEventRepositorySet<'_> {
    pub fn append_chat_event(&self, event: &ChatEventLogAppend) -> CoreResult<ChatEventLogEvent> {
        self.store.append_chat_event(event)
    }

    pub fn query_chat_events(&self, query: &ChatEventLogQuery) -> CoreResult<ChatEventLogPage> {
        self.store.query_chat_events(query)
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

fn runtime_external_event_storage_diagnostics(
    conn: &Connection,
) -> CoreResult<RuntimeExternalEventStorageDiagnostics> {
    let (
        event_rows,
        estimated_event_bytes,
        oldest_sequence,
        oldest_created_at,
        newest_sequence,
        newest_created_at,
    ) = conn
        .query_row(
            "SELECT COUNT(*), COALESCE(SUM(LENGTH(record_json)), 0),
                    MIN(sequence_id), MIN(created_at), MAX(sequence_id), MAX(created_at)
               FROM external_runtime_events",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            },
        )
        .map_err(|error| persistence_error("read external event storage diagnostics", error))?;
    let checkpoint_rows = conn
        .query_row(
            "SELECT COUNT(*) FROM external_runtime_event_checkpoints",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| persistence_error("count external event checkpoints", error))?;
    Ok(RuntimeExternalEventStorageDiagnostics {
        event_rows: event_rows.max(0) as u64,
        estimated_event_bytes: estimated_event_bytes.max(0) as u64,
        checkpoint_rows: checkpoint_rows.max(0) as u64,
        oldest_sequence: oldest_sequence.map(|value| value.max(0) as u64),
        oldest_created_at,
        newest_sequence: newest_sequence.map(|value| value.max(0) as u64),
        newest_created_at,
    })
}

impl CoordinationStore {
    pub fn open(engine_data_dir: impl AsRef<Path>) -> CoreResult<Self> {
        Self::open_with_diagnostics(engine_data_dir, None)
    }
    pub fn open_with_diagnostics(
        engine_data_dir: impl AsRef<Path>,
        filesystem_warning_free_percent: Option<u32>,
    ) -> CoreResult<Self> {
        fs::create_dir_all(engine_data_dir.as_ref())
            .map_err(|error| persistence_error("create coordination data directory", error))?;
        Self::open_file_with_diagnostics(
            engine_data_dir.as_ref().join(DB_FILE_NAME),
            filesystem_warning_free_percent,
        )
    }
    pub fn open_file(path: impl AsRef<Path>) -> CoreResult<Self> {
        Self::open_file_with_diagnostics(path, None)
    }
    pub fn open_file_with_diagnostics(
        path: impl AsRef<Path>,
        filesystem_warning_free_percent: Option<u32>,
    ) -> CoreResult<Self> {
        let conn = Connection::open(path.as_ref())
            .map_err(|error| persistence_error("open sqlite", error))?;
        configure_connection(&conn)?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
            database_path: Arc::new(path.as_ref().to_path_buf()),
            filesystem_warning_free_percent,
        };
        store.migrate()?;
        Ok(store)
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
        let external_runtime_events = runtime_external_event_storage_diagnostics(&conn)?;
        let filesystem_headroom = filesystem_headroom(
            Some(self.database_path.as_path()),
            "sqlite_database_path",
            self.filesystem_warning_free_percent,
        );
        let mut pressure_signals =
            sqlite_storage_pressure_signals(&size, &table_counts, &index_checks, search_healthy);
        if let Some(free_percent) = filesystem_headroom.free_percent {
            pressure_signals.push(RuntimeStoragePressureSignal {
                name: "filesystem_free_percent".to_string(),
                active: filesystem_headroom.warning_active,
                severity: if filesystem_headroom.warning_active {
                    "warning".to_string()
                } else {
                    "info".to_string()
                },
                observed_value: free_percent as u64,
                threshold_value: filesystem_headroom.warning_free_percent.map(u64::from),
                detail: filesystem_headroom.detail.clone(),
            });
        }
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
            external_runtime_events,
            filesystem_headroom,
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
        let mut external_runtime_event_retention = ExternalRuntimeEventRetentionReport::default();
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
            match (
                &policy.compact_terminal_external_runtime_events_before,
                &policy.external_runtime_event_retention_at,
                policy.external_runtime_event_terminal_turn_batch_size,
            ) {
                (None, None, None) => {}
                (Some(cutoff), Some(checkpointed_at), Some(batch_size)) => {
                    external_runtime_event_retention =
                        repos::external_runtime::compact_terminal_external_runtime_events_in_tx(
                            &tx,
                            cutoff,
                            checkpointed_at,
                            batch_size,
                        )?;
                }
                _ => {
                    return Err(CoreError::new(
                        CoreErrorKind::InvalidInput,
                        "external runtime event retention requires cutoff, retention timestamp, and terminal turn batch size together",
                    ));
                }
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
            external_runtime_event_retention,
            wal_checkpoint_ran: policy.run_wal_checkpoint,
            optimize_ran: policy.run_optimize,
        })
    }
    pub fn hot_query_plan_checks(&self) -> CoreResult<Vec<RuntimeQueryPlanCheck>> {
        let conn = self.conn()?;
        hot_query_plan_checks(&conn)
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
    pub(crate) fn conn(&self) -> CoreResult<std::sync::MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|_| CoreError::new(CoreErrorKind::InternalError, "sqlite lock poisoned"))
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
