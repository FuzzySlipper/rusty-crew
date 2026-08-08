use super::super::*;

impl CoordinationStore {
    pub fn query_agent_messages(
        &self,
        query: &AgentMessageQuery,
    ) -> CoreResult<Vec<AgentMessageRecord>> {
        let conn = self.conn()?;
        query_agent_messages(&conn, query)
    }

    pub fn save_event(&self, sequence: u64, event: &CoreEvent) -> CoreResult<()> {
        if !should_persist_event(event) {
            return Ok(());
        }

        let event_kind = format!("{:?}", CoreEventKind::of(event));
        let event_json = to_json_text(event)?;
        let conn = self.conn()?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|error| persistence_error("begin save event", error))?;
        let is_new_event = tx
            .query_row(
                "SELECT NOT EXISTS(SELECT 1 FROM event_history WHERE sequence = ?1)",
                params![sequence as i64],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| persistence_error("check existing event sequence", error))?
            != 0;
        tx.execute(
            "INSERT OR REPLACE INTO event_history (sequence, event_kind, event_json)
             VALUES (?1, ?2, ?3)",
            params![sequence as i64, event_kind, event_json],
        )
        .map_err(|error| persistence_error("save event history", error))?;
        save_event_indexes_in_tx(&tx, sequence, event)?;
        if is_new_event {
            increment_event_counters_in_tx(&tx, event)?;
        }
        let recorded_at = tx
            .query_row(
                "SELECT recorded_at FROM event_history WHERE sequence = ?1",
                params![sequence as i64],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| persistence_error("read event recorded timestamp", error))?;
        save_event_search_rows_in_tx(&tx, sequence, event, &recorded_at)?;

