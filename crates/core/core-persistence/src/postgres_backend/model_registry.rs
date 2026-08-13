//! PostgreSQL normalized model endpoint and configuration repositories.

use super::*;
use crate::{
    LogicalStorageApplyProof, LogicalStorageExportBundle, LogicalStorageImportDryRun,
    LogicalStorageRecordPayload,
};

const POSTGRES_LOGICAL_MODEL_TARGET_BACKEND: &str = "postgres";
const POSTGRES_LOGICAL_MODEL_CAPABILITY: &str = "logical_export_import";

pub(super) fn apply_postgres_model_registry(
    tx: &mut Transaction<'_>,
    schema: &str,
) -> CoreResult<()> {
    tx.batch_execute(&format!(
        "CREATE TABLE IF NOT EXISTS {schema}.model_endpoints (
            endpoint_id TEXT PRIMARY KEY,
            status TEXT NOT NULL,
            protocol TEXT NOT NULL,
            credential_id TEXT,
            record_json TEXT NOT NULL,
            revision BIGINT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS model_endpoints_status_idx
            ON {schema}.model_endpoints(status, updated_at DESC, endpoint_id);
         CREATE INDEX IF NOT EXISTS model_endpoints_protocol_idx
            ON {schema}.model_endpoints(protocol, endpoint_id);
         CREATE TABLE IF NOT EXISTS {schema}.model_configurations (
            model_config_id TEXT PRIMARY KEY,
            endpoint_id TEXT NOT NULL REFERENCES {schema}.model_endpoints(endpoint_id),
            status TEXT NOT NULL,
            model_id TEXT NOT NULL,
            record_json TEXT NOT NULL,
            revision BIGINT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS model_configurations_endpoint_idx
            ON {schema}.model_configurations(endpoint_id, status, model_config_id);
         CREATE INDEX IF NOT EXISTS model_configurations_model_idx
            ON {schema}.model_configurations(model_id, model_config_id);"
    ))
    .map_err(|error| postgres_error("create PostgreSQL normalized model registries", error))?;
    let report = backfill_postgres_legacy_model_registry(tx, schema, false)?;
    crate::repos::model_registry::ensure_migration_backfill_is_safe(&report)
}

impl PostgresBackendStore {
    pub fn apply_model_registry_logical_import(
        &self,
        bundle: &LogicalStorageExportBundle,
        dry_run: &LogicalStorageImportDryRun,
    ) -> CoreResult<Vec<LogicalStorageApplyProof>> {
        if dry_run.import_batch_id.trim().is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "logical import requires an import_batch_id",
            ));
        }
        let (mut endpoints, mut configurations) =
            crate::repos::model_registry::logical_model_records(bundle)?;
        validate_postgres_logical_model_bundle(bundle, dry_run, &endpoints, &configurations)?;
        endpoints.sort_by(|left, right| left.endpoint_id.cmp(&right.endpoint_id));
        configurations.sort_by(|left, right| left.model_config_id.cmp(&right.model_config_id));
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start PostgreSQL model registry logical import", error)
        })?;
        if tx
            .query_opt(
                &format!("SELECT 1 FROM {schema}.runtime_import_batches WHERE import_batch_id=$1"),
                &[&dry_run.import_batch_id],
            )
            .map_err(|error| postgres_error("check PostgreSQL logical import batch", error))?
            .is_some()
        {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!(
                    "import batch {} is already recorded",
                    dry_run.import_batch_id
                ),
            ));
        }
        for endpoint in &endpoints {
            validate_postgres_import_credential(&mut tx, &schema, endpoint)?;
            match get_endpoint(&mut tx, &schema, &endpoint.endpoint_id)? {
                Some(existing) if existing == *endpoint => {}
                Some(_) => {
                    return Err(CoreError::new(
                        CoreErrorKind::ActionRejected,
                        format!(
                            "model endpoint {} conflicts with target record",
                            endpoint.endpoint_id
                        ),
                    ))
                }
                None => insert_postgres_logical_endpoint(&mut tx, &schema, endpoint)?,
            }
        }
        for configuration in &configurations {
            let endpoint =
                get_endpoint(&mut tx, &schema, &configuration.endpoint_id)?.ok_or_else(|| {
                    CoreError::new(
                        CoreErrorKind::NotFound,
                        format!("model endpoint {} not found", configuration.endpoint_id),
                    )
                })?;
            configuration.validate_for_endpoint(&endpoint)?;
            match get_configuration(&mut tx, &schema, &configuration.model_config_id)? {
                Some(existing) if existing == *configuration => {}
                Some(_) => {
                    return Err(CoreError::new(
                        CoreErrorKind::ActionRejected,
                        format!(
                            "model configuration {} conflicts with target record",
                            configuration.model_config_id
                        ),
                    ))
                }
                None => insert_postgres_logical_configuration(&mut tx, &schema, configuration)?,
            }
            sync_configuration_shadow(&mut tx, &schema, &endpoint, configuration)?;
        }
        tx.execute(
            &format!(
                "INSERT INTO {schema}.runtime_import_batches
                 (import_batch_id,source_system,source_label,source_snapshot_ref,notes,imported_at)
                 VALUES ($1,$2,$3,$4,$5,$6)"
            ),
            &[
                &dry_run.import_batch_id,
                &bundle.source.backend,
                &bundle.source.backend_label,
                &bundle.source.snapshot_ref,
                &Some("model endpoint/configuration logical import".to_string()),
                &dry_run.validation_time,
            ],
        )
        .map_err(|error| postgres_error("record PostgreSQL logical import batch", error))?;

        let readback_endpoints = endpoints
            .iter()
            .map(|record| get_endpoint(&mut tx, &schema, &record.endpoint_id))
            .collect::<CoreResult<Option<Vec<_>>>>()?
            .ok_or_else(|| CoreError::new(CoreErrorKind::NotFound, "imported endpoint missing"))?;
        let readback_configurations = configurations
            .iter()
            .map(|record| get_configuration(&mut tx, &schema, &record.model_config_id))
            .collect::<CoreResult<Option<Vec<_>>>>()?
            .ok_or_else(|| {
                CoreError::new(CoreErrorKind::NotFound, "imported configuration missing")
            })?;
        let applied = crate::sqlite_runtime_import::model_registry_logical_repositories(
            &readback_endpoints,
            &readback_configurations,
            &dry_run.validation_time,
        )?;
        let proofs = crate::repos::model_registry::model_registry_apply_proofs(
            bundle,
            &applied,
            &dry_run.import_batch_id,
        )?;
        if proofs.iter().any(|proof| !proof.verified) {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "PostgreSQL model registry post-import count/checksum proof failed",
            ));
        }
        tx.commit().map_err(|error| {
            postgres_error("commit PostgreSQL model registry logical import", error)
        })?;
        Ok(proofs)
    }

    pub fn upsert_model_endpoint(
        &self,
        write: &ModelEndpointWrite,
    ) -> CoreResult<ModelEndpointRecord> {
        write.validate()?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start PostgreSQL model endpoint upsert", error))?;
        validate_postgres_endpoint_credential(&mut tx, &schema, write)?;
        let existing = get_endpoint(&mut tx, &schema, &write.endpoint_id)?;
        validate_revision(
            "model endpoint",
            &write.endpoint_id,
            write.expected_revision,
            existing.as_ref().map(|record| record.revision),
        )?;
        let record = ModelEndpointRecord {
            endpoint_id: write.endpoint_id.clone(),
            status: write.status,
            display_name: write.display_name.clone(),
            description: write.description.clone(),
            base_url: write.base_url.clone(),
            protocol: write.protocol,
            wire_dialect: write.wire_dialect,
            auth_scheme: write.auth_scheme,
            credential_id: write.credential_id.clone(),
            prompt_cache_transport: write.prompt_cache_transport,
            metadata_json: write.metadata_json.clone(),
            revision: existing.as_ref().map_or(1, |record| record.revision + 1),
            created_at: existing
                .as_ref()
                .map_or_else(|| write.now.clone(), |record| record.created_at.clone()),
            updated_at: write.now.clone(),
        };
        let status = enum_text(record.status)?;
        let protocol = enum_text(record.protocol)?;
        let json = to_json_text(&record)?;
        let changed = if let Some(existing) = existing.as_ref() {
            tx.execute(
                &format!(
                    "UPDATE {schema}.model_endpoints
                     SET status=$2,protocol=$3,credential_id=$4,record_json=$5,revision=$6,updated_at=$7
                     WHERE endpoint_id=$1 AND revision=$8"
                ),
                &[&record.endpoint_id, &status, &protocol, &record.credential_id, &json,
                    &(record.revision as i64), &record.updated_at, &(existing.revision as i64)],
            )
        } else {
            tx.execute(
                &format!(
                    "INSERT INTO {schema}.model_endpoints
                     (endpoint_id,status,protocol,credential_id,record_json,revision,created_at,updated_at)
                     VALUES ($1,$2,$3,$4,$5,$6,$7,$8)"
                ),
                &[&record.endpoint_id, &status, &protocol, &record.credential_id, &json,
                    &(record.revision as i64), &record.created_at, &record.updated_at],
            )
        }
        .map_err(|error| postgres_error("write PostgreSQL model endpoint", error))?;
        if changed != 1 {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!("model endpoint {} changed concurrently", record.endpoint_id),
            ));
        }
        sync_endpoint_shadows(&mut tx, &schema, &record)?;
        tx.commit()
            .map_err(|error| postgres_error("commit PostgreSQL model endpoint upsert", error))?;
        Ok(record)
    }

    pub fn get_model_endpoint(&self, endpoint_id: &str) -> CoreResult<Option<ModelEndpointRecord>> {
        ModelEndpointQuery {
            endpoint_id: Some(endpoint_id.to_string()),
            ..Default::default()
        }
        .validate()?;
        let schema = self.quoted_schema();
        get_endpoint(&mut *self.client()?, &schema, endpoint_id)
    }

    pub fn list_model_endpoints(
        &self,
        query: &ModelEndpointQuery,
    ) -> CoreResult<Vec<ModelEndpointRecord>> {
        query.validate()?;
        let schema = self.quoted_schema();
        let status = query.status.map(enum_text).transpose()?;
        let limit = query.limit.unwrap_or(100).clamp(1, 1_000) as i64;
        let offset = query.offset.unwrap_or(0) as i64;
        self.client()?
            .query(
                &format!(
                    "SELECT record_json FROM {schema}.model_endpoints
                     WHERE ($1::TEXT IS NULL OR endpoint_id=$1)
                       AND ($2::TEXT IS NULL OR status=$2)
                     ORDER BY updated_at DESC,endpoint_id ASC LIMIT $3 OFFSET $4"
                ),
                &[&query.endpoint_id, &status, &limit, &offset],
            )
            .map_err(|error| postgres_error("list PostgreSQL model endpoints", error))?
            .iter()
            .map(|row| parse_json(row.get::<_, String>(0), "model endpoint record_json"))
            .collect()
    }

    pub fn upsert_model_configuration(
        &self,
        write: &ModelConfigurationWrite,
    ) -> CoreResult<ModelConfigurationRecord> {
        write.validate()?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start PostgreSQL model configuration upsert", error)
        })?;
        let endpoint = get_endpoint(&mut tx, &schema, &write.endpoint_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("model endpoint {} not found", write.endpoint_id),
            )
        })?;
        write.validate_for_endpoint(&endpoint)?;
        let existing = get_configuration(&mut tx, &schema, &write.model_config_id)?;
        validate_revision(
            "model configuration",
            &write.model_config_id,
            write.expected_revision,
            existing.as_ref().map(|record| record.revision),
        )?;
        let record = ModelConfigurationRecord {
            model_config_id: write.model_config_id.clone(),
            endpoint_id: write.endpoint_id.clone(),
            status: write.status,
            display_name: write.display_name.clone(),
            description: write.description.clone(),
            model_id: write.model_id.clone(),
            context_window_tokens: write.context_window_tokens,
            max_output_tokens: write.max_output_tokens,
            temperature_milli: write.temperature_milli,
            reasoning_effort: write.reasoning_effort.clone(),
            reasoning_format: write.reasoning_format.clone(),
            reasoning_history: write.reasoning_history,
            reasoning_budget_tokens: write.reasoning_budget_tokens,
            thinking_mode: write.thinking_mode,
            prompt_caching_policy: write.prompt_caching_policy,
            capabilities: write.capabilities.clone(),
            metadata_json: write.metadata_json.clone(),
            revision: existing.as_ref().map_or(1, |record| record.revision + 1),
            created_at: existing
                .as_ref()
                .map_or_else(|| write.now.clone(), |record| record.created_at.clone()),
            updated_at: write.now.clone(),
        };
        let status = enum_text(record.status)?;
        let json = to_json_text(&record)?;
        let changed = if let Some(existing) = existing.as_ref() {
            tx.execute(&format!(
                "UPDATE {schema}.model_configurations
                 SET endpoint_id=$2,status=$3,model_id=$4,record_json=$5,revision=$6,updated_at=$7
                 WHERE model_config_id=$1 AND revision=$8"),
                &[&record.model_config_id,&record.endpoint_id,&status,&record.model_id,&json,
                    &(record.revision as i64),&record.updated_at,&(existing.revision as i64)])
        } else {
            tx.execute(&format!(
                "INSERT INTO {schema}.model_configurations
                 (model_config_id,endpoint_id,status,model_id,record_json,revision,created_at,updated_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8)"),
                &[&record.model_config_id,&record.endpoint_id,&status,&record.model_id,&json,
                    &(record.revision as i64),&record.created_at,&record.updated_at])
        }.map_err(|error| postgres_error("write PostgreSQL model configuration", error))?;
        if changed != 1 {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "model configuration {} changed concurrently",
                    record.model_config_id
                ),
            ));
        }
        sync_configuration_shadow(&mut tx, &schema, &endpoint, &record)?;
        tx.commit().map_err(|error| {
            postgres_error("commit PostgreSQL model configuration upsert", error)
        })?;
        Ok(record)
    }

    pub fn get_model_configuration(
        &self,
        model_config_id: &str,
    ) -> CoreResult<Option<ModelConfigurationRecord>> {
        ModelConfigurationQuery {
            model_config_id: Some(model_config_id.to_string()),
            ..Default::default()
        }
        .validate()?;
        let schema = self.quoted_schema();
        get_configuration(&mut *self.client()?, &schema, model_config_id)
    }

    pub fn list_model_configurations(
        &self,
        query: &ModelConfigurationQuery,
    ) -> CoreResult<Vec<ModelConfigurationRecord>> {
        query.validate()?;
        let schema = self.quoted_schema();
        let status = query.status.map(enum_text).transpose()?;
        let limit = query.limit.unwrap_or(100).clamp(1, 1_000) as i64;
        let offset = query.offset.unwrap_or(0) as i64;
        self.client()?
            .query(
                &format!(
                    "SELECT record_json FROM {schema}.model_configurations
             WHERE ($1::TEXT IS NULL OR model_config_id=$1)
               AND ($2::TEXT IS NULL OR endpoint_id=$2)
               AND ($3::TEXT IS NULL OR status=$3)
             ORDER BY updated_at DESC,model_config_id ASC LIMIT $4 OFFSET $5"
                ),
                &[
                    &query.model_config_id,
                    &query.endpoint_id,
                    &status,
                    &limit,
                    &offset,
                ],
            )
            .map_err(|error| postgres_error("list PostgreSQL model configurations", error))?
            .iter()
            .map(|row| parse_json(row.get::<_, String>(0), "model configuration record_json"))
            .collect()
    }

    pub fn backfill_legacy_model_registry(
        &self,
        dry_run: bool,
    ) -> CoreResult<ModelEndpointBackfillReport> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start PostgreSQL legacy model registry backfill", error)
        })?;
        let report = backfill_postgres_legacy_model_registry(&mut tx, &schema, dry_run)?;
        if dry_run {
            tx.rollback().map_err(|error| {
                postgres_error("rollback PostgreSQL model registry dry-run", error)
            })?;
        } else {
            tx.commit().map_err(|error| {
                postgres_error("commit PostgreSQL legacy model registry backfill", error)
            })?;
        }
        Ok(report)
    }
}

