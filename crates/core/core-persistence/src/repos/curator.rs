use super::super::*;

pub(crate) fn migrate_v34_add_curator_governance(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS module_curator_candidates (
            candidate_id TEXT PRIMARY KEY,
            batch_id TEXT NOT NULL,
            profile_id TEXT NOT NULL,
            session_id TEXT,
            status TEXT NOT NULL,
            lifecycle_state TEXT NOT NULL,
            fingerprint TEXT NOT NULL,
            expires_at TEXT,
            revision INTEGER NOT NULL,
            record_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_curator_candidates_profile_status
            ON module_curator_candidates(profile_id, status, updated_at DESC, candidate_id);
        CREATE INDEX IF NOT EXISTS idx_curator_candidates_profile_lifecycle
            ON module_curator_candidates(profile_id, lifecycle_state, updated_at DESC, candidate_id);
        CREATE INDEX IF NOT EXISTS idx_curator_candidates_batch
            ON module_curator_candidates(batch_id, candidate_id);
        CREATE INDEX IF NOT EXISTS idx_curator_candidates_session
            ON module_curator_candidates(session_id, updated_at DESC, candidate_id);
        CREATE INDEX IF NOT EXISTS idx_curator_candidates_expires
            ON module_curator_candidates(expires_at);

        CREATE TABLE IF NOT EXISTS module_curator_approvals (
            approval_id TEXT PRIMARY KEY,
            receipt_id TEXT NOT NULL UNIQUE,
            candidate_id TEXT NOT NULL,
            actor_id TEXT,
            approved_at TEXT NOT NULL,
            record_json TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_curator_approvals_candidate
            ON module_curator_approvals(candidate_id, approved_at DESC, approval_id);
        CREATE INDEX IF NOT EXISTS idx_curator_approvals_actor
            ON module_curator_approvals(actor_id, approved_at DESC, approval_id);

        CREATE TABLE IF NOT EXISTS module_curator_snapshot_refs (
            snapshot_id TEXT PRIMARY KEY,
            candidate_id TEXT NOT NULL,
            status TEXT NOT NULL,
            created_at TEXT NOT NULL,
            record_json TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_curator_snapshots_candidate
            ON module_curator_snapshot_refs(candidate_id, created_at DESC, snapshot_id);

        CREATE TABLE IF NOT EXISTS module_curator_mutations (
            mutation_id TEXT PRIMARY KEY,
            receipt_id TEXT NOT NULL UNIQUE,
            candidate_id TEXT NOT NULL,
            snapshot_id TEXT NOT NULL,
            actor_id TEXT,
            status TEXT NOT NULL,
            revision INTEGER NOT NULL,
            record_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            applied_at TEXT,
            rolled_back_at TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_curator_mutations_candidate
            ON module_curator_mutations(candidate_id, created_at DESC, mutation_id);
        CREATE INDEX IF NOT EXISTS idx_curator_mutations_status
            ON module_curator_mutations(status, created_at DESC, mutation_id);
        CREATE INDEX IF NOT EXISTS idx_curator_mutations_snapshot
            ON module_curator_mutations(snapshot_id);
        CREATE INDEX IF NOT EXISTS idx_curator_mutations_actor
            ON module_curator_mutations(actor_id, created_at DESC, mutation_id);

        CREATE TABLE IF NOT EXISTS module_curator_audit_receipts (
            sequence INTEGER PRIMARY KEY AUTOINCREMENT,
            receipt_id TEXT NOT NULL UNIQUE,
            correlation_id TEXT,
            idempotency_key TEXT,
            profile_id TEXT,
            session_id TEXT,
            candidate_id TEXT,
            mutation_id TEXT,
            activity_kind TEXT NOT NULL,
            outcome TEXT NOT NULL,
            reason_code TEXT,
            occurred_at TEXT NOT NULL,
            record_json TEXT NOT NULL,
            UNIQUE(activity_kind, idempotency_key)
        );
        CREATE INDEX IF NOT EXISTS idx_curator_audit_candidate
            ON module_curator_audit_receipts(candidate_id, sequence);
        CREATE INDEX IF NOT EXISTS idx_curator_audit_mutation
            ON module_curator_audit_receipts(mutation_id, sequence);
        CREATE INDEX IF NOT EXISTS idx_curator_audit_profile
            ON module_curator_audit_receipts(profile_id, sequence);
        CREATE INDEX IF NOT EXISTS idx_curator_audit_session
            ON module_curator_audit_receipts(session_id, sequence);
        CREATE INDEX IF NOT EXISTS idx_curator_audit_kind
            ON module_curator_audit_receipts(activity_kind, sequence);
        CREATE INDEX IF NOT EXISTS idx_curator_audit_time
            ON module_curator_audit_receipts(occurred_at, sequence);
        ",
    )
    .map_err(|error| persistence_error("create typed curator governance tables", error))?;
    Ok(())
}

impl CoordinationStore {
    pub fn apply_curator_governance_write(
        &self,
        write: &CuratorGovernanceWrite,
    ) -> CoreResult<CuratorGovernanceWriteResult> {
        validate_governance_write(write)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|error| persistence_error("start curator governance write", error))?;
        if let Some(existing) = find_idempotent_receipt(&tx, &write.receipt)? {
            return idempotent_result(&tx, write, existing);
        }

        let candidate = write
            .candidate
            .as_ref()
            .map(|candidate| put_candidate_in_tx(&tx, candidate))
            .transpose()?;
        if let Some(approval) = &write.approval {
            put_approval_in_tx(&tx, approval)?;
        }
        if let Some(snapshot) = &write.snapshot {
            put_snapshot_in_tx(&tx, snapshot)?;
        }
        let mutation = write
            .mutation
            .as_ref()
            .map(|mutation| put_mutation_in_tx(&tx, mutation))
            .transpose()?;
        let receipt = insert_receipt_in_tx(&tx, &write.receipt)?;
        tx.commit()
            .map_err(|error| persistence_error("commit curator governance write", error))?;
        Ok(CuratorGovernanceWriteResult {
            candidate,
            mutation,
            receipt,
            idempotent_replay: false,
        })
    }

    pub fn get_curator_candidate(
        &self,
        candidate_id: &str,
    ) -> CoreResult<Option<CuratorCandidateRecord>> {
        validate_id("candidate_id", candidate_id)?;
        let conn = self.conn()?;
        get_json_record(
            &conn,
            "module_curator_candidates",
            "candidate_id",
            candidate_id,
        )
    }

    pub fn list_curator_candidates(
        &self,
        query: &CuratorCandidateQuery,
    ) -> CoreResult<ExactPage<CuratorCandidateRecord>> {
        let (limit, offset) = bounded_page(query.page);
        let conn = self.conn()?;
        let status = query.status.as_ref().map(candidate_status);
        let lifecycle = query.lifecycle_state.as_ref().map(lifecycle_state);
        let predicate = "(?1 IS NULL OR profile_id = ?1) AND (?2 IS NULL OR session_id = ?2) AND (?3 IS NULL OR status = ?3) AND (?4 IS NULL OR lifecycle_state = ?4)";
        let total: i64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM module_curator_candidates WHERE {predicate}"),
                params![query.profile_id, query.session_id, status, lifecycle],
                |row| row.get(0),
            )
            .map_err(|error| persistence_error("count curator candidates", error))?;
        let mut stmt = conn
            .prepare(&format!(
                "SELECT record_json FROM module_curator_candidates WHERE {predicate} ORDER BY updated_at DESC, candidate_id ASC LIMIT ?5 OFFSET ?6"
            ))
            .map_err(|error| persistence_error("prepare curator candidate query", error))?;
        let items = collect_json_rows(
            stmt.query_map(
                params![
                    query.profile_id,
                    query.session_id,
                    status,
                    lifecycle,
                    limit,
                    offset
                ],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| persistence_error("query curator candidates", error))?,
            "curator candidate",
        )?;
        Ok(ExactPage::new(
            items,
            total as u64,
            limit as u32,
            offset as u32,
        ))
    }

    pub fn get_curator_mutation(
        &self,
        mutation_id: &str,
    ) -> CoreResult<Option<CuratorMutationRecord>> {
        validate_id("mutation_id", mutation_id)?;
        let conn = self.conn()?;
        get_json_record(
            &conn,
            "module_curator_mutations",
            "mutation_id",
            mutation_id,
        )
    }

    pub fn list_curator_mutations(
        &self,
        query: &CuratorMutationQuery,
    ) -> CoreResult<ExactPage<CuratorMutationRecord>> {
        let (limit, offset) = bounded_page(query.page);
        let conn = self.conn()?;
        let status = query.status.as_ref().map(mutation_status);
        let predicate = "(?1 IS NULL OR candidate_id = ?1) AND (?2 IS NULL OR status = ?2)";
        let total: i64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM module_curator_mutations WHERE {predicate}"),
                params![query.candidate_id, status],
                |row| row.get(0),
            )
            .map_err(|error| persistence_error("count curator mutations", error))?;
        let mut stmt = conn
            .prepare(&format!(
                "SELECT record_json FROM module_curator_mutations WHERE {predicate} ORDER BY created_at DESC, mutation_id ASC LIMIT ?3 OFFSET ?4"
            ))
            .map_err(|error| persistence_error("prepare curator mutation query", error))?;
        let items = collect_json_rows(
            stmt.query_map(params![query.candidate_id, status, limit, offset], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|error| persistence_error("query curator mutations", error))?,
            "curator mutation",
        )?;
        Ok(ExactPage::new(
            items,
            total as u64,
            limit as u32,
            offset as u32,
        ))
    }

    pub fn list_curator_audit_receipts(
        &self,
        query: &CuratorAuditQuery,
    ) -> CoreResult<ExactPage<CuratorAuditReceiptRecord>> {
        let (limit, offset) = bounded_page(query.page);
        let conn = self.conn()?;
        let predicate = "(?1 IS NULL OR profile_id = ?1) AND (?2 IS NULL OR session_id = ?2) AND (?3 IS NULL OR candidate_id = ?3) AND (?4 IS NULL OR mutation_id = ?4) AND (?5 IS NULL OR activity_kind = ?5)";
        let total: i64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM module_curator_audit_receipts WHERE {predicate}"),
                params![
                    query.profile_id,
                    query.session_id,
                    query.candidate_id,
                    query.mutation_id,
                    query.activity_kind
                ],
                |row| row.get(0),
            )
            .map_err(|error| persistence_error("count curator audit receipts", error))?;
        let mut stmt = conn
            .prepare(&format!(
                "SELECT record_json FROM module_curator_audit_receipts WHERE {predicate} ORDER BY sequence ASC LIMIT ?6 OFFSET ?7"
            ))
            .map_err(|error| persistence_error("prepare curator audit query", error))?;
        let items = collect_json_rows(
            stmt.query_map(
                params![
                    query.profile_id,
                    query.session_id,
                    query.candidate_id,
                    query.mutation_id,
                    query.activity_kind,
                    limit,
                    offset
                ],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| persistence_error("query curator audit receipts", error))?,
            "curator audit receipt",
        )?;
        Ok(ExactPage::new(
            items,
            total as u64,
            limit as u32,
            offset as u32,
        ))
    }

    pub fn purge_curator_profile(&self, profile_id: &str) -> CoreResult<CuratorPurgeReport> {
        purge_curator_scope(self, "profile_id", profile_id)
    }

    pub fn purge_curator_session(&self, session_id: &str) -> CoreResult<CuratorPurgeReport> {
        purge_curator_scope(self, "session_id", session_id)
    }
}

