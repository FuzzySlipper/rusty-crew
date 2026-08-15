use super::external_runtime::{external_creation_request, ready_external_creation_dependencies};
use super::*;
use rusty_crew_core_protocol::{ExternalAgentSessionCreationPhase, ExternalBindingStatus};

#[test]
fn external_agent_session_creation_is_idempotent_and_recovers_native_start() {
    let engine = test_engine();
    let controller = ready_external_creation_dependencies(&engine);
    let request = external_creation_request("create-agent-1");

    let prepared = engine
        .prepare_external_agent_session_creation(request.clone())
        .unwrap();
    assert_eq!(
        prepared.phase,
        ExternalAgentSessionCreationPhase::BindingReady
    );
    assert_eq!(prepared.session.profile_id, ProfileId::new("codex-profile"));
    assert_eq!(
        prepared.binding.session_id,
        Some(prepared.session.session_id.clone())
    );
    assert_eq!(prepared.binding.native_thread_id, None);
    assert_eq!(prepared.binding.status, ExternalBindingStatus::Paused);
    assert!(!prepared.binding.is_routable());
    assert_eq!(
        prepared.native_thread_source.len(),
        "rusty-crew:".len() + 24
    );

    let mut retry = request.clone();
    retry.requested_at = "2026-06-19T00:00:05Z".into();
    assert_eq!(
        engine
            .prepare_external_agent_session_creation(retry)
            .unwrap(),
        prepared
    );

    let starting = engine
        .mark_external_agent_session_native_starting(
            &controller,
            &prepared.creation_id,
            prepared.revision,
            "2026-06-19T00:00:06Z".into(),
        )
        .unwrap();
    let recovering = engine
        .record_external_agent_session_creation_failure(
            &controller,
            &starting.creation_id,
            starting.revision,
            "external_agent_creation_native_start_failed".into(),
            "native transport disconnected".into(),
            "2026-06-19T00:00:07Z".into(),
        )
        .unwrap();
    assert_eq!(
        recovering.phase,
        ExternalAgentSessionCreationPhase::RecoveryRequired
    );
    assert_eq!(recovering.binding.status, ExternalBindingStatus::Paused);
    assert_eq!(recovering.session.status, SessionStatus::Archived);
    assert!(engine.list_agent_directory().unwrap().is_empty());

    let recovered = engine
        .prepare_external_agent_session_creation(request.clone())
        .unwrap();
    assert_eq!(recovered.session.status, SessionStatus::Idle);
    let restarting = engine
        .mark_external_agent_session_native_starting(
            &controller,
            &recovered.creation_id,
            recovered.revision,
            "2026-06-19T00:00:08Z".into(),
        )
        .unwrap();
    let ready = engine
        .complete_external_agent_session_creation(
            &controller,
            &restarting.creation_id,
            restarting.revision,
            "native-thread-created".into(),
            "2026-06-19T00:00:09Z".into(),
        )
        .unwrap();
    assert_eq!(ready.phase, ExternalAgentSessionCreationPhase::Ready);
    assert_eq!(ready.binding.status, ExternalBindingStatus::Active);
    assert!(ready.binding.is_routable());
    assert_eq!(
        ready.binding.native_thread_id.as_deref(),
        Some("native-thread-created")
    );
    assert_eq!(
        engine
            .prepare_external_agent_session_creation(request)
            .unwrap(),
        ready
    );
}

#[test]
fn failed_external_creation_stays_non_live_across_restart() {
    let data_dir = unique_data_dir("failed-external-creation-restart");
    let engine = test_engine_with_data_dir(data_dir.clone());
    let controller = ready_external_creation_dependencies(&engine);
    let prepared = engine
        .prepare_external_agent_session_creation(external_creation_request("failed-create-restart"))
        .unwrap();
    let starting = engine
        .mark_external_agent_session_native_starting(
            &controller,
            &prepared.creation_id,
            prepared.revision,
            "2026-06-19T00:00:02Z".into(),
        )
        .unwrap();
    let failed = engine
        .record_external_agent_session_creation_failure(
            &controller,
            &starting.creation_id,
            starting.revision,
            "external_agent_creation_native_start_failed".into(),
            "native transport disconnected".into(),
            "2026-06-19T00:00:03Z".into(),
        )
        .unwrap();
    assert_eq!(failed.session.status, SessionStatus::Archived);
    assert_eq!(failed.binding.status, ExternalBindingStatus::Paused);
    drop(engine);

    let restarted = test_engine_with_data_dir(data_dir);
    assert_eq!(
        restarted
            .get_session(&failed.session.session_id)
            .unwrap()
            .status,
        SessionStatus::Archived
    );
    assert_eq!(
        restarted
            .get_external_binding(&failed.binding.binding_id)
            .unwrap()
            .unwrap()
            .status,
        ExternalBindingStatus::Paused
    );
    assert!(restarted.list_agent_directory().unwrap().is_empty());
}

#[test]
fn restart_reconciles_legacy_active_binding_without_native_thread() {
    let data_dir = unique_data_dir("legacy-incomplete-external-binding");
    let engine = test_engine_with_data_dir(data_dir.clone());
    ready_external_creation_dependencies(&engine);
    let creation = engine
        .prepare_external_agent_session_creation(external_creation_request(
            "legacy-incomplete-binding",
        ))
        .unwrap();
    let mut placeholder = creation.binding;
    placeholder.status = ExternalBindingStatus::Active;
    placeholder.updated_at = "2026-06-19T00:00:02Z".into();
    let placeholder = engine
        .bind_external_agent(&placeholder, Some(placeholder.revision))
        .unwrap();
    assert!(engine
        .list_agent_directory()
        .unwrap()
        .iter()
        .any(|entry| entry.session_id == creation.session.session_id));
    drop(engine);

    let restarted = test_engine_with_data_dir(data_dir);
    assert_eq!(
        restarted
            .get_external_binding(&placeholder.binding_id)
            .unwrap()
            .unwrap()
            .status,
        ExternalBindingStatus::Paused
    );
    assert_eq!(
        restarted
            .get_session(&creation.session.session_id)
            .unwrap()
            .status,
        SessionStatus::Archived
    );
    assert!(restarted.list_agent_directory().unwrap().is_empty());
}
