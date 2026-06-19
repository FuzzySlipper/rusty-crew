//! In-process bridge implementation for tests and early TS integration spikes.

use rusty_crew_core_bridge_api::{
    CoreResult, DenDataUpdate, EngineConfig, EventReceipt, ExternalEvent, SessionConfig, SessionId,
    SessionState, ShutdownSummary,
};
use rusty_crew_core_engine::CoreEngine;

#[derive(Debug)]
pub struct MockBridge {
    engine: CoreEngine,
}

impl MockBridge {
    pub fn initialize(config: EngineConfig) -> CoreResult<Self> {
        Ok(Self {
            engine: CoreEngine::initialize(config)?,
        })
    }

    pub fn create_session(&self, config: SessionConfig) -> CoreResult<SessionState> {
        self.engine.create_session(config)
    }

    pub fn get_session(&self, session_id: &SessionId) -> CoreResult<SessionState> {
        self.engine.get_session(session_id)
    }

    pub fn inject_external_event(&self, event: ExternalEvent) -> CoreResult<EventReceipt> {
        self.engine.inject_external_event(event)
    }

    pub fn inject_den_data_update(&self, update: DenDataUpdate) -> CoreResult<EventReceipt> {
        self.engine.inject_den_data_update(update)
    }

    pub fn shutdown(self) -> CoreResult<ShutdownSummary> {
        self.engine.shutdown()
    }
}
