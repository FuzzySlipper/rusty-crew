use super::*;

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
fn delegation_rejects_unregistered_profile_before_creating_worker_state() {
    let data_dir = unique_data_dir("delegated-unregistered-profile");
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
            actions: vec![delegation_request_for_profile("missing-worker-profile")],
        })
        .unwrap();

    assert_eq!(receipt.accepted_actions, 0);
    assert_eq!(receipt.rejected_actions.len(), 1);
    assert_eq!(
        receipt.rejected_actions[0].kind,
        CoreErrorKind::ActionRejected
    );
    assert_eq!(
        receipt.rejected_actions[0].message,
        "delegation profile missing-worker-profile is not registered with a brain implementation"
    );
    let store = CoordinationStore::open(data_dir).unwrap();
    assert_eq!(store.count_rows("sessions").unwrap(), 1);
    assert_eq!(store.count_rows("worker_runs").unwrap(), 0);
}

#[test]
fn delegation_accepts_registered_profile_with_empty_tool_profile() {
    let data_dir = unique_data_dir("delegated-empty-profile");
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
            ProfileId::new("empty-worker-profile"),
            ToolProfile { tools: Vec::new() },
        )
        .unwrap();

    let receipt = engine
        .execute_brain_actions(BrainActionBatch {
            wake_id: "planner-wake".to_string(),
            session_id: planner.session_id.clone(),
            actions: vec![delegation_request_for_profile("empty-worker-profile")],
        })
        .unwrap();

    assert_eq!(receipt.accepted_actions, 1);
    assert!(receipt.rejected_actions.is_empty());
    let delegated = engine
        .get_session(&delegated_session_id(
            &planner.session_id,
            "planner-wake",
            0,
        ))
        .unwrap();
    assert_eq!(delegated.profile_id, ProfileId::new("empty-worker-profile"));
    assert!(delegated.tool_profile.tools.is_empty());
}

fn delegation_request_for_profile(profile_id: &str) -> BrainAction {
    BrainAction::RequestDelegation {
        profile_id: ProfileId::new(profile_id),
        task_id: None,
        prompt: "complete a delegated profile validation proof".to_string(),
        expected_output: None,
        resource_limits: Some(ResourceLimits {
            workdir: Some("/home/dev/rusty-crew".to_string()),
            max_duration_ms: None,
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
    }
}
