//! Coordination engine composition.

use rusty_crew_core_body::{
    apply_history_window, session_kind_can_wake, BodyProjector, BrainActionExecutor,
    DefaultWakeThreshold, WakeThreshold,
};
use rusty_crew_core_bus::{CoreBus, SequencedEvent};
use rusty_crew_core_persistence::{
    ConversationBranchQuery, ConversationBranchRecord, ConversationBranchStateRecord,
    ConversationBranchWrite, ConversationJumpRequest, ConversationJumpResult,
    ConversationSnapshotQuery, ConversationSnapshotRecord, ConversationSnapshotWrite,
    CoordinationStore, MessageSlotQuery, MessageSlotRecord, MessageSlotWrite, MessageVariantQuery,
    MessageVariantRecord, MessageVariantWrite, ProfileMemoryCaps, ProfileMemoryDelete,
    ProfileMemoryQuery, ProfileMemoryRecord, ProfileMemoryReplace, ProfileMemoryTarget,
    ProfileMemoryWrite, ProviderWireStateInvalidationReason, ProviderWireStateKey,
    ProviderWireStateWakeLookup, ProviderWireStateWrite, QueryPage, QueuedMessageFilter,
    QueuedMessageRecord, QueuedMessageState, RuntimeCounterQuery, RuntimeCounterRecord,
    RuntimeCounterScope, RuntimeDatabaseSize, RuntimeMaintenancePolicy, RuntimeMaintenanceReport,
    RuntimeSearchFilter, RuntimeSearchResult, RuntimeStateSummary, ScheduledJobQuery,
    ScheduledJobRecord, ScheduledJobStatus, ScheduledRunQuery, ScheduledRunRecord,
    ScheduledRunStatus, ScheduledRunTrigger, SelectActiveBranchRequest, SelectActiveBranchResult,
    SelectActiveVariantRequest, SelectActiveVariantResult, UpdateBranchHeadRequest,
    UpdateBranchHeadResult, WorkerRunRecord, WorkerRunStatus,
};
use rusty_crew_core_protocol::{
    ActionBatchReceipt, ActionRejection, AgentId, AgentMessage, BodyState, BrainAction,
    BrainActionBatch, BrainEvent, BrainEventEnvelope, BrainImplementationRegistration,
    BrainProviderStateScope, BrainWakeProviderStateInput, BrainWakeProviderStateOutput,
    BrainWakeProviderStateUpdate, ClockConfig, CompletionStatus, CoreError, CoreErrorKind,
    CoreEvent, CoreResult, DelegatedResourceCleanupReport, DelegatedRunStatus,
    DelegatedSessionRuntimeStatus, DelegationLifecycleEvent, DelegationLifecyclePhase,
    DelegationLineage, DenDataUpdate, EngineConfig, EngineHandle, EventReceipt, EventSubscription,
    ExternalEvent, FanOutFailurePolicy, IsoTimestamp, MessageSlotId, MessageVariantId,
    ParentConsumptionPolicy, ProfileId, ProviderStateAbsenceReason, ProviderStateClearReason,
    ProviderStateMode, ResourceLimits, RunId, SessionConfig, SessionId, SessionKind, SessionState,
    SessionStatus, ShutdownSummary, ToolProfile,
};
use rusty_crew_core_session::SessionRegistry;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use time::format_description::well_known::Rfc3339;
use time::Duration;
use time::OffsetDateTime;

static NEXT_ENGINE_HANDLE: AtomicU64 = AtomicU64::new(1);
static NEXT_SCHEDULED_RUN: AtomicU64 = AtomicU64::new(1);
static NEXT_QUEUED_MESSAGE: AtomicU64 = AtomicU64::new(1);

const SCHEDULED_WAKE_JOB_KIND: &str = "runtime.wake.session";
const SCHEDULER_CLAIM_TTL_MS: u64 = 30_000;
const DEFAULT_PROVIDER_WIRE_STATE_TTL_MS: u64 = 24 * 60 * 60 * 1_000;
const MAX_PROVIDER_WIRE_STATE_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1_000;

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
    store: CoordinationStore,
    body_projector: BodyProjector,
    action_executor: BrainActionExecutor,
    profile_tool_profiles: Arc<Mutex<HashMap<ProfileId, ToolProfile>>>,
    scheduler_tick_lock: Arc<Mutex<()>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SchedulerTickReport {
    pub stale_runs_expired: u32,
    pub due_runs_claimed: u32,
    pub wakes_requested: u32,
    pub runs_completed: u32,
    pub runs_skipped: u32,
    pub runs_failed: u32,
}

#[derive(Debug, Default)]
struct FanOutValidationGroup {
    indexes: Vec<u32>,
    max_concurrency: Option<u32>,
    failure_policy: Option<FanOutFailurePolicy>,
}

impl CoreEngine {
    pub fn initialize(config: EngineConfig) -> CoreResult<Self> {
        let store = CoordinationStore::open(&config.engine_data_dir)?;
        let persisted_sessions = store.load_sessions()?;
        let persisted_events = store
            .load_event_history()?
            .into_iter()
            .map(|entry| SequencedEvent {
                sequence: entry.sequence,
                event: entry.event,
            })
            .collect();
        let recorder_store = store.clone();
        let bus = CoreBus::with_history_and_recorder(
            persisted_events,
            Some(Arc::new(move |sequence, event| {
                recorder_store.save_event(sequence, event)
            })),
        );
        let sessions = SessionRegistry::from_states(persisted_sessions);

        let engine = Self {
            handle: EngineHandle::new(NEXT_ENGINE_HANDLE.fetch_add(1, Ordering::Relaxed)),
            config,
            body_projector: BodyProjector::new(bus.clone(), sessions.clone()),
            action_executor: BrainActionExecutor::new(bus.clone(), sessions.clone()),
            bus,
            sessions,
            store,
            profile_tool_profiles: Arc::new(Mutex::new(HashMap::new())),
            scheduler_tick_lock: Arc::new(Mutex::new(())),
        };
        engine.cleanup_orphaned_delegated_sessions()?;
        engine.expire_delegated_sessions()?;
        Ok(engine)
    }

    pub fn handle(&self) -> EngineHandle {
        self.handle
    }

    pub fn bus(&self) -> &CoreBus {
        &self.bus
    }

    pub fn subscribe_events(
        &self,
        filter: EventSubscription,
    ) -> CoreResult<(u64, Receiver<CoreEvent>)> {
        self.bus.subscribe(filter)
    }

    pub fn unsubscribe_events(&self, id: u64) -> CoreResult<()> {
        self.bus.unsubscribe(id)
    }

    pub fn create_session(&self, config: SessionConfig) -> CoreResult<SessionState> {
        let state = self.sessions.create_session(config.clone(), self.now())?;
        self.store.save_session_with_config(&state, &config)?;
        self.bus.publish(CoreEvent::SessionCreated {
            state: Box::new(state.clone()),
        })?;
        Ok(state)
    }

    pub fn ensure_configured_session(&self, config: SessionConfig) -> CoreResult<SessionState> {
        match self.sessions.get_session(&config.session_id) {
            Ok(existing) => {
                if existing.agent_id != config.agent_id
                    || existing.profile_id != config.profile_id
                    || existing.kind != config.kind
                    || existing.delegation != config.delegation
                {
                    return Err(CoreError::new(
                        CoreErrorKind::AlreadyExists,
                        format!(
                            "session {} already exists with a different configured identity",
                            config.session_id
                        ),
                    ));
                }
                if existing.status == SessionStatus::Archived {
                    let now = self.now();
                    self.store.expire_queued_messages_at(&now)?;
                    self.sessions.apply_config(&config)?;
                    let state = self.sessions.reactivate_session(&config.session_id, now)?;
                    self.store.save_session(&state)?;
                    return Ok(state);
                }
                let state = self.sessions.apply_config(&config)?;
                self.store.save_session(&state)?;
                Ok(state)
            }
            Err(error) if error.kind == CoreErrorKind::NotFound => self.create_session(config),
            Err(error) => Err(error),
        }
    }

