//! SQLite provider wire-state repository domain.
//!
//! Provider wire state is owned by the brain/provider integration contract, but
//! persisted by Crew so provider-owned continuation state can survive restarts
//! and be invalidated deterministically when profile/provider/module strategy
//! fingerprints drift.

use super::*;

impl CoordinationStore {
    pub fn save_provider_wire_state(
        &self,
        write: &ProviderWireStateWrite,
    ) -> CoreResult<ProviderWireStateRecord> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start save provider wire state", error))?;
        let record = save_provider_wire_state_in_tx(&tx, write)?;
        tx.commit()
            .map_err(|error| persistence_error("commit save provider wire state", error))?;
        Ok(record)
    }
    pub fn load_provider_wire_state_for_wake(
        &self,
        lookup: &ProviderWireStateWakeLookup,
    ) -> CoreResult<ProviderWireStateWakeResult> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start load provider wire state", error))?;
        let result = load_provider_wire_state_for_wake_in_tx(&tx, lookup)?;
        tx.commit()
            .map_err(|error| persistence_error("commit load provider wire state", error))?;
        Ok(result)
    }
    pub fn clear_provider_wire_state(
        &self,
        key: &ProviderWireStateKey,
        now: &IsoTimestamp,
        reason: ProviderWireStateInvalidationReason,
    ) -> CoreResult<Option<ProviderWireStateRecord>> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start clear provider wire state", error))?;
        let cleared = clear_provider_wire_state_in_tx(&tx, key, now, reason)?;
        tx.commit()
            .map_err(|error| persistence_error("commit clear provider wire state", error))?;
        Ok(cleared)
    }
    pub fn expire_provider_wire_states_at(
        &self,
        now: &IsoTimestamp,
    ) -> CoreResult<Vec<ProviderWireStateRecord>> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start expire provider wire states", error))?;
        let expired = expire_provider_wire_states_in_tx(&tx, now)?;
        tx.commit()
            .map_err(|error| persistence_error("commit expire provider wire states", error))?;
        Ok(expired)
    }
    pub fn list_provider_wire_state_diagnostics(
        &self,
        limit: u32,
    ) -> CoreResult<Vec<ProviderWireStateDiagnostic>> {
        let conn = self.conn()?;
        list_provider_wire_state_diagnostics(&conn, limit)
    }
}

fn save_provider_wire_state_in_tx(
    tx: &rusqlite::Transaction<'_>,
    write: &ProviderWireStateWrite,
) -> CoreResult<ProviderWireStateRecord> {
    validate_provider_wire_state_key(&write.key)?;
    let payload_json = to_json_text(&write.payload_json)?;
    invalidate_current_provider_wire_state_for_key_in_tx(
        tx,
        &write.key,
        &write.now,
        ProviderWireStateInvalidationReason::Superseded,
    )?;
    tx.execute(
        "INSERT INTO provider_wire_states (
            session_id,
            module_id,
            strategy_id,
            profile_fingerprint,
            provider_fingerprint,
            payload_version,
            payload_json,
            payload_encoding,
            created_at,
            updated_at,
            expires_at,
            last_wake_id,
            invalidated_at,
            invalidation_reason
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'json', ?8, ?8, ?9, ?10, NULL, NULL)",
        params![
            write.key.session_id.0.as_str(),
            write.key.module_id.as_str(),
            write.key.strategy_id.as_str(),
            write.profile_fingerprint.as_str(),
            write.provider_fingerprint.as_str(),
            write.payload_version.as_str(),
            payload_json,
            write.now.as_str(),
            write.expires_at.as_deref(),
            write.last_wake_id.as_deref(),
        ],
    )
    .map_err(|error| persistence_error("insert provider wire state", error))?;
    load_provider_wire_state_by_row_id(tx, tx.last_insert_rowid())
}

