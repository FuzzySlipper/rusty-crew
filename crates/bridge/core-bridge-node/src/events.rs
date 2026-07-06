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
