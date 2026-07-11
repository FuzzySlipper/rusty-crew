//! Typed PostgreSQL curator governance repositories.

use super::*;
use crate::{
    CuratorAuditQuery, CuratorAuditReceiptRecord, CuratorCandidateLifecycleState,
    CuratorCandidateQuery, CuratorCandidateRecord, CuratorCandidateStatus, CuratorCandidateWrite,
    CuratorGovernanceWrite, CuratorGovernanceWriteResult, CuratorMutationQuery,
    CuratorMutationRecord, CuratorMutationStatus, CuratorMutationWrite, CuratorPurgeReport,
    ExactPage, QueryPage,
};

impl PostgresBackendStore {
    pub fn apply_curator_governance_write(
        &self,
        write: &CuratorGovernanceWrite,
    ) -> CoreResult<CuratorGovernanceWriteResult> {
        validate_governance_write(write)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start PostgreSQL curator governance write", error))?;
        if let Some(existing) = find_idempotent_receipt(&mut tx, &schema, &write.receipt)? {
            let result = idempotent_result(&mut tx, &schema, write, existing)?;
            tx.rollback().map_err(|error| {
                postgres_error("rollback idempotent PostgreSQL curator write", error)
            })?;
            return Ok(result);
        }
        let candidate = write
            .candidate
            .as_ref()
            .map(|value| put_candidate(&mut tx, &schema, value))
            .transpose()?;
        if let Some(record) = &write.approval {
            tx.execute(
                &format!("INSERT INTO {schema}.module_curator_approvals (approval_id, receipt_id, candidate_id, actor_id, approved_at, record_json) VALUES ($1,$2,$3,$4,$5,$6::text::jsonb)"),
                &[&record.approval_id, &record.receipt_id, &record.candidate_id, &record.actor_id, &record.approved_at, &json(record)?],
            ).map_err(|error| postgres_error("write PostgreSQL curator approval", error))?;
        }
        if let Some(record) = &write.snapshot {
            validate_snapshot_root_ref(&record.snapshot_root_ref)?;
            tx.execute(
                &format!("INSERT INTO {schema}.module_curator_snapshot_refs (snapshot_id,candidate_id,status,created_at,record_json) VALUES ($1,$2,$3,$4,$5::text::jsonb) ON CONFLICT(snapshot_id) DO UPDATE SET status=EXCLUDED.status, record_json=EXCLUDED.record_json"),
                &[&record.snapshot_id, &record.candidate_id, &record.status, &record.created_at, &json(record)?],
            ).map_err(|error| postgres_error("write PostgreSQL curator snapshot", error))?;
        }
        let mutation = write
            .mutation
            .as_ref()
            .map(|value| put_mutation(&mut tx, &schema, value))
            .transpose()?;
        let receipt = insert_receipt(&mut tx, &schema, &write.receipt)?;
        tx.commit()
            .map_err(|error| postgres_error("commit PostgreSQL curator governance write", error))?;
        Ok(CuratorGovernanceWriteResult {
            candidate,
            mutation,
            receipt,
            idempotent_replay: false,
        })
    }

    pub fn get_curator_candidate(&self, id: &str) -> CoreResult<Option<CuratorCandidateRecord>> {
        validate_id("candidate_id", id)?;
        self.get_curator_json("module_curator_candidates", "candidate_id", id)
    }

    pub fn list_curator_candidates(
        &self,
        query: &CuratorCandidateQuery,
    ) -> CoreResult<ExactPage<CuratorCandidateRecord>> {
        let schema = self.quoted_schema();
        let (limit, offset) = bounded_page(query.page);
        let status = query.status.as_ref().map(candidate_status);
        let lifecycle = query.lifecycle_state.as_ref().map(lifecycle_state);
        let predicate = "($1::TEXT IS NULL OR profile_id=$1) AND ($2::TEXT IS NULL OR session_id=$2) AND ($3::TEXT IS NULL OR status=$3) AND ($4::TEXT IS NULL OR lifecycle_state=$4)";
        let mut client = self.client()?;
        let total: i64 = client
            .query_one(
                &format!(
                    "SELECT COUNT(*) FROM {schema}.module_curator_candidates WHERE {predicate}"
                ),
                &[&query.profile_id, &query.session_id, &status, &lifecycle],
            )
            .map_err(|error| postgres_error("count PostgreSQL curator candidates", error))?
            .get(0);
        let items = client.query(&format!("SELECT record_json::text FROM {schema}.module_curator_candidates WHERE {predicate} ORDER BY updated_at DESC, candidate_id ASC LIMIT $5 OFFSET $6"), &[&query.profile_id, &query.session_id, &status, &lifecycle, &limit, &offset]).map_err(|error| postgres_error("query PostgreSQL curator candidates", error))?.into_iter().map(parse_row).collect::<CoreResult<Vec<_>>>()?;
        Ok(ExactPage::new(
            items,
            total as u64,
            limit as u32,
            offset as u32,
        ))
    }

