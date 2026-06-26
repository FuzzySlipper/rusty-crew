//! Rust-owned module schema descriptor types.
//!
//! These descriptors are intentionally storage-boundary data. TypeScript may
//! call repository/query APIs built from them later, but it must not register
//! physical schema directly.

use rusty_crew_core_protocol::{CoreError, CoreErrorKind, CoreResult};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::{HashMap, HashSet};
use std::fmt;

const MAX_IDENTIFIER_BYTES: usize = 48;
const MAX_GENERATED_NAME_BYTES: usize = 120;

const RESERVED_IDENTIFIERS: &[&str] = &[
    "agents",
    "agent_instances",
    "agent_messages",
    "attachments",
    "completion_packets",
    "conversation_branches",
    "conversation_snapshots",
    "event_history",
    "messages",
    "message_blocks",
    "message_slots",
    "message_variants",
    "mcp_bindings",
    "profile_memories",
    "provider_wire_states",
    "queued_messages",
    "runtime_counters",
    "runtime_import_batches",
    "runtime_search_fts",
    "schema_migrations",
    "scheduled_jobs",
    "scheduled_job_runs",
    "session_configs",
    "session_identity",
    "sessions",
    "sqlite_sequence",
    "tool_call_history",
    "worker_runs",
];

const SQL_KEYWORDS: &[&str] = &[
    "alter", "and", "as", "by", "create", "delete", "drop", "from", "group", "index", "insert",
    "join", "not", "null", "or", "pragma", "select", "table", "trigger", "update", "where",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleSchemaCapability {
    AdvisoryLocks,
    CaseInsensitiveSearch,
    ConcurrentWriters,
    FullTextSearch,
    GeneratedColumns,
    JsonDocuments,
    OnlineIndexBuild,
    Transactions,
}

impl ModuleSchemaCapability {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AdvisoryLocks => "advisory_locks",
            Self::CaseInsensitiveSearch => "case_insensitive_search",
            Self::ConcurrentWriters => "concurrent_writers",
            Self::FullTextSearch => "full_text_search",
            Self::GeneratedColumns => "generated_columns",
            Self::JsonDocuments => "json_documents",
            Self::OnlineIndexBuild => "online_index_build",
            Self::Transactions => "transactions",
        }
    }

    pub fn parse(raw: &str) -> CoreResult<Self> {
        match raw {
            "advisory_locks" => Ok(Self::AdvisoryLocks),
            "case_insensitive_search" => Ok(Self::CaseInsensitiveSearch),
            "concurrent_writers" => Ok(Self::ConcurrentWriters),
            "full_text_search" => Ok(Self::FullTextSearch),
            "generated_columns" => Ok(Self::GeneratedColumns),
            "json_documents" => Ok(Self::JsonDocuments),
            "online_index_build" => Ok(Self::OnlineIndexBuild),
            "transactions" => Ok(Self::Transactions),
            _ => Err(invalid_descriptor(format!(
                "unknown module schema capability {raw:?}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleCapabilityRequirementKind {
    Required,
    Optional,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleCapabilityRequirement {
    pub capability: ModuleSchemaCapability,
    pub kind: ModuleCapabilityRequirementKind,
    pub backend_variant: Option<String>,
}

impl ModuleCapabilityRequirement {
    pub fn required(capability: ModuleSchemaCapability) -> Self {
        Self {
            capability,
            kind: ModuleCapabilityRequirementKind::Required,
            backend_variant: None,
        }
    }

    pub fn optional(capability: ModuleSchemaCapability) -> Self {
        Self {
            capability,
            kind: ModuleCapabilityRequirementKind::Optional,
            backend_variant: None,
        }
    }

    pub fn validate(&self) -> CoreResult<()> {
        if let Some(variant) = &self.backend_variant {
            validate_schema_identifier(variant, IdentifierKind::BackendVariant)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleSchemaBundle {
    pub module_id: ModuleId,
    pub schema_version: u32,
    pub owner: ModuleOwner,
    pub logical_stores: Vec<LogicalStoreDescriptor>,
    pub tables: Vec<ModuleTableDescriptor>,
    pub indexes: Vec<ModuleIndexDescriptor>,
    pub retention: Vec<ModuleRetentionDeclaration>,
    pub capability_requirements: Vec<ModuleCapabilityRequirement>,
    pub repository_contracts: Vec<RepositoryContractDescriptor>,
    pub query_catalog_entries: Vec<QueryCatalogEntryDescriptor>,
    pub export_hooks: Vec<ModuleTransferHookDescriptor>,
    pub import_hooks: Vec<ModuleTransferHookDescriptor>,
    pub migration_notes: Vec<String>,
}

impl ModuleSchemaBundle {
    pub fn validate(&self) -> CoreResult<ValidatedModuleSchemaBundle> {
        validate_schema_version(self.schema_version)?;
        self.module_id.validate()?;
        self.owner.validate()?;

        require_non_empty(
            "module schema bundle",
            "logical_stores",
            self.logical_stores.len(),
        )?;
        require_non_empty("module schema bundle", "tables", self.tables.len())?;

        let mut logical_store_names = Vec::new();
        for store in &self.logical_stores {
            store.validate()?;
            push_unique(
                "logical store",
                store.store_name.as_str(),
                &mut logical_store_names,
            )?;
        }

        let mut table_names = Vec::new();
        let mut physical_tables = Vec::new();
        for table in &self.tables {
            table.validate()?;
            push_unique("table", table.table_name.as_str(), &mut table_names)?;
            let physical_name = generated_module_table_name(&self.module_id, &table.table_name)?;
            push_unique(
                "physical table",
                physical_name.as_str(),
                &mut physical_tables,
            )?;
        }

        let mut index_purposes = Vec::new();
        let mut physical_indexes = Vec::new();
        for index in &self.indexes {
            index.validate(&table_names)?;
            let key = format!("{}:{}", index.table_name, index.purpose);
            push_unique("index purpose", key.as_str(), &mut index_purposes)?;
            let physical_name =
                generated_module_index_name(&self.module_id, &index.table_name, &index.purpose)?;
            push_unique(
                "physical index",
                physical_name.as_str(),
                &mut physical_indexes,
            )?;
        }

        for retention in &self.retention {
            retention.validate(&logical_store_names)?;
        }
        for requirement in &self.capability_requirements {
            requirement.validate()?;
        }
        for contract in &self.repository_contracts {
            contract.validate()?;
        }
        for entry in &self.query_catalog_entries {
            entry.validate(&logical_store_names)?;
        }
        for hook in &self.export_hooks {
            hook.validate()?;
        }
        for hook in &self.import_hooks {
            hook.validate()?;
        }

        Ok(ValidatedModuleSchemaBundle {
            module_id: self.module_id.clone(),
            schema_version: self.schema_version,
            physical_tables,
            physical_indexes,
        })
    }

    pub fn descriptor_fingerprint(&self) -> CoreResult<String> {
        let value = serde_json::to_value(self).map_err(descriptor_fingerprint_error)?;
        let canonical = canonical_json(value);
        Ok(format!("fnv1a64:{:016x}", fnv1a64(canonical.as_bytes())))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedModuleSchemaBundle {
    pub module_id: ModuleId,
    pub schema_version: u32,
    pub physical_tables: Vec<String>,
    pub physical_indexes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleSchemaRegistry {
    bundles: Vec<ModuleSchemaBundle>,
}

impl ModuleSchemaRegistry {
    pub fn new(bundles: Vec<ModuleSchemaBundle>) -> CoreResult<Self> {
        let registry = Self { bundles };
        registry.validate()?;
        Ok(registry)
    }

    pub fn empty() -> Self {
        Self {
            bundles: Vec::new(),
        }
    }

    pub fn bundles(&self) -> &[ModuleSchemaBundle] {
        self.bundles.as_slice()
    }

    pub fn validate(&self) -> CoreResult<Vec<ValidatedModuleSchemaBundle>> {
        let mut module_ids = Vec::new();
        self.bundles
            .iter()
            .map(|bundle| {
                let validated = bundle.validate()?;
                push_unique(
                    "module schema bundle",
                    bundle.module_id.as_str(),
                    &mut module_ids,
                )?;
                Ok(validated)
            })
            .collect()
    }

    pub fn validate_capabilities(
        &self,
        supported_capabilities: &[ModuleSchemaCapability],
    ) -> CoreResult<()> {
        for bundle in &self.bundles {
            for requirement in &bundle.capability_requirements {
                requirement.validate()?;
                if requirement.kind == ModuleCapabilityRequirementKind::Required
                    && !supported_capabilities.contains(&requirement.capability)
                {
                    return Err(invalid_descriptor(format!(
                        "module {} requires unsupported storage capability {}",
                        bundle.module_id,
                        requirement.capability.as_str()
                    )));
                }
            }
        }
        Ok(())
    }
}

pub fn compiled_module_schema_registry() -> ModuleSchemaRegistry {
    ModuleSchemaRegistry::new(vec![simple_kv_schema_bundle()])
        .expect("compiled module schema registry must be valid")
}

pub fn simple_kv_schema_bundle() -> ModuleSchemaBundle {
    ModuleSchemaBundle {
        module_id: ModuleId::new("simple_kv").expect("valid simple_kv module id"),
        schema_version: 1,
        owner: ModuleOwner {
            crate_name: "core_persistence".to_string(),
            rust_module: "simple_kv".to_string(),
        },
        logical_stores: vec![LogicalStoreDescriptor {
            store_name: StoreName::new("entries").expect("valid simple_kv store name"),
            description: "Simple scoped key/value records".to_string(),
        }],
        tables: vec![ModuleTableDescriptor {
            table_name: TableName::new("entries").expect("valid simple_kv table name"),
            logical_store: StoreName::new("entries").expect("valid simple_kv store name"),
            declaration: TableDeclaration::Owned,
        }],
        indexes: vec![
            ModuleIndexDescriptor {
                table_name: TableName::new("entries").expect("valid simple_kv table name"),
                purpose: IndexPurpose::new("scope_key").expect("valid simple_kv index purpose"),
                columns: vec![
                    "scope_type".to_string(),
                    "scope_id".to_string(),
                    "entry_key".to_string(),
                ],
                unique: true,
            },
            ModuleIndexDescriptor {
                table_name: TableName::new("entries").expect("valid simple_kv table name"),
                purpose: IndexPurpose::new("expires_at").expect("valid simple_kv index purpose"),
                columns: vec!["expires_at".to_string()],
                unique: false,
            },
        ],
        retention: vec![ModuleRetentionDeclaration::PurgeExpired {
            store_name: StoreName::new("entries").expect("valid simple_kv store name"),
            timestamp_column: "expires_at".to_string(),
        }],
        capability_requirements: vec![
            ModuleCapabilityRequirement::required(ModuleSchemaCapability::Transactions),
            ModuleCapabilityRequirement::optional(ModuleSchemaCapability::JsonDocuments),
        ],
        repository_contracts: vec![
            RepositoryContractDescriptor {
                contract_name: "get_kv".to_string(),
                description: "Get a key/value entry".to_string(),
            },
            RepositoryContractDescriptor {
                contract_name: "list_kv".to_string(),
                description: "List key/value entries by scope".to_string(),
            },
            RepositoryContractDescriptor {
                contract_name: "put_kv".to_string(),
                description: "Create or replace a key/value entry".to_string(),
            },
            RepositoryContractDescriptor {
                contract_name: "compare_and_swap_kv".to_string(),
                description: "Replace a key/value entry when its revision matches".to_string(),
            },
            RepositoryContractDescriptor {
                contract_name: "delete_kv".to_string(),
                description: "Delete a key/value entry when its revision matches".to_string(),
            },
            RepositoryContractDescriptor {
                contract_name: "expire_kv".to_string(),
                description: "Remove expired key/value entries".to_string(),
            },
        ],
        query_catalog_entries: vec![QueryCatalogEntryDescriptor {
            query_id: "list_entries_by_scope".to_string(),
            store_name: StoreName::new("entries").expect("valid simple_kv store name"),
            description: "List simple key/value entries for a scope".to_string(),
            parameter_schema_id: Some("simple_kv_scope_query".to_string()),
        }],
        export_hooks: vec![ModuleTransferHookDescriptor {
            hook_name: "export_simple_kv".to_string(),
            format_version: 1,
        }],
        import_hooks: vec![ModuleTransferHookDescriptor {
            hook_name: "import_simple_kv".to_string(),
            format_version: 1,
        }],
        migration_notes: vec!["initial simple_kv module schema".to_string()],
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledModuleSchemaRecord {
    pub module_id: ModuleId,
    pub installed_version: u32,
    pub descriptor_fingerprint: String,
    pub installed_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeModuleSchemaRegistryDiagnostics {
    pub source: String,
    pub backend_capabilities: Vec<String>,
    pub modules: Vec<RuntimeModuleSchemaDiagnostic>,
    pub orphan_installed_modules: Vec<RuntimeInstalledModuleSchemaDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeModuleSchemaDiagnostic {
    pub module_id: String,
    pub owner_crate: String,
    pub owner_module: String,
    pub descriptor_version: u32,
    pub installed_version: Option<u32>,
    pub migration_status: String,
    pub descriptor_fingerprint: String,
    pub installed_descriptor_fingerprint: Option<String>,
    pub installed_at: Option<String>,
    pub updated_at: Option<String>,
    pub capability_status: Vec<RuntimeModuleCapabilityStatus>,
    pub logical_stores: Vec<RuntimeModuleLogicalStoreDiagnostic>,
    pub physical_tables: Vec<RuntimeModulePhysicalTableDiagnostic>,
    pub physical_indexes: Vec<RuntimeModulePhysicalIndexDiagnostic>,
    pub retention: Vec<RuntimeModuleRetentionDiagnostic>,
    pub repository_contracts: Vec<RuntimeModuleNamedDiagnostic>,
    pub query_catalog_entries: Vec<RuntimeModuleQueryCatalogDiagnostic>,
    pub export_hooks: Vec<RuntimeModuleTransferHookDiagnostic>,
    pub import_hooks: Vec<RuntimeModuleTransferHookDiagnostic>,
    pub migration_notes: Vec<String>,
    pub degraded_reasons: Vec<String>,
    pub blocked_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeModuleCapabilityStatus {
    pub capability: String,
    pub required: bool,
    pub supported: bool,
    pub backend_variant: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeModuleLogicalStoreDiagnostic {
    pub store_name: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeModulePhysicalTableDiagnostic {
    pub table_name: String,
    pub logical_store: String,
    pub physical_table: String,
    pub declaration: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeModulePhysicalIndexDiagnostic {
    pub table_name: String,
    pub purpose: String,
    pub physical_index: String,
    pub columns: Vec<String>,
    pub unique: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeModuleRetentionDiagnostic {
    pub store_name: String,
    pub policy: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeModuleNamedDiagnostic {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeModuleQueryCatalogDiagnostic {
    pub query_id: String,
    pub store_name: String,
    pub description: String,
    pub parameter_schema_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeModuleTransferHookDiagnostic {
    pub hook_name: String,
    pub format_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeInstalledModuleSchemaDiagnostic {
    pub module_id: String,
    pub installed_version: u32,
    pub descriptor_fingerprint: String,
    pub installed_at: String,
    pub updated_at: String,
}

pub fn module_schema_registry_diagnostics(
    registry: &ModuleSchemaRegistry,
    installed: &[InstalledModuleSchemaRecord],
    supported_capabilities: &[ModuleSchemaCapability],
) -> CoreResult<RuntimeModuleSchemaRegistryDiagnostics> {
    let supported = supported_capabilities
        .iter()
        .map(|capability| capability.as_str().to_string())
        .collect::<HashSet<_>>();
    let installed_by_module = installed
        .iter()
        .map(|record| (record.module_id.as_str().to_string(), record))
        .collect::<HashMap<_, _>>();
    let mut registered_module_ids = HashSet::new();

    let mut modules = Vec::new();
    for bundle in registry.bundles() {
        let validated = bundle.validate()?;
        let module_id = bundle.module_id.as_str().to_string();
        registered_module_ids.insert(module_id.clone());
        let descriptor_fingerprint = bundle.descriptor_fingerprint()?;
        let installed_record = installed_by_module.get(&module_id).copied();
        let mut degraded_reasons = Vec::new();
        let mut blocked_reasons = Vec::new();
        let capability_status = bundle
            .capability_requirements
            .iter()
            .map(|requirement| {
                let capability = requirement.capability.as_str().to_string();
                let is_supported = supported.contains(&capability);
                if !is_supported {
                    match requirement.kind {
                        ModuleCapabilityRequirementKind::Required => blocked_reasons.push(format!(
                            "required capability {capability} is not supported by this backend"
                        )),
                        ModuleCapabilityRequirementKind::Optional => {
                            degraded_reasons.push(format!(
                                "optional capability {capability} is not supported by this backend"
                            ))
                        }
                    }
                }
                RuntimeModuleCapabilityStatus {
                    capability,
                    required: requirement.kind == ModuleCapabilityRequirementKind::Required,
                    supported: is_supported,
                    backend_variant: requirement.backend_variant.clone(),
                }
            })
            .collect::<Vec<_>>();

        let migration_status = match installed_record {
            None if blocked_reasons.is_empty() => "not_installed",
            None => "blocked",
            Some(record) if record.installed_version > bundle.schema_version => {
                blocked_reasons.push(format!(
                    "installed version {} is newer than descriptor version {}",
                    record.installed_version, bundle.schema_version
                ));
                "blocked"
            }
            Some(record)
                if record.installed_version == bundle.schema_version
                    && record.descriptor_fingerprint != descriptor_fingerprint =>
            {
                blocked_reasons.push(
                    "installed descriptor fingerprint differs without a version change".to_string(),
                );
                "blocked"
            }
            Some(record) if record.installed_version < bundle.schema_version => "upgrade_available",
            Some(_) if !blocked_reasons.is_empty() => "blocked",
            Some(_) if !degraded_reasons.is_empty() => "degraded",
            Some(_) => "installed",
        }
        .to_string();

        modules.push(RuntimeModuleSchemaDiagnostic {
            module_id,
            owner_crate: bundle.owner.crate_name.clone(),
            owner_module: bundle.owner.rust_module.clone(),
            descriptor_version: validated.schema_version,
            installed_version: installed_record.map(|record| record.installed_version),
            migration_status,
            descriptor_fingerprint,
            installed_descriptor_fingerprint: installed_record
                .map(|record| record.descriptor_fingerprint.clone()),
            installed_at: installed_record.map(|record| record.installed_at.clone()),
            updated_at: installed_record.map(|record| record.updated_at.clone()),
            capability_status,
            logical_stores: bundle
                .logical_stores
                .iter()
                .map(|store| RuntimeModuleLogicalStoreDiagnostic {
                    store_name: store.store_name.as_str().to_string(),
                    description: store.description.clone(),
                })
                .collect(),
            physical_tables: bundle
                .tables
                .iter()
                .map(|table| {
                    Ok(RuntimeModulePhysicalTableDiagnostic {
                        table_name: table.table_name.as_str().to_string(),
                        logical_store: table.logical_store.as_str().to_string(),
                        physical_table: table.physical_name(&bundle.module_id)?,
                        declaration: table.declaration.diagnostic_label().to_string(),
                    })
                })
                .collect::<CoreResult<Vec<_>>>()?,
            physical_indexes: bundle
                .indexes
                .iter()
                .map(|index| {
                    Ok(RuntimeModulePhysicalIndexDiagnostic {
                        table_name: index.table_name.as_str().to_string(),
                        purpose: index.purpose.as_str().to_string(),
                        physical_index: index.physical_name(&bundle.module_id)?,
                        columns: index.columns.clone(),
                        unique: index.unique,
                    })
                })
                .collect::<CoreResult<Vec<_>>>()?,
            retention: bundle
                .retention
                .iter()
                .map(ModuleRetentionDeclaration::diagnostic)
                .collect(),
            repository_contracts: bundle
                .repository_contracts
                .iter()
                .map(|contract| RuntimeModuleNamedDiagnostic {
                    name: contract.contract_name.clone(),
                    description: contract.description.clone(),
                })
                .collect(),
            query_catalog_entries: bundle
                .query_catalog_entries
                .iter()
                .map(|entry| RuntimeModuleQueryCatalogDiagnostic {
                    query_id: entry.query_id.clone(),
                    store_name: entry.store_name.as_str().to_string(),
                    description: entry.description.clone(),
                    parameter_schema_id: entry.parameter_schema_id.clone(),
                })
                .collect(),
            export_hooks: bundle
                .export_hooks
                .iter()
                .map(|hook| RuntimeModuleTransferHookDiagnostic {
                    hook_name: hook.hook_name.clone(),
                    format_version: hook.format_version,
                })
                .collect(),
            import_hooks: bundle
                .import_hooks
                .iter()
                .map(|hook| RuntimeModuleTransferHookDiagnostic {
                    hook_name: hook.hook_name.clone(),
                    format_version: hook.format_version,
                })
                .collect(),
            migration_notes: bundle.migration_notes.clone(),
            degraded_reasons,
            blocked_reasons,
        });
    }

    let orphan_installed_modules = installed
        .iter()
        .filter(|record| !registered_module_ids.contains(record.module_id.as_str()))
        .map(|record| RuntimeInstalledModuleSchemaDiagnostic {
            module_id: record.module_id.as_str().to_string(),
            installed_version: record.installed_version,
            descriptor_fingerprint: record.descriptor_fingerprint.clone(),
            installed_at: record.installed_at.clone(),
            updated_at: record.updated_at.clone(),
        })
        .collect();

    Ok(RuntimeModuleSchemaRegistryDiagnostics {
        source: "compiled_module_schema_registry".to_string(),
        backend_capabilities: supported_capabilities
            .iter()
            .map(|capability| capability.as_str().to_string())
            .collect(),
        modules,
        orphan_installed_modules,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleOwner {
    pub crate_name: String,
    pub rust_module: String,
}

impl ModuleOwner {
    pub fn validate(&self) -> CoreResult<()> {
        validate_schema_identifier(&self.crate_name, IdentifierKind::OwnerCrate)?;
        validate_schema_identifier(&self.rust_module, IdentifierKind::OwnerModule)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModuleId(String);

impl ModuleId {
    pub fn new(value: impl Into<String>) -> CoreResult<Self> {
        let value = value.into();
        validate_schema_identifier(&value, IdentifierKind::ModuleId)?;
        Ok(Self(value))
    }

    pub fn unchecked_for_test(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    fn validate(&self) -> CoreResult<()> {
        validate_schema_identifier(self.as_str(), IdentifierKind::ModuleId)
    }
}

impl fmt::Display for ModuleId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StoreName(String);

impl StoreName {
    pub fn new(value: impl Into<String>) -> CoreResult<Self> {
        let value = value.into();
        validate_schema_identifier(&value, IdentifierKind::StoreName)?;
        Ok(Self(value))
    }

    pub fn unchecked_for_test(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for StoreName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TableName(String);

impl TableName {
    pub fn new(value: impl Into<String>) -> CoreResult<Self> {
        let value = value.into();
        validate_schema_identifier(&value, IdentifierKind::TableName)?;
        Ok(Self(value))
    }

    pub fn unchecked_for_test(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for TableName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IndexPurpose(String);

impl IndexPurpose {
    pub fn new(value: impl Into<String>) -> CoreResult<Self> {
        let value = value.into();
        validate_schema_identifier(&value, IdentifierKind::IndexPurpose)?;
        Ok(Self(value))
    }

    pub fn unchecked_for_test(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for IndexPurpose {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogicalStoreDescriptor {
    pub store_name: StoreName,
    pub description: String,
}

impl LogicalStoreDescriptor {
    pub fn validate(&self) -> CoreResult<()> {
        validate_schema_identifier(self.store_name.as_str(), IdentifierKind::StoreName)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleTableDescriptor {
    pub table_name: TableName,
    pub logical_store: StoreName,
    pub declaration: TableDeclaration,
}

impl ModuleTableDescriptor {
    pub fn validate(&self) -> CoreResult<()> {
        validate_schema_identifier(self.table_name.as_str(), IdentifierKind::TableName)?;
        validate_schema_identifier(self.logical_store.as_str(), IdentifierKind::StoreName)?;
        self.declaration.validate()
    }

    pub fn physical_name(&self, module_id: &ModuleId) -> CoreResult<String> {
        generated_module_table_name(module_id, &self.table_name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TableDeclaration {
    Owned,
    MigrationFragment { fragment_id: String },
}

impl TableDeclaration {
    pub fn validate(&self) -> CoreResult<()> {
        match self {
            Self::Owned => Ok(()),
            Self::MigrationFragment { fragment_id } => {
                validate_schema_identifier(fragment_id, IdentifierKind::MigrationFragment)
            }
        }
    }

    fn diagnostic_label(&self) -> &'static str {
        match self {
            Self::Owned => "owned",
            Self::MigrationFragment { .. } => "migration_fragment",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleIndexDescriptor {
    pub table_name: TableName,
    pub purpose: IndexPurpose,
    pub columns: Vec<String>,
    pub unique: bool,
}

impl ModuleIndexDescriptor {
    pub fn validate(&self, known_tables: &[String]) -> CoreResult<()> {
        validate_schema_identifier(self.table_name.as_str(), IdentifierKind::TableName)?;
        validate_schema_identifier(self.purpose.as_str(), IdentifierKind::IndexPurpose)?;
        if !known_tables
            .iter()
            .any(|table| table == self.table_name.as_str())
        {
            return Err(invalid_descriptor(format!(
                "index {} references unknown module table {}",
                self.purpose, self.table_name
            )));
        }
        require_non_empty("module index", "columns", self.columns.len())?;
        for column in &self.columns {
            validate_schema_identifier(column, IdentifierKind::ColumnName)?;
        }
        Ok(())
    }

    pub fn physical_name(&self, module_id: &ModuleId) -> CoreResult<String> {
        generated_module_index_name(module_id, &self.table_name, &self.purpose)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleRetentionDeclaration {
    ManualOnly {
        store_name: StoreName,
    },
    PurgeExpired {
        store_name: StoreName,
        timestamp_column: String,
    },
    CompactAfter {
        store_name: StoreName,
        policy_id: String,
    },
}

impl ModuleRetentionDeclaration {
    pub fn validate(&self, known_stores: &[String]) -> CoreResult<()> {
        let store_name = match self {
            Self::ManualOnly { store_name }
            | Self::PurgeExpired { store_name, .. }
            | Self::CompactAfter { store_name, .. } => store_name,
        };
        validate_schema_identifier(store_name.as_str(), IdentifierKind::StoreName)?;
        if !known_stores
            .iter()
            .any(|store| store == store_name.as_str())
        {
            return Err(invalid_descriptor(format!(
                "retention declaration references unknown logical store {store_name}"
            )));
        }
        match self {
            Self::ManualOnly { .. } => Ok(()),
            Self::PurgeExpired {
                timestamp_column, ..
            } => validate_schema_identifier(timestamp_column, IdentifierKind::ColumnName),
            Self::CompactAfter { policy_id, .. } => {
                validate_schema_identifier(policy_id, IdentifierKind::RetentionPolicy)
            }
        }
    }

    fn diagnostic(&self) -> RuntimeModuleRetentionDiagnostic {
        match self {
            Self::ManualOnly { store_name } => RuntimeModuleRetentionDiagnostic {
                store_name: store_name.as_str().to_string(),
                policy: "manual_only".to_string(),
                detail: None,
            },
            Self::PurgeExpired {
                store_name,
                timestamp_column,
            } => RuntimeModuleRetentionDiagnostic {
                store_name: store_name.as_str().to_string(),
                policy: "purge_expired".to_string(),
                detail: Some(timestamp_column.clone()),
            },
            Self::CompactAfter {
                store_name,
                policy_id,
            } => RuntimeModuleRetentionDiagnostic {
                store_name: store_name.as_str().to_string(),
                policy: "compact_after".to_string(),
                detail: Some(policy_id.clone()),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryContractDescriptor {
    pub contract_name: String,
    pub description: String,
}

impl RepositoryContractDescriptor {
    pub fn validate(&self) -> CoreResult<()> {
        validate_schema_identifier(&self.contract_name, IdentifierKind::RepositoryContract)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryCatalogEntryDescriptor {
    pub query_id: String,
    pub store_name: StoreName,
    pub description: String,
    pub parameter_schema_id: Option<String>,
}

impl QueryCatalogEntryDescriptor {
    pub fn validate(&self, known_stores: &[String]) -> CoreResult<()> {
        validate_schema_identifier(&self.query_id, IdentifierKind::QueryId)?;
        validate_schema_identifier(self.store_name.as_str(), IdentifierKind::StoreName)?;
        if !known_stores
            .iter()
            .any(|store| store == self.store_name.as_str())
        {
            return Err(invalid_descriptor(format!(
                "query catalog entry {} references unknown logical store {}",
                self.query_id, self.store_name
            )));
        }
        if let Some(schema_id) = &self.parameter_schema_id {
            validate_schema_identifier(schema_id, IdentifierKind::QueryParameterSchema)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleTransferHookDescriptor {
    pub hook_name: String,
    pub format_version: u32,
}

impl ModuleTransferHookDescriptor {
    pub fn validate(&self) -> CoreResult<()> {
        validate_schema_identifier(&self.hook_name, IdentifierKind::TransferHook)?;
        validate_schema_version(self.format_version)
    }
}

pub fn generated_module_table_name(
    module_id: &ModuleId,
    table_name: &TableName,
) -> CoreResult<String> {
    module_id.validate()?;
    validate_schema_identifier(table_name.as_str(), IdentifierKind::TableName)?;
    validate_generated_name(format!("module_{module_id}_{table_name}"))
}

pub fn generated_module_index_name(
    module_id: &ModuleId,
    table_name: &TableName,
    purpose: &IndexPurpose,
) -> CoreResult<String> {
    module_id.validate()?;
    validate_schema_identifier(table_name.as_str(), IdentifierKind::TableName)?;
    validate_schema_identifier(purpose.as_str(), IdentifierKind::IndexPurpose)?;
    validate_generated_name(format!("idx_module_{module_id}_{table_name}_{purpose}"))
}

pub fn generated_module_trigger_name(
    module_id: &ModuleId,
    table_name: &TableName,
    purpose: &IndexPurpose,
) -> CoreResult<String> {
    module_id.validate()?;
    validate_schema_identifier(table_name.as_str(), IdentifierKind::TableName)?;
    validate_schema_identifier(purpose.as_str(), IdentifierKind::IndexPurpose)?;
    validate_generated_name(format!("trg_module_{module_id}_{table_name}_{purpose}"))
}

pub fn generated_module_search_table_name(
    module_id: &ModuleId,
    store_name: &StoreName,
) -> CoreResult<String> {
    module_id.validate()?;
    validate_schema_identifier(store_name.as_str(), IdentifierKind::StoreName)?;
    validate_generated_name(format!("module_{module_id}_{store_name}_search"))
}

pub fn validate_version_progression(
    installed_version: Option<u32>,
    descriptor_version: u32,
) -> CoreResult<()> {
    validate_schema_version(descriptor_version)?;
    if let Some(installed_version) = installed_version {
        validate_schema_version(installed_version)?;
        if descriptor_version < installed_version {
            return Err(invalid_descriptor(format!(
                "module descriptor version {descriptor_version} is older than installed version {installed_version}"
            )));
        }
    }
    Ok(())
}

fn validate_schema_version(version: u32) -> CoreResult<()> {
    if version == 0 {
        return Err(invalid_descriptor("module schema version must be positive"));
    }
    Ok(())
}

fn validate_generated_name(name: String) -> CoreResult<String> {
    if name.len() > MAX_GENERATED_NAME_BYTES {
        return Err(invalid_descriptor(format!(
            "generated physical name {name:?} exceeds {MAX_GENERATED_NAME_BYTES} bytes"
        )));
    }
    if RESERVED_IDENTIFIERS.contains(&name.as_str()) {
        return Err(invalid_descriptor(format!(
            "generated physical name {name:?} collides with a core runtime table"
        )));
    }
    Ok(name)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IdentifierKind {
    BackendVariant,
    ColumnName,
    IndexPurpose,
    MigrationFragment,
    ModuleId,
    OwnerCrate,
    OwnerModule,
    QueryId,
    QueryParameterSchema,
    RepositoryContract,
    RetentionPolicy,
    StoreName,
    TableName,
    TransferHook,
}

impl IdentifierKind {
    fn label(self) -> &'static str {
        match self {
            Self::BackendVariant => "backend variant",
            Self::ColumnName => "column name",
            Self::IndexPurpose => "index purpose",
            Self::MigrationFragment => "migration fragment",
            Self::ModuleId => "module id",
            Self::OwnerCrate => "owner crate",
            Self::OwnerModule => "owner module",
            Self::QueryId => "query id",
            Self::QueryParameterSchema => "query parameter schema",
            Self::RepositoryContract => "repository contract",
            Self::RetentionPolicy => "retention policy",
            Self::StoreName => "store name",
            Self::TableName => "table name",
            Self::TransferHook => "transfer hook",
        }
    }

    fn disallow_reserved(self) -> bool {
        matches!(self, Self::ModuleId | Self::StoreName | Self::TableName)
    }

    fn disallow_physical_prefix(self) -> bool {
        !matches!(self, Self::BackendVariant)
    }
}

fn validate_schema_identifier(value: &str, kind: IdentifierKind) -> CoreResult<()> {
    let label = kind.label();
    if value.is_empty() {
        return Err(invalid_descriptor(format!("{label} must not be empty")));
    }
    if value.len() > MAX_IDENTIFIER_BYTES {
        return Err(invalid_descriptor(format!(
            "{label} {value:?} exceeds {MAX_IDENTIFIER_BYTES} bytes"
        )));
    }
    if value.starts_with('_') || value.ends_with('_') || value.contains("__") {
        return Err(invalid_descriptor(format!(
            "{label} {value:?} must use non-empty snake_case segments"
        )));
    }
    if kind.disallow_physical_prefix()
        && (value.starts_with("module_")
            || value.starts_with("idx_")
            || value.starts_with("trg_")
            || value.starts_with("sqlite_")
            || value.starts_with("pg_"))
    {
        return Err(invalid_descriptor(format!(
            "{label} {value:?} uses a reserved physical-name prefix"
        )));
    }
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(invalid_descriptor(format!("{label} must not be empty")));
    };
    if !first.is_ascii_lowercase() {
        return Err(invalid_descriptor(format!(
            "{label} {value:?} must start with a lowercase ASCII letter"
        )));
    }
    if !chars.all(|character| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
    }) {
        return Err(invalid_descriptor(format!(
            "{label} {value:?} must be lowercase ASCII snake_case"
        )));
    }
    if SQL_KEYWORDS.contains(&value) {
        return Err(invalid_descriptor(format!(
            "{label} {value:?} is a reserved SQL keyword"
        )));
    }
    if kind.disallow_reserved() && RESERVED_IDENTIFIERS.contains(&value) {
        return Err(invalid_descriptor(format!(
            "{label} {value:?} collides with a core runtime namespace"
        )));
    }
    Ok(())
}

fn require_non_empty(context: &str, field: &str, len: usize) -> CoreResult<()> {
    if len == 0 {
        return Err(invalid_descriptor(format!(
            "{context} requires at least one {field} entry"
        )));
    }
    Ok(())
}

fn push_unique(kind: &str, value: &str, seen: &mut Vec<String>) -> CoreResult<()> {
    if seen.iter().any(|existing| existing == value) {
        return Err(invalid_descriptor(format!(
            "duplicate {kind} declaration {value:?}"
        )));
    }
    seen.push(value.to_string());
    Ok(())
}

fn invalid_descriptor(message: impl Into<String>) -> CoreError {
    CoreError::new(CoreErrorKind::InvalidInput, message)
}

fn descriptor_fingerprint_error(error: serde_json::Error) -> CoreError {
    CoreError::new(
        CoreErrorKind::InternalError,
        format!("serialize module schema descriptor for fingerprint: {error}"),
    )
}

fn canonical_json(value: JsonValue) -> String {
    match value {
        JsonValue::Null => "null".to_string(),
        JsonValue::Bool(value) => value.to_string(),
        JsonValue::Number(value) => value.to_string(),
        JsonValue::String(value) => serde_json::to_string(&value).expect("serialize string"),
        JsonValue::Array(values) => {
            let values = values
                .into_iter()
                .map(canonical_json)
                .collect::<Vec<_>>()
                .join(",");
            format!("[{values}]")
        }
        JsonValue::Object(values) => {
            let values = values
                .into_iter()
                .map(|(key, value)| {
                    let key = serde_json::to_string(&key).expect("serialize object key");
                    format!("{key}:{}", canonical_json(value))
                })
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{values}}}")
        }
    }
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_simple_kv_descriptor_and_generates_names() {
        let bundle = simple_kv_bundle().unwrap();
        let validated = bundle.validate().unwrap();

        assert_eq!(validated.module_id.as_str(), "simple_kv");
        assert_eq!(validated.schema_version, 1);
        assert_eq!(validated.physical_tables, vec!["module_simple_kv_entries"]);
        assert_eq!(
            validated.physical_indexes,
            vec![
                "idx_module_simple_kv_entries_scope_key",
                "idx_module_simple_kv_entries_expires_at"
            ]
        );
        assert_eq!(
            generated_module_trigger_name(
                &bundle.module_id,
                &TableName::new("entries").unwrap(),
                &IndexPurpose::new("expiry").unwrap(),
            )
            .unwrap(),
            "trg_module_simple_kv_entries_expiry"
        );
        assert_eq!(
            generated_module_search_table_name(
                &bundle.module_id,
                &StoreName::new("entries").unwrap(),
            )
            .unwrap(),
            "module_simple_kv_entries_search"
        );
    }

    #[test]
    fn rejects_invalid_snake_case_identifiers() {
        for value in [
            "",
            "RoleplayLore",
            "roleplay-lore",
            "roleplay__lore",
            "_roleplay",
            "roleplay_",
            "roleplay$lore",
            "module_roleplay_lore",
        ] {
            assert!(
                ModuleId::new(value).is_err(),
                "expected module id {value:?} to fail"
            );
        }
    }

    #[test]
    fn rejects_sql_keywords_and_core_namespace_collisions() {
        assert_eq!(
            ModuleId::new("select").unwrap_err().kind,
            CoreErrorKind::InvalidInput
        );
        assert_eq!(
            TableName::new("sessions").unwrap_err().kind,
            CoreErrorKind::InvalidInput
        );
        assert_eq!(
            StoreName::new("runtime_counters").unwrap_err().kind,
            CoreErrorKind::InvalidInput
        );
    }

    #[test]
    fn rejects_disallowed_physical_names_from_descriptor_fragments() {
        let mut bundle = simple_kv_bundle().unwrap();
        bundle.tables[0].table_name = TableName::unchecked_for_test("module_entries");

        let error = bundle.validate().unwrap_err();
        assert_eq!(error.kind, CoreErrorKind::InvalidInput);
        assert!(error.message.contains("reserved physical-name prefix"));
    }

    #[test]
    fn rejects_unknown_table_and_store_references() {
        let mut bundle = simple_kv_bundle().unwrap();
        bundle.indexes[0].table_name = TableName::new("missing").unwrap();

        let error = bundle.validate().unwrap_err();
        assert!(error.message.contains("unknown module table"));

        let mut bundle = simple_kv_bundle().unwrap();
        bundle.retention[0] = ModuleRetentionDeclaration::ManualOnly {
            store_name: StoreName::new("missing").unwrap(),
        };

        let error = bundle.validate().unwrap_err();
        assert!(error.message.contains("unknown logical store"));
    }

    #[test]
    fn validates_version_progression_inputs() {
        validate_version_progression(None, 1).unwrap();
        validate_version_progression(Some(1), 1).unwrap();
        validate_version_progression(Some(1), 2).unwrap();

        assert_eq!(
            validate_version_progression(None, 0).unwrap_err().kind,
            CoreErrorKind::InvalidInput
        );
        let error = validate_version_progression(Some(3), 2).unwrap_err();
        assert!(error.message.contains("older than installed"));
    }

    #[test]
    fn parses_capability_declarations() {
        assert_eq!(
            ModuleSchemaCapability::parse("transactions").unwrap(),
            ModuleSchemaCapability::Transactions
        );
        assert_eq!(
            ModuleSchemaCapability::parse("json_documents").unwrap(),
            ModuleSchemaCapability::JsonDocuments
        );
        assert_eq!(
            ModuleSchemaCapability::parse("definitely_not_real")
                .unwrap_err()
                .kind,
            CoreErrorKind::InvalidInput
        );

        ModuleCapabilityRequirement {
            capability: ModuleSchemaCapability::FullTextSearch,
            kind: ModuleCapabilityRequirementKind::Optional,
            backend_variant: Some("sqlite_fts5".to_string()),
        }
        .validate()
        .unwrap();
    }

    #[test]
    fn rejects_duplicate_logical_names_and_generated_names() {
        let mut bundle = simple_kv_bundle().unwrap();
        bundle.logical_stores.push(LogicalStoreDescriptor {
            store_name: StoreName::new("entries").unwrap(),
            description: "duplicate".to_string(),
        });

        let error = bundle.validate().unwrap_err();
        assert!(error.message.contains("duplicate logical store"));
    }

    #[test]
    fn registry_rejects_duplicate_modules_and_missing_capabilities() {
        let bundle = simple_kv_bundle().unwrap();
        let duplicate = ModuleSchemaRegistry::new(vec![bundle.clone(), bundle.clone()]);
        assert!(duplicate
            .unwrap_err()
            .message
            .contains("duplicate module schema bundle"));

        let registry = ModuleSchemaRegistry::new(vec![bundle]).unwrap();
        let error = registry.validate_capabilities(&[]).unwrap_err();
        assert!(error
            .message
            .contains("requires unsupported storage capability"));

        registry
            .validate_capabilities(&[ModuleSchemaCapability::Transactions])
            .unwrap();
    }

    #[test]
    fn descriptor_fingerprint_changes_with_descriptor_content() {
        let bundle = simple_kv_bundle().unwrap();
        let mut changed = bundle.clone();
        changed.schema_version = 2;

        let first = bundle.descriptor_fingerprint().unwrap();
        let second = changed.descriptor_fingerprint().unwrap();

        assert_ne!(first, second);
        assert!(first.starts_with("fnv1a64:"));
    }

    #[test]
    fn registry_diagnostics_reports_schema_projection_without_introspection() {
        let bundle = simple_kv_bundle().unwrap();
        let fingerprint = bundle.descriptor_fingerprint().unwrap();
        let registry = ModuleSchemaRegistry::new(vec![bundle]).unwrap();
        let installed = vec![InstalledModuleSchemaRecord {
            module_id: ModuleId::new("simple_kv").unwrap(),
            installed_version: 1,
            descriptor_fingerprint: fingerprint.clone(),
            installed_at: "2026-06-26T00:00:00Z".to_string(),
            updated_at: "2026-06-26T00:00:00Z".to_string(),
        }];

        let diagnostics = module_schema_registry_diagnostics(
            &registry,
            &installed,
            &[ModuleSchemaCapability::Transactions],
        )
        .unwrap();

        assert_eq!(diagnostics.modules.len(), 1);
        let module = &diagnostics.modules[0];
        assert_eq!(module.module_id, "simple_kv");
        assert_eq!(module.migration_status, "degraded");
        assert_eq!(module.descriptor_fingerprint, fingerprint);
        assert_eq!(
            module.physical_tables[0].physical_table,
            "module_simple_kv_entries"
        );
        assert_eq!(
            module.physical_indexes[0].physical_index,
            "idx_module_simple_kv_entries_scope_key"
        );
        assert_eq!(module.logical_stores[0].store_name, "entries");
        assert_eq!(module.retention[0].policy, "purge_expired");
        assert!(module
            .degraded_reasons
            .iter()
            .any(|reason| reason.contains("json_documents")));
        assert!(diagnostics.orphan_installed_modules.is_empty());
    }

    #[test]
    fn registry_diagnostics_reports_blocked_and_orphan_state() {
        let registry = ModuleSchemaRegistry::new(vec![simple_kv_bundle().unwrap()]).unwrap();
        let installed = vec![InstalledModuleSchemaRecord {
            module_id: ModuleId::new("old_module").unwrap(),
            installed_version: 1,
            descriptor_fingerprint: "fnv1a64:0000000000000000".to_string(),
            installed_at: "2026-06-26T00:00:00Z".to_string(),
            updated_at: "2026-06-26T00:00:00Z".to_string(),
        }];

        let diagnostics = module_schema_registry_diagnostics(&registry, &installed, &[]).unwrap();

        assert_eq!(diagnostics.modules[0].migration_status, "blocked");
        assert!(diagnostics.modules[0]
            .blocked_reasons
            .iter()
            .any(|reason| reason.contains("transactions")));
        assert_eq!(diagnostics.orphan_installed_modules.len(), 1);
        assert_eq!(
            diagnostics.orphan_installed_modules[0].module_id,
            "old_module"
        );
    }

    fn simple_kv_bundle() -> CoreResult<ModuleSchemaBundle> {
        Ok(simple_kv_schema_bundle())
    }
}
