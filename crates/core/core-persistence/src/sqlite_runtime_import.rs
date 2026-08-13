//! SQLite runtime import and legacy-id mapping repository domain.
//!
//! Logical import validation lives beside the import metadata SQL because the
//! dry-run checks are the guardrail that prevents resurrecting expired queued
//! work during backend moves.

use super::*;
use sha2::{Digest, Sha256};

impl CoordinationStore {
    pub fn save_import_batch(&self, record: &RuntimeImportBatchRecord) -> CoreResult<()> {
        let conn = self.conn()?;
        save_import_batch(&conn, record)
    }

    pub fn load_import_batches(&self) -> CoreResult<Vec<RuntimeImportBatchRecord>> {
        let conn = self.conn()?;
        load_import_batches(&conn)
    }

    pub fn save_legacy_id_mapping(&self, record: &LegacyIdMappingRecord) -> CoreResult<()> {
        let conn = self.conn()?;
        save_legacy_id_mapping(&conn, record)
    }

    pub fn query_legacy_id_mappings(
        &self,
        query: &LegacyIdMappingQuery,
    ) -> CoreResult<Vec<LegacyIdMappingRecord>> {
        let conn = self.conn()?;
        query_legacy_id_mappings(&conn, query)
    }

    pub fn validate_logical_storage_import(
        &self,
        bundle: &LogicalStorageExportBundle,
        dry_run: &LogicalStorageImportDryRun,
    ) -> CoreResult<LogicalStorageImportValidationReport> {
        let conn = self.conn()?;
        validate_logical_storage_import(&conn, bundle, dry_run)
    }
}

fn save_import_batch(conn: &Connection, record: &RuntimeImportBatchRecord) -> CoreResult<()> {
    conn.execute(
        "INSERT INTO runtime_import_batches (
            import_batch_id,
            source_system,
            source_label,
            source_snapshot_ref,
            notes,
            imported_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ON CONFLICT(import_batch_id) DO UPDATE SET
            source_system = excluded.source_system,
            source_label = excluded.source_label,
            source_snapshot_ref = excluded.source_snapshot_ref,
            notes = excluded.notes",
        params![
            record.import_batch_id,
            record.source_system,
            record.source_label,
            record.source_snapshot_ref,
            record.notes,
            record.imported_at,
        ],
    )
    .map_err(|error| persistence_error("save runtime import batch", error))?;
    Ok(())
}