fn put_candidate_in_tx(
    tx: &rusqlite::Transaction<'_>,
    write: &CuratorCandidateWrite,
) -> CoreResult<CuratorCandidateRecord> {
    let current = get_json_record::<CuratorCandidateRecord>(
        tx,
        "module_curator_candidates",
        "candidate_id",
        &write.record.candidate_id,
    )?;
    if let Some(current) = &current {
        if current.fingerprint != write.record.fingerprint {
            return conflict("curator_candidate_fingerprint_conflict");
        }
        validate_candidate_transition(&current.status, &write.record.status)?;
    }
    let revision = next_revision(
        current.as_ref().map(|record| record.revision),
        write.expected_revision,
    )?;
    let mut record = write.record.clone();
    record.revision = revision;
    tx.execute(
        "INSERT INTO module_curator_candidates (candidate_id, batch_id, profile_id, session_id, status, lifecycle_state, fingerprint, expires_at, revision, record_json, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
         ON CONFLICT(candidate_id) DO UPDATE SET batch_id=excluded.batch_id, profile_id=excluded.profile_id, session_id=excluded.session_id, status=excluded.status, lifecycle_state=excluded.lifecycle_state, fingerprint=excluded.fingerprint, expires_at=excluded.expires_at, revision=excluded.revision, record_json=excluded.record_json, updated_at=excluded.updated_at",
        params![record.candidate_id, record.batch_id, record.profile_id, record.session_id, candidate_status(&record.status), lifecycle_state(&record.lifecycle_state), record.fingerprint, record.expires_at, record.revision as i64, to_json_text(&record)?, record.created_at, record.updated_at],
    ).map_err(|error| persistence_error("write curator candidate", error))?;
    Ok(record)
}

