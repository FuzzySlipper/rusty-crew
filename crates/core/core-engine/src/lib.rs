//! Coordination engine composition.

use rusty_crew_core_body::{BodyProjector, BrainActionExecutor};
use rusty_crew_core_bus::CoreBus;
use rusty_crew_core_protocol::{
    ActionBatchReceipt, BodyState, BrainActionBatch, ClockConfig, CoreEvent, CoreResult,
    DenDataUpdate, EngineConfig, EngineHandle, EventReceipt, EventSubscription, ExternalEvent,
    IsoTimestamp, SessionConfig, SessionId, SessionState, ShutdownSummary,
};
use rusty_crew_core_session::SessionRegistry;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Receiver;

static NEXT_ENGINE_HANDLE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone)]
pub struct CoreEngine {
    handle: EngineHandle,
    config: EngineConfig,
    bus: CoreBus,
    sessions: SessionRegistry,
    body_projector: BodyProjector,
    action_executor: BrainActionExecutor,
}

impl CoreEngine {
    pub fn initialize(config: EngineConfig) -> CoreResult<Self> {
        let bus = CoreBus::new();
        let sessions = SessionRegistry::new();

        Ok(Self {
            handle: EngineHandle::new(NEXT_ENGINE_HANDLE.fetch_add(1, Ordering::Relaxed)),
            config,
            body_projector: BodyProjector::new(bus.clone(), sessions.clone()),
            action_executor: BrainActionExecutor::new(bus.clone(), sessions.clone()),
            bus,
            sessions,
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
        self.bus.publish(CoreEvent::SessionArchived {
            session_id: session_id.clone(),
        })?;
        Ok(state)
    }

    pub fn project_body_state(&self, session_id: &SessionId) -> CoreResult<BodyState> {
        self.body_projector.project(session_id)
    }

    pub fn execute_brain_actions(&self, batch: BrainActionBatch) -> CoreResult<ActionBatchReceipt> {
        self.action_executor.execute(batch)
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_crew_core_protocol::{
        AdapterId, AgentId, AgentMessage, BrainAction, ClockConfig, CompletionPacket,
        CompletionStatus, CoreErrorKind, CoreEventKind, ExternalEventPayload, ProfileId, ProjectId,
        ResourceLimits, SessionKind, ToolDescriptor, ToolProfile,
    };
    use std::time::Duration;

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

    fn test_engine() -> CoreEngine {
        CoreEngine::initialize(EngineConfig {
            engine_data_dir: "/tmp/rusty-crew-test".to_string(),
            clock: ClockConfig::Fixed {
                at: "2026-06-19T00:00:00Z".to_string(),
            },
            default_turn_budget: 3,
            default_idle_timeout_ms: 1000,
        })
        .unwrap()
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
