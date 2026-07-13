use super::*;
use rusty_crew_core_persistence::{
    RoleplayMechanicDiagnosticCreate, RoleplayMechanicDiagnosticOutcome,
    RoleplayMechanicDiagnosticOutcomeUpdate, RoleplayMechanicDiagnosticQuery,
    RoleplayMechanicProposalApply, RoleplayMechanicProposalCreate,
    RoleplayMechanicProposalDecision, RoleplayMechanicProposalDecisionKind,
    RoleplayMechanicProposalKind, RoleplayMechanicProposalQuery, RoleplayMechanicProposalStatus,
    RoleplayMechanicSessionAssociationCreate, RoleplayMechanicSessionAttachmentUpdate,
    RoleplaySessionMetadataRecord, RoleplaySessionMetadataWrite,
};
use rusty_crew_core_protocol::ProfileRegistryUpdate;

#[test]
fn mechanic_proposal_requires_review_and_applies_profile_change_idempotently() {
    let engine = roleplay_proposal_engine();
    let before = engine
        .get_profile_registry_record(&ProfileId::new("narrator-profile"))
        .unwrap()
        .unwrap();
    let create = narrator_config_proposal("proposal-one");

    let proposed = engine.create_roleplay_mechanic_proposal(&create).unwrap();
    assert_eq!(proposed.status, RoleplayMechanicProposalStatus::Proposed);
    assert_eq!(proposed.target_revision, Some(before.revision));
    assert_eq!(
        engine
            .get_profile_registry_record(&ProfileId::new("narrator-profile"))
            .unwrap()
            .unwrap()
            .revision,
        before.revision
    );
    assert_eq!(
        engine.create_roleplay_mechanic_proposal(&create).unwrap(),
        proposed
    );

    let decision = RoleplayMechanicProposalDecision {
        proposal_id: proposed.proposal_id.clone(),
        decision: RoleplayMechanicProposalDecisionKind::Approve,
        reviewer_id: "operator-one".to_string(),
        note: Some("Use the tighter pacing.".to_string()),
        expected_revision: proposed.revision,
        now: "2026-07-13T01:00:01Z".to_string(),
    };
    let approved = engine.decide_roleplay_mechanic_proposal(&decision).unwrap();
    assert_eq!(approved.status, RoleplayMechanicProposalStatus::Approved);
    assert_eq!(
        engine.decide_roleplay_mechanic_proposal(&decision).unwrap(),
        approved
    );

    let apply = RoleplayMechanicProposalApply {
        proposal_id: approved.proposal_id.clone(),
        actor_id: "operator-one".to_string(),
        now: "2026-07-13T01:00:02Z".to_string(),
    };
    let applied = engine.apply_roleplay_mechanic_proposal(&apply).unwrap();
    assert_eq!(applied.status, RoleplayMechanicProposalStatus::Applied);
    assert_eq!(applied.history.len(), 3);
    assert_eq!(
        engine.apply_roleplay_mechanic_proposal(&apply).unwrap(),
        applied
    );
    let profile = engine
        .get_profile_registry_record(&ProfileId::new("narrator-profile"))
        .unwrap()
        .unwrap();
    assert_eq!(
        profile.active_runtime_settings_json["roleplayNarrator"]["pacing"],
        "leisurely"
    );
}

