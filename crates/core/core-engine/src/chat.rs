use super::*;

impl CoreEngine {
    pub fn save_message_slot(&self, slot: &MessageSlotWrite) -> CoreResult<()> {
        self.store.save_chat_message_slot(slot)
    }

    pub fn save_message_variant(
        &self,
        variant: &MessageVariantWrite,
    ) -> CoreResult<MessageVariantRecord> {
        self.store.save_chat_message_variant(variant)
    }

    pub fn create_chat_message_slot(
        &self,
        request: &CreateChatMessageSlotRequest,
    ) -> CoreResult<CreateChatMessageSlotResult> {
        ChatConversationStore::create_chat_message_slot(&self.store, request)
    }

    pub fn prune_chat_message_ingest_receipts(&self, now: &str) -> CoreResult<u64> {
        ChatConversationStore::prune_chat_message_ingest_receipts(&self.store, now)
    }

    pub fn purge_chat_message_ingest_receipts_for_session(
        &self,
        session_id: &SessionId,
    ) -> CoreResult<u64> {
        ChatConversationStore::purge_chat_message_ingest_receipts_for_session(
            &self.store,
            session_id,
        )
    }

    pub fn create_chat_message_variant(
        &self,
        request: &CreateChatMessageVariantRequest,
    ) -> CoreResult<CreateChatMessageVariantResult> {
        ChatConversationStore::create_chat_message_variant(&self.store, request)
    }
    pub fn apply_roleplay_alternative(
        &self,
        request: &ApplyRoleplayAlternativeRequest,
    ) -> CoreResult<ApplyRoleplayAlternativeResult> {
        ChatConversationStore::apply_roleplay_alternative(&self.store, request)
    }

    pub fn delete_chat_message_variant(
        &self,
        request: &DeleteChatMessageVariantRequest,
    ) -> CoreResult<MessageSlotRecord> {
        ChatConversationStore::delete_chat_message_variant(&self.store, request)
    }

    pub fn reorder_chat_message_variants(
        &self,
        request: &ReorderChatMessageVariantsRequest,
    ) -> CoreResult<Vec<MessageVariantRecord>> {
        ChatConversationStore::reorder_chat_message_variants(&self.store, request)
    }

    pub fn query_message_slots(
        &self,
        query: &MessageSlotQuery,
    ) -> CoreResult<Vec<MessageSlotRecord>> {
        self.store.query_chat_message_slots(query)
    }

    pub fn query_message_slots_page(
        &self,
        query: &MessageSlotQuery,
    ) -> CoreResult<ExactPage<MessageSlotRecord>> {
        self.store.query_chat_message_slots_page(query)
    }

    pub fn query_message_variants(
        &self,
        query: &MessageVariantQuery,
    ) -> CoreResult<Vec<MessageVariantRecord>> {
        self.store.query_chat_message_variants(query)
    }

    pub fn query_message_variants_page(
        &self,
        query: &SessionMessageVariantPageQuery,
    ) -> CoreResult<ExactPage<MessageVariantRecord>> {
        self.store.query_chat_message_variants_page(query)
    }

    pub fn chat_read_model_page(
        &self,
        query: &ChatReadModelQuery,
    ) -> CoreResult<ChatReadModelPage> {
        let after = chat_cursor_sequence(query.cursor.as_deref(), &query.session_id);
        let limit = normalize_chat_read_model_limit(query.limit);
        let offset = after.min(u32::MAX as u64) as u32;
        let slots = self
            .store
            .query_chat_message_slots_page(&MessageSlotQuery {
                session_id: Some(query.session_id.clone()),
                include_alternates: true,
                page: Some(rusty_crew_core_persistence::QueryPage {
                    limit: Some(limit.max(1)),
                    offset: Some(offset),
                }),
            })?;
        if slots.total > 0 {
            let items = slots
                .items
                .into_iter()
                .take(limit as usize)
                .enumerate()
                .map(|(index, slot)| {
                    chat_read_model_event_from_slot(
                        &query.session_id,
                        &query.agent_id,
                        after + index as u64 + 1,
                        &slot,
                    )
                })
                .collect::<Vec<_>>();
            let latest_sequence = items.last().map(|event| event.sequence_id).unwrap_or(after);
            return Ok(ChatReadModelPage {
                items,
                latest_cursor: chat_cursor_for(&query.session_id, latest_sequence),
                has_more: u64::from(offset).saturating_add(u64::from(limit)) < slots.total,
                total: slots.total,
                source: ChatReadModelSource::MessageSlots,
            });
        }

        let body = self.project_body_state(&query.session_id)?;
        let total = body.pending_messages.len() as u64;
        let session = body.session;
        let created_at = self.now();
        let items = body
            .pending_messages
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .enumerate()
            .map(|(index, message)| {
                pending_message_event(
                    &session,
                    after + index as u64 + 1,
                    &message,
                    created_at.clone(),
                )
            })
            .collect::<Vec<_>>();
        let latest_sequence = items.last().map(|event| event.sequence_id).unwrap_or(after);
        Ok(ChatReadModelPage {
            items,
            latest_cursor: chat_cursor_for(&query.session_id, latest_sequence),
            has_more: u64::from(offset).saturating_add(u64::from(limit)) < total,
            total,
            source: if total == 0 {
                ChatReadModelSource::Empty
            } else {
                ChatReadModelSource::PendingMessages
            },
        })
    }

