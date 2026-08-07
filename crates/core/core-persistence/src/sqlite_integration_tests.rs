//! SQLite integration and cross-repository persistence tests.
//!
//! Domain-specific tests should live beside their repository modules once those
//! modules exist. This file keeps broad cross-repository and legacy integration
//! coverage out of the crate entrypoint while the remaining persistence domains
//! are decomposed.

use super::*;
use crate::repos::runtime_counters::COUNTER_MESSAGES;
use rusty_crew_core_protocol::{
    AgentMessage, ChatCompletionsPromptCachingPolicy, MemoryConflictPolicy,
    MemoryDiagnosticsPolicy, MemoryEvidenceKind, MemoryEvidenceRef, MemoryExportImportPolicy,
    MemoryFieldType, MemoryIndexingPolicy, MemoryOperationPolicy, MemoryPromptPolicy,
    MemoryProvenancePolicy, MemoryRecordFieldDescriptor, MemoryRecordShapeDescriptor,
    MemoryRecordShapeId, MemoryRecordShapeRef, MemoryRetentionPolicy, MemoryRetrievalStrategy,
    MemoryScope, MemoryScopeModel, MemorySpaceId, MemoryVisibilityModel, MemoryWritePolicy,
    ModelProviderCredentialKind, ProfileRegistryDerivedRuntimeRef,
    ProfileRegistryImportExportMetadata, ProfileRegistrySourceAssetRef, ToolDescriptor,
    MODEL_PROVIDER_SECRET_ENVELOPE_VERSION,
};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Barrier},
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

mod repository_conformance {
    use super::*;

    trait RepositoryConformanceBackend {
        fn with_store<F>(&self, label: &str, test: F)
        where
            F: FnOnce(&CoordinationStore);
    }

    struct SqliteRepositoryConformance;

    impl RepositoryConformanceBackend for SqliteRepositoryConformance {
        fn with_store<F>(&self, label: &str, test: F)
        where
            F: FnOnce(&CoordinationStore),
        {
            let db_path = temp_db_path(&format!("sqlite-conformance-{label}"));
            let store = CoordinationStore::open_file(&db_path).unwrap();
            test(&store);
            remove_temp_db(&db_path);
        }
    }

    struct SqliteFacadeRepositoryConformance;

    impl RepositoryConformanceBackend for SqliteFacadeRepositoryConformance {
        fn with_store<F>(&self, label: &str, test: F)
        where
            F: FnOnce(&CoordinationStore),
        {
            let db_path = temp_db_path(&format!("sqlite-facade-conformance-{label}"));
            let store = CoreCoordinationStore::open_sqlite_file(&db_path).unwrap();
            assert_eq!(store.backend(), CoreCoordinationStoreBackend::Sqlite);
            test(store.sqlite_compat_store());
            remove_temp_db(&db_path);
        }
    }

    #[test]
    fn sqlite_satisfies_repository_conformance_suite() {
        run_repository_conformance_suite(&SqliteRepositoryConformance);
    }

    #[test]
    fn sqlite_facade_satisfies_repository_conformance_suite() {
        run_repository_conformance_suite(&SqliteFacadeRepositoryConformance);
    }

    #[test]
    fn sqlite_store_facades_expose_distinct_concern_boundaries() {
        let db_path = temp_db_path("sqlite-store-facades");
        let store = CoreCoordinationStore::open_sqlite_file(&db_path).unwrap();

        let state = sample_session_state();
        let config = sample_session_config();
        store
            .coordination()
            .save_session_with_config(&state, &config)
            .unwrap();
        assert_eq!(store.coordination().load_sessions().unwrap().len(), 1);

        let profile = store
            .service_data()
            .create_profile_registry_record(&profile_registry_write("facade-profile"))
            .unwrap();
        assert_eq!(profile.profile_id, ProfileId::new("facade-profile"));

        let scope = SimpleKvScope {
            scope_type: "profile".to_string(),
            scope_id: "facade-profile".to_string(),
        };
        store
            .module_data()
            .put_simple_kv(&SimpleKvWrite {
                scope: scope.clone(),
                key: "checkpoint".to_string(),
                value_json: json!({"ok": true}),
                now: "2026-07-02T00:00:00Z".to_string(),
                expires_at: None,
            })
            .unwrap();
        assert_eq!(
            store
                .module_data()
                .list_simple_kv(&SimpleKvQuery {
                    scope,
                    key_prefix: Some("check".to_string()),
                    include_expired: false,
                    expired_only: false,
                    now: Some("2026-07-02T00:01:00Z".to_string()),
                    page: Some(page()),
                })
                .unwrap()
                .len(),
            1
        );

        store
            .memory()
            .add_roleplay_lore_record(&roleplay_lore_write(
                "facade-lore",
                "facade-world",
                None,
                "Facade Lore",
                "Facade memory/lore boundary survives restart.",
                "2026-07-02T00:00:00Z",
            ))
            .unwrap();
        assert_eq!(
            store
                .memory()
                .query_roleplay_lore_records(&RoleplayLoreQuery {
                    world_id: Some("facade-world".to_string()),
                    ..RoleplayLoreQuery::default()
                })
                .unwrap()
                .len(),
            1
        );

        assert!(store.admin().database_size().unwrap().database_bytes > 0);

        drop(store);
        let reopened = CoreCoordinationStore::open_sqlite_file(&db_path).unwrap();
        assert_eq!(reopened.coordination().load_sessions().unwrap().len(), 1);
        assert!(reopened
            .service_data()
            .get_profile_registry_record(&ProfileId::new("facade-profile"))
            .unwrap()
            .is_some());

        remove_temp_db(&db_path);
    }

    fn run_repository_conformance_suite<B: RepositoryConformanceBackend>(backend: &B) {
        session_persistence_contract(backend);
        event_ordering_projection_contract(backend);
        queued_message_ttl_no_resurrection_contract(backend);
        scheduler_claim_and_expiry_contract(backend);
        runtime_counters_contract(backend);
        dense_profile_memory_revision_contract(backend);
        runtime_search_contract(backend);
        conversation_branch_message_contract(backend);
        provider_wire_state_expiry_contract(backend);
        model_provider_secret_envelope_contract(backend);
        model_provider_prompt_caching_contract(backend);
    }

    fn page() -> QueryPage {
        QueryPage {
            limit: Some(10),
            offset: Some(0),
        }
    }

    fn session_persistence_contract<B: RepositoryConformanceBackend>(backend: &B) {
        backend.with_store("session-persistence", |store| {
            let state = sample_session_state();
            let config = sample_session_config();
            store.save_session_with_config(&state, &config).unwrap();

            let sessions = store
                .query_sessions(&SessionQuery {
                    agent_id: Some(AgentId::new("agent-alpha")),
                    profile_id: Some(ProfileId::new("full-profile")),
                    kind: Some(SessionKind::Full),
                    status: Some(SessionStatus::Idle),
                    page: Some(page()),
                })
                .unwrap();
            let configs = store.load_session_configs().unwrap();
            let identities = store.load_session_identities().unwrap();

            assert_eq!(sessions.len(), 1);
            assert_eq!(sessions[0].session_id, SessionId::new("session-alpha"));
            assert_eq!(configs.len(), 1);
            assert_eq!(
                configs[0].config.resource_limits.max_duration_ms,
                Some(60_000)
            );
            assert_eq!(configs[0].tool_profile.tools[0].name, "apply_patch");
            assert_eq!(identities.len(), 1);
            assert_eq!(
                identities[0].instance_id,
                AgentInstanceId::new("instance:session-alpha")
            );
        });
    }

    fn event_ordering_projection_contract<B: RepositoryConformanceBackend>(backend: &B) {
        backend.with_store("event-ordering-projections", |store| {
            let session = sample_session_state();
            store
                .save_event(
                    1,
                    &CoreEvent::SessionCreated {
                        state: Box::new(session.clone()),
                    },
                )
                .unwrap();
            store
                .save_event(
                    2,
                    &CoreEvent::AgentMessageRouted {
                        message: AgentMessage {
                            from: AgentId::new("agent-alpha"),
                            to: AgentId::new("agent-beta"),
                            body: "projected conformance message".to_string(),
                            correlation_id: Some("conformance-corr".to_string()),
                            projection: None,
                        },
                    },
                )
                .unwrap();
            store
                .save_event(
                    3,
                    &CoreEvent::BrainEventObserved {
                        session_id: session.session_id.clone(),
                        wake_id: Some("wake-conformance".to_string()),
                        event: BrainEvent::Started,
                    },
                )
                .unwrap();

            let all = store
                .query_events(&RuntimeEventFilter {
                    limit: Some(10),
                    ..RuntimeEventFilter::default()
                })
                .unwrap();
            let by_session = store
                .query_events(&RuntimeEventFilter {
                    session_id: Some(SessionId::new("session-alpha")),
                    ..RuntimeEventFilter::default()
                })
                .unwrap();
            let by_agent = store
                .query_events(&RuntimeEventFilter {
                    agent_id: Some(AgentId::new("agent-beta")),
                    ..RuntimeEventFilter::default()
                })
                .unwrap();
            let by_correlation = store
                .query_events(&RuntimeEventFilter {
                    correlation_id: Some("conformance-corr".to_string()),
                    ..RuntimeEventFilter::default()
                })
                .unwrap();
            let by_wake = store
                .query_events(&RuntimeEventFilter {
                    source_wake_id: Some("wake-conformance".to_string()),
                    ..RuntimeEventFilter::default()
                })
                .unwrap();

            assert_eq!(
                all.iter().map(|event| event.sequence).collect::<Vec<_>>(),
                vec![1, 2, 3]
            );
            assert_eq!(by_session.len(), 2);
            assert_eq!(by_agent.len(), 1);
            assert_eq!(by_agent[0].agent_ids.len(), 2);
            assert_eq!(by_correlation[0].sequence, 2);
            assert_eq!(by_wake[0].source_wake_ids, vec!["wake-conformance"]);
        });
    }

    fn queued_message_ttl_no_resurrection_contract<B: RepositoryConformanceBackend>(backend: &B) {
        backend.with_store("queue-ttl-no-resurrection", |store| {
            let record = QueuedMessageRecord {
                message_id: "queue-conformance-1".to_string(),
                owner_session_id: Some(SessionId::new("session-alpha")),
                owner_agent_id: AgentId::new("agent-alpha"),
                message: AgentMessage {
                    from: AgentId::new("operator"),
                    to: AgentId::new("agent-alpha"),
                    body: "ttl bounded conformance queue".to_string(),
                    correlation_id: Some("queue-conformance".to_string()),
                    projection: None,
                },
                source_sequence: Some(42),
                enqueued_at: "2026-06-20T00:00:00Z".to_string(),
                expires_at: "2026-06-20T00:00:05Z".to_string(),
                ttl_ms: 5_000,
                delivery_attempts: 0,
                state: QueuedMessageState::Pending,
                terminal_at: None,
                state_reason: None,
            };

            store.save_queued_message(&record).unwrap();
            assert_eq!(pending_queue_messages(store).len(), 1);
            assert!(store
                .expire_queued_messages_at(&"2026-06-20T00:00:04Z".to_string())
                .unwrap()
                .is_empty());
            assert_eq!(pending_queue_messages(store).len(), 1);

            let expired = store
                .expire_queued_messages_at(&"2026-06-20T00:00:06Z".to_string())
                .unwrap();
            assert_eq!(expired.len(), 1);
            assert_eq!(expired[0].state, QueuedMessageState::Expired);
            assert_eq!(expired[0].state_reason.as_deref(), Some("ttl_expired"));
            assert!(pending_queue_messages(store).is_empty());

            let expired_query = store
                .load_queued_messages(&QueuedMessageFilter {
                    state: Some(QueuedMessageState::Expired),
                    owner_session_id: Some(SessionId::new("session-alpha")),
                    owner_agent_id: Some(AgentId::new("agent-alpha")),
                    limit: Some(10),
                })
                .unwrap();
            assert_eq!(expired_query.len(), 1);
            assert!(store
                .expire_queued_messages_at(&"2026-06-20T00:00:10Z".to_string())
                .unwrap()
                .is_empty());
            assert!(pending_queue_messages(store).is_empty());
            assert_eq!(
                store
                    .runtime_summary(&RuntimeCounterScope::Session(SessionId::new(
                        "session-alpha"
                    )))
                    .unwrap()
                    .queue_expirations,
                1
            );
        });
    }

    fn pending_queue_messages(store: &CoordinationStore) -> Vec<QueuedMessageRecord> {
        store
            .load_queued_messages(&QueuedMessageFilter {
                state: Some(QueuedMessageState::Pending),
                owner_session_id: Some(SessionId::new("session-alpha")),
                owner_agent_id: Some(AgentId::new("agent-alpha")),
                limit: Some(10),
            })
            .unwrap()
    }

    fn scheduler_claim_and_expiry_contract<B: RepositoryConformanceBackend>(backend: &B) {
        backend.with_store("scheduler-claim-expiry", |store| {
            store
                .upsert_scheduled_job(&ScheduledJobRecord {
                    job_id: "conformance-wake".to_string(),
                    job_kind: "wake".to_string(),
                    target_session_id: Some(SessionId::new("session-alpha")),
                    interval_ms: Some(60_000),
                    next_due_at: Some("2026-06-20T06:00:00Z".to_string()),
                    payload_json: json!({"reason": "conformance"}),
                    status: ScheduledJobStatus::Active,
                    created_at: "2026-06-20T05:59:00Z".to_string(),
                    updated_at: "2026-06-20T05:59:00Z".to_string(),
                    paused_at: None,
                })
                .unwrap();

            let due = store
                .query_scheduled_jobs(&ScheduledJobQuery {
                    status: Some(ScheduledJobStatus::Active),
                    job_kind: Some("wake".to_string()),
                    due_at_or_before: Some("2026-06-20T06:00:00Z".to_string()),
                    page: Some(page()),
                })
                .unwrap();
            assert_eq!(due.len(), 1);

            let claimed = ScheduledRunRecord {
                run_id: RunId::new("scheduled:conformance-wake:1"),
                job_id: "conformance-wake".to_string(),
                job_kind: "wake".to_string(),
                target_session_id: Some(SessionId::new("session-alpha")),
                status: ScheduledRunStatus::Claimed,
                trigger: ScheduledRunTrigger::Due,
                scheduled_for: Some("2026-06-20T06:00:00Z".to_string()),
                claimed_at: "2026-06-20T06:00:01Z".to_string(),
                claim_deadline_at: "2026-06-20T06:01:00Z".to_string(),
                completed_at: None,
                error: None,
                output_json: json!({}),
                created_at: "2026-06-20T06:00:01Z".to_string(),
                updated_at: "2026-06-20T06:00:01Z".to_string(),
            };
            store
                .claim_scheduled_run(&claimed, Some(&"2026-06-20T06:05:00Z".to_string()))
                .unwrap();
            assert_eq!(
                store
                    .load_scheduled_job("conformance-wake")
                    .unwrap()
                    .unwrap()
                    .next_due_at,
                Some("2026-06-20T06:05:00Z".to_string())
            );
            store
                .complete_scheduled_run(
                    &RunId::new("scheduled:conformance-wake:1"),
                    ScheduledRunStatus::Completed,
                    &"2026-06-20T06:00:30Z".to_string(),
                    &json!({"woke": true}),
                    None,
                )
                .unwrap();
            assert_eq!(
                scheduled_runs(store, Some(ScheduledRunStatus::Completed)).len(),
                1
            );

            let stale = ScheduledRunRecord {
                run_id: RunId::new("scheduled:conformance-wake:2"),
                job_id: "conformance-wake".to_string(),
                job_kind: "wake".to_string(),
                target_session_id: Some(SessionId::new("session-alpha")),
                status: ScheduledRunStatus::Claimed,
                trigger: ScheduledRunTrigger::Manual,
                scheduled_for: None,
                claimed_at: "2026-06-20T06:01:00Z".to_string(),
                claim_deadline_at: "2026-06-20T06:02:00Z".to_string(),
                completed_at: None,
                error: None,
                output_json: json!({}),
                created_at: "2026-06-20T06:01:00Z".to_string(),
                updated_at: "2026-06-20T06:01:00Z".to_string(),
            };
            store.claim_scheduled_run(&stale, None).unwrap();
            let expired = store
                .expire_stale_scheduled_runs(
                    &"2026-06-20T06:02:01Z".to_string(),
                    &"2026-06-20T06:03:00Z".to_string(),
                )
                .unwrap();
            assert_eq!(expired.len(), 1);
            assert_eq!(
                expired[0].run_id,
                RunId::new("scheduled:conformance-wake:2")
            );
            assert_eq!(
                scheduled_runs(store, Some(ScheduledRunStatus::Expired))[0]
                    .error
                    .as_deref(),
                Some("claim deadline elapsed")
            );
        });
    }

    fn scheduled_runs(
        store: &CoordinationStore,
        status: Option<ScheduledRunStatus>,
    ) -> Vec<ScheduledRunRecord> {
        store
            .query_scheduled_runs(&ScheduledRunQuery {
                job_id: Some("conformance-wake".to_string()),
                status,
                trigger: None,
                target_session_id: None,
                stale_claim_deadline_before: None,
                page: Some(page()),
            })
            .unwrap()
    }

    fn runtime_counters_contract<B: RepositoryConformanceBackend>(backend: &B) {
        backend.with_store("runtime-counters", |store| {
            store
                .save_event(
                    1,
                    &CoreEvent::BrainWakeRequested {
                        session_id: SessionId::new("session-alpha"),
                    },
                )
                .unwrap();
            store
                .save_event(
                    2,
                    &CoreEvent::BrainActionsAccepted {
                        session_id: SessionId::new("session-alpha"),
                        count: 2,
                    },
                )
                .unwrap();
            store
                .save_event(
                    3,
                    &CoreEvent::AgentMessageRouted {
                        message: AgentMessage {
                            from: AgentId::new("agent-alpha"),
                            to: AgentId::new("agent-beta"),
                            body: "counter conformance message".to_string(),
                            correlation_id: None,
                            projection: None,
                        },
                    },
                )
                .unwrap();

            let runtime = store
                .runtime_summary(&RuntimeCounterScope::Runtime)
                .unwrap();
            let session = store
                .runtime_summary(&RuntimeCounterScope::Session(SessionId::new(
                    "session-alpha",
                )))
                .unwrap();
            let message_counter = store
                .query_runtime_counters(&RuntimeCounterQuery {
                    scope: Some(RuntimeCounterScope::Runtime),
                    counter_name: Some(COUNTER_MESSAGES.to_string()),
                    page: Some(page()),
                })
                .unwrap();

            assert_eq!(runtime.wakes, 1);
            assert_eq!(runtime.brain_turns, 1);
            assert_eq!(runtime.messages, 1);
            assert_eq!(session.wakes, 1);
            assert_eq!(message_counter[0].value, 1);
        });
    }

    fn dense_profile_memory_revision_contract<B: RepositoryConformanceBackend>(backend: &B) {
        backend.with_store("profile-memory-revisions", |store| {
            let profile_id = ProfileId::new("profile-conformance");
            let target = ProfileMemoryTarget::Profile;
            let added = store
                .add_profile_memory(
                    &ProfileMemoryWrite {
                        profile_id: profile_id.clone(),
                        target: target.clone(),
                        key: "tone".to_string(),
                        content: "prefers direct conformance checks".to_string(),
                        metadata: json!({"source": "test"}),
                        now: "2026-06-20T05:00:00Z".to_string(),
                    },
                    &ProfileMemoryCaps::default(),
                )
                .unwrap();
            assert_eq!(added.revision, 1);

            let replaced = store
                .replace_profile_memory(
                    &ProfileMemoryReplace {
                        write: ProfileMemoryWrite {
                            profile_id: profile_id.clone(),
                            target: target.clone(),
                            key: "tone".to_string(),
                            content: "prefers backend-neutral repository checks".to_string(),
                            metadata: json!({"source": "replace"}),
                            now: "2026-06-20T05:01:00Z".to_string(),
                        },
                        expected_revision: 1,
                    },
                    &ProfileMemoryCaps::default(),
                )
                .unwrap();
            assert_eq!(replaced.revision, 2);
            assert!(store
                .replace_profile_memory(
                    &ProfileMemoryReplace {
                        write: replaced_write("profile-conformance", target.clone(), "tone"),
                        expected_revision: 1,
                    },
                    &ProfileMemoryCaps::default(),
                )
                .is_err());
            assert_eq!(
                store
                    .get_profile_memory(&profile_id, &target, "tone")
                    .unwrap()
                    .unwrap()
                    .content,
                "prefers backend-neutral repository checks"
            );
            assert_eq!(
                store
                    .list_profile_memory(&ProfileMemoryQuery {
                        profile_id,
                        target: Some(target),
                        page: Some(page()),
                    })
                    .unwrap()
                    .len(),
                1
            );
        });
    }

    fn runtime_search_contract<B: RepositoryConformanceBackend>(backend: &B) {
        backend.with_store("runtime-search", |store| {
            store
                .save_session_with_config(&sample_session_state(), &sample_session_config())
                .unwrap();
            store
                .save_event(
                    1,
                    &CoreEvent::AgentMessageRouted {
                        message: AgentMessage {
                            from: AgentId::new("agent-alpha"),
                            to: AgentId::new("agent-beta"),
                            body: "needle event search".to_string(),
                            correlation_id: Some("search-conformance".to_string()),
                            projection: None,
                        },
                    },
                )
                .unwrap();
            store
                .save_queued_message(&QueuedMessageRecord {
                    message_id: "queue-search-conformance".to_string(),
                    owner_session_id: Some(SessionId::new("session-alpha")),
                    owner_agent_id: AgentId::new("agent-alpha"),
                    message: AgentMessage {
                        from: AgentId::new("operator"),
                        to: AgentId::new("agent-alpha"),
                        body: "needle queue search".to_string(),
                        correlation_id: None,
                        projection: None,
                    },
                    source_sequence: Some(1),
                    enqueued_at: "2026-06-20T00:00:00Z".to_string(),
                    expires_at: "2026-06-20T00:05:00Z".to_string(),
                    ttl_ms: 300_000,
                    delivery_attempts: 0,
                    state: QueuedMessageState::Pending,
                    terminal_at: None,
                    state_reason: None,
                })
                .unwrap();

            let sessions = store
                .search_runtime(&RuntimeSearchFilter {
                    query: "tools".to_string(),
                    row_type: Some(RuntimeSearchRowType::Session),
                    session_id: Some(SessionId::new("session-alpha")),
                    agent_id: None,
                    instance_id: None,
                    task_id: None,
                    event_kind: None,
                    recorded_after: None,
                    recorded_before: None,
                    limit: Some(10),
                })
                .unwrap();
            let messages = store
                .search_runtime(&RuntimeSearchFilter {
                    query: "needle".to_string(),
                    row_type: Some(RuntimeSearchRowType::Message),
                    session_id: None,
                    agent_id: Some(AgentId::new("agent-beta")),
                    instance_id: None,
                    task_id: None,
                    event_kind: Some(CoreEventKind::AgentMessageRouted),
                    recorded_after: None,
                    recorded_before: None,
                    limit: Some(10),
                })
                .unwrap();
            let queued = store
                .search_runtime(&RuntimeSearchFilter {
                    query: "needle".to_string(),
                    row_type: Some(RuntimeSearchRowType::QueueMessage),
                    session_id: Some(SessionId::new("session-alpha")),
                    agent_id: Some(AgentId::new("agent-alpha")),
                    instance_id: None,
                    task_id: None,
                    event_kind: None,
                    recorded_after: None,
                    recorded_before: None,
                    limit: Some(10),
                })
                .unwrap();

            assert_eq!(sessions.len(), 1);
            assert_eq!(sessions[0].row_type, RuntimeSearchRowType::Session);
            assert_eq!(messages.len(), 1);
            assert_eq!(messages[0].sequence, Some(1));
            assert_eq!(queued.len(), 1);
            assert_eq!(queued[0].row_key, "queue-search-conformance");
        });
    }

    fn conversation_branch_message_contract<B: RepositoryConformanceBackend>(backend: &B) {
        backend.with_store("conversation-branch-message", |store| {
            let now = "2026-06-25T04:00:00Z".to_string();
            let session_id = SessionId::new("session-1");
            let root_branch = ConversationBranchId::new("branch-conformance-root");
            let slot_id = MessageSlotId::new("slot-conformance");
            let primary_variant_id = MessageVariantId::new("variant-conformance-primary");
            let root_message_id = MessageId::new("message-conformance-root");
            store
                .save_conversation_branch(&ConversationBranchWrite {
                    branch_id: root_branch.clone(),
                    session_id: session_id.clone(),
                    parent_branch_id: None,
                    parent_message_id: None,
                    origin_message_id: None,
                    head_message_id: Some(root_message_id.clone()),
                    label: Some("Root".to_string()),
                    metadata_json: json!({"kind": "conformance"}),
                    created_at: now.clone(),
                    updated_at: now.clone(),
                })
                .unwrap();
            store
                .save_message_slot(&MessageSlotWrite {
                    slot_id: slot_id.clone(),
                    session_id: session_id.clone(),
                    primary_variant_id: primary_variant_id.clone(),
                    active_variant_id: None,
                    metadata_json: json!({"origin": "conformance"}),
                    created_at: now.clone(),
                    updated_at: now.clone(),
                })
                .unwrap();
            let mut variant = variant_write(
                &slot_id,
                &primary_variant_id,
                MessageVariantSource::Primary,
                0,
                &root_message_id.0,
                "root conformance body",
            );
            variant.message.branch_id = Some(root_branch.clone());
            store.save_message_variant(&variant).unwrap();

            let branches = store
                .query_conversation_branches(&ConversationBranchQuery {
                    session_id: Some(session_id.clone()),
                    parent_branch_id: None,
                    page: Some(page()),
                })
                .unwrap();
            let slots = store
                .query_message_slots(&MessageSlotQuery {
                    session_id: Some(session_id.clone()),
                    include_alternates: false,
                    page: Some(page()),
                })
                .unwrap();
            let selected = store
                .select_active_conversation_branch(&SelectActiveBranchRequest {
                    session_id: session_id.clone(),
                    active_branch_id: Some(root_branch.clone()),
                    expected: ActiveBranchExpectation::None,
                    updated_at: "2026-06-25T04:01:00Z".to_string(),
                })
                .unwrap();
            let updated = store
                .update_conversation_branch_head(&UpdateBranchHeadRequest {
                    branch_id: root_branch.clone(),
                    head_message_id: Some(root_message_id.clone()),
                    expected: BranchHeadExpectation::Message(root_message_id.clone()),
                    updated_at: "2026-06-25T04:02:00Z".to_string(),
                })
                .unwrap();
            let jump = store
                .resolve_conversation_jump(&ConversationJumpRequest {
                    session_id,
                    target: ConversationJumpTarget::Message {
                        message_id: root_message_id.clone(),
                    },
                })
                .unwrap();

            assert_eq!(branches.len(), 1);
            assert_eq!(slots.len(), 1);
            assert_eq!(slots[0].primary.message.body, "root conformance body");
            assert!(selected.conflict.is_none());
            assert_eq!(selected.state.active_branch_id, Some(root_branch.clone()));
            assert!(updated.conflict.is_none());
            assert_eq!(jump.branch_id, Some(root_branch));
        });
    }

    fn provider_wire_state_expiry_contract<B: RepositoryConformanceBackend>(backend: &B) {
        backend.with_store("provider-wire-state-expiry", |store| {
            let key = sample_provider_wire_state_key();
            store
                .save_provider_wire_state(&sample_provider_wire_state_write(
                    ProviderWireStateWriteFixture {
                        key: key.clone(),
                        profile_fingerprint: "profile:v1",
                        provider_fingerprint: "provider:v1",
                        payload_version: "responses:v1",
                        payload_json: json!({"response_id": "resp_conformance"}),
                        now: "2026-06-20T00:00:00Z",
                        expires_at: Some("2026-06-20T00:00:05Z"),
                        last_wake_id: Some("wake-conformance"),
                    },
                ))
                .unwrap();
            let current = store
                .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
                    key: key.clone(),
                    profile_fingerprint: "profile:v1".to_string(),
                    provider_fingerprint: "provider:v1".to_string(),
                    now: "2026-06-20T00:00:04Z".to_string(),
                })
                .unwrap();
            assert!(current.record.unwrap().is_current());

            let expired_lookup = store
                .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
                    key: key.clone(),
                    profile_fingerprint: "profile:v1".to_string(),
                    provider_fingerprint: "provider:v1".to_string(),
                    now: "2026-06-20T00:00:06Z".to_string(),
                })
                .unwrap();
            assert!(expired_lookup.record.is_none());
            assert_eq!(
                expired_lookup.absence_reason,
                Some(ProviderStateAbsenceReason::Expired)
            );

            store
                .save_provider_wire_state(&sample_provider_wire_state_write(
                    ProviderWireStateWriteFixture {
                        key: key.clone(),
                        profile_fingerprint: "profile:v1",
                        provider_fingerprint: "provider:v1",
                        payload_version: "responses:v2",
                        payload_json: json!({"response_id": "resp_maintenance"}),
                        now: "2026-06-20T00:00:07Z",
                        expires_at: Some("2026-06-20T00:00:08Z"),
                        last_wake_id: Some("wake-maintenance"),
                    },
                ))
                .unwrap();
            let expired = store
                .expire_provider_wire_states_at(&"2026-06-20T00:00:09Z".to_string())
                .unwrap();
            assert_eq!(expired.len(), 1);
            assert_eq!(
                expired[0].invalidation_reason,
                Some(ProviderWireStateInvalidationReason::Expired)
            );
            assert!(store
                .expire_provider_wire_states_at(&"2026-06-20T00:00:10Z".to_string())
                .unwrap()
                .is_empty());
        });
    }

    fn model_provider_secret_envelope_contract<B: RepositoryConformanceBackend>(backend: &B) {
        backend.with_store("model-provider-secret-envelope", |store| {
            let api_key = store
                .upsert_model_provider(&model_provider_write(
                    "deepseek-flash",
                    ModelProviderProtocol::ChatCompletions,
                    "deepseek",
                    "deepseek-chat",
                    Some("sk-legacy-api-key"),
                ))
                .unwrap();
            assert_eq!(
                api_key.credential.kind,
                Some(ModelProviderCredentialKind::ApiKey)
            );
            let stored_api_key = store
                .get_model_provider_secret("deepseek-flash")
                .unwrap()
                .expect("stored API key secret");
            assert_ne!(stored_api_key, "sk-legacy-api-key");
            let api_key_envelope =
                ModelProviderSecretEnvelope::from_storage_text(&stored_api_key).unwrap();
            assert_eq!(api_key_envelope.api_key_value(), Some("sk-legacy-api-key"));

            let oauth_secret = ModelProviderSecretEnvelope::OpenAiOauth {
                version: MODEL_PROVIDER_SECRET_ENVELOPE_VERSION,
                issuer: "https://auth.openai.com".to_string(),
                client_id: "app-client".to_string(),
                id_token: "id.jwt.token".to_string(),
                access_token: "access.jwt.token".to_string(),
                refresh_token: "refresh-token".to_string(),
                exchanged_api_token: Some("exchanged-token".to_string()),
                last_refresh_at: Some("2026-07-02T00:00:00Z".to_string()),
                account_id: Some("account-1".to_string()),
                email: Some("agent@example.test".to_string()),
                plan_type: Some("pro".to_string()),
                is_fedramp_account: false,
                access_token_expires_at: Some("2026-07-02T01:00:00Z".to_string()),
            }
            .to_storage_text()
            .unwrap();
            let oauth = store
                .upsert_model_provider(&model_provider_write(
                    "gpt-oauth",
                    ModelProviderProtocol::Responses,
                    "openai",
                    "gpt-5",
                    Some(&oauth_secret),
                ))
                .unwrap();
            assert_eq!(
                oauth.credential.kind,
                Some(ModelProviderCredentialKind::OpenAiOauth)
            );
            let stored_oauth = store
                .get_model_provider_secret("gpt-oauth")
                .unwrap()
                .expect("stored OAuth secret");
            let oauth_envelope =
                ModelProviderSecretEnvelope::from_storage_text(&stored_oauth).unwrap();
            assert_eq!(
                oauth_envelope.kind(),
                ModelProviderCredentialKind::OpenAiOauth
            );
            assert!(!serde_json::to_string(&oauth.credential)
                .unwrap()
                .contains("refresh-token"));
        });
    }

    fn model_provider_prompt_caching_contract<B: RepositoryConformanceBackend>(backend: &B) {
        backend.with_store("model-provider-prompt-caching", |store| {
            let mut write = model_provider_write(
                "haiku-cache",
                ModelProviderProtocol::ChatCompletions,
                "openrouter",
                "anthropic/claude-haiku-4.5",
                None,
            );
            write.prompt_caching = ChatCompletionsPromptCachingPolicy::Automatic5m;
            let stored = store.upsert_model_provider(&write).unwrap();
            assert_eq!(
                stored.prompt_caching,
                ChatCompletionsPromptCachingPolicy::Automatic5m
            );

            write.alias = "wrong-provider".to_string();
            write.provider_kind = "anthropic".to_string();
            assert!(store.upsert_model_provider(&write).is_err());

            write.alias = "wrong-model".to_string();
            write.provider_kind = "openrouter".to_string();
            write.model_id = "openai/gpt-4.1-mini".to_string();
            assert!(store.upsert_model_provider(&write).is_err());

            write.alias = "wrong-protocol".to_string();
            write.model_id = "anthropic/claude-haiku-4.5".to_string();
            write.protocol = ModelProviderProtocol::Responses;
            assert!(store.upsert_model_provider(&write).is_err());
        });
    }
}

