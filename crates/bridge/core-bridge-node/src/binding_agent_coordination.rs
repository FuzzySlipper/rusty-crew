use super::*;

impl NativeBridge {
    pub fn list_agent_directory(&self) -> CoreResult<Vec<AgentDirectoryEntry>> {
        self.engine()?.list_agent_directory()
    }

    pub fn list_agent_route_resolutions(&self) -> CoreResult<Vec<AgentRouteResolution>> {
        self.engine()?.list_agent_route_resolutions()
    }

    pub fn get_agent_route_resolution(
        &self,
        route_key: &AgentRouteKey,
    ) -> CoreResult<Option<AgentRouteResolution>> {
        self.engine()?.get_agent_route_resolution(route_key)
    }

    pub fn resolve_agent_address(&self, address: &str) -> CoreResult<AgentRouteResolution> {
        self.engine()?.resolve_agent_address(address)
    }

    pub fn put_agent_route(&self, write: AgentRouteWrite) -> CoreResult<AgentRouteRecord> {
        self.engine()?.put_agent_route(write)
    }

    pub fn delete_agent_route(&self, delete: AgentRouteDelete) -> CoreResult<AgentRouteRecord> {
        self.engine()?.delete_agent_route(delete)
    }

    pub fn deliver_agent_message(
        &self,
        command: AgentMessageCommand,
    ) -> CoreResult<AgentMessageDeliveryReceipt> {
        self.engine()?.deliver_agent_message(command)
    }

    pub fn reply_agent_message(
        &self,
        command: AgentMessageReplyCommand,
    ) -> CoreResult<AgentMessageDeliveryReceipt> {
        self.engine()?.reply_agent_message(command)
    }

    pub fn list_agent_message_inbox(
        &self,
        query: &AgentMessageInboxQuery,
    ) -> CoreResult<Vec<AgentMessageInboxItem>> {
        self.engine()?.list_agent_message_inbox(query)
    }

    pub fn list_agent_message_traffic(
        &self,
        query: &AgentMessageInboxQuery,
    ) -> CoreResult<Vec<AgentMessageTrafficItem>> {
        self.engine()?.list_agent_message_traffic(query)
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

    pub fn complete_agent_message_delivery(
        &self,
        completion: AgentMessageDeliveryCompletion,
    ) -> CoreResult<AgentMessageDeliveryReceipt> {
        self.engine()?.complete_agent_message_delivery(completion)
    }
}

#[napi_derive::napi]
impl NativeBridgeBinding {
    #[napi]
    pub fn list_agent_directory_json(&self) -> napi::Result<String> {
        let entries = self
            .bridge()?
            .list_agent_directory()
            .map_err(to_napi_error)?;
        serde_json::to_string(&entries)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn list_agent_route_resolutions_json(&self) -> napi::Result<String> {
        let routes = self
            .bridge()?
            .list_agent_route_resolutions()
            .map_err(to_napi_error)?;
        serde_json::to_string(&routes)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn get_agent_route_resolution_json(
        &self,
        route_key: String,
    ) -> napi::Result<Option<String>> {
        self.bridge()?
            .get_agent_route_resolution(&AgentRouteKey::new(route_key))
            .map_err(to_napi_error)?
            .map(|route| {
                serde_json::to_string(&route).map_err(|error| {
                    napi::Error::new(napi::Status::GenericFailure, error.to_string())
                })
            })
            .transpose()
    }

    #[napi]
    pub fn resolve_agent_address_json(&self, address: String) -> napi::Result<String> {
        let resolution = self
            .bridge()?
            .resolve_agent_address(&address)
            .map_err(to_napi_error)?;
        serde_json::to_string(&resolution)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn put_agent_route_json(&self, write_json: String) -> napi::Result<String> {
        let write = serde_json::from_str::<AgentRouteWrite>(&write_json)
            .map_err(|error| napi::Error::new(napi::Status::InvalidArg, error.to_string()))?;
        let route = self
            .bridge()?
            .put_agent_route(write)
            .map_err(to_napi_error)?;
        serde_json::to_string(&route)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn delete_agent_route_json(&self, delete_json: String) -> napi::Result<String> {
        let delete = serde_json::from_str::<AgentRouteDelete>(&delete_json)
            .map_err(|error| napi::Error::new(napi::Status::InvalidArg, error.to_string()))?;
        let route = self
            .bridge()?
            .delete_agent_route(delete)
            .map_err(to_napi_error)?;
        serde_json::to_string(&route)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

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
    pub fn reply_agent_message_json(&self, command_json: String) -> napi::Result<String> {
        let command = serde_json::from_str::<AgentMessageReplyCommand>(&command_json)
            .map_err(|error| napi::Error::new(napi::Status::InvalidArg, error.to_string()))?;
        let receipt = self
            .bridge()?
            .reply_agent_message(command)
            .map_err(to_napi_error)?;
        serde_json::to_string(&receipt)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn list_agent_message_inbox_json(&self, query_json: String) -> napi::Result<String> {
        let query = serde_json::from_str::<AgentMessageInboxQuery>(&query_json)
            .map_err(|error| napi::Error::new(napi::Status::InvalidArg, error.to_string()))?;
        let items = self
            .bridge()?
            .list_agent_message_inbox(&query)
            .map_err(to_napi_error)?;
        serde_json::to_string(&items)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn list_agent_message_traffic_json(&self, query_json: String) -> napi::Result<String> {
        let query = serde_json::from_str::<AgentMessageInboxQuery>(&query_json)
            .map_err(|error| napi::Error::new(napi::Status::InvalidArg, error.to_string()))?;
        let items = self
            .bridge()?
            .list_agent_message_traffic(&query)
            .map_err(to_napi_error)?;
        serde_json::to_string(&items)
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

    #[napi]
    pub fn complete_agent_message_delivery_json(
        &self,
        completion_json: String,
    ) -> napi::Result<String> {
        let completion =
            serde_json::from_str::<AgentMessageDeliveryCompletion>(&completion_json)
                .map_err(|error| napi::Error::new(napi::Status::InvalidArg, error.to_string()))?;
        let receipt = self
            .bridge()?
            .complete_agent_message_delivery(completion)
            .map_err(to_napi_error)?;
        serde_json::to_string(&receipt)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }
}
