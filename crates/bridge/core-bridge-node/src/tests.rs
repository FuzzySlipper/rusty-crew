use super::*;
use rusty_crew_core_bridge_api::{
    AgentId, BrainAction, BrainActionBatch, BrainImplementationHandle, BrainImplementationId,
    BrainModelConfig, BrainProviderStateScope, BrainProviderStateStrategyMetadata,
    BrainStrategyMetadata, BrainWakeProviderStateOutput, BrainWakeProviderStateUpdate,
    CoreEventKind, EventSubscription, ProfileId, ProviderStateAbsenceReason,
    ProviderStateCompatibilityAction, ProviderStateCompatibilityClass,
    ProviderStateCompatibilityFacts, ProviderStateCompatibilityPlan, ProviderStateMode,
    ResourceLimits, SessionConfig, SessionId, SessionKind, SessionWorkspace,
    SessionWorkspaceUpdate, ShutdownRequest, ToolDescriptor, ToolProfile,
};
use rusty_crew_core_protocol::{
    BrainEvent, ModelProviderSecretEnvelope, MODEL_PROVIDER_SECRET_ENVELOPE_VERSION,
};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;

#[test]
fn native_bridge_exposes_the_current_manifest_surface() {
    let bridge = NativeBridge::new();

    assert_eq!(bridge.manifest_version(), MANIFEST_VERSION);
    assert_eq!(bridge.operation_names(), OPERATION_NAMES);
    assert_eq!(bridge.wire_shape_fingerprint(), wire_shape_fingerprint());
    assert!(bridge.operation_names().contains(&"get_buffer"));
    assert!(bridge.operation_names().contains(&"release_buffer"));
    assert_eq!(
        bridge.manifest_summary().native_package,
        "@rusty-crew/native-bridge"
    );
}

#[test]
fn openai_responses_bridge_uses_oauth_bearer_and_headers_without_secret_update() {
    let server = FakeResponsesServer::new();
    let mut bridge = NativeBridge::new();
    bridge
        .initialize_engine(EngineConfig {
            engine_data_dir: std::env::temp_dir()
                .join(format!(
                    "rusty-crew-native-openai-oauth-{}",
                    std::process::id()
                ))
                .to_string_lossy()
                .to_string(),
            clock: rusty_crew_core_bridge_api::ClockConfig::Fixed {
                at: "2026-07-02T00:00:00Z".to_string(),
            },
            default_turn_budget: 3,
            default_idle_timeout_ms: 1000,
            storage: None,
        })
        .unwrap();
    bridge
        .create_session(SessionConfig {
            session_id: SessionId::new("responses-session"),
            agent_id: AgentId::new("responses-agent"),
            profile_id: ProfileId::new("responses-profile"),
            kind: SessionKind::Full,
            delegation: None,
            workspace: None,
            resource_limits: ResourceLimits {
                max_duration_ms: None,
                max_delegation_depth: None,
            },
            tool_profile: ToolProfile { tools: Vec::new() },
            history_window: None,
        })
        .unwrap();
    let body_state: serde_json::Value = serde_json::from_slice(
        &bridge
            .project_body_state_json(SessionId::new("responses-session"))
            .unwrap(),
    )
    .unwrap();
    let secret = ModelProviderSecretEnvelope::OpenAiOauth {
        version: MODEL_PROVIDER_SECRET_ENVELOPE_VERSION,
        issuer: "http://127.0.0.1:9".to_string(),
        client_id: "client".to_string(),
        id_token: test_jwt(4_102_444_800, serde_json::json!({})),
        access_token: test_jwt(4_102_444_800, serde_json::json!({})),
        refresh_token: "refresh-secret".to_string(),
        exchanged_api_token: None,
        last_refresh_at: Some(
            time::OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap(),
        ),
        account_id: Some("account-1".to_string()),
        email: None,
        plan_type: None,
        is_fedramp_account: true,
        access_token_expires_at: None,
    };
    let input = json!({
        "wakeId": "wake-oauth",
        "sessionId": "responses-session",
        "bodyState": body_state,
        "config": {"model": "gpt-5", "responsesDialect": "openai_stateless", "instructions": "say ok"},
        "client": {
            "mode": "live",
            "base_url": server.base_url(),
            "auth_kind": "openai_oauth",
            "provider_alias": "gpt",
            "oauth_credential_secret": secret.to_storage_text().unwrap()
        }
    });

    let output: serde_json::Value =
        serde_json::from_str(&run_openai_responses_brain_json_blocking(input.to_string()).unwrap())
            .unwrap();

    assert!(output.get("credential_secret_update").unwrap().is_null());
    let captured = server.captured();
    assert!(captured.contains("post /responses http/1.1"));
    assert!(captured.contains("authorization: bearer "));
    assert!(captured.contains("chatgpt-account-id: account-1"));
    assert!(captured.contains("x-openai-fedramp: true"));
    assert!(captured.contains("\"tools\":[]"));
    assert!(!captured.contains("\"tool_choice\""));
    assert!(!captured.contains("refresh-secret"));
}

#[test]
fn native_bridge_releases_buffer_handles_once() {
    let bridge = NativeBridge::new();
    let buffered = bridge
        .build_brain_wake_request(BrainWakeBufferInput {
            brain: BrainImplementationHandle::new(1),
            session_id: SessionId::new("session"),
            body_state_json: vec![b'{', b'}'],
            system_prompt: "system".to_string(),
            role_assembly_json: vec![b'{', b'}'],
            wake_id: "wake".to_string(),
            compaction_intent: None,
        })
        .unwrap();
    let body_handle = buffered.request.body_state;

    assert_eq!(bridge.get_buffer(body_handle).unwrap().bytes, b"{}");
    bridge.release_buffer(body_handle).unwrap();
    let error = bridge
        .release_buffer(body_handle)
        .expect_err("double release must fail loudly");

    assert_eq!(error.kind, CoreErrorKind::NotFound);
}

#[test]
fn openai_responses_tool_schema_normalization_preserves_required_fields() {
    let schema = serde_json::json!({
        "properties": {
            "project_id": { "type": "string" },
            "status": { "type": "string" }
        },
        "required": ["project_id"]
    });
    assert_eq!(
        normalize_responses_tool_schema(&schema),
        serde_json::json!({
            "type": "object",
            "properties": {
                "project_id": { "type": "string" },
                "status": { "type": "string" }
            },
            "required": ["project_id"]
        })
    );
    assert_eq!(
        normalize_responses_tool_schema(&serde_json::json!("not-a-schema")),
        serde_json::json!({"type": "object", "properties": {}})
    );
}

#[test]
fn native_bridge_reports_leaked_runtime_buffers() {
    let bridge = NativeBridge::new();
    let _buffered = bridge
        .build_brain_wake_request(BrainWakeBufferInput {
            brain: BrainImplementationHandle::new(1),
            session_id: SessionId::new("session"),
            body_state_json: vec![b'{', b'}'],
            system_prompt: "system".to_string(),
            role_assembly_json: vec![b'{', b'}'],
            wake_id: "wake".to_string(),
            compaction_intent: None,
        })
        .unwrap();

    let error = bridge
        .assert_no_buffer_leaks()
        .expect_err("unreleased wake buffers should be visible");

    assert_eq!(error.kind, CoreErrorKind::InternalError);
}

#[test]
fn native_bridge_registers_brain_implementations_with_stable_handles() {
    let mut bridge = NativeBridge::new();
    let first = bridge
        .register_brain_implementation(brain_registration("planner", "planner-profile"))
        .unwrap();
    let second = bridge
        .register_brain_implementation(brain_registration("coder", "coder-profile"))
        .unwrap();

    assert_eq!(first, BrainImplementationHandle::new(1));
    assert_eq!(second, BrainImplementationHandle::new(2));
}

#[test]
fn native_bridge_rejects_duplicate_brain_registration_ids() {
    let mut bridge = NativeBridge::new();
    bridge
        .register_brain_implementation(brain_registration("planner", "planner-profile"))
        .unwrap();
    let error = bridge
        .register_brain_implementation(brain_registration("planner", "other-profile"))
        .expect_err("duplicate implementation ids must fail");

    assert_eq!(error.kind, CoreErrorKind::AlreadyExists);
}

