//! Rust provider-wire foundation for the chat-completions brain.
//!
//! This crate intentionally stops below the full agent loop. It owns
//! OpenAI-compatible chat-completions request construction, live SSE transport,
//! and provider stream parsing. Coordination, profile loading, tool execution,
//! and service-host wiring stay outside this crate.

use reqwest::blocking::Client as BlockingHttpClient;
use reqwest::{Client as AsyncHttpClient, Response as AsyncHttpResponse};
use rusty_crew_core_protocol::{
    BrainActionBatch, BrainContinuationPayload, BrainEvent, BrainEventEnvelope,
    BrainNoProgressPolicy, BrainNoProgressState, BrainProgressDisposition,
    BrainProgressResultClass, BrainProgressSample, BrainProviderStatusLevel, BrainWakeAttention,
    BrainWakeFailure, BrainWakeProviderStateInput, BrainWakeProviderStateOutput,
    BrainWakeProviderStateUpdate, BrainWakeStreamItem, ChatCompletionsReasoningHistory,
    ChatCompletionsThinkingMode, ChatCompletionsWireDialect, CoreErrorKind,
    LogicalTurnAttentionReason, LogicalTurnResolutionAction, ModelProviderRecord, SessionId,
};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::io::Read;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use tokio::runtime::{Builder as RuntimeBuilder, Runtime};

pub const MODULE_ID: &str = "chat-completions";
pub const DEFAULT_WORK_QUANTUM_TOOL_ROUNDS: usize = 64;
pub const CONTINUATION_PAYLOAD_VERSION: &str = "chat-completions-continuation-v2";
pub const DEFAULT_NO_PROGRESS_ATTENTION_THRESHOLD: u32 = 3;
pub const DEFAULT_DEN_ROUTER_URL: &str = "http://127.0.0.1:18082";
pub const OUTPUT_LIMIT_EXCEEDED_REASON_CODE: &str = "chat_completions_output_limit_exceeded";
pub const MALFORMED_PROVIDER_STREAM_REASON_CODE: &str =
    "chat_completions_malformed_provider_stream";
