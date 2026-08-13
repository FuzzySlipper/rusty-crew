use super::*;
use rusty_crew_core_bridge_api::{
    runtime_dispatch_activity_id, runtime_provider_activity_id, runtime_tool_activity_id,
    runtime_wake_activity_id,
};

const CHAT_COMPLETIONS_MODULE_ID: &str = "chat-completions";
const OPENAI_RESPONSES_MODULE_ID: &str = "openai-responses";

fn unsupported_brain_module(module_id: &str) -> napi::Error {
    napi::Error::new(
        napi::Status::InvalidArg,
        format!("Rust brain catalog module {module_id} has no buffered run host"),
    )
}

fn attach_brain_module_id(module_id: &str, raw_json: String) -> napi::Result<String> {
    let mut value = serde_json::from_str::<serde_json::Value>(&raw_json).map_err(|error| {
        napi::Error::new(
            napi::Status::GenericFailure,
            format!("invalid {module_id} buffered brain result JSON: {error}"),
        )
    })?;
    let object = value.as_object_mut().ok_or_else(|| {
        napi::Error::new(
            napi::Status::GenericFailure,
            format!("{module_id} buffered brain result must be an object"),
        )
    })?;
    object.insert(
        "module_id".to_string(),
        serde_json::Value::String(module_id.to_string()),
    );
    serde_json::to_string(&value).map_err(|error| {
        napi::Error::new(
            napi::Status::GenericFailure,
            format!("serialize {module_id} buffered brain result: {error}"),
        )
    })
}

#[napi_derive::napi]
impl NativeBridgeBinding {
    #[napi]
    pub fn start_brain_run_json(
        &self,
        module_id: String,
        input_json: String,
    ) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let identity = parse_brain_run_identity(&input_json)?;
        lease_provider_operation(&bridge, &identity.wake_id, &input_json)
            .map_err(core_error_to_napi)?;
        begin_native_brain_activities(&bridge, &module_id, &identity);
        let result = match module_id.as_str() {
            CHAT_COMPLETIONS_MODULE_ID => start_chat_completions_brain_json(
                bridge.chat_completions_buffered_runs(),
                input_json,
            ),
            OPENAI_RESPONSES_MODULE_ID => start_openai_responses_brain_json(
                bridge.openai_responses_buffered_runs(),
                input_json,
            ),
            _ => return Err(unsupported_brain_module(&module_id)),
        };
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                let _ = complete_provider_operation(
                    &bridge,
                    &identity.wake_id,
                    serde_json::json!({
                        "status": "failed_to_start",
                        "message": error.to_string(),
                    }),
                );
                finish_native_brain_activity_tree(
                    &bridge,
                    &identity.wake_id,
                    RuntimeActivityStatus::Failed,
                    Some("native_start_failed"),
                    "native brain run failed to start",
                );
                return Err(error);
            }
        };
        attach_brain_module_id(&module_id, result)
    }

    #[napi]
    pub fn drain_brain_run_json(
        &self,
        module_id: String,
        wake_id: String,
        max_items: Option<u32>,
    ) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let result = match module_id.as_str() {
            CHAT_COMPLETIONS_MODULE_ID => drain_chat_completions_brain_stream_json(
                &bridge.chat_completions_buffered_runs(),
                wake_id.clone(),
                max_items,
            ),
            OPENAI_RESPONSES_MODULE_ID => drain_openai_responses_brain_stream_json(
                &bridge.openai_responses_buffered_runs(),
                wake_id.clone(),
                max_items,
            ),
            _ => return Err(unsupported_brain_module(&module_id)),
        }?;
        let result = prepare_native_brain_drain(&bridge, &module_id, &wake_id, result)
            .map_err(core_error_to_napi)?;
        attach_brain_module_id(&module_id, result)
    }

    #[napi]
    pub fn submit_brain_host_result_json(
        &self,
        module_id: String,
        input_json: String,
    ) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let activity_identity = parse_tool_result_identity(&input_json);
        let deliver_to_provider =
            complete_host_tool_operation(&bridge, &input_json).map_err(core_error_to_napi)?;
        if !deliver_to_provider {
            let value =
                serde_json::from_str::<serde_json::Value>(&input_json).map_err(|error| {
                    napi::Error::new(
                        napi::Status::InvalidArg,
                        format!("invalid host tool result JSON: {error}"),
                    )
                })?;
            return attach_brain_module_id(
                &module_id,
                serde_json::json!({
                    "ok": true,
                    "wake_id": value.get("wakeId").and_then(|value| value.as_str()),
                    "call_id": value.get("callId").and_then(|value| value.as_str()),
                    "receipt": "completed_after_cancel",
                })
                .to_string(),
            );
        }
        let result = match module_id.as_str() {
            CHAT_COMPLETIONS_MODULE_ID => submit_chat_completions_tool_output_json(
                &bridge.chat_completions_buffered_runs(),
                input_json,
            ),
            OPENAI_RESPONSES_MODULE_ID => submit_openai_responses_tool_output_json(
                &bridge.openai_responses_buffered_runs(),
                input_json,
            ),
            _ => return Err(unsupported_brain_module(&module_id)),
        };
        if let Some((wake_id, call_id, failed)) = activity_identity {
            let (status, reason_code, summary) = match (&result, failed) {
                (Ok(_), false) => (
                    RuntimeActivityStatus::Completed,
                    None,
                    "host tool call completed",
                ),
                (Ok(_), true) => (
                    RuntimeActivityStatus::Failed,
                    Some("tool_result_failed"),
                    "host tool call returned an error",
                ),
                (Err(_), _) => (
                    RuntimeActivityStatus::Failed,
                    Some("tool_result_submission_failed"),
                    "host tool result submission failed",
                ),
            };
            finish_native_activity(
                &bridge,
                runtime_tool_activity_id(&wake_id, &call_id),
                status,
                reason_code,
                summary,
            );
        }
        let result = result?;
        attach_brain_module_id(&module_id, result)
    }

    #[napi]
    pub fn cancel_brain_run_json(
        &self,
        module_id: String,
        input_json: String,
    ) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let wake_id = serde_json::from_str::<serde_json::Value>(&input_json)
            .ok()
            .and_then(|value| {
                value
                    .get("wakeId")
                    .and_then(|value| value.as_str())
                    .map(str::to_string)
            });
        let result = match module_id.as_str() {
            CHAT_COMPLETIONS_MODULE_ID => cancel_chat_completions_brain_json(
                &bridge.chat_completions_buffered_runs(),
                input_json,
            ),
            OPENAI_RESPONSES_MODULE_ID => cancel_openai_responses_brain_json(
                &bridge.openai_responses_buffered_runs(),
                input_json,
            ),
            _ => return Err(unsupported_brain_module(&module_id)),
        }?;
        if let Some(wake_id) = wake_id {
            finish_native_brain_activity_tree(
                &bridge,
                &wake_id,
                RuntimeActivityStatus::Cancelled,
                Some("operator_cancelled"),
                "native brain run was cancelled",
            );
        }
        attach_brain_module_id(&module_id, result)
    }
}

