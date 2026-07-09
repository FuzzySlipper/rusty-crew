//! Native Node transport boundary.
//!
//! napi-rs glue belongs in this crate. The transport-neutral pieces here expose
//! the current manifest surface and own runtime buffers without leaking native
//! transport dependencies into core crates.

use rusty_crew_brain_runtime::{
    BrainRuntimeError, BufferedNeutralRunCleanupReport, BufferedNeutralRunDiagnostic,
};
use rusty_crew_core_bridge_api::{
    manifest_summary, wire_shape_fingerprint, ActionBatchReceipt, BrainActionBatch,
    BrainEventEnvelope, BrainImplementationHandle, BrainImplementationRegistration,
    BrainWakeAccepted, BrainWakeBufferInput, BrainWakeProviderStateOutput, BrainWakeRequest,
    BridgeManifestSummary, CoreError, CoreErrorKind, CoreEvent, CoreResult, DenDataUpdate,
    EngineConfig, EngineHandle, EngineStorageConfig, EventReceipt, EventSubscription,
    ExternalEvent, PlatformAdapterHandle, PlatformAdapterRegistration, ProfileId,
    RuntimeBufferHandle, RuntimeBufferStore, RuntimeBufferView, SessionId, ShutdownRequest,
    ShutdownSummary, SubscriptionHandle, Unit, MANIFEST_VERSION, OPERATION_NAMES,
};
use rusty_crew_core_config::{
    plan_channel_ingress_route, plan_create_profile, plan_delegated_role_lifecycle,
    plan_den_product_ingress_policy, plan_runtime_config, validate_runtime_config_input,
    ChannelIngressRoutePlan, ChannelIngressRoutePlanInput, CreateProfilePlan,
    CreateProfilePlanInput, DelegatedRoleLifecyclePlan, DelegatedRoleLifecyclePlanInput,
    DenProductIngressPolicyInput, DenProductIngressPolicyPlan, NewSessionControlPlan,
    NewSessionControlPlanInput, ProfileRegistryMutationPlan, ProfileRegistryMutationRequest,
    ReloadMcpControlPlan, ReloadMcpControlPlanInput, RuntimeConfigPlan,
    RuntimeConfigValidationInput,
};
use rusty_crew_core_engine::CoreEngine;
use rusty_crew_core_persistence::{
    AttachmentQuery, AttachmentRecord, AttachmentWrite, BranchAwareSessionMemoryQuery,
    ChatEventLogAppend, ChatEventLogEvent, ChatEventLogPage, ChatEventLogQuery, ChatReadModelPage,
    ChatReadModelQuery, ConversationBranchQuery, ConversationBranchRecord,
    ConversationBranchStateRecord, ConversationBranchWrite, ConversationJumpRequest,
    ConversationJumpResult, ConversationSnapshotQuery, ConversationSnapshotRecord,
    ConversationSnapshotWrite, CreateChatAttachmentRequest, CreateChatAttachmentResult,
    CreateChatConversationBranchRequest, CreateChatConversationSnapshotRequest,
    CreateChatConversationSnapshotResult, CreateChatDataBankScopeRequest,
    CreateChatDataBankScopeResult, CreateChatMessageSlotRequest, CreateChatMessageSlotResult,
    CreateChatMessageVariantRequest, CreateChatMessageVariantResult, DataBankScopeQuery,
    DataBankScopeRecord, DataBankScopeWrite, DeleteChatMessageVariantRequest,
    EnsureActiveChatConversationBranchRequest, EnsureActiveChatConversationBranchResult,
    LoreRecallQuery, LoreRecallResult, LoreRecallTraceQuery, LoreRecallTraceRecord,
    MessageSlotQuery, MessageSlotRecord, MessageSlotWrite, MessageVariantQuery,
    MessageVariantRecord, MessageVariantWrite, ProfileMemoryCaps, ProfileMemoryDelete,
    ProfileMemoryQuery, ProfileMemoryRecord, ProfileMemoryReplace, ProfileMemoryTarget,
    ProfileMemoryWrite, ProfileRegistryQuery, QueuedMessageRecord, RemoveChatAttachmentRequest,
    RemoveChatDataBankScopeRequest, ReorderChatMessageVariantsRequest, RoleplayChatLayerRecord,
    RoleplayChatLayersWrite, RoleplayLoreEntryPromotion, RoleplayLoreFactCapture,
    RoleplayLoreLayerArchive, RoleplayLoreLayerConfigRecord, RoleplayLoreLayerConfigWrite,
    RoleplayLoreLayerEntryJoin, RoleplayLoreLayerEntryLink, RoleplayLoreLayerRecord,
    RoleplayLoreLayerUpdate, RoleplayLoreLayerWrite, RoleplayLoreProvenanceEvent,
    RoleplayLoreQuery, RoleplayLoreRecord, RoleplayLoreReplace, RoleplayLoreSupersede,
    RoleplayLoreTombstone, RoleplayLoreWrite, RuntimeCounterQuery, RuntimeCounterRecord,
    RuntimeCounterScope, RuntimeDatabaseSize, RuntimeInstalledModuleSchemaDiagnostic,
    RuntimeMaintenancePolicy, RuntimeMaintenanceReport, RuntimeModuleCapabilityStatus,
    RuntimeModuleLogicalStoreDiagnostic, RuntimeModuleNamedDiagnostic,
    RuntimeModulePhysicalIndexDiagnostic, RuntimeModulePhysicalTableDiagnostic,
    RuntimeModuleQueryCatalogDiagnostic, RuntimeModuleRetentionDiagnostic,
    RuntimeModuleSchemaDiagnostic, RuntimeModuleSchemaRegistryDiagnostics,
    RuntimeModuleTransferHookDiagnostic, RuntimeRepositoryBackendRequirement,
    RuntimeRepositoryGroupDiagnostic, RuntimeSearchFilter, RuntimeSearchResult,
    RuntimeSearchRowType, RuntimeStateSummary, RuntimeStorageCapability,
    RuntimeStorageConnectionHealth, RuntimeStorageDiagnostics, RuntimeStoragePressureSignal,
    RuntimeStorageTableCount, SchemaMigrationRecord, SelectActiveBranchRequest,
    SelectActiveBranchResult, SelectActiveChatMessageVariantRequest,
    SelectActiveChatMessageVariantResult, SelectActiveVariantRequest, SelectActiveVariantResult,
    SessionMemoryCompactionReport, SessionMemoryPromptContext, SessionMemoryQuery,
    SessionMemoryRecord, SimpleKvDelete, SimpleKvQuery, SimpleKvRecord, SimpleKvScope,
    SimpleKvWrite, UpdateBranchHeadRequest, UpdateBranchHeadResult,
};
use rusty_crew_core_protocol::{
    plan_background_memory_auto_mutations, plan_capture_memory_proposals,
    plan_curator_governance_transition, plan_curator_lifecycle_transition, AttachmentId,
    BackgroundMemoryAutoMutationPlanInput, BodyState, BrainWakeProviderStateInput,
    BrainWakeStreamItem, CaptureMemoryProposalPlanInput, ContextCompactionArtifact,
    ContextCompactionArtifactQuery, CuratorGovernancePlanInput, CuratorLifecyclePlanInput,
    DataBankScopeId, MemoryGovernanceDecisionInput, MemoryGovernanceDecisionRecord,
    MemoryProposalEnvelope, MemoryProposalQuery, MemoryProposalRecord, MemorySpaceDescriptor,
    MessageSlotId, MessageVariantId, ModelProviderQuery, ModelProviderRefreshImpactRequest,
    ModelProviderRefreshPlanRequest, ModelProviderWrite, ProfileRegistryLifecycleStatus,
    ProfileRegistryUpdate, ProfileRegistryWrite, SessionActivityDigest, SessionActivityDigestQuery,
};
use rusty_crew_core_tool_registry::{
    plan_tool_availability, validate_local_tool_profile_policy, validate_tool_metadata_policy,
    LocalToolProfileValidationInput, ToolAvailabilityPlanInput, ToolMetadataPolicyValidationInput,
};
use rusty_crew_openai_responses_brain::{
    openai_oauth_envelope_from_exchange_result, resolve_openai_oauth_bearer, FakeResponsesClient,
    LiveResponsesClient, NeutralBrainTool, NeutralToolExecutor, NeutralToolOutput,
    OpenAiOauthClient, OpenAiOauthCodeExchangeRequest, OpenAiOauthError, OpenAiOauthRefreshPolicy,
    OpenAiOauthSecretStore, PendingResponsesFunctionCall, ResponsesBrainConfig, ResponsesEvent,
    ResponsesOutputItem, ResponsesReplayBrain, ResponsesTokenUsage, ResponsesTransportMetrics,
};
use rusty_crew_roleplay_core::{
    build_prompt_context, merge_character, merge_player_persona, narrator_auto_capture_request,
    narrator_mandatory_explore_requests, narrator_review_requests_revision, next_narrator_phase,
    normalize_lore_search_controls, normalize_narrator_config, patch_session_metadata,
    plan_assistant_alternative, plan_chat_layer_binding, plan_scene_state_update,
    plan_session_lifecycle, read_scene_state, speaker_identity_snapshot, start_narrator_turn,
    write_character, write_player_persona, RoleplayAssistantAlternativePlanInput,
    RoleplayCharacterMergeInput, RoleplayCharacterWriteInput, RoleplayChatLayerBindingPlanInput,
    RoleplayLoreSearchControlsInput, RoleplayNarratorAutoCaptureInput,
    RoleplayNarratorMandatoryExploreInput, RoleplayNarratorNextInput, RoleplayNarratorStartInput,
    RoleplayPlayerPersonaMergeInput, RoleplayPlayerPersonaWriteInput, RoleplayPromptContextInput,
    RoleplaySceneStateReadInput, RoleplaySceneStateUpdateInput, RoleplaySessionLifecyclePlanInput,
    RoleplaySessionMetadataPatchInput, RoleplaySpeakerIdentityInput,
};
mod binding_config_profiles;
mod binding_conversation;
mod binding_delegation;
mod binding_events;
mod binding_manifest;
mod binding_memory;
mod binding_pi_agent;
mod binding_responses;
mod binding_roleplay;
mod binding_scheduler;
mod binding_sessions;
mod binding_storage;
mod config_profiles;
mod conversation;
mod delegation;
mod engine;
mod events;
mod memory;
mod pi_agent;
mod registries;
mod responses;
mod roleplay;
mod scheduler;
mod sessions;
mod storage_admin;
mod wire_helpers;
mod wire_types;

