use super::*;

impl CoreEngine {
    pub fn project_body_state(&self, session_id: &SessionId) -> CoreResult<BodyState> {
        let mut state = self.body_projector.project(session_id)?;
        state.child_completions = delegated_completions_for_parent(&self.store, session_id)?;
        state.fan_out_groups = delegated_fan_out_groups_for_parent(&self.store, session_id)?;
        Ok(state)
    }

    pub fn prepare_body_state_for_wake(&self, session_id: &SessionId) -> CoreResult<BodyState> {
        let mut state = self.project_body_state(session_id)?;
        let queued_capacity = state
            .session
            .history_window
            .as_ref()
            .and_then(|window| window.max_messages)
            .map(|max_messages| max_messages.saturating_sub(state.pending_messages.len() as u32));
        let queued = self.drain_body_follow_up_messages_for_wake(session_id, queued_capacity)?;
        state
            .pending_messages
            .extend(queued.into_iter().map(|record| record.message));
        Ok(state)
    }

    pub fn enqueue_body_follow_up_message(
        &self,
        session_id: &SessionId,
        from: AgentId,
        body: impl Into<String>,
        correlation_id: Option<String>,
    ) -> CoreResult<QueuedMessageRecord> {
        let session = self.sessions.get_session(session_id)?;
        if !session_kind_can_wake(&session.kind) || session.status == SessionStatus::Archived {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!(
                    "session {} cannot receive follow-up wakes",
                    session.session_id
                ),
            ));
        }
        let state = self.body_projector.project(session_id)?;
        let ttl_ms = state.delta_policy.queued_message_ttl_ms;
        let now = self.now();
        let expires_at = add_millis_to_iso(&now, ttl_ms as u64)?;
        let record = QueuedMessageRecord {
            message_id: next_queued_message_id(session_id, &now),
            owner_session_id: Some(session_id.clone()),
            owner_agent_id: session.agent_id.clone(),
            message: AgentMessage {
                from,
                to: session.agent_id.clone(),
                body: body.into(),
                correlation_id,
                projection: None,
            },
            source_sequence: None,
            enqueued_at: now.clone(),
            expires_at,
            ttl_ms,
            delivery_attempts: 0,
            state: QueuedMessageState::Pending,
            terminal_at: None,
            state_reason: None,
        };
        save_body_follow_up_message(&self.store, &record)?;
        self.enforce_body_follow_up_cap(session_id, state.delta_policy.max_queued_messages)?;
        self.bus.publish(CoreEvent::BrainWakeRequested {
            session_id: session_id.clone(),
        })?;
        Ok(record)
    }

    pub(crate) fn expire_body_follow_up_messages(
        &self,
        now: &IsoTimestamp,
    ) -> CoreResult<Vec<QueuedMessageRecord>> {
        body_queue::BodyQueueStore::expire_body_follow_up_messages_at(&self.store, now)
    }

    pub(crate) fn drain_body_follow_up_messages_for_wake(
        &self,
        session_id: &SessionId,
        max_delivered_messages: Option<u32>,
    ) -> CoreResult<Vec<QueuedMessageRecord>> {
        let now = self.now();
        drain_follow_up_queue_for_wake(&self.store, &now, session_id, max_delivered_messages)
    }

    pub(crate) fn enforce_body_follow_up_cap(
        &self,
        session_id: &SessionId,
        max_queued_messages: u32,
    ) -> CoreResult<()> {
        let now = self.now();
        enforce_follow_up_queue_cap(&self.store, &now, session_id, max_queued_messages)
    }
}

pub(crate) fn add_millis_to_iso(at: &IsoTimestamp, millis: u64) -> CoreResult<IsoTimestamp> {
    let parsed = OffsetDateTime::parse(at, &Rfc3339).map_err(|error| {
        CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("invalid scheduler timestamp {at}: {error}"),
        )
    })?;
    let millis = i64::try_from(millis).map_err(|_| {
        CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("scheduler interval {millis}ms is too large"),
        )
    })?;
    (parsed + Duration::milliseconds(millis))
        .format(&Rfc3339)
        .map_err(|error| {
            CoreError::new(
                CoreErrorKind::InternalError,
                format!("format scheduler timestamp: {error}"),
            )
        })
}

fn next_queued_message_id(session_id: &SessionId, now: &IsoTimestamp) -> String {
    let sequence = NEXT_QUEUED_MESSAGE.fetch_add(1, Ordering::Relaxed);
    format!(
        "follow-up:{session_id}:{}:{sequence}",
        sanitized_clock_key(now)
    )
}

pub(crate) fn sanitized_clock_key(now: &IsoTimestamp) -> String {
    now.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect()
}