fn put_approval_in_tx(
    tx: &rusqlite::Transaction<'_>,
    record: &CuratorApprovalRecord,
) -> CoreResult<()> {
    tx.execute(
        "INSERT INTO module_curator_approvals (approval_id, receipt_id, candidate_id, actor_id, approved_at, record_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![record.approval_id, record.receipt_id, record.candidate_id, record.actor_id, record.approved_at, to_json_text(record)?],
    ).map_err(|error| persistence_error("write curator approval", error))?;
    Ok(())
}

fn put_snapshot_in_tx(
    tx: &rusqlite::Transaction<'_>,
    record: &CuratorSnapshotRefRecord,
) -> CoreResult<()> {
    validate_snapshot_root_ref(&record.snapshot_root_ref)?;
    tx.execute(
        "INSERT INTO module_curator_snapshot_refs (snapshot_id, candidate_id, status, created_at, record_json) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(snapshot_id) DO UPDATE SET status=excluded.status, record_json=excluded.record_json",
        params![record.snapshot_id, record.candidate_id, record.status, record.created_at, to_json_text(record)?],
    ).map_err(|error| persistence_error("write curator snapshot ref", error))?;
    Ok(())
}

fn put_mutation_in_tx(
    tx: &rusqlite::Transaction<'_>,
    write: &CuratorMutationWrite,
) -> CoreResult<CuratorMutationRecord> {
    let current = get_json_record::<CuratorMutationRecord>(
        tx,
        "module_curator_mutations",
        "mutation_id",
        &write.record.mutation_id,
    )?;
    if let Some(current) = &current {
        validate_mutation_transition(&current.status, &write.record.status)?;
    }
    let revision = next_revision(
        current.as_ref().map(|record| record.revision),
        write.expected_revision,
    )?;
    let mut record = write.record.clone();
    record.revision = revision;
    tx.execute(
        "INSERT INTO module_curator_mutations (mutation_id, receipt_id, candidate_id, snapshot_id, actor_id, status, revision, record_json, created_at, applied_at, rolled_back_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT(mutation_id) DO UPDATE SET status=excluded.status, revision=excluded.revision, record_json=excluded.record_json, applied_at=excluded.applied_at, rolled_back_at=excluded.rolled_back_at",
        params![record.mutation_id, record.receipt_id, record.candidate_id, record.snapshot_id, record.actor_id, mutation_status(&record.status), record.revision as i64, to_json_text(&record)?, record.created_at, record.applied_at, record.rolled_back_at],
    ).map_err(|error| persistence_error("write curator mutation", error))?;
    Ok(record)
}

