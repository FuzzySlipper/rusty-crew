use super::*;

pub(super) fn save_test_message_slot(
    engine: &CoreEngine,
    session_id: &str,
    ordinal: u32,
    author_id: &str,
    author_role: &str,
    body: &str,
) {
    let timestamp = format!("2026-06-19T00:{ordinal:02}:00Z");
    engine
        .save_message_slot(&MessageSlotWrite {
            slot_id: MessageSlotId::new(format!("{session_id}-slot-{ordinal}")),
            session_id: SessionId::new(session_id),
            primary_variant_id: MessageVariantId::new(format!(
                "{session_id}-variant-{ordinal}-primary"
            )),
            active_variant_id: None,
            metadata_json: json!({}),
            created_at: timestamp.clone(),
            updated_at: timestamp.clone(),
        })
        .unwrap();
    engine
        .save_message_variant(&MessageVariantWrite {
            variant_id: MessageVariantId::new(format!("{session_id}-variant-{ordinal}-primary")),
            slot_id: MessageSlotId::new(format!("{session_id}-slot-{ordinal}")),
            source: MessageVariantSource::Primary,
            ordinal: 0,
            status: MessageVariantStatus::Active,
            message: test_message_write(session_id, ordinal, author_id, author_role, body),
            metadata_json: json!({}),
            created_at: timestamp.clone(),
            updated_at: timestamp,
        })
        .unwrap();
}

pub(super) fn save_test_alternate_variant(
    engine: &CoreEngine,
    session_id: &str,
    slot_ordinal: u32,
    variant_ordinal: u32,
    body: &str,
) {
    let timestamp = format!("2026-06-19T00:{variant_ordinal:02}:00Z");
    engine
        .save_message_variant(&MessageVariantWrite {
            variant_id: MessageVariantId::new(format!(
                "{session_id}-variant-{variant_ordinal}-alt"
            )),
            slot_id: MessageSlotId::new(format!("{session_id}-slot-{slot_ordinal}")),
            source: MessageVariantSource::Alternate,
            ordinal: variant_ordinal.saturating_sub(1),
            status: MessageVariantStatus::Active,
            message: test_message_write(
                session_id,
                variant_ordinal,
                "assistant",
                "assistant",
                body,
            ),
            metadata_json: json!({}),
            created_at: timestamp.clone(),
            updated_at: timestamp,
        })
        .unwrap();
}

pub(super) fn save_test_branch(
    engine: &CoreEngine,
    session_id: &str,
    branch_id: &str,
    parent_branch_id: Option<&str>,
    head_message_id: Option<&str>,
) {
    engine
        .save_conversation_branch(&test_branch_write(
            session_id,
            branch_id,
            parent_branch_id,
            head_message_id,
        ))
        .unwrap();
}

pub(super) fn test_branch_write(
    session_id: &str,
    branch_id: &str,
    parent_branch_id: Option<&str>,
    head_message_id: Option<&str>,
) -> ConversationBranchWrite {
    ConversationBranchWrite {
        branch_id: ConversationBranchId::new(branch_id),
        session_id: SessionId::new(session_id),
        parent_branch_id: parent_branch_id.map(ConversationBranchId::new),
        parent_message_id: None,
        origin_message_id: None,
        head_message_id: head_message_id.map(MessageId::new),
        label: Some("Branch".to_string()),
        metadata_json: json!({}),
        created_at: "2026-06-19T00:00:00Z".to_string(),
        updated_at: "2026-06-19T00:00:00Z".to_string(),
    }
}

pub(super) fn test_snapshot_write(
    session_id: &str,
    snapshot_id: &str,
    branch_id: Option<&str>,
    message_id: Option<&str>,
) -> ConversationSnapshotWrite {
    ConversationSnapshotWrite {
        snapshot_id: ConversationSnapshotId::new(snapshot_id),
        session_id: SessionId::new(session_id),
        branch_id: branch_id.map(ConversationBranchId::new),
        message_id: message_id.map(MessageId::new),
        cursor: Some(format!("{session_id}:cursor")),
        label: Some("Snapshot".to_string()),
        summary: Some("Snapshot summary".to_string()),
        source: ConversationSnapshotSource::User,
        metadata_json: json!({}),
        created_at: "2026-06-19T00:01:00Z".to_string(),
        updated_at: "2026-06-19T00:01:00Z".to_string(),
    }
}