struct BrainRunIdentity {
    wake_id: String,
    session_id: SessionId,
    model_config_id: Option<String>,
    endpoint_id: Option<String>,
    model: Option<String>,
}

fn parse_brain_run_identity(input_json: &str) -> napi::Result<BrainRunIdentity> {
    let value = serde_json::from_str::<serde_json::Value>(input_json).map_err(|error| {
        napi::Error::new(
            napi::Status::InvalidArg,
            format!("invalid buffered brain run JSON: {error}"),
        )
    })?;
    let required = |name: &str| {
        value
            .get(name)
            .and_then(|value| value.as_str())
            .map(str::to_string)
            .ok_or_else(|| {
                napi::Error::new(
                    napi::Status::InvalidArg,
                    format!("buffered brain run requires {name}"),
                )
            })
    };
    Ok(BrainRunIdentity {
        wake_id: required("wakeId")?,
        session_id: SessionId::new(required("sessionId")?),
        model_config_id: value
            .pointer("/config/modelConfigId")
            .and_then(|value| value.as_str())
            .map(str::to_string),
        endpoint_id: value
            .pointer("/config/endpointId")
            .and_then(|value| value.as_str())
            .map(str::to_string),
        model: value
            .pointer("/config/model")
            .and_then(|value| value.as_str())
            .map(str::to_string),
    })
}

fn begin_native_brain_activities(
    bridge: &NativeBridge,
    module_id: &str,
    identity: &BrainRunIdentity,
) {
    let Ok(engine) = bridge.engine() else {
        return;
    };
    let Ok(session) = engine.get_session(&identity.session_id) else {
        eprintln!(
            "runtime activity ledger could not resolve session {} for wake {}",
            identity.session_id.0, identity.wake_id
        );
        return;
    };
    for input in [
        RuntimeActivityBegin {
            activity_id: runtime_wake_activity_id(&identity.wake_id),
            parent_activity_id: Some(runtime_dispatch_activity_id(&identity.wake_id)),
            kind: RuntimeActivityKind::Wake,
            owner: RuntimeActivityOwner::RustBrain,
            agent_id: Some(session.agent_id.clone()),
            profile_id: Some(session.profile_id.clone()),
            session_id: Some(session.session_id.clone()),
            wake_id: Some(identity.wake_id.clone()),
            phase: "running".into(),
            summary: Some(format!("{module_id} native brain wake")),
            provider_alias: None,
            model_config_id: identity.model_config_id.clone(),
            endpoint_id: identity.endpoint_id.clone(),
            model: identity.model.clone(),
            tool_name: None,
            process_id: None,
            debug_detail_id: None,
        },
        RuntimeActivityBegin {
            activity_id: runtime_provider_activity_id(&identity.wake_id),
            parent_activity_id: Some(runtime_wake_activity_id(&identity.wake_id)),
            kind: RuntimeActivityKind::ProviderRequest,
            owner: RuntimeActivityOwner::RustBrain,
            agent_id: Some(session.agent_id.clone()),
            profile_id: Some(session.profile_id.clone()),
            session_id: Some(session.session_id.clone()),
            wake_id: Some(identity.wake_id.clone()),
            phase: "provider_stream".into(),
            summary: Some(format!("{module_id} provider loop")),
            provider_alias: None,
            model_config_id: identity.model_config_id.clone(),
            endpoint_id: identity.endpoint_id.clone(),
            model: identity.model.clone(),
            tool_name: None,
            process_id: None,
            debug_detail_id: None,
        },
    ] {
        if let Err(error) = bridge.begin_runtime_activity(input) {
            if error.kind != CoreErrorKind::AlreadyExists {
                eprintln!("runtime activity begin failed: {error}");
            }
        }
    }
}

