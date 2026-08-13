use super::external_runtime::{binding, runtime};
use super::*;
use rusty_crew_core_protocol::{ExternalBindingId, ExternalBindingStatus};

#[test]
fn projects_same_service_direct_and_external_routability() {
    let engine = test_engine();
    let mut direct_profile =
        profile_registry_write("direct-profile", "tester-chat", "direct-session");
    direct_profile.display_name = Some("Direct planner".into());
    engine
        .create_profile_registry_record(&direct_profile)
        .unwrap();
    engine
        .create_session(session_config(
            "direct-session",
            "direct-agent",
            "direct-profile",
            SessionKind::Full,
        ))
        .unwrap();

    let mut external_profile = profile_registry_write("codex-profile", "gpt", "codex-session");
    external_profile.display_name = Some("Codex coder".into());
    engine
        .create_profile_registry_record(&external_profile)
        .unwrap();
    engine
        .create_session(session_config(
            "codex-session",
            "codex-agent",
            "codex-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine.register_external_runtime(&runtime(), None).unwrap();
    let saved_binding = engine.bind_external_agent(&binding(), None).unwrap();

    let directory = engine.list_agent_directory().unwrap();
    assert_eq!(directory.len(), 2);
    let direct = directory
        .iter()
        .find(|entry| entry.agent_id == AgentId::new("direct-agent"))
        .unwrap();
    assert_eq!(direct.display_label, "Direct planner");
    assert_eq!(direct.runtime_kind, AgentDirectoryRuntimeKind::DirectBrain);
    assert!(direct.routable);
    assert!(direct.binding_id.is_none());
    assert_eq!(
        direct
            .workspace
            .as_ref()
            .map(|workspace| workspace.cwd.as_str()),
        Some("/home/dev/rusty-crew")
    );
    assert_eq!(
        direct
            .workspace
            .as_ref()
            .map(|workspace| workspace.revision),
        Some(1)
    );
    assert_eq!(
        direct.execution.as_ref().map(|execution| execution.phase),
        Some(SessionExecutionPhase::Idle)
    );

    engine
        .begin_runtime_activity(RuntimeActivityBegin {
            activity_id: RuntimeActivityId::new("wake:directory-direct"),
            parent_activity_id: None,
            kind: RuntimeActivityKind::Wake,
            owner: RuntimeActivityOwner::RustBrain,
            agent_id: Some(direct.agent_id.clone()),
            profile_id: Some(direct.profile_id.clone()),
            session_id: Some(direct.session_id.clone()),
            wake_id: Some("directory-direct".into()),
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
    let active_direct = engine
        .list_agent_directory()
        .unwrap()
        .into_iter()
        .find(|entry| entry.agent_id == AgentId::new("direct-agent"))
        .unwrap();
    assert_eq!(active_direct.session_status, SessionStatus::Active);
    assert_eq!(
        active_direct.execution.map(|execution| execution.phase),
        Some(SessionExecutionPhase::Active)
    );

    let external = directory
        .iter()
        .find(|entry| entry.agent_id == AgentId::new("codex-agent"))
        .unwrap();
    assert_eq!(external.display_label, "Codex coder");
    assert_eq!(
        external.runtime_kind,
        AgentDirectoryRuntimeKind::CodexAppServer
    );
    assert_eq!(
        external.binding_id,
        Some(ExternalBindingId::new("codex-binding"))
    );
    assert!(external.execution.is_none());
    assert!(external.routable);

    let mut paused = saved_binding.clone();
    paused.status = ExternalBindingStatus::Paused;
    paused.updated_at = "2026-06-19T00:00:01Z".into();
    engine
        .bind_external_agent(&paused, Some(saved_binding.revision))
        .unwrap();
    let paused = engine
        .list_agent_directory()
        .unwrap()
        .into_iter()
        .find(|entry| entry.agent_id == AgentId::new("codex-agent"))
        .unwrap();
    assert!(!paused.routable);
    assert_eq!(
        paused.routability_reason_code.as_deref(),
        Some("external_binding_not_active")
    );
}
