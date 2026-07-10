use super::*;

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
    let mut config = session_config("prime-session", "prime", "prime-profile", SessionKind::Full);
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
    let mut config = session_config("prime-session", "prime", "prime-profile", SessionKind::Full);
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
