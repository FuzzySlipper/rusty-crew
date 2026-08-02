//! Receipt-level operator projection for durable coordination traffic.

use super::*;
use crate::agent_message_format::agent_message_model_text;
use rusty_crew_core_protocol::{
    AgentActivation, AgentMessageDeliveryReceipt, AgentMessageInboxQuery, AgentMessageTrafficItem,
    ExternalTurnRequestId, RuntimeActivityWakeSettlement,
};

impl CoreEngine {
    pub fn list_agent_message_traffic(
        &self,
        query: &AgentMessageInboxQuery,
    ) -> CoreResult<Vec<AgentMessageTrafficItem>> {
        let limit = query.limit.unwrap_or(100).clamp(1, 500);
        self.store
            .list_agent_message_traffic_deliveries(query, limit)?
            .into_iter()
            .map(|delivery| self.project_agent_message_traffic_item(delivery))
            .collect()
    }

    fn project_agent_message_traffic_item(
        &self,
        delivery: AgentMessageDeliveryReceipt,
    ) -> CoreResult<AgentMessageTrafficItem> {
        let queued_message_id = format!("agent-message-queue:{}", delivery.request.message_id);
        let queued = delivery
            .request
            .to_session_id
            .as_ref()
            .map(|session_id| {
                self.store.load_queued_messages(&QueuedMessageFilter {
                    state: None,
                    owner_session_id: Some(session_id.clone()),
                    owner_agent_id: None,
                    limit: None,
                })
            })
            .transpose()?
            .unwrap_or_default()
            .into_iter()
            .find(|record| record.message_id == queued_message_id);

        let direct_request_id =
            ExternalTurnRequestId::new(format!("agent-message:{}", delivery.request.message_id));
        let follow_up_request_id =
            ExternalTurnRequestId::new(format!("external-follow-up:{queued_message_id}"));
        let external_turn = self
            .store
            .get_external_turn(&direct_request_id)?
            .or(self.store.get_external_turn(&follow_up_request_id)?);

        let wake_settlement = match &delivery.activation {
            Some(AgentActivation::DirectBrainWakeRequested {
                session_id,
                wake_id,
            }) => self
                .store
                .list_runtime_activities_for_session(session_id, Some(500))?
                .into_iter()
                .find(|record| record.wake_id.as_deref() == Some(wake_id.as_str()))
                .map(|record| RuntimeActivityWakeSettlement {
                    wake_id: wake_id.clone(),
                    status: record.status,
                    reason_code: record.reason_code,
                    summary: record.summary.unwrap_or_else(|| record.phase.clone()),
                }),
            _ => None,
        };
        let terminal_reason_code = external_turn
            .as_ref()
            .and_then(|record| record.terminal_reason_code.clone())
            .or_else(|| {
                queued
                    .as_ref()
                    .and_then(|record| record.state_reason.clone())
            });

        Ok(AgentMessageTrafficItem {
            delivered_model_text: agent_message_model_text(&delivery.request),
            wake_settlement,
            external_turn_phase: external_turn.as_ref().map(|record| record.phase),
            queued_message_id: queued.map(|record| record.message_id),
            terminal_reason_code,
            delivery,
        })
    }
}
