use super::*;
use rusty_crew_core_protocol::{
    CrewAgentSessionCreationOutcome, CrewAgentSessionCreationRequest,
    ProfileRegistryLifecycleStatus,
};

#[test]
fn crew_session_creation_is_rust_owned_idempotent_and_restart_durable() {
    let data_dir = unique_data_dir("crew-session-creation");
    let engine = test_engine_with_data_dir(data_dir.clone());
    let template = engine
        .create_session(session_config(
            "planner-session",
            "planner-profile",
            "planner-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine.archive_session(&template.session_id).unwrap();
    let profile = engine
        .create_profile_registry_record(&profile_registry_write(
            "planner-profile",
            "tester-chat",
            "planner-session",
        ))
        .unwrap();
    let request = CrewAgentSessionCreationRequest {
        idempotency_key: "browser-create-1".to_string(),
        profile_id: ProfileId::new("planner-profile"),
        expected_profile_revision: profile.revision,
        workspace_cwd: "/home/dev/rusty-crew".to_string(),
        requested_at: "2026-06-19T00:01:00Z".to_string(),
    };

    let created = engine.create_crew_agent_session(&request).unwrap();
    assert_eq!(created.outcome, CrewAgentSessionCreationOutcome::Created);
    assert_eq!(
        created.template_session_id,
        Some(SessionId::new("planner-session"))
    );
    assert_eq!(created.session.agent_id, AgentId::new("planner-profile"));
    assert_eq!(
        created.session.profile_id,
        ProfileId::new("planner-profile")
    );
    assert_eq!(created.session.kind, SessionKind::Full);
    assert_eq!(created.session.resource_limits, template.resource_limits);
    assert_eq!(created.session.tool_profile, template.tool_profile);

    let replayed = engine.create_crew_agent_session(&request).unwrap();
    assert_eq!(replayed.outcome, CrewAgentSessionCreationOutcome::Replayed);
    assert_eq!(replayed.session.session_id, created.session.session_id);
    drop(engine);

    let restarted = test_engine_with_data_dir(data_dir);
    let replayed_after_restart = restarted.create_crew_agent_session(&request).unwrap();
    assert_eq!(
        replayed_after_restart.outcome,
        CrewAgentSessionCreationOutcome::Replayed
    );
    assert_eq!(
        replayed_after_restart.session.session_id,
        created.session.session_id
    );
}

#[test]
fn crew_session_creation_recovers_archived_idempotent_session() {
    let engine = test_engine();
    let profile = engine
        .create_profile_registry_record(&profile_registry_write(
            "recover-profile",
            "tester-chat",
            "missing-template",
        ))
        .unwrap();
    let request = CrewAgentSessionCreationRequest {
        idempotency_key: "recover-key".to_string(),
        profile_id: ProfileId::new("recover-profile"),
        expected_profile_revision: profile.revision,
        workspace_cwd: "/home/dev/rusty-crew".to_string(),
        requested_at: "2026-06-19T00:01:00Z".to_string(),
    };
    let created = engine.create_crew_agent_session(&request).unwrap();
    engine.archive_session(&created.session.session_id).unwrap();
    let mut archived_profile = engine
        .get_profile_registry_record(&ProfileId::new("recover-profile"))
        .unwrap()
        .unwrap();
    archived_profile.derived_runtime_refs[0].status = "archived".to_string();
    let archived_profile = engine
        .update_profile_registry_record(&rusty_crew_core_protocol::ProfileRegistryUpdate {
            write: ProfileRegistryWrite {
                profile_id: archived_profile.profile_id.clone(),
                lifecycle_status: archived_profile.lifecycle_status,
                display_name: archived_profile.display_name.clone(),
                summary: archived_profile.summary.clone(),
                default_session_kind: archived_profile.default_session_kind,
                agent_id: archived_profile.agent_id.clone(),
                owner_id: archived_profile.owner_id.clone(),
                prompt_soul_markdown: archived_profile.prompt_soul_markdown.clone(),
                prompt_memory_markdown: archived_profile.prompt_memory_markdown.clone(),
                active_runtime_settings_json: archived_profile.active_runtime_settings_json.clone(),
                source_asset_refs: archived_profile.source_asset_refs.clone(),
                derived_runtime_refs: archived_profile.derived_runtime_refs.clone(),
                import_export: archived_profile.import_export.clone(),
                now: "2026-06-19T00:02:00Z".to_string(),
            },
            expected_revision: archived_profile.revision,
        })
        .unwrap();

    let recovered = engine.create_crew_agent_session(&request).unwrap();
    assert_eq!(
        recovered.outcome,
        CrewAgentSessionCreationOutcome::Recovered
    );
    assert_ne!(recovered.session.status, SessionStatus::Archived);
    assert!(recovered.profile_revision > archived_profile.revision);
    let recovered_profile = engine
        .get_profile_registry_record(&ProfileId::new("recover-profile"))
        .unwrap()
        .unwrap();
    let recovered_ref = recovered_profile
        .derived_runtime_refs
        .iter()
        .find(|reference| reference.ref_id == recovered.session.session_id.0)
        .unwrap();
    assert_eq!(recovered_ref.status, "active");
}

#[test]
fn crew_session_creation_rejects_changed_intent_and_stale_profile() {
    let engine = test_engine();
    let first = engine
        .create_profile_registry_record(&profile_registry_write(
            "first-profile",
            "tester-chat",
            "first-old",
        ))
        .unwrap();
    let second = engine
        .create_profile_registry_record(&profile_registry_write(
            "second-profile",
            "tester-chat",
            "second-old",
        ))
        .unwrap();
    let first_request = CrewAgentSessionCreationRequest {
        idempotency_key: "shared-key".to_string(),
        profile_id: first.profile_id.clone(),
        expected_profile_revision: first.revision,
        workspace_cwd: "/home/dev/rusty-crew".to_string(),
        requested_at: "2026-06-19T00:01:00Z".to_string(),
    };
    engine.create_crew_agent_session(&first_request).unwrap();

    let changed_intent = engine
        .create_crew_agent_session(&CrewAgentSessionCreationRequest {
            idempotency_key: "shared-key".to_string(),
            profile_id: second.profile_id.clone(),
            expected_profile_revision: second.revision,
            workspace_cwd: "/home/dev/second".to_string(),
            requested_at: "2026-06-19T00:01:01Z".to_string(),
        })
        .unwrap_err();
    assert!(changed_intent
        .message
        .contains("crew_agent_session_creation_idempotency_conflict"));

    let stale = engine
        .create_crew_agent_session(&CrewAgentSessionCreationRequest {
            idempotency_key: "stale-key".to_string(),
            profile_id: second.profile_id.clone(),
            expected_profile_revision: second.revision + 1,
            workspace_cwd: "/home/dev/second".to_string(),
            requested_at: "2026-06-19T00:01:02Z".to_string(),
        })
        .unwrap_err();
    assert!(stale
        .message
        .contains("crew_agent_session_creation_profile_revision_conflict"));
}

#[test]
fn same_profile_sessions_are_independent_ambiguous_by_agent_and_restart_durable() {
    let data_dir = unique_data_dir("same-profile-sessions");
    let engine = test_engine_with_data_dir(data_dir.clone());
    let profile = engine
        .create_profile_registry_record(&profile_registry_write(
            "shared-profile",
            "tester-chat",
            "missing-template",
        ))
        .unwrap();
    let first = engine
        .create_crew_agent_session(&CrewAgentSessionCreationRequest {
            idempotency_key: "shared-profile-first".to_string(),
            profile_id: profile.profile_id.clone(),
            expected_profile_revision: profile.revision,
            workspace_cwd: "/home/dev/workspace-a".to_string(),
            requested_at: "2026-06-19T00:01:00Z".to_string(),
        })
        .unwrap();
    let second = engine
        .create_crew_agent_session(&CrewAgentSessionCreationRequest {
            idempotency_key: "shared-profile-second".to_string(),
            profile_id: profile.profile_id.clone(),
            expected_profile_revision: first.profile_revision,
            workspace_cwd: "/home/dev/workspace-b".to_string(),
            requested_at: "2026-06-19T00:02:00Z".to_string(),
        })
        .unwrap();

    assert_ne!(first.session.session_id, second.session.session_id);
    assert_eq!(first.session.agent_id, second.session.agent_id);
    assert_eq!(first.session.profile_id, second.session.profile_id);
    assert_eq!(
        first.session.workspace.as_ref().unwrap().cwd,
        "/home/dev/workspace-a"
    );
    assert_eq!(
        second.session.workspace.as_ref().unwrap().cwd,
        "/home/dev/workspace-b"
    );
    let profile = engine
        .get_profile_registry_record(&profile.profile_id)
        .unwrap()
        .unwrap();
    let active_refs = profile
        .derived_runtime_refs
        .iter()
        .filter(|reference| reference.ref_kind == "session" && reference.status == "active")
        .map(|reference| reference.ref_id.as_str())
        .collect::<Vec<_>>();
    assert!(active_refs.contains(&first.session.session_id.0.as_str()));
    assert!(active_refs.contains(&second.session.session_id.0.as_str()));

    let ambiguous = engine
        .sessions
        .get_session_by_agent(&first.session.agent_id)
        .unwrap_err();
    assert_eq!(ambiguous.kind, CoreErrorKind::ActionRejected);
    assert!(ambiguous.message.contains("agent_session_ambiguous"));
    assert!(ambiguous.message.contains(&first.session.session_id.0));
    assert!(ambiguous.message.contains(&second.session.session_id.0));

    engine.archive_session(&first.session.session_id).unwrap();
    assert_eq!(
        engine
            .sessions
            .get_session_by_agent(&second.session.agent_id)
            .unwrap()
            .session_id,
        second.session.session_id
    );
    drop(engine);

    let restarted = test_engine_with_data_dir(data_dir);
    assert_eq!(
        restarted
            .sessions
            .get_session(&first.session.session_id)
            .unwrap()
            .status,
        SessionStatus::Archived
    );
    assert_eq!(
        restarted
            .sessions
            .get_session_by_agent(&second.session.agent_id)
            .unwrap()
            .session_id,
        second.session.session_id
    );
}

#[test]
fn crew_session_creation_rejects_inactive_profiles() {
    let engine = test_engine();
    let mut write = profile_registry_write("paused-profile", "tester-chat", "paused-old");
    write.lifecycle_status = ProfileRegistryLifecycleStatus::Paused;
    let profile = engine.create_profile_registry_record(&write).unwrap();
    let error = engine
        .create_crew_agent_session(&CrewAgentSessionCreationRequest {
            idempotency_key: "paused-key".to_string(),
            profile_id: profile.profile_id,
            expected_profile_revision: profile.revision,
            workspace_cwd: "/home/dev/rusty-crew".to_string(),
            requested_at: "2026-06-19T00:01:00Z".to_string(),
        })
        .unwrap_err();
    assert!(error
        .message
        .contains("crew_agent_session_creation_profile_inactive"));
}
