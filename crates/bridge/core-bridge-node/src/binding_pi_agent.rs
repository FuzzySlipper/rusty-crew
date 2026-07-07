use super::*;

#[napi_derive::napi]
impl NativeBridgeBinding {
    #[napi]
    pub fn start_pi_agent_brain_json(&self, input_json: String) -> napi::Result<String> {
        start_pi_agent_brain_json(input_json)
    }

    #[napi]
    pub fn drain_pi_agent_brain_stream_json(
        &self,
        wake_id: String,
        max_items: Option<u32>,
    ) -> napi::Result<String> {
        drain_pi_agent_brain_stream_json(wake_id, max_items)
    }

    #[napi]
    pub fn submit_pi_agent_tool_output_json(&self, input_json: String) -> napi::Result<String> {
        submit_pi_agent_tool_output_json(input_json)
    }

    #[napi]
    pub fn cancel_pi_agent_brain_json(&self, input_json: String) -> napi::Result<String> {
        cancel_pi_agent_brain_json(input_json)
    }
}
