use super::*;

#[test]
fn github_gate_wait_is_durable_idempotent_and_recovers_exact_session_wake() {
    let data_dir = unique_data_dir("github-gate-wait");
    let engine = test_engine_with_data_dir(data_dir.clone());
    let session = engine
        .create_session(session_config(
            "gate-session",
            "gate-agent",
            "gate-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let wait = engine
        .suspend_for_github_gate(GitHubGateSuspendRequest {
            session_id: session.session_id.clone(),
            run_id: Some(RunId::new("run-1")),
            provider_thread_id: Some("thread-1".to_string()),
            project_id: ProjectId::new("den-services"),
            task_id: TaskId::new("5500"),
            gate_id: 901,
            commit_sha: "1111111111111111111111111111111111111111".to_string(),
            now: "2026-06-19T00:00:10Z".to_string(),
        })
        .unwrap();
    assert_eq!(wait.phase, GitHubGateWaitPhase::Waiting);
    assert_eq!(
        engine.get_session(&session.session_id).unwrap().status,
        SessionStatus::Idle
    );

    let (_, receiver) = engine
        .subscribe_events(EventSubscription {
            event_kinds: vec![CoreEventKind::BrainWakeRequested],
            session_id: Some(session.session_id.clone()),
            agent_id: None,
            adapter_id: None,
        })
        .unwrap();
    let event = GitHubGateTerminalEvent {
        event_id: 44,
        gate_id: 901,
        project_id: ProjectId::new("den-services"),
        task_id: TaskId::new("5500"),
        commit_sha: "1111111111111111111111111111111111111111".to_string(),
        status: "failed".to_string(),
        terminal_reason: "required_checks_missing".to_string(),
        summary: Some("wrong check name".to_string()),
        failure_summary: Some("missing Verify".to_string()),
        completed_at: "2026-06-19T00:01:00Z".to_string(),
    };
    let receipt = engine
        .consume_github_gate_terminal_event(event.clone())
        .unwrap();
    assert!(receipt.wake_scheduled);
    assert!(matches!(
        receiver.recv_timeout(std::time::Duration::from_millis(50)),
        Ok(CoreEvent::BrainWakeRequested { session_id }) if session_id == session.session_id
    ));
    let duplicate = engine.consume_github_gate_terminal_event(event).unwrap();
    assert!(duplicate.duplicate);
    assert!(!duplicate.wake_scheduled);
    let queued = engine
        .store
        .load_queued_messages(&rusty_crew_core_persistence::QueuedMessageFilter {
            state: Some(QueuedMessageState::Pending),
            owner_session_id: Some(session.session_id.clone()),
            owner_agent_id: None,
            limit: None,
        })
        .unwrap();
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].message_id, "github-gate-event:44");
    assert!(queued[0].message.body.contains("required_checks_missing"));
    drop(engine);

    let hydrated = test_engine_with_data_dir(data_dir);
    let persisted = hydrated
        .github_gate_wait(&session.session_id)
        .unwrap()
        .unwrap();
    assert_eq!(persisted.phase, GitHubGateWaitPhase::WakeScheduled);
    let (_, recovered_receiver) = hydrated
        .subscribe_events(EventSubscription {
            event_kinds: vec![CoreEventKind::BrainWakeRequested],
            session_id: Some(session.session_id.clone()),
            agent_id: None,
            adapter_id: None,
        })
        .unwrap();
    assert_eq!(hydrated.recover_github_gate_wakes().unwrap(), 1);
    assert!(recovered_receiver
        .recv_timeout(std::time::Duration::from_millis(50))
        .is_ok());
}

#[test]
fn newer_github_gate_wait_rejects_stale_sha_terminal_event() {
    let engine = test_engine();
    let session = engine
        .create_session(session_config(
            "newer-gate-session",
            "gate-agent",
            "gate-profile",
            SessionKind::Full,
        ))
        .unwrap();
    for (gate_id, sha) in [
        (1, "1111111111111111111111111111111111111111"),
        (2, "2222222222222222222222222222222222222222"),
    ] {
        engine
            .suspend_for_github_gate(GitHubGateSuspendRequest {
                session_id: session.session_id.clone(),
                run_id: None,
                provider_thread_id: None,
                project_id: ProjectId::new("den-services"),
                task_id: TaskId::new("5500"),
                gate_id,
                commit_sha: sha.to_string(),
                now: "2026-06-19T00:00:10Z".to_string(),
            })
            .unwrap();
    }
    let receipt = engine
        .consume_github_gate_terminal_event(GitHubGateTerminalEvent {
            event_id: 1,
            gate_id: 1,
            project_id: ProjectId::new("den-services"),
            task_id: TaskId::new("5500"),
            commit_sha: "1111111111111111111111111111111111111111".to_string(),
            status: "superseded".to_string(),
            terminal_reason: "superseded".to_string(),
            summary: None,
            failure_summary: None,
            completed_at: "2026-06-19T00:01:00Z".to_string(),
        })
        .unwrap();
    assert!(!receipt.wake_scheduled);
    assert_eq!(
        receipt.ignored_reason.as_deref(),
        Some("no_current_wait_for_gate_and_sha")
    );
    assert_eq!(
        engine
            .github_gate_wait(&session.session_id)
            .unwrap()
            .unwrap()
            .gate_id,
        2
    );
}
