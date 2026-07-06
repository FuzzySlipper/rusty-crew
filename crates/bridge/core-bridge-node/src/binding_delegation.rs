use super::*;

#[napi_derive::napi]
impl NativeBridgeBinding {
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
}
