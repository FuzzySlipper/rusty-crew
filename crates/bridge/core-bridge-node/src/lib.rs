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

    #[napi(getter)]
    pub fn manifest_version(&self) -> u32 {
        MANIFEST_VERSION
    }

    #[napi(getter)]
    pub fn operation_names(&self) -> Vec<String> {
        OPERATION_NAMES
            .iter()
            .map(|name| name.to_string())
            .collect()
    }

    #[napi(getter)]
    pub fn wire_shape_fingerprint(&self) -> String {
        wire_shape_fingerprint().to_string()
    }

    #[napi]
    pub fn validate_runtime_config_draft_json(&self, input_json: String) -> napi::Result<String> {
        let input: RuntimeConfigValidationInput =
            serde_json::from_str(&input_json).map_err(|error| {
                napi::Error::new(
                    napi::Status::InvalidArg,
                    format!("invalid runtime config validation input JSON: {error}"),
                )
            })?;
        let bridge = self.bridge()?;
        let result = bridge.validate_runtime_config_draft(input);
        serde_json::to_string(&result)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn plan_create_profile_json(&self, input_json: String) -> napi::Result<String> {
        let input: CreateProfilePlanInput = serde_json::from_str(&input_json).map_err(|error| {
            napi::Error::new(
                napi::Status::InvalidArg,
                format!("invalid create-profile plan input JSON: {error}"),
            )
        })?;
        let bridge = self.bridge()?;
        let plan = bridge.plan_create_profile(input);
        serde_json::to_string(&plan)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn plan_profile_registry_mutation_json(&self, input_json: String) -> napi::Result<String> {
        let input: ProfileRegistryMutationRequest =
            serde_json::from_str(&input_json).map_err(|error| {
                napi::Error::new(
                    napi::Status::InvalidArg,
                    format!("invalid profile registry mutation plan input JSON: {error}"),
                )
            })?;
        let bridge = self.bridge()?;
        let plan = bridge
            .plan_profile_registry_mutation(input)
            .map_err(|error| napi::Error::new(napi::Status::InvalidArg, error))?;
        serde_json::to_string(&plan)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn plan_runtime_config_json(&self, input_json: String) -> napi::Result<String> {
        let input: RuntimeConfigValidationInput =
            serde_json::from_str(&input_json).map_err(|error| {
                napi::Error::new(
                    napi::Status::InvalidArg,
                    format!("invalid runtime config plan input JSON: {error}"),
                )
            })?;
        let bridge = self.bridge()?;
        let plan = bridge.plan_runtime_config(input);
        serde_json::to_string(&plan)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
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
    pub fn register_brain_implementation(
        &self,
        registration: JsBrainImplementationRegistration,
    ) -> napi::Result<f64> {
        let mut bridge = self.bridge()?;
        let handle = bridge
            .register_brain_implementation(to_brain_registration(registration)?)
            .map_err(to_napi_error)?;
        Ok(handle.get() as f64)
    }

    #[napi]
    pub fn replace_brain_implementation(
        &self,
        registration: JsBrainImplementationRegistration,
    ) -> napi::Result<f64> {
        let mut bridge = self.bridge()?;
        let handle = bridge
            .replace_brain_implementation(to_brain_registration(registration)?)
            .map_err(to_napi_error)?;
        Ok(handle.get() as f64)
    }

    #[napi]
    pub fn unregister_brain_implementation_for_profile(
        &self,
        profile_id: String,
    ) -> napi::Result<f64> {
        let mut bridge = self.bridge()?;
        let handle = bridge
            .unregister_brain_implementation_for_profile(ProfileId::new(profile_id))
            .map_err(to_napi_error)?;
        Ok(handle.get() as f64)
    }

    #[napi]
    pub fn register_platform_adapter(
        &self,
        registration: JsPlatformAdapterRegistration,
    ) -> napi::Result<f64> {
        let mut bridge = self.bridge()?;
        let handle = bridge
            .register_platform_adapter(to_platform_adapter_registration(registration)?)
            .map_err(to_napi_error)?;
        Ok(handle.get() as f64)
    }

    #[napi]
    pub fn build_brain_wake_request(
        &self,
        brain: f64,
        session_id: String,
        body_state_json: napi::bindgen_prelude::Buffer,
        system_prompt: String,
        role_assembly_json: napi::bindgen_prelude::Buffer,
        wake_id: String,
    ) -> napi::Result<JsBufferedBrainWakeRequest> {
        let bridge = self.bridge()?;
        let buffered = bridge
            .build_brain_wake_request(BrainWakeBufferInput {
                brain: BrainImplementationHandle::new(brain as u64),
                session_id: rusty_crew_core_bridge_api::SessionId::new(session_id),
                body_state_json: body_state_json.to_vec(),
                system_prompt,
                role_assembly_json: role_assembly_json.to_vec(),
                wake_id,
            })
            .map_err(to_napi_error)?;
        Ok(JsBufferedBrainWakeRequest {
            body_state: handle_to_u32(buffered.request.body_state)?,
            system_prompt: handle_to_u32(buffered.request.system_prompt)?,
            role_assembly: handle_to_u32(buffered.request.role_assembly)?,
            provider_state_json: buffered
                .request
                .provider_state
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(|error| {
                    napi::Error::new(napi::Status::GenericFailure, error.to_string())
                })?,
            provider_state_absence: buffered
                .request
                .provider_state_absence
                .as_ref()
                .map(provider_state_absence_reason_as_str)
                .map(str::to_string),
        })
    }

    #[napi]
    pub fn build_brain_wake_request_for_session(
        &self,
        brain: f64,
        session_id: String,
        system_prompt: String,
        role_assembly_json: napi::bindgen_prelude::Buffer,
        wake_id: String,
    ) -> napi::Result<JsBufferedBrainWakeRequest> {
        let bridge = self.bridge()?;
        let buffered = bridge
            .build_brain_wake_request_for_session(
                BrainImplementationHandle::new(brain as u64),
                rusty_crew_core_bridge_api::SessionId::new(session_id),
                system_prompt,
                role_assembly_json.to_vec(),
                wake_id,
            )
            .map_err(to_napi_error)?;
        Ok(JsBufferedBrainWakeRequest {
            body_state: handle_to_u32(buffered.request.body_state)?,
            system_prompt: handle_to_u32(buffered.request.system_prompt)?,
            role_assembly: handle_to_u32(buffered.request.role_assembly)?,
            provider_state_json: buffered
                .request
                .provider_state
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(|error| {
                    napi::Error::new(napi::Status::GenericFailure, error.to_string())
                })?,
            provider_state_absence: buffered
                .request
                .provider_state_absence
                .as_ref()
                .map(provider_state_absence_reason_as_str)
                .map(str::to_string),
        })
    }

    #[napi]
    pub fn apply_brain_provider_state_output_json(
        &self,
        brain: f64,
        session_id: String,
        wake_id: String,
        output_json: String,
    ) -> napi::Result<()> {
        let bridge = self.bridge()?;
        let output = parse_brain_provider_state_output_json(&output_json).map_err(to_napi_error)?;
        bridge
            .apply_provider_state_output(
                BrainImplementationHandle::new(brain as u64),
                &rusty_crew_core_bridge_api::SessionId::new(session_id),
                &wake_id,
                output,
            )
            .map_err(to_napi_error)
    }

    #[napi]
    pub fn provider_state_diagnostics(
        &self,
        limit: Option<u32>,
    ) -> napi::Result<Vec<JsProviderStateDiagnostic>> {
        let bridge = self.bridge()?;
        bridge
            .provider_state_diagnostics(limit.unwrap_or(100))
            .map_err(to_napi_error)
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
    pub fn get_buffer(&self, handle: u32) -> napi::Result<JsRuntimeBufferView> {
        let bridge = self.bridge()?;
        let view = bridge
            .get_buffer(RuntimeBufferHandle::new(handle as u64))
            .map_err(to_napi_error)?;
        Ok(JsRuntimeBufferView {
            handle,
            media_type: view.media_type,
            byte_len: view.byte_len as f64,
            bytes: view.bytes.into(),
        })
    }

    #[napi]
    pub fn release_buffer(&self, handle: u32) -> napi::Result<()> {
        let bridge = self.bridge()?;
        bridge
            .release_buffer(RuntimeBufferHandle::new(handle as u64))
            .map_err(to_napi_error)?;
        Ok(())
    }

    #[napi]
    pub fn assert_no_buffer_leaks(&self) -> napi::Result<()> {
        let bridge = self.bridge()?;
        bridge.assert_no_buffer_leaks().map_err(to_napi_error)?;
        Ok(())
    }

    #[napi]
    pub fn submit_brain_text_delta(
        &self,
        wake_id: String,
        session_id: String,
        text: String,
    ) -> napi::Result<JsEventReceipt> {
        let bridge = self.bridge()?;
        let receipt = bridge
            .submit_brain_event(BrainEventEnvelope {
                wake_id,
                session_id: rusty_crew_core_bridge_api::SessionId::new(session_id),
                event: rusty_crew_core_bridge_api::BrainEvent::TextDelta { text },
            })
            .map_err(to_napi_error)?;
        Ok(to_js_event_receipt(receipt))
    }

    #[napi]
    pub fn inject_external_event(
        &self,
        event_json: napi::bindgen_prelude::Buffer,
    ) -> napi::Result<JsEventReceipt> {
        let event = serde_json::from_slice(event_json.as_ref()).map_err(|error| {
            napi::Error::new(
                napi::Status::InvalidArg,
                format!("invalid external event JSON: {error}"),
            )
        })?;
        let bridge = self.bridge()?;
        let receipt = bridge.inject_external_event(event).map_err(to_napi_error)?;
        Ok(to_js_event_receipt(receipt))
    }

    #[napi]
    pub fn inject_den_data_update(
        &self,
        update_json: napi::bindgen_prelude::Buffer,
    ) -> napi::Result<JsEventReceipt> {
        let update = serde_json::from_slice(update_json.as_ref()).map_err(|error| {
            napi::Error::new(
                napi::Status::InvalidArg,
                format!("invalid Den data update JSON: {error}"),
            )
        })?;
        let bridge = self.bridge()?;
        let receipt = bridge
            .inject_den_data_update(update)
            .map_err(to_napi_error)?;
        Ok(to_js_event_receipt(receipt))
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
    pub fn subscribe_events(&self, subscription: JsEventSubscription) -> napi::Result<f64> {
        let mut bridge = self.bridge()?;
        let handle = bridge
            .subscribe_events(to_event_subscription(subscription)?)
            .map_err(to_napi_error)?;
        Ok(handle.get() as f64)
    }

    #[napi]
    pub fn unsubscribe_events(&self, handle: f64) -> napi::Result<()> {
        let mut bridge = self.bridge()?;
        bridge
            .unsubscribe_events(SubscriptionHandle::new(handle as u64))
            .map_err(to_napi_error)?;
        Ok(())
    }

    #[napi]
    pub fn drain_subscription_events(
        &self,
        handle: f64,
        max_events: u32,
    ) -> napi::Result<Vec<String>> {
        let bridge = self.bridge()?;
        let events = bridge
            .drain_subscription_events(SubscriptionHandle::new(handle as u64), max_events)
            .map_err(to_napi_error)?;
        events
            .into_iter()
            .map(|event| {
                serde_json::to_string(&event).map_err(|error| {
                    napi::Error::new(napi::Status::GenericFailure, error.to_string())
                })
            })
            .collect()
    }

    #[napi]
    pub fn create_session(&self, config: JsSessionConfig) -> napi::Result<JsSessionState> {
        let bridge = self.bridge()?;
        let state = bridge
            .create_session(js_session_config(config)?)
            .map_err(to_napi_error)?;
        Ok(to_js_session_state(state))
    }

    #[napi]
    pub fn ensure_configured_session(
        &self,
        config: JsSessionConfig,
    ) -> napi::Result<JsSessionState> {
        let bridge = self.bridge()?;
        let state = bridge
            .ensure_configured_session(js_session_config(config)?)
            .map_err(to_napi_error)?;
        Ok(to_js_session_state(state))
    }

    #[napi]
    pub fn archive_session(&self, session_id: String) -> napi::Result<JsSessionState> {
        let bridge = self.bridge()?;
        let state = bridge
            .archive_session(SessionId::new(session_id))
            .map_err(to_napi_error)?;
        Ok(to_js_session_state(state))
    }

    #[napi]
    pub fn list_sessions_json(&self) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let sessions = bridge.list_sessions().map_err(to_napi_error)?;
        serde_json::to_string(&sessions)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn route_agent_message(
        &self,
        from: String,
        to: String,
        body: String,
        correlation_id: Option<String>,
    ) -> napi::Result<JsEventReceipt> {
        let bridge = self.bridge()?;
        let receipt = bridge
            .route_agent_message(
                rusty_crew_core_bridge_api::AgentId::new(from),
                rusty_crew_core_bridge_api::AgentId::new(to),
                body,
                correlation_id,
            )
            .map_err(to_napi_error)?;
        Ok(to_js_event_receipt(receipt))
    }

    #[napi]
    pub fn enqueue_body_follow_up_message(
        &self,
        session_id: String,
        from: String,
        body: String,
        correlation_id: Option<String>,
    ) -> napi::Result<JsQueuedMessageRecord> {
        let bridge = self.bridge()?;
        let record = bridge
            .enqueue_body_follow_up_message(
                rusty_crew_core_bridge_api::SessionId::new(session_id),
                rusty_crew_core_bridge_api::AgentId::new(from),
                body,
                correlation_id,
            )
            .map_err(to_napi_error)?;
        Ok(to_js_queued_message_record(record))
    }

    #[napi]
    pub fn register_scheduled_wake_job_json(
        &self,
        job_id: String,
        target_session_id: String,
        interval_ms: Option<f64>,
        first_due_at: String,
    ) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let job = bridge
            .register_scheduled_wake_job(
                job_id,
                SessionId::new(target_session_id),
                interval_ms.map(|value| value as u64),
                first_due_at,
            )
            .map_err(to_napi_error)?;
        serde_json::to_string(&job)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn register_scheduled_host_job_json(
        &self,
        job_id: String,
        job_kind: String,
        interval_ms: Option<f64>,
        first_due_at: String,
        payload_json: String,
    ) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let payload_json = serde_json::from_str(&payload_json)
            .map_err(|error| napi::Error::new(napi::Status::InvalidArg, error.to_string()))?;
        let job = bridge
            .register_scheduled_host_job(
                job_id,
                job_kind,
                interval_ms.map(|value| value as u64),
                first_due_at,
                payload_json,
            )
            .map_err(to_napi_error)?;
        serde_json::to_string(&job)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn list_scheduled_jobs_json(
        &self,
        status: Option<String>,
        job_kind: Option<String>,
        limit: Option<f64>,
        offset: Option<f64>,
    ) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let jobs = bridge
            .list_scheduled_jobs(
                status,
                job_kind,
                limit.map(|value| value as u32),
                offset.map(|value| value as u32),
            )
            .map_err(to_napi_error)?;
        serde_json::to_string(&jobs)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn list_scheduled_runs_json(
        &self,
        job_id: Option<String>,
        status: Option<String>,
        trigger: Option<String>,
        target_session_id: Option<String>,
        limit: Option<f64>,
        offset: Option<f64>,
    ) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let runs = bridge
            .list_scheduled_runs(
                job_id,
                status,
                trigger,
                target_session_id,
                limit.map(|value| value as u32),
                offset.map(|value| value as u32),
            )
            .map_err(to_napi_error)?;
        serde_json::to_string(&runs)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn claim_scheduled_host_runs_json(
        &self,
        supported_job_kinds: Vec<String>,
        limit: Option<f64>,
    ) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let runs = bridge
            .claim_scheduled_host_runs(supported_job_kinds, limit.map(|value| value as u32))
            .map_err(to_napi_error)?;
        serde_json::to_string(&runs)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn request_scheduled_host_job_run_json(
        &self,
        job_id: String,
        supported_job_kinds: Vec<String>,
    ) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let run = bridge
            .request_scheduled_host_job_run(job_id, supported_job_kinds)
            .map_err(to_napi_error)?;
        serde_json::to_string(&run)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn complete_scheduled_host_run(
        &self,
        run_id: String,
        status: String,
        output_json: String,
        error: Option<String>,
    ) -> napi::Result<()> {
        let bridge = self.bridge()?;
        let output_json = serde_json::from_str(&output_json)
            .map_err(|error| napi::Error::new(napi::Status::InvalidArg, error.to_string()))?;
        bridge
            .complete_scheduled_host_run(
                rusty_crew_core_bridge_api::RunId::new(run_id),
                status,
                output_json,
                error,
            )
            .map_err(to_napi_error)?;
        Ok(())
    }

    #[napi]
    pub fn run_scheduler_tick_json(&self) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let report = bridge.run_scheduler_tick().map_err(to_napi_error)?;
        serde_json::to_string(&report)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn request_scheduled_job_run_json(&self, job_id: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let run = bridge
            .request_scheduled_job_run(job_id)
            .map_err(to_napi_error)?;
        serde_json::to_string(&run)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn pause_scheduled_job(&self, job_id: String) -> napi::Result<()> {
        let bridge = self.bridge()?;
        bridge.pause_scheduled_job(job_id).map_err(to_napi_error)?;
        Ok(())
    }

    #[napi]
    pub fn resume_scheduled_job(&self, job_id: String, next_due_at: String) -> napi::Result<()> {
        let bridge = self.bridge()?;
        bridge
            .resume_scheduled_job(job_id, next_due_at)
            .map_err(to_napi_error)?;
        Ok(())
    }

    #[napi]
    pub fn project_body_state_json(
        &self,
        session_id: String,
    ) -> napi::Result<napi::bindgen_prelude::Buffer> {
        let bridge = self.bridge()?;
        let bytes = bridge
            .project_body_state_json(rusty_crew_core_bridge_api::SessionId::new(session_id))
            .map_err(to_napi_error)?;
        Ok(bytes.into())
    }

    #[napi]
    pub fn submit_brain_actions_json(
        &self,
        wake_id: String,
        session_id: String,
        actions_json: napi::bindgen_prelude::Buffer,
    ) -> napi::Result<JsActionBatchReceipt> {
        let bridge = self.bridge()?;
        let actions = serde_json::from_slice(actions_json.as_ref()).map_err(|error| {
            napi::Error::new(
                napi::Status::InvalidArg,
                format!("invalid brain action JSON: {error}"),
            )
        })?;
        let receipt = bridge
            .submit_brain_actions(BrainActionBatch {
                wake_id,
                session_id: rusty_crew_core_bridge_api::SessionId::new(session_id),
                actions,
            })
            .map_err(to_napi_error)?;
        Ok(JsActionBatchReceipt {
            wake_id: receipt.wake_id,
            accepted_actions: receipt.accepted_actions,
            rejected_actions_json: serde_json::to_string(&receipt.rejected_actions).map_err(
                |error| napi::Error::new(napi::Status::GenericFailure, error.to_string()),
            )?,
        })
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
    pub fn create_profile_registry_record_json(&self, write_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let write = parse_json::<ProfileRegistryWrite>(&write_json, "profile registry write")?;
        let record = bridge
            .create_profile_registry_record(&write)
            .map_err(to_napi_error)?;
        serialize_json(&record, "profile registry record")
    }

    #[napi]
    pub fn update_profile_registry_record_json(&self, update_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let update = parse_json::<ProfileRegistryUpdate>(&update_json, "profile registry update")?;
        let record = bridge
            .update_profile_registry_record(&update)
            .map_err(to_napi_error)?;
        serialize_json(&record, "profile registry record")
    }

    #[napi]
    pub fn list_profile_registry_records_json(&self, query_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let query = parse_json::<WireProfileRegistryQuery>(&query_json, "profile registry query")?;
        let records = bridge
            .list_profile_registry_records(&to_profile_registry_query(query)?)
            .map_err(to_napi_error)?;
        serialize_json(&records, "profile registry records")
    }

    #[napi]
    pub fn get_profile_registry_record_json(&self, profile_id: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let record = bridge
            .get_profile_registry_record(&rusty_crew_core_bridge_api::ProfileId::new(profile_id))
            .map_err(to_napi_error)?;
        serialize_json(&record, "profile registry record")
    }

    #[napi]
    pub fn purge_profile_json(&self, profile_id: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let report = bridge
            .purge_profile(&rusty_crew_core_bridge_api::ProfileId::new(profile_id))
            .map_err(to_napi_error)?;
        serialize_json(&report, "profile purge report")
    }

    #[napi]
    pub fn upsert_model_provider_json(&self, write_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let write = parse_json::<ModelProviderWrite>(&write_json, "model provider write")?;
        let record = bridge
            .upsert_model_provider(&write)
            .map_err(to_napi_error)?;
        serialize_json(&record, "model provider record")
    }

    #[napi]
    pub fn list_model_providers_json(&self, query_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let query = parse_json::<ModelProviderQuery>(&query_json, "model provider query")?;
        let records = bridge.list_model_providers(&query).map_err(to_napi_error)?;
        serialize_json(&records, "model provider records")
    }

    #[napi]
    pub fn get_model_provider_json(&self, alias: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let record = bridge.get_model_provider(&alias).map_err(to_napi_error)?;
        serialize_json(&record, "model provider record")
    }

    #[napi]
    pub fn get_model_provider_secret_json(&self, alias: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let secret = bridge
            .get_model_provider_secret(&alias)
            .map_err(to_napi_error)?;
        serialize_json(&secret, "model provider secret")
    }

    #[napi]
    pub fn model_provider_refresh_impact_json(&self, request_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let request = parse_json::<ModelProviderRefreshImpactRequest>(
            &request_json,
            "model provider refresh impact request",
        )?;
        let impact = bridge
            .model_provider_refresh_impact(&request)
            .map_err(to_napi_error)?;
        serialize_json(&impact, "model provider refresh impact")
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

    #[napi]
    #[allow(clippy::too_many_arguments)]
    pub fn submit_brain_event(
        &self,
        wake_id: String,
        session_id: String,
        event_type: String,
        text: Option<String>,
        tool_name: Option<String>,
        is_error: Option<bool>,
        metadata_json: Option<String>,
    ) -> napi::Result<JsEventReceipt> {
        let bridge = self.bridge()?;
        let event = match event_type.as_str() {
            "started" => rusty_crew_core_bridge_api::BrainEvent::Started,
            "text_delta" => rusty_crew_core_bridge_api::BrainEvent::TextDelta {
                text: text.unwrap_or_default(),
            },
            "reasoning_delta" => rusty_crew_core_bridge_api::BrainEvent::ReasoningDelta {
                text: text.unwrap_or_default(),
                format: tool_name,
            },
            "phase_change" => rusty_crew_core_bridge_api::BrainEvent::PhaseChange {
                phase: match tool_name.as_deref().unwrap_or("idle") {
                    "idle" => rusty_crew_core_bridge_api::BrainPhase::Idle,
                    "exploring" => rusty_crew_core_bridge_api::BrainPhase::Exploring,
                    "composing" => rusty_crew_core_bridge_api::BrainPhase::Composing,
                    "reviewing" => rusty_crew_core_bridge_api::BrainPhase::Reviewing,
                    other => {
                        return Err(napi::Error::new(
                            napi::Status::InvalidArg,
                            format!("unsupported brain phase {other}"),
                        ))
                    }
                },
                message: text,
            },
            "tool_call_started" => rusty_crew_core_bridge_api::BrainEvent::ToolCallStarted {
                tool_name: tool_name.ok_or_else(|| {
                    napi::Error::new(
                        napi::Status::InvalidArg,
                        "tool_call_started requires toolName".to_string(),
                    )
                })?,
                metadata: parse_tool_call_metadata(metadata_json.as_deref())?,
            },
            "tool_call_finished" => rusty_crew_core_bridge_api::BrainEvent::ToolCallFinished {
                tool_name: tool_name.ok_or_else(|| {
                    napi::Error::new(
                        napi::Status::InvalidArg,
                        "tool_call_finished requires toolName".to_string(),
                    )
                })?,
                is_error: is_error.unwrap_or(false),
                metadata: parse_tool_call_metadata(metadata_json.as_deref())?,
            },
            "provider_status" => rusty_crew_core_bridge_api::BrainEvent::ProviderStatus {
                level: match tool_name.as_deref().unwrap_or("info") {
                    "info" => rusty_crew_core_bridge_api::BrainProviderStatusLevel::Info,
                    "degraded" => rusty_crew_core_bridge_api::BrainProviderStatusLevel::Degraded,
                    "error" => rusty_crew_core_bridge_api::BrainProviderStatusLevel::Error,
                    other => {
                        return Err(napi::Error::new(
                            napi::Status::InvalidArg,
                            format!("unsupported provider status level {other}"),
                        ))
                    }
                },
                message: text.unwrap_or_default(),
                metadata_json,
            },
            "finished" => rusty_crew_core_bridge_api::BrainEvent::Finished,
            other => {
                return Err(napi::Error::new(
                    napi::Status::InvalidArg,
                    format!("unsupported brain event type {other}"),
                ))
            }
        };
        let receipt = bridge
            .submit_brain_event(BrainEventEnvelope {
                wake_id,
                session_id: rusty_crew_core_bridge_api::SessionId::new(session_id),
                event,
            })
            .map_err(to_napi_error)?;
        Ok(to_js_event_receipt(receipt))
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
