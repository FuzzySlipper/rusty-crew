use super::*;
use rusty_crew_core_persistence::{
    LogicalTurnAdmissionWrite, LogicalTurnContentWrite, LogicalTurnOperationCompletionRequest,
    LogicalTurnOperationLeaseRequest,
};
use rusty_crew_core_protocol::{
    BrainOperationId, LogicalTurnAttentionReason, LogicalTurnOperationKind,
    LogicalTurnOperationPhase, LogicalTurnOperationRecord, LogicalTurnResolutionAction,
};
use serde_json::json;
use sha2::{Digest, Sha256};

const NOW: &str = "2026-06-19T00:00:00Z";

#[test]
fn production_operation_receipts_are_durable_and_cancellation_fenced() {
    let data_dir = unique_data_dir("logical-turn-operation-receipts");
    let engine = test_engine_with_data_dir(data_dir.clone());
    let session = engine
        .create_session(session_config(
            "operation-session",
            "operation-agent",
            "prepared-logical-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let prepared = engine
        .prepare_logical_turn_wake(
            &chat_completions_registration(),
            &session.session_id,
            "operation-wake",
            "system prompt".into(),
            br#"{"messages":[]}"#.to_vec(),
        )
        .unwrap();
    let claim_generation = prepared.claim.record.claim_generation.unwrap();
    let epoch_id = prepared.claim.record.active_epoch_id.clone().unwrap();
    let leased = LogicalTurnOperationRecord {
        operation_id: BrainOperationId::new("operation:test:1"),
        logical_turn_id: prepared.claim.record.logical_turn_id.clone(),
        continuation_id: prepared.claim.record.current_continuation_id.clone(),
        execution_epoch_id: epoch_id,
        kind: LogicalTurnOperationKind::HostToolExecution,
        phase: LogicalTurnOperationPhase::Leased,
        request_fingerprint: "request-fingerprint".into(),
        idempotency_key: "operation:test:1".into(),
        lease_holder: prepared.claim.record.claim_holder.clone(),
        lease_generation: Some(claim_generation),
        lease_expires_at: prepared.claim.record.claim_expires_at.clone(),
        result_ref: None,
        result_payload: None,
        reason_code: None,
        revision: 1,
        created_at: NOW.into(),
        updated_at: NOW.into(),
    };
    engine
        .lease_logical_turn_operation(&LogicalTurnOperationLeaseRequest {
            operation: leased.clone(),
            expected_turn_revision: prepared.claim.record.revision,
            expected_claim_generation: claim_generation,
            expected_cancellation_generation: prepared.claim.record.cancellation_generation,
        })
        .unwrap();
    let mut completed = leased.clone();
    completed.phase = LogicalTurnOperationPhase::Completed;
    completed.result_ref = Some("sha256:result".into());
    completed.result_payload = Some(json!({"status":"succeeded","output":"ok"}));
    completed.revision = 2;
    let completed = engine
        .complete_logical_turn_operation(&LogicalTurnOperationCompletionRequest {
            operation: completed,
            expected_operation_revision: 1,
            expected_turn_revision: prepared.claim.record.revision,
            expected_claim_generation: claim_generation,
            expected_cancellation_generation: prepared.claim.record.cancellation_generation,
        })
        .unwrap();
    assert_eq!(completed.phase, LogicalTurnOperationPhase::Completed);
    let mut cancelled_lease = leased.clone();
    cancelled_lease.operation_id = BrainOperationId::new("operation:test:2");
    cancelled_lease.idempotency_key = "operation:test:2".into();
    engine
        .lease_logical_turn_operation(&LogicalTurnOperationLeaseRequest {
            operation: cancelled_lease.clone(),
            expected_turn_revision: prepared.claim.record.revision,
            expected_claim_generation: claim_generation,
            expected_cancellation_generation: prepared.claim.record.cancellation_generation,
        })
        .unwrap();
    engine
        .cancel_logical_turn(&LogicalTurnCancelRequest {
            logical_turn_id: prepared.claim.record.logical_turn_id.clone(),
            expected_revision: prepared.claim.record.revision,
            idempotency_key: "cancel-operation-turn".into(),
            reason_code: "operator_cancelled".into(),
            summary: "operator cancelled while a tool result was returning".into(),
            now: NOW.into(),
        })
        .unwrap();
    cancelled_lease.phase = LogicalTurnOperationPhase::Completed;
    cancelled_lease.result_ref = Some("sha256:cancelled-result".into());
    cancelled_lease.result_payload = Some(json!({"status":"succeeded","output":"late"}));
    cancelled_lease.revision = 2;
    let cancelled_completion = engine
        .complete_logical_turn_operation(&LogicalTurnOperationCompletionRequest {
            operation: cancelled_lease,
            expected_operation_revision: 1,
            expected_turn_revision: prepared.claim.record.revision,
            expected_claim_generation: claim_generation,
            expected_cancellation_generation: prepared.claim.record.cancellation_generation,
        })
        .unwrap();
    assert_eq!(
        cancelled_completion.phase,
        LogicalTurnOperationPhase::CompletedAfterCancel
    );
    drop(engine);

    let restarted = test_engine_with_data_dir(data_dir.clone());
    let operations = restarted
        .list_logical_turn_operations(&prepared.claim.record.logical_turn_id)
        .unwrap();
    assert_eq!(operations, vec![completed, cancelled_completion]);
    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn restart_unknown_tool_outcome_resolutions_reconcile_operation_and_turn_atomically() {
    for (suffix, action, expected_phase) in [
        (
            "completed",
            LogicalTurnResolutionAction::ConfirmToolCompleted,
            LogicalTurnOperationPhase::Completed,
        ),
        (
            "not-completed",
            LogicalTurnResolutionAction::ConfirmToolNotCompleted,
            LogicalTurnOperationPhase::Superseded,
        ),
    ] {
        let data_dir = unique_data_dir(&format!("logical-turn-outcome-{suffix}"));
        let engine = test_engine_with_data_dir(data_dir.clone());
        let session = engine
            .create_session(session_config(
                &format!("outcome-session-{suffix}"),
                &format!("outcome-agent-{suffix}"),
                "prepared-logical-profile",
                SessionKind::Full,
            ))
            .unwrap();
        let prepared = engine
            .prepare_logical_turn_wake(
                &chat_completions_registration(),
                &session.session_id,
                &format!("outcome-wake-{suffix}"),
                "system prompt".into(),
                br#"{"messages":[]}"#.to_vec(),
            )
            .unwrap();
        let operation_id = BrainOperationId::new(format!("operation:outcome:{suffix}"));
        engine
            .lease_logical_turn_operation(&LogicalTurnOperationLeaseRequest {
                operation: LogicalTurnOperationRecord {
                    operation_id: operation_id.clone(),
                    logical_turn_id: prepared.claim.record.logical_turn_id.clone(),
                    continuation_id: prepared.claim.record.current_continuation_id.clone(),
                    execution_epoch_id: prepared.claim.record.active_epoch_id.clone().unwrap(),
                    kind: LogicalTurnOperationKind::HostToolExecution,
                    phase: LogicalTurnOperationPhase::Leased,
                    request_fingerprint: "request-fingerprint".into(),
                    idempotency_key: operation_id.0.clone(),
                    lease_holder: prepared.claim.record.claim_holder.clone(),
                    lease_generation: prepared.claim.record.claim_generation,
                    lease_expires_at: prepared.claim.record.claim_expires_at.clone(),
                    result_ref: None,
                    result_payload: None,
                    reason_code: None,
                    revision: 1,
                    created_at: NOW.into(),
                    updated_at: NOW.into(),
                },
                expected_turn_revision: prepared.claim.record.revision,
                expected_claim_generation: prepared.claim.record.claim_generation.unwrap(),
                expected_cancellation_generation: prepared.claim.record.cancellation_generation,
            })
            .unwrap();
        let logical_turn_id = prepared.claim.record.logical_turn_id.clone();
        drop(engine);

        let restarted = test_engine_with_data_dir(data_dir.clone());
        let attention = restarted
            .get_logical_turn(&logical_turn_id)
            .unwrap()
            .unwrap();
        assert_eq!(attention.phase, LogicalTurnPhase::AttentionRequired);
        assert_eq!(
            attention.attention.as_ref().unwrap().reason,
            LogicalTurnAttentionReason::ToolOutcomeUnknown
        );
        assert!(attention
            .attention
            .as_ref()
            .unwrap()
            .resolution_actions
            .contains(&action));
        let unknown = restarted
            .list_logical_turn_operations(&logical_turn_id)
            .unwrap();
        assert_eq!(unknown.len(), 1);
        assert_eq!(unknown[0].phase, LogicalTurnOperationPhase::OutcomeUnknown);

        let resolved = restarted
            .resolve_logical_turn_attention_for_operator(
                &logical_turn_id,
                attention.revision,
                action,
            )
            .unwrap();
        assert_eq!(resolved.record.phase, LogicalTurnPhase::Runnable);
        assert!(resolved.record.attention.is_none());
        assert_eq!(
            restarted.logical_turn_continuation_tickets().unwrap().len(),
            1
        );
        let reconciled = restarted
            .list_logical_turn_operations(&logical_turn_id)
            .unwrap();
        assert_eq!(reconciled.len(), 1);
        assert_eq!(reconciled[0].phase, expected_phase);
        if action == LogicalTurnResolutionAction::ConfirmToolCompleted {
            assert_eq!(
                reconciled[0].result_payload.as_ref().unwrap()["reasonCode"],
                "operator_confirmed_tool_completed"
            );
        } else {
            assert!(reconciled[0].result_payload.is_none());
        }
        let stale = restarted.resolve_logical_turn_attention_for_operator(
            &logical_turn_id,
            attention.revision,
            action,
        );
        assert!(stale.is_err());
        std::fs::remove_dir_all(data_dir).unwrap();
    }
}

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
fn restart_during_initial_epoch_replays_frozen_input_without_admission_payload() {
    let cases = [
        (
            "initial-epoch-chat-restart",
            "initial-epoch-chat-session",
            "prepared-logical-profile",
            chat_completions_registration(),
        ),
        (
            "initial-epoch-responses-restart",
            "initial-epoch-responses-session",
            "prepared-responses-profile",
            openai_responses_registration(),
        ),
    ];

    for (data_label, session_id, profile_id, registration) in cases {
        let data_dir = unique_data_dir(data_label);
        let engine = test_engine_with_data_dir(data_dir.clone());
        let session = engine
            .create_session(session_config(
                session_id,
                &format!("{session_id}-agent"),
                profile_id,
                SessionKind::Full,
            ))
            .unwrap();
        engine
            .enqueue_body_follow_up_message(
                &session.session_id,
                AgentId::new("operator"),
                "original input",
                Some(format!("{session_id}-message")),
            )
            .unwrap();
        let first = engine
            .prepare_logical_turn_wake(
                &registration,
                &session.session_id,
                "source-wake",
                "frozen system prompt".into(),
                br#"{"messages":["frozen role"]}"#.to_vec(),
            )
            .unwrap();
        assert!(first.continuation_state.is_none());
        let logical_turn_id = first.claim.record.logical_turn_id.clone();
        drop(engine);

        let restarted = test_engine_with_data_dir(data_dir.clone());
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
        assert_eq!(resumed.system_prompt, "frozen system prompt");
        assert_eq!(
            resumed.role_assembly_json,
            br#"{"messages":["frozen role"]}"#
        );
        assert!(resumed.continuation_state.is_none());

        restarted
            .settle_logical_turn_epoch(&resumed.claim, LogicalTurnEpochResult::Completed)
            .unwrap();
        std::fs::remove_dir_all(data_dir).unwrap();
    }
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
    assert_eq!(
        engine
            .session_execution_state(&session.session_id)
            .unwrap()
            .phase,
        SessionExecutionPhase::Queued
    );
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
    assert_eq!(
        engine
            .session_execution_state(&session.session_id)
            .unwrap()
            .phase,
        SessionExecutionPhase::Active
    );

    let yield_request = yield_request(&claim);
    let yielded = engine.yield_logical_turn(&yield_request).unwrap();
    assert_eq!(yielded.record.phase, LogicalTurnPhase::Yielded);
    assert_eq!(
        engine
            .session_execution_state(&session.session_id)
            .unwrap()
            .phase,
        SessionExecutionPhase::Queued
    );
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
    assert_eq!(
        restarted
            .session_execution_state(&session.session_id)
            .unwrap()
            .phase,
        SessionExecutionPhase::Queued
    );
    assert_eq!(turn.current_continuation_id.0, "continuation-1");
    assert_eq!(
        restarted.logical_turn_continuation_tickets().unwrap().len(),
        1
    );
    let (_wake_subscription, wake_events) = restarted
        .subscribe_events(EventSubscription {
            event_kinds: vec![CoreEventKind::BrainWakeRequested],
            session_id: Some(session.session_id.clone()),
            agent_id: None,
            adapter_id: None,
        })
        .unwrap();
    assert_eq!(restarted.requeue_logical_turn_continuations().unwrap(), 1);
    assert!(matches!(
        wake_events.recv_timeout(Duration::from_secs(1)).unwrap(),
        CoreEvent::BrainWakeRequested { session_id } if session_id == session.session_id
    ));
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
    let execution = engine.session_execution_state(&session.session_id).unwrap();
    assert_eq!(execution.phase, SessionExecutionPhase::Idle);
    assert_eq!(
        execution.last_outcome,
        Some(SessionExecutionOutcome::Cancelled)
    );
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
fn logical_turn_cancellation_fences_an_active_running_epoch() {
    let data_dir = unique_data_dir("logical-turn-running-cancel");
    let engine = test_engine_with_data_dir(data_dir.clone());
    let session = engine
        .create_session(session_config(
            "logical-running-cancel-session",
            "logical-running-cancel-agent",
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
            execution_epoch_id: ExecutionEpochId::new("epoch-running"),
            claim_holder: "service-running".into(),
            claim_expires_at: "2026-06-19T00:01:00Z".into(),
            now: NOW.into(),
        })
        .unwrap();
    assert_eq!(claim.record.phase, LogicalTurnPhase::Running);

    let cancelled = engine
        .cancel_logical_turn(&LogicalTurnCancelRequest {
            logical_turn_id: claim.record.logical_turn_id,
            expected_revision: claim.record.revision,
            idempotency_key: "cancel-running".into(),
            reason_code: "operator_cancelled".into(),
            summary: "operator cancelled active running epoch".into(),
            now: NOW.into(),
        })
        .unwrap();
    assert_eq!(cancelled.record.phase, LogicalTurnPhase::Cancelled);
    assert!(cancelled.record.active_epoch_id.is_none());
    assert!(engine
        .logical_turn_continuation_tickets()
        .unwrap()
        .is_empty());
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
#[ignore = "focused >512-continuation certification"]
fn sqlite_logical_turn_survives_over_512_yields_restart_and_cancel() {
    let data_dir = unique_data_dir("logical-turn-513-sqlite");
    let mut engine = test_engine_with_data_dir(data_dir.clone());
    let session = engine
        .create_session(session_config(
            "logical-513-sqlite-session",
            "logical-513-sqlite-agent",
            "logical-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine
        .admit_logical_turn(&admission(&session.session_id))
        .unwrap();

    for sequence in 1..=513u64 {
        let turn = engine
            .get_logical_turn(&LogicalTurnId::new("turn-1"))
            .unwrap()
            .unwrap();
        let claim = engine
            .claim_logical_turn(&LogicalTurnClaimRequest {
                logical_turn_id: turn.logical_turn_id,
                expected_revision: turn.revision,
                continuation_id: turn.current_continuation_id,
                execution_epoch_id: ExecutionEpochId::new(format!("epoch-sqlite-{sequence}")),
                claim_holder: "service-sqlite".into(),
                claim_expires_at: "2026-06-19T00:01:00Z".into(),
                now: NOW.into(),
            })
            .unwrap();
        if sequence == 1 {
            let claim_generation = claim.record.claim_generation.unwrap();
            let leased = LogicalTurnOperationRecord {
                operation_id: BrainOperationId::new("operation:sqlite:1"),
                logical_turn_id: claim.record.logical_turn_id.clone(),
                continuation_id: claim.record.current_continuation_id.clone(),
                execution_epoch_id: claim.record.active_epoch_id.clone().unwrap(),
                kind: LogicalTurnOperationKind::HostToolExecution,
                phase: LogicalTurnOperationPhase::Leased,
                request_fingerprint: "sqlite-request".into(),
                idempotency_key: "operation:sqlite:1".into(),
                lease_holder: claim.record.claim_holder.clone(),
                lease_generation: Some(claim_generation),
                lease_expires_at: claim.record.claim_expires_at.clone(),
                result_ref: None,
                result_payload: None,
                reason_code: None,
                revision: 1,
                created_at: NOW.into(),
                updated_at: NOW.into(),
            };
            engine
                .lease_logical_turn_operation(&LogicalTurnOperationLeaseRequest {
                    operation: leased.clone(),
                    expected_turn_revision: claim.record.revision,
                    expected_claim_generation: claim_generation,
                    expected_cancellation_generation: claim.record.cancellation_generation,
                })
                .unwrap();
            let mut completed = leased;
            completed.phase = LogicalTurnOperationPhase::Completed;
            completed.result_ref = Some("sha256:sqlite-result".into());
            completed.result_payload = Some(json!({"output":"sqlite-ok"}));
            completed.revision = 2;
            assert_eq!(
                engine
                    .complete_logical_turn_operation(&LogicalTurnOperationCompletionRequest {
                        operation: completed,
                        expected_operation_revision: 1,
                        expected_turn_revision: claim.record.revision,
                        expected_claim_generation: claim_generation,
                        expected_cancellation_generation: claim.record.cancellation_generation,
                    },)
                    .unwrap()
                    .phase,
                LogicalTurnOperationPhase::Completed
            );
        }
        let request = yield_request_at(&claim, sequence);
        let yielded = engine.yield_logical_turn(&request).unwrap();
        assert_eq!(yielded.record.continuation_sequence, sequence);
        if sequence == 257 {
            assert!(engine.yield_logical_turn(&request).unwrap().replayed);
        }
        if sequence % 128 == 0 {
            drop(engine);
            engine = test_engine_with_data_dir(data_dir.clone());
            assert_eq!(
                engine
                    .get_logical_turn(&LogicalTurnId::new("turn-1"))
                    .unwrap()
                    .unwrap()
                    .phase,
                LogicalTurnPhase::Runnable
            );
        }
    }

    let sqlite_turn = engine
        .get_logical_turn(&LogicalTurnId::new("turn-1"))
        .unwrap()
        .expect("SQLite logical turn");
    let sqlite_compaction = engine
        .get_logical_turn_checkpoint(&sqlite_turn.current_continuation_id)
        .unwrap()
        .expect("SQLite continuation checkpoint");
    assert_eq!(
        sqlite_compaction.module_state.payload["contextCompaction"]["artifacts"][0]["sequence"],
        513
    );
    let diagnostic = engine
        .logical_turn_diagnostics(&LogicalTurnDiagnosticQuery {
            logical_turn_id: Some(LogicalTurnId::new("turn-1")),
            session_id: Some(session.session_id),
            include_terminal: false,
            limit: 1,
        })
        .unwrap()
        .items
        .remove(0);
    assert_eq!(diagnostic.continuation_count, 514);
    assert_eq!(diagnostic.provider_request_total, 514);
    assert_eq!(diagnostic.tool_round_total, 513);
    assert_eq!(
        diagnostic.operator_state,
        LogicalTurnOperatorState::QueuedToContinue
    );
    engine
        .cancel_logical_turn(&LogicalTurnCancelRequest {
            logical_turn_id: diagnostic.logical_turn_id,
            expected_revision: diagnostic.revision,
            idempotency_key: "cancel-sqlite-513".into(),
            reason_code: "certification_cancelled".into(),
            summary: "cancel SQLite turn after 513 continuations".into(),
            now: NOW.into(),
        })
        .unwrap();
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
            backing_filesystem_path: None,
            filesystem_warning_free_percent: None,
        }),
    };
    let mut engine = CoreEngine::initialize(config.clone()).unwrap();
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
    for sequence in 1..=513u64 {
        let turn = engine
            .get_logical_turn(&LogicalTurnId::new("turn-1"))
            .unwrap()
            .unwrap();
        let claim = engine
            .claim_logical_turn(&LogicalTurnClaimRequest {
                logical_turn_id: turn.logical_turn_id,
                expected_revision: turn.revision,
                continuation_id: turn.current_continuation_id,
                execution_epoch_id: ExecutionEpochId::new(format!("epoch-pg-{sequence}")),
                claim_holder: "service-pg".into(),
                claim_expires_at: "2026-06-19T00:01:00Z".into(),
                now: NOW.into(),
            })
            .unwrap();
        if sequence == 1 {
            let claim_generation = claim.record.claim_generation.unwrap();
            let leased = LogicalTurnOperationRecord {
                operation_id: BrainOperationId::new("operation:postgres:1"),
                logical_turn_id: claim.record.logical_turn_id.clone(),
                continuation_id: claim.record.current_continuation_id.clone(),
                execution_epoch_id: claim.record.active_epoch_id.clone().unwrap(),
                kind: LogicalTurnOperationKind::HostToolExecution,
                phase: LogicalTurnOperationPhase::Leased,
                request_fingerprint: "postgres-request".into(),
                idempotency_key: "operation:postgres:1".into(),
                lease_holder: claim.record.claim_holder.clone(),
                lease_generation: Some(claim_generation),
                lease_expires_at: claim.record.claim_expires_at.clone(),
                result_ref: None,
                result_payload: None,
                reason_code: None,
                revision: 1,
                created_at: NOW.into(),
                updated_at: NOW.into(),
            };
            engine
                .lease_logical_turn_operation(&LogicalTurnOperationLeaseRequest {
                    operation: leased.clone(),
                    expected_turn_revision: claim.record.revision,
                    expected_claim_generation: claim_generation,
                    expected_cancellation_generation: claim.record.cancellation_generation,
                })
                .unwrap();
            let mut completed = leased;
            completed.phase = LogicalTurnOperationPhase::Completed;
            completed.result_ref = Some("sha256:postgres-result".into());
            completed.result_payload = Some(json!({"output":"postgres-ok"}));
            completed.revision = 2;
            assert_eq!(
                engine
                    .complete_logical_turn_operation(&LogicalTurnOperationCompletionRequest {
                        operation: completed,
                        expected_operation_revision: 1,
                        expected_turn_revision: claim.record.revision,
                        expected_claim_generation: claim_generation,
                        expected_cancellation_generation: claim.record.cancellation_generation,
                    },)
                    .unwrap()
                    .phase,
                LogicalTurnOperationPhase::Completed
            );
        }
        let request = yield_request_at(&claim, sequence);
        let yielded = engine.yield_logical_turn(&request).unwrap();
        assert_eq!(yielded.record.continuation_sequence, sequence);
        if sequence == 257 {
            assert!(engine.yield_logical_turn(&request).unwrap().replayed);
            drop(engine);
            engine = CoreEngine::initialize(config.clone()).unwrap();
            assert_eq!(
                engine
                    .get_logical_turn(&LogicalTurnId::new("turn-1"))
                    .unwrap()
                    .unwrap()
                    .phase,
                LogicalTurnPhase::Runnable
            );
        }
    }

    let turn = engine
        .get_logical_turn(&LogicalTurnId::new("turn-1"))
        .unwrap()
        .unwrap();
    assert_eq!(turn.continuation_sequence, 513);
    assert_eq!(turn.phase, LogicalTurnPhase::Yielded);
    let persisted_compaction = engine
        .get_logical_turn_checkpoint(&turn.current_continuation_id)
        .unwrap()
        .expect("PostgreSQL continuation checkpoint");
    assert_eq!(
        persisted_compaction.module_state.payload["contextCompaction"]["artifacts"][0]["sequence"],
        513
    );
    assert_eq!(
        engine
            .list_logical_turn_operations(&LogicalTurnId::new("turn-1"))
            .unwrap()
            .len(),
        1
    );
    engine
        .cancel_logical_turn(&LogicalTurnCancelRequest {
            logical_turn_id: turn.logical_turn_id,
            expected_revision: turn.revision,
            idempotency_key: "cancel-pg".into(),
            reason_code: "parity_test_cancel".into(),
            summary: "cancel PostgreSQL parity turn".into(),
            now: NOW.into(),
        })
        .unwrap();
    let outcome_session = engine
        .create_session(session_config(
            "logical-postgres-outcome-session",
            "logical-postgres-outcome-agent",
            "prepared-logical-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let outcome = engine
        .prepare_logical_turn_wake(
            &chat_completions_registration(),
            &outcome_session.session_id,
            "logical-postgres-outcome-wake",
            "system prompt".into(),
            br#"{"messages":[]}"#.to_vec(),
        )
        .unwrap();
    let outcome_turn_id = outcome.claim.record.logical_turn_id.clone();
    let outcome_operation_id = BrainOperationId::new("operation:postgres:outcome");
    engine
        .lease_logical_turn_operation(&LogicalTurnOperationLeaseRequest {
            operation: LogicalTurnOperationRecord {
                operation_id: outcome_operation_id.clone(),
                logical_turn_id: outcome_turn_id.clone(),
                continuation_id: outcome.claim.record.current_continuation_id.clone(),
                execution_epoch_id: outcome.claim.record.active_epoch_id.clone().unwrap(),
                kind: LogicalTurnOperationKind::HostToolExecution,
                phase: LogicalTurnOperationPhase::Leased,
                request_fingerprint: "postgres-outcome-request".into(),
                idempotency_key: outcome_operation_id.0,
                lease_holder: outcome.claim.record.claim_holder.clone(),
                lease_generation: outcome.claim.record.claim_generation,
                lease_expires_at: outcome.claim.record.claim_expires_at.clone(),
                result_ref: None,
                result_payload: None,
                reason_code: None,
                revision: 1,
                created_at: NOW.into(),
                updated_at: NOW.into(),
            },
            expected_turn_revision: outcome.claim.record.revision,
            expected_claim_generation: outcome.claim.record.claim_generation.unwrap(),
            expected_cancellation_generation: outcome.claim.record.cancellation_generation,
        })
        .unwrap();
    drop(engine);

    let restarted = CoreEngine::initialize(config).unwrap();
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
    let outcome_attention = restarted
        .get_logical_turn(&outcome_turn_id)
        .unwrap()
        .unwrap();
    assert_eq!(outcome_attention.phase, LogicalTurnPhase::AttentionRequired);
    restarted
        .resolve_logical_turn_attention_for_operator(
            &outcome_turn_id,
            outcome_attention.revision,
            LogicalTurnResolutionAction::ConfirmToolNotCompleted,
        )
        .unwrap();
    assert_eq!(
        restarted
            .list_logical_turn_operations(&outcome_turn_id)
            .unwrap()[0]
            .phase,
        LogicalTurnOperationPhase::Superseded
    );
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
    yield_request_at(claim, 1)
}

fn yield_request_at(
    claim: &rusty_crew_core_protocol::LogicalTurnContinuationClaim,
    sequence: u64,
) -> LogicalTurnYieldRequest {
    let fingerprint = format!("yielded-{sequence}");
    let mut progress = progress(sequence, &fingerprint);
    progress.committed_provider_operations = sequence + 1;
    progress.committed_tool_operations = sequence;
    let checkpoint = LogicalTurnCheckpoint {
        continuation_id: ContinuationId::new(format!("continuation-{sequence}")),
        logical_turn_id: claim.record.logical_turn_id.clone(),
        sequence,
        parent_continuation_id: Some(claim.record.current_continuation_id.clone()),
        completed_epoch_id: claim.record.active_epoch_id.clone(),
        binding_generation: claim.record.binding_generation,
        frozen_input: claim.checkpoint.frozen_input.clone(),
        module_state: BrainContinuationPayload {
            module_id: "test-brain".into(),
            payload_version: "1".into(),
            payload_fingerprint: fingerprint,
            payload: json!({
                "cursor": sequence,
                "contextCompaction": {
                    "artifacts": [{
                        "sequence": sequence,
                        "strategyId": "rolling_summary_compaction",
                        "reasonCode": "context_fill_threshold_exceeded",
                        "summaryText": format!("compaction {sequence}")
                    }]
                }
            }),
        },
        operation_cursor: sequence,
        projection_cursor: sequence,
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
            projection_id: TurnProjectionId::new(format!(
                "projection:turn-1:{}:yielded",
                claim.record.revision + 1
            )),
            logical_turn_id: claim.record.logical_turn_id.clone(),
            session_id: claim.record.session_id.clone(),
            wake_id: claim.record.source_wake_id.clone(),
            continuation_id: checkpoint.continuation_id.clone(),
            execution_epoch_id: claim.record.active_epoch_id.clone(),
            kind: LogicalTurnLifecycleEventKind::ContinuationYielded,
            phase: LogicalTurnPhase::Yielded,
            continuation_count: sequence + 1,
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