fn validate_postgres_logical_model_bundle(
    bundle: &LogicalStorageExportBundle,
    dry_run: &LogicalStorageImportDryRun,
    endpoints: &[ModelEndpointRecord],
    configurations: &[ModelConfigurationRecord],
) -> CoreResult<()> {
    crate::sqlite_runtime_import::validate_model_registry_logical_import_envelope(
        bundle,
        dry_run,
        POSTGRES_LOGICAL_MODEL_TARGET_BACKEND,
        "PostgreSQL",
    )?;
    let supported_capabilities = dry_run
        .supported_capabilities
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if !supported_capabilities.contains(POSTGRES_LOGICAL_MODEL_CAPABILITY) {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!(
                "PostgreSQL model registry logical import requires declared capability {}",
                POSTGRES_LOGICAL_MODEL_CAPABILITY
            ),
        ));
    }
    let supported_repositories = dry_run
        .supported_repositories
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let missing_repositories = crate::sqlite_runtime_import::MODEL_REGISTRY_LOGICAL_REPOSITORIES
        .iter()
        .copied()
        .filter(|repository_id| !supported_repositories.contains(repository_id))
        .collect::<Vec<_>>();
    if !missing_repositories.is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!(
                "PostgreSQL model registry logical import requires declared repository support: {}",
                missing_repositories.join(", ")
            ),
        ));
    }
    if bundle.bundle_version != 1 {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!(
                "unsupported logical bundle version {}",
                bundle.bundle_version
            ),
        ));
    }
    for repository_id in crate::sqlite_runtime_import::MODEL_REGISTRY_LOGICAL_REPOSITORIES {
        let repository = bundle
            .repositories
            .iter()
            .find(|repository| repository.repository_id == repository_id)
            .expect("model registry envelope validation requires each repository exactly once");
        let mut stable_ids = BTreeSet::new();
        for record in &repository.records {
            if record.stable_id.trim().is_empty() {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    format!("repository {repository_id} contains a record without a stable_id"),
                ));
            }
            if !stable_ids.insert(record.stable_id.as_str()) {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    format!(
                        "repository {repository_id} contains duplicate stable_id {}",
                        record.stable_id
                    ),
                ));
            }
            match (repository_id, &record.payload) {
                ("model_endpoints", LogicalStorageRecordPayload::ModelEndpoint(endpoint))
                    if record.record_version == 1 && record.stable_id == endpoint.endpoint_id => {}
                (
                    "model_configurations",
                    LogicalStorageRecordPayload::ModelConfiguration(configuration),
                ) if record.record_version == 1
                    && record.stable_id == configuration.model_config_id =>
                {}
                _ => {
                    return Err(CoreError::new(
                        CoreErrorKind::InvalidInput,
                        format!(
                            "repository {repository_id} contains a record with invalid stable_id, payload, or record version"
                        ),
                    ))
                }
            }
        }
        if repository.schema_version != 1
            || repository.exported_count != repository.records.len() as u64
        {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("repository {repository_id} version/count proof failed"),
            ));
        }
        if !repository
            .required_capabilities
            .iter()
            .any(|capability| capability == POSTGRES_LOGICAL_MODEL_CAPABILITY)
        {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!(
                    "repository {repository_id} must require capability {}",
                    POSTGRES_LOGICAL_MODEL_CAPABILITY
                ),
            ));
        }
        let missing_capabilities = repository
            .required_capabilities
            .iter()
            .filter(|capability| !supported_capabilities.contains(capability.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        if !missing_capabilities.is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!(
                    "target backend {} is missing required capabilities for repository {}: {}",
                    dry_run.target_backend,
                    repository_id,
                    missing_capabilities.join(", ")
                ),
            ));
        }
        let Some(declared_checksum) = repository.checksum.as_deref() else {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("repository {repository_id} requires a checksum proof"),
            ));
        };
        let checksum =
            crate::sqlite_runtime_import::logical_storage_records_checksum(&repository.records)?;
        if declared_checksum != checksum {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("repository {repository_id} checksum proof failed"),
            ));
        }
    }
    let endpoint_by_id = endpoints
        .iter()
        .map(|endpoint| (endpoint.endpoint_id.as_str(), endpoint))
        .collect::<std::collections::BTreeMap<_, _>>();
    for endpoint in endpoints {
        endpoint.validate()?;
    }
    for configuration in configurations {
        let endpoint = endpoint_by_id
            .get(configuration.endpoint_id.as_str())
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::InvalidInput,
                    format!(
                        "model configuration {} references missing endpoint {}",
                        configuration.model_config_id, configuration.endpoint_id
                    ),
                )
            })?;
        configuration.validate_for_endpoint(endpoint)?;
    }
    Ok(())
}

