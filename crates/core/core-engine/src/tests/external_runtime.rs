use super::*;
use rusty_crew_core_protocol::{
    AgentActivation, AgentCoordinationCaller, AgentMessageCommand, AgentMessageDeliveryId,
    AgentMessageDeliveryStatus, AgentRoundCommand, AgentRoundId, AgentRoundStatus,
    ExternalAgentBinding, ExternalBindingId, ExternalBindingPurpose, ExternalBindingStatus,
    ExternalControllerLease, ExternalEndpoint, ExternalEndpointTransport, ExternalProcessOwnership,
    ExternalRuntimeDesiredState, ExternalRuntimeId, ExternalRuntimeKind,
    ExternalRuntimeObservedState, ExternalRuntimeRegistration, ExternalTurnInputPart,
    ExternalTurnPhase, ExternalTurnRequestId, TurnInputProvenance, TurnInputProvenanceKind,
};

#[test]
fn durable_agent_round_resolves_reply_without_second_wake() {
    let data_dir = unique_data_dir("agent-round-reply");
    let engine = test_engine_with_data_dir(data_dir.clone());
    let sender = engine
        .create_session(session_config(
            "sender-session",
            "sender-agent",
            "sender-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let recipient = engine
        .create_session(session_config(
            "recipient-session",
            "recipient-agent",
            "recipient-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let (_subscription_id, wakes) = engine
        .subscribe_events(EventSubscription {
            event_kinds: vec![CoreEventKind::BrainWakeRequested],
            session_id: Some(sender.session_id.clone()),
            agent_id: None,
            adapter_id: None,
        })
        .unwrap();
    let (_observation_subscription_id, observations) = engine
        .subscribe_events(EventSubscription {
            event_kinds: vec![
                CoreEventKind::AgentMessageDeliveryObserved,
                CoreEventKind::AgentRoundObserved,
            ],
            session_id: None,
            agent_id: None,
            adapter_id: None,
        })
        .unwrap();

    let started = engine
        .begin_agent_round(AgentRoundCommand {
            caller: AgentCoordinationCaller::DirectBrain {
                session_id: sender.session_id.clone(),
                wake_id: "sender-wake".into(),
                tool_call_id: "round-call".into(),
            },
            round_id: AgentRoundId::new("round-1"),
            idempotency_key: "round-key-1".into(),
            message_id: "round-message-1".into(),
            to_agent_id: recipient.agent_id.clone(),
            body: "please inspect this".into(),
            correlation_id: "round-correlation-1".into(),
            created_at: "2026-06-19T00:00:00Z".into(),
            expires_at: "2026-06-19T00:05:00Z".into(),
        })
        .unwrap();
    assert_eq!(started.round.status, AgentRoundStatus::Pending);
    assert_eq!(
        started.delivery.status,
        AgentMessageDeliveryStatus::Accepted
    );
    assert!(matches!(
        observations.recv_timeout(Duration::from_secs(1)).unwrap(),
        CoreEvent::AgentRoundObserved { round } if round.status == AgentRoundStatus::Pending
    ));
    assert!(matches!(
        observations.recv_timeout(Duration::from_secs(1)).unwrap(),
        CoreEvent::AgentMessageDeliveryObserved { receipt }
            if receipt.status == AgentMessageDeliveryStatus::Accepted
    ));

    let reply = engine
        .deliver_agent_message(AgentMessageCommand {
            caller: AgentCoordinationCaller::DirectBrain {
                session_id: recipient.session_id,
                wake_id: "recipient-wake".into(),
                tool_call_id: "reply-call".into(),
            },
            delivery_id: AgentMessageDeliveryId::new("reply-delivery-1"),
            idempotency_key: "reply-delivery-key-1".into(),
            message_id: "reply-message-1".into(),
            to_agent_id: sender.agent_id,
            body: "inspection complete".into(),
            correlation_id: Some("round-correlation-1".into()),
            require_wake: true,
            created_at: "2026-06-19T00:00:01Z".into(),
            expires_at: "2026-06-19T00:05:00Z".into(),
        })
        .unwrap();
    assert_eq!(reply.status, AgentMessageDeliveryStatus::Accepted);
    assert_eq!(reply.resolved_round_id, Some(AgentRoundId::new("round-1")));
    assert!(reply.activation.is_none());
    assert!(matches!(
        observations.recv_timeout(Duration::from_secs(1)).unwrap(),
        CoreEvent::AgentRoundObserved { round } if round.status == AgentRoundStatus::Replied
    ));
    assert!(matches!(
        observations.recv_timeout(Duration::from_secs(1)).unwrap(),
        CoreEvent::AgentMessageDeliveryObserved { receipt }
            if receipt.resolved_round_id == Some(AgentRoundId::new("round-1"))
    ));
    assert!(wakes.recv_timeout(Duration::from_millis(50)).is_err());

    drop(engine);
    let restarted = test_engine_with_data_dir(data_dir);
    let round = restarted
        .get_agent_round(&AgentRoundId::new("round-1"))
        .unwrap()
        .unwrap();
    assert_eq!(round.status, AgentRoundStatus::Replied);
    assert_eq!(round.reply_message_id.as_deref(), Some("reply-message-1"));
}

#[test]
fn active_external_recipient_queues_without_brain_wake() {
    let engine = test_engine();
    let sender = engine
        .create_session(session_config(
            "sender-session",
            "sender-agent",
            "sender-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let codex = engine
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
        .activate_agent_execution(activation("codex-agent", "already-active"))
        .unwrap();
    let (_subscription_id, wakes) = engine
        .subscribe_events(EventSubscription {
            event_kinds: vec![CoreEventKind::BrainWakeRequested],
            session_id: Some(codex.session_id.clone()),
            agent_id: None,
            adapter_id: None,
        })
        .unwrap();

    let command = AgentMessageCommand {
        caller: AgentCoordinationCaller::DirectBrain {
            session_id: sender.session_id,
            wake_id: "sender-wake".into(),
            tool_call_id: "send-call".into(),
        },
        delivery_id: AgentMessageDeliveryId::new("delivery-busy"),
        idempotency_key: "delivery-busy-key".into(),
        message_id: "message-busy".into(),
        to_agent_id: codex.agent_id.clone(),
        body: "queue for later".into(),
        correlation_id: None,
        require_wake: true,
        created_at: "2026-06-19T00:00:00Z".into(),
        expires_at: "2026-06-19T00:05:00Z".into(),
    };
    let receipt = engine.deliver_agent_message(command.clone()).unwrap();
    assert!(matches!(
        receipt.activation,
        Some(AgentActivation::QueuedForNextTurn { .. })
    ));
    assert!(wakes.recv_timeout(Duration::from_millis(50)).is_err());
    let queued = CoordinationStore::open(engine.config.engine_data_dir.clone())
        .unwrap()
        .load_queued_messages(&QueuedMessageFilter {
            state: Some(QueuedMessageState::Pending),
            owner_session_id: Some(codex.session_id.clone()),
            owner_agent_id: None,
            limit: None,
        })
        .unwrap();
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].message.body, "queue for later");
    assert_eq!(engine.deliver_agent_message(command).unwrap(), receipt);
    let queued_after_replay = CoordinationStore::open(engine.config.engine_data_dir.clone())
        .unwrap()
        .load_queued_messages(&QueuedMessageFilter {
            state: Some(QueuedMessageState::Pending),
            owner_session_id: Some(codex.session_id),
            owner_agent_id: None,
            limit: None,
        })
        .unwrap();
    assert_eq!(queued_after_replay.len(), 1);
}

#[test]
fn codex_caller_requires_current_controller_and_active_native_turn() {
    let engine = test_engine();
    let direct = engine
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
    engine.register_external_runtime(&runtime(), None).unwrap();
    engine.bind_external_agent(&binding(), None).unwrap();
    let lease = engine
        .acquire_external_runtime_controller(
            &ExternalControllerLease {
                runtime_id: ExternalRuntimeId::new("codex-local"),
                holder_instance_id: "controller-a".into(),
                generation: 0,
                acquired_at: "2026-06-19T00:00:00Z".into(),
                renewed_at: "2026-06-19T00:00:00Z".into(),
                expires_at: "2026-06-19T00:10:00Z".into(),
                revision: 0,
            },
            &"2026-06-19T00:00:00Z".into(),
        )
        .unwrap();
    engine
        .activate_agent_execution(activation("codex-agent", "codex-active"))
        .unwrap();
    engine
        .transition_external_turn(
            &ExternalTurnRequestId::new("codex-active"),
            ExternalTurnPhase::Starting,
            None,
            None,
            "2026-06-19T00:00:01Z".into(),
        )
        .unwrap();
    engine
        .transition_external_turn(
            &ExternalTurnRequestId::new("codex-active"),
            ExternalTurnPhase::Active,
            Some("native-turn-7".into()),
            None,
            "2026-06-19T00:00:02Z".into(),
        )
        .unwrap();

    let command = AgentMessageCommand {
        caller: AgentCoordinationCaller::ExternalAgent {
            runtime_id: ExternalRuntimeId::new("codex-local"),
            binding_id: ExternalBindingId::new("codex-binding"),
            controller_instance_id: "controller-a".into(),
            controller_generation: lease.generation,
            native_thread_id: "native-thread-7".into(),
            native_turn_id: "native-turn-7".into(),
            native_request_id: "tool-call-1".into(),
        },
        delivery_id: AgentMessageDeliveryId::new("codex-delivery"),
        idempotency_key: "codex-delivery-key".into(),
        message_id: "codex-message".into(),
        to_agent_id: direct.agent_id,
        body: "message from Codex".into(),
        correlation_id: None,
        require_wake: true,
        created_at: "2026-06-19T00:00:03Z".into(),
        expires_at: "2026-06-19T00:05:00Z".into(),
    };
    assert!(matches!(
        engine
            .deliver_agent_message(command.clone())
            .unwrap()
            .activation,
        Some(AgentActivation::DirectBrainWakeRequested { .. })
    ));

    let mut stale = command;
    stale.delivery_id = AgentMessageDeliveryId::new("codex-delivery-stale");
    stale.idempotency_key = "codex-delivery-stale-key".into();
    stale.message_id = "codex-message-stale".into();
    let AgentCoordinationCaller::ExternalAgent {
        controller_generation,
        ..
    } = &mut stale.caller
    else {
        unreachable!()
    };
    *controller_generation += 1;
    assert_eq!(
        engine.deliver_agent_message(stale).unwrap_err().kind,
        CoreErrorKind::ActionRejected
    );
}

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

fn activation(agent_id: &str, request_id: &str) -> AgentActivationRequest {
    AgentActivationRequest {
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
