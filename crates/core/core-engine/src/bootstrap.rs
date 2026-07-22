use super::*;

impl CoreEngine {
    pub fn initialize(config: EngineConfig) -> CoreResult<Self> {
        let validation = validate_engine_config(&config);
        if let Some(diagnostic) = validation.diagnostics.into_iter().next() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!(
                    "{}{}: {}",
                    diagnostic.code,
                    diagnostic
                        .path
                        .as_deref()
                        .map(|path| format!(" at {path}"))
                        .unwrap_or_default(),
                    diagnostic.message
                ),
            ));
        }
        let store =
            CoreCoordinationStore::open_storage(&config.engine_data_dir, config.storage.as_ref())?;
        let (persisted_sessions, persisted_events) = load_engine_bootstrap(&store)?;
        let recorder_store = store.clone();
        let bus = CoreBus::with_history_and_recorder(
            persisted_events,
            Some(Arc::new(move |sequence, event| {
                save_engine_event(&recorder_store, sequence, event)
            })),
        );
        let sessions = SessionRegistry::from_states(persisted_sessions);

        let handle = EngineHandle::new(NEXT_ENGINE_HANDLE.fetch_add(1, Ordering::Relaxed));
        let service_instance_id = format!("service-{}-{}", std::process::id(), handle.get());
        let now = match &config.clock {
            ClockConfig::System => OffsetDateTime::now_utc()
                .format(&Rfc3339)
                .expect("formatting current UTC timestamp as RFC3339 should not fail"),
            ClockConfig::Fixed { at } => at.clone(),
        };
        store.interrupt_runtime_activities_from_other_instances(&service_instance_id, &now)?;

        let engine = Self {
            handle,
            service_instance_id,
            config,
            body_projector: BodyProjector::new(bus.clone(), sessions.clone()),
            action_executor: BrainActionExecutor::new(bus.clone(), sessions.clone()),
            bus,
            sessions,
            store,
            profile_tool_profiles: Arc::new(Mutex::new(HashMap::new())),
            scheduler_tick_lock: Arc::new(Mutex::new(())),
            github_gate_lock: Arc::new(Mutex::new(())),
            external_follow_up_lock: Arc::new(Mutex::new(())),
            agent_route_lifecycle_lock: Arc::new(Mutex::new(())),
        };
        engine.validate_agent_route_session_collisions()?;
        engine.cleanup_orphaned_delegated_sessions()?;
        engine.expire_delegated_sessions()?;
        engine.reactivate_active_roleplay_sessions()?;
        engine.reactivate_active_external_sessions()?;
        Ok(engine)
    }

    fn reactivate_active_roleplay_sessions(&self) -> CoreResult<()> {
        let configs = load_engine_session_configs(&self.store)?
            .into_iter()
            .map(|config| (config.session_id.clone(), config))
            .collect::<HashMap<_, _>>();
        let active_roleplay_sessions = RoleplayRecordsStore::list_session_metadata(
            &self.store,
            &RoleplaySessionMetadataQuery {
                profile_id: None,
                archived: Some(false),
                page: None,
            },
        )?;
        for metadata in active_roleplay_sessions {
            let session_id = SessionId::new(metadata.session_id.clone());
            let config = configs.get(&session_id).cloned().ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    format!(
                        "active roleplay session {} has no persisted session config",
                        metadata.session_id
                    ),
                )
            })?;
            self.ensure_configured_session(config)?;
        }
        Ok(())
    }

    pub fn shutdown(self) -> CoreResult<ShutdownSummary> {
        self.shutdown_with_timeout(0)
    }

    pub fn shutdown_with_timeout(self, drain_timeout_ms: u32) -> CoreResult<ShutdownSummary> {
        let active_external_session_ids = self.active_external_session_ids()?;
        let active_sessions = self
            .sessions
            .all_sessions()?
            .into_iter()
            .filter(|session| {
                session.status != SessionStatus::Archived
                    && !active_external_session_ids.contains(&session.session_id)
            })
            .collect::<Vec<_>>();
        let archived_sessions = active_sessions.len() as u32;
        for session in active_sessions {
            if self.sessions.get_session(&session.session_id)?.status != SessionStatus::Archived {
                self.archive_session(&session.session_id)?;
            }
        }
        // Shutdown is currently synchronous. This timeout becomes meaningful
        // when the engine owns background tasks that require bounded joins.
        let _ = drain_timeout_ms;
        let dropped_subscriptions = self.bus.shutdown_subscribers()?;
        Ok(ShutdownSummary {
            engine: self.handle,
            archived_sessions,
            dropped_subscriptions,
        })
    }

    pub fn diagnostic_now(&self) -> IsoTimestamp {
        self.now()
    }

    pub fn service_instance_id(&self) -> &str {
        &self.service_instance_id
    }

    pub(crate) fn now(&self) -> IsoTimestamp {
        match &self.config.clock {
            ClockConfig::System => OffsetDateTime::now_utc()
                .format(&Rfc3339)
                .expect("formatting current UTC timestamp as RFC3339 should not fail"),
            ClockConfig::Fixed { at } => at.clone(),
        }
    }
}