#[test]
fn sqlite_small_roleplay_deployment_storage_proof() {
    let data_dir = temp_data_dir("small-roleplay-storage");
    let store = CoordinationStore::open(&data_dir).unwrap();
    let session_id = SessionId::new("session-alpha");
    let profile_id = ProfileId::new("full-profile");
    let now = "2026-06-26T00:00:00Z".to_string();

    store
        .create_profile_registry_record(&profile_registry_write("full-profile"))
        .unwrap();
    store
        .save_session_with_config(&sample_session_state(), &sample_session_config())
        .unwrap();

    let branch_id = ConversationBranchId::new("branch-roleplay-root");
    let root_message_id = MessageId::new("message-roleplay-root");
    store
        .save_conversation_branch(&ConversationBranchWrite {
            branch_id: branch_id.clone(),
            session_id: session_id.clone(),
            parent_branch_id: None,
            parent_message_id: None,
            origin_message_id: None,
            head_message_id: Some(root_message_id.clone()),
            label: Some("Roleplay Root".to_string()),
            metadata_json: json!({"deployment": "small_sqlite"}),
            created_at: now.clone(),
            updated_at: now.clone(),
        })
        .unwrap();

    let slot_id = MessageSlotId::new("slot-roleplay-root");
    let variant_id = MessageVariantId::new("variant-roleplay-primary");
    store
        .save_message_slot(&MessageSlotWrite {
            slot_id: slot_id.clone(),
            session_id: session_id.clone(),
            primary_variant_id: variant_id.clone(),
            active_variant_id: None,
            metadata_json: json!({"kind": "roleplay_turn"}),
            created_at: now.clone(),
            updated_at: now.clone(),
        })
        .unwrap();
    let mut variant = variant_write(
        &slot_id,
        &variant_id,
        MessageVariantSource::Primary,
        0,
        &root_message_id.0,
        "The moonlit tavern keeps a private lore ledger.",
    );
    variant.message.session_id = session_id.clone();
    variant.message.branch_id = Some(branch_id.clone());
    store.save_message_variant(&variant).unwrap();

    store
        .add_profile_memory(
            &ProfileMemoryWrite {
                profile_id: profile_id.clone(),
                target: ProfileMemoryTarget::User("player-1".to_string()),
                key: "tone".to_string(),
                content: "prefers slow-burn mystery with grounded sensory detail".to_string(),
                metadata: json!({"source": "roleplay_smoke"}),
                now: "2026-06-26T00:01:00Z".to_string(),
            },
            &ProfileMemoryCaps::default(),
        )
        .unwrap();

    store
        .save_event(
            1,
            &CoreEvent::AgentMessageRouted {
                message: AgentMessage {
                    from: AgentId::new("player-1"),
                    to: AgentId::new("agent-alpha"),
                    body: "roleplay search needle: ask about the tavern ledger".to_string(),
                    correlation_id: Some("roleplay-search".to_string()),
                    projection: None,
                },
            },
        )
        .unwrap();

    store
        .save_provider_wire_state(&sample_provider_wire_state_write(
            ProviderWireStateWriteFixture {
                key: sample_provider_wire_state_key(),
                profile_fingerprint: "profile:roleplay:v1",
                provider_fingerprint: "provider:gpt:v1",
                payload_version: "responses:v1",
                payload_json: json!({"response_id": "resp_roleplay_root"}),
                now: "2026-06-26T00:02:00Z",
                expires_at: Some("2026-06-26T06:00:00Z"),
                last_wake_id: Some("wake-roleplay"),
            },
        ))
        .unwrap();

    store
        .upsert_scheduled_job(&ScheduledJobRecord {
            job_id: "roleplay-maintenance".to_string(),
            job_kind: "maintenance".to_string(),
            target_session_id: Some(session_id.clone()),
            interval_ms: Some(300_000),
            next_due_at: Some("2026-06-26T00:05:00Z".to_string()),
            payload_json: json!({"mode": "small_sqlite"}),
            status: ScheduledJobStatus::Active,
            created_at: now.clone(),
            updated_at: now.clone(),
            paused_at: None,
        })
        .unwrap();

    let sessions = store.load_sessions().unwrap();
    let branches = store
        .query_conversation_branches(&ConversationBranchQuery {
            session_id: Some(session_id.clone()),
            parent_branch_id: None,
            page: None,
        })
        .unwrap();
    let slots = store
        .query_message_slots(&MessageSlotQuery {
            session_id: Some(session_id.clone()),
            include_alternates: false,
            page: None,
        })
        .unwrap();
    let memories = store
        .list_profile_memory(&ProfileMemoryQuery {
            profile_id,
            target: Some(ProfileMemoryTarget::User("player-1".to_string())),
            page: None,
        })
        .unwrap();
    let search = store
        .search_runtime(&RuntimeSearchFilter {
            query: "tavern".to_string(),
            row_type: Some(RuntimeSearchRowType::Message),
            session_id: None,
            agent_id: Some(AgentId::new("agent-alpha")),
            instance_id: None,
            task_id: None,
            event_kind: Some(CoreEventKind::AgentMessageRouted),
            recorded_after: None,
            recorded_before: None,
            limit: Some(10),
        })
        .unwrap();
    let provider = store
        .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
            key: sample_provider_wire_state_key(),
            profile_fingerprint: "profile:roleplay:v1".to_string(),
            provider_fingerprint: "provider:gpt:v1".to_string(),
            now: "2026-06-26T00:03:00Z".to_string(),
        })
        .unwrap();
    let scheduled = store
        .query_scheduled_jobs(&ScheduledJobQuery {
            status: Some(ScheduledJobStatus::Active),
            job_kind: Some("maintenance".to_string()),
            due_at_or_before: Some("2026-06-26T00:05:00Z".to_string()),
            page: None,
        })
        .unwrap();
    let before_maintenance = store.storage_diagnostics().unwrap();
    let maintenance = store
        .run_maintenance(&RuntimeMaintenancePolicy {
            run_wal_checkpoint: true,
            run_optimize: true,
            ..RuntimeMaintenancePolicy::default()
        })
        .unwrap();
    let after_maintenance = store.storage_diagnostics().unwrap();

    assert_eq!(sessions.len(), 1);
    assert_eq!(branches.len(), 1);
    assert_eq!(slots[0].primary.message.body, variant.message.body);
    assert_eq!(memories.len(), 1);
    assert_eq!(search.len(), 1);
    assert!(provider.record.unwrap().is_current());
    assert_eq!(scheduled.len(), 1);
    assert_eq!(before_maintenance.backend, "sqlite");
    assert!(before_maintenance.search_healthy);
    assert!(before_maintenance
        .capabilities
        .iter()
        .any(|capability| capability.name == "maintenance_checkpoint" && capability.supported));
    assert!(before_maintenance
        .capabilities
        .iter()
        .any(
            |capability| capability.name == "maintenance_vacuum_or_optimize"
                && capability.supported
        ));
    assert!(before_maintenance
        .repository_groups
        .iter()
        .any(|group| group.group_id == "conversations_attachments"));
    assert!(before_maintenance
        .repository_groups
        .iter()
        .any(|group| group.group_id == "profile_memory"));
    assert!(maintenance.wal_checkpoint_ran);
    assert!(maintenance.optimize_ran);
    assert!(after_maintenance.size.wal_bytes < 64 * 1024 * 1024);

    remove_temp_dir(&data_dir);
}

#[test]
fn roleplay_lore_layers_configs_entries_and_chat_links_round_trip() {
    let db_path = temp_db_path("roleplay-lore-layers");
    let store = CoordinationStore::open_file(&db_path).unwrap();

    let world_layer = store
        .create_lore_layer(&RoleplayLoreLayerWrite {
            layer_id: "layer-world".to_string(),
            profile_id: "profile-narrator".to_string(),
            name: "World Details".to_string(),
            description: Some("Durable world facts.".to_string()),
            purpose: RoleplayLoreLayerPurpose::World,
            write_policy: RoleplayLoreLayerWritePolicy::Manual,
            now: "2026-06-27T01:00:00Z".to_string(),
        })
        .unwrap();
    assert_eq!(world_layer.purpose, RoleplayLoreLayerPurpose::World);

    store
        .create_lore_layer(&RoleplayLoreLayerWrite {
            layer_id: "layer-story".to_string(),
            profile_id: "profile-narrator".to_string(),
            name: "Current Story".to_string(),
            description: None,
            purpose: RoleplayLoreLayerPurpose::Story,
            write_policy: RoleplayLoreLayerWritePolicy::AutoCapture,
            now: "2026-06-27T01:01:00Z".to_string(),
        })
        .unwrap();

    let updated = store
        .update_lore_layer(&RoleplayLoreLayerUpdate {
            layer_id: "layer-world".to_string(),
            name: Some("World Bible".to_string()),
            description: Some(None),
            purpose: Some(RoleplayLoreLayerPurpose::Mixed),
            write_policy: Some(RoleplayLoreLayerWritePolicy::Readonly),
            now: "2026-06-27T01:02:00Z".to_string(),
        })
        .unwrap();
    assert_eq!(updated.name, "World Bible");
    assert_eq!(updated.description, None);
    assert_eq!(updated.write_policy, RoleplayLoreLayerWritePolicy::Readonly);

    let config = store
        .set_lore_layer_config(&RoleplayLoreLayerConfigWrite {
            config_id: "config-world".to_string(),
            layer_id: "layer-world".to_string(),
            fts_weight: 1.25,
            subject_weight: 1.0,
            canon_weight: 0.75,
            tag_boost_weight: 0.5,
            recency_weight: 0.1,
            default_token_budget: 3200,
            constant_token_reserve: 400,
            min_relevance_score: 0.25,
            max_constants: 7,
            now: "2026-06-27T01:03:00Z".to_string(),
        })
        .unwrap();
    assert_eq!(config.max_constants, 7);
    assert_eq!(
        store
            .get_lore_layer_config("layer-world")
            .unwrap()
            .unwrap()
            .default_token_budget,
        3200
    );

    store
        .add_roleplay_lore_record(&roleplay_lore_write(
            "lore-tide-calendar",
            "world-moonlit",
            Some("entity-clockmaker"),
            "Tide Calendar",
            "The tide calendar opens the moon gate.",
            "2026-06-27T01:04:00Z",
        ))
        .unwrap();
    store
        .add_roleplay_lore_record(&roleplay_lore_write(
            "lore-brass-needle",
            "world-moonlit",
            Some("entity-clockmaker"),
            "Brass Needle",
            "The brass needle points to hidden observatory doors.",
            "2026-06-27T01:05:00Z",
        ))
        .unwrap();

    store
        .add_entry_to_layer(&RoleplayLoreLayerEntryLink {
            layer_id: "layer-world".to_string(),
            record_id: "lore-tide-calendar".to_string(),
            is_constant: false,
            priority: 10,
            added_at: "2026-06-27T01:06:00Z".to_string(),
        })
        .unwrap();
    store
        .add_entry_to_layer(&RoleplayLoreLayerEntryLink {
            layer_id: "layer-world".to_string(),
            record_id: "lore-brass-needle".to_string(),
            is_constant: true,
            priority: 0,
            added_at: "2026-06-27T01:07:00Z".to_string(),
        })
        .unwrap();

    let entries = store.list_entries_by_layer("layer-world").unwrap();
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.record_id.as_str())
            .collect::<Vec<_>>(),
        vec!["lore-brass-needle", "lore-tide-calendar"]
    );
    assert!(entries[0].is_constant);
    store
        .set_entry_constant("layer-world", "lore-tide-calendar", true)
        .unwrap();
    assert!(store
        .list_entries_by_layer("layer-world")
        .unwrap()
        .iter()
        .any(|entry| entry.record_id == "lore-tide-calendar" && entry.is_constant));
    store
        .remove_entry_from_layer("layer-world", "lore-brass-needle")
        .unwrap();
    assert_eq!(store.list_entries_by_layer("layer-world").unwrap().len(), 1);

    let mut captured_write = roleplay_lore_write(
        "lore-captured-orchard",
        "world-moonlit",
        Some("entity-clockmaker"),
        "Silver Orchard",
        "The silver orchard blooms after the clockmaker sings.",
        "2026-06-27T01:07:30Z",
    );
    captured_write.source = MemoryProposalSource::CaptureProducer;
    let captured = store
        .capture_lore_fact(&RoleplayLoreFactCapture {
            layer_id: "layer-story".to_string(),
            write: captured_write,
            is_constant: false,
            priority: 4,
            capture_reason: Some("observed in chat turn".to_string()),
        })
        .unwrap();
    assert_eq!(captured.layer_id, "layer-story");
    assert_eq!(captured.record.record_id, "lore-captured-orchard");
    assert_eq!(
        captured.record.source,
        MemoryProposalSource::CaptureProducer
    );
    assert_eq!(
        store
            .roleplay_lore_provenance_events("lore-captured-orchard")
            .unwrap()[0]
            .note
            .as_deref(),
        Some("observed in chat turn")
    );
    let mut invalid_capture = roleplay_lore_write(
        "lore-invalid-capture-target",
        "world-moonlit",
        None,
        "Invalid Capture",
        "This should not enter a manual layer.",
        "2026-06-27T01:07:31Z",
    );
    invalid_capture.source = MemoryProposalSource::CaptureProducer;
    assert!(store
        .capture_lore_fact(&RoleplayLoreFactCapture {
            layer_id: "layer-world".to_string(),
            write: invalid_capture,
            is_constant: false,
            priority: 0,
            capture_reason: None,
        })
        .is_err());

    assert!(store
        .promote_lore_entry(&RoleplayLoreEntryPromotion {
            source_layer_id: "layer-story".to_string(),
            source_record_id: "lore-captured-orchard".to_string(),
            target_layer_id: "layer-world".to_string(),
            new_record_id: "lore-promoted-orchard".to_string(),
            is_constant: false,
            priority: 2,
            now: "2026-06-27T01:07:40Z".to_string(),
        })
        .is_err());
    store
        .update_lore_layer(&RoleplayLoreLayerUpdate {
            layer_id: "layer-world".to_string(),
            name: None,
            description: None,
            purpose: None,
            write_policy: Some(RoleplayLoreLayerWritePolicy::Manual),
            now: "2026-06-27T01:07:41Z".to_string(),
        })
        .unwrap();
    let promoted = store
        .promote_lore_entry(&RoleplayLoreEntryPromotion {
            source_layer_id: "layer-story".to_string(),
            source_record_id: "lore-captured-orchard".to_string(),
            target_layer_id: "layer-world".to_string(),
            new_record_id: "lore-promoted-orchard".to_string(),
            is_constant: false,
            priority: 2,
            now: "2026-06-27T01:07:42Z".to_string(),
        })
        .unwrap();
    assert_eq!(promoted.layer_id, "layer-world");
    assert_eq!(promoted.record.record_id, "lore-promoted-orchard");
    assert_eq!(promoted.record.title, "Silver Orchard");
    assert_eq!(
        promoted.record.supersedes_record_id.as_deref(),
        Some("lore-captured-orchard")
    );
    let promoted_source = store
        .get_roleplay_lore_record("lore-captured-orchard")
        .unwrap()
        .unwrap();
    assert_eq!(promoted_source.status, RoleplayLoreRecordStatus::Superseded);
    assert_eq!(
        promoted_source.superseded_by_record_id.as_deref(),
        Some("lore-promoted-orchard")
    );
    assert_eq!(
        store
            .roleplay_lore_provenance_events("lore-promoted-orchard")
            .unwrap()[0]
            .note
            .as_deref(),
        Some("promoted from layer-story:lore-captured-orchard")
    );

    store
        .set_chat_layers(&RoleplayChatLayersWrite {
            chat_id: "chat-moonlit".to_string(),
            layers: vec![
                RoleplayChatLayerLink {
                    layer_id: "layer-story".to_string(),
                    priority: 0,
                    enabled: true,
                },
                RoleplayChatLayerLink {
                    layer_id: "layer-world".to_string(),
                    priority: 1,
                    enabled: true,
                },
            ],
            now: "2026-06-27T01:08:00Z".to_string(),
        })
        .unwrap();
    assert_eq!(
        store
            .get_chat_layers("chat-moonlit")
            .unwrap()
            .iter()
            .map(|layer| layer.layer_id.as_str())
            .collect::<Vec<_>>(),
        vec!["layer-story", "layer-world"]
    );
    store
        .toggle_chat_layer("chat-moonlit", "layer-world", false)
        .unwrap();
    assert!(
        !store
            .get_chat_layers("chat-moonlit")
            .unwrap()
            .iter()
            .find(|layer| layer.layer_id == "layer-world")
            .unwrap()
            .enabled
    );
    store
        .reorder_chat_layers(
            "chat-moonlit",
            &["layer-world".to_string(), "layer-story".to_string()],
        )
        .unwrap();
    store
        .toggle_chat_layer("chat-moonlit", "layer-world", true)
        .unwrap();
    assert_eq!(
        store
            .get_chat_layers("chat-moonlit")
            .unwrap()
            .iter()
            .map(|layer| layer.layer_id.as_str())
            .collect::<Vec<_>>(),
        vec!["layer-world", "layer-story"]
    );

    let recall = store
        .recall_lore(&LoreRecallQuery {
            chat_id: "chat-moonlit".to_string(),
            session_id: Some(SessionId::new("session-moonlit")),
            query_text: Some("moon gate tide".to_string()),
            active_subjects: vec!["entity-clockmaker".to_string()],
            excluded_subjects: Vec::new(),
            token_budget: Some(120),
            trace_id: Some("trace-moonlit-1".to_string()),
            record_trace: true,
            now: "2026-06-27T01:08:30Z".to_string(),
        })
        .unwrap();
    assert_eq!(recall.entries.len(), 1);
    assert_eq!(recall.entries[0].record.record_id, "lore-tide-calendar");
    assert!(recall.tokens_consumed > 0);
    assert_eq!(recall.trace.as_ref().unwrap().trace_id, "trace-moonlit-1");
    assert_eq!(
        store
            .count_rows("module_roleplay_lore_recall_traces")
            .unwrap(),
        1
    );
    let traces = store
        .list_recall_traces(&LoreRecallTraceQuery {
            session_id: Some(SessionId::new("session-moonlit")),
            chat_id: None,
            page: None,
        })
        .unwrap();
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].trace_id, "trace-moonlit-1");
    let trace = store.get_recall_trace("trace-moonlit-1").unwrap().unwrap();
    assert_eq!(trace.entries_returned, 1);
    assert_eq!(trace.tokens_consumed, recall.tokens_consumed);
    assert_eq!(trace.entry_decisions.len(), 1);
    assert!(trace.entry_decisions[0].included);
    assert_eq!(
        trace.entry_decisions[0].reason,
        LoreRecallTraceDecisionReason::Included
    );

    store
        .archive_lore_layer(&RoleplayLoreLayerArchive {
            layer_id: "layer-story".to_string(),
            now: "2026-06-27T01:09:00Z".to_string(),
        })
        .unwrap();
    assert_eq!(
        store
            .list_lore_layers_by_profile("profile-narrator")
            .unwrap()
            .iter()
            .map(|layer| layer.layer_id.as_str())
            .collect::<Vec<_>>(),
        vec!["layer-world"]
    );

    remove_temp_db(&db_path);
}

#[test]
fn sqlite_scale_fixture_reports_backend_move_pressure_without_resurrection() {
    let data_dir = temp_data_dir("scale-backend-pressure");
    let store = CoordinationStore::open(&data_dir).unwrap();
    let now = "2026-06-26T02:00:00Z".to_string();
    let mut sequence = 1_u64;

    for index in 0..36 {
        let session_id = SessionId::new(format!("scale-session-{index:02}"));
        let agent_id = AgentId::new(format!("scale-agent-{index:02}"));
        let profile_id = ProfileId::new(format!("scale-profile-{index:02}"));
        store
            .create_profile_registry_record(&profile_registry_write(&profile_id.0))
            .unwrap();
        let config = SessionConfig {
            session_id: session_id.clone(),
            agent_id: agent_id.clone(),
            profile_id: profile_id.clone(),
            kind: SessionKind::Full,
            delegation: None,
            resource_limits: sample_resource_limits(),
            tool_profile: sample_tool_profile(),
            history_window: None,
        };
        store
            .save_session_with_config(
                &SessionState {
                    handle: SessionHandle::new((index + 1) as u64),
                    session_id: session_id.clone(),
                    agent_id: agent_id.clone(),
                    profile_id: profile_id.clone(),
                    kind: SessionKind::Full,
                    delegation: None,
                    resource_limits: sample_resource_limits(),
                    tool_profile: sample_tool_profile(),
                    history_window: None,
                    inference_overrides: Default::default(),
                    status: SessionStatus::Idle,
                    brain_turn_count: 0,
                    created_at: now.clone(),
                    last_active_at: now.clone(),
                },
                &config,
            )
            .unwrap();
        for memory_index in 0..2 {
            store
                .add_profile_memory(
                    &ProfileMemoryWrite {
                        profile_id: profile_id.clone(),
                        target: ProfileMemoryTarget::User(format!("player-{memory_index}")),
                        key: format!("lore-seed-{memory_index}"),
                        content: format!(
                            "scale lore memory {index}-{memory_index}: persistent roleplay fact"
                        ),
                        metadata: json!({"fixture": "scale_backend_pressure"}),
                        now: now.clone(),
                    },
                    &ProfileMemoryCaps::default(),
                )
                .unwrap();
        }
    }

    let session_id = SessionId::new("scale-session-00");
    let branch_id = ConversationBranchId::new("scale-branch-root");
    store
        .save_conversation_branch(&ConversationBranchWrite {
            branch_id: branch_id.clone(),
            session_id: session_id.clone(),
            parent_branch_id: None,
            parent_message_id: None,
            origin_message_id: None,
            head_message_id: Some(MessageId::new("scale-message-069")),
            label: Some("Scale transcript root".to_string()),
            metadata_json: json!({"fixture": "scale_backend_pressure"}),
            created_at: now.clone(),
            updated_at: now.clone(),
        })
        .unwrap();
    for turn in 0..70 {
        let slot_id = MessageSlotId::new(format!("scale-slot-{turn:03}"));
        let variant_id = MessageVariantId::new(format!("scale-variant-{turn:03}"));
        let message_id = format!("scale-message-{turn:03}");
        store
            .save_message_slot(&MessageSlotWrite {
                slot_id: slot_id.clone(),
                session_id: session_id.clone(),
                primary_variant_id: variant_id.clone(),
                active_variant_id: None,
                metadata_json: json!({"turn": turn}),
                created_at: now.clone(),
                updated_at: now.clone(),
            })
            .unwrap();
        let mut variant = variant_write(
            &slot_id,
            &variant_id,
            MessageVariantSource::Primary,
            0,
            &message_id,
            &format!("scale transcript turn {turn}: roleplay lore and search pressure needle"),
        );
        variant.message.session_id = session_id.clone();
        variant.message.branch_id = Some(branch_id.clone());
        store.save_message_variant(&variant).unwrap();
        store
            .save_event(
                sequence,
                &CoreEvent::AgentMessageRouted {
                    message: AgentMessage {
                        from: AgentId::new(format!("scale-agent-{:02}", turn % 36)),
                        to: AgentId::new(format!("scale-agent-{:02}", (turn + 1) % 36)),
                        body: format!("scale search row {turn}: roleplay lore needle"),
                        correlation_id: Some("scale-pressure".to_string()),
                        projection: None,
                    },
                },
            )
            .unwrap();
        sequence += 1;
    }

    for index in 0..34 {
        store
            .upsert_scheduled_job(&ScheduledJobRecord {
                job_id: format!("scale-job-{index:02}"),
                job_kind: "maintenance".to_string(),
                target_session_id: Some(SessionId::new(format!("scale-session-{:02}", index % 36))),
                interval_ms: Some(300_000),
                next_due_at: Some("2026-06-26T02:05:00Z".to_string()),
                payload_json: json!({"fixture": "scale_backend_pressure", "index": index}),
                status: ScheduledJobStatus::Active,
                created_at: now.clone(),
                updated_at: now.clone(),
                paused_at: None,
            })
            .unwrap();
        store
            .save_provider_wire_state(&sample_provider_wire_state_write(
                ProviderWireStateWriteFixture {
                    key: ProviderWireStateKey {
                        session_id: SessionId::new(format!("scale-session-{:02}", index % 36)),
                        module_id: "openai-responses".to_string(),
                        strategy_id: format!("scale-wire-{index:02}"),
                    },
                    profile_fingerprint: "profile:scale:v1",
                    provider_fingerprint: "provider:gpt:v1",
                    payload_version: "responses:v1",
                    payload_json: json!({"response_id": format!("resp_scale_{index:02}")}),
                    now: "2026-06-26T02:01:00Z",
                    expires_at: Some("2026-06-27T02:01:00Z"),
                    last_wake_id: Some("wake-scale"),
                },
            ))
            .unwrap();
    }

    for index in 0..40 {
        let expires_at = if index < 5 {
            "2026-06-26T02:00:01Z"
        } else {
            "2026-06-26T03:00:00Z"
        };
        store
            .save_queued_message(&QueuedMessageRecord {
                message_id: format!("scale-queue-{index:02}"),
                owner_session_id: Some(session_id.clone()),
                owner_agent_id: AgentId::new("scale-agent-00"),
                message: AgentMessage {
                    from: AgentId::new("operator"),
                    to: AgentId::new("scale-agent-00"),
                    body: format!("scale queued message {index}"),
                    correlation_id: Some("scale-queue".to_string()),
                    projection: None,
                },
                source_sequence: Some(sequence + index as u64),
                enqueued_at: "2026-06-26T02:00:00Z".to_string(),
                expires_at: expires_at.to_string(),
                ttl_ms: if index < 5 { 1_000 } else { 3_600_000 },
                delivery_attempts: 0,
                state: QueuedMessageState::Pending,
                terminal_at: None,
                state_reason: None,
            })
            .unwrap();
    }

    let before_maintenance = store.storage_diagnostics().unwrap();
    assert!(before_maintenance.pressure);
    assert_active_storage_signal(&before_maintenance, "active_agent_count");
    assert_active_storage_signal(&before_maintenance, "conversation_transcript_growth");
    assert_active_storage_signal(&before_maintenance, "memory_lore_growth");
    assert_active_storage_signal(&before_maintenance, "runtime_search_growth");
    assert_active_storage_signal(&before_maintenance, "queued_message_retention");
    assert_active_storage_signal(&before_maintenance, "scheduler_row_growth");
    assert_active_storage_signal(&before_maintenance, "provider_wire_state_growth");
    assert_inactive_storage_signal(&before_maintenance, "single_service_writer_assumption");

    let report = store
        .run_maintenance(&RuntimeMaintenancePolicy {
            expire_queued_messages_at: Some("2026-06-26T02:00:02Z".to_string()),
            purge_terminal_queued_messages_before: Some("2026-06-26T02:00:03Z".to_string()),
            run_wal_checkpoint: true,
            run_optimize: true,
            ..RuntimeMaintenancePolicy::default()
        })
        .unwrap();
    assert_eq!(report.expired_queue_messages, 5);
    assert_eq!(report.purged_terminal_queue_messages, 5);
    assert_eq!(store.count_rows("queued_messages").unwrap(), 35);

    let pending = store
        .load_queued_messages(&QueuedMessageFilter {
            state: Some(QueuedMessageState::Pending),
            owner_session_id: Some(session_id.clone()),
            owner_agent_id: Some(AgentId::new("scale-agent-00")),
            limit: None,
        })
        .unwrap();
    assert_eq!(pending.len(), 35);
    assert!(pending.iter().all(|message| !matches!(
        message.message_id.as_str(),
        "scale-queue-00"
            | "scale-queue-01"
            | "scale-queue-02"
            | "scale-queue-03"
            | "scale-queue-04"
    )));
    assert_eq!(
        store
            .search_runtime(&RuntimeSearchFilter {
                query: "scale queued message 0".to_string(),
                row_type: Some(RuntimeSearchRowType::QueueMessage),
                session_id: Some(session_id),
                agent_id: Some(AgentId::new("scale-agent-00")),
                instance_id: None,
                task_id: None,
                event_kind: None,
                recorded_after: None,
                recorded_before: None,
                limit: Some(10),
            })
            .unwrap()
            .len(),
        0
    );

    remove_temp_dir(&data_dir);
}