        match event {
            CoreEvent::AgentMessageRouted { message } => {
                let message_json = to_json_text(message)?;
                tx.execute(
                    "INSERT OR REPLACE INTO agent_messages (
                        sequence,
                        from_agent,
                        to_agent,
                        body,
                        correlation_id,
                        message_json
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        sequence as i64,
                        message.from.0,
                        message.to.0,
                        message.body,
                        message.correlation_id,
                        message_json,
                    ],
                )
                .map_err(|error| persistence_error("save message history", error))?;
            }
            CoreEvent::CompletionPacketDelivered { packet } => {
                self.save_completion_packet_in_tx(&tx, sequence, packet)?;
            }
            CoreEvent::BrainEventObserved {
                session_id,
                wake_id,
                event,
            } => {
                self.save_tool_call_in_tx(&tx, sequence, session_id, wake_id.as_deref(), event)?;
            }
            _ => {}
        }

        tx.commit()
            .map_err(|error| persistence_error("commit save event", error))?;
        Ok(())
    }

    pub fn load_event_history(&self) -> CoreResult<Vec<PersistedEvent>> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare("SELECT sequence, event_json FROM event_history ORDER BY sequence ASC")
            .map_err(|error| persistence_error("prepare load event history", error))?;

        let rows = stmt
            .query_map([], |row| {
                let event_json: String = row.get(1)?;
                let event = from_json_text::<CoreEvent>(&event_json).map_err(to_sql_error)?;
                Ok(PersistedEvent {
                    sequence: row.get::<_, i64>(0)? as u64,
                    event,
                })
            })
            .map_err(|error| persistence_error("query event history", error))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| persistence_error("load event history", error))
    }

    pub fn query_events(&self, filter: &RuntimeEventFilter) -> CoreResult<Vec<RuntimeEventRecord>> {
        let conn = self.conn()?;
        let kind = filter.kind.as_ref().map(|kind| format!("{kind:?}"));
        let session_id = filter.session_id.as_ref().map(|value| value.0.as_str());
        let agent_id = filter.agent_id.as_ref().map(|value| value.0.as_str());
        let instance_id = filter.instance_id.as_ref().map(|value| value.0.as_str());
        let correlation_id = filter.correlation_id.as_deref();
        let source_wake_id = filter.source_wake_id.as_deref();
        let limit = filter.limit.unwrap_or(1_000).max(1) as i64;
        let mut stmt = conn
            .prepare(
                "SELECT sequence, event_kind, recorded_at, event_json
                 FROM event_history
                 WHERE (?1 IS NULL OR event_kind = ?1)
                   AND (?2 IS NULL OR EXISTS (
                        SELECT 1 FROM event_session_index
                        WHERE event_session_index.sequence = event_history.sequence
                          AND event_session_index.session_id = ?2
                   ))
                   AND (?3 IS NULL OR EXISTS (
                        SELECT 1 FROM event_agent_index
                        WHERE event_agent_index.sequence = event_history.sequence
                          AND event_agent_index.agent_id = ?3
                   ))
                   AND (?4 IS NULL OR EXISTS (
                        SELECT 1 FROM event_instance_index
                        WHERE event_instance_index.sequence = event_history.sequence
                          AND event_instance_index.instance_id = ?4
                   ))
                   AND (?5 IS NULL OR EXISTS (
                        SELECT 1 FROM event_correlation_index
                        WHERE event_correlation_index.sequence = event_history.sequence
                          AND event_correlation_index.correlation_id = ?5
                   ))
                   AND (?6 IS NULL OR EXISTS (
                        SELECT 1 FROM event_wake_index
                        WHERE event_wake_index.sequence = event_history.sequence
                          AND event_wake_index.source_wake_id = ?6
                   ))
                 ORDER BY sequence ASC
                 LIMIT ?7",
            )
            .map_err(|error| persistence_error("prepare query events", error))?;
        let rows = stmt
            .query_map(
                params![
                    kind,
                    session_id,
                    agent_id,
                    instance_id,
                    correlation_id,
                    source_wake_id,
                    limit,
                ],
                |row| {
                    let sequence = row.get::<_, i64>(0)? as u64;
                    let event_json: String = row.get(3)?;
                    let event = from_json_text::<CoreEvent>(&event_json).map_err(to_sql_error)?;
                    Ok(RuntimeEventRecord {
                        sequence,
                        kind: CoreEventKind::of(&event),
                        recorded_at: row.get(2)?,
                        event,
                        session_ids: load_event_index_values(
                            &conn,
                            EventIndexProjection::Session,
                            sequence,
                        )?
                        .into_iter()
                        .map(SessionId)
                        .collect(),
                        agent_ids: load_event_index_values(
                            &conn,
                            EventIndexProjection::Agent,
                            sequence,
                        )?
                        .into_iter()
                        .map(AgentId)
                        .collect(),
                        instance_ids: load_event_index_values(
                            &conn,
                            EventIndexProjection::Instance,
                            sequence,
                        )?
                        .into_iter()
                        .map(AgentInstanceId)
                        .collect(),
                        correlation_ids: load_event_index_values(
                            &conn,
                            EventIndexProjection::Correlation,
                            sequence,
                        )?,
                        source_wake_ids: load_event_index_values(
                            &conn,
                            EventIndexProjection::Wake,
                            sequence,
                        )?,
                    })
                },
            )
            .map_err(|error| persistence_error("query events", error))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| persistence_error("load queried events", error))
    }
}

