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
use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};
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

fn to_js_event_receipt(receipt: EventReceipt) -> JsEventReceipt {
    JsEventReceipt {
        accepted: receipt.accepted,
        sequence: receipt.sequence as f64,
    }
}

fn parse_tool_call_metadata(
    metadata_json: Option<&str>,
) -> napi::Result<Option<rusty_crew_core_bridge_api::ToolCallMetadata>> {
    metadata_json
        .map(serde_json::from_str::<rusty_crew_core_bridge_api::ToolCallMetadata>)
        .transpose()
        .map_err(|error| napi::Error::new(napi::Status::InvalidArg, error.to_string()))
}

fn to_js_session_state(state: rusty_crew_core_bridge_api::SessionState) -> JsSessionState {
    JsSessionState {
        handle: state.handle.get() as f64,
        session_id: state.session_id.0,
        agent_id: state.agent_id.0,
        profile_id: state.profile_id.0,
        kind: format!("{:?}", state.kind).to_ascii_lowercase(),
        status: format!("{:?}", state.status).to_ascii_lowercase(),
        history_window: state.history_window.map(|window| JsSessionHistoryWindow {
            max_messages: window.max_messages,
        }),
    }
}

fn to_js_profile_memory_record(record: ProfileMemoryRecord) -> napi::Result<JsProfileMemoryRecord> {
    let (target_type, target_id) = profile_memory_target_parts(&record.profile_id, &record.target);
    Ok(JsProfileMemoryRecord {
        profile_id: record.profile_id.0,
        target_type: target_type.to_string(),
        target_id,
        key: record.key,
        content: record.content,
        metadata_json: serde_json::to_string(&record.metadata)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))?,
        revision: record.revision as f64,
        created_at: record.created_at,
        updated_at: record.updated_at,
    })
}

fn to_js_simple_kv_record(record: SimpleKvRecord) -> napi::Result<JsSimpleKvRecord> {
    Ok(JsSimpleKvRecord {
        scope_type: record.scope.scope_type,
        scope_id: record.scope.scope_id,
        key: record.key,
        value_json: serde_json::to_string(&record.value_json)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))?,
        revision: record.revision as f64,
        created_at: record.created_at,
        updated_at: record.updated_at,
        expires_at: record.expires_at,
    })
}

fn to_js_queued_message_record(record: QueuedMessageRecord) -> JsQueuedMessageRecord {
    JsQueuedMessageRecord {
        message_id: record.message_id,
        owner_session_id: record.owner_session_id.map(|session_id| session_id.0),
        owner_agent_id: record.owner_agent_id.0,
        from_agent: record.message.from.0,
        to_agent: record.message.to.0,
        body: record.message.body,
        correlation_id: record.message.correlation_id,
        enqueued_at: record.enqueued_at,
        expires_at: record.expires_at,
        ttl_ms: record.ttl_ms,
        delivery_attempts: record.delivery_attempts,
        state: format!("{:?}", record.state).to_ascii_lowercase(),
        terminal_at: record.terminal_at,
        state_reason: record.state_reason,
    }
}

fn to_profile_memory_query(query: JsProfileMemoryQuery) -> napi::Result<ProfileMemoryQuery> {
    let profile_id = rusty_crew_core_bridge_api::ProfileId::new(query.profile_id);
    let target = match query.target_type {
        Some(target_type) => Some(to_profile_memory_target(
            &profile_id,
            &target_type,
            query.target_id,
        )?),
        None => None,
    };
    Ok(ProfileMemoryQuery {
        profile_id,
        target,
        page: Some(rusty_crew_core_persistence::QueryPage {
            limit: query.limit,
            offset: query.offset,
        }),
    })
}

fn to_profile_registry_query(
    query: WireProfileRegistryQuery,
) -> napi::Result<ProfileRegistryQuery> {
    Ok(ProfileRegistryQuery {
        lifecycle_status: query
            .lifecycle_status
            .as_deref()
            .map(profile_registry_lifecycle_status_from_str)
            .transpose()?,
        page: Some(rusty_crew_core_persistence::QueryPage {
            limit: query.limit,
            offset: query.offset,
        }),
    })
}

fn profile_registry_lifecycle_status_from_str(
    raw: &str,
) -> napi::Result<ProfileRegistryLifecycleStatus> {
    match raw {
        "active" => Ok(ProfileRegistryLifecycleStatus::Active),
        "paused" => Ok(ProfileRegistryLifecycleStatus::Paused),
        "decommissioned" => Ok(ProfileRegistryLifecycleStatus::Decommissioned),
        "archived" => Ok(ProfileRegistryLifecycleStatus::Archived),
        other => Err(napi::Error::new(
            napi::Status::InvalidArg,
            format!("unsupported profile registry lifecycle status {other}"),
        )),
    }
}

fn to_simple_kv_query(query: JsSimpleKvQuery) -> SimpleKvQuery {
    SimpleKvQuery {
        scope: SimpleKvScope {
            scope_type: query.scope_type,
            scope_id: query.scope_id,
        },
        key_prefix: query.key_prefix,
        include_expired: query.include_expired.unwrap_or(false),
        expired_only: query.expired_only.unwrap_or(false),
        now: query.now,
        page: Some(rusty_crew_core_persistence::QueryPage {
            limit: query.limit,
            offset: query.offset,
        }),
    }
}

fn to_simple_kv_write(write: JsSimpleKvWrite) -> napi::Result<SimpleKvWrite> {
    Ok(SimpleKvWrite {
        scope: SimpleKvScope {
            scope_type: write.scope_type,
            scope_id: write.scope_id,
        },
        key: write.key,
        value_json: serde_json::from_str(&write.value_json).map_err(|error| {
            napi::Error::new(
                napi::Status::InvalidArg,
                format!("invalid simple_kv value_json: {error}"),
            )
        })?,
        now: write.now,
        expires_at: write.expires_at,
    })
}

fn to_simple_kv_delete(delete: JsSimpleKvDelete) -> napi::Result<SimpleKvDelete> {
    if !delete.expected_revision.is_finite() || delete.expected_revision < 0.0 {
        return Err(napi::Error::new(
            napi::Status::InvalidArg,
            "simple_kv expected_revision must be a non-negative finite number",
        ));
    }
    Ok(SimpleKvDelete {
        scope: SimpleKvScope {
            scope_type: delete.scope_type,
            scope_id: delete.scope_id,
        },
        key: delete.key,
        expected_revision: delete.expected_revision as u64,
    })
}

fn to_profile_memory_write(write: JsProfileMemoryWrite) -> napi::Result<ProfileMemoryWrite> {
    let profile_id = rusty_crew_core_bridge_api::ProfileId::new(write.profile_id);
    let target = to_profile_memory_target(&profile_id, &write.target_type, write.target_id)?;
    let metadata = write
        .metadata_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|error| napi::Error::new(napi::Status::InvalidArg, error.to_string()))?
        .unwrap_or_else(|| serde_json::json!({}));
    Ok(ProfileMemoryWrite {
        profile_id,
        target,
        key: write.key,
        content: write.content,
        metadata,
        now: String::new(),
    })
}

fn to_profile_memory_target(
    profile_id: &rusty_crew_core_bridge_api::ProfileId,
    target_type: &str,
    target_id: Option<String>,
) -> napi::Result<ProfileMemoryTarget> {
    match target_type {
        "profile" => Ok(ProfileMemoryTarget::Profile),
        "user" => target_id
            .filter(|value| !value.trim().is_empty())
            .map(ProfileMemoryTarget::User)
            .ok_or_else(|| {
                napi::Error::new(
                    napi::Status::InvalidArg,
                    "user profile memory target requires targetId".to_string(),
                )
            }),
        other => Err(napi::Error::new(
            napi::Status::InvalidArg,
            format!(
                "unsupported profile memory target type {other} for profile {}",
                profile_id.0
            ),
        )),
    }
}

fn to_profile_memory_caps(caps: Option<&JsProfileMemoryCaps>) -> ProfileMemoryCaps {
    let defaults = ProfileMemoryCaps::default();
    ProfileMemoryCaps {
        max_records_per_profile: caps
            .and_then(|caps| caps.max_records_per_profile)
            .unwrap_or(defaults.max_records_per_profile),
        max_key_bytes: caps
            .and_then(|caps| caps.max_key_bytes)
            .unwrap_or(defaults.max_key_bytes),
        max_content_bytes: caps
            .and_then(|caps| caps.max_content_bytes)
            .unwrap_or(defaults.max_content_bytes),
    }
}