fn insert_receipt_in_tx(
    tx: &rusqlite::Transaction<'_>,
    input: &CuratorAuditReceiptRecord,
) -> CoreResult<CuratorAuditReceiptRecord> {
    let mut record = input.clone();
    record.sequence = 0;
    tx.execute(
        "INSERT INTO module_curator_audit_receipts (receipt_id, correlation_id, idempotency_key, profile_id, session_id, candidate_id, mutation_id, activity_kind, outcome, reason_code, occurred_at, record_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![record.receipt_id, record.correlation_id, record.idempotency_key, record.profile_id, record.session_id, record.candidate_id, record.mutation_id, record.activity_kind, record.outcome, record.reason_code, record.occurred_at, to_json_text(&record)?],
    ).map_err(|error| persistence_error("write curator audit receipt", error))?;
    record.sequence = tx.last_insert_rowid() as u64;
    tx.execute(
        "UPDATE module_curator_audit_receipts SET record_json=?1 WHERE sequence=?2",
        params![to_json_text(&record)?, record.sequence as i64],
    )
    .map_err(|error| persistence_error("finalize curator audit receipt", error))?;
    Ok(record)
}

fn find_idempotent_receipt(
    tx: &rusqlite::Transaction<'_>,
    input: &CuratorAuditReceiptRecord,
) -> CoreResult<Option<CuratorAuditReceiptRecord>> {
    let Some(key) = input.idempotency_key.as_deref() else {
        return Ok(None);
    };
    let existing = tx.query_row(
        "SELECT record_json FROM module_curator_audit_receipts WHERE activity_kind=?1 AND idempotency_key=?2",
        params![input.activity_kind, key],
        |row| row.get::<_, String>(0),
    ).optional().map_err(|error| persistence_error("read curator idempotency receipt", error))?;
    existing.map(|json| parse_json(&json)).transpose()
}