fn validate_postgres_import_credential(
    tx: &mut Transaction<'_>,
    schema: &str,
    endpoint: &ModelEndpointRecord,
) -> CoreResult<()> {
    let Some(credential_id) = endpoint.credential_id.as_deref() else {
        return Ok(());
    };
    let row = tx
        .query_opt(
            &format!(
                "SELECT credential_kind,secret_ciphertext IS NOT NULL
                 FROM {schema}.service_credentials WHERE credential_id=$1"
            ),
            &[&credential_id],
        )
        .map_err(|error| postgres_error("load PostgreSQL logical import credential", error))?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("service credential {credential_id} not found"),
            )
        })?;
    let kind = row.get::<_, String>(0);
    let has_secret = row.get::<_, bool>(1);
    if !has_secret {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("service credential {credential_id} has no secret"),
        ));
    }
    let compatible = matches!(
        (endpoint.auth_scheme, kind.as_str()),
        (
            ModelEndpointAuthScheme::BearerApiKey,
            "api_key" | "legacy_raw_api_key"
        ) | (ModelEndpointAuthScheme::OpenAiCodexOauth, "openai_oauth")
    );
    if compatible {
        Ok(())
    } else {
        Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!(
                "model endpoint {} auth scheme is incompatible with credential kind {kind}",
                endpoint.endpoint_id
            ),
        ))
    }
}

