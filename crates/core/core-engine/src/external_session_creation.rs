//! Durable preparation, native attachment, and restart recovery for external sessions.

use super::*;
use crate::external_runtime::{external_profile_prompt_hash, hash_json, hex_sha256};
use rusty_crew_core_protocol::{
    ExternalAgentBinding, ExternalAgentSessionCreationId, ExternalAgentSessionCreationPhase,
    ExternalAgentSessionCreationRecord, ExternalAgentSessionCreationRequest,
    ExternalAgentSessionIdentity, ExternalBindingId, ExternalBindingPurpose, ExternalBindingStatus,
    ExternalControllerContext, ExternalMessageDeliveryPolicy, ExternalRuntimeDesiredState,
    ExternalRuntimeObservedState, ExternalRuntimeRegistration, ProfileRegistryLifecycleStatus,
};
use std::path::{Component, Path};

impl CoreEngine {
    pub(crate) fn reconcile_incomplete_external_bindings(&self) -> CoreResult<()> {
        let incomplete_creation_bindings = self
            .store
            .list_external_agent_session_creations()?
            .into_iter()
            .filter(|creation| {
                creation.phase != ExternalAgentSessionCreationPhase::Ready
                    && creation.native_thread_id.is_none()
            })
            .map(|creation| creation.binding.binding_id)
            .collect::<HashSet<_>>();
        for binding in self
            .store
            .list_external_agent_bindings()?
            .into_iter()
            .filter(|binding| {
                binding.purpose == ExternalBindingPurpose::CrewAgent
                    && binding.status == ExternalBindingStatus::Active
                    && binding.native_thread_id.is_none()
                    && incomplete_creation_bindings.contains(&binding.binding_id)
            })
        {
            let mut paused = binding.clone();
            paused.status = ExternalBindingStatus::Paused;
            paused.updated_at = self.now();
            self.store
                .put_external_agent_binding(&paused, Some(binding.revision))?;

            let Some(session_id) = paused.session_id.as_ref() else {
                continue;
            };
            match self.sessions.get_session(session_id) {
                Ok(session) if session.status != SessionStatus::Archived => {
                    self.archive_session(session_id)?;
                }
                Ok(_) => {}
                Err(error) if error.kind == CoreErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
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
            workspace: Some(SessionWorkspace {
                cwd: cwd.clone(),
                revision: 1,
                updated_at: request.requested_at.clone(),
            }),
            resource_limits: ResourceLimits {
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
            lineage: None,
            effective_config_fingerprint: external_agent_effective_config_fingerprint(
                &runtime, &profile, &request,
            )?,
            status: ExternalBindingStatus::Paused,
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
        }
        binding.status = ExternalBindingStatus::Active;
        binding.updated_at = now.clone();
        binding = self.bind_external_agent(&binding, Some(binding.revision))?;
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
        let mut binding = current.binding.clone();
        if binding.native_thread_id.is_none() && binding.status != ExternalBindingStatus::Paused {
            binding.status = ExternalBindingStatus::Paused;
            binding.updated_at = now.clone();
            binding = self.bind_external_agent(&binding, Some(binding.revision))?;
        }
        let archived_session = if current.session.status == SessionStatus::Archived {
            self.get_session(&current.session.session_id)?
        } else {
            self.archive_session(&current.session.session_id)?
        };
        let mut next = current.clone();
        next.binding = binding;
        next.session = ExternalAgentSessionIdentity {
            session_id: archived_session.session_id,
            agent_id: archived_session.agent_id,
            profile_id: archived_session.profile_id,
            status: archived_session.status,
        };
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
            workspace: Some(SessionWorkspace {
                cwd: current.request.cwd.clone(),
                revision: 1,
                updated_at: current.request.requested_at.clone(),
            }),
            resource_limits: ResourceLimits {
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
