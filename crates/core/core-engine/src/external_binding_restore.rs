//! Explicit recovery for archived external bindings and their exact Crew sessions.

use super::*;
use rusty_crew_core_protocol::{
    ExternalAgentBindingRestoreOutcome, ExternalAgentBindingRestoreReceipt,
    ExternalAgentBindingRestoreRequest, ExternalBindingPurpose, ExternalBindingStatus,
    ExternalRuntimeDesiredState, ProfileRegistryLifecycleStatus,
};

impl CoreEngine {
    pub fn restore_external_agent_binding(
        &self,
        request: &ExternalAgentBindingRestoreRequest,
    ) -> CoreResult<ExternalAgentBindingRestoreReceipt> {
        request.validate()?;
        let _lifecycle_guard = self.agent_route_lifecycle_lock.lock().map_err(|_| {
            CoreError::new(
                CoreErrorKind::InternalError,
                "external_binding_restore_lock_poisoned: agent route lifecycle lock poisoned",
            )
        })?;
        let current = self
            .store
            .get_external_agent_binding(&request.binding_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    format!(
                        "external_binding_restore_not_found: external binding {} was not found",
                        request.binding_id.0
                    ),
                )
            })?;
        if current.revision != request.expected_binding_revision {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "external_binding_restore_revision_conflict: expected {}, found {}",
                    request.expected_binding_revision, current.revision
                ),
            ));
        }
        if current.purpose != ExternalBindingPurpose::CrewAgent {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "external_binding_restore_identity_conflict: only crew_agent bindings can be restored",
            ));
        }
        let runtime = self
            .store
            .get_external_runtime_registration(&current.runtime_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    "external_binding_restore_runtime_unavailable: bound external runtime was not found",
                )
            })?;
        if runtime.desired_state != ExternalRuntimeDesiredState::Enabled {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "external_binding_restore_runtime_unavailable: bound external runtime is disabled",
            ));
        }
        if current.session_id.as_ref() != Some(&request.expected_session_id)
            || current.agent_id.as_ref() != Some(&request.expected_agent_id)
            || current.profile_id.as_ref() != Some(&request.expected_profile_id)
            || current.native_thread_id.as_deref()
                != Some(request.expected_native_thread_id.as_str())
        {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "external_binding_restore_identity_conflict: binding identity no longer matches the requested session, agent, profile, or native thread",
            ));
        }
        if current.status == ExternalBindingStatus::Paused {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "external_binding_restore_status_conflict: paused bindings require an explicit lifecycle decision",
            ));
        }
        if self
            .store
            .list_nonterminal_external_turns()?
            .iter()
            .any(|turn| turn.request.binding_id == request.binding_id)
            || self
                .store
                .list_pending_external_interactions()?
                .iter()
                .any(|interaction| interaction.binding_id == request.binding_id)
        {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "external_binding_restore_work_conflict: binding has nonterminal turns or pending interactions",
            ));
        }

        let config = load_engine_session_configs(&self.store)?
            .into_iter()
            .find(|config| config.session_id == request.expected_session_id)
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    "external_binding_restore_session_config_missing: persisted session config was not found",
                )
            })?;
        if config.agent_id != request.expected_agent_id
            || config.profile_id != request.expected_profile_id
            || config.kind != SessionKind::Full
        {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "external_binding_restore_identity_conflict: persisted session config does not match the binding identity",
            ));
        }
        let session = self
            .sessions
            .get_session(&request.expected_session_id)
            .map_err(|error| {
                let error_kind = error.kind.clone();
                CoreError::new(
                    error_kind,
                    format!("external_binding_restore_session_config_missing: {error}"),
                )
            })?;
        if session.agent_id != request.expected_agent_id
            || session.profile_id != request.expected_profile_id
            || session.kind != SessionKind::Full
        {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "external_binding_restore_identity_conflict: hydrated session does not match the binding identity",
            ));
        }
        let profile = self
            .store
            .get_profile_registry_record(&request.expected_profile_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    "external_binding_restore_profile_missing: bound profile was not found",
                )
            })?;
        if profile.lifecycle_status != ProfileRegistryLifecycleStatus::Active {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "external_binding_restore_profile_inactive: bound profile is not active",
            ));
        }
        let prompt_hash = crate::external_runtime::external_profile_prompt_hash(&profile);
        if current.profile_prompt_hash.as_deref() != Some(prompt_hash.as_str()) {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "external_binding_restore_prompt_conflict: profile instructions changed; refresh into a new native thread",
            ));
        }

        let profile_revision_updated = current.profile_revision != Some(profile.revision);
        if current.status == ExternalBindingStatus::Active
            && session.status != SessionStatus::Archived
            && !profile_revision_updated
        {
            return Ok(ExternalAgentBindingRestoreReceipt {
                outcome: ExternalAgentBindingRestoreOutcome::AlreadyActive,
                binding: current,
                session,
                profile_revision_updated: false,
            });
        }

        self.validate_agent_id_route_reservation(&config.agent_id)?;
        let session_was_archived = session.status == SessionStatus::Archived;
        let restored_session = if session_was_archived {
            self.expire_body_follow_up_messages(&request.restored_at)?;
            self.sessions.apply_config(&config)?;
            let restored = self
                .sessions
                .reactivate_session(&config.session_id, request.restored_at.clone())?;
            if let Err(error) = save_engine_session(&self.store, &restored) {
                let _ = self
                    .sessions
                    .archive_session(&config.session_id, request.restored_at.clone());
                return Err(CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    format!("external_binding_restore_session_persist_failed: {error}"),
                ));
            }
            restored
        } else if session.status != SessionStatus::Archived {
            session
        } else {
            return Err(CoreError::new(
                CoreErrorKind::InternalError,
                "external_binding_restore_session_status_conflict: session status changed during restore",
            ));
        };

        let mut restored_binding = current.clone();
        restored_binding.status = ExternalBindingStatus::Active;
        restored_binding.profile_revision = Some(profile.revision);
        restored_binding.updated_at = request.restored_at.clone();
        let restored_binding = match self
            .store
            .put_external_agent_binding(&restored_binding, Some(request.expected_binding_revision))
        {
            Ok(binding) => binding,
            Err(error) => {
                let error_kind = error.kind.clone();
                if session_was_archived {
                    if let Ok(archived) = self
                        .sessions
                        .archive_session(&config.session_id, request.restored_at.clone())
                    {
                        let _ = save_engine_session(&self.store, &archived);
                    }
                }
                return Err(CoreError::new(
                    error_kind,
                    format!("external_binding_restore_binding_persist_failed: {error}"),
                ));
            }
        };
        Ok(ExternalAgentBindingRestoreReceipt {
            outcome: ExternalAgentBindingRestoreOutcome::Restored,
            binding: restored_binding,
            session: restored_session,
            profile_revision_updated,
        })
    }
}
