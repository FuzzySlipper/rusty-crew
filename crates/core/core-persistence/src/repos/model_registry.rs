//! Normalized model endpoint and model configuration persistence.

use super::super::*;

pub(crate) const LEGACY_MISSING_BASE_URL_METADATA_KEY: &str = "legacyBaseUrlMissing";
pub(crate) const LEGACY_MISSING_BASE_URL_SENTINEL: &str = "http://legacy-unconfigured.invalid";

pub(crate) fn migrate_v75_add_model_registry(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "CREATE TABLE model_endpoints (
            endpoint_id TEXT PRIMARY KEY,
            status TEXT NOT NULL,
            protocol TEXT NOT NULL,
            credential_id TEXT,
            record_json TEXT NOT NULL,
            revision INTEGER NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(credential_id) REFERENCES service_credentials(credential_id)
         );
         CREATE INDEX model_endpoints_status_idx
            ON model_endpoints(status, updated_at DESC, endpoint_id);
         CREATE INDEX model_endpoints_protocol_idx
            ON model_endpoints(protocol, endpoint_id);
         CREATE TABLE model_configurations (
            model_config_id TEXT PRIMARY KEY,
            endpoint_id TEXT NOT NULL,
            status TEXT NOT NULL,
            model_id TEXT NOT NULL,
            record_json TEXT NOT NULL,
            revision INTEGER NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(endpoint_id) REFERENCES model_endpoints(endpoint_id)
         );
         CREATE INDEX model_configurations_endpoint_idx
            ON model_configurations(endpoint_id, status, model_config_id);
         CREATE INDEX model_configurations_model_idx
            ON model_configurations(model_id, model_config_id);",
    )
    .map_err(|error| persistence_error("create normalized model registries", error))?;
    let report = backfill_legacy_model_registry_in_tx(tx, false)?;
    ensure_migration_backfill_is_safe(&report)
}

pub(crate) fn ensure_migration_backfill_is_safe(
    report: &ModelEndpointBackfillReport,
) -> CoreResult<()> {
    let unequal = report
        .joined_projection_equality
        .iter()
        .filter(|entry| !entry.projection_equal)
        .count();
    if report.representability_errors.is_empty() && unequal == 0 {
        return Ok(());
    }
    Err(CoreError::new(
        CoreErrorKind::InvalidInput,
        format!(
            "legacy model registry backfill failed: {} unrepresentable row(s), {unequal} unequal joined projection(s): {}",
            report.representability_errors.len(),
            report.representability_errors
                .iter()
                .map(|entry| format!(
                    "{}[{}]={}",
                    entry.legacy_alias, entry.field, entry.reason
                ))
                .chain(
                    report
                        .joined_projection_equality
                        .iter()
                        .filter(|entry| !entry.projection_equal)
                        .map(|entry| {
                            format!(
                                "{}=[{}]",
                                entry.legacy_alias,
                                entry.differing_fields.join(",")
                            )
                        }),
                )
                .collect::<Vec<_>>()
                .join(";")
        ),
    ))
}

impl CoordinationStore {
    pub fn apply_model_registry_logical_import(
        &self,
        bundle: &LogicalStorageExportBundle,
        dry_run: &LogicalStorageImportDryRun,
    ) -> CoreResult<Vec<LogicalStorageApplyProof>> {
        crate::sqlite_runtime_import::validate_model_registry_logical_import_envelope(
            bundle,
            dry_run,
            crate::sqlite_runtime_import::SQLITE_MODEL_REGISTRY_TARGET_BACKEND,
            "SQLite",
        )?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start model registry logical import", error))?;
        let validation =
            crate::sqlite_runtime_import::validate_logical_storage_import(&tx, bundle, dry_run)?;
        if !validation.can_apply() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "model registry logical import dry-run did not pass",
            ));
        }
        let (mut endpoints, mut configurations) = logical_model_records(bundle)?;
        endpoints.sort_by(|left, right| left.endpoint_id.cmp(&right.endpoint_id));
        configurations.sort_by(|left, right| left.model_config_id.cmp(&right.model_config_id));

        for endpoint in &endpoints {
            endpoint.validate()?;
            validate_import_endpoint_credential(&tx, endpoint)?;
            match get_model_endpoint_in_conn(&tx, &endpoint.endpoint_id)? {
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
                None => write_model_endpoint_in_tx(&tx, endpoint, None)?,
            }
        }
        for configuration in &configurations {
            let endpoint = get_model_endpoint_in_conn(&tx, &configuration.endpoint_id)?
                .ok_or_else(|| {
                    CoreError::new(
                        CoreErrorKind::NotFound,
                        format!(
                            "model endpoint {} not found during logical import",
                            configuration.endpoint_id
                        ),
                    )
                })?;
            configuration.validate_for_endpoint(&endpoint)?;
            match get_model_configuration_in_conn(&tx, &configuration.model_config_id)? {
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
                None => write_model_configuration_in_tx(&tx, configuration, None)?,
            }
            sync_normalized_configuration_shadow_in_tx(&tx, &endpoint, configuration)?;
        }

        tx.execute(
            "INSERT INTO runtime_import_batches (
                import_batch_id,source_system,source_label,source_snapshot_ref,notes,imported_at
             ) VALUES (?1,?2,?3,?4,?5,?6)",
            params![
                dry_run.import_batch_id,
                bundle.source.backend,
                bundle.source.backend_label,
                bundle.source.snapshot_ref,
                "model endpoint/configuration logical import",
                dry_run.validation_time,
            ],
        )
        .map_err(|error| persistence_error("record model registry import batch", error))?;

        let readback_endpoints = endpoints
            .iter()
            .map(|record| get_model_endpoint_in_conn(&tx, &record.endpoint_id))
            .collect::<CoreResult<Option<Vec<_>>>>()?
            .ok_or_else(|| CoreError::new(CoreErrorKind::NotFound, "imported endpoint missing"))?;
        let readback_configurations = configurations
            .iter()
            .map(|record| get_model_configuration_in_conn(&tx, &record.model_config_id))
            .collect::<CoreResult<Option<Vec<_>>>>()?
            .ok_or_else(|| {
                CoreError::new(CoreErrorKind::NotFound, "imported configuration missing")
            })?;
        let applied_repositories =
            crate::sqlite_runtime_import::model_registry_logical_repositories(
                &readback_endpoints,
                &readback_configurations,
                &dry_run.validation_time,
            )?;
        let proofs =
            model_registry_apply_proofs(bundle, &applied_repositories, &dry_run.import_batch_id)?;
        if proofs.iter().any(|proof| !proof.verified) {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "model registry post-import count/checksum proof failed",
            ));
        }
        tx.commit()
            .map_err(|error| persistence_error("commit model registry logical import", error))?;
        Ok(proofs)
    }

    pub fn upsert_model_endpoint(
        &self,
        write: &ModelEndpointWrite,
    ) -> CoreResult<ModelEndpointRecord> {
        write.validate()?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start model endpoint upsert", error))?;
        validate_endpoint_credential_in_tx(&tx, write)?;
        let existing = get_model_endpoint_in_conn(&tx, &write.endpoint_id)?;
        validate_registry_revision(
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
        write_model_endpoint_in_tx(&tx, &record, existing.as_ref().map(|row| row.revision))?;
        sync_normalized_endpoint_shadows_in_tx(&tx, &record)?;
        tx.commit()
            .map_err(|error| persistence_error("commit model endpoint upsert", error))?;
        Ok(record)
    }

    pub fn get_model_endpoint(&self, endpoint_id: &str) -> CoreResult<Option<ModelEndpointRecord>> {
        ModelEndpointQuery {
            endpoint_id: Some(endpoint_id.to_string()),
            ..Default::default()
        }
        .validate()?;
        get_model_endpoint_in_conn(&*self.conn()?, endpoint_id)
    }

    pub fn list_model_endpoints(
        &self,
        query: &ModelEndpointQuery,
    ) -> CoreResult<Vec<ModelEndpointRecord>> {
        query.validate()?;
        let status = query.status.map(enum_storage_text).transpose()?;
        let limit = query.limit.unwrap_or(100).clamp(1, 1_000) as i64;
        let offset = query.offset.unwrap_or(0) as i64;
        let conn = self.conn()?;
        let mut statement = conn
            .prepare(
                "SELECT record_json FROM model_endpoints
                 WHERE (?1 IS NULL OR endpoint_id = ?1)
                   AND (?2 IS NULL OR status = ?2)
                 ORDER BY updated_at DESC, endpoint_id ASC LIMIT ?3 OFFSET ?4",
            )
            .map_err(|error| persistence_error("prepare model endpoint query", error))?;
        let rows = statement
            .query_map(params![query.endpoint_id, status, limit, offset], |row| {
                parse_record_json(row.get::<_, String>(0)?)
            })
            .map_err(|error| persistence_error("query model endpoints", error))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| persistence_error("load model endpoints", error))
    }

    pub fn delete_model_endpoint(
        &self,
        delete: &ModelEndpointDelete,
    ) -> CoreResult<ModelEndpointRecord> {
        delete.validate()?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start model endpoint delete", error))?;
        let existing = get_model_endpoint_in_conn(&tx, &delete.endpoint_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("model endpoint {} not found", delete.endpoint_id),
            )
        })?;
        validate_registry_revision(
            "model endpoint",
            &delete.endpoint_id,
            Some(delete.expected_revision),
            Some(existing.revision),
        )?;
        let configuration_count: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM model_configurations WHERE endpoint_id=?1",
                params![delete.endpoint_id],
                |row| row.get(0),
            )
            .map_err(|error| persistence_error("count endpoint model configurations", error))?;
        if configuration_count != 0 {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "model endpoint {} is still referenced by {configuration_count} model configuration(s)",
                    delete.endpoint_id
                ),
            ));
        }
        let changed = tx
            .execute(
                "DELETE FROM model_endpoints WHERE endpoint_id=?1 AND revision=?2",
                params![delete.endpoint_id, delete.expected_revision as i64],
            )
            .map_err(|error| persistence_error("delete model endpoint", error))?;
        if changed != 1 {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!("model endpoint {} changed concurrently", delete.endpoint_id),
            ));
        }
        tx.commit()
            .map_err(|error| persistence_error("commit model endpoint delete", error))?;
        Ok(existing)
    }

    pub fn upsert_model_configuration(
        &self,
        write: &ModelConfigurationWrite,
    ) -> CoreResult<ModelConfigurationRecord> {
        write.validate()?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start model configuration upsert", error))?;
        let endpoint = get_model_endpoint_in_conn(&tx, &write.endpoint_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("model endpoint {} not found", write.endpoint_id),
            )
        })?;
        write.validate_for_endpoint(&endpoint)?;
        let existing = get_model_configuration_in_conn(&tx, &write.model_config_id)?;
        validate_registry_revision(
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
        write_model_configuration_in_tx(&tx, &record, existing.as_ref().map(|row| row.revision))?;
        sync_normalized_configuration_shadow_in_tx(&tx, &endpoint, &record)?;
        tx.commit()
            .map_err(|error| persistence_error("commit model configuration upsert", error))?;
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
        get_model_configuration_in_conn(&*self.conn()?, model_config_id)
    }

    pub fn list_model_configurations(
        &self,
        query: &ModelConfigurationQuery,
    ) -> CoreResult<Vec<ModelConfigurationRecord>> {
        query.validate()?;
        let status = query.status.map(enum_storage_text).transpose()?;
        let limit = query.limit.unwrap_or(100).clamp(1, 1_000) as i64;
        let offset = query.offset.unwrap_or(0) as i64;
        let conn = self.conn()?;
        let mut statement = conn
            .prepare(
                "SELECT record_json FROM model_configurations
                 WHERE (?1 IS NULL OR model_config_id = ?1)
                   AND (?2 IS NULL OR endpoint_id = ?2)
                   AND (?3 IS NULL OR status = ?3)
                 ORDER BY updated_at DESC, model_config_id ASC LIMIT ?4 OFFSET ?5",
            )
            .map_err(|error| persistence_error("prepare model configuration query", error))?;
        let rows = statement
            .query_map(
                params![
                    query.model_config_id,
                    query.endpoint_id,
                    status,
                    limit,
                    offset
                ],
                |row| parse_record_json(row.get::<_, String>(0)?),
            )
            .map_err(|error| persistence_error("query model configurations", error))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| persistence_error("load model configurations", error))
    }

    pub fn delete_model_configuration(
        &self,
        delete: &ModelConfigurationDelete,
    ) -> CoreResult<ModelConfigurationRecord> {
        delete.validate()?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|error| persistence_error("start model configuration delete", error))?;
        let existing =
            get_model_configuration_in_conn(&tx, &delete.model_config_id)?.ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    format!("model configuration {} not found", delete.model_config_id),
                )
            })?;
        validate_registry_revision(
            "model configuration",
            &delete.model_config_id,
            Some(delete.expected_revision),
            Some(existing.revision),
        )?;
        let mut statement = tx
            .prepare("SELECT profile_id, active_runtime_settings_json FROM profile_registry")
            .map_err(|error| {
                persistence_error("prepare model configuration profile references", error)
            })?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|error| {
                persistence_error("query model configuration profile references", error)
            })?;
        let mut referencing_profiles = Vec::new();
        for row in rows {
            let (profile_id, settings_json) = row.map_err(|error| {
                persistence_error("load model configuration profile reference", error)
            })?;
            let settings: JsonValue = from_json_text(&settings_json).map_err(|error| {
                persistence_error("parse profile model configuration reference", error)
            })?;
            if crate::effective_profile_model_config_id(&settings).as_deref()
                == Some(delete.model_config_id.as_str())
            {
                referencing_profiles.push(profile_id);
            }
        }
        drop(statement);
        if !referencing_profiles.is_empty() {
            referencing_profiles.sort();
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "model configuration {} is still referenced by profile(s): {}",
                    delete.model_config_id,
                    referencing_profiles.join(", ")
                ),
            ));
        }
        tx.execute(
            "INSERT INTO module_simple_kv_entries (
                scope_type, scope_id, entry_key, value_json, revision,
                created_at, updated_at, expires_at
             ) VALUES ('model_registry', 'deleted_model_configurations', ?1, '{}', 1, ?2, ?2, NULL)
             ON CONFLICT(scope_type, scope_id, entry_key) DO UPDATE SET
                revision = module_simple_kv_entries.revision + 1,
                updated_at = excluded.updated_at",
            params![delete.model_config_id, existing.updated_at],
        )
        .map_err(|error| persistence_error("record deleted model configuration", error))?;
        let changed = tx
            .execute(
                "DELETE FROM model_configurations WHERE model_config_id=?1 AND revision=?2",
                params![delete.model_config_id, delete.expected_revision as i64],
            )
            .map_err(|error| persistence_error("delete model configuration", error))?;
        if changed != 1 {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "model configuration {} changed concurrently",
                    delete.model_config_id
                ),
            ));
        }
        tx.execute(
            "DELETE FROM model_providers WHERE alias=?1",
            params![delete.model_config_id],
        )
        .map_err(|error| persistence_error("delete legacy model provider shadow", error))?;
        tx.commit()
            .map_err(|error| persistence_error("commit model configuration delete", error))?;
        Ok(existing)
    }

    pub fn backfill_legacy_model_registry(
        &self,
        dry_run: bool,
    ) -> CoreResult<ModelEndpointBackfillReport> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start legacy model registry backfill", error))?;
        let report = backfill_legacy_model_registry_in_tx(&tx, dry_run)?;
        if dry_run {
            tx.rollback()
                .map_err(|error| persistence_error("rollback dry-run model backfill", error))?;
        } else {
            tx.commit()
                .map_err(|error| persistence_error("commit model registry backfill", error))?;
        }
        Ok(report)
    }
}