fn insert_postgres_logical_endpoint(
    tx: &mut Transaction<'_>,
    schema: &str,
    record: &ModelEndpointRecord,
) -> CoreResult<()> {
    let status = enum_text(record.status)?;
    let protocol = enum_text(record.protocol)?;
    let json = to_json_text(record)?;
    tx.execute(
        &format!(
            "INSERT INTO {schema}.model_endpoints
             (endpoint_id,status,protocol,credential_id,record_json,revision,created_at,updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8)"
        ),
        &[
            &record.endpoint_id,
            &status,
            &protocol,
            &record.credential_id,
            &json,
            &(record.revision as i64),
            &record.created_at,
            &record.updated_at,
        ],
    )
    .map_err(|error| postgres_error("insert PostgreSQL logical model endpoint", error))?;
    Ok(())
}

fn insert_postgres_logical_configuration(
    tx: &mut Transaction<'_>,
    schema: &str,
    record: &ModelConfigurationRecord,
) -> CoreResult<()> {
    let status = enum_text(record.status)?;
    let json = to_json_text(record)?;
    tx.execute(
        &format!(
            "INSERT INTO {schema}.model_configurations
             (model_config_id,endpoint_id,status,model_id,record_json,revision,created_at,updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8)"
        ),
        &[
            &record.model_config_id,
            &record.endpoint_id,
            &status,
            &record.model_id,
            &json,
            &(record.revision as i64),
            &record.created_at,
            &record.updated_at,
        ],
    )
    .map_err(|error| postgres_error("insert PostgreSQL logical model configuration", error))?;
    Ok(())
}

