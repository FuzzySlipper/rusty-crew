//! Coordination engine composition.

use rusty_crew_core_bus::CoreBus;
use rusty_crew_core_protocol::{
    ClockConfig, CoreEvent, CoreResult, EngineConfig, EngineHandle, IsoTimestamp, SessionConfig,
    SessionId, SessionState,
};
use rusty_crew_core_session::SessionRegistry;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ENGINE_HANDLE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone)]
pub struct CoreEngine {
    handle: EngineHandle,
    config: EngineConfig,
    bus: CoreBus,
    sessions: SessionRegistry,
}

impl CoreEngine {
    pub fn initialize(config: EngineConfig) -> CoreResult<Self> {
        Ok(Self {
            handle: EngineHandle::new(NEXT_ENGINE_HANDLE.fetch_add(1, Ordering::Relaxed)),
            config,
            bus: CoreBus::new(),
            sessions: SessionRegistry::new(),
        })
    }

    pub fn handle(&self) -> EngineHandle {
        self.handle
    }

    pub fn bus(&self) -> &CoreBus {
        &self.bus
    }

    pub fn create_session(&self, config: SessionConfig) -> CoreResult<SessionState> {
        let state = self.sessions.create_session(config, self.now())?;
        self.bus.publish(CoreEvent::SessionCreated {
            state: state.clone(),
        })?;
        Ok(state)
    }

    pub fn get_session(&self, session_id: &SessionId) -> CoreResult<SessionState> {
        self.sessions.get_session(session_id)
    }

    pub fn archive_session(&self, session_id: &SessionId) -> CoreResult<SessionState> {
        let state = self.sessions.archive_session(session_id, self.now())?;
        self.bus.publish(CoreEvent::SessionArchived {
            session_id: session_id.clone(),
        })?;
        Ok(state)
    }

    pub fn shutdown(self) -> CoreResult<ShutdownSummary> {
        Ok(ShutdownSummary {
            engine: self.handle,
            archived_sessions: 0,
        })
    }

    fn now(&self) -> IsoTimestamp {
        match &self.config.clock {
            ClockConfig::System => "system-clock-placeholder".to_string(),
            ClockConfig::Fixed { at } => at.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShutdownSummary {
    pub engine: EngineHandle,
    pub archived_sessions: u32,
}
