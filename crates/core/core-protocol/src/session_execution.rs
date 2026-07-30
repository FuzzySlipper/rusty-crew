//! Canonical per-session execution projection.
//!
//! Durable session lifecycle answers whether a session may be used. Execution
//! phase answers what its current native brain work is doing. Rust derives this
//! projection from logical turns and runtime activities; adapters must not
//! infer it from process-local wake bookkeeping.

use crate::{IsoTimestamp, LogicalTurnId, SessionId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionLifecycleStatus {
    Live,
    Archived,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionExecutionPhase {
    Idle,
    Queued,
    Active,
    Waiting,
    Paused,
    Cancelling,
}

impl SessionExecutionPhase {
    pub fn is_working(self) -> bool {
        self != Self::Idle
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionExecutionOutcome {
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionExecutionSource {
    SessionLifecycle,
    LogicalTurn,
    RuntimeActivity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionExecutionState {
    pub session_id: SessionId,
    pub lifecycle_status: SessionLifecycleStatus,
    pub phase: SessionExecutionPhase,
    pub source: SessionExecutionSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wake_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logical_turn_id: Option<LogicalTurnId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_outcome: Option<SessionExecutionOutcome>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<IsoTimestamp>,
    pub updated_at: IsoTimestamp,
}
