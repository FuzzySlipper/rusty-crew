use super::*;

#[test]
fn repeated_unchanged_adapter_failure_is_a_durable_noop() {
    let engine = test_engine();
    let session = engine
        .create_session(session_config(
            "review-retry-session",
            "review-retry-agent",
            "review-retry-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let record = begin(&engine, &session.session_id, 6698, 'a');
    let failure = ReviewSubmissionTransition::AdapterFailed {
        reason_code: "den_mcp_binding_unavailable".to_string(),
        summary: "configured binding unavailable".to_string(),
    };
    let first = engine
        .transition_review_submission(ReviewSubmissionTransitionRequest {
            submission_id: record.submission_id.clone(),
            expected_revision: record.revision,
            transition: failure.clone(),
            now: "2026-08-08T09:00:00Z".to_string(),
        })
        .unwrap();
    let retry = engine
        .transition_review_submission(ReviewSubmissionTransitionRequest {
            submission_id: record.submission_id,
            expected_revision: first.revision,
            transition: failure,
            now: "2026-08-08T09:01:00Z".to_string(),
        })
        .unwrap();

    assert_eq!(retry.phase, ReviewSubmissionPhase::Submitted);
    assert_eq!(retry.revision, first.revision);
    assert_eq!(retry.updated_at, first.updated_at);
    assert_eq!(retry.last_adapter_error, first.last_adapter_error);
}

#[test]
fn reviewer_dispatch_failures_exhaust_one_generation_and_route_change_rearms() {
    let engine = test_engine();
    let session = engine
        .create_session(session_config(
            "review-bounded-retry-session",
            "review-bounded-retry-agent",
            "review-bounded-retry-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let mut record = begin(&engine, &session.session_id, 6841, 'b');
    for transition in [
        ReviewSubmissionTransition::DenHandoffRecorded {
            review_round_id: 4475,
        },
        ReviewSubmissionTransition::GateRegistered { gate_id: 3237 },
        ReviewSubmissionTransition::GateTerminal {
            gate_status: "passed".to_string(),
            terminal_reason: "checks_passed".to_string(),
        },
    ] {
        record = engine
            .transition_review_submission(ReviewSubmissionTransitionRequest {
                submission_id: record.submission_id.clone(),
                expected_revision: record.revision,
                transition,
                now: "2026-08-11T16:00:00Z".to_string(),
            })
            .unwrap();
    }

    for minute in 1..=6 {
        record = engine
            .transition_review_submission(ReviewSubmissionTransitionRequest {
                submission_id: record.submission_id.clone(),
                expected_revision: record.revision,
                transition: ReviewSubmissionTransition::ReviewerDispatchFailed {
                    reason_code: "agent_route_disabled".to_string(),
                    summary: "reviewer route is disabled".to_string(),
                    retry_generation: "route-4:binding-9".to_string(),
                },
                now: format!("2026-08-11T16:{minute:02}:00Z"),
            })
            .unwrap();
    }
    assert_eq!(record.reviewer_dispatch_attempts, 6);
    assert_eq!(record.reviewer_dispatch_next_retry_at, None);
    let exhausted_revision = record.revision;
    let unchanged = engine
        .transition_review_submission(ReviewSubmissionTransitionRequest {
            submission_id: record.submission_id.clone(),
            expected_revision: record.revision,
            transition: ReviewSubmissionTransition::ReviewerDispatchFailed {
                reason_code: "agent_route_disabled".to_string(),
                summary: "reviewer route is disabled".to_string(),
                retry_generation: "route-4:binding-9".to_string(),
            },
            now: "2026-08-12T16:00:00Z".to_string(),
        })
        .unwrap();
    assert_eq!(unchanged.revision, exhausted_revision);

    let rearmed = engine
        .transition_review_submission(ReviewSubmissionTransitionRequest {
            submission_id: record.submission_id,
            expected_revision: unchanged.revision,
            transition: ReviewSubmissionTransition::ReviewerDispatchFailed {
                reason_code: "agent_route_disabled".to_string(),
                summary: "replacement reviewer is not enabled yet".to_string(),
                retry_generation: "route-5:binding-10".to_string(),
            },
            now: "2026-08-12T16:01:00Z".to_string(),
        })
        .unwrap();
    assert_eq!(rearmed.reviewer_dispatch_attempts, 1);
    assert_eq!(
        rearmed.reviewer_dispatch_generation.as_deref(),
        Some("route-5:binding-10")
    );
    assert!(rearmed.reviewer_dispatch_next_retry_at.is_some());
}

#[test]
fn terminal_reviewer_dispatch_can_return_to_dispatch_pending_without_losing_evidence() {
    let engine = test_engine();
    let session = engine
        .create_session(session_config(
            "review-recovery-session",
            "review-recovery-agent",
            "review-recovery-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let mut record = begin(&engine, &session.session_id, 6779, 'd');
    for transition in [
        ReviewSubmissionTransition::DenHandoffRecorded {
            review_round_id: 4386,
        },
        ReviewSubmissionTransition::GateRegistered { gate_id: 3156 },
        ReviewSubmissionTransition::GateTerminal {
            gate_status: "passed".to_string(),
            terminal_reason: "checks_passed".to_string(),
        },
    ] {
        record = engine
            .transition_review_submission(ReviewSubmissionTransitionRequest {
                submission_id: record.submission_id.clone(),
                expected_revision: record.revision,
                transition,
                now: "2026-08-10T12:00:00Z".to_string(),
            })
            .unwrap();
    }
    record = engine
        .transition_review_submission(ReviewSubmissionTransitionRequest {
            submission_id: record.submission_id.clone(),
            expected_revision: record.revision,
            transition: ReviewSubmissionTransition::ReviewerDispatched {
                reviewer_session_id: session.session_id,
                dispatch_message_id: "review-message:original".to_string(),
                dispatch_delivery_id: "review-delivery:original".to_string(),
            },
            now: "2026-08-10T12:01:00Z".to_string(),
        })
        .unwrap();
    let recovered = engine
        .transition_review_submission(ReviewSubmissionTransitionRequest {
            submission_id: record.submission_id.clone(),
            expected_revision: record.revision,
            transition: ReviewSubmissionTransition::ReviewerRedispatchPending {
                reason_code: "reviewer_inbox_awaiting_reply".to_string(),
            },
            now: "2026-08-10T12:02:00Z".to_string(),
        })
        .unwrap();
    let late_original = engine.transition_review_submission(ReviewSubmissionTransitionRequest {
        submission_id: recovered.submission_id.clone(),
        expected_revision: recovered.revision,
        transition: ReviewSubmissionTransition::DenFinalizationPending {
            result_digest: "late-original".to_string(),
            result_json: r#"{"verdict":"looks_good"}"#.to_string(),
        },
        now: "2026-08-10T12:02:30Z".to_string(),
    });
    assert_eq!(
        late_original.unwrap_err().kind,
        CoreErrorKind::ActionRejected
    );

    assert_eq!(
        recovered.phase,
        ReviewSubmissionPhase::ReviewerDispatchPending
    );
    assert_eq!(
        recovered.dispatch_message_id.as_deref(),
        Some("review-message:original")
    );
    assert_eq!(
        recovered.dispatch_delivery_id.as_deref(),
        Some("review-delivery:original")
    );
    assert_eq!(
        recovered.last_adapter_error.as_deref(),
        Some("reviewer_redispatch_pending: reviewer_inbox_awaiting_reply")
    );
}

#[test]
fn already_finalized_den_round_settles_pending_submission_with_verdict() {
    let engine = test_engine();
    let session = engine
        .create_session(session_config(
            "review-finalized-session",
            "review-finalized-agent",
            "review-finalized-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let record = begin(&engine, &session.session_id, 6698, 'b');
    let settled = engine
        .transition_review_submission(ReviewSubmissionTransitionRequest {
            submission_id: record.submission_id,
            expected_revision: record.revision,
            transition: ReviewSubmissionTransition::DenAlreadyFinalized {
                review_round_id: 4149,
                exact_head_commit: exact_sha('b'),
                verdict: "looks_good".to_string(),
                task_status: "done".to_string(),
                terminal_reason: "den_round_already_finalized".to_string(),
            },
            now: "2026-08-08T09:30:00Z".to_string(),
        })
        .unwrap();

    assert_eq!(settled.phase, ReviewSubmissionPhase::ReviewTerminal);
    assert_eq!(settled.review_round_id, Some(4149));
    assert_eq!(settled.review_exact_head_commit, Some(exact_sha('b')));
    assert_eq!(settled.review_verdict.as_deref(), Some("looks_good"));
    assert_eq!(settled.review_task_status.as_deref(), Some("done"));
    assert_eq!(
        settled.terminal_reason.as_deref(),
        Some("den_round_already_finalized")
    );
}

#[test]
fn passing_review_gate_is_restart_durable_and_does_not_wake_submitter() {
    let data_dir = unique_data_dir("review-submission-pass");
    let engine = test_engine_with_data_dir(data_dir.clone());
    let session = engine
        .create_session(session_config(
            "review-submit-session",
            "review-submit-agent",
            "review-submit-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let mut record = begin(&engine, &session.session_id, 6574, 'a');
    record = engine
        .transition_review_submission(ReviewSubmissionTransitionRequest {
            submission_id: record.submission_id.clone(),
            expected_revision: record.revision,
            transition: ReviewSubmissionTransition::DenHandoffRecorded {
                review_round_id: 77,
            },
            now: "2026-08-02T00:01:00Z".to_string(),
        })
        .unwrap();
    record = engine
        .transition_review_submission(ReviewSubmissionTransitionRequest {
            submission_id: record.submission_id.clone(),
            expected_revision: record.revision,
            transition: ReviewSubmissionTransition::GateRegistered { gate_id: 91 },
            now: "2026-08-02T00:02:00Z".to_string(),
        })
        .unwrap();
    assert_eq!(record.phase, ReviewSubmissionPhase::GatePending);
    let persisted_pending = engine
        .list_review_submissions(&ReviewSubmissionQuery {
            submission_id: Some(record.submission_id.clone()),
            pending_only: true,
            ..ReviewSubmissionQuery::default()
        })
        .unwrap();
    assert_eq!(persisted_pending.len(), 1);
    assert_eq!(
        persisted_pending[0].phase,
        ReviewSubmissionPhase::GatePending
    );
    assert_eq!(persisted_pending[0].task_id, TaskId::new("6574"));
    assert_eq!(persisted_pending[0].commit_sha, exact_sha('a'));

    let (_, receiver) = engine
        .subscribe_events(EventSubscription {
            event_kinds: vec![CoreEventKind::BrainWakeRequested],
            session_id: Some(session.session_id.clone()),
            agent_id: None,
            adapter_id: None,
        })
        .unwrap();
    let receipt = engine
        .consume_github_gate_terminal_event(GitHubGateTerminalEvent {
            event_id: 80,
            gate_id: 91,
            project_id: ProjectId::new("rusty-crew"),
            task_id: TaskId::new("6574"),
            commit_sha: exact_sha('a'),
            status: "passed".to_string(),
            terminal_reason: "checks_passed".to_string(),
            summary: Some("all checks passed".to_string()),
            failure_summary: None,
            completed_at: "2026-08-02T00:03:00Z".to_string(),
        })
        .unwrap();
    assert!(!receipt.wake_scheduled);
    assert!(receiver
        .recv_timeout(std::time::Duration::from_millis(25))
        .is_err());
    drop(engine);

    let hydrated = test_engine_with_data_dir(data_dir);
    let pending = hydrated
        .list_review_submissions(&ReviewSubmissionQuery {
            pending_only: true,
            ..ReviewSubmissionQuery::default()
        })
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(
        pending[0].phase,
        ReviewSubmissionPhase::ReviewerDispatchPending
    );
    assert_eq!(pending[0].submitter_session_id, Some(session.session_id));
}

#[test]
fn review_submission_query_bounds_and_pages_the_store_read() {
    let engine = test_engine();
    let session = engine
        .create_session(session_config(
            "review-page-session",
            "review-page-agent",
            "review-page-profile",
            SessionKind::Full,
        ))
        .unwrap();
    begin(&engine, &session.session_id, 7101, 'a');
    begin(&engine, &session.session_id, 7102, 'b');
    begin(&engine, &session.session_id, 7103, 'c');

    let first = engine
        .list_review_submissions(&ReviewSubmissionQuery {
            project_id: Some(ProjectId::new("rusty-crew")),
            limit: Some(2),
            offset: Some(0),
            ..ReviewSubmissionQuery::default()
        })
        .unwrap();
    let second = engine
        .list_review_submissions(&ReviewSubmissionQuery {
            project_id: Some(ProjectId::new("rusty-crew")),
            limit: Some(2),
            offset: Some(2),
            ..ReviewSubmissionQuery::default()
        })
        .unwrap();
    assert_eq!(first.len(), 2);
    assert_eq!(second.len(), 1);
    assert!(first.iter().all(|record| !second
        .iter()
        .any(|next| next.submission_id == record.submission_id)));
}

#[test]
fn newer_sha_supersedes_old_workflow_and_duplicate_submit_is_idempotent() {
    let engine = test_engine();
    let session = engine
        .create_session(session_config(
            "review-supersede-session",
            "review-supersede-agent",
            "review-supersede-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let first = begin(&engine, &session.session_id, 6574, '1');
    let duplicate = begin(&engine, &session.session_id, 6574, '1');
    assert_eq!(duplicate.submission_id, first.submission_id);
    assert_eq!(duplicate.revision, first.revision);
    let mismatch = engine.begin_review_submission(ReviewSubmissionRequest {
        review_summary_md: "Changed summary after submission.".to_string(),
        ..request(&session.session_id, 6574, '1')
    });
    assert_eq!(mismatch.unwrap_err().kind, CoreErrorKind::ActionRejected);
    let second = begin(&engine, &session.session_id, 6574, '2');
    assert_ne!(second.submission_id, first.submission_id);
    let records = engine
        .list_review_submissions(&ReviewSubmissionQuery {
            task_id: Some(TaskId::new("6574")),
            ..ReviewSubmissionQuery::default()
        })
        .unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(
        records
            .iter()
            .find(|record| record.submission_id == first.submission_id)
            .unwrap()
            .phase,
        ReviewSubmissionPhase::Superseded
    );
}

#[test]
fn external_cli_review_is_sessionless_and_gate_terminal_event_advances_it() {
    let engine = test_engine();
    let commit_sha = exact_sha('e');
    let mut record = engine
        .begin_review_submission(ReviewSubmissionRequest {
            caller: AgentCoordinationCaller::ExternalCli {
                client_id: "unmanaged-codex".to_string(),
                idempotency_key: "review-6644-e".to_string(),
            },
            project_id: ProjectId::new("rusty-crew"),
            task_id: TaskId::new("6644"),
            repository: "FuzzySlipper/rusty-crew".to_string(),
            commit_sha: commit_sha.clone(),
            git_ref: "main".to_string(),
            required_checks: vec!["Verify Offline".to_string()],
            base_commit: Some(exact_sha('0')),
            review_summary_md: "Submitted by an external CLI.".to_string(),
            reviewer: "@reviewer".to_string(),
            now: "2026-08-04T00:00:00Z".to_string(),
        })
        .unwrap();
    assert_eq!(record.submitter_session_id, None);
    assert!(matches!(
        record.caller,
        AgentCoordinationCaller::ExternalCli { .. }
    ));
    record = engine
        .transition_review_submission(ReviewSubmissionTransitionRequest {
            submission_id: record.submission_id.clone(),
            expected_revision: record.revision,
            transition: ReviewSubmissionTransition::DenHandoffRecorded {
                review_round_id: 664401,
            },
            now: "2026-08-04T00:01:00Z".to_string(),
        })
        .unwrap();
    record = engine
        .transition_review_submission(ReviewSubmissionTransitionRequest {
            submission_id: record.submission_id.clone(),
            expected_revision: record.revision,
            transition: ReviewSubmissionTransition::GateRegistered { gate_id: 664402 },
            now: "2026-08-04T00:02:00Z".to_string(),
        })
        .unwrap();
    assert_eq!(record.phase, ReviewSubmissionPhase::GatePending);
    assert!(engine
        .github_gate_wait(&SessionId::new("never-created"))
        .unwrap()
        .is_none());

    let receipt = engine
        .consume_github_gate_terminal_event(GitHubGateTerminalEvent {
            event_id: 664403,
            gate_id: 664402,
            project_id: ProjectId::new("rusty-crew"),
            task_id: TaskId::new("6644"),
            commit_sha,
            status: "passed".to_string(),
            terminal_reason: "checks_passed".to_string(),
            summary: Some("checks passed".to_string()),
            failure_summary: None,
            completed_at: "2026-08-04T00:03:00Z".to_string(),
        })
        .unwrap();
    assert!(!receipt.wake_scheduled);
    assert_eq!(
        receipt.ignored_reason.as_deref(),
        Some("review_submission_dispatch_pending")
    );
    let pending = engine
        .list_review_submissions(&ReviewSubmissionQuery {
            submission_id: Some(record.submission_id),
            pending_only: true,
            ..ReviewSubmissionQuery::default()
        })
        .unwrap();
    assert_eq!(
        pending[0].phase,
        ReviewSubmissionPhase::ReviewerDispatchPending
    );
    assert_eq!(pending[0].submitter_session_id, None);
}

#[test]
fn external_cli_checkless_review_advances_directly_after_den_handoff() {
    let engine = test_engine();
    let mut record = engine
        .begin_review_submission(ReviewSubmissionRequest {
            caller: AgentCoordinationCaller::ExternalCli {
                client_id: "unmanaged-codex".to_string(),
                idempotency_key: "review-6797-checkless".to_string(),
            },
            project_id: ProjectId::new("den-services"),
            task_id: TaskId::new("6797"),
            repository: "FuzzySlipper/den-services".to_string(),
            commit_sha: exact_sha('f'),
            git_ref: "main".to_string(),
            required_checks: Vec::new(),
            base_commit: Some(exact_sha('0')),
            review_summary_md: "Submitted without a GitHub gate.".to_string(),
            reviewer: "@reviewer".to_string(),
            now: "2026-08-11T05:00:00Z".to_string(),
        })
        .unwrap();
    record = engine
        .transition_review_submission(ReviewSubmissionTransitionRequest {
            submission_id: record.submission_id.clone(),
            expected_revision: record.revision,
            transition: ReviewSubmissionTransition::DenHandoffRecorded {
                review_round_id: 679701,
            },
            now: "2026-08-11T05:01:00Z".to_string(),
        })
        .unwrap();
    record = engine
        .transition_review_submission(ReviewSubmissionTransitionRequest {
            submission_id: record.submission_id.clone(),
            expected_revision: record.revision,
            transition: ReviewSubmissionTransition::GateTerminal {
                gate_status: "passed".to_string(),
                terminal_reason: "no_required_checks".to_string(),
            },
            now: "2026-08-11T05:02:00Z".to_string(),
        })
        .unwrap();

    assert_eq!(record.phase, ReviewSubmissionPhase::ReviewerDispatchPending);
    assert_eq!(record.gate_id, None);
    assert_eq!(record.gate_status.as_deref(), Some("passed"));
    assert_eq!(
        record.terminal_reason.as_deref(),
        Some("no_required_checks")
    );
}

#[test]
fn managed_brain_submission_still_requires_declared_checks() {
    let engine = test_engine();
    let session = engine
        .create_session(session_config(
            "review-check-required-session",
            "review-check-required-agent",
            "review-check-required-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let mut input = request(&session.session_id, 6798, 'a');
    input.required_checks.clear();

    let error = engine.begin_review_submission(input).unwrap_err();
    assert_eq!(error.kind, CoreErrorKind::InvalidInput);
}

#[test]
fn submitted_workflow_is_restart_pending_and_can_fill_initial_base_commit() {
    let data_dir = unique_data_dir("review-submission-early-restart");
    let engine = test_engine_with_data_dir(data_dir.clone());
    let session = engine
        .create_session(session_config(
            "review-early-session",
            "review-early-agent",
            "review-early-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let mut initial = request(&session.session_id, 6574, '3');
    initial.base_commit = None;
    let record = engine.begin_review_submission(initial).unwrap();
    drop(engine);

    let hydrated = test_engine_with_data_dir(data_dir);
    let pending = hydrated
        .list_review_submissions(&ReviewSubmissionQuery {
            pending_only: true,
            ..ReviewSubmissionQuery::default()
        })
        .unwrap();
    assert_eq!(pending, vec![record.clone()]);

    let resumed = hydrated
        .begin_review_submission(request(&session.session_id, 6574, '3'))
        .unwrap();
    assert_eq!(resumed.submission_id, record.submission_id);
    assert_eq!(resumed.base_commit, Some(exact_sha('0')));
    assert_eq!(resumed.revision, record.revision + 1);
}

#[test]
fn routed_completion_checkpoints_are_restart_durable_and_reply_terminal_is_stable() {
    let engine = test_engine();
    let submitter = engine
        .create_session(session_config(
            "review-checkpoint-submit",
            "review-checkpoint-runner",
            "review-checkpoint-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let reviewer = engine
        .create_session(session_config(
            "review-checkpoint-reviewer",
            "review-checkpoint-reviewer-agent",
            "review-checkpoint-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let mut record = begin(&engine, &submitter.session_id, 6609, '5');
    record = engine
        .transition_review_submission(ReviewSubmissionTransitionRequest {
            submission_id: record.submission_id.clone(),
            expected_revision: record.revision,
            transition: ReviewSubmissionTransition::DenHandoffRecorded {
                review_round_id: 701,
            },
            now: "2026-08-02T00:04:00Z".to_string(),
        })
        .unwrap();
    record = engine
        .transition_review_submission(ReviewSubmissionTransitionRequest {
            submission_id: record.submission_id.clone(),
            expected_revision: record.revision,
            transition: ReviewSubmissionTransition::GateRegistered { gate_id: 702 },
            now: "2026-08-02T00:04:30Z".to_string(),
        })
        .unwrap();
    engine
        .consume_github_gate_terminal_event(GitHubGateTerminalEvent {
            event_id: 703,
            gate_id: 702,
            project_id: ProjectId::new("rusty-crew"),
            task_id: TaskId::new("6609"),
            commit_sha: exact_sha('5'),
            status: "passed".to_string(),
            terminal_reason: "checks_passed".to_string(),
            summary: None,
            failure_summary: None,
            completed_at: "2026-08-02T00:04:45Z".to_string(),
        })
        .unwrap();
    record = engine
        .list_review_submissions(&ReviewSubmissionQuery {
            submission_id: Some(record.submission_id.clone()),
            ..ReviewSubmissionQuery::default()
        })
        .unwrap()
        .remove(0);
    record = engine
        .transition_review_submission(ReviewSubmissionTransitionRequest {
            submission_id: record.submission_id.clone(),
            expected_revision: record.revision,
            transition: ReviewSubmissionTransition::ReviewerDispatched {
                reviewer_session_id: reviewer.session_id.clone(),
                dispatch_message_id: "review-message:checkpoint".to_string(),
                dispatch_delivery_id: "review-delivery:checkpoint".to_string(),
            },
            now: "2026-08-02T00:05:00Z".to_string(),
        })
        .unwrap();
    record = engine
        .transition_review_submission(ReviewSubmissionTransitionRequest {
            submission_id: record.submission_id.clone(),
            expected_revision: record.revision,
            transition: ReviewSubmissionTransition::DenFinalizationPending {
                result_digest: "digest".to_string(),
                result_json: r#"{"verdict":"looks_good"}"#.to_string(),
            },
            now: "2026-08-02T00:06:00Z".to_string(),
        })
        .unwrap();
    record = engine
        .transition_review_submission(ReviewSubmissionTransitionRequest {
            submission_id: record.submission_id.clone(),
            expected_revision: record.revision,
            transition: ReviewSubmissionTransition::DenFinalized {
                finalization_id: 1,
                packet_id: 2,
                packet_message_id: 3,
                exact_head_commit: exact_sha('5'),
                verdict: "looks_good".to_string(),
                finding_statuses: vec![],
                task_status: "done".to_string(),
                material_digest: Some("material".to_string()),
            },
            now: "2026-08-02T00:07:00Z".to_string(),
        })
        .unwrap();
    record = engine
        .transition_review_submission(ReviewSubmissionTransitionRequest {
            submission_id: record.submission_id.clone(),
            expected_revision: record.revision,
            transition: ReviewSubmissionTransition::ReplyPending,
            now: "2026-08-02T00:08:00Z".to_string(),
        })
        .unwrap();
    let terminal = engine
        .transition_review_submission(ReviewSubmissionTransitionRequest {
            submission_id: record.submission_id.clone(),
            expected_revision: record.revision,
            transition: ReviewSubmissionTransition::ReplyTerminal {
                reason_code: "requester_expired".to_string(),
            },
            now: "2026-08-02T00:09:00Z".to_string(),
        })
        .unwrap();
    assert_eq!(terminal.phase, ReviewSubmissionPhase::ReplyTerminal);
    assert_eq!(terminal.review_finalization_id, Some(1));
    assert_eq!(
        terminal.reply_reason_code.as_deref(),
        Some("requester_expired")
    );
    let pending = engine
        .list_review_submissions(&ReviewSubmissionQuery {
            reviewer_session_id: Some(reviewer.session_id),
            ..ReviewSubmissionQuery::default()
        })
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].phase, ReviewSubmissionPhase::ReplyTerminal);
}

#[test]
#[cfg(feature = "postgres")]
#[ignore = "requires local PostgreSQL dev database env"]
fn postgres_review_submission_matches_restart_and_revision_contract() {
    let database_url = std::env::var("RUSTY_CREW_DATABASE_URL")
        .or_else(|_| std::env::var("RUSTY_CREW_POSTGRES_BACKEND_DATABASE_URL"))
        .expect("PostgreSQL database URL must be configured");
    let schema = format!(
        "rc_review_submission_{}_{}",
        std::process::id(),
        NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed)
    );
    let data_dir = unique_data_dir("review-submission-postgres");
    let config = EngineConfig {
        engine_data_dir: data_dir.to_string_lossy().to_string(),
        clock: ClockConfig::Fixed {
            at: "2026-08-02T00:00:00Z".to_string(),
        },
        default_turn_budget: 3,
        default_idle_timeout_ms: 1_000,
        storage: Some(EngineStorageConfig::Postgres {
            database_url,
            schema,
            max_connections: None,
            statement_timeout_ms: None,
            backing_filesystem_path: None,
            filesystem_warning_free_percent: None,
        }),
    };
    let engine = CoreEngine::initialize(config.clone()).unwrap();
    let session = engine
        .create_session(session_config(
            "review-postgres-session",
            "review-postgres-agent",
            "review-postgres-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let mut record = begin(&engine, &session.session_id, 6574, '4');
    record = engine
        .transition_review_submission(ReviewSubmissionTransitionRequest {
            submission_id: record.submission_id.clone(),
            expected_revision: record.revision,
            transition: ReviewSubmissionTransition::DenHandoffRecorded {
                review_round_id: 8_201,
            },
            now: "2026-08-02T00:01:00Z".to_string(),
        })
        .unwrap();
    record = engine
        .transition_review_submission(ReviewSubmissionTransitionRequest {
            submission_id: record.submission_id.clone(),
            expected_revision: record.revision,
            transition: ReviewSubmissionTransition::GateRegistered { gate_id: 8_202 },
            now: "2026-08-02T00:02:00Z".to_string(),
        })
        .unwrap();
    engine
        .consume_github_gate_terminal_event(GitHubGateTerminalEvent {
            event_id: 8_203,
            gate_id: 8_202,
            project_id: ProjectId::new("rusty-crew"),
            task_id: TaskId::new("6574"),
            commit_sha: exact_sha('4'),
            status: "passed".to_string(),
            terminal_reason: "checks_passed".to_string(),
            summary: None,
            failure_summary: None,
            completed_at: "2026-08-02T00:03:00Z".to_string(),
        })
        .unwrap();
    record = engine
        .list_review_submissions(&ReviewSubmissionQuery {
            submission_id: Some(record.submission_id.clone()),
            ..ReviewSubmissionQuery::default()
        })
        .unwrap()
        .remove(0);
    record = engine
        .transition_review_submission(ReviewSubmissionTransitionRequest {
            submission_id: record.submission_id.clone(),
            expected_revision: record.revision,
            transition: ReviewSubmissionTransition::ReviewerDispatchFailed {
                reason_code: "route_disabled".to_string(),
                summary: "Reviewer route is disabled.".to_string(),
                retry_generation: "route-7:binding-0".to_string(),
            },
            now: "2026-08-02T00:04:00Z".to_string(),
        })
        .unwrap();
    drop(engine);

    let hydrated = CoreEngine::initialize(config).unwrap();
    let pending = hydrated
        .list_review_submissions(&ReviewSubmissionQuery {
            pending_only: true,
            ..ReviewSubmissionQuery::default()
        })
        .unwrap();
    assert_eq!(pending, vec![record]);
    assert_eq!(pending[0].reviewer_dispatch_attempts, 1);
    assert_eq!(
        pending[0].reviewer_dispatch_generation.as_deref(),
        Some("route-7:binding-0")
    );
    assert_eq!(
        pending[0].reviewer_dispatch_next_retry_at.as_deref(),
        Some("2026-08-02T00:04:30Z")
    );
}

fn begin(
    engine: &CoreEngine,
    session_id: &SessionId,
    task_id: u64,
    sha: char,
) -> rusty_crew_core_protocol::ReviewSubmissionRecord {
    engine
        .begin_review_submission(request(session_id, task_id, sha))
        .unwrap()
}

fn request(session_id: &SessionId, task_id: u64, sha: char) -> ReviewSubmissionRequest {
    ReviewSubmissionRequest {
        caller: AgentCoordinationCaller::DirectBrain {
            session_id: session_id.clone(),
            wake_id: "wake-review".to_string(),
            tool_call_id: "tool-review".to_string(),
        },
        project_id: ProjectId::new("rusty-crew"),
        task_id: TaskId::new(task_id.to_string()),
        repository: "FuzzySlipper/rusty-crew".to_string(),
        commit_sha: exact_sha(sha),
        git_ref: "main".to_string(),
        required_checks: vec!["Verify Offline".to_string()],
        base_commit: Some(exact_sha('0')),
        review_summary_md: "Implemented and verified.".to_string(),
        reviewer: "@reviewer".to_string(),
        now: "2026-08-02T00:00:00Z".to_string(),
    }
}

fn exact_sha(character: char) -> String {
    std::iter::repeat_n(character, 40).collect()
}
