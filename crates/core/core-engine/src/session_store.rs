use rusty_crew_core_bus::SequencedEvent;
use rusty_crew_core_persistence::{CoreCoordinationStore, PersistedEvent};
use rusty_crew_core_protocol::{CoreEvent, CoreResult, SessionConfig, SessionState};

pub(crate) trait EngineBootstrapStore {
    fn load_engine_sessions(&self) -> CoreResult<Vec<SessionState>>;
    fn load_engine_event_history(&self) -> CoreResult<Vec<PersistedEvent>>;
    fn save_engine_event(&self, sequence: u64, event: &CoreEvent) -> CoreResult<()>;
}

pub(crate) trait SessionLifecycleStore {
    fn load_engine_session_configs(&self) -> CoreResult<Vec<SessionConfig>>;
    fn save_engine_session(&self, state: &SessionState) -> CoreResult<()>;
    fn save_engine_session_with_config(
        &self,
        state: &SessionState,
        config: &SessionConfig,
    ) -> CoreResult<()>;
}

impl EngineBootstrapStore for CoreCoordinationStore {
    fn load_engine_sessions(&self) -> CoreResult<Vec<SessionState>> {
        self.load_sessions()
    }

    fn load_engine_event_history(&self) -> CoreResult<Vec<PersistedEvent>> {
        self.load_event_history()
    }

    fn save_engine_event(&self, sequence: u64, event: &CoreEvent) -> CoreResult<()> {
        self.save_event(sequence, event)
    }
}

impl SessionLifecycleStore for CoreCoordinationStore {
    fn load_engine_session_configs(&self) -> CoreResult<Vec<SessionConfig>> {
        Ok(self
            .load_session_configs()?
            .into_iter()
            .map(|record| record.config)
            .collect())
    }

    fn save_engine_session(&self, state: &SessionState) -> CoreResult<()> {
        self.save_session(state)
    }

    fn save_engine_session_with_config(
        &self,
        state: &SessionState,
        config: &SessionConfig,
    ) -> CoreResult<()> {
        self.save_session_with_config(state, config)
    }
}

pub(crate) fn load_engine_session_configs(
    store: &impl SessionLifecycleStore,
) -> CoreResult<Vec<SessionConfig>> {
    store.load_engine_session_configs()
}

pub(crate) fn load_engine_bootstrap(
    store: &impl EngineBootstrapStore,
) -> CoreResult<(Vec<SessionState>, Vec<SequencedEvent>)> {
    let sessions = store.load_engine_sessions()?;
    let events = store
        .load_engine_event_history()?
        .into_iter()
        .map(|entry| SequencedEvent {
            sequence: entry.sequence,
            event: entry.event,
        })
        .collect();
    Ok((sessions, events))
}

pub(crate) fn save_engine_event(
    store: &impl EngineBootstrapStore,
    sequence: u64,
    event: &CoreEvent,
) -> CoreResult<()> {
    store.save_engine_event(sequence, event)
}

pub(crate) fn save_engine_session(
    store: &impl SessionLifecycleStore,
    state: &SessionState,
) -> CoreResult<()> {
    store.save_engine_session(state)
}

pub(crate) fn save_engine_session_with_config(
    store: &impl SessionLifecycleStore,
    state: &SessionState,
    config: &SessionConfig,
) -> CoreResult<()> {
    store.save_engine_session_with_config(state, config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_crew_core_protocol::{
        AgentId, ProfileId, ResourceLimits, SessionId, SessionKind, SessionWorkspace, ToolProfile,
    };
    use rusty_crew_core_session::SessionRegistry;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeSessionStore {
        sessions: Mutex<Vec<SessionState>>,
        events: Mutex<Vec<PersistedEvent>>,
        saved_configs: Mutex<Vec<SessionConfig>>,
    }

    impl EngineBootstrapStore for FakeSessionStore {
        fn load_engine_sessions(&self) -> CoreResult<Vec<SessionState>> {
            Ok(self.sessions.lock().unwrap().clone())
        }

        fn load_engine_event_history(&self) -> CoreResult<Vec<PersistedEvent>> {
            Ok(self.events.lock().unwrap().clone())
        }

        fn save_engine_event(&self, sequence: u64, event: &CoreEvent) -> CoreResult<()> {
            self.events.lock().unwrap().push(PersistedEvent {
                sequence,
                event: event.clone(),
            });
            Ok(())
        }
    }

    impl SessionLifecycleStore for FakeSessionStore {
        fn load_engine_session_configs(&self) -> CoreResult<Vec<SessionConfig>> {
            Ok(self.saved_configs.lock().unwrap().clone())
        }

        fn save_engine_session(&self, state: &SessionState) -> CoreResult<()> {
            let mut sessions = self.sessions.lock().unwrap();
            if let Some(existing) = sessions
                .iter_mut()
                .find(|existing| existing.session_id == state.session_id)
            {
                *existing = state.clone();
            } else {
                sessions.push(state.clone());
            }
            Ok(())
        }

        fn save_engine_session_with_config(
            &self,
            state: &SessionState,
            config: &SessionConfig,
        ) -> CoreResult<()> {
            self.save_engine_session(state)?;
            self.saved_configs.lock().unwrap().push(config.clone());
            Ok(())
        }
    }

    #[test]
    fn bootstrap_uses_fake_store_without_concrete_database() {
        let store = FakeSessionStore::default();
        let config = session_config("prime-session", "prime", "prime-profile");
        let state = SessionRegistry::new()
            .create_session(config.clone(), "2026-07-09T08:00:00Z".to_string())
            .unwrap();

        save_engine_session_with_config(&store, &state, &config).unwrap();
        save_engine_event(
            &store,
            7,
            &CoreEvent::SessionCreated {
                state: Box::new(state.clone()),
            },
        )
        .unwrap();

        let (sessions, events) = load_engine_bootstrap(&store).unwrap();

        assert_eq!(sessions, vec![state]);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].sequence, 7);
        assert!(matches!(events[0].event, CoreEvent::SessionCreated { .. }));
        assert_eq!(
            store.saved_configs.lock().unwrap()[0]
                .session_id
                .to_string(),
            "prime-session"
        );
    }

    fn session_config(session_id: &str, agent_id: &str, profile_id: &str) -> SessionConfig {
        SessionConfig {
            session_id: SessionId::new(session_id),
            agent_id: AgentId::new(agent_id),
            profile_id: ProfileId::new(profile_id),
            kind: SessionKind::Full,
            delegation: None,
            workspace: Some(SessionWorkspace {
                cwd: "/home/dev/rusty-crew".to_string(),
                revision: 1,
                updated_at: "2026-01-01T00:00:00Z".to_string(),
            }),
            resource_limits: ResourceLimits {
                max_duration_ms: Some(60_000),
                max_delegation_depth: Some(1),
            },
            tool_profile: ToolProfile { tools: Vec::new() },
            history_window: None,
        }
    }
}