pub(crate) fn logical_model_records(
    bundle: &LogicalStorageExportBundle,
) -> CoreResult<(Vec<ModelEndpointRecord>, Vec<ModelConfigurationRecord>)> {
    let mut endpoints = Vec::new();
    let mut configurations = Vec::new();
    for repository in &bundle.repositories {
        for record in &repository.records {
            match &record.payload {
                LogicalStorageRecordPayload::ModelEndpoint(endpoint)
                    if repository.repository_id == "model_endpoints" =>
                {
                    endpoints.push(endpoint.as_ref().clone());
                }
                LogicalStorageRecordPayload::ModelConfiguration(configuration)
                    if repository.repository_id == "model_configurations" =>
                {
                    configurations.push(configuration.as_ref().clone());
                }
                _ => {}
            }
        }
    }
    Ok((endpoints, configurations))
}

pub(crate) fn validate_import_endpoint_credential(
    conn: &Connection,
    endpoint: &ModelEndpointRecord,
) -> CoreResult<()> {
    let Some(credential_id) = endpoint.credential_id.as_deref() else {
        return Ok(());
    };
    let (kind, has_secret) = conn
        .query_row(
            "SELECT credential_kind, secret_ciphertext IS NOT NULL
             FROM service_credentials WHERE credential_id=?1",
            params![credential_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?)),
        )
        .optional()
        .map_err(|error| persistence_error("load logical import credential", error))?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("service credential {credential_id} not found"),
            )
        })?;
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

pub(crate) fn model_registry_apply_proofs(
    expected: &LogicalStorageExportBundle,
    applied: &[LogicalStorageRepositoryBundle],
    import_batch_id: &str,
) -> CoreResult<Vec<LogicalStorageApplyProof>> {
    ["model_endpoints", "model_configurations"]
        .into_iter()
        .map(|repository_id| {
            let expected = expected
                .repositories
                .iter()
                .find(|repository| repository.repository_id == repository_id)
                .ok_or_else(|| {
                    CoreError::new(
                        CoreErrorKind::InvalidInput,
                        format!("logical bundle is missing repository {repository_id}"),
                    )
                })?;
            let applied = applied
                .iter()
                .find(|repository| repository.repository_id == repository_id)
                .expect("model registry exporter always returns both repositories");
            let applied_checksum = applied.checksum.clone().unwrap_or_default();
            Ok(LogicalStorageApplyProof {
                import_batch_id: import_batch_id.to_string(),
                repository_id: repository_id.to_string(),
                expected_count: expected.exported_count,
                applied_count: applied.exported_count,
                expected_checksum: expected.checksum.clone(),
                verified: expected.exported_count == applied.exported_count
                    && expected.checksum.as_deref() == Some(applied_checksum.as_str()),
                applied_checksum,
            })
        })
        .collect()
}

