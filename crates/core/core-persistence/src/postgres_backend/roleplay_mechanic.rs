use super::*;
use crate::repos::roleplay_mechanic::{
    diagnostic_outcome_as_str, validate_association_query, validate_association_record,
    validate_diagnostic_query, validate_diagnostic_record,
};
use crate::repos::roleplay_proposals::validate_actor_id;
use crate::{
    RoleplayMechanicDiagnosticOutcomeUpdate, RoleplayMechanicDiagnosticQuery,
    RoleplayMechanicDiagnosticRecord, RoleplayMechanicSessionAssociationQuery,
    RoleplayMechanicSessionAssociationRecord, RoleplayMechanicSessionAssociationWrite,
};

impl PostgresBackendStore {
    pub fn put_roleplay_mechanic_session_association(
        &self,
        write: &RoleplayMechanicSessionAssociationWrite,
    ) -> CoreResult<RoleplayMechanicSessionAssociationRecord> {
        validate_association_record(&write.record)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error(
                "start PostgreSQL roleplay mechanic session association write",
                error,
            )
        })?;
        let existing = get_association(&mut tx, &schema, &write.record.mechanic_session_id)?;
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
                    postgres_error(
                        "commit idempotent PostgreSQL roleplay mechanic session association",
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
        let json = to_json_text(&write.record)?;
        let revision = write.record.revision as i64;
        let roleplay_profile_id = write
            .record
            .roleplay_profile_id
            .as_ref()
            .map(|value| value.0.as_str());
        tx.execute(
            &format!(
                "INSERT INTO {schema}.module_roleplay_mechanic_sessions (
                    mechanic_session_id, mechanic_profile_id, roleplay_session_id,
                    roleplay_profile_id, revision, record_json, created_at, updated_at
                 ) VALUES ($1,$2,$3,$4,$5,$6::text::jsonb,$7,$8)
                 ON CONFLICT(mechanic_session_id) DO UPDATE SET
                    mechanic_profile_id=EXCLUDED.mechanic_profile_id,
                    roleplay_session_id=EXCLUDED.roleplay_session_id,
                    roleplay_profile_id=EXCLUDED.roleplay_profile_id,
                    revision=EXCLUDED.revision,
                    record_json=EXCLUDED.record_json,
                    updated_at=EXCLUDED.updated_at"
            ),
            &[
                &write.record.mechanic_session_id.0,
                &write.record.mechanic_profile_id.0,
                &write.record.roleplay_session_id,
                &roleplay_profile_id,
                &revision,
                &json,
                &write.record.created_at.as_str(),
                &write.record.updated_at.as_str(),
            ],
        )
        .map_err(|error| {
            postgres_error(
                "write PostgreSQL roleplay mechanic session association",
                error,
            )
        })?;
        tx.commit().map_err(|error| {
            postgres_error(
                "commit PostgreSQL roleplay mechanic session association write",
                error,
            )
        })?;
        Ok(write.record.clone())
    }

    pub fn get_roleplay_mechanic_session_association(
        &self,
        mechanic_session_id: &SessionId,
    ) -> CoreResult<Option<RoleplayMechanicSessionAssociationRecord>> {
        validate_actor_id("mechanic_session_id", &mechanic_session_id.0)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        get_association(&mut *client, &schema, mechanic_session_id)
    }

    pub fn list_roleplay_mechanic_session_associations(
        &self,
        query: &RoleplayMechanicSessionAssociationQuery,
    ) -> CoreResult<Vec<RoleplayMechanicSessionAssociationRecord>> {
        validate_association_query(query)?;
        let schema = self.quoted_schema();
        let (limit, offset) = query.page.unwrap_or_default().bounded(100, 1_000);
        let mechanic_profile_id = query
            .mechanic_profile_id
            .as_ref()
            .map(|value| value.0.as_str());
        let roleplay_profile_id = query
            .roleplay_profile_id
            .as_ref()
            .map(|value| value.0.as_str());
        let mut client = self.client()?;
        client
            .query(
                &format!(
                    "SELECT record_json::text FROM {schema}.module_roleplay_mechanic_sessions
                     WHERE ($1::TEXT IS NULL OR mechanic_profile_id = $1)
                       AND ($2::TEXT IS NULL OR roleplay_session_id = $2)
                       AND ($3::TEXT IS NULL OR roleplay_profile_id = $3)
                       AND ($4::BOOLEAN IS NULL OR (roleplay_session_id IS NOT NULL) = $4)
                     ORDER BY updated_at DESC, mechanic_session_id ASC LIMIT $5 OFFSET $6"
                ),
                &[
                    &mechanic_profile_id,
                    &query.roleplay_session_id,
                    &roleplay_profile_id,
                    &query.attached,
                    &limit,
                    &offset,
                ],
            )
            .map_err(|error| {
                postgres_error(
                    "query PostgreSQL roleplay mechanic session associations",
                    error,
                )
            })?
            .into_iter()
            .map(|row| {
                parse_postgres_json(
                    row.get::<_, String>(0).as_str(),
                    "roleplay mechanic session association",
                )
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
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error(
                "start PostgreSQL roleplay mechanic diagnostic create",
                error,
            )
        })?;
        if let Some(existing) = get_diagnostic(&mut tx, &schema, &record.diagnostic_id)? {
            if existing == *record {
                tx.commit().map_err(|error| {
                    postgres_error(
                        "commit idempotent PostgreSQL roleplay mechanic diagnostic create",
                        error,
                    )
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
        let json = to_json_text(record)?;
        let revision = record.revision as i64;
        tx.execute(
            &format!(
                "INSERT INTO {schema}.module_roleplay_mechanic_diagnostics (
                    diagnostic_id, mechanic_session_id, mechanic_profile_id, roleplay_session_id,
                    roleplay_profile_id, outcome, revision, record_json, created_at, updated_at
                 ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8::text::jsonb,$9,$10)"
            ),
            &[
                &record.diagnostic_id,
                &record.mechanic_session_id.0,
                &record.mechanic_profile_id.0,
                &record.roleplay_session_id,
                &record.roleplay_profile_id.0,
                &diagnostic_outcome_as_str(record.outcome),
                &revision,
                &json,
                &record.created_at.as_str(),
                &record.updated_at.as_str(),
            ],
        )
        .map_err(|error| postgres_error("insert PostgreSQL roleplay mechanic diagnostic", error))?;
        for proposal_id in &record.proposal_ids {
            let applied = record.applied_proposal_ids.contains(proposal_id);
            tx.execute(
                &format!(
                    "INSERT INTO {schema}.module_roleplay_mechanic_diagnostic_proposals (
                        diagnostic_id, proposal_id, applied
                     ) VALUES ($1,$2,$3)"
                ),
                &[&record.diagnostic_id, proposal_id, &applied],
            )
            .map_err(|error| {
                postgres_error(
                    "insert PostgreSQL roleplay mechanic diagnostic proposal link",
                    error,
                )
            })?;
        }
        tx.commit().map_err(|error| {
            postgres_error(
                "commit PostgreSQL roleplay mechanic diagnostic create",
                error,
            )
        })?;
        Ok(record.clone())
    }

    pub fn get_roleplay_mechanic_diagnostic(
        &self,
        diagnostic_id: &str,
    ) -> CoreResult<Option<RoleplayMechanicDiagnosticRecord>> {
        validate_actor_id("diagnostic_id", diagnostic_id)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        get_diagnostic(&mut *client, &schema, diagnostic_id)
    }

    pub fn list_roleplay_mechanic_diagnostics(
        &self,
        query: &RoleplayMechanicDiagnosticQuery,
    ) -> CoreResult<Vec<RoleplayMechanicDiagnosticRecord>> {
        validate_diagnostic_query(query)?;
        let schema = self.quoted_schema();
        let (limit, offset) = query.page.unwrap_or_default().bounded(100, 1_000);
        let mechanic_session_id = query
            .mechanic_session_id
            .as_ref()
            .map(|value| value.0.as_str());
        let roleplay_profile_id = query
            .roleplay_profile_id
            .as_ref()
            .map(|value| value.0.as_str());
        let outcome = query.outcome.map(diagnostic_outcome_as_str);
        let mut client = self.client()?;
        client
            .query(
                &format!(
                    "SELECT record_json::text FROM {schema}.module_roleplay_mechanic_diagnostics AS diagnostic
                     WHERE ($1::TEXT IS NULL OR mechanic_session_id = $1)
                       AND ($2::TEXT IS NULL OR roleplay_session_id = $2)
                       AND ($3::TEXT IS NULL OR roleplay_profile_id = $3)
                       AND ($4::TEXT IS NULL OR outcome = $4)
                       AND ($5::TEXT IS NULL OR EXISTS (
                           SELECT 1 FROM {schema}.module_roleplay_mechanic_diagnostic_proposals AS link
                           WHERE link.diagnostic_id = diagnostic.diagnostic_id AND link.proposal_id = $5
                       ))
                     ORDER BY updated_at DESC, diagnostic_id ASC LIMIT $6 OFFSET $7"
                ),
                &[
                    &mechanic_session_id,
                    &query.roleplay_session_id,
                    &roleplay_profile_id,
                    &outcome,
                    &query.proposal_id,
                    &limit,
                    &offset,
                ],
            )
            .map_err(|error| {
                postgres_error("query PostgreSQL roleplay mechanic diagnostics", error)
            })?
            .into_iter()
            .map(|row| {
                parse_postgres_json(
                    row.get::<_, String>(0).as_str(),
                    "roleplay mechanic diagnostic",
                )
            })
            .collect()
    }

    pub fn update_roleplay_mechanic_diagnostic_outcome(
        &self,
        update: &RoleplayMechanicDiagnosticOutcomeUpdate,
    ) -> CoreResult<RoleplayMechanicDiagnosticRecord> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error(
                "start PostgreSQL roleplay mechanic diagnostic outcome update",
                error,
            )
        })?;
        let mut record =
            get_diagnostic(&mut tx, &schema, &update.diagnostic_id)?.ok_or_else(|| {
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
                postgres_error(
                    "commit idempotent PostgreSQL roleplay mechanic diagnostic update",
                    error,
                )
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
        validate_diagnostic_record(&record)?;
        let json = to_json_text(&record)?;
        let revision = record.revision as i64;
        tx.execute(
            &format!(
                "UPDATE {schema}.module_roleplay_mechanic_diagnostics
                 SET outcome=$2, revision=$3, record_json=$4::text::jsonb, updated_at=$5
                 WHERE diagnostic_id=$1"
            ),
            &[
                &record.diagnostic_id,
                &diagnostic_outcome_as_str(record.outcome),
                &revision,
                &json,
                &record.updated_at.as_str(),
            ],
        )
        .map_err(|error| postgres_error("update PostgreSQL roleplay mechanic diagnostic", error))?;
        tx.commit().map_err(|error| {
            postgres_error(
                "commit PostgreSQL roleplay mechanic diagnostic outcome update",
                error,
            )
        })?;
        Ok(record)
    }
}

