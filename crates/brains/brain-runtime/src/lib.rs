mod catalog;
mod context_accounting;
mod context_compaction;
mod coordinator;
mod host_protocol;
mod tool_policy;

pub use catalog::*;
pub use context_accounting::*;
pub use context_compaction::*;
pub use coordinator::*;
pub use host_protocol::*;
pub use tool_policy::*;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;

pub type BrainRuntimeResult<T> = Result<T, BrainRuntimeError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrainRuntimeError {
    DuplicateWake {
        module_label: &'static str,
        wake_id: String,
    },
    WakeNotFound {
        module_label: &'static str,
        wake_id: String,
    },
    RegistryPoisoned {
        module_label: &'static str,
    },
}

impl BrainRuntimeError {
    pub fn is_invalid_argument(&self) -> bool {
        matches!(
            self,
            BrainRuntimeError::DuplicateWake { .. } | BrainRuntimeError::WakeNotFound { .. }
        )
    }
}

impl fmt::Display for BrainRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BrainRuntimeError::DuplicateWake {
                module_label,
                wake_id,
            } => write!(
                formatter,
                "{module_label} buffered wake {wake_id} already exists"
            ),
            BrainRuntimeError::WakeNotFound {
                module_label,
                wake_id,
            } => write!(
                formatter,
                "{module_label} buffered wake {wake_id} was not found"
            ),
            BrainRuntimeError::RegistryPoisoned { module_label } => {
                write!(
                    formatter,
                    "{module_label} buffered run registry is poisoned"
                )
            }
        }
    }
}

impl std::error::Error for BrainRuntimeError {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct BufferedNeutralPendingToolRequest {
    pub call_id: String,
    pub provider_item_id: Option<String>,
    pub name: String,
    pub arguments_json: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, JsonSchema)]
pub struct BufferedNeutralToolOutput {
    pub output: String,
    pub is_error: bool,
    #[serde(default)]
    pub state_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(tag = "status", content = "output", rename_all = "snake_case")]
pub enum BufferedNeutralToolOutputPoll {
    Pending,
    Ready(BufferedNeutralToolOutput),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct BufferedNeutralCancellation {
    pub reason_code: String,
    pub summary: String,
    pub cancelled_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct BufferedBrainStreamRetentionMetrics {
    pub raw_stream_item_count: usize,
    pub raw_delta_item_count: usize,
    pub retained_stream_item_count: usize,
    pub coalesced_delta_item_count: usize,
    pub dropped_stream_item_count: usize,
    pub retained_delta_bytes: usize,
    pub queued_delta_bytes: usize,
    pub max_stream_items: usize,
    pub max_stream_delta_bytes: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, JsonSchema)]
pub struct BufferedBrainTurnDiagnostic {
    pub module_label: String,
    pub wake_id: String,
    pub session_id: String,
    pub agent_id: Option<String>,
    pub profile_id: Option<String>,
    pub phase: String,
    pub queued_stream_item_count: usize,
    pub stream_retention_metrics: BufferedBrainStreamRetentionMetrics,
    pub pending_tool_request_count: usize,
    pub submitted_tool_output_count: usize,
    pub age_ms: u64,
    pub terminal: bool,
    pub cancelled: bool,
    pub has_error: bool,
    pub started_at: String,
    pub last_transition_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, JsonSchema)]
pub struct BufferedBrainTurnCleanupReport {
    pub module_label: String,
    pub active_runs: usize,
    pub terminal_runs: usize,
    pub cancelled_nonterminal_runs: usize,
    pub removed_runs: usize,
}