fn profile_memory_target_parts(
    profile_id: &rusty_crew_core_bridge_api::ProfileId,
    target: &ProfileMemoryTarget,
) -> (&'static str, String) {
    match target {
        ProfileMemoryTarget::Profile => ("profile", profile_id.0.clone()),
        ProfileMemoryTarget::User(user_id) => ("user", user_id.clone()),
    }
}

fn to_runtime_search_filter(query: JsRuntimeSearchQuery) -> napi::Result<RuntimeSearchFilter> {
    Ok(RuntimeSearchFilter {
        query: query.query,
        row_type: query
            .row_type
            .as_deref()
            .map(parse_runtime_search_row_type)
            .transpose()?,
        session_id: query
            .session_id
            .map(rusty_crew_core_bridge_api::SessionId::new),
        agent_id: query.agent_id.map(rusty_crew_core_bridge_api::AgentId::new),
        instance_id: query
            .instance_id
            .map(rusty_crew_core_bridge_api::AgentInstanceId::new),
        task_id: query.task_id.map(rusty_crew_core_bridge_api::TaskId::new),
        event_kind: query
            .event_kind
            .as_deref()
            .map(parse_event_kind)
            .transpose()?,
        recorded_after: query.recorded_after,
        recorded_before: query.recorded_before,
        limit: query.limit,
    })
}

fn to_js_runtime_search_result(result: RuntimeSearchResult) -> JsRuntimeSearchResult {
    JsRuntimeSearchResult {
        row_type: runtime_search_row_type_as_str(result.row_type).to_string(),
        row_key: result.row_key,
        sequence: result.sequence.map(|sequence| sequence as f64),
        session_id: result.session_id.map(|value| value.0),
        agent_id: result.agent_id.map(|value| value.0),
        instance_id: result.instance_id.map(|value| value.0),
        task_id: result.task_id.map(|value| value.0),
        event_kind: result.event_kind.map(|kind| format!("{kind:?}")),
        recorded_at: result.recorded_at,
        title: result.title,
        body: result.body,
    }
}

fn to_runtime_counter_query(query: JsRuntimeCounterQuery) -> napi::Result<RuntimeCounterQuery> {
    Ok(RuntimeCounterQuery {
        scope: query
            .scope_type
            .as_deref()
            .map(|scope_type| to_runtime_counter_scope(scope_type, query.scope_id.clone()))
            .transpose()?,
        counter_name: query.counter_name,
        page: Some(rusty_crew_core_persistence::QueryPage {
            limit: query.limit,
            offset: query.offset,
        }),
    })
}

fn to_runtime_counter_scope(
    scope_type: &str,
    scope_id: Option<String>,
) -> napi::Result<RuntimeCounterScope> {
    match scope_type {
        "runtime" => Ok(RuntimeCounterScope::Runtime),
        "agent" => required_scope_id(scope_type, scope_id)
            .map(rusty_crew_core_bridge_api::AgentId::new)
            .map(RuntimeCounterScope::Agent),
        "instance" => required_scope_id(scope_type, scope_id)
            .map(rusty_crew_core_bridge_api::AgentInstanceId::new)
            .map(RuntimeCounterScope::Instance),
        "session" => required_scope_id(scope_type, scope_id)
            .map(rusty_crew_core_bridge_api::SessionId::new)
            .map(RuntimeCounterScope::Session),
        other => Err(napi::Error::new(
            napi::Status::InvalidArg,
            format!("unsupported runtime counter scope type {other}"),
        )),
    }
}

fn required_scope_id(scope_type: &str, scope_id: Option<String>) -> napi::Result<String> {
    scope_id
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            napi::Error::new(
                napi::Status::InvalidArg,
                format!("runtime counter scope {scope_type} requires scopeId"),
            )
        })
}

fn to_js_runtime_counter(record: RuntimeCounterRecord) -> JsRuntimeCounterRecord {
    let (scope_type, scope_id) = runtime_counter_scope_parts(record.scope);
    JsRuntimeCounterRecord {
        scope_type,
        scope_id,
        counter_name: record.counter_name,
        value: record.value as f64,
        updated_at: record.updated_at,
    }
}

fn to_js_runtime_counter_summary(summary: RuntimeStateSummary) -> JsRuntimeCounterSummary {
    let (scope_type, scope_id) = runtime_counter_scope_parts(summary.scope);
    JsRuntimeCounterSummary {
        scope_type,
        scope_id,
        brain_turns: summary.brain_turns as f64,
        wakes: summary.wakes as f64,
        tool_calls: summary.tool_calls as f64,
        tool_errors: summary.tool_errors as f64,
        delegations_created: summary.delegations_created as f64,
        delegations_completed: summary.delegations_completed as f64,
        delegations_failed: summary.delegations_failed as f64,
        delegations_timed_out: summary.delegations_timed_out as f64,
        delegations_cancelled: summary.delegations_cancelled as f64,
        messages: summary.messages as f64,
        completions: summary.completions as f64,
        queue_expirations: summary.queue_expirations as f64,
    }
}

fn to_js_runtime_database_size(size: RuntimeDatabaseSize) -> JsRuntimeDatabaseSize {
    JsRuntimeDatabaseSize {
        database_bytes: size.database_bytes as f64,
        page_count: size.page_count as f64,
        page_size_bytes: size.page_size_bytes as f64,
        freelist_pages: size.freelist_pages as f64,
        freelist_bytes: size.freelist_bytes as f64,
        wal_bytes: size.wal_bytes as f64,
    }
}

fn to_js_schema_migration_record(record: SchemaMigrationRecord) -> JsSchemaMigrationRecord {
    JsSchemaMigrationRecord {
        version: record.version as f64,
        description: record.description,
        applied_at: record.applied_at,
    }
}

fn to_js_runtime_storage_capability(
    capability: RuntimeStorageCapability,
) -> JsRuntimeStorageCapability {
    JsRuntimeStorageCapability {
        name: capability.name,
        supported: capability.supported,
        detail: capability.detail,
    }
}

fn to_js_runtime_repository_backend_requirement(
    requirement: RuntimeRepositoryBackendRequirement,
) -> JsRuntimeRepositoryBackendRequirement {
    JsRuntimeRepositoryBackendRequirement {
        capability: requirement.capability,
        required: requirement.required,
        detail: requirement.detail,
    }
}

fn to_js_runtime_repository_group_diagnostic(
    group: RuntimeRepositoryGroupDiagnostic,
) -> JsRuntimeRepositoryGroupDiagnostic {
    JsRuntimeRepositoryGroupDiagnostic {
        group_id: group.group_id,
        label: group.label,
        correctness_sensitive: group.correctness_sensitive,
        backend_requirements: group
            .backend_requirements
            .into_iter()
            .map(to_js_runtime_repository_backend_requirement)
            .collect(),
        notes: group.notes,
    }
}

fn to_js_runtime_module_capability_status(
    status: RuntimeModuleCapabilityStatus,
) -> JsRuntimeModuleCapabilityStatus {
    JsRuntimeModuleCapabilityStatus {
        capability: status.capability,
        required: status.required,
        supported: status.supported,
        backend_variant: status.backend_variant,
    }
}

fn to_js_runtime_module_logical_store_diagnostic(
    store: RuntimeModuleLogicalStoreDiagnostic,
) -> JsRuntimeModuleLogicalStoreDiagnostic {
    JsRuntimeModuleLogicalStoreDiagnostic {
        store_name: store.store_name,
        description: store.description,
    }
}

fn to_js_runtime_module_physical_table_diagnostic(
    table: RuntimeModulePhysicalTableDiagnostic,
) -> JsRuntimeModulePhysicalTableDiagnostic {
    JsRuntimeModulePhysicalTableDiagnostic {
        table_name: table.table_name,
        logical_store: table.logical_store,
        physical_table: table.physical_table,
        declaration: table.declaration,
    }
}

fn to_js_runtime_module_physical_index_diagnostic(
    index: RuntimeModulePhysicalIndexDiagnostic,
) -> JsRuntimeModulePhysicalIndexDiagnostic {
    JsRuntimeModulePhysicalIndexDiagnostic {
        table_name: index.table_name,
        purpose: index.purpose,
        physical_index: index.physical_index,
        columns: index.columns,
        unique: index.unique,
    }
}

fn to_js_runtime_module_retention_diagnostic(
    retention: RuntimeModuleRetentionDiagnostic,
) -> JsRuntimeModuleRetentionDiagnostic {
    JsRuntimeModuleRetentionDiagnostic {
        store_name: retention.store_name,
        policy: retention.policy,
        detail: retention.detail,
    }
}

