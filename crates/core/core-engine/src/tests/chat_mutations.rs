use super::*;

#[test]
fn select_active_chat_message_variant_updates_branch_head() {
    let engine = test_engine();
    engine
        .save_conversation_branch(&ConversationBranchWrite {
            branch_id: ConversationBranchId::new("variant-branch"),
            session_id: SessionId::new("chat-variant-session"),
            parent_branch_id: None,
            parent_message_id: None,
            origin_message_id: None,
            head_message_id: Some(MessageId::new("chat-variant-session-message-1")),
            label: Some("Main".to_string()),
            metadata_json: json!({}),
            created_at: "2026-06-19T00:00:00Z".to_string(),
            updated_at: "2026-06-19T00:00:00Z".to_string(),
        })
        .unwrap();
    save_test_message_slot(
        &engine,
        "chat-variant-session",
        1,
        "agent",
        "assistant",
        "primary",
    );
    let mut alternate =
        test_message_write("chat-variant-session", 2, "agent", "assistant", "alternate");
    alternate.branch_id = Some(ConversationBranchId::new("variant-branch"));
    engine
        .save_message_variant(&MessageVariantWrite {
            variant_id: MessageVariantId::new("chat-variant-session-variant-alt"),
            slot_id: MessageSlotId::new("chat-variant-session-slot-1"),
            source: MessageVariantSource::Alternate,
            ordinal: 1,
            status: MessageVariantStatus::Active,
            message: alternate,
            metadata_json: json!({}),
            created_at: "2026-06-19T00:02:00Z".to_string(),
            updated_at: "2026-06-19T00:02:00Z".to_string(),
        })
        .unwrap();

    let result = engine
        .select_active_chat_message_variant(&SelectActiveChatMessageVariantRequest {
            session_id: SessionId::new("chat-variant-session"),
            slot_id: MessageSlotId::new("chat-variant-session-slot-1"),
            active_variant_id: Some(MessageVariantId::new("chat-variant-session-variant-alt")),
            expected: rusty_crew_core_persistence::ActiveVariantExpectation::Any,
            updated_at: "2026-06-19T00:03:00Z".to_string(),
        })
        .unwrap();

    assert!(result.conflict.is_none());
    assert_eq!(
        result.slot.active_variant_id,
        Some(MessageVariantId::new("chat-variant-session-variant-alt"))
    );
    let branches = engine
        .query_conversation_branches(&ConversationBranchQuery {
            session_id: Some(SessionId::new("chat-variant-session")),
            parent_branch_id: None,
            page: None,
        })
        .unwrap();
    assert_eq!(
        branches
            .iter()
            .find(|branch| branch.branch_id == ConversationBranchId::new("variant-branch"))
            .and_then(|branch| branch.head_message_id.clone()),
        Some(MessageId::new("chat-variant-session-message-2"))
    );
}

#[test]
fn select_active_chat_message_variant_preserves_conflict_output() {
    let engine = test_engine();
    save_test_message_slot(
        &engine,
        "chat-conflict-session",
        1,
        "agent",
        "assistant",
        "primary",
    );

    let result = engine
        .select_active_chat_message_variant(&SelectActiveChatMessageVariantRequest {
            session_id: SessionId::new("chat-conflict-session"),
            slot_id: MessageSlotId::new("chat-conflict-session-slot-1"),
            active_variant_id: None,
            expected: rusty_crew_core_persistence::ActiveVariantExpectation::Variant(
                MessageVariantId::new("missing-active-variant"),
            ),
            updated_at: "2026-06-19T00:03:00Z".to_string(),
        })
        .unwrap();

    assert_eq!(
        result.conflict,
        Some(ActiveVariantConflict {
            expected: Some(MessageVariantId::new("missing-active-variant")),
            actual: None,
        })
    );
    assert_eq!(result.slot.active_variant_id, None);
}

#[test]
fn create_chat_message_slot_updates_branch_head_atomically() {
    let engine = test_engine();
    engine
        .save_conversation_branch(&ConversationBranchWrite {
            branch_id: ConversationBranchId::new("create-slot-branch"),
            session_id: SessionId::new("create-slot-session"),
            parent_branch_id: None,
            parent_message_id: None,
            origin_message_id: None,
            head_message_id: None,
            label: Some("Main".to_string()),
            metadata_json: json!({}),
            created_at: "2026-06-19T00:00:00Z".to_string(),
            updated_at: "2026-06-19T00:00:00Z".to_string(),
        })
        .unwrap();
    let mut message = test_message_write("create-slot-session", 1, "user", "user", "hello");
    message.branch_id = Some(ConversationBranchId::new("create-slot-branch"));

    let result = engine
        .create_chat_message_slot(&CreateChatMessageSlotRequest {
            slot: MessageSlotWrite {
                slot_id: MessageSlotId::new("create-slot-session-slot-1"),
                session_id: SessionId::new("create-slot-session"),
                primary_variant_id: MessageVariantId::new("create-slot-session-primary-1"),
                active_variant_id: None,
                metadata_json: json!({ "source": "test" }),
                created_at: "2026-06-19T00:01:00Z".to_string(),
                updated_at: "2026-06-19T00:01:00Z".to_string(),
            },
            primary_variant: MessageVariantWrite {
                variant_id: MessageVariantId::new("create-slot-session-primary-1"),
                slot_id: MessageSlotId::new("create-slot-session-slot-1"),
                source: MessageVariantSource::Primary,
                ordinal: 0,
                status: MessageVariantStatus::Active,
                message,
                metadata_json: json!({}),
                created_at: "2026-06-19T00:01:00Z".to_string(),
                updated_at: "2026-06-19T00:01:00Z".to_string(),
            },
            branch_id: ConversationBranchId::new("create-slot-branch"),
            expected_branch_head: BranchHeadExpectation::None,
            updated_at: "2026-06-19T00:01:30Z".to_string(),
            ensure_active_branch: None,
            inherit_branch_head: false,
            idempotency_key: None,
        })
        .unwrap();

    assert!(result.conflict.is_none());
    assert_eq!(
        result
            .slot
            .as_ref()
            .map(|slot| slot.primary.message.message_id.clone()),
        Some(MessageId::new("create-slot-session-message-1"))
    );
    assert_eq!(
        result.branch.head_message_id,
        Some(MessageId::new("create-slot-session-message-1"))
    );
}