fn load_provider_wire_state_for_wake_in_tx(
    tx: &rusqlite::Transaction<'_>,
    lookup: &ProviderWireStateWakeLookup,
) -> CoreResult<ProviderWireStateWakeResult> {
    validate_provider_wire_state_key(&lookup.key)?;
    invalidate_provider_wire_states_for_session_except_in_tx(tx, &lookup.key, &lookup.now)?;
    let Some(record) = load_current_provider_wire_state_by_key(tx, &lookup.key)? else {
        return Ok(ProviderWireStateWakeResult {
            record: None,
            absence_reason: Some(ProviderStateAbsenceReason::Missing),
        });
    };
    if record
        .expires_at
        .as_ref()
        .is_some_and(|expires| expires <= &lookup.now)
    {
        invalidate_provider_wire_state_by_row_in_tx(
            tx,
            record.row_id,
            &lookup.now,
            ProviderWireStateInvalidationReason::Expired,
        )?;
        return Ok(ProviderWireStateWakeResult {
            record: None,
            absence_reason: Some(ProviderStateAbsenceReason::Expired),
        });
    }
    if record.profile_fingerprint != lookup.profile_fingerprint {
        // Fingerprint drift is a recoverable rebuild boundary, not a reason to
        // destroy the last provider-owned continuation. Keep the old row
        // available for a later rollback and let the brain reconstruct from
        // the durable conversation projection. A successful replacement will
        // supersede it transactionally.
        return Ok(ProviderWireStateWakeResult {
            record: None,
            absence_reason: Some(ProviderStateAbsenceReason::Invalidated),
        });
    }
    if record.provider_fingerprint != lookup.provider_fingerprint {
        // See the profile-fingerprint branch above. Provider changes may be
        // reversed, and failed reconstruction must not erase the prior state.
        return Ok(ProviderWireStateWakeResult {
            record: None,
            absence_reason: Some(ProviderStateAbsenceReason::Invalidated),
        });
    }
    Ok(ProviderWireStateWakeResult {
        record: Some(record),
        absence_reason: None,
    })
}

fn clear_provider_wire_state_in_tx(
    tx: &rusqlite::Transaction<'_>,
    key: &ProviderWireStateKey,
    now: &IsoTimestamp,
    reason: ProviderWireStateInvalidationReason,
) -> CoreResult<Option<ProviderWireStateRecord>> {
    validate_provider_wire_state_key(key)?;
    let Some(record) = load_current_provider_wire_state_by_key(tx, key)? else {
        return Ok(None);
    };
    invalidate_provider_wire_state_by_row_in_tx(tx, record.row_id, now, reason)?;
    load_provider_wire_state_by_row_id(tx, record.row_id).map(Some)
}

pub(crate) fn expire_provider_wire_states_in_tx(
    tx: &rusqlite::Transaction<'_>,
    now: &IsoTimestamp,
) -> CoreResult<Vec<ProviderWireStateRecord>> {
    let expiring = load_expired_current_provider_wire_states(tx, now)?;
    for record in &expiring {
        invalidate_provider_wire_state_by_row_in_tx(
            tx,
            record.row_id,
            now,
            ProviderWireStateInvalidationReason::Expired,
        )?;
    }
    expiring
        .into_iter()
        .map(|record| load_provider_wire_state_by_row_id(tx, record.row_id))
        .collect()
}

fn invalidate_provider_wire_states_for_session_except_in_tx(
    tx: &rusqlite::Transaction<'_>,
    key: &ProviderWireStateKey,
    now: &IsoTimestamp,
) -> CoreResult<()> {
    tx.execute(
        "UPDATE provider_wire_states
         SET invalidated_at = ?4,
             updated_at = ?4,
             invalidation_reason = CASE
                 WHEN module_id != ?2 THEN 'module_changed'
                 ELSE 'strategy_changed'
             END
         WHERE session_id = ?1
           AND invalidated_at IS NULL
           AND (module_id != ?2 OR strategy_id != ?3)",
        params![
            key.session_id.0.as_str(),
            key.module_id.as_str(),
            key.strategy_id.as_str(),
            now.as_str(),
        ],
    )
    .map_err(|error| persistence_error("invalidate changed provider wire state", error))?;
    Ok(())
}

fn invalidate_current_provider_wire_state_for_key_in_tx(
    tx: &rusqlite::Transaction<'_>,
    key: &ProviderWireStateKey,
    now: &IsoTimestamp,
    reason: ProviderWireStateInvalidationReason,
) -> CoreResult<()> {
    tx.execute(
        "UPDATE provider_wire_states
         SET invalidated_at = ?4,
             updated_at = ?4,
             invalidation_reason = ?5
         WHERE session_id = ?1
           AND module_id = ?2
           AND strategy_id = ?3
           AND invalidated_at IS NULL",
        params![
            key.session_id.0.as_str(),
            key.module_id.as_str(),
            key.strategy_id.as_str(),
            now.as_str(),
            provider_wire_state_invalidation_reason_as_str(reason),
        ],
    )
    .map_err(|error| persistence_error("invalidate current provider wire state", error))?;
    Ok(())
}