fn query_agent_messages(
    conn: &Connection,
    query: &AgentMessageQuery,
) -> CoreResult<Vec<AgentMessageRecord>> {
    let agent_id = query.agent_id.as_ref().map(|value| value.0.as_str());
    let correlation_id = query.correlation_id.as_deref();
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let mut stmt = conn
        .prepare(
            "SELECT sequence, message_json
             FROM agent_messages
             WHERE (?1 IS NULL OR from_agent = ?1 OR to_agent = ?1)
               AND (?2 IS NULL OR correlation_id = ?2)
             ORDER BY sequence ASC
             LIMIT ?3 OFFSET ?4",
        )
        .map_err(|error| persistence_error("prepare query agent messages", error))?;
    let rows = stmt
        .query_map(params![agent_id, correlation_id, limit, offset], |row| {
            let message_json: String = row.get(1)?;
            Ok(AgentMessageRecord {
                sequence: row.get::<_, i64>(0)? as u64,
                message: from_json_text(&message_json).map_err(to_sql_error)?,
            })
        })
        .map_err(|error| persistence_error("query agent messages", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load queried agent messages", error))
}

fn save_event_indexes_in_tx(
    tx: &rusqlite::Transaction<'_>,
    sequence: u64,
    event: &CoreEvent,
) -> CoreResult<()> {
    let session_ids = event_session_ids(event);
    replace_event_index_values(
        tx,
        EventIndexProjection::Session,
        sequence,
        session_ids.iter().map(|id| id.0.clone()).collect(),
    )?;
    replace_event_index_values(
        tx,
        EventIndexProjection::Agent,
        sequence,
        event_agent_ids(event).into_iter().map(|id| id.0).collect(),
    )?;
    replace_event_index_values(
        tx,
        EventIndexProjection::Instance,
        sequence,
        session_ids
            .into_iter()
            .map(|id| AgentInstanceId::new(format!("instance:{id}")).0)
            .collect(),
    )?;
    replace_event_index_values(
        tx,
        EventIndexProjection::Correlation,
        sequence,
        event_correlation_ids(event),
    )?;
    replace_event_index_values(
        tx,
        EventIndexProjection::Wake,
        sequence,
        event_source_wake_ids(event),
    )
}

#[derive(Debug, Clone, Copy)]
enum EventIndexProjection {
    Session,
    Agent,
    Instance,
    Correlation,
    Wake,
}

impl EventIndexProjection {
    fn delete_sql(self) -> &'static str {
        match self {
            Self::Session => "DELETE FROM event_session_index WHERE sequence = ?1",
            Self::Agent => "DELETE FROM event_agent_index WHERE sequence = ?1",
            Self::Instance => "DELETE FROM event_instance_index WHERE sequence = ?1",
            Self::Correlation => "DELETE FROM event_correlation_index WHERE sequence = ?1",
            Self::Wake => "DELETE FROM event_wake_index WHERE sequence = ?1",
        }
    }

    fn insert_sql(self) -> &'static str {
        match self {
            Self::Session => {
                "INSERT OR IGNORE INTO event_session_index (sequence, session_id) VALUES (?1, ?2)"
            }
            Self::Agent => {
                "INSERT OR IGNORE INTO event_agent_index (sequence, agent_id) VALUES (?1, ?2)"
            }
            Self::Instance => {
                "INSERT OR IGNORE INTO event_instance_index (sequence, instance_id) VALUES (?1, ?2)"
            }
            Self::Correlation => {
                "INSERT OR IGNORE INTO event_correlation_index (sequence, correlation_id) VALUES (?1, ?2)"
            }
            Self::Wake => {
                "INSERT OR IGNORE INTO event_wake_index (sequence, source_wake_id) VALUES (?1, ?2)"
            }
        }
    }

    fn select_sql(self) -> &'static str {
        match self {
            Self::Session => {
                "SELECT session_id FROM event_session_index WHERE sequence = ?1 ORDER BY session_id ASC"
            }
            Self::Agent => {
                "SELECT agent_id FROM event_agent_index WHERE sequence = ?1 ORDER BY agent_id ASC"
            }
            Self::Instance => {
                "SELECT instance_id FROM event_instance_index WHERE sequence = ?1 ORDER BY instance_id ASC"
            }
            Self::Correlation => {
                "SELECT correlation_id FROM event_correlation_index WHERE sequence = ?1 ORDER BY correlation_id ASC"
            }
            Self::Wake => {
                "SELECT source_wake_id FROM event_wake_index WHERE sequence = ?1 ORDER BY source_wake_id ASC"
            }
        }
    }
}

fn replace_event_index_values(
    tx: &rusqlite::Transaction<'_>,
    projection: EventIndexProjection,
    sequence: u64,
    values: Vec<String>,
) -> CoreResult<()> {
    tx.execute(projection.delete_sql(), params![sequence as i64])
        .map_err(|error| persistence_error("delete event index values", error))?;
    for value in dedupe_non_empty(values) {
        tx.execute(projection.insert_sql(), params![sequence as i64, value])
            .map_err(|error| persistence_error("insert event index value", error))?;
    }
    Ok(())
}

fn load_event_index_values(
    conn: &Connection,
    projection: EventIndexProjection,
    sequence: u64,
) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(projection.select_sql())?;
    let rows = stmt.query_map(params![sequence as i64], |row| row.get::<_, String>(0))?;
    rows.collect::<Result<Vec<_>, _>>()
}

