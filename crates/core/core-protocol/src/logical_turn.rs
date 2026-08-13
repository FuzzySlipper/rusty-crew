//! Durable logical-turn continuation contracts.
//!
//! These types separate one logical user turn from process-local execution
//! epochs. Rust coordination owns their lifecycle; provider crates own the
//! versioned module payload carried by a checkpoint.

use crate::{IsoTimestamp, ProfileId, SessionId};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogicalTurnBindingSnapshot {
    pub profile_id: ProfileId,
    pub profile_revision: u64,
    pub prompt_fingerprint: String,
    pub tool_selection_fingerprint: String,
    pub tool_registry_revision: String,
    pub brain_module_id: String,
    pub brain_strategy_id: String,
    pub model_config_id: String,
    pub model_config_revision: u64,
    pub endpoint_id: String,
    pub endpoint_revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_alias: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_revision: Option<u64>,
    pub provider_fingerprint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_revision: Option<u64>,
}

impl<'de> Deserialize<'de> for LogicalTurnBindingSnapshot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct PersistedBindingSnapshot {
            profile_id: ProfileId,
            profile_revision: u64,
            prompt_fingerprint: String,
            tool_selection_fingerprint: String,
            tool_registry_revision: String,
            brain_module_id: String,
            brain_strategy_id: String,
            model_config_id: Option<String>,
            model_config_revision: Option<u64>,
            endpoint_id: Option<String>,
            endpoint_revision: Option<u64>,
            provider_alias: Option<String>,
            provider_revision: Option<u64>,
            provider_fingerprint: String,
            credential_id: Option<String>,
            credential_revision: Option<u64>,
        }

        let persisted = PersistedBindingSnapshot::deserialize(deserializer)?;
        let model_config_id = persisted
            .model_config_id
            .or_else(|| persisted.provider_alias.clone())
            .ok_or_else(|| serde::de::Error::missing_field("modelConfigId"))?;
        let endpoint_id = persisted
            .endpoint_id
            .or_else(|| persisted.provider_alias.clone())
            .ok_or_else(|| serde::de::Error::missing_field("endpointId"))?;
        let model_config_revision = persisted
            .model_config_revision
            .or(persisted.provider_revision)
            .unwrap_or_default();
        let endpoint_revision = persisted
            .endpoint_revision
            .or(persisted.provider_revision)
            .unwrap_or_default();

        Ok(Self {
            profile_id: persisted.profile_id,
            profile_revision: persisted.profile_revision,
            prompt_fingerprint: persisted.prompt_fingerprint,
            tool_selection_fingerprint: persisted.tool_selection_fingerprint,
            tool_registry_revision: persisted.tool_registry_revision,
            brain_module_id: persisted.brain_module_id,
            brain_strategy_id: persisted.brain_strategy_id,
            model_config_id,
            model_config_revision,
            endpoint_id,
            endpoint_revision,
            provider_alias: persisted.provider_alias,
            provider_revision: persisted.provider_revision,
            provider_fingerprint: persisted.provider_fingerprint,
            credential_id: persisted.credential_id,
            credential_revision: persisted.credential_revision,
        })
    }
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
    pub result_payload: Option<JsonValue>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
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

