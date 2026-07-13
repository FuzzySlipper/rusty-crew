use super::*;
use crate::repos::roleplay_proposals::{
    roleplay_proposal_kind_as_str, roleplay_proposal_status_as_str,
    validate_roleplay_mechanic_proposal_persist, validate_roleplay_mechanic_proposal_query,
};
use crate::{
    RoleplayMechanicProposalApplyOutcome, RoleplayMechanicProposalDecision,
    RoleplayMechanicProposalDecisionKind, RoleplayMechanicProposalEvent,
    RoleplayMechanicProposalEventKind, RoleplayMechanicProposalPersist,
    RoleplayMechanicProposalQuery, RoleplayMechanicProposalRecord, RoleplayMechanicProposalStatus,
};
use serde_json::Value as JsonValue;

impl PostgresBackendStore {
    pub fn create_roleplay_mechanic_proposal(
        &self,
        persist: &RoleplayMechanicProposalPersist,
    ) -> CoreResult<RoleplayMechanicProposalRecord> {
        validate_roleplay_mechanic_proposal_persist(persist)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start PostgreSQL roleplay mechanic proposal create", error)
        })?;
        if let Some(existing) = get_proposal(&mut tx, &schema, &persist.create.proposal_id)? {
            if proposal_matches_persist(&existing, persist) {
                tx.commit().map_err(|error| {
                    postgres_error(
                        "commit idempotent PostgreSQL roleplay mechanic proposal create",
                        error,
                    )
                })?;
                return Ok(existing);
            }
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "roleplay mechanic proposal {} already exists with different content",
                    persist.create.proposal_id
                ),
            ));
        }
        let record = new_proposal_record(persist);
        write_proposal(&mut tx, &schema, &record, true)?;
        tx.commit().map_err(|error| {
            postgres_error("commit PostgreSQL roleplay mechanic proposal create", error)
        })?;
        Ok(record)
    }

    pub fn get_roleplay_mechanic_proposal(
        &self,
        proposal_id: &str,
    ) -> CoreResult<Option<RoleplayMechanicProposalRecord>> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        get_proposal(&mut *client, &schema, proposal_id)
    }

    pub fn list_roleplay_mechanic_proposals(
        &self,
        query: &RoleplayMechanicProposalQuery,
    ) -> CoreResult<Vec<RoleplayMechanicProposalRecord>> {
        validate_roleplay_mechanic_proposal_query(query)?;
        let schema = self.quoted_schema();
        let status = query.status.map(roleplay_proposal_status_as_str);
        let kind = query.kind.map(roleplay_proposal_kind_as_str);
        let (limit, offset) = query
            .page
            .unwrap_or(QueryPage {
                limit: None,
                offset: None,
            })
            .bounded(100, 1_000);
        let mechanic_session_id = query
            .mechanic_session_id
            .as_ref()
            .map(|value| value.0.as_str());
        let profile_id = query.profile_id.as_ref().map(|value| value.0.as_str());
        let mut client = self.client()?;
        client
            .query(
                &format!(
                    "SELECT record_json::text FROM {schema}.module_roleplay_mechanic_proposals
                     WHERE ($1::TEXT IS NULL OR mechanic_session_id = $1)
                       AND ($2::TEXT IS NULL OR roleplay_session_id = $2)
                       AND ($3::TEXT IS NULL OR profile_id = $3)
                       AND ($4::TEXT IS NULL OR status = $4)
                       AND ($5::TEXT IS NULL OR kind = $5)
                     ORDER BY updated_at DESC, proposal_id ASC LIMIT $6 OFFSET $7"
                ),
                &[
                    &mechanic_session_id,
                    &query.roleplay_session_id,
                    &profile_id,
                    &status,
                    &kind,
                    &limit,
                    &offset,
                ],
            )
            .map_err(|error| postgres_error("query PostgreSQL roleplay mechanic proposals", error))?
            .into_iter()
            .map(|row| {
                let json = row.get::<_, String>(0);
                parse_postgres_json(&json, "roleplay mechanic proposal")
            })
            .collect()
    }

    pub fn decide_roleplay_mechanic_proposal(
        &self,
        decision: &RoleplayMechanicProposalDecision,
    ) -> CoreResult<RoleplayMechanicProposalRecord> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error(
                "start PostgreSQL roleplay mechanic proposal decision",
                error,
            )
        })?;
        let mut record =
            get_proposal(&mut tx, &schema, &decision.proposal_id)?.ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    format!(
                        "roleplay mechanic proposal {} not found",
                        decision.proposal_id
                    ),
                )
            })?;
        let requested_status = match decision.decision {
            RoleplayMechanicProposalDecisionKind::Approve => {
                RoleplayMechanicProposalStatus::Approved
            }
            RoleplayMechanicProposalDecisionKind::Reject => {
                RoleplayMechanicProposalStatus::Rejected
            }
        };
        if record.status == requested_status {
            tx.commit().map_err(|error| {
                postgres_error(
                    "commit idempotent PostgreSQL roleplay mechanic proposal decision",
                    error,
                )
            })?;
            return Ok(record);
        }
        if record.status != RoleplayMechanicProposalStatus::Proposed {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "roleplay mechanic proposal {} is {:?}, not proposed",
                    decision.proposal_id, record.status
                ),
            ));
        }
        if record.revision != decision.expected_revision {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "roleplay mechanic proposal {} revision mismatch: expected {}, found {}",
                    decision.proposal_id, decision.expected_revision, record.revision
                ),
            ));
        }
        record.status = requested_status;
        record.reviewer_id = Some(decision.reviewer_id.clone());
        record.review_note = decision.note.clone();
        record.reviewed_at = Some(decision.now.clone());
        record.updated_at = decision.now.clone();
        record.revision += 1;
        record.history.push(RoleplayMechanicProposalEvent {
            event_id: format!("{}:review:{}", record.proposal_id, record.revision),
            kind: match decision.decision {
                RoleplayMechanicProposalDecisionKind::Approve => {
                    RoleplayMechanicProposalEventKind::Approved
                }
                RoleplayMechanicProposalDecisionKind::Reject => {
                    RoleplayMechanicProposalEventKind::Rejected
                }
            },
            actor_id: decision.reviewer_id.clone(),
            note: decision.note.clone(),
            target_revision: record.target_revision,
            details: JsonValue::Null,
            created_at: decision.now.clone(),
        });
        write_proposal(&mut tx, &schema, &record, false)?;
        tx.commit().map_err(|error| {
            postgres_error(
                "commit PostgreSQL roleplay mechanic proposal decision",
                error,
            )
        })?;
        Ok(record)
    }

    pub fn record_roleplay_mechanic_proposal_apply(
        &self,
        outcome: &RoleplayMechanicProposalApplyOutcome,
    ) -> CoreResult<RoleplayMechanicProposalRecord> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error(
                "start PostgreSQL roleplay mechanic proposal apply outcome",
                error,
            )
        })?;
        let mut record =
            get_proposal(&mut tx, &schema, &outcome.proposal_id)?.ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    format!(
                        "roleplay mechanic proposal {} not found",
                        outcome.proposal_id
                    ),
                )
            })?;
        if record.status == RoleplayMechanicProposalStatus::Applied && outcome.applied {
            tx.commit().map_err(|error| {
                postgres_error(
                    "commit idempotent PostgreSQL roleplay mechanic proposal apply",
                    error,
                )
            })?;
            return Ok(record);
        }
        if record.status != RoleplayMechanicProposalStatus::Approved {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "roleplay mechanic proposal {} is {:?}, not approved",
                    outcome.proposal_id, record.status
                ),
            ));
        }
        record.outcome = Some(outcome.outcome.clone());
        record.updated_at = outcome.now.clone();
        record.revision += 1;
        if outcome.applied {
            record.status = RoleplayMechanicProposalStatus::Applied;
            record.applied_at = Some(outcome.now.clone());
        }
        record.history.push(RoleplayMechanicProposalEvent {
            event_id: format!("{}:apply:{}", record.proposal_id, record.revision),
            kind: if outcome.applied {
                RoleplayMechanicProposalEventKind::Applied
            } else {
                RoleplayMechanicProposalEventKind::ApplyConflict
            },
            actor_id: outcome.actor_id.clone(),
            note: None,
            target_revision: outcome.target_revision,
            details: outcome.outcome.clone(),
            created_at: outcome.now.clone(),
        });
        write_proposal(&mut tx, &schema, &record, false)?;
        tx.commit().map_err(|error| {
            postgres_error(
                "commit PostgreSQL roleplay mechanic proposal apply outcome",
                error,
            )
        })?;
        Ok(record)
    }
}

