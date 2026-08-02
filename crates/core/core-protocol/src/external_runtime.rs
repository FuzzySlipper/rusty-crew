//! Runtime-neutral contracts for complete external agent runtimes.
//!
//! External runtimes own their native agent loop. These types keep Crew's
//! lifecycle, routing, controller, and persistence decisions in Rust without
//! pretending a native turn is a direct-brain wake.

use crate::{
    AgentId, CoreError, CoreErrorKind, CoreResult, DenRuntimeReference, IsoTimestamp, ProfileId,
    RunId, SessionId, SessionKind, SessionState, SessionStatus,
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
external_string_id!(AgentRouteKey);

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution: Option<crate::SessionExecutionState>,
    pub runtime_kind: AgentDirectoryRuntimeKind,
    pub runtime_id: Option<ExternalRuntimeId>,
    pub binding_id: Option<ExternalBindingId>,
    pub binding_status: Option<ExternalBindingStatus>,
    pub task_ref: Option<DenRuntimeReference>,
    pub workdir: Option<String>,
    pub routable: bool,
    pub routability_reason_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    tag = "type"
)]
pub enum AgentRouteTarget {
    DirectBrain {
        agent_id: AgentId,
        session_id: SessionId,
    },
    ManagedExternal {
        agent_id: AgentId,
        binding_id: ExternalBindingId,
        binding_revision: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentRouteRecord {
    pub route_key: AgentRouteKey,
    pub label: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub target: AgentRouteTarget,
    pub required_runtime_kind: Option<AgentDirectoryRuntimeKind>,
    pub required_delivery_policy: Option<ExternalMessageDeliveryPolicy>,
    pub revision: u64,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

impl AgentRouteRecord {
    pub fn address(&self) -> String {
        format!("@{}", self.route_key.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentRouteWrite {
    pub route_key: AgentRouteKey,
    pub label: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub target: AgentRouteTarget,
    pub required_runtime_kind: Option<AgentDirectoryRuntimeKind>,
    pub required_delivery_policy: Option<ExternalMessageDeliveryPolicy>,
    pub expected_revision: Option<u64>,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentRouteDelete {
    pub route_key: AgentRouteKey,
    pub expected_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentRouteResolvedTarget {
    pub agent_id: AgentId,
    pub session_id: SessionId,
    pub profile_id: ProfileId,
    pub display_label: String,
    pub runtime_kind: AgentDirectoryRuntimeKind,
    pub runtime_id: Option<ExternalRuntimeId>,
    pub binding_id: Option<ExternalBindingId>,
    pub binding_revision: Option<u64>,
    pub delivery_policy: Option<ExternalMessageDeliveryPolicy>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentRouteResolution {
    pub address: String,
    pub route: Option<AgentRouteRecord>,
    pub routable: bool,
    pub reason_code: Option<String>,
    pub resolved_target: Option<AgentRouteResolvedTarget>,
    pub last_delivery: Option<AgentRouteLastDelivery>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentRouteLastDelivery {
    pub delivery_id: AgentMessageDeliveryId,
    pub route_revision: u64,
    pub status: AgentMessageDeliveryStatus,
    pub reason_code: Option<String>,
    pub created_at: IsoTimestamp,
    pub terminal_at: Option<IsoTimestamp>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentRouteDeliveryProvenance {
    pub address: String,
    pub route_key: AgentRouteKey,
    pub route_revision: u64,
    pub resolved_target: AgentRouteResolvedTarget,
}

pub fn agent_route_address(route_key: &AgentRouteKey) -> String {
    format!("@{}", route_key.0)
}

pub fn parse_agent_route_address(address: &str) -> CoreResult<Option<AgentRouteKey>> {
    let Some(route_key) = address.strip_prefix('@') else {
        return Ok(None);
    };
    validate_agent_route_key(route_key)?;
    Ok(Some(AgentRouteKey::new(route_key)))
}

pub fn validate_agent_route_write(write: &AgentRouteWrite) -> CoreResult<()> {
    validate_agent_route_key(&write.route_key.0)?;
    validate_bounded_route_text("label", &write.label, 256, false)?;
    if let Some(description) = &write.description {
        validate_bounded_route_text("description", description, 4_096, true)?;
    }
    validate_non_empty("updated_at", &write.updated_at)?;
    if write.expected_revision == Some(0) {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "agent route expected_revision must be positive",
        ));
    }
    match &write.target {
        AgentRouteTarget::DirectBrain {
            agent_id,
            session_id,
        } => {
            validate_non_empty("target.agent_id", &agent_id.0)?;
            validate_non_empty("target.session_id", &session_id.0)?;
            if matches!(
                write.required_runtime_kind,
                Some(AgentDirectoryRuntimeKind::CodexAppServer)
            ) {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "direct-brain route cannot require codex_app_server runtime",
                ));
            }
            if write.required_delivery_policy.is_some() {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "direct-brain route cannot require an external delivery policy",
                ));
            }
        }
        AgentRouteTarget::ManagedExternal {
            agent_id,
            binding_id,
            binding_revision,
        } => {
            validate_non_empty("target.agent_id", &agent_id.0)?;
            validate_non_empty("target.binding_id", &binding_id.0)?;
            if *binding_revision == 0 {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "managed external route binding_revision must be positive",
                ));
            }
            if matches!(
                write.required_runtime_kind,
                Some(AgentDirectoryRuntimeKind::DirectBrain)
            ) {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "managed external route cannot require direct_brain runtime",
                ));
            }
        }
    }
    Ok(())
}

fn validate_agent_route_key(route_key: &str) -> CoreResult<()> {
    if route_key.is_empty()
        || route_key.len() > 128
        || !route_key
            .chars()
            .next()
            .is_some_and(|value| value.is_ascii_alphanumeric())
        || !route_key
            .chars()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, '.' | '_' | '-'))
    {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "agent route key must be 1-128 characters, start alphanumeric, and contain only alphanumeric, '.', '_', or '-'",
        ));
    }
    Ok(())
}

