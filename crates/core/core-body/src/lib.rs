//! Deterministic body-loop state and wake threshold evaluation.

use rusty_crew_core_bus::CoreBus;
use rusty_crew_core_protocol::{
    ActionBatchReceipt, ActionRejection, AgentMessage, BodyDeltaPolicy, BodyState, BrainAction,
    BrainActionBatch, CompletionPacket, CoreError, CoreErrorKind, CoreEvent, CoreResult,
    DeltaQueueOwner, EventReceipt, MidTurnDeltaMode, SessionId, SessionKind, SessionStatus,
};
use rusty_crew_core_session::SessionRegistry;

pub trait WakeThreshold {
    fn should_wake(&self, state: &BodyState, event: &CoreEvent) -> bool;
}

#[derive(Debug, Clone)]
pub struct BodyProjector {
    bus: CoreBus,
    sessions: SessionRegistry,
    recent_event_limit: usize,
}

impl BodyProjector {
    pub fn new(bus: CoreBus, sessions: SessionRegistry) -> Self {
        Self {
            bus,
            sessions,
            recent_event_limit: 32,
        }
    }

    pub fn project(&self, session_id: &SessionId) -> CoreResult<BodyState> {
        let session = self.sessions.get_session(session_id)?;
        let pending_messages = self.bus.pending_messages_for_agent(&session.agent_id)?;
        let recent_events = self
            .bus
            .recent_events_for_session(session_id, self.recent_event_limit)?;

        Ok(BodyState {
            session,
            pending_messages,
            recent_events,
            delta_policy: default_delta_policy(),
        })
    }
}

pub const fn default_delta_policy() -> BodyDeltaPolicy {
    BodyDeltaPolicy {
        mode: MidTurnDeltaMode::FrozenSnapshotNextWake,
        queue_owner: DeltaQueueOwner::Body,
        queued_message_ttl_ms: 5_000,
        max_queued_messages: 32,
    }
}

#[derive(Debug, Clone)]
pub struct BrainActionExecutor {
    bus: CoreBus,
    sessions: SessionRegistry,
}

impl BrainActionExecutor {
    pub fn new(bus: CoreBus, sessions: SessionRegistry) -> Self {
        Self { bus, sessions }
    }

    pub fn validate(&self, batch: &BrainActionBatch) -> Vec<ActionRejection> {
        batch
            .actions
            .iter()
            .enumerate()
            .filter_map(|(index, action)| {
                validate_action(&batch.session_id, action)
                    .err()
                    .map(|error| ActionRejection {
                        index: index as u32,
                        kind: error.kind,
                        message: error.message,
                    })
            })
            .collect()
    }

    pub fn execute(&self, batch: BrainActionBatch) -> CoreResult<ActionBatchReceipt> {
        let session = self.sessions.get_session(&batch.session_id)?;
        if session.status == SessionStatus::Archived {
            return Err(CoreError::new(
                CoreErrorKind::SessionExpired,
                format!("session {} is archived", batch.session_id),
            ));
        }

        let rejected_actions = self.validate(&batch);
        if !rejected_actions.is_empty() {
            return Ok(ActionBatchReceipt {
                wake_id: batch.wake_id,
                accepted_actions: 0,
                rejected_actions,
            });
        }

        for action in &batch.actions {
            match action {
                BrainAction::SendMessage { message } => {
                    self.publish_message(message.clone())?;
                }
                BrainAction::RequestDelegation {
                    profile_id: _,
                    task_id: _,
                    prompt: _,
                } => {}
                BrainAction::DeliverCompletion { packet } => {
                    self.publish_completion(packet.clone())?;
                }
            }
        }

        self.bus.publish(CoreEvent::BrainActionsAccepted {
            session_id: batch.session_id,
            count: batch.actions.len() as u32,
        })?;

        Ok(ActionBatchReceipt {
            wake_id: batch.wake_id,
            accepted_actions: batch.actions.len() as u32,
            rejected_actions: Vec::new(),
        })
    }

    fn publish_message(&self, message: AgentMessage) -> CoreResult<EventReceipt> {
        let sequence = self
            .bus
            .publish(CoreEvent::AgentMessageRouted { message })?;
        Ok(EventReceipt {
            accepted: true,
            sequence,
        })
    }

    fn publish_completion(&self, packet: CompletionPacket) -> CoreResult<EventReceipt> {
        let sequence = self
            .bus
            .publish(CoreEvent::CompletionPacketDelivered { packet })?;
        Ok(EventReceipt {
            accepted: true,
            sequence,
        })
    }
}

fn validate_action(batch_session_id: &SessionId, action: &BrainAction) -> CoreResult<()> {
    match action {
        BrainAction::SendMessage { message } => {
            if message.from.0.trim().is_empty() || message.to.0.trim().is_empty() {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "send_message requires non-empty from and to agents",
                ));
            }
            if message.body.trim().is_empty() {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "send_message requires a non-empty body",
                ));
            }
        }
        BrainAction::RequestDelegation {
            profile_id, prompt, ..
        } => {
            if profile_id.0.trim().is_empty() {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "request_delegation requires a profile_id",
                ));
            }
            if prompt.trim().is_empty() {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "request_delegation requires a prompt",
                ));
            }
        }
        BrainAction::DeliverCompletion { packet } => {
            if &packet.session_id != batch_session_id {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "completion packet session_id must match the action batch session_id",
                ));
            }
            if packet.session_id.0.trim().is_empty() {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "completion packet requires a session_id",
                ));
            }
            if packet.summary.trim().is_empty() {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "completion packet requires a summary",
                ));
            }
        }
    }
    Ok(())
}

pub fn session_kind_can_wake(kind: &SessionKind) -> bool {
    matches!(
        kind,
        SessionKind::Full | SessionKind::Worker | SessionKind::Delegated
    )
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DefaultWakeThreshold;

impl WakeThreshold for DefaultWakeThreshold {
    fn should_wake(&self, state: &BodyState, event: &CoreEvent) -> bool {
        if state.session.status == SessionStatus::Archived {
            return false;
        }

        matches!(
            event,
            CoreEvent::AgentMessageRouted { .. }
                | CoreEvent::DenDataUpdated { .. }
                | CoreEvent::ExternalEventInjected { .. }
        )
    }
}