fn backfill_postgres_legacy_model_registry(
    tx: &mut Transaction<'_>,
    schema: &str,
    dry_run: bool,
) -> CoreResult<ModelEndpointBackfillReport> {
    let catalog_schema = schema
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(schema)
        .replace("\"\"", "\"")
        .chars()
        .take(63)
        .collect::<String>();
    let has_credentials: bool = tx
        .query_one(
            "SELECT EXISTS (
                SELECT 1
                FROM pg_catalog.pg_class c
                JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace
                WHERE n.nspname::TEXT=$1 AND c.relname='service_credentials'
             )",
            &[&catalog_schema],
        )
        .map_err(|error| postgres_error("inspect PostgreSQL credential table for backfill", error))?
        .get(0);
    let credential_projection = if has_credentials {
        format!(
            "mp.credential_id,sc.credential_kind,sc.secret_ciphertext,sc.secret_updated_at,sc.revision
             FROM {schema}.model_providers mp
             LEFT JOIN {schema}.service_credentials sc ON sc.credential_id=mp.credential_id"
        )
    } else {
        format!(
            "NULL::TEXT,NULL::TEXT,NULL::TEXT,NULL::TEXT,NULL::BIGINT
             FROM {schema}.model_providers mp"
        )
    };
    let rows = tx
        .query(
            &format!(
                "SELECT mp.alias,mp.provider_json,{credential_projection}
                 ORDER BY mp.alias"
            ),
            &[],
        )
        .map_err(|error| {
            postgres_error("query PostgreSQL legacy model registry backfill", error)
        })?;
    let mut report = ModelEndpointBackfillReport::default();
    for row in rows {
        let alias: String = row.get(0);
        let mut provider = match parse_json::<ModelProviderRecord>(
            row.get::<_, String>(1),
            "legacy model provider_json",
        ) {
            Ok(provider) => provider,
            Err(error) => {
                report
                    .representability_errors
                    .push(ModelEndpointRepresentabilityError {
                        legacy_alias: alias,
                        field: "provider_json".to_string(),
                        reason: error.message,
                    });
                continue;
            }
        };
        provider.credential_id = row.get(2);
        let kind_raw: Option<String> = row.get(3);
        let secret: Option<String> = row.get(4);
        let kind = match kind_raw.as_deref().map(parse_credential_kind).transpose() {
            Ok(kind) => kind,
            Err(error) => {
                report
                    .representability_errors
                    .push(ModelEndpointRepresentabilityError {
                        legacy_alias: alias,
                        field: "credential_kind".to_string(),
                        reason: error.message,
                    });
                continue;
            }
        };
        provider.credential = ModelProviderCredential {
            has_secret: secret.is_some(),
            secret_ref: secret
                .as_ref()
                .and(provider.credential_id.as_ref())
                .map(|id| format!("db://service_credentials/{id}/secret")),
            updated_at: row.get(5),
            kind,
            revision: row.get::<_, Option<i64>>(6).map(|value| value as u64),
        };
        let (endpoint, configuration) = match normalized_from_provider(&provider) {
            Ok(records) => records,
            Err(error) => {
                report
                    .representability_errors
                    .push(ModelEndpointRepresentabilityError {
                        legacy_alias: alias,
                        field: "legacy_provider".to_string(),
                        reason: error.message,
                    });
                continue;
            }
        };
        let mut actual_configuration = get_configuration(tx, schema, &alias)?;
        let resolved_endpoint_id = actual_configuration
            .as_ref()
            .map_or_else(|| alias.clone(), |record| record.endpoint_id.clone());
        let mut actual_endpoint = get_endpoint(tx, schema, &resolved_endpoint_id)?;
        if !dry_run {
            if actual_endpoint.is_none() {
                insert_endpoint_if_absent(tx, schema, &endpoint)?;
                actual_endpoint = Some(endpoint.clone());
            }
            if actual_configuration.is_none() {
                insert_configuration_if_absent(tx, schema, &configuration)?;
                actual_configuration = Some(configuration.clone());
            }
        }
        let mapping = ModelEndpointLegacyAliasMapping {
            legacy_alias: alias.clone(),
            endpoint_id: actual_endpoint
                .as_ref()
                .map_or_else(|| alias.clone(), |record| record.endpoint_id.clone()),
            model_config_id: actual_configuration
                .as_ref()
                .map_or_else(|| alias.clone(), |record| record.model_config_id.clone()),
        };
        let projected_endpoint = actual_endpoint.as_ref().unwrap_or(&endpoint);
        let projected_configuration = actual_configuration.as_ref().unwrap_or(&configuration);
        let credential = if has_credentials {
            endpoint_credential_summary(tx, schema, projected_endpoint.credential_id.as_deref())?
        } else {
            provider.credential.clone()
        };
        let joined = crate::repos::model_registry::joined_provider_projection(
            projected_endpoint,
            projected_configuration,
            credential,
        );
        let differing_fields =
            crate::repos::model_registry::provider_projection_differences(&provider, &joined);
        report.mappings.push(mapping);
        report
            .joined_projection_equality
            .push(ModelEndpointJoinedProjectionEquality {
                legacy_alias: alias.clone(),
                endpoint_id: projected_endpoint.endpoint_id.clone(),
                model_config_id: projected_configuration.model_config_id.clone(),
                projection_equal: differing_fields.is_empty(),
                differing_fields,
            });
    }
    Ok(report)
}

fn parse_credential_kind(raw: &str) -> CoreResult<ModelProviderCredentialKind> {
    match raw {
        "api_key" => Ok(ModelProviderCredentialKind::ApiKey),
        "openai_oauth" => Ok(ModelProviderCredentialKind::OpenAiOauth),
        "legacy_raw_api_key" => Ok(ModelProviderCredentialKind::LegacyRawApiKey),
        _ => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("unknown credential kind {raw}"),
        )),
    }
}

fn sync_endpoint_shadows<C: GenericClient>(
    client: &mut C,
    schema: &str,
    endpoint: &ModelEndpointRecord,
) -> CoreResult<()> {
    let rows = client
        .query(
            &format!(
                "SELECT record_json FROM {schema}.model_configurations
                 WHERE endpoint_id=$1 ORDER BY model_config_id"
            ),
            &[&endpoint.endpoint_id],
        )
        .map_err(|error| postgres_error("query PostgreSQL endpoint shadows", error))?;
    for row in rows {
        let configuration = parse_json::<ModelConfigurationRecord>(
            row.get::<_, String>(0),
            "model configuration record_json",
        )?;
        sync_configuration_shadow(client, schema, endpoint, &configuration)?;
    }
    Ok(())
}

