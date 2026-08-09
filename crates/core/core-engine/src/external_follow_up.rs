//! Durable serial follow-up promotion for externally hosted agents.

use super::*;
use rusty_crew_core_protocol::{
    AgentActivation, AgentMessageDeliveryId, AgentMessageInputKind, ExternalAgentBinding,
    ExternalControllerContext, ExternalMessageDeliveryPolicy, ExternalTurnCorrelation,
    ExternalTurnInputPart, ExternalTurnPhase, ExternalTurnRequestId, TurnInputProvenance,
    TurnInputProvenanceKind,
};

impl CoreEngine {
    pub(crate) fn reconcile_idle_external_follow_ups(
        &self,
        now: &IsoTimestamp,
    ) -> CoreResult<Vec<ExternalTurnRequestId>> {
        let nonterminal_session_ids = self
            .store
            .list_nonterminal_external_turns()?
            .into_iter()
            .map(|turn| turn.request.session_id)
            .collect::<HashSet<_>>();
        let mut inspected_session_ids = HashSet::new();
        let mut promoted_request_ids = Vec::new();

        for binding in self
            .store
            .list_external_agent_bindings()?
            .into_iter()
            .filter(ExternalAgentBinding::is_routable)
            .filter(|binding| {
                binding.message_delivery_policy == ExternalMessageDeliveryPolicy::SerialNextTurn
            })
        {
            let session_id = binding.session_id.as_ref().expect("routable session id");
            if !inspected_session_ids.insert(session_id.clone())
                || nonterminal_session_ids.contains(session_id)
            {
                continue;
            }
            let has_pending_follow_up = self
                .store
                .load_queued_messages(&QueuedMessageFilter {
                    state: Some(QueuedMessageState::Pending),
                    owner_session_id: Some(session_id.clone()),
                    owner_agent_id: None,
                    limit: Some(1),
                })?
                .into_iter()
                .next()
                .is_some();
            if !has_pending_follow_up {
                continue;
            }

            let latest_turn = match binding.native_thread_id.as_deref() {
                Some(native_thread_id) => self
                    .store
                    .list_external_turns_for_native_thread(&binding.runtime_id, native_thread_id)?
                    .into_iter()
                    .rfind(|turn| turn.request.session_id == *session_id),
                None => None,
            };
            if let Some(latest_turn) = latest_turn.as_ref() {
                if !self.external_turn_allows_follow_up_promotion(latest_turn)? {
                    continue;
                }
            }
            if let Some(promoted) = self.promote_next_external_follow_up(session_id, now)? {
                promoted_request_ids.push(promoted.request.request_id);
            }
        }
        Ok(promoted_request_ids)
    }

    pub fn transition_external_turn(
        &self,
        request_id: &ExternalTurnRequestId,
        next_phase: ExternalTurnPhase,
        native_turn_id: Option<String>,
        terminal_reason_code: Option<String>,
        now: IsoTimestamp,
    ) -> CoreResult<ExternalTurnCorrelation> {
        self.transition_external_turn_with_error(
            request_id,
            next_phase,
            native_turn_id,
            terminal_reason_code,
            None,
            now,
        )
    }

    fn transition_external_turn_with_error(
        &self,
        request_id: &ExternalTurnRequestId,
        next_phase: ExternalTurnPhase,
        native_turn_id: Option<String>,
        terminal_reason_code: Option<String>,
        terminal_error: Option<rusty_crew_core_protocol::ExternalTurnTerminalError>,
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
        next.terminal_error = terminal_error;
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
        let source_delivery = turn
            .request
            .provenance
            .source_id
            .as_deref()
            .map(|source_id| {
                source_id
                    .strip_prefix("agent-message-queue:")
                    .unwrap_or(source_id)
            })
            .map(|message_id| {
                self.store
                    .get_agent_message_delivery_by_message_id(message_id)
            })
            .transpose()?
            .flatten();
        if source_delivery
            .as_ref()
            .is_some_and(|delivery| delivery.request.reply_to_message_id.is_some())
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
        if source_delivery
            .as_ref()
            .is_some_and(|delivery| delivery.request.from_session_id.is_none())
        {
            return Ok(true);
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
            .get_session(session_id)
            .map(|current| {
                current.agent_id != queued.message.to || current.status == SessionStatus::Archived
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
        let source_delivery = match queued
            .state_reason
            .as_deref()
            .and_then(|reason| reason.strip_prefix("agent_delivery:"))
        {
            Some(delivery_id) => {
                let delivery = self
                    .store
                    .get_agent_message_delivery(&AgentMessageDeliveryId::new(delivery_id))?
                    .ok_or_else(|| {
                        CoreError::new(
                            CoreErrorKind::PersistenceFailure,
                            format!(
                                "queued external follow-up references missing delivery {delivery_id}"
                            ),
                        )
                    })?;
                Some(delivery)
            }
            None => None,
        };
        let provenance_kind = match source_delivery
            .as_ref()
            .map(|delivery| delivery.request.input_kind)
        {
            Some(AgentMessageInputKind::Operator) => TurnInputProvenanceKind::Operator,
            Some(AgentMessageInputKind::RoutedAgentMessage) => {
                TurnInputProvenanceKind::RoutedAgentMessage
            }
            None => TurnInputProvenanceKind::ScheduledWake,
        };
        let resolved_target = source_delivery
            .as_ref()
            .and_then(|delivery| delivery.request.routing.as_deref())
            .map(|routing| &routing.resolved_target);
        let mut follow_up_input = vec![ExternalTurnInputPart::Text {
            text: queued.message.body.clone(),
        }];
        if let Some(delivery) = source_delivery.as_ref() {
            follow_up_input.extend(self.external_image_inputs(
                &queued.owner_session_id.clone().ok_or_else(|| {
                    CoreError::new(
                        CoreErrorKind::PersistenceFailure,
                        "queued external image message has no owner session",
                    )
                })?,
                &delivery.request.image_attachment_ids,
            )?);
        }
        let activation = self.activate_agent_execution_inner(
            AgentActivationRequest {
                agent_id: queued.message.to.clone(),
                request_id: request_id.clone(),
                idempotency_key: format!("external-follow-up:{}", queued.message_id),
                input: follow_up_input,
                collaboration_mode: None,
                provenance: TurnInputProvenance {
                    kind: provenance_kind,
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
            resolved_target,
        )?;
        if matches!(
            &activation,
            AgentActivation::Rejected { reason_code }
                if reason_code == "agent_route_activation_target_changed"
        ) {
            let mut cancelled = queued;
            cancelled.state = QueuedMessageState::Cancelled;
            cancelled.terminal_at = Some(now.clone());
            cancelled.state_reason = Some("agent_route_activation_target_changed".into());
            self.store.save_queued_message(&cancelled)?;
            return Ok(None);
        }
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
        transition: ExternalControllerTurnTransition,
    ) -> CoreResult<ExternalTurnCorrelation> {
        let current = self
            .store
            .get_external_turn(&transition.request_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    format!("external turn {} was not found", transition.request_id.0),
                )
            })?;
        self.validate_external_controller(&current.runtime_id, controller)?;
        self.transition_external_turn_with_error(
            &transition.request_id,
            transition.next_phase,
            transition.native_turn_id,
            transition.terminal_reason_code,
            transition.terminal_error,
            transition.now,
        )
    }
}