fn load_import_batches(conn: &Connection) -> CoreResult<Vec<RuntimeImportBatchRecord>> {
    let mut stmt = conn
        .prepare(
            "SELECT
                import_batch_id,
                source_system,
                source_label,
                source_snapshot_ref,
                notes,
                imported_at
             FROM runtime_import_batches
             ORDER BY imported_at ASC, import_batch_id ASC",
        )
        .map_err(|error| persistence_error("prepare load runtime import batches", error))?;
    let rows = stmt
        .query_map([], row_to_import_batch)
        .map_err(|error| persistence_error("query runtime import batches", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load runtime import batches", error))
}

fn save_legacy_id_mapping(conn: &Connection, record: &LegacyIdMappingRecord) -> CoreResult<()> {
    let provenance_json = to_json_text(&record.provenance)?;
    conn.execute(
        "INSERT INTO legacy_id_mappings (
            import_batch_id,
            source_system,
            legacy_kind,
            legacy_id,
            rusty_kind,
            rusty_id,
            provenance_json,
            created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        ON CONFLICT(source_system, legacy_kind, legacy_id) DO UPDATE SET
            import_batch_id = excluded.import_batch_id,
            rusty_kind = excluded.rusty_kind,
            rusty_id = excluded.rusty_id,
            provenance_json = excluded.provenance_json",
        params![
            record.import_batch_id,
            record.source.system,
            runtime_object_kind_as_str(record.legacy_kind),
            record.source.external_id,
            runtime_object_kind_as_str(record.rusty_kind),
            record.rusty_id,
            provenance_json,
            record.created_at,
        ],
    )
    .map_err(|error| persistence_error("save legacy id mapping", error))?;
    Ok(())
}

fn query_legacy_id_mappings(
    conn: &Connection,
    query: &LegacyIdMappingQuery,
) -> CoreResult<Vec<LegacyIdMappingRecord>> {
    let import_batch_id = query.import_batch_id.as_deref();
    let source_system = query.source_system.as_deref();
    let legacy_kind = query.legacy_kind.map(runtime_object_kind_as_str);
    let rusty_kind = query.rusty_kind.map(runtime_object_kind_as_str);
    let rusty_id = query.rusty_id.as_deref();
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let mut stmt = conn
        .prepare(
            "SELECT
                import_batch_id,
                source_system,
                legacy_kind,
                legacy_id,
                rusty_kind,
                rusty_id,
                provenance_json,
                created_at
             FROM legacy_id_mappings
             WHERE (?1 IS NULL OR import_batch_id = ?1)
               AND (?2 IS NULL OR source_system = ?2)
               AND (?3 IS NULL OR legacy_kind = ?3)
               AND (?4 IS NULL OR rusty_kind = ?4)
               AND (?5 IS NULL OR rusty_id = ?5)
             ORDER BY created_at ASC, source_system ASC, legacy_kind ASC, legacy_id ASC
             LIMIT ?6 OFFSET ?7",
        )
        .map_err(|error| persistence_error("prepare query legacy id mappings", error))?;
    let rows = stmt
        .query_map(
            params![
                import_batch_id,
                source_system,
                legacy_kind,
                rusty_kind,
                rusty_id,
                limit,
                offset,
            ],
            row_to_legacy_id_mapping,
        )
        .map_err(|error| persistence_error("query legacy id mappings", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load legacy id mappings", error))
}

pub(crate) fn validate_logical_storage_import(
    conn: &Connection,
    bundle: &LogicalStorageExportBundle,
    dry_run: &LogicalStorageImportDryRun,
) -> CoreResult<LogicalStorageImportValidationReport> {
    if dry_run.import_batch_id.trim().is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "logical import dry-run requires an import_batch_id",
        ));
    }
    if dry_run.target_backend.trim().is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "logical import dry-run requires a target_backend",
        ));
    }

    let mut issues = Vec::new();
    let mut accepted_records = 0_u64;
    let mut accepted_record_keys = BTreeSet::new();
    let mut unsupported_records = 0_u64;
    let mut refused_records = 0_u64;
    let record_count = bundle
        .repositories
        .iter()
        .map(|repository| repository.records.len() as u64)
        .sum();
    let mut logical_record_counts = BTreeMap::<(String, String), u64>::new();
    for repository in &bundle.repositories {
        for record in &repository.records {
            *logical_record_counts
                .entry((repository.repository_id.clone(), record.stable_id.clone()))
                .or_default() += 1;
        }
    }
    let duplicate_logical_record_keys = logical_record_counts
        .into_iter()
        .filter_map(|(key, count)| {
            if count > 1 {
                issues.push(logical_import_issue(
                    LogicalStorageImportIssueSeverity::Error,
                    "duplicate_logical_record",
                    Some(key.0.clone()),
                    Some(key.1.clone()),
                    format!(
                        "logical record key ({}, {}) appears {count} times",
                        key.0, key.1
                    ),
                ));
                Some(key)
            } else {
                None
            }
        })
        .collect::<BTreeSet<_>>();
    let supported_capabilities = if dry_run.supported_capabilities.is_empty() {
        sqlite_storage_capabilities()
            .into_iter()
            .filter(|capability| capability.supported)
            .map(|capability| capability.name)
            .collect::<BTreeSet<_>>()
    } else {
        dry_run
            .supported_capabilities
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>()
    };
    let supported_repositories = dry_run
        .supported_repositories
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();

    if bundle.bundle_version != 1 {
        issues.push(logical_import_issue(
            LogicalStorageImportIssueSeverity::Error,
            "unsupported_bundle_version",
            None,
            None,
            format!(
                "logical storage bundle version {} is not supported",
                bundle.bundle_version
            ),
        ));
    }
    if bundle.export_id.trim().is_empty() {
        issues.push(logical_import_issue(
            LogicalStorageImportIssueSeverity::Error,
            "missing_export_id",
            None,
            None,
            "logical storage bundle requires an export_id",
        ));
    }

    let already_imported = import_batch_exists(conn, &dry_run.import_batch_id)?;
    if already_imported {
        issues.push(logical_import_issue(
            LogicalStorageImportIssueSeverity::Info,
            "import_batch_already_recorded",
            None,
            None,
            format!(
                "import batch {} is already recorded; validation is idempotent and will not apply records",
                dry_run.import_batch_id
            ),
        ));
    }

    for repository in &bundle.repositories {
        let repository_supported = supported_repositories.is_empty()
            || supported_repositories.contains(&repository.repository_id);
        let missing_capabilities = repository
            .required_capabilities
            .iter()
            .filter(|capability| !supported_capabilities.contains(*capability))
            .cloned()
            .collect::<Vec<_>>();

        if !repository_supported {
            unsupported_records += repository.records.len() as u64;
            issues.push(logical_import_issue(
                LogicalStorageImportIssueSeverity::Error,
                "unsupported_repository",
                Some(repository.repository_id.clone()),
                None,
                format!(
                    "target backend {} does not declare support for repository {}",
                    dry_run.target_backend, repository.repository_id
                ),
            ));
            continue;
        }

        if !missing_capabilities.is_empty() {
            unsupported_records += repository.records.len() as u64;
            issues.push(logical_import_issue(
                LogicalStorageImportIssueSeverity::Error,
                "missing_storage_capability",
                Some(repository.repository_id.clone()),
                None,
                format!(
                    "target backend {} is missing required capabilities: {}",
                    dry_run.target_backend,
                    missing_capabilities.join(", ")
                ),
            ));
            continue;
        }

        let mut repository_integrity_refused = false;
        if repository.exported_count != repository.records.len() as u64 {
            refused_records += repository.records.len() as u64;
            repository_integrity_refused = true;
            issues.push(logical_import_issue(
                LogicalStorageImportIssueSeverity::Error,
                "repository_count_mismatch",
                Some(repository.repository_id.clone()),
                None,
                format!(
                    "repository {} declared {} records but contains {} records",
                    repository.repository_id,
                    repository.exported_count,
                    repository.records.len()
                ),
            ));
        }

        if matches!(
            repository.repository_id.as_str(),
            "model_endpoints" | "model_configurations"
        ) && repository.checksum.is_none()
        {
            refused_records += repository.records.len() as u64;
            repository_integrity_refused = true;
            issues.push(logical_import_issue(
                LogicalStorageImportIssueSeverity::Error,
                "model_repository_checksum_missing",
                Some(repository.repository_id.clone()),
                None,
                format!(
                    "model repository {} must include a checksum",
                    repository.repository_id
                ),
            ));
        }

        if let Some(expected_checksum) = repository.checksum.as_deref() {
            let actual_checksum = logical_storage_records_checksum(&repository.records)?;
            if expected_checksum != actual_checksum {
                if !repository_integrity_refused {
                    refused_records += repository.records.len() as u64;
                }
                issues.push(logical_import_issue(
                    LogicalStorageImportIssueSeverity::Error,
                    "repository_checksum_mismatch",
                    Some(repository.repository_id.clone()),
                    None,
                    format!(
                        "repository {} checksum mismatch: expected {}, calculated {}",
                        repository.repository_id, expected_checksum, actual_checksum
                    ),
                ));
                continue;
            }
        }

        if repository_integrity_refused {
            continue;
        }

        for record in &repository.records {
            if duplicate_logical_record_keys
                .contains(&(repository.repository_id.clone(), record.stable_id.clone()))
            {
                refused_records += 1;
                continue;
            }
            match validate_logical_storage_record(repository, record, &dry_run.validation_time) {
                Ok(()) => {
                    if let LogicalStorageRecordPayload::ModelEndpoint(endpoint) = &record.payload {
                        if let Err(error) =
                            crate::repos::model_registry::validate_import_endpoint_credential(
                                conn, endpoint,
                            )
                        {
                            if !matches!(
                                &error.kind,
                                CoreErrorKind::NotFound | CoreErrorKind::InvalidInput
                            ) {
                                return Err(error);
                            }
                            refused_records += 1;
                            issues.push(logical_model_endpoint_credential_issue(
                                repository, record, error,
                            ));
                            continue;
                        }
                    }
                    accepted_records += 1;
                    accepted_record_keys
                        .insert((repository.repository_id.clone(), record.stable_id.clone()));
                }
                Err(issue) => {
                    refused_records += 1;
                    issues.push(issue);
                }
            }
        }
    }

    validate_model_registry_references(
        bundle,
        &mut issues,
        &mut accepted_records,
        &mut accepted_record_keys,
        &mut refused_records,
    );

    Ok(LogicalStorageImportValidationReport {
        import_batch_id: dry_run.import_batch_id.clone(),
        dry_run: true,
        source_backend: bundle.source.backend.clone(),
        target_backend: dry_run.target_backend.clone(),
        repository_count: bundle.repositories.len() as u64,
        record_count,
        accepted_records,
        unsupported_records,
        refused_records,
        already_imported,
        issues,
    })
}

