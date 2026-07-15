use super::*;
use rusty_crew_core_protocol::{
    AgentActivation, AgentCoordinationCaller, AgentMessageCommand, AgentMessageDeliveryId,
    AgentMessageDeliveryStatus, AgentRoundCommand, AgentRoundId, AgentRoundStatus,
    DenRuntimeReference, ExternalAgentBinding, ExternalAgentBindingMetadataWrite,
    ExternalAgentSessionCreationPhase, ExternalAgentSessionCreationRequest, ExternalBindingId,
    ExternalBindingPurpose, ExternalBindingStatus, ExternalCollaborationMode, ExternalControlId,
    ExternalControlKind, ExternalControlRequest, ExternalControlStatus, ExternalControllerContext,
    ExternalControllerLease, ExternalEndpoint, ExternalEndpointTransport,
    ExternalMessageDeliveryPolicy, ExternalProcessOwnership,
    ExternalRuntimeCompatibilityProbeOutcome, ExternalRuntimeCompatibilityProbeReport,
    ExternalRuntimeCompatibilityProbeStep, ExternalRuntimeCompatibilityProbeStepStatus,
    ExternalRuntimeCompatibilityState, ExternalRuntimeDesiredState, ExternalRuntimeEventInput,
    ExternalRuntimeHandshakeObservation, ExternalRuntimeId, ExternalRuntimeKind,
    ExternalRuntimeObservedState, ExternalRuntimeRegistration, ExternalTurnInputPart,
    ExternalTurnPhase, ExternalTurnRequestId, TurnInputProvenance, TurnInputProvenanceKind,
};
use serde_json::json;

