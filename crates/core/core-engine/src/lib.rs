//! Coordination engine composition.

mod agent_coordination;
mod agent_route_activation;
mod body;
mod body_queue;
mod bootstrap;
mod brain_runtime;
mod chat;
mod chat_store;
mod crew_sessions;
mod curator;
mod delegation;
mod delegation_store;
mod external_binding_restore;
mod external_controls;
mod external_follow_up;
mod external_runtime;
mod external_runtime_certification;
mod external_runtime_compatibility;
mod github_gate;
mod github_gate_wait;
mod logical_turns;
pub use logical_turns::{
    LogicalTurnEpochResult, LogicalTurnEpochSettlement, LogicalTurnWakePreparation,
};
mod maintenance;
mod memory;
mod memory_spaces;
mod memory_store;
mod profile_admin;
mod provider_runtime;
mod provider_state_store;
mod roleplay;
mod roleplay_lore_store;
mod roleplay_mechanic;
mod roleplay_proposals;
mod roleplay_records_store;
mod runtime_activity;
mod runtime_admin;
mod runtime_admin_store;
mod scheduler;
mod session_store;
mod sessions;

pub(crate) use body::{add_millis_to_iso, sanitized_clock_key};
pub use delegation::{delegated_agent_id, delegated_session_id};
pub use external_runtime::{
    AgentActivationRequest, ExternalControllerTurnTransition, ExternalRuntimeHydrationReport,
};
pub use scheduler::SchedulerTickReport;