fn to_js_runtime_module_named_diagnostic(
    contract: RuntimeModuleNamedDiagnostic,
) -> JsRuntimeModuleNamedDiagnostic {
    JsRuntimeModuleNamedDiagnostic {
        name: contract.name,
        description: contract.description,
    }
}

fn to_js_runtime_module_query_catalog_diagnostic(
    entry: RuntimeModuleQueryCatalogDiagnostic,
) -> JsRuntimeModuleQueryCatalogDiagnostic {
    JsRuntimeModuleQueryCatalogDiagnostic {
        query_id: entry.query_id,
        store_name: entry.store_name,
        description: entry.description,
        parameter_schema_id: entry.parameter_schema_id,
    }
}

fn to_js_runtime_module_transfer_hook_diagnostic(
    hook: RuntimeModuleTransferHookDiagnostic,
) -> JsRuntimeModuleTransferHookDiagnostic {
    JsRuntimeModuleTransferHookDiagnostic {
        hook_name: hook.hook_name,
        format_version: hook.format_version as f64,
    }
}

fn to_js_runtime_installed_module_schema_diagnostic(
    installed: RuntimeInstalledModuleSchemaDiagnostic,
) -> JsRuntimeInstalledModuleSchemaDiagnostic {
    JsRuntimeInstalledModuleSchemaDiagnostic {
        module_id: installed.module_id,
        installed_version: installed.installed_version as f64,
        descriptor_fingerprint: installed.descriptor_fingerprint,
        installed_at: installed.installed_at,
        updated_at: installed.updated_at,
    }
}

fn to_js_runtime_module_schema_diagnostic(
    module: RuntimeModuleSchemaDiagnostic,
) -> JsRuntimeModuleSchemaDiagnostic {
    JsRuntimeModuleSchemaDiagnostic {
        module_id: module.module_id,
        owner_crate: module.owner_crate,
        owner_module: module.owner_module,
        descriptor_version: module.descriptor_version as f64,
        installed_version: module.installed_version.map(|version| version as f64),
        migration_status: module.migration_status,
        descriptor_fingerprint: module.descriptor_fingerprint,
        installed_descriptor_fingerprint: module.installed_descriptor_fingerprint,
        installed_at: module.installed_at,
        updated_at: module.updated_at,
        capability_status: module
            .capability_status
            .into_iter()
            .map(to_js_runtime_module_capability_status)
            .collect(),
        logical_stores: module
            .logical_stores
            .into_iter()
            .map(to_js_runtime_module_logical_store_diagnostic)
            .collect(),
        physical_tables: module
            .physical_tables
            .into_iter()
            .map(to_js_runtime_module_physical_table_diagnostic)
            .collect(),
        physical_indexes: module
            .physical_indexes
            .into_iter()
            .map(to_js_runtime_module_physical_index_diagnostic)
            .collect(),
        retention: module
            .retention
            .into_iter()
            .map(to_js_runtime_module_retention_diagnostic)
            .collect(),
        repository_contracts: module
            .repository_contracts
            .into_iter()
            .map(to_js_runtime_module_named_diagnostic)
            .collect(),
        query_catalog_entries: module
            .query_catalog_entries
            .into_iter()
            .map(to_js_runtime_module_query_catalog_diagnostic)
            .collect(),
        export_hooks: module
            .export_hooks
            .into_iter()
            .map(to_js_runtime_module_transfer_hook_diagnostic)
            .collect(),
        import_hooks: module
            .import_hooks
            .into_iter()
            .map(to_js_runtime_module_transfer_hook_diagnostic)
            .collect(),
        migration_notes: module.migration_notes,
        degraded_reasons: module.degraded_reasons,
        blocked_reasons: module.blocked_reasons,
    }
}

fn to_js_runtime_module_schema_registry_diagnostics(
    diagnostics: RuntimeModuleSchemaRegistryDiagnostics,
) -> JsRuntimeModuleSchemaRegistryDiagnostics {
    JsRuntimeModuleSchemaRegistryDiagnostics {
        source: diagnostics.source,
        backend_capabilities: diagnostics.backend_capabilities,
        modules: diagnostics
            .modules
            .into_iter()
            .map(to_js_runtime_module_schema_diagnostic)
            .collect(),
        orphan_installed_modules: diagnostics
            .orphan_installed_modules
            .into_iter()
            .map(to_js_runtime_installed_module_schema_diagnostic)
            .collect(),
    }
}

fn to_js_runtime_storage_table_count(
    count: RuntimeStorageTableCount,
) -> JsRuntimeStorageTableCount {
    JsRuntimeStorageTableCount {
        table: count.table,
        rows: count.rows as f64,
    }
}

fn to_js_runtime_query_plan_check(
    check: rusty_crew_core_persistence::RuntimeQueryPlanCheck,
) -> JsRuntimeQueryPlanCheck {
    JsRuntimeQueryPlanCheck {
        name: check.name.to_string(),
        uses_index: check.uses_index,
        detail: check.detail,
    }
}

fn to_js_runtime_storage_pressure_signal(
    signal: RuntimeStoragePressureSignal,
) -> JsRuntimeStoragePressureSignal {
    JsRuntimeStoragePressureSignal {
        name: signal.name,
        active: signal.active,
        severity: signal.severity,
        observed_value: signal.observed_value as f64,
        threshold_value: signal.threshold_value.map(|value| value as f64),
        detail: signal.detail,
    }
}

fn to_js_runtime_storage_connection_health(
    health: RuntimeStorageConnectionHealth,
) -> JsRuntimeStorageConnectionHealth {
    JsRuntimeStorageConnectionHealth {
        backend: health.backend,
        status: health.status,
        max_connections: health.max_connections as f64,
        active_connections: health.active_connections as f64,
        idle_connections: health.idle_connections as f64,
        total_opened: health.total_opened as f64,
        checkout_count: health.checkout_count as f64,
        checkout_reuse_count: health.checkout_reuse_count as f64,
        reconnect_attempts: health.reconnect_attempts as f64,
        reconnect_successes: health.reconnect_successes as f64,
        closed_connections_discarded: health.closed_connections_discarded as f64,
        last_error: health.last_error,
    }
}

fn to_js_runtime_storage_diagnostics(
    diagnostics: RuntimeStorageDiagnostics,
) -> JsRuntimeStorageDiagnostics {
    JsRuntimeStorageDiagnostics {
        backend: diagnostics.backend,
        backend_label: diagnostics.backend_label,
        schema_version: diagnostics.schema_version as f64,
        supported_schema_version: diagnostics.supported_schema_version as f64,
        migrations: diagnostics
            .migrations
            .into_iter()
            .map(to_js_schema_migration_record)
            .collect(),
        size: to_js_runtime_database_size(diagnostics.size),
        table_counts: diagnostics
            .table_counts
            .into_iter()
            .map(to_js_runtime_storage_table_count)
            .collect(),
        capabilities: diagnostics
            .capabilities
            .into_iter()
            .map(to_js_runtime_storage_capability)
            .collect(),
        repository_groups: diagnostics
            .repository_groups
            .into_iter()
            .map(to_js_runtime_repository_group_diagnostic)
            .collect(),
        connection_health: to_js_runtime_storage_connection_health(diagnostics.connection_health),
        module_registry: to_js_runtime_module_schema_registry_diagnostics(
            diagnostics.module_registry,
        ),
        index_checks: diagnostics
            .index_checks
            .into_iter()
            .map(to_js_runtime_query_plan_check)
            .collect(),
        search_healthy: diagnostics.search_healthy,
        pressure_signals: diagnostics
            .pressure_signals
            .into_iter()
            .map(to_js_runtime_storage_pressure_signal)
            .collect(),
        pressure: diagnostics.pressure,
    }
}

fn to_js_runtime_maintenance_report(
    report: RuntimeMaintenanceReport,
) -> JsRuntimeMaintenanceReport {
    JsRuntimeMaintenanceReport {
        size_before: to_js_runtime_database_size(report.size_before),
        size_after: to_js_runtime_database_size(report.size_after),
        expired_queue_messages: report.expired_queue_messages as f64,
        purged_terminal_queue_messages: report.purged_terminal_queue_messages as f64,
        expired_provider_wire_states: report.expired_provider_wire_states as f64,
        session_memory_compaction: to_js_session_memory_compaction_report(
            report.session_memory_compaction,
        ),
        wal_checkpoint_ran: report.wal_checkpoint_ran,
        optimize_ran: report.optimize_ran,
    }
}

