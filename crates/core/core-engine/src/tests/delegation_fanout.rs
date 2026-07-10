use super::*;

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
