//! Direct Rust scaffold for the OpenAI Responses brain module.
//!
//! This crate intentionally stays behind the language-neutral wake/stream
//! contract. It owns provider request/event shapes and fake-client tests, but
//! it does not reach into Rusty Crew coordination internals.

mod openai_oauth;

pub use openai_oauth::{
    openai_oauth_envelope_from_exchange_result, resolve_openai_oauth_bearer,
    OpenAiOauthBearerResolution, OpenAiOauthClient, OpenAiOauthCodeExchangeRequest,
    OpenAiOauthError, OpenAiOauthRefreshPolicy, OpenAiOauthSecretStore,
    OpenAiOauthTokenExchangeResult,
};

use reqwest::blocking::Client as HttpClient;
use rusty_crew_core_bridge_api::{BrainWakeStream, BrainWakeStreamProducer};
use rusty_crew_core_protocol::{
    AgentMessage, BodyState, BrainAction, BrainActionBatch, BrainEvent, BrainEventEnvelope,
    BrainProviderStatusLevel, BrainWakeFailure, BrainWakeProviderStateInput,
    BrainWakeProviderStateOutput, BrainWakeProviderStateUpdate, BrainWakeRequest,
    BrainWakeStreamItem, CompletionPacket, CompletionStatus, CoreError, CoreErrorKind, CoreEvent,
    CoreResult, ExternalEventPayload, ProviderStateAbsenceReason, ProviderStateMode,
    ToolCallMetadata, ToolCallPolicyMetadata, ToolCallSource,
};
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::io::Read;
use std::time::{Duration, Instant};

