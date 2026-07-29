use super::*;
use rusty_crew_brain_runtime::{
    BrainRuntimeError, BufferedBrainHostToolResult, BufferedBrainHostToolStatus,
    BufferedBrainTurnCoordinator, BufferedBrainTurnError, BufferedBrainTurnLimits,
    BufferedBrainTurnPhase, BufferedBrainTurnRegistry, BufferedBrainTurnRun,
    BufferedNeutralPendingToolRequest, BufferedNeutralToolOutputPoll,
};
use serde::Serialize;
use std::sync::Arc;

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
    #[serde(default)]
    reasoning_effort: Option<String>,
    #[serde(default)]
    max_output_tokens: Option<u32>,
    #[serde(default)]
    provider_request_timeout_ms: Option<u64>,
    #[serde(default)]
    max_continuation_rounds: Option<usize>,
    #[serde(default)]
    wake_timeout_ms: Option<u64>,
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
    #[cfg_attr(not(test), allow(dead_code))]
    stream: Vec<BrainWakeStreamItem>,
    provider_state: Option<BrainWakeProviderStateOutput>,
    transport_metrics: ResponsesTransportMetrics,
    credential_secret_update: Option<OpenAiResponsesCredentialSecretUpdate>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct OpenAiResponsesCredentialSecretUpdate {
    provider_alias: String,
    secret: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsOpenAiResponsesToolOutputInput {
    wake_id: String,
    call_id: String,
    output: String,
    status: BufferedBrainHostToolStatus,
    #[serde(default)]
    reason_code: Option<String>,
    retryable: bool,
    #[serde(default)]
    action: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    debug_detail_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsOpenAiResponsesCancelInput {
    wake_id: String,
    reason_code: String,
    summary: String,
}

#[derive(Debug, Default)]
pub(crate) struct OpenAiResponsesBufferedRunPayload {
    transport_metrics: Option<ResponsesTransportMetrics>,
    credential_secret_update: Option<OpenAiResponsesCredentialSecretUpdate>,
    provider_finished: bool,
    provider_cancellation: ResponsesProviderCancellation,
}

pub(crate) type OpenAiResponsesBufferedRunRegistry =
    BufferedBrainTurnRegistry<OpenAiResponsesBufferedRunPayload>;

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

fn brain_runtime_error_to_napi(error: BrainRuntimeError) -> napi::Error {
    let status = if error.is_invalid_argument() {
        napi::Status::InvalidArg
    } else {
        napi::Status::GenericFailure
    };
    napi::Error::new(status, error.to_string())
}

fn brain_turn_error_to_napi(error: BufferedBrainTurnError) -> napi::Error {
    napi::Error::new(napi::Status::InvalidArg, error.to_string())
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

pub(crate) fn start_openai_responses_brain_json(
    buffered_runs: Arc<OpenAiResponsesBufferedRunRegistry>,
    input_json: String,
) -> napi::Result<String> {
    let input: JsOpenAiResponsesBrainRunInput =
        serde_json::from_str(&input_json).map_err(|error| {
            napi::Error::new(
                napi::Status::InvalidArg,
                format!("invalid OpenAI Responses brain input JSON: {error}"),
            )
        })?;
    let wake_id = input.wake_id.clone();
    let mut coordinator = BufferedBrainTurnCoordinator::new(
        "openai-responses",
        wake_id.clone(),
        SessionId::new(input.session_id),
        input.config.wake_timeout_ms,
        BufferedBrainTurnLimits::default(),
    )
    .map_err(brain_turn_error_to_napi)?;
    coordinator.start().map_err(brain_turn_error_to_napi)?;
    buffered_runs
        .insert(BufferedBrainTurnRun::new(
            coordinator,
            OpenAiResponsesBufferedRunPayload::default(),
        ))
        .map_err(brain_runtime_error_to_napi)?;
    let thread_wake_id = wake_id.clone();
    std::thread::spawn(move || {
        run_openai_responses_brain_buffered(buffered_runs, thread_wake_id, input_json)
    });
    serde_json::to_string(&json!({ "wake_id": wake_id })).map_err(|error| {
        napi::Error::new(
            napi::Status::GenericFailure,
            format!("serialize OpenAI Responses buffered wake start: {error}"),
        )
    })
}

pub(crate) fn drain_openai_responses_brain_stream_json(
    buffered_runs: &OpenAiResponsesBufferedRunRegistry,
    wake_id: String,
    max_items: Option<u32>,
) -> napi::Result<String> {
    let max_items = max_items.unwrap_or(64).max(1) as usize;
    let terminal = buffered_runs.with_run_mut(&wake_id, |run| {
        if run.coordinator.timeout_if_due() {
            run.payload.provider_cancellation.cancel();
        }
        let drain = run.coordinator.drain_stream(max_items);
        let stream_retention_metrics = run.coordinator.stream_retention_metrics();
        let tool_requests = run.coordinator.drain_host_tool_requests(128);
        let terminal = drain.terminal && run.payload.provider_finished;
        let terminal_reason_code = terminal
            .then(|| run.coordinator.terminal())
            .flatten()
            .map(|terminal| terminal.reason_code.clone());
        let error = terminal
            .then(|| run.coordinator.terminal())
            .flatten()
            .filter(|_| run.coordinator.has_error())
            .map(|terminal| terminal.summary.clone());
        let output = json!({
            "wake_id": wake_id,
            "items": drain.items.into_iter().map(|item| item.item).collect::<Vec<_>>(),
            "tool_requests": tool_requests,
            "stream_retention_metrics": stream_retention_metrics,
            "terminal": terminal,
            "terminal_reason_code": terminal_reason_code,
            "provider_state": terminal.then(|| run.coordinator.provider_state_output().cloned()).flatten(),
            "transport_metrics": terminal.then(|| run.payload.transport_metrics.clone()).flatten(),
            "credential_secret_update": terminal.then(|| run.payload.credential_secret_update.clone()).flatten(),
            "error": error,
            "cancellation": terminal.then(|| run.coordinator.cancellation()).flatten(),
        });
        (terminal, output)
    })
    .map_err(brain_runtime_error_to_napi)?;
    let (terminal, output) = terminal;
    if terminal {
        buffered_runs
            .remove(&wake_id)
            .map_err(brain_runtime_error_to_napi)?;
    }
    serde_json::to_string(&output).map_err(|error| {
        napi::Error::new(
            napi::Status::GenericFailure,
            format!("serialize OpenAI Responses buffered wake drain: {error}"),
        )
    })
}

pub(crate) fn cancel_openai_responses_brain_json(
    buffered_runs: &OpenAiResponsesBufferedRunRegistry,
    input_json: String,
) -> napi::Result<String> {
    let input: JsOpenAiResponsesCancelInput =
        serde_json::from_str(&input_json).map_err(|error| {
            napi::Error::new(
                napi::Status::InvalidArg,
                format!("invalid OpenAI Responses cancel JSON: {error}"),
            )
        })?;
    let output = buffered_runs
        .with_run_mut(&input.wake_id, |run| {
            run.payload.provider_cancellation.cancel();
            run.coordinator
                .cancel(input.reason_code, input.summary)
                .map(|()| {
                    json!({
                        "ok": true,
                        "wake_id": input.wake_id,
                        "cancelled": true,
                        "terminal": run.coordinator.phase().is_terminal(),
                        "cancellation": run.coordinator.cancellation(),
                    })
                })
        })
        .map_err(brain_runtime_error_to_napi)?
        .map_err(brain_turn_error_to_napi)?;
    serde_json::to_string(&output).map_err(|error| {
        napi::Error::new(
            napi::Status::GenericFailure,
            format!("serialize OpenAI Responses cancel receipt: {error}"),
        )
    })
}

pub(crate) fn submit_openai_responses_tool_output_json(
    buffered_runs: &OpenAiResponsesBufferedRunRegistry,
    input_json: String,
) -> napi::Result<String> {
    let input: JsOpenAiResponsesToolOutputInput =
        serde_json::from_str(&input_json).map_err(|error| {
            napi::Error::new(
                napi::Status::InvalidArg,
                format!("invalid OpenAI Responses tool output JSON: {error}"),
            )
        })?;
    let submission = buffered_runs
        .with_run_mut(&input.wake_id, |run| {
            run.coordinator.submit_host_tool_result(
                &input.call_id,
                BufferedBrainHostToolResult {
                    status: input.status,
                    output_text: input.output,
                    reason_code: input.reason_code,
                    retryable: input.retryable,
                    action: input.action,
                    summary: input.summary,
                    debug_detail_id: input.debug_detail_id,
                },
            )
        })
        .map_err(brain_runtime_error_to_napi)?
        .map_err(brain_turn_error_to_napi)?;
    serde_json::to_string(&json!({
        "ok": true,
        "wake_id": input.wake_id,
        "call_id": input.call_id,
        "receipt": submission.receipt,
        "decision": submission.decision,
    }))
    .map_err(|error| {
        napi::Error::new(
            napi::Status::GenericFailure,
            format!("serialize OpenAI Responses tool output receipt: {error}"),
        )
    })
}

fn run_openai_responses_brain_buffered(
    buffered_runs: Arc<OpenAiResponsesBufferedRunRegistry>,
    wake_id: String,
    input_json: String,
) {
    let sink_wake_id = wake_id.clone();
    let sink_buffered_runs = Arc::clone(&buffered_runs);
    let mut sink = move |item: BrainWakeStreamItem| {
        let _ = sink_buffered_runs.with_run_mut(&sink_wake_id, |run| {
            if run.coordinator.phase().is_terminal() {
                return;
            }
            let _ = run.coordinator.enqueue_provider_stream_item(item);
        });
    };
    let result = run_openai_responses_brain_with_buffered_tools(
        Arc::clone(&buffered_runs),
        wake_id.clone(),
        input_json,
        &mut sink,
    );
    let _ = buffered_runs.with_run_mut(&wake_id, |run| {
        if !run.coordinator.is_cancelled() {
            match result {
                Ok(output) => {
                    if run.coordinator.phase() == BufferedBrainTurnPhase::Completed {
                        if let Some(provider_state) = output.provider_state {
                            let _ = run.coordinator.set_provider_state_output(provider_state);
                        }
                    }
                    run.payload.transport_metrics = Some(output.transport_metrics);
                    run.payload.credential_secret_update = output.credential_secret_update;
                    if !run.coordinator.phase().is_terminal() {
                        let _ = run.coordinator.fail(
                            "provider_stream_missing_terminal",
                            "OpenAI Responses provider loop ended without a terminal stream item",
                        );
                    }
                }
                Err(error) => {
                    if !run.coordinator.phase().is_terminal() {
                        let _ = run.coordinator.fail("provider_error", error.to_string());
                    }
                }
            }
        }
        run.payload.provider_finished = true;
    });
}

#[cfg(test)]
struct EchoNeutralToolExecutor;

#[cfg(test)]
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
    buffered_runs: Arc<OpenAiResponsesBufferedRunRegistry>,
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
            let queued = self.buffered_runs.with_run_mut(&self.wake_id, |run| {
                run.coordinator.queue_tool_request(request)
            });
            if !matches!(queued, Ok(Ok(()))) {
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
            let result = self.buffered_runs.with_run_mut(&self.wake_id, |run| {
                if let Some(cancellation) = run.coordinator.cancellation() {
                    return Some(NeutralToolOutput {
                        output: format!(
                            "OpenAI Responses buffered wake {} cancelled before tool output {}: {}",
                            self.wake_id, call.call_id, cancellation.summary
                        ),
                        is_error: true,
                    });
                }
                if let BufferedNeutralToolOutputPoll::Ready(output) =
                    run.coordinator.poll_submitted_tool_output(&call.call_id)
                {
                    return Some(NeutralToolOutput {
                        output: output.output,
                        is_error: output.is_error,
                    });
                }
                if run.coordinator.phase().is_terminal() {
                    let summary = run
                        .coordinator
                        .terminal()
                        .map(|terminal| terminal.summary.as_str())
                        .unwrap_or("turn ended");
                    return Some(NeutralToolOutput {
                        output: format!(
                            "OpenAI Responses buffered wake {} ended before tool output {}: {}",
                            self.wake_id, call.call_id, summary
                        ),
                        is_error: true,
                    });
                }
                if run.coordinator.timeout_if_due() {
                    run.payload.provider_cancellation.cancel();
                    return Some(NeutralToolOutput {
                        output: run
                            .coordinator
                            .terminal()
                            .map(|terminal| terminal.summary.clone())
                            .unwrap_or_else(|| {
                                "OpenAI Responses buffered wake timed out".to_string()
                            }),
                        is_error: true,
                    });
                }
                None
            });
            match result {
                Ok(Some(output)) => return output,
                Err(_) => {
                    return NeutralToolOutput {
                        output: format!(
                            "OpenAI Responses buffered wake {} disappeared before tool output {}",
                            self.wake_id, call.call_id
                        ),
                        is_error: true,
                    };
                }
                Ok(None) => {}
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    }
}

#[cfg(test)]
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

#[cfg(test)]
fn run_openai_responses_brain(input_json: String) -> napi::Result<OpenAiResponsesBrainRunOutput> {
    run_openai_responses_brain_internal(
        input_json,
        None,
        EchoNeutralToolExecutor,
        ResponsesProviderCancellation::default(),
    )
}

fn run_openai_responses_brain_with_buffered_tools(
    buffered_runs: Arc<OpenAiResponsesBufferedRunRegistry>,
    wake_id: String,
    input_json: String,
    sink: &mut dyn FnMut(BrainWakeStreamItem),
) -> napi::Result<OpenAiResponsesBrainRunOutput> {
    let provider_cancellation = buffered_runs
        .with_run_mut(&wake_id, |run| run.payload.provider_cancellation.clone())
        .map_err(brain_runtime_error_to_napi)?;
    run_openai_responses_brain_internal(
        input_json,
        Some(sink),
        BufferedOpenAiResponsesToolExecutor {
            wake_id,
            buffered_runs,
        },
        provider_cancellation,
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
    provider_cancellation: ResponsesProviderCancellation,
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
    config.reasoning = input.config.reasoning_effort.map(|effort| {
        rusty_crew_openai_responses_brain::ResponsesReasoningConfig {
            effort: Some(effort),
            summary: None,
        }
    });
    config.max_output_tokens = input.config.max_output_tokens;
    config.provider_request_timeout_ms = input.config.provider_request_timeout_ms;
    if let Some(max_continuation_rounds) = input.config.max_continuation_rounds {
        if max_continuation_rounds == 0 || max_continuation_rounds > 512 {
            return Err(napi::Error::new(
                napi::Status::InvalidArg,
                "OpenAI Responses max_continuation_rounds must be between 1 and 512",
            ));
        }
        config.max_continuation_rounds = max_continuation_rounds;
    }
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
        continuation_state: None,
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
                config.provider_request_timeout_ms,
                provider_cancellation,
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
