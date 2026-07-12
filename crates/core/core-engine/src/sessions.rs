use super::*;

impl CoreEngine {
    pub fn handle(&self) -> EngineHandle {
        self.handle
    }

    pub fn bus(&self) -> &CoreBus {
        &self.bus
    }

    pub fn subscribe_events(
        &self,
        filter: EventSubscription,
    ) -> CoreResult<(u64, Receiver<CoreEvent>)> {
        self.bus.subscribe(filter)
    }

    pub fn unsubscribe_events(&self, id: u64) -> CoreResult<()> {
        self.bus.unsubscribe(id)
    }

    pub fn create_session(&self, config: SessionConfig) -> CoreResult<SessionState> {
        let state = self.sessions.create_session(config.clone(), self.now())?;
        save_engine_session_with_config(&self.store, &state, &config)?;
        self.bus.publish(CoreEvent::SessionCreated {
            state: Box::new(state.clone()),
        })?;
        Ok(state)
    }

    pub fn ensure_configured_session(&self, config: SessionConfig) -> CoreResult<SessionState> {
        match self.sessions.get_session(&config.session_id) {
            Ok(existing) => {
                if existing.agent_id != config.agent_id
                    || existing.profile_id != config.profile_id
                    || existing.kind != config.kind
                    || existing.delegation != config.delegation
                {
                    return Err(CoreError::new(
                        CoreErrorKind::AlreadyExists,
                        format!(
                            "session {} already exists with a different configured identity",
                            config.session_id
                        ),
                    ));
                }
                if existing.status == SessionStatus::Archived {
                    let now = self.now();
                    self.expire_body_follow_up_messages(&now)?;
                    self.sessions.apply_config(&config)?;
                    let state = self.sessions.reactivate_session(&config.session_id, now)?;
                    save_engine_session(&self.store, &state)?;
                    return Ok(state);
                }
                let state = self.sessions.apply_config(&config)?;
                save_engine_session(&self.store, &state)?;
                Ok(state)
            }
            Err(error) if error.kind == CoreErrorKind::NotFound => self.create_session(config),
            Err(error) => Err(error),
        }
    }

    pub fn get_session(&self, session_id: &SessionId) -> CoreResult<SessionState> {
        self.sessions.get_session(session_id)
    }

    pub fn list_sessions(&self) -> CoreResult<Vec<SessionState>> {
        self.sessions.all_sessions()
    }

    pub fn archive_session(&self, session_id: &SessionId) -> CoreResult<SessionState> {
        let now = self.now();
        self.archive_active_external_bindings_for_session(session_id, &now)?;
        let state = self.sessions.archive_session(session_id, now)?;
        save_engine_session(&self.store, &state)?;
        self.bus.publish(CoreEvent::SessionArchived {
            session_id: session_id.clone(),
        })?;
        if state.kind == SessionKind::Delegated {
            if !load_delegated_worker_run_by_session(&self.store, session_id)?
                .as_ref()
                .is_some_and(|run| run.status.is_terminal())
            {
                update_delegated_worker_run_status_by_session(
                    &self.store,
                    session_id,
                    WorkerRunStatus::Cancelled,
                    self.now(),
                )?;
            }
        } else {
            self.cancel_delegated_children_for_parent(session_id)?;
        }
        Ok(state)
    }
}