pub(crate) fn sync_legacy_provider_to_normalized_in_tx(
    tx: &rusqlite::Transaction<'_>,
    provider: &ModelProviderRecord,
) -> CoreResult<()> {
    let existing_configuration = get_model_configuration_in_conn(tx, &provider.alias)?;
    let (desired_endpoint, mut desired_configuration) =
        match normalized_records_from_provider(provider) {
            Ok(records) => records,
            Err(error)
                if error.kind == CoreErrorKind::InvalidInput
                    && existing_configuration.is_none() =>
            {
                // Legacy writes predate the normalized contract and may be intentionally
                // incomplete while an operator is still configuring them. Preserve those
                // rollback rows; the deterministic backfill report identifies them as
                // unrepresentable once enough endpoint facts exist to project safely.
                return Ok(());
            }
            Err(error) => return Err(error),
        };
    let endpoint_id = existing_configuration.as_ref().map_or_else(
        || provider.alias.clone(),
        |record| record.endpoint_id.clone(),
    );
    let existing_endpoint = get_model_endpoint_in_conn(tx, &endpoint_id)?;
    let mut desired_endpoint = ModelEndpointRecord {
        endpoint_id: endpoint_id.clone(),
        ..desired_endpoint
    };
    desired_configuration.endpoint_id = endpoint_id.clone();

    if let Some(existing_endpoint) = existing_endpoint.as_ref() {
        // Legacy joined reads project lifecycle/display fields from the model
        // configuration. They therefore cannot express edits to the shared
        // endpoint copies of those fields; preserve endpoint authority.
        desired_endpoint.status = existing_endpoint.status;
        desired_endpoint.display_name = existing_endpoint.display_name.clone();
        desired_endpoint.description = existing_endpoint.description.clone();
        let projected_vendor_label = existing_endpoint
            .metadata_json
            .get("legacyVendorLabel")
            .and_then(JsonValue::as_str)
            .unwrap_or("custom");
        if provider.provider_kind == projected_vendor_label {
            desired_endpoint.metadata_json = existing_endpoint.metadata_json.clone();
        }
        let shared_count: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM model_configurations WHERE endpoint_id=?1",
                params![endpoint_id],
                |row| row.get(0),
            )
            .map_err(|error| {
                persistence_error("count shared model endpoint configurations", error)
            })?;
        let endpoint_changed = endpoint_fields_changed(existing_endpoint, &desired_endpoint);
        if shared_count > 1 && endpoint_changed {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "legacy_provider_shared_endpoint_conflict",
            ));
        }
        if endpoint_changed {
            desired_endpoint.revision = existing_endpoint.revision + 1;
            desired_endpoint.created_at = existing_endpoint.created_at.clone();
            write_model_endpoint_in_tx(tx, &desired_endpoint, Some(existing_endpoint.revision))?;
        }
    } else {
        write_model_endpoint_in_tx(tx, &desired_endpoint, None)?;
    }

    if let Some(existing) = existing_configuration.as_ref() {
        desired_configuration.revision = existing.revision + 1;
        desired_configuration.created_at = existing.created_at.clone();
        write_model_configuration_in_tx(tx, &desired_configuration, Some(existing.revision))?;
    } else {
        write_model_configuration_in_tx(tx, &desired_configuration, None)?;
    }
    Ok(())
}

fn normalized_records_from_provider(
    provider: &ModelProviderRecord,
) -> CoreResult<(ModelEndpointRecord, ModelConfigurationRecord)> {
    let mut endpoint_metadata = serde_json::Map::new();
    endpoint_metadata.insert(
        "legacyVendorLabel".to_string(),
        JsonValue::String(provider.provider_kind.clone()),
    );
    let base_url = legacy_endpoint_base_url(provider, &mut endpoint_metadata);
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
            ChatCompletionsWireDialect::Standard => ModelEndpointWireDialect::Standard,
            ChatCompletionsWireDialect::Kimi => ModelEndpointWireDialect::Kimi,
            ChatCompletionsWireDialect::Glm => ModelEndpointWireDialect::Glm,
            ChatCompletionsWireDialect::Qwen => ModelEndpointWireDialect::Qwen,
            ChatCompletionsWireDialect::Deepseek => ModelEndpointWireDialect::Deepseek,
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
            == ChatCompletionsPromptCachingPolicy::Disabled
        {
            PromptCacheTransport::None
        } else {
            PromptCacheTransport::OpenrouterAnthropic
        },
        metadata_json: JsonValue::Object(endpoint_metadata),
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

fn sync_normalized_endpoint_shadows_in_tx(
    tx: &rusqlite::Transaction<'_>,
    endpoint: &ModelEndpointRecord,
) -> CoreResult<()> {
    let mut statement = tx.prepare(
        "SELECT record_json FROM model_configurations WHERE endpoint_id=?1 ORDER BY model_config_id",
    ).map_err(|error| persistence_error("prepare endpoint shadow configurations", error))?;
    let configurations = statement
        .query_map(params![endpoint.endpoint_id], |row| {
            parse_record_json(row.get::<_, String>(0)?)
        })
        .map_err(|error| persistence_error("query endpoint shadow configurations", error))?
        .collect::<Result<Vec<ModelConfigurationRecord>, _>>()
        .map_err(|error| persistence_error("load endpoint shadow configurations", error))?;
    for configuration in configurations {
        sync_normalized_configuration_shadow_in_tx(tx, endpoint, &configuration)?;
    }
    Ok(())
}

fn sync_normalized_configuration_shadow_in_tx(
    tx: &rusqlite::Transaction<'_>,
    endpoint: &ModelEndpointRecord,
    configuration: &ModelConfigurationRecord,
) -> CoreResult<()> {
    let provider_kind = endpoint
        .metadata_json
        .get("legacyVendorLabel")
        .and_then(JsonValue::as_str)
        .unwrap_or("custom");
    let responses_dialect = match endpoint.wire_dialect {
        ModelEndpointWireDialect::OpenaiStateful => Some("openai_stateful"),
        ModelEndpointWireDialect::OpenaiStateless => Some("openai_stateless"),
        ModelEndpointWireDialect::GenericStateless => Some("generic_stateless"),
        ModelEndpointWireDialect::Deepseek
            if endpoint.protocol == ModelProviderProtocol::Responses =>
        {
            Some("deepseek")
        }
        ModelEndpointWireDialect::Meta => Some("meta"),
        _ => None,
    };
    let chat_dialect = match endpoint.wire_dialect {
        ModelEndpointWireDialect::Kimi => "kimi",
        ModelEndpointWireDialect::Glm => "glm",
        ModelEndpointWireDialect::Qwen => "qwen",
        ModelEndpointWireDialect::Deepseek => "deepseek",
        _ => "standard",
    };
    let changed = tx
        .execute(
            "UPDATE model_providers SET status=?2,protocol=?3,provider_kind=?4,display_name=?5,
            description=?6,base_url=?7,model_id=?8,context_window_tokens=?9,max_output_tokens=?10,
            temperature_milli=?11,reasoning_effort=?12,reasoning_format=?13,responses_dialect=?14,
            chat_completions_dialect=?15,thinking_mode=?16,reasoning_history=?17,
            reasoning_budget_tokens=?18,prompt_caching=?19,metadata_json=?20,credential_id=?21,
            revision=revision+1,updated_at=?22 WHERE alias=?1",
            params![
                configuration.model_config_id,
                enum_storage_text(configuration.status)?,
                enum_storage_text(endpoint.protocol)?,
                provider_kind,
                configuration.display_name,
                configuration.description,
                endpoint.base_url,
                configuration.model_id,
                configuration.context_window_tokens.map(i64::from),
                configuration.max_output_tokens.map(i64::from),
                configuration.temperature_milli.map(i64::from),
                configuration.reasoning_effort,
                configuration.reasoning_format,
                responses_dialect,
                chat_dialect,
                enum_storage_text(configuration.thinking_mode)?,
                enum_storage_text(configuration.reasoning_history)?,
                configuration.reasoning_budget_tokens.map(i64::from),
                enum_storage_text(configuration.prompt_caching_policy)?,
                to_json_text(&configuration.metadata_json)?,
                endpoint.credential_id,
                configuration.updated_at
            ],
        )
        .map_err(|error| persistence_error("update normalized legacy provider shadow", error))?;
    if changed == 0 {
        tx.execute(
            "INSERT INTO model_providers (alias,status,protocol,provider_kind,display_name,description,base_url,model_id,
                context_window_tokens,max_output_tokens,temperature_milli,reasoning_effort,reasoning_format,responses_dialect,
                chat_completions_dialect,thinking_mode,reasoning_history,reasoning_budget_tokens,prompt_caching,
                secret_ciphertext,secret_updated_at,metadata_json,revision,created_at,updated_at,credential_id)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,NULL,NULL,?20,1,?21,?22,?23)",
            params![configuration.model_config_id, enum_storage_text(configuration.status)?, enum_storage_text(endpoint.protocol)?, provider_kind,
                configuration.display_name, configuration.description, endpoint.base_url, configuration.model_id,
                configuration.context_window_tokens.map(i64::from), configuration.max_output_tokens.map(i64::from),
                configuration.temperature_milli.map(i64::from), configuration.reasoning_effort, configuration.reasoning_format,
                responses_dialect, chat_dialect, enum_storage_text(configuration.thinking_mode)?, enum_storage_text(configuration.reasoning_history)?,
                configuration.reasoning_budget_tokens.map(i64::from), enum_storage_text(configuration.prompt_caching_policy)?,
                to_json_text(&configuration.metadata_json)?, configuration.created_at, configuration.updated_at, endpoint.credential_id],
        ).map_err(|error| persistence_error("insert normalized legacy provider shadow", error))?;
    }
    Ok(())
}

fn validate_registry_revision(
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

fn validate_endpoint_credential_in_tx(
    tx: &rusqlite::Transaction<'_>,
    write: &ModelEndpointWrite,
) -> CoreResult<()> {
    let Some(credential_id) = write.credential_id.as_deref() else {
        return Ok(());
    };
    let kind = tx
        .query_row(
            "SELECT credential_kind FROM service_credentials WHERE credential_id = ?1",
            params![credential_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| persistence_error("load endpoint credential kind", error))?
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("service credential {credential_id} not found"),
            )
        })?;
    let compatible = matches!(
        (write.auth_scheme, kind.as_str()),
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
                write.endpoint_id
            ),
        ))
    }
}

fn write_model_endpoint_in_tx(
    tx: &rusqlite::Transaction<'_>,
    record: &ModelEndpointRecord,
    previous_revision: Option<u64>,
) -> CoreResult<()> {
    let status = enum_storage_text(record.status)?;
    let protocol = enum_storage_text(record.protocol)?;
    let json = to_json_text(record)?;
    if let Some(previous_revision) = previous_revision {
        let changed = tx
            .execute(
                "UPDATE model_endpoints SET status=?2, protocol=?3, credential_id=?4,
                    record_json=?5, revision=?6, updated_at=?7
                 WHERE endpoint_id=?1 AND revision=?8",
                params![
                    record.endpoint_id,
                    status,
                    protocol,
                    record.credential_id,
                    json,
                    record.revision as i64,
                    record.updated_at,
                    previous_revision as i64
                ],
            )
            .map_err(|error| persistence_error("update model endpoint", error))?;
        if changed != 1 {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!("model endpoint {} changed concurrently", record.endpoint_id),
            ));
        }
    } else {
        tx.execute(
            "INSERT INTO model_endpoints
             (endpoint_id,status,protocol,credential_id,record_json,revision,created_at,updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                record.endpoint_id,
                status,
                protocol,
                record.credential_id,
                json,
                record.revision as i64,
                record.created_at,
                record.updated_at
            ],
        )
        .map_err(|error| persistence_error("insert model endpoint", error))?;
    }
    Ok(())
}