pub const MODULE_ID: &str = "openai-responses";
pub const REPLAY_STRATEGY_ID: &str = "replay";
pub const PREVIOUS_RESPONSE_CHAIN_STRATEGY_ID: &str = "previous-response-chain";
pub const PROVIDER_STATE_PAYLOAD_VERSION: &str = "openai-responses-state-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponsesBrainConfig {
    pub strategy: ResponsesBrainStrategy,
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
            strategy: ResponsesBrainStrategy::Replay,
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

    pub fn previous_response_chain(model: impl Into<String>) -> Self {
        Self {
            strategy: ResponsesBrainStrategy::PreviousResponseChain,
            ..Self::replay(model)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponsesBrainStrategy {
    Replay,
    PreviousResponseChain,
}

impl ResponsesBrainStrategy {
    fn strategy_id(self) -> &'static str {
        match self {
            Self::Replay => REPLAY_STRATEGY_ID,
            Self::PreviousResponseChain => PREVIOUS_RESPONSE_CHAIN_STRATEGY_ID,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,
    pub input: Vec<ResponsesInputItem>,
    pub tools: Vec<ResponsesToolDescriptor>,
    pub tool_choice: Value,
    pub parallel_tool_calls: bool,
    pub reasoning: Option<Value>,
    pub store: bool,
    pub stream: bool,
    pub include: Vec<String>,
    pub service_tier: Option<String>,
    pub prompt_cache_key: Option<String>,
    pub text: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
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

impl Serialize for ResponsesInputItem {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::UserMessage { content } => {
                let mut map = serializer.serialize_map(Some(3))?;
                map.serialize_entry("type", "message")?;
                map.serialize_entry("role", "user")?;
                map.serialize_entry("content", content)?;
                map.end()
            }
            Self::AssistantMessage { content } => {
                let mut map = serializer.serialize_map(Some(3))?;
                map.serialize_entry("type", "message")?;
                map.serialize_entry("role", "assistant")?;
                map.serialize_entry("content", content)?;
                map.end()
            }
            Self::Reasoning {
                id,
                summary,
                encrypted_content,
            } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("type", "reasoning")?;
                if let Some(id) = id {
                    map.serialize_entry("id", id)?;
                }
                let summary_value = summary
                    .as_ref()
                    .map(|text| json!([{"type": "summary_text", "text": text}]))
                    .unwrap_or_else(|| json!([]));
                map.serialize_entry("summary", &summary_value)?;
                if let Some(encrypted_content) = encrypted_content {
                    map.serialize_entry("encrypted_content", encrypted_content)?;
                }
                map.end()
            }
            Self::FunctionCall {
                id,
                call_id,
                name,
                arguments,
            } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("type", "function_call")?;
                if let Some(id) = id {
                    map.serialize_entry("id", id)?;
                }
                map.serialize_entry("call_id", call_id)?;
                map.serialize_entry("name", name)?;
                map.serialize_entry("arguments", arguments)?;
                map.end()
            }
            Self::FunctionCallOutput {
                call_id, output, ..
            } => {
                let mut map = serializer.serialize_map(Some(3))?;
                map.serialize_entry("type", "function_call_output")?;
                map.serialize_entry("call_id", call_id)?;
                map.serialize_entry("output", output)?;
                map.end()
            }
            Self::ReplayHint { raw_json } => raw_json.serialize(serializer),
        }
    }
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
        self.build_replay(wake, provider_state, history, continuation_items)
    }

    fn build_replay(
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
            previous_response_id: None,
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
            store: matches!(
                self.config.strategy,
                ResponsesBrainStrategy::PreviousResponseChain
            ),
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

    fn build_for_strategy(
        &self,
        wake: &BrainWakeRequest,
        provider_state: Option<&BrainWakeProviderStateInput>,
        provider_state_absence: Option<&ProviderStateAbsenceReason>,
        history: ResponsesReplayProjection,
        continuation_items: Vec<ResponsesInputItem>,
    ) -> ResponsesPlannedRequest {
        let replay_request =
            self.build_replay(wake, provider_state, history, continuation_items.clone());
        if self.config.strategy != ResponsesBrainStrategy::PreviousResponseChain {
            return ResponsesPlannedRequest {
                request: replay_request,
                fallback_reason: None,
            };
        }
        if !continuation_items.is_empty() {
            return ResponsesPlannedRequest {
                request: replay_request,
                fallback_reason: Some(PreviousResponseChainFallbackReason::NormalInvalidation),
            };
        }

        let fallback_reason =
            match previous_response_chain_state(provider_state, provider_state_absence) {
                Ok(Some(chain_state)) => {
                    let request_fingerprint = request_fingerprint(&replay_request);
                    if chain_state.request_fingerprint != request_fingerprint {
                        Some(PreviousResponseChainFallbackReason::RequestFingerprintMismatch)
                    } else {
                        match append_only_input_suffix(
                            &replay_request.input,
                            &chain_state.committed_context_items(),
                        ) {
                            Some(suffix) => {
                                let mut chained_request = replay_request.clone();
                                chained_request.previous_response_id =
                                    Some(chain_state.previous_response_id.clone());
                                chained_request.input = suffix;
                                return ResponsesPlannedRequest {
                                    request: chained_request,
                                    fallback_reason: None,
                                };
                            }
                            None => Some(PreviousResponseChainFallbackReason::InputNotAppendOnly),
                        }
                    }
                }
                Ok(None) => Some(absence_fallback_reason(provider_state_absence)),
                Err(reason) => Some(reason),
            };

        ResponsesPlannedRequest {
            request: replay_request,
            fallback_reason,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResponsesPlannedRequest {
    request: ResponsesRequest,
    fallback_reason: Option<PreviousResponseChainFallbackReason>,
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
    pub previous_response_chain: Option<PreviousResponseChainStateV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replay_hints: Option<OpenAiResponsesReplayHints>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviousResponseChainStateV1 {
    pub previous_response_id: String,
    pub request_fingerprint: String,
    pub completed_at: String,
    pub expires_at: String,
    pub committed_input_items: Vec<Value>,
    pub committed_output_items: Vec<OpenAiResponseOutputItemRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_response_metadata: Option<Value>,
}

impl PreviousResponseChainStateV1 {
    fn committed_context_items(&self) -> Vec<Value> {
        let mut items = self.committed_input_items.clone();
        items.extend(
            self.committed_output_items
                .iter()
                .cloned()
                .filter_map(replay_item_from_record)
                .filter_map(|item| serde_json::to_value(item).ok()),
        );
        items
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviousResponseChainFallbackReason {
    NoPredecessorState,
    RequestFingerprintMismatch,
    ProfileFingerprintMismatch,
    ProviderFingerprintMismatch,
    PredecessorRejectedByProvider,
    ProviderStateExpired,
    ProviderStateLoadFailed,
    InputNotAppendOnly,
    NormalInvalidation,
}

impl PreviousResponseChainFallbackReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::NoPredecessorState => "no_predecessor_state",
            Self::RequestFingerprintMismatch => "request_fingerprint_mismatch",
            Self::ProfileFingerprintMismatch => "profile_fingerprint_mismatch",
            Self::ProviderFingerprintMismatch => "provider_fingerprint_mismatch",
            Self::PredecessorRejectedByProvider => "predecessor_rejected_by_provider",
            Self::ProviderStateExpired => "provider_state_expired",
            Self::ProviderStateLoadFailed => "provider_state_load_failed",
            Self::InputNotAppendOnly => "input_not_append_only",
            Self::NormalInvalidation => "normal_invalidation",
        }
    }
}

fn previous_response_chain_state(
    provider_state: Option<&BrainWakeProviderStateInput>,
    provider_state_absence: Option<&ProviderStateAbsenceReason>,
) -> Result<Option<PreviousResponseChainStateV1>, PreviousResponseChainFallbackReason> {
    let Some(state) = provider_state else {
        return Ok(None);
    };
    if state.payload_version != PROVIDER_STATE_PAYLOAD_VERSION {
        return Err(PreviousResponseChainFallbackReason::ProviderStateLoadFailed);
    }
    let payload = serde_json::from_value::<OpenAiResponsesProviderStateV1>(state.payload.clone())
        .map_err(|_| PreviousResponseChainFallbackReason::ProviderStateLoadFailed)?;
    if provider_state_absence.is_some() {
        return Ok(None);
    }
    Ok(payload.previous_response_chain)
}

fn absence_fallback_reason(
    provider_state_absence: Option<&ProviderStateAbsenceReason>,
) -> PreviousResponseChainFallbackReason {
    match provider_state_absence {
        Some(ProviderStateAbsenceReason::Expired) => {
            PreviousResponseChainFallbackReason::ProviderStateExpired
        }
        Some(ProviderStateAbsenceReason::LoadFailed) => {
            PreviousResponseChainFallbackReason::ProviderStateLoadFailed
        }
        Some(ProviderStateAbsenceReason::Invalidated) => {
            PreviousResponseChainFallbackReason::NormalInvalidation
        }
        _ => PreviousResponseChainFallbackReason::NoPredecessorState,
    }
}

fn append_only_input_suffix(
    current_input: &[ResponsesInputItem],
    predecessor_context: &[Value],
) -> Option<Vec<ResponsesInputItem>> {
    let current_values = current_input
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if current_values.len() < predecessor_context.len() {
        return None;
    }
    if &current_values[..predecessor_context.len()] != predecessor_context {
        return None;
    }
    Some(current_input[predecessor_context.len()..].to_vec())
}

fn request_fingerprint(request: &ResponsesRequest) -> String {
    serde_json::to_string(&json!({
        "model": request.model,
        "instructions": request.instructions,
        "tools": request.tools,
        "toolChoice": request.tool_choice,
        "parallelToolCalls": request.parallel_tool_calls,
        "reasoning": request.reasoning,
        "store": request.store,
        "stream": request.stream,
        "include": request.include,
        "serviceTier": request.service_tier,
        "promptCacheKey": request.prompt_cache_key,
        "text": request.text,
    }))
    .unwrap_or_else(|_| "fingerprint-unavailable".to_string())
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
    #[error("provider transport error: {0}")]
    Transport(String),
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

    fn stream_observed(
        &mut self,
        request: ResponsesRequest,
        on_event: &mut dyn FnMut(&ResponsesEvent),
    ) -> Result<Vec<ResponsesEvent>, ResponsesStreamError> {
        let events = self.stream(request)?;
        for event in &events {
            on_event(event);
        }
        Ok(events)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponsesTransportMetrics {
    pub effective_transport: String,
    pub selected_strategy_id: String,
    pub effective_strategy_id: String,
    pub fallback_reason: Option<String>,
    pub provider_request_count: u64,
    pub continuation_round_count: u64,
    pub provider_request_payload_bytes: u64,
    pub provider_event_counts: HashMap<String, u64>,
    pub first_text_delta_latency_ms: Option<u64>,
    pub total_turn_duration_ms: u64,
}

struct ResponsesTransportMetricsBuilder {
    selected_strategy_id: &'static str,
    effective_strategy_id: &'static str,
    fallback_reason: Option<PreviousResponseChainFallbackReason>,
    provider_request_count: u64,
    continuation_round_count: u64,
    provider_request_payload_bytes: u64,
    provider_event_counts: HashMap<String, u64>,
    first_text_delta_latency_ms: Option<u64>,
    turn_started_at: Instant,
}

impl ResponsesTransportMetricsBuilder {
    fn new(config: &ResponsesBrainConfig) -> Self {
        let selected_strategy_id = config.strategy.strategy_id();
        Self {
            selected_strategy_id,
            effective_strategy_id: selected_strategy_id,
            fallback_reason: None,
            provider_request_count: 0,
            continuation_round_count: 0,
            provider_request_payload_bytes: 0,
            provider_event_counts: HashMap::new(),
            first_text_delta_latency_ms: None,
            turn_started_at: Instant::now(),
        }
    }

    fn observe_fallback(&mut self, reason: PreviousResponseChainFallbackReason) {
        self.effective_strategy_id = REPLAY_STRATEGY_ID;
        self.fallback_reason.get_or_insert(reason);
    }

    fn observe_request(&mut self, request: &ResponsesRequest) {
        self.provider_request_count += 1;
        if let Ok(payload) = serde_json::to_vec(request) {
            self.provider_request_payload_bytes += payload.len() as u64;
        }
    }

    fn observe_events(&mut self, events: &[ResponsesEvent], elapsed: Duration) {
        for event in events {
            *self
                .provider_event_counts
                .entry(responses_event_kind(event).to_string())
                .or_insert(0) += 1;
            if self.first_text_delta_latency_ms.is_none()
                && matches!(event, ResponsesEvent::TextDelta(_))
            {
                self.first_text_delta_latency_ms = Some(duration_ms(elapsed));
            }
        }
    }

    fn observe_continuation_round(&mut self) {
        self.continuation_round_count += 1;
    }

    fn finish(&self) -> ResponsesTransportMetrics {
        ResponsesTransportMetrics {
            effective_transport: "http-sse".to_string(),
            selected_strategy_id: self.selected_strategy_id.to_string(),
            effective_strategy_id: self.effective_strategy_id.to_string(),
            fallback_reason: self
                .fallback_reason
                .map(|reason| reason.as_str().to_string()),
            provider_request_count: self.provider_request_count,
            continuation_round_count: self.continuation_round_count,
            provider_request_payload_bytes: self.provider_request_payload_bytes,
            provider_event_counts: self.provider_event_counts.clone(),
            first_text_delta_latency_ms: self.first_text_delta_latency_ms,
            total_turn_duration_ms: duration_ms(self.turn_started_at.elapsed()),
        }
    }
}

fn responses_event_kind(event: &ResponsesEvent) -> &'static str {
    match event {
        ResponsesEvent::TextDelta(_) => "response.output_text.delta",
        ResponsesEvent::ReasoningDelta(_) => "response.reasoning.delta",
        ResponsesEvent::OutputItemAdded(_) => "response.output_item.added",
        ResponsesEvent::OutputItemDone(_) => "response.output_item.done",
        ResponsesEvent::Completed { .. } => "response.completed",
        ResponsesEvent::Failed(_) => "response.failed",
        ResponsesEvent::Incomplete(_) => "response.incomplete",
    }
}

fn duration_ms(duration: Duration) -> u64 {
    duration.as_millis().try_into().unwrap_or(u64::MAX)
}

#[derive(Debug, Clone)]
pub struct LiveResponsesClient {
    client: HttpClient,
    endpoint: String,
    bearer_token: Option<String>,
    account_id: Option<String>,
    is_fedramp_account: bool,
}

impl LiveResponsesClient {
    pub fn new(
        base_url: impl Into<String>,
        api_key: Option<String>,
        idle_timeout_ms: u64,
    ) -> Result<Self, ResponsesStreamError> {
        let base_url = base_url.into();
        let endpoint = format!("{}/responses", base_url.trim_end_matches('/'));
        let _idle_timeout_ms = idle_timeout_ms;
        let client = HttpClient::builder()
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|error| ResponsesStreamError::Transport(error.to_string()))?;
        Ok(Self {
            client,
            endpoint,
            bearer_token: api_key,
            account_id: None,
            is_fedramp_account: false,
        })
    }

    pub fn new_with_bearer_metadata(
        base_url: impl Into<String>,
        bearer_token: Option<String>,
        account_id: Option<String>,
        is_fedramp_account: bool,
        idle_timeout_ms: u64,
    ) -> Result<Self, ResponsesStreamError> {
        let mut client = Self::new(base_url, bearer_token, idle_timeout_ms)?;
        client.account_id = account_id;
        client.is_fedramp_account = is_fedramp_account;
        Ok(client)
    }
}

impl ResponsesClient for LiveResponsesClient {
    fn stream(
        &mut self,
        request: ResponsesRequest,
    ) -> Result<Vec<ResponsesEvent>, ResponsesStreamError> {
        self.stream_observed(request, &mut |_| {})
    }

    fn stream_observed(
        &mut self,
        request: ResponsesRequest,
        on_event: &mut dyn FnMut(&ResponsesEvent),
    ) -> Result<Vec<ResponsesEvent>, ResponsesStreamError> {
        let mut request = self.client.post(&self.endpoint).json(&request);
        if let Some(bearer_token) = &self.bearer_token {
            request = request.bearer_auth(bearer_token);
        }
        if let Some(account_id) = &self.account_id {
            request = request.header("ChatGPT-Account-ID", account_id);
        }
        if self.is_fedramp_account {
            request = request.header("X-OpenAI-Fedramp", "true");
        }
        let mut response = request.send().map_err(transport_error)?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().map_err(transport_error)?;
            return Err(ResponsesStreamError::Transport(format!(
                "HTTP {status}: {body}"
            )));
        }
        parse_sse_response(&mut response, on_event)
    }
}

fn transport_error(error: reqwest::Error) -> ResponsesStreamError {
    if error.is_timeout() {
        ResponsesStreamError::IdleTimeout
    } else {
        ResponsesStreamError::Transport(error.to_string())
    }
}

fn parse_sse_response(
    response: &mut reqwest::blocking::Response,
    on_event: &mut dyn FnMut(&ResponsesEvent),
) -> Result<Vec<ResponsesEvent>, ResponsesStreamError> {
    let mut events = Vec::new();
    let mut data_lines = Vec::new();
    let mut pending_line = String::new();
    let mut buffer = [0_u8; 8192];

    loop {
        let read = response.read(&mut buffer).map_err(|error| {
            if error.kind() == std::io::ErrorKind::TimedOut {
                ResponsesStreamError::IdleTimeout
            } else {
                ResponsesStreamError::Transport(error.to_string())
            }
        })?;
        if read == 0 {
            break;
        }
        let chunk = String::from_utf8_lossy(&buffer[..read]);
        pending_line.push_str(&chunk);
        while let Some(newline_index) = pending_line.find('\n') {
            let line = pending_line[..newline_index].to_string();
            pending_line.replace_range(..=newline_index, "");
            handle_sse_line(&line, &mut data_lines, &mut events, on_event)?;
        }
    }

    if !pending_line.is_empty() {
        handle_sse_line(&pending_line, &mut data_lines, &mut events, on_event)?;
    }
    flush_sse_data(&mut data_lines, &mut events, Some(on_event))?;
    Ok(events)
}

fn handle_sse_line(
    line: &str,
    data_lines: &mut Vec<String>,
    events: &mut Vec<ResponsesEvent>,
    on_event: &mut dyn FnMut(&ResponsesEvent),
) -> Result<(), ResponsesStreamError> {
    let line = line.trim_end_matches('\r');
    if line.is_empty() {
        flush_sse_data(data_lines, events, Some(on_event))?;
        return Ok(());
    }
    if let Some(data) = line.strip_prefix("data:") {
        data_lines.push(data.trim_start().to_string());
    }
    Ok(())
}

fn flush_sse_data(
    data_lines: &mut Vec<String>,
    events: &mut Vec<ResponsesEvent>,
    on_event: Option<&mut dyn FnMut(&ResponsesEvent)>,
) -> Result<(), ResponsesStreamError> {
    if data_lines.is_empty() {
        return Ok(());
    }
    let data = data_lines.join("\n");
    data_lines.clear();
    if data == "[DONE]" {
        return Ok(());
    }
    let value: Value = serde_json::from_str(&data)
        .map_err(|error| ResponsesStreamError::Transport(format!("invalid SSE JSON: {error}")))?;
    if let Some(event) = event_from_provider_value(value)? {
        if let Some(on_event) = on_event {
            on_event(&event);
        }
        events.push(event);
    }
    Ok(())
}

fn event_from_provider_value(value: Value) -> Result<Option<ResponsesEvent>, ResponsesStreamError> {
    let event_type = value
        .get("type")
        .and_then(Value::as_str)
        .ok_or(ResponsesStreamError::MissingField("type"))?;
    match event_type {
        "response.output_text.delta" => Ok(Some(ResponsesEvent::TextDelta(
            value
                .get("delta")
                .and_then(Value::as_str)
                .ok_or(ResponsesStreamError::MissingField("delta"))?
                .to_string(),
        ))),
        "response.reasoning.delta" | "response.reasoning_summary_text.delta" => {
            Ok(Some(ResponsesEvent::ReasoningDelta(
                value
                    .get("delta")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            )))
        }
        "response.output_item.added" => {
            let item = output_item_from_provider_value(
                value
                    .get("item")
                    .ok_or(ResponsesStreamError::MissingField("item"))?,
            )?;
            Ok(Some(ResponsesEvent::OutputItemAdded(item)))
        }
        "response.output_item.done" => {
            let item = output_item_from_provider_value(
                value
                    .get("item")
                    .ok_or(ResponsesStreamError::MissingField("item"))?,
            )?;
            Ok(Some(ResponsesEvent::OutputItemDone(item)))
        }
        "response.completed" => {
            let response = value
                .get("response")
                .ok_or(ResponsesStreamError::MissingField("response"))?;
            let response_id = response
                .get("id")
                .or_else(|| value.get("response_id"))
                .and_then(Value::as_str)
                .ok_or(ResponsesStreamError::MissingField("response.id"))?
                .to_string();
            Ok(Some(ResponsesEvent::Completed {
                response_id,
                usage: response
                    .get("usage")
                    .and_then(token_usage_from_provider_value),
            }))
        }
        "response.failed" => Ok(Some(ResponsesEvent::Failed(provider_message(
            &value,
            "provider response failed",
        )))),
        "response.incomplete" => Ok(Some(ResponsesEvent::Incomplete(provider_message(
            &value,
            "provider response incomplete",
        )))),
        _ => Ok(None),
    }
}

fn output_item_from_provider_value(
    value: &Value,
) -> Result<ResponsesOutputItem, ResponsesStreamError> {
    let item_type = value
        .get("type")
        .and_then(Value::as_str)
        .ok_or(ResponsesStreamError::MissingField("item.type"))?;
    match item_type {
        "message" => Ok(ResponsesOutputItem::Message {
            id: value.get("id").and_then(Value::as_str).map(str::to_string),
            text: message_text_from_provider_item(value),
        }),
        "reasoning" => Ok(ResponsesOutputItem::Reasoning {
            id: value.get("id").and_then(Value::as_str).map(str::to_string),
            summary: value
                .get("summary")
                .and_then(Value::as_str)
                .map(str::to_string),
            encrypted_content: value
                .get("encrypted_content")
                .and_then(Value::as_str)
                .map(str::to_string),
        }),
        "function_call" => Ok(ResponsesOutputItem::FunctionCall {
            id: value.get("id").and_then(Value::as_str).map(str::to_string),
            call_id: value
                .get("call_id")
                .and_then(Value::as_str)
                .ok_or(ResponsesStreamError::MissingField("item.call_id"))?
                .to_string(),
            name: value
                .get("name")
                .and_then(Value::as_str)
                .ok_or(ResponsesStreamError::MissingField("item.name"))?
                .to_string(),
            arguments: value
                .get("arguments")
                .and_then(Value::as_str)
                .unwrap_or("{}")
                .to_string(),
        }),
        _ => Ok(ResponsesOutputItem::Other {
            item_type: item_type.to_string(),
            raw_json: value.clone(),
        }),
    }
}

fn message_text_from_provider_item(value: &Value) -> String {
    value
        .get("content")
        .and_then(Value::as_array)
        .map(|content| {
            content
                .iter()
                .filter_map(|item| item.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("")
        })
        .filter(|text| !text.is_empty())
        .or_else(|| {
            value
                .get("text")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default()
}

fn token_usage_from_provider_value(value: &Value) -> Option<ResponsesTokenUsage> {
    Some(ResponsesTokenUsage {
        input_tokens: value.get("input_tokens")?.as_u64()?,
        cached_input_tokens: value
            .get("input_tokens_details")
            .and_then(|details| details.get("cached_tokens"))
            .and_then(Value::as_u64)
            .unwrap_or(0),
        output_tokens: value.get("output_tokens")?.as_u64()?,
        reasoning_output_tokens: value
            .get("output_tokens_details")
            .and_then(|details| details.get("reasoning_tokens"))
            .and_then(Value::as_u64)
            .unwrap_or(0),
        total_tokens: value.get("total_tokens")?.as_u64()?,
    })
}

fn provider_message(value: &Value, fallback: &str) -> String {
    value
        .get("response")
        .and_then(|response| response.get("error"))
        .and_then(|error| error.get("message"))
        .or_else(|| value.get("message"))
        .and_then(Value::as_str)
        .unwrap_or(fallback)
        .to_string()
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

type BrainWakeItemSink<'a> = Option<&'a mut dyn FnMut(BrainWakeStreamItem)>;

fn push_stream_item(
    items: &mut Vec<BrainWakeStreamItem>,
    item: BrainWakeStreamItem,
    sink: &mut BrainWakeItemSink<'_>,
) {
    if let Some(sink) = sink.as_deref_mut() {
        sink(item.clone());
    }
    items.push(item);
}

fn streaming_item_from_provider_event(
    request: &BrainWakeRequest,
    provider_event: &ResponsesEvent,
) -> Option<BrainWakeStreamItem> {
    match provider_event {
        ResponsesEvent::TextDelta(text) => {
            Some(event(request, BrainEvent::TextDelta { text: text.clone() }))
        }
        ResponsesEvent::ReasoningDelta(text) => Some(event(
            request,
            BrainEvent::ReasoningDelta {
                text: text.clone(),
                format: Some("openai-responses".to_string()),
            },
        )),
        _ => None,
    }
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
        self.wake_with_history_internal(request, history, None)
    }

    pub fn wake_with_history_and_stream_sink(
        &mut self,
        request: BrainWakeRequest,
        history: ResponsesReplayProjection,
        sink: &mut dyn FnMut(BrainWakeStreamItem),
    ) -> CoreResult<ResponsesBrainWakeResult> {
        self.wake_with_history_internal(request, history, Some(sink))
    }

    fn wake_with_history_internal(
        &mut self,
        request: BrainWakeRequest,
        history: ResponsesReplayProjection,
        mut sink: BrainWakeItemSink<'_>,
    ) -> CoreResult<ResponsesBrainWakeResult> {
        let mut metrics = ResponsesTransportMetricsBuilder::new(&self.request_builder.config);
        let mut items = Vec::new();
        push_stream_item(&mut items, event(&request, BrainEvent::Started), &mut sink);
        if let Some(absence) = &request.provider_state_absence {
            if matches!(
                absence,
                ProviderStateAbsenceReason::Missing
                    | ProviderStateAbsenceReason::Expired
                    | ProviderStateAbsenceReason::Invalidated
                    | ProviderStateAbsenceReason::LoadFailed
            ) {
                push_stream_item(
                    &mut items,
                    event(
                        &request,
                        BrainEvent::ProviderStatus {
                            level: BrainProviderStatusLevel::Info,
                            message: format!(
                                "responses replay starting without provider state: {absence:?}"
                            ),
                            metadata_json: None,
                        },
                    ),
                    &mut sink,
                );
            }
        }

        let mut continuation_items = Vec::new();
        let mut committed_output_items = Vec::new();
        let mut last_response_id = None;
        let mut last_usage = None;
        let base_history = history;

        for _ in 0..=self.max_continuations {
            let planned_request = self.request_builder.build_for_strategy(
                &request,
                request.provider_state.as_ref(),
                request.provider_state_absence.as_ref(),
                base_history.clone(),
                continuation_items.clone(),
            );
            if let Some(reason) = planned_request.fallback_reason {
                metrics.observe_fallback(reason);
                push_stream_item(
                    &mut items,
                    previous_response_chain_fallback_event(&request, reason),
                    &mut sink,
                );
            }
            let planned_fingerprint = request_fingerprint(&planned_request.request);
            let committed_input_items = planned_request.request.input.clone();
            metrics.observe_request(&planned_request.request);
            let request_started_at = Instant::now();
            let mut observed_deltas = Vec::new();
            let events = match self.client.stream_observed(
                planned_request.request.clone(),
                &mut |provider_event| {
                    if let Some(item) = streaming_item_from_provider_event(&request, provider_event)
                    {
                        if let Some(sink) = sink.as_deref_mut() {
                            sink(item.clone());
                        }
                        observed_deltas.push(item);
                    }
                },
            ) {
                Ok(events) => {
                    metrics.observe_events(&events, request_started_at.elapsed());
                    events
                }
                Err(error) => {
                    if planned_request.request.previous_response_id.is_some() {
                        metrics.observe_fallback(
                            PreviousResponseChainFallbackReason::PredecessorRejectedByProvider,
                        );
                        push_stream_item(
                            &mut items,
                            previous_response_chain_fallback_event(
                                &request,
                                PreviousResponseChainFallbackReason::PredecessorRejectedByProvider,
                            ),
                            &mut sink,
                        );
                        let replay_request = self.request_builder.build_replay(
                            &request,
                            request.provider_state.as_ref(),
                            base_history.clone(),
                            continuation_items.clone(),
                        );
                        let replay_fingerprint = request_fingerprint(&replay_request);
                        let replay_input_items = replay_request.input.clone();
                        metrics.observe_request(&replay_request);
                        let request_started_at = Instant::now();
                        let mut observed_deltas = Vec::new();
                        let completed_without_pending = match self.client.stream_observed(
                            replay_request,
                            &mut |provider_event| {
                                if let Some(item) =
                                    streaming_item_from_provider_event(&request, provider_event)
                                {
                                    if let Some(sink) = sink.as_deref_mut() {
                                        sink(item.clone());
                                    }
                                    observed_deltas.push(item);
                                }
                            },
                        ) {
                            Ok(events) => {
                                metrics.observe_events(&events, request_started_at.elapsed());
                                self.process_provider_events(
                                    &request,
                                    &mut items,
                                    events,
                                    observed_deltas.len(),
                                    &mut sink,
                                    &mut continuation_items,
                                    &mut committed_output_items,
                                    &mut last_response_id,
                                    &mut last_usage,
                                )
                            }
                            Err(error) => {
                                return Ok(failed_result(
                                    &request,
                                    items,
                                    error,
                                    metrics.finish(),
                                    &mut sink,
                                ))
                            }
                        };
                        let completed_without_pending = match completed_without_pending {
                            Ok(done) => done,
                            Err(error) => {
                                return Ok(failed_result(
                                    &request,
                                    items,
                                    error,
                                    metrics.finish(),
                                    &mut sink,
                                ))
                            }
                        };
                        if continuation_items.is_empty() {
                            debug_assert!(completed_without_pending);
                            return Ok(finish_responses_wake(
                                &request,
                                &self.request_builder.config,
                                items,
                                &mut sink,
                                CompletedResponsesAttempt {
                                    response_id: last_response_id,
                                    output_items: committed_output_items,
                                    usage: last_usage,
                                    committed_input_items: replay_input_items,
                                    request_fingerprint: replay_fingerprint,
                                },
                                metrics.finish(),
                            ));
                        }
                        metrics.observe_continuation_round();
                        continue;
                    }
                    return Ok(failed_result(
                        &request,
                        items,
                        error,
                        metrics.finish(),
                        &mut sink,
                    ));
                }
            };
            let completed_without_pending = self.process_provider_events(
                &request,
                &mut items,
                events,
                observed_deltas.len(),
                &mut sink,
                &mut continuation_items,
                &mut committed_output_items,
                &mut last_response_id,
                &mut last_usage,
            );
            let completed_without_pending = match completed_without_pending {
                Ok(done) => done,
                Err(error) => {
                    return Ok(failed_result(
                        &request,
                        items,
                        error,
                        metrics.finish(),
                        &mut sink,
                    ))
                }
            };
            if completed_without_pending {
                return Ok(finish_responses_wake(
                    &request,
                    &self.request_builder.config,
                    items,
                    &mut sink,
                    CompletedResponsesAttempt {
                        response_id: last_response_id,
                        output_items: committed_output_items,
                        usage: last_usage,
                        committed_input_items,
                        request_fingerprint: planned_fingerprint,
                    },
                    metrics.finish(),
                ));
            }
            metrics.observe_continuation_round();
        }

        Ok(failed_result(
            &request,
            items,
            ResponsesStreamError::IdleTimeout,
            metrics.finish(),
            &mut sink,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn process_provider_events(
        &self,
        request: &BrainWakeRequest,
        items: &mut Vec<BrainWakeStreamItem>,
        events: Vec<ResponsesEvent>,
        eagerly_streamed_delta_count: usize,
        sink: &mut BrainWakeItemSink<'_>,
        continuation_items: &mut Vec<ResponsesInputItem>,
        committed_output_items: &mut Vec<ResponsesOutputItem>,
        last_response_id: &mut Option<String>,
        last_usage: &mut Option<ResponsesTokenUsage>,
    ) -> Result<bool, ResponsesStreamError> {
        let mut completed = false;
        let mut pending_calls = Vec::new();
        let mut observed_delta_index = 0;
        for provider_event in events {
            match provider_event {
                ResponsesEvent::TextDelta(text) => {
                    let item = event(request, BrainEvent::TextDelta { text });
                    if observed_delta_index < eagerly_streamed_delta_count {
                        items.push(item);
                    } else {
                        push_stream_item(items, item, sink);
                    }
                    observed_delta_index += 1;
                }
                ResponsesEvent::ReasoningDelta(delta) => {
                    let item = event(
                        request,
                        BrainEvent::ReasoningDelta {
                            text: delta,
                            format: Some("openai-responses".to_string()),
                        },
                    );
                    if observed_delta_index < eagerly_streamed_delta_count {
                        items.push(item);
                    } else {
                        push_stream_item(items, item, sink);
                    }
                    observed_delta_index += 1;
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
                    *last_response_id = Some(response_id);
                    *last_usage = usage;
                }
                ResponsesEvent::Failed(message) => {
                    return Err(ResponsesStreamError::ResponseFailed(message));
                }
                ResponsesEvent::Incomplete(message) => {
                    return Err(ResponsesStreamError::ResponseIncomplete(message));
                }
            }
        }
        if !completed {
            return Err(ResponsesStreamError::ClosedBeforeComplete);
        }
        if pending_calls.is_empty() {
            return Ok(true);
        }
        for call in pending_calls {
            push_stream_item(
                items,
                event(
                    request,
                    BrainEvent::ToolCallStarted {
                        tool_name: call.name.clone(),
                        metadata: Some(tool_metadata(&call)),
                    },
                ),
                sink,
            );
            let output = self.tools.execute(&call);
            push_stream_item(
                items,
                event(
                    request,
                    BrainEvent::ToolCallFinished {
                        tool_name: call.name.clone(),
                        is_error: output.is_error,
                        metadata: Some(tool_metadata(&call)),
                    },
                ),
                sink,
            );
            continuation_items.push(ResponsesInputItem::FunctionCall {
                id: call.provider_item_id.clone(),
                call_id: call.call_id.clone(),
                name: call.name.clone(),
                arguments: call.arguments_json.clone(),
            });
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
        Ok(false)
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
    pub transport_metrics: ResponsesTransportMetrics,
}

struct CompletedResponsesAttempt {
    response_id: Option<String>,
    output_items: Vec<ResponsesOutputItem>,
    usage: Option<ResponsesTokenUsage>,
    committed_input_items: Vec<ResponsesInputItem>,
    request_fingerprint: String,
}

fn finish_responses_wake(
    request: &BrainWakeRequest,
    config: &ResponsesBrainConfig,
    mut items: Vec<BrainWakeStreamItem>,
    sink: &mut BrainWakeItemSink<'_>,
    completed: CompletedResponsesAttempt,
    transport_metrics: ResponsesTransportMetrics,
) -> ResponsesBrainWakeResult {
    push_stream_item(&mut items, event(request, BrainEvent::Finished), sink);
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
    push_stream_item(&mut items, BrainWakeStreamItem::actions(batch), sink);
    let provider_state = provider_state_output(
        request,
        config,
        completed
            .response_id
            .unwrap_or_else(|| "unknown-response".to_string()),
        completed.output_items,
        completed.usage,
        completed.committed_input_items,
        completed.request_fingerprint,
    );
    ResponsesBrainWakeResult {
        stream: BrainWakeStream::from_items(items),
        provider_state: Some(provider_state),
        transport_metrics,
    }
}

fn provider_state_output(
    request: &BrainWakeRequest,
    config: &ResponsesBrainConfig,
    response_id: String,
    output_items: Vec<ResponsesOutputItem>,
    usage: Option<ResponsesTokenUsage>,
    committed_input_items: Vec<ResponsesInputItem>,
    request_fingerprint: String,
) -> BrainWakeProviderStateOutput {
    let output_records: Vec<_> = output_items.iter().map(output_record_from_item).collect();
    let previous_response_chain = (config.strategy
        == ResponsesBrainStrategy::PreviousResponseChain)
        .then(|| PreviousResponseChainStateV1 {
            previous_response_id: response_id.clone(),
            request_fingerprint,
            completed_at: format!("wake:{}", request.wake_id),
            expires_at: "provider-wire-state-ttl".to_string(),
            committed_input_items: committed_input_items
                .into_iter()
                .filter_map(|item| serde_json::to_value(item).ok())
                .collect(),
            committed_output_items: output_records.clone(),
            provider_response_metadata: None,
        });
    let payload = OpenAiResponsesProviderStateV1 {
        kind: MODULE_ID.to_string(),
        strategy_id: config.strategy.strategy_id().to_string(),
        payload_version: PROVIDER_STATE_PAYLOAD_VERSION.to_string(),
        last_completed_response: Some(OpenAiResponsesCompletedResponseRecord {
            response_id,
            output_items: output_records,
            token_usage: usage,
        }),
        previous_response_chain,
        replay_hints: None,
    };
    BrainWakeProviderStateOutput::Replace {
        state: BrainWakeProviderStateUpdate {
            module_id: MODULE_ID.to_string(),
            strategy_id: config.strategy.strategy_id().to_string(),
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

fn previous_response_chain_fallback_event(
    request: &BrainWakeRequest,
    reason: PreviousResponseChainFallbackReason,
) -> BrainWakeStreamItem {
    event(
        request,
        BrainEvent::ProviderStatus {
            level: BrainProviderStatusLevel::Info,
            message: format!(
                "previous_response_id chain fell back to replay: {}",
                reason.as_str()
            ),
            metadata_json: Some(
                json!({
                    "selectedStrategyId": PREVIOUS_RESPONSE_CHAIN_STRATEGY_ID,
                    "effectiveStrategyId": REPLAY_STRATEGY_ID,
                    "replayFallbackUsed": true,
                    "fallbackReason": reason.as_str(),
                })
                .to_string(),
            ),
        },
    )
}

fn failed_result(
    request: &BrainWakeRequest,
    mut items: Vec<BrainWakeStreamItem>,
    error: ResponsesStreamError,
    transport_metrics: ResponsesTransportMetrics,
    sink: &mut BrainWakeItemSink<'_>,
) -> ResponsesBrainWakeResult {
    push_stream_item(
        &mut items,
        BrainWakeStreamItem::wake_failed(BrainWakeFailure {
            wake_id: request.wake_id.clone(),
            session_id: request.session_id.clone(),
            kind: CoreErrorKind::BrainUnavailable,
            message: error.to_string(),
        }),
        sink,
    );
    ResponsesBrainWakeResult {
        stream: BrainWakeStream::from_items(items),
        provider_state: None,
        transport_metrics,
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
    fn previous_response_chain_commits_predecessor_only_after_completion() {
        let mut brain = brain_with_config(
            FakeResponsesClient::new(vec![Ok(vec![
                ResponsesEvent::OutputItemDone(ResponsesOutputItem::Message {
                    id: Some("msg-1".to_string()),
                    text: "reply one".to_string(),
                }),
                ResponsesEvent::Completed {
                    response_id: "resp-1".to_string(),
                    usage: Some(usage()),
                },
            ])]),
            MapToolExecutor::default(),
            ResponsesBrainConfig::previous_response_chain("gpt-5"),
        );
        let history = ResponsesReplayProjection {
            input_items: vec![ResponsesInputItem::UserMessage {
                content: "human: first".to_string(),
            }],
            replay_hints: Vec::new(),
        };
        let result = brain
            .wake_with_history(wake_request(None, None), history.clone())
            .unwrap();

        let payload = provider_state_payload_from_output(result.provider_state);
        let chain = payload
            .previous_response_chain
            .expect("chain state should be present after completion");
        assert_eq!(chain.previous_response_id, "resp-1");
        assert_eq!(
            chain.committed_input_items,
            history
                .input_items
                .iter()
                .map(serde_json::to_value)
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        );
        assert_eq!(chain.committed_output_items[0].item_type, "message");

        let mut failed = brain_with_config(
            FakeResponsesClient::new(vec![Ok(vec![ResponsesEvent::TextDelta(
                "partial".to_string(),
            )])]),
            MapToolExecutor::default(),
            ResponsesBrainConfig::previous_response_chain("gpt-5"),
        );
        let failed_result = failed.wake(wake_request(None, None)).unwrap();
        assert!(failed_result.provider_state.is_none());
    }

    #[test]
    fn previous_response_chain_uses_compact_append_only_input_when_valid() {
        let state = valid_chain_provider_state();
        let history = append_only_history();
        let mut brain = brain_with_config(
            FakeResponsesClient::new(vec![Ok(vec![ResponsesEvent::Completed {
                response_id: "resp-2".to_string(),
                usage: None,
            }])]),
            MapToolExecutor::default(),
            ResponsesBrainConfig::previous_response_chain("gpt-5"),
        );
        let result = brain
            .wake_with_history(wake_request(Some(state), None), history)
            .unwrap();

        assert!(matches!(
            result.provider_state,
            Some(BrainWakeProviderStateOutput::Replace { .. })
        ));
        assert_eq!(brain.client.requests().len(), 1);
        assert_eq!(
            brain.client.requests()[0].previous_response_id.as_deref(),
            Some("resp-1")
        );
        assert_eq!(
            brain.client.requests()[0].input,
            vec![ResponsesInputItem::UserMessage {
                content: "human: second".to_string(),
            }]
        );
    }

    #[test]
    fn previous_response_chain_falls_back_on_request_fingerprint_mismatch() {
        let mut state = valid_chain_provider_state();
        let mut payload: OpenAiResponsesProviderStateV1 =
            serde_json::from_value(state.payload.clone()).unwrap();
        payload
            .previous_response_chain
            .as_mut()
            .unwrap()
            .request_fingerprint = "stale-fingerprint".to_string();
        state.payload = serde_json::to_value(payload).unwrap();

        let mut brain = brain_with_config(
            FakeResponsesClient::new(vec![Ok(vec![ResponsesEvent::Completed {
                response_id: "resp-replay".to_string(),
                usage: None,
            }])]),
            MapToolExecutor::default(),
            ResponsesBrainConfig::previous_response_chain("gpt-5"),
        );
        let result = brain
            .wake_with_history(wake_request(Some(state), None), append_only_history())
            .unwrap();
        let items = result.stream.drain_until_terminal().unwrap();

        assert!(fallback_reason_seen(
            &items,
            PreviousResponseChainFallbackReason::RequestFingerprintMismatch
        ));
        assert_eq!(brain.client.requests()[0].previous_response_id, None);
        assert!(brain.client.requests()[0].input.len() > 1);
    }

    #[test]
    fn previous_response_chain_falls_back_on_non_append_only_input() {
        let mut brain = brain_with_config(
            FakeResponsesClient::new(vec![Ok(vec![ResponsesEvent::Completed {
                response_id: "resp-replay".to_string(),
                usage: None,
            }])]),
            MapToolExecutor::default(),
            ResponsesBrainConfig::previous_response_chain("gpt-5"),
        );
        let result = brain
            .wake_with_history(
                wake_request(Some(valid_chain_provider_state()), None),
                ResponsesReplayProjection {
                    input_items: vec![ResponsesInputItem::UserMessage {
                        content: "human: rewritten first".to_string(),
                    }],
                    replay_hints: vec![ResponsesInputItem::UserMessage {
                        content: "human: second".to_string(),
                    }],
                },
            )
            .unwrap();
        let items = result.stream.drain_until_terminal().unwrap();

        assert!(fallback_reason_seen(
            &items,
            PreviousResponseChainFallbackReason::InputNotAppendOnly
        ));
        assert_eq!(brain.client.requests()[0].previous_response_id, None);
    }

    #[test]
    fn previous_response_chain_provider_rejection_replays_with_typed_diagnostic() {
        let mut brain = brain_with_config(
            FakeResponsesClient::new(vec![
                Err(ResponsesStreamError::Transport("HTTP 404".to_string())),
                Ok(vec![ResponsesEvent::Completed {
                    response_id: "resp-recovered".to_string(),
                    usage: None,
                }]),
            ]),
            MapToolExecutor::default(),
            ResponsesBrainConfig::previous_response_chain("gpt-5"),
        );
        let result = brain
            .wake_with_history(
                wake_request(Some(valid_chain_provider_state()), None),
                append_only_history(),
            )
            .unwrap();
        let items = result.stream.drain_until_terminal().unwrap();

        assert!(fallback_reason_seen(
            &items,
            PreviousResponseChainFallbackReason::PredecessorRejectedByProvider
        ));
        assert_eq!(brain.client.requests().len(), 2);
        assert_eq!(
            brain.client.requests()[0].previous_response_id.as_deref(),
            Some("resp-1")
        );
        assert_eq!(brain.client.requests()[1].previous_response_id, None);
        assert!(matches!(
            result.provider_state,
            Some(BrainWakeProviderStateOutput::Replace { .. })
        ));
    }

    #[test]
    fn previous_response_chain_expired_state_replays_with_typed_diagnostic() {
        let mut brain = brain_with_config(
            FakeResponsesClient::new(vec![Ok(vec![ResponsesEvent::Completed {
                response_id: "resp-replay".to_string(),
                usage: None,
            }])]),
            MapToolExecutor::default(),
            ResponsesBrainConfig::previous_response_chain("gpt-5"),
        );
        let result = brain
            .wake_with_history(
                wake_request(None, Some(ProviderStateAbsenceReason::Expired)),
                append_only_history(),
            )
            .unwrap();
        let items = result.stream.drain_until_terminal().unwrap();

        assert!(fallback_reason_seen(
            &items,
            PreviousResponseChainFallbackReason::ProviderStateExpired
        ));
        assert_eq!(brain.client.requests()[0].previous_response_id, None);
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
            BrainWakeStreamItem::Event { event } if matches!(&event.event, BrainEvent::ReasoningDelta { text, format } if text == "thinking" && format.as_deref() == Some("openai-responses"))
        )));
        assert!(matches!(
            result.provider_state,
            Some(BrainWakeProviderStateOutput::Replace { .. })
        ));
        assert_eq!(result.transport_metrics.effective_transport, "http-sse");
        assert_eq!(
            result.transport_metrics.selected_strategy_id,
            REPLAY_STRATEGY_ID
        );
        assert_eq!(
            result.transport_metrics.effective_strategy_id,
            REPLAY_STRATEGY_ID
        );
        assert_eq!(result.transport_metrics.provider_request_count, 1);
        assert!(
            result.transport_metrics.provider_request_payload_bytes > 0,
            "request payload bytes should be measured"
        );
        assert_eq!(
            result
                .transport_metrics
                .provider_event_counts
                .get("response.output_text.delta"),
            Some(&1)
        );
        assert_eq!(
            result
                .transport_metrics
                .provider_event_counts
                .get("response.completed"),
            Some(&1)
        );
        assert!(result
            .transport_metrics
            .first_text_delta_latency_ms
            .is_some());
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
        assert_eq!(result.transport_metrics.provider_request_count, 2);
        assert_eq!(result.transport_metrics.continuation_round_count, 1);
        assert_eq!(
            result
                .transport_metrics
                .provider_event_counts
                .get("response.output_item.done"),
            Some(&1)
        );
        assert_eq!(
            result
                .transport_metrics
                .provider_event_counts
                .get("response.completed"),
            Some(&2)
        );
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
            previous_response_id: None,
            input: vec![ResponsesInputItem::FunctionCallOutput {
                call_id: "wrong-call".to_string(),
                output: "oops".to_string(),
                is_error: false,
            }],
            tools: Vec::new(),
            tool_choice: json!("auto"),
            parallel_tool_calls: true,
            reasoning: None,
            store: false,
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
        brain_with_config(client, tools, ResponsesBrainConfig::replay("gpt-5"))
    }

    fn brain_with_config(
        client: FakeResponsesClient,
        tools: MapToolExecutor,
        config: ResponsesBrainConfig,
    ) -> ResponsesReplayBrain<FakeResponsesClient, MapToolExecutor> {
        ResponsesReplayBrain::new(
            client,
            tools,
            config,
            vec![NeutralBrainTool {
                name: "lookup".to_string(),
                description: "Look up data".to_string(),
                input_schema: json!({"type": "object"}),
            }],
        )
    }

    fn append_only_history() -> ResponsesReplayProjection {
        ResponsesReplayProjection {
            input_items: vec![ResponsesInputItem::UserMessage {
                content: "human: first".to_string(),
            }],
            replay_hints: vec![ResponsesInputItem::UserMessage {
                content: "human: second".to_string(),
            }],
        }
    }

    fn valid_chain_provider_state() -> BrainWakeProviderStateInput {
        let config = ResponsesBrainConfig::previous_response_chain("gpt-5");
        let builder = ResponsesRequestBuilder::new(config).tools(vec![NeutralBrainTool {
            name: "lookup".to_string(),
            description: "Look up data".to_string(),
            input_schema: json!({"type": "object"}),
        }]);
        let output = ResponsesOutputItem::Message {
            id: Some("msg-1".to_string()),
            text: "reply one".to_string(),
        };
        let completed_record = output_record_from_item(&output);
        let replay_state = provider_state(provider_state_payload("resp-1", vec![output]));
        let replay_request = builder.build_replay(
            &wake_request(Some(replay_state.clone()), None),
            Some(&replay_state),
            append_only_history(),
            Vec::new(),
        );
        let payload = OpenAiResponsesProviderStateV1 {
            kind: MODULE_ID.to_string(),
            strategy_id: PREVIOUS_RESPONSE_CHAIN_STRATEGY_ID.to_string(),
            payload_version: PROVIDER_STATE_PAYLOAD_VERSION.to_string(),
            last_completed_response: Some(OpenAiResponsesCompletedResponseRecord {
                response_id: "resp-1".to_string(),
                output_items: vec![completed_record.clone()],
                token_usage: None,
            }),
            previous_response_chain: Some(PreviousResponseChainStateV1 {
                previous_response_id: "resp-1".to_string(),
                request_fingerprint: request_fingerprint(&replay_request),
                completed_at: "wake:wake-1".to_string(),
                expires_at: "provider-wire-state-ttl".to_string(),
                committed_input_items: vec![serde_json::to_value(
                    ResponsesInputItem::UserMessage {
                        content: "human: first".to_string(),
                    },
                )
                .unwrap()],
                committed_output_items: vec![completed_record],
                provider_response_metadata: None,
            }),
            replay_hints: None,
        };
        provider_state(serde_json::to_value(payload).unwrap())
    }

    fn provider_state_payload_from_output(
        output: Option<BrainWakeProviderStateOutput>,
    ) -> OpenAiResponsesProviderStateV1 {
        let Some(BrainWakeProviderStateOutput::Replace { state }) = output else {
            panic!("expected provider-state replacement");
        };
        serde_json::from_value(state.payload).unwrap()
    }

    fn fallback_reason_seen(
        items: &[BrainWakeStreamItem],
        reason: PreviousResponseChainFallbackReason,
    ) -> bool {
        items.iter().any(|item| {
            let BrainWakeStreamItem::Event { event } = item else {
                return false;
            };
            let BrainEvent::ProviderStatus {
                metadata_json: Some(metadata),
                ..
            } = &event.event
            else {
                return false;
            };
            serde_json::from_str::<Value>(metadata)
                .ok()
                .and_then(|value| {
                    value
                        .get("fallbackReason")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .as_deref()
                == Some(reason.as_str())
        })
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
            previous_response_chain: None,
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
            projection: None,
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
