//! Rust-owned module schema descriptor types.
//!
//! These descriptors are intentionally storage-boundary data. TypeScript may
//! call repository/query APIs built from them later, but it must not register
//! physical schema directly.

use rusty_crew_core_protocol::{CoreError, CoreErrorKind, CoreResult};
use serde::{Deserialize, Serialize};
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedModuleSchemaBundle {
    pub module_id: ModuleId,
    pub schema_version: u32,
    pub physical_tables: Vec<String>,
    pub physical_indexes: Vec<String>,
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
            vec!["idx_module_simple_kv_entries_scope_key"]
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

    fn simple_kv_bundle() -> CoreResult<ModuleSchemaBundle> {
        Ok(ModuleSchemaBundle {
            module_id: ModuleId::new("simple_kv")?,
            schema_version: 1,
            owner: ModuleOwner {
                crate_name: "core_persistence".to_string(),
                rust_module: "simple_kv".to_string(),
            },
            logical_stores: vec![LogicalStoreDescriptor {
                store_name: StoreName::new("entries")?,
                description: "Simple scoped key/value records".to_string(),
            }],
            tables: vec![ModuleTableDescriptor {
                table_name: TableName::new("entries")?,
                logical_store: StoreName::new("entries")?,
                declaration: TableDeclaration::Owned,
            }],
            indexes: vec![ModuleIndexDescriptor {
                table_name: TableName::new("entries")?,
                purpose: IndexPurpose::new("scope_key")?,
                columns: vec![
                    "scope_type".to_string(),
                    "scope_id".to_string(),
                    "entry_key".to_string(),
                ],
                unique: true,
            }],
            retention: vec![ModuleRetentionDeclaration::PurgeExpired {
                store_name: StoreName::new("entries")?,
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
                    contract_name: "put_kv".to_string(),
                    description: "Create or replace a key/value entry".to_string(),
                },
            ],
            query_catalog_entries: vec![QueryCatalogEntryDescriptor {
                query_id: "list_entries_by_scope".to_string(),
                store_name: StoreName::new("entries")?,
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
            migration_notes: vec!["initial descriptor fixture".to_string()],
        })
    }
}