fn new_proposal_record(
    persist: &RoleplayMechanicProposalPersist,
) -> RoleplayMechanicProposalRecord {
    RoleplayMechanicProposalRecord {
        proposal_id: persist.create.proposal_id.clone(),
        mechanic_session_id: persist.create.mechanic_session_id.clone(),
        roleplay_session_id: persist.create.roleplay_session_id.clone(),
        profile_id: persist.captured.profile_id.clone(),
        kind: persist.create.kind,
        target_id: persist.create.target_id.clone(),
        target_revision: persist.captured.target_revision,
        before_value: persist.captured.before_value.clone(),
        proposed_value: persist.create.proposed_value.clone(),
        rationale: persist.create.rationale.clone(),
        diagnostic_context: persist.create.diagnostic_context.clone(),
        status: RoleplayMechanicProposalStatus::Proposed,
        reviewer_id: None,
        review_note: None,
        reviewed_at: None,
        applied_at: None,
        outcome: None,
        revision: 1,
        history: vec![RoleplayMechanicProposalEvent {
            event_id: format!("{}:proposed:1", persist.create.proposal_id),
            kind: RoleplayMechanicProposalEventKind::Proposed,
            actor_id: persist.create.mechanic_session_id.0.clone(),
            note: Some(persist.create.rationale.clone()),
            target_revision: persist.captured.target_revision,
            details: persist.create.diagnostic_context.clone(),
            created_at: persist.create.now.clone(),
        }],
        created_at: persist.create.now.clone(),
        updated_at: persist.create.now.clone(),
    }
}

