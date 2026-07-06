//! PostgreSQL session, event, queue, tool-call, and completion repository methods.

use super::*;

impl PostgresBackendStore {
    pub fn save_session(&self, state: &SessionState) -> CoreResult<()> {
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start save PostgreSQL session", error))?;
        self.save_session_in_tx(&mut tx, state)?;
        tx.commit()
            .map_err(|error| postgres_error("commit save PostgreSQL session", error))?;
        Ok(())
    }

    pub fn save_session_with_config(
        &self,
        state: &SessionState,
        config: &SessionConfig,
    ) -> CoreResult<()> {
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start save PostgreSQL session with config", error))?;
        self.save_session_in_tx(&mut tx, state)?;
        self.save_session_config_in_tx(&mut tx, config, &state.created_at)?;
        tx.commit()
            .map_err(|error| postgres_error("commit save PostgreSQL session with config", error))?;
        Ok(())
    }

    pub fn load_sessions(&self) -> CoreResult<Vec<SessionState>> {
        let schema = self.quoted_schema();
        let rows = self
            .client()?
            .query(
                &format!("SELECT state_json FROM {schema}.sessions ORDER BY session_id ASC"),
                &[],
            )
            .map_err(|error| postgres_error("load PostgreSQL sessions", error))?;
        rows.into_iter()
            .map(|row| {
                let state_json: String = row.get(0);
                parse_postgres_json(&state_json, "session state_json")
            })
            .collect()
    }

    pub fn load_agent_identities(&self) -> CoreResult<Vec<DurableAgentRecord>> {
        self.load_json_records("agent_identities", "record_json", "agent_id")
    }

    pub fn load_agent_instances(
        &self,
    ) -> CoreResult<Vec<rusty_crew_core_protocol::AgentInstanceRecord>> {
        self.load_json_records("agent_instances", "record_json", "instance_id")
    }

    pub fn load_session_identities(&self) -> CoreResult<Vec<SessionIdentityRecord>> {
        self.load_json_records("session_identities", "record_json", "session_id")
    }

    pub fn load_session_configs(&self) -> CoreResult<Vec<SessionConfigRecord>> {
        let schema = self.quoted_schema();
        let rows = self
            .client()?
            .query(
                &format!(
                    "SELECT session_id, profile_id, kind, record_json, created_at
                     FROM {schema}.session_configs
                     ORDER BY session_id ASC"
                ),
                &[],
            )
            .map_err(|error| postgres_error("load PostgreSQL session configs", error))?;
        rows.into_iter()
            .map(|row| {
                let session_id: String = row.get(0);
                let profile_id: String = row.get(1);
                let config_json: String = row.get(3);
                let config: SessionConfig =
                    parse_postgres_json(&config_json, "session config_json")?;
                Ok(SessionConfigRecord {
                    session_id: SessionId(session_id),
                    profile_id: ProfileId(profile_id),
                    kind: config.kind.clone(),
                    resource_limits: config.resource_limits.clone(),
                    tool_profile: config.tool_profile.clone(),
                    config,
                    created_at: row.get(4),
                })
            })
            .collect()
    }