fn validate_bounded_route_text(
    label: &str,
    value: &str,
    max_chars: usize,
    allow_empty: bool,
) -> CoreResult<()> {
    if (!allow_empty && value.trim().is_empty()) || value.chars().count() > max_chars {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("agent route {label} must contain at most {max_chars} characters"),
        ));
    }
    Ok(())
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExternalRuntimeCompatibilityState {
    Unassessed,
    CompatibleUncertified,
    Certified,
    Incompatible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExternalRuntimeCompatibilityProbeOutcome {
    Passed,
    TransportRetryable,
    Incompatible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExternalRuntimeCompatibilityProbeStepStatus {
    Passed,
    Skipped,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalRuntimeCompatibilityProbeStep {
    pub step_id: String,
    pub status: ExternalRuntimeCompatibilityProbeStepStatus,
    pub duration_ms: u64,
    pub reason_code: Option<String>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalRuntimeCompatibilityProbeReport {
    pub suite_revision: String,
    pub outcome: ExternalRuntimeCompatibilityProbeOutcome,
    pub steps: Vec<ExternalRuntimeCompatibilityProbeStep>,
    pub completed_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalRuntimeProbeEvidenceRecord {
    pub runtime_id: ExternalRuntimeId,
    pub runtime_kind: ExternalRuntimeKind,
    pub observed_cli_version: String,
    pub consumed_contract_revision: String,
    pub probe_report: ExternalRuntimeCompatibilityProbeReport,
    pub observed_at: IsoTimestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExternalRuntimeCertificationStatus {
    Active,
    Superseded,
    Invalidated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalRuntimeCertificationRecord {
    pub certification_id: String,
    pub idempotency_key: String,
    pub certified_runtime_id: ExternalRuntimeId,
    pub runtime_kind: ExternalRuntimeKind,
    pub observed_cli_version: String,
    pub consumed_contract_revision: String,
    pub probe_suite_revision: String,
    pub evidence_summary: String,
    pub status: ExternalRuntimeCertificationStatus,
    pub superseded_by_certification_id: Option<String>,
    pub invalidated_at: Option<IsoTimestamp>,
    pub invalidation_reason: Option<String>,
    pub revision: u64,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalRuntimeCertificationRequest {
    pub certification_id: String,
    pub idempotency_key: String,
    pub runtime_id: ExternalRuntimeId,
    pub evidence_summary: String,
    pub requested_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalRuntimeCertificationInvalidation {
    pub certification_id: String,
    pub expected_revision: u64,
    pub reason: String,
    pub invalidated_at: IsoTimestamp,
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
    pub observed_cli_version: Option<String>,
    pub consumed_contract_revision: Option<String>,
    pub compatibility_state: ExternalRuntimeCompatibilityState,
    pub last_compatibility_probe: Option<ExternalRuntimeCompatibilityProbeReport>,
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
    pub consumed_contract_revision: String,
    pub probe_report: ExternalRuntimeCompatibilityProbeReport,
    pub observed_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalRuntimeHandshakeDecision {
    pub accepted: bool,
    pub retryable: bool,
    pub compatibility_state: ExternalRuntimeCompatibilityState,
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExternalMessageDeliveryPolicy {
    #[default]
    ImmediateSteer,
    SerialNextTurn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalAgentBinding {
    pub binding_id: ExternalBindingId,
    pub runtime_id: ExternalRuntimeId,
    pub session_id: Option<SessionId>,
    pub agent_id: Option<AgentId>,
    #[serde(default)]
    pub profile_id: Option<ProfileId>,
    #[serde(default)]
    pub profile_revision: Option<u64>,
    #[serde(default)]
    pub profile_prompt_hash: Option<String>,
    #[serde(default)]
    pub profile_prompt_snapshot: Option<String>,
    #[serde(default)]
    pub message_delivery_policy: ExternalMessageDeliveryPolicy,
    pub purpose: ExternalBindingPurpose,
    pub native_thread_id: Option<String>,
    pub cwd: Option<String>,
    pub label: Option<String>,
    pub task_ref: Option<DenRuntimeReference>,
    pub effective_config_fingerprint: String,
    pub status: ExternalBindingStatus,
    pub revision: u64,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalAgentBindingMetadataWrite {
    pub binding_id: ExternalBindingId,
    pub expected_revision: u64,
    pub label: Option<String>,
    pub task_ref: Option<DenRuntimeReference>,
    pub updated_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalAgentBindingRestoreRequest {
    pub binding_id: ExternalBindingId,
    pub expected_binding_revision: u64,
    pub expected_session_id: SessionId,
    pub expected_agent_id: AgentId,
    pub expected_profile_id: ProfileId,
    pub expected_native_thread_id: String,
    pub restored_at: IsoTimestamp,
}

impl ExternalAgentBindingRestoreRequest {
    pub fn validate(&self) -> CoreResult<()> {
        validate_non_empty("binding_id", &self.binding_id.0)?;
        validate_non_empty("expected_session_id", &self.expected_session_id.0)?;
        validate_non_empty("expected_agent_id", &self.expected_agent_id.0)?;
        validate_non_empty("expected_profile_id", &self.expected_profile_id.0)?;
        validate_non_empty("expected_native_thread_id", &self.expected_native_thread_id)?;
        validate_non_empty("restored_at", &self.restored_at)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExternalAgentBindingRestoreOutcome {
    Restored,
    AlreadyActive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExternalAgentBindingRestoreReceipt {
    pub outcome: ExternalAgentBindingRestoreOutcome,
    pub binding: ExternalAgentBinding,
    pub session: SessionState,
    pub profile_revision_updated: bool,
}

impl ExternalAgentBindingMetadataWrite {
    pub fn validate(&self) -> CoreResult<()> {
        validate_non_empty("binding_id", &self.binding_id.0)?;
        validate_non_empty("updated_at", &self.updated_at)?;
        if let Some(label) = &self.label {
            validate_non_empty("label", label)?;
            if label.chars().count() > 256 {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "external binding label exceeds 256 characters",
                ));
            }
        }
        if let Some(task_ref) = &self.task_ref {
            if task_ref.project_id.is_none() && task_ref.task_id.is_none() {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "external binding task_ref requires project_id or task_id",
                ));
            }
        }
        Ok(())
    }
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
            && (self.session_id.is_none()
                || self.agent_id.is_none()
                || self.profile_id.is_none()
                || self.profile_revision.is_none()
                || self.profile_prompt_hash.is_none())
        {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "crew_agent external binding requires session, agent, and profile prompt provenance",
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
pub struct ExternalTurnTerminalError {
    pub message: String,
    pub code: Option<String>,
    pub additional_details: Option<String>,
    pub will_retry: Option<bool>,
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
    #[serde(default)]
    pub terminal_error: Option<ExternalTurnTerminalError>,
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
    ExternalTurnSteerRequested {
        session_id: SessionId,
        request_id: ExternalTurnRequestId,
        binding_id: ExternalBindingId,
        native_thread_id: String,
        native_turn_id: String,
        message_text: String,
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
    ExecuteThreadCommand,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentMessageInputKind {
    Operator,
    RoutedAgentMessage,
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
    pub from_session_id: Option<SessionId>,
    pub requested_address: String,
    pub to_agent_id: AgentId,
    pub to_session_id: Option<SessionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routing: Option<Box<AgentRouteDeliveryProvenance>>,
    pub reply_to_message_id: Option<String>,
    pub input_kind: AgentMessageInputKind,
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
pub struct AgentMessageReplyCommand {
    pub caller: AgentCoordinationCaller,
    pub delivery_id: AgentMessageDeliveryId,
    pub idempotency_key: String,
    pub message_id: String,
    pub in_reply_to_message_id: String,
    pub body: String,
    pub created_at: IsoTimestamp,
    pub expires_at: IsoTimestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentMessageInboxStatus {
    Queued,
    InProgress,
    AwaitingReply,
    Replied,
    NoReply,
    Failed,
    Expired,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentMessageInboxItem {
    pub delivery: AgentMessageDeliveryReceipt,
    pub reply: Option<AgentMessageDeliveryReceipt>,
    pub status: AgentMessageInboxStatus,
    /// Exact text presented to the recipient brain after Rust-owned provenance framing.
    pub delivered_model_text: String,
    pub queued_message_id: Option<String>,
    pub external_turn_request_id: Option<ExternalTurnRequestId>,
    pub terminal_reason_code: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentMessageInboxQuery {
    pub to_agent_id: Option<AgentId>,
    pub to_session_id: Option<SessionId>,
    pub from_agent_id: Option<AgentId>,
    pub from_session_id: Option<SessionId>,
    pub correlation_id: Option<String>,
    pub message_id: Option<String>,
    pub limit: Option<u32>,
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
    pub to_address: String,
    pub input_kind: AgentMessageInputKind,
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
    pub to_address: String,
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
pub struct AgentMessageDeliveryCompletion {
    pub delivery_id: AgentMessageDeliveryId,
    pub expected_revision: u64,
    pub status: AgentMessageDeliveryStatus,
    pub reason_code: Option<String>,
    pub completed_at: IsoTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentCorrelatedRound {
    pub round_id: AgentRoundId,
    pub idempotency_key: String,
    pub sender_agent_id: AgentId,
    pub sender_session_id: Option<SessionId>,
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
    if let Some(cli_version) = &registration.observed_cli_version {
        validate_non_empty("observed_cli_version", cli_version)?;
    }
    if let Some(contract_revision) = &registration.consumed_contract_revision {
        validate_non_empty("consumed_contract_revision", contract_revision)?;
    }
    if registration.observed_cli_version.is_some()
        != registration.consumed_contract_revision.is_some()
    {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "external runtime compatibility identity must be complete",
        ));
    }
    if registration.compatibility_state != ExternalRuntimeCompatibilityState::Unassessed
        && (registration.observed_cli_version.is_none()
            || registration.consumed_contract_revision.is_none())
    {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "assessed external runtime requires observed CLI and consumed contract identity",
        ));
    }
    if registration.observed_state == ExternalRuntimeObservedState::Ready
        && !matches!(
            registration.compatibility_state,
            ExternalRuntimeCompatibilityState::CompatibleUncertified
                | ExternalRuntimeCompatibilityState::Certified
        )
    {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "ready external runtime requires compatible contract state",
        ));
    }
    if registration.compatibility_state == ExternalRuntimeCompatibilityState::Incompatible
        && (registration.observed_state != ExternalRuntimeObservedState::Incompatible
            || registration.observed_reason_code.is_none())
    {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "incompatible external runtime requires incompatible observed state and reason",
        ));
    }
    if let Some(report) = &registration.last_compatibility_probe {
        validate_external_runtime_compatibility_probe_report(report)?;
    }
    match (
        registration
            .last_compatibility_probe
            .as_ref()
            .map(|report| report.outcome),
        registration.compatibility_state,
    ) {
        (None, ExternalRuntimeCompatibilityState::Unassessed)
        | (
            Some(ExternalRuntimeCompatibilityProbeOutcome::TransportRetryable),
            ExternalRuntimeCompatibilityState::Unassessed,
        )
        | (
            Some(ExternalRuntimeCompatibilityProbeOutcome::Passed),
            ExternalRuntimeCompatibilityState::CompatibleUncertified,
        )
        | (
            Some(ExternalRuntimeCompatibilityProbeOutcome::Passed),
            ExternalRuntimeCompatibilityState::Certified,
        )
        | (
            Some(ExternalRuntimeCompatibilityProbeOutcome::Incompatible),
            ExternalRuntimeCompatibilityState::Incompatible,
        ) => {}
        _ => {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "external runtime compatibility state must match its latest probe report",
            ));
        }
    }
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

pub fn validate_external_runtime_certification_record(
    record: &ExternalRuntimeCertificationRecord,
) -> CoreResult<()> {
    validate_non_empty("certification_id", &record.certification_id)?;
    validate_non_empty("idempotency_key", &record.idempotency_key)?;
    validate_non_empty("certified_runtime_id", &record.certified_runtime_id.0)?;
    validate_non_empty("observed_cli_version", &record.observed_cli_version)?;
    validate_non_empty(
        "consumed_contract_revision",
        &record.consumed_contract_revision,
    )?;
    validate_non_empty("probe_suite_revision", &record.probe_suite_revision)?;
    validate_bounded_certification_text("evidence_summary", &record.evidence_summary, 4_096)?;
    validate_non_empty("created_at", &record.created_at)?;
    validate_non_empty("updated_at", &record.updated_at)?;
    if record.certification_id.len() > 256 || record.idempotency_key.len() > 256 {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "certification identifiers exceed 256 bytes",
        ));
    }
    match record.status {
        ExternalRuntimeCertificationStatus::Active => {
            if record.superseded_by_certification_id.is_some()
                || record.invalidated_at.is_some()
                || record.invalidation_reason.is_some()
            {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "active certification cannot carry terminal metadata",
                ));
            }
        }
        ExternalRuntimeCertificationStatus::Superseded => {
            let successor = record
                .superseded_by_certification_id
                .as_deref()
                .ok_or_else(|| {
                    CoreError::new(
                        CoreErrorKind::InvalidInput,
                        "superseded certification requires successor identifier",
                    )
                })?;
            validate_non_empty("superseded_by_certification_id", successor)?;
            if record.invalidated_at.is_some() || record.invalidation_reason.is_some() {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "superseded certification cannot carry invalidation metadata",
                ));
            }
        }
        ExternalRuntimeCertificationStatus::Invalidated => {
            let at = record.invalidated_at.as_deref().ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "invalidated certification requires timestamp",
                )
            })?;
            let reason = record.invalidation_reason.as_deref().ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "invalidated certification requires reason",
                )
            })?;
            validate_non_empty("invalidated_at", at)?;
            validate_bounded_certification_text("invalidation_reason", reason, 1_024)?;
            if record.superseded_by_certification_id.is_some() {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "invalidated certification cannot carry successor metadata",
                ));
            }
        }
    }
    Ok(())
}

