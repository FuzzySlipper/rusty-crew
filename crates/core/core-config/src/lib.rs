//! Pure runtime/profile config validation for Rusty Crew service control-plane data.
//!
//! This crate validates draft config graphs before TypeScript writes files or
//! applies changes to the engine. It deliberately does not load profile files,
//! render prompts, discover tools, call providers, or mutate runtime state.

use rusty_crew_core_protocol::{
    AdapterId, AgentId, AgentInstanceId, BrainImplementationId, DelegatedWorkspaceConstraint,
    ExternalMessageDeliveryPolicy, IsoTimestamp, ProfileId, ProfileRegistryDerivedRuntimeRef,
    ProfileRegistryImportExportMetadata, ProfileRegistryLifecycleStatus, ProfileRegistryRecord,
    ProfileRegistrySourceAssetRef, ProfileRegistryWrite, ResourceLimits, SessionHistoryWindow,
    SessionId, SessionKind, TaskId, MAX_RESOURCE_DELEGATION_DEPTH, MAX_RESOURCE_DURATION_MS,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::Path;

mod runtime_graph;

pub use runtime_graph::*;

const MAX_HISTORY_MESSAGES: u32 = 10_000;
pub const DEFAULT_POSTGRES_SCHEMA: &str = "rusty_crew";
const ID_PATTERN_DESCRIPTION: &str =
    "must start with a letter or digit and contain only letters, digits, '.', '_', ':' or '-'";
const RUNTIME_REVIEW_MEMORY_SKILLS_JOB_KIND: &str = "runtime.review.memory_skills";
const CONTEXT_STRATEGY_IDS: &[&str] = &[
    "recent_window",
    "session_memory_augmented",
    "rolling_summary_compaction",
    "roleplay_scene_aware_compaction",
];
const CONTEXT_DEBUG_VISIBILITY_VALUES: &[&str] = &["off", "status", "verbose"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ClockConfig {
    System,
    Fixed { at: IsoTimestamp },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EngineConfig {
    pub engine_data_dir: String,
    pub clock: ClockConfig,
    pub default_turn_budget: u32,
    pub default_idle_timeout_ms: u32,
    pub storage: Option<EngineStorageConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "backend", rename_all = "snake_case")]
pub enum EngineStorageConfig {
    Sqlite {
        filesystem_warning_free_percent: Option<u32>,
    },
    Postgres {
        database_url: String,
        schema: String,
        max_connections: Option<u32>,
        statement_timeout_ms: Option<u32>,
        backing_filesystem_path: Option<String>,
        filesystem_warning_free_percent: Option<u32>,
    },
}

impl EngineStorageConfig {
    pub fn postgres_with_defaults(
        database_url: impl Into<String>,
        schema: Option<String>,
        max_connections: Option<u32>,
        statement_timeout_ms: Option<u32>,
    ) -> Self {
        let schema = schema
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_POSTGRES_SCHEMA.to_string());
        Self::Postgres {
            database_url: database_url.into(),
            schema,
            max_connections,
            statement_timeout_ms,
            backing_filesystem_path: None,
            filesystem_warning_free_percent: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeConfigValidationInput {
    pub runtime_config: RuntimeConfigDraft,
    #[serde(default)]
    pub profiles: Vec<ProfileRuntimeMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CreateProfilePlanInput {
    pub runtime_config: RuntimeConfigDraft,
    #[serde(default)]
    pub profiles: Vec<ProfileRuntimeMetadata>,
    #[serde(default)]
    pub profile_registry: Vec<ProfileRegistryRuntimeMetadata>,
    pub request: CreateProfileRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NewSessionControlPlanInput {
    pub command: AdminControlPlanCommand,
    pub template: Option<NewSessionControlTemplate>,
    pub generated_session_id: Option<String>,
    #[serde(default)]
    pub rebind_handler_available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReloadMcpControlPlanInput {
    pub command: AdminControlPlanCommand,
    pub binding: Option<ReloadMcpControlBinding>,
    #[serde(default)]
    pub reload_handler_available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DelegatedRoleLifecyclePlanInput {
    pub parent_session: DelegatedRoleParentSession,
    pub delegated_session_id: String,
    pub delegated_agent_id: String,
    pub profile_id: ProfileId,
    pub tool_profile_key: Option<String>,
    pub requested_resource_limits: Option<ResourceLimits>,
    pub requested_workspace_constraint: Option<DelegatedWorkspaceConstraint>,
    pub source_wake_id: String,
    pub source_action_index: u32,
    pub task_id: Option<TaskId>,
    pub correlation_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DelegatedRoleParentSession {
    pub session_id: SessionId,
    pub agent_id: AgentId,
    pub kind: SessionKind,
    pub resource_limits: Option<ResourceLimits>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DelegatedRoleLifecyclePlan {
    pub accepted: bool,
    pub reason_code: String,
    pub diagnostics: Vec<RuntimeConfigDiagnostic>,
    pub session_id: SessionId,
    pub agent_id: AgentId,
    pub parent_session_id: SessionId,
    pub parent_agent_id: AgentId,
    pub profile_id: ProfileId,
    pub kind: SessionKind,
    pub resource_limits: ResourceLimits,
    pub workspace_constraint: Option<DelegatedWorkspaceConstraint>,
    pub tool_profile_key: Option<String>,
    pub source_wake_id: String,
    pub source_action_index: u32,
    pub task_id: Option<TaskId>,
    pub correlation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ChannelIngressRoutePlanInput {
    pub message: ChannelIngressRouteMessage,
    #[serde(default)]
    pub bindings: Vec<ChannelBindingConfigDraft>,
    #[serde(default)]
    pub mention_aliases: HashMap<String, AgentId>,
    pub system_agent_id: Option<AgentId>,
    pub now: Option<String>,
    #[serde(default)]
    pub seen_idempotency_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ChannelIngressRouteMessage {
    pub adapter_id: AdapterId,
    pub binding_id: String,
    pub provider: String,
    pub external_channel_id: String,
    pub external_thread_id: Option<String>,
    pub external_user_id: String,
    pub body: String,
    #[serde(default)]
    pub mentions: Vec<String>,
    pub expires_at: String,
    pub idempotency_key: String,
    pub runtime_agent_id: Option<AgentId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ChannelIngressRoutePlan {
    pub status: ChannelIngressRouteDecision,
    pub reason_code: String,
    pub reason: String,
    pub correlation_id: Option<String>,
    pub binding: Option<ChannelBindingConfigDraft>,
    #[serde(default)]
    pub candidates: Vec<ChannelBindingConfigDraft>,
    pub route: Option<ChannelIngressRouteRequest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChannelIngressRouteDecision {
    Routed,
    NoBinding,
    InactiveBinding,
    Ambiguous,
    Expired,
    Duplicate,
    Denied,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ChannelIngressRouteRequest {
    pub from: AgentId,
    pub to: AgentId,
    pub body: String,
    pub correlation_id: String,
    pub binding_id: String,
    pub session_id: Option<SessionId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DenProductIngressPolicyInput {
    pub operation: String,
    pub entity_kind: String,
    pub entity_id: String,
    pub project_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DenProductIngressPolicyPlan {
    pub status: DenProductIngressPolicyStatus,
    pub operation: String,
    pub reason_code: String,
    pub reason: String,
    pub lifecycle_operation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DenProductIngressPolicyStatus {
    Allowed,
    Denied,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AdminControlPlanCommand {
    pub command_kind: String,
    pub target_session_id: Option<String>,
    pub request_id: Option<String>,
    pub idempotency_key: Option<String>,
    pub operator_reason: Option<String>,
    pub operator_reason_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReloadMcpControlBinding {
    pub binding_id: String,
    pub session_id: String,
    pub profile_id: ProfileId,
    pub tool_profile_key: Option<String>,
    pub endpoint_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NewSessionControlTemplate {
    pub agent_id: AgentId,
    pub profile_id: ProfileId,
    pub kind: SessionKind,
    pub workspace_cwd: Option<String>,
    pub channel_binding_id: Option<String>,
    pub channel_id: Option<String>,
    pub tool_profile_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NewSessionControlPlan {
    pub accepted: bool,
    pub command_kind: String,
    pub target: NewSessionControlTarget,
    pub idempotency_key: Option<String>,
    pub operator_reason: String,
    pub reason_code: String,
    pub denial: Option<AdminControlPlanDenial>,
    #[serde(default)]
    pub preconditions: Vec<AdminControlPlanPrecondition>,
    #[serde(default)]
    pub actions: Vec<NewSessionControlAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReloadMcpControlPlan {
    pub accepted: bool,
    pub command_kind: String,
    pub target: ReloadMcpControlTarget,
    pub idempotency_key: Option<String>,
    pub operator_reason: String,
    pub reason_code: String,
    pub denial: Option<AdminControlPlanDenial>,
    #[serde(default)]
    pub preconditions: Vec<AdminControlPlanPrecondition>,
    #[serde(default)]
    pub actions: Vec<ReloadMcpControlAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NewSessionControlTarget {
    pub old_session_id: Option<String>,
    pub new_session_id: Option<String>,
    pub agent_id: Option<AgentId>,
    pub profile_id: Option<ProfileId>,
    pub channel_binding_id: Option<String>,
    pub channel_id: Option<String>,
    pub tool_profile_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReloadMcpControlTarget {
    pub session_id: Option<String>,
    pub binding_id: Option<String>,
    pub profile_id: Option<ProfileId>,
    pub tool_profile_key: Option<String>,
    pub endpoint_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AdminControlPlanDenial {
    pub reason_code: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AdminControlPlanPrecondition {
    pub code: String,
    pub status: AdminControlPlanPreconditionStatus,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AdminControlPlanPreconditionStatus {
    Satisfied,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NewSessionControlAction {
    pub action: NewSessionControlActionKind,
    pub session_id: Option<String>,
    pub old_session_id: Option<String>,
    pub new_session_id: Option<String>,
    pub reason_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NewSessionControlActionKind {
    ArchiveSession,
    CreateSession,
    RebindChannel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReloadMcpControlAction {
    pub action: ReloadMcpControlActionKind,
    pub session_id: String,
    pub binding_id: String,
    pub reason_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReloadMcpControlActionKind {
    ReloadMcpSurface,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CreateProfileRequest {
    pub profile_id: String,
    pub display_name: Option<String>,
    pub soul_markdown: Option<String>,
    pub memory_markdown: Option<String>,
    pub agent_id: Option<String>,
    pub session_id: Option<String>,
    pub implementation_id: Option<String>,
    pub kind: Option<SessionKind>,
    #[serde(default)]
    pub workspace_cwd: Option<String>,
    #[serde(default)]
    pub model_config_id: Option<String>,
    /// Compatibility-only legacy selector. New writes emit model_config_id.
    pub provider_alias: Option<String>,
    #[serde(default)]
    pub external_message_delivery_policy: Option<ExternalMessageDeliveryPolicy>,
    pub model_config: Option<ProfileModelConfigSeed>,
    pub brain: Option<ProfileBrainMetadata>,
    #[serde(default)]
    pub mcp_bindings: Vec<CreateProfileMcpBindingRequest>,
    pub mcp_tool_profile: Option<String>,
    pub source: Option<CreateProfileSourceRequest>,
    pub now: Option<String>,
    #[serde(default)]
    pub profile_file_exists: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CreateProfileMcpBindingRequest {
    pub server_id: String,
    pub binding_id: Option<String>,
    pub adapter_id: Option<String>,
    pub server_names: Option<Vec<String>>,
    pub transport: Option<String>,
    pub tool_profile_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProfileRegistryRuntimeMetadata {
    pub profile_id: ProfileId,
    pub lifecycle_status: Option<ProfileRegistryLifecycleStatus>,
    pub revision: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CreateProfileSourceRequest {
    pub template_id: Option<String>,
    pub source_profile_id: Option<ProfileId>,
    pub source_bundle_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProfileModelConfigSeed {
    pub provider: String,
    pub model_name: String,
    pub base_url: Option<String>,
    pub api: Option<String>,
    pub api_key_env: Option<String>,
    pub temperature_milli: Option<u32>,
    pub max_output_tokens: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CreateProfilePlan {
    pub diagnostics: Vec<RuntimeConfigDiagnostic>,
    pub registry_write: Option<ProfileRegistryWrite>,
    #[serde(default)]
    pub file_asset_actions: Vec<CreateProfileFileAssetAction>,
    #[serde(default)]
    pub derived_runtime_actions: Vec<CreateProfileDerivedRuntimeAction>,
    pub profile_seed: Option<CreateProfileSeedMetadata>,
    pub runtime_brain: Option<BrainConfigDraft>,
    pub runtime_session: Option<SessionConfigDraft>,
    pub profile_mcp_config: Option<ProfileMcpConfig>,
    #[serde(default)]
    pub runtime_mcp_bindings: Vec<McpBindingConfigDraft>,
}

impl CreateProfilePlan {
    pub fn ok(&self) -> bool {
        !self
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == RuntimeConfigDiagnosticSeverity::Error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProfileRegistryMutationKind {
    Update,
    Lifecycle,
    Prompt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProfileRegistryMutationMode {
    Plan,
    Apply,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProfileRegistryMutationRequest {
    pub profile_id: ProfileId,
    pub kind: ProfileRegistryMutationKind,
    pub mode: ProfileRegistryMutationMode,
    pub current: ProfileRegistryRecord,
    pub body_json: Value,
    pub now: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProfileRegistryMutationImplications {
    pub registry_revision_will_increment: bool,
    pub profile_files_unchanged: bool,
    pub service_config_unchanged: bool,
    pub runtime_rebuild_recommended: bool,
    pub lifecycle_effects: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProfileRegistryMutationPlan {
    pub ok: bool,
    pub profile_id: ProfileId,
    pub kind: ProfileRegistryMutationKind,
    pub mode: ProfileRegistryMutationMode,
    pub expected_revision: u64,
    pub current: ProfileRegistryRecord,
    pub next: ProfileRegistryRecord,
    pub next_write: ProfileRegistryWrite,
    pub diagnostics: Vec<RuntimeConfigDiagnostic>,
    pub implications: ProfileRegistryMutationImplications,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CreateProfileSeedMetadata {
    pub profile_id: ProfileId,
    pub display_name: Option<String>,
    pub model_config_id: String,
    pub provider_alias: Option<String>,
    pub model_config: ProfileModelConfigSeed,
    pub brain: ProfileBrainMetadata,
    pub external_message_delivery_policy: ExternalMessageDeliveryPolicy,
    pub skills_mode: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreateProfileFileAssetActionKind {
    WriteProfileJson,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CreateProfileFileAssetAction {
    pub kind: CreateProfileFileAssetActionKind,
    pub profile_id: ProfileId,
    pub relative_path: String,
    pub overwrite: bool,
    pub metadata_json: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreateProfileDerivedRuntimeActionKind {
    AddBrain,
    AddSession,
    AddProfileMcpConfig,
    AddMcpBinding,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CreateProfileDerivedRuntimeAction {
    pub kind: CreateProfileDerivedRuntimeActionKind,
    pub ref_kind: String,
    pub ref_id: String,
    pub apply_phase: String,
    pub metadata_json: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BrainConfigDraft {
    pub implementation_id: BrainImplementationId,
    pub profile_id: ProfileId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SessionConfigDraft {
    pub session_id: SessionId,
    pub agent_id: AgentId,
    pub profile_id: ProfileId,
    pub kind: SessionKind,
    pub workspace_cwd: Option<String>,
    pub resource_limits: Option<ResourceLimits>,
    pub owner_id: Option<String>,
    pub history_window: Option<SessionHistoryWindow>,
    pub max_history_messages: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScheduledJobShape {
    HostJob,
    SessionWake,
    ScriptOnly,
    DataCollection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ScheduledJobConfigDraft {
    pub id: String,
    pub schedule: String,
    pub shape: ScheduledJobShape,
    pub job_kind: Option<String>,
    pub target_session_id: Option<SessionId>,
    pub script: Option<String>,
    pub delivery_channel_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExternalBindingStatusDraft {
    Active,
    Degraded,
    Disconnected,
    Archived,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProfileRuntimeMetadata {
    pub profile_id: ProfileId,
    pub brain: Option<ProfileBrainMetadata>,
    pub runtime: Option<ProfileRuntimeOptions>,
    pub session_defaults: Option<ProfileSessionDefaults>,
    pub mcp_config: Option<ProfileMcpConfig>,
    pub background_review: Option<ProfileBackgroundReviewConfig>,
    pub channel_defaults: Option<ProfileChannelDefaults>,
    pub context_policy: Option<ProfileContextPolicy>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProfileBrainMetadata {
    pub module: Option<String>,
    pub strategy: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProfileRuntimeOptions {
    pub default_resource_limits: Option<ResourceLimits>,
    pub max_tokens_per_turn: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProfileSessionDefaults {
    pub owner_id: Option<String>,
    pub max_history_messages: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProfileMcpConfig {
    pub binding_id: Option<String>,
    pub endpoint_ref: Option<String>,
    pub server_names: Vec<String>,
    pub transport: Option<String>,
    pub tool_profile: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProfileBackgroundReviewType {
    Memory,
    Skills,
    Combined,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProfileBackgroundReviewConfig {
    pub enabled: bool,
    pub review_type: Option<ProfileBackgroundReviewType>,
    pub schedule: Option<String>,
    pub memory_nudge_interval: Option<u32>,
    pub skill_nudge_interval: Option<u32>,
    pub max_tokens: Option<u32>,
    pub max_findings: Option<u32>,
    pub max_candidates: Option<u32>,
    pub llm_review_enabled: Option<bool>,
    pub capture_model_config_id: Option<String>,
    pub capture_provider_alias: Option<String>,
    pub capture_max_proposals: Option<u32>,
    pub dry_run: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChannelWakePolicy {
    Subscription,
    Manual,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProfileChannelDefaults {
    pub wake_policy: Option<ChannelWakePolicy>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProfileContextPolicy {
    pub enabled: bool,
    pub strategy_id: String,
    pub auto_compaction_enabled: bool,
    pub compact_at_percent: u32,
    pub target_percent_after_compaction: u32,
    pub max_context_percent_for_wake: u32,
    pub debug_visibility: String,
    pub include_debug_events_in_model_context: bool,
    #[serde(default)]
    pub strategy_config: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeConfigDiagnosticSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

pub fn validate_engine_config(config: &EngineConfig) -> RuntimeConfigValidationResult {
    let mut diagnostics = Vec::new();
    if config.engine_data_dir.trim().is_empty() {
        diagnostics.push(RuntimeConfigDiagnostic::error(
            "engine_data_dir_required",
            "engineDataDir",
            "engineDataDir must not be empty",
        ));
    }
    if let ClockConfig::Fixed { at } = &config.clock {
        if at.trim().is_empty() {
            diagnostics.push(RuntimeConfigDiagnostic::error(
                "fixed_clock_required",
                "clock.fixed",
                "fixed clock timestamp must not be empty",
            ));
        }
    }
    if config.default_turn_budget == 0 {
        diagnostics.push(RuntimeConfigDiagnostic::error(
            "default_turn_budget_required",
            "defaultTurnBudget",
            "defaultTurnBudget must be greater than zero",
        ));
    }
    if config.default_idle_timeout_ms == 0 {
        diagnostics.push(RuntimeConfigDiagnostic::error(
            "default_idle_timeout_required",
            "defaultIdleTimeoutMs",
            "defaultIdleTimeoutMs must be greater than zero",
        ));
    }
    if let Some(storage) = &config.storage {
        validate_engine_storage_config(storage, &mut diagnostics);
    }
    RuntimeConfigValidationResult { diagnostics }
}

fn validate_engine_storage_config(
    storage: &EngineStorageConfig,
    diagnostics: &mut Vec<RuntimeConfigDiagnostic>,
) {
    match storage {
        EngineStorageConfig::Sqlite {
            filesystem_warning_free_percent,
        } => validate_filesystem_warning_percent(
            *filesystem_warning_free_percent,
            "storage.filesystemWarningFreePercent",
            diagnostics,
        ),
        EngineStorageConfig::Postgres {
            database_url,
            schema,
            max_connections,
            statement_timeout_ms,
            backing_filesystem_path,
            filesystem_warning_free_percent,
        } => {
            if database_url.trim().is_empty() {
                diagnostics.push(RuntimeConfigDiagnostic::error(
                    "postgres_database_url_required",
                    "storage.databaseUrl",
                    "Postgres storage requires a non-empty databaseUrl",
                ));
            }
            if schema.trim().is_empty() {
                diagnostics.push(RuntimeConfigDiagnostic::error(
                    "postgres_schema_required",
                    "storage.schema",
                    "Postgres storage schema must not be empty",
                ));
            }
            if matches!(max_connections, Some(0)) {
                diagnostics.push(RuntimeConfigDiagnostic::error(
                    "postgres_max_connections_invalid",
                    "storage.maxConnections",
                    "Postgres maxConnections must be greater than zero when provided",
                ));
            }
            if matches!(statement_timeout_ms, Some(0)) {
                diagnostics.push(RuntimeConfigDiagnostic::error(
                    "postgres_statement_timeout_invalid",
                    "storage.statementTimeoutMs",
                    "Postgres statementTimeoutMs must be greater than zero when provided",
                ));
            }
            if backing_filesystem_path
                .as_ref()
                .is_some_and(|path| path.trim().is_empty())
            {
                diagnostics.push(RuntimeConfigDiagnostic::error(
                    "postgres_backing_filesystem_path_invalid",
                    "storage.backingFilesystemPath",
                    "Postgres backingFilesystemPath must be omitted or non-empty",
                ));
            }
            validate_filesystem_warning_percent(
                *filesystem_warning_free_percent,
                "storage.filesystemWarningFreePercent",
                diagnostics,
            );
        }
    }
}

fn validate_filesystem_warning_percent(
    value: Option<u32>,
    path: &str,
    diagnostics: &mut Vec<RuntimeConfigDiagnostic>,
) {
    if value.is_some_and(|percent| percent > 100) {
        diagnostics.push(RuntimeConfigDiagnostic::error(
            "filesystem_warning_free_percent_invalid",
            path,
            "filesystem warning free percent must be between 0 and 100",
        ));
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeConfigPlan {
    pub runtime_config: RuntimeConfigDraft,
    pub diagnostics: Vec<RuntimeConfigDiagnostic>,
    pub derived_scheduled_jobs: Vec<ScheduledJobConfigDraft>,
    pub derived_mcp_bindings: Vec<McpBindingConfigDraft>,
}

impl RuntimeConfigPlan {
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

pub fn validate_runtime_config_input(
    input: &RuntimeConfigValidationInput,
) -> RuntimeConfigValidationResult {
    validate_runtime_config_draft(&input.runtime_config, &input.profiles)
}

pub fn plan_runtime_config(input: &RuntimeConfigValidationInput) -> RuntimeConfigPlan {
    let mut runtime_config = input.runtime_config.clone();
    let profiles_by_id: HashMap<ProfileId, &ProfileRuntimeMetadata> = input
        .profiles
        .iter()
        .map(|profile| (profile.profile_id.clone(), profile))
        .collect();
    let mut derived_scheduled_jobs = Vec::new();
    let mut derived_mcp_bindings = Vec::new();
    let mut scheduled_job_ids: HashSet<String> = runtime_config
        .scheduled_jobs
        .iter()
        .map(|job| job.id.clone())
        .collect();
    let mut profiles_with_review_jobs = HashSet::new();

    for session in &mut runtime_config.sessions {
        let Some(profile) = profiles_by_id.get(&session.profile_id) else {
            continue;
        };
        apply_profile_session_defaults(session, profile);

        if let Some(job) = derive_background_review_job(
            profile,
            &mut scheduled_job_ids,
            &mut profiles_with_review_jobs,
        ) {
            runtime_config.scheduled_jobs.push(job.clone());
            derived_scheduled_jobs.push(job);
        }

        if let Some(binding) =
            derive_profile_mcp_binding(&runtime_config.mcp_bindings, session, profile)
        {
            runtime_config.mcp_bindings.push(binding.clone());
            derived_mcp_bindings.push(binding);
        }
    }

    resolve_unique_binding_targets(&mut runtime_config);

    let diagnostics = validate_runtime_config_draft(&runtime_config, &input.profiles).diagnostics;
    RuntimeConfigPlan {
        runtime_config,
        diagnostics,
        derived_scheduled_jobs,
        derived_mcp_bindings,
    }
}

fn resolve_unique_binding_targets(runtime_config: &mut RuntimeConfigDraft) {
    let sessions = &runtime_config.sessions;
    for binding in &mut runtime_config.channel_bindings {
        if binding.session_id.is_none() {
            binding.session_id =
                unique_binding_target_session(sessions, &binding.agent_id, &binding.profile_id);
        }
    }
    for binding in &mut runtime_config.mcp_bindings {
        if binding.session_id.is_none() {
            binding.session_id =
                unique_binding_target_session(sessions, &binding.agent_id, &binding.profile_id);
        }
    }
}

fn unique_binding_target_session(
    sessions: &[SessionConfigDraft],
    agent_id: &AgentId,
    profile_id: &ProfileId,
) -> Option<SessionId> {
    let mut matches = sessions
        .iter()
        .filter(|session| session.agent_id == *agent_id && session.profile_id == *profile_id);
    let target = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    Some(target.session_id.clone())
}

pub fn plan_delegated_role_lifecycle(
    input: &DelegatedRoleLifecyclePlanInput,
) -> DelegatedRoleLifecyclePlan {
    let mut diagnostics = Vec::new();
    collect_id_diagnostic(
        &mut diagnostics,
        "invalid_delegated_session_id",
        "delegatedSessionId",
        &input.delegated_session_id,
    );
    collect_id_diagnostic(
        &mut diagnostics,
        "invalid_delegated_agent_id",
        "delegatedAgentId",
        &input.delegated_agent_id,
    );
    collect_id_diagnostic(
        &mut diagnostics,
        "invalid_profile_id",
        "profileId",
        &input.profile_id.0,
    );
    collect_non_empty_diagnostic(
        &mut diagnostics,
        "invalid_source_wake_id",
        "sourceWakeId",
        &input.source_wake_id,
    );
    if let Some(tool_profile_key) = &input.tool_profile_key {
        collect_non_empty_diagnostic(
            &mut diagnostics,
            "invalid_tool_profile_key",
            "toolProfileKey",
            tool_profile_key,
        );
    }
    if let Some(correlation_id) = &input.correlation_id {
        collect_non_empty_diagnostic(
            &mut diagnostics,
            "invalid_correlation_id",
            "correlationId",
            correlation_id,
        );
    }
    if input.delegated_session_id == input.parent_session.session_id.0 {
        diagnostics.push(RuntimeConfigDiagnostic::error(
            "delegated_session_matches_parent",
            "delegatedSessionId",
            "delegated session id must differ from parent session id",
        ));
    }
    if input.delegated_agent_id == input.parent_session.agent_id.0 {
        diagnostics.push(RuntimeConfigDiagnostic::error(
            "delegated_agent_matches_parent",
            "delegatedAgentId",
            "delegated agent id must differ from parent agent id",
        ));
    }
    if !matches!(
        input.parent_session.kind,
        SessionKind::Full | SessionKind::Worker | SessionKind::Delegated
    ) {
        diagnostics.push(RuntimeConfigDiagnostic::error(
            "delegation_parent_kind_invalid",
            "parentSession.kind",
            "parent session kind cannot delegate",
        ));
    }

    let parent_limits = input
        .parent_session
        .resource_limits
        .clone()
        .unwrap_or(ResourceLimits {
            max_duration_ms: None,
            max_delegation_depth: None,
        });
    let inherited_depth = parent_limits
        .max_delegation_depth
        .map(|depth| depth.saturating_sub(1));
    if parent_limits.max_delegation_depth == Some(0) {
        diagnostics.push(RuntimeConfigDiagnostic::error(
            "delegation_depth_exhausted",
            "parentSession.resourceLimits.maxDelegationDepth",
            "parent session has no remaining delegation depth",
        ));
    }
    let requested_limits = input.requested_resource_limits.as_ref();
    let requested_depth = requested_limits.and_then(|limits| limits.max_delegation_depth);
    if let (Some(requested), Some(max_child_depth)) = (requested_depth, inherited_depth) {
        if requested > max_child_depth {
            diagnostics.push(RuntimeConfigDiagnostic::error(
                "delegation_depth_escalation",
                "requestedResourceLimits.maxDelegationDepth",
                format!(
                    "requested child delegation depth {requested} exceeds inherited maximum {max_child_depth}"
                ),
            ));
        }
    }
    let requested_duration = requested_limits.and_then(|limits| limits.max_duration_ms);
    if let (Some(requested), Some(parent)) = (requested_duration, parent_limits.max_duration_ms) {
        if requested > parent {
            diagnostics.push(RuntimeConfigDiagnostic::error(
                "delegation_duration_escalation",
                "requestedResourceLimits.maxDurationMs",
                format!("requested child duration {requested} exceeds parent maximum {parent}"),
            ));
        }
    }
    if let Some(constraint) = input.requested_workspace_constraint.as_ref() {
        if constraint.cwd.trim().is_empty() || !Path::new(&constraint.cwd).is_absolute() {
            diagnostics.push(RuntimeConfigDiagnostic::error(
                "invalid_delegated_workspace_constraint",
                "requestedWorkspaceConstraint.cwd",
                "delegated workspace constraint cwd must be a non-empty absolute path",
            ));
        }
    }

    let effective_limits = ResourceLimits {
        max_duration_ms: requested_duration.or(parent_limits.max_duration_ms),
        max_delegation_depth: requested_depth.or(inherited_depth),
    };
    let accepted = !diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == RuntimeConfigDiagnosticSeverity::Error);
    let reason_code = if accepted {
        "delegated_role_lifecycle_planned".to_string()
    } else {
        diagnostics
            .iter()
            .find(|diagnostic| diagnostic.severity == RuntimeConfigDiagnosticSeverity::Error)
            .map(|diagnostic| diagnostic.code.clone())
            .unwrap_or_else(|| "delegated_role_lifecycle_rejected".to_string())
    };
    DelegatedRoleLifecyclePlan {
        accepted,
        reason_code,
        diagnostics,
        session_id: SessionId::new(input.delegated_session_id.clone()),
        agent_id: AgentId::new(input.delegated_agent_id.clone()),
        parent_session_id: input.parent_session.session_id.clone(),
        parent_agent_id: input.parent_session.agent_id.clone(),
        profile_id: input.profile_id.clone(),
        kind: SessionKind::Delegated,
        resource_limits: effective_limits,
        workspace_constraint: input.requested_workspace_constraint.clone(),
        tool_profile_key: input.tool_profile_key.clone(),
        source_wake_id: input.source_wake_id.clone(),
        source_action_index: input.source_action_index,
        task_id: input.task_id.clone(),
        correlation_id: input.correlation_id.clone().unwrap_or_else(|| {
            format!(
                "delegation:{}:{}",
                input.source_wake_id, input.source_action_index
            )
        }),
    }
}

fn apply_profile_session_defaults(
    session: &mut SessionConfigDraft,
    profile: &ProfileRuntimeMetadata,
) {
    if session.resource_limits.is_none() {
        session.resource_limits = profile
            .runtime
            .as_ref()
            .and_then(|runtime| runtime.default_resource_limits.clone());
    }
    let Some(defaults) = &profile.session_defaults else {
        return;
    };
    if session.owner_id.is_none() {
        session.owner_id = defaults.owner_id.clone();
    }
    if session.max_history_messages.is_none() {
        session.max_history_messages = defaults.max_history_messages;
    }
}

fn derive_background_review_job(
    profile: &ProfileRuntimeMetadata,
    scheduled_job_ids: &mut HashSet<String>,
    profiles_with_review_jobs: &mut HashSet<ProfileId>,
) -> Option<ScheduledJobConfigDraft> {
    let review = profile.background_review.as_ref()?;
    if !review.enabled || !profiles_with_review_jobs.insert(profile.profile_id.clone()) {
        return None;
    }
    let id = format!("background-review-{}", profile.profile_id);
    if !scheduled_job_ids.insert(id.clone()) {
        return None;
    }
    Some(ScheduledJobConfigDraft {
        id,
        schedule: review
            .schedule
            .clone()
            .unwrap_or_else(|| "0 3 * * *".to_string()),
        shape: ScheduledJobShape::HostJob,
        job_kind: Some(RUNTIME_REVIEW_MEMORY_SKILLS_JOB_KIND.to_string()),
        target_session_id: None,
        script: None,
        delivery_channel_id: None,
    })
}

fn derive_profile_mcp_binding(
    bindings: &[McpBindingConfigDraft],
    session: &SessionConfigDraft,
    profile: &ProfileRuntimeMetadata,
) -> Option<McpBindingConfigDraft> {
    let mcp = profile.mcp_config.as_ref()?;
    let tool_profile = mcp.tool_profile.as_ref()?;
    let binding_id = mcp
        .binding_id
        .clone()
        .unwrap_or_else(|| format!("{}-mcp", session.agent_id));
    if bindings.iter().any(|binding| {
        binding.binding_id == binding_id
            || (binding.profile_id == session.profile_id
                && binding
                    .session_id
                    .as_ref()
                    .is_none_or(|session_id| *session_id == session.session_id)
                && binding.tool_profile_key == *tool_profile)
    }) {
        return None;
    }
    Some(McpBindingConfigDraft {
        binding_id,
        adapter_id: AdapterId::new("mcp-ts-main"),
        agent_id: session.agent_id.clone(),
        instance_id: None,
        session_id: Some(session.session_id.clone()),
        profile_id: session.profile_id.clone(),
        server_names: if mcp.server_names.is_empty() {
            vec![session.agent_id.to_string()]
        } else {
            mcp.server_names.clone()
        },
        endpoint_ref: mcp
            .endpoint_ref
            .clone()
            .unwrap_or_else(|| format!("config://mcp/{}", session.agent_id)),
        transport: mcp.transport.clone().unwrap_or_else(|| "stdio".to_string()),
        tool_profile_key: tool_profile.clone(),
        status: ExternalBindingStatusDraft::Active,
    })
}

pub fn plan_create_profile(input: &CreateProfilePlanInput) -> CreateProfilePlan {
    let profile_id = input.request.profile_id.trim();
    let agent_id = input
        .request
        .agent_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(profile_id);
    let session_id = input
        .request
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("{agent_id}-session"));
    let implementation_id = input
        .request
        .implementation_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("{profile_id}-brain"));
    let kind = input.request.kind.clone().unwrap_or(SessionKind::Full);
    let workspace_cwd = input
        .request
        .workspace_cwd
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let legacy_provider_alias = input
        .request
        .provider_alias
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let model_config_id = input
        .request
        .model_config_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or(legacy_provider_alias)
        .unwrap_or("default")
        .to_string();
    let external_message_delivery_policy = input
        .request
        .external_message_delivery_policy
        .unwrap_or_default();
    let model_config = input
        .request
        .model_config
        .clone()
        .unwrap_or_else(default_profile_model_config);
    let brain = input.request.brain.clone().unwrap_or(ProfileBrainMetadata {
        module: Some("local".to_string()),
        strategy: None,
    });
    let mut diagnostics = Vec::new();
    match workspace_cwd.as_deref() {
        Some(cwd) if !Path::new(cwd).is_absolute() => {
            diagnostics.push(RuntimeConfigDiagnostic::error(
                "invalid_session_workspace",
                "request.workspaceCwd",
                "workspaceCwd must be an absolute path",
            ))
        }
        None if kind == SessionKind::Full => diagnostics.push(RuntimeConfigDiagnostic::error(
            "session_workspace_required",
            "request.workspaceCwd",
            "full sessions require an explicit workspaceCwd",
        )),
        _ => {}
    }
    collect_id_diagnostic(
        &mut diagnostics,
        "invalid_profile_id",
        "request.profileId",
        profile_id,
    );
    collect_id_diagnostic(
        &mut diagnostics,
        "invalid_agent_id",
        "request.agentId",
        agent_id,
    );
    collect_id_diagnostic(
        &mut diagnostics,
        "invalid_session_id",
        "request.sessionId",
        &session_id,
    );
    collect_id_diagnostic(
        &mut diagnostics,
        "invalid_brain_implementation_id",
        "request.implementationId",
        &implementation_id,
    );
    if let Some(mcp_tool_profile) = input
        .request
        .mcp_tool_profile
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        collect_id_diagnostic(
            &mut diagnostics,
            "invalid_tool_profile_key",
            "request.mcpToolProfile",
            mcp_tool_profile,
        );
    }
    for (index, binding) in input.request.mcp_bindings.iter().enumerate() {
        collect_id_diagnostic(
            &mut diagnostics,
            "invalid_mcp_server_id",
            &format!("request.mcpBindings[{index}].serverId"),
            &binding.server_id,
        );
        if let Some(binding_id) = binding.binding_id.as_deref() {
            collect_id_diagnostic(
                &mut diagnostics,
                "invalid_mcp_binding_id",
                &format!("request.mcpBindings[{index}].bindingId"),
                binding_id,
            );
        }
        if let Some(adapter_id) = binding.adapter_id.as_deref() {
            collect_id_diagnostic(
                &mut diagnostics,
                "invalid_mcp_adapter_id",
                &format!("request.mcpBindings[{index}].adapterId"),
                adapter_id,
            );
        }
        if let Some(transport) = binding.transport.as_deref() {
            collect_non_empty_diagnostic(
                &mut diagnostics,
                "invalid_mcp_transport",
                &format!("request.mcpBindings[{index}].transport"),
                transport,
            );
        }
        if let Some(tool_profile_key) = binding.tool_profile_key.as_deref() {
            collect_id_diagnostic(
                &mut diagnostics,
                "invalid_tool_profile_key",
                &format!("request.mcpBindings[{index}].toolProfileKey"),
                tool_profile_key,
            );
        }
        if let Some(server_names) = binding.server_names.as_ref() {
            if server_names.is_empty() {
                diagnostics.push(RuntimeConfigDiagnostic::error(
                    "mcp_binding_missing_server_names",
                    format!("request.mcpBindings[{index}].serverNames"),
                    "MCP bindings require at least one server name",
                ));
            }
            for (server_index, server_name) in server_names.iter().enumerate() {
                collect_non_empty_diagnostic(
                    &mut diagnostics,
                    "invalid_server_name",
                    &format!("request.mcpBindings[{index}].serverNames[{server_index}]"),
                    server_name,
                );
            }
        }
    }
    collect_id_diagnostic(
        &mut diagnostics,
        "invalid_model_config_id",
        "request.modelConfigId",
        &model_config_id,
    );
    if let Some(provider_alias) = legacy_provider_alias {
        diagnostics.push(RuntimeConfigDiagnostic::warning(
            "legacy_provider_alias_selection",
            "request.providerAlias",
            format!("providerAlias {provider_alias} is compatibility-only; use modelConfigId"),
        ));
    }
    if input.request.model_config.is_some() {
        diagnostics.push(RuntimeConfigDiagnostic::warning(
            "inline_model_config_selection",
            "request.modelConfig",
            "inline modelConfig is compatibility-only; resolve a registered modelConfigId",
        ));
    }
    collect_non_empty_diagnostic(
        &mut diagnostics,
        "invalid_model_provider",
        "request.modelConfig.provider",
        &model_config.provider,
    );
    collect_non_empty_diagnostic(
        &mut diagnostics,
        "invalid_model_name",
        "request.modelConfig.modelName",
        &model_config.model_name,
    );
    if input.request.profile_file_exists {
        diagnostics.push(RuntimeConfigDiagnostic::error(
            "profile_file_exists",
            "request.profileId",
            format!("profile file for {profile_id} already exists"),
        ));
    }

    let profile_id = ProfileId::new(profile_id.to_string());
    let agent_id = AgentId::new(agent_id.to_string());
    let session_id = SessionId::new(session_id);
    let implementation_id = BrainImplementationId::new(implementation_id);

    if input
        .profiles
        .iter()
        .any(|profile| profile.profile_id == profile_id)
    {
        diagnostics.push(RuntimeConfigDiagnostic::error(
            "duplicate_profile_id",
            "request.profileId",
            format!("profile metadata for {profile_id} already exists"),
        ));
    }
    if let Some(existing_registry) = input
        .profile_registry
        .iter()
        .find(|record| record.profile_id == profile_id)
    {
        let lifecycle = existing_registry
            .lifecycle_status
            .as_ref()
            .map(|status| format!(" with lifecycle status {status:?}"))
            .unwrap_or_default();
        diagnostics.push(RuntimeConfigDiagnostic::error(
            "duplicate_profile_registry_record",
            "request.profileId",
            format!("profile registry record for {profile_id} already exists{lifecycle}"),
        ));
    }
    if input
        .runtime_config
        .brains
        .iter()
        .any(|brain| brain.profile_id == profile_id)
    {
        diagnostics.push(RuntimeConfigDiagnostic::error(
            "duplicate_profile_brain",
            "request.profileId",
            format!("runtime config already has a brain for {profile_id}"),
        ));
    }
    if input
        .runtime_config
        .brains
        .iter()
        .any(|brain| brain.implementation_id == implementation_id)
    {
        diagnostics.push(RuntimeConfigDiagnostic::error(
            "duplicate_brain_implementation_id",
            "request.implementationId",
            format!("runtime config already has brain implementation {implementation_id}"),
        ));
    }
    if input
        .runtime_config
        .sessions
        .iter()
        .any(|session| session.profile_id == profile_id)
    {
        diagnostics.push(RuntimeConfigDiagnostic::error(
            "duplicate_profile_session",
            "request.profileId",
            format!("runtime config already has a session for {profile_id}"),
        ));
    }
    if input
        .runtime_config
        .sessions
        .iter()
        .any(|session| session.session_id == session_id)
    {
        diagnostics.push(RuntimeConfigDiagnostic::error(
            "duplicate_session_id",
            "request.sessionId",
            format!("runtime config already has session {session_id}"),
        ));
    }
    if input
        .runtime_config
        .sessions
        .iter()
        .any(|session| session.agent_id == agent_id)
    {
        diagnostics.push(RuntimeConfigDiagnostic::error(
            "duplicate_agent_id",
            "request.agentId",
            format!("runtime config already has agent {agent_id}"),
        ));
    }
    let mut requested_mcp_binding_ids = HashSet::new();
    for (index, binding) in input.request.mcp_bindings.iter().enumerate() {
        let binding_id = binding
            .binding_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("{agent_id}-mcp-{}", index + 1));
        if !requested_mcp_binding_ids.insert(binding_id.clone()) {
            diagnostics.push(RuntimeConfigDiagnostic::error(
                "duplicate_mcp_binding_id",
                format!("request.mcpBindings[{index}].bindingId"),
                format!("duplicate requested MCP binding {binding_id}"),
            ));
        }
        if input
            .runtime_config
            .mcp_bindings
            .iter()
            .any(|existing| existing.binding_id == binding_id)
        {
            diagnostics.push(RuntimeConfigDiagnostic::error(
                "duplicate_mcp_binding_id",
                format!("request.mcpBindings[{index}].bindingId"),
                format!("runtime config already has MCP binding {binding_id}"),
            ));
        }
    }

    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == RuntimeConfigDiagnosticSeverity::Error)
    {
        return CreateProfilePlan {
            diagnostics,
            registry_write: None,
            file_asset_actions: Vec::new(),
            derived_runtime_actions: Vec::new(),
            profile_seed: None,
            runtime_brain: None,
            runtime_session: None,
            profile_mcp_config: None,
            runtime_mcp_bindings: Vec::new(),
        };
    }

    let runtime_brain = BrainConfigDraft {
        implementation_id: implementation_id.clone(),
        profile_id: profile_id.clone(),
    };
    let runtime_session = SessionConfigDraft {
        session_id: session_id.clone(),
        agent_id: agent_id.clone(),
        profile_id: profile_id.clone(),
        kind,
        workspace_cwd,
        resource_limits: None,
        owner_id: None,
        history_window: None,
        max_history_messages: None,
    };
    let profile_mcp_config = input
        .request
        .mcp_tool_profile
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|mcp_tool_profile| ProfileMcpConfig {
            binding_id: Some(format!("{agent_id}-mcp")),
            endpoint_ref: Some(format!("config://mcp/{agent_id}")),
            server_names: vec![agent_id.to_string()],
            transport: None,
            tool_profile: Some(mcp_tool_profile.to_string()),
        });
    let runtime_mcp_bindings = input
        .request
        .mcp_bindings
        .iter()
        .enumerate()
        .map(|(index, binding)| {
            let server_id = binding.server_id.trim();
            let desired_binding_id = binding
                .binding_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(desired_mcp_binding_id)
                .unwrap_or_else(|| format!("{agent_id}-mcp-{}", index + 1));
            let tool_profile_key = binding
                .tool_profile_key
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| profile_id.to_string());
            McpBindingConfigDraft {
                binding_id: materialized_mcp_binding_id(&desired_binding_id, &session_id),
                adapter_id: AdapterId::new(
                    binding
                        .adapter_id
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .unwrap_or("mcp-ts-main")
                        .to_string(),
                ),
                agent_id: agent_id.clone(),
                instance_id: None,
                session_id: Some(session_id.clone()),
                profile_id: profile_id.clone(),
                server_names: binding
                    .server_names
                    .clone()
                    .unwrap_or_else(|| vec![server_id.to_string()]),
                endpoint_ref: format!("config://mcp/{server_id}"),
                transport: binding
                    .transport
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("streamable_http")
                    .to_string(),
                tool_profile_key,
                status: ExternalBindingStatusDraft::Active,
            }
        })
        .collect::<Vec<_>>();
    let now = input
        .request
        .now
        .clone()
        .unwrap_or_else(|| "1970-01-01T00:00:00.000Z".to_string());
    let registry_write = ProfileRegistryWrite {
        profile_id: profile_id.clone(),
        lifecycle_status: ProfileRegistryLifecycleStatus::Active,
        display_name: input.request.display_name.clone(),
        summary: None,
        default_session_kind: Some(runtime_session.kind.clone()),
        agent_id: Some(runtime_session.agent_id.clone()),
        owner_id: runtime_session.owner_id.clone(),
        prompt_soul_markdown: input.request.soul_markdown.clone(),
        prompt_memory_markdown: input.request.memory_markdown.clone(),
        active_runtime_settings_json: create_profile_runtime_settings_json(
            &model_config_id,
            &brain,
            external_message_delivery_policy,
            profile_mcp_config.as_ref(),
            &input.request.mcp_bindings,
            input.request.source.as_ref(),
        ),
        source_asset_refs: vec![ProfileRegistrySourceAssetRef {
            asset_kind: "profile_json".to_string(),
            path: format!("{}.json", profile_id),
            content_hash: None,
            last_seen_at: None,
            metadata_json: json!({
                "planned_by": "create_profile",
                "compatibility_export": true,
            }),
        }],
        derived_runtime_refs: {
            let mut refs = vec![
                ProfileRegistryDerivedRuntimeRef {
                    ref_kind: "brain".to_string(),
                    ref_id: runtime_brain.implementation_id.to_string(),
                    status: "planned".to_string(),
                    updated_at: None,
                    metadata_json: json!({
                        "profile_id": runtime_brain.profile_id,
                    }),
                },
                ProfileRegistryDerivedRuntimeRef {
                    ref_kind: "session".to_string(),
                    ref_id: runtime_session.session_id.to_string(),
                    status: "planned".to_string(),
                    updated_at: None,
                    metadata_json: json!({
                        "agent_id": runtime_session.agent_id,
                        "profile_id": runtime_session.profile_id,
                        "kind": runtime_session.kind,
                    }),
                },
            ];
            if let Some(profile_mcp_config) = profile_mcp_config.as_ref() {
                refs.push(ProfileRegistryDerivedRuntimeRef {
                    ref_kind: "profile_mcp_config".to_string(),
                    ref_id: profile_mcp_config
                        .binding_id
                        .clone()
                        .unwrap_or_else(|| format!("{agent_id}-mcp")),
                    status: "planned".to_string(),
                    updated_at: None,
                    metadata_json: json!({
                        "tool_profile": profile_mcp_config.tool_profile,
                    }),
                });
            }
            refs.extend(runtime_mcp_bindings.iter().map(|binding| {
                ProfileRegistryDerivedRuntimeRef {
                    ref_kind: "mcp_binding".to_string(),
                    ref_id: binding.binding_id.clone(),
                    status: "planned".to_string(),
                    updated_at: None,
                    metadata_json: json!({
                        "server_names": binding.server_names,
                        "endpoint_ref": binding.endpoint_ref,
                        "tool_profile_key": binding.tool_profile_key,
                    }),
                }
            }));
            refs
        },
        import_export: ProfileRegistryImportExportMetadata {
            imported_from: create_profile_import_source(input.request.source.as_ref()),
            imported_at: input.request.source.as_ref().map(|_| now.clone()),
            exported_to: None,
            exported_at: None,
            metadata_json: json!({
                "created_by": "create_profile_plan",
                "source": input.request.source,
            }),
        },
        now,
    };
    let file_asset_actions = vec![CreateProfileFileAssetAction {
        kind: CreateProfileFileAssetActionKind::WriteProfileJson,
        profile_id: profile_id.clone(),
        relative_path: format!("{}.json", profile_id),
        overwrite: false,
        metadata_json: json!({
            "compatibility": true,
            "registry_first": true,
        }),
    }];
    let mut derived_runtime_actions = vec![
        CreateProfileDerivedRuntimeAction {
            kind: CreateProfileDerivedRuntimeActionKind::AddBrain,
            ref_kind: "brain".to_string(),
            ref_id: runtime_brain.implementation_id.to_string(),
            apply_phase: "compatibility_runtime_config".to_string(),
            metadata_json: json!({
                "profile_id": runtime_brain.profile_id,
            }),
        },
        CreateProfileDerivedRuntimeAction {
            kind: CreateProfileDerivedRuntimeActionKind::AddSession,
            ref_kind: "session".to_string(),
            ref_id: runtime_session.session_id.to_string(),
            apply_phase: "compatibility_runtime_config".to_string(),
            metadata_json: json!({
                "agent_id": runtime_session.agent_id,
                "profile_id": runtime_session.profile_id,
                "kind": runtime_session.kind,
            }),
        },
    ];
    if let Some(profile_mcp_config) = profile_mcp_config.as_ref() {
        derived_runtime_actions.push(CreateProfileDerivedRuntimeAction {
            kind: CreateProfileDerivedRuntimeActionKind::AddProfileMcpConfig,
            ref_kind: "profile_mcp_config".to_string(),
            ref_id: profile_mcp_config
                .binding_id
                .clone()
                .unwrap_or_else(|| format!("{agent_id}-mcp")),
            apply_phase: "compatibility_profile_file".to_string(),
            metadata_json: json!({
                "tool_profile": profile_mcp_config.tool_profile,
            }),
        });
    }
    derived_runtime_actions.extend(runtime_mcp_bindings.iter().map(|binding| {
        CreateProfileDerivedRuntimeAction {
            kind: CreateProfileDerivedRuntimeActionKind::AddMcpBinding,
            ref_kind: "mcp_binding".to_string(),
            ref_id: binding.binding_id.clone(),
            apply_phase: "compatibility_runtime_config".to_string(),
            metadata_json: json!({
                "server_names": binding.server_names,
                "endpoint_ref": binding.endpoint_ref,
                "tool_profile_key": binding.tool_profile_key,
            }),
        }
    }));

    CreateProfilePlan {
        diagnostics,
        registry_write: Some(registry_write),
        file_asset_actions,
        derived_runtime_actions,
        profile_seed: Some(CreateProfileSeedMetadata {
            profile_id,
            display_name: input.request.display_name.clone(),
            model_config_id,
            provider_alias: legacy_provider_alias.map(str::to_string),
            model_config,
            brain,
            external_message_delivery_policy,
            skills_mode: "all".to_string(),
        }),
        runtime_brain: Some(runtime_brain),
        runtime_session: Some(runtime_session),
        profile_mcp_config,
        runtime_mcp_bindings,
    }
}

pub fn plan_new_session_control(input: &NewSessionControlPlanInput) -> NewSessionControlPlan {
    let command_kind = input.command.command_kind.trim().to_string();
    let old_session_id = input
        .command
        .target_session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let new_session_id = input
        .generated_session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let operator_reason = input
        .command
        .operator_reason
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("slash command /new")
        .to_string();
    let reason_code = input
        .command
        .operator_reason_code
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("slash_command_new")
        .to_string();
    let target = NewSessionControlTarget {
        old_session_id: old_session_id.clone(),
        new_session_id: new_session_id.clone(),
        agent_id: input
            .template
            .as_ref()
            .map(|template| template.agent_id.clone()),
        profile_id: input
            .template
            .as_ref()
            .map(|template| template.profile_id.clone()),
        channel_binding_id: input
            .template
            .as_ref()
            .and_then(|template| template.channel_binding_id.clone()),
        channel_id: input
            .template
            .as_ref()
            .and_then(|template| template.channel_id.clone()),
        tool_profile_key: input
            .template
            .as_ref()
            .and_then(|template| template.tool_profile_key.clone()),
    };
    let mut preconditions = Vec::new();
    let denied_context = |preconditions| DeniedNewSessionPlanInput {
        command_kind: command_kind.clone(),
        target: target.clone(),
        idempotency_key: input.command.idempotency_key.clone(),
        operator_reason: operator_reason.clone(),
        reason_code: reason_code.clone(),
        preconditions,
    };

    if command_kind != "new_session" {
        preconditions.push(failed_precondition(
            "unsupported_control_command",
            format!("control command {command_kind} is not supported by the /new planner"),
        ));
        return denied_new_session_plan(
            denied_context(preconditions),
            "unsupported_control_command",
            "Only new_session controls can use the new-session planner.",
        );
    }
    preconditions.push(satisfied_precondition(
        "command_kind_supported",
        "new_session command is supported by the Rust planner",
    ));

    let Some(old_session_id) = old_session_id else {
        preconditions.push(failed_precondition(
            "target_session_required",
            "new_session requires the current session id",
        ));
        return denied_new_session_plan(
            denied_context(preconditions),
            "missing_session_id",
            "Cannot create a new session without a current session.",
        );
    };
    if !is_valid_component_id(&old_session_id) {
        preconditions.push(failed_precondition(
            "target_session_valid",
            format!("target session id {ID_PATTERN_DESCRIPTION}"),
        ));
        return denied_new_session_plan(
            denied_context(preconditions),
            "invalid_session_id",
            "Current session id is invalid.",
        );
    }
    preconditions.push(satisfied_precondition(
        "target_session_valid",
        "current session id is present and valid",
    ));

    let Some(template) = input.template.as_ref() else {
        preconditions.push(failed_precondition(
            "session_template_loaded",
            "new_session requires a loaded current-session template",
        ));
        return denied_new_session_plan(
            denied_context(preconditions),
            "missing_session_template",
            "Cannot create a new session without a current-session template.",
        );
    };
    preconditions.push(satisfied_precondition(
        "session_template_loaded",
        "current-session template is loaded",
    ));

    let workspace_cwd = template
        .workspace_cwd
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if template.kind == SessionKind::Full
        && !workspace_cwd.is_some_and(|value| value.starts_with('/'))
    {
        preconditions.push(failed_precondition(
            "session_workspace_inheritable",
            "full-session /new requires an absolute canonical workspace on the current session",
        ));
        return denied_new_session_plan(
            denied_context(preconditions),
            "session_workspace_missing",
            "Cannot create a replacement session without the current canonical workspace.",
        );
    }
    preconditions.push(satisfied_precondition(
        "session_workspace_inheritable",
        match workspace_cwd {
            Some(workspace_cwd) => {
                format!("replacement inherits canonical workspace {workspace_cwd}")
            }
            None => "non-full replacement does not require a canonical workspace".to_string(),
        },
    ));

    let Some(new_session_id) = new_session_id else {
        preconditions.push(failed_precondition(
            "new_session_identity_distinct",
            "generated new session id is missing",
        ));
        return denied_new_session_plan(
            denied_context(preconditions),
            "new_session_identity_not_distinct",
            "New session ID must be distinct from the archived session.",
        );
    };
    if !is_valid_component_id(&new_session_id) {
        preconditions.push(failed_precondition(
            "new_session_identity_valid",
            format!("new session id {ID_PATTERN_DESCRIPTION}"),
        ));
        return denied_new_session_plan(
            denied_context(preconditions),
            "invalid_new_session_id",
            "New session id is invalid.",
        );
    }
    if new_session_id == old_session_id {
        preconditions.push(failed_precondition(
            "new_session_identity_distinct",
            "new session id matches the current session id",
        ));
        return denied_new_session_plan(
            denied_context(preconditions),
            "new_session_identity_not_distinct",
            "New session ID must be distinct from the archived session.",
        );
    }
    preconditions.push(satisfied_precondition(
        "new_session_identity_distinct",
        "new session id is valid and distinct",
    ));

    let requires_rebind = template.channel_binding_id.is_some() || template.channel_id.is_some();
    if requires_rebind && !input.rebind_handler_available {
        preconditions.push(failed_precondition(
            "channel_rebind_available",
            "current session has channel binding context but no rebind handler is available",
        ));
        return denied_new_session_plan(
            denied_context(preconditions),
            "missing_channel_rebind",
            "Channel binding context requires an explicit rebind handler.",
        );
    }
    preconditions.push(satisfied_precondition(
        "channel_rebind_available",
        if requires_rebind {
            "channel rebind handler is available"
        } else {
            "no channel rebind is required"
        },
    ));

    let mut actions = vec![
        NewSessionControlAction {
            action: NewSessionControlActionKind::ArchiveSession,
            session_id: Some(old_session_id.clone()),
            old_session_id: Some(old_session_id.clone()),
            new_session_id: Some(new_session_id.clone()),
            reason_code: reason_code.clone(),
        },
        NewSessionControlAction {
            action: NewSessionControlActionKind::CreateSession,
            session_id: Some(new_session_id.clone()),
            old_session_id: Some(old_session_id.clone()),
            new_session_id: Some(new_session_id.clone()),
            reason_code: reason_code.clone(),
        },
    ];
    if requires_rebind {
        actions.push(NewSessionControlAction {
            action: NewSessionControlActionKind::RebindChannel,
            session_id: None,
            old_session_id: Some(old_session_id),
            new_session_id: Some(new_session_id),
            reason_code: reason_code.clone(),
        });
    }

    NewSessionControlPlan {
        accepted: true,
        command_kind,
        target,
        idempotency_key: input.command.idempotency_key.clone(),
        operator_reason,
        reason_code,
        denial: None,
        preconditions,
        actions,
    }
}

pub fn plan_reload_mcp_control(input: &ReloadMcpControlPlanInput) -> ReloadMcpControlPlan {
    let command_kind = input.command.command_kind.trim().to_string();
    let session_id = input
        .command
        .target_session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let operator_reason = input
        .command
        .operator_reason
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("slash command /reload-mcp")
        .to_string();
    let reason_code = input
        .command
        .operator_reason_code
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("slash_reload_mcp")
        .to_string();
    let target = ReloadMcpControlTarget {
        session_id: session_id.clone(),
        binding_id: input
            .binding
            .as_ref()
            .map(|binding| binding.binding_id.clone()),
        profile_id: input
            .binding
            .as_ref()
            .map(|binding| binding.profile_id.clone()),
        tool_profile_key: input
            .binding
            .as_ref()
            .and_then(|binding| binding.tool_profile_key.clone()),
        endpoint_ref: input
            .binding
            .as_ref()
            .and_then(|binding| binding.endpoint_ref.clone()),
    };
    let mut preconditions = Vec::new();
    let denied_context = |preconditions| DeniedReloadMcpPlanInput {
        command_kind: command_kind.clone(),
        target: target.clone(),
        idempotency_key: input.command.idempotency_key.clone(),
        operator_reason: operator_reason.clone(),
        reason_code: reason_code.clone(),
        preconditions,
    };

    if command_kind != "reload_mcp" {
        preconditions.push(failed_precondition(
            "unsupported_control_command",
            format!("control command {command_kind} is not supported by the /reload-mcp planner"),
        ));
        return denied_reload_mcp_plan(
            denied_context(preconditions),
            "unsupported_control_command",
            "Only reload_mcp controls can use the reload-MCP planner.",
        );
    }
    preconditions.push(satisfied_precondition(
        "command_kind_supported",
        "reload_mcp command is supported by the Rust planner",
    ));

    let Some(session_id) = session_id else {
        preconditions.push(failed_precondition(
            "target_session_required",
            "reload_mcp requires the current session id",
        ));
        return denied_reload_mcp_plan(
            denied_context(preconditions),
            "missing_session_id",
            "Cannot reload MCP without a current session.",
        );
    };
    if !is_valid_component_id(&session_id) {
        preconditions.push(failed_precondition(
            "target_session_valid",
            format!("target session id {ID_PATTERN_DESCRIPTION}"),
        ));
        return denied_reload_mcp_plan(
            denied_context(preconditions),
            "invalid_session_id",
            "Current session id is invalid.",
        );
    }
    preconditions.push(satisfied_precondition(
        "target_session_valid",
        "current session id is present and valid",
    ));

    let Some(binding) = input.binding.as_ref() else {
        preconditions.push(failed_precondition(
            "mcp_binding_resolved",
            "reload_mcp requires a resolved MCP binding for the current session",
        ));
        return denied_reload_mcp_plan(
            denied_context(preconditions),
            "mcp_binding_not_found",
            "No MCP binding is available for the current session.",
        );
    };
    if binding.binding_id.trim().is_empty() || !is_valid_component_id(&binding.binding_id) {
        preconditions.push(failed_precondition(
            "mcp_binding_valid",
            format!("MCP binding id {ID_PATTERN_DESCRIPTION}"),
        ));
        return denied_reload_mcp_plan(
            denied_context(preconditions),
            "invalid_mcp_binding",
            "Resolved MCP binding id is invalid.",
        );
    }
    if binding.session_id != session_id {
        preconditions.push(failed_precondition(
            "mcp_binding_matches_session",
            "resolved MCP binding belongs to a different session",
        ));
        return denied_reload_mcp_plan(
            denied_context(preconditions),
            "mcp_binding_session_mismatch",
            "Resolved MCP binding does not belong to the requested session.",
        );
    }
    preconditions.push(satisfied_precondition(
        "mcp_binding_resolved",
        "MCP binding is resolved for the current session",
    ));
    preconditions.push(satisfied_precondition(
        "mcp_binding_matches_session",
        "MCP binding belongs to the current session",
    ));

    if !input.reload_handler_available {
        preconditions.push(failed_precondition(
            "mcp_reload_handler_available",
            "reload_mcp requires an explicit reload handler",
        ));
        return denied_reload_mcp_plan(
            denied_context(preconditions),
            "missing_mcp_reload_handler",
            "MCP reload requires an explicit reload handler.",
        );
    }
    preconditions.push(satisfied_precondition(
        "mcp_reload_handler_available",
        "MCP reload handler is available",
    ));

    ReloadMcpControlPlan {
        accepted: true,
        command_kind,
        target,
        idempotency_key: input.command.idempotency_key.clone(),
        operator_reason,
        reason_code: reason_code.clone(),
        denial: None,
        preconditions,
        actions: vec![ReloadMcpControlAction {
            action: ReloadMcpControlActionKind::ReloadMcpSurface,
            session_id,
            binding_id: binding.binding_id.clone(),
            reason_code,
        }],
    }
}

pub fn plan_channel_ingress_route(input: &ChannelIngressRoutePlanInput) -> ChannelIngressRoutePlan {
    let message = &input.message;
    let denied = |reason_code: &str, reason: &str| ChannelIngressRoutePlan {
        status: ChannelIngressRouteDecision::Denied,
        reason_code: reason_code.to_string(),
        reason: reason.to_string(),
        correlation_id: None,
        binding: None,
        candidates: Vec::new(),
        route: None,
    };

    if !is_valid_component_id(&message.binding_id) {
        return denied(
            "invalid_message_binding_id",
            "Inbound channel message binding id is invalid.",
        );
    }
    if message.idempotency_key.trim().is_empty() {
        return denied(
            "missing_idempotency_key",
            "Inbound channel message must include an idempotency key.",
        );
    }
    if message.provider.trim().is_empty() || message.external_channel_id.trim().is_empty() {
        return denied(
            "missing_provider_ref",
            "Inbound channel message must include provider and external channel refs.",
        );
    }

    let correlation_id = format!("channel:{}:{}", message.binding_id, message.idempotency_key);
    if input
        .seen_idempotency_keys
        .iter()
        .any(|key| key == &message.idempotency_key)
    {
        return ChannelIngressRoutePlan {
            status: ChannelIngressRouteDecision::Duplicate,
            reason_code: "duplicate_idempotency_key".to_string(),
            reason: "Inbound channel message idempotency key was already routed.".to_string(),
            correlation_id: Some(correlation_id),
            binding: None,
            candidates: Vec::new(),
            route: None,
        };
    }

    if let Some(now) = input.now.as_deref().filter(|value| !value.is_empty()) {
        if !message.expires_at.trim().is_empty() && now >= message.expires_at.as_str() {
            return ChannelIngressRoutePlan {
                status: ChannelIngressRouteDecision::Expired,
                reason_code: "message_ttl_expired".to_string(),
                reason: "Inbound channel message expired before route planning.".to_string(),
                correlation_id: Some(correlation_id),
                binding: None,
                candidates: Vec::new(),
                route: None,
            };
        }
    }

    let matching_surface: Vec<ChannelBindingConfigDraft> = input
        .bindings
        .iter()
        .filter(|binding| binding.status == ExternalBindingStatusDraft::Active)
        .filter(|binding| binding.provider == message.provider)
        .filter(|binding| binding.external_channel_id == message.external_channel_id)
        .filter(|binding| {
            message.external_thread_id.is_none()
                || binding.external_thread_id.is_none()
                || binding.external_thread_id == message.external_thread_id
        })
        .cloned()
        .collect();

    if matching_surface.is_empty() {
        let inactive_candidates: Vec<ChannelBindingConfigDraft> = input
            .bindings
            .iter()
            .filter(|binding| binding.provider == message.provider)
            .filter(|binding| binding.external_channel_id == message.external_channel_id)
            .cloned()
            .collect();
        if inactive_candidates.is_empty() {
            return ChannelIngressRoutePlan {
                status: ChannelIngressRouteDecision::NoBinding,
                reason_code: "no_active_channel_binding".to_string(),
                reason: "No active channel binding matches provider/channel.".to_string(),
                correlation_id: Some(correlation_id),
                binding: None,
                candidates: Vec::new(),
                route: None,
            };
        }
        return ChannelIngressRoutePlan {
            status: ChannelIngressRouteDecision::InactiveBinding,
            reason_code: "channel_binding_inactive".to_string(),
            reason: "Matching channel bindings are not active.".to_string(),
            correlation_id: Some(correlation_id),
            binding: None,
            candidates: inactive_candidates,
            route: None,
        };
    }

    let explicit_binding: Vec<ChannelBindingConfigDraft> = matching_surface
        .iter()
        .filter(|binding| binding.binding_id == message.binding_id)
        .cloned()
        .collect();
    let mention_targets = mentioned_agent_ids(&message.mentions, &input.mention_aliases);
    let mentioned_bindings: Vec<ChannelBindingConfigDraft> = if mention_targets.is_empty() {
        Vec::new()
    } else {
        matching_surface
            .iter()
            .filter(|binding| mention_targets.contains(&binding.agent_id))
            .cloned()
            .collect()
    };
    let runtime_binding: Vec<ChannelBindingConfigDraft> =
        if let Some(runtime_agent_id) = message.runtime_agent_id.as_ref() {
            matching_surface
                .iter()
                .filter(|binding| &binding.agent_id == runtime_agent_id)
                .cloned()
                .collect()
        } else {
            Vec::new()
        };
    let singleton_surface = if matching_surface.len() == 1 {
        matching_surface.clone()
    } else {
        Vec::new()
    };
    let candidates = first_non_empty(vec![
        explicit_binding,
        mentioned_bindings,
        runtime_binding,
        singleton_surface,
    ]);

    if candidates.len() != 1 {
        let ambiguity_candidates = if candidates.len() > 1 {
            candidates
        } else {
            matching_surface
        };
        let reason = if ambiguity_candidates.len() > 1 {
            "Multiple bindings matched channel route."
        } else {
            "Multiple bindings share this channel and no mention/runtime binding disambiguated them."
        };
        return ChannelIngressRoutePlan {
            status: ChannelIngressRouteDecision::Ambiguous,
            reason_code: "channel_route_ambiguous".to_string(),
            reason: reason.to_string(),
            correlation_id: Some(correlation_id),
            binding: None,
            candidates: ambiguity_candidates,
            route: None,
        };
    }

    let binding = candidates
        .into_iter()
        .next()
        .expect("checked one candidate");
    let from = input.system_agent_id.clone().unwrap_or_else(|| {
        AgentId::new(format!(
            "channel:{}:{}",
            message.provider, message.external_user_id
        ))
    });
    let route = ChannelIngressRouteRequest {
        from,
        to: binding.agent_id.clone(),
        body: message.body.clone(),
        correlation_id: correlation_id.clone(),
        binding_id: binding.binding_id.clone(),
        session_id: binding.session_id.clone(),
    };

    ChannelIngressRoutePlan {
        status: ChannelIngressRouteDecision::Routed,
        reason_code: "channel_route_routed".to_string(),
        reason: "Inbound channel message routed to an active binding.".to_string(),
        correlation_id: Some(correlation_id),
        binding: Some(binding),
        candidates: Vec::new(),
        route: Some(route),
    }
}

pub fn plan_den_product_ingress_policy(
    input: &DenProductIngressPolicyInput,
) -> DenProductIngressPolicyPlan {
    let operation = input.operation.trim();
    let normalized_operation = if operation.is_empty() {
        "observe"
    } else {
        operation
    };
    let lifecycle_operation = !matches!(normalized_operation, "observe");

    if lifecycle_operation {
        return DenProductIngressPolicyPlan {
            status: DenProductIngressPolicyStatus::Denied,
            operation: normalized_operation.to_string(),
            reason_code: "adapter_lifecycle_operation_denied".to_string(),
            reason:
                "Den product ingress may observe/reference Den data but cannot claim, complete, retry, expire, or otherwise mutate Crew lifecycle state."
                    .to_string(),
            lifecycle_operation,
        };
    }

    DenProductIngressPolicyPlan {
        status: DenProductIngressPolicyStatus::Allowed,
        operation: normalized_operation.to_string(),
        reason_code: "den_product_observe_allowed".to_string(),
        reason: "Den product ingress observation/reference update is allowed.".to_string(),
        lifecycle_operation,
    }
}

pub fn plan_profile_registry_mutation(
    input: &ProfileRegistryMutationRequest,
) -> Result<ProfileRegistryMutationPlan, String> {
    let body = input
        .body_json
        .as_object()
        .ok_or_else(|| "profile registry write body must be an object".to_string())?;
    if input.current.profile_id != input.profile_id {
        return Err(format!(
            "profile registry plan profile id mismatch: route={}, current={}",
            input.profile_id, input.current.profile_id
        ));
    }
    let expected_revision = required_revision(body)?;
    let mut diagnostics = Vec::new();
    if expected_revision != input.current.revision {
        diagnostics.push(RuntimeConfigDiagnostic::error(
            "profile_registry_revision_mismatch",
            "expectedRevision",
            format!(
                "expected revision {expected_revision}, found {}",
                input.current.revision
            ),
        ));
    }

    let next = match input.kind {
        ProfileRegistryMutationKind::Update => {
            next_profile_registry_field_record(&input.current, body, &input.now)?
        }
        ProfileRegistryMutationKind::Lifecycle => {
            next_profile_registry_lifecycle_record(&input.current, body, &input.now)?
        }
        ProfileRegistryMutationKind::Prompt => {
            next_profile_registry_prompt_record(&input.current, body, &input.now)?
        }
    };
    let next_write = profile_registry_record_to_write(&next, &input.now);
    let runtime_rebuild_recommended = matches!(input.kind, ProfileRegistryMutationKind::Lifecycle)
        || input.current.active_runtime_settings_json != next.active_runtime_settings_json
        || input.current.default_session_kind != next.default_session_kind
        || input.current.agent_id != next.agent_id
        || input.current.prompt_soul_markdown != next.prompt_soul_markdown
        || input.current.prompt_memory_markdown != next.prompt_memory_markdown;
    let lifecycle_effects = if matches!(input.kind, ProfileRegistryMutationKind::Lifecycle)
        && next.lifecycle_status != ProfileRegistryLifecycleStatus::Active
    {
        "archive_active_sessions_and_unregister_brain"
    } else {
        "none"
    };

    Ok(ProfileRegistryMutationPlan {
        ok: !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == RuntimeConfigDiagnosticSeverity::Error),
        profile_id: input.profile_id.clone(),
        kind: input.kind.clone(),
        mode: input.mode.clone(),
        expected_revision,
        current: input.current.clone(),
        next,
        next_write,
        diagnostics,
        implications: ProfileRegistryMutationImplications {
            registry_revision_will_increment: true,
            profile_files_unchanged: true,
            service_config_unchanged: true,
            runtime_rebuild_recommended,
            lifecycle_effects: lifecycle_effects.to_string(),
        },
    })
}

fn required_revision(body: &serde_json::Map<String, Value>) -> Result<u64, String> {
    body.get("expectedRevision")
        .or_else(|| body.get("expected_revision"))
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
        .ok_or_else(|| "expectedRevision is required and must be a positive integer".to_string())
}

fn next_profile_registry_field_record(
    current: &ProfileRegistryRecord,
    body: &serde_json::Map<String, Value>,
    now: &str,
) -> Result<ProfileRegistryRecord, String> {
    let mut next = current.clone();
    next.display_name = body_field_string(body, "displayName", current.display_name.clone())?;
    next.summary = body_field_string(body, "summary", current.summary.clone())?;
    next.default_session_kind = body_session_kind(
        body,
        "defaultSessionKind",
        current.default_session_kind.clone(),
    )?;
    next.agent_id = body_field_string(
        body,
        "agentId",
        current.agent_id.as_ref().map(ToString::to_string),
    )?
    .map(AgentId::new);
    next.owner_id = body_field_string(body, "ownerId", current.owner_id.clone())?;
    if body.contains_key("activeRuntimeSettingsJson") {
        next.active_runtime_settings_json = body
            .get("activeRuntimeSettingsJson")
            .cloned()
            .unwrap_or(Value::Null);
    }
    next.updated_at = now.to_string();
    Ok(next)
}

fn next_profile_registry_prompt_record(
    current: &ProfileRegistryRecord,
    body: &serde_json::Map<String, Value>,
    now: &str,
) -> Result<ProfileRegistryRecord, String> {
    let mut next = current.clone();
    next.prompt_soul_markdown = body_markdown_field(
        body,
        "soulMarkdown",
        "promptSoulMarkdown",
        current.prompt_soul_markdown.clone(),
    )?;
    next.prompt_memory_markdown = body_markdown_field(
        body,
        "memoryMarkdown",
        "promptMemoryMarkdown",
        current.prompt_memory_markdown.clone(),
    )?;
    next.updated_at = now.to_string();
    Ok(next)
}

fn next_profile_registry_lifecycle_record(
    current: &ProfileRegistryRecord,
    body: &serde_json::Map<String, Value>,
    now: &str,
) -> Result<ProfileRegistryRecord, String> {
    let lifecycle_status = body
        .get("lifecycleStatus")
        .or_else(|| body.get("lifecycle_status"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            "lifecycleStatus must be active, paused, decommissioned, or archived".to_string()
        })
        .and_then(profile_registry_lifecycle_status_from_str)?;
    let mut next = current.clone();
    next.lifecycle_status = lifecycle_status;
    next.derived_runtime_refs = current
        .derived_runtime_refs
        .iter()
        .cloned()
        .map(|mut runtime_ref| {
            runtime_ref.status =
                derived_runtime_ref_status_for_lifecycle(next.lifecycle_status).to_string();
            runtime_ref.updated_at = Some(now.to_string());
            runtime_ref
        })
        .collect();
    next.updated_at = now.to_string();
    Ok(next)
}

fn profile_registry_record_to_write(
    record: &ProfileRegistryRecord,
    now: &str,
) -> ProfileRegistryWrite {
    ProfileRegistryWrite {
        profile_id: record.profile_id.clone(),
        lifecycle_status: record.lifecycle_status,
        display_name: record.display_name.clone(),
        summary: record.summary.clone(),
        default_session_kind: record.default_session_kind.clone(),
        agent_id: record.agent_id.clone(),
        owner_id: record.owner_id.clone(),
        prompt_soul_markdown: record.prompt_soul_markdown.clone(),
        prompt_memory_markdown: record.prompt_memory_markdown.clone(),
        active_runtime_settings_json: record.active_runtime_settings_json.clone(),
        source_asset_refs: record.source_asset_refs.clone(),
        derived_runtime_refs: record.derived_runtime_refs.clone(),
        import_export: record.import_export.clone(),
        now: now.to_string(),
    }
}

fn body_field_string(
    body: &serde_json::Map<String, Value>,
    key: &str,
    current: Option<String>,
) -> Result<Option<String>, String> {
    let Some(value) = body.get(key) else {
        return Ok(current);
    };
    if value.is_null() {
        return Ok(None);
    }
    let raw = value
        .as_str()
        .ok_or_else(|| format!("{key} must be a string or null"))?
        .trim()
        .to_string();
    Ok((!raw.is_empty()).then_some(raw))
}

fn body_markdown_field(
    body: &serde_json::Map<String, Value>,
    camel_key: &str,
    registry_key: &str,
    current: Option<String>,
) -> Result<Option<String>, String> {
    let key = if body.contains_key(camel_key) {
        Some(camel_key)
    } else if body.contains_key(registry_key) {
        Some(registry_key)
    } else {
        None
    };
    let Some(key) = key else {
        return Ok(current);
    };
    let value = &body[key];
    if value.is_null() {
        return Ok(None);
    }
    Ok(Some(
        value
            .as_str()
            .ok_or_else(|| format!("{camel_key} must be a string or null"))?
            .to_string(),
    ))
}

fn body_session_kind(
    body: &serde_json::Map<String, Value>,
    key: &str,
    current: Option<SessionKind>,
) -> Result<Option<SessionKind>, String> {
    let Some(value) = body.get(key) else {
        return Ok(current);
    };
    if value.is_null() {
        return Ok(None);
    }
    match value.as_str() {
        Some("full") => Ok(Some(SessionKind::Full)),
        Some("worker") => Ok(Some(SessionKind::Worker)),
        Some("delegated") => Ok(Some(SessionKind::Delegated)),
        _ => Err(format!("{key} must be full, worker, delegated, or null")),
    }
}

fn profile_registry_lifecycle_status_from_str(
    value: &str,
) -> Result<ProfileRegistryLifecycleStatus, String> {
    match value {
        "active" => Ok(ProfileRegistryLifecycleStatus::Active),
        "paused" => Ok(ProfileRegistryLifecycleStatus::Paused),
        "decommissioned" => Ok(ProfileRegistryLifecycleStatus::Decommissioned),
        "archived" => Ok(ProfileRegistryLifecycleStatus::Archived),
        _ => Err("lifecycleStatus must be active, paused, decommissioned, or archived".to_string()),
    }
}

fn derived_runtime_ref_status_for_lifecycle(
    status: ProfileRegistryLifecycleStatus,
) -> &'static str {
    match status {
        ProfileRegistryLifecycleStatus::Active => "active",
        ProfileRegistryLifecycleStatus::Paused => "paused",
        ProfileRegistryLifecycleStatus::Decommissioned
        | ProfileRegistryLifecycleStatus::Archived => "disabled",
    }
}

fn create_profile_runtime_settings_json(
    model_config_id: &str,
    brain: &ProfileBrainMetadata,
    external_message_delivery_policy: ExternalMessageDeliveryPolicy,
    mcp_config: Option<&ProfileMcpConfig>,
    mcp_bindings: &[CreateProfileMcpBindingRequest],
    source: Option<&CreateProfileSourceRequest>,
) -> Value {
    json!({
        "modelConfigId": model_config_id,
        "brain": brain,
        "externalMessageDeliveryPolicy": external_message_delivery_policy,
        "skills_mode": "all",
        "mcp_config": mcp_config,
        "mcp_bindings": mcp_bindings,
        "source": source,
    })
}

fn desired_mcp_binding_id(binding_id: &str) -> String {
    binding_id
        .split_once("--session--")
        .map(|(desired, _)| desired)
        .filter(|desired| !desired.is_empty())
        .unwrap_or(binding_id)
        .to_string()
}

fn materialized_mcp_binding_id(binding_id: &str, session_id: &SessionId) -> String {
    format!(
        "{}--session--{}",
        desired_mcp_binding_id(binding_id),
        session_id
    )
}

fn create_profile_import_source(source: Option<&CreateProfileSourceRequest>) -> Option<String> {
    let source = source?;
    if let Some(source_bundle_path) = source.source_bundle_path.as_deref() {
        return Some(format!("bundle:{source_bundle_path}"));
    }
    if let Some(source_profile_id) = source.source_profile_id.as_ref() {
        return Some(format!("profile:{source_profile_id}"));
    }
    source
        .template_id
        .as_deref()
        .map(|template_id| format!("template:{template_id}"))
}

fn default_profile_model_config() -> ProfileModelConfigSeed {
    ProfileModelConfigSeed {
        provider: "local".to_string(),
        model_name: "deterministic".to_string(),
        base_url: None,
        api: None,
        api_key_env: None,
        temperature_milli: None,
        max_output_tokens: None,
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
            }
            if let Some(defaults) = &profile.session_defaults {
                validate_optional_max(
                    self,
                    "invalid_history_window",
                    &format!("profiles[{index}].sessionDefaults.maxHistoryMessages"),
                    defaults.max_history_messages,
                    MAX_HISTORY_MESSAGES,
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
            if let Some(context_policy) = &profile.context_policy {
                validate_context_policy(self, index, context_policy);
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
        let mut agent_profiles = HashMap::new();
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
            if let Some(existing_profile) =
                agent_profiles.insert(session.agent_id.clone(), session.profile_id.clone())
            {
                if existing_profile != session.profile_id {
                    self.error(
                        "duplicate_agent_id",
                        format!("sessions[{index}].agentId"),
                        format!(
                            "configured agent {} is shared by profiles {} and {}; only same-profile session siblings may share an agent identity",
                            session.agent_id, existing_profile, session.profile_id
                        ),
                    );
                }
            }
            self.require_profile(
                &session.profile_id,
                &format!("sessions[{index}].profileId"),
                "session",
            );
            validate_session_workspace(self, index, session);
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
    validate_optional_max(
        validator,
        "invalid_resource_limits",
        &format!("{path}.maxDurationMs"),
        limits.max_duration_ms,
        MAX_RESOURCE_DURATION_MS,
    );
    validate_optional_max(
        validator,
        "invalid_resource_limits",
        &format!("{path}.maxDelegationDepth"),
        limits.max_delegation_depth,
        MAX_RESOURCE_DELEGATION_DEPTH,
    );
}

fn validate_session_workspace(
    validator: &mut RuntimeConfigValidator<'_>,
    index: usize,
    session: &SessionConfigDraft,
) {
    let path = format!("sessions[{index}].workspaceCwd");
    match session.workspace_cwd.as_deref() {
        Some(cwd) if cwd.trim().is_empty() => validator.error(
            "invalid_session_workspace",
            path,
            "workspaceCwd must not be blank",
        ),
        Some(cwd) if !Path::new(cwd).is_absolute() => validator.error(
            "invalid_session_workspace",
            path,
            "workspaceCwd must be an absolute path",
        ),
        None if session.kind == SessionKind::Full => validator.error(
            "session_workspace_required",
            path,
            "full sessions require an explicit workspaceCwd",
        ),
        _ => {}
    }
}

fn validate_context_policy(
    validator: &mut RuntimeConfigValidator<'_>,
    profile_index: usize,
    policy: &ProfileContextPolicy,
) {
    let path = format!("profiles[{profile_index}].contextPolicy");
    if !CONTEXT_STRATEGY_IDS.contains(&policy.strategy_id.as_str()) {
        validator.error(
            "context_strategy_unknown",
            format!("{path}.strategyId"),
            format!("unknown context strategy {}", policy.strategy_id),
        );
    }
    validate_percent(
        validator,
        &format!("{path}.compactAtPercent"),
        policy.compact_at_percent,
    );
    validate_percent(
        validator,
        &format!("{path}.targetPercentAfterCompaction"),
        policy.target_percent_after_compaction,
    );
    validate_percent(
        validator,
        &format!("{path}.maxContextPercentForWake"),
        policy.max_context_percent_for_wake,
    );
    if policy.target_percent_after_compaction >= policy.compact_at_percent {
        validator.error(
            "context_policy_target_not_below_trigger",
            format!("{path}.targetPercentAfterCompaction"),
            "targetPercentAfterCompaction must be lower than compactAtPercent",
        );
    }
    if policy.compact_at_percent > policy.max_context_percent_for_wake {
        validator.error(
            "context_policy_trigger_above_wake_guard",
            format!("{path}.compactAtPercent"),
            "compactAtPercent must not exceed maxContextPercentForWake",
        );
    }
    if !CONTEXT_DEBUG_VISIBILITY_VALUES.contains(&policy.debug_visibility.as_str()) {
        validator.error(
            "context_policy_debug_visibility_invalid",
            format!("{path}.debugVisibility"),
            "debugVisibility must be off, status, or verbose",
        );
    }
}

fn validate_percent(validator: &mut RuntimeConfigValidator<'_>, path: &str, value: u32) {
    if !(1..=100).contains(&value) {
        validator.error(
            "context_policy_percent_out_of_range",
            path,
            format!("{path} must be between 1 and 100"),
        );
    }
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

fn collect_id_diagnostic(
    diagnostics: &mut Vec<RuntimeConfigDiagnostic>,
    code: &str,
    path: &str,
    value: &str,
) {
    if !is_valid_component_id(value) {
        diagnostics.push(RuntimeConfigDiagnostic::error(
            code,
            path,
            format!("{path} {ID_PATTERN_DESCRIPTION}"),
        ));
    }
}

fn collect_non_empty_diagnostic(
    diagnostics: &mut Vec<RuntimeConfigDiagnostic>,
    code: &str,
    path: &str,
    value: &str,
) {
    if value.trim().is_empty() {
        diagnostics.push(RuntimeConfigDiagnostic::error(
            code,
            path,
            "value must not be empty",
        ));
    }
}

fn denied_new_session_plan(
    input: DeniedNewSessionPlanInput,
    denial_reason_code: &str,
    summary: &str,
) -> NewSessionControlPlan {
    NewSessionControlPlan {
        accepted: false,
        command_kind: input.command_kind,
        target: input.target,
        idempotency_key: input.idempotency_key,
        operator_reason: input.operator_reason,
        reason_code: input.reason_code,
        denial: Some(AdminControlPlanDenial {
            reason_code: denial_reason_code.to_string(),
            summary: summary.to_string(),
        }),
        preconditions: input.preconditions,
        actions: Vec::new(),
    }
}

fn denied_reload_mcp_plan(
    input: DeniedReloadMcpPlanInput,
    denial_reason_code: &str,
    summary: &str,
) -> ReloadMcpControlPlan {
    ReloadMcpControlPlan {
        accepted: false,
        command_kind: input.command_kind,
        target: input.target,
        idempotency_key: input.idempotency_key,
        operator_reason: input.operator_reason,
        reason_code: input.reason_code,
        denial: Some(AdminControlPlanDenial {
            reason_code: denial_reason_code.to_string(),
            summary: summary.to_string(),
        }),
        preconditions: input.preconditions,
        actions: Vec::new(),
    }
}

struct DeniedNewSessionPlanInput {
    command_kind: String,
    target: NewSessionControlTarget,
    idempotency_key: Option<String>,
    operator_reason: String,
    reason_code: String,
    preconditions: Vec<AdminControlPlanPrecondition>,
}

struct DeniedReloadMcpPlanInput {
    command_kind: String,
    target: ReloadMcpControlTarget,
    idempotency_key: Option<String>,
    operator_reason: String,
    reason_code: String,
    preconditions: Vec<AdminControlPlanPrecondition>,
}

fn satisfied_precondition(code: &str, summary: impl Into<String>) -> AdminControlPlanPrecondition {
    AdminControlPlanPrecondition {
        code: code.to_string(),
        status: AdminControlPlanPreconditionStatus::Satisfied,
        summary: summary.into(),
    }
}

fn failed_precondition(code: &str, summary: impl Into<String>) -> AdminControlPlanPrecondition {
    AdminControlPlanPrecondition {
        code: code.to_string(),
        status: AdminControlPlanPreconditionStatus::Failed,
        summary: summary.into(),
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

fn mentioned_agent_ids(
    mentions: &[String],
    aliases: &HashMap<String, AgentId>,
) -> HashSet<AgentId> {
    let mut result = HashSet::new();
    for mention in mentions {
        if is_valid_component_id(mention) {
            result.insert(AgentId::new(mention.clone()));
        }
        if let Some(agent_id) = aliases.get(mention) {
            result.insert(agent_id.clone());
        }
    }
    result
}

fn first_non_empty<T>(groups: Vec<Vec<T>>) -> Vec<T> {
    groups
        .into_iter()
        .find(|group| !group.is_empty())
        .unwrap_or_default()
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
    fn validates_engine_config_and_defaults_postgres_schema() {
        let storage = EngineStorageConfig::postgres_with_defaults(
            "postgres://crew/db",
            None,
            Some(4),
            Some(30_000),
        );
        assert_eq!(
            storage,
            EngineStorageConfig::Postgres {
                database_url: "postgres://crew/db".to_string(),
                schema: DEFAULT_POSTGRES_SCHEMA.to_string(),
                max_connections: Some(4),
                statement_timeout_ms: Some(30_000),
                backing_filesystem_path: None,
                filesystem_warning_free_percent: None,
            },
        );

        let result = validate_engine_config(&EngineConfig {
            engine_data_dir: "/tmp/rusty-crew".to_string(),
            clock: ClockConfig::Fixed {
                at: "2026-07-08T00:00:00Z".to_string(),
            },
            default_turn_budget: 3,
            default_idle_timeout_ms: 1_000,
            storage: Some(storage),
        });
        assert!(result.ok(), "{:?}", result.diagnostics);
    }

    #[test]
    fn rejects_invalid_engine_config_values() {
        let result = validate_engine_config(&EngineConfig {
            engine_data_dir: " ".to_string(),
            clock: ClockConfig::Fixed { at: String::new() },
            default_turn_budget: 0,
            default_idle_timeout_ms: 0,
            storage: Some(EngineStorageConfig::Postgres {
                database_url: String::new(),
                schema: String::new(),
                max_connections: Some(0),
                statement_timeout_ms: Some(0),
                backing_filesystem_path: None,
                filesystem_warning_free_percent: Some(101),
            }),
        });

        assert_codes(
            &result,
            &[
                "engine_data_dir_required",
                "fixed_clock_required",
                "default_turn_budget_required",
                "default_idle_timeout_required",
                "postgres_database_url_required",
                "postgres_schema_required",
                "postgres_max_connections_invalid",
                "postgres_statement_timeout_invalid",
                "filesystem_warning_free_percent_invalid",
            ],
        );
    }

    #[test]
    fn plans_new_session_archive_create_and_rebind_in_rust() {
        let plan = plan_new_session_control(&NewSessionControlPlanInput {
            command: AdminControlPlanCommand {
                command_kind: "new_session".to_string(),
                target_session_id: Some("session-alpha".to_string()),
                request_id: Some("req-new".to_string()),
                idempotency_key: Some("idem-new".to_string()),
                operator_reason: Some("fresh planning context".to_string()),
                operator_reason_code: Some("slash_command_new".to_string()),
            },
            template: Some(NewSessionControlTemplate {
                agent_id: AgentId::new("agent-alpha"),
                profile_id: ProfileId::new("prime"),
                kind: SessionKind::Full,
                workspace_cwd: Some("/home/dev/rusty-crew".to_string()),
                channel_binding_id: Some("binding-alpha".to_string()),
                channel_id: Some("crew-room".to_string()),
                tool_profile_key: Some("prime-tools".to_string()),
            }),
            generated_session_id: Some("session-alpha-new".to_string()),
            rebind_handler_available: true,
        });

        assert!(plan.accepted, "{:?}", plan.denial);
        assert_eq!(plan.denial, None);
        assert_eq!(plan.reason_code, "slash_command_new");
        assert_eq!(
            plan.actions
                .iter()
                .map(|action| action.action.clone())
                .collect::<Vec<_>>(),
            vec![
                NewSessionControlActionKind::ArchiveSession,
                NewSessionControlActionKind::CreateSession,
                NewSessionControlActionKind::RebindChannel,
            ],
        );
        assert!(plan.preconditions.iter().all(|precondition| {
            precondition.status == AdminControlPlanPreconditionStatus::Satisfied
        }));
    }

    #[test]
    fn denies_new_session_when_identity_is_not_distinct() {
        let plan = plan_new_session_control(&NewSessionControlPlanInput {
            command: AdminControlPlanCommand {
                command_kind: "new_session".to_string(),
                target_session_id: Some("session-alpha".to_string()),
                request_id: Some("req-new".to_string()),
                idempotency_key: None,
                operator_reason: None,
                operator_reason_code: None,
            },
            template: Some(NewSessionControlTemplate {
                agent_id: AgentId::new("agent-alpha"),
                profile_id: ProfileId::new("prime"),
                kind: SessionKind::Full,
                workspace_cwd: Some("/home/dev/rusty-crew".to_string()),
                channel_binding_id: None,
                channel_id: None,
                tool_profile_key: None,
            }),
            generated_session_id: Some("session-alpha".to_string()),
            rebind_handler_available: false,
        });

        assert!(!plan.accepted);
        assert_eq!(
            plan.denial
                .as_ref()
                .map(|denial| denial.reason_code.as_str()),
            Some("new_session_identity_not_distinct"),
        );
        assert!(plan.actions.is_empty());
    }

    #[test]
    fn denies_new_session_without_required_channel_rebind() {
        let plan = plan_new_session_control(&NewSessionControlPlanInput {
            command: AdminControlPlanCommand {
                command_kind: "new_session".to_string(),
                target_session_id: Some("session-alpha".to_string()),
                request_id: Some("req-new".to_string()),
                idempotency_key: None,
                operator_reason: None,
                operator_reason_code: None,
            },
            template: Some(NewSessionControlTemplate {
                agent_id: AgentId::new("agent-alpha"),
                profile_id: ProfileId::new("prime"),
                kind: SessionKind::Full,
                workspace_cwd: Some("/home/dev/rusty-crew".to_string()),
                channel_binding_id: Some("binding-alpha".to_string()),
                channel_id: None,
                tool_profile_key: None,
            }),
            generated_session_id: Some("session-beta".to_string()),
            rebind_handler_available: false,
        });

        assert!(!plan.accepted);
        assert_eq!(
            plan.denial
                .as_ref()
                .map(|denial| denial.reason_code.as_str()),
            Some("missing_channel_rebind"),
        );
        assert!(plan.preconditions.iter().any(|precondition| {
            precondition.code == "channel_rebind_available"
                && precondition.status == AdminControlPlanPreconditionStatus::Failed
        }));
    }

    #[test]
    fn denies_full_new_session_without_canonical_workspace_before_actions() {
        let plan = plan_new_session_control(&NewSessionControlPlanInput {
            command: AdminControlPlanCommand {
                command_kind: "new_session".to_string(),
                target_session_id: Some("session-alpha".to_string()),
                request_id: Some("req-new".to_string()),
                idempotency_key: None,
                operator_reason: None,
                operator_reason_code: None,
            },
            template: Some(NewSessionControlTemplate {
                agent_id: AgentId::new("agent-alpha"),
                profile_id: ProfileId::new("prime"),
                kind: SessionKind::Full,
                workspace_cwd: None,
                channel_binding_id: None,
                channel_id: None,
                tool_profile_key: None,
            }),
            generated_session_id: Some("session-beta".to_string()),
            rebind_handler_available: false,
        });

        assert!(!plan.accepted);
        assert_eq!(
            plan.denial
                .as_ref()
                .map(|denial| denial.reason_code.as_str()),
            Some("session_workspace_missing"),
        );
        assert!(plan.actions.is_empty());
    }

    #[test]
    fn plans_reload_mcp_surface_in_rust() {
        let plan = plan_reload_mcp_control(&ReloadMcpControlPlanInput {
            command: AdminControlPlanCommand {
                command_kind: "reload_mcp".to_string(),
                target_session_id: Some("session-alpha".to_string()),
                request_id: Some("req-reload".to_string()),
                idempotency_key: Some("idem-reload".to_string()),
                operator_reason: Some("refresh tool catalog".to_string()),
                operator_reason_code: Some("slash_reload_mcp".to_string()),
            },
            binding: Some(ReloadMcpControlBinding {
                binding_id: "mcp-alpha".to_string(),
                session_id: "session-alpha".to_string(),
                profile_id: ProfileId::new("prime"),
                tool_profile_key: Some("prime-mcp".to_string()),
                endpoint_ref: Some("config://mcp/den".to_string()),
            }),
            reload_handler_available: true,
        });

        assert!(plan.accepted, "{:?}", plan.denial);
        assert_eq!(plan.denial, None);
        assert_eq!(plan.target.binding_id.as_deref(), Some("mcp-alpha"));
        assert_eq!(plan.target.profile_id, Some(ProfileId::new("prime")));
        assert_eq!(plan.reason_code, "slash_reload_mcp");
        assert_eq!(plan.actions.len(), 1);
        assert_eq!(
            plan.actions[0].action,
            ReloadMcpControlActionKind::ReloadMcpSurface,
        );
        assert!(plan.preconditions.iter().all(|precondition| {
            precondition.status == AdminControlPlanPreconditionStatus::Satisfied
        }));
    }

    #[test]
    fn denies_reload_mcp_without_binding() {
        let plan = plan_reload_mcp_control(&ReloadMcpControlPlanInput {
            command: AdminControlPlanCommand {
                command_kind: "reload_mcp".to_string(),
                target_session_id: Some("session-alpha".to_string()),
                request_id: Some("req-reload".to_string()),
                idempotency_key: None,
                operator_reason: None,
                operator_reason_code: None,
            },
            binding: None,
            reload_handler_available: true,
        });

        assert!(!plan.accepted);
        assert_eq!(
            plan.denial
                .as_ref()
                .map(|denial| denial.reason_code.as_str()),
            Some("mcp_binding_not_found"),
        );
        assert!(plan.actions.is_empty());
    }

    #[test]
    fn denies_reload_mcp_binding_session_mismatch() {
        let plan = plan_reload_mcp_control(&ReloadMcpControlPlanInput {
            command: AdminControlPlanCommand {
                command_kind: "reload_mcp".to_string(),
                target_session_id: Some("session-alpha".to_string()),
                request_id: Some("req-reload".to_string()),
                idempotency_key: None,
                operator_reason: None,
                operator_reason_code: None,
            },
            binding: Some(ReloadMcpControlBinding {
                binding_id: "mcp-beta".to_string(),
                session_id: "session-beta".to_string(),
                profile_id: ProfileId::new("review"),
                tool_profile_key: Some("review-mcp".to_string()),
                endpoint_ref: None,
            }),
            reload_handler_available: true,
        });

        assert!(!plan.accepted);
        assert_eq!(
            plan.denial
                .as_ref()
                .map(|denial| denial.reason_code.as_str()),
            Some("mcp_binding_session_mismatch"),
        );
        assert!(plan.preconditions.iter().any(|precondition| {
            precondition.code == "mcp_binding_matches_session"
                && precondition.status == AdminControlPlanPreconditionStatus::Failed
        }));
    }

    #[test]
    fn denies_reload_mcp_without_handler() {
        let plan = plan_reload_mcp_control(&ReloadMcpControlPlanInput {
            command: AdminControlPlanCommand {
                command_kind: "reload_mcp".to_string(),
                target_session_id: Some("session-alpha".to_string()),
                request_id: Some("req-reload".to_string()),
                idempotency_key: None,
                operator_reason: None,
                operator_reason_code: None,
            },
            binding: Some(ReloadMcpControlBinding {
                binding_id: "mcp-alpha".to_string(),
                session_id: "session-alpha".to_string(),
                profile_id: ProfileId::new("prime"),
                tool_profile_key: Some("prime-mcp".to_string()),
                endpoint_ref: None,
            }),
            reload_handler_available: false,
        });

        assert!(!plan.accepted);
        assert_eq!(
            plan.denial
                .as_ref()
                .map(|denial| denial.reason_code.as_str()),
            Some("missing_mcp_reload_handler"),
        );
        assert!(plan.actions.is_empty());
    }

    #[test]
    fn plans_channel_ingress_route_with_mention_disambiguation() {
        let input = ChannelIngressRoutePlanInput {
            message: channel_message("unresolved-binding", None, vec!["reviewer".to_string()]),
            bindings: vec![
                channel_binding("binding-alpha", "agent-alpha", "session-alpha"),
                channel_binding("binding-beta", "agent-beta", "session-beta"),
            ],
            mention_aliases: HashMap::from([("reviewer".to_string(), AgentId::new("agent-beta"))]),
            system_agent_id: None,
            now: Some("2026-06-20T05:00:01.000Z".to_string()),
            seen_idempotency_keys: Vec::new(),
        };

        let plan = plan_channel_ingress_route(&input);

        assert_eq!(plan.status, ChannelIngressRouteDecision::Routed);
        assert_eq!(plan.reason_code, "channel_route_routed");
        let route = plan.route.expect("route should be planned");
        assert_eq!(route.to, AgentId::new("agent-beta"));
        assert_eq!(route.binding_id, "binding-beta");
        assert_eq!(route.session_id, Some(SessionId::new("session-beta")));
        assert_eq!(
            route.correlation_id,
            "channel:unresolved-binding:message-alpha"
        );
    }

    #[test]
    fn channel_ingress_route_reports_ambiguous_and_inactive_bindings() {
        let ambiguous = plan_channel_ingress_route(&ChannelIngressRoutePlanInput {
            message: channel_message("unresolved-binding", None, Vec::new()),
            bindings: vec![
                channel_binding("binding-alpha", "agent-alpha", "session-alpha"),
                channel_binding("binding-beta", "agent-beta", "session-beta"),
            ],
            mention_aliases: HashMap::new(),
            system_agent_id: None,
            now: Some("2026-06-20T05:00:01.000Z".to_string()),
            seen_idempotency_keys: Vec::new(),
        });
        assert_eq!(ambiguous.status, ChannelIngressRouteDecision::Ambiguous);
        assert_eq!(ambiguous.reason_code, "channel_route_ambiguous");
        assert_eq!(ambiguous.candidates.len(), 2);

        let inactive = plan_channel_ingress_route(&ChannelIngressRoutePlanInput {
            message: channel_message("binding-alpha", None, Vec::new()),
            bindings: vec![ChannelBindingConfigDraft {
                status: ExternalBindingStatusDraft::Degraded,
                ..channel_binding("binding-alpha", "agent-alpha", "session-alpha")
            }],
            mention_aliases: HashMap::new(),
            system_agent_id: None,
            now: Some("2026-06-20T05:00:01.000Z".to_string()),
            seen_idempotency_keys: Vec::new(),
        });
        assert_eq!(
            inactive.status,
            ChannelIngressRouteDecision::InactiveBinding
        );
        assert_eq!(inactive.reason_code, "channel_binding_inactive");
    }

    #[test]
    fn channel_ingress_route_reports_duplicate_and_expired_messages() {
        let duplicate = plan_channel_ingress_route(&ChannelIngressRoutePlanInput {
            message: channel_message("binding-alpha", None, Vec::new()),
            bindings: vec![channel_binding(
                "binding-alpha",
                "agent-alpha",
                "session-alpha",
            )],
            mention_aliases: HashMap::new(),
            system_agent_id: None,
            now: Some("2026-06-20T05:00:01.000Z".to_string()),
            seen_idempotency_keys: vec!["message-alpha".to_string()],
        });
        assert_eq!(duplicate.status, ChannelIngressRouteDecision::Duplicate);
        assert_eq!(duplicate.reason_code, "duplicate_idempotency_key");

        let expired = plan_channel_ingress_route(&ChannelIngressRoutePlanInput {
            message: channel_message("binding-alpha", None, Vec::new()),
            bindings: vec![channel_binding(
                "binding-alpha",
                "agent-alpha",
                "session-alpha",
            )],
            mention_aliases: HashMap::new(),
            system_agent_id: None,
            now: Some("2026-06-20T05:00:10.000Z".to_string()),
            seen_idempotency_keys: Vec::new(),
        });
        assert_eq!(expired.status, ChannelIngressRouteDecision::Expired);
        assert_eq!(expired.reason_code, "message_ttl_expired");
    }

    #[test]
    fn den_product_ingress_policy_allows_observe_only() {
        let observed = plan_den_product_ingress_policy(&DenProductIngressPolicyInput {
            operation: "observe".to_string(),
            entity_kind: "assignment".to_string(),
            entity_id: "assignment-1".to_string(),
            project_id: Some("rusty-crew".to_string()),
        });
        assert_eq!(observed.status, DenProductIngressPolicyStatus::Allowed);
        assert_eq!(observed.reason_code, "den_product_observe_allowed");
        assert!(!observed.lifecycle_operation);

        for operation in ["claim", "complete", "retry", "expire"] {
            let denied = plan_den_product_ingress_policy(&DenProductIngressPolicyInput {
                operation: operation.to_string(),
                entity_kind: "assignment".to_string(),
                entity_id: "assignment-1".to_string(),
                project_id: Some("rusty-crew".to_string()),
            });
            assert_eq!(denied.status, DenProductIngressPolicyStatus::Denied);
            assert_eq!(denied.reason_code, "adapter_lifecycle_operation_denied");
            assert!(denied.lifecycle_operation);
        }
    }

    #[test]
    fn profile_registry_mutation_reports_revision_mismatch() {
        let plan = plan_profile_registry_mutation(&ProfileRegistryMutationRequest {
            profile_id: ProfileId::new("runner"),
            kind: ProfileRegistryMutationKind::Update,
            mode: ProfileRegistryMutationMode::Plan,
            current: registry_record("runner"),
            body_json: json!({
                "expectedRevision": 3,
                "displayName": "Updated Runner"
            }),
            now: "2026-07-06T00:00:00.000Z".to_string(),
        })
        .expect("profile registry mutation should plan");

        assert!(!plan.ok);
        assert_eq!(plan.expected_revision, 3);
        assert_eq!(plan.next.display_name.as_deref(), Some("Updated Runner"));
        assert_codes(
            &RuntimeConfigValidationResult {
                diagnostics: plan.diagnostics,
            },
            &["profile_registry_revision_mismatch"],
        );
    }

    #[test]
    fn profile_registry_lifecycle_plan_updates_runtime_refs_and_effects() {
        let plan = plan_profile_registry_mutation(&ProfileRegistryMutationRequest {
            profile_id: ProfileId::new("runner"),
            kind: ProfileRegistryMutationKind::Lifecycle,
            mode: ProfileRegistryMutationMode::Apply,
            current: registry_record("runner"),
            body_json: json!({
                "expectedRevision": 7,
                "lifecycleStatus": "decommissioned"
            }),
            now: "2026-07-06T00:00:00.000Z".to_string(),
        })
        .expect("profile registry lifecycle should plan");

        assert!(plan.ok, "{:?}", plan.diagnostics);
        assert_eq!(
            plan.next.lifecycle_status,
            ProfileRegistryLifecycleStatus::Decommissioned
        );
        assert!(plan
            .next
            .derived_runtime_refs
            .iter()
            .all(|reference| reference.status == "disabled"));
        assert_eq!(
            plan.implications.lifecycle_effects,
            "archive_active_sessions_and_unregister_brain"
        );
        assert!(plan.implications.runtime_rebuild_recommended);
    }

    #[test]
    fn profile_registry_prompt_plan_preserves_markdown_and_next_write() {
        let plan = plan_profile_registry_mutation(&ProfileRegistryMutationRequest {
            profile_id: ProfileId::new("runner"),
            kind: ProfileRegistryMutationKind::Prompt,
            mode: ProfileRegistryMutationMode::Apply,
            current: registry_record("runner"),
            body_json: json!({
                "expectedRevision": 7,
                "soulMarkdown": "# Soul\n\nKeep exact spacing.  ",
                "memoryMarkdown": null
            }),
            now: "2026-07-06T00:00:00.000Z".to_string(),
        })
        .expect("profile registry prompt should plan");

        assert!(plan.ok, "{:?}", plan.diagnostics);
        assert_eq!(
            plan.next.prompt_soul_markdown.as_deref(),
            Some("# Soul\n\nKeep exact spacing.  ")
        );
        assert_eq!(plan.next.prompt_memory_markdown, None);
        assert_eq!(
            plan.next_write.prompt_soul_markdown.as_deref(),
            Some("# Soul\n\nKeep exact spacing.  ")
        );
        assert_eq!(plan.next_write.now, "2026-07-06T00:00:00.000Z");
    }

    #[test]
    fn validates_a_runtime_config_graph() {
        let result = validate_runtime_config_draft(&valid_draft(), &[profile("runner")]);
        assert!(result.ok(), "{:?}", result.diagnostics);
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn allows_same_profile_agent_siblings_but_rejects_unscoped_binding_ambiguity() {
        let mut draft = valid_draft();
        let mut sibling = draft.sessions[0].clone();
        sibling.session_id = SessionId::new("runner-session-sibling");
        sibling.workspace_cwd = Some("/tmp/rusty-crew/sibling".to_string());
        draft.sessions.push(sibling);

        let valid = validate_runtime_config_draft(&draft, &[profile("runner")]);
        assert!(valid.ok(), "{:?}", valid.diagnostics);

        draft.channel_bindings[0].session_id = None;
        let ambiguous = validate_runtime_config_draft(&draft, &[profile("runner")]);
        assert_codes(&ambiguous, &["binding_target_ambiguous"]);
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
            max_duration_ms: Some(MAX_RESOURCE_DURATION_MS + 1),
            max_delegation_depth: Some(MAX_RESOURCE_DELEGATION_DEPTH + 1),
        });
        draft.scheduled_jobs[0].schedule = "not a cron".to_string();

        let result = validate_runtime_config_draft(&draft, &[profile("runner")]);
        assert_codes(
            &result,
            &[
                "invalid_session_id",
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

    #[test]
    fn plans_runtime_config_with_profile_expansions_and_defaults() {
        let mut draft = RuntimeConfigDraft {
            profiles_dir: "/tmp/rusty-crew/profiles".to_string(),
            skills_dir: None,
            brains: vec![BrainConfigDraft {
                implementation_id: BrainImplementationId::new("runner-brain"),
                profile_id: ProfileId::new("runner"),
            }],
            sessions: vec![SessionConfigDraft {
                session_id: SessionId::new("runner-session"),
                agent_id: AgentId::new("runner-agent"),
                profile_id: ProfileId::new("runner"),
                kind: SessionKind::Full,
                workspace_cwd: Some("/tmp/rusty-crew/work".to_string()),
                resource_limits: None,
                owner_id: None,
                history_window: None,
                max_history_messages: None,
            }],
            scheduled_jobs: Vec::new(),
            channel_bindings: Vec::new(),
            mcp_bindings: Vec::new(),
        };
        let runner = profile("runner");

        let plan = plan_runtime_config(&RuntimeConfigValidationInput {
            runtime_config: draft.clone(),
            profiles: vec![runner.clone()],
        });

        assert!(plan.ok(), "{:?}", plan.diagnostics);
        assert_eq!(plan.derived_scheduled_jobs.len(), 1);
        assert_eq!(
            plan.derived_scheduled_jobs[0],
            ScheduledJobConfigDraft {
                id: "background-review-runner".to_string(),
                schedule: "0 3 * * *".to_string(),
                shape: ScheduledJobShape::HostJob,
                job_kind: Some("runtime.review.memory_skills".to_string()),
                target_session_id: None,
                script: None,
                delivery_channel_id: None,
            }
        );
        assert_eq!(plan.derived_mcp_bindings.len(), 1);
        assert_eq!(
            plan.derived_mcp_bindings[0],
            McpBindingConfigDraft {
                binding_id: "runner-mcp".to_string(),
                adapter_id: AdapterId::new("mcp-ts-main"),
                agent_id: AgentId::new("runner-agent"),
                instance_id: None,
                session_id: Some(SessionId::new("runner-session")),
                profile_id: ProfileId::new("runner"),
                server_names: vec!["den".to_string()],
                endpoint_ref: "config://mcp/runner".to_string(),
                transport: "streamable_http".to_string(),
                tool_profile_key: "runner".to_string(),
                status: ExternalBindingStatusDraft::Active,
            }
        );
        let expanded_session = &plan.runtime_config.sessions[0];
        assert_eq!(expanded_session.owner_id.as_deref(), Some("owner"));
        assert_eq!(expanded_session.max_history_messages, Some(500));

        draft.scheduled_jobs = plan.derived_scheduled_jobs.clone();
        draft.mcp_bindings = plan.derived_mcp_bindings.clone();
        let idempotent = plan_runtime_config(&RuntimeConfigValidationInput {
            runtime_config: draft,
            profiles: vec![runner],
        });
        assert!(idempotent.ok(), "{:?}", idempotent.diagnostics);
        assert!(idempotent.derived_scheduled_jobs.is_empty());
        assert!(idempotent.derived_mcp_bindings.is_empty());
    }

    #[test]
    fn plans_unique_unscoped_bindings_with_canonical_session_targets() {
        let mut draft = valid_draft();
        draft.channel_bindings[0].session_id = None;
        draft.mcp_bindings[0].session_id = None;

        let validation = validate_runtime_config_draft(&draft, &[profile("runner")]);
        assert!(validation.ok(), "{:?}", validation.diagnostics);

        let plan = plan_runtime_config(&RuntimeConfigValidationInput {
            runtime_config: draft,
            profiles: vec![profile("runner")],
        });

        assert!(plan.ok(), "{:?}", plan.diagnostics);
        assert_eq!(
            plan.runtime_config.channel_bindings[0].session_id,
            Some(SessionId::new("runner-session"))
        );
        assert_eq!(
            plan.runtime_config.mcp_bindings[0].session_id,
            Some(SessionId::new("runner-session"))
        );
    }

    #[test]
    fn plans_runtime_config_reports_invalid_expanded_graph() {
        let mut draft = valid_draft();
        draft.channel_bindings[0].agent_id = AgentId::new("wrong-agent");
        draft.scheduled_jobs.push(ScheduledJobConfigDraft {
            id: "script-job".to_string(),
            schedule: "0 1 * * *".to_string(),
            shape: ScheduledJobShape::ScriptOnly,
            job_kind: None,
            target_session_id: None,
            script: Some("echo hi".to_string()),
            delivery_channel_id: None,
        });

        let plan = plan_runtime_config(&RuntimeConfigValidationInput {
            runtime_config: draft,
            profiles: vec![profile("runner")],
        });

        assert!(!plan.ok());
        assert_codes(
            &RuntimeConfigValidationResult {
                diagnostics: plan.diagnostics,
            },
            &["binding_session_mismatch", "scheduled_job_not_executable"],
        );
    }

    #[test]
    fn validates_profile_context_policy_in_rust() {
        let mut runner = profile("runner");
        runner.context_policy = Some(ProfileContextPolicy {
            enabled: true,
            strategy_id: "mystery_strategy".to_string(),
            auto_compaction_enabled: true,
            compact_at_percent: 110,
            target_percent_after_compaction: 80,
            max_context_percent_for_wake: 75,
            debug_visibility: "loud".to_string(),
            include_debug_events_in_model_context: true,
            strategy_config: json!({}),
        });

        let result = validate_runtime_config_input(&RuntimeConfigValidationInput {
            runtime_config: valid_draft(),
            profiles: vec![runner],
        });

        assert_codes(
            &result,
            &[
                "context_strategy_unknown",
                "context_policy_percent_out_of_range",
                "context_policy_trigger_above_wake_guard",
                "context_policy_debug_visibility_invalid",
            ],
        );
    }

    #[test]
    fn accepts_roleplay_scene_aware_context_policy() {
        let mut runner = profile("runner");
        runner.context_policy.as_mut().unwrap().strategy_id =
            "roleplay_scene_aware_compaction".to_string();

        let result = validate_runtime_config_input(&RuntimeConfigValidationInput {
            runtime_config: valid_draft(),
            profiles: vec![runner],
        });

        assert!(
            result.ok(),
            "unexpected diagnostics: {:?}",
            result.diagnostics
        );
    }

    #[test]
    fn plans_delegated_role_lifecycle_with_inherited_limits() {
        let plan = plan_delegated_role_lifecycle(&DelegatedRoleLifecyclePlanInput {
            parent_session: DelegatedRoleParentSession {
                session_id: SessionId::new("parent-session"),
                agent_id: AgentId::new("parent-agent"),
                kind: SessionKind::Full,
                resource_limits: Some(ResourceLimits {
                    max_duration_ms: Some(60_000),
                    max_delegation_depth: Some(2),
                }),
            },
            delegated_session_id: "parent-session:delegated:wake-1:0".to_string(),
            delegated_agent_id: "agent:parent-session:delegated:wake-1:0".to_string(),
            profile_id: ProfileId::new("coder-profile"),
            tool_profile_key: Some("bounded-coder".to_string()),
            requested_resource_limits: Some(ResourceLimits {
                max_duration_ms: Some(30_000),
                max_delegation_depth: None,
            }),
            requested_workspace_constraint: Some(DelegatedWorkspaceConstraint {
                cwd: "/home/dev/rusty-crew".to_string(),
            }),
            source_wake_id: "wake-1".to_string(),
            source_action_index: 0,
            task_id: Some(TaskId::new("4737")),
            correlation_id: None,
        });

        assert!(plan.accepted, "{:?}", plan.diagnostics);
        assert_eq!(plan.reason_code, "delegated_role_lifecycle_planned");
        assert_eq!(plan.kind, SessionKind::Delegated);
        assert_eq!(
            plan.resource_limits,
            ResourceLimits {
                max_duration_ms: Some(30_000),
                max_delegation_depth: Some(1),
            }
        );
        assert_eq!(
            plan.workspace_constraint,
            Some(DelegatedWorkspaceConstraint {
                cwd: "/home/dev/rusty-crew".to_string(),
            })
        );
        assert_eq!(plan.tool_profile_key.as_deref(), Some("bounded-coder"));
        assert_eq!(plan.correlation_id, "delegation:wake-1:0");
    }

    #[test]
    fn rejects_delegated_role_lifecycle_escalation() {
        let plan = plan_delegated_role_lifecycle(&DelegatedRoleLifecyclePlanInput {
            parent_session: DelegatedRoleParentSession {
                session_id: SessionId::new("parent-session"),
                agent_id: AgentId::new("parent-agent"),
                kind: SessionKind::Delegated,
                resource_limits: Some(ResourceLimits {
                    max_duration_ms: Some(30_000),
                    max_delegation_depth: Some(0),
                }),
            },
            delegated_session_id: "parent-session".to_string(),
            delegated_agent_id: "parent-agent".to_string(),
            profile_id: ProfileId::new("coder-profile"),
            tool_profile_key: Some(" ".to_string()),
            requested_resource_limits: Some(ResourceLimits {
                max_duration_ms: Some(60_000),
                max_delegation_depth: Some(2),
            }),
            requested_workspace_constraint: Some(DelegatedWorkspaceConstraint {
                cwd: " ".to_string(),
            }),
            source_wake_id: " ".to_string(),
            source_action_index: 4,
            task_id: None,
            correlation_id: Some(" ".to_string()),
        });

        assert!(!plan.accepted);
        assert_codes(
            &RuntimeConfigValidationResult {
                diagnostics: plan.diagnostics.clone(),
            },
            &[
                "invalid_tool_profile_key",
                "invalid_source_wake_id",
                "invalid_correlation_id",
                "delegated_session_matches_parent",
                "delegated_agent_matches_parent",
                "delegation_depth_exhausted",
                "delegation_depth_escalation",
                "delegation_duration_escalation",
                "invalid_delegated_workspace_constraint",
            ],
        );
    }

    #[test]
    fn plans_create_profile_with_defaults_without_mutating_runtime() {
        let input = CreateProfilePlanInput {
            runtime_config: valid_draft(),
            profiles: vec![profile("runner")],
            profile_registry: Vec::new(),
            request: CreateProfileRequest {
                model_config_id: None,
                profile_id: "field-created-profile".to_string(),
                display_name: Some("Field Created Profile".to_string()),
                soul_markdown: Some("# Field soul\n\n  Preserve spacing.\n".to_string()),
                memory_markdown: Some("# Field memory\n".to_string()),
                agent_id: None,
                session_id: None,
                implementation_id: None,
                kind: None,
                workspace_cwd: Some("/home/dev/rusty-crew".to_string()),
                provider_alias: None,
                external_message_delivery_policy: None,
                model_config: None,
                brain: None,
                mcp_bindings: Vec::new(),
                mcp_tool_profile: None,
                source: Some(CreateProfileSourceRequest {
                    template_id: Some("starter".to_string()),
                    source_profile_id: None,
                    source_bundle_path: None,
                }),
                now: Some("2026-06-26T09:30:00.000Z".to_string()),
                profile_file_exists: false,
            },
        };

        let plan = plan_create_profile(&input);
        assert!(plan.ok(), "{:?}", plan.diagnostics);
        assert!(plan.diagnostics.is_empty());
        let seed = plan.profile_seed.expect("profile seed should be planned");
        assert_eq!(seed.profile_id, ProfileId::new("field-created-profile"));
        assert_eq!(seed.display_name.as_deref(), Some("Field Created Profile"));
        assert_eq!(seed.model_config.provider, "local");
        assert_eq!(seed.model_config.model_name, "deterministic");
        assert_eq!(seed.brain.module.as_deref(), Some("local"));
        assert_eq!(
            seed.external_message_delivery_policy,
            ExternalMessageDeliveryPolicy::ImmediateSteer
        );
        assert_eq!(seed.skills_mode, "all");
        assert_eq!(
            plan.runtime_brain.expect("brain should be planned"),
            BrainConfigDraft {
                implementation_id: BrainImplementationId::new("field-created-profile-brain"),
                profile_id: ProfileId::new("field-created-profile"),
            }
        );
        assert_eq!(
            plan.runtime_session.expect("session should be planned"),
            SessionConfigDraft {
                session_id: SessionId::new("field-created-profile-session"),
                agent_id: AgentId::new("field-created-profile"),
                profile_id: ProfileId::new("field-created-profile"),
                kind: SessionKind::Full,
                workspace_cwd: Some("/home/dev/rusty-crew".to_string()),
                resource_limits: None,
                owner_id: None,
                history_window: None,
                max_history_messages: None,
            }
        );
        assert_eq!(plan.profile_mcp_config, None);
        assert!(plan.runtime_mcp_bindings.is_empty());
        let registry_write = plan
            .registry_write
            .expect("registry write should be planned first");
        assert_eq!(
            registry_write.profile_id,
            ProfileId::new("field-created-profile")
        );
        assert_eq!(
            registry_write.lifecycle_status,
            ProfileRegistryLifecycleStatus::Active
        );
        assert_eq!(
            registry_write.prompt_soul_markdown.as_deref(),
            Some("# Field soul\n\n  Preserve spacing.\n")
        );
        assert_eq!(
            registry_write.prompt_memory_markdown.as_deref(),
            Some("# Field memory\n")
        );
        assert_eq!(
            registry_write.active_runtime_settings_json["externalMessageDeliveryPolicy"],
            "immediate_steer"
        );
        assert_eq!(
            registry_write.import_export.imported_from.as_deref(),
            Some("template:starter")
        );
        assert_eq!(registry_write.derived_runtime_refs.len(), 2);
        assert!(registry_write
            .derived_runtime_refs
            .iter()
            .any(|runtime_ref| runtime_ref.ref_kind == "session"
                && runtime_ref.ref_id == "field-created-profile-session"));
        assert_eq!(plan.file_asset_actions.len(), 1);
        assert_eq!(
            plan.file_asset_actions[0].kind,
            CreateProfileFileAssetActionKind::WriteProfileJson
        );
        assert_eq!(
            plan.file_asset_actions[0].relative_path,
            "field-created-profile.json"
        );
        assert!(!plan.file_asset_actions[0].overwrite);
        assert_eq!(plan.derived_runtime_actions.len(), 2);
        assert_eq!(
            plan.derived_runtime_actions
                .iter()
                .map(|action| action.ref_kind.as_str())
                .collect::<Vec<_>>(),
            vec!["brain", "session"]
        );

        let mut missing_workspace = input.clone();
        missing_workspace.request.workspace_cwd = None;
        let missing_plan = plan_create_profile(&missing_workspace);
        assert!(!missing_plan.ok());
        assert!(missing_plan.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "session_workspace_required"
                && diagnostic.path.as_deref() == Some("request.workspaceCwd")
        }));

        let mut relative_workspace = input;
        relative_workspace.request.workspace_cwd = Some("relative/repo".to_string());
        let relative_plan = plan_create_profile(&relative_workspace);
        assert!(!relative_plan.ok());
        assert!(relative_plan.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "invalid_session_workspace"
                && diagnostic.path.as_deref() == Some("request.workspaceCwd")
        }));
    }

    #[test]
    fn plans_create_profile_with_explicit_runtime_mcp_bindings() {
        let input = CreateProfilePlanInput {
            runtime_config: valid_draft(),
            profiles: vec![profile("runner")],
            profile_registry: Vec::new(),
            request: CreateProfileRequest {
                model_config_id: None,
                profile_id: "field-created-profile".to_string(),
                display_name: None,
                soul_markdown: None,
                memory_markdown: None,
                agent_id: None,
                session_id: None,
                implementation_id: None,
                kind: None,
                workspace_cwd: Some("/home/dev/rusty-crew".to_string()),
                provider_alias: None,
                external_message_delivery_policy: Some(
                    ExternalMessageDeliveryPolicy::SerialNextTurn,
                ),
                model_config: None,
                brain: None,
                mcp_bindings: vec![
                    CreateProfileMcpBindingRequest {
                        server_id: "den".to_string(),
                        binding_id: None,
                        adapter_id: None,
                        server_names: None,
                        transport: None,
                        tool_profile_key: Some("planner".to_string()),
                    },
                    CreateProfileMcpBindingRequest {
                        server_id: "filesystem".to_string(),
                        binding_id: Some("field-files-mcp".to_string()),
                        adapter_id: Some("mcp-ts-files".to_string()),
                        server_names: Some(vec!["files".to_string()]),
                        transport: Some("streamable_http".to_string()),
                        tool_profile_key: Some("files".to_string()),
                    },
                ],
                mcp_tool_profile: None,
                source: None,
                now: None,
                profile_file_exists: false,
            },
        };

        let plan = plan_create_profile(&input);
        assert!(plan.ok(), "{:?}", plan.diagnostics);
        assert_eq!(plan.profile_mcp_config, None);
        assert_eq!(
            plan.profile_seed
                .as_ref()
                .expect("profile seed should be planned")
                .external_message_delivery_policy,
            ExternalMessageDeliveryPolicy::SerialNextTurn
        );
        let registry_write = plan
            .registry_write
            .as_ref()
            .expect("registry write should be planned");
        assert_eq!(registry_write.prompt_soul_markdown, None);
        assert_eq!(registry_write.prompt_memory_markdown, None);
        assert_eq!(
            registry_write.active_runtime_settings_json["externalMessageDeliveryPolicy"],
            "serial_next_turn"
        );
        assert_eq!(plan.runtime_mcp_bindings.len(), 2);
        assert_eq!(
            plan.runtime_mcp_bindings[0],
            McpBindingConfigDraft {
                binding_id: "field-created-profile-mcp-1--session--field-created-profile-session"
                    .to_string(),
                adapter_id: AdapterId::new("mcp-ts-main"),
                agent_id: AgentId::new("field-created-profile"),
                instance_id: None,
                session_id: Some(SessionId::new("field-created-profile-session")),
                profile_id: ProfileId::new("field-created-profile"),
                server_names: vec!["den".to_string()],
                endpoint_ref: "config://mcp/den".to_string(),
                transport: "streamable_http".to_string(),
                tool_profile_key: "planner".to_string(),
                status: ExternalBindingStatusDraft::Active,
            }
        );
        assert_eq!(
            plan.runtime_mcp_bindings[1].binding_id,
            "field-files-mcp--session--field-created-profile-session"
        );
        assert_eq!(
            plan.runtime_mcp_bindings[1].endpoint_ref,
            "config://mcp/filesystem"
        );
        let desired_bindings = registry_write.active_runtime_settings_json["mcp_bindings"]
            .as_array()
            .expect("profile MCP intent should be stored as a desired binding list");
        assert_eq!(desired_bindings[0]["binding_id"], Value::Null);
        assert_eq!(desired_bindings[0]["server_id"], "den");
        assert!(desired_bindings[0].get("session_id").is_none());
        assert_eq!(desired_bindings[1]["binding_id"], "field-files-mcp");
        assert!(desired_bindings[1].get("session_id").is_none());
        assert_eq!(
            plan.registry_write
                .expect("registry write should be planned")
                .derived_runtime_refs
                .iter()
                .map(|runtime_ref| runtime_ref.ref_kind.as_str())
                .collect::<Vec<_>>(),
            vec!["brain", "session", "mcp_binding", "mcp_binding"]
        );
    }

    #[test]
    fn rejects_create_profile_duplicates_with_structured_diagnostics() {
        let input = CreateProfilePlanInput {
            runtime_config: valid_draft(),
            profiles: vec![profile("runner")],
            profile_registry: vec![ProfileRegistryRuntimeMetadata {
                profile_id: ProfileId::new("runner"),
                lifecycle_status: Some(ProfileRegistryLifecycleStatus::Active),
                revision: Some(7),
            }],
            request: CreateProfileRequest {
                model_config_id: None,
                profile_id: "runner".to_string(),
                display_name: None,
                soul_markdown: None,
                memory_markdown: None,
                agent_id: Some("runner-agent".to_string()),
                session_id: Some("runner-session".to_string()),
                implementation_id: Some("runner-brain".to_string()),
                kind: Some(SessionKind::Full),
                workspace_cwd: Some("/home/dev/rusty-crew".to_string()),
                provider_alias: None,
                external_message_delivery_policy: None,
                model_config: None,
                brain: None,
                mcp_bindings: Vec::new(),
                mcp_tool_profile: None,
                source: None,
                now: None,
                profile_file_exists: true,
            },
        };

        let plan = plan_create_profile(&input);
        assert!(!plan.ok());
        assert_eq!(plan.profile_seed, None);
        assert_codes(
            &RuntimeConfigValidationResult {
                diagnostics: plan.diagnostics,
            },
            &[
                "profile_file_exists",
                "duplicate_profile_id",
                "duplicate_profile_registry_record",
                "duplicate_profile_brain",
                "duplicate_brain_implementation_id",
                "duplicate_profile_session",
                "duplicate_session_id",
                "duplicate_agent_id",
            ],
        );
    }

    #[test]
    fn rejects_create_profile_invalid_ids_before_returning_plan_entries() {
        let input = CreateProfilePlanInput {
            runtime_config: valid_draft(),
            profiles: vec![profile("runner")],
            profile_registry: Vec::new(),
            request: CreateProfileRequest {
                model_config_id: None,
                profile_id: "../bad".to_string(),
                display_name: None,
                soul_markdown: None,
                memory_markdown: None,
                agent_id: None,
                session_id: None,
                implementation_id: None,
                kind: None,
                workspace_cwd: Some("/home/dev/rusty-crew".to_string()),
                provider_alias: None,
                external_message_delivery_policy: None,
                model_config: Some(ProfileModelConfigSeed {
                    provider: "".to_string(),
                    model_name: "".to_string(),
                    base_url: None,
                    api: None,
                    api_key_env: None,
                    temperature_milli: None,
                    max_output_tokens: None,
                }),
                brain: None,
                mcp_bindings: Vec::new(),
                mcp_tool_profile: Some("bad tool".to_string()),
                source: None,
                now: None,
                profile_file_exists: false,
            },
        };

        let plan = plan_create_profile(&input);
        assert!(!plan.ok());
        assert_eq!(plan.runtime_brain, None);
        assert_codes(
            &RuntimeConfigValidationResult {
                diagnostics: plan.diagnostics,
            },
            &[
                "invalid_profile_id",
                "invalid_agent_id",
                "invalid_session_id",
                "invalid_brain_implementation_id",
                "invalid_tool_profile_key",
                "invalid_model_provider",
                "invalid_model_name",
            ],
        );
    }

    #[test]
    fn shared_runtime_config_parity_fixture_validates_and_plans_create_profile() {
        // Temporary parity guard: TS still hand-loads service/profile files and
        // converts them into this Rust-owned validation shape. Keep this
        // fixture aligned with `validation-input.camel.json` until bridge
        // manifest/codegen generates the TS facade types.
        let input: RuntimeConfigValidationInput = fixture_json(
            include_str!(
                "../../../../fixtures/runtime-config-parity/valid/validation-input.snake.json"
            ),
            "/tmp/rusty-crew-config-parity",
        );

        let validation = validate_runtime_config_input(&input);
        assert!(validation.ok(), "{:?}", validation.diagnostics);

        let plan = plan_runtime_config(&input);
        assert!(plan.ok(), "{:?}", plan.diagnostics);
        assert_eq!(plan.runtime_config, input.runtime_config);
        assert!(plan.derived_scheduled_jobs.is_empty());
        assert!(plan.derived_mcp_bindings.is_empty());

        assert_eq!(input.runtime_config.brains.len(), 1);
        assert_eq!(input.runtime_config.sessions.len(), 1);
        assert_eq!(input.runtime_config.scheduled_jobs.len(), 3);
        assert_eq!(input.runtime_config.channel_bindings.len(), 1);
        assert_eq!(input.runtime_config.mcp_bindings.len(), 2);
        assert_eq!(
            input.runtime_config.channel_bindings[0].status,
            ExternalBindingStatusDraft::Disconnected
        );

        let request: CreateProfileRequest = fixture_json(
            include_str!(
                "../../../../fixtures/runtime-config-parity/valid/create-profile-request.snake.json"
            ),
            "/tmp/rusty-crew-config-parity",
        );
        let create_plan = plan_create_profile(&CreateProfilePlanInput {
            runtime_config: input.runtime_config,
            profiles: input.profiles,
            profile_registry: Vec::new(),
            request,
        });
        assert!(create_plan.ok(), "{:?}", create_plan.diagnostics);
        assert_eq!(
            create_plan
                .profile_seed
                .as_ref()
                .map(|seed| seed.profile_id.to_string()),
            Some("parity-created".to_string())
        );
        assert_eq!(
            create_plan
                .runtime_brain
                .as_ref()
                .map(|brain| brain.implementation_id.to_string()),
            Some("parity-created-brain".to_string())
        );
        assert_eq!(
            create_plan
                .runtime_session
                .as_ref()
                .map(|session| session.session_id.to_string()),
            Some("parity-created-session".to_string())
        );
        assert_eq!(
            create_plan
                .profile_mcp_config
                .as_ref()
                .and_then(|mcp| mcp.tool_profile.as_deref()),
            Some("planner")
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
                workspace_cwd: Some("/tmp/rusty-crew/work".to_string()),
                resource_limits: Some(ResourceLimits {
                    max_duration_ms: Some(60_000),
                    max_delegation_depth: Some(4),
                }),
                owner_id: Some("owner".to_string()),
                history_window: Some(SessionHistoryWindow {
                    max_messages: Some(200),
                }),
                max_history_messages: None,
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
                max_tokens_per_turn: Some(8_000),
            }),
            session_defaults: Some(ProfileSessionDefaults {
                owner_id: Some("owner".to_string()),
                max_history_messages: Some(500),
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
                ..ProfileBackgroundReviewConfig::default()
            }),
            channel_defaults: Some(ProfileChannelDefaults {
                wake_policy: Some(ChannelWakePolicy::Subscription),
            }),
            context_policy: Some(ProfileContextPolicy {
                enabled: true,
                strategy_id: "recent_window".to_string(),
                auto_compaction_enabled: false,
                compact_at_percent: 80,
                target_percent_after_compaction: 55,
                max_context_percent_for_wake: 95,
                debug_visibility: "status".to_string(),
                include_debug_events_in_model_context: false,
                strategy_config: json!({}),
            }),
        }
    }

    fn registry_record(profile_id: &str) -> ProfileRegistryRecord {
        ProfileRegistryRecord {
            profile_id: ProfileId::new(profile_id),
            lifecycle_status: ProfileRegistryLifecycleStatus::Active,
            display_name: Some("Runner".to_string()),
            summary: Some("Runs work".to_string()),
            default_session_kind: Some(SessionKind::Full),
            agent_id: Some(AgentId::new("runner-agent")),
            owner_id: Some("owner".to_string()),
            prompt_soul_markdown: Some("old soul".to_string()),
            prompt_memory_markdown: Some("old memory".to_string()),
            active_runtime_settings_json: json!({
                "providerAlias": "default",
            }),
            source_asset_refs: vec![ProfileRegistrySourceAssetRef {
                asset_kind: "profile_json".to_string(),
                path: "runner.json".to_string(),
                content_hash: None,
                last_seen_at: None,
                metadata_json: json!({}),
            }],
            derived_runtime_refs: vec![
                ProfileRegistryDerivedRuntimeRef {
                    ref_kind: "session".to_string(),
                    ref_id: "runner-session".to_string(),
                    status: "active".to_string(),
                    updated_at: None,
                    metadata_json: json!({}),
                },
                ProfileRegistryDerivedRuntimeRef {
                    ref_kind: "brain".to_string(),
                    ref_id: "runner-brain".to_string(),
                    status: "active".to_string(),
                    updated_at: None,
                    metadata_json: json!({}),
                },
            ],
            import_export: ProfileRegistryImportExportMetadata {
                imported_from: None,
                imported_at: None,
                exported_to: None,
                exported_at: None,
                metadata_json: json!({}),
            },
            revision: 7,
            created_at: "2026-07-05T00:00:00.000Z".to_string(),
            updated_at: "2026-07-05T00:00:00.000Z".to_string(),
        }
    }

    fn channel_message(
        binding_id: &str,
        runtime_agent_id: Option<&str>,
        mentions: Vec<String>,
    ) -> ChannelIngressRouteMessage {
        ChannelIngressRouteMessage {
            adapter_id: AdapterId::new("den-channel-main"),
            binding_id: binding_id.to_string(),
            provider: "den_channels".to_string(),
            external_channel_id: "crew-room".to_string(),
            external_thread_id: Some("thread-alpha".to_string()),
            external_user_id: "den-user-alpha".to_string(),
            body: "hello from channel".to_string(),
            mentions,
            expires_at: "2026-06-20T05:00:05.000Z".to_string(),
            idempotency_key: "message-alpha".to_string(),
            runtime_agent_id: runtime_agent_id.map(AgentId::new),
        }
    }

    fn channel_binding(
        binding_id: &str,
        agent_id: &str,
        session_id: &str,
    ) -> ChannelBindingConfigDraft {
        ChannelBindingConfigDraft {
            binding_id: binding_id.to_string(),
            adapter_id: AdapterId::new("den-channel-main"),
            provider: "den_channels".to_string(),
            agent_id: AgentId::new(agent_id),
            instance_id: None,
            session_id: Some(SessionId::new(session_id)),
            profile_id: ProfileId::new(format!("{agent_id}-profile")),
            external_channel_id: "crew-room".to_string(),
            external_thread_id: Some("thread-alpha".to_string()),
            external_user_id: None,
            conversation_project_id: None,
            conversation_channel_id: None,
            provider_subscription_id: None,
            status: ExternalBindingStatusDraft::Active,
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

    fn fixture_json<T>(raw: &str, root: &str) -> T
    where
        T: serde::de::DeserializeOwned,
    {
        serde_json::from_str(&raw.replace("__FIXTURE_ROOT__", root))
            .expect("shared runtime config parity fixture should deserialize")
    }
}
