use super::*;
use rusty_crew_core_persistence::{
    RoleplayLoreLayerConfigWrite, RoleplayLoreReplace, RoleplayLoreWrite,
    RoleplayMechanicProposalApply, RoleplayMechanicProposalApplyOutcome,
    RoleplayMechanicProposalCapturedTarget, RoleplayMechanicProposalCreate,
    RoleplayMechanicProposalDecision, RoleplayMechanicProposalKind,
    RoleplayMechanicProposalPersist, RoleplayMechanicProposalQuery, RoleplayMechanicProposalRecord,
    RoleplayMechanicProposalStatus,
};
use rusty_crew_core_protocol::{MemoryProposalSource, ProfileRegistryUpdate};
use rusty_crew_roleplay_core::normalize_narrator_config;
use serde_json::{json, Map as JsonMap, Value as JsonValue};

impl CoreEngine {
    pub fn create_roleplay_mechanic_proposal(
        &self,
        create: &RoleplayMechanicProposalCreate,
    ) -> CoreResult<RoleplayMechanicProposalRecord> {
        self.require_mechanic_session(&create.mechanic_session_id)?;
        let association = self
            .get_roleplay_mechanic_session_association(&create.mechanic_session_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::ActionRejected,
                    format!(
                        "mechanic session {} has no roleplay association",
                        create.mechanic_session_id
                    ),
                )
            })?;
        if association.roleplay_session_id.as_deref() != Some(create.roleplay_session_id.as_str()) {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "mechanic session {} is not attached to roleplay session {}",
                    create.mechanic_session_id, create.roleplay_session_id
                ),
            ));
        }
        let metadata = self
            .get_roleplay_session_metadata(&create.roleplay_session_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    format!("roleplay session {} not found", create.roleplay_session_id),
                )
            })?;
        if metadata.archived {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "roleplay mechanic proposals cannot target an archived session",
            ));
        }
        let mut normalized = create.clone();
        normalized.proposed_value = normalize_proposed_value(create)?;
        let captured = self.capture_roleplay_proposal_target(&normalized, &metadata.profile_id)?;
        self.store
            .create_roleplay_mechanic_proposal(&RoleplayMechanicProposalPersist {
                create: normalized,
                captured,
            })
    }

    pub fn get_roleplay_mechanic_proposal(
        &self,
        proposal_id: &str,
    ) -> CoreResult<Option<RoleplayMechanicProposalRecord>> {
        self.store.get_roleplay_mechanic_proposal(proposal_id)
    }

    pub fn list_roleplay_mechanic_proposals(
        &self,
        query: &RoleplayMechanicProposalQuery,
    ) -> CoreResult<Vec<RoleplayMechanicProposalRecord>> {
        self.store.list_roleplay_mechanic_proposals(query)
    }

    pub fn decide_roleplay_mechanic_proposal(
        &self,
        decision: &RoleplayMechanicProposalDecision,
    ) -> CoreResult<RoleplayMechanicProposalRecord> {
        self.store.decide_roleplay_mechanic_proposal(decision)
    }

    pub fn apply_roleplay_mechanic_proposal(
        &self,
        apply: &RoleplayMechanicProposalApply,
    ) -> CoreResult<RoleplayMechanicProposalRecord> {
        let proposal = self
            .store
            .get_roleplay_mechanic_proposal(&apply.proposal_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    format!("roleplay mechanic proposal {} not found", apply.proposal_id),
                )
            })?;
        if proposal.status == RoleplayMechanicProposalStatus::Applied {
            return Ok(proposal);
        }
        if proposal.status != RoleplayMechanicProposalStatus::Approved {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "roleplay mechanic proposal {} must be approved before apply",
                    proposal.proposal_id
                ),
            ));
        }

        match self.apply_roleplay_proposal_target(&proposal, apply) {
            Ok((target_revision, outcome)) => self.store.record_roleplay_mechanic_proposal_apply(
                &RoleplayMechanicProposalApplyOutcome {
                    proposal_id: proposal.proposal_id,
                    actor_id: apply.actor_id.clone(),
                    applied: true,
                    target_revision,
                    outcome,
                    now: apply.now.clone(),
                },
            ),
            Err(error) if error.kind == CoreErrorKind::ActionRejected => {
                let conflict = json!({
                    "status": "conflict",
                    "reasonCode": "roleplay_mechanic_proposal_target_conflict",
                    "message": error.message,
                });
                self.store.record_roleplay_mechanic_proposal_apply(
                    &RoleplayMechanicProposalApplyOutcome {
                        proposal_id: proposal.proposal_id,
                        actor_id: apply.actor_id.clone(),
                        applied: false,
                        target_revision: None,
                        outcome: conflict,
                        now: apply.now.clone(),
                    },
                )?;
                Err(error)
            }
            Err(error) => Err(error),
        }
    }

    pub(crate) fn require_mechanic_session(
        &self,
        session_id: &SessionId,
    ) -> CoreResult<SessionState> {
        let session = self.get_session(session_id)?;
        let profile = self
            .get_profile_registry_record(&session.profile_id)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    format!("mechanic profile {} not found", session.profile_id.0),
                )
            })?;
        let configured = profile
            .active_runtime_settings_json
            .as_object()
            .and_then(|settings| {
                settings
                    .get("roleplayMechanic")
                    .or_else(|| settings.get("roleplay_mechanic"))
            })
            .is_some();
        if configured {
            Ok(session)
        } else {
            Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "session {} is not a roleplay mechanic session",
                    session_id.0
                ),
            ))
        }
    }

    fn capture_roleplay_proposal_target(
        &self,
        create: &RoleplayMechanicProposalCreate,
        profile_id: &str,
    ) -> CoreResult<RoleplayMechanicProposalCapturedTarget> {
        let profile_id = ProfileId(profile_id.to_string());
        match create.kind {
            RoleplayMechanicProposalKind::NarratorConfig
            | RoleplayMechanicProposalKind::Exemplar => {
                let profile = self
                    .get_profile_registry_record(&profile_id)?
                    .ok_or_else(|| {
                        CoreError::new(
                            CoreErrorKind::NotFound,
                            format!("target profile {} not found", profile_id.0),
                        )
                    })?;
                Ok(RoleplayMechanicProposalCapturedTarget {
                    profile_id,
                    target_revision: Some(profile.revision),
                    before_value: profile_target_value(
                        &profile.active_runtime_settings_json,
                        create.kind,
                    ),
                })
            }
            RoleplayMechanicProposalKind::LoreAdd => {
                let target_id = require_target_id(create)?;
                if self.get_roleplay_lore_record(target_id)?.is_some() {
                    return Err(CoreError::new(
                        CoreErrorKind::AlreadyExists,
                        format!("roleplay lore record {target_id} already exists"),
                    ));
                }
                Ok(RoleplayMechanicProposalCapturedTarget {
                    profile_id,
                    target_revision: None,
                    before_value: JsonValue::Null,
                })
            }
            RoleplayMechanicProposalKind::LoreEdit | RoleplayMechanicProposalKind::LoreTags => {
                let target_id = require_target_id(create)?;
                let record = self.get_roleplay_lore_record(target_id)?.ok_or_else(|| {
                    CoreError::new(
                        CoreErrorKind::NotFound,
                        format!("roleplay lore record {target_id} not found"),
                    )
                })?;
                let before_value = if create.kind == RoleplayMechanicProposalKind::LoreTags {
                    record
                        .content
                        .get("tags")
                        .cloned()
                        .unwrap_or_else(|| json!([]))
                } else {
                    serde_json::to_value(&record).map_err(json_error)?
                };
                Ok(RoleplayMechanicProposalCapturedTarget {
                    profile_id,
                    target_revision: Some(record.revision),
                    before_value,
                })
            }
            RoleplayMechanicProposalKind::LayerRetrievalConfig => {
                let target_id = require_target_id(create)?;
                let layer = self.get_lore_layer(target_id)?.ok_or_else(|| {
                    CoreError::new(
                        CoreErrorKind::NotFound,
                        format!("roleplay lore layer {target_id} not found"),
                    )
                })?;
                if layer.profile_id != profile_id.0 {
                    return Err(CoreError::new(
                        CoreErrorKind::ActionRejected,
                        format!("lore layer {target_id} is owned by another profile"),
                    ));
                }
                let current = self.get_lore_layer_config(target_id)?;
                Ok(RoleplayMechanicProposalCapturedTarget {
                    profile_id,
                    target_revision: None,
                    before_value: current
                        .map(|value| serde_json::to_value(value).map_err(json_error))
                        .transpose()?
                        .unwrap_or(JsonValue::Null),
                })
            }
        }
    }

    fn apply_roleplay_proposal_target(
        &self,
        proposal: &RoleplayMechanicProposalRecord,
        apply: &RoleplayMechanicProposalApply,
    ) -> CoreResult<(Option<u64>, JsonValue)> {
        match proposal.kind {
            RoleplayMechanicProposalKind::NarratorConfig
            | RoleplayMechanicProposalKind::Exemplar => {
                self.apply_profile_proposal(proposal, apply)
            }
            RoleplayMechanicProposalKind::LoreAdd => {
                let target_id = proposal
                    .target_id
                    .as_deref()
                    .ok_or_else(missing_target_id)?;
                if let Some(existing) = self.get_roleplay_lore_record(target_id)? {
                    if lore_record_matches_proposal(&existing, &proposal.proposed_value)? {
                        return Ok((
                            Some(existing.revision),
                            json!({
                                "status": "applied",
                                "targetType": "lore_record",
                                "targetId": target_id,
                                "recoveredExistingMutation": true,
                            }),
                        ));
                    }
                    return Err(target_conflict(proposal, Some(existing.revision)));
                }
                let mut write: RoleplayLoreWrite =
                    serde_json::from_value(proposal.proposed_value.clone()).map_err(json_error)?;
                write.now = apply.now.clone();
                let record = self.add_roleplay_lore_record(&write)?;
                Ok((
                    Some(record.revision),
                    json!({
                        "status": "applied",
                        "targetType": "lore_record",
                        "targetId": record.record_id,
                    }),
                ))
            }
            RoleplayMechanicProposalKind::LoreEdit => {
                let target_id = proposal
                    .target_id
                    .as_deref()
                    .ok_or_else(missing_target_id)?;
                let current = self
                    .get_roleplay_lore_record(target_id)?
                    .ok_or_else(|| target_conflict(proposal, None))?;
                if current.revision != proposal.target_revision.unwrap_or_default() {
                    if lore_record_matches_proposal(&current, &proposal.proposed_value)? {
                        return Ok((
                            Some(current.revision),
                            json!({
                                "status": "applied",
                                "targetType": "lore_record",
                                "targetId": target_id,
                                "recoveredExistingMutation": true,
                            }),
                        ));
                    }
                    return Err(target_conflict(proposal, Some(current.revision)));
                }
                let mut write: RoleplayLoreWrite =
                    serde_json::from_value(proposal.proposed_value.clone()).map_err(json_error)?;
                write.now = apply.now.clone();
                let record = self.replace_roleplay_lore_record(&RoleplayLoreReplace {
                    write,
                    expected_revision: current.revision,
                })?;
                Ok((
                    Some(record.revision),
                    json!({
                        "status": "applied",
                        "targetType": "lore_record",
                        "targetId": record.record_id,
                    }),
                ))
            }
            RoleplayMechanicProposalKind::LoreTags => {
                self.apply_lore_tags_proposal(proposal, apply)
            }
            RoleplayMechanicProposalKind::LayerRetrievalConfig => {
                self.apply_layer_config_proposal(proposal, apply)
            }
        }
    }

    fn apply_profile_proposal(
        &self,
        proposal: &RoleplayMechanicProposalRecord,
        apply: &RoleplayMechanicProposalApply,
    ) -> CoreResult<(Option<u64>, JsonValue)> {
        let current = self
            .get_profile_registry_record(&proposal.profile_id)?
            .ok_or_else(|| target_conflict(proposal, None))?;
        let current_value =
            profile_target_value(&current.active_runtime_settings_json, proposal.kind);
        if current_value == proposal_target_after_value(proposal)? {
            return Ok((
                Some(current.revision),
                json!({
                    "status": "applied",
                    "targetType": "profile_runtime_settings",
                    "profileId": proposal.profile_id.0,
                    "requiresRuntimeMaterialization": true,
                    "recoveredExistingMutation": true,
                }),
            ));
        }
        if current.revision != proposal.target_revision.unwrap_or_default()
            || current_value != proposal.before_value
        {
            return Err(target_conflict(proposal, Some(current.revision)));
        }
        let mut settings = current
            .active_runtime_settings_json
            .as_object()
            .cloned()
            .unwrap_or_default();
        apply_profile_target_value(&mut settings, proposal)?;
        let updated = self.update_profile_registry_record(&ProfileRegistryUpdate {
            write: profile_registry_write_with_settings(
                &current,
                JsonValue::Object(settings),
                &apply.now,
            ),
            expected_revision: current.revision,
        })?;
        Ok((
            Some(updated.revision),
            json!({
                "status": "applied",
                "targetType": "profile_runtime_settings",
                "profileId": updated.profile_id.0,
                "requiresRuntimeMaterialization": true,
            }),
        ))
    }

    fn apply_lore_tags_proposal(
        &self,
        proposal: &RoleplayMechanicProposalRecord,
        apply: &RoleplayMechanicProposalApply,
    ) -> CoreResult<(Option<u64>, JsonValue)> {
        let target_id = proposal
            .target_id
            .as_deref()
            .ok_or_else(missing_target_id)?;
        let current = self
            .get_roleplay_lore_record(target_id)?
            .ok_or_else(|| target_conflict(proposal, None))?;
        let current_tags = current
            .content
            .get("tags")
            .cloned()
            .unwrap_or_else(|| json!([]));
        if current_tags == proposal.proposed_value {
            return Ok((
                Some(current.revision),
                json!({
                    "status": "applied",
                    "targetType": "lore_tags",
                    "targetId": target_id,
                    "recoveredExistingMutation": true,
                }),
            ));
        }
        if current.revision != proposal.target_revision.unwrap_or_default()
            || current_tags != proposal.before_value
        {
            return Err(target_conflict(proposal, Some(current.revision)));
        }
        let mut content = current.content.as_object().cloned().unwrap_or_default();
        content.insert("tags".to_string(), proposal.proposed_value.clone());
        let write = lore_write_from_record(&current, JsonValue::Object(content), apply.now.clone());
        let updated = self.replace_roleplay_lore_record(&RoleplayLoreReplace {
            write,
            expected_revision: current.revision,
        })?;
        Ok((
            Some(updated.revision),
            json!({
                "status": "applied",
                "targetType": "lore_tags",
                "targetId": target_id,
            }),
        ))
    }

    fn apply_layer_config_proposal(
        &self,
        proposal: &RoleplayMechanicProposalRecord,
        apply: &RoleplayMechanicProposalApply,
    ) -> CoreResult<(Option<u64>, JsonValue)> {
        let target_id = proposal
            .target_id
            .as_deref()
            .ok_or_else(missing_target_id)?;
        let current = self.get_lore_layer_config(target_id)?;
        let current_value = current
            .as_ref()
            .map(serde_json::to_value)
            .transpose()
            .map_err(json_error)?
            .unwrap_or(JsonValue::Null);
        let mut write: RoleplayLoreLayerConfigWrite =
            serde_json::from_value(proposal.proposed_value.clone()).map_err(json_error)?;
        if layer_config_matches(current.as_ref(), &write) {
            return Ok((
                None,
                json!({
                    "status": "applied",
                    "targetType": "lore_layer_config",
                    "targetId": target_id,
                    "recoveredExistingMutation": true,
                }),
            ));
        }
        if current_value != proposal.before_value {
            return Err(target_conflict(proposal, None));
        }
        write.now = apply.now.clone();
        let updated = self.set_lore_layer_config(&write)?;
        Ok((
            None,
            json!({
                "status": "applied",
                "targetType": "lore_layer_config",
                "targetId": updated.layer_id,
            }),
        ))
    }
}

