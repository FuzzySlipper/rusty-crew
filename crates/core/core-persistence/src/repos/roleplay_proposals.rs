use super::super::*;

impl CoordinationStore {
    pub fn create_roleplay_mechanic_proposal(
        &self,
        persist: &RoleplayMechanicProposalPersist,
    ) -> CoreResult<RoleplayMechanicProposalRecord> {
        validate_roleplay_mechanic_proposal_persist(persist)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start roleplay mechanic proposal create", error))?;
        if let Some(existing) =
            get_roleplay_mechanic_proposal_in_tx(&tx, &persist.create.proposal_id)?
        {
            if proposal_matches_persist(&existing, persist) {
                tx.commit().map_err(|error| {
                    persistence_error("commit idempotent roleplay mechanic proposal create", error)
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
        let event = RoleplayMechanicProposalEvent {
            event_id: format!("{}:proposed:1", persist.create.proposal_id),
            kind: RoleplayMechanicProposalEventKind::Proposed,
            actor_id: persist.create.mechanic_session_id.0.clone(),
            note: Some(persist.create.rationale.clone()),
            target_revision: persist.captured.target_revision,
            details: persist.create.diagnostic_context.clone(),
            created_at: persist.create.now.clone(),
        };
        let record = RoleplayMechanicProposalRecord {
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
            history: vec![event],
            created_at: persist.create.now.clone(),
            updated_at: persist.create.now.clone(),
        };
        insert_roleplay_mechanic_proposal_in_tx(&tx, &record)?;
        tx.commit().map_err(|error| {
            persistence_error("commit roleplay mechanic proposal create", error)
        })?;
        Ok(record)
    }

    pub fn get_roleplay_mechanic_proposal(
        &self,
        proposal_id: &str,
    ) -> CoreResult<Option<RoleplayMechanicProposalRecord>> {
        validate_proposal_id(proposal_id)?;
        let conn = self.conn()?;
        get_roleplay_mechanic_proposal_in_tx(&conn, proposal_id)
    }

    pub fn list_roleplay_mechanic_proposals(
        &self,
        query: &RoleplayMechanicProposalQuery,
    ) -> CoreResult<Vec<RoleplayMechanicProposalRecord>> {
        validate_roleplay_mechanic_proposal_query(query)?;
        let (limit, offset) = query
            .page
            .unwrap_or(QueryPage {
                limit: None,
                offset: None,
            })
            .bounded(100, 1_000);
        let conn = self.conn()?;
        let status = query.status.map(roleplay_proposal_status_as_str);
        let kind = query.kind.map(roleplay_proposal_kind_as_str);
        let mut stmt = conn
            .prepare(
                "SELECT record_json FROM module_roleplay_mechanic_proposals
                 WHERE (?1 IS NULL OR mechanic_session_id = ?1)
                   AND (?2 IS NULL OR roleplay_session_id = ?2)
                   AND (?3 IS NULL OR profile_id = ?3)
                   AND (?4 IS NULL OR status = ?4)
                   AND (?5 IS NULL OR kind = ?5)
                 ORDER BY updated_at DESC, proposal_id ASC
                 LIMIT ?6 OFFSET ?7",
            )
            .map_err(|error| {
                persistence_error("prepare roleplay mechanic proposal query", error)
            })?;
        let rows = stmt
            .query_map(
                params![
                    query
                        .mechanic_session_id
                        .as_ref()
                        .map(|value| value.0.as_str()),
                    query.roleplay_session_id.as_deref(),
                    query.profile_id.as_ref().map(|value| value.0.as_str()),
                    status,
                    kind,
                    limit,
                    offset,
                ],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| persistence_error("query roleplay mechanic proposals", error))?;
        rows.map(|row| {
            row.map_err(|error| persistence_error("read roleplay mechanic proposal row", error))
                .and_then(|json| parse_json_record(&json))
        })
        .collect()
    }

    pub fn decide_roleplay_mechanic_proposal(
        &self,
        decision: &RoleplayMechanicProposalDecision,
    ) -> CoreResult<RoleplayMechanicProposalRecord> {
        validate_proposal_id(&decision.proposal_id)?;
        validate_actor_id("reviewer_id", &decision.reviewer_id)?;
        let mut conn = self.conn()?;
        let tx = conn.transaction().map_err(|error| {
            persistence_error("start roleplay mechanic proposal decision", error)
        })?;
        let mut record = get_roleplay_mechanic_proposal_in_tx(&tx, &decision.proposal_id)?
            .ok_or_else(|| {
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
                persistence_error(
                    "commit idempotent roleplay mechanic proposal decision",
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
        update_roleplay_mechanic_proposal_in_tx(&tx, &record)?;
        tx.commit().map_err(|error| {
            persistence_error("commit roleplay mechanic proposal decision", error)
        })?;
        Ok(record)
    }

    pub fn record_roleplay_mechanic_proposal_apply(
        &self,
        outcome: &RoleplayMechanicProposalApplyOutcome,
    ) -> CoreResult<RoleplayMechanicProposalRecord> {
        validate_proposal_id(&outcome.proposal_id)?;
        validate_actor_id("actor_id", &outcome.actor_id)?;
        let mut conn = self.conn()?;
        let tx = conn.transaction().map_err(|error| {
            persistence_error("start roleplay mechanic proposal apply outcome", error)
        })?;
        let mut record = get_roleplay_mechanic_proposal_in_tx(&tx, &outcome.proposal_id)?
            .ok_or_else(|| {
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
                persistence_error("commit idempotent roleplay mechanic proposal apply", error)
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
        update_roleplay_mechanic_proposal_in_tx(&tx, &record)?;
        tx.commit().map_err(|error| {
            persistence_error("commit roleplay mechanic proposal apply outcome", error)
        })?;
        Ok(record)
    }
}

pub(crate) fn validate_roleplay_mechanic_proposal_persist(
    persist: &RoleplayMechanicProposalPersist,
) -> CoreResult<()> {
    validate_proposal_id(&persist.create.proposal_id)?;
    validate_actor_id("mechanic_session_id", &persist.create.mechanic_session_id.0)?;
    validate_actor_id("roleplay_session_id", &persist.create.roleplay_session_id)?;
    validate_actor_id("profile_id", &persist.captured.profile_id.0)?;
    if persist.create.rationale.trim().is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "roleplay mechanic proposal rationale must not be empty",
        ));
    }
    if persist.create.rationale.chars().count() > 16_000 {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "roleplay mechanic proposal rationale exceeds 16000 characters",
        ));
    }
    if let Some(target_id) = &persist.create.target_id {
        validate_actor_id("target_id", target_id)?;
    }
    if persist.create.proposed_value.is_null() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "roleplay mechanic proposal proposed_value must not be null",
        ));
    }
    Ok(())
}

pub(crate) fn validate_roleplay_mechanic_proposal_query(
    query: &RoleplayMechanicProposalQuery,
) -> CoreResult<()> {
    if let Some(value) = &query.mechanic_session_id {
        validate_actor_id("mechanic_session_id", &value.0)?;
    }
    if let Some(value) = &query.roleplay_session_id {
        validate_actor_id("roleplay_session_id", value)?;
    }
    if let Some(value) = &query.profile_id {
        validate_actor_id("profile_id", &value.0)?;
    }
    Ok(())
}

pub(crate) fn roleplay_proposal_kind_as_str(kind: RoleplayMechanicProposalKind) -> &'static str {
    match kind {
        RoleplayMechanicProposalKind::NarratorConfig => "narrator_config",
        RoleplayMechanicProposalKind::Exemplar => "exemplar",
        RoleplayMechanicProposalKind::LoreAdd => "lore_add",
        RoleplayMechanicProposalKind::LoreEdit => "lore_edit",
        RoleplayMechanicProposalKind::LoreTags => "lore_tags",
        RoleplayMechanicProposalKind::LayerRetrievalConfig => "layer_retrieval_config",
        RoleplayMechanicProposalKind::ProviderFailurePattern => "provider_failure_pattern",
    }
}

