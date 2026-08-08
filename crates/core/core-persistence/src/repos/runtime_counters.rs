use super::super::*;

pub(crate) const COUNTER_BRAIN_TURNS: &str = "brain_turns";
pub(crate) const COUNTER_WAKES: &str = "wakes";
pub(crate) const COUNTER_TOOL_CALLS: &str = "tool_calls";
pub(crate) const COUNTER_TOOL_ERRORS: &str = "tool_errors";
pub(crate) const COUNTER_DELEGATIONS_CREATED: &str = "delegations_created";
pub(crate) const COUNTER_DELEGATIONS_COMPLETED: &str = "delegations_completed";
pub(crate) const COUNTER_DELEGATIONS_FAILED: &str = "delegations_failed";
pub(crate) const COUNTER_DELEGATIONS_TIMED_OUT: &str = "delegations_timed_out";
pub(crate) const COUNTER_DELEGATIONS_CANCELLED: &str = "delegations_cancelled";
pub(crate) const COUNTER_MESSAGES: &str = "messages";
pub(crate) const COUNTER_COMPLETIONS: &str = "completions";
pub(crate) const COUNTER_QUEUE_EXPIRATIONS: &str = "queue_expirations";

pub(crate) trait RuntimeCounterRepository {
    #[cfg(test)]
    fn record_runtime_counter_delta(
        &self,
        scope: &RuntimeCounterScope,
        counter_name: &str,
        amount: u64,
        now: &IsoTimestamp,
    ) -> CoreResult<()>;

    fn runtime_counters(
        &self,
        scope: Option<&RuntimeCounterScope>,
    ) -> CoreResult<Vec<RuntimeCounterRecord>>;

    fn query_runtime_counters(
        &self,
        query: &RuntimeCounterQuery,
    ) -> CoreResult<Vec<RuntimeCounterRecord>>;

    fn reset_runtime_counters(
        &self,
        query: &RuntimeCounterQuery,
        now: IsoTimestamp,
    ) -> CoreResult<u64>;

    fn runtime_summary(&self, scope: &RuntimeCounterScope) -> CoreResult<RuntimeStateSummary> {
        let counters = self.runtime_counters(Some(scope))?;
        Ok(runtime_summary_from_counters(scope, &counters))
    }
}

pub(crate) fn runtime_summary_from_counters(
    scope: &RuntimeCounterScope,
    counters: &[RuntimeCounterRecord],
) -> RuntimeStateSummary {
    RuntimeStateSummary {
        scope: scope.clone(),
        brain_turns: counter_value(counters, COUNTER_BRAIN_TURNS),
        wakes: counter_value(counters, COUNTER_WAKES),
        tool_calls: counter_value(counters, COUNTER_TOOL_CALLS),
        tool_errors: counter_value(counters, COUNTER_TOOL_ERRORS),
        delegations_created: counter_value(counters, COUNTER_DELEGATIONS_CREATED),
        delegations_completed: counter_value(counters, COUNTER_DELEGATIONS_COMPLETED),
        delegations_failed: counter_value(counters, COUNTER_DELEGATIONS_FAILED),
        delegations_timed_out: counter_value(counters, COUNTER_DELEGATIONS_TIMED_OUT),
        delegations_cancelled: counter_value(counters, COUNTER_DELEGATIONS_CANCELLED),
        messages: counter_value(counters, COUNTER_MESSAGES),
        completions: counter_value(counters, COUNTER_COMPLETIONS),
        queue_expirations: counter_value(counters, COUNTER_QUEUE_EXPIRATIONS),
    }
}

pub(crate) fn migrate_v7_add_runtime_counters(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS runtime_counters (
                scope_type TEXT NOT NULL,
                scope_id TEXT NOT NULL,
                counter_name TEXT NOT NULL,
                value INTEGER NOT NULL,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (scope_type, scope_id, counter_name)
            );
            CREATE INDEX IF NOT EXISTS idx_runtime_counters_scope
                ON runtime_counters(scope_type, scope_id);
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 7", error))
}

