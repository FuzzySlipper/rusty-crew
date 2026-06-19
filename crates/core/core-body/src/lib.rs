//! Deterministic body-loop state and wake threshold evaluation.

use rusty_crew_core_protocol::{BodyState, CoreEvent, SessionStatus};

pub trait WakeThreshold {
    fn should_wake(&self, state: &BodyState, event: &CoreEvent) -> bool;
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
