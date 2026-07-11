//! Rust-owned lifecycle and activation rules for complete external runtimes.

use super::*;
use rusty_crew_core_protocol::{
    AgentActivation, AgentMessageDeliveryStatus, AgentRoundStatus, ExternalAgentBinding,
    ExternalBindingId, ExternalBindingPurpose, ExternalBindingStatus, ExternalControlId,
    ExternalControlKind, ExternalControlReceipt, ExternalControlRequest, ExternalControlStatus,
    ExternalControllerContext, ExternalControllerLease, ExternalInteractionRecord,
    ExternalInteractionStatus, ExternalRuntimeDesiredState, ExternalRuntimeEventInput,
    ExternalRuntimeHandshakeDecision, ExternalRuntimeHandshakeObservation, ExternalRuntimeId,
    ExternalRuntimeObservedState, ExternalRuntimeRegistration, ExternalRuntimeStateObservation,
    ExternalTurnCorrelation, ExternalTurnInputPart, ExternalTurnPhase, ExternalTurnRequestId,
    NormalizedExternalRuntimeEvent, SessionTurnRequested, TurnInputProvenance,
};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq)]
pub struct AgentActivationRequest {
    pub agent_id: AgentId,
    pub request_id: ExternalTurnRequestId,
    pub idempotency_key: String,
    pub input: Vec<ExternalTurnInputPart>,
    pub provenance: TurnInputProvenance,
    pub run_id: Option<RunId>,
    pub capacity_lease_id: String,
    pub direct_wake_id: String,
    pub queued_message_id: String,
    pub created_at: IsoTimestamp,
    pub expires_at: Option<IsoTimestamp>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExternalRuntimeHydrationReport {
    pub runtime_count: usize,
    pub binding_count: usize,
    pub driver_reconciliation_request_ids: Vec<ExternalTurnRequestId>,
    pub terminalized_request_ids: Vec<ExternalTurnRequestId>,
    pub expired_interaction_ids: Vec<String>,
    pub expired_round_ids: Vec<String>,
    pub reconciled_delivery_ids: Vec<String>,
}

impl CoreEngine {
    fn validate_external_controller(
        &self,
        runtime_id: &ExternalRuntimeId,
        context: &ExternalControllerContext,
    ) -> CoreResult<ExternalControllerLease> {
        let lease = self
            .store
            .get_external_controller_lease(runtime_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::ActionRejected,
                    "external runtime has no active controller lease",
                )
            })?;
        if lease.holder_instance_id != context.holder_instance_id
            || lease.generation != context.generation
            || lease.expires_at <= self.now()
        {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "external runtime controller lease is stale or expired",
            ));
        }
        Ok(lease)
    }

    pub fn list_external_runtimes(&self) -> CoreResult<Vec<ExternalRuntimeRegistration>> {
        self.store.list_external_runtime_registrations()
    }

    pub fn get_external_runtime(
        &self,
        runtime_id: &ExternalRuntimeId,
    ) -> CoreResult<Option<ExternalRuntimeRegistration>> {
        self.store.get_external_runtime_registration(runtime_id)
    }

    pub fn authorize_external_runtime_handshake(
        &self,
        observation: &ExternalRuntimeHandshakeObservation,
    ) -> CoreResult<ExternalRuntimeHandshakeDecision> {
        self.validate_external_controller(&observation.runtime_id, &observation.controller)?;
        let current = self
            .store
            .get_external_runtime_registration(&observation.runtime_id)?
            .ok_or_else(|| {
                CoreError::new(CoreErrorKind::NotFound, "external runtime was not found")
            })?;
        let reason_code = if current.desired_state != ExternalRuntimeDesiredState::Enabled {
            Some("external_runtime_disabled")
        } else if current.expected_cli_version != observation.cli_version {
            Some("external_runtime_cli_version_mismatch")
        } else if current.executable_sha256 != observation.executable_sha256 {
            Some("external_runtime_executable_mismatch")
        } else if current.protocol_schema_sha256 != observation.protocol_schema_sha256 {
            Some("external_runtime_protocol_schema_mismatch")
        } else {
            None
        };
        let mut next = current.clone();
        next.observed_state = if reason_code.is_none() {
            ExternalRuntimeObservedState::Ready
        } else {
            ExternalRuntimeObservedState::Incompatible
        };
        next.observed_reason_code = reason_code.map(str::to_owned);
        next.updated_at = observation.observed_at.clone();
        let saved = self
            .store
            .put_external_runtime_registration(&next, Some(current.revision))?;
        Ok(ExternalRuntimeHandshakeDecision {
            accepted: reason_code.is_none(),
            reason_code: reason_code.map(str::to_owned),
            registration: saved,
        })
    }

    pub fn record_external_runtime_state(
        &self,
        observation: &ExternalRuntimeStateObservation,
    ) -> CoreResult<ExternalRuntimeRegistration> {
        self.validate_external_controller(&observation.runtime_id, &observation.controller)?;
        if matches!(
            observation.observed_state,
            ExternalRuntimeObservedState::Ready | ExternalRuntimeObservedState::Incompatible
        ) {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "ready and incompatible states require exact handshake authorization",
            ));
        }
        let current = self
            .store
            .get_external_runtime_registration(&observation.runtime_id)?
            .ok_or_else(|| {
                CoreError::new(CoreErrorKind::NotFound, "external runtime was not found")
            })?;
        let mut next = current.clone();
        next.observed_state = observation.observed_state;
        next.observed_reason_code = observation.reason_code.clone();
        next.updated_at = observation.observed_at.clone();
        self.store
            .put_external_runtime_registration(&next, Some(current.revision))
    }

    pub fn list_external_bindings(&self) -> CoreResult<Vec<ExternalAgentBinding>> {
        self.store.list_external_agent_bindings()
    }

    pub fn get_external_binding(
        &self,
        binding_id: &ExternalBindingId,
    ) -> CoreResult<Option<ExternalAgentBinding>> {
        self.store.get_external_agent_binding(binding_id)
    }

    pub fn get_external_turn(
        &self,
        request_id: &ExternalTurnRequestId,
    ) -> CoreResult<Option<ExternalTurnCorrelation>> {
        self.store.get_external_turn(request_id)
    }

    pub fn list_active_external_turns(&self) -> CoreResult<Vec<ExternalTurnCorrelation>> {
        self.store.list_nonterminal_external_turns()
    }

    pub fn submit_external_control(
        &self,
        request: ExternalControlRequest,
    ) -> CoreResult<ExternalControlReceipt> {
        let binding = self
            .store
            .get_external_agent_binding(&request.binding_id)?
            .ok_or_else(|| {
                CoreError::new(CoreErrorKind::NotFound, "external binding was not found")
            })?;
        if binding.revision != request.expected_binding_revision {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "external binding revision mismatch: expected {}, found {}",
                    request.expected_binding_revision, binding.revision
                ),
            ));
        }
        if binding.status != ExternalBindingStatus::Active {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "external control requires an active binding",
            ));
        }
        let active_turn = self
            .store
            .list_nonterminal_external_turns()?
            .into_iter()
            .find(|turn| turn.request.binding_id == request.binding_id);
        if matches!(
            request.kind,
            ExternalControlKind::SteerTurn
                | ExternalControlKind::InterruptTurn
                | ExternalControlKind::ResolveInteraction
        ) && request.expected_native_turn_id.is_none()
        {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "mid-turn external control requires expected_native_turn_id",
            ));
        }
        if request.kind == ExternalControlKind::CompactThread && active_turn.is_some() {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "external thread compaction requires an idle binding",
            ));
        }
        if let Some(expected_native_turn_id) = &request.expected_native_turn_id {
            let matches_active_turn = active_turn
                .as_ref()
                .is_some_and(|turn| turn.native_turn_id.as_ref() == Some(expected_native_turn_id));
            if !matches_active_turn {
                return Err(CoreError::new(
                    CoreErrorKind::ActionRejected,
                    "expected native turn is not the binding's active turn",
                ));
            }
        }
        let request_json = serde_json::to_vec(&request)
            .map_err(|error| CoreError::new(CoreErrorKind::InternalError, error.to_string()))?;
        let receipt = ExternalControlReceipt {
            request,
            request_fingerprint: format!("{:x}", Sha256::digest(request_json)),
            status: ExternalControlStatus::Pending,
            outcome: None,
            reason_code: None,
            revision: 1,
            updated_at: self.now(),
        };
        self.store.put_external_control_receipt(&receipt)
    }

    pub fn complete_external_control(
        &self,
        control_id: &ExternalControlId,
        status: ExternalControlStatus,
        outcome: Option<serde_json::Value>,
        reason_code: Option<String>,
        now: IsoTimestamp,
    ) -> CoreResult<ExternalControlReceipt> {
        if !status.is_terminal() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "external control completion requires a terminal status",
            ));
        }
        let current = self
            .store
            .get_external_control_receipt(control_id)?
            .ok_or_else(|| {
                CoreError::new(CoreErrorKind::NotFound, "external control was not found")
            })?;
        let mut next = current.clone();
        next.status = status;
        next.outcome = outcome;
        next.reason_code = reason_code;
        next.updated_at = now;
        self.store
            .update_external_control_receipt(&next, current.revision)
    }

    pub fn complete_external_control_from_controller(
        &self,
        controller: &ExternalControllerContext,
        control_id: &ExternalControlId,
        status: ExternalControlStatus,
        outcome: Option<serde_json::Value>,
        reason_code: Option<String>,
        now: IsoTimestamp,
    ) -> CoreResult<ExternalControlReceipt> {
        let receipt = self
            .store
            .get_external_control_receipt(control_id)?
            .ok_or_else(|| {
                CoreError::new(CoreErrorKind::NotFound, "external control was not found")
            })?;
        let binding = self
            .store
            .get_external_agent_binding(&receipt.request.binding_id)?
            .ok_or_else(|| {
                CoreError::new(CoreErrorKind::NotFound, "external binding was not found")
            })?;
        self.validate_external_controller(&binding.runtime_id, controller)?;
        self.complete_external_control(control_id, status, outcome, reason_code, now)
    }

    pub fn record_external_interaction(
        &self,
        interaction: &ExternalInteractionRecord,
    ) -> CoreResult<ExternalInteractionRecord> {
        let turn = self
            .store
            .get_external_turn(&interaction.request_id)?
            .ok_or_else(|| {
                CoreError::new(CoreErrorKind::NotFound, "external turn was not found")
            })?;
        if turn.runtime_id != interaction.runtime_id
            || turn.request.binding_id != interaction.binding_id
            || turn.native_thread_id != interaction.native_thread_id
            || turn.native_turn_id.as_ref() != Some(&interaction.native_turn_id)
        {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "external interaction identity does not match its active turn",
            ));
        }
        if turn.phase.is_terminal() {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "external interaction cannot be recorded for a terminal turn",
            ));
        }
        self.store.put_external_interaction(interaction)
    }

    pub fn record_external_interaction_from_controller(
        &self,
        controller: &ExternalControllerContext,
        interaction: &ExternalInteractionRecord,
    ) -> CoreResult<ExternalInteractionRecord> {
        self.validate_external_controller(&interaction.runtime_id, controller)?;
        self.record_external_interaction(interaction)
    }

    pub fn resolve_external_interaction(
        &self,
        next: &ExternalInteractionRecord,
        expected_revision: u64,
    ) -> CoreResult<ExternalInteractionRecord> {
        if next.status != ExternalInteractionStatus::Resolved {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "interaction resolution must use resolved status",
            ));
        }
        self.store
            .update_external_interaction(next, expected_revision)
    }

    pub fn terminalize_external_interaction_from_controller(
        &self,
        controller: &ExternalControllerContext,
        next: &ExternalInteractionRecord,
        expected_revision: u64,
    ) -> CoreResult<ExternalInteractionRecord> {
        if !matches!(
            next.status,
            ExternalInteractionStatus::Expired | ExternalInteractionStatus::Lost
        ) {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "controller interaction terminalization requires expired or lost status",
            ));
        }
        self.validate_external_controller(&next.runtime_id, controller)?;
        self.store
            .update_external_interaction(next, expected_revision)
    }

    pub fn list_pending_external_interactions(&self) -> CoreResult<Vec<ExternalInteractionRecord>> {
        self.store.list_pending_external_interactions()
    }

    pub fn record_external_runtime_event(
        &self,
        controller: &ExternalControllerContext,
        event: &ExternalRuntimeEventInput,
    ) -> CoreResult<NormalizedExternalRuntimeEvent> {
        self.validate_external_controller(&event.runtime_id, controller)?;
        if self
            .store
            .get_external_runtime_registration(&event.runtime_id)?
            .is_none()
        {
            return Err(CoreError::new(
                CoreErrorKind::NotFound,
                "external runtime event references an unknown runtime",
            ));
        }
        self.store.append_external_runtime_event_allocated(event)
    }

    pub fn query_external_runtime_events(
        &self,
        runtime_id: &ExternalRuntimeId,
        after_sequence: u64,
        limit: u32,
    ) -> CoreResult<Vec<NormalizedExternalRuntimeEvent>> {
        self.store
            .query_external_runtime_events(runtime_id, after_sequence, limit)
    }

    pub fn register_external_runtime(
        &self,
        registration: &ExternalRuntimeRegistration,
        expected_revision: Option<u64>,
    ) -> CoreResult<ExternalRuntimeRegistration> {
        self.store
            .put_external_runtime_registration(registration, expected_revision)
    }

    pub fn acquire_external_runtime_controller(
        &self,
        candidate: &ExternalControllerLease,
        now: &IsoTimestamp,
    ) -> CoreResult<ExternalControllerLease> {
        self.store.acquire_external_controller_lease(candidate, now)
    }

    pub fn release_external_runtime_controller(
        &self,
        runtime_id: &rusty_crew_core_protocol::ExternalRuntimeId,
        holder_instance_id: &str,
        generation: u64,
        now: &IsoTimestamp,
    ) -> CoreResult<ExternalControllerLease> {
        self.store.release_external_controller_lease(
            runtime_id,
            holder_instance_id,
            generation,
            now,
        )
    }

    pub fn bind_external_agent(
        &self,
        binding: &ExternalAgentBinding,
        expected_revision: Option<u64>,
    ) -> CoreResult<ExternalAgentBinding> {
        binding.validate()?;
        if binding.purpose == ExternalBindingPurpose::CrewAgent {
            let session_id = binding.session_id.as_ref().ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "routable external binding requires session_id",
                )
            })?;
            let session = self.sessions.get_session(session_id)?;
            if binding.agent_id.as_ref() != Some(&session.agent_id) {
                return Err(CoreError::new(
                    CoreErrorKind::ActionRejected,
                    "external binding agent_id does not match the bound Crew session",
                ));
            }
            if session.status == SessionStatus::Archived {
                return Err(CoreError::new(
                    CoreErrorKind::SessionExpired,
                    "cannot bind an archived Crew session to an external runtime",
                ));
            }
        }
        let runtime = self
            .store
            .get_external_runtime_registration(&binding.runtime_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    format!("external runtime {} was not found", binding.runtime_id.0),
                )
            })?;
        if runtime.desired_state != ExternalRuntimeDesiredState::Enabled {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "cannot bind to a disabled external runtime",
            ));
        }
        self.store
            .put_external_agent_binding(binding, expected_revision)
    }

    pub fn activate_agent_execution(
        &self,
        request: AgentActivationRequest,
    ) -> CoreResult<AgentActivation> {
        let session = self.sessions.get_session_by_agent(&request.agent_id)?;
        let Some(binding) = self
            .store
            .get_external_binding_for_agent(&request.agent_id)?
        else {
            return Ok(AgentActivation::DirectBrainWakeRequested {
                session_id: session.session_id,
                wake_id: request.direct_wake_id,
            });
        };
        if binding.status != ExternalBindingStatus::Active || !binding.is_routable() {
            return Ok(AgentActivation::Rejected {
                reason_code: "external_binding_not_routable".into(),
            });
        }
        let runtime = self
            .store
            .get_external_runtime_registration(&binding.runtime_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    format!("external runtime {} was not found", binding.runtime_id.0),
                )
            })?;
        if runtime.desired_state != ExternalRuntimeDesiredState::Enabled {
            return Ok(AgentActivation::Rejected {
                reason_code: "external_runtime_disabled".into(),
            });
        }
        if runtime.observed_state != ExternalRuntimeObservedState::Ready {
            return Ok(AgentActivation::Rejected {
                reason_code: match runtime.observed_state {
                    ExternalRuntimeObservedState::Incompatible => {
                        "external_runtime_incompatible".into()
                    }
                    _ => "external_runtime_not_ready".into(),
                },
            });
        }
        if self
            .store
            .list_nonterminal_external_turns()?
            .iter()
            .any(|turn| turn.request.session_id == session.session_id)
        {
            return Ok(AgentActivation::QueuedForNextTurn {
                session_id: session.session_id,
                queue_id: request.queued_message_id,
            });
        }
        let Some(native_thread_id) = binding.native_thread_id.clone() else {
            return Ok(AgentActivation::Rejected {
                reason_code: "external_thread_not_bound".into(),
            });
        };
        let turn = ExternalTurnCorrelation {
            request: SessionTurnRequested {
                request_id: request.request_id.clone(),
                idempotency_key: request.idempotency_key,
                session_id: session.session_id.clone(),
                run_id: request.run_id,
                binding_id: binding.binding_id.clone(),
                input: request.input,
                provenance: request.provenance,
                created_at: request.created_at.clone(),
                expires_at: request.expires_at,
            },
            runtime_id: binding.runtime_id,
            native_thread_id,
            native_turn_id: None,
            task_ref: binding.task_ref,
            phase: ExternalTurnPhase::Accepted,
            capacity_lease_id: Some(request.capacity_lease_id),
            terminal_reason_code: None,
            revision: 1,
            updated_at: request.created_at,
        };
        let saved = self.store.create_external_turn(&turn)?;
        Ok(AgentActivation::ExternalTurnRequested {
            session_id: saved.request.session_id,
            request_id: saved.request.request_id,
            binding_id: saved.request.binding_id,
        })
    }

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
        if became_terminal {
            self.promote_next_external_follow_up(&saved.request.session_id, &saved.updated_at)?;
        }
        Ok(saved)
    }

    fn promote_next_external_follow_up(
        &self,
        session_id: &SessionId,
        now: &IsoTimestamp,
    ) -> CoreResult<Option<ExternalTurnCorrelation>> {
        self.store.expire_queued_messages_at(now)?;
        let Some(mut queued) = self
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
        let request_id =
            ExternalTurnRequestId::new(format!("external-follow-up:{}", queued.message_id));
        let activation = self.activate_agent_execution(AgentActivationRequest {
            agent_id: queued.message.to.clone(),
            request_id: request_id.clone(),
            idempotency_key: format!("external-follow-up:{}", queued.message_id),
            input: vec![ExternalTurnInputPart::Text {
                text: queued.message.body.clone(),
            }],
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
        })?;
        if !matches!(activation, AgentActivation::ExternalTurnRequested { .. }) {
            return Ok(None);
        }
        let turn = self.store.get_external_turn(&request_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::InternalError,
                format!("promoted external turn {} was not persisted", request_id.0),
            )
        })?;
        queued.state = QueuedMessageState::Delivered;
        queued.delivery_attempts += 1;
        queued.terminal_at = Some(now.clone());
        queued.state_reason = Some(format!("promoted_to_external_turn:{}", request_id.0));
        self.store.save_queued_message(&queued)?;
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

    pub fn hydrate_external_runtime_lifecycle(
        &self,
        now: &IsoTimestamp,
    ) -> CoreResult<ExternalRuntimeHydrationReport> {
        let runtimes = self.store.list_external_runtime_registrations()?;
        let bindings = self.store.list_external_agent_bindings()?;
        let runtime_by_id = runtimes
            .iter()
            .map(|runtime| (runtime.runtime_id.clone(), runtime))
            .collect::<HashMap<_, _>>();
        let mut report = ExternalRuntimeHydrationReport {
            runtime_count: runtimes.len(),
            binding_count: bindings.len(),
            ..Default::default()
        };

        for turn in self.store.list_nonterminal_external_turns()? {
            let session_unavailable = self
                .sessions
                .get_session(&turn.request.session_id)
                .map(|session| session.status == SessionStatus::Archived)
                .unwrap_or(true);
            let runtime = runtime_by_id.get(&turn.runtime_id).copied();
            let terminal = if session_unavailable {
                Some((
                    ExternalTurnPhase::Interrupted,
                    "external_session_unavailable",
                ))
            } else if runtime.is_none_or(|runtime| {
                runtime.desired_state != ExternalRuntimeDesiredState::Enabled
                    || runtime.observed_state == ExternalRuntimeObservedState::Incompatible
            }) {
                Some((
                    ExternalTurnPhase::OutcomeUnknown,
                    "external_runtime_unavailable",
                ))
            } else {
                None
            };
            if let Some((phase, reason)) = terminal {
                self.transition_external_turn(
                    &turn.request.request_id,
                    phase,
                    None,
                    Some(reason.into()),
                    now.clone(),
                )?;
                report
                    .terminalized_request_ids
                    .push(turn.request.request_id);
            } else {
                report
                    .driver_reconciliation_request_ids
                    .push(turn.request.request_id);
            }
        }

        for interaction in self.store.list_pending_external_interactions()? {
            if interaction.expires_at <= *now {
                let mut expired = interaction.clone();
                expired.status = ExternalInteractionStatus::Expired;
                expired.resolved_at = Some(now.clone());
                self.store
                    .update_external_interaction(&expired, interaction.revision)?;
                report
                    .expired_interaction_ids
                    .push(interaction.interaction_id.0);
            }
        }

        for round in self.store.list_pending_agent_rounds()? {
            if round.expires_at <= *now {
                let mut expired = round.clone();
                expired.status = AgentRoundStatus::Expired;
                expired.terminal_reason_code = Some("agent_round_timeout".into());
                expired.terminal_at = Some(now.clone());
                let expired = self
                    .store
                    .update_agent_correlated_round(&expired, round.revision)?;
                self.bus
                    .publish(CoreEvent::AgentRoundObserved { round: expired })?;
                report.expired_round_ids.push(round.round_id.0);
            }
        }
        for delivery in self.store.list_pending_agent_message_deliveries()? {
            let mut reconciled = delivery.clone();
            if delivery.request.expires_at <= *now {
                reconciled.status = AgentMessageDeliveryStatus::Expired;
                reconciled.reason_code = Some("agent_message_expired".into());
            } else {
                reconciled.status = AgentMessageDeliveryStatus::Rejected;
                reconciled.reason_code = Some("delivery_outcome_unknown_after_restart".into());
            }
            reconciled.terminal_at = Some(now.clone());
            let saved = self
                .store
                .update_agent_message_delivery(&reconciled, delivery.revision)?;
            self.bus.publish(CoreEvent::AgentMessageDeliveryObserved {
                receipt: saved.clone(),
            })?;
            report
                .reconciled_delivery_ids
                .push(saved.request.delivery_id.0);
        }
        Ok(report)
    }
}
