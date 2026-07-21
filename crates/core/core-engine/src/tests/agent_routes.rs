use super::external_runtime::{binding, runtime};
use super::*;
use rusty_crew_core_protocol::{
    AgentActivation, AgentCoordinationCaller, AgentMessageCommand, AgentMessageDeliveryId,
    AgentMessageDeliveryStatus, AgentMessageInputKind, AgentRoundCommand, AgentRoundId,
    AgentRouteDelete, AgentRouteKey, AgentRouteTarget, AgentRouteWrite,
    ExternalAgentBindingMetadataWrite, ExternalMessageDeliveryPolicy, ExternalTurnPhase,
    ExternalTurnRequestId,
};

#[test]
fn switchboard_routes_wake_the_exact_resolved_direct_session() {
    let engine = test_engine();
    let first = engine
        .create_session(session_config(
            "shared-agent-first",
            "shared-agent",
            "shared-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let second = engine
        .create_session(session_config(
            "shared-agent-second",
            "shared-agent",
            "shared-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let agent_only_selection = engine
        .sessions
        .get_session_by_agent(&AgentId::new("shared-agent"))
        .unwrap();
    let target = if agent_only_selection.session_id == first.session_id {
        second
    } else {
        first
    };
    engine
        .put_agent_route(AgentRouteWrite {
            route_key: AgentRouteKey::new("exact-direct"),
            label: "Exact direct target".into(),
            description: None,
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
    let (_, wakes) = engine
        .subscribe_events(EventSubscription {
            event_kinds: vec![CoreEventKind::BrainWakeRequested],
            session_id: None,
            agent_id: None,
            adapter_id: None,
        })
        .unwrap();

    let receipt = engine
        .deliver_agent_message(route_message("exact-direct", "exact-direct-message"))
        .unwrap();

    assert!(matches!(
        receipt.activation,
        Some(AgentActivation::DirectBrainWakeRequested { session_id, .. })
            if session_id == target.session_id
    ));
    assert!(matches!(
        wakes.recv_timeout(Duration::from_secs(1)).unwrap(),
        CoreEvent::BrainWakeRequested { session_id } if session_id == target.session_id
    ));
}

#[test]
fn switchboard_routes_activate_the_exact_resolved_managed_session_and_binding() {
    let engine = test_engine();
    let first = engine
        .create_session(session_config(
            "shared-codex-first",
            "codex-agent",
            "codex-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let second = engine
        .create_session(session_config(
            "shared-codex-second",
            "codex-agent",
            "codex-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let agent_only_selection = engine
        .sessions
        .get_session_by_agent(&AgentId::new("codex-agent"))
        .unwrap();
    let target = if agent_only_selection.session_id == first.session_id {
        second
    } else {
        first
    };
    engine.register_external_runtime(&runtime(), None).unwrap();
    let mut exact_binding = binding();
    exact_binding.session_id = Some(target.session_id.clone());
    exact_binding.message_delivery_policy = ExternalMessageDeliveryPolicy::SerialNextTurn;
    let exact_binding = engine.bind_external_agent(&exact_binding, None).unwrap();
    engine
        .put_agent_route(AgentRouteWrite {
            route_key: AgentRouteKey::new("exact-managed"),
            label: "Exact managed target".into(),
            description: None,
            enabled: true,
            target: AgentRouteTarget::ManagedExternal {
                agent_id: target.agent_id.clone(),
                binding_id: exact_binding.binding_id.clone(),
                binding_revision: exact_binding.revision,
            },
            required_runtime_kind: Some(AgentDirectoryRuntimeKind::CodexAppServer),
            required_delivery_policy: Some(ExternalMessageDeliveryPolicy::SerialNextTurn),
            expected_revision: None,
            updated_at: "2026-06-19T00:00:00Z".into(),
        })
        .unwrap();

    let receipt = engine
        .deliver_agent_message(route_message("exact-managed", "exact-managed-message"))
        .unwrap();

    assert!(matches!(
        receipt.activation,
        Some(AgentActivation::ExternalTurnRequested {
            session_id,
            binding_id,
            ..
        }) if session_id == target.session_id && binding_id == exact_binding.binding_id
    ));
    let turn = engine
        .get_external_turn(&ExternalTurnRequestId::new(
            "agent-message:exact-managed-message",
        ))
        .unwrap()
        .unwrap();
    assert_eq!(turn.request.session_id, target.session_id);
    assert_eq!(turn.request.binding_id, exact_binding.binding_id);

    let starting = engine
        .transition_external_turn(
            &turn.request.request_id,
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
            Some("native-turn-exact-managed".into()),
            None,
            "2026-06-19T00:00:02Z".into(),
        )
        .unwrap();
    let queued = engine
        .deliver_agent_message(route_message(
            "exact-managed",
            "exact-managed-queued-message",
        ))
        .unwrap();
    assert!(matches!(
        queued.activation,
        Some(AgentActivation::QueuedForNextTurn { session_id, .. })
            if session_id == target.session_id
    ));
    engine
        .transition_external_turn(
            &active.request.request_id,
            ExternalTurnPhase::Completed,
            None,
            Some("agent_message_no_reply".into()),
            "2026-06-19T00:00:03Z".into(),
        )
        .unwrap();
    let promoted = engine
        .get_external_turn(&ExternalTurnRequestId::new(
            "external-follow-up:agent-message-queue:exact-managed-queued-message",
        ))
        .unwrap()
        .unwrap();
    assert_eq!(promoted.request.session_id, target.session_id);
    assert_eq!(promoted.request.binding_id, exact_binding.binding_id);
}

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

    let data_dir = unique_data_dir("route-first-agent-collision");
    let engine = test_engine_with_data_dir(data_dir.clone());
    let target = engine
        .create_session(session_config(
            "route-target-session",
            "route-target-agent",
            "route-target-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let route = AgentRouteWrite {
        route_key: AgentRouteKey::new("reviewer"),
        label: "Reviewer".into(),
        description: None,
        enabled: true,
        target: AgentRouteTarget::DirectBrain {
            agent_id: target.agent_id,
            session_id: target.session_id,
        },
        required_runtime_kind: Some(AgentDirectoryRuntimeKind::DirectBrain),
        required_delivery_policy: None,
        expected_revision: None,
        updated_at: "2026-06-19T00:00:00Z".into(),
    };
    engine.put_agent_route(route.clone()).unwrap();
    let error = engine
        .create_session(session_config(
            "shadowed-raw-session",
            "@reviewer",
            "shadowed-raw-profile",
            SessionKind::Full,
        ))
        .unwrap_err();
    assert_eq!(
        error.message,
        "agent_route_address_collides_with_raw_agent_id"
    );

    engine
        .store
        .delete_agent_route(&AgentRouteDelete {
            route_key: AgentRouteKey::new("reviewer"),
            expected_revision: 1,
        })
        .unwrap();
    engine
        .create_session(session_config(
            "persisted-collision-session",
            "@reviewer",
            "persisted-collision-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine.store.put_agent_route(&route).unwrap();
    drop(engine);
    let restart_error = CoreEngine::initialize(test_engine_config(data_dir)).unwrap_err();
    assert_eq!(
        restart_error.message,
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

fn route_message(route_key: &str, message_id: &str) -> AgentMessageCommand {
    AgentMessageCommand {
        caller: AgentCoordinationCaller::System {
            sender_agent_id: AgentId::new("operator"),
        },
        delivery_id: AgentMessageDeliveryId::new(format!("delivery:{message_id}")),
        idempotency_key: format!("delivery:{message_id}"),
        message_id: message_id.into(),
        to_address: format!("@{route_key}"),
        input_kind: AgentMessageInputKind::RoutedAgentMessage,
        body: "route exactly once".into(),
        collaboration_mode: None,
        correlation_id: Some(format!("correlation:{message_id}")),
        require_wake: true,
        created_at: "2026-06-19T00:00:00Z".into(),
        expires_at: "2026-06-19T00:05:00Z".into(),
    }
}