fn prepare_native_brain_drain(
    bridge: &NativeBridge,
    module_id: &str,
    wake_id: &str,
    output_json: String,
) -> CoreResult<String> {
    let mut value = serde_json::from_str::<serde_json::Value>(&output_json).map_err(|error| {
        CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("invalid native brain drain JSON: {error}"),
        )
    })?;
    if let Some(tool_requests) = value
        .get_mut("tool_requests")
        .and_then(|value| value.as_array_mut())
    {
        let mut dispatchable = Vec::with_capacity(tool_requests.len());
        for request in std::mem::take(tool_requests) {
            if prepare_host_tool_operation(bridge, module_id, wake_id, &request)? {
                dispatchable.push(request);
            }
        }
        *tool_requests = dispatchable;
        let session = bridge
            .buffered_brain_run_diagnostics()
            .ok()
            .and_then(|diagnostics| {
                diagnostics
                    .runs
                    .into_iter()
                    .find(|run| run.wake_id == wake_id)
            });
        for request in tool_requests.iter() {
            let Some(call_id) = request.get("call_id").and_then(|value| value.as_str()) else {
                continue;
            };
            let Some(name) = request.get("name").and_then(|value| value.as_str()) else {
                continue;
            };
            let input = RuntimeActivityBegin {
                activity_id: runtime_tool_activity_id(wake_id, call_id),
                parent_activity_id: Some(runtime_provider_activity_id(wake_id)),
                kind: RuntimeActivityKind::ToolCall,
                owner: RuntimeActivityOwner::TypeScriptHost,
                agent_id: session
                    .as_ref()
                    .and_then(|run| run.agent_id.clone())
                    .map(rusty_crew_core_bridge_api::AgentId::new),
                profile_id: session
                    .as_ref()
                    .and_then(|run| run.profile_id.clone())
                    .map(ProfileId::new),
                session_id: session
                    .as_ref()
                    .map(|run| SessionId::new(run.session_id.clone())),
                wake_id: Some(wake_id.into()),
                phase: "awaiting_host".into(),
                summary: Some(format!("{module_id} host tool call")),
                provider_alias: None,
                model_config_id: None,
                endpoint_id: None,
                model: None,
                tool_name: Some(name.into()),
                process_id: None,
                debug_detail_id: None,
            };
            if let Err(error) = bridge.begin_runtime_activity(input) {
                if error.kind != CoreErrorKind::AlreadyExists {
                    eprintln!("runtime tool activity begin failed: {error}");
                }
            }
        }
    }

    if value
        .get("terminal")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        let cancelled = value
            .get("cancellation")
            .is_some_and(|value| !value.is_null());
        let failed = value.get("error").is_some_and(|value| !value.is_null());
        let (status, default_reason, summary) = if cancelled {
            (
                RuntimeActivityStatus::Cancelled,
                Some("provider_cancelled"),
                "native brain run was cancelled",
            )
        } else if failed {
            (
                RuntimeActivityStatus::Failed,
                Some("provider_failed"),
                "native brain run failed",
            )
        } else {
            (
                RuntimeActivityStatus::Completed,
                None,
                "native brain run completed",
            )
        };
        let reason_code = value
            .get("terminal_reason_code")
            .and_then(|value| value.as_str())
            .or(default_reason);
        complete_provider_operation(
            bridge,
            wake_id,
            serde_json::json!({
                "status": if cancelled { "cancelled" } else if failed { "failed" } else { "completed" },
                "reasonCode": reason_code,
                "summary": summary,
            }),
        )?;
        finish_native_brain_activity_tree(bridge, wake_id, status, reason_code, summary);
    }
    serde_json::to_string(&value).map_err(|error| {
        CoreError::new(
            CoreErrorKind::InternalError,
            format!("serialize native brain drain JSON: {error}"),
        )
    })
}

fn lease_provider_operation(
    bridge: &NativeBridge,
    wake_id: &str,
    input_json: &str,
) -> CoreResult<()> {
    let Some(active) = bridge.active_logical_wakes.get(wake_id).cloned() else {
        return Ok(());
    };
    let epoch_id = active.claim.record.active_epoch_id.clone().ok_or_else(|| {
        CoreError::new(
            CoreErrorKind::ActionRejected,
            "active logical wake has no execution epoch",
        )
    })?;
    let base_operation_id = format!(
        "operation:{}:{}:provider:{}",
        active.claim.record.logical_turn_id.0,
        active.claim.record.current_continuation_id.0,
        sha256_text(&epoch_id.0)
    );
    let sequence = active
        .next_provider_operation
        .fetch_add(1, Ordering::SeqCst);
    let operation_id = BrainOperationId::new(if sequence == 0 {
        base_operation_id
    } else {
        format!("{base_operation_id}:request:{sequence}")
    });
    let now = now_iso();
    let operation = LogicalTurnOperationRecord {
        operation_id: operation_id.clone(),
        logical_turn_id: active.claim.record.logical_turn_id.clone(),
        continuation_id: active.claim.record.current_continuation_id.clone(),
        execution_epoch_id: epoch_id,
        kind: LogicalTurnOperationKind::ProviderRequest,
        phase: LogicalTurnOperationPhase::Leased,
        request_fingerprint: sha256_text(input_json),
        idempotency_key: operation_id.0.clone(),
        lease_holder: active.claim.record.claim_holder.clone(),
        lease_generation: active.claim.record.claim_generation,
        lease_expires_at: active.claim.record.claim_expires_at.clone(),
        result_ref: None,
        result_payload: None,
        reason_code: None,
        revision: 1,
        created_at: now.clone(),
        updated_at: now,
    };
    bridge
        .engine()?
        .lease_logical_turn_operation(&LogicalTurnOperationLeaseRequest {
            operation,
            expected_turn_revision: active.claim.record.revision,
            expected_claim_generation: required_claim_generation(&active)?,
            expected_cancellation_generation: active.claim.record.cancellation_generation,
        })?;
    *active.provider_operation_id.lock().map_err(|_| {
        CoreError::new(
            CoreErrorKind::InternalError,
            "provider operation map is poisoned",
        )
    })? = Some(operation_id);
    Ok(())
}

