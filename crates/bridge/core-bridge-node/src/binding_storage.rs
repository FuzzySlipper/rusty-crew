use super::*;

#[napi_derive::napi]
impl NativeBridgeBinding {
    #[napi]
    pub fn count_rows(&self, table: String) -> napi::Result<f64> {
        let bridge = self.bridge()?;
        let count = bridge.count_rows(&table).map_err(to_napi_error)?;
        Ok(count as f64)
    }

    #[napi]
    pub fn database_size(&self) -> napi::Result<JsRuntimeDatabaseSize> {
        let bridge = self.bridge()?;
        let size = bridge.database_size().map_err(to_napi_error)?;
        Ok(to_js_runtime_database_size(size))
    }

    #[napi]
    pub fn storage_diagnostics(&self) -> napi::Result<JsRuntimeStorageDiagnostics> {
        let bridge = self.bridge()?;
        let diagnostics = bridge.storage_diagnostics().map_err(to_napi_error)?;
        Ok(to_js_runtime_storage_diagnostics(diagnostics))
    }

    #[napi]
    pub fn storage_schema(&self) -> napi::Result<JsRuntimeModuleSchemaRegistryDiagnostics> {
        let bridge = self.bridge()?;
        let diagnostics = bridge.storage_schema().map_err(to_napi_error)?;
        Ok(to_js_runtime_module_schema_registry_diagnostics(
            diagnostics,
        ))
    }

    #[napi]
    pub fn run_maintenance(
        &self,
        policy: JsRuntimeMaintenancePolicy,
    ) -> napi::Result<JsRuntimeMaintenanceReport> {
        let bridge = self.bridge()?;
        let report = bridge
            .run_maintenance(&RuntimeMaintenancePolicy {
                expire_queued_messages_at: policy.expire_queued_messages_at,
                purge_terminal_queued_messages_before: policy.purge_terminal_queued_messages_before,
                expire_provider_wire_states_at: policy.expire_provider_wire_states_at,
                compact_session_memory_at: policy.compact_session_memory_at,
                session_memory_max_active_records_per_scope: policy
                    .session_memory_max_active_records_per_scope,
                session_memory_archive_batch_size: policy.session_memory_archive_batch_size,
                compact_terminal_external_runtime_events_before: policy
                    .compact_terminal_external_runtime_events_before,
                external_runtime_event_retention_at: policy.external_runtime_event_retention_at,
                external_runtime_event_terminal_turn_batch_size: policy
                    .external_runtime_event_terminal_turn_batch_size,
                run_wal_checkpoint: policy.run_wal_checkpoint.unwrap_or(false),
                run_optimize: policy.run_optimize.unwrap_or(false),
            })
            .map_err(to_napi_error)?;
        Ok(to_js_runtime_maintenance_report(report))
    }

    #[napi]
    pub fn search_runtime(
        &self,
        query: JsRuntimeSearchQuery,
    ) -> napi::Result<Vec<JsRuntimeSearchResult>> {
        let bridge = self.bridge()?;
        let results = bridge
            .search_runtime(&to_runtime_search_filter(query)?)
            .map_err(to_napi_error)?;
        Ok(results
            .into_iter()
            .map(to_js_runtime_search_result)
            .collect())
    }

    #[napi]
    pub fn query_runtime_counters(
        &self,
        query: JsRuntimeCounterQuery,
    ) -> napi::Result<Vec<JsRuntimeCounterRecord>> {
        let bridge = self.bridge()?;
        let results = bridge
            .query_runtime_counters(&to_runtime_counter_query(query)?)
            .map_err(to_napi_error)?;
        Ok(results.into_iter().map(to_js_runtime_counter).collect())
    }

    #[napi]
    pub fn list_simple_kv(&self, query: JsSimpleKvQuery) -> napi::Result<Vec<JsSimpleKvRecord>> {
        let bridge = self.bridge()?;
        let records = bridge
            .list_simple_kv(&to_simple_kv_query(query))
            .map_err(to_napi_error)?;
        records.into_iter().map(to_js_simple_kv_record).collect()
    }

    #[napi]
    pub fn put_simple_kv(&self, write: JsSimpleKvWrite) -> napi::Result<JsSimpleKvRecord> {
        let bridge = self.bridge()?;
        let record = bridge
            .put_simple_kv(&to_simple_kv_write(write)?)
            .map_err(to_napi_error)?;
        to_js_simple_kv_record(record)
    }

    #[napi]
    pub fn delete_simple_kv(&self, delete: JsSimpleKvDelete) -> napi::Result<JsSimpleKvRecord> {
        let bridge = self.bridge()?;
        let record = bridge
            .delete_simple_kv(&to_simple_kv_delete(delete)?)
            .map_err(to_napi_error)?;
        to_js_simple_kv_record(record)
    }

    #[napi]
    pub fn runtime_summary(
        &self,
        scope_type: String,
        scope_id: Option<String>,
    ) -> napi::Result<JsRuntimeCounterSummary> {
        let bridge = self.bridge()?;
        let summary = bridge
            .runtime_summary(&to_runtime_counter_scope(&scope_type, scope_id)?)
            .map_err(to_napi_error)?;
        Ok(to_js_runtime_counter_summary(summary))
    }

    #[napi]
    pub fn reset_runtime_counters(&self, query: JsRuntimeCounterQuery) -> napi::Result<f64> {
        let bridge = self.bridge()?;
        let reset = bridge
            .reset_runtime_counters(&to_runtime_counter_query(query)?)
            .map_err(to_napi_error)?;
        Ok(reset as f64)
    }
}
