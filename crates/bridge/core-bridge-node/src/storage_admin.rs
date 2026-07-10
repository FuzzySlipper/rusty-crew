use super::*;

impl NativeBridge {
    pub fn apply_curator_governance_write(
        &self,
        write: &CuratorGovernanceWrite,
    ) -> CoreResult<CuratorGovernanceWriteResult> {
        self.engine()?.apply_curator_governance_write(write)
    }

    pub fn get_curator_candidate(
        &self,
        candidate_id: &str,
    ) -> CoreResult<Option<CuratorCandidateRecord>> {
        self.engine()?.get_curator_candidate(candidate_id)
    }

    pub fn list_curator_candidates(
        &self,
        query: &CuratorCandidateQuery,
    ) -> CoreResult<ExactPage<CuratorCandidateRecord>> {
        self.engine()?.list_curator_candidates(query)
    }

    pub fn get_curator_mutation(
        &self,
        mutation_id: &str,
    ) -> CoreResult<Option<CuratorMutationRecord>> {
        self.engine()?.get_curator_mutation(mutation_id)
    }

    pub fn list_curator_mutations(
        &self,
        query: &CuratorMutationQuery,
    ) -> CoreResult<ExactPage<CuratorMutationRecord>> {
        self.engine()?.list_curator_mutations(query)
    }

    pub fn list_curator_audit_receipts(
        &self,
        query: &CuratorAuditQuery,
    ) -> CoreResult<ExactPage<CuratorAuditReceiptRecord>> {
        self.engine()?.list_curator_audit_receipts(query)
    }

    pub fn count_rows(&self, table: &str) -> CoreResult<u64> {
        self.engine()?.count_rows(table)
    }

    pub fn database_size(&self) -> CoreResult<RuntimeDatabaseSize> {
        self.engine()?.database_size()
    }

    pub fn storage_diagnostics(&self) -> CoreResult<RuntimeStorageDiagnostics> {
        self.engine()?.storage_diagnostics()
    }

    pub fn storage_schema(&self) -> CoreResult<RuntimeModuleSchemaRegistryDiagnostics> {
        self.engine()?.storage_schema()
    }

    pub fn run_maintenance(
        &self,
        policy: &RuntimeMaintenancePolicy,
    ) -> CoreResult<RuntimeMaintenanceReport> {
        self.engine()?.run_maintenance(policy)
    }

    pub fn search_runtime(
        &self,
        filter: &RuntimeSearchFilter,
    ) -> CoreResult<Vec<RuntimeSearchResult>> {
        self.engine()?.search_runtime(filter)
    }

    pub fn query_runtime_counters(
        &self,
        query: &RuntimeCounterQuery,
    ) -> CoreResult<Vec<RuntimeCounterRecord>> {
        self.engine()?.query_runtime_counters(query)
    }

    pub fn list_simple_kv(&self, query: &SimpleKvQuery) -> CoreResult<Vec<SimpleKvRecord>> {
        self.engine()?.list_simple_kv(query)
    }

    pub fn put_simple_kv(&self, write: &SimpleKvWrite) -> CoreResult<SimpleKvRecord> {
        self.engine()?.put_simple_kv(write)
    }

    pub fn delete_simple_kv(&self, delete: &SimpleKvDelete) -> CoreResult<SimpleKvRecord> {
        self.engine()?.delete_simple_kv(delete)
    }

    pub fn runtime_summary(&self, scope: &RuntimeCounterScope) -> CoreResult<RuntimeStateSummary> {
        self.engine()?.runtime_summary(scope)
    }

    pub fn reset_runtime_counters(&self, query: &RuntimeCounterQuery) -> CoreResult<u64> {
        self.engine()?.reset_runtime_counters(query)
    }
}

pub(crate) fn to_js_simple_kv_record(record: SimpleKvRecord) -> napi::Result<JsSimpleKvRecord> {
    Ok(JsSimpleKvRecord {
        scope_type: record.scope.scope_type,
        scope_id: record.scope.scope_id,
        key: record.key,
        value_json: serde_json::to_string(&record.value_json)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))?,
        revision: record.revision as f64,
        created_at: record.created_at,
        updated_at: record.updated_at,
        expires_at: record.expires_at,
    })
}