fn sync_configuration_shadow<C: GenericClient>(
    client: &mut C,
    schema: &str,
    endpoint: &ModelEndpointRecord,
    configuration: &ModelConfigurationRecord,
) -> CoreResult<()> {
    let existing = client
        .query_opt(
            &format!(
                "SELECT provider_json,revision,created_at FROM {schema}.model_providers WHERE alias=$1"
            ),
            &[&configuration.model_config_id],
        )
        .map_err(|error| postgres_error("load PostgreSQL provider shadow", error))?;
    let (revision, created_at) = existing.as_ref().map_or_else(
        || (1, configuration.created_at.clone()),
        |row| (row.get::<_, i64>(1) as u64 + 1, row.get::<_, String>(2)),
    );
    let credential =
        endpoint_credential_summary(client, schema, endpoint.credential_id.as_deref())?;
    let provider_kind = endpoint
        .metadata_json
        .get("legacyVendorLabel")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("custom")
        .to_string();
    let responses_dialect = match endpoint.wire_dialect {
        ModelEndpointWireDialect::OpenaiStateful => Some(ResponsesProviderDialect::OpenaiStateful),
        ModelEndpointWireDialect::OpenaiStateless => {
            Some(ResponsesProviderDialect::OpenaiStateless)
        }
        ModelEndpointWireDialect::GenericStateless => {
            Some(ResponsesProviderDialect::GenericStateless)
        }
        ModelEndpointWireDialect::Deepseek
            if endpoint.protocol == ModelProviderProtocol::Responses =>
        {
            Some(ResponsesProviderDialect::Deepseek)
        }
        ModelEndpointWireDialect::Meta => Some(ResponsesProviderDialect::Meta),
        _ => None,
    };
    let chat_completions_dialect = match (endpoint.protocol, endpoint.wire_dialect) {
        (ModelProviderProtocol::Responses, _) => {
            rusty_crew_core_protocol::ChatCompletionsWireDialect::Standard
        }
        (_, ModelEndpointWireDialect::Kimi) => {
            rusty_crew_core_protocol::ChatCompletionsWireDialect::Kimi
        }
        (_, ModelEndpointWireDialect::Glm) => {
            rusty_crew_core_protocol::ChatCompletionsWireDialect::Glm
        }
        (_, ModelEndpointWireDialect::Qwen) => {
            rusty_crew_core_protocol::ChatCompletionsWireDialect::Qwen
        }
        (_, ModelEndpointWireDialect::Deepseek) => {
            rusty_crew_core_protocol::ChatCompletionsWireDialect::Deepseek
        }
        _ => rusty_crew_core_protocol::ChatCompletionsWireDialect::Standard,
    };
    let provider = ModelProviderRecord {
        alias: configuration.model_config_id.clone(),
        status: configuration.status,
        protocol: endpoint.protocol,
        provider_kind,
        display_name: configuration.display_name.clone(),
        description: configuration.description.clone(),
        base_url: crate::repos::model_registry::projected_legacy_base_url(endpoint),
        model_id: configuration.model_id.clone(),
        context_window_tokens: configuration.context_window_tokens,
        max_output_tokens: configuration.max_output_tokens,
        temperature_milli: configuration.temperature_milli,
        reasoning_effort: configuration.reasoning_effort.clone(),
        reasoning_format: configuration.reasoning_format.clone(),
        responses_dialect,
        chat_completions_dialect,
        thinking_mode: configuration.thinking_mode,
        reasoning_history: configuration.reasoning_history,
        reasoning_budget_tokens: configuration.reasoning_budget_tokens,
        prompt_caching: configuration.prompt_caching_policy,
        credential_id: endpoint.credential_id.clone(),
        credential,
        metadata_json: configuration.metadata_json.clone(),
        revision,
        created_at: created_at.clone(),
        updated_at: configuration.updated_at.clone(),
    };
    client.execute(&format!(
        "INSERT INTO {schema}.model_providers
         (alias,status,protocol,provider_json,secret_ciphertext,secret_updated_at,revision,created_at,updated_at,credential_id)
         VALUES ($1,$2,$3,$4,NULL,NULL,$5,$6,$7,$8)
         ON CONFLICT(alias) DO UPDATE SET status=excluded.status,protocol=excluded.protocol,
           provider_json=excluded.provider_json,secret_ciphertext=NULL,secret_updated_at=NULL,
           revision=excluded.revision,updated_at=excluded.updated_at,credential_id=excluded.credential_id"),
        &[&provider.alias,&enum_text(provider.status)?,&enum_text(provider.protocol)?,&to_json_text(&provider)?,
            &(revision as i64),&created_at,&provider.updated_at,&provider.credential_id])
        .map_err(|error| postgres_error("write PostgreSQL legacy provider shadow", error))?;
    Ok(())
}

fn endpoint_credential_summary<C: GenericClient>(
    client: &mut C,
    schema: &str,
    credential_id: Option<&str>,
) -> CoreResult<ModelProviderCredential> {
    let Some(id) = credential_id else {
        return Ok(ModelProviderCredential {
            has_secret: false,
            secret_ref: None,
            updated_at: None,
            kind: None,
            revision: None,
        });
    };
    let row = client
        .query_one(
            &format!(
                "SELECT credential_kind,secret_ciphertext,secret_updated_at,revision
         FROM {schema}.service_credentials WHERE credential_id=$1"
            ),
            &[&id],
        )
        .map_err(|error| postgres_error("load PostgreSQL endpoint credential summary", error))?;
    let kind_raw: String = row.get(0);
    let kind = match kind_raw.as_str() {
        "api_key" => ModelProviderCredentialKind::ApiKey,
        "openai_oauth" => ModelProviderCredentialKind::OpenAiOauth,
        "legacy_raw_api_key" => ModelProviderCredentialKind::LegacyRawApiKey,
        _ => {
            return Err(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown credential kind {kind_raw}"),
            ))
        }
    };
    let secret: Option<String> = row.get(1);
    Ok(ModelProviderCredential {
        has_secret: secret.is_some(),
        secret_ref: secret
            .as_ref()
            .map(|_| format!("db://service_credentials/{id}/secret")),
        updated_at: row.get(2),
        kind: Some(kind),
        revision: Some(row.get::<_, i64>(3) as u64),
    })
}

pub(super) fn sync_legacy_provider_to_normalized_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    provider: &ModelProviderRecord,
) -> CoreResult<()> {
    let existing_configuration = get_configuration(tx, schema, &provider.alias)?;
    let (mut endpoint, mut configuration) = match normalized_from_provider(provider) {
        Ok(records) => records,
        Err(error)
            if error.kind == CoreErrorKind::InvalidInput && existing_configuration.is_none() =>
        {
            // Keep incomplete legacy rollback rows writable. They remain absent from the
            // normalized registry until deterministic projection is representable.
            return Ok(());
        }
        Err(error) => return Err(error),
    };
    if let Some(existing) = existing_configuration.as_ref() {
        configuration.endpoint_id = existing.endpoint_id.clone();
        endpoint.endpoint_id = existing.endpoint_id.clone();
    }
    let existing_endpoint = get_endpoint(tx, schema, &endpoint.endpoint_id)?;
    if let Some(existing) = existing_endpoint.as_ref() {
        // Joined legacy rows expose configuration lifecycle/display values, so
        // those fields cannot safely mutate a shared endpoint.
        endpoint.status = existing.status;
        endpoint.display_name = existing.display_name.clone();
        endpoint.description = existing.description.clone();
        let projected_vendor_label = existing
            .metadata_json
            .get("legacyVendorLabel")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("custom");
        if provider.provider_kind == projected_vendor_label {
            endpoint.metadata_json = existing.metadata_json.clone();
        }
        let shared_count: i64 = tx
            .query_one(
                &format!("SELECT COUNT(*) FROM {schema}.model_configurations WHERE endpoint_id=$1"),
                &[&endpoint.endpoint_id],
            )
            .map_err(|error| postgres_error("count PostgreSQL shared endpoint configs", error))?
            .get(0);
        let endpoint_changed = endpoint_fields_changed(existing, &endpoint);
        if shared_count > 1 && endpoint_changed {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "legacy_provider_shared_endpoint_conflict",
            ));
        }
        if endpoint_changed {
            endpoint.revision = existing.revision + 1;
            endpoint.created_at = existing.created_at.clone();
            update_endpoint_record(tx, schema, &endpoint, existing.revision)?;
        }
    } else {
        insert_endpoint_if_absent(tx, schema, &endpoint)?;
    }
    if let Some(existing) = existing_configuration.as_ref() {
        configuration.revision = existing.revision + 1;
        configuration.created_at = existing.created_at.clone();
        update_configuration_record(tx, schema, &configuration, existing.revision)?;
    } else {
        insert_configuration_if_absent(tx, schema, &configuration)?;
    }
    Ok(())
}