#[test]
fn external_binding_metadata_is_revisioned_and_survives_restart() {
    let data_dir = unique_data_dir("external-binding-metadata");
    let engine = test_engine_with_data_dir(data_dir.clone());
    engine
        .create_session(session_config(
            "codex-session",
            "codex-agent",
            "codex-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine.register_external_runtime(&runtime(), None).unwrap();
    let binding = engine.bind_external_agent(&binding(), None).unwrap();

    let saved = engine
        .update_external_binding_metadata(&ExternalAgentBindingMetadataWrite {
            binding_id: binding.binding_id.clone(),
            expected_revision: binding.revision,
            label: Some("Asha implementation".into()),
            task_ref: Some(DenRuntimeReference {
                project_id: Some(ProjectId::new("asha")),
                task_id: Some(TaskId::new("4281")),
            }),
            updated_at: "2026-07-13T10:00:00Z".into(),
        })
        .unwrap();
    assert_eq!(saved.label.as_deref(), Some("Asha implementation"));
    assert_eq!(saved.revision, binding.revision + 1);

    let stale = engine
        .update_external_binding_metadata(&ExternalAgentBindingMetadataWrite {
            binding_id: binding.binding_id.clone(),
            expected_revision: binding.revision,
            label: Some("stale".into()),
            task_ref: None,
            updated_at: "2026-07-13T10:00:01Z".into(),
        })
        .unwrap_err();
    assert_eq!(stale.kind, CoreErrorKind::ActionRejected);
    assert!(stale
        .message
        .contains("external_binding_metadata_revision_conflict"));

    drop(engine);
    let restarted = test_engine_with_data_dir(data_dir);
    let hydrated = restarted
        .get_external_binding(&binding.binding_id)
        .unwrap()
        .unwrap();
    assert_eq!(hydrated.label, saved.label);
    assert_eq!(hydrated.task_ref, saved.task_ref);

    let cleared = restarted
        .update_external_binding_metadata(&ExternalAgentBindingMetadataWrite {
            binding_id: hydrated.binding_id,
            expected_revision: hydrated.revision,
            label: None,
            task_ref: None,
            updated_at: "2026-07-13T10:00:02Z".into(),
        })
        .unwrap();
    assert_eq!(cleared.label, None);
    assert_eq!(cleared.task_ref, None);
}

#[test]
fn external_agent_session_creation_is_idempotent_and_recovers_native_start() {
    let engine = test_engine();
    let controller = ready_external_creation_dependencies(&engine);
    let request = external_creation_request("create-agent-1");

    let prepared = engine
        .prepare_external_agent_session_creation(request.clone())
        .unwrap();
    assert_eq!(
        prepared.phase,
        ExternalAgentSessionCreationPhase::BindingReady
    );
    assert_eq!(prepared.session.profile_id, ProfileId::new("codex-profile"));
    assert_eq!(
        prepared.binding.session_id,
        Some(prepared.session.session_id.clone())
    );
    assert_eq!(prepared.binding.native_thread_id, None);
    assert_eq!(
        prepared.native_thread_source.len(),
        "rusty-crew:".len() + 24
    );

    let mut retry = request.clone();
    retry.requested_at = "2026-06-19T00:00:05Z".into();
    assert_eq!(
        engine
            .prepare_external_agent_session_creation(retry)
            .unwrap(),
        prepared
    );

    let starting = engine
        .mark_external_agent_session_native_starting(
            &controller,
            &prepared.creation_id,
            prepared.revision,
            "2026-06-19T00:00:06Z".into(),
        )
        .unwrap();
    let recovering = engine
        .record_external_agent_session_creation_failure(
            &controller,
            &starting.creation_id,
            starting.revision,
            "external_agent_creation_native_start_failed".into(),
            "native transport disconnected".into(),
            "2026-06-19T00:00:07Z".into(),
        )
        .unwrap();
    assert_eq!(
        recovering.phase,
        ExternalAgentSessionCreationPhase::RecoveryRequired
    );

    let restarting = engine
        .mark_external_agent_session_native_starting(
            &controller,
            &recovering.creation_id,
            recovering.revision,
            "2026-06-19T00:00:08Z".into(),
        )
        .unwrap();
    let ready = engine
        .complete_external_agent_session_creation(
            &controller,
            &restarting.creation_id,
            restarting.revision,
            "native-thread-created".into(),
            "2026-06-19T00:00:09Z".into(),
        )
        .unwrap();
    assert_eq!(ready.phase, ExternalAgentSessionCreationPhase::Ready);
    assert_eq!(
        ready.binding.native_thread_id.as_deref(),
        Some("native-thread-created")
    );
    assert_eq!(
        engine
            .prepare_external_agent_session_creation(request)
            .unwrap(),
        ready
    );
}

#[test]
fn external_agent_creation_reads_explicit_serial_delivery_policy_from_profile() {
    let engine = test_engine();
    let mut profile = profile_registry_write("codex-profile", "gpt", "configured-codex-session");
    profile.active_runtime_settings_json["externalMessageDeliveryPolicy"] =
        json!("serial_next_turn");
    engine.create_profile_registry_record(&profile).unwrap();
    engine.register_external_runtime(&runtime(), None).unwrap();
    engine
        .acquire_external_runtime_controller(
            &external_controller_lease(),
            &"2026-06-19T00:00:00Z".into(),
        )
        .unwrap();

    let prepared = engine
        .prepare_external_agent_session_creation(external_creation_request("serial-create"))
        .unwrap();
    assert_eq!(
        prepared.binding.message_delivery_policy,
        ExternalMessageDeliveryPolicy::SerialNextTurn
    );
}

#[test]
fn external_agent_session_creation_rejects_changed_retry_and_invalid_dependencies() {
    let engine = test_engine();
    ready_external_creation_dependencies(&engine);
    let request = external_creation_request("create-agent-conflict");
    engine
        .prepare_external_agent_session_creation(request.clone())
        .unwrap();

    let mut changed = request;
    changed.label = Some("different label".into());
    let conflict = engine
        .prepare_external_agent_session_creation(changed)
        .unwrap_err();
    assert_eq!(conflict.kind, CoreErrorKind::AlreadyExists);
    assert!(conflict
        .message
        .contains("external_agent_creation_idempotency_conflict"));

    let missing_runtime = test_engine();
    missing_runtime
        .create_profile_registry_record(&profile_registry_write(
            "codex-profile",
            "gpt",
            "configured-codex-session",
        ))
        .unwrap();
    let error = missing_runtime
        .prepare_external_agent_session_creation(external_creation_request("missing-runtime"))
        .unwrap_err();
    assert!(error
        .message
        .contains("external_agent_creation_runtime_unavailable"));

    let missing_profile = test_engine();
    missing_profile
        .register_external_runtime(&runtime(), None)
        .unwrap();
    missing_profile
        .acquire_external_runtime_controller(
            &external_controller_lease(),
            &"2026-06-19T00:00:00Z".into(),
        )
        .unwrap();
    let error = missing_profile
        .prepare_external_agent_session_creation(external_creation_request("missing-profile"))
        .unwrap_err();
    assert!(error
        .message
        .contains("external_agent_creation_profile_invalid"));

    let mut invalid_cwd = external_creation_request("invalid-cwd");
    invalid_cwd.cwd = "/home/dev/../dev/rusty-crew".into();
    let error = engine
        .prepare_external_agent_session_creation(invalid_cwd)
        .unwrap_err();
    assert!(error
        .message
        .contains("external_agent_creation_cwd_invalid"));
}

#[test]
fn external_agent_session_creation_recovers_binding_correlation_before_ready_record() {
    let engine = test_engine();
    let controller = ready_external_creation_dependencies(&engine);
    let request = external_creation_request("binding-correlated-before-ready");
    let prepared = engine
        .prepare_external_agent_session_creation(request.clone())
        .unwrap();
    let starting = engine
        .mark_external_agent_session_native_starting(
            &controller,
            &prepared.creation_id,
            prepared.revision,
            "2026-06-19T00:00:02Z".into(),
        )
        .unwrap();
    let mut correlated_binding = starting.binding.clone();
    correlated_binding.native_thread_id = Some("native-thread-before-ready".into());
    correlated_binding.updated_at = "2026-06-19T00:00:03Z".into();
    engine
        .bind_external_agent(&correlated_binding, Some(starting.binding.revision))
        .unwrap();

    let recovered = engine
        .prepare_external_agent_session_creation(request)
        .unwrap();
    assert_eq!(recovered.phase, ExternalAgentSessionCreationPhase::Ready);
    assert_eq!(
        recovered.native_thread_id.as_deref(),
        Some("native-thread-before-ready")
    );
    assert_eq!(
        recovered.binding.native_thread_id,
        recovered.native_thread_id
    );
}

#[test]
fn handshake_and_runtime_event_replay_require_current_controller_authority() {
    let engine = test_engine();
    engine.register_external_runtime(&runtime(), None).unwrap();
    let lease = engine
        .acquire_external_runtime_controller(
            &ExternalControllerLease {
                runtime_id: ExternalRuntimeId::new("codex-local"),
                holder_instance_id: "controller-a".into(),
                generation: 0,
                acquired_at: "2026-06-19T00:00:00Z".into(),
                renewed_at: "2026-06-19T00:00:00Z".into(),
                expires_at: "2026-06-19T00:10:00Z".into(),
                revision: 0,
            },
            &"2026-06-19T00:00:00Z".into(),
        )
        .unwrap();
    let controller = ExternalControllerContext {
        holder_instance_id: "controller-a".into(),
        generation: lease.generation,
    };
    let accepted = engine
        .authorize_external_runtime_handshake(&ExternalRuntimeHandshakeObservation {
            runtime_id: ExternalRuntimeId::new("codex-local"),
            controller: controller.clone(),
            cli_version: "0.144.3".into(),
            consumed_contract_revision: "contract-v1".into(),
            probe_report: probe_report(ExternalRuntimeCompatibilityProbeOutcome::Passed, None),
            observed_at: "2026-06-19T00:00:01Z".into(),
        })
        .unwrap();
    assert!(accepted.accepted);
    assert!(!accepted.retryable);
    assert_eq!(
        accepted.compatibility_state,
        ExternalRuntimeCompatibilityState::CompatibleUncertified
    );
    assert_eq!(
        accepted.registration.observed_cli_version.as_deref(),
        Some("0.144.3")
    );
    assert_eq!(
        accepted
            .registration
            .last_compatibility_probe
            .as_ref()
            .map(|report| report.outcome),
        Some(ExternalRuntimeCompatibilityProbeOutcome::Passed)
    );

    let first = engine
        .record_external_runtime_event(
            &controller,
            &ExternalRuntimeEventInput {
                event_id: "connection-1:event-1".into(),
                session_id: None,
                created_at: "2026-06-19T00:00:02Z".into(),
                kind: "runtime_status".into(),
                runtime_id: ExternalRuntimeId::new("codex-local"),
                native_thread_id: None,
                native_turn_id: None,
                item_id: None,
                request_id: None,
                payload: json!({"transportSequence": 1}),
                raw_detail_ref: None,
            },
        )
        .unwrap();
    let after_reconnect = engine
        .record_external_runtime_event(
            &controller,
            &ExternalRuntimeEventInput {
                event_id: "connection-2:event-1".into(),
                session_id: None,
                created_at: "2026-06-19T00:00:03Z".into(),
                kind: "runtime_status".into(),
                runtime_id: ExternalRuntimeId::new("codex-local"),
                native_thread_id: None,
                native_turn_id: None,
                item_id: None,
                request_id: None,
                payload: json!({"transportSequence": 1}),
                raw_detail_ref: None,
            },
        )
        .unwrap();
    assert_eq!(first.sequence_id, 1);
    assert_eq!(after_reconnect.sequence_id, 2);

    let stale = ExternalControllerContext {
        holder_instance_id: "controller-a".into(),
        generation: lease.generation + 1,
    };
    assert_eq!(
        engine
            .record_external_runtime_event(
                &stale,
                &ExternalRuntimeEventInput {
                    event_id: "stale:event".into(),
                    session_id: None,
                    created_at: "2026-06-19T00:00:04Z".into(),
                    kind: "runtime_status".into(),
                    runtime_id: ExternalRuntimeId::new("codex-local"),
                    native_thread_id: None,
                    native_turn_id: None,
                    item_id: None,
                    request_id: None,
                    payload: json!({}),
                    raw_detail_ref: None,
                },
            )
            .unwrap_err()
            .kind,
        CoreErrorKind::ActionRejected
    );
}

#[test]
fn required_contract_failure_is_incompatible_without_version_pinning() {
    let engine = test_engine();
    engine.register_external_runtime(&runtime(), None).unwrap();
    let lease = engine
        .acquire_external_runtime_controller(
            &ExternalControllerLease {
                runtime_id: ExternalRuntimeId::new("codex-local"),
                holder_instance_id: "controller-a".into(),
                generation: 0,
                acquired_at: "2026-06-19T00:00:00Z".into(),
                renewed_at: "2026-06-19T00:00:00Z".into(),
                expires_at: "2026-06-19T00:10:00Z".into(),
                revision: 0,
            },
            &"2026-06-19T00:00:00Z".into(),
        )
        .unwrap();
    let decision = engine
        .authorize_external_runtime_handshake(&ExternalRuntimeHandshakeObservation {
            runtime_id: ExternalRuntimeId::new("codex-local"),
            controller: ExternalControllerContext {
                holder_instance_id: "controller-a".into(),
                generation: lease.generation,
            },
            cli_version: "0.200.0".into(),
            consumed_contract_revision: "contract-v1".into(),
            probe_report: probe_report(
                ExternalRuntimeCompatibilityProbeOutcome::Incompatible,
                Some("external_runtime_required_method_missing"),
            ),
            observed_at: "2026-06-19T00:00:01Z".into(),
        })
        .unwrap();

    assert!(!decision.accepted);
    assert!(!decision.retryable);
    assert_eq!(
        decision.compatibility_state,
        ExternalRuntimeCompatibilityState::Incompatible
    );
    assert_eq!(
        decision.reason_code.as_deref(),
        Some("external_runtime_required_method_missing")
    );
    assert_eq!(
        decision.registration.observed_state,
        ExternalRuntimeObservedState::Incompatible
    );
}

#[test]
fn transport_probe_failure_is_retryable_without_claiming_incompatibility() {
    let engine = test_engine();
    engine.register_external_runtime(&runtime(), None).unwrap();
    let lease = engine
        .acquire_external_runtime_controller(
            &ExternalControllerLease {
                runtime_id: ExternalRuntimeId::new("codex-local"),
                holder_instance_id: "controller-a".into(),
                generation: 0,
                acquired_at: "2026-06-19T00:00:00Z".into(),
                renewed_at: "2026-06-19T00:00:00Z".into(),
                expires_at: "2026-06-19T00:10:00Z".into(),
                revision: 0,
            },
            &"2026-06-19T00:00:00Z".into(),
        )
        .unwrap();
    let decision = engine
        .authorize_external_runtime_handshake(&ExternalRuntimeHandshakeObservation {
            runtime_id: ExternalRuntimeId::new("codex-local"),
            controller: ExternalControllerContext {
                holder_instance_id: "controller-a".into(),
                generation: lease.generation,
            },
            cli_version: "0.200.0".into(),
            consumed_contract_revision: "contract-v1".into(),
            probe_report: probe_report(
                ExternalRuntimeCompatibilityProbeOutcome::TransportRetryable,
                Some("external_runtime_probe_transport_retryable"),
            ),
            observed_at: "2026-06-19T00:00:01Z".into(),
        })
        .unwrap();

    assert!(!decision.accepted);
    assert!(decision.retryable);
    assert_eq!(
        decision.compatibility_state,
        ExternalRuntimeCompatibilityState::Unassessed
    );
    assert_eq!(
        decision.registration.observed_state,
        ExternalRuntimeObservedState::Degraded
    );
}

#[test]
fn mid_turn_controls_require_exact_native_turn_identity() {
    let engine = test_engine();
    engine
        .create_session(session_config(
            "codex-session",
            "codex-agent",
            "codex-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine.register_external_runtime(&runtime(), None).unwrap();
    engine.bind_external_agent(&binding(), None).unwrap();
    let request = ExternalControlRequest {
        control_id: ExternalControlId::new("steer-without-turn"),
        idempotency_key: "steer-without-turn".into(),
        binding_id: ExternalBindingId::new("codex-binding"),
        expected_binding_revision: 1,
        expected_native_turn_id: None,
        kind: ExternalControlKind::SteerTurn,
        payload: json!({}),
        requested_at: "2026-06-19T00:00:00Z".into(),
    };
    assert_eq!(
        engine.submit_external_control(request).unwrap_err().kind,
        CoreErrorKind::InvalidInput
    );
}

#[test]
fn external_thread_commands_are_validated_and_replay_by_semantic_idempotency() {
    let engine = test_engine();
    engine
        .create_session(session_config(
            "codex-session",
            "codex-agent",
            "codex-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine.register_external_runtime(&runtime(), None).unwrap();
    engine.bind_external_agent(&binding(), None).unwrap();

    let request = ExternalControlRequest {
        control_id: ExternalControlId::new("command-status-a"),
        idempotency_key: "command-status-key".into(),
        binding_id: ExternalBindingId::new("codex-binding"),
        expected_binding_revision: 1,
        expected_native_turn_id: None,
        kind: ExternalControlKind::ExecuteThreadCommand,
        payload: json!({"command": "status", "argument": null}),
        requested_at: "2026-06-19T00:00:00Z".into(),
    };
    let first = engine.submit_external_control(request.clone()).unwrap();
    assert_eq!(first.status, ExternalControlStatus::Pending);

    let mut replay = request.clone();
    replay.control_id = ExternalControlId::new("command-status-retry");
    replay.requested_at = "2026-06-19T00:00:05Z".into();
    assert_eq!(engine.submit_external_control(replay).unwrap(), first);

    let mut conflict = request;
    conflict.control_id = ExternalControlId::new("command-status-conflict");
    conflict.payload = json!({"command": "model", "argument": "gpt-5.4"});
    assert_eq!(
        engine.submit_external_control(conflict).unwrap_err().kind,
        CoreErrorKind::AlreadyExists
    );

    let current_binding = engine
        .get_external_binding(&ExternalBindingId::new("codex-binding"))
        .unwrap()
        .unwrap();
    let mut revised_binding = current_binding.clone();
    revised_binding.updated_at = "2026-06-19T00:00:04Z".into();
    engine
        .bind_external_agent(&revised_binding, Some(current_binding.revision))
        .unwrap();
    let mut replay_after_revision = first.request.clone();
    replay_after_revision.expected_binding_revision = revised_binding.revision;
    replay_after_revision.requested_at = "2026-06-19T00:00:05Z".into();
    assert_eq!(
        engine
            .submit_external_control(replay_after_revision)
            .unwrap(),
        first
    );

    let invalid = ExternalControlRequest {
        control_id: ExternalControlId::new("command-unknown"),
        idempotency_key: "command-unknown-key".into(),
        binding_id: ExternalBindingId::new("codex-binding"),
        expected_binding_revision: 2,
        expected_native_turn_id: None,
        kind: ExternalControlKind::ExecuteThreadCommand,
        payload: json!({"command": "future-command", "argument": null}),
        requested_at: "2026-06-19T00:00:06Z".into(),
    };
    assert_eq!(
        engine.submit_external_control(invalid).unwrap_err().kind,
        CoreErrorKind::InvalidInput
    );
}

#[test]
fn archived_session_allows_binding_archival_but_not_reactivation() {
    let data_dir = unique_data_dir("external-explicit-archive");
    let engine = test_engine_with_data_dir(data_dir.clone());
    engine
        .create_session(session_config(
            "codex-session",
            "codex-agent",
            "codex-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine.register_external_runtime(&runtime(), None).unwrap();
    engine.bind_external_agent(&binding(), None).unwrap();
    engine
        .archive_session(&SessionId::new("codex-session"))
        .unwrap();

    let archived = engine
        .get_external_binding(&ExternalBindingId::new("codex-binding"))
        .unwrap()
        .unwrap();
    assert_eq!(archived.status, ExternalBindingStatus::Archived);

    let mut active = archived.clone();
    active.status = ExternalBindingStatus::Active;
    active.updated_at = "2026-06-19T00:00:01Z".into();
    assert_eq!(
        engine
            .bind_external_agent(&active, Some(archived.revision))
            .unwrap_err()
            .kind,
        CoreErrorKind::SessionExpired
    );

    drop(engine);
    let restarted = test_engine_with_data_dir(data_dir.clone());
    assert_eq!(
        restarted
            .get_session(&SessionId::new("codex-session"))
            .unwrap()
            .status,
        SessionStatus::Archived
    );
    assert_eq!(
        restarted
            .get_external_binding(&ExternalBindingId::new("codex-binding"))
            .unwrap()
            .unwrap()
            .status,
        ExternalBindingStatus::Archived
    );
    drop(restarted);
    let _ = std::fs::remove_dir_all(data_dir);
}

#[test]
fn shutdown_preserves_active_external_session_and_native_thread_binding() {
    let data_dir = unique_data_dir("external-clean-restart");
    {
        let engine = test_engine_with_data_dir(data_dir.clone());
        engine
            .create_session(session_config(
                "codex-session",
                "codex-agent",
                "codex-profile",
                SessionKind::Full,
            ))
            .unwrap();
        engine.register_external_runtime(&runtime(), None).unwrap();
        engine.bind_external_agent(&binding(), None).unwrap();

        let summary = engine.shutdown_with_timeout(25).unwrap();
        assert_eq!(summary.archived_sessions, 0);
    }

    let restarted = test_engine_with_data_dir(data_dir.clone());
    assert_ne!(
        restarted
            .get_session(&SessionId::new("codex-session"))
            .unwrap()
            .status,
        SessionStatus::Archived
    );
    let binding = restarted
        .get_external_binding(&ExternalBindingId::new("codex-binding"))
        .unwrap()
        .unwrap();
    assert_eq!(binding.status, ExternalBindingStatus::Active);
    assert_eq!(binding.native_thread_id.as_deref(), Some("native-thread-7"));
    assert!(matches!(
        restarted
            .activate_agent_execution(activation("codex-agent", "restart-request"))
            .unwrap(),
        AgentActivation::ExternalTurnRequested { .. }
    ));
    drop(restarted);
    let _ = std::fs::remove_dir_all(data_dir);
}

#[test]
fn agent_directory_projects_same_service_direct_and_external_routability() {
    let engine = test_engine();
    let mut direct_profile =
        profile_registry_write("direct-profile", "tester-chat", "direct-session");
    direct_profile.display_name = Some("Direct planner".into());
    engine
        .create_profile_registry_record(&direct_profile)
        .unwrap();
    engine
        .create_session(session_config(
            "direct-session",
            "direct-agent",
            "direct-profile",
            SessionKind::Full,
        ))
        .unwrap();

    let mut external_profile = profile_registry_write("codex-profile", "gpt", "codex-session");
    external_profile.display_name = Some("Codex coder".into());
    engine
        .create_profile_registry_record(&external_profile)
        .unwrap();
    engine
        .create_session(session_config(
            "codex-session",
            "codex-agent",
            "codex-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine.register_external_runtime(&runtime(), None).unwrap();
    let saved_binding = engine.bind_external_agent(&binding(), None).unwrap();

    let directory = engine.list_agent_directory().unwrap();
    assert_eq!(directory.len(), 2);
    let direct = directory
        .iter()
        .find(|entry| entry.agent_id == AgentId::new("direct-agent"))
        .unwrap();
    assert_eq!(direct.display_label, "Direct planner");
    assert_eq!(direct.runtime_kind, AgentDirectoryRuntimeKind::DirectBrain);
    assert!(direct.routable);
    assert!(direct.binding_id.is_none());

    let external = directory
        .iter()
        .find(|entry| entry.agent_id == AgentId::new("codex-agent"))
        .unwrap();
    assert_eq!(external.display_label, "Codex coder");
    assert_eq!(
        external.runtime_kind,
        AgentDirectoryRuntimeKind::CodexAppServer
    );
    assert_eq!(
        external.binding_id,
        Some(ExternalBindingId::new("codex-binding"))
    );
    assert!(external.routable);

    let mut paused = saved_binding.clone();
    paused.status = ExternalBindingStatus::Paused;
    paused.updated_at = "2026-06-19T00:00:01Z".into();
    engine
        .bind_external_agent(&paused, Some(saved_binding.revision))
        .unwrap();
    let paused = engine
        .list_agent_directory()
        .unwrap()
        .into_iter()
        .find(|entry| entry.agent_id == AgentId::new("codex-agent"))
        .unwrap();
    assert!(!paused.routable);
    assert_eq!(
        paused.routability_reason_code.as_deref(),
        Some("external_binding_not_active")
    );
}

#[test]
fn restart_repairs_legacy_active_binding_with_archived_session() {
    let data_dir = unique_data_dir("external-legacy-archive-repair");
    {
        let engine = test_engine_with_data_dir(data_dir.clone());
        engine
            .create_session(session_config(
                "codex-session",
                "codex-agent",
                "codex-profile",
                SessionKind::Full,
            ))
            .unwrap();
        engine.register_external_runtime(&runtime(), None).unwrap();
        engine.bind_external_agent(&binding(), None).unwrap();

        let archived = engine
            .sessions
            .archive_session(&SessionId::new("codex-session"), engine.now())
            .unwrap();
        save_engine_session(&engine.store, &archived).unwrap();
    }

    let restarted = test_engine_with_data_dir(data_dir.clone());
    let session = restarted
        .get_session(&SessionId::new("codex-session"))
        .unwrap();
    assert_eq!(session.status, SessionStatus::Idle);
    assert_eq!(session.handle.get(), 1);
    let binding = restarted
        .get_external_binding(&ExternalBindingId::new("codex-binding"))
        .unwrap()
        .unwrap();
    assert_eq!(binding.status, ExternalBindingStatus::Active);
    assert_eq!(binding.native_thread_id.as_deref(), Some("native-thread-7"));
    drop(restarted);
    let _ = std::fs::remove_dir_all(data_dir);
}

#[test]
fn durable_agent_round_resolves_reply_without_second_wake() {
    let data_dir = unique_data_dir("agent-round-reply");
    let engine = test_engine_with_data_dir(data_dir.clone());
    let sender = engine
        .create_session(session_config(
            "sender-session",
            "sender-agent",
            "sender-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let recipient = engine
        .create_session(session_config(
            "recipient-session",
            "recipient-agent",
            "recipient-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let (_subscription_id, wakes) = engine
        .subscribe_events(EventSubscription {
            event_kinds: vec![CoreEventKind::BrainWakeRequested],
            session_id: Some(sender.session_id.clone()),
            agent_id: None,
            adapter_id: None,
        })
        .unwrap();
    let (_observation_subscription_id, observations) = engine
        .subscribe_events(EventSubscription {
            event_kinds: vec![
                CoreEventKind::AgentMessageDeliveryObserved,
                CoreEventKind::AgentRoundObserved,
            ],
            session_id: None,
            agent_id: None,
            adapter_id: None,
        })
        .unwrap();

    let started = engine
        .begin_agent_round(AgentRoundCommand {
            caller: AgentCoordinationCaller::DirectBrain {
                session_id: sender.session_id.clone(),
                wake_id: "sender-wake".into(),
                tool_call_id: "round-call".into(),
            },
            round_id: AgentRoundId::new("round-1"),
            idempotency_key: "round-key-1".into(),
            message_id: "round-message-1".into(),
            to_agent_id: recipient.agent_id.clone(),
            body: "please inspect this".into(),
            correlation_id: "round-correlation-1".into(),
            created_at: "2026-06-19T00:00:00Z".into(),
            expires_at: "2026-06-19T00:05:00Z".into(),
        })
        .unwrap();
    assert_eq!(started.round.status, AgentRoundStatus::Pending);
    assert_eq!(
        started.delivery.status,
        AgentMessageDeliveryStatus::Accepted
    );
    assert!(matches!(
        observations.recv_timeout(Duration::from_secs(1)).unwrap(),
        CoreEvent::AgentRoundObserved { round } if round.status == AgentRoundStatus::Pending
    ));
    assert!(matches!(
        observations.recv_timeout(Duration::from_secs(1)).unwrap(),
        CoreEvent::AgentMessageDeliveryObserved { receipt }
            if receipt.status == AgentMessageDeliveryStatus::Accepted
    ));

    let reply = engine
        .deliver_agent_message(AgentMessageCommand {
            caller: AgentCoordinationCaller::DirectBrain {
                session_id: recipient.session_id,
                wake_id: "recipient-wake".into(),
                tool_call_id: "reply-call".into(),
            },
            delivery_id: AgentMessageDeliveryId::new("reply-delivery-1"),
            idempotency_key: "reply-delivery-key-1".into(),
            message_id: "reply-message-1".into(),
            to_agent_id: sender.agent_id,
            body: "inspection complete".into(),
            collaboration_mode: None,
            correlation_id: Some("round-correlation-1".into()),
            require_wake: true,
            created_at: "2026-06-19T00:00:01Z".into(),
            expires_at: "2026-06-19T00:05:00Z".into(),
        })
        .unwrap();
    assert_eq!(reply.status, AgentMessageDeliveryStatus::Accepted);
    assert_eq!(reply.resolved_round_id, Some(AgentRoundId::new("round-1")));
    assert!(reply.activation.is_none());
    assert!(matches!(
        observations.recv_timeout(Duration::from_secs(1)).unwrap(),
        CoreEvent::AgentRoundObserved { round } if round.status == AgentRoundStatus::Replied
    ));
    assert!(matches!(
        observations.recv_timeout(Duration::from_secs(1)).unwrap(),
        CoreEvent::AgentMessageDeliveryObserved { receipt }
            if receipt.resolved_round_id == Some(AgentRoundId::new("round-1"))
    ));
    assert!(wakes.recv_timeout(Duration::from_millis(50)).is_err());

    drop(engine);
    let restarted = test_engine_with_data_dir(data_dir);
    let round = restarted
        .get_agent_round(&AgentRoundId::new("round-1"))
        .unwrap()
        .unwrap();
    assert_eq!(round.status, AgentRoundStatus::Replied);
    assert_eq!(round.reply_message_id.as_deref(), Some("reply-message-1"));
}

#[test]
fn system_operator_round_resolves_without_fake_sender_session() {
    let data_dir = unique_data_dir("operator-round-reply");
    let engine = test_engine_with_data_dir(data_dir.clone());
    let recipient = engine
        .create_session(session_config(
            "recipient-session",
            "recipient-agent",
            "recipient-profile",
            SessionKind::Full,
        ))
        .unwrap();

    let started = engine
        .begin_agent_round(AgentRoundCommand {
            caller: AgentCoordinationCaller::System {
                sender_agent_id: AgentId::new("rusty-crew-debug-operator"),
            },
            round_id: AgentRoundId::new("operator-round-1"),
            idempotency_key: "operator-round-key-1".into(),
            message_id: "operator-round-message-1".into(),
            to_agent_id: recipient.agent_id.clone(),
            body: "report back".into(),
            correlation_id: "operator-correlation-1".into(),
            created_at: "2026-06-19T00:00:00Z".into(),
            expires_at: "2026-06-19T00:05:00Z".into(),
        })
        .unwrap();
    assert_eq!(started.round.sender_session_id, None);
    assert_eq!(started.round.status, AgentRoundStatus::Pending);

    let reply = engine
        .deliver_agent_message(AgentMessageCommand {
            caller: AgentCoordinationCaller::DirectBrain {
                session_id: recipient.session_id,
                wake_id: "recipient-wake".into(),
                tool_call_id: "reply-call".into(),
            },
            delivery_id: AgentMessageDeliveryId::new("operator-reply-delivery"),
            idempotency_key: "operator-reply-key".into(),
            message_id: "operator-reply-message".into(),
            to_agent_id: AgentId::new("rusty-crew-debug-operator"),
            body: "report complete".into(),
            collaboration_mode: None,
            correlation_id: Some("operator-correlation-1".into()),
            require_wake: true,
            created_at: "2026-06-19T00:00:01Z".into(),
            expires_at: "2026-06-19T00:05:00Z".into(),
        })
        .unwrap();
    assert_eq!(
        reply.resolved_round_id,
        Some(AgentRoundId::new("operator-round-1"))
    );
    assert!(reply.activation.is_none());

    drop(engine);
    let restarted = test_engine_with_data_dir(data_dir.clone());
    let round = restarted
        .get_agent_round(&AgentRoundId::new("operator-round-1"))
        .unwrap()
        .unwrap();
    assert_eq!(round.status, AgentRoundStatus::Replied);
    assert_eq!(round.sender_session_id, None);
    drop(restarted);
    let _ = std::fs::remove_dir_all(data_dir);
}

#[test]
fn active_external_recipient_queues_without_brain_wake() {
    let engine = test_engine();
    let sender = engine
        .create_session(session_config(
            "sender-session",
            "sender-agent",
            "sender-profile",
            SessionKind::Full,
        ))
        .unwrap();
    let codex = engine
        .create_session(session_config(
            "codex-session",
            "codex-agent",
            "codex-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine.register_external_runtime(&runtime(), None).unwrap();
    engine.bind_external_agent(&binding(), None).unwrap();
    engine
        .activate_agent_execution(activation("codex-agent", "already-active"))
        .unwrap();
    let (_subscription_id, wakes) = engine
        .subscribe_events(EventSubscription {
            event_kinds: vec![CoreEventKind::BrainWakeRequested],
            session_id: Some(codex.session_id.clone()),
            agent_id: None,
            adapter_id: None,
        })
        .unwrap();

    let command = AgentMessageCommand {
        caller: AgentCoordinationCaller::DirectBrain {
            session_id: sender.session_id,
            wake_id: "sender-wake".into(),
            tool_call_id: "send-call".into(),
        },
        delivery_id: AgentMessageDeliveryId::new("delivery-busy"),
        idempotency_key: "delivery-busy-key".into(),
        message_id: "message-busy".into(),
        to_agent_id: codex.agent_id.clone(),
        body: "queue for later".into(),
        collaboration_mode: None,
        correlation_id: None,
        require_wake: true,
        created_at: "2026-06-19T00:00:00Z".into(),
        expires_at: "2026-06-19T00:05:00Z".into(),
    };
    let receipt = engine.deliver_agent_message(command.clone()).unwrap();
    assert!(matches!(
        receipt.activation,
        Some(AgentActivation::QueuedForNextTurn { .. })
    ));
    assert!(wakes.recv_timeout(Duration::from_millis(50)).is_err());
    let queued = CoordinationStore::open(engine.config.engine_data_dir.clone())
        .unwrap()
        .load_queued_messages(&QueuedMessageFilter {
            state: Some(QueuedMessageState::Pending),
            owner_session_id: Some(codex.session_id.clone()),
            owner_agent_id: None,
            limit: None,
        })
        .unwrap();
    assert_eq!(queued.len(), 1);
    assert!(queued[0].message.body.contains("message_id: message-busy"));
    assert!(queued[0].message.body.ends_with("queue for later"));
    assert_eq!(
        engine.deliver_agent_message(command.clone()).unwrap(),
        receipt
    );
    let queued_after_replay = CoordinationStore::open(engine.config.engine_data_dir.clone())
        .unwrap()
        .load_queued_messages(&QueuedMessageFilter {
            state: Some(QueuedMessageState::Pending),
            owner_session_id: Some(codex.session_id.clone()),
            owner_agent_id: None,
            limit: None,
        })
        .unwrap();
    assert_eq!(queued_after_replay.len(), 1);

    let mut plan_command = command.clone();
    plan_command.delivery_id = AgentMessageDeliveryId::new("delivery-busy-plan");
    plan_command.idempotency_key = "delivery-busy-plan-key".into();
    plan_command.message_id = "message-busy-plan".into();
    plan_command.collaboration_mode = Some(ExternalCollaborationMode::Plan);
    let rejected = engine.deliver_agent_message(plan_command).unwrap();
    assert_eq!(rejected.status, AgentMessageDeliveryStatus::Rejected);
    assert_eq!(
        rejected.reason_code.as_deref(),
        Some("external_collaboration_mode_turn_already_active")
    );

    let active_request = ExternalTurnRequestId::new("already-active");
    engine
        .transition_external_turn(
            &active_request,
            ExternalTurnPhase::Starting,
            None,
            None,
            "2026-06-19T00:00:01Z".into(),
        )
        .unwrap();
    engine
        .transition_external_turn(
            &active_request,
            ExternalTurnPhase::Active,
            Some("native-active".into()),
            None,
            "2026-06-19T00:00:02Z".into(),
        )
        .unwrap();
    engine
        .transition_external_turn(
            &active_request,
            ExternalTurnPhase::Completed,
            None,
            None,
            "2026-06-19T00:00:03Z".into(),
        )
        .unwrap();
    let promoted = CoordinationStore::open(engine.config.engine_data_dir.clone())
        .unwrap()
        .get_external_turn(&ExternalTurnRequestId::new(format!(
            "external-follow-up:{}",
            queued[0].message_id
        )))
        .unwrap()
        .unwrap();
    assert_eq!(promoted.phase, ExternalTurnPhase::Accepted);
    assert_eq!(
        promoted.request.input,
        vec![ExternalTurnInputPart::Text {
            text: queued[0].message.body.clone()
        }]
    );
    let delivered = CoordinationStore::open(engine.config.engine_data_dir.clone())
        .unwrap()
        .load_queued_messages(&QueuedMessageFilter {
            state: Some(QueuedMessageState::Delivered),
            owner_session_id: Some(codex.session_id),
            owner_agent_id: None,
            limit: None,
        })
        .unwrap();
    assert_eq!(delivered.len(), 1);
    assert_eq!(delivered[0].delivery_attempts, 1);
}

#[test]
fn external_collaboration_mode_is_persisted_on_the_requested_turn() {
    let engine = test_engine();
    let sender = engine
        .create_session(session_config(
            "sender-session",
            "sender-agent",
            "sender-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine
        .create_session(session_config(
            "codex-session",
            "codex-agent",
            "codex-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine.register_external_runtime(&runtime(), None).unwrap();
    let mut serial_binding = binding();
    serial_binding.message_delivery_policy = ExternalMessageDeliveryPolicy::SerialNextTurn;
    engine.bind_external_agent(&serial_binding, None).unwrap();

    let receipt = engine
        .deliver_agent_message(AgentMessageCommand {
            caller: AgentCoordinationCaller::DirectBrain {
                session_id: sender.session_id,
                wake_id: "sender-wake".into(),
                tool_call_id: "send-call".into(),
            },
            delivery_id: AgentMessageDeliveryId::new("delivery-plan"),
            idempotency_key: "delivery-plan-key".into(),
            message_id: "message-plan".into(),
            to_agent_id: AgentId::new("codex-agent"),
            body: "ask an operator question".into(),
            collaboration_mode: Some(ExternalCollaborationMode::Plan),
            correlation_id: None,
            require_wake: true,
            created_at: "2026-06-19T00:00:00Z".into(),
            expires_at: "2026-06-19T00:05:00Z".into(),
        })
        .unwrap();
    assert_eq!(receipt.status, AgentMessageDeliveryStatus::Accepted);
    let turn = engine
        .get_external_turn(&ExternalTurnRequestId::new("agent-message:message-plan"))
        .unwrap()
        .unwrap();
    assert_eq!(
        turn.request.collaboration_mode,
        Some(ExternalCollaborationMode::Plan)
    );
}

#[test]
fn expired_accepted_external_turn_is_terminalized_without_capping_active_work() {
    let engine = test_engine();
    engine
        .create_session(session_config(
            "codex-session",
            "codex-agent",
            "codex-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine.register_external_runtime(&runtime(), None).unwrap();
    engine.bind_external_agent(&binding(), None).unwrap();

    let expired_request = ExternalTurnRequestId::new("expired-before-dispatch");
    engine
        .activate_agent_execution(AgentActivationRequest {
            expires_at: Some("2026-06-19T00:00:05Z".into()),
            ..activation("codex-agent", &expired_request.0)
        })
        .unwrap();
    let expired = engine
        .expire_external_turn_dispatches(&"2026-06-19T00:00:06Z".into())
        .unwrap();
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].request.request_id, expired_request);
    assert_eq!(expired[0].phase, ExternalTurnPhase::Failed);
    assert_eq!(
        expired[0].terminal_reason_code.as_deref(),
        Some("external_turn_dispatch_expired")
    );

    let active_request = ExternalTurnRequestId::new("active-past-input-ttl");
    engine
        .activate_agent_execution(AgentActivationRequest {
            expires_at: Some("2026-06-19T00:00:07Z".into()),
            ..activation("codex-agent", &active_request.0)
        })
        .unwrap();
    engine
        .transition_external_turn(
            &active_request,
            ExternalTurnPhase::Starting,
            None,
            None,
            "2026-06-19T00:00:06Z".into(),
        )
        .unwrap();
    engine
        .transition_external_turn(
            &active_request,
            ExternalTurnPhase::Active,
            Some("native-active".into()),
            None,
            "2026-06-19T00:00:06Z".into(),
        )
        .unwrap();
    assert!(engine
        .expire_external_turn_dispatches(&"2026-06-19T00:00:08Z".into())
        .unwrap()
        .is_empty());
    assert_eq!(
        engine
            .get_external_turn(&active_request)
            .unwrap()
            .unwrap()
            .phase,
        ExternalTurnPhase::Active
    );
}

#[test]
fn expired_external_follow_up_is_not_promoted_after_terminal_turn() {
    let engine = test_engine();
    let codex = engine
        .create_session(session_config(
            "codex-session",
            "codex-agent",
            "codex-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine.register_external_runtime(&runtime(), None).unwrap();
    engine.bind_external_agent(&binding(), None).unwrap();
    let active_request = ExternalTurnRequestId::new("active-before-expiry");
    engine
        .activate_agent_execution(activation("codex-agent", &active_request.0))
        .unwrap();
    let receipt = engine
        .deliver_agent_message(AgentMessageCommand {
            caller: AgentCoordinationCaller::System {
                sender_agent_id: AgentId::new("operator"),
            },
            delivery_id: AgentMessageDeliveryId::new("delivery-expiring"),
            idempotency_key: "delivery-expiring".into(),
            message_id: "message-expiring".into(),
            to_agent_id: codex.agent_id.clone(),
            body: "do not resurrect".into(),
            collaboration_mode: None,
            correlation_id: None,
            require_wake: true,
            created_at: "2026-06-19T00:00:00Z".into(),
            expires_at: "2026-06-19T00:05:00Z".into(),
        })
        .unwrap();
    assert!(matches!(
        receipt.activation,
        Some(AgentActivation::QueuedForNextTurn { .. })
    ));
    engine
        .transition_external_turn(
            &active_request,
            ExternalTurnPhase::Starting,
            None,
            None,
            "2026-06-19T00:00:01Z".into(),
        )
        .unwrap();
    engine
        .transition_external_turn(
            &active_request,
            ExternalTurnPhase::Active,
            Some("native-active".into()),
            None,
            "2026-06-19T00:00:02Z".into(),
        )
        .unwrap();
    engine
        .transition_external_turn(
            &active_request,
            ExternalTurnPhase::Completed,
            None,
            None,
            "2026-06-20T00:00:00Z".into(),
        )
        .unwrap();

    let store = CoordinationStore::open(engine.config.engine_data_dir.clone()).unwrap();
    let expired = store
        .load_queued_messages(&QueuedMessageFilter {
            state: Some(QueuedMessageState::Expired),
            owner_session_id: Some(codex.session_id),
            owner_agent_id: None,
            limit: None,
        })
        .unwrap();
    assert_eq!(expired.len(), 1);
    assert!(expired[0]
        .message
        .body
        .contains("message_id: message-expiring"));
    assert!(expired[0].message.body.ends_with("do not resurrect"));
    assert!(store
        .get_external_turn(&ExternalTurnRequestId::new(format!(
            "external-follow-up:{}",
            expired[0].message_id
        )))
        .unwrap()
        .is_none());
}

#[test]
fn codex_caller_requires_current_controller_and_active_native_turn() {
    let engine = test_engine();
    let direct = engine
        .create_session(session_config(
            "direct-session",
            "direct-agent",
            "direct-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine
        .create_session(session_config(
            "codex-session",
            "codex-agent",
            "codex-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine.register_external_runtime(&runtime(), None).unwrap();
    engine.bind_external_agent(&binding(), None).unwrap();
    let lease = engine
        .acquire_external_runtime_controller(
            &ExternalControllerLease {
                runtime_id: ExternalRuntimeId::new("codex-local"),
                holder_instance_id: "controller-a".into(),
                generation: 0,
                acquired_at: "2026-06-19T00:00:00Z".into(),
                renewed_at: "2026-06-19T00:00:00Z".into(),
                expires_at: "2026-06-19T00:10:00Z".into(),
                revision: 0,
            },
            &"2026-06-19T00:00:00Z".into(),
        )
        .unwrap();
    engine
        .activate_agent_execution(activation("codex-agent", "codex-active"))
        .unwrap();
    engine
        .transition_external_turn(
            &ExternalTurnRequestId::new("codex-active"),
            ExternalTurnPhase::Starting,
            None,
            None,
            "2026-06-19T00:00:01Z".into(),
        )
        .unwrap();
    engine
        .transition_external_turn(
            &ExternalTurnRequestId::new("codex-active"),
            ExternalTurnPhase::Active,
            Some("native-turn-7".into()),
            None,
            "2026-06-19T00:00:02Z".into(),
        )
        .unwrap();

    let command = AgentMessageCommand {
        caller: AgentCoordinationCaller::ExternalAgent {
            runtime_id: ExternalRuntimeId::new("codex-local"),
            binding_id: ExternalBindingId::new("codex-binding"),
            controller_instance_id: "controller-a".into(),
            controller_generation: lease.generation,
            native_thread_id: "native-thread-7".into(),
            native_turn_id: "native-turn-7".into(),
            native_request_id: "tool-call-1".into(),
        },
        delivery_id: AgentMessageDeliveryId::new("codex-delivery"),
        idempotency_key: "codex-delivery-key".into(),
        message_id: "codex-message".into(),
        to_agent_id: direct.agent_id,
        body: "message from Codex".into(),
        collaboration_mode: None,
        correlation_id: None,
        require_wake: true,
        created_at: "2026-06-19T00:00:03Z".into(),
        expires_at: "2026-06-19T00:05:00Z".into(),
    };
    assert!(matches!(
        engine
            .deliver_agent_message(command.clone())
            .unwrap()
            .activation,
        Some(AgentActivation::DirectBrainWakeRequested { .. })
    ));

    let mut stale = command;
    stale.delivery_id = AgentMessageDeliveryId::new("codex-delivery-stale");
    stale.idempotency_key = "codex-delivery-stale-key".into();
    stale.message_id = "codex-message-stale".into();
    let AgentCoordinationCaller::ExternalAgent {
        controller_generation,
        ..
    } = &mut stale.caller
    else {
        unreachable!()
    };
    *controller_generation += 1;
    assert_eq!(
        engine.deliver_agent_message(stale).unwrap_err().kind,
        CoreErrorKind::ActionRejected
    );
}

#[test]
fn external_activation_is_runtime_neutral_and_rehydrates_exact_turn() {
    let data_dir = unique_data_dir("external-runtime-activation");
    let engine = test_engine_with_data_dir(data_dir.clone());
    engine
        .create_session(session_config(
            "direct-session",
            "direct-agent",
            "direct-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine
        .create_session(session_config(
            "codex-session",
            "codex-agent",
            "codex-profile",
            SessionKind::Full,
        ))
        .unwrap();

    assert!(matches!(
        engine.activate_agent_execution(activation("direct-agent", "direct-request")),
        Ok(AgentActivation::DirectBrainWakeRequested { .. })
    ));

    engine.register_external_runtime(&runtime(), None).unwrap();
    engine.bind_external_agent(&binding(), None).unwrap();
    let activation_result = engine
        .activate_agent_execution(activation("codex-agent", "codex-request"))
        .unwrap();
    assert!(matches!(
        activation_result,
        AgentActivation::ExternalTurnRequested { .. }
    ));
    assert!(matches!(
        engine
            .activate_agent_execution(activation("codex-agent", "queued-request"))
            .unwrap(),
        AgentActivation::QueuedForNextTurn { .. }
    ));

    let starting = engine
        .transition_external_turn(
            &ExternalTurnRequestId::new("codex-request"),
            ExternalTurnPhase::Starting,
            None,
            None,
            "2026-06-19T00:00:01Z".into(),
        )
        .unwrap();
    let active = engine
        .transition_external_turn(
            &starting.request.request_id,
            ExternalTurnPhase::Active,
            Some("native-turn-7".into()),
            None,
            "2026-06-19T00:00:02Z".into(),
        )
        .unwrap();
    assert!(active.capacity_lease_id.is_some());
    assert!(matches!(
        engine
            .activate_agent_execution(activation("codex-agent", "steer-request"))
            .unwrap(),
        AgentActivation::ExternalTurnSteerRequested {
            native_turn_id,
            ..
        } if native_turn_id == "native-turn-7"
    ));
    let mut serial_binding = engine
        .get_external_binding(&ExternalBindingId::new("codex-binding"))
        .unwrap()
        .unwrap();
    let serial_revision = serial_binding.revision;
    serial_binding.message_delivery_policy = ExternalMessageDeliveryPolicy::SerialNextTurn;
    serial_binding.updated_at = "2026-06-19T00:00:02.500Z".into();
    engine
        .bind_external_agent(&serial_binding, Some(serial_revision))
        .unwrap();
    assert!(matches!(
        engine
            .activate_agent_execution(activation("codex-agent", "serial-request"))
            .unwrap(),
        AgentActivation::QueuedForNextTurn { .. }
    ));
    let completed = engine
        .transition_external_turn(
            &active.request.request_id,
            ExternalTurnPhase::Completed,
            None,
            None,
            "2026-06-19T00:00:03Z".into(),
        )
        .unwrap();
    assert!(completed.capacity_lease_id.is_none());
    drop(engine);

    let restarted = test_engine_with_data_dir(data_dir.clone());
    let report = restarted
        .hydrate_external_runtime_lifecycle(&"2026-06-19T00:01:00Z".into())
        .unwrap();
    assert_eq!(report.runtime_count, 1);
    assert_eq!(report.binding_count, 1);
    assert!(report.driver_reconciliation_request_ids.is_empty());
    assert!(report.terminalized_request_ids.is_empty());
    let store = CoordinationStore::open(data_dir).unwrap();
    let hydrated = store
        .get_external_turn(&ExternalTurnRequestId::new("codex-request"))
        .unwrap()
        .unwrap();
    assert_eq!(hydrated.native_thread_id, "native-thread-7");
    assert_eq!(hydrated.native_turn_id.as_deref(), Some("native-turn-7"));
    assert_eq!(hydrated.phase, ExternalTurnPhase::Completed);
}

#[test]
fn restart_reconciliation_terminalizes_archived_session_without_replay() {
    let data_dir = unique_data_dir("external-runtime-reconcile");
    let engine = test_engine_with_data_dir(data_dir.clone());
    engine
        .create_session(session_config(
            "codex-session",
            "codex-agent",
            "codex-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine.register_external_runtime(&runtime(), None).unwrap();
    engine.bind_external_agent(&binding(), None).unwrap();
    engine
        .activate_agent_execution(activation("codex-agent", "codex-request"))
        .unwrap();
    engine
        .archive_session(&SessionId::new("codex-session"))
        .unwrap();
    drop(engine);

    let restarted = test_engine_with_data_dir(data_dir.clone());
    let report = restarted
        .hydrate_external_runtime_lifecycle(&"2026-06-19T00:01:00Z".into())
        .unwrap();
    assert_eq!(
        report.terminalized_request_ids,
        vec![ExternalTurnRequestId::new("codex-request")]
    );
    let store = CoordinationStore::open(data_dir).unwrap();
    let turn = store
        .get_external_turn(&ExternalTurnRequestId::new("codex-request"))
        .unwrap()
        .unwrap();
    assert_eq!(turn.phase, ExternalTurnPhase::Interrupted);
    assert_eq!(
        turn.terminal_reason_code.as_deref(),
        Some("external_session_unavailable")
    );
    assert!(turn.capacity_lease_id.is_none());
}

pub(super) fn runtime() -> ExternalRuntimeRegistration {
    ExternalRuntimeRegistration {
        runtime_id: ExternalRuntimeId::new("codex-local"),
        kind: ExternalRuntimeKind::CodexAppServer,
        endpoint: ExternalEndpoint {
            transport: ExternalEndpointTransport::UnixWebSocket,
            address: "/run/user/1001/codex.sock".into(),
        },
        process_ownership: ExternalProcessOwnership::Attached,
        codex_home_ref: Some("/home/agent/.codex".into()),
        observed_cli_version: Some("0.144.1".into()),
        consumed_contract_revision: Some("contract-v1".into()),
        compatibility_state: ExternalRuntimeCompatibilityState::CompatibleUncertified,
        last_compatibility_probe: Some(probe_report(
            ExternalRuntimeCompatibilityProbeOutcome::Passed,
            None,
        )),
        desired_state: ExternalRuntimeDesiredState::Enabled,
        observed_state: ExternalRuntimeObservedState::Ready,
        observed_reason_code: None,
        revision: 0,
        created_at: "2026-06-19T00:00:00Z".into(),
        updated_at: "2026-06-19T00:00:00Z".into(),
    }
}

pub(super) fn probe_report(
    outcome: ExternalRuntimeCompatibilityProbeOutcome,
    reason_code: Option<&str>,
) -> ExternalRuntimeCompatibilityProbeReport {
    ExternalRuntimeCompatibilityProbeReport {
        suite_revision: "codex-required-capabilities-v1".into(),
        outcome,
        steps: vec![ExternalRuntimeCompatibilityProbeStep {
            step_id: "model_list".into(),
            status: if reason_code.is_some() {
                ExternalRuntimeCompatibilityProbeStepStatus::Failed
            } else {
                ExternalRuntimeCompatibilityProbeStepStatus::Passed
            },
            duration_ms: 1,
            reason_code: reason_code.map(str::to_owned),
            detail: None,
        }],
        completed_at: "2026-06-19T00:00:01Z".into(),
    }
}

pub(super) fn binding() -> ExternalAgentBinding {
    ExternalAgentBinding {
        binding_id: ExternalBindingId::new("codex-binding"),
        runtime_id: ExternalRuntimeId::new("codex-local"),
        session_id: Some(SessionId::new("codex-session")),
        agent_id: Some(AgentId::new("codex-agent")),
        profile_id: Some(ProfileId::new("codex-profile")),
        profile_revision: Some(1),
        profile_prompt_hash: Some("profile-prompt-hash".into()),
        message_delivery_policy: ExternalMessageDeliveryPolicy::ImmediateSteer,
        purpose: ExternalBindingPurpose::CrewAgent,
        native_thread_id: Some("native-thread-7".into()),
        cwd: Some("/home/dev/rusty-crew".into()),
        label: None,
        task_ref: None,
        effective_config_fingerprint: "config-fingerprint".into(),
        status: ExternalBindingStatus::Active,
        revision: 0,
        created_at: "2026-06-19T00:00:00Z".into(),
        updated_at: "2026-06-19T00:00:00Z".into(),
    }
}

pub(super) fn external_controller_lease() -> ExternalControllerLease {
    ExternalControllerLease {
        runtime_id: ExternalRuntimeId::new("codex-local"),
        holder_instance_id: "controller-a".into(),
        generation: 0,
        acquired_at: "2026-06-19T00:00:00Z".into(),
        renewed_at: "2026-06-19T00:00:00Z".into(),
        expires_at: "2026-06-19T00:10:00Z".into(),
        revision: 0,
    }
}

fn ready_external_creation_dependencies(engine: &CoreEngine) -> ExternalControllerContext {
    engine
        .create_profile_registry_record(&profile_registry_write(
            "codex-profile",
            "gpt",
            "configured-codex-session",
        ))
        .unwrap();
    engine.register_external_runtime(&runtime(), None).unwrap();
    let lease = engine
        .acquire_external_runtime_controller(
            &external_controller_lease(),
            &"2026-06-19T00:00:00Z".into(),
        )
        .unwrap();
    ExternalControllerContext {
        holder_instance_id: lease.holder_instance_id,
        generation: lease.generation,
    }
}

fn external_creation_request(idempotency_key: &str) -> ExternalAgentSessionCreationRequest {
    ExternalAgentSessionCreationRequest {
        idempotency_key: idempotency_key.into(),
        runtime_id: ExternalRuntimeId::new("codex-local"),
        profile_id: ProfileId::new("codex-profile"),
        cwd: "/home/dev/rusty-crew".into(),
        task_ref: None,
        label: Some("Codex implementation agent".into()),
        requested_at: "2026-06-19T00:00:01Z".into(),
    }
}

fn activation(agent_id: &str, request_id: &str) -> AgentActivationRequest {
    AgentActivationRequest {
        agent_id: AgentId::new(agent_id),
        request_id: ExternalTurnRequestId::new(request_id),
        idempotency_key: format!("{request_id}-key"),
        input: vec![ExternalTurnInputPart::Text {
            text: "inspect the project".into(),
        }],
        collaboration_mode: None,
        provenance: TurnInputProvenance {
            kind: TurnInputProvenanceKind::Operator,
            source_id: None,
            correlation_id: None,
        },
        run_id: None,
        capacity_lease_id: format!("{request_id}-capacity"),
        direct_wake_id: format!("{request_id}-wake"),
        queued_message_id: format!("{request_id}-queue"),
        created_at: "2026-06-19T00:00:00Z".into(),
        expires_at: None,
    }
}
