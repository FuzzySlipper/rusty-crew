use rusty_crew_core_persistence::*;
use rusty_crew_core_protocol::{
    AttachmentId, CoreResult, DataBankScopeId, IsoTimestamp, MessageSlotId, MessageVariantId,
    SessionId,
};

pub(crate) trait ChatConversationStore {
    fn save_chat_message_slot(&self, slot: &MessageSlotWrite) -> CoreResult<()>;
    fn save_chat_message_variant(
        &self,
        variant: &MessageVariantWrite,
    ) -> CoreResult<MessageVariantRecord>;
    fn create_chat_message_slot(
        &self,
        request: &CreateChatMessageSlotRequest,
    ) -> CoreResult<CreateChatMessageSlotResult>;
    fn prune_chat_message_ingest_receipts(&self, now: &str) -> CoreResult<u64>;
    fn purge_chat_message_ingest_receipts_for_session(
        &self,
        session_id: &SessionId,
    ) -> CoreResult<u64>;
    fn create_chat_message_variant(
        &self,
        request: &CreateChatMessageVariantRequest,
    ) -> CoreResult<CreateChatMessageVariantResult>;
    fn apply_roleplay_alternative(
        &self,
        request: &ApplyRoleplayAlternativeRequest,
    ) -> CoreResult<ApplyRoleplayAlternativeResult>;
    fn delete_chat_message_variant(
        &self,
        request: &DeleteChatMessageVariantRequest,
    ) -> CoreResult<MessageSlotRecord>;
    fn reorder_chat_message_variants(
        &self,
        request: &ReorderChatMessageVariantsRequest,
    ) -> CoreResult<Vec<MessageVariantRecord>>;
    fn query_chat_message_slots(
        &self,
        query: &MessageSlotQuery,
    ) -> CoreResult<Vec<MessageSlotRecord>>;
    fn query_chat_message_slots_page(
        &self,
        query: &MessageSlotQuery,
    ) -> CoreResult<ExactPage<MessageSlotRecord>>;
    fn query_chat_message_variants(
        &self,
        query: &MessageVariantQuery,
    ) -> CoreResult<Vec<MessageVariantRecord>>;
    fn query_chat_message_variants_page(
        &self,
        query: &SessionMessageVariantPageQuery,
    ) -> CoreResult<ExactPage<MessageVariantRecord>>;
    fn save_chat_conversation_branch(
        &self,
        branch: &ConversationBranchWrite,
    ) -> CoreResult<ConversationBranchRecord>;
    fn query_chat_conversation_branches(
        &self,
        query: &ConversationBranchQuery,
    ) -> CoreResult<Vec<ConversationBranchRecord>>;
    fn create_chat_conversation_branch(
        &self,
        request: &CreateChatConversationBranchRequest,
    ) -> CoreResult<ConversationBranchRecord>;
    fn ensure_active_chat_conversation_branch(
        &self,
        request: &EnsureActiveChatConversationBranchRequest,
    ) -> CoreResult<EnsureActiveChatConversationBranchResult>;
    fn get_chat_conversation_branch_state(
        &self,
        session_id: &SessionId,
        default_updated_at: &IsoTimestamp,
    ) -> CoreResult<ConversationBranchStateRecord>;
    fn select_active_chat_conversation_branch(
        &self,
        request: &SelectActiveBranchRequest,
    ) -> CoreResult<SelectActiveBranchResult>;
    fn update_chat_conversation_branch_head(
        &self,
        request: &UpdateBranchHeadRequest,
    ) -> CoreResult<UpdateBranchHeadResult>;
    fn save_chat_conversation_snapshot(
        &self,
        snapshot: &ConversationSnapshotWrite,
    ) -> CoreResult<ConversationSnapshotRecord>;
    fn create_chat_conversation_snapshot(
        &self,
        request: &CreateChatConversationSnapshotRequest,
    ) -> CoreResult<CreateChatConversationSnapshotResult>;
    fn query_chat_conversation_snapshots(
        &self,
        query: &ConversationSnapshotQuery,
    ) -> CoreResult<Vec<ConversationSnapshotRecord>>;
    fn read_chat_conversation_tree(
        &self,
        query: &ConversationTreeReadQuery,
    ) -> CoreResult<ConversationTreeReadResult>;
    fn search_chat_transcript(
        &self,
        query: &ChatTranscriptSearchQuery,
    ) -> CoreResult<ChatTranscriptSearchPage>;
    fn resolve_chat_conversation_jump(
        &self,
        request: &ConversationJumpRequest,
    ) -> CoreResult<ConversationJumpResult>;
    fn save_chat_attachment(&self, attachment: &AttachmentWrite) -> CoreResult<AttachmentRecord>;
    fn create_chat_attachment(
        &self,
        request: &CreateChatAttachmentRequest,
    ) -> CoreResult<CreateChatAttachmentResult>;
    fn query_chat_attachments(&self, query: &AttachmentQuery) -> CoreResult<Vec<AttachmentRecord>>;
    fn query_chat_attachments_page(
        &self,
        query: &AttachmentQuery,
    ) -> CoreResult<ExactPage<AttachmentRecord>>;
    fn remove_attachment(
        &self,
        attachment_id: &AttachmentId,
        updated_at: &IsoTimestamp,
    ) -> CoreResult<AttachmentRecord>;
    fn remove_chat_attachment(
        &self,
        request: &RemoveChatAttachmentRequest,
    ) -> CoreResult<AttachmentRecord>;
    fn save_chat_data_bank_scope(
        &self,
        scope: &DataBankScopeWrite,
    ) -> CoreResult<DataBankScopeRecord>;
    fn create_chat_data_bank_scope(
        &self,
        request: &CreateChatDataBankScopeRequest,
    ) -> CoreResult<CreateChatDataBankScopeResult>;
    fn query_chat_data_bank_scopes(
        &self,
        query: &DataBankScopeQuery,
    ) -> CoreResult<Vec<DataBankScopeRecord>>;
    fn query_chat_data_bank_scopes_page(
        &self,
        query: &DataBankScopeQuery,
    ) -> CoreResult<ExactPage<DataBankScopeRecord>>;
    fn remove_data_bank_scope(
        &self,
        scope_id: &DataBankScopeId,
        updated_at: &IsoTimestamp,
    ) -> CoreResult<DataBankScopeRecord>;
    fn remove_chat_data_bank_scope(
        &self,
        request: &RemoveChatDataBankScopeRequest,
    ) -> CoreResult<DataBankScopeRecord>;
    fn select_active_chat_message_variant_store(
        &self,
        request: &SelectActiveVariantRequest,
    ) -> CoreResult<SelectActiveVariantResult>;
    fn delete_message_variant(
        &self,
        slot_id: &MessageSlotId,
        variant_id: &MessageVariantId,
        updated_at: &IsoTimestamp,
    ) -> CoreResult<MessageSlotRecord>;
    fn reorder_message_variants(
        &self,
        slot_id: &MessageSlotId,
        ordered_variant_ids: &[MessageVariantId],
        updated_at: &IsoTimestamp,
    ) -> CoreResult<Vec<MessageVariantRecord>>;
}