pub(crate) fn event_session_ids(event: &CoreEvent) -> Vec<SessionId> {
    match event {
        CoreEvent::SessionCreated { state } => vec![state.session_id.clone()],
        CoreEvent::SessionArchived { session_id }
        | CoreEvent::SessionWorkspaceChanged { session_id, .. } => vec![session_id.clone()],
        CoreEvent::DelegationLifecycleObserved { lifecycle } => vec![
            lifecycle.parent_session_id.clone(),
            lifecycle.delegated_session_id.clone(),
        ],
        CoreEvent::BrainWakeRequested { session_id }
        | CoreEvent::BrainEventObserved { session_id, .. }
        | CoreEvent::BrainActionsAccepted { session_id, .. } => vec![session_id.clone()],
        CoreEvent::SessionExecutionObserved { execution } => {
            vec![execution.session_id.clone()]
        }
        CoreEvent::LogicalTurnLifecycleObserved { lifecycle } => {
            vec![lifecycle.session_id.clone()]
        }
        CoreEvent::CompletionPacketDelivered { packet } => vec![packet.session_id.clone()],
        CoreEvent::AgentRoundObserved { round } => round
            .sender_session_id
            .iter()
            .cloned()
            .chain(std::iter::once(round.recipient_session_id.clone()))
            .collect(),
        CoreEvent::AgentMessageRouted { .. }
        | CoreEvent::AgentMessageDeliveryObserved { .. }
        | CoreEvent::ExternalEventInjected { .. }
        | CoreEvent::DenDataUpdated { .. } => Vec::new(),
    }
}

pub(crate) fn event_agent_ids(event: &CoreEvent) -> Vec<AgentId> {
    match event {
        CoreEvent::SessionCreated { state } => vec![state.agent_id.clone()],
        CoreEvent::AgentMessageRouted { message } => vec![message.from.clone(), message.to.clone()],
        CoreEvent::AgentMessageDeliveryObserved { receipt } => vec![
            receipt.request.from_agent_id.clone(),
            receipt.request.to_agent_id.clone(),
        ],
        CoreEvent::AgentRoundObserved { round } => vec![
            round.sender_agent_id.clone(),
            round.recipient_agent_id.clone(),
        ],
        CoreEvent::SessionArchived { .. }
        | CoreEvent::SessionWorkspaceChanged { .. }
        | CoreEvent::DelegationLifecycleObserved { .. }
        | CoreEvent::ExternalEventInjected { .. }
        | CoreEvent::DenDataUpdated { .. }
        | CoreEvent::BrainWakeRequested { .. }
        | CoreEvent::SessionExecutionObserved { .. }
        | CoreEvent::LogicalTurnLifecycleObserved { .. }
        | CoreEvent::BrainEventObserved { .. }
        | CoreEvent::BrainActionsAccepted { .. }
        | CoreEvent::CompletionPacketDelivered { .. } => Vec::new(),
    }
}

fn event_correlation_ids(event: &CoreEvent) -> Vec<String> {
    match event {
        CoreEvent::SessionCreated { state } => state
            .delegation
            .as_ref()
            .map(|lineage| lineage.correlation_id.clone())
            .into_iter()
            .collect(),
        CoreEvent::AgentMessageRouted { message } => {
            message.correlation_id.clone().into_iter().collect()
        }
        CoreEvent::AgentMessageDeliveryObserved { receipt } => {
            receipt.request.correlation_id.clone().into_iter().collect()
        }
        CoreEvent::AgentRoundObserved { round } => vec![round.correlation_id.clone()],
        CoreEvent::SessionArchived { .. }
        | CoreEvent::SessionWorkspaceChanged { .. }
        | CoreEvent::DelegationLifecycleObserved { .. }
        | CoreEvent::ExternalEventInjected { .. }
        | CoreEvent::DenDataUpdated { .. }
        | CoreEvent::BrainWakeRequested { .. }
        | CoreEvent::SessionExecutionObserved { .. }
        | CoreEvent::LogicalTurnLifecycleObserved { .. }
        | CoreEvent::BrainEventObserved { .. }
        | CoreEvent::BrainActionsAccepted { .. }
        | CoreEvent::CompletionPacketDelivered { .. } => Vec::new(),
    }
}

