//! Runtime activity accounting contracts.
//!
//! These records deliberately carry bounded operational metadata rather than
//! prompts, tool inputs, provider payloads, credentials, or command lines.

use crate::{AgentId, IsoTimestamp, ProfileId, SessionId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, PartialOrd, Ord,
)]
#[serde(transparent)]
pub struct RuntimeActivityId(pub String);

impl RuntimeActivityId {
    pub fn new(raw: impl Into<String>) -> Self {
        Self(raw.into())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeActivityKind {
    Dispatch,
    Wake,
    ProviderRequest,
    ToolCall,
    Subprocess,
    Browser,
    ExternalTurn,
}

impl RuntimeActivityKind {
    pub fn is_root(self) -> bool {
        matches!(self, Self::Dispatch | Self::Wake | Self::ExternalTurn)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeActivityOwner {
    RustCoordination,
    RustBrain,
    TypeScriptHost,
    ExternalRuntime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeActivityStatus {
    Active,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}

impl RuntimeActivityStatus {
    pub fn is_terminal(self) -> bool {
        self != Self::Active
    }

    pub fn is_abnormal(self) -> bool {
        matches!(self, Self::Failed | Self::Cancelled | Self::Interrupted)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeActivityRecord {
    pub activity_id: RuntimeActivityId,
    pub service_instance_id: String,
    pub parent_activity_id: Option<RuntimeActivityId>,
    pub kind: RuntimeActivityKind,
    pub owner: RuntimeActivityOwner,
    pub status: RuntimeActivityStatus,
    pub agent_id: Option<AgentId>,
    pub profile_id: Option<ProfileId>,
    pub session_id: Option<SessionId>,
    pub wake_id: Option<String>,
    pub phase: String,
    pub summary: Option<String>,
    pub provider_alias: Option<String>,
    pub model: Option<String>,
    pub tool_name: Option<String>,
    pub process_id: Option<u32>,
    pub debug_detail_id: Option<String>,
    pub reason_code: Option<String>,
    pub started_at: IsoTimestamp,
    pub last_progress_at: IsoTimestamp,
    pub terminal_at: Option<IsoTimestamp>,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeActivityBegin {
    pub activity_id: RuntimeActivityId,
    pub parent_activity_id: Option<RuntimeActivityId>,
    pub kind: RuntimeActivityKind,
    pub owner: RuntimeActivityOwner,
    pub agent_id: Option<AgentId>,
    pub profile_id: Option<ProfileId>,
    pub session_id: Option<SessionId>,
    pub wake_id: Option<String>,
    pub phase: String,
    pub summary: Option<String>,
    pub provider_alias: Option<String>,
    pub model: Option<String>,
    pub tool_name: Option<String>,
    pub process_id: Option<u32>,
    pub debug_detail_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeActivityProgress {
    pub activity_id: RuntimeActivityId,
    pub phase: String,
    pub summary: Option<String>,
    pub process_id: Option<u32>,
    pub debug_detail_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeActivityFinish {
    pub activity_id: RuntimeActivityId,
    pub status: RuntimeActivityStatus,
    pub phase: String,
    pub reason_code: Option<String>,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeActivityWakeSettlement {
    pub wake_id: String,
    pub status: RuntimeActivityStatus,
    pub reason_code: Option<String>,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeActivityLiveEvidence {
    pub activity_id: RuntimeActivityId,
    pub parent_activity_id: Option<RuntimeActivityId>,
    pub kind: RuntimeActivityKind,
    pub owner: RuntimeActivityOwner,
    pub agent_id: Option<AgentId>,
    pub profile_id: Option<ProfileId>,
    pub session_id: Option<SessionId>,
    pub wake_id: Option<String>,
    pub phase: String,
    pub summary: Option<String>,
    pub process_id: Option<u32>,
    pub started_at: IsoTimestamp,
    pub last_progress_at: IsoTimestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeActivityFindingCode {
    SessionProjectionMismatch,
    UntrackedNativeRun,
    DetachedDispatch,
    OrphanToolExecution,
    StaleLedgerEntry,
    Stalled,
    RestartInterrupted,
    UntrackedServiceProcess,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeActivityFinding {
    pub code: RuntimeActivityFindingCode,
    pub activity_id: RuntimeActivityId,
    pub related_activity_id: Option<RuntimeActivityId>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeActivityCensusQuery {
    pub stall_after_ms: Option<u64>,
    pub recent_abnormal_limit: Option<u32>,
    #[serde(default)]
    pub live_evidence: Vec<RuntimeActivityLiveEvidence>,
}

impl Default for RuntimeActivityCensusQuery {
    fn default() -> Self {
        Self {
            stall_after_ms: Some(5 * 60 * 1_000),
            recent_abnormal_limit: Some(100),
            live_evidence: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeActivityCensusSummary {
    pub active: u32,
    pub recently_abnormal: u32,
    pub findings: u32,
    pub untracked_processes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeActivityView {
    pub activity: RuntimeActivityRecord,
    pub elapsed_ms: u64,
    pub since_progress_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeActivityCensus {
    pub generated_at: IsoTimestamp,
    pub service_instance_id: String,
    pub active: Vec<RuntimeActivityView>,
    pub recently_abnormal: Vec<RuntimeActivityView>,
    pub findings: Vec<RuntimeActivityFinding>,
    pub summary: RuntimeActivityCensusSummary,
    pub automatic_cancellation_enabled: bool,
}

pub fn runtime_dispatch_activity_id(wake_id: &str) -> RuntimeActivityId {
    RuntimeActivityId::new(format!("dispatch:{wake_id}"))
}

pub fn runtime_wake_activity_id(wake_id: &str) -> RuntimeActivityId {
    RuntimeActivityId::new(format!("wake:{wake_id}"))
}

pub fn runtime_provider_activity_id(wake_id: &str) -> RuntimeActivityId {
    RuntimeActivityId::new(format!("provider:{wake_id}"))
}

pub fn runtime_tool_activity_id(wake_id: &str, call_id: &str) -> RuntimeActivityId {
    RuntimeActivityId::new(format!("tool:{wake_id}:{call_id}"))
}
