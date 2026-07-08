use super::*;

#[napi_derive::napi]
impl NativeBridgeBinding {
    #[napi]
    pub fn start_pi_agent_brain_json(&self, input_json: String) -> napi::Result<String> {
        let buffered_runs = self.bridge()?.pi_agent_buffered_runs();
        start_pi_agent_brain_json(buffered_runs, input_json)
    }

    #[napi]
    pub fn drain_pi_agent_brain_stream_json(
        &self,
        wake_id: String,
        max_items: Option<u32>,
    ) -> napi::Result<String> {
        let buffered_runs = self.bridge()?.pi_agent_buffered_runs();
        drain_pi_agent_brain_stream_json(&buffered_runs, wake_id, max_items)
    }

    #[napi]
    pub fn submit_pi_agent_tool_output_json(&self, input_json: String) -> napi::Result<String> {
        let buffered_runs = self.bridge()?.pi_agent_buffered_runs();
        submit_pi_agent_tool_output_json(&buffered_runs, input_json)
    }

    #[napi]
    pub fn cancel_pi_agent_brain_json(&self, input_json: String) -> napi::Result<String> {
        let buffered_runs = self.bridge()?.pi_agent_buffered_runs();
        cancel_pi_agent_brain_json(&buffered_runs, input_json)
    }
}