pub const CANONICAL_REASONING_FORMAT: &str = "chat-completions:reasoning";
pub const PROVIDER_STATE_PAYLOAD_VERSION: &str = "chat-completions-history-v1";
const PROVIDER_STATE_TTL_MS: u64 = 24 * 60 * 60 * 1000;
const KIMI_THINKING_MIN_OUTPUT_TOKENS: u32 = 16_000;
pub const DEFAULT_DEN_ROUTER_MODEL_CANDIDATES: [&str; 4] =
    ["deepseek-flash", "grok", "glm", "local-coder"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DenRouterApi {
    OpenaiCompletions,
    OpenaiResponses,
}

impl DenRouterApi {
    pub fn parse(raw: &str) -> Result<Self, DenRouterSelectionError> {
        match raw {
            "openai-completions" => Ok(Self::OpenaiCompletions),
            "openai-responses" => Ok(Self::OpenaiResponses),
            other => Err(DenRouterSelectionError::UnsupportedApi(other.to_string())),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpenaiCompletions => "openai-completions",
            Self::OpenaiResponses => "openai-responses",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DenRouterSelectionOptions {
    pub requested_model_id: Option<String>,
    pub requested_api: Option<DenRouterApi>,
    pub max_tokens: Option<u32>,
}

impl Default for DenRouterSelectionOptions {
    fn default() -> Self {
        Self {
            requested_model_id: None,
            requested_api: None,
            max_tokens: Some(128),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DenRouterModelInfo {
    pub id: String,
    #[serde(default)]
    pub context_length: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DenRouterRoutes {
    #[serde(default)]
    pub models: HashMap<String, DenRouterRouteModel>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DenRouterRouteModel {
    #[serde(default)]
    pub backends: Vec<DenRouterRouteBackend>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DenRouterRouteBackend {
    #[serde(rename = "type", default)]
    pub backend_type: Option<String>,
    #[serde(default)]
    pub healthy: Option<bool>,
    #[serde(default)]
    pub drained: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DenRouterModelSelection {
    pub model_id: String,
    pub api: DenRouterApi,
    pub provider: String,
    pub base_url: String,
    pub reasoning: bool,
    pub context_window_tokens: u32,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DenRouterSelectionError {
    #[error("den-router /v1/models returned HTTP {0}")]
    ModelsHttpStatus(u16),
    #[error("den-router returned no models")]
    NoModels,
    #[error("den-router model {0} is not available")]
    RequestedModelUnavailable(String),
    #[error("unsupported den-router api {0}")]
    UnsupportedApi(String),
    #[error("malformed den-router {path} response: {message}")]
    MalformedResponse { path: &'static str, message: String },
    #[error("den-router transport error: {0}")]
    Transport(String),
}

pub trait DenRouterModelSource {
    fn base_url(&self) -> &str;
    fn fetch_models(&self) -> Result<Vec<DenRouterModelInfo>, DenRouterSelectionError>;
    fn fetch_routes(&self) -> Result<Option<DenRouterRoutes>, DenRouterSelectionError>;
}

pub fn resolve_den_router_model<S: DenRouterModelSource>(
    source: &S,
    options: &DenRouterSelectionOptions,
) -> Result<DenRouterModelSelection, DenRouterSelectionError> {
    let base_url = normalize_den_router_base_url(source.base_url());
    let models = source.fetch_models()?;
    if models.is_empty() {
        return Err(DenRouterSelectionError::NoModels);
    }
    let routes = source.fetch_routes().unwrap_or(None);
    let selected = select_den_router_model(&models, options.requested_model_id.as_deref())?;
    let api = options.requested_api.unwrap_or_else(|| {
        if is_codex_backed(&selected.id, routes.as_ref()) {
            DenRouterApi::OpenaiResponses
        } else {
            DenRouterApi::OpenaiCompletions
        }
    });
    Ok(DenRouterModelSelection {
        model_id: selected.id.clone(),
        api,
        provider: "den-router".to_string(),
        base_url: format!("{base_url}/v1"),
        reasoning: api == DenRouterApi::OpenaiResponses,
        context_window_tokens: selected.context_length.unwrap_or(128_000),
        max_tokens: options.max_tokens.unwrap_or(128),
    })
}

#[derive(Debug, Clone)]
pub struct LiveDenRouterModelSource {
    base_url: String,
    client: BlockingHttpClient,
}

impl LiveDenRouterModelSource {
    pub fn new(base_url: Option<String>) -> Result<Self, DenRouterSelectionError> {
        let client = BlockingHttpClient::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|error| DenRouterSelectionError::Transport(error.to_string()))?;
        Ok(Self {
            base_url: normalize_den_router_base_url(
                base_url.as_deref().unwrap_or(DEFAULT_DEN_ROUTER_URL),
            ),
            client,
        })
    }
}

impl DenRouterModelSource for LiveDenRouterModelSource {
    fn base_url(&self) -> &str {
        &self.base_url
    }

    fn fetch_models(&self) -> Result<Vec<DenRouterModelInfo>, DenRouterSelectionError> {
        let response = self
            .client
            .get(format!("{}/v1/models", self.base_url))
            .send()
            .map_err(|error| DenRouterSelectionError::Transport(error.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(DenRouterSelectionError::ModelsHttpStatus(status.as_u16()));
        }
        let value = response.json::<Value>().map_err(|error| {
            DenRouterSelectionError::MalformedResponse {
                path: "/v1/models",
                message: error.to_string(),
            }
        })?;
        den_router_models_from_value(value)
    }

    fn fetch_routes(&self) -> Result<Option<DenRouterRoutes>, DenRouterSelectionError> {
        let response = self
            .client
            .get(format!("{}/routes", self.base_url))
            .send()
            .map_err(|error| DenRouterSelectionError::Transport(error.to_string()))?;
        if !response.status().is_success() {
            return Ok(None);
        }
        let value = response.json::<Value>().map_err(|error| {
            DenRouterSelectionError::MalformedResponse {
                path: "/routes",
                message: error.to_string(),
            }
        })?;
        den_router_routes_from_value(value).map(Some)
    }
}

pub fn normalize_den_router_base_url(raw: &str) -> String {
    raw.trim_end_matches('/')
        .strip_suffix("/v1")
        .unwrap_or_else(|| raw.trim_end_matches('/'))
        .to_string()
}

fn select_den_router_model<'a>(
    models: &'a [DenRouterModelInfo],
    requested: Option<&str>,
) -> Result<&'a DenRouterModelInfo, DenRouterSelectionError> {
    if let Some(requested) = requested {
        return models
            .iter()
            .find(|model| model.id == requested)
            .ok_or_else(|| DenRouterSelectionError::RequestedModelUnavailable(requested.into()));
    }
    for candidate in DEFAULT_DEN_ROUTER_MODEL_CANDIDATES {
        if let Some(model) = models.iter().find(|model| model.id == candidate) {
            return Ok(model);
        }
    }
    models.first().ok_or(DenRouterSelectionError::NoModels)
}

fn is_codex_backed(model_id: &str, routes: Option<&DenRouterRoutes>) -> bool {
    routes
        .and_then(|routes| routes.models.get(model_id))
        .map(|route| {
            route
                .backends
                .iter()
                .any(|backend| backend.backend_type.as_deref() == Some("codex-oauth"))
        })
        .unwrap_or(false)
}

fn den_router_models_from_value(
    value: Value,
) -> Result<Vec<DenRouterModelInfo>, DenRouterSelectionError> {
    #[derive(Deserialize)]
    struct ModelsResponse {
        data: Option<Vec<DenRouterModelInfo>>,
    }

    let response: ModelsResponse = serde_json::from_value(value).map_err(|error| {
        DenRouterSelectionError::MalformedResponse {
            path: "/v1/models",
            message: error.to_string(),
        }
    })?;
    match response.data {
        Some(models) if !models.is_empty() => Ok(models),
        _ => Err(DenRouterSelectionError::NoModels),
    }
}

fn den_router_routes_from_value(value: Value) -> Result<DenRouterRoutes, DenRouterSelectionError> {
    serde_json::from_value(value).map_err(|error| DenRouterSelectionError::MalformedResponse {
        path: "/routes",
        message: error.to_string(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatCompletionsChatConfig {
    pub model: String,
    pub temperature_milli: Option<u32>,
    pub reasoning_effort: Option<String>,
    pub wire_dialect: ChatCompletionsWireDialect,
    pub thinking_mode: ChatCompletionsThinkingMode,
    pub reasoning_history: ChatCompletionsReasoningHistory,
    pub reasoning_budget_tokens: Option<u32>,
    pub provider_state_strategy_id: String,
    pub max_output_tokens: Option<u32>,
    pub provider_request_timeout_ms: Option<u64>,
}

impl ChatCompletionsChatConfig {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            temperature_milli: None,
            reasoning_effort: None,
            wire_dialect: ChatCompletionsWireDialect::Standard,
            thinking_mode: ChatCompletionsThinkingMode::ProviderDefault,
            reasoning_history: ChatCompletionsReasoningHistory::ProviderDefault,
            reasoning_budget_tokens: None,
            provider_state_strategy_id: "default".to_string(),
            max_output_tokens: Some(128),
            provider_request_timeout_ms: None,
        }
    }

    pub fn from_model_provider(provider: &ModelProviderRecord) -> Self {
        Self {
            model: provider.model_id.clone(),
            temperature_milli: provider.temperature_milli,
            reasoning_effort: provider.reasoning_effort.clone(),
            wire_dialect: provider.chat_completions_dialect,
            thinking_mode: provider.thinking_mode,
            reasoning_history: provider.reasoning_history,
            reasoning_budget_tokens: provider.reasoning_budget_tokens,
            provider_state_strategy_id: "default".to_string(),
            max_output_tokens: provider.max_output_tokens,
            provider_request_timeout_ms: None,
        }
    }

    pub fn validate(&self) -> Result<(), ChatCompletionsConfigError> {
        let default_policy = self.thinking_mode == ChatCompletionsThinkingMode::ProviderDefault
            && self.reasoning_history == ChatCompletionsReasoningHistory::ProviderDefault
            && self.reasoning_budget_tokens.is_none();
        if self.reasoning_history == ChatCompletionsReasoningHistory::ToolCallsOnly
            && self.wire_dialect != ChatCompletionsWireDialect::Deepseek
        {
            return Err(ChatCompletionsConfigError::UnsupportedDialectOption(
                "tool_calls_only reasoning history requires the deepseek chat completions dialect",
            ));
        }
        if self.wire_dialect == ChatCompletionsWireDialect::Standard && !default_policy {
            return Err(ChatCompletionsConfigError::UnsupportedDialectOption(
                "standard chat completions dialect does not accept vendor thinking settings",
            ));
        }
        if self.thinking_mode == ChatCompletionsThinkingMode::Disabled
            && self.reasoning_history != ChatCompletionsReasoningHistory::ProviderDefault
        {
            return Err(ChatCompletionsConfigError::UnsupportedDialectOption(
                "disabled thinking cannot configure reasoning history preservation",
            ));
        }
        if self.wire_dialect == ChatCompletionsWireDialect::Kimi
            && self.thinking_mode != ChatCompletionsThinkingMode::Disabled
        {
            if self.temperature_milli.is_some() {
                return Err(ChatCompletionsConfigError::UnsupportedDialectOption(
                    "kimi thinking models do not accept a temperature override",
                ));
            }
            if !matches!(
                self.max_output_tokens,
                Some(tokens) if tokens >= KIMI_THINKING_MIN_OUTPUT_TOKENS
            ) {
                return Err(ChatCompletionsConfigError::UnsupportedDialectOption(
                    "kimi thinking models require max output tokens of at least 16000",
                ));
            }
        }
        if let Some(budget) = self.reasoning_budget_tokens {
            if budget == 0 {
                return Err(ChatCompletionsConfigError::UnsupportedDialectOption(
                    "reasoning budget tokens must be greater than zero",
                ));
            }
            if self.wire_dialect != ChatCompletionsWireDialect::Qwen
                || self.thinking_mode != ChatCompletionsThinkingMode::Enabled
            {
                return Err(ChatCompletionsConfigError::UnsupportedDialectOption(
                    "reasoning budget tokens require qwen dialect with thinking enabled",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatCompletionsConfigError {
    UnsupportedDialectOption(&'static str),
}

impl std::fmt::Display for ChatCompletionsConfigError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedDialectOption(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for ChatCompletionsConfigError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatMessageRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatCompletionMessage {
    pub role: ChatMessageRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ChatAssistantToolCall>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatCompletionsInputImage {
    pub attachment_id: String,
    pub mime_type: String,
    pub bytes_base64: String,
    pub byte_size: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChatCompletionsRequestMessage {
    pub role: ChatMessageRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ChatAssistantToolCall>,
}

impl ChatCompletionMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self::text(ChatMessageRole::System, content)
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::text(ChatMessageRole::User, content)
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self::text(ChatMessageRole::Assistant, content)
    }

    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: ChatMessageRole::Tool,
            content: Some(content.into()),
            reasoning_content: None,
            name: None,
            tool_call_id: Some(tool_call_id.into()),
            tool_calls: Vec::new(),
        }
    }

    fn text(role: ChatMessageRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: Some(content.into()),
            reasoning_content: None,
            name: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatAssistantToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub function: ChatFunctionCall,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatFunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NeutralBrainTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatToolDescriptor {
    #[serde(rename = "type")]
    pub kind: String,
    pub function: ChatToolFunctionDescriptor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatToolFunctionDescriptor {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatToolChoice {
    Auto,
    None,
    Function { name: String },
}

impl ChatToolChoice {
    fn as_value(&self) -> Value {
        match self {
            Self::Auto => json!("auto"),
            Self::None => json!("none"),
            Self::Function { name } => {
                json!({"type": "function", "function": {"name": name}})
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatCompletionsRequest {
    pub model: String,
    pub messages: Vec<ChatCompletionsRequestMessage>,
    pub tools: Vec<ChatToolDescriptor>,
    pub tool_choice: Value,
    pub stream: bool,
    pub stream_options: Option<ChatCompletionsStreamOptions>,
    pub temperature: Option<f64>,
    pub reasoning_effort: Option<String>,
    pub thinking: Option<Value>,
    pub enable_thinking: Option<bool>,
    pub preserve_thinking: Option<bool>,
    pub thinking_budget: Option<u32>,
    pub max_tokens: Option<u32>,
}

impl Serialize for ChatCompletionsRequest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut fields = 4;
        fields += (!self.tools.is_empty()) as usize;
        fields += self.stream_options.is_some() as usize;
        fields += self.temperature.is_some() as usize;
        fields += self.reasoning_effort.is_some() as usize;
        fields += self.thinking.is_some() as usize;
        fields += self.enable_thinking.is_some() as usize;
        fields += self.preserve_thinking.is_some() as usize;
        fields += self.thinking_budget.is_some() as usize;
        fields += self.max_tokens.is_some() as usize;

        let mut map = serializer.serialize_struct("ChatCompletionsRequest", fields)?;
        map.serialize_field("model", &self.model)?;
        map.serialize_field("messages", &self.messages)?;
        if !self.tools.is_empty() {
            map.serialize_field("tools", &self.tools)?;
            map.serialize_field("tool_choice", &self.tool_choice)?;
        }
        map.serialize_field("stream", &self.stream)?;
        if let Some(stream_options) = &self.stream_options {
            map.serialize_field("stream_options", stream_options)?;
        }
        if let Some(temperature) = self.temperature {
            map.serialize_field("temperature", &temperature)?;
        }
        if let Some(reasoning_effort) = &self.reasoning_effort {
            map.serialize_field("reasoning_effort", reasoning_effort)?;
        }
        if let Some(thinking) = &self.thinking {
            map.serialize_field("thinking", thinking)?;
        }
        if let Some(enable_thinking) = self.enable_thinking {
            map.serialize_field("enable_thinking", &enable_thinking)?;
        }
        if let Some(preserve_thinking) = self.preserve_thinking {
            map.serialize_field("preserve_thinking", &preserve_thinking)?;
        }
        if let Some(thinking_budget) = self.thinking_budget {
            map.serialize_field("thinking_budget", &thinking_budget)?;
        }
        if let Some(max_tokens) = self.max_tokens {
            map.serialize_field("max_tokens", &max_tokens)?;
        }
        map.end()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatCompletionsStreamOptions {
    pub include_usage: bool,
}

pub struct ChatCompletionsRequestBuilder {
    config: ChatCompletionsChatConfig,
    tools: Vec<NeutralBrainTool>,
    tool_choice: ChatToolChoice,
    include_usage: bool,
}

impl ChatCompletionsRequestBuilder {
    pub fn new(config: ChatCompletionsChatConfig) -> Self {
        Self {
            config,
            tools: Vec::new(),
            tool_choice: ChatToolChoice::Auto,
            include_usage: true,
        }
    }

    pub fn tools(mut self, tools: Vec<NeutralBrainTool>) -> Self {
        self.tools = tools;
        self
    }

    pub fn tool_choice(mut self, tool_choice: ChatToolChoice) -> Self {
        self.tool_choice = tool_choice;
        self
    }

    pub fn include_usage(mut self, include_usage: bool) -> Self {
        self.include_usage = include_usage;
        self
    }

    pub fn build(&self, messages: Vec<ChatCompletionMessage>) -> ChatCompletionsRequest {
        self.build_with_images(messages, &[])
    }

    pub fn build_with_images(
        &self,
        messages: Vec<ChatCompletionMessage>,
        input_images: &[ChatCompletionsInputImage],
    ) -> ChatCompletionsRequest {
        let extensions = chat_completions_dialect_extensions(&self.config);
        let mut request_messages = messages
            .into_iter()
            .map(chat_completions_request_message)
            .collect::<Vec<_>>();
        if !input_images.is_empty() {
            if let Some(message) = request_messages
                .iter_mut()
                .rev()
                .find(|message| message.role == ChatMessageRole::User)
            {
                let mut content = Vec::with_capacity(input_images.len() + 1);
                if let Some(Value::String(text)) = message.content.take() {
                    if !text.is_empty() {
                        content.push(json!({"type": "text", "text": text}));
                    }
                }
                content.extend(input_images.iter().map(|image| {
                    json!({
                        "type": "image_url",
                        "image_url": {
                            "url": format!(
                                "data:{};base64,{}",
                                image.mime_type, image.bytes_base64
                            )
                        }
                    })
                }));
                message.content = Some(Value::Array(content));
            }
        }
        ChatCompletionsRequest {
            model: self.config.model.clone(),
            messages: request_messages,
            tools: self.tools.iter().map(adapt_neutral_tool).collect(),
            tool_choice: self.tool_choice.as_value(),
            stream: true,
            stream_options: self.include_usage.then_some(ChatCompletionsStreamOptions {
                include_usage: true,
            }),
            temperature: self
                .config
                .temperature_milli
                .map(|milli| f64::from(milli) / 1000.0),
            reasoning_effort: self.config.reasoning_effort.clone(),
            thinking: extensions.thinking,
            enable_thinking: extensions.enable_thinking,
            preserve_thinking: extensions.preserve_thinking,
            thinking_budget: extensions.thinking_budget,
            max_tokens: self.config.max_output_tokens,
        }
    }
}

fn chat_completions_request_message(
    message: ChatCompletionMessage,
) -> ChatCompletionsRequestMessage {
    ChatCompletionsRequestMessage {
        role: message.role,
        content: message.content.map(Value::String),
        reasoning_content: message.reasoning_content,
        name: message.name,
        tool_call_id: message.tool_call_id,
        tool_calls: message.tool_calls,
    }
}

#[derive(Debug, Default)]
struct ChatCompletionsDialectExtensions {
    thinking: Option<Value>,
    enable_thinking: Option<bool>,
    preserve_thinking: Option<bool>,
    thinking_budget: Option<u32>,
}

fn chat_completions_dialect_extensions(
    config: &ChatCompletionsChatConfig,
) -> ChatCompletionsDialectExtensions {
    match config.wire_dialect {
        ChatCompletionsWireDialect::Standard => ChatCompletionsDialectExtensions::default(),
        ChatCompletionsWireDialect::Kimi => {
            let mut thinking = serde_json::Map::new();
            match config.thinking_mode {
                ChatCompletionsThinkingMode::ProviderDefault => {}
                ChatCompletionsThinkingMode::Enabled => {
                    thinking.insert("type".to_string(), json!("enabled"));
                }
                ChatCompletionsThinkingMode::Disabled => {
                    thinking.insert("type".to_string(), json!("disabled"));
                }
            }
            match config.reasoning_history {
                ChatCompletionsReasoningHistory::ProviderDefault => {}
                ChatCompletionsReasoningHistory::Discard => {
                    thinking.insert("keep".to_string(), Value::Null);
                }
                ChatCompletionsReasoningHistory::PreserveAll => {
                    thinking.insert("keep".to_string(), json!("all"));
                }
                ChatCompletionsReasoningHistory::ToolCallsOnly => {}
            }
            ChatCompletionsDialectExtensions {
                thinking: (!thinking.is_empty()).then_some(Value::Object(thinking)),
                ..ChatCompletionsDialectExtensions::default()
            }
        }
        ChatCompletionsWireDialect::Glm => {
            let mut thinking = serde_json::Map::new();
            match config.thinking_mode {
                ChatCompletionsThinkingMode::ProviderDefault => {}
                ChatCompletionsThinkingMode::Enabled => {
                    thinking.insert("type".to_string(), json!("enabled"));
                }
                ChatCompletionsThinkingMode::Disabled => {
                    thinking.insert("type".to_string(), json!("disabled"));
                }
            }
            match config.reasoning_history {
                ChatCompletionsReasoningHistory::ProviderDefault => {}
                ChatCompletionsReasoningHistory::Discard => {
                    thinking.insert("clear_thinking".to_string(), json!(true));
                }
                ChatCompletionsReasoningHistory::PreserveAll => {
                    thinking.insert("clear_thinking".to_string(), json!(false));
                }
                ChatCompletionsReasoningHistory::ToolCallsOnly => {}
            }
            ChatCompletionsDialectExtensions {
                thinking: (!thinking.is_empty()).then_some(Value::Object(thinking)),
                ..ChatCompletionsDialectExtensions::default()
            }
        }
        ChatCompletionsWireDialect::Qwen => ChatCompletionsDialectExtensions {
            enable_thinking: match config.thinking_mode {
                ChatCompletionsThinkingMode::ProviderDefault => None,
                ChatCompletionsThinkingMode::Enabled => Some(true),
                ChatCompletionsThinkingMode::Disabled => Some(false),
            },
            preserve_thinking: match config.reasoning_history {
                ChatCompletionsReasoningHistory::ProviderDefault => None,
                ChatCompletionsReasoningHistory::Discard => Some(false),
                ChatCompletionsReasoningHistory::PreserveAll => Some(true),
                ChatCompletionsReasoningHistory::ToolCallsOnly => None,
            },
            thinking_budget: config.reasoning_budget_tokens,
            thinking: None,
        },
        ChatCompletionsWireDialect::Deepseek => {
            let mut thinking = serde_json::Map::new();
            match config.thinking_mode {
                ChatCompletionsThinkingMode::ProviderDefault => {}
                ChatCompletionsThinkingMode::Enabled => {
                    thinking.insert("type".to_string(), json!("enabled"));
                }
                ChatCompletionsThinkingMode::Disabled => {
                    thinking.insert("type".to_string(), json!("disabled"));
                }
            }
            ChatCompletionsDialectExtensions {
                thinking: (!thinking.is_empty()).then_some(Value::Object(thinking)),
                ..ChatCompletionsDialectExtensions::default()
            }
        }
    }
}

fn adapt_neutral_tool(tool: &NeutralBrainTool) -> ChatToolDescriptor {
    ChatToolDescriptor {
        kind: "function".to_string(),
        function: ChatToolFunctionDescriptor {
            name: tool.name.clone(),
            description: tool.description.clone(),
            parameters: tool.input_schema.clone(),
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatTokenUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    pub cached_prompt_tokens: u64,
    pub reasoning_completion_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingChatFunctionCall {
    pub index: u32,
    pub id: Option<String>,
    pub name: String,
    pub arguments_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MalformedChatFunctionCall {
    pub index: u32,
    pub id: Option<String>,
    pub name: Option<String>,
    pub arguments_json: String,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatCompletionsEvent {
    ContentDelta(String),
    ReasoningDelta {
        text: String,
        field: String,
    },
    ToolCallDelta {
        index: u32,
        id: Option<String>,
        name: Option<String>,
        arguments_delta: String,
    },
    ToolCallFinished(PendingChatFunctionCall),
    ToolCallMalformed(MalformedChatFunctionCall),
    Usage(ChatTokenUsage),
    Finished {
        finish_reason: Option<String>,
    },
    ProviderError(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatCompletionsToolOutput {
    pub output: String,
    pub is_error: bool,
    pub cancelled: bool,
    pub timed_out: bool,
}

impl ChatCompletionsToolOutput {
    pub fn ok(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            is_error: false,
            cancelled: false,
            timed_out: false,
        }
    }

    pub fn error(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            is_error: true,
            cancelled: false,
            timed_out: false,
        }
    }

    pub fn cancelled(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            is_error: true,
            cancelled: true,
            timed_out: false,
        }
    }

    pub fn timed_out(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            is_error: true,
            cancelled: false,
            timed_out: true,
        }
    }
}

pub trait ChatCompletionsNeutralToolExecutor {
    fn execute(&self, call: &PendingChatFunctionCall) -> ChatCompletionsToolOutput;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatCompletionsBrainLoopConfig {
    pub work_quantum_tool_rounds: usize,
    pub no_progress_attention_threshold: u32,
}

impl Default for ChatCompletionsBrainLoopConfig {
    fn default() -> Self {
        Self {
            work_quantum_tool_rounds: DEFAULT_WORK_QUANTUM_TOOL_ROUNDS,
            no_progress_attention_threshold: DEFAULT_NO_PROGRESS_ATTENTION_THRESHOLD,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatCompletionsBrainLoopInput {
    pub context: BrainEventContext,
    pub messages: Vec<ChatCompletionMessage>,
    pub input_images: Vec<ChatCompletionsInputImage>,
    pub provider_state: Option<BrainWakeProviderStateInput>,
    pub continuation_state: Option<BrainContinuationPayload>,
    pub final_message_fallback: Option<ChatCompletionsFinalMessage>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatCompletionsBrainLoopOutput {
    pub stream: Vec<BrainWakeStreamItem>,
    pub completed: bool,
    pub yielded: bool,
    pub attention: Option<BrainWakeAttention>,
    pub provider_request_count: usize,
    pub tool_round_count: usize,
    pub provider_event_counts: BTreeMap<String, usize>,
    pub provider_request_debug_samples: Vec<Value>,
    pub provider_state: Option<BrainWakeProviderStateOutput>,
    pub continuation_state: Option<BrainContinuationPayload>,
}

type BrainWakeItemSink<'a> = Option<&'a mut dyn FnMut(BrainWakeStreamItem)>;

fn push_stream_item(
    stream: &mut Vec<BrainWakeStreamItem>,
    item: BrainWakeStreamItem,
    sink: &mut BrainWakeItemSink<'_>,
) {
    if let Some(sink) = sink.as_deref_mut() {
        sink(item.clone());
    }
    stream.push(item);
}

fn extend_stream_items(
    stream: &mut Vec<BrainWakeStreamItem>,
    items: impl IntoIterator<Item = BrainWakeStreamItem>,
    sink: &mut BrainWakeItemSink<'_>,
) {
    for item in items {
        push_stream_item(stream, item, sink);
    }
}

pub struct ChatCompletionsBrainLoop<C, T> {
    client: C,
    tools: T,
    request_builder: ChatCompletionsRequestBuilder,
    config: ChatCompletionsBrainLoopConfig,
}

impl<C, T> ChatCompletionsBrainLoop<C, T>
where
    C: ChatCompletionsClient,
    T: ChatCompletionsNeutralToolExecutor,
{
    pub fn new(
        client: C,
        tools: T,
        chat_config: ChatCompletionsChatConfig,
        descriptors: Vec<NeutralBrainTool>,
    ) -> Self {
        Self {
            client,
            tools,
            request_builder: ChatCompletionsRequestBuilder::new(chat_config).tools(descriptors),
            config: ChatCompletionsBrainLoopConfig::default(),
        }
    }

    pub fn with_loop_config(mut self, config: ChatCompletionsBrainLoopConfig) -> Self {
        self.config = config;
        self
    }

    pub fn wake_with_messages(
        &mut self,
        context: BrainEventContext,
        messages: Vec<ChatCompletionMessage>,
    ) -> ChatCompletionsBrainLoopOutput {
        self.wake(ChatCompletionsBrainLoopInput {
            context,
            messages,
            input_images: Vec::new(),
            provider_state: None,
            continuation_state: None,
            final_message_fallback: None,
        })
    }

    pub fn wake(&mut self, input: ChatCompletionsBrainLoopInput) -> ChatCompletionsBrainLoopOutput {
        self.wake_internal(input, None)
    }

    pub fn wake_with_stream_sink(
        &mut self,
        input: ChatCompletionsBrainLoopInput,
        sink: &mut dyn FnMut(BrainWakeStreamItem),
    ) -> ChatCompletionsBrainLoopOutput {
        self.wake_internal(input, Some(sink))
    }

    fn wake_internal(
        &mut self,
        input: ChatCompletionsBrainLoopInput,
        mut sink: BrainWakeItemSink<'_>,
    ) -> ChatCompletionsBrainLoopOutput {
        let mut mapper = ChatCompletionsEventMapper::new();
        let mut stream = Vec::new();
        let restored = match input.continuation_state.as_ref() {
            Some(state) => match chat_completions_continuation_state(state) {
                Ok(state) => Some(state),
                Err(message) => {
                    push_stream_item(
                        &mut stream,
                        wake_failed_item_with_reason(
                            &input.context,
                            CoreErrorKind::InvalidInput,
                            "chat_completions_continuation_state_invalid",
                            message,
                        ),
                        &mut sink,
                    );
                    return ChatCompletionsBrainLoopOutput {
                        stream,
                        completed: false,
                        yielded: false,
                        attention: None,
                        provider_request_count: 0,
                        tool_round_count: 0,
                        provider_event_counts: BTreeMap::new(),
                        provider_request_debug_samples: Vec::new(),
                        provider_state: None,
                        continuation_state: None,
                    };
                }
            },
            None => None,
        };
        if restored.is_none() {
            extend_stream_items(&mut stream, mapper.map_started(&input.context), &mut sink);
        }
        let fresh_messages = if restored.is_some() {
            Vec::new()
        } else {
            match chat_completions_messages_with_provider_state(
                &self.request_builder.config,
                input.provider_state.as_ref(),
                input.messages,
            ) {
                Ok(messages) => messages,
                Err(message) => {
                    push_stream_item(
                        &mut stream,
                        wake_failed_item_with_reason(
                            &input.context,
                            CoreErrorKind::InvalidInput,
                            "chat_completions_provider_state_invalid",
                            message,
                        ),
                        &mut sink,
                    );
                    return ChatCompletionsBrainLoopOutput {
                        stream,
                        completed: false,
                        yielded: false,
                        attention: None,
                        provider_request_count: 0,
                        tool_round_count: 0,
                        provider_event_counts: BTreeMap::new(),
                        provider_request_debug_samples: Vec::new(),
                        provider_state: None,
                        continuation_state: None,
                    };
                }
            }
        };
        let mut messages = restored
            .as_ref()
            .map(|state| state.messages.clone())
            .unwrap_or_else(|| fresh_messages.clone());
        let mut durable_messages = restored
            .as_ref()
            .map(|state| state.durable_messages.clone())
            .unwrap_or(fresh_messages);
        let mut no_progress_state = restored
            .as_ref()
            .map(|state| state.no_progress_state.clone())
            .unwrap_or_default();
        let no_progress_policy =
            match BrainNoProgressPolicy::new(self.config.no_progress_attention_threshold) {
                Ok(policy) => policy,
                Err(message) => {
                    push_stream_item(
                        &mut stream,
                        wake_failed_item_with_reason(
                            &input.context,
                            CoreErrorKind::InvalidInput,
                            "chat_completions_no_progress_policy_invalid",
                            message,
                        ),
                        &mut sink,
                    );
                    return ChatCompletionsBrainLoopOutput {
                        stream,
                        completed: false,
                        yielded: false,
                        attention: None,
                        provider_request_count: 0,
                        tool_round_count: 0,
                        provider_event_counts: BTreeMap::new(),
                        provider_request_debug_samples: Vec::new(),
                        provider_state: None,
                        continuation_state: None,
                    };
                }
            };
        let mut provider_request_count = restored
            .as_ref()
            .map_or(0, |state| state.provider_request_count);
        let mut tool_round_count = restored.as_ref().map_or(0, |state| state.tool_round_count);
        let mut epoch_tool_round_count = 0;
        let mut provider_event_counts = restored
            .as_ref()
            .map(|state| state.provider_event_counts.clone())
            .unwrap_or_default();
        let mut provider_request_debug_samples = restored
            .as_ref()
            .map(|state| state.provider_request_debug_samples.clone())
            .unwrap_or_default();
        let input_images = restored
            .as_ref()
            .map(|state| state.input_images.clone())
            .unwrap_or(input.input_images);

        loop {
            provider_request_count += 1;
            let request = self
                .request_builder
                .build_with_images(messages.clone(), &input_images);
            provider_request_debug_samples
                .push(chat_completions_debug_request(&request, &input_images));
            let mut assistant_text = String::new();
            let mut assistant_reasoning = String::new();
            let mut tool_calls = Vec::new();
            let mut malformed_tool_calls = Vec::new();
            let mut finish_reason = None;
            let result = self.client.stream_observed(request, &mut |event| {
                record_provider_event(&mut provider_event_counts, event);
                if let ChatCompletionsEvent::ContentDelta(text) = event {
                    assistant_text.push_str(text);
                }
                if let ChatCompletionsEvent::ReasoningDelta { text, .. } = event {
                    assistant_reasoning.push_str(text);
                }
                if let ChatCompletionsEvent::ToolCallFinished(call) = event {
                    tool_calls.push(call.clone());
                }
                if let ChatCompletionsEvent::ToolCallMalformed(call) = event {
                    malformed_tool_calls.push(call.clone());
                }
                if let ChatCompletionsEvent::Finished {
                    finish_reason: provider_finish_reason,
                } = event
                {
                    finish_reason = provider_finish_reason.clone();
                }
                if matches!(event, ChatCompletionsEvent::Finished { .. })
                    && (!tool_calls.is_empty() || !malformed_tool_calls.is_empty())
                {
                    return;
                }
                extend_stream_items(
                    &mut stream,
                    mapper.map_provider_event(&input.context, event),
                    &mut sink,
                );
            });

            if let Err(error) = result {
                push_stream_item(
                    &mut stream,
                    wake_failed_item(
                        &input.context,
                        CoreErrorKind::BrainUnavailable,
                        format!("chat-completions provider stream failed: {error}"),
                    ),
                    &mut sink,
                );
                let provider_state = if tool_round_count > 0 {
                    chat_completions_provider_state_output(
                        &input.context,
                        &self.request_builder.config,
                        input.provider_state.as_ref(),
                        durable_messages,
                    )
                } else {
                    None
                };
                return ChatCompletionsBrainLoopOutput {
                    stream,
                    completed: false,
                    yielded: false,
                    attention: None,
                    provider_request_count,
                    tool_round_count,
                    provider_event_counts,
                    provider_request_debug_samples,
                    provider_state,
                    continuation_state: None,
                };
            }

            if !malformed_tool_calls.is_empty() {
                let trigger_reason_code = if finish_reason.as_deref() == Some("length") {
                    OUTPUT_LIMIT_EXCEEDED_REASON_CODE
                } else {
                    MALFORMED_PROVIDER_STREAM_REASON_CODE
                };
                let malformed_summary =
                    malformed_tool_call_summary(finish_reason.as_deref(), &malformed_tool_calls);
                let disposition = no_progress_policy.observe(
                    &mut no_progress_state,
                    BrainProgressSample {
                        intent_fingerprint: progress_fingerprint(&[
                            "malformed_tool_call",
                            finish_reason.as_deref().unwrap_or("unknown"),
                        ]),
                        result_fingerprint: progress_fingerprint(&[&malformed_summary]),
                        state_fingerprint: progress_json_fingerprint(&durable_messages),
                        assistant_progress_fingerprint: progress_fingerprint(&[
                            &assistant_text,
                            &assistant_reasoning,
                        ]),
                        result_class: BrainProgressResultClass::MalformedProviderOutput,
                    },
                );
                // Some OpenAI-compatible providers reject assistant history
                // entries that contain reasoning_content without visible
                // content or executable tool calls. The reasoning deltas
                // remain observable, but only replay a provider-valid partial
                // assistant message during recovery.
                if !assistant_text.is_empty() {
                    messages.push(assistant_partial_message(
                        &assistant_text,
                        &assistant_reasoning,
                    ));
                }
                if let BrainProgressDisposition::AttentionRequired {
                    consecutive_samples,
                } = disposition
                {
                    let reason_code = "chat_completions_malformed_tool_call_no_progress";
                    let summary = format!(
                        "provider repeatedly emitted the same malformed tool call without recoverable progress ({consecutive_samples} equivalent repetitions)"
                    );
                    messages.push(ChatCompletionMessage::user(format!(
                        "[Rusty Crew operator attention] {summary}. Stop retrying this call until the provider configuration or prompt is adjusted."
                    )));
                    push_stream_item(
                        &mut stream,
                        no_progress_attention_status(
                            &input.context,
                            reason_code,
                            &summary,
                            consecutive_samples,
                        ),
                        &mut sink,
                    );
                    let continuation_state = match chat_completions_continuation_output(
                        messages,
                        durable_messages,
                        input_images,
                        no_progress_state,
                        provider_request_count,
                        tool_round_count,
                        provider_event_counts.clone(),
                        provider_request_debug_samples.clone(),
                    ) {
                        Ok(state) => state,
                        Err(message) => {
                            push_stream_item(
                                &mut stream,
                                wake_failed_item_with_reason(
                                    &input.context,
                                    CoreErrorKind::InternalError,
                                    "chat_completions_continuation_checkpoint_failed",
                                    message,
                                ),
                                &mut sink,
                            );
                            return ChatCompletionsBrainLoopOutput {
                                stream,
                                completed: false,
                                yielded: false,
                                attention: None,
                                provider_request_count,
                                tool_round_count,
                                provider_event_counts,
                                provider_request_debug_samples,
                                provider_state: None,
                                continuation_state: None,
                            };
                        }
                    };
                    return ChatCompletionsBrainLoopOutput {
                        stream,
                        completed: false,
                        yielded: false,
                        attention: Some(no_progress_attention(
                            reason_code,
                            summary,
                            consecutive_samples,
                        )),
                        provider_request_count,
                        tool_round_count,
                        provider_event_counts,
                        provider_request_debug_samples,
                        provider_state: None,
                        continuation_state: Some(continuation_state),
                    };
                }
                let recovery_number = no_progress_state
                    .consecutive_no_progress_samples
                    .saturating_add(1);
                messages.push(ChatCompletionMessage::user(
                    malformed_tool_call_recovery_feedback(
                        finish_reason.as_deref(),
                        &malformed_tool_calls,
                        recovery_number,
                    ),
                ));
                push_stream_item(
                    &mut stream,
                    malformed_tool_call_recovery_status(
                        &input.context,
                        trigger_reason_code,
                        recovery_number,
                        no_progress_policy.attention_threshold(),
                        &malformed_tool_calls,
                    ),
                    &mut sink,
                );
                continue;
            }

            if finish_reason.as_deref() == Some("length") && !tool_calls_are_actionable(&tool_calls)
            {
                push_stream_item(
                    &mut stream,
                    wake_failed_item_with_reason(
                        &input.context,
                        CoreErrorKind::BrainUnavailable,
                        OUTPUT_LIMIT_EXCEEDED_REASON_CODE,
                        "chat-completions provider reached finish_reason length before completing the turn",
                    ),
                    &mut sink,
                );
                return ChatCompletionsBrainLoopOutput {
                    stream,
                    completed: false,
                    yielded: false,
                    attention: None,
                    provider_request_count,
                    tool_round_count,
                    provider_event_counts,
                    provider_request_debug_samples,
                    provider_state: None,
                    continuation_state: None,
                };
            }

            if tool_calls.is_empty() {
                if !assistant_text.is_empty() || !assistant_reasoning.is_empty() {
                    let assistant_message = ChatCompletionMessage {
                        role: ChatMessageRole::Assistant,
                        content: (!assistant_text.is_empty()).then_some(assistant_text.clone()),
                        reasoning_content: (!assistant_reasoning.is_empty())
                            .then_some(assistant_reasoning.clone()),
                        name: None,
                        tool_call_id: None,
                        tool_calls: Vec::new(),
                    };
                    messages.push(assistant_message.clone());
                    durable_messages.push(assistant_message);
                }
                if assistant_text.trim().is_empty() {
                    if let Some(fallback) = input.final_message_fallback {
                        extend_stream_items(
                            &mut stream,
                            mapper.map_final_message(&input.context, fallback),
                            &mut sink,
                        );
                    }
                }
                push_stream_item(&mut stream, success_actions_item(&input.context), &mut sink);
                let provider_state = chat_completions_provider_state_output(
                    &input.context,
                    &self.request_builder.config,
                    input.provider_state.as_ref(),
                    durable_messages,
                );
                return ChatCompletionsBrainLoopOutput {
                    stream,
                    completed: true,
                    yielded: false,
                    attention: None,
                    provider_request_count,
                    tool_round_count,
                    provider_event_counts,
                    provider_request_debug_samples,
                    provider_state,
                    continuation_state: None,
                };
            }

            tool_round_count += 1;
            epoch_tool_round_count += 1;

            let assistant_tool_message =
                assistant_tool_call_message(&assistant_text, &assistant_reasoning, &tool_calls);
            messages.push(assistant_tool_message.clone());
            durable_messages.push(assistant_tool_message);
            for call in tool_calls {
                push_stream_item(
                    &mut stream,
                    brain_event_item(
                        &input.context,
                        BrainEvent::ToolCallStarted {
                            tool_name: call.name.clone(),
                            metadata: None,
                        },
                    ),
                    &mut sink,
                );
                let mut output = self.tools.execute(&call);
                push_stream_item(
                    &mut stream,
                    brain_event_item(
                        &input.context,
                        BrainEvent::ToolCallFinished {
                            tool_name: call.name.clone(),
                            is_error: output.is_error,
                            metadata: None,
                        },
                    ),
                    &mut sink,
                );
                if output.cancelled || output.timed_out {
                    let kind = if output.timed_out {
                        CoreErrorKind::TimeoutExpired
                    } else {
                        CoreErrorKind::BrainUnavailable
                    };
                    push_stream_item(
                        &mut stream,
                        wake_failed_item(&input.context, kind, output.output),
                        &mut sink,
                    );
                    return ChatCompletionsBrainLoopOutput {
                        stream,
                        completed: false,
                        yielded: false,
                        attention: None,
                        provider_request_count,
                        tool_round_count,
                        provider_event_counts,
                        provider_request_debug_samples,
                        provider_state: None,
                        continuation_state: None,
                    };
                }
                let disposition = no_progress_policy.observe(
                    &mut no_progress_state,
                    BrainProgressSample {
                        intent_fingerprint: progress_fingerprint(&[
                            "tool_call",
                            &call.name,
                            &call.arguments_json,
                        ]),
                        result_fingerprint: progress_fingerprint(&[
                            if output.is_error { "error" } else { "success" },
                            &output.output,
                        ]),
                        state_fingerprint: String::new(),
                        assistant_progress_fingerprint: progress_fingerprint(&[
                            &assistant_text,
                            &assistant_reasoning,
                        ]),
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
                        "\n\n[Rusty Crew no-progress guidance] This tool returned the same failure for the same arguments again ({consecutive_samples} equivalent repetition(s)). Change the arguments, choose another tool, or report the dependency as unavailable instead of repeating it unchanged."
                    ));
                    push_stream_item(
                        &mut stream,
                        no_progress_correction_status(
                            &input.context,
                            &call.name,
                            consecutive_samples,
                            no_progress_policy.attention_threshold(),
                        ),
                        &mut sink,
                    );
                }
                let tool_message = ChatCompletionMessage::tool(
                    call.id
                        .clone()
                        .unwrap_or_else(|| format!("call_{}", call.index)),
                    output.output,
                );
                messages.push(tool_message.clone());
                durable_messages.push(tool_message);
                if let BrainProgressDisposition::AttentionRequired {
                    consecutive_samples,
                } = disposition
                {
                    let reason_code = "chat_completions_tool_no_progress";
                    let summary = format!(
                        "tool {} returned an equivalent failure for unchanged arguments {consecutive_samples} consecutive times",
                        call.name
                    );
                    push_stream_item(
                        &mut stream,
                        no_progress_attention_status(
                            &input.context,
                            reason_code,
                            &summary,
                            consecutive_samples,
                        ),
                        &mut sink,
                    );
                    let continuation_state = match chat_completions_continuation_output(
                        messages,
                        durable_messages,
                        input_images,
                        no_progress_state,
                        provider_request_count,
                        tool_round_count,
                        provider_event_counts.clone(),
                        provider_request_debug_samples.clone(),
                    ) {
                        Ok(state) => state,
                        Err(message) => {
                            push_stream_item(
                                &mut stream,
                                wake_failed_item_with_reason(
                                    &input.context,
                                    CoreErrorKind::InternalError,
                                    "chat_completions_continuation_checkpoint_failed",
                                    message,
                                ),
                                &mut sink,
                            );
                            return ChatCompletionsBrainLoopOutput {
                                stream,
                                completed: false,
                                yielded: false,
                                attention: None,
                                provider_request_count,
                                tool_round_count,
                                provider_event_counts,
                                provider_request_debug_samples,
                                provider_state: None,
                                continuation_state: None,
                            };
                        }
                    };
                    return ChatCompletionsBrainLoopOutput {
                        stream,
                        completed: false,
                        yielded: false,
                        attention: Some(no_progress_attention(
                            reason_code,
                            summary,
                            consecutive_samples,
                        )),
                        provider_request_count,
                        tool_round_count,
                        provider_event_counts,
                        provider_request_debug_samples,
                        provider_state: None,
                        continuation_state: Some(continuation_state),
                    };
                }
            }
            if epoch_tool_round_count >= self.config.work_quantum_tool_rounds {
                let continuation_state = match chat_completions_continuation_output(
                    messages,
                    durable_messages,
                    input_images,
                    no_progress_state,
                    provider_request_count,
                    tool_round_count,
                    provider_event_counts.clone(),
                    provider_request_debug_samples.clone(),
                ) {
                    Ok(state) => state,
                    Err(message) => {
                        push_stream_item(
                            &mut stream,
                            wake_failed_item_with_reason(
                                &input.context,
                                CoreErrorKind::InternalError,
                                "chat_completions_continuation_checkpoint_failed",
                                message,
                            ),
                            &mut sink,
                        );
                        return ChatCompletionsBrainLoopOutput {
                            stream,
                            completed: false,
                            yielded: false,
                            attention: None,
                            provider_request_count,
                            tool_round_count,
                            provider_event_counts,
                            provider_request_debug_samples,
                            provider_state: None,
                            continuation_state: None,
                        };
                    }
                };
                return ChatCompletionsBrainLoopOutput {
                    stream,
                    completed: false,
                    yielded: true,
                    attention: None,
                    provider_request_count,
                    tool_round_count,
                    provider_event_counts,
                    provider_request_debug_samples,
                    provider_state: None,
                    continuation_state: Some(continuation_state),
                };
            }
        }
    }
}

fn chat_completions_debug_request(
    request: &ChatCompletionsRequest,
    input_images: &[ChatCompletionsInputImage],
) -> Value {
    let mut value = serde_json::to_value(request).unwrap_or_else(|_| json!({"error": "serialize"}));
    let Some(messages) = value.get_mut("messages").and_then(Value::as_array_mut) else {
        return value;
    };
    let mut image_index = 0usize;
    for message in messages {
        let Some(parts) = message.get_mut("content").and_then(Value::as_array_mut) else {
            continue;
        };
        for part in parts {
            if part.get("type").and_then(Value::as_str) != Some("image_url") {
                continue;
            }
            let image = input_images.get(image_index);
            part["image_url"] = json!({
                "redacted": true,
                "reason": "image_bytes",
                "attachment_id": image.map(|item| item.attachment_id.as_str()),
                "mime_type": image.map(|item| item.mime_type.as_str()),
                "byte_size": image.map(|item| item.byte_size),
            });
            image_index += 1;
        }
    }
    value
}

#[derive(Debug, Default)]
pub struct FakeChatCompletionsClient {
    scripts: VecDeque<Result<Vec<ChatCompletionsEvent>, ChatCompletionsStreamError>>,
    requests: Vec<ChatCompletionsRequest>,
}

impl FakeChatCompletionsClient {
    pub fn new(
        scripts: impl IntoIterator<Item = Result<Vec<ChatCompletionsEvent>, ChatCompletionsStreamError>>,
    ) -> Self {
        Self {
            scripts: scripts.into_iter().collect(),
            requests: Vec::new(),
        }
    }

    pub fn requests(&self) -> &[ChatCompletionsRequest] {
        &self.requests
    }
}

impl ChatCompletionsClient for FakeChatCompletionsClient {
    fn stream(
        &mut self,
        request: ChatCompletionsRequest,
    ) -> Result<Vec<ChatCompletionsEvent>, ChatCompletionsStreamError> {
        self.requests.push(request);
        self.scripts.pop_front().unwrap_or_else(|| {
            Err(ChatCompletionsStreamError::Transport(
                "no fake script".into(),
            ))
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrainEventContext {
    pub wake_id: String,
    pub session_id: SessionId,
}

impl BrainEventContext {
    pub fn new(wake_id: impl Into<String>, session_id: SessionId) -> Self {
        Self {
            wake_id: wake_id.into(),
            session_id,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChatCompletionsFinalMessage {
    pub text: Option<String>,
    pub thinking: Option<String>,
    pub stop_reason: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Default)]
pub struct ChatCompletionsEventMapper {
    saw_text_delta: bool,
    think_scanner: LiteralThinkScanner,
}

impl ChatCompletionsEventMapper {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn map_started(&self, context: &BrainEventContext) -> Vec<BrainWakeStreamItem> {
        vec![brain_event_item(context, BrainEvent::Started)]
    }

    pub fn map_provider_event(
        &mut self,
        context: &BrainEventContext,
        provider_event: &ChatCompletionsEvent,
    ) -> Vec<BrainWakeStreamItem> {
        match provider_event {
            ChatCompletionsEvent::ContentDelta(text) => self.map_text_delta(context, text),
            ChatCompletionsEvent::ReasoningDelta { text, field: _ } => non_empty_event(
                context,
                BrainEvent::ReasoningDelta {
                    text: text.clone(),
                    format: Some(CANONICAL_REASONING_FORMAT.to_string()),
                },
                !text.is_empty(),
            ),
            ChatCompletionsEvent::ProviderError(message) => vec![brain_event_item(
                context,
                BrainEvent::ProviderStatus {
                    level: BrainProviderStatusLevel::Error,
                    message: format!("Provider error: {message}"),
                    metadata_json: None,
                },
            )],
            ChatCompletionsEvent::ToolCallMalformed(call) => vec![brain_event_item(
                context,
                BrainEvent::ProviderStatus {
                    level: BrainProviderStatusLevel::Error,
                    message: format!(
                        "Provider emitted a malformed tool-call fragment at index {}: {}",
                        call.index,
                        call.diagnostics.join("; ")
                    ),
                    metadata_json: Some(
                        json!({
                            "kind": "malformed_tool_call",
                            "index": call.index,
                            "id": call.id,
                            "name": call.name,
                            "diagnostics": call.diagnostics,
                        })
                        .to_string(),
                    ),
                },
            )],
            ChatCompletionsEvent::Finished { finish_reason } => {
                let mut items = self.finish_text_scanner(context);
                if let Some(reason) = finish_reason {
                    if reason != "stop" && reason != "tool_calls" {
                        items.push(brain_event_item(
                            context,
                            BrainEvent::ProviderStatus {
                                level: BrainProviderStatusLevel::Info,
                                message: format!("Provider finished with reason: {reason}"),
                                metadata_json: Some(json!({"finish_reason": reason}).to_string()),
                            },
                        ));
                    }
                }
                if finish_reason.as_deref() != Some("length") {
                    items.push(brain_event_item(context, BrainEvent::Finished));
                }
                items
            }
            ChatCompletionsEvent::Usage(_)
            | ChatCompletionsEvent::ToolCallDelta { .. }
            | ChatCompletionsEvent::ToolCallFinished(_) => Vec::new(),
        }
    }

    pub fn map_final_message(
        &mut self,
        context: &BrainEventContext,
        message: ChatCompletionsFinalMessage,
    ) -> Vec<BrainWakeStreamItem> {
        if self.saw_text_delta {
            return Vec::new();
        }

        let mut items = Vec::new();
        if let Some(thinking) = message.thinking.filter(|value| !value.trim().is_empty()) {
            items.push(brain_event_item(
                context,
                BrainEvent::ReasoningDelta {
                    text: thinking,
                    format: Some("chat-completions-thinking".to_string()),
                },
            ));
        }

        if let Some(text) = message.text.filter(|value| !value.trim().is_empty()) {
            items.extend(self.map_text_delta(context, &text));
            items.extend(self.finish_text_scanner(context));
        }

        if !items.is_empty() {
            return items;
        }

        if message.stop_reason.as_deref() == Some("error") {
            if let Some(error_message) = message.error_message {
                let trimmed = error_message.trim();
                if !trimmed.is_empty() {
                    items.push(brain_event_item(
                        context,
                        BrainEvent::TextDelta {
                            text: format!("LLM error: {trimmed}"),
                        },
                    ));
                }
            }
        }
        items
    }

    pub fn finish_text_scanner(&mut self, context: &BrainEventContext) -> Vec<BrainWakeStreamItem> {
        let events = self.think_scanner.finish();
        self.map_scanner_events(context, events)
    }

    fn map_text_delta(
        &mut self,
        context: &BrainEventContext,
        text: &str,
    ) -> Vec<BrainWakeStreamItem> {
        let events = self.think_scanner.push(text);
        self.map_scanner_events(context, events)
    }

    fn map_scanner_events(
        &mut self,
        context: &BrainEventContext,
        events: Vec<LiteralThinkEvent>,
    ) -> Vec<BrainWakeStreamItem> {
        events
            .into_iter()
            .map(|event| match event {
                LiteralThinkEvent::Text(text) => {
                    self.saw_text_delta = true;
                    brain_event_item(context, BrainEvent::TextDelta { text })
                }
                LiteralThinkEvent::Reasoning(text) => brain_event_item(
                    context,
                    BrainEvent::ReasoningDelta {
                        text,
                        format: Some("literal-think-tag".to_string()),
                    },
                ),
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LiteralThinkEvent {
    Text(String),
    Reasoning(String),
}

#[derive(Debug, Default)]
struct LiteralThinkScanner {
    buffer: String,
    in_think: bool,
}

impl LiteralThinkScanner {
    fn push(&mut self, text: &str) -> Vec<LiteralThinkEvent> {
        self.buffer.push_str(text);
        self.drain(false)
    }

    fn finish(&mut self) -> Vec<LiteralThinkEvent> {
        self.drain(true)
    }

    fn drain(&mut self, finishing: bool) -> Vec<LiteralThinkEvent> {
        const OPEN_TAG: &str = "<think>";
        const CLOSE_TAG: &str = "</think>";

        let mut events = Vec::new();
        loop {
            if self.buffer.is_empty() {
                break;
            }

            if self.in_think {
                if let Some(close) = self.buffer.find(CLOSE_TAG) {
                    push_literal_event(
                        &mut events,
                        LiteralThinkEvent::Reasoning(self.buffer[..close].to_string()),
                    );
                    self.buffer.replace_range(..close + CLOSE_TAG.len(), "");
                    self.in_think = false;
                    continue;
                }

                let keep = if finishing {
                    0
                } else {
                    partial_tag_suffix_len(&self.buffer, CLOSE_TAG)
                };
                let emit_len = self.buffer.len().saturating_sub(keep);
                if emit_len == 0 {
                    break;
                }
                push_literal_event(
                    &mut events,
                    LiteralThinkEvent::Reasoning(self.buffer[..emit_len].to_string()),
                );
                self.buffer.replace_range(..emit_len, "");
                break;
            }

            if let Some(open) = self.buffer.find(OPEN_TAG) {
                push_literal_event(
                    &mut events,
                    LiteralThinkEvent::Text(self.buffer[..open].to_string()),
                );
                self.buffer.replace_range(..open + OPEN_TAG.len(), "");
                self.in_think = true;
                continue;
            }

            let keep = if finishing {
                0
            } else {
                partial_tag_suffix_len(&self.buffer, OPEN_TAG)
            };
            let emit_len = self.buffer.len().saturating_sub(keep);
            if emit_len == 0 {
                break;
            }
            push_literal_event(
                &mut events,
                LiteralThinkEvent::Text(self.buffer[..emit_len].to_string()),
            );
            self.buffer.replace_range(..emit_len, "");
            break;
        }
        events
    }
}

fn partial_tag_suffix_len(text: &str, tag: &str) -> usize {
    let max = text.len().min(tag.len().saturating_sub(1));
    for len in (1..=max).rev() {
        if text.is_char_boundary(text.len() - len) && tag.starts_with(&text[text.len() - len..]) {
            return len;
        }
    }
    0
}

fn push_literal_event(events: &mut Vec<LiteralThinkEvent>, event: LiteralThinkEvent) {
    match &event {
        LiteralThinkEvent::Text(text) | LiteralThinkEvent::Reasoning(text) if text.is_empty() => {}
        _ => events.push(event),
    }
}

fn brain_event_item(context: &BrainEventContext, event: BrainEvent) -> BrainWakeStreamItem {
    BrainWakeStreamItem::event(BrainEventEnvelope {
        wake_id: context.wake_id.clone(),
        session_id: context.session_id.clone(),
        event,
    })
}

fn success_actions_item(context: &BrainEventContext) -> BrainWakeStreamItem {
    BrainWakeStreamItem::actions(BrainActionBatch {
        wake_id: context.wake_id.clone(),
        session_id: context.session_id.clone(),
        actions: Vec::new(),
    })
}

fn wake_failed_item(
    context: &BrainEventContext,
    kind: CoreErrorKind,
    message: impl Into<String>,
) -> BrainWakeStreamItem {
    BrainWakeStreamItem::wake_failed(BrainWakeFailure {
        wake_id: context.wake_id.clone(),
        session_id: context.session_id.clone(),
        kind,
        reason_code: None,
        message: message.into(),
    })
}

fn wake_failed_item_with_reason(
    context: &BrainEventContext,
    kind: CoreErrorKind,
    reason_code: impl Into<String>,
    message: impl Into<String>,
) -> BrainWakeStreamItem {
    BrainWakeStreamItem::wake_failed(BrainWakeFailure {
        wake_id: context.wake_id.clone(),
        session_id: context.session_id.clone(),
        kind,
        reason_code: Some(reason_code.into()),
        message: message.into(),
    })
}

fn tool_calls_are_actionable(calls: &[PendingChatFunctionCall]) -> bool {
    !calls.is_empty()
        && calls.iter().all(|call| {
            matches!(
                serde_json::from_str::<Value>(&call.arguments_json),
                Ok(Value::Object(_))
            )
        })
}

fn malformed_tool_call_summary(
    finish_reason: Option<&str>,
    calls: &[MalformedChatFunctionCall],
) -> String {
    let diagnostics = calls
        .iter()
        .map(|call| format!("index {}: {}", call.index, call.diagnostics.join("; ")))
        .collect::<Vec<_>>()
        .join(" | ");
    format!(
        "chat-completions provider finished with reason {} and {} malformed tool-call fragment(s): {diagnostics}",
        finish_reason.unwrap_or("unknown"),
        calls.len()
    )
}

fn record_provider_event(counts: &mut BTreeMap<String, usize>, event: &ChatCompletionsEvent) {
    let keys: &[&str] = match event {
        ChatCompletionsEvent::ContentDelta(_) => &["content_delta"],
        ChatCompletionsEvent::ReasoningDelta { field, .. } => {
            increment_count(counts, &format!("reasoning_delta:{field}"));
            &["reasoning_delta"]
        }
        ChatCompletionsEvent::ToolCallDelta { .. } => &["tool_call_delta"],
        ChatCompletionsEvent::ToolCallFinished(_) => &["tool_call_finished"],
        ChatCompletionsEvent::ToolCallMalformed(_) => &["tool_call_malformed"],
        ChatCompletionsEvent::Usage(_) => &["usage"],
        ChatCompletionsEvent::Finished { .. } => &["finished"],
        ChatCompletionsEvent::ProviderError(_) => &["provider_error"],
    };
    for key in keys {
        increment_count(counts, key);
    }
}

fn increment_count(counts: &mut BTreeMap<String, usize>, key: &str) {
    let count = counts.entry(key.to_string()).or_default();
    *count = count.saturating_add(1);
}

fn assistant_tool_call_message(
    content: &str,
    reasoning_content: &str,
    calls: &[PendingChatFunctionCall],
) -> ChatCompletionMessage {
    ChatCompletionMessage {
        role: ChatMessageRole::Assistant,
        content: (!content.is_empty()).then(|| content.to_string()),
        reasoning_content: (!reasoning_content.is_empty()).then(|| reasoning_content.to_string()),
        name: None,
        tool_call_id: None,
        tool_calls: calls
            .iter()
            .map(|call| ChatAssistantToolCall {
                id: call
                    .id
                    .clone()
                    .unwrap_or_else(|| format!("call_{}", call.index)),
                kind: "function".to_string(),
                function: ChatFunctionCall {
                    name: call.name.clone(),
                    arguments: call.arguments_json.clone(),
                },
            })
            .collect(),
    }
}

fn assistant_partial_message(content: &str, reasoning_content: &str) -> ChatCompletionMessage {
    ChatCompletionMessage {
        role: ChatMessageRole::Assistant,
        content: (!content.is_empty()).then(|| content.to_string()),
        reasoning_content: (!reasoning_content.is_empty()).then(|| reasoning_content.to_string()),
        name: None,
        tool_call_id: None,
        tool_calls: Vec::new(),
    }
}

fn malformed_tool_call_recovery_feedback(
    finish_reason: Option<&str>,
    calls: &[MalformedChatFunctionCall],
    attempt: u32,
) -> String {
    let diagnostics = calls
        .iter()
        .map(|call| {
            let name = call.name.as_deref().unwrap_or("unknown tool");
            format!(
                "{name} at index {}: {}",
                call.index,
                call.diagnostics.join("; ")
            )
        })
        .collect::<Vec<_>>()
        .join(" | ");
    let output_limit_guidance = if finish_reason == Some("length") {
        " The provider also reached its output limit, so keep the corrected call concise."
    } else {
        ""
    };
    format!(
        "[Rusty Crew tool-call recovery {attempt}] The previous assistant response did not produce executable tool arguments: {diagnostics}. No tool from that response was executed. Retry the intended tool call with one complete JSON object, or continue without the tool if it is unnecessary. Do not repeat text already emitted.{output_limit_guidance}"
    )
}

fn malformed_tool_call_recovery_status(
    context: &BrainEventContext,
    trigger_reason_code: &str,
    attempt: u32,
    attention_threshold: u32,
    calls: &[MalformedChatFunctionCall],
) -> BrainWakeStreamItem {
    brain_event_item(
        context,
        BrainEvent::ProviderStatus {
            level: BrainProviderStatusLevel::Degraded,
            message: format!(
                "Retrying provider after malformed tool call (recovery {attempt}); no malformed call was executed."
            ),
            metadata_json: Some(
                json!({
                    "kind": "malformed_tool_call_recovery",
                    "trigger_reason_code": trigger_reason_code,
                    "attempt": attempt,
                    "attention_threshold": attention_threshold,
                    "malformed_call_count": calls.len(),
                    "tool_names": calls
                        .iter()
                        .filter_map(|call| call.name.as_deref())
                        .collect::<Vec<_>>(),
                })
                .to_string(),
            ),
        },
    )
}

fn no_progress_correction_status(
    context: &BrainEventContext,
    tool_name: &str,
    consecutive_samples: u32,
    attention_threshold: u32,
) -> BrainWakeStreamItem {
    brain_event_item(
        context,
        BrainEvent::ProviderStatus {
            level: BrainProviderStatusLevel::Degraded,
            message: format!(
                "Tool {tool_name} repeated an equivalent failed result; corrective guidance was returned to the model."
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

fn no_progress_attention_status(
    context: &BrainEventContext,
    reason_code: &str,
    summary: &str,
    consecutive_samples: u32,
) -> BrainWakeStreamItem {
    brain_event_item(
        context,
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

fn no_progress_attention(
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

fn progress_json_fingerprint(value: &impl Serialize) -> String {
    serde_json::to_vec(value)
        .map(|bytes| format!("{:x}", Sha256::digest(bytes)))
        .unwrap_or_else(|_| progress_fingerprint(&["serialization_failed"]))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ChatCompletionsProviderStateV1 {
    kind: String,
    strategy_id: String,
    payload_version: String,
    messages: Vec<ChatCompletionMessage>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ChatCompletionsContinuationStateV1 {
    kind: String,
    payload_version: String,
    messages: Vec<ChatCompletionMessage>,
    durable_messages: Vec<ChatCompletionMessage>,
    input_images: Vec<ChatCompletionsInputImage>,
    no_progress_state: BrainNoProgressState,
    provider_request_count: usize,
    tool_round_count: usize,
    provider_event_counts: BTreeMap<String, usize>,
    provider_request_debug_samples: Vec<Value>,
}

fn chat_completions_continuation_state(
    state: &BrainContinuationPayload,
) -> Result<ChatCompletionsContinuationStateV1, String> {
    if state.module_id != MODULE_ID || state.payload_version != CONTINUATION_PAYLOAD_VERSION {
        return Err(format!(
            "unsupported chat-completions continuation identity {} {}",
            state.module_id, state.payload_version
        ));
    }
    let fingerprint = continuation_payload_fingerprint(&state.payload)?;
    if fingerprint != state.payload_fingerprint {
        return Err("chat-completions continuation fingerprint mismatch".to_string());
    }
    let payload: ChatCompletionsContinuationStateV1 = serde_json::from_value(state.payload.clone())
        .map_err(|error| format!("chat-completions continuation payload is malformed: {error}"))?;
    if payload.kind != MODULE_ID || payload.payload_version != CONTINUATION_PAYLOAD_VERSION {
        return Err("chat-completions continuation payload identity mismatch".to_string());
    }
    Ok(payload)
}

#[allow(clippy::too_many_arguments)]
fn chat_completions_continuation_output(
    messages: Vec<ChatCompletionMessage>,
    durable_messages: Vec<ChatCompletionMessage>,
    input_images: Vec<ChatCompletionsInputImage>,
    no_progress_state: BrainNoProgressState,
    provider_request_count: usize,
    tool_round_count: usize,
    provider_event_counts: BTreeMap<String, usize>,
    provider_request_debug_samples: Vec<Value>,
) -> Result<BrainContinuationPayload, String> {
    let payload = serde_json::to_value(ChatCompletionsContinuationStateV1 {
        kind: MODULE_ID.to_string(),
        payload_version: CONTINUATION_PAYLOAD_VERSION.to_string(),
        messages,
        durable_messages,
        input_images,
        no_progress_state,
        provider_request_count,
        tool_round_count,
        provider_event_counts,
        provider_request_debug_samples,
    })
    .map_err(|error| format!("serialize chat-completions continuation payload: {error}"))?;
    let payload_fingerprint = continuation_payload_fingerprint(&payload)?;
    Ok(BrainContinuationPayload {
        module_id: MODULE_ID.to_string(),
        payload_version: CONTINUATION_PAYLOAD_VERSION.to_string(),
        payload_fingerprint,
        payload,
    })
}

fn continuation_payload_fingerprint(payload: &Value) -> Result<String, String> {
    serde_json::to_vec(payload)
        .map(|bytes| format!("{:x}", Sha256::digest(bytes)))
        .map_err(|error| format!("fingerprint chat-completions continuation payload: {error}"))
}

fn chat_completions_messages_with_provider_state(
    config: &ChatCompletionsChatConfig,
    provider_state: Option<&BrainWakeProviderStateInput>,
    current_messages: Vec<ChatCompletionMessage>,
) -> Result<Vec<ChatCompletionMessage>, String> {
    let Some(state) = provider_state else {
        return Ok(current_messages);
    };
    if state.module_id != MODULE_ID {
        return Err(format!(
            "chat completions provider state belongs to module {}",
            state.module_id
        ));
    }
    if state.strategy_id != config.provider_state_strategy_id {
        return Err(format!(
            "chat completions provider state strategy {} does not match {}",
            state.strategy_id, config.provider_state_strategy_id
        ));
    }
    if state.payload_version != PROVIDER_STATE_PAYLOAD_VERSION {
        return Err(format!(
            "chat completions provider state payload version {} is unsupported",
            state.payload_version
        ));
    }
    let payload: ChatCompletionsProviderStateV1 = serde_json::from_value(state.payload.clone())
        .map_err(|error| {
            format!("chat completions provider state payload is malformed: {error}")
        })?;
    if payload.kind != MODULE_ID
        || payload.strategy_id != config.provider_state_strategy_id
        || payload.payload_version != PROVIDER_STATE_PAYLOAD_VERSION
    {
        return Err("chat completions provider state payload identity mismatch".to_string());
    }

    let mut merged = Vec::with_capacity(payload.messages.len() + current_messages.len());
    merged.extend(
        current_messages
            .iter()
            .filter(|message| message.role == ChatMessageRole::System)
            .cloned(),
    );
    merged.extend(payload.messages.into_iter().filter_map(|message| {
        (message.role != ChatMessageRole::System)
            .then(|| message_for_reasoning_history(message, config.reasoning_history))
    }));
    merged.extend(
        current_messages
            .into_iter()
            .filter(|message| message.role != ChatMessageRole::System),
    );
    Ok(merged)
}

fn chat_completions_provider_state_output(
    _context: &BrainEventContext,
    config: &ChatCompletionsChatConfig,
    previous_state: Option<&BrainWakeProviderStateInput>,
    messages: Vec<ChatCompletionMessage>,
) -> Option<BrainWakeProviderStateOutput> {
    let payload = ChatCompletionsProviderStateV1 {
        kind: MODULE_ID.to_string(),
        strategy_id: config.provider_state_strategy_id.clone(),
        payload_version: PROVIDER_STATE_PAYLOAD_VERSION.to_string(),
        messages: messages
            .into_iter()
            .filter(|message| message.role != ChatMessageRole::System)
            .map(|message| message_for_reasoning_history(message, config.reasoning_history))
            .collect(),
    };
    Some(BrainWakeProviderStateOutput::Replace {
        state: BrainWakeProviderStateUpdate {
            module_id: MODULE_ID.to_string(),
            strategy_id: config.provider_state_strategy_id.clone(),
            profile_fingerprint: previous_state
                .map(|state| state.profile_fingerprint.clone())
                .unwrap_or_else(|| "profile-fingerprint".to_string()),
            provider_fingerprint: previous_state
                .map(|state| state.provider_fingerprint.clone())
                .unwrap_or_else(|| "provider-fingerprint".to_string()),
            payload_version: PROVIDER_STATE_PAYLOAD_VERSION.to_string(),
            payload: serde_json::to_value(payload).unwrap_or_else(|_| json!({})),
            ttl_ms: Some(PROVIDER_STATE_TTL_MS),
        },
    })
}

fn message_for_reasoning_history(
    mut message: ChatCompletionMessage,
    history: ChatCompletionsReasoningHistory,
) -> ChatCompletionMessage {
    let preserve_reasoning = match history {
        ChatCompletionsReasoningHistory::PreserveAll => true,
        ChatCompletionsReasoningHistory::ToolCallsOnly => {
            message.role == ChatMessageRole::Assistant && !message.tool_calls.is_empty()
        }
        ChatCompletionsReasoningHistory::ProviderDefault
        | ChatCompletionsReasoningHistory::Discard => false,
    };
    if !preserve_reasoning {
        message.reasoning_content = None;
    }
    message
}

fn non_empty_event(
    context: &BrainEventContext,
    event: BrainEvent,
    emit: bool,
) -> Vec<BrainWakeStreamItem> {
    if emit {
        vec![brain_event_item(context, event)]
    } else {
        Vec::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChatCompletionsStreamError {
    #[error("provider stream missing {0}")]
    MissingField(&'static str),
    #[error("provider request timeout")]
    RequestTimeout,
    #[error("provider request cancelled")]
    Cancelled,
    #[error("provider stream closed before [DONE] or finish reason")]
    ClosedBeforeFinish,
    #[error("provider returned error: {0}")]
    ProviderError(String),
    #[error("provider transport error: {0}")]
    Transport(String),
}

pub trait ChatCompletionsClient {
    fn stream(
        &mut self,
        request: ChatCompletionsRequest,
    ) -> Result<Vec<ChatCompletionsEvent>, ChatCompletionsStreamError>;

    fn stream_observed(
        &mut self,
        request: ChatCompletionsRequest,
        on_event: &mut dyn FnMut(&ChatCompletionsEvent),
    ) -> Result<Vec<ChatCompletionsEvent>, ChatCompletionsStreamError> {
        let events = self.stream(request)?;
        for event in &events {
            on_event(event);
        }
        Ok(events)
    }
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

pub struct LiveChatCompletionsClient {
    client: AsyncHttpClient,
    endpoint: String,
    bearer_token: Option<String>,
    provider_request_timeout: Option<Duration>,
    cancellation: ProviderCancellation,
    runtime: Runtime,
}

impl LiveChatCompletionsClient {
    pub fn new(
        base_url: impl Into<String>,
        api_key: Option<String>,
        provider_request_timeout_ms: Option<u64>,
        cancellation: ProviderCancellation,
    ) -> Result<Self, ChatCompletionsStreamError> {
        let endpoint = chat_completions_endpoint(&base_url.into());
        let client = AsyncHttpClient::builder()
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|error| ChatCompletionsStreamError::Transport(error.to_string()))?;
        let runtime = RuntimeBuilder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| ChatCompletionsStreamError::Transport(error.to_string()))?;
        Ok(Self {
            client,
            endpoint,
            bearer_token: api_key,
            provider_request_timeout: provider_request_timeout_ms.map(Duration::from_millis),
            cancellation,
            runtime,
        })
    }
}

impl ChatCompletionsClient for LiveChatCompletionsClient {
    fn stream(
        &mut self,
        request: ChatCompletionsRequest,
    ) -> Result<Vec<ChatCompletionsEvent>, ChatCompletionsStreamError> {
        self.stream_observed(request, &mut |_| {})
    }

    fn stream_observed(
        &mut self,
        request: ChatCompletionsRequest,
        on_event: &mut dyn FnMut(&ChatCompletionsEvent),
    ) -> Result<Vec<ChatCompletionsEvent>, ChatCompletionsStreamError> {
        let mut request = self.client.post(&self.endpoint).json(&request);
        if let Some(bearer_token) = &self.bearer_token {
            request = request.bearer_auth(bearer_token);
        }
        self.runtime.block_on(stream_chat_completions_response(
            request,
            self.provider_request_timeout,
            &self.cancellation,
            on_event,
        ))
    }
}

const PROVIDER_CANCELLATION_POLL_INTERVAL: Duration = Duration::from_millis(25);

async fn stream_chat_completions_response(
    request: reqwest::RequestBuilder,
    provider_request_timeout: Option<Duration>,
    cancellation: &ProviderCancellation,
    on_event: &mut dyn FnMut(&ChatCompletionsEvent),
) -> Result<Vec<ChatCompletionsEvent>, ChatCompletionsStreamError> {
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
        return Err(ChatCompletionsStreamError::Transport(format!(
            "HTTP {status}: {body}"
        )));
    }
    parse_async_sse_response(&mut response, cancellation, deadline, on_event).await
}

async fn parse_async_sse_response(
    response: &mut AsyncHttpResponse,
    cancellation: &ProviderCancellation,
    deadline: Option<Instant>,
    on_event: &mut dyn FnMut(&ChatCompletionsEvent),
) -> Result<Vec<ChatCompletionsEvent>, ChatCompletionsStreamError> {
    let mut parser = ChatCompletionsSseParser::default();
    while let Some(chunk) = next_provider_chunk(response, cancellation, deadline).await? {
        parser.push_text(&String::from_utf8_lossy(&chunk), on_event)?;
    }
    parser.finish(on_event)
}

async fn read_provider_response_text(
    response: &mut AsyncHttpResponse,
    cancellation: &ProviderCancellation,
    deadline: Option<Instant>,
) -> Result<String, ChatCompletionsStreamError> {
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
) -> Result<Option<Vec<u8>>, ChatCompletionsStreamError> {
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
) -> Result<(), ChatCompletionsStreamError> {
    if cancellation.is_cancelled() {
        return Err(ChatCompletionsStreamError::Cancelled);
    }
    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        return Err(ChatCompletionsStreamError::RequestTimeout);
    }
    Ok(())
}

fn provider_poll_duration(
    deadline: Option<Instant>,
) -> Result<Duration, ChatCompletionsStreamError> {
    let Some(deadline) = deadline else {
        return Ok(PROVIDER_CANCELLATION_POLL_INTERVAL);
    };
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .ok_or(ChatCompletionsStreamError::RequestTimeout)?;
    Ok(remaining.min(PROVIDER_CANCELLATION_POLL_INTERVAL))
}

fn chat_completions_endpoint(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/chat/completions")
    }
}

fn transport_error(error: reqwest::Error) -> ChatCompletionsStreamError {
    ChatCompletionsStreamError::Transport(error.to_string())
}

pub fn parse_sse_reader<R: Read>(
    reader: &mut R,
    on_event: &mut dyn FnMut(&ChatCompletionsEvent),
) -> Result<Vec<ChatCompletionsEvent>, ChatCompletionsStreamError> {
    let mut parser = ChatCompletionsSseParser::default();
    let mut buffer = [0_u8; 8192];

    loop {
        let read = reader.read(&mut buffer).map_err(io_transport_error)?;
        if read == 0 {
            break;
        }
        let chunk = String::from_utf8_lossy(&buffer[..read]);
        parser.push_text(&chunk, on_event)?;
    }

    parser.finish(on_event)
}

fn io_transport_error(error: std::io::Error) -> ChatCompletionsStreamError {
    ChatCompletionsStreamError::Transport(error.to_string())
}

#[derive(Default)]
struct ChatCompletionsSseParser {
    pending_line: String,
    data_lines: Vec<String>,
    accumulator: ChatCompletionsAccumulator,
}

impl ChatCompletionsSseParser {
    fn push_text(
        &mut self,
        chunk: &str,
        on_event: &mut dyn FnMut(&ChatCompletionsEvent),
    ) -> Result<(), ChatCompletionsStreamError> {
        self.pending_line.push_str(chunk);
        while let Some(newline_index) = self.pending_line.find('\n') {
            let line = self.pending_line[..newline_index].to_string();
            self.pending_line.replace_range(..=newline_index, "");
            self.handle_sse_line(&line, on_event)?;
        }
        Ok(())
    }

    fn finish(
        mut self,
        on_event: &mut dyn FnMut(&ChatCompletionsEvent),
    ) -> Result<Vec<ChatCompletionsEvent>, ChatCompletionsStreamError> {
        if !self.pending_line.is_empty() {
            let line = std::mem::take(&mut self.pending_line);
            self.handle_sse_line(&line, on_event)?;
        }
        self.flush_data(on_event)?;
        if !self.accumulator.saw_terminal {
            return Err(ChatCompletionsStreamError::ClosedBeforeFinish);
        }
        Ok(self.accumulator.events)
    }

    fn handle_sse_line(
        &mut self,
        line: &str,
        on_event: &mut dyn FnMut(&ChatCompletionsEvent),
    ) -> Result<(), ChatCompletionsStreamError> {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            return self.flush_data(on_event);
        }
        if let Some(data) = line.strip_prefix("data:") {
            self.data_lines.push(data.trim_start().to_string());
        }
        Ok(())
    }

    fn flush_data(
        &mut self,
        on_event: &mut dyn FnMut(&ChatCompletionsEvent),
    ) -> Result<(), ChatCompletionsStreamError> {
        if self.data_lines.is_empty() {
            return Ok(());
        }
        let data = self.data_lines.join("\n");
        self.data_lines.clear();
        if data == "[DONE]" {
            self.accumulator.finish_at_done(on_event);
            return Ok(());
        }
        let value: Value = serde_json::from_str(&data).map_err(|error| {
            ChatCompletionsStreamError::Transport(format!("invalid SSE JSON: {error}"))
        })?;
        self.accumulator.process_value(value, on_event)
    }
}

#[derive(Default)]
struct ChatCompletionsAccumulator {
    pending_tool_calls: BTreeMap<u32, PendingToolCallBuilder>,
    events: Vec<ChatCompletionsEvent>,
    saw_terminal: bool,
    synthetic_tool_call_count: u32,
}

impl ChatCompletionsAccumulator {
    fn process_value(
        &mut self,
        value: Value,
        on_event: &mut dyn FnMut(&ChatCompletionsEvent),
    ) -> Result<(), ChatCompletionsStreamError> {
        if let Some(error) = provider_error_message(&value) {
            let event = ChatCompletionsEvent::ProviderError(error.clone());
            self.push(event, on_event);
            return Err(ChatCompletionsStreamError::ProviderError(error));
        }

        if let Some(usage) = value.get("usage").and_then(token_usage_from_provider_value) {
            self.push(ChatCompletionsEvent::Usage(usage), on_event);
        }

        let Some(choices) = value.get("choices").and_then(Value::as_array) else {
            return Ok(());
        };

        for choice in choices {
            if let Some(delta) = choice.get("delta") {
                self.process_delta(delta, on_event)?;
            }
            let finish_reason = choice
                .get("finish_reason")
                .and_then(Value::as_str)
                .map(str::to_string);
            if finish_reason.is_some() {
                for call in self.flush_tool_calls() {
                    match call {
                        ClassifiedToolCall::Actionable(call) => {
                            self.push(ChatCompletionsEvent::ToolCallFinished(call), on_event);
                        }
                        ClassifiedToolCall::Malformed(call) => {
                            self.push(ChatCompletionsEvent::ToolCallMalformed(call), on_event);
                        }
                    }
                }
                self.saw_terminal = true;
                self.push(ChatCompletionsEvent::Finished { finish_reason }, on_event);
            }
        }
        Ok(())
    }

    fn process_delta(
        &mut self,
        delta: &Value,
        on_event: &mut dyn FnMut(&ChatCompletionsEvent),
    ) -> Result<(), ChatCompletionsStreamError> {
        if let Some(content) = delta.get("content").and_then(Value::as_str) {
            if !content.is_empty() {
                self.push(
                    ChatCompletionsEvent::ContentDelta(content.to_string()),
                    on_event,
                );
            }
        }

        for field in [
            "reasoning_content",
            "reasoning",
            "reasoning_delta",
            "thinking",
        ] {
            if let Some(text) = delta.get(field).and_then(Value::as_str) {
                if !text.is_empty() {
                    self.push(
                        ChatCompletionsEvent::ReasoningDelta {
                            text: text.to_string(),
                            field: field.to_string(),
                        },
                        on_event,
                    );
                }
            }
        }

        let Some(tool_calls) = delta.get("tool_calls").and_then(Value::as_array) else {
            return Ok(());
        };
        for raw_call in tool_calls {
            let index = match raw_call.get("index").and_then(Value::as_u64) {
                Some(index) if u32::try_from(index).is_ok() => index as u32,
                _ => {
                    let diagnostic = match raw_call.get("index") {
                        None => "tool call index is missing".to_string(),
                        Some(value) => format!(
                            "tool call index must be an unsigned 32-bit integer, found {}",
                            json_value_kind(value)
                        ),
                    };
                    let index = u32::MAX.saturating_sub(self.synthetic_tool_call_count);
                    self.synthetic_tool_call_count =
                        self.synthetic_tool_call_count.saturating_add(1);
                    self.push(
                        malformed_tool_call_from_raw(index, raw_call, diagnostic),
                        on_event,
                    );
                    continue;
                }
            };
            let id = raw_call
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string);
            let function = raw_call.get("function");
            let name = function
                .and_then(|value| value.get("name"))
                .and_then(Value::as_str)
                .map(str::to_string);
            let arguments_value = function.and_then(|value| value.get("arguments"));
            let arguments_delta = arguments_value
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();

            let builder = self.pending_tool_calls.entry(index).or_default();
            if let Some(id) = &id {
                builder.id = Some(id.clone());
            }
            if let Some(name) = &name {
                builder.name = Some(name.clone());
            }
            if let Some(arguments_value) = arguments_value {
                if arguments_value.is_string() {
                    builder.arguments_observed = true;
                    builder.arguments_json.push_str(&arguments_delta);
                } else {
                    builder.argument_diagnostics.push(format!(
                        "function.arguments must be a string, found {}",
                        json_value_kind(arguments_value)
                    ));
                }
            }

            self.push(
                ChatCompletionsEvent::ToolCallDelta {
                    index,
                    id,
                    name,
                    arguments_delta,
                },
                on_event,
            );
        }
        Ok(())
    }

    fn flush_tool_calls(&mut self) -> Vec<ClassifiedToolCall> {
        let pending = std::mem::take(&mut self.pending_tool_calls);
        pending
            .into_iter()
            .map(|(index, builder)| classify_tool_call(index, builder))
            .collect()
    }

    fn finish_at_done(&mut self, on_event: &mut dyn FnMut(&ChatCompletionsEvent)) {
        for call in self.flush_tool_calls() {
            match call {
                ClassifiedToolCall::Actionable(call) => {
                    self.push(ChatCompletionsEvent::ToolCallFinished(call), on_event);
                }
                ClassifiedToolCall::Malformed(call) => {
                    self.push(ChatCompletionsEvent::ToolCallMalformed(call), on_event);
                }
            }
        }
        if !self.saw_terminal {
            self.saw_terminal = true;
            self.push(
                ChatCompletionsEvent::Finished {
                    finish_reason: None,
                },
                on_event,
            );
        }
    }

    fn push(
        &mut self,
        event: ChatCompletionsEvent,
        on_event: &mut dyn FnMut(&ChatCompletionsEvent),
    ) {
        on_event(&event);
        self.events.push(event);
    }
}

#[derive(Default)]
struct PendingToolCallBuilder {
    id: Option<String>,
    name: Option<String>,
    arguments_json: String,
    arguments_observed: bool,
    argument_diagnostics: Vec<String>,
}

enum ClassifiedToolCall {
    Actionable(PendingChatFunctionCall),
    Malformed(MalformedChatFunctionCall),
}

fn classify_tool_call(index: u32, builder: PendingToolCallBuilder) -> ClassifiedToolCall {
    let arguments_json = builder.arguments_json;
    let mut diagnostics = builder.argument_diagnostics;
    let name = builder.name.filter(|name| !name.trim().is_empty());
    if name.is_none() {
        diagnostics.push("function.name is missing or empty".to_string());
    }
    if !builder.arguments_observed {
        diagnostics.push("function.arguments is missing".to_string());
    } else if arguments_json.trim().is_empty() {
        diagnostics.push("function.arguments is empty".to_string());
    } else {
        match serde_json::from_str::<Value>(&arguments_json) {
            Ok(Value::Object(_)) => {}
            Ok(value) => diagnostics.push(format!(
                "function.arguments must decode to a JSON object, found {}",
                json_value_kind(&value)
            )),
            Err(error) => diagnostics.push(format!("function.arguments is invalid JSON: {error}")),
        }
    }

    if diagnostics.is_empty() {
        ClassifiedToolCall::Actionable(PendingChatFunctionCall {
            index,
            id: builder.id,
            name: name.expect("validated tool-call name"),
            arguments_json,
        })
    } else {
        ClassifiedToolCall::Malformed(MalformedChatFunctionCall {
            index,
            id: builder.id,
            name,
            arguments_json,
            diagnostics,
        })
    }
}

fn malformed_tool_call_from_raw(
    index: u32,
    raw_call: &Value,
    diagnostic: String,
) -> ChatCompletionsEvent {
    let function = raw_call.get("function");
    let arguments_json = function
        .and_then(|value| value.get("arguments"))
        .map(|value| match value {
            Value::String(value) => value.clone(),
            value => value.to_string(),
        })
        .unwrap_or_default();
    ChatCompletionsEvent::ToolCallMalformed(MalformedChatFunctionCall {
        index,
        id: raw_call
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string),
        name: function
            .and_then(|value| value.get("name"))
            .and_then(Value::as_str)
            .filter(|name| !name.trim().is_empty())
            .map(str::to_string),
        arguments_json,
        diagnostics: vec![diagnostic],
    })
}

fn json_value_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn provider_error_message(value: &Value) -> Option<String> {
    let error = value.get("error")?;
    error
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| error.as_str())
        .map(str::to_string)
}

fn token_usage_from_provider_value(value: &Value) -> Option<ChatTokenUsage> {
    Some(ChatTokenUsage {
        prompt_tokens: value.get("prompt_tokens")?.as_u64()?,
        completion_tokens: value.get("completion_tokens")?.as_u64()?,
        total_tokens: value.get("total_tokens")?.as_u64()?,
        cached_prompt_tokens: value
            .get("prompt_tokens_details")
            .and_then(|details| details.get("cached_tokens"))
            .and_then(Value::as_u64)
            .unwrap_or(0),
        reasoning_completion_tokens: value
            .get("completion_tokens_details")
            .and_then(|details| details.get("reasoning_tokens"))
            .and_then(Value::as_u64)
            .unwrap_or(0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn parse(input: &str) -> Result<Vec<ChatCompletionsEvent>, ChatCompletionsStreamError> {
        parse_sse_reader(&mut Cursor::new(input.as_bytes()), &mut |_| {})
    }

    fn delayed_chat_completions_server(delay: Duration) -> String {
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
                "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"finish_reason\":null}]}\n\n",
                "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
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

    fn live_chat_request() -> ChatCompletionsRequest {
        ChatCompletionsRequestBuilder::new(ChatCompletionsChatConfig::new("test-model"))
            .build(vec![ChatCompletionMessage::user("hello")])
    }

    #[test]
    fn opted_image_is_serialized_once_and_redacted_from_debug_samples() {
        let image = ChatCompletionsInputImage {
            attachment_id: "attachment-1".to_string(),
            mime_type: "image/png".to_string(),
            bytes_base64: "YWJj".to_string(),
            byte_size: 3,
        };
        let request =
            ChatCompletionsRequestBuilder::new(ChatCompletionsChatConfig::new("test-model"))
                .build_with_images(
                    vec![
                        ChatCompletionMessage::system("system"),
                        ChatCompletionMessage::user("look at this"),
                    ],
                    std::slice::from_ref(&image),
                );
        let wire = serde_json::to_string(&request).expect("serialize image request");
        assert_eq!(wire.matches("data:image/png;base64,YWJj").count(), 1);
        assert_eq!(wire.matches("\"type\":\"image_url\"").count(), 1);

        let debug = chat_completions_debug_request(&request, &[image]);
        let debug_json = serde_json::to_string(&debug).expect("serialize debug request");
        assert!(!debug_json.contains("YWJj"));
        assert!(debug_json.contains("attachment-1"));
        assert!(debug_json.contains("image_bytes"));
    }

    #[test]
    fn live_provider_has_no_request_deadline_by_default() {
        let base_url = delayed_chat_completions_server(Duration::from_millis(100));
        let mut client =
            LiveChatCompletionsClient::new(base_url, None, None, ProviderCancellation::default())
                .expect("create live client");

        let events = client
            .stream(live_chat_request())
            .expect("uncapped provider request should complete");

        assert!(events.iter().any(
            |event| matches!(event, ChatCompletionsEvent::ContentDelta(text) if text == "ok")
        ));
    }

    #[test]
    fn configured_provider_request_deadline_remains_available() {
        let base_url = delayed_chat_completions_server(Duration::from_millis(200));
        let mut client = LiveChatCompletionsClient::new(
            base_url,
            None,
            Some(50),
            ProviderCancellation::default(),
        )
        .expect("create live client");

        assert_eq!(
            client.stream(live_chat_request()),
            Err(ChatCompletionsStreamError::RequestTimeout)
        );
    }

    #[test]
    fn cancellation_interrupts_an_uncapped_provider_request() {
        let base_url = delayed_chat_completions_server(Duration::from_secs(2));
        let cancellation = ProviderCancellation::default();
        let cancel_from_thread = cancellation.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            cancel_from_thread.cancel();
        });
        let mut client = LiveChatCompletionsClient::new(base_url, None, None, cancellation)
            .expect("create live client");
        let started_at = Instant::now();

        assert_eq!(
            client.stream(live_chat_request()),
            Err(ChatCompletionsStreamError::Cancelled)
        );
        assert!(
            started_at.elapsed() < Duration::from_millis(500),
            "cancellation should interrupt the active HTTP future promptly"
        );
    }

    #[test]
    fn builds_chat_completions_request_from_provider_config() {
        let request = ChatCompletionsRequestBuilder::new(ChatCompletionsChatConfig {
            model: "deepseek-flash".to_string(),
            temperature_milli: Some(500),
            reasoning_effort: Some("high".to_string()),
            max_output_tokens: Some(256),
            provider_request_timeout_ms: Some(45_000),
            ..ChatCompletionsChatConfig::new("unused")
        })
        .tools(vec![NeutralBrainTool {
            name: "lookup".to_string(),
            description: "Look something up".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {"query": {"type": "string"}},
                "required": ["query"]
            }),
        }])
        .build(vec![
            ChatCompletionMessage::system("be useful"),
            ChatCompletionMessage::user("hello"),
        ]);

        let value = serde_json::to_value(&request).expect("request json");
        assert_eq!(value["model"], "deepseek-flash");
        assert_eq!(value["stream"], true);
        assert_eq!(value["stream_options"]["include_usage"], true);
        assert_eq!(value["temperature"], 0.5);
        assert_eq!(value["reasoning_effort"], "high");
        assert_eq!(value["max_tokens"], 256);
        assert_eq!(value["tools"][0]["type"], "function");
        assert_eq!(value["tools"][0]["function"]["name"], "lookup");
    }

    #[test]
    fn omits_optional_generation_settings_for_provider_defaults() {
        let request = ChatCompletionsRequestBuilder::new(ChatCompletionsChatConfig::new(
            "provider-default-model",
        ))
        .build(vec![ChatCompletionMessage::user("hello")]);
        let value = serde_json::to_value(&request).expect("request json");

        assert!(value.get("temperature").is_none());
        assert!(value.get("reasoning_effort").is_none());
    }

    #[test]
    fn maps_typed_chat_completions_reasoning_dialects() {
        let request_value = |config: ChatCompletionsChatConfig| {
            serde_json::to_value(
                ChatCompletionsRequestBuilder::new(config)
                    .build(vec![ChatCompletionMessage::user("hello")]),
            )
            .expect("request json")
        };

        let kimi_default = request_value(ChatCompletionsChatConfig {
            wire_dialect: ChatCompletionsWireDialect::Kimi,
            max_output_tokens: Some(KIMI_THINKING_MIN_OUTPUT_TOKENS),
            ..ChatCompletionsChatConfig::new("kimi")
        });
        assert!(kimi_default.get("thinking").is_none());

        let kimi_preserved = request_value(ChatCompletionsChatConfig {
            wire_dialect: ChatCompletionsWireDialect::Kimi,
            thinking_mode: ChatCompletionsThinkingMode::Enabled,
            reasoning_history: ChatCompletionsReasoningHistory::PreserveAll,
            max_output_tokens: Some(KIMI_THINKING_MIN_OUTPUT_TOKENS),
            ..ChatCompletionsChatConfig::new("kimi")
        });
        assert_eq!(kimi_preserved["thinking"]["type"], "enabled");
        assert_eq!(kimi_preserved["thinking"]["keep"], "all");

        let kimi_disabled = request_value(ChatCompletionsChatConfig {
            wire_dialect: ChatCompletionsWireDialect::Kimi,
            thinking_mode: ChatCompletionsThinkingMode::Disabled,
            ..ChatCompletionsChatConfig::new("kimi")
        });
        assert_eq!(kimi_disabled["thinking"]["type"], "disabled");

        let glm_preserved = request_value(ChatCompletionsChatConfig {
            wire_dialect: ChatCompletionsWireDialect::Glm,
            thinking_mode: ChatCompletionsThinkingMode::Enabled,
            reasoning_history: ChatCompletionsReasoningHistory::PreserveAll,
            ..ChatCompletionsChatConfig::new("glm")
        });
        assert_eq!(glm_preserved["thinking"]["type"], "enabled");
        assert_eq!(glm_preserved["thinking"]["clear_thinking"], false);

        let qwen_preserved = request_value(ChatCompletionsChatConfig {
            wire_dialect: ChatCompletionsWireDialect::Qwen,
            thinking_mode: ChatCompletionsThinkingMode::Enabled,
            reasoning_history: ChatCompletionsReasoningHistory::PreserveAll,
            reasoning_budget_tokens: Some(4096),
            ..ChatCompletionsChatConfig::new("qwen")
        });
        assert_eq!(qwen_preserved["enable_thinking"], true);
        assert_eq!(qwen_preserved["preserve_thinking"], true);
        assert_eq!(qwen_preserved["thinking_budget"], 4096);

        let deepseek_tool_history = request_value(ChatCompletionsChatConfig {
            wire_dialect: ChatCompletionsWireDialect::Deepseek,
            thinking_mode: ChatCompletionsThinkingMode::Enabled,
            reasoning_history: ChatCompletionsReasoningHistory::ToolCallsOnly,
            ..ChatCompletionsChatConfig::new("deepseek-v4-pro")
        });
        assert_eq!(deepseek_tool_history["thinking"]["type"], "enabled");
        assert!(deepseek_tool_history.get("enable_thinking").is_none());
        assert!(deepseek_tool_history.get("preserve_thinking").is_none());
        assert!(deepseek_tool_history["thinking"].get("keep").is_none());
        assert!(deepseek_tool_history["thinking"]
            .get("clear_thinking")
            .is_none());
    }

    #[test]
    fn rejects_vendor_thinking_options_for_standard_dialect() {
        let config = ChatCompletionsChatConfig {
            thinking_mode: ChatCompletionsThinkingMode::Enabled,
            ..ChatCompletionsChatConfig::new("standard")
        };
        assert!(matches!(
            config.validate(),
            Err(ChatCompletionsConfigError::UnsupportedDialectOption(message))
                if message.contains("standard")
        ));
    }

    #[test]
    fn rejects_tool_call_only_history_for_non_deepseek_dialects() {
        for wire_dialect in [
            ChatCompletionsWireDialect::Standard,
            ChatCompletionsWireDialect::Kimi,
            ChatCompletionsWireDialect::Glm,
            ChatCompletionsWireDialect::Qwen,
        ] {
            let config = ChatCompletionsChatConfig {
                wire_dialect,
                thinking_mode: ChatCompletionsThinkingMode::Enabled,
                reasoning_history: ChatCompletionsReasoningHistory::ToolCallsOnly,
                max_output_tokens: Some(KIMI_THINKING_MIN_OUTPUT_TOKENS),
                ..ChatCompletionsChatConfig::new("model")
            };
            assert!(matches!(
                config.validate(),
                Err(ChatCompletionsConfigError::UnsupportedDialectOption(message))
                    if message.contains("deepseek")
            ));
        }
    }

    #[test]
    fn rejects_invalid_kimi_thinking_generation_limits() {
        let low_limit = ChatCompletionsChatConfig {
            wire_dialect: ChatCompletionsWireDialect::Kimi,
            max_output_tokens: Some(KIMI_THINKING_MIN_OUTPUT_TOKENS - 1),
            ..ChatCompletionsChatConfig::new("kimi")
        };
        assert!(matches!(
            low_limit.validate(),
            Err(ChatCompletionsConfigError::UnsupportedDialectOption(message))
                if message.contains("at least 16000")
        ));

        let temperature = ChatCompletionsChatConfig {
            wire_dialect: ChatCompletionsWireDialect::Kimi,
            max_output_tokens: Some(KIMI_THINKING_MIN_OUTPUT_TOKENS),
            temperature_milli: Some(500),
            ..ChatCompletionsChatConfig::new("kimi")
        };
        assert!(matches!(
            temperature.validate(),
            Err(ChatCompletionsConfigError::UnsupportedDialectOption(message))
                if message.contains("temperature")
        ));
    }

    #[test]
    fn parses_content_reasoning_usage_and_finish() {
        let input = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"hello \",\"reasoning_content\":\"thinking\"},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"world\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":4,\"total_tokens\":14,\"prompt_tokens_details\":{\"cached_tokens\":2},\"completion_tokens_details\":{\"reasoning_tokens\":1}}}\n\n",
            "data: [DONE]\n\n",
        );

        let events = parse(input).expect("parsed stream");
        assert!(events.contains(&ChatCompletionsEvent::ContentDelta("hello ".to_string())));
        assert!(events.contains(&ChatCompletionsEvent::ReasoningDelta {
            text: "thinking".to_string(),
            field: "reasoning_content".to_string()
        }));
        assert!(events.contains(&ChatCompletionsEvent::ContentDelta("world".to_string())));
        assert!(
            events.contains(&ChatCompletionsEvent::Usage(ChatTokenUsage {
                prompt_tokens: 10,
                completion_tokens: 4,
                total_tokens: 14,
                cached_prompt_tokens: 2,
                reasoning_completion_tokens: 1,
            }))
        );
        assert!(matches!(
            events.last(),
            Some(ChatCompletionsEvent::Finished {
                finish_reason: Some(reason)
            }) if reason == "stop"
        ));
    }

    #[test]
    fn accumulates_tool_call_arguments_across_chunks() {
        let input = concat!(
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"lookup\",\"arguments\":\"{\\\"query\\\":\"}}]},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"den docs\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n",
        );

        let events = parse(input).expect("parsed stream");
        assert!(events.iter().any(|event| matches!(
            event,
            ChatCompletionsEvent::ToolCallDelta {
                index: 0,
                id: Some(id),
                name: Some(name),
                arguments_delta
            } if id == "call_1" && name == "lookup" && arguments_delta == "{\"query\":"
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            ChatCompletionsEvent::ToolCallFinished(call)
                if call.index == 0
                    && call.id.as_deref() == Some("call_1")
                    && call.name == "lookup"
                    && call.arguments_json == "{\"query\":\"den docs\"}"
        )));
        assert!(!events
            .iter()
            .any(|event| matches!(event, ChatCompletionsEvent::ToolCallMalformed(_))));
        assert!(matches!(
            events.last(),
            Some(ChatCompletionsEvent::Finished {
                finish_reason: Some(reason)
            }) if reason == "tool_calls"
        ));
    }

    #[test]
    fn classifies_missing_tool_name_without_losing_length_finish() {
        let input = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"partial\",\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"arguments\":\"{\\\"query\\\":\\\"den\\\"}\"}}]},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"length\"}]}\n\n",
            "data: [DONE]\n\n",
        );

        let events = parse(input).expect("classified incomplete stream");
        assert!(events.contains(&ChatCompletionsEvent::ContentDelta("partial".to_string())));
        assert!(events.iter().any(|event| matches!(
            event,
            ChatCompletionsEvent::ToolCallMalformed(call)
                if call.index == 0
                    && call.id.as_deref() == Some("call_1")
                    && call.name.is_none()
                    && call.diagnostics == ["function.name is missing or empty"]
        )));
        assert!(matches!(
            events.last(),
            Some(ChatCompletionsEvent::Finished {
                finish_reason: Some(reason)
            }) if reason == "length"
        ));
    }

    #[test]
    fn classifies_malformed_tool_arguments_before_non_length_finish() {
        let input = concat!(
            "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"checking\",\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"lookup\",\"arguments\":\"{\\\"query\\\":\"}}]},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n",
        );

        let events = parse(input).expect("classified malformed stream");
        assert!(events.iter().any(|event| matches!(
            event,
            ChatCompletionsEvent::ToolCallMalformed(call)
                if call.name.as_deref() == Some("lookup")
                    && call.diagnostics.iter().any(|diagnostic|
                        diagnostic.starts_with("function.arguments is invalid JSON:"))
        )));
        assert!(matches!(
            events.last(),
            Some(ChatCompletionsEvent::Finished {
                finish_reason: Some(reason)
            }) if reason == "tool_calls"
        ));
    }

    #[test]
    fn missing_or_non_string_tool_arguments_remain_malformed() {
        for (label, arguments, expected_diagnostic) in [
            ("missing", "", "function.arguments is missing"),
            (
                "null",
                ",\"arguments\":null",
                "function.arguments must be a string, found null",
            ),
            (
                "object",
                ",\"arguments\":{\"query\":\"den\"}",
                "function.arguments must be a string, found object",
            ),
        ] {
            let input = format!(
                "data: {{\"choices\":[{{\"delta\":{{\"tool_calls\":[{{\"index\":0,\"id\":\"call_{label}\",\"function\":{{\"name\":\"lookup\"{arguments}}}}}]}},\"finish_reason\":\"length\"}}]}}\n\ndata: [DONE]\n\n"
            );
            let events = parse(&input).expect("classify missing or wrong-type arguments");
            assert!(events.iter().any(|event| matches!(
                event,
                ChatCompletionsEvent::ToolCallMalformed(call)
                    if call.name.as_deref() == Some("lookup")
                        && call.diagnostics.iter().any(|diagnostic| diagnostic == expected_diagnostic)
            )));
            assert!(!events
                .iter()
                .any(|event| matches!(event, ChatCompletionsEvent::ToolCallFinished(_))));
        }
    }

    #[test]
    fn done_flushes_pending_tool_fragments_instead_of_hiding_them() {
        let input = concat!(
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_partial\",\"function\":{\"name\":\"lookup\",\"arguments\":\"{\\\"query\\\":\"}}]},\"finish_reason\":null}]}\n\n",
            "data: [DONE]\n\n",
        );

        let events = parse(input).expect("classify pending call at done");
        assert!(events.iter().any(|event| matches!(
            event,
            ChatCompletionsEvent::ToolCallMalformed(call)
                if call.name.as_deref() == Some("lookup")
                    && call.diagnostics.iter().any(|diagnostic|
                        diagnostic.starts_with("function.arguments is invalid JSON:"))
        )));
        assert!(matches!(
            events.last(),
            Some(ChatCompletionsEvent::Finished {
                finish_reason: None
            })
        ));
    }

    #[test]
    fn malformed_tool_call_index_is_recoverable_provider_output() {
        let input = concat!(
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"id\":\"call_no_index\",\"function\":{\"name\":\"lookup\",\"arguments\":\"{}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n",
        );

        let events = parse(input).expect("retain malformed index as an event");
        assert!(events.iter().any(|event| matches!(
            event,
            ChatCompletionsEvent::ToolCallMalformed(call)
                if call.id.as_deref() == Some("call_no_index")
                    && call.name.as_deref() == Some("lookup")
                    && call.diagnostics == ["tool call index is missing"]
        )));
        assert!(!events
            .iter()
            .any(|event| matches!(event, ChatCompletionsEvent::ToolCallFinished(_))));
    }

    #[test]
    fn ignores_empty_deltas_but_accepts_finish() {
        let input = concat!(
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"\"},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n",
        );

        let events = parse(input).expect("parsed stream");
        assert_eq!(
            events,
            vec![ChatCompletionsEvent::Finished {
                finish_reason: Some("stop".to_string())
            }]
        );
    }

    #[test]
    fn reports_malformed_sse_json() {
        let error = parse("data: {nope}\n\n").expect_err("malformed json");
        assert!(matches!(
            error,
            ChatCompletionsStreamError::Transport(message)
                if message.contains("invalid SSE JSON")
        ));
    }

    #[test]
    fn reports_provider_error_payload() {
        let error = parse("data: {\"error\":{\"message\":\"model overloaded\"}}\n\n")
            .expect_err("provider error");
        assert_eq!(
            error,
            ChatCompletionsStreamError::ProviderError("model overloaded".to_string())
        );
    }

    #[test]
    fn detects_stream_closed_before_terminal() {
        let error = parse(
            "data: {\"choices\":[{\"delta\":{\"content\":\"hi\"},\"finish_reason\":null}]}\n\n",
        )
        .expect_err("closed early");
        assert_eq!(error, ChatCompletionsStreamError::ClosedBeforeFinish);
    }

    #[test]
    fn parses_sse_lines_split_across_reads() {
        struct OneByteReader {
            bytes: Vec<u8>,
            index: usize,
        }

        impl Read for OneByteReader {
            fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
                if self.index >= self.bytes.len() {
                    return Ok(0);
                }
                output[0] = self.bytes[self.index];
                self.index += 1;
                Ok(1)
            }
        }

        let mut reader = OneByteReader {
            bytes: b"data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n".to_vec(),
            index: 0,
        };
        let events = parse_sse_reader(&mut reader, &mut |_| {}).expect("chunked parse");
        assert_eq!(
            events,
            vec![
                ChatCompletionsEvent::ContentDelta("ok".to_string()),
                ChatCompletionsEvent::Finished {
                    finish_reason: Some("stop".to_string())
                }
            ]
        );
    }

    #[derive(Debug, Clone)]
    struct FakeDenRouterSource {
        base_url: String,
        models: Result<Vec<DenRouterModelInfo>, DenRouterSelectionError>,
        routes: Result<Option<DenRouterRoutes>, DenRouterSelectionError>,
    }

    impl FakeDenRouterSource {
        fn new(models: Vec<DenRouterModelInfo>, routes: Option<DenRouterRoutes>) -> Self {
            Self {
                base_url: "http://router.local:18082/v1/".to_string(),
                models: Ok(models),
                routes: Ok(routes),
            }
        }
    }

    impl DenRouterModelSource for FakeDenRouterSource {
        fn base_url(&self) -> &str {
            &self.base_url
        }

        fn fetch_models(&self) -> Result<Vec<DenRouterModelInfo>, DenRouterSelectionError> {
            self.models.clone()
        }

        fn fetch_routes(&self) -> Result<Option<DenRouterRoutes>, DenRouterSelectionError> {
            self.routes.clone()
        }
    }

    fn model(id: &str) -> DenRouterModelInfo {
        DenRouterModelInfo {
            id: id.to_string(),
            context_length: None,
        }
    }

    fn codex_routes(model_id: &str) -> DenRouterRoutes {
        let mut routes = DenRouterRoutes::default();
        routes.models.insert(
            model_id.to_string(),
            DenRouterRouteModel {
                backends: vec![DenRouterRouteBackend {
                    backend_type: Some("codex-oauth".to_string()),
                    healthy: Some(true),
                    drained: Some(false),
                }],
            },
        );
        routes
    }

    #[test]
    fn den_router_selection_uses_candidate_order_and_normalized_v1_base_url() {
        let source = FakeDenRouterSource::new(
            vec![model("zzz"), model("grok"), model("deepseek-flash")],
            None,
        );
        let selection = resolve_den_router_model(&source, &DenRouterSelectionOptions::default())
            .expect("selection");

        assert_eq!(selection.model_id, "deepseek-flash");
        assert_eq!(selection.api, DenRouterApi::OpenaiCompletions);
        assert_eq!(selection.base_url, "http://router.local:18082/v1");
        assert!(!selection.reasoning);
        assert_eq!(selection.context_window_tokens, 128_000);
        assert_eq!(selection.max_tokens, 128);
    }

    #[test]
    fn den_router_selection_honors_requested_model_and_context() {
        let source = FakeDenRouterSource::new(
            vec![DenRouterModelInfo {
                id: "custom".to_string(),
                context_length: Some(64_000),
            }],
            None,
        );
        let selection = resolve_den_router_model(
            &source,
            &DenRouterSelectionOptions {
                requested_model_id: Some("custom".to_string()),
                requested_api: Some(DenRouterApi::OpenaiCompletions),
                max_tokens: Some(512),
            },
        )
        .expect("selection");

        assert_eq!(selection.model_id, "custom");
        assert_eq!(selection.context_window_tokens, 64_000);
        assert_eq!(selection.max_tokens, 512);
    }

    #[test]
    fn den_router_selection_rejects_missing_requested_model_and_no_models() {
        let source = FakeDenRouterSource::new(vec![model("deepseek-flash")], None);
        let error = resolve_den_router_model(
            &source,
            &DenRouterSelectionOptions {
                requested_model_id: Some("missing".to_string()),
                ..DenRouterSelectionOptions::default()
            },
        )
        .expect_err("missing model");
        assert_eq!(
            error,
            DenRouterSelectionError::RequestedModelUnavailable("missing".to_string())
        );

        let source = FakeDenRouterSource::new(Vec::new(), None);
        assert_eq!(
            resolve_den_router_model(&source, &DenRouterSelectionOptions::default())
                .expect_err("no models"),
            DenRouterSelectionError::NoModels
        );
    }

    #[test]
    fn den_router_selection_detects_codex_backed_responses_and_allows_explicit_override() {
        let source = FakeDenRouterSource::new(vec![model("gpt")], Some(codex_routes("gpt")));
        let selection = resolve_den_router_model(&source, &DenRouterSelectionOptions::default())
            .expect("selection");
        assert_eq!(selection.api, DenRouterApi::OpenaiResponses);
        assert!(selection.reasoning);

        let selection = resolve_den_router_model(
            &source,
            &DenRouterSelectionOptions {
                requested_api: Some(DenRouterApi::OpenaiCompletions),
                ..DenRouterSelectionOptions::default()
            },
        )
        .expect("selection");
        assert_eq!(selection.api, DenRouterApi::OpenaiCompletions);
        assert!(!selection.reasoning);
    }

    #[test]
    fn den_router_selection_ignores_route_probe_failure_like_ts_factory() {
        let source = FakeDenRouterSource {
            base_url: DEFAULT_DEN_ROUTER_URL.to_string(),
            models: Ok(vec![model("gpt")]),
            routes: Err(DenRouterSelectionError::MalformedResponse {
                path: "/routes",
                message: "bad json".to_string(),
            }),
        };
        let selection = resolve_den_router_model(&source, &DenRouterSelectionOptions::default())
            .expect("selection");
        assert_eq!(selection.api, DenRouterApi::OpenaiCompletions);
    }

    #[test]
    fn den_router_api_parse_rejects_unsupported_values() {
        assert_eq!(
            DenRouterApi::parse("openai-responses").expect("responses"),
            DenRouterApi::OpenaiResponses
        );
        assert_eq!(
            DenRouterApi::parse("anthropic").expect_err("unsupported"),
            DenRouterSelectionError::UnsupportedApi("anthropic".to_string())
        );
    }

    #[test]
    fn den_router_model_response_parser_rejects_malformed_or_empty_payloads() {
        let error = den_router_models_from_value(json!({"data": {"id": "bad"}}))
            .expect_err("malformed data");
        assert!(matches!(
            error,
            DenRouterSelectionError::MalformedResponse {
                path: "/v1/models",
                ..
            }
        ));
        assert_eq!(
            den_router_models_from_value(json!({"data": []})).expect_err("empty"),
            DenRouterSelectionError::NoModels
        );
    }

    #[test]
    fn normalize_den_router_base_url_strips_one_v1_suffix_and_slash() {
        assert_eq!(
            normalize_den_router_base_url("http://127.0.0.1:18082/v1/"),
            "http://127.0.0.1:18082"
        );
        assert_eq!(
            normalize_den_router_base_url("http://127.0.0.1:18082/"),
            "http://127.0.0.1:18082"
        );
    }

    fn context() -> BrainEventContext {
        BrainEventContext::new("wake-1", SessionId::new("session-1"))
    }

    fn events(items: &[BrainWakeStreamItem]) -> Vec<BrainEvent> {
        items
            .iter()
            .filter_map(|item| match item {
                BrainWakeStreamItem::Event { event } => Some(event.event.clone()),
                _ => None,
            })
            .collect()
    }

    fn terminal_kind(items: &[BrainWakeStreamItem]) -> &'static str {
        match items.last() {
            Some(BrainWakeStreamItem::Actions { .. }) => "actions",
            Some(BrainWakeStreamItem::WakeFailed { .. }) => "wake_failed",
            _ => "none",
        }
    }

    fn tool_call(name: &str, args: &str) -> PendingChatFunctionCall {
        PendingChatFunctionCall {
            index: 0,
            id: Some("call_1".to_string()),
            name: name.to_string(),
            arguments_json: args.to_string(),
        }
    }

    #[derive(Debug, Default)]
    struct ScriptedToolExecutor {
        outputs: std::sync::Mutex<VecDeque<ChatCompletionsToolOutput>>,
    }

    impl ScriptedToolExecutor {
        fn new(outputs: impl IntoIterator<Item = ChatCompletionsToolOutput>) -> Self {
            Self {
                outputs: std::sync::Mutex::new(outputs.into_iter().collect()),
            }
        }
    }

    impl ChatCompletionsNeutralToolExecutor for ScriptedToolExecutor {
        fn execute(&self, _call: &PendingChatFunctionCall) -> ChatCompletionsToolOutput {
            self.outputs
                .lock()
                .expect("tool script mutex")
                .pop_front()
                .unwrap_or_else(|| ChatCompletionsToolOutput::ok("default tool output"))
        }
    }

    fn loop_with(
        scripts: Vec<Result<Vec<ChatCompletionsEvent>, ChatCompletionsStreamError>>,
        outputs: Vec<ChatCompletionsToolOutput>,
    ) -> ChatCompletionsBrainLoop<FakeChatCompletionsClient, ScriptedToolExecutor> {
        ChatCompletionsBrainLoop::new(
            FakeChatCompletionsClient::new(scripts),
            ScriptedToolExecutor::new(outputs),
            ChatCompletionsChatConfig::new("deepseek-flash"),
            vec![NeutralBrainTool {
                name: "lookup".to_string(),
                description: "Look up".to_string(),
                input_schema: json!({"type": "object"}),
            }],
        )
    }

    fn loop_with_config(
        scripts: Vec<Result<Vec<ChatCompletionsEvent>, ChatCompletionsStreamError>>,
        outputs: Vec<ChatCompletionsToolOutput>,
        config: ChatCompletionsChatConfig,
    ) -> ChatCompletionsBrainLoop<FakeChatCompletionsClient, ScriptedToolExecutor> {
        ChatCompletionsBrainLoop::new(
            FakeChatCompletionsClient::new(scripts),
            ScriptedToolExecutor::new(outputs),
            config,
            vec![NeutralBrainTool {
                name: "lookup".to_string(),
                description: "Look up".to_string(),
                input_schema: json!({"type": "object"}),
            }],
        )
    }

    #[test]
    fn stream_sink_receives_started_deltas_and_tool_lifecycle_before_wake_returns() {
        #[derive(Debug)]
        struct SinkAwareToolExecutor {
            streamed: std::sync::Arc<std::sync::Mutex<Vec<BrainWakeStreamItem>>>,
        }

        impl ChatCompletionsNeutralToolExecutor for SinkAwareToolExecutor {
            fn execute(&self, _call: &PendingChatFunctionCall) -> ChatCompletionsToolOutput {
                let streamed = self.streamed.lock().expect("stream sink mutex");
                let observed = events(&streamed);
                assert!(observed.contains(&BrainEvent::Started));
                assert!(observed.contains(&BrainEvent::ReasoningDelta {
                    text: "planning".to_string(),
                    format: Some(CANONICAL_REASONING_FORMAT.to_string()),
                }));
                assert!(observed.iter().any(|event| matches!(
                    event,
                    BrainEvent::ToolCallStarted { tool_name, .. } if tool_name == "lookup"
                )));
                ChatCompletionsToolOutput::ok("tool output")
            }
        }

        let streamed = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut brain = ChatCompletionsBrainLoop::new(
            FakeChatCompletionsClient::new([
                Ok(vec![
                    ChatCompletionsEvent::ReasoningDelta {
                        text: "planning".to_string(),
                        field: "reasoning_content".to_string(),
                    },
                    ChatCompletionsEvent::ToolCallFinished(tool_call("lookup", "{}")),
                    ChatCompletionsEvent::Finished {
                        finish_reason: Some("tool_calls".to_string()),
                    },
                ]),
                Ok(vec![
                    ChatCompletionsEvent::ContentDelta("done".to_string()),
                    ChatCompletionsEvent::Finished {
                        finish_reason: Some("stop".to_string()),
                    },
                ]),
            ]),
            SinkAwareToolExecutor {
                streamed: std::sync::Arc::clone(&streamed),
            },
            ChatCompletionsChatConfig::new("test-model"),
            vec![NeutralBrainTool {
                name: "lookup".to_string(),
                description: "Look up".to_string(),
                input_schema: json!({"type": "object"}),
            }],
        );
        let sink_items = std::sync::Arc::clone(&streamed);
        let mut sink = move |item| sink_items.lock().expect("stream sink mutex").push(item);

        let output = brain.wake_with_stream_sink(
            ChatCompletionsBrainLoopInput {
                context: context(),
                messages: vec![ChatCompletionMessage::user("use a tool")],
                input_images: Vec::new(),
                provider_state: None,
                continuation_state: None,
                final_message_fallback: None,
            },
            &mut sink,
        );

        assert_eq!(
            streamed.lock().expect("stream sink mutex").as_slice(),
            output.stream.as_slice()
        );
        assert_eq!(terminal_kind(&output.stream), "actions");
    }

    fn kimi_preserved_config() -> ChatCompletionsChatConfig {
        ChatCompletionsChatConfig {
            wire_dialect: ChatCompletionsWireDialect::Kimi,
            thinking_mode: ChatCompletionsThinkingMode::Enabled,
            reasoning_history: ChatCompletionsReasoningHistory::PreserveAll,
            max_output_tokens: Some(KIMI_THINKING_MIN_OUTPUT_TOKENS),
            ..ChatCompletionsChatConfig::new("kimi-k2.7")
        }
    }

    fn deepseek_tool_history_config() -> ChatCompletionsChatConfig {
        ChatCompletionsChatConfig {
            wire_dialect: ChatCompletionsWireDialect::Deepseek,
            thinking_mode: ChatCompletionsThinkingMode::Enabled,
            reasoning_history: ChatCompletionsReasoningHistory::ToolCallsOnly,
            ..ChatCompletionsChatConfig::new("deepseek-v4-pro")
        }
    }

    fn provider_state_input(state: BrainWakeProviderStateUpdate) -> BrainWakeProviderStateInput {
        BrainWakeProviderStateInput {
            module_id: state.module_id,
            strategy_id: state.strategy_id,
            profile_fingerprint: state.profile_fingerprint,
            provider_fingerprint: state.provider_fingerprint,
            payload_version: state.payload_version,
            payload: state.payload,
            expires_at: None,
        }
    }

    #[test]
    fn tool_continuation_replays_exact_reasoning_content() {
        let exact_reasoning = "line one\nline two with spacing  ";
        let mut brain = loop_with_config(
            vec![
                Ok(vec![
                    ChatCompletionsEvent::ReasoningDelta {
                        text: exact_reasoning.to_string(),
                        field: "reasoning_content".to_string(),
                    },
                    ChatCompletionsEvent::ToolCallFinished(tool_call(
                        "lookup",
                        r#"{"query":"one"}"#,
                    )),
                    ChatCompletionsEvent::Finished {
                        finish_reason: Some("tool_calls".to_string()),
                    },
                ]),
                Ok(vec![
                    ChatCompletionsEvent::ContentDelta("done".to_string()),
                    ChatCompletionsEvent::Finished {
                        finish_reason: Some("stop".to_string()),
                    },
                ]),
            ],
            vec![ChatCompletionsToolOutput::ok("result")],
            kimi_preserved_config(),
        );

        let output =
            brain.wake_with_messages(context(), vec![ChatCompletionMessage::user("start")]);
        let second_request = &output.provider_request_debug_samples[1];
        let assistant = second_request["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .find(|message| message["role"] == "assistant")
            .expect("assistant tool message");
        assert_eq!(assistant["reasoning_content"], exact_reasoning);
        assert_eq!(assistant["tool_calls"][0]["function"]["name"], "lookup");
        assert!(
            events(&output.stream).contains(&BrainEvent::ReasoningDelta {
                text: exact_reasoning.to_string(),
                format: Some(CANONICAL_REASONING_FORMAT.to_string()),
            })
        );
        assert!(!events(&output.stream).iter().any(
            |event| matches!(event, BrainEvent::TextDelta { text } if text.contains("line one"))
        ));
    }

    #[test]
    fn multiple_tool_rounds_preserve_reasoning_order() {
        let mut brain = loop_with_config(
            vec![
                Ok(vec![
                    ChatCompletionsEvent::ReasoningDelta {
                        text: "reason-one".to_string(),
                        field: "reasoning_content".to_string(),
                    },
                    ChatCompletionsEvent::ToolCallFinished(tool_call("lookup", r#"{"round":1}"#)),
                    ChatCompletionsEvent::Finished {
                        finish_reason: Some("tool_calls".to_string()),
                    },
                ]),
                Ok(vec![
                    ChatCompletionsEvent::ReasoningDelta {
                        text: "reason-two".to_string(),
                        field: "reasoning_content".to_string(),
                    },
                    ChatCompletionsEvent::ToolCallFinished(tool_call("lookup", r#"{"round":2}"#)),
                    ChatCompletionsEvent::Finished {
                        finish_reason: Some("tool_calls".to_string()),
                    },
                ]),
                Ok(vec![
                    ChatCompletionsEvent::ContentDelta("done".to_string()),
                    ChatCompletionsEvent::Finished {
                        finish_reason: Some("stop".to_string()),
                    },
                ]),
            ],
            vec![
                ChatCompletionsToolOutput::ok("one"),
                ChatCompletionsToolOutput::ok("two"),
            ],
            kimi_preserved_config(),
        );

        let output =
            brain.wake_with_messages(context(), vec![ChatCompletionMessage::user("start")]);
        let reasoning = output.provider_request_debug_samples[2]["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .filter_map(|message| message["reasoning_content"].as_str())
            .collect::<Vec<_>>();
        assert_eq!(reasoning, vec!["reason-one", "reason-two"]);
    }

    #[test]
    fn preserved_reasoning_state_survives_serialization_and_next_wake() {
        let mut first_brain = loop_with_config(
            vec![Ok(vec![
                ChatCompletionsEvent::ReasoningDelta {
                    text: "prior-reasoning".to_string(),
                    field: "reasoning_content".to_string(),
                },
                ChatCompletionsEvent::ContentDelta("prior answer".to_string()),
                ChatCompletionsEvent::Finished {
                    finish_reason: Some("stop".to_string()),
                },
            ])],
            Vec::new(),
            kimi_preserved_config(),
        );
        let first = first_brain.wake_with_messages(
            context(),
            vec![
                ChatCompletionMessage::user("bootstrap role message"),
                ChatCompletionMessage::user("first question"),
            ],
        );
        let state_json = serde_json::to_string(&first.provider_state).expect("serialize state");
        let restored_state: Option<BrainWakeProviderStateOutput> =
            serde_json::from_str(&state_json).expect("restore state");
        let Some(BrainWakeProviderStateOutput::Replace { state }) = restored_state else {
            panic!("expected replacement state");
        };
        let provider_state = BrainWakeProviderStateInput {
            module_id: state.module_id,
            strategy_id: state.strategy_id,
            profile_fingerprint: state.profile_fingerprint,
            provider_fingerprint: state.provider_fingerprint,
            payload_version: state.payload_version,
            payload: state.payload,
            expires_at: None,
        };
        let mut second_brain = loop_with_config(
            vec![Ok(vec![
                ChatCompletionsEvent::ContentDelta("next answer".to_string()),
                ChatCompletionsEvent::Finished {
                    finish_reason: Some("stop".to_string()),
                },
            ])],
            Vec::new(),
            kimi_preserved_config(),
        );
        let second = second_brain.wake(ChatCompletionsBrainLoopInput {
            context: BrainEventContext::new("wake-2", SessionId::new("session-1")),
            messages: vec![
                ChatCompletionMessage::system("system"),
                ChatCompletionMessage::user("second question"),
            ],
            input_images: Vec::new(),
            provider_state: Some(provider_state),
            continuation_state: None,
            final_message_fallback: None,
        });
        let messages = second.provider_request_debug_samples[0]["messages"]
            .as_array()
            .expect("messages");
        assert_eq!(messages[0]["role"], "system");
        assert!(messages.iter().any(|message| {
            message["role"] == "assistant"
                && message["reasoning_content"] == "prior-reasoning"
                && message["content"] == "prior answer"
        }));
        assert_eq!(
            messages
                .iter()
                .filter(|message| message["content"] == "bootstrap role message")
                .count(),
            1,
            "role bootstrap history must appear exactly once after hydration"
        );
        assert_eq!(
            messages.last().expect("new user")["content"],
            "second question"
        );
    }

    #[test]
    fn deepseek_tool_call_reasoning_survives_restart_and_non_tool_reasoning_does_not() {
        let mut first_brain = loop_with_config(
            vec![
                Ok(vec![
                    ChatCompletionsEvent::ReasoningDelta {
                        text: "tool-reasoning-one".to_string(),
                        field: "reasoning_content".to_string(),
                    },
                    ChatCompletionsEvent::ToolCallFinished(tool_call("lookup", r#"{"round":1}"#)),
                    ChatCompletionsEvent::Finished {
                        finish_reason: Some("tool_calls".to_string()),
                    },
                ]),
                Ok(vec![
                    ChatCompletionsEvent::ReasoningDelta {
                        text: "tool-reasoning-two".to_string(),
                        field: "reasoning_content".to_string(),
                    },
                    ChatCompletionsEvent::ToolCallFinished(tool_call("lookup", r#"{"round":2}"#)),
                    ChatCompletionsEvent::Finished {
                        finish_reason: Some("tool_calls".to_string()),
                    },
                ]),
                Ok(vec![
                    ChatCompletionsEvent::ReasoningDelta {
                        text: "final-answer-reasoning".to_string(),
                        field: "reasoning_content".to_string(),
                    },
                    ChatCompletionsEvent::ContentDelta("first answer".to_string()),
                    ChatCompletionsEvent::Finished {
                        finish_reason: Some("stop".to_string()),
                    },
                ]),
            ],
            vec![
                ChatCompletionsToolOutput::ok("result-one"),
                ChatCompletionsToolOutput::ok("result-two"),
            ],
            deepseek_tool_history_config(),
        );
        let first = first_brain.wake_with_messages(
            context(),
            vec![ChatCompletionMessage::user("first question")],
        );
        assert!(first.completed);
        assert_eq!(first.provider_request_count, 3);
        let third_request_messages = first.provider_request_debug_samples[2]["messages"]
            .as_array()
            .expect("third request messages");
        assert_eq!(
            third_request_messages
                .iter()
                .filter_map(|message| message["reasoning_content"].as_str())
                .collect::<Vec<_>>(),
            vec!["tool-reasoning-one", "tool-reasoning-two"]
        );

        let serialized = serde_json::to_string(&first.provider_state).expect("serialize state");
        let restored: Option<BrainWakeProviderStateOutput> =
            serde_json::from_str(&serialized).expect("restore state");
        let Some(BrainWakeProviderStateOutput::Replace { state }) = restored else {
            panic!("expected replacement state");
        };
        let persisted: ChatCompletionsProviderStateV1 =
            serde_json::from_value(state.payload.clone()).expect("persisted history");
        assert_eq!(
            persisted
                .messages
                .iter()
                .filter_map(|message| message.reasoning_content.as_deref())
                .collect::<Vec<_>>(),
            vec!["tool-reasoning-one", "tool-reasoning-two"]
        );
        assert!(persisted.messages.iter().any(|message| {
            message.content.as_deref() == Some("first answer")
                && message.reasoning_content.is_none()
                && message.tool_calls.is_empty()
        }));

        let mut second_brain = loop_with_config(
            vec![Ok(vec![
                ChatCompletionsEvent::ContentDelta("second answer".to_string()),
                ChatCompletionsEvent::Finished {
                    finish_reason: Some("stop".to_string()),
                },
            ])],
            Vec::new(),
            deepseek_tool_history_config(),
        );
        let second = second_brain.wake(ChatCompletionsBrainLoopInput {
            context: BrainEventContext::new("wake-2", SessionId::new("session-1")),
            messages: vec![ChatCompletionMessage::user("second question")],
            input_images: Vec::new(),
            provider_state: Some(provider_state_input(state)),
            continuation_state: None,
            final_message_fallback: None,
        });
        let second_request = &second.provider_request_debug_samples[0];
        let second_messages = second_request["messages"].as_array().expect("messages");
        assert_eq!(second_request["thinking"]["type"], "enabled");
        assert_eq!(
            second_messages
                .iter()
                .filter_map(|message| message["reasoning_content"].as_str())
                .collect::<Vec<_>>(),
            vec!["tool-reasoning-one", "tool-reasoning-two"]
        );
        assert_eq!(
            second_messages
                .iter()
                .filter_map(|message| message["content"].as_str())
                .collect::<Vec<_>>(),
            vec![
                "first question",
                "result-one",
                "result-two",
                "first answer",
                "second question"
            ]
        );
    }

    #[test]
    fn provider_default_preserves_visible_history_without_reasoning_or_vendor_controls() {
        let config = ChatCompletionsChatConfig::new("standard-model");
        let mut first_brain = loop_with_config(
            vec![Ok(vec![
                ChatCompletionsEvent::ReasoningDelta {
                    text: "first private reasoning".to_string(),
                    field: "reasoning_content".to_string(),
                },
                ChatCompletionsEvent::ContentDelta("first answer".to_string()),
                ChatCompletionsEvent::Finished {
                    finish_reason: Some("stop".to_string()),
                },
            ])],
            Vec::new(),
            config.clone(),
        );
        let first = first_brain.wake_with_messages(
            context(),
            vec![ChatCompletionMessage::user("first question")],
        );
        let Some(BrainWakeProviderStateOutput::Replace { state }) = first.provider_state else {
            panic!("provider default must persist ordinary conversation history");
        };
        let mut second_brain = loop_with_config(
            vec![Ok(vec![
                ChatCompletionsEvent::ContentDelta("second answer".to_string()),
                ChatCompletionsEvent::Finished {
                    finish_reason: Some("stop".to_string()),
                },
            ])],
            Vec::new(),
            config,
        );
        let second = second_brain.wake(ChatCompletionsBrainLoopInput {
            context: BrainEventContext::new("wake-2", SessionId::new("session-1")),
            messages: vec![ChatCompletionMessage::user("second question")],
            input_images: Vec::new(),
            provider_state: Some(BrainWakeProviderStateInput {
                module_id: state.module_id,
                strategy_id: state.strategy_id,
                profile_fingerprint: state.profile_fingerprint,
                provider_fingerprint: state.provider_fingerprint,
                payload_version: state.payload_version,
                payload: state.payload,
                expires_at: None,
            }),
            continuation_state: None,
            final_message_fallback: None,
        });
        let request = &second.provider_request_debug_samples[0];
        let messages = request["messages"].as_array().expect("messages");
        assert_eq!(
            messages
                .iter()
                .filter_map(|message| message["content"].as_str())
                .collect::<Vec<_>>(),
            vec!["first question", "first answer", "second question"]
        );
        assert!(messages
            .iter()
            .all(|message| message.get("reasoning_content").is_none()));
        assert!(request.get("thinking").is_none());
        assert!(request.get("preserve_thinking").is_none());
        assert!(request.get("enable_thinking").is_none());
    }

    #[test]
    fn discard_policy_preserves_visible_history_without_replaying_reasoning() {
        let preserved = kimi_preserved_config();
        let state_payload = ChatCompletionsProviderStateV1 {
            kind: MODULE_ID.to_string(),
            payload_version: PROVIDER_STATE_PAYLOAD_VERSION.to_string(),
            strategy_id: preserved.provider_state_strategy_id.clone(),
            messages: vec![
                ChatCompletionMessage::user("prior question"),
                ChatCompletionMessage {
                    role: ChatMessageRole::Assistant,
                    content: Some("checking".to_string()),
                    reasoning_content: Some("prior tool reasoning".to_string()),
                    name: None,
                    tool_call_id: None,
                    tool_calls: vec![ChatAssistantToolCall {
                        id: "call_prior".to_string(),
                        kind: "function".to_string(),
                        function: ChatFunctionCall {
                            name: "lookup".to_string(),
                            arguments: r#"{"query":"prior"}"#.to_string(),
                        },
                    }],
                },
                ChatCompletionMessage::tool("call_prior", "prior tool result"),
                ChatCompletionMessage {
                    role: ChatMessageRole::Assistant,
                    content: Some("prior answer".to_string()),
                    reasoning_content: Some("prior final reasoning".to_string()),
                    name: None,
                    tool_call_id: None,
                    tool_calls: Vec::new(),
                },
            ],
        };
        let prior_state = BrainWakeProviderStateInput {
            module_id: MODULE_ID.to_string(),
            strategy_id: preserved.provider_state_strategy_id,
            profile_fingerprint: "profile".to_string(),
            provider_fingerprint: "provider".to_string(),
            payload_version: PROVIDER_STATE_PAYLOAD_VERSION.to_string(),
            payload: serde_json::to_value(state_payload).expect("state payload"),
            expires_at: None,
        };
        let mut brain = loop_with_config(
            vec![Ok(vec![
                ChatCompletionsEvent::ContentDelta("fresh answer".to_string()),
                ChatCompletionsEvent::Finished {
                    finish_reason: Some("stop".to_string()),
                },
            ])],
            Vec::new(),
            ChatCompletionsChatConfig {
                wire_dialect: ChatCompletionsWireDialect::Kimi,
                thinking_mode: ChatCompletionsThinkingMode::Enabled,
                reasoning_history: ChatCompletionsReasoningHistory::Discard,
                max_output_tokens: Some(KIMI_THINKING_MIN_OUTPUT_TOKENS),
                ..ChatCompletionsChatConfig::new("kimi-k2.6")
            },
        );

        let output = brain.wake(ChatCompletionsBrainLoopInput {
            context: BrainEventContext::new("wake-2", SessionId::new("session-1")),
            messages: vec![ChatCompletionMessage::user("new question")],
            input_images: Vec::new(),
            provider_state: Some(prior_state),
            continuation_state: None,
            final_message_fallback: None,
        });

        let request = &output.provider_request_debug_samples[0];
        let messages = request["messages"].as_array().expect("messages");
        assert!(!messages
            .iter()
            .any(|message| message.get("reasoning_content").is_some()));
        assert_eq!(
            messages
                .iter()
                .filter_map(|message| message["content"].as_str())
                .collect::<Vec<_>>(),
            vec![
                "prior question",
                "checking",
                "prior tool result",
                "prior answer",
                "new question"
            ]
        );
        assert_eq!(messages[1]["tool_calls"][0]["id"], "call_prior");
        assert_eq!(messages[2]["tool_call_id"], "call_prior");
        assert_eq!(request["thinking"]["keep"], Value::Null);
        let Some(BrainWakeProviderStateOutput::Replace { state }) = output.provider_state else {
            panic!("discard must replace provider state with sanitized ordinary history");
        };
        let persisted: ChatCompletionsProviderStateV1 =
            serde_json::from_value(state.payload.clone()).expect("persisted discard history");
        assert!(persisted
            .messages
            .iter()
            .all(|message| message.reasoning_content.is_none()));

        let mut next_brain = loop_with_config(
            vec![Ok(vec![
                ChatCompletionsEvent::ContentDelta("next answer".to_string()),
                ChatCompletionsEvent::Finished {
                    finish_reason: Some("stop".to_string()),
                },
            ])],
            Vec::new(),
            ChatCompletionsChatConfig {
                wire_dialect: ChatCompletionsWireDialect::Kimi,
                thinking_mode: ChatCompletionsThinkingMode::Enabled,
                reasoning_history: ChatCompletionsReasoningHistory::Discard,
                max_output_tokens: Some(KIMI_THINKING_MIN_OUTPUT_TOKENS),
                ..ChatCompletionsChatConfig::new("kimi-k2.6")
            },
        );
        let next = next_brain.wake(ChatCompletionsBrainLoopInput {
            context: BrainEventContext::new("wake-3", SessionId::new("session-1")),
            messages: vec![ChatCompletionMessage::user("third question")],
            input_images: Vec::new(),
            provider_state: Some(BrainWakeProviderStateInput {
                module_id: state.module_id,
                strategy_id: state.strategy_id,
                profile_fingerprint: state.profile_fingerprint,
                provider_fingerprint: state.provider_fingerprint,
                payload_version: state.payload_version,
                payload: state.payload,
                expires_at: None,
            }),
            continuation_state: None,
            final_message_fallback: None,
        });
        let next_messages = next.provider_request_debug_samples[0]["messages"]
            .as_array()
            .expect("next messages");
        assert!(next_messages
            .iter()
            .any(|message| message["content"] == "prior answer"));
        assert!(next_messages
            .iter()
            .any(|message| message["content"] == "fresh answer"));
        assert!(next_messages
            .iter()
            .all(|message| message.get("reasoning_content").is_none()));
    }

    #[test]
    fn minimal_loop_completes_no_tool_turn_with_actions_terminal() {
        let context = context();
        let mut brain = loop_with(
            vec![Ok(vec![
                ChatCompletionsEvent::ContentDelta("hello".to_string()),
                ChatCompletionsEvent::Finished {
                    finish_reason: Some("stop".to_string()),
                },
            ])],
            Vec::new(),
        );

        let output = brain.wake_with_messages(context, vec![ChatCompletionMessage::user("hi")]);

        assert!(output.completed);
        assert_eq!(output.provider_request_count, 1);
        assert_eq!(terminal_kind(&output.stream), "actions");
        assert!(events(&output.stream).contains(&BrainEvent::TextDelta {
            text: "hello".to_string()
        }));
        assert!(events(&output.stream).contains(&BrainEvent::Finished));
    }

    #[test]
    fn minimal_loop_fails_reasoning_only_output_limit_without_false_completion() {
        let mut brain = loop_with(
            vec![Ok(vec![
                ChatCompletionsEvent::ReasoningDelta {
                    text: "still working through the implementation".to_string(),
                    field: "reasoning_content".to_string(),
                },
                ChatCompletionsEvent::Finished {
                    finish_reason: Some("length".to_string()),
                },
            ])],
            Vec::new(),
        );

        let output = brain.wake_with_messages(
            context(),
            vec![ChatCompletionMessage::user("finish the implementation")],
        );

        assert!(!output.completed);
        assert_eq!(terminal_kind(&output.stream), "wake_failed");
        assert!(
            events(&output.stream).contains(&BrainEvent::ReasoningDelta {
                text: "still working through the implementation".to_string(),
                format: Some(CANONICAL_REASONING_FORMAT.to_string()),
            })
        );
        assert!(!events(&output.stream).contains(&BrainEvent::Finished));
        assert!(events(&output.stream).iter().any(|event| matches!(
            event,
            BrainEvent::ProviderStatus {
                level: BrainProviderStatusLevel::Info,
                message,
                metadata_json: Some(metadata),
            } if message == "Provider finished with reason: length"
                && metadata == r#"{"finish_reason":"length"}"#
        )));
        assert!(matches!(
            output.stream.last(),
            Some(BrainWakeStreamItem::WakeFailed { failure })
                if failure.reason_code.as_deref()
                    == Some(OUTPUT_LIMIT_EXCEEDED_REASON_CODE)
                    && failure.message.contains("finish_reason length")
        ));
    }

    #[test]
    fn minimal_loop_preserves_partial_text_when_output_limit_is_reached() {
        let mut brain = loop_with(
            vec![Ok(vec![
                ChatCompletionsEvent::ContentDelta(
                    "I found the target files and started the patch".to_string(),
                ),
                ChatCompletionsEvent::Finished {
                    finish_reason: Some("length".to_string()),
                },
            ])],
            Vec::new(),
        );

        let output = brain.wake_with_messages(
            context(),
            vec![ChatCompletionMessage::user("apply the patch")],
        );

        assert!(!output.completed);
        assert_eq!(terminal_kind(&output.stream), "wake_failed");
        assert!(events(&output.stream).contains(&BrainEvent::TextDelta {
            text: "I found the target files and started the patch".to_string(),
        }));
        assert!(!events(&output.stream).contains(&BrainEvent::Finished));
        assert!(!output
            .stream
            .iter()
            .any(|item| matches!(item, BrainWakeStreamItem::Actions { .. })));
    }

    #[test]
    fn minimal_loop_recovers_truncated_tool_arguments_at_output_limit() {
        let provider_stream = concat!(
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"lookup\",\"arguments\":\"{\\\"query\\\":\\\"den\"}}]},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"length\"}]}\n\n",
            "data: [DONE]\n\n",
        );
        let mut brain = loop_with(
            vec![
                Ok(parse(provider_stream).expect("parse truncated tool stream")),
                Ok(vec![
                    ChatCompletionsEvent::ToolCallFinished(tool_call(
                        "lookup",
                        r#"{"query":"den"}"#,
                    )),
                    ChatCompletionsEvent::Finished {
                        finish_reason: Some("tool_calls".to_string()),
                    },
                ]),
                Ok(vec![
                    ChatCompletionsEvent::ContentDelta("recovered".to_string()),
                    ChatCompletionsEvent::Finished {
                        finish_reason: Some("stop".to_string()),
                    },
                ]),
            ],
            vec![ChatCompletionsToolOutput::ok("found")],
        );

        let output =
            brain.wake_with_messages(context(), vec![ChatCompletionMessage::user("look it up")]);

        assert!(output.completed);
        assert_eq!(output.provider_request_count, 3);
        assert_eq!(output.tool_round_count, 1);
        assert_eq!(terminal_kind(&output.stream), "actions");
        assert_eq!(
            events(&output.stream)
                .iter()
                .filter(|event| matches!(event, BrainEvent::ToolCallStarted { .. }))
                .count(),
            1
        );
        assert!(events(&output.stream).iter().any(|event| matches!(
            event,
            BrainEvent::ProviderStatus {
                level: BrainProviderStatusLevel::Degraded,
                metadata_json: Some(metadata),
                ..
            } if metadata.contains("malformed_tool_call_recovery")
                && metadata.contains(OUTPUT_LIMIT_EXCEEDED_REASON_CODE)
        )));
        let recovery_messages = output.provider_request_debug_samples[1]["messages"]
            .as_array()
            .expect("recovery request messages");
        assert!(recovery_messages.iter().any(|message| {
            message["content"]
                .as_str()
                .is_some_and(|content| content.contains("No tool from that response was executed"))
        }));
        let Some(BrainWakeProviderStateOutput::Replace { state }) = output.provider_state else {
            panic!("successful recovery must persist provider state");
        };
        let persisted: ChatCompletionsProviderStateV1 =
            serde_json::from_value(state.payload).expect("persisted recovery history");
        assert!(persisted.messages.iter().all(|message| {
            !message
                .content
                .as_deref()
                .unwrap_or_default()
                .contains("Rusty Crew tool-call recovery")
        }));
    }

    #[test]
    fn minimal_loop_requires_attention_for_repeated_missing_name_at_length() {
        let provider_stream = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"partial answer\",\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"arguments\":\"{}\"}}]},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"length\"}]}\n\n",
            "data: [DONE]\n\n",
        );
        let mut brain = loop_with(
            vec![
                Ok(parse(provider_stream).expect("parse first incomplete tool stream")),
                Ok(parse(provider_stream).expect("parse repeated incomplete tool stream")),
                Ok(parse(provider_stream).expect("parse stalled incomplete tool stream")),
                Ok(parse(provider_stream).expect("parse attention incomplete tool stream")),
            ],
            Vec::new(),
        );

        let output =
            brain.wake_with_messages(context(), vec![ChatCompletionMessage::user("look it up")]);

        assert!(!output.completed);
        assert_eq!(output.provider_request_count, 4);
        assert_eq!(output.tool_round_count, 0);
        assert!(events(&output.stream).contains(&BrainEvent::TextDelta {
            text: "partial answer".to_string(),
        }));
        assert!(events(&output.stream).iter().any(|event| matches!(
            event,
            BrainEvent::ProviderStatus {
                level: BrainProviderStatusLevel::Error,
                message,
                metadata_json: Some(metadata),
            } if message.contains("function.name is missing")
                && metadata.contains("malformed_tool_call")
        )));
        assert!(!events(&output.stream).iter().any(|event| matches!(
            event,
            BrainEvent::ToolCallStarted { .. } | BrainEvent::ToolCallFinished { .. }
        )));
        assert!(output
            .stream
            .iter()
            .all(|item| !matches!(item, BrainWakeStreamItem::WakeFailed { .. })));
        assert_eq!(
            output
                .attention
                .as_ref()
                .map(|attention| attention.reason_code.as_str()),
            Some("chat_completions_malformed_tool_call_no_progress")
        );
        assert!(output.continuation_state.is_some());
    }

    #[test]
    fn minimal_loop_recovers_malformed_non_length_tool_call() {
        let malformed = MalformedChatFunctionCall {
            index: 0,
            id: Some("call_bad".to_string()),
            name: Some("lookup".to_string()),
            arguments_json: "[1,2]".to_string(),
            diagnostics: vec![
                "function.arguments must decode to a JSON object, found array".to_string(),
            ],
        };
        let mut brain = loop_with(
            vec![
                Ok(vec![
                    ChatCompletionsEvent::ToolCallMalformed(malformed),
                    ChatCompletionsEvent::Finished {
                        finish_reason: Some("stop".to_string()),
                    },
                ]),
                Ok(vec![
                    ChatCompletionsEvent::ToolCallFinished(tool_call(
                        "lookup",
                        r#"{"query":"den"}"#,
                    )),
                    ChatCompletionsEvent::Finished {
                        finish_reason: Some("tool_calls".to_string()),
                    },
                ]),
                Ok(vec![
                    ChatCompletionsEvent::ContentDelta("corrected".to_string()),
                    ChatCompletionsEvent::Finished {
                        finish_reason: Some("stop".to_string()),
                    },
                ]),
            ],
            vec![ChatCompletionsToolOutput::ok("found")],
        );

        let output =
            brain.wake_with_messages(context(), vec![ChatCompletionMessage::user("look it up")]);

        assert!(output.completed);
        assert_eq!(output.provider_request_count, 3);
        assert_eq!(output.tool_round_count, 1);
        assert_eq!(
            events(&output.stream)
                .iter()
                .filter(|event| matches!(event, BrainEvent::Finished))
                .count(),
            1
        );
        assert!(events(&output.stream).iter().any(|event| matches!(
            event,
            BrainEvent::ProviderStatus {
                level: BrainProviderStatusLevel::Degraded,
                metadata_json: Some(metadata),
                ..
            } if metadata.contains(MALFORMED_PROVIDER_STREAM_REASON_CODE)
        )));
    }

    #[test]
    fn minimal_loop_preserves_completed_round_across_malformed_recovery() {
        let valid_stream = concat!(
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"lookup\",\"arguments\":\"{}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n",
        );
        let malformed_stream = concat!(
            "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"continuing\",\"tool_calls\":[{\"index\":1,\"id\":\"call_2\",\"function\":{\"name\":\"patch\",\"arguments\":\"[1,2]\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n",
        );
        let mut brain = loop_with(
            vec![
                Ok(parse(valid_stream).expect("parse valid tool stream")),
                Ok(parse(malformed_stream).expect("parse malformed tool stream")),
                Ok(vec![
                    ChatCompletionsEvent::ToolCallFinished(PendingChatFunctionCall {
                        index: 0,
                        id: Some("call_3".to_string()),
                        name: "patch".to_string(),
                        arguments_json: r#"{"path":"target"}"#.to_string(),
                    }),
                    ChatCompletionsEvent::Finished {
                        finish_reason: Some("tool_calls".to_string()),
                    },
                ]),
                Ok(vec![
                    ChatCompletionsEvent::ContentDelta("patched".to_string()),
                    ChatCompletionsEvent::Finished {
                        finish_reason: Some("stop".to_string()),
                    },
                ]),
            ],
            vec![
                ChatCompletionsToolOutput::ok("found"),
                ChatCompletionsToolOutput::ok("patched target"),
            ],
        );

        let output = brain.wake_with_messages(
            context(),
            vec![ChatCompletionMessage::user("inspect and patch")],
        );

        assert!(output.completed);
        assert_eq!(output.tool_round_count, 2);
        assert_eq!(output.provider_request_count, 4);
        assert_eq!(
            events(&output.stream)
                .iter()
                .filter(|event| matches!(event, BrainEvent::ToolCallStarted { .. }))
                .count(),
            2
        );
        assert_eq!(
            events(&output.stream)
                .iter()
                .filter(|event| matches!(event, BrainEvent::ToolCallFinished { .. }))
                .count(),
            2
        );
        assert!(
            events(&output.stream).contains(&BrainEvent::ReasoningDelta {
                text: "continuing".to_string(),
                format: Some(CANONICAL_REASONING_FORMAT.to_string()),
            })
        );
        let recovery_messages = output.provider_request_debug_samples[2]["messages"]
            .as_array()
            .expect("post-malformed recovery request");
        assert!(recovery_messages
            .iter()
            .any(|message| message["content"] == "found"));
        assert!(recovery_messages
            .iter()
            .all(|message| message["reasoning_content"] != "continuing"));
        let Some(BrainWakeProviderStateOutput::Replace { state }) = output.provider_state else {
            panic!("successful recovery must persist completed rounds");
        };
        let persisted: ChatCompletionsProviderStateV1 =
            serde_json::from_value(state.payload).expect("persisted completed rounds");
        assert!(persisted
            .messages
            .iter()
            .any(|message| message.content.as_deref() == Some("found")));
        assert!(persisted
            .messages
            .iter()
            .any(|message| message.content.as_deref() == Some("patched target")));
        assert!(persisted.messages.iter().all(|message| {
            !message
                .content
                .as_deref()
                .unwrap_or_default()
                .contains("Rusty Crew tool-call recovery")
        }));
    }

    fn assert_completed_round_survives_attention_pause(finish_reason: &str) {
        let malformed_round = || {
            vec![
                ChatCompletionsEvent::ContentDelta("partial malformed output".to_string()),
                ChatCompletionsEvent::ReasoningDelta {
                    text: "malformed reasoning".to_string(),
                    field: "reasoning_content".to_string(),
                },
                ChatCompletionsEvent::ToolCallMalformed(MalformedChatFunctionCall {
                    index: 0,
                    id: Some("call_bad".to_string()),
                    name: Some("lookup".to_string()),
                    arguments_json: r#"{"query":"unfinished"#.to_string(),
                    diagnostics: vec![
                        "function.arguments is invalid JSON: EOF while parsing a string"
                            .to_string(),
                    ],
                }),
                ChatCompletionsEvent::Finished {
                    finish_reason: Some(finish_reason.to_string()),
                },
            ]
        };
        let mut brain = loop_with(
            vec![
                Ok(vec![
                    ChatCompletionsEvent::ToolCallFinished(tool_call(
                        "lookup",
                        r#"{"query":"commit side effect"}"#,
                    )),
                    ChatCompletionsEvent::Finished {
                        finish_reason: Some("tool_calls".to_string()),
                    },
                ]),
                Ok(malformed_round()),
                Ok(malformed_round()),
                Ok(malformed_round()),
                Ok(malformed_round()),
            ],
            vec![ChatCompletionsToolOutput::ok("side effect committed")],
        );

        let output = brain.wake_with_messages(
            context(),
            vec![ChatCompletionMessage::user("commit once, then continue")],
        );

        assert!(!output.completed);
        assert_eq!(output.provider_request_count, 5);
        assert_eq!(output.tool_round_count, 1);
        assert_eq!(
            events(&output.stream)
                .iter()
                .filter(|event| matches!(event, BrainEvent::ToolCallStarted { .. }))
                .count(),
            1,
            "malformed calls must never execute",
        );
        assert!(output
            .stream
            .iter()
            .all(|item| !matches!(item, BrainWakeStreamItem::WakeFailed { .. })));
        assert_eq!(
            output
                .attention
                .as_ref()
                .map(|attention| attention.reason_code.as_str()),
            Some("chat_completions_malformed_tool_call_no_progress")
        );
        let continuation = output
            .continuation_state
            .as_ref()
            .expect("attention pause must retain continuation");
        let persisted =
            chat_completions_continuation_state(continuation).expect("persisted continuation");
        assert!(persisted.durable_messages.iter().any(|message| {
            message.role == ChatMessageRole::Assistant
                && message.tool_calls.iter().any(|call| {
                    call.function.name == "lookup"
                        && call.function.arguments == r#"{"query":"commit side effect"}"#
                })
        }));
        assert!(persisted.durable_messages.iter().any(|message| {
            message.role == ChatMessageRole::Tool
                && message.content.as_deref() == Some("side effect committed")
        }));
        assert!(persisted.durable_messages.iter().all(|message| {
            let content = message.content.as_deref().unwrap_or_default();
            content != "partial malformed output"
                && !content.contains("Rusty Crew tool-call recovery")
                && message.reasoning_content.as_deref() != Some("malformed reasoning")
        }));
    }

    #[test]
    fn minimal_loop_preserves_completed_round_when_length_recovery_needs_attention() {
        assert_completed_round_survives_attention_pause("length");
    }

    #[test]
    fn minimal_loop_preserves_completed_round_when_malformed_recovery_needs_attention() {
        assert_completed_round_survives_attention_pause("stop");
    }

    #[test]
    fn minimal_loop_recovers_missing_arguments_without_executing_them() {
        let missing_arguments_stream = concat!(
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_missing\",\"function\":{\"name\":\"lookup\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n",
        );
        let corrected_stream = concat!(
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_corrected\",\"function\":{\"name\":\"lookup\",\"arguments\":\"{\\\"query\\\":\\\"den\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n",
        );
        let mut brain = loop_with(
            vec![
                Ok(parse(missing_arguments_stream).expect("parse missing arguments")),
                Ok(parse(corrected_stream).expect("parse corrected arguments")),
                Ok(vec![
                    ChatCompletionsEvent::ContentDelta("recovered".to_string()),
                    ChatCompletionsEvent::Finished {
                        finish_reason: Some("stop".to_string()),
                    },
                ]),
            ],
            vec![ChatCompletionsToolOutput::ok("found")],
        );

        let output =
            brain.wake_with_messages(context(), vec![ChatCompletionMessage::user("look it up")]);

        assert!(output.completed);
        assert_eq!(output.provider_request_count, 3);
        assert_eq!(output.tool_round_count, 1);
        assert_eq!(
            events(&output.stream)
                .iter()
                .filter(|event| matches!(event, BrainEvent::ToolCallStarted { .. }))
                .count(),
            1,
            "the malformed call must not execute",
        );
        assert!(events(&output.stream).iter().any(|event| matches!(
            event,
            BrainEvent::ProviderStatus {
                level: BrainProviderStatusLevel::Degraded,
                metadata_json: Some(metadata),
                ..
            } if metadata.contains(MALFORMED_PROVIDER_STREAM_REASON_CODE)
        )));
    }

    #[test]
    fn provider_error_during_recovery_preserves_completed_tool_history() {
        let malformed = MalformedChatFunctionCall {
            index: 0,
            id: Some("call_bad".to_string()),
            name: Some("lookup".to_string()),
            arguments_json: "".to_string(),
            diagnostics: vec!["function.arguments is missing".to_string()],
        };
        let mut brain = loop_with(
            vec![
                Ok(vec![
                    ChatCompletionsEvent::ToolCallFinished(tool_call(
                        "lookup",
                        r#"{"query":"commit once"}"#,
                    )),
                    ChatCompletionsEvent::Finished {
                        finish_reason: Some("tool_calls".to_string()),
                    },
                ]),
                Ok(vec![
                    ChatCompletionsEvent::ToolCallMalformed(malformed),
                    ChatCompletionsEvent::Finished {
                        finish_reason: Some("tool_calls".to_string()),
                    },
                ]),
                Err(ChatCompletionsStreamError::ProviderError(
                    "recovery request rejected".to_string(),
                )),
            ],
            vec![ChatCompletionsToolOutput::ok("side effect committed")],
        );

        let output = brain.wake_with_messages(
            context(),
            vec![ChatCompletionMessage::user("commit once, then continue")],
        );

        assert!(!output.completed);
        assert_eq!(output.provider_request_count, 3);
        assert_eq!(output.tool_round_count, 1);
        assert_eq!(
            events(&output.stream)
                .iter()
                .filter(|event| matches!(event, BrainEvent::ToolCallStarted { .. }))
                .count(),
            1,
        );
        let Some(BrainWakeProviderStateOutput::Replace { state }) = output.provider_state else {
            panic!("completed tool history must survive a recovery provider error");
        };
        let persisted: ChatCompletionsProviderStateV1 =
            serde_json::from_value(state.payload).expect("persisted completed tool history");
        assert!(persisted.messages.iter().any(|message| {
            message.role == ChatMessageRole::Tool
                && message.content.as_deref() == Some("side effect committed")
        }));
        assert!(persisted.messages.iter().all(|message| {
            !message
                .content
                .as_deref()
                .unwrap_or_default()
                .contains("Rusty Crew tool-call recovery")
        }));
    }

    #[test]
    fn minimal_loop_executes_one_tool_round_and_continues_with_tool_output() {
        let context = context();
        let mut brain = loop_with(
            vec![
                Ok(vec![
                    ChatCompletionsEvent::ToolCallFinished(tool_call("lookup", "{\"q\":\"den\"}")),
                    ChatCompletionsEvent::Finished {
                        finish_reason: Some("tool_calls".to_string()),
                    },
                ]),
                Ok(vec![
                    ChatCompletionsEvent::ContentDelta("found it".to_string()),
                    ChatCompletionsEvent::Finished {
                        finish_reason: Some("stop".to_string()),
                    },
                ]),
            ],
            vec![ChatCompletionsToolOutput::ok("tool says yes")],
        );

        let output =
            brain.wake_with_messages(context, vec![ChatCompletionMessage::user("lookup den")]);

        assert!(output.completed);
        assert_eq!(output.provider_request_count, 2);
        assert_eq!(output.tool_round_count, 1);
        assert_eq!(terminal_kind(&output.stream), "actions");
        assert!(events(&output.stream).iter().any(|event| matches!(
            event,
            BrainEvent::ToolCallStarted { tool_name, .. } if tool_name == "lookup"
        )));
        assert!(events(&output.stream).iter().any(|event| matches!(
            event,
            BrainEvent::ToolCallFinished {
                tool_name,
                is_error: false,
                ..
            } if tool_name == "lookup"
        )));
    }

    #[test]
    fn minimal_loop_continues_after_output_limit_with_an_actionable_tool_call() {
        let mut brain = loop_with(
            vec![
                Ok(vec![
                    ChatCompletionsEvent::ToolCallFinished(tool_call("lookup", "{\"q\":\"den\"}")),
                    ChatCompletionsEvent::Finished {
                        finish_reason: Some("length".to_string()),
                    },
                ]),
                Ok(vec![
                    ChatCompletionsEvent::ContentDelta("tool round completed".to_string()),
                    ChatCompletionsEvent::Finished {
                        finish_reason: Some("stop".to_string()),
                    },
                ]),
            ],
            vec![ChatCompletionsToolOutput::ok("tool says yes")],
        );

        let output =
            brain.wake_with_messages(context(), vec![ChatCompletionMessage::user("look it up")]);

        assert!(output.completed);
        assert_eq!(output.provider_request_count, 2);
        assert_eq!(output.tool_round_count, 1);
        assert_eq!(terminal_kind(&output.stream), "actions");
        assert!(events(&output.stream).contains(&BrainEvent::TextDelta {
            text: "tool round completed".to_string(),
        }));
    }

    #[test]
    fn minimal_loop_completes_more_than_eight_distinct_tool_rounds() {
        let tool_rounds = 12;
        let mut scripts = (1..=tool_rounds)
            .map(|round| {
                Ok(vec![
                    ChatCompletionsEvent::ToolCallFinished(tool_call(
                        "lookup",
                        &format!(r#"{{"round":{round}}}"#),
                    )),
                    ChatCompletionsEvent::Finished {
                        finish_reason: Some("tool_calls".to_string()),
                    },
                ])
            })
            .collect::<Vec<_>>();
        scripts.push(Ok(vec![
            ChatCompletionsEvent::ContentDelta("long turn complete".to_string()),
            ChatCompletionsEvent::Finished {
                finish_reason: Some("stop".to_string()),
            },
        ]));
        let outputs = (1..=tool_rounds)
            .map(|round| ChatCompletionsToolOutput::ok(format!("result {round}")))
            .collect();
        let mut brain =
            loop_with(scripts, outputs).with_loop_config(ChatCompletionsBrainLoopConfig {
                work_quantum_tool_rounds: DEFAULT_WORK_QUANTUM_TOOL_ROUNDS,
                no_progress_attention_threshold: DEFAULT_NO_PROGRESS_ATTENTION_THRESHOLD,
            });

        let output = brain.wake_with_messages(
            context(),
            vec![ChatCompletionMessage::user("complete the long tool turn")],
        );

        assert!(output.completed);
        assert_eq!(output.tool_round_count, tool_rounds);
        assert_eq!(output.provider_request_count, tool_rounds + 1);
        assert_eq!(terminal_kind(&output.stream), "actions");
        assert!(events(&output.stream).contains(&BrainEvent::TextDelta {
            text: "long turn complete".to_string()
        }));
    }

    #[test]
    fn minimal_loop_yields_and_resumes_beyond_each_work_quantum() {
        let mut brain = loop_with(
            vec![
                Ok(vec![
                    ChatCompletionsEvent::ToolCallFinished(tool_call("lookup", r#"{"round":1}"#)),
                    ChatCompletionsEvent::Finished {
                        finish_reason: Some("tool_calls".to_string()),
                    },
                ]),
                Ok(vec![
                    ChatCompletionsEvent::ToolCallFinished(tool_call("lookup", r#"{"round":2}"#)),
                    ChatCompletionsEvent::Finished {
                        finish_reason: Some("tool_calls".to_string()),
                    },
                ]),
                Ok(vec![
                    ChatCompletionsEvent::ContentDelta("continued to completion".to_string()),
                    ChatCompletionsEvent::Finished {
                        finish_reason: Some("stop".to_string()),
                    },
                ]),
            ],
            vec![
                ChatCompletionsToolOutput::ok("first result"),
                ChatCompletionsToolOutput::ok("second result"),
            ],
        )
        .with_loop_config(ChatCompletionsBrainLoopConfig {
            work_quantum_tool_rounds: 1,
            no_progress_attention_threshold: DEFAULT_NO_PROGRESS_ATTENTION_THRESHOLD,
        });

        let first = brain.wake(ChatCompletionsBrainLoopInput {
            context: context(),
            messages: vec![ChatCompletionMessage::user("continue across epochs")],
            input_images: Vec::new(),
            provider_state: None,
            continuation_state: None,
            final_message_fallback: None,
        });
        assert!(first.yielded);
        assert!(!first.completed);
        assert!(!first.stream.iter().any(BrainWakeStreamItem::is_terminal));

        let second = brain.wake(ChatCompletionsBrainLoopInput {
            context: context(),
            messages: vec![ChatCompletionMessage::user(
                "replacement input must not join the resumed turn",
            )],
            input_images: Vec::new(),
            provider_state: Some(BrainWakeProviderStateInput {
                module_id: "wrong-module".into(),
                strategy_id: "wrong-strategy".into(),
                profile_fingerprint: "wrong-profile".into(),
                provider_fingerprint: "wrong-provider".into(),
                payload_version: "wrong-version".into(),
                payload: json!({"invalid": true}),
                expires_at: None,
            }),
            continuation_state: first.continuation_state,
            final_message_fallback: None,
        });
        assert!(second.yielded);
        assert_eq!(second.tool_round_count, 2);
        assert!(!second.provider_request_debug_samples[1]["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| {
                message["content"] == "replacement input must not join the resumed turn"
            }));

        let completed = brain.wake(ChatCompletionsBrainLoopInput {
            context: context(),
            messages: Vec::new(),
            input_images: Vec::new(),
            provider_state: None,
            continuation_state: second.continuation_state,
            final_message_fallback: None,
        });
        assert!(completed.completed);
        assert!(!completed.yielded);
        assert_eq!(completed.tool_round_count, 2);
        assert_eq!(completed.provider_request_count, 3);
        assert!(events(&completed.stream).contains(&BrainEvent::TextDelta {
            text: "continued to completion".to_string(),
        }));
    }

    #[test]
    #[ignore = "focused >512-round continuation certification"]
    fn minimal_loop_completes_over_512_rounds_across_many_work_quanta() {
        const TOOL_ROUNDS: usize = 513;
        const WORK_QUANTUM: usize = 7;

        let mut scripts = (1..=TOOL_ROUNDS)
            .map(|round| {
                Ok(vec![
                    ChatCompletionsEvent::ToolCallFinished(tool_call(
                        "lookup",
                        &format!(r#"{{"round":{round}}}"#),
                    )),
                    ChatCompletionsEvent::Finished {
                        finish_reason: Some("tool_calls".to_string()),
                    },
                ])
            })
            .collect::<Vec<_>>();
        scripts.push(Ok(vec![
            ChatCompletionsEvent::ContentDelta("513-round turn complete".to_string()),
            ChatCompletionsEvent::Finished {
                finish_reason: Some("stop".to_string()),
            },
        ]));
        let outputs = (1..=TOOL_ROUNDS)
            .map(|round| ChatCompletionsToolOutput::ok(format!("result {round}")))
            .collect();
        let mut brain =
            loop_with(scripts, outputs).with_loop_config(ChatCompletionsBrainLoopConfig {
                work_quantum_tool_rounds: WORK_QUANTUM,
                no_progress_attention_threshold: DEFAULT_NO_PROGRESS_ATTENTION_THRESHOLD,
            });

        let mut continuation_state = None;
        let mut yielded_epochs = 0usize;
        let mut streamed_tool_finishes = 0usize;
        let completed = loop {
            let output = brain.wake(ChatCompletionsBrainLoopInput {
                context: context(),
                messages: if continuation_state.is_none() {
                    vec![ChatCompletionMessage::user(
                        "complete more than 512 distinct tool rounds",
                    )]
                } else {
                    vec![ChatCompletionMessage::user(
                        "replacement input must not enter the resumed turn",
                    )]
                },
                input_images: Vec::new(),
                provider_state: None,
                continuation_state,
                final_message_fallback: None,
            });
            streamed_tool_finishes += events(&output.stream)
                .iter()
                .filter(|event| matches!(event, BrainEvent::ToolCallFinished { .. }))
                .count();
            if output.yielded {
                assert!(!output.completed);
                assert!(!output.stream.iter().any(BrainWakeStreamItem::is_terminal));
                yielded_epochs += 1;
                continuation_state = output.continuation_state;
                continue;
            }
            break output;
        };

        assert!(completed.completed);
        assert_eq!(completed.tool_round_count, TOOL_ROUNDS);
        assert_eq!(completed.provider_request_count, TOOL_ROUNDS + 1);
        assert_eq!(streamed_tool_finishes, TOOL_ROUNDS);
        assert!(yielded_epochs > 64);
        assert_eq!(terminal_kind(&completed.stream), "actions");
        assert!(events(&completed.stream).contains(&BrainEvent::TextDelta {
            text: "513-round turn complete".to_string(),
        }));
    }

    #[test]
    fn minimal_loop_allows_tool_error_recovery() {
        let context = context();
        let mut brain = loop_with(
            vec![
                Ok(vec![
                    ChatCompletionsEvent::ToolCallFinished(tool_call("lookup", "{}")),
                    ChatCompletionsEvent::Finished {
                        finish_reason: Some("tool_calls".to_string()),
                    },
                ]),
                Ok(vec![
                    ChatCompletionsEvent::ContentDelta("I recovered".to_string()),
                    ChatCompletionsEvent::Finished {
                        finish_reason: Some("stop".to_string()),
                    },
                ]),
            ],
            vec![ChatCompletionsToolOutput::error("not available")],
        );

        let output = brain.wake_with_messages(context, vec![ChatCompletionMessage::user("go")]);

        assert!(output.completed);
        assert_eq!(terminal_kind(&output.stream), "actions");
        assert!(events(&output.stream).iter().any(|event| matches!(
            event,
            BrainEvent::ToolCallFinished {
                tool_name,
                is_error: true,
                ..
            } if tool_name == "lookup"
        )));
        assert!(events(&output.stream).contains(&BrainEvent::TextDelta {
            text: "I recovered".to_string()
        }));
    }

    #[test]
    fn minimal_loop_allows_repeated_identical_successful_tool_calls() {
        let context = context();
        let repeated = Ok(vec![
            ChatCompletionsEvent::ToolCallFinished(tool_call("lookup", "{}")),
            ChatCompletionsEvent::Finished {
                finish_reason: Some("tool_calls".to_string()),
            },
        ]);
        let mut brain = loop_with(
            vec![
                repeated.clone(),
                repeated.clone(),
                repeated.clone(),
                repeated,
                Ok(vec![
                    ChatCompletionsEvent::ContentDelta("all repeated work completed".to_string()),
                    ChatCompletionsEvent::Finished {
                        finish_reason: Some("stop".to_string()),
                    },
                ]),
            ],
            vec![
                ChatCompletionsToolOutput::ok("one"),
                ChatCompletionsToolOutput::ok("two"),
                ChatCompletionsToolOutput::ok("three"),
                ChatCompletionsToolOutput::ok("four"),
            ],
        );

        let output = brain.wake_with_messages(context, vec![ChatCompletionMessage::user("loop")]);

        assert!(output.completed);
        assert_eq!(output.tool_round_count, 4);
        assert_eq!(terminal_kind(&output.stream), "actions");
        assert!(events(&output.stream).contains(&BrainEvent::TextDelta {
            text: "all repeated work completed".to_string(),
        }));
    }

    #[test]
    fn minimal_loop_pauses_after_confirmed_repeated_failed_tool_calls() {
        let repeated = || {
            Ok(vec![
                ChatCompletionsEvent::ToolCallFinished(tool_call("lookup", "{}")),
                ChatCompletionsEvent::Finished {
                    finish_reason: Some("tool_calls".to_string()),
                },
            ])
        };
        let mut brain = loop_with(
            vec![repeated(), repeated(), repeated(), repeated()],
            vec![
                ChatCompletionsToolOutput::error("dependency unavailable"),
                ChatCompletionsToolOutput::error("dependency unavailable"),
                ChatCompletionsToolOutput::error("dependency unavailable"),
                ChatCompletionsToolOutput::error("dependency unavailable"),
            ],
        );

        let output = brain.wake_with_messages(
            context(),
            vec![ChatCompletionMessage::user(
                "retry the unavailable dependency",
            )],
        );

        assert!(!output.completed);
        assert_eq!(output.tool_round_count, 4);
        assert!(output
            .stream
            .iter()
            .all(|item| !matches!(item, BrainWakeStreamItem::WakeFailed { .. })));
        let attention = output.attention.expect("operator attention");
        assert_eq!(attention.reason_code, "chat_completions_tool_no_progress");
        assert_eq!(attention.consecutive_no_progress_samples, 3);
        assert!(output.continuation_state.is_some());
    }

    #[test]
    fn minimal_loop_fails_visibly_on_tool_cancellation_or_timeout() {
        for tool_output in [
            ChatCompletionsToolOutput::cancelled("cancelled by operator"),
            ChatCompletionsToolOutput::timed_out("tool timed out"),
        ] {
            let context = context();
            let mut brain = loop_with(
                vec![Ok(vec![
                    ChatCompletionsEvent::ToolCallFinished(tool_call("lookup", "{}")),
                    ChatCompletionsEvent::Finished {
                        finish_reason: Some("tool_calls".to_string()),
                    },
                ])],
                vec![tool_output.clone()],
            );

            let output = brain.wake_with_messages(context, vec![ChatCompletionMessage::user("go")]);
            assert!(!output.completed);
            assert_eq!(terminal_kind(&output.stream), "wake_failed");
            assert!(matches!(
                output.stream.last(),
                Some(BrainWakeStreamItem::WakeFailed { failure })
                    if failure.message == tool_output.output
            ));
        }
    }

    #[test]
    fn minimal_loop_fails_visibly_on_provider_error() {
        let context = context();
        let mut brain = loop_with(
            vec![Err(ChatCompletionsStreamError::Transport(
                "upstream closed".to_string(),
            ))],
            Vec::new(),
        );

        let output = brain.wake_with_messages(context, vec![ChatCompletionMessage::user("hi")]);

        assert!(!output.completed);
        assert_eq!(terminal_kind(&output.stream), "wake_failed");
        assert!(matches!(
            output.stream.last(),
            Some(BrainWakeStreamItem::WakeFailed { failure })
                if failure.message.contains("upstream closed")
        ));
    }

    #[test]
    fn minimal_loop_uses_final_message_fallback_when_stream_has_no_visible_text() {
        let context = context();
        let mut brain = loop_with(
            vec![Ok(vec![ChatCompletionsEvent::Finished {
                finish_reason: Some("stop".to_string()),
            }])],
            Vec::new(),
        );

        let output = brain.wake(ChatCompletionsBrainLoopInput {
            context,
            messages: vec![ChatCompletionMessage::user("hi")],
            input_images: Vec::new(),
            provider_state: None,
            continuation_state: None,
            final_message_fallback: Some(ChatCompletionsFinalMessage {
                text: Some("final-only".to_string()),
                ..ChatCompletionsFinalMessage::default()
            }),
        });

        assert!(output.completed);
        assert_eq!(terminal_kind(&output.stream), "actions");
        assert!(events(&output.stream).contains(&BrainEvent::TextDelta {
            text: "final-only".to_string()
        }));
    }

    #[test]
    fn mapper_splits_literal_think_tags_across_provider_chunks() {
        let context = context();
        let mut mapper = ChatCompletionsEventMapper::new();

        let mut items = Vec::new();
        items.extend(mapper.map_provider_event(
            &context,
            &ChatCompletionsEvent::ContentDelta("visible <thi".to_string()),
        ));
        items.extend(mapper.map_provider_event(
            &context,
            &ChatCompletionsEvent::ContentDelta("nk>secret</thi".to_string()),
        ));
        items.extend(mapper.map_provider_event(
            &context,
            &ChatCompletionsEvent::ContentDelta("nk> done".to_string()),
        ));
        items.extend(mapper.map_provider_event(
            &context,
            &ChatCompletionsEvent::Finished {
                finish_reason: Some("stop".to_string()),
            },
        ));

        assert_eq!(
            events(&items),
            vec![
                BrainEvent::TextDelta {
                    text: "visible ".to_string()
                },
                BrainEvent::ReasoningDelta {
                    text: "secret".to_string(),
                    format: Some("literal-think-tag".to_string()),
                },
                BrainEvent::TextDelta {
                    text: " done".to_string()
                },
                BrainEvent::Finished,
            ]
        );
    }

    #[test]
    fn mapper_keeps_unterminated_think_content_as_reasoning_at_finish() {
        let context = context();
        let mut mapper = ChatCompletionsEventMapper::new();

        let mut items = mapper.map_provider_event(
            &context,
            &ChatCompletionsEvent::ContentDelta("start <think>still hidden".to_string()),
        );
        items.extend(mapper.map_provider_event(
            &context,
            &ChatCompletionsEvent::Finished {
                finish_reason: Some("stop".to_string()),
            },
        ));

        assert_eq!(
            events(&items),
            vec![
                BrainEvent::TextDelta {
                    text: "start ".to_string()
                },
                BrainEvent::ReasoningDelta {
                    text: "still hidden".to_string(),
                    format: Some("literal-think-tag".to_string()),
                },
                BrainEvent::Finished,
            ]
        );
    }

    #[test]
    fn mapper_treats_nested_think_like_content_as_reasoning_until_first_close() {
        let context = context();
        let mut mapper = ChatCompletionsEventMapper::new();

        let mut items = mapper.map_provider_event(
            &context,
            &ChatCompletionsEvent::ContentDelta("a<think>b<think>c</think>d".to_string()),
        );
        items.extend(mapper.map_provider_event(
            &context,
            &ChatCompletionsEvent::Finished {
                finish_reason: Some("stop".to_string()),
            },
        ));

        assert_eq!(
            events(&items),
            vec![
                BrainEvent::TextDelta {
                    text: "a".to_string()
                },
                BrainEvent::ReasoningDelta {
                    text: "b<think>c".to_string(),
                    format: Some("literal-think-tag".to_string()),
                },
                BrainEvent::TextDelta {
                    text: "d".to_string()
                },
                BrainEvent::Finished,
            ]
        );
    }

    #[test]
    fn mapper_suppresses_final_message_when_streamed_text_was_seen() {
        let context = context();
        let mut mapper = ChatCompletionsEventMapper::new();

        let streamed = mapper.map_provider_event(
            &context,
            &ChatCompletionsEvent::ContentDelta("streamed".to_string()),
        );
        let fallback = mapper.map_final_message(
            &context,
            ChatCompletionsFinalMessage {
                text: Some("final duplicate".to_string()),
                ..ChatCompletionsFinalMessage::default()
            },
        );

        assert_eq!(
            events(&streamed),
            vec![BrainEvent::TextDelta {
                text: "streamed".to_string()
            }]
        );
        assert!(fallback.is_empty());
    }

    #[test]
    fn mapper_uses_final_message_text_thinking_and_error_fallbacks() {
        let context = context();
        let mut mapper = ChatCompletionsEventMapper::new();

        let items = mapper.map_final_message(
            &context,
            ChatCompletionsFinalMessage {
                text: Some("answer <think>trace</think>".to_string()),
                thinking: Some("native thought".to_string()),
                ..ChatCompletionsFinalMessage::default()
            },
        );

        assert_eq!(
            events(&items),
            vec![
                BrainEvent::ReasoningDelta {
                    text: "native thought".to_string(),
                    format: Some("chat-completions-thinking".to_string()),
                },
                BrainEvent::TextDelta {
                    text: "answer ".to_string()
                },
                BrainEvent::ReasoningDelta {
                    text: "trace".to_string(),
                    format: Some("literal-think-tag".to_string()),
                },
            ]
        );

        let mut mapper = ChatCompletionsEventMapper::new();
        let items = mapper.map_final_message(
            &context,
            ChatCompletionsFinalMessage {
                stop_reason: Some("error".to_string()),
                error_message: Some(" provider timed out ".to_string()),
                ..ChatCompletionsFinalMessage::default()
            },
        );
        assert_eq!(
            events(&items),
            vec![BrainEvent::TextDelta {
                text: "LLM error: provider timed out".to_string()
            }]
        );
    }

    #[test]
    fn mapper_projects_provider_reasoning_error_and_finish_reason() {
        let context = context();
        let mut mapper = ChatCompletionsEventMapper::new();

        let mut items = mapper.map_provider_event(
            &context,
            &ChatCompletionsEvent::ReasoningDelta {
                text: "chain".to_string(),
                field: "reasoning_content".to_string(),
            },
        );
        items.extend(mapper.map_provider_event(
            &context,
            &ChatCompletionsEvent::ProviderError("bad gateway".to_string()),
        ));
        items.extend(mapper.map_provider_event(
            &context,
            &ChatCompletionsEvent::Finished {
                finish_reason: Some("length".to_string()),
            },
        ));

        assert_eq!(
            events(&items),
            vec![
                BrainEvent::ReasoningDelta {
                    text: "chain".to_string(),
                    format: Some(CANONICAL_REASONING_FORMAT.to_string()),
                },
                BrainEvent::ProviderStatus {
                    level: BrainProviderStatusLevel::Error,
                    message: "Provider error: bad gateway".to_string(),
                    metadata_json: None,
                },
                BrainEvent::ProviderStatus {
                    level: BrainProviderStatusLevel::Info,
                    message: "Provider finished with reason: length".to_string(),
                    metadata_json: Some("{\"finish_reason\":\"length\"}".to_string()),
                },
            ]
        );
    }

    #[test]
    fn stepfun_like_reasoning_aliases_share_canonical_semantics_and_raw_counts() {
        let context = context();
        let mut mapper = ChatCompletionsEventMapper::new();
        let mut provider_event_counts = BTreeMap::new();

        let fields = [
            "reasoning_content",
            "reasoning",
            "reasoning_delta",
            "thinking",
        ];
        for index in 0..5_000 {
            let provider_event = ChatCompletionsEvent::ReasoningDelta {
                text: "r".to_string(),
                field: fields[index % fields.len()].to_string(),
            };
            record_provider_event(&mut provider_event_counts, &provider_event);
            let mapped = mapper.map_provider_event(&context, &provider_event);
            assert_eq!(mapped.len(), 1);
            assert!(matches!(
                &mapped[0],
                BrainWakeStreamItem::Event { event }
                    if matches!(&event.event, BrainEvent::ReasoningDelta { format, .. }
                        if format.as_deref() == Some(CANONICAL_REASONING_FORMAT))
            ));
        }
        assert_eq!(provider_event_counts["reasoning_delta"], 5_000);
        for field in fields {
            assert_eq!(
                provider_event_counts[&format!("reasoning_delta:{field}")],
                1_250
            );
        }
    }
}
