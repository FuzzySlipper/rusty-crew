use super::*;
use rusty_crew_core_persistence::{LogicalTurnAdmissionWrite, LogicalTurnContentWrite};
use serde_json::json;

const NOW: &str = "2026-06-19T00:00:00Z";

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
