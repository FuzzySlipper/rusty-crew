//! Session lifecycle records for full agents and workers.

use rusty_crew_core_protocol::{
    CoreError, CoreErrorKind, CoreResult, IsoTimestamp, SessionConfig, SessionHandle, SessionId,
    SessionState, SessionStatus,
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
        Self {
            inner: Arc::new(Inner {
                next_handle: AtomicU64::new(1),
                sessions: Mutex::new(HashMap::new()),
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
