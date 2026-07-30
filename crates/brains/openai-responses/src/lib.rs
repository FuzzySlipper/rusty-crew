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

use reqwest::{Client as AsyncHttpClient, Response as AsyncHttpResponse};
use rusty_crew_brain_runtime::{
    decide_context_compaction, is_context_limit_provider_error, BrainContextCompactionArtifact,
    BrainContextCompactionDecision, BrainContextCompactionPolicy,
};
use rusty_crew_core_bridge_api::{BrainWakeStream, BrainWakeStreamProducer};
use rusty_crew_core_protocol::{
    AgentMessage, BodyState, BrainAction, BrainActionBatch, BrainContinuationPayload, BrainEvent,
    BrainEventEnvelope, BrainNoProgressPolicy, BrainNoProgressState, BrainProgressDisposition,
    BrainProgressResultClass, BrainProgressSample, BrainProviderStatusLevel, BrainWakeAttention,
    BrainWakeFailure, BrainWakeProviderStateInput, BrainWakeProviderStateOutput,
    BrainWakeProviderStateUpdate, BrainWakeRequest, BrainWakeStreamItem, CompletionPacket,
    CompletionStatus, CoreError, CoreErrorKind, CoreEvent, CoreResult, ExternalEventPayload,
    LogicalTurnAttentionReason, LogicalTurnResolutionAction, ProviderStateAbsenceReason,
    ProviderStateMode, ToolCallMetadata, ToolCallPolicyMetadata, ToolCallSource,
};
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use tokio::runtime::{Builder as RuntimeBuilder, Runtime};

