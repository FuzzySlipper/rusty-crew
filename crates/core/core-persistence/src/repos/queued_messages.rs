use super::super::*;

pub(crate) fn migrate_v8_add_queued_message_retention(
    tx: &rusqlite::Transaction<'_>,
) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS queued_messages (
                message_id TEXT PRIMARY KEY,
                owner_session_id TEXT,
                owner_agent_id TEXT NOT NULL,
                from_agent TEXT NOT NULL,
                to_agent TEXT NOT NULL,
                body TEXT NOT NULL,
                correlation_id TEXT,
                source_sequence INTEGER,
                enqueued_at TEXT NOT NULL,
                expires_at TEXT NOT NULL,
                ttl_ms INTEGER NOT NULL,
                delivery_attempts INTEGER NOT NULL DEFAULT 0,
                state TEXT NOT NULL,
                terminal_at TEXT,
                state_reason TEXT,
                message_json TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_queued_messages_state_expiry
                ON queued_messages(state, expires_at);
            CREATE INDEX IF NOT EXISTS idx_queued_messages_owner_agent
                ON queued_messages(owner_agent_id, state, expires_at);
            CREATE INDEX IF NOT EXISTS idx_queued_messages_owner_session
                ON queued_messages(owner_session_id, state, expires_at);
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 8", error))
}

pub(crate) fn save_queued_message_in_tx(
    tx: &rusqlite::Transaction<'_>,
    record: &QueuedMessageRecord,
) -> CoreResult<()> {
    let message_json = to_json_text(&record.message)?;
    tx.execute(
        "INSERT INTO queued_messages (
            message_id,
            owner_session_id,
            owner_agent_id,
            from_agent,
            to_agent,
            body,
            correlation_id,
            source_sequence,
            enqueued_at,
            expires_at,
            ttl_ms,
            delivery_attempts,
            state,
            terminal_at,
            state_reason,
            message_json
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
        ON CONFLICT(message_id) DO UPDATE SET
            owner_session_id = excluded.owner_session_id,
            owner_agent_id = excluded.owner_agent_id,
            from_agent = excluded.from_agent,
            to_agent = excluded.to_agent,
            body = excluded.body,
            correlation_id = excluded.correlation_id,
            source_sequence = excluded.source_sequence,
            expires_at = excluded.expires_at,
            ttl_ms = excluded.ttl_ms,
            delivery_attempts = excluded.delivery_attempts,
            state = excluded.state,
            terminal_at = excluded.terminal_at,
            state_reason = excluded.state_reason,
            message_json = excluded.message_json",
        params![
            record.message_id,
            record
                .owner_session_id
                .as_ref()
                .map(|value| value.0.as_str()),
            record.owner_agent_id.0,
            record.message.from.0,
            record.message.to.0,
            record.message.body,
            record.message.correlation_id,
            record.source_sequence.map(|value| value as i64),
            record.enqueued_at,
            record.expires_at,
            record.ttl_ms as i64,
            record.delivery_attempts as i64,
            queued_message_state_as_str(record.state),
            record.terminal_at,
            record.state_reason,
            message_json,
        ],
    )
    .map_err(|error| persistence_error("save queued message", error))?;
    save_queued_message_search_row_in_tx(tx, record)
}

pub(crate) fn expire_queued_messages_in_tx(
    tx: &rusqlite::Transaction<'_>,
    now: &IsoTimestamp,
) -> CoreResult<Vec<QueuedMessageRecord>> {
    let expiring = load_queued_messages_in_tx(
        tx,
        &QueuedMessageFilter {
            state: Some(QueuedMessageState::Pending),
            owner_session_id: None,
            owner_agent_id: None,
            limit: None,
        },
    )?
    .into_iter()
    .filter(|message| message.expires_at <= *now)
    .collect::<Vec<_>>();

    for mut message in expiring.clone() {
        message.state = QueuedMessageState::Expired;
        message.terminal_at = Some(now.clone());
        message.state_reason = Some("ttl_expired".to_string());
        save_queued_message_in_tx(tx, &message)?;
        increment_counter_for_scopes_in_tx(
            tx,
            queued_message_counter_scopes(&message),
            COUNTER_QUEUE_EXPIRATIONS,
            1,
        )?;
    }
    Ok(expiring
        .into_iter()
        .map(|mut message| {
            message.state = QueuedMessageState::Expired;
            message.terminal_at = Some(now.clone());
            message.state_reason = Some("ttl_expired".to_string());
            message
        })
        .collect())
}

