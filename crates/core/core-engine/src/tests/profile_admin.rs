use super::*;

#[test]
fn normalized_model_deletes_are_revisioned_and_reference_safe() {
    let engine = test_engine();
    let endpoint = engine
        .upsert_model_endpoint(&rusty_crew_core_protocol::ModelEndpointWrite {
            endpoint_id: "shared-endpoint".to_string(),
            status: rusty_crew_core_protocol::ModelEndpointStatus::Active,
            display_name: None,
            description: None,
            base_url: "https://models.test/v1".to_string(),
            protocol: rusty_crew_core_protocol::ModelEndpointProtocol::Responses,
            wire_dialect: rusty_crew_core_protocol::ModelEndpointWireDialect::OpenaiStateful,
            auth_scheme: rusty_crew_core_protocol::ModelEndpointAuthScheme::None,
            credential_id: None,
            prompt_cache_transport: rusty_crew_core_protocol::PromptCacheTransport::None,
            metadata_json: serde_json::json!({}),
            expected_revision: None,
            now: "2026-08-13T00:00:00Z".to_string(),
        })
        .unwrap();
    let configuration = engine
        .upsert_model_configuration(&rusty_crew_core_protocol::ModelConfigurationWrite {
            model_config_id: "model-config".to_string(),
            endpoint_id: endpoint.endpoint_id.clone(),
            status: rusty_crew_core_protocol::ModelEndpointStatus::Active,
            display_name: None,
            description: None,
            model_id: "model-1".to_string(),
            context_window_tokens: None,
            max_output_tokens: None,
            temperature_milli: None,
            reasoning_effort: None,
            reasoning_format: None,
            reasoning_history: Default::default(),
            reasoning_budget_tokens: None,
            thinking_mode: Default::default(),
            prompt_caching_policy: Default::default(),
            capabilities: Default::default(),
            metadata_json: serde_json::json!({}),
            expected_revision: None,
            now: "2026-08-13T00:00:00Z".to_string(),
        })
        .unwrap();
    let mut profile = profile_registry_write("profile-1", "model-config", "unused-session");
    profile.active_runtime_settings_json = serde_json::json!({
        "modelConfigId": configuration.model_config_id,
    });
    profile.derived_runtime_refs.clear();
    engine.create_profile_registry_record(&profile).unwrap();

    let endpoint_error = engine
        .delete_model_endpoint(&rusty_crew_core_protocol::ModelEndpointDelete {
            endpoint_id: endpoint.endpoint_id.clone(),
            expected_revision: endpoint.revision,
        })
        .unwrap_err();
    assert_eq!(endpoint_error.kind, CoreErrorKind::ActionRejected);
    assert!(endpoint_error.message.contains("1 model configuration"));

    let configuration_error = engine
        .delete_model_configuration(&rusty_crew_core_protocol::ModelConfigurationDelete {
            model_config_id: configuration.model_config_id.clone(),
            expected_revision: configuration.revision,
        })
        .unwrap_err();
    assert_eq!(configuration_error.kind, CoreErrorKind::ActionRejected);
    assert!(configuration_error.message.contains("profile-1"));

    engine.purge_profile(&ProfileId::new("profile-1")).unwrap();
    engine
        .delete_model_configuration(&rusty_crew_core_protocol::ModelConfigurationDelete {
            model_config_id: configuration.model_config_id,
            expected_revision: configuration.revision,
        })
        .unwrap();
    engine
        .delete_model_endpoint(&rusty_crew_core_protocol::ModelEndpointDelete {
            endpoint_id: endpoint.endpoint_id,
            expected_revision: endpoint.revision,
        })
        .unwrap();
}

#[test]
fn model_provider_refresh_impact_uses_profile_registry_and_session_state() {
    let engine = test_engine();
    engine
        .create_profile_registry_record(&profile_registry_write(
            "planner-profile",
            "alternate",
            "configured-planner-session",
        ))
        .unwrap();
    engine
        .create_profile_registry_record(&profile_registry_write(
            "other-profile",
            "default",
            "other-session",
        ))
        .unwrap();
    engine
        .create_session(session_config(
            "active-planner-session",
            "planner",
            "planner-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine
        .create_session(session_config(
            "archived-planner-session",
            "planner-archived",
            "planner-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine
        .archive_session(&SessionId::new("archived-planner-session"))
        .unwrap();
    engine
        .create_session(session_config(
            "active-other-session",
            "other",
            "other-profile",
            SessionKind::Full,
        ))
        .unwrap();

    let impact = engine
        .model_provider_refresh_impact(&ModelProviderRefreshImpactRequest {
            provider_alias: "alternate".to_string(),
        })
        .unwrap();

    assert_eq!(impact.provider_alias, "alternate");
    assert_eq!(impact.affected_profiles.len(), 1);
    let affected = &impact.affected_profiles[0];
    assert_eq!(affected.profile_id, ProfileId::new("planner-profile"));
    assert_eq!(
        affected.configured_session_ids,
        vec![SessionId::new("configured-planner-session")]
    );
    assert_eq!(
        affected.active_session_ids,
        vec![SessionId::new("active-planner-session")]
    );
    assert_eq!(
        affected.session_ids,
        vec![
            SessionId::new("active-planner-session"),
            SessionId::new("configured-planner-session")
        ]
    );
}

#[test]
fn model_provider_refresh_plan_none_keeps_impact_but_no_actions() {
    let engine = test_engine();
    engine
        .create_profile_registry_record(&profile_registry_write(
            "planner-profile",
            "alternate",
            "configured-planner-session",
        ))
        .unwrap();

    let plan = engine
        .plan_model_provider_refresh(&ModelProviderRefreshPlanRequest {
            provider_alias: "alternate".to_string(),
            mode: ModelProviderRefreshMode::None,
        })
        .unwrap();

    assert_eq!(plan.provider_alias, "alternate");
    assert_eq!(plan.mode, ModelProviderRefreshMode::None);
    assert_eq!(plan.affected_profiles.len(), 1);
    assert!(plan.actions.is_empty());
}

#[test]
fn model_provider_refresh_plan_apply_builds_rebuild_actions() {
    let engine = test_engine();
    engine
        .create_profile_registry_record(&profile_registry_write(
            "planner-profile",
            "alternate",
            "configured-planner-session",
        ))
        .unwrap();

    let plan = engine
        .plan_model_provider_refresh(&ModelProviderRefreshPlanRequest {
            provider_alias: "alternate".to_string(),
            mode: ModelProviderRefreshMode::Apply,
        })
        .unwrap();

    assert_eq!(plan.provider_alias, "alternate");
    assert_eq!(plan.mode, ModelProviderRefreshMode::Apply);
    assert_eq!(plan.actions.len(), 1);
    let action = &plan.actions[0];
    assert_eq!(action.profile_id, ProfileId::new("planner-profile"));
    assert_eq!(action.command_name, "apply_runtime_rebuild");
    assert_eq!(action.reason, "model provider alternate updated");
    assert_eq!(
        action.applied_summary,
        "runtime rebuild applied for profile planner-profile"
    );
    assert_eq!(
        action.blocked_summary,
        "runtime rebuild blocked for profile planner-profile"
    );
    assert_eq!(action.failure_reason_code, "model_provider_refresh_failed");
}