pub(crate) trait ChatEventStore {
    fn append_chat_event_log(&self, event: &ChatEventLogAppend) -> CoreResult<ChatEventLogEvent>;
    fn query_chat_event_log(&self, query: &ChatEventLogQuery) -> CoreResult<ChatEventLogPage>;
}

impl ChatConversationStore for CoreCoordinationStore {
    fn save_chat_message_slot(&self, slot: &MessageSlotWrite) -> CoreResult<()> {
        self.conversation().save_message_slot(slot)
    }

    fn save_chat_message_variant(
        &self,
        variant: &MessageVariantWrite,
    ) -> CoreResult<MessageVariantRecord> {
        self.conversation().save_message_variant(variant)
    }

    fn create_chat_message_slot(
        &self,
        request: &CreateChatMessageSlotRequest,
    ) -> CoreResult<CreateChatMessageSlotResult> {
        self.conversation().create_chat_message_slot(request)
    }

    fn prune_chat_message_ingest_receipts(&self, now: &str) -> CoreResult<u64> {
        self.conversation().prune_chat_message_ingest_receipts(now)
    }

    fn purge_chat_message_ingest_receipts_for_session(
        &self,
        session_id: &SessionId,
    ) -> CoreResult<u64> {
        self.conversation()
            .purge_chat_message_ingest_receipts_for_session(session_id)
    }

