//! Session lifecycle records for full agents and workers.

use rusty_crew_core_protocol::{
    AgentId, CoreError, CoreErrorKind, CoreResult, IsoTimestamp, ProfileId, SessionConfig,
    SessionHandle, SessionId, SessionKind, SessionState, SessionStatus, SessionWorkspace,
    SessionWorkspaceUpdate, MAX_RESOURCE_DELEGATION_DEPTH, MAX_RESOURCE_DURATION_MS,
};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

const SUPPORTED_REASONING_EFFORTS: [&str; 6] =
    ["none", "minimal", "low", "medium", "high", "xhigh"];

#[derive(Debug, Clone)]
pub struct SessionRegistry {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    next_handle: AtomicU64,
    sessions: Mutex<HashMap<SessionId, SessionState>>,
}

impl SessionRegistry {
    pub fn new() -> Self {
        Self::from_states(Vec::new())
    }

    pub fn from_states(states: Vec<SessionState>) -> Self {
        let next_handle = states
            .iter()
            .map(|state| state.handle.get())
            .max()
            .unwrap_or(0)
            + 1;
        let sessions = states
            .into_iter()
            .map(|state| (state.session_id.clone(), state))
            .collect();

        Self {
            inner: Arc::new(Inner {
                next_handle: AtomicU64::new(next_handle),
                sessions: Mutex::new(sessions),
            }),
        }
    }

