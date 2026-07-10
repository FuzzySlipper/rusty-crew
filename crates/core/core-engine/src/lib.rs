//! Coordination engine composition.

mod body;
mod body_queue;
mod bootstrap;
mod chat_store;
mod delegation;
mod delegation_store;
mod github_gate_wait;
mod memory_spaces;
mod memory_store;
mod provider_state_store;
mod roleplay_lore_store;
mod roleplay_records_store;
mod runtime_admin_store;
mod scheduler;
mod session_store;
mod sessions;

pub(crate) use body::{add_millis_to_iso, sanitized_clock_key};
pub use delegation::{delegated_agent_id, delegated_session_id};
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
    ApplyRoleplayAlternativeRequest, ApplyRoleplayAlternativeResult, AttachmentQuery,
    AttachmentRecord, AttachmentWrite, BranchAwareSessionMemoryQuery, BranchHeadExpectation,
    ChatEventLogAppend, ChatEventLogEvent, ChatEventLogPage, ChatEventLogQuery, ChatReadModelEvent,
    ChatReadModelEventKind, ChatReadModelPage, ChatReadModelQuery, ChatReadModelSource,
    ChatSessionReadFacts, ChatSessionReadQuery, ChatSessionReadResult, ChatSessionSummaryPage,
    ChatSessionSummaryPageQuery, ChatTranscriptSearchPage, ChatTranscriptSearchQuery,
    ConversationBranchQuery, ConversationBranchRecord, ConversationBranchStateRecord,
    ConversationBranchWrite, ConversationJumpRequest, ConversationJumpResult,
    ConversationSnapshotQuery, ConversationSnapshotRecord, ConversationSnapshotWrite,
    ConversationTreeReadQuery, ConversationTreeReadResult, CoreCoordinationStore,
    CreateChatAttachmentRequest, CreateChatAttachmentResult, CreateChatConversationBranchRequest,
    CreateChatConversationSnapshotRequest, CreateChatConversationSnapshotResult,
    CreateChatDataBankScopeRequest, CreateChatDataBankScopeResult, CreateChatMessageSlotRequest,
    CreateChatMessageSlotResult, CreateChatMessageVariantRequest, CreateChatMessageVariantResult,
    DataBankScopeQuery, DataBankScopeRecord, DataBankScopeWrite, DeleteChatMessageVariantRequest,
    DurableMessageRecord, EnsureActiveChatConversationBranchRequest,
    EnsureActiveChatConversationBranchResult, ExactPage, LoreRecallQuery, LoreRecallResult,
    LoreRecallTraceQuery, LoreRecallTraceRecord, MessageSlotQuery, MessageSlotRecord,
    MessageSlotWrite, MessageVariantQuery, MessageVariantRecord, MessageVariantWrite,
    ProfileMemoryCaps, ProfileMemoryDelete, ProfileMemoryQuery, ProfileMemoryRecord,
    ProfileMemoryReplace, ProfileMemoryTarget, ProfileMemoryWrite, ProfileRegistryQuery,
    ProviderWireStateDiagnostic, ProviderWireStateInvalidationReason, ProviderWireStateKey,
    ProviderWireStateWakeLookup, ProviderWireStateWrite, QueuedMessageRecord, QueuedMessageState,
    RemoveChatAttachmentRequest, RemoveChatDataBankScopeRequest, ReorderChatMessageVariantsRequest,
    RoleplayCharacterQuery, RoleplayCharacterRecord, RoleplayCharacterWrite,
    RoleplayChatLayerRecord, RoleplayChatLayersWrite, RoleplayImportQuery, RoleplayImportRecord,
    RoleplayImportWrite, RoleplayLoreEntryPromotion, RoleplayLoreFactCapture,
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
    AttachmentId, BodyState, BrainAction, BrainActionBatch, BrainEvent, BrainEventEnvelope,
    BrainImplementationRegistration, BrainProviderStateScope, BrainWakeProviderStateInput,
    BrainWakeProviderStateOutput, BrainWakeProviderStateUpdate, CompletionStatus,
    ContextCompactionArtifact, ContextCompactionArtifactQuery, CoreError, CoreErrorKind, CoreEvent,
    CoreResult, DataBankScopeId, DelegatedResourceCleanupReport, DelegatedRunStatus,
    DelegatedSessionRuntimeStatus, DelegationLifecycleEvent, DelegationLifecyclePhase,
    DelegationLineage, DenDataUpdate, EngineHandle, EventReceipt, EventSubscription, ExternalEvent,
    FanOutFailurePolicy, GitHubGateSuspendRequest, GitHubGateTerminalEvent,
    GitHubGateTerminalReceipt, GitHubGateWaitPhase, GitHubGateWaitRecord, GitHubGateWakeResult,
    IsoTimestamp, MemoryGovernanceDecisionInput, MemoryGovernanceDecisionRecord,
    MemoryProposalEnvelope, MemoryProposalQuery, MemoryProposalRecord, MemorySpaceDescriptor,
    MessageSlotId, MessageVariantId, ModelProviderQuery, ModelProviderRecord,
    ModelProviderRefreshImpact, ModelProviderRefreshImpactRequest, ModelProviderRefreshMode,
    ModelProviderRefreshPlan, ModelProviderRefreshPlanRequest, ModelProviderRefreshProfileAction,
    ModelProviderWrite, ParentConsumptionPolicy, ProfileId, ProfilePurgeReport,
    ProfileRegistryRecord, ProfileRegistryWrite, ProviderStateAbsenceReason,
    ProviderStateClearReason, ProviderStateMode, ResourceLimits, RunId, SessionActivityDigest,
    SessionActivityDigestQuery, SessionConfig, SessionId, SessionKind, SessionState, SessionStatus,
    ShutdownSummary, ToolProfile, WorkerPoolCapacityFallbackPolicy, WorkerPoolCapacityRequest,
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
    config: EngineConfig,
    bus: CoreBus,
    sessions: SessionRegistry,
    store: CoreCoordinationStore,
    body_projector: BodyProjector,
    action_executor: BrainActionExecutor,
    profile_tool_profiles: Arc<Mutex<HashMap<ProfileId, ToolProfile>>>,
    scheduler_tick_lock: Arc<Mutex<()>>,
    github_gate_lock: Arc<Mutex<()>>,
}

impl CoreEngine {
    pub fn provider_state_for_wake(
        &self,
        registration: &BrainImplementationRegistration,
        session_id: &SessionId,
    ) -> CoreResult<ProviderStateHydration> {
        let Some(strategy) = &registration.strategy else {
            return Ok(ProviderStateHydration {
                state: None,
                absence_reason: Some(ProviderStateAbsenceReason::NotConfigured),
            });
        };
        match strategy.provider_state.mode {
            ProviderStateMode::Unused => {
                return Ok(ProviderStateHydration {
                    state: None,
                    absence_reason: Some(ProviderStateAbsenceReason::ModuleDoesNotUseState),
                });
            }
            ProviderStateMode::Optional | ProviderStateMode::Required => {}
        }
        let Some(scope) = &registration.provider_state_scope else {
            return self.provider_state_unavailable_for_mode(
                strategy.provider_state.mode.clone(),
                ProviderStateAbsenceReason::NotConfigured,
            );
        };
        let key = provider_wire_state_key(session_id, &strategy.module_id, &strategy.strategy_id);
        let lookup = ProviderWireStateWakeLookup {
            key,
            profile_fingerprint: scope.profile_fingerprint.clone(),
            provider_fingerprint: scope.provider_fingerprint.clone(),
            now: self.now(),
        };
        let loaded = match load_provider_state_for_wake(&self.store, &lookup) {
            Ok(loaded) => loaded,
            Err(error) => {
                if strategy.provider_state.mode == ProviderStateMode::Optional {
                    return Ok(ProviderStateHydration {
                        state: None,
                        absence_reason: Some(ProviderStateAbsenceReason::LoadFailed),
                    });
                }
                return Err(error);
            }
        };
        let Some(record) = loaded.record else {
            return self.provider_state_unavailable_for_mode(
                strategy.provider_state.mode.clone(),
                loaded
                    .absence_reason
                    .unwrap_or(ProviderStateAbsenceReason::Missing),
            );
        };
        Ok(ProviderStateHydration {
            state: Some(BrainWakeProviderStateInput {
                module_id: record.key.module_id,
                strategy_id: record.key.strategy_id,
                profile_fingerprint: record.profile_fingerprint,
                provider_fingerprint: record.provider_fingerprint,
                payload_version: record.payload_version,
                payload: record.payload_json,
                expires_at: record.expires_at,
            }),
            absence_reason: None,
        })
    }

    pub fn apply_provider_state_output(
        &self,
        registration: &BrainImplementationRegistration,
        session_id: &SessionId,
        wake_id: &str,
        output: BrainWakeProviderStateOutput,
    ) -> CoreResult<()> {
        match output {
            BrainWakeProviderStateOutput::Unchanged => Ok(()),
            BrainWakeProviderStateOutput::Replace { state } => {
                self.replace_provider_state(registration, session_id, wake_id, state)
            }
            BrainWakeProviderStateOutput::Clear { reason } => {
                self.clear_provider_state(registration, session_id, reason)
            }
        }
    }

    pub fn provider_wire_state_diagnostics(
        &self,
        limit: u32,
    ) -> CoreResult<Vec<ProviderWireStateDiagnostic>> {
        list_provider_state_store_diagnostics(&self.store, limit)
    }

    fn provider_state_unavailable_for_mode(
        &self,
        mode: ProviderStateMode,
        absence_reason: ProviderStateAbsenceReason,
    ) -> CoreResult<ProviderStateHydration> {
        if mode == ProviderStateMode::Required {
            return Err(CoreError::new(
                CoreErrorKind::BrainUnavailable,
                format!("required provider state unavailable: {absence_reason:?}"),
            ));
        }
        Ok(ProviderStateHydration {
            state: None,
            absence_reason: Some(absence_reason),
        })
    }

