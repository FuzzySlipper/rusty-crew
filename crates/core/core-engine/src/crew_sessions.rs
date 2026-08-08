use super::*;
use rusty_crew_core_protocol::{
    CrewAgentSessionCreationOutcome, CrewAgentSessionCreationRecord,
    CrewAgentSessionCreationRequest, ProfileRegistryLifecycleStatus, SessionWorkspace,
};
use rusty_crew_core_session::normalize_session_workspace_cwd;
use sha2::{Digest, Sha256};

const CREW_SESSION_CREATION_FINGERPRINT_KEY: &str = "crewSessionCreationFingerprint";

impl CoreEngine {
    pub fn create_crew_agent_session(
        &self,
        request: &CrewAgentSessionCreationRequest,
    ) -> CoreResult<CrewAgentSessionCreationRecord> {
        let idempotency_key = request.idempotency_key.trim();
        if idempotency_key.is_empty() {
            return Err(creation_error(
                CoreErrorKind::InvalidInput,
                "idempotency_key_required",
                "idempotencyKey is required",
            ));
        }
        if request.requested_at.trim().is_empty() {
            return Err(creation_error(
                CoreErrorKind::InvalidInput,
                "requested_at_required",
                "requestedAt is required",
            ));
        }
        let workspace_cwd = normalize_session_workspace_cwd(&request.workspace_cwd)
            .map_err(|error| creation_error(error.kind, "workspace_cwd_invalid", error.message))?;

        let fingerprint = creation_fingerprint(request)?;
        let session_suffix = sha256_hex(idempotency_key.as_bytes())[..24].to_owned();
        let profile =
            RuntimeServiceDataStore::get_profile_registry_record(&self.store, &request.profile_id)?
                .ok_or_else(|| {
                    creation_error(
                        CoreErrorKind::NotFound,
                        "profile_not_found",
                        format!("profile {} was not found", request.profile_id),
                    )
                })?;
        let agent_id = profile.agent_id.clone().ok_or_else(|| {
            creation_error(
                CoreErrorKind::InvalidInput,
                "profile_agent_missing",
                format!("profile {} has no agent identity", profile.profile_id),
            )
        })?;
        let session_id = SessionId::new(format!("crew-session-{session_suffix}"));

        let _lifecycle_guard = self.agent_route_lifecycle_lock.lock().map_err(|_| {
            CoreError::new(
                CoreErrorKind::InternalError,
                "agent route lifecycle lock poisoned",
            )
        })?;

        let sessions = self.sessions.all_sessions()?;
        if let Some(existing) = sessions
            .iter()
            .find(|session| session.session_id == session_id)
        {
            if existing.profile_id != request.profile_id
                || existing.agent_id != agent_id
                || existing.kind != SessionKind::Full
                || creation_fingerprint_for_session(&profile, &session_id).as_deref()
                    != Some(fingerprint.as_str())
            {
                return Err(creation_error(
                    CoreErrorKind::AlreadyExists,
                    "idempotency_conflict",
                    "idempotencyKey was already used for a different Crew session intent",
                ));
            }
            let (session, outcome, profile_revision) = if existing.status == SessionStatus::Archived
            {
                if profile.lifecycle_status != ProfileRegistryLifecycleStatus::Active {
                    return Err(creation_error(
                        CoreErrorKind::ActionRejected,
                        "profile_inactive",
                        format!(
                            "profile {} is {:?}; active profile required",
                            profile.profile_id, profile.lifecycle_status
                        ),
                    ));
                }
                let config = load_engine_session_configs(&self.store)?
                    .into_iter()
                    .find(|config| config.session_id == session_id)
                    .ok_or_else(|| {
                        creation_error(
                            CoreErrorKind::NotFound,
                            "session_config_missing",
                            "idempotent Crew session is missing its persisted configuration",
                        )
                    })?;
                self.validate_agent_id_route_reservation(&agent_id)?;
                self.sessions.apply_config(&config)?;
                let session = self.sessions.reactivate_session(&session_id, self.now())?;
                save_engine_session(&self.store, &session)?;
                let updated_profile = match update_profile_session_ref_status(
                    &self.store,
                    &profile,
                    &session_id,
                    "active",
                    &request.requested_at,
                ) {
                    Ok(updated) => updated,
                    Err(error) => {
                        let archived = self.sessions.archive_session(&session_id, self.now())?;
                        save_engine_session(&self.store, &archived)?;
                        return Err(error);
                    }
                };
                (
                    session,
                    CrewAgentSessionCreationOutcome::Recovered,
                    updated_profile.revision,
                )
            } else {
                (
                    existing.clone(),
                    CrewAgentSessionCreationOutcome::Replayed,
                    profile.revision,
                )
            };
            return Ok(CrewAgentSessionCreationRecord {
                request_fingerprint: fingerprint,
                profile_revision,
                template_session_id: persisted_template_session_id(
                    &sessions,
                    &request.profile_id,
                    &agent_id,
                    Some(&session_id),
                ),
                outcome,
                session,
            });
        }

        if profile.lifecycle_status != ProfileRegistryLifecycleStatus::Active {
            return Err(creation_error(
                CoreErrorKind::ActionRejected,
                "profile_inactive",
                format!(
                    "profile {} is {:?}; active profile required",
                    profile.profile_id, profile.lifecycle_status
                ),
            ));
        }
        if profile.revision != request.expected_profile_revision {
            return Err(creation_error(
                CoreErrorKind::ActionRejected,
                "profile_revision_conflict",
                format!(
                    "profile {} revision mismatch: expected {}, found {}",
                    profile.profile_id, request.expected_profile_revision, profile.revision
                ),
            ));
        }
        if profile.default_session_kind != Some(SessionKind::Full) {
            return Err(creation_error(
                CoreErrorKind::InvalidInput,
                "profile_session_kind_invalid",
                "fresh Crew brain sessions require a full-session profile",
            ));
        }
        self.validate_agent_id_route_reservation(&agent_id)?;

        let configs = load_engine_session_configs(&self.store)?;
        let template_session_id =
            persisted_template_session_id(&sessions, &request.profile_id, &agent_id, None);
        let template = template_session_id.as_ref().and_then(|template_id| {
            configs
                .iter()
                .find(|config| &config.session_id == template_id)
        });
        let config = SessionConfig {
            session_id: session_id.clone(),
            agent_id: agent_id.clone(),
            profile_id: request.profile_id.clone(),
            kind: SessionKind::Full,
            delegation: None,
            workspace: Some(SessionWorkspace {
                cwd: workspace_cwd,
                revision: 1,
                updated_at: request.requested_at.clone(),
            }),
            resource_limits: template
                .map(|config| config.resource_limits.clone())
                .unwrap_or(ResourceLimits {
                    max_duration_ms: None,
                    max_delegation_depth: None,
                }),
            tool_profile: template
                .map(|config| config.tool_profile.clone())
                .unwrap_or(ToolProfile { tools: vec![] }),
            history_window: template.and_then(|config| config.history_window.clone()),
        };
        let session = self.sessions.create_session(config.clone(), self.now())?;
        save_engine_session_with_config(&self.store, &session, &config)?;
        self.bus.publish(CoreEvent::SessionCreated {
            state: Box::new(session.clone()),
        })?;

        let mut updated_profile = profile.clone();
        updated_profile.derived_runtime_refs.push(
            rusty_crew_core_protocol::ProfileRegistryDerivedRuntimeRef {
                ref_kind: "session".to_string(),
                ref_id: session_id.to_string(),
                status: "active".to_string(),
                updated_at: Some(request.requested_at.clone()),
                metadata_json: json!({
                    (CREW_SESSION_CREATION_FINGERPRINT_KEY): fingerprint,
                }),
            },
        );
        let write = profile_registry_write(&updated_profile, request.requested_at.clone());
        let updated_profile = match RuntimeServiceDataStore::update_profile_registry_record(
            &self.store,
            &rusty_crew_core_protocol::ProfileRegistryUpdate {
                write,
                expected_revision: profile.revision,
            },
        ) {
            Ok(updated) => updated,
            Err(error) => {
                let archived = self
                    .sessions
                    .archive_session(&session.session_id, self.now())?;
                save_engine_session(&self.store, &archived)?;
                self.bus.publish(CoreEvent::SessionArchived {
                    session_id: session.session_id.clone(),
                })?;
                return Err(error);
            }
        };

        Ok(CrewAgentSessionCreationRecord {
            request_fingerprint: fingerprint,
            profile_revision: updated_profile.revision,
            template_session_id,
            outcome: CrewAgentSessionCreationOutcome::Created,
            session,
        })
    }
}

