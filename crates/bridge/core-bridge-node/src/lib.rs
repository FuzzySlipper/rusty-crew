//! Native Node transport boundary.
//!
//! napi-rs glue belongs in this crate. The transport-neutral pieces here expose
//! the current manifest surface and own runtime buffers without leaking native
//! transport dependencies into core crates.

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
    plan_create_profile, plan_runtime_config, validate_runtime_config_input, CreateProfilePlan,
    CreateProfilePlanInput, ProfileRegistryMutationPlan, ProfileRegistryMutationRequest,
    RuntimeConfigPlan, RuntimeConfigValidationInput,
};
use rusty_crew_core_engine::CoreEngine;
use rusty_crew_core_persistence::{
    AttachmentQuery, AttachmentRecord, AttachmentWrite, BranchAwareSessionMemoryQuery,
    ConversationBranchQuery, ConversationBranchRecord, ConversationBranchStateRecord,
    ConversationBranchWrite, ConversationJumpRequest, ConversationJumpResult,
    ConversationSnapshotQuery, ConversationSnapshotRecord, ConversationSnapshotWrite,
    DataBankScopeQuery, DataBankScopeRecord, DataBankScopeWrite, LoreRecallQuery, LoreRecallResult,
    LoreRecallTraceQuery, LoreRecallTraceRecord, MessageSlotQuery, MessageSlotRecord,
    MessageSlotWrite, MessageVariantQuery, MessageVariantRecord, MessageVariantWrite,
    ProfileMemoryCaps, ProfileMemoryDelete, ProfileMemoryQuery, ProfileMemoryRecord,
    ProfileMemoryReplace, ProfileMemoryTarget, ProfileMemoryWrite, ProfileRegistryQuery,
    QueuedMessageRecord, RoleplayChatLayerRecord, RoleplayChatLayersWrite,
    RoleplayLoreEntryPromotion, RoleplayLoreFactCapture, RoleplayLoreLayerArchive,
    RoleplayLoreLayerConfigRecord, RoleplayLoreLayerConfigWrite, RoleplayLoreLayerEntryJoin,
    RoleplayLoreLayerEntryLink, RoleplayLoreLayerRecord, RoleplayLoreLayerUpdate,
    RoleplayLoreLayerWrite, RoleplayLoreProvenanceEvent, RoleplayLoreQuery, RoleplayLoreRecord,
    RoleplayLoreReplace, RoleplayLoreSupersede, RoleplayLoreTombstone, RoleplayLoreWrite,
    RuntimeCounterQuery, RuntimeCounterRecord, RuntimeCounterScope, RuntimeDatabaseSize,
    RuntimeInstalledModuleSchemaDiagnostic, RuntimeMaintenancePolicy, RuntimeMaintenanceReport,
    RuntimeModuleCapabilityStatus, RuntimeModuleLogicalStoreDiagnostic,
    RuntimeModuleNamedDiagnostic, RuntimeModulePhysicalIndexDiagnostic,
    RuntimeModulePhysicalTableDiagnostic, RuntimeModuleQueryCatalogDiagnostic,
    RuntimeModuleRetentionDiagnostic, RuntimeModuleSchemaDiagnostic,
    RuntimeModuleSchemaRegistryDiagnostics, RuntimeModuleTransferHookDiagnostic,
    RuntimeRepositoryBackendRequirement, RuntimeRepositoryGroupDiagnostic, RuntimeSearchFilter,
    RuntimeSearchResult, RuntimeSearchRowType, RuntimeStateSummary, RuntimeStorageCapability,
    RuntimeStorageConnectionHealth, RuntimeStorageDiagnostics, RuntimeStoragePressureSignal,
    RuntimeStorageTableCount, SchemaMigrationRecord, SelectActiveBranchRequest,
    SelectActiveBranchResult, SelectActiveVariantRequest, SelectActiveVariantResult,
    SessionMemoryCompactionReport, SessionMemoryPromptContext, SessionMemoryQuery,
    SessionMemoryRecord, SimpleKvDelete, SimpleKvQuery, SimpleKvRecord, SimpleKvScope,
    SimpleKvWrite, UpdateBranchHeadRequest, UpdateBranchHeadResult,
};
use rusty_crew_core_protocol::{
    AttachmentId, BodyState, BrainWakeProviderStateInput, BrainWakeStreamItem,
    ContextCompactionArtifact, ContextCompactionArtifactQuery, DataBankScopeId,
    MemoryGovernanceDecisionInput, MemoryGovernanceDecisionRecord, MemoryProposalEnvelope,
    MemoryProposalQuery, MemoryProposalRecord, MemorySpaceDescriptor, MessageSlotId,
    MessageVariantId, ModelProviderQuery, ModelProviderRefreshImpactRequest, ModelProviderWrite,
    ProfileRegistryLifecycleStatus, ProfileRegistryUpdate, ProfileRegistryWrite,
    SessionActivityDigest, SessionActivityDigestQuery,
};
use rusty_crew_openai_responses_brain::{
    openai_oauth_envelope_from_exchange_result, resolve_openai_oauth_bearer, FakeResponsesClient,
    LiveResponsesClient, NeutralBrainTool, NeutralToolExecutor, NeutralToolOutput,
    OpenAiOauthClient, OpenAiOauthCodeExchangeRequest, OpenAiOauthError, OpenAiOauthRefreshPolicy,
    OpenAiOauthSecretStore, PendingResponsesFunctionCall, ResponsesBrainConfig, ResponsesEvent,
    ResponsesOutputItem, ResponsesReplayBrain, ResponsesTokenUsage, ResponsesTransportMetrics,
};
mod binding_config_profiles;
mod binding_events;
mod binding_manifest;
mod binding_scheduler;
mod binding_sessions;
mod config_profiles;
mod conversation;
mod delegation;
mod engine;
mod events;
mod memory;
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
use registries::{BrainImplementationRegistry, PlatformAdapterRegistry, SubscriptionRegistry};
use responses::{
    drain_openai_responses_brain_stream_json, start_openai_responses_brain_json,
    submit_openai_responses_tool_output_json, OpenAiOauthCodeExchangeTask,
    OpenAiResponsesBrainRunTask,
};
#[cfg(test)]
use responses::{normalize_responses_tool_schema, run_openai_responses_brain_json_blocking};
use serde::Deserialize;
use serde_json::{json, Value};
pub(crate) use sessions::*;
use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};
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
}

