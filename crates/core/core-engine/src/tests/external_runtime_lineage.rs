use super::*;
use rusty_crew_core_protocol::{
    ExternalAgentBindingLineage, ExternalBindingId, ExternalBindingStatus,
};

#[test]
fn external_binding_lineage_is_rust_validated_immutable_idempotent_and_restartable() {
    let data_dir = unique_data_dir("external-binding-lineage-authority");
    let engine = test_engine_with_data_dir(data_dir.clone());
    for (session_id, agent_id) in [
        ("codex-session", "codex-agent"),
        ("successor-session", "successor-agent"),
    ] {
        engine
            .create_session(session_config(
                session_id,
                agent_id,
                "codex-profile",
                SessionKind::Full,
            ))
            .unwrap();
    }
    engine
        .register_external_runtime(&external_runtime::runtime(), None)
        .unwrap();
    let predecessor = engine
        .bind_external_agent(&external_runtime::binding(), None)
        .unwrap();
    let mut successor_seed = external_runtime::binding();
    successor_seed.binding_id = ExternalBindingId::new("successor-binding");
    successor_seed.session_id = Some(SessionId::new("successor-session"));
    successor_seed.agent_id = Some(AgentId::new("successor-agent"));
    successor_seed.native_thread_id = Some("native-thread-8".into());
    let successor_seed = engine.bind_external_agent(&successor_seed, None).unwrap();
    let authoritative_lineage = ExternalAgentBindingLineage {
        predecessor_binding_id: predecessor.binding_id.clone(),
        predecessor_session_id: predecessor.session_id.clone().unwrap(),
        predecessor_native_thread_id: predecessor.native_thread_id.clone().unwrap(),
        transition_id: "transition-1".into(),
        reason_code: "explicit_new".into(),
        created_at: "2026-06-19T00:00:01Z".into(),
    };

    let mut nonexistent = successor_seed.clone();
    nonexistent.lineage = Some(ExternalAgentBindingLineage {
        predecessor_binding_id: ExternalBindingId::new("missing-binding"),
        ..authoritative_lineage.clone()
    });
    assert_eq!(
        engine
            .bind_external_agent(&nonexistent, Some(successor_seed.revision))
            .unwrap_err()
            .kind,
        CoreErrorKind::NotFound
    );

    for invalid_lineage in [
        ExternalAgentBindingLineage {
            predecessor_session_id: SessionId::new("successor-session"),
            ..authoritative_lineage.clone()
        },
        ExternalAgentBindingLineage {
            predecessor_native_thread_id: "native-thread-8".into(),
            ..authoritative_lineage.clone()
        },
    ] {
        let mut invalid = successor_seed.clone();
        invalid.lineage = Some(invalid_lineage);
        assert_eq!(
            engine
                .bind_external_agent(&invalid, Some(successor_seed.revision))
                .unwrap_err()
                .kind,
            CoreErrorKind::ActionRejected
        );
    }

    let mut missing_successor_session = successor_seed.clone();
    missing_successor_session.session_id = None;
    let mut reused_predecessor_session = successor_seed.clone();
    reused_predecessor_session.session_id = predecessor.session_id.clone();
    let mut missing_successor_thread = successor_seed.clone();
    missing_successor_thread.native_thread_id = None;
    let mut empty_successor_thread = successor_seed.clone();
    empty_successor_thread.native_thread_id = Some(String::new());
    let mut blank_successor_thread = successor_seed.clone();
    blank_successor_thread.native_thread_id = Some(" \t".to_string());
    let mut reused_predecessor_thread = successor_seed.clone();
    reused_predecessor_thread.native_thread_id = predecessor.native_thread_id.clone();
    for (mut invalid, expected_kind) in [
        (missing_successor_session, CoreErrorKind::InvalidInput),
        (reused_predecessor_session, CoreErrorKind::ActionRejected),
        (missing_successor_thread, CoreErrorKind::ActionRejected),
        (empty_successor_thread, CoreErrorKind::InvalidInput),
        (blank_successor_thread, CoreErrorKind::InvalidInput),
        (reused_predecessor_thread, CoreErrorKind::ActionRejected),
    ] {
        invalid.lineage = Some(authoritative_lineage.clone());
        assert_eq!(
            engine
                .bind_external_agent(&invalid, Some(successor_seed.revision))
                .unwrap_err()
                .kind,
            expected_kind,
            "a lineaged successor requires present and distinct session/thread identities"
        );
    }

    let mut establish = successor_seed.clone();
    establish.lineage = Some(authoritative_lineage.clone());
    establish.updated_at = "2026-06-19T00:00:01Z".into();
    let established = engine
        .bind_external_agent(&establish, Some(successor_seed.revision))
        .unwrap();
    let replay = engine
        .bind_external_agent(&establish, Some(successor_seed.revision))
        .unwrap();
    assert_eq!(replay, established, "exact stale replay is idempotent");

    let mut redirected_predecessor = predecessor.clone();
    redirected_predecessor.native_thread_id = Some("redirected-thread".into());
    assert_eq!(
        engine
            .bind_external_agent(&redirected_predecessor, Some(predecessor.revision))
            .unwrap_err()
            .kind,
        CoreErrorKind::ActionRejected
    );

    let mut removed = established.clone();
    removed.lineage = None;
    assert_eq!(
        engine
            .bind_external_agent(&removed, Some(established.revision))
            .unwrap_err()
            .kind,
        CoreErrorKind::ActionRejected
    );
    let mut overwritten = established.clone();
    overwritten.lineage.as_mut().unwrap().transition_id = "conflict".into();
    assert_eq!(
        engine
            .bind_external_agent(&overwritten, Some(established.revision))
            .unwrap_err()
            .kind,
        CoreErrorKind::ActionRejected
    );
    let mut stale = established.clone();
    stale.status = ExternalBindingStatus::Paused;
    assert_eq!(
        engine
            .bind_external_agent(&stale, Some(successor_seed.revision))
            .unwrap_err()
            .kind,
        CoreErrorKind::ActionRejected
    );

    drop(engine);
    let restarted = test_engine_with_data_dir(data_dir);
    assert_eq!(
        restarted
            .get_external_binding(&established.binding_id)
            .unwrap()
            .unwrap()
            .lineage,
        Some(authoritative_lineage)
    );
}
