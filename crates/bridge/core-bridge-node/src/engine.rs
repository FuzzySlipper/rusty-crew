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
        self.cleanup_buffered_brain_runs(
            "service_shutdown",
            "service shutdown cleaned up active buffered brain runs",
        )
        .map_err(brain_runtime_error_to_core)?;
        let engine = self.engine.take().ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                "native bridge engine is not initialized",
            )
        })?;
        let summary = engine.shutdown_with_timeout(request.drain_timeout_ms)?;
        self.subscriptions.clear();
        self.active_logical_wakes.clear();
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
        &mut self,
        brain: BrainImplementationHandle,
        session_id: rusty_crew_core_bridge_api::SessionId,
        system_prompt: String,
        role_assembly_json: Vec<u8>,
        wake_id: String,
    ) -> CoreResult<rusty_crew_core_bridge_api::BufferedBrainWakeRequest> {
        let registration = self.brain_registrations.get(brain)?.clone();
        let module_id = registration
            .strategy
            .as_ref()
            .map(|strategy| strategy.module_id.as_str());
        if module_id != Some("chat-completions") {
            let body_state = self.engine()?.prepare_body_state_for_wake(&session_id)?;
            let body_state_json = serde_json::to_vec(&body_state).map_err(|error| {
                CoreError::new(
                    CoreErrorKind::InternalError,
                    format!("serialize body state: {error}"),
                )
            })?;
            return self.build_brain_wake_request(BrainWakeBufferInput {
                brain,
                session_id,
                body_state_json,
                system_prompt,
                role_assembly_json,
                wake_id,
            });
        }
        if self.active_logical_wakes.contains_key(&wake_id) {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!("logical wake {wake_id} is already active"),
            ));
        }
        let prepared = self.engine()?.prepare_logical_turn_wake(
            &registration,
            &session_id,
            &wake_id,
            system_prompt,
            role_assembly_json,
        )?;
        let mut buffered = self.build_brain_wake_request(BrainWakeBufferInput {
            brain,
            session_id: session_id.clone(),
            body_state_json: prepared.body_state_json,
            system_prompt: prepared.system_prompt,
            role_assembly_json: prepared.role_assembly_json,
            wake_id: wake_id.clone(),
        })?;
        buffered.request.continuation_state = prepared.continuation_state;
        self.active_logical_wakes.insert(
            wake_id,
            ActiveLogicalWake {
                brain,
                session_id,
                claim: prepared.claim,
            },
        );
        Ok(buffered)
    }

    pub fn settle_brain_wake(
        &mut self,
        wake_id: &str,
        result: LogicalTurnEpochResult,
    ) -> CoreResult<Option<LogicalTurnEpochSettlement>> {
        let Some(active) = self.active_logical_wakes.get(wake_id).cloned() else {
            return Ok(None);
        };
        let registration = self.brain_registrations.get(active.brain)?;
        if registration.profile_id != active.claim.record.binding.profile_id
            || active.session_id != active.claim.record.session_id
        {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "logical wake binding changed before settlement",
            ));
        }
        let settlement = self
            .engine()?
            .settle_logical_turn_epoch(&active.claim, result)?;
        self.active_logical_wakes.remove(wake_id);
        Ok(Some(settlement))
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

pub(crate) fn parse_brain_provider_state_output_json(
    raw: &str,
) -> CoreResult<BrainWakeProviderStateOutput> {
    #[derive(serde::Deserialize)]
    #[serde(tag = "type", rename_all = "snake_case")]
    enum WireOutput {
        Unchanged,
        Replace { state: WireUpdate },
        Clear { reason: WireClearReason },
    }

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct WireUpdate {
        module_id: String,
        strategy_id: String,
        profile_fingerprint: String,
        provider_fingerprint: String,
        payload_version: String,
        payload: serde_json::Value,
        ttl_ms: Option<u64>,
    }

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "snake_case")]
    enum WireClearReason {
        BrainRequestedClear,
    }

    let parsed = serde_json::from_str::<WireOutput>(raw).map_err(|error| {
        CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("invalid provider state output json: {error}"),
        )
    })?;
    Ok(match parsed {
        WireOutput::Unchanged => BrainWakeProviderStateOutput::Unchanged,
        WireOutput::Replace { state } => BrainWakeProviderStateOutput::Replace {
            state: rusty_crew_core_bridge_api::BrainWakeProviderStateUpdate {
                module_id: state.module_id,
                strategy_id: state.strategy_id,
                profile_fingerprint: state.profile_fingerprint,
                provider_fingerprint: state.provider_fingerprint,
                payload_version: state.payload_version,
                payload: state.payload,
                ttl_ms: state.ttl_ms,
            },
        },
        WireOutput::Clear { reason } => BrainWakeProviderStateOutput::Clear {
            reason: match reason {
                WireClearReason::BrainRequestedClear => {
                    rusty_crew_core_bridge_api::ProviderStateClearReason::BrainRequestedClear
                }
            },
        },
    })
}

pub(crate) fn provider_state_absence_reason_as_str(
    reason: &rusty_crew_core_bridge_api::ProviderStateAbsenceReason,
) -> &'static str {
    match reason {
        rusty_crew_core_bridge_api::ProviderStateAbsenceReason::NotConfigured => "not_configured",
        rusty_crew_core_bridge_api::ProviderStateAbsenceReason::Missing => "missing",
        rusty_crew_core_bridge_api::ProviderStateAbsenceReason::Expired => "expired",
        rusty_crew_core_bridge_api::ProviderStateAbsenceReason::Invalidated => "invalidated",
        rusty_crew_core_bridge_api::ProviderStateAbsenceReason::ModuleDoesNotUseState => {
            "module_does_not_use_state"
        }
        rusty_crew_core_bridge_api::ProviderStateAbsenceReason::LoadFailed => "load_failed",
    }
}

pub(crate) fn parse_provider_state_absence_reason(
    raw: &str,
) -> CoreResult<rusty_crew_core_bridge_api::ProviderStateAbsenceReason> {
    Ok(match raw {
        "not_configured" => rusty_crew_core_bridge_api::ProviderStateAbsenceReason::NotConfigured,
        "missing" => rusty_crew_core_bridge_api::ProviderStateAbsenceReason::Missing,
        "expired" => rusty_crew_core_bridge_api::ProviderStateAbsenceReason::Expired,
        "invalidated" => rusty_crew_core_bridge_api::ProviderStateAbsenceReason::Invalidated,
        "module_does_not_use_state" => {
            rusty_crew_core_bridge_api::ProviderStateAbsenceReason::ModuleDoesNotUseState
        }
        "load_failed" => rusty_crew_core_bridge_api::ProviderStateAbsenceReason::LoadFailed,
        other => {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("invalid provider state absence reason {other}"),
            ))
        }
    })
}

pub(crate) fn provider_wire_state_status(
    invalidated_at: Option<&String>,
    invalidation_reason: Option<&str>,
    expires_at: Option<&String>,
    now: &String,
) -> &'static str {
    if invalidation_reason == Some("expired") {
        return "expired";
    }
    if invalidated_at.is_some() {
        return "invalidated";
    }
    if expires_at.is_some_and(|expires| expires <= now) {
        return "expired";
    }
    "valid"
}