pub(crate) fn query_runtime_counters(
    conn: &Connection,
    query: &RuntimeCounterQuery,
) -> CoreResult<Vec<RuntimeCounterRecord>> {
    let scope_parts = query.scope.as_ref().map(runtime_counter_scope_parts);
    let scope_type = scope_parts.as_ref().map(|(scope_type, _)| *scope_type);
    let scope_id = scope_parts.as_ref().map(|(_, scope_id)| scope_id.as_str());
    let counter_name = query.counter_name.as_deref();
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(200, 5_000);
    let mut stmt = conn
        .prepare(
            "SELECT scope_type, scope_id, counter_name, value, updated_at
             FROM runtime_counters
             WHERE (?1 IS NULL OR scope_type = ?1)
               AND (?2 IS NULL OR scope_id = ?2)
               AND (?3 IS NULL OR counter_name = ?3)
             ORDER BY scope_type ASC, scope_id ASC, counter_name ASC
             LIMIT ?4 OFFSET ?5",
        )
        .map_err(|error| persistence_error("prepare query runtime counters", error))?;
    let rows = stmt
        .query_map(
            params![scope_type, scope_id, counter_name, limit, offset],
            row_to_runtime_counter,
        )
        .map_err(|error| persistence_error("query runtime counters", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load queried runtime counters", error))
}

pub(crate) fn reset_runtime_counters(
    conn: &Connection,
    query: &RuntimeCounterQuery,
    now: &IsoTimestamp,
) -> CoreResult<u64> {
    let scope_parts = query.scope.as_ref().map(runtime_counter_scope_parts);
    let scope_type = scope_parts.as_ref().map(|(scope_type, _)| *scope_type);
    let scope_id = scope_parts.as_ref().map(|(_, scope_id)| scope_id.as_str());
    let counter_name = query.counter_name.as_deref();
    let changed = conn
        .execute(
            "UPDATE runtime_counters
             SET value = 0, updated_at = ?4
             WHERE (?1 IS NULL OR scope_type = ?1)
               AND (?2 IS NULL OR scope_id = ?2)
               AND (?3 IS NULL OR counter_name = ?3)",
            params![scope_type, scope_id, counter_name, now],
        )
        .map_err(|error| persistence_error("reset runtime counters", error))?;
    Ok(changed as u64)
}

pub(crate) fn load_runtime_counters(
    conn: &Connection,
    scope: Option<&RuntimeCounterScope>,
) -> CoreResult<Vec<RuntimeCounterRecord>> {
    if let Some(scope) = scope {
        let (scope_type, scope_id) = runtime_counter_scope_parts(scope);
        let mut stmt = conn
            .prepare(
                "SELECT scope_type, scope_id, counter_name, value, updated_at
                 FROM runtime_counters
                 WHERE scope_type = ?1 AND scope_id = ?2
                 ORDER BY counter_name ASC",
            )
            .map_err(|error| persistence_error("prepare scoped runtime counters", error))?;
        let rows = stmt
            .query_map(params![scope_type, scope_id], row_to_runtime_counter)
            .map_err(|error| persistence_error("query scoped runtime counters", error))?;
        return rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| persistence_error("load scoped runtime counters", error));
    }

    let mut stmt = conn
        .prepare(
            "SELECT scope_type, scope_id, counter_name, value, updated_at
             FROM runtime_counters
             ORDER BY scope_type ASC, scope_id ASC, counter_name ASC",
        )
        .map_err(|error| persistence_error("prepare runtime counters", error))?;
    let rows = stmt
        .query_map([], row_to_runtime_counter)
        .map_err(|error| persistence_error("query runtime counters", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load runtime counters", error))
}

pub(crate) fn counter_value(counters: &[RuntimeCounterRecord], name: &str) -> u64 {
    counters
        .iter()
        .find(|counter| counter.counter_name == name)
        .map_or(0, |counter| counter.value)
}

#[cfg(test)]
pub(crate) fn record_runtime_counter_delta(
    conn: &mut Connection,
    scope: &RuntimeCounterScope,
    counter_name: &str,
    amount: u64,
    now: &IsoTimestamp,
) -> CoreResult<()> {
    if amount == 0 {
        return Ok(());
    }

    let tx = conn
        .transaction()
        .map_err(|error| persistence_error("begin runtime counter delta", error))?;
    increment_counter_in_tx(&tx, scope, counter_name, amount, Some(now))?;
    tx.commit()
        .map_err(|error| persistence_error("commit runtime counter delta", error))
}

pub(crate) fn increment_counter_for_scopes_in_tx(
    tx: &rusqlite::Transaction<'_>,
    scopes: Vec<RuntimeCounterScope>,
    counter_name: &str,
    amount: u64,
) -> CoreResult<()> {
    for scope in dedupe_counter_scopes(scopes) {
        increment_counter_in_tx(tx, &scope, counter_name, amount, None)?;
    }
    Ok(())
}

pub(crate) fn increment_event_counters_in_tx(
    tx: &rusqlite::Transaction<'_>,
    event: &CoreEvent,
) -> CoreResult<()> {
    for (counter_name, amount) in event_counter_deltas(event) {
        increment_counter_for_scopes_in_tx(tx, event_counter_scopes(event), counter_name, amount)?;
    }
    Ok(())
}

fn row_to_runtime_counter(row: &rusqlite::Row<'_>) -> rusqlite::Result<RuntimeCounterRecord> {
    let scope_type: String = row.get(0)?;
    let scope_id: String = row.get(1)?;
    Ok(RuntimeCounterRecord {
        scope: runtime_counter_scope_from_parts(&scope_type, &scope_id)?,
        counter_name: row.get(2)?,
        value: row.get::<_, i64>(3)? as u64,
        updated_at: row.get(4)?,
    })
}

fn increment_counter_in_tx(
    tx: &rusqlite::Transaction<'_>,
    scope: &RuntimeCounterScope,
    counter_name: &str,
    amount: u64,
    now: Option<&IsoTimestamp>,
) -> CoreResult<()> {
    if amount == 0 {
        return Ok(());
    }

    let (scope_type, scope_id) = runtime_counter_scope_parts(scope);
    match now {
        Some(now) => tx.execute(
            "INSERT INTO runtime_counters (
                scope_type,
                scope_id,
                counter_name,
                value,
                updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(scope_type, scope_id, counter_name) DO UPDATE SET
                value = value + excluded.value,
                updated_at = excluded.updated_at",
            params![scope_type, scope_id, counter_name, amount as i64, now],
        ),
        None => tx.execute(
            "INSERT INTO runtime_counters (
                scope_type,
                scope_id,
                counter_name,
                value
            ) VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(scope_type, scope_id, counter_name) DO UPDATE SET
                value = value + excluded.value,
                updated_at = CURRENT_TIMESTAMP",
            params![scope_type, scope_id, counter_name, amount as i64],
        ),
    }
    .map_err(|error| persistence_error("increment runtime counter", error))?;
    Ok(())
}

fn event_counter_deltas(event: &CoreEvent) -> Vec<(&'static str, u64)> {
    match event {
        CoreEvent::AgentMessageRouted { .. } => vec![(COUNTER_MESSAGES, 1)],
        CoreEvent::BrainWakeRequested { .. } => vec![(COUNTER_WAKES, 1)],
        CoreEvent::BrainActionsAccepted { count, .. } => {
            vec![
                (COUNTER_BRAIN_TURNS, 1),
                ("accepted_actions", *count as u64),
            ]
        }
        CoreEvent::BrainEventObserved { event, .. } => match event {
            BrainEvent::ToolCallStarted { .. } => vec![(COUNTER_TOOL_CALLS, 1)],
            BrainEvent::ToolCallFinished { is_error: true, .. } => vec![(COUNTER_TOOL_ERRORS, 1)],
            _ => Vec::new(),
        },
        CoreEvent::DelegationLifecycleObserved { lifecycle } => match lifecycle.phase {
            rusty_crew_core_protocol::DelegationLifecyclePhase::Created => {
                vec![(COUNTER_DELEGATIONS_CREATED, 1)]
            }
            rusty_crew_core_protocol::DelegationLifecyclePhase::Completed => {
                vec![(COUNTER_DELEGATIONS_COMPLETED, 1)]
            }
            rusty_crew_core_protocol::DelegationLifecyclePhase::Failed
            | rusty_crew_core_protocol::DelegationLifecyclePhase::Blocked
            | rusty_crew_core_protocol::DelegationLifecyclePhase::Exhausted => {
                vec![(COUNTER_DELEGATIONS_FAILED, 1)]
            }
            rusty_crew_core_protocol::DelegationLifecyclePhase::TimedOut => {
                vec![(COUNTER_DELEGATIONS_TIMED_OUT, 1)]
            }
            rusty_crew_core_protocol::DelegationLifecyclePhase::Cancelled => {
                vec![(COUNTER_DELEGATIONS_CANCELLED, 1)]
            }
            rusty_crew_core_protocol::DelegationLifecyclePhase::WakeRequested
            | rusty_crew_core_protocol::DelegationLifecyclePhase::CheckpointRequested => Vec::new(),
        },
        CoreEvent::CompletionPacketDelivered { .. } => vec![(COUNTER_COMPLETIONS, 1)],
        CoreEvent::SessionCreated { .. }
        | CoreEvent::SessionWorkspaceChanged { .. }
        | CoreEvent::SessionArchived { .. }
        | CoreEvent::AgentMessageDeliveryObserved { .. }
        | CoreEvent::AgentRoundObserved { .. }
        | CoreEvent::ExternalEventInjected { .. }
        | CoreEvent::DenDataUpdated { .. }
        | CoreEvent::SessionExecutionObserved { .. }
        | CoreEvent::LogicalTurnLifecycleObserved { .. } => Vec::new(),
    }
}

fn event_counter_scopes(event: &CoreEvent) -> Vec<RuntimeCounterScope> {
    let mut scopes = vec![RuntimeCounterScope::Runtime];
    scopes.extend(
        event_agent_ids(event)
            .into_iter()
            .map(RuntimeCounterScope::Agent),
    );
    let session_ids = event_session_ids(event);
    scopes.extend(
        session_ids
            .iter()
            .cloned()
            .map(RuntimeCounterScope::Session),
    );
    scopes.extend(session_ids.into_iter().map(|session_id| {
        RuntimeCounterScope::Instance(AgentInstanceId::new(format!("instance:{session_id}")))
    }));
    scopes
}

fn runtime_counter_scope_parts(scope: &RuntimeCounterScope) -> (&'static str, String) {
    match scope {
        RuntimeCounterScope::Runtime => ("runtime", "_global".to_string()),
        RuntimeCounterScope::Agent(agent_id) => ("agent", agent_id.0.clone()),
        RuntimeCounterScope::Instance(instance_id) => ("instance", instance_id.0.clone()),
        RuntimeCounterScope::Session(session_id) => ("session", session_id.0.clone()),
    }
}

fn runtime_counter_scope_from_parts(
    scope_type: &str,
    scope_id: &str,
) -> rusqlite::Result<RuntimeCounterScope> {
    match scope_type {
        "runtime" if scope_id == "_global" => Ok(RuntimeCounterScope::Runtime),
        "agent" => Ok(RuntimeCounterScope::Agent(AgentId::new(scope_id))),
        "instance" => Ok(RuntimeCounterScope::Instance(AgentInstanceId::new(
            scope_id,
        ))),
        "session" => Ok(RuntimeCounterScope::Session(SessionId::new(scope_id))),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown runtime counter scope {other}:{scope_id}"),
            )),
        )),
    }
}

