use rusty_crew_core_persistence::*;
use rusty_crew_core_protocol::{
    CoreResult, IsoTimestamp, ModelProviderCredentialLink, ModelProviderCredentialLinkResult,
    ModelProviderCredentialUnlink, ModelProviderQuery, ModelProviderRecord, ModelProviderWrite,
    ProfileId, ProfilePurgeReport, ProfileRegistryRecord, ProfileRegistryUpdate,
    ProfileRegistryWrite, ServiceCredentialDelete, ServiceCredentialQuery, ServiceCredentialRecord,
    ServiceCredentialWrite,
};

pub(crate) trait RuntimeStorageAdminStore {
    fn count_rows(&self, table: &str) -> CoreResult<u64>;
    fn database_size(&self) -> CoreResult<RuntimeDatabaseSize>;
    fn storage_diagnostics(&self) -> CoreResult<RuntimeStorageDiagnostics>;
    fn storage_schema(&self) -> CoreResult<RuntimeModuleSchemaRegistryDiagnostics>;
    fn run_maintenance(
        &self,
        policy: &RuntimeMaintenancePolicy,
    ) -> CoreResult<RuntimeMaintenanceReport>;
    fn search_runtime(&self, filter: &RuntimeSearchFilter) -> CoreResult<Vec<RuntimeSearchResult>>;
    fn query_runtime_counters(
        &self,
        query: &RuntimeCounterQuery,
    ) -> CoreResult<Vec<RuntimeCounterRecord>>;
    fn runtime_summary(&self, scope: &RuntimeCounterScope) -> CoreResult<RuntimeStateSummary>;
    fn reset_runtime_counters(
        &self,
        query: &RuntimeCounterQuery,
        now: IsoTimestamp,
    ) -> CoreResult<u64>;
}

pub(crate) trait RuntimeServiceDataStore {
    fn list_profile_registry_records(
        &self,
        query: &ProfileRegistryQuery,
    ) -> CoreResult<Vec<ProfileRegistryRecord>>;
    fn create_profile_registry_record(
        &self,
        write: &ProfileRegistryWrite,
    ) -> CoreResult<ProfileRegistryRecord>;
    fn update_profile_registry_record(
        &self,
        update: &ProfileRegistryUpdate,
    ) -> CoreResult<ProfileRegistryRecord>;
    fn get_profile_registry_record(
        &self,
        profile_id: &ProfileId,
    ) -> CoreResult<Option<ProfileRegistryRecord>>;
    fn purge_profile(&self, profile_id: &ProfileId) -> CoreResult<ProfilePurgeReport>;
    fn upsert_model_provider(&self, write: &ModelProviderWrite) -> CoreResult<ModelProviderRecord>;
    fn get_model_provider(&self, alias: &str) -> CoreResult<Option<ModelProviderRecord>>;
    fn get_model_provider_secret(&self, alias: &str) -> CoreResult<Option<String>>;
    fn upsert_service_credential(
        &self,
        write: &ServiceCredentialWrite,
    ) -> CoreResult<ServiceCredentialRecord>;
    fn get_service_credential(
        &self,
        credential_id: &str,
    ) -> CoreResult<Option<ServiceCredentialRecord>>;
    fn get_service_credential_secret(&self, credential_id: &str) -> CoreResult<Option<String>>;
    fn delete_service_credential(
        &self,
        delete: &ServiceCredentialDelete,
    ) -> CoreResult<ServiceCredentialRecord>;
    fn list_service_credentials(
        &self,
        query: &ServiceCredentialQuery,
    ) -> CoreResult<Vec<ServiceCredentialRecord>>;
    fn link_model_provider_credential(
        &self,
        link: &ModelProviderCredentialLink,
    ) -> CoreResult<ModelProviderCredentialLinkResult>;
    fn unlink_model_provider_credential(
        &self,
        unlink: &ModelProviderCredentialUnlink,
    ) -> CoreResult<ModelProviderRecord>;
    fn list_model_providers(
        &self,
        query: &ModelProviderQuery,
    ) -> CoreResult<Vec<ModelProviderRecord>>;
}

