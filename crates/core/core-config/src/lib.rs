//! Pure runtime/profile config validation for Rusty Crew service control-plane data.
//!
//! This crate validates draft config graphs before TypeScript writes files or
//! applies changes to the engine. It deliberately does not load profile files,
//! render prompts, discover tools, call providers, or mutate runtime state.

use rusty_crew_core_protocol::{
    AdapterId, AgentId, AgentInstanceId, BrainImplementationId, ProfileId, ResourceLimits,
    SessionHistoryWindow, SessionId, SessionKind,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

const MAX_HISTORY_MESSAGES: u32 = 10_000;
const MAX_DURATION_MS: u32 = 30 * 24 * 60 * 60 * 1_000;
const MAX_DELEGATION_DEPTH: u32 = 64;
const MAX_TURN_TIMEOUT_MS: u32 = 24 * 60 * 60 * 1_000;
const ID_PATTERN_DESCRIPTION: &str =
    "must start with a letter or digit and contain only letters, digits, '.', '_', ':' or '-'";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeConfigDraft {
    pub profiles_dir: String,
    pub skills_dir: Option<String>,
    #[serde(default)]
    pub brains: Vec<BrainConfigDraft>,
    #[serde(default)]
    pub sessions: Vec<SessionConfigDraft>,
    #[serde(default)]
    pub scheduled_jobs: Vec<ScheduledJobConfigDraft>,
    #[serde(default)]
    pub channel_bindings: Vec<ChannelBindingConfigDraft>,
    #[serde(default)]
    pub mcp_bindings: Vec<McpBindingConfigDraft>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrainConfigDraft {
    pub implementation_id: BrainImplementationId,
    pub profile_id: ProfileId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionConfigDraft {
    pub session_id: SessionId,
    pub agent_id: AgentId,
    pub profile_id: ProfileId,
    pub kind: SessionKind,
    pub resource_limits: Option<ResourceLimits>,
    pub owner_id: Option<String>,
    pub history_window: Option<SessionHistoryWindow>,
    pub max_history_messages: Option<u32>,
    pub turn_timeout_ms: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduledJobShape {
    HostJob,
    SessionWake,
    ScriptOnly,
    DataCollection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduledJobConfigDraft {
    pub id: String,
    pub schedule: String,
    pub shape: ScheduledJobShape,
    pub job_kind: Option<String>,
    pub target_session_id: Option<SessionId>,
    pub script: Option<String>,
    pub delivery_channel_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalBindingStatusDraft {
    Active,
    Degraded,
    Archived,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelBindingConfigDraft {
    pub binding_id: String,
    pub adapter_id: AdapterId,
    pub provider: String,
    pub agent_id: AgentId,
    pub instance_id: Option<AgentInstanceId>,
    pub session_id: Option<SessionId>,
    pub profile_id: ProfileId,
    pub external_channel_id: String,
    pub external_thread_id: Option<String>,
    pub external_user_id: Option<String>,
    pub conversation_project_id: Option<String>,
    pub conversation_channel_id: Option<u32>,
    pub provider_subscription_id: Option<String>,
    pub status: ExternalBindingStatusDraft,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpBindingConfigDraft {
    pub binding_id: String,
    pub adapter_id: AdapterId,
    pub agent_id: AgentId,
    pub instance_id: Option<AgentInstanceId>,
    pub session_id: Option<SessionId>,
    pub profile_id: ProfileId,
    pub server_names: Vec<String>,
    pub endpoint_ref: String,
    pub transport: String,
    pub tool_profile_key: String,
    pub status: ExternalBindingStatusDraft,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileRuntimeMetadata {
    pub profile_id: ProfileId,
    pub brain: Option<ProfileBrainMetadata>,
    pub runtime: Option<ProfileRuntimeOptions>,
    pub session_defaults: Option<ProfileSessionDefaults>,
    pub mcp_config: Option<ProfileMcpConfig>,
    pub background_review: Option<ProfileBackgroundReviewConfig>,
    pub channel_defaults: Option<ProfileChannelDefaults>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileBrainMetadata {
    pub module: Option<String>,
    pub strategy: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileRuntimeOptions {
    pub default_resource_limits: Option<ResourceLimits>,
    pub max_turn_duration_ms: Option<u32>,
    pub max_tokens_per_turn: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileSessionDefaults {
    pub owner_id: Option<String>,
    pub max_history_messages: Option<u32>,
    pub turn_timeout_ms: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileMcpConfig {
    pub binding_id: Option<String>,
    pub endpoint_ref: Option<String>,
    pub server_names: Vec<String>,
    pub transport: Option<String>,
    pub tool_profile: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileBackgroundReviewType {
    Memory,
    Skills,
    Combined,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileBackgroundReviewConfig {
    pub enabled: bool,
    pub review_type: Option<ProfileBackgroundReviewType>,
    pub schedule: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelWakePolicy {
    Subscription,
    Manual,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileChannelDefaults {
    pub wake_policy: Option<ChannelWakePolicy>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeConfigDiagnosticSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeConfigDiagnostic {
    pub severity: RuntimeConfigDiagnosticSeverity,
    pub code: String,
    pub path: Option<String>,
    pub message: String,
}

impl RuntimeConfigDiagnostic {
    pub fn error(
        code: impl Into<String>,
        path: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: RuntimeConfigDiagnosticSeverity::Error,
            code: code.into(),
            path: Some(path.into()),
            message: message.into(),
        }
    }

    pub fn warning(
        code: impl Into<String>,
        path: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: RuntimeConfigDiagnosticSeverity::Warning,
            code: code.into(),
            path: Some(path.into()),
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeConfigValidationResult {
    pub diagnostics: Vec<RuntimeConfigDiagnostic>,
}

impl RuntimeConfigValidationResult {
    pub fn ok(&self) -> bool {
        !self
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == RuntimeConfigDiagnosticSeverity::Error)
    }
}

pub fn validate_runtime_config_draft(
    draft: &RuntimeConfigDraft,
    profiles: &[ProfileRuntimeMetadata],
) -> RuntimeConfigValidationResult {
    let mut validator = RuntimeConfigValidator::new(draft, profiles);
    validator.validate();
    RuntimeConfigValidationResult {
        diagnostics: validator.diagnostics,
    }
}

struct RuntimeConfigValidator<'a> {
    draft: &'a RuntimeConfigDraft,
    profiles: &'a [ProfileRuntimeMetadata],
    diagnostics: Vec<RuntimeConfigDiagnostic>,
    profile_ids: HashSet<ProfileId>,
    sessions_by_id: HashMap<SessionId, &'a SessionConfigDraft>,
}

impl<'a> RuntimeConfigValidator<'a> {
    fn new(draft: &'a RuntimeConfigDraft, profiles: &'a [ProfileRuntimeMetadata]) -> Self {
        Self {
            draft,
            profiles,
            diagnostics: Vec::new(),
            profile_ids: HashSet::new(),
            sessions_by_id: HashMap::new(),
        }
    }

    fn validate(&mut self) {
        self.validate_root_paths();
        self.validate_profile_metadata();
        self.validate_brains();
        self.validate_sessions();
        self.validate_scheduled_jobs();
        self.validate_channel_bindings();
        self.validate_mcp_bindings();
    }

    fn validate_root_paths(&mut self) {
        if self.draft.profiles_dir.trim().is_empty() {
            self.error(
                "missing_profiles_dir",
                "profilesDir",
                "profilesDir is required",
            );
        }
        if self
            .draft
            .skills_dir
            .as_deref()
            .is_some_and(|value| value.trim().is_empty())
        {
            self.error(
                "invalid_skills_dir",
                "skillsDir",
                "skillsDir must not be empty when provided",
            );
        }
    }

    fn validate_profile_metadata(&mut self) {
        let mut seen = HashSet::new();
        for (index, profile) in self.profiles.iter().enumerate() {
            let path = format!("profiles[{index}].profileId");
            validate_id_text(self, "invalid_profile_id", &path, &profile.profile_id.0);
            if !seen.insert(profile.profile_id.clone()) {
                self.error(
                    "duplicate_profile_id",
                    path,
                    format!("duplicate profile metadata {}", profile.profile_id),
                );
            }
            self.profile_ids.insert(profile.profile_id.clone());
            if let Some(runtime) = &profile.runtime {
                validate_resource_limits(
                    self,
                    &format!("profiles[{index}].runtime.defaultResourceLimits"),
                    runtime.default_resource_limits.as_ref(),
                );
                validate_optional_max(
                    self,
                    "invalid_turn_duration",
                    &format!("profiles[{index}].runtime.maxTurnDurationMs"),
                    runtime.max_turn_duration_ms,
                    MAX_TURN_TIMEOUT_MS,
                );
            }
            if let Some(defaults) = &profile.session_defaults {
                validate_optional_max(
                    self,
                    "invalid_history_window",
                    &format!("profiles[{index}].sessionDefaults.maxHistoryMessages"),
                    defaults.max_history_messages,
                    MAX_HISTORY_MESSAGES,
                );
                validate_optional_max(
                    self,
                    "invalid_turn_timeout",
                    &format!("profiles[{index}].sessionDefaults.turnTimeoutMs"),
                    defaults.turn_timeout_ms,
                    MAX_TURN_TIMEOUT_MS,
                );
            }
            if let Some(review) = &profile.background_review {
                if review.enabled {
                    if let Some(schedule) = &review.schedule {
                        validate_schedule(
                            self,
                            &format!("profiles[{index}].backgroundReview.schedule"),
                            schedule,
                        );
                    }
                }
            }
            if let Some(mcp) = &profile.mcp_config {
                if let Some(binding_id) = &mcp.binding_id {
                    validate_id_text(
                        self,
                        "invalid_binding_id",
                        &format!("profiles[{index}].mcpConfig.bindingId"),
                        binding_id,
                    );
                }
                if let Some(tool_profile) = &mcp.tool_profile {
                    validate_id_text(
                        self,
                        "invalid_tool_profile_key",
                        &format!("profiles[{index}].mcpConfig.toolProfile"),
                        tool_profile,
                    );
                }
                if let Some(endpoint_ref) = &mcp.endpoint_ref {
                    validate_non_empty(
                        self,
                        "invalid_endpoint_ref",
                        &format!("profiles[{index}].mcpConfig.endpointRef"),
                        endpoint_ref,
                    );
                }
                for (server_index, server) in mcp.server_names.iter().enumerate() {
                    validate_non_empty(
                        self,
                        "invalid_server_name",
                        &format!("profiles[{index}].mcpConfig.serverNames[{server_index}]"),
                        server,
                    );
                }
            }
        }
    }

    fn validate_brains(&mut self) {
        let mut implementation_ids = HashSet::new();
        for (index, brain) in self.draft.brains.iter().enumerate() {
            validate_id_text(
                self,
                "invalid_brain_implementation_id",
                &format!("brains[{index}].implementationId"),
                &brain.implementation_id.0,
            );
            validate_id_text(
                self,
                "invalid_profile_id",
                &format!("brains[{index}].profileId"),
                &brain.profile_id.0,
            );
            if !implementation_ids.insert(brain.implementation_id.clone()) {
                self.error(
                    "duplicate_brain_implementation_id",
                    format!("brains[{index}].implementationId"),
                    format!("duplicate brain implementation {}", brain.implementation_id),
                );
            }
            self.require_profile(
                &brain.profile_id,
                &format!("brains[{index}].profileId"),
                "brain",
            );
        }
    }

    fn validate_sessions(&mut self) {
        let mut agent_ids = HashSet::new();
        for (index, session) in self.draft.sessions.iter().enumerate() {
            validate_id_text(
                self,
                "invalid_session_id",
                &format!("sessions[{index}].sessionId"),
                &session.session_id.0,
            );
            validate_id_text(
                self,
                "invalid_agent_id",
                &format!("sessions[{index}].agentId"),
                &session.agent_id.0,
            );
            validate_id_text(
                self,
                "invalid_profile_id",
                &format!("sessions[{index}].profileId"),
                &session.profile_id.0,
            );
            if self
                .sessions_by_id
                .insert(session.session_id.clone(), session)
                .is_some()
            {
                self.error(
                    "duplicate_session_id",
                    format!("sessions[{index}].sessionId"),
                    format!("duplicate session {}", session.session_id),
                );
            }
            if !agent_ids.insert(session.agent_id.clone()) {
                self.error(
                    "duplicate_agent_id",
                    format!("sessions[{index}].agentId"),
                    format!("duplicate configured agent {}", session.agent_id),
                );
            }
            self.require_profile(
                &session.profile_id,
                &format!("sessions[{index}].profileId"),
                "session",
            );
            validate_resource_limits(
                self,
                &format!("sessions[{index}].resourceLimits"),
                session.resource_limits.as_ref(),
            );
            validate_history_window(
                self,
                &format!("sessions[{index}].historyWindow"),
                session.history_window.as_ref(),
            );
            validate_optional_max(
                self,
                "invalid_history_window",
                &format!("sessions[{index}].maxHistoryMessages"),
                session.max_history_messages,
                MAX_HISTORY_MESSAGES,
            );
            validate_optional_max(
                self,
                "invalid_turn_timeout",
                &format!("sessions[{index}].turnTimeoutMs"),
                session.turn_timeout_ms,
                MAX_TURN_TIMEOUT_MS,
            );
        }
    }

    fn validate_scheduled_jobs(&mut self) {
        let mut job_ids = HashSet::new();
        for (index, job) in self.draft.scheduled_jobs.iter().enumerate() {
            validate_id_text(
                self,
                "invalid_scheduled_job_id",
                &format!("scheduledJobs[{index}].id"),
                &job.id,
            );
            if !job_ids.insert(job.id.clone()) {
                self.error(
                    "duplicate_scheduled_job_id",
                    format!("scheduledJobs[{index}].id"),
                    format!("duplicate scheduled job {}", job.id),
                );
            }
            validate_schedule(
                self,
                &format!("scheduledJobs[{index}].schedule"),
                &job.schedule,
            );
            match job.shape {
                ScheduledJobShape::SessionWake => {
                    let Some(target_session_id) = &job.target_session_id else {
                        self.error(
                            "scheduled_job_missing_target_session",
                            format!("scheduledJobs[{index}].targetSessionId"),
                            "session_wake jobs require targetSessionId",
                        );
                        continue;
                    };
                    self.require_session(
                        target_session_id,
                        &format!("scheduledJobs[{index}].targetSessionId"),
                        "scheduled job",
                    );
                }
                ScheduledJobShape::HostJob => {
                    if job.job_kind.as_deref().is_none_or(str::is_empty) {
                        self.error(
                            "scheduled_job_missing_job_kind",
                            format!("scheduledJobs[{index}].jobKind"),
                            "host_job jobs require jobKind",
                        );
                    }
                }
                ScheduledJobShape::ScriptOnly | ScheduledJobShape::DataCollection => {
                    self.error(
                        "scheduled_job_not_executable",
                        format!("scheduledJobs[{index}].shape"),
                        format!(
                            "scheduled job shape {:?} is not executable by Rusty Crew v1",
                            job.shape
                        ),
                    );
                }
            }
        }
    }

    fn validate_channel_bindings(&mut self) {
        let mut binding_ids = HashSet::new();
        for (index, binding) in self.draft.channel_bindings.iter().enumerate() {
            validate_binding_common(
                self,
                BindingCommon {
                    family: "channelBindings",
                    index,
                    binding_id: &binding.binding_id,
                    adapter_id: &binding.adapter_id,
                    agent_id: &binding.agent_id,
                    session_id: binding.session_id.as_ref(),
                    profile_id: &binding.profile_id,
                },
            );
            if !binding_ids.insert(binding.binding_id.clone()) {
                self.error(
                    "duplicate_channel_binding_id",
                    format!("channelBindings[{index}].bindingId"),
                    format!("duplicate channel binding {}", binding.binding_id),
                );
            }
            validate_non_empty(
                self,
                "invalid_channel_provider",
                &format!("channelBindings[{index}].provider"),
                &binding.provider,
            );
            validate_non_empty(
                self,
                "invalid_external_channel_id",
                &format!("channelBindings[{index}].externalChannelId"),
                &binding.external_channel_id,
            );
        }
    }

    fn validate_mcp_bindings(&mut self) {
        let mut binding_ids = HashSet::new();
        for (index, binding) in self.draft.mcp_bindings.iter().enumerate() {
            validate_binding_common(
                self,
                BindingCommon {
                    family: "mcpBindings",
                    index,
                    binding_id: &binding.binding_id,
                    adapter_id: &binding.adapter_id,
                    agent_id: &binding.agent_id,
                    session_id: binding.session_id.as_ref(),
                    profile_id: &binding.profile_id,
                },
            );
            if !binding_ids.insert(binding.binding_id.clone()) {
                self.error(
                    "duplicate_mcp_binding_id",
                    format!("mcpBindings[{index}].bindingId"),
                    format!("duplicate MCP binding {}", binding.binding_id),
                );
            }
            if binding.server_names.is_empty() {
                self.error(
                    "mcp_binding_missing_server_names",
                    format!("mcpBindings[{index}].serverNames"),
                    "MCP bindings require at least one server name",
                );
            }
            for (server_index, server) in binding.server_names.iter().enumerate() {
                validate_non_empty(
                    self,
                    "invalid_server_name",
                    &format!("mcpBindings[{index}].serverNames[{server_index}]"),
                    server,
                );
            }
            validate_non_empty(
                self,
                "invalid_endpoint_ref",
                &format!("mcpBindings[{index}].endpointRef"),
                &binding.endpoint_ref,
            );
            validate_non_empty(
                self,
                "invalid_mcp_transport",
                &format!("mcpBindings[{index}].transport"),
                &binding.transport,
            );
            validate_id_text(
                self,
                "invalid_tool_profile_key",
                &format!("mcpBindings[{index}].toolProfileKey"),
                &binding.tool_profile_key,
            );
        }
    }

    fn require_profile(&mut self, profile_id: &ProfileId, path: &str, owner: &str) {
        if !self.profile_ids.contains(profile_id) {
            self.error(
                "missing_profile_metadata",
                path,
                format!("{owner} references profile {profile_id}, but metadata was not provided"),
            );
        }
    }

    fn require_session(&mut self, session_id: &SessionId, path: &str, owner: &str) {
        if !self.sessions_by_id.contains_key(session_id) {
            self.error(
                "missing_session",
                path,
                format!("{owner} references session {session_id}, but no session is configured"),
            );
        }
    }

    fn error(&mut self, code: &str, path: impl Into<String>, message: impl Into<String>) {
        self.diagnostics
            .push(RuntimeConfigDiagnostic::error(code, path, message));
    }
}

struct BindingCommon<'a> {
    family: &'a str,
    index: usize,
    binding_id: &'a str,
    adapter_id: &'a AdapterId,
    agent_id: &'a AgentId,
    session_id: Option<&'a SessionId>,
    profile_id: &'a ProfileId,
}

fn validate_binding_common(validator: &mut RuntimeConfigValidator<'_>, binding: BindingCommon<'_>) {
    let family = binding.family;
    let index = binding.index;
    validate_id_text(
        validator,
        "invalid_binding_id",
        &format!("{family}[{index}].bindingId"),
        binding.binding_id,
    );
    validate_id_text(
        validator,
        "invalid_adapter_id",
        &format!("{family}[{index}].adapterId"),
        &binding.adapter_id.0,
    );
    validate_id_text(
        validator,
        "invalid_agent_id",
        &format!("{family}[{index}].agentId"),
        &binding.agent_id.0,
    );
    validate_id_text(
        validator,
        "invalid_profile_id",
        &format!("{family}[{index}].profileId"),
        &binding.profile_id.0,
    );
    validator.require_profile(
        binding.profile_id,
        &format!("{family}[{index}].profileId"),
        family,
    );

    let Some(session_id) = binding.session_id else {
        let matches = validator
            .draft
            .sessions
            .iter()
            .filter(|session| {
                session.agent_id == *binding.agent_id && session.profile_id == *binding.profile_id
            })
            .count();
        if matches == 0 {
            validator.error(
                "binding_target_missing",
                format!("{family}[{index}].sessionId"),
                "binding without sessionId must match exactly one configured session by agentId/profileId, but none matched",
            );
        } else if matches > 1 {
            validator.error(
                "binding_target_ambiguous",
                format!("{family}[{index}].sessionId"),
                "binding without sessionId must match exactly one configured session by agentId/profileId, but multiple matched",
            );
        }
        return;
    };

    validate_id_text(
        validator,
        "invalid_session_id",
        &format!("{family}[{index}].sessionId"),
        &session_id.0,
    );
    let Some(session) = validator.sessions_by_id.get(session_id) else {
        validator.error(
            "missing_session",
            format!("{family}[{index}].sessionId"),
            format!("binding references session {session_id}, but no session is configured"),
        );
        return;
    };
    if session.agent_id != *binding.agent_id || session.profile_id != *binding.profile_id {
        validator.error(
            "binding_session_mismatch",
            format!("{family}[{index}].sessionId"),
            format!(
                "binding target session {session_id} has agent/profile {}/{}, but binding has {}/{}",
                session.agent_id, session.profile_id, binding.agent_id, binding.profile_id
            ),
        );
    }
}

fn validate_resource_limits(
    validator: &mut RuntimeConfigValidator<'_>,
    path: &str,
    limits: Option<&ResourceLimits>,
) {
    let Some(limits) = limits else { return };
    if limits.workdir.as_deref().is_some_and(str::is_empty) {
        validator.error(
            "invalid_resource_limits",
            format!("{path}.workdir"),
            "workdir must not be empty when provided",
        );
    }
    validate_optional_max(
        validator,
        "invalid_resource_limits",
        &format!("{path}.maxDurationMs"),
        limits.max_duration_ms,
        MAX_DURATION_MS,
    );
    validate_optional_max(
        validator,
        "invalid_resource_limits",
        &format!("{path}.maxDelegationDepth"),
        limits.max_delegation_depth,
        MAX_DELEGATION_DEPTH,
    );
}

fn validate_history_window(
    validator: &mut RuntimeConfigValidator<'_>,
    path: &str,
    history_window: Option<&SessionHistoryWindow>,
) {
    let Some(history_window) = history_window else {
        return;
    };
    validate_optional_max(
        validator,
        "invalid_history_window",
        &format!("{path}.maxMessages"),
        history_window.max_messages,
        MAX_HISTORY_MESSAGES,
    );
}

fn validate_optional_max(
    validator: &mut RuntimeConfigValidator<'_>,
    code: &str,
    path: &str,
    value: Option<u32>,
    max: u32,
) {
    if let Some(value) = value {
        if value > max {
            validator.error(
                code,
                path,
                format!("value {value} exceeds maximum allowed value {max}"),
            );
        }
    }
}

fn validate_non_empty(
    validator: &mut RuntimeConfigValidator<'_>,
    code: &str,
    path: &str,
    value: &str,
) {
    if value.trim().is_empty() {
        validator.error(code, path, "value must not be empty");
    }
}

fn validate_id_text(
    validator: &mut RuntimeConfigValidator<'_>,
    code: &str,
    path: &str,
    value: &str,
) {
    if !is_valid_component_id(value) {
        validator.error(code, path, format!("{path} {ID_PATTERN_DESCRIPTION}"));
    }
}

fn validate_schedule(validator: &mut RuntimeConfigValidator<'_>, path: &str, schedule: &str) {
    validate_non_empty(validator, "invalid_schedule", path, schedule);
    if !looks_like_cron(schedule) {
        validator.error(
            "invalid_schedule",
            path,
            "schedule must be a five-field cron expression",
        );
    }
}

fn is_valid_component_id(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphanumeric() {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | ':' | '-'))
}

fn looks_like_cron(schedule: &str) -> bool {
    let fields: Vec<&str> = schedule.split_whitespace().collect();
    fields.len() == 5 && fields.iter().all(|field| !field.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_a_runtime_config_graph() {
        let result = validate_runtime_config_draft(&valid_draft(), &[profile("runner")]);
        assert!(result.ok(), "{:?}", result.diagnostics);
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn reports_duplicate_ids() {
        let mut draft = valid_draft();
        draft.sessions.push(draft.sessions[0].clone());
        draft.brains.push(draft.brains[0].clone());
        draft.scheduled_jobs.push(draft.scheduled_jobs[0].clone());
        draft
            .channel_bindings
            .push(draft.channel_bindings[0].clone());
        draft.mcp_bindings.push(draft.mcp_bindings[0].clone());

        let result = validate_runtime_config_draft(&draft, &[profile("runner")]);
        assert_codes(
            &result,
            &[
                "duplicate_session_id",
                "duplicate_brain_implementation_id",
                "duplicate_scheduled_job_id",
                "duplicate_channel_binding_id",
                "duplicate_mcp_binding_id",
            ],
        );
    }

    #[test]
    fn reports_missing_profile_metadata() {
        let result = validate_runtime_config_draft(&valid_draft(), &[]);
        assert_codes(&result, &["missing_profile_metadata"]);
    }

    #[test]
    fn reports_binding_session_mismatch() {
        let mut draft = valid_draft();
        draft.channel_bindings[0].agent_id = AgentId::new("other-agent");
        draft.mcp_bindings[0].profile_id = ProfileId::new("other-profile");

        let result =
            validate_runtime_config_draft(&draft, &[profile("runner"), profile("other-profile")]);
        assert_codes(
            &result,
            &["binding_session_mismatch", "binding_session_mismatch"],
        );
    }

    #[test]
    fn reports_non_executable_scheduled_job_shape() {
        let mut draft = valid_draft();
        draft.scheduled_jobs.push(ScheduledJobConfigDraft {
            id: "script-job".to_string(),
            schedule: "0 1 * * *".to_string(),
            shape: ScheduledJobShape::ScriptOnly,
            job_kind: None,
            target_session_id: None,
            script: Some("echo hi".to_string()),
            delivery_channel_id: None,
        });

        let result = validate_runtime_config_draft(&draft, &[profile("runner")]);
        assert_codes(&result, &["scheduled_job_not_executable"]);
    }

    #[test]
    fn reports_invalid_ids_and_values() {
        let mut draft = valid_draft();
        draft.sessions[0].session_id = SessionId::new(" bad");
        draft.sessions[0].resource_limits = Some(ResourceLimits {
            workdir: Some(String::new()),
            max_duration_ms: Some(MAX_DURATION_MS + 1),
            max_delegation_depth: Some(MAX_DELEGATION_DEPTH + 1),
        });
        draft.scheduled_jobs[0].schedule = "not a cron".to_string();

        let result = validate_runtime_config_draft(&draft, &[profile("runner")]);
        assert_codes(
            &result,
            &[
                "invalid_session_id",
                "invalid_resource_limits",
                "invalid_resource_limits",
                "invalid_resource_limits",
                "invalid_schedule",
            ],
        );
    }

    #[test]
    fn serializes_validation_diagnostics_as_structured_data() {
        let mut draft = valid_draft();
        draft.scheduled_jobs[0].target_session_id = Some(SessionId::new("missing-session"));
        let result = validate_runtime_config_draft(&draft, &[profile("runner")]);

        let json = serde_json::to_value(&result).expect("validation result should serialize");
        assert_eq!(
            json["diagnostics"][0]["severity"],
            serde_json::json!("error")
        );
        assert_eq!(json["diagnostics"][0]["code"], "missing_session");
        assert_eq!(
            json["diagnostics"][0]["path"],
            "scheduledJobs[0].targetSessionId"
        );
    }

    fn valid_draft() -> RuntimeConfigDraft {
        RuntimeConfigDraft {
            profiles_dir: "/tmp/rusty-crew/profiles".to_string(),
            skills_dir: Some("/tmp/rusty-crew/skills".to_string()),
            brains: vec![BrainConfigDraft {
                implementation_id: BrainImplementationId::new("runner-brain"),
                profile_id: ProfileId::new("runner"),
            }],
            sessions: vec![SessionConfigDraft {
                session_id: SessionId::new("runner-session"),
                agent_id: AgentId::new("runner-agent"),
                profile_id: ProfileId::new("runner"),
                kind: SessionKind::Full,
                resource_limits: Some(ResourceLimits {
                    workdir: Some("/tmp/rusty-crew/work".to_string()),
                    max_duration_ms: Some(60_000),
                    max_delegation_depth: Some(4),
                }),
                owner_id: Some("owner".to_string()),
                history_window: Some(SessionHistoryWindow {
                    max_messages: Some(200),
                }),
                max_history_messages: None,
                turn_timeout_ms: Some(30_000),
            }],
            scheduled_jobs: vec![
                ScheduledJobConfigDraft {
                    id: "runner-wake".to_string(),
                    schedule: "*/5 * * * *".to_string(),
                    shape: ScheduledJobShape::SessionWake,
                    job_kind: None,
                    target_session_id: Some(SessionId::new("runner-session")),
                    script: None,
                    delivery_channel_id: None,
                },
                ScheduledJobConfigDraft {
                    id: "runner-background-review".to_string(),
                    schedule: "0 3 * * *".to_string(),
                    shape: ScheduledJobShape::HostJob,
                    job_kind: Some("runtime_review_memory_skills".to_string()),
                    target_session_id: None,
                    script: None,
                    delivery_channel_id: None,
                },
            ],
            channel_bindings: vec![ChannelBindingConfigDraft {
                binding_id: "runner-channel".to_string(),
                adapter_id: AdapterId::new("den-gateway"),
                provider: "den_conversation".to_string(),
                agent_id: AgentId::new("runner-agent"),
                instance_id: None,
                session_id: Some(SessionId::new("runner-session")),
                profile_id: ProfileId::new("runner"),
                external_channel_id: "40".to_string(),
                external_thread_id: None,
                external_user_id: None,
                conversation_project_id: Some("rusty-crew".to_string()),
                conversation_channel_id: Some(40),
                provider_subscription_id: None,
                status: ExternalBindingStatusDraft::Active,
            }],
            mcp_bindings: vec![McpBindingConfigDraft {
                binding_id: "runner-mcp".to_string(),
                adapter_id: AdapterId::new("mcp-ts-main"),
                agent_id: AgentId::new("runner-agent"),
                instance_id: None,
                session_id: Some(SessionId::new("runner-session")),
                profile_id: ProfileId::new("runner"),
                server_names: vec!["den".to_string()],
                endpoint_ref: "http://127.0.0.1:5199/mcp?tool_profile=runner".to_string(),
                transport: "streamable_http".to_string(),
                tool_profile_key: "runner".to_string(),
                status: ExternalBindingStatusDraft::Active,
            }],
        }
    }

    fn profile(profile_id: &str) -> ProfileRuntimeMetadata {
        ProfileRuntimeMetadata {
            profile_id: ProfileId::new(profile_id),
            brain: Some(ProfileBrainMetadata {
                module: Some("local".to_string()),
                strategy: None,
            }),
            runtime: Some(ProfileRuntimeOptions {
                default_resource_limits: None,
                max_turn_duration_ms: Some(60_000),
                max_tokens_per_turn: Some(8_000),
            }),
            session_defaults: Some(ProfileSessionDefaults {
                owner_id: Some("owner".to_string()),
                max_history_messages: Some(500),
                turn_timeout_ms: Some(30_000),
            }),
            mcp_config: Some(ProfileMcpConfig {
                binding_id: Some(format!("{profile_id}-mcp")),
                endpoint_ref: Some("config://mcp/runner".to_string()),
                server_names: vec!["den".to_string()],
                transport: Some("streamable_http".to_string()),
                tool_profile: Some(profile_id.to_string()),
            }),
            background_review: Some(ProfileBackgroundReviewConfig {
                enabled: true,
                review_type: Some(ProfileBackgroundReviewType::Combined),
                schedule: Some("0 3 * * *".to_string()),
            }),
            channel_defaults: Some(ProfileChannelDefaults {
                wake_policy: Some(ChannelWakePolicy::Subscription),
            }),
        }
    }

    fn assert_codes(result: &RuntimeConfigValidationResult, expected: &[&str]) {
        let mut actual: Vec<&str> = result
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();
        for code in expected {
            let Some(index) = actual.iter().position(|actual| actual == code) else {
                panic!("missing diagnostic code {code}; actual={actual:?}");
            };
            actual.remove(index);
        }
    }
}
