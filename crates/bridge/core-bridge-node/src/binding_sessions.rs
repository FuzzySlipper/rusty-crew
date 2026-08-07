use super::*;

#[napi_derive::napi]
impl NativeBridgeBinding {
    #[napi]
    pub fn register_brain_implementation(
        &self,
        registration: JsBrainImplementationRegistration,
    ) -> napi::Result<f64> {
        let mut bridge = self.bridge()?;
        let handle = bridge
            .register_brain_implementation(to_brain_registration(registration)?)
            .map_err(to_napi_error)?;
        Ok(handle.get() as f64)
    }

    #[napi]
    pub fn replace_brain_implementation(
        &self,
        registration: JsBrainImplementationRegistration,
    ) -> napi::Result<f64> {
        let mut bridge = self.bridge()?;
        let handle = bridge
            .replace_brain_implementation(to_brain_registration(registration)?)
            .map_err(to_napi_error)?;
        Ok(handle.get() as f64)
    }

    #[napi]
    pub fn unregister_brain_implementation_for_profile(
        &self,
        profile_id: String,
    ) -> napi::Result<f64> {
        let mut bridge = self.bridge()?;
        let handle = bridge
            .unregister_brain_implementation_for_profile(ProfileId::new(profile_id))
            .map_err(to_napi_error)?;
        Ok(handle.get() as f64)
    }

    #[napi]
    pub fn build_brain_wake_request(
        &self,
        brain: f64,
        session_id: String,
        body_state_json: napi::bindgen_prelude::Buffer,
        system_prompt: String,
        role_assembly_json: napi::bindgen_prelude::Buffer,
        wake_id: String,
    ) -> napi::Result<JsBufferedBrainWakeRequest> {
        let bridge = self.bridge()?;
        let buffered = bridge
            .build_brain_wake_request(BrainWakeBufferInput {
                brain: BrainImplementationHandle::new(brain as u64),
                session_id: rusty_crew_core_bridge_api::SessionId::new(session_id),
                body_state_json: body_state_json.to_vec(),
                system_prompt,
                role_assembly_json: role_assembly_json.to_vec(),
                wake_id,
                compaction_intent: None,
            })
            .map_err(to_napi_error)?;
        Ok(JsBufferedBrainWakeRequest {
            body_state: handle_to_u32(buffered.request.body_state)?,
            system_prompt: handle_to_u32(buffered.request.system_prompt)?,
            role_assembly: handle_to_u32(buffered.request.role_assembly)?,
            continuation_state_json: buffered
                .request
                .continuation_state
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(|error| {
                    napi::Error::new(napi::Status::GenericFailure, error.to_string())
                })?,
            provider_state_json: buffered
                .request
                .provider_state
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(|error| {
                    napi::Error::new(napi::Status::GenericFailure, error.to_string())
                })?,
            provider_state_absence: buffered
                .request
                .provider_state_absence
                .as_ref()
                .map(provider_state_absence_reason_as_str)
                .map(str::to_string),
        })
    }

    #[napi]
    pub fn build_brain_wake_request_for_session(
        &self,
        brain: f64,
        session_id: String,
        system_prompt: String,
        role_assembly_json: napi::bindgen_prelude::Buffer,
        wake_id: String,
    ) -> napi::Result<JsBufferedBrainWakeRequest> {
        let mut bridge = self.bridge()?;
        let buffered = bridge
            .build_brain_wake_request_for_session(
                BrainImplementationHandle::new(brain as u64),
                rusty_crew_core_bridge_api::SessionId::new(session_id),
                system_prompt,
                role_assembly_json.to_vec(),
                wake_id,
            )
            .map_err(to_napi_error)?;
        Ok(JsBufferedBrainWakeRequest {
            body_state: handle_to_u32(buffered.request.body_state)?,
            system_prompt: handle_to_u32(buffered.request.system_prompt)?,
            role_assembly: handle_to_u32(buffered.request.role_assembly)?,
            continuation_state_json: buffered
                .request
                .continuation_state
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(|error| {
                    napi::Error::new(napi::Status::GenericFailure, error.to_string())
                })?,
            provider_state_json: buffered
                .request
                .provider_state
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(|error| {
                    napi::Error::new(napi::Status::GenericFailure, error.to_string())
                })?,
            provider_state_absence: buffered
                .request
                .provider_state_absence
                .as_ref()
                .map(provider_state_absence_reason_as_str)
                .map(str::to_string),
        })
    }