fn normalize_proposed_value(create: &RoleplayMechanicProposalCreate) -> CoreResult<JsonValue> {
    match create.kind {
        RoleplayMechanicProposalKind::NarratorConfig => {
            let normalized = normalize_narrator_config(create.proposed_value.clone())
                .map_err(|error| CoreError::new(CoreErrorKind::InvalidInput, error.to_string()))?;
            serde_json::to_value(normalized).map_err(json_error)
        }
        RoleplayMechanicProposalKind::Exemplar => {
            let value = create.proposed_value.as_str().ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "exemplar proposal value must be text",
                )
            })?;
            if value.trim().is_empty() || value.chars().count() > 100_000 {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "exemplar proposal value must contain 1 to 100000 characters",
                ));
            }
            Ok(JsonValue::String(value.to_string()))
        }
        RoleplayMechanicProposalKind::LoreAdd | RoleplayMechanicProposalKind::LoreEdit => {
            let write: RoleplayLoreWrite =
                serde_json::from_value(create.proposed_value.clone()).map_err(json_error)?;
            let target_id = require_target_id(create)?;
            if write.record_id != target_id {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "lore proposal target_id must match proposed record_id",
                ));
            }
            serde_json::to_value(write).map_err(json_error)
        }
        RoleplayMechanicProposalKind::LoreTags => {
            let tags = create.proposed_value.as_array().ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "lore tags must be an array of strings",
                )
            })?;
            if tags.len() > 100
                || tags.iter().any(|tag| {
                    tag.as_str()
                        .is_none_or(|value| value.trim().is_empty() || value.len() > 200)
                })
            {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "lore tags must contain at most 100 non-empty strings up to 200 bytes",
                ));
            }
            Ok(JsonValue::Array(tags.clone()))
        }
        RoleplayMechanicProposalKind::LayerRetrievalConfig => {
            let write: RoleplayLoreLayerConfigWrite =
                serde_json::from_value(create.proposed_value.clone()).map_err(json_error)?;
            let target_id = require_target_id(create)?;
            if write.layer_id != target_id {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "layer config proposal target_id must match proposed layer_id",
                ));
            }
            serde_json::to_value(write).map_err(json_error)
        }
    }
}