pub(crate) fn to_simple_kv_query(query: JsSimpleKvQuery) -> SimpleKvQuery {
    SimpleKvQuery {
        scope: SimpleKvScope {
            scope_type: query.scope_type,
            scope_id: query.scope_id,
        },
        key_prefix: query.key_prefix,
        include_expired: query.include_expired.unwrap_or(false),
        expired_only: query.expired_only.unwrap_or(false),
        now: query.now,
        page: Some(rusty_crew_core_persistence::QueryPage {
            limit: query.limit,
            offset: query.offset,
        }),
    }
}

pub(crate) fn to_simple_kv_write(write: JsSimpleKvWrite) -> napi::Result<SimpleKvWrite> {
    Ok(SimpleKvWrite {
        scope: SimpleKvScope {
            scope_type: write.scope_type,
            scope_id: write.scope_id,
        },
        key: write.key,
        value_json: serde_json::from_str(&write.value_json).map_err(|error| {
            napi::Error::new(
                napi::Status::InvalidArg,
                format!("invalid simple_kv value_json: {error}"),
            )
        })?,
        now: write.now,
        expires_at: write.expires_at,
    })
}

pub(crate) fn to_simple_kv_delete(delete: JsSimpleKvDelete) -> napi::Result<SimpleKvDelete> {
    if !delete.expected_revision.is_finite() || delete.expected_revision < 0.0 {
        return Err(napi::Error::new(
            napi::Status::InvalidArg,
            "simple_kv expected_revision must be a non-negative finite number",
        ));
    }
    Ok(SimpleKvDelete {
        scope: SimpleKvScope {
            scope_type: delete.scope_type,
            scope_id: delete.scope_id,
        },
        key: delete.key,
        expected_revision: delete.expected_revision as u64,
    })
}

pub(crate) fn to_runtime_search_filter(
    query: JsRuntimeSearchQuery,
) -> napi::Result<RuntimeSearchFilter> {
    Ok(RuntimeSearchFilter {
        query: query.query,
        row_type: query
            .row_type
            .as_deref()
            .map(parse_runtime_search_row_type)
            .transpose()?,
        session_id: query
            .session_id
            .map(rusty_crew_core_bridge_api::SessionId::new),
        agent_id: query.agent_id.map(rusty_crew_core_bridge_api::AgentId::new),
        instance_id: query
            .instance_id
            .map(rusty_crew_core_bridge_api::AgentInstanceId::new),
        task_id: query.task_id.map(rusty_crew_core_bridge_api::TaskId::new),
        event_kind: query
            .event_kind
            .as_deref()
            .map(parse_event_kind)
            .transpose()?,
        recorded_after: query.recorded_after,
        recorded_before: query.recorded_before,
        limit: query.limit,
    })
}

pub(crate) fn to_js_runtime_search_result(result: RuntimeSearchResult) -> JsRuntimeSearchResult {
    JsRuntimeSearchResult {
        row_type: runtime_search_row_type_as_str(result.row_type).to_string(),
        row_key: result.row_key,
        sequence: result.sequence.map(|sequence| sequence as f64),
        session_id: result.session_id.map(|value| value.0),
        agent_id: result.agent_id.map(|value| value.0),
        instance_id: result.instance_id.map(|value| value.0),
        task_id: result.task_id.map(|value| value.0),
        event_kind: result.event_kind.map(|kind| format!("{kind:?}")),
        recorded_at: result.recorded_at,
        title: result.title,
        body: result.body,
    }
}

pub(crate) fn to_runtime_counter_query(
    query: JsRuntimeCounterQuery,
) -> napi::Result<RuntimeCounterQuery> {
    Ok(RuntimeCounterQuery {
        scope: query
            .scope_type
            .as_deref()
            .map(|scope_type| to_runtime_counter_scope(scope_type, query.scope_id.clone()))
            .transpose()?,
        counter_name: query.counter_name,
        page: Some(rusty_crew_core_persistence::QueryPage {
            limit: query.limit,
            offset: query.offset,
        }),
    })
}

