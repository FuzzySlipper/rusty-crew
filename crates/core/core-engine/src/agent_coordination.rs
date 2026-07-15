//! Runtime-neutral direct-agent messaging and durable correlated rounds.

use super::*;
use rusty_crew_core_protocol::{
    AgentActivation, AgentCoordinationCaller, AgentCorrelatedRound, AgentDirectoryEntry,
    AgentDirectoryRuntimeKind, AgentMessageCommand, AgentMessageDeliveryReceipt,
    AgentMessageDeliveryRequest, AgentMessageDeliveryStatus, AgentMessageInboxItem,
    AgentMessageInboxQuery, AgentMessageInboxStatus, AgentMessageReplyCommand, AgentRoundCommand,
    AgentRoundStartReceipt, AgentRoundStatus, ExternalBindingPurpose, ExternalBindingStatus,
    ExternalRuntimeDesiredState, ExternalRuntimeKind, ExternalRuntimeObservedState,
    ExternalTurnInputPart, ExternalTurnPhase, ExternalTurnRequestId, TurnInputProvenance,
    TurnInputProvenanceKind,
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
        self.deliver_agent_message_with_reply(command, None)
    }

    fn deliver_agent_message_with_reply(
        &self,
        command: AgentMessageCommand,
        reply_to_message_id: Option<String>,
    ) -> CoreResult<AgentMessageDeliveryReceipt> {
        validate_agent_message_bounds(&command.body, &command.created_at, &command.expires_at)?;
        let (sender_agent_id, sender_session_id, sender_request_id) =
            self.resolve_coordination_caller(&command.caller)?;
        let recipient = self.sessions.get_session_by_agent(&command.to_agent_id);
        let request = AgentMessageDeliveryRequest {
            delivery_id: command.delivery_id,
            idempotency_key: command.idempotency_key,
            message_id: command.message_id.clone(),
            from_agent_id: sender_agent_id.clone(),
            from_session_id: sender_session_id,
            to_agent_id: command.to_agent_id.clone(),
            to_session_id: recipient
                .as_ref()
                .ok()
                .map(|session| session.session_id.clone()),
            reply_to_message_id,
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
        let session = match recipient {
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
                if let Err(error) = self.enqueue_routed_agent_message_without_wake(
                    session_id,
                    &pending.request,
                    model_body,
                ) {
                    if matches!(
                        error.kind,
                        CoreErrorKind::ActionRejected | CoreErrorKind::InvalidInput
                    ) {
                        return self.finish_agent_message_delivery(
                            pending,
                            AgentMessageDeliveryStatus::Rejected,
                            Some(sequence),
                            Some(activation.clone()),
                            None,
                            Some(error.message),
                        );
                    }
                    return Err(error);
                }
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

    pub fn get_agent_message_delivery_by_message_id(
        &self,
        message_id: &str,
    ) -> CoreResult<Option<AgentMessageDeliveryReceipt>> {
        self.store
            .get_agent_message_delivery_by_message_id(message_id)
    }

    pub fn list_agent_message_inbox(
        &self,
        query: &AgentMessageInboxQuery,
    ) -> CoreResult<Vec<AgentMessageInboxItem>> {
        let limit = query.limit.unwrap_or(100).clamp(1, 500);
        let deliveries = self
            .store
            .list_agent_message_inbox_deliveries(query.to_agent_id.as_ref(), limit)?;
        deliveries
            .into_iter()
            .map(|delivery| self.project_agent_message_inbox_item(delivery))
            .collect()
    }

    fn project_agent_message_inbox_item(
        &self,
        delivery: AgentMessageDeliveryReceipt,
    ) -> CoreResult<AgentMessageInboxItem> {
        let reply = self
            .store
            .get_agent_message_reply(&delivery.request.message_id)?;
        let queued_message_id = format!("agent-message-queue:{}", delivery.request.message_id);
        let queue = delivery
            .request
            .to_session_id
            .as_ref()
            .map(|session_id| {
                self.store.load_queued_messages(&QueuedMessageFilter {
                    state: None,
                    owner_session_id: Some(session_id.clone()),
                    owner_agent_id: None,
                    limit: None,
                })
            })
            .transpose()?
            .unwrap_or_default()
            .into_iter()
            .find(|queued| queued.message_id == queued_message_id);
        let direct_request_id =
            ExternalTurnRequestId::new(format!("agent-message:{}", delivery.request.message_id));
        let follow_up_request_id =
            ExternalTurnRequestId::new(format!("external-follow-up:{queued_message_id}"));
        let turn = self
            .store
            .get_external_turn(&direct_request_id)?
            .or(self.store.get_external_turn(&follow_up_request_id)?);
        let status = match delivery.status {
            AgentMessageDeliveryStatus::Rejected => AgentMessageInboxStatus::Rejected,
            AgentMessageDeliveryStatus::Expired => AgentMessageInboxStatus::Expired,
            AgentMessageDeliveryStatus::Pending => AgentMessageInboxStatus::InProgress,
            AgentMessageDeliveryStatus::Accepted => {
                if matches!(
                    queue.as_ref().map(|record| record.state),
                    Some(QueuedMessageState::Pending)
                ) {
                    AgentMessageInboxStatus::Queued
                } else if let Some(turn) = turn.as_ref() {
                    match turn.phase {
                        ExternalTurnPhase::Accepted
                        | ExternalTurnPhase::Starting
                        | ExternalTurnPhase::Active
                        | ExternalTurnPhase::WaitingInteraction => {
                            AgentMessageInboxStatus::InProgress
                        }
                        ExternalTurnPhase::Completed if reply.is_some() => {
                            AgentMessageInboxStatus::Replied
                        }
                        ExternalTurnPhase::Completed
                            if matches!(
                                turn.terminal_reason_code.as_deref(),
                                Some("review_no_reply" | "agent_message_no_reply")
                            ) =>
                        {
                            AgentMessageInboxStatus::NoReply
                        }
                        ExternalTurnPhase::Completed => AgentMessageInboxStatus::AwaitingReply,
                        ExternalTurnPhase::Failed
                        | ExternalTurnPhase::Interrupted
                        | ExternalTurnPhase::OutcomeUnknown => AgentMessageInboxStatus::Failed,
                    }
                } else if matches!(
                    queue.as_ref().map(|record| record.state),
                    Some(QueuedMessageState::Expired)
                ) {
                    AgentMessageInboxStatus::Expired
                } else if matches!(
                    queue.as_ref().map(|record| record.state),
                    Some(QueuedMessageState::Cancelled)
                ) {
                    AgentMessageInboxStatus::Rejected
                } else if matches!(
                    queue.as_ref().map(|record| record.state),
                    Some(QueuedMessageState::Discarded)
                ) {
                    AgentMessageInboxStatus::Failed
                } else if reply.is_some() {
                    AgentMessageInboxStatus::Replied
                } else {
                    AgentMessageInboxStatus::NoReply
                }
            }
        };
        let external_turn_request_id = turn
            .as_ref()
            .map(|record| record.request.request_id.clone());
        let terminal_reason_code = turn
            .as_ref()
            .and_then(|record| record.terminal_reason_code.clone())
            .or_else(|| {
                queue
                    .as_ref()
                    .and_then(|record| record.state_reason.clone())
            });
        Ok(AgentMessageInboxItem {
            delivery,
            reply,
            status,
            queued_message_id: queue.map(|record| record.message_id),
            external_turn_request_id,
            terminal_reason_code,
        })
    }

    pub fn reply_agent_message(
        &self,
        command: AgentMessageReplyCommand,
    ) -> CoreResult<AgentMessageDeliveryReceipt> {
        let (replying_agent_id, replying_session_id, _) =
            self.resolve_coordination_caller(&command.caller)?;
        let original = self
            .store
            .get_agent_message_delivery_by_message_id(&command.in_reply_to_message_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    "agent_message_reply_original_not_found",
                )
            })?;
        if original.status != AgentMessageDeliveryStatus::Accepted {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "agent_message_reply_original_not_accepted",
            ));
        }
        if original.request.expires_at <= self.now() {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "agent_message_reply_original_expired",
            ));
        }
        if original.request.to_agent_id != replying_agent_id
            || original.request.to_session_id != replying_session_id
        {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "agent_message_reply_wrong_recipient",
            ));
        }
        let expected_reply_session = original.request.from_session_id.clone().ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::ActionRejected,
                "agent_message_reply_sender_has_no_session",
            )
        })?;
        let current_reply_session = self
            .sessions
            .get_session_by_agent(&original.request.from_agent_id)?;
        if current_reply_session.session_id != expected_reply_session
            || current_reply_session.status == SessionStatus::Archived
        {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "agent_message_reply_sender_session_changed",
            ));
        }
        if let Some(existing) = self
            .store
            .get_agent_message_reply(&command.in_reply_to_message_id)?
        {
            if existing.request.delivery_id == command.delivery_id
                && existing.request.idempotency_key == command.idempotency_key
                && existing.request.message_id == command.message_id
                && existing.request.body == command.body
            {
                return Ok(existing);
            }
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                "agent_message_reply_already_exists",
            ));
        }
        self.deliver_agent_message_with_reply(
            AgentMessageCommand {
                caller: command.caller,
                delivery_id: command.delivery_id,
                idempotency_key: command.idempotency_key,
                message_id: command.message_id,
                to_agent_id: original.request.from_agent_id,
                body: command.body,
                collaboration_mode: None,
                correlation_id: original
                    .request
                    .correlation_id
                    .or(Some(command.in_reply_to_message_id.clone())),
                require_wake: true,
                created_at: command.created_at,
                expires_at: command.expires_at,
            },
            Some(command.in_reply_to_message_id),
        )
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
    let from_session_id = request
        .from_session_id
        .as_ref()
        .map(|value| value.0.as_str());
    let reply_instruction = match from_session_id {
        Some(_) => format!(
            "reply_instruction: call rusty_crew.reply_agent_message with messageId={} and your reply body",
            request.message_id
        ),
        None => "reply_instruction: unavailable (sender has no routable agent session; respond in this turn only)".to_string(),
    };
    format!(
        "[Rusty Crew routed message]\nmessage_id: {}\nfrom_agent_id: {}\nfrom_session_id: {}\ncorrelation_id: {}\ncreated_at: {}\nexpires_at: {}\n{}\n\n{}",
        request.message_id,
        request.from_agent_id.0,
        from_session_id.unwrap_or("none"),
        request.correlation_id.as_deref().unwrap_or("none"),
        request.created_at,
        request.expires_at,
        reply_instruction,
        request.body
    )
}