pub const MODULE_ID: &str = "openai-responses";
pub const REPLAY_STRATEGY_ID: &str = "replay";
pub const PREVIOUS_RESPONSE_CHAIN_STRATEGY_ID: &str = "previous-response-chain";
pub const PROVIDER_STATE_PAYLOAD_VERSION: &str = "openai-responses-state-v1";
pub const DEFAULT_NO_PROGRESS_ATTENTION_THRESHOLD: u32 = 3;
pub const DEFAULT_WORK_QUANTUM_CONTINUATION_ROUNDS: usize = 64;
pub const CONTINUATION_PAYLOAD_VERSION: &str = "openai-responses-continuation-v2";

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
    pub max_output_tokens: Option<u32>,
    pub provider_request_timeout_ms: Option<u64>,
    pub work_quantum_continuation_rounds: usize,
    pub no_progress_attention_threshold: u32,
    pub context_compaction: Option<BrainContextCompactionPolicy>,
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
            max_output_tokens: None,
            provider_request_timeout_ms: None,
            work_quantum_continuation_rounds: DEFAULT_WORK_QUANTUM_CONTINUATION_ROUNDS,
            no_progress_attention_threshold: DEFAULT_NO_PROGRESS_ATTENTION_THRESHOLD,
            context_compaction: None,
        }
    }

    pub fn previous_response_chain(model: impl Into<String>) -> Self {
        Self {
            strategy: ResponsesBrainStrategy::PreviousResponseChain,
            ..Self::replay(model)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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

    fn from_strategy_id(strategy_id: &str) -> Result<Self, ResponsesStreamError> {
        match strategy_id {
            REPLAY_STRATEGY_ID => Ok(Self::Replay),
            PREVIOUS_RESPONSE_CHAIN_STRATEGY_ID => Ok(Self::PreviousResponseChain),
            other => Err(ResponsesStreamError::ContinuationStateInvalid(format!(
                "unknown Responses continuation strategy {other}"
            ))),
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,
    pub parallel_tool_calls: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Value>,
    pub store: bool,
    pub stream: bool,
    pub include: Vec<String>,
    pub service_tier: Option<String>,
    pub prompt_cache_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
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
        let include_provider_item_ids = matches!(
            self.config.strategy,
            ResponsesBrainStrategy::PreviousResponseChain
        );
        let mut input = history.input_items;
        input.extend(provider_replay_items(
            provider_state,
            include_provider_item_ids,
        ));
        input.extend(history.replay_hints);
        input.extend(continuation_items);
        if input.is_empty() {
            input.push(ResponsesInputItem::UserMessage {
                content: format!("wake {} has no Rust-owned history yet", wake.wake_id),
            });
        }
        let tools = self
            .tools
            .iter()
            .map(adapt_neutral_tool)
            .collect::<Vec<_>>();
        let tool_choice = (!tools.is_empty()).then(|| match &self.config.tool_choice {
            ResponsesToolChoice::Auto => json!("auto"),
            ResponsesToolChoice::None => json!("none"),
            ResponsesToolChoice::Function { name } => {
                json!({"type": "function", "name": name})
            }
        });
        ResponsesRequest {
            model: self.config.model.clone(),
            instructions: self.config.instructions.clone(),
            previous_response_id: None,
            input,
            tools,
            tool_choice,
            parallel_tool_calls: self.config.parallel_tool_calls,
            reasoning: self
                .config
                .reasoning
                .as_ref()
                .and_then(responses_reasoning_value),
            store: matches!(
                self.config.strategy,
                ResponsesBrainStrategy::PreviousResponseChain
            ),
            stream: true,
            include: self.config.include.clone(),
            service_tier: self.config.service_tier.clone(),
            prompt_cache_key: self.config.prompt_cache_key.clone(),
            max_output_tokens: self.config.max_output_tokens,
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

fn responses_reasoning_value(reasoning: &ResponsesReasoningConfig) -> Option<Value> {
    let mut value = serde_json::Map::new();
    if let Some(effort) = reasoning.effort.as_ref() {
        value.insert("effort".to_string(), json!(effort));
    }
    if let Some(summary) = reasoning.summary.as_ref() {
        value.insert("summary".to_string(), json!(summary));
    }
    (!value.is_empty()).then_some(Value::Object(value))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResponsesPlannedRequest {
    request: ResponsesRequest,
    fallback_reason: Option<PreviousResponseChainFallbackReason>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
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
                    ExternalEventPayload::ChannelMessage { payload } => {
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
                .filter_map(|record| replay_item_from_record(record, true))
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
        "maxOutputTokens": request.max_output_tokens,
        "text": request.text,
    }))
    .unwrap_or_else(|_| "fingerprint-unavailable".to_string())
}

fn provider_replay_items(
    provider_state: Option<&BrainWakeProviderStateInput>,
    include_provider_item_ids: bool,
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
            if let Some(item) = replay_item_from_record(record, include_provider_item_ids) {
                items.push(item);
            }
        }
    }
    if let Some(hints) = payload.replay_hints {
        for record in hints.reasoning_items {
            if let Some(item) = replay_item_from_record(record, include_provider_item_ids) {
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

fn replay_item_from_record(
    record: OpenAiResponseOutputItemRecord,
    include_provider_item_ids: bool,
) -> Option<ResponsesInputItem> {
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
            id: include_provider_item_ids.then_some(id).flatten(),
            summary,
            encrypted_content,
        }),
        ResponsesOutputItem::FunctionCall {
            id,
            call_id,
            name,
            arguments,
        } => Some(ResponsesInputItem::FunctionCall {
            id: include_provider_item_ids.then_some(id).flatten(),
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
    #[error("provider request timeout")]
    RequestTimeout,
    #[error("provider request cancelled")]
    Cancelled,
    #[error("provider stream closed before response.completed")]
    ClosedBeforeComplete,
    #[error("provider response failed: {0}")]
    ResponseFailed(String),
    #[error("provider response incomplete: {0}")]
    ResponseIncomplete(String),
    #[error("function call output call_id mismatch: expected {expected}, got {actual}")]
    FunctionCallOutputMismatch { expected: String, actual: String },
    #[error("Responses continuation state is invalid: {0}")]
    ContinuationStateInvalid(String),
    #[error("Responses continuation checkpoint failed: {0}")]
    ContinuationCheckpointFailed(String),
    #[error("provider transport error: {0}")]
    Transport(String),
}

impl ResponsesStreamError {
    pub fn reason_code(&self) -> &'static str {
        match self {
            Self::RequestTimeout => "provider_request_timeout",
            Self::Cancelled => "provider_request_cancelled",
            Self::ResponseFailed(_) => "provider_response_failed",
            Self::ResponseIncomplete(_) => "provider_response_incomplete",
            Self::ContinuationStateInvalid(_) => "responses_continuation_state_invalid",
            Self::ContinuationCheckpointFailed(_) => "responses_continuation_checkpoint_failed",
            Self::ClosedBeforeComplete => "provider_stream_closed_before_complete",
            Self::MissingField(_) | Self::UnknownEvent(_) => "provider_protocol_error",
            Self::FunctionCallOutputMismatch { .. } => "responses_function_call_output_mismatch",
            Self::Transport(_) => "provider_transport_error",
        }
    }

    pub fn source(&self) -> &'static str {
        match self {
            Self::RequestTimeout | Self::Cancelled | Self::Transport(_) => "provider_transport",
            Self::ResponseFailed(_) | Self::ResponseIncomplete(_) => "provider_response",
            Self::ContinuationStateInvalid(_)
            | Self::ContinuationCheckpointFailed(_)
            | Self::FunctionCallOutputMismatch { .. } => "responses_loop",
            Self::ClosedBeforeComplete | Self::MissingField(_) | Self::UnknownEvent(_) => {
                "provider_protocol"
            }
        }
    }
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
    pub state_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResponsesContinuationStateV1 {
    strategy: ResponsesBrainStrategy,
    base_history: ResponsesContinuationProjection,
    continuation_items: Vec<ResponsesContinuationInputItem>,
    committed_output_items: Vec<ResponsesOutputItem>,
    last_response_id: Option<String>,
    last_usage: Option<ResponsesTokenUsage>,
    no_progress_state: BrainNoProgressState,
    #[serde(default)]
    output_continuation: ResponsesOutputContinuationState,
    provider_state: Option<BrainWakeProviderStateInput>,
    provider_state_absence: Option<ProviderStateAbsenceReason>,
    metrics: ResponsesContinuationMetrics,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResponsesOutputContinuationState {
    overlap_text: String,
    overlap_reasoning: String,
    accumulated_text: String,
    accumulated_reasoning: String,
    provider_guidance: Option<String>,
    compaction_guidance: Option<String>,
    #[serde(default)]
    context_compaction: ResponsesContextCompactionState,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResponsesContextCompactionState {
    last_compacted_item_count: usize,
    artifacts: Vec<BrainContextCompactionArtifact>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResponsesContinuationProjection {
    input_items: Vec<ResponsesContinuationInputItem>,
    replay_hints: Vec<ResponsesContinuationInputItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ResponsesContinuationInputItem {
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResponsesContinuationMetrics {
    selected_strategy_id: String,
    effective_strategy_id: String,
    fallback_reason: Option<PreviousResponseChainFallbackReason>,
    provider_request_count: u64,
    continuation_round_count: u64,
    provider_request_payload_bytes: u64,
    provider_request_debug_samples: Vec<Value>,
    provider_event_counts: HashMap<String, u64>,
    first_text_delta_latency_ms: Option<u64>,
    elapsed_turn_duration_ms: u64,
}

impl From<&ResponsesInputItem> for ResponsesContinuationInputItem {
    fn from(item: &ResponsesInputItem) -> Self {
        match item {
            ResponsesInputItem::UserMessage { content } => Self::UserMessage {
                content: content.clone(),
            },
            ResponsesInputItem::AssistantMessage { content } => Self::AssistantMessage {
                content: content.clone(),
            },
            ResponsesInputItem::Reasoning {
                id,
                summary,
                encrypted_content,
            } => Self::Reasoning {
                id: id.clone(),
                summary: summary.clone(),
                encrypted_content: encrypted_content.clone(),
            },
            ResponsesInputItem::FunctionCall {
                id,
                call_id,
                name,
                arguments,
            } => Self::FunctionCall {
                id: id.clone(),
                call_id: call_id.clone(),
                name: name.clone(),
                arguments: arguments.clone(),
            },
            ResponsesInputItem::FunctionCallOutput {
                call_id,
                output,
                is_error,
            } => Self::FunctionCallOutput {
                call_id: call_id.clone(),
                output: output.clone(),
                is_error: *is_error,
            },
            ResponsesInputItem::ReplayHint { raw_json } => Self::ReplayHint {
                raw_json: raw_json.clone(),
            },
        }
    }
}

impl From<ResponsesContinuationInputItem> for ResponsesInputItem {
    fn from(item: ResponsesContinuationInputItem) -> Self {
        match item {
            ResponsesContinuationInputItem::UserMessage { content } => {
                Self::UserMessage { content }
            }
            ResponsesContinuationInputItem::AssistantMessage { content } => {
                Self::AssistantMessage { content }
            }
            ResponsesContinuationInputItem::Reasoning {
                id,
                summary,
                encrypted_content,
            } => Self::Reasoning {
                id,
                summary,
                encrypted_content,
            },
            ResponsesContinuationInputItem::FunctionCall {
                id,
                call_id,
                name,
                arguments,
            } => Self::FunctionCall {
                id,
                call_id,
                name,
                arguments,
            },
            ResponsesContinuationInputItem::FunctionCallOutput {
                call_id,
                output,
                is_error,
            } => Self::FunctionCallOutput {
                call_id,
                output,
                is_error,
            },
            ResponsesContinuationInputItem::ReplayHint { raw_json } => {
                Self::ReplayHint { raw_json }
            }
        }
    }
}

impl From<&ResponsesReplayProjection> for ResponsesContinuationProjection {
    fn from(projection: &ResponsesReplayProjection) -> Self {
        Self {
            input_items: projection
                .input_items
                .iter()
                .map(ResponsesContinuationInputItem::from)
                .collect(),
            replay_hints: projection
                .replay_hints
                .iter()
                .map(ResponsesContinuationInputItem::from)
                .collect(),
        }
    }
}

impl From<ResponsesContinuationProjection> for ResponsesReplayProjection {
    fn from(projection: ResponsesContinuationProjection) -> Self {
        Self {
            input_items: projection
                .input_items
                .into_iter()
                .map(ResponsesInputItem::from)
                .collect(),
            replay_hints: projection
                .replay_hints
                .into_iter()
                .map(ResponsesInputItem::from)
                .collect(),
        }
    }
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
    pub provider_request_debug_samples: Vec<Value>,
    pub provider_event_counts: HashMap<String, u64>,
    pub first_text_delta_latency_ms: Option<u64>,
    pub total_turn_duration_ms: u64,
    pub terminal_failure_reason_code: Option<String>,
    pub terminal_failure_source: Option<String>,
}

struct ResponsesTransportMetricsBuilder {
    selected_strategy_id: &'static str,
    effective_strategy_id: &'static str,
    fallback_reason: Option<PreviousResponseChainFallbackReason>,
    provider_request_count: u64,
    continuation_round_count: u64,
    provider_request_payload_bytes: u64,
    provider_request_debug_samples: Vec<Value>,
    provider_event_counts: HashMap<String, u64>,
    first_text_delta_latency_ms: Option<u64>,
    prior_turn_duration_ms: u64,
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
            provider_request_debug_samples: Vec::new(),
            provider_event_counts: HashMap::new(),
            first_text_delta_latency_ms: None,
            prior_turn_duration_ms: 0,
            turn_started_at: Instant::now(),
        }
    }

    fn restore(state: ResponsesContinuationMetrics) -> Self {
        Self {
            selected_strategy_id: strategy_id_static(&state.selected_strategy_id),
            effective_strategy_id: strategy_id_static(&state.effective_strategy_id),
            fallback_reason: state.fallback_reason,
            provider_request_count: state.provider_request_count,
            continuation_round_count: state.continuation_round_count,
            provider_request_payload_bytes: state.provider_request_payload_bytes,
            provider_request_debug_samples: state.provider_request_debug_samples,
            provider_event_counts: state.provider_event_counts,
            first_text_delta_latency_ms: state.first_text_delta_latency_ms,
            prior_turn_duration_ms: state.elapsed_turn_duration_ms,
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
        if self.provider_request_debug_samples.len() < 4 {
            if let Ok(value) = serde_json::to_value(request) {
                self.provider_request_debug_samples.push(value);
            }
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
            provider_request_debug_samples: self.provider_request_debug_samples.clone(),
            provider_event_counts: self.provider_event_counts.clone(),
            first_text_delta_latency_ms: self.first_text_delta_latency_ms,
            total_turn_duration_ms: self
                .prior_turn_duration_ms
                .saturating_add(duration_ms(self.turn_started_at.elapsed())),
            terminal_failure_reason_code: None,
            terminal_failure_source: None,
        }
    }

    fn checkpoint(&self) -> ResponsesContinuationMetrics {
        ResponsesContinuationMetrics {
            selected_strategy_id: self.selected_strategy_id.to_string(),
            effective_strategy_id: self.effective_strategy_id.to_string(),
            fallback_reason: self.fallback_reason,
            provider_request_count: self.provider_request_count,
            continuation_round_count: self.continuation_round_count,
            provider_request_payload_bytes: self.provider_request_payload_bytes,
            provider_request_debug_samples: self.provider_request_debug_samples.clone(),
            provider_event_counts: self.provider_event_counts.clone(),
            first_text_delta_latency_ms: self.first_text_delta_latency_ms,
            elapsed_turn_duration_ms: self
                .prior_turn_duration_ms
                .saturating_add(duration_ms(self.turn_started_at.elapsed())),
        }
    }
}

fn strategy_id_static(strategy_id: &str) -> &'static str {
    match strategy_id {
        PREVIOUS_RESPONSE_CHAIN_STRATEGY_ID => PREVIOUS_RESPONSE_CHAIN_STRATEGY_ID,
        _ => REPLAY_STRATEGY_ID,
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

#[derive(Debug, Clone, Default)]
pub struct ProviderCancellation {
    cancelled: Arc<AtomicBool>,
}

impl ProviderCancellation {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

pub struct LiveResponsesClient {
    client: AsyncHttpClient,
    endpoint: String,
    bearer_token: Option<String>,
    account_id: Option<String>,
    is_fedramp_account: bool,
    provider_request_timeout: Option<Duration>,
    cancellation: ProviderCancellation,
    runtime: Runtime,
}

impl LiveResponsesClient {
    pub fn new(
        base_url: impl Into<String>,
        api_key: Option<String>,
        provider_request_timeout_ms: Option<u64>,
        cancellation: ProviderCancellation,
    ) -> Result<Self, ResponsesStreamError> {
        let base_url = base_url.into();
        let endpoint = format!("{}/responses", base_url.trim_end_matches('/'));
        let client = AsyncHttpClient::builder()
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|error| ResponsesStreamError::Transport(error.to_string()))?;
        let runtime = RuntimeBuilder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| ResponsesStreamError::Transport(error.to_string()))?;
        Ok(Self {
            client,
            endpoint,
            bearer_token: api_key,
            account_id: None,
            is_fedramp_account: false,
            provider_request_timeout: provider_request_timeout_ms.map(Duration::from_millis),
            cancellation,
            runtime,
        })
    }

    pub fn new_with_bearer_metadata(
        base_url: impl Into<String>,
        bearer_token: Option<String>,
        account_id: Option<String>,
        is_fedramp_account: bool,
        provider_request_timeout_ms: Option<u64>,
        cancellation: ProviderCancellation,
    ) -> Result<Self, ResponsesStreamError> {
        let mut client = Self::new(
            base_url,
            bearer_token,
            provider_request_timeout_ms,
            cancellation,
        )?;
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
        self.runtime.block_on(stream_responses_response(
            request,
            self.provider_request_timeout,
            &self.cancellation,
            on_event,
        ))
    }
}

const PROVIDER_CANCELLATION_POLL_INTERVAL: Duration = Duration::from_millis(25);

async fn stream_responses_response(
    request: reqwest::RequestBuilder,
    provider_request_timeout: Option<Duration>,
    cancellation: &ProviderCancellation,
    on_event: &mut dyn FnMut(&ResponsesEvent),
) -> Result<Vec<ResponsesEvent>, ResponsesStreamError> {
    let deadline = provider_request_timeout.map(|timeout| Instant::now() + timeout);
    let mut send = Box::pin(request.send());
    let mut response = loop {
        ensure_provider_request_active(cancellation, deadline)?;
        let poll_for = provider_poll_duration(deadline)?;
        match tokio::time::timeout(poll_for, &mut send).await {
            Ok(result) => break result.map_err(transport_error)?,
            Err(_) => continue,
        }
    };
    let status = response.status();
    if !status.is_success() {
        let body = read_provider_response_text(&mut response, cancellation, deadline).await?;
        return Err(ResponsesStreamError::Transport(format!(
            "HTTP {status}: {body}"
        )));
    }
    parse_async_sse_response(&mut response, cancellation, deadline, on_event).await
}

async fn parse_async_sse_response(
    response: &mut AsyncHttpResponse,
    cancellation: &ProviderCancellation,
    deadline: Option<Instant>,
    on_event: &mut dyn FnMut(&ResponsesEvent),
) -> Result<Vec<ResponsesEvent>, ResponsesStreamError> {
    let mut events = Vec::new();
    let mut data_lines = Vec::new();
    let mut pending_line = String::new();

    while let Some(chunk) = next_provider_chunk(response, cancellation, deadline).await? {
        pending_line.push_str(&String::from_utf8_lossy(&chunk));
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

async fn read_provider_response_text(
    response: &mut AsyncHttpResponse,
    cancellation: &ProviderCancellation,
    deadline: Option<Instant>,
) -> Result<String, ResponsesStreamError> {
    let mut body = Vec::new();
    while let Some(chunk) = next_provider_chunk(response, cancellation, deadline).await? {
        body.extend_from_slice(&chunk);
    }
    Ok(String::from_utf8_lossy(&body).into_owned())
}

async fn next_provider_chunk(
    response: &mut AsyncHttpResponse,
    cancellation: &ProviderCancellation,
    deadline: Option<Instant>,
) -> Result<Option<Vec<u8>>, ResponsesStreamError> {
    let mut next = Box::pin(response.chunk());
    loop {
        ensure_provider_request_active(cancellation, deadline)?;
        let poll_for = provider_poll_duration(deadline)?;
        match tokio::time::timeout(poll_for, &mut next).await {
            Ok(result) => {
                return result
                    .map(|chunk| chunk.map(|value| value.to_vec()))
                    .map_err(transport_error)
            }
            Err(_) => continue,
        }
    }
}

fn ensure_provider_request_active(
    cancellation: &ProviderCancellation,
    deadline: Option<Instant>,
) -> Result<(), ResponsesStreamError> {
    if cancellation.is_cancelled() {
        return Err(ResponsesStreamError::Cancelled);
    }
    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        return Err(ResponsesStreamError::RequestTimeout);
    }
    Ok(())
}

fn provider_poll_duration(deadline: Option<Instant>) -> Result<Duration, ResponsesStreamError> {
    let Some(deadline) = deadline else {
        return Ok(PROVIDER_CANCELLATION_POLL_INTERVAL);
    };
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .ok_or(ResponsesStreamError::RequestTimeout)?;
    Ok(remaining.min(PROVIDER_CANCELLATION_POLL_INTERVAL))
}

fn transport_error(error: reqwest::Error) -> ResponsesStreamError {
    ResponsesStreamError::Transport(error.to_string())
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
}

enum ResponsesProviderDisposition {
    Complete,
    Continue,
    Yield,
    AttentionRequired(BrainWakeAttention),
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

fn projected_streaming_item_from_provider_event(
    request: &BrainWakeRequest,
    provider_event: &ResponsesEvent,
    pending_text_overlap: &mut String,
    pending_reasoning_overlap: &mut String,
    projected_text: &mut String,
    projected_reasoning: &mut String,
) -> Option<BrainWakeStreamItem> {
    match provider_event {
        ResponsesEvent::TextDelta(text) => {
            let projected = suppress_responses_replayed_prefix(pending_text_overlap, text);
            if projected.is_empty() {
                None
            } else {
                projected_text.push_str(&projected);
                Some(event(request, BrainEvent::TextDelta { text: projected }))
            }
        }
        ResponsesEvent::ReasoningDelta(text) => {
            let projected = suppress_responses_replayed_prefix(pending_reasoning_overlap, text);
            if projected.is_empty() {
                None
            } else {
                projected_reasoning.push_str(&projected);
                Some(event(
                    request,
                    BrainEvent::ReasoningDelta {
                        text: projected,
                        format: Some("openai-responses".to_string()),
                    },
                ))
            }
        }
        _ => None,
    }
}

fn suppress_responses_replayed_prefix(overlap: &mut String, delta: &str) -> String {
    if overlap.is_empty() || delta.is_empty() {
        return delta.to_string();
    }
    if overlap.starts_with(delta) {
        overlap.drain(..delta.len());
        return String::new();
    }
    if delta.starts_with(overlap.as_str()) {
        let suffix = delta[overlap.len()..].to_string();
        overlap.clear();
        return suffix;
    }
    overlap.clear();
    delta.to_string()
}

fn apply_output_continuation_guidance(request: &mut ResponsesRequest, guidance: Option<&str>) {
    let Some(guidance) = guidance else {
        return;
    };
    let instructions = request.instructions.get_or_insert_with(String::new);
    if !instructions.is_empty() {
        instructions.push_str("\n\n");
    }
    instructions.push_str(guidance);
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
        let mut items = Vec::new();
        let restored = match request.continuation_state.as_ref() {
            Some(payload) => match responses_continuation_state(payload) {
                Ok(state) => Some(state),
                Err(error) => {
                    let metrics =
                        ResponsesTransportMetricsBuilder::new(&self.request_builder.config)
                            .finish();
                    return Ok(failed_result(&request, items, error, metrics, &mut sink));
                }
            },
            None => None,
        };
        if let Some(state) = restored.as_ref() {
            self.request_builder.config.strategy = state.strategy;
        }
        let mut metrics = restored
            .as_ref()
            .map(|state| ResponsesTransportMetricsBuilder::restore(state.metrics.clone()))
            .unwrap_or_else(|| ResponsesTransportMetricsBuilder::new(&self.request_builder.config));
        if restored.is_none() {
            push_stream_item(&mut items, event(&request, BrainEvent::Started), &mut sink);
        }
        let mut provider_state = restored
            .as_ref()
            .and_then(|state| state.provider_state.clone())
            .or_else(|| request.provider_state.clone());
        let mut provider_state_absence = restored
            .as_ref()
            .and_then(|state| state.provider_state_absence.clone())
            .or_else(|| request.provider_state_absence.clone());
        if restored.is_none() {
            if let Some(absence) = &provider_state_absence {
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
        }

        let mut continuation_items: Vec<ResponsesInputItem> = restored
            .as_ref()
            .map(|state| {
                state
                    .continuation_items
                    .clone()
                    .into_iter()
                    .map(ResponsesInputItem::from)
                    .collect()
            })
            .unwrap_or_default();
        let mut committed_output_items = restored
            .as_ref()
            .map(|state| state.committed_output_items.clone())
            .unwrap_or_default();
        let mut last_response_id = restored
            .as_ref()
            .and_then(|state| state.last_response_id.clone());
        let mut last_usage = restored.as_ref().and_then(|state| state.last_usage.clone());
        let base_history = restored
            .as_ref()
            .map(|state| ResponsesReplayProjection::from(state.base_history.clone()))
            .unwrap_or(history);
        let mut no_progress_state = restored
            .as_ref()
            .map(|state| state.no_progress_state.clone())
            .unwrap_or_default();
        let mut output_continuation = restored
            .as_ref()
            .map(|state| state.output_continuation.clone())
            .unwrap_or_default();
        let no_progress_policy =
            BrainNoProgressPolicy::new(self.request_builder.config.no_progress_attention_threshold)
                .map_err(|message| {
                    CoreError::new(
                        CoreErrorKind::InvalidInput,
                        format!("invalid Responses no-progress policy: {message}"),
                    )
                })?;
        let mut epoch_continuation_round_count = 0usize;

        loop {
            macro_rules! pause_on_provider_context_limit {
                ($error:expr) => {
                    if self.request_builder.config.context_compaction.is_some()
                        && is_context_limit_provider_error(&$error.to_string())
                    {
                        let summary = format!(
                            "The provider rejected the Responses projection before mid-turn context compaction could complete: {}",
                            $error
                        );
                        push_stream_item(
                            &mut items,
                            responses_context_compaction_failure_event(&request, &summary),
                            &mut sink,
                        );
                        return Ok(attention_responses_wake(
                            &request,
                            &self.request_builder.config,
                            items,
                            &mut sink,
                            &base_history,
                            continuation_items,
                            committed_output_items,
                            last_response_id,
                            last_usage,
                            no_progress_state,
                            output_continuation,
                            provider_state,
                            provider_state_absence,
                            metrics,
                            responses_context_compaction_attention(summary),
                        ));
                    }
                };
            }
            if continuation_items.len()
                > output_continuation
                    .context_compaction
                    .last_compacted_item_count
            {
                let serialized_bytes = serde_json::to_vec(&(
                    &base_history,
                    &continuation_items,
                    &output_continuation.compaction_guidance,
                ))
                .map(|value| value.len())
                .unwrap_or(usize::MAX);
                let provider_input_tokens = last_usage.as_ref().map(|usage| usage.input_tokens);
                match decide_context_compaction(
                    self.request_builder.config.context_compaction.as_ref(),
                    provider_input_tokens,
                    serialized_bytes,
                ) {
                    Ok(BrainContextCompactionDecision::Compact(usage)) => {
                        push_stream_item(
                            &mut items,
                            responses_context_compaction_event(
                                &request,
                                "context_compaction_started",
                                BrainProviderStatusLevel::Info,
                                "Mid-turn Responses context compaction started at a safe provider boundary.",
                                &usage,
                                None,
                            ),
                            &mut sink,
                        );
                        let sequence =
                            output_continuation.context_compaction.artifacts.len() as u64 + 1;
                        match compact_responses_items(
                            &mut continuation_items,
                            self.request_builder
                                .config
                                .context_compaction
                                .as_ref()
                                .expect("compact decision policy"),
                            usage.clone(),
                            sequence,
                            output_continuation.compaction_guidance.as_deref(),
                        ) {
                            Ok((artifact, guidance)) => {
                                output_continuation.compaction_guidance = Some(guidance);
                                output_continuation
                                    .context_compaction
                                    .artifacts
                                    .push(artifact.clone());
                                output_continuation
                                    .context_compaction
                                    .last_compacted_item_count = continuation_items.len();
                                provider_state = None;
                                provider_state_absence =
                                    Some(ProviderStateAbsenceReason::Invalidated);
                                push_stream_item(
                                    &mut items,
                                    responses_context_compaction_event(
                                        &request,
                                        "context_compaction_completed",
                                        BrainProviderStatusLevel::Info,
                                        "Mid-turn Responses context compaction completed; previous-response chaining was deliberately rebuilt from the compacted replay projection.",
                                        &usage,
                                        Some(&artifact),
                                    ),
                                    &mut sink,
                                );
                                return Ok(yield_responses_wake(
                                    &request,
                                    &self.request_builder.config,
                                    items,
                                    &mut sink,
                                    &base_history,
                                    continuation_items,
                                    committed_output_items,
                                    last_response_id,
                                    last_usage,
                                    no_progress_state,
                                    output_continuation,
                                    provider_state,
                                    provider_state_absence,
                                    metrics,
                                ));
                            }
                            Err(summary) => {
                                push_stream_item(
                                    &mut items,
                                    responses_context_compaction_event(
                                        &request,
                                        "context_compaction_failed",
                                        BrainProviderStatusLevel::Error,
                                        &summary,
                                        &usage,
                                        None,
                                    ),
                                    &mut sink,
                                );
                                return Ok(attention_responses_wake(
                                    &request,
                                    &self.request_builder.config,
                                    items,
                                    &mut sink,
                                    &base_history,
                                    continuation_items,
                                    committed_output_items,
                                    last_response_id,
                                    last_usage,
                                    no_progress_state,
                                    output_continuation,
                                    provider_state,
                                    provider_state_absence,
                                    metrics,
                                    responses_context_compaction_attention(summary),
                                ));
                            }
                        }
                    }
                    Ok(BrainContextCompactionDecision::Disabled)
                    | Ok(BrainContextCompactionDecision::BelowThreshold(_)) => {}
                    Err(message) => {
                        return Err(CoreError::new(
                            CoreErrorKind::InvalidInput,
                            format!("invalid Responses context compaction policy: {message}"),
                        ));
                    }
                }
            }
            let mut planned_request = self.request_builder.build_for_strategy(
                &request,
                provider_state.as_ref(),
                provider_state_absence.as_ref(),
                base_history.clone(),
                continuation_items.clone(),
            );
            apply_output_continuation_guidance(
                &mut planned_request.request,
                responses_continuation_guidance(&output_continuation).as_deref(),
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
            let mut pending_text_overlap = output_continuation.overlap_text.clone();
            let mut pending_reasoning_overlap = output_continuation.overlap_reasoning.clone();
            let mut projected_text = String::new();
            let mut projected_reasoning = String::new();
            let events = match self.client.stream_observed(
                planned_request.request.clone(),
                &mut |provider_event| {
                    let item = projected_streaming_item_from_provider_event(
                        &request,
                        provider_event,
                        &mut pending_text_overlap,
                        &mut pending_reasoning_overlap,
                        &mut projected_text,
                        &mut projected_reasoning,
                    );
                    if let Some(item) = item.as_ref() {
                        if let Some(sink) = sink.as_deref_mut() {
                            sink(item.clone());
                        }
                    }
                    if matches!(
                        provider_event,
                        ResponsesEvent::TextDelta(_) | ResponsesEvent::ReasoningDelta(_)
                    ) {
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
                        let mut replay_request = self.request_builder.build_replay(
                            &request,
                            provider_state.as_ref(),
                            base_history.clone(),
                            continuation_items.clone(),
                        );
                        apply_output_continuation_guidance(
                            &mut replay_request,
                            responses_continuation_guidance(&output_continuation).as_deref(),
                        );
                        let replay_fingerprint = request_fingerprint(&replay_request);
                        let replay_input_items = replay_request.input.clone();
                        metrics.observe_request(&replay_request);
                        let request_started_at = Instant::now();
                        let mut observed_deltas = Vec::new();
                        let mut pending_text_overlap = output_continuation.overlap_text.clone();
                        let mut pending_reasoning_overlap =
                            output_continuation.overlap_reasoning.clone();
                        let mut projected_text = String::new();
                        let mut projected_reasoning = String::new();
                        let disposition = match self.client.stream_observed(
                            replay_request,
                            &mut |provider_event| {
                                let item = projected_streaming_item_from_provider_event(
                                    &request,
                                    provider_event,
                                    &mut pending_text_overlap,
                                    &mut pending_reasoning_overlap,
                                    &mut projected_text,
                                    &mut projected_reasoning,
                                );
                                if let Some(item) = item.as_ref() {
                                    if let Some(sink) = sink.as_deref_mut() {
                                        sink(item.clone());
                                    }
                                }
                                if matches!(
                                    provider_event,
                                    ResponsesEvent::TextDelta(_)
                                        | ResponsesEvent::ReasoningDelta(_)
                                ) {
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
                                    &observed_deltas,
                                    &projected_text,
                                    &projected_reasoning,
                                    &mut sink,
                                    &mut continuation_items,
                                    &mut committed_output_items,
                                    &mut last_response_id,
                                    &mut last_usage,
                                    no_progress_policy,
                                    &mut no_progress_state,
                                    &mut output_continuation,
                                )
                            }
                            Err(error) => {
                                pause_on_provider_context_limit!(error);
                                return Ok(failed_result(
                                    &request,
                                    items,
                                    error,
                                    metrics.finish(),
                                    &mut sink,
                                ));
                            }
                        };
                        let disposition = match disposition {
                            Ok(done) => done,
                            Err(error) => {
                                pause_on_provider_context_limit!(error);
                                return Ok(failed_result(
                                    &request,
                                    items,
                                    error,
                                    metrics.finish(),
                                    &mut sink,
                                ));
                            }
                        };
                        if matches!(&disposition, ResponsesProviderDisposition::Complete) {
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
                        let must_yield =
                            matches!(&disposition, ResponsesProviderDisposition::Yield);
                        if let ResponsesProviderDisposition::AttentionRequired(attention) =
                            disposition
                        {
                            return Ok(attention_responses_wake(
                                &request,
                                &self.request_builder.config,
                                items,
                                &mut sink,
                                &base_history,
                                continuation_items,
                                committed_output_items,
                                last_response_id,
                                last_usage,
                                no_progress_state,
                                output_continuation,
                                provider_state,
                                provider_state_absence,
                                metrics,
                                attention,
                            ));
                        }
                        metrics.observe_continuation_round();
                        epoch_continuation_round_count += 1;
                        if must_yield
                            || epoch_continuation_round_count
                                >= self.request_builder.config.work_quantum_continuation_rounds
                        {
                            return Ok(yield_responses_wake(
                                &request,
                                &self.request_builder.config,
                                items,
                                &mut sink,
                                &base_history,
                                continuation_items,
                                committed_output_items,
                                last_response_id,
                                last_usage,
                                no_progress_state,
                                output_continuation,
                                provider_state,
                                provider_state_absence,
                                metrics,
                            ));
                        }
                        continue;
                    }
                    pause_on_provider_context_limit!(error);
                    return Ok(failed_result(
                        &request,
                        items,
                        error,
                        metrics.finish(),
                        &mut sink,
                    ));
                }
            };
            let disposition = self.process_provider_events(
                &request,
                &mut items,
                events,
                &observed_deltas,
                &projected_text,
                &projected_reasoning,
                &mut sink,
                &mut continuation_items,
                &mut committed_output_items,
                &mut last_response_id,
                &mut last_usage,
                no_progress_policy,
                &mut no_progress_state,
                &mut output_continuation,
            );
            let disposition = match disposition {
                Ok(done) => done,
                Err(error) => {
                    pause_on_provider_context_limit!(error);
                    return Ok(failed_result(
                        &request,
                        items,
                        error,
                        metrics.finish(),
                        &mut sink,
                    ));
                }
            };
            if matches!(&disposition, ResponsesProviderDisposition::Complete) {
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
            let must_yield = matches!(&disposition, ResponsesProviderDisposition::Yield);
            if let ResponsesProviderDisposition::AttentionRequired(attention) = disposition {
                return Ok(attention_responses_wake(
                    &request,
                    &self.request_builder.config,
                    items,
                    &mut sink,
                    &base_history,
                    continuation_items,
                    committed_output_items,
                    last_response_id,
                    last_usage,
                    no_progress_state,
                    output_continuation,
                    provider_state,
                    provider_state_absence,
                    metrics,
                    attention,
                ));
            }
            metrics.observe_continuation_round();
            epoch_continuation_round_count += 1;
            if must_yield
                || epoch_continuation_round_count
                    >= self.request_builder.config.work_quantum_continuation_rounds
            {
                return Ok(yield_responses_wake(
                    &request,
                    &self.request_builder.config,
                    items,
                    &mut sink,
                    &base_history,
                    continuation_items,
                    committed_output_items,
                    last_response_id,
                    last_usage,
                    no_progress_state,
                    output_continuation,
                    provider_state,
                    provider_state_absence,
                    metrics,
                ));
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn process_provider_events(
        &self,
        request: &BrainWakeRequest,
        items: &mut Vec<BrainWakeStreamItem>,
        events: Vec<ResponsesEvent>,
        eagerly_streamed_deltas: &[Option<BrainWakeStreamItem>],
        projected_text: &str,
        projected_reasoning: &str,
        sink: &mut BrainWakeItemSink<'_>,
        continuation_items: &mut Vec<ResponsesInputItem>,
        committed_output_items: &mut Vec<ResponsesOutputItem>,
        last_response_id: &mut Option<String>,
        last_usage: &mut Option<ResponsesTokenUsage>,
        no_progress_policy: BrainNoProgressPolicy,
        no_progress_state: &mut BrainNoProgressState,
        output_continuation: &mut ResponsesOutputContinuationState,
    ) -> Result<ResponsesProviderDisposition, ResponsesStreamError> {
        let mut completed = false;
        let mut incomplete = None;
        let mut pending_calls = Vec::new();
        let mut observed_delta_index = 0;
        let mut assistant_progress = Sha256::new();
        let mut raw_text = String::new();
        let mut raw_reasoning = String::new();
        for provider_event in events {
            match provider_event {
                ResponsesEvent::TextDelta(text) => {
                    assistant_progress.update(b"text");
                    assistant_progress.update(text.as_bytes());
                    raw_text.push_str(&text);
                    if let Some(observed) = eagerly_streamed_deltas.get(observed_delta_index) {
                        if let Some(item) = observed {
                            items.push(item.clone());
                        }
                    } else {
                        push_stream_item(
                            items,
                            event(request, BrainEvent::TextDelta { text }),
                            sink,
                        );
                    }
                    observed_delta_index += 1;
                }
                ResponsesEvent::ReasoningDelta(delta) => {
                    assistant_progress.update(b"reasoning");
                    assistant_progress.update(delta.as_bytes());
                    raw_reasoning.push_str(&delta);
                    if let Some(observed) = eagerly_streamed_deltas.get(observed_delta_index) {
                        if let Some(item) = observed {
                            items.push(item.clone());
                        }
                    } else {
                        push_stream_item(
                            items,
                            event(
                                request,
                                BrainEvent::ReasoningDelta {
                                    text: delta,
                                    format: Some("openai-responses".to_string()),
                                },
                            ),
                            sink,
                        );
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
                    incomplete = Some(message);
                }
            }
        }
        if !completed && incomplete.is_none() {
            return Err(ResponsesStreamError::ClosedBeforeComplete);
        }
        if completed {
            output_continuation.overlap_text.clear();
            output_continuation.overlap_reasoning.clear();
            output_continuation.accumulated_text.clear();
            output_continuation.accumulated_reasoning.clear();
            output_continuation.provider_guidance = None;
        }
        if completed && pending_calls.is_empty() {
            return Ok(ResponsesProviderDisposition::Complete);
        }

        let mut output_attention = None;
        if let Some(incomplete_message) = incomplete.as_deref() {
            output_continuation
                .accumulated_text
                .push_str(projected_text);
            output_continuation
                .accumulated_reasoning
                .push_str(projected_reasoning);
            output_continuation.overlap_text = raw_text.clone();
            output_continuation.overlap_reasoning = raw_reasoning.clone();
            output_continuation.provider_guidance = Some(format!(
                "[Rusty Crew output continuation] The previous Responses request ended incomplete ({incomplete_message}). Continue exactly where the assistant output stopped. Do not repeat text or reasoning already emitted, and finish the requested work or next complete tool call."
            ));
            if !projected_text.is_empty() {
                continuation_items.push(ResponsesInputItem::AssistantMessage {
                    content: projected_text.to_string(),
                });
            }
            push_stream_item(
                items,
                event(
                    request,
                    BrainEvent::ProviderStatus {
                        level: BrainProviderStatusLevel::Degraded,
                        message: "Responses provider exhausted its per-request output budget; the same logical turn was checkpointed for continuation.".to_string(),
                        metadata_json: Some(
                            json!({
                                "kind": "output_limit_continuation",
                                "provider_message": incomplete_message,
                                "attention_threshold": no_progress_policy.attention_threshold(),
                            })
                            .to_string(),
                        ),
                    },
                ),
                sink,
            );
            if pending_calls.is_empty() {
                let state_json = serde_json::to_string(continuation_items).unwrap_or_default();
                let disposition = no_progress_policy.observe(
                    no_progress_state,
                    BrainProgressSample {
                        intent_fingerprint: progress_fingerprint(&[
                            "responses_output_limit_continuation",
                            incomplete_message,
                        ]),
                        result_fingerprint: progress_fingerprint(&[&raw_text, &raw_reasoning]),
                        state_fingerprint: progress_fingerprint(&[&state_json]),
                        assistant_progress_fingerprint: progress_fingerprint(&[
                            &output_continuation.accumulated_text,
                            &output_continuation.accumulated_reasoning,
                        ]),
                        result_class: BrainProgressResultClass::MalformedProviderOutput,
                    },
                );
                if let BrainProgressDisposition::AttentionRequired {
                    consecutive_samples,
                } = disposition
                {
                    let reason_code = "responses_output_limit_no_progress";
                    let summary = format!(
                        "provider repeatedly exhausted its output budget without advancing the assistant response ({consecutive_samples} equivalent repetitions)"
                    );
                    push_stream_item(
                        items,
                        responses_no_progress_attention_event(
                            request,
                            reason_code,
                            &summary,
                            consecutive_samples,
                        ),
                        sink,
                    );
                    output_attention = Some(responses_no_progress_attention(
                        reason_code,
                        summary,
                        consecutive_samples,
                    ));
                }
            }
        }
        let assistant_progress_fingerprint = format!("{:x}", assistant_progress.finalize());
        let mut attention = None;
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
            let mut output = self.tools.execute(&call);
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
            let disposition = no_progress_policy.observe(
                no_progress_state,
                BrainProgressSample {
                    intent_fingerprint: progress_fingerprint(&[
                        "function_call",
                        &call.name,
                        &call.arguments_json,
                    ]),
                    result_fingerprint: progress_fingerprint(&[
                        if output.is_error { "error" } else { "success" },
                        &output.output,
                    ]),
                    state_fingerprint: output.state_fingerprint.clone(),
                    assistant_progress_fingerprint: assistant_progress_fingerprint.clone(),
                    result_class: if output.is_error {
                        BrainProgressResultClass::Failed
                    } else {
                        BrainProgressResultClass::Succeeded
                    },
                },
            );
            if let BrainProgressDisposition::Correction {
                consecutive_samples,
            } = disposition
            {
                output.output.push_str(&format!(
                    "\n\n[Rusty Crew no-progress guidance] This function returned the same failure for unchanged arguments again ({consecutive_samples} equivalent repetition(s)). Change the arguments, choose another tool, or report the dependency as unavailable instead of repeating it unchanged."
                ));
                push_stream_item(
                    items,
                    responses_no_progress_correction_event(
                        request,
                        &call.name,
                        consecutive_samples,
                        no_progress_policy.attention_threshold(),
                    ),
                    sink,
                );
            }
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
            if let BrainProgressDisposition::AttentionRequired {
                consecutive_samples,
            } = disposition
            {
                let reason_code = "responses_tool_no_progress";
                let summary = format!(
                    "function {} returned an equivalent failure for unchanged arguments {consecutive_samples} consecutive times",
                    call.name
                );
                push_stream_item(
                    items,
                    responses_no_progress_attention_event(
                        request,
                        reason_code,
                        &summary,
                        consecutive_samples,
                    ),
                    sink,
                );
                attention = Some(responses_no_progress_attention(
                    reason_code,
                    summary,
                    consecutive_samples,
                ));
            }
        }
        Ok(match attention.or(output_attention) {
            Some(attention) => ResponsesProviderDisposition::AttentionRequired(attention),
            None if incomplete.is_some() => ResponsesProviderDisposition::Yield,
            None => ResponsesProviderDisposition::Continue,
        })
    }
}

fn responses_continuation_guidance(state: &ResponsesOutputContinuationState) -> Option<String> {
    let parts = [
        state.compaction_guidance.as_deref(),
        state.provider_guidance.as_deref(),
    ]
    .into_iter()
    .flatten()
    .filter(|value| !value.trim().is_empty())
    .collect::<Vec<_>>();
    (!parts.is_empty()).then(|| parts.join("\n\n"))
}

fn responses_context_compaction_event(
    request: &BrainWakeRequest,
    kind: &str,
    level: BrainProviderStatusLevel,
    message: &str,
    usage: &rusty_crew_brain_runtime::BrainContextUsageSnapshot,
    artifact: Option<&BrainContextCompactionArtifact>,
) -> BrainWakeStreamItem {
    event(
        request,
        BrainEvent::ProviderStatus {
            level,
            message: message.to_string(),
            metadata_json: Some(
                json!({
                    "kind": kind,
                    "usage": usage,
                    "artifact": artifact,
                })
                .to_string(),
            ),
        },
    )
}

fn responses_context_compaction_failure_event(
    request: &BrainWakeRequest,
    message: &str,
) -> BrainWakeStreamItem {
    event(
        request,
        BrainEvent::ProviderStatus {
            level: BrainProviderStatusLevel::Error,
            message: message.to_string(),
            metadata_json: Some(
                json!({
                    "kind": "context_compaction_failed",
                    "reasonCode": "provider_context_limit_before_compaction",
                })
                .to_string(),
            ),
        },
    )
}

fn responses_context_compaction_attention(summary: String) -> BrainWakeAttention {
    BrainWakeAttention {
        reason: LogicalTurnAttentionReason::InvariantRepairRequired,
        reason_code: "responses_context_compaction_attention".to_string(),
        summary,
        evidence_refs: vec!["context_compaction".to_string()],
        resolution_actions: vec![
            LogicalTurnResolutionAction::RetryProviderOperation,
            LogicalTurnResolutionAction::Cancel,
        ],
        retry_unchanged_safe: false,
        consecutive_no_progress_samples: 0,
    }
}

fn compact_responses_items(
    items: &mut Vec<ResponsesInputItem>,
    policy: &BrainContextCompactionPolicy,
    usage_before: rusty_crew_brain_runtime::BrainContextUsageSnapshot,
    sequence: u64,
    prior_guidance: Option<&str>,
) -> Result<(BrainContextCompactionArtifact, String), String> {
    let mut recent_start = items.len().saturating_sub(4);
    while recent_start > 0
        && matches!(
            items.get(recent_start),
            Some(ResponsesInputItem::FunctionCallOutput { .. })
        )
    {
        recent_start -= 1;
    }
    if recent_start == 0 {
        return Err(
            "Responses context pressure exceeded the configured threshold, but no completed continuation exchange can be compacted without touching the current tool context"
                .to_string(),
        );
    }
    let compacted = &items[..recent_start];
    let summary_budget = policy
        .target_tokens()
        .saturating_mul(4)
        .saturating_div(4)
        .clamp(256, 4096) as usize;
    let mut summary = String::from(
        "[Rusty Crew mid-turn context summary]\nEarlier completed Responses exchanges were compacted from provider input only. Raw transcript, reasoning telemetry, and tool effects remain authoritative.\n",
    );
    if let Some(prior) = prior_guidance {
        summary.push_str(truncate_utf8_responses(
            prior,
            summary_budget.saturating_div(3),
        ));
        summary.push('\n');
    }
    for item in compacted {
        let line = match item {
            ResponsesInputItem::UserMessage { content } => format!("user: {content}\n"),
            ResponsesInputItem::AssistantMessage { content } => {
                format!("assistant: {content}\n")
            }
            ResponsesInputItem::Reasoning { summary, .. } => {
                format!(
                    "reasoning summary: {}\n",
                    summary.as_deref().unwrap_or("retained")
                )
            }
            ResponsesInputItem::FunctionCall {
                name, arguments, ..
            } => format!("tool call: {name}({arguments})\n"),
            ResponsesInputItem::FunctionCallOutput {
                call_id,
                output,
                is_error,
            } => format!("tool result {call_id} error={is_error}: {output}\n"),
            ResponsesInputItem::ReplayHint { .. } => "provider replay hint retained\n".to_string(),
        };
        let remaining = summary_budget.saturating_sub(summary.len());
        if remaining <= 32 {
            break;
        }
        summary.push_str(truncate_utf8_responses(&line, remaining.min(320)));
    }
    let replacement = items[recent_start..].to_vec();
    let estimated_tokens_after = serde_json::to_vec(&(&replacement, &summary))
        .map(|value| (value.len() as u64).saturating_add(3) / 4)
        .map_err(|error| format!("serialize compacted Responses context: {error}"))?;
    if estimated_tokens_after >= usage_before.input_tokens {
        return Err(
            "Responses context compaction could not produce a smaller provider projection while preserving the current tool exchange"
                .to_string(),
        );
    }
    let artifact = BrainContextCompactionArtifact {
        sequence,
        strategy_id: policy.strategy_id.clone(),
        reason_code: "context_fill_threshold_exceeded".to_string(),
        usage_before,
        estimated_tokens_after,
        compacted_item_count: compacted.len() as u64,
        retained_item_count: replacement.len() as u64,
        summary_text: summary.clone(),
        provider_chain_action: Some("rebuild_replay_after_compaction".to_string()),
    };
    *items = replacement;
    Ok((artifact, summary))
}

fn truncate_utf8_responses(value: &str, max_bytes: usize) -> &str {
    if value.len() <= max_bytes {
        return value;
    }
    let mut boundary = max_bytes;
    while boundary > 0 && !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    &value[..boundary]
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
    pub yielded: bool,
    pub attention: Option<BrainWakeAttention>,
    pub continuation_state: Option<BrainContinuationPayload>,
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
        yielded: false,
        attention: None,
        continuation_state: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn yield_responses_wake(
    request: &BrainWakeRequest,
    config: &ResponsesBrainConfig,
    items: Vec<BrainWakeStreamItem>,
    sink: &mut BrainWakeItemSink<'_>,
    base_history: &ResponsesReplayProjection,
    continuation_items: Vec<ResponsesInputItem>,
    committed_output_items: Vec<ResponsesOutputItem>,
    last_response_id: Option<String>,
    last_usage: Option<ResponsesTokenUsage>,
    no_progress_state: BrainNoProgressState,
    output_continuation: ResponsesOutputContinuationState,
    provider_state: Option<BrainWakeProviderStateInput>,
    provider_state_absence: Option<ProviderStateAbsenceReason>,
    metrics: ResponsesTransportMetricsBuilder,
) -> ResponsesBrainWakeResult {
    let continuation_state = ResponsesContinuationStateV1 {
        strategy: config.strategy,
        base_history: ResponsesContinuationProjection::from(base_history),
        continuation_items: continuation_items
            .iter()
            .map(ResponsesContinuationInputItem::from)
            .collect(),
        committed_output_items,
        last_response_id,
        last_usage,
        no_progress_state,
        output_continuation,
        provider_state,
        provider_state_absence,
        metrics: metrics.checkpoint(),
    };
    let continuation_state = match responses_continuation_output(continuation_state) {
        Ok(state) => state,
        Err(error) => {
            return failed_result(request, items, error, metrics.finish(), sink);
        }
    };
    ResponsesBrainWakeResult {
        stream: BrainWakeStream::from_items(items),
        provider_state: None,
        transport_metrics: metrics.finish(),
        yielded: true,
        attention: None,
        continuation_state: Some(continuation_state),
    }
}

#[allow(clippy::too_many_arguments)]
fn attention_responses_wake(
    request: &BrainWakeRequest,
    config: &ResponsesBrainConfig,
    items: Vec<BrainWakeStreamItem>,
    sink: &mut BrainWakeItemSink<'_>,
    base_history: &ResponsesReplayProjection,
    continuation_items: Vec<ResponsesInputItem>,
    committed_output_items: Vec<ResponsesOutputItem>,
    last_response_id: Option<String>,
    last_usage: Option<ResponsesTokenUsage>,
    no_progress_state: BrainNoProgressState,
    output_continuation: ResponsesOutputContinuationState,
    provider_state: Option<BrainWakeProviderStateInput>,
    provider_state_absence: Option<ProviderStateAbsenceReason>,
    metrics: ResponsesTransportMetricsBuilder,
    attention: BrainWakeAttention,
) -> ResponsesBrainWakeResult {
    let continuation_state = ResponsesContinuationStateV1 {
        strategy: config.strategy,
        base_history: ResponsesContinuationProjection::from(base_history),
        continuation_items: continuation_items
            .iter()
            .map(ResponsesContinuationInputItem::from)
            .collect(),
        committed_output_items,
        last_response_id,
        last_usage,
        no_progress_state,
        output_continuation,
        provider_state,
        provider_state_absence,
        metrics: metrics.checkpoint(),
    };
    let continuation_state = match responses_continuation_output(continuation_state) {
        Ok(state) => state,
        Err(error) => {
            return failed_result(request, items, error, metrics.finish(), sink);
        }
    };
    ResponsesBrainWakeResult {
        stream: BrainWakeStream::from_items(items),
        provider_state: None,
        transport_metrics: metrics.finish(),
        yielded: false,
        attention: Some(attention),
        continuation_state: Some(continuation_state),
    }
}

fn responses_continuation_output(
    state: ResponsesContinuationStateV1,
) -> Result<BrainContinuationPayload, ResponsesStreamError> {
    let payload = serde_json::to_value(state)
        .map_err(|error| ResponsesStreamError::ContinuationCheckpointFailed(error.to_string()))?;
    let payload_fingerprint = responses_json_fingerprint(&payload)
        .map_err(|error| ResponsesStreamError::ContinuationCheckpointFailed(error.to_string()))?;
    Ok(BrainContinuationPayload {
        module_id: MODULE_ID.to_string(),
        payload_version: CONTINUATION_PAYLOAD_VERSION.to_string(),
        payload_fingerprint,
        payload,
    })
}

fn responses_continuation_state(
    payload: &BrainContinuationPayload,
) -> Result<ResponsesContinuationStateV1, ResponsesStreamError> {
    if payload.module_id != MODULE_ID {
        return Err(ResponsesStreamError::ContinuationStateInvalid(format!(
            "continuation module {} does not match {MODULE_ID}",
            payload.module_id
        )));
    }
    if payload.payload_version != CONTINUATION_PAYLOAD_VERSION {
        return Err(ResponsesStreamError::ContinuationStateInvalid(format!(
            "unsupported continuation payload version {}",
            payload.payload_version
        )));
    }
    let fingerprint = responses_json_fingerprint(&payload.payload)
        .map_err(|error| ResponsesStreamError::ContinuationStateInvalid(error.to_string()))?;
    if fingerprint != payload.payload_fingerprint {
        return Err(ResponsesStreamError::ContinuationStateInvalid(
            "continuation payload fingerprint mismatch".to_string(),
        ));
    }
    let state: ResponsesContinuationStateV1 = serde_json::from_value(payload.payload.clone())
        .map_err(|error| ResponsesStreamError::ContinuationStateInvalid(error.to_string()))?;
    ResponsesBrainStrategy::from_strategy_id(&state.metrics.selected_strategy_id)?;
    ResponsesBrainStrategy::from_strategy_id(&state.metrics.effective_strategy_id)?;
    if state.strategy.strategy_id() != state.metrics.selected_strategy_id {
        return Err(ResponsesStreamError::ContinuationStateInvalid(
            "continuation strategy and metrics selection disagree".to_string(),
        ));
    }
    Ok(state)
}

fn responses_json_fingerprint(value: &Value) -> Result<String, serde_json::Error> {
    let bytes = serde_json::to_vec(value)?;
    let mut digest = Sha256::new();
    digest.update(bytes);
    Ok(format!("{:x}", digest.finalize()))
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
    mut transport_metrics: ResponsesTransportMetrics,
    sink: &mut BrainWakeItemSink<'_>,
) -> ResponsesBrainWakeResult {
    transport_metrics.terminal_failure_reason_code = Some(error.reason_code().to_string());
    transport_metrics.terminal_failure_source = Some(error.source().to_string());
    push_stream_item(
        &mut items,
        event(
            request,
            BrainEvent::ProviderStatus {
                level: BrainProviderStatusLevel::Error,
                message: error.to_string(),
                metadata_json: Some(
                    json!({
                        "reasonCode": error.reason_code(),
                        "source": error.source(),
                        "providerRequestCount": transport_metrics.provider_request_count,
                        "continuationRoundCount": transport_metrics.continuation_round_count,
                    })
                    .to_string(),
                ),
            },
        ),
        sink,
    );
    push_stream_item(
        &mut items,
        BrainWakeStreamItem::wake_failed(BrainWakeFailure {
            wake_id: request.wake_id.clone(),
            session_id: request.session_id.clone(),
            kind: CoreErrorKind::BrainUnavailable,
            reason_code: Some(error.reason_code().to_string()),
            message: error.to_string(),
        }),
        sink,
    );
    ResponsesBrainWakeResult {
        stream: BrainWakeStream::from_items(items),
        provider_state: None,
        transport_metrics,
        yielded: false,
        attention: None,
        continuation_state: None,
    }
}

fn responses_no_progress_correction_event(
    request: &BrainWakeRequest,
    tool_name: &str,
    consecutive_samples: u32,
    attention_threshold: u32,
) -> BrainWakeStreamItem {
    event(
        request,
        BrainEvent::ProviderStatus {
            level: BrainProviderStatusLevel::Degraded,
            message: format!(
                "Function {tool_name} repeated an equivalent failed result; corrective guidance was returned to the model."
            ),
            metadata_json: Some(
                json!({
                    "kind": "tool_no_progress_correction",
                    "tool_name": tool_name,
                    "consecutive_samples": consecutive_samples,
                    "attention_threshold": attention_threshold,
                })
                .to_string(),
            ),
        },
    )
}

fn responses_no_progress_attention_event(
    request: &BrainWakeRequest,
    reason_code: &str,
    summary: &str,
    consecutive_samples: u32,
) -> BrainWakeStreamItem {
    event(
        request,
        BrainEvent::ProviderStatus {
            level: BrainProviderStatusLevel::Degraded,
            message: summary.to_string(),
            metadata_json: Some(
                json!({
                    "kind": "logical_turn_attention_required",
                    "reason_code": reason_code,
                    "consecutive_no_progress_samples": consecutive_samples,
                })
                .to_string(),
            ),
        },
    )
}

fn responses_no_progress_attention(
    reason_code: impl Into<String>,
    summary: impl Into<String>,
    consecutive_no_progress_samples: u32,
) -> BrainWakeAttention {
    BrainWakeAttention {
        reason: LogicalTurnAttentionReason::NoProgress,
        reason_code: reason_code.into(),
        summary: summary.into(),
        evidence_refs: Vec::new(),
        resolution_actions: vec![
            LogicalTurnResolutionAction::RetryProviderOperation,
            LogicalTurnResolutionAction::Cancel,
        ],
        retry_unchanged_safe: false,
        consecutive_no_progress_samples,
    }
}

fn progress_fingerprint(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.len().to_le_bytes());
        hasher.update(part.as_bytes());
    }
    format!("{:x}", hasher.finalize())
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
        catalog_revision: Some("openai-responses-neutral-tools".to_string()),
        debug_detail_id: None,
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
                state_fingerprint: String::new(),
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
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn delayed_responses_server(delay: Duration) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test provider");
        let address = listener.local_addr().expect("test provider address");
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept provider request");
            let mut request = Vec::new();
            let mut buffer = [0_u8; 1024];
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                let read = stream.read(&mut buffer).expect("read provider request");
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
            }
            thread::sleep(delay);
            let body = concat!(
                "data: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}\n\n",
                "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\"}}\n\n",
                "data: [DONE]\n\n"
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
        });
        format!("http://{address}/v1")
    }

    fn live_responses_request() -> ResponsesRequest {
        ResponsesRequest {
            model: "test-model".to_string(),
            instructions: None,
            previous_response_id: None,
            input: Vec::new(),
            tools: Vec::new(),
            tool_choice: None,
            parallel_tool_calls: true,
            reasoning: None,
            store: false,
            stream: true,
            include: Vec::new(),
            service_tier: None,
            prompt_cache_key: None,
            max_output_tokens: None,
            text: None,
        }
    }

    #[test]
    fn live_provider_has_no_request_deadline_by_default() {
        let base_url = delayed_responses_server(Duration::from_millis(100));
        let mut client =
            LiveResponsesClient::new(base_url, None, None, ProviderCancellation::default())
                .expect("create live client");

        let events = client
            .stream(live_responses_request())
            .expect("uncapped provider request should complete");

        assert!(events
            .iter()
            .any(|event| matches!(event, ResponsesEvent::TextDelta(text) if text == "ok")));
    }

    #[test]
    fn configured_provider_request_deadline_remains_available() {
        let base_url = delayed_responses_server(Duration::from_millis(200));
        let mut client =
            LiveResponsesClient::new(base_url, None, Some(50), ProviderCancellation::default())
                .expect("create live client");

        assert_eq!(
            client.stream(live_responses_request()),
            Err(ResponsesStreamError::RequestTimeout)
        );
    }

    #[test]
    fn cancellation_interrupts_an_uncapped_provider_request() {
        let base_url = delayed_responses_server(Duration::from_secs(2));
        let cancellation = ProviderCancellation::default();
        let cancel_from_thread = cancellation.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            cancel_from_thread.cancel();
        });
        let mut client = LiveResponsesClient::new(base_url, None, None, cancellation)
            .expect("create live client");
        let started_at = Instant::now();

        assert_eq!(
            client.stream(live_responses_request()),
            Err(ResponsesStreamError::Cancelled)
        );
        assert!(
            started_at.elapsed() < Duration::from_millis(500),
            "cancellation should interrupt the active HTTP future promptly"
        );
    }

    #[test]
    fn request_builder_adapts_neutral_tools_and_provider_state() {
        let config = ResponsesBrainConfig {
            instructions: Some("be useful".to_string()),
            reasoning: Some(ResponsesReasoningConfig {
                effort: Some("medium".to_string()),
                summary: Some("auto".to_string()),
            }),
            text: Some(ResponsesTextConfig {
                verbosity: Some("low".to_string()),
            }),
            include: vec!["reasoning.encrypted_content".to_string()],
            service_tier: Some("default".to_string()),
            prompt_cache_key: Some("profile-cache".to_string()),
            max_output_tokens: Some(2048),
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
        assert_eq!(request.tool_choice, Some(json!("auto")));
        assert_eq!(
            serde_json::to_value(&request).unwrap()["tool_choice"],
            json!("auto")
        );
        assert_eq!(request.input.len(), 2);
        assert_eq!(request.reasoning.as_ref().unwrap()["effort"], "medium");
        assert_eq!(request.max_output_tokens, Some(2048));
        assert_eq!(request.text.as_ref().unwrap()["verbosity"], "low");
        assert!(request.stream);
    }

    #[test]
    fn zero_tool_basic_chat_request_omits_tool_choice() {
        for configured_choice in [
            ResponsesToolChoice::Auto,
            ResponsesToolChoice::None,
            ResponsesToolChoice::Function {
                name: "unavailable_tool".to_string(),
            },
        ] {
            let mut config = ResponsesBrainConfig::replay("grok-4.5");
            config.tool_choice = configured_choice;
            let request = ResponsesRequestBuilder::new(config).build(
                &wake_request(None, None),
                None,
                ResponsesReplayProjection {
                    input_items: vec![ResponsesInputItem::UserMessage {
                        content: "continue the scene".to_string(),
                    }],
                    replay_hints: Vec::new(),
                },
                Vec::new(),
            );

            assert!(request.tools.is_empty());
            assert_eq!(request.tool_choice, None);
            let payload = serde_json::to_value(request).expect("basic_chat request json");
            assert_eq!(payload["tools"], json!([]));
            assert!(payload.get("tool_choice").is_none());
        }
    }

    #[test]
    fn request_builder_distinguishes_efforts_and_omits_provider_default() {
        fn request_value(effort: Option<&str>) -> Value {
            let mut config = ResponsesBrainConfig::replay("gpt-5");
            config.reasoning = effort.map(|effort| ResponsesReasoningConfig {
                effort: Some(effort.to_string()),
                summary: None,
            });
            let request = ResponsesRequestBuilder::new(config).build(
                &wake_request(None, None),
                None,
                ResponsesReplayProjection {
                    input_items: vec![ResponsesInputItem::UserMessage {
                        content: "compare effort".to_string(),
                    }],
                    replay_hints: Vec::new(),
                },
                Vec::new(),
            );
            serde_json::to_value(request).expect("request json")
        }

        let provider_default = request_value(None);
        let low = request_value(Some("low"));
        let high = request_value(Some("high"));

        assert!(provider_default.get("reasoning").is_none());
        assert_eq!(low["reasoning"]["effort"], "low");
        assert_eq!(high["reasoning"]["effort"], "high");
        assert_ne!(low, high);
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
    fn stateless_replay_strips_provider_item_ids_from_replayed_items() {
        let builder = ResponsesRequestBuilder::new(ResponsesBrainConfig::replay("gpt-5"));
        let state = provider_state(provider_state_payload(
            "resp-typed",
            vec![
                ResponsesOutputItem::Reasoning {
                    id: Some("rs_ephemeral".to_string()),
                    summary: Some("kept as reasoning".to_string()),
                    encrypted_content: Some("opaque".to_string()),
                },
                ResponsesOutputItem::FunctionCall {
                    id: Some("fc_ephemeral".to_string()),
                    call_id: "call-1".to_string(),
                    name: "lookup".to_string(),
                    arguments: "{\"q\":\"rust\"}".to_string(),
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

        let replayed_reasoning = request
            .input
            .iter()
            .find_map(|item| match item {
                ResponsesInputItem::Reasoning { id, .. } => Some(id),
                _ => None,
            })
            .expect("reasoning item should be replayed");
        assert_eq!(replayed_reasoning, &None);

        let replayed_call = request
            .input
            .iter()
            .find_map(|item| match item {
                ResponsesInputItem::FunctionCall { id, call_id, .. } if call_id == "call-1" => {
                    Some(id)
                }
                _ => None,
            })
            .expect("function call should be replayed");
        assert_eq!(replayed_call, &None);
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
                .provider_request_debug_samples
                .len(),
            1,
            "one exact provider request sample should be cached in metrics"
        );
        assert_eq!(
            result.transport_metrics.provider_request_debug_samples[0]
                .get("stream")
                .and_then(Value::as_bool),
            Some(true)
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
                state_fingerprint: String::new(),
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
    fn repeated_identical_successful_function_calls_continue() {
        let repeated_call = || {
            Ok(vec![
                ResponsesEvent::OutputItemDone(ResponsesOutputItem::FunctionCall {
                    id: None,
                    call_id: "call-repeat".to_string(),
                    name: "lookup".to_string(),
                    arguments: r#"{"query":"same"}"#.to_string(),
                }),
                ResponsesEvent::Completed {
                    response_id: "resp-repeat".to_string(),
                    usage: None,
                },
            ])
        };
        let client = FakeResponsesClient::new(vec![
            repeated_call(),
            repeated_call(),
            repeated_call(),
            repeated_call(),
            Ok(vec![
                ResponsesEvent::TextDelta("repeated work completed".to_string()),
                ResponsesEvent::Completed {
                    response_id: "resp-final".to_string(),
                    usage: None,
                },
            ]),
        ])
        .expect_function_output("call-repeat")
        .expect_function_output("call-repeat")
        .expect_function_output("call-repeat")
        .expect_function_output("call-repeat");
        let mut brain = brain_with(
            client,
            MapToolExecutor::new([(
                "lookup".to_string(),
                NeutralToolOutput {
                    output: "same answer".to_string(),
                    is_error: false,
                    state_fingerprint: String::new(),
                },
            )]),
        );
        let result = brain.wake(wake_request(None, None)).unwrap();
        let items = result.stream.drain_until_terminal().unwrap();
        assert!(items.iter().any(|item| matches!(
            item,
            BrainWakeStreamItem::Event { event }
                if event.event == BrainEvent::TextDelta { text: "repeated work completed".to_string() }
        )));
        assert!(result.attention.is_none());
        assert!(result.provider_state.is_some());
    }

    #[test]
    fn repeated_identical_failed_function_calls_require_attention() {
        let repeated_call = |call_id: &str, response_id: &str| {
            Ok(vec![
                ResponsesEvent::OutputItemDone(ResponsesOutputItem::FunctionCall {
                    id: None,
                    call_id: call_id.to_string(),
                    name: "lookup".to_string(),
                    arguments: r#"{"query":"same"}"#.to_string(),
                }),
                ResponsesEvent::Completed {
                    response_id: response_id.to_string(),
                    usage: None,
                },
            ])
        };
        let client = FakeResponsesClient::new(vec![
            repeated_call("call-repeat", "resp-1"),
            repeated_call("call-repeat", "resp-2"),
            repeated_call("call-repeat", "resp-3"),
            repeated_call("call-repeat", "resp-4"),
        ])
        .expect_function_output("call-repeat")
        .expect_function_output("call-repeat")
        .expect_function_output("call-repeat")
        .expect_function_output("call-repeat");
        let mut brain = brain_with(
            client,
            MapToolExecutor::new([(
                "lookup".to_string(),
                NeutralToolOutput {
                    output: "dependency unavailable".to_string(),
                    is_error: true,
                    state_fingerprint: String::new(),
                },
            )]),
        );

        let result = brain.wake(wake_request(None, None)).unwrap();
        let items = result.stream.drain_until_closed().unwrap();

        assert!(
            items
                .iter()
                .all(|item| !matches!(item, BrainWakeStreamItem::WakeFailed { .. })),
            "attention path emitted failure items: {items:?}"
        );
        let attention = result.attention.expect("operator attention");
        assert_eq!(attention.reason_code, "responses_tool_no_progress");
        assert_eq!(attention.consecutive_no_progress_samples, 3);
        assert!(result.continuation_state.is_some());
    }

    #[test]
    fn repeated_failed_function_calls_continue_when_host_state_changes() {
        #[derive(Debug)]
        struct StateAdvancingTool {
            outputs: std::sync::Mutex<VecDeque<NeutralToolOutput>>,
        }

        impl NeutralToolExecutor for StateAdvancingTool {
            fn execute(&self, _call: &PendingResponsesFunctionCall) -> NeutralToolOutput {
                self.outputs
                    .lock()
                    .expect("state tool mutex")
                    .pop_front()
                    .expect("scripted state output")
            }
        }

        let repeated_call = |call_id: &str, response_id: &str| {
            Ok(vec![
                ResponsesEvent::OutputItemDone(ResponsesOutputItem::FunctionCall {
                    id: None,
                    call_id: call_id.to_string(),
                    name: "lookup".to_string(),
                    arguments: r#"{"query":"same"}"#.to_string(),
                }),
                ResponsesEvent::Completed {
                    response_id: response_id.to_string(),
                    usage: None,
                },
            ])
        };
        let client = FakeResponsesClient::new(vec![
            repeated_call("call-repeat", "resp-1"),
            repeated_call("call-repeat", "resp-2"),
            repeated_call("call-repeat", "resp-3"),
            repeated_call("call-repeat", "resp-4"),
            Ok(vec![
                ResponsesEvent::TextDelta("recovered after state changes".into()),
                ResponsesEvent::Completed {
                    response_id: "resp-final".into(),
                    usage: None,
                },
            ]),
        ])
        .expect_function_output("call-repeat")
        .expect_function_output("call-repeat")
        .expect_function_output("call-repeat")
        .expect_function_output("call-repeat");
        let tool = StateAdvancingTool {
            outputs: std::sync::Mutex::new(
                (1..=4)
                    .map(|revision| NeutralToolOutput {
                        output: "dependency unavailable".into(),
                        is_error: true,
                        state_fingerprint: format!("resource-revision:{revision}"),
                    })
                    .collect(),
            ),
        };
        let mut brain = ResponsesReplayBrain::new(
            client,
            tool,
            ResponsesBrainConfig::replay("gpt-5"),
            vec![NeutralBrainTool {
                name: "lookup".into(),
                description: "Look up data".into(),
                input_schema: json!({"type":"object"}),
            }],
        );

        let result = brain.wake(wake_request(None, None)).unwrap();
        let items = result.stream.drain_until_closed().unwrap();

        assert!(result.attention.is_none());
        assert!(result.provider_state.is_some());
        assert!(items.iter().any(|item| matches!(
            item,
            BrainWakeStreamItem::Event { event }
                if event.event == BrainEvent::TextDelta {
                    text: "recovered after state changes".into()
                }
        )));
    }

    #[test]
    fn long_multi_tool_replay_finishes_with_reasoning_and_output_policy_intact() {
        const TOOL_CALL_COUNT: usize = 20;
        let mut scripts = Vec::new();
        for index in 0..TOOL_CALL_COUNT {
            scripts.push(Ok(vec![
                ResponsesEvent::OutputItemDone(ResponsesOutputItem::FunctionCall {
                    id: Some(format!("item-{index}")),
                    call_id: format!("call-{index}"),
                    name: "lookup".to_string(),
                    arguments: format!(r#"{{"step":{index}}}"#),
                }),
                ResponsesEvent::Completed {
                    response_id: format!("response-{index}"),
                    usage: None,
                },
            ]));
        }
        scripts.push(Ok(vec![
            ResponsesEvent::TextDelta("long replay complete".to_string()),
            ResponsesEvent::Completed {
                response_id: "response-final".to_string(),
                usage: None,
            },
        ]));
        let mut client = FakeResponsesClient::new(scripts);
        for _ in 0..TOOL_CALL_COUNT {
            client = client.expect_function_output("call-0");
        }
        let mut config = ResponsesBrainConfig::replay("gpt-5");
        config.reasoning = Some(ResponsesReasoningConfig {
            effort: Some("high".to_string()),
            summary: None,
        });
        config.max_output_tokens = Some(8192);
        let mut brain = brain_with_config(
            client,
            MapToolExecutor::new([(
                "lookup".to_string(),
                NeutralToolOutput {
                    output: "evidence".to_string(),
                    is_error: false,
                    state_fingerprint: String::new(),
                },
            )]),
            config,
        );

        let result = brain.wake(wake_request(None, None)).unwrap();
        let items = result.stream.drain_until_terminal().unwrap();
        let completed_calls = items
            .iter()
            .filter(|item| {
                matches!(
                    item,
                    BrainWakeStreamItem::Event { event }
                        if matches!(event.event, BrainEvent::ToolCallFinished { .. })
                )
            })
            .count();
        assert_eq!(completed_calls, TOOL_CALL_COUNT);
        assert!(matches!(
            items.last(),
            Some(BrainWakeStreamItem::Actions { .. })
        ));
        assert_eq!(result.transport_metrics.provider_request_count, 21);
        assert_eq!(result.transport_metrics.continuation_round_count, 20);
        assert_eq!(result.transport_metrics.terminal_failure_reason_code, None);
        assert!(brain.client.requests().iter().all(|request| {
            request.max_output_tokens == Some(8192)
                && request.reasoning == Some(json!({"effort": "high"}))
        }));
    }

    #[test]
    fn continuation_quantum_preserves_previous_response_strategy_without_repeating_tools() {
        let tool_call = |index| {
            Ok(vec![
                ResponsesEvent::OutputItemDone(ResponsesOutputItem::FunctionCall {
                    id: Some(format!("item-{index}")),
                    call_id: format!("call-{index}"),
                    name: "lookup".to_string(),
                    arguments: format!(r#"{{"step":{index}}}"#),
                }),
                ResponsesEvent::Completed {
                    response_id: format!("response-{index}"),
                    usage: None,
                },
            ])
        };
        let client = FakeResponsesClient::new(vec![
            tool_call(0),
            tool_call(1),
            Ok(vec![
                ResponsesEvent::TextDelta("continued to completion".to_string()),
                ResponsesEvent::Completed {
                    response_id: "response-final".to_string(),
                    usage: Some(ResponsesTokenUsage {
                        input_tokens: 30,
                        cached_input_tokens: 10,
                        output_tokens: 5,
                        reasoning_output_tokens: 2,
                        total_tokens: 35,
                    }),
                },
            ]),
        ])
        .expect_function_output("call-0")
        .expect_function_output("call-0");
        let mut config = ResponsesBrainConfig::previous_response_chain("gpt-5");
        config.work_quantum_continuation_rounds = 1;
        let mut brain = brain_with_config(
            client,
            MapToolExecutor::new([(
                "lookup".to_string(),
                NeutralToolOutput {
                    output: "evidence".to_string(),
                    is_error: false,
                    state_fingerprint: String::new(),
                },
            )]),
            config,
        );

        let history = ResponsesReplayProjection {
            input_items: vec![ResponsesInputItem::UserMessage {
                content: "original frozen request".to_string(),
            }],
            replay_hints: Vec::new(),
        };
        let first = brain
            .wake_with_history(wake_request(None, None), history)
            .unwrap();
        assert!(first.yielded);
        assert!(first.provider_state.is_none());
        assert_eq!(
            first.transport_metrics.selected_strategy_id,
            PREVIOUS_RESPONSE_CHAIN_STRATEGY_ID
        );
        assert_eq!(
            first.transport_metrics.effective_strategy_id,
            REPLAY_STRATEGY_ID
        );
        assert!(!first
            .stream
            .drain_until_closed()
            .unwrap()
            .iter()
            .any(BrainWakeStreamItem::is_terminal));

        let replacement_provider_state = provider_state(provider_state_payload(
            "replacement-response",
            vec![ResponsesOutputItem::Message {
                id: None,
                text: "replacement provider state".to_string(),
            }],
        ));
        let mut second_request = wake_request(Some(replacement_provider_state), None);
        second_request.continuation_state = first.continuation_state;
        let second = brain
            .wake_with_history(
                second_request,
                ResponsesReplayProjection {
                    input_items: vec![ResponsesInputItem::UserMessage {
                        content: "replacement body state".to_string(),
                    }],
                    replay_hints: Vec::new(),
                },
            )
            .unwrap();
        assert!(second.yielded);
        assert_eq!(second.transport_metrics.provider_request_count, 2);
        assert_eq!(second.transport_metrics.continuation_round_count, 2);
        assert!(!second
            .stream
            .drain_until_closed()
            .unwrap()
            .iter()
            .any(BrainWakeStreamItem::is_terminal));

        let mut final_request = wake_request(None, None);
        final_request.continuation_state = second.continuation_state;
        let completed = brain
            .wake_with_history(final_request, ResponsesReplayProjection::default())
            .unwrap();
        let terminal_items = completed.stream.drain_until_terminal().unwrap();
        assert!(!completed.yielded);
        assert_eq!(completed.transport_metrics.provider_request_count, 3);
        assert_eq!(completed.transport_metrics.continuation_round_count, 2);
        assert!(matches!(
            completed.provider_state.as_ref(),
            Some(BrainWakeProviderStateOutput::Replace { state })
                if state.strategy_id == PREVIOUS_RESPONSE_CHAIN_STRATEGY_ID
        ));
        assert!(matches!(
            terminal_items.last(),
            Some(BrainWakeStreamItem::Actions { .. })
        ));
        assert!(terminal_items.iter().any(|item| matches!(
            item,
            BrainWakeStreamItem::Event { event }
                if matches!(&event.event, BrainEvent::TextDelta { text } if text == "continued to completion")
        )));
        assert!(brain.client.requests().iter().all(|request| {
            !request.input.iter().any(|item| {
                matches!(
                    item,
                    ResponsesInputItem::UserMessage { content }
                        if content == "replacement body state"
                )
            })
        }));
        let tool_call_ids = brain
            .client
            .requests()
            .iter()
            .flat_map(|request| request.input.iter())
            .filter_map(|item| match item {
                ResponsesInputItem::FunctionCallOutput { call_id, .. } => Some(call_id.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            tool_call_ids,
            vec!["call-0", "call-0", "call-1"],
            "each resumed request should contain one copy of every completed function result"
        );
    }

    #[test]
    #[ignore = "focused >512-round continuation certification"]
    fn continuation_quantum_completes_over_512_rounds_without_duplicate_tools() {
        const TOOL_ROUNDS: usize = 513;
        const WORK_QUANTUM: usize = 7;

        let mut scripts = (0..TOOL_ROUNDS)
            .map(|index| {
                Ok(vec![
                    ResponsesEvent::OutputItemDone(ResponsesOutputItem::FunctionCall {
                        id: Some(format!("item-{index}")),
                        call_id: format!("call-{index}"),
                        name: "lookup".to_string(),
                        arguments: format!(r#"{{"step":{index}}}"#),
                    }),
                    ResponsesEvent::Completed {
                        response_id: format!("response-{index}"),
                        usage: None,
                    },
                ])
            })
            .collect::<Vec<_>>();
        scripts.push(Ok(vec![
            ResponsesEvent::TextDelta("513-round Responses turn complete".to_string()),
            ResponsesEvent::Completed {
                response_id: "response-final".to_string(),
                usage: None,
            },
        ]));
        let mut client = FakeResponsesClient::new(scripts);
        for _ in 0..TOOL_ROUNDS {
            client = client.expect_function_output("call-0");
        }
        let mut config = ResponsesBrainConfig::replay("gpt-5");
        config.work_quantum_continuation_rounds = WORK_QUANTUM;
        let mut brain = brain_with_config(
            client,
            MapToolExecutor::new([(
                "lookup".to_string(),
                NeutralToolOutput {
                    output: "evidence".to_string(),
                    is_error: false,
                    state_fingerprint: String::new(),
                },
            )]),
            config,
        );

        let mut continuation_state = None;
        let mut yielded_epochs = 0usize;
        let mut streamed_tool_finishes = 0usize;
        let completed = loop {
            let mut request = wake_request(None, None);
            request.continuation_state = continuation_state;
            let output = brain.wake(request).unwrap();
            let items = if output.yielded {
                output.stream.drain_until_closed().unwrap()
            } else {
                output.stream.drain_until_terminal().unwrap()
            };
            streamed_tool_finishes += items
                .iter()
                .filter(|item| {
                    matches!(
                        item,
                        BrainWakeStreamItem::Event { event }
                            if matches!(event.event, BrainEvent::ToolCallFinished { .. })
                    )
                })
                .count();
            if output.yielded {
                assert!(!items.iter().any(BrainWakeStreamItem::is_terminal));
                yielded_epochs += 1;
                continuation_state = output.continuation_state;
                continue;
            }
            break (output, items);
        };

        assert_eq!(completed.0.transport_metrics.continuation_round_count, 513);
        assert_eq!(completed.0.transport_metrics.provider_request_count, 514);
        assert_eq!(streamed_tool_finishes, TOOL_ROUNDS);
        assert!(yielded_epochs > 64);
        assert_eq!(
            completed.0.transport_metrics.terminal_failure_reason_code,
            None
        );
        assert!(matches!(
            completed.1.last(),
            Some(BrainWakeStreamItem::Actions { .. })
        ));
        assert!(completed.1.iter().any(|item| matches!(
            item,
            BrainWakeStreamItem::Event { event }
                if matches!(&event.event, BrainEvent::TextDelta { text } if text == "513-round Responses turn complete")
        )));
    }

    #[test]
    fn provider_failure_and_closed_stream_do_not_commit_provider_state() {
        for script in [
            Ok(vec![ResponsesEvent::Failed("rate limited".to_string())]),
            Err(ResponsesStreamError::RequestTimeout),
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
    fn incomplete_response_yields_and_resumes_without_duplicate_text_or_reasoning() {
        let mut brain = brain_with(
            FakeResponsesClient::new(vec![
                Ok(vec![
                    ResponsesEvent::ReasoningDelta("checking the tree".to_string()),
                    ResponsesEvent::TextDelta("partial answer".to_string()),
                    ResponsesEvent::Incomplete("max output tokens".to_string()),
                ]),
                Ok(vec![
                    ResponsesEvent::ReasoningDelta("checking the tree".to_string()),
                    ResponsesEvent::TextDelta("partial answer".to_string()),
                    ResponsesEvent::TextDelta(" completed".to_string()),
                    ResponsesEvent::Completed {
                        response_id: "resp-complete".to_string(),
                        usage: None,
                    },
                ]),
            ]),
            MapToolExecutor::default(),
        );

        let first = brain.wake(wake_request(None, None)).unwrap();
        assert!(first.yielded);
        assert!(first.continuation_state.is_some());
        let mut streamed = first.stream.drain_until_closed().unwrap();
        let mut resumed_request = wake_request(None, None);
        resumed_request.continuation_state = first.continuation_state;
        let second = brain.wake(resumed_request).unwrap();
        assert!(!second.yielded);
        streamed.extend(second.stream.drain_until_terminal().unwrap());

        let text = streamed
            .iter()
            .filter_map(|item| match item {
                BrainWakeStreamItem::Event { event } => match &event.event {
                    BrainEvent::TextDelta { text } => Some(text.as_str()),
                    _ => None,
                },
                _ => None,
            })
            .collect::<String>();
        let reasoning = streamed
            .iter()
            .filter_map(|item| match item {
                BrainWakeStreamItem::Event { event } => match &event.event {
                    BrainEvent::ReasoningDelta { text, .. } => Some(text.as_str()),
                    _ => None,
                },
                _ => None,
            })
            .collect::<String>();
        assert_eq!(text, "partial answer completed");
        assert_eq!(reasoning, "checking the tree");
        assert!(brain.client.requests()[1]
            .instructions
            .as_deref()
            .is_some_and(|instructions| instructions.contains("Continue exactly where")));
        assert!(!streamed
            .iter()
            .any(|item| matches!(item, BrainWakeStreamItem::WakeFailed { .. })));
    }

    #[test]
    fn repeated_incomplete_response_requires_attention_instead_of_failing() {
        let incomplete = || {
            Ok(vec![
                ResponsesEvent::TextDelta("same partial".to_string()),
                ResponsesEvent::Incomplete("max output tokens".to_string()),
            ])
        };
        let mut brain = brain_with(
            FakeResponsesClient::new(vec![incomplete(), incomplete(), incomplete(), incomplete()]),
            MapToolExecutor::default(),
        );
        let mut continuation_state = None;
        let mut attention = None;
        let mut streamed = Vec::new();
        for _ in 0..4 {
            let mut request = wake_request(None, None);
            request.continuation_state = continuation_state;
            let output = brain.wake(request).unwrap();
            streamed.extend(output.stream.drain_until_closed().unwrap());
            continuation_state = output.continuation_state;
            if output.attention.is_some() {
                attention = output.attention;
                break;
            }
        }

        let attention = attention.expect("repeated output exhaustion attention");
        assert_eq!(attention.reason_code, "responses_output_limit_no_progress");
        assert!(attention
            .resolution_actions
            .contains(&LogicalTurnResolutionAction::RetryProviderOperation));
        assert!(attention
            .resolution_actions
            .contains(&LogicalTurnResolutionAction::Cancel));
        assert_eq!(
            streamed
                .iter()
                .filter(|item| matches!(
                    item,
                    BrainWakeStreamItem::Event { event }
                        if matches!(&event.event, BrainEvent::TextDelta { text } if text == "same partial")
                ))
                .count(),
            1
        );
        assert!(!streamed
            .iter()
            .any(|item| matches!(item, BrainWakeStreamItem::WakeFailed { .. })));
    }

    #[test]
    fn completed_tool_call_at_output_limit_executes_once_then_resumes() {
        let mut brain = brain_with(
            FakeResponsesClient::new(vec![
                Ok(vec![
                    ResponsesEvent::OutputItemDone(ResponsesOutputItem::FunctionCall {
                        id: Some("item-1".to_string()),
                        call_id: "call-1".to_string(),
                        name: "lookup".to_string(),
                        arguments: r#"{"query":"rust"}"#.to_string(),
                    }),
                    ResponsesEvent::Incomplete("max output tokens".to_string()),
                ]),
                Ok(vec![ResponsesEvent::Completed {
                    response_id: "resp-after-tool".to_string(),
                    usage: None,
                }]),
            ])
            .expect_function_output("call-1"),
            MapToolExecutor::new([(
                "lookup".to_string(),
                NeutralToolOutput {
                    output: "found rust".to_string(),
                    is_error: false,
                    state_fingerprint: "state-1".to_string(),
                },
            )]),
        );

        let first = brain.wake(wake_request(None, None)).unwrap();
        assert!(first.yielded);
        let first_items = first.stream.drain_until_closed().unwrap();
        let mut resumed_request = wake_request(None, None);
        resumed_request.continuation_state = first.continuation_state;
        let second = brain.wake(resumed_request).unwrap();
        let second_items = second.stream.drain_until_terminal().unwrap();
        assert!(matches!(
            second_items.last(),
            Some(BrainWakeStreamItem::Actions { .. })
        ));
        assert_eq!(
            first_items
                .iter()
                .chain(second_items.iter())
                .filter(|item| matches!(
                    item,
                    BrainWakeStreamItem::Event { event }
                        if matches!(&event.event, BrainEvent::ToolCallFinished { tool_name, .. } if tool_name == "lookup")
                ))
                .count(),
            1
        );
    }

    #[test]
    fn mid_turn_context_compaction_rebuilds_responses_replay_before_continuing() {
        let mut scripts = (1..=6)
            .map(|round| {
                Ok(vec![
                    ResponsesEvent::OutputItemDone(ResponsesOutputItem::FunctionCall {
                        id: Some(format!("item-{round}")),
                        call_id: format!("call-{round}"),
                        name: "lookup".to_string(),
                        arguments: format!(r#"{{"round":{round}}}"#),
                    }),
                    ResponsesEvent::Completed {
                        response_id: format!("response-{round}"),
                        usage: Some(ResponsesTokenUsage {
                            input_tokens: if round >= 5 { 20_000 } else { 100 },
                            cached_input_tokens: 0,
                            output_tokens: 10,
                            reasoning_output_tokens: 0,
                            total_tokens: if round >= 5 { 20_010 } else { 110 },
                        }),
                    },
                ])
            })
            .collect::<Vec<_>>();
        scripts.push(Ok(vec![
            ResponsesEvent::TextDelta("responses compaction complete".to_string()),
            ResponsesEvent::Completed {
                response_id: "response-final".to_string(),
                usage: None,
            },
        ]));
        let mut config = ResponsesBrainConfig::previous_response_chain("gpt-5");
        config.context_compaction = Some(BrainContextCompactionPolicy {
            enabled: true,
            auto_compaction_enabled: true,
            strategy_id: "rolling_summary_compaction".to_string(),
            context_window_tokens: 22_000,
            compact_at_percent: 80,
            target_percent_after_compaction: 55,
        });
        let mut client = FakeResponsesClient::new(scripts);
        for call_id in ["call-1", "call-1", "call-1", "call-1", "call-4", "call-5"] {
            client = client.expect_function_output(call_id);
        }
        let mut brain = brain_with_config(
            client,
            MapToolExecutor::new([(
                "lookup".to_string(),
                NeutralToolOutput {
                    output: format!("{}-tool-result", "x".repeat(3000)),
                    is_error: false,
                    state_fingerprint: "changing-state".to_string(),
                },
            )]),
            config,
        );

        let first = brain.wake(wake_request(None, None)).unwrap();
        assert!(
            first.yielded,
            "attention: {:?}, requests: {}",
            first.attention,
            brain.client.requests().len()
        );
        let first_items = first.stream.drain_until_closed().unwrap();
        assert!(first_items.iter().any(|item| matches!(
            item,
            BrainWakeStreamItem::Event { event }
                if matches!(&event.event, BrainEvent::ProviderStatus { metadata_json: Some(metadata), .. }
                    if metadata.contains("context_compaction_completed"))
        )));
        let checkpoint = responses_continuation_state(
            first
                .continuation_state
                .as_ref()
                .expect("Responses checkpoint"),
        )
        .expect("valid Responses checkpoint");
        assert_eq!(
            checkpoint
                .output_continuation
                .context_compaction
                .artifacts
                .len(),
            1
        );
        assert_eq!(
            checkpoint.output_continuation.context_compaction.artifacts[0]
                .provider_chain_action
                .as_deref(),
            Some("rebuild_replay_after_compaction")
        );
        assert_eq!(
            checkpoint.provider_state_absence,
            Some(ProviderStateAbsenceReason::Invalidated)
        );

        let mut resumed = wake_request(None, None);
        resumed.continuation_state = first.continuation_state;
        let second = brain.wake(resumed).unwrap();
        let second_items = second.stream.drain_until_closed().unwrap();
        assert!(second.yielded);
        let second_checkpoint = responses_continuation_state(
            second
                .continuation_state
                .as_ref()
                .expect("second Responses checkpoint"),
        )
        .expect("valid second Responses checkpoint");
        assert_eq!(
            second_checkpoint
                .output_continuation
                .context_compaction
                .artifacts
                .len(),
            2
        );

        let mut resumed = wake_request(None, None);
        resumed.continuation_state = second.continuation_state;
        let third = brain.wake(resumed).unwrap();
        let third_items = third.stream.drain_until_terminal().unwrap();
        assert!(!third.yielded);
        assert!(
            third_items.iter().any(|item| matches!(
                item,
                BrainWakeStreamItem::Event { event }
                    if matches!(&event.event, BrainEvent::TextDelta { text }
                        if text == "responses compaction complete")
            )),
            "third wake items: {third_items:#?}"
        );
        assert!(!first_items
            .iter()
            .chain(second_items.iter())
            .chain(third_items.iter())
            .any(|item| matches!(item, BrainWakeStreamItem::WakeFailed { .. })));
        assert_eq!(
            first_items
                .iter()
                .chain(second_items.iter())
                .chain(third_items.iter())
                .filter(|item| matches!(
                    item,
                    BrainWakeStreamItem::Event { event }
                        if matches!(event.event, BrainEvent::ToolCallFinished { .. })
                ))
                .count(),
            6,
        );
    }

    #[test]
    fn provider_context_rejection_pauses_compaction_turn_for_attention() {
        let mut config = ResponsesBrainConfig::replay("gpt-5");
        config.context_compaction = Some(BrainContextCompactionPolicy {
            enabled: true,
            auto_compaction_enabled: true,
            strategy_id: "rolling_summary_compaction".to_string(),
            context_window_tokens: 1_000,
            compact_at_percent: 80,
            target_percent_after_compaction: 55,
        });
        let mut brain = brain_with_config(
            FakeResponsesClient::new(vec![Err(ResponsesStreamError::Transport(
                "context_length_exceeded".to_string(),
            ))]),
            MapToolExecutor::default(),
            config,
        );

        let result = brain.wake(wake_request(None, None)).unwrap();
        let items = result.stream.drain_until_closed().unwrap();

        assert_eq!(
            result
                .attention
                .as_ref()
                .map(|value| value.reason_code.as_str()),
            Some("responses_context_compaction_attention")
        );
        assert!(result.continuation_state.is_some());
        assert!(items.iter().any(|item| matches!(
            item,
            BrainWakeStreamItem::Event { event }
                if matches!(&event.event, BrainEvent::ProviderStatus { metadata_json: Some(metadata), .. }
                    if metadata.contains("context_compaction_failed"))
        )));
        assert!(!items
            .iter()
            .any(|item| matches!(item, BrainWakeStreamItem::WakeFailed { .. })));
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
            tool_choice: None,
            parallel_tool_calls: true,
            reasoning: None,
            store: false,
            stream: true,
            include: Vec::new(),
            service_tier: None,
            prompt_cache_key: None,
            max_output_tokens: None,
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
            continuation_state: None,
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
            inference_overrides: Default::default(),
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