pub(crate) fn to_runtime_counter_scope(
    scope_type: &str,
    scope_id: Option<String>,
) -> napi::Result<RuntimeCounterScope> {
    match scope_type {
        "runtime" => Ok(RuntimeCounterScope::Runtime),
        "agent" => required_scope_id(scope_type, scope_id)
            .map(rusty_crew_core_bridge_api::AgentId::new)
            .map(RuntimeCounterScope::Agent),
        "instance" => required_scope_id(scope_type, scope_id)
            .map(rusty_crew_core_bridge_api::AgentInstanceId::new)
            .map(RuntimeCounterScope::Instance),
        "session" => required_scope_id(scope_type, scope_id)
            .map(rusty_crew_core_bridge_api::SessionId::new)
            .map(RuntimeCounterScope::Session),
        other => Err(napi::Error::new(
            napi::Status::InvalidArg,
            format!("unsupported runtime counter scope type {other}"),
        )),
    }
}

pub(crate) fn required_scope_id(
    scope_type: &str,
    scope_id: Option<String>,
) -> napi::Result<String> {
    scope_id
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            napi::Error::new(
                napi::Status::InvalidArg,
                format!("runtime counter scope {scope_type} requires scopeId"),
            )
        })
}

pub(crate) fn to_js_runtime_counter(record: RuntimeCounterRecord) -> JsRuntimeCounterRecord {
    let (scope_type, scope_id) = runtime_counter_scope_parts(record.scope);
    JsRuntimeCounterRecord {
        scope_type,
        scope_id,
        counter_name: record.counter_name,
        value: record.value as f64,
        updated_at: record.updated_at,
    }
}

pub(crate) fn to_js_runtime_counter_summary(
    summary: RuntimeStateSummary,
) -> JsRuntimeCounterSummary {
    let (scope_type, scope_id) = runtime_counter_scope_parts(summary.scope);
    JsRuntimeCounterSummary {
        scope_type,
        scope_id,
        brain_turns: summary.brain_turns as f64,
        wakes: summary.wakes as f64,
        tool_calls: summary.tool_calls as f64,
        tool_errors: summary.tool_errors as f64,
        delegations_created: summary.delegations_created as f64,
        delegations_completed: summary.delegations_completed as f64,
        delegations_failed: summary.delegations_failed as f64,
        delegations_timed_out: summary.delegations_timed_out as f64,
        delegations_cancelled: summary.delegations_cancelled as f64,
        messages: summary.messages as f64,
        completions: summary.completions as f64,
        queue_expirations: summary.queue_expirations as f64,
    }
}

pub(crate) fn to_js_runtime_database_size(size: RuntimeDatabaseSize) -> JsRuntimeDatabaseSize {
    JsRuntimeDatabaseSize {
        database_bytes: size.database_bytes as f64,
        page_count: size.page_count as f64,
        page_size_bytes: size.page_size_bytes as f64,
        freelist_pages: size.freelist_pages as f64,
        freelist_bytes: size.freelist_bytes as f64,
        wal_bytes: size.wal_bytes as f64,
    }
}

pub(crate) fn to_js_schema_migration_record(
    record: SchemaMigrationRecord,
) -> JsSchemaMigrationRecord {
    JsSchemaMigrationRecord {
        version: record.version as f64,
        description: record.description,
        applied_at: record.applied_at,
    }
}

pub(crate) fn to_js_runtime_storage_capability(
    capability: RuntimeStorageCapability,
) -> JsRuntimeStorageCapability {
    JsRuntimeStorageCapability {
        name: capability.name,
        supported: capability.supported,
        detail: capability.detail,
    }
}

pub(crate) fn to_js_runtime_repository_backend_requirement(
    requirement: RuntimeRepositoryBackendRequirement,
) -> JsRuntimeRepositoryBackendRequirement {
    JsRuntimeRepositoryBackendRequirement {
        capability: requirement.capability,
        required: requirement.required,
        detail: requirement.detail,
    }
}

pub(crate) fn to_js_runtime_repository_group_diagnostic(
    group: RuntimeRepositoryGroupDiagnostic,
) -> JsRuntimeRepositoryGroupDiagnostic {
    JsRuntimeRepositoryGroupDiagnostic {
        group_id: group.group_id,
        label: group.label,
        correctness_sensitive: group.correctness_sensitive,
        backend_requirements: group
            .backend_requirements
            .into_iter()
            .map(to_js_runtime_repository_backend_requirement)
            .collect(),
        notes: group.notes,
    }
}

