use super::*;
use rusty_crew_brain_runtime::{
    BrainRuntimeError, BufferedNeutralPendingToolRequest, BufferedNeutralRun,
    BufferedNeutralRunRegistry, BufferedNeutralToolOutput,
};
use rusty_crew_pi_agent_brain::{
    ChatCompletionMessage, ChatCompletionsEvent, FakeChatCompletionsClient,
    LiveChatCompletionsClient, NeutralBrainTool as PiAgentNeutralBrainTool,
    PendingChatFunctionCall, PiAgentBrainLoop, PiAgentBrainLoopConfig, PiAgentBrainLoopInput,
    PiAgentChatConfig, PiAgentFinalMessage, PiAgentNeutralToolExecutor, PiAgentToolOutput,
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
    stream_idle_timeout_ms: Option<u64>,
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
    is_error: bool,
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

type PiAgentBufferedRun = BufferedNeutralRun<PiAgentTransportMetrics, ()>;
pub(crate) type PiAgentBufferedRunRegistry =
    BufferedNeutralRunRegistry<PiAgentTransportMetrics, ()>;

fn brain_runtime_error_to_napi(error: BrainRuntimeError) -> napi::Error {
    let status = if error.is_invalid_argument() {
        napi::Status::InvalidArg
    } else {
        napi::Status::GenericFailure
    };
    napi::Error::new(status, error.to_string())
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
    let wake_id = input.wake_id;
    let wake_timeout_ms = input.config.wake_timeout_ms.unwrap_or(300_000);
    buffered_runs
        .insert(wake_id.clone(), PiAgentBufferedRun::new(wake_timeout_ms))
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
            if !run.terminal && run.is_timed_out() {
                run.terminal = true;
                run.error = Some(format!(
                    "pi-agent buffered wake {wake_id} exceeded {}ms timeout",
                    run.wake_timeout_ms
                ));
                run.record_transition();
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
            if !items.is_empty() {
                run.record_transition();
            }
            let tool_requests = run.drain_pending_tool_requests();
            let terminal = run.terminal && run.items.is_empty();
            let output = json!({
                "wake_id": wake_id,
                "items": items,
                "tool_requests": tool_requests,
                "terminal": terminal,
                "transport_metrics": terminal.then(|| run.transport_metrics.clone()).flatten(),
                "error": terminal.then(|| run.error.clone()).flatten(),
                "cancellation": terminal.then(|| run.cancellation.clone()).flatten(),
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
    buffered_runs
        .with_run_mut(&input.wake_id, |run| {
            run.submit_tool_output(
                input.call_id.clone(),
                BufferedNeutralToolOutput {
                    output: input.output,
                    is_error: input.is_error,
                },
            );
        })
        .map_err(brain_runtime_error_to_napi)?;
    serde_json::to_string(&json!({
        "ok": true,
        "wake_id": input.wake_id,
        "call_id": input.call_id,
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
            run.cancel(input.reason_code, input.summary);
            json!({
                "ok": true,
                "wake_id": input.wake_id,
                "cancelled": true,
                "terminal": run.terminal,
                "cancellation": run.cancellation.clone(),
            })
        })
        .map_err(brain_runtime_error_to_napi)?;
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
        if run.is_cancelled() {
            return;
        }
        match result {
            Ok(output) => {
                run.items.extend(output.stream);
                run.transport_metrics = Some(PiAgentTransportMetrics {
                    provider_request_count: output.provider_request_count,
                    tool_round_count: output.tool_round_count,
                });
            }
            Err(error) => {
                run.error = Some(error.to_string());
            }
        }
        run.terminal = true;
        run.record_transition();
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
            let client = fake_pi_agent_client(!descriptors.is_empty());
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
                chat_config.stream_idle_timeout_ms,
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
        stream_idle_timeout_ms: config.stream_idle_timeout_ms.unwrap_or(30_000),
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

fn fake_pi_agent_client(with_tool: bool) -> FakeChatCompletionsClient {
    if !with_tool {
        return FakeChatCompletionsClient::new([Ok(vec![
            ChatCompletionsEvent::ContentDelta("pi-agent Rust bridge wake completed".to_string()),
            ChatCompletionsEvent::Finished {
                finish_reason: Some("stop".to_string()),
            },
        ])]);
    }
    FakeChatCompletionsClient::new([
        Ok(vec![
            ChatCompletionsEvent::ToolCallFinished(PendingChatFunctionCall {
                index: 0,
                id: Some("fake-pi-call".to_string()),
                name: "echo_tool".to_string(),
                arguments_json: "{}".to_string(),
            }),
            ChatCompletionsEvent::Finished {
                finish_reason: Some("tool_calls".to_string()),
            },
        ]),
        Ok(vec![
            ChatCompletionsEvent::ContentDelta("pi-agent Rust bridge wake completed".to_string()),
            ChatCompletionsEvent::Finished {
                finish_reason: Some("stop".to_string()),
            },
        ]),
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
                run.queue_pending_tool_request(request);
            });
            if queued.is_err() {
                return PiAgentToolOutput::error(format!(
                    "pi-agent buffered wake {} disappeared before tool request {}",
                    self.wake_id, call_id
                ));
            }
        }

        loop {
            let result = self.buffered_runs.with_run_mut(&self.wake_id, |run| {
                if let Some(cancellation) = run.cancellation.clone() {
                    return PiAgentToolOutput::cancelled(format!(
                        "pi-agent buffered wake {} cancelled before tool output {}: {}",
                        self.wake_id, call_id, cancellation.summary
                    ));
                }
                if let Some(output) = run.take_submitted_tool_output(&call_id) {
                    return if output.is_error {
                        PiAgentToolOutput::error(output.output)
                    } else {
                        PiAgentToolOutput::ok(output.output)
                    };
                }
                if run.terminal {
                    return PiAgentToolOutput::error(format!(
                        "pi-agent buffered wake {} ended before tool output {}",
                        self.wake_id, call_id
                    ));
                }
                if run.is_timed_out() {
                    run.terminal = true;
                    run.error = Some(format!(
                        "pi-agent buffered wake {} exceeded {}ms timeout while waiting for tool output {}",
                        self.wake_id, run.wake_timeout_ms, call_id
                    ));
                    run.record_transition();
                    return PiAgentToolOutput::timed_out(
                        run.error
                            .clone()
                            .unwrap_or_else(|| "pi-agent buffered wake timed out".to_string()),
                    );
                }
                PiAgentToolOutput::ok("")
            });
            match result {
                Ok(output) if !output.output.is_empty() || output.is_error => return output,
                Err(_) => {
                    return PiAgentToolOutput::error(format!(
                        "pi-agent buffered wake {} disappeared before tool output {}",
                        self.wake_id, call_id
                    ));
                }
                Ok(_) => {}
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    }
}