pub(crate) fn purge_terminal_queued_messages_in_tx(
    tx: &rusqlite::Transaction<'_>,
    cutoff: &IsoTimestamp,
) -> CoreResult<u64> {
    tx.execute(
        "DELETE FROM runtime_search_fts
         WHERE row_type = 'queue_message'
           AND row_key IN (
               SELECT message_id FROM queued_messages
               WHERE state IN ('delivered', 'expired', 'discarded', 'cancelled')
                 AND terminal_at IS NOT NULL
                 AND terminal_at < ?1
           )",
        params![cutoff],
    )
    .map_err(|error| persistence_error("delete purged queue search rows", error))?;
    let purged = tx
        .execute(
            "DELETE FROM queued_messages
             WHERE state IN ('delivered', 'expired', 'discarded', 'cancelled')
               AND terminal_at IS NOT NULL
               AND terminal_at < ?1",
            params![cutoff],
        )
        .map_err(|error| persistence_error("purge terminal queued messages", error))?;
    Ok(purged as u64)
}

pub(crate) fn load_queued_messages(
    conn: &Connection,
    filter: &QueuedMessageFilter,
) -> CoreResult<Vec<QueuedMessageRecord>> {
    load_queued_messages_with_conn(conn, filter)
}

fn load_queued_messages_in_tx(
    tx: &rusqlite::Transaction<'_>,
    filter: &QueuedMessageFilter,
) -> CoreResult<Vec<QueuedMessageRecord>> {
    load_queued_messages_with_conn(tx, filter)
}

