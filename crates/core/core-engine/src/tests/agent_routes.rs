use super::external_runtime::{binding, runtime};
use super::*;
use rusty_crew_core_protocol::{
    AgentCoordinationCaller, AgentMessageCommand, AgentMessageDeliveryId,
    AgentMessageDeliveryStatus, AgentMessageInputKind, AgentRoundCommand, AgentRoundId,
    AgentRouteKey, AgentRouteTarget, AgentRouteWrite, ExternalAgentBindingMetadataWrite,
    ExternalMessageDeliveryPolicy,
};

#[test]
fn switchboard_routes_resolve_exact_sessions_and_persist_delivery_provenance() {
    let data_dir = unique_data_dir("agent-switchboard-restart");
    let engine = test_engine_with_data_dir(data_dir.clone());
    let target = engine
        .create_session(session_config(
            "review-session",
            "review-agent",
            "review-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let route = engine
        .put_agent_route(AgentRouteWrite {
            route_key: AgentRouteKey::new("reviewer"),
            label: "Reviewer".into(),
            description: Some("Stable serial review destination".into()),
            enabled: true,
            target: AgentRouteTarget::DirectBrain {
                agent_id: target.agent_id.clone(),
                session_id: target.session_id.clone(),
            },
            required_runtime_kind: Some(AgentDirectoryRuntimeKind::DirectBrain),
            required_delivery_policy: None,
            expected_revision: None,
            updated_at: "2026-06-19T00:00:00Z".into(),
        })
        .unwrap();
    drop(engine);
    let engine = test_engine_with_data_dir(data_dir);

    let resolution = engine.resolve_agent_address("@reviewer").unwrap();
    assert!(resolution.routable);
    assert_eq!(
        resolution.resolved_target.unwrap().session_id,
        target.session_id
    );
    let receipt = engine
        .deliver_agent_message(AgentMessageCommand {
            caller: AgentCoordinationCaller::System {
                sender_agent_id: AgentId::new("operator"),
            },
            delivery_id: AgentMessageDeliveryId::new("route-delivery"),
            idempotency_key: "route-delivery".into(),
            message_id: "route-message".into(),
            to_address: "@reviewer".into(),
            input_kind: AgentMessageInputKind::RoutedAgentMessage,
            body: "review this".into(),
            collaboration_mode: None,
            correlation_id: Some("route-proof".into()),
            require_wake: false,
            created_at: "2026-06-19T00:00:00Z".into(),
            expires_at: "2026-06-19T00:05:00Z".into(),
        })
        .unwrap();
    assert_eq!(receipt.status, AgentMessageDeliveryStatus::Accepted);
    assert_eq!(receipt.request.requested_address, "@reviewer");
    assert_eq!(
        receipt.request.to_session_id,
        Some(target.session_id.clone())
    );
    let provenance = receipt.request.routing.unwrap();
    assert_eq!(provenance.route_key, route.route_key);
    assert_eq!(provenance.route_revision, route.revision);
    assert_eq!(provenance.resolved_target.session_id, target.session_id);
    let last_delivery = engine
        .get_agent_route_resolution(&AgentRouteKey::new("reviewer"))
        .unwrap()
        .unwrap()
        .last_delivery
        .unwrap();
    assert_eq!(last_delivery.delivery_id.0, "route-delivery");
    assert_eq!(last_delivery.route_revision, route.revision);
    assert_eq!(last_delivery.status, AgentMessageDeliveryStatus::Accepted);

    let started = engine
        .begin_agent_round(AgentRoundCommand {
            caller: AgentCoordinationCaller::System {
                sender_agent_id: AgentId::new("operator"),
            },
            round_id: AgentRoundId::new("route-round"),
            idempotency_key: "route-round".into(),
            message_id: "route-round-message".into(),
            to_address: "@reviewer".into(),
            body: "reply once".into(),
            correlation_id: "route-round-proof".into(),
            created_at: "2026-06-19T00:00:00Z".into(),
            expires_at: "2026-06-19T00:05:00Z".into(),
        })
        .unwrap();
    assert_eq!(started.round.recipient_session_id, target.session_id);
    assert_eq!(
        started
            .delivery
            .request
            .routing
            .as_ref()
            .unwrap()
            .route_revision,
        route.revision
    );

    engine.archive_session(&target.session_id).unwrap();
    let stale = engine
        .deliver_agent_message(AgentMessageCommand {
            caller: AgentCoordinationCaller::System {
                sender_agent_id: AgentId::new("operator"),
            },
            delivery_id: AgentMessageDeliveryId::new("stale-route-delivery"),
            idempotency_key: "stale-route-delivery".into(),
            message_id: "stale-route-message".into(),
            to_address: "@reviewer".into(),
            input_kind: AgentMessageInputKind::RoutedAgentMessage,
            body: "do not reroute".into(),
            collaboration_mode: None,
            correlation_id: None,
            require_wake: true,
            created_at: "2026-06-19T00:00:00Z".into(),
            expires_at: "2026-06-19T00:05:00Z".into(),
        })
        .unwrap();
    assert_eq!(stale.status, AgentMessageDeliveryStatus::Rejected);
    assert_eq!(
        stale.reason_code.as_deref(),
        Some("agent_route_target_archived")
    );
}

#[test]
fn switchboard_route_addresses_cannot_collide_with_raw_agent_ids() {
    let engine = test_engine();
    engine
        .create_session(session_config(
            "raw-alias-session",
            "@reviewer",
            "raw-alias-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let error = engine
        .put_agent_route(AgentRouteWrite {
            route_key: AgentRouteKey::new("reviewer"),
            label: "Reviewer".into(),
            description: None,
            enabled: true,
            target: AgentRouteTarget::DirectBrain {
                agent_id: AgentId::new("@reviewer"),
                session_id: SessionId::new("raw-alias-session"),
            },
            required_runtime_kind: Some(AgentDirectoryRuntimeKind::DirectBrain),
            required_delivery_policy: None,
            expected_revision: None,
            updated_at: "2026-06-19T00:00:00Z".into(),
        })
        .unwrap_err();
    assert_eq!(
        error.message,
        "agent_route_address_collides_with_raw_agent_id"
    );
}

#[test]
fn managed_switchboard_routes_fail_closed_on_policy_and_binding_revision_drift() {
    let engine = test_engine();
    engine
        .create_session(session_config(
            "codex-session",
            "codex-agent",
            "codex-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine.register_external_runtime(&runtime(), None).unwrap();
    let binding = engine.bind_external_agent(&binding(), None).unwrap();
    let route_write = |route_key: &str, required_delivery_policy| AgentRouteWrite {
        route_key: AgentRouteKey::new(route_key),
        label: "Managed reviewer".into(),
        description: None,
        enabled: true,
        target: AgentRouteTarget::ManagedExternal {
            agent_id: AgentId::new("codex-agent"),
            binding_id: binding.binding_id.clone(),
            binding_revision: binding.revision,
        },
        required_runtime_kind: Some(AgentDirectoryRuntimeKind::CodexAppServer),
        required_delivery_policy,
        expected_revision: None,
        updated_at: "2026-06-19T00:00:00Z".into(),
    };
    engine
        .put_agent_route(route_write(
            "serial-reviewer",
            Some(ExternalMessageDeliveryPolicy::SerialNextTurn),
        ))
        .unwrap();
    let policy_mismatch = engine.resolve_agent_address("@serial-reviewer").unwrap();
    assert!(!policy_mismatch.routable);
    assert_eq!(
        policy_mismatch.reason_code.as_deref(),
        Some("agent_route_delivery_policy_mismatch")
    );

    engine
        .put_agent_route(route_write(
            "codex-reviewer",
            Some(ExternalMessageDeliveryPolicy::ImmediateSteer),
        ))
        .unwrap();
    assert!(
        engine
            .resolve_agent_address("@codex-reviewer")
            .unwrap()
            .routable
    );
    engine
        .update_external_binding_metadata(&ExternalAgentBindingMetadataWrite {
            binding_id: binding.binding_id,
            expected_revision: binding.revision,
            label: Some("new metadata revision".into()),
            task_ref: None,
            updated_at: "2026-06-19T00:01:00Z".into(),
        })
        .unwrap();
    let stale = engine.resolve_agent_address("@codex-reviewer").unwrap();
    assert!(!stale.routable);
    assert_eq!(
        stale.reason_code.as_deref(),
        Some("agent_route_external_binding_replaced")
    );
}
