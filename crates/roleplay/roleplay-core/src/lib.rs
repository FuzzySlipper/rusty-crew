//! Deterministic roleplay domain helpers.
//!
//! This crate owns roleplay invariants that should not live in TypeScript route
//! glue. It intentionally accepts transport/storage-shaped DTOs so callers can
//! keep HTTP and persistence wiring outside the domain module.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

#[derive(Debug, Clone, Error, PartialEq, Eq)]
#[error("{message}")]
pub struct RoleplayDomainError {
    pub reason_code: &'static str,
    pub message: String,
}

impl RoleplayDomainError {
    fn invalid(reason_code: &'static str, message: impl Into<String>) -> Self {
        Self {
            reason_code,
            message: message.into(),
        }
    }
}

pub type RoleplayDomainResult<T> = Result<T, RoleplayDomainError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplayPromptContextInput {
    pub metadata: RoleplaySessionMetadata,
    #[serde(default)]
    pub player_persona: Option<RoleplayPlayerPersona>,
    #[serde(default)]
    pub character: Option<RoleplayCharacter>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplayPromptContextOutput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_context: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplaySpeakerIdentityInput {
    pub actor: RoleplayChatActor,
    pub now: String,
    #[serde(default)]
    pub metadata: Option<RoleplaySessionMetadata>,
    #[serde(default)]
    pub player_persona: Option<RoleplayPlayerPersona>,
    #[serde(default)]
    pub character: Option<RoleplayCharacter>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplaySpeakerIdentitySnapshot {
    pub speaker_kind: String,
    pub role: String,
    pub source_id: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_asset_ref: Option<String>,
    pub snapshot_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplayChatActor {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplaySessionMetadata {
    #[serde(alias = "sessionId")]
    pub session_id: String,
    #[serde(alias = "profileId")]
    pub profile_id: String,
    #[serde(default, alias = "displayName")]
    pub display_name: Option<String>,
    #[serde(default, alias = "playerPersonaId")]
    pub player_persona_id: Option<String>,
    #[serde(default, alias = "characterId")]
    pub character_id: Option<String>,
    #[serde(default, alias = "activeLayerIds")]
    pub active_layer_ids: Vec<String>,
    #[serde(default)]
    pub archived: bool,
    #[serde(alias = "createdAt")]
    pub created_at: String,
    #[serde(alias = "updatedAt")]
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplayPlayerPersona {
    pub id: String,
    #[serde(alias = "profileId")]
    pub profile_id: String,
    #[serde(alias = "displayName")]
    pub display_name: String,
    #[serde(default, alias = "avatarUrl")]
    pub avatar_url: Option<String>,
    #[serde(default, alias = "avatarAssetRef")]
    pub avatar_asset_ref: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default = "active_status")]
    pub status: String,
    #[serde(alias = "createdAt")]
    pub created_at: String,
    #[serde(default, alias = "updatedAt")]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplayCharacter {
    pub id: String,
    #[serde(alias = "profileId")]
    pub profile_id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub personality: String,
    #[serde(default)]
    pub scenario: String,
    #[serde(default, alias = "firstMessage")]
    pub first_message: String,
    #[serde(default, alias = "alternateGreetings")]
    pub alternate_greetings: Vec<String>,
    #[serde(default, alias = "exampleMessages")]
    pub example_messages: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, alias = "avatarUrl")]
    pub avatar_url: Option<String>,
    #[serde(default = "active_status")]
    pub status: String,
    #[serde(alias = "createdAt")]
    pub created_at: String,
    #[serde(default, alias = "updatedAt")]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplayAssistantAlternativePlanInput {
    pub session_id: String,
    #[serde(default)]
    pub requested_slot_id: Option<String>,
    #[serde(default)]
    pub slots: Vec<RoleplayMessageSlot>,
    #[serde(default)]
    pub active_branch_id: Option<String>,
    #[serde(default)]
    pub branches: Vec<RoleplayConversationBranch>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplayAssistantAlternativePlan {
    pub session_id: String,
    pub terminal_slot: RoleplayMessageSlot,
    pub active_variant: RoleplayMessageVariant,
    pub variant_projection: RoleplayAlternativeSlotProjection,
    pub next_alternate_ordinal: u32,
    pub branch_id_for_variant: Option<String>,
    pub parent_message_id: Option<String>,
    pub previous_message_id: Option<String>,
    pub branch_head_update: Option<RoleplayBranchHeadUpdatePlan>,
    pub append_chat_message: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplayBranchHeadUpdatePlan {
    pub branch_id: String,
    pub head_message_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplayAlternativeSlotProjection {
    pub slot_id: String,
    pub active_variant_id: Option<String>,
    pub primary_variant_id: String,
    pub alternate_count: u32,
    pub variant_count: u32,
    pub active_variant: RoleplayMessageVariant,
    pub variants: Vec<RoleplayMessageVariant>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplayMessageSlot {
    pub slot_id: String,
    pub session_id: String,
    pub primary_variant_id: String,
    #[serde(default)]
    pub active_variant_id: Option<String>,
    #[serde(default)]
    pub metadata_json: JsonValue,
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub version: u64,
    pub primary: RoleplayMessageVariant,
    #[serde(default)]
    pub alternates: Vec<RoleplayMessageVariant>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplayMessageVariant {
    pub variant_id: String,
    pub slot_id: String,
    pub source: String,
    pub ordinal: u32,
    pub status: String,
    pub message: RoleplayDurableMessage,
    #[serde(default)]
    pub metadata_json: JsonValue,
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplayDurableMessage {
    pub message_id: String,
    pub session_id: String,
    #[serde(default)]
    pub branch_id: Option<String>,
    #[serde(default)]
    pub parent_message_id: Option<String>,
    #[serde(default)]
    pub previous_message_id: Option<String>,
    pub author_id: String,
    pub author_role: String,
    pub status: String,
    pub body: String,
    #[serde(default)]
    pub metadata_json: JsonValue,
    pub created_at: String,
    #[serde(default)]
    pub blocks: Vec<JsonValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplayConversationBranch {
    pub branch_id: String,
    pub session_id: String,
    #[serde(default)]
    pub parent_branch_id: Option<String>,
    #[serde(default)]
    pub parent_message_id: Option<String>,
    #[serde(default)]
    pub origin_message_id: Option<String>,
    #[serde(default)]
    pub head_message_id: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub metadata_json: JsonValue,
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub version: u64,
}

pub fn plan_assistant_alternative(
    input: RoleplayAssistantAlternativePlanInput,
) -> RoleplayDomainResult<RoleplayAssistantAlternativePlan> {
    validate_unique_slots(&input.slots)?;
    let terminal = terminal_assistant_slot(
        &input.session_id,
        &input.slots,
        input.requested_slot_id.as_deref(),
        input.active_branch_id.as_deref(),
        &input.branches,
    )?;
    let active_variant = active_variant_for_slot(&terminal).clone();
    let branch_id_for_variant = terminal.primary.message.branch_id.clone();
    let branch_head_update =
        branch_id_for_variant
            .as_ref()
            .map(|branch_id| RoleplayBranchHeadUpdatePlan {
                branch_id: branch_id.clone(),
                head_message_id: active_variant.message.message_id.clone(),
            });
    Ok(RoleplayAssistantAlternativePlan {
        session_id: input.session_id,
        next_alternate_ordinal: next_alternate_ordinal(&terminal),
        parent_message_id: terminal.primary.message.parent_message_id.clone(),
        previous_message_id: terminal.primary.message.previous_message_id.clone(),
        branch_id_for_variant,
        branch_head_update,
        variant_projection: alternative_slot_projection(&terminal),
        active_variant,
        terminal_slot: terminal,
        append_chat_message: false,
    })
}

pub fn build_prompt_context(input: RoleplayPromptContextInput) -> RoleplayPromptContextOutput {
    let player_persona = active_persona(input.player_persona.as_ref());
    let character = active_character(input.character.as_ref());
    if player_persona.is_none() && character.is_none() && input.metadata.active_layer_ids.is_empty()
    {
        return RoleplayPromptContextOutput {
            prompt_context: None,
        };
    }

    let mut lines = vec!["# Roleplay Session Context".to_string()];
    if let Some(display_name) = non_empty(input.metadata.display_name.as_deref()) {
        lines.push(format!("Session: {display_name}"));
    }
    match player_persona {
        Some(persona) => {
            lines.push(format!("Player persona: {}", persona.display_name));
            if let Some(description) = non_empty(Some(persona.description.as_str())) {
                lines.push(format!("Player persona description: {description}"));
            }
            if let Some(notes) = non_empty(Some(persona.notes.as_str())) {
                lines.push(format!("Player persona notes: {notes}"));
            }
        }
        None => lines.push("Player persona: Player (default fallback)".to_string()),
    }
    if let Some(character) = character {
        lines.push(format!("Selected character: {}", character.name));
        if let Some(description) = non_empty(Some(character.description.as_str())) {
            lines.push(format!("Description: {description}"));
        }
        if let Some(personality) = non_empty(Some(character.personality.as_str())) {
            lines.push(format!("Personality: {personality}"));
        }
        if let Some(scenario) = non_empty(Some(character.scenario.as_str())) {
            lines.push(format!("Scenario: {scenario}"));
        }
        if let Some(first_message) = non_empty(Some(character.first_message.as_str())) {
            lines.push(format!("First message: {first_message}"));
        }
        if !character.alternate_greetings.is_empty() {
            lines.push(format!(
                "Alternate greetings: {}",
                character.alternate_greetings.join(" | ")
            ));
        }
        if !character.example_messages.is_empty() {
            lines.push(format!(
                "Example messages: {}",
                character.example_messages.join(" | ")
            ));
        }
    }
    if !input.metadata.active_layer_ids.is_empty() {
        lines.push(format!(
            "Active lore layers: {}",
            input.metadata.active_layer_ids.join(", ")
        ));
    }
    lines.push("Use this roleplay context as session-scoped setup. Prefer current chat evidence if it conflicts with older character or lore metadata.".to_string());

    RoleplayPromptContextOutput {
        prompt_context: Some(lines.join("\n")),
    }
}

pub fn speaker_identity_snapshot(
    input: RoleplaySpeakerIdentityInput,
) -> RoleplaySpeakerIdentitySnapshot {
    let role = match input.actor.kind.as_str() {
        "agent" => "assistant",
        "system" => "system",
        _ => "user",
    }
    .to_string();

    if role == "system" {
        return RoleplaySpeakerIdentitySnapshot {
            speaker_kind: "system".to_string(),
            role,
            source_id: input.actor.id.clone(),
            display_name: input
                .actor
                .display_name
                .as_deref()
                .or_else(|| non_empty(Some(input.actor.id.as_str())))
                .unwrap_or("System")
                .to_string(),
            avatar_url: None,
            avatar_asset_ref: None,
            snapshot_at: input.now,
        };
    }

    if role == "user" {
        if let Some(persona) = active_persona(input.player_persona.as_ref()) {
            return RoleplaySpeakerIdentitySnapshot {
                speaker_kind: "player_persona".to_string(),
                role,
                source_id: persona.id.clone(),
                display_name: persona.display_name.clone(),
                avatar_url: persona.avatar_url.clone(),
                avatar_asset_ref: persona.avatar_asset_ref.clone(),
                snapshot_at: input.now,
            };
        }
        return RoleplaySpeakerIdentitySnapshot {
            speaker_kind: "fallback_player".to_string(),
            role,
            source_id: input.actor.id,
            display_name: input
                .actor
                .display_name
                .unwrap_or_else(|| "Player".to_string()),
            avatar_url: None,
            avatar_asset_ref: None,
            snapshot_at: input.now,
        };
    }

    if let Some(character) = active_character(input.character.as_ref()) {
        return RoleplaySpeakerIdentitySnapshot {
            speaker_kind: "assistant_character".to_string(),
            role,
            source_id: character.id.clone(),
            display_name: character.name.clone(),
            avatar_url: character.avatar_url.clone(),
            avatar_asset_ref: None,
            snapshot_at: input.now,
        };
    }
    RoleplaySpeakerIdentitySnapshot {
        speaker_kind: "fallback_assistant".to_string(),
        role,
        source_id: input.actor.id,
        display_name: input
            .actor
            .display_name
            .unwrap_or_else(|| "Assistant".to_string()),
        avatar_url: None,
        avatar_asset_ref: None,
        snapshot_at: input.now,
    }
}

pub fn alternative_slot_projection(
    slot: &RoleplayMessageSlot,
) -> RoleplayAlternativeSlotProjection {
    let variants = live_variants(slot);
    RoleplayAlternativeSlotProjection {
        slot_id: slot.slot_id.clone(),
        active_variant_id: slot.active_variant_id.clone(),
        primary_variant_id: slot.primary_variant_id.clone(),
        alternate_count: slot
            .alternates
            .iter()
            .filter(|variant| variant.status != "deleted")
            .count() as u32,
        variant_count: variants.len() as u32,
        active_variant: active_variant_for_slot(slot).clone(),
        variants,
    }
}

pub fn active_variant_for_slot(slot: &RoleplayMessageSlot) -> &RoleplayMessageVariant {
    if let Some(active_variant_id) = slot.active_variant_id.as_deref() {
        if let Some(variant) = slot
            .alternates
            .iter()
            .chain(std::iter::once(&slot.primary))
            .find(|variant| variant.variant_id == active_variant_id && variant.status != "deleted")
        {
            return variant;
        }
    }
    &slot.primary
}

pub fn ordered_roleplay_slots(slots: &[RoleplayMessageSlot]) -> Vec<RoleplayMessageSlot> {
    let mut by_previous: BTreeMap<String, Vec<&RoleplayMessageSlot>> = BTreeMap::new();
    let mut roots = Vec::new();
    for slot in slots {
        match active_variant_for_slot(slot)
            .message
            .previous_message_id
            .as_deref()
        {
            Some(previous) => by_previous
                .entry(previous.to_string())
                .or_default()
                .push(slot),
            None => roots.push(slot),
        }
    }
    let mut ordered = Vec::new();
    let mut visited = BTreeSet::new();
    for root in sorted_slot_refs(roots) {
        append_slot_chain(root, &by_previous, &mut visited, &mut ordered);
    }
    for slot in sorted_slot_refs(slots.iter().collect()) {
        append_slot_chain(slot, &by_previous, &mut visited, &mut ordered);
    }
    ordered
}

fn terminal_assistant_slot(
    session_id: &str,
    slots: &[RoleplayMessageSlot],
    requested_slot_id: Option<&str>,
    active_branch_id: Option<&str>,
    branches: &[RoleplayConversationBranch],
) -> RoleplayDomainResult<RoleplayMessageSlot> {
    let terminal = active_branch_head_slot(active_branch_id, branches, slots)
        .or_else(|| ordered_roleplay_slots(slots).into_iter().last())
        .ok_or_else(|| {
            RoleplayDomainError::invalid(
                "roleplay_terminal_slot_missing",
                format!("roleplay session {session_id} has no terminal message slot"),
            )
        })?;

    if let Some(slot_id) = requested_slot_id {
        let explicit = slots
            .iter()
            .find(|slot| slot.slot_id == slot_id)
            .ok_or_else(|| {
                RoleplayDomainError::invalid(
                    "roleplay_requested_slot_missing",
                    format!("message slot {slot_id} was not found for {session_id}"),
                )
            })?;
        let explicit_role = active_variant_for_slot(explicit)
            .message
            .author_role
            .as_str();
        if explicit_role != "assistant" {
            return Err(RoleplayDomainError::invalid(
                "roleplay_requested_slot_not_assistant",
                format!(
                    "message slot {slot_id} is {explicit_role}; assistant alternatives are only available for assistant message slots"
                ),
            ));
        }
        if terminal.slot_id != slot_id {
            return Err(RoleplayDomainError::invalid(
                "roleplay_requested_slot_not_terminal",
                format!(
                    "message slot {slot_id} is not the current terminal assistant slot for {session_id}"
                ),
            ));
        }
        return Ok(terminal);
    }

    let terminal_role = active_variant_for_slot(&terminal)
        .message
        .author_role
        .as_str();
    if terminal_role != "assistant" {
        return Err(RoleplayDomainError::invalid(
            "roleplay_terminal_slot_not_assistant",
            format!(
                "roleplay session {session_id} terminal message is {terminal_role}; assistant alternatives are only available for the current terminal assistant message"
            ),
        ));
    }
    Ok(terminal)
}

fn active_branch_head_slot(
    active_branch_id: Option<&str>,
    branches: &[RoleplayConversationBranch],
    slots: &[RoleplayMessageSlot],
) -> Option<RoleplayMessageSlot> {
    let branch_id = active_branch_id?;
    let branch = branches
        .iter()
        .find(|candidate| candidate.branch_id == branch_id)?;
    let head_message_id = branch.head_message_id.as_deref()?;
    slots
        .iter()
        .find(|slot| {
            std::iter::once(&slot.primary)
                .chain(slot.alternates.iter())
                .any(|variant| variant.message.message_id == head_message_id)
        })
        .cloned()
}

fn append_slot_chain(
    slot: &RoleplayMessageSlot,
    by_previous: &BTreeMap<String, Vec<&RoleplayMessageSlot>>,
    visited: &mut BTreeSet<String>,
    ordered: &mut Vec<RoleplayMessageSlot>,
) {
    if !visited.insert(slot.slot_id.clone()) {
        return;
    }
    ordered.push(slot.clone());
    let message_id = active_variant_for_slot(slot).message.message_id.as_str();
    for child in sorted_slot_refs(by_previous.get(message_id).cloned().unwrap_or_default()) {
        append_slot_chain(child, by_previous, visited, ordered);
    }
}

fn sorted_slot_refs(mut slots: Vec<&RoleplayMessageSlot>) -> Vec<&RoleplayMessageSlot> {
    slots.sort_by(|left, right| {
        left.created_at
            .cmp(&right.created_at)
            .then_with(|| left.slot_id.cmp(&right.slot_id))
    });
    slots
}

fn live_variants(slot: &RoleplayMessageSlot) -> Vec<RoleplayMessageVariant> {
    std::iter::once(&slot.primary)
        .chain(slot.alternates.iter())
        .filter(|variant| variant.status != "deleted")
        .cloned()
        .collect()
}

fn next_alternate_ordinal(slot: &RoleplayMessageSlot) -> u32 {
    slot.alternates
        .iter()
        .filter(|variant| variant.status != "deleted")
        .map(|variant| variant.ordinal)
        .max()
        .unwrap_or(0)
        .saturating_add(1)
}

fn validate_unique_slots(slots: &[RoleplayMessageSlot]) -> RoleplayDomainResult<()> {
    let mut ids = BTreeSet::new();
    for slot in slots {
        if !ids.insert(slot.slot_id.as_str()) {
            return Err(RoleplayDomainError::invalid(
                "roleplay_duplicate_slot",
                format!("duplicate roleplay message slot {}", slot.slot_id),
            ));
        }
    }
    Ok(())
}

fn active_status() -> String {
    "active".to_string()
}

fn active_persona(persona: Option<&RoleplayPlayerPersona>) -> Option<&RoleplayPlayerPersona> {
    persona.filter(|persona| persona.status != "archived")
}

fn active_character(character: Option<&RoleplayCharacter>) -> Option<&RoleplayCharacter> {
    character.filter(|character| character.status != "archived")
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_current_branch_head_assistant_slot() {
        let user = slot(
            "slot-1",
            "msg-1",
            "user",
            None,
            None,
            "2026-07-07T00:00:00Z",
        );
        let assistant = slot(
            "slot-2",
            "msg-2",
            "assistant",
            Some("msg-1"),
            Some("branch-main"),
            "2026-07-07T00:01:00Z",
        );
        let plan = plan_assistant_alternative(RoleplayAssistantAlternativePlanInput {
            session_id: "session-rp".to_string(),
            requested_slot_id: Some("slot-2".to_string()),
            slots: vec![assistant.clone(), user],
            active_branch_id: Some("branch-main".to_string()),
            branches: vec![branch("branch-main", Some("msg-2"))],
        })
        .expect("plan should succeed");

        assert_eq!(plan.terminal_slot.slot_id, "slot-2");
        assert_eq!(plan.active_variant.message.message_id, "msg-2");
        assert_eq!(plan.next_alternate_ordinal, 1);
        assert_eq!(plan.branch_id_for_variant.as_deref(), Some("branch-main"));
        assert_eq!(plan.parent_message_id, None);
        assert_eq!(plan.previous_message_id.as_deref(), Some("msg-1"));
        assert_eq!(
            plan.branch_head_update,
            Some(RoleplayBranchHeadUpdatePlan {
                branch_id: "branch-main".to_string(),
                head_message_id: "msg-2".to_string()
            })
        );
        assert!(!plan.append_chat_message);
    }

    #[test]
    fn rejects_requested_stale_slot() {
        let first = slot(
            "slot-1",
            "msg-1",
            "assistant",
            None,
            None,
            "2026-07-07T00:00:00Z",
        );
        let second = slot(
            "slot-2",
            "msg-2",
            "assistant",
            Some("msg-1"),
            None,
            "2026-07-07T00:01:00Z",
        );
        let error = plan_assistant_alternative(RoleplayAssistantAlternativePlanInput {
            session_id: "session-rp".to_string(),
            requested_slot_id: Some("slot-1".to_string()),
            slots: vec![first, second],
            active_branch_id: None,
            branches: vec![],
        })
        .expect_err("stale slot should fail");

        assert_eq!(error.reason_code, "roleplay_requested_slot_not_terminal");
    }

    #[test]
    fn rejects_branch_head_that_points_to_user_slot() {
        let user = slot(
            "slot-1",
            "msg-1",
            "user",
            None,
            Some("branch-main"),
            "2026-07-07T00:00:00Z",
        );
        let assistant = slot(
            "slot-2",
            "msg-2",
            "assistant",
            Some("msg-1"),
            Some("branch-main"),
            "2026-07-07T00:01:00Z",
        );
        let error = plan_assistant_alternative(RoleplayAssistantAlternativePlanInput {
            session_id: "session-rp".to_string(),
            requested_slot_id: None,
            slots: vec![user, assistant],
            active_branch_id: Some("branch-main".to_string()),
            branches: vec![branch("branch-main", Some("msg-1"))],
        })
        .expect_err("user branch head should fail");

        assert_eq!(error.reason_code, "roleplay_terminal_slot_not_assistant");
    }

    #[test]
    fn active_variant_drives_projection_and_ordering() {
        let mut slot = slot(
            "slot-1",
            "msg-primary",
            "assistant",
            None,
            None,
            "2026-07-07T00:00:00Z",
        );
        slot.active_variant_id = Some("variant-alt".to_string());
        slot.alternates = vec![
            variant(
                "slot-1",
                "variant-alt",
                "msg-alt",
                "assistant",
                "active",
                1,
                None,
                None,
            ),
            variant(
                "slot-1",
                "variant-deleted",
                "msg-deleted",
                "assistant",
                "deleted",
                2,
                None,
                None,
            ),
        ];

        let projection = alternative_slot_projection(&slot);
        assert_eq!(projection.active_variant.variant_id, "variant-alt");
        assert_eq!(projection.alternate_count, 1);
        assert_eq!(projection.variant_count, 2);
        assert_eq!(next_alternate_ordinal(&slot), 2);
    }

    #[test]
    fn builds_prompt_context_for_selected_roleplay_records() {
        let output = build_prompt_context(RoleplayPromptContextInput {
            metadata: metadata(vec!["world".to_string(), "scene".to_string()]),
            player_persona: Some(persona(
                "Player Prime",
                "careful cartographer",
                "keeps notes",
            )),
            character: Some(character("Guide", "knows the city")),
        });
        let prompt = output.prompt_context.expect("prompt context");
        assert!(prompt.contains("Session: Evening run"));
        assert!(prompt.contains("Player persona: Player Prime"));
        assert!(prompt.contains("Player persona description: careful cartographer"));
        assert!(prompt.contains("Selected character: Guide"));
        assert!(prompt.contains("Description: knows the city"));
        assert!(prompt.contains("Active lore layers: world, scene"));
    }

    #[test]
    fn prompt_context_ignores_archived_records_but_keeps_layers() {
        let mut archived_persona = persona("Old Player", "archived", "");
        archived_persona.status = "archived".to_string();
        let mut archived_character = character("Old Guide", "archived");
        archived_character.status = "archived".to_string();

        let output = build_prompt_context(RoleplayPromptContextInput {
            metadata: metadata(vec!["scene".to_string()]),
            player_persona: Some(archived_persona),
            character: Some(archived_character),
        });
        let prompt = output.prompt_context.expect("layers keep context active");
        assert!(prompt.contains("Player persona: Player (default fallback)"));
        assert!(!prompt.contains("Old Guide"));
        assert!(prompt.contains("Active lore layers: scene"));
    }

    #[test]
    fn prompt_context_is_absent_without_records_or_layers() {
        let output = build_prompt_context(RoleplayPromptContextInput {
            metadata: metadata(vec![]),
            player_persona: None,
            character: None,
        });
        assert_eq!(output.prompt_context, None);
    }

    #[test]
    fn speaker_identity_projects_persona_character_and_fallbacks() {
        let now = "2026-07-07T00:00:00Z".to_string();
        let user = speaker_identity_snapshot(RoleplaySpeakerIdentityInput {
            actor: actor("human-1", "human", Some("Human Name")),
            now: now.clone(),
            metadata: Some(metadata(vec![])),
            player_persona: Some(persona("Player Prime", "", "")),
            character: None,
        });
        assert_eq!(user.speaker_kind, "player_persona");
        assert_eq!(user.display_name, "Player Prime");

        let assistant = speaker_identity_snapshot(RoleplaySpeakerIdentityInput {
            actor: actor("agent-1", "agent", None),
            now: now.clone(),
            metadata: Some(metadata(vec![])),
            player_persona: None,
            character: Some(character("Guide", "")),
        });
        assert_eq!(assistant.speaker_kind, "assistant_character");
        assert_eq!(assistant.display_name, "Guide");

        let fallback = speaker_identity_snapshot(RoleplaySpeakerIdentityInput {
            actor: actor("agent-1", "agent", None),
            now,
            metadata: Some(metadata(vec![])),
            player_persona: None,
            character: None,
        });
        assert_eq!(fallback.speaker_kind, "fallback_assistant");
        assert_eq!(fallback.display_name, "Assistant");
    }

    fn branch(branch_id: &str, head_message_id: Option<&str>) -> RoleplayConversationBranch {
        RoleplayConversationBranch {
            branch_id: branch_id.to_string(),
            session_id: "session-rp".to_string(),
            parent_branch_id: None,
            parent_message_id: None,
            origin_message_id: None,
            head_message_id: head_message_id.map(str::to_string),
            label: None,
            metadata_json: JsonValue::Object(Default::default()),
            created_at: "2026-07-07T00:00:00Z".to_string(),
            updated_at: "2026-07-07T00:00:00Z".to_string(),
            version: 1,
        }
    }

    fn metadata(active_layer_ids: Vec<String>) -> RoleplaySessionMetadata {
        RoleplaySessionMetadata {
            session_id: "session-rp".to_string(),
            profile_id: "profile-rp".to_string(),
            display_name: Some("Evening run".to_string()),
            player_persona_id: Some("persona-1".to_string()),
            character_id: Some("character-1".to_string()),
            active_layer_ids,
            archived: false,
            created_at: "2026-07-07T00:00:00Z".to_string(),
            updated_at: "2026-07-07T00:00:00Z".to_string(),
        }
    }

    fn persona(display_name: &str, description: &str, notes: &str) -> RoleplayPlayerPersona {
        RoleplayPlayerPersona {
            id: "persona-1".to_string(),
            profile_id: "profile-rp".to_string(),
            display_name: display_name.to_string(),
            avatar_url: Some("https://example.test/avatar.png".to_string()),
            avatar_asset_ref: Some("asset:avatar".to_string()),
            description: description.to_string(),
            notes: notes.to_string(),
            status: "active".to_string(),
            created_at: "2026-07-07T00:00:00Z".to_string(),
            updated_at: None,
        }
    }

    fn character(name: &str, description: &str) -> RoleplayCharacter {
        RoleplayCharacter {
            id: "character-1".to_string(),
            profile_id: "profile-rp".to_string(),
            name: name.to_string(),
            description: description.to_string(),
            personality: "warm".to_string(),
            scenario: "market square".to_string(),
            first_message: "Welcome back.".to_string(),
            alternate_greetings: vec!["Hello".to_string(), "Well met".to_string()],
            example_messages: vec!["Guide: stay close".to_string()],
            tags: vec![],
            avatar_url: Some("https://example.test/guide.png".to_string()),
            status: "active".to_string(),
            created_at: "2026-07-07T00:00:00Z".to_string(),
            updated_at: None,
        }
    }

    fn actor(id: &str, kind: &str, display_name: Option<&str>) -> RoleplayChatActor {
        RoleplayChatActor {
            id: id.to_string(),
            kind: kind.to_string(),
            display_name: display_name.map(str::to_string),
        }
    }

    fn slot(
        slot_id: &str,
        message_id: &str,
        author_role: &str,
        previous_message_id: Option<&str>,
        branch_id: Option<&str>,
        created_at: &str,
    ) -> RoleplayMessageSlot {
        RoleplayMessageSlot {
            slot_id: slot_id.to_string(),
            session_id: "session-rp".to_string(),
            primary_variant_id: format!("variant-{slot_id}"),
            active_variant_id: None,
            metadata_json: JsonValue::Object(Default::default()),
            created_at: created_at.to_string(),
            updated_at: created_at.to_string(),
            version: 1,
            primary: variant(
                slot_id,
                &format!("variant-{slot_id}"),
                message_id,
                author_role,
                "active",
                0,
                previous_message_id,
                branch_id,
            ),
            alternates: Vec::new(),
        }
    }

    fn variant(
        slot_id: &str,
        variant_id: &str,
        message_id: &str,
        author_role: &str,
        status: &str,
        ordinal: u32,
        previous_message_id: Option<&str>,
        branch_id: Option<&str>,
    ) -> RoleplayMessageVariant {
        RoleplayMessageVariant {
            variant_id: variant_id.to_string(),
            slot_id: slot_id.to_string(),
            source: if ordinal == 0 { "primary" } else { "alternate" }.to_string(),
            ordinal,
            status: status.to_string(),
            message: RoleplayDurableMessage {
                message_id: message_id.to_string(),
                session_id: "session-rp".to_string(),
                branch_id: branch_id.map(str::to_string),
                parent_message_id: None,
                previous_message_id: previous_message_id.map(str::to_string),
                author_id: "actor".to_string(),
                author_role: author_role.to_string(),
                status: "completed".to_string(),
                body: format!("body {message_id}"),
                metadata_json: JsonValue::Object(Default::default()),
                created_at: "2026-07-07T00:00:00Z".to_string(),
                blocks: Vec::new(),
            },
            metadata_json: JsonValue::Object(Default::default()),
            created_at: "2026-07-07T00:00:00Z".to_string(),
            updated_at: "2026-07-07T00:00:00Z".to_string(),
        }
    }
}