use body_queue::{
    drain_follow_up_queue_for_wake, enforce_follow_up_queue_cap, save_body_follow_up_message,
};
use chat_store::{ChatConversationStore, ChatEventStore};
use delegation_store::{
    claim_next_worker_pool_work_item, complete_worker_pool_work_item, create_worker_pool_work_item,
    delegated_completions_for_parent, delegated_fan_out_groups_for_parent,
    load_delegated_worker_run, load_delegated_worker_run_by_session, load_worker_pool_member,
    save_delegated_worker_run_requested, update_delegated_worker_run_status,
    update_delegated_worker_run_status_by_session,
};
use github_gate_wait::{
    list_waits as list_github_gate_waits, load_cursor as load_github_gate_cursor,
    load_wait as load_github_gate_wait, save_cursor as save_github_gate_cursor,
    save_wait as save_github_gate_wait,
};
use memory_store::CrewMemoryStore;
use provider_state_store::{
    clear_provider_state as clear_provider_state_store,
    list_provider_state_diagnostics as list_provider_state_store_diagnostics,
    load_provider_state_for_wake, save_provider_state as save_provider_state_store,
};
use roleplay_lore_store::RoleplayLoreStore;
use roleplay_records_store::RoleplayRecordsStore;
use runtime_admin_store::{
    RuntimeModuleDataStore, RuntimeServiceDataStore, RuntimeStorageAdminStore,
};
use rusty_crew_core_body::{
    session_kind_can_wake, BodyProjector, BrainActionExecutor, DefaultWakeThreshold, WakeThreshold,
};
use rusty_crew_core_bus::CoreBus;
use rusty_crew_core_config::{validate_engine_config, ClockConfig, EngineConfig};
use rusty_crew_core_persistence::{
    roleplay_lore_memory_space_descriptor, ApplyRoleplayAlternativeRequest,
    ApplyRoleplayAlternativeResult, AttachmentQuery, AttachmentRecord, AttachmentWrite,
    BranchAwareSessionMemoryQuery, BranchHeadExpectation, ChatEventLogAppend, ChatEventLogEvent,
    ChatEventLogPage, ChatEventLogQuery, ChatReadModelEvent, ChatReadModelEventKind,
    ChatReadModelPage, ChatReadModelQuery, ChatReadModelSource, ChatSessionReadFacts,
    ChatSessionReadQuery, ChatSessionReadResult, ChatSessionSummaryPage,
    ChatSessionSummaryPageQuery, ChatTranscriptSearchPage, ChatTranscriptSearchQuery,
    ConversationBranchQuery, ConversationBranchRecord, ConversationBranchStateRecord,
    ConversationBranchWrite, ConversationJumpRequest, ConversationJumpResult,
    ConversationSnapshotQuery, ConversationSnapshotRecord, ConversationSnapshotWrite,
    ConversationTreeReadQuery, ConversationTreeReadResult, CoreCoordinationStore,
    CreateChatAttachmentRequest, CreateChatAttachmentResult, CreateChatConversationBranchRequest,
    CreateChatConversationSnapshotRequest, CreateChatConversationSnapshotResult,
    CreateChatDataBankScopeRequest, CreateChatDataBankScopeResult, CreateChatMessageSlotRequest,
    CreateChatMessageSlotResult, CreateChatMessageVariantRequest, CreateChatMessageVariantResult,
    CuratorAuditQuery, CuratorAuditReceiptRecord, CuratorCandidateQuery, CuratorCandidateRecord,
    CuratorGovernanceWrite, CuratorGovernanceWriteResult, CuratorMutationQuery,
    CuratorMutationRecord, DataBankScopeQuery, DataBankScopeRecord, DataBankScopeWrite,
    DeleteChatMessageVariantRequest, DurableMessageRecord,
    EnsureActiveChatConversationBranchRequest, EnsureActiveChatConversationBranchResult, ExactPage,
    LogicalTurnAdmissionWrite, LogicalTurnCompletionRequest, LogicalTurnContentWrite,
    LogicalTurnContinuationTicket, LogicalTurnOperationCompletionRequest,
    LogicalTurnOperationLeaseRequest, LogicalTurnOutboxRecord, LoreRecallQuery, LoreRecallResult,
    LoreRecallTraceQuery, LoreRecallTraceRecord, MessageSlotQuery, MessageSlotRecord,
    MessageSlotWrite, MessageVariantQuery, MessageVariantRecord, MessageVariantWrite,
    ProfileMemoryCaps, ProfileMemoryDelete, ProfileMemoryQuery, ProfileMemoryRecord,
    ProfileMemoryReplace, ProfileMemoryTarget, ProfileMemoryWrite, ProfileRegistryQuery,
    ProviderWireStateDiagnostic, ProviderWireStateInvalidationReason, ProviderWireStateKey,
    ProviderWireStateWakeLookup, ProviderWireStateWrite, QueuedMessageFilter, QueuedMessageRecord,
    QueuedMessageState, RemoveChatAttachmentRequest, RemoveChatDataBankScopeRequest,
    ReorderChatMessageVariantsRequest, RoleplayCharacterQuery, RoleplayCharacterRecord,
    RoleplayCharacterWrite, RoleplayChatLayerRecord, RoleplayChatLayersWrite, RoleplayImportQuery,
    RoleplayImportRecord, RoleplayImportWrite, RoleplayLoreEntryPromotion, RoleplayLoreFactCapture,
    RoleplayLoreLayerArchive, RoleplayLoreLayerConfigRecord, RoleplayLoreLayerConfigWrite,
    RoleplayLoreLayerEntryJoin, RoleplayLoreLayerEntryLink, RoleplayLoreLayerRecord,
    RoleplayLoreLayerUpdate, RoleplayLoreLayerWrite, RoleplayLoreProvenanceEvent,
    RoleplayLoreQuery, RoleplayLoreRecord, RoleplayLoreReplace, RoleplayLoreSupersede,
    RoleplayLoreTombstone, RoleplayLoreWrite, RoleplayPlayerPersonaQuery,
    RoleplayPlayerPersonaRecord, RoleplayPlayerPersonaWrite, RoleplaySessionMetadataQuery,
    RoleplaySessionMetadataRecord, RoleplaySessionMetadataWrite, RoleplaySessionProjectionRecord,
    RoleplaySessionProjectionWrite, RuntimeCounterQuery, RuntimeCounterRecord, RuntimeCounterScope,
    RuntimeDatabaseSize, RuntimeMaintenancePolicy, RuntimeMaintenanceReport,
    RuntimeModuleSchemaRegistryDiagnostics, RuntimeSearchFilter, RuntimeSearchResult,
    RuntimeStateSummary, RuntimeStorageDiagnostics, SelectActiveBranchRequest,
    SelectActiveBranchResult, SelectActiveChatMessageVariantRequest,
    SelectActiveChatMessageVariantResult, SelectActiveVariantRequest, SelectActiveVariantResult,
    SessionMemoryPromptContext, SessionMemoryQuery, SessionMemoryRecord,
    SessionMessageVariantPageQuery, SimpleKvDelete, SimpleKvQuery, SimpleKvRecord, SimpleKvWrite,
    UpdateBranchHeadRequest, UpdateBranchHeadResult, WorkerPoolClaimRecord, WorkerPoolClaimRequest,
    WorkerPoolCompletionRequest, WorkerPoolMemberStatus, WorkerPoolNoCapacityReason,
    WorkerPoolWorkItemRecord, WorkerPoolWorkStatus, WorkerRunRecord, WorkerRunStatus,
};
use rusty_crew_core_protocol::{
    session_memory_space_descriptor, ActionBatchReceipt, ActionRejection, AgentId, AgentMessage,
    AttachmentId, BodyState, BrainAction, BrainActionBatch, BrainContinuationPayload, BrainEvent,
    BrainEventEnvelope, BrainImplementationRegistration, BrainProviderStateScope,
    BrainWakeAttention, BrainWakeOutcome, BrainWakeProgressSnapshot, BrainWakeProviderStateInput,
    BrainWakeProviderStateOutput, BrainWakeProviderStateUpdate, CompletionStatus,
    ContextCompactionArtifact, ContextCompactionArtifactQuery, ContinuationId,
    ContinuationYieldReason, CoreError, CoreErrorKind, CoreEvent, CoreResult, DataBankScopeId,
    DelegatedResourceCleanupReport, DelegatedRunStatus, DelegatedSessionRuntimeStatus,
    DelegationLifecycleEvent, DelegationLifecyclePhase, DelegationLineage, DenDataUpdate,
    EngineHandle, EventReceipt, EventSubscription, ExecutionEpochId, ExternalEvent,
    ExternalTurnPhase, FanOutFailurePolicy, GitHubGateSuspendRequest, GitHubGateTerminalEvent,
    GitHubGateTerminalReceipt, GitHubGateWaitPhase, GitHubGateWaitRecord, GitHubGateWakeResult,
    IsoTimestamp, LogicalTurnAdmission, LogicalTurnAttention, LogicalTurnAttentionReceipt,
    LogicalTurnAttentionRequest, LogicalTurnAttentionResolutionReceipt,
    LogicalTurnAttentionResolutionRequest, LogicalTurnBindingSnapshot, LogicalTurnCancelRequest,
    LogicalTurnCancellationReceipt, LogicalTurnCheckpoint, LogicalTurnClaimRequest,
    LogicalTurnContinuationClaim, LogicalTurnDiagnostic, LogicalTurnDiagnosticPage,
    LogicalTurnDiagnosticQuery, LogicalTurnFrozenInput, LogicalTurnHydrationReport, LogicalTurnId,
    LogicalTurnLifecycleEvent, LogicalTurnLifecycleEventKind, LogicalTurnOperationRecord,
    LogicalTurnOperatorState, LogicalTurnPhase, LogicalTurnProgress,
    LogicalTurnProgressClassification, LogicalTurnRecord, LogicalTurnResolutionAction,
    LogicalTurnYieldReceipt, LogicalTurnYieldRequest, MemoryGovernanceDecisionInput,
    MemoryGovernanceDecisionRecord, MemoryProposalEnvelope, MemoryProposalQuery,
    MemoryProposalRecord, MemorySpaceDescriptor, MessageSlotId, MessageVariantId,
    ModelProviderCredentialLink, ModelProviderCredentialLinkResult, ModelProviderCredentialUnlink,
    ModelProviderQuery, ModelProviderRecord, ModelProviderRefreshImpact,
    ModelProviderRefreshImpactRequest, ModelProviderRefreshMode, ModelProviderRefreshPlan,
    ModelProviderRefreshPlanRequest, ModelProviderRefreshProfileAction, ModelProviderWrite,
    ParentConsumptionPolicy, ProfileId, ProfilePurgeReport, ProfileRegistryRecord,
    ProfileRegistryWrite, ProviderStateAbsenceReason, ProviderStateClearReason, ProviderStateMode,
    ResourceLimits, RunId, RuntimeActivityBegin, RuntimeActivityCensus, RuntimeActivityCensusQuery,
    RuntimeActivityCensusSummary, RuntimeActivityFinding, RuntimeActivityFindingCode,
    RuntimeActivityFinish, RuntimeActivityId, RuntimeActivityKind, RuntimeActivityOwner,
    RuntimeActivityProgress, RuntimeActivityRecord, RuntimeActivityStatus, RuntimeActivityView,
    ServiceCredentialDelete, ServiceCredentialQuery, ServiceCredentialRecord,
    ServiceCredentialWrite, SessionActivityDigest, SessionActivityDigestQuery, SessionConfig,
    SessionId, SessionKind, SessionState, SessionStatus, ShutdownSummary, ToolProfile,
    TurnProjectionId, WorkerPoolCapacityFallbackPolicy, WorkerPoolCapacityRequest,
};
use rusty_crew_core_session::SessionRegistry;
use serde_json::json;
use session_store::{
    load_engine_bootstrap, load_engine_session_configs, save_engine_event, save_engine_session,
    save_engine_session_with_config,
};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use time::format_description::well_known::Rfc3339;
use time::Duration;
use time::OffsetDateTime;

