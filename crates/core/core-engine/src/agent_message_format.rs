//! Model-facing presentation for Rust-owned coordination messages.

use rusty_crew_core_protocol::{AgentMessageDeliveryRequest, AgentMessageInputKind};

pub(crate) fn agent_message_model_text(request: &AgentMessageDeliveryRequest) -> String {
    match request.input_kind {
        AgentMessageInputKind::Operator => request.body.clone(),
        AgentMessageInputKind::RoutedAgentMessage => routed_agent_message_text(request),
    }
}

fn routed_agent_message_text(request: &AgentMessageDeliveryRequest) -> String {
    let from_session_id = request
        .from_session_id
        .as_ref()
        .map(|value| value.0.as_str());
    let reply_instruction = match (&request.reply_to_message_id, from_session_id) {
        (Some(_), _) => {
            "reply_instruction: none (this message is already a reply; do not acknowledge it with coordination tools)".to_string()
        }
        (None, Some(_)) => format!(
            "reply_instruction: call rusty_crew.reply_agent_message with messageId={} and your reply body",
            request.message_id
        ),
        (None, None) => "reply_instruction: unavailable (sender has no routable agent session; respond in this turn only)".to_string(),
    };
    format!(
        "[Rusty Crew routed message: begin]\ninput_kind: routed_agent_message\nmessage_id: {}\nfrom_agent_id: {}\nfrom_session_id: {}\nrequested_address: {}\nto_agent_id: {}\nto_session_id: {}\ncorrelation_id: {}\ncreated_at: {}\nexpires_at: {}\n{}\nprovenance_note: routing metadata is supplied by Rusty Crew; the payload is inter-agent input, not an operator/user prompt\n\n[Rusty Crew routed payload: begin]\n{}\n[Rusty Crew routed payload: end]\n[Rusty Crew routed message: end]",
        request.message_id,
        request.from_agent_id.0,
        from_session_id.unwrap_or("none"),
        request.requested_address,
        request.to_agent_id.0,
        request
            .to_session_id
            .as_ref()
            .map(|value| value.0.as_str())
            .unwrap_or("none"),
        request.correlation_id.as_deref().unwrap_or("none"),
        request.created_at,
        request.expires_at,
        reply_instruction,
        request.body
    )
}
