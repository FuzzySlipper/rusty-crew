use super::external_runtime::{external_creation_request, ready_external_creation_dependencies};
use super::*;
use rusty_crew_core_protocol::{
    ExternalAgentBindingRestoreOutcome, ExternalAgentBindingRestoreRequest, ExternalBindingStatus,
    ProfileRegistryUpdate,
};

#[test]
fn archived_external_agent_restore_preserves_exact_identity_and_repairs_profile_revision() {
    let data_dir = unique_data_dir("external-binding-restore");
    let engine = test_engine_with_data_dir(data_dir.clone());
    let controller = ready_external_creation_dependencies(&engine);
    let prepared = engine
        .prepare_external_agent_session_creation(external_creation_request("restore-agent"))
        .unwrap();
    let starting = engine
        .mark_external_agent_session_native_starting(
            &controller,
            &prepared.creation_id,
            prepared.revision,
            "2026-07-27T01:00:01Z".into(),
        )
        .unwrap();
    let ready = engine
        .complete_external_agent_session_creation(
            &controller,
            &starting.creation_id,
            starting.revision,
            "native-thread-preserved".into(),
            "2026-07-27T01:00:02Z".into(),
        )
        .unwrap();
    engine.archive_session(&ready.session.session_id).unwrap();
    let archived = engine
        .get_external_binding(&ready.binding.binding_id)
        .unwrap()
        .unwrap();
    assert_eq!(archived.status, ExternalBindingStatus::Archived);

    let profile = engine
        .get_profile_registry_record(&ready.session.profile_id)
        .unwrap()
        .unwrap();
    let mut profile_write =
        profile_registry_write("codex-profile", "gpt", "configured-codex-session");
    profile_write.summary = Some("lifecycle-only edit".into());
    profile_write.now = "2026-07-27T01:00:03Z".into();
    let revised_profile = engine
        .update_profile_registry_record(&ProfileRegistryUpdate {
            write: profile_write,
            expected_revision: profile.revision,
        })
        .unwrap();

    let request = ExternalAgentBindingRestoreRequest {
        binding_id: archived.binding_id.clone(),
        expected_binding_revision: archived.revision,
        expected_session_id: ready.session.session_id.clone(),
        expected_agent_id: ready.session.agent_id.clone(),
        expected_profile_id: ready.session.profile_id.clone(),
        expected_native_thread_id: "native-thread-preserved".into(),
        restored_at: "2026-07-27T01:00:04Z".into(),
    };
    let restored = engine.restore_external_agent_binding(&request).unwrap();
    assert_eq!(
        restored.outcome,
        ExternalAgentBindingRestoreOutcome::Restored
    );
    assert!(restored.profile_revision_updated);
    assert_ne!(restored.session.status, SessionStatus::Archived);
    assert_eq!(restored.binding.status, ExternalBindingStatus::Active);
    assert_eq!(
        restored.binding.native_thread_id.as_deref(),
        Some("native-thread-preserved")
    );
    assert_eq!(
        restored.binding.profile_revision,
        Some(revised_profile.revision)
    );

    let repeated = engine
        .restore_external_agent_binding(&ExternalAgentBindingRestoreRequest {
            expected_binding_revision: restored.binding.revision,
            restored_at: "2026-07-27T01:00:05Z".into(),
            ..request
        })
        .unwrap();
    assert_eq!(
        repeated.outcome,
        ExternalAgentBindingRestoreOutcome::AlreadyActive
    );
    assert_eq!(repeated.binding.binding_id, restored.binding.binding_id);
    assert_eq!(repeated.session.session_id, restored.session.session_id);

    drop(engine);
    let restarted = test_engine_with_data_dir(data_dir);
    let persisted = restarted
        .get_external_binding(&restored.binding.binding_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        persisted.native_thread_id,
        restored.binding.native_thread_id
    );
    assert_eq!(
        restarted
            .get_session(&restored.session.session_id)
            .unwrap()
            .status,
        repeated.session.status
    );
}

#[test]
fn archived_external_agent_restore_rejects_identity_drift_and_changed_prompt() {
    let engine = test_engine();
    let controller = ready_external_creation_dependencies(&engine);
    let prepared = engine
        .prepare_external_agent_session_creation(external_creation_request("restore-conflict"))
        .unwrap();
    let starting = engine
        .mark_external_agent_session_native_starting(
            &controller,
            &prepared.creation_id,
            prepared.revision,
            "2026-07-27T02:00:01Z".into(),
        )
        .unwrap();
    let ready = engine
        .complete_external_agent_session_creation(
            &controller,
            &starting.creation_id,
            starting.revision,
            "native-thread-conflict".into(),
            "2026-07-27T02:00:02Z".into(),
        )
        .unwrap();
    engine.archive_session(&ready.session.session_id).unwrap();
    let archived = engine
        .get_external_binding(&ready.binding.binding_id)
        .unwrap()
        .unwrap();
    let base = ExternalAgentBindingRestoreRequest {
        binding_id: archived.binding_id.clone(),
        expected_binding_revision: archived.revision,
        expected_session_id: ready.session.session_id.clone(),
        expected_agent_id: ready.session.agent_id.clone(),
        expected_profile_id: ready.session.profile_id.clone(),
        expected_native_thread_id: "native-thread-conflict".into(),
        restored_at: "2026-07-27T02:00:03Z".into(),
    };
    let mut wrong_thread = base.clone();
    wrong_thread.expected_native_thread_id = "different-native-thread".into();
    assert!(engine
        .restore_external_agent_binding(&wrong_thread)
        .unwrap_err()
        .message
        .contains("external_binding_restore_identity_conflict"));

    let profile = engine
        .get_profile_registry_record(&ready.session.profile_id)
        .unwrap()
        .unwrap();
    let mut changed = profile_registry_write("codex-profile", "gpt", "configured-codex-session");
    changed.prompt_soul_markdown = Some("changed developer instructions".into());
    changed.now = "2026-07-27T02:00:04Z".into();
    engine
        .update_profile_registry_record(&ProfileRegistryUpdate {
            write: changed,
            expected_revision: profile.revision,
        })
        .unwrap();
    assert!(engine
        .restore_external_agent_binding(&base)
        .unwrap_err()
        .message
        .contains("external_binding_restore_prompt_conflict"));
}