fn idempotent_result(
    tx: &rusqlite::Transaction<'_>,
    write: &CuratorGovernanceWrite,
    existing: CuratorAuditReceiptRecord,
) -> CoreResult<CuratorGovernanceWriteResult> {
    let mut expected = write.receipt.clone();
    expected.sequence = existing.sequence;
    if expected != existing {
        return conflict("curator_idempotency_conflict");
    }
    let candidate = write
        .candidate
        .as_ref()
        .map(|value| {
            get_json_record(
                tx,
                "module_curator_candidates",
                "candidate_id",
                &value.record.candidate_id,
            )
        })
        .transpose()?
        .flatten();
    let mutation = write
        .mutation
        .as_ref()
        .map(|value| {
            get_json_record(
                tx,
                "module_curator_mutations",
                "mutation_id",
                &value.record.mutation_id,
            )
        })
        .transpose()?
        .flatten();
    Ok(CuratorGovernanceWriteResult {
        candidate,
        mutation,
        receipt: existing,
        idempotent_replay: true,
    })
}

fn purge_curator_scope(
    store: &CoordinationStore,
    column: &str,
    value: &str,
) -> CoreResult<CuratorPurgeReport> {
    validate_id(column, value)?;
    let mut conn = store.conn()?;
    let tx = conn
        .transaction()
        .map_err(|error| persistence_error("start curator purge", error))?;
    let candidate_predicate = format!("{column}=?1");
    let candidates =
        tx.execute(
            &format!("DELETE FROM module_curator_candidates WHERE {candidate_predicate}"),
            params![value],
        )
        .map_err(|error| persistence_error("purge curator candidates", error))? as u64;
    let approvals = tx.execute("DELETE FROM module_curator_approvals WHERE candidate_id NOT IN (SELECT candidate_id FROM module_curator_candidates)", []).map_err(|error| persistence_error("purge curator approvals", error))? as u64;
    let snapshots = tx.execute("DELETE FROM module_curator_snapshot_refs WHERE candidate_id NOT IN (SELECT candidate_id FROM module_curator_candidates)", []).map_err(|error| persistence_error("purge curator snapshots", error))? as u64;
    let mutations = tx.execute("DELETE FROM module_curator_mutations WHERE candidate_id NOT IN (SELECT candidate_id FROM module_curator_candidates)", []).map_err(|error| persistence_error("purge curator mutations", error))? as u64;
    let audit_receipts =
        tx.execute(
            &format!("DELETE FROM module_curator_audit_receipts WHERE {candidate_predicate}"),
            params![value],
        )
        .map_err(|error| persistence_error("purge curator audit receipts", error))? as u64;
    tx.commit()
        .map_err(|error| persistence_error("commit curator purge", error))?;
    Ok(CuratorPurgeReport {
        candidates,
        approvals,
        snapshots,
        mutations,
        audit_receipts,
    })
}

fn validate_governance_write(write: &CuratorGovernanceWrite) -> CoreResult<()> {
    validate_id("receipt_id", &write.receipt.receipt_id)?;
    validate_id("activity_kind", &write.receipt.activity_kind)?;
    if let Some(candidate) = &write.candidate {
        validate_id("candidate_id", &candidate.record.candidate_id)?;
        validate_id("profile_id", &candidate.record.profile_id)?;
        validate_id("fingerprint", &candidate.record.fingerprint)?;
    }
    Ok(())
}