fn dedupe_counter_scopes(scopes: Vec<RuntimeCounterScope>) -> Vec<RuntimeCounterScope> {
    let mut deduped = Vec::new();
    for scope in scopes {
        if deduped.contains(&scope) {
            continue;
        }
        deduped.push(scope);
    }
    deduped
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    pub(crate) fn runtime_counter_repository_conformance<R: RuntimeCounterRepository>(store: &R) {
        store
            .record_runtime_counter_delta(
                &RuntimeCounterScope::Runtime,
                COUNTER_MESSAGES,
                2,
                &"2026-07-05T00:00:00Z".to_string(),
            )
            .unwrap();
        store
            .record_runtime_counter_delta(
                &RuntimeCounterScope::Session(SessionId::new("session-alpha")),
                COUNTER_WAKES,
                1,
                &"2026-07-05T00:00:01Z".to_string(),
            )
            .unwrap();

        let runtime_summary = store
            .runtime_summary(&RuntimeCounterScope::Runtime)
            .unwrap();
        let session_summary = store
            .runtime_summary(&RuntimeCounterScope::Session(SessionId::new(
                "session-alpha",
            )))
            .unwrap();
        assert_eq!(runtime_summary.messages, 2);
        assert_eq!(session_summary.wakes, 1);

        let messages = store
            .query_runtime_counters(&RuntimeCounterQuery {
                scope: Some(RuntimeCounterScope::Runtime),
                counter_name: Some(COUNTER_MESSAGES.to_string()),
                page: None,
            })
            .unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].value, 2);

        assert_eq!(
            store
                .reset_runtime_counters(
                    &RuntimeCounterQuery {
                        scope: Some(RuntimeCounterScope::Runtime),
                        counter_name: Some(COUNTER_MESSAGES.to_string()),
                        page: None,
                    },
                    "2026-07-05T00:00:02Z".to_string(),
                )
                .unwrap(),
            1
        );
        assert_eq!(
            store
                .runtime_summary(&RuntimeCounterScope::Runtime)
                .unwrap()
                .messages,
            0
        );
        assert_eq!(
            store
                .runtime_summary(&RuntimeCounterScope::Session(SessionId::new(
                    "session-alpha"
                )))
                .unwrap()
                .wakes,
            1
        );
    }

    #[test]
    fn runtime_counter_repo_records_and_resets_event_projection() {
        let db_path = std::env::temp_dir().join(format!(
            "rusty-crew-runtime-counter-repo-{}.sqlite3",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = CoordinationStore::open_file(&db_path).unwrap();

        store
            .save_event(
                1,
                &CoreEvent::AgentMessageRouted {
                    message: AgentMessage {
                        from: AgentId::new("agent-alpha"),
                        to: AgentId::new("agent-beta"),
                        body: "module-owned runtime counter test".to_string(),
                        correlation_id: None,
                        projection: None,
                    },
                },
            )
            .unwrap();

        let runtime = store
            .runtime_summary(&RuntimeCounterScope::Runtime)
            .unwrap();
        assert_eq!(runtime.messages, 1);

        let reset = store
            .reset_runtime_counters(
                &RuntimeCounterQuery {
                    scope: Some(RuntimeCounterScope::Runtime),
                    counter_name: Some(COUNTER_MESSAGES.to_string()),
                    page: None,
                },
                "2026-07-02T00:00:00Z".to_string(),
            )
            .unwrap();
        let runtime = store
            .runtime_summary(&RuntimeCounterScope::Runtime)
            .unwrap();

        assert_eq!(reset, 1);
        assert_eq!(runtime.messages, 0);

        drop(store);
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn sqlite_runtime_counter_repository_matches_shared_contract() {
        let db_path = std::env::temp_dir().join(format!(
            "rusty-crew-runtime-counter-repository-contract-{}.sqlite3",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = CoordinationStore::open_file(&db_path).unwrap();

        runtime_counter_repository_conformance(&store);

        drop(store);
        let _ = std::fs::remove_file(db_path);
    }
}
