use super::*;
use rusty_crew_core_protocol::AgentMessageDeliveryRequest;

impl CoreEngine {
    pub fn project_body_state(&self, session_id: &SessionId) -> CoreResult<BodyState> {
        let mut state = self.body_projector.project(session_id)?;
        state.child_completions = delegated_completions_for_parent(&self.store, session_id)?;
        state.fan_out_groups = delegated_fan_out_groups_for_parent(&self.store, session_id)?;
        Ok(state)
    }

    pub fn prepare_body_state_for_wake(&self, session_id: &SessionId) -> CoreResult<BodyState> {
        self.validate_pending_direct_agent_messages_for_wake(session_id)?;
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
        self.enqueue_body_follow_up_message_with_wake(session_id, from, body, correlation_id, true)
    }

    pub(crate) fn enqueue_routed_agent_message_without_wake(
        &self,
        session_id: &SessionId,
        request: &AgentMessageDeliveryRequest,
        body: String,
    ) -> CoreResult<QueuedMessageRecord> {
        const MAX_ROUTED_MESSAGE_BYTES: usize = 256 * 1024;
        if body.len() > MAX_ROUTED_MESSAGE_BYTES {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "agent_message_body_too_large",
            ));
        }
        let session = self.sessions.get_session(session_id)?;
        if session.status == SessionStatus::Archived {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "agent_message_recipient_session_archived",
            ));
        }
        let state = self.body_projector.project(session_id)?;
        let pending_count = self
            .store
            .load_queued_messages(&QueuedMessageFilter {
                state: Some(QueuedMessageState::Pending),
                owner_session_id: Some(session_id.clone()),
                owner_agent_id: None,
                limit: None,
            })?
            .len();
        if pending_count >= state.delta_policy.max_queued_messages as usize {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "agent_message_serial_inbox_full",
            ));
        }
        let created_at = parse_rfc3339(&request.created_at)?;
        let expires_at = parse_rfc3339(&request.expires_at)?;
        let ttl_ms = (expires_at - created_at).whole_milliseconds();
        if ttl_ms <= 0 || ttl_ms > u32::MAX as i128 {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "agent_message_ttl_out_of_bounds",
            ));
        }
        let record = QueuedMessageRecord {
            message_id: format!("agent-message-queue:{}", request.message_id),
            owner_session_id: Some(session_id.clone()),
            owner_agent_id: session.agent_id.clone(),
            message: AgentMessage {
                from: request.from_agent_id.clone(),
                to: request.to_agent_id.clone(),
                from_session_id: request.from_session_id.clone(),
                to_session_id: request.to_session_id.clone(),
                body,
                correlation_id: request.correlation_id.clone(),
                projection: None,
            },
            source_sequence: None,
            enqueued_at: request.created_at.clone(),
            expires_at: request.expires_at.clone(),
            ttl_ms: ttl_ms as u32,
            delivery_attempts: 0,
            state: QueuedMessageState::Pending,
            terminal_at: None,
            state_reason: Some(format!("agent_delivery:{}", request.delivery_id.0)),
        };
        self.store.save_queued_message(&record)?;
        Ok(record)
    }

    fn enqueue_body_follow_up_message_with_wake(
        &self,
        session_id: &SessionId,
        from: AgentId,
        body: impl Into<String>,
        correlation_id: Option<String>,
        request_wake: bool,
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
                from_session_id: None,
                to_session_id: Some(session.session_id.clone()),
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
        if request_wake {
            self.bus.publish(CoreEvent::BrainWakeRequested {
                session_id: session_id.clone(),
            })?;
        }
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