    #[napi]
    pub fn settle_brain_wake_json(&self, input_json: String) -> napi::Result<String> {
        let input: BrainWakeSettlementRequest =
            serde_json::from_str(&input_json).map_err(|error| {
                napi::Error::new(
                    napi::Status::InvalidArg,
                    format!("invalid brain wake settlement JSON: {error}"),
                )
            })?;
        let result = match input.outcome {
            BrainWakeSettlementKind::Completed => LogicalTurnEpochResult::Completed,
            BrainWakeSettlementKind::Yielded => {
                LogicalTurnEpochResult::Yielded(input.continuation_state.ok_or_else(|| {
                    napi::Error::new(
                        napi::Status::InvalidArg,
                        "yielded brain wake requires continuation state",
                    )
                })?)
            }
            BrainWakeSettlementKind::AttentionRequired => {
                LogicalTurnEpochResult::AttentionRequired {
                    module_state: input.continuation_state.ok_or_else(|| {
                        napi::Error::new(
                            napi::Status::InvalidArg,
                            "attention-required brain wake requires continuation state",
                        )
                    })?,
                    attention: input.attention.ok_or_else(|| {
                        napi::Error::new(
                            napi::Status::InvalidArg,
                            "attention-required brain wake requires attention details",
                        )
                    })?,
                }
            }
            BrainWakeSettlementKind::Failed => LogicalTurnEpochResult::Failed {
                reason_code: input
                    .reason_code
                    .unwrap_or_else(|| "brain_wake_failed".to_string()),
                summary: input
                    .summary
                    .unwrap_or_else(|| "brain wake failed".to_string()),
            },
        };
        let settlement = self
            .bridge()?
            .settle_brain_wake(&input.wake_id, result, input.progress)
            .map_err(to_napi_error)?;
        serialize_json(
            &BrainWakeSettlementReceipt {
                managed: settlement.is_some(),
                outcome: settlement
                    .as_ref()
                    .map(|settlement| settlement.outcome)
                    .unwrap_or(BrainWakeOutcome::Completed),
                phase: settlement.map(|settlement| settlement.phase),
            },
            "brain wake settlement",
        )
    }

    #[napi]
    pub fn logical_turn_diagnostics_json(&self, input_json: String) -> napi::Result<String> {
        let query: LogicalTurnDiagnosticQuery =
            serde_json::from_str(&input_json).map_err(|error| {
                napi::Error::new(
                    napi::Status::InvalidArg,
                    format!("invalid logical-turn diagnostic query JSON: {error}"),
                )
            })?;
        let page = self
            .bridge()?
            .logical_turn_diagnostics(&query)
            .map_err(to_napi_error)?;
        serialize_json(&page, "logical-turn diagnostics")
    }

    #[napi]
    pub fn requeue_logical_turn_continuations(&self) -> napi::Result<f64> {
        Ok(self
            .bridge()?
            .requeue_logical_turn_continuations()
            .map_err(to_napi_error)? as f64)
    }