#[test]
fn create_chat_message_slot_conflict_does_not_create_slot() {
    let engine = test_engine();
    save_test_message_slot(
        &engine,
        "create-conflict-session",
        1,
        "user",
        "user",
        "existing",
    );
    engine
        .save_conversation_branch(&ConversationBranchWrite {
            branch_id: ConversationBranchId::new("create-conflict-branch"),
            session_id: SessionId::new("create-conflict-session"),
            parent_branch_id: None,
            parent_message_id: None,
            origin_message_id: None,
            head_message_id: Some(MessageId::new("create-conflict-session-message-1")),
            label: Some("Main".to_string()),
            metadata_json: json!({}),
            created_at: "2026-06-19T00:00:00Z".to_string(),
            updated_at: "2026-06-19T00:00:00Z".to_string(),
        })
        .unwrap();
    let mut message = test_message_write("create-conflict-session", 2, "user", "user", "new");
    message.branch_id = Some(ConversationBranchId::new("create-conflict-branch"));

    let result = engine
        .create_chat_message_slot(&CreateChatMessageSlotRequest {
            slot: MessageSlotWrite {
                slot_id: MessageSlotId::new("create-conflict-session-slot-2"),
                session_id: SessionId::new("create-conflict-session"),
                primary_variant_id: MessageVariantId::new("create-conflict-session-primary-2"),
                active_variant_id: None,
                metadata_json: json!({}),
                created_at: "2026-06-19T00:02:00Z".to_string(),
                updated_at: "2026-06-19T00:02:00Z".to_string(),
            },
            primary_variant: MessageVariantWrite {
                variant_id: MessageVariantId::new("create-conflict-session-primary-2"),
                slot_id: MessageSlotId::new("create-conflict-session-slot-2"),
                source: MessageVariantSource::Primary,
                ordinal: 0,
                status: MessageVariantStatus::Active,
                message,
                metadata_json: json!({}),
                created_at: "2026-06-19T00:02:00Z".to_string(),
                updated_at: "2026-06-19T00:02:00Z".to_string(),
            },
            branch_id: ConversationBranchId::new("create-conflict-branch"),
            expected_branch_head: BranchHeadExpectation::None,
            updated_at: "2026-06-19T00:02:30Z".to_string(),
            ensure_active_branch: None,
            inherit_branch_head: false,
            idempotency_key: None,
        })
        .unwrap();

    assert_eq!(
        result.conflict,
        Some(BranchHeadConflict {
            expected: None,
            actual: Some(MessageId::new("create-conflict-session-message-1")),
        })
    );
    assert!(result.slot.is_none());
    let slots = engine
        .query_message_slots(&MessageSlotQuery {
            session_id: Some(SessionId::new("create-conflict-session")),
            include_alternates: true,
            page: None,
        })
        .unwrap();
    assert!(slots
        .iter()
        .all(|slot| slot.slot_id != MessageSlotId::new("create-conflict-session-slot-2")));
}