#[test]
fn native_bridge_rejects_duplicate_profile_brain_registrations() {
    let mut bridge = NativeBridge::new();
    bridge
        .register_brain_implementation(brain_registration("planner", "planner-profile"))
        .unwrap();
    let error = bridge
        .register_brain_implementation(brain_registration("other", "planner-profile"))
        .expect_err("duplicate profile bindings must fail");

    assert_eq!(error.kind, CoreErrorKind::AlreadyExists);
}

#[test]
fn native_bridge_replaces_profile_brain_registration_in_place() {
    let mut bridge = NativeBridge::new();
    let handle = bridge
        .register_brain_implementation(brain_registration("planner", "planner-profile"))
        .unwrap();
    let replaced = bridge
        .replace_brain_implementation(brain_registration_with_tools(
            "planner-rebuilt",
            "planner-profile",
            vec!["read_file", "patch"],
        ))
        .unwrap();

    assert_eq!(replaced, handle);
    let registration = bridge.brain_registrations.get(handle).unwrap();
    assert_eq!(
        registration.implementation_id.to_string(),
        "planner-rebuilt"
    );
    assert_eq!(registration.tool_profile.tools.len(), 2);
}

#[test]
fn native_bridge_replace_registers_missing_profile_brain() {
    let mut bridge = NativeBridge::new();
    let handle = bridge
        .replace_brain_implementation(brain_registration("planner", "planner-profile"))
        .unwrap();

    assert_eq!(handle, BrainImplementationHandle::new(1));
    let registration = bridge.brain_registrations.get(handle).unwrap();
    assert_eq!(registration.profile_id.to_string(), "planner-profile");
}

#[test]
fn native_bridge_unregisters_profile_brain_and_allows_reregister() {
    let mut bridge = NativeBridge::new();
    let handle = bridge
        .register_brain_implementation(brain_registration("planner", "planner-profile"))
        .unwrap();

    let removed = bridge
        .unregister_brain_implementation_for_profile(ProfileId::new("planner-profile"))
        .unwrap();
    assert_eq!(removed, handle);
    assert!(bridge.brain_registrations.get(handle).is_err());

    let next = bridge
        .register_brain_implementation(brain_registration("planner-next", "planner-profile"))
        .unwrap();
    assert_ne!(next, handle);
    let registration = bridge.brain_registrations.get(next).unwrap();
    assert_eq!(registration.profile_id.to_string(), "planner-profile");
}

#[test]
fn native_bridge_unregister_missing_profile_brain_fails_closed() {
    let mut bridge = NativeBridge::new();
    let error = bridge
        .unregister_brain_implementation_for_profile(ProfileId::new("missing-profile"))
        .expect_err("missing profile brain unregister must fail");

    assert_eq!(error.kind, CoreErrorKind::NotFound);
}

#[test]
fn native_bridge_rejects_replacement_using_another_profile_implementation_id() {
    let mut bridge = NativeBridge::new();
    bridge
        .register_brain_implementation(brain_registration("planner", "planner-profile"))
        .unwrap();
    bridge
        .register_brain_implementation(brain_registration("coder", "coder-profile"))
        .unwrap();

    let error = bridge
        .replace_brain_implementation(brain_registration("coder", "planner-profile"))
        .expect_err("replacement cannot steal another profile implementation id");

    assert_eq!(error.kind, CoreErrorKind::AlreadyExists);
}

#[test]
fn native_bridge_mirrors_registered_tool_profiles_into_delegated_sessions() {
    let mut bridge = NativeBridge::new();
    bridge
        .register_brain_implementation(brain_registration_with_tools(
            "coder",
            "coder-profile",
            vec!["read_file", "patch"],
        ))
        .unwrap();
    bridge
        .initialize_engine(EngineConfig {
            engine_data_dir: std::env::temp_dir()
                .join(format!(
                    "rusty-crew-native-tool-profile-{}",
                    std::process::id()
                ))
                .to_string_lossy()
                .to_string(),
            clock: rusty_crew_core_bridge_api::ClockConfig::Fixed {
                at: "2026-06-19T00:00:00Z".to_string(),
            },
            default_turn_budget: 3,
            default_idle_timeout_ms: 1000,
            storage: None,
        })
        .unwrap();
    let planner = bridge
        .create_session(SessionConfig {
            session_id: SessionId::new("planner-session"),
            agent_id: AgentId::new("planner"),
            profile_id: ProfileId::new("planner-profile"),
            kind: SessionKind::Full,
            delegation: None,
            workspace: None,
            resource_limits: ResourceLimits {
                max_duration_ms: None,
                max_delegation_depth: Some(1),
            },
            tool_profile: ToolProfile {
                tools: vec![ToolDescriptor {
                    name: "planner_only".to_string(),
                    description: "Only visible to the planner".to_string(),
                    input_schema: None,
                }],
            },
            history_window: None,
        })
        .unwrap();

    bridge
        .submit_brain_actions(BrainActionBatch {
            wake_id: "planner-wake".to_string(),
            session_id: planner.session_id.clone(),
            actions: vec![BrainAction::RequestDelegation {
                profile_id: ProfileId::new("coder-profile"),
                task_id: None,
                prompt: "use registered coder tools".to_string(),
                expected_output: None,
                workspace_constraint: None,
                resource_limits: None,
                timeout_ms: None,
                priority: None,
                fan_out_group_id: None,
                fan_out_max_concurrency: None,
                fan_out_failure_policy: None,
                correlation_id: None,
                parent_consumption: None,
                capacity_request: None,
            }],
        })
        .unwrap();

    let body_json = bridge
        .project_body_state_json(SessionId::new("planner-session:delegated:planner-wake:0"))
        .unwrap();
    let body: rusty_crew_core_bridge_api::BodyState =
        serde_json::from_slice(&body_json).expect("delegated body state should deserialize");

    assert_eq!(
        body.session
            .tool_profile
            .tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>(),
        vec!["read_file", "patch"]
    );
}

