use super::*;
use crate::buffered_tools::{
    BufferedNeutralPendingToolRequest, BufferedNeutralRun, BufferedNeutralRunRegistry,
    BufferedNeutralToolOutput,
};
use serde::Serialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsOpenAiResponsesBrainRunInput {
    wake_id: String,
    session_id: String,
    body_state: BodyState,
    #[serde(default)]
    tools: Vec<JsOpenAiResponsesNeutralTool>,
    #[serde(default)]
    provider_state: Option<BrainWakeProviderStateInput>,
    #[serde(default)]
    provider_state_absence: Option<String>,
    config: JsOpenAiResponsesBrainConfig,
    #[serde(default)]
    client: JsOpenAiResponsesClientConfig,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsOpenAiResponsesNeutralTool {
    name: String,
    description: String,
    input_schema: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsOpenAiResponsesBrainConfig {
    model: String,
    #[serde(default)]
    instructions: Option<String>,
    #[serde(default = "default_responses_stream_idle_timeout_ms")]
    stream_idle_timeout_ms: u64,
    #[serde(default = "default_responses_wake_timeout_ms")]
    wake_timeout_ms: u64,
}

fn default_responses_wake_timeout_ms() -> u64 {
    300_000
}

#[derive(Debug, Default, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
enum JsOpenAiResponsesClientConfig {
    #[default]
    Fake,
    Live {
        base_url: String,
        #[serde(default)]
        api_key: Option<String>,
        #[serde(default)]
        auth_kind: Option<String>,
        #[serde(default)]
        provider_alias: Option<String>,
        #[serde(default)]
        oauth_credential_secret: Option<String>,
    },
}

fn default_responses_stream_idle_timeout_ms() -> u64 {
    30_000
}

pub struct OpenAiResponsesBrainRunTask {
    pub(crate) input_json: String,
}

impl OpenAiResponsesBrainRunTask {
    pub(crate) fn new(input_json: String) -> Self {
        Self { input_json }
    }
}

pub struct OpenAiOauthCodeExchangeTask {
    pub(crate) input_json: String,
}

impl OpenAiOauthCodeExchangeTask {
    pub(crate) fn new(input_json: String) -> Self {
        Self { input_json }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsOpenAiOauthCodeExchangeInput {
    issuer: String,
    client_id: String,
    redirect_uri: String,
    code: String,
    code_verifier: String,
    #[serde(default)]
    now: Option<String>,
}

struct OpenAiResponsesBrainRunOutput {
    stream: Vec<BrainWakeStreamItem>,
    provider_state: Option<BrainWakeProviderStateOutput>,
    transport_metrics: ResponsesTransportMetrics,
    credential_secret_update: Option<OpenAiResponsesCredentialSecretUpdate>,
}

#[derive(Debug, Clone, Serialize)]
struct OpenAiResponsesCredentialSecretUpdate {
    provider_alias: String,
    secret: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsOpenAiResponsesToolOutputInput {
    wake_id: String,
    call_id: String,
    output: String,
    is_error: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsOpenAiResponsesCancelInput {
    wake_id: String,
    reason_code: String,
    summary: String,
}

type OpenAiResponsesBufferedRun =
    BufferedNeutralRun<ResponsesTransportMetrics, OpenAiResponsesCredentialSecretUpdate>;
type OpenAiResponsesBufferedRunRegistry =
    BufferedNeutralRunRegistry<ResponsesTransportMetrics, OpenAiResponsesCredentialSecretUpdate>;

struct OneShotOpenAiOauthSecretStore {
    provider_alias: String,
    secret: Option<String>,
    saved_secret: Option<String>,
}

impl OneShotOpenAiOauthSecretStore {
    fn new(provider_alias: String, secret: String) -> Self {
        Self {
            provider_alias,
            secret: Some(secret),
            saved_secret: None,
        }
    }

    fn credential_update(&self) -> Option<OpenAiResponsesCredentialSecretUpdate> {
        self.saved_secret
            .as_ref()
            .map(|secret| OpenAiResponsesCredentialSecretUpdate {
                provider_alias: self.provider_alias.clone(),
                secret: secret.clone(),
            })
    }
}

impl OpenAiOauthSecretStore for OneShotOpenAiOauthSecretStore {
    fn load_openai_oauth_secret(&mut self, provider_alias: &str) -> CoreResult<Option<String>> {
        if provider_alias != self.provider_alias {
            return Ok(None);
        }
        Ok(self.secret.clone())
    }

    fn save_openai_oauth_secret(
        &mut self,
        provider_alias: &str,
        secret_storage_text: String,
    ) -> CoreResult<()> {
        if provider_alias != self.provider_alias {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("OpenAI OAuth provider alias mismatch for {provider_alias}"),
            ));
        }
        self.secret = Some(secret_storage_text.clone());
        self.saved_secret = Some(secret_storage_text);
        Ok(())
    }
}

static OPENAI_RESPONSES_BUFFERED_RUNS: OnceLock<OpenAiResponsesBufferedRunRegistry> =
    OnceLock::new();

fn openai_responses_buffered_runs() -> &'static OpenAiResponsesBufferedRunRegistry {
    OPENAI_RESPONSES_BUFFERED_RUNS
        .get_or_init(|| BufferedNeutralRunRegistry::new("OpenAI Responses"))
}

impl napi::Task for OpenAiResponsesBrainRunTask {
    type Output = String;
    type JsValue = String;

    fn compute(&mut self) -> napi::Result<Self::Output> {
        run_openai_responses_brain_json_blocking(std::mem::take(&mut self.input_json))
    }

    fn resolve(&mut self, _env: napi::Env, output: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(output)
    }
}

impl napi::Task for OpenAiOauthCodeExchangeTask {
    type Output = String;
    type JsValue = String;

    fn compute(&mut self) -> napi::Result<Self::Output> {
        exchange_openai_oauth_code_json_blocking(std::mem::take(&mut self.input_json))
    }

    fn resolve(&mut self, _env: napi::Env, output: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(output)
    }
}

pub(crate) fn start_openai_responses_brain_json(input_json: String) -> napi::Result<String> {
    let input: JsOpenAiResponsesBrainRunInput =
        serde_json::from_str(&input_json).map_err(|error| {
            napi::Error::new(
                napi::Status::InvalidArg,
                format!("invalid OpenAI Responses brain input JSON: {error}"),
            )
        })?;
    let wake_id = input.wake_id;
    openai_responses_buffered_runs().insert(
        wake_id.clone(),
        OpenAiResponsesBufferedRun::new(input.config.wake_timeout_ms),
    )?;
    let thread_wake_id = wake_id.clone();
    std::thread::spawn(move || run_openai_responses_brain_buffered(thread_wake_id, input_json));
    serde_json::to_string(&json!({ "wake_id": wake_id })).map_err(|error| {
        napi::Error::new(
            napi::Status::GenericFailure,
            format!("serialize OpenAI Responses buffered wake start: {error}"),
        )
    })
}

pub(crate) fn drain_openai_responses_brain_stream_json(
    wake_id: String,
    max_items: Option<u32>,
) -> napi::Result<String> {
    let max_items = max_items.unwrap_or(64).max(1) as usize;
    let terminal = openai_responses_buffered_runs().with_run_mut(&wake_id, |run| {
        if !run.terminal && run.is_timed_out() {
            run.terminal = true;
            run.error = Some(format!(
                "OpenAI Responses buffered wake {wake_id} exceeded {}ms timeout",
                run.wake_timeout_ms
            ));
        }
        let mut items = Vec::new();
        for _ in 0..max_items {
            if run.terminal
                && !items.is_empty()
                && run
                    .items
                    .front()
                    .is_some_and(BrainWakeStreamItem::is_terminal)
            {
                break;
            }
            let Some(item) = run.items.pop_front() else {
                break;
            };
            let is_terminal = item.is_terminal();
            items.push(item);
            if is_terminal {
                break;
            }
        }
        let mut tool_requests = Vec::new();
        while let Some(request) = run.pending_tool_requests.pop_front() {
            tool_requests.push(request);
        }
        let terminal = run.terminal && run.items.is_empty();
        let output = json!({
            "wake_id": wake_id,
            "items": items,
            "tool_requests": tool_requests,
            "terminal": terminal,
            "provider_state": terminal.then(|| run.provider_state.clone()).flatten(),
            "transport_metrics": terminal.then(|| run.transport_metrics.clone()).flatten(),
            "credential_secret_update": terminal.then(|| run.credential_secret_update.clone()).flatten(),
            "error": terminal.then(|| run.error.clone()).flatten(),
            "cancellation": terminal.then(|| run.cancellation.clone()).flatten(),
        });
        (terminal, output)
    })?;
    let (terminal, output) = terminal;
    if terminal {
        openai_responses_buffered_runs().remove(&wake_id)?;
    }
    serde_json::to_string(&output).map_err(|error| {
        napi::Error::new(
            napi::Status::GenericFailure,
            format!("serialize OpenAI Responses buffered wake drain: {error}"),
        )
    })
}

pub(crate) fn cancel_openai_responses_brain_json(input_json: String) -> napi::Result<String> {
    let input: JsOpenAiResponsesCancelInput =
        serde_json::from_str(&input_json).map_err(|error| {
            napi::Error::new(
                napi::Status::InvalidArg,
                format!("invalid OpenAI Responses cancel JSON: {error}"),
            )
        })?;
    let output = openai_responses_buffered_runs().with_run_mut(&input.wake_id, |run| {
        run.cancel(input.reason_code, input.summary);
        json!({
        "ok": true,
        "wake_id": input.wake_id,
        "cancelled": true,
        "terminal": run.terminal,
        "cancellation": run.cancellation.clone(),
        })
    })?;
    serde_json::to_string(&output).map_err(|error| {
        napi::Error::new(
            napi::Status::GenericFailure,
            format!("serialize OpenAI Responses cancel receipt: {error}"),
        )
    })
}

pub(crate) fn submit_openai_responses_tool_output_json(input_json: String) -> napi::Result<String> {
    let input: JsOpenAiResponsesToolOutputInput =
        serde_json::from_str(&input_json).map_err(|error| {
            napi::Error::new(
                napi::Status::InvalidArg,
                format!("invalid OpenAI Responses tool output JSON: {error}"),
            )
        })?;
    openai_responses_buffered_runs().with_run_mut(&input.wake_id, |run| {
        run.submitted_tool_outputs.insert(
            input.call_id.clone(),
            BufferedNeutralToolOutput {
                output: input.output,
                is_error: input.is_error,
            },
        );
    })?;
    serde_json::to_string(&json!({
        "ok": true,
        "wake_id": input.wake_id,
        "call_id": input.call_id,
    }))
    .map_err(|error| {
        napi::Error::new(
            napi::Status::GenericFailure,
            format!("serialize OpenAI Responses tool output receipt: {error}"),
        )
    })
}

fn run_openai_responses_brain_buffered(wake_id: String, input_json: String) {
    let sink_wake_id = wake_id.clone();
    let mut sink = move |item: BrainWakeStreamItem| {
        let _ = openai_responses_buffered_runs().with_run_mut(&sink_wake_id, |run| {
            if run.is_cancelled() {
                return;
            }
            run.items.push_back(item);
        });
    };
    let result =
        run_openai_responses_brain_with_buffered_tools(wake_id.clone(), input_json, &mut sink);
    let _ = openai_responses_buffered_runs().with_run_mut(&wake_id, |run| {
        if run.is_cancelled() {
            return;
        }
        match result {
            Ok(output) => {
                run.provider_state = output.provider_state;
                run.transport_metrics = Some(output.transport_metrics);
                run.credential_secret_update = output.credential_secret_update;
            }
            Err(error) => {
                run.error = Some(error.to_string());
            }
        }
        run.terminal = true;
    });
}

struct EchoNeutralToolExecutor;

impl NeutralToolExecutor for EchoNeutralToolExecutor {
    fn execute(&self, call: &PendingResponsesFunctionCall) -> NeutralToolOutput {
        NeutralToolOutput {
            output: format!("{} completed by Rust Responses bridge", call.name),
            is_error: false,
        }
    }
}

struct BufferedOpenAiResponsesToolExecutor {
    wake_id: String,
}

impl NeutralToolExecutor for BufferedOpenAiResponsesToolExecutor {
    fn execute(&self, call: &PendingResponsesFunctionCall) -> NeutralToolOutput {
        let request = BufferedNeutralPendingToolRequest {
            call_id: call.call_id.clone(),
            provider_item_id: call.provider_item_id.clone(),
            name: call.name.clone(),
            arguments_json: call.arguments_json.clone(),
        };
        {
            let queued = openai_responses_buffered_runs().with_run_mut(&self.wake_id, |run| {
                run.pending_tool_requests.push_back(request);
            });
            if queued.is_err() {
                return NeutralToolOutput {
                    output: format!(
                        "OpenAI Responses buffered wake {} disappeared before tool request {}",
                        self.wake_id, call.call_id
                    ),
                    is_error: true,
                };
            }
        }

        loop {
            let result = openai_responses_buffered_runs().with_run_mut(&self.wake_id, |run| {
                if let Some(cancellation) = run.cancellation.clone() {
                    return NeutralToolOutput {
                        output: format!(
                            "OpenAI Responses buffered wake {} cancelled before tool output {}: {}",
                            self.wake_id, call.call_id, cancellation.summary
                        ),
                        is_error: true,
                    };
                }
                if let Some(output) = run.submitted_tool_outputs.remove(&call.call_id) {
                    return NeutralToolOutput {
                        output: output.output,
                        is_error: output.is_error,
                    };
                }
                if run.terminal {
                    return NeutralToolOutput {
                        output: format!(
                            "OpenAI Responses buffered wake {} ended before tool output {}",
                            self.wake_id, call.call_id
                        ),
                        is_error: true,
                    };
                }
                if run.is_timed_out() {
                    run.terminal = true;
                    run.error = Some(format!(
                        "OpenAI Responses buffered wake {} exceeded {}ms timeout while waiting for tool output {}",
                        self.wake_id, run.wake_timeout_ms, call.call_id
                    ));
                    return NeutralToolOutput {
                        output: run
                            .error
                            .clone()
                            .unwrap_or_else(|| "OpenAI Responses buffered wake timed out".to_string()),
                        is_error: true,
                    };
                }
                NeutralToolOutput {
                    output: String::new(),
                    is_error: false,
                }
            });
            match result {
                Ok(output) if !output.output.is_empty() || output.is_error => return output,
                Err(_) => {
                    return NeutralToolOutput {
                        output: format!(
                            "OpenAI Responses buffered wake {} disappeared before tool output {}",
                            self.wake_id, call.call_id
                        ),
                        is_error: true,
                    };
                }
                Ok(_) => {}
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    }
}

pub(crate) fn run_openai_responses_brain_json_blocking(input_json: String) -> napi::Result<String> {
    let output = run_openai_responses_brain(input_json)?;
    let output = json!({
        "stream": output.stream,
        "provider_state": output.provider_state,
        "transport_metrics": output.transport_metrics,
        "credential_secret_update": output.credential_secret_update,
    });
    serde_json::to_string(&output)
        .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
}

fn exchange_openai_oauth_code_json_blocking(input_json: String) -> napi::Result<String> {
    let input: JsOpenAiOauthCodeExchangeInput =
        serde_json::from_str(&input_json).map_err(|error| {
            napi::Error::new(
                napi::Status::InvalidArg,
                format!("invalid OpenAI OAuth code exchange input JSON: {error}"),
            )
        })?;
    let now = match input.now.as_deref() {
        Some(value) => OffsetDateTime::parse(value, &Rfc3339).map_err(|error| {
            napi::Error::new(
                napi::Status::InvalidArg,
                format!("invalid OpenAI OAuth code exchange now timestamp: {error}"),
            )
        })?,
        None => OffsetDateTime::now_utc(),
    };
    let client = OpenAiOauthClient::new()
        .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))?;
    let result = client.exchange_authorization_code(&OpenAiOauthCodeExchangeRequest {
        issuer: input.issuer.clone(),
        client_id: input.client_id.clone(),
        redirect_uri: input.redirect_uri,
        code: input.code,
        code_verifier: input.code_verifier,
    });
    let output = match result {
        Ok(result) => {
            let envelope = openai_oauth_envelope_from_exchange_result(
                result,
                input.issuer,
                input.client_id,
                now,
            );
            let secret = envelope
                .to_storage_text()
                .map_err(|error| napi::Error::new(napi::Status::InvalidArg, error.to_string()))?;
            json!({
                "ok": true,
                "secret": secret,
                "summary": envelope.redacted_summary(),
            })
        }
        Err(error) => json!({
            "ok": false,
            "error": openai_oauth_exchange_error_json(error),
        }),
    };
    serde_json::to_string(&output)
        .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
}

fn openai_oauth_exchange_error_json(error: OpenAiOauthError) -> serde_json::Value {
    match error {
        OpenAiOauthError::Status {
            status,
            reason_code,
            message,
        } => json!({
            "code": "upstream_status",
            "reasonCode": reason_code.unwrap_or_else(|| "openai_oauth_upstream_status".to_string()),
            "status": status,
            "message": format!("OpenAI OAuth endpoint returned status {status}: {message}"),
            "retryable": status >= 500,
        }),
        OpenAiOauthError::Transport => json!({
            "code": "transport",
            "reasonCode": "openai_oauth_transport",
            "message": "OpenAI OAuth request transport failed",
            "retryable": true,
        }),
        OpenAiOauthError::MalformedResponse(message) => json!({
            "code": "malformed_response",
            "reasonCode": "openai_oauth_malformed_response",
            "message": format!("OpenAI OAuth endpoint returned malformed JSON: {message}"),
            "retryable": false,
        }),
        other => json!({
            "code": "credential_error",
            "reasonCode": "openai_oauth_credential_error",
            "message": other.to_string(),
            "retryable": false,
        }),
    }
}

fn run_openai_responses_brain(input_json: String) -> napi::Result<OpenAiResponsesBrainRunOutput> {
    run_openai_responses_brain_internal(input_json, None, EchoNeutralToolExecutor)
}

fn run_openai_responses_brain_with_buffered_tools(
    wake_id: String,
    input_json: String,
    sink: &mut dyn FnMut(BrainWakeStreamItem),
) -> napi::Result<OpenAiResponsesBrainRunOutput> {
    run_openai_responses_brain_internal(
        input_json,
        Some(sink),
        BufferedOpenAiResponsesToolExecutor { wake_id },
    )
}

pub(crate) fn normalize_responses_tool_schema(schema: &Value) -> Value {
    match schema {
        Value::Object(object) if object.get("type").is_some() => schema.clone(),
        Value::Object(_) => {
            let mut normalized = schema.clone();
            if let Value::Object(object) = &mut normalized {
                object.insert("type".to_string(), json!("object"));
            }
            normalized
        }
        _ => json!({"type": "object", "properties": {}}),
    }
}

fn run_openai_responses_brain_internal<T>(
    input_json: String,
    mut sink: Option<&mut dyn FnMut(BrainWakeStreamItem)>,
    tool_executor: T,
) -> napi::Result<OpenAiResponsesBrainRunOutput>
where
    T: NeutralToolExecutor,
{
    let input: JsOpenAiResponsesBrainRunInput =
        serde_json::from_str(&input_json).map_err(|error| {
            napi::Error::new(
                napi::Status::InvalidArg,
                format!("invalid OpenAI Responses brain input JSON: {error}"),
            )
        })?;
    let mut config = ResponsesBrainConfig::replay(input.config.model);
    config.instructions = input.config.instructions;
    config.stream_idle_timeout_ms = input.config.stream_idle_timeout_ms;
    let descriptors = if input.tools.is_empty() {
        input
            .body_state
            .session
            .tool_profile
            .tools
            .iter()
            .map(|tool| NeutralBrainTool {
                name: tool.name.clone(),
                description: tool.description.clone(),
                input_schema: json!({"type": "object", "properties": {}}),
            })
            .collect::<Vec<_>>()
    } else {
        input
            .tools
            .iter()
            .map(|tool| NeutralBrainTool {
                name: tool.name.clone(),
                description: tool.description.clone(),
                input_schema: normalize_responses_tool_schema(&tool.input_schema),
            })
            .collect::<Vec<_>>()
    };
    let history = rusty_crew_openai_responses_brain::ResponsesReplayProjection::from_body_state(
        &input.body_state,
    );
    let request = BrainWakeRequest {
        brain: BrainImplementationHandle::new(0),
        session_id: SessionId::new(input.session_id),
        body_state: RuntimeBufferHandle::new(0),
        system_prompt: RuntimeBufferHandle::new(0),
        role_assembly: RuntimeBufferHandle::new(0),
        wake_id: input.wake_id,
        provider_state: input.provider_state,
        provider_state_absence: input
            .provider_state_absence
            .as_deref()
            .map(parse_provider_state_absence_reason)
            .transpose()
            .map_err(to_napi_error)?,
    };
    let mut credential_secret_update = None;
    let result = match input.client {
        JsOpenAiResponsesClientConfig::Fake => {
            let client = fake_responses_client_for_body(&input.body_state);
            let mut brain = ResponsesReplayBrain::new(client, tool_executor, config, descriptors);
            if let Some(sink) = &mut sink {
                brain.wake_with_history_and_stream_sink(request, history, *sink)
            } else {
                brain.wake_with_history(request, history)
            }
        }
        JsOpenAiResponsesClientConfig::Live {
            base_url,
            api_key,
            auth_kind,
            provider_alias,
            oauth_credential_secret,
        } => {
            let (bearer_token, account_id, is_fedramp_account) =
                if auth_kind.as_deref() == Some("openai_oauth") {
                    let provider_alias = provider_alias.ok_or_else(|| {
                        napi::Error::new(
                            napi::Status::InvalidArg,
                            "openai_oauth Responses client requires provider_alias",
                        )
                    })?;
                    let oauth_credential_secret = oauth_credential_secret.ok_or_else(|| {
                        napi::Error::new(
                            napi::Status::InvalidArg,
                            "openai_oauth Responses client requires oauth_credential_secret",
                        )
                    })?;
                    let mut secret_store = OneShotOpenAiOauthSecretStore::new(
                        provider_alias.clone(),
                        oauth_credential_secret,
                    );
                    let resolution = resolve_openai_oauth_bearer(
                        &provider_alias,
                        &mut secret_store,
                        &OpenAiOauthClient::new().map_err(|error| {
                            napi::Error::new(napi::Status::GenericFailure, error.to_string())
                        })?,
                        OffsetDateTime::now_utc(),
                        &OpenAiOauthRefreshPolicy::default(),
                    )
                    .map_err(|error| {
                        napi::Error::new(napi::Status::GenericFailure, error.to_string())
                    })?;
                    credential_secret_update = secret_store.credential_update();
                    (
                        Some(resolution.bearer_token),
                        resolution.account_id,
                        resolution.is_fedramp_account,
                    )
                } else {
                    (api_key, None, false)
                };
            let client = LiveResponsesClient::new_with_bearer_metadata(
                base_url,
                bearer_token,
                account_id,
                is_fedramp_account,
                config.stream_idle_timeout_ms,
            )
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))?;
            let mut brain = ResponsesReplayBrain::new(client, tool_executor, config, descriptors);
            if let Some(sink) = &mut sink {
                brain.wake_with_history_and_stream_sink(request, history, *sink)
            } else {
                brain.wake_with_history(request, history)
            }
        }
    }
    .map_err(to_napi_error)?;
    Ok(OpenAiResponsesBrainRunOutput {
        stream: result
            .stream
            .drain_until_terminal()
            .map_err(to_napi_error)?,
        provider_state: result.provider_state,
        transport_metrics: result.transport_metrics,
        credential_secret_update,
    })
}

fn fake_responses_client_for_body(body: &BodyState) -> FakeResponsesClient {
    if let Ok(raw_delay_ms) = std::env::var("RUSTY_CREW_OPENAI_RESPONSES_FAKE_DELAY_MS") {
        if let Ok(delay_ms) = raw_delay_ms.parse::<u64>() {
            if delay_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
            }
        }
    }

    let Some(tool) = body.session.tool_profile.tools.first() else {
        return FakeResponsesClient::new(vec![Ok(vec![
            ResponsesEvent::TextDelta("responses module scaffold wake completed".to_string()),
            ResponsesEvent::Completed {
                response_id: "fake-response".to_string(),
                usage: Some(fake_responses_usage(false)),
            },
        ])]);
    };
    FakeResponsesClient::new(vec![
        Ok(vec![
            ResponsesEvent::OutputItemDone(ResponsesOutputItem::FunctionCall {
                id: Some("fake-call-item".to_string()),
                call_id: "fake-call".to_string(),
                name: tool.name.clone(),
                arguments: "{}".to_string(),
            }),
            ResponsesEvent::Completed {
                response_id: "fake-response-tool-call".to_string(),
                usage: Some(fake_responses_usage(false)),
            },
        ]),
        Ok(vec![
            ResponsesEvent::TextDelta("responses module scaffold wake completed".to_string()),
            ResponsesEvent::Completed {
                response_id: "fake-response-final".to_string(),
                usage: Some(fake_responses_usage(true)),
            },
        ]),
    ])
    .expect_function_output("fake-call")
    .expect_function_output("fake-call")
}

fn fake_responses_usage(cached: bool) -> ResponsesTokenUsage {
    ResponsesTokenUsage {
        input_tokens: 1,
        cached_input_tokens: u64::from(cached),
        output_tokens: 1,
        reasoning_output_tokens: 0,
        total_tokens: 2,
    }
}
