use super::*;
use rusty_crew_core_protocol::{
    ExternalRuntimeId, RuntimeActivityBegin, RuntimeActivityCensusQuery,
    RuntimeActivityFindingCode, RuntimeActivityFinish, RuntimeActivityId, RuntimeActivityKind,
    RuntimeActivityLiveEvidence, RuntimeActivityOwner, RuntimeActivityProgress,
    RuntimeActivityStatus, RuntimeActivityWakeSettlement,
};

#[test]
fn thread_scoped_external_event_replay_rejects_blank_thread_ids() {
    let engine = test_engine();
    for native_thread_id in ["", " \t\n"] {
        let error = engine
            .query_external_runtime_thread_events(
                &ExternalRuntimeId::new("runtime-a"),
                native_thread_id,
                0,
                100,
            )
            .unwrap_err();
        assert_eq!(error.kind, CoreErrorKind::InvalidInput);
        assert!(error.message.contains("native thread id must not be empty"));
    }
}

#[test]
fn wake_settlement_terminalizes_rust_owned_activity_tree_idempotently() {
    let engine = test_engine();
    let session = engine
        .create_session(session_config(
            "settlement-session",
            "settlement-agent",
            "settlement-profile",
            SessionKind::Full,
        ))
        .unwrap();
    for (activity_id, parent_activity_id, kind) in [
        (
            "wake:settlement",
            Some(RuntimeActivityId::new("dispatch:settlement")),
            RuntimeActivityKind::Wake,
        ),
        (
            "provider:settlement",
            Some(RuntimeActivityId::new("wake:settlement")),
            RuntimeActivityKind::ProviderRequest,
        ),
    ] {
        engine
            .begin_runtime_activity(RuntimeActivityBegin {
                activity_id: RuntimeActivityId::new(activity_id),
                parent_activity_id,
                kind,
                owner: RuntimeActivityOwner::RustBrain,
                agent_id: Some(session.agent_id.clone()),
                profile_id: Some(session.profile_id.clone()),
                session_id: Some(session.session_id.clone()),
                wake_id: Some("settlement".into()),
                phase: "running".into(),
                summary: None,
                provider_alias: None,
                model_config_id: None,
                endpoint_id: None,
                model: None,
                tool_name: None,
                process_id: None,
                debug_detail_id: None,
            })
            .unwrap();
    }
    let request = RuntimeActivityWakeSettlement {
        wake_id: "settlement".into(),
        status: RuntimeActivityStatus::Failed,
        reason_code: Some("postgres_storage_exhausted".into()),
        summary: "wake persistence failed".into(),
    };

    let settled = engine
        .settle_runtime_activity_wake(request.clone())
        .unwrap();
    assert_eq!(settled.len(), 2);
    assert!(settled
        .iter()
        .all(|record| record.status == RuntimeActivityStatus::Failed));
    assert!(settled
        .iter()
        .all(|record| record.reason_code.as_deref() == Some("postgres_storage_exhausted")));
    let repeated = engine.settle_runtime_activity_wake(request).unwrap();
    assert!(repeated.is_empty());
    let census = engine
        .runtime_activity_census(RuntimeActivityCensusQuery::default())
        .unwrap();
    assert!(census.active.is_empty());
}

