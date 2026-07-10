use super::*;

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
