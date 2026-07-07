//! Rust provider-wire foundation for the pi-agent brain.
//!
//! This crate intentionally stops below the full agent loop. It owns
//! OpenAI-compatible chat-completions request construction, live SSE transport,
//! and provider stream parsing. Coordination, profile loading, tool execution,
//! and service-host wiring stay outside this crate.

use reqwest::blocking::Client as HttpClient;
use rusty_crew_core_protocol::{
    BrainActionBatch, BrainEvent, BrainEventEnvelope, BrainProviderStatusLevel, BrainWakeFailure,
    BrainWakeStreamItem, CoreErrorKind, ModelProviderRecord, SessionId,
};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::io::Read;
use std::time::Duration;

pub const MODULE_ID: &str = "pi-agent-rust";
pub const DEFAULT_DEN_ROUTER_URL: &str = "http://127.0.0.1:18082";
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
    client: HttpClient,
}

impl LiveDenRouterModelSource {
    pub fn new(base_url: Option<String>) -> Result<Self, DenRouterSelectionError> {
        let client = HttpClient::builder()
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
pub struct PiAgentChatConfig {
    pub model: String,
    pub temperature_milli: Option<u32>,
    pub max_output_tokens: Option<u32>,
    pub stream_idle_timeout_ms: u64,
}

impl PiAgentChatConfig {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            temperature_milli: None,
            max_output_tokens: Some(128),
            stream_idle_timeout_ms: 30_000,
        }
    }

    pub fn from_model_provider(provider: &ModelProviderRecord) -> Self {
        Self {
            model: provider.model_id.clone(),
            temperature_milli: provider.temperature_milli,
            max_output_tokens: provider.max_output_tokens,
            stream_idle_timeout_ms: 30_000,
        }
    }
}

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
            name: None,
            tool_call_id: Some(tool_call_id.into()),
            tool_calls: Vec::new(),
        }
    }

    fn text(role: ChatMessageRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: Some(content.into()),
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
    pub messages: Vec<ChatCompletionMessage>,
    pub tools: Vec<ChatToolDescriptor>,
    pub tool_choice: Value,
    pub stream: bool,
    pub stream_options: Option<ChatCompletionsStreamOptions>,
    pub temperature: Option<f64>,
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
    config: PiAgentChatConfig,
    tools: Vec<NeutralBrainTool>,
    tool_choice: ChatToolChoice,
    include_usage: bool,
}