fn profile_target_value(settings: &JsonValue, kind: RoleplayMechanicProposalKind) -> JsonValue {
    let settings = settings.as_object();
    match kind {
        RoleplayMechanicProposalKind::NarratorConfig => settings
            .and_then(|value| value.get("roleplayNarrator"))
            .cloned()
            .unwrap_or(JsonValue::Null),
        RoleplayMechanicProposalKind::Exemplar => settings
            .and_then(|value| value.get("roleplayNarrator"))
            .and_then(JsonValue::as_object)
            .and_then(|value| value.get("exemplar"))
            .cloned()
            .unwrap_or(JsonValue::Null),
        _ => JsonValue::Null,
    }
}

fn proposal_target_after_value(proposal: &RoleplayMechanicProposalRecord) -> CoreResult<JsonValue> {
    match proposal.kind {
        RoleplayMechanicProposalKind::NarratorConfig | RoleplayMechanicProposalKind::Exemplar => {
            Ok(proposal.proposed_value.clone())
        }
        _ => Err(CoreError::new(
            CoreErrorKind::InternalError,
            "non-profile proposal passed to profile target projection",
        )),
    }
}

fn apply_profile_target_value(
    settings: &mut JsonMap<String, JsonValue>,
    proposal: &RoleplayMechanicProposalRecord,
) -> CoreResult<()> {
    match proposal.kind {
        RoleplayMechanicProposalKind::NarratorConfig => {
            settings.insert(
                "roleplayNarrator".to_string(),
                proposal.proposed_value.clone(),
            );
        }
        RoleplayMechanicProposalKind::Exemplar => {
            let narrator = settings
                .entry("roleplayNarrator".to_string())
                .or_insert_with(|| json!({}));
            let narrator = narrator.as_object_mut().ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::ActionRejected,
                    "current roleplay narrator config is not an object",
                )
            })?;
            narrator.insert("exemplar".to_string(), proposal.proposed_value.clone());
        }
        _ => {
            return Err(CoreError::new(
                CoreErrorKind::InternalError,
                "non-profile proposal passed to profile mutation",
            ));
        }
    }
    Ok(())
}