pub(crate) fn to_js_runtime_module_capability_status(
    status: RuntimeModuleCapabilityStatus,
) -> JsRuntimeModuleCapabilityStatus {
    JsRuntimeModuleCapabilityStatus {
        capability: status.capability,
        required: status.required,
        supported: status.supported,
        backend_variant: status.backend_variant,
    }
}

pub(crate) fn to_js_runtime_module_logical_store_diagnostic(
    store: RuntimeModuleLogicalStoreDiagnostic,
) -> JsRuntimeModuleLogicalStoreDiagnostic {
    JsRuntimeModuleLogicalStoreDiagnostic {
        store_name: store.store_name,
        description: store.description,
    }
}

pub(crate) fn to_js_runtime_module_physical_table_diagnostic(
    table: RuntimeModulePhysicalTableDiagnostic,
) -> JsRuntimeModulePhysicalTableDiagnostic {
    JsRuntimeModulePhysicalTableDiagnostic {
        table_name: table.table_name,
        logical_store: table.logical_store,
        physical_table: table.physical_table,
        declaration: table.declaration,
    }
}

pub(crate) fn to_js_runtime_module_physical_index_diagnostic(
    index: RuntimeModulePhysicalIndexDiagnostic,
) -> JsRuntimeModulePhysicalIndexDiagnostic {
    JsRuntimeModulePhysicalIndexDiagnostic {
        table_name: index.table_name,
        purpose: index.purpose,
        physical_index: index.physical_index,
        columns: index.columns,
        unique: index.unique,
    }
}

pub(crate) fn to_js_runtime_module_retention_diagnostic(
    retention: RuntimeModuleRetentionDiagnostic,
) -> JsRuntimeModuleRetentionDiagnostic {
    JsRuntimeModuleRetentionDiagnostic {
        store_name: retention.store_name,
        policy: retention.policy,
        detail: retention.detail,
    }
}

pub(crate) fn to_js_runtime_module_named_diagnostic(
    contract: RuntimeModuleNamedDiagnostic,
) -> JsRuntimeModuleNamedDiagnostic {
    JsRuntimeModuleNamedDiagnostic {
        name: contract.name,
        description: contract.description,
    }
}

pub(crate) fn to_js_runtime_module_query_catalog_diagnostic(
    entry: RuntimeModuleQueryCatalogDiagnostic,
) -> JsRuntimeModuleQueryCatalogDiagnostic {
    JsRuntimeModuleQueryCatalogDiagnostic {
        query_id: entry.query_id,
        store_name: entry.store_name,
        description: entry.description,
        parameter_schema_id: entry.parameter_schema_id,
    }
}

pub(crate) fn to_js_runtime_module_transfer_hook_diagnostic(
    hook: RuntimeModuleTransferHookDiagnostic,
) -> JsRuntimeModuleTransferHookDiagnostic {
    JsRuntimeModuleTransferHookDiagnostic {
        hook_name: hook.hook_name,
        format_version: hook.format_version as f64,
    }
}

pub(crate) fn to_js_runtime_installed_module_schema_diagnostic(
    installed: RuntimeInstalledModuleSchemaDiagnostic,
) -> JsRuntimeInstalledModuleSchemaDiagnostic {
    JsRuntimeInstalledModuleSchemaDiagnostic {
        module_id: installed.module_id,
        installed_version: installed.installed_version as f64,
        descriptor_fingerprint: installed.descriptor_fingerprint,
        installed_at: installed.installed_at,
        updated_at: installed.updated_at,
    }
}

