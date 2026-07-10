use super::*;

impl CoreEngine {
    pub fn list_simple_kv(&self, query: &SimpleKvQuery) -> CoreResult<Vec<SimpleKvRecord>> {
        RuntimeModuleDataStore::list_simple_kv(&self.store, query)
    }

    pub fn put_simple_kv(&self, write: &SimpleKvWrite) -> CoreResult<SimpleKvRecord> {
        RuntimeModuleDataStore::put_simple_kv(&self.store, write)
    }

    pub fn delete_simple_kv(&self, delete: &SimpleKvDelete) -> CoreResult<SimpleKvRecord> {
        RuntimeModuleDataStore::delete_simple_kv(&self.store, delete)
    }

    pub fn run_maintenance(
        &self,
        policy: &RuntimeMaintenancePolicy,
    ) -> CoreResult<RuntimeMaintenanceReport> {
        RuntimeStorageAdminStore::run_maintenance(&self.store, policy)
    }
}