#[test]
fn roleplay_lore_fts_triggers_track_record_changes() {
    let db_path = temp_db_path("roleplay-lore-fts");
    let _store = CoordinationStore::open_file(&db_path).unwrap();
    let conn = Connection::open(&db_path).unwrap();
    conn.execute(
        "INSERT INTO module_roleplay_lore_records (
                record_id,
                world_id,
                entity_id,
                session_id,
                branch_id,
                shape_id,
                shape_version,
                canon_status,
                visibility,
                status,
                revision,
                title,
                body,
                content_json,
                evidence_refs_json,
                source,
                confidence,
                durability_rationale,
                supersedes_record_id,
                superseded_by_record_id,
                tombstoned_at,
                tombstone_reason,
                created_at,
                updated_at
            ) VALUES (
                'lore-observatory',
                'world-moonlit',
                'entity-clockmaker',
                NULL,
                NULL,
                'lore_entry',
                1,
                'canon',
                'public',
                'active',
                1,
                'Observatory Door',
                'The observatory door opens at eclipse tide.',
                '{\"tags\":[\"observatory\",\"eclipse\"]}',
                '[]',
                'test',
                0.9,
                'schema test',
                NULL,
                NULL,
                NULL,
                NULL,
                '2026-06-27T00:00:00Z',
                '2026-06-27T00:00:00Z'
            )",
        [],
    )
    .unwrap();
    assert_eq!(roleplay_lore_fts_matches(&conn, "observatory"), 1);

    conn.execute(
        "UPDATE module_roleplay_lore_records
             SET title = 'Moon Gate',
                 body = 'The moon gate opens only when the brass needle turns.',
                 content_json = '{\"tags\":[\"moon\",\"brass\"]}',
                 updated_at = '2026-06-27T00:01:00Z'
             WHERE record_id = 'lore-observatory'",
        [],
    )
    .unwrap();
    assert_eq!(roleplay_lore_fts_matches(&conn, "observatory"), 0);
    assert_eq!(roleplay_lore_fts_matches(&conn, "moon"), 1);

    conn.execute(
        "DELETE FROM module_roleplay_lore_records WHERE record_id = 'lore-observatory'",
        [],
    )
    .unwrap();
    assert_eq!(roleplay_lore_fts_matches(&conn, "moon"), 0);

    remove_temp_db(&db_path);
}

#[test]
fn module_schema_registry_tracks_fresh_install_and_existing_descriptor() {
    let db_path = temp_db_path("module-schema-fresh");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    let registry = ModuleSchemaRegistry::new(vec![simple_kv_schema_bundle(1).unwrap()]).unwrap();

    let installed = store
        .install_module_schema_registry(
            &registry,
            &[ModuleSchemaCapability::Transactions],
            &"2026-06-26T00:00:00Z".to_string(),
        )
        .unwrap();
    assert_eq!(installed.len(), 1);
    assert_eq!(installed[0].module_id.as_str(), "simple_kv");
    assert_eq!(installed[0].installed_version, 1);

    let second = store
        .install_module_schema_registry(
            &registry,
            &[ModuleSchemaCapability::Transactions],
            &"2026-06-26T00:01:00Z".to_string(),
        )
        .unwrap();
    assert_eq!(second, installed);
    let all_installed = store.installed_module_schemas().unwrap();
    assert_eq!(all_installed.len(), 3);
    assert!(all_installed
        .iter()
        .any(|record| record.module_id.as_str() == "roleplay"));
    assert!(all_installed
        .iter()
        .any(|record| record.module_id.as_str() == "curator"));

    remove_temp_db(&db_path);
}

#[test]
fn module_schema_registry_rejects_upgrade_without_migration_implementation() {
    let db_path = temp_db_path("module-schema-upgrade");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    let v1 = ModuleSchemaRegistry::new(vec![simple_kv_schema_bundle(1).unwrap()]).unwrap();
    let v2 = ModuleSchemaRegistry::new(vec![simple_kv_schema_bundle(2).unwrap()]).unwrap();

    store
        .install_module_schema_registry(
            &v1,
            &[ModuleSchemaCapability::Transactions],
            &"2026-06-26T00:00:00Z".to_string(),
        )
        .unwrap();
    let error = store
        .install_module_schema_registry(
            &v2,
            &[ModuleSchemaCapability::Transactions],
            &"2026-06-26T00:02:00Z".to_string(),
        )
        .unwrap_err();

    assert_eq!(error.kind, CoreErrorKind::PersistenceFailure);
    assert!(error.message.contains("no migration implementation"));

    remove_temp_db(&db_path);
}

#[test]
fn module_schema_registry_rejects_same_version_fingerprint_change() {
    let db_path = temp_db_path("module-schema-fingerprint");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    let v1 = ModuleSchemaRegistry::new(vec![simple_kv_schema_bundle(1).unwrap()]).unwrap();
    let mut changed_bundle = simple_kv_schema_bundle(1).unwrap();
    changed_bundle.migration_notes = vec!["same version but changed descriptor".to_string()];
    let changed = ModuleSchemaRegistry::new(vec![changed_bundle]).unwrap();

    store
        .install_module_schema_registry(
            &v1,
            &[ModuleSchemaCapability::Transactions],
            &"2026-06-26T00:00:00Z".to_string(),
        )
        .unwrap();
    let error = store
        .install_module_schema_registry(
            &changed,
            &[ModuleSchemaCapability::Transactions],
            &"2026-06-26T00:01:00Z".to_string(),
        )
        .unwrap_err();

    assert_eq!(error.kind, CoreErrorKind::ActionRejected);
    assert!(error.message.contains("fingerprint changed"));

    remove_temp_db(&db_path);
}

#[test]
fn module_schema_registry_rejects_missing_required_capability() {
    let db_path = temp_db_path("module-schema-capability");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    let registry = ModuleSchemaRegistry::new(vec![simple_kv_schema_bundle(1).unwrap()]).unwrap();

    let error = store
        .install_module_schema_registry(&registry, &[], &"2026-06-26T00:00:00Z".to_string())
        .unwrap_err();

    assert_eq!(error.kind, CoreErrorKind::InvalidInput);
    assert!(error
        .message
        .contains("requires unsupported storage capability"));

    remove_temp_db(&db_path);
}

#[test]
fn module_schema_registry_rejects_invalid_installed_state() {
    let db_path = temp_db_path("module-schema-invalid-state");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute(
            "INSERT INTO module_schema_versions (
                    module_id,
                    installed_version,
                    descriptor_fingerprint,
                    installed_at,
                    updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?4)",
            params!["old_module", 0_i64, "bad", "2026-06-26T00:00:00Z"],
        )
        .unwrap();
    }

    let error = store.installed_module_schemas().unwrap_err();
    assert_eq!(error.kind, CoreErrorKind::PersistenceFailure);
    assert!(error
        .message
        .contains("invalid installed module schema version"));

    remove_temp_db(&db_path);
}

#[test]
fn simple_kv_repository_round_trips_revisions_and_expiry() {
    let db_path = temp_db_path("simple-kv-repository");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    let scope = SimpleKvScope {
        scope_type: "profile".to_string(),
        scope_id: "rusty-crew-runner".to_string(),
    };

    let first = store
        .put_simple_kv(&SimpleKvWrite {
            scope: scope.clone(),
            key: "tone".to_string(),
            value_json: json!({"style": "steady"}),
            now: "2026-06-26T00:00:00Z".to_string(),
            expires_at: None,
        })
        .unwrap();
    assert_eq!(first.revision, 1);
    assert_eq!(first.value_json, json!({"style": "steady"}));

    let fetched = store
        .get_simple_kv(&scope, "tone", Some(&"2026-06-26T00:01:00Z".to_string()))
        .unwrap()
        .unwrap();
    assert_eq!(fetched, first);

    let second = store
        .put_simple_kv(&SimpleKvWrite {
            scope: scope.clone(),
            key: "tone".to_string(),
            value_json: json!({"style": "direct"}),
            now: "2026-06-26T00:02:00Z".to_string(),
            expires_at: Some("2026-06-26T01:00:00Z".to_string()),
        })
        .unwrap();
    assert_eq!(second.revision, 2);
    assert_eq!(second.created_at, first.created_at);
    assert_eq!(second.value_json, json!({"style": "direct"}));

    let stale = store
        .compare_and_swap_simple_kv(&SimpleKvCompareAndSwap {
            write: SimpleKvWrite {
                scope: scope.clone(),
                key: "tone".to_string(),
                value_json: json!({"style": "stale"}),
                now: "2026-06-26T00:03:00Z".to_string(),
                expires_at: None,
            },
            expected_revision: 1,
        })
        .unwrap_err();
    assert_eq!(stale.kind, CoreErrorKind::ActionRejected);

    let third = store
        .compare_and_swap_simple_kv(&SimpleKvCompareAndSwap {
            write: SimpleKvWrite {
                scope: scope.clone(),
                key: "tone".to_string(),
                value_json: json!({"style": "precise"}),
                now: "2026-06-26T00:04:00Z".to_string(),
                expires_at: Some("2026-06-26T00:05:00Z".to_string()),
            },
            expected_revision: 2,
        })
        .unwrap();
    assert_eq!(third.revision, 3);

    store
        .put_simple_kv(&SimpleKvWrite {
            scope: scope.clone(),
            key: "working_set".to_string(),
            value_json: json!(["a", "b"]),
            now: "2026-06-26T00:04:30Z".to_string(),
            expires_at: None,
        })
        .unwrap();

    let visible = store
        .list_simple_kv(&SimpleKvQuery {
            scope: scope.clone(),
            key_prefix: None,
            include_expired: false,
            expired_only: false,
            now: Some("2026-06-26T00:04:45Z".to_string()),
            page: None,
        })
        .unwrap();
    assert_eq!(
        visible
            .iter()
            .map(|record| record.key.as_str())
            .collect::<Vec<_>>(),
        vec!["tone", "working_set"]
    );
    let prefixed = store
        .list_simple_kv(&SimpleKvQuery {
            scope: scope.clone(),
            key_prefix: Some("work".to_string()),
            include_expired: false,
            expired_only: false,
            now: Some("2026-06-26T00:04:45Z".to_string()),
            page: None,
        })
        .unwrap();
    assert_eq!(prefixed.len(), 1);
    assert_eq!(prefixed[0].key, "working_set");

    assert!(store
        .get_simple_kv(&scope, "tone", Some(&"2026-06-26T00:05:01Z".to_string()))
        .unwrap()
        .is_none());
    let with_expired = store
        .list_simple_kv(&SimpleKvQuery {
            scope: scope.clone(),
            key_prefix: None,
            include_expired: true,
            expired_only: false,
            now: Some("2026-06-26T00:05:01Z".to_string()),
            page: None,
        })
        .unwrap();
    assert_eq!(with_expired.len(), 2);
    let expired_only = store
        .list_simple_kv(&SimpleKvQuery {
            scope: scope.clone(),
            key_prefix: None,
            include_expired: true,
            expired_only: true,
            now: Some("2026-06-26T00:05:01Z".to_string()),
            page: None,
        })
        .unwrap();
    assert_eq!(expired_only.len(), 1);
    assert_eq!(expired_only[0].key, "tone");

    assert_eq!(
        store
            .delete_simple_kv(&SimpleKvDelete {
                scope: scope.clone(),
                key: "working_set".to_string(),
                expected_revision: 1,
            })
            .unwrap()
            .key,
        "working_set"
    );
    assert_eq!(
        store
            .expire_simple_kv(&"2026-06-26T00:05:01Z".to_string())
            .unwrap(),
        1
    );
    assert!(store
        .list_simple_kv(&SimpleKvQuery {
            scope,
            key_prefix: None,
            include_expired: true,
            expired_only: false,
            now: None,
            page: None,
        })
        .unwrap()
        .is_empty());

    remove_temp_db(&db_path);
}

#[test]
fn storage_schema_diagnostics_project_installed_module_registry() {
    let db_path = temp_db_path("module-schema-diagnostics");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    let registry = ModuleSchemaRegistry::new(vec![simple_kv_schema_bundle(1).unwrap()]).unwrap();

    store
        .install_module_schema_registry(
            &registry,
            &[
                ModuleSchemaCapability::Transactions,
                ModuleSchemaCapability::JsonDocuments,
            ],
            &"2026-06-26T00:00:00Z".to_string(),
        )
        .unwrap();

    let diagnostics = store
        .storage_schema_for_registry(
            &registry,
            &[
                ModuleSchemaCapability::Transactions,
                ModuleSchemaCapability::JsonDocuments,
            ],
        )
        .unwrap();

    assert_eq!(diagnostics.modules.len(), 1);
    let module = &diagnostics.modules[0];
    assert_eq!(module.module_id, "simple_kv");
    assert_eq!(module.migration_status, "installed");
    assert_eq!(module.installed_version, Some(1));
    assert_eq!(module.logical_stores[0].store_name, "entries");
    assert_eq!(
        module.physical_tables[0].physical_table,
        "module_simple_kv_entries"
    );
    assert!(module.blocked_reasons.is_empty());
    assert!(module.degraded_reasons.is_empty());

    remove_temp_db(&db_path);
}

#[test]
fn legacy_import_metadata_maps_pi_crew_and_hermes_ids_without_runtime_coupling() {
    let db_path = temp_db_path("legacy-import-metadata");
    let store = CoordinationStore::open_file(&db_path).unwrap();

    store
        .save_import_batch(&RuntimeImportBatchRecord {
            import_batch_id: "import-pi-crew-001".to_string(),
            source_system: "pi-crew".to_string(),
            source_label: "pi-crew production snapshot".to_string(),
            source_snapshot_ref: Some("/backup/pi-crew/2026-06-20.sqlite3".to_string()),
            notes: Some("worker-pool history imported as provenance only".to_string()),
            imported_at: "2026-06-20T03:00:00Z".to_string(),
        })
        .unwrap();
    store
        .save_import_batch(&RuntimeImportBatchRecord {
            import_batch_id: "import-hermes-001".to_string(),
            source_system: "hermes".to_string(),
            source_label: "Hermes profile sqlite exports".to_string(),
            source_snapshot_ref: Some("/backup/hermes/profiles".to_string()),
            notes: Some("one sqlite source per profile".to_string()),
            imported_at: "2026-06-20T03:05:00Z".to_string(),
        })
        .unwrap();

    store
        .save_legacy_id_mapping(&LegacyIdMappingRecord {
            import_batch_id: "import-pi-crew-001".to_string(),
            source: SourceSystemReference {
                system: "pi-crew".to_string(),
                external_id: "worker-run:abc123".to_string(),
            },
            legacy_kind: RuntimeObjectKind::WorkerRun,
            rusty_kind: RuntimeObjectKind::WorkerRun,
            rusty_id: "run-rusty-001".to_string(),
            provenance: RuntimeImportProvenance {
                profile_id: Some(ProfileId::new("coder-profile")),
                session_id: Some(SessionId::new("session-rusty-001")),
                agent_id: Some(AgentId::new("agent-rusty")),
                externally_owned: false,
                notes: Some("pi-crew worker-pool run mapped to delegated run".to_string()),
            },
            created_at: "2026-06-20T03:10:00Z".to_string(),
        })
        .unwrap();
    store
        .save_legacy_id_mapping(&LegacyIdMappingRecord {
            import_batch_id: "import-hermes-001".to_string(),
            source: SourceSystemReference {
                system: "hermes".to_string(),
                external_id: "profile-db:/home/dev/.hermes/profiles/alpha.sqlite3".to_string(),
            },
            legacy_kind: RuntimeObjectKind::ExternalArtifact,
            rusty_kind: RuntimeObjectKind::Profile,
            rusty_id: "profile-alpha".to_string(),
            provenance: RuntimeImportProvenance {
                profile_id: Some(ProfileId::new("profile-alpha")),
                session_id: None,
                agent_id: None,
                externally_owned: true,
                notes: Some("Hermes source database remains external".to_string()),
            },
            created_at: "2026-06-20T03:11:00Z".to_string(),
        })
        .unwrap();

    assert_eq!(store.load_import_batches().unwrap().len(), 2);
    let pi_crew_mapping = store
        .query_legacy_id_mappings(&LegacyIdMappingQuery {
            source_system: Some("pi-crew".to_string()),
            legacy_kind: Some(RuntimeObjectKind::WorkerRun),
            ..LegacyIdMappingQuery::default()
        })
        .unwrap();
    assert_eq!(pi_crew_mapping.len(), 1);
    assert_eq!(pi_crew_mapping[0].rusty_id, "run-rusty-001");
    assert!(!pi_crew_mapping[0].provenance.externally_owned);

    let hermes_mapping = store
        .query_legacy_id_mappings(&LegacyIdMappingQuery {
            rusty_kind: Some(RuntimeObjectKind::Profile),
            rusty_id: Some("profile-alpha".to_string()),
            ..LegacyIdMappingQuery::default()
        })
        .unwrap();
    assert_eq!(hermes_mapping.len(), 1);
    assert_eq!(hermes_mapping[0].source.system, "hermes");
    assert!(hermes_mapping[0].provenance.externally_owned);
    assert_eq!(store.count_rows("runtime_import_batches").unwrap(), 2);
    assert_eq!(store.count_rows("legacy_id_mappings").unwrap(), 2);

    remove_temp_db(&db_path);
}

#[test]
fn logical_storage_import_dry_run_validates_capabilities_and_idempotency_without_writes() {
    let db_path = temp_db_path("logical-import-dry-run");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    let bundle = logical_import_bundle(vec![LogicalStorageRepositoryBundle {
        repository_id: "runtime_counters".to_string(),
        schema_version: 1,
        required_capabilities: vec!["transactions".to_string()],
        exported_count: 1,
        checksum: Some("sha256:runtime-counters".to_string()),
        records: vec![LogicalStorageRecord {
            stable_id: "runtime-counter:brain_turns".to_string(),
            record_version: 1,
            exported_at: "2026-06-26T10:00:00Z".to_string(),
            payload: LogicalStorageRecordPayload::TypedJson {
                object_kind: "runtime_counter".to_string(),
                payload_json: json!({
                    "scope_type": "runtime",
                    "counter_name": "brain_turns",
                    "value": 7
                }),
            },
        }],
    }]);
    let dry_run = LogicalStorageImportDryRun {
        import_batch_id: "dry-run-batch-1".to_string(),
        target_backend: "sqlite".to_string(),
        validation_time: "2026-06-26T10:01:00Z".to_string(),
        supported_capabilities: vec!["transactions".to_string()],
        supported_repositories: vec!["runtime_counters".to_string()],
    };

    let report = store
        .validate_logical_storage_import(&bundle, &dry_run)
        .unwrap();
    assert_eq!(report.record_count, 1);
    assert_eq!(report.accepted_records, 1);
    assert_eq!(report.unsupported_records, 0);
    assert_eq!(report.refused_records, 0);
    assert!(report.can_apply());
    assert_eq!(store.count_rows("runtime_import_batches").unwrap(), 0);

    store
        .save_import_batch(&RuntimeImportBatchRecord {
            import_batch_id: "dry-run-batch-1".to_string(),
            source_system: "logical-export".to_string(),
            source_label: "already imported".to_string(),
            source_snapshot_ref: Some("logical://bundle/export-1".to_string()),
            notes: None,
            imported_at: "2026-06-26T10:02:00Z".to_string(),
        })
        .unwrap();
    let idempotent = store
        .validate_logical_storage_import(&bundle, &dry_run)
        .unwrap();
    assert!(idempotent.already_imported);
    assert!(!idempotent.can_apply());
    assert!(idempotent
        .issues
        .iter()
        .any(|issue| issue.code == "import_batch_already_recorded"));

    remove_temp_db(&db_path);
}

#[test]
fn logical_storage_import_dry_run_refuses_queue_resurrection_risks() {
    let db_path = temp_db_path("logical-import-queue-safety");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    let bundle = logical_import_bundle(vec![LogicalStorageRepositoryBundle {
        repository_id: "queues_messages".to_string(),
        schema_version: 1,
        required_capabilities: vec!["transactions".to_string()],
        exported_count: 2,
        checksum: None,
        records: vec![
            LogicalStorageRecord {
                stable_id: "queue:fresh".to_string(),
                record_version: 1,
                exported_at: "2026-06-26T10:00:00Z".to_string(),
                payload: LogicalStorageRecordPayload::QueueMessage(Box::new(
                    logical_queue_message(
                        "queue-fresh",
                        QueuedMessageState::Pending,
                        "2026-06-26T10:05:00Z",
                        None,
                    ),
                )),
            },
            LogicalStorageRecord {
                stable_id: "queue:stale".to_string(),
                record_version: 1,
                exported_at: "2026-06-26T10:00:00Z".to_string(),
                payload: LogicalStorageRecordPayload::QueueMessage(Box::new(
                    logical_queue_message(
                        "queue-stale",
                        QueuedMessageState::Pending,
                        "2026-06-26T09:59:00Z",
                        None,
                    ),
                )),
            },
        ],
    }]);
    let report = store
        .validate_logical_storage_import(
            &bundle,
            &LogicalStorageImportDryRun {
                import_batch_id: "queue-dry-run".to_string(),
                target_backend: "postgres".to_string(),
                validation_time: "2026-06-26T10:01:00Z".to_string(),
                supported_capabilities: vec!["transactions".to_string()],
                supported_repositories: vec!["queues_messages".to_string()],
            },
        )
        .unwrap();

    assert_eq!(report.accepted_records, 1);
    assert_eq!(report.refused_records, 1);
    assert!(!report.can_apply());
    assert!(report.issues.iter().any(|issue| {
        issue.code == "queue_pending_expired_would_resurrect"
            && issue.record_id.as_deref() == Some("queue:stale")
    }));
    assert_eq!(store.count_rows("queued_messages").unwrap(), 0);

    remove_temp_db(&db_path);
}

#[test]
fn external_bindings_are_scoped_per_agent_without_secret_material() {
    let db_path = temp_db_path("external-bindings");
    let store = CoordinationStore::open_file(&db_path).unwrap();

    let base_provenance = ExternalBindingProvenance {
        source_system: Some("den-channels".to_string()),
        source_ref: Some("den-channel:crew-room".to_string()),
        externally_owned: true,
        notes: Some("provider secret remains in adapter config".to_string()),
    };
    let alpha_channel = ChannelBindingRecord {
        binding_id: "channel-alpha".to_string(),
        adapter_id: AdapterId::new("den-channels-main"),
        provider: "den_channels".to_string(),
        agent_id: AgentId::new("agent-alpha"),
        instance_id: Some(AgentInstanceId::new("instance-alpha")),
        session_id: Some(SessionId::new("session-alpha")),
        profile_id: ProfileId::new("prime-profile"),
        external_channel_id: "crew-room".to_string(),
        external_thread_id: Some("thread-42".to_string()),
        external_user_id: Some("den-user-alpha".to_string()),
        provider_subscription_id: Some("sub-alpha".to_string()),
        cursor: Some("cursor-alpha".to_string()),
        membership_state: Some("joined".to_string()),
        presence_state: Some("online".to_string()),
        status: ExternalBindingStatus::Active,
        degraded_reason: None,
        provenance: base_provenance.clone(),
        created_at: "2026-06-20T04:00:00Z".to_string(),
        updated_at: "2026-06-20T04:01:00Z".to_string(),
    };
    let beta_channel = ChannelBindingRecord {
        binding_id: "channel-beta".to_string(),
        agent_id: AgentId::new("agent-beta"),
        instance_id: Some(AgentInstanceId::new("instance-beta")),
        session_id: Some(SessionId::new("session-beta")),
        profile_id: ProfileId::new("review-profile"),
        provider_subscription_id: Some("sub-beta".to_string()),
        cursor: Some("cursor-beta".to_string()),
        presence_state: Some("idle".to_string()),
        updated_at: "2026-06-20T04:02:00Z".to_string(),
        ..alpha_channel.clone()
    };

    store.save_channel_binding(&alpha_channel).unwrap();
    store.save_channel_binding(&beta_channel).unwrap();

    let shared_channel = store
        .query_channel_bindings(&ChannelBindingQuery {
            provider: Some("den_channels".to_string()),
            external_channel_id: Some("crew-room".to_string()),
            ..ChannelBindingQuery::default()
        })
        .unwrap();
    let alpha_only = store
        .query_channel_bindings(&ChannelBindingQuery {
            agent_id: Some(AgentId::new("agent-alpha")),
            status: Some(ExternalBindingStatus::Active),
            ..ChannelBindingQuery::default()
        })
        .unwrap();

    assert_eq!(shared_channel.len(), 2);
    assert_eq!(alpha_only.len(), 1);
    assert_eq!(
        alpha_only[0].provider_subscription_id.as_deref(),
        Some("sub-alpha")
    );
    assert_eq!(alpha_only[0].cursor.as_deref(), Some("cursor-alpha"));
    assert_eq!(alpha_only[0].profile_id, ProfileId::new("prime-profile"));

    store
        .save_mcp_binding(&McpBindingRecord {
            binding_id: "mcp-alpha".to_string(),
            adapter_id: AdapterId::new("mcp-ts-main"),
            agent_id: AgentId::new("agent-alpha"),
            instance_id: Some(AgentInstanceId::new("instance-alpha")),
            session_id: Some(SessionId::new("session-alpha")),
            profile_id: ProfileId::new("prime-profile"),
            server_names: vec!["den".to_string(), "filesystem".to_string()],
            endpoint_ref: "config://mcp/alpha".to_string(),
            transport: "stdio".to_string(),
            tool_profile_key: "tool-profile-alpha".to_string(),
            discovered_tool_revision: Some("rev-alpha".to_string()),
            status: ExternalBindingStatus::Active,
            degraded_reason: None,
            diagnostics: McpBindingDiagnostics {
                last_error: None,
                last_checked_at: Some("2026-06-20T04:05:00Z".to_string()),
                notes: Some("no secret fields".to_string()),
            },
            created_at: "2026-06-20T04:00:00Z".to_string(),
            updated_at: "2026-06-20T04:05:00Z".to_string(),
        })
        .unwrap();
    store
        .save_mcp_binding(&McpBindingRecord {
            binding_id: "mcp-beta".to_string(),
            adapter_id: AdapterId::new("mcp-ts-main"),
            agent_id: AgentId::new("agent-beta"),
            instance_id: Some(AgentInstanceId::new("instance-beta")),
            session_id: Some(SessionId::new("session-beta")),
            profile_id: ProfileId::new("review-profile"),
            server_names: vec!["den".to_string()],
            endpoint_ref: "config://mcp/beta".to_string(),
            transport: "stdio".to_string(),
            tool_profile_key: "tool-profile-beta".to_string(),
            discovered_tool_revision: Some("rev-beta".to_string()),
            status: ExternalBindingStatus::Degraded,
            degraded_reason: Some("tool discovery stale".to_string()),
            diagnostics: McpBindingDiagnostics {
                last_error: Some("catalog revision mismatch".to_string()),
                last_checked_at: Some("2026-06-20T04:06:00Z".to_string()),
                notes: None,
            },
            created_at: "2026-06-20T04:00:00Z".to_string(),
            updated_at: "2026-06-20T04:06:00Z".to_string(),
        })
        .unwrap();

    let alpha_mcp = store
        .query_mcp_bindings(&McpBindingQuery {
            session_id: Some(SessionId::new("session-alpha")),
            ..McpBindingQuery::default()
        })
        .unwrap();
    let degraded = store
        .query_mcp_bindings(&McpBindingQuery {
            status: Some(ExternalBindingStatus::Degraded),
            ..McpBindingQuery::default()
        })
        .unwrap();

    assert_eq!(alpha_mcp.len(), 1);
    assert_eq!(
        alpha_mcp[0].server_names,
        vec!["den".to_string(), "filesystem".to_string()]
    );
    assert_eq!(alpha_mcp[0].endpoint_ref, "config://mcp/alpha");
    assert_eq!(alpha_mcp[0].tool_profile_key, "tool-profile-alpha");
    assert!(!alpha_mcp[0].endpoint_ref.contains("secret"));
    assert_eq!(degraded.len(), 1);
    assert_eq!(degraded[0].agent_id, AgentId::new("agent-beta"));
    assert_eq!(
        degraded[0].diagnostics.last_error.as_deref(),
        Some("catalog revision mismatch")
    );
    assert_eq!(store.count_rows("channel_bindings").unwrap(), 2);
    assert_eq!(store.count_rows("mcp_bindings").unwrap(), 2);

    remove_temp_db(&db_path);
}

#[test]
fn profile_registry_supports_lifecycle_revisions_and_asset_refs() {
    let db_path = temp_db_path("profile-registry");
    let store = CoordinationStore::open_file(&db_path).unwrap();

    let created = store
        .create_profile_registry_record(&profile_registry_write("runner-profile"))
        .unwrap();
    assert_eq!(created.profile_id, ProfileId::new("runner-profile"));
    assert_eq!(
        created.lifecycle_status,
        ProfileRegistryLifecycleStatus::Active
    );
    assert_eq!(created.revision, 1);
    assert_eq!(created.display_name.as_deref(), Some("Runner Profile"));
    assert_eq!(created.default_session_kind, Some(SessionKind::Full));
    assert_eq!(created.source_asset_refs.len(), 2);
    assert_eq!(created.source_asset_refs[0].asset_kind, "profile_yaml");
    assert_eq!(
        created.source_asset_refs[0].path,
        "/home/agents/rusty-crew/config/profiles/runner-profile/profile.yaml"
    );
    assert_eq!(created.derived_runtime_refs[0].ref_kind, "session");

    let loaded = store
        .get_profile_registry_record(&ProfileId::new("runner-profile"))
        .unwrap()
        .unwrap();
    assert_eq!(loaded.source_asset_refs, created.source_asset_refs);
    assert_eq!(loaded.import_export.imported_from.as_deref(), Some("file"));
    assert_eq!(
        loaded.prompt_soul_markdown.as_deref(),
        Some("You are a registry-backed runner.")
    );
    assert_eq!(
        loaded.prompt_memory_markdown.as_deref(),
        Some("Static deployment-safe memory.")
    );

    let duplicate = store
        .create_profile_registry_record(&profile_registry_write("runner-profile"))
        .unwrap_err();
    assert_eq!(duplicate.kind, CoreErrorKind::AlreadyExists);

    store
        .create_profile_registry_record(&ProfileRegistryWrite {
            lifecycle_status: ProfileRegistryLifecycleStatus::Paused,
            display_name: Some("Paused Profile".to_string()),
            now: "2026-06-26T02:00:00Z".to_string(),
            ..profile_registry_write("paused-profile")
        })
        .unwrap();

    let active = store
        .list_profile_registry_records(&ProfileRegistryQuery {
            lifecycle_status: Some(ProfileRegistryLifecycleStatus::Active),
            page: None,
        })
        .unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].profile_id, ProfileId::new("runner-profile"));

    let paused = store
        .update_profile_registry_lifecycle(&ProfileRegistryLifecycleUpdate {
            profile_id: ProfileId::new("runner-profile"),
            lifecycle_status: ProfileRegistryLifecycleStatus::Paused,
            expected_revision: created.revision,
            now: "2026-06-26T03:00:00Z".to_string(),
        })
        .unwrap();
    assert_eq!(
        paused.lifecycle_status,
        ProfileRegistryLifecycleStatus::Paused
    );
    assert_eq!(paused.revision, 2);
    assert_eq!(paused.created_at, "2026-06-26T01:00:00Z");
    assert_eq!(paused.updated_at, "2026-06-26T03:00:00Z");

    let stale = store
        .update_profile_registry_lifecycle(&ProfileRegistryLifecycleUpdate {
            profile_id: ProfileId::new("runner-profile"),
            lifecycle_status: ProfileRegistryLifecycleStatus::Archived,
            expected_revision: 1,
            now: "2026-06-26T04:00:00Z".to_string(),
        })
        .unwrap_err();
    assert_eq!(stale.kind, CoreErrorKind::ActionRejected);

    let invalid_id = store
        .create_profile_registry_record(&profile_registry_write("../bad"))
        .unwrap_err();
    assert_eq!(invalid_id.kind, CoreErrorKind::InvalidInput);

    assert_eq!(store.count_rows("profile_registry").unwrap(), 2);
    remove_temp_db(&db_path);
}

