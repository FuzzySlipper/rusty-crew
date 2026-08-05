//! Rust-owned lifecycle and activation rules for complete external runtimes.

use super::*;
use crate::external_controls::{
    external_thread_command_requires_idle, validate_external_control_payload,
    validate_external_thread_command,
};
use rusty_crew_core_protocol::{
    validate_external_runtime_handshake_observation, AgentActivation, AgentMessageDeliveryStatus,
    AgentRoundStatus, AgentRouteResolvedTarget, ExternalAgentBinding,
    ExternalAgentBindingMetadataWrite, ExternalAgentSessionCreationId,
    ExternalAgentSessionCreationPhase, ExternalAgentSessionCreationRecord,
    ExternalAgentSessionCreationRequest, ExternalAgentSessionIdentity, ExternalBindingId,
    ExternalBindingPurpose, ExternalBindingStatus, ExternalCollaborationMode, ExternalControlId,
    ExternalControlKind, ExternalControlReceipt, ExternalControlRequest, ExternalControlStatus,
    ExternalControllerContext, ExternalControllerLease, ExternalInteractionRecord,
    ExternalInteractionStatus, ExternalMessageDeliveryPolicy,
    ExternalRuntimeCompatibilityProbeOutcome, ExternalRuntimeCompatibilityState,
    ExternalRuntimeDesiredState, ExternalRuntimeEventInput, ExternalRuntimeHandshakeDecision,
    ExternalRuntimeHandshakeObservation, ExternalRuntimeId, ExternalRuntimeObservedState,
    ExternalRuntimeRegistration, ExternalRuntimeStateObservation, ExternalTurnCorrelation,
    ExternalTurnInputPart, ExternalTurnPhase, ExternalTurnRequestId, ExternalTurnTerminalError,
    NormalizedExternalRuntimeEvent, ProfileRegistryLifecycleStatus, SessionTurnRequested,
    TurnInputProvenance,
};
use sha2::{Digest, Sha256};
use std::path::{Component, Path};

#[derive(Debug, Clone, PartialEq)]
pub struct AgentActivationRequest {
    pub agent_id: AgentId,
    pub request_id: ExternalTurnRequestId,
    pub idempotency_key: String,
    pub input: Vec<ExternalTurnInputPart>,
    pub collaboration_mode: Option<ExternalCollaborationMode>,
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

#[derive(Debug, Clone)]
pub struct ExternalControllerTurnTransition {
    pub request_id: ExternalTurnRequestId,
    pub next_phase: ExternalTurnPhase,
    pub native_turn_id: Option<String>,
    pub terminal_reason_code: Option<String>,
    pub terminal_error: Option<ExternalTurnTerminalError>,
    pub now: IsoTimestamp,
}

impl CoreEngine {
    pub(crate) fn active_external_session_ids(&self) -> CoreResult<HashSet<SessionId>> {
        Ok(self
            .store
            .list_external_agent_bindings()?
            .into_iter()
            .filter(ExternalAgentBinding::is_routable)
            .filter_map(|binding| binding.session_id)
            .collect())
    }

