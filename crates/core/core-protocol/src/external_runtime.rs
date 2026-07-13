//! Runtime-neutral contracts for complete external agent runtimes.
//!
//! External runtimes own their native agent loop. These types keep Crew's
//! lifecycle, routing, controller, and persistence decisions in Rust without
//! pretending a native turn is a direct-brain wake.

use crate::{
    AgentId, CoreError, CoreErrorKind, CoreResult, DenRuntimeReference, IsoTimestamp, ProfileId,
    RunId, SessionId, SessionKind, SessionStatus,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

macro_rules! external_string_id {
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

external_string_id!(ExternalRuntimeId);
external_string_id!(ExternalBindingId);
external_string_id!(ExternalTurnRequestId);
external_string_id!(ExternalControlId);
external_string_id!(ExternalInteractionId);
external_string_id!(AgentRoundId);
external_string_id!(AgentMessageDeliveryId);
external_string_id!(ExternalAgentSessionCreationId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExternalRuntimeKind {
    CodexAppServer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentDirectoryRuntimeKind {
    DirectBrain,
    CodexAppServer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentDirectoryEntry {
    pub agent_id: AgentId,
    pub session_id: SessionId,
    pub profile_id: ProfileId,
    pub display_label: String,
    pub session_kind: SessionKind,
    pub session_status: SessionStatus,
    pub runtime_kind: AgentDirectoryRuntimeKind,
    pub runtime_id: Option<ExternalRuntimeId>,
    pub binding_id: Option<ExternalBindingId>,
    pub binding_status: Option<ExternalBindingStatus>,
    pub task_ref: Option<DenRuntimeReference>,
    pub workdir: Option<String>,
    pub routable: bool,
    pub routability_reason_code: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExternalEndpointTransport {
    UnixWebSocket,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExternalProcessOwnership {
    Attached,
    Managed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExternalRuntimeDesiredState {
    Enabled,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExternalRuntimeObservedState {
    Disconnected,
    Connecting,
    Ready,
    Degraded,
    Incompatible,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalEndpoint {
    pub transport: ExternalEndpointTransport,
    pub address: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalRuntimeRegistration {
    pub runtime_id: ExternalRuntimeId,
    pub kind: ExternalRuntimeKind,
    pub endpoint: ExternalEndpoint,
    pub process_ownership: ExternalProcessOwnership,
    pub codex_home_ref: Option<String>,
    pub expected_cli_version: String,
    pub executable_sha256: String,
    pub protocol_schema_sha256: String,
    pub desired_state: ExternalRuntimeDesiredState,
    pub observed_state: ExternalRuntimeObservedState,
    pub observed_reason_code: Option<String>,
    pub revision: u64,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalControllerContext {
    pub holder_instance_id: String,
    pub generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalRuntimeHandshakeObservation {
    pub runtime_id: ExternalRuntimeId,
    pub controller: ExternalControllerContext,
    pub cli_version: String,
    pub executable_sha256: String,
    pub protocol_schema_sha256: String,
    pub observed_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalRuntimeHandshakeDecision {
    pub accepted: bool,
    pub reason_code: Option<String>,
    pub registration: ExternalRuntimeRegistration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalRuntimeStateObservation {
    pub runtime_id: ExternalRuntimeId,
    pub controller: ExternalControllerContext,
    pub observed_state: ExternalRuntimeObservedState,
    pub reason_code: Option<String>,
    pub observed_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalControllerLease {
    pub runtime_id: ExternalRuntimeId,
    pub holder_instance_id: String,
    pub generation: u64,
    pub acquired_at: IsoTimestamp,
    pub renewed_at: IsoTimestamp,
    pub expires_at: IsoTimestamp,
    pub revision: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExternalBindingPurpose {
    CrewAgent,
    ImportedObserver,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExternalBindingStatus {
    Active,
    Paused,
    Archived,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalAgentBinding {
    pub binding_id: ExternalBindingId,
    pub runtime_id: ExternalRuntimeId,
    pub session_id: Option<SessionId>,
    pub agent_id: Option<AgentId>,
    pub purpose: ExternalBindingPurpose,
    pub native_thread_id: Option<String>,
    pub cwd: Option<String>,
    pub task_ref: Option<DenRuntimeReference>,
    pub effective_config_fingerprint: String,
    pub status: ExternalBindingStatus,
    pub revision: u64,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

impl ExternalAgentBinding {
    pub fn validate(&self) -> CoreResult<()> {
        validate_non_empty("binding_id", &self.binding_id.0)?;
        validate_non_empty("runtime_id", &self.runtime_id.0)?;
        validate_non_empty(
            "effective_config_fingerprint",
            &self.effective_config_fingerprint,
        )?;
        if self.purpose == ExternalBindingPurpose::CrewAgent
            && (self.session_id.is_none() || self.agent_id.is_none())
        {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "crew_agent external binding requires session_id and agent_id",
            ));
        }
        if self.purpose == ExternalBindingPurpose::ImportedObserver
            && (self.session_id.is_some() || self.agent_id.is_some())
        {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "imported_observer external binding cannot be routable",
            ));
        }
        Ok(())
    }

    pub fn is_routable(&self) -> bool {
        self.purpose == ExternalBindingPurpose::CrewAgent
            && self.status == ExternalBindingStatus::Active
            && self.session_id.is_some()
            && self.agent_id.is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExternalAgentSessionCreationPhase {
    Prepared,
    BindingReady,
    NativeStarting,
    RecoveryRequired,
    Ready,
}

impl ExternalAgentSessionCreationPhase {
    pub fn can_transition_to(self, next: Self) -> bool {
        match (self, next) {
            (current, next) if current == next => true,
            (Self::Prepared, Self::BindingReady | Self::RecoveryRequired) => true,
            (Self::BindingReady, Self::NativeStarting | Self::RecoveryRequired) => true,
            (Self::NativeStarting, Self::RecoveryRequired | Self::Ready) => true,
            (Self::RecoveryRequired, Self::NativeStarting | Self::Ready) => true,
            (Self::Ready, Self::Ready) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalAgentSessionCreationRequest {
    pub idempotency_key: String,
    pub runtime_id: ExternalRuntimeId,
    pub profile_id: ProfileId,
    pub cwd: String,
    pub task_ref: Option<DenRuntimeReference>,
    pub label: Option<String>,
    pub requested_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalAgentSessionIdentity {
    pub session_id: SessionId,
    pub agent_id: AgentId,
    pub profile_id: ProfileId,
    pub status: SessionStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalAgentSessionCreationRecord {
    pub creation_id: ExternalAgentSessionCreationId,
    pub request: ExternalAgentSessionCreationRequest,
    pub request_fingerprint: String,
    pub session: ExternalAgentSessionIdentity,
    pub binding: ExternalAgentBinding,
    pub native_thread_source: String,
    pub native_thread_id: Option<String>,
    pub phase: ExternalAgentSessionCreationPhase,
    pub reason_code: Option<String>,
    pub reason_message: Option<String>,
    pub revision: u64,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TurnInputProvenanceKind {
    Operator,
    RoutedAgentMessage,
    ScheduledWake,
    ExternalWaitResult,
    Control,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExternalCollaborationMode {
    Plan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TurnInputProvenance {
    pub kind: TurnInputProvenanceKind,
    pub source_id: Option<String>,
    pub correlation_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ExternalTurnInputPart {
    Text { text: String },
    Image { url: String },
    Skill { name: String, path: Option<String> },
    MachineFact { kind: String, payload: Value },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionTurnRequested {
    pub request_id: ExternalTurnRequestId,
    pub idempotency_key: String,
    pub session_id: SessionId,
    pub run_id: Option<RunId>,
    pub binding_id: ExternalBindingId,
    pub input: Vec<ExternalTurnInputPart>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collaboration_mode: Option<ExternalCollaborationMode>,
    pub provenance: TurnInputProvenance,
    pub created_at: IsoTimestamp,
    pub expires_at: Option<IsoTimestamp>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExternalTurnPhase {
    Accepted,
    Starting,
    Active,
    WaitingInteraction,
    Completed,
    Failed,
    Interrupted,
    OutcomeUnknown,
}

impl ExternalTurnPhase {
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Interrupted | Self::OutcomeUnknown
        )
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        match (self, next) {
            (current, next) if current == next => true,
            (Self::Accepted, Self::Starting | Self::Failed | Self::Interrupted) => true,
            (
                Self::Starting,
                Self::Active | Self::Failed | Self::Interrupted | Self::OutcomeUnknown,
            ) => true,
            (
                Self::Active,
                Self::WaitingInteraction
                | Self::Completed
                | Self::Failed
                | Self::Interrupted
                | Self::OutcomeUnknown,
            ) => true,
            (
                Self::WaitingInteraction,
                Self::Active | Self::Failed | Self::Interrupted | Self::OutcomeUnknown,
            ) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalTurnCorrelation {
    pub request: SessionTurnRequested,
    pub runtime_id: ExternalRuntimeId,
    pub native_thread_id: String,
    pub native_turn_id: Option<String>,
    pub task_ref: Option<DenRuntimeReference>,
    pub phase: ExternalTurnPhase,
    pub capacity_lease_id: Option<String>,
    pub terminal_reason_code: Option<String>,
    pub revision: u64,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    tag = "type"
)]
pub enum AgentActivation {
    DirectBrainWakeRequested {
        session_id: SessionId,
        wake_id: String,
    },
    ExternalTurnRequested {
        session_id: SessionId,
        request_id: ExternalTurnRequestId,
        binding_id: ExternalBindingId,
    },
    QueuedForNextTurn {
        session_id: SessionId,
        queue_id: String,
    },
    Rejected {
        reason_code: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExternalControlKind {
    StartOrResumeThread,
    StartTurn,
    SteerTurn,
    InterruptTurn,
    CompactThread,
    ResolveInteraction,
    ReconcileRuntime,
    ArchiveBinding,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalControlRequest {
    pub control_id: ExternalControlId,
    pub idempotency_key: String,
    pub binding_id: ExternalBindingId,
    pub expected_binding_revision: u64,
    pub expected_native_turn_id: Option<String>,
    pub kind: ExternalControlKind,
    pub payload: Value,
    pub requested_at: IsoTimestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExternalControlStatus {
    Pending,
    Applied,
    Rejected,
    Failed,
}

impl ExternalControlStatus {
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Applied | Self::Rejected | Self::Failed)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalControlReceipt {
    pub request: ExternalControlRequest,
    pub request_fingerprint: String,
    pub status: ExternalControlStatus,
    pub outcome: Option<Value>,
    pub reason_code: Option<String>,
    pub revision: u64,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExternalInteractionKind {
    CommandApproval,
    FileApproval,
    RequestUserInput,
    PermissionRequest,
    McpElicitation,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExternalInteractionStatus {
    Pending,
    Resolved,
    Expired,
    Lost,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalInteractionRecord {
    pub interaction_id: ExternalInteractionId,
    pub runtime_id: ExternalRuntimeId,
    pub binding_id: ExternalBindingId,
    pub request_id: ExternalTurnRequestId,
    pub native_thread_id: String,
    pub native_turn_id: String,
    pub native_request_id: String,
    pub kind: ExternalInteractionKind,
    pub prompt: Value,
    pub allowed_responses: Vec<String>,
    pub status: ExternalInteractionStatus,
    pub resolution_idempotency_key: Option<String>,
    pub outcome: Option<Value>,
    pub raw_detail_ref: Option<String>,
    pub requested_at: IsoTimestamp,
    pub expires_at: IsoTimestamp,
    pub resolved_at: Option<IsoTimestamp>,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedExternalRuntimeEvent {
    pub event_id: String,
    pub session_id: Option<SessionId>,
    pub sequence_id: u64,
    pub created_at: IsoTimestamp,
    pub kind: String,
    pub runtime_id: ExternalRuntimeId,
    pub native_thread_id: Option<String>,
    pub native_turn_id: Option<String>,
    pub item_id: Option<String>,
    pub request_id: Option<String>,
    pub payload: Value,
    pub raw_detail_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalRuntimeEventInput {
    pub event_id: String,
    pub session_id: Option<SessionId>,
    pub created_at: IsoTimestamp,
    pub kind: String,
    pub runtime_id: ExternalRuntimeId,
    pub native_thread_id: Option<String>,
    pub native_turn_id: Option<String>,
    pub item_id: Option<String>,
    pub request_id: Option<String>,
    pub payload: Value,
    pub raw_detail_ref: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentRoundStatus {
    Pending,
    Replied,
    Expired,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentMessageDeliveryStatus {
    Pending,
    Accepted,
    Rejected,
    Expired,
}

impl AgentMessageDeliveryStatus {
    pub const fn is_terminal(self) -> bool {
        !matches!(self, Self::Pending)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentMessageDeliveryRequest {
    pub delivery_id: AgentMessageDeliveryId,
    pub idempotency_key: String,
    pub message_id: String,
    pub from_agent_id: AgentId,
    pub to_agent_id: AgentId,
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collaboration_mode: Option<ExternalCollaborationMode>,
    pub correlation_id: Option<String>,
    pub require_wake: bool,
    pub created_at: IsoTimestamp,
    pub expires_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    tag = "type"
)]
pub enum AgentCoordinationCaller {
    System {
        sender_agent_id: AgentId,
    },
    DirectBrain {
        session_id: SessionId,
        wake_id: String,
        tool_call_id: String,
    },
    ExternalAgent {
        runtime_id: ExternalRuntimeId,
        binding_id: ExternalBindingId,
        controller_instance_id: String,
        controller_generation: u64,
        native_thread_id: String,
        native_turn_id: String,
        native_request_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentMessageCommand {
    pub caller: AgentCoordinationCaller,
    pub delivery_id: AgentMessageDeliveryId,
    pub idempotency_key: String,
    pub message_id: String,
    pub to_agent_id: AgentId,
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collaboration_mode: Option<ExternalCollaborationMode>,
    pub correlation_id: Option<String>,
    pub require_wake: bool,
    pub created_at: IsoTimestamp,
    pub expires_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentRoundCommand {
    pub caller: AgentCoordinationCaller,
    pub round_id: AgentRoundId,
    pub idempotency_key: String,
    pub message_id: String,
    pub to_agent_id: AgentId,
    pub body: String,
    pub correlation_id: String,
    pub created_at: IsoTimestamp,
    pub expires_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentRoundStartReceipt {
    pub round: AgentCorrelatedRound,
    pub delivery: AgentMessageDeliveryReceipt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentMessageDeliveryReceipt {
    pub request: AgentMessageDeliveryRequest,
    pub status: AgentMessageDeliveryStatus,
    pub sequence: Option<u64>,
    pub activation: Option<AgentActivation>,
    pub resolved_round_id: Option<AgentRoundId>,
    pub reason_code: Option<String>,
    pub terminal_at: Option<IsoTimestamp>,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentCorrelatedRound {
    pub round_id: AgentRoundId,
    pub idempotency_key: String,
    pub sender_agent_id: AgentId,
    pub sender_session_id: SessionId,
    pub recipient_agent_id: AgentId,
    pub recipient_session_id: SessionId,
    pub sender_request_id: Option<ExternalTurnRequestId>,
    pub message_id: String,
    pub correlation_id: String,
    pub reply_message_id: Option<String>,
    pub status: AgentRoundStatus,
    pub outcome: Option<Value>,
    pub terminal_reason_code: Option<String>,
    pub created_at: IsoTimestamp,
    pub expires_at: IsoTimestamp,
    pub terminal_at: Option<IsoTimestamp>,
    pub revision: u64,
}

pub fn validate_external_runtime_registration(
    registration: &ExternalRuntimeRegistration,
) -> CoreResult<()> {
    validate_non_empty("runtime_id", &registration.runtime_id.0)?;
    validate_non_empty("endpoint.address", &registration.endpoint.address)?;
    validate_non_empty("expected_cli_version", &registration.expected_cli_version)?;
    validate_sha256("executable_sha256", &registration.executable_sha256)?;
    validate_sha256(
        "protocol_schema_sha256",
        &registration.protocol_schema_sha256,
    )?;
    if registration.kind == ExternalRuntimeKind::CodexAppServer
        && registration.endpoint.transport != ExternalEndpointTransport::UnixWebSocket
    {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "codex_app_server requires unix_web_socket transport",
        ));
    }
    Ok(())
}

pub fn validate_external_turn_transition(
    current: ExternalTurnPhase,
    next: ExternalTurnPhase,
) -> CoreResult<()> {
    if current.can_transition_to(next) {
        return Ok(());
    }
    Err(CoreError::new(
        CoreErrorKind::ActionRejected,
        format!("invalid external turn transition {current:?} -> {next:?}"),
    ))
}

fn validate_non_empty(field: &str, value: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("{field} must not be empty"),
        ));
    }
    Ok(())
}

fn validate_sha256(field: &str, value: &str) -> CoreResult<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("{field} must be a 64-character hexadecimal SHA-256"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_external_turns_cannot_resurrect() {
        for terminal in [
            ExternalTurnPhase::Completed,
            ExternalTurnPhase::Failed,
            ExternalTurnPhase::Interrupted,
            ExternalTurnPhase::OutcomeUnknown,
        ] {
            assert!(validate_external_turn_transition(terminal, terminal).is_ok());
            assert!(
                validate_external_turn_transition(terminal, ExternalTurnPhase::Active).is_err()
            );
        }
    }

    #[test]
    fn imported_bindings_cannot_become_implicit_agent_identities() {
        let binding = ExternalAgentBinding {
            binding_id: ExternalBindingId::new("imported"),
            runtime_id: ExternalRuntimeId::new("codex-local"),
            session_id: Some(SessionId::new("session")),
            agent_id: None,
            purpose: ExternalBindingPurpose::ImportedObserver,
            native_thread_id: Some("thread".into()),
            cwd: None,
            task_ref: None,
            effective_config_fingerprint: "config".into(),
            status: ExternalBindingStatus::Active,
            revision: 1,
            created_at: "2026-07-10T00:00:00Z".into(),
            updated_at: "2026-07-10T00:00:00Z".into(),
        };
        assert!(binding.validate().is_err());
        assert!(!binding.is_routable());
    }
}
