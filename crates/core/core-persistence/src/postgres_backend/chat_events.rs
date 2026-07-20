//! PostgreSQL chat event replay log repository.

use super::*;

const CHAT_EVENT_COUNTER_NAME: &str = "chat_events";

impl PostgresBackendStore {
    pub fn append_chat_event(&self, event: &ChatEventLogAppend) -> CoreResult<ChatEventLogEvent> {
        crate::repos::chat_events::validate_chat_event_append(event)?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start append PostgreSQL chat event", error))?;
        let sequence = next_chat_event_sequence_in_tx(&mut tx, &schema, &event.session_id)?;
        let record = crate::repos::chat_events::chat_event_record(event, sequence);
        tx.execute(
            &format!(
                "INSERT INTO {schema}.chat_events (
                    session_id,
                    sequence_id,
                    event_id,
                    created_at,
                    kind,
                    payload_json
                 ) VALUES ($1, $2, $3, $4, $5, $6)"
            ),
            &[
                &record.session_id.0,
                &(record.sequence_id as i64),
                &record.event_id,
                &record.created_at,
                &record.kind,
                &to_json_text(&record.payload_json)?,
            ],
        )
        .map_err(|error| postgres_error("append PostgreSQL chat event", error))?;
        tx.commit()
            .map_err(|error| postgres_error("commit append PostgreSQL chat event", error))?;
        Ok(record)
    }

    pub fn query_chat_events(&self, query: &ChatEventLogQuery) -> CoreResult<ChatEventLogPage> {
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        query_chat_events(&mut *client, &schema, query)
    }
}

fn query_chat_events(
    client: &mut impl GenericClient,
    schema: &str,
    query: &ChatEventLogQuery,
) -> CoreResult<ChatEventLogPage> {
    let requested_after = crate::repos::chat_events::chat_event_cursor_sequence(
        query.cursor.as_deref(),
        &query.session_id,
    );
    let stats = client
        .query_one(
            &format!(
                "SELECT COUNT(*), COALESCE(MAX(sequence_id), 0),
                        COALESCE(SUM(CASE WHEN kind IN ('message_created', 'assistant_message_completed') THEN 1 ELSE 0 END), 0)
                 FROM {schema}.chat_events WHERE session_id = $1"
            ),
            &[&query.session_id.0],
        )
        .map_err(|error| postgres_error("read PostgreSQL chat event page stats", error))?;
    let total = stats.get::<_, i64>(0).max(0) as u64;
    let latest_sequence = stats.get::<_, i64>(1).max(0) as u64;
    let message_count = stats.get::<_, i64>(2).max(0) as u64;
    let cursor_ahead = total > 0 && requested_after > latest_sequence;
    let after = crate::repos::chat_events::normalize_chat_event_after(
        requested_after,
        latest_sequence,
        total,
    );
    let limit = crate::repos::chat_events::normalize_chat_event_limit(query.limit);
    if limit == 0 {
        return Ok(ChatEventLogPage {
            items: Vec::new(),
            latest_cursor: crate::repos::chat_events::chat_event_cursor_for(
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
    let probe_limit = limit.saturating_add(1) as i64;
    if query.cursor.is_none() {
        let rows = client
            .query(
                &format!(
                    "SELECT session_id, sequence_id, event_id, created_at, kind, payload_json
                     FROM {schema}.chat_events
                     WHERE session_id = $1
                     ORDER BY sequence_id DESC
                     LIMIT $2"
                ),
                &[&query.session_id.0, &probe_limit],
            )
            .map_err(|error| postgres_error("query latest PostgreSQL chat events", error))?;
        let mut records = rows
            .iter()
            .map(row_to_chat_event)
            .collect::<CoreResult<Vec<_>>>()?;
        let has_more = records.len() > limit as usize;
        records.truncate(limit as usize);
        records.reverse();
        let latest_sequence = records
            .last()
            .map(|event| event.sequence_id)
            .unwrap_or(after);
        return Ok(ChatEventLogPage {
            items: records,
            latest_cursor: crate::repos::chat_events::chat_event_cursor_for(
                &query.session_id,
                latest_sequence,
            ),
            has_more,
            total,
            message_count,
            has_more_before: has_more,
        });
    }
    let rows = client
        .query(
            &format!(
                "SELECT session_id, sequence_id, event_id, created_at, kind, payload_json
                 FROM {schema}.chat_events
                 WHERE session_id = $1 AND sequence_id > $2
                 ORDER BY sequence_id ASC
                 LIMIT $3"
            ),
            &[&query.session_id.0, &(after as i64), &probe_limit],
        )
        .map_err(|error| postgres_error("query PostgreSQL chat events", error))?;
    let mut records = rows
        .iter()
        .map(row_to_chat_event)
        .collect::<CoreResult<Vec<_>>>()?;
    let has_more = records.len() > limit as usize;
    records.truncate(limit as usize);
    let latest_sequence = records
        .last()
        .map(|event| event.sequence_id)
        .unwrap_or(after);
    Ok(ChatEventLogPage {
        items: records,
        latest_cursor: crate::repos::chat_events::chat_event_cursor_for(
            &query.session_id,
            latest_sequence,
        ),
        has_more,
        total,
        message_count,
        has_more_before: after > 0,
    })
}

fn next_chat_event_sequence_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    session_id: &SessionId,
) -> CoreResult<u64> {
    let now = postgres_now_iso(tx)?;
    let counter_row = tx
        .query_one(
            &format!(
                "INSERT INTO {schema}.runtime_counters (
                    scope_type,
                    scope_id,
                    counter_name,
                    value,
                    updated_at
                 ) VALUES ('session', $1, $2, 1, $3)
                 ON CONFLICT(scope_type, scope_id, counter_name) DO UPDATE SET
                    value = runtime_counters.value + 1,
                    updated_at = EXCLUDED.updated_at
                 RETURNING value"
            ),
            &[&session_id.0, &CHAT_EVENT_COUNTER_NAME, &now],
        )
        .map_err(|error| postgres_error("allocate PostgreSQL chat event sequence", error))?;
    let counter_sequence = counter_row.get::<_, i64>(0).max(0) as u64;
    let latest_row = tx
        .query_one(
            &format!(
                "SELECT COALESCE(MAX(sequence_id), 0)
                 FROM {schema}.chat_events
                 WHERE session_id = $1"
            ),
            &[&session_id.0],
        )
        .map_err(|error| postgres_error("read latest PostgreSQL chat event sequence", error))?;
    let next_from_rows = (latest_row.get::<_, i64>(0).max(0) as u64).saturating_add(1);
    Ok(counter_sequence.max(next_from_rows))
}

fn row_to_chat_event(row: &postgres::Row) -> CoreResult<ChatEventLogEvent> {
    Ok(ChatEventLogEvent {
        session_id: SessionId::new(row.get::<_, String>(0)),
        sequence_id: row.get::<_, i64>(1) as u64,
        event_id: row.get(2),
        created_at: row.get(3),
        kind: row.get(4),
        payload_json: from_json_text(&row.get::<_, String>(5)).map_err(|error| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("invalid PostgreSQL chat event payload JSON: {error}"),
            )
        })?,
    })
}