    pub(crate) fn reactivate_active_external_sessions(&self) -> CoreResult<()> {
        let configs = load_engine_session_configs(&self.store)?
            .into_iter()
            .map(|config| (config.session_id.clone(), config))
            .collect::<HashMap<_, _>>();
        let mut ensured = HashSet::new();
        for binding in self
            .store
            .list_external_agent_bindings()?
            .into_iter()
            .filter(ExternalAgentBinding::is_routable)
        {
            let session_id = binding.session_id.as_ref().expect("routable session id");
            if !ensured.insert(session_id.clone()) {
                continue;
            }
            let config = configs.get(session_id).cloned().ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    format!(
                        "active external binding {} has no persisted session config",
                        binding.binding_id.0
                    ),
                )
            })?;
            if binding.agent_id.as_ref() != Some(&config.agent_id) {
                return Err(CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    format!(
                        "active external binding {} does not match persisted session agent",
                        binding.binding_id.0
                    ),
                ));
            }
            self.ensure_configured_session(config)?;
        }
        Ok(())
    }

    pub(crate) fn archive_active_external_bindings_for_session(
        &self,
        session_id: &SessionId,
        now: &IsoTimestamp,
    ) -> CoreResult<()> {
        for binding in self
            .store
            .list_external_agent_bindings()?
            .into_iter()
            .filter(|binding| {
                binding.purpose == ExternalBindingPurpose::CrewAgent
                    && binding.status == ExternalBindingStatus::Active
                    && binding.session_id.as_ref() == Some(session_id)
            })
        {
            let mut archived = binding.clone();
            archived.status = ExternalBindingStatus::Archived;
            archived.updated_at = now.clone();
            self.store
                .put_external_agent_binding(&archived, Some(binding.revision))?;
        }
        Ok(())
    }

    pub(crate) fn validate_external_controller(
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
        validate_external_runtime_handshake_observation(observation)?;
        self.validate_external_controller(&observation.runtime_id, &observation.controller)?;
        let current = self
            .store
            .get_external_runtime_registration(&observation.runtime_id)?
            .ok_or_else(|| {
                CoreError::new(CoreErrorKind::NotFound, "external runtime was not found")
            })?;
        let has_active_certification = if observation.probe_report.outcome
            == ExternalRuntimeCompatibilityProbeOutcome::Passed
        {
            self.store
                .find_active_external_runtime_certification(
                    &current.kind,
                    &observation.cli_version,
                    &observation.consumed_contract_revision,
                    &observation.probe_report.suite_revision,
                )?
                .is_some()
        } else {
            false
        };
        let classification = crate::external_runtime_compatibility::classify_probe(
            current.desired_state,
            &observation.probe_report,
            has_active_certification,
        );
        let compatibility_state = classification.compatibility_state;
        let reason_code = classification.reason_code.as_deref();
        let mut next = current.clone();
        next.observed_cli_version = Some(observation.cli_version.clone());
        next.consumed_contract_revision = Some(observation.consumed_contract_revision.clone());
        next.compatibility_state = compatibility_state;
        next.last_compatibility_probe = Some(observation.probe_report.clone());
        next.observed_state = match (reason_code, compatibility_state) {
            (None, _) => ExternalRuntimeObservedState::Ready,
            (Some(_), ExternalRuntimeCompatibilityState::Incompatible) => {
                ExternalRuntimeObservedState::Incompatible
            }
            (Some(_), _) => ExternalRuntimeObservedState::Degraded,
        };
        next.observed_reason_code = reason_code.map(str::to_owned);
        next.updated_at = observation.observed_at.clone();
        let saved = self
            .store
            .put_external_runtime_registration(&next, Some(current.revision))?;
        self.record_external_runtime_probe_evidence(observation, current.kind)?;
        Ok(ExternalRuntimeHandshakeDecision {
            accepted: reason_code.is_none(),
            retryable: classification.retryable,
            compatibility_state,
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

    pub fn prepare_external_agent_session_creation(
        &self,
        request: ExternalAgentSessionCreationRequest,
    ) -> CoreResult<ExternalAgentSessionCreationRecord> {
        let request_fingerprint = external_agent_creation_fingerprint(&request)?;
        let creation_id = external_agent_creation_id(&request.idempotency_key)?;
        if let Some(existing) = self
            .store
            .get_external_agent_session_creation(&creation_id)?
        {
            if existing.request_fingerprint != request_fingerprint {
                return Err(CoreError::new(
                    CoreErrorKind::AlreadyExists,
                    "external_agent_creation_idempotency_conflict: idempotency key was reused with a different payload",
                ));
            }
            return self.reconcile_external_agent_session_creation(existing);
        }

        let (runtime, profile, cwd) = self.validate_external_agent_creation_request(&request)?;
        let suffix = external_agent_creation_suffix(&request.idempotency_key)?;
        let session_state = self.ensure_configured_session(SessionConfig {
            session_id: SessionId::new(format!("external-session-{suffix}")),
            agent_id: AgentId::new(format!("external-agent-{suffix}")),
            profile_id: profile.profile_id.clone(),
            kind: SessionKind::Full,
            delegation: None,
            resource_limits: ResourceLimits {
                workdir: Some(cwd.clone()),
                max_duration_ms: None,
                max_delegation_depth: None,
            },
            tool_profile: ToolProfile { tools: Vec::new() },
            history_window: None,
        })?;
        let session = ExternalAgentSessionIdentity {
            session_id: session_state.session_id,
            agent_id: session_state.agent_id,
            profile_id: session_state.profile_id,
            status: session_state.status,
        };
        let now = request.requested_at.clone();
        let binding = ExternalAgentBinding {
            binding_id: ExternalBindingId::new(format!("external-binding-{suffix}")),
            runtime_id: runtime.runtime_id.clone(),
            session_id: Some(session.session_id.clone()),
            agent_id: Some(session.agent_id.clone()),
            profile_id: Some(profile.profile_id.clone()),
            profile_revision: Some(profile.revision),
            profile_prompt_hash: Some(external_profile_prompt_hash(&profile)),
            profile_prompt_snapshot: Some(
                profile
                    .prompt_soul_markdown
                    .as_deref()
                    .unwrap_or_default()
                    .trim()
                    .to_owned(),
            ),
            dynamic_tool_catalog_fingerprint: None,
            message_delivery_policy: external_message_delivery_policy(&profile)?,
            purpose: ExternalBindingPurpose::CrewAgent,
            native_thread_id: None,
            cwd: Some(cwd),
            label: None,
            task_ref: None,
            effective_config_fingerprint: external_agent_effective_config_fingerprint(
                &runtime, &profile, &request,
            )?,
            status: ExternalBindingStatus::Active,
            revision: 0,
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        let prepared = ExternalAgentSessionCreationRecord {
            creation_id,
            request,
            request_fingerprint,
            session,
            binding,
            native_thread_source: format!("rusty-crew:{suffix}"),
            native_thread_id: None,
            phase: ExternalAgentSessionCreationPhase::Prepared,
            reason_code: None,
            reason_message: None,
            revision: 1,
            created_at: now.clone(),
            updated_at: now,
        };
        let prepared = self
            .store
            .create_external_agent_session_creation(&prepared)?;
        self.reconcile_external_agent_session_creation(prepared)
    }

    pub fn mark_external_agent_session_native_starting(
        &self,
        controller: &ExternalControllerContext,
        creation_id: &ExternalAgentSessionCreationId,
        expected_revision: u64,
        now: IsoTimestamp,
    ) -> CoreResult<ExternalAgentSessionCreationRecord> {
        let current = self.require_external_agent_session_creation(creation_id)?;
        self.validate_external_controller(&current.request.runtime_id, controller)?;
        if current.phase == ExternalAgentSessionCreationPhase::Ready {
            return Ok(current);
        }
        if current.revision != expected_revision {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "external_agent_creation_revision_conflict: expected {}, found {}",
                    expected_revision, current.revision
                ),
            ));
        }
        let mut next = current.clone();
        next.phase = ExternalAgentSessionCreationPhase::NativeStarting;
        next.reason_code = None;
        next.reason_message = None;
        next.updated_at = now;
        self.store
            .update_external_agent_session_creation(&next, expected_revision)
    }

    pub fn complete_external_agent_session_creation(
        &self,
        controller: &ExternalControllerContext,
        creation_id: &ExternalAgentSessionCreationId,
        expected_revision: u64,
        native_thread_id: String,
        now: IsoTimestamp,
    ) -> CoreResult<ExternalAgentSessionCreationRecord> {
        let current = self.require_external_agent_session_creation(creation_id)?;
        self.validate_external_controller(&current.request.runtime_id, controller)?;
        if current.phase == ExternalAgentSessionCreationPhase::Ready {
            if current.native_thread_id.as_deref() == Some(native_thread_id.as_str()) {
                return Ok(current);
            }
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                "external_agent_creation_native_thread_conflict: creation is already bound to a different native thread",
            ));
        }
        if current.revision != expected_revision {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "external_agent_creation_revision_conflict: expected {}, found {}",
                    expected_revision, current.revision
                ),
            ));
        }
        let mut binding = current.binding.clone();
        if let Some(existing_thread_id) = binding.native_thread_id.as_deref() {
            if existing_thread_id != native_thread_id {
                return Err(CoreError::new(
                    CoreErrorKind::AlreadyExists,
                    "external_agent_creation_native_thread_conflict: binding is already correlated to a different native thread",
                ));
            }
        } else {
            binding.native_thread_id = Some(native_thread_id.clone());
            binding.updated_at = now.clone();
            binding = self.bind_external_agent(&binding, Some(binding.revision))?;
        }
        let mut next = current.clone();
        next.binding = binding;
        next.native_thread_id = Some(native_thread_id);
        next.phase = ExternalAgentSessionCreationPhase::Ready;
        next.reason_code = None;
        next.reason_message = None;
        next.updated_at = now;
        self.store
            .update_external_agent_session_creation(&next, expected_revision)
    }

    pub fn record_external_agent_session_creation_failure(
        &self,
        controller: &ExternalControllerContext,
        creation_id: &ExternalAgentSessionCreationId,
        expected_revision: u64,
        reason_code: String,
        reason_message: String,
        now: IsoTimestamp,
    ) -> CoreResult<ExternalAgentSessionCreationRecord> {
        let current = self.require_external_agent_session_creation(creation_id)?;
        self.validate_external_controller(&current.request.runtime_id, controller)?;
        if current.phase == ExternalAgentSessionCreationPhase::Ready {
            return Ok(current);
        }
        if current.revision != expected_revision {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "external_agent_creation_revision_conflict: creation changed before failure could be recorded",
            ));
        }
        let mut next = current.clone();
        next.phase = ExternalAgentSessionCreationPhase::RecoveryRequired;
        next.reason_code = Some(reason_code);
        next.reason_message = Some(reason_message);
        next.updated_at = now;
        self.store
            .update_external_agent_session_creation(&next, expected_revision)
    }

    fn require_external_agent_session_creation(
        &self,
        creation_id: &ExternalAgentSessionCreationId,
    ) -> CoreResult<ExternalAgentSessionCreationRecord> {
        self.store
            .get_external_agent_session_creation(creation_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    "external_agent_creation_not_found: external agent session creation was not found",
                )
            })
    }

    fn reconcile_external_agent_session_creation(
        &self,
        current: ExternalAgentSessionCreationRecord,
    ) -> CoreResult<ExternalAgentSessionCreationRecord> {
        if current.phase == ExternalAgentSessionCreationPhase::Ready {
            return Ok(current);
        }
        self.validate_external_agent_creation_request(&current.request)?;
        let session_state = self.ensure_configured_session(SessionConfig {
            session_id: current.session.session_id.clone(),
            agent_id: current.session.agent_id.clone(),
            profile_id: current.session.profile_id.clone(),
            kind: SessionKind::Full,
            delegation: None,
            resource_limits: ResourceLimits {
                workdir: Some(current.request.cwd.clone()),
                max_duration_ms: None,
                max_delegation_depth: None,
            },
            tool_profile: ToolProfile { tools: Vec::new() },
            history_window: None,
        })?;
        let session = ExternalAgentSessionIdentity {
            session_id: session_state.session_id,
            agent_id: session_state.agent_id,
            profile_id: session_state.profile_id,
            status: session_state.status,
        };
        let binding = match self.get_external_binding(&current.binding.binding_id)? {
            Some(binding) => {
                if binding.runtime_id != current.binding.runtime_id
                    || binding.session_id != current.binding.session_id
                    || binding.agent_id != current.binding.agent_id
                {
                    return Err(CoreError::new(
                        CoreErrorKind::AlreadyExists,
                        "external_agent_creation_binding_conflict: generated binding identity is already in use",
                    ));
                }
                binding
            }
            None => self.bind_external_agent(&current.binding, None)?,
        };
        let mut next = current.clone();
        next.session = session;
        next.binding = binding.clone();
        if let Some(native_thread_id) = binding.native_thread_id {
            next.native_thread_id = Some(native_thread_id);
            next.phase = ExternalAgentSessionCreationPhase::Ready;
            next.reason_code = None;
            next.reason_message = None;
        } else if current.phase == ExternalAgentSessionCreationPhase::Prepared {
            next.phase = ExternalAgentSessionCreationPhase::BindingReady;
        }
        if next == current {
            return Ok(current);
        }
        next.updated_at = self.now();
        self.store
            .update_external_agent_session_creation(&next, current.revision)
    }

    fn validate_external_agent_creation_request(
        &self,
        request: &ExternalAgentSessionCreationRequest,
    ) -> CoreResult<(ExternalRuntimeRegistration, ProfileRegistryRecord, String)> {
        if request.idempotency_key.trim().is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "external_agent_creation_idempotency_key_required: idempotencyKey is required",
            ));
        }
        let cwd = normalized_external_agent_cwd(&request.cwd)?;
        let runtime = self
            .store
            .get_external_runtime_registration(&request.runtime_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    "external_agent_creation_runtime_unavailable: external runtime was not found",
                )
            })?;
        if runtime.desired_state != ExternalRuntimeDesiredState::Enabled
            || runtime.observed_state != ExternalRuntimeObservedState::Ready
        {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "external_agent_creation_runtime_unavailable: external runtime is not ready",
            ));
        }
        let lease = self
            .store
            .get_external_controller_lease(&request.runtime_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::ActionRejected,
                    "external_agent_creation_runtime_unavailable: external runtime has no controller lease",
                )
            })?;
        if lease.expires_at <= self.now() {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "external_agent_creation_runtime_unavailable: external runtime controller lease expired",
            ));
        }
        let profile = self
            .get_profile_registry_record(&request.profile_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    "external_agent_creation_profile_invalid: profile was not found",
                )
            })?;
        if profile.lifecycle_status != ProfileRegistryLifecycleStatus::Active {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "external_agent_creation_profile_invalid: profile is not active",
            ));
        }
        if profile
            .default_session_kind
            .as_ref()
            .is_some_and(|kind| kind != &SessionKind::Full)
        {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "external_agent_creation_profile_invalid: external agents require a full-session profile",
            ));
        }
        Ok((runtime, profile, cwd))
    }

    pub fn get_external_turn(
        &self,
        request_id: &ExternalTurnRequestId,
    ) -> CoreResult<Option<ExternalTurnCorrelation>> {
        self.store.get_external_turn(request_id)
    }

    pub fn list_external_turns_for_native_thread(
        &self,
        runtime_id: &ExternalRuntimeId,
        native_thread_id: &str,
    ) -> CoreResult<Vec<ExternalTurnCorrelation>> {
        self.store
            .list_external_turns_for_native_thread(runtime_id, native_thread_id)
    }

    pub fn list_active_external_turns(&self) -> CoreResult<Vec<ExternalTurnCorrelation>> {
        self.store.list_nonterminal_external_turns()
    }

    pub fn expire_external_turn_dispatches(
        &self,
        now: &IsoTimestamp,
    ) -> CoreResult<Vec<ExternalTurnCorrelation>> {
        let mut expired = Vec::new();
        for turn in self.store.list_nonterminal_external_turns()? {
            if turn.phase != ExternalTurnPhase::Accepted
                || turn
                    .request
                    .expires_at
                    .as_ref()
                    .is_none_or(|expires_at| expires_at > now)
            {
                continue;
            }
            expired.push(self.transition_external_turn(
                &turn.request.request_id,
                ExternalTurnPhase::Failed,
                None,
                Some("external_turn_dispatch_expired".into()),
                now.clone(),
            )?);
        }
        Ok(expired)
    }

    pub fn submit_external_control(
        &self,
        request: ExternalControlRequest,
    ) -> CoreResult<ExternalControlReceipt> {
        validate_external_control_payload(&request)?;
        let mut request_fingerprint_input = serde_json::json!({
            "bindingId": request.binding_id,
            "expectedNativeTurnId": request.expected_native_turn_id,
            "kind": request.kind,
            "payload": request.payload,
        });
        if request.kind != ExternalControlKind::ExecuteThreadCommand {
            request_fingerprint_input["expectedBindingRevision"] =
                serde_json::json!(request.expected_binding_revision);
        }
        let request_fingerprint = hash_json(
            &request_fingerprint_input,
            "fingerprint external control request",
        )?;
        if let Some(existing) = self
            .store
            .get_external_control_receipt(&request.control_id)?
        {
            if existing.request.idempotency_key == request.idempotency_key
                && existing.request_fingerprint == request_fingerprint
            {
                return Ok(existing);
            }
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                "external_control_idempotency_conflict: external control identity conflicts with a different request",
            ));
        }
        let binding = self
            .store
            .get_external_agent_binding(&request.binding_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    "external_control_binding_not_found",
                )
            })?;
        if binding.revision != request.expected_binding_revision {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "external_control_binding_revision_conflict: external binding revision mismatch: expected {}, found {}",
                    request.expected_binding_revision, binding.revision
                ),
            ));
        }
        if binding.status != ExternalBindingStatus::Active {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "external_control_binding_inactive: external control requires an active binding",
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
                "external_control_native_turn_required: mid-turn external control requires expected_native_turn_id",
            ));
        }
        if request.kind == ExternalControlKind::ExecuteThreadCommand {
            if binding.native_thread_id.is_none() {
                return Err(CoreError::new(
                    CoreErrorKind::ActionRejected,
                    "external_control_thread_unbound: external thread command requires a bound native thread",
                ));
            }
            validate_external_thread_command(&request.payload)?;
        }
        if (request.kind == ExternalControlKind::CompactThread
            || (request.kind == ExternalControlKind::ExecuteThreadCommand
                && external_thread_command_requires_idle(&request.payload)))
            && active_turn.is_some()
        {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "external_control_thread_busy: external thread command requires an idle binding",
            ));
        }
        if let Some(expected_native_turn_id) = &request.expected_native_turn_id {
            let matches_active_turn = active_turn
                .as_ref()
                .is_some_and(|turn| turn.native_turn_id.as_ref() == Some(expected_native_turn_id));
            if !matches_active_turn {
                return Err(CoreError::new(
                    CoreErrorKind::ActionRejected,
                    "external_control_native_turn_conflict: expected native turn is not the binding's active turn",
                ));
            }
        }
        let receipt = ExternalControlReceipt {
            request,
            request_fingerprint,
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

    pub fn query_external_runtime_event_tail(
        &self,
        runtime_id: &ExternalRuntimeId,
        limit: u32,
    ) -> CoreResult<Vec<NormalizedExternalRuntimeEvent>> {
        self.store
            .query_external_runtime_event_tail(runtime_id, limit)
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
            if binding.profile_id.as_ref() != Some(&session.profile_id) {
                return Err(CoreError::new(
                    CoreErrorKind::ActionRejected,
                    "external binding profile_id does not match the bound Crew session",
                ));
            }
            if session.status == SessionStatus::Archived
                && binding.status != ExternalBindingStatus::Archived
            {
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

    pub fn update_external_binding_metadata(
        &self,
        write: &ExternalAgentBindingMetadataWrite,
    ) -> CoreResult<ExternalAgentBinding> {
        write.validate()?;
        let current = self
            .store
            .get_external_agent_binding(&write.binding_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    format!("external binding {} was not found", write.binding_id.0),
                )
            })?;
        if current.revision != write.expected_revision {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "external_binding_metadata_revision_conflict: expected {}, found {}",
                    write.expected_revision, current.revision
                ),
            ));
        }
        let mut next = current;
        next.label = write.label.clone();
        next.task_ref = write.task_ref.clone();
        next.updated_at = write.updated_at.clone();
        self.store
            .put_external_agent_binding(&next, Some(write.expected_revision))
    }

    pub fn activate_agent_execution(
        &self,
        request: AgentActivationRequest,
    ) -> CoreResult<AgentActivation> {
        self.activate_agent_execution_inner(request, None, None)
    }

    pub(crate) fn activate_agent_execution_for_resolved_target(
        &self,
        request: AgentActivationRequest,
        target: &AgentRouteResolvedTarget,
    ) -> CoreResult<AgentActivation> {
        self.activate_agent_execution_inner(request, None, Some(target))
    }

    pub(crate) fn activate_agent_execution_inner(
        &self,
        request: AgentActivationRequest,
        queued_claim: Option<(&str, &IsoTimestamp)>,
        resolved_target: Option<&AgentRouteResolvedTarget>,
    ) -> CoreResult<AgentActivation> {
        let (session, binding) = if let Some(target) = resolved_target {
            let Some(target) =
                self.resolve_agent_route_activation_target(&request.agent_id, target)?
            else {
                return Ok(AgentActivation::Rejected {
                    reason_code: "agent_route_activation_target_changed".into(),
                });
            };
            target
        } else {
            let session = self.sessions.get_session_by_agent(&request.agent_id)?;
            let binding = self
                .store
                .get_external_binding_for_agent(&request.agent_id)?;
            (session, binding)
        };
        let Some(binding) = binding else {
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
        if let Some(active_turn) = self
            .store
            .list_nonterminal_external_turns()?
            .into_iter()
            .find(|turn| turn.request.session_id == session.session_id)
        {
            match binding.message_delivery_policy {
                ExternalMessageDeliveryPolicy::ImmediateSteer
                    if active_turn.phase == ExternalTurnPhase::Active
                        && active_turn.native_turn_id.is_some() =>
                {
                    return Ok(AgentActivation::ExternalTurnSteerRequested {
                        session_id: session.session_id,
                        request_id: active_turn.request.request_id,
                        binding_id: binding.binding_id,
                        native_thread_id: active_turn.native_thread_id,
                        native_turn_id: active_turn.native_turn_id.expect("checked above"),
                        message_text: request
                            .input
                            .iter()
                            .find_map(|part| match part {
                                ExternalTurnInputPart::Text { text } => Some(text.clone()),
                                _ => None,
                            })
                            .unwrap_or_default(),
                    });
                }
                ExternalMessageDeliveryPolicy::ImmediateSteer
                | ExternalMessageDeliveryPolicy::SerialNextTurn => {}
            }
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
                collaboration_mode: request.collaboration_mode,
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
            terminal_error: None,
            revision: 1,
            updated_at: request.created_at,
        };
        let saved = if let Some((queued_message_id, claim_at)) = queued_claim {
            let Some(saved) = self.store.promote_queued_message_to_external_turn(
                queued_message_id,
                claim_at,
                &turn,
            )?
            else {
                return Ok(AgentActivation::QueuedForNextTurn {
                    session_id: turn.request.session_id,
                    queue_id: queued_message_id.to_string(),
                });
            };
            saved
        } else {
            self.store.create_external_turn(&turn)?
        };
        Ok(AgentActivation::ExternalTurnRequested {
            session_id: saved.request.session_id,
            request_id: saved.request.request_id,
            binding_id: saved.request.binding_id,
        })
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

fn external_message_delivery_policy(
    profile: &ProfileRegistryRecord,
) -> CoreResult<ExternalMessageDeliveryPolicy> {
    let value = profile
        .active_runtime_settings_json
        .get("externalMessageDeliveryPolicy")
        .or_else(|| {
            profile
                .active_runtime_settings_json
                .get("external_message_delivery_policy")
        })
        .and_then(serde_json::Value::as_str)
        .unwrap_or("immediate_steer");
    match value {
        "immediate_steer" => Ok(ExternalMessageDeliveryPolicy::ImmediateSteer),
        "serial_next_turn" => Ok(ExternalMessageDeliveryPolicy::SerialNextTurn),
        _ => Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "external_message_delivery_policy_invalid: expected immediate_steer or serial_next_turn",
        )),
    }
}