    pub fn read_chat_session(
        &self,
        query: &ChatSessionReadQuery,
    ) -> CoreResult<ChatSessionReadResult> {
        let session = self.get_session(&query.session_id)?;
        let event_page = self.query_chat_events(&ChatEventLogQuery {
            session_id: query.session_id.clone(),
            cursor: query.cursor.clone(),
            limit: Some(query.limit),
        })?;
        let message_slots = self.query_message_slots_page(&MessageSlotQuery {
            session_id: Some(query.session_id.clone()),
            include_alternates: query.include_alternates,
            page: Some(rusty_crew_core_persistence::QueryPage {
                limit: Some(query.limit.max(1)),
                offset: Some(0),
            }),
        })?;
        if event_page.total > 0 {
            return Ok(ChatSessionReadResult {
                execution: self.session_execution_state(&session.session_id)?,
                session,
                events: event_page.items,
                latest_cursor: event_page.latest_cursor,
                has_more: event_page.has_more,
                has_more_before: event_page.has_more_before,
                total: event_page.total,
                message_count: event_page.message_count,
                source: ChatReadModelSource::EventLog,
                message_slots,
            });
        }
        let read_model = self.chat_read_model_page(&ChatReadModelQuery {
            session_id: query.session_id.clone(),
            agent_id: session.agent_id.to_string(),
            cursor: query.cursor.clone(),
            limit: Some(query.limit),
        })?;
        Ok(ChatSessionReadResult {
            execution: self.session_execution_state(&session.session_id)?,
            session,
            events: read_model
                .items
                .into_iter()
                .map(chat_read_model_event_as_log_event)
                .collect(),
            latest_cursor: read_model.latest_cursor,
            has_more: read_model.has_more,
            has_more_before: false,
            total: read_model.total,
            message_count: read_model.total,
            source: read_model.source,
            message_slots,
        })
    }

    pub fn query_chat_session_summaries(
        &self,
        query: &ChatSessionSummaryPageQuery,
    ) -> CoreResult<ChatSessionSummaryPage> {
        let mut sessions = self.list_sessions()?;
        sessions.retain(|session| {
            query
                .profile_id
                .as_ref()
                .is_none_or(|profile_id| &session.profile_id == profile_id)
                && match query.status.as_deref() {
                    Some(status) => session_status_wire_value(&session.status) == status,
                    None => session.status != SessionStatus::Archived,
                }
        });
        sessions.sort_by(|left, right| left.session_id.0.cmp(&right.session_id.0));
        let total = sessions.len() as u64;
        let limit = query.page.limit.unwrap_or(100).clamp(1, 500);
        let offset = query.page.offset.unwrap_or(0);
        let items = sessions
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .map(|session| {
                let event_page = self.query_chat_events(&ChatEventLogQuery {
                    session_id: session.session_id.clone(),
                    cursor: None,
                    limit: Some(0),
                })?;
                if event_page.total > 0 {
                    return Ok(ChatSessionReadFacts {
                        execution: self.session_execution_state(&session.session_id)?,
                        session,
                        message_count: event_page.message_count,
                        latest_cursor: event_page.latest_cursor,
                        source: ChatReadModelSource::EventLog,
                    });
                }
                let slot_page = self.query_message_slots_page(&MessageSlotQuery {
                    session_id: Some(session.session_id.clone()),
                    include_alternates: false,
                    page: Some(rusty_crew_core_persistence::QueryPage {
                        limit: Some(1),
                        offset: Some(0),
                    }),
                })?;
                if slot_page.total > 0 {
                    return Ok(ChatSessionReadFacts {
                        execution: self.session_execution_state(&session.session_id)?,
                        latest_cursor: chat_cursor_for(&session.session_id, slot_page.total),
                        session,
                        message_count: slot_page.total,
                        source: ChatReadModelSource::MessageSlots,
                    });
                }
                let pending = self
                    .project_body_state(&session.session_id)?
                    .pending_messages;
                let message_count = pending.len() as u64;
                Ok(ChatSessionReadFacts {
                    execution: self.session_execution_state(&session.session_id)?,
                    latest_cursor: chat_cursor_for(&session.session_id, message_count),
                    session,
                    message_count,
                    source: if message_count == 0 {
                        ChatReadModelSource::Empty
                    } else {
                        ChatReadModelSource::PendingMessages
                    },
                })
            })
            .collect::<CoreResult<Vec<_>>>()?;
        Ok(ChatSessionSummaryPage {
            page: ExactPage::new(items, total, limit, offset),
        })
    }

