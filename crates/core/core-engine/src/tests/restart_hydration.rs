use super::*;

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
                        from_session_id: Some(planner.session_id.clone()),
                        to_session_id: Some(reviewer.session_id.clone()),
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
                    workspace_constraint: Some(DelegatedWorkspaceConstraint {
                        cwd: "/home/dev/rusty-crew".to_string(),
                    }),
                    resource_limits: Some(ResourceLimits {
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
                    from_session_id: Some(reviewer.session_id.clone()),
                    to_session_id: Some(observer.session_id.clone()),
                    body: "persistent proof review forwarded".to_string(),
                    correlation_id: Some("proof-thread".to_string()),
                    projection: None,
                },
            }],
        })
        .unwrap();

    let delegated_session_id = delegated_session_id(&planner.session_id, "proof-planner-wake", 1);
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
                from_session_id: None,
                to_session_id: Some(planner.session_id.clone()),
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
                from_session_id: None,
                to_session_id: Some(planner.session_id.clone()),
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