    pub fn get_curator_mutation(&self, id: &str) -> CoreResult<Option<CuratorMutationRecord>> {
        validate_id("mutation_id", id)?;
        self.get_curator_json("module_curator_mutations", "mutation_id", id)
    }

    pub fn list_curator_mutations(
        &self,
        query: &CuratorMutationQuery,
    ) -> CoreResult<ExactPage<CuratorMutationRecord>> {
        let schema = self.quoted_schema();
        let (limit, offset) = bounded_page(query.page);
        let status = query.status.as_ref().map(mutation_status);
        let predicate = "($1::TEXT IS NULL OR candidate_id=$1) AND ($2::TEXT IS NULL OR status=$2)";
        let mut client = self.client()?;
        let total: i64 = client
            .query_one(
                &format!(
                    "SELECT COUNT(*) FROM {schema}.module_curator_mutations WHERE {predicate}"
                ),
                &[&query.candidate_id, &status],
            )
            .map_err(|error| postgres_error("count PostgreSQL curator mutations", error))?
            .get(0);
        let items = client.query(&format!("SELECT record_json::text FROM {schema}.module_curator_mutations WHERE {predicate} ORDER BY created_at DESC, mutation_id ASC LIMIT $3 OFFSET $4"), &[&query.candidate_id, &status, &limit, &offset]).map_err(|error| postgres_error("query PostgreSQL curator mutations", error))?.into_iter().map(parse_row).collect::<CoreResult<Vec<_>>>()?;
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
        let schema = self.quoted_schema();
        let (limit, offset) = bounded_page(query.page);
        let predicate = "($1::TEXT IS NULL OR profile_id=$1) AND ($2::TEXT IS NULL OR session_id=$2) AND ($3::TEXT IS NULL OR candidate_id=$3) AND ($4::TEXT IS NULL OR mutation_id=$4) AND ($5::TEXT IS NULL OR activity_kind=$5)";
        let mut client = self.client()?;
        let total: i64 = client
            .query_one(
                &format!(
                    "SELECT COUNT(*) FROM {schema}.module_curator_audit_receipts WHERE {predicate}"
                ),
                &[
                    &query.profile_id,
                    &query.session_id,
                    &query.candidate_id,
                    &query.mutation_id,
                    &query.activity_kind,
                ],
            )
            .map_err(|error| postgres_error("count PostgreSQL curator audit receipts", error))?
            .get(0);
        let items = client.query(&format!("SELECT record_json::text FROM {schema}.module_curator_audit_receipts WHERE {predicate} ORDER BY sequence ASC LIMIT $6 OFFSET $7"), &[&query.profile_id, &query.session_id, &query.candidate_id, &query.mutation_id, &query.activity_kind, &limit, &offset]).map_err(|error| postgres_error("query PostgreSQL curator audit receipts", error))?.into_iter().map(parse_row).collect::<CoreResult<Vec<_>>>()?;
        Ok(ExactPage::new(
            items,
            total as u64,
            limit as u32,
            offset as u32,
        ))
    }

    pub fn purge_curator_profile(&self, profile_id: &str) -> CoreResult<CuratorPurgeReport> {
        self.purge_curator_scope("profile_id", profile_id)
    }
    pub fn purge_curator_session(&self, session_id: &str) -> CoreResult<CuratorPurgeReport> {
        self.purge_curator_scope("session_id", session_id)
    }

