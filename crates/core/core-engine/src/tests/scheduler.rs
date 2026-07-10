use super::*;

#[test]
fn scheduler_tick_requests_wake_and_records_terminal_run() {
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
        .register_scheduled_wake_job(
            "wake-prime",
            prime.session_id.clone(),
            Some(60_000),
            "2026-06-19T00:00:00Z".to_string(),
        )
        .unwrap();
    let report = engine.run_scheduler_tick().unwrap();

    assert_eq!(report.due_runs_claimed, 1);
    assert_eq!(report.wakes_requested, 1);
    assert_eq!(report.runs_completed, 1);
    assert!(matches!(
        events.recv_timeout(Duration::from_secs(1)).unwrap(),
        CoreEvent::BrainWakeRequested { session_id } if session_id == prime.session_id
    ));
    let store = CoordinationStore::open(engine.config.engine_data_dir.clone()).unwrap();
    let runs = store
        .query_scheduled_runs(&ScheduledRunQuery {
            status: Some(ScheduledRunStatus::Completed),
            target_session_id: Some(prime.session_id.clone()),
            ..ScheduledRunQuery::default()
        })
        .unwrap();
    assert_eq!(runs.len(), 1);
    assert!(
        runs[0]
            .run_id
            .0
            .starts_with("scheduled:wake-prime:2026_06_19T00_00_00Z:"),
        "scheduled run id should be derived from the engine clock, got {}",
        runs[0].run_id
    );
    assert_eq!(
        store
            .load_scheduled_job("wake-prime")
            .unwrap()
            .unwrap()
            .next_due_at,
        Some("2026-06-19T00:01:00Z".to_string())
    );
}
