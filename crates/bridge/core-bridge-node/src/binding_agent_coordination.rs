use super::*;

impl NativeBridge {
    pub fn deliver_agent_message(
        &self,
        command: AgentMessageCommand,
    ) -> CoreResult<AgentMessageDeliveryReceipt> {
        self.engine()?.deliver_agent_message(command)
    }

    pub fn begin_agent_round(
        &self,
        command: AgentRoundCommand,
    ) -> CoreResult<AgentRoundStartReceipt> {
        self.engine()?.begin_agent_round(command)
    }

    pub fn get_agent_round(
        &self,
        round_id: &AgentRoundId,
    ) -> CoreResult<Option<AgentCorrelatedRound>> {
        self.engine()?.get_agent_round(round_id)
    }

    pub fn get_agent_message_delivery(
        &self,
        delivery_id: &AgentMessageDeliveryId,
    ) -> CoreResult<Option<AgentMessageDeliveryReceipt>> {
        self.engine()?.get_agent_message_delivery(delivery_id)
    }
}

#[napi_derive::napi]
impl NativeBridgeBinding {
    #[napi]
    pub fn deliver_agent_message_json(&self, command_json: String) -> napi::Result<String> {
        let command = serde_json::from_str::<AgentMessageCommand>(&command_json)
            .map_err(|error| napi::Error::new(napi::Status::InvalidArg, error.to_string()))?;
        let receipt = self
            .bridge()?
            .deliver_agent_message(command)
            .map_err(to_napi_error)?;
        serde_json::to_string(&receipt)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn begin_agent_round_json(&self, command_json: String) -> napi::Result<String> {
        let command = serde_json::from_str::<AgentRoundCommand>(&command_json)
            .map_err(|error| napi::Error::new(napi::Status::InvalidArg, error.to_string()))?;
        let receipt = self
            .bridge()?
            .begin_agent_round(command)
            .map_err(to_napi_error)?;
        serde_json::to_string(&receipt)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn get_agent_round_json(&self, round_id: String) -> napi::Result<Option<String>> {
        self.bridge()?
            .get_agent_round(&AgentRoundId::new(round_id))
            .map_err(to_napi_error)?
            .map(|round| {
                serde_json::to_string(&round).map_err(|error| {
                    napi::Error::new(napi::Status::GenericFailure, error.to_string())
                })
            })
            .transpose()
    }

    #[napi]
    pub fn get_agent_message_delivery_json(
        &self,
        delivery_id: String,
    ) -> napi::Result<Option<String>> {
        self.bridge()?
            .get_agent_message_delivery(&AgentMessageDeliveryId::new(delivery_id))
            .map_err(to_napi_error)?
            .map(|receipt| {
                serde_json::to_string(&receipt).map_err(|error| {
                    napi::Error::new(napi::Status::GenericFailure, error.to_string())
                })
            })
            .transpose()
    }
}