impl NativeBridge {
    pub fn new() -> Self {
        Self {
            engine: None,
            buffers: RuntimeBufferStore::new(),
            brain_registrations: BrainImplementationRegistry::new(),
            adapter_registrations: PlatformAdapterRegistry::new(),
            subscriptions: SubscriptionRegistry::new(),
        }
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
            let schema = config
                .postgres_schema
                .clone()
                .unwrap_or_else(|| "rusty_crew".to_string());
            Ok(Some(EngineStorageConfig::Postgres {
                database_url,
                schema,
                max_connections: config.postgres_max_connections,
                statement_timeout_ms: config.postgres_statement_timeout_ms,
            }))
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
    pub fn run_openai_responses_brain_json(
        &self,
        input_json: String,
    ) -> napi::bindgen_prelude::AsyncTask<OpenAiResponsesBrainRunTask> {
        // The responses brain still uses blocking provider I/O internally.
        // Running it as a napi task keeps the Node event loop available for
        // admin APIs, adapters, and SSE while this worker-thread task drains.
        napi::bindgen_prelude::AsyncTask::new(OpenAiResponsesBrainRunTask::new(input_json))
    }

    #[napi]
    pub fn exchange_openai_oauth_code_json(
        &self,
        input_json: String,
    ) -> napi::bindgen_prelude::AsyncTask<OpenAiOauthCodeExchangeTask> {
        // OAuth code exchange performs blocking provider I/O. Keep it off the
        // Node event loop just like the live Responses wake path.
        napi::bindgen_prelude::AsyncTask::new(OpenAiOauthCodeExchangeTask::new(input_json))
    }

    #[napi]
    pub fn start_openai_responses_brain_json(&self, input_json: String) -> napi::Result<String> {
        start_openai_responses_brain_json(input_json)
    }

    #[napi]
    pub fn drain_openai_responses_brain_stream_json(
        &self,
        wake_id: String,
        max_items: Option<u32>,
    ) -> napi::Result<String> {
        drain_openai_responses_brain_stream_json(wake_id, max_items)
    }

    #[napi]
    pub fn submit_openai_responses_tool_output_json(
        &self,
        input_json: String,
    ) -> napi::Result<String> {
        submit_openai_responses_tool_output_json(input_json)
    }

    #[napi]
    pub fn cancel_delegated_session(
        &self,
        delegated_session_id: String,
    ) -> napi::Result<JsSessionState> {
        let bridge = self.bridge()?;
        let state = bridge
            .cancel_delegated_session(SessionId::new(delegated_session_id))
            .map_err(to_napi_error)?;
        Ok(to_js_session_state(state))
    }

    #[napi]
    pub fn request_delegated_checkpoint(
        &self,
        parent_session_id: String,
        delegated_session_id: String,
        reason: String,
    ) -> napi::Result<JsEventReceipt> {
        let bridge = self.bridge()?;
        let receipt = bridge
            .request_delegated_checkpoint(
                SessionId::new(parent_session_id),
                SessionId::new(delegated_session_id),
                reason,
            )
            .map_err(to_napi_error)?;
        Ok(to_js_event_receipt(receipt))
    }

    #[napi]
    pub fn drain_delegated_sessions(
        &self,
        parent_session_id: Option<String>,
    ) -> napi::Result<Vec<String>> {
        let bridge = self.bridge()?;
        let drained = bridge
            .drain_delegated_sessions(parent_session_id.map(SessionId::new))
            .map_err(to_napi_error)?;
        Ok(drained.into_iter().map(|session_id| session_id.0).collect())
    }

    #[napi]
    pub fn cleanup_delegated_resources_json(&self) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let report = bridge
            .cleanup_delegated_resources()
            .map_err(to_napi_error)?;
        serde_json::to_string(&report)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn delegated_session_status_json(
        &self,
        delegated_session_id: String,
    ) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let status = bridge
            .delegated_session_status(SessionId::new(delegated_session_id))
            .map_err(to_napi_error)?;
        serde_json::to_string(&status)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn count_rows(&self, table: String) -> napi::Result<f64> {
        let bridge = self.bridge()?;
        let count = bridge.count_rows(&table).map_err(to_napi_error)?;
        Ok(count as f64)
    }

    #[napi]
    pub fn database_size(&self) -> napi::Result<JsRuntimeDatabaseSize> {
        let bridge = self.bridge()?;
        let size = bridge.database_size().map_err(to_napi_error)?;
        Ok(to_js_runtime_database_size(size))
    }

    #[napi]
    pub fn storage_diagnostics(&self) -> napi::Result<JsRuntimeStorageDiagnostics> {
        let bridge = self.bridge()?;
        let diagnostics = bridge.storage_diagnostics().map_err(to_napi_error)?;
        Ok(to_js_runtime_storage_diagnostics(diagnostics))
    }

    #[napi]
    pub fn storage_schema(&self) -> napi::Result<JsRuntimeModuleSchemaRegistryDiagnostics> {
        let bridge = self.bridge()?;
        let diagnostics = bridge.storage_schema().map_err(to_napi_error)?;
        Ok(to_js_runtime_module_schema_registry_diagnostics(
            diagnostics,
        ))
    }

    #[napi]
    pub fn add_lore_entry_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let write = parse_json::<RoleplayLoreWrite>(&input_json, "roleplay lore write")?;
        let record = bridge
            .add_roleplay_lore_record(&write)
            .map_err(to_napi_error)?;
        serialize_json(&record, "roleplay lore record")
    }