fn write_model_configuration_in_tx(
    tx: &rusqlite::Transaction<'_>,
    record: &ModelConfigurationRecord,
    previous_revision: Option<u64>,
) -> CoreResult<()> {
    let status = enum_storage_text(record.status)?;
    let json = to_json_text(record)?;
    if let Some(previous_revision) = previous_revision {
        let changed = tx
            .execute(
                "UPDATE model_configurations SET endpoint_id=?2,status=?3,model_id=?4,
                record_json=?5,revision=?6,updated_at=?7
             WHERE model_config_id=?1 AND revision=?8",
                params![
                    record.model_config_id,
                    record.endpoint_id,
                    status,
                    record.model_id,
                    json,
                    record.revision as i64,
                    record.updated_at,
                    previous_revision as i64
                ],
            )
            .map_err(|error| persistence_error("update model configuration", error))?;
        if changed != 1 {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "model configuration {} changed concurrently",
                    record.model_config_id
                ),
            ));
        }
    } else {
        tx.execute(
            "INSERT INTO model_configurations
             (model_config_id,endpoint_id,status,model_id,record_json,revision,created_at,updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![record.model_config_id, record.endpoint_id, status, record.model_id, json,
                record.revision as i64, record.created_at, record.updated_at],
        ).map_err(|error| persistence_error("insert model configuration", error))?;
    }
    Ok(())
}

fn get_model_endpoint_in_conn(
    conn: &Connection,
    endpoint_id: &str,
) -> CoreResult<Option<ModelEndpointRecord>> {
    conn.query_row(
        "SELECT record_json FROM model_endpoints WHERE endpoint_id=?1",
        params![endpoint_id],
        |row| parse_record_json(row.get::<_, String>(0)?),
    )
    .optional()
    .map_err(|error| persistence_error("get model endpoint", error))
}

fn get_model_configuration_in_conn(
    conn: &Connection,
    model_config_id: &str,
) -> CoreResult<Option<ModelConfigurationRecord>> {
    conn.query_row(
        "SELECT record_json FROM model_configurations WHERE model_config_id=?1",
        params![model_config_id],
        |row| parse_record_json(row.get::<_, String>(0)?),
    )
    .optional()
    .map_err(|error| persistence_error("get model configuration", error))
}

fn parse_record_json<T: DeserializeOwned>(json: String) -> rusqlite::Result<T> {
    from_json_text(&json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
    })
}

fn enum_storage_text<T: Serialize>(value: T) -> CoreResult<String> {
    serde_json::to_value(value)
        .map_err(|error| persistence_error("serialize model registry enum", error))?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::InternalError,
                "enum did not serialize as text",
            )
        })
}

fn parse_enum<T: DeserializeOwned>(raw: &str) -> CoreResult<T> {
    serde_json::from_value(JsonValue::String(raw.to_string())).map_err(|error| {
        CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("parse legacy model registry enum {raw}: {error}"),
        )
    })
}

