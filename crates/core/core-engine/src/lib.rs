//! Coordination engine composition.

use rusty_crew_core_body::{session_kind_can_wake, BodyProjector, BrainActionExecutor};
use rusty_crew_core_bus::{CoreBus, SequencedEvent};
use rusty_crew_core_persistence::CoordinationStore;
use rusty_crew_core_protocol::{
    ActionBatchReceipt, AgentId, AgentMessage, BodyState, BrainAction, BrainActionBatch,
    BrainEventEnvelope, ClockConfig, CoreError, CoreErrorKind, CoreEvent, CoreResult,
    DenDataUpdate, EngineConfig, EngineHandle, EventReceipt, EventSubscription, ExternalEvent,
    IsoTimestamp, ResourceLimits, SessionConfig, SessionId, SessionKind, SessionState,
    SessionStatus, ShutdownSummary, ToolProfile,
};
use rusty_crew_core_session::SessionRegistry;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;

static NEXT_ENGINE_HANDLE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone)]
pub struct CoreEngine {
    handle: EngineHandle,
    config: EngineConfig,
    bus: CoreBus,
    sessions: SessionRegistry,
    store: CoordinationStore,
    body_projector: BodyProjector,
    action_executor: BrainActionExecutor,
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

        Ok(Self {
            handle: EngineHandle::new(NEXT_ENGINE_HANDLE.fetch_add(1, Ordering::Relaxed)),
            config,
            body_projector: BodyProjector::new(bus.clone(), sessions.clone()),
            action_executor: BrainActionExecutor::new(bus.clone(), sessions.clone()),
            bus,
            sessions,
            store,
        })
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

    pub fn create_session(&self, config: SessionConfig) -> CoreResult<SessionState> {
        let state = self.sessions.create_session(config, self.now())?;
        self.store.save_session(&state)?;
        self.bus.publish(CoreEvent::SessionCreated {
            state: state.clone(),
        })?;
        Ok(state)
    }

    pub fn get_session(&self, session_id: &SessionId) -> CoreResult<SessionState> {
        self.sessions.get_session(session_id)
    }

    pub fn archive_session(&self, session_id: &SessionId) -> CoreResult<SessionState> {
        let state = self.sessions.archive_session(session_id, self.now())?;
        self.store.save_session(&state)?;
        self.bus.publish(CoreEvent::SessionArchived {
            session_id: session_id.clone(),
        })?;
        Ok(state)
    }

    pub fn project_body_state(&self, session_id: &SessionId) -> CoreResult<BodyState> {
        self.body_projector.project(session_id)
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

        self.store.save_worker_runs_requested(&batch, self.now())?;
        self.spawn_delegated_workers(&session, &batch)?;
        self.action_executor.execute(batch)
    }

    pub fn submit_brain_event(&self, envelope: BrainEventEnvelope) -> CoreResult<EventReceipt> {
        let sequence = self.bus.publish(CoreEvent::BrainEventObserved {
            session_id: envelope.session_id,
            event: envelope.event,
        })?;
        Ok(EventReceipt {
            accepted: true,
            sequence,
        })
    }

    pub fn inject_external_event(&self, event: ExternalEvent) -> CoreResult<EventReceipt> {
        let sequence = self
            .bus
            .publish(CoreEvent::ExternalEventInjected { event })?;
        Ok(EventReceipt {
            accepted: true,
            sequence,
        })
    }

    pub fn inject_den_data_update(&self, update: DenDataUpdate) -> CoreResult<EventReceipt> {
        let sequence = self.bus.publish(CoreEvent::DenDataUpdated { update })?;
        Ok(EventReceipt {
            accepted: true,
            sequence,
        })
    }

    pub fn count_rows(&self, table: &str) -> CoreResult<u64> {
        self.store.count_rows(table)
    }

    pub fn shutdown(self) -> CoreResult<ShutdownSummary> {
        Ok(ShutdownSummary {
            engine: self.handle,
            archived_sessions: 0,
            dropped_subscriptions: 0,
        })
    }

    fn now(&self) -> IsoTimestamp {
        match &self.config.clock {
            ClockConfig::System => "system-clock-placeholder".to_string(),
            ClockConfig::Fixed { at } => at.clone(),
        }
    }

    fn spawn_delegated_workers(
        &self,
        parent: &SessionState,
        batch: &BrainActionBatch,
    ) -> CoreResult<()> {
        for (index, action) in batch.actions.iter().enumerate() {
            let BrainAction::RequestDelegation {
                profile_id, prompt, ..
            } = action
            else {
                continue;
            };

            let session_id = delegated_session_id(&batch.session_id, &batch.wake_id, index);
            let agent_id = delegated_agent_id(&session_id);
            let state = self.sessions.create_session(
                SessionConfig {
                    session_id: session_id.clone(),
                    agent_id: agent_id.clone(),
                    profile_id: profile_id.clone(),
                    kind: SessionKind::Delegated,
                    resource_limits: ResourceLimits {
                        workdir: None,
                        max_duration_ms: None,
                        max_delegation_depth: Some(0),
                    },
                    tool_profile: ToolProfile { tools: Vec::new() },
                },
                self.now(),
            )?;
            self.store.save_session(&state)?;
            self.bus.publish(CoreEvent::SessionCreated {
                state: state.clone(),
            })?;
            self.bus.publish(CoreEvent::AgentMessageRouted {
                message: AgentMessage {
                    from: parent.agent_id.clone(),
                    to: agent_id,
                    body: prompt.clone(),
                    correlation_id: Some(format!("delegation:{}:{index}", batch.wake_id)),
                },
            })?;
            if session_kind_can_wake(&state.kind) {
                self.bus.publish(CoreEvent::BrainWakeRequested {
                    session_id: state.session_id,
                })?;
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_crew_core_persistence::CoordinationStore;
    use rusty_crew_core_protocol::{
        AdapterId, AgentId, AgentMessage, BrainAction, BrainEvent, ClockConfig, CompletionPacket,
        CompletionStatus, CoreErrorKind, CoreEventKind, ExternalEventPayload, ProfileId, ProjectId,
        ResourceLimits, SessionKind, ToolDescriptor, ToolProfile,
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
                }],
            })
            .unwrap();

        assert_eq!(receipt.accepted_actions, 1);
        let delegated_session_id = delegated_session_id(&planner.session_id, "planner-wake", 0);
        let delegated = engine.get_session(&delegated_session_id).unwrap();
        assert_eq!(delegated.kind, SessionKind::Delegated);
        assert_eq!(delegated.profile_id, ProfileId::new("coder-profile"));

        let body = engine.project_body_state(&delegated_session_id).unwrap();
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
                event: BrainEvent::TextDelta { .. },
                ..
            }
        ));
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
        let hydrated_body = restarted_engine
            .project_body_state(&worker.session_id)
            .expect("worker body should hydrate from persisted bus history");
        let store = CoordinationStore::open(data_dir).unwrap();

        assert_eq!(hydrated_planner.kind, SessionKind::Full);
        assert_eq!(hydrated_worker.kind, SessionKind::Worker);
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
        }
    }
}