fn update_profile_session_ref_status(
    store: &CoreCoordinationStore,
    profile: &ProfileRegistryRecord,
    session_id: &SessionId,
    status: &str,
    now: &str,
) -> CoreResult<ProfileRegistryRecord> {
    let mut updated = profile.clone();
    let reference = updated
        .derived_runtime_refs
        .iter_mut()
        .find(|reference| reference.ref_kind == "session" && reference.ref_id == session_id.0)
        .ok_or_else(|| {
            creation_error(
                CoreErrorKind::NotFound,
                "session_ref_missing",
                format!(
                    "profile {} has no session ref for {session_id}",
                    profile.profile_id
                ),
            )
        })?;
    reference.status = status.to_string();
    reference.updated_at = Some(now.to_string());
    RuntimeServiceDataStore::update_profile_registry_record(
        store,
        &rusty_crew_core_protocol::ProfileRegistryUpdate {
            write: profile_registry_write(&updated, now.to_string()),
            expected_revision: profile.revision,
        },
    )
}

fn profile_registry_write(profile: &ProfileRegistryRecord, now: String) -> ProfileRegistryWrite {
    ProfileRegistryWrite {
        profile_id: profile.profile_id.clone(),
        lifecycle_status: profile.lifecycle_status,
        display_name: profile.display_name.clone(),
        summary: profile.summary.clone(),
        default_session_kind: profile.default_session_kind.clone(),
        agent_id: profile.agent_id.clone(),
        owner_id: profile.owner_id.clone(),
        prompt_soul_markdown: profile.prompt_soul_markdown.clone(),
        prompt_memory_markdown: profile.prompt_memory_markdown.clone(),
        active_runtime_settings_json: profile.active_runtime_settings_json.clone(),
        source_asset_refs: profile.source_asset_refs.clone(),
        derived_runtime_refs: profile.derived_runtime_refs.clone(),
        import_export: profile.import_export.clone(),
        now,
    }
}