impl<'de> Deserialize<'de> for LogicalTurnLifecycleEvent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct PersistedLifecycleEvent {
            projection_id: TurnProjectionId,
            logical_turn_id: LogicalTurnId,
            session_id: SessionId,
            wake_id: String,
            continuation_id: ContinuationId,
            continuation_count: Option<u64>,
            execution_epoch_id: Option<ExecutionEpochId>,
            kind: LogicalTurnLifecycleEventKind,
            phase: LogicalTurnPhase,
            operator_state: Option<LogicalTurnOperatorState>,
            progress_classification: Option<LogicalTurnProgressClassification>,
            progress: LogicalTurnProgress,
            reason_code: String,
            summary: String,
            occurred_at: IsoTimestamp,
            logical_turn_revision: u64,
        }

        let persisted = PersistedLifecycleEvent::deserialize(deserializer)?;
        let attention_required = persisted.kind == LogicalTurnLifecycleEventKind::AttentionRequired;
        let operator_state = persisted
            .operator_state
            .unwrap_or_else(|| LogicalTurnOperatorState::for_phase(persisted.phase));
        let progress_classification = persisted.progress_classification.unwrap_or_else(|| {
            LogicalTurnProgressClassification::for_state(
                persisted.phase,
                attention_required,
                &persisted.progress,
            )
        });

        Ok(Self {
            projection_id: persisted.projection_id,
            logical_turn_id: persisted.logical_turn_id,
            session_id: persisted.session_id,
            wake_id: persisted.wake_id,
            continuation_id: persisted.continuation_id,
            continuation_count: persisted.continuation_count.unwrap_or(0),
            execution_epoch_id: persisted.execution_epoch_id,
            kind: persisted.kind,
            phase: persisted.phase,
            operator_state,
            progress_classification,
            progress: persisted.progress,
            reason_code: persisted.reason_code,
            summary: persisted.summary,
            occurred_at: persisted.occurred_at,
            logical_turn_revision: persisted.logical_turn_revision,
        })
    }
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
    pub binding: LogicalTurnBindingSnapshot,
    pub current_continuation_id: ContinuationId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_execution_epoch_id: Option<ExecutionEpochId>,
    pub continuation_count: u64,
    pub provider_request_total: u64,
    pub tool_round_total: u64,
    pub phase: LogicalTurnPhase,
    pub operator_state: LogicalTurnOperatorState,
    pub progress_classification: LogicalTurnProgressClassification,
    pub progress: LogicalTurnProgress,
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

    #[test]
    fn lifecycle_events_hydrate_from_the_pre_diagnostics_persisted_shape() {
        let event: LogicalTurnLifecycleEvent = serde_json::from_value(serde_json::json!({
            "projectionId": "projection-1",
            "logicalTurnId": "turn-1",
            "sessionId": "session-1",
            "wakeId": "wake-1",
            "continuationId": "continuation-2",
            "executionEpochId": "epoch-1",
            "kind": "continuation_yielded",
            "phase": "yielded",
            "progress": {
                "semanticRevision": 3,
                "committedProviderOperations": 2,
                "committedToolOperations": 1,
                "committedProjectionCursor": 4,
                "assistantContentBytes": 5,
                "acceptedActionCount": 0,
                "delegatedCompletionCount": 0,
                "stateFingerprint": "state-1",
                "lastLivenessAt": "2026-07-29T00:00:01Z",
                "lastSemanticProgressAt": "2026-07-29T00:00:01Z"
            },
            "reasonCode": "logical_turn_yielded",
            "summary": "Turn yielded",
            "occurredAt": "2026-07-29T00:00:01Z",
            "logicalTurnRevision": 2
        }))
        .unwrap();

        assert_eq!(event.continuation_count, 0);
        assert_eq!(
            event.operator_state,
            LogicalTurnOperatorState::QueuedToContinue
        );
        assert_eq!(
            event.progress_classification,
            LogicalTurnProgressClassification::ToolProgress
        );
    }

    #[test]
    fn binding_snapshot_hydrates_from_legacy_provider_identity() {
        let binding: LogicalTurnBindingSnapshot = serde_json::from_value(serde_json::json!({
            "profileId": "profile-1",
            "profileRevision": 2,
            "promptFingerprint": "prompt-1",
            "toolSelectionFingerprint": "tools-1",
            "toolRegistryRevision": "registry-1",
            "brainModuleId": "brain-1",
            "brainStrategyId": "strategy-1",
            "providerAlias": "legacy-provider",
            "providerRevision": 7,
            "providerFingerprint": "provider-1"
        }))
        .unwrap();

        assert_eq!(binding.model_config_id, "legacy-provider");
        assert_eq!(binding.model_config_revision, 7);
        assert_eq!(binding.endpoint_id, "legacy-provider");
        assert_eq!(binding.endpoint_revision, 7);
        assert_eq!(binding.provider_alias.as_deref(), Some("legacy-provider"));
        assert_eq!(binding.provider_revision, Some(7));
    }
}
