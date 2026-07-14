//! Runtime-neutral direct-agent messaging and durable correlated rounds.

use super::*;
use rusty_crew_core_protocol::{
    AgentActivation, AgentCoordinationCaller, AgentCorrelatedRound, AgentDirectoryEntry,
    AgentDirectoryRuntimeKind, AgentMessageCommand, AgentMessageDeliveryReceipt,
    AgentMessageDeliveryRequest, AgentMessageDeliveryStatus, AgentRoundCommand,
    AgentRoundStartReceipt, AgentRoundStatus, ExternalBindingPurpose, ExternalBindingStatus,
    ExternalRuntimeDesiredState, ExternalRuntimeKind, ExternalRuntimeObservedState,
    ExternalTurnInputPart, ExternalTurnRequestId, TurnInputProvenance, TurnInputProvenanceKind,
};
use serde_json::json;

impl CoreEngine {
    pub fn list_agent_directory(&self) -> CoreResult<Vec<AgentDirectoryEntry>> {
        let profiles = self
            .list_profile_registry_records(&ProfileRegistryQuery::default())?
            .into_iter()
            .map(|profile| (profile.profile_id.clone(), profile))
            .collect::<HashMap<_, _>>();
        let bindings = self.store.list_external_agent_bindings()?;
        let runtimes = self
            .store
            .list_external_runtime_registrations()?
            .into_iter()
            .map(|runtime| (runtime.runtime_id.clone(), runtime))
            .collect::<HashMap<_, _>>();
        let mut entries = Vec::new();

        for session in self
            .sessions
            .all_sessions()?
            .into_iter()
            .filter(|session| session.status != SessionStatus::Archived)
        {
            let profile = profiles.get(&session.profile_id);
            let binding = bindings
                .iter()
                .filter(|binding| {
                    binding.purpose == ExternalBindingPurpose::CrewAgent
                        && binding.agent_id.as_ref() == Some(&session.agent_id)
                        && binding.session_id.as_ref() == Some(&session.session_id)
                })
                .max_by_key(|binding| match binding.status {
                    ExternalBindingStatus::Active => 2,
                    ExternalBindingStatus::Paused => 1,
                    ExternalBindingStatus::Archived => 0,
                });

            let (
                runtime_kind,
                runtime_id,
                binding_id,
                binding_status,
                task_ref,
                workdir,
                routable,
                reason_code,
            ) = if let Some(binding) = binding {
                let runtime = runtimes.get(&binding.runtime_id).ok_or_else(|| {
                    CoreError::new(
                        CoreErrorKind::PersistenceFailure,
                        format!(
                            "external binding {} references missing runtime {}",
                            binding.binding_id.0, binding.runtime_id.0
                        ),
                    )
                })?;
                let runtime_kind = match runtime.kind {
                    ExternalRuntimeKind::CodexAppServer => {
                        AgentDirectoryRuntimeKind::CodexAppServer
                    }
                };
                let reason_code = if binding.status != ExternalBindingStatus::Active {
                    Some("external_binding_not_active".to_string())
                } else if runtime.desired_state != ExternalRuntimeDesiredState::Enabled {
                    Some("external_runtime_disabled".to_string())
                } else if runtime.observed_state != ExternalRuntimeObservedState::Ready {
                    Some("external_runtime_not_ready".to_string())
                } else {
                    None
                };
                (
                    runtime_kind,
                    Some(binding.runtime_id.clone()),
                    Some(binding.binding_id.clone()),
                    Some(binding.status),
                    binding.task_ref.clone(),
                    binding.cwd.clone(),
                    reason_code.is_none(),
                    reason_code,
                )
            } else {
                (
                    AgentDirectoryRuntimeKind::DirectBrain,
                    None,
                    None,
                    None,
                    None,
                    session.resource_limits.workdir.clone(),
                    true,
                    None,
                )
            };

            entries.push(AgentDirectoryEntry {
                agent_id: session.agent_id,
                session_id: session.session_id,
                profile_id: session.profile_id.clone(),
                display_label: profile
                    .and_then(|profile| profile.display_name.clone())
                    .unwrap_or(session.profile_id.0),
                session_kind: session.kind,
                session_status: session.status,
                runtime_kind,
                runtime_id,
                binding_id,
                binding_status,
                task_ref,
                workdir,
                routable,
                routability_reason_code: reason_code,
            });
        }
        entries.sort_by(|left, right| {
            left.display_label
                .cmp(&right.display_label)
                .then_with(|| left.agent_id.0.cmp(&right.agent_id.0))
        });
        Ok(entries)
    }