    fn create_chat_message_variant(
        &self,
        request: &CreateChatMessageVariantRequest,
    ) -> CoreResult<CreateChatMessageVariantResult> {
        self.conversation().create_chat_message_variant(request)
    }
    fn apply_roleplay_alternative(
        &self,
        request: &ApplyRoleplayAlternativeRequest,
    ) -> CoreResult<ApplyRoleplayAlternativeResult> {
        self.apply_roleplay_alternative(request)
    }

    fn delete_chat_message_variant(
        &self,
        request: &DeleteChatMessageVariantRequest,
    ) -> CoreResult<MessageSlotRecord> {
        self.conversation().delete_chat_message_variant(request)
    }

    fn reorder_chat_message_variants(
        &self,
        request: &ReorderChatMessageVariantsRequest,
    ) -> CoreResult<Vec<MessageVariantRecord>> {
        self.conversation().reorder_chat_message_variants(request)
    }

    fn query_chat_message_slots(
        &self,
        query: &MessageSlotQuery,
    ) -> CoreResult<Vec<MessageSlotRecord>> {
        self.conversation().query_message_slots(query)
    }

    fn query_chat_message_slots_page(
        &self,
        query: &MessageSlotQuery,
    ) -> CoreResult<ExactPage<MessageSlotRecord>> {
        self.conversation().query_message_slots_page(query)
    }

    fn query_chat_message_variants(
        &self,
        query: &MessageVariantQuery,
    ) -> CoreResult<Vec<MessageVariantRecord>> {
        self.conversation().query_message_variants(query)
    }

    fn query_chat_message_variants_page(
        &self,
        query: &SessionMessageVariantPageQuery,
    ) -> CoreResult<ExactPage<MessageVariantRecord>> {
        self.conversation().query_message_variants_page(query)
    }

    fn save_chat_conversation_branch(
        &self,
        branch: &ConversationBranchWrite,
    ) -> CoreResult<ConversationBranchRecord> {
        self.conversation().save_conversation_branch(branch)
    }

    fn query_chat_conversation_branches(
        &self,
        query: &ConversationBranchQuery,
    ) -> CoreResult<Vec<ConversationBranchRecord>> {
        self.conversation().query_conversation_branches(query)
    }

    fn create_chat_conversation_branch(
        &self,
        request: &CreateChatConversationBranchRequest,
    ) -> CoreResult<ConversationBranchRecord> {
        self.conversation().create_chat_conversation_branch(request)
    }

    fn ensure_active_chat_conversation_branch(
        &self,
        request: &EnsureActiveChatConversationBranchRequest,
    ) -> CoreResult<EnsureActiveChatConversationBranchResult> {
        self.conversation()
            .ensure_active_chat_conversation_branch(request)
    }

    fn get_chat_conversation_branch_state(
        &self,
        session_id: &SessionId,
        default_updated_at: &IsoTimestamp,
    ) -> CoreResult<ConversationBranchStateRecord> {
        self.conversation()
            .get_conversation_branch_state(session_id, default_updated_at)
    }

    fn select_active_chat_conversation_branch(
        &self,
        request: &SelectActiveBranchRequest,
    ) -> CoreResult<SelectActiveBranchResult> {
        self.conversation()
            .select_active_conversation_branch(request)
    }

    fn update_chat_conversation_branch_head(
        &self,
        request: &UpdateBranchHeadRequest,
    ) -> CoreResult<UpdateBranchHeadResult> {
        self.conversation().update_conversation_branch_head(request)
    }

    fn save_chat_conversation_snapshot(
        &self,
        snapshot: &ConversationSnapshotWrite,
    ) -> CoreResult<ConversationSnapshotRecord> {
        self.conversation().save_conversation_snapshot(snapshot)
    }