fn invalidate_provider_wire_state_by_row_in_tx(
    tx: &rusqlite::Transaction<'_>,
    row_id: i64,
    now: &IsoTimestamp,
    reason: ProviderWireStateInvalidationReason,
) -> CoreResult<()> {
    tx.execute(
        "UPDATE provider_wire_states
         SET invalidated_at = ?2,
             updated_at = ?2,
             invalidation_reason = ?3
         WHERE row_id = ?1
           AND invalidated_at IS NULL",
        params![
            row_id,
            now.as_str(),
            provider_wire_state_invalidation_reason_as_str(reason),
        ],
    )
    .map_err(|error| persistence_error("invalidate provider wire state row", error))?;
    Ok(())
}

fn load_current_provider_wire_state_by_key(
    conn: &Connection,
    key: &ProviderWireStateKey,
) -> CoreResult<Option<ProviderWireStateRecord>> {
    conn.query_row(
        "SELECT
            row_id,
            session_id,
            module_id,
            strategy_id,
            profile_fingerprint,
            provider_fingerprint,
            payload_version,
            payload_json,
            payload_encoding,
            created_at,
            updated_at,
            expires_at,
            last_wake_id,
            invalidated_at,
            invalidation_reason
         FROM provider_wire_states
         WHERE session_id = ?1
           AND module_id = ?2
           AND strategy_id = ?3
           AND invalidated_at IS NULL
         LIMIT 1",
        params![
            key.session_id.0.as_str(),
            key.module_id.as_str(),
            key.strategy_id.as_str(),
        ],
        row_to_provider_wire_state_record,
    )
    .optional()
    .map_err(|error| persistence_error("load current provider wire state", error))
}

fn load_provider_wire_state_by_row_id(
    conn: &Connection,
    row_id: i64,
) -> CoreResult<ProviderWireStateRecord> {
    conn.query_row(
        "SELECT
            row_id,
            session_id,
            module_id,
            strategy_id,
            profile_fingerprint,
            provider_fingerprint,
            payload_version,
            payload_json,
            payload_encoding,
            created_at,
            updated_at,
            expires_at,
            last_wake_id,
            invalidated_at,
            invalidation_reason
         FROM provider_wire_states
         WHERE row_id = ?1",
        params![row_id],
        row_to_provider_wire_state_record,
    )
    .map_err(|error| persistence_error("load provider wire state by row id", error))
}

