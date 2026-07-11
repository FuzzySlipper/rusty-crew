use super::*;
use rusty_crew_core_protocol::{
    AgentActivation, ExternalAgentBinding, ExternalBindingId, ExternalBindingPurpose,
    ExternalBindingStatus, ExternalEndpoint, ExternalEndpointTransport, ExternalProcessOwnership,
    ExternalRuntimeDesiredState, ExternalRuntimeId, ExternalRuntimeKind,
    ExternalRuntimeObservedState, ExternalRuntimeRegistration, ExternalTurnInputPart,
    ExternalTurnPhase, ExternalTurnRequestId, TurnInputProvenance, TurnInputProvenanceKind,
};

#[test]
fn external_activation_is_runtime_neutral_and_rehydrates_exact_turn() {
    let data_dir = unique_data_dir("external-runtime-activation");
    let engine = test_engine_with_data_dir(data_dir.clone());
    engine
        .create_session(session_config(
            "direct-session",
            "direct-agent",
            "direct-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine
        .create_session(session_config(
            "codex-session",
            "codex-agent",
            "codex-profile",
            SessionKind::Full,
        ))
        .unwrap();

    assert!(matches!(
        engine.activate_agent_execution(activation("direct-agent", "direct-request")),
        Ok(AgentActivation::DirectBrainWakeRequested { .. })
    ));

    engine.register_external_runtime(&runtime(), None).unwrap();
    engine.bind_external_agent(&binding(), None).unwrap();
    let activation_result = engine
        .activate_agent_execution(activation("codex-agent", "codex-request"))
        .unwrap();
    assert!(matches!(
        activation_result,
        AgentActivation::ExternalTurnRequested { .. }
    ));
    assert!(matches!(
        engine
            .activate_agent_execution(activation("codex-agent", "queued-request"))
            .unwrap(),
        AgentActivation::QueuedForNextTurn { .. }
    ));

    let starting = engine
        .transition_external_turn(
            &ExternalTurnRequestId::new("codex-request"),
            ExternalTurnPhase::Starting,
            None,
            None,
            "2026-06-19T00:00:01Z".into(),
        )
        .unwrap();
    let active = engine
        .transition_external_turn(
            &starting.request.request_id,
            ExternalTurnPhase::Active,
            Some("native-turn-7".into()),
            None,
            "2026-06-19T00:00:02Z".into(),
        )
        .unwrap();
    assert!(active.capacity_lease_id.is_some());
    let completed = engine
        .transition_external_turn(
            &active.request.request_id,
            ExternalTurnPhase::Completed,
            None,
            None,
            "2026-06-19T00:00:03Z".into(),
        )
        .unwrap();
    assert!(completed.capacity_lease_id.is_none());
    drop(engine);

    let restarted = test_engine_with_data_dir(data_dir.clone());
    let report = restarted
        .hydrate_external_runtime_lifecycle(&"2026-06-19T00:01:00Z".into())
        .unwrap();
    assert_eq!(report.runtime_count, 1);
    assert_eq!(report.binding_count, 1);
    assert!(report.driver_reconciliation_request_ids.is_empty());
    assert!(report.terminalized_request_ids.is_empty());
    let store = CoordinationStore::open(data_dir).unwrap();
    let hydrated = store
        .get_external_turn(&ExternalTurnRequestId::new("codex-request"))
        .unwrap()
        .unwrap();
    assert_eq!(hydrated.native_thread_id, "native-thread-7");
    assert_eq!(hydrated.native_turn_id.as_deref(), Some("native-turn-7"));
    assert_eq!(hydrated.phase, ExternalTurnPhase::Completed);
}

#[test]
fn restart_reconciliation_terminalizes_archived_session_without_replay() {
    let data_dir = unique_data_dir("external-runtime-reconcile");
    let engine = test_engine_with_data_dir(data_dir.clone());
    engine
        .create_session(session_config(
            "codex-session",
            "codex-agent",
            "codex-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine.register_external_runtime(&runtime(), None).unwrap();
    engine.bind_external_agent(&binding(), None).unwrap();
    engine
        .activate_agent_execution(activation("codex-agent", "codex-request"))
        .unwrap();
    engine
        .archive_session(&SessionId::new("codex-session"))
        .unwrap();
    drop(engine);

    let restarted = test_engine_with_data_dir(data_dir.clone());
    let report = restarted
        .hydrate_external_runtime_lifecycle(&"2026-06-19T00:01:00Z".into())
        .unwrap();
    assert_eq!(
        report.terminalized_request_ids,
        vec![ExternalTurnRequestId::new("codex-request")]
    );
    let store = CoordinationStore::open(data_dir).unwrap();
    let turn = store
        .get_external_turn(&ExternalTurnRequestId::new("codex-request"))
        .unwrap()
        .unwrap();
    assert_eq!(turn.phase, ExternalTurnPhase::Interrupted);
    assert_eq!(
        turn.terminal_reason_code.as_deref(),
        Some("external_session_unavailable")
    );
    assert!(turn.capacity_lease_id.is_none());
}

fn runtime() -> ExternalRuntimeRegistration {
    ExternalRuntimeRegistration {
        runtime_id: ExternalRuntimeId::new("codex-local"),
        kind: ExternalRuntimeKind::CodexAppServer,
        endpoint: ExternalEndpoint {
            transport: ExternalEndpointTransport::UnixWebSocket,
            address: "/run/user/1001/codex.sock".into(),
        },
        process_ownership: ExternalProcessOwnership::Attached,
        codex_home_ref: Some("/home/agent/.codex".into()),
        expected_cli_version: "0.144.1".into(),
        executable_sha256: "a".repeat(64),
        protocol_schema_sha256: "b".repeat(64),
        desired_state: ExternalRuntimeDesiredState::Enabled,
        observed_state: ExternalRuntimeObservedState::Ready,
        observed_reason_code: None,
        revision: 0,
        created_at: "2026-06-19T00:00:00Z".into(),
        updated_at: "2026-06-19T00:00:00Z".into(),
    }
}

fn binding() -> ExternalAgentBinding {
    ExternalAgentBinding {
        binding_id: ExternalBindingId::new("codex-binding"),
        runtime_id: ExternalRuntimeId::new("codex-local"),
        session_id: Some(SessionId::new("codex-session")),
        agent_id: Some(AgentId::new("codex-agent")),
        purpose: ExternalBindingPurpose::CrewAgent,
        native_thread_id: Some("native-thread-7".into()),
        cwd: Some("/home/dev/rusty-crew".into()),
        task_ref: None,
        effective_config_fingerprint: "config-fingerprint".into(),
        status: ExternalBindingStatus::Active,
        revision: 0,
        created_at: "2026-06-19T00:00:00Z".into(),
        updated_at: "2026-06-19T00:00:00Z".into(),
    }
}

fn activation(agent_id: &str, request_id: &str) -> ExternalActivationRequest {
    ExternalActivationRequest {
        agent_id: AgentId::new(agent_id),
        request_id: ExternalTurnRequestId::new(request_id),
        idempotency_key: format!("{request_id}-key"),
        input: vec![ExternalTurnInputPart::Text {
            text: "inspect the project".into(),
        }],
        provenance: TurnInputProvenance {
            kind: TurnInputProvenanceKind::Operator,
            source_id: None,
            correlation_id: None,
        },
        run_id: None,
        capacity_lease_id: format!("{request_id}-capacity"),
        direct_wake_id: format!("{request_id}-wake"),
        queued_message_id: format!("{request_id}-queue"),
        created_at: "2026-06-19T00:00:00Z".into(),
        expires_at: None,
    }
}
