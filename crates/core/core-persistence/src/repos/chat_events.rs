use super::super::*;

const DEFAULT_CHAT_EVENT_LIMIT: u32 = 500;
const MAX_CHAT_EVENT_LIMIT: u32 = 1_000;

impl CoordinationStore {
    pub fn append_chat_event(&self, event: &ChatEventLogAppend) -> CoreResult<ChatEventLogEvent> {
        validate_chat_event_append(event)?;
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("begin append chat event", error))?;
        let sequence = next_chat_event_sequence_in_tx(&tx, &event.session_id)?;
        let record = chat_event_record(event, sequence);
        tx.execute(
            "INSERT INTO chat_events (
                session_id,
                sequence_id,
                event_id,
                created_at,
                kind,
                payload_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                record.session_id.0,
                record.sequence_id as i64,
                record.event_id,
                record.created_at,
                record.kind,
                to_json_text(&record.payload_json)?,
            ],
        )
        .map_err(|error| persistence_error("append chat event", error))?;
        tx.commit()
            .map_err(|error| persistence_error("commit append chat event", error))?;
        Ok(record)
    }

    pub fn query_chat_events(&self, query: &ChatEventLogQuery) -> CoreResult<ChatEventLogPage> {
        let conn = self.conn()?;
        query_chat_events(&conn, query)
    }
}

pub(crate) fn migrate_v32_add_chat_event_log(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS chat_events (
                session_id TEXT NOT NULL,
                sequence_id INTEGER NOT NULL,
                event_id TEXT NOT NULL UNIQUE,
                created_at TEXT NOT NULL,
                kind TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                PRIMARY KEY(session_id, sequence_id)
            );
            CREATE INDEX IF NOT EXISTS idx_chat_events_session_created
                ON chat_events(session_id, created_at, sequence_id);
            CREATE INDEX IF NOT EXISTS idx_chat_events_kind
                ON chat_events(kind, created_at, session_id, sequence_id);
        ",
    )
    .map_err(|error| persistence_error("apply schema migration 32", error))
}

pub(crate) fn validate_chat_event_append(event: &ChatEventLogAppend) -> CoreResult<()> {
    if event.session_id.0.trim().is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "chat event session_id must not be empty",
        ));
    }
    if event.created_at.trim().is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "chat event created_at must not be empty",
        ));
    }
    if event.kind.trim().is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "chat event kind must not be empty",
        ));
    }
    Ok(())
}

fn query_chat_events(conn: &Connection, query: &ChatEventLogQuery) -> CoreResult<ChatEventLogPage> {
    let requested_after = chat_event_cursor_sequence(query.cursor.as_deref(), &query.session_id);
    let (total, latest_sequence, message_count): (i64, i64, i64) = conn
        .query_row(
            "SELECT COUNT(*), COALESCE(MAX(sequence_id), 0),
                    COALESCE(SUM(CASE WHEN kind IN ('message_created', 'assistant_message_completed') THEN 1 ELSE 0 END), 0)
             FROM chat_events WHERE session_id = ?1",
            params![query.session_id.0],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|error| persistence_error("read chat event page stats", error))?;
    let total = total.max(0) as u64;
    let latest_sequence = latest_sequence.max(0) as u64;
    let message_count = message_count.max(0) as u64;
    let cursor_ahead = total > 0 && requested_after > latest_sequence;
    let after = normalize_chat_event_after(requested_after, latest_sequence, total);
    let limit = normalize_chat_event_limit(query.limit);
    if limit == 0 {
        return Ok(ChatEventLogPage {
            items: Vec::new(),
            latest_cursor: chat_event_cursor_for(
                &query.session_id,
                if query.cursor.is_none() || cursor_ahead {
                    latest_sequence
                } else {
                    after
                },
            ),
            has_more: false,
            total,
            message_count,
            has_more_before: query.cursor.is_none() && total > 0,
        });
    }
    let probe_limit = limit.saturating_add(1).min(MAX_CHAT_EVENT_LIMIT + 1);
    if query.cursor.is_none() {
        let mut stmt = conn
            .prepare(
                "SELECT session_id, sequence_id, event_id, created_at, kind, payload_json
                 FROM chat_events
                 WHERE session_id = ?1
                 ORDER BY sequence_id DESC
                 LIMIT ?2",
            )
            .map_err(|error| persistence_error("prepare query latest chat events", error))?;
        let rows = stmt
            .query_map(
                params![query.session_id.0, probe_limit as i64],
                row_to_chat_event,
            )
            .map_err(|error| persistence_error("query latest chat events", error))?;
        let mut records = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| persistence_error("load latest chat events", error))?;
        let has_more = records.len() > limit as usize;
        records.truncate(limit as usize);
        records.reverse();
        let latest_sequence = records
            .last()
            .map(|event| event.sequence_id)
            .unwrap_or(after);
        return Ok(ChatEventLogPage {
            items: records,
            latest_cursor: chat_event_cursor_for(&query.session_id, latest_sequence),
            has_more,
            total,
            message_count,
            has_more_before: has_more,
        });
    }
    let mut stmt = conn
        .prepare(
            "SELECT session_id, sequence_id, event_id, created_at, kind, payload_json
             FROM chat_events
             WHERE session_id = ?1 AND sequence_id > ?2
             ORDER BY sequence_id ASC
             LIMIT ?3",
        )
        .map_err(|error| persistence_error("prepare query chat events", error))?;
    let rows = stmt
        .query_map(
            params![query.session_id.0, after as i64, probe_limit as i64],
            row_to_chat_event,
        )
        .map_err(|error| persistence_error("query chat events", error))?;
    let mut records = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load chat events", error))?;
    let has_more = records.len() > limit as usize;
    records.truncate(limit as usize);
    let latest_sequence = records
        .last()
        .map(|event| event.sequence_id)
        .unwrap_or(after);
    Ok(ChatEventLogPage {
        items: records,
        latest_cursor: chat_event_cursor_for(&query.session_id, latest_sequence),
        has_more,
        total,
        message_count,
        has_more_before: after > 0,
    })
}