    pub fn get_session(&self, session_id: &SessionId) -> CoreResult<SessionState> {
        self.sessions.get_session(session_id)
    }

    pub fn list_sessions(&self) -> CoreResult<Vec<SessionState>> {
        self.sessions.all_sessions()
    }

    pub fn archive_session(&self, session_id: &SessionId) -> CoreResult<SessionState> {
        let state = self.sessions.archive_session(session_id, self.now())?;
        self.store.save_session(&state)?;
        self.bus.publish(CoreEvent::SessionArchived {
            session_id: session_id.clone(),
        })?;
        if state.kind == SessionKind::Delegated {
            if !self
                .store
                .load_worker_run_by_delegated_session(session_id)?
                .as_ref()
                .is_some_and(|run| run.status.is_terminal())
            {
                self.store.update_worker_run_status_by_delegated_session(
                    session_id,
                    WorkerRunStatus::Cancelled,
                    self.now(),
                )?;
            }
        } else {
            self.cancel_delegated_children_for_parent(session_id)?;
        }
        Ok(state)
    }

    pub fn project_body_state(&self, session_id: &SessionId) -> CoreResult<BodyState> {
        let mut state = self.body_projector.project(session_id)?;
        state.child_completions = self.store.delegated_completions_for_parent(session_id)?;
        state.fan_out_groups = self.store.fan_out_groups_for_parent(session_id)?;
        Ok(state)
    }

    pub fn prepare_body_state_for_wake(&self, session_id: &SessionId) -> CoreResult<BodyState> {
        let mut state = self.project_body_state(session_id)?;
        let queued_capacity = state
            .session
            .history_window
            .as_ref()
            .and_then(|window| window.max_messages)
            .map(|max_messages| max_messages.saturating_sub(state.pending_messages.len() as u32));
        let queued = self.drain_body_follow_up_messages_for_wake(session_id, queued_capacity)?;
        state
            .pending_messages
            .extend(queued.into_iter().map(|record| record.message));
        Ok(state)
    }

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
        let loaded = match self.store.load_provider_wire_state_for_wake(&lookup) {
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
    ) -> CoreResult<Vec<rusty_crew_core_persistence::ProviderWireStateDiagnostic>> {
        self.store.list_provider_wire_state_diagnostics(limit)
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
        self.store
            .save_provider_wire_state(&ProviderWireStateWrite {
                key: provider_wire_state_key(session_id, &module_id, &strategy_id),
                profile_fingerprint: state.profile_fingerprint,
                provider_fingerprint: state.provider_fingerprint,
                payload_version: state.payload_version,
                payload_json: state.payload,
                now,
                expires_at: Some(expires_at),
                last_wake_id: Some(wake_id.to_string()),
            })?;
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
        self.store.clear_provider_wire_state(
            &provider_wire_state_key(session_id, &module_id, &strategy_id),
            &self.now(),
            invalidation_reason,
        )?;
        Ok(())
    }