pub(crate) use config_profiles::*;
pub(crate) use engine::*;
pub(crate) use events::*;
pub(crate) use memory::*;
use pi_agent::{
    cancel_pi_agent_brain_json, drain_pi_agent_brain_stream_json, start_pi_agent_brain_json,
    submit_pi_agent_tool_output_json, PiAgentBufferedRunRegistry,
};
use registries::{BrainImplementationRegistry, PlatformAdapterRegistry, SubscriptionRegistry};
use responses::{
    cancel_openai_responses_brain_json, drain_openai_responses_brain_stream_json,
    start_openai_responses_brain_json, submit_openai_responses_tool_output_json,
    OpenAiOauthCodeExchangeTask, OpenAiResponsesBrainRunTask, OpenAiResponsesBufferedRunRegistry,
};
#[cfg(test)]
use responses::{normalize_responses_tool_schema, run_openai_responses_brain_json_blocking};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
pub(crate) use sessions::*;
use std::sync::{Arc, Mutex};
pub(crate) use storage_admin::*;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
pub(crate) use wire_helpers::*;
pub(crate) use wire_types::*;

#[derive(Debug)]
pub struct NativeBridge {
    engine: Option<CoreEngine>,
    buffers: RuntimeBufferStore,
    brain_registrations: BrainImplementationRegistry,
    adapter_registrations: PlatformAdapterRegistry,
    subscriptions: SubscriptionRegistry,
    openai_responses_buffered_runs: Arc<OpenAiResponsesBufferedRunRegistry>,
    pi_agent_buffered_runs: Arc<PiAgentBufferedRunRegistry>,
}

