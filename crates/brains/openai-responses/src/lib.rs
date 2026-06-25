//! Direct Rust scaffold for the OpenAI Responses brain module.
//!
//! This crate intentionally stays behind the language-neutral wake/stream
//! contract. It owns provider request/event shapes and fake-client tests, but
//! it does not reach into Rusty Crew coordination internals.

use rusty_crew_core_bridge_api::{BrainWakeStream, BrainWakeStreamProducer};
use rusty_crew_core_protocol::{
    AgentMessage, BodyState, BrainAction, BrainActionBatch, BrainEvent, BrainEventEnvelope,
    BrainProviderStatusLevel, BrainWakeFailure, BrainWakeProviderStateInput,
    BrainWakeProviderStateOutput, BrainWakeProviderStateUpdate, BrainWakeRequest,
    BrainWakeStreamItem, CompletionPacket, CompletionStatus, CoreError, CoreErrorKind, CoreEvent,
    CoreResult, ExternalEventPayload, ProviderStateAbsenceReason, ProviderStateMode,
    ToolCallMetadata, ToolCallPolicyMetadata, ToolCallSource,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};

pub const MODULE_ID: &str = "openai-responses";
pub const REPLAY_STRATEGY_ID: &str = "replay";
pub const PROVIDER_STATE_PAYLOAD_VERSION: &str = "openai-responses-state-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponsesBrainConfig {
    pub model: String,
    pub instructions: Option<String>,
    pub tool_choice: ResponsesToolChoice,
    pub parallel_tool_calls: bool,
    pub reasoning: Option<ResponsesReasoningConfig>,
    pub text: Option<ResponsesTextConfig>,
    pub include: Vec<String>,
    pub service_tier: Option<String>,
    pub prompt_cache_key: Option<String>,
    pub stream_idle_timeout_ms: u64,
}

