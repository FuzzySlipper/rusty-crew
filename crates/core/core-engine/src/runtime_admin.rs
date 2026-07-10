use super::*;

impl CoreEngine {
    pub fn count_rows(&self, table: &str) -> CoreResult<u64> {
        RuntimeStorageAdminStore::count_rows(&self.store, table)
    }

    pub fn database_size(&self) -> CoreResult<RuntimeDatabaseSize> {
        RuntimeStorageAdminStore::database_size(&self.store)
    }

    pub fn storage_diagnostics(&self) -> CoreResult<RuntimeStorageDiagnostics> {
        RuntimeStorageAdminStore::storage_diagnostics(&self.store)
    }

    pub fn storage_schema(&self) -> CoreResult<RuntimeModuleSchemaRegistryDiagnostics> {
        RuntimeStorageAdminStore::storage_schema(&self.store)
    }

    pub fn search_runtime(
        &self,
        filter: &RuntimeSearchFilter,
    ) -> CoreResult<Vec<RuntimeSearchResult>> {
        RuntimeStorageAdminStore::search_runtime(&self.store, filter)
    }

    pub fn query_runtime_counters(
        &self,
        query: &RuntimeCounterQuery,
    ) -> CoreResult<Vec<RuntimeCounterRecord>> {
        RuntimeStorageAdminStore::query_runtime_counters(&self.store, query)
    }

    pub fn runtime_summary(&self, scope: &RuntimeCounterScope) -> CoreResult<RuntimeStateSummary> {
        RuntimeStorageAdminStore::runtime_summary(&self.store, scope)
    }

    pub fn reset_runtime_counters(&self, query: &RuntimeCounterQuery) -> CoreResult<u64> {
        RuntimeStorageAdminStore::reset_runtime_counters(&self.store, query, self.now())
    }
}