#[derive(Debug, Clone, Serialize)]
struct BufferedBrainRunDiagnostics {
    active_run_count: usize,
    modules: Vec<BufferedBrainRunModuleDiagnostics>,
    runs: Vec<BufferedNeutralRunDiagnostic>,
}

#[derive(Debug, Clone, Serialize)]
struct BufferedBrainRunModuleDiagnostics {
    module_label: String,
    active_run_count: usize,
}

#[derive(Debug, Clone, Serialize)]
struct BufferedBrainRunCleanupSummary {
    active_runs: usize,
    terminal_runs: usize,
    cancelled_nonterminal_runs: usize,
    removed_runs: usize,
    modules: Vec<BufferedNeutralRunCleanupReport>,
}

impl NativeBridge {
    pub fn new() -> Self {
        Self {
            engine: None,
            buffers: RuntimeBufferStore::new(),
            brain_registrations: BrainImplementationRegistry::new(),
            adapter_registrations: PlatformAdapterRegistry::new(),
            subscriptions: SubscriptionRegistry::new(),
            openai_responses_buffered_runs: Arc::new(OpenAiResponsesBufferedRunRegistry::new(
                "OpenAI Responses",
            )),
            pi_agent_buffered_runs: Arc::new(PiAgentBufferedRunRegistry::new("pi-agent")),
        }
    }

