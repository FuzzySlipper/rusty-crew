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
        observe_native_brain_drain(&bridge, &module_id, &wake_id, &result);
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

fn observe_native_brain_drain(
    bridge: &NativeBridge,
    module_id: &str,
    wake_id: &str,
    output_json: &str,
) {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(output_json) else {
        return;
    };
    if let Some(tool_requests) = value
        .get("tool_requests")
        .and_then(|value| value.as_array())
    {
        let session = bridge
            .buffered_brain_run_diagnostics()
            .ok()
            .and_then(|diagnostics| {
                diagnostics
                    .runs
                    .into_iter()
                    .find(|run| run.wake_id == wake_id)
            });
        for request in tool_requests {
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
        finish_native_brain_activity_tree(bridge, wake_id, status, reason_code, summary);
    }
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
