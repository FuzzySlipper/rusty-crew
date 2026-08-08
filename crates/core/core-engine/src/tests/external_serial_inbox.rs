use super::external_runtime::{binding, runtime};
use super::*;
use rusty_crew_core_protocol::{
    AgentActivation, AgentCoordinationCaller, AgentMessageCommand, AgentMessageDeliveryId,
    AgentMessageDeliveryStatus, AgentMessageInputKind, AgentMessageReplyCommand, ExternalBindingId,
    ExternalControllerLease, ExternalMessageDeliveryPolicy, ExternalRuntimeId, ExternalTurnPhase,
    ExternalTurnRequestId,
};

#[test]
fn serial_operator_input_promotes_without_agent_reply_and_stays_plain() {
    let engine = test_engine();
    let recipient = engine
        .create_session(session_config(
            "operator-recipient-session",
            "codex-agent",
            "codex-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine.register_external_runtime(&runtime(), None).unwrap();
    let mut serial_binding = binding();
    serial_binding.session_id = Some(recipient.session_id.clone());
    serial_binding.message_delivery_policy = ExternalMessageDeliveryPolicy::SerialNextTurn;
    engine.bind_external_agent(&serial_binding, None).unwrap();

    let deliver = |index: u8, body: &str| {
        engine
            .deliver_agent_message(AgentMessageCommand {
                caller: AgentCoordinationCaller::System {
                    sender_agent_id: AgentId::new("rusty-view-operator"),
                },
                delivery_id: AgentMessageDeliveryId::new(format!("operator-delivery-{index}")),
                idempotency_key: format!("operator-delivery-{index}"),
                message_id: format!("operator-message-{index}"),
                to_address: recipient.agent_id.0.clone(),
                input_kind: AgentMessageInputKind::Operator,
                body: body.into(),
                collaboration_mode: None,
                correlation_id: None,
                require_wake: true,
                created_at: format!("2026-06-19T00:00:0{index}Z"),
                expires_at: "2026-06-19T00:30:00Z".into(),
            })
            .unwrap()
    };

    let first = deliver(1, "first operator prompt");
    assert!(matches!(
        first.activation,
        Some(AgentActivation::ExternalTurnRequested { .. })
    ));
    let first_request = ExternalTurnRequestId::new("agent-message:operator-message-1");
    engine
        .transition_external_turn(
            &first_request,
            ExternalTurnPhase::Starting,
            None,
            None,
            "2026-06-19T00:00:01Z".into(),
        )
        .unwrap();
    engine
        .transition_external_turn(
            &first_request,
            ExternalTurnPhase::Active,
            Some("native-operator-turn-1".into()),
            None,
            "2026-06-19T00:00:02Z".into(),
        )
        .unwrap();

    let second = deliver(2, "second operator prompt");
    assert!(matches!(
        second.activation,
        Some(AgentActivation::QueuedForNextTurn { .. })
    ));
    engine
        .transition_external_turn(
            &first_request,
            ExternalTurnPhase::Completed,
            None,
            None,
            "2026-06-19T00:00:03Z".into(),
        )
        .unwrap();

    let promoted = engine
        .get_external_turn(&ExternalTurnRequestId::new(
            "external-follow-up:agent-message-queue:operator-message-2",
        ))
        .unwrap()
        .expect("operator follow-up should promote without an agent reply receipt");
    assert_eq!(
        promoted.request.provenance.kind,
        rusty_crew_core_protocol::TurnInputProvenanceKind::Operator
    );
    assert_eq!(
        promoted.request.input,
        vec![rusty_crew_core_protocol::ExternalTurnInputPart::Text {
            text: "second operator prompt".into(),
        }]
    );
}

#[test]
fn serial_routed_input_without_replyable_sender_promotes_after_completion() {
    let engine = test_engine();
    let reviewer = engine
        .create_session(session_config(
            "unreplyable-reviewer-session",
            "codex-agent",
            "codex-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine.register_external_runtime(&runtime(), None).unwrap();
    let mut serial_binding = binding();
    serial_binding.session_id = Some(reviewer.session_id.clone());
    serial_binding.message_delivery_policy = ExternalMessageDeliveryPolicy::SerialNextTurn;
    engine.bind_external_agent(&serial_binding, None).unwrap();

    for index in 1..=2 {
        engine
            .deliver_agent_message(AgentMessageCommand {
                caller: AgentCoordinationCaller::System {
                    sender_agent_id: AgentId::new("external-cli"),
                },
                delivery_id: AgentMessageDeliveryId::new(format!("unreplyable-delivery-{index}")),
                idempotency_key: format!("unreplyable-delivery-{index}"),
                message_id: format!("unreplyable-message-{index}"),
                to_address: reviewer.agent_id.0.clone(),
                input_kind: AgentMessageInputKind::RoutedAgentMessage,
                body: format!("managed review {index}"),
                collaboration_mode: None,
                correlation_id: Some(format!("unreplyable-review-{index}")),
                require_wake: true,
                created_at: format!("2026-06-19T00:00:0{index}Z"),
                expires_at: "2026-06-19T00:30:00Z".into(),
            })
            .unwrap();
    }

    let first_request = ExternalTurnRequestId::new("agent-message:unreplyable-message-1");
    engine
        .transition_external_turn(
            &first_request,
            ExternalTurnPhase::Starting,
            None,
            None,
            "2026-06-19T00:00:03Z".into(),
        )
        .unwrap();
    engine
        .transition_external_turn(
            &first_request,
            ExternalTurnPhase::Active,
            Some("native-unreplyable-turn".into()),
            None,
            "2026-06-19T00:00:04Z".into(),
        )
        .unwrap();
    engine
        .transition_external_turn(
            &first_request,
            ExternalTurnPhase::Completed,
            None,
            None,
            "2026-06-19T00:00:05Z".into(),
        )
        .unwrap();

    assert!(engine
        .get_external_turn(&ExternalTurnRequestId::new(
            "external-follow-up:agent-message-queue:unreplyable-message-2"
        ))
        .unwrap()
        .is_some());
}

#[test]
fn restart_promotes_legacy_unreplyable_serial_follow_up() {
    let data_dir = unique_data_dir("serial-legacy-unreplyable-restart");
    let engine = test_engine_with_data_dir(data_dir.clone());
    let reviewer = engine
        .create_session(session_config(
            "legacy-reviewer-session",
            "codex-agent",
            "codex-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine.register_external_runtime(&runtime(), None).unwrap();
    let mut serial_binding = binding();
    serial_binding.session_id = Some(reviewer.session_id.clone());
    serial_binding.message_delivery_policy = ExternalMessageDeliveryPolicy::SerialNextTurn;
    engine.bind_external_agent(&serial_binding, None).unwrap();

    engine
        .deliver_agent_message(AgentMessageCommand {
            caller: AgentCoordinationCaller::System {
                sender_agent_id: AgentId::new("external-cli"),
            },
            delivery_id: AgentMessageDeliveryId::new("legacy-delivery-1"),
            idempotency_key: "legacy-delivery-1".into(),
            message_id: "legacy-message-1".into(),
            to_address: reviewer.agent_id.0.clone(),
            input_kind: AgentMessageInputKind::RoutedAgentMessage,
            body: "legacy managed review 1".into(),
            collaboration_mode: None,
            correlation_id: Some("legacy-review-1".into()),
            require_wake: true,
            created_at: "2026-06-19T00:00:01Z".into(),
            expires_at: "2026-06-19T00:30:00Z".into(),
        })
        .unwrap();
    let first_request = ExternalTurnRequestId::new("agent-message:legacy-message-1");
    engine
        .transition_external_turn(
            &first_request,
            ExternalTurnPhase::Starting,
            None,
            None,
            "2026-06-19T00:00:03Z".into(),
        )
        .unwrap();
    engine
        .transition_external_turn(
            &first_request,
            ExternalTurnPhase::Active,
            Some("native-legacy-turn".into()),
            None,
            "2026-06-19T00:00:04Z".into(),
        )
        .unwrap();
    engine
        .transition_external_turn(
            &first_request,
            ExternalTurnPhase::Completed,
            None,
            None,
            "2026-06-19T00:00:05Z".into(),
        )
        .unwrap();
    engine
        .store
        .save_queued_message(&QueuedMessageRecord {
            message_id: "agent-message-queue:legacy-message-2".into(),
            owner_session_id: Some(reviewer.session_id.clone()),
            owner_agent_id: reviewer.agent_id.clone(),
            message: AgentMessage {
                from: AgentId::new("external-cli"),
                to: reviewer.agent_id,
                from_session_id: None,
                to_session_id: None,
                body: "legacy managed review 2".into(),
                correlation_id: Some("legacy-review-2".into()),
                projection: None,
            },
            source_sequence: None,
            enqueued_at: "2026-06-19T00:00:06Z".into(),
            expires_at: "2026-06-19T00:30:00Z".into(),
            ttl_ms: 1_800_000,
            delivery_attempts: 0,
            state: QueuedMessageState::Pending,
            terminal_at: None,
            state_reason: None,
        })
        .unwrap();
    drop(engine);

    let mut restart_config = test_engine_config(data_dir);
    restart_config.clock = rusty_crew_core_config::ClockConfig::Fixed {
        at: "2026-06-19T00:01:00Z".into(),
    };
    let restarted = CoreEngine::initialize(restart_config).unwrap();
    assert!(restarted
        .get_external_turn(&ExternalTurnRequestId::new(
            "external-follow-up:agent-message-queue:legacy-message-2"
        ))
        .unwrap()
        .is_some());
}

#[test]
fn restart_preserves_reply_capable_serial_wait() {
    let data_dir = unique_data_dir("serial-reply-wait-restart");
    let engine = test_engine_with_data_dir(data_dir.clone());
    let sender = engine
        .create_session(session_config(
            "restart-wait-sender-session",
            "restart-wait-sender",
            "restart-wait-sender-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let reviewer = engine
        .create_session(session_config(
            "restart-wait-reviewer-session",
            "codex-agent",
            "codex-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine.register_external_runtime(&runtime(), None).unwrap();
    let mut serial_binding = binding();
    serial_binding.session_id = Some(reviewer.session_id.clone());
    serial_binding.message_delivery_policy = ExternalMessageDeliveryPolicy::SerialNextTurn;
    engine.bind_external_agent(&serial_binding, None).unwrap();

    for index in 1..=2 {
        engine
            .deliver_agent_message(AgentMessageCommand {
                caller: AgentCoordinationCaller::DirectBrain {
                    session_id: sender.session_id.clone(),
                    wake_id: format!("restart-wait-wake-{index}"),
                    tool_call_id: format!("restart-wait-call-{index}"),
                },
                delivery_id: AgentMessageDeliveryId::new(format!("restart-wait-delivery-{index}")),
                idempotency_key: format!("restart-wait-delivery-{index}"),
                message_id: format!("restart-wait-message-{index}"),
                to_address: reviewer.agent_id.0.clone(),
                input_kind: AgentMessageInputKind::RoutedAgentMessage,
                body: format!("reply-capable review {index}"),
                collaboration_mode: None,
                correlation_id: None,
                require_wake: true,
                created_at: format!("2026-06-19T00:00:0{index}Z"),
                expires_at: "2026-06-19T00:30:00Z".into(),
            })
            .unwrap();
    }
    let first_request = ExternalTurnRequestId::new("agent-message:restart-wait-message-1");
    engine
        .transition_external_turn(
            &first_request,
            ExternalTurnPhase::Starting,
            None,
            None,
            "2026-06-19T00:00:03Z".into(),
        )
        .unwrap();
    engine
        .transition_external_turn(
            &first_request,
            ExternalTurnPhase::Active,
            Some("native-restart-wait-turn".into()),
            None,
            "2026-06-19T00:00:04Z".into(),
        )
        .unwrap();
    engine
        .transition_external_turn(
            &first_request,
            ExternalTurnPhase::Completed,
            None,
            None,
            "2026-06-19T00:00:05Z".into(),
        )
        .unwrap();
    drop(engine);

    let mut restart_config = test_engine_config(data_dir);
    restart_config.clock = rusty_crew_core_config::ClockConfig::Fixed {
        at: "2026-06-19T00:01:00Z".into(),
    };
    let restarted = CoreEngine::initialize(restart_config).unwrap();
    assert!(restarted
        .get_external_turn(&ExternalTurnRequestId::new(
            "external-follow-up:agent-message-queue:restart-wait-message-2"
        ))
        .unwrap()
        .is_none());
}

#[test]
fn restart_never_double_dispatches_behind_an_active_successor() {
    let data_dir = unique_data_dir("serial-active-successor-restart");
    let engine = test_engine_with_data_dir(data_dir.clone());
    let reviewer = engine
        .create_session(session_config(
            "active-successor-reviewer-session",
            "codex-agent",
            "codex-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine.register_external_runtime(&runtime(), None).unwrap();
    let mut serial_binding = binding();
    serial_binding.session_id = Some(reviewer.session_id.clone());
    serial_binding.message_delivery_policy = ExternalMessageDeliveryPolicy::SerialNextTurn;
    engine.bind_external_agent(&serial_binding, None).unwrap();

    for index in 1..=2 {
        engine
            .deliver_agent_message(AgentMessageCommand {
                caller: AgentCoordinationCaller::System {
                    sender_agent_id: AgentId::new("external-cli"),
                },
                delivery_id: AgentMessageDeliveryId::new(format!(
                    "active-successor-delivery-{index}"
                )),
                idempotency_key: format!("active-successor-delivery-{index}"),
                message_id: format!("active-successor-message-{index}"),
                to_address: reviewer.agent_id.0.clone(),
                input_kind: AgentMessageInputKind::RoutedAgentMessage,
                body: format!("active successor review {index}"),
                collaboration_mode: None,
                correlation_id: None,
                require_wake: true,
                created_at: format!("2026-06-19T00:00:0{index}Z"),
                expires_at: "2026-06-19T00:30:00Z".into(),
            })
            .unwrap();
    }
    let first_request = ExternalTurnRequestId::new("agent-message:active-successor-message-1");
    engine
        .transition_external_turn(
            &first_request,
            ExternalTurnPhase::Starting,
            None,
            None,
            "2026-06-19T00:00:03Z".into(),
        )
        .unwrap();
    engine
        .transition_external_turn(
            &first_request,
            ExternalTurnPhase::Active,
            Some("native-first-successor".into()),
            None,
            "2026-06-19T00:00:04Z".into(),
        )
        .unwrap();
    engine
        .transition_external_turn(
            &first_request,
            ExternalTurnPhase::Completed,
            None,
            None,
            "2026-06-19T00:00:05Z".into(),
        )
        .unwrap();
    let second_request = ExternalTurnRequestId::new(
        "external-follow-up:agent-message-queue:active-successor-message-2",
    );
    engine
        .transition_external_turn(
            &second_request,
            ExternalTurnPhase::Starting,
            None,
            None,
            "2026-06-19T00:00:06Z".into(),
        )
        .unwrap();
    engine
        .transition_external_turn(
            &second_request,
            ExternalTurnPhase::Active,
            Some("native-active-successor".into()),
            None,
            "2026-06-19T00:00:07Z".into(),
        )
        .unwrap();
    engine
        .deliver_agent_message(AgentMessageCommand {
            caller: AgentCoordinationCaller::System {
                sender_agent_id: AgentId::new("external-cli"),
            },
            delivery_id: AgentMessageDeliveryId::new("active-successor-delivery-3"),
            idempotency_key: "active-successor-delivery-3".into(),
            message_id: "active-successor-message-3".into(),
            to_address: reviewer.agent_id.0.clone(),
            input_kind: AgentMessageInputKind::RoutedAgentMessage,
            body: "queued behind active successor".into(),
            collaboration_mode: None,
            correlation_id: None,
            require_wake: true,
            created_at: "2026-06-19T00:00:08Z".into(),
            expires_at: "2026-06-19T00:30:00Z".into(),
        })
        .unwrap();
    drop(engine);

    let mut restart_config = test_engine_config(data_dir);
    restart_config.clock = rusty_crew_core_config::ClockConfig::Fixed {
        at: "2026-06-19T00:01:00Z".into(),
    };
    let restarted = CoreEngine::initialize(restart_config).unwrap();
    assert_eq!(
        restarted
            .get_external_turn(&second_request)
            .unwrap()
            .unwrap()
            .phase,
        ExternalTurnPhase::Active
    );
    assert!(restarted
        .get_external_turn(&ExternalTurnRequestId::new(
            "external-follow-up:agent-message-queue:active-successor-message-3"
        ))
        .unwrap()
        .is_none());
}

#[test]
fn serial_external_inbox_preserves_fifo_expiry_and_reply_identity() {
    let engine = test_engine();
    let sender = engine
        .create_session(session_config(
            "serial-sender-session",
            "serial-sender",
            "serial-sender-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let reviewer = engine
        .create_session(session_config(
            "serial-reviewer-session",
            "codex-agent",
            "codex-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine.register_external_runtime(&runtime(), None).unwrap();
    let mut serial_binding = binding();
    serial_binding.session_id = Some(reviewer.session_id.clone());
    serial_binding.message_delivery_policy = ExternalMessageDeliveryPolicy::SerialNextTurn;
    engine.bind_external_agent(&serial_binding, None).unwrap();
    let lease = engine
        .acquire_external_runtime_controller(
            &ExternalControllerLease {
                runtime_id: ExternalRuntimeId::new("codex-local"),
                holder_instance_id: "serial-controller".into(),
                generation: 0,
                acquired_at: "2026-06-19T00:00:00Z".into(),
                renewed_at: "2026-06-19T00:00:00Z".into(),
                expires_at: "2026-06-19T01:00:00Z".into(),
                revision: 0,
            },
            &"2026-06-19T00:00:00Z".into(),
        )
        .unwrap();

    let first = engine
        .deliver_agent_message(AgentMessageCommand {
            caller: AgentCoordinationCaller::DirectBrain {
                session_id: sender.session_id.clone(),
                wake_id: "sender-wake-1".into(),
                tool_call_id: "sender-call-1".into(),
            },
            delivery_id: AgentMessageDeliveryId::new("serial-delivery-1"),
            idempotency_key: "serial-delivery-1".into(),
            message_id: "serial-message-1".into(),
            to_address: reviewer.agent_id.0.clone(),
            input_kind: AgentMessageInputKind::RoutedAgentMessage,
            body: "review the first change".into(),
            collaboration_mode: None,
            correlation_id: Some("serial-correlation-1".into()),
            require_wake: true,
            created_at: "2026-06-19T00:00:00Z".into(),
            expires_at: "2026-06-19T00:30:00Z".into(),
        })
        .unwrap();
    assert_eq!(first.status, AgentMessageDeliveryStatus::Accepted);
    assert_eq!(
        first.request.from_session_id,
        Some(sender.session_id.clone())
    );
    assert_eq!(
        first.request.to_session_id,
        Some(reviewer.session_id.clone())
    );
    assert!(matches!(
        first.activation,
        Some(AgentActivation::ExternalTurnRequested { .. })
    ));
    let first_request = ExternalTurnRequestId::new("agent-message:serial-message-1");
    engine
        .transition_external_turn(
            &first_request,
            ExternalTurnPhase::Starting,
            None,
            None,
            "2026-06-19T00:00:01Z".into(),
        )
        .unwrap();
    engine
        .transition_external_turn(
            &first_request,
            ExternalTurnPhase::Active,
            Some("native-review-turn-1".into()),
            None,
            "2026-06-19T00:00:02Z".into(),
        )
        .unwrap();

    let reply_command = AgentMessageReplyCommand {
        caller: AgentCoordinationCaller::ExternalAgent {
            runtime_id: ExternalRuntimeId::new("codex-local"),
            binding_id: ExternalBindingId::new("codex-binding"),
            controller_instance_id: "serial-controller".into(),
            controller_generation: lease.generation,
            native_thread_id: "native-thread-7".into(),
            native_turn_id: "native-review-turn-1".into(),
            native_request_id: "reply-call-1".into(),
        },
        delivery_id: AgentMessageDeliveryId::new("serial-reply-delivery-1"),
        idempotency_key: "serial-reply-delivery-1".into(),
        message_id: "serial-reply-message-1".into(),
        in_reply_to_message_id: "serial-message-1".into(),
        body: "the first change passes review".into(),
        created_at: "2026-06-19T00:00:03Z".into(),
        expires_at: "2026-06-19T00:05:03Z".into(),
    };
    let reply = engine.reply_agent_message(reply_command.clone()).unwrap();
    assert_eq!(reply.status, AgentMessageDeliveryStatus::Accepted);
    assert_eq!(
        reply.request.reply_to_message_id.as_deref(),
        Some("serial-message-1")
    );
    assert_eq!(reply.request.to_agent_id, sender.agent_id);
    assert_eq!(
        reply.request.correlation_id.as_deref(),
        Some("serial-correlation-1")
    );
    assert_eq!(
        engine.reply_agent_message(reply_command.clone()).unwrap(),
        reply
    );
    let reply_traffic = engine
        .list_agent_message_traffic(&rusty_crew_core_protocol::AgentMessageInboxQuery {
            to_session_id: Some(sender.session_id.clone()),
            from_session_id: Some(reviewer.session_id.clone()),
            correlation_id: Some("serial-correlation-1".into()),
            message_id: Some("serial-reply-message-1".into()),
            limit: Some(10),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(reply_traffic.len(), 1);
    assert_eq!(reply_traffic[0].delivery, reply);
    assert!(reply_traffic[0]
        .delivered_model_text
        .contains("to_session_id: serial-sender-session"));
    assert!(reply_traffic[0]
        .delivered_model_text
        .contains("reply_instruction: none"));
    let conflicting_reply = AgentMessageReplyCommand {
        delivery_id: AgentMessageDeliveryId::new("serial-reply-delivery-conflict"),
        idempotency_key: "serial-reply-delivery-conflict".into(),
        message_id: "serial-reply-message-conflict".into(),
        body: "a conflicting second reply".into(),
        ..reply_command
    };
    let conflict = engine.reply_agent_message(conflicting_reply).unwrap_err();
    assert_eq!(conflict.kind, CoreErrorKind::AlreadyExists);
    assert_eq!(conflict.message, "agent_message_reply_already_exists");

    for index in 2..=3 {
        let receipt = engine
            .deliver_agent_message(AgentMessageCommand {
                caller: AgentCoordinationCaller::DirectBrain {
                    session_id: sender.session_id.clone(),
                    wake_id: format!("sender-wake-{index}"),
                    tool_call_id: format!("sender-call-{index}"),
                },
                delivery_id: AgentMessageDeliveryId::new(format!("serial-delivery-{index}")),
                idempotency_key: format!("serial-delivery-{index}"),
                message_id: format!("serial-message-{index}"),
                to_address: reviewer.agent_id.0.clone(),
                input_kind: AgentMessageInputKind::RoutedAgentMessage,
                body: format!("review change {index}"),
                collaboration_mode: None,
                correlation_id: Some(format!("serial-correlation-{index}")),
                require_wake: true,
                created_at: format!("2026-06-19T00:00:0{index}Z"),
                expires_at: "2026-06-19T00:30:00Z".into(),
            })
            .unwrap();
        assert!(matches!(
            receipt.activation,
            Some(AgentActivation::QueuedForNextTurn { .. })
        ));
    }
    let queued_projection = engine
        .list_agent_message_inbox(&rusty_crew_core_protocol::AgentMessageInboxQuery {
            to_agent_id: Some(reviewer.agent_id.clone()),
            limit: Some(10),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(queued_projection.len(), 3);
    assert_eq!(
        queued_projection[0].status,
        rusty_crew_core_protocol::AgentMessageInboxStatus::InProgress
    );
    assert_eq!(
        queued_projection[1].status,
        rusty_crew_core_protocol::AgentMessageInboxStatus::Queued
    );
    assert_eq!(
        queued_projection[2].status,
        rusty_crew_core_protocol::AgentMessageInboxStatus::Queued
    );
    let exact_projection = engine
        .list_agent_message_inbox(&rusty_crew_core_protocol::AgentMessageInboxQuery {
            to_session_id: Some(reviewer.session_id.clone()),
            from_session_id: Some(sender.session_id.clone()),
            correlation_id: Some("serial-correlation-2".into()),
            message_id: Some("serial-message-2".into()),
            limit: Some(10),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(exact_projection.len(), 1);
    assert_eq!(
        exact_projection[0].delivery.request.to_session_id.as_ref(),
        Some(&reviewer.session_id)
    );
    assert!(exact_projection[0]
        .delivered_model_text
        .contains("[Rusty Crew routed payload: begin]\nreview change 2"));
    assert!(exact_projection[0]
        .delivered_model_text
        .contains("to_session_id: serial-reviewer-session"));
    engine
        .transition_external_turn(
            &first_request,
            ExternalTurnPhase::Completed,
            None,
            None,
            "2026-06-19T00:00:05Z".into(),
        )
        .unwrap();
    let second_request =
        ExternalTurnRequestId::new("external-follow-up:agent-message-queue:serial-message-2");
    assert!(engine.get_external_turn(&second_request).unwrap().is_some());
    assert!(engine
        .get_external_turn(&ExternalTurnRequestId::new(
            "external-follow-up:agent-message-queue:serial-message-3"
        ))
        .unwrap()
        .is_none());
    let advanced_projection = engine
        .list_agent_message_inbox(&rusty_crew_core_protocol::AgentMessageInboxQuery {
            to_agent_id: Some(reviewer.agent_id.clone()),
            limit: Some(10),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        advanced_projection[0].status,
        rusty_crew_core_protocol::AgentMessageInboxStatus::Replied
    );
    assert_eq!(
        advanced_projection[1].status,
        rusty_crew_core_protocol::AgentMessageInboxStatus::InProgress
    );
    engine
        .transition_external_turn(
            &second_request,
            ExternalTurnPhase::Starting,
            None,
            None,
            "2026-06-19T00:00:06Z".into(),
        )
        .unwrap();
    engine
        .transition_external_turn(
            &second_request,
            ExternalTurnPhase::Active,
            Some("native-review-turn-2".into()),
            None,
            "2026-06-19T00:00:07Z".into(),
        )
        .unwrap();
    engine
        .transition_external_turn(
            &second_request,
            ExternalTurnPhase::Completed,
            None,
            Some("review_no_reply".into()),
            "2026-06-19T00:00:08Z".into(),
        )
        .unwrap();
    assert!(engine
        .get_external_turn(&ExternalTurnRequestId::new(
            "external-follow-up:agent-message-queue:serial-message-3"
        ))
        .unwrap()
        .is_some());
}

#[test]
fn serial_external_inbox_does_not_advance_after_completed_turn_without_reply() {
    let engine = test_engine();
    let sender = engine
        .create_session(session_config(
            "serial-stalled-sender-session",
            "serial-stalled-sender",
            "serial-stalled-sender-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let reviewer = engine
        .create_session(session_config(
            "serial-stalled-reviewer-session",
            "codex-agent",
            "codex-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine.register_external_runtime(&runtime(), None).unwrap();
    let mut serial_binding = binding();
    serial_binding.session_id = Some(reviewer.session_id.clone());
    serial_binding.message_delivery_policy = ExternalMessageDeliveryPolicy::SerialNextTurn;
    engine.bind_external_agent(&serial_binding, None).unwrap();

    for index in 1..=2 {
        engine
            .deliver_agent_message(AgentMessageCommand {
                caller: AgentCoordinationCaller::DirectBrain {
                    session_id: sender.session_id.clone(),
                    wake_id: format!("stalled-wake-{index}"),
                    tool_call_id: format!("stalled-call-{index}"),
                },
                delivery_id: AgentMessageDeliveryId::new(format!("stalled-delivery-{index}")),
                idempotency_key: format!("stalled-delivery-{index}"),
                message_id: format!("stalled-message-{index}"),
                to_address: reviewer.agent_id.0.clone(),
                input_kind: AgentMessageInputKind::RoutedAgentMessage,
                body: format!("review stalled change {index}"),
                collaboration_mode: None,
                correlation_id: None,
                require_wake: true,
                created_at: format!("2026-06-19T00:00:0{index}Z"),
                expires_at: "2026-06-19T00:30:00Z".into(),
            })
            .unwrap();
    }
    let first_request = ExternalTurnRequestId::new("agent-message:stalled-message-1");
    engine
        .transition_external_turn(
            &first_request,
            ExternalTurnPhase::Starting,
            None,
            None,
            "2026-06-19T00:00:03Z".into(),
        )
        .unwrap();
    engine
        .transition_external_turn(
            &first_request,
            ExternalTurnPhase::Active,
            Some("native-stalled-turn".into()),
            None,
            "2026-06-19T00:00:04Z".into(),
        )
        .unwrap();
    engine
        .transition_external_turn(
            &first_request,
            ExternalTurnPhase::Completed,
            None,
            None,
            "2026-06-19T00:00:05Z".into(),
        )
        .unwrap();

    assert!(engine
        .get_external_turn(&ExternalTurnRequestId::new(
            "external-follow-up:agent-message-queue:stalled-message-2"
        ))
        .unwrap()
        .is_none());
    let inbox = engine
        .list_agent_message_inbox(&rusty_crew_core_protocol::AgentMessageInboxQuery {
            to_agent_id: Some(reviewer.agent_id),
            limit: Some(10),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        inbox[0].status,
        rusty_crew_core_protocol::AgentMessageInboxStatus::AwaitingReply
    );
    assert_eq!(
        inbox[1].status,
        rusty_crew_core_protocol::AgentMessageInboxStatus::Queued
    );
}

#[test]
fn serial_external_inbox_cancels_pending_work_when_recipient_session_is_replaced() {
    let engine = test_engine();
    let sender = engine
        .create_session(session_config(
            "replacement-sender-session",
            "replacement-sender",
            "replacement-sender-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let reviewer = engine
        .create_session(session_config(
            "replacement-reviewer-session-old",
            "codex-agent",
            "codex-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine.register_external_runtime(&runtime(), None).unwrap();
    let mut serial_binding = binding();
    serial_binding.session_id = Some(reviewer.session_id.clone());
    serial_binding.message_delivery_policy = ExternalMessageDeliveryPolicy::SerialNextTurn;
    engine.bind_external_agent(&serial_binding, None).unwrap();

    for index in 1..=2 {
        engine
            .deliver_agent_message(AgentMessageCommand {
                caller: AgentCoordinationCaller::DirectBrain {
                    session_id: sender.session_id.clone(),
                    wake_id: format!("replacement-wake-{index}"),
                    tool_call_id: format!("replacement-call-{index}"),
                },
                delivery_id: AgentMessageDeliveryId::new(format!("replacement-delivery-{index}")),
                idempotency_key: format!("replacement-delivery-{index}"),
                message_id: format!("replacement-message-{index}"),
                to_address: reviewer.agent_id.0.clone(),
                input_kind: AgentMessageInputKind::RoutedAgentMessage,
                body: format!("review replacement change {index}"),
                collaboration_mode: None,
                correlation_id: None,
                require_wake: true,
                created_at: format!("2026-06-19T00:00:0{index}Z"),
                expires_at: "2026-06-19T00:30:00Z".into(),
            })
            .unwrap();
    }
    let first_request = ExternalTurnRequestId::new("agent-message:replacement-message-1");
    engine
        .transition_external_turn(
            &first_request,
            ExternalTurnPhase::Starting,
            None,
            None,
            "2026-06-19T00:00:03Z".into(),
        )
        .unwrap();
    engine
        .transition_external_turn(
            &first_request,
            ExternalTurnPhase::Active,
            Some("native-replacement-turn".into()),
            None,
            "2026-06-19T00:00:04Z".into(),
        )
        .unwrap();
    engine.archive_session(&reviewer.session_id).unwrap();
    engine
        .create_session(session_config(
            "replacement-reviewer-session-new",
            "codex-agent",
            "codex-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine
        .transition_external_turn(
            &first_request,
            ExternalTurnPhase::Failed,
            None,
            Some("codex_failed".into()),
            "2026-06-19T00:00:05Z".into(),
        )
        .unwrap();

    assert!(engine
        .get_external_turn(&ExternalTurnRequestId::new(
            "external-follow-up:agent-message-queue:replacement-message-2"
        ))
        .unwrap()
        .is_none());
    let inbox = engine
        .list_agent_message_inbox(&rusty_crew_core_protocol::AgentMessageInboxQuery {
            to_agent_id: Some(reviewer.agent_id),
            limit: Some(10),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        inbox[1].status,
        rusty_crew_core_protocol::AgentMessageInboxStatus::Rejected
    );
    assert_eq!(
        inbox[1].terminal_reason_code.as_deref(),
        Some("agent_message_recipient_session_changed")
    );
}
