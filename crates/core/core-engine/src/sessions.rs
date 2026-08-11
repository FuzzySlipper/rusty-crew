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
        let _lifecycle_guard = self.agent_route_lifecycle_lock.lock().map_err(|_| {
            CoreError::new(
                CoreErrorKind::InternalError,
                "agent route lifecycle lock poisoned",
            )
        })?;
        self.validate_agent_id_route_reservation(&config.agent_id)?;
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
                let _lifecycle_guard = self.agent_route_lifecycle_lock.lock().map_err(|_| {
                    CoreError::new(
                        CoreErrorKind::InternalError,
                        "agent route lifecycle lock poisoned",
                    )
                })?;
                self.validate_agent_id_route_reservation(&config.agent_id)?;
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
                    self.reconcile_install_diplomat_bindings_for_session(&state, &self.now())?;
                    self.publish_session_execution(&config.session_id)?;
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
        self.project_session_execution(self.sessions.get_session(session_id)?)
    }

    pub fn list_sessions(&self) -> CoreResult<Vec<SessionState>> {
        self.sessions
            .all_sessions()?
            .into_iter()
            .map(|session| self.project_session_execution(session))
            .collect()
    }

    pub fn set_session_reasoning_effort(
        &self,
        session_id: &SessionId,
        reasoning_effort: Option<String>,
    ) -> CoreResult<SessionState> {
        let state = self.sessions.set_reasoning_effort_override(
            session_id,
            reasoning_effort,
            self.now(),
        )?;
        save_engine_session(&self.store, &state)?;
        Ok(state)
    }

    pub fn update_session_workspace(
        &self,
        update: &SessionWorkspaceUpdate,
    ) -> CoreResult<SessionWorkspaceUpdateRecord> {
        if update.requested_at.trim().is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "session_workspace_requested_at_required: requestedAt is required",
            ));
        }
        let original = self.sessions.get_session(&update.session_id)?;
        if original.status != SessionStatus::Archived
            && self.project_session_execution(original.clone())?.status != SessionStatus::Idle
        {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "session_workspace_busy: finish or cancel the active turn before switching workspace",
            ));
        }
        let mut config = load_engine_session_configs(&self.store)?
            .into_iter()
            .find(|config| config.session_id == update.session_id)
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    "session_workspace_config_missing: persisted session configuration was not found",
                )
            })?;
        let (previous, state) = self.sessions.update_workspace(update)?;
        let current = state.workspace.clone().ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::InternalError,
                "updated session workspace was not retained",
            )
        })?;
        if current == previous {
            return Ok(SessionWorkspaceUpdateRecord {
                previous,
                current,
                session: state,
            });
        }
        config.workspace = Some(current.clone());
        if let Err(error) = save_engine_session_with_config(&self.store, &state, &config) {
            self.sessions.restore_state(original)?;
            return Err(error);
        }
        self.bus.publish(CoreEvent::SessionWorkspaceChanged {
            session_id: update.session_id.clone(),
            previous: previous.clone(),
            current: current.clone(),
        })?;
        Ok(SessionWorkspaceUpdateRecord {
            previous,
            current,
            session: state,
        })
    }

    pub fn archive_session(&self, session_id: &SessionId) -> CoreResult<SessionState> {
        let now = self.now();
        self.archive_active_external_bindings_for_session(session_id, &now)?;
        self.degrade_install_diplomat_bindings_for_session(session_id, &now)?;
        let state = self.sessions.archive_session(session_id, now)?;
        save_engine_session(&self.store, &state)?;
        self.bus.publish(CoreEvent::SessionArchived {
            session_id: session_id.clone(),
        })?;
        self.publish_session_execution(session_id)?;
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
