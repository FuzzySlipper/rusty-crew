use super::*;
use rusty_crew_core_protocol::{
    AgentCoordinationCaller, AgentMessageCommand, AgentMessageDeliveryId,
    AgentMessageDeliveryStatus, AgentMessageInputKind, AgentMessageReplyCommand,
    InstallDiplomatBindingQuery, InstallDiplomatBindingStatus, InstallDiplomatBindingWrite,
    InstallDiplomatParticipationMode, InstallDiplomatRebindRequest,
    TelegramDiplomatIngressDecision, TelegramDiplomatIngressRequest, TelegramDiplomatSender,
    TelegramDiplomatSenderKind,
};

#[test]
fn diplomat_binding_is_session_scoped_revisioned_and_restart_hydrated() {
    let data_dir = unique_data_dir("install-diplomat-binding");
    let engine = test_engine_with_data_dir(data_dir.clone());
    let mut alpha_config = session_config(
        "diplomat-alpha-session",
        "install-diplomat",
        "shared-diplomat-profile",
        SessionKind::Full,
    );
    alpha_config.workspace.as_mut().unwrap().cwd = "/srv/install-alpha".to_string();
    let alpha = engine.create_session(alpha_config).unwrap();
    let mut beta_config = session_config(
        "diplomat-beta-session",
        "install-diplomat",
        "shared-diplomat-profile",
        SessionKind::Full,
    );
    beta_config.workspace.as_mut().unwrap().cwd = "/srv/install-beta".to_string();
    let beta = engine.create_session(beta_config).unwrap();

    let binding = engine
        .put_install_diplomat_binding(binding_write(
            "diplomat-binding",
            &alpha,
            "-100500",
            Some("42"),
        ))
        .unwrap();
    assert_eq!(binding.revision, 1);
    assert_eq!(binding.session_id, alpha.session_id);
    assert_eq!(binding.installation_label, "Workshop Alpha");
    assert_eq!(binding.bot_username, "installalphabot");
    assert_eq!(
        engine
            .get_session(&alpha.session_id)
            .unwrap()
            .workspace
            .unwrap()
            .cwd,
        "/srv/install-alpha"
    );
    assert_eq!(
        engine
            .get_session(&beta.session_id)
            .unwrap()
            .workspace
            .unwrap()
            .cwd,
        "/srv/install-beta"
    );
    assert_eq!(alpha.profile_id, beta.profile_id);

    let duplicate_surface = engine
        .put_install_diplomat_binding(InstallDiplomatBindingWrite {
            binding_id: "other-binding".to_string(),
            installation_id: "install-other".to_string(),
            ..binding_write("ignored", &beta, "-100500", Some("42"))
        })
        .unwrap_err();
    assert_eq!(
        duplicate_surface.message,
        "install_diplomat_surface_conflict"
    );

    drop(engine);
    let engine = test_engine_with_data_dir(data_dir);
    let hydrated = engine
        .get_install_diplomat_binding("diplomat-binding")
        .unwrap()
        .unwrap();
    assert_eq!(hydrated.session_id, alpha.session_id);
    assert_eq!(hydrated.revision, 1);

    engine.archive_session(&alpha.session_id).unwrap();
    let degraded = engine
        .get_install_diplomat_binding("diplomat-binding")
        .unwrap()
        .unwrap();
    assert_eq!(degraded.status, InstallDiplomatBindingStatus::NeedsRebind);
    assert_eq!(
        degraded.degraded_reason.as_deref(),
        Some("diplomat_session_archived")
    );

    let rebound = engine
        .rebind_install_diplomat(InstallDiplomatRebindRequest {
            binding_id: degraded.binding_id,
            expected_revision: degraded.revision,
            agent_id: beta.agent_id.clone(),
            instance_id: None,
            session_id: beta.session_id.clone(),
            updated_at: "2026-06-19T00:01:00Z".to_string(),
        })
        .unwrap();
    assert_eq!(rebound.status, InstallDiplomatBindingStatus::Active);
    assert_eq!(rebound.session_id, beta.session_id);
    assert_eq!(rebound.revision, 3);
    assert_eq!(
        engine
            .list_install_diplomat_bindings(&InstallDiplomatBindingQuery {
                session_id: Some(beta.session_id.clone()),
                ..InstallDiplomatBindingQuery::default()
            })
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        engine
            .get_session(&beta.session_id)
            .unwrap()
            .workspace
            .unwrap()
            .cwd,
        "/srv/install-beta"
    );
}

