use super::*;

impl NativeBridge {
    pub fn wake_brain(&self, request: BrainWakeRequest) -> CoreResult<BrainWakeAccepted> {
        self.brain_registrations.get(request.brain)?;
        self.get_buffer(request.body_state)?;
        self.get_buffer(request.system_prompt)?;
        self.get_buffer(request.role_assembly)?;
        // Callback invocation is owned by the TS runtime wrapper, which binds a
        // BrainWakeExecutor to the registered handle. This Rust method only
        // validates the handle/buffer request shape until bridge codegen grows
        // a transport-neutral callback story.
        Err(not_implemented("wake_brain"))
    }

    pub fn submit_brain_event(&self, event: BrainEventEnvelope) -> CoreResult<EventReceipt> {
        self.engine()?.submit_brain_event(event)
    }

    pub fn submit_brain_actions(&self, batch: BrainActionBatch) -> CoreResult<ActionBatchReceipt> {
        self.engine()?.execute_brain_actions(batch)
    }

    pub fn route_agent_message(
        &self,
        from: rusty_crew_core_bridge_api::AgentId,
        to: rusty_crew_core_bridge_api::AgentId,
        body: String,
        correlation_id: Option<String>,
    ) -> CoreResult<EventReceipt> {
        self.engine()?
            .route_agent_message(rusty_crew_core_bridge_api::AgentMessage {
                from,
                to,
                from_session_id: None,
                to_session_id: None,
                body,
                correlation_id,
                projection: None,
            })
    }

    pub fn enqueue_body_follow_up_message(
        &self,
        session_id: SessionId,
        from: rusty_crew_core_bridge_api::AgentId,
        body: String,
        correlation_id: Option<String>,
    ) -> CoreResult<QueuedMessageRecord> {
        self.engine()?
            .enqueue_body_follow_up_message(&session_id, from, body, correlation_id)
    }

    pub fn suspend_for_github_gate(
        &self,
        request: rusty_crew_core_bridge_api::GitHubGateSuspendRequest,
    ) -> CoreResult<rusty_crew_core_bridge_api::GitHubGateWaitRecord> {
        self.engine()?.suspend_for_github_gate(request)
    }

    pub fn consume_github_gate_terminal_event(
        &self,
        event: rusty_crew_core_bridge_api::GitHubGateTerminalEvent,
    ) -> CoreResult<rusty_crew_core_bridge_api::GitHubGateTerminalReceipt> {
        self.engine()?.consume_github_gate_terminal_event(event)
    }

    pub fn recover_github_gate_wakes(&self) -> CoreResult<u32> {
        self.engine()?.recover_github_gate_wakes()
    }

    pub fn github_gate_wait(
        &self,
        session_id: SessionId,
    ) -> CoreResult<Option<rusty_crew_core_bridge_api::GitHubGateWaitRecord>> {
        self.engine()?.github_gate_wait(&session_id)
    }

    pub fn github_gate_event_cursor(&self) -> CoreResult<u64> {
        self.engine()?.github_gate_event_cursor()
    }

    pub fn begin_review_submission(
        &self,
        request: rusty_crew_core_bridge_api::ReviewSubmissionRequest,
    ) -> CoreResult<rusty_crew_core_bridge_api::ReviewSubmissionRecord> {
        self.engine()?.begin_review_submission(request)
    }

    pub fn transition_review_submission(
        &self,
        request: rusty_crew_core_bridge_api::ReviewSubmissionTransitionRequest,
    ) -> CoreResult<rusty_crew_core_bridge_api::ReviewSubmissionRecord> {
        self.engine()?.transition_review_submission(request)
    }

    pub fn list_review_submissions(
        &self,
        query: &rusty_crew_core_bridge_api::ReviewSubmissionQuery,
    ) -> CoreResult<Vec<rusty_crew_core_bridge_api::ReviewSubmissionRecord>> {
        self.engine()?.list_review_submissions(query)
    }

    pub fn register_platform_adapter(
        &mut self,
        registration: PlatformAdapterRegistration,
    ) -> CoreResult<PlatformAdapterHandle> {
        self.adapter_registrations.register(registration)
    }

    pub fn inject_external_event(&self, event: ExternalEvent) -> CoreResult<EventReceipt> {
        self.engine()?.inject_external_event(event)
    }

    pub fn inject_den_data_update(&self, update: DenDataUpdate) -> CoreResult<EventReceipt> {
        self.engine()?.inject_den_data_update(update)
    }

    pub fn subscribe_events(
        &mut self,
        subscription: EventSubscription,
    ) -> CoreResult<SubscriptionHandle> {
        let (bus_subscription_id, receiver) = self.engine()?.subscribe_events(subscription)?;
        Ok(self.subscriptions.insert(bus_subscription_id, receiver))
    }

    pub fn unsubscribe_events(&mut self, handle: SubscriptionHandle) -> CoreResult<Unit> {
        let record = self.subscriptions.remove(handle)?;
        self.engine()?
            .unsubscribe_events(record.bus_subscription_id)?;
        Ok(Unit)
    }

    pub fn drain_subscription_events(
        &self,
        handle: SubscriptionHandle,
        max_events: u32,
    ) -> CoreResult<Vec<CoreEvent>> {
        self.subscriptions.drain(handle, max_events)
    }
}