fn normalized_from_provider(
    provider: &ModelProviderRecord,
) -> CoreResult<(ModelEndpointRecord, ModelConfigurationRecord)> {
    let mut metadata = serde_json::Map::new();
    metadata.insert(
        "legacyVendorLabel".to_string(),
        serde_json::Value::String(provider.provider_kind.clone()),
    );
    let base_url = crate::repos::model_registry::legacy_endpoint_base_url(provider, &mut metadata);
    let wire_dialect = match provider.protocol {
        ModelProviderProtocol::Responses => match provider.responses_dialect {
            Some(ResponsesProviderDialect::OpenaiStateful) => {
                ModelEndpointWireDialect::OpenaiStateful
            }
            Some(ResponsesProviderDialect::OpenaiStateless) => {
                ModelEndpointWireDialect::OpenaiStateless
            }
            Some(ResponsesProviderDialect::GenericStateless) => {
                ModelEndpointWireDialect::GenericStateless
            }
            Some(ResponsesProviderDialect::Deepseek) => ModelEndpointWireDialect::Deepseek,
            Some(ResponsesProviderDialect::Meta) => ModelEndpointWireDialect::Meta,
            None => {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "legacy responses_dialect is not representable",
                ))
            }
        },
        ModelProviderProtocol::ChatCompletions => match provider.chat_completions_dialect {
            rusty_crew_core_protocol::ChatCompletionsWireDialect::Standard => {
                ModelEndpointWireDialect::Standard
            }
            rusty_crew_core_protocol::ChatCompletionsWireDialect::Kimi => {
                ModelEndpointWireDialect::Kimi
            }
            rusty_crew_core_protocol::ChatCompletionsWireDialect::Glm => {
                ModelEndpointWireDialect::Glm
            }
            rusty_crew_core_protocol::ChatCompletionsWireDialect::Qwen => {
                ModelEndpointWireDialect::Qwen
            }
            rusty_crew_core_protocol::ChatCompletionsWireDialect::Deepseek => {
                ModelEndpointWireDialect::Deepseek
            }
        },
    };
    let auth_scheme = match provider.credential.kind {
        None if provider.credential_id.is_none() => ModelEndpointAuthScheme::None,
        Some(
            ModelProviderCredentialKind::ApiKey | ModelProviderCredentialKind::LegacyRawApiKey,
        ) => ModelEndpointAuthScheme::BearerApiKey,
        Some(ModelProviderCredentialKind::OpenAiOauth)
            if provider.protocol == ModelProviderProtocol::Responses =>
        {
            ModelEndpointAuthScheme::OpenAiCodexOauth
        }
        _ => {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "legacy credential binding is not representable",
            ))
        }
    };
    let endpoint = ModelEndpointRecord {
        endpoint_id: provider.alias.clone(),
        status: provider.status,
        display_name: provider.display_name.clone(),
        description: provider.description.clone(),
        base_url,
        protocol: provider.protocol,
        wire_dialect,
        auth_scheme,
        credential_id: provider.credential_id.clone(),
        prompt_cache_transport: if provider.prompt_caching
            == rusty_crew_core_protocol::ChatCompletionsPromptCachingPolicy::Disabled
        {
            PromptCacheTransport::None
        } else {
            PromptCacheTransport::OpenrouterAnthropic
        },
        metadata_json: serde_json::Value::Object(metadata),
        revision: provider.revision,
        created_at: provider.created_at.clone(),
        updated_at: provider.updated_at.clone(),
    };
    endpoint.validate()?;
    let configuration = ModelConfigurationRecord {
        model_config_id: provider.alias.clone(),
        endpoint_id: provider.alias.clone(),
        status: provider.status,
        display_name: provider.display_name.clone(),
        description: provider.description.clone(),
        model_id: provider.model_id.clone(),
        context_window_tokens: provider.context_window_tokens,
        max_output_tokens: provider.max_output_tokens,
        temperature_milli: provider.temperature_milli,
        reasoning_effort: provider.reasoning_effort.clone(),
        reasoning_format: provider.reasoning_format.clone(),
        reasoning_history: provider.reasoning_history,
        reasoning_budget_tokens: provider.reasoning_budget_tokens,
        thinking_mode: provider.thinking_mode,
        prompt_caching_policy: provider.prompt_caching,
        capabilities: Default::default(),
        metadata_json: provider.metadata_json.clone(),
        revision: provider.revision,
        created_at: provider.created_at.clone(),
        updated_at: provider.updated_at.clone(),
    };
    configuration.validate_for_endpoint(&endpoint)?;
    Ok((endpoint, configuration))
}

fn endpoint_fields_changed(a: &ModelEndpointRecord, b: &ModelEndpointRecord) -> bool {
    a.status != b.status
        || a.display_name != b.display_name
        || a.description != b.description
        || a.base_url != b.base_url
        || a.protocol != b.protocol
        || a.wire_dialect != b.wire_dialect
        || a.auth_scheme != b.auth_scheme
        || a.credential_id != b.credential_id
        || a.prompt_cache_transport != b.prompt_cache_transport
        || a.metadata_json != b.metadata_json
}