pub fn logical_storage_records_checksum(records: &[LogicalStorageRecord]) -> CoreResult<String> {
    let mut canonical_records = records.to_vec();
    canonical_records.sort_by(|left, right| left.stable_id.cmp(&right.stable_id));
    let canonical_records = canonical_records
        .iter()
        .map(|record| {
            serde_json::json!({
                "stable_id": record.stable_id,
                "record_version": record.record_version,
                "payload": record.payload,
            })
        })
        .collect::<Vec<_>>();
    let bytes = serde_json::to_vec(&canonical_records).map_err(|error| {
        CoreError::new(
            CoreErrorKind::InternalError,
            format!("serialize logical storage records for checksum: {error}"),
        )
    })?;
    let digest = Sha256::digest(bytes);
    Ok(format!(
        "sha256:{}",
        digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    ))
}

pub fn model_registry_logical_repositories(
    endpoints: &[ModelEndpointRecord],
    configurations: &[ModelConfigurationRecord],
    exported_at: &IsoTimestamp,
) -> CoreResult<Vec<LogicalStorageRepositoryBundle>> {
    let endpoint_records = endpoints
        .iter()
        .map(|endpoint| LogicalStorageRecord {
            stable_id: endpoint.endpoint_id.clone(),
            record_version: 1,
            exported_at: exported_at.clone(),
            payload: LogicalStorageRecordPayload::ModelEndpoint(Box::new(endpoint.clone())),
        })
        .collect::<Vec<_>>();
    let configuration_records = configurations
        .iter()
        .map(|configuration| LogicalStorageRecord {
            stable_id: configuration.model_config_id.clone(),
            record_version: 1,
            exported_at: exported_at.clone(),
            payload: LogicalStorageRecordPayload::ModelConfiguration(Box::new(
                configuration.clone(),
            )),
        })
        .collect::<Vec<_>>();
    Ok(vec![
        LogicalStorageRepositoryBundle {
            repository_id: "model_endpoints".to_string(),
            schema_version: 1,
            required_capabilities: vec!["logical_export_import".to_string()],
            exported_count: endpoint_records.len() as u64,
            checksum: Some(logical_storage_records_checksum(&endpoint_records)?),
            records: endpoint_records,
        },
        LogicalStorageRepositoryBundle {
            repository_id: "model_configurations".to_string(),
            schema_version: 1,
            required_capabilities: vec!["logical_export_import".to_string()],
            exported_count: configuration_records.len() as u64,
            checksum: Some(logical_storage_records_checksum(&configuration_records)?),
            records: configuration_records,
        },
    ])
}