impl ResponsesBrainConfig {
    pub fn replay(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            instructions: None,
            tool_choice: ResponsesToolChoice::Auto,
            parallel_tool_calls: true,
            reasoning: None,
            text: None,
            include: Vec::new(),
            service_tier: None,
            prompt_cache_key: None,
            stream_idle_timeout_ms: 30_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponsesToolChoice {
    Auto,
    None,
    Function { name: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponsesReasoningConfig {
    pub effort: Option<String>,
    pub summary: Option<String>,
    pub include_encrypted_content: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponsesTextConfig {
    pub verbosity: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NeutralBrainTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponsesRequest {
    pub model: String,
    pub instructions: Option<String>,
    pub input: Vec<ResponsesInputItem>,
    pub tools: Vec<ResponsesToolDescriptor>,
    pub tool_choice: Value,
    pub parallel_tool_calls: bool,
    pub reasoning: Option<Value>,
    pub stream: bool,
    pub include: Vec<String>,
    pub service_tier: Option<String>,
    pub prompt_cache_key: Option<String>,
    pub text: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponsesInputItem {
    UserMessage {
        content: String,
    },
    AssistantMessage {
        content: String,
    },
    Reasoning {
        id: Option<String>,
        summary: Option<String>,
        encrypted_content: Option<String>,
    },
    FunctionCall {
        id: Option<String>,
        call_id: String,
        name: String,
        arguments: String,
    },
    FunctionCallOutput {
        call_id: String,
        output: String,
        is_error: bool,
    },
    ReplayHint {
        raw_json: Value,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponsesToolDescriptor {
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

pub struct ResponsesRequestBuilder {
    config: ResponsesBrainConfig,
    tools: Vec<NeutralBrainTool>,
}

impl ResponsesRequestBuilder {
    pub fn new(config: ResponsesBrainConfig) -> Self {
        Self {
            config,
            tools: Vec::new(),
        }
    }

    pub fn tools(mut self, tools: Vec<NeutralBrainTool>) -> Self {
        self.tools = tools;
        self
    }

    pub fn build(
        &self,
        wake: &BrainWakeRequest,
        provider_state: Option<&BrainWakeProviderStateInput>,
        history: ResponsesReplayProjection,
        continuation_items: Vec<ResponsesInputItem>,
    ) -> ResponsesRequest {
        let mut input = history.input_items;
        input.extend(provider_replay_items(provider_state));
        input.extend(history.replay_hints);
        input.extend(continuation_items);
        if input.is_empty() {
            input.push(ResponsesInputItem::UserMessage {
                content: format!("wake {} has no Rust-owned history yet", wake.wake_id),
            });
        }
        ResponsesRequest {
            model: self.config.model.clone(),
            instructions: self.config.instructions.clone(),
            input,
            tools: self.tools.iter().map(adapt_neutral_tool).collect(),
            tool_choice: match &self.config.tool_choice {
                ResponsesToolChoice::Auto => json!("auto"),
                ResponsesToolChoice::None => json!("none"),
                ResponsesToolChoice::Function { name } => {
                    json!({"type": "function", "name": name})
                }
            },
            parallel_tool_calls: self.config.parallel_tool_calls,
            reasoning: self.config.reasoning.as_ref().map(|reasoning| {
                json!({
                    "effort": reasoning.effort,
                    "summary": reasoning.summary,
                    "include_encrypted_content": reasoning.include_encrypted_content
                })
            }),
            stream: true,
            include: self.config.include.clone(),
            service_tier: self.config.service_tier.clone(),
            prompt_cache_key: self.config.prompt_cache_key.clone(),
            text: self
                .config
                .text
                .as_ref()
                .map(|text| json!({"verbosity": text.verbosity})),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResponsesReplayProjection {
    pub input_items: Vec<ResponsesInputItem>,
    pub replay_hints: Vec<ResponsesInputItem>,
}

impl ResponsesReplayProjection {
    pub fn from_body_state(body: &BodyState) -> Self {
        let mut input_items = Vec::new();
        let mut seen_messages = Vec::new();

        for event in &body.recent_events {
            match event {
                CoreEvent::AgentMessageRouted { message } => {
                    push_message_item(&mut input_items, &mut seen_messages, &body.session, message);
                }
                CoreEvent::ExternalEventInjected { event } => match &event.payload {
                    ExternalEventPayload::HumanMessage { from, text } => {
                        input_items.push(ResponsesInputItem::UserMessage {
                            content: format!("{from}: {text}"),
                        });
                    }
                    ExternalEventPayload::ChannelMessage(payload) => {
                        input_items.push(ResponsesInputItem::UserMessage {
                            content: format!("{}: {}", payload.from, payload.text),
                        });
                    }
                    _ => {}
                },
                CoreEvent::CompletionPacketDelivered { packet } => {
                    input_items.push(ResponsesInputItem::UserMessage {
                        content: format!(
                            "delegated session {} reported {:?}: {}",
                            packet.session_id.0.as_str(),
                            packet.status,
                            packet.summary
                        ),
                    });
                }
                _ => {}
            }
        }

        for message in &body.pending_messages {
            push_message_item(&mut input_items, &mut seen_messages, &body.session, message);
        }

        for completion in &body.child_completions {
            input_items.push(ResponsesInputItem::UserMessage {
                content: format!(
                    "delegated run {} from {} reported {:?}: {}",
                    completion.run_id.0.as_str(),
                    completion.child_session_id.0.as_str(),
                    completion.packet.status,
                    completion.packet.summary
                ),
            });
        }

        Self {
            input_items,
            replay_hints: Vec::new(),
        }
    }
}

fn push_message_item(
    input_items: &mut Vec<ResponsesInputItem>,
    seen_messages: &mut Vec<(String, String, String, Option<String>)>,
    session: &rusty_crew_core_protocol::SessionState,
    message: &AgentMessage,
) {
    let key = (
        message.from.0.clone(),
        message.to.0.clone(),
        message.body.clone(),
        message.correlation_id.clone(),
    );
    if seen_messages.contains(&key) {
        return;
    }
    seen_messages.push(key);
    if message.from == session.agent_id {
        input_items.push(ResponsesInputItem::AssistantMessage {
            content: message.body.clone(),
        });
    } else {
        input_items.push(ResponsesInputItem::UserMessage {
            content: format!("{}: {}", message.from.0, message.body),
        });
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAiResponsesProviderStateV1 {
    pub kind: String,
    pub strategy_id: String,
    pub payload_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_completed_response: Option<OpenAiResponsesCompletedResponseRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replay_hints: Option<OpenAiResponsesReplayHints>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAiResponsesCompletedResponseRecord {
    pub response_id: String,
    pub output_items: Vec<OpenAiResponseOutputItemRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_usage: Option<ResponsesTokenUsage>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAiResponsesReplayHints {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasoning_items: Vec<OpenAiResponseOutputItemRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_cache_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_item_watermark: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAiResponseOutputItemRecord {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    pub item_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
    pub raw_json: Value,
}

fn provider_replay_items(
    provider_state: Option<&BrainWakeProviderStateInput>,
) -> Vec<ResponsesInputItem> {
    let Some(state) = provider_state else {
        return Vec::new();
    };
    let Ok(payload) =
        serde_json::from_value::<OpenAiResponsesProviderStateV1>(state.payload.clone())
    else {
        return vec![ResponsesInputItem::ReplayHint {
            raw_json: state.payload.clone(),
        }];
    };

    let mut items = Vec::new();
    if let Some(completed) = payload.last_completed_response {
        for record in completed.output_items {
            if let Some(item) = replay_item_from_record(record) {
                items.push(item);
            }
        }
    }
    if let Some(hints) = payload.replay_hints {
        for record in hints.reasoning_items {
            if let Some(item) = replay_item_from_record(record) {
                items.push(item);
            }
        }
        if hints.prompt_cache_key.is_some() || hints.provider_item_watermark.is_some() {
            items.push(ResponsesInputItem::ReplayHint {
                raw_json: json!({
                    "promptCacheKey": hints.prompt_cache_key,
                    "providerItemWatermark": hints.provider_item_watermark,
                }),
            });
        }
    }
    items
}

fn replay_item_from_record(record: OpenAiResponseOutputItemRecord) -> Option<ResponsesInputItem> {
    let output = serde_json::from_value::<ResponsesOutputItem>(record.raw_json).ok()?;
    match output {
        ResponsesOutputItem::Message { text, .. } => {
            Some(ResponsesInputItem::AssistantMessage { content: text })
        }
        ResponsesOutputItem::Reasoning {
            id,
            summary,
            encrypted_content,
        } => Some(ResponsesInputItem::Reasoning {
            id,
            summary,
            encrypted_content,
        }),
        ResponsesOutputItem::FunctionCall {
            id,
            call_id,
            name,
            arguments,
        } => Some(ResponsesInputItem::FunctionCall {
            id,
            call_id,
            name,
            arguments,
        }),
        ResponsesOutputItem::FunctionCallOutput {
            call_id,
            output,
            is_error,
        } => Some(ResponsesInputItem::FunctionCallOutput {
            call_id,
            output,
            is_error,
        }),
        ResponsesOutputItem::Other { raw_json, .. } => {
            Some(ResponsesInputItem::ReplayHint { raw_json })
        }
    }
}

fn output_record_from_item(item: &ResponsesOutputItem) -> OpenAiResponseOutputItemRecord {
    let (item_id, item_type, call_id) = match item {
        ResponsesOutputItem::Message { id, .. } => (id.clone(), "message".to_string(), None),
        ResponsesOutputItem::Reasoning { id, .. } => (id.clone(), "reasoning".to_string(), None),
        ResponsesOutputItem::FunctionCall { id, call_id, .. } => (
            id.clone(),
            "function_call".to_string(),
            Some(call_id.clone()),
        ),
        ResponsesOutputItem::FunctionCallOutput { call_id, .. } => (
            None,
            "function_call_output".to_string(),
            Some(call_id.clone()),
        ),
        ResponsesOutputItem::Other { item_type, .. } => (None, item_type.clone(), None),
    };
    OpenAiResponseOutputItemRecord {
        item_id,
        item_type,
        call_id,
        raw_json: serde_json::to_value(item).unwrap_or_else(|_| json!({})),
    }
}

fn adapt_neutral_tool(tool: &NeutralBrainTool) -> ResponsesToolDescriptor {
    ResponsesToolDescriptor {
        kind: "function".to_string(),
        name: tool.name.clone(),
        description: tool.description.clone(),
        parameters: tool.input_schema.clone(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponsesRawStreamEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default)]
    pub response_id: Option<String>,
    #[serde(default)]
    pub item: Option<ResponsesOutputItem>,
    #[serde(default)]
    pub item_id: Option<String>,
    #[serde(default)]
    pub call_id: Option<String>,
    #[serde(default)]
    pub delta: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub usage: Option<ResponsesTokenUsage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponsesOutputItem {
    Message {
        id: Option<String>,
        text: String,
    },
    Reasoning {
        id: Option<String>,
        summary: Option<String>,
        encrypted_content: Option<String>,
    },
    FunctionCall {
        id: Option<String>,
        call_id: String,
        name: String,
        arguments: String,
    },
    FunctionCallOutput {
        call_id: String,
        output: String,
        is_error: bool,
    },
    Other {
        item_type: String,
        raw_json: Value,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponsesTokenUsage {
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_output_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponsesEvent {
    TextDelta(String),
    ReasoningDelta(String),
    OutputItemAdded(ResponsesOutputItem),
    OutputItemDone(ResponsesOutputItem),
    Completed {
        response_id: String,
        usage: Option<ResponsesTokenUsage>,
    },
    Failed(String),
    Incomplete(String),
}

pub fn process_responses_event(
    raw: ResponsesRawStreamEvent,
) -> Result<ResponsesEvent, ResponsesStreamError> {
    match raw.event_type.as_str() {
        "response.output_text.delta" => Ok(ResponsesEvent::TextDelta(
            raw.delta
                .ok_or(ResponsesStreamError::MissingField("delta"))?,
        )),
        "response.reasoning.delta" => Ok(ResponsesEvent::ReasoningDelta(
            raw.delta
                .ok_or(ResponsesStreamError::MissingField("delta"))?,
        )),
        "response.output_item.added" => Ok(ResponsesEvent::OutputItemAdded(
            raw.item.ok_or(ResponsesStreamError::MissingField("item"))?,
        )),
        "response.output_item.done" => Ok(ResponsesEvent::OutputItemDone(
            raw.item.ok_or(ResponsesStreamError::MissingField("item"))?,
        )),
        "response.completed" => Ok(ResponsesEvent::Completed {
            response_id: raw
                .response_id
                .ok_or(ResponsesStreamError::MissingField("response_id"))?,
            usage: raw.usage,
        }),
        "response.failed" => Ok(ResponsesEvent::Failed(
            raw.message
                .unwrap_or_else(|| "provider response failed".to_string()),
        )),
        "response.incomplete" => Ok(ResponsesEvent::Incomplete(
            raw.message
                .unwrap_or_else(|| "provider response incomplete".to_string()),
        )),
        other => Err(ResponsesStreamError::UnknownEvent(other.to_string())),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResponsesStreamError {
    #[error("provider stream missing {0}")]
    MissingField(&'static str),
    #[error("unknown provider event {0}")]
    UnknownEvent(String),
    #[error("provider stream idle timeout")]
    IdleTimeout,
    #[error("provider stream closed before response.completed")]
    ClosedBeforeComplete,
    #[error("provider response failed: {0}")]
    ResponseFailed(String),
    #[error("provider response incomplete: {0}")]
    ResponseIncomplete(String),
    #[error("function call output call_id mismatch: expected {expected}, got {actual}")]
    FunctionCallOutputMismatch { expected: String, actual: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingResponsesFunctionCall {
    pub provider_item_id: Option<String>,
    pub call_id: String,
    pub name: String,
    pub arguments_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NeutralToolOutput {
    pub output: String,
    pub is_error: bool,
}

pub trait ResponsesClient {
    fn stream(
        &mut self,
        request: ResponsesRequest,
    ) -> Result<Vec<ResponsesEvent>, ResponsesStreamError>;
}

pub trait NeutralToolExecutor {
    fn execute(&self, call: &PendingResponsesFunctionCall) -> NeutralToolOutput;
}

pub struct ResponsesReplayBrain<C, T> {
    client: C,
    tools: T,
    request_builder: ResponsesRequestBuilder,
    max_continuations: usize,
}

impl<C, T> ResponsesReplayBrain<C, T>
where
    C: ResponsesClient,
    T: NeutralToolExecutor,
{
    pub fn new(
        client: C,
        tools: T,
        config: ResponsesBrainConfig,
        descriptors: Vec<NeutralBrainTool>,
    ) -> Self {
        Self {
            client,
            tools,
            request_builder: ResponsesRequestBuilder::new(config).tools(descriptors),
            max_continuations: 8,
        }
    }

    pub fn strategy_metadata() -> (String, String, ProviderStateMode) {
        (
            MODULE_ID.to_string(),
            REPLAY_STRATEGY_ID.to_string(),
            ProviderStateMode::Optional,
        )
    }

    pub fn wake(&mut self, request: BrainWakeRequest) -> CoreResult<ResponsesBrainWakeResult> {
        self.wake_with_history(request, ResponsesReplayProjection::default())
    }

    pub fn wake_with_history(
        &mut self,
        request: BrainWakeRequest,
        history: ResponsesReplayProjection,
    ) -> CoreResult<ResponsesBrainWakeResult> {
        let mut items = vec![event(&request, BrainEvent::Started)];
        if let Some(absence) = &request.provider_state_absence {
            if matches!(
                absence,
                ProviderStateAbsenceReason::Missing
                    | ProviderStateAbsenceReason::Expired
                    | ProviderStateAbsenceReason::Invalidated
                    | ProviderStateAbsenceReason::LoadFailed
            ) {
                items.push(event(
                    &request,
                    BrainEvent::ProviderStatus {
                        level: BrainProviderStatusLevel::Info,
                        message: format!(
                            "responses replay starting without provider state: {absence:?}"
                        ),
                        metadata_json: None,
                    },
                ));
            }
        }

        let mut continuation_items = Vec::new();
        let mut committed_output_items = Vec::new();
        let mut last_response_id = None;
        let mut last_usage = None;
        let base_history = history;

        for _ in 0..=self.max_continuations {
            let provider_request = self.request_builder.build(
                &request,
                request.provider_state.as_ref(),
                base_history.clone(),
                continuation_items.clone(),
            );
            let events = match self.client.stream(provider_request) {
                Ok(events) => events,
                Err(error) => return Ok(failed_result(&request, items, error)),
            };
            let mut completed = false;
            let mut pending_calls = Vec::new();
            for provider_event in events {
                match provider_event {
                    ResponsesEvent::TextDelta(text) => {
                        items.push(event(&request, BrainEvent::TextDelta { text }));
                    }
                    ResponsesEvent::ReasoningDelta(delta) => {
                        items.push(event(
                            &request,
                            BrainEvent::ProviderStatus {
                                level: BrainProviderStatusLevel::Info,
                                message: "reasoning delta".to_string(),
                                metadata_json: Some(json!({"delta": delta}).to_string()),
                            },
                        ));
                    }
                    ResponsesEvent::OutputItemAdded(output) => {
                        committed_output_items.push(output);
                    }
                    ResponsesEvent::OutputItemDone(output) => match output {
                        ResponsesOutputItem::FunctionCall {
                            id,
                            call_id,
                            name,
                            arguments,
                        } => {
                            committed_output_items.push(ResponsesOutputItem::FunctionCall {
                                id: id.clone(),
                                call_id: call_id.clone(),
                                name: name.clone(),
                                arguments: arguments.clone(),
                            });
                            pending_calls.push(PendingResponsesFunctionCall {
                                provider_item_id: id,
                                call_id,
                                name,
                                arguments_json: arguments,
                            });
                        }
                        other => committed_output_items.push(other),
                    },
                    ResponsesEvent::Completed { response_id, usage } => {
                        completed = true;
                        last_response_id = Some(response_id);
                        last_usage = usage;
                    }
                    ResponsesEvent::Failed(message) => {
                        return Ok(failed_result(
                            &request,
                            items,
                            ResponsesStreamError::ResponseFailed(message),
                        ));
                    }
                    ResponsesEvent::Incomplete(message) => {
                        return Ok(failed_result(
                            &request,
                            items,
                            ResponsesStreamError::ResponseIncomplete(message),
                        ));
                    }
                }
            }
            if !completed {
                return Ok(failed_result(
                    &request,
                    items,
                    ResponsesStreamError::ClosedBeforeComplete,
                ));
            }
            if pending_calls.is_empty() {
                items.push(event(&request, BrainEvent::Finished));
                let batch = BrainActionBatch {
                    wake_id: request.wake_id.clone(),
                    session_id: request.session_id.clone(),
                    actions: vec![BrainAction::DeliverCompletion {
                        packet: CompletionPacket {
                            session_id: request.session_id.clone(),
                            status: CompletionStatus::Completed,
                            summary: "responses replay wake completed".to_string(),
                        },
                    }],
                };
                items.push(BrainWakeStreamItem::actions(batch));
                let provider_state = provider_state_output(
                    &request,
                    last_response_id.unwrap_or_else(|| "unknown-response".to_string()),
                    committed_output_items,
                    last_usage,
                );
                return Ok(ResponsesBrainWakeResult {
                    stream: BrainWakeStream::from_items(items),
                    provider_state: Some(provider_state),
                });
            }
            for call in pending_calls {
                items.push(event(
                    &request,
                    BrainEvent::ToolCallStarted {
                        tool_name: call.name.clone(),
                        metadata: Some(tool_metadata(&call)),
                    },
                ));
                let output = self.tools.execute(&call);
                items.push(event(
                    &request,
                    BrainEvent::ToolCallFinished {
                        tool_name: call.name.clone(),
                        is_error: output.is_error,
                        metadata: Some(tool_metadata(&call)),
                    },
                ));
                continuation_items.push(ResponsesInputItem::FunctionCallOutput {
                    call_id: call.call_id.clone(),
                    output: output.output.clone(),
                    is_error: output.is_error,
                });
                committed_output_items.push(ResponsesOutputItem::FunctionCallOutput {
                    call_id: call.call_id,
                    output: output.output,
                    is_error: output.is_error,
                });
            }
        }

        Ok(failed_result(
            &request,
            items,
            ResponsesStreamError::IdleTimeout,
        ))
    }
}

impl<C, T> BrainWakeStreamProducer for ResponsesReplayBrain<C, T>
where
    C: ResponsesClient,
    T: NeutralToolExecutor,
{
    fn wake_stream(&self, _request: BrainWakeRequest) -> CoreResult<BrainWakeStream> {
        Err(CoreError::new(
            CoreErrorKind::BrainUnavailable,
            "ResponsesReplayBrain::wake_stream requires mutable fake/live client state; call wake() on the module scaffold",
        ))
    }
}

pub struct ResponsesBrainWakeResult {
    pub stream: BrainWakeStream,
    pub provider_state: Option<BrainWakeProviderStateOutput>,
}

fn provider_state_output(
    request: &BrainWakeRequest,
    response_id: String,
    output_items: Vec<ResponsesOutputItem>,
    usage: Option<ResponsesTokenUsage>,
) -> BrainWakeProviderStateOutput {
    let output_records: Vec<_> = output_items.iter().map(output_record_from_item).collect();
    let payload = OpenAiResponsesProviderStateV1 {
        kind: MODULE_ID.to_string(),
        strategy_id: REPLAY_STRATEGY_ID.to_string(),
        payload_version: PROVIDER_STATE_PAYLOAD_VERSION.to_string(),
        last_completed_response: Some(OpenAiResponsesCompletedResponseRecord {
            response_id,
            output_items: output_records,
            token_usage: usage,
        }),
        replay_hints: None,
    };
    BrainWakeProviderStateOutput::Replace {
        state: BrainWakeProviderStateUpdate {
            module_id: MODULE_ID.to_string(),
            strategy_id: REPLAY_STRATEGY_ID.to_string(),
            profile_fingerprint: request
                .provider_state
                .as_ref()
                .map(|state| state.profile_fingerprint.clone())
                .unwrap_or_else(|| "profile-fingerprint".to_string()),
            provider_fingerprint: request
                .provider_state
                .as_ref()
                .map(|state| state.provider_fingerprint.clone())
                .unwrap_or_else(|| "provider-fingerprint".to_string()),
            payload_version: PROVIDER_STATE_PAYLOAD_VERSION.to_string(),
            payload: serde_json::to_value(payload).unwrap_or_else(|_| json!({})),
            ttl_ms: Some(24 * 60 * 60 * 1000),
        },
    }
}

fn failed_result(
    request: &BrainWakeRequest,
    mut items: Vec<BrainWakeStreamItem>,
    error: ResponsesStreamError,
) -> ResponsesBrainWakeResult {
    items.push(BrainWakeStreamItem::wake_failed(BrainWakeFailure {
        wake_id: request.wake_id.clone(),
        session_id: request.session_id.clone(),
        kind: CoreErrorKind::BrainUnavailable,
        message: error.to_string(),
    }));
    ResponsesBrainWakeResult {
        stream: BrainWakeStream::from_items(items),
        provider_state: None,
    }
}

fn event(request: &BrainWakeRequest, event: BrainEvent) -> BrainWakeStreamItem {
    BrainWakeStreamItem::event(BrainEventEnvelope {
        wake_id: request.wake_id.clone(),
        session_id: request.session_id.clone(),
        event,
    })
}

fn tool_metadata(call: &PendingResponsesFunctionCall) -> ToolCallMetadata {
    ToolCallMetadata {
        source: ToolCallSource::Local,
        adapter_id: None,
        binding_id: None,
        server_names: Vec::new(),
        profile_id: None,
        tool_profile_key: None,
        source_tool_name: Some(call.name.clone()),
        catalog_revision: Some("openai-responses-fake".to_string()),
        policy: Some(ToolCallPolicyMetadata {
            allowed: Some(true),
            denial_reason: None,
            timeout_ms: None,
            cancelled: None,
            archive_cleanup: None,
        }),
    }
}

#[derive(Debug, Default)]
pub struct FakeResponsesClient {
    scripts: VecDeque<Result<Vec<ResponsesEvent>, ResponsesStreamError>>,
    requests: Vec<ResponsesRequest>,
    expected_function_outputs: VecDeque<String>,
}

impl FakeResponsesClient {
    pub fn new(scripts: Vec<Result<Vec<ResponsesEvent>, ResponsesStreamError>>) -> Self {
        Self {
            scripts: scripts.into(),
            requests: Vec::new(),
            expected_function_outputs: VecDeque::new(),
        }
    }

    pub fn expect_function_output(mut self, call_id: impl Into<String>) -> Self {
        self.expected_function_outputs.push_back(call_id.into());
        self
    }

    pub fn requests(&self) -> &[ResponsesRequest] {
        &self.requests
    }
}

impl ResponsesClient for FakeResponsesClient {
    fn stream(
        &mut self,
        request: ResponsesRequest,
    ) -> Result<Vec<ResponsesEvent>, ResponsesStreamError> {
        let actual = request.input.iter().find_map(|item| match item {
            ResponsesInputItem::FunctionCallOutput { call_id, .. } => Some(call_id.clone()),
            _ => None,
        });
        if actual.is_some() {
            let expected = self.expected_function_outputs.pop_front().ok_or_else(|| {
                ResponsesStreamError::FunctionCallOutputMismatch {
                    expected: "<none>".to_string(),
                    actual: actual.clone().unwrap_or_else(|| "<missing>".to_string()),
                }
            })?;
            if actual.as_deref() != Some(expected.as_str()) {
                return Err(ResponsesStreamError::FunctionCallOutputMismatch {
                    expected,
                    actual: actual.unwrap_or_else(|| "<missing>".to_string()),
                });
            }
        }
        self.requests.push(request);
        self.scripts
            .pop_front()
            .unwrap_or(Err(ResponsesStreamError::ClosedBeforeComplete))
    }
}

#[derive(Debug, Default)]
pub struct MapToolExecutor {
    outputs: HashMap<String, NeutralToolOutput>,
}

impl MapToolExecutor {
    pub fn new(outputs: impl IntoIterator<Item = (String, NeutralToolOutput)>) -> Self {
        Self {
            outputs: outputs.into_iter().collect(),
        }
    }
}

impl NeutralToolExecutor for MapToolExecutor {
    fn execute(&self, call: &PendingResponsesFunctionCall) -> NeutralToolOutput {
        self.outputs
            .get(&call.name)
            .cloned()
            .unwrap_or_else(|| NeutralToolOutput {
                output: format!("tool {} is unavailable", call.name),
                is_error: true,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_crew_core_protocol::{
        AgentId, BodyDeltaPolicy, BrainImplementationHandle, DeltaQueueOwner, MidTurnDeltaMode,
        ProfileId, ResourceLimits, SessionHandle, SessionId, SessionKind, SessionState,
        SessionStatus, ToolProfile,
    };
    use rusty_crew_core_protocol::{CoreEvent, RuntimeBufferHandle};

    #[test]
    fn request_builder_adapts_neutral_tools_and_provider_state() {
        let config = ResponsesBrainConfig {
            instructions: Some("be useful".to_string()),
            reasoning: Some(ResponsesReasoningConfig {
                effort: Some("medium".to_string()),
                summary: Some("auto".to_string()),
                include_encrypted_content: true,
            }),
            text: Some(ResponsesTextConfig {
                verbosity: Some("low".to_string()),
            }),
            include: vec!["reasoning.encrypted_content".to_string()],
            service_tier: Some("default".to_string()),
            prompt_cache_key: Some("profile-cache".to_string()),
            ..ResponsesBrainConfig::replay("gpt-5")
        };
        let builder = ResponsesRequestBuilder::new(config).tools(vec![NeutralBrainTool {
            name: "lookup".to_string(),
            description: "Look up data".to_string(),
            input_schema: json!({"type": "object"}),
        }]);
        let state = provider_state(json!({"replayHints": {"watermark": "abc"}}));
        let request = builder.build(
            &wake_request(Some(state.clone()), None),
            Some(&state),
            ResponsesReplayProjection {
                input_items: vec![ResponsesInputItem::UserMessage {
                    content: "from history".to_string(),
                }],
                replay_hints: Vec::new(),
            },
            Vec::new(),
        );

        assert_eq!(request.model, "gpt-5");
        assert_eq!(request.tools[0].name, "lookup");
        assert_eq!(request.input.len(), 2);
        assert_eq!(request.reasoning.as_ref().unwrap()["effort"], "medium");
        assert_eq!(request.text.as_ref().unwrap()["verbosity"], "low");
        assert!(request.stream);
    }

    #[test]
    fn body_history_projects_messages_without_requiring_provider_state() {
        let body = body_state(
            vec![agent_message(
                "human",
                "responses-agent",
                "hello",
                Some("c1"),
            )],
            vec![
                CoreEvent::AgentMessageRouted {
                    message: agent_message("human", "responses-agent", "hello", Some("c1")),
                },
                CoreEvent::AgentMessageRouted {
                    message: agent_message("responses-agent", "human", "reply", None),
                },
            ],
        );

        let projection = ResponsesReplayProjection::from_body_state(&body);

        assert_eq!(
            projection.input_items,
            vec![
                ResponsesInputItem::UserMessage {
                    content: "human: hello".to_string(),
                },
                ResponsesInputItem::AssistantMessage {
                    content: "reply".to_string(),
                },
            ]
        );
    }

    #[test]
    fn provider_state_replays_typed_reasoning_function_call_and_output_items() {
        let builder = ResponsesRequestBuilder::new(ResponsesBrainConfig::replay("gpt-5"));
        let state = provider_state(provider_state_payload(
            "resp-typed",
            vec![
                ResponsesOutputItem::Reasoning {
                    id: Some("reasoning-1".to_string()),
                    summary: Some("kept as reasoning".to_string()),
                    encrypted_content: Some("opaque".to_string()),
                },
                ResponsesOutputItem::FunctionCall {
                    id: Some("call-item-1".to_string()),
                    call_id: "call-1".to_string(),
                    name: "lookup".to_string(),
                    arguments: "{\"q\":\"rust\"}".to_string(),
                },
                ResponsesOutputItem::FunctionCallOutput {
                    call_id: "call-1".to_string(),
                    output: "found rust".to_string(),
                    is_error: false,
                },
                ResponsesOutputItem::Message {
                    id: Some("msg-1".to_string()),
                    text: "answer".to_string(),
                },
            ],
        ));
        let request = builder.build(
            &wake_request(Some(state.clone()), None),
            Some(&state),
            ResponsesReplayProjection {
                input_items: vec![ResponsesInputItem::UserMessage {
                    content: "human: continue".to_string(),
                }],
                replay_hints: Vec::new(),
            },
            Vec::new(),
        );

        assert!(request.input.iter().any(|item| matches!(
            item,
            ResponsesInputItem::Reasoning {
                encrypted_content: Some(value),
                ..
            } if value == "opaque"
        )));
        assert!(request.input.iter().any(|item| matches!(
            item,
            ResponsesInputItem::FunctionCall {
                call_id,
                name,
                ..
            } if call_id == "call-1" && name == "lookup"
        )));
        assert!(request.input.iter().any(|item| matches!(
            item,
            ResponsesInputItem::FunctionCallOutput {
                call_id,
                output,
                is_error,
            } if call_id == "call-1" && output == "found rust" && !is_error
        )));
        assert!(request.input.iter().any(|item| matches!(
            item,
            ResponsesInputItem::AssistantMessage { content } if content == "answer"
        )));
    }

    #[test]
    fn expired_provider_state_recovers_from_rust_owned_history() {
        let mut brain = brain_with(
            FakeResponsesClient::new(vec![Ok(vec![ResponsesEvent::Completed {
                response_id: "resp-recovered".to_string(),
                usage: None,
            }])]),
            MapToolExecutor::default(),
        );
        let history = ResponsesReplayProjection::from_body_state(&body_state(
            vec![agent_message(
                "human",
                "responses-agent",
                "recover from history",
                None,
            )],
            Vec::new(),
        ));
        let result = brain
            .wake_with_history(
                wake_request(None, Some(ProviderStateAbsenceReason::Expired)),
                history.clone(),
            )
            .unwrap();
        let items = result.stream.drain_until_terminal().unwrap();

        assert!(items.iter().any(|item| matches!(
            item,
            BrainWakeStreamItem::Event { event } if matches!(&event.event, BrainEvent::ProviderStatus { message, .. } if message.contains("without provider state"))
        )));
        assert_eq!(brain.client.requests()[0].input, history.input_items);
        assert!(matches!(
            result.provider_state,
            Some(BrainWakeProviderStateOutput::Replace { .. })
        ));
    }

    #[test]
    fn fake_client_streams_text_reasoning_and_completion_action() {
        let mut brain = brain_with(
            FakeResponsesClient::new(vec![Ok(vec![
                ResponsesEvent::TextDelta("hello ".to_string()),
                ResponsesEvent::ReasoningDelta("thinking".to_string()),
                ResponsesEvent::OutputItemAdded(ResponsesOutputItem::Message {
                    id: Some("msg-1".to_string()),
                    text: "hello world".to_string(),
                }),
                ResponsesEvent::Completed {
                    response_id: "resp-1".to_string(),
                    usage: Some(usage()),
                },
            ])]),
            MapToolExecutor::default(),
        );

        let result = brain
            .wake(wake_request(
                None,
                Some(ProviderStateAbsenceReason::Missing),
            ))
            .unwrap();
        let items = result.stream.drain_until_terminal().unwrap();
        assert!(matches!(
            items.last(),
            Some(BrainWakeStreamItem::Actions { .. })
        ));
        assert!(items.iter().any(|item| matches!(
            item,
            BrainWakeStreamItem::Event { event } if matches!(&event.event, BrainEvent::TextDelta { text } if text == "hello ")
        )));
        assert!(items.iter().any(|item| matches!(
            item,
            BrainWakeStreamItem::Event { event } if matches!(&event.event, BrainEvent::ProviderStatus { message, .. } if message == "reasoning delta")
        )));
        assert!(matches!(
            result.provider_state,
            Some(BrainWakeProviderStateOutput::Replace { .. })
        ));
        let Some(BrainWakeProviderStateOutput::Replace { state }) = result.provider_state else {
            panic!("expected provider-state replacement");
        };
        let payload: OpenAiResponsesProviderStateV1 =
            serde_json::from_value(state.payload).unwrap();
        assert_eq!(
            payload.last_completed_response.unwrap().output_items[0].item_type,
            "message"
        );
    }

    #[test]
    fn function_call_continuation_requires_matching_call_id_and_emits_tool_events() {
        let client = FakeResponsesClient::new(vec![
            Ok(vec![
                ResponsesEvent::OutputItemDone(ResponsesOutputItem::FunctionCall {
                    id: Some("item-1".to_string()),
                    call_id: "call-1".to_string(),
                    name: "lookup".to_string(),
                    arguments: "{\"q\":\"rust\"}".to_string(),
                }),
                ResponsesEvent::Completed {
                    response_id: "resp-tool".to_string(),
                    usage: None,
                },
            ]),
            Ok(vec![
                ResponsesEvent::TextDelta("tool result used".to_string()),
                ResponsesEvent::Completed {
                    response_id: "resp-final".to_string(),
                    usage: None,
                },
            ]),
        ])
        .expect_function_output("call-1");
        let tools = MapToolExecutor::new([(
            "lookup".to_string(),
            NeutralToolOutput {
                output: "found rust".to_string(),
                is_error: false,
            },
        )]);
        let mut brain = brain_with(client, tools);
        let result = brain.wake(wake_request(None, None)).unwrap();
        let items = result.stream.drain_until_terminal().unwrap();

        assert!(items.iter().any(|item| matches!(
            item,
            BrainWakeStreamItem::Event { event } if matches!(&event.event, BrainEvent::ToolCallStarted { tool_name, .. } if tool_name == "lookup")
        )));
        assert!(items.iter().any(|item| matches!(
            item,
            BrainWakeStreamItem::Event { event } if matches!(&event.event, BrainEvent::ToolCallFinished { tool_name, is_error, .. } if tool_name == "lookup" && !is_error)
        )));
        assert!(matches!(
            result.provider_state,
            Some(BrainWakeProviderStateOutput::Replace { .. })
        ));
        let Some(BrainWakeProviderStateOutput::Replace { state }) = result.provider_state else {
            panic!("expected provider-state replacement");
        };
        let payload: OpenAiResponsesProviderStateV1 =
            serde_json::from_value(state.payload).unwrap();
        let records = payload.last_completed_response.unwrap().output_items;
        assert!(records.iter().any(|record| {
            record.item_type == "function_call" && record.call_id.as_deref() == Some("call-1")
        }));
        assert!(records.iter().any(|record| {
            record.item_type == "function_call_output"
                && record.call_id.as_deref() == Some("call-1")
        }));
    }

    #[test]
    fn failed_tool_call_is_reported_to_provider_and_stream() {
        let client = FakeResponsesClient::new(vec![
            Ok(vec![
                ResponsesEvent::OutputItemDone(ResponsesOutputItem::FunctionCall {
                    id: None,
                    call_id: "call-fail".to_string(),
                    name: "missing_tool".to_string(),
                    arguments: "{}".to_string(),
                }),
                ResponsesEvent::Completed {
                    response_id: "resp-tool".to_string(),
                    usage: None,
                },
            ]),
            Ok(vec![ResponsesEvent::Completed {
                response_id: "resp-final".to_string(),
                usage: None,
            }]),
        ])
        .expect_function_output("call-fail");
        let mut brain = brain_with(client, MapToolExecutor::default());
        let result = brain.wake(wake_request(None, None)).unwrap();
        let items = result.stream.drain_until_terminal().unwrap();
        assert!(items.iter().any(|item| matches!(
            item,
            BrainWakeStreamItem::Event { event } if matches!(&event.event, BrainEvent::ToolCallFinished { tool_name, is_error, .. } if tool_name == "missing_tool" && *is_error)
        )));
    }

    #[test]
    fn provider_failure_and_closed_stream_do_not_commit_provider_state() {
        for script in [
            Ok(vec![ResponsesEvent::Failed("rate limited".to_string())]),
            Ok(vec![ResponsesEvent::Incomplete("max output".to_string())]),
            Err(ResponsesStreamError::IdleTimeout),
            Ok(vec![ResponsesEvent::TextDelta("partial".to_string())]),
        ] {
            let mut brain = brain_with(
                FakeResponsesClient::new(vec![script]),
                MapToolExecutor::default(),
            );
            let result = brain.wake(wake_request(None, None)).unwrap();
            let items = result.stream.drain_until_terminal().unwrap();
            assert!(matches!(
                items.last(),
                Some(BrainWakeStreamItem::WakeFailed { .. })
            ));
            assert!(result.provider_state.is_none());
        }
    }

    #[test]
    fn fake_client_rejects_mismatched_function_call_output() {
        let mut client =
            FakeResponsesClient::new(vec![Ok(Vec::new())]).expect_function_output("expected-call");
        let request = ResponsesRequest {
            model: "gpt-5".to_string(),
            instructions: None,
            input: vec![ResponsesInputItem::FunctionCallOutput {
                call_id: "wrong-call".to_string(),
                output: "oops".to_string(),
                is_error: false,
            }],
            tools: Vec::new(),
            tool_choice: json!("auto"),
            parallel_tool_calls: true,
            reasoning: None,
            stream: true,
            include: Vec::new(),
            service_tier: None,
            prompt_cache_key: None,
            text: None,
        };
        assert!(matches!(
            client.stream(request),
            Err(ResponsesStreamError::FunctionCallOutputMismatch { .. })
        ));
    }

    #[test]
    fn raw_provider_events_map_to_internal_events() {
        assert_eq!(
            process_responses_event(raw_event("response.output_text.delta").delta("hello")),
            Ok(ResponsesEvent::TextDelta("hello".to_string()))
        );
        assert_eq!(
            process_responses_event(raw_event("response.reasoning.delta").delta("thinking")),
            Ok(ResponsesEvent::ReasoningDelta("thinking".to_string()))
        );
        assert_eq!(
            process_responses_event(raw_event("response.output_item.done").item(
                ResponsesOutputItem::FunctionCall {
                    id: Some("item-1".to_string()),
                    call_id: "call-1".to_string(),
                    name: "lookup".to_string(),
                    arguments: "{}".to_string(),
                }
            )),
            Ok(ResponsesEvent::OutputItemDone(
                ResponsesOutputItem::FunctionCall {
                    id: Some("item-1".to_string()),
                    call_id: "call-1".to_string(),
                    name: "lookup".to_string(),
                    arguments: "{}".to_string(),
                }
            ))
        );
        assert_eq!(
            process_responses_event(raw_event("response.completed").response_id("resp-1")),
            Ok(ResponsesEvent::Completed {
                response_id: "resp-1".to_string(),
                usage: None,
            })
        );
        assert!(matches!(
            process_responses_event(raw_event("response.output_text.delta")),
            Err(ResponsesStreamError::MissingField("delta"))
        ));
        assert!(matches!(
            process_responses_event(raw_event("response.unknown")),
            Err(ResponsesStreamError::UnknownEvent(_))
        ));
    }

    fn brain_with(
        client: FakeResponsesClient,
        tools: MapToolExecutor,
    ) -> ResponsesReplayBrain<FakeResponsesClient, MapToolExecutor> {
        ResponsesReplayBrain::new(
            client,
            tools,
            ResponsesBrainConfig::replay("gpt-5"),
            vec![NeutralBrainTool {
                name: "lookup".to_string(),
                description: "Look up data".to_string(),
                input_schema: json!({"type": "object"}),
            }],
        )
    }

    fn wake_request(
        provider_state: Option<BrainWakeProviderStateInput>,
        absence: Option<ProviderStateAbsenceReason>,
    ) -> BrainWakeRequest {
        BrainWakeRequest {
            brain: BrainImplementationHandle::new(1),
            session_id: SessionId::new("responses-session"),
            body_state: RuntimeBufferHandle::new(1),
            system_prompt: RuntimeBufferHandle::new(2),
            role_assembly: RuntimeBufferHandle::new(3),
            wake_id: "wake-responses".to_string(),
            provider_state,
            provider_state_absence: absence,
        }
    }

    fn provider_state(payload: Value) -> BrainWakeProviderStateInput {
        BrainWakeProviderStateInput {
            module_id: MODULE_ID.to_string(),
            strategy_id: REPLAY_STRATEGY_ID.to_string(),
            profile_fingerprint: "profile-fingerprint".to_string(),
            provider_fingerprint: "provider-fingerprint".to_string(),
            payload_version: PROVIDER_STATE_PAYLOAD_VERSION.to_string(),
            payload,
            expires_at: None,
        }
    }

    fn provider_state_payload(response_id: &str, output_items: Vec<ResponsesOutputItem>) -> Value {
        serde_json::to_value(OpenAiResponsesProviderStateV1 {
            kind: MODULE_ID.to_string(),
            strategy_id: REPLAY_STRATEGY_ID.to_string(),
            payload_version: PROVIDER_STATE_PAYLOAD_VERSION.to_string(),
            last_completed_response: Some(OpenAiResponsesCompletedResponseRecord {
                response_id: response_id.to_string(),
                output_items: output_items.iter().map(output_record_from_item).collect(),
                token_usage: None,
            }),
            replay_hints: None,
        })
        .unwrap()
    }

    fn body_state(pending_messages: Vec<AgentMessage>, recent_events: Vec<CoreEvent>) -> BodyState {
        BodyState {
            session: session_state(),
            pending_messages,
            recent_events,
            child_completions: Vec::new(),
            fan_out_groups: Vec::new(),
            delta_policy: BodyDeltaPolicy {
                mode: MidTurnDeltaMode::FrozenSnapshotNextWake,
                queue_owner: DeltaQueueOwner::Body,
                queued_message_ttl_ms: 5_000,
                max_queued_messages: 32,
            },
        }
    }

    fn session_state() -> SessionState {
        SessionState {
            handle: SessionHandle::new(1),
            session_id: SessionId::new("responses-session"),
            agent_id: AgentId::new("responses-agent"),
            profile_id: ProfileId::new("responses-profile"),
            kind: SessionKind::Full,
            delegation: None,
            resource_limits: ResourceLimits {
                workdir: None,
                max_duration_ms: None,
                max_delegation_depth: None,
            },
            tool_profile: ToolProfile { tools: Vec::new() },
            history_window: None,
            status: SessionStatus::Idle,
            brain_turn_count: 0,
            created_at: "2026-06-24T00:00:00Z".to_string(),
            last_active_at: "2026-06-24T00:00:00Z".to_string(),
        }
    }

    fn agent_message(
        from: &str,
        to: &str,
        body: &str,
        correlation_id: Option<&str>,
    ) -> AgentMessage {
        AgentMessage {
            from: AgentId::new(from),
            to: AgentId::new(to),
            body: body.to_string(),
            correlation_id: correlation_id.map(str::to_string),
        }
    }

    fn usage() -> ResponsesTokenUsage {
        ResponsesTokenUsage {
            input_tokens: 10,
            cached_input_tokens: 2,
            output_tokens: 5,
            reasoning_output_tokens: 1,
            total_tokens: 15,
        }
    }

    fn raw_event(event_type: impl Into<String>) -> ResponsesRawStreamEvent {
        ResponsesRawStreamEvent {
            event_type: event_type.into(),
            response_id: None,
            item: None,
            item_id: None,
            call_id: None,
            delta: None,
            message: None,
            usage: None,
        }
    }

    trait RawEventTestExt {
        fn delta(self, delta: impl Into<String>) -> Self;
        fn item(self, item: ResponsesOutputItem) -> Self;
        fn response_id(self, response_id: impl Into<String>) -> Self;
    }

    impl RawEventTestExt for ResponsesRawStreamEvent {
        fn delta(mut self, delta: impl Into<String>) -> Self {
            self.delta = Some(delta.into());
            self
        }

        fn item(mut self, item: ResponsesOutputItem) -> Self {
            self.item = Some(item);
            self
        }

        fn response_id(mut self, response_id: impl Into<String>) -> Self {
            self.response_id = Some(response_id.into());
            self
        }
    }
}