    fn purge_curator_scope(&self, column: &str, value: &str) -> CoreResult<CuratorPurgeReport> {
        validate_id(column, value)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start PostgreSQL curator purge", error))?;
        let candidates = tx
            .execute(
                &format!("DELETE FROM {schema}.module_curator_candidates WHERE {column}=$1"),
                &[&value],
            )
            .map_err(|error| postgres_error("purge PostgreSQL curator candidates", error))?;
        let approvals = tx.execute(&format!("DELETE FROM {schema}.module_curator_approvals WHERE candidate_id NOT IN (SELECT candidate_id FROM {schema}.module_curator_candidates)"), &[]).map_err(|error| postgres_error("purge PostgreSQL curator approvals", error))?;
        let snapshots = tx.execute(&format!("DELETE FROM {schema}.module_curator_snapshot_refs WHERE candidate_id NOT IN (SELECT candidate_id FROM {schema}.module_curator_candidates)"), &[]).map_err(|error| postgres_error("purge PostgreSQL curator snapshots", error))?;
        let mutations = tx.execute(&format!("DELETE FROM {schema}.module_curator_mutations WHERE candidate_id NOT IN (SELECT candidate_id FROM {schema}.module_curator_candidates)"), &[]).map_err(|error| postgres_error("purge PostgreSQL curator mutations", error))?;
        let audit_receipts = tx
            .execute(
                &format!("DELETE FROM {schema}.module_curator_audit_receipts WHERE {column}=$1"),
                &[&value],
            )
            .map_err(|error| postgres_error("purge PostgreSQL curator audit receipts", error))?;
        tx.commit()
            .map_err(|error| postgres_error("commit PostgreSQL curator purge", error))?;
        Ok(CuratorPurgeReport {
            candidates,
            approvals,
            snapshots,
            mutations,
            audit_receipts,
        })
    }

    fn get_curator_json<T: serde::de::DeserializeOwned>(
        &self,
        table: &str,
        column: &str,
        id: &str,
    ) -> CoreResult<Option<T>> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        client
            .query_opt(
                &format!("SELECT record_json::text FROM {schema}.{table} WHERE {column}=$1"),
                &[&id],
            )
            .map_err(|error| postgres_error("read PostgreSQL curator record", error))?
            .map(parse_row)
            .transpose()
    }
}

fn put_candidate(
    tx: &mut Transaction<'_>,
    schema: &str,
    write: &CuratorCandidateWrite,
) -> CoreResult<CuratorCandidateRecord> {
    let current: Option<CuratorCandidateRecord> = tx.query_opt(&format!("SELECT record_json::text FROM {schema}.module_curator_candidates WHERE candidate_id=$1 FOR UPDATE"), &[&write.record.candidate_id]).map_err(|error| postgres_error("lock PostgreSQL curator candidate", error))?.map(parse_row).transpose()?;
    if let Some(current) = &current {
        if current.fingerprint != write.record.fingerprint {
            return conflict("curator_candidate_fingerprint_conflict");
        }
        crate::repos::curator::validate_candidate_transition(
            &current.status,
            &write.record.status,
        )?;
    }
    let mut record = write.record.clone();
    record.revision = next_revision(
        current
            .as_ref()
            .map(|value: &CuratorCandidateRecord| value.revision),
        write.expected_revision,
    )?;
    tx.execute(&format!("INSERT INTO {schema}.module_curator_candidates (candidate_id,batch_id,profile_id,session_id,status,lifecycle_state,fingerprint,expires_at,revision,record_json,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10::text::jsonb,$11,$12) ON CONFLICT(candidate_id) DO UPDATE SET batch_id=EXCLUDED.batch_id,profile_id=EXCLUDED.profile_id,session_id=EXCLUDED.session_id,status=EXCLUDED.status,lifecycle_state=EXCLUDED.lifecycle_state,fingerprint=EXCLUDED.fingerprint,expires_at=EXCLUDED.expires_at,revision=EXCLUDED.revision,record_json=EXCLUDED.record_json,updated_at=EXCLUDED.updated_at"), &[&record.candidate_id,&record.batch_id,&record.profile_id,&record.session_id,&candidate_status(&record.status),&lifecycle_state(&record.lifecycle_state),&record.fingerprint,&record.expires_at,&(record.revision as i64),&json(&record)?,&record.created_at,&record.updated_at]).map_err(|error| postgres_error("write PostgreSQL curator candidate", error))?;
    Ok(record)
}

