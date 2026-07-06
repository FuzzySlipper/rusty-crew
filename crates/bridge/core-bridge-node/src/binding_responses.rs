use super::*;

#[napi_derive::napi]
impl NativeBridgeBinding {
    #[napi]
    pub fn run_openai_responses_brain_json(
        &self,
        input_json: String,
    ) -> napi::bindgen_prelude::AsyncTask<OpenAiResponsesBrainRunTask> {
        // The responses brain still uses blocking provider I/O internally.
        // Running it as a napi task keeps the Node event loop available for
        // admin APIs, adapters, and SSE while this worker-thread task drains.
        napi::bindgen_prelude::AsyncTask::new(OpenAiResponsesBrainRunTask::new(input_json))
    }

    #[napi]
    pub fn exchange_openai_oauth_code_json(
        &self,
        input_json: String,
    ) -> napi::bindgen_prelude::AsyncTask<OpenAiOauthCodeExchangeTask> {
        // OAuth code exchange performs blocking provider I/O. Keep it off the
        // Node event loop just like the live Responses wake path.
        napi::bindgen_prelude::AsyncTask::new(OpenAiOauthCodeExchangeTask::new(input_json))
    }

    #[napi]
    pub fn start_openai_responses_brain_json(&self, input_json: String) -> napi::Result<String> {
        start_openai_responses_brain_json(input_json)
    }

    #[napi]
    pub fn drain_openai_responses_brain_stream_json(
        &self,
        wake_id: String,
        max_items: Option<u32>,
    ) -> napi::Result<String> {
        drain_openai_responses_brain_stream_json(wake_id, max_items)
    }

    #[napi]
    pub fn submit_openai_responses_tool_output_json(
        &self,
        input_json: String,
    ) -> napi::Result<String> {
        submit_openai_responses_tool_output_json(input_json)
    }
}