fn profile_registry_write_with_settings(
    current: &ProfileRegistryRecord,
    settings: JsonValue,
    now: &IsoTimestamp,
) -> ProfileRegistryWrite {
    ProfileRegistryWrite {
        profile_id: current.profile_id.clone(),
        lifecycle_status: current.lifecycle_status,
        display_name: current.display_name.clone(),
        summary: current.summary.clone(),
        default_session_kind: current.default_session_kind.clone(),
        agent_id: current.agent_id.clone(),
        owner_id: current.owner_id.clone(),
        prompt_soul_markdown: current.prompt_soul_markdown.clone(),
        prompt_memory_markdown: current.prompt_memory_markdown.clone(),
        active_runtime_settings_json: settings,
        source_asset_refs: current.source_asset_refs.clone(),
        derived_runtime_refs: current.derived_runtime_refs.clone(),
        import_export: current.import_export.clone(),
        now: now.clone(),
    }
}

fn lore_write_from_record(
    record: &RoleplayLoreRecord,
    content: JsonValue,
    now: IsoTimestamp,
) -> RoleplayLoreWrite {
    RoleplayLoreWrite {
        record_id: record.record_id.clone(),
        world_id: record.world_id.clone(),
        entity_id: record.entity_id.clone(),
        session_id: record.session_id.clone(),
        branch_id: record.branch_id.clone(),
        shape: record.shape.clone(),
        canon_status: record.canon_status,
        visibility: record.visibility,
        title: record.title.clone(),
        body: record.body.clone(),
        content,
        evidence_refs: record.evidence_refs.clone(),
        source: MemoryProposalSource::Human,
        confidence: record.confidence,
        durability_rationale: record.durability_rationale.clone(),
        supersedes_record_id: record.supersedes_record_id.clone(),
        now,
    }
}