pub(crate) trait RuntimeModuleDataStore {
    fn list_simple_kv(&self, query: &SimpleKvQuery) -> CoreResult<Vec<SimpleKvRecord>>;
    fn put_simple_kv(&self, write: &SimpleKvWrite) -> CoreResult<SimpleKvRecord>;
    fn delete_simple_kv(&self, delete: &SimpleKvDelete) -> CoreResult<SimpleKvRecord>;
}

impl RuntimeStorageAdminStore for CoreCoordinationStore {
    fn count_rows(&self, table: &str) -> CoreResult<u64> {
        self.admin().count_rows(table)
    }

    fn database_size(&self) -> CoreResult<RuntimeDatabaseSize> {
        self.admin().database_size()
    }

    fn storage_diagnostics(&self) -> CoreResult<RuntimeStorageDiagnostics> {
        self.admin().storage_diagnostics()
    }

    fn storage_schema(&self) -> CoreResult<RuntimeModuleSchemaRegistryDiagnostics> {
        self.admin().storage_schema()
    }

    fn run_maintenance(
        &self,
        policy: &RuntimeMaintenancePolicy,
    ) -> CoreResult<RuntimeMaintenanceReport> {
        self.admin().run_maintenance(policy)
    }

    fn search_runtime(&self, filter: &RuntimeSearchFilter) -> CoreResult<Vec<RuntimeSearchResult>> {
        self.admin().search_runtime(filter)
    }

    fn query_runtime_counters(
        &self,
        query: &RuntimeCounterQuery,
    ) -> CoreResult<Vec<RuntimeCounterRecord>> {
        self.admin().query_runtime_counters(query)
    }

    fn runtime_summary(&self, scope: &RuntimeCounterScope) -> CoreResult<RuntimeStateSummary> {
        self.admin().runtime_summary(scope)
    }

    fn reset_runtime_counters(
        &self,
        query: &RuntimeCounterQuery,
        now: IsoTimestamp,
    ) -> CoreResult<u64> {
        self.admin().reset_runtime_counters(query, now)
    }
}

impl RuntimeServiceDataStore for CoreCoordinationStore {
    fn list_profile_registry_records(
        &self,
        query: &ProfileRegistryQuery,
    ) -> CoreResult<Vec<ProfileRegistryRecord>> {
        self.service_data().list_profile_registry_records(query)
    }

    fn create_profile_registry_record(
        &self,
        write: &ProfileRegistryWrite,
    ) -> CoreResult<ProfileRegistryRecord> {
        self.service_data().create_profile_registry_record(write)
    }

    fn update_profile_registry_record(
        &self,
        update: &ProfileRegistryUpdate,
    ) -> CoreResult<ProfileRegistryRecord> {
        self.service_data().update_profile_registry_record(update)
    }

    fn get_profile_registry_record(
        &self,
        profile_id: &ProfileId,
    ) -> CoreResult<Option<ProfileRegistryRecord>> {
        self.service_data().get_profile_registry_record(profile_id)
    }

    fn purge_profile(&self, profile_id: &ProfileId) -> CoreResult<ProfilePurgeReport> {
        self.service_data().purge_profile(profile_id)
    }

    fn upsert_model_provider(&self, write: &ModelProviderWrite) -> CoreResult<ModelProviderRecord> {
        self.service_data().upsert_model_provider(write)
    }

    fn get_model_provider(&self, alias: &str) -> CoreResult<Option<ModelProviderRecord>> {
        self.service_data().get_model_provider(alias)
    }

    fn get_model_provider_secret(&self, alias: &str) -> CoreResult<Option<String>> {
        self.service_data().get_model_provider_secret(alias)
    }

    fn upsert_service_credential(
        &self,
        write: &ServiceCredentialWrite,
    ) -> CoreResult<ServiceCredentialRecord> {
        self.service_data().upsert_service_credential(write)
    }

    fn get_service_credential(
        &self,
        credential_id: &str,
    ) -> CoreResult<Option<ServiceCredentialRecord>> {
        self.service_data().get_service_credential(credential_id)
    }

    fn get_service_credential_secret(&self, credential_id: &str) -> CoreResult<Option<String>> {
        self.service_data()
            .get_service_credential_secret(credential_id)
    }

    fn delete_service_credential(
        &self,
        delete: &ServiceCredentialDelete,
    ) -> CoreResult<ServiceCredentialRecord> {
        self.service_data().delete_service_credential(delete)
    }

