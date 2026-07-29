use super::*;
use rusty_crew_core_persistence::{LogicalTurnAdmissionWrite, LogicalTurnContentWrite};
use rusty_crew_core_protocol::{LogicalTurnAttentionReason, LogicalTurnResolutionAction};
use serde_json::json;
use sha2::{Digest, Sha256};

const NOW: &str = "2026-06-19T00:00:00Z";

#[test]
fn logical_turn_diagnostics_preserve_progress_across_restart_and_cancel_yielded_turns() {
    let data_dir = unique_data_dir("logical-turn-diagnostics-restart");
    let engine = test_engine_with_data_dir(data_dir.clone());
    let session = engine
        .create_session(session_config(
            "diagnostic-session",
            "diagnostic-agent",
            "prepared-logical-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let registration = chat_completions_registration();
    let first = engine
        .prepare_logical_turn_wake(
            &registration,
            &session.session_id,
            "diagnostic-wake",
            "system prompt".into(),
            br#"{"messages":[]}"#.to_vec(),
        )
        .unwrap();
    let continuation_payload = json!({
        "messages": ["tool request", "tool result"],
        "providerRequestCount": 7,
        "toolRoundCount": 3
    });
    let continuation = BrainContinuationPayload {
        module_id: "chat-completions".into(),
        payload_version: "chat-completions-work-quantum-v1".into(),
        payload_fingerprint: json_sha256(&continuation_payload),
        payload: continuation_payload,
    };
    engine
        .settle_logical_turn_epoch_with_progress(
            &first.claim,
            LogicalTurnEpochResult::Yielded(continuation),
            Some(BrainWakeProgressSnapshot {
                provider_request_count: 7,
                tool_round_count: 3,
            }),
        )
        .unwrap();
    let logical_turn_id = first.claim.record.logical_turn_id.clone();
    let page = engine
        .logical_turn_diagnostics(&LogicalTurnDiagnosticQuery {
            logical_turn_id: Some(logical_turn_id.clone()),
            session_id: Some(session.session_id.clone()),
            include_terminal: false,
            limit: 10,
        })
        .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].continuation_count, 2);
    assert_eq!(page.items[0].provider_request_total, 7);
    assert_eq!(page.items[0].tool_round_total, 3);
    assert_eq!(
        page.items[0].operator_state,
        LogicalTurnOperatorState::QueuedToContinue
    );
    drop(engine);

    let restarted = test_engine_with_data_dir(data_dir.clone());
    let before_cancel = restarted
        .logical_turn_diagnostics(&LogicalTurnDiagnosticQuery {
            logical_turn_id: Some(logical_turn_id.clone()),
            session_id: None,
            include_terminal: false,
            limit: 1,
        })
        .unwrap()
        .items
        .remove(0);
    let cancellation = restarted
        .cancel_logical_turn(&LogicalTurnCancelRequest {
            logical_turn_id: logical_turn_id.clone(),
            expected_revision: before_cancel.revision,
            idempotency_key: "diagnostic-cancel".into(),
            reason_code: "operator_cancelled".into(),
            summary: "operator cancelled yielded continuation".into(),
            now: NOW.into(),
        })
        .unwrap();
    assert!(!cancellation.already_terminal);
    assert_eq!(cancellation.record.phase, LogicalTurnPhase::Cancelled);
    assert!(restarted
        .logical_turn_continuation_tickets()
        .unwrap()
        .is_empty());
    let terminal = restarted
        .logical_turn_diagnostics(&LogicalTurnDiagnosticQuery {
            logical_turn_id: Some(logical_turn_id),
            session_id: Some(session.session_id),
            include_terminal: true,
            limit: 1,
        })
        .unwrap();
    assert_eq!(terminal.items[0].provider_request_total, 7);
    assert_eq!(terminal.items[0].tool_round_total, 3);
    assert_eq!(
        terminal.items[0].operator_state,
        LogicalTurnOperatorState::Cancelled
    );
    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn prepared_chat_turn_yields_and_restart_resumes_frozen_input_exactly_once() {
    let data_dir = unique_data_dir("logical-turn-prepared-restart");
    let engine = test_engine_with_data_dir(data_dir.clone());
    let session = engine
        .create_session(session_config(
            "prepared-logical-session",
            "prepared-logical-agent",
            "prepared-logical-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine
        .enqueue_body_follow_up_message(
            &session.session_id,
            AgentId::new("operator"),
            "original user input",
            Some("original-message".into()),
        )
        .unwrap();
    let registration = chat_completions_registration();
    let first = engine
        .prepare_logical_turn_wake(
            &registration,
            &session.session_id,
            "wake-original",
            "original system prompt".into(),
            br#"{"messages":["original role message"]}"#.to_vec(),
        )
        .unwrap();
    assert!(first.continuation_state.is_none());
    assert!(String::from_utf8_lossy(&first.body_state_json).contains("original user input"));

    let continuation_payload = json!({
        "messages": ["original user input", "assistant tool call", "tool result"],
        "reasoning": "retained reasoning",
        "provider_request_count": 65
    });
    let continuation = BrainContinuationPayload {
        module_id: "chat-completions".into(),
        payload_version: "chat-completions-work-quantum-v1".into(),
        payload_fingerprint: json_sha256(&continuation_payload),
        payload: continuation_payload.clone(),
    };
    let settlement = engine
        .settle_logical_turn_epoch(
            &first.claim,
            LogicalTurnEpochResult::Yielded(continuation.clone()),
        )
        .unwrap();
    assert_eq!(settlement.outcome, BrainWakeOutcome::Continuing);
    let logical_turn_id = first.claim.record.logical_turn_id.clone();
    let frozen_body = first.body_state_json;
    drop(engine);

    let restarted = test_engine_with_data_dir(data_dir.clone());
    let resumed = restarted
        .prepare_logical_turn_wake(
            &registration,
            &session.session_id,
            "wake-that-must-not-replace-source",
            "replacement prompt that must be ignored".into(),
            br#"{"messages":["replacement role message"]}"#.to_vec(),
        )
        .unwrap();
    assert_eq!(resumed.claim.record.logical_turn_id, logical_turn_id);
    assert_eq!(resumed.system_prompt, "original system prompt");
    assert_eq!(
        resumed.role_assembly_json,
        br#"{"messages":["original role message"]}"#
    );
    assert_eq!(resumed.body_state_json, frozen_body);
    assert_eq!(resumed.continuation_state, Some(continuation));

    let completed = restarted
        .settle_logical_turn_epoch(&resumed.claim, LogicalTurnEpochResult::Completed)
        .unwrap();
    assert_eq!(completed.outcome, BrainWakeOutcome::Completed);
    assert!(restarted
        .logical_turn_continuation_tickets()
        .unwrap()
        .is_empty());
    assert_eq!(
        restarted
            .get_logical_turn(&logical_turn_id)
            .unwrap()
            .unwrap()
            .phase,
        LogicalTurnPhase::Completed
    );
    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn prepared_responses_turn_restores_opaque_checkpoint_after_restart() {
    let data_dir = unique_data_dir("logical-turn-responses-restart");
    let engine = test_engine_with_data_dir(data_dir.clone());
    let session = engine
        .create_session(session_config(
            "prepared-responses-session",
            "prepared-responses-agent",
            "prepared-responses-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine
        .enqueue_body_follow_up_message(
            &session.session_id,
            AgentId::new("operator"),
            "responses input",
            Some("responses-message".into()),
        )
        .unwrap();
    let registration = openai_responses_registration();
    let first = engine
        .prepare_logical_turn_wake(
            &registration,
            &session.session_id,
            "responses-source-wake",
            "responses system prompt".into(),
            br#"{"instructions":"responses role"}"#.to_vec(),
        )
        .unwrap();
    let payload = json!({
        "strategy": "replay",
        "continuation_items": [{"type":"function_call_output","call_id":"call-1"}],
        "last_response_id": "resp-1",
        "provider_request_count": 65
    });
    let continuation = BrainContinuationPayload {
        module_id: "openai-responses".into(),
        payload_version: "openai-responses-continuation-v1".into(),
        payload_fingerprint: json_sha256(&payload),
        payload,
    };
    let settlement = engine
        .settle_logical_turn_epoch(
            &first.claim,
            LogicalTurnEpochResult::Yielded(continuation.clone()),
        )
        .unwrap();
    assert_eq!(settlement.outcome, BrainWakeOutcome::Continuing);
    let logical_turn_id = first.claim.record.logical_turn_id.clone();
    drop(engine);

    let restarted = test_engine_with_data_dir(data_dir.clone());
    let resumed = restarted
        .prepare_logical_turn_wake(
            &registration,
            &session.session_id,
            "replacement-wake",
            "replacement system prompt".into(),
            br#"{"instructions":"replacement role"}"#.to_vec(),
        )
        .unwrap();
    assert_eq!(resumed.claim.record.logical_turn_id, logical_turn_id);
    assert_eq!(resumed.continuation_state, Some(continuation));
    assert_eq!(resumed.system_prompt, "responses system prompt");
    assert_eq!(
        resumed.role_assembly_json,
        br#"{"instructions":"responses role"}"#
    );
    restarted
        .settle_logical_turn_epoch(&resumed.claim, LogicalTurnEpochResult::Completed)
        .unwrap();
    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn operator_attention_survives_restart_and_explicit_retry_resumes_same_turn() {
    let data_dir = unique_data_dir("logical-turn-attention-restart");
    let engine = test_engine_with_data_dir(data_dir.clone());
    let session = engine
        .create_session(session_config(
            "attention-session",
            "attention-agent",
            "prepared-logical-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let registration = chat_completions_registration();
    let first = engine
        .prepare_logical_turn_wake(
            &registration,
            &session.session_id,
            "attention-source-wake",
            "attention system prompt".into(),
            br#"{"messages":["attention role"]}"#.to_vec(),
        )
        .unwrap();
    let payload = json!({"messages": ["failed call"], "no_progress": 3});
    let continuation = BrainContinuationPayload {
        module_id: "chat-completions".into(),
        payload_version: "chat-completions-continuation-v2".into(),
        payload_fingerprint: json_sha256(&payload),
        payload,
    };
    let settlement = engine
        .settle_logical_turn_epoch(
            &first.claim,
            LogicalTurnEpochResult::AttentionRequired {
                module_state: continuation.clone(),
                attention: BrainWakeAttention {
                    reason: LogicalTurnAttentionReason::NoProgress,
                    reason_code: "chat_completions_tool_no_progress".into(),
                    summary: "tool returned the same failure repeatedly".into(),
                    evidence_refs: vec!["tool:lookup".into()],
                    resolution_actions: vec![
                        LogicalTurnResolutionAction::RetryProviderOperation,
                        LogicalTurnResolutionAction::Cancel,
                    ],
                    retry_unchanged_safe: false,
                    consecutive_no_progress_samples: 3,
                },
            },
        )
        .unwrap();
    assert_eq!(settlement.phase, LogicalTurnPhase::AttentionRequired);
    assert!(engine
        .logical_turn_continuation_tickets()
        .unwrap()
        .is_empty());
    let logical_turn_id = first.claim.record.logical_turn_id.clone();
    drop(engine);

    let restarted = test_engine_with_data_dir(data_dir.clone());
    let attention_record = restarted
        .get_logical_turn(&logical_turn_id)
        .unwrap()
        .unwrap();
    assert_eq!(attention_record.phase, LogicalTurnPhase::AttentionRequired);
    assert!(attention_record.attention.is_some());
    assert!(restarted
        .logical_turn_continuation_tickets()
        .unwrap()
        .is_empty());
    let checkpoint = restarted
        .get_logical_turn_checkpoint(&attention_record.current_continuation_id)
        .unwrap()
        .unwrap();
    let resolved = restarted
        .resolve_logical_turn_attention(&LogicalTurnAttentionResolutionRequest {
            logical_turn_id: logical_turn_id.clone(),
            expected_revision: attention_record.revision,
            action: LogicalTurnResolutionAction::RetryProviderOperation,
            lifecycle_event: LogicalTurnLifecycleEvent {
                projection_id: TurnProjectionId::new(format!(
                    "projection:{}:{}:attention-resolved",
                    logical_turn_id.0,
                    attention_record.revision + 1
                )),
                logical_turn_id: logical_turn_id.clone(),
                session_id: attention_record.session_id.clone(),
                wake_id: attention_record.source_wake_id.clone(),
                continuation_id: attention_record.current_continuation_id.clone(),
                execution_epoch_id: None,
                kind: LogicalTurnLifecycleEventKind::ContinuationResumed,
                phase: LogicalTurnPhase::Runnable,
                continuation_count: attention_record.continuation_sequence + 1,
                operator_state:
                    rusty_crew_core_protocol::LogicalTurnOperatorState::QueuedToContinue,
                progress_classification:
                    rusty_crew_core_protocol::LogicalTurnProgressClassification::NoProgress,
                progress: checkpoint.progress.clone(),
                reason_code: "operator_retry_provider_operation".into(),
                summary: "operator resolved attention and requested a provider retry".into(),
                occurred_at: NOW.into(),
                logical_turn_revision: attention_record.revision + 1,
            },
            now: NOW.into(),
        })
        .unwrap();
    assert_eq!(resolved.record.phase, LogicalTurnPhase::Runnable);
    assert!(resolved.record.attention.is_none());
    assert_eq!(
        restarted.logical_turn_continuation_tickets().unwrap().len(),
        1
    );

    let resumed = restarted
        .prepare_logical_turn_wake(
            &registration,
            &session.session_id,
            "replacement-wake",
            "replacement system prompt".into(),
            br#"{"messages":["replacement role"]}"#.to_vec(),
        )
        .unwrap();
    assert_eq!(resumed.claim.record.logical_turn_id, logical_turn_id);
    assert_eq!(resumed.continuation_state, Some(continuation));
    restarted
        .settle_logical_turn_epoch(&resumed.claim, LogicalTurnEpochResult::Completed)
        .unwrap();
    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn logical_turn_yields_idempotently_and_restart_resumes_without_new_input() {
    let data_dir = unique_data_dir("logical-turn-restart");
    let engine = test_engine_with_data_dir(data_dir.clone());
    let session = engine
        .create_session(session_config(
            "logical-session",
            "logical-agent",
            "logical-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let (_lifecycle_subscription, lifecycle_events) = engine
        .subscribe_events(EventSubscription {
            event_kinds: vec![CoreEventKind::LogicalTurnLifecycleObserved],
            session_id: Some(session.session_id.clone()),
            agent_id: None,
            adapter_id: None,
        })
        .unwrap();
    let admission = admission(&session.session_id);
    engine.admit_logical_turn(&admission).unwrap();
    assert!(matches!(
        lifecycle_events.recv_timeout(Duration::from_secs(1)).unwrap(),
        CoreEvent::LogicalTurnLifecycleObserved { lifecycle }
            if lifecycle.kind == LogicalTurnLifecycleEventKind::Admitted
    ));

    let claim = engine
        .claim_logical_turn(&LogicalTurnClaimRequest {
            logical_turn_id: LogicalTurnId::new("turn-1"),
            expected_revision: 1,
            continuation_id: ContinuationId::new("continuation-0"),
            execution_epoch_id: ExecutionEpochId::new("epoch-1"),
            claim_holder: "service-a".into(),
            claim_expires_at: "2026-06-19T00:01:00Z".into(),
            now: NOW.into(),
        })
        .unwrap();
    assert_eq!(claim.record.phase, LogicalTurnPhase::Running);

    let yield_request = yield_request(&claim);
    let yielded = engine.yield_logical_turn(&yield_request).unwrap();
    assert_eq!(yielded.record.phase, LogicalTurnPhase::Yielded);
    assert!(!yielded.replayed);
    let replay = engine.yield_logical_turn(&yield_request).unwrap();
    assert!(replay.replayed);
    assert_eq!(engine.logical_turn_continuation_tickets().unwrap().len(), 1);

    let body = engine.project_body_state(&session.session_id).unwrap();
    assert!(body
        .recent_events
        .iter()
        .all(|event| !matches!(event, CoreEvent::LogicalTurnLifecycleObserved { .. })));
    drop(engine);

    let restarted = test_engine_with_data_dir(data_dir.clone());
    let turn = restarted
        .get_logical_turn(&LogicalTurnId::new("turn-1"))
        .unwrap()
        .unwrap();
    assert_eq!(turn.phase, LogicalTurnPhase::Runnable);
    assert_eq!(turn.current_continuation_id.0, "continuation-1");
    assert_eq!(
        restarted.logical_turn_continuation_tickets().unwrap().len(),
        1
    );
    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn logical_turn_cancellation_fences_restart_resurrection() {
    let data_dir = unique_data_dir("logical-turn-cancel");
    let engine = test_engine_with_data_dir(data_dir.clone());
    let session = engine
        .create_session(session_config(
            "logical-cancel-session",
            "logical-cancel-agent",
            "logical-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine
        .admit_logical_turn(&admission(&session.session_id))
        .unwrap();
    let receipt = engine
        .cancel_logical_turn(&LogicalTurnCancelRequest {
            logical_turn_id: LogicalTurnId::new("turn-1"),
            expected_revision: 1,
            idempotency_key: "cancel-1".into(),
            reason_code: "operator_cancelled".into(),
            summary: "operator cancelled the logical turn".into(),
            now: NOW.into(),
        })
        .unwrap();
    assert_eq!(receipt.record.phase, LogicalTurnPhase::Cancelled);
    assert!(engine
        .logical_turn_continuation_tickets()
        .unwrap()
        .is_empty());
    let replay = engine
        .cancel_logical_turn(&LogicalTurnCancelRequest {
            logical_turn_id: LogicalTurnId::new("turn-1"),
            expected_revision: 1,
            idempotency_key: "cancel-1".into(),
            reason_code: "operator_cancelled".into(),
            summary: "operator cancelled the logical turn".into(),
            now: NOW.into(),
        })
        .unwrap();
    assert!(replay.replayed);
    drop(engine);

    let restarted = test_engine_with_data_dir(data_dir.clone());
    assert!(restarted
        .logical_turn_continuation_tickets()
        .unwrap()
        .is_empty());
    assert_eq!(
        restarted
            .get_logical_turn(&LogicalTurnId::new("turn-1"))
            .unwrap()
            .unwrap()
            .phase,
        LogicalTurnPhase::Cancelled
    );
    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
#[cfg(feature = "postgres")]
#[ignore = "requires local PostgreSQL dev database env"]
fn postgres_logical_turn_checkpoint_restart_and_cancel_match_sqlite() {
    let database_url = std::env::var("RUSTY_CREW_DATABASE_URL")
        .or_else(|_| std::env::var("RUSTY_CREW_POSTGRES_BACKEND_DATABASE_URL"))
        .expect("PostgreSQL database URL must be set for logical-turn parity test");
    let schema = format!(
        "rc_logical_turn_{}_{}",
        std::process::id(),
        NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed)
    );
    let data_dir = unique_data_dir("logical-turn-postgres");
    let config = EngineConfig {
        engine_data_dir: data_dir.to_string_lossy().to_string(),
        clock: ClockConfig::Fixed { at: NOW.into() },
        default_turn_budget: 3,
        default_idle_timeout_ms: 1_000,
        storage: Some(EngineStorageConfig::Postgres {
            database_url: database_url.clone(),
            schema: schema.clone(),
            max_connections: None,
            statement_timeout_ms: None,
        }),
    };
    let engine = CoreEngine::initialize(config.clone()).unwrap();
    let session = engine
        .create_session(session_config(
            "logical-postgres-session",
            "logical-postgres-agent",
            "logical-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine
        .admit_logical_turn(&admission(&session.session_id))
        .unwrap();
    let claim = engine
        .claim_logical_turn(&LogicalTurnClaimRequest {
            logical_turn_id: LogicalTurnId::new("turn-1"),
            expected_revision: 1,
            continuation_id: ContinuationId::new("continuation-0"),
            execution_epoch_id: ExecutionEpochId::new("epoch-pg-1"),
            claim_holder: "service-pg".into(),
            claim_expires_at: "2026-06-19T00:01:00Z".into(),
            now: NOW.into(),
        })
        .unwrap();
    let yielded = engine.yield_logical_turn(&yield_request(&claim)).unwrap();
    assert_eq!(yielded.record.phase, LogicalTurnPhase::Yielded);
    drop(engine);

    let restarted = CoreEngine::initialize(config).unwrap();
    let turn = restarted
        .get_logical_turn(&LogicalTurnId::new("turn-1"))
        .unwrap()
        .unwrap();
    assert_eq!(turn.phase, LogicalTurnPhase::Runnable);
    restarted
        .cancel_logical_turn(&LogicalTurnCancelRequest {
            logical_turn_id: turn.logical_turn_id,
            expected_revision: turn.revision,
            idempotency_key: "cancel-pg".into(),
            reason_code: "parity_test_cancel".into(),
            summary: "cancel PostgreSQL parity turn".into(),
            now: NOW.into(),
        })
        .unwrap();
    drop(restarted);

    postgres::Client::connect(&database_url, postgres::NoTls)
        .unwrap()
        .batch_execute(&format!("DROP SCHEMA IF EXISTS \"{schema}\" CASCADE"))
        .unwrap();
    let _ = std::fs::remove_dir_all(data_dir);
}

fn admission(session_id: &SessionId) -> LogicalTurnAdmissionWrite {
    let progress = progress(0, "initial");
    let frozen_input = LogicalTurnFrozenInput {
        body_state_ref: "sha256:body".into(),
        body_state_fingerprint: "body".into(),
        system_prompt_ref: "sha256:prompt".into(),
        system_prompt_fingerprint: "prompt".into(),
        role_assembly_ref: "sha256:role".into(),
        role_assembly_fingerprint: "role".into(),
        transcript_cursor: 0,
        attachment_refs: Vec::new(),
    };
    let record = LogicalTurnRecord {
        logical_turn_id: LogicalTurnId::new("turn-1"),
        session_id: session_id.clone(),
        source_wake_id: "wake-1".into(),
        phase: LogicalTurnPhase::Admitted,
        binding: LogicalTurnBindingSnapshot {
            profile_id: ProfileId::new("logical-profile"),
            profile_revision: 1,
            prompt_fingerprint: "prompt".into(),
            tool_selection_fingerprint: "tools".into(),
            tool_registry_revision: "1".into(),
            brain_module_id: "test-brain".into(),
            brain_strategy_id: "test-strategy".into(),
            provider_alias: "test-provider".into(),
            provider_revision: 1,
            provider_fingerprint: "provider".into(),
            credential_id: None,
            credential_revision: None,
        },
        current_continuation_id: ContinuationId::new("continuation-0"),
        continuation_sequence: 0,
        binding_generation: 1,
        cancellation_generation: 0,
        active_epoch_id: None,
        claim_generation: None,
        claim_holder: None,
        claim_expires_at: None,
        attention: None,
        revision: 1,
        admitted_at: NOW.into(),
        updated_at: NOW.into(),
        terminal_at: None,
    };
    let checkpoint = LogicalTurnCheckpoint {
        continuation_id: ContinuationId::new("continuation-0"),
        logical_turn_id: record.logical_turn_id.clone(),
        sequence: 0,
        parent_continuation_id: None,
        completed_epoch_id: None,
        binding_generation: 1,
        frozen_input,
        module_state: BrainContinuationPayload {
            module_id: "test-brain".into(),
            payload_version: "1".into(),
            payload_fingerprint: "initial".into(),
            payload: json!({"cursor": 0}),
        },
        operation_cursor: 0,
        projection_cursor: 0,
        progress: progress.clone(),
        yield_reason: ContinuationYieldReason::InitialAdmission,
        created_at: NOW.into(),
    };
    let lifecycle = LogicalTurnLifecycleEvent {
        projection_id: TurnProjectionId::new("projection:turn-1:1:admitted"),
        logical_turn_id: record.logical_turn_id.clone(),
        session_id: session_id.clone(),
        wake_id: record.source_wake_id.clone(),
        continuation_id: checkpoint.continuation_id.clone(),
        execution_epoch_id: None,
        kind: LogicalTurnLifecycleEventKind::Admitted,
        phase: LogicalTurnPhase::Admitted,
        continuation_count: 1,
        operator_state: rusty_crew_core_protocol::LogicalTurnOperatorState::QueuedToContinue,
        progress_classification:
            rusty_crew_core_protocol::LogicalTurnProgressClassification::Admitted,
        progress,
        reason_code: "initial_admission".into(),
        summary: "logical turn admitted".into(),
        occurred_at: NOW.into(),
        logical_turn_revision: 1,
    };
    LogicalTurnAdmissionWrite {
        admission: LogicalTurnAdmission {
            record,
            initial_checkpoint: checkpoint,
            lifecycle_event: lifecycle,
        },
        frozen_content: vec![
            content("sha256:body", "body", "body_state", b"{}"),
            content("sha256:prompt", "prompt", "system_prompt", b"prompt"),
            content("sha256:role", "role", "role_assembly", b"{}"),
        ],
    }
}

fn yield_request(
    claim: &rusty_crew_core_protocol::LogicalTurnContinuationClaim,
) -> LogicalTurnYieldRequest {
    let progress = progress(1, "yielded");
    let checkpoint = LogicalTurnCheckpoint {
        continuation_id: ContinuationId::new("continuation-1"),
        logical_turn_id: claim.record.logical_turn_id.clone(),
        sequence: 1,
        parent_continuation_id: Some(claim.record.current_continuation_id.clone()),
        completed_epoch_id: claim.record.active_epoch_id.clone(),
        binding_generation: claim.record.binding_generation,
        frozen_input: claim.checkpoint.frozen_input.clone(),
        module_state: BrainContinuationPayload {
            module_id: "test-brain".into(),
            payload_version: "1".into(),
            payload_fingerprint: "yielded".into(),
            payload: json!({"cursor": 1}),
        },
        operation_cursor: 1,
        projection_cursor: 1,
        progress: progress.clone(),
        yield_reason: ContinuationYieldReason::WorkQuantumReached,
        created_at: NOW.into(),
    };
    LogicalTurnYieldRequest {
        logical_turn_id: claim.record.logical_turn_id.clone(),
        expected_revision: claim.record.revision,
        expected_epoch_id: claim.record.active_epoch_id.clone().unwrap(),
        expected_claim_generation: claim.claim_generation,
        expected_cancellation_generation: claim.record.cancellation_generation,
        checkpoint: checkpoint.clone(),
        lifecycle_event: LogicalTurnLifecycleEvent {
            projection_id: TurnProjectionId::new("projection:turn-1:3:yielded"),
            logical_turn_id: claim.record.logical_turn_id.clone(),
            session_id: claim.record.session_id.clone(),
            wake_id: claim.record.source_wake_id.clone(),
            continuation_id: checkpoint.continuation_id.clone(),
            execution_epoch_id: claim.record.active_epoch_id.clone(),
            kind: LogicalTurnLifecycleEventKind::ContinuationYielded,
            phase: LogicalTurnPhase::Yielded,
            continuation_count: claim.record.continuation_sequence + 2,
            operator_state: rusty_crew_core_protocol::LogicalTurnOperatorState::QueuedToContinue,
            progress_classification:
                rusty_crew_core_protocol::LogicalTurnProgressClassification::SemanticProgress,
            progress,
            reason_code: "work_quantum_reached".into(),
            summary: "logical turn yielded and will continue".into(),
            occurred_at: NOW.into(),
            logical_turn_revision: claim.record.revision + 1,
        },
        now: NOW.into(),
    }
}

fn content(
    content_ref: &str,
    fingerprint: &str,
    content_kind: &str,
    content: &[u8],
) -> LogicalTurnContentWrite {
    LogicalTurnContentWrite {
        content_ref: content_ref.into(),
        fingerprint: fingerprint.into(),
        content_kind: content_kind.into(),
        content: content.to_vec(),
        created_at: NOW.into(),
    }
}

fn progress(revision: u64, fingerprint: &str) -> LogicalTurnProgress {
    LogicalTurnProgress {
        semantic_revision: revision,
        state_fingerprint: fingerprint.into(),
        last_liveness_at: NOW.into(),
        last_semantic_progress_at: NOW.into(),
        ..LogicalTurnProgress::default()
    }
}

fn chat_completions_registration() -> BrainImplementationRegistration {
    BrainImplementationRegistration {
        implementation_id: BrainImplementationId::new("chat-completions"),
        profile_id: ProfileId::new("prepared-logical-profile"),
        tool_profile: ToolProfile { tools: Vec::new() },
        model_config: BrainModelConfig {
            provider: "test-provider".into(),
            model_name: "test-model".into(),
            temperature_milli: None,
            max_output_tokens: None,
        },
        strategy: None,
        provider_state_scope: None,
    }
}

fn openai_responses_registration() -> BrainImplementationRegistration {
    BrainImplementationRegistration {
        implementation_id: BrainImplementationId::new("openai-responses"),
        profile_id: ProfileId::new("prepared-responses-profile"),
        tool_profile: ToolProfile { tools: Vec::new() },
        model_config: BrainModelConfig {
            provider: "test-provider".into(),
            model_name: "test-model".into(),
            temperature_milli: None,
            max_output_tokens: None,
        },
        strategy: Some(rusty_crew_core_protocol::BrainStrategyMetadata {
            module_id: "openai-responses".into(),
            strategy_id: "replay".into(),
            provider_state: rusty_crew_core_protocol::BrainProviderStateStrategyMetadata {
                mode: ProviderStateMode::Optional,
            },
        }),
        provider_state_scope: None,
    }
}

fn json_sha256(value: &serde_json::Value) -> String {
    let bytes = serde_json::to_vec(value).unwrap();
    format!("{:x}", Sha256::digest(bytes))
}