#[test]
fn mechanic_proposal_rejection_and_stale_target_conflict_are_audited() {
    let engine = roleplay_proposal_engine();
    let rejected = engine
        .create_roleplay_mechanic_proposal(&RoleplayMechanicProposalCreate {
            proposal_id: "proposal-rejected".to_string(),
            mechanic_session_id: SessionId::new("mechanic-session"),
            roleplay_session_id: "roleplay-session".to_string(),
            kind: RoleplayMechanicProposalKind::Exemplar,
            target_id: None,
            proposed_value: serde_json::json!("A clean, spare exemplar."),
            rationale: "Reduce ornamental drift.".to_string(),
            diagnostic_context: serde_json::json!({}),
            now: "2026-07-13T02:00:00Z".to_string(),
        })
        .unwrap();
    let rejected = engine
        .decide_roleplay_mechanic_proposal(&RoleplayMechanicProposalDecision {
            proposal_id: rejected.proposal_id,
            decision: RoleplayMechanicProposalDecisionKind::Reject,
            reviewer_id: "operator-one".to_string(),
            note: Some("Keep the current voice.".to_string()),
            expected_revision: rejected.revision,
            now: "2026-07-13T02:00:01Z".to_string(),
        })
        .unwrap();
    assert_eq!(rejected.status, RoleplayMechanicProposalStatus::Rejected);
    assert_eq!(rejected.history.len(), 2);

    let proposed = engine
        .create_roleplay_mechanic_proposal(&narrator_config_proposal("proposal-stale"))
        .unwrap();
    let mut profile = engine
        .get_profile_registry_record(&ProfileId::new("narrator-profile"))
        .unwrap()
        .unwrap();
    profile.active_runtime_settings_json["unrelatedChange"] = serde_json::json!(true);
    let current_revision = profile.revision;
    engine
        .update_profile_registry_record(&ProfileRegistryUpdate {
            write: ProfileRegistryWrite {
                profile_id: profile.profile_id.clone(),
                lifecycle_status: profile.lifecycle_status,
                display_name: profile.display_name,
                summary: profile.summary,
                default_session_kind: profile.default_session_kind,
                agent_id: profile.agent_id,
                owner_id: profile.owner_id,
                prompt_soul_markdown: profile.prompt_soul_markdown,
                prompt_memory_markdown: profile.prompt_memory_markdown,
                active_runtime_settings_json: profile.active_runtime_settings_json,
                source_asset_refs: profile.source_asset_refs,
                derived_runtime_refs: profile.derived_runtime_refs,
                import_export: profile.import_export,
                now: "2026-07-13T02:00:02Z".to_string(),
            },
            expected_revision: current_revision,
        })
        .unwrap();
    let approved = engine
        .decide_roleplay_mechanic_proposal(&RoleplayMechanicProposalDecision {
            proposal_id: proposed.proposal_id.clone(),
            decision: RoleplayMechanicProposalDecisionKind::Approve,
            reviewer_id: "operator-one".to_string(),
            note: None,
            expected_revision: proposed.revision,
            now: "2026-07-13T02:00:03Z".to_string(),
        })
        .unwrap();
    let error = engine
        .apply_roleplay_mechanic_proposal(&RoleplayMechanicProposalApply {
            proposal_id: approved.proposal_id.clone(),
            actor_id: "operator-one".to_string(),
            now: "2026-07-13T02:00:04Z".to_string(),
        })
        .unwrap_err();
    assert_eq!(error.kind, CoreErrorKind::ActionRejected);
    let conflicted = engine
        .get_roleplay_mechanic_proposal(&approved.proposal_id)
        .unwrap()
        .unwrap();
    assert_eq!(conflicted.status, RoleplayMechanicProposalStatus::Approved);
    assert_eq!(conflicted.history.len(), 3);
    assert_eq!(conflicted.outcome.unwrap()["status"], "conflict");
}

#[test]
fn mechanic_proposal_history_survives_restart() {
    let data_dir = unique_data_dir("roleplay-mechanic-proposals");
    {
        let engine = test_engine_with_data_dir(data_dir.clone());
        seed_roleplay_proposal_engine(&engine);
        engine
            .create_roleplay_mechanic_proposal(&narrator_config_proposal("proposal-restart"))
            .unwrap();
    }
    let reopened = test_engine_with_data_dir(data_dir);
    let records = reopened
        .list_roleplay_mechanic_proposals(&RoleplayMechanicProposalQuery {
            roleplay_session_id: Some("roleplay-session".to_string()),
            ..RoleplayMechanicProposalQuery::default()
        })
        .unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].proposal_id, "proposal-restart");
    assert_eq!(records[0].history.len(), 1);
}