fn validate_id(label: &str, value: &str) -> CoreResult<()> {
    if value.trim().is_empty() || value.len() > 512 {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("{label} must be 1..=512 characters"),
        ));
    }
    Ok(())
}

fn validate_snapshot_root_ref(value: &str) -> CoreResult<()> {
    let path = std::path::Path::new(value);
    if value.trim().is_empty()
        || path.is_absolute()
        || path.components().any(|part| {
            matches!(
                part,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "curator_snapshot_ref_invalid",
        ));
    }
    Ok(())
}

fn next_revision(current: Option<u64>, expected: Option<u64>) -> CoreResult<u64> {
    match (current, expected) {
        (None, None | Some(0)) => Ok(1),
        (Some(current), Some(expected)) if current == expected => Ok(current + 1),
        _ => conflict("curator_candidate_revision_conflict"),
    }
}

fn conflict<T>(message: &str) -> CoreResult<T> {
    Err(CoreError::new(CoreErrorKind::ActionRejected, message))
}

pub(crate) fn validate_candidate_transition(
    current: &CuratorCandidateStatus,
    next: &CuratorCandidateStatus,
) -> CoreResult<()> {
    let valid = current == next
        || matches!(
            (current, next),
            (CuratorCandidateStatus::Proposed, CuratorCandidateStatus::Previewed)
                | (CuratorCandidateStatus::Proposed, CuratorCandidateStatus::Approved)
                | (CuratorCandidateStatus::Previewed, CuratorCandidateStatus::Approved)
                | (CuratorCandidateStatus::Approved, CuratorCandidateStatus::Applied)
        );
    if valid {
        Ok(())
    } else {
        conflict("curator_candidate_transition_rejected")
    }
}

pub(crate) fn validate_mutation_transition(
    current: &CuratorMutationStatus,
    next: &CuratorMutationStatus,
) -> CoreResult<()> {
    let valid = current == next
        || matches!(
            (current, next),
            (CuratorMutationStatus::Prepared, CuratorMutationStatus::Applied)
                | (CuratorMutationStatus::Prepared, CuratorMutationStatus::Failed)
                | (CuratorMutationStatus::Applied, CuratorMutationStatus::RollbackPrepared)
                | (CuratorMutationStatus::Applied, CuratorMutationStatus::RolledBack)
                | (CuratorMutationStatus::Applied, CuratorMutationStatus::RollbackFailed)
                | (
                    CuratorMutationStatus::RollbackPrepared,
                    CuratorMutationStatus::RolledBack
                )
                | (
                    CuratorMutationStatus::RollbackPrepared,
                    CuratorMutationStatus::RollbackFailed
                )
        );
    if valid {
        Ok(())
    } else {
        conflict("curator_mutation_already_terminal")
    }
}

fn bounded_page(page: Option<QueryPage>) -> (i64, i64) {
    page.unwrap_or(QueryPage {
        limit: None,
        offset: None,
    })
    .bounded(50, 200)
}

fn get_json_record<T: DeserializeOwned>(
    conn: &rusqlite::Connection,
    table: &str,
    id_column: &str,
    id: &str,
) -> CoreResult<Option<T>> {
    conn.query_row(
        &format!("SELECT record_json FROM {table} WHERE {id_column}=?1"),
        params![id],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(|error| persistence_error("read curator record", error))?
    .map(|json| parse_json(&json))
    .transpose()
}

fn collect_json_rows<T: DeserializeOwned>(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<String>>,
    _label: &str,
) -> CoreResult<Vec<T>> {
    rows.map(|row| {
        let json = row.map_err(|error| persistence_error("read curator query row", error))?;
        parse_json(&json)
    })
    .collect()
}

fn parse_json<T: DeserializeOwned>(json: &str) -> CoreResult<T> {
    serde_json::from_str(json).map_err(|error| {
        CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("parse curator record: {error}"),
        )
    })
}