#[allow(clippy::too_many_lines)]
fn backfill_legacy_model_registry_in_tx(
    tx: &rusqlite::Transaction<'_>,
    dry_run: bool,
) -> CoreResult<ModelEndpointBackfillReport> {
    let mut statement = tx
        .prepare(
            "SELECT mp.alias,mp.status,mp.protocol,mp.provider_kind,mp.display_name,mp.description,
                mp.base_url,mp.model_id,mp.context_window_tokens,mp.max_output_tokens,
                mp.temperature_milli,mp.reasoning_effort,mp.reasoning_format,
                mp.responses_dialect,mp.chat_completions_dialect,mp.thinking_mode,
                mp.reasoning_history,mp.reasoning_budget_tokens,mp.prompt_caching,
                mp.credential_id,sc.secret_ciphertext,sc.secret_updated_at,sc.credential_kind,
                sc.revision,mp.metadata_json,mp.revision,mp.created_at,mp.updated_at
         FROM model_providers mp
         LEFT JOIN service_credentials sc ON sc.credential_id=mp.credential_id
         ORDER BY mp.alias ASC",
        )
        .map_err(|error| persistence_error("prepare legacy model registry backfill", error))?;
    let mut rows = statement
        .query([])
        .map_err(|error| persistence_error("query legacy model registry backfill", error))?;
    let mut report = ModelEndpointBackfillReport::default();
    while let Some(row) = rows
        .next()
        .map_err(|error| persistence_error("read legacy model registry backfill", error))?
    {
        let alias = row
            .get::<_, String>(0)
            .map_err(|error| persistence_error("read legacy alias", error))?;
        let provider = match super::service_config::row_to_model_provider(row) {
            Ok(provider) => provider,
            Err(error) => {
                report
                    .representability_errors
                    .push(ModelEndpointRepresentabilityError {
                        legacy_alias: alias,
                        field: "legacy_provider".to_string(),
                        reason: format!("legacy provider row is not readable: {error}"),
                    });
                continue;
            }
        };
        match normalized_records_from_provider(&provider) {
            Ok((endpoint, configuration)) => {
                let mut actual_configuration = get_model_configuration_in_conn(tx, &alias)?;
                let resolved_endpoint_id = actual_configuration
                    .as_ref()
                    .map_or_else(|| alias.clone(), |record| record.endpoint_id.clone());
                let mut actual_endpoint = get_model_endpoint_in_conn(tx, &resolved_endpoint_id)?;
                if !dry_run {
                    if actual_endpoint.is_none() {
                        write_model_endpoint_in_tx(tx, &endpoint, None)?;
                        actual_endpoint = Some(endpoint.clone());
                    }
                    if actual_configuration.is_none() {
                        write_model_configuration_in_tx(tx, &configuration, None)?;
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
                report.mappings.push(mapping);
                let projected_endpoint = actual_endpoint.as_ref().unwrap_or(&endpoint);
                let projected_configuration =
                    actual_configuration.as_ref().unwrap_or(&configuration);
                let joined = joined_provider_projection(
                    projected_endpoint,
                    projected_configuration,
                    provider.credential.clone(),
                );
                let differing_fields = provider_projection_differences(&provider, &joined);
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
            Err(error) => report
                .representability_errors
                .push(ModelEndpointRepresentabilityError {
                    legacy_alias: alias,
                    field: "legacy_provider".to_string(),
                    reason: error.message,
                }),
        }
    }
    Ok(report)
}

pub(crate) fn joined_provider_projection(
    endpoint: &ModelEndpointRecord,
    configuration: &ModelConfigurationRecord,
    credential: ModelProviderCredential,
) -> ModelProviderRecord {
    let provider_kind = endpoint
        .metadata_json
        .get("legacyVendorLabel")
        .and_then(JsonValue::as_str)
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
        (ModelProviderProtocol::Responses, _) => ChatCompletionsWireDialect::Standard,
        (_, ModelEndpointWireDialect::Kimi) => ChatCompletionsWireDialect::Kimi,
        (_, ModelEndpointWireDialect::Glm) => ChatCompletionsWireDialect::Glm,
        (_, ModelEndpointWireDialect::Qwen) => ChatCompletionsWireDialect::Qwen,
        (_, ModelEndpointWireDialect::Deepseek) => ChatCompletionsWireDialect::Deepseek,
        _ => ChatCompletionsWireDialect::Standard,
    };
    ModelProviderRecord {
        alias: configuration.model_config_id.clone(),
        status: configuration.status,
        protocol: endpoint.protocol,
        provider_kind,
        display_name: configuration.display_name.clone(),
        description: configuration.description.clone(),
        base_url: projected_legacy_base_url(endpoint),
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
        revision: configuration.revision,
        created_at: configuration.created_at.clone(),
        updated_at: configuration.updated_at.clone(),
    }
}

pub(crate) fn legacy_endpoint_base_url(
    provider: &ModelProviderRecord,
    metadata: &mut serde_json::Map<String, JsonValue>,
) -> String {
    match provider
        .base_url
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        Some(base_url) => base_url.clone(),
        None => {
            metadata.insert(
                LEGACY_MISSING_BASE_URL_METADATA_KEY.to_string(),
                JsonValue::Bool(true),
            );
            LEGACY_MISSING_BASE_URL_SENTINEL.to_string()
        }
    }
}

pub(crate) fn projected_legacy_base_url(endpoint: &ModelEndpointRecord) -> Option<String> {
    if endpoint
        .metadata_json
        .get(LEGACY_MISSING_BASE_URL_METADATA_KEY)
        .and_then(JsonValue::as_bool)
        .unwrap_or(false)
    {
        None
    } else {
        Some(endpoint.base_url.clone())
    }
}

pub(crate) fn provider_projection_differences(
    source: &ModelProviderRecord,
    joined: &ModelProviderRecord,
) -> Vec<String> {
    let mut fields = Vec::new();
    macro_rules! compare {
        ($field:ident) => {
            if source.$field != joined.$field {
                fields.push(stringify!($field).to_string());
            }
        };
    }
    compare!(alias);
    compare!(status);
    compare!(protocol);
    compare!(provider_kind);
    compare!(display_name);
    compare!(description);
    compare!(base_url);
    compare!(model_id);
    compare!(context_window_tokens);
    compare!(max_output_tokens);
    compare!(temperature_milli);
    compare!(reasoning_effort);
    compare!(reasoning_format);
    compare!(responses_dialect);
    compare!(chat_completions_dialect);
    compare!(thinking_mode);
    compare!(reasoning_history);
    compare!(reasoning_budget_tokens);
    compare!(prompt_caching);
    compare!(credential_id);
    compare!(credential);
    compare!(metadata_json);
    compare!(revision);
    compare!(created_at);
    compare!(updated_at);
    fields
}

#[allow(dead_code)]
fn legacy_records_from_row(
    row: &rusqlite::Row<'_>,
) -> CoreResult<(ModelEndpointRecord, ModelConfigurationRecord)> {
    let alias: String = row.get(0).map_err(|e| persistence_error("read alias", e))?;
    let status = parse_enum(
        &row.get::<_, String>(1)
            .map_err(|e| persistence_error("read status", e))?,
    )?;
    let protocol: ModelProviderProtocol = parse_enum(
        &row.get::<_, String>(2)
            .map_err(|e| persistence_error("read protocol", e))?,
    )?;
    let provider_kind: String = row
        .get(3)
        .map_err(|e| persistence_error("read provider kind", e))?;
    let base_url: Option<String> = row
        .get(6)
        .map_err(|e| persistence_error("read base url", e))?;
    let base_url = base_url
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::InvalidInput,
                "legacy base_url is not representable",
            )
        })?;
    let responses_dialect: Option<String> = row
        .get(13)
        .map_err(|e| persistence_error("read responses dialect", e))?;
    let chat_dialect: String = row
        .get(14)
        .map_err(|e| persistence_error("read chat dialect", e))?;
    let wire_dialect = match protocol {
        ModelProviderProtocol::Responses => match responses_dialect.as_deref() {
            Some("openai_stateful") => ModelEndpointWireDialect::OpenaiStateful,
            Some("openai_stateless") => ModelEndpointWireDialect::OpenaiStateless,
            Some("generic_stateless") => ModelEndpointWireDialect::GenericStateless,
            Some("deepseek") => ModelEndpointWireDialect::Deepseek,
            Some("meta") => ModelEndpointWireDialect::Meta,
            _ => {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "legacy responses_dialect is not representable",
                ))
            }
        },
        ModelProviderProtocol::ChatCompletions => parse_enum(&chat_dialect)?,
    };
    let credential_id: Option<String> = row
        .get(19)
        .map_err(|e| persistence_error("read credential id", e))?;
    let credential_kind: Option<String> = row
        .get(20)
        .map_err(|e| persistence_error("read credential kind", e))?;
    let auth_scheme = match credential_kind.as_deref() {
        None if credential_id.is_none() => ModelEndpointAuthScheme::None,
        Some("api_key" | "legacy_raw_api_key") => ModelEndpointAuthScheme::BearerApiKey,
        Some("openai_oauth") if protocol == ModelProviderProtocol::Responses => {
            ModelEndpointAuthScheme::OpenAiCodexOauth
        }
        _ => {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "legacy credential binding is not representable",
            ))
        }
    };
    let mut endpoint_metadata = serde_json::Map::new();
    endpoint_metadata.insert(
        "legacyVendorLabel".to_string(),
        JsonValue::String(provider_kind),
    );
    let prompt_caching: ChatCompletionsPromptCachingPolicy = parse_enum(
        &row.get::<_, String>(18)
            .map_err(|e| persistence_error("read prompt caching", e))?,
    )?;
    let endpoint = ModelEndpointRecord {
        endpoint_id: alias.clone(),
        status,
        display_name: row
            .get(4)
            .map_err(|e| persistence_error("read display name", e))?,
        description: row
            .get(5)
            .map_err(|e| persistence_error("read description", e))?,
        base_url,
        protocol,
        wire_dialect,
        auth_scheme,
        credential_id,
        prompt_cache_transport: if prompt_caching == ChatCompletionsPromptCachingPolicy::Disabled {
            PromptCacheTransport::None
        } else {
            PromptCacheTransport::OpenrouterAnthropic
        },
        metadata_json: JsonValue::Object(endpoint_metadata),
        revision: row
            .get::<_, i64>(22)
            .map_err(|e| persistence_error("read revision", e))? as u64,
        created_at: row
            .get(23)
            .map_err(|e| persistence_error("read created at", e))?,
        updated_at: row
            .get(24)
            .map_err(|e| persistence_error("read updated at", e))?,
    };
    endpoint.validate()?;
    let configuration = ModelConfigurationRecord {
        model_config_id: alias.clone(),
        endpoint_id: alias,
        status,
        display_name: endpoint.display_name.clone(),
        description: endpoint.description.clone(),
        model_id: row
            .get(7)
            .map_err(|e| persistence_error("read model id", e))?,
        context_window_tokens: row
            .get::<_, Option<i64>>(8)
            .map_err(|e| persistence_error("read context window", e))?
            .map(|v| v as u32),
        max_output_tokens: row
            .get::<_, Option<i64>>(9)
            .map_err(|e| persistence_error("read max output", e))?
            .map(|v| v as u32),
        temperature_milli: row
            .get::<_, Option<i64>>(10)
            .map_err(|e| persistence_error("read temperature", e))?
            .map(|v| v as u32),
        reasoning_effort: row
            .get(11)
            .map_err(|e| persistence_error("read reasoning effort", e))?,
        reasoning_format: row
            .get(12)
            .map_err(|e| persistence_error("read reasoning format", e))?,
        reasoning_history: parse_enum(
            &row.get::<_, String>(16)
                .map_err(|e| persistence_error("read reasoning history", e))?,
        )?,
        reasoning_budget_tokens: row
            .get::<_, Option<i64>>(17)
            .map_err(|e| persistence_error("read reasoning budget", e))?
            .map(|v| v as u32),
        thinking_mode: parse_enum(
            &row.get::<_, String>(15)
                .map_err(|e| persistence_error("read thinking mode", e))?,
        )?,
        prompt_caching_policy: prompt_caching,
        capabilities: Default::default(),
        metadata_json: from_json_text(
            &row.get::<_, String>(21)
                .map_err(|e| persistence_error("read metadata", e))?,
        )
        .map_err(|e| persistence_error("parse legacy metadata", e))?,
        revision: endpoint.revision,
        created_at: endpoint.created_at.clone(),
        updated_at: endpoint.updated_at.clone(),
    };
    configuration.validate_for_endpoint(&endpoint)?;
    Ok((endpoint, configuration))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn store() -> (CoordinationStore, PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "rusty-crew-model-registry-{}.sqlite3",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        (CoordinationStore::open_file(&path).unwrap(), path)
    }

    fn endpoint(id: &str, expected_revision: Option<u64>) -> ModelEndpointWrite {
        ModelEndpointWrite {
            endpoint_id: id.to_string(),
            status: ModelProviderStatus::Active,
            display_name: Some("Shared endpoint".to_string()),
            description: None,
            base_url: "http://127.0.0.1:8080/v1".to_string(),
            protocol: ModelProviderProtocol::ChatCompletions,
            wire_dialect: ModelEndpointWireDialect::Standard,
            auth_scheme: ModelEndpointAuthScheme::None,
            credential_id: None,
            prompt_cache_transport: PromptCacheTransport::None,
            metadata_json: json!({}),
            expected_revision,
            now: "2026-08-12T20:00:00Z".to_string(),
        }
    }

    fn configuration(id: &str, endpoint_id: &str) -> ModelConfigurationWrite {
        ModelConfigurationWrite {
            model_config_id: id.to_string(),
            endpoint_id: endpoint_id.to_string(),
            status: ModelProviderStatus::Active,
            display_name: Some(id.to_string()),
            description: None,
            model_id: format!("model-{id}"),
            context_window_tokens: Some(32_000),
            max_output_tokens: Some(4_096),
            temperature_milli: Some(500),
            reasoning_effort: None,
            reasoning_format: None,
            reasoning_history: ChatCompletionsReasoningHistory::ProviderDefault,
            reasoning_budget_tokens: None,
            thinking_mode: ChatCompletionsThinkingMode::ProviderDefault,
            prompt_caching_policy: ChatCompletionsPromptCachingPolicy::Disabled,
            capabilities: Default::default(),
            metadata_json: json!({}),
            expected_revision: None,
            now: "2026-08-12T20:00:01Z".to_string(),
        }
    }

    fn normalized_profile(profile_id: &str, model_config_id: &str) -> ProfileRegistryWrite {
        ProfileRegistryWrite {
            profile_id: ProfileId::new(profile_id),
            lifecycle_status: ProfileRegistryLifecycleStatus::Active,
            display_name: None,
            summary: None,
            default_session_kind: None,
            agent_id: None,
            owner_id: None,
            prompt_soul_markdown: None,
            prompt_memory_markdown: None,
            active_runtime_settings_json: json!({
                "providerAlias": "legacy-other",
                "profile": {"modelConfigId": model_config_id},
            }),
            source_asset_refs: Vec::new(),
            derived_runtime_refs: Vec::new(),
            import_export: rusty_crew_core_protocol::ProfileRegistryImportExportMetadata {
                imported_from: None,
                imported_at: None,
                exported_to: None,
                exported_at: None,
                metadata_json: json!({}),
            },
            now: "2026-08-13T00:00:00Z".to_string(),
        }
    }

    fn legacy_profile(
        profile_id: &str,
        model_config_id: &str,
        snake_case: bool,
    ) -> ProfileRegistryWrite {
        let mut write = normalized_profile(profile_id, model_config_id);
        write.active_runtime_settings_json = if snake_case {
            json!({"provider_alias": model_config_id})
        } else {
            json!({"providerAlias": model_config_id})
        };
        write
    }

    fn legacy_write(provider: ModelProviderRecord, now: &str) -> ModelProviderWrite {
        ModelProviderWrite {
            alias: provider.alias,
            status: provider.status,
            protocol: provider.protocol,
            provider_kind: provider.provider_kind,
            display_name: provider.display_name,
            description: provider.description,
            base_url: provider.base_url,
            model_id: provider.model_id,
            context_window_tokens: provider.context_window_tokens,
            max_output_tokens: provider.max_output_tokens,
            temperature_milli: provider.temperature_milli,
            reasoning_effort: provider.reasoning_effort,
            reasoning_format: provider.reasoning_format,
            responses_dialect: provider.responses_dialect,
            chat_completions_dialect: provider.chat_completions_dialect,
            thinking_mode: provider.thinking_mode,
            reasoning_history: provider.reasoning_history,
            reasoning_budget_tokens: provider.reasoning_budget_tokens,
            prompt_caching: provider.prompt_caching,
            secret: None,
            clear_secret: false,
            expected_credential_revision: None,
            metadata_json: provider.metadata_json,
            expected_revision: Some(provider.revision),
            now: now.to_string(),
        }
    }

    fn logical_bundle(
        endpoints: &[ModelEndpointRecord],
        configurations: &[ModelConfigurationRecord],
    ) -> LogicalStorageExportBundle {
        LogicalStorageExportBundle {
            bundle_version: 1,
            export_id: "model-registry-export-1".to_string(),
            exported_at: "2026-08-12T20:10:00Z".to_string(),
            service_version: Some("test".to_string()),
            source: LogicalStorageExportSource {
                backend: "sqlite".to_string(),
                backend_label: "source".to_string(),
                source_instance_id: Some("source-1".to_string()),
                snapshot_ref: Some("snapshot-1".to_string()),
            },
            schema_version: 1,
            module_versions: Vec::new(),
            capability_snapshot: Vec::new(),
            repositories: crate::sqlite_runtime_import::model_registry_logical_repositories(
                endpoints,
                configurations,
                &"2026-08-12T20:10:00Z".to_string(),
            )
            .unwrap(),
            legacy_id_mappings: Vec::new(),
            profile_asset_refs: Vec::new(),
        }
    }

    #[test]
    fn normalized_model_deletes_require_current_revisions_and_no_dependents() {
        let (store, path) = store();
        let endpoint = store
            .upsert_model_endpoint(&endpoint("shared", None))
            .unwrap();
        let configuration = store
            .upsert_model_configuration(&configuration("model-a", "shared"))
            .unwrap();

        let in_use = store
            .delete_model_endpoint(&ModelEndpointDelete {
                endpoint_id: endpoint.endpoint_id.clone(),
                expected_revision: endpoint.revision,
            })
            .unwrap_err();
        assert_eq!(in_use.kind, CoreErrorKind::ActionRejected);

        let stale = store
            .delete_model_configuration(&ModelConfigurationDelete {
                model_config_id: configuration.model_config_id.clone(),
                expected_revision: configuration.revision + 1,
            })
            .unwrap_err();
        assert_eq!(stale.kind, CoreErrorKind::ActionRejected);
        assert!(stale.message.contains("revision mismatch"));

        store
            .delete_model_configuration(&ModelConfigurationDelete {
                model_config_id: configuration.model_config_id.clone(),
                expected_revision: configuration.revision,
            })
            .unwrap();
        assert!(store
            .get_model_configuration(&configuration.model_config_id)
            .unwrap()
            .is_none());
        assert!(store
            .get_model_provider(&configuration.model_config_id)
            .unwrap()
            .is_none());

        store
            .delete_model_endpoint(&ModelEndpointDelete {
                endpoint_id: endpoint.endpoint_id.clone(),
                expected_revision: endpoint.revision,
            })
            .unwrap();
        assert!(store
            .get_model_endpoint(&endpoint.endpoint_id)
            .unwrap()
            .is_none());

        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn concurrent_profile_create_and_configuration_delete_cannot_leave_a_dangling_reference() {
        use std::sync::{Arc, Barrier};

        let (store, path) = store();
        store
            .upsert_model_endpoint(&endpoint("race-endpoint", None))
            .unwrap();

        for iteration in 0..20 {
            let model_config_id = format!("race-config-{iteration}");
            let profile_id = format!("race-profile-{iteration}");
            let configuration = store
                .upsert_model_configuration(&configuration(&model_config_id, "race-endpoint"))
                .unwrap();
            let barrier = Arc::new(Barrier::new(3));
            let create_store = store.clone();
            let create_barrier = Arc::clone(&barrier);
            let profile = normalized_profile(&profile_id, &model_config_id);
            let create = std::thread::spawn(move || {
                create_barrier.wait();
                create_store.create_profile_registry_record(&profile)
            });
            let delete_store = store.clone();
            let delete_barrier = Arc::clone(&barrier);
            let delete = std::thread::spawn(move || {
                delete_barrier.wait();
                delete_store.delete_model_configuration(&ModelConfigurationDelete {
                    model_config_id,
                    expected_revision: configuration.revision,
                })
            });
            barrier.wait();
            let create_result = create.join().unwrap();
            let delete_result = delete.join().unwrap();
            assert_ne!(create_result.is_ok(), delete_result.is_ok());

            let persisted_profile = store
                .get_profile_registry_record(&ProfileId::new(&profile_id))
                .unwrap();
            let persisted_configuration = store
                .get_model_configuration(&format!("race-config-{iteration}"))
                .unwrap();
            assert_eq!(
                persisted_profile.is_some(),
                persisted_configuration.is_some()
            );
            if persisted_profile.is_some() {
                store.purge_profile(&ProfileId::new(&profile_id)).unwrap();
                store
                    .delete_model_configuration(&ModelConfigurationDelete {
                        model_config_id: format!("race-config-{iteration}"),
                        expected_revision: persisted_configuration.unwrap().revision,
                    })
                    .unwrap();
            }
        }

        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn concurrent_legacy_profile_mutation_and_configuration_delete_cannot_restore_a_reference() {
        use std::sync::{Arc, Barrier};

        let (store, path) = store();
        store
            .upsert_model_endpoint(&endpoint("legacy-race-endpoint", None))
            .unwrap();

        for iteration in 0..20 {
            let model_config_id = format!("legacy-race-config-{iteration}");
            let profile_id = format!("legacy-race-profile-{iteration}");
            let configuration = store
                .upsert_model_configuration(&configuration(
                    &model_config_id,
                    "legacy-race-endpoint",
                ))
                .unwrap();
            let update_existing = iteration % 2 == 1;
            if update_existing {
                store
                    .create_profile_registry_record(&legacy_profile(
                        &profile_id,
                        "unmigrated-legacy-alias",
                        iteration % 4 == 1,
                    ))
                    .unwrap();
            }
            let profile = legacy_profile(&profile_id, &model_config_id, iteration % 2 == 0);
            let barrier = Arc::new(Barrier::new(3));
            let mutation_store = store.clone();
            let mutation_barrier = Arc::clone(&barrier);
            let mutation = std::thread::spawn(move || {
                mutation_barrier.wait();
                if update_existing {
                    mutation_store.update_profile_registry_record(&ProfileRegistryUpdate {
                        write: profile,
                        expected_revision: 1,
                    })
                } else {
                    mutation_store.create_profile_registry_record(&profile)
                }
            });
            let delete_store = store.clone();
            let delete_barrier = Arc::clone(&barrier);
            let delete_model_config_id = model_config_id.clone();
            let delete = std::thread::spawn(move || {
                delete_barrier.wait();
                delete_store.delete_model_configuration(&ModelConfigurationDelete {
                    model_config_id: delete_model_config_id,
                    expected_revision: configuration.revision,
                })
            });
            barrier.wait();
            let mutation_result = mutation.join().unwrap();
            let delete_result = delete.join().unwrap();
            assert_ne!(mutation_result.is_ok(), delete_result.is_ok());
            let persisted_configuration = store.get_model_configuration(&model_config_id).unwrap();
            let persisted_profile = store
                .get_profile_registry_record(&ProfileId::new(&profile_id))
                .unwrap();
            let references_target = persisted_profile.as_ref().is_some_and(|profile| {
                crate::effective_profile_model_config_id(&profile.active_runtime_settings_json)
                    .as_deref()
                    == Some(model_config_id.as_str())
            });
            assert!(!references_target || persisted_configuration.is_some());
            if persisted_profile.is_some() {
                store.purge_profile(&ProfileId::new(&profile_id)).unwrap();
            }
            if let Some(configuration) = persisted_configuration {
                store
                    .delete_model_configuration(&ModelConfigurationDelete {
                        model_config_id,
                        expected_revision: configuration.revision,
                    })
                    .unwrap();
            }
        }

        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn legacy_projection_preserves_missing_base_url_as_explicit_endpoint_metadata() {
        let (store, path) = store();
        store
            .upsert_model_endpoint(&endpoint("legacy", None))
            .unwrap();
        store
            .upsert_model_configuration(&configuration("legacy", "legacy"))
            .unwrap();
        let mut provider = store.get_model_provider("legacy").unwrap().unwrap();
        provider.base_url = None;

        let (normalized_endpoint, normalized_configuration) =
            normalized_records_from_provider(&provider).unwrap();
        assert_eq!(
            normalized_endpoint.base_url,
            LEGACY_MISSING_BASE_URL_SENTINEL
        );
        assert_eq!(
            normalized_endpoint
                .metadata_json
                .get(LEGACY_MISSING_BASE_URL_METADATA_KEY),
            Some(&JsonValue::Bool(true))
        );
        let joined = joined_provider_projection(
            &normalized_endpoint,
            &normalized_configuration,
            provider.credential.clone(),
        );
        assert_eq!(joined.base_url, None);
        assert!(provider_projection_differences(&provider, &joined).is_empty());

        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn responses_projection_ignores_legacy_chat_completions_dialect() {
        let (store, path) = store();
        store
            .upsert_model_endpoint(&endpoint("responses", None))
            .unwrap();
        store
            .upsert_model_configuration(&configuration("responses", "responses"))
            .unwrap();
        let mut provider = store.get_model_provider("responses").unwrap().unwrap();
        provider.protocol = ModelProviderProtocol::Responses;
        provider.responses_dialect = Some(ResponsesProviderDialect::Deepseek);
        provider.chat_completions_dialect = ChatCompletionsWireDialect::Standard;

        let (normalized_endpoint, normalized_configuration) =
            normalized_records_from_provider(&provider).unwrap();
        let joined = joined_provider_projection(
            &normalized_endpoint,
            &normalized_configuration,
            provider.credential.clone(),
        );
        assert_eq!(
            joined.chat_completions_dialect,
            ChatCompletionsWireDialect::Standard
        );
        assert!(provider_projection_differences(&provider, &joined).is_empty());

        drop(store);
        let _ = std::fs::remove_file(path);
    }

    fn model_repository(
        repository_id: &str,
        records: Vec<LogicalStorageRecord>,
    ) -> LogicalStorageRepositoryBundle {
        LogicalStorageRepositoryBundle {
            repository_id: repository_id.to_string(),
            schema_version: 1,
            required_capabilities: vec!["logical_export_import".to_string()],
            exported_count: records.len() as u64,
            checksum: Some(
                crate::sqlite_runtime_import::logical_storage_records_checksum(&records).unwrap(),
            ),
            records,
        }
    }

    fn endpoint_record_with_auth(
        endpoint_id: &str,
        protocol: ModelProviderProtocol,
        wire_dialect: ModelEndpointWireDialect,
        auth_scheme: ModelEndpointAuthScheme,
        credential_id: &str,
    ) -> ModelEndpointRecord {
        ModelEndpointRecord {
            endpoint_id: endpoint_id.to_string(),
            status: ModelProviderStatus::Active,
            display_name: Some("Secured endpoint".to_string()),
            description: None,
            base_url: "http://127.0.0.1:8080/v1".to_string(),
            protocol,
            wire_dialect,
            auth_scheme,
            credential_id: Some(credential_id.to_string()),
            prompt_cache_transport: PromptCacheTransport::None,
            metadata_json: json!({}),
            revision: 1,
            created_at: "2026-08-13T00:00:00Z".to_string(),
            updated_at: "2026-08-13T00:00:00Z".to_string(),
        }
    }

    fn service_credential(
        credential_id: &str,
        credential_kind: ModelProviderCredentialKind,
        secret: Option<&str>,
    ) -> ServiceCredentialWrite {
        ServiceCredentialWrite {
            credential_id: credential_id.to_string(),
            display_name: "Import credential".to_string(),
            provider_kind: "test".to_string(),
            credential_kind,
            secret: secret.map(str::to_string),
            clear_secret: false,
            expected_revision: None,
            now: "2026-08-13T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn normalized_registries_have_independent_cas_and_secret_free_shadows() {
        let (store, path) = store();
        let created_endpoint = store
            .upsert_model_endpoint(&endpoint("shared", None))
            .unwrap();
        let created_config = store
            .upsert_model_configuration(&configuration("model-a", "shared"))
            .unwrap();
        assert_eq!(created_endpoint.revision, 1);
        assert_eq!(created_config.revision, 1);

        let mut endpoint_update = endpoint("shared", Some(1));
        endpoint_update.description = Some("transport update".to_string());
        let updated_endpoint = store.upsert_model_endpoint(&endpoint_update).unwrap();
        assert_eq!(updated_endpoint.revision, 2);
        assert_eq!(
            store
                .get_model_configuration("model-a")
                .unwrap()
                .unwrap()
                .revision,
            1
        );
        assert!(store
            .upsert_model_endpoint(&endpoint("shared", Some(1)))
            .unwrap_err()
            .message
            .contains("revision mismatch"));

        let shadow = store.get_model_provider("model-a").unwrap().unwrap();
        assert_eq!(shadow.model_id, "model-model-a");
        assert!(!shadow.credential.has_secret);
        assert_eq!(store.get_model_provider_secret("model-a").unwrap(), None);
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn model_registry_logical_import_preserves_revisions_and_proves_readback() {
        let (source, source_path) = store();
        source
            .upsert_model_endpoint(&endpoint("shared", None))
            .unwrap();
        source
            .upsert_model_configuration(&configuration("model-a", "shared"))
            .unwrap();
        let mut endpoint_update = endpoint("shared", Some(1));
        endpoint_update.description = Some("revision two".to_string());
        let source_endpoint = source.upsert_model_endpoint(&endpoint_update).unwrap();
        let source_configuration = source.get_model_configuration("model-a").unwrap().unwrap();
        assert_eq!(source_endpoint.revision, 2);
        assert_eq!(source_configuration.revision, 1);
        let bundle = logical_bundle(
            std::slice::from_ref(&source_endpoint),
            std::slice::from_ref(&source_configuration),
        );

        let (target, target_path) = store();
        let dry_run = LogicalStorageImportDryRun {
            import_batch_id: "model-registry-import-1".to_string(),
            target_backend: "sqlite".to_string(),
            validation_time: "2026-08-12T20:11:00Z".to_string(),
            supported_capabilities: vec!["logical_export_import".to_string()],
            supported_repositories: vec![
                "model_endpoints".to_string(),
                "model_configurations".to_string(),
            ],
        };
        let validation = target
            .validate_logical_storage_import(&bundle, &dry_run)
            .unwrap();
        assert!(validation.can_apply(), "{:#?}", validation.issues);
        let proofs = target
            .apply_model_registry_logical_import(&bundle, &dry_run)
            .unwrap();
        assert_eq!(proofs.len(), 2);
        assert!(proofs.iter().all(|proof| proof.verified));
        assert_eq!(
            target
                .list_model_endpoints(&ModelEndpointQuery::default())
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            target
                .list_model_configurations(&ModelConfigurationQuery::default())
                .unwrap()
                .len(),
            1
        );
        assert_eq!(target.load_import_batches().unwrap().len(), 1);
        assert_eq!(
            target
                .get_model_endpoint("shared")
                .unwrap()
                .unwrap()
                .revision,
            2
        );
        assert_eq!(
            target
                .get_model_configuration("model-a")
                .unwrap()
                .unwrap()
                .revision,
            1
        );
        assert!(target
            .apply_model_registry_logical_import(&bundle, &dry_run)
            .unwrap_err()
            .message
            .contains("dry-run did not pass"));

        drop(source);
        drop(target);
        let _ = std::fs::remove_file(source_path);
        let _ = std::fs::remove_file(target_path);
    }

    #[test]
    fn model_registry_logical_import_rejects_foreign_target_without_writes() {
        let (source, source_path) = store();
        let endpoint_record = source
            .upsert_model_endpoint(&endpoint("shared", None))
            .unwrap();
        let configuration_record = source
            .upsert_model_configuration(&configuration("model-a", "shared"))
            .unwrap();
        let bundle = logical_bundle(
            std::slice::from_ref(&endpoint_record),
            std::slice::from_ref(&configuration_record),
        );
        let (target, target_path) = store();
        let dry_run = LogicalStorageImportDryRun {
            import_batch_id: "foreign-target-model-import".to_string(),
            target_backend: "postgres".to_string(),
            validation_time: "2026-08-13T00:01:00Z".to_string(),
            supported_capabilities: vec!["logical_export_import".to_string()],
            supported_repositories: vec![
                "model_endpoints".to_string(),
                "model_configurations".to_string(),
            ],
        };

        let report = target
            .validate_logical_storage_import(&bundle, &dry_run)
            .unwrap();
        assert!(!report.can_apply());
        assert!(report.issues.iter().any(|issue| {
            issue.code == "model_registry_envelope_invalid"
                && issue
                    .message
                    .contains("requires target_backend sqlite, got postgres")
        }));

        let error = target
            .apply_model_registry_logical_import(&bundle, &dry_run)
            .unwrap_err();
        assert_eq!(error.kind, CoreErrorKind::InvalidInput);
        assert!(error
            .message
            .contains("requires target_backend sqlite, got postgres"));
        assert!(target.get_model_endpoint("shared").unwrap().is_none());
        assert!(target.get_model_configuration("model-a").unwrap().is_none());
        assert!(target.load_import_batches().unwrap().is_empty());

        drop(source);
        drop(target);
        let _ = std::fs::remove_file(source_path);
        let _ = std::fs::remove_file(target_path);
    }

    #[test]
    fn model_registry_logical_import_rejects_duplicate_empty_and_nonempty_repositories_without_writes(
    ) {
        let (source, source_path) = store();
        let endpoint_record = source
            .upsert_model_endpoint(&endpoint("shared", None))
            .unwrap();
        let configuration_record = source
            .upsert_model_configuration(&configuration("model-a", "shared"))
            .unwrap();
        let base_bundle = logical_bundle(
            std::slice::from_ref(&endpoint_record),
            std::slice::from_ref(&configuration_record),
        );

        let mut duplicate_endpoint_record = base_bundle.repositories[0].records[0].clone();
        duplicate_endpoint_record.stable_id = "duplicate-endpoint".to_string();
        if let LogicalStorageRecordPayload::ModelEndpoint(endpoint) =
            &mut duplicate_endpoint_record.payload
        {
            endpoint.endpoint_id = "duplicate-endpoint".to_string();
        }
        let mut duplicate_configuration_record = base_bundle.repositories[1].records[0].clone();
        duplicate_configuration_record.stable_id = "model-b".to_string();
        if let LogicalStorageRecordPayload::ModelConfiguration(configuration) =
            &mut duplicate_configuration_record.payload
        {
            configuration.model_config_id = "model-b".to_string();
        }

        let duplicate_repositories = vec![
            (
                "model_endpoints empty",
                model_repository("model_endpoints", Vec::new()),
            ),
            (
                "model_endpoints nonempty",
                model_repository("model_endpoints", vec![duplicate_endpoint_record]),
            ),
            (
                "model_configurations empty",
                model_repository("model_configurations", Vec::new()),
            ),
            (
                "model_configurations nonempty",
                model_repository("model_configurations", vec![duplicate_configuration_record]),
            ),
        ];

        for (index, (label, duplicate_repository)) in duplicate_repositories.into_iter().enumerate()
        {
            let mut bundle = base_bundle.clone();
            let repository_id = duplicate_repository.repository_id.clone();
            bundle.repositories.push(duplicate_repository);
            let (target, target_path) = store();
            let dry_run = LogicalStorageImportDryRun {
                import_batch_id: format!("duplicate-model-import-{index}"),
                target_backend: "sqlite".to_string(),
                validation_time: "2026-08-13T00:02:00Z".to_string(),
                supported_capabilities: vec!["logical_export_import".to_string()],
                supported_repositories: vec![
                    "model_endpoints".to_string(),
                    "model_configurations".to_string(),
                ],
            };

            let report = target
                .validate_logical_storage_import(&bundle, &dry_run)
                .unwrap();
            assert!(!report.can_apply(), "{label}: {report:#?}");
            assert!(report.issues.iter().any(|issue| {
                issue.code == "model_registry_envelope_invalid"
                    && issue
                        .message
                        .contains(&format!("duplicate repository {repository_id}"))
            }));

            let error = target
                .apply_model_registry_logical_import(&bundle, &dry_run)
                .unwrap_err();
            assert_eq!(error.kind, CoreErrorKind::InvalidInput, "{label}");
            assert!(error
                .message
                .contains(&format!("duplicate repository {repository_id}")));
            assert!(
                target.get_model_endpoint("shared").unwrap().is_none(),
                "{label}"
            );
            assert!(
                target.get_model_configuration("model-a").unwrap().is_none(),
                "{label}"
            );
            assert!(target.load_import_batches().unwrap().is_empty(), "{label}");

            drop(target);
            let _ = std::fs::remove_file(target_path);
        }

        drop(source);
        let _ = std::fs::remove_file(source_path);
    }

    #[test]
    fn model_registry_logical_dry_run_rejects_checksum_and_reference_drift() {
        let endpoint_record = ModelEndpointRecord {
            endpoint_id: "shared".to_string(),
            status: ModelProviderStatus::Active,
            display_name: None,
            description: None,
            base_url: "http://127.0.0.1:8080/v1".to_string(),
            protocol: ModelProviderProtocol::ChatCompletions,
            wire_dialect: ModelEndpointWireDialect::Standard,
            auth_scheme: ModelEndpointAuthScheme::None,
            credential_id: None,
            prompt_cache_transport: PromptCacheTransport::None,
            metadata_json: json!({}),
            revision: 1,
            created_at: "2026-08-12T20:00:00Z".to_string(),
            updated_at: "2026-08-12T20:00:00Z".to_string(),
        };
        let mut configuration_record = ModelConfigurationRecord {
            model_config_id: "model-a".to_string(),
            endpoint_id: "missing".to_string(),
            status: ModelProviderStatus::Active,
            display_name: None,
            description: None,
            model_id: "model-a".to_string(),
            context_window_tokens: Some(32_000),
            max_output_tokens: Some(4_096),
            temperature_milli: None,
            reasoning_effort: None,
            reasoning_format: None,
            reasoning_history: ChatCompletionsReasoningHistory::ProviderDefault,
            reasoning_budget_tokens: None,
            thinking_mode: ChatCompletionsThinkingMode::ProviderDefault,
            prompt_caching_policy: ChatCompletionsPromptCachingPolicy::Disabled,
            capabilities: Default::default(),
            metadata_json: json!({}),
            revision: 1,
            created_at: "2026-08-12T20:00:00Z".to_string(),
            updated_at: "2026-08-12T20:00:00Z".to_string(),
        };
        let mut bundle = logical_bundle(&[endpoint_record], &[configuration_record.clone()]);
        bundle.repositories[0].checksum = Some("sha256:wrong".to_string());
        let (target, path) = store();
        let dry_run = LogicalStorageImportDryRun {
            import_batch_id: "bad-model-registry".to_string(),
            target_backend: "sqlite".to_string(),
            validation_time: "2026-08-12T20:11:00Z".to_string(),
            supported_capabilities: vec!["logical_export_import".to_string()],
            supported_repositories: Vec::new(),
        };
        let report = target
            .validate_logical_storage_import(&bundle, &dry_run)
            .unwrap();
        assert!(!report.can_apply());
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.code == "repository_checksum_mismatch"));
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.code == "model_configuration_endpoint_missing"));

        configuration_record.endpoint_id = "shared".to_string();
        drop(target);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn model_registry_logical_dry_run_requires_model_repository_checksums() {
        let (source, source_path) = store();
        let endpoint_record = source
            .upsert_model_endpoint(&endpoint("shared", None))
            .unwrap();
        let mut bundle = logical_bundle(std::slice::from_ref(&endpoint_record), &[]);
        bundle.repositories[0].checksum = None;
        let (target, target_path) = store();
        let report = target
            .validate_logical_storage_import(
                &bundle,
                &LogicalStorageImportDryRun {
                    import_batch_id: "missing-model-checksum".to_string(),
                    target_backend: "sqlite".to_string(),
                    validation_time: "2026-08-13T00:01:00Z".to_string(),
                    supported_capabilities: vec!["logical_export_import".to_string()],
                    supported_repositories: vec![
                        "model_endpoints".to_string(),
                        "model_configurations".to_string(),
                    ],
                },
            )
            .unwrap();

        assert!(!report.can_apply());
        assert!(report.issues.iter().any(|issue| {
            issue.code == "model_repository_checksum_missing"
                && issue.repository_id.as_deref() == Some("model_endpoints")
        }));

        drop(source);
        drop(target);
        let _ = std::fs::remove_file(source_path);
        let _ = std::fs::remove_file(target_path);
    }

    #[test]
    fn model_registry_logical_dry_run_matches_apply_for_target_credentials() {
        let cases = [
            (
                "missing credential",
                endpoint_record_with_auth(
                    "secured",
                    ModelProviderProtocol::ChatCompletions,
                    ModelEndpointWireDialect::Standard,
                    ModelEndpointAuthScheme::BearerApiKey,
                    "missing-credential",
                ),
                None,
                "model_endpoint_credential_missing",
            ),
            (
                "credential without secret",
                endpoint_record_with_auth(
                    "secured",
                    ModelProviderProtocol::ChatCompletions,
                    ModelEndpointWireDialect::Standard,
                    ModelEndpointAuthScheme::BearerApiKey,
                    "empty-credential",
                ),
                Some(service_credential(
                    "empty-credential",
                    ModelProviderCredentialKind::ApiKey,
                    None,
                )),
                "model_endpoint_credential_secret_missing",
            ),
            (
                "incompatible credential kind",
                endpoint_record_with_auth(
                    "secured",
                    ModelProviderProtocol::Responses,
                    ModelEndpointWireDialect::OpenaiStateless,
                    ModelEndpointAuthScheme::OpenAiCodexOauth,
                    "api-key-credential",
                ),
                Some(service_credential(
                    "api-key-credential",
                    ModelProviderCredentialKind::ApiKey,
                    Some("sk-target"),
                )),
                "model_endpoint_credential_auth_kind_mismatch",
            ),
        ];

        for (index, (_label, endpoint_record, credential, issue_code)) in
            cases.into_iter().enumerate()
        {
            let (target, target_path) = store();
            if let Some(credential) = credential {
                target.upsert_service_credential(&credential).unwrap();
            }
            let bundle = logical_bundle(std::slice::from_ref(&endpoint_record), &[]);
            let dry_run = LogicalStorageImportDryRun {
                import_batch_id: format!("credential-import-{index}"),
                target_backend: "sqlite".to_string(),
                validation_time: "2026-08-13T00:01:00Z".to_string(),
                supported_capabilities: vec!["logical_export_import".to_string()],
                supported_repositories: vec![
                    "model_endpoints".to_string(),
                    "model_configurations".to_string(),
                ],
            };

            let report = target
                .validate_logical_storage_import(&bundle, &dry_run)
                .unwrap();
            assert!(!report.can_apply(), "{_label}: {report:#?}");
            assert!(report.issues.iter().any(|issue| issue.code == issue_code));
            assert!(target
                .apply_model_registry_logical_import(&bundle, &dry_run)
                .unwrap_err()
                .message
                .contains("dry-run did not pass"));
            assert!(target.get_model_endpoint("secured").unwrap().is_none());

            drop(target);
            let _ = std::fs::remove_file(target_path);
        }
    }

    #[test]
    fn legacy_write_rejects_ambiguous_shared_endpoint_mutation() {
        let (store, path) = store();
        store
            .upsert_model_endpoint(&endpoint("shared", None))
            .unwrap();
        store
            .upsert_model_configuration(&configuration("model-a", "shared"))
            .unwrap();
        store
            .upsert_model_configuration(&configuration("model-b", "shared"))
            .unwrap();
        let report = store.backfill_legacy_model_registry(true).unwrap();
        assert!(report.representability_errors.is_empty());
        assert!(report
            .joined_projection_equality
            .iter()
            .all(|entry| entry.projection_equal));
        assert!(report
            .mappings
            .iter()
            .all(|mapping| mapping.endpoint_id == "shared"));
        let mut model_only = legacy_write(
            store.get_model_provider("model-a").unwrap().unwrap(),
            "2026-08-12T20:00:02Z",
        );
        model_only.model_id = "model-a-v2".to_string();
        store.upsert_model_provider(&model_only).unwrap();
        assert_eq!(
            store
                .get_model_endpoint("shared")
                .unwrap()
                .unwrap()
                .revision,
            1,
            "model-only legacy writes must not advance shared endpoint authority"
        );

        let mut write = legacy_write(
            store.get_model_provider("model-a").unwrap().unwrap(),
            "2026-08-12T20:00:03Z",
        );
        write.base_url = Some("http://127.0.0.1:9090/v1".to_string());
        assert_eq!(
            store.upsert_model_provider(&write).unwrap_err().message,
            "legacy_provider_shared_endpoint_conflict"
        );
        drop(store);
        let _ = std::fs::remove_file(path);
    }
}
