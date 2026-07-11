//! Rust-owned lifecycle and activation rules for complete external runtimes.

use super::*;
use rusty_crew_core_protocol::{
    AgentActivation, ExternalAgentBinding, ExternalBindingPurpose, ExternalBindingStatus,
    ExternalControllerLease, ExternalInteractionStatus, ExternalRoundStatus,
    ExternalRuntimeDesiredState, ExternalRuntimeObservedState, ExternalRuntimeRegistration,
    ExternalTurnCorrelation, ExternalTurnInputPart, ExternalTurnPhase, ExternalTurnRequestId,
    SessionTurnRequested, TurnInputProvenance,
};

#[derive(Debug, Clone, PartialEq)]
pub struct ExternalActivationRequest {
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
}

impl CoreEngine {
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
        request: ExternalActivationRequest,
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
        if next.phase.is_terminal() {
            next.capacity_lease_id = None;
        }
        self.store.update_external_turn(&next, current.revision)
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

        for round in self.store.list_pending_external_rounds()? {
            if round.expires_at <= *now {
                let mut expired = round.clone();
                expired.status = ExternalRoundStatus::Expired;
                expired.terminal_at = Some(now.clone());
                self.store
                    .update_external_correlated_round(&expired, round.revision)?;
                report.expired_round_ids.push(round.round_id.0);
            }
        }
        Ok(report)
    }
}