    pub fn create_session(
        &self,
        mut config: SessionConfig,
        now: IsoTimestamp,
    ) -> CoreResult<SessionState> {
        normalize_session_config_workspace(&mut config)?;
        validate_session_resource_limits(&config)?;
        let mut sessions =
            self.inner.sessions.lock().map_err(|_| {
                CoreError::new(CoreErrorKind::InternalError, "session lock poisoned")
            })?;

        if sessions.contains_key(&config.session_id) {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!("session {} already exists", config.session_id),
            ));
        }

        let state = SessionState {
            handle: SessionHandle::new(self.inner.next_handle.fetch_add(1, Ordering::Relaxed)),
            session_id: config.session_id.clone(),
            agent_id: config.agent_id,
            profile_id: config.profile_id,
            kind: config.kind,
            delegation: config.delegation,
            workspace: config.workspace,
            resource_limits: config.resource_limits,
            tool_profile: config.tool_profile,
            history_window: config.history_window,
            inference_overrides: Default::default(),
            status: SessionStatus::Idle,
            brain_turn_count: 0,
            created_at: now.clone(),
            last_active_at: now,
        };
        sessions.insert(config.session_id, state.clone());
        Ok(state)
    }

    pub fn get_session(&self, session_id: &SessionId) -> CoreResult<SessionState> {
        self.inner
            .sessions
            .lock()
            .map_err(|_| CoreError::new(CoreErrorKind::InternalError, "session lock poisoned"))?
            .get(session_id)
            .cloned()
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    format!("session {session_id} not found"),
                )
            })
    }

    pub fn apply_config(&self, config: &SessionConfig) -> CoreResult<SessionState> {
        let mut config = config.clone();
        normalize_session_config_workspace(&mut config)?;
        validate_session_resource_limits(&config)?;
        let mut sessions =
            self.inner.sessions.lock().map_err(|_| {
                CoreError::new(CoreErrorKind::InternalError, "session lock poisoned")
            })?;
        let state = sessions.get_mut(&config.session_id).ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("session {} not found", config.session_id),
            )
        })?;
        state.resource_limits = config.resource_limits.clone();
        if state.workspace.is_none() && config.workspace.is_some() {
            state.workspace = config.workspace.clone();
        }
        state.tool_profile = config.tool_profile.clone();
        state.history_window = config.history_window.clone();
        Ok(state.clone())
    }

    pub fn set_reasoning_effort_override(
        &self,
        session_id: &SessionId,
        reasoning_effort: Option<String>,
        now: IsoTimestamp,
    ) -> CoreResult<SessionState> {
        if let Some(value) = reasoning_effort.as_deref() {
            if !SUPPORTED_REASONING_EFFORTS.contains(&value) {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "unsupported reasoning effort; expected one of none, minimal, low, medium, high, xhigh",
                ));
            }
        }
        let mut sessions =
            self.inner.sessions.lock().map_err(|_| {
                CoreError::new(CoreErrorKind::InternalError, "session lock poisoned")
            })?;
        let state = sessions.get_mut(session_id).ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("session {session_id} not found"),
            )
        })?;
        state.inference_overrides.reasoning_effort = reasoning_effort;
        state.last_active_at = now;
        Ok(state.clone())
    }

    pub fn update_workspace(
        &self,
        update: &SessionWorkspaceUpdate,
    ) -> CoreResult<(SessionWorkspace, SessionState)> {
        let mut sessions =
            self.inner.sessions.lock().map_err(|_| {
                CoreError::new(CoreErrorKind::InternalError, "session lock poisoned")
            })?;
        let state = sessions.get_mut(&update.session_id).ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("session {} not found", update.session_id),
            )
        })?;
        if state.status == SessionStatus::Archived {
            return Err(CoreError::new(
                CoreErrorKind::SessionExpired,
                "session_workspace_archived: archived sessions cannot switch workspace",
            ));
        }
        if state.status != SessionStatus::Idle {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "session_workspace_busy: finish or cancel the active turn before switching workspace",
            ));
        }
        let previous = state.workspace.clone().ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::InvalidInput,
                "session_workspace_missing: session has no canonical workspace",
            )
        })?;
        if previous.revision != update.expected_revision {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "session_workspace_revision_conflict: expected {}, found {}",
                    update.expected_revision, previous.revision
                ),
            ));
        }
        let cwd = normalize_session_workspace_cwd(&update.cwd)?;
        if cwd == previous.cwd {
            return Ok((previous, state.clone()));
        }
        state.workspace = Some(SessionWorkspace {
            cwd,
            revision: previous.revision + 1,
            updated_at: update.requested_at.clone(),
        });
        state.last_active_at = update.requested_at.clone();
        Ok((previous, state.clone()))
    }

    pub fn restore_state(&self, state: SessionState) -> CoreResult<()> {
        self.inner
            .sessions
            .lock()
            .map_err(|_| CoreError::new(CoreErrorKind::InternalError, "session lock poisoned"))?
            .insert(state.session_id.clone(), state);
        Ok(())
    }

    pub fn get_session_by_agent(&self, agent_id: &AgentId) -> CoreResult<SessionState> {
        let mut candidates = self
            .inner
            .sessions
            .lock()
            .map_err(|_| CoreError::new(CoreErrorKind::InternalError, "session lock poisoned"))?
            .values()
            .filter(|state| &state.agent_id == agent_id && state.status != SessionStatus::Archived)
            .cloned()
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| left.session_id.0.cmp(&right.session_id.0));
        match candidates.as_slice() {
            [] => Err(CoreError::new(
                CoreErrorKind::NotFound,
                format!("active session for agent {agent_id} not found"),
            )),
            [session] => Ok(session.clone()),
            _ => Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "agent_session_ambiguous: agent {agent_id} has multiple active sessions; specify session_id; candidate_session_ids=[{}]",
                    candidates
                        .iter()
                        .map(|session| session.session_id.0.as_str())
                        .collect::<Vec<_>>()
                        .join(",")
                ),
            )),
        }
    }

    pub fn all_sessions(&self) -> CoreResult<Vec<SessionState>> {
        let mut sessions = self
            .inner
            .sessions
            .lock()
            .map_err(|_| CoreError::new(CoreErrorKind::InternalError, "session lock poisoned"))?
            .values()
            .cloned()
            .collect::<Vec<_>>();
        sessions.sort_by_key(|state| state.handle.get());
        Ok(sessions)
    }

    pub fn remove_sessions_for_profile(
        &self,
        profile_id: &ProfileId,
    ) -> CoreResult<Vec<SessionState>> {
        let mut sessions =
            self.inner.sessions.lock().map_err(|_| {
                CoreError::new(CoreErrorKind::InternalError, "session lock poisoned")
            })?;
        let mut removed = Vec::new();
        let session_ids = sessions
            .iter()
            .filter(|(_, state)| &state.profile_id == profile_id)
            .map(|(session_id, _)| session_id.clone())
            .collect::<Vec<_>>();
        for session_id in session_ids {
            if let Some(state) = sessions.remove(&session_id) {
                removed.push(state);
            }
        }
        removed.sort_by_key(|state| state.handle.get());
        Ok(removed)
    }

    pub fn delegated_sessions_for_parent(
        &self,
        parent_session_id: &SessionId,
    ) -> CoreResult<Vec<SessionState>> {
        let mut children = self
            .inner
            .sessions
            .lock()
            .map_err(|_| CoreError::new(CoreErrorKind::InternalError, "session lock poisoned"))?
            .values()
            .filter(|state| {
                state
                    .delegation
                    .as_ref()
                    .is_some_and(|lineage| &lineage.parent_session_id == parent_session_id)
            })
            .cloned()
            .collect::<Vec<_>>();
        children.sort_by_key(|state| {
            state
                .delegation
                .as_ref()
                .map(|lineage| lineage.source_action_index)
                .unwrap_or(u32::MAX)
        });
        Ok(children)
    }

    pub fn delegated_session_for_source(
        &self,
        parent_session_id: &SessionId,
        source_wake_id: &str,
        source_action_index: u32,
    ) -> CoreResult<Option<SessionState>> {
        Ok(self
            .delegated_sessions_for_parent(parent_session_id)?
            .into_iter()
            .find(|state| {
                state.delegation.as_ref().is_some_and(|lineage| {
                    lineage.source_wake_id == source_wake_id
                        && lineage.source_action_index == source_action_index
                })
            }))
    }

    pub fn archive_session(
        &self,
        session_id: &SessionId,
        now: IsoTimestamp,
    ) -> CoreResult<SessionState> {
        let mut sessions =
            self.inner.sessions.lock().map_err(|_| {
                CoreError::new(CoreErrorKind::InternalError, "session lock poisoned")
            })?;
        let state = sessions.get_mut(session_id).ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("session {session_id} not found"),
            )
        })?;
        state.status = SessionStatus::Archived;
        state.last_active_at = now;
        Ok(state.clone())
    }

    pub fn reactivate_session(
        &self,
        session_id: &SessionId,
        now: IsoTimestamp,
    ) -> CoreResult<SessionState> {
        let mut sessions =
            self.inner.sessions.lock().map_err(|_| {
                CoreError::new(CoreErrorKind::InternalError, "session lock poisoned")
            })?;
        let state = sessions.get_mut(session_id).ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("session {session_id} not found"),
            )
        })?;
        if state.status == SessionStatus::Archived {
            state.status = SessionStatus::Idle;
            state.last_active_at = now;
        }
        Ok(state.clone())
    }

    pub fn mark_idle(&self, session_id: &SessionId, now: IsoTimestamp) -> CoreResult<SessionState> {
        let mut sessions =
            self.inner.sessions.lock().map_err(|_| {
                CoreError::new(CoreErrorKind::InternalError, "session lock poisoned")
            })?;
        let state = sessions.get_mut(session_id).ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("session {session_id} not found"),
            )
        })?;
        if state.status == SessionStatus::Archived {
            return Err(CoreError::new(
                CoreErrorKind::SessionExpired,
                format!("session {session_id} is archived"),
            ));
        }
        state.status = SessionStatus::Idle;
        state.last_active_at = now;
        Ok(state.clone())
    }
}