fn to_js_session_memory_compaction_report(
    report: SessionMemoryCompactionReport,
) -> JsSessionMemoryCompactionReport {
    JsSessionMemoryCompactionReport {
        enabled: report.enabled,
        scopes_inspected: report.scopes_inspected as f64,
        retention_pressure_scopes: report.retention_pressure_scopes as f64,
        scopes_compacted: report.scopes_compacted as f64,
        session_summaries_created: report.session_summaries_created as f64,
        branch_summaries_created: report.branch_summaries_created as f64,
        records_archived: report.records_archived as f64,
        records_superseded: report.records_superseded as f64,
        skipped_scopes: report.skipped_scopes as f64,
    }
}

fn runtime_counter_scope_parts(scope: RuntimeCounterScope) -> (String, String) {
    match scope {
        RuntimeCounterScope::Runtime => ("runtime".to_string(), "_global".to_string()),
        RuntimeCounterScope::Agent(agent_id) => ("agent".to_string(), agent_id.0),
        RuntimeCounterScope::Instance(instance_id) => ("instance".to_string(), instance_id.0),
        RuntimeCounterScope::Session(session_id) => ("session".to_string(), session_id.0),
    }
}

fn parse_runtime_search_row_type(raw: &str) -> napi::Result<RuntimeSearchRowType> {
    match raw {
        "message" => Ok(RuntimeSearchRowType::Message),
        "queue_message" => Ok(RuntimeSearchRowType::QueueMessage),
        "session" => Ok(RuntimeSearchRowType::Session),
        other => Err(napi::Error::new(
            napi::Status::InvalidArg,
            format!("unsupported runtime search row type {other}"),
        )),
    }
}

fn runtime_search_row_type_as_str(row_type: RuntimeSearchRowType) -> &'static str {
    match row_type {
        RuntimeSearchRowType::Message => "message",
        RuntimeSearchRowType::QueueMessage => "queue_message",
        RuntimeSearchRowType::Session => "session",
    }
}

fn parse_session_kind(raw: &str) -> napi::Result<rusty_crew_core_bridge_api::SessionKind> {
    match raw {
        "full" => Ok(rusty_crew_core_bridge_api::SessionKind::Full),
        "worker" => Ok(rusty_crew_core_bridge_api::SessionKind::Worker),
        "delegated" => Ok(rusty_crew_core_bridge_api::SessionKind::Delegated),
        other => Err(napi::Error::new(
            napi::Status::InvalidArg,
            format!("unsupported session kind {other}"),
        )),
    }
}

fn js_session_config(
    config: JsSessionConfig,
) -> napi::Result<rusty_crew_core_bridge_api::SessionConfig> {
    let resource_limits = config.resource_limits;
    let tool_profile = config.tool_profile;
    let history_window = config.history_window;
    Ok(rusty_crew_core_bridge_api::SessionConfig {
        session_id: rusty_crew_core_bridge_api::SessionId::new(config.session_id),
        agent_id: rusty_crew_core_bridge_api::AgentId::new(config.agent_id),
        profile_id: rusty_crew_core_bridge_api::ProfileId::new(config.profile_id),
        kind: parse_session_kind(&config.kind)?,
        delegation: None,
        resource_limits: match resource_limits {
            Some(limits) => rusty_crew_core_bridge_api::ResourceLimits {
                workdir: limits.workdir,
                max_duration_ms: limits.max_duration_ms,
                max_delegation_depth: limits.max_delegation_depth,
            },
            None => rusty_crew_core_bridge_api::ResourceLimits {
                workdir: None,
                max_duration_ms: None,
                max_delegation_depth: None,
            },
        },
        tool_profile: match tool_profile {
            Some(profile) => rusty_crew_core_bridge_api::ToolProfile {
                tools: profile
                    .tools
                    .into_iter()
                    .map(|tool| rusty_crew_core_bridge_api::ToolDescriptor {
                        name: tool.name,
                        description: tool.description,
                        input_schema: tool
                            .input_schema
                            .map(|handle| RuntimeBufferHandle::new(handle as u64)),
                    })
                    .collect(),
            },
            None => rusty_crew_core_bridge_api::ToolProfile { tools: Vec::new() },
        },
        history_window: history_window.map(|window| {
            rusty_crew_core_bridge_api::SessionHistoryWindow {
                max_messages: window.max_messages,
            }
        }),
    })
}

fn to_brain_registration(
    registration: JsBrainImplementationRegistration,
) -> napi::Result<BrainImplementationRegistration> {
    Ok(BrainImplementationRegistration {
        implementation_id: rusty_crew_core_bridge_api::BrainImplementationId::new(
            registration.implementation_id,
        ),
        profile_id: rusty_crew_core_bridge_api::ProfileId::new(registration.profile_id),
        tool_profile: rusty_crew_core_bridge_api::ToolProfile {
            tools: registration
                .tool_profile
                .tools
                .into_iter()
                .map(|tool| rusty_crew_core_bridge_api::ToolDescriptor {
                    name: tool.name,
                    description: tool.description,
                    input_schema: tool
                        .input_schema
                        .map(|handle| RuntimeBufferHandle::new(handle as u64)),
                })
                .collect(),
        },
        model_config: rusty_crew_core_bridge_api::BrainModelConfig {
            provider: registration.model_config.provider,
            model_name: registration.model_config.model_name,
            temperature_milli: registration.model_config.temperature_milli,
            max_output_tokens: registration.model_config.max_output_tokens,
        },
        strategy: registration
            .strategy
            .map(to_brain_strategy_metadata)
            .transpose()?,
        provider_state_scope: registration.provider_state_scope.map(|scope| {
            rusty_crew_core_bridge_api::BrainProviderStateScope {
                profile_fingerprint: scope.profile_fingerprint,
                provider_fingerprint: scope.provider_fingerprint,
            }
        }),
    })
}

fn to_brain_strategy_metadata(
    strategy: JsBrainStrategyMetadata,
) -> napi::Result<rusty_crew_core_bridge_api::BrainStrategyMetadata> {
    Ok(rusty_crew_core_bridge_api::BrainStrategyMetadata {
        module_id: strategy.module_id,
        strategy_id: strategy.strategy_id,
        provider_state: rusty_crew_core_bridge_api::BrainProviderStateStrategyMetadata {
            mode: parse_provider_state_mode(&strategy.provider_state.mode)?,
        },
    })
}

fn parse_provider_state_mode(
    mode: &str,
) -> napi::Result<rusty_crew_core_bridge_api::ProviderStateMode> {
    match mode {
        "unused" => Ok(rusty_crew_core_bridge_api::ProviderStateMode::Unused),
        "optional" => Ok(rusty_crew_core_bridge_api::ProviderStateMode::Optional),
        "required" => Ok(rusty_crew_core_bridge_api::ProviderStateMode::Required),
        _ => Err(napi::Error::new(
            napi::Status::InvalidArg,
            format!("unknown provider state mode {mode}"),
        )),
    }
}

fn parse_brain_provider_state_output_json(raw: &str) -> CoreResult<BrainWakeProviderStateOutput> {
    #[derive(serde::Deserialize)]
    #[serde(tag = "type", rename_all = "snake_case")]
    enum WireOutput {
        Unchanged,
        Replace { state: WireUpdate },
        Clear { reason: WireClearReason },
    }

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct WireUpdate {
        module_id: String,
        strategy_id: String,
        profile_fingerprint: String,
        provider_fingerprint: String,
        payload_version: String,
        payload: serde_json::Value,
        ttl_ms: Option<u64>,
    }

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "snake_case")]
    enum WireClearReason {
        BrainRequestedClear,
    }

    let parsed = serde_json::from_str::<WireOutput>(raw).map_err(|error| {
        CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("invalid provider state output json: {error}"),
        )
    })?;
    Ok(match parsed {
        WireOutput::Unchanged => BrainWakeProviderStateOutput::Unchanged,
        WireOutput::Replace { state } => BrainWakeProviderStateOutput::Replace {
            state: rusty_crew_core_bridge_api::BrainWakeProviderStateUpdate {
                module_id: state.module_id,
                strategy_id: state.strategy_id,
                profile_fingerprint: state.profile_fingerprint,
                provider_fingerprint: state.provider_fingerprint,
                payload_version: state.payload_version,
                payload: state.payload,
                ttl_ms: state.ttl_ms,
            },
        },
        WireOutput::Clear { reason } => BrainWakeProviderStateOutput::Clear {
            reason: match reason {
                WireClearReason::BrainRequestedClear => {
                    rusty_crew_core_bridge_api::ProviderStateClearReason::BrainRequestedClear
                }
            },
        },
    })
}

