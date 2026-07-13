use super::super::*;
use crate::repos::roleplay_proposals::{validate_actor_id, validate_proposal_id};

impl CoordinationStore {
    pub fn put_roleplay_mechanic_session_association(
        &self,
        write: &RoleplayMechanicSessionAssociationWrite,
    ) -> CoreResult<RoleplayMechanicSessionAssociationRecord> {
        validate_association_record(&write.record)?;
        let mut conn = self.conn()?;
        let tx = conn.transaction().map_err(|error| {
            persistence_error("start roleplay mechanic session association write", error)
        })?;
        let existing = get_association_in_tx(&tx, &write.record.mechanic_session_id)?;
        match (existing.as_ref(), write.expected_revision) {
            (None, None) if write.record.revision == 1 => {}
            (None, _) => {
                return Err(CoreError::new(
                    CoreErrorKind::NotFound,
                    format!(
                        "roleplay mechanic session association {} not found",
                        write.record.mechanic_session_id
                    ),
                ));
            }
            (Some(current), None) if association_identity_matches(current, &write.record) => {
                tx.commit().map_err(|error| {
                    persistence_error(
                        "commit idempotent roleplay mechanic session association write",
                        error,
                    )
                })?;
                return Ok(current.clone());
            }
            (Some(_), None) => {
                return Err(CoreError::new(
                    CoreErrorKind::AlreadyExists,
                    format!(
                        "roleplay mechanic session association {} already exists",
                        write.record.mechanic_session_id
                    ),
                ));
            }
            (Some(current), Some(expected)) if current.revision != expected => {
                return Err(CoreError::new(
                    CoreErrorKind::ActionRejected,
                    format!(
                        "roleplay mechanic session association {} revision mismatch: expected {}, found {}",
                        write.record.mechanic_session_id, expected, current.revision
                    ),
                ));
            }
            (Some(current), Some(_)) if write.record.revision != current.revision + 1 => {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "roleplay mechanic session association revision must advance by one",
                ));
            }
            (Some(_), Some(_)) => {}
        }

        tx.execute(
            "INSERT INTO module_roleplay_mechanic_sessions (
                mechanic_session_id, mechanic_profile_id, roleplay_session_id,
                roleplay_profile_id, revision, record_json, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(mechanic_session_id) DO UPDATE SET
                mechanic_profile_id = excluded.mechanic_profile_id,
                roleplay_session_id = excluded.roleplay_session_id,
                roleplay_profile_id = excluded.roleplay_profile_id,
                revision = excluded.revision,
                record_json = excluded.record_json,
                updated_at = excluded.updated_at",
            params![
                write.record.mechanic_session_id.0,
                write.record.mechanic_profile_id.0,
                write.record.roleplay_session_id,
                write
                    .record
                    .roleplay_profile_id
                    .as_ref()
                    .map(|value| value.0.as_str()),
                write.record.revision as i64,
                to_json_text(&write.record)?,
                write.record.created_at.as_str(),
                write.record.updated_at.as_str(),
            ],
        )
        .map_err(|error| persistence_error("write roleplay mechanic session association", error))?;
        tx.commit().map_err(|error| {
            persistence_error("commit roleplay mechanic session association write", error)
        })?;
        Ok(write.record.clone())
    }

    pub fn get_roleplay_mechanic_session_association(
        &self,
        mechanic_session_id: &SessionId,
    ) -> CoreResult<Option<RoleplayMechanicSessionAssociationRecord>> {
        validate_actor_id("mechanic_session_id", &mechanic_session_id.0)?;
        let conn = self.conn()?;
        get_association_in_tx(&conn, mechanic_session_id)
    }

    pub fn list_roleplay_mechanic_session_associations(
        &self,
        query: &RoleplayMechanicSessionAssociationQuery,
    ) -> CoreResult<Vec<RoleplayMechanicSessionAssociationRecord>> {
        validate_association_query(query)?;
        let (limit, offset) = query.page.unwrap_or_default().bounded(100, 1_000);
        let conn = self.conn()?;
        let attached = query.attached.map(i64::from);
        let mut stmt = conn
            .prepare(
                "SELECT record_json FROM module_roleplay_mechanic_sessions
                 WHERE (?1 IS NULL OR mechanic_profile_id = ?1)
                   AND (?2 IS NULL OR roleplay_session_id = ?2)
                   AND (?3 IS NULL OR roleplay_profile_id = ?3)
                   AND (?4 IS NULL OR (roleplay_session_id IS NOT NULL) = ?4)
                 ORDER BY updated_at DESC, mechanic_session_id ASC
                 LIMIT ?5 OFFSET ?6",
            )
            .map_err(|error| {
                persistence_error("prepare roleplay mechanic session association query", error)
            })?;
        let rows = stmt
            .query_map(
                params![
                    query
                        .mechanic_profile_id
                        .as_ref()
                        .map(|value| value.0.as_str()),
                    query.roleplay_session_id.as_deref(),
                    query
                        .roleplay_profile_id
                        .as_ref()
                        .map(|value| value.0.as_str()),
                    attached,
                    limit,
                    offset,
                ],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| {
                persistence_error("query roleplay mechanic session associations", error)
            })?;
        rows.map(|row| {
            row.map_err(|error| {
                persistence_error("read roleplay mechanic session association row", error)
            })
            .and_then(|json| parse_json_record(&json))
        })
        .collect()
    }

    pub fn create_roleplay_mechanic_diagnostic(
        &self,
        record: &RoleplayMechanicDiagnosticRecord,
    ) -> CoreResult<RoleplayMechanicDiagnosticRecord> {
        validate_diagnostic_record(record)?;
        if record.revision != 1 {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "new roleplay mechanic diagnostic revision must be one",
            ));
        }
        let mut conn = self.conn()?;
        let tx = conn.transaction().map_err(|error| {
            persistence_error("start roleplay mechanic diagnostic create", error)
        })?;
        if let Some(existing) = get_diagnostic_in_tx(&tx, &record.diagnostic_id)? {
            if existing == *record {
                tx.commit().map_err(|error| {
                    persistence_error("commit idempotent mechanic diagnostic create", error)
                })?;
                return Ok(existing);
            }
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!(
                    "roleplay mechanic diagnostic {} already exists",
                    record.diagnostic_id
                ),
            ));
        }
        tx.execute(
            "INSERT INTO module_roleplay_mechanic_diagnostics (
                diagnostic_id, mechanic_session_id, mechanic_profile_id, roleplay_session_id,
                roleplay_profile_id, outcome, revision, record_json, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                record.diagnostic_id,
                record.mechanic_session_id.0,
                record.mechanic_profile_id.0,
                record.roleplay_session_id,
                record.roleplay_profile_id.0,
                diagnostic_outcome_as_str(record.outcome),
                record.revision as i64,
                to_json_text(record)?,
                record.created_at.as_str(),
                record.updated_at.as_str(),
            ],
        )
        .map_err(|error| persistence_error("insert roleplay mechanic diagnostic", error))?;
        replace_diagnostic_proposal_links(&tx, record)?;
        tx.commit().map_err(|error| {
            persistence_error("commit roleplay mechanic diagnostic create", error)
        })?;
        Ok(record.clone())
    }

    pub fn get_roleplay_mechanic_diagnostic(
        &self,
        diagnostic_id: &str,
    ) -> CoreResult<Option<RoleplayMechanicDiagnosticRecord>> {
        validate_actor_id("diagnostic_id", diagnostic_id)?;
        let conn = self.conn()?;
        get_diagnostic_in_tx(&conn, diagnostic_id)
    }

    pub fn list_roleplay_mechanic_diagnostics(
        &self,
        query: &RoleplayMechanicDiagnosticQuery,
    ) -> CoreResult<Vec<RoleplayMechanicDiagnosticRecord>> {
        validate_diagnostic_query(query)?;
        let (limit, offset) = query.page.unwrap_or_default().bounded(100, 1_000);
        let conn = self.conn()?;
        let outcome = query.outcome.map(diagnostic_outcome_as_str);
        let mut stmt = conn
            .prepare(
                "SELECT record_json FROM module_roleplay_mechanic_diagnostics AS diagnostic
                 WHERE (?1 IS NULL OR mechanic_session_id = ?1)
                   AND (?2 IS NULL OR roleplay_session_id = ?2)
                   AND (?3 IS NULL OR roleplay_profile_id = ?3)
                   AND (?4 IS NULL OR outcome = ?4)
                   AND (?5 IS NULL OR EXISTS (
                       SELECT 1 FROM module_roleplay_mechanic_diagnostic_proposals AS link
                       WHERE link.diagnostic_id = diagnostic.diagnostic_id AND link.proposal_id = ?5
                   ))
                 ORDER BY updated_at DESC, diagnostic_id ASC
                 LIMIT ?6 OFFSET ?7",
            )
            .map_err(|error| {
                persistence_error("prepare roleplay mechanic diagnostic query", error)
            })?;
        let rows = stmt
            .query_map(
                params![
                    query
                        .mechanic_session_id
                        .as_ref()
                        .map(|value| value.0.as_str()),
                    query.roleplay_session_id.as_deref(),
                    query
                        .roleplay_profile_id
                        .as_ref()
                        .map(|value| value.0.as_str()),
                    outcome,
                    query.proposal_id.as_deref(),
                    limit,
                    offset,
                ],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| persistence_error("query roleplay mechanic diagnostics", error))?;
        rows.map(|row| {
            row.map_err(|error| persistence_error("read roleplay mechanic diagnostic row", error))
                .and_then(|json| parse_json_record(&json))
        })
        .collect()
    }

    pub fn update_roleplay_mechanic_diagnostic_outcome(
        &self,
        update: &RoleplayMechanicDiagnosticOutcomeUpdate,
    ) -> CoreResult<RoleplayMechanicDiagnosticRecord> {
        validate_actor_id("diagnostic_id", &update.diagnostic_id)?;
        validate_optional_text("notes", update.notes.as_deref(), 32_000)?;
        let mut conn = self.conn()?;
        let tx = conn.transaction().map_err(|error| {
            persistence_error("start roleplay mechanic diagnostic outcome update", error)
        })?;
        let mut record = get_diagnostic_in_tx(&tx, &update.diagnostic_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!(
                    "roleplay mechanic diagnostic {} not found",
                    update.diagnostic_id
                ),
            )
        })?;
        if record.outcome == update.outcome && record.notes == update.notes {
            tx.commit().map_err(|error| {
                persistence_error("commit idempotent mechanic diagnostic update", error)
            })?;
            return Ok(record);
        }
        if record.revision != update.expected_revision {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "roleplay mechanic diagnostic {} revision mismatch: expected {}, found {}",
                    update.diagnostic_id, update.expected_revision, record.revision
                ),
            ));
        }
        record.outcome = update.outcome;
        record.notes = update.notes.clone();
        record.revision += 1;
        record.updated_at = update.now.clone();
        tx.execute(
            "UPDATE module_roleplay_mechanic_diagnostics
             SET outcome = ?2, revision = ?3, record_json = ?4, updated_at = ?5
             WHERE diagnostic_id = ?1",
            params![
                record.diagnostic_id,
                diagnostic_outcome_as_str(record.outcome),
                record.revision as i64,
                to_json_text(&record)?,
                record.updated_at.as_str(),
            ],
        )
        .map_err(|error| persistence_error("update roleplay mechanic diagnostic", error))?;
        tx.commit().map_err(|error| {
            persistence_error("commit roleplay mechanic diagnostic outcome update", error)
        })?;
        Ok(record)
    }
}