    pub fn enqueue_body_follow_up_message(
        &self,
        session_id: &SessionId,
        from: AgentId,
        body: impl Into<String>,
        correlation_id: Option<String>,
    ) -> CoreResult<QueuedMessageRecord> {
        let session = self.sessions.get_session(session_id)?;
        if !session_kind_can_wake(&session.kind) || session.status == SessionStatus::Archived {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!(
                    "session {} cannot receive follow-up wakes",
                    session.session_id
                ),
            ));
        }
        let state = self.body_projector.project(session_id)?;
        let ttl_ms = state.delta_policy.queued_message_ttl_ms;
        let now = self.now();
        let expires_at = add_millis_to_iso(&now, ttl_ms as u64)?;
        let record = QueuedMessageRecord {
            message_id: next_queued_message_id(session_id),
            owner_session_id: Some(session_id.clone()),
            owner_agent_id: session.agent_id.clone(),
            message: AgentMessage {
                from,
                to: session.agent_id.clone(),
                body: body.into(),
                correlation_id,
            },
            source_sequence: None,
            enqueued_at: now.clone(),
            expires_at,
            ttl_ms,
            delivery_attempts: 0,
            state: QueuedMessageState::Pending,
            terminal_at: None,
            state_reason: None,
        };
        self.store.save_queued_message(&record)?;
        self.enforce_body_follow_up_cap(session_id, state.delta_policy.max_queued_messages)?;
        self.bus.publish(CoreEvent::BrainWakeRequested {
            session_id: session_id.clone(),
        })?;
        Ok(record)
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
            self.store.update_worker_run_status_by_delegated_session(
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
        self.store.count_rows(table)
    }

    pub fn database_size(&self) -> CoreResult<RuntimeDatabaseSize> {
        self.store.database_size()
    }

    pub fn run_maintenance(
        &self,
        policy: &RuntimeMaintenancePolicy,
    ) -> CoreResult<RuntimeMaintenanceReport> {
        self.store.run_maintenance(policy)
    }

    pub fn save_message_slot(&self, slot: &MessageSlotWrite) -> CoreResult<()> {
        self.store.save_message_slot(slot)
    }

    pub fn save_message_variant(
        &self,
        variant: &MessageVariantWrite,
    ) -> CoreResult<MessageVariantRecord> {
        self.store.save_message_variant(variant)
    }

    pub fn query_message_slots(
        &self,
        query: &MessageSlotQuery,
    ) -> CoreResult<Vec<MessageSlotRecord>> {
        self.store.query_message_slots(query)
    }

    pub fn query_message_variants(
        &self,
        query: &MessageVariantQuery,
    ) -> CoreResult<Vec<MessageVariantRecord>> {
        self.store.query_message_variants(query)
    }

    pub fn save_conversation_branch(
        &self,
        branch: &ConversationBranchWrite,
    ) -> CoreResult<ConversationBranchRecord> {
        self.store.save_conversation_branch(branch)
    }

    pub fn query_conversation_branches(
        &self,
        query: &ConversationBranchQuery,
    ) -> CoreResult<Vec<ConversationBranchRecord>> {
        self.store.query_conversation_branches(query)
    }

    pub fn get_conversation_branch_state(
        &self,
        session_id: &SessionId,
        default_updated_at: &IsoTimestamp,
    ) -> CoreResult<ConversationBranchStateRecord> {
        self.store
            .get_conversation_branch_state(session_id, default_updated_at)
    }

    pub fn select_active_conversation_branch(
        &self,
        request: &SelectActiveBranchRequest,
    ) -> CoreResult<SelectActiveBranchResult> {
        self.store.select_active_conversation_branch(request)
    }

    pub fn update_conversation_branch_head(
        &self,
        request: &UpdateBranchHeadRequest,
    ) -> CoreResult<UpdateBranchHeadResult> {
        self.store.update_conversation_branch_head(request)
    }

    pub fn save_conversation_snapshot(
        &self,
        snapshot: &ConversationSnapshotWrite,
    ) -> CoreResult<ConversationSnapshotRecord> {
        self.store.save_conversation_snapshot(snapshot)
    }

    pub fn query_conversation_snapshots(
        &self,
        query: &ConversationSnapshotQuery,
    ) -> CoreResult<Vec<ConversationSnapshotRecord>> {
        self.store.query_conversation_snapshots(query)
    }

    pub fn resolve_conversation_jump(
        &self,
        request: &ConversationJumpRequest,
    ) -> CoreResult<ConversationJumpResult> {
        self.store.resolve_conversation_jump(request)
    }

    pub fn select_active_message_variant(
        &self,
        request: &SelectActiveVariantRequest,
    ) -> CoreResult<SelectActiveVariantResult> {
        self.store.select_active_message_variant(request)
    }

    pub fn delete_message_variant(
        &self,
        slot_id: &MessageSlotId,
        variant_id: &MessageVariantId,
        updated_at: &IsoTimestamp,
    ) -> CoreResult<MessageSlotRecord> {
        self.store
            .delete_message_variant(slot_id, variant_id, updated_at)
    }

    pub fn reorder_message_variants(
        &self,
        slot_id: &MessageSlotId,
        ordered_variant_ids: &[MessageVariantId],
        updated_at: &IsoTimestamp,
    ) -> CoreResult<Vec<MessageVariantRecord>> {
        self.store
            .reorder_message_variants(slot_id, ordered_variant_ids, updated_at)
    }

    pub fn list_profile_memory(
        &self,
        query: &ProfileMemoryQuery,
    ) -> CoreResult<Vec<ProfileMemoryRecord>> {
        self.store.list_profile_memory(query)
    }

    pub fn get_profile_memory(
        &self,
        profile_id: &ProfileId,
        target: &ProfileMemoryTarget,
        key: &str,
    ) -> CoreResult<Option<ProfileMemoryRecord>> {
        self.store.get_profile_memory(profile_id, target, key)
    }

    pub fn add_profile_memory(
        &self,
        mut write: ProfileMemoryWrite,
        caps: &ProfileMemoryCaps,
    ) -> CoreResult<ProfileMemoryRecord> {
        write.now = self.now();
        self.store.add_profile_memory(&write, caps)
    }

    pub fn replace_profile_memory(
        &self,
        mut replace: ProfileMemoryReplace,
        caps: &ProfileMemoryCaps,
    ) -> CoreResult<ProfileMemoryRecord> {
        replace.write.now = self.now();
        self.store.replace_profile_memory(&replace, caps)
    }

    pub fn remove_profile_memory(
        &self,
        delete: &ProfileMemoryDelete,
    ) -> CoreResult<ProfileMemoryRecord> {
        self.store.remove_profile_memory(delete)
    }

    pub fn search_runtime(
        &self,
        filter: &RuntimeSearchFilter,
    ) -> CoreResult<Vec<RuntimeSearchResult>> {
        self.store.search_runtime(filter)
    }

    pub fn query_runtime_counters(
        &self,
        query: &RuntimeCounterQuery,
    ) -> CoreResult<Vec<RuntimeCounterRecord>> {
        self.store.query_runtime_counters(query)
    }

    pub fn runtime_summary(&self, scope: &RuntimeCounterScope) -> CoreResult<RuntimeStateSummary> {
        self.store.runtime_summary(scope)
    }

    pub fn reset_runtime_counters(&self, query: &RuntimeCounterQuery) -> CoreResult<u64> {
        self.store.reset_runtime_counters(query, self.now())
    }

    pub fn register_scheduled_wake_job(
        &self,
        job_id: impl Into<String>,
        target_session_id: SessionId,
        interval_ms: Option<u64>,
        first_due_at: IsoTimestamp,
    ) -> CoreResult<ScheduledJobRecord> {
        let session = self.sessions.get_session(&target_session_id)?;
        if !session_kind_can_wake(&session.kind) {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!(
                    "session {} cannot be woken by scheduler",
                    session.session_id
                ),
            ));
        }
        let now = self.now();
        let record = ScheduledJobRecord {
            job_id: job_id.into(),
            job_kind: SCHEDULED_WAKE_JOB_KIND.to_string(),
            target_session_id: Some(target_session_id),
            interval_ms,
            next_due_at: Some(first_due_at),
            payload_json: serde_json::json!({}),
            status: ScheduledJobStatus::Active,
            created_at: now.clone(),
            updated_at: now,
            paused_at: None,
        };
        self.store.upsert_scheduled_job(&record)?;
        Ok(record)
    }

    pub fn register_scheduled_host_job(
        &self,
        job_id: impl Into<String>,
        job_kind: impl Into<String>,
        interval_ms: Option<u64>,
        first_due_at: IsoTimestamp,
        payload_json: serde_json::Value,
    ) -> CoreResult<ScheduledJobRecord> {
        let job_kind = job_kind.into();
        if job_kind.trim().is_empty() || job_kind == SCHEDULED_WAKE_JOB_KIND {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "scheduled host job requires a non-wake job kind",
            ));
        }
        let now = self.now();
        let record = ScheduledJobRecord {
            job_id: job_id.into(),
            job_kind,
            target_session_id: None,
            interval_ms,
            next_due_at: Some(first_due_at),
            payload_json,
            status: ScheduledJobStatus::Active,
            created_at: now.clone(),
            updated_at: now,
            paused_at: None,
        };
        self.store.upsert_scheduled_job(&record)?;
        Ok(record)
    }

    pub fn list_scheduled_jobs(
        &self,
        status: Option<ScheduledJobStatus>,
        job_kind: Option<String>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> CoreResult<Vec<ScheduledJobRecord>> {
        self.store.query_scheduled_jobs(&ScheduledJobQuery {
            status,
            job_kind,
            page: Some(QueryPage { limit, offset }),
            ..ScheduledJobQuery::default()
        })
    }

    pub fn list_scheduled_runs(
        &self,
        job_id: Option<String>,
        status: Option<ScheduledRunStatus>,
        trigger: Option<ScheduledRunTrigger>,
        target_session_id: Option<SessionId>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> CoreResult<Vec<ScheduledRunRecord>> {
        self.store.query_scheduled_runs(&ScheduledRunQuery {
            job_id,
            status,
            trigger,
            target_session_id,
            page: Some(QueryPage { limit, offset }),
            ..ScheduledRunQuery::default()
        })
    }

    pub fn claim_scheduled_host_runs(
        &self,
        supported_job_kinds: Vec<String>,
        limit: Option<u32>,
    ) -> CoreResult<Vec<ScheduledRunRecord>> {
        let _guard = self.scheduler_tick_lock.lock().map_err(|_| {
            CoreError::new(CoreErrorKind::InternalError, "scheduler tick lock poisoned")
        })?;
        let supported_job_kinds = normalized_supported_host_job_kinds(supported_job_kinds)?;
        let now = self.now();
        self.store.expire_stale_scheduled_runs(&now, &now)?;
        let mut claimed = Vec::new();
        let max_claims = limit.unwrap_or(10).clamp(1, 100);
        for job_kind in supported_job_kinds {
            if claimed.len() >= max_claims as usize {
                break;
            }
            let remaining = max_claims.saturating_sub(claimed.len() as u32);
            let due_jobs = self.store.query_scheduled_jobs(&ScheduledJobQuery {
                status: Some(ScheduledJobStatus::Active),
                job_kind: Some(job_kind),
                due_at_or_before: Some(now.clone()),
                page: Some(QueryPage {
                    limit: Some(remaining),
                    offset: None,
                }),
            })?;
            for job in due_jobs {
                claimed.push(self.claim_scheduled_run(
                    &job,
                    ScheduledRunTrigger::Due,
                    job.next_due_at.clone(),
                )?);
            }
        }
        Ok(claimed)
    }

    pub fn request_scheduled_host_job_run(
        &self,
        job_id: &str,
        supported_job_kinds: Vec<String>,
    ) -> CoreResult<Option<ScheduledRunRecord>> {
        let supported_job_kinds = normalized_supported_host_job_kinds(supported_job_kinds)?;
        let Some(job) = self.store.load_scheduled_job(job_id)? else {
            return Ok(None);
        };
        if job.status == ScheduledJobStatus::Archived {
            return Ok(None);
        }
        if !supported_job_kinds.contains(&job.job_kind) {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("scheduled job kind {} is not host-supported", job.job_kind),
            ));
        }
        self.claim_scheduled_run(&job, ScheduledRunTrigger::Manual, None)
            .map(Some)
    }

    pub fn complete_scheduled_host_run(
        &self,
        run_id: &RunId,
        status: ScheduledRunStatus,
        output_json: serde_json::Value,
        error: Option<String>,
    ) -> CoreResult<()> {
        if !matches!(
            status,
            ScheduledRunStatus::Completed
                | ScheduledRunStatus::Skipped
                | ScheduledRunStatus::Failed
                | ScheduledRunStatus::Cancelled
        ) {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "scheduled host run completion requires a terminal host status",
            ));
        }
        self.store.complete_scheduled_run(
            run_id,
            status,
            &self.now(),
            &output_json,
            error.as_deref(),
        )
    }

    pub fn pause_scheduled_job(&self, job_id: &str) -> CoreResult<()> {
        self.store.pause_scheduled_job(job_id, &self.now())
    }

    pub fn resume_scheduled_job(&self, job_id: &str, next_due_at: IsoTimestamp) -> CoreResult<()> {
        self.store
            .resume_scheduled_job(job_id, &next_due_at, &self.now())
    }

    pub fn request_scheduled_job_run(
        &self,
        job_id: &str,
    ) -> CoreResult<Option<ScheduledRunRecord>> {
        let Some(job) = self.store.load_scheduled_job(job_id)? else {
            return Ok(None);
        };
        if job.status == ScheduledJobStatus::Archived {
            return Ok(None);
        }
        let run = self.claim_scheduled_run(&job, ScheduledRunTrigger::Manual, None)?;
        self.finish_scheduler_run(run)
    }

    pub fn run_scheduler_tick(&self) -> CoreResult<SchedulerTickReport> {
        let _guard = self.scheduler_tick_lock.lock().map_err(|_| {
            CoreError::new(CoreErrorKind::InternalError, "scheduler tick lock poisoned")
        })?;
        let now = self.now();
        let stale_runs = self.store.expire_stale_scheduled_runs(&now, &now)?;
        let due_jobs = self.store.query_scheduled_jobs(&ScheduledJobQuery {
            status: Some(ScheduledJobStatus::Active),
            job_kind: Some(SCHEDULED_WAKE_JOB_KIND.to_string()),
            due_at_or_before: Some(now.clone()),
            page: None,
        })?;
        let mut report = SchedulerTickReport {
            stale_runs_expired: stale_runs.len() as u32,
            ..SchedulerTickReport::default()
        };
        for job in due_jobs {
            let run =
                self.claim_scheduled_run(&job, ScheduledRunTrigger::Due, job.next_due_at.clone())?;
            report.due_runs_claimed += 1;
            if let Some(run) = self.finish_scheduler_run(run)? {
                match run.status {
                    ScheduledRunStatus::Completed => {
                        report.runs_completed += 1;
                        report.wakes_requested += 1;
                    }
                    ScheduledRunStatus::Skipped => report.runs_skipped += 1,
                    ScheduledRunStatus::Failed => report.runs_failed += 1,
                    _ => {}
                }
            }
        }
        Ok(report)
    }

    pub fn request_delegated_checkpoint(
        &self,
        parent_session_id: &SessionId,
        delegated_session_id: &SessionId,
        reason: impl Into<String>,
    ) -> CoreResult<EventReceipt> {
        let parent = self.sessions.get_session(parent_session_id)?;
        let delegated = self.sessions.get_session(delegated_session_id)?;
        if delegated.status == SessionStatus::Archived {
            return Err(CoreError::new(
                CoreErrorKind::SessionExpired,
                format!("delegated session {} is archived", delegated.session_id),
            ));
        }
        let lineage = delegated.delegation.as_ref().ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("session {} is not delegated", delegated.session_id),
            )
        })?;
        if &lineage.parent_session_id != parent_session_id {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!(
                    "delegated session {} does not belong to parent {}",
                    delegated.session_id, parent_session_id
                ),
            ));
        }

        let receipt = self.route_agent_message(AgentMessage {
            from: parent.agent_id,
            to: delegated.agent_id.clone(),
            body: format!("Checkpoint requested: {}", reason.into()),
            correlation_id: Some(format!("checkpoint:{}", delegated.session_id)),
        })?;
        self.store.update_worker_run_status_by_delegated_session(
            &delegated.session_id,
            WorkerRunStatus::CheckpointWaiting,
            self.now(),
        )?;
        self.publish_delegation_lifecycle(
            &delegated,
            Some(lineage.source_wake_id.as_str()),
            lineage.source_action_index,
            DelegationLifecyclePhase::CheckpointRequested,
            None,
        )?;
        Ok(receipt)
    }

    pub fn cancel_delegated_session(
        &self,
        delegated_session_id: &SessionId,
    ) -> CoreResult<SessionState> {
        let delegated = self.sessions.get_session(delegated_session_id)?;
        if delegated.kind != SessionKind::Delegated {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("session {} is not delegated", delegated.session_id),
            ));
        }
        self.archive_delegated_session_if_nonterminal(&delegated, WorkerRunStatus::Cancelled)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::SessionExpired,
                    format!(
                        "delegated session {} is already terminal",
                        delegated.session_id
                    ),
                )
            })
    }

    pub fn drain_delegated_sessions(
        &self,
        parent_session_id: Option<&SessionId>,
    ) -> CoreResult<Vec<SessionId>> {
        let sessions = match parent_session_id {
            Some(parent_session_id) => self
                .sessions
                .delegated_sessions_for_parent(parent_session_id)?,
            None => self.sessions.all_sessions()?,
        };
        let mut drained = Vec::new();
        for session in sessions {
            if session.kind != SessionKind::Delegated || session.status == SessionStatus::Archived {
                continue;
            }
            if let Some(archived) =
                self.archive_delegated_session_if_nonterminal(&session, WorkerRunStatus::Cancelled)?
            {
                drained.push(archived.session_id);
            }
        }
        Ok(drained)
    }

    pub fn delegated_session_status(
        &self,
        delegated_session_id: &SessionId,
    ) -> CoreResult<DelegatedSessionRuntimeStatus> {
        let session = self.sessions.get_session(delegated_session_id)?;
        if session.kind != SessionKind::Delegated {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("session {} is not delegated", session.session_id),
            ));
        }
        let run = self
            .store
            .load_worker_run_by_delegated_session(delegated_session_id)?;
        Ok(DelegatedSessionRuntimeStatus {
            parent_session_id: session
                .delegation
                .as_ref()
                .map(|lineage| lineage.parent_session_id.clone()),
            session,
            run_id: run.as_ref().map(|run| run.run_id.clone()),
            run_status: run.as_ref().map(|run| delegated_run_status(run.status)),
            terminal: run.as_ref().is_some_and(|run| run.status.is_terminal()),
        })
    }

    pub fn expire_delegated_sessions(&self) -> CoreResult<Vec<SessionId>> {
        self.expire_delegated_sessions_at(self.now())
    }

    pub fn cleanup_delegated_resources(&self) -> CoreResult<DelegatedResourceCleanupReport> {
        let cleaned_at = self.now();
        let terminal_archived = self.archive_terminal_delegated_sessions()?;
        let orphaned_archived = self.cleanup_orphaned_delegated_sessions()?;
        let expired_archived = self.expire_delegated_sessions_at(cleaned_at.clone())?;
        Ok(DelegatedResourceCleanupReport {
            cleaned_at,
            resources_released: 0,
            terminal_archived,
            orphaned_archived,
            expired_archived,
        })
    }

    pub fn expire_delegated_sessions_at(&self, now: IsoTimestamp) -> CoreResult<Vec<SessionId>> {
        let now_time = parse_rfc3339(&now)?;
        let mut expired = Vec::new();
        for session in self.sessions.all_sessions()? {
            if session.kind != SessionKind::Delegated || session.status == SessionStatus::Archived {
                continue;
            }
            let Some(max_duration_ms) = session.resource_limits.max_duration_ms else {
                continue;
            };
            let created_at = parse_rfc3339(&session.created_at)?;
            if now_time - created_at < Duration::milliseconds(max_duration_ms.into()) {
                continue;
            }
            if let Some(archived) =
                self.archive_delegated_session_if_nonterminal(&session, WorkerRunStatus::Expired)?
            {
                expired.push(archived.session_id);
            }
        }
        Ok(expired)
    }

    pub fn delegated_sessions_for_parent(
        &self,
        parent_session_id: &SessionId,
    ) -> CoreResult<Vec<SessionState>> {
        self.sessions
            .delegated_sessions_for_parent(parent_session_id)
    }

    pub fn delegated_session_for_run(&self, run_id: &RunId) -> CoreResult<Option<SessionState>> {
        let Some(run) = self.store.load_worker_run(run_id)? else {
            return Ok(None);
        };
        if let Some(session_id) = run.delegated_session_id {
            return match self.sessions.get_session(&session_id) {
                Ok(session) => Ok(Some(session)),
                Err(error) if error.kind == CoreErrorKind::NotFound => Ok(None),
                Err(error) => Err(error),
            };
        }

        self.sessions.delegated_session_for_source(
            &run.parent_session_id,
            &run.source_wake_id,
            run.source_action_index,
        )
    }

    pub fn shutdown(self) -> CoreResult<ShutdownSummary> {
        self.shutdown_with_timeout(0)
    }

    pub fn shutdown_with_timeout(self, drain_timeout_ms: u32) -> CoreResult<ShutdownSummary> {
        let active_sessions = self
            .sessions
            .all_sessions()?
            .into_iter()
            .filter(|session| session.status != SessionStatus::Archived)
            .collect::<Vec<_>>();
        let archived_sessions = active_sessions.len() as u32;
        for session in active_sessions {
            if self.sessions.get_session(&session.session_id)?.status != SessionStatus::Archived {
                self.archive_session(&session.session_id)?;
            }
        }
        // Shutdown is currently synchronous: session archive events are
        // published before subscriber senders are dropped, so there is no
        // async work to wait for. The timeout becomes meaningful once the
        // engine owns background tasks that require bounded joins.
        let _ = drain_timeout_ms;
        let dropped_subscriptions = self.bus.shutdown_subscribers()?;
        Ok(ShutdownSummary {
            engine: self.handle,
            archived_sessions,
            dropped_subscriptions,
        })
    }

    pub fn diagnostic_now(&self) -> IsoTimestamp {
        self.now()
    }

    fn now(&self) -> IsoTimestamp {
        match &self.config.clock {
            ClockConfig::System => OffsetDateTime::now_utc()
                .format(&Rfc3339)
                .expect("formatting current UTC timestamp as RFC3339 should not fail"),
            ClockConfig::Fixed { at } => at.clone(),
        }
    }

    fn drain_body_follow_up_messages_for_wake(
        &self,
        session_id: &SessionId,
        max_delivered_messages: Option<u32>,
    ) -> CoreResult<Vec<QueuedMessageRecord>> {
        let now = self.now();
        self.store.expire_queued_messages_at(&now)?;
        let pending = self.store.load_queued_messages(&QueuedMessageFilter {
            state: Some(QueuedMessageState::Pending),
            owner_session_id: Some(session_id.clone()),
            owner_agent_id: None,
            limit: None,
        })?;
        let delivered_ids = apply_history_window(
            pending
                .iter()
                .map(|record| record.message_id.clone())
                .collect::<Vec<_>>(),
            max_delivered_messages,
        )
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
        let mut delivered = Vec::new();
        for mut record in pending {
            let include_in_wake = delivered_ids.contains(&record.message_id);
            record.state = if include_in_wake {
                QueuedMessageState::Delivered
            } else {
                QueuedMessageState::Discarded
            };
            record.delivery_attempts += 1;
            record.terminal_at = Some(now.clone());
            record.state_reason = Some(if include_in_wake {
                "delivered_for_wake".to_string()
            } else {
                "history_window_exceeded".to_string()
            });
            self.store.save_queued_message(&record)?;
            if include_in_wake {
                delivered.push(record);
            }
        }
        Ok(delivered)
    }

    fn enforce_body_follow_up_cap(
        &self,
        session_id: &SessionId,
        max_queued_messages: u32,
    ) -> CoreResult<()> {
        let pending = self.store.load_queued_messages(&QueuedMessageFilter {
            state: Some(QueuedMessageState::Pending),
            owner_session_id: Some(session_id.clone()),
            owner_agent_id: None,
            limit: None,
        })?;
        let overflow = pending.len().saturating_sub(max_queued_messages as usize);
        if overflow == 0 {
            return Ok(());
        }
        let now = self.now();
        for mut record in pending.into_iter().take(overflow) {
            record.state = QueuedMessageState::Discarded;
            record.terminal_at = Some(now.clone());
            record.state_reason = Some("queue_cap_exceeded".to_string());
            self.store.save_queued_message(&record)?;
        }
        Ok(())
    }

    fn claim_scheduled_run(
        &self,
        job: &ScheduledJobRecord,
        trigger: ScheduledRunTrigger,
        scheduled_for: Option<IsoTimestamp>,
    ) -> CoreResult<ScheduledRunRecord> {
        let now = self.now();
        let claim_deadline_at = add_millis_to_iso(&now, SCHEDULER_CLAIM_TTL_MS)?;
        let run = ScheduledRunRecord {
            run_id: next_scheduled_run_id(&job.job_id),
            job_id: job.job_id.clone(),
            job_kind: job.job_kind.clone(),
            target_session_id: job.target_session_id.clone(),
            status: ScheduledRunStatus::Claimed,
            trigger,
            scheduled_for,
            claimed_at: now.clone(),
            claim_deadline_at,
            completed_at: None,
            error: None,
            output_json: serde_json::json!({}),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        let next_due_at = if trigger == ScheduledRunTrigger::Due {
            job.interval_ms
                .map(|interval_ms| add_millis_to_iso(&now, interval_ms))
                .transpose()?
        } else {
            None
        };
        self.store.claim_scheduled_run(&run, next_due_at.as_ref())?;
        Ok(run)
    }

    fn finish_scheduler_run(
        &self,
        mut run: ScheduledRunRecord,
    ) -> CoreResult<Option<ScheduledRunRecord>> {
        if run.job_kind != SCHEDULED_WAKE_JOB_KIND {
            let now = self.now();
            run.status = ScheduledRunStatus::Skipped;
            run.completed_at = Some(now.clone());
            run.updated_at = now.clone();
            run.error = Some(format!("unsupported scheduled job kind {}", run.job_kind));
            run.output_json = serde_json::json!({ "wake_requested": false });
            self.store.complete_scheduled_run(
                &run.run_id,
                run.status,
                &now,
                &run.output_json,
                run.error.as_deref(),
            )?;
            return Ok(Some(run));
        }
        let Some(session_id) = &run.target_session_id else {
            let now = self.now();
            run.status = ScheduledRunStatus::Failed;
            run.completed_at = Some(now.clone());
            run.updated_at = now.clone();
            run.error = Some("scheduled wake job has no target session".to_string());
            run.output_json = serde_json::json!({ "wake_requested": false });
            self.store.complete_scheduled_run(
                &run.run_id,
                run.status,
                &now,
                &run.output_json,
                run.error.as_deref(),
            )?;
            return Ok(Some(run));
        };
        let session = match self.sessions.get_session(session_id) {
            Ok(session) => session,
            Err(error) if error.kind == CoreErrorKind::NotFound => {
                let now = self.now();
                run.status = ScheduledRunStatus::Skipped;
                run.completed_at = Some(now.clone());
                run.updated_at = now.clone();
                run.error = Some(format!("target session {session_id} not found"));
                run.output_json = serde_json::json!({ "wake_requested": false });
                self.store.complete_scheduled_run(
                    &run.run_id,
                    run.status,
                    &now,
                    &run.output_json,
                    run.error.as_deref(),
                )?;
                return Ok(Some(run));
            }
            Err(error) => return Err(error),
        };
        let now = self.now();
        if session.status == SessionStatus::Archived || !session_kind_can_wake(&session.kind) {
            run.status = ScheduledRunStatus::Skipped;
            run.completed_at = Some(now.clone());
            run.updated_at = now.clone();
            run.error = Some(format!(
                "target session {} is not wakeable",
                session.session_id
            ));
            run.output_json = serde_json::json!({ "wake_requested": false });
            self.store.complete_scheduled_run(
                &run.run_id,
                run.status,
                &now,
                &run.output_json,
                run.error.as_deref(),
            )?;
            return Ok(Some(run));
        }
        self.bus.publish(CoreEvent::BrainWakeRequested {
            session_id: session.session_id.clone(),
        })?;
        run.status = ScheduledRunStatus::Completed;
        run.completed_at = Some(now.clone());
        run.updated_at = now.clone();
        run.output_json = serde_json::json!({
            "wake_requested": true,
            "session_id": session.session_id.0,
        });
        self.store
            .complete_scheduled_run(&run.run_id, run.status, &now, &run.output_json, None)?;
        Ok(Some(run))
    }

    fn spawn_delegated_workers(
        &self,
        parent: &SessionState,
        batch: &BrainActionBatch,
    ) -> CoreResult<()> {
        for (index, action) in batch.actions.iter().enumerate() {
            let BrainAction::RequestDelegation {
                profile_id,
                task_id,
                prompt,
                resource_limits,
                correlation_id,
                parent_consumption,
                fan_out_group_id,
                fan_out_max_concurrency,
                fan_out_failure_policy,
                ..
            } = action
            else {
                continue;
            };

            let run_id = RunId::new(format!("{}:{index}", batch.wake_id));
            if self.store.load_worker_run(&run_id)?.is_some() {
                continue;
            }

            let session_id = delegated_session_id(&batch.session_id, &batch.wake_id, index);
            let agent_id = delegated_agent_id(&session_id);
            let correlation_id = correlation_id
                .clone()
                .unwrap_or_else(|| format!("delegation:{}:{index}", batch.wake_id));
            let lineage = DelegationLineage {
                parent_session_id: parent.session_id.clone(),
                parent_agent_id: parent.agent_id.clone(),
                source_wake_id: batch.wake_id.clone(),
                source_action_index: index as u32,
                requested_task_id: task_id.clone(),
                correlation_id: correlation_id.clone(),
            };
            let config = SessionConfig {
                session_id: session_id.clone(),
                agent_id: agent_id.clone(),
                profile_id: profile_id.clone(),
                kind: SessionKind::Delegated,
                delegation: Some(lineage.clone()),
                resource_limits: resource_limits.clone().unwrap_or(ResourceLimits {
                    workdir: None,
                    max_duration_ms: None,
                    max_delegation_depth: Some(0),
                }),
                tool_profile: self.tool_profile_for_profile(profile_id)?,
                history_window: parent.history_window.clone(),
            };
            let state = self.sessions.create_session(config.clone(), self.now())?;
            self.store.save_worker_run_requested(&WorkerRunRecord {
                run_id,
                parent_session_id: parent.session_id.clone(),
                delegated_session_id: Some(state.session_id.clone()),
                parent_agent_id: Some(parent.agent_id.clone()),
                profile_id: profile_id.clone(),
                task_id: task_id.clone(),
                status: WorkerRunStatus::Requested,
                created_at: state.created_at.clone(),
                last_updated_at: state.created_at.clone(),
                source_wake_id: batch.wake_id.clone(),
                source_action_index: index as u32,
                delegation_correlation_id: Some(correlation_id.clone()),
                parent_consumption: parent_consumption
                    .clone()
                    .unwrap_or(ParentConsumptionPolicy::AwaitCompletion),
                fan_out_group_id: fan_out_group_id.clone(),
                fan_out_max_concurrency: *fan_out_max_concurrency,
                fan_out_failure_policy: fan_out_failure_policy
                    .clone()
                    .unwrap_or(FanOutFailurePolicy::FailSoft),
            })?;
            self.store.save_session_with_config(&state, &config)?;
            self.store.update_worker_run_status_by_delegated_session(
                &state.session_id,
                WorkerRunStatus::SessionCreated,
                self.now(),
            )?;
            self.bus.publish(CoreEvent::SessionCreated {
                state: Box::new(state.clone()),
            })?;
            self.publish_delegation_lifecycle(
                &state,
                Some(&batch.wake_id),
                index as u32,
                DelegationLifecyclePhase::Created,
                None,
            )?;
            self.bus.publish(CoreEvent::AgentMessageRouted {
                message: AgentMessage {
                    from: parent.agent_id.clone(),
                    to: agent_id,
                    body: prompt.clone(),
                    correlation_id: Some(correlation_id),
                },
            })?;
            if session_kind_can_wake(&state.kind) {
                self.bus.publish(CoreEvent::BrainWakeRequested {
                    session_id: state.session_id.clone(),
                })?;
                self.store.update_worker_run_status_by_delegated_session(
                    &state.session_id,
                    WorkerRunStatus::WakeRequested,
                    self.now(),
                )?;
                self.publish_delegation_lifecycle(
                    &state,
                    Some(&batch.wake_id),
                    index as u32,
                    DelegationLifecyclePhase::WakeRequested,
                    None,
                )?;
            }
        }

        Ok(())
    }

    fn cancel_delegated_children_for_parent(
        &self,
        parent_session_id: &SessionId,
    ) -> CoreResult<()> {
        for child in self
            .sessions
            .delegated_sessions_for_parent(parent_session_id)?
        {
            let _ =
                self.archive_delegated_session_if_nonterminal(&child, WorkerRunStatus::Cancelled)?;
        }
        Ok(())
    }

    fn cleanup_orphaned_delegated_sessions(&self) -> CoreResult<Vec<SessionId>> {
        let mut cleaned = Vec::new();
        for session in self.sessions.all_sessions()? {
            if session.kind != SessionKind::Delegated || session.status == SessionStatus::Archived {
                continue;
            }
            let Some(lineage) = &session.delegation else {
                if let Some(archived) = self
                    .archive_delegated_session_if_nonterminal(&session, WorkerRunStatus::Expired)?
                {
                    cleaned.push(archived.session_id);
                }
                continue;
            };
            let parent = self.sessions.get_session(&lineage.parent_session_id);
            match parent {
                Ok(parent) if parent.status != SessionStatus::Archived => {}
                Ok(_) => {
                    if let Some(archived) = self.archive_delegated_session_if_nonterminal(
                        &session,
                        WorkerRunStatus::Cancelled,
                    )? {
                        cleaned.push(archived.session_id);
                    }
                }
                Err(error) if error.kind == CoreErrorKind::NotFound => {
                    if let Some(archived) = self.archive_delegated_session_if_nonterminal(
                        &session,
                        WorkerRunStatus::Expired,
                    )? {
                        cleaned.push(archived.session_id);
                    }
                }
                Err(error) => return Err(error),
            }
        }
        Ok(cleaned)
    }

    fn archive_terminal_delegated_sessions(&self) -> CoreResult<Vec<SessionId>> {
        let mut archived = Vec::new();
        for session in self.sessions.all_sessions()? {
            if session.kind != SessionKind::Delegated || session.status == SessionStatus::Archived {
                continue;
            }
            let Some(run) = self
                .store
                .load_worker_run_by_delegated_session(&session.session_id)?
            else {
                continue;
            };
            if !run.status.is_terminal() {
                continue;
            }
            let archived_session = self
                .sessions
                .archive_session(&session.session_id, self.now())?;
            self.store.save_session(&archived_session)?;
            self.bus.publish(CoreEvent::SessionArchived {
                session_id: archived_session.session_id.clone(),
            })?;
            self.publish_delegation_lifecycle(
                &archived_session,
                Some(&run.source_wake_id),
                run.source_action_index,
                delegation_phase_for_worker_status(run.status),
                Some("cleanup archived terminal delegated session".to_string()),
            )?;
            archived.push(archived_session.session_id);
        }
        Ok(archived)
    }

    fn archive_delegated_session_if_nonterminal(
        &self,
        session: &SessionState,
        status: WorkerRunStatus,
    ) -> CoreResult<Option<SessionState>> {
        if session.kind != SessionKind::Delegated {
            return Ok(None);
        }
        let run = self
            .store
            .load_worker_run_by_delegated_session(&session.session_id)?;
        if run.as_ref().is_some_and(|run| run.status.is_terminal()) {
            return Ok(None);
        }
        let archived = self
            .sessions
            .archive_session(&session.session_id, self.now())?;
        self.store.save_session(&archived)?;
        if let Some(run) = &run {
            self.store
                .update_worker_run_status(&run.run_id, status, self.now())?;
        }
        self.bus.publish(CoreEvent::SessionArchived {
            session_id: archived.session_id.clone(),
        })?;
        self.publish_delegation_lifecycle(
            &archived,
            run.as_ref().map(|run| run.source_wake_id.as_str()),
            run.as_ref().map_or(0, |run| run.source_action_index),
            delegation_phase_for_worker_status(status),
            None,
        )?;
        Ok(Some(archived))
    }

    fn tool_profile_for_profile(&self, profile_id: &ProfileId) -> CoreResult<ToolProfile> {
        Ok(self
            .profile_tool_profiles
            .lock()
            .map_err(|_| {
                CoreError::new(
                    CoreErrorKind::InternalError,
                    "profile registry lock poisoned",
                )
            })?
            .get(profile_id)
            .cloned()
            .unwrap_or(ToolProfile { tools: Vec::new() }))
    }

    fn validate_delegation_invariants(
        &self,
        session: &SessionState,
        batch: &BrainActionBatch,
    ) -> Vec<ActionRejection> {
        batch
            .actions
            .iter()
            .enumerate()
            .filter_map(|(index, action)| {
                let BrainAction::RequestDelegation { .. } = action else {
                    return None;
                };
                match session.resource_limits.max_delegation_depth {
                    Some(0) => Some(ActionRejection {
                        index: index as u32,
                        kind: CoreErrorKind::ActionRejected,
                        message: "request_delegation exceeds max_delegation_depth".to_string(),
                    }),
                    _ => None,
                }
            })
            .collect()
    }

    fn validate_fan_out_invariants(&self, batch: &BrainActionBatch) -> Vec<ActionRejection> {
        let mut groups: HashMap<String, FanOutValidationGroup> = HashMap::new();
        let mut rejections = Vec::new();
        for (index, action) in batch.actions.iter().enumerate() {
            let BrainAction::RequestDelegation {
                fan_out_group_id: Some(group_id),
                fan_out_max_concurrency,
                fan_out_failure_policy,
                ..
            } = action
            else {
                continue;
            };
            let group = groups.entry(group_id.clone()).or_default();
            group.indexes.push(index as u32);
            if let Some(max_concurrency) = fan_out_max_concurrency {
                match group.max_concurrency {
                    Some(existing) if existing != *max_concurrency => {
                        rejections.push(ActionRejection {
                            index: index as u32,
                            kind: CoreErrorKind::ActionRejected,
                            message: format!(
                                "fan-out group {group_id} has inconsistent max concurrency"
                            ),
                        });
                    }
                    None => group.max_concurrency = Some(*max_concurrency),
                    _ => {}
                }
            }
            if let Some(policy) = fan_out_failure_policy {
                match &group.failure_policy {
                    Some(existing) if existing != policy => {
                        rejections.push(ActionRejection {
                            index: index as u32,
                            kind: CoreErrorKind::ActionRejected,
                            message: format!(
                                "fan-out group {group_id} has inconsistent failure policy"
                            ),
                        });
                    }
                    None => group.failure_policy = Some(policy.clone()),
                    _ => {}
                }
            }
        }

        for (group_id, group) in groups {
            if let Some(max_concurrency) = group.max_concurrency {
                if group.indexes.len() as u32 > max_concurrency {
                    rejections.extend(group.indexes.into_iter().map(|index| ActionRejection {
                        index,
                        kind: CoreErrorKind::ActionRejected,
                        message: format!(
                            "fan-out group {group_id} exceeds max concurrency {max_concurrency}"
                        ),
                    }));
                }
            }
        }

        rejections
    }

    fn update_lifecycle_for_actions(&self, batch: &BrainActionBatch) -> CoreResult<()> {
        for action in &batch.actions {
            let BrainAction::DeliverCompletion { packet } = action else {
                continue;
            };
            let status = match packet.status {
                CompletionStatus::Completed => WorkerRunStatus::Completed,
                CompletionStatus::Failed => WorkerRunStatus::Failed,
                CompletionStatus::Blocked => WorkerRunStatus::Blocked,
                CompletionStatus::Exhausted => WorkerRunStatus::Exhausted,
            };
            self.store.update_worker_run_status_by_delegated_session(
                &packet.session_id,
                status,
                self.now(),
            )?;
            if let Ok(session) = self.sessions.get_session(&packet.session_id) {
                self.publish_delegation_lifecycle(
                    &session,
                    None,
                    0,
                    delegation_phase_for_completion_status(packet.status.clone()),
                    Some(packet.summary.clone()),
                )?;
            }
        }
        Ok(())
    }

    fn publish_delegation_lifecycle(
        &self,
        session: &SessionState,
        source_wake_id: Option<&str>,
        source_action_index: u32,
        phase: DelegationLifecyclePhase,
        detail: Option<String>,
    ) -> CoreResult<()> {
        let Some(lineage) = &session.delegation else {
            return Ok(());
        };
        let run_id = self
            .store
            .load_worker_run_by_delegated_session(&session.session_id)?
            .map(|run| run.run_id)
            .or_else(|| {
                source_wake_id.map(|wake_id| RunId::new(format!("{wake_id}:{source_action_index}")))
            });
        self.bus.publish(CoreEvent::DelegationLifecycleObserved {
            lifecycle: DelegationLifecycleEvent {
                parent_session_id: lineage.parent_session_id.clone(),
                delegated_session_id: session.session_id.clone(),
                run_id,
                phase,
                detail,
            },
        })?;
        Ok(())
    }

    fn schedule_parent_completion_wakes(&self, batch: &BrainActionBatch) -> CoreResult<()> {
        for action in &batch.actions {
            let BrainAction::DeliverCompletion { packet } = action else {
                continue;
            };
            let Some(run) = self
                .store
                .load_worker_run_by_delegated_session(&packet.session_id)?
            else {
                continue;
            };
            if run.parent_consumption != ParentConsumptionPolicy::AwaitCompletion {
                continue;
            }
            let parent = match self.sessions.get_session(&run.parent_session_id) {
                Ok(parent) => parent,
                Err(error) if error.kind == CoreErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            if !session_kind_can_wake(&parent.kind) || parent.status == SessionStatus::Archived {
                continue;
            }
            self.bus.publish(CoreEvent::BrainWakeRequested {
                session_id: parent.session_id,
            })?;
        }
        Ok(())
    }

    fn apply_fan_out_failure_policy(&self, batch: &BrainActionBatch) -> CoreResult<()> {
        for action in &batch.actions {
            let BrainAction::DeliverCompletion { packet } = action else {
                continue;
            };
            if packet.status == CompletionStatus::Completed {
                continue;
            }
            let Some(run) = self
                .store
                .load_worker_run_by_delegated_session(&packet.session_id)?
            else {
                continue;
            };
            if run.fan_out_failure_policy != FanOutFailurePolicy::FailFast {
                continue;
            }
            let Some(group_id) = run.fan_out_group_id.as_deref() else {
                continue;
            };
            for sibling in self
                .store
                .worker_runs_for_fan_out_group(&run.parent_session_id, group_id)?
            {
                if sibling.run_id == run.run_id || sibling.status.is_terminal() {
                    continue;
                }
                let Some(session_id) = sibling.delegated_session_id else {
                    continue;
                };
                let sibling_session = match self.sessions.get_session(&session_id) {
                    Ok(session) => session,
                    Err(error) if error.kind == CoreErrorKind::NotFound => continue,
                    Err(error) => return Err(error),
                };
                let _ = self.archive_delegated_session_if_nonterminal(
                    &sibling_session,
                    WorkerRunStatus::Cancelled,
                )?;
            }
        }
        Ok(())
    }

    fn schedule_wake_for_event(&self, event: &CoreEvent) -> CoreResult<()> {
        let CoreEvent::AgentMessageRouted { message } = event else {
            return Ok(());
        };
        let session = match self.sessions.get_session_by_agent(&message.to) {
            Ok(session) => session,
            Err(error) if error.kind == CoreErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };

        if !session_kind_can_wake(&session.kind) || session.status == SessionStatus::Archived {
            return Ok(());
        }

        let state = self.body_projector.project(&session.session_id)?;
        if DefaultWakeThreshold.should_wake(&state, event) {
            self.bus.publish(CoreEvent::BrainWakeRequested {
                session_id: session.session_id,
            })?;
        }

        Ok(())
    }
}

pub fn delegated_session_id(
    parent_session_id: &SessionId,
    wake_id: &str,
    index: usize,
) -> SessionId {
    SessionId::new(format!("{parent_session_id}:delegated:{wake_id}:{index}"))
}

pub fn delegated_agent_id(session_id: &SessionId) -> AgentId {
    AgentId::new(format!("agent:{session_id}"))
}

fn add_millis_to_iso(at: &IsoTimestamp, millis: u64) -> CoreResult<IsoTimestamp> {
    let parsed = OffsetDateTime::parse(at, &Rfc3339).map_err(|error| {
        CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("invalid scheduler timestamp {at}: {error}"),
        )
    })?;
    let millis = i64::try_from(millis).map_err(|_| {
        CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("scheduler interval {millis}ms is too large"),
        )
    })?;
    (parsed + Duration::milliseconds(millis))
        .format(&Rfc3339)
        .map_err(|error| {
            CoreError::new(
                CoreErrorKind::InternalError,
                format!("format scheduler timestamp: {error}"),
            )
        })
}

