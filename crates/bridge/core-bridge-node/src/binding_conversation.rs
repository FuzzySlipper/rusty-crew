use super::*;

#[napi_derive::napi]
impl NativeBridgeBinding {
    #[napi]
    pub fn save_message_slot_json(&self, input_json: String) -> napi::Result<()> {
        let bridge = self.bridge()?;
        let slot = parse_json::<MessageSlotWrite>(&input_json, "message slot write")?;
        bridge.save_message_slot(&slot).map_err(to_napi_error)
    }

    #[napi]
    pub fn save_message_variant_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let variant = parse_json::<MessageVariantWrite>(&input_json, "message variant write")?;
        let record = bridge
            .save_message_variant(&variant)
            .map_err(to_napi_error)?;
        serialize_json(&record, "message variant record")
    }

    #[napi]
    pub fn create_chat_message_slot_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let request = parse_json::<CreateChatMessageSlotRequest>(
            &input_json,
            "create chat message slot request",
        )?;
        let result = bridge
            .create_chat_message_slot(&request)
            .map_err(to_napi_error)?;
        serialize_json(&result, "create chat message slot result")
    }

    #[napi]
    pub fn query_message_slots_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let query = parse_json::<MessageSlotQuery>(&input_json, "message slot query")?;
        let records = bridge.query_message_slots(&query).map_err(to_napi_error)?;
        serialize_json(&records, "message slot records")
    }

    #[napi]
    pub fn query_message_variants_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let query = parse_json::<MessageVariantQuery>(&input_json, "message variant query")?;
        let records = bridge
            .query_message_variants(&query)
            .map_err(to_napi_error)?;
        serialize_json(&records, "message variant records")
    }

    #[napi]
    pub fn chat_read_model_page_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let query = parse_json::<ChatReadModelQuery>(&input_json, "chat read-model query")?;
        let page = bridge.chat_read_model_page(&query).map_err(to_napi_error)?;
        serialize_json(&page, "chat read-model page")
    }

    #[napi]
    pub fn append_chat_event_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let event = parse_json::<ChatEventLogAppend>(&input_json, "chat event append")?;
        let record = bridge.append_chat_event(&event).map_err(to_napi_error)?;
        serialize_json(&record, "chat event record")
    }

    #[napi]
    pub fn query_chat_events_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let query = parse_json::<ChatEventLogQuery>(&input_json, "chat event query")?;
        let page = bridge.query_chat_events(&query).map_err(to_napi_error)?;
        serialize_json(&page, "chat event page")
    }

    #[napi]
    pub fn save_conversation_branch_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let branch =
            parse_json::<ConversationBranchWrite>(&input_json, "conversation branch write")?;
        let record = bridge
            .save_conversation_branch(&branch)
            .map_err(to_napi_error)?;
        serialize_json(&record, "conversation branch record")
    }

    #[napi]
    pub fn query_conversation_branches_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let query =
            parse_json::<ConversationBranchQuery>(&input_json, "conversation branch query")?;
        let records = bridge
            .query_conversation_branches(&query)
            .map_err(to_napi_error)?;
        serialize_json(&records, "conversation branch records")
    }

    #[napi]
    pub fn get_conversation_branch_state_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let request = parse_json::<WireGetConversationBranchStateRequest>(
            &input_json,
            "get conversation branch state request",
        )?;
        let state = bridge
            .get_conversation_branch_state(&request.session_id, &request.default_updated_at)
            .map_err(to_napi_error)?;
        serialize_json(&state, "conversation branch state")
    }

    #[napi]
    pub fn select_active_conversation_branch_json(
        &self,
        input_json: String,
    ) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let request = parse_json::<SelectActiveBranchRequest>(
            &input_json,
            "select active conversation branch request",
        )?;
        let result = bridge
            .select_active_conversation_branch(&request)
            .map_err(to_napi_error)?;
        serialize_json(&result, "select active conversation branch result")
    }

    #[napi]
    pub fn update_conversation_branch_head_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let request = parse_json::<UpdateBranchHeadRequest>(
            &input_json,
            "update conversation branch head request",
        )?;
        let result = bridge
            .update_conversation_branch_head(&request)
            .map_err(to_napi_error)?;
        serialize_json(&result, "update conversation branch head result")
    }

    #[napi]
    pub fn save_conversation_snapshot_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let snapshot =
            parse_json::<ConversationSnapshotWrite>(&input_json, "conversation snapshot write")?;
        let record = bridge
            .save_conversation_snapshot(&snapshot)
            .map_err(to_napi_error)?;
        serialize_json(&record, "conversation snapshot record")
    }

    #[napi]
    pub fn query_conversation_snapshots_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let query =
            parse_json::<ConversationSnapshotQuery>(&input_json, "conversation snapshot query")?;
        let records = bridge
            .query_conversation_snapshots(&query)
            .map_err(to_napi_error)?;
        serialize_json(&records, "conversation snapshot records")
    }

    #[napi]
    pub fn resolve_conversation_jump_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let request =
            parse_json::<ConversationJumpRequest>(&input_json, "conversation jump request")?;
        let result = bridge
            .resolve_conversation_jump(&request)
            .map_err(to_napi_error)?;
        serialize_json(&result, "conversation jump result")
    }

    #[napi]
    pub fn save_attachment_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let attachment = parse_json::<AttachmentWrite>(&input_json, "attachment write")?;
        let record = bridge.save_attachment(&attachment).map_err(to_napi_error)?;
        serialize_json(&record, "attachment record")
    }

    #[napi]
    pub fn query_attachments_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let query = parse_json::<AttachmentQuery>(&input_json, "attachment query")?;
        let records = bridge.query_attachments(&query).map_err(to_napi_error)?;
        serialize_json(&records, "attachment records")
    }

    #[napi]
    pub fn remove_attachment_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let request =
            parse_json::<WireRemoveAttachmentRequest>(&input_json, "remove attachment request")?;
        let record = bridge
            .remove_attachment(&request.attachment_id, &request.updated_at)
            .map_err(to_napi_error)?;
        serialize_json(&record, "attachment record")
    }

    #[napi]
    pub fn save_data_bank_scope_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let scope = parse_json::<DataBankScopeWrite>(&input_json, "data-bank scope write")?;
        let record = bridge.save_data_bank_scope(&scope).map_err(to_napi_error)?;
        serialize_json(&record, "data-bank scope record")
    }

    #[napi]
    pub fn query_data_bank_scopes_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let query = parse_json::<DataBankScopeQuery>(&input_json, "data-bank scope query")?;
        let records = bridge
            .query_data_bank_scopes(&query)
            .map_err(to_napi_error)?;
        serialize_json(&records, "data-bank scope records")
    }

    #[napi]
    pub fn remove_data_bank_scope_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let request = parse_json::<WireRemoveDataBankScopeRequest>(
            &input_json,
            "remove data-bank scope request",
        )?;
        let record = bridge
            .remove_data_bank_scope(&request.scope_id, &request.updated_at)
            .map_err(to_napi_error)?;
        serialize_json(&record, "data-bank scope record")
    }

    #[napi]
    pub fn select_active_message_variant_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let request = parse_json::<SelectActiveVariantRequest>(
            &input_json,
            "select active message variant request",
        )?;
        let result = bridge
            .select_active_message_variant(&request)
            .map_err(to_napi_error)?;
        serialize_json(&result, "select active message variant result")
    }

    #[napi]
    pub fn select_active_chat_message_variant_json(
        &self,
        input_json: String,
    ) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let request = parse_json::<SelectActiveChatMessageVariantRequest>(
            &input_json,
            "select active chat message variant request",
        )?;
        let result = bridge
            .select_active_chat_message_variant(&request)
            .map_err(to_napi_error)?;
        serialize_json(&result, "select active chat message variant result")
    }

    #[napi]
    pub fn delete_message_variant_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let request = parse_json::<WireDeleteMessageVariantRequest>(
            &input_json,
            "delete message variant request",
        )?;
        let slot = bridge
            .delete_message_variant(&request.slot_id, &request.variant_id, &request.updated_at)
            .map_err(to_napi_error)?;
        serialize_json(&slot, "message slot record")
    }

    #[napi]
    pub fn reorder_message_variants_json(&self, input_json: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let request = parse_json::<WireReorderMessageVariantsRequest>(
            &input_json,
            "reorder message variants request",
        )?;
        let variants = bridge
            .reorder_message_variants(
                &request.slot_id,
                &request.ordered_variant_ids,
                &request.updated_at,
            )
            .map_err(to_napi_error)?;
        serialize_json(&variants, "message variant records")
    }
}