fn validate_model_registry_references(
    bundle: &LogicalStorageExportBundle,
    issues: &mut Vec<LogicalStorageImportIssue>,
    accepted_records: &mut u64,
    accepted_record_keys: &mut BTreeSet<(String, String)>,
    refused_records: &mut u64,
) {
    let endpoint_ids = bundle
        .repositories
        .iter()
        .filter(|repository| repository.repository_id == "model_endpoints")
        .flat_map(|repository| repository.records.iter())
        .filter_map(|record| match &record.payload {
            LogicalStorageRecordPayload::ModelEndpoint(endpoint) => {
                Some(endpoint.endpoint_id.as_str())
            }
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    for record in bundle
        .repositories
        .iter()
        .filter(|repository| repository.repository_id == "model_configurations")
        .flat_map(|repository| repository.records.iter())
    {
        let LogicalStorageRecordPayload::ModelConfiguration(configuration) = &record.payload else {
            continue;
        };
        if !endpoint_ids.contains(configuration.endpoint_id.as_str()) {
            refuse_accepted_record(
                "model_configurations",
                &record.stable_id,
                accepted_records,
                accepted_record_keys,
                refused_records,
            );
            issues.push(logical_import_issue(
                LogicalStorageImportIssueSeverity::Error,
                "model_configuration_endpoint_missing",
                Some("model_configurations".to_string()),
                Some(record.stable_id.clone()),
                format!(
                    "model configuration {} references endpoint {} absent from the logical bundle",
                    configuration.model_config_id, configuration.endpoint_id
                ),
            ));
        }
    }

    for repository in bundle.repositories.iter().filter(|repository| {
        repository.repository_id == "model_endpoints"
            || repository.repository_id == "model_configurations"
    }) {
        if repository.schema_version != 1 {
            for record in &repository.records {
                refuse_accepted_record(
                    &repository.repository_id,
                    &record.stable_id,
                    accepted_records,
                    accepted_record_keys,
                    refused_records,
                );
            }
            issues.push(logical_import_issue(
                LogicalStorageImportIssueSeverity::Error,
                "model_repository_schema_version_unsupported",
                Some(repository.repository_id.clone()),
                None,
                format!(
                    "repository {} schema version {} is unsupported; expected 1",
                    repository.repository_id, repository.schema_version
                ),
            ));
        }
    }
}

fn refuse_accepted_record(
    repository_id: &str,
    stable_id: &str,
    accepted_records: &mut u64,
    accepted_record_keys: &mut BTreeSet<(String, String)>,
    refused_records: &mut u64,
) {
    if accepted_record_keys.remove(&(repository_id.to_string(), stable_id.to_string())) {
        *accepted_records = accepted_records.saturating_sub(1);
        *refused_records += 1;
    }
}

fn validate_logical_storage_record(
    repository: &LogicalStorageRepositoryBundle,
    record: &LogicalStorageRecord,
    now: &IsoTimestamp,
) -> Result<(), LogicalStorageImportIssue> {
    if record.stable_id.trim().is_empty() {
        return Err(logical_import_issue(
            LogicalStorageImportIssueSeverity::Error,
            "missing_stable_id",
            Some(repository.repository_id.clone()),
            None,
            "logical import record requires a stable_id",
        ));
    }

    match &record.payload {
        LogicalStorageRecordPayload::QueueMessage(message) => {
            validate_logical_queue_message(repository, record, message.as_ref(), now)
        }
        LogicalStorageRecordPayload::ModelEndpoint(endpoint) => {
            if repository.repository_id != "model_endpoints" {
                return Err(logical_import_issue(
                    LogicalStorageImportIssueSeverity::Error,
                    "model_endpoint_in_wrong_repository",
                    Some(repository.repository_id.clone()),
                    Some(record.stable_id.clone()),
                    "model endpoint records must be grouped under model_endpoints",
                ));
            }
            if record.record_version != 1 || record.stable_id != endpoint.endpoint_id {
                return Err(logical_import_issue(
                    LogicalStorageImportIssueSeverity::Error,
                    "model_endpoint_identity_or_version_invalid",
                    Some(repository.repository_id.clone()),
                    Some(record.stable_id.clone()),
                    "model endpoint logical record requires version 1 and a stable_id matching endpoint_id",
                ));
            }
            endpoint.validate().map_err(|error| {
                logical_import_issue(
                    LogicalStorageImportIssueSeverity::Error,
                    "model_endpoint_invalid",
                    Some(repository.repository_id.clone()),
                    Some(record.stable_id.clone()),
                    error.to_string(),
                )
            })
        }
        LogicalStorageRecordPayload::ModelConfiguration(configuration) => {
            if repository.repository_id != "model_configurations" {
                return Err(logical_import_issue(
                    LogicalStorageImportIssueSeverity::Error,
                    "model_configuration_in_wrong_repository",
                    Some(repository.repository_id.clone()),
                    Some(record.stable_id.clone()),
                    "model configuration records must be grouped under model_configurations",
                ));
            }
            if record.record_version != 1 || record.stable_id != configuration.model_config_id {
                return Err(logical_import_issue(
                    LogicalStorageImportIssueSeverity::Error,
                    "model_configuration_identity_or_version_invalid",
                    Some(repository.repository_id.clone()),
                    Some(record.stable_id.clone()),
                    "model configuration logical record requires version 1 and a stable_id matching model_config_id",
                ));
            }
            configuration.validate().map_err(|error| {
                logical_import_issue(
                    LogicalStorageImportIssueSeverity::Error,
                    "model_configuration_invalid",
                    Some(repository.repository_id.clone()),
                    Some(record.stable_id.clone()),
                    error.to_string(),
                )
            })
        }
        LogicalStorageRecordPayload::TypedJson { .. } => Ok(()),
    }
}

fn validate_logical_queue_message(
    repository: &LogicalStorageRepositoryBundle,
    record: &LogicalStorageRecord,
    message: &LogicalQueuedMessageExportRecord,
    now: &IsoTimestamp,
) -> Result<(), LogicalStorageImportIssue> {
    if repository.repository_id != "queues_messages" {
        return Err(logical_import_issue(
            LogicalStorageImportIssueSeverity::Error,
            "queue_record_in_wrong_repository",
            Some(repository.repository_id.clone()),
            Some(record.stable_id.clone()),
            "queue message records must be grouped under queues_messages",
        ));
    }
    if message.state == QueuedMessageState::Pending && message.expires_at <= *now {
        return Err(logical_import_issue(
            LogicalStorageImportIssueSeverity::Error,
            "queue_pending_expired_would_resurrect",
            Some(repository.repository_id.clone()),
            Some(record.stable_id.clone()),
            "pending queue message is already expired at validation time and must not be imported as deliverable work",
        ));
    }
    if message.state == QueuedMessageState::Pending && message.terminal_at.is_some() {
        return Err(logical_import_issue(
            LogicalStorageImportIssueSeverity::Error,
            "queue_pending_has_terminal_at",
            Some(repository.repository_id.clone()),
            Some(record.stable_id.clone()),
            "pending queue message cannot carry terminal_at",
        ));
    }
    if message.state != QueuedMessageState::Pending && message.terminal_at.is_none() {
        return Err(logical_import_issue(
            LogicalStorageImportIssueSeverity::Error,
            "queue_terminal_missing_terminal_at",
            Some(repository.repository_id.clone()),
            Some(record.stable_id.clone()),
            "terminal queue message must preserve terminal_at so it cannot be resurrected",
        ));
    }
    Ok(())
}

fn import_batch_exists(conn: &Connection, import_batch_id: &str) -> CoreResult<bool> {
    conn.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM runtime_import_batches WHERE import_batch_id = ?1
        )",
        params![import_batch_id],
        |row| row.get::<_, i64>(0),
    )
    .map(|value| value != 0)
    .map_err(|error| persistence_error("check runtime import batch", error))
}