pub(crate) fn to_js_runtime_module_schema_diagnostic(
    module: RuntimeModuleSchemaDiagnostic,
) -> JsRuntimeModuleSchemaDiagnostic {
    JsRuntimeModuleSchemaDiagnostic {
        module_id: module.module_id,
        owner_crate: module.owner_crate,
        owner_module: module.owner_module,
        descriptor_version: module.descriptor_version as f64,
        installed_version: module.installed_version.map(|version| version as f64),
        migration_status: module.migration_status,
        descriptor_fingerprint: module.descriptor_fingerprint,
        installed_descriptor_fingerprint: module.installed_descriptor_fingerprint,
        installed_at: module.installed_at,
        updated_at: module.updated_at,
        capability_status: module
            .capability_status
            .into_iter()
            .map(to_js_runtime_module_capability_status)
            .collect(),
        logical_stores: module
            .logical_stores
            .into_iter()
            .map(to_js_runtime_module_logical_store_diagnostic)
            .collect(),
        physical_tables: module
            .physical_tables
            .into_iter()
            .map(to_js_runtime_module_physical_table_diagnostic)
            .collect(),
        physical_indexes: module
            .physical_indexes
            .into_iter()
            .map(to_js_runtime_module_physical_index_diagnostic)
            .collect(),
        retention: module
            .retention
            .into_iter()
            .map(to_js_runtime_module_retention_diagnostic)
            .collect(),
        repository_contracts: module
            .repository_contracts
            .into_iter()
            .map(to_js_runtime_module_named_diagnostic)
            .collect(),
        query_catalog_entries: module
            .query_catalog_entries
            .into_iter()
            .map(to_js_runtime_module_query_catalog_diagnostic)
            .collect(),
        export_hooks: module
            .export_hooks
            .into_iter()
            .map(to_js_runtime_module_transfer_hook_diagnostic)
            .collect(),
        import_hooks: module
            .import_hooks
            .into_iter()
            .map(to_js_runtime_module_transfer_hook_diagnostic)
            .collect(),
        migration_notes: module.migration_notes,
        degraded_reasons: module.degraded_reasons,
        blocked_reasons: module.blocked_reasons,
    }
}

pub(crate) fn to_js_runtime_module_schema_registry_diagnostics(
    diagnostics: RuntimeModuleSchemaRegistryDiagnostics,
) -> JsRuntimeModuleSchemaRegistryDiagnostics {
    JsRuntimeModuleSchemaRegistryDiagnostics {
        source: diagnostics.source,
        backend_capabilities: diagnostics.backend_capabilities,
        modules: diagnostics
            .modules
            .into_iter()
            .map(to_js_runtime_module_schema_diagnostic)
            .collect(),
        orphan_installed_modules: diagnostics
            .orphan_installed_modules
            .into_iter()
            .map(to_js_runtime_installed_module_schema_diagnostic)
            .collect(),
    }
}

pub(crate) fn to_js_runtime_storage_table_count(
    count: RuntimeStorageTableCount,
) -> JsRuntimeStorageTableCount {
    JsRuntimeStorageTableCount {
        table: count.table,
        rows: count.rows as f64,
    }
}

pub(crate) fn to_js_runtime_query_plan_check(
    check: rusty_crew_core_persistence::RuntimeQueryPlanCheck,
) -> JsRuntimeQueryPlanCheck {
    JsRuntimeQueryPlanCheck {
        name: check.name.to_string(),
        uses_index: check.uses_index,
        detail: check.detail,
    }
}

pub(crate) fn to_js_runtime_storage_pressure_signal(
    signal: RuntimeStoragePressureSignal,
) -> JsRuntimeStoragePressureSignal {
    JsRuntimeStoragePressureSignal {
        name: signal.name,
        active: signal.active,
        severity: signal.severity,
        observed_value: signal.observed_value as f64,
        threshold_value: signal.threshold_value.map(|value| value as f64),
        detail: signal.detail,
    }
}

pub(crate) fn to_js_runtime_storage_connection_health(
    health: RuntimeStorageConnectionHealth,
) -> JsRuntimeStorageConnectionHealth {
    JsRuntimeStorageConnectionHealth {
        backend: health.backend,
        status: health.status,
        max_connections: health.max_connections as f64,
        active_connections: health.active_connections as f64,
        idle_connections: health.idle_connections as f64,
        total_opened: health.total_opened as f64,
        checkout_count: health.checkout_count as f64,
        checkout_reuse_count: health.checkout_reuse_count as f64,
        reconnect_attempts: health.reconnect_attempts as f64,
        reconnect_successes: health.reconnect_successes as f64,
        closed_connections_discarded: health.closed_connections_discarded as f64,
        last_error: health.last_error,
    }
}