    #[napi]
    pub fn resolve_logical_turn_attention_json(&self, input_json: String) -> napi::Result<String> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Input {
            logical_turn_id: LogicalTurnId,
            expected_revision: u64,
            action: LogicalTurnResolutionAction,
        }
        let input: Input = serde_json::from_str(&input_json).map_err(|error| {
            napi::Error::new(
                napi::Status::InvalidArg,
                format!("invalid logical-turn resolution JSON: {error}"),
            )
        })?;
        let receipt = self
            .bridge()?
            .resolve_logical_turn_attention_for_operator(
                &input.logical_turn_id,
                input.expected_revision,
                input.action,
            )
            .map_err(to_napi_error)?;
        serialize_json(&receipt, "logical-turn attention resolution")
    }

    #[napi]
    pub fn cancel_logical_turn_json(&self, input_json: String) -> napi::Result<String> {
        let input: LogicalTurnCancelRequest =
            serde_json::from_str(&input_json).map_err(|error| {
                napi::Error::new(
                    napi::Status::InvalidArg,
                    format!("invalid logical-turn cancellation JSON: {error}"),
                )
            })?;
        let receipt = self
            .bridge()?
            .cancel_logical_turn(&input)
            .map_err(to_napi_error)?;
        serialize_json(&receipt, "logical-turn cancellation")
    }

    #[napi]
    pub fn apply_brain_provider_state_output_json(
        &self,
        brain: f64,
        session_id: String,
        wake_id: String,
        output_json: String,
    ) -> napi::Result<()> {
        let bridge = self.bridge()?;
        let output = parse_brain_provider_state_output_json(&output_json).map_err(to_napi_error)?;
        bridge
            .apply_provider_state_output(
                BrainImplementationHandle::new(brain as u64),
                &rusty_crew_core_bridge_api::SessionId::new(session_id),
                &wake_id,
                output,
            )
            .map_err(to_napi_error)
    }

    #[napi]
    pub fn provider_state_diagnostics(
        &self,
        limit: Option<u32>,
    ) -> napi::Result<Vec<JsProviderStateDiagnostic>> {
        let bridge = self.bridge()?;
        bridge
            .provider_state_diagnostics(limit.unwrap_or(100))
            .map_err(to_napi_error)
    }

    #[napi]
    pub fn get_buffer(&self, handle: u32) -> napi::Result<JsRuntimeBufferView> {
        let bridge = self.bridge()?;
        let view = bridge
            .get_buffer(RuntimeBufferHandle::new(handle as u64))
            .map_err(to_napi_error)?;
        Ok(JsRuntimeBufferView {
            handle,
            media_type: view.media_type,
            byte_len: view.byte_len as f64,
            bytes: view.bytes.into(),
        })
    }

    #[napi]
    pub fn release_buffer(&self, handle: u32) -> napi::Result<()> {
        let bridge = self.bridge()?;
        bridge
            .release_buffer(RuntimeBufferHandle::new(handle as u64))
            .map_err(to_napi_error)?;
        Ok(())
    }

    #[napi]
    pub fn assert_no_buffer_leaks(&self) -> napi::Result<()> {
        let bridge = self.bridge()?;
        bridge.assert_no_buffer_leaks().map_err(to_napi_error)?;
        Ok(())
    }

    #[napi]
    pub fn create_session(&self, config: JsSessionConfig) -> napi::Result<JsSessionState> {
        let bridge = self.bridge()?;
        let state = bridge
            .create_session(js_session_config(config)?)
            .map_err(to_napi_error)?;
        Ok(to_js_session_state(state))
    }

    #[napi]
    pub fn ensure_configured_session(
        &self,
        config: JsSessionConfig,
    ) -> napi::Result<JsSessionState> {
        let bridge = self.bridge()?;
        let state = bridge
            .ensure_configured_session(js_session_config(config)?)
            .map_err(to_napi_error)?;
        Ok(to_js_session_state(state))
    }

    #[napi]
    pub fn create_crew_agent_session_json(&self, input_json: String) -> napi::Result<String> {
        let request = parse_json::<CrewAgentSessionCreationRequest>(
            &input_json,
            "Crew agent session creation request",
        )?;
        serialize_json(
            &self
                .bridge()?
                .engine()
                .map_err(to_napi_error)?
                .create_crew_agent_session(&request)
                .map_err(to_napi_error)?,
            "Crew agent session creation record",
        )
    }

    #[napi]
    pub fn archive_session(&self, session_id: String) -> napi::Result<JsSessionState> {
        let bridge = self.bridge()?;
        let state = bridge
            .archive_session(SessionId::new(session_id))
            .map_err(to_napi_error)?;
        Ok(to_js_session_state(state))
    }

    #[napi]
    pub fn set_session_reasoning_effort(
        &self,
        session_id: String,
        reasoning_effort: Option<String>,
    ) -> napi::Result<JsSessionState> {
        let bridge = self.bridge()?;
        let state = bridge
            .set_session_reasoning_effort(SessionId::new(session_id), reasoning_effort)
            .map_err(to_napi_error)?;
        Ok(to_js_session_state(state))
    }

    #[napi]
    pub fn list_sessions_json(&self) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let sessions = bridge.list_sessions().map_err(to_napi_error)?;
        serde_json::to_string(&sessions)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn route_agent_message(
        &self,
        from: String,
        to: String,
        body: String,
        correlation_id: Option<String>,
    ) -> napi::Result<JsEventReceipt> {
        let bridge = self.bridge()?;
        let receipt = bridge
            .route_agent_message(
                rusty_crew_core_bridge_api::AgentId::new(from),
                rusty_crew_core_bridge_api::AgentId::new(to),
                body,
                correlation_id,
            )
            .map_err(to_napi_error)?;
        Ok(to_js_event_receipt(receipt))
    }

    #[napi]
    pub fn enqueue_body_follow_up_message(
        &self,
        session_id: String,
        from: String,
        body: String,
        correlation_id: Option<String>,
    ) -> napi::Result<JsQueuedMessageRecord> {
        let bridge = self.bridge()?;
        let record = bridge
            .enqueue_body_follow_up_message(
                rusty_crew_core_bridge_api::SessionId::new(session_id),
                rusty_crew_core_bridge_api::AgentId::new(from),
                body,
                correlation_id,
            )
            .map_err(to_napi_error)?;
        Ok(to_js_queued_message_record(record))
    }

    #[napi]
    pub fn project_body_state_json(
        &self,
        session_id: String,
    ) -> napi::Result<napi::bindgen_prelude::Buffer> {
        let bridge = self.bridge()?;
        let bytes = bridge
            .project_body_state_json(rusty_crew_core_bridge_api::SessionId::new(session_id))
            .map_err(to_napi_error)?;
        Ok(bytes.into())
    }

    #[napi]
    pub fn submit_brain_actions_json(
        &self,
        wake_id: String,
        session_id: String,
        actions_json: napi::bindgen_prelude::Buffer,
    ) -> napi::Result<JsActionBatchReceipt> {
        let bridge = self.bridge()?;
        let actions = serde_json::from_slice(actions_json.as_ref()).map_err(|error| {
            napi::Error::new(
                napi::Status::InvalidArg,
                format!("invalid brain action JSON: {error}"),
            )
        })?;
        let receipt = bridge
            .submit_brain_actions(BrainActionBatch {
                wake_id,
                session_id: rusty_crew_core_bridge_api::SessionId::new(session_id),
                actions,
            })
            .map_err(to_napi_error)?;
        Ok(JsActionBatchReceipt {
            wake_id: receipt.wake_id,
            accepted_actions: receipt.accepted_actions,
            rejected_actions_json: serde_json::to_string(&receipt.rejected_actions).map_err(
                |error| napi::Error::new(napi::Status::GenericFailure, error.to_string()),
            )?,
        })
    }
}