#[test]
fn create_chat_message_slot_ensures_branch_and_replays_durable_receipt() {
    let engine = test_engine();
    let request = chat_slot_ingest_request("ingest-session", 1, "request-alpha");

    let created = engine.create_chat_message_slot(&request).unwrap();
    assert!(!created.duplicate);
    assert!(created.conflict.is_none());
    assert_eq!(
        created.branch.branch_id,
        ConversationBranchId::new("branch:ingest-session:default")
    );
    let created_slot = created.slot.unwrap();
    assert_eq!(
        created_slot.primary.message.branch_id,
        Some(created.branch.branch_id.clone())
    );
    assert_eq!(created_slot.primary.message.parent_message_id, None);

    let duplicate = engine.create_chat_message_slot(&request).unwrap();
    assert!(duplicate.duplicate);
    assert!(duplicate.conflict.is_none());
    assert_eq!(duplicate.slot.unwrap().slot_id, created_slot.slot_id);
    assert_eq!(duplicate.branch.branch_id, created.branch.branch_id);
    assert_eq!(
        engine
            .query_message_slots(&MessageSlotQuery {
                session_id: Some(SessionId::new("ingest-session")),
                include_alternates: true,
                page: None,
            })
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn create_chat_message_slot_receipt_rolls_back_with_conflict() {
    let engine = test_engine();
    engine
        .create_chat_message_slot(&chat_slot_ingest_request(
            "receipt-rollback-session",
            1,
            "request-first",
        ))
        .unwrap();
    let mut request = chat_slot_ingest_request("receipt-rollback-session", 2, "request-retry");
    request.expected_branch_head = BranchHeadExpectation::None;
    let conflict = engine.create_chat_message_slot(&request).unwrap();
    assert!(conflict.conflict.is_some());
    assert!(!conflict.duplicate);

    request.expected_branch_head = BranchHeadExpectation::Any;
    let retried = engine.create_chat_message_slot(&request).unwrap();
    assert!(retried.conflict.is_none());
    assert!(!retried.duplicate);
    assert_eq!(
        engine
            .query_message_slots(&MessageSlotQuery {
                session_id: Some(SessionId::new("receipt-rollback-session")),
                include_alternates: true,
                page: None,
            })
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn concurrent_chat_message_slot_ingest_creates_once() {
    let engine = test_engine();
    let request = chat_slot_ingest_request("concurrent-ingest-session", 1, "same-key");
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let results = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for _ in 0..2 {
            let engine = engine.clone();
            let request = request.clone();
            let barrier = barrier.clone();
            handles.push(scope.spawn(move || {
                barrier.wait();
                engine.create_chat_message_slot(&request).unwrap()
            }));
        }
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(results.iter().filter(|result| result.duplicate).count(), 1);
    assert_eq!(results.iter().filter(|result| !result.duplicate).count(), 1);
    assert_eq!(
        engine
            .query_message_slots(&MessageSlotQuery {
                session_id: Some(SessionId::new("concurrent-ingest-session")),
                include_alternates: true,
                page: None,
            })
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn create_chat_message_variant_allocates_next_ordinal() {
    let engine = test_engine();
    save_test_message_slot(
        &engine,
        "create-variant-session",
        1,
        "assistant",
        "assistant",
        "primary",
    );
    let mut first = test_message_write(
        "create-variant-session",
        2,
        "assistant",
        "assistant",
        "alt 1",
    );
    first.branch_id = Some(ConversationBranchId::new("variant-branch"));
    let mut second = test_message_write(
        "create-variant-session",
        3,
        "assistant",
        "assistant",
        "alt 2",
    );
    second.branch_id = Some(ConversationBranchId::new("variant-branch"));

    let first_result = engine
        .create_chat_message_variant(&CreateChatMessageVariantRequest {
            session_id: SessionId::new("create-variant-session"),
            slot_id: MessageSlotId::new("create-variant-session-slot-1"),
            variant: MessageVariantWrite {
                variant_id: MessageVariantId::new("create-variant-session-alt-1"),
                slot_id: MessageSlotId::new("create-variant-session-slot-1"),
                source: MessageVariantSource::Alternate,
                ordinal: 0,
                status: MessageVariantStatus::Active,
                message: first,
                metadata_json: json!({}),
                created_at: "2026-06-19T00:02:00Z".to_string(),
                updated_at: "2026-06-19T00:02:00Z".to_string(),
            },
        })
        .unwrap();
    let second_result = engine
        .create_chat_message_variant(&CreateChatMessageVariantRequest {
            session_id: SessionId::new("create-variant-session"),
            slot_id: MessageSlotId::new("create-variant-session-slot-1"),
            variant: MessageVariantWrite {
                variant_id: MessageVariantId::new("create-variant-session-alt-2"),
                slot_id: MessageSlotId::new("create-variant-session-slot-1"),
                source: MessageVariantSource::Alternate,
                ordinal: 0,
                status: MessageVariantStatus::Active,
                message: second,
                metadata_json: json!({}),
                created_at: "2026-06-19T00:03:00Z".to_string(),
                updated_at: "2026-06-19T00:03:00Z".to_string(),
            },
        })
        .unwrap();

    assert_eq!(first_result.variant.ordinal, 1);
    assert_eq!(second_result.variant.ordinal, 2);
}

#[test]
fn roleplay_alternative_creation_selection_and_branch_head_are_atomic() {
    let engine = test_engine();
    save_test_message_slot(
        &engine,
        "roleplay-alt-session",
        1,
        "assistant",
        "assistant",
        "primary",
    );
    save_test_branch(
        &engine,
        "roleplay-alt-session",
        "roleplay-alt-branch",
        None,
        None,
    );
    let mut message = test_message_write(
        "roleplay-alt-session",
        2,
        "assistant",
        "assistant",
        "alternate",
    );
    message.branch_id = Some(ConversationBranchId::new("roleplay-alt-branch"));
    let request = ApplyRoleplayAlternativeRequest {
        session_id: SessionId::new("roleplay-alt-session"),
        slot_id: MessageSlotId::new("roleplay-alt-session-slot-1"),
        create_variant: Some(MessageVariantWrite {
            variant_id: MessageVariantId::new("roleplay-alt-variant"),
            slot_id: MessageSlotId::new("roleplay-alt-session-slot-1"),
            source: MessageVariantSource::Alternate,
            ordinal: 0,
            status: MessageVariantStatus::Active,
            message,
            metadata_json: json!({}),
            created_at: "2026-06-19T00:02:00Z".into(),
            updated_at: "2026-06-19T00:02:00Z".into(),
        }),
        active_variant_id: Some(MessageVariantId::new("roleplay-alt-variant")),
        expected: ActiveVariantExpectation::Any,
        updated_at: "2026-06-19T00:02:00Z".into(),
    };
    let result = engine.apply_roleplay_alternative(&request).unwrap();
    assert_eq!(result.created_variant.as_ref().unwrap().ordinal, 1);
    assert_eq!(result.slot.active_variant_id, request.active_variant_id);
    assert_eq!(
        result.branch.unwrap().head_message_id,
        Some(MessageId::new("roleplay-alt-session-message-2"))
    );

    let mut losing = request.clone();
    losing.create_variant.as_mut().unwrap().variant_id =
        MessageVariantId::new("roleplay-alt-loser");
    losing.create_variant.as_mut().unwrap().message.message_id =
        MessageId::new("roleplay-alt-session-message-3");
    losing.active_variant_id = Some(MessageVariantId::new("roleplay-alt-loser"));
    losing.expected = ActiveVariantExpectation::Primary;
    let conflict = engine.apply_roleplay_alternative(&losing).unwrap();
    assert!(conflict.conflict.is_some());
    assert!(conflict.created_variant.is_none());
    assert!(engine
        .query_message_variants(&MessageVariantQuery {
            slot_id: Some(request.slot_id),
            include_deleted: false,
            page: None
        })
        .unwrap()
        .iter()
        .all(|variant| variant.variant_id != MessageVariantId::new("roleplay-alt-loser")));
}

#[test]
fn create_chat_message_variant_validates_slot_session_ownership() {
    let engine = test_engine();
    save_test_message_slot(
        &engine,
        "owned-variant-session",
        1,
        "assistant",
        "assistant",
        "primary",
    );
    let message = test_message_write("other-variant-session", 2, "assistant", "assistant", "alt");

    let error = engine
        .create_chat_message_variant(&CreateChatMessageVariantRequest {
            session_id: SessionId::new("other-variant-session"),
            slot_id: MessageSlotId::new("owned-variant-session-slot-1"),
            variant: MessageVariantWrite {
                variant_id: MessageVariantId::new("owned-variant-session-alt-1"),
                slot_id: MessageSlotId::new("owned-variant-session-slot-1"),
                source: MessageVariantSource::Alternate,
                ordinal: 0,
                status: MessageVariantStatus::Active,
                message,
                metadata_json: json!({}),
                created_at: "2026-06-19T00:02:00Z".to_string(),
                updated_at: "2026-06-19T00:02:00Z".to_string(),
            },
        })
        .unwrap_err();

    assert_eq!(error.kind, CoreErrorKind::NotFound);
}

#[test]
fn ensure_active_chat_conversation_branch_creates_and_selects_default() {
    let engine = test_engine();

    let result = engine
        .ensure_active_chat_conversation_branch(&EnsureActiveChatConversationBranchRequest {
            session_id: SessionId::new("ensure-branch-session"),
            branch_id: ConversationBranchId::new("ensure-branch-default"),
            label: Some("Default".to_string()),
            metadata_json: json!({ "source": "test" }),
            created_at: "2026-06-19T00:01:00Z".to_string(),
            updated_at: "2026-06-19T00:01:00Z".to_string(),
        })
        .unwrap();

    assert_eq!(
        result.branch.branch_id,
        ConversationBranchId::new("ensure-branch-default")
    );
    assert_eq!(
        result.state.active_branch_id,
        Some(ConversationBranchId::new("ensure-branch-default"))
    );
    assert!(result.conflict.is_none());
}

#[test]
fn ensure_active_chat_conversation_branch_selects_existing_default_when_none_active() {
    let engine = test_engine();
    save_test_branch(
        &engine,
        "ensure-existing-session",
        "ensure-existing-default",
        None,
        None,
    );

    let result = engine
        .ensure_active_chat_conversation_branch(&EnsureActiveChatConversationBranchRequest {
            session_id: SessionId::new("ensure-existing-session"),
            branch_id: ConversationBranchId::new("ensure-existing-default"),
            label: Some("Default".to_string()),
            metadata_json: json!({}),
            created_at: "2026-06-19T00:01:00Z".to_string(),
            updated_at: "2026-06-19T00:02:00Z".to_string(),
        })
        .unwrap();

    assert_eq!(
        result.state.active_branch_id,
        Some(ConversationBranchId::new("ensure-existing-default"))
    );
    assert!(result.conflict.is_none());
}

#[test]
fn ensure_active_chat_conversation_branch_returns_active_conflict() {
    let engine = test_engine();
    save_test_branch(
        &engine,
        "ensure-conflict-session",
        "ensure-conflict-active",
        None,
        None,
    );
    engine
        .select_active_conversation_branch(&SelectActiveBranchRequest {
            session_id: SessionId::new("ensure-conflict-session"),
            active_branch_id: Some(ConversationBranchId::new("ensure-conflict-active")),
            expected: rusty_crew_core_persistence::ActiveBranchExpectation::Any,
            updated_at: "2026-06-19T00:01:00Z".to_string(),
        })
        .unwrap();

    let result = engine
        .ensure_active_chat_conversation_branch(&EnsureActiveChatConversationBranchRequest {
            session_id: SessionId::new("ensure-conflict-session"),
            branch_id: ConversationBranchId::new("ensure-conflict-default"),
            label: Some("Default".to_string()),
            metadata_json: json!({}),
            created_at: "2026-06-19T00:02:00Z".to_string(),
            updated_at: "2026-06-19T00:02:00Z".to_string(),
        })
        .unwrap();

    assert_eq!(
        result.branch.branch_id,
        ConversationBranchId::new("ensure-conflict-active")
    );
    assert_eq!(
        result.conflict,
        Some(rusty_crew_core_persistence::ActiveBranchConflict {
            expected: None,
            actual: Some(ConversationBranchId::new("ensure-conflict-active")),
        })
    );
}

#[test]
fn create_chat_conversation_branch_rejects_wrong_session_parent_and_head() {
    let engine = test_engine();
    save_test_message_slot(
        &engine,
        "branch-owner-session",
        1,
        "assistant",
        "assistant",
        "primary",
    );
    save_test_message_slot(
        &engine,
        "branch-other-session",
        1,
        "assistant",
        "assistant",
        "primary",
    );
    save_test_branch(
        &engine,
        "branch-other-session",
        "branch-other-parent",
        None,
        None,
    );

    let parent_error = engine
        .create_chat_conversation_branch(&CreateChatConversationBranchRequest {
            branch: test_branch_write(
                "branch-owner-session",
                "branch-owner-child",
                Some("branch-other-parent"),
                Some("branch-owner-session-message-1"),
            ),
        })
        .unwrap_err();
    assert_eq!(parent_error.kind, CoreErrorKind::NotFound);

    let head_error = engine
        .create_chat_conversation_branch(&CreateChatConversationBranchRequest {
            branch: test_branch_write(
                "branch-owner-session",
                "branch-owner-child-2",
                None,
                Some("branch-other-session-message-1"),
            ),
        })
        .unwrap_err();
    assert_eq!(head_error.kind, CoreErrorKind::NotFound);
}

#[test]
fn create_chat_conversation_snapshot_rejects_cross_session_snapshot_collision() {
    let engine = test_engine();
    let first = engine
        .create_chat_conversation_snapshot(&CreateChatConversationSnapshotRequest {
            snapshot: test_snapshot_write("snapshot-session-a", "shared-snapshot", None, None),
        })
        .unwrap();
    assert_eq!(
        first.status,
        ChatConversationSnapshotMutationStatus::Created
    );

    let updated = engine
        .create_chat_conversation_snapshot(&CreateChatConversationSnapshotRequest {
            snapshot: ConversationSnapshotWrite {
                label: Some("Updated".to_string()),
                created_at: "2026-06-19T00:09:00Z".to_string(),
                updated_at: "2026-06-19T00:09:00Z".to_string(),
                ..test_snapshot_write("snapshot-session-a", "shared-snapshot", None, None)
            },
        })
        .unwrap();
    assert_eq!(
        updated.status,
        ChatConversationSnapshotMutationStatus::Updated
    );
    assert_eq!(
        updated.snapshot.created_at,
        "2026-06-19T00:01:00Z".to_string()
    );
    assert_eq!(updated.snapshot.label, Some("Updated".to_string()));

    let error = engine
        .create_chat_conversation_snapshot(&CreateChatConversationSnapshotRequest {
            snapshot: test_snapshot_write("snapshot-session-b", "shared-snapshot", None, None),
        })
        .unwrap_err();
    assert_eq!(error.kind, CoreErrorKind::NotFound);

    let records = engine
        .query_conversation_snapshots(&ConversationSnapshotQuery {
            session_id: Some(SessionId::new("snapshot-session-a")),
            branch_id: None,
            message_id: None,
            page: None,
        })
        .unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].session_id, SessionId::new("snapshot-session-a"));
}

#[test]
fn create_chat_conversation_snapshot_validates_branch_and_message_ownership() {
    let engine = test_engine();
    save_test_branch(
        &engine,
        "snapshot-owner-session",
        "snapshot-owner-branch",
        None,
        None,
    );
    save_test_branch(
        &engine,
        "snapshot-other-session",
        "snapshot-other-branch",
        None,
        None,
    );
    save_test_message_slot(
        &engine,
        "snapshot-other-session",
        1,
        "assistant",
        "assistant",
        "other",
    );

    let wrong_branch = engine
        .create_chat_conversation_snapshot(&CreateChatConversationSnapshotRequest {
            snapshot: test_snapshot_write(
                "snapshot-owner-session",
                "wrong-branch-snapshot",
                Some("snapshot-other-branch"),
                None,
            ),
        })
        .unwrap_err();
    assert_eq!(wrong_branch.kind, CoreErrorKind::NotFound);

    let wrong_message = engine
        .create_chat_conversation_snapshot(&CreateChatConversationSnapshotRequest {
            snapshot: test_snapshot_write(
                "snapshot-owner-session",
                "wrong-message-snapshot",
                None,
                Some("snapshot-other-session-message-1"),
            ),
        })
        .unwrap_err();
    assert_eq!(wrong_message.kind, CoreErrorKind::NotFound);
}

#[test]
fn create_chat_conversation_snapshot_allows_same_session_branch_and_message_anchors() {
    let engine = test_engine();
    save_test_branch(
        &engine,
        "snapshot-branch-session",
        "snapshot-branch-a",
        None,
        None,
    );
    save_test_branch(
        &engine,
        "snapshot-branch-session",
        "snapshot-branch-b",
        None,
        None,
    );
    engine
        .save_message_slot(&MessageSlotWrite {
            slot_id: MessageSlotId::new("snapshot-branch-slot"),
            session_id: SessionId::new("snapshot-branch-session"),
            primary_variant_id: MessageVariantId::new("snapshot-branch-primary"),
            active_variant_id: None,
            metadata_json: json!({}),
            created_at: "2026-06-19T00:01:00Z".to_string(),
            updated_at: "2026-06-19T00:01:00Z".to_string(),
        })
        .unwrap();
    let mut message = test_message_write(
        "snapshot-branch-session",
        1,
        "assistant",
        "assistant",
        "body",
    );
    message.branch_id = Some(ConversationBranchId::new("snapshot-branch-b"));
    engine
        .save_message_variant(&MessageVariantWrite {
            variant_id: MessageVariantId::new("snapshot-branch-primary"),
            slot_id: MessageSlotId::new("snapshot-branch-slot"),
            source: MessageVariantSource::Primary,
            ordinal: 0,
            status: MessageVariantStatus::Active,
            message,
            metadata_json: json!({}),
            created_at: "2026-06-19T00:01:00Z".to_string(),
            updated_at: "2026-06-19T00:01:00Z".to_string(),
        })
        .unwrap();

    let result = engine
        .create_chat_conversation_snapshot(&CreateChatConversationSnapshotRequest {
            snapshot: test_snapshot_write(
                "snapshot-branch-session",
                "independent-branch-message-snapshot",
                Some("snapshot-branch-a"),
                Some("snapshot-branch-session-message-1"),
            ),
        })
        .unwrap();
    assert_eq!(
        result.status,
        ChatConversationSnapshotMutationStatus::Created
    );
}

#[test]
fn create_chat_conversation_snapshot_allows_message_referenced_by_branch_head() {
    let engine = test_engine();
    save_test_message_slot(
        &engine,
        "snapshot-head-session",
        1,
        "assistant",
        "assistant",
        "head",
    );
    save_test_branch(
        &engine,
        "snapshot-head-session",
        "snapshot-head-branch",
        None,
        Some("snapshot-head-session-message-1"),
    );

    let result = engine
        .create_chat_conversation_snapshot(&CreateChatConversationSnapshotRequest {
            snapshot: test_snapshot_write(
                "snapshot-head-session",
                "snapshot-head-snapshot",
                Some("snapshot-head-branch"),
                Some("snapshot-head-session-message-1"),
            ),
        })
        .unwrap();

    assert_eq!(
        result.status,
        ChatConversationSnapshotMutationStatus::Created
    );
}

#[test]
fn create_chat_attachment_rejects_cross_session_attachment_collision() {
    let engine = test_engine();
    let first = engine
        .create_chat_attachment(&CreateChatAttachmentRequest {
            attachment: test_attachment_write("attachment-session-a", "shared-attachment", None),
        })
        .unwrap();
    assert_eq!(first.status, ChatAttachmentMutationStatus::Created);

    let error = engine
        .create_chat_attachment(&CreateChatAttachmentRequest {
            attachment: test_attachment_write("attachment-session-b", "shared-attachment", None),
        })
        .unwrap_err();

    assert_eq!(error.kind, CoreErrorKind::NotFound);
    let records = engine
        .query_attachments(&AttachmentQuery {
            session_id: Some(SessionId::new("attachment-session-a")),
            include_removed: true,
            include_expired: true,
            ..AttachmentQuery::default()
        })
        .unwrap();
    assert_eq!(
        records[0].session_id,
        SessionId::new("attachment-session-a")
    );
}

#[test]
fn create_chat_attachment_validates_link_targets() {
    let engine = test_engine();
    save_test_message_slot(
        &engine,
        "attachment-link-session",
        1,
        "assistant",
        "assistant",
        "primary",
    );
    save_test_message_slot(
        &engine,
        "attachment-other-session",
        1,
        "assistant",
        "assistant",
        "primary",
    );

    let linked = engine
        .create_chat_attachment(&CreateChatAttachmentRequest {
            attachment: test_attachment_write(
                "attachment-link-session",
                "linked-attachment",
                Some(AttachmentLinkWrite {
                    link_id: AttachmentLinkId::new("linked-attachment-link"),
                    attachment_id: AttachmentId::new("linked-attachment"),
                    session_id: SessionId::new("attachment-link-session"),
                    message_id: Some(MessageId::new("attachment-link-session-message-1")),
                    block_id: None,
                    scope_id: None,
                    metadata_json: json!({}),
                    created_at: "2026-06-19T00:01:00Z".to_string(),
                }),
            ),
        })
        .unwrap();
    assert_eq!(linked.status, ChatAttachmentMutationStatus::Linked);
    assert_eq!(linked.attachment.links.len(), 1);

    let error = engine
        .create_chat_attachment(&CreateChatAttachmentRequest {
            attachment: test_attachment_write(
                "attachment-link-session",
                "wrong-link-attachment",
                Some(AttachmentLinkWrite {
                    link_id: AttachmentLinkId::new("wrong-link-attachment-link"),
                    attachment_id: AttachmentId::new("wrong-link-attachment"),
                    session_id: SessionId::new("attachment-link-session"),
                    message_id: Some(MessageId::new("attachment-other-session-message-1")),
                    block_id: None,
                    scope_id: None,
                    metadata_json: json!({}),
                    created_at: "2026-06-19T00:01:00Z".to_string(),
                }),
            ),
        })
        .unwrap_err();
    assert_eq!(error.kind, CoreErrorKind::NotFound);
}

#[test]
fn remove_chat_attachment_is_session_scoped() {
    let engine = test_engine();
    engine
        .create_chat_attachment(&CreateChatAttachmentRequest {
            attachment: test_attachment_write(
                "remove-attachment-session",
                "remove-attachment",
                None,
            ),
        })
        .unwrap();

    let error = engine
        .remove_chat_attachment(&RemoveChatAttachmentRequest {
            session_id: SessionId::new("remove-other-session"),
            attachment_id: AttachmentId::new("remove-attachment"),
            updated_at: "2026-06-19T00:02:00Z".to_string(),
        })
        .unwrap_err();
    assert_eq!(error.kind, CoreErrorKind::NotFound);

    let record = engine
        .remove_chat_attachment(&RemoveChatAttachmentRequest {
            session_id: SessionId::new("remove-attachment-session"),
            attachment_id: AttachmentId::new("remove-attachment"),
            updated_at: "2026-06-19T00:03:00Z".to_string(),
        })
        .unwrap();
    assert_eq!(record.status, AttachmentStatus::Removed);
}

#[test]
fn create_chat_data_bank_scope_rejects_cross_session_scope_collision() {
    let engine = test_engine();
    let first = engine
        .create_chat_data_bank_scope(&CreateChatDataBankScopeRequest {
            scope: test_data_bank_scope_write("scope-session-a", "shared-scope"),
        })
        .unwrap();
    assert_eq!(first.status, ChatDataBankScopeMutationStatus::Created);

    let error = engine
        .create_chat_data_bank_scope(&CreateChatDataBankScopeRequest {
            scope: test_data_bank_scope_write("scope-session-b", "shared-scope"),
        })
        .unwrap_err();
    assert_eq!(error.kind, CoreErrorKind::NotFound);

    let records = engine
        .query_data_bank_scopes(&DataBankScopeQuery {
            session_id: Some(SessionId::new("scope-session-a")),
            include_removed: true,
            ..DataBankScopeQuery::default()
        })
        .unwrap();
    assert_eq!(records[0].session_id, SessionId::new("scope-session-a"));
}

#[test]
fn remove_chat_data_bank_scope_is_session_scoped() {
    let engine = test_engine();
    engine
        .create_chat_data_bank_scope(&CreateChatDataBankScopeRequest {
            scope: test_data_bank_scope_write("remove-scope-session", "remove-scope"),
        })
        .unwrap();

    let error = engine
        .remove_chat_data_bank_scope(&RemoveChatDataBankScopeRequest {
            session_id: SessionId::new("remove-other-session"),
            scope_id: DataBankScopeId::new("remove-scope"),
            updated_at: "2026-06-19T00:02:00Z".to_string(),
        })
        .unwrap_err();
    assert_eq!(error.kind, CoreErrorKind::NotFound);

    let record = engine
        .remove_chat_data_bank_scope(&RemoveChatDataBankScopeRequest {
            session_id: SessionId::new("remove-scope-session"),
            scope_id: DataBankScopeId::new("remove-scope"),
            updated_at: "2026-06-19T00:03:00Z".to_string(),
        })
        .unwrap();
    assert_eq!(record.status, DataBankScopeStatus::Removed);
}

#[test]
fn delete_chat_message_variant_validates_slot_session_ownership() {
    let engine = test_engine();
    save_test_message_slot(
        &engine,
        "delete-owned-session",
        1,
        "assistant",
        "assistant",
        "primary",
    );
    save_test_alternate_variant(&engine, "delete-owned-session", 1, 2, "alt");

    let error = engine
        .delete_chat_message_variant(&DeleteChatMessageVariantRequest {
            session_id: SessionId::new("other-delete-session"),
            slot_id: MessageSlotId::new("delete-owned-session-slot-1"),
            variant_id: MessageVariantId::new("delete-owned-session-variant-2-alt"),
            updated_at: "2026-06-19T00:04:00Z".to_string(),
        })
        .unwrap_err();

    assert_eq!(error.kind, CoreErrorKind::NotFound);
}

#[test]
fn delete_chat_message_variant_rejects_primary_variant() {
    let engine = test_engine();
    save_test_message_slot(
        &engine,
        "delete-primary-session",
        1,
        "assistant",
        "assistant",
        "primary",
    );

    let error = engine
        .delete_chat_message_variant(&DeleteChatMessageVariantRequest {
            session_id: SessionId::new("delete-primary-session"),
            slot_id: MessageSlotId::new("delete-primary-session-slot-1"),
            variant_id: MessageVariantId::new("delete-primary-session-variant-1-primary"),
            updated_at: "2026-06-19T00:04:00Z".to_string(),
        })
        .unwrap_err();

    assert_eq!(error.kind, CoreErrorKind::InvalidInput);
}

#[test]
fn delete_chat_message_variant_clears_active_alternate() {
    let engine = test_engine();
    save_test_message_slot(
        &engine,
        "delete-active-session",
        1,
        "assistant",
        "assistant",
        "primary",
    );
    save_test_alternate_variant(&engine, "delete-active-session", 1, 2, "alt");
    engine
        .select_active_message_variant(&SelectActiveVariantRequest {
            slot_id: MessageSlotId::new("delete-active-session-slot-1"),
            active_variant_id: Some(MessageVariantId::new("delete-active-session-variant-2-alt")),
            expected: rusty_crew_core_persistence::ActiveVariantExpectation::Any,
            updated_at: "2026-06-19T00:03:00Z".to_string(),
        })
        .unwrap();

    let slot = engine
        .delete_chat_message_variant(&DeleteChatMessageVariantRequest {
            session_id: SessionId::new("delete-active-session"),
            slot_id: MessageSlotId::new("delete-active-session-slot-1"),
            variant_id: MessageVariantId::new("delete-active-session-variant-2-alt"),
            updated_at: "2026-06-19T00:04:00Z".to_string(),
        })
        .unwrap();

    assert_eq!(slot.active_variant_id, None);
    assert!(slot.alternates.is_empty());
}

#[test]
fn reorder_chat_message_variants_validates_session_and_reorders() {
    let engine = test_engine();
    save_test_message_slot(
        &engine,
        "reorder-session",
        1,
        "assistant",
        "assistant",
        "primary",
    );
    save_test_alternate_variant(&engine, "reorder-session", 1, 2, "alt 1");
    save_test_alternate_variant(&engine, "reorder-session", 1, 3, "alt 2");

    let mismatch = engine
        .reorder_chat_message_variants(&ReorderChatMessageVariantsRequest {
            session_id: SessionId::new("other-reorder-session"),
            slot_id: MessageSlotId::new("reorder-session-slot-1"),
            ordered_variant_ids: vec![
                MessageVariantId::new("reorder-session-variant-3-alt"),
                MessageVariantId::new("reorder-session-variant-2-alt"),
            ],
            updated_at: "2026-06-19T00:04:00Z".to_string(),
        })
        .unwrap_err();
    assert_eq!(mismatch.kind, CoreErrorKind::NotFound);

    let variants = engine
        .reorder_chat_message_variants(&ReorderChatMessageVariantsRequest {
            session_id: SessionId::new("reorder-session"),
            slot_id: MessageSlotId::new("reorder-session-slot-1"),
            ordered_variant_ids: vec![
                MessageVariantId::new("reorder-session-variant-3-alt"),
                MessageVariantId::new("reorder-session-variant-2-alt"),
            ],
            updated_at: "2026-06-19T00:05:00Z".to_string(),
        })
        .unwrap();

    let alternate_order = variants
        .iter()
        .filter(|variant| variant.source == MessageVariantSource::Alternate)
        .map(|variant| (variant.variant_id.clone(), variant.ordinal))
        .collect::<Vec<_>>();
    assert_eq!(
        alternate_order,
        vec![
            (MessageVariantId::new("reorder-session-variant-3-alt"), 1),
            (MessageVariantId::new("reorder-session-variant-2-alt"), 2),
        ]
    );
}

#[test]
fn reorder_chat_message_variants_rejects_primary_variant() {
    let engine = test_engine();
    save_test_message_slot(
        &engine,
        "reorder-primary-session",
        1,
        "assistant",
        "assistant",
        "primary",
    );

    let error = engine
        .reorder_chat_message_variants(&ReorderChatMessageVariantsRequest {
            session_id: SessionId::new("reorder-primary-session"),
            slot_id: MessageSlotId::new("reorder-primary-session-slot-1"),
            ordered_variant_ids: vec![MessageVariantId::new(
                "reorder-primary-session-variant-1-primary",
            )],
            updated_at: "2026-06-19T00:04:00Z".to_string(),
        })
        .unwrap_err();

    assert_eq!(error.kind, CoreErrorKind::InvalidInput);
}
