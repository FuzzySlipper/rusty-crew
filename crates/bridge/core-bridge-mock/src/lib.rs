//! In-process bridge implementation for tests and early TS integration spikes.

use rusty_crew_core_bridge_api::{
    CoreResult, EngineConfig, SessionConfig, SessionId, SessionState, ShutdownSummary,
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

    pub fn shutdown(self) -> CoreResult<ShutdownSummary> {
        self.engine.shutdown()
    }
}
