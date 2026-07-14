use super::*;
use rusty_crew_brain_runtime::{
    BrainRuntimeError, BufferedBrainHostToolResult, BufferedBrainHostToolStatus,
    BufferedBrainTurnCoordinator, BufferedBrainTurnError, BufferedBrainTurnLimits,
    BufferedBrainTurnRegistry, BufferedBrainTurnRun, BufferedNeutralPendingToolRequest,
    BufferedNeutralToolOutputPoll,
};
use rusty_crew_pi_agent_brain::{
    ChatCompletionMessage, ChatCompletionsEvent, FakeChatCompletionsClient,
    LiveChatCompletionsClient, NeutralBrainTool as PiAgentNeutralBrainTool,
    PendingChatFunctionCall, PiAgentBrainLoop, PiAgentBrainLoopConfig, PiAgentBrainLoopInput,
    PiAgentChatConfig, PiAgentFinalMessage, PiAgentNeutralToolExecutor, PiAgentToolOutput,
    ProviderCancellation,
};
use serde::Serialize;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsPiAgentBrainRunInput {
    wake_id: String,
    session_id: String,
    #[serde(default)]
    messages: Vec<ChatCompletionMessage>,
    #[serde(default)]
    tools: Vec<JsPiAgentNeutralTool>,
    config: JsPiAgentBrainConfig,
    #[serde(default)]
    client: JsPiAgentClientConfig,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsPiAgentNeutralTool {
    name: String,
    description: String,
    input_schema: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsPiAgentBrainConfig {
    model: String,
    #[serde(default)]
    provider_request_timeout_ms: Option<u64>,
    #[serde(default)]
    wake_timeout_ms: Option<u64>,
    #[serde(default)]
    temperature_milli: Option<u32>,
    #[serde(default)]
    max_output_tokens: Option<u32>,
    #[serde(default)]
    max_tool_rounds: Option<usize>,
    #[serde(default)]
    repeated_tool_call_limit: Option<usize>,
    #[serde(default)]
    final_message_fallback_text: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
enum JsPiAgentClientConfig {
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
struct JsPiAgentToolOutputInput {
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
struct JsPiAgentCancelInput {
    wake_id: String,
    reason_code: String,
    summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PiAgentTransportMetrics {
    provider_request_count: usize,
    tool_round_count: usize,
}

#[derive(Debug, Default)]
pub(crate) struct PiAgentBufferedRunPayload {
    transport_metrics: Option<PiAgentTransportMetrics>,
    provider_finished: bool,
    provider_cancellation: ProviderCancellation,
}

pub(crate) type PiAgentBufferedRunRegistry = BufferedBrainTurnRegistry<PiAgentBufferedRunPayload>;

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

pub(crate) fn start_pi_agent_brain_json(
    buffered_runs: Arc<PiAgentBufferedRunRegistry>,
    input_json: String,
) -> napi::Result<String> {
    let input: JsPiAgentBrainRunInput = serde_json::from_str(&input_json).map_err(|error| {
        napi::Error::new(
            napi::Status::InvalidArg,
            format!("invalid pi-agent brain input JSON: {error}"),
        )
    })?;
    let wake_id = input.wake_id.clone();
    let mut coordinator = BufferedBrainTurnCoordinator::new(
        "pi-agent",
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
            PiAgentBufferedRunPayload::default(),
        ))
        .map_err(brain_runtime_error_to_napi)?;
    let thread_wake_id = wake_id.clone();
    std::thread::spawn(move || {
        run_pi_agent_brain_buffered(buffered_runs, thread_wake_id, input_json)
    });
    serde_json::to_string(&json!({ "wake_id": wake_id })).map_err(|error| {
        napi::Error::new(
            napi::Status::GenericFailure,
            format!("serialize pi-agent buffered wake start: {error}"),
        )
    })
}

pub(crate) fn drain_pi_agent_brain_stream_json(
    buffered_runs: &PiAgentBufferedRunRegistry,
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
            let tool_requests = run.coordinator.drain_host_tool_requests(128);
            let terminal = drain.terminal && run.payload.provider_finished;
            let error = terminal
                .then(|| run.coordinator.terminal())
                .flatten()
                .filter(|_| run.coordinator.has_error())
                .map(|terminal| terminal.summary.clone());
            let output = json!({
                "wake_id": wake_id,
                "items": drain.items.into_iter().map(|item| item.item).collect::<Vec<_>>(),
                "tool_requests": tool_requests,
                "terminal": terminal,
                "transport_metrics": terminal.then(|| run.payload.transport_metrics.clone()).flatten(),
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
            format!("serialize pi-agent buffered wake drain: {error}"),
        )
    })
}

pub(crate) fn submit_pi_agent_tool_output_json(
    buffered_runs: &PiAgentBufferedRunRegistry,
    input_json: String,
) -> napi::Result<String> {
    let input: JsPiAgentToolOutputInput = serde_json::from_str(&input_json).map_err(|error| {
        napi::Error::new(
            napi::Status::InvalidArg,
            format!("invalid pi-agent tool output JSON: {error}"),
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
            format!("serialize pi-agent tool output receipt: {error}"),
        )
    })
}

pub(crate) fn cancel_pi_agent_brain_json(
    buffered_runs: &PiAgentBufferedRunRegistry,
    input_json: String,
) -> napi::Result<String> {
    let input: JsPiAgentCancelInput = serde_json::from_str(&input_json).map_err(|error| {
        napi::Error::new(
            napi::Status::InvalidArg,
            format!("invalid pi-agent cancel JSON: {error}"),
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
            format!("serialize pi-agent cancel receipt: {error}"),
        )
    })
}

fn run_pi_agent_brain_buffered(
    buffered_runs: Arc<PiAgentBufferedRunRegistry>,
    wake_id: String,
    input_json: String,
) {
    let result = run_pi_agent_brain_with_buffered_tools(
        Arc::clone(&buffered_runs),
        wake_id.clone(),
        input_json,
    );
    let _ = buffered_runs.with_run_mut(&wake_id, |run| {
        if !run.coordinator.is_cancelled() {
            match result {
                Ok(output) => {
                    for item in output.stream {
                        if run.coordinator.phase().is_terminal() {
                            break;
                        }
                        if run.coordinator.enqueue_provider_stream_item(item).is_err() {
                            break;
                        }
                    }
                    run.payload.transport_metrics = Some(PiAgentTransportMetrics {
                        provider_request_count: output.provider_request_count,
                        tool_round_count: output.tool_round_count,
                    });
                    if !run.coordinator.phase().is_terminal() {
                        let _ = run.coordinator.fail(
                            "provider_stream_missing_terminal",
                            "pi-agent provider loop ended without a terminal stream item",
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

fn run_pi_agent_brain_with_buffered_tools(
    buffered_runs: Arc<PiAgentBufferedRunRegistry>,
    wake_id: String,
    input_json: String,
) -> napi::Result<rusty_crew_pi_agent_brain::PiAgentBrainLoopOutput> {
    let input: JsPiAgentBrainRunInput = serde_json::from_str(&input_json).map_err(|error| {
        napi::Error::new(
            napi::Status::InvalidArg,
            format!("invalid pi-agent brain input JSON: {error}"),
        )
    })?;
    let provider_cancellation = buffered_runs
        .with_run_mut(&wake_id, |run| run.payload.provider_cancellation.clone())
        .map_err(brain_runtime_error_to_napi)?;
    let chat_config = pi_agent_chat_config(&input.config);
    let loop_config = PiAgentBrainLoopConfig {
        max_tool_rounds: input.config.max_tool_rounds.unwrap_or(8),
        repeated_tool_call_limit: input.config.repeated_tool_call_limit.unwrap_or(3),
    };
    let descriptors = input
        .tools
        .iter()
        .map(|tool| PiAgentNeutralBrainTool {
            name: tool.name.clone(),
            description: tool.description.clone(),
            input_schema: normalize_pi_agent_tool_schema(&tool.input_schema),
        })
        .collect::<Vec<_>>();
    let context = rusty_crew_pi_agent_brain::BrainEventContext {
        wake_id: input.wake_id,
        session_id: SessionId::new(input.session_id),
    };
    let final_message_fallback = input
        .config
        .final_message_fallback_text
        .filter(|text| !text.trim().is_empty())
        .map(|text| PiAgentFinalMessage {
            text: Some(text),
            ..PiAgentFinalMessage::default()
        });
    let loop_input = PiAgentBrainLoopInput {
        context,
        messages: input.messages,
        final_message_fallback,
    };
    match input.client {
        JsPiAgentClientConfig::Fake => {
            let client = fake_pi_agent_client(
                descriptors
                    .first()
                    .map(|descriptor| descriptor.name.as_str()),
            );
            let mut brain = PiAgentBrainLoop::new(
                client,
                BufferedPiAgentToolExecutor {
                    wake_id,
                    buffered_runs,
                },
                chat_config,
                descriptors,
            )
            .with_loop_config(loop_config);
            Ok(brain.wake(loop_input))
        }
        JsPiAgentClientConfig::Live { base_url, api_key } => {
            let client = LiveChatCompletionsClient::new(
                base_url,
                api_key,
                chat_config.provider_request_timeout_ms,
                provider_cancellation,
            )
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))?;
            let mut brain = PiAgentBrainLoop::new(
                client,
                BufferedPiAgentToolExecutor {
                    wake_id,
                    buffered_runs,
                },
                chat_config,
                descriptors,
            )
            .with_loop_config(loop_config);
            Ok(brain.wake(loop_input))
        }
    }
}

fn pi_agent_chat_config(config: &JsPiAgentBrainConfig) -> PiAgentChatConfig {
    PiAgentChatConfig {
        model: config.model.clone(),
        temperature_milli: config.temperature_milli,
        max_output_tokens: config.max_output_tokens,
        provider_request_timeout_ms: config.provider_request_timeout_ms,
    }
}

fn normalize_pi_agent_tool_schema(schema: &Value) -> Value {
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

fn fake_pi_agent_client(tool_name: Option<&str>) -> FakeChatCompletionsClient {
    let Some(tool_name) = tool_name else {
        return FakeChatCompletionsClient::new([Ok(vec![
            ChatCompletionsEvent::ContentDelta(
                "<think>pi-agent Rust reasoning</think>pi-agent Rust bridge wake completed"
                    .to_string(),
            ),
            ChatCompletionsEvent::Finished {
                finish_reason: Some("stop".to_string()),
            },
        ])]);
    };
    if tool_name == "repeat_failure_tool" {
        return FakeChatCompletionsClient::new([
            fake_pi_agent_tool_call_script(tool_name, "fake-pi-call-1"),
            fake_pi_agent_tool_call_script(tool_name, "fake-pi-call-2"),
            Ok(vec![
                ChatCompletionsEvent::ContentDelta(
                    "pi-agent should not reach post-policy completion".to_string(),
                ),
                ChatCompletionsEvent::Finished {
                    finish_reason: Some("stop".to_string()),
                },
            ]),
        ]);
    }
    FakeChatCompletionsClient::new([
        fake_pi_agent_tool_call_script(tool_name, "fake-pi-call"),
        Ok(vec![
            ChatCompletionsEvent::ContentDelta(
                "<think>pi-agent Rust reasoning</think>pi-agent Rust bridge wake completed"
                    .to_string(),
            ),
            ChatCompletionsEvent::Finished {
                finish_reason: Some("stop".to_string()),
            },
        ]),
    ])
}

fn fake_pi_agent_tool_call_script(
    tool_name: &str,
    call_id: &str,
) -> Result<Vec<ChatCompletionsEvent>, rusty_crew_pi_agent_brain::ChatCompletionsStreamError> {
    Ok(vec![
        ChatCompletionsEvent::ToolCallFinished(PendingChatFunctionCall {
            index: 0,
            id: Some(call_id.to_string()),
            name: tool_name.to_string(),
            arguments_json: "{}".to_string(),
        }),
        ChatCompletionsEvent::Finished {
            finish_reason: Some("tool_calls".to_string()),
        },
    ])
}

struct BufferedPiAgentToolExecutor {
    wake_id: String,
    buffered_runs: Arc<PiAgentBufferedRunRegistry>,
}

impl PiAgentNeutralToolExecutor for BufferedPiAgentToolExecutor {
    fn execute(&self, call: &PendingChatFunctionCall) -> PiAgentToolOutput {
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
        {
            let queued = self.buffered_runs.with_run_mut(&self.wake_id, |run| {
                run.coordinator.queue_tool_request(request)
            });
            if !matches!(queued, Ok(Ok(()))) {
                return PiAgentToolOutput::error(format!(
                    "pi-agent buffered wake {} disappeared before tool request {}",
                    self.wake_id, call_id
                ));
            }
        }

        loop {
            let result = self.buffered_runs.with_run_mut(&self.wake_id, |run| {
                if let Some(cancellation) = run.coordinator.cancellation() {
                    return Some(PiAgentToolOutput::cancelled(format!(
                        "pi-agent buffered wake {} cancelled before tool output {}: {}",
                        self.wake_id, call_id, cancellation.summary
                    )));
                }
                if let BufferedNeutralToolOutputPoll::Ready(output) =
                    run.coordinator.poll_submitted_tool_output(&call_id)
                {
                    return Some(if output.is_error {
                        PiAgentToolOutput::error(output.output)
                    } else {
                        PiAgentToolOutput::ok(output.output)
                    });
                }
                if run.coordinator.phase().is_terminal() {
                    let summary = run
                        .coordinator
                        .terminal()
                        .map(|terminal| terminal.summary.as_str())
                        .unwrap_or("turn ended");
                    return Some(PiAgentToolOutput::error(format!(
                        "pi-agent buffered wake {} ended before tool output {}: {}",
                        self.wake_id, call_id, summary
                    )));
                }
                if run.coordinator.timeout_if_due() {
                    run.payload.provider_cancellation.cancel();
                    return Some(PiAgentToolOutput::timed_out(
                        run.coordinator
                            .terminal()
                            .map(|terminal| terminal.summary.clone())
                            .unwrap_or_else(|| "pi-agent buffered wake timed out".to_string()),
                    ));
                }
                None
            });
            match result {
                Ok(Some(output)) => return output,
                Err(_) => {
                    return PiAgentToolOutput::error(format!(
                        "pi-agent buffered wake {} disappeared before tool output {}",
                        self.wake_id, call_id
                    ));
                }
                Ok(None) => {}
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    }
}
