use super::external_runtime::{external_creation_request, ready_external_creation_dependencies};
use super::*;

#[test]
fn external_sessions_sharing_a_profile_project_as_distinct_instances() {
    let data_dir = unique_data_dir("external-profile-siblings");
    let engine = test_engine_with_data_dir(data_dir.clone());
    ready_external_creation_dependencies(&engine);
    let mut first_request = external_creation_request("external-sibling-first");
    first_request.cwd = "/home/dev/external-a".into();
    let first = engine
        .prepare_external_agent_session_creation(first_request)
        .unwrap();
    let mut second_request = external_creation_request("external-sibling-second");
    second_request.cwd = "/home/dev/external-b".into();
    second_request.requested_at = "2026-06-19T00:00:02Z".into();
    let second = engine
        .prepare_external_agent_session_creation(second_request)
        .unwrap();

    assert_eq!(first.session.profile_id, second.session.profile_id);
    assert_ne!(first.session.session_id, second.session.session_id);
    assert_ne!(first.session.agent_id, second.session.agent_id);
    let directory = engine
        .list_agent_directory()
        .unwrap()
        .into_iter()
        .filter(|entry| entry.profile_id == ProfileId::new("codex-profile"))
        .collect::<Vec<_>>();
    assert_eq!(directory.len(), 2);
    assert_eq!(
        directory
            .iter()
            .find(|entry| entry.session_id == first.session.session_id)
            .and_then(|entry| entry.workdir.as_deref()),
        Some("/home/dev/external-a")
    );
    assert_eq!(
        directory
            .iter()
            .find(|entry| entry.session_id == second.session.session_id)
            .and_then(|entry| entry.workdir.as_deref()),
        Some("/home/dev/external-b")
    );
    drop(engine);

    let restarted = test_engine_with_data_dir(data_dir);
    let hydrated = restarted
        .list_agent_directory()
        .unwrap()
        .into_iter()
        .filter(|entry| entry.profile_id == ProfileId::new("codex-profile"))
        .collect::<Vec<_>>();
    assert_eq!(hydrated.len(), 2);
    assert!(hydrated
        .iter()
        .any(|entry| entry.session_id == first.session.session_id));
    assert!(hydrated
        .iter()
        .any(|entry| entry.session_id == second.session.session_id));
}
