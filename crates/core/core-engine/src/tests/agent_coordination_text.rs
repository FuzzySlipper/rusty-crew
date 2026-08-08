use super::*;
use rusty_crew_core_protocol::{AgentId, AgentMessageDeliveryId, SessionId};

#[test]
fn agent_sender_receives_reply_by_message_instruction() {
    let text = agent_message_model_text(&request(
        AgentMessageInputKind::RoutedAgentMessage,
        Some("sender-session"),
    ));

    assert!(text.contains("from_session_id: sender-session"));
    assert!(text.contains("to_agent_id: recipient"));
    assert!(text.contains("to_session_id: recipient-session"));
    assert!(text.contains("input_kind: routed_agent_message"));
    assert!(text.contains(
        "[Rusty Crew routed payload: begin]\ninspect this\n[Rusty Crew routed payload: end]"
    ));
    assert!(text.ends_with("[Rusty Crew routed message: end]"));
    assert!(text.contains(
        "reply_instruction: call rusty_crew.reply_agent_message with messageId=message-1 and your reply body"
    ));
}

#[test]
fn operator_sender_does_not_receive_impossible_reply_instruction() {
    let text = agent_message_model_text(&request(AgentMessageInputKind::RoutedAgentMessage, None));

    assert!(text.contains("from_session_id: none"));
    assert!(text.contains(
        "reply_instruction: unavailable (sender has no routable agent session; respond in this turn only)"
    ));
    assert!(text.contains(
        "complete_routed_review is valid only when the payload explicitly begins with a Rusty Crew managed review submission identifier"
    ));
    assert!(!text.contains("call rusty_crew.reply_agent_message"));
}

#[test]
fn operator_input_is_projected_as_plain_user_text() {
    let text = agent_message_model_text(&request(AgentMessageInputKind::Operator, None));

    assert_eq!(text, "inspect this");
    assert!(!text.contains("Rusty Crew routed message"));
    assert!(!text.contains("reply_instruction"));
}

#[test]
fn routed_reply_does_not_request_an_acknowledgement_reply() {
    let mut request = request(
        AgentMessageInputKind::RoutedAgentMessage,
        Some("sender-session"),
    );
    request.reply_to_message_id = Some("original-message".into());

    let text = agent_message_model_text(&request);

    assert!(text.contains("this message is already a reply"));
    assert!(text.contains("do not acknowledge it with coordination tools"));
    assert!(!text.contains("call rusty_crew.reply_agent_message"));
}

fn request(
    input_kind: AgentMessageInputKind,
    from_session_id: Option<&str>,
) -> AgentMessageDeliveryRequest {
    AgentMessageDeliveryRequest {
        delivery_id: AgentMessageDeliveryId::new("delivery-1"),
        idempotency_key: "delivery-1".into(),
        message_id: "message-1".into(),
        from_agent_id: AgentId::new("sender"),
        from_session_id: from_session_id.map(SessionId::new),
        requested_address: "recipient".into(),
        to_agent_id: AgentId::new("recipient"),
        to_session_id: Some(SessionId::new("recipient-session")),
        routing: None,
        reply_to_message_id: None,
        input_kind,
        body: "inspect this".into(),
        collaboration_mode: None,
        correlation_id: Some("correlation-1".into()),
        require_wake: true,
        created_at: "2026-07-15T00:00:00Z".into(),
        expires_at: "2026-07-15T00:10:00Z".into(),
    }
}