    fn create_chat_conversation_snapshot(
        &self,
        request: &CreateChatConversationSnapshotRequest,
    ) -> CoreResult<CreateChatConversationSnapshotResult> {
        self.conversation()
            .create_chat_conversation_snapshot(request)
    }

    fn query_chat_conversation_snapshots(
        &self,
        query: &ConversationSnapshotQuery,
    ) -> CoreResult<Vec<ConversationSnapshotRecord>> {
        self.conversation().query_conversation_snapshots(query)
    }

    fn read_chat_conversation_tree(
        &self,
        query: &ConversationTreeReadQuery,
    ) -> CoreResult<ConversationTreeReadResult> {
        self.conversation().read_conversation_tree(query)
    }

    fn search_chat_transcript(
        &self,
        query: &ChatTranscriptSearchQuery,
    ) -> CoreResult<ChatTranscriptSearchPage> {
        self.conversation().search_chat_transcript(query)
    }

    fn resolve_chat_conversation_jump(
        &self,
        request: &ConversationJumpRequest,
    ) -> CoreResult<ConversationJumpResult> {
        self.conversation().resolve_conversation_jump(request)
    }

    fn save_chat_attachment(&self, attachment: &AttachmentWrite) -> CoreResult<AttachmentRecord> {
        self.conversation().save_attachment(attachment)
    }

    fn create_chat_attachment(
        &self,
        request: &CreateChatAttachmentRequest,
    ) -> CoreResult<CreateChatAttachmentResult> {
        self.conversation().create_chat_attachment(request)
    }

    fn query_chat_attachments(&self, query: &AttachmentQuery) -> CoreResult<Vec<AttachmentRecord>> {
        self.conversation().query_attachments(query)
    }

    fn query_chat_attachments_page(
        &self,
        query: &AttachmentQuery,
    ) -> CoreResult<ExactPage<AttachmentRecord>> {
        self.conversation().query_attachments_page(query)
    }

    fn remove_attachment(
        &self,
        attachment_id: &AttachmentId,
        updated_at: &IsoTimestamp,
    ) -> CoreResult<AttachmentRecord> {
        self.conversation()
            .remove_attachment(attachment_id, updated_at)
    }

    fn remove_chat_attachment(
        &self,
        request: &RemoveChatAttachmentRequest,
    ) -> CoreResult<AttachmentRecord> {
        self.conversation().remove_chat_attachment(request)
    }

    fn save_chat_data_bank_scope(
        &self,
        scope: &DataBankScopeWrite,
    ) -> CoreResult<DataBankScopeRecord> {
        self.conversation().save_data_bank_scope(scope)
    }

    fn create_chat_data_bank_scope(
        &self,
        request: &CreateChatDataBankScopeRequest,
    ) -> CoreResult<CreateChatDataBankScopeResult> {
        self.conversation().create_chat_data_bank_scope(request)
    }

    fn query_chat_data_bank_scopes(
        &self,
        query: &DataBankScopeQuery,
    ) -> CoreResult<Vec<DataBankScopeRecord>> {
        self.conversation().query_data_bank_scopes(query)
    }

    fn query_chat_data_bank_scopes_page(
        &self,
        query: &DataBankScopeQuery,
    ) -> CoreResult<ExactPage<DataBankScopeRecord>> {
        self.conversation().query_data_bank_scopes_page(query)
    }

    fn remove_data_bank_scope(
        &self,
        scope_id: &DataBankScopeId,
        updated_at: &IsoTimestamp,
    ) -> CoreResult<DataBankScopeRecord> {
        self.conversation()
            .remove_data_bank_scope(scope_id, updated_at)
    }

    fn remove_chat_data_bank_scope(
        &self,
        request: &RemoveChatDataBankScopeRequest,
    ) -> CoreResult<DataBankScopeRecord> {
        self.conversation().remove_chat_data_bank_scope(request)
    }

    fn select_active_chat_message_variant_store(
        &self,
        request: &SelectActiveVariantRequest,
    ) -> CoreResult<SelectActiveVariantResult> {
        self.conversation().select_active_message_variant(request)
    }

