//! Durable serial follow-up promotion for externally hosted agents.

use super::*;
use rusty_crew_core_protocol::{
    AgentActivation, ExternalControllerContext, ExternalMessageDeliveryPolicy,
    ExternalTurnCorrelation, ExternalTurnInputPart, ExternalTurnPhase, ExternalTurnRequestId,
    TurnInputProvenance,
};

impl CoreEngine {
    pub fn transition_external_turn(
        &self,
        request_id: &ExternalTurnRequestId,
        next_phase: ExternalTurnPhase,
        native_turn_id: Option<String>,
        terminal_reason_code: Option<String>,
        now: IsoTimestamp,
    ) -> CoreResult<ExternalTurnCorrelation> {
        let current = self.store.get_external_turn(request_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("external turn {} was not found", request_id.0),
            )
        })?;
        let mut next = current.clone();
        next.phase = next_phase;
        if let Some(native_turn_id) = native_turn_id {
            next.native_turn_id = Some(native_turn_id);
        }
        next.terminal_reason_code = terminal_reason_code;
        next.updated_at = now;
        let became_terminal = !current.phase.is_terminal() && next.phase.is_terminal();
        if next.phase.is_terminal() {
            next.capacity_lease_id = None;
        }
        let saved = self.store.update_external_turn(&next, current.revision)?;
        if became_terminal && self.external_turn_allows_follow_up_promotion(&saved)? {
            self.promote_next_external_follow_up(&saved.request.session_id, &saved.updated_at)?;
        }
        Ok(saved)
    }

    fn external_turn_allows_follow_up_promotion(
        &self,
        turn: &ExternalTurnCorrelation,
    ) -> CoreResult<bool> {
        if turn.request.provenance.kind
            != rusty_crew_core_protocol::TurnInputProvenanceKind::RoutedAgentMessage
        {
            return Ok(true);
        }
        let binding = self
            .store
            .get_external_agent_binding(&turn.request.binding_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "serial review turn binding was not found",
                )
            })?;
        if binding.message_delivery_policy != ExternalMessageDeliveryPolicy::SerialNextTurn {
            return Ok(true);
        }
        if matches!(
            turn.phase,
            ExternalTurnPhase::Failed
                | ExternalTurnPhase::Interrupted
                | ExternalTurnPhase::OutcomeUnknown
        ) {
            return Ok(true);
        }
        if turn.phase != ExternalTurnPhase::Completed {
            return Ok(false);
        }
        if matches!(
            turn.terminal_reason_code.as_deref(),
            Some("review_no_reply" | "agent_message_no_reply")
        ) {
            return Ok(true);
        }
        let Some(source_id) = turn.request.provenance.source_id.as_deref() else {
            return Ok(false);
        };
        let original_message_id = source_id
            .strip_prefix("agent-message-queue:")
            .unwrap_or(source_id);
        Ok(self
            .store
            .get_agent_message_reply(original_message_id)?
            .is_some())
    }

    fn promote_next_external_follow_up(
        &self,
        session_id: &SessionId,
        now: &IsoTimestamp,
    ) -> CoreResult<Option<ExternalTurnCorrelation>> {
        let _promotion_guard = self.external_follow_up_lock.lock().map_err(|_| {
            CoreError::new(
                CoreErrorKind::InternalError,
                "external follow-up promotion lock poisoned",
            )
        })?;
        self.store.expire_queued_messages_at(now)?;
        let Some(queued) = self
            .store
            .load_queued_messages(&QueuedMessageFilter {
                state: Some(QueuedMessageState::Pending),
                owner_session_id: Some(session_id.clone()),
                owner_agent_id: None,
                limit: Some(1),
            })?
            .into_iter()
            .next()
        else {
            return Ok(None);
        };
        let recipient_session_changed = self
            .sessions
            .get_session_by_agent(&queued.message.to)
            .map(|current| {
                current.session_id != *session_id || current.status == SessionStatus::Archived
            })
            .unwrap_or(true);
        if recipient_session_changed {
            for mut pending in self.store.load_queued_messages(&QueuedMessageFilter {
                state: Some(QueuedMessageState::Pending),
                owner_session_id: Some(session_id.clone()),
                owner_agent_id: None,
                limit: None,
            })? {
                pending.state = QueuedMessageState::Cancelled;
                pending.terminal_at = Some(now.clone());
                pending.state_reason = Some("agent_message_recipient_session_changed".into());
                self.store.save_queued_message(&pending)?;
            }
            return Ok(None);
        }
        let request_id =
            ExternalTurnRequestId::new(format!("external-follow-up:{}", queued.message_id));
        let activation = self.activate_agent_execution_inner(
            AgentActivationRequest {
                agent_id: queued.message.to.clone(),
                request_id: request_id.clone(),
                idempotency_key: format!("external-follow-up:{}", queued.message_id),
                input: vec![ExternalTurnInputPart::Text {
                    text: queued.message.body.clone(),
                }],
                collaboration_mode: None,
                provenance: TurnInputProvenance {
                    kind: rusty_crew_core_protocol::TurnInputProvenanceKind::RoutedAgentMessage,
                    source_id: Some(queued.message_id.clone()),
                    correlation_id: queued.message.correlation_id.clone(),
                },
                run_id: None,
                capacity_lease_id: format!("external-follow-up-capacity:{}", queued.message_id),
                direct_wake_id: format!("external-follow-up-wake:{}", queued.message_id),
                queued_message_id: format!("external-follow-up-queue:{}", queued.message_id),
                created_at: now.clone(),
                expires_at: Some(queued.expires_at.clone()),
            },
            Some((&queued.message_id, now)),
        )?;
        if !matches!(activation, AgentActivation::ExternalTurnRequested { .. }) {
            return Ok(None);
        }
        let turn = self.store.get_external_turn(&request_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::InternalError,
                format!("promoted external turn {} was not persisted", request_id.0),
            )
        })?;
        Ok(Some(turn))
    }

    pub fn transition_external_turn_from_controller(
        &self,
        controller: &ExternalControllerContext,
        request_id: &ExternalTurnRequestId,
        next_phase: ExternalTurnPhase,
        native_turn_id: Option<String>,
        terminal_reason_code: Option<String>,
        now: IsoTimestamp,
    ) -> CoreResult<ExternalTurnCorrelation> {
        let current = self.store.get_external_turn(request_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("external turn {} was not found", request_id.0),
            )
        })?;
        self.validate_external_controller(&current.runtime_id, controller)?;
        self.transition_external_turn(
            request_id,
            next_phase,
            native_turn_id,
            terminal_reason_code,
            now,
        )
    }
}
