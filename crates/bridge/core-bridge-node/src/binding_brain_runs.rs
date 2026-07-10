use super::*;

const PI_AGENT_MODULE_ID: &str = "pi-agent";
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
        let result = match module_id.as_str() {
            PI_AGENT_MODULE_ID => {
                start_pi_agent_brain_json(bridge.pi_agent_buffered_runs(), input_json)
            }
            OPENAI_RESPONSES_MODULE_ID => start_openai_responses_brain_json(
                bridge.openai_responses_buffered_runs(),
                input_json,
            ),
            _ => return Err(unsupported_brain_module(&module_id)),
        }?;
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
            PI_AGENT_MODULE_ID => drain_pi_agent_brain_stream_json(
                &bridge.pi_agent_buffered_runs(),
                wake_id,
                max_items,
            ),
            OPENAI_RESPONSES_MODULE_ID => drain_openai_responses_brain_stream_json(
                &bridge.openai_responses_buffered_runs(),
                wake_id,
                max_items,
            ),
            _ => return Err(unsupported_brain_module(&module_id)),
        }?;
        attach_brain_module_id(&module_id, result)
    }

    #[napi]
    pub fn submit_brain_host_result_json(
        &self,
        module_id: String,
        input_json: String,
    ) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let result = match module_id.as_str() {
            PI_AGENT_MODULE_ID => {
                submit_pi_agent_tool_output_json(&bridge.pi_agent_buffered_runs(), input_json)
            }
            OPENAI_RESPONSES_MODULE_ID => submit_openai_responses_tool_output_json(
                &bridge.openai_responses_buffered_runs(),
                input_json,
            ),
            _ => return Err(unsupported_brain_module(&module_id)),
        }?;
        attach_brain_module_id(&module_id, result)
    }

    #[napi]
    pub fn cancel_brain_run_json(
        &self,
        module_id: String,
        input_json: String,
    ) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let result = match module_id.as_str() {
            PI_AGENT_MODULE_ID => {
                cancel_pi_agent_brain_json(&bridge.pi_agent_buffered_runs(), input_json)
            }
            OPENAI_RESPONSES_MODULE_ID => cancel_openai_responses_brain_json(
                &bridge.openai_responses_buffered_runs(),
                input_json,
            ),
            _ => return Err(unsupported_brain_module(&module_id)),
        }?;
        attach_brain_module_id(&module_id, result)
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
            PI_AGENT_MODULE_ID,
            serde_json::json!({"wake_id": "wake-1"}).to_string(),
        )
        .expect("attach module id");
        let value: serde_json::Value = serde_json::from_str(&attached).expect("valid JSON");
        assert_eq!(value["module_id"], PI_AGENT_MODULE_ID);
        assert_eq!(value["wake_id"], "wake-1");
    }
}