fn prepare_host_tool_operation(
    bridge: &NativeBridge,
    module_id: &str,
    wake_id: &str,
    request: &serde_json::Value,
) -> CoreResult<bool> {
    let Some(active) = bridge.active_logical_wakes.get(wake_id).cloned() else {
        return Ok(true);
    };
    let call_id = required_json_string(request, "call_id")?;
    let name = required_json_string(request, "name")?;
    let arguments_json = required_json_string(request, "arguments_json")?;
    let sequence = active
        .next_host_tool_operation
        .fetch_add(1, Ordering::SeqCst);
    let base_operation_id = format!(
        "operation:{}:{}:host-tool:{sequence}",
        active.claim.record.logical_turn_id.0, active.claim.record.current_continuation_id.0,
    );
    let request_fingerprint = sha256_text(
        &serde_json::json!({"name": name, "argumentsJson": arguments_json}).to_string(),
    );
    let existing = bridge
        .engine()?
        .list_logical_turn_operations(&active.claim.record.logical_turn_id)?
        .into_iter()
        .filter_map(|operation| {
            host_tool_operation_attempt(&base_operation_id, &operation.operation_id.0)
                .map(|attempt| (attempt, operation))
        })
        .max_by_key(|(attempt, _)| *attempt);
    let mut operation_id = BrainOperationId::new(base_operation_id.clone());
    if let Some((attempt, existing)) = existing {
        if existing.request_fingerprint != request_fingerprint {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!("logical turn replay diverged at host tool operation {sequence}"),
            ));
        }
        operation_id = existing.operation_id.clone();
        if existing.phase == LogicalTurnOperationPhase::Completed {
            active
                .host_tool_operations
                .lock()
                .map_err(|_| {
                    CoreError::new(
                        CoreErrorKind::InternalError,
                        "host tool operation map is poisoned",
                    )
                })?
                .insert(call_id.clone(), operation_id);
            let mut payload = existing.result_payload.ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "completed host tool operation has no durable result payload",
                )
            })?;
            let payload_object = payload.as_object_mut().ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "durable host tool result payload is not an object",
                )
            })?;
            payload_object.insert("wakeId".into(), serde_json::Value::String(wake_id.into()));
            payload_object.insert("callId".into(), serde_json::Value::String(call_id));
            let replay_json = payload.to_string();
            match module_id {
                CHAT_COMPLETIONS_MODULE_ID => submit_chat_completions_tool_output_json(
                    &bridge.chat_completions_buffered_runs,
                    replay_json,
                ),
                OPENAI_RESPONSES_MODULE_ID => submit_openai_responses_tool_output_json(
                    &bridge.openai_responses_buffered_runs,
                    replay_json,
                ),
                _ => {
                    return Err(CoreError::new(
                        CoreErrorKind::InvalidInput,
                        "unsupported brain module",
                    ))
                }
            }
            .map_err(|error| {
                CoreError::new(
                    CoreErrorKind::InternalError,
                    format!("replay durable host tool result: {error}"),
                )
            })?;
            return Ok(false);
        }
        if existing.phase == LogicalTurnOperationPhase::CompletedAfterCancel {
            return Ok(false);
        }
        if existing.phase == LogicalTurnOperationPhase::Superseded {
            operation_id = BrainOperationId::new(format!(
                "{base_operation_id}:retry:{}",
                attempt.saturating_add(1)
            ));
        } else {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "host tool operation already exists without a replayable completed result",
            ));
        }
    }

    let epoch_id = active.claim.record.active_epoch_id.clone().ok_or_else(|| {
        CoreError::new(
            CoreErrorKind::ActionRejected,
            "active logical wake has no execution epoch",
        )
    })?;
    let now = now_iso();
    let operation = LogicalTurnOperationRecord {
        operation_id: operation_id.clone(),
        logical_turn_id: active.claim.record.logical_turn_id.clone(),
        continuation_id: active.claim.record.current_continuation_id.clone(),
        execution_epoch_id: epoch_id,
        kind: LogicalTurnOperationKind::HostToolExecution,
        phase: LogicalTurnOperationPhase::Leased,
        request_fingerprint,
        idempotency_key: operation_id.0.clone(),
        lease_holder: active.claim.record.claim_holder.clone(),
        lease_generation: active.claim.record.claim_generation,
        lease_expires_at: active.claim.record.claim_expires_at.clone(),
        result_ref: None,
        result_payload: None,
        reason_code: None,
        revision: 1,
        created_at: now.clone(),
        updated_at: now,
    };
    bridge
        .engine()?
        .lease_logical_turn_operation(&LogicalTurnOperationLeaseRequest {
            operation,
            expected_turn_revision: active.claim.record.revision,
            expected_claim_generation: required_claim_generation(&active)?,
            expected_cancellation_generation: active.claim.record.cancellation_generation,
        })?;
    active
        .host_tool_operations
        .lock()
        .map_err(|_| {
            CoreError::new(
                CoreErrorKind::InternalError,
                "host tool operation map is poisoned",
            )
        })?
        .insert(call_id, operation_id);
    Ok(true)
}