#[test]
fn mechanic_association_survives_restart_and_rejects_mismatched_roleplay_profiles() {
    let data_dir = unique_data_dir("roleplay-mechanic-associations");
    {
        let engine = test_engine_with_data_dir(data_dir.clone());
        seed_roleplay_proposal_engine(&engine);
        let association = engine
            .get_roleplay_mechanic_session_association(&SessionId::new("mechanic-session"))
            .unwrap()
            .unwrap();
        assert_eq!(
            association.roleplay_session_id.as_deref(),
            Some("roleplay-session")
        );

        engine
            .create_session(session_config(
                "mismatched-roleplay-session",
                "narrator-agent",
                "narrator-profile",
                SessionKind::Full,
            ))
            .unwrap();
        engine
            .put_roleplay_session_metadata(&RoleplaySessionMetadataWrite {
                record: RoleplaySessionMetadataRecord {
                    session_id: "mismatched-roleplay-session".to_string(),
                    profile_id: "different-profile".to_string(),
                    display_name: None,
                    player_persona_id: None,
                    character_id: None,
                    active_layer_ids: vec![],
                    archived: false,
                    narrator_diagnostic: None,
                    revision: 0,
                    created_at: "2026-07-13T04:00:00Z".to_string(),
                    updated_at: "2026-07-13T04:00:00Z".to_string(),
                },
                expected_revision: None,
            })
            .unwrap();
        let error = engine
            .update_roleplay_mechanic_session_attachment(&RoleplayMechanicSessionAttachmentUpdate {
                mechanic_session_id: SessionId::new("mechanic-session"),
                roleplay_session_id: Some("mismatched-roleplay-session".to_string()),
                expected_revision: association.revision,
                now: "2026-07-13T04:00:01Z".to_string(),
            })
            .unwrap_err();
        assert_eq!(error.kind, CoreErrorKind::ActionRejected);

        engine
            .archive_session(&SessionId::new("mechanic-session"))
            .unwrap();
        assert_eq!(
            engine
                .get_session(&SessionId::new("roleplay-session"))
                .unwrap()
                .status,
            SessionStatus::Idle
        );
    }
    let reopened = test_engine_with_data_dir(data_dir);
    let association = reopened
        .get_roleplay_mechanic_session_association(&SessionId::new("mechanic-session"))
        .unwrap()
        .unwrap();
    assert_eq!(association.mechanic_profile_id.0, "mechanic-profile");
    assert_eq!(
        reopened
            .get_session(&SessionId::new("mechanic-session"))
            .unwrap()
            .status,
        SessionStatus::Archived
    );
}

