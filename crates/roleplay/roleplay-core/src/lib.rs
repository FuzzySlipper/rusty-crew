//! Deterministic roleplay domain helpers.
//!
//! This crate owns roleplay invariants that should not live in TypeScript route
//! glue. It intentionally accepts transport/storage-shaped DTOs so callers can
//! keep HTTP and persistence wiring outside the domain module.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};
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
    #[serde(default)]
    pub scene_setup: Option<String>,
    #[serde(default)]
    pub relevant_lore: Vec<RoleplayPromptStackSourceText>,
    #[serde(default)]
    pub recent_history: Vec<RoleplayPromptStackSourceText>,
    #[serde(default)]
    pub response_guidance: Option<String>,
    #[serde(default)]
    pub imported_prompt_blocks: Vec<RoleplayPromptStackRawBlock>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplayPromptContextOutput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_context: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack: Option<RoleplayPromptStackOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RoleplayPromptStackSourceText {
    pub source_kind: String,
    pub source_id: String,
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub editable: bool,
    #[serde(default)]
    pub derived: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RoleplayPromptStackRawBlock {
    pub source_kind: String,
    pub source_id: String,
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub metadata_json: JsonValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RoleplayPromptStackOutput {
    pub version: u32,
    pub compiled_text: String,
    pub messages: Vec<RoleplayPromptStackMessage>,
    pub sections: Vec<RoleplayPromptStackSection>,
    pub trace: Vec<RoleplayPromptStackTraceEntry>,
    #[serde(default)]
    pub macro_resolutions: Vec<RoleplayPromptMacroResolution>,
    #[serde(default)]
    pub imported_prompt_blocks: Vec<RoleplayPromptStackRawBlock>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RoleplayPromptStackMessage {
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub section_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RoleplayPromptStackSection {
    pub id: String,
    pub title: String,
    pub body: String,
    pub source_kind: String,
    pub source_id: String,
    pub inclusion_reason: String,
    pub token_estimate: u32,
    pub editable: bool,
    pub derived: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RoleplayPromptStackTraceEntry {
    pub section_id: String,
    pub source_kind: String,
    pub source_id: String,
    pub inclusion_reason: String,
    pub token_estimate: u32,
    pub editable: bool,
    pub derived: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RoleplayPromptMacroResolution {
    pub macro_name: String,
    pub replacement: String,
    pub occurrences: u32,
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
#[serde(rename_all = "camelCase")]
pub struct RoleplaySessionMetadata {
    #[serde(alias = "session_id")]
    pub session_id: String,
    #[serde(alias = "profile_id")]
    pub profile_id: String,
    #[serde(
        default,
        alias = "display_name",
        skip_serializing_if = "Option::is_none"
    )]
    pub display_name: Option<String>,
    #[serde(
        default,
        alias = "player_persona_id",
        skip_serializing_if = "Option::is_none"
    )]
    pub player_persona_id: Option<String>,
    #[serde(
        default,
        alias = "character_id",
        skip_serializing_if = "Option::is_none"
    )]
    pub character_id: Option<String>,
    #[serde(default, alias = "active_layer_ids")]
    pub active_layer_ids: Vec<String>,
    #[serde(default)]
    pub archived: bool,
    #[serde(alias = "created_at")]
    pub created_at: String,
    #[serde(alias = "updated_at")]
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayPlayerPersona {
    pub id: String,
    #[serde(alias = "profile_id")]
    pub profile_id: String,
    #[serde(alias = "display_name")]
    pub display_name: String,
    #[serde(default, alias = "avatar_url", skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    #[serde(
        default,
        alias = "avatar_asset_ref",
        skip_serializing_if = "Option::is_none"
    )]
    pub avatar_asset_ref: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default = "active_status")]
    pub status: String,
    #[serde(alias = "created_at")]
    pub created_at: String,
    #[serde(default, alias = "updated_at", skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayCharacter {
    pub id: String,
    #[serde(alias = "profile_id")]
    pub profile_id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub personality: String,
    #[serde(default)]
    pub scenario: String,
    #[serde(default, alias = "first_message")]
    pub first_message: String,
    #[serde(default, alias = "alternate_greetings")]
    pub alternate_greetings: Vec<String>,
    #[serde(default, alias = "example_messages")]
    pub example_messages: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, alias = "avatar_url", skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    #[serde(default = "active_status")]
    pub status: String,
    #[serde(alias = "created_at")]
    pub created_at: String,
    #[serde(default, alias = "updated_at", skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplayCharacterWriteInput {
    pub profile_id: String,
    pub now: String,
    pub fallback_id: String,
    pub body: JsonValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplayCharacterMergeInput {
    pub current: RoleplayCharacter,
    pub now: String,
    pub body: JsonValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplayPlayerPersonaWriteInput {
    pub profile_id: String,
    pub now: String,
    pub fallback_id: String,
    pub body: JsonValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplayPlayerPersonaMergeInput {
    pub current: RoleplayPlayerPersona,
    pub now: String,
    pub body: JsonValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplaySessionMetadataPatchInput {
    pub current: RoleplaySessionMetadata,
    pub session_id: String,
    pub profile_id: String,
    pub now: String,
    pub body: JsonValue,
    #[serde(default)]
    pub player_persona: Option<RoleplayPlayerPersona>,
    #[serde(default)]
    pub character: Option<RoleplayCharacter>,
    #[serde(default)]
    pub available_layer_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplaySessionMetadataPatchOutput {
    pub metadata: RoleplaySessionMetadata,
    pub active_layer_ids_changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayNarratorConfig {
    pub tone: String,
    pub pacing: String,
    pub explicitness: String,
    pub memory_depth: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exemplar: Option<String>,
    pub review: RoleplayNarratorReviewConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayNarratorReviewConfig {
    pub enabled: bool,
    pub max_review_cycles: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleplayNarratorPhaseKind {
    Explore,
    Compose,
    ComposeDraft,
    Review,
    Done,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayNarratorToolRequest {
    pub tool_name: String,
    #[serde(default)]
    pub params_json: JsonValue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayNarratorToolObservation {
    pub tool_name: String,
    pub ok: bool,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details_json: Option<JsonValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayNarratorMandatoryExploreInput {
    pub session_id: String,
    pub profile_id: String,
    #[serde(default)]
    pub pending_text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayNarratorAutoCaptureInput {
    pub session_id: String,
    pub profile_id: String,
    pub wake_id: String,
    #[serde(default)]
    pub pending_text: String,
    #[serde(default)]
    pub layer_details_json: JsonValue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayNarratorPhasePlan {
    pub phase: RoleplayNarratorPhaseKind,
    #[serde(default)]
    pub instructions: String,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub mandatory_tool_requests: Vec<RoleplayNarratorToolRequest>,
    pub state: RoleplayNarratorTurnState,
    pub terminal: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayNarratorTurnState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub narrator_config: Option<RoleplayNarratorConfig>,
    pub review_enabled: bool,
    pub max_review_cycles: u32,
    pub review_cycle: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene_brief: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_feedback: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayNarratorStartInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub narrator_config: Option<RoleplayNarratorConfig>,
    pub review_enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_review_cycles: Option<u32>,
    #[serde(default)]
    pub prelude_observations: Vec<RoleplayNarratorToolObservation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayNarratorNextInput {
    pub state: RoleplayNarratorTurnState,
    pub completed_phase: RoleplayNarratorPhaseKind,
    #[serde(default)]
    pub output_text: String,
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
    if player_persona.is_none()
        && character.is_none()
        && input.metadata.active_layer_ids.is_empty()
        && input
            .scene_setup
            .as_deref()
            .and_then(|value| non_empty(Some(value)))
            .is_none()
        && input.relevant_lore.is_empty()
        && input.recent_history.is_empty()
        && input
            .response_guidance
            .as_deref()
            .and_then(|value| non_empty(Some(value)))
            .is_none()
    {
        return RoleplayPromptContextOutput {
            prompt_context: None,
            stack: None,
        };
    }

    let character_name = character
        .map(|record| record.name.as_str())
        .and_then(|name| non_empty(Some(name)))
        .unwrap_or("Assistant");
    let user_name = player_persona
        .map(|record| record.display_name.as_str())
        .and_then(|name| non_empty(Some(name)))
        .unwrap_or("Player");
    let mut macro_tracker = RoleplayMacroTracker::new(character_name, user_name);
    let mut sections = Vec::new();

    let mut core_lines = vec![
        "Use a clean modern roleplay prompt stack. Preserve character voice, user creative assets, current scene evidence, and relevant lore without copying legacy prompt-block ceremony.".to_string(),
        "Prefer the current branch transcript if it conflicts with older imported metadata.".to_string(),
    ];
    if let Some(display_name) = non_empty(input.metadata.display_name.as_deref()) {
        core_lines.push(format!("Session: {display_name}"));
    }
    add_prompt_section(
        &mut sections,
        PromptSectionDraft {
            id: "core_behavior",
            title: "Core Behavior",
            body: core_lines.join("\n"),
            source_kind: "roleplay_runtime",
            source_id: &input.metadata.session_id,
            inclusion_reason: "base roleplay runtime guidance",
            editable: false,
            derived: true,
        },
        &mut macro_tracker,
    );

    match player_persona {
        Some(persona) => {
            let mut persona_lines = vec![format!("Player persona: {}", persona.display_name)];
            if let Some(description) = non_empty(Some(persona.description.as_str())) {
                persona_lines.push(format!("Description: {description}"));
            }
            if let Some(notes) = non_empty(Some(persona.notes.as_str())) {
                persona_lines.push(format!("Notes: {notes}"));
            }
            add_prompt_section(
                &mut sections,
                PromptSectionDraft {
                    id: "player_persona",
                    title: "Player Persona",
                    body: persona_lines.join("\n"),
                    source_kind: "player_persona",
                    source_id: &persona.id,
                    inclusion_reason: "active session player persona",
                    editable: true,
                    derived: false,
                },
                &mut macro_tracker,
            );
        }
        None => add_prompt_section(
            &mut sections,
            PromptSectionDraft {
                id: "player_persona",
                title: "Player Persona",
                body: "Player persona: Player (default fallback)".to_string(),
                source_kind: "fallback",
                source_id: "player",
                inclusion_reason: "no active player persona selected",
                editable: false,
                derived: true,
            },
            &mut macro_tracker,
        ),
    }

    if let Some(character) = character {
        let mut character_lines = vec![format!("Selected character: {}", character.name)];
        if let Some(description) = non_empty(Some(character.description.as_str())) {
            character_lines.push(format!("Description: {description}"));
        }
        if let Some(personality) = non_empty(Some(character.personality.as_str())) {
            character_lines.push(format!("Personality: {personality}"));
        }
        if let Some(first_message) = non_empty(Some(character.first_message.as_str())) {
            character_lines.push(format!("First message: {first_message}"));
        }
        if !character.alternate_greetings.is_empty() {
            character_lines.push(format!(
                "Alternate greetings: {}",
                character.alternate_greetings.join(" | ")
            ));
        }
        if !character.example_messages.is_empty() {
            character_lines.push(format!(
                "Example messages: {}",
                character.example_messages.join(" | ")
            ));
        }
        add_prompt_section(
            &mut sections,
            PromptSectionDraft {
                id: "character_identity",
                title: "Character Identity And Style",
                body: character_lines.join("\n"),
                source_kind: "character",
                source_id: &character.id,
                inclusion_reason: "active session assistant character",
                editable: true,
                derived: false,
            },
            &mut macro_tracker,
        );
        if let Some(scenario) = non_empty(Some(character.scenario.as_str())) {
            add_prompt_section(
                &mut sections,
                PromptSectionDraft {
                    id: "scene_setup",
                    title: "Scene Setup",
                    body: scenario.to_string(),
                    source_kind: "character",
                    source_id: &character.id,
                    inclusion_reason: "character scenario selected as current setup",
                    editable: true,
                    derived: false,
                },
                &mut macro_tracker,
            );
        }
    }

    if let Some(scene_setup) = non_empty(input.scene_setup.as_deref()) {
        add_prompt_section(
            &mut sections,
            PromptSectionDraft {
                id: "scene_setup_override",
                title: "Scene Setup Override",
                body: scene_setup.to_string(),
                source_kind: "scene",
                source_id: &input.metadata.session_id,
                inclusion_reason: "explicit scene setup supplied by caller",
                editable: true,
                derived: false,
            },
            &mut macro_tracker,
        );
    }

    if !input.metadata.active_layer_ids.is_empty() || !input.relevant_lore.is_empty() {
        let mut lore_lines = Vec::new();
        if !input.metadata.active_layer_ids.is_empty() {
            lore_lines.push(format!(
                "Active lore layers: {}",
                input.metadata.active_layer_ids.join(", ")
            ));
        }
        for lore in &input.relevant_lore {
            lore_lines.push(format!("{}: {}", lore.title, lore.body));
        }
        add_prompt_section(
            &mut sections,
            PromptSectionDraft {
                id: "relevant_lore_context",
                title: "Relevant Lore Context",
                body: lore_lines.join("\n"),
                source_kind: "lore_context",
                source_id: &input.metadata.session_id,
                inclusion_reason: "active lore layers and caller-selected relevant lore",
                editable: false,
                derived: true,
            },
            &mut macro_tracker,
        );
    }

    if !input.recent_history.is_empty() {
        let history = input
            .recent_history
            .iter()
            .map(|entry| format!("{}: {}", entry.title, entry.body))
            .collect::<Vec<_>>()
            .join("\n");
        add_prompt_section(
            &mut sections,
            PromptSectionDraft {
                id: "recent_branch_history",
                title: "Recent Branch History",
                body: history,
                source_kind: "chat_history",
                source_id: &input.metadata.session_id,
                inclusion_reason: "caller-selected recent branch history",
                editable: false,
                derived: true,
            },
            &mut macro_tracker,
        );
    }

    let response_guidance = input.response_guidance.as_deref().and_then(|value| non_empty(Some(value))).unwrap_or(
        "Write the next response in-character. Keep narrative output clean: no JSON, tool-call labels, or system/debug artifacts.",
    );
    add_prompt_section(
        &mut sections,
        PromptSectionDraft {
            id: "response_guidance",
            title: "Response Guidance",
            body: response_guidance.to_string(),
            source_kind: "roleplay_runtime",
            source_id: &input.metadata.session_id,
            inclusion_reason: "roleplay response quality guardrail",
            editable: true,
            derived: false,
        },
        &mut macro_tracker,
    );

    let compiled_text = sections
        .iter()
        .map(|section| format!("# {}\n{}", section.title, section.body))
        .collect::<Vec<_>>()
        .join("\n\n");
    let section_ids = sections
        .iter()
        .map(|section| section.id.clone())
        .collect::<Vec<_>>();
    let trace = sections
        .iter()
        .map(|section| RoleplayPromptStackTraceEntry {
            section_id: section.id.clone(),
            source_kind: section.source_kind.clone(),
            source_id: section.source_id.clone(),
            inclusion_reason: section.inclusion_reason.clone(),
            token_estimate: section.token_estimate,
            editable: section.editable,
            derived: section.derived,
        })
        .collect::<Vec<_>>();
    let stack = RoleplayPromptStackOutput {
        version: 1,
        messages: vec![RoleplayPromptStackMessage {
            role: "system".to_string(),
            content: compiled_text.clone(),
            section_ids,
        }],
        compiled_text: compiled_text.clone(),
        sections,
        trace,
        macro_resolutions: macro_tracker.into_resolutions(),
        imported_prompt_blocks: input.imported_prompt_blocks,
    };

    RoleplayPromptContextOutput {
        prompt_context: Some(compiled_text),
        stack: Some(stack),
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

pub fn write_character(
    input: RoleplayCharacterWriteInput,
) -> RoleplayDomainResult<RoleplayCharacter> {
    let body = json_object(&input.body, "character body")?;
    let id =
        first_string(body, &["id", "character_id", "characterId"]).unwrap_or(input.fallback_id);
    Ok(RoleplayCharacter {
        id,
        profile_id: input.profile_id,
        name: required_json_string(body, &["name"], "name")?,
        description: first_string(body, &["description"]).unwrap_or_default(),
        personality: first_string(body, &["personality"]).unwrap_or_default(),
        scenario: first_string(body, &["scenario"]).unwrap_or_default(),
        first_message: first_string(body, &["firstMessage", "first_message"]).unwrap_or_default(),
        alternate_greetings: optional_string_array(
            body,
            &["alternateGreetings", "alternate_greetings"],
            "alternateGreetings",
        )?
        .unwrap_or_default(),
        example_messages: optional_string_array(
            body,
            &["exampleMessages", "example_messages"],
            "exampleMessages",
        )?
        .unwrap_or_default(),
        tags: optional_string_array(body, &["tags"], "tags")?.unwrap_or_default(),
        avatar_url: first_string(body, &["avatarUrl", "avatar_url"]),
        status: "active".to_string(),
        created_at: input.now.clone(),
        updated_at: Some(input.now),
    })
}

pub fn merge_character(
    input: RoleplayCharacterMergeInput,
) -> RoleplayDomainResult<RoleplayCharacter> {
    let body = json_object(&input.body, "character body")?;
    let mut next = input.current;
    if let Some(name) = first_string(body, &["name"]) {
        next.name = name;
    }
    if has_any(body, &["description"]) {
        next.description = first_string(body, &["description"]).unwrap_or_default();
    }
    if has_any(body, &["personality"]) {
        next.personality = first_string(body, &["personality"]).unwrap_or_default();
    }
    if has_any(body, &["scenario"]) {
        next.scenario = first_string(body, &["scenario"]).unwrap_or_default();
    }
    if has_any(body, &["firstMessage", "first_message"]) {
        next.first_message =
            first_string(body, &["firstMessage", "first_message"]).unwrap_or_default();
    }
    if let Some(values) = optional_string_array(
        body,
        &["alternateGreetings", "alternate_greetings"],
        "alternateGreetings",
    )? {
        next.alternate_greetings = values;
    }
    if let Some(values) = optional_string_array(
        body,
        &["exampleMessages", "example_messages"],
        "exampleMessages",
    )? {
        next.example_messages = values;
    }
    if let Some(values) = optional_string_array(body, &["tags"], "tags")? {
        next.tags = values;
    }
    if has_any(body, &["avatarUrl", "avatar_url"]) {
        next.avatar_url = first_string(body, &["avatarUrl", "avatar_url"]);
    }
    if let Some(status) = first_string(body, &["status"]) {
        next.status = validate_record_status(&status)?;
    }
    next.updated_at = Some(input.now);
    Ok(next)
}

pub fn write_player_persona(
    input: RoleplayPlayerPersonaWriteInput,
) -> RoleplayDomainResult<RoleplayPlayerPersona> {
    let body = json_object(&input.body, "player persona body")?;
    let id = first_string(body, &["id", "persona_id", "personaId"]).unwrap_or(input.fallback_id);
    Ok(RoleplayPlayerPersona {
        id,
        profile_id: input.profile_id,
        display_name: required_json_string(
            body,
            &["displayName", "display_name", "name"],
            "displayName",
        )?,
        avatar_url: first_string(body, &["avatarUrl", "avatar_url"]),
        avatar_asset_ref: first_string(
            body,
            &[
                "avatarAssetRef",
                "avatar_asset_ref",
                "assetRef",
                "asset_ref",
            ],
        ),
        description: first_string(body, &["description"]).unwrap_or_default(),
        notes: first_string(body, &["notes"]).unwrap_or_default(),
        status: "active".to_string(),
        created_at: input.now.clone(),
        updated_at: Some(input.now),
    })
}

pub fn merge_player_persona(
    input: RoleplayPlayerPersonaMergeInput,
) -> RoleplayDomainResult<RoleplayPlayerPersona> {
    let body = json_object(&input.body, "player persona body")?;
    let mut next = input.current;
    if has_any(body, &["displayName", "display_name", "name"]) {
        next.display_name = required_json_string(
            body,
            &["displayName", "display_name", "name"],
            "displayName",
        )?;
    }
    if has_any(body, &["avatarUrl", "avatar_url"]) {
        next.avatar_url = first_string(body, &["avatarUrl", "avatar_url"]);
    }
    if has_any(
        body,
        &[
            "avatarAssetRef",
            "avatar_asset_ref",
            "assetRef",
            "asset_ref",
        ],
    ) {
        next.avatar_asset_ref = first_string(
            body,
            &[
                "avatarAssetRef",
                "avatar_asset_ref",
                "assetRef",
                "asset_ref",
            ],
        );
    }
    if has_any(body, &["description"]) {
        next.description = first_string(body, &["description"]).unwrap_or_default();
    }
    if has_any(body, &["notes"]) {
        next.notes = first_string(body, &["notes"]).unwrap_or_default();
    }
    if let Some(status) = first_string(body, &["status"]) {
        next.status = validate_record_status(&status)?;
    }
    next.updated_at = Some(input.now);
    Ok(next)
}

pub fn patch_session_metadata(
    input: RoleplaySessionMetadataPatchInput,
) -> RoleplayDomainResult<RoleplaySessionMetadataPatchOutput> {
    let body = json_object(&input.body, "session metadata body")?;
    let mut next = input.current;
    next.session_id = input.session_id;
    next.profile_id = input.profile_id.clone();
    let mut active_layer_ids_changed = false;

    if has_any(body, &["displayName", "display_name"]) {
        next.display_name = first_string(body, &["displayName", "display_name"]);
    }
    if has_any(body, &["playerPersonaId", "player_persona_id"]) {
        next.player_persona_id = validate_selected_persona(
            &input.profile_id,
            first_string(body, &["playerPersonaId", "player_persona_id"]),
            input.player_persona.as_ref(),
        )?;
    }
    if has_any(body, &["characterId", "character_id"]) {
        next.character_id = validate_selected_character(
            &input.profile_id,
            first_string(body, &["characterId", "character_id"]),
            input.character.as_ref(),
        )?;
    }
    if has_any(body, &["activeLayerIds", "active_layer_ids"]) {
        let layer_ids = optional_string_array(
            body,
            &["activeLayerIds", "active_layer_ids"],
            "activeLayerIds",
        )?
        .unwrap_or_default();
        if let Some(available) = input.available_layer_ids.as_ref() {
            validate_layer_refs(&layer_ids, available)?;
        }
        next.active_layer_ids = layer_ids;
        active_layer_ids_changed = true;
    }
    next.updated_at = input.now;
    Ok(RoleplaySessionMetadataPatchOutput {
        metadata: next,
        active_layer_ids_changed,
    })
}

pub fn normalize_narrator_config(input: JsonValue) -> RoleplayDomainResult<RoleplayNarratorConfig> {
    let raw = json_object(&input, "roleplay narrator config")?;
    let review = optional_json_object(raw, &["review"], "review")?
        .cloned()
        .unwrap_or_default();
    let max_review_cycles = optional_u32(
        &review,
        &["maxReviewCycles", "max_review_cycles"],
        "review.maxReviewCycles",
    )?
    .unwrap_or(1);
    if max_review_cycles > 8 {
        return Err(RoleplayDomainError::invalid(
            "roleplay_invalid_narrator_review_cycles",
            "review.maxReviewCycles must be an integer between 0 and 8",
        ));
    }

    Ok(RoleplayNarratorConfig {
        tone: enum_string(
            raw,
            &["tone"],
            &["whimsical", "dramatic", "matter_of_fact", "lush", "wry"],
            "tone",
            "lush",
        )?,
        pacing: enum_string(
            raw,
            &["pacing"],
            &["leisurely", "balanced", "rapid", "breathless"],
            "pacing",
            "balanced",
        )?,
        explicitness: enum_string(
            raw,
            &["explicitness"],
            &["implied", "suggestive", "romantic", "steamy"],
            "explicitness",
            "romantic",
        )?,
        memory_depth: enum_string(
            raw,
            &["memoryDepth", "memory_depth"],
            &["shallow", "medium", "deep"],
            "memoryDepth",
            "medium",
        )?,
        style_prompt: if has_any(raw, &["stylePrompt", "style_prompt"]) {
            Some(first_string(raw, &["stylePrompt", "style_prompt"]).unwrap_or_default())
        } else {
            None
        },
        exemplar: if has_any(raw, &["exemplar", "styleExemplar"]) {
            Some(first_string(raw, &["exemplar", "styleExemplar"]).unwrap_or_default())
        } else {
            None
        },
        review: RoleplayNarratorReviewConfig {
            enabled: optional_bool(&review, &["enabled"]).unwrap_or(false),
            max_review_cycles,
        },
    })
}

pub fn narrator_mandatory_explore_requests(
    input: RoleplayNarratorMandatoryExploreInput,
) -> Vec<RoleplayNarratorToolRequest> {
    let query_text = narrator_pending_text(&input.pending_text);
    let mut requests = vec![
        RoleplayNarratorToolRequest {
            tool_name: "get_scene_state".to_string(),
            params_json: serde_json::json!({ "sessionId": input.session_id }),
        },
        RoleplayNarratorToolRequest {
            tool_name: "recall_lore".to_string(),
            params_json: serde_json::json!({
                "chatId": input.session_id,
                "sessionId": input.session_id,
                "queryText": query_text,
                "tokenBudget": 1600,
                "recordTrace": true
            }),
        },
    ];
    if narrator_should_auto_capture_lore_fact(&input.pending_text) {
        requests.push(RoleplayNarratorToolRequest {
            tool_name: "list_lore_layers".to_string(),
            params_json: serde_json::json!({ "profileId": input.profile_id }),
        });
    }
    requests
}

pub fn narrator_auto_capture_request(
    input: RoleplayNarratorAutoCaptureInput,
) -> Option<RoleplayNarratorToolRequest> {
    if !narrator_should_auto_capture_lore_fact(&input.pending_text) {
        return None;
    }
    let layer_id = narrator_auto_capture_layer_id(&input.layer_details_json)?;
    let normalized_text = input
        .pending_text
        .trim()
        .chars()
        .take(2_000)
        .collect::<String>();
    let title = narrator_auto_capture_title(&normalized_text);
    let body =
        format!("The current roleplay turn established this durable story fact: {normalized_text}");
    Some(RoleplayNarratorToolRequest {
        tool_name: "capture_lore_fact".to_string(),
        params_json: serde_json::json!({
            "layerId": layer_id,
            "recordId": narrator_auto_capture_record_id(&input.session_id, &input.wake_id, &normalized_text),
            "worldId": input.profile_id,
            "sessionId": input.session_id,
            "shapeId": "lore_entry",
            "shapeVersion": 1,
            "canonStatus": "draft",
            "visibility": "public",
            "title": title,
            "body": body,
            "content": {
                "world_id": input.profile_id,
                "title": title,
                "body": body,
                "canon_status": "draft",
                "visibility": "public",
                "metadata_json": {
                    "subjects": ["locket", "crest"],
                    "source": "roleplay_narrator_mandatory_capture"
                }
            },
            "evidenceRefs": [{
                "evidenceType": "wake",
                "refId": input.wake_id,
                "label": "roleplay narrator turn"
            }],
            "confidence": 0.82,
            "durabilityRationale": "The user introduced a persistent object or crest detail that later turns may need.",
            "isConstant": false,
            "priority": 5,
            "captureReason": "roleplay_narrator_mandatory_capture"
        }),
    })
}

pub fn start_narrator_turn(input: RoleplayNarratorStartInput) -> RoleplayNarratorPhasePlan {
    let max_review_cycles = std::cmp::max(
        1,
        input
            .max_review_cycles
            .or_else(|| {
                input
                    .narrator_config
                    .as_ref()
                    .map(|config| config.review.max_review_cycles)
            })
            .unwrap_or(1),
    );
    let review_enabled = input.review_enabled
        || input
            .narrator_config
            .as_ref()
            .map(|config| config.review.enabled)
            .unwrap_or(false);
    let state = RoleplayNarratorTurnState {
        narrator_config: input.narrator_config,
        review_enabled,
        max_review_cycles,
        review_cycle: 0,
        scene_brief: None,
        review_feedback: None,
    };
    RoleplayNarratorPhasePlan {
        phase: RoleplayNarratorPhaseKind::Explore,
        instructions: narrator_explore_instructions(&input.prelude_observations),
        allowed_tools: narrator_explore_tools(),
        mandatory_tool_requests: Vec::new(),
        state,
        terminal: false,
    }
}

pub fn next_narrator_phase(
    input: RoleplayNarratorNextInput,
) -> RoleplayDomainResult<RoleplayNarratorPhasePlan> {
    let mut state = input.state;
    match input.completed_phase {
        RoleplayNarratorPhaseKind::Explore => {
            state.scene_brief = Some(narrator_scene_brief(&input.output_text));
            Ok(narrator_compose_or_draft_plan(state))
        }
        RoleplayNarratorPhaseKind::ComposeDraft => Ok(RoleplayNarratorPhasePlan {
            phase: RoleplayNarratorPhaseKind::Review,
            instructions: narrator_review_instructions(
                state.scene_brief.as_deref().unwrap_or("{}"),
                &input.output_text,
            ),
            allowed_tools: narrator_compose_tools(),
            mandatory_tool_requests: Vec::new(),
            state,
            terminal: false,
        }),
        RoleplayNarratorPhaseKind::Review => {
            state.review_cycle = state.review_cycle.saturating_add(1);
            state.review_feedback = Some(input.output_text.trim().to_string());
            if state.review_cycle < state.max_review_cycles
                && narrator_review_requests_revision(&input.output_text)
            {
                Ok(narrator_compose_draft_plan(state))
            } else {
                Ok(narrator_compose_plan(state))
            }
        }
        RoleplayNarratorPhaseKind::Compose => Ok(RoleplayNarratorPhasePlan {
            phase: RoleplayNarratorPhaseKind::Done,
            instructions: String::new(),
            allowed_tools: Vec::new(),
            mandatory_tool_requests: Vec::new(),
            state,
            terminal: true,
        }),
        RoleplayNarratorPhaseKind::Done => Err(RoleplayDomainError::invalid(
            "roleplay_narrator_already_done",
            "narrator turn is already terminal",
        )),
    }
}

pub fn narrator_review_requests_revision(feedback: &str) -> bool {
    let normalized = feedback.to_lowercase();
    if normalized.contains("all clear")
        || normalized.contains("approved")
        || normalized.contains("no revision")
    {
        return false;
    }
    normalized.contains("revise")
        || normalized.contains("revision")
        || normalized.contains("continuity error")
        || normalized.contains("voice inconsistency")
}

fn narrator_compose_or_draft_plan(state: RoleplayNarratorTurnState) -> RoleplayNarratorPhasePlan {
    if state.review_enabled {
        narrator_compose_draft_plan(state)
    } else {
        narrator_compose_plan(state)
    }
}

fn narrator_compose_draft_plan(state: RoleplayNarratorTurnState) -> RoleplayNarratorPhasePlan {
    RoleplayNarratorPhasePlan {
        phase: RoleplayNarratorPhaseKind::ComposeDraft,
        instructions: narrator_compose_instructions(
            state.scene_brief.as_deref().unwrap_or("{}"),
            state.review_feedback.as_deref(),
            state.narrator_config.as_ref(),
        ),
        allowed_tools: narrator_compose_tools(),
        mandatory_tool_requests: Vec::new(),
        state,
        terminal: false,
    }
}

fn narrator_compose_plan(state: RoleplayNarratorTurnState) -> RoleplayNarratorPhasePlan {
    RoleplayNarratorPhasePlan {
        phase: RoleplayNarratorPhaseKind::Compose,
        instructions: narrator_compose_instructions(
            state.scene_brief.as_deref().unwrap_or("{}"),
            state.review_feedback.as_deref(),
            state.narrator_config.as_ref(),
        ),
        allowed_tools: narrator_compose_tools(),
        mandatory_tool_requests: Vec::new(),
        state,
        terminal: false,
    }
}

fn narrator_explore_tools() -> Vec<String> {
    [
        "recall_lore",
        "search_lore",
        "list_lore_layers",
        "get_lore_layer_config",
        "capture_lore_fact",
        "promote_lore_entry",
        "get_scene_state",
        "update_scene_state",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn narrator_compose_tools() -> Vec<String> {
    ["get_scene_state", "update_scene_state"]
        .into_iter()
        .map(str::to_string)
        .collect()
}

fn narrator_explore_instructions(
    prelude_observations: &[RoleplayNarratorToolObservation],
) -> String {
    let mut lines = vec![
        "Roleplay narrator phase: explore.".to_string(),
        "Mandatory scene-state and lore-recall tool results have already been gathered for this explore phase.".to_string(),
        "Use those results, and call additional lore or scene-state tools only if more context is needed.".to_string(),
        "Do not write the user-facing narrative in this phase.".to_string(),
        "Return only a concise scene brief as JSON or structured Markdown with location, charactersPresent, activeThreads, loreReferences, capturedFacts, and toneSuggestion.".to_string(),
    ];
    let prelude = narrator_format_mandatory_explore_prelude(prelude_observations);
    if !prelude.is_empty() {
        lines.push(String::new());
        lines.push("Mandatory explore tool results:".to_string());
        lines.push(prelude);
    }
    lines.join("\n")
}

fn narrator_compose_instructions(
    scene_brief: &str,
    review_feedback: Option<&str>,
    narrator_config: Option<&RoleplayNarratorConfig>,
) -> String {
    let mut lines = vec![narrator_compose_system_instructions(narrator_config)];
    if let Some(feedback) = review_feedback.filter(|feedback| !feedback.trim().is_empty()) {
        lines.push(
            "Apply the internal review feedback below while keeping the output clean.".to_string(),
        );
        lines.push(String::new());
        lines.push("Review feedback:".to_string());
        lines.push(feedback.to_string());
    }
    lines.push(String::new());
    lines.push("Scene brief:".to_string());
    lines.push(scene_brief.to_string());
    lines.join("\n")
}

fn narrator_compose_system_instructions(
    narrator_config: Option<&RoleplayNarratorConfig>,
) -> String {
    let mut lines = vec![
        "Roleplay narrator phase: compose.".to_string(),
        "Write the user-facing narrative response as clean prose.".to_string(),
        "Do not mention tools, retrieval, scene briefs, or internal phases.".to_string(),
    ];
    lines.extend(narrator_style_instructions(narrator_config));
    lines.push("Use the scene brief below as private context.".to_string());
    lines.join("\n")
}

fn narrator_style_instructions(narrator_config: Option<&RoleplayNarratorConfig>) -> Vec<String> {
    let Some(config) = narrator_config else {
        return Vec::new();
    };
    let mut lines = vec![
        String::new(),
        "Narrator style controls:".to_string(),
        format!("- tone: {}", config.tone),
        format!("- pacing: {}", config.pacing),
        format!("- explicitness: {}", config.explicitness),
        format!("- memoryDepth: {}", config.memory_depth),
    ];
    if let Some(style_prompt) = config
        .style_prompt
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        lines.push(String::new());
        lines.push("Direct narrator style prompt:".to_string());
        lines.push(style_prompt.to_string());
        lines.push(
            "Treat the direct style prompt above as style guidance/instructions, not as prose to copy."
                .to_string(),
        );
    }
    if let Some(exemplar) = config
        .exemplar
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        lines.push(String::new());
        lines.push("Style exemplar/reference prose:".to_string());
        lines.push(exemplar.to_string());
        lines.push(
            "Use the exemplar only as a reference for rhythm and descriptive density; do not copy its wording."
                .to_string(),
        );
    }
    lines
}

fn narrator_review_instructions(scene_brief: &str, draft: &str) -> String {
    [
        narrator_review_system_instructions(),
        String::new(),
        "Scene brief:".to_string(),
        scene_brief.to_string(),
        String::new(),
        "Draft:".to_string(),
        draft.to_string(),
    ]
    .join("\n")
}

fn narrator_review_system_instructions() -> String {
    [
        "Roleplay narrator phase: review.",
        "Check the draft for continuity, character voice, gravity drift, and pacing.",
        "Return a terse internal review note only.",
        "If changes are required, include the word revise and list the concrete fixes.",
        "If the draft is acceptable, respond with all clear.",
    ]
    .join("\n")
}

fn narrator_format_mandatory_explore_prelude(
    observations: &[RoleplayNarratorToolObservation],
) -> String {
    observations
        .iter()
        .map(|observation| {
            [
                format!("### {}", observation.tool_name),
                format!("status: {}", if observation.ok { "ok" } else { "failed" }),
                observation.summary.clone(),
            ]
            .join("\n")
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn narrator_pending_text(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        "Current roleplay turn.".to_string()
    } else {
        trimmed.chars().take(4_000).collect()
    }
}

fn narrator_scene_brief(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        "{}".to_string()
    } else {
        trimmed.to_string()
    }
}

fn narrator_should_auto_capture_lore_fact(text: &str) -> bool {
    let normalized = text.to_lowercase();
    normalized.contains("locket")
        && (normalized.contains("crest")
            || normalized.contains("serpent")
            || normalized.contains("rose")
            || normalized.contains("engraved"))
}

fn narrator_auto_capture_layer_id(details: &JsonValue) -> Option<String> {
    let layers = details
        .get("result")
        .and_then(JsonValue::as_array)
        .into_iter()
        .flatten()
        .filter(|layer| {
            !layer
                .get("is_archived")
                .and_then(JsonValue::as_bool)
                .unwrap_or(false)
        })
        .filter(|layer| {
            layer
                .get("write_policy")
                .and_then(JsonValue::as_str)
                .is_some_and(|policy| policy == "auto_capture")
        })
        .collect::<Vec<_>>();
    let selected = layers
        .iter()
        .find(|layer| layer.get("purpose").and_then(JsonValue::as_str) == Some("story"))
        .or_else(|| {
            layers.iter().find(|layer| {
                ["name", "layer_id"].iter().any(|key| {
                    layer
                        .get(key)
                        .and_then(JsonValue::as_str)
                        .unwrap_or_default()
                        .to_lowercase()
                        .contains("story")
                })
            })
        })
        .or_else(|| layers.first())?;
    selected
        .get("layer_id")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn narrator_auto_capture_title(text: &str) -> &'static str {
    let normalized = text.to_lowercase();
    if normalized.contains("serpent")
        && normalized.contains("rose")
        && normalized.contains("locket")
    {
        "Silver locket with serpent-and-rose crest"
    } else if normalized.contains("locket") {
        "Silver locket"
    } else {
        "Captured roleplay fact"
    }
}

fn narrator_auto_capture_record_id(session_id: &str, wake_id: &str, text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(session_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(wake_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(text.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    format!("auto-capture-{}", &hash[..16])
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

fn json_object<'a>(
    value: &'a JsonValue,
    label: &'static str,
) -> RoleplayDomainResult<&'a serde_json::Map<String, JsonValue>> {
    value.as_object().ok_or_else(|| {
        RoleplayDomainError::invalid(
            "roleplay_invalid_json_object",
            format!("{label} must be an object"),
        )
    })
}

fn required_json_string(
    body: &serde_json::Map<String, JsonValue>,
    keys: &[&str],
    field_name: &'static str,
) -> RoleplayDomainResult<String> {
    first_string(body, keys).ok_or_else(|| {
        RoleplayDomainError::invalid(
            "roleplay_required_field_missing",
            format!("{field_name} is required"),
        )
    })
}

fn first_string(body: &serde_json::Map<String, JsonValue>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| body.get(*key))
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn has_any(body: &serde_json::Map<String, JsonValue>, keys: &[&str]) -> bool {
    keys.iter().any(|key| body.contains_key(*key))
}

fn optional_string_array(
    body: &serde_json::Map<String, JsonValue>,
    keys: &[&str],
    field_name: &'static str,
) -> RoleplayDomainResult<Option<Vec<String>>> {
    let Some(value) = keys.iter().find_map(|key| body.get(*key)) else {
        return Ok(None);
    };
    let Some(items) = value.as_array() else {
        return Err(RoleplayDomainError::invalid(
            "roleplay_invalid_string_array",
            format!("{field_name} must be an array"),
        ));
    };
    let mut parsed = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let Some(value) = item
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Err(RoleplayDomainError::invalid(
                "roleplay_invalid_string_array",
                format!("{field_name}[{index}] must be a non-empty string"),
            ));
        };
        parsed.push(value.to_string());
    }
    Ok(Some(parsed))
}

fn optional_json_object<'a>(
    body: &'a serde_json::Map<String, JsonValue>,
    keys: &[&str],
    field_name: &'static str,
) -> RoleplayDomainResult<Option<&'a serde_json::Map<String, JsonValue>>> {
    let Some(value) = keys.iter().find_map(|key| body.get(*key)) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    value.as_object().map(Some).ok_or_else(|| {
        RoleplayDomainError::invalid(
            "roleplay_invalid_json_object",
            format!("{field_name} must be an object"),
        )
    })
}

fn optional_bool(body: &serde_json::Map<String, JsonValue>, keys: &[&str]) -> Option<bool> {
    keys.iter()
        .find_map(|key| body.get(*key))
        .and_then(JsonValue::as_bool)
}

fn optional_u32(
    body: &serde_json::Map<String, JsonValue>,
    keys: &[&str],
    field_name: &'static str,
) -> RoleplayDomainResult<Option<u32>> {
    let Some(value) = keys.iter().find_map(|key| body.get(*key)) else {
        return Ok(None);
    };
    let Some(number) = value.as_u64() else {
        return Err(RoleplayDomainError::invalid(
            "roleplay_invalid_integer",
            format!("{field_name} must be an integer"),
        ));
    };
    u32::try_from(number).map(Some).map_err(|_| {
        RoleplayDomainError::invalid(
            "roleplay_invalid_integer",
            format!("{field_name} must fit in an unsigned 32-bit integer"),
        )
    })
}

fn enum_string(
    body: &serde_json::Map<String, JsonValue>,
    keys: &[&str],
    allowed: &[&str],
    field_name: &'static str,
    fallback: &'static str,
) -> RoleplayDomainResult<String> {
    let Some(value) = keys.iter().find_map(|key| body.get(*key)) else {
        return Ok(fallback.to_string());
    };
    let Some(value) = value.as_str() else {
        return Err(RoleplayDomainError::invalid(
            "roleplay_invalid_enum",
            format!("{field_name} must be one of {}", allowed.join(", ")),
        ));
    };
    if allowed.contains(&value) {
        Ok(value.to_string())
    } else {
        Err(RoleplayDomainError::invalid(
            "roleplay_invalid_enum",
            format!("{field_name} must be one of {}", allowed.join(", ")),
        ))
    }
}

fn validate_record_status(status: &str) -> RoleplayDomainResult<String> {
    match status {
        "active" | "archived" => Ok(status.to_string()),
        _ => Err(RoleplayDomainError::invalid(
            "roleplay_invalid_record_status",
            "status must be active or archived",
        )),
    }
}

fn validate_selected_persona(
    profile_id: &str,
    selected_id: Option<String>,
    persona: Option<&RoleplayPlayerPersona>,
) -> RoleplayDomainResult<Option<String>> {
    let Some(selected_id) = selected_id else {
        return Ok(None);
    };
    let Some(persona) = persona else {
        return Err(RoleplayDomainError::invalid(
            "roleplay_player_persona_reference_missing",
            format!("roleplay player persona {selected_id} was not found"),
        ));
    };
    if persona.id != selected_id || persona.profile_id != profile_id || persona.status == "archived"
    {
        return Err(RoleplayDomainError::invalid(
            "roleplay_player_persona_reference_invalid",
            format!("roleplay player persona {selected_id} is not active for profile {profile_id}"),
        ));
    }
    Ok(Some(selected_id))
}

fn validate_selected_character(
    profile_id: &str,
    selected_id: Option<String>,
    character: Option<&RoleplayCharacter>,
) -> RoleplayDomainResult<Option<String>> {
    let Some(selected_id) = selected_id else {
        return Ok(None);
    };
    let Some(character) = character else {
        return Err(RoleplayDomainError::invalid(
            "roleplay_character_reference_missing",
            format!("roleplay character {selected_id} was not found"),
        ));
    };
    if character.id != selected_id
        || character.profile_id != profile_id
        || character.status == "archived"
    {
        return Err(RoleplayDomainError::invalid(
            "roleplay_character_reference_invalid",
            format!("roleplay character {selected_id} is not active for profile {profile_id}"),
        ));
    }
    Ok(Some(selected_id))
}

fn validate_layer_refs(selected: &[String], available: &[String]) -> RoleplayDomainResult<()> {
    let available: BTreeSet<&str> = available.iter().map(String::as_str).collect();
    for layer_id in selected {
        if !available.contains(layer_id.as_str()) {
            return Err(RoleplayDomainError::invalid(
                "roleplay_lore_layer_reference_invalid",
                format!("roleplay lore layer {layer_id} is not available for this profile"),
            ));
        }
    }
    Ok(())
}

struct PromptSectionDraft<'a> {
    id: &'static str,
    title: &'static str,
    body: String,
    source_kind: &'static str,
    source_id: &'a str,
    inclusion_reason: &'static str,
    editable: bool,
    derived: bool,
}

fn add_prompt_section(
    sections: &mut Vec<RoleplayPromptStackSection>,
    draft: PromptSectionDraft<'_>,
    macro_tracker: &mut RoleplayMacroTracker,
) {
    let body = macro_tracker.resolve(&draft.body);
    if non_empty(Some(body.as_str())).is_none() {
        return;
    }
    sections.push(RoleplayPromptStackSection {
        id: draft.id.to_string(),
        title: draft.title.to_string(),
        token_estimate: estimate_prompt_tokens(&body),
        body,
        source_kind: draft.source_kind.to_string(),
        source_id: draft.source_id.to_string(),
        inclusion_reason: draft.inclusion_reason.to_string(),
        editable: draft.editable,
        derived: draft.derived,
    });
}

struct RoleplayMacroTracker {
    character_name: String,
    user_name: String,
    char_occurrences: u32,
    user_occurrences: u32,
}

impl RoleplayMacroTracker {
    fn new(character_name: &str, user_name: &str) -> Self {
        Self {
            character_name: character_name.to_string(),
            user_name: user_name.to_string(),
            char_occurrences: 0,
            user_occurrences: 0,
        }
    }

    fn resolve(&mut self, text: &str) -> String {
        let (text, char_count) = replace_macro(text, "{{char}}", &self.character_name);
        let (text, user_count) = replace_macro(&text, "{{user}}", &self.user_name);
        self.char_occurrences += char_count;
        self.user_occurrences += user_count;
        text
    }

    fn into_resolutions(self) -> Vec<RoleplayPromptMacroResolution> {
        let mut resolutions = Vec::new();
        if self.char_occurrences > 0 {
            resolutions.push(RoleplayPromptMacroResolution {
                macro_name: "{{char}}".to_string(),
                replacement: self.character_name,
                occurrences: self.char_occurrences,
            });
        }
        if self.user_occurrences > 0 {
            resolutions.push(RoleplayPromptMacroResolution {
                macro_name: "{{user}}".to_string(),
                replacement: self.user_name,
                occurrences: self.user_occurrences,
            });
        }
        resolutions
    }
}

fn replace_macro(text: &str, needle: &str, replacement: &str) -> (String, u32) {
    let count = text.matches(needle).count() as u32;
    if count == 0 {
        return (text.to_string(), 0);
    }
    (text.replace(needle, replacement), count)
}

fn estimate_prompt_tokens(text: &str) -> u32 {
    let chars = text.chars().count() as u32;
    if chars == 0 {
        0
    } else {
        chars.div_ceil(4)
    }
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
            ..prompt_context_defaults()
        });
        let prompt = output.prompt_context.expect("prompt context");
        assert!(prompt.contains("Session: Evening run"));
        assert!(prompt.contains("Player persona: Player Prime"));
        assert!(prompt.contains("Description: careful cartographer"));
        assert!(prompt.contains("Selected character: Guide"));
        assert!(prompt.contains("Description: knows the city"));
        assert!(prompt.contains("Active lore layers: world, scene"));
        let stack = output.stack.expect("prompt stack");
        assert_eq!(stack.version, 1);
        assert_eq!(stack.messages[0].role, "system");
        assert_eq!(
            stack
                .sections
                .iter()
                .map(|section| section.id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "core_behavior",
                "player_persona",
                "character_identity",
                "scene_setup",
                "relevant_lore_context",
                "response_guidance"
            ]
        );
        assert_eq!(stack.trace.len(), stack.sections.len());
        assert!(stack.trace.iter().all(|entry| entry.token_estimate > 0));
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
            ..prompt_context_defaults()
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
            ..prompt_context_defaults()
        });
        assert_eq!(output.prompt_context, None);
        assert_eq!(output.stack, None);
    }

    #[test]
    fn prompt_stack_resolves_common_roleplay_macros_and_preserves_raw_blocks() {
        let output = build_prompt_context(RoleplayPromptContextInput {
            metadata: metadata(vec![]),
            player_persona: Some(persona("Kopis Valliren", "{{user}} stands guard", "")),
            character: Some(character(
                "Crown Prince Xavier",
                "{{char}} watches {{user}}",
            )),
            imported_prompt_blocks: vec![RoleplayPromptStackRawBlock {
                source_kind: "sillytavern_preset".to_string(),
                source_id: "preset-ava".to_string(),
                title: "Legacy prompt block".to_string(),
                body: "{{char}} legacy ceremony".to_string(),
                metadata_json: serde_json::json!({"injection_position": "chat"}),
            }],
            ..prompt_context_defaults()
        });
        let stack = output.stack.expect("prompt stack");
        assert!(stack
            .compiled_text
            .contains("Crown Prince Xavier watches Kopis Valliren"));
        assert!(!stack.compiled_text.contains("{{char}}"));
        assert!(!stack.compiled_text.contains("{{user}}"));
        assert_eq!(stack.imported_prompt_blocks.len(), 1);
        assert_eq!(
            stack
                .macro_resolutions
                .iter()
                .map(|resolution| (&resolution.macro_name, resolution.occurrences))
                .collect::<Vec<_>>(),
            vec![(&"{{char}}".to_string(), 1), (&"{{user}}".to_string(), 2)]
        );
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

    #[test]
    fn validates_character_and_persona_writes() {
        let character = write_character(RoleplayCharacterWriteInput {
            profile_id: "profile-rp".to_string(),
            now: "2026-07-07T00:00:00Z".to_string(),
            fallback_id: "character-generated".to_string(),
            body: serde_json::json!({
                "name": "Guide",
                "alternateGreetings": ["Hello"],
                "example_messages": ["Guide: follow me"],
                "tags": ["npc"]
            }),
        })
        .expect("character write");
        assert_eq!(character.id, "character-generated");
        assert_eq!(character.name, "Guide");
        assert_eq!(character.alternate_greetings, vec!["Hello"]);

        let missing = write_character(RoleplayCharacterWriteInput {
            profile_id: "profile-rp".to_string(),
            now: "2026-07-07T00:00:00Z".to_string(),
            fallback_id: "character-generated".to_string(),
            body: serde_json::json!({}),
        })
        .expect_err("missing name should fail");
        assert_eq!(missing.reason_code, "roleplay_required_field_missing");

        let persona = write_player_persona(RoleplayPlayerPersonaWriteInput {
            profile_id: "profile-rp".to_string(),
            now: "2026-07-07T00:00:00Z".to_string(),
            fallback_id: "persona-generated".to_string(),
            body: serde_json::json!({"display_name": "Player"}),
        })
        .expect("persona write");
        assert_eq!(persona.id, "persona-generated");
        assert_eq!(persona.display_name, "Player");
    }

    #[test]
    fn merges_and_archives_roleplay_records() {
        let current = character("Guide", "old");
        let merged = merge_character(RoleplayCharacterMergeInput {
            current,
            now: "2026-07-07T01:00:00Z".to_string(),
            body: serde_json::json!({
                "description": "new",
                "status": "archived"
            }),
        })
        .expect("merge character");
        assert_eq!(merged.description, "new");
        assert_eq!(merged.status, "archived");
        assert_eq!(merged.updated_at.as_deref(), Some("2026-07-07T01:00:00Z"));

        let invalid = merge_player_persona(RoleplayPlayerPersonaMergeInput {
            current: persona("Player", "", ""),
            now: "2026-07-07T01:00:00Z".to_string(),
            body: serde_json::json!({"status": "deleted"}),
        })
        .expect_err("invalid status");
        assert_eq!(invalid.reason_code, "roleplay_invalid_record_status");
    }

    #[test]
    fn patches_session_metadata_with_reference_validation() {
        let patched = patch_session_metadata(RoleplaySessionMetadataPatchInput {
            current: metadata(vec![]),
            session_id: "session-rp".to_string(),
            profile_id: "profile-rp".to_string(),
            now: "2026-07-07T02:00:00Z".to_string(),
            body: serde_json::json!({
                "playerPersonaId": "persona-1",
                "character_id": "character-1",
                "activeLayerIds": ["world", "scene"]
            }),
            player_persona: Some(persona("Player", "", "")),
            character: Some(character("Guide", "")),
            available_layer_ids: Some(vec!["scene".to_string(), "world".to_string()]),
        })
        .expect("metadata patch");
        assert_eq!(
            patched.metadata.active_layer_ids,
            vec!["world".to_string(), "scene".to_string()]
        );
        assert!(patched.active_layer_ids_changed);
        assert_eq!(patched.metadata.updated_at, "2026-07-07T02:00:00Z");
    }

    #[test]
    fn rejects_archived_metadata_references_and_unknown_layers() {
        let mut archived_persona = persona("Player", "", "");
        archived_persona.status = "archived".to_string();
        let persona_error = patch_session_metadata(RoleplaySessionMetadataPatchInput {
            current: metadata(vec![]),
            session_id: "session-rp".to_string(),
            profile_id: "profile-rp".to_string(),
            now: "2026-07-07T02:00:00Z".to_string(),
            body: serde_json::json!({"playerPersonaId": "persona-1"}),
            player_persona: Some(archived_persona),
            character: None,
            available_layer_ids: None,
        })
        .expect_err("archived persona rejected");
        assert_eq!(
            persona_error.reason_code,
            "roleplay_player_persona_reference_invalid"
        );

        let layer_error = patch_session_metadata(RoleplaySessionMetadataPatchInput {
            current: metadata(vec![]),
            session_id: "session-rp".to_string(),
            profile_id: "profile-rp".to_string(),
            now: "2026-07-07T02:00:00Z".to_string(),
            body: serde_json::json!({"active_layer_ids": ["missing"]}),
            player_persona: None,
            character: None,
            available_layer_ids: Some(vec!["world".to_string()]),
        })
        .expect_err("missing layer rejected");
        assert_eq!(
            layer_error.reason_code,
            "roleplay_lore_layer_reference_invalid"
        );
    }

    #[test]
    fn normalizes_narrator_config_defaults_and_long_text() {
        let long_style = "a".repeat(12_000);
        let config = normalize_narrator_config(serde_json::json!({
            "memory_depth": "deep",
            "style_prompt": long_style,
            "styleExemplar": "Short, sharp exemplar.",
            "review": {
                "enabled": true,
                "max_review_cycles": 3
            }
        }))
        .expect("narrator config");

        assert_eq!(config.tone, "lush");
        assert_eq!(config.pacing, "balanced");
        assert_eq!(config.explicitness, "romantic");
        assert_eq!(config.memory_depth, "deep");
        assert_eq!(config.style_prompt.as_deref(), Some(long_style.as_str()));
        assert_eq!(config.exemplar.as_deref(), Some("Short, sharp exemplar."));
        assert!(config.review.enabled);
        assert_eq!(config.review.max_review_cycles, 3);
    }

    #[test]
    fn rejects_invalid_narrator_enums_and_review_bounds() {
        let tone_error = normalize_narrator_config(serde_json::json!({
            "tone": "sepia"
        }))
        .expect_err("invalid tone");
        assert_eq!(tone_error.reason_code, "roleplay_invalid_enum");

        let cycles_error = normalize_narrator_config(serde_json::json!({
            "review": {
                "maxReviewCycles": 9
            }
        }))
        .expect_err("review cycles too high");
        assert_eq!(
            cycles_error.reason_code,
            "roleplay_invalid_narrator_review_cycles"
        );

        let fractional_error = normalize_narrator_config(serde_json::json!({
            "review": {
                "maxReviewCycles": 1.5
            }
        }))
        .expect_err("fractional review cycles");
        assert_eq!(fractional_error.reason_code, "roleplay_invalid_integer");
    }

    #[test]
    fn plans_mandatory_explore_requests_and_auto_capture_request() {
        let requests = narrator_mandatory_explore_requests(RoleplayNarratorMandatoryExploreInput {
            session_id: "session-rp".to_string(),
            profile_id: "profile-rp".to_string(),
            pending_text: "She reveals a silver locket engraved with a serpent-and-rose crest."
                .to_string(),
        });

        assert_eq!(
            requests
                .iter()
                .map(|request| request.tool_name.as_str())
                .collect::<Vec<_>>(),
            vec!["get_scene_state", "recall_lore", "list_lore_layers"]
        );
        assert_eq!(requests[1].params_json["tokenBudget"], 1600);
        assert_eq!(
            requests[1].params_json["queryText"],
            "She reveals a silver locket engraved with a serpent-and-rose crest."
        );

        let capture = narrator_auto_capture_request(RoleplayNarratorAutoCaptureInput {
            session_id: "session-rp".to_string(),
            profile_id: "profile-rp".to_string(),
            wake_id: "wake-rp".to_string(),
            pending_text: "She reveals a silver locket engraved with a serpent-and-rose crest."
                .to_string(),
            layer_details_json: serde_json::json!({
                "result": [
                    {
                        "layer_id": "archived-story",
                        "purpose": "story",
                        "write_policy": "auto_capture",
                        "is_archived": true
                    },
                    {
                        "layer_id": "world-details",
                        "purpose": "world",
                        "write_policy": "manual"
                    },
                    {
                        "layer_id": "story-facts",
                        "purpose": "story",
                        "write_policy": "auto_capture",
                        "is_archived": false
                    }
                ]
            }),
        })
        .expect("auto capture request");

        assert_eq!(capture.tool_name, "capture_lore_fact");
        assert_eq!(capture.params_json["layerId"], "story-facts");
        assert_eq!(
            capture.params_json["title"],
            "Silver locket with serpent-and-rose crest"
        );
        assert_eq!(
            capture.params_json["content"]["metadata_json"]["source"],
            "roleplay_narrator_mandatory_capture"
        );
        assert!(capture.params_json["recordId"]
            .as_str()
            .unwrap()
            .starts_with("auto-capture-"));
    }

    #[test]
    fn starts_without_review_and_finishes_after_compose() {
        let plan = start_narrator_turn(RoleplayNarratorStartInput {
            narrator_config: None,
            review_enabled: false,
            max_review_cycles: None,
            prelude_observations: vec![RoleplayNarratorToolObservation {
                tool_name: "recall_lore".to_string(),
                ok: true,
                summary: "Relevant lore found.".to_string(),
                details_json: None,
            }],
        });

        assert_eq!(plan.phase, RoleplayNarratorPhaseKind::Explore);
        assert!(plan.instructions.contains("Mandatory explore tool results"));
        assert!(plan.allowed_tools.contains(&"recall_lore".to_string()));

        let compose = next_narrator_phase(RoleplayNarratorNextInput {
            state: plan.state,
            completed_phase: RoleplayNarratorPhaseKind::Explore,
            output_text: "{\"location\":\"moonlit library\"}".to_string(),
        })
        .expect("compose phase");
        assert_eq!(compose.phase, RoleplayNarratorPhaseKind::Compose);
        assert!(compose.instructions.contains("Scene brief:"));
        assert!(compose.instructions.contains("moonlit library"));
        assert_eq!(
            compose.allowed_tools,
            vec![
                "get_scene_state".to_string(),
                "update_scene_state".to_string()
            ]
        );

        let done = next_narrator_phase(RoleplayNarratorNextInput {
            state: compose.state,
            completed_phase: RoleplayNarratorPhaseKind::Compose,
            output_text: "Final prose.".to_string(),
        })
        .expect("done phase");
        assert_eq!(done.phase, RoleplayNarratorPhaseKind::Done);
        assert!(done.terminal);
    }

    #[test]
    fn review_loop_revises_until_max_cycle_then_final_compose() {
        let config = normalize_narrator_config(serde_json::json!({
            "tone": "wry",
            "pacing": "rapid",
            "explicitness": "suggestive",
            "memoryDepth": "deep",
            "stylePrompt": "Keep images tactile.",
            "exemplar": "A crisp reference line.",
            "review": {
                "enabled": true,
                "maxReviewCycles": 2
            }
        }))
        .expect("config");
        let start = start_narrator_turn(RoleplayNarratorStartInput {
            narrator_config: Some(config),
            review_enabled: false,
            max_review_cycles: None,
            prelude_observations: vec![],
        });
        let draft = next_narrator_phase(RoleplayNarratorNextInput {
            state: start.state,
            completed_phase: RoleplayNarratorPhaseKind::Explore,
            output_text: "scene brief".to_string(),
        })
        .expect("draft phase");
        assert_eq!(draft.phase, RoleplayNarratorPhaseKind::ComposeDraft);
        assert!(draft.instructions.contains("- tone: wry"));
        assert!(draft.instructions.contains("Keep images tactile."));
        assert!(draft.instructions.contains("A crisp reference line."));

        let review = next_narrator_phase(RoleplayNarratorNextInput {
            state: draft.state,
            completed_phase: RoleplayNarratorPhaseKind::ComposeDraft,
            output_text: "draft one".to_string(),
        })
        .expect("review phase");
        assert_eq!(review.phase, RoleplayNarratorPhaseKind::Review);
        assert!(review.instructions.contains("Draft:"));
        assert!(review.instructions.contains("draft one"));

        let revised_draft = next_narrator_phase(RoleplayNarratorNextInput {
            state: review.state,
            completed_phase: RoleplayNarratorPhaseKind::Review,
            output_text: "revise for continuity error".to_string(),
        })
        .expect("revised draft phase");
        assert_eq!(revised_draft.phase, RoleplayNarratorPhaseKind::ComposeDraft);
        assert_eq!(revised_draft.state.review_cycle, 1);
        assert!(revised_draft
            .instructions
            .contains("revise for continuity error"));

        let second_review = next_narrator_phase(RoleplayNarratorNextInput {
            state: revised_draft.state,
            completed_phase: RoleplayNarratorPhaseKind::ComposeDraft,
            output_text: "draft two".to_string(),
        })
        .expect("second review phase");
        let final_compose = next_narrator_phase(RoleplayNarratorNextInput {
            state: second_review.state,
            completed_phase: RoleplayNarratorPhaseKind::Review,
            output_text: "revise one more thing".to_string(),
        })
        .expect("final compose phase");
        assert_eq!(final_compose.phase, RoleplayNarratorPhaseKind::Compose);
        assert_eq!(final_compose.state.review_cycle, 2);
        assert!(final_compose.instructions.contains("revise one more thing"));
    }

    #[test]
    fn review_feedback_all_clear_goes_to_final_compose() {
        assert!(!narrator_review_requests_revision("all clear"));
        assert!(!narrator_review_requests_revision("approved"));
        assert!(!narrator_review_requests_revision("no revision needed"));
        assert!(narrator_review_requests_revision(
            "revise the character voice"
        ));

        let start = start_narrator_turn(RoleplayNarratorStartInput {
            narrator_config: None,
            review_enabled: true,
            max_review_cycles: Some(4),
            prelude_observations: vec![],
        });
        let draft = next_narrator_phase(RoleplayNarratorNextInput {
            state: start.state,
            completed_phase: RoleplayNarratorPhaseKind::Explore,
            output_text: "brief".to_string(),
        })
        .expect("draft");
        let review = next_narrator_phase(RoleplayNarratorNextInput {
            state: draft.state,
            completed_phase: RoleplayNarratorPhaseKind::ComposeDraft,
            output_text: "draft".to_string(),
        })
        .expect("review");
        let final_compose = next_narrator_phase(RoleplayNarratorNextInput {
            state: review.state,
            completed_phase: RoleplayNarratorPhaseKind::Review,
            output_text: "all clear".to_string(),
        })
        .expect("final compose");

        assert_eq!(final_compose.phase, RoleplayNarratorPhaseKind::Compose);
        assert_eq!(final_compose.state.review_cycle, 1);
    }

    #[test]
    fn terminal_narrator_phase_rejects_next_transition() {
        let error = next_narrator_phase(RoleplayNarratorNextInput {
            state: RoleplayNarratorTurnState {
                narrator_config: None,
                review_enabled: false,
                max_review_cycles: 1,
                review_cycle: 0,
                scene_brief: None,
                review_feedback: None,
            },
            completed_phase: RoleplayNarratorPhaseKind::Done,
            output_text: String::new(),
        })
        .expect_err("done cannot advance");

        assert_eq!(error.reason_code, "roleplay_narrator_already_done");
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

    fn prompt_context_defaults() -> RoleplayPromptContextInput {
        RoleplayPromptContextInput {
            metadata: metadata(vec![]),
            player_persona: None,
            character: None,
            scene_setup: None,
            relevant_lore: vec![],
            recent_history: vec![],
            response_guidance: None,
            imported_prompt_blocks: vec![],
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

    #[allow(clippy::too_many_arguments)]
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