#[test]
fn profile_purge_removes_registry_sessions_and_profile_owned_readbacks() {
    let db_path = temp_db_path("profile-purge");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    let config = sample_session_config();
    let state = sample_session_state();

    store
        .create_profile_registry_record(&profile_registry_write("full-profile"))
        .unwrap();
    store
        .create_profile_registry_record(&profile_registry_write("other-profile"))
        .unwrap();
    store.save_session_with_config(&state, &config).unwrap();
    let external_runtime = rusty_crew_core_protocol::ExternalRuntimeRegistration {
        runtime_id: rusty_crew_core_protocol::ExternalRuntimeId::new("profile-purge-runtime"),
        kind: rusty_crew_core_protocol::ExternalRuntimeKind::CodexAppServer,
        endpoint: rusty_crew_core_protocol::ExternalEndpoint {
            transport: rusty_crew_core_protocol::ExternalEndpointTransport::UnixWebSocket,
            address: "/tmp/profile-purge-codex.sock".into(),
        },
        process_ownership: rusty_crew_core_protocol::ExternalProcessOwnership::Attached,
        codex_home_ref: None,
        observed_cli_version: None,
        consumed_contract_revision: None,
        compatibility_state:
            rusty_crew_core_protocol::ExternalRuntimeCompatibilityState::Unassessed,
        last_compatibility_probe: None,
        desired_state: rusty_crew_core_protocol::ExternalRuntimeDesiredState::Enabled,
        observed_state: rusty_crew_core_protocol::ExternalRuntimeObservedState::Disconnected,
        observed_reason_code: None,
        revision: 0,
        created_at: "2026-06-20T05:00:00Z".into(),
        updated_at: "2026-06-20T05:00:00Z".into(),
    };
    store
        .put_external_runtime_registration(&external_runtime, None)
        .unwrap();
    let external_binding = rusty_crew_core_protocol::ExternalAgentBinding {
        binding_id: rusty_crew_core_protocol::ExternalBindingId::new("profile-purge-binding"),
        runtime_id: external_runtime.runtime_id,
        session_id: Some(state.session_id.clone()),
        agent_id: Some(state.agent_id.clone()),
        profile_id: Some(state.profile_id.clone()),
        profile_revision: Some(1),
        profile_prompt_hash: Some("profile-prompt-hash".into()),
        profile_prompt_snapshot: Some("profile prompt".into()),
        dynamic_tool_catalog_fingerprint: None,
        message_delivery_policy:
            rusty_crew_core_protocol::ExternalMessageDeliveryPolicy::ImmediateSteer,
        purpose: rusty_crew_core_protocol::ExternalBindingPurpose::CrewAgent,
        native_thread_id: Some("profile-purge-thread".into()),
        cwd: Some("/home/dev/rusty-crew".into()),
        label: None,
        task_ref: None,
        effective_config_fingerprint: "profile-purge-config".into(),
        status: rusty_crew_core_protocol::ExternalBindingStatus::Archived,
        revision: 0,
        created_at: "2026-06-20T05:00:00Z".into(),
        updated_at: "2026-06-20T05:00:00Z".into(),
    };
    store
        .put_external_agent_binding(&external_binding, None)
        .unwrap();
    store
        .add_profile_memory(
            &ProfileMemoryWrite {
                profile_id: ProfileId::new("full-profile"),
                target: ProfileMemoryTarget::Profile,
                key: "style".to_string(),
                content: "delete me".to_string(),
                metadata: serde_json::json!({"source": "profile_purge_test"}),
                now: "2026-06-20T05:00:00Z".to_string(),
            },
            &ProfileMemoryCaps::default(),
        )
        .unwrap();
    store
        .save_event(
            1,
            &CoreEvent::SessionCreated {
                state: Box::new(state.clone()),
            },
        )
        .unwrap();
    store
        .save_event(
            2,
            &CoreEvent::AgentMessageRouted {
                message: AgentMessage {
                    from: AgentId::new("agent-alpha"),
                    to: AgentId::new("agent-beta"),
                    body: "profile purge message".to_string(),
                    correlation_id: Some("corr-profile-purge".to_string()),
                    projection: None,
                },
            },
        )
        .unwrap();
    let slot_id = MessageSlotId::new("slot-profile-purge");
    let variant_id = MessageVariantId::new("variant-profile-purge");
    store
        .save_message_slot(&MessageSlotWrite {
            slot_id: slot_id.clone(),
            session_id: state.session_id.clone(),
            primary_variant_id: variant_id.clone(),
            active_variant_id: None,
            metadata_json: json!({"test": "profile_purge"}),
            created_at: "2026-06-25T03:00:00Z".to_string(),
            updated_at: "2026-06-25T03:00:00Z".to_string(),
        })
        .unwrap();
    let mut variant = variant_write(
        &slot_id,
        &variant_id,
        MessageVariantSource::Primary,
        0,
        "message-profile-purge",
        "visible transcript residue",
    );
    variant.message.session_id = state.session_id.clone();
    store.save_message_variant(&variant).unwrap();

    assert_eq!(store.count_rows("sessions").unwrap(), 1);
    assert_eq!(store.count_rows("profile_registry").unwrap(), 2);
    assert_eq!(store.count_rows("message_slots").unwrap(), 1);
    assert_eq!(store.count_rows("profile_memories").unwrap(), 1);
    assert_eq!(store.count_rows("external_agent_bindings").unwrap(), 1);

    let report = store
        .purge_profile(&ProfileId::new("full-profile"))
        .unwrap();
    assert!(report.profile_registry_deleted);
    assert_eq!(report.profile_id, ProfileId::new("full-profile"));
    assert_eq!(report.session_ids, vec![SessionId::new("session-alpha")]);
    assert!(report.agent_ids.contains(&AgentId::new("agent-alpha")));
    assert!(report.rows_deleted > 0);

    assert!(store
        .get_profile_registry_record(&ProfileId::new("full-profile"))
        .unwrap()
        .is_none());
    assert!(store
        .get_profile_registry_record(&ProfileId::new("other-profile"))
        .unwrap()
        .is_some());
    assert_eq!(store.count_rows("sessions").unwrap(), 0);
    assert_eq!(store.count_rows("session_configs").unwrap(), 0);
    assert_eq!(store.count_rows("event_history").unwrap(), 0);
    assert_eq!(store.count_rows("event_session_index").unwrap(), 0);
    assert_eq!(store.count_rows("event_agent_index").unwrap(), 0);
    assert_eq!(store.count_rows("agent_messages").unwrap(), 0);
    assert_eq!(store.count_rows("message_slots").unwrap(), 0);
    assert_eq!(store.count_rows("message_variants").unwrap(), 0);
    assert_eq!(store.count_rows("messages").unwrap(), 0);
    assert_eq!(store.count_rows("message_blocks").unwrap(), 0);
    assert_eq!(store.count_rows("profile_memories").unwrap(), 0);
    assert_eq!(store.count_rows("external_agent_bindings").unwrap(), 0);
    assert!(report
        .table_counts
        .iter()
        .any(|count| { count.table == "external_agent_bindings" && count.rows_deleted == 1 }));
    assert_eq!(store.count_rows("profile_registry").unwrap(), 1);

    remove_temp_db(&db_path);
}

#[test]
fn profile_memory_supports_caps_revisions_and_profile_isolation() {
    let db_path = temp_db_path("profile-memory");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    let caps = ProfileMemoryCaps {
        max_records_per_profile: 2,
        max_key_bytes: 32,
        max_content_bytes: 64,
    };

    let added = store
        .add_profile_memory(
            &ProfileMemoryWrite {
                profile_id: ProfileId::new("prime-profile"),
                target: ProfileMemoryTarget::Profile,
                key: "style".to_string(),
                content: "prefers concise handoffs".to_string(),
                metadata: serde_json::json!({"source": "smoke"}),
                now: "2026-06-20T05:00:00Z".to_string(),
            },
            &caps,
        )
        .unwrap();
    assert_eq!(added.revision, 1);
    assert_eq!(added.target, ProfileMemoryTarget::Profile);

    let replaced = store
        .replace_profile_memory(
            &ProfileMemoryReplace {
                expected_revision: added.revision,
                write: ProfileMemoryWrite {
                    profile_id: ProfileId::new("prime-profile"),
                    target: ProfileMemoryTarget::Profile,
                    key: "style".to_string(),
                    content: "prefers concise handoffs with citations".to_string(),
                    metadata: serde_json::json!({"source": "replacement"}),
                    now: "2026-06-20T05:01:00Z".to_string(),
                },
            },
            &caps,
        )
        .unwrap();
    assert_eq!(replaced.revision, 2);
    assert_eq!(replaced.created_at, "2026-06-20T05:00:00Z");
    assert_eq!(replaced.updated_at, "2026-06-20T05:01:00Z");

    let stale_replace = store
        .replace_profile_memory(
            &ProfileMemoryReplace {
                expected_revision: 1,
                write: ProfileMemoryWrite {
                    now: "2026-06-20T05:02:00Z".to_string(),
                    ..replaced_write("prime-profile", ProfileMemoryTarget::Profile, "style")
                },
            },
            &caps,
        )
        .unwrap_err();
    assert_eq!(stale_replace.kind, CoreErrorKind::ActionRejected);

    store
        .add_profile_memory(
            &ProfileMemoryWrite {
                profile_id: ProfileId::new("prime-profile"),
                target: ProfileMemoryTarget::User("den-user-alpha".to_string()),
                key: "salutation".to_string(),
                content: "likes direct updates".to_string(),
                metadata: serde_json::json!({"scope": "user"}),
                now: "2026-06-20T05:03:00Z".to_string(),
            },
            &caps,
        )
        .unwrap();
    let cap_error = store
        .add_profile_memory(
            &ProfileMemoryWrite {
                profile_id: ProfileId::new("prime-profile"),
                target: ProfileMemoryTarget::Profile,
                key: "third".to_string(),
                content: "would exceed cap".to_string(),
                metadata: serde_json::json!({}),
                now: "2026-06-20T05:04:00Z".to_string(),
            },
            &caps,
        )
        .unwrap_err();
    assert_eq!(cap_error.kind, CoreErrorKind::ActionRejected);

    store
        .add_profile_memory(
            &ProfileMemoryWrite {
                profile_id: ProfileId::new("review-profile"),
                target: ProfileMemoryTarget::Profile,
                key: "style".to_string(),
                content: "prefers detailed risk notes".to_string(),
                metadata: serde_json::json!({}),
                now: "2026-06-20T05:05:00Z".to_string(),
            },
            &caps,
        )
        .unwrap();

    let prime_rows = store
        .list_profile_memory(&ProfileMemoryQuery {
            profile_id: ProfileId::new("prime-profile"),
            target: None,
            page: None,
        })
        .unwrap();
    assert_eq!(prime_rows.len(), 2);
    assert!(prime_rows
        .iter()
        .all(|row| row.profile_id == ProfileId::new("prime-profile")));

    let profile_style = store
        .get_profile_memory(
            &ProfileId::new("prime-profile"),
            &ProfileMemoryTarget::Profile,
            "style",
        )
        .unwrap()
        .unwrap();
    let user_style = store
        .get_profile_memory(
            &ProfileId::new("prime-profile"),
            &ProfileMemoryTarget::User("den-user-alpha".to_string()),
            "salutation",
        )
        .unwrap()
        .unwrap();
    assert_ne!(profile_style.target, user_style.target);

    let stale_delete = store
        .remove_profile_memory(&ProfileMemoryDelete {
            profile_id: ProfileId::new("prime-profile"),
            target: ProfileMemoryTarget::Profile,
            key: "style".to_string(),
            expected_revision: 1,
        })
        .unwrap_err();
    assert_eq!(stale_delete.kind, CoreErrorKind::ActionRejected);

    let removed = store
        .remove_profile_memory(&ProfileMemoryDelete {
            profile_id: ProfileId::new("prime-profile"),
            target: ProfileMemoryTarget::Profile,
            key: "style".to_string(),
            expected_revision: 2,
        })
        .unwrap();
    assert_eq!(removed.key, "style");
    assert!(store
        .get_profile_memory(
            &ProfileId::new("prime-profile"),
            &ProfileMemoryTarget::Profile,
            "style"
        )
        .unwrap()
        .is_none());

    let too_large = store
        .add_profile_memory(
            &ProfileMemoryWrite {
                profile_id: ProfileId::new("review-profile"),
                target: ProfileMemoryTarget::Profile,
                key: "large".to_string(),
                content: "x".repeat(65),
                metadata: serde_json::json!({}),
                now: "2026-06-20T05:06:00Z".to_string(),
            },
            &caps,
        )
        .unwrap_err();
    assert_eq!(too_large.kind, CoreErrorKind::ActionRejected);

    remove_temp_db(&db_path);
}

#[test]
fn session_memory_round_trips_and_isolates_by_session() {
    let db_path = temp_db_path("session-memory-basic");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    store.save_session(&sample_session_state()).unwrap();
    let mut other_session = sample_session_state();
    other_session.session_id = SessionId::new("session-beta");
    other_session.agent_id = AgentId::new("agent-beta");
    other_session.handle = SessionHandle::new(2);
    store.save_session(&other_session).unwrap();

    let added = store
        .add_session_memory_record(&session_fact_memory_write(
            "session-fact-one",
            &SessionId::new("session-alpha"),
            "2026-06-26T01:00:00Z",
        ))
        .unwrap();

    assert_eq!(added.revision, 1);
    assert_eq!(added.status, SessionMemoryRecordStatus::Active);
    assert_eq!(added.scope.scope_type, MemoryScopeType::Session);
    assert_eq!(added.shape.shape_id.as_str(), "session_fact");
    assert_eq!(
        added.content["content"],
        "The user prefers slow-burn pacing."
    );

    let alpha_rows = store
        .query_session_memory_records(&SessionMemoryQuery {
            session_id: Some(SessionId::new("session-alpha")),
            shape_id: Some("session_fact".to_string()),
            ..SessionMemoryQuery::default()
        })
        .unwrap();
    assert_eq!(alpha_rows, vec![added.clone()]);

    let beta_rows = store
        .query_session_memory_records(&SessionMemoryQuery {
            session_id: Some(SessionId::new("session-beta")),
            ..SessionMemoryQuery::default()
        })
        .unwrap();
    assert!(beta_rows.is_empty());

    let invalid_shape = store
        .add_session_memory_record(&SessionMemoryRecordWrite {
            shape: MemoryRecordShapeRef {
                shape_id: MemoryRecordShapeId::unchecked("transcript_message"),
                version: 1,
            },
            ..session_fact_memory_write(
                "session-fact-two",
                &SessionId::new("session-alpha"),
                "2026-06-26T01:01:00Z",
            )
        })
        .unwrap_err();
    assert_eq!(invalid_shape.kind, CoreErrorKind::InvalidInput);

    remove_temp_db(&db_path);
}

#[test]
fn session_memory_validates_branch_membership() {
    let db_path = temp_db_path("session-memory-branch");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    store.save_session(&sample_session_state()).unwrap();
    let mut other_session = sample_session_state();
    other_session.session_id = SessionId::new("session-beta");
    other_session.agent_id = AgentId::new("agent-beta");
    other_session.handle = SessionHandle::new(2);
    store.save_session(&other_session).unwrap();
    store
        .save_conversation_branch(&ConversationBranchWrite {
            branch_id: ConversationBranchId::new("branch-alpha"),
            session_id: SessionId::new("session-alpha"),
            parent_branch_id: None,
            parent_message_id: None,
            origin_message_id: Some(MessageId::new("message-root")),
            head_message_id: Some(MessageId::new("message-alpha")),
            label: Some("Branch alpha".to_string()),
            metadata_json: json!({"fixture": true}),
            created_at: "2026-06-26T01:00:00Z".to_string(),
            updated_at: "2026-06-26T01:00:00Z".to_string(),
        })
        .unwrap();

    let missing_branch_id = store
        .add_session_memory_record(&SessionMemoryRecordWrite {
            branch_id: None,
            ..branch_summary_memory_write(
                "branch-summary-missing",
                &SessionId::new("session-alpha"),
                &ConversationBranchId::new("branch-alpha"),
                "2026-06-26T01:01:00Z",
            )
        })
        .unwrap_err();
    assert_eq!(missing_branch_id.kind, CoreErrorKind::InvalidInput);

    let wrong_session = store
        .add_session_memory_record(&branch_summary_memory_write(
            "branch-summary-wrong-session",
            &SessionId::new("session-beta"),
            &ConversationBranchId::new("branch-alpha"),
            "2026-06-26T01:02:00Z",
        ))
        .unwrap_err();
    assert_eq!(wrong_session.kind, CoreErrorKind::InvalidInput);

    let added = store
        .add_session_memory_record(&branch_summary_memory_write(
            "branch-summary-one",
            &SessionId::new("session-alpha"),
            &ConversationBranchId::new("branch-alpha"),
            "2026-06-26T01:03:00Z",
        ))
        .unwrap();
    assert_eq!(
        added.branch_id,
        Some(ConversationBranchId::new("branch-alpha"))
    );

    let branch_rows = store
        .query_session_memory_records(&SessionMemoryQuery {
            branch_id: Some(ConversationBranchId::new("branch-alpha")),
            ..SessionMemoryQuery::default()
        })
        .unwrap();
    assert_eq!(branch_rows, vec![added]);

    remove_temp_db(&db_path);
}

#[test]
fn session_memory_replace_supersede_and_archive_enforce_revisions() {
    let db_path = temp_db_path("session-memory-revisions");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    store.save_session(&sample_session_state()).unwrap();

    let added = store
        .add_session_memory_record(&session_fact_memory_write(
            "session-fact-one",
            &SessionId::new("session-alpha"),
            "2026-06-26T01:00:00Z",
        ))
        .unwrap();
    let replaced = store
        .replace_session_memory_record(&SessionMemoryReplace {
            record_id: added.record_id.clone(),
            expected_revision: added.revision,
            content: session_fact_content(
                "session-fact-one",
                "The user prefers slow-burn pacing with explicit clues.",
                "2026-06-26T01:01:00Z",
            ),
            evidence_refs: session_memory_evidence("wake-replace"),
            source: MemoryProposalSource::Human,
            confidence: 0.95,
            durability_rationale: "Human correction refined the fact.".to_string(),
            now: "2026-06-26T01:01:00Z".to_string(),
        })
        .unwrap();
    assert_eq!(replaced.revision, 2);
    assert_eq!(
        replaced.content["content"],
        "The user prefers slow-burn pacing with explicit clues."
    );

    let stale_replace = store
        .replace_session_memory_record(&SessionMemoryReplace {
            expected_revision: 1,
            now: "2026-06-26T01:02:00Z".to_string(),
            ..replace_session_fact_input("session-fact-one")
        })
        .unwrap_err();
    assert_eq!(stale_replace.kind, CoreErrorKind::ActionRejected);

    let (old_record, new_record) = store
        .supersede_session_memory_record(&SessionMemorySupersede {
            record_id: "session-fact-one".to_string(),
            expected_revision: replaced.revision,
            replacement: SessionMemoryRecordWrite {
                supersedes_record_id: Some("session-fact-one".to_string()),
                content: session_fact_content(
                    "session-fact-two",
                    "The user prefers mystery pacing with explicit clue checkpoints.",
                    "2026-06-26T01:03:00Z",
                ),
                ..session_fact_memory_write(
                    "session-fact-two",
                    &SessionId::new("session-alpha"),
                    "2026-06-26T01:03:00Z",
                )
            },
        })
        .unwrap();
    assert_eq!(old_record.status, SessionMemoryRecordStatus::Superseded);
    assert_eq!(
        old_record.superseded_by_record_id.as_deref(),
        Some("session-fact-two")
    );
    assert_eq!(old_record.revision, 3);
    assert_eq!(new_record.status, SessionMemoryRecordStatus::Active);
    assert_eq!(
        new_record.supersedes_record_id.as_deref(),
        Some("session-fact-one")
    );

    let archived = store
        .archive_session_memory_record(&SessionMemoryArchive {
            record_id: "session-fact-two".to_string(),
            expected_revision: new_record.revision,
            reason: Some("Compacted into a later summary".to_string()),
            now: "2026-06-26T01:04:00Z".to_string(),
        })
        .unwrap();
    assert_eq!(archived.status, SessionMemoryRecordStatus::Archived);
    assert_eq!(archived.revision, 2);

    let stale_archive = store
        .archive_session_memory_record(&SessionMemoryArchive {
            record_id: "session-fact-two".to_string(),
            expected_revision: 1,
            reason: None,
            now: "2026-06-26T01:05:00Z".to_string(),
        })
        .unwrap_err();
    assert_eq!(stale_archive.kind, CoreErrorKind::ActionRejected);

    let active_rows = store
        .query_session_memory_records(&SessionMemoryQuery {
            session_id: Some(SessionId::new("session-alpha")),
            ..SessionMemoryQuery::default()
        })
        .unwrap();
    assert!(active_rows.is_empty());

    let history_rows = store
        .query_session_memory_records(&SessionMemoryQuery {
            session_id: Some(SessionId::new("session-alpha")),
            include_superseded: true,
            include_archived: true,
            ..SessionMemoryQuery::default()
        })
        .unwrap();
    assert_eq!(history_rows.len(), 2);

    remove_temp_db(&db_path);
}

#[test]
fn session_memory_compaction_archives_records_without_touching_message_history() {
    let db_path = temp_db_path("session-memory-compaction");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    store.save_session(&sample_session_state()).unwrap();
    let session_id = SessionId::new("session-alpha");
    let slot_id = MessageSlotId::new("slot-compaction");
    let variant_id = MessageVariantId::new("variant-compaction");
    store
        .save_message_slot(&MessageSlotWrite {
            slot_id: slot_id.clone(),
            session_id: session_id.clone(),
            primary_variant_id: variant_id.clone(),
            active_variant_id: None,
            metadata_json: json!({"fixture": "compaction"}),
            created_at: "2026-06-26T01:00:00Z".to_string(),
            updated_at: "2026-06-26T01:00:00Z".to_string(),
        })
        .unwrap();
    store
        .save_message_variant(&variant_write(
            &slot_id,
            &variant_id,
            MessageVariantSource::Primary,
            0,
            "message-compaction",
            "raw message history must survive compaction",
        ))
        .unwrap();

    for index in 0..4 {
        store
            .add_session_memory_record(&session_fact_memory_write(
                &format!("session-fact-{index}"),
                &session_id,
                &format!("2026-06-26T01:0{index}:00Z"),
            ))
            .unwrap();
    }
    let slots_before = store.count_rows("message_slots").unwrap();
    let variants_before = store.count_rows("message_variants").unwrap();

    let report = store
        .run_maintenance(&RuntimeMaintenancePolicy {
            compact_session_memory_at: Some("2026-06-26T02:00:00Z".to_string()),
            session_memory_max_active_records_per_scope: Some(2),
            session_memory_archive_batch_size: Some(2),
            ..RuntimeMaintenancePolicy::default()
        })
        .unwrap();

    assert!(report.session_memory_compaction.enabled);
    assert_eq!(report.session_memory_compaction.scopes_inspected, 1);
    assert_eq!(
        report.session_memory_compaction.retention_pressure_scopes,
        1
    );
    assert_eq!(report.session_memory_compaction.scopes_compacted, 1);
    assert_eq!(
        report.session_memory_compaction.session_summaries_created,
        1
    );
    assert_eq!(report.session_memory_compaction.records_archived, 2);
    assert_eq!(report.session_memory_compaction.records_superseded, 0);
    assert_eq!(store.count_rows("message_slots").unwrap(), slots_before);
    assert_eq!(
        store.count_rows("message_variants").unwrap(),
        variants_before
    );

    let rows = store
        .query_session_memory_records(&SessionMemoryQuery {
            session_id: Some(session_id),
            include_archived: true,
            ..SessionMemoryQuery::default()
        })
        .unwrap();
    let summary = rows
        .iter()
        .find(|record| record.shape.shape_id.as_str() == "session_summary")
        .expect("summary record");
    assert_eq!(summary.status, SessionMemoryRecordStatus::Active);
    assert_eq!(
        summary.content["metadata_json"]["generated_by"],
        "runtime_maintenance"
    );
    let archived: Vec<_> = rows
        .iter()
        .filter(|record| record.status == SessionMemoryRecordStatus::Archived)
        .collect();
    assert_eq!(archived.len(), 2);
    assert!(archived.iter().all(|record| record
        .archive_reason
        .as_deref()
        .unwrap_or_default()
        .contains(summary.record_id.as_str())));

    remove_temp_db(&db_path);
}

#[test]
fn session_memory_compaction_writes_branch_summary_for_branch_scopes() {
    let db_path = temp_db_path("session-memory-branch-compaction");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    store.save_session(&sample_session_state()).unwrap();
    save_branch_tree(&store);
    let session_id = SessionId::new("session-alpha");
    let branch_id = ConversationBranchId::new("branch-active");

    for index in 0..3 {
        store
            .add_session_memory_record(&branch_user_choice_memory_write(
                &format!("branch-choice-{index}"),
                &session_id,
                &branch_id,
                &format!("2026-06-26T01:1{index}:00Z"),
            ))
            .unwrap();
    }

    let report = store
        .run_maintenance(&RuntimeMaintenancePolicy {
            compact_session_memory_at: Some("2026-06-26T02:10:00Z".to_string()),
            session_memory_max_active_records_per_scope: Some(1),
            session_memory_archive_batch_size: Some(2),
            ..RuntimeMaintenancePolicy::default()
        })
        .unwrap();

    assert_eq!(report.session_memory_compaction.scopes_compacted, 1);
    assert_eq!(report.session_memory_compaction.branch_summaries_created, 1);
    assert_eq!(report.session_memory_compaction.records_archived, 2);
    let rows = store
        .query_session_memory_records(&SessionMemoryQuery {
            session_id: Some(session_id),
            branch_id: Some(branch_id.clone()),
            include_archived: true,
            ..SessionMemoryQuery::default()
        })
        .unwrap();
    let summary = rows
        .iter()
        .find(|record| record.shape.shape_id.as_str() == "branch_summary")
        .expect("branch summary");
    assert_eq!(
        summary.scope.scope_type,
        MemoryScopeType::ConversationBranch
    );
    assert_eq!(summary.branch_id, Some(branch_id.clone()));
    assert_eq!(summary.content["branch_id"], branch_id.0);
    assert_eq!(summary.content["head_message_id"], "branch-active:head");

    remove_temp_db(&db_path);
}

#[test]
fn branch_aware_session_memory_orders_active_ancestor_then_session() {
    let db_path = temp_db_path("session-memory-branch-aware-order");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    store.save_session(&sample_session_state()).unwrap();
    save_branch_tree(&store);

    store
        .add_session_memory_record(&branch_summary_memory_write(
            "memory-root-branch",
            &SessionId::new("session-alpha"),
            &ConversationBranchId::new("branch-root"),
            "2026-06-26T01:01:00Z",
        ))
        .unwrap();
    store
        .add_session_memory_record(&branch_summary_memory_write(
            "memory-active-branch",
            &SessionId::new("session-alpha"),
            &ConversationBranchId::new("branch-active"),
            "2026-06-26T01:02:00Z",
        ))
        .unwrap();
    store
        .add_session_memory_record(&session_fact_memory_write(
            "memory-session",
            &SessionId::new("session-alpha"),
            "2026-06-26T01:03:00Z",
        ))
        .unwrap();

    let context = store
        .build_session_memory_prompt_context(&BranchAwareSessionMemoryQuery {
            session_id: SessionId::new("session-alpha"),
            active_branch_id: Some(ConversationBranchId::new("branch-active")),
            include_ancestors: true,
            include_siblings: false,
            shape_id: None,
            prompt_context_only: true,
            page: None,
        })
        .unwrap();

    assert_eq!(
        context
            .records
            .iter()
            .map(|record| record.record_id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "memory-active-branch",
            "memory-root-branch",
            "memory-session"
        ]
    );
    assert_eq!(
        context.diagnostics.selected_records[0].record_id,
        "memory-active-branch"
    );
    assert_eq!(context.diagnostics.excluded_counts.sibling_branch, 0);
    assert!(context.diagnostics.character_estimate > 0);
    assert!(context.diagnostics.token_estimate > 0);

    remove_temp_db(&db_path);
}

#[test]
fn branch_aware_session_memory_excludes_siblings_by_default() {
    let db_path = temp_db_path("session-memory-branch-aware-siblings");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    store.save_session(&sample_session_state()).unwrap();
    save_branch_tree(&store);

    for (record_id, branch_id, now) in [
        ("memory-root-branch", "branch-root", "2026-06-26T01:01:00Z"),
        (
            "memory-active-branch",
            "branch-active",
            "2026-06-26T01:02:00Z",
        ),
        (
            "memory-sibling-branch",
            "branch-sibling",
            "2026-06-26T01:03:00Z",
        ),
    ] {
        store
            .add_session_memory_record(&branch_summary_memory_write(
                record_id,
                &SessionId::new("session-alpha"),
                &ConversationBranchId::new(branch_id),
                now,
            ))
            .unwrap();
    }

    let default_context = store
        .build_session_memory_prompt_context(&BranchAwareSessionMemoryQuery {
            session_id: SessionId::new("session-alpha"),
            active_branch_id: Some(ConversationBranchId::new("branch-active")),
            include_ancestors: true,
            include_siblings: false,
            shape_id: None,
            prompt_context_only: true,
            page: None,
        })
        .unwrap();
    assert_eq!(
        default_context
            .records
            .iter()
            .map(|record| record.record_id.as_str())
            .collect::<Vec<_>>(),
        vec!["memory-active-branch", "memory-root-branch"]
    );
    assert_eq!(
        default_context.diagnostics.excluded_counts.sibling_branch,
        1
    );

    let sibling_context = store
        .build_session_memory_prompt_context(&BranchAwareSessionMemoryQuery {
            include_siblings: true,
            ..BranchAwareSessionMemoryQuery {
                session_id: SessionId::new("session-alpha"),
                active_branch_id: Some(ConversationBranchId::new("branch-active")),
                include_ancestors: true,
                include_siblings: false,
                shape_id: None,
                prompt_context_only: true,
                page: None,
            }
        })
        .unwrap();
    assert_eq!(
        sibling_context
            .records
            .iter()
            .map(|record| record.record_id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "memory-active-branch",
            "memory-root-branch",
            "memory-sibling-branch"
        ]
    );
    assert_eq!(
        sibling_context.diagnostics.excluded_counts.sibling_branch,
        0
    );

    remove_temp_db(&db_path);
}