fn lore_record_matches_proposal(
    record: &RoleplayLoreRecord,
    proposed: &JsonValue,
) -> CoreResult<bool> {
    let write: RoleplayLoreWrite = serde_json::from_value(proposed.clone()).map_err(json_error)?;
    Ok(record.record_id == write.record_id
        && record.world_id == write.world_id
        && record.entity_id == write.entity_id
        && record.session_id == write.session_id
        && record.branch_id == write.branch_id
        && record.shape == write.shape
        && record.canon_status == write.canon_status
        && record.visibility == write.visibility
        && record.title == write.title
        && record.body == write.body
        && record.content == write.content
        && record.evidence_refs == write.evidence_refs
        && record.source == write.source
        && record.confidence == write.confidence
        && record.durability_rationale == write.durability_rationale
        && record.supersedes_record_id == write.supersedes_record_id)
}

fn layer_config_matches(
    current: Option<&rusty_crew_core_persistence::RoleplayLoreLayerConfigRecord>,
    proposed: &RoleplayLoreLayerConfigWrite,
) -> bool {
    current.is_some_and(|current| {
        current.config_id == proposed.config_id
            && current.layer_id == proposed.layer_id
            && current.fts_weight == proposed.fts_weight
            && current.subject_weight == proposed.subject_weight
            && current.canon_weight == proposed.canon_weight
            && current.tag_boost_weight == proposed.tag_boost_weight
            && current.recency_weight == proposed.recency_weight
            && current.default_token_budget == proposed.default_token_budget
            && current.constant_token_reserve == proposed.constant_token_reserve
            && current.min_relevance_score == proposed.min_relevance_score
            && current.max_constants == proposed.max_constants
    })
}

fn require_target_id(create: &RoleplayMechanicProposalCreate) -> CoreResult<&str> {
    create.target_id.as_deref().ok_or_else(missing_target_id)
}

fn missing_target_id() -> CoreError {
    CoreError::new(
        CoreErrorKind::InvalidInput,
        "roleplay mechanic proposal kind requires target_id",
    )
}

fn target_conflict(
    proposal: &RoleplayMechanicProposalRecord,
    actual_revision: Option<u64>,
) -> CoreError {
    CoreError::new(
        CoreErrorKind::ActionRejected,
        format!(
            "roleplay mechanic proposal {} target changed: expected revision {:?}, found {:?}",
            proposal.proposal_id, proposal.target_revision, actual_revision
        ),
    )
}

fn json_error(error: impl std::fmt::Display) -> CoreError {
    CoreError::new(CoreErrorKind::InvalidInput, error.to_string())
}
