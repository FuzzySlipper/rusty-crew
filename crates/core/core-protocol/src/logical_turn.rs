//! Durable logical-turn continuation contracts.
//!
//! These types separate one logical user turn from process-local execution
//! epochs. Rust coordination owns their lifecycle; provider crates own the
//! versioned module payload carried by a checkpoint.

use crate::{IsoTimestamp, ProfileId, SessionId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

macro_rules! string_id {
    ($name:ident) => {
        #[derive(
            Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, PartialOrd, Ord,
        )]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            pub fn new(raw: impl Into<String>) -> Self {
                Self(raw.into())
            }
        }
    };
}

string_id!(LogicalTurnId);
string_id!(ContinuationId);
string_id!(ExecutionEpochId);
string_id!(BrainOperationId);
string_id!(TurnProjectionId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LogicalTurnPhase {
    Admitted,
    Runnable,
    Running,
    Yielded,
    AttentionRequired,
    CancelRequested,
    Completed,
    Cancelled,
    Failed,
}

impl LogicalTurnPhase {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Failed)
    }

    pub fn is_runnable(self) -> bool {
        matches!(self, Self::Admitted | Self::Runnable | Self::Yielded)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContinuationYieldReason {
    InitialAdmission,
    WorkQuantumReached,
    SchedulerFairness,
    ProviderRetry,
    BufferPressure,
    RestartRecovery,
    OperatorRequested,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LogicalTurnAttentionReason {
    NoProgress,
    ToolOutcomeUnknown,
    ProviderConfigurationRequired,
    ProviderCredentialRequired,
    ProviderProtocolFailure,
    CheckpointVersionUnsupported,
    RebindIncompatible,
    StorageRepairRequired,
    InvariantRepairRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LogicalTurnResolutionAction {
    RetryUnchanged,
    RetryProviderOperation,
    ConfirmToolCompleted,
    ConfirmToolNotCompleted,
    Rebind,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogicalTurnBindingSnapshot {
    pub profile_id: ProfileId,
    pub profile_revision: u64,
    pub prompt_fingerprint: String,
    pub tool_selection_fingerprint: String,
    pub tool_registry_revision: String,
    pub brain_module_id: String,
    pub brain_strategy_id: String,
    pub provider_alias: String,
    pub provider_revision: u64,
    pub provider_fingerprint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_revision: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogicalTurnFrozenInput {
    pub body_state_ref: String,
    pub body_state_fingerprint: String,
    pub system_prompt_ref: String,
    pub system_prompt_fingerprint: String,
    pub role_assembly_ref: String,
    pub role_assembly_fingerprint: String,
    pub transcript_cursor: u64,
    #[serde(default)]
    pub attachment_refs: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogicalTurnProgress {
    pub semantic_revision: u64,
    pub committed_provider_operations: u64,
    pub committed_tool_operations: u64,
    pub committed_projection_cursor: u64,
    pub assistant_content_bytes: u64,
    pub accepted_action_count: u64,
    pub delegated_completion_count: u64,
    pub state_fingerprint: String,
    pub last_liveness_at: IsoTimestamp,
    pub last_semantic_progress_at: IsoTimestamp,
    #[serde(default)]
    pub consecutive_no_progress_samples: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BrainContinuationPayload {
    pub module_id: String,
    pub payload_version: String,
    pub payload_fingerprint: String,
    pub payload: JsonValue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogicalTurnCheckpoint {
    pub continuation_id: ContinuationId,
    pub logical_turn_id: LogicalTurnId,
    pub sequence: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_continuation_id: Option<ContinuationId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_epoch_id: Option<ExecutionEpochId>,
    pub binding_generation: u64,
    pub frozen_input: LogicalTurnFrozenInput,
    pub module_state: BrainContinuationPayload,
    pub operation_cursor: u64,
    pub projection_cursor: u64,
    pub progress: LogicalTurnProgress,
    pub yield_reason: ContinuationYieldReason,
    pub created_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogicalTurnRecord {
    pub logical_turn_id: LogicalTurnId,
    pub session_id: SessionId,
    pub source_wake_id: String,
    pub phase: LogicalTurnPhase,
    pub binding: LogicalTurnBindingSnapshot,
    pub current_continuation_id: ContinuationId,
    pub continuation_sequence: u64,
    pub binding_generation: u64,
    pub cancellation_generation: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_epoch_id: Option<ExecutionEpochId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim_generation: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim_holder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim_expires_at: Option<IsoTimestamp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attention: Option<LogicalTurnAttention>,
    pub revision: u64,
    pub admitted_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_at: Option<IsoTimestamp>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogicalTurnAttention {
    pub reason: LogicalTurnAttentionReason,
    pub reason_code: String,
    pub summary: String,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    #[serde(default)]
    pub resolution_actions: Vec<LogicalTurnResolutionAction>,
    pub retry_unchanged_safe: bool,
    pub required_at: IsoTimestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LogicalTurnOperationKind {
    ProviderRequest,
    HostToolExecution,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LogicalTurnOperationPhase {
    Planned,
    Leased,
    Completed,
    OutcomeUnknown,
    Superseded,
    CompletedAfterCancel,
}

impl LogicalTurnOperationPhase {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Superseded | Self::CompletedAfterCancel
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogicalTurnOperationRecord {
    pub operation_id: BrainOperationId,
    pub logical_turn_id: LogicalTurnId,
    pub continuation_id: ContinuationId,
    pub execution_epoch_id: ExecutionEpochId,
    pub kind: LogicalTurnOperationKind,
    pub phase: LogicalTurnOperationPhase,
    pub request_fingerprint: String,
    pub idempotency_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease_holder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease_generation: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease_expires_at: Option<IsoTimestamp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
    pub revision: u64,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LogicalTurnLifecycleEventKind {
    Admitted,
    ContinuationClaimed,
    ContinuationProgress,
    ContinuationCheckpointed,
    ContinuationYielded,
    ContinuationResumed,
    AttentionRequired,
    RebindRequested,
    Rebound,
    CancelRequested,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogicalTurnLifecycleEvent {
    pub projection_id: TurnProjectionId,
    pub logical_turn_id: LogicalTurnId,
    pub session_id: SessionId,
    pub wake_id: String,
    pub continuation_id: ContinuationId,
    pub continuation_count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_epoch_id: Option<ExecutionEpochId>,
    pub kind: LogicalTurnLifecycleEventKind,
    pub phase: LogicalTurnPhase,
    pub operator_state: LogicalTurnOperatorState,
    pub progress_classification: LogicalTurnProgressClassification,
    pub progress: LogicalTurnProgress,
    pub reason_code: String,
    pub summary: String,
    pub occurred_at: IsoTimestamp,
    pub logical_turn_revision: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LogicalTurnOperatorState {
    QueuedToContinue,
    Running,
    PausedForAttention,
    Cancelling,
    Completed,
    Cancelled,
    Failed,
}

impl LogicalTurnOperatorState {
    pub fn for_phase(phase: LogicalTurnPhase) -> Self {
        match phase {
            LogicalTurnPhase::Admitted | LogicalTurnPhase::Runnable | LogicalTurnPhase::Yielded => {
                Self::QueuedToContinue
            }
            LogicalTurnPhase::Running => Self::Running,
            LogicalTurnPhase::AttentionRequired => Self::PausedForAttention,
            LogicalTurnPhase::CancelRequested => Self::Cancelling,
            LogicalTurnPhase::Completed => Self::Completed,
            LogicalTurnPhase::Cancelled => Self::Cancelled,
            LogicalTurnPhase::Failed => Self::Failed,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LogicalTurnProgressClassification {
    Admitted,
    ProviderProgress,
    ToolProgress,
    SemanticProgress,
    LivenessOnly,
    NoProgress,
    AttentionRequired,
    Completed,
    Cancelled,
    Failed,
}

impl LogicalTurnProgressClassification {
    pub fn for_state(
        phase: LogicalTurnPhase,
        attention_required: bool,
        progress: &LogicalTurnProgress,
    ) -> Self {
        if attention_required {
            return Self::AttentionRequired;
        }
        match phase {
            LogicalTurnPhase::Completed => Self::Completed,
            LogicalTurnPhase::Cancelled => Self::Cancelled,
            LogicalTurnPhase::Failed => Self::Failed,
            _ if progress.consecutive_no_progress_samples > 0 => Self::NoProgress,
            _ if progress.committed_tool_operations > 0 => Self::ToolProgress,
            _ if progress.committed_provider_operations > 0 => Self::ProviderProgress,
            _ if progress.semantic_revision > 0 => Self::SemanticProgress,
            LogicalTurnPhase::Admitted => Self::Admitted,
            _ => Self::LivenessOnly,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogicalTurnDiagnostic {
    pub logical_turn_id: LogicalTurnId,
    pub session_id: SessionId,
    pub source_wake_id: String,
    pub current_continuation_id: ContinuationId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_execution_epoch_id: Option<ExecutionEpochId>,
    pub continuation_count: u64,
    pub provider_request_total: u64,
    pub tool_round_total: u64,
    pub phase: LogicalTurnPhase,
    pub operator_state: LogicalTurnOperatorState,
    pub progress_classification: LogicalTurnProgressClassification,
    pub last_progress_at: IsoTimestamp,
    pub last_liveness_at: IsoTimestamp,
    pub reason_code: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attention: Option<LogicalTurnAttention>,
    pub revision: u64,
    pub admitted_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_at: Option<IsoTimestamp>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogicalTurnDiagnosticQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logical_turn_id: Option<LogicalTurnId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    #[serde(default)]
    pub include_terminal: bool,
    #[serde(default = "default_logical_turn_diagnostic_limit")]
    pub limit: u32,
}

fn default_logical_turn_diagnostic_limit() -> u32 {
    100
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogicalTurnDiagnosticPage {
    pub items: Vec<LogicalTurnDiagnostic>,
    pub total: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogicalTurnAdmission {
    pub record: LogicalTurnRecord,
    pub initial_checkpoint: LogicalTurnCheckpoint,
    pub lifecycle_event: LogicalTurnLifecycleEvent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogicalTurnClaimRequest {
    pub logical_turn_id: LogicalTurnId,
    pub expected_revision: u64,
    pub continuation_id: ContinuationId,
    pub execution_epoch_id: ExecutionEpochId,
    pub claim_holder: String,
    pub claim_expires_at: IsoTimestamp,
    pub now: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogicalTurnContinuationClaim {
    pub record: LogicalTurnRecord,
    pub checkpoint: LogicalTurnCheckpoint,
    pub claim_generation: u64,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogicalTurnYieldRequest {
    pub logical_turn_id: LogicalTurnId,
    pub expected_revision: u64,
    pub expected_epoch_id: ExecutionEpochId,
    pub expected_claim_generation: u64,
    pub expected_cancellation_generation: u64,
    pub checkpoint: LogicalTurnCheckpoint,
    pub lifecycle_event: LogicalTurnLifecycleEvent,
    pub now: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogicalTurnYieldReceipt {
    pub record: LogicalTurnRecord,
    pub checkpoint: LogicalTurnCheckpoint,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogicalTurnAttentionRequest {
    pub logical_turn_id: LogicalTurnId,
    pub expected_revision: u64,
    pub expected_epoch_id: ExecutionEpochId,
    pub expected_claim_generation: u64,
    pub expected_cancellation_generation: u64,
    pub checkpoint: LogicalTurnCheckpoint,
    pub attention: LogicalTurnAttention,
    pub lifecycle_event: LogicalTurnLifecycleEvent,
    pub now: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogicalTurnAttentionReceipt {
    pub record: LogicalTurnRecord,
    pub checkpoint: LogicalTurnCheckpoint,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogicalTurnAttentionResolutionRequest {
    pub logical_turn_id: LogicalTurnId,
    pub expected_revision: u64,
    pub action: LogicalTurnResolutionAction,
    pub lifecycle_event: LogicalTurnLifecycleEvent,
    pub now: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogicalTurnAttentionResolutionReceipt {
    pub record: LogicalTurnRecord,
    pub checkpoint: LogicalTurnCheckpoint,
    pub action: LogicalTurnResolutionAction,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogicalTurnCancelRequest {
    pub logical_turn_id: LogicalTurnId,
    pub expected_revision: u64,
    pub idempotency_key: String,
    pub reason_code: String,
    pub summary: String,
    pub now: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogicalTurnCancellationReceipt {
    pub record: LogicalTurnRecord,
    pub replayed: bool,
    pub already_terminal: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogicalTurnHydrationReport {
    pub inspected: u32,
    pub made_runnable: u32,
    pub attention_required: u32,
    pub already_runnable: u32,
    pub terminal_skipped: u32,
    pub hydrated_at: IsoTimestamp,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yielded_and_attention_phases_are_nonterminal() {
        assert!(!LogicalTurnPhase::Yielded.is_terminal());
        assert!(!LogicalTurnPhase::AttentionRequired.is_terminal());
        assert!(LogicalTurnPhase::Yielded.is_runnable());
        assert!(!LogicalTurnPhase::AttentionRequired.is_runnable());
        assert!(LogicalTurnPhase::Completed.is_terminal());
        assert!(LogicalTurnPhase::Cancelled.is_terminal());
        assert!(LogicalTurnPhase::Failed.is_terminal());
    }

    #[test]
    fn lifecycle_wire_names_are_stable_and_do_not_add_timeout_phase() {
        assert_eq!(
            serde_json::to_value(LogicalTurnPhase::AttentionRequired).unwrap(),
            "attention_required"
        );
        assert_eq!(
            serde_json::to_value(ContinuationYieldReason::WorkQuantumReached).unwrap(),
            "work_quantum_reached"
        );
        let serialized = serde_json::to_value(LogicalTurnClaimRequest {
            logical_turn_id: LogicalTurnId::new("turn-1"),
            expected_revision: 1,
            continuation_id: ContinuationId::new("continuation-1"),
            execution_epoch_id: ExecutionEpochId::new("epoch-1"),
            claim_holder: "service-1".into(),
            claim_expires_at: "2026-07-29T00:00:30Z".into(),
            now: "2026-07-29T00:00:00Z".into(),
        })
        .unwrap();
        assert_eq!(serialized["logicalTurnId"], "turn-1");
        assert!(serialized.get("timeoutMs").is_none());
    }
}