fn external_agent_creation_id(idempotency_key: &str) -> CoreResult<ExternalAgentSessionCreationId> {
    Ok(ExternalAgentSessionCreationId::new(format!(
        "external-creation-{}",
        external_agent_creation_suffix(idempotency_key)?
    )))
}

fn external_agent_creation_suffix(idempotency_key: &str) -> CoreResult<String> {
    let idempotency_key = idempotency_key.trim();
    if idempotency_key.is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "external_agent_creation_idempotency_key_required: idempotencyKey is required",
        ));
    }
    Ok(hex_sha256(idempotency_key.as_bytes())[..24].to_owned())
}

fn external_agent_creation_fingerprint(
    request: &ExternalAgentSessionCreationRequest,
) -> CoreResult<String> {
    let canonical = serde_json::json!({
        "runtimeId": request.runtime_id,
        "profileId": request.profile_id,
        "cwd": request.cwd,
        "taskRef": request.task_ref,
        "label": request.label,
    });
    hash_json(
        &canonical,
        "fingerprint external agent session creation request",
    )
}

fn external_agent_effective_config_fingerprint(
    runtime: &ExternalRuntimeRegistration,
    profile: &ProfileRegistryRecord,
    request: &ExternalAgentSessionCreationRequest,
) -> CoreResult<String> {
    let canonical = serde_json::json!({
        "runtimeId": runtime.runtime_id,
        "runtimeRevision": runtime.revision,
        "profileId": profile.profile_id,
        "profileRevision": profile.revision,
        "cwd": request.cwd,
        "taskRef": request.task_ref,
    });
    hash_json(
        &canonical,
        "fingerprint external agent effective configuration",
    )
}