fn insert_endpoint_if_absent<C: GenericClient>(
    client: &mut C,
    schema: &str,
    record: &ModelEndpointRecord,
) -> CoreResult<()> {
    let status = enum_text(record.status)?;
    let protocol = enum_text(record.protocol)?;
    client
        .execute(
            &format!(
                "INSERT INTO {schema}.model_endpoints
                 (endpoint_id,status,protocol,credential_id,record_json,revision,created_at,updated_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(endpoint_id) DO NOTHING"
            ),
            &[
                &record.endpoint_id,
                &status,
                &protocol,
                &record.credential_id,
                &to_json_text(record)?,
                &(record.revision as i64),
                &record.created_at,
                &record.updated_at,
            ],
        )
        .map_err(|error| postgres_error("backfill PostgreSQL model endpoint", error))?;
    Ok(())
}

fn insert_configuration_if_absent<C: GenericClient>(
    client: &mut C,
    schema: &str,
    record: &ModelConfigurationRecord,
) -> CoreResult<()> {
    let status = enum_text(record.status)?;
    client
        .execute(
            &format!(
                "INSERT INTO {schema}.model_configurations
                 (model_config_id,endpoint_id,status,model_id,record_json,revision,created_at,updated_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(model_config_id) DO NOTHING"
            ),
            &[
                &record.model_config_id,
                &record.endpoint_id,
                &status,
                &record.model_id,
                &to_json_text(record)?,
                &(record.revision as i64),
                &record.created_at,
                &record.updated_at,
            ],
        )
        .map_err(|error| postgres_error("backfill PostgreSQL model configuration", error))?;
    Ok(())
}

fn update_endpoint_record<C: GenericClient>(
    client: &mut C,
    schema: &str,
    record: &ModelEndpointRecord,
    expected: u64,
) -> CoreResult<()> {
    let changed = client
        .execute(
            &format!(
                "UPDATE {schema}.model_endpoints SET status=$2,protocol=$3,credential_id=$4,
                 record_json=$5,revision=$6,updated_at=$7 WHERE endpoint_id=$1 AND revision=$8"
            ),
            &[
                &record.endpoint_id,
                &enum_text(record.status)?,
                &enum_text(record.protocol)?,
                &record.credential_id,
                &to_json_text(record)?,
                &(record.revision as i64),
                &record.updated_at,
                &(expected as i64),
            ],
        )
        .map_err(|error| postgres_error("update PostgreSQL normalized endpoint", error))?;
    if changed != 1 {
        return Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            format!("model endpoint {} changed concurrently", record.endpoint_id),
        ));
    }
    Ok(())
}

fn update_configuration_record<C: GenericClient>(
    client: &mut C,
    schema: &str,
    record: &ModelConfigurationRecord,
    expected: u64,
) -> CoreResult<()> {
    let changed = client
        .execute(
            &format!(
                "UPDATE {schema}.model_configurations SET endpoint_id=$2,status=$3,model_id=$4,
         record_json=$5,revision=$6,updated_at=$7 WHERE model_config_id=$1 AND revision=$8"
            ),
            &[
                &record.model_config_id,
                &record.endpoint_id,
                &enum_text(record.status)?,
                &record.model_id,
                &to_json_text(record)?,
                &(record.revision as i64),
                &record.updated_at,
                &(expected as i64),
            ],
        )
        .map_err(|error| postgres_error("update PostgreSQL normalized configuration", error))?;
    if changed != 1 {
        return Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            format!(
                "model configuration {} changed concurrently",
                record.model_config_id
            ),
        ));
    }
    Ok(())
}

fn get_endpoint<C: GenericClient>(
    client: &mut C,
    schema: &str,
    id: &str,
) -> CoreResult<Option<ModelEndpointRecord>> {
    client
        .query_opt(
            &format!("SELECT record_json FROM {schema}.model_endpoints WHERE endpoint_id=$1"),
            &[&id],
        )
        .map_err(|error| postgres_error("get PostgreSQL model endpoint", error))?
        .map(|row| parse_json(row.get::<_, String>(0), "model endpoint record_json"))
        .transpose()
}

fn get_configuration<C: GenericClient>(
    client: &mut C,
    schema: &str,
    id: &str,
) -> CoreResult<Option<ModelConfigurationRecord>> {
    client
        .query_opt(
            &format!(
                "SELECT record_json FROM {schema}.model_configurations WHERE model_config_id=$1"
            ),
            &[&id],
        )
        .map_err(|error| postgres_error("get PostgreSQL model configuration", error))?
        .map(|row| parse_json(row.get::<_, String>(0), "model configuration record_json"))
        .transpose()
}

fn validate_postgres_endpoint_credential<C: GenericClient>(
    client: &mut C,
    schema: &str,
    write: &ModelEndpointWrite,
) -> CoreResult<()> {
    let Some(id) = write.credential_id.as_deref() else {
        return Ok(());
    };
    let kind = client
        .query_opt(
            &format!(
                "SELECT credential_kind FROM {schema}.service_credentials WHERE credential_id=$1"
            ),
            &[&id],
        )
        .map_err(|error| postgres_error("load PostgreSQL endpoint credential", error))?
        .map(|row| row.get::<_, String>(0))
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("service credential {id} not found"),
            )
        })?;
    if matches!(
        (write.auth_scheme, kind.as_str()),
        (
            ModelEndpointAuthScheme::BearerApiKey,
            "api_key" | "legacy_raw_api_key"
        ) | (ModelEndpointAuthScheme::OpenAiCodexOauth, "openai_oauth")
    ) {
        Ok(())
    } else {
        Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!(
                "model endpoint {} auth scheme is incompatible with credential kind {kind}",
                write.endpoint_id
            ),
        ))
    }
}

fn validate_revision(
    kind: &str,
    id: &str,
    expected: Option<u64>,
    found: Option<u64>,
) -> CoreResult<()> {
    if let Some(expected) = expected {
        let found = found.unwrap_or(0);
        if expected != found {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!("{kind} {id} revision mismatch: expected {expected}, found {found}"),
            ));
        }
    }
    Ok(())
}

fn enum_text<T: serde::Serialize>(value: T) -> CoreResult<String> {
    serde_json::to_value(value)
        .map_err(|error| CoreError::new(CoreErrorKind::InternalError, error.to_string()))?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::InternalError,
                "enum did not serialize as text",
            )
        })
}

fn parse_json<T: serde::de::DeserializeOwned>(value: String, label: &str) -> CoreResult<T> {
    serde_json::from_str(&value).map_err(|error| {
        CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("parse {label}: {error}"),
        )
    })
}