fn logical_import_issue(
    severity: LogicalStorageImportIssueSeverity,
    code: impl Into<String>,
    repository_id: Option<String>,
    record_id: Option<String>,
    message: impl Into<String>,
) -> LogicalStorageImportIssue {
    LogicalStorageImportIssue {
        severity,
        code: code.into(),
        repository_id,
        record_id,
        message: message.into(),
    }
}

fn logical_model_endpoint_credential_issue(
    repository: &LogicalStorageRepositoryBundle,
    record: &LogicalStorageRecord,
    error: CoreError,
) -> LogicalStorageImportIssue {
    let code = match error.kind {
        CoreErrorKind::NotFound => "model_endpoint_credential_missing",
        CoreErrorKind::InvalidInput if error.message.contains("has no secret") => {
            "model_endpoint_credential_secret_missing"
        }
        CoreErrorKind::InvalidInput if error.message.contains("auth scheme is incompatible") => {
            "model_endpoint_credential_auth_kind_mismatch"
        }
        _ => "model_endpoint_credential_validation_failed",
    };
    logical_import_issue(
        LogicalStorageImportIssueSeverity::Error,
        code,
        Some(repository.repository_id.clone()),
        Some(record.stable_id.clone()),
        error.message,
    )
}

fn row_to_import_batch(row: &rusqlite::Row<'_>) -> rusqlite::Result<RuntimeImportBatchRecord> {
    Ok(RuntimeImportBatchRecord {
        import_batch_id: row.get(0)?,
        source_system: row.get(1)?,
        source_label: row.get(2)?,
        source_snapshot_ref: row.get(3)?,
        notes: row.get(4)?,
        imported_at: row.get(5)?,
    })
}

