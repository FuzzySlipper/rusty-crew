//! Native Node transport boundary.
//!
//! napi-rs glue belongs in this crate. The transport-neutral pieces here expose
//! the current manifest surface and own runtime buffers without leaking native
//! transport dependencies into core crates.

use rusty_crew_core_bridge_api::{
    manifest_summary, ActionBatchReceipt, BrainActionBatch, BrainEventEnvelope,
    BrainImplementationHandle, BrainImplementationRegistration, BrainWakeAccepted,
    BrainWakeBufferInput, BrainWakeProviderStateOutput, BrainWakeRequest, BridgeManifestSummary,
    CoreError, CoreErrorKind, CoreEvent, CoreResult, DenDataUpdate, EngineConfig, EngineHandle,
    EventReceipt, EventSubscription, ExternalEvent, PlatformAdapterHandle,
    PlatformAdapterRegistration, ProfileId, RuntimeBufferHandle, RuntimeBufferStore,
    RuntimeBufferView, SessionId, ShutdownRequest, ShutdownSummary, SubscriptionHandle, Unit,
    MANIFEST_VERSION, OPERATION_NAMES,
};
use rusty_crew_core_config::{
    plan_create_profile, plan_runtime_config, validate_runtime_config_input, CreateProfilePlan,
    CreateProfilePlanInput, RuntimeConfigPlan, RuntimeConfigValidationInput,
};
use rusty_crew_core_engine::CoreEngine;
use rusty_crew_core_persistence::{
    AttachmentQuery, AttachmentRecord, AttachmentWrite, ConversationBranchQuery,
    ConversationBranchRecord, ConversationBranchStateRecord, ConversationBranchWrite,
    ConversationJumpRequest, ConversationJumpResult, ConversationSnapshotQuery,
    ConversationSnapshotRecord, ConversationSnapshotWrite, DataBankScopeQuery, DataBankScopeRecord,
    DataBankScopeWrite, MessageSlotQuery, MessageSlotRecord, MessageSlotWrite, MessageVariantQuery,
    MessageVariantRecord, MessageVariantWrite, ProfileMemoryCaps, ProfileMemoryDelete,
    ProfileMemoryQuery, ProfileMemoryRecord, ProfileMemoryReplace, ProfileMemoryTarget,
    ProfileMemoryWrite, QueuedMessageRecord, RuntimeCounterQuery, RuntimeCounterRecord,
    RuntimeCounterScope, RuntimeDatabaseSize, RuntimeMaintenancePolicy, RuntimeMaintenanceReport,
    RuntimeSearchFilter, RuntimeSearchResult, RuntimeSearchRowType, RuntimeStateSummary,
    ScheduledJobRecord, ScheduledJobStatus, ScheduledRunRecord, ScheduledRunStatus,
    ScheduledRunTrigger, SelectActiveBranchRequest, SelectActiveBranchResult,
    SelectActiveVariantRequest, SelectActiveVariantResult, UpdateBranchHeadRequest,
    UpdateBranchHeadResult,
};
use rusty_crew_core_protocol::{
    AttachmentId, BodyState, BrainWakeProviderStateInput, DataBankScopeId, MessageSlotId,
    MessageVariantId,
};
use rusty_crew_openai_responses_brain::{
    FakeResponsesClient, LiveResponsesClient, NeutralBrainTool, NeutralToolExecutor,
    NeutralToolOutput, PendingResponsesFunctionCall, ResponsesBrainConfig, ResponsesEvent,
    ResponsesOutputItem, ResponsesReplayBrain, ResponsesTokenUsage,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Receiver;
use std::sync::Mutex;

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

    pub fn manifest_version(&self) -> u32 {
        MANIFEST_VERSION
    }

    pub fn operation_names(&self) -> &'static [&'static str] {
        OPERATION_NAMES
    }

    pub fn manifest_summary(&self) -> BridgeManifestSummary {
        manifest_summary()
    }

    pub fn validate_runtime_config_draft(
        &self,
        input: RuntimeConfigValidationInput,
    ) -> rusty_crew_core_config::RuntimeConfigValidationResult {
        validate_runtime_config_input(&input)
    }

    pub fn plan_create_profile(&self, input: CreateProfilePlanInput) -> CreateProfilePlan {
        plan_create_profile(&input)
    }

    pub fn plan_runtime_config(&self, input: RuntimeConfigValidationInput) -> RuntimeConfigPlan {
        plan_runtime_config(&input)
    }

    pub fn initialize_engine(&mut self, config: EngineConfig) -> CoreResult<EngineHandle> {
        if self.engine.is_some() {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                "native bridge engine is already initialized",
            ));
        }

        let engine = CoreEngine::initialize(config)?;
        for registration in self.brain_registrations.registrations() {
            engine.register_profile_tool_profile(
                registration.profile_id.clone(),
                registration.tool_profile.clone(),
            )?;
        }
        let handle = engine.handle();
        self.engine = Some(engine);
        Ok(handle)
    }

    pub fn shutdown_engine(&mut self, request: ShutdownRequest) -> CoreResult<ShutdownSummary> {
        let engine = self.engine.take().ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                "native bridge engine is not initialized",
            )
        })?;
        let summary = engine.shutdown_with_timeout(request.drain_timeout_ms)?;
        self.subscriptions.clear();
        Ok(summary)
    }

    pub fn register_brain_implementation(
        &mut self,
        registration: BrainImplementationRegistration,
    ) -> CoreResult<rusty_crew_core_bridge_api::BrainImplementationHandle> {
        let handle = self.brain_registrations.register(registration.clone())?;
        if let Some(engine) = &self.engine {
            engine.register_profile_tool_profile(
                registration.profile_id,
                registration.tool_profile,
            )?;
        }
        Ok(handle)
    }

    pub fn replace_brain_implementation(
        &mut self,
        registration: BrainImplementationRegistration,
    ) -> CoreResult<rusty_crew_core_bridge_api::BrainImplementationHandle> {
        let handle = self
            .brain_registrations
            .replace_for_profile(registration.clone())?;
        if let Some(engine) = &self.engine {
            engine.register_profile_tool_profile(
                registration.profile_id,
                registration.tool_profile,
            )?;
        }
        Ok(handle)
    }

    pub fn unregister_brain_implementation_for_profile(
        &mut self,
        profile_id: ProfileId,
    ) -> CoreResult<BrainImplementationHandle> {
        let handle = self
            .brain_registrations
            .unregister_for_profile(&profile_id)?;
        if let Some(engine) = &self.engine {
            engine.unregister_profile_tool_profile(&profile_id)?;
        }
        Ok(handle)
    }

    pub fn wake_brain(&self, request: BrainWakeRequest) -> CoreResult<BrainWakeAccepted> {
        self.brain_registrations.get(request.brain)?;
        self.get_buffer(request.body_state)?;
        self.get_buffer(request.system_prompt)?;
        self.get_buffer(request.role_assembly)?;
        // Callback invocation is owned by the TS runtime wrapper, which binds a
        // BrainWakeExecutor to the registered handle. This Rust method only
        // validates the handle/buffer request shape until bridge codegen grows
        // a transport-neutral callback story.
        Err(not_implemented("wake_brain"))
    }

    pub fn submit_brain_event(&self, event: BrainEventEnvelope) -> CoreResult<EventReceipt> {
        self.engine()?.submit_brain_event(event)
    }

    pub fn submit_brain_actions(&self, batch: BrainActionBatch) -> CoreResult<ActionBatchReceipt> {
        self.engine()?.execute_brain_actions(batch)
    }

    pub fn create_session(
        &self,
        config: rusty_crew_core_bridge_api::SessionConfig,
    ) -> CoreResult<rusty_crew_core_bridge_api::SessionState> {
        self.engine()?.create_session(config)
    }

    pub fn ensure_configured_session(
        &self,
        config: rusty_crew_core_bridge_api::SessionConfig,
    ) -> CoreResult<rusty_crew_core_bridge_api::SessionState> {
        self.engine()?.ensure_configured_session(config)
    }

    pub fn archive_session(
        &self,
        session_id: SessionId,
    ) -> CoreResult<rusty_crew_core_bridge_api::SessionState> {
        self.engine()?.archive_session(&session_id)
    }

    pub fn list_sessions(&self) -> CoreResult<Vec<rusty_crew_core_bridge_api::SessionState>> {
        self.engine()?.list_sessions()
    }

    pub fn route_agent_message(
        &self,
        from: rusty_crew_core_bridge_api::AgentId,
        to: rusty_crew_core_bridge_api::AgentId,
        body: String,
        correlation_id: Option<String>,
    ) -> CoreResult<EventReceipt> {
        self.engine()?
            .route_agent_message(rusty_crew_core_bridge_api::AgentMessage {
                from,
                to,
                body,
                correlation_id,
            })
    }

    pub fn enqueue_body_follow_up_message(
        &self,
        session_id: SessionId,
        from: rusty_crew_core_bridge_api::AgentId,
        body: String,
        correlation_id: Option<String>,
    ) -> CoreResult<QueuedMessageRecord> {
        self.engine()?
            .enqueue_body_follow_up_message(&session_id, from, body, correlation_id)
    }

    pub fn register_scheduled_wake_job(
        &self,
        job_id: String,
        target_session_id: SessionId,
        interval_ms: Option<u64>,
        first_due_at: String,
    ) -> CoreResult<serde_json::Value> {
        self.engine()?
            .register_scheduled_wake_job(job_id, target_session_id, interval_ms, first_due_at)
            .map(scheduled_job_json)
    }

    pub fn register_scheduled_host_job(
        &self,
        job_id: String,
        job_kind: String,
        interval_ms: Option<u64>,
        first_due_at: String,
        payload_json: serde_json::Value,
    ) -> CoreResult<serde_json::Value> {
        self.engine()?
            .register_scheduled_host_job(job_id, job_kind, interval_ms, first_due_at, payload_json)
            .map(scheduled_job_json)
    }

    pub fn list_scheduled_jobs(
        &self,
        status: Option<String>,
        job_kind: Option<String>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> CoreResult<Vec<serde_json::Value>> {
        let status = status
            .as_deref()
            .map(scheduled_job_status_from_str)
            .transpose()?;
        self.engine()?
            .list_scheduled_jobs(status, job_kind, limit, offset)
            .map(|jobs| jobs.into_iter().map(scheduled_job_json).collect())
    }

    pub fn list_scheduled_runs(
        &self,
        job_id: Option<String>,
        status: Option<String>,
        trigger: Option<String>,
        target_session_id: Option<String>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> CoreResult<Vec<serde_json::Value>> {
        let status = status
            .as_deref()
            .map(scheduled_run_status_from_str)
            .transpose()?;
        let trigger = trigger
            .as_deref()
            .map(scheduled_run_trigger_from_str)
            .transpose()?;
        self.engine()?
            .list_scheduled_runs(
                job_id,
                status,
                trigger,
                target_session_id.map(SessionId::new),
                limit,
                offset,
            )
            .map(|runs| runs.into_iter().map(scheduled_run_json).collect())
    }

    pub fn claim_scheduled_host_runs(
        &self,
        supported_job_kinds: Vec<String>,
        limit: Option<u32>,
    ) -> CoreResult<Vec<serde_json::Value>> {
        self.engine()?
            .claim_scheduled_host_runs(supported_job_kinds, limit)
            .map(|runs| runs.into_iter().map(scheduled_run_json).collect())
    }

    pub fn request_scheduled_host_job_run(
        &self,
        job_id: String,
        supported_job_kinds: Vec<String>,
    ) -> CoreResult<Option<serde_json::Value>> {
        self.engine()?
            .request_scheduled_host_job_run(&job_id, supported_job_kinds)
            .map(|run| run.map(scheduled_run_json))
    }

    pub fn complete_scheduled_host_run(
        &self,
        run_id: rusty_crew_core_bridge_api::RunId,
        status: String,
        output_json: serde_json::Value,
        error: Option<String>,
    ) -> CoreResult<Unit> {
        let status = scheduled_run_status_from_str(&status)?;
        self.engine()?
            .complete_scheduled_host_run(&run_id, status, output_json, error)?;
        Ok(Unit)
    }

    pub fn run_scheduler_tick(&self) -> CoreResult<serde_json::Value> {
        self.engine()?.run_scheduler_tick().map(|report| {
            serde_json::json!({
                "stale_runs_expired": report.stale_runs_expired,
                "due_runs_claimed": report.due_runs_claimed,
                "wakes_requested": report.wakes_requested,
                "runs_completed": report.runs_completed,
                "runs_skipped": report.runs_skipped,
                "runs_failed": report.runs_failed,
            })
        })
    }

    pub fn request_scheduled_job_run(
        &self,
        job_id: String,
    ) -> CoreResult<Option<serde_json::Value>> {
        self.engine()?
            .request_scheduled_job_run(&job_id)
            .map(|run| run.map(scheduled_run_json))
    }

    pub fn pause_scheduled_job(&self, job_id: String) -> CoreResult<Unit> {
        self.engine()?.pause_scheduled_job(&job_id)?;
        Ok(Unit)
    }

    pub fn resume_scheduled_job(&self, job_id: String, next_due_at: String) -> CoreResult<Unit> {
        self.engine()?.resume_scheduled_job(&job_id, next_due_at)?;
        Ok(Unit)
    }

    pub fn project_body_state_json(
        &self,
        session_id: rusty_crew_core_bridge_api::SessionId,
    ) -> CoreResult<Vec<u8>> {
        let state = self.engine()?.project_body_state(&session_id)?;
        serde_json::to_vec(&state).map_err(|error| {
            CoreError::new(
                CoreErrorKind::InternalError,
                format!("serialize body state: {error}"),
            )
        })
    }

    pub fn count_rows(&self, table: &str) -> CoreResult<u64> {
        self.engine()?.count_rows(table)
    }

    pub fn database_size(&self) -> CoreResult<RuntimeDatabaseSize> {
        self.engine()?.database_size()
    }

    pub fn run_maintenance(
        &self,
        policy: &RuntimeMaintenancePolicy,
    ) -> CoreResult<RuntimeMaintenanceReport> {
        self.engine()?.run_maintenance(policy)
    }

    pub fn save_message_slot(&self, slot: &MessageSlotWrite) -> CoreResult<()> {
        self.engine()?.save_message_slot(slot)
    }

    pub fn save_message_variant(
        &self,
        variant: &MessageVariantWrite,
    ) -> CoreResult<MessageVariantRecord> {
        self.engine()?.save_message_variant(variant)
    }

    pub fn query_message_slots(
        &self,
        query: &MessageSlotQuery,
    ) -> CoreResult<Vec<MessageSlotRecord>> {
        self.engine()?.query_message_slots(query)
    }

    pub fn query_message_variants(
        &self,
        query: &MessageVariantQuery,
    ) -> CoreResult<Vec<MessageVariantRecord>> {
        self.engine()?.query_message_variants(query)
    }

    pub fn save_conversation_branch(
        &self,
        branch: &ConversationBranchWrite,
    ) -> CoreResult<ConversationBranchRecord> {
        self.engine()?.save_conversation_branch(branch)
    }

    pub fn query_conversation_branches(
        &self,
        query: &ConversationBranchQuery,
    ) -> CoreResult<Vec<ConversationBranchRecord>> {
        self.engine()?.query_conversation_branches(query)
    }

    pub fn get_conversation_branch_state(
        &self,
        session_id: &SessionId,
        default_updated_at: &rusty_crew_core_bridge_api::IsoTimestamp,
    ) -> CoreResult<ConversationBranchStateRecord> {
        self.engine()?
            .get_conversation_branch_state(session_id, default_updated_at)
    }

    pub fn select_active_conversation_branch(
        &self,
        request: &SelectActiveBranchRequest,
    ) -> CoreResult<SelectActiveBranchResult> {
        self.engine()?.select_active_conversation_branch(request)
    }

    pub fn update_conversation_branch_head(
        &self,
        request: &UpdateBranchHeadRequest,
    ) -> CoreResult<UpdateBranchHeadResult> {
        self.engine()?.update_conversation_branch_head(request)
    }

    pub fn save_conversation_snapshot(
        &self,
        snapshot: &ConversationSnapshotWrite,
    ) -> CoreResult<ConversationSnapshotRecord> {
        self.engine()?.save_conversation_snapshot(snapshot)
    }

    pub fn query_conversation_snapshots(
        &self,
        query: &ConversationSnapshotQuery,
    ) -> CoreResult<Vec<ConversationSnapshotRecord>> {
        self.engine()?.query_conversation_snapshots(query)
    }

    pub fn resolve_conversation_jump(
        &self,
        request: &ConversationJumpRequest,
    ) -> CoreResult<ConversationJumpResult> {
        self.engine()?.resolve_conversation_jump(request)
    }

    pub fn save_attachment(&self, attachment: &AttachmentWrite) -> CoreResult<AttachmentRecord> {
        self.engine()?.save_attachment(attachment)
    }

    pub fn query_attachments(&self, query: &AttachmentQuery) -> CoreResult<Vec<AttachmentRecord>> {
        self.engine()?.query_attachments(query)
    }

    pub fn remove_attachment(
        &self,
        attachment_id: &AttachmentId,
        updated_at: &rusty_crew_core_bridge_api::IsoTimestamp,
    ) -> CoreResult<AttachmentRecord> {
        self.engine()?.remove_attachment(attachment_id, updated_at)
    }

    pub fn save_data_bank_scope(
        &self,
        scope: &DataBankScopeWrite,
    ) -> CoreResult<DataBankScopeRecord> {
        self.engine()?.save_data_bank_scope(scope)
    }

    pub fn query_data_bank_scopes(
        &self,
        query: &DataBankScopeQuery,
    ) -> CoreResult<Vec<DataBankScopeRecord>> {
        self.engine()?.query_data_bank_scopes(query)
    }

    pub fn remove_data_bank_scope(
        &self,
        scope_id: &DataBankScopeId,
        updated_at: &rusty_crew_core_bridge_api::IsoTimestamp,
    ) -> CoreResult<DataBankScopeRecord> {
        self.engine()?.remove_data_bank_scope(scope_id, updated_at)
    }

    pub fn select_active_message_variant(
        &self,
        request: &SelectActiveVariantRequest,
    ) -> CoreResult<SelectActiveVariantResult> {
        self.engine()?.select_active_message_variant(request)
    }

    pub fn delete_message_variant(
        &self,
        slot_id: &MessageSlotId,
        variant_id: &MessageVariantId,
        updated_at: &rusty_crew_core_bridge_api::IsoTimestamp,
    ) -> CoreResult<MessageSlotRecord> {
        self.engine()?
            .delete_message_variant(slot_id, variant_id, updated_at)
    }

    pub fn reorder_message_variants(
        &self,
        slot_id: &MessageSlotId,
        ordered_variant_ids: &[MessageVariantId],
        updated_at: &rusty_crew_core_bridge_api::IsoTimestamp,
    ) -> CoreResult<Vec<MessageVariantRecord>> {
        self.engine()?
            .reorder_message_variants(slot_id, ordered_variant_ids, updated_at)
    }

    pub fn list_profile_memory(
        &self,
        query: &ProfileMemoryQuery,
    ) -> CoreResult<Vec<ProfileMemoryRecord>> {
        self.engine()?.list_profile_memory(query)
    }

    pub fn get_profile_memory(
        &self,
        profile_id: &rusty_crew_core_bridge_api::ProfileId,
        target: &ProfileMemoryTarget,
        key: &str,
    ) -> CoreResult<Option<ProfileMemoryRecord>> {
        self.engine()?.get_profile_memory(profile_id, target, key)
    }

    pub fn add_profile_memory(
        &self,
        write: ProfileMemoryWrite,
        caps: &ProfileMemoryCaps,
    ) -> CoreResult<ProfileMemoryRecord> {
        self.engine()?.add_profile_memory(write, caps)
    }

    pub fn replace_profile_memory(
        &self,
        replace: ProfileMemoryReplace,
        caps: &ProfileMemoryCaps,
    ) -> CoreResult<ProfileMemoryRecord> {
        self.engine()?.replace_profile_memory(replace, caps)
    }

    pub fn remove_profile_memory(
        &self,
        delete: &ProfileMemoryDelete,
    ) -> CoreResult<ProfileMemoryRecord> {
        self.engine()?.remove_profile_memory(delete)
    }

    pub fn search_runtime(
        &self,
        filter: &RuntimeSearchFilter,
    ) -> CoreResult<Vec<RuntimeSearchResult>> {
        self.engine()?.search_runtime(filter)
    }

    pub fn query_runtime_counters(
        &self,
        query: &RuntimeCounterQuery,
    ) -> CoreResult<Vec<RuntimeCounterRecord>> {
        self.engine()?.query_runtime_counters(query)
    }

    pub fn runtime_summary(&self, scope: &RuntimeCounterScope) -> CoreResult<RuntimeStateSummary> {
        self.engine()?.runtime_summary(scope)
    }

    pub fn reset_runtime_counters(&self, query: &RuntimeCounterQuery) -> CoreResult<u64> {
        self.engine()?.reset_runtime_counters(query)
    }

    pub fn register_platform_adapter(
        &mut self,
        registration: PlatformAdapterRegistration,
    ) -> CoreResult<PlatformAdapterHandle> {
        self.adapter_registrations.register(registration)
    }

    pub fn inject_external_event(&self, event: ExternalEvent) -> CoreResult<EventReceipt> {
        self.engine()?.inject_external_event(event)
    }

    pub fn inject_den_data_update(&self, update: DenDataUpdate) -> CoreResult<EventReceipt> {
        self.engine()?.inject_den_data_update(update)
    }

    pub fn cancel_delegated_session(
        &self,
        delegated_session_id: SessionId,
    ) -> CoreResult<rusty_crew_core_bridge_api::SessionState> {
        self.engine()?
            .cancel_delegated_session(&delegated_session_id)
    }

    pub fn request_delegated_checkpoint(
        &self,
        parent_session_id: SessionId,
        delegated_session_id: SessionId,
        reason: String,
    ) -> CoreResult<EventReceipt> {
        self.engine()?.request_delegated_checkpoint(
            &parent_session_id,
            &delegated_session_id,
            reason,
        )
    }

    pub fn drain_delegated_sessions(
        &self,
        parent_session_id: Option<SessionId>,
    ) -> CoreResult<Vec<SessionId>> {
        self.engine()?
            .drain_delegated_sessions(parent_session_id.as_ref())
    }

    pub fn cleanup_delegated_resources(
        &self,
    ) -> CoreResult<rusty_crew_core_bridge_api::DelegatedResourceCleanupReport> {
        self.engine()?.cleanup_delegated_resources()
    }

    pub fn delegated_session_status(
        &self,
        delegated_session_id: SessionId,
    ) -> CoreResult<rusty_crew_core_bridge_api::DelegatedSessionRuntimeStatus> {
        self.engine()?
            .delegated_session_status(&delegated_session_id)
    }

    pub fn subscribe_events(
        &mut self,
        subscription: EventSubscription,
    ) -> CoreResult<SubscriptionHandle> {
        let (bus_subscription_id, receiver) = self.engine()?.subscribe_events(subscription)?;
        Ok(self.subscriptions.insert(bus_subscription_id, receiver))
    }

    pub fn unsubscribe_events(&mut self, handle: SubscriptionHandle) -> CoreResult<Unit> {
        let record = self.subscriptions.remove(handle)?;
        self.engine()?
            .unsubscribe_events(record.bus_subscription_id)?;
        Ok(Unit)
    }

    pub fn drain_subscription_events(
        &self,
        handle: SubscriptionHandle,
        max_events: u32,
    ) -> CoreResult<Vec<CoreEvent>> {
        self.subscriptions.drain(handle, max_events)
    }

    pub fn build_brain_wake_request(
        &self,
        input: BrainWakeBufferInput,
    ) -> CoreResult<rusty_crew_core_bridge_api::BufferedBrainWakeRequest> {
        let mut buffered = self.buffers.build_brain_wake_request(input)?;
        self.hydrate_provider_state(&mut buffered.request)?;
        Ok(buffered)
    }

    pub fn build_brain_wake_request_for_session(
        &self,
        brain: BrainImplementationHandle,
        session_id: rusty_crew_core_bridge_api::SessionId,
        system_prompt: String,
        role_assembly_json: Vec<u8>,
        wake_id: String,
    ) -> CoreResult<rusty_crew_core_bridge_api::BufferedBrainWakeRequest> {
        let body_state = self.engine()?.prepare_body_state_for_wake(&session_id)?;
        let body_state_json = serde_json::to_vec(&body_state).map_err(|error| {
            CoreError::new(
                CoreErrorKind::InternalError,
                format!("serialize body state: {error}"),
            )
        })?;
        self.build_brain_wake_request(BrainWakeBufferInput {
            brain,
            session_id,
            body_state_json,
            system_prompt,
            role_assembly_json,
            wake_id,
        })
    }

    fn hydrate_provider_state(&self, request: &mut BrainWakeRequest) -> CoreResult<()> {
        let Ok(registration) = self.brain_registrations.get(request.brain) else {
            return Ok(());
        };
        let Some(engine) = &self.engine else {
            return Ok(());
        };
        let hydration = engine.provider_state_for_wake(registration, &request.session_id)?;
        request.provider_state = hydration.state;
        request.provider_state_absence = hydration.absence_reason;
        Ok(())
    }

    pub fn apply_provider_state_output(
        &self,
        brain: BrainImplementationHandle,
        session_id: &SessionId,
        wake_id: &str,
        output: BrainWakeProviderStateOutput,
    ) -> CoreResult<()> {
        let registration = self.brain_registrations.get(brain)?;
        self.engine()?
            .apply_provider_state_output(registration, session_id, wake_id, output)
    }

    pub fn provider_state_diagnostics(
        &self,
        limit: u32,
    ) -> CoreResult<Vec<JsProviderStateDiagnostic>> {
        let now = self.engine()?.diagnostic_now();
        self.engine()?
            .provider_wire_state_diagnostics(limit)
            .map(|records| {
                records
                    .into_iter()
                    .map(|record| {
                        let status = provider_wire_state_status(
                            record.invalidated_at.as_ref(),
                            record.invalidation_reason.as_deref(),
                            record.expires_at.as_ref(),
                            &now,
                        );
                        JsProviderStateDiagnostic {
                            session_id: record.key.session_id.0,
                            module_id: record.key.module_id,
                            strategy_id: record.key.strategy_id,
                            status: status.to_string(),
                            payload_version: Some(record.payload_version),
                            payload_bytes: Some(record.payload_bytes as f64),
                            created_at: Some(record.created_at),
                            updated_at: Some(record.updated_at),
                            expires_at: record.expires_at,
                            last_wake_id: record.last_wake_id,
                            invalidated_at: record.invalidated_at,
                            invalidation_reason: record.invalidation_reason,
                        }
                    })
                    .collect()
            })
    }

    pub fn get_buffer(&self, handle: RuntimeBufferHandle) -> CoreResult<RuntimeBufferView> {
        self.buffers.get_buffer(handle)
    }

    pub fn release_buffer(&self, handle: RuntimeBufferHandle) -> CoreResult<Unit> {
        self.buffers.release_buffer(handle)?;
        Ok(Unit)
    }

    pub fn assert_no_buffer_leaks(&self) -> CoreResult<()> {
        self.buffers.assert_no_leaks()
    }

    fn engine(&self) -> CoreResult<&CoreEngine> {
        self.engine.as_ref().ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                "native bridge engine is not initialized",
            )
        })
    }
}