#[test]
fn native_bridge_hydrates_and_updates_provider_state_around_wakes() {
    let mut bridge = NativeBridge::new();
    bridge
        .initialize_engine(EngineConfig {
            engine_data_dir: std::env::temp_dir()
                .join(format!(
                    "rusty-crew-native-provider-state-{}",
                    std::process::id()
                ))
                .to_string_lossy()
                .to_string(),
            clock: rusty_crew_core_bridge_api::ClockConfig::Fixed {
                at: "2026-06-24T00:00:00Z".to_string(),
            },
            default_turn_budget: 3,
            default_idle_timeout_ms: 1000,
            storage: None,
        })
        .unwrap();
    let optional_handle = bridge
        .register_brain_implementation(provider_state_brain_registration(
            "optional-provider-brain",
            "optional-provider-profile",
            ProviderStateMode::Optional,
        ))
        .unwrap();
    let required_handle = bridge
        .register_brain_implementation(provider_state_brain_registration(
            "required-provider-brain",
            "required-provider-profile",
            ProviderStateMode::Required,
        ))
        .unwrap();
    bridge
        .create_session(provider_state_session_config(
            "optional-provider-session",
            "optional-provider-profile",
        ))
        .unwrap();
    bridge
        .create_session(provider_state_session_config(
            "required-provider-session",
            "required-provider-profile",
        ))
        .unwrap();

    let first_optional = bridge
        .build_brain_wake_request_for_session(
            optional_handle,
            SessionId::new("optional-provider-session"),
            "system".to_string(),
            b"{}".to_vec(),
            "wake-1".to_string(),
        )
        .unwrap();
    assert!(first_optional.request.provider_state.is_none());
    assert_eq!(
        first_optional.request.provider_state_absence,
        Some(rusty_crew_core_bridge_api::ProviderStateAbsenceReason::Missing)
    );

    bridge
        .apply_provider_state_output(
            optional_handle,
            &SessionId::new("optional-provider-session"),
            "wake-1",
            BrainWakeProviderStateOutput::Replace {
                state: BrainWakeProviderStateUpdate {
                    module_id: "openai-responses".to_string(),
                    strategy_id: "replay".to_string(),
                    profile_fingerprint: "profile-fingerprint".to_string(),
                    provider_fingerprint: "provider-fingerprint".to_string(),
                    payload_version: "provider-owned-v1".to_string(),
                    payload: serde_json::json!({"response_id": "resp-1"}),
                    ttl_ms: Some(60_000),
                },
            },
        )
        .unwrap();
    let hydrated = bridge
        .build_brain_wake_request_for_session(
            optional_handle,
            SessionId::new("optional-provider-session"),
            "system".to_string(),
            b"{}".to_vec(),
            "wake-2".to_string(),
        )
        .unwrap();
    let state = hydrated
        .request
        .provider_state
        .expect("provider state should hydrate after replace");
    assert_eq!(state.module_id, "openai-responses");
    assert_eq!(state.strategy_id, "replay");
    assert_eq!(state.payload_version, "provider-owned-v1");
    assert_eq!(state.payload, serde_json::json!({"response_id": "resp-1"}));
    assert!(hydrated.request.provider_state_absence.is_none());

    let changed_scope_handle = bridge
        .replace_brain_implementation(provider_state_brain_registration_with_scope(
            "optional-provider-brain-changed-scope",
            "optional-provider-profile",
            ProviderStateMode::Optional,
            "changed-profile-fingerprint",
            "provider-fingerprint",
        ))
        .unwrap();
    let invalidated = bridge
        .build_brain_wake_request_for_session(
            changed_scope_handle,
            SessionId::new("optional-provider-session"),
            "system".to_string(),
            b"{}".to_vec(),
            "wake-changed-scope".to_string(),
        )
        .unwrap();
    assert!(invalidated.request.provider_state.is_none());
    assert_eq!(
        invalidated.request.provider_state_absence,
        Some(rusty_crew_core_bridge_api::ProviderStateAbsenceReason::Invalidated)
    );
    let restored_scope_handle = bridge
        .replace_brain_implementation(provider_state_brain_registration(
            "optional-provider-brain-restored-scope",
            "optional-provider-profile",
            ProviderStateMode::Optional,
        ))
        .unwrap();

    bridge
        .apply_provider_state_output(
            restored_scope_handle,
            &SessionId::new("optional-provider-session"),
            "wake-2b",
            BrainWakeProviderStateOutput::Replace {
                state: BrainWakeProviderStateUpdate {
                    module_id: "openai-responses".to_string(),
                    strategy_id: "replay".to_string(),
                    profile_fingerprint: "profile-fingerprint".to_string(),
                    provider_fingerprint: "provider-fingerprint".to_string(),
                    payload_version: "provider-owned-v1".to_string(),
                    payload: serde_json::json!({"response_id": "resp-2"}),
                    ttl_ms: Some(60_000),
                },
            },
        )
        .unwrap();

    bridge
        .apply_provider_state_output(
            restored_scope_handle,
            &SessionId::new("optional-provider-session"),
            "wake-2",
            BrainWakeProviderStateOutput::Clear {
                reason: rusty_crew_core_bridge_api::ProviderStateClearReason::BrainRequestedClear,
            },
        )
        .unwrap();
    let after_clear = bridge
        .build_brain_wake_request_for_session(
            restored_scope_handle,
            SessionId::new("optional-provider-session"),
            "system".to_string(),
            b"{}".to_vec(),
            "wake-3".to_string(),
        )
        .unwrap();
    assert!(after_clear.request.provider_state.is_none());
    assert_eq!(
        after_clear.request.provider_state_absence,
        Some(rusty_crew_core_bridge_api::ProviderStateAbsenceReason::Missing)
    );

    let required_error = bridge
        .build_brain_wake_request_for_session(
            required_handle,
            SessionId::new("required-provider-session"),
            "system".to_string(),
            b"{}".to_vec(),
            "wake-required".to_string(),
        )
        .expect_err("required state should fail before provider invocation");
    assert_eq!(required_error.kind, CoreErrorKind::BrainUnavailable);
}