fn next_scheduled_run_id(job_id: &str) -> RunId {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let sequence = NEXT_SCHEDULED_RUN.fetch_add(1, Ordering::Relaxed);
    RunId::new(format!("scheduled:{job_id}:{nanos}:{sequence}"))
}

fn normalized_supported_host_job_kinds(job_kinds: Vec<String>) -> CoreResult<Vec<String>> {
    let mut normalized = Vec::new();
    for job_kind in job_kinds {
        let job_kind = job_kind.trim().to_string();
        if job_kind.is_empty() || job_kind == SCHEDULED_WAKE_JOB_KIND {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "host scheduler claims require non-wake job kinds",
            ));
        }
        if !normalized.contains(&job_kind) {
            normalized.push(job_kind);
        }
    }
    if normalized.is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "host scheduler claims require at least one supported job kind",
        ));
    }
    Ok(normalized)
}

fn next_queued_message_id(session_id: &SessionId) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let sequence = NEXT_QUEUED_MESSAGE.fetch_add(1, Ordering::Relaxed);
    format!("follow-up:{session_id}:{nanos}:{sequence}")
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

fn delegated_run_status(status: WorkerRunStatus) -> DelegatedRunStatus {
    match status {
        WorkerRunStatus::Requested => DelegatedRunStatus::Requested,
        WorkerRunStatus::SessionCreated => DelegatedRunStatus::SessionCreated,
        WorkerRunStatus::WakeRequested => DelegatedRunStatus::WakeRequested,
        WorkerRunStatus::Running => DelegatedRunStatus::Running,
        WorkerRunStatus::CheckpointWaiting => DelegatedRunStatus::CheckpointWaiting,
        WorkerRunStatus::Completed => DelegatedRunStatus::Completed,
        WorkerRunStatus::Failed => DelegatedRunStatus::Failed,
        WorkerRunStatus::Blocked => DelegatedRunStatus::Blocked,
        WorkerRunStatus::Exhausted => DelegatedRunStatus::Exhausted,
        WorkerRunStatus::Cancelled => DelegatedRunStatus::Cancelled,
        WorkerRunStatus::Expired => DelegatedRunStatus::Expired,
    }
}