impl Default for NativeBridge {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
struct BrainImplementationRegistry {
    next_handle: u64,
    by_handle: HashMap<BrainImplementationHandle, BrainImplementationRegistration>,
    by_implementation_id:
        HashMap<rusty_crew_core_bridge_api::BrainImplementationId, BrainImplementationHandle>,
    by_profile_id: HashMap<rusty_crew_core_bridge_api::ProfileId, BrainImplementationHandle>,
}

impl BrainImplementationRegistry {
    fn new() -> Self {
        Self {
            next_handle: 1,
            by_handle: HashMap::new(),
            by_implementation_id: HashMap::new(),
            by_profile_id: HashMap::new(),
        }
    }

    fn register(
        &mut self,
        registration: BrainImplementationRegistration,
    ) -> CoreResult<BrainImplementationHandle> {
        validate_brain_registration(&registration)?;

        if self
            .by_implementation_id
            .contains_key(&registration.implementation_id)
        {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!(
                    "brain implementation {} is already registered",
                    registration.implementation_id
                ),
            ));
        }

        if self.by_profile_id.contains_key(&registration.profile_id) {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!(
                    "brain implementation for profile {} is already registered",
                    registration.profile_id
                ),
            ));
        }

        let handle = BrainImplementationHandle::new(self.next_handle);
        self.next_handle = self.next_handle.checked_add(1).ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::InvalidInput,
                "brain implementation handle overflow",
            )
        })?;

        self.by_implementation_id
            .insert(registration.implementation_id.clone(), handle);
        self.by_profile_id
            .insert(registration.profile_id.clone(), handle);
        self.by_handle.insert(handle, registration);

        Ok(handle)
    }

    fn replace_for_profile(
        &mut self,
        registration: BrainImplementationRegistration,
    ) -> CoreResult<BrainImplementationHandle> {
        validate_brain_registration(&registration)?;

        let Some(handle) = self.by_profile_id.get(&registration.profile_id).copied() else {
            return self.register(registration);
        };

        if let Some(existing_handle) = self
            .by_implementation_id
            .get(&registration.implementation_id)
            .copied()
        {
            if existing_handle != handle {
                return Err(CoreError::new(
                    CoreErrorKind::AlreadyExists,
                    format!(
                        "brain implementation {} is already registered",
                        registration.implementation_id
                    ),
                ));
            }
        }

        let previous = self.by_handle.insert(handle, registration.clone());
        if let Some(previous) = previous {
            self.by_implementation_id
                .remove(&previous.implementation_id);
        }
        self.by_implementation_id
            .insert(registration.implementation_id.clone(), handle);
        self.by_profile_id
            .insert(registration.profile_id.clone(), handle);

        Ok(handle)
    }

    fn get(
        &self,
        handle: BrainImplementationHandle,
    ) -> CoreResult<&BrainImplementationRegistration> {
        self.by_handle.get(&handle).ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::BrainUnavailable,
                format!(
                    "brain implementation handle {} is not registered",
                    handle.get()
                ),
            )
        })
    }

    fn unregister_for_profile(
        &mut self,
        profile_id: &rusty_crew_core_bridge_api::ProfileId,
    ) -> CoreResult<BrainImplementationHandle> {
        let handle = self.by_profile_id.remove(profile_id).ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("brain implementation for profile {profile_id} is not registered"),
            )
        })?;
        let registration = self.by_handle.remove(&handle).ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::BrainUnavailable,
                format!(
                    "brain implementation handle {} is not registered",
                    handle.get()
                ),
            )
        })?;
        self.by_implementation_id
            .remove(&registration.implementation_id);
        Ok(handle)
    }

    fn registrations(&self) -> impl Iterator<Item = &BrainImplementationRegistration> {
        self.by_handle.values()
    }
}