pub(crate) fn to_js_runtime_storage_diagnostics(
    diagnostics: RuntimeStorageDiagnostics,
) -> JsRuntimeStorageDiagnostics {
    JsRuntimeStorageDiagnostics {
        backend: diagnostics.backend,
        backend_label: diagnostics.backend_label,
        schema_version: diagnostics.schema_version as f64,
        supported_schema_version: diagnostics.supported_schema_version as f64,
        migrations: diagnostics
            .migrations
            .into_iter()
            .map(to_js_schema_migration_record)
            .collect(),
        size: to_js_runtime_database_size(diagnostics.size),
        table_counts: diagnostics
            .table_counts
            .into_iter()
            .map(to_js_runtime_storage_table_count)
            .collect(),
        capabilities: diagnostics
            .capabilities
            .into_iter()
            .map(to_js_runtime_storage_capability)
            .collect(),
        repository_groups: diagnostics
            .repository_groups
            .into_iter()
            .map(to_js_runtime_repository_group_diagnostic)
            .collect(),
        connection_health: to_js_runtime_storage_connection_health(diagnostics.connection_health),
        module_registry: to_js_runtime_module_schema_registry_diagnostics(
            diagnostics.module_registry,
        ),
        index_checks: diagnostics
            .index_checks
            .into_iter()
            .map(to_js_runtime_query_plan_check)
            .collect(),
        search_healthy: diagnostics.search_healthy,
        pressure_signals: diagnostics
            .pressure_signals
            .into_iter()
            .map(to_js_runtime_storage_pressure_signal)
            .collect(),
        pressure: diagnostics.pressure,
    }
}

pub(crate) fn to_js_runtime_maintenance_report(
    report: RuntimeMaintenanceReport,
) -> JsRuntimeMaintenanceReport {
    JsRuntimeMaintenanceReport {
        size_before: to_js_runtime_database_size(report.size_before),
        size_after: to_js_runtime_database_size(report.size_after),
        expired_queue_messages: report.expired_queue_messages as f64,
        purged_terminal_queue_messages: report.purged_terminal_queue_messages as f64,
        expired_provider_wire_states: report.expired_provider_wire_states as f64,
        session_memory_compaction: to_js_session_memory_compaction_report(
            report.session_memory_compaction,
        ),
        wal_checkpoint_ran: report.wal_checkpoint_ran,
        optimize_ran: report.optimize_ran,
    }
}

pub(crate) fn to_js_session_memory_compaction_report(
    report: SessionMemoryCompactionReport,
) -> JsSessionMemoryCompactionReport {
    JsSessionMemoryCompactionReport {
        enabled: report.enabled,
        scopes_inspected: report.scopes_inspected as f64,
        retention_pressure_scopes: report.retention_pressure_scopes as f64,
        scopes_compacted: report.scopes_compacted as f64,
        session_summaries_created: report.session_summaries_created as f64,
        branch_summaries_created: report.branch_summaries_created as f64,
        records_archived: report.records_archived as f64,
        records_superseded: report.records_superseded as f64,
        skipped_scopes: report.skipped_scopes as f64,
    }
}

pub(crate) fn runtime_counter_scope_parts(scope: RuntimeCounterScope) -> (String, String) {
    match scope {
        RuntimeCounterScope::Runtime => ("runtime".to_string(), "_global".to_string()),
        RuntimeCounterScope::Agent(agent_id) => ("agent".to_string(), agent_id.0),
        RuntimeCounterScope::Instance(instance_id) => ("instance".to_string(), instance_id.0),
        RuntimeCounterScope::Session(session_id) => ("session".to_string(), session_id.0),
    }
}

pub(crate) fn parse_runtime_search_row_type(raw: &str) -> napi::Result<RuntimeSearchRowType> {
    match raw {
        "message" => Ok(RuntimeSearchRowType::Message),
        "queue_message" => Ok(RuntimeSearchRowType::QueueMessage),
        "session" => Ok(RuntimeSearchRowType::Session),
        other => Err(napi::Error::new(
            napi::Status::InvalidArg,
            format!("unsupported runtime search row type {other}"),
        )),
    }
}

pub(crate) fn runtime_search_row_type_as_str(row_type: RuntimeSearchRowType) -> &'static str {
    match row_type {
        RuntimeSearchRowType::Message => "message",
        RuntimeSearchRowType::QueueMessage => "queue_message",
        RuntimeSearchRowType::Session => "session",
    }
}