pub(crate) fn diagnostic_outcome_as_str(
    outcome: RoleplayMechanicDiagnosticOutcome,
) -> &'static str {
    match outcome {
        RoleplayMechanicDiagnosticOutcome::Pending => "pending",
        RoleplayMechanicDiagnosticOutcome::Improved => "improved",
        RoleplayMechanicDiagnosticOutcome::NoChange => "no_change",
        RoleplayMechanicDiagnosticOutcome::Worse => "worse",
    }
}

pub(crate) fn validate_association_record(
    record: &RoleplayMechanicSessionAssociationRecord,
) -> CoreResult<()> {
    validate_actor_id("mechanic_session_id", &record.mechanic_session_id.0)?;
    validate_actor_id("mechanic_profile_id", &record.mechanic_profile_id.0)?;
    match (&record.roleplay_session_id, &record.roleplay_profile_id) {
        (Some(session_id), Some(profile_id)) => {
            validate_actor_id("roleplay_session_id", session_id)?;
            validate_actor_id("roleplay_profile_id", &profile_id.0)?;
        }
        (None, None) => {}
        _ => {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "roleplay mechanic association target session and profile must both be present or absent",
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_association_query(
    query: &RoleplayMechanicSessionAssociationQuery,
) -> CoreResult<()> {
    if let Some(value) = &query.mechanic_profile_id {
        validate_actor_id("mechanic_profile_id", &value.0)?;
    }
    if let Some(value) = &query.roleplay_session_id {
        validate_actor_id("roleplay_session_id", value)?;
    }
    if let Some(value) = &query.roleplay_profile_id {
        validate_actor_id("roleplay_profile_id", &value.0)?;
    }
    Ok(())
}

pub(crate) fn validate_diagnostic_record(
    record: &RoleplayMechanicDiagnosticRecord,
) -> CoreResult<()> {
    validate_actor_id("diagnostic_id", &record.diagnostic_id)?;
    validate_actor_id("mechanic_session_id", &record.mechanic_session_id.0)?;
    validate_actor_id("mechanic_profile_id", &record.mechanic_profile_id.0)?;
    validate_actor_id("roleplay_session_id", &record.roleplay_session_id)?;
    validate_actor_id("roleplay_profile_id", &record.roleplay_profile_id.0)?;
    validate_required_text("symptom", &record.symptom, 16_000)?;
    validate_required_text("hypothesis", &record.hypothesis, 16_000)?;
    validate_optional_text("notes", record.notes.as_deref(), 32_000)?;
    validate_proposal_ids(&record.proposal_ids)?;
    validate_proposal_ids(&record.applied_proposal_ids)?;
    if record
        .applied_proposal_ids
        .iter()
        .any(|id| !record.proposal_ids.contains(id))
    {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "applied proposal IDs must also be present in proposal IDs",
        ));
    }
    Ok(())
}

pub(crate) fn validate_diagnostic_query(query: &RoleplayMechanicDiagnosticQuery) -> CoreResult<()> {
    if let Some(value) = &query.mechanic_session_id {
        validate_actor_id("mechanic_session_id", &value.0)?;
    }
    if let Some(value) = &query.roleplay_session_id {
        validate_actor_id("roleplay_session_id", value)?;
    }
    if let Some(value) = &query.roleplay_profile_id {
        validate_actor_id("roleplay_profile_id", &value.0)?;
    }
    if let Some(value) = &query.proposal_id {
        validate_proposal_id(value)?;
    }
    Ok(())
}

fn validate_required_text(field: &str, value: &str, max: usize) -> CoreResult<()> {
    if value.trim().is_empty() || value.chars().count() > max {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("roleplay mechanic diagnostic {field} must contain 1 to {max} characters"),
        ));
    }
    Ok(())
}