#[test]
fn census_reports_stable_runtime_mismatch_and_orphan_reason_codes() {
    let engine = test_engine();
    let session = engine
        .create_session(session_config(
            "activity-session",
            "activity-agent",
            "activity-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine
        .begin_runtime_activity(RuntimeActivityBegin {
            activity_id: RuntimeActivityId::new("wake:tracked"),
            parent_activity_id: Some(RuntimeActivityId::new("dispatch:missing")),
            kind: RuntimeActivityKind::Wake,
            owner: RuntimeActivityOwner::RustBrain,
            agent_id: Some(session.agent_id.clone()),
            profile_id: Some(session.profile_id.clone()),
            session_id: Some(session.session_id.clone()),
            wake_id: Some("tracked".into()),
            phase: "running".into(),
            summary: None,
            provider_alias: None,
            model_config_id: None,
            endpoint_id: None,
            model: None,
            tool_name: None,
            process_id: None,
            debug_detail_id: None,
        })
        .unwrap();

    let census = engine
        .runtime_activity_census(RuntimeActivityCensusQuery {
            stall_after_ms: Some(1_000),
            recent_abnormal_limit: Some(10),
            live_evidence: vec![
                RuntimeActivityLiveEvidence {
                    activity_id: RuntimeActivityId::new("tool:orphan"),
                    parent_activity_id: Some(RuntimeActivityId::new("provider:missing")),
                    kind: RuntimeActivityKind::ToolCall,
                    owner: RuntimeActivityOwner::TypeScriptHost,
                    agent_id: Some(AgentId::new("wrong-agent")),
                    profile_id: Some(session.profile_id.clone()),
                    session_id: Some(session.session_id.clone()),
                    wake_id: Some("orphan".into()),
                    phase: "running".into(),
                    summary: None,
                    process_id: None,
                    started_at: "2026-06-18T23:00:00Z".into(),
                    last_progress_at: "2026-06-18T23:00:00Z".into(),
                },
                RuntimeActivityLiveEvidence {
                    activity_id: RuntimeActivityId::new("process:777"),
                    parent_activity_id: None,
                    kind: RuntimeActivityKind::Subprocess,
                    owner: RuntimeActivityOwner::TypeScriptHost,
                    agent_id: None,
                    profile_id: None,
                    session_id: None,
                    wake_id: None,
                    phase: "running".into(),
                    summary: Some("untracked child".into()),
                    process_id: Some(777),
                    started_at: "2026-06-19T00:00:00Z".into(),
                    last_progress_at: "2026-06-19T00:00:00Z".into(),
                },
            ],
        })
        .unwrap();
    let codes = census
        .findings
        .iter()
        .map(|finding| finding.code)
        .collect::<Vec<_>>();
    assert!(codes.contains(&RuntimeActivityFindingCode::UntrackedNativeRun));
    assert!(codes.contains(&RuntimeActivityFindingCode::UntrackedServiceProcess));
    assert!(codes.contains(&RuntimeActivityFindingCode::SessionProjectionMismatch));
    assert!(codes.contains(&RuntimeActivityFindingCode::DetachedDispatch));
    assert!(codes.contains(&RuntimeActivityFindingCode::OrphanToolExecution));
    assert!(codes.contains(&RuntimeActivityFindingCode::StaleLedgerEntry));
    assert!(codes.contains(&RuntimeActivityFindingCode::Stalled));
    assert!(!census.automatic_cancellation_enabled);
    assert!(census
        .active
        .iter()
        .all(|view| view.elapsed_ms >= view.since_progress_ms));
}

#[test]
fn canonical_execution_projection_tracks_active_runtime_work() {
    let engine = test_engine();
    let session = engine
        .create_session(session_config(
            "projection-session",
            "projection-agent",
            "projection-profile",
            SessionKind::Full,
        ))
        .unwrap();
    assert_eq!(session.status, SessionStatus::Idle);
    let (_subscription_id, events) = engine
        .subscribe_events(EventSubscription {
            event_kinds: vec![CoreEventKind::SessionExecutionObserved],
            session_id: Some(session.session_id.clone()),
            agent_id: None,
            adapter_id: None,
        })
        .unwrap();
    engine
        .begin_runtime_activity(RuntimeActivityBegin {
            activity_id: RuntimeActivityId::new("dispatch:projection"),
            parent_activity_id: None,
            kind: RuntimeActivityKind::Dispatch,
            owner: RuntimeActivityOwner::TypeScriptHost,
            agent_id: Some(session.agent_id.clone()),
            profile_id: Some(session.profile_id.clone()),
            session_id: Some(session.session_id.clone()),
            wake_id: Some("projection".into()),
            phase: "queued".into(),
            summary: Some("wake queued".into()),
            provider_alias: None,
            model_config_id: None,
            endpoint_id: None,
            model: None,
            tool_name: None,
            process_id: None,
            debug_detail_id: None,
        })
        .unwrap();
    assert_eq!(
        engine
            .session_execution_state(&session.session_id)
            .unwrap()
            .phase,
        SessionExecutionPhase::Queued
    );
    engine
        .begin_runtime_activity(RuntimeActivityBegin {
            activity_id: RuntimeActivityId::new("wake:projection"),
            parent_activity_id: Some(RuntimeActivityId::new("dispatch:projection")),
            kind: RuntimeActivityKind::Wake,
            owner: RuntimeActivityOwner::RustBrain,
            agent_id: Some(session.agent_id.clone()),
            profile_id: Some(session.profile_id.clone()),
            session_id: Some(session.session_id.clone()),
            wake_id: Some("projection".into()),
            phase: "provider_stream".into(),
            summary: None,
            provider_alias: None,
            model_config_id: None,
            endpoint_id: None,
            model: None,
            tool_name: None,
            process_id: None,
            debug_detail_id: None,
        })
        .unwrap();

    let active = engine.session_execution_state(&session.session_id).unwrap();
    assert_eq!(active.phase, SessionExecutionPhase::Active);
    assert_eq!(active.wake_id.as_deref(), Some("projection"));
    assert_eq!(active.last_outcome, None);
    assert_eq!(
        engine.get_session(&session.session_id).unwrap().status,
        SessionStatus::Active
    );
    engine
        .progress_runtime_activity(RuntimeActivityProgress {
            activity_id: RuntimeActivityId::new("wake:projection"),
            phase: "provider_stream".into(),
            summary: Some("heartbeat without a phase transition".into()),
            process_id: None,
            debug_detail_id: None,
        })
        .unwrap();

    engine
        .begin_runtime_activity(RuntimeActivityBegin {
            activity_id: RuntimeActivityId::new("tool:projection"),
            parent_activity_id: Some(RuntimeActivityId::new("wake:projection")),
            kind: RuntimeActivityKind::ToolCall,
            owner: RuntimeActivityOwner::TypeScriptHost,
            agent_id: Some(session.agent_id.clone()),
            profile_id: Some(session.profile_id.clone()),
            session_id: Some(session.session_id.clone()),
            wake_id: Some("projection".into()),
            phase: "executing".into(),
            summary: Some("running tool".into()),
            provider_alias: None,
            model_config_id: None,
            endpoint_id: None,
            model: None,
            tool_name: Some("read_file".into()),
            process_id: None,
            debug_detail_id: None,
        })
        .unwrap();
    assert_eq!(
        engine
            .session_execution_state(&session.session_id)
            .unwrap()
            .phase,
        SessionExecutionPhase::Waiting
    );
    engine
        .finish_runtime_activity(RuntimeActivityFinish {
            activity_id: RuntimeActivityId::new("tool:projection"),
            status: RuntimeActivityStatus::Completed,
            phase: "completed".into(),
            reason_code: Some("tool_completed".into()),
            summary: Some("tool completed".into()),
        })
        .unwrap();
    assert_eq!(
        engine
            .session_execution_state(&session.session_id)
            .unwrap()
            .phase,
        SessionExecutionPhase::Active
    );
    engine
        .finish_runtime_activity(RuntimeActivityFinish {
            activity_id: RuntimeActivityId::new("wake:projection"),
            status: RuntimeActivityStatus::Completed,
            phase: "completed".into(),
            reason_code: Some("wake_completed".into()),
            summary: Some("wake completed".into()),
        })
        .unwrap();
    assert_eq!(
        engine
            .session_execution_state(&session.session_id)
            .unwrap()
            .phase,
        SessionExecutionPhase::Queued
    );
    engine
        .finish_runtime_activity(RuntimeActivityFinish {
            activity_id: RuntimeActivityId::new("dispatch:projection"),
            status: RuntimeActivityStatus::Completed,
            phase: "completed".into(),
            reason_code: Some("dispatch_completed".into()),
            summary: Some("dispatch completed".into()),
        })
        .unwrap();
    let idle = engine.session_execution_state(&session.session_id).unwrap();
    assert_eq!(idle.phase, SessionExecutionPhase::Idle);
    assert_eq!(idle.last_outcome, Some(SessionExecutionOutcome::Completed));
    assert_eq!(
        engine.get_session(&session.session_id).unwrap().status,
        SessionStatus::Idle
    );

    let phases = (0..6)
        .map(|_| events.recv_timeout(Duration::from_millis(100)).unwrap())
        .map(|event| match event {
            CoreEvent::SessionExecutionObserved { execution } => execution.phase,
            event => panic!("unexpected execution event: {event:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        phases,
        vec![
            SessionExecutionPhase::Queued,
            SessionExecutionPhase::Active,
            SessionExecutionPhase::Waiting,
            SessionExecutionPhase::Active,
            SessionExecutionPhase::Queued,
            SessionExecutionPhase::Idle,
        ]
    );
    assert!(matches!(
        events.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
    let healthy = engine
        .runtime_activity_census(RuntimeActivityCensusQuery::default())
        .unwrap();
    assert!(!healthy.findings.iter().any(|finding| {
        finding.code == RuntimeActivityFindingCode::SessionProjectionMismatch
            && finding.activity_id.0 == "wake:projection"
    }));
}

#[test]
fn canonical_execution_projection_isolates_concurrent_failure_and_cancellation() {
    let engine = test_engine();
    let failed = engine
        .create_session(session_config(
            "failed-session",
            "failed-agent",
            "failed-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let cancelled = engine
        .create_session(session_config(
            "cancelled-session",
            "cancelled-agent",
            "cancelled-profile",
            SessionKind::Full,
        ))
        .unwrap();

    for (session, activity_id) in [(&failed, "wake:failed"), (&cancelled, "wake:cancelled")] {
        engine
            .begin_runtime_activity(RuntimeActivityBegin {
                activity_id: RuntimeActivityId::new(activity_id),
                parent_activity_id: None,
                kind: RuntimeActivityKind::Wake,
                owner: RuntimeActivityOwner::RustBrain,
                agent_id: Some(session.agent_id.clone()),
                profile_id: Some(session.profile_id.clone()),
                session_id: Some(session.session_id.clone()),
                wake_id: Some(activity_id.into()),
                phase: "running".into(),
                summary: None,
                provider_alias: None,
                model_config_id: None,
                endpoint_id: None,
                model: None,
                tool_name: None,
                process_id: None,
                debug_detail_id: None,
            })
            .unwrap();
    }
    assert_eq!(
        engine
            .session_execution_state(&failed.session_id)
            .unwrap()
            .phase,
        SessionExecutionPhase::Active
    );
    assert_eq!(
        engine
            .session_execution_state(&cancelled.session_id)
            .unwrap()
            .phase,
        SessionExecutionPhase::Active
    );

    engine
        .finish_runtime_activity(RuntimeActivityFinish {
            activity_id: RuntimeActivityId::new("wake:failed"),
            status: RuntimeActivityStatus::Failed,
            phase: "failed".into(),
            reason_code: Some("provider_failed".into()),
            summary: Some("provider failed".into()),
        })
        .unwrap();
    let failed_state = engine.session_execution_state(&failed.session_id).unwrap();
    assert_eq!(failed_state.phase, SessionExecutionPhase::Idle);
    assert_eq!(
        failed_state.last_outcome,
        Some(SessionExecutionOutcome::Failed)
    );
    assert_eq!(
        engine
            .session_execution_state(&cancelled.session_id)
            .unwrap()
            .phase,
        SessionExecutionPhase::Active
    );

    engine
        .finish_runtime_activity(RuntimeActivityFinish {
            activity_id: RuntimeActivityId::new("wake:cancelled"),
            status: RuntimeActivityStatus::Cancelled,
            phase: "cancelled".into(),
            reason_code: Some("operator_cancelled".into()),
            summary: Some("operator cancelled".into()),
        })
        .unwrap();
    let cancelled_state = engine
        .session_execution_state(&cancelled.session_id)
        .unwrap();
    assert_eq!(cancelled_state.phase, SessionExecutionPhase::Idle);
    assert_eq!(
        cancelled_state.last_outcome,
        Some(SessionExecutionOutcome::Cancelled)
    );
}

#[test]
fn restart_interrupts_prior_instance_activity_without_resurrecting_it() {
    let data_dir = unique_data_dir("runtime-activity-restart");
    let first = test_engine_with_data_dir(data_dir.clone());
    let session = first
        .create_session(session_config(
            "restart-session",
            "restart-agent",
            "restart-profile",
            SessionKind::Full,
        ))
        .unwrap();
    first
        .begin_runtime_activity(RuntimeActivityBegin {
            activity_id: RuntimeActivityId::new("dispatch:restart"),
            parent_activity_id: None,
            kind: RuntimeActivityKind::Dispatch,
            owner: RuntimeActivityOwner::TypeScriptHost,
            agent_id: Some(session.agent_id.clone()),
            profile_id: Some(session.profile_id.clone()),
            session_id: Some(session.session_id.clone()),
            wake_id: Some("restart".into()),
            phase: "running".into(),
            summary: None,
            provider_alias: None,
            model_config_id: None,
            endpoint_id: None,
            model: None,
            tool_name: None,
            process_id: None,
            debug_detail_id: None,
        })
        .unwrap();
    let first_instance = first.service_instance_id().to_string();
    drop(first);

    let restarted = test_engine_with_data_dir(data_dir);
    assert_ne!(restarted.service_instance_id(), first_instance);
    let census = restarted
        .runtime_activity_census(RuntimeActivityCensusQuery::default())
        .unwrap();
    assert!(census.active.is_empty());
    let execution = restarted
        .session_execution_state(&session.session_id)
        .unwrap();
    assert_eq!(execution.phase, SessionExecutionPhase::Idle);
    assert_eq!(
        execution.last_outcome,
        Some(SessionExecutionOutcome::Interrupted)
    );
    assert_eq!(
        execution.reason_code.as_deref(),
        Some("restart_interrupted")
    );
    assert_eq!(
        restarted.get_session(&session.session_id).unwrap().status,
        SessionStatus::Idle
    );
    let interrupted = census
        .recently_abnormal
        .iter()
        .find(|view| view.activity.activity_id.0 == "dispatch:restart")
        .expect("restart-interrupted activity");
    assert_eq!(
        interrupted.activity.status,
        RuntimeActivityStatus::Interrupted
    );
    assert_eq!(
        interrupted.activity.reason_code.as_deref(),
        Some("restart_interrupted")
    );
    assert!(census
        .findings
        .iter()
        .any(|finding| finding.code == RuntimeActivityFindingCode::RestartInterrupted));
}