pub fn validate_external_runtime_probe_evidence(
    evidence: &ExternalRuntimeProbeEvidenceRecord,
) -> CoreResult<()> {
    validate_non_empty("runtime_id", &evidence.runtime_id.0)?;
    validate_non_empty("observed_cli_version", &evidence.observed_cli_version)?;
    validate_non_empty(
        "consumed_contract_revision",
        &evidence.consumed_contract_revision,
    )?;
    validate_non_empty("observed_at", &evidence.observed_at)?;
    validate_external_runtime_compatibility_probe_report(&evidence.probe_report)?;
    if evidence.probe_report.outcome != ExternalRuntimeCompatibilityProbeOutcome::Passed {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "durable compatibility evidence requires a passing probe",
        ));
    }
    Ok(())
}

pub fn validate_external_runtime_certification_request(
    request: &ExternalRuntimeCertificationRequest,
) -> CoreResult<()> {
    validate_non_empty("certification_id", &request.certification_id)?;
    validate_non_empty("idempotency_key", &request.idempotency_key)?;
    validate_non_empty("runtime_id", &request.runtime_id.0)?;
    validate_bounded_certification_text("evidence_summary", &request.evidence_summary, 4_096)?;
    validate_non_empty("requested_at", &request.requested_at)?;
    if request.certification_id.len() > 256 || request.idempotency_key.len() > 256 {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "certification identifiers exceed 256 bytes",
        ));
    }
    Ok(())
}

