use super::*;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RevisionedRuntimeWrite {
    registration: ExternalRuntimeRegistration,
    expected_revision: Option<u64>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RevisionedBindingWrite {
    binding: ExternalAgentBinding,
    expected_revision: Option<u64>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExternalAgentSessionCreationTransition {
    controller: ExternalControllerContext,
    creation_id: ExternalAgentSessionCreationId,
    expected_revision: u64,
    now: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExternalAgentSessionCreationCompletion {
    controller: ExternalControllerContext,
    creation_id: ExternalAgentSessionCreationId,
    expected_revision: u64,
    native_thread_id: String,
    now: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExternalAgentSessionCreationFailure {
    controller: ExternalControllerContext,
    creation_id: ExternalAgentSessionCreationId,
    expected_revision: u64,
    reason_code: String,
    reason_message: String,
    now: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ControllerAcquire {
    lease: ExternalControllerLease,
    now: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ControllerRelease {
    runtime_id: ExternalRuntimeId,
    holder_instance_id: String,
    generation: u64,
    now: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct TurnTransition {
    controller: ExternalControllerContext,
    request_id: ExternalTurnRequestId,
    next_phase: ExternalTurnPhase,
    native_turn_id: Option<String>,
    terminal_reason_code: Option<String>,
    terminal_error: Option<ExternalTurnTerminalError>,
    now: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ControlCompletion {
    controller: ExternalControllerContext,
    control_id: ExternalControlId,
    status: ExternalControlStatus,
    outcome: Option<serde_json::Value>,
    reason_code: Option<String>,
    now: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ControllerInteraction {
    controller: ExternalControllerContext,
    interaction: ExternalInteractionRecord,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ControllerRuntimeEvent {
    controller: ExternalControllerContext,
    event: ExternalRuntimeEventInput,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct InteractionResolution {
    interaction: ExternalInteractionRecord,
    expected_revision: u64,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ControllerInteractionResolution {
    controller: ExternalControllerContext,
    interaction: ExternalInteractionRecord,
    expected_revision: u64,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeEventQuery {
    runtime_id: ExternalRuntimeId,
    after_sequence: u64,
    limit: u32,
}

#[napi_derive::napi]
impl NativeBridgeBinding {
    #[napi]
    pub fn register_external_runtime_json(&self, input_json: String) -> napi::Result<String> {
        let input = parse_json::<RevisionedRuntimeWrite>(&input_json)?;
        serialize_json(
            &self
                .bridge()?
                .engine()
                .map_err(to_napi_error)?
                .register_external_runtime(&input.registration, input.expected_revision)
                .map_err(to_napi_error)?,
        )
    }

    #[napi]
    pub fn authorize_external_runtime_handshake_json(
        &self,
        input_json: String,
    ) -> napi::Result<String> {
        let observation = parse_json::<ExternalRuntimeHandshakeObservation>(&input_json)?;
        serialize_json(
            &self
                .bridge()?
                .engine()
                .map_err(to_napi_error)?
                .authorize_external_runtime_handshake(&observation)
                .map_err(to_napi_error)?,
        )
    }

    #[napi]
    pub fn record_external_runtime_state_json(&self, input_json: String) -> napi::Result<String> {
        let observation = parse_json::<ExternalRuntimeStateObservation>(&input_json)?;
        serialize_json(
            &self
                .bridge()?
                .engine()
                .map_err(to_napi_error)?
                .record_external_runtime_state(&observation)
                .map_err(to_napi_error)?,
        )
    }

    #[napi]
    pub fn list_external_runtimes_json(&self) -> napi::Result<String> {
        serialize_json(
            &self
                .bridge()?
                .engine()
                .map_err(to_napi_error)?
                .list_external_runtimes()
                .map_err(to_napi_error)?,
        )
    }

    #[napi]
    pub fn get_external_runtime_json(&self, runtime_id: String) -> napi::Result<String> {
        serialize_json(
            &self
                .bridge()?
                .engine()
                .map_err(to_napi_error)?
                .get_external_runtime(&ExternalRuntimeId::new(runtime_id))
                .map_err(to_napi_error)?,
        )
    }

    #[napi]
    pub fn certify_external_runtime_json(&self, input_json: String) -> napi::Result<String> {
        let request = parse_json::<ExternalRuntimeCertificationRequest>(&input_json)?;
        serialize_json(
            &self
                .bridge()?
                .engine()
                .map_err(to_napi_error)?
                .certify_external_runtime(&request)
                .map_err(to_napi_error)?,
        )
    }

    #[napi]
    pub fn invalidate_external_runtime_certification_json(
        &self,
        input_json: String,
    ) -> napi::Result<String> {
        let invalidation = parse_json::<ExternalRuntimeCertificationInvalidation>(&input_json)?;
        serialize_json(
            &self
                .bridge()?
                .engine()
                .map_err(to_napi_error)?
                .invalidate_external_runtime_certification(&invalidation)
                .map_err(to_napi_error)?,
        )
    }

    #[napi]
    pub fn list_external_runtime_certifications_json(&self) -> napi::Result<String> {
        serialize_json(
            &self
                .bridge()?
                .engine()
                .map_err(to_napi_error)?
                .list_external_runtime_certifications()
                .map_err(to_napi_error)?,
        )
    }

    #[napi]
    pub fn get_external_runtime_certification_json(
        &self,
        certification_id: String,
    ) -> napi::Result<String> {
        serialize_json(
            &self
                .bridge()?
                .engine()
                .map_err(to_napi_error)?
                .get_external_runtime_certification(&certification_id)
                .map_err(to_napi_error)?,
        )
    }

    #[napi]
    pub fn acquire_external_controller_json(&self, input_json: String) -> napi::Result<String> {
        let input = parse_json::<ControllerAcquire>(&input_json)?;
        serialize_json(
            &self
                .bridge()?
                .engine()
                .map_err(to_napi_error)?
                .acquire_external_runtime_controller(&input.lease, &input.now)
                .map_err(to_napi_error)?,
        )
    }

    #[napi]
    pub fn release_external_controller_json(&self, input_json: String) -> napi::Result<String> {
        let input = parse_json::<ControllerRelease>(&input_json)?;
        serialize_json(
            &self
                .bridge()?
                .engine()
                .map_err(to_napi_error)?
                .release_external_runtime_controller(
                    &input.runtime_id,
                    &input.holder_instance_id,
                    input.generation,
                    &input.now,
                )
                .map_err(to_napi_error)?,
        )
    }

    #[napi]
    pub fn bind_external_agent_json(&self, input_json: String) -> napi::Result<String> {
        let input = parse_json::<RevisionedBindingWrite>(&input_json)?;
        serialize_json(
            &self
                .bridge()?
                .engine()
                .map_err(to_napi_error)?
                .bind_external_agent(&input.binding, input.expected_revision)
                .map_err(to_napi_error)?,
        )
    }

    #[napi]
    pub fn list_external_bindings_json(&self) -> napi::Result<String> {
        serialize_json(
            &self
                .bridge()?
                .engine()
                .map_err(to_napi_error)?
                .list_external_bindings()
                .map_err(to_napi_error)?,
        )
    }

    #[napi]
    pub fn update_external_binding_metadata_json(
        &self,
        input_json: String,
    ) -> napi::Result<String> {
        let write = parse_json::<ExternalAgentBindingMetadataWrite>(&input_json)?;
        serialize_json(
            &self
                .bridge()?
                .engine()
                .map_err(to_napi_error)?
                .update_external_binding_metadata(&write)
                .map_err(to_napi_error)?,
        )
    }

    #[napi]
    pub fn get_external_binding_json(&self, binding_id: String) -> napi::Result<String> {
        serialize_json(
            &self
                .bridge()?
                .engine()
                .map_err(to_napi_error)?
                .get_external_binding(&ExternalBindingId::new(binding_id))
                .map_err(to_napi_error)?,
        )
    }

    #[napi]
    pub fn prepare_external_agent_session_creation_json(
        &self,
        input_json: String,
    ) -> napi::Result<String> {
        let request = parse_json::<ExternalAgentSessionCreationRequest>(&input_json)?;
        serialize_json(
            &self
                .bridge()?
                .engine()
                .map_err(to_napi_error)?
                .prepare_external_agent_session_creation(request)
                .map_err(to_napi_error)?,
        )
    }

    #[napi]
    pub fn mark_external_agent_session_native_starting_json(
        &self,
        input_json: String,
    ) -> napi::Result<String> {
        let input = parse_json::<ExternalAgentSessionCreationTransition>(&input_json)?;
        serialize_json(
            &self
                .bridge()?
                .engine()
                .map_err(to_napi_error)?
                .mark_external_agent_session_native_starting(
                    &input.controller,
                    &input.creation_id,
                    input.expected_revision,
                    input.now,
                )
                .map_err(to_napi_error)?,
        )
    }

    #[napi]
    pub fn complete_external_agent_session_creation_json(
        &self,
        input_json: String,
    ) -> napi::Result<String> {
        let input = parse_json::<ExternalAgentSessionCreationCompletion>(&input_json)?;
        serialize_json(
            &self
                .bridge()?
                .engine()
                .map_err(to_napi_error)?
                .complete_external_agent_session_creation(
                    &input.controller,
                    &input.creation_id,
                    input.expected_revision,
                    input.native_thread_id,
                    input.now,
                )
                .map_err(to_napi_error)?,
        )
    }

    #[napi]
    pub fn record_external_agent_session_creation_failure_json(
        &self,
        input_json: String,
    ) -> napi::Result<String> {
        let input = parse_json::<ExternalAgentSessionCreationFailure>(&input_json)?;
        serialize_json(
            &self
                .bridge()?
                .engine()
                .map_err(to_napi_error)?
                .record_external_agent_session_creation_failure(
                    &input.controller,
                    &input.creation_id,
                    input.expected_revision,
                    input.reason_code,
                    input.reason_message,
                    input.now,
                )
                .map_err(to_napi_error)?,
        )
    }

    #[napi]
    pub fn get_external_turn_json(&self, request_id: String) -> napi::Result<String> {
        serialize_json(
            &self
                .bridge()?
                .engine()
                .map_err(to_napi_error)?
                .get_external_turn(&ExternalTurnRequestId::new(request_id))
                .map_err(to_napi_error)?,
        )
    }

    #[napi]
    pub fn list_external_turns_for_native_thread_json(
        &self,
        runtime_id: String,
        native_thread_id: String,
    ) -> napi::Result<String> {
        serialize_json(
            &self
                .bridge()?
                .engine()
                .map_err(to_napi_error)?
                .list_external_turns_for_native_thread(
                    &ExternalRuntimeId::new(runtime_id),
                    &native_thread_id,
                )
                .map_err(to_napi_error)?,
        )
    }

    #[napi]
    pub fn list_active_external_turns_json(&self) -> napi::Result<String> {
        serialize_json(
            &self
                .bridge()?
                .engine()
                .map_err(to_napi_error)?
                .list_active_external_turns()
                .map_err(to_napi_error)?,
        )
    }

    #[napi]
    pub fn expire_external_turn_dispatches_json(&self, now: String) -> napi::Result<String> {
        serialize_json(
            &self
                .bridge()?
                .engine()
                .map_err(to_napi_error)?
                .expire_external_turn_dispatches(&now)
                .map_err(to_napi_error)?,
        )
    }

    #[napi]
    pub fn transition_external_turn_json(&self, input_json: String) -> napi::Result<String> {
        let input = parse_json::<TurnTransition>(&input_json)?;
        serialize_json(
            &self
                .bridge()?
                .engine()
                .map_err(to_napi_error)?
                .transition_external_turn_from_controller(
                    &input.controller,
                    rusty_crew_core_engine::ExternalControllerTurnTransition {
                        request_id: input.request_id,
                        next_phase: input.next_phase,
                        native_turn_id: input.native_turn_id,
                        terminal_reason_code: input.terminal_reason_code,
                        terminal_error: input.terminal_error,
                        now: input.now,
                    },
                )
                .map_err(to_napi_error)?,
        )
    }

    #[napi]
    pub fn submit_external_control_json(&self, input_json: String) -> napi::Result<String> {
        let request = parse_json::<ExternalControlRequest>(&input_json)?;
        serialize_json(
            &self
                .bridge()?
                .engine()
                .map_err(to_napi_error)?
                .submit_external_control(request)
                .map_err(to_napi_error)?,
        )
    }

    #[napi]
    pub fn complete_external_control_json(&self, input_json: String) -> napi::Result<String> {
        let input = parse_json::<ControlCompletion>(&input_json)?;
        serialize_json(
            &self
                .bridge()?
                .engine()
                .map_err(to_napi_error)?
                .complete_external_control_from_controller(
                    &input.controller,
                    &input.control_id,
                    input.status,
                    input.outcome,
                    input.reason_code,
                    input.now,
                )
                .map_err(to_napi_error)?,
        )
    }

    #[napi]
    pub fn record_external_interaction_json(&self, input_json: String) -> napi::Result<String> {
        let input = parse_json::<ControllerInteraction>(&input_json)?;
        serialize_json(
            &self
                .bridge()?
                .engine()
                .map_err(to_napi_error)?
                .record_external_interaction_from_controller(&input.controller, &input.interaction)
                .map_err(to_napi_error)?,
        )
    }

    #[napi]
    pub fn resolve_external_interaction_json(&self, input_json: String) -> napi::Result<String> {
        let input = parse_json::<InteractionResolution>(&input_json)?;
        serialize_json(
            &self
                .bridge()?
                .engine()
                .map_err(to_napi_error)?
                .resolve_external_interaction(&input.interaction, input.expected_revision)
                .map_err(to_napi_error)?,
        )
    }

    #[napi]
    pub fn terminalize_external_interaction_json(
        &self,
        input_json: String,
    ) -> napi::Result<String> {
        let input = parse_json::<ControllerInteractionResolution>(&input_json)?;
        serialize_json(
            &self
                .bridge()?
                .engine()
                .map_err(to_napi_error)?
                .terminalize_external_interaction_from_controller(
                    &input.controller,
                    &input.interaction,
                    input.expected_revision,
                )
                .map_err(to_napi_error)?,
        )
    }

    #[napi]
    pub fn list_pending_external_interactions_json(&self) -> napi::Result<String> {
        serialize_json(
            &self
                .bridge()?
                .engine()
                .map_err(to_napi_error)?
                .list_pending_external_interactions()
                .map_err(to_napi_error)?,
        )
    }

    #[napi]
    pub fn record_external_runtime_event_json(&self, input_json: String) -> napi::Result<String> {
        let input = parse_json::<ControllerRuntimeEvent>(&input_json)?;
        serialize_json(
            &self
                .bridge()?
                .engine()
                .map_err(to_napi_error)?
                .record_external_runtime_event(&input.controller, &input.event)
                .map_err(to_napi_error)?,
        )
    }

    #[napi]
    pub fn query_external_runtime_events_json(&self, input_json: String) -> napi::Result<String> {
        let input = parse_json::<RuntimeEventQuery>(&input_json)?;
        serialize_json(
            &self
                .bridge()?
                .engine()
                .map_err(to_napi_error)?
                .query_external_runtime_events(&input.runtime_id, input.after_sequence, input.limit)
                .map_err(to_napi_error)?,
        )
    }
}

fn parse_json<T: serde::de::DeserializeOwned>(input: &str) -> napi::Result<T> {
    serde_json::from_str(input)
        .map_err(|error| napi::Error::new(napi::Status::InvalidArg, error.to_string()))
}

fn serialize_json<T: serde::Serialize>(value: &T) -> napi::Result<String> {
    serde_json::to_string(value)
        .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
}