    fn replace_provider_state(
        &self,
        registration: &BrainImplementationRegistration,
        session_id: &SessionId,
        wake_id: &str,
        state: BrainWakeProviderStateUpdate,
    ) -> CoreResult<()> {
        let (module_id, strategy_id) = provider_state_registration_key(registration)?;
        if state.module_id != module_id || state.strategy_id != strategy_id {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!(
                    "provider state update targeted {}/{}, registered brain uses {}/{}",
                    state.module_id, state.strategy_id, module_id, strategy_id
                ),
            ));
        }
        if let Some(scope) = &registration.provider_state_scope {
            validate_provider_state_update_scope(&state, scope)?;
        }
        let ttl_ms = state
            .ttl_ms
            .unwrap_or(DEFAULT_PROVIDER_WIRE_STATE_TTL_MS)
            .min(MAX_PROVIDER_WIRE_STATE_TTL_MS);
        let now = self.now();
        let expires_at = add_millis_to_iso(&now, ttl_ms)?;
        save_provider_state_store(
            &self.store,
            &ProviderWireStateWrite {
                key: provider_wire_state_key(session_id, &module_id, &strategy_id),
                profile_fingerprint: state.profile_fingerprint,
                provider_fingerprint: state.provider_fingerprint,
                payload_version: state.payload_version,
                payload_json: state.payload,
                now,
                expires_at: Some(expires_at),
                last_wake_id: Some(wake_id.to_string()),
            },
        )?;
        Ok(())
    }

    fn clear_provider_state(
        &self,
        registration: &BrainImplementationRegistration,
        session_id: &SessionId,
        reason: ProviderStateClearReason,
    ) -> CoreResult<()> {
        let (module_id, strategy_id) = provider_state_registration_key(registration)?;
        let invalidation_reason = match reason {
            ProviderStateClearReason::BrainRequestedClear => {
                ProviderWireStateInvalidationReason::BrainRequestedClear
            }
        };
        clear_provider_state_store(
            &self.store,
            &provider_wire_state_key(session_id, &module_id, &strategy_id),
            &self.now(),
            invalidation_reason,
        )?;
        Ok(())
    }

    pub fn suspend_for_github_gate(
        &self,
        request: GitHubGateSuspendRequest,
    ) -> CoreResult<GitHubGateWaitRecord> {
        let _guard = self.github_gate_lock.lock().map_err(|_| {
            CoreError::new(CoreErrorKind::InternalError, "GitHub gate lock poisoned")
        })?;
        validate_github_gate_suspend(&request)?;
        let session = self.sessions.get_session(&request.session_id)?;
        if session.status == SessionStatus::Archived {
            return Err(CoreError::new(
                CoreErrorKind::SessionExpired,
                format!("session {} is archived", request.session_id),
            ));
        }
        if let Some(existing) = load_github_gate_wait(&self.store, &request.session_id)? {
            if existing.phase == GitHubGateWaitPhase::Waiting
                && existing.gate_id == request.gate_id
                && existing.commit_sha == request.commit_sha
            {
                return Ok(existing);
            }
        }
        let wait = GitHubGateWaitRecord {
            session_id: request.session_id.clone(),
            run_id: request.run_id,
            provider_thread_id: request.provider_thread_id,
            project_id: request.project_id,
            task_id: request.task_id,
            gate_id: request.gate_id,
            commit_sha: request.commit_sha.to_ascii_lowercase(),
            phase: GitHubGateWaitPhase::Waiting,
            terminal_event_id: None,
            created_at: request.now.clone(),
            updated_at: request.now.clone(),
        };
        save_github_gate_wait(&self.store, &wait)?;
        let idle = self.sessions.mark_idle(&request.session_id, request.now)?;
        save_engine_session(&self.store, &idle)?;
        Ok(wait)
    }

    pub fn consume_github_gate_terminal_event(
        &self,
        event: GitHubGateTerminalEvent,
    ) -> CoreResult<GitHubGateTerminalReceipt> {
        let _guard = self.github_gate_lock.lock().map_err(|_| {
            CoreError::new(CoreErrorKind::InternalError, "GitHub gate lock poisoned")
        })?;
        validate_github_gate_terminal_event(&event)?;
        let cursor = load_github_gate_cursor(&self.store)?;
        if event.event_id <= cursor {
            return Ok(GitHubGateTerminalReceipt {
                event_id: event.event_id,
                cursor,
                duplicate: true,
                wake_scheduled: false,
                ignored_reason: Some("event_cursor_already_consumed".to_string()),
                wait: None,
            });
        }
        let matching = list_github_gate_waits(&self.store)?
            .into_iter()
            .find(|wait| {
                wait.phase == GitHubGateWaitPhase::Waiting
                    && wait.gate_id == event.gate_id
                    && wait.commit_sha.eq_ignore_ascii_case(&event.commit_sha)
            });
        let Some(mut wait) = matching else {
            save_github_gate_cursor(&self.store, event.event_id, &event.completed_at)?;
            return Ok(GitHubGateTerminalReceipt {
                event_id: event.event_id,
                cursor: event.event_id,
                duplicate: false,
                wake_scheduled: false,
                ignored_reason: Some("no_current_wait_for_gate_and_sha".to_string()),
                wait: None,
            });
        };
        let session = self.sessions.get_session(&wait.session_id)?;
        if session.status == SessionStatus::Archived {
            wait.phase = GitHubGateWaitPhase::Cancelled;
            wait.terminal_event_id = Some(event.event_id);
            wait.updated_at = event.completed_at.clone();
            save_github_gate_wait(&self.store, &wait)?;
            save_github_gate_cursor(&self.store, event.event_id, &event.completed_at)?;
            return Ok(GitHubGateTerminalReceipt {
                event_id: event.event_id,
                cursor: event.event_id,
                duplicate: false,
                wake_scheduled: false,
                ignored_reason: Some("session_cancelled_or_archived".to_string()),
                wait: Some(wait),
            });
        }
        let result = GitHubGateWakeResult {
            event_id: event.event_id,
            gate_id: event.gate_id,
            commit_sha: event.commit_sha,
            status: event.status,
            terminal_reason: event.terminal_reason,
            summary: event.summary,
            failure_summary: event.failure_summary,
            completed_at: event.completed_at.clone(),
        };
        let body = serde_json::to_string(&serde_json::json!({
            "type": "github_gate_terminal_result",
            "result": result,
        }))
        .map_err(|error| {
            CoreError::new(
                CoreErrorKind::InternalError,
                format!("encode GitHub gate wake result: {error}"),
            )
        })?;
        let state = self.body_projector.project(&wait.session_id)?;
        let ttl_ms = state.delta_policy.queued_message_ttl_ms;
        let message = QueuedMessageRecord {
            message_id: format!("github-gate-event:{}", event.event_id),
            owner_session_id: Some(wait.session_id.clone()),
            owner_agent_id: session.agent_id.clone(),
            message: AgentMessage {
                from: AgentId::new("rusty-crew:review-gate"),
                to: session.agent_id,
                body,
                correlation_id: Some(format!("github-gate-event:{}", event.event_id)),
                projection: None,
            },
            source_sequence: None,
            enqueued_at: event.completed_at.clone(),
            expires_at: add_millis_to_iso(&event.completed_at, ttl_ms as u64)?,
            ttl_ms,
            delivery_attempts: 0,
            state: QueuedMessageState::Pending,
            terminal_at: None,
            state_reason: None,
        };
        self.store.save_queued_message(&message)?;
        wait.phase = GitHubGateWaitPhase::WakeScheduled;
        wait.terminal_event_id = Some(event.event_id);
        wait.updated_at = event.completed_at.clone();
        save_github_gate_wait(&self.store, &wait)?;
        save_github_gate_cursor(&self.store, event.event_id, &event.completed_at)?;
        self.bus.publish(CoreEvent::BrainWakeRequested {
            session_id: wait.session_id.clone(),
        })?;
        Ok(GitHubGateTerminalReceipt {
            event_id: event.event_id,
            cursor: event.event_id,
            duplicate: false,
            wake_scheduled: true,
            ignored_reason: None,
            wait: Some(wait),
        })
    }

    pub fn recover_github_gate_wakes(&self) -> CoreResult<u32> {
        let _guard = self.github_gate_lock.lock().map_err(|_| {
            CoreError::new(CoreErrorKind::InternalError, "GitHub gate lock poisoned")
        })?;
        let mut recovered = 0_u32;
        for wait in list_github_gate_waits(&self.store)? {
            if wait.phase != GitHubGateWaitPhase::WakeScheduled {
                continue;
            }
            let session = self.sessions.get_session(&wait.session_id)?;
            if session.status == SessionStatus::Archived {
                continue;
            }
            self.bus.publish(CoreEvent::BrainWakeRequested {
                session_id: wait.session_id,
            })?;
            recovered += 1;
        }
        Ok(recovered)
    }

    pub fn github_gate_wait(
        &self,
        session_id: &SessionId,
    ) -> CoreResult<Option<GitHubGateWaitRecord>> {
        load_github_gate_wait(&self.store, session_id)
    }

    pub fn github_gate_event_cursor(&self) -> CoreResult<u64> {
        load_github_gate_cursor(&self.store)
    }

    pub fn register_profile_tool_profile(
        &self,
        profile_id: ProfileId,
        tool_profile: ToolProfile,
    ) -> CoreResult<()> {
        validate_tool_profile(&tool_profile)?;
        self.profile_tool_profiles
            .lock()
            .map_err(|_| {
                CoreError::new(
                    CoreErrorKind::InternalError,
                    "profile registry lock poisoned",
                )
            })?
            .insert(profile_id, tool_profile);
        Ok(())
    }

    pub fn unregister_profile_tool_profile(&self, profile_id: &ProfileId) -> CoreResult<()> {
        self.profile_tool_profiles
            .lock()
            .map_err(|_| {
                CoreError::new(
                    CoreErrorKind::InternalError,
                    "profile registry lock poisoned",
                )
            })?
            .remove(profile_id);
        Ok(())
    }

    pub fn route_agent_message(&self, message: AgentMessage) -> CoreResult<EventReceipt> {
        let event = CoreEvent::AgentMessageRouted { message };
        let sequence = self.bus.publish(event.clone())?;
        self.schedule_wake_for_event(&event)?;
        Ok(EventReceipt {
            accepted: true,
            sequence,
        })
    }

    pub fn execute_brain_actions(&self, batch: BrainActionBatch) -> CoreResult<ActionBatchReceipt> {
        let session = self.sessions.get_session(&batch.session_id)?;
        if session.status == SessionStatus::Archived {
            return Err(CoreError::new(
                CoreErrorKind::SessionExpired,
                format!("session {} is archived", batch.session_id),
            ));
        }

        let rejected_actions = self.action_executor.validate(&batch);
        if !rejected_actions.is_empty() {
            return Ok(ActionBatchReceipt {
                wake_id: batch.wake_id,
                accepted_actions: 0,
                rejected_actions,
            });
        }

        let rejected_actions = self.validate_delegation_invariants(&session, &batch);
        if !rejected_actions.is_empty() {
            return Ok(ActionBatchReceipt {
                wake_id: batch.wake_id,
                accepted_actions: 0,
                rejected_actions,
            });
        }

        let rejected_actions = self.validate_fan_out_invariants(&batch);
        if !rejected_actions.is_empty() {
            return Ok(ActionBatchReceipt {
                wake_id: batch.wake_id,
                accepted_actions: 0,
                rejected_actions,
            });
        }

        self.spawn_delegated_workers(&session, &batch)?;
        let receipt = self.action_executor.execute(batch.clone())?;
        self.update_lifecycle_for_actions(&batch)?;
        self.apply_fan_out_failure_policy(&batch)?;
        self.schedule_parent_completion_wakes(&batch)?;
        Ok(receipt)
    }

    pub fn submit_brain_event(&self, envelope: BrainEventEnvelope) -> CoreResult<EventReceipt> {
        if matches!(envelope.event, BrainEvent::Started) {
            update_delegated_worker_run_status_by_session(
                &self.store,
                &envelope.session_id,
                WorkerRunStatus::Running,
                self.now(),
            )?;
        }
        let sequence = self.bus.publish(CoreEvent::BrainEventObserved {
            session_id: envelope.session_id,
            wake_id: Some(envelope.wake_id),
            event: envelope.event,
        })?;
        Ok(EventReceipt {
            accepted: true,
            sequence,
        })
    }

    pub fn inject_external_event(&self, event: ExternalEvent) -> CoreResult<EventReceipt> {
        let event = CoreEvent::ExternalEventInjected { event };
        let sequence = self.bus.publish(event.clone())?;
        self.schedule_wake_for_event(&event)?;
        Ok(EventReceipt {
            accepted: true,
            sequence,
        })
    }

    pub fn inject_den_data_update(&self, update: DenDataUpdate) -> CoreResult<EventReceipt> {
        let event = CoreEvent::DenDataUpdated { update };
        let sequence = self.bus.publish(event.clone())?;
        self.schedule_wake_for_event(&event)?;
        Ok(EventReceipt {
            accepted: true,
            sequence,
        })
    }

    pub fn count_rows(&self, table: &str) -> CoreResult<u64> {
        RuntimeStorageAdminStore::count_rows(&self.store, table)
    }

    pub fn database_size(&self) -> CoreResult<RuntimeDatabaseSize> {
        RuntimeStorageAdminStore::database_size(&self.store)
    }

    pub fn storage_diagnostics(&self) -> CoreResult<RuntimeStorageDiagnostics> {
        RuntimeStorageAdminStore::storage_diagnostics(&self.store)
    }

    pub fn storage_schema(&self) -> CoreResult<RuntimeModuleSchemaRegistryDiagnostics> {
        RuntimeStorageAdminStore::storage_schema(&self.store)
    }

    pub fn list_profile_registry_records(
        &self,
        query: &ProfileRegistryQuery,
    ) -> CoreResult<Vec<ProfileRegistryRecord>> {
        RuntimeServiceDataStore::list_profile_registry_records(&self.store, query)
    }

    pub fn create_profile_registry_record(
        &self,
        write: &ProfileRegistryWrite,
    ) -> CoreResult<ProfileRegistryRecord> {
        RuntimeServiceDataStore::create_profile_registry_record(&self.store, write)
    }

    pub fn update_profile_registry_record(
        &self,
        update: &rusty_crew_core_protocol::ProfileRegistryUpdate,
    ) -> CoreResult<ProfileRegistryRecord> {
        RuntimeServiceDataStore::update_profile_registry_record(&self.store, update)
    }

    pub fn get_profile_registry_record(
        &self,
        profile_id: &ProfileId,
    ) -> CoreResult<Option<ProfileRegistryRecord>> {
        RuntimeServiceDataStore::get_profile_registry_record(&self.store, profile_id)
    }

    pub fn purge_profile(&self, profile_id: &ProfileId) -> CoreResult<ProfilePurgeReport> {
        let removed_sessions = self.sessions.remove_sessions_for_profile(profile_id)?;
        self.profile_tool_profiles
            .lock()
            .map_err(|_| {
                CoreError::new(
                    CoreErrorKind::InternalError,
                    "profile tool profiles lock poisoned",
                )
            })?
            .remove(profile_id);
        let mut report = RuntimeServiceDataStore::purge_profile(&self.store, profile_id)?;
        for state in removed_sessions {
            if !report
                .session_ids
                .iter()
                .any(|session_id| session_id == &state.session_id)
            {
                report.session_ids.push(state.session_id);
            }
            if !report
                .agent_ids
                .iter()
                .any(|agent_id| agent_id == &state.agent_id)
            {
                report.agent_ids.push(state.agent_id);
            }
        }
        Ok(report)
    }

    pub fn upsert_model_provider(
        &self,
        write: &ModelProviderWrite,
    ) -> CoreResult<ModelProviderRecord> {
        RuntimeServiceDataStore::upsert_model_provider(&self.store, write)
    }

    pub fn get_model_provider(&self, alias: &str) -> CoreResult<Option<ModelProviderRecord>> {
        RuntimeServiceDataStore::get_model_provider(&self.store, alias)
    }

    pub fn get_model_provider_secret(&self, alias: &str) -> CoreResult<Option<String>> {
        RuntimeServiceDataStore::get_model_provider_secret(&self.store, alias)
    }

    pub fn list_model_providers(
        &self,
        query: &ModelProviderQuery,
    ) -> CoreResult<Vec<ModelProviderRecord>> {
        RuntimeServiceDataStore::list_model_providers(&self.store, query)
    }

    pub fn model_provider_refresh_impact(
        &self,
        request: &ModelProviderRefreshImpactRequest,
    ) -> CoreResult<ModelProviderRefreshImpact> {
        let provider_alias = request.provider_alias.trim();
        if provider_alias.is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "model provider refresh impact provider_alias is required",
            ));
        }

        let profiles = self
            .store
            .service_data()
            .list_profile_registry_records(&ProfileRegistryQuery::default())?;
        let sessions = self.sessions.all_sessions()?;
        let mut affected_profiles = Vec::new();

        for profile in profiles {
            if profile_registry_provider_alias(&profile).as_deref() != Some(provider_alias) {
                continue;
            }

            let configured_session_ids = profile
                .derived_runtime_refs
                .iter()
                .filter(|runtime_ref| {
                    runtime_ref.ref_kind == "session"
                        && runtime_ref.status != "archived"
                        && runtime_ref.status != "disabled"
                })
                .map(|runtime_ref| SessionId::new(runtime_ref.ref_id.clone()))
                .collect::<HashSet<_>>();
            let active_session_ids = sessions
                .iter()
                .filter(|session| {
                    session.profile_id == profile.profile_id
                        && session.status != SessionStatus::Archived
                })
                .map(|session| session.session_id.clone())
                .collect::<HashSet<_>>();
            let mut session_ids = configured_session_ids
                .union(&active_session_ids)
                .cloned()
                .collect::<Vec<_>>();
            session_ids.sort_by(|left, right| left.0.cmp(&right.0));
            let mut configured_session_ids = configured_session_ids.into_iter().collect::<Vec<_>>();
            configured_session_ids.sort_by(|left, right| left.0.cmp(&right.0));
            let mut active_session_ids = active_session_ids.into_iter().collect::<Vec<_>>();
            active_session_ids.sort_by(|left, right| left.0.cmp(&right.0));

            affected_profiles.push(rusty_crew_core_protocol::ModelProviderAffectedProfile {
                profile_id: profile.profile_id,
                session_ids,
                configured_session_ids,
                active_session_ids,
            });
        }

        affected_profiles.sort_by(|left, right| left.profile_id.0.cmp(&right.profile_id.0));

        Ok(ModelProviderRefreshImpact {
            provider_alias: provider_alias.to_string(),
            affected_profiles,
        })
    }

    pub fn plan_model_provider_refresh(
        &self,
        request: &ModelProviderRefreshPlanRequest,
    ) -> CoreResult<ModelProviderRefreshPlan> {
        let impact = self.model_provider_refresh_impact(&ModelProviderRefreshImpactRequest {
            provider_alias: request.provider_alias.clone(),
        })?;
        let command_name = match request.mode {
            ModelProviderRefreshMode::None => None,
            ModelProviderRefreshMode::Plan => Some("plan_runtime_rebuild"),
            ModelProviderRefreshMode::Apply => Some("apply_runtime_rebuild"),
        };
        let actions = command_name
            .map(|command_name| {
                impact
                    .affected_profiles
                    .iter()
                    .map(|affected| {
                        let profile_id = affected.profile_id.to_string();
                        ModelProviderRefreshProfileAction {
                            profile_id: affected.profile_id.clone(),
                            command_name: command_name.to_string(),
                            reason: format!("model provider {} updated", impact.provider_alias),
                            planned_summary: format!(
                                "runtime rebuild plan prepared for profile {profile_id}"
                            ),
                            applied_summary: format!(
                                "runtime rebuild applied for profile {profile_id}"
                            ),
                            blocked_summary: format!(
                                "runtime rebuild blocked for profile {profile_id}"
                            ),
                            failure_reason_code: "model_provider_refresh_failed".to_string(),
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        Ok(ModelProviderRefreshPlan {
            provider_alias: impact.provider_alias,
            mode: request.mode.clone(),
            affected_profiles: impact.affected_profiles,
            actions,
        })
    }

    pub fn put_roleplay_character(
        &self,
        write: &RoleplayCharacterWrite,
    ) -> CoreResult<RoleplayCharacterRecord> {
        RoleplayRecordsStore::put_character(&self.store, write)
    }
    pub fn get_roleplay_character(&self, id: &str) -> CoreResult<Option<RoleplayCharacterRecord>> {
        RoleplayRecordsStore::get_character(&self.store, id)
    }
    pub fn list_roleplay_characters(
        &self,
        query: &RoleplayCharacterQuery,
    ) -> CoreResult<Vec<RoleplayCharacterRecord>> {
        RoleplayRecordsStore::list_characters(&self.store, query)
    }
    pub fn put_roleplay_player_persona(
        &self,
        write: &RoleplayPlayerPersonaWrite,
    ) -> CoreResult<RoleplayPlayerPersonaRecord> {
        RoleplayRecordsStore::put_persona(&self.store, write)
    }
    pub fn get_roleplay_player_persona(
        &self,
        id: &str,
    ) -> CoreResult<Option<RoleplayPlayerPersonaRecord>> {
        RoleplayRecordsStore::get_persona(&self.store, id)
    }
    pub fn list_roleplay_player_personas(
        &self,
        query: &RoleplayPlayerPersonaQuery,
    ) -> CoreResult<Vec<RoleplayPlayerPersonaRecord>> {
        RoleplayRecordsStore::list_personas(&self.store, query)
    }
    pub fn put_roleplay_session_metadata(
        &self,
        write: &RoleplaySessionMetadataWrite,
    ) -> CoreResult<RoleplaySessionMetadataRecord> {
        RoleplayRecordsStore::put_session_metadata(&self.store, write)
    }
    pub fn get_roleplay_session_metadata(
        &self,
        id: &str,
    ) -> CoreResult<Option<RoleplaySessionMetadataRecord>> {
        RoleplayRecordsStore::get_session_metadata(&self.store, id)
    }
    pub fn list_roleplay_session_metadata(
        &self,
        query: &RoleplaySessionMetadataQuery,
    ) -> CoreResult<Vec<RoleplaySessionMetadataRecord>> {
        RoleplayRecordsStore::list_session_metadata(&self.store, query)
    }
    pub fn apply_roleplay_session_projection(
        &self,
        write: &RoleplaySessionProjectionWrite,
    ) -> CoreResult<RoleplaySessionProjectionRecord> {
        RoleplayRecordsStore::apply_session_projection(&self.store, write)
    }
    pub fn put_roleplay_import(
        &self,
        write: &RoleplayImportWrite,
    ) -> CoreResult<RoleplayImportRecord> {
        RoleplayRecordsStore::put_import(&self.store, write)
    }
    pub fn get_roleplay_import(&self, id: &str) -> CoreResult<Option<RoleplayImportRecord>> {
        RoleplayRecordsStore::get_import(&self.store, id)
    }
    pub fn list_roleplay_imports(
        &self,
        query: &RoleplayImportQuery,
    ) -> CoreResult<Vec<RoleplayImportRecord>> {
        RoleplayRecordsStore::list_imports(&self.store, query)
    }

    pub fn add_roleplay_lore_record(
        &self,
        write: &RoleplayLoreWrite,
    ) -> CoreResult<RoleplayLoreRecord> {
        RoleplayLoreStore::add_lore_record(&self.store, write)
    }

    pub fn replace_roleplay_lore_record(
        &self,
        replace: &RoleplayLoreReplace,
    ) -> CoreResult<RoleplayLoreRecord> {
        RoleplayLoreStore::replace_lore_record(&self.store, replace)
    }

    pub fn supersede_roleplay_lore_record(
        &self,
        supersede: &RoleplayLoreSupersede,
    ) -> CoreResult<(RoleplayLoreRecord, RoleplayLoreRecord)> {
        RoleplayLoreStore::supersede_lore_record(&self.store, supersede)
    }

    pub fn tombstone_roleplay_lore_record(
        &self,
        tombstone: &RoleplayLoreTombstone,
    ) -> CoreResult<RoleplayLoreRecord> {
        RoleplayLoreStore::tombstone_lore_record(&self.store, tombstone)
    }

    pub fn query_roleplay_lore_records(
        &self,
        query: &RoleplayLoreQuery,
    ) -> CoreResult<Vec<RoleplayLoreRecord>> {
        RoleplayLoreStore::query_lore_records(&self.store, query)
    }

    pub fn get_roleplay_lore_record(
        &self,
        record_id: &str,
    ) -> CoreResult<Option<RoleplayLoreRecord>> {
        RoleplayLoreStore::get_lore_record(&self.store, record_id)
    }

    pub fn roleplay_lore_provenance_events(
        &self,
        record_id: &str,
    ) -> CoreResult<Vec<RoleplayLoreProvenanceEvent>> {
        RoleplayLoreStore::lore_provenance_events(&self.store, record_id)
    }

    pub fn create_lore_layer(
        &self,
        write: &RoleplayLoreLayerWrite,
    ) -> CoreResult<RoleplayLoreLayerRecord> {
        RoleplayLoreStore::create_lore_layer(&self.store, write)
    }

    pub fn get_lore_layer(&self, layer_id: &str) -> CoreResult<Option<RoleplayLoreLayerRecord>> {
        RoleplayLoreStore::get_lore_layer(&self.store, layer_id)
    }

    pub fn list_lore_layers_by_profile(
        &self,
        profile_id: &str,
    ) -> CoreResult<Vec<RoleplayLoreLayerRecord>> {
        RoleplayLoreStore::list_lore_layers_by_profile(&self.store, profile_id)
    }

    pub fn update_lore_layer(
        &self,
        update: &RoleplayLoreLayerUpdate,
    ) -> CoreResult<RoleplayLoreLayerRecord> {
        RoleplayLoreStore::update_lore_layer(&self.store, update)
    }

    pub fn archive_lore_layer(
        &self,
        archive: &RoleplayLoreLayerArchive,
    ) -> CoreResult<RoleplayLoreLayerRecord> {
        RoleplayLoreStore::archive_lore_layer(&self.store, archive)
    }

    pub fn get_lore_layer_config(
        &self,
        layer_id: &str,
    ) -> CoreResult<Option<RoleplayLoreLayerConfigRecord>> {
        RoleplayLoreStore::get_lore_layer_config(&self.store, layer_id)
    }

    pub fn set_lore_layer_config(
        &self,
        write: &RoleplayLoreLayerConfigWrite,
    ) -> CoreResult<RoleplayLoreLayerConfigRecord> {
        RoleplayLoreStore::set_lore_layer_config(&self.store, write)
    }

    pub fn add_entry_to_layer(&self, link: &RoleplayLoreLayerEntryLink) -> CoreResult<()> {
        RoleplayLoreStore::add_entry_to_layer(&self.store, link)
    }

    pub fn capture_lore_fact(
        &self,
        capture: &RoleplayLoreFactCapture,
    ) -> CoreResult<RoleplayLoreLayerEntryJoin> {
        RoleplayLoreStore::capture_lore_fact(&self.store, capture)
    }

    pub fn promote_lore_entry(
        &self,
        promotion: &RoleplayLoreEntryPromotion,
    ) -> CoreResult<RoleplayLoreLayerEntryJoin> {
        RoleplayLoreStore::promote_lore_entry(&self.store, promotion)
    }

    pub fn remove_entry_from_layer(&self, layer_id: &str, record_id: &str) -> CoreResult<()> {
        RoleplayLoreStore::remove_entry_from_layer(&self.store, layer_id, record_id)
    }

    pub fn set_entry_constant(
        &self,
        layer_id: &str,
        record_id: &str,
        is_constant: bool,
    ) -> CoreResult<()> {
        RoleplayLoreStore::set_entry_constant(&self.store, layer_id, record_id, is_constant)
    }

    pub fn list_entries_by_layer(
        &self,
        layer_id: &str,
    ) -> CoreResult<Vec<RoleplayLoreLayerEntryJoin>> {
        RoleplayLoreStore::list_entries_by_layer(&self.store, layer_id)
    }

    pub fn set_chat_layers(&self, write: &RoleplayChatLayersWrite) -> CoreResult<()> {
        RoleplayLoreStore::set_chat_layers(&self.store, write)
    }

    pub fn get_chat_layers(&self, chat_id: &str) -> CoreResult<Vec<RoleplayChatLayerRecord>> {
        RoleplayLoreStore::get_chat_layers(&self.store, chat_id)
    }

    pub fn toggle_chat_layer(
        &self,
        chat_id: &str,
        layer_id: &str,
        enabled: bool,
    ) -> CoreResult<()> {
        RoleplayLoreStore::toggle_chat_layer(&self.store, chat_id, layer_id, enabled)
    }

    pub fn reorder_chat_layers(&self, chat_id: &str, layer_ids: &[String]) -> CoreResult<()> {
        RoleplayLoreStore::reorder_chat_layers(&self.store, chat_id, layer_ids)
    }

    pub fn recall_lore(&self, query: &LoreRecallQuery) -> CoreResult<LoreRecallResult> {
        RoleplayLoreStore::recall_lore(&self.store, query)
    }

    pub fn list_recall_traces(
        &self,
        query: &LoreRecallTraceQuery,
    ) -> CoreResult<Vec<LoreRecallTraceRecord>> {
        RoleplayLoreStore::list_recall_traces(&self.store, query)
    }

    pub fn get_recall_trace(&self, trace_id: &str) -> CoreResult<Option<LoreRecallTraceRecord>> {
        RoleplayLoreStore::get_recall_trace(&self.store, trace_id)
    }

    pub fn list_simple_kv(&self, query: &SimpleKvQuery) -> CoreResult<Vec<SimpleKvRecord>> {
        RuntimeModuleDataStore::list_simple_kv(&self.store, query)
    }

    pub fn put_simple_kv(&self, write: &SimpleKvWrite) -> CoreResult<SimpleKvRecord> {
        RuntimeModuleDataStore::put_simple_kv(&self.store, write)
    }

    pub fn delete_simple_kv(&self, delete: &SimpleKvDelete) -> CoreResult<SimpleKvRecord> {
        RuntimeModuleDataStore::delete_simple_kv(&self.store, delete)
    }

    pub fn run_maintenance(
        &self,
        policy: &RuntimeMaintenancePolicy,
    ) -> CoreResult<RuntimeMaintenanceReport> {
        RuntimeStorageAdminStore::run_maintenance(&self.store, policy)
    }

    pub fn save_message_slot(&self, slot: &MessageSlotWrite) -> CoreResult<()> {
        self.store.save_chat_message_slot(slot)
    }

    pub fn save_message_variant(
        &self,
        variant: &MessageVariantWrite,
    ) -> CoreResult<MessageVariantRecord> {
        self.store.save_chat_message_variant(variant)
    }

    pub fn create_chat_message_slot(
        &self,
        request: &CreateChatMessageSlotRequest,
    ) -> CoreResult<CreateChatMessageSlotResult> {
        ChatConversationStore::create_chat_message_slot(&self.store, request)
    }

    pub fn create_chat_message_variant(
        &self,
        request: &CreateChatMessageVariantRequest,
    ) -> CoreResult<CreateChatMessageVariantResult> {
        ChatConversationStore::create_chat_message_variant(&self.store, request)
    }
    pub fn apply_roleplay_alternative(
        &self,
        request: &ApplyRoleplayAlternativeRequest,
    ) -> CoreResult<ApplyRoleplayAlternativeResult> {
        ChatConversationStore::apply_roleplay_alternative(&self.store, request)
    }

    pub fn delete_chat_message_variant(
        &self,
        request: &DeleteChatMessageVariantRequest,
    ) -> CoreResult<MessageSlotRecord> {
        ChatConversationStore::delete_chat_message_variant(&self.store, request)
    }

    pub fn reorder_chat_message_variants(
        &self,
        request: &ReorderChatMessageVariantsRequest,
    ) -> CoreResult<Vec<MessageVariantRecord>> {
        ChatConversationStore::reorder_chat_message_variants(&self.store, request)
    }

    pub fn query_message_slots(
        &self,
        query: &MessageSlotQuery,
    ) -> CoreResult<Vec<MessageSlotRecord>> {
        self.store.query_chat_message_slots(query)
    }

    pub fn query_message_slots_page(
        &self,
        query: &MessageSlotQuery,
    ) -> CoreResult<ExactPage<MessageSlotRecord>> {
        self.store.query_chat_message_slots_page(query)
    }

    pub fn query_message_variants(
        &self,
        query: &MessageVariantQuery,
    ) -> CoreResult<Vec<MessageVariantRecord>> {
        self.store.query_chat_message_variants(query)
    }

    pub fn query_message_variants_page(
        &self,
        query: &SessionMessageVariantPageQuery,
    ) -> CoreResult<ExactPage<MessageVariantRecord>> {
        self.store.query_chat_message_variants_page(query)
    }

    pub fn chat_read_model_page(
        &self,
        query: &ChatReadModelQuery,
    ) -> CoreResult<ChatReadModelPage> {
        let after = chat_cursor_sequence(query.cursor.as_deref(), &query.session_id);
        let limit = normalize_chat_read_model_limit(query.limit);
        let offset = after.min(u32::MAX as u64) as u32;
        let slots = self
            .store
            .query_chat_message_slots_page(&MessageSlotQuery {
                session_id: Some(query.session_id.clone()),
                include_alternates: true,
                page: Some(rusty_crew_core_persistence::QueryPage {
                    limit: Some(limit.max(1)),
                    offset: Some(offset),
                }),
            })?;
        if slots.total > 0 {
            let items = slots
                .items
                .into_iter()
                .take(limit as usize)
                .enumerate()
                .map(|(index, slot)| {
                    chat_read_model_event_from_slot(
                        &query.session_id,
                        &query.agent_id,
                        after + index as u64 + 1,
                        &slot,
                    )
                })
                .collect::<Vec<_>>();
            let latest_sequence = items.last().map(|event| event.sequence_id).unwrap_or(after);
            return Ok(ChatReadModelPage {
                items,
                latest_cursor: chat_cursor_for(&query.session_id, latest_sequence),
                has_more: u64::from(offset).saturating_add(u64::from(limit)) < slots.total,
                total: slots.total,
                source: ChatReadModelSource::MessageSlots,
            });
        }

        let body = self.project_body_state(&query.session_id)?;
        let total = body.pending_messages.len() as u64;
        let session = body.session;
        let created_at = self.now();
        let items = body
            .pending_messages
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .enumerate()
            .map(|(index, message)| {
                pending_message_event(
                    &session,
                    after + index as u64 + 1,
                    &message,
                    created_at.clone(),
                )
            })
            .collect::<Vec<_>>();
        let latest_sequence = items.last().map(|event| event.sequence_id).unwrap_or(after);
        Ok(ChatReadModelPage {
            items,
            latest_cursor: chat_cursor_for(&query.session_id, latest_sequence),
            has_more: u64::from(offset).saturating_add(u64::from(limit)) < total,
            total,
            source: if total == 0 {
                ChatReadModelSource::Empty
            } else {
                ChatReadModelSource::PendingMessages
            },
        })
    }

    pub fn read_chat_session(
        &self,
        query: &ChatSessionReadQuery,
    ) -> CoreResult<ChatSessionReadResult> {
        let session = self.get_session(&query.session_id)?;
        let event_page = self.query_chat_events(&ChatEventLogQuery {
            session_id: query.session_id.clone(),
            cursor: query.cursor.clone(),
            limit: Some(query.limit),
        })?;
        let message_slots = self.query_message_slots_page(&MessageSlotQuery {
            session_id: Some(query.session_id.clone()),
            include_alternates: query.include_alternates,
            page: Some(rusty_crew_core_persistence::QueryPage {
                limit: Some(query.limit.max(1)),
                offset: Some(0),
            }),
        })?;
        if event_page.total > 0 {
            return Ok(ChatSessionReadResult {
                session,
                events: event_page.items,
                latest_cursor: event_page.latest_cursor,
                has_more: event_page.has_more,
                has_more_before: event_page.has_more_before,
                total: event_page.total,
                message_count: event_page.message_count,
                source: ChatReadModelSource::EventLog,
                message_slots,
            });
        }
        let read_model = self.chat_read_model_page(&ChatReadModelQuery {
            session_id: query.session_id.clone(),
            agent_id: session.agent_id.to_string(),
            cursor: query.cursor.clone(),
            limit: Some(query.limit),
        })?;
        Ok(ChatSessionReadResult {
            session,
            events: read_model
                .items
                .into_iter()
                .map(chat_read_model_event_as_log_event)
                .collect(),
            latest_cursor: read_model.latest_cursor,
            has_more: read_model.has_more,
            has_more_before: false,
            total: read_model.total,
            message_count: read_model.total,
            source: read_model.source,
            message_slots,
        })
    }

    pub fn query_chat_session_summaries(
        &self,
        query: &ChatSessionSummaryPageQuery,
    ) -> CoreResult<ChatSessionSummaryPage> {
        let mut sessions = self.list_sessions()?;
        sessions.retain(|session| {
            query
                .profile_id
                .as_ref()
                .is_none_or(|profile_id| &session.profile_id == profile_id)
                && query
                    .status
                    .as_deref()
                    .is_none_or(|status| session_status_wire_value(&session.status) == status)
        });
        sessions.sort_by(|left, right| left.session_id.0.cmp(&right.session_id.0));
        let total = sessions.len() as u64;
        let limit = query.page.limit.unwrap_or(100).clamp(1, 500);
        let offset = query.page.offset.unwrap_or(0);
        let items = sessions
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .map(|session| {
                let event_page = self.query_chat_events(&ChatEventLogQuery {
                    session_id: session.session_id.clone(),
                    cursor: None,
                    limit: Some(0),
                })?;
                if event_page.total > 0 {
                    return Ok(ChatSessionReadFacts {
                        session,
                        message_count: event_page.message_count,
                        latest_cursor: event_page.latest_cursor,
                        source: ChatReadModelSource::EventLog,
                    });
                }
                let slot_page = self.query_message_slots_page(&MessageSlotQuery {
                    session_id: Some(session.session_id.clone()),
                    include_alternates: false,
                    page: Some(rusty_crew_core_persistence::QueryPage {
                        limit: Some(1),
                        offset: Some(0),
                    }),
                })?;
                if slot_page.total > 0 {
                    return Ok(ChatSessionReadFacts {
                        latest_cursor: chat_cursor_for(&session.session_id, slot_page.total),
                        session,
                        message_count: slot_page.total,
                        source: ChatReadModelSource::MessageSlots,
                    });
                }
                let pending = self
                    .project_body_state(&session.session_id)?
                    .pending_messages;
                let message_count = pending.len() as u64;
                Ok(ChatSessionReadFacts {
                    latest_cursor: chat_cursor_for(&session.session_id, message_count),
                    session,
                    message_count,
                    source: if message_count == 0 {
                        ChatReadModelSource::Empty
                    } else {
                        ChatReadModelSource::PendingMessages
                    },
                })
            })
            .collect::<CoreResult<Vec<_>>>()?;
        Ok(ChatSessionSummaryPage {
            page: ExactPage::new(items, total, limit, offset),
        })
    }

    pub fn append_chat_event(&self, event: &ChatEventLogAppend) -> CoreResult<ChatEventLogEvent> {
        self.store.append_chat_event_log(event)
    }

    pub fn query_chat_events(&self, query: &ChatEventLogQuery) -> CoreResult<ChatEventLogPage> {
        self.store.query_chat_event_log(query)
    }

    pub fn save_conversation_branch(
        &self,
        branch: &ConversationBranchWrite,
    ) -> CoreResult<ConversationBranchRecord> {
        self.store.save_chat_conversation_branch(branch)
    }

    pub fn query_conversation_branches(
        &self,
        query: &ConversationBranchQuery,
    ) -> CoreResult<Vec<ConversationBranchRecord>> {
        self.store.query_chat_conversation_branches(query)
    }

    pub fn create_chat_conversation_branch(
        &self,
        request: &CreateChatConversationBranchRequest,
    ) -> CoreResult<ConversationBranchRecord> {
        ChatConversationStore::create_chat_conversation_branch(&self.store, request)
    }

    pub fn ensure_active_chat_conversation_branch(
        &self,
        request: &EnsureActiveChatConversationBranchRequest,
    ) -> CoreResult<EnsureActiveChatConversationBranchResult> {
        ChatConversationStore::ensure_active_chat_conversation_branch(&self.store, request)
    }

    pub fn get_conversation_branch_state(
        &self,
        session_id: &SessionId,
        default_updated_at: &IsoTimestamp,
    ) -> CoreResult<ConversationBranchStateRecord> {
        self.store
            .get_chat_conversation_branch_state(session_id, default_updated_at)
    }

    pub fn select_active_conversation_branch(
        &self,
        request: &SelectActiveBranchRequest,
    ) -> CoreResult<SelectActiveBranchResult> {
        self.store.select_active_chat_conversation_branch(request)
    }

    pub fn update_conversation_branch_head(
        &self,
        request: &UpdateBranchHeadRequest,
    ) -> CoreResult<UpdateBranchHeadResult> {
        self.store.update_chat_conversation_branch_head(request)
    }

    pub fn save_conversation_snapshot(
        &self,
        snapshot: &ConversationSnapshotWrite,
    ) -> CoreResult<ConversationSnapshotRecord> {
        self.store.save_chat_conversation_snapshot(snapshot)
    }

    pub fn create_chat_conversation_snapshot(
        &self,
        request: &CreateChatConversationSnapshotRequest,
    ) -> CoreResult<CreateChatConversationSnapshotResult> {
        ChatConversationStore::create_chat_conversation_snapshot(&self.store, request)
    }

    pub fn query_conversation_snapshots(
        &self,
        query: &ConversationSnapshotQuery,
    ) -> CoreResult<Vec<ConversationSnapshotRecord>> {
        self.store.query_chat_conversation_snapshots(query)
    }

    pub fn read_conversation_tree(
        &self,
        query: &ConversationTreeReadQuery,
    ) -> CoreResult<ConversationTreeReadResult> {
        self.store.read_chat_conversation_tree(query)
    }

    pub fn search_chat_transcript(
        &self,
        query: &ChatTranscriptSearchQuery,
    ) -> CoreResult<ChatTranscriptSearchPage> {
        ChatConversationStore::search_chat_transcript(&self.store, query)
    }

    pub fn resolve_conversation_jump(
        &self,
        request: &ConversationJumpRequest,
    ) -> CoreResult<ConversationJumpResult> {
        self.store.resolve_chat_conversation_jump(request)
    }

    pub fn save_attachment(&self, attachment: &AttachmentWrite) -> CoreResult<AttachmentRecord> {
        self.store.save_chat_attachment(attachment)
    }

    pub fn create_chat_attachment(
        &self,
        request: &CreateChatAttachmentRequest,
    ) -> CoreResult<CreateChatAttachmentResult> {
        ChatConversationStore::create_chat_attachment(&self.store, request)
    }

    pub fn query_attachments(&self, query: &AttachmentQuery) -> CoreResult<Vec<AttachmentRecord>> {
        self.store.query_chat_attachments(query)
    }

    pub fn query_attachments_page(
        &self,
        query: &AttachmentQuery,
    ) -> CoreResult<ExactPage<AttachmentRecord>> {
        self.store.query_chat_attachments_page(query)
    }

    pub fn remove_attachment(
        &self,
        attachment_id: &AttachmentId,
        updated_at: &IsoTimestamp,
    ) -> CoreResult<AttachmentRecord> {
        ChatConversationStore::remove_attachment(&self.store, attachment_id, updated_at)
    }

    pub fn remove_chat_attachment(
        &self,
        request: &RemoveChatAttachmentRequest,
    ) -> CoreResult<AttachmentRecord> {
        ChatConversationStore::remove_chat_attachment(&self.store, request)
    }

    pub fn save_data_bank_scope(
        &self,
        scope: &DataBankScopeWrite,
    ) -> CoreResult<DataBankScopeRecord> {
        self.store.save_chat_data_bank_scope(scope)
    }

    pub fn create_chat_data_bank_scope(
        &self,
        request: &CreateChatDataBankScopeRequest,
    ) -> CoreResult<CreateChatDataBankScopeResult> {
        ChatConversationStore::create_chat_data_bank_scope(&self.store, request)
    }

    pub fn query_data_bank_scopes(
        &self,
        query: &DataBankScopeQuery,
    ) -> CoreResult<Vec<DataBankScopeRecord>> {
        self.store.query_chat_data_bank_scopes(query)
    }

    pub fn query_data_bank_scopes_page(
        &self,
        query: &DataBankScopeQuery,
    ) -> CoreResult<ExactPage<DataBankScopeRecord>> {
        self.store.query_chat_data_bank_scopes_page(query)
    }

    pub fn remove_data_bank_scope(
        &self,
        scope_id: &DataBankScopeId,
        updated_at: &IsoTimestamp,
    ) -> CoreResult<DataBankScopeRecord> {
        ChatConversationStore::remove_data_bank_scope(&self.store, scope_id, updated_at)
    }

    pub fn remove_chat_data_bank_scope(
        &self,
        request: &RemoveChatDataBankScopeRequest,
    ) -> CoreResult<DataBankScopeRecord> {
        ChatConversationStore::remove_chat_data_bank_scope(&self.store, request)
    }

    pub fn select_active_message_variant(
        &self,
        request: &SelectActiveVariantRequest,
    ) -> CoreResult<SelectActiveVariantResult> {
        self.store.select_active_chat_message_variant_store(request)
    }

    pub fn select_active_chat_message_variant(
        &self,
        request: &SelectActiveChatMessageVariantRequest,
    ) -> CoreResult<SelectActiveChatMessageVariantResult> {
        let result = ChatConversationStore::select_active_chat_message_variant_store(
            &self.store,
            &SelectActiveVariantRequest {
                slot_id: request.slot_id.clone(),
                active_variant_id: request.active_variant_id.clone(),
                expected: request.expected.clone(),
                updated_at: request.updated_at.clone(),
            },
        )?;
        if result.slot.session_id != request.session_id {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!(
                    "message slot {} does not belong to chat session {}",
                    request.slot_id, request.session_id
                ),
            ));
        }
        if result.conflict.is_none() {
            if let Some(selected) = selected_message_variant(&result.slot) {
                if let Some(branch_id) = &selected.message.branch_id {
                    ChatConversationStore::update_chat_conversation_branch_head(
                        &self.store,
                        &UpdateBranchHeadRequest {
                            branch_id: branch_id.clone(),
                            head_message_id: Some(selected.message.message_id.clone()),
                            expected: BranchHeadExpectation::Any,
                            updated_at: request.updated_at.clone(),
                        },
                    )?;
                }
            }
        }
        Ok(SelectActiveChatMessageVariantResult {
            slot: result.slot,
            conflict: result.conflict,
        })
    }

    pub fn delete_message_variant(
        &self,
        slot_id: &MessageSlotId,
        variant_id: &MessageVariantId,
        updated_at: &IsoTimestamp,
    ) -> CoreResult<MessageSlotRecord> {
        ChatConversationStore::delete_message_variant(&self.store, slot_id, variant_id, updated_at)
    }

    pub fn reorder_message_variants(
        &self,
        slot_id: &MessageSlotId,
        ordered_variant_ids: &[MessageVariantId],
        updated_at: &IsoTimestamp,
    ) -> CoreResult<Vec<MessageVariantRecord>> {
        ChatConversationStore::reorder_message_variants(
            &self.store,
            slot_id,
            ordered_variant_ids,
            updated_at,
        )
    }

    pub fn list_profile_memory(
        &self,
        query: &ProfileMemoryQuery,
    ) -> CoreResult<Vec<ProfileMemoryRecord>> {
        CrewMemoryStore::list_profile_memory(&self.store, query)
    }

    pub fn list_memory_space_descriptors(&self) -> CoreResult<Vec<MemorySpaceDescriptor>> {
        Ok(vec![
            memory_spaces::profile_dense_descriptor(&ProfileMemoryCaps::default()),
            session_memory_space_descriptor(),
        ])
    }

    pub fn query_session_memory_records(
        &self,
        query: &SessionMemoryQuery,
    ) -> CoreResult<Vec<SessionMemoryRecord>> {
        CrewMemoryStore::query_session_memory_records(&self.store, query)
    }

    pub fn build_session_memory_prompt_context(
        &self,
        query: &BranchAwareSessionMemoryQuery,
    ) -> CoreResult<SessionMemoryPromptContext> {
        CrewMemoryStore::build_session_memory_prompt_context(&self.store, query)
    }

    pub fn save_memory_proposal(
        &self,
        mut proposal: MemoryProposalEnvelope,
    ) -> CoreResult<MemoryProposalRecord> {
        let descriptor = self.memory_space_descriptor(&proposal.space_id)?;
        let now = self.now();
        if proposal.created_at.is_none() {
            proposal.created_at = Some(now.clone());
        }
        CrewMemoryStore::save_memory_proposal(&self.store, &proposal, &descriptor, &now)
    }

    pub fn list_memory_proposals(
        &self,
        query: &MemoryProposalQuery,
    ) -> CoreResult<Vec<MemoryProposalRecord>> {
        CrewMemoryStore::list_memory_proposals(&self.store, query)
    }

    pub fn save_session_activity_digest(
        &self,
        digest: &SessionActivityDigest,
    ) -> CoreResult<SessionActivityDigest> {
        CrewMemoryStore::save_session_activity_digest(&self.store, digest)
    }

    pub fn list_session_activity_digests(
        &self,
        query: &SessionActivityDigestQuery,
    ) -> CoreResult<Vec<SessionActivityDigest>> {
        CrewMemoryStore::list_session_activity_digests(&self.store, query)
    }

    pub fn save_context_compaction_artifact(
        &self,
        artifact: &ContextCompactionArtifact,
    ) -> CoreResult<ContextCompactionArtifact> {
        CrewMemoryStore::save_context_compaction_artifact(&self.store, artifact)
    }

    pub fn list_context_compaction_artifacts(
        &self,
        query: &ContextCompactionArtifactQuery,
    ) -> CoreResult<Vec<ContextCompactionArtifact>> {
        CrewMemoryStore::list_context_compaction_artifacts(&self.store, query)
    }

    pub fn record_memory_governance_decision(
        &self,
        decision: &MemoryGovernanceDecisionInput,
    ) -> CoreResult<MemoryGovernanceDecisionRecord> {
        CrewMemoryStore::record_memory_governance_decision(&self.store, decision, &self.now())
    }

    fn memory_space_descriptor(
        &self,
        space_id: &rusty_crew_core_protocol::MemorySpaceId,
    ) -> CoreResult<MemorySpaceDescriptor> {
        self.list_memory_space_descriptors()?
            .into_iter()
            .find(|descriptor| descriptor.space_id == *space_id)
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    format!("memory space {} is not registered", space_id),
                )
            })
    }

    pub fn get_profile_memory(
        &self,
        profile_id: &ProfileId,
        target: &ProfileMemoryTarget,
        key: &str,
    ) -> CoreResult<Option<ProfileMemoryRecord>> {
        CrewMemoryStore::get_profile_memory(&self.store, profile_id, target, key)
    }

    pub fn add_profile_memory(
        &self,
        mut write: ProfileMemoryWrite,
        caps: &ProfileMemoryCaps,
    ) -> CoreResult<ProfileMemoryRecord> {
        write.now = self.now();
        CrewMemoryStore::add_profile_memory(&self.store, &write, caps)
    }

    pub fn replace_profile_memory(
        &self,
        mut replace: ProfileMemoryReplace,
        caps: &ProfileMemoryCaps,
    ) -> CoreResult<ProfileMemoryRecord> {
        replace.write.now = self.now();
        CrewMemoryStore::replace_profile_memory(&self.store, &replace, caps)
    }

    pub fn remove_profile_memory(
        &self,
        delete: &ProfileMemoryDelete,
    ) -> CoreResult<ProfileMemoryRecord> {
        CrewMemoryStore::remove_profile_memory(&self.store, delete)
    }

    pub fn search_runtime(
        &self,
        filter: &RuntimeSearchFilter,
    ) -> CoreResult<Vec<RuntimeSearchResult>> {
        RuntimeStorageAdminStore::search_runtime(&self.store, filter)
    }

    pub fn query_runtime_counters(
        &self,
        query: &RuntimeCounterQuery,
    ) -> CoreResult<Vec<RuntimeCounterRecord>> {
        RuntimeStorageAdminStore::query_runtime_counters(&self.store, query)
    }

    pub fn runtime_summary(&self, scope: &RuntimeCounterScope) -> CoreResult<RuntimeStateSummary> {
        RuntimeStorageAdminStore::runtime_summary(&self.store, scope)
    }

    pub fn reset_runtime_counters(&self, query: &RuntimeCounterQuery) -> CoreResult<u64> {
        RuntimeStorageAdminStore::reset_runtime_counters(&self.store, query, self.now())
    }
}