#[derive(Debug)]
struct SubscriptionRecord {
    bus_subscription_id: u64,
    receiver: Receiver<CoreEvent>,
}

#[derive(Debug)]
struct SubscriptionRegistry {
    next_handle: u64,
    by_handle: HashMap<SubscriptionHandle, SubscriptionRecord>,
}

#[derive(Debug)]
struct PlatformAdapterRegistry {
    next_handle: u64,
    by_handle: HashMap<PlatformAdapterHandle, PlatformAdapterRegistration>,
    by_adapter_id: HashMap<rusty_crew_core_bridge_api::AdapterId, PlatformAdapterHandle>,
}

impl PlatformAdapterRegistry {
    fn new() -> Self {
        Self {
            next_handle: 1,
            by_handle: HashMap::new(),
            by_adapter_id: HashMap::new(),
        }
    }

    fn register(
        &mut self,
        registration: PlatformAdapterRegistration,
    ) -> CoreResult<PlatformAdapterHandle> {
        if registration.adapter_id.0.trim().is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "platform adapter requires an adapter_id",
            ));
        }
        if registration.display_name.trim().is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "platform adapter requires a display_name",
            ));
        }
        if self.by_adapter_id.contains_key(&registration.adapter_id) {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!(
                    "platform adapter {} is already registered",
                    registration.adapter_id
                ),
            ));
        }

        let handle = PlatformAdapterHandle::new(self.next_handle);
        self.next_handle = self.next_handle.checked_add(1).ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::InvalidInput,
                "platform adapter handle overflow",
            )
        })?;
        self.by_adapter_id
            .insert(registration.adapter_id.clone(), handle);
        self.by_handle.insert(handle, registration);
        Ok(handle)
    }
}