pub fn normalize_session_workspace_cwd(cwd: &str) -> CoreResult<String> {
    let trimmed = cwd.trim();
    if trimmed.is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "session workspace cwd must not be blank",
        ));
    }
    let path = Path::new(trimmed);
    if !path.is_absolute() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "session workspace cwd must be an absolute path",
        ));
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(Path::new("/")),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    normalized.into_os_string().into_string().map_err(|_| {
        CoreError::new(
            CoreErrorKind::InvalidInput,
            "session workspace cwd must be valid UTF-8",
        )
    })
}

fn normalize_session_config_workspace(config: &mut SessionConfig) -> CoreResult<()> {
    if let Some(workspace) = config.workspace.as_mut() {
        workspace.cwd = normalize_session_workspace_cwd(&workspace.cwd)?;
        if workspace.revision == 0 {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "session workspace revision must be greater than zero",
            ));
        }
        if workspace.updated_at.trim().is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "session workspace updatedAt must not be blank",
            ));
        }
    }
    Ok(())
}

fn validate_session_resource_limits(config: &SessionConfig) -> CoreResult<()> {
    if let Some(constraint) = config
        .delegation
        .as_ref()
        .and_then(|lineage| lineage.workspace_constraint.as_ref())
    {
        if config.kind != SessionKind::Delegated {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "delegated workspace constraints are valid only on delegated sessions",
            ));
        }
        if constraint.cwd.trim().is_empty() || !Path::new(&constraint.cwd).is_absolute() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "delegated workspace constraint cwd must be a non-empty absolute path",
            ));
        }
    }
    if config
        .resource_limits
        .max_duration_ms
        .is_some_and(|value| value > MAX_RESOURCE_DURATION_MS)
    {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("session resourceLimits.maxDurationMs exceeds {MAX_RESOURCE_DURATION_MS}"),
        ));
    }
    if config
        .resource_limits
        .max_delegation_depth
        .is_some_and(|value| value > MAX_RESOURCE_DELEGATION_DEPTH)
    {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!(
                "session resourceLimits.maxDelegationDepth exceeds {MAX_RESOURCE_DELEGATION_DEPTH}"
            ),
        ));
    }
    Ok(())
}