fn load_expired_current_provider_wire_states(
    conn: &Connection,
    now: &IsoTimestamp,
) -> CoreResult<Vec<ProviderWireStateRecord>> {
    let mut stmt = conn
        .prepare(
            "SELECT
                row_id,
                session_id,
                module_id,
                strategy_id,
                profile_fingerprint,
                provider_fingerprint,
                payload_version,
                payload_json,
                payload_encoding,
                created_at,
                updated_at,
                expires_at,
                last_wake_id,
                invalidated_at,
                invalidation_reason
             FROM provider_wire_states
             WHERE invalidated_at IS NULL
               AND expires_at IS NOT NULL
               AND expires_at <= ?1
             ORDER BY expires_at ASC, row_id ASC",
        )
        .map_err(|error| persistence_error("prepare expired provider wire state query", error))?;
    let rows = stmt
        .query_map(params![now.as_str()], row_to_provider_wire_state_record)
        .map_err(|error| persistence_error("query expired provider wire states", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load expired provider wire states", error))
}

fn list_provider_wire_state_diagnostics(
    conn: &Connection,
    limit: u32,
) -> CoreResult<Vec<ProviderWireStateDiagnostic>> {
    let mut stmt = conn
        .prepare(
            "SELECT
                row_id,
                session_id,
                module_id,
                strategy_id,
                profile_fingerprint,
                provider_fingerprint,
                payload_version,
                length(payload_json),
                created_at,
                updated_at,
                expires_at,
                last_wake_id,
                invalidated_at,
                invalidation_reason
             FROM provider_wire_states
             ORDER BY
                CASE
                    WHEN invalidated_at IS NULL AND invalidation_reason IS NULL THEN 0
                    ELSE 1
                END ASC,
                updated_at DESC,
                row_id DESC
             LIMIT ?1",
        )
        .map_err(|error| persistence_error("prepare provider wire state diagnostics", error))?;
    let rows = stmt
        .query_map(params![limit], row_to_provider_wire_state_diagnostic)
        .map_err(|error| persistence_error("query provider wire state diagnostics", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load provider wire state diagnostics", error))
}

fn row_to_provider_wire_state_record(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ProviderWireStateRecord> {
    let payload_json: String = row.get(7)?;
    let invalidation_reason = row
        .get::<_, Option<String>>(14)?
        .map(|raw| provider_wire_state_invalidation_reason_from_str(&raw))
        .transpose()?;
    Ok(ProviderWireStateRecord {
        row_id: row.get(0)?,
        key: ProviderWireStateKey {
            session_id: SessionId(row.get(1)?),
            module_id: row.get(2)?,
            strategy_id: row.get(3)?,
        },
        profile_fingerprint: row.get(4)?,
        provider_fingerprint: row.get(5)?,
        payload_version: row.get(6)?,
        payload_json: from_json_text(&payload_json).map_err(to_sql_error)?,
        payload_encoding: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        expires_at: row.get(11)?,
        last_wake_id: row.get(12)?,
        invalidated_at: row.get(13)?,
        invalidation_reason,
    })
}

fn row_to_provider_wire_state_diagnostic(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ProviderWireStateDiagnostic> {
    Ok(ProviderWireStateDiagnostic {
        key: ProviderWireStateKey {
            session_id: SessionId(row.get(1)?),
            module_id: row.get(2)?,
            strategy_id: row.get(3)?,
        },
        row_id: row.get(0)?,
        profile_fingerprint: row.get(4)?,
        provider_fingerprint: row.get(5)?,
        payload_version: row.get(6)?,
        payload_bytes: row.get::<_, u64>(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        expires_at: row.get(10)?,
        last_wake_id: row.get(11)?,
        invalidated_at: row.get(12)?,
        invalidation_reason: row.get(13)?,
        is_current: row.get::<_, Option<String>>(13)?.is_none()
            && row.get::<_, Option<String>>(12)?.is_none(),
    })
}

pub(crate) fn validate_provider_wire_state_key(key: &ProviderWireStateKey) -> CoreResult<()> {
    if key.session_id.0.trim().is_empty()
        || key.module_id.trim().is_empty()
        || key.strategy_id.trim().is_empty()
    {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "provider wire state key requires session_id, module_id, and strategy_id",
        ));
    }
    Ok(())
}

fn provider_wire_state_invalidation_reason_as_str(
    reason: ProviderWireStateInvalidationReason,
) -> &'static str {
    match reason {
        ProviderWireStateInvalidationReason::ProfileChanged => "profile_changed",
        ProviderWireStateInvalidationReason::ProviderChanged => "provider_changed",
        ProviderWireStateInvalidationReason::ModuleChanged => "module_changed",
        ProviderWireStateInvalidationReason::StrategyChanged => "strategy_changed",
        ProviderWireStateInvalidationReason::Expired => "expired",
        ProviderWireStateInvalidationReason::BrainRequestedClear => "brain_requested_clear",
        ProviderWireStateInvalidationReason::OperatorRequestedClear => "operator_requested_clear",
        ProviderWireStateInvalidationReason::Superseded => "superseded",
    }
}

fn provider_wire_state_invalidation_reason_from_str(
    raw: &str,
) -> rusqlite::Result<ProviderWireStateInvalidationReason> {
    match raw {
        "profile_changed" => Ok(ProviderWireStateInvalidationReason::ProfileChanged),
        "provider_changed" => Ok(ProviderWireStateInvalidationReason::ProviderChanged),
        "module_changed" => Ok(ProviderWireStateInvalidationReason::ModuleChanged),
        "strategy_changed" => Ok(ProviderWireStateInvalidationReason::StrategyChanged),
        "expired" => Ok(ProviderWireStateInvalidationReason::Expired),
        "brain_requested_clear" => Ok(ProviderWireStateInvalidationReason::BrainRequestedClear),
        "operator_requested_clear" => {
            Ok(ProviderWireStateInvalidationReason::OperatorRequestedClear)
        }
        "superseded" => Ok(ProviderWireStateInvalidationReason::Superseded),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("unknown provider wire state invalidation reason {other}").into(),
        )),
    }
}