impl SubscriptionRegistry {
    fn new() -> Self {
        Self {
            next_handle: 1,
            by_handle: HashMap::new(),
        }
    }

    fn insert(
        &mut self,
        bus_subscription_id: u64,
        receiver: Receiver<CoreEvent>,
    ) -> SubscriptionHandle {
        let handle = SubscriptionHandle::new(self.next_handle);
        self.next_handle += 1;
        self.by_handle.insert(
            handle,
            SubscriptionRecord {
                bus_subscription_id,
                receiver,
            },
        );
        handle
    }

    fn remove(&mut self, handle: SubscriptionHandle) -> CoreResult<SubscriptionRecord> {
        self.by_handle.remove(&handle).ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("subscription handle {} is not registered", handle.get()),
            )
        })
    }

    fn clear(&mut self) {
        self.by_handle.clear();
    }

    fn drain(&self, handle: SubscriptionHandle, max_events: u32) -> CoreResult<Vec<CoreEvent>> {
        let record = self.by_handle.get(&handle).ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("subscription handle {} is not registered", handle.get()),
            )
        })?;
        let mut events = Vec::new();
        for _ in 0..max_events {
            match record.receiver.try_recv() {
                Ok(event) => events.push(event),
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
            }
        }
        Ok(events)
    }
}

fn validate_brain_registration(registration: &BrainImplementationRegistration) -> CoreResult<()> {
    if registration.implementation_id.0.trim().is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "brain implementation requires an implementation_id",
        ));
    }
    if registration.profile_id.0.trim().is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "brain implementation requires a profile_id",
        ));
    }
    if registration.model_config.provider.trim().is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "brain implementation requires a model provider",
        ));
    }
    if registration.model_config.model_name.trim().is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "brain implementation requires a model name",
        ));
    }
    if let Some(strategy) = &registration.strategy {
        if strategy.module_id.trim().is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "brain strategy metadata requires a module_id",
            ));
        }
        if strategy.strategy_id.trim().is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "brain strategy metadata requires a strategy_id",
            ));
        }
    }
    let mut tool_names = HashSet::new();
    for tool in &registration.tool_profile.tools {
        if tool.name.trim().is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "brain implementation tool name must be non-empty",
            ));
        }
        if tool.description.trim().is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!(
                    "brain implementation tool {} requires a description",
                    tool.name
                ),
            ));
        }
        if !tool_names.insert(tool.name.clone()) {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("brain implementation has duplicate tool {}", tool.name),
            ));
        }
    }
    Ok(())
}