fn validate_agent_message_bounds(
    body: &str,
    created_at: &IsoTimestamp,
    expires_at: &IsoTimestamp,
) -> CoreResult<()> {
    const MIN_TTL_MS: i128 = 1;
    const MAX_TTL_MS: i128 = 24 * 60 * 60 * 1_000;
    const MAX_BODY_BYTES: usize = 256 * 1024;
    if body.is_empty() || body.len() > MAX_BODY_BYTES {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "agent_message_body_size_invalid",
        ));
    }
    let ttl_ms = (parse_rfc3339(expires_at)? - parse_rfc3339(created_at)?).whole_milliseconds();
    if !(MIN_TTL_MS..=MAX_TTL_MS).contains(&ttl_ms) {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "agent_message_ttl_out_of_bounds",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod routed_agent_message_text_tests {
    use super::*;
    use rusty_crew_core_protocol::{AgentId, AgentMessageDeliveryId, SessionId};

    #[test]
    fn agent_sender_receives_reply_by_message_instruction() {
        let text = routed_agent_message_text(&request(Some("sender-session")));

        assert!(text.contains("from_session_id: sender-session"));
        assert!(text.contains(
            "reply_instruction: call rusty_crew.reply_agent_message with messageId=message-1 and your reply body"
        ));
    }

    #[test]
    fn operator_sender_does_not_receive_impossible_reply_instruction() {
        let text = routed_agent_message_text(&request(None));

        assert!(text.contains("from_session_id: none"));
        assert!(text.contains(
            "reply_instruction: unavailable (sender has no routable agent session; respond in this turn only)"
        ));
        assert!(!text.contains("call rusty_crew.reply_agent_message"));
    }

    fn request(from_session_id: Option<&str>) -> AgentMessageDeliveryRequest {
        AgentMessageDeliveryRequest {
            delivery_id: AgentMessageDeliveryId::new("delivery-1"),
            idempotency_key: "delivery-1".into(),
            message_id: "message-1".into(),
            from_agent_id: AgentId::new("sender"),
            from_session_id: from_session_id.map(SessionId::new),
            to_agent_id: AgentId::new("recipient"),
            to_session_id: Some(SessionId::new("recipient-session")),
            reply_to_message_id: None,
            body: "inspect this".into(),
            collaboration_mode: None,
            correlation_id: Some("correlation-1".into()),
            require_wake: true,
            created_at: "2026-07-15T00:00:00Z".into(),
            expires_at: "2026-07-15T00:10:00Z".into(),
        }
    }
}
