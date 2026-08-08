use super::*;
use rusty_crew_core_protocol::{CoreEvent, EventSubscription, SessionWorkspaceUpdate};

#[test]
fn workspace_switch_is_revisioned_evented_and_restart_durable() {
    let data_dir = unique_data_dir("session-workspace-switch");
    let engine = test_engine_with_data_dir(data_dir.clone());
    let session = engine
        .create_session(session_config(
            "workspace-session",
            "coder",
            "coder-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let (_subscription, events) = engine
        .subscribe_events(EventSubscription {
            event_kinds: vec![CoreEventKind::SessionWorkspaceChanged],
            session_id: Some(session.session_id.clone()),
            agent_id: None,
            adapter_id: None,
        })
        .unwrap();

    let switched = engine
        .update_session_workspace(&SessionWorkspaceUpdate {
            session_id: session.session_id.clone(),
            cwd: "/home/dev/other/../second-repo".to_string(),
            expected_revision: 1,
            requested_at: "2026-08-08T12:00:00Z".to_string(),
        })
        .unwrap();
    assert_eq!(switched.current.cwd, "/home/dev/second-repo");
    assert_eq!(switched.current.revision, 2);
    assert_eq!(switched.session.session_id, session.session_id);
    assert!(matches!(
        events.recv_timeout(Duration::from_secs(1)).unwrap(),
        CoreEvent::SessionWorkspaceChanged { current, .. }
            if current.cwd == "/home/dev/second-repo" && current.revision == 2
    ));

    engine.shutdown().unwrap();
    let restarted = test_engine_with_data_dir(data_dir);
    let hydrated = restarted.get_session(&session.session_id).unwrap();
    assert_eq!(hydrated.workspace, Some(switched.current));
}
