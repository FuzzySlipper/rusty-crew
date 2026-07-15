use super::*;
use rusty_crew_core_protocol::{
    ExternalBindingId, ExternalControlId, ExternalControlKind, ExternalControlRequest,
    ExternalControlStatus, ExternalTurnInputPart, ExternalTurnPhase, ExternalTurnRequestId,
    TurnInputProvenance, TurnInputProvenanceKind,
};
use serde_json::json;

#[test]
fn interrupt_control_uses_only_rust_validated_turn_identity() {
    let engine = test_engine();
    engine
        .create_session(session_config(
            "codex-session",
            "codex-agent",
            "codex-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine
        .register_external_runtime(&external_runtime::runtime(), None)
        .unwrap();
    engine
        .bind_external_agent(&external_runtime::binding(), None)
        .unwrap();
    engine
        .activate_agent_execution(AgentActivationRequest {
            agent_id: AgentId::new("codex-agent"),
            request_id: ExternalTurnRequestId::new("interrupt-active"),
            idempotency_key: "interrupt-active-key".into(),
            input: vec![ExternalTurnInputPart::Text {
                text: "inspect the project".into(),
            }],
            collaboration_mode: None,
            provenance: TurnInputProvenance {
                kind: TurnInputProvenanceKind::Operator,
                source_id: None,
                correlation_id: None,
            },
            run_id: None,
            capacity_lease_id: "interrupt-active-capacity".into(),
            direct_wake_id: "interrupt-active-wake".into(),
            queued_message_id: "interrupt-active-queue".into(),
            created_at: "2026-06-19T00:00:00Z".into(),
            expires_at: None,
        })
        .unwrap();
    engine
        .transition_external_turn(
            &ExternalTurnRequestId::new("interrupt-active"),
            ExternalTurnPhase::Starting,
            None,
            None,
            "2026-06-19T00:00:01Z".into(),
        )
        .unwrap();
    engine
        .transition_external_turn(
            &ExternalTurnRequestId::new("interrupt-active"),
            ExternalTurnPhase::Active,
            Some("native-turn-7".into()),
            None,
            "2026-06-19T00:00:02Z".into(),
        )
        .unwrap();

    let restart_while_active = ExternalControlRequest {
        control_id: ExternalControlId::new("restart-active-thread"),
        idempotency_key: "restart-active-thread".into(),
        binding_id: ExternalBindingId::new("codex-binding"),
        expected_binding_revision: 1,
        expected_native_turn_id: None,
        kind: ExternalControlKind::ExecuteThreadCommand,
        payload: json!({"command": "new", "argument": null}),
        requested_at: "2026-06-19T00:00:02Z".into(),
    };
    let restart_error = engine
        .submit_external_control(restart_while_active)
        .unwrap_err();
    assert_eq!(restart_error.kind, CoreErrorKind::ActionRejected);
    assert!(restart_error.message.contains("idle binding"));

    let request = ExternalControlRequest {
        control_id: ExternalControlId::new("interrupt-current-turn"),
        idempotency_key: "interrupt-current-turn".into(),
        binding_id: ExternalBindingId::new("codex-binding"),
        expected_binding_revision: 1,
        expected_native_turn_id: Some("native-turn-7".into()),
        kind: ExternalControlKind::InterruptTurn,
        payload: json!({}),
        requested_at: "2026-06-19T00:00:03Z".into(),
    };
    let receipt = engine.submit_external_control(request.clone()).unwrap();
    assert_eq!(receipt.status, ExternalControlStatus::Pending);
    assert_eq!(receipt.request.payload, json!({}));

    let mut duplicate_identity = request;
    duplicate_identity.control_id = ExternalControlId::new("interrupt-duplicate-identity");
    duplicate_identity.idempotency_key = "interrupt-duplicate-identity".into();
    duplicate_identity.payload = json!({
        "threadId": "native-thread-7",
        "turnId": "native-turn-7"
    });
    let error = engine
        .submit_external_control(duplicate_identity)
        .unwrap_err();
    assert_eq!(error.kind, CoreErrorKind::InvalidInput);
    assert!(error.message.contains("payload must be empty"));
}