fn proposal_matches_persist(
    record: &RoleplayMechanicProposalRecord,
    persist: &RoleplayMechanicProposalPersist,
) -> bool {
    record.mechanic_session_id == persist.create.mechanic_session_id
        && record.roleplay_session_id == persist.create.roleplay_session_id
        && record.profile_id == persist.captured.profile_id
        && record.kind == persist.create.kind
        && record.target_id == persist.create.target_id
        && record.target_revision == persist.captured.target_revision
        && record.before_value == persist.captured.before_value
        && record.proposed_value == persist.create.proposed_value
        && record.rationale == persist.create.rationale
        && record.diagnostic_context == persist.create.diagnostic_context
}

fn get_proposal<C: GenericClient>(
    client: &mut C,
    schema: &str,
    proposal_id: &str,
) -> CoreResult<Option<RoleplayMechanicProposalRecord>> {
    client
        .query_opt(
            &format!(
                "SELECT record_json::text FROM {schema}.module_roleplay_mechanic_proposals WHERE proposal_id = $1"
            ),
            &[&proposal_id],
        )
        .map_err(|error| postgres_error("read PostgreSQL roleplay mechanic proposal", error))?
        .map(|row| {
            let json = row.get::<_, String>(0);
            parse_postgres_json(&json, "roleplay mechanic proposal")
        })
        .transpose()
}

fn write_proposal<C: GenericClient>(
    client: &mut C,
    schema: &str,
    record: &RoleplayMechanicProposalRecord,
    insert: bool,
) -> CoreResult<()> {
    let record_json = to_json_text(record)?;
    let target_revision = record.target_revision.map(|value| value as i64);
    let revision = record.revision as i64;
    if insert {
        client
            .execute(
                &format!(
                    "INSERT INTO {schema}.module_roleplay_mechanic_proposals (
                proposal_id, mechanic_session_id, roleplay_session_id, profile_id, kind, status,
                target_id, target_revision, revision, record_json, created_at, updated_at
             ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10::text::jsonb,$11,$12)"
                ),
                &[
                    &record.proposal_id,
                    &record.mechanic_session_id.0,
                    &record.roleplay_session_id,
                    &record.profile_id.0,
                    &roleplay_proposal_kind_as_str(record.kind),
                    &roleplay_proposal_status_as_str(record.status),
                    &record.target_id,
                    &target_revision,
                    &revision,
                    &record_json,
                    &record.created_at.as_str(),
                    &record.updated_at.as_str(),
                ],
            )
            .map_err(|error| {
                postgres_error("write PostgreSQL roleplay mechanic proposal", error)
            })?;
    } else {
        client
            .execute(
                &format!(
                    "UPDATE {schema}.module_roleplay_mechanic_proposals
                     SET status=$2,target_revision=$3,revision=$4,record_json=$5::text::jsonb,updated_at=$6
                     WHERE proposal_id=$1"
                ),
                &[
                    &record.proposal_id,
                    &roleplay_proposal_status_as_str(record.status),
                    &target_revision,
                    &revision,
                    &record_json,
                    &record.updated_at.as_str(),
                ],
            )
            .map_err(|error| postgres_error("write PostgreSQL roleplay mechanic proposal", error))?;
    }
    Ok(())
}