#[test]
fn mechanic_diagnostics_link_applied_proposals_and_protect_outcome_revisions() {
    let data_dir = unique_data_dir("roleplay-mechanic-diagnostics");
    {
        let engine = test_engine_with_data_dir(data_dir.clone());
        seed_roleplay_proposal_engine(&engine);
        let applied = approve_and_apply(&engine, narrator_config_proposal("proposal-diagnostic"));
        let diagnostic = engine
            .create_roleplay_mechanic_diagnostic(&RoleplayMechanicDiagnosticCreate {
                diagnostic_id: "diagnostic-one".to_string(),
                mechanic_session_id: SessionId::new("mechanic-session"),
                roleplay_session_id: "roleplay-session".to_string(),
                symptom: "Scene transitions rush established beats.".to_string(),
                hypothesis: "Narrator pacing is too fast.".to_string(),
                proposal_ids: vec![applied.proposal_id.clone()],
                applied_proposal_ids: vec![applied.proposal_id],
                notes: Some("Observe the next three turns.".to_string()),
                now: "2026-07-13T05:00:00Z".to_string(),
            })
            .unwrap();
        let updated = engine
            .update_roleplay_mechanic_diagnostic_outcome(&RoleplayMechanicDiagnosticOutcomeUpdate {
                diagnostic_id: diagnostic.diagnostic_id.clone(),
                outcome: RoleplayMechanicDiagnosticOutcome::Improved,
                notes: Some("Transitions now preserve scene beats.".to_string()),
                expected_revision: diagnostic.revision,
                now: "2026-07-13T05:01:00Z".to_string(),
            })
            .unwrap();
        assert_eq!(updated.outcome, RoleplayMechanicDiagnosticOutcome::Improved);
        let conflict = engine
            .update_roleplay_mechanic_diagnostic_outcome(&RoleplayMechanicDiagnosticOutcomeUpdate {
                diagnostic_id: diagnostic.diagnostic_id,
                outcome: RoleplayMechanicDiagnosticOutcome::Worse,
                notes: None,
                expected_revision: 1,
                now: "2026-07-13T05:02:00Z".to_string(),
            })
            .unwrap_err();
        assert_eq!(conflict.kind, CoreErrorKind::ActionRejected);
    }
    let reopened = test_engine_with_data_dir(data_dir);
    let records = reopened
        .list_roleplay_mechanic_diagnostics(&RoleplayMechanicDiagnosticQuery {
            roleplay_session_id: Some("roleplay-session".to_string()),
            ..RoleplayMechanicDiagnosticQuery::default()
        })
        .unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0].outcome,
        RoleplayMechanicDiagnosticOutcome::Improved
    );
    assert_eq!(records[0].revision, 2);
}