fn next_chat_event_sequence_in_tx(
    tx: &rusqlite::Transaction<'_>,
    session_id: &SessionId,
) -> CoreResult<u64> {
    let latest = tx
        .query_row(
            "SELECT COALESCE(MAX(sequence_id), 0)
             FROM chat_events
             WHERE session_id = ?1",
            params![session_id.0],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| persistence_error("read latest chat event sequence", error))?;
    Ok((latest.max(0) as u64).saturating_add(1))
}

pub(crate) fn chat_event_cursor_for(session_id: &SessionId, sequence: u64) -> String {
    format!("{session_id}:{sequence}")
}

pub(crate) fn chat_event_cursor_sequence(cursor: Option<&str>, session_id: &SessionId) -> u64 {
    let Some(cursor) = cursor else {
        return 0;
    };
    let Some(sequence) = cursor.strip_prefix(&format!("{session_id}:")) else {
        return 0;
    };
    sequence.parse::<u64>().unwrap_or(0)
}

pub(crate) fn normalize_chat_event_after(
    requested_after: u64,
    latest_sequence: u64,
    total: u64,
) -> u64 {
    // A session can begin with a message-slot projection (for example an
    // imported roleplay fork) and later acquire its first durable event-log
    // entries. The slot cursor may be numerically ahead of the new event log.
    // Rebase that one stale-ahead cursor to the start so the client can cross
    // projections without waiting forever for an impossible sequence.
    if total > 0 && requested_after > latest_sequence {
        0
    } else {
        requested_after
    }
}

pub(crate) fn normalize_chat_event_limit(limit: Option<u32>) -> u32 {
    limit
        .unwrap_or(DEFAULT_CHAT_EVENT_LIMIT)
        .min(MAX_CHAT_EVENT_LIMIT)
}

pub(crate) fn chat_event_record(event: &ChatEventLogAppend, sequence: u64) -> ChatEventLogEvent {
    ChatEventLogEvent {
        event_id: chat_event_cursor_for(&event.session_id, sequence),
        session_id: event.session_id.clone(),
        sequence_id: sequence,
        created_at: event.created_at.clone(),
        kind: event.kind.clone(),
        payload_json: event.payload_json.clone(),
    }
}

fn row_to_chat_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChatEventLogEvent> {
    let payload_json = from_json_text(&row.get::<_, String>(5)?).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(ChatEventLogEvent {
        session_id: SessionId::new(row.get::<_, String>(0)?),
        sequence_id: row.get::<_, i64>(1)? as u64,
        event_id: row.get(2)?,
        created_at: row.get(3)?,
        kind: row.get(4)?,
        payload_json,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_ahead_projection_cursor_replays_new_event_log() {
        let db_path = std::env::temp_dir().join(format!(
            "rusty-crew-chat-events-stale-ahead-{}.sqlite3",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&db_path);
        let store = CoordinationStore::open_file(&db_path).unwrap();
        let session_id = SessionId::new("imported-fork");
        for kind in ["message_created", "assistant_turn_finished"] {
            store
                .append_chat_event(&ChatEventLogAppend {
                    session_id: session_id.clone(),
                    created_at: "2026-07-20T01:00:00Z".to_string(),
                    kind: kind.to_string(),
                    payload_json: serde_json::json!({}),
                })
                .unwrap();
        }

        let page = store
            .query_chat_events(&ChatEventLogQuery {
                session_id: session_id.clone(),
                cursor: Some(chat_event_cursor_for(&session_id, 71)),
                limit: Some(100),
            })
            .unwrap();

        assert_eq!(page.items.len(), 2);
        assert_eq!(page.latest_cursor, "imported-fork:2");
        assert!(!page.has_more_before);

        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }
}