fn not_implemented(operation: &str) -> CoreError {
    CoreError::new(
        CoreErrorKind::AdapterUnavailable,
        format!("native bridge operation {operation} is not implemented yet"),
    )
}

#[napi_derive::napi(object)]
pub struct JsEngineConfig {
    pub engine_data_dir: String,
    pub fixed_clock: Option<String>,
    pub default_turn_budget: u32,
    pub default_idle_timeout_ms: u32,
}

#[napi_derive::napi(object)]
pub struct JsToolDescriptor {
    pub name: String,
    pub description: String,
    pub input_schema: Option<u32>,
}

#[napi_derive::napi(object)]
pub struct JsToolProfile {
    pub tools: Vec<JsToolDescriptor>,
}

#[napi_derive::napi(object)]
pub struct JsResourceLimits {
    pub workdir: Option<String>,
    pub max_duration_ms: Option<u32>,
    pub max_delegation_depth: Option<u32>,
}

#[napi_derive::napi(object)]
pub struct JsBrainModelConfig {
    pub provider: String,
    pub model_name: String,
    pub temperature_milli: Option<u32>,
    pub max_output_tokens: Option<u32>,
}

#[napi_derive::napi(object)]
pub struct JsBrainProviderStateStrategyMetadata {
    pub mode: String,
}