fn load_queued_messages_with_conn(
    conn: &Connection,
    filter: &QueuedMessageFilter,
) -> CoreResult<Vec<QueuedMessageRecord>> {
    let state = filter.state.map(queued_message_state_as_str);
    let owner_session_id = filter
        .owner_session_id
        .as_ref()
        .map(|value| value.0.as_str());
    let owner_agent_id = filter.owner_agent_id.as_ref().map(|value| value.0.as_str());
    let limit = filter.limit.unwrap_or(1_000).clamp(1, 10_000) as i64;
    let mut stmt = conn
        .prepare(
            "SELECT
                message_id,
                owner_session_id,
                owner_agent_id,
                from_agent,
                to_agent,
                body,
                correlation_id,
                source_sequence,
                enqueued_at,
                expires_at,
                ttl_ms,
                delivery_attempts,
                state,
                terminal_at,
                state_reason,
                message_json
             FROM queued_messages
             WHERE (?1 IS NULL OR state = ?1)
               AND (?2 IS NULL OR owner_session_id = ?2)
               AND (?3 IS NULL OR owner_agent_id = ?3)
             ORDER BY enqueued_at ASC, message_id ASC
             LIMIT ?4",
        )
        .map_err(|error| persistence_error("prepare queued message query", error))?;
    let rows = stmt
        .query_map(
            params![state, owner_session_id, owner_agent_id, limit],
            row_to_queued_message,
        )
        .map_err(|error| persistence_error("query queued messages", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load queued messages", error))
}

fn row_to_queued_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<QueuedMessageRecord> {
    let message_json: String = row.get(15)?;
    let state: String = row.get(12)?;
    Ok(QueuedMessageRecord {
        message_id: row.get(0)?,
        owner_session_id: row.get::<_, Option<String>>(1)?.map(SessionId),
        owner_agent_id: AgentId(row.get(2)?),
        message: from_json_text(&message_json).map_err(to_sql_error)?,
        source_sequence: row.get::<_, Option<i64>>(7)?.map(|value| value as u64),
        enqueued_at: row.get(8)?,
        expires_at: row.get(9)?,
        ttl_ms: row.get::<_, i64>(10)? as u32,
        delivery_attempts: row.get::<_, i64>(11)? as u32,
        state: queued_message_state_from_str(&state)?,
        terminal_at: row.get(13)?,
        state_reason: row.get(14)?,
    })
}

fn save_queued_message_search_row_in_tx(
    tx: &rusqlite::Transaction<'_>,
    record: &QueuedMessageRecord,
) -> CoreResult<()> {
    tx.execute(
        "DELETE FROM runtime_search_fts WHERE row_type = ?1 AND row_key = ?2",
        params!["queue_message", record.message_id],
    )
    .map_err(|error| persistence_error("delete queued message search row", error))?;
    insert_runtime_search_row(
        tx,
        &RuntimeSearchInsert {
            row_type: RuntimeSearchRowType::QueueMessage,
            row_key: record.message_id.clone(),
            sequence: record.source_sequence,
            session_id: record
                .owner_session_id
                .as_ref()
                .map(|value| value.0.clone()),
            agent_id: Some(record.owner_agent_id.0.clone()),
            instance_id: record
                .owner_session_id
                .as_ref()
                .map(|value| AgentInstanceId::new(format!("instance:{value}")).0),
            task_id: None,
            event_kind: Some(CoreEventKind::AgentMessageRouted),
            recorded_at: record.enqueued_at.clone(),
            title: format!(
                "queued message {}",
                queued_message_state_as_str(record.state)
            ),
            body: record.message.body.clone(),
        },
    )
}

fn queued_message_counter_scopes(message: &QueuedMessageRecord) -> Vec<RuntimeCounterScope> {
    let mut scopes = vec![
        RuntimeCounterScope::Runtime,
        RuntimeCounterScope::Agent(message.owner_agent_id.clone()),
    ];
    if let Some(session_id) = &message.owner_session_id {
        scopes.push(RuntimeCounterScope::Session(session_id.clone()));
        scopes.push(RuntimeCounterScope::Instance(AgentInstanceId::new(
            format!("instance:{session_id}"),
        )));
    }
    scopes
}

fn queued_message_state_as_str(state: QueuedMessageState) -> &'static str {
    match state {
        QueuedMessageState::Pending => "pending",
        QueuedMessageState::Delivered => "delivered",
        QueuedMessageState::Expired => "expired",
        QueuedMessageState::Discarded => "discarded",
        QueuedMessageState::Cancelled => "cancelled",
    }
}

fn queued_message_state_from_str(raw: &str) -> rusqlite::Result<QueuedMessageState> {
    match raw {
        "pending" => Ok(QueuedMessageState::Pending),
        "delivered" => Ok(QueuedMessageState::Delivered),
        "expired" => Ok(QueuedMessageState::Expired),
        "discarded" => Ok(QueuedMessageState::Discarded),
        "cancelled" => Ok(QueuedMessageState::Cancelled),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            12,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown queued message state {other}"),
            )),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn queued_message_repo_preserves_terminal_expiry_after_restart() {
        let db_path = std::env::temp_dir().join(format!(
            "rusty-crew-queued-message-repo-{}.sqlite3",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = CoordinationStore::open_file(&db_path).unwrap();
        store
            .save_queued_message(&QueuedMessageRecord {
                message_id: "queue-module-1".to_string(),
                owner_session_id: Some(SessionId::new("session-alpha")),
                owner_agent_id: AgentId::new("agent-alpha"),
                message: AgentMessage {
                    from: AgentId::new("operator"),
                    to: AgentId::new("agent-alpha"),
                    body: "expire once and never resurrect".to_string(),
                    correlation_id: None,
                    projection: None,
                },
                source_sequence: Some(7),
                enqueued_at: "2026-07-02T00:00:00Z".to_string(),
                expires_at: "2026-07-02T00:00:05Z".to_string(),
                ttl_ms: 5_000,
                delivery_attempts: 0,
                state: QueuedMessageState::Pending,
                terminal_at: None,
                state_reason: None,
            })
            .unwrap();

        let expired = store
            .expire_queued_messages_at(&"2026-07-02T00:00:06Z".to_string())
            .unwrap();
        assert_eq!(expired.len(), 1);
        drop(store);

        let reopened = CoordinationStore::open_file(&db_path).unwrap();
        let pending = reopened
            .load_queued_messages(&QueuedMessageFilter {
                state: Some(QueuedMessageState::Pending),
                owner_session_id: None,
                owner_agent_id: None,
                limit: None,
            })
            .unwrap();
        let expired = reopened
            .load_queued_messages(&QueuedMessageFilter {
                state: Some(QueuedMessageState::Expired),
                owner_session_id: None,
                owner_agent_id: None,
                limit: None,
            })
            .unwrap();

        assert!(pending.is_empty());
        assert_eq!(expired.len(), 1);
        assert_eq!(
            expired[0].terminal_at.as_deref(),
            Some("2026-07-02T00:00:06Z")
        );

        drop(reopened);
        let _ = std::fs::remove_file(db_path);
    }
}
