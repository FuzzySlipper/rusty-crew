use super::*;

impl NativeBridge {
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
}