#[test]
fn provider_state_compatibility_preserves_benign_session_and_profile_refreshes() {
    let mut bridge = NativeBridge::new();
    bridge
        .initialize_engine(EngineConfig {
            engine_data_dir: std::env::temp_dir()
                .join(format!(
                    "rusty-crew-native-provider-compatibility-{}",
                    std::process::id()
                ))
                .to_string_lossy()
                .to_string(),
            clock: rusty_crew_core_bridge_api::ClockConfig::Fixed {
                at: "2026-08-08T12:00:00Z".to_string(),
            },
            default_turn_budget: 3,
            default_idle_timeout_ms: 1000,
            storage: None,
        })
        .unwrap();
    let mut baseline = provider_state_brain_registration(
        "compatibility-responses-v1",
        "compatibility-profile",
        ProviderStateMode::Optional,
    );
    baseline
        .provider_state_scope
        .as_mut()
        .unwrap()
        .compatibility = Some(provider_state_compatibility_facts());
    let handle = bridge.register_brain_implementation(baseline).unwrap();
    let mut session =
        provider_state_session_config("compatibility-session", "compatibility-profile");
    session.workspace = Some(SessionWorkspace {
        cwd: "/workspace/one".to_string(),
        revision: 1,
        updated_at: "2026-08-08T12:00:00Z".to_string(),
    });
    bridge.create_session(session.clone()).unwrap();
    bridge
        .apply_provider_state_output(
            handle,
            &session.session_id,
            "wake-1",
            BrainWakeProviderStateOutput::Replace {
                state: BrainWakeProviderStateUpdate {
                    module_id: "openai-responses".to_string(),
                    strategy_id: "replay".to_string(),
                    profile_fingerprint: "profile-fingerprint".to_string(),
                    provider_fingerprint: "provider-fingerprint".to_string(),
                    payload_version: "provider-owned-v1".to_string(),
                    payload: serde_json::json!({"response_id": "resp-lineage"}),
                    ttl_ms: Some(60_000),
                },
            },
        )
        .unwrap();

    bridge
        .update_session_workspace(SessionWorkspaceUpdate {
            session_id: session.session_id.clone(),
            cwd: "/workspace/two".to_string(),
            expected_revision: 1,
            requested_at: "2026-08-08T12:01:00Z".to_string(),
        })
        .unwrap();
    bridge
        .set_session_reasoning_effort(session.session_id.clone(), Some("high".to_string()))
        .unwrap();
    let mut refreshed = provider_state_brain_registration(
        "compatibility-responses-v2",
        "compatibility-profile",
        ProviderStateMode::Optional,
    );
    let mut refreshed_facts = provider_state_compatibility_facts();
    refreshed_facts.display_metadata = "display-v2".to_string();
    refreshed_facts.prompt = "prompt-v2".to_string();
    refreshed_facts.skills = "skills-v2".to_string();
    refreshed_facts.tool_catalog = "tools-v2".to_string();
    let refreshed_snapshot_facts = refreshed_facts.clone();
    refreshed
        .provider_state_scope
        .as_mut()
        .unwrap()
        .compatibility = Some(refreshed_facts);
    let refreshed_handle = bridge.replace_brain_implementation(refreshed).unwrap();

    let hydrated = bridge
        .build_brain_wake_request_for_session(
            refreshed_handle,
            session.session_id.clone(),
            "system".to_string(),
            b"{}".to_vec(),
            "wake-2".to_string(),
        )
        .unwrap();
    assert_eq!(
        hydrated.request.provider_state.unwrap().payload,
        serde_json::json!({"response_id": "resp-lineage"})
    );
    let diagnostic = bridge.provider_state_diagnostics(1).unwrap().remove(0);
    let plan: ProviderStateCompatibilityPlan =
        serde_json::from_str(diagnostic.compatibility_plan_json.as_deref().unwrap()).unwrap();
    assert_eq!(plan.class, ProviderStateCompatibilityClass::Compatible);
    assert_eq!(
        plan.action,
        ProviderStateCompatibilityAction::PreserveLineage
    );
    assert!(plan
        .changes
        .iter()
        .any(|change| change.dimension == "session_workspace"));
    assert!(plan
        .changes
        .iter()
        .any(|change| change.dimension == "session_effort"));

    let mut incompatible = provider_state_brain_registration_with_scope(
        "compatibility-responses-v3",
        "compatibility-profile",
        ProviderStateMode::Optional,
        "profile-fingerprint",
        "provider-fingerprint-v2",
    );
    let mut incompatible_facts = provider_state_compatibility_facts();
    incompatible_facts.model = "model-v2".to_string();
    incompatible
        .provider_state_scope
        .as_mut()
        .unwrap()
        .compatibility = Some(incompatible_facts);
    let incompatible_handle = bridge.replace_brain_implementation(incompatible).unwrap();
    let rebuilt = bridge
        .build_brain_wake_request_for_session(
            incompatible_handle,
            session.session_id.clone(),
            "system".to_string(),
            b"{}".to_vec(),
            "wake-3".to_string(),
        )
        .unwrap();
    assert!(rebuilt.request.provider_state.is_none());
    assert_eq!(
        rebuilt.request.provider_state_absence,
        Some(ProviderStateAbsenceReason::Invalidated)
    );
    let diagnostic = bridge.provider_state_diagnostics(1).unwrap().remove(0);
    let plan: ProviderStateCompatibilityPlan =
        serde_json::from_str(diagnostic.compatibility_plan_json.as_deref().unwrap()).unwrap();
    assert_eq!(plan.class, ProviderStateCompatibilityClass::Incompatible);
    assert_eq!(
        plan.action,
        ProviderStateCompatibilityAction::ReconstructFromDurableProjection
    );
    assert!(
        diagnostic.is_current,
        "prior provider row remains inspectable"
    );

    let mut strategy_changed = provider_state_brain_registration_with_scope(
        "compatibility-responses-v4",
        "compatibility-profile",
        ProviderStateMode::Optional,
        "profile-fingerprint",
        "provider-fingerprint",
    );
    strategy_changed.strategy.as_mut().unwrap().strategy_id = "replay-v2".to_string();
    let mut strategy_facts = refreshed_snapshot_facts.clone();
    strategy_facts.brain_strategy = "replay-v2".to_string();
    strategy_changed
        .provider_state_scope
        .as_mut()
        .unwrap()
        .compatibility = Some(strategy_facts.clone());
    let strategy_handle = bridge
        .replace_brain_implementation(strategy_changed)
        .unwrap();
    let strategy_rebuilt = bridge
        .build_brain_wake_request_for_session(
            strategy_handle,
            session.session_id.clone(),
            "system".to_string(),
            b"{}".to_vec(),
            "wake-4".to_string(),
        )
        .unwrap();
    assert_eq!(
        strategy_rebuilt.request.provider_state_absence,
        Some(ProviderStateAbsenceReason::Invalidated)
    );
    let diagnostic = bridge.provider_state_diagnostics(1).unwrap().remove(0);
    let plan: ProviderStateCompatibilityPlan =
        serde_json::from_str(diagnostic.compatibility_plan_json.as_deref().unwrap()).unwrap();
    assert!(plan
        .changes
        .iter()
        .any(|change| change.dimension == "brain_strategy"));

    bridge
        .apply_provider_state_output(
            strategy_handle,
            &session.session_id,
            "wake-4-reconstructed",
            BrainWakeProviderStateOutput::Replace {
                state: BrainWakeProviderStateUpdate {
                    module_id: "openai-responses".to_string(),
                    strategy_id: "replay-v2".to_string(),
                    profile_fingerprint: "profile-fingerprint".to_string(),
                    provider_fingerprint: "provider-fingerprint".to_string(),
                    payload_version: "provider-owned-v2".to_string(),
                    payload: serde_json::json!({"response_id": "resp-strategy-v2"}),
                    ttl_ms: Some(60_000),
                },
            },
        )
        .unwrap();

    let mut module_changed = provider_state_brain_registration_with_scope(
        "compatibility-responses-v5",
        "compatibility-profile",
        ProviderStateMode::Optional,
        "profile-fingerprint",
        "provider-fingerprint",
    );
    let module_strategy = module_changed.strategy.as_mut().unwrap();
    module_strategy.module_id = "chat-completions".to_string();
    module_strategy.strategy_id = "replay-v2".to_string();
    strategy_facts.brain_module = "chat-completions".to_string();
    module_changed
        .provider_state_scope
        .as_mut()
        .unwrap()
        .compatibility = Some(strategy_facts);
    let module_handle = bridge.replace_brain_implementation(module_changed).unwrap();
    let module_rebuilt = bridge
        .build_brain_wake_request_for_session(
            module_handle,
            session.session_id,
            "system".to_string(),
            b"{}".to_vec(),
            "wake-5".to_string(),
        )
        .unwrap();
    assert_eq!(
        module_rebuilt.request.provider_state_absence,
        Some(ProviderStateAbsenceReason::Invalidated)
    );
    let diagnostic = bridge.provider_state_diagnostics(1).unwrap().remove(0);
    let plan: ProviderStateCompatibilityPlan =
        serde_json::from_str(diagnostic.compatibility_plan_json.as_deref().unwrap()).unwrap();
    assert!(plan
        .changes
        .iter()
        .any(|change| change.dimension == "brain_module"));
}

#[test]
fn native_bridge_submits_brain_events_to_the_engine() {
    let mut bridge = NativeBridge::new();
    bridge
        .initialize_engine(EngineConfig {
            engine_data_dir: std::env::temp_dir()
                .join(format!("rusty-crew-native-event-{}", std::process::id()))
                .to_string_lossy()
                .to_string(),
            clock: rusty_crew_core_bridge_api::ClockConfig::Fixed {
                at: "2026-06-19T00:00:00Z".to_string(),
            },
            default_turn_budget: 3,
            default_idle_timeout_ms: 1000,
            storage: None,
        })
        .unwrap();

    let receipt = bridge
        .submit_brain_event(BrainEventEnvelope {
            wake_id: "wake".to_string(),
            session_id: SessionId::new("session"),
            event: rusty_crew_core_bridge_api::BrainEvent::Started,
        })
        .unwrap();

    assert!(receipt.accepted);
}

#[test]
fn native_bridge_shutdown_reports_and_clears_subscriptions() {
    let mut bridge = NativeBridge::new();
    let engine = bridge
        .initialize_engine(EngineConfig {
            engine_data_dir: std::env::temp_dir()
                .join(format!("rusty-crew-native-shutdown-{}", std::process::id()))
                .to_string_lossy()
                .to_string(),
            clock: rusty_crew_core_bridge_api::ClockConfig::Fixed {
                at: "2026-06-19T00:00:00Z".to_string(),
            },
            default_turn_budget: 3,
            default_idle_timeout_ms: 1000,
            storage: None,
        })
        .unwrap();
    bridge
        .create_session(SessionConfig {
            session_id: SessionId::new("shutdown-session"),
            agent_id: AgentId::new("shutdown-agent"),
            profile_id: ProfileId::new("shutdown-profile"),
            kind: SessionKind::Full,
            delegation: None,
            workspace: None,
            resource_limits: ResourceLimits {
                max_duration_ms: None,
                max_delegation_depth: None,
            },
            tool_profile: ToolProfile { tools: vec![] },
            history_window: None,
        })
        .unwrap();
    let subscription = bridge
        .subscribe_events(EventSubscription {
            event_kinds: vec![CoreEventKind::SessionArchived],
            session_id: None,
            agent_id: None,
            adapter_id: None,
        })
        .unwrap();

    let summary = bridge
        .shutdown_engine(ShutdownRequest {
            engine,
            drain_timeout_ms: 25,
        })
        .unwrap();

    assert_eq!(summary.archived_sessions, 1);
    assert_eq!(summary.dropped_subscriptions, 1);
    let error = bridge
        .drain_subscription_events(subscription, 1)
        .expect_err("shutdown should clear native subscription handles");
    assert_eq!(error.kind, CoreErrorKind::NotFound);
}