impl ChatCompletionsRequestBuilder {
    pub fn new(config: PiAgentChatConfig) -> Self {
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
        ChatCompletionsRequest {
            model: self.config.model.clone(),
            messages,
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
            max_tokens: self.config.max_output_tokens,
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
    Usage(ChatTokenUsage),
    Finished {
        finish_reason: Option<String>,
    },
    ProviderError(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PiAgentToolOutput {
    pub output: String,
    pub is_error: bool,
    pub cancelled: bool,
    pub timed_out: bool,
}

impl PiAgentToolOutput {
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

pub trait PiAgentNeutralToolExecutor {
    fn execute(&self, call: &PendingChatFunctionCall) -> PiAgentToolOutput;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PiAgentBrainLoopConfig {
    pub max_tool_rounds: usize,
    pub repeated_tool_call_limit: usize,
}

impl Default for PiAgentBrainLoopConfig {
    fn default() -> Self {
        Self {
            max_tool_rounds: 8,
            repeated_tool_call_limit: 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PiAgentBrainLoopInput {
    pub context: BrainEventContext,
    pub messages: Vec<ChatCompletionMessage>,
    pub final_message_fallback: Option<PiAgentFinalMessage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PiAgentBrainLoopOutput {
    pub stream: Vec<BrainWakeStreamItem>,
    pub completed: bool,
    pub provider_request_count: usize,
    pub tool_round_count: usize,
}

pub struct PiAgentBrainLoop<C, T> {
    client: C,
    tools: T,
    request_builder: ChatCompletionsRequestBuilder,
    config: PiAgentBrainLoopConfig,
}

impl<C, T> PiAgentBrainLoop<C, T>
where
    C: ChatCompletionsClient,
    T: PiAgentNeutralToolExecutor,
{
    pub fn new(
        client: C,
        tools: T,
        chat_config: PiAgentChatConfig,
        descriptors: Vec<NeutralBrainTool>,
    ) -> Self {
        Self {
            client,
            tools,
            request_builder: ChatCompletionsRequestBuilder::new(chat_config).tools(descriptors),
            config: PiAgentBrainLoopConfig::default(),
        }
    }

    pub fn with_loop_config(mut self, config: PiAgentBrainLoopConfig) -> Self {
        self.config = config;
        self
    }

    pub fn wake_with_messages(
        &mut self,
        context: BrainEventContext,
        messages: Vec<ChatCompletionMessage>,
    ) -> PiAgentBrainLoopOutput {
        self.wake(PiAgentBrainLoopInput {
            context,
            messages,
            final_message_fallback: None,
        })
    }

    pub fn wake(&mut self, input: PiAgentBrainLoopInput) -> PiAgentBrainLoopOutput {
        let mut mapper = PiAgentEventMapper::new();
        let mut stream = mapper.map_started(&input.context);
        let mut messages = input.messages;
        let mut repeated_calls: HashMap<(String, String), usize> = HashMap::new();
        let mut provider_request_count = 0;
        let mut tool_round_count = 0;

        loop {
            provider_request_count += 1;
            let request = self.request_builder.build(messages.clone());
            let mut assistant_text = String::new();
            let mut tool_calls = Vec::new();
            let result = self.client.stream_observed(request, &mut |event| {
                if let ChatCompletionsEvent::ContentDelta(text) = event {
                    assistant_text.push_str(text);
                }
                if let ChatCompletionsEvent::ToolCallFinished(call) = event {
                    tool_calls.push(call.clone());
                }
                if matches!(
                    event,
                    ChatCompletionsEvent::Finished {
                        finish_reason: Some(reason)
                    } if reason == "tool_calls"
                ) {
                    return;
                }
                stream.extend(mapper.map_provider_event(&input.context, event));
            });

            if let Err(error) = result {
                stream.push(wake_failed_item(
                    &input.context,
                    CoreErrorKind::BrainUnavailable,
                    format!("pi-agent provider stream failed: {error}"),
                ));
                return PiAgentBrainLoopOutput {
                    stream,
                    completed: false,
                    provider_request_count,
                    tool_round_count,
                };
            }

            if tool_calls.is_empty() {
                if assistant_text.trim().is_empty() {
                    if let Some(fallback) = input.final_message_fallback {
                        stream.extend(mapper.map_final_message(&input.context, fallback));
                    }
                }
                stream.push(success_actions_item(&input.context));
                return PiAgentBrainLoopOutput {
                    stream,
                    completed: true,
                    provider_request_count,
                    tool_round_count,
                };
            }

            if tool_round_count >= self.config.max_tool_rounds {
                stream.push(wake_failed_item(
                    &input.context,
                    CoreErrorKind::BrainUnavailable,
                    format!(
                        "pi-agent exceeded {} tool continuation rounds",
                        self.config.max_tool_rounds
                    ),
                ));
                return PiAgentBrainLoopOutput {
                    stream,
                    completed: false,
                    provider_request_count,
                    tool_round_count,
                };
            }
            tool_round_count += 1;

            messages.push(assistant_tool_call_message(&assistant_text, &tool_calls));
            for call in tool_calls {
                let repeated_key = (call.name.clone(), call.arguments_json.clone());
                let count = repeated_calls.entry(repeated_key).or_insert(0);
                *count += 1;
                if *count > self.config.repeated_tool_call_limit {
                    stream.push(wake_failed_item(
                        &input.context,
                        CoreErrorKind::BrainUnavailable,
                        format!(
                            "pi-agent repeated tool call {} with unchanged arguments more than {} times",
                            call.name, self.config.repeated_tool_call_limit
                        ),
                    ));
                    return PiAgentBrainLoopOutput {
                        stream,
                        completed: false,
                        provider_request_count,
                        tool_round_count,
                    };
                }

                stream.push(brain_event_item(
                    &input.context,
                    BrainEvent::ToolCallStarted {
                        tool_name: call.name.clone(),
                        metadata: None,
                    },
                ));
                let output = self.tools.execute(&call);
                stream.push(brain_event_item(
                    &input.context,
                    BrainEvent::ToolCallFinished {
                        tool_name: call.name.clone(),
                        is_error: output.is_error,
                        metadata: None,
                    },
                ));
                if output.cancelled || output.timed_out {
                    let kind = if output.timed_out {
                        CoreErrorKind::TimeoutExpired
                    } else {
                        CoreErrorKind::BrainUnavailable
                    };
                    stream.push(wake_failed_item(&input.context, kind, output.output));
                    return PiAgentBrainLoopOutput {
                        stream,
                        completed: false,
                        provider_request_count,
                        tool_round_count,
                    };
                }
                messages.push(ChatCompletionMessage::tool(
                    call.id
                        .clone()
                        .unwrap_or_else(|| format!("call_{}", call.index)),
                    output.output,
                ));
            }
        }
    }
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
pub struct PiAgentFinalMessage {
    pub text: Option<String>,
    pub thinking: Option<String>,
    pub stop_reason: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Default)]
pub struct PiAgentEventMapper {
    saw_text_delta: bool,
    think_scanner: LiteralThinkScanner,
}

impl PiAgentEventMapper {
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
            ChatCompletionsEvent::ReasoningDelta { text, field } => non_empty_event(
                context,
                BrainEvent::ReasoningDelta {
                    text: text.clone(),
                    format: Some(format!("chat-completions:{field}")),
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
            ChatCompletionsEvent::Finished { finish_reason } => {
                let mut items = self.finish_text_scanner(context);
                if let Some(reason) = finish_reason {
                    if reason != "stop" && reason != "tool_calls" {
                        items.push(brain_event_item(
                            context,
                            BrainEvent::ProviderStatus {
                                level: BrainProviderStatusLevel::Info,
                                message: format!("Provider finished with reason: {reason}"),
                                metadata_json: None,
                            },
                        ));
                    }
                }
                items.push(brain_event_item(context, BrainEvent::Finished));
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
        message: PiAgentFinalMessage,
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
                    format: Some("pi-thinking".to_string()),
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
        message: message.into(),
    })
}

fn assistant_tool_call_message(
    content: &str,
    calls: &[PendingChatFunctionCall],
) -> ChatCompletionMessage {
    ChatCompletionMessage {
        role: ChatMessageRole::Assistant,
        content: (!content.is_empty()).then(|| content.to_string()),
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
    #[error("provider stream idle timeout")]
    IdleTimeout,
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

#[derive(Debug, Clone)]
pub struct LiveChatCompletionsClient {
    client: HttpClient,
    endpoint: String,
    bearer_token: Option<String>,
}

impl LiveChatCompletionsClient {
    pub fn new(
        base_url: impl Into<String>,
        api_key: Option<String>,
        idle_timeout_ms: u64,
    ) -> Result<Self, ChatCompletionsStreamError> {
        let endpoint = chat_completions_endpoint(&base_url.into());
        let client = HttpClient::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_millis(idle_timeout_ms))
            .build()
            .map_err(|error| ChatCompletionsStreamError::Transport(error.to_string()))?;
        Ok(Self {
            client,
            endpoint,
            bearer_token: api_key,
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
        let mut response = request.send().map_err(transport_error)?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().map_err(transport_error)?;
            return Err(ChatCompletionsStreamError::Transport(format!(
                "HTTP {status}: {body}"
            )));
        }
        parse_sse_response(&mut response, on_event)
    }
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
    if error.is_timeout() {
        ChatCompletionsStreamError::IdleTimeout
    } else {
        ChatCompletionsStreamError::Transport(error.to_string())
    }
}

fn parse_sse_response(
    response: &mut reqwest::blocking::Response,
    on_event: &mut dyn FnMut(&ChatCompletionsEvent),
) -> Result<Vec<ChatCompletionsEvent>, ChatCompletionsStreamError> {
    parse_sse_reader(response, on_event)
}

pub fn parse_sse_reader<R: Read>(
    reader: &mut R,
    on_event: &mut dyn FnMut(&ChatCompletionsEvent),
) -> Result<Vec<ChatCompletionsEvent>, ChatCompletionsStreamError> {
    let mut parser = ChatCompletionsSseParser::default();
    let mut buffer = [0_u8; 8192];

    loop {
        let read = reader.read(&mut buffer).map_err(|error| {
            if error.kind() == std::io::ErrorKind::TimedOut {
                ChatCompletionsStreamError::IdleTimeout
            } else {
                ChatCompletionsStreamError::Transport(error.to_string())
            }
        })?;
        if read == 0 {
            break;
        }
        let chunk = String::from_utf8_lossy(&buffer[..read]);
        parser.push_text(&chunk, on_event)?;
    }

    parser.finish(on_event)
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
            self.accumulator.saw_terminal = true;
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
                for call in self.flush_tool_calls()? {
                    self.push(ChatCompletionsEvent::ToolCallFinished(call), on_event);
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
            let index = raw_call.get("index").and_then(Value::as_u64).ok_or(
                ChatCompletionsStreamError::MissingField("choices[].delta.tool_calls[].index"),
            )? as u32;
            let id = raw_call
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string);
            let function = raw_call.get("function");
            let name = function
                .and_then(|value| value.get("name"))
                .and_then(Value::as_str)
                .map(str::to_string);
            let arguments_delta = function
                .and_then(|value| value.get("arguments"))
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
            builder.arguments_json.push_str(&arguments_delta);

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

    fn flush_tool_calls(
        &mut self,
    ) -> Result<Vec<PendingChatFunctionCall>, ChatCompletionsStreamError> {
        let pending = std::mem::take(&mut self.pending_tool_calls);
        pending
            .into_iter()
            .map(|(index, builder)| {
                Ok(PendingChatFunctionCall {
                    index,
                    id: builder.id,
                    name: builder
                        .name
                        .ok_or(ChatCompletionsStreamError::MissingField(
                            "choices[].delta.tool_calls[].function.name",
                        ))?,
                    arguments_json: if builder.arguments_json.trim().is_empty() {
                        "{}".to_string()
                    } else {
                        builder.arguments_json
                    },
                })
            })
            .collect()
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
    use std::io::{Cursor, Read};

    fn parse(input: &str) -> Result<Vec<ChatCompletionsEvent>, ChatCompletionsStreamError> {
        parse_sse_reader(&mut Cursor::new(input.as_bytes()), &mut |_| {})
    }

    #[test]
    fn builds_chat_completions_request_from_provider_config() {
        let request = ChatCompletionsRequestBuilder::new(PiAgentChatConfig {
            model: "deepseek-flash".to_string(),
            temperature_milli: Some(500),
            max_output_tokens: Some(256),
            stream_idle_timeout_ms: 45_000,
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
        assert_eq!(value["max_tokens"], 256);
        assert_eq!(value["tools"][0]["type"], "function");
        assert_eq!(value["tools"][0]["function"]["name"], "lookup");
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
        outputs: std::sync::Mutex<VecDeque<PiAgentToolOutput>>,
    }

    impl ScriptedToolExecutor {
        fn new(outputs: impl IntoIterator<Item = PiAgentToolOutput>) -> Self {
            Self {
                outputs: std::sync::Mutex::new(outputs.into_iter().collect()),
            }
        }
    }

    impl PiAgentNeutralToolExecutor for ScriptedToolExecutor {
        fn execute(&self, _call: &PendingChatFunctionCall) -> PiAgentToolOutput {
            self.outputs
                .lock()
                .expect("tool script mutex")
                .pop_front()
                .unwrap_or_else(|| PiAgentToolOutput::ok("default tool output"))
        }
    }

    fn loop_with(
        scripts: Vec<Result<Vec<ChatCompletionsEvent>, ChatCompletionsStreamError>>,
        outputs: Vec<PiAgentToolOutput>,
    ) -> PiAgentBrainLoop<FakeChatCompletionsClient, ScriptedToolExecutor> {
        PiAgentBrainLoop::new(
            FakeChatCompletionsClient::new(scripts),
            ScriptedToolExecutor::new(outputs),
            PiAgentChatConfig::new("deepseek-flash"),
            vec![NeutralBrainTool {
                name: "lookup".to_string(),
                description: "Look up".to_string(),
                input_schema: json!({"type": "object"}),
            }],
        )
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
            vec![PiAgentToolOutput::ok("tool says yes")],
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
            vec![PiAgentToolOutput::error("not available")],
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
    fn minimal_loop_rejects_repeated_identical_tool_calls() {
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
            ],
            vec![
                PiAgentToolOutput::ok("one"),
                PiAgentToolOutput::ok("two"),
                PiAgentToolOutput::ok("three"),
                PiAgentToolOutput::ok("four"),
            ],
        );

        let output = brain.wake_with_messages(context, vec![ChatCompletionMessage::user("loop")]);

        assert!(!output.completed);
        assert_eq!(terminal_kind(&output.stream), "wake_failed");
        assert!(matches!(
            output.stream.last(),
            Some(BrainWakeStreamItem::WakeFailed { failure })
                if failure.message.contains("repeated tool call lookup")
        ));
    }

    #[test]
    fn minimal_loop_fails_visibly_on_tool_cancellation_or_timeout() {
        for tool_output in [
            PiAgentToolOutput::cancelled("cancelled by operator"),
            PiAgentToolOutput::timed_out("tool timed out"),
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

        let output = brain.wake(PiAgentBrainLoopInput {
            context,
            messages: vec![ChatCompletionMessage::user("hi")],
            final_message_fallback: Some(PiAgentFinalMessage {
                text: Some("final-only".to_string()),
                ..PiAgentFinalMessage::default()
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
        let mut mapper = PiAgentEventMapper::new();

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
        let mut mapper = PiAgentEventMapper::new();

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
        let mut mapper = PiAgentEventMapper::new();

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
        let mut mapper = PiAgentEventMapper::new();

        let streamed = mapper.map_provider_event(
            &context,
            &ChatCompletionsEvent::ContentDelta("streamed".to_string()),
        );
        let fallback = mapper.map_final_message(
            &context,
            PiAgentFinalMessage {
                text: Some("final duplicate".to_string()),
                ..PiAgentFinalMessage::default()
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
        let mut mapper = PiAgentEventMapper::new();

        let items = mapper.map_final_message(
            &context,
            PiAgentFinalMessage {
                text: Some("answer <think>trace</think>".to_string()),
                thinking: Some("native thought".to_string()),
                ..PiAgentFinalMessage::default()
            },
        );

        assert_eq!(
            events(&items),
            vec![
                BrainEvent::ReasoningDelta {
                    text: "native thought".to_string(),
                    format: Some("pi-thinking".to_string()),
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

        let mut mapper = PiAgentEventMapper::new();
        let items = mapper.map_final_message(
            &context,
            PiAgentFinalMessage {
                stop_reason: Some("error".to_string()),
                error_message: Some(" provider timed out ".to_string()),
                ..PiAgentFinalMessage::default()
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
        let mut mapper = PiAgentEventMapper::new();

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
                    format: Some("chat-completions:reasoning_content".to_string()),
                },
                BrainEvent::ProviderStatus {
                    level: BrainProviderStatusLevel::Error,
                    message: "Provider error: bad gateway".to_string(),
                    metadata_json: None,
                },
                BrainEvent::ProviderStatus {
                    level: BrainProviderStatusLevel::Info,
                    message: "Provider finished with reason: length".to_string(),
                    metadata_json: None,
                },
                BrainEvent::Finished,
            ]
        );
    }
}