#[test]
fn shutdown_preserves_bound_diplomat_session_and_binding() {
    let data_dir = unique_data_dir("install-diplomat-clean-restart");
    let session_id;
    {
        let engine = test_engine_with_data_dir(data_dir.clone());
        let session = engine
            .create_session(session_config(
                "diplomat-session",
                "install-diplomat",
                "diplomat-profile",
                SessionKind::Full,
            ))
            .unwrap();
        session_id = session.session_id.clone();
        engine
            .put_install_diplomat_binding(binding_write(
                "diplomat-binding",
                &session,
                "-100500",
                None,
            ))
            .unwrap();

        let summary = engine.shutdown_with_timeout(25).unwrap();
        assert_eq!(summary.archived_sessions, 0);
    }

    let restarted = test_engine_with_data_dir(data_dir.clone());
    assert_ne!(
        restarted.get_session(&session_id).unwrap().status,
        SessionStatus::Archived
    );
    let binding = restarted
        .get_install_diplomat_binding("diplomat-binding")
        .unwrap()
        .unwrap();
    assert_eq!(binding.status, InstallDiplomatBindingStatus::Active);
    assert_eq!(binding.revision, 1);
    drop(restarted);
    let _ = std::fs::remove_dir_all(data_dir);
}

#[test]
fn reactivating_exact_diplomat_session_repairs_archive_degradation() {
    let engine = test_engine();
    let config = session_config(
        "diplomat-session",
        "install-diplomat",
        "diplomat-profile",
        SessionKind::Full,
    );
    let session = engine.create_session(config.clone()).unwrap();
    engine
        .put_install_diplomat_binding(binding_write("diplomat-binding", &session, "-100500", None))
        .unwrap();

    engine.archive_session(&session.session_id).unwrap();
    assert_eq!(
        engine
            .get_install_diplomat_binding("diplomat-binding")
            .unwrap()
            .unwrap()
            .status,
        InstallDiplomatBindingStatus::NeedsRebind
    );

    engine.ensure_configured_session(config).unwrap();
    let repaired = engine
        .get_install_diplomat_binding("diplomat-binding")
        .unwrap()
        .unwrap();
    assert_eq!(repaired.status, InstallDiplomatBindingStatus::Active);
    assert_eq!(repaired.degraded_reason, None);
}