#[test]
fn mechanic_proposals_apply_every_supported_lore_and_provider_kind() {
    let engine = roleplay_proposal_engine();

    approve_and_apply(
        &engine,
        RoleplayMechanicProposalCreate {
            proposal_id: "proposal-provider-pattern".to_string(),
            mechanic_session_id: SessionId::new("mechanic-session"),
            roleplay_session_id: "roleplay-session".to_string(),
            kind: RoleplayMechanicProposalKind::ProviderFailurePattern,
            target_id: None,
            proposed_value: serde_json::json!({
                "pattern": "provider returned an empty narrative",
                "classification": "empty_output"
            }),
            rationale: "Preserve a diagnosed provider failure signature.".to_string(),
            diagnostic_context: serde_json::json!({"wakeId": "wake-provider"}),
            now: "2026-07-13T03:00:00Z".to_string(),
        },
    );
    let profile = engine
        .get_profile_registry_record(&ProfileId::new("narrator-profile"))
        .unwrap()
        .unwrap();
    assert_eq!(
        profile.active_runtime_settings_json["roleplayProviderFailurePatterns"][0]["pattern"],
        "provider returned an empty narrative"
    );

    let lore_write = serde_json::json!({
        "record_id": "lore-observatory",
        "world_id": "world-one",
        "entity_id": "observatory",
        "session_id": "roleplay-session",
        "branch_id": null,
        "shape": { "shape_id": "lore_entry", "version": 1 },
        "canon_status": "canon",
        "visibility": "public",
        "title": "Brass Observatory",
        "body": "The blue bell opens the observatory door.",
        "content": {
            "world_id": "world-one",
            "entity_id": "observatory",
            "title": "Brass Observatory",
            "body": "The blue bell opens the observatory door.",
            "canon_status": "canon",
            "visibility": "public",
            "metadata_json": { "source": "proposal-test" },
            "tags": ["observatory"]
        },
        "evidence_refs": [{
            "evidence_type": "source_document",
            "ref_id": "task-5690-all-kinds",
            "label": "proposal coverage"
        }],
        "source": "human",
        "confidence": 1.0,
        "durability_rationale": "Mechanic proposal test fixture.",
        "supersedes_record_id": null,
        "now": "2026-07-13T03:01:00Z"
    });
    approve_and_apply(
        &engine,
        proposal_with_target(
            "proposal-lore-add",
            RoleplayMechanicProposalKind::LoreAdd,
            "lore-observatory",
            lore_write.clone(),
        ),
    );
    assert_eq!(
        engine
            .get_roleplay_lore_record("lore-observatory")
            .unwrap()
            .unwrap()
            .body,
        "The blue bell opens the observatory door."
    );

    let mut edited_lore = lore_write;
    edited_lore["body"] =
        serde_json::json!("The blue bell opens the observatory door at midnight.");
    edited_lore["content"]["body"] =
        serde_json::json!("The blue bell opens the observatory door at midnight.");
    edited_lore["now"] = serde_json::json!("2026-07-13T03:02:00Z");
    approve_and_apply(
        &engine,
        proposal_with_target(
            "proposal-lore-edit",
            RoleplayMechanicProposalKind::LoreEdit,
            "lore-observatory",
            edited_lore,
        ),
    );
    approve_and_apply(
        &engine,
        proposal_with_target(
            "proposal-lore-tags",
            RoleplayMechanicProposalKind::LoreTags,
            "lore-observatory",
            serde_json::json!(["observatory", "blue-bell"]),
        ),
    );
    let lore = engine
        .get_roleplay_lore_record("lore-observatory")
        .unwrap()
        .unwrap();
    assert_eq!(
        lore.body,
        "The blue bell opens the observatory door at midnight."
    );
    assert_eq!(
        lore.content["tags"],
        serde_json::json!(["observatory", "blue-bell"])
    );

    engine
        .create_lore_layer(
            &serde_json::from_value(serde_json::json!({
                "layer_id": "layer-world",
                "profile_id": "narrator-profile",
                "name": "World",
                "description": "World facts",
                "purpose": "world",
                "write_policy": "manual",
                "now": "2026-07-13T03:03:00Z"
            }))
            .unwrap(),
        )
        .unwrap();
    approve_and_apply(
        &engine,
        proposal_with_target(
            "proposal-layer-config",
            RoleplayMechanicProposalKind::LayerRetrievalConfig,
            "layer-world",
            serde_json::json!({
                "config_id": "config-layer-world",
                "layer_id": "layer-world",
                "fts_weight": 1.2,
                "subject_weight": 1.1,
                "canon_weight": 0.8,
                "tag_boost_weight": 0.6,
                "recency_weight": 0.1,
                "default_token_budget": 3200,
                "constant_token_reserve": 400,
                "min_relevance_score": 0.4,
                "max_constants": 4,
                "now": "2026-07-13T03:04:00Z"
            }),
        ),
    );
    let config = engine
        .get_lore_layer_config("layer-world")
        .unwrap()
        .unwrap();
    assert_eq!(config.default_token_budget, 3200);
    assert_eq!(config.max_constants, 4);
}

fn roleplay_proposal_engine() -> CoreEngine {
    let engine = test_engine();
    seed_roleplay_proposal_engine(&engine);
    engine
}

