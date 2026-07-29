use super::{
    validate_runtime_config_draft, BrainConfigDraft, ChannelBindingConfigDraft,
    ExternalBindingStatusDraft, McpBindingConfigDraft, ProfileBackgroundReviewConfig,
    ProfileBrainMetadata, ProfileContextPolicy, ProfileMcpConfig, ProfileRuntimeMetadata,
    ProfileRuntimeOptions, ProfileSessionDefaults, RuntimeConfigDiagnostic,
    RuntimeConfigDiagnosticSeverity, RuntimeConfigDraft, ScheduledJobConfigDraft,
    ScheduledJobShape, SessionConfigDraft, RUNTIME_REVIEW_MEMORY_SKILLS_JOB_KIND,
};
use rusty_crew_core_protocol::{
    AdapterId, AgentId, BrainImplementationId, ProfileId, ResourceLimits, SessionHistoryWindow,
    SessionId, SessionKind,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeGraphPlanInput {
    pub host_facts: RuntimeGraphHostFacts,
    #[serde(default)]
    pub service_defaults: RuntimeGraphServiceDefaults,
    pub runtime_config: RuntimeGraphSourceDraft,
    #[serde(default)]
    pub profiles: Vec<RuntimeGraphProfileSource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeGraphHostFacts {
    pub config_dir: String,
    pub engine_data_dir: String,
    pub default_workdir: Option<String>,
    #[serde(default)]
    pub postgres_database_url_env_present: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeGraphServiceDefaults {
    pub storage: Option<RuntimeGraphStorageSource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeGraphSourceDraft {
    pub profiles_dir: String,
    pub skills_dir: Option<String>,
    pub storage: Option<RuntimeGraphStorageSource>,
    #[serde(default)]
    pub brains: Vec<RuntimeGraphBrainSource>,
    #[serde(default)]
    pub sessions: Vec<RuntimeGraphSessionSource>,
    #[serde(default)]
    pub scheduled_jobs: Vec<RuntimeGraphScheduledJobSource>,
    #[serde(default)]
    pub channel_bindings: Vec<ChannelBindingConfigDraft>,
    #[serde(default)]
    pub mcp_bindings: Vec<McpBindingConfigDraft>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeGraphBrainSource {
    pub implementation_id: Option<String>,
    pub profile_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeGraphSessionSource {
    pub session_id: String,
    pub agent_id: String,
    pub profile_id: String,
    pub kind: Option<SessionKind>,
    pub resource_limits: Option<ResourceLimits>,
    pub owner_id: Option<String>,
    pub history_window: Option<SessionHistoryWindow>,
    pub max_history_messages: Option<u32>,
    pub local_tool_profile_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeGraphScheduledJobSource {
    pub id: String,
    pub schedule: String,
    pub shape: ScheduledJobShape,
    pub job_kind: Option<String>,
    pub target_session_id: Option<String>,
    #[serde(default)]
    pub payload: Value,
    pub script: Option<String>,
    pub delivery_channel_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeGraphStorageBackend {
    Sqlite,
    Postgres,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeGraphPostgresBootMode {
    Blocked,
    ProofAdmin,
    Active,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeGraphStorageSource {
    pub backend: String,
    pub sqlite: Option<RuntimeGraphSqliteStorageSource>,
    pub postgres: Option<RuntimeGraphPostgresStorageSource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeGraphSqliteStorageSource {
    pub path: Option<String>,
    pub wal: Option<bool>,
    pub busy_timeout_ms: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeGraphPostgresStorageSource {
    pub database_url_env: Option<String>,
    pub schema: Option<String>,
    pub boot_mode: Option<RuntimeGraphPostgresBootMode>,
    pub max_connections: Option<u32>,
    pub statement_timeout_ms: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeGraphProfileSource {
    pub profile_id: String,
    pub brain: Option<ProfileBrainMetadata>,
    pub runtime: Option<ProfileRuntimeOptions>,
    pub local_tool_profile_id: Option<String>,
    pub session_defaults: Option<ProfileSessionDefaults>,
    pub session_memory_prompt: Option<RuntimeGraphSessionMemoryPromptSource>,
    pub mcp_config: Option<ProfileMcpConfig>,
    pub background_review: Option<ProfileBackgroundReviewConfig>,
    pub context_policy: Option<ProfileContextPolicy>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeGraphSessionMemoryPromptSource {
    pub enabled: Option<bool>,
    pub max_records: Option<u32>,
    pub include_ancestors: Option<bool>,
    pub include_siblings: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeGraphPlan {
    pub accepted: bool,
    pub source_revision: String,
    pub runtime_config: RuntimeGraphEffectiveConfig,
    pub derived: Vec<RuntimeGraphDerivedRecord>,
    pub defaults_applied: Vec<RuntimeGraphDefaultRecord>,
    pub diagnostics: Vec<RuntimeConfigDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeGraphEffectiveConfig {
    pub profiles_dir: String,
    pub skills_dir: Option<String>,
    pub storage: RuntimeGraphStoragePlan,
    pub brains: Vec<BrainConfigDraft>,
    pub sessions: Vec<RuntimeGraphSessionPlan>,
    pub scheduled_jobs: Vec<RuntimeGraphScheduledJobPlan>,
    pub channel_bindings: Vec<ChannelBindingConfigDraft>,
    pub mcp_bindings: Vec<McpBindingConfigDraft>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeGraphSessionPlan {
    pub session_id: SessionId,
    pub agent_id: AgentId,
    pub profile_id: ProfileId,
    pub kind: SessionKind,
    pub resource_limits: ResourceLimits,
    pub owner_id: Option<String>,
    pub history_window: Option<SessionHistoryWindow>,
    pub max_history_messages: Option<u32>,
    pub local_tool_profile_id: Option<String>,
    pub context_policy_profile_id: Option<ProfileId>,
    pub session_memory_prompt_profile_id: Option<ProfileId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeGraphScheduledJobPlan {
    pub id: String,
    pub schedule: String,
    pub shape: ScheduledJobShape,
    pub job_kind: Option<String>,
    pub target_session_id: Option<SessionId>,
    #[serde(default)]
    pub payload: Value,
    pub script: Option<String>,
    pub delivery_channel_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeGraphStoragePlan {
    pub backend: RuntimeGraphStorageBackend,
    pub implementation_status: RuntimeGraphStorageImplementationStatus,
    pub sqlite: RuntimeGraphSqliteStoragePlan,
    pub postgres: RuntimeGraphPostgresStoragePlan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeGraphStorageImplementationStatus {
    Active,
    ProofAdminOnly,
    BlockedUnimplemented,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeGraphSqliteStoragePlan {
    pub path: String,
    pub effective_path: String,
    pub wal: bool,
    pub busy_timeout_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeGraphPostgresStoragePlan {
    pub database_url_env: String,
    pub schema: String,
    pub boot_mode: RuntimeGraphPostgresBootMode,
    pub max_connections: u32,
    pub statement_timeout_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeGraphDerivedRecord {
    pub kind: RuntimeGraphDerivedKind,
    pub id: String,
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeGraphDerivedKind {
    ScheduledJob,
    McpBinding,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeGraphDefaultRecord {
    pub path: String,
    pub source: RuntimeGraphDefaultSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeGraphDefaultSource {
    CanonicalProfileDefault,
    ServiceDefault,
    HostDefaultWorkdir,
    ProfileRuntimeDefault,
    ProfileSessionDefault,
}

pub fn plan_runtime_graph(input: &RuntimeGraphPlanInput) -> RuntimeGraphPlan {
    let source_revision = runtime_graph_source_revision(input);
    let mut diagnostics = Vec::new();
    let mut defaults_applied = Vec::new();
    let profiles = canonical_profiles(&input.profiles);
    let profiles_by_id: HashMap<ProfileId, &RuntimeGraphProfileSource> = input
        .profiles
        .iter()
        .map(|profile| (ProfileId::new(profile.profile_id.clone()), profile))
        .collect();
    let storage = plan_storage(input, &mut diagnostics);
    let brains = plan_brains(input, &mut defaults_applied, &mut diagnostics);
    let sessions = plan_sessions(input, &profiles_by_id, &mut defaults_applied);
    let (scheduled_jobs, mut derived_jobs) = plan_scheduled_jobs(input, &profiles_by_id, &sessions);
    let (mcp_bindings, mut derived_mcp) = plan_mcp_bindings(input, &profiles_by_id, &sessions);
    let mut channel_bindings = input.runtime_config.channel_bindings.clone();
    channel_bindings.sort_by(|left, right| left.binding_id.cmp(&right.binding_id));

    let validation_draft = RuntimeConfigDraft {
        profiles_dir: input.runtime_config.profiles_dir.clone(),
        skills_dir: input.runtime_config.skills_dir.clone(),
        brains: brains.clone(),
        sessions: sessions
            .iter()
            .map(|session| SessionConfigDraft {
                session_id: session.session_id.clone(),
                agent_id: session.agent_id.clone(),
                profile_id: session.profile_id.clone(),
                kind: session.kind.clone(),
                resource_limits: Some(session.resource_limits.clone()),
                owner_id: session.owner_id.clone(),
                history_window: session.history_window.clone(),
                max_history_messages: session.max_history_messages,
            })
            .collect(),
        scheduled_jobs: scheduled_jobs
            .iter()
            .map(|job| ScheduledJobConfigDraft {
                id: job.id.clone(),
                schedule: job.schedule.clone(),
                shape: job.shape.clone(),
                job_kind: job.job_kind.clone(),
                target_session_id: job.target_session_id.clone(),
                script: job.script.clone(),
                delivery_channel_id: job.delivery_channel_id.clone(),
            })
            .collect(),
        channel_bindings: channel_bindings.clone(),
        mcp_bindings: mcp_bindings.clone(),
    };
    diagnostics.extend(validate_runtime_config_draft(&validation_draft, &profiles).diagnostics);
    diagnostics.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.code.cmp(&right.code))
    });
    diagnostics.dedup_by(|left, right| left.code == right.code && left.path == right.path);
    defaults_applied.sort_by(|left, right| left.path.cmp(&right.path));
    derived_jobs.append(&mut derived_mcp);
    derived_jobs.sort_by(|left, right| left.id.cmp(&right.id));
    let accepted = !diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == RuntimeConfigDiagnosticSeverity::Error);

    RuntimeGraphPlan {
        accepted,
        source_revision,
        runtime_config: RuntimeGraphEffectiveConfig {
            profiles_dir: input.runtime_config.profiles_dir.clone(),
            skills_dir: input.runtime_config.skills_dir.clone(),
            storage,
            brains,
            sessions,
            scheduled_jobs,
            channel_bindings,
            mcp_bindings,
        },
        derived: derived_jobs,
        defaults_applied,
        diagnostics,
    }
}

fn canonical_profiles(profiles: &[RuntimeGraphProfileSource]) -> Vec<ProfileRuntimeMetadata> {
    profiles
        .iter()
        .map(|profile| ProfileRuntimeMetadata {
            profile_id: ProfileId::new(profile.profile_id.clone()),
            brain: profile.brain.clone(),
            runtime: profile.runtime.clone(),
            session_defaults: profile.session_defaults.clone(),
            mcp_config: profile.mcp_config.clone(),
            background_review: profile.background_review.clone(),
            channel_defaults: None,
            context_policy: profile.context_policy.clone(),
        })
        .collect()
}

fn plan_brains(
    input: &RuntimeGraphPlanInput,
    defaults: &mut Vec<RuntimeGraphDefaultRecord>,
    diagnostics: &mut Vec<RuntimeConfigDiagnostic>,
) -> Vec<BrainConfigDraft> {
    let mut profile_ids = HashSet::new();
    let mut brains: Vec<_> = input
        .runtime_config
        .brains
        .iter()
        .map(|brain| {
            if !profile_ids.insert(brain.profile_id.clone()) {
                diagnostics.push(RuntimeConfigDiagnostic::error(
                    "duplicate_brain_profile_id",
                    format!("brains[{}].profileId", brain.profile_id),
                    format!(
                        "profile {} has more than one configured brain",
                        brain.profile_id
                    ),
                ));
            }
            let implementation_id = brain.implementation_id.clone().unwrap_or_else(|| {
                defaults.push(RuntimeGraphDefaultRecord {
                    path: format!("brains[{}].implementationId", brain.profile_id),
                    source: RuntimeGraphDefaultSource::CanonicalProfileDefault,
                });
                format!("{}-brain", brain.profile_id)
            });
            BrainConfigDraft {
                implementation_id: BrainImplementationId::new(implementation_id),
                profile_id: ProfileId::new(brain.profile_id.clone()),
            }
        })
        .collect();
    brains.sort_by(|left, right| left.implementation_id.0.cmp(&right.implementation_id.0));
    brains
}

fn plan_sessions(
    input: &RuntimeGraphPlanInput,
    profiles: &HashMap<ProfileId, &RuntimeGraphProfileSource>,
    defaults: &mut Vec<RuntimeGraphDefaultRecord>,
) -> Vec<RuntimeGraphSessionPlan> {
    let mut sessions: Vec<_> = input
        .runtime_config
        .sessions
        .iter()
        .map(|session| {
            let profile_id = ProfileId::new(session.profile_id.clone());
            let profile = profiles.get(&profile_id).copied();
            let kind = session.kind.clone().unwrap_or_else(|| {
                defaults.push(RuntimeGraphDefaultRecord {
                    path: format!("sessions[{}].kind", session.session_id),
                    source: RuntimeGraphDefaultSource::ServiceDefault,
                });
                SessionKind::Full
            });
            let mut resource_limits = session
                .resource_limits
                .clone()
                .or_else(|| {
                    profile
                        .and_then(|profile| profile.runtime.as_ref())
                        .and_then(|runtime| runtime.default_resource_limits.clone())
                })
                .unwrap_or(ResourceLimits {
                    workdir: None,
                    max_duration_ms: None,
                    max_delegation_depth: None,
                });
            if session.resource_limits.is_none()
                && profile
                    .and_then(|profile| profile.runtime.as_ref())
                    .and_then(|runtime| runtime.default_resource_limits.as_ref())
                    .is_some()
            {
                defaults.push(RuntimeGraphDefaultRecord {
                    path: format!("sessions[{}].resourceLimits", session.session_id),
                    source: RuntimeGraphDefaultSource::ProfileRuntimeDefault,
                });
            }
            if resource_limits.workdir.is_none() {
                if let Some(workdir) = input.host_facts.default_workdir.clone() {
                    resource_limits.workdir = Some(workdir);
                    defaults.push(RuntimeGraphDefaultRecord {
                        path: format!("sessions[{}].resourceLimits.workdir", session.session_id),
                        source: RuntimeGraphDefaultSource::HostDefaultWorkdir,
                    });
                }
            }
            let profile_defaults = profile.and_then(|profile| profile.session_defaults.as_ref());
            let owner_id = session
                .owner_id
                .clone()
                .or_else(|| profile_defaults.and_then(|defaults| defaults.owner_id.clone()));
            if session.owner_id.is_none() && owner_id.is_some() {
                defaults.push(RuntimeGraphDefaultRecord {
                    path: format!("sessions[{}].ownerId", session.session_id),
                    source: RuntimeGraphDefaultSource::ProfileSessionDefault,
                });
            }
            let max_history_messages = session
                .max_history_messages
                .or_else(|| profile_defaults.and_then(|defaults| defaults.max_history_messages));
            RuntimeGraphSessionPlan {
                session_id: SessionId::new(session.session_id.clone()),
                agent_id: AgentId::new(session.agent_id.clone()),
                profile_id: profile_id.clone(),
                kind,
                resource_limits,
                owner_id,
                history_window: session.history_window.clone(),
                max_history_messages,
                local_tool_profile_id: session
                    .local_tool_profile_id
                    .clone()
                    .or_else(|| profile.and_then(|profile| profile.local_tool_profile_id.clone())),
                context_policy_profile_id: profile
                    .filter(|profile| profile.context_policy.is_some())
                    .map(|_| profile_id.clone()),
                session_memory_prompt_profile_id: profile
                    .filter(|profile| profile.session_memory_prompt.is_some())
                    .map(|_| profile_id),
            }
        })
        .collect();
    sessions.sort_by(|left, right| left.session_id.0.cmp(&right.session_id.0));
    sessions
}

fn plan_scheduled_jobs(
    input: &RuntimeGraphPlanInput,
    profiles: &HashMap<ProfileId, &RuntimeGraphProfileSource>,
    sessions: &[RuntimeGraphSessionPlan],
) -> (
    Vec<RuntimeGraphScheduledJobPlan>,
    Vec<RuntimeGraphDerivedRecord>,
) {
    let mut jobs: Vec<_> = input
        .runtime_config
        .scheduled_jobs
        .iter()
        .map(|job| RuntimeGraphScheduledJobPlan {
            id: job.id.clone(),
            schedule: job.schedule.clone(),
            shape: job.shape.clone(),
            job_kind: job.job_kind.clone(),
            target_session_id: job.target_session_id.clone().map(SessionId::new),
            payload: job.payload.clone(),
            script: job.script.clone(),
            delivery_channel_id: job.delivery_channel_id.clone(),
        })
        .collect();
    let mut ids: HashSet<String> = jobs.iter().map(|job| job.id.clone()).collect();
    let mut reviewed_profiles = HashSet::new();
    let mut derived = Vec::new();
    for session in sessions {
        if !reviewed_profiles.insert(session.profile_id.clone()) {
            continue;
        }
        let Some(profile) = profiles.get(&session.profile_id).copied() else {
            continue;
        };
        let Some(review) = profile.background_review.as_ref() else {
            continue;
        };
        if !review.enabled {
            continue;
        }
        let id = format!("background-review-{}", profile.profile_id);
        if !ids.insert(id.clone()) {
            continue;
        }
        let review_type = review
            .review_type
            .as_ref()
            .map(|review_type| match review_type {
                super::ProfileBackgroundReviewType::Memory => "memory",
                super::ProfileBackgroundReviewType::Skills => "skills",
                super::ProfileBackgroundReviewType::Combined => "combined",
            })
            .unwrap_or("combined");
        jobs.push(RuntimeGraphScheduledJobPlan {
            id: id.clone(),
            schedule: review
                .schedule
                .clone()
                .unwrap_or_else(|| "0 3 * * *".to_string()),
            shape: ScheduledJobShape::HostJob,
            job_kind: Some(RUNTIME_REVIEW_MEMORY_SKILLS_JOB_KIND.to_string()),
            target_session_id: None,
            payload: background_review_payload(profile, review, review_type),
            script: None,
            delivery_channel_id: None,
        });
        derived.push(RuntimeGraphDerivedRecord {
            kind: RuntimeGraphDerivedKind::ScheduledJob,
            id,
            source: format!("profiles[{}].backgroundReview", profile.profile_id),
        });
    }
    jobs.sort_by(|left, right| left.id.cmp(&right.id));
    (jobs, derived)
}

fn background_review_payload(
    profile: &RuntimeGraphProfileSource,
    review: &ProfileBackgroundReviewConfig,
    review_type: &str,
) -> Value {
    let mut payload = serde_json::json!({
        "schemaVersion": 1,
        "reviewType": review_type,
        "profileId": profile.profile_id,
        "triggerSource": "profile_background_review",
        "includeDenseProfileMemory": true,
        "includeDenMemoryDiagnostics": true,
        "llmReviewEnabled": review.llm_review_enabled.unwrap_or(false),
        "dryRun": review.dry_run.unwrap_or(true),
        "reason": format!("profile {} backgroundReview", profile.profile_id),
    });
    let object = payload.as_object_mut().expect("payload is an object");
    insert_optional_u32(object, "memoryNudgeInterval", review.memory_nudge_interval);
    insert_optional_u32(object, "skillNudgeInterval", review.skill_nudge_interval);
    insert_optional_u32(object, "maxTokens", review.max_tokens);
    insert_optional_u32(object, "maxFindings", review.max_findings);
    insert_optional_u32(object, "maxCandidates", review.max_candidates);
    insert_optional_u32(object, "captureMaxProposals", review.capture_max_proposals);
    if let Some(alias) = &review.capture_provider_alias {
        object.insert(
            "captureProviderAlias".to_string(),
            Value::String(alias.clone()),
        );
    }
    payload
}

fn insert_optional_u32(object: &mut serde_json::Map<String, Value>, key: &str, value: Option<u32>) {
    if let Some(value) = value {
        object.insert(key.to_string(), Value::from(value));
    }
}

fn plan_mcp_bindings(
    input: &RuntimeGraphPlanInput,
    profiles: &HashMap<ProfileId, &RuntimeGraphProfileSource>,
    sessions: &[RuntimeGraphSessionPlan],
) -> (Vec<McpBindingConfigDraft>, Vec<RuntimeGraphDerivedRecord>) {
    let mut bindings = input.runtime_config.mcp_bindings.clone();
    let mut ids: HashSet<String> = bindings
        .iter()
        .map(|binding| binding.binding_id.clone())
        .collect();
    let mut derived = Vec::new();
    for session in sessions {
        let Some(profile) = profiles.get(&session.profile_id).copied() else {
            continue;
        };
        let Some(mcp) = profile.mcp_config.as_ref() else {
            continue;
        };
        let Some(tool_profile) = mcp.tool_profile.as_ref() else {
            continue;
        };
        let id = mcp
            .binding_id
            .clone()
            .unwrap_or_else(|| format!("{}-mcp", session.agent_id));
        if !ids.insert(id.clone()) {
            continue;
        }
        bindings.push(McpBindingConfigDraft {
            binding_id: id.clone(),
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
        });
        derived.push(RuntimeGraphDerivedRecord {
            kind: RuntimeGraphDerivedKind::McpBinding,
            id,
            source: format!("profiles[{}].mcpConfig", profile.profile_id),
        });
    }
    bindings.sort_by(|left, right| left.binding_id.cmp(&right.binding_id));
    (bindings, derived)
}

fn plan_storage(
    input: &RuntimeGraphPlanInput,
    diagnostics: &mut Vec<RuntimeConfigDiagnostic>,
) -> RuntimeGraphStoragePlan {
    let source = input.runtime_config.storage.as_ref();
    let service_default = input.service_defaults.storage.as_ref();
    let backend_source = source
        .map(|source| source.backend.as_str())
        .or_else(|| service_default.map(|source| source.backend.as_str()))
        .unwrap_or("sqlite");
    let backend = match backend_source {
        "sqlite" => RuntimeGraphStorageBackend::Sqlite,
        "postgres" => RuntimeGraphStorageBackend::Postgres,
        _ => {
            diagnostics.push(RuntimeConfigDiagnostic::error(
                "storage_backend_invalid",
                "runtimeConfig.storage.backend",
                "storage backend must be sqlite or postgres",
            ));
            RuntimeGraphStorageBackend::Sqlite
        }
    };
    let sqlite_source = source.and_then(|source| source.sqlite.as_ref());
    let sqlite_default = service_default.and_then(|source| source.sqlite.as_ref());
    let sqlite_path = sqlite_source
        .and_then(|sqlite| sqlite.path.clone())
        .or_else(|| sqlite_default.and_then(|sqlite| sqlite.path.clone()))
        .unwrap_or_else(|| "runtime.sqlite3".to_string());
    if sqlite_path.trim().is_empty() {
        diagnostics.push(RuntimeConfigDiagnostic::error(
            "sqlite_path_required",
            "runtimeConfig.storage.sqlite.path",
            "SQLite path must not be empty",
        ));
    }
    let effective_path = if sqlite_path.starts_with('/') {
        sqlite_path.clone()
    } else {
        format!(
            "{}/{}",
            input.host_facts.engine_data_dir.trim_end_matches('/'),
            sqlite_path
        )
    };
    let postgres_source = source.and_then(|source| source.postgres.as_ref());
    let postgres_default = service_default.and_then(|source| source.postgres.as_ref());
    let database_url_env = postgres_source
        .and_then(|postgres| postgres.database_url_env.clone())
        .or_else(|| postgres_default.and_then(|postgres| postgres.database_url_env.clone()))
        .unwrap_or_else(|| "RUSTY_CREW_POSTGRES_URL".to_string());
    let schema = postgres_source
        .and_then(|postgres| postgres.schema.clone())
        .or_else(|| postgres_default.and_then(|postgres| postgres.schema.clone()))
        .unwrap_or_else(|| "rusty_crew".to_string());
    if !valid_identifier(&database_url_env) {
        diagnostics.push(RuntimeConfigDiagnostic::error(
            "postgres_database_url_env_invalid",
            "runtimeConfig.storage.postgres.databaseUrlEnv",
            "Postgres database URL environment variable name is invalid",
        ));
    }
    if !valid_identifier(&schema) {
        diagnostics.push(RuntimeConfigDiagnostic::error(
            "postgres_schema_invalid",
            "runtimeConfig.storage.postgres.schema",
            "Postgres schema is not a valid identifier",
        ));
    }
    let boot_mode = postgres_source
        .and_then(|postgres| postgres.boot_mode)
        .or_else(|| postgres_default.and_then(|postgres| postgres.boot_mode))
        .unwrap_or(RuntimeGraphPostgresBootMode::Active);
    if backend == RuntimeGraphStorageBackend::Postgres
        && boot_mode == RuntimeGraphPostgresBootMode::Active
        && !input.host_facts.postgres_database_url_env_present
    {
        diagnostics.push(RuntimeConfigDiagnostic::error(
            "postgres_database_url_env_missing",
            "hostFacts.postgresDatabaseUrlEnvPresent",
            "active Postgres storage requires the selected database URL environment variable",
        ));
    }
    let implementation_status = match (backend, boot_mode) {
        (RuntimeGraphStorageBackend::Sqlite, _) | (_, RuntimeGraphPostgresBootMode::Active) => {
            RuntimeGraphStorageImplementationStatus::Active
        }
        (_, RuntimeGraphPostgresBootMode::ProofAdmin) => {
            RuntimeGraphStorageImplementationStatus::ProofAdminOnly
        }
        (_, RuntimeGraphPostgresBootMode::Blocked) => {
            RuntimeGraphStorageImplementationStatus::BlockedUnimplemented
        }
    };
    RuntimeGraphStoragePlan {
        backend,
        implementation_status,
        sqlite: RuntimeGraphSqliteStoragePlan {
            path: sqlite_path,
            effective_path,
            wal: sqlite_source
                .and_then(|sqlite| sqlite.wal)
                .or_else(|| sqlite_default.and_then(|sqlite| sqlite.wal))
                .unwrap_or(true),
            busy_timeout_ms: sqlite_source
                .and_then(|sqlite| sqlite.busy_timeout_ms)
                .or_else(|| sqlite_default.and_then(|sqlite| sqlite.busy_timeout_ms))
                .unwrap_or(5_000),
        },
        postgres: RuntimeGraphPostgresStoragePlan {
            database_url_env,
            schema,
            boot_mode,
            max_connections: postgres_source
                .and_then(|postgres| postgres.max_connections)
                .or_else(|| postgres_default.and_then(|postgres| postgres.max_connections))
                .unwrap_or(16),
            statement_timeout_ms: postgres_source
                .and_then(|postgres| postgres.statement_timeout_ms)
                .or_else(|| postgres_default.and_then(|postgres| postgres.statement_timeout_ms))
                .unwrap_or(30_000),
        },
    }
}

fn runtime_graph_source_revision(input: &RuntimeGraphPlanInput) -> String {
    let bytes = serde_json::to_vec(input).expect("runtime graph input is serializable");
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn valid_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
        && chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Map, Value};

    const FIXTURE_ROOT: &str = "/tmp/rusty-crew-runtime-graph-fixture";

    #[test]
    fn complete_source_matches_target_plan() {
        let input = source_fixture("complete-source.camel.json");
        let plan = plan_runtime_graph(&input);
        assert!(plan.accepted, "diagnostics: {:?}", plan.diagnostics);
        let mut expected = fixture_value("complete-plan.camel.json");
        expected["sourceRevision"] = Value::String(plan.source_revision.clone());
        assert_eq!(
            strip_nulls(camelize(serde_json::to_value(plan).unwrap())),
            expected
        );
    }

    #[test]
    fn invalid_source_reports_stable_required_codes() {
        let value = fixture_value("invalid-source.camel.json");
        let expected: HashSet<String> = value["expectedDiagnosticCodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|code| code.as_str().unwrap().to_string())
            .collect();
        let input: RuntimeGraphPlanInput = serde_json::from_value(snake_case(value_without_key(
            value,
            "expectedDiagnosticCodes",
        )))
        .unwrap();
        let plan = plan_runtime_graph(&input);
        assert!(!plan.accepted);
        let actual: HashSet<String> = plan
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.clone())
            .collect();
        assert!(
            expected.is_subset(&actual),
            "missing diagnostics: {:?}; actual: {:?}",
            expected.difference(&actual).collect::<Vec<_>>(),
            actual
        );
    }

    #[test]
    fn planning_is_deterministic_and_does_not_mutate_source() {
        let input = source_fixture("complete-source.camel.json");
        let source = input.clone();
        let first = plan_runtime_graph(&input);
        let second = plan_runtime_graph(&input);
        assert_eq!(input, source);
        assert_eq!(first, second);
        assert!(first
            .runtime_config
            .scheduled_jobs
            .windows(2)
            .all(|pair| pair[0].id <= pair[1].id));
    }

    #[test]
    fn explicit_session_values_override_profile_defaults() {
        let mut input = source_fixture("complete-source.camel.json");
        let session = &mut input.runtime_config.sessions[0];
        session.local_tool_profile_id = Some("session-tools".to_string());
        session.resource_limits = Some(ResourceLimits {
            workdir: Some("/explicit".to_string()),
            max_duration_ms: Some(10_000),
            max_delegation_depth: Some(1),
        });
        let plan = plan_runtime_graph(&input);
        let session = &plan.runtime_config.sessions[0];
        assert_eq!(
            session.local_tool_profile_id.as_deref(),
            Some("session-tools")
        );
        assert_eq!(
            session.resource_limits.workdir.as_deref(),
            Some("/explicit")
        );
    }

    #[test]
    fn service_storage_defaults_are_applied_when_graph_omits_storage() {
        let mut input = source_fixture("complete-source.camel.json");
        let storage = input.runtime_config.storage.take().unwrap();
        input.service_defaults.storage = Some(storage);
        let plan = plan_runtime_graph(&input);
        assert!(plan.accepted, "diagnostics: {:?}", plan.diagnostics);
        assert_eq!(
            plan.runtime_config.storage.backend,
            RuntimeGraphStorageBackend::Sqlite
        );
        assert_eq!(plan.runtime_config.storage.sqlite.path, "runtime.sqlite3");
        assert_eq!(plan.runtime_config.storage.sqlite.busy_timeout_ms, 5_000);
    }

    fn source_fixture(name: &str) -> RuntimeGraphPlanInput {
        serde_json::from_value(snake_case(fixture_value(name))).unwrap()
    }

    fn fixture_value(name: &str) -> Value {
        let source = match name {
            "complete-source.camel.json" => include_str!(concat!(
                "../../../../fixtures/runtime-config-parity/target/complete-source.camel.json"
            )),
            "complete-plan.camel.json" => include_str!(concat!(
                "../../../../fixtures/runtime-config-parity/target/complete-plan.camel.json"
            )),
            "invalid-source.camel.json" => include_str!(concat!(
                "../../../../fixtures/runtime-config-parity/target/invalid-source.camel.json"
            )),
            _ => panic!("unknown fixture {name}"),
        };
        serde_json::from_str::<Value>(&source.replace("__FIXTURE_ROOT__", FIXTURE_ROOT)).unwrap()
    }

    fn value_without_key(mut value: Value, key: &str) -> Value {
        value.as_object_mut().unwrap().remove(key);
        value
    }

    fn snake_case(value: Value) -> Value {
        transform_keys(value, |key| {
            key.chars().fold(String::new(), |mut output, character| {
                if character.is_ascii_uppercase() {
                    output.push('_');
                    output.push(character.to_ascii_lowercase());
                } else {
                    output.push(character);
                }
                output
            })
        })
    }

    fn camelize(value: Value) -> Value {
        transform_keys(value, |key| {
            let mut uppercase = false;
            key.chars().fold(String::new(), |mut output, character| {
                if character == '_' {
                    uppercase = true;
                } else if uppercase {
                    output.push(character.to_ascii_uppercase());
                    uppercase = false;
                } else {
                    output.push(character);
                }
                output
            })
        })
    }

    fn transform_keys(value: Value, transform: impl Fn(&str) -> String + Copy) -> Value {
        match value {
            Value::Array(values) => Value::Array(
                values
                    .into_iter()
                    .map(|value| transform_keys(value, transform))
                    .collect(),
            ),
            Value::Object(values) => Value::Object(
                values
                    .into_iter()
                    .map(|(key, value)| (transform(&key), transform_keys(value, transform)))
                    .collect::<Map<_, _>>(),
            ),
            value => value,
        }
    }

    fn strip_nulls(value: Value) -> Value {
        match value {
            Value::Array(values) => Value::Array(values.into_iter().map(strip_nulls).collect()),
            Value::Object(values) => Value::Object(
                values
                    .into_iter()
                    .filter_map(|(key, value)| {
                        if value.is_null() {
                            None
                        } else {
                            Some((key, strip_nulls(value)))
                        }
                    })
                    .collect(),
            ),
            value => value,
        }
    }
}