    pub fn deliver_agent_message(
        &self,
        command: AgentMessageCommand,
    ) -> CoreResult<AgentMessageDeliveryReceipt> {
        let (sender_agent_id, _sender_session_id, sender_request_id) =
            self.resolve_coordination_caller(&command.caller)?;
        let request = AgentMessageDeliveryRequest {
            delivery_id: command.delivery_id,
            idempotency_key: command.idempotency_key,
            message_id: command.message_id.clone(),
            from_agent_id: sender_agent_id.clone(),
            to_agent_id: command.to_agent_id.clone(),
            body: command.body.clone(),
            collaboration_mode: command.collaboration_mode,
            correlation_id: command.correlation_id.clone(),
            require_wake: command.require_wake,
            created_at: command.created_at.clone(),
            expires_at: command.expires_at.clone(),
        };
        let pending = AgentMessageDeliveryReceipt {
            request,
            status: AgentMessageDeliveryStatus::Pending,
            sequence: None,
            activation: None,
            resolved_round_id: None,
            reason_code: None,
            terminal_at: None,
            revision: 1,
        };
        let pending = self.store.create_agent_message_delivery(&pending)?;
        if pending.status.is_terminal() {
            return Ok(pending);
        }
        if pending.activation.is_some() {
            return Ok(pending);
        }
        if command.expires_at <= self.now() {
            return self.finish_agent_message_delivery(
                pending,
                AgentMessageDeliveryStatus::Expired,
                None,
                None,
                None,
                Some("agent_message_expired".into()),
            );
        }
        let message = AgentMessage {
            from: sender_agent_id.clone(),
            to: command.to_agent_id.clone(),
            body: command.body.clone(),
            correlation_id: command.correlation_id.clone(),
            projection: None,
        };
        if let Some(round) = self.matching_agent_round(&message)? {
            let sequence = self.bus.publish(CoreEvent::AgentMessageRouted {
                message: message.clone(),
            })?;
            let round = self.resolve_agent_round_reply(
                round,
                &message,
                &command.message_id,
                &command.created_at,
            )?;
            return self.finish_agent_message_delivery(
                pending,
                AgentMessageDeliveryStatus::Accepted,
                Some(sequence),
                None,
                Some(round.round_id),
                None,
            );
        }
        let session = match self.sessions.get_session_by_agent(&command.to_agent_id) {
            Ok(session) if session.status != SessionStatus::Archived => session,
            Ok(_) => {
                return self.finish_agent_message_delivery(
                    pending,
                    AgentMessageDeliveryStatus::Rejected,
                    None,
                    None,
                    None,
                    Some("recipient_session_archived".into()),
                )
            }
            Err(error) if error.kind == CoreErrorKind::NotFound => {
                return self.finish_agent_message_delivery(
                    pending,
                    AgentMessageDeliveryStatus::Rejected,
                    None,
                    None,
                    None,
                    Some("recipient_not_found".into()),
                )
            }
            Err(error) => return Err(error),
        };
        if command.collaboration_mode.is_some()
            && self
                .store
                .list_nonterminal_external_turns()?
                .iter()
                .any(|turn| turn.request.session_id == session.session_id)
        {
            return self.finish_agent_message_delivery(
                pending,
                AgentMessageDeliveryStatus::Rejected,
                None,
                None,
                None,
                Some("external_collaboration_mode_turn_already_active".into()),
            );
        }

        let event = CoreEvent::AgentMessageRouted {
            message: message.clone(),
        };
        let sequence = self.bus.publish(event)?;

        if !command.require_wake {
            return self.finish_agent_message_delivery(
                pending,
                AgentMessageDeliveryStatus::Accepted,
                Some(sequence),
                None,
                None,
                None,
            );
        }

        let model_body = routed_agent_message_text(&pending.request);
        let activation = self.activate_agent_execution(AgentActivationRequest {
            agent_id: command.to_agent_id,
            request_id: ExternalTurnRequestId::new(format!("agent-message:{}", command.message_id)),
            idempotency_key: format!("agent-message-turn:{}", command.message_id),
            input: vec![ExternalTurnInputPart::Text {
                text: model_body.clone(),
            }],
            collaboration_mode: command.collaboration_mode,
            provenance: TurnInputProvenance {
                kind: TurnInputProvenanceKind::RoutedAgentMessage,
                source_id: Some(command.message_id.clone()),
                correlation_id: command.correlation_id,
            },
            run_id: None,
            capacity_lease_id: format!("agent-message-capacity:{}", command.message_id),
            direct_wake_id: format!("agent-message-wake:{}", command.message_id),
            queued_message_id: format!("agent-message-queue:{}", command.message_id),
            created_at: command.created_at,
            expires_at: Some(command.expires_at),
        })?;
        match &activation {
            AgentActivation::DirectBrainWakeRequested { session_id, .. } => {
                self.bus.publish(CoreEvent::BrainWakeRequested {
                    session_id: session_id.clone(),
                })?;
            }
            AgentActivation::QueuedForNextTurn { session_id, .. } => {
                self.enqueue_body_follow_up_message_without_wake(
                    session_id,
                    sender_agent_id,
                    model_body,
                    message.correlation_id,
                )?;
            }
            AgentActivation::ExternalTurnSteerRequested { .. } => {
                return self.observe_pending_agent_message_delivery(pending, sequence, activation);
            }
            AgentActivation::ExternalTurnRequested { .. } => {}
            AgentActivation::Rejected { reason_code } => {
                return self.finish_agent_message_delivery(
                    pending,
                    AgentMessageDeliveryStatus::Rejected,
                    Some(sequence),
                    Some(activation.clone()),
                    None,
                    Some(reason_code.clone()),
                )
            }
        }
        let _ = sender_request_id;
        self.finish_agent_message_delivery(
            pending,
            AgentMessageDeliveryStatus::Accepted,
            Some(sequence),
            Some(activation),
            None,
            None,
        )
    }