fn seed_roleplay_proposal_engine(engine: &CoreEngine) {
    let mut mechanic = profile_registry_write("mechanic-profile", "default", "mechanic-session");
    mechanic.active_runtime_settings_json = serde_json::json!({
        "providerAlias": "default",
        "roleplayMechanic": { "name": "Mechanic", "autoMonitor": false },
    });
    engine.create_profile_registry_record(&mechanic).unwrap();
    let mut narrator = profile_registry_write("narrator-profile", "default", "roleplay-session");
    narrator.active_runtime_settings_json = serde_json::json!({
        "providerAlias": "default",
        "roleplayNarrator": {
            "tone": "lush",
            "pacing": "balanced",
            "explicitness": "implied",
            "memoryDepth": "medium",
            "review": { "enabled": false, "maxReviewCycles": 0 }
        },
    });
    engine.create_profile_registry_record(&narrator).unwrap();
    engine
        .create_session(session_config(
            "mechanic-session",
            "mechanic-agent",
            "mechanic-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine
        .create_session(session_config(
            "roleplay-session",
            "narrator-agent",
            "narrator-profile",
            SessionKind::Full,
        ))
        .unwrap();
    engine
        .put_roleplay_session_metadata(&RoleplaySessionMetadataWrite {
            record: RoleplaySessionMetadataRecord {
                session_id: "roleplay-session".to_string(),
                profile_id: "narrator-profile".to_string(),
                display_name: Some("Test roleplay".to_string()),
                player_persona_id: None,
                character_id: None,
                active_layer_ids: vec![],
                archived: false,
                narrator_diagnostic: None,
                revision: 0,
                created_at: "2026-07-13T00:00:00Z".to_string(),
                updated_at: "2026-07-13T00:00:00Z".to_string(),
            },
            expected_revision: None,
        })
        .unwrap();
    engine
        .create_roleplay_mechanic_session_association(&RoleplayMechanicSessionAssociationCreate {
            mechanic_session_id: SessionId::new("mechanic-session"),
            roleplay_session_id: Some("roleplay-session".to_string()),
            now: "2026-07-13T00:00:01Z".to_string(),
        })
        .unwrap();
}

fn narrator_config_proposal(proposal_id: &str) -> RoleplayMechanicProposalCreate {
    RoleplayMechanicProposalCreate {
        proposal_id: proposal_id.to_string(),
        mechanic_session_id: SessionId::new("mechanic-session"),
        roleplay_session_id: "roleplay-session".to_string(),
        kind: RoleplayMechanicProposalKind::NarratorConfig,
        target_id: None,
        proposed_value: serde_json::json!({
            "tone": "lush",
            "pacing": "leisurely",
            "explicitness": "implied",
            "memoryDepth": "medium",
            "stylePrompt": "Favor concrete sensory continuity.",
            "review": { "enabled": false, "maxReviewCycles": 0 }
        }),
        rationale: "The last three turns rushed scene transitions.".to_string(),
        diagnostic_context: serde_json::json!({"traceIds": ["trace-one"]}),
        now: "2026-07-13T01:00:00Z".to_string(),
    }
}

fn proposal_with_target(
    proposal_id: &str,
    kind: RoleplayMechanicProposalKind,
    target_id: &str,
    proposed_value: serde_json::Value,
) -> RoleplayMechanicProposalCreate {
    RoleplayMechanicProposalCreate {
        proposal_id: proposal_id.to_string(),
        mechanic_session_id: SessionId::new("mechanic-session"),
        roleplay_session_id: "roleplay-session".to_string(),
        kind,
        target_id: Some(target_id.to_string()),
        proposed_value,
        rationale: format!("Exercise {proposal_id}."),
        diagnostic_context: serde_json::json!({"test": proposal_id}),
        now: "2026-07-13T03:00:00Z".to_string(),
    }
}

fn approve_and_apply(
    engine: &CoreEngine,
    create: RoleplayMechanicProposalCreate,
) -> rusty_crew_core_persistence::RoleplayMechanicProposalRecord {
    let proposed = engine.create_roleplay_mechanic_proposal(&create).unwrap();
    let approved = engine
        .decide_roleplay_mechanic_proposal(&RoleplayMechanicProposalDecision {
            proposal_id: proposed.proposal_id.clone(),
            decision: RoleplayMechanicProposalDecisionKind::Approve,
            reviewer_id: "operator-all-kinds".to_string(),
            note: None,
            expected_revision: proposed.revision,
            now: "2026-07-13T03:10:00Z".to_string(),
        })
        .unwrap();
    engine
        .apply_roleplay_mechanic_proposal(&RoleplayMechanicProposalApply {
            proposal_id: approved.proposal_id,
            actor_id: "operator-all-kinds".to_string(),
            now: "2026-07-13T03:11:00Z".to_string(),
        })
        .unwrap()
}
