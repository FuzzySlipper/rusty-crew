//! SQLite runtime import and legacy-id mapping repository domain.
//!
//! Logical import validation lives beside the import metadata SQL because the
//! dry-run checks are the guardrail that prevents resurrecting expired queued
//! work during backend moves.

use super::*;

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

fn validate_logical_storage_import(
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
    let mut unsupported_records = 0_u64;
    let mut refused_records = 0_u64;
    let record_count = bundle
        .repositories
        .iter()
        .map(|repository| repository.records.len() as u64)
        .sum();
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

        if repository.exported_count != repository.records.len() as u64 {
            issues.push(logical_import_issue(
                LogicalStorageImportIssueSeverity::Warning,
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

        for record in &repository.records {
            match validate_logical_storage_record(repository, record, &dry_run.validation_time) {
                Ok(()) => accepted_records += 1,
                Err(issue) => {
                    refused_records += 1;
                    issues.push(issue);
                }
            }
        }
    }

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