    pub fn begin_agent_round(
        &self,
        command: AgentRoundCommand,
    ) -> CoreResult<AgentRoundStartReceipt> {
        let (sender_agent_id, sender_session_id, sender_request_id) =
            self.resolve_coordination_caller(&command.caller)?;
        let recipient = self.sessions.get_session_by_agent(&command.to_agent_id)?;
        let round = AgentCorrelatedRound {
            round_id: command.round_id.clone(),
            idempotency_key: command.idempotency_key.clone(),
            sender_agent_id,
            sender_session_id,
            recipient_agent_id: command.to_agent_id.clone(),
            recipient_session_id: recipient.session_id,
            sender_request_id,
            message_id: command.message_id.clone(),
            correlation_id: command.correlation_id.clone(),
            reply_message_id: None,
            status: AgentRoundStatus::Pending,
            outcome: None,
            terminal_reason_code: None,
            created_at: command.created_at.clone(),
            expires_at: command.expires_at.clone(),
            terminal_at: None,
            revision: 1,
        };
        let round = self.store.create_agent_correlated_round(&round)?;
        self.bus.publish(CoreEvent::AgentRoundObserved {
            round: round.clone(),
        })?;
        let delivery = self.deliver_agent_message(AgentMessageCommand {
            caller: command.caller,
            delivery_id: rusty_crew_core_protocol::AgentMessageDeliveryId::new(format!(
                "round-delivery:{}",
                command.round_id.0
            )),
            idempotency_key: format!("round-delivery:{}", command.idempotency_key),
            message_id: command.message_id,
            to_agent_id: command.to_agent_id,
            body: command.body,
            collaboration_mode: None,
            correlation_id: Some(command.correlation_id),
            require_wake: true,
            created_at: command.created_at,
            expires_at: command.expires_at,
        })?;
        Ok(AgentRoundStartReceipt { round, delivery })
    }

    pub fn get_agent_round(
        &self,
        round_id: &rusty_crew_core_protocol::AgentRoundId,
    ) -> CoreResult<Option<AgentCorrelatedRound>> {
        let Some(round) = self.store.get_agent_correlated_round(round_id)? else {
            return Ok(None);
        };
        if round.status != AgentRoundStatus::Pending || round.expires_at > self.now() {
            return Ok(Some(round));
        }
        let mut expired = round.clone();
        expired.status = AgentRoundStatus::Expired;
        expired.terminal_reason_code = Some("agent_round_timeout".into());
        expired.terminal_at = Some(self.now());
        let expired = self
            .store
            .update_agent_correlated_round(&expired, round.revision)?;
        self.bus.publish(CoreEvent::AgentRoundObserved {
            round: expired.clone(),
        })?;
        Ok(Some(expired))
    }