fn candidate_status(status: &CuratorCandidateStatus) -> &'static str {
    match status {
        CuratorCandidateStatus::Proposed => "proposed",
        CuratorCandidateStatus::Previewed => "previewed",
        CuratorCandidateStatus::Approved => "approved",
        CuratorCandidateStatus::Applied => "applied",
    }
}
fn lifecycle_state(state: &CuratorCandidateLifecycleState) -> &'static str {
    match state {
        CuratorCandidateLifecycleState::Active => "active",
        CuratorCandidateLifecycleState::Stale => "stale",
        CuratorCandidateLifecycleState::Archived => "archived",
    }
}
fn mutation_status(status: &CuratorMutationStatus) -> &'static str {
    match status {
        CuratorMutationStatus::Prepared => "prepared",
        CuratorMutationStatus::Applied => "applied",
        CuratorMutationStatus::Failed => "failed",
        CuratorMutationStatus::RollbackPrepared => "rollback_prepared",
        CuratorMutationStatus::RolledBack => "rolled_back",
        CuratorMutationStatus::RollbackFailed => "rollback_failed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_curator_repository_covers_lifecycle_idempotency_rollback_and_purge() {
        let path = temp_db("curator-repository");
        let store = CoordinationStore::open_file(&path).unwrap();
        let created = store
            .apply_curator_governance_write(&governance_write(
                candidate(
                    CuratorCandidateStatus::Proposed,
                    CuratorCandidateLifecycleState::Active,
                    0,
                ),
                "receipt-created",
                "candidate_discovered",
                "create-one",
            ))
            .unwrap();
        assert_eq!(created.candidate.as_ref().unwrap().revision, 1);
        assert_eq!(created.receipt.sequence, 1);

        let replay = store
            .apply_curator_governance_write(&governance_write(
                candidate(
                    CuratorCandidateStatus::Proposed,
                    CuratorCandidateLifecycleState::Active,
                    0,
                ),
                "receipt-created",
                "candidate_discovered",
                "create-one",
            ))
            .unwrap();
        assert!(replay.idempotent_replay);
        assert_eq!(replay.receipt.sequence, 1);

        let mut stale = candidate(
            CuratorCandidateStatus::Proposed,
            CuratorCandidateLifecycleState::Stale,
            1,
        );
        stale.lifecycle_reason_code = Some("curator_candidate_stale".into());
        let transitioned = store
            .apply_curator_governance_write(&governance_write(
                stale.clone(),
                "receipt-stale",
                "candidate_staled",
                "stale-one",
            ))
            .unwrap();
        assert_eq!(transitioned.candidate.unwrap().revision, 2);
        assert_eq!(transitioned.receipt.sequence, 2);

        let stale_revision = store
            .apply_curator_governance_write(&governance_write(
                stale,
                "receipt-stale-loser",
                "candidate_staled",
                "stale-loser",
            ))
            .unwrap_err();
        assert_eq!(stale_revision.kind, CoreErrorKind::ActionRejected);

        let mutation = CuratorMutationRecord {
            mutation_id: "mutation-one".into(),
            receipt_id: "receipt-mutation".into(),
            candidate_id: "candidate-one".into(),
            candidate_revision: 2,
            action: "skill_patch".into(),
            actor_id: Some("operator".into()),
            reason: "apply".into(),
            snapshot_id: "snapshot-one".into(),
            mutation_payload: serde_json::json!({}),
            changed_paths: vec!["skill.md".into()],
            management: None,
            status: CuratorMutationStatus::Applied,
            error_reason_code: None,
            revision: 0,
            created_at: now(),
            applied_at: Some(now()),
            rolled_back_at: None,
        };
        store
            .apply_curator_governance_write(&CuratorGovernanceWrite {
                candidate: None,
                approval: None,
                snapshot: Some(CuratorSnapshotRefRecord {
                    snapshot_id: "snapshot-one".into(),
                    candidate_id: "candidate-one".into(),
                    snapshot_root_ref: "candidate-one/snapshot-one".into(),
                    manifest: serde_json::json!({"skill":"skill.md"}),
                    status: "consumed".into(),
                    created_at: now(),
                    verified_at: Some(now()),
                }),
                mutation: Some(CuratorMutationWrite {
                    record: mutation.clone(),
                    expected_revision: None,
                }),
                receipt: receipt("receipt-mutation", "mutation_applied", "apply-one"),
            })
            .unwrap();
        let mut rolled_back = mutation;
        rolled_back.status = CuratorMutationStatus::RolledBack;
        rolled_back.rolled_back_at = Some(now());
        let result = store
            .apply_curator_governance_write(&CuratorGovernanceWrite {
                candidate: None,
                approval: None,
                snapshot: None,
                mutation: Some(CuratorMutationWrite {
                    record: rolled_back,
                    expected_revision: Some(1),
                }),
                receipt: receipt("receipt-rollback", "rollback_completed", "rollback-one"),
            })
            .unwrap();
        assert_eq!(result.mutation.unwrap().revision, 2);
        assert_eq!(result.receipt.sequence, 4);
        let audit = store
            .list_curator_audit_receipts(&CuratorAuditQuery::default())
            .unwrap();
        assert_eq!(
            audit
                .items
                .iter()
                .map(|item| item.sequence)
                .collect::<Vec<_>>(),
            vec![1, 2, 3, 4]
        );

        let report = store.purge_curator_profile("profile-one").unwrap();
        assert_eq!(report.candidates, 1);
        assert_eq!(report.mutations, 1);
        assert!(store
            .get_curator_candidate("candidate-one")
            .unwrap()
            .is_none());
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn concurrent_candidate_updates_allow_one_revision_winner() {
        let path = temp_db("curator-concurrency");
        let store = CoordinationStore::open_file(&path).unwrap();
        store
            .apply_curator_governance_write(&governance_write(
                candidate(
                    CuratorCandidateStatus::Proposed,
                    CuratorCandidateLifecycleState::Active,
                    0,
                ),
                "receipt-created",
                "candidate_discovered",
                "create-one",
            ))
            .unwrap();
        let results = (0..2)
            .map(|index| {
                let store = store.clone();
                std::thread::spawn(move || {
                    store.apply_curator_governance_write(&governance_write(
                        candidate(
                            CuratorCandidateStatus::Previewed,
                            CuratorCandidateLifecycleState::Active,
                            1,
                        ),
                        &format!("receipt-preview-{index}"),
                        "candidate_previewed",
                        &format!("preview-{index}"),
                    ))
                })
            })
            .map(|thread| thread.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(results.iter().filter(|result| result.is_err()).count(), 1);
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    fn candidate(
        status: CuratorCandidateStatus,
        lifecycle_state: CuratorCandidateLifecycleState,
        revision: u64,
    ) -> CuratorCandidateRecord {
        CuratorCandidateRecord {
            candidate_id: "candidate-one".into(),
            batch_id: "batch-one".into(),
            profile_id: "profile-one".into(),
            session_id: Some("session-one".into()),
            kind: "skill_patch".into(),
            summary: "Patch a skill".into(),
            fingerprint: "fingerprint-one".into(),
            candidate_payload: serde_json::json!({}),
            mutation: serde_json::json!({"type":"skill_patch"}),
            source_refs: vec![],
            expires_at: None,
            status,
            lifecycle_state,
            lifecycle_reason_code: None,
            revision,
            created_at: now(),
            updated_at: now(),
        }
    }

    fn governance_write(
        record: CuratorCandidateRecord,
        receipt_id: &str,
        activity_kind: &str,
        idempotency_key: &str,
    ) -> CuratorGovernanceWrite {
        let expected_revision = (record.revision > 0).then_some(record.revision);
        CuratorGovernanceWrite {
            candidate: Some(CuratorCandidateWrite {
                record,
                expected_revision,
            }),
            approval: None,
            snapshot: None,
            mutation: None,
            receipt: receipt(receipt_id, activity_kind, idempotency_key),
        }
    }

    fn receipt(
        receipt_id: &str,
        activity_kind: &str,
        idempotency_key: &str,
    ) -> CuratorAuditReceiptRecord {
        CuratorAuditReceiptRecord {
            sequence: 0,
            receipt_id: receipt_id.into(),
            correlation_id: Some("correlation-one".into()),
            idempotency_key: Some(idempotency_key.into()),
            profile_id: Some("profile-one".into()),
            session_id: Some("session-one".into()),
            candidate_id: Some("candidate-one".into()),
            mutation_id: activity_kind
                .contains("mutation")
                .then(|| "mutation-one".into()),
            activity_kind: activity_kind.into(),
            outcome: "accepted".into(),
            reason_code: None,
            summary: activity_kind.into(),
            actor_id: Some("operator".into()),
            details: None,
            occurred_at: now(),
        }
    }

    fn now() -> String {
        "2026-07-10T00:00:00Z".into()
    }

    fn temp_db(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "rusty-crew-{label}-{}-{}.sqlite3",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
}