fn persisted_template_session_id(
    sessions: &[SessionState],
    profile_id: &ProfileId,
    agent_id: &AgentId,
    excluded: Option<&SessionId>,
) -> Option<SessionId> {
    sessions
        .iter()
        .filter(|session| {
            session.profile_id == *profile_id
                && session.agent_id == *agent_id
                && session.kind == SessionKind::Full
                && excluded != Some(&session.session_id)
        })
        .max_by(|left, right| {
            left.last_active_at
                .cmp(&right.last_active_at)
                .then_with(|| left.session_id.0.cmp(&right.session_id.0))
        })
        .map(|session| session.session_id.clone())
}

fn creation_fingerprint_for_session(
    profile: &ProfileRegistryRecord,
    session_id: &SessionId,
) -> Option<String> {
    profile
        .derived_runtime_refs
        .iter()
        .find(|reference| reference.ref_kind == "session" && reference.ref_id == session_id.0)
        .and_then(|reference| {
            reference
                .metadata_json
                .get(CREW_SESSION_CREATION_FINGERPRINT_KEY)
                .and_then(serde_json::Value::as_str)
        })
        .map(str::to_owned)
}

fn creation_fingerprint(request: &CrewAgentSessionCreationRequest) -> CoreResult<String> {
    let bytes = serde_json::to_vec(&json!({
        "profileId": request.profile_id,
        "expectedProfileRevision": request.expected_profile_revision,
        "workspaceCwd": normalize_session_workspace_cwd(&request.workspace_cwd)?,
    }))
    .map_err(|error| {
        CoreError::new(
            CoreErrorKind::InternalError,
            format!("fingerprint Crew session creation request: {error}"),
        )
    })?;
    Ok(sha256_hex(&bytes))
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn creation_error(kind: CoreErrorKind, reason: &str, message: impl Into<String>) -> CoreError {
    CoreError::new(
        kind,
        format!("crew_agent_session_creation_{reason}: {}", message.into()),
    )
}