    pub fn append_chat_event(&self, event: &ChatEventLogAppend) -> CoreResult<ChatEventLogEvent> {
        self.store.append_chat_event_log(event)
    }

    pub fn query_chat_events(&self, query: &ChatEventLogQuery) -> CoreResult<ChatEventLogPage> {
        self.store.query_chat_event_log(query)
    }

    pub fn save_conversation_branch(
        &self,
        branch: &ConversationBranchWrite,
    ) -> CoreResult<ConversationBranchRecord> {
        self.store.save_chat_conversation_branch(branch)
    }

    pub fn query_conversation_branches(
        &self,
        query: &ConversationBranchQuery,
    ) -> CoreResult<Vec<ConversationBranchRecord>> {
        self.store.query_chat_conversation_branches(query)
    }

    pub fn create_chat_conversation_branch(
        &self,
        request: &CreateChatConversationBranchRequest,
    ) -> CoreResult<ConversationBranchRecord> {
        ChatConversationStore::create_chat_conversation_branch(&self.store, request)
    }

    pub fn ensure_active_chat_conversation_branch(
        &self,
        request: &EnsureActiveChatConversationBranchRequest,
    ) -> CoreResult<EnsureActiveChatConversationBranchResult> {
        ChatConversationStore::ensure_active_chat_conversation_branch(&self.store, request)
    }

    pub fn get_conversation_branch_state(
        &self,
        session_id: &SessionId,
        default_updated_at: &IsoTimestamp,
    ) -> CoreResult<ConversationBranchStateRecord> {
        self.store
            .get_chat_conversation_branch_state(session_id, default_updated_at)
    }

    pub fn select_active_conversation_branch(
        &self,
        request: &SelectActiveBranchRequest,
    ) -> CoreResult<SelectActiveBranchResult> {
        self.store.select_active_chat_conversation_branch(request)
    }

    pub fn update_conversation_branch_head(
        &self,
        request: &UpdateBranchHeadRequest,
    ) -> CoreResult<UpdateBranchHeadResult> {
        self.store.update_chat_conversation_branch_head(request)
    }

    pub fn save_conversation_snapshot(
        &self,
        snapshot: &ConversationSnapshotWrite,
    ) -> CoreResult<ConversationSnapshotRecord> {
        self.store.save_chat_conversation_snapshot(snapshot)
    }

    pub fn create_chat_conversation_snapshot(
        &self,
        request: &CreateChatConversationSnapshotRequest,
    ) -> CoreResult<CreateChatConversationSnapshotResult> {
        ChatConversationStore::create_chat_conversation_snapshot(&self.store, request)
    }

    pub fn query_conversation_snapshots(
        &self,
        query: &ConversationSnapshotQuery,
    ) -> CoreResult<Vec<ConversationSnapshotRecord>> {
        self.store.query_chat_conversation_snapshots(query)
    }

    pub fn read_conversation_tree(
        &self,
        query: &ConversationTreeReadQuery,
    ) -> CoreResult<ConversationTreeReadResult> {
        self.store.read_chat_conversation_tree(query)
    }

    pub fn search_chat_transcript(
        &self,
        query: &ChatTranscriptSearchQuery,
    ) -> CoreResult<ChatTranscriptSearchPage> {
        ChatConversationStore::search_chat_transcript(&self.store, query)
    }

    pub fn resolve_conversation_jump(
        &self,
        request: &ConversationJumpRequest,
    ) -> CoreResult<ConversationJumpResult> {
        self.store.resolve_chat_conversation_jump(request)
    }

    pub fn save_attachment(&self, attachment: &AttachmentWrite) -> CoreResult<AttachmentRecord> {
        self.store.save_chat_attachment(attachment)
    }

    pub fn create_chat_attachment(
        &self,
        request: &CreateChatAttachmentRequest,
    ) -> CoreResult<CreateChatAttachmentResult> {
        ChatConversationStore::create_chat_attachment(&self.store, request)
    }

