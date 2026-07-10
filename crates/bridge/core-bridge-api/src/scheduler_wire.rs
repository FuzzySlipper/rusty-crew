use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScheduledJobWireOutput {
    pub job_id: String,
    pub job_kind: String,
    pub target_session_id: Option<String>,
    pub interval_ms: Option<u64>,
    pub next_due_at: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub paused_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScheduledRunWireOutput {
    pub run_id: String,
    pub job_id: String,
    pub job_kind: String,
    pub target_session_id: Option<String>,
    pub status: String,
    pub trigger: String,
    pub scheduled_for: Option<String>,
    pub claimed_at: String,
    pub claim_deadline_at: String,
    pub completed_at: Option<String>,
    pub error: Option<String>,
    pub output: Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SchedulerTickWireOutput {
    pub stale_runs_expired: u32,
    pub due_runs_claimed: u32,
    pub wakes_requested: u32,
    pub runs_completed: u32,
    pub runs_skipped: u32,
    pub runs_failed: u32,
}
