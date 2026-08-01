use super::*;

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
                        narrator_diagnostic: None,
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
    engine
        .register_profile_tool_profile(
            ProfileId::new("coder-profile"),
            ToolProfile { tools: Vec::new() },
        )
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