fn provider_state_absence_reason_as_str(
    reason: &rusty_crew_core_bridge_api::ProviderStateAbsenceReason,
) -> &'static str {
    match reason {
        rusty_crew_core_bridge_api::ProviderStateAbsenceReason::NotConfigured => "not_configured",
        rusty_crew_core_bridge_api::ProviderStateAbsenceReason::Missing => "missing",
        rusty_crew_core_bridge_api::ProviderStateAbsenceReason::Expired => "expired",
        rusty_crew_core_bridge_api::ProviderStateAbsenceReason::Invalidated => "invalidated",
        rusty_crew_core_bridge_api::ProviderStateAbsenceReason::ModuleDoesNotUseState => {
            "module_does_not_use_state"
        }
        rusty_crew_core_bridge_api::ProviderStateAbsenceReason::LoadFailed => "load_failed",
    }
}

fn parse_provider_state_absence_reason(
    raw: &str,
) -> CoreResult<rusty_crew_core_bridge_api::ProviderStateAbsenceReason> {
    Ok(match raw {
        "not_configured" => rusty_crew_core_bridge_api::ProviderStateAbsenceReason::NotConfigured,
        "missing" => rusty_crew_core_bridge_api::ProviderStateAbsenceReason::Missing,
        "expired" => rusty_crew_core_bridge_api::ProviderStateAbsenceReason::Expired,
        "invalidated" => rusty_crew_core_bridge_api::ProviderStateAbsenceReason::Invalidated,
        "module_does_not_use_state" => {
            rusty_crew_core_bridge_api::ProviderStateAbsenceReason::ModuleDoesNotUseState
        }
        "load_failed" => rusty_crew_core_bridge_api::ProviderStateAbsenceReason::LoadFailed,
        other => {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("invalid provider state absence reason {other}"),
            ))
        }
    })
}

fn provider_wire_state_status(
    invalidated_at: Option<&String>,
    invalidation_reason: Option<&str>,
    expires_at: Option<&String>,
    now: &String,
) -> &'static str {
    if invalidation_reason == Some("expired") {
        return "expired";
    }
    if invalidated_at.is_some() {
        return "invalidated";
    }
    if expires_at.is_some_and(|expires| expires <= now) {
        return "expired";
    }
    "valid"
}

fn to_event_subscription(subscription: JsEventSubscription) -> napi::Result<EventSubscription> {
    Ok(EventSubscription {
        event_kinds: subscription
            .event_kinds
            .into_iter()
            .map(|kind| parse_event_kind(&kind))
            .collect::<napi::Result<Vec<_>>>()?,
        session_id: subscription
            .session_id
            .map(rusty_crew_core_bridge_api::SessionId::new),
        agent_id: subscription
            .agent_id
            .map(rusty_crew_core_bridge_api::AgentId::new),
        adapter_id: subscription
            .adapter_id
            .map(rusty_crew_core_bridge_api::AdapterId::new),
    })
}

fn to_platform_adapter_registration(
    registration: JsPlatformAdapterRegistration,
) -> napi::Result<PlatformAdapterRegistration> {
    Ok(PlatformAdapterRegistration {
        adapter_id: rusty_crew_core_bridge_api::AdapterId::new(registration.adapter_id),
        kind: parse_platform_adapter_kind(&registration.kind)?,
        display_name: registration.display_name,
    })
}

fn parse_platform_adapter_kind(
    raw: &str,
) -> napi::Result<rusty_crew_core_bridge_api::PlatformAdapterKind> {
    match raw {
        "den" => Ok(rusty_crew_core_bridge_api::PlatformAdapterKind::Den),
        "telegram" => Ok(rusty_crew_core_bridge_api::PlatformAdapterKind::Telegram),
        "mcp" => Ok(rusty_crew_core_bridge_api::PlatformAdapterKind::Mcp),
        "tui" => Ok(rusty_crew_core_bridge_api::PlatformAdapterKind::Tui),
        "cli" => Ok(rusty_crew_core_bridge_api::PlatformAdapterKind::Cli),
        other => Err(napi::Error::new(
            napi::Status::InvalidArg,
            format!("unsupported platform adapter kind {other}"),
        )),
    }
}