fn delegation_phase_for_worker_status(status: WorkerRunStatus) -> DelegationLifecyclePhase {
    match status {
        WorkerRunStatus::Expired => DelegationLifecyclePhase::TimedOut,
        WorkerRunStatus::Cancelled => DelegationLifecyclePhase::Cancelled,
        WorkerRunStatus::Completed => DelegationLifecyclePhase::Completed,
        WorkerRunStatus::Failed => DelegationLifecyclePhase::Failed,
        WorkerRunStatus::Blocked => DelegationLifecyclePhase::Blocked,
        WorkerRunStatus::Exhausted => DelegationLifecyclePhase::Exhausted,
        WorkerRunStatus::Requested
        | WorkerRunStatus::SessionCreated
        | WorkerRunStatus::Running
        | WorkerRunStatus::CheckpointWaiting => DelegationLifecyclePhase::Created,
        WorkerRunStatus::WakeRequested => DelegationLifecyclePhase::WakeRequested,
    }
}

fn delegation_phase_for_completion_status(status: CompletionStatus) -> DelegationLifecyclePhase {
    match status {
        CompletionStatus::Completed => DelegationLifecyclePhase::Completed,
        CompletionStatus::Failed => DelegationLifecyclePhase::Failed,
        CompletionStatus::Blocked => DelegationLifecyclePhase::Blocked,
        CompletionStatus::Exhausted => DelegationLifecyclePhase::Exhausted,
    }
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
    use rusty_crew_core_persistence::{
        AgentMessageQuery, CompletionPacketQuery, CoordinationStore, QueryPage,
        QueuedMessageFilter, QueuedMessageRecord, QueuedMessageState, RuntimeCounterScope,
        RuntimeMaintenancePolicy, RuntimeSearchFilter, RuntimeSearchRowType, ScheduledRunQuery,
        ScheduledRunStatus, SessionQuery, ToolCallPhase, WorkerRunQuery,
    };
    use rusty_crew_core_protocol::SessionHistoryWindow;
    use rusty_crew_core_protocol::{
        AdapterId, AgentId, AgentMessage, BrainAction, BrainEvent, ClockConfig, CompletionPacket,
        CompletionStatus, CoreErrorKind, CoreEventKind, DelegatedRunStatus,
        DelegationLifecyclePhase, ExternalEventPayload, ProfileId, ProjectId, ResourceLimits,
        SessionKind, ToolCallMetadata, ToolCallPolicyMetadata, ToolCallSource, ToolDescriptor,
        ToolProfile,
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
        assert_eq!(
            store
                .load_queued_messages(&QueuedMessageFilter {
                    state: Some(QueuedMessageState::Delivered),
                    owner_session_id: Some(prime.session_id),
                    owner_agent_id: None,
                    limit: None,
                })
                .unwrap()
                .len(),
            1
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
            })
            .unwrap();
        engine
            .route_agent_message(AgentMessage {
                from: AgentId::new("operator"),
                to: prime.agent_id.clone(),
                body: "second".to_string(),
                correlation_id: None,
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
    fn persistence_open_failures_are_typed() {
        let data_dir = unique_data_dir("blocked");
        std::fs::write(&data_dir, "not a directory").unwrap();

        let error = CoreEngine::initialize(test_engine_config(data_dir))
            .expect_err("file-backed data dir should fail");

        assert_eq!(error.kind, CoreErrorKind::PersistenceFailure);
    }

    fn test_engine() -> CoreEngine {
        test_engine_with_data_dir(unique_data_dir("engine"))
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
}