fn normalize_chat_read_model_limit(limit: Option<u32>) -> u32 {
    limit
        .unwrap_or(DEFAULT_CHAT_READ_MODEL_LIMIT)
        .min(MAX_CHAT_READ_MODEL_LIMIT)
}

fn chat_cursor_for(session_id: &SessionId, sequence: u64) -> String {
    format!("{session_id}:{sequence}")
}

fn chat_cursor_sequence(cursor: Option<&str>, session_id: &SessionId) -> u64 {
    let Some(cursor) = cursor else {
        return 0;
    };
    let Some(sequence) = cursor.strip_prefix(&format!("{session_id}:")) else {
        return 0;
    };
    sequence.parse::<u64>().unwrap_or(0)
}

fn chat_read_model_event_from_slot(
    session_id: &SessionId,
    agent_id: &str,
    sequence: u64,
    slot: &MessageSlotRecord,
) -> ChatReadModelEvent {
    let variant = slot
        .active_variant_id
        .as_ref()
        .and_then(|active_variant_id| {
            slot.alternates
                .iter()
                .find(|candidate| &candidate.variant_id == active_variant_id)
        })
        .unwrap_or(&slot.primary);
    durable_message_event(session_id, agent_id, sequence, &variant.message)
}

fn durable_message_event(
    session_id: &SessionId,
    agent_id: &str,
    sequence: u64,
    message: &DurableMessageRecord,
) -> ChatReadModelEvent {
    let role = if message.author_role == "assistant" || message.author_id == agent_id {
        "assistant"
    } else {
        "user"
    };
    let mut payload = json!({
        "message_id": message.message_id.0.as_str(),
        "role": role,
        "body": message.body.as_str(),
        "source": "durable_message_slot",
        "slot_status": message.status,
    });
    if let Some(correlation_id) = message
        .metadata_json
        .get("correlation_id")
        .and_then(|value| value.as_str())
    {
        if let Some(payload) = payload.as_object_mut() {
            payload.insert("correlation_id".to_string(), json!(correlation_id));
        }
    }

    ChatReadModelEvent {
        event_id: chat_cursor_for(session_id, sequence),
        session_id: session_id.clone(),
        sequence_id: sequence,
        created_at: message.created_at.clone(),
        kind: ChatReadModelEventKind::MessageCreated,
        payload_json: payload,
    }
}

fn pending_message_event(
    session: &SessionState,
    sequence: u64,
    message: &AgentMessage,
    created_at: IsoTimestamp,
) -> ChatReadModelEvent {
    let message_id = message
        .correlation_id
        .as_deref()
        .map(|correlation_id| format!("pending:{correlation_id}"))
        .unwrap_or_else(|| format!("pending:{sequence}"));
    let role = if message.from == session.agent_id {
        "assistant"
    } else {
        "user"
    };
    let mut payload = json!({
        "message_id": message_id,
        "role": role,
        "body": message.body.as_str(),
        "source": "pending_body_state",
    });
    if let Some(correlation_id) = message.correlation_id.as_deref() {
        if let Some(payload) = payload.as_object_mut() {
            payload.insert("correlation_id".to_string(), json!(correlation_id));
        }
    }
    ChatReadModelEvent {
        event_id: chat_cursor_for(&session.session_id, sequence),
        session_id: session.session_id.clone(),
        sequence_id: sequence,
        created_at,
        kind: ChatReadModelEventKind::MessageCreated,
        payload_json: payload,
    }
}

fn chat_read_model_event_as_log_event(event: ChatReadModelEvent) -> ChatEventLogEvent {
    ChatEventLogEvent {
        event_id: event.event_id,
        session_id: event.session_id,
        sequence_id: event.sequence_id,
        created_at: event.created_at,
        kind: match event.kind {
            ChatReadModelEventKind::MessageCreated => "message_created".to_string(),
        },
        payload_json: event.payload_json,
    }
}

fn session_status_wire_value(status: &SessionStatus) -> &str {
    match status {
        SessionStatus::Idle => "idle",
        SessionStatus::Active => "active",
        SessionStatus::Archived => "archived",
    }
}

fn selected_message_variant(slot: &MessageSlotRecord) -> Option<&MessageVariantRecord> {
    match &slot.active_variant_id {
        Some(active_variant_id) => slot
            .alternates
            .iter()
            .find(|variant| &variant.variant_id == active_variant_id),
        None => Some(&slot.primary),
    }
}

fn profile_registry_provider_alias(record: &ProfileRegistryRecord) -> Option<String> {
    record
        .active_runtime_settings_json
        .get("providerAlias")
        .or_else(|| record.active_runtime_settings_json.get("provider_alias"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn provider_wire_state_key(
    session_id: &SessionId,
    module_id: &str,
    strategy_id: &str,
) -> ProviderWireStateKey {
    ProviderWireStateKey {
        session_id: session_id.clone(),
        module_id: module_id.to_string(),
        strategy_id: strategy_id.to_string(),
    }
}

fn provider_state_registration_key(
    registration: &BrainImplementationRegistration,
) -> CoreResult<(String, String)> {
    let Some(strategy) = &registration.strategy else {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "brain registration has no provider-state strategy metadata",
        ));
    };
    if strategy.provider_state.mode == ProviderStateMode::Unused {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "brain registration does not use provider state",
        ));
    }
    Ok((strategy.module_id.clone(), strategy.strategy_id.clone()))
}

fn validate_provider_state_update_scope(
    state: &BrainWakeProviderStateUpdate,
    scope: &BrainProviderStateScope,
) -> CoreResult<()> {
    if state.profile_fingerprint != scope.profile_fingerprint {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "provider state update profile fingerprint does not match registered scope",
        ));
    }
    if state.provider_fingerprint != scope.provider_fingerprint {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "provider state update provider fingerprint does not match registered scope",
        ));
    }
    Ok(())
}

fn validate_tool_profile(tool_profile: &ToolProfile) -> CoreResult<()> {
    let mut names = HashSet::new();
    for tool in &tool_profile.tools {
        if tool.name.trim().is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "tool profile tool name must be non-empty",
            ));
        }
        if tool.description.trim().is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("tool profile tool {} requires a description", tool.name),
            ));
        }
        if !names.insert(tool.name.clone()) {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("tool profile contains duplicate tool {}", tool.name),
            ));
        }
    }
    Ok(())
}

fn validate_github_gate_suspend(request: &GitHubGateSuspendRequest) -> CoreResult<()> {
    if request.gate_id == 0
        || request.project_id.0.trim().is_empty()
        || request.task_id.0.trim().is_empty()
        || !valid_full_github_sha(&request.commit_sha)
    {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "GitHub gate suspension requires gate_id, project/task, and an exact 40-character SHA",
        ));
    }
    Ok(())
}

fn validate_github_gate_terminal_event(event: &GitHubGateTerminalEvent) -> CoreResult<()> {
    let valid_status = matches!(
        event.status.as_str(),
        "passed" | "failed" | "timed_out" | "superseded"
    );
    let valid_reason = matches!(
        event.terminal_reason.as_str(),
        "checks_passed" | "checks_failed" | "required_checks_missing" | "timeout" | "superseded"
    );
    if event.event_id == 0
        || event.gate_id == 0
        || !valid_full_github_sha(&event.commit_sha)
        || !valid_status
        || !valid_reason
    {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "invalid Review GitHub gate terminal event",
        ));
    }
    Ok(())
}

