use super::*;
use rusty_crew_core_protocol::{
    AgentCoordinationCaller, AgentMessageCommand, AgentMessageDeliveryId,
    AgentMessageDeliveryStatus,
};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_LEGACY_MESSAGE_ID: AtomicU64 = AtomicU64::new(1);

impl CoreEngine {
    pub fn register_profile_tool_profile(
        &self,
        profile_id: ProfileId,
        tool_profile: ToolProfile,
    ) -> CoreResult<()> {
        validate_tool_profile(&tool_profile)?;
        self.profile_tool_profiles
            .lock()
            .map_err(|_| {
                CoreError::new(
                    CoreErrorKind::InternalError,
                    "profile registry lock poisoned",
                )
            })?
            .insert(profile_id, tool_profile);
        Ok(())
    }

    pub fn unregister_profile_tool_profile(&self, profile_id: &ProfileId) -> CoreResult<()> {
        self.profile_tool_profiles
            .lock()
            .map_err(|_| {
                CoreError::new(
                    CoreErrorKind::InternalError,
                    "profile registry lock poisoned",
                )
            })?
            .remove(profile_id);
        Ok(())
    }

    pub fn route_agent_message(&self, message: AgentMessage) -> CoreResult<EventReceipt> {
        let now = self.now();
        let expires_at = add_millis_to_iso(&now, 5_000)?;
        let key = format!(
            "{}-{}",
            sanitized_clock_key(&now),
            NEXT_LEGACY_MESSAGE_ID.fetch_add(1, Ordering::Relaxed)
        );
        let receipt = self.deliver_agent_message(AgentMessageCommand {
            caller: AgentCoordinationCaller::System {
                sender_agent_id: message.from,
            },
            delivery_id: AgentMessageDeliveryId::new(format!("delivery-{key}")),
            idempotency_key: format!("delivery-{key}"),
            message_id: format!("message-{key}"),
            to_agent_id: message.to,
            body: message.body,
            correlation_id: message.correlation_id,
            require_wake: true,
            created_at: now,
            expires_at,
        })?;
        Ok(EventReceipt {
            accepted: receipt.status == AgentMessageDeliveryStatus::Accepted,
            sequence: receipt.sequence.unwrap_or_default(),
        })
    }

    pub fn execute_brain_actions(&self, batch: BrainActionBatch) -> CoreResult<ActionBatchReceipt> {
        let session = self.sessions.get_session(&batch.session_id)?;
        if session.status == SessionStatus::Archived {
            return Err(CoreError::new(
                CoreErrorKind::SessionExpired,
                format!("session {} is archived", batch.session_id),
            ));
        }

        let rejected_actions = self.action_executor.validate(&batch);
        if !rejected_actions.is_empty() {
            return Ok(ActionBatchReceipt {
                wake_id: batch.wake_id,
                accepted_actions: 0,
                rejected_actions,
            });
        }

        let rejected_actions = self.validate_delegation_invariants(&session, &batch);
        if !rejected_actions.is_empty() {
            return Ok(ActionBatchReceipt {
                wake_id: batch.wake_id,
                accepted_actions: 0,
                rejected_actions,
            });
        }

        let rejected_actions = self.validate_fan_out_invariants(&batch);
        if !rejected_actions.is_empty() {
            return Ok(ActionBatchReceipt {
                wake_id: batch.wake_id,
                accepted_actions: 0,
                rejected_actions,
            });
        }

        self.spawn_delegated_workers(&session, &batch)?;
        let receipt = self.action_executor.execute(batch.clone())?;
        self.update_lifecycle_for_actions(&batch)?;
        self.apply_fan_out_failure_policy(&batch)?;
        self.schedule_parent_completion_wakes(&batch)?;
        Ok(receipt)
    }

    pub fn submit_brain_event(&self, envelope: BrainEventEnvelope) -> CoreResult<EventReceipt> {
        if matches!(envelope.event, BrainEvent::Started) {
            update_delegated_worker_run_status_by_session(
                &self.store,
                &envelope.session_id,
                WorkerRunStatus::Running,
                self.now(),
            )?;
        }
        let sequence = self.bus.publish(CoreEvent::BrainEventObserved {
            session_id: envelope.session_id,
            wake_id: Some(envelope.wake_id),
            event: envelope.event,
        })?;
        Ok(EventReceipt {
            accepted: true,
            sequence,
        })
    }

    pub fn inject_external_event(&self, event: ExternalEvent) -> CoreResult<EventReceipt> {
        let event = CoreEvent::ExternalEventInjected { event };
        let sequence = self.bus.publish(event.clone())?;
        self.schedule_wake_for_event(&event)?;
        Ok(EventReceipt {
            accepted: true,
            sequence,
        })
    }

    pub fn inject_den_data_update(&self, update: DenDataUpdate) -> CoreResult<EventReceipt> {
        let event = CoreEvent::DenDataUpdated { update };
        let sequence = self.bus.publish(event.clone())?;
        self.schedule_wake_for_event(&event)?;
        Ok(EventReceipt {
            accepted: true,
            sequence,
        })
    }
}

fn validate_tool_profile(tool_profile: &ToolProfile) -> CoreResult<()> {
    let mut names = HashSet::new();
    for tool in &tool_profile.tools {
        if tool.name.trim().is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "tool profile tool name must be non-empty",
            ));
        }
        if tool.description.trim().is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("tool profile tool {} requires a description", tool.name),
            ));
        }
        if !names.insert(tool.name.clone()) {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("tool profile contains duplicate tool {}", tool.name),
            ));
        }
    }
    Ok(())
}