fn row_to_legacy_id_mapping(row: &rusqlite::Row<'_>) -> rusqlite::Result<LegacyIdMappingRecord> {
    let legacy_kind: String = row.get(2)?;
    let rusty_kind: String = row.get(4)?;
    let provenance_json: String = row.get(6)?;
    Ok(LegacyIdMappingRecord {
        import_batch_id: row.get(0)?,
        source: SourceSystemReference {
            system: row.get(1)?,
            external_id: row.get(3)?,
        },
        legacy_kind: runtime_object_kind_from_str(&legacy_kind)?,
        rusty_kind: runtime_object_kind_from_str(&rusty_kind)?,
        rusty_id: row.get(5)?,
        provenance: from_json_text(&provenance_json).map_err(to_sql_error)?,
        created_at: row.get(7)?,
    })
}

fn runtime_object_kind_as_str(kind: RuntimeObjectKind) -> &'static str {
    match kind {
        RuntimeObjectKind::Agent => "agent",
        RuntimeObjectKind::AgentInstance => "agent_instance",
        RuntimeObjectKind::Session => "session",
        RuntimeObjectKind::Profile => "profile",
        RuntimeObjectKind::WorkerRun => "worker_run",
        RuntimeObjectKind::Message => "message",
        RuntimeObjectKind::CompletionPacket => "completion_packet",
        RuntimeObjectKind::ToolCall => "tool_call",
        RuntimeObjectKind::QueueMessage => "queue_message",
        RuntimeObjectKind::ExternalArtifact => "external_artifact",
    }
}