#[test]
fn prompt_context_reports_policy_status_and_limit_exclusions() {
    let db_path = temp_db_path("session-memory-prompt-diagnostics");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    store.save_session(&sample_session_state()).unwrap();

    store
        .add_session_memory_record(&session_fact_memory_write(
            "memory-selected",
            &SessionId::new("session-alpha"),
            "2026-06-26T01:00:00Z",
        ))
        .unwrap();
    let archived = store
        .add_session_memory_record(&session_fact_memory_write(
            "memory-archived",
            &SessionId::new("session-alpha"),
            "2026-06-26T01:01:00Z",
        ))
        .unwrap();
    store
        .archive_session_memory_record(&SessionMemoryArchive {
            record_id: archived.record_id,
            expected_revision: archived.revision,
            reason: Some("No longer useful".to_string()),
            now: "2026-06-26T01:02:00Z".to_string(),
        })
        .unwrap();
    let superseded = store
        .add_session_memory_record(&session_fact_memory_write(
            "memory-superseded",
            &SessionId::new("session-alpha"),
            "2026-06-26T01:03:00Z",
        ))
        .unwrap();
    store
        .supersede_session_memory_record(&SessionMemorySupersede {
            record_id: superseded.record_id,
            expected_revision: superseded.revision,
            replacement: SessionMemoryRecordWrite {
                supersedes_record_id: Some("memory-superseded".to_string()),
                content: session_fact_content(
                    "memory-replacement",
                    "Replacement fact remains selectable.",
                    "2026-06-26T01:04:00Z",
                ),
                ..session_fact_memory_write(
                    "memory-replacement",
                    &SessionId::new("session-alpha"),
                    "2026-06-26T01:04:00Z",
                )
            },
        })
        .unwrap();
    store
        .add_session_memory_record(&SessionMemoryRecordWrite {
            content: {
                let mut content = session_fact_content(
                    "memory-tool-only",
                    "Tool-only diagnostic detail.",
                    "2026-06-26T01:05:00Z",
                );
                content["metadata_json"] = json!({"prompt_policy": "tool_only"});
                content
            },
            ..session_fact_memory_write(
                "memory-tool-only",
                &SessionId::new("session-alpha"),
                "2026-06-26T01:05:00Z",
            )
        })
        .unwrap();
    store
        .add_session_memory_record(&SessionMemoryRecordWrite {
            content: {
                let mut content = session_fact_content(
                    "memory-policy-disabled",
                    "Never prompt detail.",
                    "2026-06-26T01:06:00Z",
                );
                content["metadata_json"] = json!({"prompt_policy": "never_prompt"});
                content
            },
            ..session_fact_memory_write(
                "memory-policy-disabled",
                &SessionId::new("session-alpha"),
                "2026-06-26T01:06:00Z",
            )
        })
        .unwrap();

    let context = store
        .build_session_memory_prompt_context(&BranchAwareSessionMemoryQuery {
            session_id: SessionId::new("session-alpha"),
            active_branch_id: None,
            include_ancestors: false,
            include_siblings: false,
            shape_id: None,
            prompt_context_only: true,
            page: Some(QueryPage {
                limit: Some(1),
                offset: None,
            }),
        })
        .unwrap();

    assert_eq!(context.records.len(), 1);
    assert_eq!(
        context.diagnostics.context_policy,
        SessionMemoryPromptContextPolicy::SummaryContext
    );
    assert_eq!(context.diagnostics.excluded_counts.archived, 1);
    assert_eq!(context.diagnostics.excluded_counts.superseded, 1);
    assert_eq!(context.diagnostics.excluded_counts.tool_only, 1);
    assert_eq!(context.diagnostics.excluded_counts.policy_disabled, 1);
    assert_eq!(context.diagnostics.excluded_counts.limit_exceeded, 1);
    assert_eq!(context.diagnostics.selected_records.len(), 1);

    let history = store
        .build_session_memory_prompt_context(&BranchAwareSessionMemoryQuery {
            prompt_context_only: false,
            page: None,
            ..BranchAwareSessionMemoryQuery {
                session_id: SessionId::new("session-alpha"),
                active_branch_id: None,
                include_ancestors: false,
                include_siblings: false,
                shape_id: None,
                prompt_context_only: true,
                page: None,
            }
        })
        .unwrap();
    assert_eq!(
        history.diagnostics.context_policy,
        SessionMemoryPromptContextPolicy::ToolOnly
    );
    assert!(history.records.len() > context.records.len());

    remove_temp_db(&db_path);
}

#[test]
fn memory_proposals_persist_governance_state_without_direct_mutation() {
    let db_path = temp_db_path("memory-proposals");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    let descriptor = profile_dense_memory_space_descriptor();
    let proposal = profile_dense_memory_proposal("proposal_one", "profile_dense:style");

    let created = store
        .save_memory_proposal(&proposal, &descriptor, &"2026-06-26T00:00:00Z".to_string())
        .unwrap();
    assert_eq!(created.proposal.proposal_id, "proposal_one");
    assert_eq!(created.status, MemoryProposalReviewStatus::PendingReview);
    assert_eq!(
        created.selected_governance_mode,
        MemoryGovernanceMode::CuratorRoute
    );
    assert!(store
        .get_profile_memory(
            &ProfileId::new("prime-profile"),
            &ProfileMemoryTarget::Profile,
            "style"
        )
        .unwrap()
        .is_none());

    let duplicate = store
        .save_memory_proposal(
            &profile_dense_memory_proposal("proposal_two", "profile_dense:style"),
            &descriptor,
            &"2026-06-26T00:01:00Z".to_string(),
        )
        .unwrap();
    assert_eq!(duplicate.proposal.proposal_id, "proposal_one");
    assert_eq!(store.count_rows("memory_proposals").unwrap(), 1);

    let pending = store
        .list_memory_proposals(&MemoryProposalQuery {
            space_id: Some(MemorySpaceId::unchecked("profile_dense")),
            status: Some(MemoryProposalReviewStatus::PendingReview),
            dedupe_key: None,
            limit: None,
            offset: None,
        })
        .unwrap();
    assert_eq!(pending.len(), 1);

    let bad_space = store
        .save_memory_proposal(
            &MemoryProposalEnvelope {
                space_id: MemorySpaceId::unchecked("roleplay_lore"),
                ..profile_dense_memory_proposal("proposal_bad_space", "profile_dense:bad")
            },
            &descriptor,
            &"2026-06-26T00:02:00Z".to_string(),
        )
        .unwrap_err();
    assert_eq!(bad_space.kind, CoreErrorKind::InvalidInput);

    let bad_scope = store
        .save_memory_proposal(
            &MemoryProposalEnvelope {
                proposal_id: "proposal_bad_scope".to_string(),
                scope: MemoryScope {
                    scope_type: MemoryScopeType::World,
                    scope_id: "world-alpha".to_string(),
                },
                dedupe_key: Some("profile_dense:bad_scope".to_string()),
                ..proposal.clone()
            },
            &descriptor,
            &"2026-06-26T00:03:00Z".to_string(),
        )
        .unwrap_err();
    assert_eq!(bad_scope.kind, CoreErrorKind::InvalidInput);

    let bad_operation = store
        .save_memory_proposal(
            &MemoryProposalEnvelope {
                proposal_id: "proposal_bad_operation".to_string(),
                operation: MemoryOperation::Merge,
                dedupe_key: Some("profile_dense:bad_operation".to_string()),
                ..proposal.clone()
            },
            &descriptor,
            &"2026-06-26T00:04:00Z".to_string(),
        )
        .unwrap_err();
    assert_eq!(bad_operation.kind, CoreErrorKind::InvalidInput);

    let approved = store
        .record_memory_governance_decision(
            &MemoryGovernanceDecisionInput {
                decision_id: "decision_approve".to_string(),
                proposal_id: "proposal_one".to_string(),
                decision: MemoryGovernanceDecisionKind::Approved,
                actor: "human_operator".to_string(),
                source: MemoryProposalSource::Human,
                evidence_refs: proposal.evidence_refs.clone(),
                policy_mode: MemoryGovernanceMode::ManualReview,
                confidence: Some(0.95),
                message: Some("approved for later apply".to_string()),
                resulting_revision: None,
                decided_at: None,
            },
            &"2026-06-26T00:05:00Z".to_string(),
        )
        .unwrap();
    assert_eq!(approved.decision, MemoryGovernanceDecisionKind::Approved);

    let applied = store
        .record_memory_governance_decision(
            &MemoryGovernanceDecisionInput {
                decision_id: "decision_apply".to_string(),
                proposal_id: "proposal_one".to_string(),
                decision: MemoryGovernanceDecisionKind::Applied,
                actor: "curator".to_string(),
                source: MemoryProposalSource::Human,
                evidence_refs: proposal.evidence_refs.clone(),
                policy_mode: MemoryGovernanceMode::ManualReview,
                confidence: Some(0.97),
                message: Some("compatibility projection only".to_string()),
                resulting_revision: Some(7),
                decided_at: None,
            },
            &"2026-06-26T00:06:00Z".to_string(),
        )
        .unwrap();
    assert_eq!(applied.resulting_revision, Some(7));

    let records = store
        .list_memory_proposals(&MemoryProposalQuery {
            space_id: None,
            status: Some(MemoryProposalReviewStatus::Applied),
            dedupe_key: None,
            limit: None,
            offset: None,
        })
        .unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].status, MemoryProposalReviewStatus::Applied);
    assert_eq!(records[0].resulting_revision, Some(7));
    assert!(store
        .get_profile_memory(
            &ProfileId::new("prime-profile"),
            &ProfileMemoryTarget::Profile,
            "style"
        )
        .unwrap()
        .is_none());

    remove_temp_db(&db_path);
}

#[test]
fn session_activity_digests_save_and_list_by_profile_session_and_wake() {
    let db_path = temp_db_path("session-activity-digests");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    let digest = SessionActivityDigest {
        digest_id: "sad_alpha".to_string(),
        profile_id: ProfileId::new("prime-profile"),
        session_id: SessionId::new("session-alpha"),
        wake_id: "wake-alpha".to_string(),
        source: "direct_debug".to_string(),
        summary_text: "Wake wake-alpha from direct_debug.".to_string(),
        event_counts_json: json!({"brain_event_observed.text_delta": 1}),
        tool_calls_json: json!([{"tool_name": "shell", "status": "failed"}]),
        signals_json: json!([{"signal_type": "tool_failure"}]),
        completion_summary: Some("wake completed".to_string()),
        allowed_capture_spaces: vec![MemorySpaceId::unchecked("profile_dense")],
        created_at: "2026-06-27T12:00:00Z".to_string(),
        retention_until: Some("2026-07-04T12:00:00Z".to_string()),
        reviewed_at: None,
    };

    let saved = store.save_session_activity_digest(&digest).unwrap();
    assert_eq!(saved.digest_id, "sad_alpha");
    assert_eq!(store.count_rows("session_activity_digests").unwrap(), 1);

    let duplicate = SessionActivityDigest {
        summary_text: "Updated deterministic digest.".to_string(),
        ..digest.clone()
    };
    let saved_duplicate = store.save_session_activity_digest(&duplicate).unwrap();
    assert_eq!(
        saved_duplicate.summary_text,
        "Updated deterministic digest."
    );
    assert_eq!(store.count_rows("session_activity_digests").unwrap(), 1);

    let by_profile = store
        .list_session_activity_digests(&SessionActivityDigestQuery {
            profile_id: Some(ProfileId::new("prime-profile")),
            session_id: None,
            wake_id: None,
            include_reviewed: false,
            limit: None,
            offset: None,
        })
        .unwrap();
    assert_eq!(by_profile.len(), 1);
    assert_eq!(by_profile[0].wake_id, "wake-alpha");

    let by_session_wake = store
        .list_session_activity_digests(&SessionActivityDigestQuery {
            profile_id: None,
            session_id: Some(SessionId::new("session-alpha")),
            wake_id: Some("wake-alpha".to_string()),
            include_reviewed: false,
            limit: Some(10),
            offset: Some(0),
        })
        .unwrap();
    assert_eq!(by_session_wake.len(), 1);
    assert_eq!(
        by_session_wake[0].allowed_capture_spaces[0].as_str(),
        "profile_dense"
    );

    remove_temp_db(&db_path);
}

#[test]
fn applied_session_memory_proposals_create_and_update_records() {
    let db_path = temp_db_path("session-memory-proposal-apply");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    store.save_session(&sample_session_state()).unwrap();
    let descriptor = session_memory_space_descriptor();
    let add_proposal = session_memory_record_proposal(
        "session_memory_proposal_add",
        MemoryOperation::Add,
        session_fact_content(
            "session-fact-proposal",
            "User chose the sqlite-first deployment path.",
            "2026-06-26T02:00:00Z",
        ),
    );

    let created = store
        .save_memory_proposal(
            &add_proposal,
            &descriptor,
            &"2026-06-26T02:00:00Z".to_string(),
        )
        .unwrap();
    assert_eq!(created.status, MemoryProposalReviewStatus::PendingReview);
    assert!(store
        .query_session_memory_records(&SessionMemoryQuery {
            session_id: Some(SessionId::new("session-alpha")),
            ..SessionMemoryQuery::default()
        })
        .unwrap()
        .is_empty());
    assert_eq!(store.count_rows("message_slots").unwrap(), 0);
    assert_eq!(store.count_rows("profile_memories").unwrap(), 0);

    store
        .record_memory_governance_decision(
            &MemoryGovernanceDecisionInput {
                decision_id: "session_memory_decision_approve".to_string(),
                proposal_id: "session_memory_proposal_add".to_string(),
                decision: MemoryGovernanceDecisionKind::Approved,
                actor: "human_operator".to_string(),
                source: MemoryProposalSource::Human,
                evidence_refs: session_memory_evidence("ui-review"),
                policy_mode: MemoryGovernanceMode::ManualReview,
                confidence: Some(0.95),
                message: Some("approved session memory add".to_string()),
                resulting_revision: None,
                decided_at: None,
            },
            &"2026-06-26T02:01:00Z".to_string(),
        )
        .unwrap();
    assert!(store
        .query_session_memory_records(&SessionMemoryQuery {
            session_id: Some(SessionId::new("session-alpha")),
            ..SessionMemoryQuery::default()
        })
        .unwrap()
        .is_empty());

    let applied = store
        .record_memory_governance_decision(
            &MemoryGovernanceDecisionInput {
                decision_id: "session_memory_decision_apply".to_string(),
                proposal_id: "session_memory_proposal_add".to_string(),
                decision: MemoryGovernanceDecisionKind::Applied,
                actor: "curator".to_string(),
                source: MemoryProposalSource::Human,
                evidence_refs: session_memory_evidence("ui-apply"),
                policy_mode: MemoryGovernanceMode::ManualReview,
                confidence: Some(0.97),
                message: Some("apply session memory add".to_string()),
                resulting_revision: None,
                decided_at: None,
            },
            &"2026-06-26T02:02:00Z".to_string(),
        )
        .unwrap();
    assert_eq!(applied.resulting_revision, Some(1));
    let records = store
        .query_session_memory_records(&SessionMemoryQuery {
            session_id: Some(SessionId::new("session-alpha")),
            ..SessionMemoryQuery::default()
        })
        .unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].record_id, "session-fact-proposal");
    assert_eq!(records[0].revision, 1);
    assert_eq!(records[0].source, MemoryProposalSource::CaptureProducer);
    assert_eq!(
        records[0].durability_rationale,
        "Session proposal should survive future wakes."
    );
    assert_eq!(records[0].evidence_refs, add_proposal.evidence_refs);
    assert_eq!(store.count_rows("message_slots").unwrap(), 0);
    assert_eq!(store.count_rows("profile_memories").unwrap(), 0);

    let replace_proposal = session_memory_record_proposal(
        "session_memory_proposal_replace",
        MemoryOperation::Replace,
        {
            let mut content = session_fact_content(
                "session-fact-proposal",
                "User chose sqlite-first deployment before Postgres shakedown.",
                "2026-06-26T02:03:00Z",
            );
            content["expected_revision"] = json!(1);
            content
        },
    );
    store
        .save_memory_proposal(
            &replace_proposal,
            &descriptor,
            &"2026-06-26T02:03:00Z".to_string(),
        )
        .unwrap();
    store
        .record_memory_governance_decision(
            &MemoryGovernanceDecisionInput {
                decision_id: "session_memory_replace_approve".to_string(),
                proposal_id: "session_memory_proposal_replace".to_string(),
                decision: MemoryGovernanceDecisionKind::Approved,
                actor: "human_operator".to_string(),
                source: MemoryProposalSource::Human,
                evidence_refs: session_memory_evidence("ui-review-replace"),
                policy_mode: MemoryGovernanceMode::ManualReview,
                confidence: Some(0.94),
                message: Some("approved session memory replace".to_string()),
                resulting_revision: None,
                decided_at: None,
            },
            &"2026-06-26T02:04:00Z".to_string(),
        )
        .unwrap();
    let replaced = store
        .record_memory_governance_decision(
            &MemoryGovernanceDecisionInput {
                decision_id: "session_memory_replace_apply".to_string(),
                proposal_id: "session_memory_proposal_replace".to_string(),
                decision: MemoryGovernanceDecisionKind::Applied,
                actor: "curator".to_string(),
                source: MemoryProposalSource::Human,
                evidence_refs: session_memory_evidence("ui-apply-replace"),
                policy_mode: MemoryGovernanceMode::ManualReview,
                confidence: Some(0.96),
                message: Some("apply session memory replace".to_string()),
                resulting_revision: None,
                decided_at: None,
            },
            &"2026-06-26T02:05:00Z".to_string(),
        )
        .unwrap();
    assert_eq!(replaced.resulting_revision, Some(2));
    let replaced_record = store
        .query_session_memory_records(&SessionMemoryQuery {
            session_id: Some(SessionId::new("session-alpha")),
            ..SessionMemoryQuery::default()
        })
        .unwrap()
        .pop()
        .expect("updated session memory record");
    assert_eq!(replaced_record.revision, 2);
    assert_eq!(
        replaced_record.content["content"],
        "User chose sqlite-first deployment before Postgres shakedown."
    );
    assert_eq!(
        replaced_record.evidence_refs,
        replace_proposal.evidence_refs
    );
    assert_eq!(store.count_rows("message_slots").unwrap(), 0);
    assert_eq!(store.count_rows("profile_memories").unwrap(), 0);

    remove_temp_db(&db_path);
}

#[test]
fn scheduled_jobs_claim_runs_and_reconcile_stale_claims() {
    let db_path = temp_db_path("scheduled-jobs");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    store
        .upsert_scheduled_job(&ScheduledJobRecord {
            job_id: "wake-prime".to_string(),
            job_kind: "runtime.wake.session".to_string(),
            target_session_id: Some(SessionId::new("prime-session")),
            interval_ms: Some(60_000),
            next_due_at: Some("2026-06-20T06:00:00Z".to_string()),
            payload_json: serde_json::json!({"reason": "scheduled"}),
            status: ScheduledJobStatus::Active,
            created_at: "2026-06-20T05:59:00Z".to_string(),
            updated_at: "2026-06-20T05:59:00Z".to_string(),
            paused_at: None,
        })
        .unwrap();

    let due = store
        .query_scheduled_jobs(&ScheduledJobQuery {
            status: Some(ScheduledJobStatus::Active),
            due_at_or_before: Some("2026-06-20T06:00:00Z".to_string()),
            ..ScheduledJobQuery::default()
        })
        .unwrap();
    assert_eq!(due.len(), 1);

    let run = ScheduledRunRecord {
        run_id: RunId::new("scheduled:wake-prime:1"),
        job_id: "wake-prime".to_string(),
        job_kind: "runtime.wake.session".to_string(),
        target_session_id: Some(SessionId::new("prime-session")),
        status: ScheduledRunStatus::Claimed,
        trigger: ScheduledRunTrigger::Due,
        scheduled_for: Some("2026-06-20T06:00:00Z".to_string()),
        claimed_at: "2026-06-20T06:00:00Z".to_string(),
        claim_deadline_at: "2026-06-20T06:00:30Z".to_string(),
        completed_at: None,
        error: None,
        output_json: serde_json::json!({}),
        created_at: "2026-06-20T06:00:00Z".to_string(),
        updated_at: "2026-06-20T06:00:00Z".to_string(),
    };
    store
        .claim_scheduled_run(&run, Some(&"2026-06-20T06:01:00Z".to_string()))
        .unwrap();
    assert_eq!(
        store
            .load_scheduled_job("wake-prime")
            .unwrap()
            .unwrap()
            .next_due_at,
        Some("2026-06-20T06:01:00Z".to_string())
    );

    store
        .complete_scheduled_run(
            &run.run_id,
            ScheduledRunStatus::Completed,
            &"2026-06-20T06:00:01Z".to_string(),
            &serde_json::json!({"wake_requested": true}),
            None,
        )
        .unwrap();
    let completed = store
        .query_scheduled_runs(&ScheduledRunQuery {
            status: Some(ScheduledRunStatus::Completed),
            ..ScheduledRunQuery::default()
        })
        .unwrap();
    assert_eq!(completed.len(), 1);

    store
        .claim_scheduled_run(
            &ScheduledRunRecord {
                run_id: RunId::new("scheduled:wake-prime:2"),
                status: ScheduledRunStatus::Claimed,
                trigger: ScheduledRunTrigger::Manual,
                claimed_at: "2026-06-20T06:02:00Z".to_string(),
                claim_deadline_at: "2026-06-20T06:02:05Z".to_string(),
                created_at: "2026-06-20T06:02:00Z".to_string(),
                updated_at: "2026-06-20T06:02:00Z".to_string(),
                scheduled_for: None,
                completed_at: None,
                error: None,
                output_json: serde_json::json!({}),
                ..run.clone()
            },
            None,
        )
        .unwrap();
    let expired = store
        .expire_stale_scheduled_runs(
            &"2026-06-20T06:02:06Z".to_string(),
            &"2026-06-20T06:02:06Z".to_string(),
        )
        .unwrap();
    assert_eq!(expired.len(), 1);
    assert_eq!(
        store
            .query_scheduled_runs(&ScheduledRunQuery {
                status: Some(ScheduledRunStatus::Expired),
                ..ScheduledRunQuery::default()
            })
            .unwrap()
            .len(),
        1
    );

    store
        .pause_scheduled_job("wake-prime", &"2026-06-20T06:03:00Z".to_string())
        .unwrap();
    assert_eq!(
        store
            .load_scheduled_job("wake-prime")
            .unwrap()
            .unwrap()
            .status,
        ScheduledJobStatus::Paused
    );
    store
        .resume_scheduled_job(
            "wake-prime",
            &"2026-06-20T06:04:00Z".to_string(),
            &"2026-06-20T06:03:30Z".to_string(),
        )
        .unwrap();
    assert_eq!(
        store
            .load_scheduled_job("wake-prime")
            .unwrap()
            .unwrap()
            .next_due_at,
        Some("2026-06-20T06:04:00Z".to_string())
    );

    remove_temp_db(&db_path);
}

#[test]
fn provider_wire_state_replaces_current_record_and_preserves_payload_version() {
    let db_path = temp_db_path("provider-wire-replace");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    let key = sample_provider_wire_state_key();

    store
        .save_provider_wire_state(&sample_provider_wire_state_write(
            ProviderWireStateWriteFixture {
                key: key.clone(),
                profile_fingerprint: "profile-fp-1",
                provider_fingerprint: "provider-fp-1",
                payload_version: "provider-owned-v1",
                payload_json: serde_json::json!({"response_id": "resp-1"}),
                now: "2026-06-20T00:00:00Z",
                expires_at: Some("2026-06-20T06:00:00Z"),
                last_wake_id: Some("wake-1"),
            },
        ))
        .unwrap();
    store
        .save_provider_wire_state(&sample_provider_wire_state_write(
            ProviderWireStateWriteFixture {
                key: key.clone(),
                profile_fingerprint: "profile-fp-1",
                provider_fingerprint: "provider-fp-1",
                payload_version: "provider-owned-v9000",
                payload_json: serde_json::json!({"response_id": "resp-2"}),
                now: "2026-06-20T00:01:00Z",
                expires_at: Some("2026-06-20T06:01:00Z"),
                last_wake_id: Some("wake-2"),
            },
        ))
        .unwrap();

    assert_eq!(store.count_rows("provider_wire_states").unwrap(), 2);
    let loaded = store
        .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
            key: key.clone(),
            profile_fingerprint: "profile-fp-1".to_string(),
            provider_fingerprint: "provider-fp-1".to_string(),
            now: "2026-06-20T00:02:00Z".to_string(),
        })
        .unwrap();
    let record = loaded.record.unwrap();
    assert_eq!(loaded.absence_reason, None);
    assert_eq!(record.payload_version, "provider-owned-v9000");
    assert_eq!(
        record.payload_json,
        serde_json::json!({"response_id": "resp-2"})
    );
    assert_eq!(record.last_wake_id.as_deref(), Some("wake-2"));
    assert!(record.is_current());

    remove_temp_db(&db_path);
}

#[test]
fn provider_wire_state_withholds_expired_and_preserves_fingerprint_stale_records() {
    let db_path = temp_db_path("provider-wire-invalidation");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    let key = sample_provider_wire_state_key();

    store
        .save_provider_wire_state(&sample_provider_wire_state_write(
            ProviderWireStateWriteFixture {
                key: key.clone(),
                profile_fingerprint: "profile-fp-1",
                provider_fingerprint: "provider-fp-1",
                payload_version: "provider-owned-v1",
                payload_json: serde_json::json!({"response_id": "expired"}),
                now: "2026-06-20T00:00:00Z",
                expires_at: Some("2026-06-20T00:05:00Z"),
                last_wake_id: Some("wake-expired"),
            },
        ))
        .unwrap();
    let expired = store
        .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
            key: key.clone(),
            profile_fingerprint: "profile-fp-1".to_string(),
            provider_fingerprint: "provider-fp-1".to_string(),
            now: "2026-06-20T00:05:00Z".to_string(),
        })
        .unwrap();
    assert_eq!(expired.record, None);
    assert_eq!(
        expired.absence_reason,
        Some(ProviderStateAbsenceReason::Expired)
    );

    store
        .save_provider_wire_state(&sample_provider_wire_state_write(
            ProviderWireStateWriteFixture {
                key: key.clone(),
                profile_fingerprint: "profile-fp-1",
                provider_fingerprint: "provider-fp-1",
                payload_version: "provider-owned-v2",
                payload_json: serde_json::json!({"response_id": "profile-stale"}),
                now: "2026-06-20T00:06:00Z",
                expires_at: Some("2026-06-20T06:00:00Z"),
                last_wake_id: Some("wake-profile-stale"),
            },
        ))
        .unwrap();
    let profile_stale = store
        .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
            key: key.clone(),
            profile_fingerprint: "profile-fp-2".to_string(),
            provider_fingerprint: "provider-fp-1".to_string(),
            now: "2026-06-20T00:07:00Z".to_string(),
        })
        .unwrap();
    assert_eq!(profile_stale.record, None);
    assert_eq!(
        profile_stale.absence_reason,
        Some(ProviderStateAbsenceReason::Invalidated)
    );
    let profile_rollback = store
        .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
            key: key.clone(),
            profile_fingerprint: "profile-fp-1".to_string(),
            provider_fingerprint: "provider-fp-1".to_string(),
            now: "2026-06-20T00:07:30Z".to_string(),
        })
        .unwrap();
    assert_eq!(
        profile_rollback.record.unwrap().payload_version,
        "provider-owned-v2"
    );

    store
        .save_provider_wire_state(&sample_provider_wire_state_write(
            ProviderWireStateWriteFixture {
                key: key.clone(),
                profile_fingerprint: "profile-fp-2",
                provider_fingerprint: "provider-fp-1",
                payload_version: "provider-owned-v3",
                payload_json: serde_json::json!({"response_id": "provider-stale"}),
                now: "2026-06-20T00:08:00Z",
                expires_at: Some("2026-06-20T06:00:00Z"),
                last_wake_id: Some("wake-provider-stale"),
            },
        ))
        .unwrap();
    let provider_stale = store
        .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
            key: key.clone(),
            profile_fingerprint: "profile-fp-2".to_string(),
            provider_fingerprint: "provider-fp-2".to_string(),
            now: "2026-06-20T00:09:00Z".to_string(),
        })
        .unwrap();
    assert_eq!(provider_stale.record, None);
    assert_eq!(
        provider_stale.absence_reason,
        Some(ProviderStateAbsenceReason::Invalidated)
    );
    let provider_rollback = store
        .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
            key,
            profile_fingerprint: "profile-fp-2".to_string(),
            provider_fingerprint: "provider-fp-1".to_string(),
            now: "2026-06-20T00:09:30Z".to_string(),
        })
        .unwrap();
    assert_eq!(
        provider_rollback.record.unwrap().payload_version,
        "provider-owned-v3"
    );

    remove_temp_db(&db_path);
}

#[test]
fn provider_wire_state_clear_and_strategy_change_remove_current_state() {
    let db_path = temp_db_path("provider-wire-clear");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    let key = sample_provider_wire_state_key();

    store
        .save_provider_wire_state(&sample_provider_wire_state_write(
            ProviderWireStateWriteFixture {
                key: key.clone(),
                profile_fingerprint: "profile-fp",
                provider_fingerprint: "provider-fp",
                payload_version: "provider-owned-v1",
                payload_json: serde_json::json!({"response_id": "clear-me"}),
                now: "2026-06-20T00:00:00Z",
                expires_at: Some("2026-06-20T06:00:00Z"),
                last_wake_id: Some("wake-clear"),
            },
        ))
        .unwrap();
    let cleared = store
        .clear_provider_wire_state(
            &key,
            &"2026-06-20T00:01:00Z".to_string(),
            ProviderWireStateInvalidationReason::BrainRequestedClear,
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        cleared.invalidation_reason,
        Some(ProviderWireStateInvalidationReason::BrainRequestedClear)
    );
    let after_clear = store
        .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
            key: key.clone(),
            profile_fingerprint: "profile-fp".to_string(),
            provider_fingerprint: "provider-fp".to_string(),
            now: "2026-06-20T00:02:00Z".to_string(),
        })
        .unwrap();
    assert_eq!(after_clear.record, None);
    assert_eq!(
        after_clear.absence_reason,
        Some(ProviderStateAbsenceReason::Missing)
    );

    store
        .save_provider_wire_state(&sample_provider_wire_state_write(
            ProviderWireStateWriteFixture {
                key: key.clone(),
                profile_fingerprint: "profile-fp",
                provider_fingerprint: "provider-fp",
                payload_version: "provider-owned-v2",
                payload_json: serde_json::json!({"response_id": "old-strategy"}),
                now: "2026-06-20T00:03:00Z",
                expires_at: Some("2026-06-20T06:00:00Z"),
                last_wake_id: Some("wake-old-strategy"),
            },
        ))
        .unwrap();
    let changed_key = ProviderWireStateKey {
        strategy_id: "replay-v2".to_string(),
        ..key.clone()
    };
    let changed = store
        .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
            key: changed_key,
            profile_fingerprint: "profile-fp".to_string(),
            provider_fingerprint: "provider-fp".to_string(),
            now: "2026-06-20T00:04:00Z".to_string(),
        })
        .unwrap();
    assert_eq!(changed.record, None);
    assert_eq!(
        changed.absence_reason,
        Some(ProviderStateAbsenceReason::Missing)
    );
    let old_key_after_strategy_change = store
        .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
            key,
            profile_fingerprint: "profile-fp".to_string(),
            provider_fingerprint: "provider-fp".to_string(),
            now: "2026-06-20T00:05:00Z".to_string(),
        })
        .unwrap();
    assert_eq!(old_key_after_strategy_change.record, None);

    remove_temp_db(&db_path);
}