fn valid_full_github_sha(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
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
mod tests {
    use super::*;
    use rusty_crew_core_config::ClockConfig;
    #[cfg(feature = "postgres")]
    use rusty_crew_core_config::EngineStorageConfig;
    use rusty_crew_core_persistence::{
        ActiveVariantConflict, ActiveVariantExpectation, AgentMessageQuery, AttachmentLinkWrite,
        AttachmentStatus, BranchHeadConflict, ChatAttachmentMutationStatus,
        ChatConversationSnapshotMutationStatus, ChatDataBankScopeMutationStatus,
        CompletionPacketQuery, ConversationJumpTarget, ConversationSnapshotSource,
        CoordinationStore, DataBankScopeStatus, DurableMessageStatus, DurableMessageWrite,
        MessageVariantSource, MessageVariantStatus, QueryPage, QueuedMessageFilter,
        QueuedMessageRecord, QueuedMessageState, RuntimeCounterScope, RuntimeMaintenancePolicy,
        RuntimeSearchFilter, RuntimeSearchRowType, ScheduledRunQuery, ScheduledRunStatus,
        SessionQuery, ToolCallPhase, WorkerRunQuery,
    };
    use rusty_crew_core_protocol::SessionHistoryWindow;
    use rusty_crew_core_protocol::{
        AdapterId, AgentId, AgentMessage, AttachmentLinkId, BrainAction, BrainEvent,
        CompletionPacket, CompletionStatus, ConversationBranchId, ConversationSnapshotId,
        CoreErrorKind, CoreEventKind, DelegatedRunStatus, DelegationLifecyclePhase,
        ExternalEventPayload, MessageId, ProfileId, ProjectId, ResourceLimits, SessionKind, TaskId,
        ToolCallMetadata, ToolCallPolicyMetadata, ToolCallSource, ToolDescriptor, ToolProfile,
    };
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    static NEXT_TEST_DIR: AtomicU64 = AtomicU64::new(1);

    #[test]
    fn projects_body_state_from_real_session_and_bus_history() {
        let engine = test_engine();
        let planner = engine
            .create_session(session_config(
                "planner-session",
                "planner",
                "planner-profile",
                SessionKind::Full,
            ))
            .unwrap();
        let worker = engine
            .create_session(session_config(
                "worker-session",
                "worker",
                "coder-profile",
                SessionKind::Worker,
            ))
            .unwrap();

        assert_ne!(planner.handle, worker.handle);
        assert_eq!(
            engine.get_session(&worker.session_id).unwrap().handle,
            worker.handle
        );

        engine
            .bus()
            .route_message(
                planner.agent_id.clone(),
                worker.agent_id.clone(),
                "please implement the slice",
            )
            .unwrap();

        let body = engine.project_body_state(&worker.session_id).unwrap();

        assert_eq!(body.session.session_id, worker.session_id);
        assert_eq!(body.pending_messages.len(), 1);
        assert_eq!(body.pending_messages[0].body, "please implement the slice");
        assert!(body
            .recent_events
            .iter()
            .any(|event| matches!(event, CoreEvent::SessionCreated { .. })));
    }

    #[test]
    fn shutdown_archives_sessions_and_releases_subscribers() {
        let engine = test_engine();
        engine
            .create_session(session_config(
                "prime-session",
                "prime",
                "prime-profile",
                SessionKind::Full,
            ))
            .unwrap();
        engine
            .create_session(session_config(
                "worker-session",
                "worker",
                "worker-profile",
                SessionKind::Worker,
            ))
            .unwrap();
        let (_first_id, first_receiver) = engine
            .subscribe_events(EventSubscription {
                event_kinds: vec![CoreEventKind::SessionArchived],
                session_id: None,
                agent_id: None,
                adapter_id: None,
            })
            .unwrap();
        let (_second_id, second_receiver) = engine
            .subscribe_events(EventSubscription {
                event_kinds: vec![CoreEventKind::SessionArchived],
                session_id: None,
                agent_id: None,
                adapter_id: None,
            })
            .unwrap();

        let summary = engine.shutdown_with_timeout(25).unwrap();

        assert_eq!(summary.archived_sessions, 2);
        assert_eq!(summary.dropped_subscriptions, 2);
        assert_receiver_disconnects_after_buffered_events(first_receiver);
        assert_receiver_disconnects_after_buffered_events(second_receiver);
    }

    #[test]
    fn ensure_configured_session_reactivates_archived_session_without_replacement() {
        let data_dir = unique_data_dir("ensure-configured-session");
        let engine = test_engine_with_data_dir(data_dir.clone());
        let config = session_config(
            "configured-session",
            "prime",
            "prime-profile",
            SessionKind::Full,
        );
        let created = engine.create_session(config.clone()).unwrap();
        engine.archive_session(&created.session_id).unwrap();

        let store = CoordinationStore::open(data_dir).unwrap();
        store
            .save_queued_message(&QueuedMessageRecord {
                message_id: "stale-follow-up".to_string(),
                owner_session_id: Some(created.session_id.clone()),
                owner_agent_id: created.agent_id.clone(),
                message: AgentMessage {
                    from: AgentId::new("operator"),
                    to: created.agent_id.clone(),
                    body: "do not resurrect this stale message".to_string(),
                    correlation_id: None,
                    projection: None,
                },
                source_sequence: None,
                enqueued_at: "2026-06-18T23:59:00Z".to_string(),
                expires_at: "2026-06-18T23:59:01Z".to_string(),
                ttl_ms: 1_000,
                delivery_attempts: 0,
                state: QueuedMessageState::Pending,
                terminal_at: None,
                state_reason: None,
            })
            .unwrap();

        let reactivated = engine.ensure_configured_session(config).unwrap();

        assert_eq!(reactivated.session_id, created.session_id);
        assert_eq!(reactivated.handle, created.handle);
        assert_eq!(reactivated.status, SessionStatus::Idle);
        let body = engine
            .prepare_body_state_for_wake(&created.session_id)
            .unwrap();
        assert!(body.pending_messages.is_empty());
        assert_eq!(
            store
                .load_queued_messages(&QueuedMessageFilter {
                    state: Some(QueuedMessageState::Expired),
                    owner_session_id: Some(created.session_id.clone()),
                    owner_agent_id: None,
                    limit: None,
                })
                .unwrap()
                .len(),
            1,
        );
    }

    #[test]
    fn ensure_configured_session_refreshes_existing_session_config() {
        let engine = test_engine();
        let mut config = session_config(
            "configured-session",
            "prime",
            "prime-profile",
            SessionKind::Full,
        );
        let created = engine.create_session(config.clone()).unwrap();

        config.resource_limits.max_duration_ms = Some(120_000);
        config.tool_profile = ToolProfile {
            tools: vec![ToolDescriptor {
                name: "read_file".to_string(),
                description: "Read a file".to_string(),
                input_schema: None,
            }],
        };
        let refreshed = engine.ensure_configured_session(config).unwrap();

        assert_eq!(refreshed.session_id, created.session_id);
        assert_eq!(refreshed.handle, created.handle);
        assert_eq!(refreshed.resource_limits.max_duration_ms, Some(120_000));
        assert_eq!(refreshed.tool_profile.tools.len(), 1);
        assert_eq!(refreshed.tool_profile.tools[0].name, "read_file");
    }

    #[test]
    fn restart_reactivates_only_roleplay_sessions_with_active_metadata() {
        let data_dir = unique_data_dir("roleplay-session-restart");
        {
            let engine = test_engine_with_data_dir(data_dir.clone());
            for (session_id, archived) in [
                ("active-roleplay-session", false),
                ("archived-roleplay-session", true),
            ] {
                engine
                    .create_session(session_config(
                        session_id,
                        "narrator",
                        "roleplay-profile",
                        SessionKind::Full,
                    ))
                    .unwrap();
                engine
                    .put_roleplay_session_metadata(&RoleplaySessionMetadataWrite {
                        record: RoleplaySessionMetadataRecord {
                            session_id: session_id.to_string(),
                            profile_id: "roleplay-profile".to_string(),
                            display_name: Some(session_id.to_string()),
                            player_persona_id: None,
                            character_id: None,
                            active_layer_ids: Vec::new(),
                            archived,
                            revision: 1,
                            created_at: "2026-06-19T00:00:00Z".to_string(),
                            updated_at: "2026-06-19T00:00:00Z".to_string(),
                        },
                        expected_revision: None,
                    })
                    .unwrap();
            }
            engine.shutdown_with_timeout(25).unwrap();
        }

        let reopened = test_engine_with_data_dir(data_dir.clone());
        assert_eq!(
            reopened
                .get_session(&SessionId::new("active-roleplay-session"))
                .unwrap()
                .status,
            SessionStatus::Idle
        );
        assert_eq!(
            reopened
                .get_session(&SessionId::new("archived-roleplay-session"))
                .unwrap()
                .status,
            SessionStatus::Archived
        );
        drop(reopened);
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn routing_message_to_active_session_requests_brain_wake() {
        let engine = test_engine();
        let worker = engine
            .create_session(session_config(
                "worker-session",
                "worker",
                "coder-profile",
                SessionKind::Worker,
            ))
            .unwrap();
        let (_subscription_id, events) = engine
            .subscribe_events(EventSubscription {
                event_kinds: vec![
                    CoreEventKind::AgentMessageRouted,
                    CoreEventKind::BrainWakeRequested,
                ],
                session_id: None,
                agent_id: None,
                adapter_id: None,
            })
            .unwrap();

        let receipt = engine
            .route_agent_message(AgentMessage {
                from: AgentId::new("planner"),
                to: worker.agent_id.clone(),
                body: "please wake".to_string(),
                correlation_id: None,
                projection: None,
            })
            .unwrap();

        assert!(receipt.accepted);
        assert!(matches!(
            events.recv_timeout(Duration::from_secs(1)).unwrap(),
            CoreEvent::AgentMessageRouted { .. }
        ));
        assert!(matches!(
            events.recv_timeout(Duration::from_secs(1)).unwrap(),
            CoreEvent::BrainWakeRequested { session_id } if session_id == worker.session_id
        ));
    }

    #[test]
    fn routing_message_to_archived_session_does_not_request_brain_wake() {
        let engine = test_engine();
        let worker = engine
            .create_session(session_config(
                "worker-session",
                "worker",
                "coder-profile",
                SessionKind::Worker,
            ))
            .unwrap();
        engine.archive_session(&worker.session_id).unwrap();
        let (_subscription_id, events) = engine
            .subscribe_events(EventSubscription {
                event_kinds: vec![CoreEventKind::BrainWakeRequested],
                session_id: None,
                agent_id: None,
                adapter_id: None,
            })
            .unwrap();

        engine
            .route_agent_message(AgentMessage {
                from: AgentId::new("planner"),
                to: worker.agent_id,
                body: "do not wake".to_string(),
                correlation_id: None,
                projection: None,
            })
            .unwrap();

        assert!(events.recv_timeout(Duration::from_millis(50)).is_err());
    }

    #[test]
    fn scheduler_tick_requests_wake_and_records_terminal_run() {
        let engine = test_engine();
        let prime = engine
            .create_session(session_config(
                "prime-session",
                "prime",
                "prime-profile",
                SessionKind::Full,
            ))
            .unwrap();
        let (_subscription_id, events) = engine
            .subscribe_events(EventSubscription {
                event_kinds: vec![CoreEventKind::BrainWakeRequested],
                session_id: Some(prime.session_id.clone()),
                agent_id: None,
                adapter_id: None,
            })
            .unwrap();

        engine
            .register_scheduled_wake_job(
                "wake-prime",
                prime.session_id.clone(),
                Some(60_000),
                "2026-06-19T00:00:00Z".to_string(),
            )
            .unwrap();
        let report = engine.run_scheduler_tick().unwrap();

        assert_eq!(report.due_runs_claimed, 1);
        assert_eq!(report.wakes_requested, 1);
        assert_eq!(report.runs_completed, 1);
        assert!(matches!(
            events.recv_timeout(Duration::from_secs(1)).unwrap(),
            CoreEvent::BrainWakeRequested { session_id } if session_id == prime.session_id
        ));
        let store = CoordinationStore::open(engine.config.engine_data_dir.clone()).unwrap();
        let runs = store
            .query_scheduled_runs(&ScheduledRunQuery {
                status: Some(ScheduledRunStatus::Completed),
                target_session_id: Some(prime.session_id.clone()),
                ..ScheduledRunQuery::default()
            })
            .unwrap();
        assert_eq!(runs.len(), 1);
        assert!(
            runs[0]
                .run_id
                .0
                .starts_with("scheduled:wake-prime:2026_06_19T00_00_00Z:"),
            "scheduled run id should be derived from the engine clock, got {}",
            runs[0].run_id
        );
        assert_eq!(
            store
                .load_scheduled_job("wake-prime")
                .unwrap()
                .unwrap()
                .next_due_at,
            Some("2026-06-19T00:01:00Z".to_string())
        );
    }

    #[test]
    fn body_follow_up_queue_drains_once_at_wake_boundary() {
        let engine = test_engine();
        let prime = engine
            .create_session(session_config(
                "prime-session",
                "prime",
                "prime-profile",
                SessionKind::Full,
            ))
            .unwrap();
        let (_subscription_id, events) = engine
            .subscribe_events(EventSubscription {
                event_kinds: vec![CoreEventKind::BrainWakeRequested],
                session_id: Some(prime.session_id.clone()),
                agent_id: None,
                adapter_id: None,
            })
            .unwrap();

        engine
            .enqueue_body_follow_up_message(
                &prime.session_id,
                AgentId::new("operator"),
                "arrived mid-turn",
                Some("follow-up-1".to_string()),
            )
            .unwrap();
        assert!(matches!(
            events.recv_timeout(Duration::from_secs(1)).unwrap(),
            CoreEvent::BrainWakeRequested { session_id } if session_id == prime.session_id
        ));

        let diagnostic = engine.project_body_state(&prime.session_id).unwrap();
        assert!(diagnostic.pending_messages.is_empty());

        let prepared = engine
            .prepare_body_state_for_wake(&prime.session_id)
            .unwrap();
        assert_eq!(
            prepared
                .pending_messages
                .iter()
                .map(|message| message.body.as_str())
                .collect::<Vec<_>>(),
            vec!["arrived mid-turn"]
        );
        let second = engine
            .prepare_body_state_for_wake(&prime.session_id)
            .unwrap();
        assert!(second.pending_messages.is_empty());

        let store = CoordinationStore::open(engine.config.engine_data_dir.clone()).unwrap();
        assert_eq!(
            store
                .load_queued_messages(&QueuedMessageFilter {
                    state: Some(QueuedMessageState::Pending),
                    owner_session_id: Some(prime.session_id.clone()),
                    owner_agent_id: None,
                    limit: None,
                })
                .unwrap()
                .len(),
            0
        );
        let delivered = store
            .load_queued_messages(&QueuedMessageFilter {
                state: Some(QueuedMessageState::Delivered),
                owner_session_id: Some(prime.session_id),
                owner_agent_id: None,
                limit: None,
            })
            .unwrap();
        assert_eq!(delivered.len(), 1);
        assert!(
            delivered[0]
                .message_id
                .starts_with("follow-up:prime-session:2026_06_19T00_00_00Z:"),
            "queued follow-up id should be derived from the engine clock, got {}",
            delivered[0].message_id
        );
    }

    #[test]
    fn session_history_window_bounds_wake_messages_without_resurrecting_queue_overflow() {
        let engine = test_engine();
        let mut config =
            session_config("prime-session", "prime", "prime-profile", SessionKind::Full);
        config.history_window = Some(SessionHistoryWindow {
            max_messages: Some(2),
        });
        let prime = engine.create_session(config).unwrap();

        for index in 1..=4 {
            engine
                .route_agent_message(AgentMessage {
                    from: AgentId::new("operator"),
                    to: prime.agent_id.clone(),
                    body: format!("bus-message-{index}"),
                    correlation_id: Some(format!("bus-{index}")),
                    projection: None,
                })
                .unwrap();
        }
        let diagnostic = engine.project_body_state(&prime.session_id).unwrap();
        assert_eq!(
            diagnostic
                .pending_messages
                .iter()
                .map(|message| message.body.as_str())
                .collect::<Vec<_>>(),
            vec!["bus-message-3", "bus-message-4"]
        );

        for index in 1..=4 {
            engine
                .enqueue_body_follow_up_message(
                    &prime.session_id,
                    AgentId::new("operator"),
                    format!("queued-message-{index}"),
                    Some(format!("queued-{index}")),
                )
                .unwrap();
        }
        let prepared = engine
            .prepare_body_state_for_wake(&prime.session_id)
            .unwrap();
        assert_eq!(
            prepared
                .pending_messages
                .iter()
                .map(|message| message.body.as_str())
                .collect::<Vec<_>>(),
            vec!["bus-message-3", "bus-message-4"]
        );

        let second = engine
            .prepare_body_state_for_wake(&prime.session_id)
            .unwrap();
        assert_eq!(
            second
                .pending_messages
                .iter()
                .map(|message| message.body.as_str())
                .collect::<Vec<_>>(),
            vec!["bus-message-3", "bus-message-4"]
        );

        let store = CoordinationStore::open(engine.config.engine_data_dir.clone()).unwrap();
        let discarded = store
            .load_queued_messages(&QueuedMessageFilter {
                state: Some(QueuedMessageState::Discarded),
                owner_session_id: Some(prime.session_id.clone()),
                owner_agent_id: None,
                limit: None,
            })
            .unwrap();
        assert_eq!(discarded.len(), 4);
        assert!(discarded
            .iter()
            .all(|record| record.state_reason.as_deref() == Some("history_window_exceeded")));
        assert_eq!(
            store
                .load_queued_messages(&QueuedMessageFilter {
                    state: Some(QueuedMessageState::Pending),
                    owner_session_id: Some(prime.session_id.clone()),
                    owner_agent_id: None,
                    limit: None,
                })
                .unwrap()
                .len(),
            0
        );

        let mut queue_only_config = session_config(
            "queue-session",
            "queue-agent",
            "prime-profile",
            SessionKind::Full,
        );
        queue_only_config.history_window = Some(SessionHistoryWindow {
            max_messages: Some(2),
        });
        let queue_only = engine.create_session(queue_only_config).unwrap();
        for index in 1..=4 {
            engine
                .enqueue_body_follow_up_message(
                    &queue_only.session_id,
                    AgentId::new("operator"),
                    format!("queue-only-{index}"),
                    Some(format!("queue-only-{index}")),
                )
                .unwrap();
        }
        let queue_only_wake = engine
            .prepare_body_state_for_wake(&queue_only.session_id)
            .unwrap();
        assert_eq!(
            queue_only_wake
                .pending_messages
                .iter()
                .map(|message| message.body.as_str())
                .collect::<Vec<_>>(),
            vec!["queue-only-3", "queue-only-4"]
        );
        assert_eq!(
            store
                .load_queued_messages(&QueuedMessageFilter {
                    state: Some(QueuedMessageState::Pending),
                    owner_session_id: Some(queue_only.session_id.clone()),
                    owner_agent_id: None,
                    limit: None,
                })
                .unwrap()
                .len(),
            0
        );
    }

    #[test]
    fn session_history_window_survives_engine_restart() {
        let data_dir = unique_data_dir("history-window-restart");
        let engine = test_engine_with_data_dir(data_dir.clone());
        let mut config =
            session_config("prime-session", "prime", "prime-profile", SessionKind::Full);
        config.history_window = Some(SessionHistoryWindow {
            max_messages: Some(1),
        });
        let prime = engine.create_session(config).unwrap();
        engine
            .route_agent_message(AgentMessage {
                from: AgentId::new("operator"),
                to: prime.agent_id.clone(),
                body: "first".to_string(),
                correlation_id: None,
                projection: None,
            })
            .unwrap();
        engine
            .route_agent_message(AgentMessage {
                from: AgentId::new("operator"),
                to: prime.agent_id.clone(),
                body: "second".to_string(),
                correlation_id: None,
                projection: None,
            })
            .unwrap();
        drop(engine);

        let restarted = test_engine_with_data_dir(data_dir);
        let session = restarted.get_session(&prime.session_id).unwrap();
        assert_eq!(
            session
                .history_window
                .as_ref()
                .and_then(|window| window.max_messages),
            Some(1)
        );
        let body = restarted.project_body_state(&prime.session_id).unwrap();
        assert_eq!(
            body.pending_messages
                .iter()
                .map(|message| message.body.as_str())
                .collect::<Vec<_>>(),
            vec!["second"]
        );
    }

    #[test]
    fn body_follow_up_queue_caps_and_expires_without_redelivery() {
        let data_dir = unique_data_dir("follow-up-queue");
        let engine = test_engine_with_data_dir(data_dir.clone());
        let prime = engine
            .create_session(session_config(
                "prime-session",
                "prime",
                "prime-profile",
                SessionKind::Full,
            ))
            .unwrap();
        for index in 0..33 {
            engine
                .enqueue_body_follow_up_message(
                    &prime.session_id,
                    AgentId::new("operator"),
                    format!("queued follow-up {index}"),
                    Some(format!("follow-up-{index}")),
                )
                .unwrap();
        }
        let store = CoordinationStore::open(data_dir.clone()).unwrap();
        assert_eq!(
            store
                .load_queued_messages(&QueuedMessageFilter {
                    state: Some(QueuedMessageState::Pending),
                    owner_session_id: Some(prime.session_id.clone()),
                    owner_agent_id: None,
                    limit: None,
                })
                .unwrap()
                .len(),
            32
        );
        assert_eq!(
            store
                .load_queued_messages(&QueuedMessageFilter {
                    state: Some(QueuedMessageState::Discarded),
                    owner_session_id: Some(prime.session_id.clone()),
                    owner_agent_id: None,
                    limit: None,
                })
                .unwrap()
                .len(),
            1
        );

        drop(engine);
        let late_engine = CoreEngine::initialize(EngineConfig {
            engine_data_dir: data_dir.to_string_lossy().to_string(),
            clock: ClockConfig::Fixed {
                at: "2026-06-19T00:00:06Z".to_string(),
            },
            default_turn_budget: 3,
            default_idle_timeout_ms: 1000,
            storage: None,
        })
        .unwrap();
        let prepared = late_engine
            .prepare_body_state_for_wake(&prime.session_id)
            .unwrap();
        assert!(prepared.pending_messages.is_empty());
        let late_store = CoordinationStore::open(data_dir.clone()).unwrap();
        assert_eq!(
            late_store
                .load_queued_messages(&QueuedMessageFilter {
                    state: Some(QueuedMessageState::Expired),
                    owner_session_id: Some(prime.session_id),
                    owner_agent_id: None,
                    limit: None,
                })
                .unwrap()
                .len(),
            32
        );
    }

    #[test]
    fn executes_valid_brain_actions_against_real_bus() {
        let engine = test_engine();
        let worker = engine
            .create_session(session_config(
                "worker-session",
                "worker",
                "coder-profile",
                SessionKind::Worker,
            ))
            .unwrap();

        let (_subscription_id, events) = engine
            .subscribe_events(EventSubscription {
                event_kinds: vec![
                    CoreEventKind::AgentMessageRouted,
                    CoreEventKind::CompletionPacketDelivered,
                    CoreEventKind::BrainActionsAccepted,
                ],
                session_id: None,
                agent_id: None,
                adapter_id: None,
            })
            .unwrap();

        let receipt = engine
            .execute_brain_actions(BrainActionBatch {
                wake_id: "wake-1".to_string(),
                session_id: worker.session_id.clone(),
                actions: vec![
                    BrainAction::SendMessage {
                        message: AgentMessage {
                            from: worker.agent_id.clone(),
                            to: AgentId::new("planner"),
                            body: "done".to_string(),
                            correlation_id: Some("reply-1".to_string()),
                            projection: None,
                        },
                    },
                    BrainAction::DeliverCompletion {
                        packet: CompletionPacket {
                            session_id: worker.session_id.clone(),
                            status: CompletionStatus::Completed,
                            summary: "implemented".to_string(),
                        },
                    },
                ],
            })
            .unwrap();

        assert_eq!(receipt.accepted_actions, 2);
        assert!(receipt.rejected_actions.is_empty());

        let first = events.recv_timeout(Duration::from_secs(1)).unwrap();
        let second = events.recv_timeout(Duration::from_secs(1)).unwrap();
        let third = events.recv_timeout(Duration::from_secs(1)).unwrap();

        assert!(matches!(first, CoreEvent::AgentMessageRouted { .. }));
        assert!(matches!(
            second,
            CoreEvent::CompletionPacketDelivered { .. }
        ));
        assert!(matches!(
            third,
            CoreEvent::BrainActionsAccepted { count: 2, .. }
        ));

        let body = engine.project_body_state(&worker.session_id).unwrap();
        assert!(body
            .recent_events
            .iter()
            .any(|event| matches!(event, CoreEvent::CompletionPacketDelivered { .. })));
    }

    #[test]
    fn request_delegation_creates_and_wakes_worker_session() {
        let data_dir = unique_data_dir("delegated-slice");
        let engine = test_engine_with_data_dir(data_dir.clone());
        let planner = engine
            .create_session(session_config(
                "planner-session",
                "planner",
                "planner-profile",
                SessionKind::Full,
            ))
            .unwrap();
        let (_subscription_id, events) = engine
            .subscribe_events(EventSubscription {
                event_kinds: vec![
                    CoreEventKind::SessionCreated,
                    CoreEventKind::AgentMessageRouted,
                    CoreEventKind::BrainWakeRequested,
                    CoreEventKind::BrainActionsAccepted,
                    CoreEventKind::CompletionPacketDelivered,
                ],
                session_id: None,
                agent_id: None,
                adapter_id: None,
            })
            .unwrap();

        let receipt = engine
            .execute_brain_actions(BrainActionBatch {
                wake_id: "planner-wake".to_string(),
                session_id: planner.session_id.clone(),
                actions: vec![BrainAction::RequestDelegation {
                    profile_id: ProfileId::new("coder-profile"),
                    task_id: Some(rusty_crew_core_protocol::TaskId::new("2772")),
                    prompt: "complete the tiny delegated slice".to_string(),
                    expected_output: Some("completion packet with concise summary".to_string()),
                    resource_limits: Some(ResourceLimits {
                        workdir: Some("/home/dev/rusty-crew".to_string()),
                        max_duration_ms: Some(30_000),
                        max_delegation_depth: Some(0),
                    }),
                    timeout_ms: Some(30_000),
                    priority: Some(rusty_crew_core_protocol::DelegationPriority::High),
                    fan_out_group_id: Some("implementation-slice".to_string()),
                    fan_out_max_concurrency: None,
                    fan_out_failure_policy: None,
                    correlation_id: Some("delegation-correlation-1".to_string()),
                    parent_consumption: Some(
                        rusty_crew_core_protocol::ParentConsumptionPolicy::AwaitCompletion,
                    ),
                    capacity_request: None,
                }],
            })
            .unwrap();

        assert_eq!(receipt.accepted_actions, 1);
        let delegated_session_id = delegated_session_id(&planner.session_id, "planner-wake", 0);
        let delegated = engine.get_session(&delegated_session_id).unwrap();
        assert_eq!(delegated.kind, SessionKind::Delegated);
        assert_eq!(delegated.profile_id, ProfileId::new("coder-profile"));
        assert_eq!(
            delegated.resource_limits,
            ResourceLimits {
                workdir: Some("/home/dev/rusty-crew".to_string()),
                max_duration_ms: Some(30_000),
                max_delegation_depth: Some(0),
            }
        );
        assert_eq!(
            delegated
                .delegation
                .as_ref()
                .map(|lineage| &lineage.parent_session_id),
            Some(&planner.session_id)
        );
        assert_eq!(
            delegated
                .delegation
                .as_ref()
                .map(|lineage| lineage.source_action_index),
            Some(0)
        );
        assert_eq!(
            delegated
                .delegation
                .as_ref()
                .map(|lineage| lineage.correlation_id.as_str()),
            Some("delegation-correlation-1")
        );
        assert_eq!(
            delegated
                .delegation
                .as_ref()
                .and_then(|lineage| lineage.requested_task_id.as_ref())
                .map(|task_id| task_id.0.as_str()),
            Some("2772")
        );
        assert_eq!(
            engine
                .delegated_sessions_for_parent(&planner.session_id)
                .unwrap(),
            vec![delegated.clone()]
        );
        assert_eq!(
            engine
                .delegated_session_for_run(&RunId::new("planner-wake:0"))
                .unwrap(),
            Some(delegated.clone())
        );
        assert_eq!(
            CoordinationStore::open(data_dir.clone())
                .unwrap()
                .load_worker_run(&RunId::new("planner-wake:0"))
                .unwrap()
                .unwrap()
                .status,
            WorkerRunStatus::WakeRequested
        );

        let body = engine.project_body_state(&delegated_session_id).unwrap();
        assert_eq!(body.session.delegation, delegated.delegation);
        assert_eq!(body.pending_messages.len(), 1);
        assert_eq!(
            body.pending_messages[0].body,
            "complete the tiny delegated slice"
        );

        let mut observed_wake = false;
        for _ in 0..4 {
            if matches!(
                events.recv_timeout(Duration::from_secs(1)).unwrap(),
                CoreEvent::BrainWakeRequested { session_id } if session_id == delegated_session_id
            ) {
                observed_wake = true;
            }
        }
        assert!(observed_wake);

        engine
            .submit_brain_event(BrainEventEnvelope {
                wake_id: "worker-wake".to_string(),
                session_id: delegated_session_id.clone(),
                event: BrainEvent::Started,
            })
            .unwrap();
        assert_eq!(
            CoordinationStore::open(data_dir.clone())
                .unwrap()
                .load_worker_run(&RunId::new("planner-wake:0"))
                .unwrap()
                .unwrap()
                .status,
            WorkerRunStatus::Running
        );

        engine
            .execute_brain_actions(BrainActionBatch {
                wake_id: "worker-wake".to_string(),
                session_id: delegated_session_id.clone(),
                actions: vec![BrainAction::DeliverCompletion {
                    packet: CompletionPacket {
                        session_id: delegated_session_id.clone(),
                        status: CompletionStatus::Completed,
                        summary: "delegated worker completed".to_string(),
                    },
                }],
            })
            .unwrap();

        assert!(matches!(
            events.recv_timeout(Duration::from_secs(1)).unwrap(),
            CoreEvent::BrainActionsAccepted { .. } | CoreEvent::CompletionPacketDelivered { .. }
        ));

        let store = CoordinationStore::open(data_dir).unwrap();
        assert_eq!(store.count_rows("sessions").unwrap(), 2);
        assert_eq!(store.count_rows("worker_runs").unwrap(), 1);
        assert_eq!(store.count_rows("completion_packets").unwrap(), 1);
        assert_eq!(
            store
                .load_worker_run(&RunId::new("planner-wake:0"))
                .unwrap()
                .unwrap()
                .status,
            WorkerRunStatus::Completed
        );
    }

    #[test]
    fn pooled_capacity_binds_to_normal_worker_run_and_closes_on_completion() {
        let data_dir = unique_data_dir("pooled-delegation");
        let engine = test_engine_with_data_dir(data_dir.clone());
        let planner = engine
            .create_session(session_config(
                "planner-session",
                "planner",
                "planner-profile",
                SessionKind::Full,
            ))
            .unwrap();
        engine
            .store
            .upsert_worker_pool_member(&rusty_crew_core_persistence::WorkerPoolMemberRecord {
                member_id: "member-coder-1".to_string(),
                profile_id: ProfileId::new("coder-profile"),
                agent_id: Some(AgentId::new("agent:member-coder-1")),
                session_id: None,
                status: WorkerPoolMemberStatus::Available,
                concurrency_limit: 1,
                active_leases: 0,
                capabilities_json: serde_json::json!({"profile": "coder-profile"}),
                registered_at: "2026-06-19T00:00:00Z".to_string(),
                last_heartbeat_at: "2026-06-19T00:00:00Z".to_string(),
                updated_at: "2026-06-19T00:00:00Z".to_string(),
            })
            .unwrap();

        let receipt = engine
            .execute_brain_actions(BrainActionBatch {
                wake_id: "planner-wake".to_string(),
                session_id: planner.session_id.clone(),
                actions: vec![
                    BrainAction::RequestDelegation {
                        profile_id: ProfileId::new("coder-profile"),
                        task_id: None,
                        prompt: "direct child".to_string(),
                        expected_output: None,
                        resource_limits: None,
                        timeout_ms: None,
                        priority: None,
                        fan_out_group_id: None,
                        fan_out_max_concurrency: None,
                        fan_out_failure_policy: None,
                        correlation_id: Some("direct-child".to_string()),
                        parent_consumption: None,
                        capacity_request: None,
                    },
                    BrainAction::RequestDelegation {
                        profile_id: ProfileId::new("coder-profile"),
                        task_id: None,
                        prompt: "pooled child".to_string(),
                        expected_output: None,
                        resource_limits: None,
                        timeout_ms: None,
                        priority: None,
                        fan_out_group_id: None,
                        fan_out_max_concurrency: None,
                        fan_out_failure_policy: None,
                        correlation_id: Some("pooled-child".to_string()),
                        parent_consumption: None,
                        capacity_request: Some(WorkerPoolCapacityRequest {
                            member_id: "member-coder-1".to_string(),
                            claim_ttl_ms: Some(60_000),
                            fallback_policy: WorkerPoolCapacityFallbackPolicy::RejectOnNoCapacity,
                        }),
                    },
                ],
            })
            .unwrap();
        assert_eq!(receipt.accepted_actions, 2);

        let store = CoordinationStore::open(data_dir.clone()).unwrap();
        let direct_run = store
            .load_worker_run(&RunId::new("planner-wake:0"))
            .unwrap()
            .unwrap();
        assert_eq!(direct_run.worker_pool_lease_id, None);
        let pooled_run = store
            .load_worker_run(&RunId::new("planner-wake:1"))
            .unwrap()
            .unwrap();
        assert_eq!(
            pooled_run.worker_pool_work_item_id.as_deref(),
            Some("planner-wake:1")
        );
        assert_eq!(
            pooled_run.worker_pool_lease_id.as_deref(),
            Some("lease:planner-wake:1")
        );
        assert_eq!(
            pooled_run.worker_pool_member_id.as_deref(),
            Some("member-coder-1")
        );
        assert_eq!(
            store
                .load_worker_pool_member("member-coder-1")
                .unwrap()
                .unwrap()
                .active_leases,
            1
        );

        let pooled_session_id = delegated_session_id(&planner.session_id, "planner-wake", 1);
        engine
            .execute_brain_actions(BrainActionBatch {
                wake_id: "pooled-worker-wake".to_string(),
                session_id: pooled_session_id,
                actions: vec![BrainAction::DeliverCompletion {
                    packet: CompletionPacket {
                        session_id: delegated_session_id(&planner.session_id, "planner-wake", 1),
                        status: CompletionStatus::Completed,
                        summary: "pooled child completed".to_string(),
                    },
                }],
            })
            .unwrap();

        let reopened = CoordinationStore::open(data_dir).unwrap();
        assert_eq!(
            reopened
                .load_worker_pool_work_item("planner-wake:1")
                .unwrap()
                .unwrap()
                .status,
            WorkerPoolWorkStatus::Completed
        );
        assert_eq!(
            reopened
                .load_worker_pool_member("member-coder-1")
                .unwrap()
                .unwrap()
                .active_leases,
            0
        );
    }

    #[test]
    fn pooled_capacity_required_reports_typed_no_capacity_without_direct_fallback() {
        let data_dir = unique_data_dir("pooled-no-capacity");
        let engine = test_engine_with_data_dir(data_dir.clone());
        let planner = engine
            .create_session(session_config(
                "planner-session",
                "planner",
                "planner-profile",
                SessionKind::Full,
            ))
            .unwrap();

        let error = engine
            .execute_brain_actions(BrainActionBatch {
                wake_id: "planner-wake".to_string(),
                session_id: planner.session_id,
                actions: vec![BrainAction::RequestDelegation {
                    profile_id: ProfileId::new("coder-profile"),
                    task_id: None,
                    prompt: "pooled child".to_string(),
                    expected_output: None,
                    resource_limits: None,
                    timeout_ms: None,
                    priority: None,
                    fan_out_group_id: None,
                    fan_out_max_concurrency: None,
                    fan_out_failure_policy: None,
                    correlation_id: None,
                    parent_consumption: None,
                    capacity_request: Some(WorkerPoolCapacityRequest {
                        member_id: "missing-member".to_string(),
                        claim_ttl_ms: Some(60_000),
                        fallback_policy: WorkerPoolCapacityFallbackPolicy::RejectOnNoCapacity,
                    }),
                }],
            })
            .unwrap_err();

        assert_eq!(error.kind, CoreErrorKind::ActionRejected);
        assert!(error.message.contains("member_unavailable"));
        let store = CoordinationStore::open(data_dir).unwrap();
        assert_eq!(store.count_rows("worker_runs").unwrap(), 0);
        assert_eq!(store.count_rows("worker_pool_work_items").unwrap(), 0);
    }

    #[test]
    fn rejects_invalid_brain_actions_before_bus_execution() {
        let engine = test_engine();
        let worker = engine
            .create_session(session_config(
                "worker-session",
                "worker",
                "coder-profile",
                SessionKind::Worker,
            ))
            .unwrap();

        let receipt = engine
            .execute_brain_actions(BrainActionBatch {
                wake_id: "wake-2".to_string(),
                session_id: worker.session_id.clone(),
                actions: vec![BrainAction::DeliverCompletion {
                    packet: CompletionPacket {
                        session_id: SessionId::new("other-session"),
                        status: CompletionStatus::Completed,
                        summary: "wrong session".to_string(),
                    },
                }],
            })
            .unwrap();

        assert_eq!(receipt.accepted_actions, 0);
        assert_eq!(receipt.rejected_actions.len(), 1);
        assert_eq!(
            receipt.rejected_actions[0].kind,
            CoreErrorKind::InvalidInput
        );

        let body = engine.project_body_state(&worker.session_id).unwrap();
        assert!(!body
            .recent_events
            .iter()
            .any(|event| matches!(event, CoreEvent::CompletionPacketDelivered { .. })));
    }

    #[test]
    fn rejects_malformed_delegation_before_side_effects() {
        let data_dir = unique_data_dir("invalid-delegation");
        let engine = test_engine_with_data_dir(data_dir.clone());
        let planner = engine
            .create_session(session_config(
                "planner-session",
                "planner",
                "planner-profile",
                SessionKind::Full,
            ))
            .unwrap();

        let receipt = engine
            .execute_brain_actions(BrainActionBatch {
                wake_id: "planner-wake".to_string(),
                session_id: planner.session_id.clone(),
                actions: vec![BrainAction::RequestDelegation {
                    profile_id: ProfileId::new("coder-profile"),
                    task_id: None,
                    prompt: "try malformed delegation".to_string(),
                    expected_output: Some(" ".to_string()),
                    resource_limits: Some(ResourceLimits {
                        workdir: None,
                        max_duration_ms: Some(0),
                        max_delegation_depth: Some(0),
                    }),
                    timeout_ms: Some(0),
                    priority: None,
                    fan_out_group_id: None,
                    fan_out_max_concurrency: None,
                    fan_out_failure_policy: None,
                    correlation_id: None,
                    parent_consumption: None,
                    capacity_request: None,
                }],
            })
            .unwrap();

        assert_eq!(receipt.accepted_actions, 0);
        assert_eq!(receipt.rejected_actions.len(), 1);
        assert_eq!(
            receipt.rejected_actions[0].kind,
            CoreErrorKind::InvalidInput
        );
        assert!(engine
            .delegated_sessions_for_parent(&planner.session_id)
            .unwrap()
            .is_empty());

        let store = CoordinationStore::open(data_dir).unwrap();
        assert_eq!(store.count_rows("sessions").unwrap(), 1);
        assert_eq!(store.count_rows("worker_runs").unwrap(), 0);
    }

    #[test]
    fn delegation_retry_does_not_duplicate_child_session() {
        let data_dir = unique_data_dir("delegation-idempotency");
        let engine = test_engine_with_data_dir(data_dir.clone());
        let planner = engine
            .create_session(session_config(
                "planner-session",
                "planner",
                "planner-profile",
                SessionKind::Full,
            ))
            .unwrap();
        let batch = BrainActionBatch {
            wake_id: "planner-wake".to_string(),
            session_id: planner.session_id.clone(),
            actions: vec![BrainAction::RequestDelegation {
                profile_id: ProfileId::new("coder-profile"),
                task_id: None,
                prompt: "retry-safe delegation".to_string(),
                expected_output: None,
                resource_limits: None,
                timeout_ms: None,
                priority: None,
                fan_out_group_id: None,
                fan_out_max_concurrency: None,
                fan_out_failure_policy: None,
                correlation_id: None,
                parent_consumption: None,
                capacity_request: None,
            }],
        };

        engine.execute_brain_actions(batch.clone()).unwrap();
        drop(engine);

        let restarted_engine = test_engine_with_data_dir(data_dir.clone());
        restarted_engine.execute_brain_actions(batch).unwrap();

        let store = CoordinationStore::open(data_dir).unwrap();
        assert_eq!(store.count_rows("sessions").unwrap(), 2);
        assert_eq!(store.count_rows("worker_runs").unwrap(), 1);
        assert_eq!(
            restarted_engine
                .delegated_sessions_for_parent(&planner.session_id)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn delegation_depth_zero_rejects_before_side_effects() {
        let data_dir = unique_data_dir("delegation-depth");
        let engine = test_engine_with_data_dir(data_dir.clone());
        let mut config = session_config(
            "planner-session",
            "planner",
            "planner-profile",
            SessionKind::Full,
        );
        config.resource_limits.max_delegation_depth = Some(0);
        let planner = engine.create_session(config).unwrap();

        let receipt = engine
            .execute_brain_actions(BrainActionBatch {
                wake_id: "planner-wake".to_string(),
                session_id: planner.session_id.clone(),
                actions: vec![BrainAction::RequestDelegation {
                    profile_id: ProfileId::new("coder-profile"),
                    task_id: None,
                    prompt: "should not spawn".to_string(),
                    expected_output: None,
                    resource_limits: None,
                    timeout_ms: None,
                    priority: None,
                    fan_out_group_id: None,
                    fan_out_max_concurrency: None,
                    fan_out_failure_policy: None,
                    correlation_id: None,
                    parent_consumption: None,
                    capacity_request: None,
                }],
            })
            .unwrap();

        assert_eq!(receipt.accepted_actions, 0);
        assert_eq!(
            receipt.rejected_actions[0].kind,
            CoreErrorKind::ActionRejected
        );

        let store = CoordinationStore::open(data_dir).unwrap();
        assert_eq!(store.count_rows("sessions").unwrap(), 1);
        assert_eq!(store.count_rows("worker_runs").unwrap(), 0);
    }

    #[test]
    fn injects_den_and_external_events_into_the_bus() {
        let engine = test_engine();
        let (_subscription_id, events) = engine
            .subscribe_events(EventSubscription {
                event_kinds: vec![
                    CoreEventKind::DenDataUpdated,
                    CoreEventKind::ExternalEventInjected,
                ],
                session_id: None,
                agent_id: None,
                adapter_id: None,
            })
            .unwrap();

        let den_receipt = engine
            .inject_den_data_update(DenDataUpdate {
                project_id: ProjectId::new("pi-crew"),
                entity_kind: "task".to_string(),
                entity_id: "2767".to_string(),
                revision: Some("rev-1".to_string()),
            })
            .unwrap();
        let external_receipt = engine
            .inject_external_event(ExternalEvent {
                adapter_id: AdapterId::new("den"),
                source: "den".to_string(),
                payload: ExternalEventPayload::AdapterStatus {
                    status: "connected".to_string(),
                    detail: None,
                },
            })
            .unwrap();

        assert!(den_receipt.accepted);
        assert!(external_receipt.accepted);
        assert!(external_receipt.sequence > den_receipt.sequence);
        assert!(matches!(
            events.recv_timeout(Duration::from_secs(1)).unwrap(),
            CoreEvent::DenDataUpdated { .. }
        ));
        assert!(matches!(
            events.recv_timeout(Duration::from_secs(1)).unwrap(),
            CoreEvent::ExternalEventInjected { .. }
        ));
    }

    #[test]
    fn submits_brain_events_into_core_event_handling() {
        let engine = test_engine();
        let (_subscription_id, events) = engine
            .subscribe_events(EventSubscription {
                event_kinds: vec![CoreEventKind::BrainEventObserved],
                session_id: Some(SessionId::new("brain-session")),
                agent_id: None,
                adapter_id: None,
            })
            .unwrap();

        let receipt = engine
            .submit_brain_event(BrainEventEnvelope {
                wake_id: "wake-1".to_string(),
                session_id: SessionId::new("brain-session"),
                event: BrainEvent::TextDelta {
                    text: "streaming".to_string(),
                },
            })
            .unwrap();

        assert!(receipt.accepted);
        assert!(matches!(
            events.recv_timeout(Duration::from_secs(1)).unwrap(),
            CoreEvent::BrainEventObserved {
                wake_id: Some(wake_id),
                event: BrainEvent::TextDelta { .. },
                ..
            } if wake_id == "wake-1"
        ));
    }

    #[test]
    fn persists_tool_call_telemetry_with_wake_context() {
        let engine = test_engine();

        engine
            .submit_brain_event(BrainEventEnvelope {
                wake_id: "wake-tools".to_string(),
                session_id: SessionId::new("brain-session"),
                event: BrainEvent::ToolCallStarted {
                    tool_name: "read_file".to_string(),
                    metadata: None,
                },
            })
            .unwrap();
        engine
            .submit_brain_event(BrainEventEnvelope {
                wake_id: "wake-tools".to_string(),
                session_id: SessionId::new("brain-session"),
                event: BrainEvent::ToolCallFinished {
                    tool_name: "read_file".to_string(),
                    is_error: false,
                    metadata: None,
                },
            })
            .unwrap();

        let records = engine.store.load_tool_call_history().unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].session_id, SessionId::new("brain-session"));
        assert_eq!(records[0].wake_id.as_deref(), Some("wake-tools"));
        assert_eq!(records[0].tool_name, "read_file");
        assert_eq!(records[0].phase, ToolCallPhase::Started);
        assert_eq!(records[0].is_error, None);
        assert_eq!(records[1].phase, ToolCallPhase::Finished);
        assert_eq!(records[1].is_error, Some(false));
    }

    #[test]
    fn persists_mcp_tool_metadata_without_payloads() {
        let engine = test_engine();
        let metadata = ToolCallMetadata {
            source: ToolCallSource::Mcp,
            adapter_id: Some(AdapterId::new("adapter-mcp")),
            binding_id: Some("binding-alpha".to_string()),
            server_names: vec!["filesystem".to_string()],
            profile_id: Some(ProfileId::new("profile-alpha")),
            tool_profile_key: Some("profile-tools".to_string()),
            source_tool_name: Some("read_file".to_string()),
            catalog_revision: Some("rev-1".to_string()),
            debug_detail_id: None,
            policy: Some(ToolCallPolicyMetadata {
                allowed: Some(true),
                denial_reason: None,
                timeout_ms: Some(5_000),
                cancelled: Some(false),
                archive_cleanup: Some(false),
            }),
        };

        engine
            .submit_brain_event(BrainEventEnvelope {
                wake_id: "wake-mcp".to_string(),
                session_id: SessionId::new("brain-session"),
                event: BrainEvent::ToolCallStarted {
                    tool_name: "mcp_read_file".to_string(),
                    metadata: Some(metadata.clone()),
                },
            })
            .unwrap();

        let records = engine.store.load_tool_call_history().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].tool_name, "mcp_read_file");
        assert_eq!(records[0].metadata, Some(metadata));
    }

    #[test]
    fn persists_web_browser_tool_metadata_without_payloads() {
        let engine = test_engine();
        let web_metadata = ToolCallMetadata {
            source: ToolCallSource::Web,
            adapter_id: None,
            binding_id: None,
            server_names: vec![],
            profile_id: Some(ProfileId::new("profile-web")),
            tool_profile_key: None,
            source_tool_name: Some("web_extract".to_string()),
            catalog_revision: None,
            debug_detail_id: None,
            policy: Some(ToolCallPolicyMetadata {
                allowed: Some(false),
                denial_reason: Some("network_denied".to_string()),
                timeout_ms: Some(5_000),
                cancelled: Some(false),
                archive_cleanup: Some(false),
            }),
        };
        let browser_metadata = ToolCallMetadata {
            source: ToolCallSource::Browser,
            adapter_id: None,
            binding_id: None,
            server_names: vec![],
            profile_id: Some(ProfileId::new("profile-browser")),
            tool_profile_key: None,
            source_tool_name: Some("browser_vision".to_string()),
            catalog_revision: None,
            debug_detail_id: None,
            policy: Some(ToolCallPolicyMetadata {
                allowed: Some(true),
                denial_reason: None,
                timeout_ms: Some(8_000),
                cancelled: Some(false),
                archive_cleanup: Some(false),
            }),
        };

        engine
            .submit_brain_event(BrainEventEnvelope {
                wake_id: "wake-web-browser".to_string(),
                session_id: SessionId::new("brain-session"),
                event: BrainEvent::ToolCallStarted {
                    tool_name: "web_extract".to_string(),
                    metadata: Some(web_metadata.clone()),
                },
            })
            .unwrap();
        engine
            .submit_brain_event(BrainEventEnvelope {
                wake_id: "wake-web-browser".to_string(),
                session_id: SessionId::new("brain-session"),
                event: BrainEvent::ToolCallFinished {
                    tool_name: "browser_vision".to_string(),
                    is_error: false,
                    metadata: Some(browser_metadata.clone()),
                },
            })
            .unwrap();

        let records = engine.store.load_tool_call_history().unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].metadata, Some(web_metadata));
        assert_eq!(records[1].metadata, Some(browser_metadata));
        let web_json = serde_json::to_string(&records[0].metadata).unwrap();
        let browser_json = serde_json::to_string(&records[1].metadata).unwrap();
        assert!(!web_json.contains("page content"));
        assert!(!browser_json.contains("base64"));
        assert!(!browser_json.contains("screenshot"));
    }

    #[test]
    fn den_observability_is_not_required_for_internal_routing() {
        let engine = test_engine();
        let worker = engine
            .create_session(session_config(
                "worker-session",
                "worker",
                "coder-profile",
                SessionKind::Worker,
            ))
            .unwrap();

        engine
            .inject_external_event(ExternalEvent {
                adapter_id: AdapterId::new("den"),
                source: "den-observability".to_string(),
                payload: ExternalEventPayload::AdapterStatus {
                    status: "disconnected".to_string(),
                    detail: Some("projection sink unavailable".to_string()),
                },
            })
            .unwrap();

        engine
            .bus()
            .route_message(
                AgentId::new("planner"),
                worker.agent_id.clone(),
                "routing continues without den",
            )
            .unwrap();

        let body = engine.project_body_state(&worker.session_id).unwrap();

        assert_eq!(body.pending_messages.len(), 1);
        assert_eq!(
            body.pending_messages[0].body,
            "routing continues without den"
        );
    }

    #[test]
    fn hydrates_persisted_coordination_state_on_restart() {
        let data_dir = unique_data_dir("hydrate");
        let first_engine = test_engine_with_data_dir(data_dir.clone());
        let planner = first_engine
            .create_session(session_config(
                "planner-session",
                "planner",
                "planner-profile",
                SessionKind::Full,
            ))
            .unwrap();
        let worker = first_engine
            .create_session(session_config(
                "worker-session",
                "worker",
                "coder-profile",
                SessionKind::Worker,
            ))
            .unwrap();

        first_engine
            .execute_brain_actions(BrainActionBatch {
                wake_id: "planner-wake".to_string(),
                session_id: planner.session_id.clone(),
                actions: vec![
                    BrainAction::SendMessage {
                        message: AgentMessage {
                            from: planner.agent_id.clone(),
                            to: worker.agent_id.clone(),
                            body: "please keep working after restart".to_string(),
                            correlation_id: Some("persisted-message".to_string()),
                            projection: None,
                        },
                    },
                    BrainAction::RequestDelegation {
                        profile_id: ProfileId::new("coder-profile"),
                        task_id: Some(rusty_crew_core_protocol::TaskId::new("2768")),
                        prompt: "persist the coordination state".to_string(),
                        expected_output: None,
                        resource_limits: None,
                        timeout_ms: None,
                        priority: None,
                        fan_out_group_id: None,
                        fan_out_max_concurrency: None,
                        fan_out_failure_policy: None,
                        correlation_id: None,
                        parent_consumption: None,
                        capacity_request: None,
                    },
                ],
            })
            .unwrap();
        first_engine
            .execute_brain_actions(BrainActionBatch {
                wake_id: "worker-wake".to_string(),
                session_id: worker.session_id.clone(),
                actions: vec![BrainAction::DeliverCompletion {
                    packet: CompletionPacket {
                        session_id: worker.session_id.clone(),
                        status: CompletionStatus::Completed,
                        summary: "persisted packet".to_string(),
                    },
                }],
            })
            .unwrap();

        drop(first_engine);

        let restarted_engine = test_engine_with_data_dir(data_dir.clone());
        let hydrated_planner = restarted_engine
            .get_session(&planner.session_id)
            .expect("planner session should hydrate");
        let hydrated_worker = restarted_engine
            .get_session(&worker.session_id)
            .expect("worker session should hydrate");
        let hydrated_delegated = restarted_engine
            .delegated_session_for_run(&RunId::new("planner-wake:1"))
            .expect("delegated run lookup should load")
            .expect("delegated session should hydrate");
        let hydrated_body = restarted_engine
            .project_body_state(&worker.session_id)
            .expect("worker body should hydrate from persisted bus history");
        let store = CoordinationStore::open(data_dir).unwrap();

        assert_eq!(hydrated_planner.kind, SessionKind::Full);
        assert_eq!(hydrated_worker.kind, SessionKind::Worker);
        assert_eq!(hydrated_delegated.kind, SessionKind::Delegated);
        assert_eq!(
            hydrated_delegated
                .delegation
                .as_ref()
                .map(|lineage| (&lineage.parent_session_id, lineage.source_action_index)),
            Some((&planner.session_id, 1))
        );
        assert_eq!(
            restarted_engine
                .delegated_sessions_for_parent(&planner.session_id)
                .unwrap(),
            vec![hydrated_delegated]
        );
        assert_eq!(hydrated_body.pending_messages.len(), 1);
        assert_eq!(
            hydrated_body.pending_messages[0].body,
            "please keep working after restart"
        );
        assert!(hydrated_body
            .recent_events
            .iter()
            .any(|event| matches!(event, CoreEvent::CompletionPacketDelivered { .. })));
        assert_eq!(store.count_rows("sessions").unwrap(), 3);
        assert_eq!(store.count_rows("agent_messages").unwrap(), 2);
        assert_eq!(store.count_rows("completion_packets").unwrap(), 1);
        assert_eq!(store.count_rows("worker_runs").unwrap(), 1);
    }

    #[test]
    fn restart_hydrates_many_agents_without_resurrecting_work() {
        let data_dir = unique_data_dir("many-agent-hydrate");
        let first_engine = test_engine_with_data_dir(data_dir.clone());
        let planner = first_engine
            .create_session(session_config(
                "planner-session",
                "planner",
                "planner-profile",
                SessionKind::Full,
            ))
            .unwrap();
        let reviewer = first_engine
            .create_session(session_config(
                "reviewer-session",
                "reviewer",
                "reviewer-profile",
                SessionKind::Full,
            ))
            .unwrap();

        first_engine
            .execute_brain_actions(BrainActionBatch {
                wake_id: "planner-wake".to_string(),
                session_id: planner.session_id.clone(),
                actions: vec![
                    BrainAction::SendMessage {
                        message: AgentMessage {
                            from: planner.agent_id.clone(),
                            to: reviewer.agent_id.clone(),
                            body: "please review restart hydration".to_string(),
                            correlation_id: Some("restart-review".to_string()),
                            projection: None,
                        },
                    },
                    BrainAction::RequestDelegation {
                        profile_id: ProfileId::new("coder-profile"),
                        task_id: Some(rusty_crew_core_protocol::TaskId::new("2874")),
                        prompt: "keep delegated work restart-safe".to_string(),
                        expected_output: Some("restart note".to_string()),
                        resource_limits: None,
                        timeout_ms: None,
                        priority: None,
                        fan_out_group_id: None,
                        fan_out_max_concurrency: None,
                        fan_out_failure_policy: None,
                        correlation_id: Some("delegated-restart".to_string()),
                        parent_consumption: None,
                        capacity_request: None,
                    },
                ],
            })
            .unwrap();
        first_engine
            .execute_brain_actions(BrainActionBatch {
                wake_id: "reviewer-wake".to_string(),
                session_id: reviewer.session_id.clone(),
                actions: vec![BrainAction::SendMessage {
                    message: AgentMessage {
                        from: reviewer.agent_id.clone(),
                        to: planner.agent_id.clone(),
                        body: "restart review acknowledged".to_string(),
                        correlation_id: Some("restart-review".to_string()),
                        projection: None,
                    },
                }],
            })
            .unwrap();

        let store_before_restart = CoordinationStore::open(data_dir.clone()).unwrap();
        let event_count_before = store_before_restart.count_rows("event_history").unwrap();
        let search_before = store_before_restart
            .search_runtime(&RuntimeSearchFilter {
                query: "hydration".to_string(),
                row_type: Some(RuntimeSearchRowType::Message),
                session_id: None,
                agent_id: Some(reviewer.agent_id.clone()),
                instance_id: None,
                task_id: None,
                event_kind: Some(CoreEventKind::AgentMessageRouted),
                recorded_after: None,
                recorded_before: None,
                limit: Some(10),
            })
            .unwrap();
        assert_eq!(search_before.len(), 1);
        drop(first_engine);
        drop(store_before_restart);

        let restarted_engine = test_engine_with_data_dir(data_dir.clone());
        let hydrated_planner = restarted_engine.get_session(&planner.session_id).unwrap();
        let hydrated_reviewer = restarted_engine.get_session(&reviewer.session_id).unwrap();
        let hydrated_delegated = restarted_engine
            .delegated_session_for_run(&RunId::new("planner-wake:1"))
            .unwrap()
            .unwrap();
        let reviewer_body = restarted_engine
            .project_body_state(&reviewer.session_id)
            .unwrap();
        let planner_body = restarted_engine
            .project_body_state(&planner.session_id)
            .unwrap();
        let store_after_restart = CoordinationStore::open(data_dir).unwrap();

        assert_eq!(hydrated_planner.status, SessionStatus::Idle);
        assert_eq!(hydrated_reviewer.status, SessionStatus::Idle);
        assert_eq!(hydrated_delegated.kind, SessionKind::Delegated);
        assert_eq!(
            hydrated_delegated
                .delegation
                .as_ref()
                .map(|lineage| (&lineage.parent_session_id, lineage.source_wake_id.as_str())),
            Some((&planner.session_id, "planner-wake"))
        );
        assert!(reviewer_body
            .pending_messages
            .iter()
            .any(|message| message.body == "please review restart hydration"));
        assert!(planner_body
            .pending_messages
            .iter()
            .any(|message| message.body == "restart review acknowledged"));
        assert_eq!(
            store_after_restart.count_rows("event_history").unwrap(),
            event_count_before
        );
        assert_eq!(
            store_after_restart.load_agent_identities().unwrap().len(),
            3
        );
        assert_eq!(store_after_restart.load_session_configs().unwrap().len(), 3);
        assert_eq!(
            store_after_restart
                .runtime_summary(&RuntimeCounterScope::Runtime)
                .unwrap()
                .messages,
            3
        );
        assert_eq!(
            store_after_restart
                .runtime_summary(&RuntimeCounterScope::Runtime)
                .unwrap()
                .wakes,
            1
        );
        assert_eq!(
            store_after_restart
                .search_runtime(&RuntimeSearchFilter {
                    query: "hydration".to_string(),
                    row_type: Some(RuntimeSearchRowType::Message),
                    session_id: None,
                    agent_id: Some(reviewer.agent_id),
                    instance_id: None,
                    task_id: None,
                    event_kind: Some(CoreEventKind::AgentMessageRouted),
                    recorded_after: None,
                    recorded_before: None,
                    limit: Some(10),
                })
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn multi_agent_restart_search_queue_and_query_apis_prove_persistence_substrate() {
        let data_dir = unique_data_dir("persistence-substrate-proof");
        let first_engine = test_engine_with_data_dir(data_dir.clone());
        let planner = first_engine
            .create_session(session_config(
                "planner-session",
                "planner",
                "planner-profile",
                SessionKind::Full,
            ))
            .unwrap();
        let reviewer = first_engine
            .create_session(session_config(
                "reviewer-session",
                "reviewer",
                "reviewer-profile",
                SessionKind::Full,
            ))
            .unwrap();
        let observer = first_engine
            .create_session(session_config(
                "observer-session",
                "observer",
                "observer-profile",
                SessionKind::Full,
            ))
            .unwrap();
        first_engine
            .register_profile_tool_profile(
                ProfileId::new("proof-coder-profile"),
                ToolProfile {
                    tools: vec![ToolDescriptor {
                        name: "patch".to_string(),
                        description: "Apply a bounded patch".to_string(),
                        input_schema: None,
                    }],
                },
            )
            .unwrap();

        first_engine
            .execute_brain_actions(BrainActionBatch {
                wake_id: "proof-planner-wake".to_string(),
                session_id: planner.session_id.clone(),
                actions: vec![
                    BrainAction::SendMessage {
                        message: AgentMessage {
                            from: planner.agent_id.clone(),
                            to: reviewer.agent_id.clone(),
                            body: "please review the persistent proof".to_string(),
                            correlation_id: Some("proof-thread".to_string()),
                            projection: None,
                        },
                    },
                    BrainAction::RequestDelegation {
                        profile_id: ProfileId::new("proof-coder-profile"),
                        task_id: Some(rusty_crew_core_protocol::TaskId::new("2879")),
                        prompt: "complete the e2e delegated persistence proof".to_string(),
                        expected_output: Some("proof completion".to_string()),
                        resource_limits: Some(ResourceLimits {
                            workdir: Some("/home/dev/rusty-crew".to_string()),
                            max_duration_ms: Some(30_000),
                            max_delegation_depth: Some(0),
                        }),
                        timeout_ms: Some(30_000),
                        priority: None,
                        fan_out_group_id: None,
                        fan_out_max_concurrency: None,
                        fan_out_failure_policy: None,
                        correlation_id: Some("proof-delegation".to_string()),
                        parent_consumption: Some(ParentConsumptionPolicy::AwaitCompletion),
                        capacity_request: None,
                    },
                ],
            })
            .unwrap();
        first_engine
            .execute_brain_actions(BrainActionBatch {
                wake_id: "proof-reviewer-wake".to_string(),
                session_id: reviewer.session_id.clone(),
                actions: vec![BrainAction::SendMessage {
                    message: AgentMessage {
                        from: reviewer.agent_id.clone(),
                        to: observer.agent_id.clone(),
                        body: "persistent proof review forwarded".to_string(),
                        correlation_id: Some("proof-thread".to_string()),
                        projection: None,
                    },
                }],
            })
            .unwrap();

        let delegated_session_id =
            delegated_session_id(&planner.session_id, "proof-planner-wake", 1);
        first_engine
            .submit_brain_event(BrainEventEnvelope {
                session_id: delegated_session_id.clone(),
                wake_id: "proof-child-wake".to_string(),
                event: BrainEvent::Started,
            })
            .unwrap();
        first_engine
            .submit_brain_event(BrainEventEnvelope {
                session_id: delegated_session_id.clone(),
                wake_id: "proof-child-wake".to_string(),
                event: BrainEvent::ToolCallStarted {
                    tool_name: "patch".to_string(),
                    metadata: None,
                },
            })
            .unwrap();
        first_engine
            .submit_brain_event(BrainEventEnvelope {
                session_id: delegated_session_id.clone(),
                wake_id: "proof-child-wake".to_string(),
                event: BrainEvent::ToolCallFinished {
                    tool_name: "patch".to_string(),
                    is_error: false,
                    metadata: None,
                },
            })
            .unwrap();
        first_engine
            .execute_brain_actions(BrainActionBatch {
                wake_id: "proof-child-completion".to_string(),
                session_id: delegated_session_id.clone(),
                actions: vec![BrainAction::DeliverCompletion {
                    packet: CompletionPacket {
                        session_id: delegated_session_id.clone(),
                        status: CompletionStatus::Completed,
                        summary: "proof child completed".to_string(),
                    },
                }],
            })
            .unwrap();

        let store_before_restart = CoordinationStore::open(data_dir.clone()).unwrap();
        store_before_restart
            .save_queued_message(&QueuedMessageRecord {
                message_id: "expired-proof-queue".to_string(),
                owner_session_id: Some(planner.session_id.clone()),
                owner_agent_id: planner.agent_id.clone(),
                message: AgentMessage {
                    from: AgentId::new("operator"),
                    to: planner.agent_id.clone(),
                    body: "expired proof queue item".to_string(),
                    correlation_id: Some("proof-queue".to_string()),
                    projection: None,
                },
                source_sequence: None,
                enqueued_at: "2026-06-19T00:00:00Z".to_string(),
                expires_at: "2026-06-19T00:00:01Z".to_string(),
                ttl_ms: 1_000,
                delivery_attempts: 0,
                state: QueuedMessageState::Pending,
                terminal_at: None,
                state_reason: None,
            })
            .unwrap();
        store_before_restart
            .save_queued_message(&QueuedMessageRecord {
                message_id: "future-proof-queue".to_string(),
                owner_session_id: Some(planner.session_id.clone()),
                owner_agent_id: planner.agent_id.clone(),
                message: AgentMessage {
                    from: AgentId::new("operator"),
                    to: planner.agent_id.clone(),
                    body: "future proof queue item".to_string(),
                    correlation_id: Some("proof-queue".to_string()),
                    projection: None,
                },
                source_sequence: None,
                enqueued_at: "2026-06-19T00:00:00Z".to_string(),
                expires_at: "2026-06-19T00:10:00Z".to_string(),
                ttl_ms: 600_000,
                delivery_attempts: 0,
                state: QueuedMessageState::Pending,
                terminal_at: None,
                state_reason: None,
            })
            .unwrap();
        drop(store_before_restart);
        drop(first_engine);

        let restarted_engine = test_engine_with_data_dir(data_dir.clone());
        let hydrated_planner = restarted_engine.get_session(&planner.session_id).unwrap();
        let hydrated_reviewer = restarted_engine.get_session(&reviewer.session_id).unwrap();
        let hydrated_observer = restarted_engine.get_session(&observer.session_id).unwrap();
        let hydrated_delegated = restarted_engine
            .get_session(&delegated_session_id)
            .expect("delegated session should hydrate");
        let planner_body = restarted_engine
            .project_body_state(&planner.session_id)
            .unwrap();
        let observer_body = restarted_engine
            .project_body_state(&observer.session_id)
            .unwrap();
        let store_after_restart = CoordinationStore::open(data_dir).unwrap();

        assert_eq!(hydrated_planner.kind, SessionKind::Full);
        assert_eq!(hydrated_reviewer.kind, SessionKind::Full);
        assert_eq!(hydrated_observer.kind, SessionKind::Full);
        assert_eq!(hydrated_delegated.kind, SessionKind::Delegated);
        assert_eq!(
            hydrated_delegated
                .delegation
                .as_ref()
                .map(|lineage| (&lineage.parent_session_id, lineage.source_action_index)),
            Some((&planner.session_id, 1))
        );
        assert!(planner_body
            .child_completions
            .iter()
            .any(|completion| completion.packet.summary == "proof child completed"));
        assert!(observer_body
            .pending_messages
            .iter()
            .any(|message| message.body == "persistent proof review forwarded"));

        let maintenance = store_after_restart
            .run_maintenance(&RuntimeMaintenancePolicy {
                expire_queued_messages_at: Some("2026-06-19T00:00:02Z".to_string()),
                purge_terminal_queued_messages_before: None,
                expire_provider_wire_states_at: None,
                run_wal_checkpoint: true,
                run_optimize: true,
                ..RuntimeMaintenancePolicy::default()
            })
            .unwrap();
        assert_eq!(maintenance.expired_queue_messages, 1);
        assert!(maintenance.size_after.database_bytes > 0);
        assert_eq!(
            store_after_restart
                .load_queued_messages(&QueuedMessageFilter {
                    state: Some(QueuedMessageState::Pending),
                    owner_session_id: Some(planner.session_id.clone()),
                    owner_agent_id: None,
                    limit: None,
                })
                .unwrap()[0]
                .message_id,
            "future-proof-queue"
        );
        assert_eq!(
            store_after_restart
                .load_queued_messages(&QueuedMessageFilter {
                    state: Some(QueuedMessageState::Expired),
                    owner_session_id: Some(planner.session_id.clone()),
                    owner_agent_id: None,
                    limit: None,
                })
                .unwrap()[0]
                .message_id,
            "expired-proof-queue"
        );

        assert_eq!(
            store_after_restart
                .query_sessions(&SessionQuery {
                    kind: Some(SessionKind::Full),
                    page: Some(QueryPage {
                        limit: Some(10),
                        offset: Some(0),
                    }),
                    ..SessionQuery::default()
                })
                .unwrap()
                .len(),
            3
        );
        assert_eq!(
            store_after_restart
                .query_agent_messages(&AgentMessageQuery {
                    agent_id: Some(reviewer.agent_id.clone()),
                    correlation_id: Some("proof-thread".to_string()),
                    page: None,
                })
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            store_after_restart
                .query_completion_packets(&CompletionPacketQuery {
                    session_id: Some(delegated_session_id.clone()),
                    status: Some(CompletionStatus::Completed),
                    page: None,
                })
                .unwrap()[0]
                .packet
                .summary,
            "proof child completed"
        );
        assert_eq!(
            store_after_restart
                .query_worker_runs(&WorkerRunQuery {
                    parent_session_id: Some(planner.session_id.clone()),
                    delegated_session_id: Some(delegated_session_id.clone()),
                    status: Some(WorkerRunStatus::Completed),
                    ..WorkerRunQuery::default()
                })
                .unwrap()
                .len(),
            1
        );
        let runtime_summary = store_after_restart
            .runtime_summary(&RuntimeCounterScope::Runtime)
            .unwrap();
        assert_eq!(runtime_summary.messages, 3);
        assert_eq!(runtime_summary.tool_calls, 1);
        assert_eq!(runtime_summary.completions, 1);
        assert_eq!(runtime_summary.delegations_created, 1);
        assert_eq!(runtime_summary.delegations_completed, 1);
        assert_eq!(runtime_summary.queue_expirations, 1);
        assert_eq!(
            store_after_restart
                .runtime_summary(&RuntimeCounterScope::Session(delegated_session_id.clone()))
                .unwrap()
                .tool_calls,
            1
        );
        assert_eq!(
            store_after_restart
                .search_runtime(&RuntimeSearchFilter {
                    query: "persistent proof".to_string(),
                    row_type: Some(RuntimeSearchRowType::Message),
                    session_id: None,
                    agent_id: Some(reviewer.agent_id.clone()),
                    instance_id: None,
                    task_id: None,
                    event_kind: Some(CoreEventKind::AgentMessageRouted),
                    recorded_after: None,
                    recorded_before: None,
                    limit: Some(10),
                })
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            store_after_restart
                .search_runtime(&RuntimeSearchFilter {
                    query: "expired proof queue".to_string(),
                    row_type: Some(RuntimeSearchRowType::QueueMessage),
                    session_id: Some(planner.session_id),
                    agent_id: Some(planner.agent_id),
                    instance_id: None,
                    task_id: None,
                    event_kind: None,
                    recorded_after: None,
                    recorded_before: None,
                    limit: Some(10),
                })
                .unwrap()
                .len(),
            1
        );
        assert!(store_after_restart
            .hot_query_plan_checks()
            .unwrap()
            .iter()
            .all(|check| check.uses_index));
    }

    #[test]
    fn delegated_completion_packets_route_to_parent_body_and_policy_wake() {
        let data_dir = unique_data_dir("delegated-completion-routing");
        let engine = test_engine_with_data_dir(data_dir.clone());
        let planner = engine
            .create_session(session_config(
                "planner-session",
                "planner",
                "planner-profile",
                SessionKind::Full,
            ))
            .unwrap();
        let cases = [
            (
                CompletionStatus::Completed,
                ParentConsumptionPolicy::AwaitCompletion,
            ),
            (
                CompletionStatus::Failed,
                ParentConsumptionPolicy::AwaitCompletion,
            ),
            (
                CompletionStatus::Blocked,
                ParentConsumptionPolicy::AwaitCompletion,
            ),
            (
                CompletionStatus::Exhausted,
                ParentConsumptionPolicy::ObserveOnly,
            ),
        ];

        engine
            .execute_brain_actions(BrainActionBatch {
                wake_id: "planner-wake".to_string(),
                session_id: planner.session_id.clone(),
                actions: cases
                    .iter()
                    .enumerate()
                    .map(
                        |(index, (_status, policy))| BrainAction::RequestDelegation {
                            profile_id: ProfileId::new(format!("coder-profile-{index}")),
                            task_id: Some(rusty_crew_core_protocol::TaskId::new(format!(
                                "task-{index}"
                            ))),
                            prompt: format!("complete delegated slice {index}"),
                            expected_output: Some("completion packet".to_string()),
                            resource_limits: None,
                            timeout_ms: None,
                            priority: None,
                            fan_out_group_id: Some("completion-routing".to_string()),
                            fan_out_max_concurrency: None,
                            fan_out_failure_policy: None,
                            correlation_id: Some(format!("correlation-{index}")),
                            parent_consumption: Some(policy.clone()),
                            capacity_request: None,
                        },
                    )
                    .collect(),
            })
            .unwrap();

        let (_subscription_id, parent_wakes) = engine
            .subscribe_events(EventSubscription {
                event_kinds: vec![CoreEventKind::BrainWakeRequested],
                session_id: Some(planner.session_id.clone()),
                agent_id: None,
                adapter_id: None,
            })
            .unwrap();

        for (index, (status, _policy)) in cases.iter().enumerate() {
            let child_session_id = delegated_session_id(&planner.session_id, "planner-wake", index);
            engine
                .execute_brain_actions(BrainActionBatch {
                    wake_id: format!("child-wake-{index}"),
                    session_id: child_session_id.clone(),
                    actions: vec![BrainAction::DeliverCompletion {
                        packet: CompletionPacket {
                            session_id: child_session_id,
                            status: status.clone(),
                            summary: format!("child {index} finished as {status:?}"),
                        },
                    }],
                })
                .unwrap();
        }

        for _ in 0..3 {
            assert!(matches!(
                parent_wakes.recv_timeout(Duration::from_secs(1)).unwrap(),
                CoreEvent::BrainWakeRequested { session_id } if session_id == planner.session_id
            ));
        }
        assert!(parent_wakes
            .recv_timeout(Duration::from_millis(50))
            .is_err());

        let body = engine.project_body_state(&planner.session_id).unwrap();
        assert_eq!(body.child_completions.len(), 4);
        assert_eq!(
            body.child_completions
                .iter()
                .map(|completion| completion.packet.status.clone())
                .collect::<Vec<_>>(),
            cases
                .iter()
                .map(|(status, _policy)| status.clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            body.child_completions
                .iter()
                .map(|completion| completion.parent_consumption.clone())
                .collect::<Vec<_>>(),
            cases
                .iter()
                .map(|(_status, policy)| policy.clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            body.child_completions[0].run_id,
            RunId::new("planner-wake:0")
        );
        assert_eq!(
            body.child_completions[3].child_session_id,
            delegated_session_id(&planner.session_id, "planner-wake", 3)
        );
        assert_eq!(
            body.child_completions[3].correlation_id.as_deref(),
            Some("correlation-3")
        );

        drop(engine);

        let restarted_engine = test_engine_with_data_dir(data_dir);
        let restarted_body = restarted_engine
            .project_body_state(&planner.session_id)
            .expect("parent completion state should hydrate");
        assert_eq!(restarted_body.child_completions, body.child_completions);
    }

    #[test]
    fn delegated_checkpoint_request_routes_message_and_wake_to_child() {
        let engine = test_engine();
        let planner = engine
            .create_session(session_config(
                "planner-session",
                "planner",
                "planner-profile",
                SessionKind::Full,
            ))
            .unwrap();
        let delegated_session_id = spawn_delegated(&engine, &planner, "planner-wake", Some(30_000));

        let receipt = engine
            .request_delegated_checkpoint(
                &planner.session_id,
                &delegated_session_id,
                "send a progress packet",
            )
            .unwrap();
        assert!(receipt.accepted);

        let body = engine.project_body_state(&delegated_session_id).unwrap();
        assert!(body.pending_messages.iter().any(|message| {
            message.body == "Checkpoint requested: send a progress packet"
                && message.correlation_id.as_deref()
                    == Some("checkpoint:planner-session:delegated:planner-wake:0")
        }));
        assert!(body.recent_events.iter().any(|event| {
            matches!(event, CoreEvent::BrainWakeRequested { session_id } if session_id == &delegated_session_id)
        }));
        assert!(body.recent_events.iter().any(|event| {
            matches!(
                event,
                CoreEvent::DelegationLifecycleObserved { lifecycle }
                    if lifecycle.delegated_session_id == delegated_session_id
                        && lifecycle.phase == DelegationLifecyclePhase::CheckpointRequested
            )
        }));
        let status = engine
            .delegated_session_status(&delegated_session_id)
            .unwrap();
        assert_eq!(status.parent_session_id.as_ref(), Some(&planner.session_id));
        assert_eq!(
            status.run_status,
            Some(DelegatedRunStatus::CheckpointWaiting)
        );
        assert!(!status.terminal);
    }

    #[test]
    fn delegated_session_timeout_expires_without_completion_packet() {
        let data_dir = unique_data_dir("delegated-timeout");
        let engine = test_engine_with_data_dir(data_dir.clone());
        let planner = engine
            .create_session(session_config(
                "planner-session",
                "planner",
                "planner-profile",
                SessionKind::Full,
            ))
            .unwrap();
        let delegated_session_id = spawn_delegated(&engine, &planner, "planner-wake", Some(10));

        assert!(engine
            .expire_delegated_sessions_at("2026-06-19T00:00:00.009Z".to_string())
            .unwrap()
            .is_empty());
        assert_eq!(
            engine
                .expire_delegated_sessions_at("2026-06-19T00:00:00.010Z".to_string())
                .unwrap(),
            vec![delegated_session_id.clone()]
        );

        assert_eq!(
            engine.get_session(&delegated_session_id).unwrap().status,
            SessionStatus::Archived
        );
        let store = CoordinationStore::open(data_dir).unwrap();
        assert_eq!(
            store
                .load_worker_run_by_delegated_session(&delegated_session_id)
                .unwrap()
                .unwrap()
                .status,
            WorkerRunStatus::Expired
        );
        assert_eq!(store.count_rows("completion_packets").unwrap(), 0);
        let body = engine.project_body_state(&delegated_session_id).unwrap();
        assert!(body.recent_events.iter().any(|event| {
            matches!(
                event,
                CoreEvent::DelegationLifecycleObserved { lifecycle }
                    if lifecycle.delegated_session_id == delegated_session_id
                        && lifecycle.phase == DelegationLifecyclePhase::TimedOut
            )
        }));
    }

    #[test]
    fn delegated_resource_cleanup_archives_terminal_sessions() {
        let data_dir = unique_data_dir("delegated-resource-cleanup");
        let engine = test_engine_with_data_dir(data_dir.clone());
        let planner = engine
            .create_session(session_config(
                "planner-session",
                "planner",
                "planner-profile",
                SessionKind::Full,
            ))
            .unwrap();
        let terminal = spawn_delegated(&engine, &planner, "planner-wake-terminal", Some(30_000));

        engine
            .execute_brain_actions(BrainActionBatch {
                wake_id: "terminal-wake".to_string(),
                session_id: terminal.clone(),
                actions: vec![BrainAction::DeliverCompletion {
                    packet: CompletionPacket {
                        session_id: terminal.clone(),
                        status: CompletionStatus::Completed,
                        summary: "delegated terminal cleanup proof".to_string(),
                    },
                }],
            })
            .unwrap();

        let report = engine.cleanup_delegated_resources().unwrap();
        assert_eq!(report.terminal_archived, vec![terminal.clone()]);
        assert!(report.expired_archived.is_empty());
        assert!(report.orphaned_archived.is_empty());
        assert_eq!(report.resources_released, 0);

        assert_eq!(
            engine.get_session(&terminal).unwrap().status,
            SessionStatus::Archived
        );
        let store = CoordinationStore::open(data_dir).unwrap();
        assert_eq!(
            store
                .load_worker_run_by_delegated_session(&terminal)
                .unwrap()
                .unwrap()
                .status,
            WorkerRunStatus::Completed
        );
    }

    #[test]
    fn duplicate_delegated_completion_is_rejected_after_terminal_run() {
        let data_dir = unique_data_dir("delegated-completion-terminal-finality");
        let engine = test_engine_with_data_dir(data_dir.clone());
        let planner = engine
            .create_session(session_config(
                "planner-session",
                "planner",
                "planner-profile",
                SessionKind::Full,
            ))
            .unwrap();
        let delegated_session_id = spawn_delegated(&engine, &planner, "planner-wake", Some(30_000));

        let first = engine
            .execute_brain_actions(BrainActionBatch {
                wake_id: "delegated-wake-1".to_string(),
                session_id: delegated_session_id.clone(),
                actions: vec![BrainAction::DeliverCompletion {
                    packet: CompletionPacket {
                        session_id: delegated_session_id.clone(),
                        status: CompletionStatus::Completed,
                        summary: "first delegated completion".to_string(),
                    },
                }],
            })
            .unwrap();
        assert_eq!(first.accepted_actions, 1);
        assert!(first.rejected_actions.is_empty());

        let duplicate = engine
            .execute_brain_actions(BrainActionBatch {
                wake_id: "delegated-wake-2".to_string(),
                session_id: delegated_session_id.clone(),
                actions: vec![BrainAction::DeliverCompletion {
                    packet: CompletionPacket {
                        session_id: delegated_session_id.clone(),
                        status: CompletionStatus::Failed,
                        summary: "stale duplicate delegated completion".to_string(),
                    },
                }],
            })
            .unwrap();
        assert_eq!(duplicate.accepted_actions, 0);
        assert_eq!(duplicate.rejected_actions.len(), 1);
        assert_eq!(
            duplicate.rejected_actions[0].kind,
            CoreErrorKind::ActionRejected
        );
        assert!(duplicate.rejected_actions[0]
            .message
            .contains("already terminal"));

        let store = CoordinationStore::open(data_dir).unwrap();
        assert_eq!(store.count_rows("completion_packets").unwrap(), 1);
        assert_eq!(
            store
                .load_worker_run_by_delegated_session(&delegated_session_id)
                .unwrap()
                .unwrap()
                .status,
            WorkerRunStatus::Completed
        );
    }

    #[test]
    fn archiving_parent_cancels_nonterminal_delegated_children() {
        let data_dir = unique_data_dir("delegated-parent-archive");
        let engine = test_engine_with_data_dir(data_dir.clone());
        let planner = engine
            .create_session(session_config(
                "planner-session",
                "planner",
                "planner-profile",
                SessionKind::Full,
            ))
            .unwrap();
        let delegated_session_id = spawn_delegated(&engine, &planner, "planner-wake", Some(30_000));

        engine.archive_session(&planner.session_id).unwrap();

        assert_eq!(
            engine.get_session(&delegated_session_id).unwrap().status,
            SessionStatus::Archived
        );
        let store = CoordinationStore::open(data_dir).unwrap();
        assert_eq!(
            store
                .load_worker_run_by_delegated_session(&delegated_session_id)
                .unwrap()
                .unwrap()
                .status,
            WorkerRunStatus::Cancelled
        );
        assert_eq!(store.count_rows("completion_packets").unwrap(), 0);
        let status = engine
            .delegated_session_status(&delegated_session_id)
            .unwrap();
        assert_eq!(status.run_status, Some(DelegatedRunStatus::Cancelled));
        assert!(status.terminal);
    }

    #[test]
    fn operator_drain_cancels_delegated_sessions_for_parent() {
        let engine = test_engine();
        let planner = engine
            .create_session(session_config(
                "planner-session",
                "planner",
                "planner-profile",
                SessionKind::Full,
            ))
            .unwrap();
        let first = spawn_delegated(&engine, &planner, "planner-wake-a", Some(30_000));
        let second = spawn_delegated(&engine, &planner, "planner-wake-b", Some(30_000));

        let mut drained = engine
            .drain_delegated_sessions(Some(&planner.session_id))
            .unwrap();
        drained.sort_by(|left, right| left.0.cmp(&right.0));

        assert_eq!(drained, vec![first.clone(), second.clone()]);
        assert_eq!(
            engine.delegated_session_status(&first).unwrap().run_status,
            Some(DelegatedRunStatus::Cancelled)
        );
        assert_eq!(
            engine.delegated_session_status(&second).unwrap().run_status,
            Some(DelegatedRunStatus::Cancelled)
        );
    }

    #[test]
    fn restart_cleanup_cancels_orphaned_delegated_children_without_completion_packet() {
        let data_dir = unique_data_dir("delegated-orphan-cleanup");
        let engine = test_engine_with_data_dir(data_dir.clone());
        let planner = engine
            .create_session(session_config(
                "planner-session",
                "planner",
                "planner-profile",
                SessionKind::Full,
            ))
            .unwrap();
        let delegated_session_id = spawn_delegated(&engine, &planner, "planner-wake", Some(30_000));

        let mut archived_parent = planner.clone();
        archived_parent.status = SessionStatus::Archived;
        CoordinationStore::open(data_dir.clone())
            .unwrap()
            .save_session(&archived_parent)
            .unwrap();
        drop(engine);

        let restarted = test_engine_with_data_dir(data_dir.clone());

        assert_eq!(
            restarted.get_session(&delegated_session_id).unwrap().status,
            SessionStatus::Archived
        );
        let store = CoordinationStore::open(data_dir).unwrap();
        assert_eq!(
            store
                .load_worker_run_by_delegated_session(&delegated_session_id)
                .unwrap()
                .unwrap()
                .status,
            WorkerRunStatus::Cancelled
        );
        assert_eq!(store.count_rows("completion_packets").unwrap(), 0);
    }

    #[test]
    fn fan_out_max_concurrency_rejects_oversized_group_without_side_effects() {
        let data_dir = unique_data_dir("fan-out-max-concurrency");
        let engine = test_engine_with_data_dir(data_dir.clone());
        let planner = engine
            .create_session(session_config(
                "planner-session",
                "planner",
                "planner-profile",
                SessionKind::Full,
            ))
            .unwrap();

        let receipt = engine
            .execute_brain_actions(BrainActionBatch {
                wake_id: "planner-wake".to_string(),
                session_id: planner.session_id,
                actions: vec![
                    fan_out_request(0, "too-wide", Some(1), FanOutFailurePolicy::FailSoft),
                    fan_out_request(1, "too-wide", Some(1), FanOutFailurePolicy::FailSoft),
                ],
            })
            .unwrap();

        assert_eq!(receipt.accepted_actions, 0);
        assert_eq!(receipt.rejected_actions.len(), 2);
        assert!(receipt.rejected_actions.iter().all(|rejection| {
            rejection
                .message
                .contains("fan-out group too-wide exceeds max concurrency 1")
        }));
        let store = CoordinationStore::open(data_dir).unwrap();
        assert_eq!(store.count_rows("worker_runs").unwrap(), 0);
    }

    #[test]
    fn fan_out_group_projects_completed_and_partial_failure_aggregates() {
        let engine = test_engine();
        let planner = engine
            .create_session(session_config(
                "planner-session",
                "planner",
                "planner-profile",
                SessionKind::Full,
            ))
            .unwrap();

        engine
            .execute_brain_actions(BrainActionBatch {
                wake_id: "planner-wake".to_string(),
                session_id: planner.session_id.clone(),
                actions: vec![
                    fan_out_request(0, "review-slices", Some(3), FanOutFailurePolicy::FailSoft),
                    fan_out_request(1, "review-slices", Some(3), FanOutFailurePolicy::FailSoft),
                    fan_out_request(2, "review-slices", Some(3), FanOutFailurePolicy::FailSoft),
                ],
            })
            .unwrap();

        deliver_child_completion(
            &engine,
            &planner.session_id,
            "planner-wake",
            0,
            CompletionStatus::Completed,
        );
        deliver_child_completion(
            &engine,
            &planner.session_id,
            "planner-wake",
            1,
            CompletionStatus::Failed,
        );

        let body = engine.project_body_state(&planner.session_id).unwrap();
        assert_eq!(body.fan_out_groups.len(), 1);
        assert_eq!(body.fan_out_groups[0].group_id, "review-slices");
        assert_eq!(body.fan_out_groups[0].total, 3);
        assert_eq!(body.fan_out_groups[0].pending, 1);
        assert_eq!(body.fan_out_groups[0].completed, 1);
        assert_eq!(body.fan_out_groups[0].failed, 1);
        assert_eq!(
            body.fan_out_groups[0].status,
            rusty_crew_core_protocol::FanOutGroupStatus::InProgress
        );

        deliver_child_completion(
            &engine,
            &planner.session_id,
            "planner-wake",
            2,
            CompletionStatus::Completed,
        );

        let body = engine.project_body_state(&planner.session_id).unwrap();
        assert_eq!(body.fan_out_groups[0].pending, 0);
        assert_eq!(body.fan_out_groups[0].completed, 2);
        assert_eq!(body.fan_out_groups[0].failed, 1);
        assert_eq!(
            body.fan_out_groups[0].status,
            rusty_crew_core_protocol::FanOutGroupStatus::PartialFailure
        );
        assert_eq!(body.child_completions.len(), 3);
    }

    #[test]
    fn fan_out_fail_fast_cancels_pending_siblings_without_fake_completion() {
        let data_dir = unique_data_dir("fan-out-fail-fast");
        let engine = test_engine_with_data_dir(data_dir.clone());
        let planner = engine
            .create_session(session_config(
                "planner-session",
                "planner",
                "planner-profile",
                SessionKind::Full,
            ))
            .unwrap();

        engine
            .execute_brain_actions(BrainActionBatch {
                wake_id: "planner-wake".to_string(),
                session_id: planner.session_id.clone(),
                actions: vec![
                    fan_out_request(0, "audit-slices", Some(2), FanOutFailurePolicy::FailFast),
                    fan_out_request(1, "audit-slices", Some(2), FanOutFailurePolicy::FailFast),
                ],
            })
            .unwrap();

        deliver_child_completion(
            &engine,
            &planner.session_id,
            "planner-wake",
            0,
            CompletionStatus::Failed,
        );

        let sibling_session_id = delegated_session_id(&planner.session_id, "planner-wake", 1);
        assert_eq!(
            engine.get_session(&sibling_session_id).unwrap().status,
            SessionStatus::Archived
        );
        let body = engine.project_body_state(&planner.session_id).unwrap();
        assert_eq!(body.fan_out_groups[0].failed, 1);
        assert_eq!(body.fan_out_groups[0].cancelled, 1);
        assert_eq!(
            body.fan_out_groups[0].status,
            rusty_crew_core_protocol::FanOutGroupStatus::FailedFast
        );
        let store = CoordinationStore::open(data_dir).unwrap();
        assert_eq!(store.count_rows("completion_packets").unwrap(), 1);
    }

    #[test]
    fn delegated_sessions_resolve_tool_profile_from_requested_profile() {
        let data_dir = unique_data_dir("delegated-tool-profile");
        let engine = test_engine_with_data_dir(data_dir);
        let planner = engine
            .create_session(session_config(
                "planner-session",
                "planner",
                "planner-profile",
                SessionKind::Full,
            ))
            .unwrap();
        engine
            .register_profile_tool_profile(
                ProfileId::new("restricted-coder-profile"),
                ToolProfile {
                    tools: vec![
                        ToolDescriptor {
                            name: "read_file".to_string(),
                            description: "Read files in the delegated workdir".to_string(),
                            input_schema: None,
                        },
                        ToolDescriptor {
                            name: "patch".to_string(),
                            description: "Apply a bounded source patch".to_string(),
                            input_schema: None,
                        },
                    ],
                },
            )
            .unwrap();

        engine
            .execute_brain_actions(BrainActionBatch {
                wake_id: "planner-wake".to_string(),
                session_id: planner.session_id.clone(),
                actions: vec![BrainAction::RequestDelegation {
                    profile_id: ProfileId::new("restricted-coder-profile"),
                    task_id: None,
                    prompt: "use only delegated profile tools".to_string(),
                    expected_output: None,
                    resource_limits: Some(ResourceLimits {
                        workdir: Some("/home/dev/rusty-crew".to_string()),
                        max_duration_ms: Some(30_000),
                        max_delegation_depth: Some(0),
                    }),
                    timeout_ms: None,
                    priority: None,
                    fan_out_group_id: None,
                    fan_out_max_concurrency: None,
                    fan_out_failure_policy: None,
                    correlation_id: None,
                    parent_consumption: None,
                    capacity_request: None,
                }],
            })
            .unwrap();

        let delegated = engine
            .get_session(&delegated_session_id(
                &planner.session_id,
                "planner-wake",
                0,
            ))
            .unwrap();

        assert_eq!(
            delegated
                .tool_profile
                .tools
                .iter()
                .map(|tool| tool.name.as_str())
                .collect::<Vec<_>>(),
            vec!["read_file", "patch"]
        );
        assert_eq!(
            delegated.resource_limits,
            ResourceLimits {
                workdir: Some("/home/dev/rusty-crew".to_string()),
                max_duration_ms: Some(30_000),
                max_delegation_depth: Some(0),
            }
        );
    }

    #[test]
    fn den_product_data_updates_are_not_persisted_to_coordination_store() {
        let data_dir = unique_data_dir("den-data");
        let engine = test_engine_with_data_dir(data_dir.clone());

        engine
            .inject_den_data_update(DenDataUpdate {
                project_id: ProjectId::new("pi-crew"),
                entity_kind: "document".to_string(),
                entity_id: "rusty-crew-unified-architecture".to_string(),
                revision: Some("den-owned".to_string()),
            })
            .unwrap();

        let store = CoordinationStore::open(data_dir).unwrap();

        assert_eq!(store.count_rows("event_history").unwrap(), 0);
        assert_eq!(store.count_rows("agent_messages").unwrap(), 0);
        assert_eq!(store.count_rows("completion_packets").unwrap(), 0);
    }

    #[test]
    fn system_clock_writes_rfc3339_timestamps() {
        let data_dir = unique_data_dir("system-clock");
        let engine = CoreEngine::initialize(EngineConfig {
            engine_data_dir: data_dir.to_string_lossy().to_string(),
            clock: ClockConfig::System,
            default_turn_budget: 3,
            default_idle_timeout_ms: 1000,
            storage: None,
        })
        .unwrap();
        let planner = engine
            .create_session(session_config(
                "planner-session",
                "planner",
                "planner-profile",
                SessionKind::Full,
            ))
            .unwrap();

        assert_ne!(planner.created_at, "system-clock-placeholder");
        assert!(time::OffsetDateTime::parse(
            &planner.created_at,
            &time::format_description::well_known::Rfc3339
        )
        .is_ok());
        assert!(time::OffsetDateTime::parse(
            &planner.last_active_at,
            &time::format_description::well_known::Rfc3339
        )
        .is_ok());

        engine
            .execute_brain_actions(BrainActionBatch {
                wake_id: "planner-wake".to_string(),
                session_id: planner.session_id.clone(),
                actions: vec![BrainAction::RequestDelegation {
                    profile_id: ProfileId::new("coder-profile"),
                    task_id: None,
                    prompt: "check system timestamps".to_string(),
                    expected_output: None,
                    resource_limits: None,
                    timeout_ms: None,
                    priority: None,
                    fan_out_group_id: None,
                    fan_out_max_concurrency: None,
                    fan_out_failure_policy: None,
                    correlation_id: None,
                    parent_consumption: None,
                    capacity_request: None,
                }],
            })
            .unwrap();

        let store = CoordinationStore::open(data_dir).unwrap();
        let run = store
            .load_worker_run(&RunId::new("planner-wake:0"))
            .unwrap()
            .unwrap();

        assert_ne!(run.created_at, "system-clock-placeholder");
        assert!(time::OffsetDateTime::parse(
            &run.created_at,
            &time::format_description::well_known::Rfc3339,
        )
        .is_ok());
        assert!(time::OffsetDateTime::parse(
            &run.last_updated_at,
            &time::format_description::well_known::Rfc3339,
        )
        .is_ok());
    }

    #[test]
    #[cfg(feature = "postgres")]
    #[ignore = "requires local PostgreSQL dev database env; source /home/system/database/rusty-crew-postgres.env or set RUSTY_CREW_DATABASE_URL"]
    fn postgres_engine_initialization_uses_postgres_without_sqlite_fallback() {
        let database_url = std::env::var("RUSTY_CREW_DATABASE_URL")
            .expect("RUSTY_CREW_DATABASE_URL must be set for live PostgreSQL engine smoke");
        let data_dir = unique_data_dir("postgres-engine-no-sqlite");
        let schema = format!(
            "rc_engine_{}_{}",
            std::process::id(),
            NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed)
        );
        let engine = CoreEngine::initialize(EngineConfig {
            engine_data_dir: data_dir.to_string_lossy().to_string(),
            clock: ClockConfig::Fixed {
                at: "2026-06-27T00:00:00Z".to_string(),
            },
            default_turn_budget: 3,
            default_idle_timeout_ms: 1000,
            storage: Some(EngineStorageConfig::Postgres {
                database_url,
                schema,
                max_connections: None,
                statement_timeout_ms: None,
            }),
        })
        .unwrap();

        let diagnostics = engine.storage_diagnostics().unwrap();
        assert_eq!(diagnostics.backend, "postgres");
        assert!(!data_dir.join("coordination.sqlite3").exists());
    }

    #[test]
    fn persistence_open_failures_are_typed() {
        let data_dir = unique_data_dir("blocked");
        std::fs::write(&data_dir, "not a directory").unwrap();

        let error = CoreEngine::initialize(test_engine_config(data_dir))
            .expect_err("file-backed data dir should fail");

        assert_eq!(error.kind, CoreErrorKind::PersistenceFailure);
    }

    #[test]
    fn model_provider_refresh_impact_uses_profile_registry_and_session_state() {
        let engine = test_engine();
        engine
            .create_profile_registry_record(&profile_registry_write(
                "planner-profile",
                "alternate",
                "configured-planner-session",
            ))
            .unwrap();
        engine
            .create_profile_registry_record(&profile_registry_write(
                "other-profile",
                "default",
                "other-session",
            ))
            .unwrap();
        engine
            .create_session(session_config(
                "active-planner-session",
                "planner",
                "planner-profile",
                SessionKind::Full,
            ))
            .unwrap();
        engine
            .create_session(session_config(
                "archived-planner-session",
                "planner-archived",
                "planner-profile",
                SessionKind::Full,
            ))
            .unwrap();
        engine
            .archive_session(&SessionId::new("archived-planner-session"))
            .unwrap();
        engine
            .create_session(session_config(
                "active-other-session",
                "other",
                "other-profile",
                SessionKind::Full,
            ))
            .unwrap();

        let impact = engine
            .model_provider_refresh_impact(&ModelProviderRefreshImpactRequest {
                provider_alias: "alternate".to_string(),
            })
            .unwrap();

        assert_eq!(impact.provider_alias, "alternate");
        assert_eq!(impact.affected_profiles.len(), 1);
        let affected = &impact.affected_profiles[0];
        assert_eq!(affected.profile_id, ProfileId::new("planner-profile"));
        assert_eq!(
            affected.configured_session_ids,
            vec![SessionId::new("configured-planner-session")]
        );
        assert_eq!(
            affected.active_session_ids,
            vec![SessionId::new("active-planner-session")]
        );
        assert_eq!(
            affected.session_ids,
            vec![
                SessionId::new("active-planner-session"),
                SessionId::new("configured-planner-session")
            ]
        );
    }

    #[test]
    fn model_provider_refresh_plan_none_keeps_impact_but_no_actions() {
        let engine = test_engine();
        engine
            .create_profile_registry_record(&profile_registry_write(
                "planner-profile",
                "alternate",
                "configured-planner-session",
            ))
            .unwrap();

        let plan = engine
            .plan_model_provider_refresh(&ModelProviderRefreshPlanRequest {
                provider_alias: "alternate".to_string(),
                mode: ModelProviderRefreshMode::None,
            })
            .unwrap();

        assert_eq!(plan.provider_alias, "alternate");
        assert_eq!(plan.mode, ModelProviderRefreshMode::None);
        assert_eq!(plan.affected_profiles.len(), 1);
        assert!(plan.actions.is_empty());
    }

    #[test]
    fn model_provider_refresh_plan_apply_builds_rebuild_actions() {
        let engine = test_engine();
        engine
            .create_profile_registry_record(&profile_registry_write(
                "planner-profile",
                "alternate",
                "configured-planner-session",
            ))
            .unwrap();

        let plan = engine
            .plan_model_provider_refresh(&ModelProviderRefreshPlanRequest {
                provider_alias: "alternate".to_string(),
                mode: ModelProviderRefreshMode::Apply,
            })
            .unwrap();

        assert_eq!(plan.provider_alias, "alternate");
        assert_eq!(plan.mode, ModelProviderRefreshMode::Apply);
        assert_eq!(plan.actions.len(), 1);
        let action = &plan.actions[0];
        assert_eq!(action.profile_id, ProfileId::new("planner-profile"));
        assert_eq!(action.command_name, "apply_runtime_rebuild");
        assert_eq!(action.reason, "model provider alternate updated");
        assert_eq!(
            action.applied_summary,
            "runtime rebuild applied for profile planner-profile"
        );
        assert_eq!(
            action.blocked_summary,
            "runtime rebuild blocked for profile planner-profile"
        );
        assert_eq!(action.failure_reason_code, "model_provider_refresh_failed");
    }

    #[test]
    fn chat_read_model_projects_slots_with_cursor_and_has_more() {
        let engine = test_engine();
        engine
            .create_session(session_config(
                "chat-session",
                "prime-agent",
                "prime-profile",
                SessionKind::Full,
            ))
            .unwrap();
        save_test_message_slot(&engine, "chat-session", 1, "operator", "user", "hello");
        save_test_message_slot(&engine, "chat-session", 2, "prime-agent", "assistant", "hi");
        save_test_message_slot(&engine, "chat-session", 3, "operator", "user", "again");

        let page = engine
            .chat_read_model_page(&ChatReadModelQuery {
                session_id: SessionId::new("chat-session"),
                agent_id: "prime-agent".to_string(),
                cursor: Some("chat-session:1".to_string()),
                limit: Some(1),
            })
            .unwrap();

        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].event_id, "chat-session:2");
        assert_eq!(page.items[0].sequence_id, 2);
        assert_eq!(page.items[0].kind, ChatReadModelEventKind::MessageCreated);
        assert_eq!(page.items[0].payload_json["role"], "assistant");
        assert_eq!(page.items[0].payload_json["body"], "hi");
        assert_eq!(page.items[0].payload_json["source"], "durable_message_slot");
        assert_eq!(page.latest_cursor, "chat-session:2");
        assert!(page.has_more);
        assert_eq!(page.total, 3);
        assert_eq!(page.source, ChatReadModelSource::MessageSlots);
    }

    #[test]
    fn chat_session_read_and_summary_choose_durable_sources_explicitly() {
        let engine = test_engine();
        let pending = engine
            .create_session(session_config(
                "pending-chat-session",
                "pending-agent",
                "prime-profile",
                SessionKind::Full,
            ))
            .unwrap();
        let logged = engine
            .create_session(session_config(
                "logged-chat-session",
                "logged-agent",
                "prime-profile",
                SessionKind::Full,
            ))
            .unwrap();
        engine
            .route_agent_message(AgentMessage {
                from: AgentId::new("operator"),
                to: pending.agent_id.clone(),
                body: "pending hello".to_string(),
                correlation_id: Some("pending-correlation".to_string()),
                projection: None,
            })
            .unwrap();
        engine
            .append_chat_event(&ChatEventLogAppend {
                session_id: logged.session_id.clone(),
                created_at: "2026-06-19T00:01:00Z".to_string(),
                kind: "message_created".to_string(),
                payload_json: json!({"body": "logged hello"}),
            })
            .unwrap();
        engine
            .append_chat_event(&ChatEventLogAppend {
                session_id: logged.session_id.clone(),
                created_at: "2026-06-19T00:01:02Z".to_string(),
                kind: "assistant_message_completed".to_string(),
                payload_json: json!({"body": "logged reply"}),
            })
            .unwrap();
        engine
            .append_chat_event(&ChatEventLogAppend {
                session_id: logged.session_id.clone(),
                created_at: "2026-06-19T00:01:01Z".to_string(),
                kind: "tool_call_completed".to_string(),
                payload_json: json!({"tool_name": "read_file"}),
            })
            .unwrap();

        let pending_read = engine
            .read_chat_session(&ChatSessionReadQuery {
                session_id: pending.session_id.clone(),
                cursor: None,
                limit: 10,
                include_alternates: false,
            })
            .unwrap();
        assert_eq!(pending_read.source, ChatReadModelSource::PendingMessages);
        assert_eq!(pending_read.total, 1);
        assert_eq!(pending_read.events[0].payload_json["body"], "pending hello");
        assert_eq!(pending_read.message_slots.total, 0);

        let logged_read = engine
            .read_chat_session(&ChatSessionReadQuery {
                session_id: logged.session_id.clone(),
                cursor: None,
                limit: 10,
                include_alternates: false,
            })
            .unwrap();
        assert_eq!(logged_read.source, ChatReadModelSource::EventLog);
        assert_eq!(logged_read.total, 3);
        assert_eq!(logged_read.message_count, 2);
        assert_eq!(logged_read.events[0].payload_json["body"], "logged hello");

        let summaries = engine
            .query_chat_session_summaries(&ChatSessionSummaryPageQuery {
                profile_id: Some(ProfileId::new("prime-profile")),
                status: Some("idle".to_string()),
                page: rusty_crew_core_persistence::QueryPage {
                    limit: Some(1),
                    offset: Some(0),
                },
            })
            .unwrap();
        assert_eq!(summaries.page.total, 2);
        assert_eq!(summaries.page.items.len(), 1);
        assert_eq!(summaries.page.next_offset, Some(1));
        assert_eq!(summaries.page.items[0].message_count, 2);
    }

    #[test]
    fn chat_session_read_sources_survive_engine_restart() {
        let data_dir = unique_data_dir("chat-session-read-restart");
        let engine = test_engine_with_data_dir(data_dir.clone());
        let pending = engine
            .create_session(session_config(
                "restart-pending-session",
                "restart-pending-agent",
                "prime-profile",
                SessionKind::Full,
            ))
            .unwrap();
        let logged = engine
            .create_session(session_config(
                "restart-logged-session",
                "restart-logged-agent",
                "prime-profile",
                SessionKind::Full,
            ))
            .unwrap();
        engine
            .route_agent_message(AgentMessage {
                from: AgentId::new("operator"),
                to: pending.agent_id.clone(),
                body: "pending across restart".to_string(),
                correlation_id: Some("restart-pending".to_string()),
                projection: None,
            })
            .unwrap();
        engine
            .append_chat_event(&ChatEventLogAppend {
                session_id: logged.session_id.clone(),
                created_at: "2026-06-19T00:01:00Z".to_string(),
                kind: "message_created".to_string(),
                payload_json: json!({"body": "logged across restart"}),
            })
            .unwrap();
        drop(engine);

        let restarted = test_engine_with_data_dir(data_dir);
        let pending_read = restarted
            .read_chat_session(&ChatSessionReadQuery {
                session_id: pending.session_id,
                cursor: None,
                limit: 10,
                include_alternates: false,
            })
            .unwrap();
        assert_eq!(pending_read.source, ChatReadModelSource::PendingMessages);
        assert_eq!(
            pending_read.events[0].payload_json["body"],
            "pending across restart"
        );
        let logged_read = restarted
            .read_chat_session(&ChatSessionReadQuery {
                session_id: logged.session_id,
                cursor: None,
                limit: 10,
                include_alternates: false,
            })
            .unwrap();
        assert_eq!(logged_read.source, ChatReadModelSource::EventLog);
        assert_eq!(
            logged_read.events[0].payload_json["body"],
            "logged across restart"
        );
    }

    #[test]
    fn chat_read_model_uses_active_alternate_and_forgives_bad_cursors() {
        let engine = test_engine();
        engine
            .create_session(session_config(
                "variant-session",
                "prime-agent",
                "prime-profile",
                SessionKind::Full,
            ))
            .unwrap();
        save_test_message_slot(
            &engine,
            "variant-session",
            1,
            "prime-agent",
            "assistant",
            "primary",
        );
        engine
            .save_message_variant(&MessageVariantWrite {
                variant_id: MessageVariantId::new("variant-session-variant-1-alt"),
                slot_id: MessageSlotId::new("variant-session-slot-1"),
                source: MessageVariantSource::Alternate,
                ordinal: 1,
                status: MessageVariantStatus::Active,
                message: test_message_write(
                    "variant-session",
                    10,
                    "prime-agent",
                    "assistant",
                    "alternate",
                ),
                metadata_json: json!({}),
                created_at: "2026-06-19T00:10:00Z".to_string(),
                updated_at: "2026-06-19T00:10:00Z".to_string(),
            })
            .unwrap();
        engine
            .select_active_message_variant(&SelectActiveVariantRequest {
                slot_id: MessageSlotId::new("variant-session-slot-1"),
                active_variant_id: Some(MessageVariantId::new("variant-session-variant-1-alt")),
                expected: rusty_crew_core_persistence::ActiveVariantExpectation::Any,
                updated_at: "2026-06-19T00:11:00Z".to_string(),
            })
            .unwrap();

        let page = engine
            .chat_read_model_page(&ChatReadModelQuery {
                session_id: SessionId::new("variant-session"),
                agent_id: "prime-agent".to_string(),
                cursor: Some("other-session:not-a-number".to_string()),
                limit: Some(10),
            })
            .unwrap();

        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].event_id, "variant-session:1");
        assert_eq!(page.items[0].payload_json["body"], "alternate");
        assert_eq!(page.latest_cursor, "variant-session:1");
        assert!(!page.has_more);
    }

    #[test]
    fn chat_event_log_allocates_sequences_and_pages_after_cursor() {
        let engine = test_engine();

        let first = engine
            .append_chat_event(&ChatEventLogAppend {
                session_id: SessionId::new("stream-session"),
                created_at: "2026-06-19T00:01:00Z".to_string(),
                kind: "message_created".to_string(),
                payload_json: json!({ "body": "hello" }),
            })
            .unwrap();
        let second = engine
            .append_chat_event(&ChatEventLogAppend {
                session_id: SessionId::new("stream-session"),
                created_at: "2026-06-19T00:02:00Z".to_string(),
                kind: "assistant_text_delta".to_string(),
                payload_json: json!({ "delta": "hi" }),
            })
            .unwrap();

        assert_eq!(first.event_id, "stream-session:1");
        assert_eq!(second.event_id, "stream-session:2");

        let page = engine
            .query_chat_events(&ChatEventLogQuery {
                session_id: SessionId::new("stream-session"),
                cursor: Some("stream-session:1".to_string()),
                limit: Some(1),
            })
            .unwrap();

        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].event_id, "stream-session:2");
        assert_eq!(page.items[0].kind, "assistant_text_delta");
        assert_eq!(page.latest_cursor, "stream-session:2");
        assert!(!page.has_more);

        let latest = engine
            .query_chat_events(&ChatEventLogQuery {
                session_id: SessionId::new("stream-session"),
                cursor: None,
                limit: Some(1),
            })
            .unwrap();

        assert_eq!(latest.items.len(), 1);
        assert_eq!(latest.items[0].event_id, "stream-session:2");
        assert_eq!(latest.latest_cursor, "stream-session:2");
        assert!(latest.has_more);
    }

    #[test]
    fn chat_event_log_replays_after_store_reload_without_memory_state() {
        let data_dir = unique_data_dir("chat-events-reload");
        {
            let engine = test_engine_with_data_dir(data_dir.clone());
            for index in 1..=3 {
                engine
                    .append_chat_event(&ChatEventLogAppend {
                        session_id: SessionId::new("reload-session"),
                        created_at: format!("2026-06-19T00:0{index}:00Z"),
                        kind: "message_created".to_string(),
                        payload_json: json!({ "body": format!("message {index}") }),
                    })
                    .unwrap();
            }
        }

        let engine = test_engine_with_data_dir(data_dir);
        let page = engine
            .query_chat_events(&ChatEventLogQuery {
                session_id: SessionId::new("reload-session"),
                cursor: Some("reload-session:1".to_string()),
                limit: Some(1),
            })
            .unwrap();

        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].event_id, "reload-session:2");
        assert_eq!(page.items[0].payload_json["body"], "message 2");
        assert_eq!(page.latest_cursor, "reload-session:2");
        assert!(page.has_more);
    }

    #[test]
    fn select_active_chat_message_variant_updates_branch_head() {
        let engine = test_engine();
        engine
            .save_conversation_branch(&ConversationBranchWrite {
                branch_id: ConversationBranchId::new("variant-branch"),
                session_id: SessionId::new("chat-variant-session"),
                parent_branch_id: None,
                parent_message_id: None,
                origin_message_id: None,
                head_message_id: Some(MessageId::new("chat-variant-session-message-1")),
                label: Some("Main".to_string()),
                metadata_json: json!({}),
                created_at: "2026-06-19T00:00:00Z".to_string(),
                updated_at: "2026-06-19T00:00:00Z".to_string(),
            })
            .unwrap();
        save_test_message_slot(
            &engine,
            "chat-variant-session",
            1,
            "agent",
            "assistant",
            "primary",
        );
        let mut alternate =
            test_message_write("chat-variant-session", 2, "agent", "assistant", "alternate");
        alternate.branch_id = Some(ConversationBranchId::new("variant-branch"));
        engine
            .save_message_variant(&MessageVariantWrite {
                variant_id: MessageVariantId::new("chat-variant-session-variant-alt"),
                slot_id: MessageSlotId::new("chat-variant-session-slot-1"),
                source: MessageVariantSource::Alternate,
                ordinal: 1,
                status: MessageVariantStatus::Active,
                message: alternate,
                metadata_json: json!({}),
                created_at: "2026-06-19T00:02:00Z".to_string(),
                updated_at: "2026-06-19T00:02:00Z".to_string(),
            })
            .unwrap();

        let result = engine
            .select_active_chat_message_variant(&SelectActiveChatMessageVariantRequest {
                session_id: SessionId::new("chat-variant-session"),
                slot_id: MessageSlotId::new("chat-variant-session-slot-1"),
                active_variant_id: Some(MessageVariantId::new("chat-variant-session-variant-alt")),
                expected: rusty_crew_core_persistence::ActiveVariantExpectation::Any,
                updated_at: "2026-06-19T00:03:00Z".to_string(),
            })
            .unwrap();

        assert!(result.conflict.is_none());
        assert_eq!(
            result.slot.active_variant_id,
            Some(MessageVariantId::new("chat-variant-session-variant-alt"))
        );
        let branches = engine
            .query_conversation_branches(&ConversationBranchQuery {
                session_id: Some(SessionId::new("chat-variant-session")),
                parent_branch_id: None,
                page: None,
            })
            .unwrap();
        assert_eq!(
            branches
                .iter()
                .find(|branch| branch.branch_id == ConversationBranchId::new("variant-branch"))
                .and_then(|branch| branch.head_message_id.clone()),
            Some(MessageId::new("chat-variant-session-message-2"))
        );
    }

    #[test]
    fn select_active_chat_message_variant_preserves_conflict_output() {
        let engine = test_engine();
        save_test_message_slot(
            &engine,
            "chat-conflict-session",
            1,
            "agent",
            "assistant",
            "primary",
        );

        let result = engine
            .select_active_chat_message_variant(&SelectActiveChatMessageVariantRequest {
                session_id: SessionId::new("chat-conflict-session"),
                slot_id: MessageSlotId::new("chat-conflict-session-slot-1"),
                active_variant_id: None,
                expected: rusty_crew_core_persistence::ActiveVariantExpectation::Variant(
                    MessageVariantId::new("missing-active-variant"),
                ),
                updated_at: "2026-06-19T00:03:00Z".to_string(),
            })
            .unwrap();

        assert_eq!(
            result.conflict,
            Some(ActiveVariantConflict {
                expected: Some(MessageVariantId::new("missing-active-variant")),
                actual: None,
            })
        );
        assert_eq!(result.slot.active_variant_id, None);
    }

    #[test]
    fn create_chat_message_slot_updates_branch_head_atomically() {
        let engine = test_engine();
        engine
            .save_conversation_branch(&ConversationBranchWrite {
                branch_id: ConversationBranchId::new("create-slot-branch"),
                session_id: SessionId::new("create-slot-session"),
                parent_branch_id: None,
                parent_message_id: None,
                origin_message_id: None,
                head_message_id: None,
                label: Some("Main".to_string()),
                metadata_json: json!({}),
                created_at: "2026-06-19T00:00:00Z".to_string(),
                updated_at: "2026-06-19T00:00:00Z".to_string(),
            })
            .unwrap();
        let mut message = test_message_write("create-slot-session", 1, "user", "user", "hello");
        message.branch_id = Some(ConversationBranchId::new("create-slot-branch"));

        let result = engine
            .create_chat_message_slot(&CreateChatMessageSlotRequest {
                slot: MessageSlotWrite {
                    slot_id: MessageSlotId::new("create-slot-session-slot-1"),
                    session_id: SessionId::new("create-slot-session"),
                    primary_variant_id: MessageVariantId::new("create-slot-session-primary-1"),
                    active_variant_id: None,
                    metadata_json: json!({ "source": "test" }),
                    created_at: "2026-06-19T00:01:00Z".to_string(),
                    updated_at: "2026-06-19T00:01:00Z".to_string(),
                },
                primary_variant: MessageVariantWrite {
                    variant_id: MessageVariantId::new("create-slot-session-primary-1"),
                    slot_id: MessageSlotId::new("create-slot-session-slot-1"),
                    source: MessageVariantSource::Primary,
                    ordinal: 0,
                    status: MessageVariantStatus::Active,
                    message,
                    metadata_json: json!({}),
                    created_at: "2026-06-19T00:01:00Z".to_string(),
                    updated_at: "2026-06-19T00:01:00Z".to_string(),
                },
                branch_id: ConversationBranchId::new("create-slot-branch"),
                expected_branch_head: BranchHeadExpectation::None,
                updated_at: "2026-06-19T00:01:30Z".to_string(),
                ensure_active_branch: None,
                inherit_branch_head: false,
                idempotency_key: None,
            })
            .unwrap();

        assert!(result.conflict.is_none());
        assert_eq!(
            result
                .slot
                .as_ref()
                .map(|slot| slot.primary.message.message_id.clone()),
            Some(MessageId::new("create-slot-session-message-1"))
        );
        assert_eq!(
            result.branch.head_message_id,
            Some(MessageId::new("create-slot-session-message-1"))
        );
    }

    #[test]
    fn create_chat_message_slot_conflict_does_not_create_slot() {
        let engine = test_engine();
        save_test_message_slot(
            &engine,
            "create-conflict-session",
            1,
            "user",
            "user",
            "existing",
        );
        engine
            .save_conversation_branch(&ConversationBranchWrite {
                branch_id: ConversationBranchId::new("create-conflict-branch"),
                session_id: SessionId::new("create-conflict-session"),
                parent_branch_id: None,
                parent_message_id: None,
                origin_message_id: None,
                head_message_id: Some(MessageId::new("create-conflict-session-message-1")),
                label: Some("Main".to_string()),
                metadata_json: json!({}),
                created_at: "2026-06-19T00:00:00Z".to_string(),
                updated_at: "2026-06-19T00:00:00Z".to_string(),
            })
            .unwrap();
        let mut message = test_message_write("create-conflict-session", 2, "user", "user", "new");
        message.branch_id = Some(ConversationBranchId::new("create-conflict-branch"));

        let result = engine
            .create_chat_message_slot(&CreateChatMessageSlotRequest {
                slot: MessageSlotWrite {
                    slot_id: MessageSlotId::new("create-conflict-session-slot-2"),
                    session_id: SessionId::new("create-conflict-session"),
                    primary_variant_id: MessageVariantId::new("create-conflict-session-primary-2"),
                    active_variant_id: None,
                    metadata_json: json!({}),
                    created_at: "2026-06-19T00:02:00Z".to_string(),
                    updated_at: "2026-06-19T00:02:00Z".to_string(),
                },
                primary_variant: MessageVariantWrite {
                    variant_id: MessageVariantId::new("create-conflict-session-primary-2"),
                    slot_id: MessageSlotId::new("create-conflict-session-slot-2"),
                    source: MessageVariantSource::Primary,
                    ordinal: 0,
                    status: MessageVariantStatus::Active,
                    message,
                    metadata_json: json!({}),
                    created_at: "2026-06-19T00:02:00Z".to_string(),
                    updated_at: "2026-06-19T00:02:00Z".to_string(),
                },
                branch_id: ConversationBranchId::new("create-conflict-branch"),
                expected_branch_head: BranchHeadExpectation::None,
                updated_at: "2026-06-19T00:02:30Z".to_string(),
                ensure_active_branch: None,
                inherit_branch_head: false,
                idempotency_key: None,
            })
            .unwrap();

        assert_eq!(
            result.conflict,
            Some(BranchHeadConflict {
                expected: None,
                actual: Some(MessageId::new("create-conflict-session-message-1")),
            })
        );
        assert!(result.slot.is_none());
        let slots = engine
            .query_message_slots(&MessageSlotQuery {
                session_id: Some(SessionId::new("create-conflict-session")),
                include_alternates: true,
                page: None,
            })
            .unwrap();
        assert!(slots
            .iter()
            .all(|slot| slot.slot_id != MessageSlotId::new("create-conflict-session-slot-2")));
    }

    #[test]
    fn create_chat_message_slot_ensures_branch_and_replays_durable_receipt() {
        let engine = test_engine();
        let request = chat_slot_ingest_request("ingest-session", 1, "request-alpha");

        let created = engine.create_chat_message_slot(&request).unwrap();
        assert!(!created.duplicate);
        assert!(created.conflict.is_none());
        assert_eq!(
            created.branch.branch_id,
            ConversationBranchId::new("branch:ingest-session:default")
        );
        let created_slot = created.slot.unwrap();
        assert_eq!(
            created_slot.primary.message.branch_id,
            Some(created.branch.branch_id.clone())
        );
        assert_eq!(created_slot.primary.message.parent_message_id, None);

        let duplicate = engine.create_chat_message_slot(&request).unwrap();
        assert!(duplicate.duplicate);
        assert!(duplicate.conflict.is_none());
        assert_eq!(duplicate.slot.unwrap().slot_id, created_slot.slot_id);
        assert_eq!(duplicate.branch.branch_id, created.branch.branch_id);
        assert_eq!(
            engine
                .query_message_slots(&MessageSlotQuery {
                    session_id: Some(SessionId::new("ingest-session")),
                    include_alternates: true,
                    page: None,
                })
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn create_chat_message_slot_receipt_rolls_back_with_conflict() {
        let engine = test_engine();
        engine
            .create_chat_message_slot(&chat_slot_ingest_request(
                "receipt-rollback-session",
                1,
                "request-first",
            ))
            .unwrap();
        let mut request = chat_slot_ingest_request("receipt-rollback-session", 2, "request-retry");
        request.expected_branch_head = BranchHeadExpectation::None;
        let conflict = engine.create_chat_message_slot(&request).unwrap();
        assert!(conflict.conflict.is_some());
        assert!(!conflict.duplicate);

        request.expected_branch_head = BranchHeadExpectation::Any;
        let retried = engine.create_chat_message_slot(&request).unwrap();
        assert!(retried.conflict.is_none());
        assert!(!retried.duplicate);
        assert_eq!(
            engine
                .query_message_slots(&MessageSlotQuery {
                    session_id: Some(SessionId::new("receipt-rollback-session")),
                    include_alternates: true,
                    page: None,
                })
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn concurrent_chat_message_slot_ingest_creates_once() {
        let engine = test_engine();
        let request = chat_slot_ingest_request("concurrent-ingest-session", 1, "same-key");
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let results = std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for _ in 0..2 {
                let engine = engine.clone();
                let request = request.clone();
                let barrier = barrier.clone();
                handles.push(scope.spawn(move || {
                    barrier.wait();
                    engine.create_chat_message_slot(&request).unwrap()
                }));
            }
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .collect::<Vec<_>>()
        });
        assert_eq!(results.iter().filter(|result| result.duplicate).count(), 1);
        assert_eq!(results.iter().filter(|result| !result.duplicate).count(), 1);
        assert_eq!(
            engine
                .query_message_slots(&MessageSlotQuery {
                    session_id: Some(SessionId::new("concurrent-ingest-session")),
                    include_alternates: true,
                    page: None,
                })
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn create_chat_message_variant_allocates_next_ordinal() {
        let engine = test_engine();
        save_test_message_slot(
            &engine,
            "create-variant-session",
            1,
            "assistant",
            "assistant",
            "primary",
        );
        let mut first = test_message_write(
            "create-variant-session",
            2,
            "assistant",
            "assistant",
            "alt 1",
        );
        first.branch_id = Some(ConversationBranchId::new("variant-branch"));
        let mut second = test_message_write(
            "create-variant-session",
            3,
            "assistant",
            "assistant",
            "alt 2",
        );
        second.branch_id = Some(ConversationBranchId::new("variant-branch"));

        let first_result = engine
            .create_chat_message_variant(&CreateChatMessageVariantRequest {
                session_id: SessionId::new("create-variant-session"),
                slot_id: MessageSlotId::new("create-variant-session-slot-1"),
                variant: MessageVariantWrite {
                    variant_id: MessageVariantId::new("create-variant-session-alt-1"),
                    slot_id: MessageSlotId::new("create-variant-session-slot-1"),
                    source: MessageVariantSource::Alternate,
                    ordinal: 0,
                    status: MessageVariantStatus::Active,
                    message: first,
                    metadata_json: json!({}),
                    created_at: "2026-06-19T00:02:00Z".to_string(),
                    updated_at: "2026-06-19T00:02:00Z".to_string(),
                },
            })
            .unwrap();
        let second_result = engine
            .create_chat_message_variant(&CreateChatMessageVariantRequest {
                session_id: SessionId::new("create-variant-session"),
                slot_id: MessageSlotId::new("create-variant-session-slot-1"),
                variant: MessageVariantWrite {
                    variant_id: MessageVariantId::new("create-variant-session-alt-2"),
                    slot_id: MessageSlotId::new("create-variant-session-slot-1"),
                    source: MessageVariantSource::Alternate,
                    ordinal: 0,
                    status: MessageVariantStatus::Active,
                    message: second,
                    metadata_json: json!({}),
                    created_at: "2026-06-19T00:03:00Z".to_string(),
                    updated_at: "2026-06-19T00:03:00Z".to_string(),
                },
            })
            .unwrap();

        assert_eq!(first_result.variant.ordinal, 1);
        assert_eq!(second_result.variant.ordinal, 2);
    }

    #[test]
    fn roleplay_alternative_creation_selection_and_branch_head_are_atomic() {
        let engine = test_engine();
        save_test_message_slot(
            &engine,
            "roleplay-alt-session",
            1,
            "assistant",
            "assistant",
            "primary",
        );
        save_test_branch(
            &engine,
            "roleplay-alt-session",
            "roleplay-alt-branch",
            None,
            None,
        );
        let mut message = test_message_write(
            "roleplay-alt-session",
            2,
            "assistant",
            "assistant",
            "alternate",
        );
        message.branch_id = Some(ConversationBranchId::new("roleplay-alt-branch"));
        let request = ApplyRoleplayAlternativeRequest {
            session_id: SessionId::new("roleplay-alt-session"),
            slot_id: MessageSlotId::new("roleplay-alt-session-slot-1"),
            create_variant: Some(MessageVariantWrite {
                variant_id: MessageVariantId::new("roleplay-alt-variant"),
                slot_id: MessageSlotId::new("roleplay-alt-session-slot-1"),
                source: MessageVariantSource::Alternate,
                ordinal: 0,
                status: MessageVariantStatus::Active,
                message,
                metadata_json: json!({}),
                created_at: "2026-06-19T00:02:00Z".into(),
                updated_at: "2026-06-19T00:02:00Z".into(),
            }),
            active_variant_id: Some(MessageVariantId::new("roleplay-alt-variant")),
            expected: ActiveVariantExpectation::Any,
            updated_at: "2026-06-19T00:02:00Z".into(),
        };
        let result = engine.apply_roleplay_alternative(&request).unwrap();
        assert_eq!(result.created_variant.as_ref().unwrap().ordinal, 1);
        assert_eq!(result.slot.active_variant_id, request.active_variant_id);
        assert_eq!(
            result.branch.unwrap().head_message_id,
            Some(MessageId::new("roleplay-alt-session-message-2"))
        );

        let mut losing = request.clone();
        losing.create_variant.as_mut().unwrap().variant_id =
            MessageVariantId::new("roleplay-alt-loser");
        losing.create_variant.as_mut().unwrap().message.message_id =
            MessageId::new("roleplay-alt-session-message-3");
        losing.active_variant_id = Some(MessageVariantId::new("roleplay-alt-loser"));
        losing.expected = ActiveVariantExpectation::Primary;
        let conflict = engine.apply_roleplay_alternative(&losing).unwrap();
        assert!(conflict.conflict.is_some());
        assert!(conflict.created_variant.is_none());
        assert!(engine
            .query_message_variants(&MessageVariantQuery {
                slot_id: Some(request.slot_id),
                include_deleted: false,
                page: None
            })
            .unwrap()
            .iter()
            .all(|variant| variant.variant_id != MessageVariantId::new("roleplay-alt-loser")));
    }

    #[test]
    fn create_chat_message_variant_validates_slot_session_ownership() {
        let engine = test_engine();
        save_test_message_slot(
            &engine,
            "owned-variant-session",
            1,
            "assistant",
            "assistant",
            "primary",
        );
        let message =
            test_message_write("other-variant-session", 2, "assistant", "assistant", "alt");

        let error = engine
            .create_chat_message_variant(&CreateChatMessageVariantRequest {
                session_id: SessionId::new("other-variant-session"),
                slot_id: MessageSlotId::new("owned-variant-session-slot-1"),
                variant: MessageVariantWrite {
                    variant_id: MessageVariantId::new("owned-variant-session-alt-1"),
                    slot_id: MessageSlotId::new("owned-variant-session-slot-1"),
                    source: MessageVariantSource::Alternate,
                    ordinal: 0,
                    status: MessageVariantStatus::Active,
                    message,
                    metadata_json: json!({}),
                    created_at: "2026-06-19T00:02:00Z".to_string(),
                    updated_at: "2026-06-19T00:02:00Z".to_string(),
                },
            })
            .unwrap_err();

        assert_eq!(error.kind, CoreErrorKind::NotFound);
    }

    #[test]
    fn ensure_active_chat_conversation_branch_creates_and_selects_default() {
        let engine = test_engine();

        let result = engine
            .ensure_active_chat_conversation_branch(&EnsureActiveChatConversationBranchRequest {
                session_id: SessionId::new("ensure-branch-session"),
                branch_id: ConversationBranchId::new("ensure-branch-default"),
                label: Some("Default".to_string()),
                metadata_json: json!({ "source": "test" }),
                created_at: "2026-06-19T00:01:00Z".to_string(),
                updated_at: "2026-06-19T00:01:00Z".to_string(),
            })
            .unwrap();

        assert_eq!(
            result.branch.branch_id,
            ConversationBranchId::new("ensure-branch-default")
        );
        assert_eq!(
            result.state.active_branch_id,
            Some(ConversationBranchId::new("ensure-branch-default"))
        );
        assert!(result.conflict.is_none());
    }

    #[test]
    fn ensure_active_chat_conversation_branch_selects_existing_default_when_none_active() {
        let engine = test_engine();
        save_test_branch(
            &engine,
            "ensure-existing-session",
            "ensure-existing-default",
            None,
            None,
        );

        let result = engine
            .ensure_active_chat_conversation_branch(&EnsureActiveChatConversationBranchRequest {
                session_id: SessionId::new("ensure-existing-session"),
                branch_id: ConversationBranchId::new("ensure-existing-default"),
                label: Some("Default".to_string()),
                metadata_json: json!({}),
                created_at: "2026-06-19T00:01:00Z".to_string(),
                updated_at: "2026-06-19T00:02:00Z".to_string(),
            })
            .unwrap();

        assert_eq!(
            result.state.active_branch_id,
            Some(ConversationBranchId::new("ensure-existing-default"))
        );
        assert!(result.conflict.is_none());
    }

    #[test]
    fn ensure_active_chat_conversation_branch_returns_active_conflict() {
        let engine = test_engine();
        save_test_branch(
            &engine,
            "ensure-conflict-session",
            "ensure-conflict-active",
            None,
            None,
        );
        engine
            .select_active_conversation_branch(&SelectActiveBranchRequest {
                session_id: SessionId::new("ensure-conflict-session"),
                active_branch_id: Some(ConversationBranchId::new("ensure-conflict-active")),
                expected: rusty_crew_core_persistence::ActiveBranchExpectation::Any,
                updated_at: "2026-06-19T00:01:00Z".to_string(),
            })
            .unwrap();

        let result = engine
            .ensure_active_chat_conversation_branch(&EnsureActiveChatConversationBranchRequest {
                session_id: SessionId::new("ensure-conflict-session"),
                branch_id: ConversationBranchId::new("ensure-conflict-default"),
                label: Some("Default".to_string()),
                metadata_json: json!({}),
                created_at: "2026-06-19T00:02:00Z".to_string(),
                updated_at: "2026-06-19T00:02:00Z".to_string(),
            })
            .unwrap();

        assert_eq!(
            result.branch.branch_id,
            ConversationBranchId::new("ensure-conflict-active")
        );
        assert_eq!(
            result.conflict,
            Some(rusty_crew_core_persistence::ActiveBranchConflict {
                expected: None,
                actual: Some(ConversationBranchId::new("ensure-conflict-active")),
            })
        );
    }

    #[test]
    fn create_chat_conversation_branch_rejects_wrong_session_parent_and_head() {
        let engine = test_engine();
        save_test_message_slot(
            &engine,
            "branch-owner-session",
            1,
            "assistant",
            "assistant",
            "primary",
        );
        save_test_message_slot(
            &engine,
            "branch-other-session",
            1,
            "assistant",
            "assistant",
            "primary",
        );
        save_test_branch(
            &engine,
            "branch-other-session",
            "branch-other-parent",
            None,
            None,
        );

        let parent_error = engine
            .create_chat_conversation_branch(&CreateChatConversationBranchRequest {
                branch: test_branch_write(
                    "branch-owner-session",
                    "branch-owner-child",
                    Some("branch-other-parent"),
                    Some("branch-owner-session-message-1"),
                ),
            })
            .unwrap_err();
        assert_eq!(parent_error.kind, CoreErrorKind::NotFound);

        let head_error = engine
            .create_chat_conversation_branch(&CreateChatConversationBranchRequest {
                branch: test_branch_write(
                    "branch-owner-session",
                    "branch-owner-child-2",
                    None,
                    Some("branch-other-session-message-1"),
                ),
            })
            .unwrap_err();
        assert_eq!(head_error.kind, CoreErrorKind::NotFound);
    }

    #[test]
    fn create_chat_conversation_snapshot_rejects_cross_session_snapshot_collision() {
        let engine = test_engine();
        let first = engine
            .create_chat_conversation_snapshot(&CreateChatConversationSnapshotRequest {
                snapshot: test_snapshot_write("snapshot-session-a", "shared-snapshot", None, None),
            })
            .unwrap();
        assert_eq!(
            first.status,
            ChatConversationSnapshotMutationStatus::Created
        );

        let updated = engine
            .create_chat_conversation_snapshot(&CreateChatConversationSnapshotRequest {
                snapshot: ConversationSnapshotWrite {
                    label: Some("Updated".to_string()),
                    created_at: "2026-06-19T00:09:00Z".to_string(),
                    updated_at: "2026-06-19T00:09:00Z".to_string(),
                    ..test_snapshot_write("snapshot-session-a", "shared-snapshot", None, None)
                },
            })
            .unwrap();
        assert_eq!(
            updated.status,
            ChatConversationSnapshotMutationStatus::Updated
        );
        assert_eq!(
            updated.snapshot.created_at,
            "2026-06-19T00:01:00Z".to_string()
        );
        assert_eq!(updated.snapshot.label, Some("Updated".to_string()));

        let error = engine
            .create_chat_conversation_snapshot(&CreateChatConversationSnapshotRequest {
                snapshot: test_snapshot_write("snapshot-session-b", "shared-snapshot", None, None),
            })
            .unwrap_err();
        assert_eq!(error.kind, CoreErrorKind::NotFound);

        let records = engine
            .query_conversation_snapshots(&ConversationSnapshotQuery {
                session_id: Some(SessionId::new("snapshot-session-a")),
                branch_id: None,
                message_id: None,
                page: None,
            })
            .unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].session_id, SessionId::new("snapshot-session-a"));
    }

    #[test]
    fn create_chat_conversation_snapshot_validates_branch_and_message_ownership() {
        let engine = test_engine();
        save_test_branch(
            &engine,
            "snapshot-owner-session",
            "snapshot-owner-branch",
            None,
            None,
        );
        save_test_branch(
            &engine,
            "snapshot-other-session",
            "snapshot-other-branch",
            None,
            None,
        );
        save_test_message_slot(
            &engine,
            "snapshot-other-session",
            1,
            "assistant",
            "assistant",
            "other",
        );

        let wrong_branch = engine
            .create_chat_conversation_snapshot(&CreateChatConversationSnapshotRequest {
                snapshot: test_snapshot_write(
                    "snapshot-owner-session",
                    "wrong-branch-snapshot",
                    Some("snapshot-other-branch"),
                    None,
                ),
            })
            .unwrap_err();
        assert_eq!(wrong_branch.kind, CoreErrorKind::NotFound);

        let wrong_message = engine
            .create_chat_conversation_snapshot(&CreateChatConversationSnapshotRequest {
                snapshot: test_snapshot_write(
                    "snapshot-owner-session",
                    "wrong-message-snapshot",
                    None,
                    Some("snapshot-other-session-message-1"),
                ),
            })
            .unwrap_err();
        assert_eq!(wrong_message.kind, CoreErrorKind::NotFound);
    }

    #[test]
    fn create_chat_conversation_snapshot_allows_same_session_branch_and_message_anchors() {
        let engine = test_engine();
        save_test_branch(
            &engine,
            "snapshot-branch-session",
            "snapshot-branch-a",
            None,
            None,
        );
        save_test_branch(
            &engine,
            "snapshot-branch-session",
            "snapshot-branch-b",
            None,
            None,
        );
        engine
            .save_message_slot(&MessageSlotWrite {
                slot_id: MessageSlotId::new("snapshot-branch-slot"),
                session_id: SessionId::new("snapshot-branch-session"),
                primary_variant_id: MessageVariantId::new("snapshot-branch-primary"),
                active_variant_id: None,
                metadata_json: json!({}),
                created_at: "2026-06-19T00:01:00Z".to_string(),
                updated_at: "2026-06-19T00:01:00Z".to_string(),
            })
            .unwrap();
        let mut message = test_message_write(
            "snapshot-branch-session",
            1,
            "assistant",
            "assistant",
            "body",
        );
        message.branch_id = Some(ConversationBranchId::new("snapshot-branch-b"));
        engine
            .save_message_variant(&MessageVariantWrite {
                variant_id: MessageVariantId::new("snapshot-branch-primary"),
                slot_id: MessageSlotId::new("snapshot-branch-slot"),
                source: MessageVariantSource::Primary,
                ordinal: 0,
                status: MessageVariantStatus::Active,
                message,
                metadata_json: json!({}),
                created_at: "2026-06-19T00:01:00Z".to_string(),
                updated_at: "2026-06-19T00:01:00Z".to_string(),
            })
            .unwrap();

        let result = engine
            .create_chat_conversation_snapshot(&CreateChatConversationSnapshotRequest {
                snapshot: test_snapshot_write(
                    "snapshot-branch-session",
                    "independent-branch-message-snapshot",
                    Some("snapshot-branch-a"),
                    Some("snapshot-branch-session-message-1"),
                ),
            })
            .unwrap();
        assert_eq!(
            result.status,
            ChatConversationSnapshotMutationStatus::Created
        );
    }

    #[test]
    fn create_chat_conversation_snapshot_allows_message_referenced_by_branch_head() {
        let engine = test_engine();
        save_test_message_slot(
            &engine,
            "snapshot-head-session",
            1,
            "assistant",
            "assistant",
            "head",
        );
        save_test_branch(
            &engine,
            "snapshot-head-session",
            "snapshot-head-branch",
            None,
            Some("snapshot-head-session-message-1"),
        );

        let result = engine
            .create_chat_conversation_snapshot(&CreateChatConversationSnapshotRequest {
                snapshot: test_snapshot_write(
                    "snapshot-head-session",
                    "snapshot-head-snapshot",
                    Some("snapshot-head-branch"),
                    Some("snapshot-head-session-message-1"),
                ),
            })
            .unwrap();

        assert_eq!(
            result.status,
            ChatConversationSnapshotMutationStatus::Created
        );
    }

    #[test]
    fn resolve_conversation_jump_rejects_wrong_session_targets() {
        let engine = test_engine();
        save_test_branch(
            &engine,
            "jump-owner-session",
            "jump-owner-branch",
            None,
            None,
        );
        save_test_message_slot(
            &engine,
            "jump-owner-session",
            1,
            "assistant",
            "assistant",
            "owner",
        );
        engine
            .create_chat_conversation_snapshot(&CreateChatConversationSnapshotRequest {
                snapshot: test_snapshot_write(
                    "jump-owner-session",
                    "jump-owner-snapshot",
                    Some("jump-owner-branch"),
                    None,
                ),
            })
            .unwrap();

        for target in [
            ConversationJumpTarget::Branch {
                branch_id: ConversationBranchId::new("jump-owner-branch"),
            },
            ConversationJumpTarget::Message {
                message_id: MessageId::new("jump-owner-session-message-1"),
            },
            ConversationJumpTarget::Snapshot {
                snapshot_id: ConversationSnapshotId::new("jump-owner-snapshot"),
            },
        ] {
            let error = engine
                .resolve_conversation_jump(&ConversationJumpRequest {
                    session_id: SessionId::new("jump-other-session"),
                    target,
                })
                .unwrap_err();
            assert_eq!(error.kind, CoreErrorKind::NotFound);
        }
    }

    #[test]
    fn create_chat_attachment_rejects_cross_session_attachment_collision() {
        let engine = test_engine();
        let first = engine
            .create_chat_attachment(&CreateChatAttachmentRequest {
                attachment: test_attachment_write(
                    "attachment-session-a",
                    "shared-attachment",
                    None,
                ),
            })
            .unwrap();
        assert_eq!(first.status, ChatAttachmentMutationStatus::Created);

        let error = engine
            .create_chat_attachment(&CreateChatAttachmentRequest {
                attachment: test_attachment_write(
                    "attachment-session-b",
                    "shared-attachment",
                    None,
                ),
            })
            .unwrap_err();

        assert_eq!(error.kind, CoreErrorKind::NotFound);
        let records = engine
            .query_attachments(&AttachmentQuery {
                session_id: Some(SessionId::new("attachment-session-a")),
                include_removed: true,
                include_expired: true,
                ..AttachmentQuery::default()
            })
            .unwrap();
        assert_eq!(
            records[0].session_id,
            SessionId::new("attachment-session-a")
        );
    }

    #[test]
    fn create_chat_attachment_validates_link_targets() {
        let engine = test_engine();
        save_test_message_slot(
            &engine,
            "attachment-link-session",
            1,
            "assistant",
            "assistant",
            "primary",
        );
        save_test_message_slot(
            &engine,
            "attachment-other-session",
            1,
            "assistant",
            "assistant",
            "primary",
        );

        let linked = engine
            .create_chat_attachment(&CreateChatAttachmentRequest {
                attachment: test_attachment_write(
                    "attachment-link-session",
                    "linked-attachment",
                    Some(AttachmentLinkWrite {
                        link_id: AttachmentLinkId::new("linked-attachment-link"),
                        attachment_id: AttachmentId::new("linked-attachment"),
                        session_id: SessionId::new("attachment-link-session"),
                        message_id: Some(MessageId::new("attachment-link-session-message-1")),
                        block_id: None,
                        scope_id: None,
                        metadata_json: json!({}),
                        created_at: "2026-06-19T00:01:00Z".to_string(),
                    }),
                ),
            })
            .unwrap();
        assert_eq!(linked.status, ChatAttachmentMutationStatus::Linked);
        assert_eq!(linked.attachment.links.len(), 1);

        let error = engine
            .create_chat_attachment(&CreateChatAttachmentRequest {
                attachment: test_attachment_write(
                    "attachment-link-session",
                    "wrong-link-attachment",
                    Some(AttachmentLinkWrite {
                        link_id: AttachmentLinkId::new("wrong-link-attachment-link"),
                        attachment_id: AttachmentId::new("wrong-link-attachment"),
                        session_id: SessionId::new("attachment-link-session"),
                        message_id: Some(MessageId::new("attachment-other-session-message-1")),
                        block_id: None,
                        scope_id: None,
                        metadata_json: json!({}),
                        created_at: "2026-06-19T00:01:00Z".to_string(),
                    }),
                ),
            })
            .unwrap_err();
        assert_eq!(error.kind, CoreErrorKind::NotFound);
    }

    #[test]
    fn remove_chat_attachment_is_session_scoped() {
        let engine = test_engine();
        engine
            .create_chat_attachment(&CreateChatAttachmentRequest {
                attachment: test_attachment_write(
                    "remove-attachment-session",
                    "remove-attachment",
                    None,
                ),
            })
            .unwrap();

        let error = engine
            .remove_chat_attachment(&RemoveChatAttachmentRequest {
                session_id: SessionId::new("remove-other-session"),
                attachment_id: AttachmentId::new("remove-attachment"),
                updated_at: "2026-06-19T00:02:00Z".to_string(),
            })
            .unwrap_err();
        assert_eq!(error.kind, CoreErrorKind::NotFound);

        let record = engine
            .remove_chat_attachment(&RemoveChatAttachmentRequest {
                session_id: SessionId::new("remove-attachment-session"),
                attachment_id: AttachmentId::new("remove-attachment"),
                updated_at: "2026-06-19T00:03:00Z".to_string(),
            })
            .unwrap();
        assert_eq!(record.status, AttachmentStatus::Removed);
    }

    #[test]
    fn create_chat_data_bank_scope_rejects_cross_session_scope_collision() {
        let engine = test_engine();
        let first = engine
            .create_chat_data_bank_scope(&CreateChatDataBankScopeRequest {
                scope: test_data_bank_scope_write("scope-session-a", "shared-scope"),
            })
            .unwrap();
        assert_eq!(first.status, ChatDataBankScopeMutationStatus::Created);

        let error = engine
            .create_chat_data_bank_scope(&CreateChatDataBankScopeRequest {
                scope: test_data_bank_scope_write("scope-session-b", "shared-scope"),
            })
            .unwrap_err();
        assert_eq!(error.kind, CoreErrorKind::NotFound);

        let records = engine
            .query_data_bank_scopes(&DataBankScopeQuery {
                session_id: Some(SessionId::new("scope-session-a")),
                include_removed: true,
                ..DataBankScopeQuery::default()
            })
            .unwrap();
        assert_eq!(records[0].session_id, SessionId::new("scope-session-a"));
    }

    #[test]
    fn remove_chat_data_bank_scope_is_session_scoped() {
        let engine = test_engine();
        engine
            .create_chat_data_bank_scope(&CreateChatDataBankScopeRequest {
                scope: test_data_bank_scope_write("remove-scope-session", "remove-scope"),
            })
            .unwrap();

        let error = engine
            .remove_chat_data_bank_scope(&RemoveChatDataBankScopeRequest {
                session_id: SessionId::new("remove-other-session"),
                scope_id: DataBankScopeId::new("remove-scope"),
                updated_at: "2026-06-19T00:02:00Z".to_string(),
            })
            .unwrap_err();
        assert_eq!(error.kind, CoreErrorKind::NotFound);

        let record = engine
            .remove_chat_data_bank_scope(&RemoveChatDataBankScopeRequest {
                session_id: SessionId::new("remove-scope-session"),
                scope_id: DataBankScopeId::new("remove-scope"),
                updated_at: "2026-06-19T00:03:00Z".to_string(),
            })
            .unwrap();
        assert_eq!(record.status, DataBankScopeStatus::Removed);
    }

    #[test]
    fn delete_chat_message_variant_validates_slot_session_ownership() {
        let engine = test_engine();
        save_test_message_slot(
            &engine,
            "delete-owned-session",
            1,
            "assistant",
            "assistant",
            "primary",
        );
        save_test_alternate_variant(&engine, "delete-owned-session", 1, 2, "alt");

        let error = engine
            .delete_chat_message_variant(&DeleteChatMessageVariantRequest {
                session_id: SessionId::new("other-delete-session"),
                slot_id: MessageSlotId::new("delete-owned-session-slot-1"),
                variant_id: MessageVariantId::new("delete-owned-session-variant-2-alt"),
                updated_at: "2026-06-19T00:04:00Z".to_string(),
            })
            .unwrap_err();

        assert_eq!(error.kind, CoreErrorKind::NotFound);
    }

    #[test]
    fn delete_chat_message_variant_rejects_primary_variant() {
        let engine = test_engine();
        save_test_message_slot(
            &engine,
            "delete-primary-session",
            1,
            "assistant",
            "assistant",
            "primary",
        );

        let error = engine
            .delete_chat_message_variant(&DeleteChatMessageVariantRequest {
                session_id: SessionId::new("delete-primary-session"),
                slot_id: MessageSlotId::new("delete-primary-session-slot-1"),
                variant_id: MessageVariantId::new("delete-primary-session-variant-1-primary"),
                updated_at: "2026-06-19T00:04:00Z".to_string(),
            })
            .unwrap_err();

        assert_eq!(error.kind, CoreErrorKind::InvalidInput);
    }

    #[test]
    fn delete_chat_message_variant_clears_active_alternate() {
        let engine = test_engine();
        save_test_message_slot(
            &engine,
            "delete-active-session",
            1,
            "assistant",
            "assistant",
            "primary",
        );
        save_test_alternate_variant(&engine, "delete-active-session", 1, 2, "alt");
        engine
            .select_active_message_variant(&SelectActiveVariantRequest {
                slot_id: MessageSlotId::new("delete-active-session-slot-1"),
                active_variant_id: Some(MessageVariantId::new(
                    "delete-active-session-variant-2-alt",
                )),
                expected: rusty_crew_core_persistence::ActiveVariantExpectation::Any,
                updated_at: "2026-06-19T00:03:00Z".to_string(),
            })
            .unwrap();

        let slot = engine
            .delete_chat_message_variant(&DeleteChatMessageVariantRequest {
                session_id: SessionId::new("delete-active-session"),
                slot_id: MessageSlotId::new("delete-active-session-slot-1"),
                variant_id: MessageVariantId::new("delete-active-session-variant-2-alt"),
                updated_at: "2026-06-19T00:04:00Z".to_string(),
            })
            .unwrap();

        assert_eq!(slot.active_variant_id, None);
        assert!(slot.alternates.is_empty());
    }

    #[test]
    fn reorder_chat_message_variants_validates_session_and_reorders() {
        let engine = test_engine();
        save_test_message_slot(
            &engine,
            "reorder-session",
            1,
            "assistant",
            "assistant",
            "primary",
        );
        save_test_alternate_variant(&engine, "reorder-session", 1, 2, "alt 1");
        save_test_alternate_variant(&engine, "reorder-session", 1, 3, "alt 2");

        let mismatch = engine
            .reorder_chat_message_variants(&ReorderChatMessageVariantsRequest {
                session_id: SessionId::new("other-reorder-session"),
                slot_id: MessageSlotId::new("reorder-session-slot-1"),
                ordered_variant_ids: vec![
                    MessageVariantId::new("reorder-session-variant-3-alt"),
                    MessageVariantId::new("reorder-session-variant-2-alt"),
                ],
                updated_at: "2026-06-19T00:04:00Z".to_string(),
            })
            .unwrap_err();
        assert_eq!(mismatch.kind, CoreErrorKind::NotFound);

        let variants = engine
            .reorder_chat_message_variants(&ReorderChatMessageVariantsRequest {
                session_id: SessionId::new("reorder-session"),
                slot_id: MessageSlotId::new("reorder-session-slot-1"),
                ordered_variant_ids: vec![
                    MessageVariantId::new("reorder-session-variant-3-alt"),
                    MessageVariantId::new("reorder-session-variant-2-alt"),
                ],
                updated_at: "2026-06-19T00:05:00Z".to_string(),
            })
            .unwrap();

        let alternate_order = variants
            .iter()
            .filter(|variant| variant.source == MessageVariantSource::Alternate)
            .map(|variant| (variant.variant_id.clone(), variant.ordinal))
            .collect::<Vec<_>>();
        assert_eq!(
            alternate_order,
            vec![
                (MessageVariantId::new("reorder-session-variant-3-alt"), 1),
                (MessageVariantId::new("reorder-session-variant-2-alt"), 2),
            ]
        );
    }

    #[test]
    fn reorder_chat_message_variants_rejects_primary_variant() {
        let engine = test_engine();
        save_test_message_slot(
            &engine,
            "reorder-primary-session",
            1,
            "assistant",
            "assistant",
            "primary",
        );

        let error = engine
            .reorder_chat_message_variants(&ReorderChatMessageVariantsRequest {
                session_id: SessionId::new("reorder-primary-session"),
                slot_id: MessageSlotId::new("reorder-primary-session-slot-1"),
                ordered_variant_ids: vec![MessageVariantId::new(
                    "reorder-primary-session-variant-1-primary",
                )],
                updated_at: "2026-06-19T00:04:00Z".to_string(),
            })
            .unwrap_err();

        assert_eq!(error.kind, CoreErrorKind::InvalidInput);
    }

    fn save_test_message_slot(
        engine: &CoreEngine,
        session_id: &str,
        ordinal: u32,
        author_id: &str,
        author_role: &str,
        body: &str,
    ) {
        let timestamp = format!("2026-06-19T00:{ordinal:02}:00Z");
        engine
            .save_message_slot(&MessageSlotWrite {
                slot_id: MessageSlotId::new(format!("{session_id}-slot-{ordinal}")),
                session_id: SessionId::new(session_id),
                primary_variant_id: MessageVariantId::new(format!(
                    "{session_id}-variant-{ordinal}-primary"
                )),
                active_variant_id: None,
                metadata_json: json!({}),
                created_at: timestamp.clone(),
                updated_at: timestamp.clone(),
            })
            .unwrap();
        engine
            .save_message_variant(&MessageVariantWrite {
                variant_id: MessageVariantId::new(format!(
                    "{session_id}-variant-{ordinal}-primary"
                )),
                slot_id: MessageSlotId::new(format!("{session_id}-slot-{ordinal}")),
                source: MessageVariantSource::Primary,
                ordinal: 0,
                status: MessageVariantStatus::Active,
                message: test_message_write(session_id, ordinal, author_id, author_role, body),
                metadata_json: json!({}),
                created_at: timestamp.clone(),
                updated_at: timestamp,
            })
            .unwrap();
    }

    fn save_test_alternate_variant(
        engine: &CoreEngine,
        session_id: &str,
        slot_ordinal: u32,
        variant_ordinal: u32,
        body: &str,
    ) {
        let timestamp = format!("2026-06-19T00:{variant_ordinal:02}:00Z");
        engine
            .save_message_variant(&MessageVariantWrite {
                variant_id: MessageVariantId::new(format!(
                    "{session_id}-variant-{variant_ordinal}-alt"
                )),
                slot_id: MessageSlotId::new(format!("{session_id}-slot-{slot_ordinal}")),
                source: MessageVariantSource::Alternate,
                ordinal: variant_ordinal.saturating_sub(1),
                status: MessageVariantStatus::Active,
                message: test_message_write(
                    session_id,
                    variant_ordinal,
                    "assistant",
                    "assistant",
                    body,
                ),
                metadata_json: json!({}),
                created_at: timestamp.clone(),
                updated_at: timestamp,
            })
            .unwrap();
    }

    fn save_test_branch(
        engine: &CoreEngine,
        session_id: &str,
        branch_id: &str,
        parent_branch_id: Option<&str>,
        head_message_id: Option<&str>,
    ) {
        engine
            .save_conversation_branch(&test_branch_write(
                session_id,
                branch_id,
                parent_branch_id,
                head_message_id,
            ))
            .unwrap();
    }

    fn test_branch_write(
        session_id: &str,
        branch_id: &str,
        parent_branch_id: Option<&str>,
        head_message_id: Option<&str>,
    ) -> ConversationBranchWrite {
        ConversationBranchWrite {
            branch_id: ConversationBranchId::new(branch_id),
            session_id: SessionId::new(session_id),
            parent_branch_id: parent_branch_id.map(ConversationBranchId::new),
            parent_message_id: None,
            origin_message_id: None,
            head_message_id: head_message_id.map(MessageId::new),
            label: Some("Branch".to_string()),
            metadata_json: json!({}),
            created_at: "2026-06-19T00:00:00Z".to_string(),
            updated_at: "2026-06-19T00:00:00Z".to_string(),
        }
    }

    fn test_snapshot_write(
        session_id: &str,
        snapshot_id: &str,
        branch_id: Option<&str>,
        message_id: Option<&str>,
    ) -> ConversationSnapshotWrite {
        ConversationSnapshotWrite {
            snapshot_id: ConversationSnapshotId::new(snapshot_id),
            session_id: SessionId::new(session_id),
            branch_id: branch_id.map(ConversationBranchId::new),
            message_id: message_id.map(MessageId::new),
            cursor: Some(format!("{session_id}:cursor")),
            label: Some("Snapshot".to_string()),
            summary: Some("Snapshot summary".to_string()),
            source: ConversationSnapshotSource::User,
            metadata_json: json!({}),
            created_at: "2026-06-19T00:01:00Z".to_string(),
            updated_at: "2026-06-19T00:01:00Z".to_string(),
        }
    }

    fn test_attachment_write(
        session_id: &str,
        attachment_id: &str,
        link: Option<AttachmentLinkWrite>,
    ) -> AttachmentWrite {
        AttachmentWrite {
            attachment_id: AttachmentId::new(attachment_id),
            session_id: SessionId::new(session_id),
            status: AttachmentStatus::Active,
            filename: format!("{attachment_id}.txt"),
            mime_type: "text/plain".to_string(),
            byte_size: 32,
            storage_url: None,
            download_url: None,
            thumbnail_url: None,
            extracted_text: Some("attachment body".to_string()),
            extracted_text_truncated: false,
            metadata_json: json!({}),
            created_at: "2026-06-19T00:01:00Z".to_string(),
            updated_at: "2026-06-19T00:01:00Z".to_string(),
            expires_at: None,
            link,
        }
    }

    fn test_data_bank_scope_write(session_id: &str, scope_id: &str) -> DataBankScopeWrite {
        DataBankScopeWrite {
            scope_id: DataBankScopeId::new(scope_id),
            session_id: SessionId::new(session_id),
            status: DataBankScopeStatus::Active,
            label: Some(format!("Scope {scope_id}")),
            description: Some("Reusable scope".to_string()),
            metadata_json: json!({}),
            created_at: "2026-06-19T00:01:00Z".to_string(),
            updated_at: "2026-06-19T00:01:00Z".to_string(),
        }
    }

    fn test_message_write(
        session_id: &str,
        ordinal: u32,
        author_id: &str,
        author_role: &str,
        body: &str,
    ) -> DurableMessageWrite {
        DurableMessageWrite {
            message_id: MessageId::new(format!("{session_id}-message-{ordinal}")),
            session_id: SessionId::new(session_id),
            branch_id: None,
            parent_message_id: None,
            previous_message_id: None,
            author_id: author_id.to_string(),
            author_role: author_role.to_string(),
            status: DurableMessageStatus::Completed,
            body: body.to_string(),
            metadata_json: json!({ "correlation_id": format!("correlation-{ordinal}") }),
            created_at: format!("2026-06-19T00:{ordinal:02}:00Z"),
            blocks: Vec::new(),
        }
    }

    fn chat_slot_ingest_request(
        session_id: &str,
        ordinal: u32,
        idempotency_key: &str,
    ) -> CreateChatMessageSlotRequest {
        let branch_id = ConversationBranchId::new(format!("branch:{session_id}:default"));
        let slot_id = MessageSlotId::new(format!("slot:{session_id}:{idempotency_key}"));
        let variant_id = MessageVariantId::new(format!("variant:{session_id}:{idempotency_key}"));
        let message_id = MessageId::new(format!("message:{session_id}:{idempotency_key}"));
        let timestamp = format!("2026-06-19T00:{ordinal:02}:00Z");
        let mut message = test_message_write(session_id, ordinal, "user", "user", "hello");
        message.message_id = message_id;
        message.branch_id = Some(branch_id.clone());
        CreateChatMessageSlotRequest {
            slot: MessageSlotWrite {
                slot_id: slot_id.clone(),
                session_id: SessionId::new(session_id),
                primary_variant_id: variant_id.clone(),
                active_variant_id: None,
                metadata_json: json!({"idempotency_key": idempotency_key}),
                created_at: timestamp.clone(),
                updated_at: timestamp.clone(),
            },
            primary_variant: MessageVariantWrite {
                variant_id,
                slot_id,
                source: MessageVariantSource::Primary,
                ordinal: 0,
                status: MessageVariantStatus::Active,
                message,
                metadata_json: json!({}),
                created_at: timestamp.clone(),
                updated_at: timestamp.clone(),
            },
            branch_id: branch_id.clone(),
            expected_branch_head: BranchHeadExpectation::Any,
            updated_at: timestamp.clone(),
            ensure_active_branch: Some(EnsureActiveChatConversationBranchRequest {
                session_id: SessionId::new(session_id),
                branch_id,
                label: Some("Default".to_string()),
                metadata_json: json!({"source": "test"}),
                created_at: timestamp.clone(),
                updated_at: timestamp,
            }),
            inherit_branch_head: true,
            idempotency_key: Some(idempotency_key.to_string()),
        }
    }

    fn test_engine() -> CoreEngine {
        test_engine_with_data_dir(unique_data_dir("engine"))
    }

    #[test]
    fn github_gate_wait_is_durable_idempotent_and_recovers_exact_session_wake() {
        let data_dir = unique_data_dir("github-gate-wait");
        let engine = test_engine_with_data_dir(data_dir.clone());
        let session = engine
            .create_session(session_config(
                "gate-session",
                "gate-agent",
                "gate-profile",
                SessionKind::Full,
            ))
            .unwrap();
        let wait = engine
            .suspend_for_github_gate(GitHubGateSuspendRequest {
                session_id: session.session_id.clone(),
                run_id: Some(RunId::new("run-1")),
                provider_thread_id: Some("thread-1".to_string()),
                project_id: ProjectId::new("den-services"),
                task_id: TaskId::new("5500"),
                gate_id: 901,
                commit_sha: "1111111111111111111111111111111111111111".to_string(),
                now: "2026-06-19T00:00:10Z".to_string(),
            })
            .unwrap();
        assert_eq!(wait.phase, GitHubGateWaitPhase::Waiting);
        assert_eq!(
            engine.get_session(&session.session_id).unwrap().status,
            SessionStatus::Idle
        );

        let (_, receiver) = engine
            .subscribe_events(EventSubscription {
                event_kinds: vec![CoreEventKind::BrainWakeRequested],
                session_id: Some(session.session_id.clone()),
                agent_id: None,
                adapter_id: None,
            })
            .unwrap();
        let event = GitHubGateTerminalEvent {
            event_id: 44,
            gate_id: 901,
            project_id: ProjectId::new("den-services"),
            task_id: TaskId::new("5500"),
            commit_sha: "1111111111111111111111111111111111111111".to_string(),
            status: "failed".to_string(),
            terminal_reason: "required_checks_missing".to_string(),
            summary: Some("wrong check name".to_string()),
            failure_summary: Some("missing Verify".to_string()),
            completed_at: "2026-06-19T00:01:00Z".to_string(),
        };
        let receipt = engine
            .consume_github_gate_terminal_event(event.clone())
            .unwrap();
        assert!(receipt.wake_scheduled);
        assert!(matches!(
            receiver.recv_timeout(std::time::Duration::from_millis(50)),
            Ok(CoreEvent::BrainWakeRequested { session_id }) if session_id == session.session_id
        ));
        let duplicate = engine.consume_github_gate_terminal_event(event).unwrap();
        assert!(duplicate.duplicate);
        assert!(!duplicate.wake_scheduled);
        let queued = engine
            .store
            .load_queued_messages(&rusty_crew_core_persistence::QueuedMessageFilter {
                state: Some(QueuedMessageState::Pending),
                owner_session_id: Some(session.session_id.clone()),
                owner_agent_id: None,
                limit: None,
            })
            .unwrap();
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].message_id, "github-gate-event:44");
        assert!(queued[0].message.body.contains("required_checks_missing"));
        drop(engine);

        let hydrated = test_engine_with_data_dir(data_dir);
        let persisted = hydrated
            .github_gate_wait(&session.session_id)
            .unwrap()
            .unwrap();
        assert_eq!(persisted.phase, GitHubGateWaitPhase::WakeScheduled);
        let (_, recovered_receiver) = hydrated
            .subscribe_events(EventSubscription {
                event_kinds: vec![CoreEventKind::BrainWakeRequested],
                session_id: Some(session.session_id.clone()),
                agent_id: None,
                adapter_id: None,
            })
            .unwrap();
        assert_eq!(hydrated.recover_github_gate_wakes().unwrap(), 1);
        assert!(recovered_receiver
            .recv_timeout(std::time::Duration::from_millis(50))
            .is_ok());
    }

    #[test]
    fn newer_github_gate_wait_rejects_stale_sha_terminal_event() {
        let engine = test_engine();
        let session = engine
            .create_session(session_config(
                "newer-gate-session",
                "gate-agent",
                "gate-profile",
                SessionKind::Full,
            ))
            .unwrap();
        for (gate_id, sha) in [
            (1, "1111111111111111111111111111111111111111"),
            (2, "2222222222222222222222222222222222222222"),
        ] {
            engine
                .suspend_for_github_gate(GitHubGateSuspendRequest {
                    session_id: session.session_id.clone(),
                    run_id: None,
                    provider_thread_id: None,
                    project_id: ProjectId::new("den-services"),
                    task_id: TaskId::new("5500"),
                    gate_id,
                    commit_sha: sha.to_string(),
                    now: "2026-06-19T00:00:10Z".to_string(),
                })
                .unwrap();
        }
        let receipt = engine
            .consume_github_gate_terminal_event(GitHubGateTerminalEvent {
                event_id: 1,
                gate_id: 1,
                project_id: ProjectId::new("den-services"),
                task_id: TaskId::new("5500"),
                commit_sha: "1111111111111111111111111111111111111111".to_string(),
                status: "superseded".to_string(),
                terminal_reason: "superseded".to_string(),
                summary: None,
                failure_summary: None,
                completed_at: "2026-06-19T00:01:00Z".to_string(),
            })
            .unwrap();
        assert!(!receipt.wake_scheduled);
        assert_eq!(
            receipt.ignored_reason.as_deref(),
            Some("no_current_wait_for_gate_and_sha")
        );
        assert_eq!(
            engine
                .github_gate_wait(&session.session_id)
                .unwrap()
                .unwrap()
                .gate_id,
            2
        );
    }

    fn test_engine_with_data_dir(data_dir: PathBuf) -> CoreEngine {
        CoreEngine::initialize(test_engine_config(data_dir)).unwrap()
    }

    fn test_engine_config(data_dir: PathBuf) -> EngineConfig {
        EngineConfig {
            engine_data_dir: data_dir.to_string_lossy().to_string(),
            clock: ClockConfig::Fixed {
                at: "2026-06-19T00:00:00Z".to_string(),
            },
            default_turn_budget: 3,
            default_idle_timeout_ms: 1000,
            storage: None,
        }
    }

    fn unique_data_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "rusty-crew-{name}-{}-{}",
            std::process::id(),
            NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        let _ = std::fs::remove_file(&path);
        path
    }

    fn assert_receiver_disconnects_after_buffered_events(
        receiver: std::sync::mpsc::Receiver<CoreEvent>,
    ) {
        for _ in 0..8 {
            match receiver.recv_timeout(Duration::from_millis(10)) {
                Ok(_) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    panic!("subscriber receiver remained open after shutdown")
                }
            }
        }
        panic!("subscriber receiver still had buffered events after shutdown");
    }

    fn spawn_delegated(
        engine: &CoreEngine,
        planner: &SessionState,
        wake_id: &str,
        max_duration_ms: Option<u32>,
    ) -> SessionId {
        engine
            .execute_brain_actions(BrainActionBatch {
                wake_id: wake_id.to_string(),
                session_id: planner.session_id.clone(),
                actions: vec![BrainAction::RequestDelegation {
                    profile_id: ProfileId::new("coder-profile"),
                    task_id: None,
                    prompt: "complete a delegated lifecycle slice".to_string(),
                    expected_output: None,
                    resource_limits: Some(ResourceLimits {
                        workdir: Some("/home/dev/rusty-crew".to_string()),
                        max_duration_ms,
                        max_delegation_depth: Some(0),
                    }),
                    timeout_ms: max_duration_ms,
                    priority: None,
                    fan_out_group_id: None,
                    fan_out_max_concurrency: None,
                    fan_out_failure_policy: None,
                    correlation_id: None,
                    parent_consumption: None,
                    capacity_request: None,
                }],
            })
            .unwrap();
        delegated_session_id(&planner.session_id, wake_id, 0)
    }

    fn fan_out_request(
        index: u32,
        group_id: &str,
        max_concurrency: Option<u32>,
        failure_policy: FanOutFailurePolicy,
    ) -> BrainAction {
        BrainAction::RequestDelegation {
            profile_id: ProfileId::new(format!("coder-profile-{index}")),
            task_id: Some(rusty_crew_core_protocol::TaskId::new(format!(
                "fan-out-{index}"
            ))),
            prompt: format!("complete fan-out slice {index}"),
            expected_output: Some("completion packet".to_string()),
            resource_limits: Some(ResourceLimits {
                workdir: Some("/home/dev/rusty-crew".to_string()),
                max_duration_ms: Some(30_000),
                max_delegation_depth: Some(0),
            }),
            timeout_ms: Some(30_000),
            priority: None,
            fan_out_group_id: Some(group_id.to_string()),
            fan_out_max_concurrency: max_concurrency,
            fan_out_failure_policy: Some(failure_policy),
            correlation_id: Some(format!("{group_id}:{index}")),
            parent_consumption: Some(ParentConsumptionPolicy::AwaitCompletion),
            capacity_request: None,
        }
    }

    fn deliver_child_completion(
        engine: &CoreEngine,
        parent_session_id: &SessionId,
        parent_wake_id: &str,
        child_index: usize,
        status: CompletionStatus,
    ) {
        let child_session_id = delegated_session_id(parent_session_id, parent_wake_id, child_index);
        engine
            .execute_brain_actions(BrainActionBatch {
                wake_id: format!("child-wake-{child_index}"),
                session_id: child_session_id.clone(),
                actions: vec![BrainAction::DeliverCompletion {
                    packet: CompletionPacket {
                        session_id: child_session_id,
                        summary: format!("fan-out child {child_index} {status:?}"),
                        status,
                    },
                }],
            })
            .unwrap();
    }

    fn session_config(
        session_id: &str,
        agent_id: &str,
        profile_id: &str,
        kind: SessionKind,
    ) -> SessionConfig {
        SessionConfig {
            session_id: SessionId::new(session_id),
            agent_id: AgentId::new(agent_id),
            profile_id: ProfileId::new(profile_id),
            kind,
            delegation: None,
            resource_limits: ResourceLimits {
                workdir: Some("/home/dev/rusty-crew".to_string()),
                max_duration_ms: Some(60_000),
                max_delegation_depth: Some(1),
            },
            tool_profile: ToolProfile {
                tools: vec![ToolDescriptor {
                    name: "patch".to_string(),
                    description: "Apply a source patch".to_string(),
                    input_schema: None,
                }],
            },
            history_window: None,
        }
    }

    fn profile_registry_write(
        profile_id: &str,
        provider_alias: &str,
        configured_session_id: &str,
    ) -> ProfileRegistryWrite {
        ProfileRegistryWrite {
            profile_id: ProfileId::new(profile_id),
            lifecycle_status: rusty_crew_core_protocol::ProfileRegistryLifecycleStatus::Active,
            display_name: None,
            summary: None,
            default_session_kind: Some(SessionKind::Full),
            agent_id: Some(AgentId::new(profile_id)),
            owner_id: None,
            prompt_soul_markdown: None,
            prompt_memory_markdown: None,
            active_runtime_settings_json: serde_json::json!({
                "provider_alias": provider_alias,
            }),
            source_asset_refs: vec![],
            derived_runtime_refs: vec![
                rusty_crew_core_protocol::ProfileRegistryDerivedRuntimeRef {
                    ref_kind: "session".to_string(),
                    ref_id: configured_session_id.to_string(),
                    status: "active".to_string(),
                    updated_at: None,
                    metadata_json: serde_json::json!({}),
                },
            ],
            import_export: rusty_crew_core_protocol::ProfileRegistryImportExportMetadata {
                imported_from: None,
                imported_at: None,
                exported_to: None,
                exported_at: None,
                metadata_json: serde_json::json!({}),
            },
            now: "2026-06-19T00:00:00Z".to_string(),
        }
    }
}
