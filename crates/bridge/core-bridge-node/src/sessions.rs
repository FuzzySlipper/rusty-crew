use super::*;

impl NativeBridge {
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
}