#[test]
fn provider_wire_state_maintenance_marks_expired_current_records() {
    let db_path = temp_db_path("provider-wire-maintenance");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    let key = sample_provider_wire_state_key();

    store
        .save_provider_wire_state(&sample_provider_wire_state_write(
            ProviderWireStateWriteFixture {
                key: key.clone(),
                profile_fingerprint: "profile-fp",
                provider_fingerprint: "provider-fp",
                payload_version: "provider-owned-v1",
                payload_json: serde_json::json!({"response_id": "expire-me"}),
                now: "2026-06-20T00:00:00Z",
                expires_at: Some("2026-06-20T00:05:00Z"),
                last_wake_id: Some("wake-expire-me"),
            },
        ))
        .unwrap();
    let report = store
        .run_maintenance(&RuntimeMaintenancePolicy {
            expire_provider_wire_states_at: Some("2026-06-20T00:05:01Z".to_string()),
            ..RuntimeMaintenancePolicy::default()
        })
        .unwrap();
    assert_eq!(report.expired_provider_wire_states, 1);
    let after_expiry = store
        .load_provider_wire_state_for_wake(&ProviderWireStateWakeLookup {
            key,
            profile_fingerprint: "profile-fp".to_string(),
            provider_fingerprint: "provider-fp".to_string(),
            now: "2026-06-20T00:05:02Z".to_string(),
        })
        .unwrap();
    assert_eq!(after_expiry.record, None);
    assert_eq!(
        after_expiry.absence_reason,
        Some(ProviderStateAbsenceReason::Missing)
    );

    remove_temp_db(&db_path);
}

#[test]
fn saving_session_projects_durable_identity_records() {
    let db_path = temp_db_path("session-identity");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    store.save_session(&sample_session_state()).unwrap();

    let agents = store.load_agent_identities().unwrap();
    let instances = store.load_agent_instances().unwrap();
    let sessions = store.load_session_identities().unwrap();

    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].agent_id, AgentId::new("agent-alpha"));
    assert_eq!(agents[0].kind, DurableAgentKind::Full);
    assert_eq!(instances.len(), 1);
    assert_eq!(
        instances[0].instance_id,
        AgentInstanceId::new("instance:session-alpha")
    );
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session_id, SessionId::new("session-alpha"));
    assert_eq!(
        sessions[0].instance_id,
        AgentInstanceId::new("instance:session-alpha")
    );

    remove_temp_db(&db_path);
}

#[test]
fn explicit_identity_records_round_trip_source_and_den_references() {
    let db_path = temp_db_path("explicit-identity");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    let den = DenRuntimeReference {
        project_id: Some(ProjectId::new("pi-crew")),
        task_id: Some(TaskId::new("123")),
    };
    let source = Some(SourceSystemReference {
        system: "hermes".to_string(),
        external_id: "hermes-agent-1".to_string(),
    });

    store
        .upsert_agent_identity(&DurableAgentRecord {
            agent_id: AgentId::new("agent-imported"),
            display_label: "Imported Agent".to_string(),
            profile_id: ProfileId::new("prime-profile"),
            kind: DurableAgentKind::Prime,
            status: DurableIdentityStatus::Active,
            source: source.clone(),
            den: den.clone(),
            created_at: "2026-06-20T01:00:00Z".to_string(),
            archived_at: None,
        })
        .unwrap();
    store
        .upsert_agent_instance(&AgentInstanceRecord {
            instance_id: AgentInstanceId::new("instance-imported"),
            agent_id: AgentId::new("agent-imported"),
            display_label: "Imported Agent / main".to_string(),
            profile_id: ProfileId::new("prime-profile"),
            status: DurableIdentityStatus::Active,
            source: source.clone(),
            den: den.clone(),
            created_at: "2026-06-20T01:00:00Z".to_string(),
            last_active_at: "2026-06-20T01:05:00Z".to_string(),
            archived_at: None,
        })
        .unwrap();
    store
        .upsert_session_identity(&SessionIdentityRecord {
            session_id: SessionId::new("session-imported"),
            instance_id: AgentInstanceId::new("instance-imported"),
            agent_id: AgentId::new("agent-imported"),
            profile_id: ProfileId::new("prime-profile"),
            kind: SessionKind::Full,
            status: SessionStatus::Active,
            source,
            den,
            created_at: "2026-06-20T01:00:00Z".to_string(),
            last_active_at: "2026-06-20T01:05:00Z".to_string(),
            archived_at: None,
        })
        .unwrap();

    let agent = store.load_agent_identities().unwrap().remove(0);
    let instance = store.load_agent_instances().unwrap().remove(0);
    let session = store.load_session_identities().unwrap().remove(0);

    assert_eq!(agent.kind, DurableAgentKind::Prime);
    assert_eq!(
        agent.source.unwrap().external_id,
        "hermes-agent-1".to_string()
    );
    assert_eq!(instance.den.project_id, Some(ProjectId::new("pi-crew")));
    assert_eq!(session.den.task_id, Some(TaskId::new("123")));

    remove_temp_db(&db_path);
}

#[test]
fn session_config_snapshot_is_immutable_creation_context() {
    let db_path = temp_db_path("session-config");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    let config = sample_session_config();
    let mut state = sample_session_state();
    store.save_session_with_config(&state, &config).unwrap();

    state.resource_limits.max_duration_ms = Some(10);
    state.tool_profile.tools.clear();
    state.last_active_at = "2026-06-20T00:10:00Z".to_string();
    store.save_session(&state).unwrap();

    let live_state = store.load_sessions().unwrap().remove(0);
    let config_snapshot = store.load_session_configs().unwrap().remove(0);

    assert_eq!(live_state.resource_limits.max_duration_ms, Some(10));
    assert_eq!(live_state.tool_profile.tools.len(), 0);
    assert_eq!(
        config_snapshot.resource_limits.max_duration_ms,
        Some(60_000)
    );
    assert_eq!(config_snapshot.tool_profile.tools.len(), 1);
    assert_eq!(
        config_snapshot.config.resource_limits.max_delegation_depth,
        Some(4)
    );
    assert_eq!(config_snapshot.created_at, state.created_at);

    remove_temp_db(&db_path);
}

#[test]
fn event_log_projection_indexes_support_typed_queries() {
    let db_path = temp_db_path("event-projections");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    let session = sample_session_state();

    store
        .save_event(
            1,
            &CoreEvent::SessionCreated {
                state: Box::new(session.clone()),
            },
        )
        .unwrap();
    store
        .save_event(
            2,
            &CoreEvent::AgentMessageRouted {
                message: AgentMessage {
                    from: AgentId::new("agent-alpha"),
                    to: AgentId::new("agent-beta"),
                    body: "hello".to_string(),
                    correlation_id: Some("corr-1".to_string()),
                    projection: None,
                },
            },
        )
        .unwrap();
    store
        .save_event(
            3,
            &CoreEvent::BrainEventObserved {
                session_id: session.session_id.clone(),
                wake_id: Some("wake-1".to_string()),
                event: BrainEvent::Started,
            },
        )
        .unwrap();

    let by_session = store
        .query_events(&RuntimeEventFilter {
            session_id: Some(SessionId::new("session-alpha")),
            ..RuntimeEventFilter::default()
        })
        .unwrap();
    let by_agent = store
        .query_events(&RuntimeEventFilter {
            agent_id: Some(AgentId::new("agent-beta")),
            ..RuntimeEventFilter::default()
        })
        .unwrap();
    let by_correlation = store
        .query_events(&RuntimeEventFilter {
            correlation_id: Some("corr-1".to_string()),
            ..RuntimeEventFilter::default()
        })
        .unwrap();
    let by_wake = store
        .query_events(&RuntimeEventFilter {
            source_wake_id: Some("wake-1".to_string()),
            ..RuntimeEventFilter::default()
        })
        .unwrap();

    assert_eq!(by_session.len(), 2);
    assert_eq!(
        by_session[0].session_ids,
        vec![SessionId::new("session-alpha")]
    );
    assert_eq!(
        by_session[0].instance_ids,
        vec![AgentInstanceId::new("instance:session-alpha")]
    );
    assert_eq!(by_agent.len(), 1);
    assert_eq!(by_agent[0].agent_ids.len(), 2);
    assert_eq!(by_correlation.len(), 1);
    assert_eq!(by_correlation[0].correlation_ids, vec!["corr-1"]);
    assert_eq!(by_wake.len(), 1);
    assert_eq!(by_wake[0].source_wake_ids, vec!["wake-1"]);
    assert_eq!(store.count_rows("event_session_index").unwrap(), 2);

    remove_temp_db(&db_path);
}

#[test]
fn runtime_search_indexes_messages_and_session_configs() {
    let db_path = temp_db_path("runtime-search");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    let config = sample_session_config();
    let state = sample_session_state();
    store.save_session_with_config(&state, &config).unwrap();
    store
        .save_event(
            1,
            &CoreEvent::AgentMessageRouted {
                message: AgentMessage {
                    from: AgentId::new("agent-alpha"),
                    to: AgentId::new("agent-beta"),
                    body: "hello nebula".to_string(),
                    correlation_id: Some("corr-search".to_string()),
                    projection: None,
                },
            },
        )
        .unwrap();

    let sessions = store
        .search_runtime(&RuntimeSearchFilter {
            query: "tools".to_string(),
            row_type: Some(RuntimeSearchRowType::Session),
            session_id: Some(SessionId::new("session-alpha")),
            agent_id: None,
            instance_id: None,
            task_id: None,
            event_kind: None,
            recorded_after: None,
            recorded_before: None,
            limit: Some(10),
        })
        .unwrap();
    let messages = store
        .search_runtime(&RuntimeSearchFilter {
            query: "nebula".to_string(),
            row_type: Some(RuntimeSearchRowType::Message),
            session_id: None,
            agent_id: Some(AgentId::new("agent-beta")),
            instance_id: None,
            task_id: None,
            event_kind: Some(CoreEventKind::AgentMessageRouted),
            recorded_after: None,
            recorded_before: None,
            limit: Some(10),
        })
        .unwrap();

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].row_type, RuntimeSearchRowType::Session);
    assert_eq!(
        sessions[0].session_id,
        Some(SessionId::new("session-alpha"))
    );
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].row_type, RuntimeSearchRowType::Message);
    assert_eq!(messages[0].agent_id, Some(AgentId::new("agent-beta")));
    assert_eq!(messages[0].sequence, Some(1));
    assert!(store
        .search_runtime(&RuntimeSearchFilter {
            query: "pi-crew".to_string(),
            row_type: None,
            session_id: None,
            agent_id: None,
            instance_id: None,
            task_id: None,
            event_kind: None,
            recorded_after: None,
            recorded_before: None,
            limit: Some(10),
        })
        .unwrap()
        .is_empty());

    remove_temp_db(&db_path);
}

#[test]
fn runtime_counters_increment_by_scope_without_scanning_history() {
    let db_path = temp_db_path("runtime-counters");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    let session = sample_session_state();
    let delegated_session_id = SessionId::new("delegated-alpha");

    store
        .save_event(
            1,
            &CoreEvent::BrainWakeRequested {
                session_id: session.session_id.clone(),
            },
        )
        .unwrap();
    store
        .save_event(
            2,
            &CoreEvent::BrainActionsAccepted {
                session_id: session.session_id.clone(),
                count: 2,
            },
        )
        .unwrap();
    store
        .save_event(
            3,
            &CoreEvent::BrainEventObserved {
                session_id: session.session_id.clone(),
                wake_id: Some("wake-tools".to_string()),
                event: BrainEvent::ToolCallStarted {
                    tool_name: "read_file".to_string(),
                    metadata: None,
                },
            },
        )
        .unwrap();
    store
        .save_event(
            4,
            &CoreEvent::BrainEventObserved {
                session_id: session.session_id.clone(),
                wake_id: Some("wake-tools".to_string()),
                event: BrainEvent::ToolCallFinished {
                    tool_name: "read_file".to_string(),
                    is_error: true,
                    metadata: None,
                },
            },
        )
        .unwrap();
    store
        .save_event(
            5,
            &CoreEvent::AgentMessageRouted {
                message: AgentMessage {
                    from: AgentId::new("agent-alpha"),
                    to: AgentId::new("agent-beta"),
                    body: "counter message".to_string(),
                    correlation_id: None,
                    projection: None,
                },
            },
        )
        .unwrap();
    store
        .save_event(
            6,
            &CoreEvent::DelegationLifecycleObserved {
                lifecycle: rusty_crew_core_protocol::DelegationLifecycleEvent {
                    parent_session_id: session.session_id.clone(),
                    delegated_session_id: delegated_session_id.clone(),
                    run_id: Some(RunId::new("wake-tools:0")),
                    phase: rusty_crew_core_protocol::DelegationLifecyclePhase::Created,
                    detail: None,
                },
            },
        )
        .unwrap();
    store
        .save_event(
            7,
            &CoreEvent::DelegationLifecycleObserved {
                lifecycle: rusty_crew_core_protocol::DelegationLifecycleEvent {
                    parent_session_id: session.session_id.clone(),
                    delegated_session_id,
                    run_id: Some(RunId::new("wake-tools:0")),
                    phase: rusty_crew_core_protocol::DelegationLifecyclePhase::TimedOut,
                    detail: None,
                },
            },
        )
        .unwrap();
    store
        .save_event(
            8,
            &CoreEvent::CompletionPacketDelivered {
                packet: CompletionPacket {
                    session_id: session.session_id.clone(),
                    status: rusty_crew_core_protocol::CompletionStatus::Completed,
                    summary: "done".to_string(),
                },
            },
        )
        .unwrap();

    // Re-saving the same sequence replaces projections but must not inflate counters.
    store
        .save_event(
            8,
            &CoreEvent::CompletionPacketDelivered {
                packet: CompletionPacket {
                    session_id: session.session_id.clone(),
                    status: rusty_crew_core_protocol::CompletionStatus::Completed,
                    summary: "done again".to_string(),
                },
            },
        )
        .unwrap();

    let runtime = store
        .runtime_summary(&RuntimeCounterScope::Runtime)
        .unwrap();
    let session_summary = store
        .runtime_summary(&RuntimeCounterScope::Session(SessionId::new(
            "session-alpha",
        )))
        .unwrap();
    let agent_summary = store
        .runtime_summary(&RuntimeCounterScope::Agent(AgentId::new("agent-beta")))
        .unwrap();

    assert_eq!(runtime.wakes, 1);
    assert_eq!(runtime.brain_turns, 1);
    assert_eq!(runtime.tool_calls, 1);
    assert_eq!(runtime.tool_errors, 1);
    assert_eq!(runtime.messages, 1);
    assert_eq!(runtime.delegations_created, 1);
    assert_eq!(runtime.delegations_timed_out, 1);
    assert_eq!(runtime.completions, 1);
    assert_eq!(session_summary.wakes, 1);
    assert_eq!(session_summary.completions, 1);
    assert_eq!(agent_summary.messages, 1);
    assert_eq!(store.count_rows("runtime_counters").unwrap(), 31);

    remove_temp_db(&db_path);
}

#[test]
fn runtime_counter_reset_zeroes_selected_derived_rows() {
    let db_path = temp_db_path("runtime-counter-reset");
    let store = CoordinationStore::open_file(&db_path).unwrap();

    store
        .save_event(
            1,
            &CoreEvent::AgentMessageRouted {
                message: AgentMessage {
                    from: AgentId::new("agent-alpha"),
                    to: AgentId::new("agent-beta"),
                    body: "reset this derived projection".to_string(),
                    correlation_id: None,
                    projection: None,
                },
            },
        )
        .unwrap();

    let reset = store
        .reset_runtime_counters(
            &RuntimeCounterQuery {
                scope: Some(RuntimeCounterScope::Runtime),
                counter_name: Some(COUNTER_MESSAGES.to_string()),
                page: None,
            },
            "2026-06-20T08:00:00Z".to_string(),
        )
        .unwrap();
    let runtime = store
        .runtime_summary(&RuntimeCounterScope::Runtime)
        .unwrap();
    let agent_beta = store
        .runtime_summary(&RuntimeCounterScope::Agent(AgentId::new("agent-beta")))
        .unwrap();

    assert_eq!(reset, 1);
    assert_eq!(runtime.messages, 0);
    assert_eq!(agent_beta.messages, 1);
    assert_eq!(
        store
            .query_runtime_counters(&RuntimeCounterQuery {
                scope: Some(RuntimeCounterScope::Runtime),
                counter_name: Some(COUNTER_MESSAGES.to_string()),
                page: None,
            })
            .unwrap()[0]
            .updated_at,
        "2026-06-20T08:00:00Z"
    );

    remove_temp_db(&db_path);
}

#[test]
fn queued_message_expiry_is_queryable_without_redelivery() {
    let db_path = temp_db_path("queued-messages");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    let record = QueuedMessageRecord {
        message_id: "queue-1".to_string(),
        owner_session_id: Some(SessionId::new("session-alpha")),
        owner_agent_id: AgentId::new("agent-alpha"),
        message: AgentMessage {
            from: AgentId::new("operator"),
            to: AgentId::new("agent-alpha"),
            body: "time boxed queue work".to_string(),
            correlation_id: Some("queue-corr".to_string()),
            projection: None,
        },
        source_sequence: Some(42),
        enqueued_at: "2026-06-20T00:00:00Z".to_string(),
        expires_at: "2026-06-20T00:00:05Z".to_string(),
        ttl_ms: 5_000,
        delivery_attempts: 0,
        state: QueuedMessageState::Pending,
        terminal_at: None,
        state_reason: None,
    };

    store.save_queued_message(&record).unwrap();
    assert_eq!(
        store
            .load_queued_messages(&QueuedMessageFilter {
                state: Some(QueuedMessageState::Pending),
                owner_session_id: Some(SessionId::new("session-alpha")),
                owner_agent_id: None,
                limit: None,
            })
            .unwrap()
            .len(),
        1
    );
    assert!(store
        .expire_queued_messages_at(&"2026-06-20T00:00:04Z".to_string())
        .unwrap()
        .is_empty());

    let expired = store
        .expire_queued_messages_at(&"2026-06-20T00:00:06Z".to_string())
        .unwrap();

    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].state, QueuedMessageState::Expired);
    assert!(store
        .load_queued_messages(&QueuedMessageFilter {
            state: Some(QueuedMessageState::Pending),
            owner_session_id: Some(SessionId::new("session-alpha")),
            owner_agent_id: None,
            limit: None,
        })
        .unwrap()
        .is_empty());
    let expired_query = store
        .load_queued_messages(&QueuedMessageFilter {
            state: Some(QueuedMessageState::Expired),
            owner_session_id: None,
            owner_agent_id: Some(AgentId::new("agent-alpha")),
            limit: None,
        })
        .unwrap();
    assert_eq!(expired_query.len(), 1);
    assert_eq!(
        expired_query[0].state_reason.as_deref(),
        Some("ttl_expired")
    );
    assert_eq!(
        store
            .runtime_summary(&RuntimeCounterScope::Session(SessionId::new(
                "session-alpha"
            )))
            .unwrap()
            .queue_expirations,
        1
    );
    let search = store
        .search_runtime(&RuntimeSearchFilter {
            query: "queue".to_string(),
            row_type: Some(RuntimeSearchRowType::QueueMessage),
            session_id: Some(SessionId::new("session-alpha")),
            agent_id: Some(AgentId::new("agent-alpha")),
            instance_id: None,
            task_id: None,
            event_kind: None,
            recorded_after: None,
            recorded_before: None,
            limit: Some(10),
        })
        .unwrap();
    assert_eq!(search.len(), 1);
    assert_eq!(search[0].row_type, RuntimeSearchRowType::QueueMessage);
    assert_eq!(store.count_rows("queued_messages").unwrap(), 1);

    remove_temp_db(&db_path);
}

#[test]
fn runtime_state_query_apis_filter_and_page_without_raw_sql() {
    let db_path = temp_db_path("runtime-query-api");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    let alpha_config = sample_session_config();
    let alpha = sample_session_state();
    let beta_config = SessionConfig {
        session_id: SessionId::new("session-beta"),
        agent_id: AgentId::new("agent-beta"),
        profile_id: ProfileId::new("review-profile"),
        kind: SessionKind::Worker,
        delegation: None,
        resource_limits: sample_resource_limits(),
        tool_profile: sample_tool_profile(),
        history_window: None,
    };
    let beta = SessionState {
        handle: SessionHandle::new(2),
        session_id: beta_config.session_id.clone(),
        agent_id: beta_config.agent_id.clone(),
        profile_id: beta_config.profile_id.clone(),
        kind: beta_config.kind.clone(),
        delegation: None,
        resource_limits: beta_config.resource_limits.clone(),
        tool_profile: beta_config.tool_profile.clone(),
        history_window: beta_config.history_window.clone(),
        inference_overrides: Default::default(),
        status: SessionStatus::Idle,
        brain_turn_count: 0,
        created_at: "2026-06-20T00:01:00Z".to_string(),
        last_active_at: "2026-06-20T00:01:00Z".to_string(),
    };

    store
        .save_session_with_config(&alpha, &alpha_config)
        .unwrap();
    store.save_session_with_config(&beta, &beta_config).unwrap();
    store
        .save_worker_run_requested(&WorkerRunRecord {
            run_id: RunId::new("alpha-wake:0"),
            parent_session_id: alpha.session_id.clone(),
            delegated_session_id: Some(SessionId::new("delegated-alpha")),
            parent_agent_id: Some(alpha.agent_id.clone()),
            profile_id: ProfileId::new("coder-profile"),
            task_id: Some(TaskId::new("2876")),
            status: WorkerRunStatus::Requested,
            created_at: "2026-06-20T00:02:00Z".to_string(),
            last_updated_at: "2026-06-20T00:02:00Z".to_string(),
            source_wake_id: "alpha-wake".to_string(),
            source_action_index: 0,
            delegation_correlation_id: Some("query-run".to_string()),
            parent_consumption: ParentConsumptionPolicy::AwaitCompletion,
            fan_out_group_id: None,
            fan_out_max_concurrency: None,
            fan_out_failure_policy: FanOutFailurePolicy::FailSoft,
            worker_pool_work_item_id: None,
            worker_pool_lease_id: None,
            worker_pool_member_id: None,
            worker_pool_claim_token: None,
        })
        .unwrap();
    store
        .save_event(
            1,
            &CoreEvent::AgentMessageRouted {
                message: AgentMessage {
                    from: alpha.agent_id.clone(),
                    to: beta.agent_id.clone(),
                    body: "first query message".to_string(),
                    correlation_id: Some("query-corr".to_string()),
                    projection: None,
                },
            },
        )
        .unwrap();
    store
        .save_event(
            2,
            &CoreEvent::AgentMessageRouted {
                message: AgentMessage {
                    from: beta.agent_id.clone(),
                    to: alpha.agent_id.clone(),
                    body: "second query message".to_string(),
                    correlation_id: Some("query-corr".to_string()),
                    projection: None,
                },
            },
        )
        .unwrap();
    store
        .save_event(
            3,
            &CoreEvent::CompletionPacketDelivered {
                packet: CompletionPacket {
                    session_id: alpha.session_id.clone(),
                    status: rusty_crew_core_protocol::CompletionStatus::Completed,
                    summary: "query completion".to_string(),
                },
            },
        )
        .unwrap();
    store
        .save_event(
            4,
            &CoreEvent::BrainWakeRequested {
                session_id: alpha.session_id.clone(),
            },
        )
        .unwrap();

    assert_eq!(
        store
            .query_sessions(&SessionQuery {
                kind: Some(SessionKind::Full),
                page: Some(QueryPage {
                    limit: Some(10),
                    offset: Some(0),
                }),
                ..SessionQuery::default()
            })
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        store
            .query_agent_instances(&AgentInstanceQuery {
                agent_id: Some(AgentId::new("agent-beta")),
                ..AgentInstanceQuery::default()
            })
            .unwrap()[0]
            .instance_id,
        AgentInstanceId::new("instance:session-beta")
    );
    assert_eq!(
        store
            .query_agent_messages(&AgentMessageQuery {
                agent_id: Some(AgentId::new("agent-alpha")),
                correlation_id: Some("query-corr".to_string()),
                page: Some(QueryPage {
                    limit: Some(1),
                    offset: Some(1),
                }),
            })
            .unwrap()[0]
            .sequence,
        2
    );
    assert_eq!(
        store
            .query_completion_packets(&CompletionPacketQuery {
                session_id: Some(SessionId::new("session-alpha")),
                status: Some(rusty_crew_core_protocol::CompletionStatus::Completed),
                page: None,
            })
            .unwrap()[0]
            .packet
            .summary,
        "query completion"
    );
    assert_eq!(
        store
            .query_worker_runs(&WorkerRunQuery {
                parent_session_id: Some(SessionId::new("session-alpha")),
                terminal: Some(false),
                ..WorkerRunQuery::default()
            })
            .unwrap()[0]
            .run_id,
        RunId::new("alpha-wake:0")
    );
    assert_eq!(
        store
            .query_runtime_counters(&RuntimeCounterQuery {
                scope: Some(RuntimeCounterScope::Runtime),
                counter_name: Some(COUNTER_MESSAGES.to_string()),
                page: None,
            })
            .unwrap()[0]
            .value,
        2
    );

    remove_temp_db(&db_path);
}

#[test]
fn context_compaction_artifacts_preserve_raw_message_history() {
    let db_path = temp_db_path("context-compaction-artifacts");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    let session_id = SessionId::new("session-1");
    let slot_id = MessageSlotId::new("slot-context");
    let variant_id = MessageVariantId::new("variant-context-primary");
    store
        .save_message_slot(&MessageSlotWrite {
            slot_id: slot_id.clone(),
            session_id: session_id.clone(),
            primary_variant_id: variant_id.clone(),
            active_variant_id: None,
            metadata_json: json!({"fixture": "context_compaction"}),
            created_at: "2026-06-30T00:00:00Z".to_string(),
            updated_at: "2026-06-30T00:00:00Z".to_string(),
        })
        .unwrap();
    store
        .save_message_variant(&variant_write(
            &slot_id,
            &variant_id,
            MessageVariantSource::Primary,
            0,
            "message-context",
            "raw context compaction source text",
        ))
        .unwrap();
    let slots_before = store.count_rows("message_slots").unwrap();
    let variants_before = store.count_rows("message_variants").unwrap();

    let artifact = ContextCompactionArtifact {
        artifact_id: "artifact_context_one".to_string(),
        session_id: session_id.clone(),
        branch_id: None,
        strategy_id: "rolling_summary_compaction".to_string(),
        strategy_revision: None,
        logical_turn_id: None,
        execution_epoch_id: None,
        source_projection_fingerprint: None,
        trigger: None,
        before_tokens: None,
        after_tokens: None,
        preserved_item_count: None,
        excised_item_count: None,
        intent_key: None,
        terminal_status: None,
        provider_chain_action: None,
        source_refs_json: json!({
            "message_slot_ids": [slot_id.0.as_str()],
            "message_variant_ids": [variant_id.0.as_str()],
            "cursor_range": {"from": "session-1:0", "to": "session-1:1"}
        }),
        provider_metadata_json: json!({
            "provider_alias": "deepseek-flash",
            "model_id": "deepseek-chat"
        }),
        estimate_before_json: json!({
            "estimator_id": "fallback_chars_words_v1",
            "estimated_prompt_tokens": 85000
        }),
        estimate_after_json: Some(json!({
            "estimated_prompt_tokens": 24000
        })),
        summary_text: "The conversation discussed durable compaction provenance.".to_string(),
        enters_future_context: true,
        context_policy: "summary_context".to_string(),
        metadata_json: json!({"created_by": "test"}),
        created_at: "2026-06-30T00:01:00Z".to_string(),
        updated_at: "2026-06-30T00:01:00Z".to_string(),
    };
    let saved = store.save_context_compaction_artifact(&artifact).unwrap();
    assert_eq!(saved.artifact_id, "artifact_context_one");
    assert_eq!(saved.strategy_id, "rolling_summary_compaction");

    let latest = store
        .list_context_compaction_artifacts(&ContextCompactionArtifactQuery {
            session_id: Some(session_id.clone()),
            branch_id: None,
            strategy_id: Some("rolling_summary_compaction".to_string()),
            enters_future_context: Some(true),
            latest_only: true,
            limit: None,
            offset: None,
        })
        .unwrap();
    assert_eq!(latest, vec![artifact]);
    assert_eq!(store.count_rows("message_slots").unwrap(), slots_before);
    assert_eq!(
        store.count_rows("message_variants").unwrap(),
        variants_before
    );
    let slots_after = store
        .query_message_slots(&MessageSlotQuery {
            session_id: Some(session_id),
            include_alternates: false,
            page: None,
        })
        .unwrap();
    assert_eq!(
        slots_after[0].primary.message.body,
        "raw context compaction source text"
    );
    drop(store);

    let reopened = CoordinationStore::open_file(&db_path).unwrap();
    let reopened_artifacts = reopened
        .list_context_compaction_artifacts(&ContextCompactionArtifactQuery {
            session_id: Some(SessionId::new("session-1")),
            branch_id: None,
            strategy_id: None,
            enters_future_context: None,
            latest_only: true,
            limit: None,
            offset: None,
        })
        .unwrap();
    assert_eq!(reopened_artifacts.len(), 1);
    assert_eq!(reopened_artifacts[0].artifact_id, "artifact_context_one");
    let reopened_slots = reopened
        .query_message_slots(&MessageSlotQuery {
            session_id: Some(SessionId::new("session-1")),
            include_alternates: false,
            page: None,
        })
        .unwrap();
    assert_eq!(
        reopened_slots[0].primary.message.body,
        "raw context compaction source text"
    );

    remove_temp_db(&db_path);
}

