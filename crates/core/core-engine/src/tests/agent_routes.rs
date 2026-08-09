use super::external_runtime::{binding, runtime};
use super::*;
use rusty_crew_core_protocol::{
    AgentActivation, AgentCoordinationCaller, AgentMessageCommand, AgentMessageDeliveryId,
    AgentMessageDeliveryStatus, AgentMessageInputKind, AgentMessageReplyCommand, AgentRoundCommand,
    AgentRoundId, AgentRouteDelete, AgentRouteKey, AgentRouteTarget, AgentRouteWrite,
    ExternalAgentBindingMetadataWrite, ExternalMessageDeliveryPolicy, ExternalTurnPhase,
    ExternalTurnRequestId,
};

#[test]
fn accepted_switchboard_message_reply_outlives_delivery_ttl_without_sender_route() {
    let mut engine = test_engine();
    let sender = engine
        .create_session(session_config(
            "unrouted-sender-session",
            "unrouted-sender",
            "sender-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let sender_sibling = engine
        .create_session(session_config(
            "unrouted-sender-sibling",
            "unrouted-sender",
            "sender-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let reviewer = engine
        .create_session(session_config(
            "reviewer-session",
            "reviewer-agent",
            "reviewer-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine
        .put_agent_route(AgentRouteWrite {
            route_key: AgentRouteKey::new("reviewer"),
            label: "Reviewer".into(),
            description: None,
            enabled: true,
            target: AgentRouteTarget::DirectBrain {
                agent_id: reviewer.agent_id.clone(),
                session_id: reviewer.session_id.clone(),
            },
            required_runtime_kind: Some(AgentDirectoryRuntimeKind::DirectBrain),
            required_delivery_policy: None,
            expected_revision: None,
            updated_at: "2026-06-19T00:00:00Z".into(),
        })
        .unwrap();
    assert!(engine
        .get_agent_route_resolution(&AgentRouteKey::new("unrouted-sender"))
        .unwrap()
        .is_none());

    let original = engine
        .deliver_agent_message(AgentMessageCommand {
            caller: AgentCoordinationCaller::DirectBrain {
                session_id: sender.session_id.clone(),
                wake_id: "sender-wake".into(),
                tool_call_id: "sender-call".into(),
            },
            delivery_id: AgentMessageDeliveryId::new("review-delivery"),
            idempotency_key: "review-delivery".into(),
            message_id: "review-message".into(),
            to_address: "@reviewer".into(),
            input_kind: AgentMessageInputKind::RoutedAgentMessage,
            body: "review this change".into(),
            image_attachment_ids: Vec::new(),
            collaboration_mode: None,
            correlation_id: Some("review-correlation".into()),
            require_wake: true,
            created_at: "2026-06-19T00:00:00Z".into(),
            expires_at: "2026-06-19T00:01:00Z".into(),
        })
        .unwrap();
    assert_eq!(original.status, AgentMessageDeliveryStatus::Accepted);
    assert_eq!(
        original.request.from_session_id,
        Some(sender.session_id.clone())
    );

    engine.config.clock = ClockConfig::Fixed {
        at: "2026-06-19T00:02:00Z".into(),
    };
    let reply_command = AgentMessageReplyCommand {
        caller: AgentCoordinationCaller::DirectBrain {
            session_id: reviewer.session_id.clone(),
            wake_id: "reviewer-wake".into(),
            tool_call_id: "reviewer-reply-call".into(),
        },
        delivery_id: AgentMessageDeliveryId::new("review-reply-delivery"),
        idempotency_key: "review-reply-delivery".into(),
        message_id: "review-reply-message".into(),
        in_reply_to_message_id: "review-message".into(),
        body: "review completed".into(),
        created_at: "2026-06-19T00:02:00Z".into(),
        expires_at: "2026-06-19T00:07:00Z".into(),
    };
    let reply = engine.reply_agent_message(reply_command.clone()).unwrap();
    assert_eq!(reply.status, AgentMessageDeliveryStatus::Accepted);
    assert_eq!(reply.request.requested_address, sender.agent_id.0);
    assert_eq!(reply.request.to_session_id, Some(sender.session_id.clone()));
    assert_ne!(reply.request.to_session_id, Some(sender_sibling.session_id));
    assert_eq!(
        reply.request.correlation_id.as_deref(),
        Some("review-correlation")
    );
    assert_eq!(
        reply.request.reply_to_message_id.as_deref(),
        Some("review-message")
    );
    assert!(reply.request.routing.is_none());

    engine.archive_session(&sender.session_id).unwrap();
    let archived_sender = engine.reply_agent_message(reply_command).unwrap_err();
    assert_eq!(
        archived_sender.message,
        "agent_message_reply_sender_session_changed"
    );

    let expired = engine
        .deliver_agent_message(AgentMessageCommand {
            caller: AgentCoordinationCaller::System {
                sender_agent_id: AgentId::new("expired-system-sender"),
            },
            delivery_id: AgentMessageDeliveryId::new("expired-delivery"),
            idempotency_key: "expired-delivery".into(),
            message_id: "expired-message".into(),
            to_address: "@reviewer".into(),
            input_kind: AgentMessageInputKind::RoutedAgentMessage,
            body: "stale request".into(),
            image_attachment_ids: Vec::new(),
            collaboration_mode: None,
            correlation_id: None,
            require_wake: true,
            created_at: "2026-06-19T00:00:00Z".into(),
            expires_at: "2026-06-19T00:01:00Z".into(),
        })
        .unwrap();
    assert_eq!(expired.status, AgentMessageDeliveryStatus::Expired);
    let expired_reply = engine
        .reply_agent_message(AgentMessageReplyCommand {
            caller: AgentCoordinationCaller::DirectBrain {
                session_id: reviewer.session_id,
                wake_id: "reviewer-wake-2".into(),
                tool_call_id: "reviewer-reply-call-2".into(),
            },
            delivery_id: AgentMessageDeliveryId::new("expired-reply-delivery"),
            idempotency_key: "expired-reply-delivery".into(),
            message_id: "expired-reply-message".into(),
            in_reply_to_message_id: "expired-message".into(),
            body: "must not revive".into(),
            created_at: "2026-06-19T00:02:00Z".into(),
            expires_at: "2026-06-19T00:07:00Z".into(),
        })
        .unwrap_err();
    assert_eq!(
        expired_reply.message,
        "agent_message_reply_original_not_accepted"
    );
}

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
    let ambiguity = engine
        .sessions
        .get_session_by_agent(&AgentId::new("shared-agent"))
        .unwrap_err();
    assert_eq!(ambiguity.kind, CoreErrorKind::ActionRejected);
    assert!(ambiguity.message.contains("agent_session_ambiguous"));
    assert!(ambiguity.message.contains(&first.session_id.0));
    assert!(ambiguity.message.contains(&second.session_id.0));
    let raw_resolution = engine.resolve_agent_address("shared-agent").unwrap_err();
    assert_eq!(raw_resolution.kind, CoreErrorKind::ActionRejected);
    assert_eq!(raw_resolution.message, ambiguity.message);
    let target = second;
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
    let ambiguity = engine
        .sessions
        .get_session_by_agent(&AgentId::new("codex-agent"))
        .unwrap_err();
    assert_eq!(ambiguity.kind, CoreErrorKind::ActionRejected);
    assert!(ambiguity.message.contains("agent_session_ambiguous"));
    assert!(ambiguity.message.contains(&first.session_id.0));
    assert!(ambiguity.message.contains(&second.session_id.0));
    let target = second;
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
            image_attachment_ids: Vec::new(),
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
            image_attachment_ids: Vec::new(),
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

#[test]
fn same_agent_sibling_routes_isolate_messages_and_session_event_streams() {
    let engine = test_engine();
    let first = engine
        .create_session(session_config(
            "sibling-first",
            "sibling-agent",
            "sibling-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let second = engine
        .create_session(session_config(
            "sibling-second",
            "sibling-agent",
            "sibling-profile",
            SessionKind::Full,
        ))
        .unwrap();
    for (key, session) in [("sibling-first", &first), ("sibling-second", &second)] {
        engine
            .put_agent_route(AgentRouteWrite {
                route_key: AgentRouteKey::new(key),
                label: key.into(),
                description: None,
                enabled: true,
                target: AgentRouteTarget::DirectBrain {
                    agent_id: session.agent_id.clone(),
                    session_id: session.session_id.clone(),
                },
                required_runtime_kind: Some(AgentDirectoryRuntimeKind::DirectBrain),
                required_delivery_policy: None,
                expected_revision: None,
                updated_at: "2026-06-19T00:00:00Z".into(),
            })
            .unwrap();
    }
    let (_, first_events) = engine
        .subscribe_events(EventSubscription {
            event_kinds: vec![CoreEventKind::AgentMessageRouted],
            session_id: Some(first.session_id.clone()),
            agent_id: None,
            adapter_id: None,
        })
        .unwrap();
    let (_, second_events) = engine
        .subscribe_events(EventSubscription {
            event_kinds: vec![CoreEventKind::AgentMessageRouted],
            session_id: Some(second.session_id.clone()),
            agent_id: None,
            adapter_id: None,
        })
        .unwrap();

    engine
        .deliver_agent_message(route_message("sibling-first", "sibling-message-first"))
        .unwrap();
    let CoreEvent::AgentMessageRouted {
        message: first_message,
    } = first_events.recv_timeout(Duration::from_secs(1)).unwrap()
    else {
        panic!("expected first sibling routed event");
    };
    assert_eq!(first_message.to_session_id, Some(first.session_id.clone()));
    assert!(second_events.try_recv().is_err());

    engine
        .deliver_agent_message(route_message("sibling-second", "sibling-message-second"))
        .unwrap();
    let CoreEvent::AgentMessageRouted {
        message: second_message,
    } = second_events.recv_timeout(Duration::from_secs(1)).unwrap()
    else {
        panic!("expected second sibling routed event");
    };
    assert_eq!(
        second_message.to_session_id,
        Some(second.session_id.clone())
    );
    assert!(first_events.try_recv().is_err());

    let first_body = engine.body_projector.project(&first.session_id).unwrap();
    let second_body = engine.body_projector.project(&second.session_id).unwrap();
    assert_eq!(first_body.pending_messages.len(), 1);
    assert_eq!(second_body.pending_messages.len(), 1);
    assert_eq!(
        first_body.pending_messages[0].to_session_id,
        Some(first.session_id)
    );
    assert_eq!(
        second_body.pending_messages[0].to_session_id,
        Some(second.session_id)
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
        image_attachment_ids: Vec::new(),
        collaboration_mode: None,
        correlation_id: Some(format!("correlation:{message_id}")),
        require_wake: true,
        created_at: "2026-06-19T00:00:00Z".into(),
        expires_at: "2026-06-19T00:05:00Z".into(),
    }
}