impl Default for SessionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_crew_core_protocol::{ResourceLimits, SessionKind, ToolProfile};

    fn config(workdir: Option<&str>) -> SessionConfig {
        SessionConfig {
            session_id: SessionId::new("resource-limits-session"),
            agent_id: AgentId::new("resource-limits-agent"),
            profile_id: ProfileId::new("resource-limits-profile"),
            kind: SessionKind::Full,
            delegation: None,
            workspace: workdir.map(|cwd| SessionWorkspace {
                cwd: cwd.to_string(),
                revision: 1,
                updated_at: "2026-01-01T00:00:00Z".to_string(),
            }),
            resource_limits: ResourceLimits {
                max_duration_ms: Some(60_000),
                max_delegation_depth: Some(2),
            },
            tool_profile: ToolProfile { tools: Vec::new() },
            history_window: None,
        }
    }

    #[test]
    fn creates_session_with_explicit_absolute_workspace() {
        let registry = SessionRegistry::new();
        let state = registry
            .create_session(
                config(Some("/home/dev/goblinbench-fixture")),
                "2026-07-15T00:00:00Z".to_string(),
            )
            .expect("absolute workdir should be accepted");

        assert_eq!(
            state
                .workspace
                .as_ref()
                .map(|workspace| workspace.cwd.as_str()),
            Some("/home/dev/goblinbench-fixture")
        );
        assert!(state.delegation.is_none());
    }

    #[test]
    fn preserves_omitted_workspace_and_rejects_blank_or_relative_values() {
        let registry = SessionRegistry::new();
        let state = registry
            .create_session(config(None), "2026-07-15T00:00:00Z".to_string())
            .expect("omitted workdir should preserve default resolution");
        assert_eq!(state.workspace, None);

        for invalid in ["", "   ", "relative/workdir"] {
            let error = SessionRegistry::new()
                .create_session(config(Some(invalid)), "2026-07-15T00:00:00Z".to_string())
                .expect_err("invalid workdir should be rejected");
            assert_eq!(error.kind, CoreErrorKind::InvalidInput);
        }
    }

    #[test]
    fn switches_idle_workspace_with_revision_and_preserves_session_identity() {
        let registry = SessionRegistry::new();
        let original = registry
            .create_session(
                config(Some("/home/dev/one/./repo")),
                "2026-07-15T00:00:00Z".to_string(),
            )
            .unwrap();
        let update = SessionWorkspaceUpdate {
            session_id: original.session_id.clone(),
            cwd: "/home/dev/two/../three".to_string(),
            expected_revision: 1,
            requested_at: "2026-07-15T00:01:00Z".to_string(),
        };
        let (previous, current) = registry.update_workspace(&update).unwrap();
        assert_eq!(previous.cwd, "/home/dev/one/repo");
        assert_eq!(current.session_id, original.session_id);
        assert_eq!(current.agent_id, original.agent_id);
        assert_eq!(current.profile_id, original.profile_id);
        assert_eq!(current.workspace.as_ref().unwrap().cwd, "/home/dev/three");
        assert_eq!(current.workspace.as_ref().unwrap().revision, 2);

        let stale = registry.update_workspace(&update).unwrap_err();
        assert_eq!(stale.kind, CoreErrorKind::ActionRejected);
        assert!(stale
            .message
            .contains("session_workspace_revision_conflict"));

        let mut busy = current;
        busy.status = SessionStatus::Active;
        registry.restore_state(busy).unwrap();
        let busy_error = registry
            .update_workspace(&SessionWorkspaceUpdate {
                session_id: original.session_id,
                cwd: "/home/dev/four".to_string(),
                expected_revision: 2,
                requested_at: "2026-07-15T00:02:00Z".to_string(),
            })
            .unwrap_err();
        assert_eq!(busy_error.kind, CoreErrorKind::ActionRejected);
        assert!(busy_error.message.contains("session_workspace_busy"));
    }

    #[test]
    fn rejects_resource_limits_above_shared_bounds() {
        let mut invalid_duration = config(Some("/home"));
        invalid_duration.resource_limits.max_duration_ms = Some(MAX_RESOURCE_DURATION_MS + 1);
        let duration_error = SessionRegistry::new()
            .create_session(invalid_duration, "2026-07-15T00:00:00Z".to_string())
            .expect_err("duration above bound should be rejected");
        assert_eq!(duration_error.kind, CoreErrorKind::InvalidInput);

        let mut invalid_depth = config(Some("/home"));
        invalid_depth.resource_limits.max_delegation_depth =
            Some(MAX_RESOURCE_DELEGATION_DEPTH + 1);
        let depth_error = SessionRegistry::new()
            .create_session(invalid_depth, "2026-07-15T00:00:00Z".to_string())
            .expect_err("delegation depth above bound should be rejected");
        assert_eq!(depth_error.kind, CoreErrorKind::InvalidInput);
    }

    #[test]
    fn reasoning_effort_override_is_session_scoped_preserved_by_config_and_clearable() {
        let registry = SessionRegistry::new();
        let config = config(Some("/home"));
        let session_id = config.session_id.clone();
        registry
            .create_session(config.clone(), "2026-07-16T00:00:00Z".to_string())
            .unwrap();

        let updated = registry
            .set_reasoning_effort_override(
                &session_id,
                Some("high".to_string()),
                "2026-07-16T00:01:00Z".to_string(),
            )
            .unwrap();
        assert_eq!(
            updated.inference_overrides.reasoning_effort.as_deref(),
            Some("high")
        );
        assert_eq!(
            registry
                .apply_config(&config)
                .unwrap()
                .inference_overrides
                .reasoning_effort
                .as_deref(),
            Some("high")
        );

        let cleared = registry
            .set_reasoning_effort_override(&session_id, None, "2026-07-16T00:02:00Z".to_string())
            .unwrap();
        assert_eq!(cleared.inference_overrides.reasoning_effort, None);

        let error = registry
            .set_reasoning_effort_override(
                &session_id,
                Some("banana".to_string()),
                "2026-07-16T00:03:00Z".to_string(),
            )
            .unwrap_err();
        assert_eq!(error.kind, CoreErrorKind::InvalidInput);
        assert_eq!(
            registry
                .get_session(&session_id)
                .unwrap()
                .inference_overrides
                .reasoning_effort,
            None
        );
    }
}