    pub fn query_attachments(&self, query: &AttachmentQuery) -> CoreResult<Vec<AttachmentRecord>> {
        self.store.query_chat_attachments(query)
    }

    pub fn query_attachments_page(
        &self,
        query: &AttachmentQuery,
    ) -> CoreResult<ExactPage<AttachmentRecord>> {
        self.store.query_chat_attachments_page(query)
    }

    pub fn remove_attachment(
        &self,
        attachment_id: &AttachmentId,
        updated_at: &IsoTimestamp,
    ) -> CoreResult<AttachmentRecord> {
        ChatConversationStore::remove_attachment(&self.store, attachment_id, updated_at)
    }

    pub fn remove_chat_attachment(
        &self,
        request: &RemoveChatAttachmentRequest,
    ) -> CoreResult<AttachmentRecord> {
        ChatConversationStore::remove_chat_attachment(&self.store, request)
    }

    pub fn save_data_bank_scope(
        &self,
        scope: &DataBankScopeWrite,
    ) -> CoreResult<DataBankScopeRecord> {
        self.store.save_chat_data_bank_scope(scope)
    }

    pub fn create_chat_data_bank_scope(
        &self,
        request: &CreateChatDataBankScopeRequest,
    ) -> CoreResult<CreateChatDataBankScopeResult> {
        ChatConversationStore::create_chat_data_bank_scope(&self.store, request)
    }

    pub fn query_data_bank_scopes(
        &self,
        query: &DataBankScopeQuery,
    ) -> CoreResult<Vec<DataBankScopeRecord>> {
        self.store.query_chat_data_bank_scopes(query)
    }

    pub fn query_data_bank_scopes_page(
        &self,
        query: &DataBankScopeQuery,
    ) -> CoreResult<ExactPage<DataBankScopeRecord>> {
        self.store.query_chat_data_bank_scopes_page(query)
    }

    pub fn remove_data_bank_scope(
        &self,
        scope_id: &DataBankScopeId,
        updated_at: &IsoTimestamp,
    ) -> CoreResult<DataBankScopeRecord> {
        ChatConversationStore::remove_data_bank_scope(&self.store, scope_id, updated_at)
    }

    pub fn remove_chat_data_bank_scope(
        &self,
        request: &RemoveChatDataBankScopeRequest,
    ) -> CoreResult<DataBankScopeRecord> {
        ChatConversationStore::remove_chat_data_bank_scope(&self.store, request)
    }

    pub fn select_active_message_variant(
        &self,
        request: &SelectActiveVariantRequest,
    ) -> CoreResult<SelectActiveVariantResult> {
        self.store.select_active_chat_message_variant_store(request)
    }

    pub fn select_active_chat_message_variant(
        &self,
        request: &SelectActiveChatMessageVariantRequest,
    ) -> CoreResult<SelectActiveChatMessageVariantResult> {
        let result = ChatConversationStore::select_active_chat_message_variant_store(
            &self.store,
            &SelectActiveVariantRequest {
                slot_id: request.slot_id.clone(),
                active_variant_id: request.active_variant_id.clone(),
                expected: request.expected.clone(),
                updated_at: request.updated_at.clone(),
            },
        )?;
        if result.slot.session_id != request.session_id {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!(
                    "message slot {} does not belong to chat session {}",
                    request.slot_id, request.session_id
                ),
            ));
        }
        if result.conflict.is_none() {
            if let Some(selected) = selected_message_variant(&result.slot) {
                if let Some(branch_id) = &selected.message.branch_id {
                    ChatConversationStore::update_chat_conversation_branch_head(
                        &self.store,
                        &UpdateBranchHeadRequest {
                            branch_id: branch_id.clone(),
                            head_message_id: Some(selected.message.message_id.clone()),
                            expected: BranchHeadExpectation::Any,
                            updated_at: request.updated_at.clone(),
                        },
                    )?;
                }
            }
        }
        Ok(SelectActiveChatMessageVariantResult {
            slot: result.slot,
            conflict: result.conflict,
        })
    }

    pub fn delete_message_variant(
        &self,
        slot_id: &MessageSlotId,
        variant_id: &MessageVariantId,
        updated_at: &IsoTimestamp,
    ) -> CoreResult<MessageSlotRecord> {
        ChatConversationStore::delete_message_variant(&self.store, slot_id, variant_id, updated_at)
    }

    pub fn reorder_message_variants(
        &self,
        slot_id: &MessageSlotId,
        ordered_variant_ids: &[MessageVariantId],
        updated_at: &IsoTimestamp,
    ) -> CoreResult<Vec<MessageVariantRecord>> {
        ChatConversationStore::reorder_message_variants(
            &self.store,
            slot_id,
            ordered_variant_ids,
            updated_at,
        )
    }
}