#[test]
fn diplomat_ingress_preserves_sender_and_terminates_correlated_bot_loops() {
    let engine = test_engine();
    let diplomat = engine
        .create_session(session_config(
            "diplomat-session",
            "install-diplomat",
            "diplomat-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine
        .put_install_diplomat_binding(binding_write(
            "diplomat-binding",
            &diplomat,
            "-100500",
            Some("42"),
        ))
        .unwrap();

    let ignored = engine
        .plan_telegram_diplomat_ingress(human_ingress(false, "ignored-interaction", "human-0"))
        .unwrap();
    assert_eq!(ignored.decision, TelegramDiplomatIngressDecision::Ignored);
    assert!(ignored.interaction.is_none());

    let human = engine
        .plan_telegram_diplomat_ingress(human_ingress(true, "support-interaction", "human-message"))
        .unwrap();
    assert_eq!(human.decision, TelegramDiplomatIngressDecision::Routed);
    assert_eq!(human.target_session_id, Some(diplomat.session_id));
    assert_eq!(human.sender.external_user_id, "7001");
    assert_eq!(
        human.sender.display_label.as_deref(),
        Some("Remote Operator")
    );
    assert_eq!(
        human.crew_correlation_id.as_deref(),
        Some("telegram:diplomat-binding:support-interaction")
    );

    for depth in 1..=6 {
        let bot = engine
            .plan_telegram_diplomat_ingress(bot_ingress(
                "support-interaction",
                &format!("bot-message-{depth}"),
                &format!("2026-06-19T00:00:0{depth}Z"),
            ))
            .unwrap();
        assert_eq!(bot.decision, TelegramDiplomatIngressDecision::Routed);
        assert_eq!(bot.interaction.unwrap().bot_depth, depth);
    }
    let terminal = engine
        .plan_telegram_diplomat_ingress(bot_ingress(
            "support-interaction",
            "bot-message-7",
            "2026-06-19T00:00:07Z",
        ))
        .unwrap();
    assert_eq!(
        terminal.decision,
        TelegramDiplomatIngressDecision::LoopTerminated
    );
    assert_eq!(terminal.reason_code, "telegram_bot_loop_depth_exceeded");
    assert!(terminal.target_session_id.is_none());

    let after_terminal = engine
        .plan_telegram_diplomat_ingress(bot_ingress(
            "support-interaction",
            "bot-message-8",
            "2026-06-19T00:00:08Z",
        ))
        .unwrap();
    assert_eq!(
        after_terminal.decision,
        TelegramDiplomatIngressDecision::LoopTerminated
    );

    engine
        .plan_telegram_diplomat_ingress(human_ingress(
            true,
            "second-interaction",
            "second-human-message",
        ))
        .unwrap();
    let eighth_pair_message = engine
        .plan_telegram_diplomat_ingress(bot_ingress(
            "second-interaction",
            "bot-message-9",
            "2026-06-19T00:00:09Z",
        ))
        .unwrap();
    assert_eq!(
        eighth_pair_message.decision,
        TelegramDiplomatIngressDecision::Routed
    );
    engine
        .plan_telegram_diplomat_ingress(human_ingress(
            true,
            "third-interaction",
            "third-human-message",
        ))
        .unwrap();
    let pair_rate_limited = engine
        .plan_telegram_diplomat_ingress(bot_ingress(
            "third-interaction",
            "bot-message-10",
            "2026-06-19T00:00:10Z",
        ))
        .unwrap();
    assert_eq!(
        pair_rate_limited.decision,
        TelegramDiplomatIngressDecision::RateLimited
    );
    assert_eq!(
        pair_rate_limited.reason_code,
        "telegram_bot_pair_rate_limited"
    );
}

#[test]
fn diplomat_uses_normal_crew_messaging_and_specialist_needs_no_telegram_binding() {
    let engine = test_engine();
    let diplomat = engine
        .create_session(session_config(
            "diplomat-session",
            "install-diplomat",
            "diplomat-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let specialist = engine
        .create_session(session_config(
            "specialist-session",
            "local-specialist",
            "specialist-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine
        .put_install_diplomat_binding(binding_write(
            "diplomat-binding",
            &diplomat,
            "-100500",
            Some("42"),
        ))
        .unwrap();
    assert!(engine
        .list_install_diplomat_bindings(&InstallDiplomatBindingQuery {
            session_id: Some(specialist.session_id.clone()),
            ..InstallDiplomatBindingQuery::default()
        })
        .unwrap()
        .is_empty());

    let ingress = engine
        .plan_telegram_diplomat_ingress(human_ingress(
            true,
            "diagnostic-interaction",
            "human-diagnostic",
        ))
        .unwrap();
    let correlation_id = ingress.crew_correlation_id.unwrap();
    let request = engine
        .deliver_agent_message(AgentMessageCommand {
            caller: AgentCoordinationCaller::DirectBrain {
                session_id: diplomat.session_id.clone(),
                wake_id: "diplomat-wake".to_string(),
                tool_call_id: "consult-specialist".to_string(),
            },
            delivery_id: AgentMessageDeliveryId::new("diplomat-consult-delivery"),
            idempotency_key: "diplomat-consult-delivery".to_string(),
            message_id: "diplomat-consult-message".to_string(),
            to_address: specialist.agent_id.0.clone(),
            input_kind: AgentMessageInputKind::RoutedAgentMessage,
            body: "Check the local service diagnostics".to_string(),
            image_attachment_ids: Vec::new(),
            collaboration_mode: None,
            correlation_id: Some(correlation_id.clone()),
            require_wake: true,
            created_at: "2026-06-19T00:00:01Z".to_string(),
            expires_at: "2026-06-19T00:05:01Z".to_string(),
        })
        .unwrap();
    assert_eq!(request.status, AgentMessageDeliveryStatus::Accepted);
    assert_eq!(
        request.request.to_session_id,
        Some(specialist.session_id.clone())
    );

    let reply = engine
        .reply_agent_message(AgentMessageReplyCommand {
            caller: AgentCoordinationCaller::DirectBrain {
                session_id: specialist.session_id,
                wake_id: "specialist-wake".to_string(),
                tool_call_id: "specialist-reply".to_string(),
            },
            delivery_id: AgentMessageDeliveryId::new("specialist-reply-delivery"),
            idempotency_key: "specialist-reply-delivery".to_string(),
            message_id: "specialist-reply-message".to_string(),
            in_reply_to_message_id: "diplomat-consult-message".to_string(),
            body: "Service is healthy".to_string(),
            created_at: "2026-06-19T00:00:02Z".to_string(),
            expires_at: "2026-06-19T00:05:02Z".to_string(),
        })
        .unwrap();
    assert_eq!(reply.status, AgentMessageDeliveryStatus::Accepted);
    assert_eq!(reply.request.to_session_id, Some(diplomat.session_id));
    assert_eq!(
        reply.request.correlation_id.as_deref(),
        Some(correlation_id.as_str())
    );
}

fn binding_write(
    binding_id: &str,
    session: &SessionState,
    external_chat_id: &str,
    external_thread_id: Option<&str>,
) -> InstallDiplomatBindingWrite {
    InstallDiplomatBindingWrite {
        binding_id: binding_id.to_string(),
        expected_revision: None,
        installation_id: "install-alpha".to_string(),
        installation_label: "Workshop Alpha".to_string(),
        adapter_id: AdapterId::new("telegram-install-alpha"),
        bot_user_id: "9001".to_string(),
        bot_username: "@InstallAlphaBot".to_string(),
        agent_id: session.agent_id.clone(),
        instance_id: None,
        session_id: session.session_id.clone(),
        external_chat_id: external_chat_id.to_string(),
        external_thread_id: external_thread_id.map(str::to_string),
        participation_mode: InstallDiplomatParticipationMode::MentionOrReply,
        updated_at: "2026-06-19T00:00:00Z".to_string(),
    }
}

fn human_ingress(
    addressed_to_bot: bool,
    interaction_id: &str,
    external_message_id: &str,
) -> TelegramDiplomatIngressRequest {
    TelegramDiplomatIngressRequest {
        binding_id: "diplomat-binding".to_string(),
        interaction_id: interaction_id.to_string(),
        external_message_id: external_message_id.to_string(),
        reply_to_external_message_id: None,
        sender: TelegramDiplomatSender {
            kind: TelegramDiplomatSenderKind::Human,
            external_user_id: "7001".to_string(),
            username: Some("remote_operator".to_string()),
            display_label: Some("Remote Operator".to_string()),
        },
        addressed_to_bot,
        correlated_interaction: false,
        receiving_bot_user_id: "9001".to_string(),
        received_at: "2026-06-19T00:00:00Z".to_string(),
    }
}

fn bot_ingress(
    interaction_id: &str,
    external_message_id: &str,
    received_at: &str,
) -> TelegramDiplomatIngressRequest {
    TelegramDiplomatIngressRequest {
        binding_id: "diplomat-binding".to_string(),
        interaction_id: interaction_id.to_string(),
        external_message_id: external_message_id.to_string(),
        reply_to_external_message_id: Some("human-message".to_string()),
        sender: TelegramDiplomatSender {
            kind: TelegramDiplomatSenderKind::Bot,
            external_user_id: "9002".to_string(),
            username: Some("install_beta_bot".to_string()),
            display_label: Some("Install Beta".to_string()),
        },
        addressed_to_bot: true,
        correlated_interaction: true,
        receiving_bot_user_id: "9001".to_string(),
        received_at: received_at.to_string(),
    }
}