pub fn validate_external_runtime_certification_invalidation(
    invalidation: &ExternalRuntimeCertificationInvalidation,
) -> CoreResult<()> {
    validate_non_empty("certification_id", &invalidation.certification_id)?;
    validate_bounded_certification_text("reason", &invalidation.reason, 1_024)?;
    validate_non_empty("invalidated_at", &invalidation.invalidated_at)
}

fn validate_bounded_certification_text(label: &str, value: &str, max: usize) -> CoreResult<()> {
    validate_non_empty(label, value)?;
    if value.len() > max {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("{label} exceeds {max} bytes"),
        ));
    }
    Ok(())
}

pub fn validate_external_runtime_handshake_observation(
    observation: &ExternalRuntimeHandshakeObservation,
) -> CoreResult<()> {
    validate_non_empty("runtime_id", &observation.runtime_id.0)?;
    validate_non_empty("cli_version", &observation.cli_version)?;
    validate_non_empty(
        "consumed_contract_revision",
        &observation.consumed_contract_revision,
    )?;
    validate_external_runtime_compatibility_probe_report(&observation.probe_report)
}

pub fn validate_external_runtime_compatibility_probe_report(
    report: &ExternalRuntimeCompatibilityProbeReport,
) -> CoreResult<()> {
    use std::collections::HashSet;

    validate_non_empty("probe_report.suite_revision", &report.suite_revision)?;
    validate_non_empty("probe_report.completed_at", &report.completed_at)?;
    if report.steps.is_empty() || report.steps.len() > 32 {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "compatibility probe report requires between 1 and 32 steps",
        ));
    }
    let mut step_ids = HashSet::new();
    let mut failed_reason = None;
    for step in &report.steps {
        validate_non_empty("probe_report.steps.step_id", &step.step_id)?;
        if step.step_id.len() > 128 || !step_ids.insert(step.step_id.as_str()) {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "compatibility probe step identifiers must be unique and at most 128 bytes",
            ));
        }
        if let Some(detail) = &step.detail {
            if detail.len() > 1_024 {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "compatibility probe step detail exceeds 1024 bytes",
                ));
            }
        }
        match (step.status, step.reason_code.as_deref()) {
            (ExternalRuntimeCompatibilityProbeStepStatus::Passed, None) => {}
            (ExternalRuntimeCompatibilityProbeStepStatus::Skipped, Some(reason))
            | (ExternalRuntimeCompatibilityProbeStepStatus::Failed, Some(reason)) => {
                validate_non_empty("probe_report.steps.reason_code", reason)?;
                if step.status == ExternalRuntimeCompatibilityProbeStepStatus::Failed
                    && failed_reason.is_none()
                {
                    failed_reason = Some(reason);
                }
            }
            _ => {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "probe step status and reason code are incoherent",
                ));
            }
        }
    }
    match (report.outcome, failed_reason) {
        (ExternalRuntimeCompatibilityProbeOutcome::Passed, None) => Ok(()),
        (ExternalRuntimeCompatibilityProbeOutcome::Passed, Some(_)) => Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "passing compatibility probe cannot contain a failed step",
        )),
        (ExternalRuntimeCompatibilityProbeOutcome::TransportRetryable, Some(_))
        | (ExternalRuntimeCompatibilityProbeOutcome::Incompatible, Some(_)) => Ok(()),
        (_, None) => Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "failed compatibility probe requires a failed step with a reason code",
        )),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switchboard_address_grammar_is_explicit_and_collision_free() {
        assert_eq!(
            parse_agent_route_address("@reviewer").unwrap(),
            Some(AgentRouteKey::new("reviewer"))
        );
        assert_eq!(parse_agent_route_address("reviewer").unwrap(), None);
        assert!(parse_agent_route_address("@review queue").is_err());
        assert!(parse_agent_route_address("@").is_err());
    }

    #[test]
    fn compatibility_probe_reports_require_coherent_reasoning() {
        let passed_report = ExternalRuntimeCompatibilityProbeReport {
            suite_revision: "codex-required-v1".into(),
            outcome: ExternalRuntimeCompatibilityProbeOutcome::Passed,
            steps: vec![ExternalRuntimeCompatibilityProbeStep {
                step_id: "model_list".into(),
                status: ExternalRuntimeCompatibilityProbeStepStatus::Passed,
                duration_ms: 2,
                reason_code: None,
                detail: None,
            }],
            completed_at: "2026-07-14T00:00:00Z".into(),
        };
        let mut observation = ExternalRuntimeHandshakeObservation {
            runtime_id: ExternalRuntimeId::new("codex-local"),
            controller: ExternalControllerContext {
                holder_instance_id: "controller-a".into(),
                generation: 1,
            },
            cli_version: "0.144.3".into(),
            consumed_contract_revision: "contract-v1".into(),
            probe_report: passed_report,
            observed_at: "2026-07-14T00:00:00Z".into(),
        };
        validate_external_runtime_handshake_observation(&observation).unwrap();

        observation.probe_report.steps[0].reason_code = Some("unexpected_reason".into());
        assert_eq!(
            validate_external_runtime_handshake_observation(&observation)
                .unwrap_err()
                .kind,
            CoreErrorKind::InvalidInput
        );

        observation.probe_report.outcome = ExternalRuntimeCompatibilityProbeOutcome::Incompatible;
        observation.probe_report.steps[0].status =
            ExternalRuntimeCompatibilityProbeStepStatus::Failed;
        observation.probe_report.steps[0].reason_code = None;
        assert_eq!(
            validate_external_runtime_handshake_observation(&observation)
                .unwrap_err()
                .kind,
            CoreErrorKind::InvalidInput
        );
    }

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
            profile_id: None,
            profile_revision: None,
            profile_prompt_hash: None,
            profile_prompt_snapshot: None,
            message_delivery_policy: ExternalMessageDeliveryPolicy::ImmediateSteer,
            purpose: ExternalBindingPurpose::ImportedObserver,
            native_thread_id: Some("thread".into()),
            cwd: None,
            label: None,
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