fn parse_event_kind(raw: &str) -> napi::Result<rusty_crew_core_bridge_api::CoreEventKind> {
    match raw {
        "session_created" => Ok(rusty_crew_core_bridge_api::CoreEventKind::SessionCreated),
        "session_archived" => Ok(rusty_crew_core_bridge_api::CoreEventKind::SessionArchived),
        "agent_message_routed" => Ok(rusty_crew_core_bridge_api::CoreEventKind::AgentMessageRouted),
        "delegation_lifecycle_observed" => {
            Ok(rusty_crew_core_bridge_api::CoreEventKind::DelegationLifecycleObserved)
        }
        "external_event_injected" => {
            Ok(rusty_crew_core_bridge_api::CoreEventKind::ExternalEventInjected)
        }
        "den_data_updated" => Ok(rusty_crew_core_bridge_api::CoreEventKind::DenDataUpdated),
        "brain_wake_requested" => Ok(rusty_crew_core_bridge_api::CoreEventKind::BrainWakeRequested),
        "brain_event_observed" => Ok(rusty_crew_core_bridge_api::CoreEventKind::BrainEventObserved),
        "brain_actions_accepted" => {
            Ok(rusty_crew_core_bridge_api::CoreEventKind::BrainActionsAccepted)
        }
        "completion_packet_delivered" => {
            Ok(rusty_crew_core_bridge_api::CoreEventKind::CompletionPacketDelivered)
        }
        other => Err(napi::Error::new(
            napi::Status::InvalidArg,
            format!("unsupported event kind {other}"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_crew_core_bridge_api::{
        AgentId, BrainAction, BrainActionBatch, BrainImplementationHandle, BrainImplementationId,
        BrainModelConfig, BrainProviderStateScope, BrainProviderStateStrategyMetadata,
        BrainStrategyMetadata, BrainWakeProviderStateOutput, BrainWakeProviderStateUpdate,
        CoreEventKind, EventSubscription, ProfileId, ProviderStateMode, ResourceLimits,
        SessionConfig, SessionId, SessionKind, ShutdownRequest, ToolDescriptor, ToolProfile,
    };
    use rusty_crew_core_protocol::{
        ModelProviderSecretEnvelope, MODEL_PROVIDER_SECRET_ENVELOPE_VERSION,
    };
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::thread;

    #[test]
    fn native_bridge_exposes_the_current_manifest_surface() {
        let bridge = NativeBridge::new();

        assert_eq!(bridge.manifest_version(), MANIFEST_VERSION);
        assert_eq!(bridge.operation_names(), OPERATION_NAMES);
        assert_eq!(bridge.wire_shape_fingerprint(), wire_shape_fingerprint());
        assert!(bridge.operation_names().contains(&"get_buffer"));
        assert!(bridge.operation_names().contains(&"release_buffer"));
        assert_eq!(
            bridge.manifest_summary().native_package,
            "@rusty-crew/native-bridge"
        );
    }

    #[test]
    fn openai_responses_bridge_uses_oauth_bearer_and_headers_without_secret_update() {
        let server = FakeResponsesServer::new();
        let mut bridge = NativeBridge::new();
        bridge
            .initialize_engine(EngineConfig {
                engine_data_dir: std::env::temp_dir()
                    .join(format!(
                        "rusty-crew-native-openai-oauth-{}",
                        std::process::id()
                    ))
                    .to_string_lossy()
                    .to_string(),
                clock: rusty_crew_core_bridge_api::ClockConfig::Fixed {
                    at: "2026-07-02T00:00:00Z".to_string(),
                },
                default_turn_budget: 3,
                default_idle_timeout_ms: 1000,
                storage: None,
            })
            .unwrap();
        bridge
            .create_session(SessionConfig {
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
            })
            .unwrap();
        let body_state: serde_json::Value = serde_json::from_slice(
            &bridge
                .project_body_state_json(SessionId::new("responses-session"))
                .unwrap(),
        )
        .unwrap();
        let secret = ModelProviderSecretEnvelope::OpenAiOauth {
            version: MODEL_PROVIDER_SECRET_ENVELOPE_VERSION,
            issuer: "http://127.0.0.1:9".to_string(),
            client_id: "client".to_string(),
            id_token: test_jwt(4_102_444_800, serde_json::json!({})),
            access_token: test_jwt(4_102_444_800, serde_json::json!({})),
            refresh_token: "refresh-secret".to_string(),
            exchanged_api_token: None,
            last_refresh_at: Some("2026-07-02T00:00:00Z".to_string()),
            account_id: Some("account-1".to_string()),
            email: None,
            plan_type: None,
            is_fedramp_account: true,
            access_token_expires_at: None,
        };
        let input = json!({
            "wakeId": "wake-oauth",
            "sessionId": "responses-session",
            "bodyState": body_state,
            "config": {"model": "gpt-5", "instructions": "say ok"},
            "client": {
                "mode": "live",
                "base_url": server.base_url(),
                "auth_kind": "openai_oauth",
                "provider_alias": "gpt",
                "oauth_credential_secret": secret.to_storage_text().unwrap()
            }
        });

        let output: serde_json::Value = serde_json::from_str(
            &run_openai_responses_brain_json_blocking(input.to_string()).unwrap(),
        )
        .unwrap();

        assert!(output.get("credential_secret_update").unwrap().is_null());
        let captured = server.captured();
        assert!(captured.contains("post /responses http/1.1"));
        assert!(captured.contains("authorization: bearer "));
        assert!(captured.contains("chatgpt-account-id: account-1"));
        assert!(captured.contains("x-openai-fedramp: true"));
        assert!(!captured.contains("refresh-secret"));
    }

    #[test]
    fn native_bridge_releases_buffer_handles_once() {
        let bridge = NativeBridge::new();
        let buffered = bridge
            .build_brain_wake_request(BrainWakeBufferInput {
                brain: BrainImplementationHandle::new(1),
                session_id: SessionId::new("session"),
                body_state_json: vec![b'{', b'}'],
                system_prompt: "system".to_string(),
                role_assembly_json: vec![b'{', b'}'],
                wake_id: "wake".to_string(),
            })
            .unwrap();
        let body_handle = buffered.request.body_state;

        assert_eq!(bridge.get_buffer(body_handle).unwrap().bytes, b"{}");
        bridge.release_buffer(body_handle).unwrap();
        let error = bridge
            .release_buffer(body_handle)
            .expect_err("double release must fail loudly");

        assert_eq!(error.kind, CoreErrorKind::NotFound);
    }

    #[test]
    fn openai_responses_tool_schema_normalization_preserves_required_fields() {
        let schema = serde_json::json!({
            "properties": {
                "project_id": { "type": "string" },
                "status": { "type": "string" }
            },
            "required": ["project_id"]
        });
        assert_eq!(
            normalize_responses_tool_schema(&schema),
            serde_json::json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "string" },
                    "status": { "type": "string" }
                },
                "required": ["project_id"]
            })
        );
        assert_eq!(
            normalize_responses_tool_schema(&serde_json::json!("not-a-schema")),
            serde_json::json!({"type": "object", "properties": {}})
        );
    }

    #[test]
    fn native_bridge_reports_leaked_runtime_buffers() {
        let bridge = NativeBridge::new();
        let _buffered = bridge
            .build_brain_wake_request(BrainWakeBufferInput {
                brain: BrainImplementationHandle::new(1),
                session_id: SessionId::new("session"),
                body_state_json: vec![b'{', b'}'],
                system_prompt: "system".to_string(),
                role_assembly_json: vec![b'{', b'}'],
                wake_id: "wake".to_string(),
            })
            .unwrap();

        let error = bridge
            .assert_no_buffer_leaks()
            .expect_err("unreleased wake buffers should be visible");

        assert_eq!(error.kind, CoreErrorKind::InternalError);
    }

    #[test]
    fn native_bridge_registers_brain_implementations_with_stable_handles() {
        let mut bridge = NativeBridge::new();
        let first = bridge
            .register_brain_implementation(brain_registration("planner", "planner-profile"))
            .unwrap();
        let second = bridge
            .register_brain_implementation(brain_registration("coder", "coder-profile"))
            .unwrap();

        assert_eq!(first, BrainImplementationHandle::new(1));
        assert_eq!(second, BrainImplementationHandle::new(2));
    }

    #[test]
    fn native_bridge_rejects_duplicate_brain_registration_ids() {
        let mut bridge = NativeBridge::new();
        bridge
            .register_brain_implementation(brain_registration("planner", "planner-profile"))
            .unwrap();
        let error = bridge
            .register_brain_implementation(brain_registration("planner", "other-profile"))
            .expect_err("duplicate implementation ids must fail");

        assert_eq!(error.kind, CoreErrorKind::AlreadyExists);
    }

    #[test]
    fn native_bridge_rejects_duplicate_profile_brain_registrations() {
        let mut bridge = NativeBridge::new();
        bridge
            .register_brain_implementation(brain_registration("planner", "planner-profile"))
            .unwrap();
        let error = bridge
            .register_brain_implementation(brain_registration("other", "planner-profile"))
            .expect_err("duplicate profile bindings must fail");

        assert_eq!(error.kind, CoreErrorKind::AlreadyExists);
    }

    #[test]
    fn native_bridge_replaces_profile_brain_registration_in_place() {
        let mut bridge = NativeBridge::new();
        let handle = bridge
            .register_brain_implementation(brain_registration("planner", "planner-profile"))
            .unwrap();
        let replaced = bridge
            .replace_brain_implementation(brain_registration_with_tools(
                "planner-rebuilt",
                "planner-profile",
                vec!["read_file", "patch"],
            ))
            .unwrap();

        assert_eq!(replaced, handle);
        let registration = bridge.brain_registrations.get(handle).unwrap();
        assert_eq!(
            registration.implementation_id.to_string(),
            "planner-rebuilt"
        );
        assert_eq!(registration.tool_profile.tools.len(), 2);
    }

    #[test]
    fn native_bridge_replace_registers_missing_profile_brain() {
        let mut bridge = NativeBridge::new();
        let handle = bridge
            .replace_brain_implementation(brain_registration("planner", "planner-profile"))
            .unwrap();

        assert_eq!(handle, BrainImplementationHandle::new(1));
        let registration = bridge.brain_registrations.get(handle).unwrap();
        assert_eq!(registration.profile_id.to_string(), "planner-profile");
    }

    #[test]
    fn native_bridge_unregisters_profile_brain_and_allows_reregister() {
        let mut bridge = NativeBridge::new();
        let handle = bridge
            .register_brain_implementation(brain_registration("planner", "planner-profile"))
            .unwrap();

        let removed = bridge
            .unregister_brain_implementation_for_profile(ProfileId::new("planner-profile"))
            .unwrap();
        assert_eq!(removed, handle);
        assert!(bridge.brain_registrations.get(handle).is_err());

        let next = bridge
            .register_brain_implementation(brain_registration("planner-next", "planner-profile"))
            .unwrap();
        assert_ne!(next, handle);
        let registration = bridge.brain_registrations.get(next).unwrap();
        assert_eq!(registration.profile_id.to_string(), "planner-profile");
    }

    #[test]
    fn native_bridge_unregister_missing_profile_brain_fails_closed() {
        let mut bridge = NativeBridge::new();
        let error = bridge
            .unregister_brain_implementation_for_profile(ProfileId::new("missing-profile"))
            .expect_err("missing profile brain unregister must fail");

        assert_eq!(error.kind, CoreErrorKind::NotFound);
    }

    #[test]
    fn native_bridge_rejects_replacement_using_another_profile_implementation_id() {
        let mut bridge = NativeBridge::new();
        bridge
            .register_brain_implementation(brain_registration("planner", "planner-profile"))
            .unwrap();
        bridge
            .register_brain_implementation(brain_registration("coder", "coder-profile"))
            .unwrap();

        let error = bridge
            .replace_brain_implementation(brain_registration("coder", "planner-profile"))
            .expect_err("replacement cannot steal another profile implementation id");

        assert_eq!(error.kind, CoreErrorKind::AlreadyExists);
    }

    #[test]
    fn native_bridge_mirrors_registered_tool_profiles_into_delegated_sessions() {
        let mut bridge = NativeBridge::new();
        bridge
            .register_brain_implementation(brain_registration_with_tools(
                "coder",
                "coder-profile",
                vec!["read_file", "patch"],
            ))
            .unwrap();
        bridge
            .initialize_engine(EngineConfig {
                engine_data_dir: std::env::temp_dir()
                    .join(format!(
                        "rusty-crew-native-tool-profile-{}",
                        std::process::id()
                    ))
                    .to_string_lossy()
                    .to_string(),
                clock: rusty_crew_core_bridge_api::ClockConfig::Fixed {
                    at: "2026-06-19T00:00:00Z".to_string(),
                },
                default_turn_budget: 3,
                default_idle_timeout_ms: 1000,
                storage: None,
            })
            .unwrap();
        let planner = bridge
            .create_session(SessionConfig {
                session_id: SessionId::new("planner-session"),
                agent_id: AgentId::new("planner"),
                profile_id: ProfileId::new("planner-profile"),
                kind: SessionKind::Full,
                delegation: None,
                resource_limits: ResourceLimits {
                    workdir: None,
                    max_duration_ms: None,
                    max_delegation_depth: Some(1),
                },
                tool_profile: ToolProfile {
                    tools: vec![ToolDescriptor {
                        name: "planner_only".to_string(),
                        description: "Only visible to the planner".to_string(),
                        input_schema: None,
                    }],
                },
                history_window: None,
            })
            .unwrap();

        bridge
            .submit_brain_actions(BrainActionBatch {
                wake_id: "planner-wake".to_string(),
                session_id: planner.session_id.clone(),
                actions: vec![BrainAction::RequestDelegation {
                    profile_id: ProfileId::new("coder-profile"),
                    task_id: None,
                    prompt: "use registered coder tools".to_string(),
                    expected_output: None,
                    resource_limits: None,
                    timeout_ms: None,
                    priority: None,
                    fan_out_group_id: None,
                    fan_out_max_concurrency: None,
                    fan_out_failure_policy: None,
                    correlation_id: None,
                    parent_consumption: None,
                    capacity_request: None,
                }],
            })
            .unwrap();

        let body_json = bridge
            .project_body_state_json(SessionId::new("planner-session:delegated:planner-wake:0"))
            .unwrap();
        let body: rusty_crew_core_bridge_api::BodyState =
            serde_json::from_slice(&body_json).expect("delegated body state should deserialize");

        assert_eq!(
            body.session
                .tool_profile
                .tools
                .iter()
                .map(|tool| tool.name.as_str())
                .collect::<Vec<_>>(),
            vec!["read_file", "patch"]
        );
    }

    #[test]
    fn native_bridge_hydrates_and_updates_provider_state_around_wakes() {
        let mut bridge = NativeBridge::new();
        bridge
            .initialize_engine(EngineConfig {
                engine_data_dir: std::env::temp_dir()
                    .join(format!(
                        "rusty-crew-native-provider-state-{}",
                        std::process::id()
                    ))
                    .to_string_lossy()
                    .to_string(),
                clock: rusty_crew_core_bridge_api::ClockConfig::Fixed {
                    at: "2026-06-24T00:00:00Z".to_string(),
                },
                default_turn_budget: 3,
                default_idle_timeout_ms: 1000,
                storage: None,
            })
            .unwrap();
        let optional_handle = bridge
            .register_brain_implementation(provider_state_brain_registration(
                "optional-provider-brain",
                "optional-provider-profile",
                ProviderStateMode::Optional,
            ))
            .unwrap();
        let required_handle = bridge
            .register_brain_implementation(provider_state_brain_registration(
                "required-provider-brain",
                "required-provider-profile",
                ProviderStateMode::Required,
            ))
            .unwrap();
        bridge
            .create_session(provider_state_session_config(
                "optional-provider-session",
                "optional-provider-profile",
            ))
            .unwrap();
        bridge
            .create_session(provider_state_session_config(
                "required-provider-session",
                "required-provider-profile",
            ))
            .unwrap();

        let first_optional = bridge
            .build_brain_wake_request_for_session(
                optional_handle,
                SessionId::new("optional-provider-session"),
                "system".to_string(),
                b"{}".to_vec(),
                "wake-1".to_string(),
            )
            .unwrap();
        assert!(first_optional.request.provider_state.is_none());
        assert_eq!(
            first_optional.request.provider_state_absence,
            Some(rusty_crew_core_bridge_api::ProviderStateAbsenceReason::Missing)
        );

        bridge
            .apply_provider_state_output(
                optional_handle,
                &SessionId::new("optional-provider-session"),
                "wake-1",
                BrainWakeProviderStateOutput::Replace {
                    state: BrainWakeProviderStateUpdate {
                        module_id: "openai-responses".to_string(),
                        strategy_id: "replay".to_string(),
                        profile_fingerprint: "profile-fingerprint".to_string(),
                        provider_fingerprint: "provider-fingerprint".to_string(),
                        payload_version: "provider-owned-v1".to_string(),
                        payload: serde_json::json!({"response_id": "resp-1"}),
                        ttl_ms: Some(60_000),
                    },
                },
            )
            .unwrap();
        let hydrated = bridge
            .build_brain_wake_request_for_session(
                optional_handle,
                SessionId::new("optional-provider-session"),
                "system".to_string(),
                b"{}".to_vec(),
                "wake-2".to_string(),
            )
            .unwrap();
        let state = hydrated
            .request
            .provider_state
            .expect("provider state should hydrate after replace");
        assert_eq!(state.module_id, "openai-responses");
        assert_eq!(state.strategy_id, "replay");
        assert_eq!(state.payload_version, "provider-owned-v1");
        assert_eq!(state.payload, serde_json::json!({"response_id": "resp-1"}));
        assert!(hydrated.request.provider_state_absence.is_none());

        let changed_scope_handle = bridge
            .register_brain_implementation(provider_state_brain_registration_with_scope(
                "optional-provider-brain-changed-scope",
                "optional-provider-profile-changed-scope",
                ProviderStateMode::Optional,
                "changed-profile-fingerprint",
                "provider-fingerprint",
            ))
            .unwrap();
        let invalidated = bridge
            .build_brain_wake_request_for_session(
                changed_scope_handle,
                SessionId::new("optional-provider-session"),
                "system".to_string(),
                b"{}".to_vec(),
                "wake-changed-scope".to_string(),
            )
            .unwrap();
        assert!(invalidated.request.provider_state.is_none());
        assert_eq!(
            invalidated.request.provider_state_absence,
            Some(rusty_crew_core_bridge_api::ProviderStateAbsenceReason::Invalidated)
        );

        bridge
            .apply_provider_state_output(
                optional_handle,
                &SessionId::new("optional-provider-session"),
                "wake-2b",
                BrainWakeProviderStateOutput::Replace {
                    state: BrainWakeProviderStateUpdate {
                        module_id: "openai-responses".to_string(),
                        strategy_id: "replay".to_string(),
                        profile_fingerprint: "profile-fingerprint".to_string(),
                        provider_fingerprint: "provider-fingerprint".to_string(),
                        payload_version: "provider-owned-v1".to_string(),
                        payload: serde_json::json!({"response_id": "resp-2"}),
                        ttl_ms: Some(60_000),
                    },
                },
            )
            .unwrap();

        bridge
            .apply_provider_state_output(
                optional_handle,
                &SessionId::new("optional-provider-session"),
                "wake-2",
                BrainWakeProviderStateOutput::Clear {
                    reason:
                        rusty_crew_core_bridge_api::ProviderStateClearReason::BrainRequestedClear,
                },
            )
            .unwrap();
        let after_clear = bridge
            .build_brain_wake_request_for_session(
                optional_handle,
                SessionId::new("optional-provider-session"),
                "system".to_string(),
                b"{}".to_vec(),
                "wake-3".to_string(),
            )
            .unwrap();
        assert!(after_clear.request.provider_state.is_none());
        assert_eq!(
            after_clear.request.provider_state_absence,
            Some(rusty_crew_core_bridge_api::ProviderStateAbsenceReason::Missing)
        );

        let required_error = bridge
            .build_brain_wake_request_for_session(
                required_handle,
                SessionId::new("required-provider-session"),
                "system".to_string(),
                b"{}".to_vec(),
                "wake-required".to_string(),
            )
            .expect_err("required state should fail before provider invocation");
        assert_eq!(required_error.kind, CoreErrorKind::BrainUnavailable);
    }

    #[test]
    fn native_bridge_submits_brain_events_to_the_engine() {
        let mut bridge = NativeBridge::new();
        bridge
            .initialize_engine(EngineConfig {
                engine_data_dir: std::env::temp_dir()
                    .join(format!("rusty-crew-native-event-{}", std::process::id()))
                    .to_string_lossy()
                    .to_string(),
                clock: rusty_crew_core_bridge_api::ClockConfig::Fixed {
                    at: "2026-06-19T00:00:00Z".to_string(),
                },
                default_turn_budget: 3,
                default_idle_timeout_ms: 1000,
                storage: None,
            })
            .unwrap();

        let receipt = bridge
            .submit_brain_event(BrainEventEnvelope {
                wake_id: "wake".to_string(),
                session_id: SessionId::new("session"),
                event: rusty_crew_core_bridge_api::BrainEvent::Started,
            })
            .unwrap();

        assert!(receipt.accepted);
    }

    #[test]
    fn native_bridge_shutdown_reports_and_clears_subscriptions() {
        let mut bridge = NativeBridge::new();
        let engine = bridge
            .initialize_engine(EngineConfig {
                engine_data_dir: std::env::temp_dir()
                    .join(format!("rusty-crew-native-shutdown-{}", std::process::id()))
                    .to_string_lossy()
                    .to_string(),
                clock: rusty_crew_core_bridge_api::ClockConfig::Fixed {
                    at: "2026-06-19T00:00:00Z".to_string(),
                },
                default_turn_budget: 3,
                default_idle_timeout_ms: 1000,
                storage: None,
            })
            .unwrap();
        bridge
            .create_session(SessionConfig {
                session_id: SessionId::new("shutdown-session"),
                agent_id: AgentId::new("shutdown-agent"),
                profile_id: ProfileId::new("shutdown-profile"),
                kind: SessionKind::Full,
                delegation: None,
                resource_limits: ResourceLimits {
                    workdir: None,
                    max_duration_ms: None,
                    max_delegation_depth: None,
                },
                tool_profile: ToolProfile { tools: vec![] },
                history_window: None,
            })
            .unwrap();
        let subscription = bridge
            .subscribe_events(EventSubscription {
                event_kinds: vec![CoreEventKind::SessionArchived],
                session_id: None,
                agent_id: None,
                adapter_id: None,
            })
            .unwrap();

        let summary = bridge
            .shutdown_engine(ShutdownRequest {
                engine,
                drain_timeout_ms: 25,
            })
            .unwrap();

        assert_eq!(summary.archived_sessions, 1);
        assert_eq!(summary.dropped_subscriptions, 1);
        let error = bridge
            .drain_subscription_events(subscription, 1)
            .expect_err("shutdown should clear native subscription handles");
        assert_eq!(error.kind, CoreErrorKind::NotFound);
    }

    fn brain_registration(
        implementation_id: &str,
        profile_id: &str,
    ) -> BrainImplementationRegistration {
        brain_registration_with_tools(implementation_id, profile_id, Vec::new())
    }

    fn brain_registration_with_tools(
        implementation_id: &str,
        profile_id: &str,
        tools: Vec<&str>,
    ) -> BrainImplementationRegistration {
        BrainImplementationRegistration {
            implementation_id: BrainImplementationId::new(implementation_id),
            profile_id: ProfileId::new(profile_id),
            tool_profile: ToolProfile {
                tools: tools
                    .into_iter()
                    .map(|name| ToolDescriptor {
                        name: name.to_string(),
                        description: format!("{name} tool"),
                        input_schema: None,
                    })
                    .collect(),
            },
            model_config: BrainModelConfig {
                provider: "local".to_string(),
                model_name: "deterministic".to_string(),
                temperature_milli: None,
                max_output_tokens: None,
            },
            strategy: Some(rusty_crew_core_bridge_api::BrainStrategyMetadata::unused(
                "local", "default",
            )),
            provider_state_scope: None,
        }
    }

    fn provider_state_brain_registration(
        implementation_id: &str,
        profile_id: &str,
        mode: ProviderStateMode,
    ) -> BrainImplementationRegistration {
        provider_state_brain_registration_with_scope(
            implementation_id,
            profile_id,
            mode,
            "profile-fingerprint",
            "provider-fingerprint",
        )
    }

    fn provider_state_brain_registration_with_scope(
        implementation_id: &str,
        profile_id: &str,
        mode: ProviderStateMode,
        profile_fingerprint: &str,
        provider_fingerprint: &str,
    ) -> BrainImplementationRegistration {
        let mut registration = brain_registration(implementation_id, profile_id);
        registration.strategy = Some(BrainStrategyMetadata {
            module_id: "openai-responses".to_string(),
            strategy_id: "replay".to_string(),
            provider_state: BrainProviderStateStrategyMetadata { mode },
        });
        registration.provider_state_scope = Some(BrainProviderStateScope {
            profile_fingerprint: profile_fingerprint.to_string(),
            provider_fingerprint: provider_fingerprint.to_string(),
        });
        registration
    }

    fn provider_state_session_config(session_id: &str, profile_id: &str) -> SessionConfig {
        SessionConfig {
            session_id: SessionId::new(session_id),
            agent_id: AgentId::new(format!("agent:{session_id}")),
            profile_id: ProfileId::new(profile_id),
            kind: SessionKind::Full,
            delegation: None,
            resource_limits: ResourceLimits {
                workdir: None,
                max_duration_ms: None,
                max_delegation_depth: None,
            },
            tool_profile: ToolProfile { tools: Vec::new() },
            history_window: None,
        }
    }

    struct FakeResponsesServer {
        addr: String,
        captured: Arc<Mutex<Option<String>>>,
    }

    impl FakeResponsesServer {
        fn new() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap().to_string();
            let captured = Arc::new(Mutex::new(None));
            let captured_for_thread = Arc::clone(&captured);
            thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buffer = Vec::new();
                let mut chunk = [0_u8; 4096];
                loop {
                    let read = stream.read(&mut chunk).unwrap();
                    if read == 0 {
                        break;
                    }
                    buffer.extend_from_slice(&chunk[..read]);
                    if request_complete(&buffer) {
                        break;
                    }
                }
                let request_text = String::from_utf8_lossy(&buffer).to_lowercase();
                *captured_for_thread.lock().unwrap() = Some(request_text);
                let body = concat!(
                    "data: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}\n\n",
                    "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp-test\",\"usage\":{\"input_tokens\":1,\"input_tokens_details\":{\"cached_tokens\":0},\"output_tokens\":1,\"output_tokens_details\":{\"reasoning_tokens\":0},\"total_tokens\":2}}}\n\n",
                    "data: [DONE]\n\n"
                );
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).unwrap();
            });
            Self { addr, captured }
        }

        fn base_url(&self) -> String {
            format!("http://{}", self.addr)
        }

        fn captured(&self) -> String {
            for _ in 0..100 {
                if let Some(captured) = self.captured.lock().unwrap().clone() {
                    return captured;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            panic!("fake responses server did not capture a request");
        }
    }

    fn request_complete(buffer: &[u8]) -> bool {
        let text = String::from_utf8_lossy(buffer);
        let Some((headers, body)) = text.split_once("\r\n\r\n") else {
            return false;
        };
        let content_length = headers
            .lines()
            .find_map(|line| {
                line.strip_prefix("content-length:")
                    .or_else(|| line.strip_prefix("Content-Length:"))
            })
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(0);
        body.len() >= content_length
    }

    fn test_jwt(exp: i64, extra: serde_json::Value) -> String {
        let mut payload = serde_json::json!({"exp": exp});
        let serde_json::Value::Object(payload_map) = &mut payload else {
            unreachable!();
        };
        if let serde_json::Value::Object(extra_map) = extra {
            for (key, value) in extra_map {
                payload_map.insert(key, value);
            }
        }
        format!(
            "{}.{}.{}",
            base64_url(r#"{"alg":"none"}"#.as_bytes()),
            base64_url(serde_json::to_string(&payload).unwrap().as_bytes()),
            "sig"
        )
    }

    fn base64_url(bytes: &[u8]) -> String {
        const TABLE: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut output = String::new();
        let mut index = 0;
        while index < bytes.len() {
            let a = bytes[index];
            let b = bytes.get(index + 1).copied().unwrap_or(0);
            let c = bytes.get(index + 2).copied().unwrap_or(0);
            output.push(TABLE[(a >> 2) as usize] as char);
            output.push(TABLE[(((a & 0b0000_0011) << 4) | (b >> 4)) as usize] as char);
            if index + 1 < bytes.len() {
                output.push(TABLE[(((b & 0b0000_1111) << 2) | (c >> 6)) as usize] as char);
            }
            if index + 2 < bytes.len() {
                output.push(TABLE[(c & 0b0011_1111) as usize] as char);
            }
            index += 3;
        }
        output
    }
}