fn validate_optional_text(field: &str, value: Option<&str>, max: usize) -> CoreResult<()> {
    if value.is_some_and(|value| value.chars().count() > max) {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("roleplay mechanic diagnostic {field} exceeds {max} characters"),
        ));
    }
    Ok(())
}

fn validate_proposal_ids(values: &[String]) -> CoreResult<()> {
    if values.len() > 128 {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "roleplay mechanic diagnostic proposal links exceed 128 entries",
        ));
    }
    let mut unique = BTreeSet::new();
    for value in values {
        validate_proposal_id(value)?;
        if !unique.insert(value) {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("duplicate roleplay mechanic diagnostic proposal ID {value}"),
            ));
        }
    }
    Ok(())
}

fn association_identity_matches(
    left: &RoleplayMechanicSessionAssociationRecord,
    right: &RoleplayMechanicSessionAssociationRecord,
) -> bool {
    left.mechanic_profile_id == right.mechanic_profile_id
        && left.roleplay_session_id == right.roleplay_session_id
        && left.roleplay_profile_id == right.roleplay_profile_id
}

fn get_association_in_tx(
    conn: &Connection,
    mechanic_session_id: &SessionId,
) -> CoreResult<Option<RoleplayMechanicSessionAssociationRecord>> {
    conn.query_row(
        "SELECT record_json FROM module_roleplay_mechanic_sessions WHERE mechanic_session_id = ?1",
        params![mechanic_session_id.0],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(|error| persistence_error("read roleplay mechanic session association", error))?
    .map(|json| parse_json_record(&json))
    .transpose()
}

fn get_diagnostic_in_tx(
    conn: &Connection,
    diagnostic_id: &str,
) -> CoreResult<Option<RoleplayMechanicDiagnosticRecord>> {
    conn.query_row(
        "SELECT record_json FROM module_roleplay_mechanic_diagnostics WHERE diagnostic_id = ?1",
        params![diagnostic_id],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(|error| persistence_error("read roleplay mechanic diagnostic", error))?
    .map(|json| parse_json_record(&json))
    .transpose()
}

fn replace_diagnostic_proposal_links(
    tx: &rusqlite::Transaction<'_>,
    record: &RoleplayMechanicDiagnosticRecord,
) -> CoreResult<()> {
    for proposal_id in &record.proposal_ids {
        tx.execute(
            "INSERT INTO module_roleplay_mechanic_diagnostic_proposals (
                diagnostic_id, proposal_id, applied
             ) VALUES (?1, ?2, ?3)",
            params![
                record.diagnostic_id,
                proposal_id,
                i64::from(record.applied_proposal_ids.contains(proposal_id)),
            ],
        )
        .map_err(|error| {
            persistence_error("insert roleplay mechanic diagnostic proposal link", error)
        })?;
    }
    Ok(())
}