    pub(crate) fn openai_responses_buffered_runs(&self) -> Arc<OpenAiResponsesBufferedRunRegistry> {
        Arc::clone(&self.openai_responses_buffered_runs)
    }

    pub(crate) fn pi_agent_buffered_runs(&self) -> Arc<PiAgentBufferedRunRegistry> {
        Arc::clone(&self.pi_agent_buffered_runs)
    }

    fn buffered_brain_run_diagnostics(
        &self,
    ) -> Result<BufferedBrainRunDiagnostics, BrainRuntimeError> {
        let mut runs = Vec::new();
        let mut modules = Vec::new();
        let responses = self.openai_responses_buffered_runs.diagnostics()?;
        modules.push(BufferedBrainRunModuleDiagnostics {
            module_label: "OpenAI Responses".to_string(),
            active_run_count: responses.len(),
        });
        runs.extend(responses);

        let pi_agent = self.pi_agent_buffered_runs.diagnostics()?;
        modules.push(BufferedBrainRunModuleDiagnostics {
            module_label: "pi-agent".to_string(),
            active_run_count: pi_agent.len(),
        });
        runs.extend(pi_agent);
        runs.sort_by(|left, right| {
            left.module_label
                .cmp(&right.module_label)
                .then_with(|| left.wake_id.cmp(&right.wake_id))
        });

        Ok(BufferedBrainRunDiagnostics {
            active_run_count: runs.len(),
            modules,
            runs,
        })
    }

    fn cleanup_buffered_brain_runs(
        &self,
        reason_code: &str,
        summary: &str,
    ) -> Result<BufferedBrainRunCleanupSummary, BrainRuntimeError> {
        let modules = vec![
            self.openai_responses_buffered_runs
                .cleanup(reason_code, summary)?,
            self.pi_agent_buffered_runs.cleanup(reason_code, summary)?,
        ];
        Ok(BufferedBrainRunCleanupSummary {
            active_runs: modules.iter().map(|module| module.active_runs).sum(),
            terminal_runs: modules.iter().map(|module| module.terminal_runs).sum(),
            cancelled_nonterminal_runs: modules
                .iter()
                .map(|module| module.cancelled_nonterminal_runs)
                .sum(),
            removed_runs: modules.iter().map(|module| module.removed_runs).sum(),
            modules,
        })
    }
}

impl Default for NativeBridge {
    fn default() -> Self {
        Self::new()
    }
}

fn not_implemented(operation: &str) -> CoreError {
    CoreError::new(
        CoreErrorKind::AdapterUnavailable,
        format!("native bridge operation {operation} is not implemented yet"),
    )
}