static NEXT_ENGINE_HANDLE: AtomicU64 = AtomicU64::new(1);
static NEXT_QUEUED_MESSAGE: AtomicU64 = AtomicU64::new(1);

const DEFAULT_PROVIDER_WIRE_STATE_TTL_MS: u64 = 24 * 60 * 60 * 1_000;
const MAX_PROVIDER_WIRE_STATE_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1_000;
const DEFAULT_CHAT_READ_MODEL_LIMIT: u32 = 100;
const MAX_CHAT_READ_MODEL_LIMIT: u32 = 500;

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderStateHydration {
    pub state: Option<BrainWakeProviderStateInput>,
    pub absence_reason: Option<ProviderStateAbsenceReason>,
}

#[derive(Debug, Clone)]
pub struct CoreEngine {
    handle: EngineHandle,
    service_instance_id: String,
    config: EngineConfig,
    bus: CoreBus,
    sessions: SessionRegistry,
    store: CoreCoordinationStore,
    body_projector: BodyProjector,
    action_executor: BrainActionExecutor,
    profile_tool_profiles: Arc<Mutex<HashMap<ProfileId, ToolProfile>>>,
    scheduler_tick_lock: Arc<Mutex<()>>,
    github_gate_lock: Arc<Mutex<()>>,
    external_follow_up_lock: Arc<Mutex<()>>,
    agent_route_lifecycle_lock: Arc<Mutex<()>>,
}

fn parse_rfc3339(value: &str) -> CoreResult<OffsetDateTime> {
    OffsetDateTime::parse(value, &Rfc3339).map_err(|error| {
        CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("invalid RFC3339 timestamp {value}: {error}"),
        )
    })
}

#[cfg(test)]
mod tests;