    fn list_service_credentials(
        &self,
        query: &ServiceCredentialQuery,
    ) -> CoreResult<Vec<ServiceCredentialRecord>> {
        self.service_data().list_service_credentials(query)
    }

    fn link_model_provider_credential(
        &self,
        link: &ModelProviderCredentialLink,
    ) -> CoreResult<ModelProviderCredentialLinkResult> {
        self.service_data().link_model_provider_credential(link)
    }

    fn unlink_model_provider_credential(
        &self,
        unlink: &ModelProviderCredentialUnlink,
    ) -> CoreResult<ModelProviderRecord> {
        self.service_data().unlink_model_provider_credential(unlink)
    }

    fn list_model_providers(
        &self,
        query: &ModelProviderQuery,
    ) -> CoreResult<Vec<ModelProviderRecord>> {
        self.service_data().list_model_providers(query)
    }
}

impl RuntimeModuleDataStore for CoreCoordinationStore {
    fn list_simple_kv(&self, query: &SimpleKvQuery) -> CoreResult<Vec<SimpleKvRecord>> {
        self.module_data().list_simple_kv(query)
    }

    fn put_simple_kv(&self, write: &SimpleKvWrite) -> CoreResult<SimpleKvRecord> {
        self.module_data().put_simple_kv(write)
    }

    fn delete_simple_kv(&self, delete: &SimpleKvDelete) -> CoreResult<SimpleKvRecord> {
        self.module_data().delete_simple_kv(delete)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeModuleDataStore {
        records: Mutex<Vec<SimpleKvRecord>>,
    }

    impl RuntimeModuleDataStore for FakeModuleDataStore {
        fn list_simple_kv(&self, query: &SimpleKvQuery) -> CoreResult<Vec<SimpleKvRecord>> {
            Ok(self
                .records
                .lock()
                .unwrap()
                .iter()
                .filter(|record| record.scope == query.scope)
                .filter(|record| {
                    query
                        .key_prefix
                        .as_ref()
                        .is_none_or(|prefix| record.key.starts_with(prefix))
                })
                .cloned()
                .collect())
        }

        fn put_simple_kv(&self, write: &SimpleKvWrite) -> CoreResult<SimpleKvRecord> {
            let mut records = self.records.lock().unwrap();
            let revision = records
                .iter()
                .find(|record| record.scope == write.scope && record.key == write.key)
                .map_or(1, |record| record.revision + 1);
            records.retain(|record| !(record.scope == write.scope && record.key == write.key));
            let record = SimpleKvRecord {
                scope: write.scope.clone(),
                key: write.key.clone(),
                value_json: write.value_json.clone(),
                revision,
                created_at: write.now.clone(),
                updated_at: write.now.clone(),
                expires_at: write.expires_at.clone(),
            };
            records.push(record.clone());
            Ok(record)
        }

        fn delete_simple_kv(&self, delete: &SimpleKvDelete) -> CoreResult<SimpleKvRecord> {
            let mut records = self.records.lock().unwrap();
            let index = records
                .iter()
                .position(|record| record.scope == delete.scope && record.key == delete.key)
                .expect("fake delete expects existing record");
            Ok(records.remove(index))
        }
    }

    #[test]
    fn simple_kv_uses_fake_runtime_module_store() {
        let store = FakeModuleDataStore::default();
        let scope = SimpleKvScope {
            scope_type: "module".to_string(),
            scope_id: "roleplay".to_string(),
        };
        let write = SimpleKvWrite {
            scope: scope.clone(),
            key: "diagnostics.enabled".to_string(),
            value_json: json!(true),
            now: "2026-07-09T10:00:00Z".to_string(),
            expires_at: None,
        };

        let first = RuntimeModuleDataStore::put_simple_kv(&store, &write).unwrap();
        let second = RuntimeModuleDataStore::put_simple_kv(&store, &write).unwrap();
        let listed = RuntimeModuleDataStore::list_simple_kv(
            &store,
            &SimpleKvQuery {
                scope,
                key_prefix: Some("diagnostics".to_string()),
                include_expired: false,
                expired_only: false,
                now: None,
                page: None,
            },
        )
        .unwrap();

        assert_eq!(first.revision, 1);
        assert_eq!(second.revision, 2);
        assert_eq!(listed, vec![second]);
    }
}