fn normalize_chat_read_model_limit(limit: Option<u32>) -> u32 {
    limit
        .unwrap_or(DEFAULT_CHAT_READ_MODEL_LIMIT)
        .min(MAX_CHAT_READ_MODEL_LIMIT)
}

fn chat_cursor_for(session_id: &SessionId, sequence: u64) -> String {
    format!("{session_id}:{sequence}")
}

fn chat_cursor_sequence(cursor: Option<&str>, session_id: &SessionId) -> u64 {
    let Some(cursor) = cursor else {
        return 0;
    };
    let Some(sequence) = cursor.strip_prefix(&format!("{session_id}:")) else {
        return 0;
    };
    sequence.parse::<u64>().unwrap_or(0)
}

fn chat_read_model_event_from_slot(
    session_id: &SessionId,
    agent_id: &str,
    sequence: u64,
    slot: &MessageSlotRecord,
) -> ChatReadModelEvent {
    let variant = slot
        .active_variant_id
        .as_ref()
        .and_then(|active_variant_id| {
            slot.alternates
                .iter()
                .find(|candidate| &candidate.variant_id == active_variant_id)
        })
        .unwrap_or(&slot.primary);
    durable_message_event(session_id, agent_id, sequence, &variant.message)
}

fn durable_message_event(
    session_id: &SessionId,
    agent_id: &str,
    sequence: u64,
    message: &DurableMessageRecord,
) -> ChatReadModelEvent {
    let role = if message.author_role == "assistant" || message.author_id == agent_id {
        "assistant"
    } else {
        "user"
    };
    let mut payload = json!({
        "message_id": message.message_id.0.as_str(),
        "role": role,
        "body": message.body.as_str(),
        "source": "durable_message_slot",
        "slot_status": message.status,
    });
    if let Some(correlation_id) = message
        .metadata_json
        .get("correlation_id")
        .and_then(|value| value.as_str())
    {
        if let Some(payload) = payload.as_object_mut() {
            payload.insert("correlation_id".to_string(), json!(correlation_id));
        }
    }

    ChatReadModelEvent {
        event_id: chat_cursor_for(session_id, sequence),
        session_id: session_id.clone(),
        sequence_id: sequence,
        created_at: message.created_at.clone(),
        kind: ChatReadModelEventKind::MessageCreated,
        payload_json: payload,
    }
}

fn pending_message_event(
    session: &SessionState,
    sequence: u64,
    message: &AgentMessage,
    created_at: IsoTimestamp,
) -> ChatReadModelEvent {
    let message_id = message
        .correlation_id
        .as_deref()
        .map(|correlation_id| format!("pending:{correlation_id}"))
        .unwrap_or_else(|| format!("pending:{sequence}"));
    let role = if message.from == session.agent_id {
        "assistant"
    } else {
        "user"
    };
    let mut payload = json!({
        "message_id": message_id,
        "role": role,
        "body": message.body.as_str(),
        "source": "pending_body_state",
    });
    if let Some(correlation_id) = message.correlation_id.as_deref() {
        if let Some(payload) = payload.as_object_mut() {
            payload.insert("correlation_id".to_string(), json!(correlation_id));
        }
    }
    ChatReadModelEvent {
        event_id: chat_cursor_for(&session.session_id, sequence),
        session_id: session.session_id.clone(),
        sequence_id: sequence,
        created_at,
        kind: ChatReadModelEventKind::MessageCreated,
        payload_json: payload,
    }
}

fn chat_read_model_event_as_log_event(event: ChatReadModelEvent) -> ChatEventLogEvent {
    ChatEventLogEvent {
        event_id: event.event_id,
        session_id: event.session_id,
        sequence_id: event.sequence_id,
        created_at: event.created_at,
        kind: match event.kind {
            ChatReadModelEventKind::MessageCreated => "message_created".to_string(),
        },
        payload_json: event.payload_json,
    }
}

fn session_status_wire_value(status: &SessionStatus) -> &str {
    match status {
        SessionStatus::Idle => "idle",
        SessionStatus::Active => "active",
        SessionStatus::Archived => "archived",
    }
}

fn selected_message_variant(slot: &MessageSlotRecord) -> Option<&MessageVariantRecord> {
    match &slot.active_variant_id {
        Some(active_variant_id) => slot
            .alternates
            .iter()
            .find(|variant| &variant.variant_id == active_variant_id),
        None => Some(&slot.primary),
    }
}