fn runtime_object_kind_from_str(raw: &str) -> rusqlite::Result<RuntimeObjectKind> {
    match raw {
        "agent" => Ok(RuntimeObjectKind::Agent),
        "agent_instance" => Ok(RuntimeObjectKind::AgentInstance),
        "session" => Ok(RuntimeObjectKind::Session),
        "profile" => Ok(RuntimeObjectKind::Profile),
        "worker_run" => Ok(RuntimeObjectKind::WorkerRun),
        "message" => Ok(RuntimeObjectKind::Message),
        "completion_packet" => Ok(RuntimeObjectKind::CompletionPacket),
        "tool_call" => Ok(RuntimeObjectKind::ToolCall),
        "queue_message" => Ok(RuntimeObjectKind::QueueMessage),
        "external_artifact" => Ok(RuntimeObjectKind::ExternalArtifact),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown runtime object kind {other}"),
            )),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn store() -> (CoordinationStore, PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "rusty-crew-runtime-import-{}.sqlite3",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        (CoordinationStore::open_file(&path).unwrap(), path)
    }

    #[test]
    fn logical_import_rejects_duplicate_repository_stable_ids() {
        let records = vec![
            LogicalStorageRecord {
                stable_id: "duplicate".to_string(),
                record_version: 1,
                exported_at: "2026-08-13T00:00:00Z".to_string(),
                payload: LogicalStorageRecordPayload::TypedJson {
                    object_kind: "test_record".to_string(),
                    payload_json: json!({"value": 1}),
                },
            },
            LogicalStorageRecord {
                stable_id: "duplicate".to_string(),
                record_version: 1,
                exported_at: "2026-08-13T00:00:00Z".to_string(),
                payload: LogicalStorageRecordPayload::TypedJson {
                    object_kind: "test_record".to_string(),
                    payload_json: json!({"value": 2}),
                },
            },
        ];
        let bundle = LogicalStorageExportBundle {
            bundle_version: 1,
            export_id: "duplicate-record-export".to_string(),
            exported_at: "2026-08-13T00:00:00Z".to_string(),
            service_version: Some("test".to_string()),
            source: LogicalStorageExportSource {
                backend: "sqlite".to_string(),
                backend_label: "test".to_string(),
                source_instance_id: None,
                snapshot_ref: None,
            },
            schema_version: 1,
            module_versions: Vec::new(),
            capability_snapshot: Vec::new(),
            repositories: vec![LogicalStorageRepositoryBundle {
                repository_id: "test_records".to_string(),
                schema_version: 1,
                required_capabilities: vec!["logical_export_import".to_string()],
                exported_count: records.len() as u64,
                checksum: Some(logical_storage_records_checksum(&records).unwrap()),
                records,
            }],
            legacy_id_mappings: Vec::new(),
            profile_asset_refs: Vec::new(),
        };
        let dry_run = LogicalStorageImportDryRun {
            import_batch_id: "duplicate-record-import".to_string(),
            target_backend: "sqlite".to_string(),
            validation_time: "2026-08-13T00:01:00Z".to_string(),
            supported_capabilities: vec!["logical_export_import".to_string()],
            supported_repositories: vec!["test_records".to_string()],
        };
        let (store, path) = store();

        let report = store
            .validate_logical_storage_import(&bundle, &dry_run)
            .unwrap();
        assert!(!report.can_apply());
        assert_eq!(report.accepted_records, 0);
        assert_eq!(report.refused_records, 2);
        assert!(report.issues.iter().any(|issue| {
            issue.code == "duplicate_logical_record"
                && issue.repository_id.as_deref() == Some("test_records")
                && issue.record_id.as_deref() == Some("duplicate")
        }));

        drop(store);
        let _ = std::fs::remove_file(path);
    }
}