#[test]
fn context_compaction_artifacts_are_idempotent_under_concurrent_conflicting_writes() {
    let db_path = temp_db_path("context-compaction-concurrent");
    let first_store = CoordinationStore::open_file(&db_path).unwrap();
    let second_store = CoordinationStore::open_file(&db_path).unwrap();
    let ready = Arc::new(Barrier::new(3));
    let first_artifact = ContextCompactionArtifact {
        artifact_id: "artifact_context_concurrent".to_string(),
        session_id: SessionId::new("session-concurrent"),
        branch_id: None,
        strategy_id: "rolling_summary_compaction".to_string(),
        strategy_revision: Some("1".to_string()),
        logical_turn_id: Some("turn-concurrent".to_string()),
        execution_epoch_id: Some("epoch-1".to_string()),
        source_projection_fingerprint: Some("fp-concurrent".to_string()),
        trigger: Some("auto_threshold".to_string()),
        before_tokens: Some(90000),
        after_tokens: Some(24000),
        preserved_item_count: Some(5),
        excised_item_count: Some(5),
        intent_key: Some("intent-concurrent".to_string()),
        terminal_status: Some("completed".to_string()),
        provider_chain_action: Some("rebuild_replay_after_compaction".to_string()),
        source_refs_json: json!({"source": "first"}),
        provider_metadata_json: json!({"provider_alias": "fixture-provider"}),
        estimate_before_json: json!({"input_tokens": 90_000}),
        estimate_after_json: Some(json!({"input_tokens": 24_000})),
        summary_text: "first concurrent candidate".to_string(),
        enters_future_context: true,
        context_policy: "summary_context".to_string(),
        metadata_json: json!({"fixture": "concurrent"}),
        created_at: "2026-06-30T00:00:00Z".to_string(),
        updated_at: "2026-06-30T00:00:01Z".to_string(),
    };
    let mut second_artifact = first_artifact.clone();
    second_artifact.source_refs_json = json!({"source": "second"});
    second_artifact.summary_text = "second concurrent candidate".to_string();
    second_artifact.updated_at = "2026-06-30T00:00:02Z".to_string();

    let first_ready = Arc::clone(&ready);
    let first_thread = thread::spawn(move || {
        first_ready.wait();
        first_store.save_context_compaction_artifact(&first_artifact)
    });
    let second_ready = Arc::clone(&ready);
    let second_thread = thread::spawn(move || {
        second_ready.wait();
        second_store.save_context_compaction_artifact(&second_artifact)
    });
    ready.wait();

    assert!(first_thread.join().unwrap().is_ok());
    assert!(second_thread.join().unwrap().is_ok());

    let reader = CoordinationStore::open_file(&db_path).unwrap();
    let artifacts = reader
        .list_context_compaction_artifacts(&ContextCompactionArtifactQuery {
            session_id: Some(SessionId::new("session-concurrent")),
            branch_id: None,
            strategy_id: None,
            enters_future_context: None,
            latest_only: false,
            limit: None,
            offset: None,
        })
        .unwrap();
    assert_eq!(
        artifacts.len(),
        1,
        "concurrent writes must retain one artifact"
    );
    assert!(
        ["first concurrent candidate", "second concurrent candidate"]
            .contains(&artifacts[0].summary_text.as_str()),
        "the final artifact must be one complete candidate, not a partial merge"
    );

    drop(reader);
    remove_temp_db(&db_path);
}

#[test]
fn message_slots_persist_variants_and_active_selection_conflicts() {
    let db_path = temp_db_path("message-slots");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    let now = "2026-06-25T03:00:00Z".to_string();
    let slot_id = MessageSlotId::new("slot-1");
    let primary_variant_id = MessageVariantId::new("variant-primary");
    store
        .save_message_slot(&MessageSlotWrite {
            slot_id: slot_id.clone(),
            session_id: SessionId::new("session-1"),
            primary_variant_id: primary_variant_id.clone(),
            active_variant_id: None,
            metadata_json: json!({"origin": "test"}),
            created_at: now.clone(),
            updated_at: now.clone(),
        })
        .unwrap();
    store
        .save_message_variant(&variant_write(
            &slot_id,
            &primary_variant_id,
            MessageVariantSource::Primary,
            0,
            "message-primary",
            "primary body",
        ))
        .unwrap();
    store
        .save_message_variant(&variant_write(
            &slot_id,
            &MessageVariantId::new("variant-a"),
            MessageVariantSource::Alternate,
            1,
            "message-a",
            "alternate a",
        ))
        .unwrap();
    store
        .save_message_variant(&variant_write(
            &slot_id,
            &MessageVariantId::new("variant-b"),
            MessageVariantSource::Alternate,
            2,
            "message-b",
            "alternate b",
        ))
        .unwrap();

    let lazy = store
        .query_message_slots(&MessageSlotQuery {
            session_id: Some(SessionId::new("session-1")),
            include_alternates: false,
            page: None,
        })
        .unwrap();
    assert_eq!(lazy.len(), 1);
    assert_eq!(lazy[0].primary.message.body, "primary body");
    assert!(lazy[0].alternates.is_empty());

    let variants = store
        .query_message_variants(&MessageVariantQuery {
            slot_id: Some(slot_id.clone()),
            include_deleted: false,
            page: None,
        })
        .unwrap();
    assert_eq!(
        variants
            .iter()
            .map(|variant| variant.variant_id.0.as_str())
            .collect::<Vec<_>>(),
        vec!["variant-primary", "variant-a", "variant-b"]
    );
    assert_eq!(variants[0].message.blocks[0].kind, "text");

    let selected = store
        .select_active_message_variant(&SelectActiveVariantRequest {
            slot_id: slot_id.clone(),
            active_variant_id: Some(MessageVariantId::new("variant-a")),
            expected: ActiveVariantExpectation::Primary,
            updated_at: "2026-06-25T03:01:00Z".to_string(),
        })
        .unwrap();
    assert!(selected.conflict.is_none());
    assert_eq!(
        selected.slot.active_variant_id,
        Some(MessageVariantId::new("variant-a"))
    );

    let conflict = store
        .select_active_message_variant(&SelectActiveVariantRequest {
            slot_id: slot_id.clone(),
            active_variant_id: Some(MessageVariantId::new("variant-b")),
            expected: ActiveVariantExpectation::Primary,
            updated_at: "2026-06-25T03:02:00Z".to_string(),
        })
        .unwrap();
    assert_eq!(
        conflict.conflict.unwrap().actual,
        Some(MessageVariantId::new("variant-a"))
    );

    store
        .reorder_message_variants(
            &slot_id,
            &[
                MessageVariantId::new("variant-b"),
                MessageVariantId::new("variant-a"),
            ],
            &"2026-06-25T03:03:00Z".to_string(),
        )
        .unwrap();
    let reordered = store
        .query_message_variants(&MessageVariantQuery {
            slot_id: Some(slot_id.clone()),
            include_deleted: false,
            page: None,
        })
        .unwrap();
    assert_eq!(
        reordered
            .iter()
            .map(|variant| variant.variant_id.0.as_str())
            .collect::<Vec<_>>(),
        vec!["variant-primary", "variant-b", "variant-a"]
    );

    let deleted = store
        .delete_message_variant(
            &slot_id,
            &MessageVariantId::new("variant-a"),
            &"2026-06-25T03:04:00Z".to_string(),
        )
        .unwrap();
    assert_eq!(deleted.active_variant_id, None);
    assert_eq!(deleted.alternates.len(), 1);
    assert_eq!(
        deleted.alternates[0].variant_id,
        MessageVariantId::new("variant-b")
    );

    remove_temp_db(&db_path);
}

#[test]
fn conversation_tree_branches_snapshots_and_jump_targets_persist() {
    let db_path = temp_db_path("conversation-tree");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    let now = "2026-06-25T04:00:00Z".to_string();
    let session_id = SessionId::new("session-1");
    let root_branch = ConversationBranchId::new("branch-root");
    let child_branch = ConversationBranchId::new("branch-child");
    let slot_id = MessageSlotId::new("slot-tree");
    let primary_variant_id = MessageVariantId::new("variant-tree-primary");
    let root_message_id = MessageId::new("message-root");
    let child_message_id = MessageId::new("message-child");

    store
        .save_conversation_branch(&ConversationBranchWrite {
            branch_id: root_branch.clone(),
            session_id: session_id.clone(),
            parent_branch_id: None,
            parent_message_id: None,
            origin_message_id: None,
            head_message_id: Some(root_message_id.clone()),
            label: Some("Root".to_string()),
            metadata_json: json!({"kind": "default"}),
            created_at: now.clone(),
            updated_at: now.clone(),
        })
        .unwrap();
    store
        .save_message_slot(&MessageSlotWrite {
            slot_id: slot_id.clone(),
            session_id: session_id.clone(),
            primary_variant_id: primary_variant_id.clone(),
            active_variant_id: None,
            metadata_json: json!({}),
            created_at: now.clone(),
            updated_at: now.clone(),
        })
        .unwrap();
    let mut variant = variant_write(
        &slot_id,
        &primary_variant_id,
        MessageVariantSource::Primary,
        0,
        &root_message_id.0,
        "root body",
    );
    variant.message.branch_id = Some(root_branch.clone());
    store.save_message_variant(&variant).unwrap();

    store
        .save_conversation_branch(&ConversationBranchWrite {
            branch_id: child_branch.clone(),
            session_id: session_id.clone(),
            parent_branch_id: Some(root_branch.clone()),
            parent_message_id: Some(root_message_id.clone()),
            origin_message_id: Some(root_message_id.clone()),
            head_message_id: Some(child_message_id.clone()),
            label: Some("Alternative".to_string()),
            metadata_json: json!({"reason": "alternate"}),
            created_at: "2026-06-25T04:01:00Z".to_string(),
            updated_at: "2026-06-25T04:01:00Z".to_string(),
        })
        .unwrap();

    let branches = store
        .query_conversation_branches(&ConversationBranchQuery {
            session_id: Some(session_id.clone()),
            parent_branch_id: None,
            page: None,
        })
        .unwrap();
    assert_eq!(branches.len(), 2);
    assert_eq!(branches[1].parent_branch_id, Some(root_branch.clone()));

    let selected = store
        .select_active_conversation_branch(&SelectActiveBranchRequest {
            session_id: session_id.clone(),
            active_branch_id: Some(child_branch.clone()),
            expected: ActiveBranchExpectation::None,
            updated_at: "2026-06-25T04:02:00Z".to_string(),
        })
        .unwrap();
    assert!(selected.conflict.is_none());
    assert_eq!(selected.state.active_branch_id, Some(child_branch.clone()));

    let conflict = store
        .select_active_conversation_branch(&SelectActiveBranchRequest {
            session_id: session_id.clone(),
            active_branch_id: Some(root_branch.clone()),
            expected: ActiveBranchExpectation::None,
            updated_at: "2026-06-25T04:03:00Z".to_string(),
        })
        .unwrap();
    assert_eq!(
        conflict.conflict.unwrap().actual,
        Some(child_branch.clone())
    );

    let head_conflict = store
        .update_conversation_branch_head(&UpdateBranchHeadRequest {
            branch_id: child_branch.clone(),
            head_message_id: Some(root_message_id.clone()),
            expected: BranchHeadExpectation::None,
            updated_at: "2026-06-25T04:04:00Z".to_string(),
        })
        .unwrap();
    assert_eq!(
        head_conflict.conflict.unwrap().actual,
        Some(child_message_id.clone())
    );

    let snapshot = store
        .save_conversation_snapshot(&ConversationSnapshotWrite {
            snapshot_id: ConversationSnapshotId::new("snapshot-1"),
            session_id: session_id.clone(),
            branch_id: Some(child_branch.clone()),
            message_id: Some(root_message_id.clone()),
            cursor: Some("session-1:42".to_string()),
            label: Some("Before alternate".to_string()),
            summary: Some("Checkpoint summary".to_string()),
            source: ConversationSnapshotSource::User,
            metadata_json: json!({"from": "test"}),
            created_at: "2026-06-25T04:05:00Z".to_string(),
            updated_at: "2026-06-25T04:05:00Z".to_string(),
        })
        .unwrap();
    assert_eq!(snapshot.branch_id, Some(child_branch.clone()));

    let snapshots = store
        .query_conversation_snapshots(&ConversationSnapshotQuery {
            session_id: Some(session_id.clone()),
            branch_id: None,
            message_id: Some(root_message_id.clone()),
            page: None,
        })
        .unwrap();
    assert_eq!(snapshots.len(), 1);

    let branch_jump = store
        .resolve_conversation_jump(&ConversationJumpRequest {
            session_id: session_id.clone(),
            target: ConversationJumpTarget::Branch {
                branch_id: child_branch.clone(),
            },
        })
        .unwrap();
    assert_eq!(branch_jump.message_id, Some(child_message_id.clone()));

    let snapshot_jump = store
        .resolve_conversation_jump(&ConversationJumpRequest {
            session_id: session_id.clone(),
            target: ConversationJumpTarget::Snapshot {
                snapshot_id: ConversationSnapshotId::new("snapshot-1"),
            },
        })
        .unwrap();
    assert_eq!(snapshot_jump.cursor, Some("session-1:42".to_string()));

    let message_jump = store
        .resolve_conversation_jump(&ConversationJumpRequest {
            session_id,
            target: ConversationJumpTarget::Message {
                message_id: root_message_id,
            },
        })
        .unwrap();
    assert_eq!(message_jump.branch_id, Some(root_branch));

    remove_temp_db(&db_path);
}

#[test]
fn attachments_and_data_bank_scopes_persist_across_reopen() {
    let db_path = temp_db_path("attachments-data-bank");
    let session_id = SessionId::new("session-attachment");
    let scope_id = DataBankScopeId::new("scope-reference");
    let attachment_id = AttachmentId::new("attachment-guide");
    let message_id = MessageId::new("message-guide");

    {
        let store = CoordinationStore::open_file(&db_path).unwrap();
        store
            .save_data_bank_scope(&DataBankScopeWrite {
                scope_id: scope_id.clone(),
                session_id: session_id.clone(),
                status: DataBankScopeStatus::Active,
                label: Some("Reference".to_string()),
                description: Some("Reusable files".to_string()),
                metadata_json: json!({"source": "test"}),
                created_at: "2026-06-25T05:00:00Z".to_string(),
                updated_at: "2026-06-25T05:00:00Z".to_string(),
            })
            .unwrap();
        let saved = store
            .save_attachment(&AttachmentWrite {
                attachment_id: attachment_id.clone(),
                session_id: session_id.clone(),
                status: AttachmentStatus::Active,
                filename: "guide.txt".to_string(),
                mime_type: "text/plain".to_string(),
                byte_size: 42,
                storage_url: None,
                download_url: Some("/download/guide".to_string()),
                thumbnail_url: None,
                extracted_text: Some("hello attachment".to_string()),
                extracted_text_truncated: false,
                metadata_json: json!({"kind": "reference"}),
                created_at: "2026-06-25T05:01:00Z".to_string(),
                updated_at: "2026-06-25T05:01:00Z".to_string(),
                expires_at: None,
                link: Some(AttachmentLinkWrite {
                    link_id: AttachmentLinkId::new("attachment-link-guide"),
                    attachment_id: attachment_id.clone(),
                    session_id: session_id.clone(),
                    message_id: Some(message_id.clone()),
                    block_id: None,
                    scope_id: Some(scope_id.clone()),
                    metadata_json: json!({"linked_by": "test"}),
                    created_at: "2026-06-25T05:01:00Z".to_string(),
                }),
            })
            .unwrap();
        assert_eq!(saved.links.len(), 1);
    }

    let store = CoordinationStore::open_file(&db_path).unwrap();
    let by_message = store
        .query_attachments(&AttachmentQuery {
            session_id: Some(session_id.clone()),
            message_id: Some(message_id),
            scope_id: None,
            include_removed: false,
            ..AttachmentQuery::default()
        })
        .unwrap();
    assert_eq!(by_message.len(), 1);
    assert_eq!(&by_message[0].attachment_id, &attachment_id);
    assert_eq!(by_message[0].links[0].scope_id, Some(scope_id.clone()));

    let by_scope = store
        .query_attachments(&AttachmentQuery {
            session_id: Some(session_id.clone()),
            message_id: None,
            scope_id: Some(scope_id.clone()),
            include_removed: false,
            ..AttachmentQuery::default()
        })
        .unwrap();
    assert_eq!(by_scope.len(), 1);

    let scopes = store
        .query_data_bank_scopes(&DataBankScopeQuery {
            session_id: Some(session_id.clone()),
            include_removed: false,
            ..DataBankScopeQuery::default()
        })
        .unwrap();
    assert_eq!(scopes.len(), 1);
    assert_eq!(&scopes[0].scope_id, &scope_id);

    let removed_attachment = store
        .remove_attachment(
            &AttachmentId::new("attachment-guide"),
            &"2026-06-25T05:02:00Z".to_string(),
        )
        .unwrap();
    assert_eq!(removed_attachment.status, AttachmentStatus::Removed);
    let active_after_remove = store
        .query_attachments(&AttachmentQuery {
            session_id: Some(session_id.clone()),
            include_removed: false,
            ..AttachmentQuery::default()
        })
        .unwrap();
    assert!(active_after_remove.is_empty());
    let removed_scope = store
        .remove_data_bank_scope(&scope_id, &"2026-06-25T05:03:00Z".to_string())
        .unwrap();
    assert_eq!(removed_scope.status, DataBankScopeStatus::Removed);

    let removed_records = store
        .query_attachments(&AttachmentQuery {
            session_id: Some(session_id.clone()),
            include_removed: true,
            ..AttachmentQuery::default()
        })
        .unwrap();
    assert_eq!(removed_records.len(), 1);
    let removed_scopes = store
        .query_data_bank_scopes(&DataBankScopeQuery {
            session_id: Some(session_id),
            include_removed: true,
            ..DataBankScopeQuery::default()
        })
        .unwrap();
    assert_eq!(removed_scopes.len(), 1);

    remove_temp_db(&db_path);
}

#[test]
fn maintenance_guardrails_cover_queue_retention_size_and_hot_indexes() {
    let db_path = temp_db_path("maintenance-guardrails");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    let mut sequence = 1_u64;
    for index in 0..30 {
        let session_id = SessionId::new(format!("session-{index:02}"));
        let agent_id = AgentId::new(format!("agent-{index:02}"));
        let profile_id = ProfileId::new(format!("profile-{}", index % 3));
        let config = SessionConfig {
            session_id: session_id.clone(),
            agent_id: agent_id.clone(),
            profile_id: profile_id.clone(),
            kind: SessionKind::Full,
            delegation: None,
            resource_limits: sample_resource_limits(),
            tool_profile: sample_tool_profile(),
            history_window: None,
        };
        store
            .save_session_with_config(
                &SessionState {
                    handle: SessionHandle::new((index + 1) as u64),
                    session_id: session_id.clone(),
                    agent_id: agent_id.clone(),
                    profile_id,
                    kind: SessionKind::Full,
                    delegation: None,
                    resource_limits: sample_resource_limits(),
                    tool_profile: sample_tool_profile(),
                    history_window: None,
                    inference_overrides: Default::default(),
                    status: SessionStatus::Idle,
                    brain_turn_count: 0,
                    created_at: format!("2026-06-20T00:{index:02}:00Z"),
                    last_active_at: format!("2026-06-20T00:{index:02}:00Z"),
                },
                &config,
            )
            .unwrap();
        store
            .save_worker_run_requested(&WorkerRunRecord {
                run_id: RunId::new(format!("run-{index:02}")),
                parent_session_id: session_id.clone(),
                delegated_session_id: Some(SessionId::new(format!("delegated-{index:02}"))),
                parent_agent_id: Some(agent_id.clone()),
                profile_id: ProfileId::new("delegated-profile"),
                task_id: Some(TaskId::new(format!("task-{index:02}"))),
                status: WorkerRunStatus::Running,
                created_at: format!("2026-06-20T01:{index:02}:00Z"),
                last_updated_at: format!("2026-06-20T01:{index:02}:00Z"),
                source_wake_id: format!("wake-{index:02}"),
                source_action_index: index,
                delegation_correlation_id: Some("scale-corr".to_string()),
                parent_consumption: ParentConsumptionPolicy::AwaitCompletion,
                fan_out_group_id: Some("scale-group".to_string()),
                fan_out_max_concurrency: Some(4),
                fan_out_failure_policy: FanOutFailurePolicy::FailSoft,
                worker_pool_work_item_id: None,
                worker_pool_lease_id: None,
                worker_pool_member_id: None,
                worker_pool_claim_token: None,
            })
            .unwrap();

        for message_index in 0..12 {
            store
                .save_event(
                    sequence,
                    &CoreEvent::AgentMessageRouted {
                        message: AgentMessage {
                            from: agent_id.clone(),
                            to: AgentId::new(format!("agent-{:02}", (index + 1) % 30)),
                            body: format!("scale message {index}-{message_index}"),
                            correlation_id: Some("corr-alpha".to_string()),
                            projection: None,
                        },
                    },
                )
                .unwrap();
            sequence += 1;
        }
    }

    for index in 0..5 {
        store
            .save_queued_message(&QueuedMessageRecord {
                message_id: format!("expired-queue-{index}"),
                owner_session_id: Some(SessionId::new("session-00")),
                owner_agent_id: AgentId::new("agent-00"),
                message: AgentMessage {
                    from: AgentId::new("operator"),
                    to: AgentId::new("agent-00"),
                    body: format!("expired queue message {index}"),
                    correlation_id: Some("queue-scale".to_string()),
                    projection: None,
                },
                source_sequence: Some(sequence + index as u64),
                enqueued_at: "2026-06-20T02:00:00Z".to_string(),
                expires_at: "2026-06-20T02:00:01Z".to_string(),
                ttl_ms: 1_000,
                delivery_attempts: 0,
                state: QueuedMessageState::Pending,
                terminal_at: None,
                state_reason: None,
            })
            .unwrap();
    }
    store
        .save_queued_message(&QueuedMessageRecord {
            message_id: "future-queue".to_string(),
            owner_session_id: Some(SessionId::new("session-00")),
            owner_agent_id: AgentId::new("agent-00"),
            message: AgentMessage {
                from: AgentId::new("operator"),
                to: AgentId::new("agent-00"),
                body: "fresh queue message".to_string(),
                correlation_id: Some("queue-scale".to_string()),
                projection: None,
            },
            source_sequence: Some(sequence + 10),
            enqueued_at: "2026-06-20T02:00:00Z".to_string(),
            expires_at: "2026-06-20T02:10:00Z".to_string(),
            ttl_ms: 600_000,
            delivery_attempts: 0,
            state: QueuedMessageState::Pending,
            terminal_at: None,
            state_reason: None,
        })
        .unwrap();

    let report = store
        .run_maintenance(&RuntimeMaintenancePolicy {
            expire_queued_messages_at: Some("2026-06-20T02:00:02Z".to_string()),
            purge_terminal_queued_messages_before: Some("2026-06-20T02:00:03Z".to_string()),
            expire_provider_wire_states_at: None,
            run_wal_checkpoint: true,
            run_optimize: true,
            ..RuntimeMaintenancePolicy::default()
        })
        .unwrap();

    assert_eq!(report.expired_queue_messages, 5);
    assert_eq!(report.purged_terminal_queue_messages, 5);
    assert!(report.optimize_ran);
    assert!(report.wal_checkpoint_ran);
    assert!(report.size_before.page_size_bytes > 0);
    assert!(report.size_after.database_bytes > 0);
    assert_eq!(store.count_rows("queued_messages").unwrap(), 1);
    assert_eq!(
        store
            .load_queued_messages(&QueuedMessageFilter {
                state: Some(QueuedMessageState::Pending),
                owner_session_id: None,
                owner_agent_id: Some(AgentId::new("agent-00")),
                limit: None,
            })
            .unwrap()[0]
            .message_id,
        "future-queue"
    );
    assert_eq!(
        store
            .search_runtime(&RuntimeSearchFilter {
                query: "expired queue message".to_string(),
                row_type: Some(RuntimeSearchRowType::QueueMessage),
                session_id: Some(SessionId::new("session-00")),
                agent_id: Some(AgentId::new("agent-00")),
                instance_id: None,
                task_id: None,
                event_kind: None,
                recorded_after: None,
                recorded_before: None,
                limit: Some(10),
            })
            .unwrap()
            .len(),
        0
    );
    let checks = store.hot_query_plan_checks().unwrap();
    assert!(
        checks.iter().all(|check| check.uses_index),
        "hot query plan lost index coverage: {checks:?}"
    );

    remove_temp_db(&db_path);
}

#[test]
fn sqlite_and_sql_literals_do_not_leak_outside_persistence_crate() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = find_workspace_root(manifest_dir);
    let mut violations = Vec::new();
    scan_source_tree(workspace_root, workspace_root, &mut violations);

    assert!(
        violations.is_empty(),
        "persistence backend leaked outside core-persistence:\n{}",
        violations.join("\n")
    );
}

fn find_workspace_root(start: &Path) -> &Path {
    start
        .ancestors()
        .find(|candidate| {
            fs::read_to_string(candidate.join("Cargo.toml"))
                .is_ok_and(|content| content.lines().any(|line| line.trim() == "[workspace]"))
        })
        .expect("workspace Cargo.toml")
}

fn scan_source_tree(workspace_root: &Path, root: &Path, violations: &mut Vec<String>) {
    for entry in fs::read_dir(root).expect("scan root") {
        let entry = entry.expect("read dir entry");
        let path = entry.path();
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if file_name == "target" || file_name == "node_modules" || file_name == ".git" {
            continue;
        }
        if path.is_dir() {
            scan_source_tree(workspace_root, &path, violations);
            continue;
        }
        if !matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("rs" | "ts")
        ) {
            continue;
        }
        if path.starts_with(workspace_root.join("crates/core/core-persistence")) {
            continue;
        }
        let content = fs::read_to_string(&path).expect("read source file");
        if contains_persistence_backend_detail(&content) {
            violations.push(
                path.strip_prefix(workspace_root)
                    .unwrap_or(&path)
                    .display()
                    .to_string(),
            );
        }
    }
}

fn contains_persistence_backend_detail(content: &str) -> bool {
    const NEEDLES: &[&str] = &[
        "rusqlite",
        "CREATE TABLE",
        "ALTER TABLE",
        "PRAGMA ",
        "SELECT ",
        "INSERT ",
        "UPDATE ",
        "DELETE ",
    ];
    NEEDLES.iter().any(|needle| content.contains(needle))
}

#[test]
fn worker_pool_member_registration_claim_and_completion_round_trip() {
    let db_path = temp_db_path("worker-pool-round-trip");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    store
        .upsert_worker_pool_member(&sample_worker_pool_member(
            "member-a",
            "worker-profile",
            WorkerPoolMemberStatus::Available,
            1,
            0,
            "2026-06-30T00:00:00Z",
        ))
        .unwrap();
    assert!(store
        .heartbeat_worker_pool_member(
            "member-a",
            WorkerPoolMemberStatus::Available,
            &"2026-06-30T00:00:10Z".to_string(),
        )
        .unwrap());
    store
        .create_worker_pool_work_item(&sample_worker_pool_work_item(
            "work-b",
            Some("worker-profile"),
            20,
            "2026-06-30T00:00:11Z",
        ))
        .unwrap();
    store
        .create_worker_pool_work_item(&sample_worker_pool_work_item(
            "work-a",
            Some("worker-profile"),
            10,
            "2026-06-30T00:00:12Z",
        ))
        .unwrap();

    let claim = store
        .claim_next_worker_pool_work_item(&WorkerPoolClaimRequest {
            member_id: "member-a".to_string(),
            lease_id: "lease-a".to_string(),
            claim_token: "token-a".to_string(),
            now: "2026-06-30T00:00:13Z".to_string(),
            claim_deadline_at: "2026-06-30T00:10:00Z".to_string(),
            min_heartbeat_at: "2026-06-30T00:00:00Z".to_string(),
        })
        .unwrap()
        .unwrap();
    assert_eq!(claim.work_item.work_item_id, "work-a");
    assert_eq!(claim.work_item.status, WorkerPoolWorkStatus::Claimed);
    assert_eq!(claim.member.active_leases, 1);
    assert_eq!(claim.member.status, WorkerPoolMemberStatus::Busy);

    let no_capacity = store
        .claim_next_worker_pool_work_item(&WorkerPoolClaimRequest {
            member_id: "member-a".to_string(),
            lease_id: "lease-b".to_string(),
            claim_token: "token-b".to_string(),
            now: "2026-06-30T00:00:14Z".to_string(),
            claim_deadline_at: "2026-06-30T00:10:00Z".to_string(),
            min_heartbeat_at: "2026-06-30T00:00:00Z".to_string(),
        })
        .unwrap()
        .unwrap_err();
    assert_eq!(no_capacity, WorkerPoolNoCapacityReason::MemberAtCapacity);

    assert!(store
        .complete_worker_pool_work_item(&WorkerPoolCompletionRequest {
            lease_id: "lease-a".to_string(),
            claim_token: "token-a".to_string(),
            status: WorkerPoolWorkStatus::Completed,
            now: "2026-06-30T00:00:15Z".to_string(),
            summary: Some("done".to_string()),
        })
        .unwrap());
    let member = store.load_worker_pool_member("member-a").unwrap().unwrap();
    assert_eq!(member.active_leases, 0);
    assert_eq!(member.status, WorkerPoolMemberStatus::Available);
    let work = store.load_worker_pool_work_item("work-a").unwrap().unwrap();
    assert_eq!(work.status, WorkerPoolWorkStatus::Completed);
    assert_eq!(work.terminal_summary.as_deref(), Some("done"));

    remove_temp_db(&db_path);
}

#[test]
fn worker_pool_stale_member_cannot_claim() {
    let db_path = temp_db_path("worker-pool-stale-member");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    store
        .upsert_worker_pool_member(&sample_worker_pool_member(
            "member-a",
            "worker-profile",
            WorkerPoolMemberStatus::Available,
            1,
            0,
            "2026-06-30T00:00:00Z",
        ))
        .unwrap();
    store
        .create_worker_pool_work_item(&sample_worker_pool_work_item(
            "work-a",
            Some("worker-profile"),
            10,
            "2026-06-30T00:00:01Z",
        ))
        .unwrap();

    let reason = store
        .claim_next_worker_pool_work_item(&WorkerPoolClaimRequest {
            member_id: "member-a".to_string(),
            lease_id: "lease-a".to_string(),
            claim_token: "token-a".to_string(),
            now: "2026-06-30T00:00:02Z".to_string(),
            claim_deadline_at: "2026-06-30T00:10:00Z".to_string(),
            min_heartbeat_at: "2026-06-30T00:00:01Z".to_string(),
        })
        .unwrap()
        .unwrap_err();
    assert_eq!(reason, WorkerPoolNoCapacityReason::MemberHeartbeatStale);
    assert_eq!(
        store
            .load_worker_pool_work_item("work-a")
            .unwrap()
            .unwrap()
            .status,
        WorkerPoolWorkStatus::Pending
    );

    remove_temp_db(&db_path);
}