fn host_tool_operation_attempt(base_operation_id: &str, operation_id: &str) -> Option<u64> {
    if operation_id == base_operation_id {
        return Some(0);
    }
    operation_id
        .strip_prefix(base_operation_id)?
        .strip_prefix(":retry:")?
        .parse()
        .ok()
}

fn complete_host_tool_operation(bridge: &NativeBridge, input_json: &str) -> CoreResult<bool> {
    let payload = serde_json::from_str::<serde_json::Value>(input_json).map_err(|error| {
        CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("invalid host tool result JSON: {error}"),
        )
    })?;
    let wake_id = required_json_string(&payload, "wakeId")?;
    let call_id = required_json_string(&payload, "callId")?;
    let Some(active) = bridge.active_logical_wakes.get(&wake_id).cloned() else {
        return Ok(true);
    };
    let Some(operation_id) = active
        .host_tool_operations
        .lock()
        .map_err(|_| {
            CoreError::new(
                CoreErrorKind::InternalError,
                "host tool operation map is poisoned",
            )
        })?
        .get(&call_id)
        .cloned()
    else {
        return Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            "host tool result has no durable operation lease",
        ));
    };
    let operation = bridge
        .engine()?
        .list_logical_turn_operations(&active.claim.record.logical_turn_id)?
        .into_iter()
        .find(|operation| operation.operation_id == operation_id)
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                "durable host tool operation not found",
            )
        })?;
    if matches!(
        operation.phase,
        LogicalTurnOperationPhase::Completed | LogicalTurnOperationPhase::CompletedAfterCancel
    ) {
        if operation.result_payload.as_ref() != Some(&payload) {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "host tool operation already completed with a different result",
            ));
        }
        return Ok(operation.phase == LogicalTurnOperationPhase::Completed);
    }
    if operation.phase != LogicalTurnOperationPhase::Leased {
        return Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            "host tool operation is not leased",
        ));
    }
    let mut completed = operation.clone();
    completed.phase = LogicalTurnOperationPhase::Completed;
    completed.result_ref = Some(format!("sha256:{}", sha256_text(input_json)));
    completed.result_payload = Some(payload);
    completed.reason_code = completed
        .result_payload
        .as_ref()
        .and_then(|value| value.get("reasonCode"))
        .and_then(|value| value.as_str())
        .map(str::to_string);
    completed.revision += 1;
    completed.updated_at = now_iso();
    let completed = bridge.engine()?.complete_logical_turn_operation(
        &LogicalTurnOperationCompletionRequest {
            operation: completed,
            expected_operation_revision: operation.revision,
            expected_turn_revision: active.claim.record.revision,
            expected_claim_generation: required_claim_generation(&active)?,
            expected_cancellation_generation: active.claim.record.cancellation_generation,
        },
    )?;
    Ok(completed.phase == LogicalTurnOperationPhase::Completed)
}

fn complete_provider_operation(
    bridge: &NativeBridge,
    wake_id: &str,
    payload: serde_json::Value,
) -> CoreResult<()> {
    let Some(active) = bridge.active_logical_wakes.get(wake_id).cloned() else {
        return Ok(());
    };
    let Some(operation_id) = active
        .provider_operation_id
        .lock()
        .map_err(|_| {
            CoreError::new(
                CoreErrorKind::InternalError,
                "provider operation map is poisoned",
            )
        })?
        .clone()
    else {
        return Ok(());
    };
    let operation = bridge
        .engine()?
        .list_logical_turn_operations(&active.claim.record.logical_turn_id)?
        .into_iter()
        .find(|operation| operation.operation_id == operation_id)
        .ok_or_else(|| CoreError::new(CoreErrorKind::NotFound, "provider operation not found"))?;
    if operation.phase != LogicalTurnOperationPhase::Leased {
        return Ok(());
    }
    let mut completed = operation.clone();
    completed.phase = LogicalTurnOperationPhase::Completed;
    completed.result_ref = Some(format!("sha256:{}", sha256_text(&payload.to_string())));
    completed.result_payload = Some(payload);
    completed.revision += 1;
    completed.updated_at = now_iso();
    bridge
        .engine()?
        .complete_logical_turn_operation(&LogicalTurnOperationCompletionRequest {
            operation: completed,
            expected_operation_revision: operation.revision,
            expected_turn_revision: active.claim.record.revision,
            expected_claim_generation: required_claim_generation(&active)?,
            expected_cancellation_generation: active.claim.record.cancellation_generation,
        })?;
    Ok(())
}

fn required_claim_generation(active: &ActiveLogicalWake) -> CoreResult<u64> {
    active.claim.record.claim_generation.ok_or_else(|| {
        CoreError::new(
            CoreErrorKind::ActionRejected,
            "active logical wake has no claim generation",
        )
    })
}

fn required_json_string(value: &serde_json::Value, name: &str) -> CoreResult<String> {
    value
        .get(name)
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("brain operation payload requires {name}"),
            )
        })
}

fn now_iso() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .expect("UTC timestamps format as RFC3339")
}

