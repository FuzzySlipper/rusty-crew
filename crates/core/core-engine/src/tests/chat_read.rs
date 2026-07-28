use super::*;

#[test]
fn chat_read_model_projects_slots_with_cursor_and_has_more() {
    let engine = test_engine();
    engine
        .create_session(session_config(
            "chat-session",
            "prime-agent",
            "prime-profile",
            SessionKind::Full,
        ))
        .unwrap();
    save_test_message_slot(&engine, "chat-session", 1, "operator", "user", "hello");
    save_test_message_slot(&engine, "chat-session", 2, "prime-agent", "assistant", "hi");
    save_test_message_slot(&engine, "chat-session", 3, "operator", "user", "again");

    let page = engine
        .chat_read_model_page(&ChatReadModelQuery {
            session_id: SessionId::new("chat-session"),
            agent_id: "prime-agent".to_string(),
            cursor: Some("chat-session:1".to_string()),
            limit: Some(1),
        })
        .unwrap();

    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].event_id, "chat-session:2");
    assert_eq!(page.items[0].sequence_id, 2);
    assert_eq!(page.items[0].kind, ChatReadModelEventKind::MessageCreated);
    assert_eq!(page.items[0].payload_json["role"], "assistant");
    assert_eq!(page.items[0].payload_json["body"], "hi");
    assert_eq!(page.items[0].payload_json["source"], "durable_message_slot");
    assert_eq!(page.latest_cursor, "chat-session:2");
    assert!(page.has_more);
    assert_eq!(page.total, 3);
    assert_eq!(page.source, ChatReadModelSource::MessageSlots);
}