#[napi_derive::napi(object)]
pub struct JsBrainStrategyMetadata {
    pub module_id: String,
    pub strategy_id: String,
    pub provider_state: JsBrainProviderStateStrategyMetadata,
}

#[napi_derive::napi(object)]
pub struct JsBrainProviderStateScope {
    pub profile_fingerprint: String,
    pub provider_fingerprint: String,
}

#[napi_derive::napi(object)]
pub struct JsProviderStateDiagnostic {
    pub session_id: String,
    pub module_id: String,
    pub strategy_id: String,
    pub status: String,
    pub payload_version: Option<String>,
    pub payload_bytes: Option<f64>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub expires_at: Option<String>,
    pub last_wake_id: Option<String>,
    pub invalidated_at: Option<String>,
    pub invalidation_reason: Option<String>,
}

#[napi_derive::napi(object)]
pub struct JsBrainImplementationRegistration {
    pub implementation_id: String,
    pub profile_id: String,
    pub tool_profile: JsToolProfile,
    pub model_config: JsBrainModelConfig,
    pub strategy: Option<JsBrainStrategyMetadata>,
    pub provider_state_scope: Option<JsBrainProviderStateScope>,
}

#[napi_derive::napi(object)]
pub struct JsEventSubscription {
    pub event_kinds: Vec<String>,
    pub session_id: Option<String>,
    pub agent_id: Option<String>,
    pub adapter_id: Option<String>,
}

#[napi_derive::napi(object)]
pub struct JsPlatformAdapterRegistration {
    pub adapter_id: String,
    pub kind: String,
    pub display_name: String,
}

#[napi_derive::napi(object)]
pub struct JsEventReceipt {
    pub accepted: bool,
    pub sequence: f64,
}

#[napi_derive::napi(object)]
pub struct JsShutdownSummary {
    pub archived_sessions: u32,
    pub dropped_subscriptions: u32,
}

#[napi_derive::napi(object)]
pub struct JsSessionConfig {
    pub session_id: String,
    pub agent_id: String,
    pub profile_id: String,
    pub kind: String,
    pub resource_limits: Option<JsResourceLimits>,
    pub tool_profile: Option<JsToolProfile>,
    pub history_window: Option<JsSessionHistoryWindow>,
}

#[napi_derive::napi(object)]
pub struct JsSessionState {
    pub handle: f64,
    pub session_id: String,
    pub agent_id: String,
    pub profile_id: String,
    pub kind: String,
    pub status: String,
    pub history_window: Option<JsSessionHistoryWindow>,
}

#[napi_derive::napi(object)]
pub struct JsSessionHistoryWindow {
    pub max_messages: Option<u32>,
}

#[napi_derive::napi(object)]
pub struct JsProfileMemoryCaps {
    pub max_records_per_profile: Option<u32>,
    pub max_key_bytes: Option<u32>,
    pub max_content_bytes: Option<u32>,
}

#[napi_derive::napi(object)]
pub struct JsProfileMemoryRecord {
    pub profile_id: String,
    pub target_type: String,
    pub target_id: String,
    pub key: String,
    pub content: String,
    pub metadata_json: String,
    pub revision: f64,
    pub created_at: String,
    pub updated_at: String,
}

#[napi_derive::napi(object)]
pub struct JsQueuedMessageRecord {
    pub message_id: String,
    pub owner_session_id: Option<String>,
    pub owner_agent_id: String,
    pub from_agent: String,
    pub to_agent: String,
    pub body: String,
    pub correlation_id: Option<String>,
    pub enqueued_at: String,
    pub expires_at: String,
    pub ttl_ms: u32,
    pub delivery_attempts: u32,
    pub state: String,
    pub terminal_at: Option<String>,
    pub state_reason: Option<String>,
}