fn js_engine_storage_config(config: &JsEngineConfig) -> napi::Result<Option<EngineStorageConfig>> {
    let Some(backend) = config.storage_backend.as_deref() else {
        return Ok(None);
    };
    match backend {
        "sqlite" => Ok(Some(EngineStorageConfig::Sqlite)),
        "postgres" | "postgresql" => {
            let database_url = config.postgres_database_url.clone().ok_or_else(|| {
                napi::Error::new(
                    napi::Status::InvalidArg,
                    "postgresDatabaseUrl is required when storageBackend=postgres",
                )
            })?;
            if database_url.trim().is_empty() {
                return Err(napi::Error::new(
                    napi::Status::InvalidArg,
                    "postgresDatabaseUrl must not be empty",
                ));
            }
            Ok(Some(EngineStorageConfig::postgres_with_defaults(
                database_url,
                config.postgres_schema.clone(),
                config.postgres_max_connections,
                config.postgres_statement_timeout_ms,
            )))
        }
        other => Err(napi::Error::new(
            napi::Status::InvalidArg,
            format!("unsupported storageBackend {other}"),
        )),
    }
}

#[napi_derive::napi]
pub struct NativeBridgeBinding {
    inner: Mutex<NativeBridge>,
}

#[napi_derive::napi]
impl Default for NativeBridgeBinding {
    fn default() -> Self {
        Self::new()
    }
}

#[napi_derive::napi]
impl NativeBridgeBinding {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(NativeBridge::new()),
        }
    }

    #[napi]
    pub fn initialize_engine(&self, config: JsEngineConfig) -> napi::Result<f64> {
        let mut bridge = self.bridge()?;
        let storage = js_engine_storage_config(&config)?;
        let handle = bridge
            .initialize_engine(EngineConfig {
                engine_data_dir: config.engine_data_dir,
                clock: match config.fixed_clock {
                    Some(at) => rusty_crew_core_bridge_api::ClockConfig::Fixed { at },
                    None => rusty_crew_core_bridge_api::ClockConfig::System,
                },
                default_turn_budget: config.default_turn_budget,
                default_idle_timeout_ms: config.default_idle_timeout_ms,
                storage,
            })
            .map_err(to_napi_error)?;
        Ok(handle.get() as f64)
    }

    #[napi]
    pub fn shutdown_engine(
        &self,
        engine: f64,
        drain_timeout_ms: u32,
    ) -> napi::Result<JsShutdownSummary> {
        let mut bridge = self.bridge()?;
        let summary = bridge
            .shutdown_engine(ShutdownRequest {
                engine: EngineHandle::new(engine as u64),
                drain_timeout_ms,
            })
            .map_err(to_napi_error)?;
        Ok(JsShutdownSummary {
            archived_sessions: summary.archived_sessions,
            dropped_subscriptions: summary.dropped_subscriptions,
        })
    }

    #[napi]
    pub fn buffered_brain_run_diagnostics_json(&self) -> napi::Result<String> {
        let bridge = self.bridge()?;
        serde_json::to_string(
            &bridge
                .buffered_brain_run_diagnostics()
                .map_err(brain_runtime_error_to_napi)?,
        )
        .map_err(|error| {
            napi::Error::new(
                napi::Status::GenericFailure,
                format!("serialize buffered brain run diagnostics: {error}"),
            )
        })
    }

    #[napi]
    pub fn cleanup_buffered_brain_runs_json(
        &self,
        reason_code: String,
        summary: String,
    ) -> napi::Result<String> {
        let bridge = self.bridge()?;
        serde_json::to_string(
            &bridge
                .cleanup_buffered_brain_runs(&reason_code, &summary)
                .map_err(brain_runtime_error_to_napi)?,
        )
        .map_err(|error| {
            napi::Error::new(
                napi::Status::GenericFailure,
                format!("serialize buffered brain run cleanup report: {error}"),
            )
        })
    }

    fn bridge(&self) -> napi::Result<std::sync::MutexGuard<'_, NativeBridge>> {
        self.inner.lock().map_err(|_| {
            napi::Error::new(
                napi::Status::GenericFailure,
                "native bridge lock poisoned".to_string(),
            )
        })
    }
}

fn brain_runtime_error_to_napi(error: BrainRuntimeError) -> napi::Error {
    let status = if error.is_invalid_argument() {
        napi::Status::InvalidArg
    } else {
        napi::Status::GenericFailure
    };
    napi::Error::new(status, error.to_string())
}

fn brain_runtime_error_to_core(error: BrainRuntimeError) -> CoreError {
    CoreError::new(
        CoreErrorKind::InternalError,
        format!("buffered brain run registry failure: {error}"),
    )
}

#[cfg(test)]
mod tests;