#[test]
fn worker_pool_claim_token_fences_terminal_completion() {
    let db_path = temp_db_path("worker-pool-token-fence");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    store
        .upsert_worker_pool_member(&sample_worker_pool_member(
            "member-a",
            "worker-profile",
            WorkerPoolMemberStatus::Available,
            1,
            0,
            "2026-06-30T00:00:00Z",
        ))
        .unwrap();
    store
        .create_worker_pool_work_item(&sample_worker_pool_work_item(
            "work-a",
            Some("worker-profile"),
            10,
            "2026-06-30T00:00:01Z",
        ))
        .unwrap();
    store
        .claim_next_worker_pool_work_item(&WorkerPoolClaimRequest {
            member_id: "member-a".to_string(),
            lease_id: "lease-a".to_string(),
            claim_token: "token-a".to_string(),
            now: "2026-06-30T00:00:02Z".to_string(),
            claim_deadline_at: "2026-06-30T00:10:00Z".to_string(),
            min_heartbeat_at: "2026-06-30T00:00:00Z".to_string(),
        })
        .unwrap()
        .unwrap();

    assert!(!store
        .complete_worker_pool_work_item(&WorkerPoolCompletionRequest {
            lease_id: "lease-a".to_string(),
            claim_token: "wrong-token".to_string(),
            status: WorkerPoolWorkStatus::Completed,
            now: "2026-06-30T00:00:03Z".to_string(),
            summary: None,
        })
        .unwrap());
    assert!(store
        .complete_worker_pool_work_item(&WorkerPoolCompletionRequest {
            lease_id: "lease-a".to_string(),
            claim_token: "token-a".to_string(),
            status: WorkerPoolWorkStatus::Completed,
            now: "2026-06-30T00:00:04Z".to_string(),
            summary: Some("done".to_string()),
        })
        .unwrap());
    assert!(!store
        .complete_worker_pool_work_item(&WorkerPoolCompletionRequest {
            lease_id: "lease-a".to_string(),
            claim_token: "token-a".to_string(),
            status: WorkerPoolWorkStatus::Failed,
            now: "2026-06-30T00:00:05Z".to_string(),
            summary: Some("too late".to_string()),
        })
        .unwrap());
    assert_eq!(
        store
            .load_worker_pool_work_item("work-a")
            .unwrap()
            .unwrap()
            .status,
        WorkerPoolWorkStatus::Completed
    );

    remove_temp_db(&db_path);
}

#[test]
fn worker_pool_expired_claims_are_terminal_not_resurrected() {
    let db_path = temp_db_path("worker-pool-expiry");
    let store = CoordinationStore::open_file(&db_path).unwrap();
    store
        .upsert_worker_pool_member(&sample_worker_pool_member(
            "member-a",
            "worker-profile",
            WorkerPoolMemberStatus::Available,
            1,
            0,
            "2026-06-30T00:00:00Z",
        ))
        .unwrap();
    store
        .create_worker_pool_work_item(&sample_worker_pool_work_item(
            "work-a",
            Some("worker-profile"),
            10,
            "2026-06-30T00:00:01Z",
        ))
        .unwrap();
    store
        .claim_next_worker_pool_work_item(&WorkerPoolClaimRequest {
            member_id: "member-a".to_string(),
            lease_id: "lease-a".to_string(),
            claim_token: "token-a".to_string(),
            now: "2026-06-30T00:00:02Z".to_string(),
            claim_deadline_at: "2026-06-30T00:00:03Z".to_string(),
            min_heartbeat_at: "2026-06-30T00:00:00Z".to_string(),
        })
        .unwrap()
        .unwrap();

    let expired = store
        .expire_worker_pool_claims(
            &"2026-06-30T00:00:04Z".to_string(),
            &"2026-06-30T00:00:05Z".to_string(),
        )
        .unwrap();
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].status, WorkerPoolWorkStatus::Expired);
    assert!(!store
        .complete_worker_pool_work_item(&WorkerPoolCompletionRequest {
            lease_id: "lease-a".to_string(),
            claim_token: "token-a".to_string(),
            status: WorkerPoolWorkStatus::Completed,
            now: "2026-06-30T00:00:06Z".to_string(),
            summary: Some("too late".to_string()),
        })
        .unwrap());
    assert_eq!(
        store
            .load_worker_pool_work_item("work-a")
            .unwrap()
            .unwrap()
            .status,
        WorkerPoolWorkStatus::Expired
    );
    assert_eq!(
        store
            .load_worker_pool_member("member-a")
            .unwrap()
            .unwrap()
            .active_leases,
        0
    );

    remove_temp_db(&db_path);
}

fn sample_worker_pool_member(
    member_id: &str,
    profile_id: &str,
    status: WorkerPoolMemberStatus,
    concurrency_limit: u32,
    active_leases: u32,
    now: &str,
) -> WorkerPoolMemberRecord {
    WorkerPoolMemberRecord {
        member_id: member_id.to_string(),
        profile_id: ProfileId(profile_id.to_string()),
        agent_id: Some(AgentId(format!("{member_id}-agent"))),
        session_id: Some(SessionId(format!("{member_id}-session"))),
        status,
        concurrency_limit,
        active_leases,
        capabilities_json: json!({"skills": ["review"]}),
        registered_at: now.to_string(),
        last_heartbeat_at: now.to_string(),
        updated_at: now.to_string(),
    }
}

fn sample_worker_pool_work_item(
    work_item_id: &str,
    requested_profile_id: Option<&str>,
    priority: i32,
    now: &str,
) -> WorkerPoolWorkItemRecord {
    WorkerPoolWorkItemRecord {
        work_item_id: work_item_id.to_string(),
        requested_profile_id: requested_profile_id.map(|value| ProfileId(value.to_string())),
        task_id: Some(TaskId(format!("task-{work_item_id}"))),
        status: WorkerPoolWorkStatus::Pending,
        priority,
        work_json: json!({"handoff_markdown": "Please review this slice."}),
        required_capabilities_json: json!({"skills": ["review"]}),
        created_at: now.to_string(),
        updated_at: now.to_string(),
        claimed_by_member_id: None,
        lease_id: None,
        claim_token: None,
        claim_deadline_at: None,
        terminal_at: None,
        terminal_summary: None,
    }
}

fn temp_db_path(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "rusty-crew-{label}-{}-{nanos}.sqlite3",
        std::process::id()
    ))
}

fn temp_data_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("rusty-crew-{label}-{}-{nanos}", std::process::id()))
}

fn roleplay_lore_fts_matches(conn: &Connection, query: &str) -> i64 {
    conn.query_row(
        "SELECT count(*)
             FROM module_roleplay_lore_records_fts
             WHERE module_roleplay_lore_records_fts MATCH ?1",
        params![query],
        |row| row.get::<_, i64>(0),
    )
    .unwrap()
}

fn remove_temp_db(db_path: &Path) {
    let _ = fs::remove_file(db_path);
    let _ = fs::remove_file(format!("{}-wal", db_path.display()));
    let _ = fs::remove_file(format!("{}-shm", db_path.display()));
}

fn remove_temp_dir(path: &Path) {
    let _ = fs::remove_dir_all(path);
}

fn logical_import_bundle(
    repositories: Vec<LogicalStorageRepositoryBundle>,
) -> LogicalStorageExportBundle {
    LogicalStorageExportBundle {
        bundle_version: 1,
        export_id: "export-1".to_string(),
        exported_at: "2026-06-26T10:00:00Z".to_string(),
        service_version: Some("test".to_string()),
        source: LogicalStorageExportSource {
            backend: "sqlite".to_string(),
            backend_label: "SQLite".to_string(),
            source_instance_id: Some("test-instance".to_string()),
            snapshot_ref: Some("logical://export-1".to_string()),
        },
        schema_version: CURRENT_SCHEMA_VERSION,
        module_versions: vec![LogicalStorageModuleVersion {
            module_id: "simple_kv".to_string(),
            schema_version: 1,
            descriptor_fingerprint: Some("test-fingerprint".to_string()),
        }],
        capability_snapshot: vec![LogicalStorageCapabilitySnapshot {
            name: "transactions".to_string(),
            supported: true,
            detail: Some("test capability".to_string()),
        }],
        repositories,
        legacy_id_mappings: vec![LogicalStorageLegacyIdMapping {
            source_system: "legacy-test".to_string(),
            legacy_kind: RuntimeObjectKind::ExternalArtifact,
            legacy_id: "legacy-1".to_string(),
            rusty_kind: RuntimeObjectKind::ExternalArtifact,
            rusty_id: "rusty-1".to_string(),
            provenance: RuntimeImportProvenance::default(),
        }],
        profile_asset_refs: vec![LogicalStorageProfileAssetRef {
            profile_id: ProfileId::new("rusty-crew-runner"),
            asset_kind: "soul".to_string(),
            asset_ref: "profiles/rusty-crew-runner/soul.md".to_string(),
            checksum: None,
            bundled: false,
        }],
    }
}

fn logical_queue_message(
    message_id: &str,
    state: QueuedMessageState,
    expires_at: &str,
    terminal_at: Option<&str>,
) -> LogicalQueuedMessageExportRecord {
    LogicalQueuedMessageExportRecord {
        message_id: message_id.to_string(),
        owner_session_id: Some(SessionId::new("session-alpha")),
        owner_agent_id: AgentId::new("agent-alpha"),
        message: AgentMessage {
            from: AgentId::new("operator"),
            to: AgentId::new("agent-alpha"),
            body: format!("logical import queue {message_id}"),
            correlation_id: Some("logical-import-queue".to_string()),
            projection: None,
        },
        source_sequence: Some(7),
        enqueued_at: "2026-06-26T09:58:00Z".to_string(),
        expires_at: expires_at.to_string(),
        ttl_ms: 5_000,
        delivery_attempts: 0,
        state,
        terminal_at: terminal_at.map(str::to_string),
        state_reason: None,
    }
}

fn assert_active_storage_signal(diagnostics: &RuntimeStorageDiagnostics, signal_name: &str) {
    let signal = diagnostics
        .pressure_signals
        .iter()
        .find(|signal| signal.name == signal_name)
        .unwrap_or_else(|| panic!("missing storage pressure signal {signal_name}"));
    assert!(
        signal.active,
        "expected active storage pressure signal {signal_name}: {signal:?}"
    );
}

fn assert_inactive_storage_signal(diagnostics: &RuntimeStorageDiagnostics, signal_name: &str) {
    let signal = diagnostics
        .pressure_signals
        .iter()
        .find(|signal| signal.name == signal_name)
        .unwrap_or_else(|| panic!("missing storage pressure signal {signal_name}"));
    assert!(
        !signal.active,
        "expected inactive storage pressure signal {signal_name}: {signal:?}"
    );
}

fn sample_provider_wire_state_key() -> ProviderWireStateKey {
    ProviderWireStateKey {
        session_id: SessionId::new("session-alpha"),
        module_id: "openai-responses".to_string(),
        strategy_id: "replay".to_string(),
    }
}

fn simple_kv_schema_bundle(version: u32) -> CoreResult<ModuleSchemaBundle> {
    let mut bundle = crate::module_schema::simple_kv_schema_bundle();
    bundle.schema_version = version;
    if version != 1 {
        bundle
            .migration_notes
            .push(format!("test schema version {version}"));
    }
    Ok(bundle)
}

struct ProviderWireStateWriteFixture<'a> {
    key: ProviderWireStateKey,
    profile_fingerprint: &'a str,
    provider_fingerprint: &'a str,
    payload_version: &'a str,
    payload_json: JsonValue,
    now: &'a str,
    expires_at: Option<&'a str>,
    last_wake_id: Option<&'a str>,
}

fn sample_provider_wire_state_write(
    input: ProviderWireStateWriteFixture<'_>,
) -> ProviderWireStateWrite {
    ProviderWireStateWrite {
        key: input.key,
        profile_fingerprint: input.profile_fingerprint.to_string(),
        provider_fingerprint: input.provider_fingerprint.to_string(),
        payload_version: input.payload_version.to_string(),
        payload_json: input.payload_json,
        now: input.now.to_string(),
        expires_at: input.expires_at.map(ToString::to_string),
        last_wake_id: input.last_wake_id.map(ToString::to_string),
    }
}

fn variant_write(
    slot_id: &MessageSlotId,
    variant_id: &MessageVariantId,
    source: MessageVariantSource,
    ordinal: u32,
    message_id: &str,
    body: &str,
) -> MessageVariantWrite {
    MessageVariantWrite {
        variant_id: variant_id.clone(),
        slot_id: slot_id.clone(),
        source,
        ordinal,
        status: MessageVariantStatus::Active,
        message: DurableMessageWrite {
            message_id: MessageId::new(message_id),
            session_id: SessionId::new("session-1"),
            branch_id: None,
            parent_message_id: None,
            previous_message_id: None,
            author_id: "agent-alpha".to_string(),
            author_role: "assistant".to_string(),
            status: DurableMessageStatus::Completed,
            body: body.to_string(),
            metadata_json: json!({"provider": "fixture"}),
            created_at: "2026-06-25T03:00:00Z".to_string(),
            blocks: vec![MessageBlockWrite {
                block_id: MessageBlockId::new(format!("{message_id}:block-1")),
                ordinal: 0,
                kind: "text".to_string(),
                content_json: json!({"text": body}),
                render_policy_json: None,
                metadata_json: json!({}),
            }],
        },
        metadata_json: json!({}),
        created_at: "2026-06-25T03:00:00Z".to_string(),
        updated_at: "2026-06-25T03:00:00Z".to_string(),
    }
}

fn sample_session_state() -> SessionState {
    SessionState {
        handle: SessionHandle::new(1),
        session_id: SessionId::new("session-alpha"),
        agent_id: AgentId::new("agent-alpha"),
        profile_id: ProfileId::new("full-profile"),
        kind: SessionKind::Full,
        delegation: None,
        resource_limits: sample_resource_limits(),
        tool_profile: sample_tool_profile(),
        history_window: None,
        inference_overrides: Default::default(),
        status: SessionStatus::Idle,
        brain_turn_count: 0,
        created_at: "2026-06-20T00:00:00Z".to_string(),
        last_active_at: "2026-06-20T00:00:00Z".to_string(),
    }
}

fn replaced_write(profile_id: &str, target: ProfileMemoryTarget, key: &str) -> ProfileMemoryWrite {
    ProfileMemoryWrite {
        profile_id: ProfileId::new(profile_id),
        target,
        key: key.to_string(),
        content: "stale write should be rejected".to_string(),
        metadata: serde_json::json!({}),
        now: "2026-06-20T05:02:00Z".to_string(),
    }
}

fn session_fact_memory_write(
    record_id: &str,
    session_id: &SessionId,
    now: &str,
) -> SessionMemoryRecordWrite {
    SessionMemoryRecordWrite {
        record_id: record_id.to_string(),
        session_id: session_id.clone(),
        scope: MemoryScope {
            scope_type: MemoryScopeType::Session,
            scope_id: session_id.0.clone(),
        },
        branch_id: None,
        shape: MemoryRecordShapeRef {
            shape_id: MemoryRecordShapeId::unchecked("session_fact"),
            version: 1,
        },
        content: session_fact_content(record_id, "The user prefers slow-burn pacing.", now),
        evidence_refs: session_memory_evidence("wake-add"),
        source: MemoryProposalSource::CaptureProducer,
        confidence: 0.9,
        durability_rationale:
            "Session fact should survive future wakes without duplicating transcript text."
                .to_string(),
        supersedes_record_id: None,
        now: now.to_string(),
    }
}

fn replace_session_fact_input(record_id: &str) -> SessionMemoryReplace {
    SessionMemoryReplace {
        record_id: record_id.to_string(),
        expected_revision: 1,
        content: session_fact_content(
            record_id,
            "Stale replacement should be rejected.",
            "2026-06-26T01:02:00Z",
        ),
        evidence_refs: session_memory_evidence("wake-stale"),
        source: MemoryProposalSource::Human,
        confidence: 0.8,
        durability_rationale: "Testing stale revision behavior.".to_string(),
        now: "2026-06-26T01:02:00Z".to_string(),
    }
}

fn session_fact_content(record_id: &str, content: &str, now: &str) -> JsonValue {
    json!({
        "record_id": record_id,
        "content": content,
        "fact_kind": "preference",
        "confidence": 0.9,
        "source_summary": "Observed during a session wake.",
        "created_at": now,
        "updated_at": now
    })
}

fn session_memory_record_proposal(
    proposal_id: &str,
    operation: MemoryOperation,
    content: JsonValue,
) -> MemoryProposalEnvelope {
    MemoryProposalEnvelope {
        proposal_id: proposal_id.to_string(),
        space_id: MemorySpaceId::unchecked("session_memory"),
        operation,
        scope: MemoryScope {
            scope_type: MemoryScopeType::Session,
            scope_id: "session-alpha".to_string(),
        },
        shape: MemoryRecordShapeRef {
            shape_id: MemoryRecordShapeId::unchecked("session_fact"),
            version: 1,
        },
        content,
        evidence_refs: session_memory_evidence("wake-proposal"),
        confidence: 0.86,
        durability_rationale: Some("Session proposal should survive future wakes.".to_string()),
        governance_mode: MemoryGovernanceMode::ManualReview,
        source: MemoryProposalSource::CaptureProducer,
        dedupe_key: Some(format!("session_memory:{proposal_id}")),
        created_at: None,
    }
}

fn branch_summary_memory_write(
    record_id: &str,
    session_id: &SessionId,
    branch_id: &ConversationBranchId,
    now: &str,
) -> SessionMemoryRecordWrite {
    SessionMemoryRecordWrite {
        record_id: record_id.to_string(),
        session_id: session_id.clone(),
        scope: MemoryScope {
            scope_type: MemoryScopeType::ConversationBranch,
            scope_id: branch_id.0.clone(),
        },
        branch_id: Some(branch_id.clone()),
        shape: MemoryRecordShapeRef {
            shape_id: MemoryRecordShapeId::unchecked("branch_summary"),
            version: 1,
        },
        content: json!({
            "record_id": record_id,
            "summary": "The branch followed the quiet clue trail.",
            "branch_id": branch_id.0,
            "head_message_id": "message-alpha",
            "coverage_start": "message-root",
            "coverage_end": "message-alpha",
            "created_at": now,
            "updated_at": now
        }),
        evidence_refs: session_memory_evidence("wake-branch"),
        source: MemoryProposalSource::CaptureProducer,
        confidence: 0.87,
        durability_rationale: "Branch summary should survive branch navigation.".to_string(),
        supersedes_record_id: None,
        now: now.to_string(),
    }
}

fn branch_user_choice_memory_write(
    record_id: &str,
    session_id: &SessionId,
    branch_id: &ConversationBranchId,
    now: &str,
) -> SessionMemoryRecordWrite {
    SessionMemoryRecordWrite {
        record_id: record_id.to_string(),
        session_id: session_id.clone(),
        scope: MemoryScope {
            scope_type: MemoryScopeType::ConversationBranch,
            scope_id: branch_id.0.clone(),
        },
        branch_id: Some(branch_id.clone()),
        shape: MemoryRecordShapeRef {
            shape_id: MemoryRecordShapeId::unchecked("user_choice"),
            version: 1,
        },
        content: json!({
            "record_id": record_id,
            "choice": "The user kept the active branch.",
            "choice_kind": "branch_direction",
            "chosen_at": now,
            "status": "active",
            "created_at": now,
            "updated_at": now
        }),
        evidence_refs: session_memory_evidence("wake-branch-choice"),
        source: MemoryProposalSource::CaptureProducer,
        confidence: 0.84,
        durability_rationale: "Branch choice should survive branch navigation.".to_string(),
        supersedes_record_id: None,
        now: now.to_string(),
    }
}

fn save_branch_tree(store: &CoordinationStore) {
    for (branch_id, parent_branch_id, now) in [
        ("branch-root", None, "2026-06-26T01:00:00Z"),
        ("branch-active", Some("branch-root"), "2026-06-26T01:01:00Z"),
        (
            "branch-sibling",
            Some("branch-root"),
            "2026-06-26T01:02:00Z",
        ),
    ] {
        store
            .save_conversation_branch(&ConversationBranchWrite {
                branch_id: ConversationBranchId::new(branch_id),
                session_id: SessionId::new("session-alpha"),
                parent_branch_id: parent_branch_id.map(ConversationBranchId::new),
                parent_message_id: None,
                origin_message_id: Some(MessageId::new(format!("{branch_id}:origin"))),
                head_message_id: Some(MessageId::new(format!("{branch_id}:head"))),
                label: Some(branch_id.to_string()),
                metadata_json: json!({"fixture": true}),
                created_at: now.to_string(),
                updated_at: now.to_string(),
            })
            .unwrap();
    }
}

fn session_memory_evidence(ref_id: &str) -> Vec<MemoryEvidenceRef> {
    vec![MemoryEvidenceRef {
        evidence_type: MemoryEvidenceKind::Wake,
        ref_id: ref_id.to_string(),
        label: Some("Test wake".to_string()),
    }]
}

fn roleplay_lore_write(
    record_id: &str,
    world_id: &str,
    entity_id: Option<&str>,
    title: &str,
    body: &str,
    now: &str,
) -> RoleplayLoreWrite {
    RoleplayLoreWrite {
        record_id: record_id.to_string(),
        world_id: world_id.to_string(),
        entity_id: entity_id.map(ToOwned::to_owned),
        session_id: None,
        branch_id: None,
        shape: MemoryRecordShapeRef {
            shape_id: MemoryRecordShapeId::unchecked("lore_entry"),
            version: 1,
        },
        canon_status: RoleplayLoreCanonStatus::Canon,
        visibility: RoleplayLoreVisibility::Public,
        title: title.to_string(),
        body: body.to_string(),
        content: json!({
            "world_id": world_id,
            "entity_id": entity_id,
            "title": title,
            "body": body,
            "canon_status": "canon",
            "visibility": "public",
            "metadata_json": {"fixture": "roleplay_lore_layers"}
        }),
        evidence_refs: session_memory_evidence("wake-roleplay-lore"),
        source: MemoryProposalSource::Human,
        confidence: 0.92,
        durability_rationale: "Roleplay lore fixture should survive recall.".to_string(),
        supersedes_record_id: None,
        now: now.to_string(),
    }
}

fn profile_registry_write(profile_id: &str) -> ProfileRegistryWrite {
    ProfileRegistryWrite {
        profile_id: ProfileId::new(profile_id),
        lifecycle_status: ProfileRegistryLifecycleStatus::Active,
        display_name: Some("Runner Profile".to_string()),
        summary: Some("Test registry-backed runner profile.".to_string()),
        default_session_kind: Some(SessionKind::Full),
        agent_id: Some(AgentId::new("runner-agent")),
        owner_id: Some("operator".to_string()),
        prompt_soul_markdown: Some("You are a registry-backed runner.".to_string()),
        prompt_memory_markdown: Some("Static deployment-safe memory.".to_string()),
        active_runtime_settings_json: json!({
            "brainModule": "chat_completions_core",
            "model": "gpt"
        }),
        source_asset_refs: vec![
            ProfileRegistrySourceAssetRef {
                asset_kind: "profile_yaml".to_string(),
                path: format!("/home/agents/rusty-crew/config/profiles/{profile_id}/profile.yaml"),
                content_hash: Some("sha256:profile".to_string()),
                last_seen_at: Some("2026-06-26T00:59:00Z".to_string()),
                metadata_json: json!({"source": "file"}),
            },
            ProfileRegistrySourceAssetRef {
                asset_kind: "soul_md".to_string(),
                path: format!("/home/agents/rusty-crew/config/profiles/{profile_id}/soul.md"),
                content_hash: Some("sha256:soul".to_string()),
                last_seen_at: Some("2026-06-26T00:59:00Z".to_string()),
                metadata_json: json!({"source": "file"}),
            },
        ],
        derived_runtime_refs: vec![ProfileRegistryDerivedRuntimeRef {
            ref_kind: "session".to_string(),
            ref_id: "session-runner".to_string(),
            status: "planned".to_string(),
            updated_at: Some("2026-06-26T00:59:00Z".to_string()),
            metadata_json: json!({"derived": true}),
        }],
        import_export: ProfileRegistryImportExportMetadata {
            imported_from: Some("file".to_string()),
            imported_at: Some("2026-06-26T01:00:00Z".to_string()),
            exported_to: None,
            exported_at: None,
            metadata_json: json!({"compatibility": "file_loader"}),
        },
        now: "2026-06-26T01:00:00Z".to_string(),
    }
}

fn profile_dense_memory_proposal(proposal_id: &str, dedupe_key: &str) -> MemoryProposalEnvelope {
    MemoryProposalEnvelope {
        proposal_id: proposal_id.to_string(),
        space_id: MemorySpaceId::unchecked("profile_dense"),
        operation: MemoryOperation::CandidateOnly,
        scope: MemoryScope {
            scope_type: MemoryScopeType::Profile,
            scope_id: "prime-profile".to_string(),
        },
        shape: MemoryRecordShapeRef {
            shape_id: MemoryRecordShapeId::unchecked("profile_dense_item"),
            version: 1,
        },
        content: json!({
            "key": "style",
            "content": "prefers typed governance review"
        }),
        evidence_refs: vec![MemoryEvidenceRef {
            evidence_type: MemoryEvidenceKind::Wake,
            ref_id: "wake-alpha".to_string(),
            label: Some("wake evidence".to_string()),
        }],
        confidence: 0.82,
        durability_rationale: Some("stable profile preference".to_string()),
        governance_mode: MemoryGovernanceMode::DirectWrite,
        source: MemoryProposalSource::InWakeTool,
        dedupe_key: Some(dedupe_key.to_string()),
        created_at: None,
    }
}

fn profile_dense_memory_space_descriptor() -> MemorySpaceDescriptor {
    MemorySpaceDescriptor {
        space_id: MemorySpaceId::unchecked("profile_dense"),
        schema_version: 1,
        module_id: Some("runtime_memory".to_string()),
        description: "Compact stable Crew profile memory.".to_string(),
        record_shapes: vec![MemoryRecordShapeDescriptor {
            shape_id: MemoryRecordShapeId::unchecked("profile_dense_item"),
            version: 1,
            description: "Keyed profile or user memory item.".to_string(),
            fields: vec![
                memory_field("key", MemoryFieldType::String, true),
                memory_field("content", MemoryFieldType::Markdown, true),
                memory_field("metadata_json", MemoryFieldType::Json, false),
                memory_field("revision", MemoryFieldType::Integer, true),
                memory_field("created_at", MemoryFieldType::Timestamp, true),
                memory_field("updated_at", MemoryFieldType::Timestamp, true),
            ],
        }],
        scope_model: MemoryScopeModel {
            allowed_scopes: vec![MemoryScopeType::Profile, MemoryScopeType::User],
            primary_scope: MemoryScopeType::Profile,
        },
        visibility_model: MemoryVisibilityModel::ProfileLocal,
        retrieval_strategies: vec![
            MemoryRetrievalStrategy::DirectLookup,
            MemoryRetrievalStrategy::QuerySearch,
        ],
        indexing: MemoryIndexingPolicy {
            required_capabilities: vec![
                "profile_target_key_lookup".to_string(),
                "expected_revision_conflicts".to_string(),
            ],
            optional_capabilities: vec![],
        },
        prompt_policy: MemoryPromptPolicy::SummaryContext,
        write_policy: MemoryWritePolicy {
            default_mode: MemoryGovernanceMode::Candidate,
            operation_policies: vec![
                memory_operation_policy(MemoryOperation::Add, false),
                memory_operation_policy(MemoryOperation::Replace, true),
                memory_operation_policy(MemoryOperation::Remove, true),
                memory_operation_policy(MemoryOperation::CandidateOnly, false),
            ],
        },
        operations: vec![
            MemoryOperation::Read,
            MemoryOperation::List,
            MemoryOperation::Add,
            MemoryOperation::Replace,
            MemoryOperation::Remove,
            MemoryOperation::CandidateOnly,
        ],
        provenance_policy: MemoryProvenancePolicy {
            required_evidence: vec![MemoryEvidenceKind::Wake],
            source_required: false,
            rationale_required: false,
        },
        retention_policy: MemoryRetentionPolicy::ManualOnly,
        conflict_policy: MemoryConflictPolicy::ExpectedRevision,
        diagnostics: MemoryDiagnosticsPolicy {
            expose_catalog: true,
            expose_record_counts: true,
            expose_policy_decisions: true,
        },
        export_import: MemoryExportImportPolicy {
            export_supported: true,
            import_supported: true,
            import_governance_mode: MemoryGovernanceMode::ManualReview,
        },
    }
}

fn memory_field(
    field_name: &str,
    field_type: MemoryFieldType,
    required: bool,
) -> MemoryRecordFieldDescriptor {
    MemoryRecordFieldDescriptor {
        field_name: field_name.to_string(),
        field_type,
        required,
        description: format!("{field_name} field"),
    }
}

fn memory_operation_policy(
    operation: MemoryOperation,
    requires_expected_revision: bool,
) -> MemoryOperationPolicy {
    MemoryOperationPolicy {
        operation,
        governance_mode: MemoryGovernanceMode::Candidate,
        requires_expected_revision,
        min_confidence: None,
    }
}

fn model_provider_write(
    alias: &str,
    protocol: ModelProviderProtocol,
    provider_kind: &str,
    model_id: &str,
    secret: Option<&str>,
) -> ModelProviderWrite {
    ModelProviderWrite {
        alias: alias.to_string(),
        status: ModelProviderStatus::Active,
        protocol,
        provider_kind: provider_kind.to_string(),
        display_name: Some(alias.to_string()),
        description: None,
        base_url: Some("http://127.0.0.1:18082".to_string()),
        model_id: model_id.to_string(),
        context_window_tokens: Some(128_000),
        max_output_tokens: Some(4_096),
        temperature_milli: Some(500),
        reasoning_effort: None,
        reasoning_format: None,
        responses_dialect: match (protocol, provider_kind) {
            (ModelProviderProtocol::Responses, "openai") => {
                Some(ResponsesProviderDialect::OpenaiStateful)
            }
            (ModelProviderProtocol::Responses, "deepseek") => {
                Some(ResponsesProviderDialect::Deepseek)
            }
            (ModelProviderProtocol::Responses, _) => {
                Some(ResponsesProviderDialect::GenericStateless)
            }
            (ModelProviderProtocol::ChatCompletions, _) => None,
        },
        chat_completions_dialect: Default::default(),
        thinking_mode: Default::default(),
        reasoning_history: Default::default(),
        reasoning_budget_tokens: None,
        prompt_caching: Default::default(),
        secret: secret.map(ToString::to_string),
        clear_secret: false,
        expected_credential_revision: None,
        metadata_json: json!({"fixture": "model_provider_secret_envelope"}),
        expected_revision: None,
        now: "2026-07-02T00:00:00Z".to_string(),
    }
}

fn sample_session_config() -> SessionConfig {
    SessionConfig {
        session_id: SessionId::new("session-alpha"),
        agent_id: AgentId::new("agent-alpha"),
        profile_id: ProfileId::new("full-profile"),
        kind: SessionKind::Full,
        delegation: None,
        resource_limits: sample_resource_limits(),
        tool_profile: sample_tool_profile(),
        history_window: None,
    }
}

fn sample_resource_limits() -> ResourceLimits {
    ResourceLimits {
        workdir: Some("/tmp/rusty-crew-test".to_string()),
        max_duration_ms: Some(60_000),
        max_delegation_depth: Some(4),
    }
}

fn sample_tool_profile() -> ToolProfile {
    ToolProfile {
        tools: vec![ToolDescriptor {
            name: "apply_patch".to_string(),
            description: "Apply a source patch".to_string(),
            input_schema: None,
        }],
    }
}