    pub fn get_agent_message_delivery(
        &self,
        delivery_id: &rusty_crew_core_protocol::AgentMessageDeliveryId,
    ) -> CoreResult<Option<AgentMessageDeliveryReceipt>> {
        self.store.get_agent_message_delivery(delivery_id)
    }

    pub fn complete_agent_message_delivery(
        &self,
        completion: rusty_crew_core_protocol::AgentMessageDeliveryCompletion,
    ) -> CoreResult<AgentMessageDeliveryReceipt> {
        let current = self
            .store
            .get_agent_message_delivery(&completion.delivery_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    "agent message delivery was not found",
                )
            })?;
        if current.status.is_terminal() {
            return Ok(current);
        }
        if current.revision != completion.expected_revision {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "agent_message_delivery_revision_conflict",
            ));
        }
        if !matches!(
            current.activation,
            Some(AgentActivation::ExternalTurnSteerRequested { .. })
        ) {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "agent_message_delivery_completion_requires_pending_steer",
            ));
        }
        if !matches!(
            completion.status,
            AgentMessageDeliveryStatus::Accepted
                | AgentMessageDeliveryStatus::Rejected
                | AgentMessageDeliveryStatus::Expired
        ) {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "agent message delivery completion must be terminal",
            ));
        }
        let mut next = current;
        let expected_revision = next.revision;
        next.status = completion.status;
        next.reason_code = completion.reason_code;
        next.terminal_at = Some(completion.completed_at);
        let saved = self
            .store
            .update_agent_message_delivery(&next, expected_revision)?;
        self.bus.publish(CoreEvent::AgentMessageDeliveryObserved {
            receipt: saved.clone(),
        })?;
        Ok(saved)
    }

    fn resolve_coordination_caller(
        &self,
        caller: &AgentCoordinationCaller,
    ) -> CoreResult<(AgentId, Option<SessionId>, Option<ExternalTurnRequestId>)> {
        match caller {
            AgentCoordinationCaller::System { sender_agent_id } => {
                Ok((sender_agent_id.clone(), None, None))
            }
            AgentCoordinationCaller::DirectBrain { session_id, .. } => {
                let session = self.sessions.get_session(session_id)?;
                if session.status == SessionStatus::Archived {
                    return Err(CoreError::new(
                        CoreErrorKind::SessionExpired,
                        "archived direct-brain session cannot send agent messages",
                    ));
                }
                Ok((session.agent_id, Some(session.session_id), None))
            }
            AgentCoordinationCaller::ExternalAgent {
                runtime_id,
                binding_id,
                controller_instance_id,
                controller_generation,
                native_thread_id,
                native_turn_id,
                ..
            } => {
                let lease = self
                    .store
                    .get_external_controller_lease(runtime_id)?
                    .ok_or_else(|| {
                        CoreError::new(
                            CoreErrorKind::ActionRejected,
                            "external coordination caller has no controller lease",
                        )
                    })?;
                if lease.holder_instance_id != *controller_instance_id
                    || lease.generation != *controller_generation
                    || lease.expires_at <= self.now()
                {
                    return Err(CoreError::new(
                        CoreErrorKind::ActionRejected,
                        "external coordination caller does not hold the current controller lease",
                    ));
                }
                let binding = self
                    .store
                    .get_external_agent_binding(binding_id)?
                    .ok_or_else(|| {
                        CoreError::new(CoreErrorKind::NotFound, "external binding was not found")
                    })?;
                if !binding.is_routable()
                    || binding.runtime_id != *runtime_id
                    || binding.native_thread_id.as_ref() != Some(native_thread_id)
                {
                    return Err(CoreError::new(
                        CoreErrorKind::ActionRejected,
                        "external coordination caller does not match its durable binding",
                    ));
                }
                let turn = self
                    .store
                    .list_nonterminal_external_turns()?
                    .into_iter()
                    .find(|turn| {
                        turn.request.binding_id == *binding_id
                            && turn.native_thread_id == *native_thread_id
                            && turn.native_turn_id.as_ref() == Some(native_turn_id)
                    })
                    .ok_or_else(|| {
                        CoreError::new(
                            CoreErrorKind::ActionRejected,
                            "external coordination caller is not the active native turn",
                        )
                    })?;
                Ok((
                    binding.agent_id.expect("routable binding has agent id"),
                    Some(binding.session_id.expect("routable binding has session id")),
                    Some(turn.request.request_id),
                ))
            }
        }
    }

    fn matching_agent_round(
        &self,
        message: &AgentMessage,
    ) -> CoreResult<Option<AgentCorrelatedRound>> {
        let Some(correlation_id) = message.correlation_id.as_ref() else {
            return Ok(None);
        };
        Ok(self
            .store
            .list_pending_agent_rounds()?
            .into_iter()
            .find(|round| {
                round.sender_agent_id == message.to
                    && round.recipient_agent_id == message.from
                    && round.correlation_id == *correlation_id
            }))
    }

    fn resolve_agent_round_reply(
        &self,
        round: AgentCorrelatedRound,
        message: &AgentMessage,
        reply_message_id: &str,
        now: &IsoTimestamp,
    ) -> CoreResult<AgentCorrelatedRound> {
        let correlation_id = message
            .correlation_id
            .as_ref()
            .expect("matched round reply has a correlation id");
        if round.expires_at <= *now {
            let mut expired = round.clone();
            expired.status = AgentRoundStatus::Expired;
            expired.terminal_reason_code = Some("late_agent_round_reply".into());
            expired.terminal_at = Some(now.clone());
            let expired = self
                .store
                .update_agent_correlated_round(&expired, round.revision)?;
            self.bus.publish(CoreEvent::AgentRoundObserved {
                round: expired.clone(),
            })?;
            return Ok(expired);
        }
        let mut replied = round.clone();
        replied.reply_message_id = Some(reply_message_id.to_string());
        replied.status = AgentRoundStatus::Replied;
        replied.outcome = Some(json!({
            "from": message.from.0,
            "to": message.to.0,
            "body": message.body,
            "correlationId": correlation_id,
        }));
        replied.terminal_at = Some(now.clone());
        let replied = self
            .store
            .update_agent_correlated_round(&replied, round.revision)?;
        self.bus.publish(CoreEvent::AgentRoundObserved {
            round: replied.clone(),
        })?;
        Ok(replied)
    }

    fn finish_agent_message_delivery(
        &self,
        mut pending: AgentMessageDeliveryReceipt,
        status: AgentMessageDeliveryStatus,
        sequence: Option<u64>,
        activation: Option<AgentActivation>,
        resolved_round_id: Option<rusty_crew_core_protocol::AgentRoundId>,
        reason_code: Option<String>,
    ) -> CoreResult<AgentMessageDeliveryReceipt> {
        let expected_revision = pending.revision;
        pending.status = status;
        pending.sequence = sequence;
        pending.activation = activation;
        pending.resolved_round_id = resolved_round_id;
        pending.reason_code = reason_code;
        pending.terminal_at = Some(self.now());
        let receipt = self
            .store
            .update_agent_message_delivery(&pending, expected_revision)?;
        self.bus.publish(CoreEvent::AgentMessageDeliveryObserved {
            receipt: receipt.clone(),
        })?;
        Ok(receipt)
    }

    fn observe_pending_agent_message_delivery(
        &self,
        mut pending: AgentMessageDeliveryReceipt,
        sequence: u64,
        activation: AgentActivation,
    ) -> CoreResult<AgentMessageDeliveryReceipt> {
        let expected_revision = pending.revision;
        pending.sequence = Some(sequence);
        pending.activation = Some(activation);
        let receipt = self
            .store
            .update_agent_message_delivery(&pending, expected_revision)?;
        self.bus.publish(CoreEvent::AgentMessageDeliveryObserved {
            receipt: receipt.clone(),
        })?;
        Ok(receipt)
    }
}

fn routed_agent_message_text(request: &AgentMessageDeliveryRequest) -> String {
    format!(
        "[Rusty Crew routed message]\nmessage_id: {}\nfrom_agent_id: {}\ncorrelation_id: {}\nexpires_at: {}\n\n{}",
        request.message_id,
        request.from_agent_id.0,
        request.correlation_id.as_deref().unwrap_or("none"),
        request.expires_at,
        request.body
    )
}