#[napi_derive::napi(object)]
pub struct JsProfileMemoryQuery {
    pub profile_id: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[napi_derive::napi(object)]
pub struct JsProfileMemoryWrite {
    pub profile_id: String,
    pub target_type: String,
    pub target_id: Option<String>,
    pub key: String,
    pub content: String,
    pub metadata_json: Option<String>,
    pub caps: Option<JsProfileMemoryCaps>,
}

#[napi_derive::napi(object)]
pub struct JsProfileMemoryReplace {
    pub write: JsProfileMemoryWrite,
    pub expected_revision: f64,
}

#[napi_derive::napi(object)]
pub struct JsProfileMemoryDelete {
    pub profile_id: String,
    pub target_type: String,
    pub target_id: Option<String>,
    pub key: String,
    pub expected_revision: f64,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeSearchQuery {
    pub query: String,
    pub row_type: Option<String>,
    pub session_id: Option<String>,
    pub agent_id: Option<String>,
    pub instance_id: Option<String>,
    pub task_id: Option<String>,
    pub event_kind: Option<String>,
    pub recorded_after: Option<String>,
    pub recorded_before: Option<String>,
    pub limit: Option<u32>,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeSearchResult {
    pub row_type: String,
    pub row_key: String,
    pub sequence: Option<f64>,
    pub session_id: Option<String>,
    pub agent_id: Option<String>,
    pub instance_id: Option<String>,
    pub task_id: Option<String>,
    pub event_kind: Option<String>,
    pub recorded_at: String,
    pub title: String,
    pub body: String,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeCounterQuery {
    pub scope_type: Option<String>,
    pub scope_id: Option<String>,
    pub counter_name: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeCounterRecord {
    pub scope_type: String,
    pub scope_id: String,
    pub counter_name: String,
    pub value: f64,
    pub updated_at: String,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeCounterSummary {
    pub scope_type: String,
    pub scope_id: String,
    pub brain_turns: f64,
    pub wakes: f64,
    pub tool_calls: f64,
    pub tool_errors: f64,
    pub delegations_created: f64,
    pub delegations_completed: f64,
    pub delegations_failed: f64,
    pub delegations_timed_out: f64,
    pub delegations_cancelled: f64,
    pub messages: f64,
    pub completions: f64,
    pub queue_expirations: f64,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeDatabaseSize {
    pub database_bytes: f64,
    pub page_count: f64,
    pub page_size_bytes: f64,
    pub freelist_pages: f64,
    pub freelist_bytes: f64,
    pub wal_bytes: f64,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeMaintenancePolicy {
    pub expire_queued_messages_at: Option<String>,
    pub purge_terminal_queued_messages_before: Option<String>,
    pub expire_provider_wire_states_at: Option<String>,
    pub run_wal_checkpoint: Option<bool>,
    pub run_optimize: Option<bool>,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeMaintenanceReport {
    pub size_before: JsRuntimeDatabaseSize,
    pub size_after: JsRuntimeDatabaseSize,
    pub expired_queue_messages: f64,
    pub purged_terminal_queue_messages: f64,
    pub expired_provider_wire_states: f64,
    pub wal_checkpoint_ran: bool,
    pub optimize_ran: bool,
}

#[derive(Debug, Deserialize)]
struct WireDeleteMessageVariantRequest {
    slot_id: MessageSlotId,
    variant_id: MessageVariantId,
    updated_at: rusty_crew_core_bridge_api::IsoTimestamp,
}

#[derive(Debug, Deserialize)]
struct WireReorderMessageVariantsRequest {
    slot_id: MessageSlotId,
    ordered_variant_ids: Vec<MessageVariantId>,
    updated_at: rusty_crew_core_bridge_api::IsoTimestamp,
}

#[derive(Debug, Deserialize)]
struct WireGetConversationBranchStateRequest {
    session_id: SessionId,
    default_updated_at: rusty_crew_core_bridge_api::IsoTimestamp,
}

#[derive(Debug, Deserialize)]
struct WireRemoveAttachmentRequest {
    attachment_id: AttachmentId,
    updated_at: rusty_crew_core_bridge_api::IsoTimestamp,
}

#[derive(Debug, Deserialize)]
struct WireRemoveDataBankScopeRequest {
    scope_id: DataBankScopeId,
    updated_at: rusty_crew_core_bridge_api::IsoTimestamp,
}

#[napi_derive::napi(object)]
pub struct JsActionBatchReceipt {
    pub wake_id: String,
    pub accepted_actions: u32,
    pub rejected_actions_json: String,
}

#[napi_derive::napi(object)]
pub struct JsBufferedBrainWakeRequest {
    pub body_state: u32,
    pub system_prompt: u32,
    pub role_assembly: u32,
    pub provider_state_json: Option<String>,
    pub provider_state_absence: Option<String>,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeBufferView {
    pub handle: u32,
    pub media_type: String,
    pub byte_len: f64,
    pub bytes: napi::bindgen_prelude::Buffer,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsOpenAiResponsesBrainRunInput {
    wake_id: String,
    session_id: String,
    body_state: BodyState,
    #[serde(default)]
    provider_state: Option<BrainWakeProviderStateInput>,
    #[serde(default)]
    provider_state_absence: Option<String>,
    config: JsOpenAiResponsesBrainConfig,
    #[serde(default)]
    client: JsOpenAiResponsesClientConfig,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsOpenAiResponsesBrainConfig {
    model: String,
    #[serde(default)]
    instructions: Option<String>,
    #[serde(default = "default_responses_stream_idle_timeout_ms")]
    stream_idle_timeout_ms: u64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
enum JsOpenAiResponsesClientConfig {
    #[default]
    Fake,
    Live {
        base_url: String,
        #[serde(default)]
        api_key: Option<String>,
    },
}

fn default_responses_stream_idle_timeout_ms() -> u64 {
    30_000
}

struct EchoNeutralToolExecutor;

impl NeutralToolExecutor for EchoNeutralToolExecutor {
    fn execute(&self, call: &PendingResponsesFunctionCall) -> NeutralToolOutput {
        NeutralToolOutput {
            output: format!("{} completed by Rust Responses bridge", call.name),
            is_error: false,
        }
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
        let handle = bridge
            .initialize_engine(EngineConfig {
                engine_data_dir: config.engine_data_dir,
                clock: match config.fixed_clock {
                    Some(at) => rusty_crew_core_bridge_api::ClockConfig::Fixed { at },
                    None => rusty_crew_core_bridge_api::ClockConfig::System,
                },
                default_turn_budget: config.default_turn_budget,
                default_idle_timeout_ms: config.default_idle_timeout_ms,
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
    pub fn run_openai_responses_brain_json(&self, input_json: String) -> napi::Result<String> {
        let input: JsOpenAiResponsesBrainRunInput =
            serde_json::from_str(&input_json).map_err(|error| {
                napi::Error::new(
                    napi::Status::InvalidArg,
                    format!("invalid OpenAI Responses brain input JSON: {error}"),
                )
            })?;
        let mut config = ResponsesBrainConfig::replay(input.config.model);
        config.instructions = input.config.instructions;
        config.stream_idle_timeout_ms = input.config.stream_idle_timeout_ms;
        let descriptors = input
            .body_state
            .session
            .tool_profile
            .tools
            .iter()
            .map(|tool| NeutralBrainTool {
                name: tool.name.clone(),
                description: tool.description.clone(),
                input_schema: json!({"type": "object", "properties": {}}),
            })
            .collect::<Vec<_>>();
        let history = rusty_crew_openai_responses_brain::ResponsesReplayProjection::from_body_state(
            &input.body_state,
        );
        let request = BrainWakeRequest {
            brain: BrainImplementationHandle::new(0),
            session_id: SessionId::new(input.session_id),
            body_state: RuntimeBufferHandle::new(0),
            system_prompt: RuntimeBufferHandle::new(0),
            role_assembly: RuntimeBufferHandle::new(0),
            wake_id: input.wake_id,
            provider_state: input.provider_state,
            provider_state_absence: input
                .provider_state_absence
                .as_deref()
                .map(parse_provider_state_absence_reason)
                .transpose()
                .map_err(to_napi_error)?,
        };
        let result = match input.client {
            JsOpenAiResponsesClientConfig::Fake => {
                let client = fake_responses_client_for_body(&input.body_state);
                let mut brain =
                    ResponsesReplayBrain::new(client, EchoNeutralToolExecutor, config, descriptors);
                brain.wake_with_history(request, history)
            }
            JsOpenAiResponsesClientConfig::Live { base_url, api_key } => {
                let client =
                    LiveResponsesClient::new(base_url, api_key, config.stream_idle_timeout_ms)
                        .map_err(|error| {
                            napi::Error::new(napi::Status::GenericFailure, error.to_string())
                        })?;
                let mut brain =
                    ResponsesReplayBrain::new(client, EchoNeutralToolExecutor, config, descriptors);
                brain.wake_with_history(request, history)
            }
        }
        .map_err(to_napi_error)?;
        let output = json!({
            "stream": result.stream.drain_until_terminal().map_err(to_napi_error)?,
            "provider_state": result.provider_state,
        });
        serde_json::to_string(&output)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
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

fn scheduled_job_json(record: ScheduledJobRecord) -> serde_json::Value {
    serde_json::json!({
        "job_id": record.job_id,
        "job_kind": record.job_kind,
        "target_session_id": record.target_session_id.map(|session_id| session_id.0),
        "interval_ms": record.interval_ms,
        "next_due_at": record.next_due_at,
        "status": scheduled_job_status_as_str(record.status),
        "created_at": record.created_at,
        "updated_at": record.updated_at,
        "paused_at": record.paused_at,
    })
}

fn scheduled_run_json(record: ScheduledRunRecord) -> serde_json::Value {
    serde_json::json!({
        "run_id": record.run_id.0,
        "job_id": record.job_id,
        "job_kind": record.job_kind,
        "target_session_id": record.target_session_id.map(|session_id| session_id.0),
        "status": scheduled_run_status_as_str(record.status),
        "trigger": scheduled_run_trigger_as_str(record.trigger),
        "scheduled_for": record.scheduled_for,
        "claimed_at": record.claimed_at,
        "claim_deadline_at": record.claim_deadline_at,
        "completed_at": record.completed_at,
        "error": record.error,
        "output": record.output_json,
        "created_at": record.created_at,
        "updated_at": record.updated_at,
    })
}

fn scheduled_job_status_as_str(status: ScheduledJobStatus) -> &'static str {
    match status {
        ScheduledJobStatus::Active => "active",
        ScheduledJobStatus::Paused => "paused",
        ScheduledJobStatus::Archived => "archived",
    }
}

fn scheduled_job_status_from_str(raw: &str) -> CoreResult<ScheduledJobStatus> {
    match raw {
        "active" => Ok(ScheduledJobStatus::Active),
        "paused" => Ok(ScheduledJobStatus::Paused),
        "archived" => Ok(ScheduledJobStatus::Archived),
        other => Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("unknown scheduled job status {other}"),
        )),
    }
}

fn scheduled_run_status_as_str(status: ScheduledRunStatus) -> &'static str {
    match status {
        ScheduledRunStatus::Claimed => "claimed",
        ScheduledRunStatus::Completed => "completed",
        ScheduledRunStatus::Skipped => "skipped",
        ScheduledRunStatus::Failed => "failed",
        ScheduledRunStatus::Expired => "expired",
        ScheduledRunStatus::Cancelled => "cancelled",
    }
}

fn scheduled_run_status_from_str(raw: &str) -> CoreResult<ScheduledRunStatus> {
    match raw {
        "claimed" => Ok(ScheduledRunStatus::Claimed),
        "completed" => Ok(ScheduledRunStatus::Completed),
        "skipped" => Ok(ScheduledRunStatus::Skipped),
        "failed" => Ok(ScheduledRunStatus::Failed),
        "expired" => Ok(ScheduledRunStatus::Expired),
        "cancelled" => Ok(ScheduledRunStatus::Cancelled),
        other => Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("unknown scheduled run status {other}"),
        )),
    }
}

fn scheduled_run_trigger_as_str(trigger: ScheduledRunTrigger) -> &'static str {
    match trigger {
        ScheduledRunTrigger::Due => "due",
        ScheduledRunTrigger::Manual => "manual",
    }
}

fn scheduled_run_trigger_from_str(raw: &str) -> CoreResult<ScheduledRunTrigger> {
    match raw {
        "due" => Ok(ScheduledRunTrigger::Due),
        "manual" => Ok(ScheduledRunTrigger::Manual),
        other => Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("unknown scheduled run trigger {other}"),
        )),
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

fn to_js_runtime_maintenance_report(
    report: RuntimeMaintenanceReport,
) -> JsRuntimeMaintenanceReport {
    JsRuntimeMaintenanceReport {
        size_before: to_js_runtime_database_size(report.size_before),
        size_after: to_js_runtime_database_size(report.size_after),
        expired_queue_messages: report.expired_queue_messages as f64,
        purged_terminal_queue_messages: report.purged_terminal_queue_messages as f64,
        expired_provider_wire_states: report.expired_provider_wire_states as f64,
        wal_checkpoint_ran: report.wal_checkpoint_ran,
        optimize_ran: report.optimize_ran,
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

fn fake_responses_client_for_body(body: &BodyState) -> FakeResponsesClient {
    let Some(tool) = body.session.tool_profile.tools.first() else {
        return FakeResponsesClient::new(vec![Ok(vec![
            ResponsesEvent::TextDelta("responses module scaffold wake completed".to_string()),
            ResponsesEvent::Completed {
                response_id: "fake-response".to_string(),
                usage: Some(fake_responses_usage(false)),
            },
        ])]);
    };
    FakeResponsesClient::new(vec![
        Ok(vec![
            ResponsesEvent::OutputItemDone(ResponsesOutputItem::FunctionCall {
                id: Some("fake-call-item".to_string()),
                call_id: "fake-call".to_string(),
                name: tool.name.clone(),
                arguments: "{}".to_string(),
            }),
            ResponsesEvent::Completed {
                response_id: "fake-response-tool-call".to_string(),
                usage: Some(fake_responses_usage(false)),
            },
        ]),
        Ok(vec![
            ResponsesEvent::TextDelta("responses module scaffold wake completed".to_string()),
            ResponsesEvent::Completed {
                response_id: "fake-response-final".to_string(),
                usage: Some(fake_responses_usage(true)),
            },
        ]),
    ])
    .expect_function_output("fake-call")
}

fn fake_responses_usage(cached: bool) -> ResponsesTokenUsage {
    ResponsesTokenUsage {
        input_tokens: 1,
        cached_input_tokens: u64::from(cached),
        output_tokens: 1,
        reasoning_output_tokens: 0,
        total_tokens: 2,
    }
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

fn handle_to_u32(handle: RuntimeBufferHandle) -> napi::Result<u32> {
    u32::try_from(handle.get()).map_err(|_| {
        napi::Error::new(
            napi::Status::InvalidArg,
            format!("runtime buffer handle {} does not fit in u32", handle.get()),
        )
    })
}

fn to_napi_error(error: CoreError) -> napi::Error {
    napi::Error::new(
        napi::Status::GenericFailure,
        format!("{:?}: {}", error.kind, error.message),
    )
}

fn parse_json<T: DeserializeOwned>(raw: &str, label: &str) -> napi::Result<T> {
    serde_json::from_str(raw).map_err(|error| {
        napi::Error::new(
            napi::Status::InvalidArg,
            format!("invalid {label} json: {error}"),
        )
    })
}

fn serialize_json<T: Serialize>(value: &T, label: &str) -> napi::Result<String> {
    serde_json::to_string(value).map_err(|error| {
        napi::Error::new(
            napi::Status::GenericFailure,
            format!("serialize {label}: {error}"),
        )
    })
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

    #[test]
    fn native_bridge_exposes_the_current_manifest_surface() {
        let bridge = NativeBridge::new();

        assert_eq!(bridge.manifest_version(), MANIFEST_VERSION);
        assert_eq!(bridge.operation_names(), OPERATION_NAMES);
        assert!(bridge.operation_names().contains(&"get_buffer"));
        assert!(bridge.operation_names().contains(&"release_buffer"));
        assert_eq!(
            bridge.manifest_summary().native_package,
            "@rusty-crew/native-bridge"
        );
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
}