pub(crate) fn roleplay_proposal_status_as_str(
    status: RoleplayMechanicProposalStatus,
) -> &'static str {
    match status {
        RoleplayMechanicProposalStatus::Proposed => "proposed",
        RoleplayMechanicProposalStatus::Approved => "approved",
        RoleplayMechanicProposalStatus::Rejected => "rejected",
        RoleplayMechanicProposalStatus::Applied => "applied",
    }
}

pub(crate) fn validate_proposal_id(value: &str) -> CoreResult<()> {
    validate_actor_id("proposal_id", value)
}

pub(crate) fn validate_actor_id(field: &str, value: &str) -> CoreResult<()> {
    let valid = !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'));
    if valid {
        Ok(())
    } else {
        Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("invalid roleplay mechanic proposal {field}"),
        ))
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

fn insert_roleplay_mechanic_proposal_in_tx(
    tx: &rusqlite::Transaction<'_>,
    record: &RoleplayMechanicProposalRecord,
) -> CoreResult<()> {
    tx.execute(
        "INSERT INTO module_roleplay_mechanic_proposals (
            proposal_id, mechanic_session_id, roleplay_session_id, profile_id, kind, status,
            target_id, target_revision, revision, record_json, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            record.proposal_id,
            record.mechanic_session_id.0,
            record.roleplay_session_id,
            record.profile_id.0,
            roleplay_proposal_kind_as_str(record.kind),
            roleplay_proposal_status_as_str(record.status),
            record.target_id,
            record.target_revision.map(|value| value as i64),
            record.revision as i64,
            to_json_text(record)?,
            record.created_at.as_str(),
            record.updated_at.as_str(),
        ],
    )
    .map_err(|error| persistence_error("insert roleplay mechanic proposal", error))?;
    Ok(())
}

fn update_roleplay_mechanic_proposal_in_tx(
    tx: &rusqlite::Transaction<'_>,
    record: &RoleplayMechanicProposalRecord,
) -> CoreResult<()> {
    tx.execute(
        "UPDATE module_roleplay_mechanic_proposals
         SET status = ?2, target_revision = ?3, revision = ?4, record_json = ?5, updated_at = ?6
         WHERE proposal_id = ?1",
        params![
            record.proposal_id,
            roleplay_proposal_status_as_str(record.status),
            record.target_revision.map(|value| value as i64),
            record.revision as i64,
            to_json_text(record)?,
            record.updated_at.as_str(),
        ],
    )
    .map_err(|error| persistence_error("update roleplay mechanic proposal", error))?;
    Ok(())
}

fn get_roleplay_mechanic_proposal_in_tx(
    conn: &Connection,
    proposal_id: &str,
) -> CoreResult<Option<RoleplayMechanicProposalRecord>> {
    conn.query_row(
        "SELECT record_json FROM module_roleplay_mechanic_proposals WHERE proposal_id = ?1",
        params![proposal_id],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(|error| persistence_error("read roleplay mechanic proposal", error))?
    .map(|json| parse_json_record(&json))
    .transpose()
}