fn get_association<C: GenericClient>(
    client: &mut C,
    schema: &str,
    mechanic_session_id: &SessionId,
) -> CoreResult<Option<RoleplayMechanicSessionAssociationRecord>> {
    client
        .query_opt(
            &format!(
                "SELECT record_json::text FROM {schema}.module_roleplay_mechanic_sessions WHERE mechanic_session_id=$1"
            ),
            &[&mechanic_session_id.0],
        )
        .map_err(|error| {
            postgres_error("read PostgreSQL roleplay mechanic session association", error)
        })?
        .map(|row| {
            parse_postgres_json(
                row.get::<_, String>(0).as_str(),
                "roleplay mechanic session association",
            )
        })
        .transpose()
}

fn get_diagnostic<C: GenericClient>(
    client: &mut C,
    schema: &str,
    diagnostic_id: &str,
) -> CoreResult<Option<RoleplayMechanicDiagnosticRecord>> {
    client
        .query_opt(
            &format!(
                "SELECT record_json::text FROM {schema}.module_roleplay_mechanic_diagnostics WHERE diagnostic_id=$1"
            ),
            &[&diagnostic_id],
        )
        .map_err(|error| {
            postgres_error("read PostgreSQL roleplay mechanic diagnostic", error)
        })?
        .map(|row| {
            parse_postgres_json(
                row.get::<_, String>(0).as_str(),
                "roleplay mechanic diagnostic",
            )
        })
        .transpose()
}

fn association_identity_matches(
    left: &RoleplayMechanicSessionAssociationRecord,
    right: &RoleplayMechanicSessionAssociationRecord,
) -> bool {
    left.mechanic_profile_id == right.mechanic_profile_id
        && left.roleplay_session_id == right.roleplay_session_id
        && left.roleplay_profile_id == right.roleplay_profile_id
}