#[test]
fn native_bridge_shutdown_cleans_buffered_brain_runs() {
    let mut bridge = NativeBridge::new();
    let engine = bridge
        .initialize_engine(EngineConfig {
            engine_data_dir: std::env::temp_dir()
                .join(format!(
                    "rusty-crew-native-buffered-shutdown-{}",
                    std::process::id()
                ))
                .to_string_lossy()
                .to_string(),
            clock: rusty_crew_core_bridge_api::ClockConfig::Fixed {
                at: "2026-06-19T00:00:00Z".to_string(),
            },
            default_turn_budget: 3,
            default_idle_timeout_ms: 1000,
            storage: None,
        })
        .unwrap();
    bridge
        .create_session(SessionConfig {
            session_id: SessionId::new("shutdown-buffered-session"),
            agent_id: AgentId::new("shutdown-buffered-agent"),
            profile_id: ProfileId::new("shutdown-buffered-profile"),
            kind: SessionKind::Full,
            delegation: None,
            workspace: None,
            resource_limits: ResourceLimits {
                max_duration_ms: None,
                max_delegation_depth: None,
            },
            tool_profile: ToolProfile { tools: vec![] },
            history_window: None,
        })
        .unwrap();

    let mut coordinator = rusty_crew_brain_runtime::BufferedBrainTurnCoordinator::new(
        "chat-completions",
        "shutdown-buffered-wake",
        SessionId::new("shutdown-buffered-session"),
        rusty_crew_brain_runtime::BufferedBrainTurnLimits::default(),
    )
    .unwrap();
    coordinator.start().unwrap();
    bridge
        .chat_completions_buffered_runs()
        .insert(rusty_crew_brain_runtime::BufferedBrainTurnRun::new(
            coordinator,
            crate::chat_completions::ChatCompletionsBufferedRunPayload::default(),
        ))
        .unwrap();
    let active = bridge.buffered_brain_run_diagnostics().unwrap();
    assert_eq!(active.active_run_count, 1);
    assert_eq!(active.runs[0].module_label, "chat-completions");
    assert_eq!(active.runs[0].wake_id, "shutdown-buffered-wake");
    assert_eq!(active.runs[0].session_id, "shutdown-buffered-session");
    assert_eq!(
        active.runs[0].agent_id.as_deref(),
        Some("shutdown-buffered-agent")
    );
    assert_eq!(
        active.runs[0].profile_id.as_deref(),
        Some("shutdown-buffered-profile")
    );
    assert_eq!(active.runs[0].phase, "running");

    bridge
        .shutdown_engine(ShutdownRequest {
            engine,
            drain_timeout_ms: 25,
        })
        .unwrap();

    let after_shutdown = bridge.buffered_brain_run_diagnostics().unwrap();
    assert_eq!(after_shutdown.active_run_count, 0);
}

#[test]
fn chat_completions_reasoning_compacts_past_stream_item_limit_without_losing_boundaries() {
    use rusty_crew_chat_completions_brain::{
        BrainEventContext, ChatCompletionsEvent, ChatCompletionsEventMapper,
        CANONICAL_REASONING_FORMAT,
    };

    let context = BrainEventContext::new(
        "stepfun-retention-wake",
        SessionId::new("stepfun-retention-session"),
    );
    let mut mapper = ChatCompletionsEventMapper::new();
    let mut coordinator = rusty_crew_brain_runtime::BufferedBrainTurnCoordinator::new(
        "chat-completions",
        context.wake_id.clone(),
        context.session_id.clone(),
        rusty_crew_brain_runtime::BufferedBrainTurnLimits {
            max_stream_items: 16,
            max_stream_delta_bytes: 8 * 1_024 * 1_024,
            ..rusty_crew_brain_runtime::BufferedBrainTurnLimits::default()
        },
    )
    .unwrap();
    coordinator.start().unwrap();

    let fields = [
        "reasoning_content",
        "reasoning",
        "reasoning_delta",
        "thinking",
    ];
    for index in 0..5_000 {
        if index == 2_500 {
            coordinator
                .enqueue_stream_item(BrainWakeStreamItem::event(BrainEventEnvelope {
                    wake_id: context.wake_id.clone(),
                    session_id: context.session_id.clone(),
                    event: BrainEvent::ToolCallStarted {
                        tool_name: "read_file".to_string(),
                        metadata: None,
                    },
                }))
                .unwrap();
            coordinator
                .enqueue_stream_item(BrainWakeStreamItem::event(BrainEventEnvelope {
                    wake_id: context.wake_id.clone(),
                    session_id: context.session_id.clone(),
                    event: BrainEvent::ToolCallFinished {
                        tool_name: "read_file".to_string(),
                        is_error: false,
                        metadata: None,
                    },
                }))
                .unwrap();
        }
        let mapped = mapper.map_provider_event(
            &context,
            &ChatCompletionsEvent::ReasoningDelta {
                text: "r".to_string(),
                field: fields[index % fields.len()].to_string(),
            },
        );
        coordinator
            .enqueue_stream_item(mapped.into_iter().next().unwrap())
            .unwrap();
    }
    coordinator
        .enqueue_stream_item(BrainWakeStreamItem::actions(BrainActionBatch {
            wake_id: context.wake_id.clone(),
            session_id: context.session_id,
            actions: Vec::new(),
        }))
        .unwrap();

    let metrics = coordinator.stream_retention_metrics();
    assert_eq!(metrics.raw_stream_item_count, 5_003);
    assert_eq!(metrics.raw_delta_item_count, 5_000);
    assert_eq!(metrics.retained_stream_item_count, 5);
    assert_eq!(metrics.coalesced_delta_item_count, 4_998);
    assert_eq!(metrics.dropped_stream_item_count, 0);
    assert_eq!(metrics.retained_delta_bytes, 5_000);
    assert_eq!(metrics.max_stream_items, 16);
    assert_eq!(metrics.max_stream_delta_bytes, 8 * 1_024 * 1_024);

    let drain = coordinator.drain_stream(16);
    assert!(drain.terminal);
    assert_eq!(drain.items.len(), 5);
    for item in [&drain.items[0], &drain.items[3]] {
        assert!(matches!(
            &item.item,
            BrainWakeStreamItem::Event { event }
                if matches!(&event.event, BrainEvent::ReasoningDelta { format, .. }
                    if format.as_deref() == Some(CANONICAL_REASONING_FORMAT))
        ));
    }
    assert!(matches!(
        &drain.items[1].item,
        BrainWakeStreamItem::Event { event }
            if matches!(event.event, BrainEvent::ToolCallStarted { .. })
    ));
    assert!(matches!(
        &drain.items[2].item,
        BrainWakeStreamItem::Event { event }
            if matches!(event.event, BrainEvent::ToolCallFinished { .. })
    ));
    assert!(matches!(
        &drain.items[4].item,
        BrainWakeStreamItem::Actions { .. }
    ));
}

#[test]
fn chat_completions_buffered_bridge_streams_started_and_tool_request_before_completion() {
    let bridge = NativeBridge::new();
    let registry = bridge.chat_completions_buffered_runs();
    let wake_id = "live-buffered-chat-wake";
    crate::chat_completions::start_chat_completions_brain_json(
        Arc::clone(&registry),
        serde_json::json!({
            "wakeId": wake_id,
            "sessionId": "live-buffered-chat-session",
            "messages": [{ "role": "user", "content": "use the tool" }],
            "tools": [{
                "name": "read_file",
                "description": "Read one file",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }
            }],
            "config": { "model": "fake-chat-model" }
        })
        .to_string(),
    )
    .unwrap();

    let mut pre_tool_items = Vec::new();
    let tool_request = (0..100)
        .find_map(|_| {
            let drain: serde_json::Value = serde_json::from_str(
                &crate::chat_completions::drain_chat_completions_brain_stream_json(
                    &registry,
                    wake_id.to_string(),
                    Some(64),
                )
                .unwrap(),
            )
            .unwrap();
            pre_tool_items.extend(drain["items"].as_array().unwrap().iter().cloned());
            let request = drain["tool_requests"].as_array().unwrap().first().cloned();
            if request.is_none() {
                thread::sleep(std::time::Duration::from_millis(5));
            }
            request
        })
        .expect("fake provider should request its host tool");

    assert!(pre_tool_items
        .iter()
        .any(|item| { item["type"] == "event" && item["event"]["event"]["type"] == "started" }));
    assert!(pre_tool_items.iter().any(|item| {
        item["type"] == "event" && item["event"]["event"]["type"] == "tool_call_started"
    }));
    assert!(!pre_tool_items
        .iter()
        .any(|item| { item["type"] == "actions" || item["type"] == "wake_failed" }));

    crate::chat_completions::submit_chat_completions_tool_output_json(
        &registry,
        serde_json::json!({
            "wakeId": wake_id,
            "callId": tool_request["call_id"],
            "output": "file contents",
            "status": "succeeded",
            "retryable": false
        })
        .to_string(),
    )
    .unwrap();

    let mut post_tool_items = Vec::new();
    let _terminal = (0..100)
        .find_map(|_| {
            let drain: serde_json::Value = serde_json::from_str(
                &crate::chat_completions::drain_chat_completions_brain_stream_json(
                    &registry,
                    wake_id.to_string(),
                    Some(64),
                )
                .unwrap(),
            )
            .unwrap();
            post_tool_items.extend(drain["items"].as_array().unwrap().iter().cloned());
            if drain["terminal"] == true {
                Some(drain)
            } else {
                thread::sleep(std::time::Duration::from_millis(5));
                None
            }
        })
        .expect("fake provider should finish after host tool output");
    assert_eq!(
        post_tool_items
            .iter()
            .filter(|item| item["type"] == "actions")
            .count(),
        1
    );
}

