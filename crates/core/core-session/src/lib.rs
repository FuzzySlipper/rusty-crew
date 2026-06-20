//! Session lifecycle records for full agents and workers.

use rusty_crew_core_protocol::{
    AgentId, CoreError, CoreErrorKind, CoreResult, IsoTimestamp, SessionConfig, SessionHandle,
    SessionId, SessionState, SessionStatus,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

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
        config: SessionConfig,
        now: IsoTimestamp,
    ) -> CoreResult<SessionState> {
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
            resource_limits: config.resource_limits,
            tool_profile: config.tool_profile,
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

    pub fn get_session_by_agent(&self, agent_id: &AgentId) -> CoreResult<SessionState> {
        self.inner
            .sessions
            .lock()
            .map_err(|_| CoreError::new(CoreErrorKind::InternalError, "session lock poisoned"))?
            .values()
            .find(|state| &state.agent_id == agent_id && state.status != SessionStatus::Archived)
            .cloned()
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    format!("active session for agent {agent_id} not found"),
                )
            })
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
}

impl Default for SessionRegistry {
    fn default() -> Self {
        Self::new()
    }
}
