use super::*;

impl NativeBridge {
    pub fn save_message_slot(&self, slot: &MessageSlotWrite) -> CoreResult<()> {
        self.engine()?.save_message_slot(slot)
    }

    pub fn save_message_variant(
        &self,
        variant: &MessageVariantWrite,
    ) -> CoreResult<MessageVariantRecord> {
        self.engine()?.save_message_variant(variant)
    }

    pub fn query_message_slots(
        &self,
        query: &MessageSlotQuery,
    ) -> CoreResult<Vec<MessageSlotRecord>> {
        self.engine()?.query_message_slots(query)
    }

    pub fn query_message_variants(
        &self,
        query: &MessageVariantQuery,
    ) -> CoreResult<Vec<MessageVariantRecord>> {
        self.engine()?.query_message_variants(query)
    }

    pub fn chat_read_model_page(
        &self,
        query: &ChatReadModelQuery,
    ) -> CoreResult<ChatReadModelPage> {
        self.engine()?.chat_read_model_page(query)
    }

    pub fn save_conversation_branch(
        &self,
        branch: &ConversationBranchWrite,
    ) -> CoreResult<ConversationBranchRecord> {
        self.engine()?.save_conversation_branch(branch)
    }

    pub fn query_conversation_branches(
        &self,
        query: &ConversationBranchQuery,
    ) -> CoreResult<Vec<ConversationBranchRecord>> {
        self.engine()?.query_conversation_branches(query)
    }

    pub fn get_conversation_branch_state(
        &self,
        session_id: &SessionId,
        default_updated_at: &rusty_crew_core_bridge_api::IsoTimestamp,
    ) -> CoreResult<ConversationBranchStateRecord> {
        self.engine()?
            .get_conversation_branch_state(session_id, default_updated_at)
    }

    pub fn select_active_conversation_branch(
        &self,
        request: &SelectActiveBranchRequest,
    ) -> CoreResult<SelectActiveBranchResult> {
        self.engine()?.select_active_conversation_branch(request)
    }

    pub fn update_conversation_branch_head(
        &self,
        request: &UpdateBranchHeadRequest,
    ) -> CoreResult<UpdateBranchHeadResult> {
        self.engine()?.update_conversation_branch_head(request)
    }

    pub fn save_conversation_snapshot(
        &self,
        snapshot: &ConversationSnapshotWrite,
    ) -> CoreResult<ConversationSnapshotRecord> {
        self.engine()?.save_conversation_snapshot(snapshot)
    }

    pub fn query_conversation_snapshots(
        &self,
        query: &ConversationSnapshotQuery,
    ) -> CoreResult<Vec<ConversationSnapshotRecord>> {
        self.engine()?.query_conversation_snapshots(query)
    }

    pub fn resolve_conversation_jump(
        &self,
        request: &ConversationJumpRequest,
    ) -> CoreResult<ConversationJumpResult> {
        self.engine()?.resolve_conversation_jump(request)
    }

    pub fn save_attachment(&self, attachment: &AttachmentWrite) -> CoreResult<AttachmentRecord> {
        self.engine()?.save_attachment(attachment)
    }

    pub fn query_attachments(&self, query: &AttachmentQuery) -> CoreResult<Vec<AttachmentRecord>> {
        self.engine()?.query_attachments(query)
    }

    pub fn remove_attachment(
        &self,
        attachment_id: &AttachmentId,
        updated_at: &rusty_crew_core_bridge_api::IsoTimestamp,
    ) -> CoreResult<AttachmentRecord> {
        self.engine()?.remove_attachment(attachment_id, updated_at)
    }

    pub fn save_data_bank_scope(
        &self,
        scope: &DataBankScopeWrite,
    ) -> CoreResult<DataBankScopeRecord> {
        self.engine()?.save_data_bank_scope(scope)
    }

    pub fn query_data_bank_scopes(
        &self,
        query: &DataBankScopeQuery,
    ) -> CoreResult<Vec<DataBankScopeRecord>> {
        self.engine()?.query_data_bank_scopes(query)
    }

    pub fn remove_data_bank_scope(
        &self,
        scope_id: &DataBankScopeId,
        updated_at: &rusty_crew_core_bridge_api::IsoTimestamp,
    ) -> CoreResult<DataBankScopeRecord> {
        self.engine()?.remove_data_bank_scope(scope_id, updated_at)
    }

    pub fn select_active_message_variant(
        &self,
        request: &SelectActiveVariantRequest,
    ) -> CoreResult<SelectActiveVariantResult> {
        self.engine()?.select_active_message_variant(request)
    }

    pub fn delete_message_variant(
        &self,
        slot_id: &MessageSlotId,
        variant_id: &MessageVariantId,
        updated_at: &rusty_crew_core_bridge_api::IsoTimestamp,
    ) -> CoreResult<MessageSlotRecord> {
        self.engine()?
            .delete_message_variant(slot_id, variant_id, updated_at)
    }

    pub fn reorder_message_variants(
        &self,
        slot_id: &MessageSlotId,
        ordered_variant_ids: &[MessageVariantId],
        updated_at: &rusty_crew_core_bridge_api::IsoTimestamp,
    ) -> CoreResult<Vec<MessageVariantRecord>> {
        self.engine()?
            .reorder_message_variants(slot_id, ordered_variant_ids, updated_at)
    }
}