fn sha256_text(value: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn core_error_to_napi(error: CoreError) -> napi::Error {
    let status = if error.kind == CoreErrorKind::InvalidInput {
        napi::Status::InvalidArg
    } else {
        napi::Status::GenericFailure
    };
    napi::Error::new(status, error.to_string())
}

fn parse_tool_result_identity(input_json: &str) -> Option<(String, String, bool)> {
    let value = serde_json::from_str::<serde_json::Value>(input_json).ok()?;
    Some((
        value.get("wakeId")?.as_str()?.to_string(),
        value.get("callId")?.as_str()?.to_string(),
        value.get("status")?.as_str()? != "succeeded",
    ))
}

fn finish_native_activity(
    bridge: &NativeBridge,
    activity_id: RuntimeActivityId,
    status: RuntimeActivityStatus,
    reason_code: Option<&str>,
    summary: &str,
) {
    if let Err(error) = bridge.finish_runtime_activity(RuntimeActivityFinish {
        activity_id,
        status,
        phase: if status == RuntimeActivityStatus::Completed {
            "completed".into()
        } else {
            "failed".into()
        },
        reason_code: reason_code.map(str::to_string),
        summary: Some(summary.into()),
    }) {
        if error.kind != CoreErrorKind::NotFound {
            eprintln!("runtime activity finish failed: {error}");
        }
    }
}

fn finish_native_brain_activity_tree(
    bridge: &NativeBridge,
    wake_id: &str,
    status: RuntimeActivityStatus,
    reason_code: Option<&str>,
    summary: &str,
) {
    if let Ok(engine) = bridge.engine() {
        if let Err(error) =
            engine.finish_runtime_activity_tree(wake_id, status, reason_code, summary)
        {
            eprintln!("runtime activity tree finish failed: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brain_run_identity_preserves_model_configuration_and_endpoint() {
        let identity = parse_brain_run_identity(
            &serde_json::json!({
                "wakeId": "wake-model-identity",
                "sessionId": "session-model-identity",
                "config": {
                    "modelConfigId": "config-gpt-test",
                    "endpointId": "endpoint-openai",
                    "model": "gpt-test"
                }
            })
            .to_string(),
        )
        .expect("parse normalized brain config identity");

        assert_eq!(identity.model_config_id.as_deref(), Some("config-gpt-test"));
        assert_eq!(identity.endpoint_id.as_deref(), Some("endpoint-openai"));
        assert_eq!(identity.model.as_deref(), Some("gpt-test"));
    }

    #[test]
    fn host_tool_retry_operation_ids_are_ordered_and_do_not_shadow_the_base_receipt() {
        let base = "operation:turn:continuation:host-tool:0";
        assert_eq!(host_tool_operation_attempt(base, base), Some(0));
        assert_eq!(
            host_tool_operation_attempt(base, &format!("{base}:retry:1")),
            Some(1)
        );
        assert_eq!(
            host_tool_operation_attempt(base, &format!("{base}:retry:42")),
            Some(42)
        );
        assert_eq!(host_tool_operation_attempt(base, "operation:other"), None);
    }

    #[test]
    fn multiple_provider_requests_in_one_logical_wake_receive_distinct_durable_operations() {
        let data_dir = std::env::temp_dir().join(format!(
            "rusty-crew-provider-operation-ledger-{}",
            OffsetDateTime::now_utc().unix_timestamp_nanos()
        ));
        let mut bridge = NativeBridge::new();
        let brain = bridge
            .register_brain_implementation(BrainImplementationRegistration {
                implementation_id: rusty_crew_core_bridge_api::BrainImplementationId::new(
                    "provider-operation-brain",
                ),
                profile_id: ProfileId::new("provider-operation-profile"),
                tool_profile: rusty_crew_core_bridge_api::ToolProfile { tools: Vec::new() },
                model_config: rusty_crew_core_bridge_api::BrainModelConfig {
                    model_config_id: None,
                    model_config_revision: None,
                    endpoint_id: None,
                    endpoint_revision: None,
                    credential_id: None,
                    credential_revision: None,
                    credential_kind: None,
                    protocol: None,
                    dialect: None,
                    auth_scheme: None,
                    prompt_cache_transport: None,
                    provider: "fake".into(),
                    model_name: "fake".into(),
                    temperature_milli: None,
                    max_output_tokens: None,
                },
                strategy: Some(rusty_crew_core_bridge_api::BrainStrategyMetadata::unused(
                    CHAT_COMPLETIONS_MODULE_ID,
                    "default",
                )),
                provider_state_scope: None,
            })
            .expect("register brain");
        bridge
            .initialize_engine(EngineConfig {
                engine_data_dir: data_dir.to_string_lossy().into_owned(),
                clock: rusty_crew_core_bridge_api::ClockConfig::System,
                default_turn_budget: 3,
                default_idle_timeout_ms: 1_000,
                storage: None,
            })
            .expect("initialize engine");
        bridge
            .create_session(rusty_crew_core_bridge_api::SessionConfig {
                session_id: SessionId::new("provider-operation-session"),
                agent_id: rusty_crew_core_bridge_api::AgentId::new("provider-operation-agent"),
                profile_id: ProfileId::new("provider-operation-profile"),
                kind: rusty_crew_core_bridge_api::SessionKind::Full,
                delegation: None,
                workspace: None,
                resource_limits: rusty_crew_core_bridge_api::ResourceLimits {
                    max_duration_ms: None,
                    max_delegation_depth: None,
                },
                tool_profile: rusty_crew_core_bridge_api::ToolProfile { tools: Vec::new() },
                history_window: None,
            })
            .expect("create session");
        bridge
            .build_brain_wake_request_for_session(
                brain,
                SessionId::new("provider-operation-session"),
                "system".into(),
                br#"{"messages":[]}"#.to_vec(),
                "provider-operation-wake".into(),
            )
            .expect("prepare logical wake");

        lease_provider_operation(
            &bridge,
            "provider-operation-wake",
            r#"{"phase":"explore","messages":["find lore"]}"#,
        )
        .expect("lease explore provider request");
        complete_provider_operation(
            &bridge,
            "provider-operation-wake",
            serde_json::json!({"status": "completed", "phase": "explore"}),
        )
        .expect("complete explore provider request");
        lease_provider_operation(
            &bridge,
            "provider-operation-wake",
            r#"{"phase":"compose","messages":["write response"]}"#,
        )
        .expect("lease compose provider request");
        complete_provider_operation(
            &bridge,
            "provider-operation-wake",
            serde_json::json!({"status": "completed", "phase": "compose"}),
        )
        .expect("complete compose provider request");

        let active = bridge
            .active_logical_wakes
            .get("provider-operation-wake")
            .unwrap();
        let operations = bridge
            .engine()
            .unwrap()
            .list_logical_turn_operations(&active.claim.record.logical_turn_id)
            .unwrap();
        assert_eq!(operations.len(), 2);
        assert!(operations
            .iter()
            .all(|operation| operation.phase == LogicalTurnOperationPhase::Completed));
        assert_ne!(operations[0].operation_id, operations[1].operation_id);
        assert_ne!(
            operations[0].request_fingerprint,
            operations[1].request_fingerprint
        );
        assert!(operations
            .iter()
            .any(|operation| operation.operation_id.0.ends_with(":request:1")));
        std::fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn production_host_tool_dispatch_persists_lease_and_result_before_consumption() {
        let data_dir = std::env::temp_dir().join(format!(
            "rusty-crew-operation-ledger-{}",
            OffsetDateTime::now_utc().unix_timestamp_nanos()
        ));
        let mut bridge = NativeBridge::new();
        let brain = bridge
            .register_brain_implementation(BrainImplementationRegistration {
                implementation_id: rusty_crew_core_bridge_api::BrainImplementationId::new(
                    "operation-brain",
                ),
                profile_id: ProfileId::new("operation-profile"),
                tool_profile: rusty_crew_core_bridge_api::ToolProfile { tools: Vec::new() },
                model_config: rusty_crew_core_bridge_api::BrainModelConfig {
                    model_config_id: None,
                    model_config_revision: None,
                    endpoint_id: None,
                    endpoint_revision: None,
                    credential_id: None,
                    credential_revision: None,
                    credential_kind: None,
                    protocol: None,
                    dialect: None,
                    auth_scheme: None,
                    prompt_cache_transport: None,
                    provider: "fake".into(),
                    model_name: "fake".into(),
                    temperature_milli: None,
                    max_output_tokens: None,
                },
                strategy: Some(rusty_crew_core_bridge_api::BrainStrategyMetadata::unused(
                    CHAT_COMPLETIONS_MODULE_ID,
                    "default",
                )),
                provider_state_scope: None,
            })
            .expect("register brain");
        bridge
            .initialize_engine(EngineConfig {
                engine_data_dir: data_dir.to_string_lossy().into_owned(),
                clock: rusty_crew_core_bridge_api::ClockConfig::System,
                default_turn_budget: 3,
                default_idle_timeout_ms: 1_000,
                storage: None,
            })
            .expect("initialize engine");
        bridge
            .create_session(rusty_crew_core_bridge_api::SessionConfig {
                session_id: SessionId::new("operation-session"),
                agent_id: rusty_crew_core_bridge_api::AgentId::new("operation-agent"),
                profile_id: ProfileId::new("operation-profile"),
                kind: rusty_crew_core_bridge_api::SessionKind::Full,
                delegation: None,
                workspace: None,
                resource_limits: rusty_crew_core_bridge_api::ResourceLimits {
                    max_duration_ms: None,
                    max_delegation_depth: None,
                },
                tool_profile: rusty_crew_core_bridge_api::ToolProfile { tools: Vec::new() },
                history_window: None,
            })
            .expect("create session");
        bridge
            .build_brain_wake_request_for_session(
                brain,
                SessionId::new("operation-session"),
                "system".into(),
                br#"{"messages":[]}"#.to_vec(),
                "operation-wake".into(),
            )
            .expect("prepare logical wake");

        let request = serde_json::json!({
            "call_id": "call-1",
            "provider_item_id": null,
            "name": "read_file",
            "arguments_json": "{\"path\":\"README.md\"}"
        });
        let output = prepare_native_brain_drain(
            &bridge,
            CHAT_COMPLETIONS_MODULE_ID,
            "operation-wake",
            serde_json::json!({
                "tool_requests": [request],
                "terminal": false
            })
            .to_string(),
        )
        .expect("prepare host dispatch");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&output).unwrap()["tool_requests"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let active = bridge.active_logical_wakes.get("operation-wake").unwrap();
        let logical_turn_id = active.claim.record.logical_turn_id.clone();
        let leased = bridge
            .engine()
            .unwrap()
            .list_logical_turn_operations(&logical_turn_id)
            .unwrap();
        assert_eq!(leased.len(), 1);
        assert_eq!(leased[0].phase, LogicalTurnOperationPhase::Leased);

        assert!(complete_host_tool_operation(
            &bridge,
            &serde_json::json!({
                "wakeId": "operation-wake",
                "callId": "call-1",
                "output": "contents",
                "status": "succeeded",
                "retryable": false
            })
            .to_string()
        )
        .expect("persist host result"));
        let completed = bridge
            .engine()
            .unwrap()
            .list_logical_turn_operations(&logical_turn_id)
            .unwrap();
        assert_eq!(completed[0].phase, LogicalTurnOperationPhase::Completed);
        assert!(completed[0].result_payload.is_some());

        drop(bridge);
        let mut restarted = NativeBridge::new();
        let restarted_brain = restarted
            .register_brain_implementation(BrainImplementationRegistration {
                implementation_id: rusty_crew_core_bridge_api::BrainImplementationId::new(
                    "operation-brain",
                ),
                profile_id: ProfileId::new("operation-profile"),
                tool_profile: rusty_crew_core_bridge_api::ToolProfile { tools: Vec::new() },
                model_config: rusty_crew_core_bridge_api::BrainModelConfig {
                    model_config_id: None,
                    model_config_revision: None,
                    endpoint_id: None,
                    endpoint_revision: None,
                    credential_id: None,
                    credential_revision: None,
                    credential_kind: None,
                    protocol: None,
                    dialect: None,
                    auth_scheme: None,
                    prompt_cache_transport: None,
                    provider: "fake".into(),
                    model_name: "fake".into(),
                    temperature_milli: None,
                    max_output_tokens: None,
                },
                strategy: Some(rusty_crew_core_bridge_api::BrainStrategyMetadata::unused(
                    CHAT_COMPLETIONS_MODULE_ID,
                    "default",
                )),
                provider_state_scope: None,
            })
            .expect("register restarted brain");
        restarted
            .initialize_engine(EngineConfig {
                engine_data_dir: data_dir.to_string_lossy().into_owned(),
                clock: rusty_crew_core_bridge_api::ClockConfig::System,
                default_turn_budget: 3,
                default_idle_timeout_ms: 1_000,
                storage: None,
            })
            .expect("restart engine");
        restarted
            .build_brain_wake_request_for_session(
                restarted_brain,
                SessionId::new("operation-session"),
                "replacement system".into(),
                br#"{"messages":["replacement"]}"#.to_vec(),
                "operation-wake-restarted".into(),
            )
            .expect("resume logical wake");
        let mut coordinator = rusty_crew_brain_runtime::BufferedBrainTurnCoordinator::new(
            CHAT_COMPLETIONS_MODULE_ID,
            "operation-wake-restarted",
            SessionId::new("operation-session"),
            rusty_crew_brain_runtime::BufferedBrainTurnLimits::default(),
        )
        .expect("coordinator");
        coordinator.start().unwrap();
        coordinator
            .queue_tool_request(
                rusty_crew_brain_runtime::BufferedNeutralPendingToolRequest {
                    call_id: "call-replayed".into(),
                    provider_item_id: None,
                    name: "read_file".into(),
                    arguments_json: "{\"path\":\"README.md\"}".into(),
                },
            )
            .unwrap();
        restarted
            .chat_completions_buffered_runs
            .insert(rusty_crew_brain_runtime::BufferedBrainTurnRun::new(
                coordinator,
                crate::chat_completions::ChatCompletionsBufferedRunPayload::default(),
            ))
            .unwrap();
        let replayed = prepare_native_brain_drain(
            &restarted,
            CHAT_COMPLETIONS_MODULE_ID,
            "operation-wake-restarted",
            serde_json::json!({
                "tool_requests": [{
                    "call_id": "call-replayed",
                    "provider_item_id": null,
                    "name": "read_file",
                    "arguments_json": "{\"path\":\"README.md\"}"
                }],
                "terminal": false
            })
            .to_string(),
        )
        .expect("replay durable result");
        assert!(
            serde_json::from_str::<serde_json::Value>(&replayed).unwrap()["tool_requests"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        restarted
            .chat_completions_buffered_runs
            .with_run_mut("operation-wake-restarted", |run| {
                assert!(matches!(
                    run.coordinator
                        .poll_submitted_tool_output("call-replayed"),
                    rusty_crew_brain_runtime::BufferedNeutralToolOutputPoll::Ready(output)
                        if output.output == "contents"
                ));
            })
            .unwrap();
        std::fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn unsupported_modules_fail_closed() {
        let error = unsupported_brain_module("third-party-js");
        assert_eq!(error.status, napi::Status::InvalidArg);
        assert!(error
            .reason
            .contains("Rust brain catalog module third-party-js has no buffered run host"));
    }

    #[test]
    fn generic_results_carry_the_rust_selected_module_id() {
        let attached = attach_brain_module_id(
            CHAT_COMPLETIONS_MODULE_ID,
            serde_json::json!({"wake_id": "wake-1"}).to_string(),
        )
        .expect("attach module id");
        let value: serde_json::Value = serde_json::from_str(&attached).expect("valid JSON");
        assert_eq!(value["module_id"], CHAT_COMPLETIONS_MODULE_ID);
        assert_eq!(value["wake_id"], "wake-1");
    }
}