pub(crate) fn external_profile_prompt_hash(profile: &ProfileRegistryRecord) -> String {
    let soul = profile
        .prompt_soul_markdown
        .as_deref()
        .map(str::trim)
        .unwrap_or_default();
    hex_sha256(soul.as_bytes())
}

fn hash_json(value: &serde_json::Value, action: &str) -> CoreResult<String> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        CoreError::new(
            CoreErrorKind::InternalError,
            format!("failed to {action}: {error}"),
        )
    })?;
    Ok(hex_sha256(&bytes))
}

fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn normalized_external_agent_cwd(raw: &str) -> CoreResult<String> {
    if raw.trim().is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "external_agent_creation_cwd_invalid: cwd is required",
        ));
    }
    let path = Path::new(raw);
    if !path.is_absolute() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "external_agent_creation_cwd_invalid: cwd must be an absolute normalized path",
        ));
    }
    let mut normalized = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            Component::RootDir => normalized.push(Path::new("/")),
            Component::Normal(part) => normalized.push(part),
            Component::CurDir | Component::ParentDir | Component::Prefix(_) => {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "external_agent_creation_cwd_invalid: cwd must not contain relative path components",
                ));
            }
        }
    }
    let normalized = normalized.to_string_lossy().into_owned();
    if normalized != raw {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "external_agent_creation_cwd_invalid: cwd must already be normalized",
        ));
    }
    Ok(normalized)
}
