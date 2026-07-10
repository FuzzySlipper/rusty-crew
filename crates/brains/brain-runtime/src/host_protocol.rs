use crate::{BufferedNeutralCancellation, BufferedNeutralPendingToolRequest};
use rusty_crew_core_protocol::{BrainWakeProviderStateOutput, BrainWakeStreamItem};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BufferedBrainRunStartReceipt {
    pub module_id: String,
    pub wake_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BufferedBrainRunDrain {
    pub module_id: String,
    pub wake_id: String,
    #[serde(default)]
    pub items: Vec<BrainWakeStreamItem>,
    #[serde(default)]
    pub tool_requests: Vec<BufferedNeutralPendingToolRequest>,
    pub terminal: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_state: Option<BrainWakeProviderStateOutput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport_metrics: Option<JsonValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_secret_update: Option<JsonValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancellation: Option<BufferedNeutralCancellation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BufferedBrainHostResultReceipt {
    pub module_id: String,
    pub wake_id: String,
    pub call_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BufferedBrainRunCancellationReceipt {
    pub module_id: String,
    pub wake_id: String,
    pub cancelled: bool,
    pub terminal: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancellation: Option<BufferedNeutralCancellation>,
}
