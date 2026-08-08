use super::*;

#[test]
fn executes_valid_brain_actions_against_real_bus() {
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
                        to: planner.agent_id.clone(),
                        from_session_id: Some(worker.session_id.clone()),
                        to_session_id: Some(planner.session_id.clone()),
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
