use super::*;

#[napi_derive::napi]
impl NativeBridgeBinding {
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
}