    pub fn save_event(&self, sequence: u64, event: &CoreEvent) -> CoreResult<()> {
        if !postgres_should_persist_event(event) {
            return Ok(());
        }
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start save PostgreSQL event", error))?;
        let schema = self.quoted_schema();
        let event_kind = format!("{:?}", CoreEventKind::of(event));
        let event_json = to_json_text(event)?;
        let is_new = tx
            .query_one(
                &format!(
                    "SELECT NOT EXISTS(
                        SELECT 1 FROM {schema}.event_history WHERE sequence = $1
                     )"
                ),
                &[&(sequence as i64)],
            )
            .map_err(|error| postgres_error("check PostgreSQL event existence", error))?
            .get::<_, bool>(0);
        tx.execute(
            &format!(
                "INSERT INTO {schema}.event_history (sequence, event_kind, event_json)
                 VALUES ($1, $2, $3)
                 ON CONFLICT(sequence) DO UPDATE SET
                    event_kind = EXCLUDED.event_kind,
                    event_json = EXCLUDED.event_json"
            ),
            &[&(sequence as i64), &event_kind, &event_json],
        )
        .map_err(|error| postgres_error("save PostgreSQL event history", error))?;
        self.replace_event_indexes_in_tx(&mut tx, sequence, event)?;
        if let CoreEvent::CompletionPacketDelivered { packet } = event {
            save_completion_packet_in_tx(&mut tx, &schema, sequence, packet)?;
        }
        if let CoreEvent::BrainEventObserved {
            session_id,
            wake_id,
            event,
        } = event
        {
            save_tool_call_in_tx(
                &mut tx,
                &schema,
                sequence,
                session_id,
                wake_id.as_deref(),
                event,
            )?;
        }
        if is_new {
            for scope in postgres_event_counter_scopes(event) {
                for (counter_name, amount) in postgres_event_counter_deltas(event) {
                    self.increment_counter_in_tx(&mut tx, &scope, counter_name, amount)?;
                }
            }
        }
        tx.commit()
            .map_err(|error| postgres_error("commit save PostgreSQL event", error))?;
        Ok(())
    }

    pub fn load_event_history(&self) -> CoreResult<Vec<PersistedEvent>> {
        let schema = self.quoted_schema();
        let rows = self
            .client()?
            .query(
                &format!(
                    "SELECT sequence, event_json
                     FROM {schema}.event_history
                     ORDER BY sequence ASC"
                ),
                &[],
            )
            .map_err(|error| postgres_error("load PostgreSQL event history", error))?;
        rows.into_iter()
            .map(|row| {
                let sequence: i64 = row.get(0);
                let event_json: String = row.get(1);
                Ok(PersistedEvent {
                    sequence: sequence as u64,
                    event: parse_postgres_json(&event_json, "event event_json")?,
                })
            })
            .collect()
    }

    pub fn query_events(&self, filter: &RuntimeEventFilter) -> CoreResult<Vec<RuntimeEventRecord>> {
        let schema = self.quoted_schema();
        let kind = filter.kind.as_ref().map(|kind| format!("{kind:?}"));
        let session_id = filter.session_id.as_ref().map(|value| value.0.as_str());
        let agent_id = filter.agent_id.as_ref().map(|value| value.0.as_str());
        let instance_id = filter.instance_id.as_ref().map(|value| value.0.as_str());
        let correlation_id = filter.correlation_id.as_deref();
        let source_wake_id = filter.source_wake_id.as_deref();
        let limit = filter.limit.unwrap_or(1_000).max(1) as i64;
        let rows = self
            .client()?
            .query(
                &format!(
                    "SELECT sequence, event_kind, recorded_at, event_json
                     FROM {schema}.event_history
                     WHERE ($1::TEXT IS NULL OR event_kind = $1)
                       AND ($2::TEXT IS NULL OR EXISTS (
                            SELECT 1 FROM {schema}.event_index
                            WHERE event_index.sequence = event_history.sequence
                              AND projection = 'session'
                              AND value = $2
                       ))
                       AND ($3::TEXT IS NULL OR EXISTS (
                            SELECT 1 FROM {schema}.event_index
                            WHERE event_index.sequence = event_history.sequence
                              AND projection = 'agent'
                              AND value = $3
                       ))
                       AND ($4::TEXT IS NULL OR EXISTS (
                            SELECT 1 FROM {schema}.event_index
                            WHERE event_index.sequence = event_history.sequence
                              AND projection = 'instance'
                              AND value = $4
                       ))
                       AND ($5::TEXT IS NULL OR EXISTS (
                            SELECT 1 FROM {schema}.event_index
                            WHERE event_index.sequence = event_history.sequence
                              AND projection = 'correlation'
                              AND value = $5
                       ))
                       AND ($6::TEXT IS NULL OR EXISTS (
                            SELECT 1 FROM {schema}.event_index
                            WHERE event_index.sequence = event_history.sequence
                              AND projection = 'wake'
                              AND value = $6
                       ))
                     ORDER BY sequence ASC
                     LIMIT $7"
                ),
                &[
                    &kind,
                    &session_id,
                    &agent_id,
                    &instance_id,
                    &correlation_id,
                    &source_wake_id,
                    &limit,
                ],
            )
            .map_err(|error| postgres_error("query PostgreSQL events", error))?;
        rows.into_iter()
            .map(|row| self.row_to_event_record(row))
            .collect()
    }

    pub fn save_queued_message(&self, record: &QueuedMessageRecord) -> CoreResult<()> {
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start save PostgreSQL queued message", error))?;
        self.save_queued_message_in_tx(&mut tx, record)?;
        tx.commit()
            .map_err(|error| postgres_error("commit save PostgreSQL queued message", error))?;
        Ok(())
    }

    pub fn expire_queued_messages_at(
        &self,
        now: &IsoTimestamp,
    ) -> CoreResult<Vec<QueuedMessageRecord>> {
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start expire PostgreSQL queued messages", error))?;
        let expired = self.expire_queued_messages_in_tx(&mut tx, now)?;
        tx.commit()
            .map_err(|error| postgres_error("commit expire PostgreSQL queued messages", error))?;
        Ok(expired)
    }

    pub fn load_queued_messages(
        &self,
        filter: &QueuedMessageFilter,
    ) -> CoreResult<Vec<QueuedMessageRecord>> {
        let mut client = self.client()?;
        self.load_queued_messages_in_tx(&mut *client, filter)
    }

    pub fn load_tool_call_history(&self) -> CoreResult<Vec<ToolCallRecord>> {
        let schema = self.quoted_schema();
        let rows = self
            .client()?
            .query(
                &format!(
                    "SELECT sequence,
                            session_id,
                            wake_id,
                            tool_name,
                            phase,
                            is_error,
                            metadata_json
                     FROM {schema}.tool_call_history
                     ORDER BY sequence ASC"
                ),
                &[],
            )
            .map_err(|error| postgres_error("load PostgreSQL tool call history", error))?;
        rows.iter().map(row_to_tool_call_record).collect()
    }

    pub fn query_completion_packets(
        &self,
        query: &CompletionPacketQuery,
    ) -> CoreResult<Vec<CompletionPacketRecord>> {
        let schema = self.quoted_schema();
        let session_id = query.session_id.as_ref().map(|value| value.0.as_str());
        let status = query
            .status
            .as_ref()
            .map(postgres_completion_status_as_str)
            .map(str::to_string);
        let (limit, offset) = query
            .page
            .unwrap_or(QueryPage {
                limit: None,
                offset: None,
            })
            .bounded(100, 1_000);
        let rows = self
            .client()?
            .query(
                &format!(
                    "SELECT sequence, packet_json
                     FROM {schema}.completion_packets
                     WHERE ($1::TEXT IS NULL OR session_id = $1)
                       AND ($2::TEXT IS NULL OR status = $2)
                     ORDER BY sequence ASC
                     LIMIT $3 OFFSET $4"
                ),
                &[&session_id, &status, &limit, &offset],
            )
            .map_err(|error| postgres_error("query PostgreSQL completion packets", error))?;
        rows.iter().map(row_to_completion_packet_record).collect()
    }
}