pub(super) fn test_attachment_write(
    session_id: &str,
    attachment_id: &str,
    link: Option<AttachmentLinkWrite>,
) -> AttachmentWrite {
    AttachmentWrite {
        attachment_id: AttachmentId::new(attachment_id),
        session_id: SessionId::new(session_id),
        status: AttachmentStatus::Active,
        filename: format!("{attachment_id}.txt"),
        mime_type: "text/plain".to_string(),
        byte_size: 32,
        storage_url: None,
        download_url: None,
        thumbnail_url: None,
        extracted_text: Some("attachment body".to_string()),
        extracted_text_truncated: false,
        metadata_json: json!({}),
        created_at: "2026-06-19T00:01:00Z".to_string(),
        updated_at: "2026-06-19T00:01:00Z".to_string(),
        expires_at: None,
        link,
    }
}

pub(super) fn test_data_bank_scope_write(session_id: &str, scope_id: &str) -> DataBankScopeWrite {
    DataBankScopeWrite {
        scope_id: DataBankScopeId::new(scope_id),
        session_id: SessionId::new(session_id),
        status: DataBankScopeStatus::Active,
        label: Some(format!("Scope {scope_id}")),
        description: Some("Reusable scope".to_string()),
        metadata_json: json!({}),
        created_at: "2026-06-19T00:01:00Z".to_string(),
        updated_at: "2026-06-19T00:01:00Z".to_string(),
    }
}

pub(super) fn test_message_write(
    session_id: &str,
    ordinal: u32,
    author_id: &str,
    author_role: &str,
    body: &str,
) -> DurableMessageWrite {
    DurableMessageWrite {
        message_id: MessageId::new(format!("{session_id}-message-{ordinal}")),
        session_id: SessionId::new(session_id),
        branch_id: None,
        parent_message_id: None,
        previous_message_id: None,
        author_id: author_id.to_string(),
        author_role: author_role.to_string(),
        status: DurableMessageStatus::Completed,
        body: body.to_string(),
        metadata_json: json!({ "correlation_id": format!("correlation-{ordinal}") }),
        created_at: format!("2026-06-19T00:{ordinal:02}:00Z"),
        blocks: Vec::new(),
    }
}

pub(super) fn chat_slot_ingest_request(
    session_id: &str,
    ordinal: u32,
    idempotency_key: &str,
) -> CreateChatMessageSlotRequest {
    let branch_id = ConversationBranchId::new(format!("branch:{session_id}:default"));
    let slot_id = MessageSlotId::new(format!("slot:{session_id}:{idempotency_key}"));
    let variant_id = MessageVariantId::new(format!("variant:{session_id}:{idempotency_key}"));
    let message_id = MessageId::new(format!("message:{session_id}:{idempotency_key}"));
    let timestamp = format!("2026-06-19T00:{ordinal:02}:00Z");
    let mut message = test_message_write(session_id, ordinal, "user", "user", "hello");
    message.message_id = message_id;
    message.branch_id = Some(branch_id.clone());
    CreateChatMessageSlotRequest {
        slot: MessageSlotWrite {
            slot_id: slot_id.clone(),
            session_id: SessionId::new(session_id),
            primary_variant_id: variant_id.clone(),
            active_variant_id: None,
            metadata_json: json!({"idempotency_key": idempotency_key}),
            created_at: timestamp.clone(),
            updated_at: timestamp.clone(),
        },
        primary_variant: MessageVariantWrite {
            variant_id,
            slot_id,
            source: MessageVariantSource::Primary,
            ordinal: 0,
            status: MessageVariantStatus::Active,
            message,
            metadata_json: json!({}),
            created_at: timestamp.clone(),
            updated_at: timestamp.clone(),
        },
        branch_id: branch_id.clone(),
        expected_branch_head: BranchHeadExpectation::Any,
        updated_at: timestamp.clone(),
        ensure_active_branch: Some(EnsureActiveChatConversationBranchRequest {
            session_id: SessionId::new(session_id),
            branch_id,
            label: Some("Default".to_string()),
            metadata_json: json!({"source": "test"}),
            created_at: timestamp.clone(),
            updated_at: timestamp,
        }),
        inherit_branch_head: true,
        idempotency_key: Some(idempotency_key.to_string()),
    }
}