fn put_mutation(
    tx: &mut Transaction<'_>,
    schema: &str,
    write: &CuratorMutationWrite,
) -> CoreResult<CuratorMutationRecord> {
    let current: Option<CuratorMutationRecord> = tx.query_opt(&format!("SELECT record_json::text FROM {schema}.module_curator_mutations WHERE mutation_id=$1 FOR UPDATE"), &[&write.record.mutation_id]).map_err(|error| postgres_error("lock PostgreSQL curator mutation", error))?.map(parse_row).transpose()?;
    if let Some(current) = &current {
        crate::repos::curator::validate_mutation_transition(&current.status, &write.record.status)?;
    }
    let mut record = write.record.clone();
    record.revision = next_revision(
        current
            .as_ref()
            .map(|value: &CuratorMutationRecord| value.revision),
        write.expected_revision,
    )?;
    tx.execute(&format!("INSERT INTO {schema}.module_curator_mutations (mutation_id,receipt_id,candidate_id,snapshot_id,actor_id,status,revision,record_json,created_at,applied_at,rolled_back_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8::text::jsonb,$9,$10,$11) ON CONFLICT(mutation_id) DO UPDATE SET status=EXCLUDED.status,revision=EXCLUDED.revision,record_json=EXCLUDED.record_json,applied_at=EXCLUDED.applied_at,rolled_back_at=EXCLUDED.rolled_back_at"), &[&record.mutation_id,&record.receipt_id,&record.candidate_id,&record.snapshot_id,&record.actor_id,&mutation_status(&record.status),&(record.revision as i64),&json(&record)?,&record.created_at,&record.applied_at,&record.rolled_back_at]).map_err(|error| postgres_error("write PostgreSQL curator mutation", error))?;
    Ok(record)
}

fn insert_receipt(
    tx: &mut Transaction<'_>,
    schema: &str,
    input: &CuratorAuditReceiptRecord,
) -> CoreResult<CuratorAuditReceiptRecord> {
    let mut record = input.clone();
    record.sequence = 0;
    let sequence: i64 = tx.query_one(&format!("INSERT INTO {schema}.module_curator_audit_receipts (receipt_id,correlation_id,idempotency_key,profile_id,session_id,candidate_id,mutation_id,activity_kind,outcome,reason_code,occurred_at,record_json) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12::text::jsonb) RETURNING sequence"), &[&record.receipt_id,&record.correlation_id,&record.idempotency_key,&record.profile_id,&record.session_id,&record.candidate_id,&record.mutation_id,&record.activity_kind,&record.outcome,&record.reason_code,&record.occurred_at,&json(&record)?]).map_err(|error| postgres_error("write PostgreSQL curator receipt", error))?.get(0);
    record.sequence = sequence as u64;
    tx.execute(&format!("UPDATE {schema}.module_curator_audit_receipts SET record_json=$1::text::jsonb WHERE sequence=$2"), &[&json(&record)?, &sequence]).map_err(|error| postgres_error("finalize PostgreSQL curator receipt", error))?;
    Ok(record)
}

fn find_idempotent_receipt(
    tx: &mut Transaction<'_>,
    schema: &str,
    input: &CuratorAuditReceiptRecord,
) -> CoreResult<Option<CuratorAuditReceiptRecord>> {
    let Some(key) = input.idempotency_key.as_deref() else {
        return Ok(None);
    };
    tx.query_opt(&format!("SELECT record_json::text FROM {schema}.module_curator_audit_receipts WHERE activity_kind=$1 AND idempotency_key=$2 FOR UPDATE"), &[&input.activity_kind,&key]).map_err(|error| postgres_error("read PostgreSQL curator idempotency receipt", error))?.map(parse_row).transpose()
}