pub(crate) fn to_js_event_receipt(receipt: EventReceipt) -> JsEventReceipt {
    JsEventReceipt {
        accepted: receipt.accepted,
        sequence: receipt.sequence as f64,
    }
}

pub(crate) fn parse_tool_call_metadata(
    metadata_json: Option<&str>,
) -> napi::Result<Option<rusty_crew_core_bridge_api::ToolCallMetadata>> {
    metadata_json
        .map(serde_json::from_str::<rusty_crew_core_bridge_api::ToolCallMetadata>)
        .transpose()
        .map_err(|error| napi::Error::new(napi::Status::InvalidArg, error.to_string()))
}

pub(crate) fn to_js_queued_message_record(record: QueuedMessageRecord) -> JsQueuedMessageRecord {
    JsQueuedMessageRecord {
        message_id: record.message_id,
        owner_session_id: record.owner_session_id.map(|session_id| session_id.0),
        owner_agent_id: record.owner_agent_id.0,
        from_agent: record.message.from.0,
        to_agent: record.message.to.0,
        body: record.message.body,
        correlation_id: record.message.correlation_id,
        enqueued_at: record.enqueued_at,
        expires_at: record.expires_at,
        ttl_ms: record.ttl_ms,
        delivery_attempts: record.delivery_attempts,
        state: format!("{:?}", record.state).to_ascii_lowercase(),
        terminal_at: record.terminal_at,
        state_reason: record.state_reason,
    }
}

pub(crate) fn to_event_subscription(
    subscription: JsEventSubscription,
) -> napi::Result<EventSubscription> {
    Ok(EventSubscription {
        event_kinds: subscription
            .event_kinds
            .into_iter()
            .map(|kind| parse_event_kind(&kind))
            .collect::<napi::Result<Vec<_>>>()?,
        session_id: subscription
            .session_id
            .map(rusty_crew_core_bridge_api::SessionId::new),
        agent_id: subscription
            .agent_id
            .map(rusty_crew_core_bridge_api::AgentId::new),
        adapter_id: subscription
            .adapter_id
            .map(rusty_crew_core_bridge_api::AdapterId::new),
    })
}

pub(crate) fn to_platform_adapter_registration(
    registration: JsPlatformAdapterRegistration,
) -> napi::Result<PlatformAdapterRegistration> {
    Ok(PlatformAdapterRegistration {
        adapter_id: rusty_crew_core_bridge_api::AdapterId::new(registration.adapter_id),
        kind: parse_platform_adapter_kind(&registration.kind)?,
        display_name: registration.display_name,
    })
}

pub(crate) fn parse_platform_adapter_kind(
    raw: &str,
) -> napi::Result<rusty_crew_core_bridge_api::PlatformAdapterKind> {
    match raw {
        "den" => Ok(rusty_crew_core_bridge_api::PlatformAdapterKind::Den),
        "telegram" => Ok(rusty_crew_core_bridge_api::PlatformAdapterKind::Telegram),
        "mcp" => Ok(rusty_crew_core_bridge_api::PlatformAdapterKind::Mcp),
        "tui" => Ok(rusty_crew_core_bridge_api::PlatformAdapterKind::Tui),
        "cli" => Ok(rusty_crew_core_bridge_api::PlatformAdapterKind::Cli),
        other => Err(napi::Error::new(
            napi::Status::InvalidArg,
            format!("unsupported platform adapter kind {other}"),
        )),
    }
}

pub(crate) fn parse_event_kind(
    raw: &str,
) -> napi::Result<rusty_crew_core_bridge_api::CoreEventKind> {
    match raw {
        "session_created" => Ok(rusty_crew_core_bridge_api::CoreEventKind::SessionCreated),
        "session_archived" => Ok(rusty_crew_core_bridge_api::CoreEventKind::SessionArchived),
        "agent_message_routed" => Ok(rusty_crew_core_bridge_api::CoreEventKind::AgentMessageRouted),
        "delegation_lifecycle_observed" => {
            Ok(rusty_crew_core_bridge_api::CoreEventKind::DelegationLifecycleObserved)
        }
        "external_event_injected" => {
            Ok(rusty_crew_core_bridge_api::CoreEventKind::ExternalEventInjected)
        }
        "den_data_updated" => Ok(rusty_crew_core_bridge_api::CoreEventKind::DenDataUpdated),
        "brain_wake_requested" => Ok(rusty_crew_core_bridge_api::CoreEventKind::BrainWakeRequested),
        "session_execution_observed" => {
            Ok(rusty_crew_core_bridge_api::CoreEventKind::SessionExecutionObserved)
        }
        "logical_turn_lifecycle_observed" => {
            Ok(rusty_crew_core_bridge_api::CoreEventKind::LogicalTurnLifecycleObserved)
        }
        "brain_event_observed" => Ok(rusty_crew_core_bridge_api::CoreEventKind::BrainEventObserved),
        "brain_actions_accepted" => {
            Ok(rusty_crew_core_bridge_api::CoreEventKind::BrainActionsAccepted)
        }
        "completion_packet_delivered" => {
            Ok(rusty_crew_core_bridge_api::CoreEventKind::CompletionPacketDelivered)
        }
        other => Err(napi::Error::new(
            napi::Status::InvalidArg,
            format!("unsupported event kind {other}"),
        )),
    }
}