#[test]
fn chat_completions_buffered_bridge_pauses_repeated_failed_work_for_attention() {
    let bridge = NativeBridge::new();
    let registry = bridge.chat_completions_buffered_runs();
    let wake_id = "buffered-chat-no-progress";
    crate::chat_completions::start_chat_completions_brain_json(
        Arc::clone(&registry),
        serde_json::json!({
            "wakeId": wake_id,
            "sessionId": "buffered-chat-no-progress-session",
            "messages": [{ "role": "user", "content": "attempt the operation" }],
            "tools": [{
                "name": "no_progress_failure_tool",
                "description": "Always returns the same failed result",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }
            }],
            "config": {
                "model": "fake-chat-model",
                "noProgressAttentionThreshold": 3
            }
        })
        .to_string(),
    )
    .unwrap();

    let mut submitted_call_ids = Vec::new();
    let terminal = (0..400)
        .find_map(|_| {
            let drain: serde_json::Value = serde_json::from_str(
                &crate::chat_completions::drain_chat_completions_brain_stream_json(
                    &registry,
                    wake_id.to_string(),
                    Some(64),
                )
                .unwrap(),
            )
            .unwrap();
            for request in drain["tool_requests"].as_array().unwrap() {
                let call_id = request["call_id"].as_str().unwrap().to_string();
                submitted_call_ids.push(call_id.clone());
                crate::chat_completions::submit_chat_completions_tool_output_json(
                    &registry,
                    serde_json::json!({
                        "wakeId": wake_id,
                        "callId": call_id,
                        "output": "dependency remains unavailable",
                        "status": "failed",
                        "reasonCode": "dependency_unavailable",
                        "retryable": true
                    })
                    .to_string(),
                )
                .unwrap();
            }
            if drain["terminal"] == true {
                Some(drain)
            } else {
                thread::sleep(std::time::Duration::from_millis(5));
                None
            }
        })
        .expect("repeated failed work should pause for operator attention");

    assert_eq!(submitted_call_ids.len(), 4);
    assert_eq!(terminal["error"], serde_json::Value::Null);
    assert_eq!(terminal["yielded"], false);
    assert_eq!(
        terminal["terminal_reason_code"],
        "chat_completions_tool_no_progress"
    );
    assert_eq!(
        terminal["attention"]["reasonCode"],
        "chat_completions_tool_no_progress"
    );
    assert!(terminal["attention"]["resolutionActions"]
        .as_array()
        .is_some_and(|actions| !actions.is_empty()));
    assert!(terminal["continuation_state"].is_object());
    assert!(!terminal["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| { item["type"] == "actions" || item["type"] == "wake_failed" }));
}

#[test]
fn openai_responses_buffered_bridge_yields_and_resumes_without_repeating_tools() {
    let mut bridge = NativeBridge::new();
    bridge
        .initialize_engine(EngineConfig {
            engine_data_dir: std::env::temp_dir()
                .join(format!(
                    "rusty-crew-native-responses-continuation-{}",
                    std::process::id()
                ))
                .to_string_lossy()
                .to_string(),
            clock: rusty_crew_core_bridge_api::ClockConfig::Fixed {
                at: "2026-07-29T00:00:00Z".to_string(),
            },
            default_turn_budget: 3,
            default_idle_timeout_ms: 1000,
            storage: None,
        })
        .unwrap();
    bridge
        .create_session(SessionConfig {
            session_id: SessionId::new("responses-continuation-session"),
            agent_id: AgentId::new("responses-continuation-agent"),
            profile_id: ProfileId::new("responses-continuation-profile"),
            kind: SessionKind::Full,
            delegation: None,
            workspace: None,
            resource_limits: ResourceLimits {
                max_duration_ms: None,
                max_delegation_depth: None,
            },
            tool_profile: ToolProfile {
                tools: vec![ToolDescriptor {
                    name: "read_file".to_string(),
                    description: "Read one file".to_string(),
                    input_schema: None,
                }],
            },
            history_window: None,
        })
        .unwrap();
    let body_state: serde_json::Value = serde_json::from_slice(
        &bridge
            .project_body_state_json(SessionId::new("responses-continuation-session"))
            .unwrap(),
    )
    .unwrap();
    let registry = bridge.openai_responses_buffered_runs();
    let first_wake_id = "responses-continuation-epoch-1";
    crate::responses::start_openai_responses_brain_json(
        Arc::clone(&registry),
        serde_json::json!({
            "wakeId": first_wake_id,
            "sessionId": "responses-continuation-session",
            "bodyState": body_state,
            "config": {
                "model": "fake-responses-model",
                "responsesDialect": "openai_stateless",
                "strategyId": "replay",
                "workQuantumContinuationRounds": 1
            }
        })
        .to_string(),
    )
    .unwrap();

    let tool_request = (0..100)
        .find_map(|_| {
            let drain: serde_json::Value = serde_json::from_str(
                &crate::responses::drain_openai_responses_brain_stream_json(
                    &registry,
                    first_wake_id.to_string(),
                    Some(64),
                )
                .unwrap(),
            )
            .unwrap();
            let request = drain["tool_requests"].as_array().unwrap().first().cloned();
            if request.is_none() {
                thread::sleep(std::time::Duration::from_millis(5));
            }
            request
        })
        .expect("first Responses epoch should request its host tool");
    crate::responses::submit_openai_responses_tool_output_json(
        &registry,
        serde_json::json!({
            "wakeId": first_wake_id,
            "callId": tool_request["call_id"],
            "output": "file contents",
            "status": "succeeded",
            "retryable": false
        })
        .to_string(),
    )
    .unwrap();
    let yielded = (0..100)
        .find_map(|_| {
            let drain: serde_json::Value = serde_json::from_str(
                &crate::responses::drain_openai_responses_brain_stream_json(
                    &registry,
                    first_wake_id.to_string(),
                    Some(64),
                )
                .unwrap(),
            )
            .unwrap();
            if drain["terminal"] == true {
                Some(drain)
            } else {
                thread::sleep(std::time::Duration::from_millis(5));
                None
            }
        })
        .expect("first Responses epoch should yield after its work quantum");
    assert_eq!(yielded["yielded"], true);
    assert!(yielded["continuation_state"].is_object());
    assert!(!yielded["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["type"] == "actions" || item["type"] == "wake_failed"));

    let second_wake_id = "responses-continuation-epoch-2";
    crate::responses::start_openai_responses_brain_json(
        Arc::clone(&registry),
        serde_json::json!({
            "wakeId": second_wake_id,
            "sessionId": "responses-continuation-session",
            "bodyState": body_state,
            "continuationState": yielded["continuation_state"],
            "config": {
                "model": "fake-responses-model",
                "responsesDialect": "openai_stateless",
                "strategyId": "replay",
                "workQuantumContinuationRounds": 1
            }
        })
        .to_string(),
    )
    .unwrap();
    let completed = (0..100)
        .find_map(|_| {
            let drain: serde_json::Value = serde_json::from_str(
                &crate::responses::drain_openai_responses_brain_stream_json(
                    &registry,
                    second_wake_id.to_string(),
                    Some(64),
                )
                .unwrap(),
            )
            .unwrap();
            assert!(drain["tool_requests"].as_array().unwrap().is_empty());
            if drain["terminal"] == true {
                Some(drain)
            } else {
                thread::sleep(std::time::Duration::from_millis(5));
                None
            }
        })
        .expect("resumed Responses epoch should complete");
    assert_eq!(completed["yielded"], false);
    assert!(completed["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["type"] == "actions"));
}

#[test]
fn native_brain_host_result_bridges_keep_repeated_tool_failures_recoverable() {
    fn coordinator(
        module_id: &str,
        wake_id: &str,
        session_id: &str,
    ) -> rusty_crew_brain_runtime::BufferedBrainTurnCoordinator {
        let mut coordinator = rusty_crew_brain_runtime::BufferedBrainTurnCoordinator::new(
            module_id,
            wake_id,
            SessionId::new(session_id),
            rusty_crew_brain_runtime::BufferedBrainTurnLimits::default(),
        )
        .expect("coordinator");
        coordinator.start().expect("start coordinator");
        coordinator
    }

    fn request(call_id: &str) -> rusty_crew_brain_runtime::BufferedNeutralPendingToolRequest {
        rusty_crew_brain_runtime::BufferedNeutralPendingToolRequest {
            call_id: call_id.to_string(),
            provider_item_id: None,
            name: "patch".to_string(),
            arguments_json: format!(r#"{{"attempt":"{call_id}"}}"#),
        }
    }

    fn failed_input(wake_id: &str, call_id: &str) -> String {
        serde_json::json!({
            "wakeId": wake_id,
            "callId": call_id,
            "output": "patch target did not match",
            "status": "failed",
            "reasonCode": "tool_reported_unsuccessful_result",
            "retryable": true,
            "action": "failed"
        })
        .to_string()
    }

    let chat_registry = Arc::new(rusty_crew_brain_runtime::BufferedBrainTurnRegistry::new(
        "chat-completions",
    ));
    let mut chat_coordinator = coordinator("chat-completions", "chat-recovery", "chat-session");
    chat_coordinator
        .queue_tool_request(request("chat-call-1"))
        .expect("first chat request");
    chat_registry
        .insert(rusty_crew_brain_runtime::BufferedBrainTurnRun::new(
            chat_coordinator,
            crate::chat_completions::ChatCompletionsBufferedRunPayload::default(),
        ))
        .expect("insert chat run");
    crate::chat_completions::submit_chat_completions_tool_output_json(
        &chat_registry,
        failed_input("chat-recovery", "chat-call-1"),
    )
    .expect("first chat failure");
    chat_registry
        .with_run_mut("chat-recovery", |run| {
            run.coordinator.queue_tool_request(request("chat-call-2"))
        })
        .expect("chat run")
        .expect("second chat request");
    let chat_second: serde_json::Value = serde_json::from_str(
        &crate::chat_completions::submit_chat_completions_tool_output_json(
            &chat_registry,
            failed_input("chat-recovery", "chat-call-2"),
        )
        .expect("second chat failure"),
    )
    .expect("chat receipt");
    assert_eq!(
        chat_second["decision"]["recovery_guidance"]["reason_code"],
        "repeated_tool_failure_guidance"
    );
    chat_registry
        .with_run_mut("chat-recovery", |run| {
            assert_eq!(
                run.coordinator.phase(),
                rusty_crew_brain_runtime::BufferedBrainTurnPhase::Running
            );
            assert!(run.coordinator.terminal().is_none());
        })
        .expect("inspect chat run");

    let responses_registry = Arc::new(rusty_crew_brain_runtime::BufferedBrainTurnRegistry::new(
        "OpenAI Responses",
    ));
    let mut responses_coordinator = coordinator(
        "openai-responses",
        "responses-recovery",
        "responses-session",
    );
    responses_coordinator
        .queue_tool_request(request("responses-call-1"))
        .expect("first Responses request");
    responses_registry
        .insert(rusty_crew_brain_runtime::BufferedBrainTurnRun::new(
            responses_coordinator,
            crate::responses::OpenAiResponsesBufferedRunPayload::default(),
        ))
        .expect("insert Responses run");
    crate::responses::submit_openai_responses_tool_output_json(
        &responses_registry,
        failed_input("responses-recovery", "responses-call-1"),
    )
    .expect("first Responses failure");
    responses_registry
        .with_run_mut("responses-recovery", |run| {
            run.coordinator
                .queue_tool_request(request("responses-call-2"))
        })
        .expect("Responses run")
        .expect("second Responses request");
    let responses_second: serde_json::Value = serde_json::from_str(
        &crate::responses::submit_openai_responses_tool_output_json(
            &responses_registry,
            failed_input("responses-recovery", "responses-call-2"),
        )
        .expect("second Responses failure"),
    )
    .expect("Responses receipt");
    assert_eq!(
        responses_second["decision"]["recovery_guidance"]["reason_code"],
        "repeated_tool_failure_guidance"
    );
    responses_registry
        .with_run_mut("responses-recovery", |run| {
            assert_eq!(
                run.coordinator.phase(),
                rusty_crew_brain_runtime::BufferedBrainTurnPhase::Running
            );
            assert!(run.coordinator.terminal().is_none());
        })
        .expect("inspect Responses run");
}

#[test]
fn binding_runtime_census_tracks_native_wake_provider_and_tool_topology() {
    let binding = NativeBridgeBinding::new();
    let data_dir = std::env::temp_dir().join(format!(
        "rusty-crew-native-activity-{}-{}",
        std::process::id(),
        time::OffsetDateTime::now_utc().unix_timestamp_nanos()
    ));
    binding
        .initialize_engine(JsEngineConfig {
            engine_data_dir: data_dir.to_string_lossy().into_owned(),
            fixed_clock: None,
            default_turn_budget: 3,
            default_idle_timeout_ms: 1_000,
            storage_backend: None,
            postgres_database_url: None,
            postgres_schema: None,
            postgres_max_connections: None,
            postgres_statement_timeout_ms: None,
            backing_filesystem_path: None,
            filesystem_warning_free_percent: None,
        })
        .unwrap();
    binding
        .create_session(JsSessionConfig {
            session_id: "activity-native-session".into(),
            agent_id: "activity-native-agent".into(),
            profile_id: "activity-native-profile".into(),
            kind: "full".into(),
            workspace: None,
            resource_limits: None,
            tool_profile: None,
            history_window: None,
        })
        .unwrap();

    let wake_id = "activity-native-wake";
    binding
        .start_brain_run_json(
            "chat-completions".into(),
            serde_json::json!({
                "wakeId": wake_id,
                "sessionId": "activity-native-session",
                "messages": [{ "role": "user", "content": "use the tool" }],
                "tools": [{
                    "name": "read_file",
                    "description": "Read one file",
                    "inputSchema": {
                        "type": "object",
                        "properties": {},
                        "additionalProperties": false
                    }
                }],
                "config": { "model": "fake-chat-model" }
            })
            .to_string(),
        )
        .unwrap();

    let tool_request = (0..100)
        .find_map(|_| {
            let drain: serde_json::Value = serde_json::from_str(
                &binding
                    .drain_brain_run_json("chat-completions".into(), wake_id.into(), Some(64))
                    .unwrap(),
            )
            .unwrap();
            let request = drain["tool_requests"].as_array().unwrap().first().cloned();
            if request.is_none() {
                thread::sleep(std::time::Duration::from_millis(5));
            }
            request
        })
        .expect("fake chat-completions provider should request a host tool");
    let call_id = tool_request["call_id"].as_str().unwrap();

    let census: RuntimeActivityCensus =
        serde_json::from_str(&binding.runtime_activity_census_json("{}".into()).unwrap()).unwrap();
    let wake = census
        .active
        .iter()
        .find(|view| view.activity.activity_id.0 == format!("wake:{wake_id}"))
        .expect("native wake activity");
    assert_eq!(
        wake.activity.agent_id.as_ref().unwrap().0,
        "activity-native-agent"
    );
    let provider = census
        .active
        .iter()
        .find(|view| view.activity.activity_id.0 == format!("provider:{wake_id}"))
        .expect("provider activity");
    assert_eq!(
        provider.activity.parent_activity_id.as_ref().unwrap().0,
        format!("wake:{wake_id}")
    );
    let tool = census
        .active
        .iter()
        .find(|view| view.activity.tool_name.as_deref() == Some("read_file"))
        .expect("tool activity");
    assert_eq!(
        tool.activity.parent_activity_id.as_ref().unwrap().0,
        format!("provider:{wake_id}")
    );

    binding
        .submit_brain_host_result_json(
            "chat-completions".into(),
            serde_json::json!({
                "wakeId": wake_id,
                "callId": call_id,
                "output": "file contents",
                "status": "succeeded",
                "retryable": false
            })
            .to_string(),
        )
        .unwrap();
    let terminal = (0..100).any(|_| {
        let drain: serde_json::Value = serde_json::from_str(
            &binding
                .drain_brain_run_json("chat-completions".into(), wake_id.into(), Some(64))
                .unwrap(),
        )
        .unwrap();
        if drain["terminal"].as_bool() == Some(true) {
            true
        } else {
            thread::sleep(std::time::Duration::from_millis(5));
            false
        }
    });
    assert!(terminal);
    let after: RuntimeActivityCensus =
        serde_json::from_str(&binding.runtime_activity_census_json("{}".into()).unwrap()).unwrap();
    assert!(!after.active.iter().any(|view| {
        view.activity.wake_id.as_deref() == Some(wake_id)
            && view.activity.kind != RuntimeActivityKind::Dispatch
    }));

    let _ = std::fs::remove_dir_all(data_dir);
}

fn brain_registration(
    implementation_id: &str,
    profile_id: &str,
) -> BrainImplementationRegistration {
    brain_registration_with_tools(implementation_id, profile_id, Vec::new())
}

fn brain_registration_with_tools(
    implementation_id: &str,
    profile_id: &str,
    tools: Vec<&str>,
) -> BrainImplementationRegistration {
    BrainImplementationRegistration {
        implementation_id: BrainImplementationId::new(implementation_id),
        profile_id: ProfileId::new(profile_id),
        tool_profile: ToolProfile {
            tools: tools
                .into_iter()
                .map(|name| ToolDescriptor {
                    name: name.to_string(),
                    description: format!("{name} tool"),
                    input_schema: None,
                })
                .collect(),
        },
        model_config: BrainModelConfig {
            provider: "local".to_string(),
            model_name: "deterministic".to_string(),
            temperature_milli: None,
            max_output_tokens: None,
        },
        strategy: Some(rusty_crew_core_bridge_api::BrainStrategyMetadata::unused(
            "local", "default",
        )),
        provider_state_scope: None,
    }
}

fn provider_state_brain_registration(
    implementation_id: &str,
    profile_id: &str,
    mode: ProviderStateMode,
) -> BrainImplementationRegistration {
    provider_state_brain_registration_with_scope(
        implementation_id,
        profile_id,
        mode,
        "profile-fingerprint",
        "provider-fingerprint",
    )
}

fn provider_state_brain_registration_with_scope(
    implementation_id: &str,
    profile_id: &str,
    mode: ProviderStateMode,
    profile_fingerprint: &str,
    provider_fingerprint: &str,
) -> BrainImplementationRegistration {
    let mut registration = brain_registration(implementation_id, profile_id);
    registration.strategy = Some(BrainStrategyMetadata {
        module_id: "openai-responses".to_string(),
        strategy_id: "replay".to_string(),
        provider_state: BrainProviderStateStrategyMetadata { mode },
    });
    registration.provider_state_scope = Some(BrainProviderStateScope {
        profile_fingerprint: profile_fingerprint.to_string(),
        provider_fingerprint: provider_fingerprint.to_string(),
        compatibility: None,
    });
    registration
}

fn provider_state_compatibility_facts() -> ProviderStateCompatibilityFacts {
    ProviderStateCompatibilityFacts {
        version: "1".to_string(),
        profile_identity: "profile".to_string(),
        display_metadata: "display-v1".to_string(),
        prompt: "prompt-v1".to_string(),
        skills: "skills-v1".to_string(),
        tool_catalog: "tools-v1".to_string(),
        provider_endpoint: "endpoint-v1".to_string(),
        model: "model-v1".to_string(),
        protocol: "responses".to_string(),
        dialect: "openai-stateful".to_string(),
        reasoning_semantics: "reasoning-v1".to_string(),
        brain_module: "openai-responses".to_string(),
        brain_strategy: "replay".to_string(),
        provider_state_schema: "provider-owned-v1".to_string(),
    }
}

fn provider_state_session_config(session_id: &str, profile_id: &str) -> SessionConfig {
    SessionConfig {
        session_id: SessionId::new(session_id),
        agent_id: AgentId::new(format!("agent:{session_id}")),
        profile_id: ProfileId::new(profile_id),
        kind: SessionKind::Full,
        delegation: None,
        workspace: None,
        resource_limits: ResourceLimits {
            max_duration_ms: None,
            max_delegation_depth: None,
        },
        tool_profile: ToolProfile { tools: Vec::new() },
        history_window: None,
    }
}

struct FakeResponsesServer {
    addr: String,
    captured: Arc<Mutex<Option<String>>>,
}

impl FakeResponsesServer {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let captured = Arc::new(Mutex::new(None));
        let captured_for_thread = Arc::clone(&captured);
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = Vec::new();
            let mut chunk = [0_u8; 4096];
            loop {
                let read = stream.read(&mut chunk).unwrap();
                if read == 0 {
                    break;
                }
                buffer.extend_from_slice(&chunk[..read]);
                if request_complete(&buffer) {
                    break;
                }
            }
            let request_text = String::from_utf8_lossy(&buffer).to_lowercase();
            *captured_for_thread.lock().unwrap() = Some(request_text);
            let body = concat!(
                    "data: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}\n\n",
                    "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp-test\",\"usage\":{\"input_tokens\":1,\"input_tokens_details\":{\"cached_tokens\":0},\"output_tokens\":1,\"output_tokens_details\":{\"reasoning_tokens\":0},\"total_tokens\":2}}}\n\n",
                    "data: [DONE]\n\n"
                );
            let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
            stream.write_all(response.as_bytes()).unwrap();
        });
        Self { addr, captured }
    }

    fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn captured(&self) -> String {
        for _ in 0..100 {
            if let Some(captured) = self.captured.lock().unwrap().clone() {
                return captured;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("fake responses server did not capture a request");
    }
}

fn request_complete(buffer: &[u8]) -> bool {
    let text = String::from_utf8_lossy(buffer);
    let Some((headers, body)) = text.split_once("\r\n\r\n") else {
        return false;
    };
    let content_length = headers
        .lines()
        .find_map(|line| {
            line.strip_prefix("content-length:")
                .or_else(|| line.strip_prefix("Content-Length:"))
        })
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(0);
    body.len() >= content_length
}

fn test_jwt(exp: i64, extra: serde_json::Value) -> String {
    let mut payload = serde_json::json!({"exp": exp});
    let serde_json::Value::Object(payload_map) = &mut payload else {
        unreachable!();
    };
    if let serde_json::Value::Object(extra_map) = extra {
        for (key, value) in extra_map {
            payload_map.insert(key, value);
        }
    }
    format!(
        "{}.{}.{}",
        base64_url(r#"{"alg":"none"}"#.as_bytes()),
        base64_url(serde_json::to_string(&payload).unwrap().as_bytes()),
        "sig"
    )
}

fn base64_url(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut output = String::new();
    let mut index = 0;
    while index < bytes.len() {
        let a = bytes[index];
        let b = bytes.get(index + 1).copied().unwrap_or(0);
        let c = bytes.get(index + 2).copied().unwrap_or(0);
        output.push(TABLE[(a >> 2) as usize] as char);
        output.push(TABLE[(((a & 0b0000_0011) << 4) | (b >> 4)) as usize] as char);
        if index + 1 < bytes.len() {
            output.push(TABLE[(((b & 0b0000_1111) << 2) | (c >> 6)) as usize] as char);
        }
        if index + 2 < bytes.len() {
            output.push(TABLE[(c & 0b0011_1111) as usize] as char);
        }
        index += 3;
    }
    output
}