    #[napi]
    pub fn replace_lore_entry_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let replace = parse_json::<RoleplayLoreReplace>(&input_json, "roleplay lore replace")?;
        let record = bridge
            .replace_roleplay_lore_record(&replace)
            .map_err(to_napi_error)?;
        serialize_json(&record, "roleplay lore record")
    }

    #[napi]
    pub fn supersede_lore_entry_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let supersede =
            parse_json::<RoleplayLoreSupersede>(&input_json, "roleplay lore supersede")?;
        let records = bridge
            .supersede_roleplay_lore_record(&supersede)
            .map_err(to_napi_error)?;
        serialize_json(&records, "roleplay lore supersede records")
    }

    #[napi]
    pub fn tombstone_lore_entry_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let tombstone =
            parse_json::<RoleplayLoreTombstone>(&input_json, "roleplay lore tombstone")?;
        let record = bridge
            .tombstone_roleplay_lore_record(&tombstone)
            .map_err(to_napi_error)?;
        serialize_json(&record, "roleplay lore record")
    }

    #[napi]
    pub fn query_lore_entries_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let query = parse_json::<RoleplayLoreQuery>(&input_json, "roleplay lore query")?;
        let records = bridge
            .query_roleplay_lore_records(&query)
            .map_err(to_napi_error)?;
        serialize_json(&records, "roleplay lore records")
    }

    #[napi]
    pub fn get_lore_entry_json(&self, record_id: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let record = bridge
            .get_roleplay_lore_record(&record_id)
            .map_err(to_napi_error)?;
        serialize_json(&record, "roleplay lore record")
    }

    #[napi]
    pub fn lore_entry_provenance_events_json(&self, record_id: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let events = bridge
            .roleplay_lore_provenance_events(&record_id)
            .map_err(to_napi_error)?;
        serialize_json(&events, "roleplay lore provenance events")
    }

    #[napi]
    pub fn create_lore_layer_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let write = parse_json::<RoleplayLoreLayerWrite>(&input_json, "roleplay lore layer write")?;
        let layer = bridge.create_lore_layer(&write).map_err(to_napi_error)?;
        serialize_json(&layer, "roleplay lore layer")
    }

    #[napi]
    pub fn get_lore_layer_json(&self, layer_id: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let layer = bridge.get_lore_layer(&layer_id).map_err(to_napi_error)?;
        serialize_json(&layer, "roleplay lore layer")
    }

    #[napi]
    pub fn list_lore_layers_json(&self, profile_id: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let layers = bridge
            .list_lore_layers_by_profile(&profile_id)
            .map_err(to_napi_error)?;
        serialize_json(&layers, "roleplay lore layers")
    }

    #[napi]
    pub fn update_lore_layer_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let update =
            parse_json::<RoleplayLoreLayerUpdate>(&input_json, "roleplay lore layer update")?;
        let layer = bridge.update_lore_layer(&update).map_err(to_napi_error)?;
        serialize_json(&layer, "roleplay lore layer")
    }

    #[napi]
    pub fn archive_lore_layer_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let archive =
            parse_json::<RoleplayLoreLayerArchive>(&input_json, "roleplay lore layer archive")?;
        let layer = bridge.archive_lore_layer(&archive).map_err(to_napi_error)?;
        serialize_json(&layer, "roleplay lore layer")
    }

    #[napi]
    pub fn get_lore_layer_config_json(&self, layer_id: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let config = bridge
            .get_lore_layer_config(&layer_id)
            .map_err(to_napi_error)?;
        serialize_json(&config, "roleplay lore layer config")
    }

    #[napi]
    pub fn set_lore_layer_config_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let write = parse_json::<RoleplayLoreLayerConfigWrite>(
            &input_json,
            "roleplay lore layer config write",
        )?;
        let config = bridge
            .set_lore_layer_config(&write)
            .map_err(to_napi_error)?;
        serialize_json(&config, "roleplay lore layer config")
    }

    #[napi]
    pub fn add_entry_to_layer_json(&self, input_json: String) -> napi::Result<()> {
        let bridge = self.bridge()?;
        let link = parse_json::<RoleplayLoreLayerEntryLink>(
            &input_json,
            "roleplay lore layer entry link",
        )?;
        bridge.add_entry_to_layer(&link).map_err(to_napi_error)
    }

    #[napi]
    pub fn remove_entry_from_layer_json(&self, input_json: String) -> napi::Result<()> {
        let bridge = self.bridge()?;
        let request = parse_json::<WireRemoveLoreEntryFromLayerRequest>(
            &input_json,
            "remove roleplay lore layer entry request",
        )?;
        bridge
            .remove_entry_from_layer(&request.layer_id, &request.record_id)
            .map_err(to_napi_error)
    }

    #[napi]
    pub fn set_entry_constant_json(&self, input_json: String) -> napi::Result<()> {
        let bridge = self.bridge()?;
        let request = parse_json::<WireSetLoreEntryConstantRequest>(
            &input_json,
            "set roleplay lore layer entry constant request",
        )?;
        bridge
            .set_entry_constant(&request.layer_id, &request.record_id, request.is_constant)
            .map_err(to_napi_error)
    }

    #[napi]
    pub fn list_entries_by_layer_json(&self, layer_id: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let entries = bridge
            .list_entries_by_layer(&layer_id)
            .map_err(to_napi_error)?;
        serialize_json(&entries, "roleplay lore layer entries")
    }

    #[napi]
    pub fn capture_lore_fact_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let capture =
            parse_json::<RoleplayLoreFactCapture>(&input_json, "roleplay lore fact capture")?;
        let entry = bridge.capture_lore_fact(&capture).map_err(to_napi_error)?;
        serialize_json(&entry, "roleplay lore layer entry")
    }

    #[napi]
    pub fn promote_lore_entry_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let promotion =
            parse_json::<RoleplayLoreEntryPromotion>(&input_json, "roleplay lore entry promotion")?;
        let entry = bridge
            .promote_lore_entry(&promotion)
            .map_err(to_napi_error)?;
        serialize_json(&entry, "roleplay lore layer entry")
    }

    #[napi]
    pub fn set_chat_layers_json(&self, input_json: String) -> napi::Result<()> {
        let bridge = self.bridge()?;
        let write = parse_json::<RoleplayChatLayersWrite>(&input_json, "roleplay chat layers")?;
        bridge.set_chat_layers(&write).map_err(to_napi_error)
    }

    #[napi]
    pub fn get_chat_layers_json(&self, chat_id: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let layers = bridge.get_chat_layers(&chat_id).map_err(to_napi_error)?;
        serialize_json(&layers, "roleplay chat layers")
    }

    #[napi]
    pub fn toggle_chat_layer_json(&self, input_json: String) -> napi::Result<()> {
        let bridge = self.bridge()?;
        let request =
            parse_json::<WireToggleChatLayerRequest>(&input_json, "toggle roleplay chat layer")?;
        bridge
            .toggle_chat_layer(&request.chat_id, &request.layer_id, request.enabled)
            .map_err(to_napi_error)
    }

    #[napi]
    pub fn reorder_chat_layers_json(&self, input_json: String) -> napi::Result<()> {
        let bridge = self.bridge()?;
        let request = parse_json::<WireReorderChatLayersRequest>(
            &input_json,
            "reorder roleplay chat layers",
        )?;
        bridge
            .reorder_chat_layers(&request.chat_id, &request.layer_ids)
            .map_err(to_napi_error)
    }

    #[napi]
    pub fn recall_lore_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let query = parse_json::<LoreRecallQuery>(&input_json, "roleplay lore recall query")?;
        let recall = bridge.recall_lore(&query).map_err(to_napi_error)?;
        serialize_json(&recall, "roleplay lore recall")
    }

    #[napi]
    pub fn list_recall_traces_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let query =
            parse_json::<LoreRecallTraceQuery>(&input_json, "roleplay lore recall trace query")?;
        let traces = bridge.list_recall_traces(&query).map_err(to_napi_error)?;
        serialize_json(&traces, "roleplay lore recall traces")
    }

    #[napi]
    pub fn get_recall_trace_json(&self, trace_id: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let trace = bridge.get_recall_trace(&trace_id).map_err(to_napi_error)?;
        serialize_json(&trace, "roleplay lore recall trace")
    }

    #[napi]
    pub fn run_maintenance(
        &self,
        policy: JsRuntimeMaintenancePolicy,
    ) -> napi::Result<JsRuntimeMaintenanceReport> {
        let bridge = self.bridge()?;
        let report = bridge
            .run_maintenance(&RuntimeMaintenancePolicy {
                expire_queued_messages_at: policy.expire_queued_messages_at,
                purge_terminal_queued_messages_before: policy.purge_terminal_queued_messages_before,
                expire_provider_wire_states_at: policy.expire_provider_wire_states_at,
                compact_session_memory_at: policy.compact_session_memory_at,
                session_memory_max_active_records_per_scope: policy
                    .session_memory_max_active_records_per_scope,
                session_memory_archive_batch_size: policy.session_memory_archive_batch_size,
                run_wal_checkpoint: policy.run_wal_checkpoint.unwrap_or(false),
                run_optimize: policy.run_optimize.unwrap_or(false),
            })
            .map_err(to_napi_error)?;
        Ok(to_js_runtime_maintenance_report(report))
    }

    #[napi]
    pub fn save_message_slot_json(&self, input_json: String) -> napi::Result<()> {
        let bridge = self.bridge()?;
        let slot = parse_json::<MessageSlotWrite>(&input_json, "message slot write")?;
        bridge.save_message_slot(&slot).map_err(to_napi_error)
    }

    #[napi]
    pub fn save_message_variant_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let variant = parse_json::<MessageVariantWrite>(&input_json, "message variant write")?;
        let record = bridge
            .save_message_variant(&variant)
            .map_err(to_napi_error)?;
        serialize_json(&record, "message variant record")
    }

    #[napi]
    pub fn query_message_slots_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let query = parse_json::<MessageSlotQuery>(&input_json, "message slot query")?;
        let records = bridge.query_message_slots(&query).map_err(to_napi_error)?;
        serialize_json(&records, "message slot records")
    }

    #[napi]
    pub fn query_message_variants_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let query = parse_json::<MessageVariantQuery>(&input_json, "message variant query")?;
        let records = bridge
            .query_message_variants(&query)
            .map_err(to_napi_error)?;
        serialize_json(&records, "message variant records")
    }

    #[napi]
    pub fn save_conversation_branch_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let branch =
            parse_json::<ConversationBranchWrite>(&input_json, "conversation branch write")?;
        let record = bridge
            .save_conversation_branch(&branch)
            .map_err(to_napi_error)?;
        serialize_json(&record, "conversation branch record")
    }

    #[napi]
    pub fn query_conversation_branches_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let query =
            parse_json::<ConversationBranchQuery>(&input_json, "conversation branch query")?;
        let records = bridge
            .query_conversation_branches(&query)
            .map_err(to_napi_error)?;
        serialize_json(&records, "conversation branch records")
    }

    #[napi]
    pub fn get_conversation_branch_state_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let request = parse_json::<WireGetConversationBranchStateRequest>(
            &input_json,
            "get conversation branch state request",
        )?;
        let state = bridge
            .get_conversation_branch_state(&request.session_id, &request.default_updated_at)
            .map_err(to_napi_error)?;
        serialize_json(&state, "conversation branch state")
    }

    #[napi]
    pub fn select_active_conversation_branch_json(
        &self,
        input_json: String,
    ) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let request = parse_json::<SelectActiveBranchRequest>(
            &input_json,
            "select active conversation branch request",
        )?;
        let result = bridge
            .select_active_conversation_branch(&request)
            .map_err(to_napi_error)?;
        serialize_json(&result, "select active conversation branch result")
    }

    #[napi]
    pub fn update_conversation_branch_head_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let request = parse_json::<UpdateBranchHeadRequest>(
            &input_json,
            "update conversation branch head request",
        )?;
        let result = bridge
            .update_conversation_branch_head(&request)
            .map_err(to_napi_error)?;
        serialize_json(&result, "update conversation branch head result")
    }

    #[napi]
    pub fn save_conversation_snapshot_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let snapshot =
            parse_json::<ConversationSnapshotWrite>(&input_json, "conversation snapshot write")?;
        let record = bridge
            .save_conversation_snapshot(&snapshot)
            .map_err(to_napi_error)?;
        serialize_json(&record, "conversation snapshot record")
    }

    #[napi]
    pub fn query_conversation_snapshots_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let query =
            parse_json::<ConversationSnapshotQuery>(&input_json, "conversation snapshot query")?;
        let records = bridge
            .query_conversation_snapshots(&query)
            .map_err(to_napi_error)?;
        serialize_json(&records, "conversation snapshot records")
    }

    #[napi]
    pub fn resolve_conversation_jump_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let request =
            parse_json::<ConversationJumpRequest>(&input_json, "conversation jump request")?;
        let result = bridge
            .resolve_conversation_jump(&request)
            .map_err(to_napi_error)?;
        serialize_json(&result, "conversation jump result")
    }

    #[napi]
    pub fn save_attachment_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let attachment = parse_json::<AttachmentWrite>(&input_json, "attachment write")?;
        let record = bridge.save_attachment(&attachment).map_err(to_napi_error)?;
        serialize_json(&record, "attachment record")
    }

    #[napi]
    pub fn query_attachments_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let query = parse_json::<AttachmentQuery>(&input_json, "attachment query")?;
        let records = bridge.query_attachments(&query).map_err(to_napi_error)?;
        serialize_json(&records, "attachment records")
    }

    #[napi]
    pub fn remove_attachment_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let request =
            parse_json::<WireRemoveAttachmentRequest>(&input_json, "remove attachment request")?;
        let record = bridge
            .remove_attachment(&request.attachment_id, &request.updated_at)
            .map_err(to_napi_error)?;
        serialize_json(&record, "attachment record")
    }

    #[napi]
    pub fn save_data_bank_scope_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let scope = parse_json::<DataBankScopeWrite>(&input_json, "data-bank scope write")?;
        let record = bridge.save_data_bank_scope(&scope).map_err(to_napi_error)?;
        serialize_json(&record, "data-bank scope record")
    }

    #[napi]
    pub fn query_data_bank_scopes_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let query = parse_json::<DataBankScopeQuery>(&input_json, "data-bank scope query")?;
        let records = bridge
            .query_data_bank_scopes(&query)
            .map_err(to_napi_error)?;
        serialize_json(&records, "data-bank scope records")
    }

    #[napi]
    pub fn remove_data_bank_scope_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let request = parse_json::<WireRemoveDataBankScopeRequest>(
            &input_json,
            "remove data-bank scope request",
        )?;
        let record = bridge
            .remove_data_bank_scope(&request.scope_id, &request.updated_at)
            .map_err(to_napi_error)?;
        serialize_json(&record, "data-bank scope record")
    }

    #[napi]
    pub fn select_active_message_variant_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let request = parse_json::<SelectActiveVariantRequest>(
            &input_json,
            "select active message variant request",
        )?;
        let result = bridge
            .select_active_message_variant(&request)
            .map_err(to_napi_error)?;
        serialize_json(&result, "select active message variant result")
    }

    #[napi]
    pub fn delete_message_variant_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let request = parse_json::<WireDeleteMessageVariantRequest>(
            &input_json,
            "delete message variant request",
        )?;
        let slot = bridge
            .delete_message_variant(&request.slot_id, &request.variant_id, &request.updated_at)
            .map_err(to_napi_error)?;
        serialize_json(&slot, "message slot record")
    }

    #[napi]
    pub fn reorder_message_variants_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let request = parse_json::<WireReorderMessageVariantsRequest>(
            &input_json,
            "reorder message variants request",
        )?;
        let variants = bridge
            .reorder_message_variants(
                &request.slot_id,
                &request.ordered_variant_ids,
                &request.updated_at,
            )
            .map_err(to_napi_error)?;
        serialize_json(&variants, "message variant records")
    }

    #[napi]
    pub fn list_profile_memory(
        &self,
        query: JsProfileMemoryQuery,
    ) -> napi::Result<Vec<JsProfileMemoryRecord>> {
        let bridge = self.bridge()?;
        let records = bridge
            .list_profile_memory(&to_profile_memory_query(query)?)
            .map_err(to_napi_error)?;
        records
            .into_iter()
            .map(to_js_profile_memory_record)
            .collect()
    }

    #[napi]
    pub fn list_memory_space_descriptors_json(&self) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let descriptors = bridge
            .list_memory_space_descriptors()
            .map_err(to_napi_error)?;
        serialize_json(&descriptors, "memory space descriptors")
    }

    #[napi]
    pub fn query_session_memory_records_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let query = parse_json::<SessionMemoryQuery>(&input_json, "session memory query")?;
        let records = bridge
            .query_session_memory_records(&query)
            .map_err(to_napi_error)?;
        serialize_json(&records, "session memory records")
    }

    #[napi]
    pub fn build_session_memory_prompt_context_json(
        &self,
        input_json: String,
    ) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let query = parse_json::<BranchAwareSessionMemoryQuery>(
            &input_json,
            "session memory prompt context query",
        )?;
        let context = bridge
            .build_session_memory_prompt_context(&query)
            .map_err(to_napi_error)?;
        serialize_json(&context, "session memory prompt context")
    }

    #[napi]
    pub fn save_memory_proposal_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let proposal = parse_json::<MemoryProposalEnvelope>(&input_json, "memory proposal")?;
        let record = bridge
            .save_memory_proposal(proposal)
            .map_err(to_napi_error)?;
        serialize_json(&record, "memory proposal record")
    }

    #[napi]
    pub fn list_memory_proposals_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let query = parse_json::<MemoryProposalQuery>(&input_json, "memory proposal query")?;
        let records = bridge
            .list_memory_proposals(&query)
            .map_err(to_napi_error)?;
        serialize_json(&records, "memory proposal records")
    }

    #[napi]
    pub fn save_session_activity_digest_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let digest = parse_json::<SessionActivityDigest>(&input_json, "session activity digest")?;
        let record = bridge
            .save_session_activity_digest(&digest)
            .map_err(to_napi_error)?;
        serialize_json(&record, "session activity digest")
    }

    #[napi]
    pub fn list_session_activity_digests_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let query =
            parse_json::<SessionActivityDigestQuery>(&input_json, "session activity digest query")?;
        let records = bridge
            .list_session_activity_digests(&query)
            .map_err(to_napi_error)?;
        serialize_json(&records, "session activity digests")
    }

    #[napi]
    pub fn save_context_compaction_artifact_json(
        &self,
        input_json: String,
    ) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let artifact =
            parse_json::<ContextCompactionArtifact>(&input_json, "context compaction artifact")?;
        let record = bridge
            .save_context_compaction_artifact(&artifact)
            .map_err(to_napi_error)?;
        serialize_json(&record, "context compaction artifact")
    }

    #[napi]
    pub fn list_context_compaction_artifacts_json(
        &self,
        input_json: String,
    ) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let query = parse_json::<ContextCompactionArtifactQuery>(
            &input_json,
            "context compaction artifact query",
        )?;
        let records = bridge
            .list_context_compaction_artifacts(&query)
            .map_err(to_napi_error)?;
        serialize_json(&records, "context compaction artifacts")
    }

    #[napi]
    pub fn record_memory_governance_decision_json(
        &self,
        input_json: String,
    ) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let decision =
            parse_json::<MemoryGovernanceDecisionInput>(&input_json, "memory governance decision")?;
        let record = bridge
            .record_memory_governance_decision(&decision)
            .map_err(to_napi_error)?;
        serialize_json(&record, "memory governance decision record")
    }

    #[napi]
    pub fn get_profile_memory(
        &self,
        profile_id: String,
        target_type: String,
        target_id: Option<String>,
        key: String,
    ) -> napi::Result<Option<JsProfileMemoryRecord>> {
        let bridge = self.bridge()?;
        let profile_id = rusty_crew_core_bridge_api::ProfileId::new(profile_id);
        let target = to_profile_memory_target(&profile_id, &target_type, target_id)?;
        bridge
            .get_profile_memory(&profile_id, &target, &key)
            .map_err(to_napi_error)?
            .map(to_js_profile_memory_record)
            .transpose()
    }

    #[napi]
    pub fn add_profile_memory(
        &self,
        write: JsProfileMemoryWrite,
    ) -> napi::Result<JsProfileMemoryRecord> {
        let caps = to_profile_memory_caps(write.caps.as_ref());
        let bridge = self.bridge()?;
        let record = bridge
            .add_profile_memory(to_profile_memory_write(write)?, &caps)
            .map_err(to_napi_error)?;
        to_js_profile_memory_record(record)
    }

    #[napi]
    pub fn replace_profile_memory(
        &self,
        replace: JsProfileMemoryReplace,
    ) -> napi::Result<JsProfileMemoryRecord> {
        let caps = to_profile_memory_caps(replace.write.caps.as_ref());
        let bridge = self.bridge()?;
        let record = bridge
            .replace_profile_memory(
                ProfileMemoryReplace {
                    write: to_profile_memory_write(replace.write)?,
                    expected_revision: replace.expected_revision as u64,
                },
                &caps,
            )
            .map_err(to_napi_error)?;
        to_js_profile_memory_record(record)
    }

    #[napi]
    pub fn remove_profile_memory(
        &self,
        delete: JsProfileMemoryDelete,
    ) -> napi::Result<JsProfileMemoryRecord> {
        let bridge = self.bridge()?;
        let profile_id = rusty_crew_core_bridge_api::ProfileId::new(delete.profile_id);
        let record = bridge
            .remove_profile_memory(&ProfileMemoryDelete {
                target: to_profile_memory_target(
                    &profile_id,
                    &delete.target_type,
                    delete.target_id,
                )?,
                profile_id,
                key: delete.key,
                expected_revision: delete.expected_revision as u64,
            })
            .map_err(to_napi_error)?;
        to_js_profile_memory_record(record)
    }

    #[napi]
    pub fn search_runtime(
        &self,
        query: JsRuntimeSearchQuery,
    ) -> napi::Result<Vec<JsRuntimeSearchResult>> {
        let bridge = self.bridge()?;
        let results = bridge
            .search_runtime(&to_runtime_search_filter(query)?)
            .map_err(to_napi_error)?;
        Ok(results
            .into_iter()
            .map(to_js_runtime_search_result)
            .collect())
    }

    #[napi]
    pub fn query_runtime_counters(
        &self,
        query: JsRuntimeCounterQuery,
    ) -> napi::Result<Vec<JsRuntimeCounterRecord>> {
        let bridge = self.bridge()?;
        let results = bridge
            .query_runtime_counters(&to_runtime_counter_query(query)?)
            .map_err(to_napi_error)?;
        Ok(results.into_iter().map(to_js_runtime_counter).collect())
    }

    #[napi]
    pub fn list_simple_kv(&self, query: JsSimpleKvQuery) -> napi::Result<Vec<JsSimpleKvRecord>> {
        let bridge = self.bridge()?;
        let records = bridge
            .list_simple_kv(&to_simple_kv_query(query))
            .map_err(to_napi_error)?;
        records.into_iter().map(to_js_simple_kv_record).collect()
    }

    #[napi]
    pub fn put_simple_kv(&self, write: JsSimpleKvWrite) -> napi::Result<JsSimpleKvRecord> {
        let bridge = self.bridge()?;
        let record = bridge
            .put_simple_kv(&to_simple_kv_write(write)?)
            .map_err(to_napi_error)?;
        to_js_simple_kv_record(record)
    }

    #[napi]
    pub fn delete_simple_kv(&self, delete: JsSimpleKvDelete) -> napi::Result<JsSimpleKvRecord> {
        let bridge = self.bridge()?;
        let record = bridge
            .delete_simple_kv(&to_simple_kv_delete(delete)?)
            .map_err(to_napi_error)?;
        to_js_simple_kv_record(record)
    }

    #[napi]
    pub fn runtime_summary(
        &self,
        scope_type: String,
        scope_id: Option<String>,
    ) -> napi::Result<JsRuntimeCounterSummary> {
        let bridge = self.bridge()?;
        let summary = bridge
            .runtime_summary(&to_runtime_counter_scope(&scope_type, scope_id)?)
            .map_err(to_napi_error)?;
        Ok(to_js_runtime_counter_summary(summary))
    }

    #[napi]
    pub fn reset_runtime_counters(&self, query: JsRuntimeCounterQuery) -> napi::Result<f64> {
        let bridge = self.bridge()?;
        let reset = bridge
            .reset_runtime_counters(&to_runtime_counter_query(query)?)
            .map_err(to_napi_error)?;
        Ok(reset as f64)
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

#[cfg(test)]
mod tests;
