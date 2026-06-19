//! Native Node transport boundary.
//!
//! napi-rs glue belongs in this crate. The transport-neutral pieces here expose
//! the current manifest surface and own runtime buffers without leaking native
//! transport dependencies into core crates.

use rusty_crew_core_bridge_api::{
    manifest_summary, ActionBatchReceipt, BrainActionBatch, BrainEventEnvelope,
    BrainImplementationRegistration, BrainWakeAccepted, BrainWakeBufferInput, BrainWakeRequest,
    BridgeManifestSummary, CoreError, CoreErrorKind, CoreResult, DenDataUpdate, EngineConfig,
    EngineHandle, EventReceipt, EventSubscription, ExternalEvent, PlatformAdapterHandle,
    PlatformAdapterRegistration, RuntimeBufferHandle, RuntimeBufferStore, RuntimeBufferView,
    ShutdownRequest, ShutdownSummary, SubscriptionHandle, Unit, MANIFEST_VERSION, OPERATION_NAMES,
};
use rusty_crew_core_engine::CoreEngine;

#[derive(Debug)]
pub struct NativeBridge {
    engine: Option<CoreEngine>,
    buffers: RuntimeBufferStore,
}

impl NativeBridge {
    pub fn new() -> Self {
        Self {
            engine: None,
            buffers: RuntimeBufferStore::new(),
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
        let handle = engine.handle();
        self.engine = Some(engine);
        Ok(handle)
    }

    pub fn shutdown_engine(&mut self, _request: ShutdownRequest) -> CoreResult<ShutdownSummary> {
        let engine = self.engine.take().ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                "native bridge engine is not initialized",
            )
        })?;
        engine.shutdown()
    }

    pub fn register_brain_implementation(
        &self,
        _registration: BrainImplementationRegistration,
    ) -> CoreResult<rusty_crew_core_bridge_api::BrainImplementationHandle> {
        Err(not_implemented("register_brain_implementation"))
    }

    pub fn wake_brain(&self, request: BrainWakeRequest) -> CoreResult<BrainWakeAccepted> {
        self.get_buffer(request.body_state)?;
        self.get_buffer(request.system_prompt)?;
        self.get_buffer(request.role_assembly)?;
        Err(not_implemented("wake_brain"))
    }

    pub fn submit_brain_event(&self, event: BrainEventEnvelope) -> CoreResult<EventReceipt> {
        self.engine()?.submit_brain_event(event)
    }

    pub fn submit_brain_actions(&self, batch: BrainActionBatch) -> CoreResult<ActionBatchReceipt> {
        self.engine()?.execute_brain_actions(batch)
    }

    pub fn register_platform_adapter(
        &self,
        _registration: PlatformAdapterRegistration,
    ) -> CoreResult<PlatformAdapterHandle> {
        Err(not_implemented("register_platform_adapter"))
    }

    pub fn inject_external_event(&self, event: ExternalEvent) -> CoreResult<EventReceipt> {
        self.engine()?.inject_external_event(event)
    }

    pub fn inject_den_data_update(&self, update: DenDataUpdate) -> CoreResult<EventReceipt> {
        self.engine()?.inject_den_data_update(update)
    }

    pub fn subscribe_events(
        &self,
        _subscription: EventSubscription,
    ) -> CoreResult<SubscriptionHandle> {
        Err(not_implemented("subscribe_events"))
    }

    pub fn unsubscribe_events(&self, _handle: SubscriptionHandle) -> CoreResult<Unit> {
        Err(not_implemented("unsubscribe_events"))
    }

    pub fn build_brain_wake_request(
        &self,
        input: BrainWakeBufferInput,
    ) -> CoreResult<rusty_crew_core_bridge_api::BufferedBrainWakeRequest> {
        self.buffers.build_brain_wake_request(input)
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

fn not_implemented(operation: &str) -> CoreError {
    CoreError::new(
        CoreErrorKind::AdapterUnavailable,
        format!("native bridge operation {operation} is not implemented yet"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_crew_core_bridge_api::{BrainImplementationHandle, SessionId};

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
}