    fn delete_message_variant(
        &self,
        slot_id: &MessageSlotId,
        variant_id: &MessageVariantId,
        updated_at: &IsoTimestamp,
    ) -> CoreResult<MessageSlotRecord> {
        self.conversation()
            .delete_message_variant(slot_id, variant_id, updated_at)
    }

    fn reorder_message_variants(
        &self,
        slot_id: &MessageSlotId,
        ordered_variant_ids: &[MessageVariantId],
        updated_at: &IsoTimestamp,
    ) -> CoreResult<Vec<MessageVariantRecord>> {
        self.conversation()
            .reorder_message_variants(slot_id, ordered_variant_ids, updated_at)
    }
}

impl ChatEventStore for CoreCoordinationStore {
    fn append_chat_event_log(&self, event: &ChatEventLogAppend) -> CoreResult<ChatEventLogEvent> {
        self.chat_events().append_chat_event(event)
    }

    fn query_chat_event_log(&self, query: &ChatEventLogQuery) -> CoreResult<ChatEventLogPage> {
        self.chat_events().query_chat_events(query)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeChatEventStore {
        events: Mutex<Vec<ChatEventLogEvent>>,
    }

    impl ChatEventStore for FakeChatEventStore {
        fn append_chat_event_log(
            &self,
            event: &ChatEventLogAppend,
        ) -> CoreResult<ChatEventLogEvent> {
            let mut events = self.events.lock().unwrap();
            let sequence_id = events.len() as u64 + 1;
            let record = ChatEventLogEvent {
                event_id: format!("chat-event-{sequence_id}"),
                session_id: event.session_id.clone(),
                sequence_id,
                created_at: event.created_at.clone(),
                kind: event.kind.clone(),
                payload_json: event.payload_json.clone(),
            };
            events.push(record.clone());
            Ok(record)
        }

        fn query_chat_event_log(&self, query: &ChatEventLogQuery) -> CoreResult<ChatEventLogPage> {
            let events = self.events.lock().unwrap();
            let after = query
                .cursor
                .as_deref()
                .and_then(|cursor| cursor.rsplit(':').next())
                .and_then(|sequence| sequence.parse::<u64>().ok())
                .unwrap_or(0);
            let limit = query.limit.unwrap_or(100).clamp(1, 500) as usize;
            let mut items = events
                .iter()
                .filter(|event| event.session_id == query.session_id)
                .filter(|event| event.sequence_id > after)
                .cloned()
                .collect::<Vec<_>>();
            let has_more = items.len() > limit;
            items.truncate(limit);
            let latest = items.last().map(|event| event.sequence_id).unwrap_or(after);
            Ok(ChatEventLogPage {
                items,
                latest_cursor: format!("{}:{latest}", query.session_id),
                has_more,
                total: events
                    .iter()
                    .filter(|event| event.session_id == query.session_id)
                    .count() as u64,
                message_count: events
                    .iter()
                    .filter(|event| {
                        event.session_id == query.session_id && event.kind == "message_created"
                    })
                    .count() as u64,
                has_more_before: after > 0,
            })
        }
    }

    #[test]
    fn chat_event_port_uses_fake_store_without_database() {
        let store = FakeChatEventStore::default();
        let session_id = SessionId::new("prime-session");

        let first = ChatEventStore::append_chat_event_log(
            &store,
            &ChatEventLogAppend {
                session_id: session_id.clone(),
                created_at: "2026-07-09T09:00:00Z".to_string(),
                kind: "message_created".to_string(),
                payload_json: json!({"message_id": "message-1"}),
            },
        )
        .unwrap();
        ChatEventStore::append_chat_event_log(
            &store,
            &ChatEventLogAppend {
                session_id: session_id.clone(),
                created_at: "2026-07-09T09:00:01Z".to_string(),
                kind: "message_completed".to_string(),
                payload_json: json!({"message_id": "message-1"}),
            },
        )
        .unwrap();

        let page = ChatEventStore::query_chat_event_log(
            &store,
            &ChatEventLogQuery {
                session_id,
                cursor: Some(format!("{}:{}", first.session_id, first.sequence_id)),
                limit: Some(1),
            },
        )
        .unwrap();

        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].kind, "message_completed");
        assert!(!page.has_more);
    }
}