#[test]
fn chat_session_read_and_summary_choose_durable_sources_explicitly() {
    let engine = test_engine();
    let pending = engine
        .create_session(session_config(
            "pending-chat-session",
            "pending-agent",
            "prime-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let logged = engine
        .create_session(session_config(
            "logged-chat-session",
            "logged-agent",
            "prime-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine
        .route_agent_message(AgentMessage {
            from: AgentId::new("operator"),
            to: pending.agent_id.clone(),
            body: "pending hello".to_string(),
            correlation_id: Some("pending-correlation".to_string()),
            projection: None,
        })
        .unwrap();
    engine
        .append_chat_event(&ChatEventLogAppend {
            session_id: logged.session_id.clone(),
            created_at: "2026-06-19T00:01:00Z".to_string(),
            kind: "message_created".to_string(),
            payload_json: json!({"body": "logged hello"}),
        })
        .unwrap();
    engine
        .append_chat_event(&ChatEventLogAppend {
            session_id: logged.session_id.clone(),
            created_at: "2026-06-19T00:01:02Z".to_string(),
            kind: "assistant_message_completed".to_string(),
            payload_json: json!({"body": "logged reply"}),
        })
        .unwrap();
    engine
        .append_chat_event(&ChatEventLogAppend {
            session_id: logged.session_id.clone(),
            created_at: "2026-06-19T00:01:01Z".to_string(),
            kind: "tool_call_completed".to_string(),
            payload_json: json!({"tool_name": "read_file"}),
        })
        .unwrap();

    let pending_read = engine
        .read_chat_session(&ChatSessionReadQuery {
            session_id: pending.session_id.clone(),
            cursor: None,
            limit: 10,
            include_alternates: false,
        })
        .unwrap();
    assert_eq!(pending_read.source, ChatReadModelSource::PendingMessages);
    assert_eq!(pending_read.total, 1);
    assert_eq!(pending_read.events[0].payload_json["body"], "pending hello");
    assert_eq!(pending_read.message_slots.total, 0);

    let logged_read = engine
        .read_chat_session(&ChatSessionReadQuery {
            session_id: logged.session_id.clone(),
            cursor: None,
            limit: 10,
            include_alternates: false,
        })
        .unwrap();
    assert_eq!(logged_read.source, ChatReadModelSource::EventLog);
    assert_eq!(logged_read.total, 3);
    assert_eq!(logged_read.message_count, 2);
    assert_eq!(logged_read.events[0].payload_json["body"], "logged hello");

    let summaries = engine
        .query_chat_session_summaries(&ChatSessionSummaryPageQuery {
            profile_id: Some(ProfileId::new("prime-profile")),
            status: Some("idle".to_string()),
            page: rusty_crew_core_persistence::QueryPage {
                limit: Some(1),
                offset: Some(0),
            },
        })
        .unwrap();
    assert_eq!(summaries.page.total, 2);
    assert_eq!(summaries.page.items.len(), 1);
    assert_eq!(summaries.page.next_offset, Some(1));
    assert_eq!(summaries.page.items[0].message_count, 2);
}

#[test]
fn chat_session_read_sources_survive_engine_restart() {
    let data_dir = unique_data_dir("chat-session-read-restart");
    let engine = test_engine_with_data_dir(data_dir.clone());
    let pending = engine
        .create_session(session_config(
            "restart-pending-session",
            "restart-pending-agent",
            "prime-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let logged = engine
        .create_session(session_config(
            "restart-logged-session",
            "restart-logged-agent",
            "prime-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine
        .route_agent_message(AgentMessage {
            from: AgentId::new("operator"),
            to: pending.agent_id.clone(),
            body: "pending across restart".to_string(),
            correlation_id: Some("restart-pending".to_string()),
            projection: None,
        })
        .unwrap();
    engine
        .append_chat_event(&ChatEventLogAppend {
            session_id: logged.session_id.clone(),
            created_at: "2026-06-19T00:01:00Z".to_string(),
            kind: "message_created".to_string(),
            payload_json: json!({"body": "logged across restart"}),
        })
        .unwrap();
    drop(engine);

    let restarted = test_engine_with_data_dir(data_dir);
    let pending_read = restarted
        .read_chat_session(&ChatSessionReadQuery {
            session_id: pending.session_id,
            cursor: None,
            limit: 10,
            include_alternates: false,
        })
        .unwrap();
    assert_eq!(pending_read.source, ChatReadModelSource::PendingMessages);
    assert_eq!(
        pending_read.events[0].payload_json["body"],
        "pending across restart"
    );
    let logged_read = restarted
        .read_chat_session(&ChatSessionReadQuery {
            session_id: logged.session_id,
            cursor: None,
            limit: 10,
            include_alternates: false,
        })
        .unwrap();
    assert_eq!(logged_read.source, ChatReadModelSource::EventLog);
    assert_eq!(
        logged_read.events[0].payload_json["body"],
        "logged across restart"
    );
}

#[test]
fn archived_chat_sessions_require_explicit_history_filter() {
    let data_dir = unique_data_dir("archived-chat-history");
    let engine = test_engine_with_data_dir(data_dir.clone());
    let session = engine
        .create_session(session_config(
            "archived-chat-session",
            "archived-agent",
            "archived-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine
        .append_chat_event(&ChatEventLogAppend {
            session_id: session.session_id.clone(),
            created_at: "2026-06-19T00:01:00Z".to_string(),
            kind: "message_created".to_string(),
            payload_json: json!({"body": "preserved history"}),
        })
        .unwrap();
    engine.archive_session(&session.session_id).unwrap();

    let default_page = engine
        .query_chat_session_summaries(&ChatSessionSummaryPageQuery {
            profile_id: None,
            status: None,
            page: QueryPage::default(),
        })
        .unwrap();
    assert!(default_page.page.items.is_empty());

    let archived_page = engine
        .query_chat_session_summaries(&ChatSessionSummaryPageQuery {
            profile_id: None,
            status: Some("archived".to_string()),
            page: QueryPage::default(),
        })
        .unwrap();
    assert_eq!(archived_page.page.items.len(), 1);
    assert_eq!(archived_page.page.items[0].message_count, 1);
    drop(engine);

    let restarted = test_engine_with_data_dir(data_dir);
    let history = restarted
        .read_chat_session(&ChatSessionReadQuery {
            session_id: SessionId::new("archived-chat-session"),
            cursor: None,
            limit: 10,
            include_alternates: false,
        })
        .unwrap();
    assert_eq!(history.session.status, SessionStatus::Archived);
    assert_eq!(history.events[0].payload_json["body"], "preserved history");
}

#[test]
fn chat_read_model_uses_active_alternate_and_forgives_bad_cursors() {
    let engine = test_engine();
    engine
        .create_session(session_config(
            "variant-session",
            "prime-agent",
            "prime-profile",
            SessionKind::Full,
        ))
        .unwrap();
    save_test_message_slot(
        &engine,
        "variant-session",
        1,
        "prime-agent",
        "assistant",
        "primary",
    );
    engine
        .save_message_variant(&MessageVariantWrite {
            variant_id: MessageVariantId::new("variant-session-variant-1-alt"),
            slot_id: MessageSlotId::new("variant-session-slot-1"),
            source: MessageVariantSource::Alternate,
            ordinal: 1,
            status: MessageVariantStatus::Active,
            message: test_message_write(
                "variant-session",
                10,
                "prime-agent",
                "assistant",
                "alternate",
            ),
            metadata_json: json!({}),
            created_at: "2026-06-19T00:10:00Z".to_string(),
            updated_at: "2026-06-19T00:10:00Z".to_string(),
        })
        .unwrap();
    engine
        .select_active_message_variant(&SelectActiveVariantRequest {
            slot_id: MessageSlotId::new("variant-session-slot-1"),
            active_variant_id: Some(MessageVariantId::new("variant-session-variant-1-alt")),
            expected: rusty_crew_core_persistence::ActiveVariantExpectation::Any,
            updated_at: "2026-06-19T00:11:00Z".to_string(),
        })
        .unwrap();

    let page = engine
        .chat_read_model_page(&ChatReadModelQuery {
            session_id: SessionId::new("variant-session"),
            agent_id: "prime-agent".to_string(),
            cursor: Some("other-session:not-a-number".to_string()),
            limit: Some(10),
        })
        .unwrap();

    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].event_id, "variant-session:1");
    assert_eq!(page.items[0].payload_json["body"], "alternate");
    assert_eq!(page.latest_cursor, "variant-session:1");
    assert!(!page.has_more);
}

#[test]
fn chat_event_log_allocates_sequences_and_pages_after_cursor() {
    let engine = test_engine();

    let first = engine
        .append_chat_event(&ChatEventLogAppend {
            session_id: SessionId::new("stream-session"),
            created_at: "2026-06-19T00:01:00Z".to_string(),
            kind: "message_created".to_string(),
            payload_json: json!({ "body": "hello" }),
        })
        .unwrap();
    let second = engine
        .append_chat_event(&ChatEventLogAppend {
            session_id: SessionId::new("stream-session"),
            created_at: "2026-06-19T00:02:00Z".to_string(),
            kind: "assistant_text_delta".to_string(),
            payload_json: json!({ "delta": "hi" }),
        })
        .unwrap();

    assert_eq!(first.event_id, "stream-session:1");
    assert_eq!(second.event_id, "stream-session:2");

    let page = engine
        .query_chat_events(&ChatEventLogQuery {
            session_id: SessionId::new("stream-session"),
            cursor: Some("stream-session:1".to_string()),
            limit: Some(1),
        })
        .unwrap();

    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].event_id, "stream-session:2");
    assert_eq!(page.items[0].kind, "assistant_text_delta");
    assert_eq!(page.latest_cursor, "stream-session:2");
    assert!(!page.has_more);

    let latest = engine
        .query_chat_events(&ChatEventLogQuery {
            session_id: SessionId::new("stream-session"),
            cursor: None,
            limit: Some(1),
        })
        .unwrap();

    assert_eq!(latest.items.len(), 1);
    assert_eq!(latest.items[0].event_id, "stream-session:2");
    assert_eq!(latest.latest_cursor, "stream-session:2");
    assert!(latest.has_more);
}

#[test]
fn chat_event_log_replays_after_store_reload_without_memory_state() {
    let data_dir = unique_data_dir("chat-events-reload");
    {
        let engine = test_engine_with_data_dir(data_dir.clone());
        for index in 1..=3 {
            engine
                .append_chat_event(&ChatEventLogAppend {
                    session_id: SessionId::new("reload-session"),
                    created_at: format!("2026-06-19T00:0{index}:00Z"),
                    kind: "message_created".to_string(),
                    payload_json: json!({ "body": format!("message {index}") }),
                })
                .unwrap();
        }
    }

    let engine = test_engine_with_data_dir(data_dir);
    let page = engine
        .query_chat_events(&ChatEventLogQuery {
            session_id: SessionId::new("reload-session"),
            cursor: Some("reload-session:1".to_string()),
            limit: Some(1),
        })
        .unwrap();

    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].event_id, "reload-session:2");
    assert_eq!(page.items[0].payload_json["body"], "message 2");
    assert_eq!(page.latest_cursor, "reload-session:2");
    assert!(page.has_more);
}

#[test]
fn resolve_conversation_jump_rejects_wrong_session_targets() {
    let engine = test_engine();
    save_test_branch(
        &engine,
        "jump-owner-session",
        "jump-owner-branch",
        None,
        None,
    );
    save_test_message_slot(
        &engine,
        "jump-owner-session",
        1,
        "assistant",
        "assistant",
        "owner",
    );
    engine
        .create_chat_conversation_snapshot(&CreateChatConversationSnapshotRequest {
            snapshot: test_snapshot_write(
                "jump-owner-session",
                "jump-owner-snapshot",
                Some("jump-owner-branch"),
                None,
            ),
        })
        .unwrap();

    for target in [
        ConversationJumpTarget::Branch {
            branch_id: ConversationBranchId::new("jump-owner-branch"),
        },
        ConversationJumpTarget::Message {
            message_id: MessageId::new("jump-owner-session-message-1"),
        },
        ConversationJumpTarget::Snapshot {
            snapshot_id: ConversationSnapshotId::new("jump-owner-snapshot"),
        },
    ] {
        let error = engine
            .resolve_conversation_jump(&ConversationJumpRequest {
                session_id: SessionId::new("jump-other-session"),
                target,
            })
            .unwrap_err();
        assert_eq!(error.kind, CoreErrorKind::NotFound);
    }
}
