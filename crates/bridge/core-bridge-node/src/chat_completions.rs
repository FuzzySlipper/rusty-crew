use super::*;
use rusty_crew_brain_runtime::{
    BrainRuntimeError, BufferedBrainHostToolResult, BufferedBrainHostToolStatus,
    BufferedBrainTurnCoordinator, BufferedBrainTurnError, BufferedBrainTurnLimits,
    BufferedBrainTurnRegistry, BufferedBrainTurnRun, BufferedNeutralPendingToolRequest,
    BufferedNeutralToolOutputPoll,
};
use rusty_crew_chat_completions_brain::{
    ChatCompletionMessage, ChatCompletionsBrainLoop, ChatCompletionsBrainLoopConfig,
    ChatCompletionsBrainLoopInput, ChatCompletionsChatConfig, ChatCompletionsEvent,
    ChatCompletionsFinalMessage, ChatCompletionsInputImage, ChatCompletionsNeutralToolExecutor,
    ChatCompletionsToolOutput, FakeChatCompletionsClient, LiveChatCompletionsClient,
    NeutralBrainTool as ChatCompletionsNeutralBrainTool, PendingChatFunctionCall,
    ProviderCancellation, DEFAULT_NO_PROGRESS_ATTENTION_THRESHOLD,
    DEFAULT_WORK_QUANTUM_TOOL_ROUNDS,
};
use rusty_crew_core_protocol::{
    BrainWakeAttention, BrainWakeProviderStateInput, ChatCompletionsReasoningHistory,
    ChatCompletionsThinkingMode, ChatCompletionsWireDialect,
};
use serde::Serialize;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsChatCompletionsBrainRunInput {
    wake_id: String,
    session_id: String,
    #[serde(default)]
    messages: Vec<ChatCompletionMessage>,
    #[serde(default)]
    input_images: Vec<ChatCompletionsInputImage>,
    #[serde(default)]
    provider_state: Option<BrainWakeProviderStateInput>,
    #[serde(default)]
    continuation_state: Option<rusty_crew_core_protocol::BrainContinuationPayload>,
    #[serde(default)]
    tools: Vec<JsChatCompletionsNeutralTool>,
    config: JsChatCompletionsBrainConfig,
    #[serde(default)]
    client: JsChatCompletionsClientConfig,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsChatCompletionsNeutralTool {
    name: String,
    description: String,
    input_schema: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsChatCompletionsBrainConfig {
    model: String,
    #[serde(default)]
    provider_request_timeout_ms: Option<u64>,
    #[serde(default)]
    wake_timeout_ms: Option<u64>,
    #[serde(default)]
    temperature_milli: Option<u32>,
    #[serde(default)]
    reasoning_effort: Option<String>,
    #[serde(default)]
    wire_dialect: ChatCompletionsWireDialect,
    #[serde(default)]
    thinking_mode: ChatCompletionsThinkingMode,
    #[serde(default)]
    reasoning_history: ChatCompletionsReasoningHistory,
    #[serde(default)]
    reasoning_budget_tokens: Option<u32>,
    #[serde(default = "default_chat_completions_strategy_id")]
    provider_state_strategy_id: String,
    #[serde(default)]
    max_output_tokens: Option<u32>,
    #[serde(default)]
    work_quantum_tool_rounds: Option<usize>,
    #[serde(default)]
    no_progress_attention_threshold: Option<u32>,
    #[serde(default)]
    final_message_fallback_text: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
enum JsChatCompletionsClientConfig {
    #[default]
    Fake,
    Live {
        base_url: String,
        #[serde(default)]
        api_key: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsChatCompletionsToolOutputInput {
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
struct JsChatCompletionsCancelInput {
    wake_id: String,
    reason_code: String,
    summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ChatCompletionsTransportMetrics {
    provider_request_count: usize,
    tool_round_count: usize,
    provider_event_counts: std::collections::BTreeMap<String, usize>,
    provider_request_debug_samples: Vec<Value>,
}

fn default_chat_completions_strategy_id() -> String {
    "default".to_string()
}

#[derive(Debug, Default)]
pub(crate) struct ChatCompletionsBufferedRunPayload {
    transport_metrics: Option<ChatCompletionsTransportMetrics>,
    provider_finished: bool,
    provider_cancellation: ProviderCancellation,
    continuation_state: Option<rusty_crew_core_protocol::BrainContinuationPayload>,
    attention: Option<BrainWakeAttention>,
}

pub(crate) type ChatCompletionsBufferedRunRegistry =
    BufferedBrainTurnRegistry<ChatCompletionsBufferedRunPayload>;

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

pub(crate) fn start_chat_completions_brain_json(
    buffered_runs: Arc<ChatCompletionsBufferedRunRegistry>,
    input_json: String,
) -> napi::Result<String> {
    let input: JsChatCompletionsBrainRunInput =
        serde_json::from_str(&input_json).map_err(|error| {
            napi::Error::new(
                napi::Status::InvalidArg,
                format!("invalid chat-completions brain input JSON: {error}"),
            )
        })?;
    validate_chat_completions_input_images(&input.input_images)?;
    let wake_id = input.wake_id.clone();
    let mut coordinator = BufferedBrainTurnCoordinator::new(
        "chat-completions",
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
            ChatCompletionsBufferedRunPayload::default(),
        ))
        .map_err(brain_runtime_error_to_napi)?;
    let thread_wake_id = wake_id.clone();
    std::thread::spawn(move || {
        run_chat_completions_brain_buffered(buffered_runs, thread_wake_id, input_json)
    });
    serde_json::to_string(&json!({ "wake_id": wake_id })).map_err(|error| {
        napi::Error::new(
            napi::Status::GenericFailure,
            format!("serialize chat-completions buffered wake start: {error}"),
        )
    })
}

pub(crate) fn drain_chat_completions_brain_stream_json(
    buffered_runs: &ChatCompletionsBufferedRunRegistry,
    wake_id: String,
    max_items: Option<u32>,
) -> napi::Result<String> {
    let max_items = max_items.unwrap_or(64).max(1) as usize;
    let terminal = buffered_runs
        .with_run_mut(&wake_id, |run| {
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
                "attention": terminal.then(|| run.payload.attention.clone()).flatten(),
                "terminal_reason_code": terminal_reason_code,
                "transport_metrics": terminal.then(|| run.payload.transport_metrics.clone()).flatten(),
                "provider_state": terminal.then(|| run.coordinator.provider_state_output().cloned()).flatten(),
                "yielded": terminal && run.coordinator.phase() == rusty_crew_brain_runtime::BufferedBrainTurnPhase::Yielded,
                "continuation_state": terminal.then(|| run.payload.continuation_state.clone()).flatten(),
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
            format!("serialize chat-completions buffered wake drain: {error}"),
        )
    })
}

pub(crate) fn submit_chat_completions_tool_output_json(
    buffered_runs: &ChatCompletionsBufferedRunRegistry,
    input_json: String,
) -> napi::Result<String> {
    let input: JsChatCompletionsToolOutputInput =
        serde_json::from_str(&input_json).map_err(|error| {
            napi::Error::new(
                napi::Status::InvalidArg,
                format!("invalid chat-completions tool output JSON: {error}"),
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
            format!("serialize chat-completions tool output receipt: {error}"),
        )
    })
}

pub(crate) fn cancel_chat_completions_brain_json(
    buffered_runs: &ChatCompletionsBufferedRunRegistry,
    input_json: String,
) -> napi::Result<String> {
    let input: JsChatCompletionsCancelInput =
        serde_json::from_str(&input_json).map_err(|error| {
            napi::Error::new(
                napi::Status::InvalidArg,
                format!("invalid chat-completions cancel JSON: {error}"),
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
            format!("serialize chat-completions cancel receipt: {error}"),
        )
    })
}

fn run_chat_completions_brain_buffered(
    buffered_runs: Arc<ChatCompletionsBufferedRunRegistry>,
    wake_id: String,
    input_json: String,
) {
    let sink_wake_id = wake_id.clone();
    let sink_buffered_runs = Arc::clone(&buffered_runs);
    let mut sink = move |item: BrainWakeStreamItem| loop {
        let attempt = sink_buffered_runs.with_run_mut(&sink_wake_id, |run| {
            if run.coordinator.phase().is_terminal() {
                return Ok(false);
            }
            run.coordinator
                .enqueue_provider_stream_item(item.clone())
                .map(|_| true)
        });
        match attempt {
            Ok(Ok(true)) | Ok(Ok(false)) => break,
            Ok(Err(BufferedBrainTurnError::BufferLimitExceeded { .. })) => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Ok(Err(error)) => {
                eprintln!(
                    "chat-completions wake {} could not enqueue a live stream item: {}",
                    sink_wake_id, error
                );
                break;
            }
            Err(error) => {
                eprintln!(
                        "chat-completions wake {} could not access its buffered run while streaming: {}",
                        sink_wake_id, error
                    );
                break;
            }
        }
    };
    let result = run_chat_completions_brain_with_buffered_tools(
        Arc::clone(&buffered_runs),
        wake_id.clone(),
        input_json,
        &mut sink,
    );
    let _ = buffered_runs.with_run_mut(&wake_id, |run| {
        if !run.coordinator.is_cancelled() {
            match result {
                Ok(output) => {
                    run.payload.transport_metrics = Some(ChatCompletionsTransportMetrics {
                        provider_request_count: output.provider_request_count,
                        tool_round_count: output.tool_round_count,
                        provider_event_counts: output.provider_event_counts,
                        provider_request_debug_samples: output.provider_request_debug_samples,
                    });
                    if let Some(provider_state) = output.provider_state {
                        let _ = run.coordinator.set_provider_state_output(provider_state);
                    }
                    run.payload.continuation_state = output.continuation_state;
                    if let Some(attention) = output.attention {
                        let reason_code = attention.reason_code.clone();
                        let summary = attention.summary.clone();
                        run.payload.attention = Some(attention);
                        if !run.coordinator.phase().is_terminal() {
                            let _ = run.coordinator.require_attention(reason_code, summary);
                        }
                    }
                    if output.yielded && !run.coordinator.phase().is_terminal() {
                        let _ = run.coordinator.yield_turn();
                    }
                    if !run.coordinator.phase().is_terminal() {
                        let _ = run.coordinator.fail(
                            "provider_stream_missing_terminal",
                            "chat-completions provider loop ended without a terminal stream item",
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

fn run_chat_completions_brain_with_buffered_tools(
    buffered_runs: Arc<ChatCompletionsBufferedRunRegistry>,
    wake_id: String,
    input_json: String,
    sink: &mut dyn FnMut(BrainWakeStreamItem),
) -> napi::Result<rusty_crew_chat_completions_brain::ChatCompletionsBrainLoopOutput> {
    let input: JsChatCompletionsBrainRunInput =
        serde_json::from_str(&input_json).map_err(|error| {
            napi::Error::new(
                napi::Status::InvalidArg,
                format!("invalid chat-completions brain input JSON: {error}"),
            )
        })?;
    let provider_cancellation = buffered_runs
        .with_run_mut(&wake_id, |run| run.payload.provider_cancellation.clone())
        .map_err(brain_runtime_error_to_napi)?;
    let chat_config = chat_completions_chat_config(&input.config);
    chat_config.validate().map_err(|error| {
        napi::Error::new(
            napi::Status::InvalidArg,
            format!("invalid chat-completions provider config: {error}"),
        )
    })?;
    let loop_config = ChatCompletionsBrainLoopConfig {
        work_quantum_tool_rounds: input
            .config
            .work_quantum_tool_rounds
            .unwrap_or(DEFAULT_WORK_QUANTUM_TOOL_ROUNDS),
        no_progress_attention_threshold: input
            .config
            .no_progress_attention_threshold
            .unwrap_or(DEFAULT_NO_PROGRESS_ATTENTION_THRESHOLD),
    };
    let descriptors = input
        .tools
        .iter()
        .map(|tool| ChatCompletionsNeutralBrainTool {
            name: tool.name.clone(),
            description: tool.description.clone(),
            input_schema: normalize_chat_completions_tool_schema(&tool.input_schema),
        })
        .collect::<Vec<_>>();
    let context = rusty_crew_chat_completions_brain::BrainEventContext {
        wake_id: input.wake_id,
        session_id: SessionId::new(input.session_id),
    };
    let final_message_fallback = input
        .config
        .final_message_fallback_text
        .filter(|text| !text.trim().is_empty())
        .map(|text| ChatCompletionsFinalMessage {
            text: Some(text),
            ..ChatCompletionsFinalMessage::default()
        });
    let loop_input = ChatCompletionsBrainLoopInput {
        context,
        messages: input.messages,
        input_images: input.input_images,
        provider_state: input.provider_state,
        continuation_state: input.continuation_state,
        final_message_fallback,
    };
    match input.client {
        JsChatCompletionsClientConfig::Fake => {
            let client = fake_chat_completions_client(
                descriptors
                    .first()
                    .map(|descriptor| descriptor.name.as_str()),
            );
            let mut brain = ChatCompletionsBrainLoop::new(
                client,
                BufferedChatCompletionsToolExecutor {
                    wake_id,
                    buffered_runs,
                },
                chat_config,
                descriptors,
            )
            .with_loop_config(loop_config);
            Ok(brain.wake_with_stream_sink(loop_input, sink))
        }
        JsChatCompletionsClientConfig::Live { base_url, api_key } => {
            let client = LiveChatCompletionsClient::new(
                base_url,
                api_key,
                chat_config.provider_request_timeout_ms,
                provider_cancellation,
            )
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))?;
            let mut brain = ChatCompletionsBrainLoop::new(
                client,
                BufferedChatCompletionsToolExecutor {
                    wake_id,
                    buffered_runs,
                },
                chat_config,
                descriptors,
            )
            .with_loop_config(loop_config);
            Ok(brain.wake_with_stream_sink(loop_input, sink))
        }
    }
}

fn validate_chat_completions_input_images(
    images: &[ChatCompletionsInputImage],
) -> napi::Result<()> {
    const MAX_IMAGES: usize = 4;
    const MAX_IMAGE_BYTES: u64 = 10 * 1024 * 1024;
    const MAX_TOTAL_BYTES: u64 = 20 * 1024 * 1024;
    if images.len() > MAX_IMAGES {
        return Err(napi::Error::new(
            napi::Status::InvalidArg,
            format!("chat-completions image input exceeds {MAX_IMAGES} images"),
        ));
    }
    let mut total = 0u64;
    for image in images {
        if !matches!(
            image.mime_type.as_str(),
            "image/png" | "image/jpeg" | "image/gif" | "image/webp"
        ) {
            return Err(napi::Error::new(
                napi::Status::InvalidArg,
                format!(
                    "chat-completions image {} has unsupported MIME type {}",
                    image.attachment_id, image.mime_type
                ),
            ));
        }
        let decoded_len = canonical_base64_decoded_len(&image.bytes_base64).ok_or_else(|| {
            napi::Error::new(
                napi::Status::InvalidArg,
                format!(
                    "chat-completions image {} is not canonical base64",
                    image.attachment_id
                ),
            )
        })?;
        if decoded_len == 0 || decoded_len != image.byte_size || decoded_len > MAX_IMAGE_BYTES {
            return Err(napi::Error::new(
                napi::Status::InvalidArg,
                format!(
                    "chat-completions image {} has invalid byte size {}",
                    image.attachment_id, image.byte_size
                ),
            ));
        }
        total = total.saturating_add(decoded_len);
        if total > MAX_TOTAL_BYTES {
            return Err(napi::Error::new(
                napi::Status::InvalidArg,
                "chat-completions image input exceeds the total byte limit".to_string(),
            ));
        }
    }
    Ok(())
}

fn canonical_base64_decoded_len(value: &str) -> Option<u64> {
    if value.is_empty() || !value.len().is_multiple_of(4) {
        return None;
    }
    let padding = value.bytes().rev().take_while(|byte| *byte == b'=').count();
    if padding > 2 {
        return None;
    }
    let content_len = value.len().saturating_sub(padding);
    if !value.bytes().enumerate().all(|(index, byte)| {
        if index >= content_len {
            byte == b'='
        } else {
            byte.is_ascii_alphanumeric() || byte == b'+' || byte == b'/'
        }
    }) {
        return None;
    }
    Some(((value.len() / 4) * 3 - padding) as u64)
}

fn chat_completions_chat_config(
    config: &JsChatCompletionsBrainConfig,
) -> ChatCompletionsChatConfig {
    ChatCompletionsChatConfig {
        model: config.model.clone(),
        temperature_milli: config.temperature_milli,
        reasoning_effort: config.reasoning_effort.clone(),
        wire_dialect: config.wire_dialect,
        thinking_mode: config.thinking_mode,
        reasoning_history: config.reasoning_history,
        reasoning_budget_tokens: config.reasoning_budget_tokens,
        provider_state_strategy_id: config.provider_state_strategy_id.clone(),
        max_output_tokens: config.max_output_tokens,
        provider_request_timeout_ms: config.provider_request_timeout_ms,
    }
}

fn normalize_chat_completions_tool_schema(schema: &Value) -> Value {
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

fn fake_chat_completions_client(tool_name: Option<&str>) -> FakeChatCompletionsClient {
    let Some(tool_name) = tool_name else {
        return FakeChatCompletionsClient::new([Ok(vec![
            ChatCompletionsEvent::ContentDelta(
                "<think>chat-completions Rust reasoning</think>chat-completions Rust bridge wake completed"
                    .to_string(),
            ),
            ChatCompletionsEvent::Finished {
                finish_reason: Some("stop".to_string()),
            },
        ])]);
    };
    if tool_name == "repeat_failure_tool" {
        return FakeChatCompletionsClient::new([
            fake_chat_completions_tool_call_script(tool_name, "fake-chat-call-1", "{}"),
            fake_chat_completions_tool_call_script(tool_name, "fake-chat-call-2", "{}"),
            Ok(vec![
                ChatCompletionsEvent::ContentDelta(
                    "chat-completions recovered after repeated tool failure guidance".to_string(),
                ),
                ChatCompletionsEvent::Finished {
                    finish_reason: Some("stop".to_string()),
                },
            ]),
        ]);
    }
    if tool_name == "no_progress_failure_tool" {
        return FakeChatCompletionsClient::new((1..=4).map(|round| {
            fake_chat_completions_tool_call_script(
                tool_name,
                &format!("fake-chat-no-progress-{round}"),
                "{}",
            )
        }));
    }
    if tool_name == "long_continuation_tool" {
        let mut scripts = (1..=12)
            .map(|round| {
                fake_chat_completions_tool_call_script(
                    tool_name,
                    &format!("fake-chat-call-{round}"),
                    &format!(r#"{{"round":{round}}}"#),
                )
            })
            .collect::<Vec<_>>();
        scripts.push(Ok(vec![
            ChatCompletionsEvent::ContentDelta(
                "chat-completions long continuation completed".to_string(),
            ),
            ChatCompletionsEvent::Finished {
                finish_reason: Some("stop".to_string()),
            },
        ]));
        return FakeChatCompletionsClient::new(scripts);
    }
    FakeChatCompletionsClient::new([
        fake_chat_completions_tool_call_script(tool_name, "fake-chat-call", "{}"),
        Ok(vec![
            ChatCompletionsEvent::ContentDelta(
                "<think>chat-completions Rust reasoning</think>chat-completions Rust bridge wake completed"
                    .to_string(),
            ),
            ChatCompletionsEvent::Finished {
                finish_reason: Some("stop".to_string()),
            },
        ]),
    ])
}

fn fake_chat_completions_tool_call_script(
    tool_name: &str,
    call_id: &str,
    arguments_json: &str,
) -> Result<Vec<ChatCompletionsEvent>, rusty_crew_chat_completions_brain::ChatCompletionsStreamError>
{
    Ok(vec![
        ChatCompletionsEvent::ToolCallFinished(PendingChatFunctionCall {
            index: 0,
            id: Some(call_id.to_string()),
            name: tool_name.to_string(),
            arguments_json: arguments_json.to_string(),
        }),
        ChatCompletionsEvent::Finished {
            finish_reason: Some("tool_calls".to_string()),
        },
    ])
}

struct BufferedChatCompletionsToolExecutor {
    wake_id: String,
    buffered_runs: Arc<ChatCompletionsBufferedRunRegistry>,
}

impl ChatCompletionsNeutralToolExecutor for BufferedChatCompletionsToolExecutor {
    fn execute(&self, call: &PendingChatFunctionCall) -> ChatCompletionsToolOutput {
        let call_id = call
            .id
            .clone()
            .unwrap_or_else(|| format!("call_{}", call.index));
        let request = BufferedNeutralPendingToolRequest {
            call_id: call_id.clone(),
            provider_item_id: None,
            name: call.name.clone(),
            arguments_json: call.arguments_json.clone(),
        };
        loop {
            let queued = self.buffered_runs.with_run_mut(&self.wake_id, |run| {
                run.coordinator.queue_tool_request(request.clone())
            });
            match queued {
                Ok(Ok(())) => break,
                Ok(Err(BufferedBrainTurnError::BufferLimitExceeded { .. })) => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                _ => {
                    return ChatCompletionsToolOutput::error(format!(
                        "chat-completions buffered wake {} ended before tool request {}",
                        self.wake_id, call_id
                    ));
                }
            }
        }

        loop {
            let result = self.buffered_runs.with_run_mut(&self.wake_id, |run| {
                if let Some(cancellation) = run.coordinator.cancellation() {
                    return Some(ChatCompletionsToolOutput::cancelled(format!(
                        "chat-completions buffered wake {} cancelled before tool output {}: {}",
                        self.wake_id, call_id, cancellation.summary
                    )));
                }
                if let BufferedNeutralToolOutputPoll::Ready(output) =
                    run.coordinator.poll_submitted_tool_output(&call_id)
                {
                    return Some(if output.is_error {
                        ChatCompletionsToolOutput::error(output.output)
                    } else {
                        ChatCompletionsToolOutput::ok(output.output)
                    });
                }
                if run.coordinator.phase().is_terminal() {
                    let summary = run
                        .coordinator
                        .terminal()
                        .map(|terminal| terminal.summary.as_str())
                        .unwrap_or("turn ended");
                    return Some(ChatCompletionsToolOutput::error(format!(
                        "chat-completions buffered wake {} ended before tool output {}: {}",
                        self.wake_id, call_id, summary
                    )));
                }
                if run.coordinator.timeout_if_due() {
                    run.payload.provider_cancellation.cancel();
                    return Some(ChatCompletionsToolOutput::timed_out(
                        run.coordinator
                            .terminal()
                            .map(|terminal| terminal.summary.clone())
                            .unwrap_or_else(|| {
                                "chat-completions buffered wake timed out".to_string()
                            }),
                    ));
                }
                None
            });
            match result {
                Ok(Some(output)) => return output,
                Err(_) => {
                    return ChatCompletionsToolOutput::error(format!(
                        "chat-completions buffered wake {} disappeared before tool output {}",
                        self.wake_id, call_id
                    ));
                }
                Ok(None) => {}
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    }
}

#[cfg(test)]
mod image_input_tests {
    use super::*;

    #[test]
    fn native_image_input_validation_rejects_malformed_and_mismatched_content() {
        assert!(
            validate_chat_completions_input_images(&[ChatCompletionsInputImage {
                attachment_id: "bad-base64".to_string(),
                mime_type: "image/png".to_string(),
                bytes_base64: "not base64".to_string(),
                byte_size: 6,
            }])
            .is_err()
        );
        assert!(
            validate_chat_completions_input_images(&[ChatCompletionsInputImage {
                attachment_id: "wrong-size".to_string(),
                mime_type: "image/png".to_string(),
                bytes_base64: "YWJj".to_string(),
                byte_size: 4,
            }])
            .is_err()
        );
        assert!(
            validate_chat_completions_input_images(&[ChatCompletionsInputImage {
                attachment_id: "unsupported".to_string(),
                mime_type: "image/svg+xml".to_string(),
                bytes_base64: "YWJj".to_string(),
                byte_size: 3,
            }])
            .is_err()
        );
    }

    #[test]
    fn native_image_input_validation_accepts_canonical_bounded_content() {
        validate_chat_completions_input_images(&[ChatCompletionsInputImage {
            attachment_id: "image-1".to_string(),
            mime_type: "image/png".to_string(),
            bytes_base64: "YWJj".to_string(),
            byte_size: 3,
        }])
        .expect("canonical bounded image input");
    }
}
