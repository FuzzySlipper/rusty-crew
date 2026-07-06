use super::*;

impl NativeBridge {
    pub fn manifest_version(&self) -> u32 {
        MANIFEST_VERSION
    }

    pub fn operation_names(&self) -> &'static [&'static str] {
        OPERATION_NAMES
    }

    pub fn wire_shape_fingerprint(&self) -> &'static str {
        wire_shape_fingerprint()
    }

    pub fn manifest_summary(&self) -> BridgeManifestSummary {
        manifest_summary()
    }

    pub fn initialize_engine(&mut self, config: EngineConfig) -> CoreResult<EngineHandle> {
        if self.engine.is_some() {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                "native bridge engine is already initialized",
            ));
        }

        let engine = CoreEngine::initialize(config)?;
        for registration in self.brain_registrations.registrations() {
            engine.register_profile_tool_profile(
                registration.profile_id.clone(),
                registration.tool_profile.clone(),
            )?;
        }
        let handle = engine.handle();
        self.engine = Some(engine);
        Ok(handle)
    }

    pub fn shutdown_engine(&mut self, request: ShutdownRequest) -> CoreResult<ShutdownSummary> {
        let engine = self.engine.take().ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                "native bridge engine is not initialized",
            )
        })?;
        let summary = engine.shutdown_with_timeout(request.drain_timeout_ms)?;
        self.subscriptions.clear();
        Ok(summary)
    }

    pub fn build_brain_wake_request(
        &self,
        input: BrainWakeBufferInput,
    ) -> CoreResult<rusty_crew_core_bridge_api::BufferedBrainWakeRequest> {
        let mut buffered = self.buffers.build_brain_wake_request(input)?;
        self.hydrate_provider_state(&mut buffered.request)?;
        Ok(buffered)
    }

    pub fn build_brain_wake_request_for_session(
        &self,
        brain: BrainImplementationHandle,
        session_id: rusty_crew_core_bridge_api::SessionId,
        system_prompt: String,
        role_assembly_json: Vec<u8>,
        wake_id: String,
    ) -> CoreResult<rusty_crew_core_bridge_api::BufferedBrainWakeRequest> {
        let body_state = self.engine()?.prepare_body_state_for_wake(&session_id)?;
        let body_state_json = serde_json::to_vec(&body_state).map_err(|error| {
            CoreError::new(
                CoreErrorKind::InternalError,
                format!("serialize body state: {error}"),
            )
        })?;
        self.build_brain_wake_request(BrainWakeBufferInput {
            brain,
            session_id,
            body_state_json,
            system_prompt,
            role_assembly_json,
            wake_id,
        })
    }

    fn hydrate_provider_state(&self, request: &mut BrainWakeRequest) -> CoreResult<()> {
        let Ok(registration) = self.brain_registrations.get(request.brain) else {
            return Ok(());
        };
        let Some(engine) = &self.engine else {
            return Ok(());
        };
        let hydration = engine.provider_state_for_wake(registration, &request.session_id)?;
        request.provider_state = hydration.state;
        request.provider_state_absence = hydration.absence_reason;
        Ok(())
    }

    pub fn apply_provider_state_output(
        &self,
        brain: BrainImplementationHandle,
        session_id: &SessionId,
        wake_id: &str,
        output: BrainWakeProviderStateOutput,
    ) -> CoreResult<()> {
        let registration = self.brain_registrations.get(brain)?;
        self.engine()?
            .apply_provider_state_output(registration, session_id, wake_id, output)
    }

    pub fn provider_state_diagnostics(
        &self,
        limit: u32,
    ) -> CoreResult<Vec<JsProviderStateDiagnostic>> {
        let now = self.engine()?.diagnostic_now();
        self.engine()?
            .provider_wire_state_diagnostics(limit)
            .map(|records| {
                records
                    .into_iter()
                    .map(|record| {
                        let status = provider_wire_state_status(
                            record.invalidated_at.as_ref(),
                            record.invalidation_reason.as_deref(),
                            record.expires_at.as_ref(),
                            &now,
                        );
                        JsProviderStateDiagnostic {
                            session_id: record.key.session_id.0,
                            module_id: record.key.module_id,
                            strategy_id: record.key.strategy_id,
                            status: status.to_string(),
                            payload_version: Some(record.payload_version),
                            payload_bytes: Some(record.payload_bytes as f64),
                            created_at: Some(record.created_at),
                            updated_at: Some(record.updated_at),
                            expires_at: record.expires_at,
                            last_wake_id: record.last_wake_id,
                            invalidated_at: record.invalidated_at,
                            invalidation_reason: record.invalidation_reason,
                        }
                    })
                    .collect()
            })
    }

    pub fn get_buffer(&self, handle: RuntimeBufferHandle) -> CoreResult<RuntimeBufferView> {
        self.buffers.get_buffer(handle)
    }

    pub fn release_buffer(&self, handle: RuntimeBufferHandle) -> CoreResult<Unit> {
        self.buffers.release_buffer(handle)?;
        Ok(Unit)
    }

    pub fn assert_no_buffer_leaks(&self) -> CoreResult<()> {
        self.buffers.assert_no_leaks()
    }

    pub(crate) fn engine(&self) -> CoreResult<&CoreEngine> {
        self.engine.as_ref().ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                "native bridge engine is not initialized",
            )
        })
    }
}