fn event_source_wake_ids(event: &CoreEvent) -> Vec<String> {
    match event {
        CoreEvent::SessionCreated { state } => state
            .delegation
            .as_ref()
            .map(|lineage| lineage.source_wake_id.clone())
            .into_iter()
            .collect(),
        CoreEvent::BrainEventObserved {
            wake_id: Some(wake_id),
            ..
        } => vec![wake_id.clone()],
        CoreEvent::SessionExecutionObserved { execution } => {
            execution.wake_id.clone().into_iter().collect()
        }
        CoreEvent::SessionArchived { .. }
        | CoreEvent::SessionWorkspaceChanged { .. }
        | CoreEvent::AgentMessageRouted { .. }
        | CoreEvent::AgentMessageDeliveryObserved { .. }
        | CoreEvent::AgentRoundObserved { .. }
        | CoreEvent::DelegationLifecycleObserved { .. }
        | CoreEvent::ExternalEventInjected { .. }
        | CoreEvent::DenDataUpdated { .. }
        | CoreEvent::BrainWakeRequested { .. }
        | CoreEvent::LogicalTurnLifecycleObserved { .. }
        | CoreEvent::BrainEventObserved { wake_id: None, .. }
        | CoreEvent::BrainActionsAccepted { .. }
        | CoreEvent::CompletionPacketDelivered { .. } => Vec::new(),
    }
}

fn save_event_search_rows_in_tx(
    tx: &rusqlite::Transaction<'_>,
    sequence: u64,
    event: &CoreEvent,
    recorded_at: &IsoTimestamp,
) -> CoreResult<()> {
    tx.execute(
        "DELETE FROM runtime_search_fts WHERE row_type = ?1 AND sequence = ?2",
        params!["message", sequence as i64],
    )
    .map_err(|error| persistence_error("delete event search rows", error))?;

    if let CoreEvent::AgentMessageRouted { message } = event {
        for agent_id in dedupe_non_empty(vec![message.from.0.clone(), message.to.0.clone()]) {
            insert_runtime_search_row(
                tx,
                &RuntimeSearchInsert {
                    row_type: RuntimeSearchRowType::Message,
                    row_key: format!("message:{sequence}:{agent_id}"),
                    sequence: Some(sequence),
                    session_id: None,
                    agent_id: Some(agent_id),
                    instance_id: None,
                    task_id: None,
                    event_kind: Some(CoreEventKind::AgentMessageRouted),
                    recorded_at: recorded_at.clone(),
                    title: "agent message".to_string(),
                    body: message.body.clone(),
                },
            )?;
        }
    }
    Ok(())
}

fn should_persist_event(event: &CoreEvent) -> bool {
    !matches!(
        event,
        CoreEvent::DenDataUpdated { .. } | CoreEvent::ExternalEventInjected { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn event_repo_indexes_queries_and_agent_messages() {
        let db_path = std::env::temp_dir().join(format!(
            "rusty-crew-event-repo-{}.sqlite3",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = CoordinationStore::open_file(&db_path).unwrap();

        store
            .save_event(
                1,
                &CoreEvent::BrainWakeRequested {
                    session_id: SessionId::new("session-alpha"),
                },
            )
            .unwrap();
        store
            .save_event(
                2,
                &CoreEvent::AgentMessageRouted {
                    message: AgentMessage {
                        from: AgentId::new("agent-alpha"),
                        to: AgentId::new("agent-beta"),
                        body: "event repository query message".to_string(),
                        correlation_id: Some("corr-events".to_string()),
                        projection: None,
                    },
                },
            )
            .unwrap();
        store
            .save_event(
                3,
                &CoreEvent::BrainEventObserved {
                    session_id: SessionId::new("session-alpha"),
                    wake_id: Some("wake-events".to_string()),
                    event: BrainEvent::Started,
                },
            )
            .unwrap();

        let by_session = store
            .query_events(&RuntimeEventFilter {
                session_id: Some(SessionId::new("session-alpha")),
                ..RuntimeEventFilter::default()
            })
            .unwrap();
        let by_agent = store
            .query_events(&RuntimeEventFilter {
                agent_id: Some(AgentId::new("agent-beta")),
                ..RuntimeEventFilter::default()
            })
            .unwrap();
        let by_wake = store
            .query_events(&RuntimeEventFilter {
                source_wake_id: Some("wake-events".to_string()),
                ..RuntimeEventFilter::default()
            })
            .unwrap();
        let messages = store
            .query_agent_messages(&AgentMessageQuery {
                agent_id: Some(AgentId::new("agent-alpha")),
                correlation_id: Some("corr-events".to_string()),
                page: None,
            })
            .unwrap();

        assert_eq!(by_session.len(), 2);
        assert_eq!(by_agent.len(), 1);
        assert_eq!(by_wake.len(), 1);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].sequence, 2);
        assert_eq!(messages[0].message.to, AgentId::new("agent-beta"));

        drop(store);
        let _ = std::fs::remove_file(db_path);
    }
}
