use super::*;
use rusty_crew_core_protocol::{
    RuntimeActivityBegin, RuntimeActivityCensusQuery, RuntimeActivityFindingCode,
    RuntimeActivityId, RuntimeActivityKind, RuntimeActivityLiveEvidence, RuntimeActivityOwner,
    RuntimeActivityStatus,
};

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
fn restart_interrupts_prior_instance_activity_without_resurrecting_it() {
    let data_dir = unique_data_dir("runtime-activity-restart");
    let first = test_engine_with_data_dir(data_dir.clone());
    first
        .begin_runtime_activity(RuntimeActivityBegin {
            activity_id: RuntimeActivityId::new("dispatch:restart"),
            parent_activity_id: None,
            kind: RuntimeActivityKind::Dispatch,
            owner: RuntimeActivityOwner::TypeScriptHost,
            agent_id: None,
            profile_id: None,
            session_id: None,
            wake_id: Some("restart".into()),
            phase: "running".into(),
            summary: None,
            provider_alias: None,
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
