//! Native Node transport boundary.
//!
//! napi-rs glue belongs in this crate. The transport-neutral pieces here expose
//! the current manifest surface and own runtime buffers without leaking native
//! transport dependencies into core crates.

use rusty_crew_core_bridge_api::{
    manifest_summary, ActionBatchReceipt, BrainActionBatch, BrainEventEnvelope,
    BrainImplementationHandle, BrainImplementationRegistration, BrainWakeAccepted,
    BrainWakeBufferInput, BrainWakeRequest, BridgeManifestSummary, CoreError, CoreErrorKind,
    CoreEvent, CoreResult, DenDataUpdate, EngineConfig, EngineHandle, EventReceipt,
    EventSubscription, ExternalEvent, PlatformAdapterHandle, PlatformAdapterRegistration,
    RuntimeBufferHandle, RuntimeBufferStore, RuntimeBufferView, SessionId, ShutdownRequest,
    ShutdownSummary, SubscriptionHandle, Unit, MANIFEST_VERSION, OPERATION_NAMES,
};
use rusty_crew_core_engine::CoreEngine;
use rusty_crew_core_persistence::{
    ProfileMemoryCaps, ProfileMemoryDelete, ProfileMemoryQuery, ProfileMemoryRecord,
    ProfileMemoryReplace, ProfileMemoryTarget, ProfileMemoryWrite, QueuedMessageRecord,
    RuntimeCounterQuery, RuntimeCounterRecord, RuntimeCounterScope, RuntimeDatabaseSize,
    RuntimeMaintenancePolicy, RuntimeMaintenanceReport, RuntimeSearchFilter, RuntimeSearchResult,
    RuntimeSearchRowType, RuntimeStateSummary, ScheduledJobRecord, ScheduledJobStatus,
    ScheduledRunRecord, ScheduledRunStatus, ScheduledRunTrigger,
};
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
        self.buffers.build_brain_wake_request(input)
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
pub struct JsBrainImplementationRegistration {
    pub implementation_id: String,
    pub profile_id: String,
    pub tool_profile: JsToolProfile,
    pub model_config: JsBrainModelConfig,
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
}

#[napi_derive::napi(object)]
pub struct JsSessionState {
    pub handle: f64,
    pub session_id: String,
    pub agent_id: String,
    pub profile_id: String,
    pub kind: String,
    pub status: String,
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
    pub run_wal_checkpoint: Option<bool>,
    pub run_optimize: Option<bool>,
}

#[napi_derive::napi(object)]
pub struct JsRuntimeMaintenanceReport {
    pub size_before: JsRuntimeDatabaseSize,
    pub size_after: JsRuntimeDatabaseSize,
    pub expired_queue_messages: f64,
    pub purged_terminal_queue_messages: f64,
    pub wal_checkpoint_ran: bool,
    pub optimize_ran: bool,
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
}

#[napi_derive::napi(object)]
pub struct JsRuntimeBufferView {
    pub handle: u32,
    pub media_type: String,
    pub byte_len: f64,
    pub bytes: napi::bindgen_prelude::Buffer,
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
            .register_brain_implementation(to_brain_registration(registration))
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
        })
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
                run_wal_checkpoint: policy.run_wal_checkpoint.unwrap_or(false),
                run_optimize: policy.run_optimize.unwrap_or(false),
            })
            .map_err(to_napi_error)?;
        Ok(to_js_runtime_maintenance_report(report))
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
        let metadata = metadata_json
            .as_deref()
            .map(serde_json::from_str::<rusty_crew_core_bridge_api::ToolCallMetadata>)
            .transpose()
            .map_err(|error| napi::Error::new(napi::Status::InvalidArg, error.to_string()))?;
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
                metadata: metadata.clone(),
            },
            "tool_call_finished" => rusty_crew_core_bridge_api::BrainEvent::ToolCallFinished {
                tool_name: tool_name.ok_or_else(|| {
                    napi::Error::new(
                        napi::Status::InvalidArg,
                        "tool_call_finished requires toolName".to_string(),
                    )
                })?,
                is_error: is_error.unwrap_or(false),
                metadata,
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

fn to_js_session_state(state: rusty_crew_core_bridge_api::SessionState) -> JsSessionState {
    JsSessionState {
        handle: state.handle.get() as f64,
        session_id: state.session_id.0,
        agent_id: state.agent_id.0,
        profile_id: state.profile_id.0,
        kind: format!("{:?}", state.kind).to_ascii_lowercase(),
        status: format!("{:?}", state.status).to_ascii_lowercase(),
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

fn scheduled_run_trigger_as_str(trigger: ScheduledRunTrigger) -> &'static str {
    match trigger {
        ScheduledRunTrigger::Due => "due",
        ScheduledRunTrigger::Manual => "manual",
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
    })
}

fn to_brain_registration(
    registration: JsBrainImplementationRegistration,
) -> BrainImplementationRegistration {
    BrainImplementationRegistration {
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
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_crew_core_bridge_api::{
        AgentId, BrainAction, BrainActionBatch, BrainImplementationHandle, BrainImplementationId,
        BrainModelConfig, CoreEventKind, EventSubscription, ProfileId, ResourceLimits,
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
        }
    }
}