fn idempotent_result(
    tx: &mut Transaction<'_>,
    schema: &str,
    write: &CuratorGovernanceWrite,
    existing: CuratorAuditReceiptRecord,
) -> CoreResult<CuratorGovernanceWriteResult> {
    let mut expected = write.receipt.clone();
    expected.sequence = existing.sequence;
    if expected != existing {
        return conflict("curator_idempotency_conflict");
    }
    let candidate=write.candidate.as_ref().map(|v| tx.query_opt(&format!("SELECT record_json::text FROM {schema}.module_curator_candidates WHERE candidate_id=$1"), &[&v.record.candidate_id]).map_err(|e| postgres_error("read idempotent PostgreSQL curator candidate",e))?.map(parse_row).transpose()).transpose()?.flatten();
    let mutation=write.mutation.as_ref().map(|v| tx.query_opt(&format!("SELECT record_json::text FROM {schema}.module_curator_mutations WHERE mutation_id=$1"), &[&v.record.mutation_id]).map_err(|e| postgres_error("read idempotent PostgreSQL curator mutation",e))?.map(parse_row).transpose()).transpose()?.flatten();
    Ok(CuratorGovernanceWriteResult {
        candidate,
        mutation,
        receipt: existing,
        idempotent_replay: true,
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
fn bounded_page(page: Option<QueryPage>) -> (i64, i64) {
    page.unwrap_or(QueryPage {
        limit: None,
        offset: None,
    })
    .bounded(50, 200)
}
fn json<T: serde::Serialize>(value: &T) -> CoreResult<String> {
    serde_json::to_string(value).map_err(|error| {
        CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("serialize curator record: {error}"),
        )
    })
}
fn parse_row<T: serde::de::DeserializeOwned>(row: Row) -> CoreResult<T> {
    serde_json::from_str(row.get::<_, String>(0).as_str()).map_err(|error| {
        CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("parse PostgreSQL curator record: {error}"),
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
    use crate::{CuratorAuditReceiptRecord, CuratorCandidateRecord, CuratorGovernanceWrite};

    #[test]
    #[ignore = "requires local PostgreSQL dev database env"]
    fn postgres_curator_repository_matches_typed_revision_and_idempotency_contract() {
        let database_url = std::env::var("RUSTY_CREW_TEST_POSTGRES_URL")
            .or_else(|_| std::env::var("RUSTY_CREW_DATABASE_URL"))
            .expect("PostgreSQL test database URL");
        let schema = format!(
            "rusty_crew_curator_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let store = PostgresBackendStore::connect(&database_url, &schema).unwrap();
        let write = candidate_write("receipt-one", "idempotency-one", 0, None);
        let created = store.apply_curator_governance_write(&write).unwrap();
        assert_eq!(created.candidate.as_ref().unwrap().revision, 1);
        assert_eq!(created.receipt.sequence, 1);
        assert!(
            store
                .apply_curator_governance_write(&write)
                .unwrap()
                .idempotent_replay
        );

        let updated = candidate_write("receipt-two", "idempotency-two", 1, Some(1));
        assert_eq!(
            store
                .apply_curator_governance_write(&updated)
                .unwrap()
                .candidate
                .unwrap()
                .revision,
            2
        );
        assert_eq!(
            store
                .apply_curator_governance_write(&candidate_write(
                    "receipt-three",
                    "idempotency-three",
                    1,
                    Some(1),
                ))
                .unwrap_err()
                .kind,
            CoreErrorKind::ActionRejected
        );
        let audit = store
            .list_curator_audit_receipts(&CuratorAuditQuery::default())
            .unwrap();
        assert_eq!(audit.total, 2);
        assert_eq!(
            audit
                .items
                .iter()
                .map(|item| item.sequence)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(
            store
                .purge_curator_profile("profile-pg")
                .unwrap()
                .candidates,
            1
        );
        store.drop_schema_for_test().unwrap();
    }

    fn candidate_write(
        receipt_id: &str,
        idempotency_key: &str,
        revision: u64,
        expected_revision: Option<u64>,
    ) -> CuratorGovernanceWrite {
        CuratorGovernanceWrite {
            candidate: Some(CuratorCandidateWrite {
                record: CuratorCandidateRecord {
                    candidate_id: "candidate-pg".into(),
                    batch_id: "batch-pg".into(),
                    profile_id: "profile-pg".into(),
                    session_id: Some("session-pg".into()),
                    kind: "skill_patch".into(),
                    summary: "Patch".into(),
                    fingerprint: "fingerprint-pg".into(),
                    candidate_payload: serde_json::json!({}),
                    mutation: serde_json::json!({"type":"skill_patch"}),
                    source_refs: Vec::new(),
                    expires_at: None,
                    status: if revision == 0 {
                        CuratorCandidateStatus::Proposed
                    } else {
                        CuratorCandidateStatus::Previewed
                    },
                    lifecycle_state: CuratorCandidateLifecycleState::Active,
                    lifecycle_reason_code: None,
                    revision,
                    created_at: "2026-07-10T00:00:00Z".into(),
                    updated_at: "2026-07-10T00:00:00Z".into(),
                },
                expected_revision,
            }),
            approval: None,
            snapshot: None,
            mutation: None,
            receipt: CuratorAuditReceiptRecord {
                sequence: 0,
                receipt_id: receipt_id.into(),
                correlation_id: Some("correlation-pg".into()),
                idempotency_key: Some(idempotency_key.into()),
                profile_id: Some("profile-pg".into()),
                session_id: Some("session-pg".into()),
                candidate_id: Some("candidate-pg".into()),
                mutation_id: None,
                activity_kind: if revision == 0 {
                    "candidate_discovered".into()
                } else {
                    "candidate_previewed".into()
                },
                outcome: "accepted".into(),
                reason_code: None,
                summary: "test".into(),
                actor_id: Some("test".into()),
                details: None,
                occurred_at: "2026-07-10T00:00:00Z".into(),
            },
        }
    }
}
