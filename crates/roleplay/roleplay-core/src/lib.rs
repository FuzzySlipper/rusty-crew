//! Deterministic roleplay domain helpers.
//!
//! This crate owns roleplay invariants that should not live in TypeScript route
//! glue. It intentionally accepts transport/storage-shaped DTOs so callers can
//! keep HTTP and persistence wiring outside the domain module.

use schemars::JsonSchema;
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoleplayPromptContextOutput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_context: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack: Option<RoleplayPromptStackOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct RoleplayPromptStackMessage {
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub section_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct RoleplayPromptMacroResolution {
    pub macro_name: String,
    pub replacement: String,
    pub occurrences: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoleplayChatActor {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoleplayCharacterWriteInput {
    pub profile_id: String,
    pub now: String,
    pub fallback_id: String,
    pub body: JsonValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoleplayCharacterMergeInput {
    pub current: RoleplayCharacter,
    pub now: String,
    pub body: JsonValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoleplayPlayerPersonaWriteInput {
    pub profile_id: String,
    pub now: String,
    pub fallback_id: String,
    pub body: JsonValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoleplayPlayerPersonaMergeInput {
    pub current: RoleplayPlayerPersona,
    pub now: String,
    pub body: JsonValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoleplaySessionMetadataPatchOutput {
    pub metadata: RoleplaySessionMetadata,
    pub active_layer_ids_changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoleplaySessionLifecyclePlanInput {
    pub action: String,
    pub now: String,
    #[serde(default)]
    pub body: JsonValue,
    #[serde(default)]
    pub fallback_session_id: Option<String>,
    #[serde(default)]
    pub registry_agent_id: Option<String>,
    #[serde(default)]
    pub source_session: Option<RoleplaySessionLifecycleSession>,
    #[serde(default)]
    pub current_metadata: Option<RoleplaySessionMetadata>,
    #[serde(default)]
    pub player_persona: Option<RoleplayPlayerPersona>,
    #[serde(default)]
    pub character: Option<RoleplayCharacter>,
    #[serde(default)]
    pub available_layer_ids: Option<Vec<String>>,
    #[serde(default)]
    pub source_chat_layers: Vec<RoleplayChatLayerBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoleplaySessionLifecycleSession {
    pub session_id: String,
    pub agent_id: String,
    pub profile_id: String,
    pub kind: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoleplayChatLayerBinding {
    pub layer_id: String,
    pub priority: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoleplayChatLayerUpdatePlan {
    pub chat_id: String,
    pub layers: Vec<RoleplayChatLayerBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoleplayRuntimeSessionPlan {
    pub create_session: bool,
    pub archive_session: bool,
    pub ensure_configured_session: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoleplaySessionForkPlan {
    pub source_session_id: String,
    pub source_message_id: String,
    pub target_session_id: String,
    pub branch_id: String,
    pub branch_label: String,
    pub branch_metadata_json: JsonValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoleplaySessionLifecyclePlan {
    pub action: String,
    pub session_id: String,
    pub agent_id: String,
    pub profile_id: String,
    pub kind: String,
    pub metadata: RoleplaySessionMetadata,
    pub runtime: RoleplayRuntimeSessionPlan,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat_layer_update: Option<RoleplayChatLayerUpdatePlan>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fork: Option<RoleplaySessionForkPlan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoleplayChatLayerBindingPlanInput {
    pub now: String,
    #[serde(default)]
    pub body: JsonValue,
    #[serde(default)]
    pub current_metadata: Option<RoleplaySessionMetadata>,
    #[serde(default)]
    pub current_chat_layers: Vec<RoleplayChatLayerBinding>,
    #[serde(default)]
    pub available_layer_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoleplayChatLayersWritePlan {
    pub chat_id: String,
    pub layers: Vec<RoleplayChatLayerBinding>,
    pub now: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleplaySessionActiveLayerPatch {
    pub active_layer_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoleplayChatLayerBindingPlan {
    pub chat_layers_write: RoleplayChatLayersWritePlan,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata_patch: Option<RoleplaySessionActiveLayerPatch>,
    pub active_layer_ids: Vec<String>,
    pub chat_layers_changed: bool,
    pub active_layer_ids_changed: bool,
    pub no_op: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoleplayLoreSearchControlsInput {
    #[serde(default)]
    pub params: JsonValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoleplayLoreSearchPagePlan {
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoleplayLoreSearchControls {
    pub explicit_layer_ids: Vec<String>,
    pub page: RoleplayLoreSearchPagePlan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleplaySceneState {
    #[serde(alias = "session_id")]
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(default, alias = "characters_present")]
    pub characters_present: Vec<String>,
    #[serde(default, alias = "active_threads")]
    pub active_threads: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, alias = "updated_at", skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoleplaySceneStateReadInput {
    pub session_id: String,
    #[serde(default)]
    pub record_value_json: Option<String>,
    #[serde(default)]
    pub record_updated_at: Option<String>,
    #[serde(default)]
    pub revision: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoleplaySceneStateReadOutput {
    pub state: RoleplaySceneState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoleplaySceneStateUpdateInput {
    pub session_id: String,
    #[serde(default)]
    pub current: Option<RoleplaySceneState>,
    pub now: String,
    #[serde(default)]
    pub body: JsonValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoleplaySceneStateUpdatePlan {
    pub state: RoleplaySceneState,
    pub value_json: String,
    pub now: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayNarratorReviewConfig {
    pub enabled: bool,
    pub max_review_cycles: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RoleplayNarratorPhaseKind {
    PreludeExplore,
    PreludeCapture,
    Explore,
    Compose,
    ComposeDraft,
    Review,
    Done,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayNarratorToolRequest {
    pub tool_name: String,
    #[serde(default)]
    pub params_json: JsonValue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayNarratorToolObservation {
    pub tool_name: String,
    pub ok: bool,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details_json: Option<JsonValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayNarratorStartInput {
    pub wake_id: String,
    pub session_id: String,
    pub profile_id: String,
    #[serde(default)]
    pub pending_text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub narrator_config: Option<RoleplayNarratorConfig>,
    pub review_enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_review_cycles: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayNarratorTurnState {
    pub profile_id: String,
    pub session_id: String,
    pub pending_text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub narrator_config: Option<RoleplayNarratorConfig>,
    pub review_enabled: bool,
    pub max_review_cycles: u32,
    pub review_cycle: u32,
    #[serde(default)]
    pub prelude_observations: Vec<RoleplayNarratorToolObservation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relevant_lore: Vec<RoleplayPromptStackSourceText>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene_brief: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_feedback: Option<String>,
    #[serde(default)]
    pub completed_phases: Vec<RoleplayNarratorPhaseKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RoleplayNarratorOutputMode {
    Internal,
    Final,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RoleplayNarratorActivityPhase {
    Exploring,
    Composing,
    Reviewing,
    Idle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayNarratorActivity {
    pub phase: RoleplayNarratorActivityPhase,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum RoleplayNarratorDirective {
    ToolBatch {
        requests: Vec<RoleplayNarratorToolRequest>,
    },
    ProviderPhase {
        phase: RoleplayNarratorPhaseKind,
        instructions: String,
        allowed_tools: Vec<String>,
        output_mode: RoleplayNarratorOutputMode,
    },
    Done,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayNarratorTurnReceipt {
    pub receipt_id: String,
    pub wake_id: String,
    pub session_id: String,
    pub sequence: u32,
    pub phase: RoleplayNarratorPhaseKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity: Option<RoleplayNarratorActivity>,
    pub directive: RoleplayNarratorDirective,
    pub state: RoleplayNarratorTurnState,
    pub terminal: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum RoleplayNarratorPhaseOutcome {
    ToolBatchCompleted {
        observations: Vec<RoleplayNarratorToolObservation>,
    },
    ProviderPhaseCompleted {
        #[serde(default)]
        output_text: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayNarratorAdvanceInput {
    pub receipt: RoleplayNarratorTurnReceipt,
    pub outcome: RoleplayNarratorPhaseOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoleplayAssistantAlternativePlanInput {
    pub session_id: String,
    #[serde(default)]
    pub requested_slot_id: Option<String>,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub body: JsonValue,
    #[serde(default)]
    pub slots: Vec<RoleplayMessageSlot>,
    #[serde(default)]
    pub active_branch_id: Option<String>,
    #[serde(default)]
    pub branches: Vec<RoleplayConversationBranch>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant_write: Option<RoleplayAssistantAlternativeVariantWritePlan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoleplayBranchHeadUpdatePlan {
    pub branch_id: String,
    pub head_message_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoleplayAssistantAlternativeVariantWritePlan {
    pub slot_id: String,
    pub variant_id: String,
    pub message_id: String,
    pub source: String,
    pub ordinal: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_message_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_message_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoleplayAlternativeSlotProjection {
    pub slot_id: String,
    pub active_variant_id: Option<String>,
    pub primary_variant_id: String,
    pub alternate_count: u32,
    pub variant_count: u32,
    pub active_variant: RoleplayMessageVariant,
    pub variants: Vec<RoleplayMessageVariant>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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
    let variant_write = match input.request_id.as_deref() {
        Some(request_id) => Some(plan_assistant_alternative_variant_write(
            &terminal,
            request_id,
            &input.body,
            next_alternate_ordinal(&terminal),
            branch_id_for_variant.clone(),
        )?),
        None => None,
    };
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
        variant_write,
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

pub fn plan_session_lifecycle(
    input: RoleplaySessionLifecyclePlanInput,
) -> RoleplayDomainResult<RoleplaySessionLifecyclePlan> {
    match input.action.as_str() {
        "create" => plan_session_create(input),
        "archive" => plan_session_archive(input),
        "restore" => plan_session_restore(input),
        "fork" => plan_session_fork(input),
        action => Err(RoleplayDomainError::invalid(
            "roleplay_session_lifecycle_action_invalid",
            format!("roleplay session lifecycle action {action} is not supported"),
        )),
    }
}

pub fn plan_chat_layer_binding(
    input: RoleplayChatLayerBindingPlanInput,
) -> RoleplayDomainResult<RoleplayChatLayerBindingPlan> {
    let body = json_object(&input.body, "roleplay chat layer binding body")?;
    let chat_id = required_json_string(
        body,
        &["chat_id", "chatId", "session_id", "sessionId"],
        "chat_id",
    )?;
    validate_roleplay_identifier("roleplay chat_id", &chat_id)?;
    if let Some(metadata) = &input.current_metadata {
        if metadata.session_id != chat_id {
            return Err(RoleplayDomainError::invalid(
                "roleplay_chat_layer_metadata_session_mismatch",
                format!(
                    "roleplay chat layer metadata session {} does not match chat id {}",
                    metadata.session_id, chat_id
                ),
            ));
        }
    }
    let layers = roleplay_chat_layer_bindings_from_body(body)?;
    let layer_ids = layers
        .iter()
        .map(|layer| layer.layer_id.clone())
        .collect::<Vec<_>>();
    validate_unique_roleplay_identifiers("roleplay chat layer_ids", &layer_ids)?;
    if let Some(available_layer_ids) = &input.available_layer_ids {
        validate_layer_refs(&layer_ids, available_layer_ids)?;
    }
    let active_layer_ids = layers
        .iter()
        .filter(|layer| layer.enabled)
        .map(|layer| layer.layer_id.clone())
        .collect::<Vec<_>>();
    let active_layer_ids_changed = input
        .current_metadata
        .as_ref()
        .is_none_or(|metadata| metadata.active_layer_ids != active_layer_ids);
    let chat_layers_changed = input.current_chat_layers != layers;
    let metadata_patch = if active_layer_ids_changed {
        Some(RoleplaySessionActiveLayerPatch {
            active_layer_ids: active_layer_ids.clone(),
        })
    } else {
        None
    };
    Ok(RoleplayChatLayerBindingPlan {
        chat_layers_write: RoleplayChatLayersWritePlan {
            chat_id,
            layers,
            now: input.now,
        },
        metadata_patch,
        active_layer_ids,
        chat_layers_changed,
        active_layer_ids_changed,
        no_op: !chat_layers_changed && !active_layer_ids_changed,
    })
}

pub fn normalize_lore_search_controls(
    input: RoleplayLoreSearchControlsInput,
) -> RoleplayDomainResult<RoleplayLoreSearchControls> {
    let params = json_object(&input.params, "roleplay lore search params")?;
    let explicit_layer_ids = roleplay_lore_search_layer_ids(params)?;
    validate_unique_roleplay_identifiers("roleplay lore search layer_ids", &explicit_layer_ids)?;
    let limit = normalized_lore_search_page_value(params, &["limit"], 50, 1, 200, "limit")?;
    let offset = normalized_lore_search_page_value(params, &["offset"], 0, 0, u32::MAX, "offset")?;
    Ok(RoleplayLoreSearchControls {
        explicit_layer_ids,
        page: RoleplayLoreSearchPagePlan { limit, offset },
    })
}

pub fn read_scene_state(
    input: RoleplaySceneStateReadInput,
) -> RoleplayDomainResult<RoleplaySceneStateReadOutput> {
    validate_roleplay_identifier("roleplay scene state session_id", &input.session_id)?;
    let state = match input.record_value_json.as_deref() {
        Some(value_json) => parse_scene_state_record(
            &input.session_id,
            value_json,
            input.record_updated_at.as_deref(),
        ),
        None => empty_scene_state(&input.session_id),
    };
    Ok(RoleplaySceneStateReadOutput {
        state,
        revision: input.revision,
    })
}

pub fn plan_scene_state_update(
    input: RoleplaySceneStateUpdateInput,
) -> RoleplayDomainResult<RoleplaySceneStateUpdatePlan> {
    validate_roleplay_identifier("roleplay scene state session_id", &input.session_id)?;
    let body = json_object(&input.body, "roleplay scene state update body")?;
    let mut state = input
        .current
        .unwrap_or_else(|| empty_scene_state(&input.session_id));
    state.session_id = input.session_id.clone();
    if has_any(body, &["location"]) {
        state.location = optional_nullable_string(body, &["location"], "location")?;
    }
    if has_any(body, &["charactersPresent", "characters_present"]) {
        state.characters_present = required_normalized_string_list(
            body,
            &["charactersPresent", "characters_present"],
            "charactersPresent",
        )?;
    }
    if has_any(body, &["activeThreads", "active_threads"]) {
        state.active_threads = required_normalized_string_list(
            body,
            &["activeThreads", "active_threads"],
            "activeThreads",
        )?;
    }
    if has_any(body, &["notes"]) {
        state.notes = optional_nullable_string(body, &["notes"], "notes")?;
    }
    state.updated_at = Some(input.now.clone());
    let value_json = serde_json::to_string(&state).map_err(|error| {
        RoleplayDomainError::invalid(
            "roleplay_scene_state_serialize_failed",
            format!("failed to serialize roleplay scene state: {error}"),
        )
    })?;
    Ok(RoleplaySceneStateUpdatePlan {
        state,
        value_json,
        now: input.now,
    })
}

fn plan_session_create(
    input: RoleplaySessionLifecyclePlanInput,
) -> RoleplayDomainResult<RoleplaySessionLifecyclePlan> {
    let body = json_object(&input.body, "roleplay session create body")?;
    let profile_id = required_json_string(body, &["profileId", "profile_id"], "profileId")?;
    let display_name_was_supplied = has_any(body, &["displayName", "display_name"]);
    let agent_id = first_string(body, &["agentId", "agent_id"])
        .or(input.registry_agent_id)
        .unwrap_or_else(|| profile_id.clone());
    let session_id = first_string(body, &["sessionId", "session_id"])
        .or(input.fallback_session_id)
        .ok_or_else(|| {
            RoleplayDomainError::invalid(
                "roleplay_session_id_required",
                "sessionId or fallbackSessionId is required",
            )
        })?;
    let base = RoleplaySessionMetadata {
        session_id: session_id.clone(),
        profile_id: profile_id.clone(),
        display_name: None,
        player_persona_id: None,
        character_id: None,
        active_layer_ids: Vec::new(),
        archived: false,
        created_at: input.now.clone(),
        updated_at: input.now.clone(),
    };
    let mut patched = patch_session_metadata(RoleplaySessionMetadataPatchInput {
        current: base,
        session_id: session_id.clone(),
        profile_id: profile_id.clone(),
        now: input.now.clone(),
        body: input.body,
        player_persona: input.player_persona,
        character: input.character,
        available_layer_ids: input.available_layer_ids,
    })?
    .metadata;
    if !display_name_was_supplied && patched.display_name.is_none() {
        patched.display_name = Some("Roleplay Session".to_string());
    }
    let chat_layer_update = chat_layer_update_from_active_layers(
        &session_id,
        patched.active_layer_ids.as_slice(),
        !patched.active_layer_ids.is_empty(),
    );
    Ok(RoleplaySessionLifecyclePlan {
        action: "create".to_string(),
        session_id,
        agent_id,
        profile_id,
        kind: "full".to_string(),
        metadata: patched,
        runtime: RoleplayRuntimeSessionPlan {
            create_session: true,
            archive_session: false,
            ensure_configured_session: false,
        },
        chat_layer_update,
        fork: None,
    })
}

fn plan_session_archive(
    input: RoleplaySessionLifecyclePlanInput,
) -> RoleplayDomainResult<RoleplaySessionLifecyclePlan> {
    let source = required_lifecycle_source(input.source_session)?;
    let mut metadata = required_lifecycle_metadata(input.current_metadata)?;
    metadata.session_id = source.session_id.clone();
    metadata.profile_id = source.profile_id.clone();
    metadata.archived = true;
    metadata.updated_at = input.now;
    Ok(RoleplaySessionLifecyclePlan {
        action: "archive".to_string(),
        session_id: source.session_id,
        agent_id: source.agent_id,
        profile_id: source.profile_id,
        kind: source.kind,
        metadata,
        runtime: RoleplayRuntimeSessionPlan {
            create_session: false,
            archive_session: source.status != "archived",
            ensure_configured_session: false,
        },
        chat_layer_update: None,
        fork: None,
    })
}

fn plan_session_restore(
    input: RoleplaySessionLifecyclePlanInput,
) -> RoleplayDomainResult<RoleplaySessionLifecyclePlan> {
    let source = required_lifecycle_source(input.source_session)?;
    let mut metadata = required_lifecycle_metadata(input.current_metadata)?;
    metadata.session_id = source.session_id.clone();
    metadata.profile_id = source.profile_id.clone();
    metadata.archived = false;
    metadata.updated_at = input.now;
    Ok(RoleplaySessionLifecyclePlan {
        action: "restore".to_string(),
        session_id: source.session_id,
        agent_id: source.agent_id,
        profile_id: source.profile_id,
        kind: source.kind,
        metadata,
        runtime: RoleplayRuntimeSessionPlan {
            create_session: false,
            archive_session: false,
            ensure_configured_session: source.status == "archived",
        },
        chat_layer_update: None,
        fork: None,
    })
}

fn plan_session_fork(
    input: RoleplaySessionLifecyclePlanInput,
) -> RoleplayDomainResult<RoleplaySessionLifecyclePlan> {
    let source = required_lifecycle_source(input.source_session)?;
    if source.status == "archived" {
        return Err(RoleplayDomainError::invalid(
            "roleplay_session_fork_source_archived",
            format!("roleplay session {} is archived", source.session_id),
        ));
    }
    let body = json_object(&input.body, "roleplay session fork body")?;
    let target_message_id = required_json_string(body, &["messageId", "message_id"], "messageId")?;
    let target_session_id = first_string(
        body,
        &["sessionId", "session_id", "newSessionId", "new_session_id"],
    )
    .or(input.fallback_session_id)
    .ok_or_else(|| {
        RoleplayDomainError::invalid(
            "roleplay_session_id_required",
            "sessionId or fallbackSessionId is required",
        )
    })?;
    if target_session_id == source.session_id {
        return Err(RoleplayDomainError::invalid(
            "roleplay_session_fork_target_conflict",
            "fork target session id must differ from the source session id",
        ));
    }
    let source_metadata = required_lifecycle_metadata(input.current_metadata)?;
    validate_selected_persona(
        &source.profile_id,
        source_metadata.player_persona_id.clone(),
        input.player_persona.as_ref(),
    )?;
    validate_selected_character(
        &source.profile_id,
        source_metadata.character_id.clone(),
        input.character.as_ref(),
    )?;
    if let Some(available_layer_ids) = input.available_layer_ids.as_ref() {
        validate_layer_refs(&source_metadata.active_layer_ids, available_layer_ids)?;
    }
    let display_name = first_string(body, &["displayName", "display_name"]).unwrap_or_else(|| {
        format!(
            "{} fork",
            source_metadata
                .display_name
                .as_deref()
                .unwrap_or(source.session_id.as_str())
        )
    });
    let metadata = RoleplaySessionMetadata {
        session_id: target_session_id.clone(),
        profile_id: source.profile_id.clone(),
        display_name: Some(display_name),
        player_persona_id: source_metadata.player_persona_id,
        character_id: source_metadata.character_id,
        active_layer_ids: source_metadata.active_layer_ids,
        archived: false,
        created_at: input.now.clone(),
        updated_at: input.now.clone(),
    };
    let branch_label = first_string(body, &["label", "branchLabel", "branch_label"])
        .unwrap_or_else(|| "Fork".to_string());
    let branch_id = stable_roleplay_record_id(
        "branch",
        format!("{target_session_id}:fork:{target_message_id}").as_str(),
    );
    let chat_layer_update = if input.source_chat_layers.is_empty() {
        None
    } else {
        Some(RoleplayChatLayerUpdatePlan {
            chat_id: target_session_id.clone(),
            layers: input.source_chat_layers,
        })
    };
    Ok(RoleplaySessionLifecyclePlan {
        action: "fork".to_string(),
        session_id: target_session_id.clone(),
        agent_id: source.agent_id,
        profile_id: source.profile_id,
        kind: source.kind,
        metadata,
        runtime: RoleplayRuntimeSessionPlan {
            create_session: true,
            archive_session: false,
            ensure_configured_session: false,
        },
        chat_layer_update,
        fork: Some(RoleplaySessionForkPlan {
            source_session_id: source.session_id.clone(),
            source_message_id: target_message_id.clone(),
            target_session_id,
            branch_id,
            branch_label,
            branch_metadata_json: serde_json::json!({
                "source": "roleplay_session_fork",
                "source_session_id": source.session_id,
                "source_message_id": target_message_id,
            }),
        }),
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

pub fn start_narrator_turn(
    input: RoleplayNarratorStartInput,
) -> RoleplayDomainResult<RoleplayNarratorTurnReceipt> {
    for (field, value) in [
        ("wakeId", input.wake_id.as_str()),
        ("sessionId", input.session_id.as_str()),
        ("profileId", input.profile_id.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(RoleplayDomainError::invalid(
                "roleplay_narrator_identity_required",
                format!("{field} must not be empty"),
            ));
        }
    }
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
    )
    .min(8);
    let review_enabled = input.review_enabled
        || input
            .narrator_config
            .as_ref()
            .map(|config| config.review.enabled)
            .unwrap_or(false);
    let state = RoleplayNarratorTurnState {
        profile_id: input.profile_id,
        session_id: input.session_id.clone(),
        pending_text: input.pending_text,
        narrator_config: input.narrator_config,
        review_enabled,
        max_review_cycles,
        review_cycle: 0,
        prelude_observations: Vec::new(),
        relevant_lore: Vec::new(),
        scene_brief: None,
        review_feedback: None,
        completed_phases: Vec::new(),
    };
    narrator_receipt(
        input.wake_id,
        input.session_id,
        1,
        RoleplayNarratorPhaseKind::PreludeExplore,
        Some(narrator_activity(
            RoleplayNarratorActivityPhase::Exploring,
            "Gathering lore and scene context.",
        )),
        RoleplayNarratorDirective::ToolBatch {
            requests: narrator_mandatory_explore_requests(&state),
        },
        state,
        false,
    )
}

pub fn advance_narrator_turn(
    input: RoleplayNarratorAdvanceInput,
) -> RoleplayDomainResult<RoleplayNarratorTurnReceipt> {
    let receipt = input.receipt;
    validate_narrator_receipt(&receipt)?;
    if receipt.terminal {
        return Err(RoleplayDomainError::invalid(
            "roleplay_narrator_already_done",
            "narrator turn is already terminal",
        ));
    }

    let mut state = receipt.state;
    state.completed_phases.push(receipt.phase.clone());
    let next_sequence = receipt.sequence.saturating_add(1);
    match (receipt.phase, receipt.directive, input.outcome) {
        (
            RoleplayNarratorPhaseKind::PreludeExplore,
            RoleplayNarratorDirective::ToolBatch { .. },
            RoleplayNarratorPhaseOutcome::ToolBatchCompleted { observations },
        ) => {
            state.prelude_observations.extend(observations);
            if let Some(request) =
                narrator_auto_capture_request(&receipt.wake_id, &receipt.session_id, &state)
            {
                narrator_receipt(
                    receipt.wake_id,
                    receipt.session_id,
                    next_sequence,
                    RoleplayNarratorPhaseKind::PreludeCapture,
                    None,
                    RoleplayNarratorDirective::ToolBatch {
                        requests: vec![request],
                    },
                    state,
                    false,
                )
            } else {
                narrator_explore_receipt(
                    receipt.wake_id,
                    receipt.session_id,
                    next_sequence,
                    state,
                )
            }
        }
        (
            RoleplayNarratorPhaseKind::PreludeCapture,
            RoleplayNarratorDirective::ToolBatch { .. },
            RoleplayNarratorPhaseOutcome::ToolBatchCompleted { observations },
        ) => {
            state.prelude_observations.extend(observations);
            narrator_explore_receipt(
                receipt.wake_id,
                receipt.session_id,
                next_sequence,
                state,
            )
        }
        (
            RoleplayNarratorPhaseKind::Explore,
            RoleplayNarratorDirective::ProviderPhase { .. },
            RoleplayNarratorPhaseOutcome::ProviderPhaseCompleted { output_text },
        ) => {
            state.scene_brief = Some(narrator_scene_brief(&output_text));
            if state.review_enabled {
                narrator_compose_draft_receipt(
                    receipt.wake_id,
                    receipt.session_id,
                    next_sequence,
                    state,
                )
            } else {
                narrator_compose_receipt(
                    receipt.wake_id,
                    receipt.session_id,
                    next_sequence,
                    state,
                )
            }
        }
        (
            RoleplayNarratorPhaseKind::ComposeDraft,
            RoleplayNarratorDirective::ProviderPhase { .. },
            RoleplayNarratorPhaseOutcome::ProviderPhaseCompleted { output_text },
        ) => narrator_review_receipt(
            receipt.wake_id,
            receipt.session_id,
            next_sequence,
            state,
            output_text,
        ),
        (
            RoleplayNarratorPhaseKind::Review,
            RoleplayNarratorDirective::ProviderPhase { .. },
            RoleplayNarratorPhaseOutcome::ProviderPhaseCompleted { output_text },
        ) => {
            state.review_cycle = state.review_cycle.saturating_add(1);
            state.review_feedback = Some(output_text.trim().to_string());
            if state.review_cycle < state.max_review_cycles
                && narrator_review_requests_revision(&output_text)
            {
                narrator_compose_draft_receipt(
                    receipt.wake_id,
                    receipt.session_id,
                    next_sequence,
                    state,
                )
            } else {
                narrator_compose_receipt(
                    receipt.wake_id,
                    receipt.session_id,
                    next_sequence,
                    state,
                )
            }
        }
        (
            RoleplayNarratorPhaseKind::Compose,
            RoleplayNarratorDirective::ProviderPhase { .. },
            RoleplayNarratorPhaseOutcome::ProviderPhaseCompleted { .. },
        ) => narrator_receipt(
            receipt.wake_id,
            receipt.session_id,
            next_sequence,
            RoleplayNarratorPhaseKind::Done,
            Some(narrator_activity(
                RoleplayNarratorActivityPhase::Idle,
                "Narrator turn complete.",
            )),
            RoleplayNarratorDirective::Done,
            state,
            true,
        ),
        (phase, directive, outcome) => Err(RoleplayDomainError::invalid(
            "roleplay_narrator_outcome_mismatch",
            format!(
                "outcome {outcome:?} cannot complete narrator phase {phase:?} with directive {directive:?}"
            ),
        )),
    }
}

fn narrator_mandatory_explore_requests(
    state: &RoleplayNarratorTurnState,
) -> Vec<RoleplayNarratorToolRequest> {
    let query_text = narrator_pending_text(&state.pending_text);
    let mut requests = vec![
        RoleplayNarratorToolRequest {
            tool_name: "get_scene_state".to_string(),
            params_json: serde_json::json!({ "sessionId": state.session_id }),
        },
        RoleplayNarratorToolRequest {
            tool_name: "recall_lore".to_string(),
            params_json: serde_json::json!({
                "chatId": state.session_id,
                "sessionId": state.session_id,
                "queryText": query_text,
                "tokenBudget": 1600,
                "recordTrace": true
            }),
        },
    ];
    if narrator_should_auto_capture_lore_fact(&state.pending_text) {
        requests.push(RoleplayNarratorToolRequest {
            tool_name: "list_lore_layers".to_string(),
            params_json: serde_json::json!({ "profileId": state.profile_id }),
        });
    }
    requests
}

fn narrator_auto_capture_request(
    wake_id: &str,
    session_id: &str,
    state: &RoleplayNarratorTurnState,
) -> Option<RoleplayNarratorToolRequest> {
    if !narrator_should_auto_capture_lore_fact(&state.pending_text) {
        return None;
    }
    let layer_details = state
        .prelude_observations
        .iter()
        .find(|observation| observation.ok && observation.tool_name == "list_lore_layers")?
        .details_json
        .as_ref()?;
    let layer_id = narrator_auto_capture_layer_id(layer_details)?;
    let normalized_text = state
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
            "recordId": narrator_auto_capture_record_id(session_id, wake_id, &normalized_text),
            "worldId": state.profile_id,
            "sessionId": session_id,
            "shapeId": "lore_entry",
            "shapeVersion": 1,
            "canonStatus": "draft",
            "visibility": "public",
            "title": title,
            "body": body,
            "content": {
                "world_id": state.profile_id,
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
                "refId": wake_id,
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

fn narrator_review_requests_revision(feedback: &str) -> bool {
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

fn narrator_explore_receipt(
    wake_id: String,
    session_id: String,
    sequence: u32,
    mut state: RoleplayNarratorTurnState,
) -> RoleplayDomainResult<RoleplayNarratorTurnReceipt> {
    state.relevant_lore = narrator_relevant_lore_from_observations(&state.prelude_observations);
    let instructions = narrator_explore_instructions(&state.prelude_observations);
    narrator_receipt(
        wake_id,
        session_id,
        sequence,
        RoleplayNarratorPhaseKind::Explore,
        None,
        RoleplayNarratorDirective::ProviderPhase {
            phase: RoleplayNarratorPhaseKind::Explore,
            instructions,
            allowed_tools: narrator_explore_tools(),
            output_mode: RoleplayNarratorOutputMode::Internal,
        },
        state,
        false,
    )
}

fn narrator_compose_draft_receipt(
    wake_id: String,
    session_id: String,
    sequence: u32,
    state: RoleplayNarratorTurnState,
) -> RoleplayDomainResult<RoleplayNarratorTurnReceipt> {
    let instructions = narrator_compose_instructions(
        state.scene_brief.as_deref().unwrap_or("{}"),
        state.review_feedback.as_deref(),
        state.narrator_config.as_ref(),
        &state.relevant_lore,
    );
    narrator_receipt(
        wake_id,
        session_id,
        sequence,
        RoleplayNarratorPhaseKind::ComposeDraft,
        Some(narrator_activity(
            RoleplayNarratorActivityPhase::Composing,
            "Writing narrative response.",
        )),
        RoleplayNarratorDirective::ProviderPhase {
            phase: RoleplayNarratorPhaseKind::ComposeDraft,
            instructions,
            allowed_tools: narrator_compose_tools(),
            output_mode: RoleplayNarratorOutputMode::Internal,
        },
        state,
        false,
    )
}

fn narrator_review_receipt(
    wake_id: String,
    session_id: String,
    sequence: u32,
    state: RoleplayNarratorTurnState,
    draft: String,
) -> RoleplayDomainResult<RoleplayNarratorTurnReceipt> {
    let instructions = narrator_review_instructions(
        state.scene_brief.as_deref().unwrap_or("{}"),
        &draft,
        &state.relevant_lore,
    );
    narrator_receipt(
        wake_id,
        session_id,
        sequence,
        RoleplayNarratorPhaseKind::Review,
        Some(narrator_activity(
            RoleplayNarratorActivityPhase::Reviewing,
            "Checking continuity and voice.",
        )),
        RoleplayNarratorDirective::ProviderPhase {
            phase: RoleplayNarratorPhaseKind::Review,
            instructions,
            allowed_tools: narrator_compose_tools(),
            output_mode: RoleplayNarratorOutputMode::Internal,
        },
        state,
        false,
    )
}

fn narrator_compose_receipt(
    wake_id: String,
    session_id: String,
    sequence: u32,
    state: RoleplayNarratorTurnState,
) -> RoleplayDomainResult<RoleplayNarratorTurnReceipt> {
    let instructions = narrator_compose_instructions(
        state.scene_brief.as_deref().unwrap_or("{}"),
        state.review_feedback.as_deref(),
        state.narrator_config.as_ref(),
        &state.relevant_lore,
    );
    narrator_receipt(
        wake_id,
        session_id,
        sequence,
        RoleplayNarratorPhaseKind::Compose,
        Some(narrator_activity(
            RoleplayNarratorActivityPhase::Composing,
            "Writing final narrative response.",
        )),
        RoleplayNarratorDirective::ProviderPhase {
            phase: RoleplayNarratorPhaseKind::Compose,
            instructions,
            allowed_tools: narrator_compose_tools(),
            output_mode: RoleplayNarratorOutputMode::Final,
        },
        state,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
fn narrator_receipt(
    wake_id: String,
    session_id: String,
    sequence: u32,
    phase: RoleplayNarratorPhaseKind,
    activity: Option<RoleplayNarratorActivity>,
    directive: RoleplayNarratorDirective,
    state: RoleplayNarratorTurnState,
    terminal: bool,
) -> RoleplayDomainResult<RoleplayNarratorTurnReceipt> {
    let mut receipt = RoleplayNarratorTurnReceipt {
        receipt_id: String::new(),
        wake_id,
        session_id,
        sequence,
        phase,
        activity,
        directive,
        state,
        terminal,
    };
    receipt.receipt_id = narrator_receipt_id(&receipt)?;
    Ok(receipt)
}

fn validate_narrator_receipt(receipt: &RoleplayNarratorTurnReceipt) -> RoleplayDomainResult<()> {
    if receipt.state.session_id != receipt.session_id
        || receipt.state.completed_phases.len().saturating_add(1) != receipt.sequence as usize
    {
        return Err(RoleplayDomainError::invalid(
            "roleplay_narrator_receipt_invalid",
            "narrator receipt sequence or session does not match its state",
        ));
    }
    let expected = narrator_receipt_id(receipt)?;
    if receipt.receipt_id != expected {
        return Err(RoleplayDomainError::invalid(
            "roleplay_narrator_receipt_invalid",
            "narrator receipt identity does not match its state",
        ));
    }
    Ok(())
}

fn narrator_receipt_id(receipt: &RoleplayNarratorTurnReceipt) -> RoleplayDomainResult<String> {
    let bound_payload = serde_json::to_vec(&(
        &receipt.phase,
        &receipt.activity,
        &receipt.directive,
        &receipt.state,
        receipt.terminal,
    ))
    .map_err(|error| {
        RoleplayDomainError::invalid(
            "roleplay_narrator_receipt_serialization_failed",
            format!("could not serialize narrator receipt payload: {error}"),
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(receipt.wake_id.as_bytes());
    hasher.update([0]);
    hasher.update(receipt.session_id.as_bytes());
    hasher.update([0]);
    hasher.update(receipt.sequence.to_le_bytes());
    hasher.update(bound_payload);
    Ok(format!("narrator-{digest:x}", digest = hasher.finalize()))
}

fn narrator_activity(
    phase: RoleplayNarratorActivityPhase,
    message: &str,
) -> RoleplayNarratorActivity {
    RoleplayNarratorActivity {
        phase,
        message: message.to_string(),
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
    relevant_lore: &[RoleplayPromptStackSourceText],
) -> String {
    let mut lines = vec![narrator_compose_system_instructions(narrator_config)];
    let lore_context = narrator_compact_lore_context(relevant_lore);
    if !lore_context.is_empty() {
        lines.push(String::new());
        lines.push("Relevant lore gathered during explore:".to_string());
        lines.push(lore_context);
    }
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

fn narrator_review_instructions(
    scene_brief: &str,
    draft: &str,
    relevant_lore: &[RoleplayPromptStackSourceText],
) -> String {
    let mut lines = vec![
        narrator_review_system_instructions(),
        String::new(),
        "Scene brief:".to_string(),
        scene_brief.to_string(),
    ];
    let lore_context = narrator_compact_lore_context(relevant_lore);
    if !lore_context.is_empty() {
        lines.push(String::new());
        lines.push("Relevant lore gathered during explore:".to_string());
        lines.push(lore_context);
    }
    lines.extend([String::new(), "Draft:".to_string(), draft.to_string()]);
    lines.join("\n")
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

fn narrator_relevant_lore_from_observations(
    observations: &[RoleplayNarratorToolObservation],
) -> Vec<RoleplayPromptStackSourceText> {
    let mut seen_record_ids = BTreeSet::new();
    let mut lore = Vec::new();
    for observation in observations
        .iter()
        .filter(|observation| observation.ok && observation.details_json.is_some())
    {
        let Some(details) = observation
            .details_json
            .as_ref()
            .and_then(JsonValue::as_object)
        else {
            continue;
        };
        let Some(result) = details.get("result").and_then(JsonValue::as_object) else {
            continue;
        };
        match observation.tool_name.as_str() {
            "recall_lore" => {
                if let Some(entries) = result.get("entries").and_then(JsonValue::as_array) {
                    for entry in entries {
                        let Some(entry_object) = entry.as_object() else {
                            continue;
                        };
                        if let Some(record) =
                            entry_object.get("record").and_then(JsonValue::as_object)
                        {
                            narrator_push_lore_context(
                                &mut lore,
                                &mut seen_record_ids,
                                "roleplay_lore_recall",
                                record,
                            );
                        }
                    }
                }
            }
            "search_lore" => {
                if let Some(records) = result.get("records").and_then(JsonValue::as_array) {
                    for record in records.iter().filter_map(JsonValue::as_object) {
                        narrator_push_lore_context(
                            &mut lore,
                            &mut seen_record_ids,
                            "roleplay_lore_search",
                            record,
                        );
                    }
                }
            }
            _ => {}
        }
        if lore.len() >= 8 {
            break;
        }
    }
    lore.truncate(8);
    lore
}

fn narrator_push_lore_context(
    lore: &mut Vec<RoleplayPromptStackSourceText>,
    seen_record_ids: &mut BTreeSet<String>,
    source_kind: &str,
    record: &serde_json::Map<String, JsonValue>,
) {
    let Some(record_id) = first_string(record, &["record_id", "recordId", "id"]) else {
        return;
    };
    if !seen_record_ids.insert(record_id.clone()) {
        return;
    }
    let title = first_string(record, &["title"]).unwrap_or_else(|| record_id.clone());
    let body = first_string(record, &["body"])
        .or_else(|| {
            record
                .get("content")
                .and_then(JsonValue::as_object)
                .and_then(|content| first_string(content, &["body", "text", "summary"]))
        })
        .unwrap_or_default();
    if body.trim().is_empty() && title == record_id {
        return;
    }
    lore.push(RoleplayPromptStackSourceText {
        source_kind: source_kind.to_string(),
        source_id: record_id,
        title: narrator_compact_text(&title, 160),
        body: narrator_compact_text(&body, 900),
        editable: true,
        derived: false,
    });
}

fn narrator_compact_lore_context(lore: &[RoleplayPromptStackSourceText]) -> String {
    lore.iter()
        .map(|entry| {
            [
                format!("- {} ({})", entry.title, entry.source_id),
                format!("  source: {}", entry.source_kind),
                format!("  {}", entry.body),
            ]
            .join("\n")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn narrator_compact_text(value: &str, max_chars: usize) -> String {
    let trimmed = value.trim();
    let mut compact = trimmed.chars().take(max_chars).collect::<String>();
    if trimmed.chars().count() > max_chars {
        compact.push_str("...");
    }
    compact
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

fn plan_assistant_alternative_variant_write(
    terminal: &RoleplayMessageSlot,
    request_id: &str,
    body: &JsonValue,
    ordinal: u32,
    branch_id: Option<String>,
) -> RoleplayDomainResult<RoleplayAssistantAlternativeVariantWritePlan> {
    validate_roleplay_identifier("roleplay assistant alternative request_id", request_id)?;
    let body = json_object(body, "roleplay assistant alternative body")?;
    let variant_id = first_string(body, &["variantId", "variant_id"]).unwrap_or_else(|| {
        stable_roleplay_record_id(
            "variant",
            format!("{}:{request_id}", terminal.slot_id).as_str(),
        )
    });
    let message_id = first_string(body, &["messageId", "message_id"])
        .unwrap_or_else(|| stable_roleplay_record_id("message", &variant_id));
    validate_roleplay_identifier("roleplay assistant alternative variant_id", &variant_id)?;
    validate_roleplay_identifier("roleplay assistant alternative message_id", &message_id)?;
    for variant in std::iter::once(&terminal.primary).chain(terminal.alternates.iter()) {
        if variant.variant_id == variant_id {
            return Err(RoleplayDomainError::invalid(
                "roleplay_assistant_alternative_variant_conflict",
                format!("assistant alternative variant {variant_id} already exists"),
            ));
        }
        if variant.message.message_id == message_id {
            return Err(RoleplayDomainError::invalid(
                "roleplay_assistant_alternative_message_conflict",
                format!("assistant alternative message {message_id} already exists"),
            ));
        }
    }
    Ok(RoleplayAssistantAlternativeVariantWritePlan {
        slot_id: terminal.slot_id.clone(),
        variant_id,
        message_id,
        source: "alternate".to_string(),
        ordinal,
        branch_id,
        parent_message_id: terminal.primary.message.parent_message_id.clone(),
        previous_message_id: terminal.primary.message.previous_message_id.clone(),
    })
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

fn optional_string_list(
    body: &serde_json::Map<String, JsonValue>,
    keys: &[&str],
    field_name: &'static str,
) -> RoleplayDomainResult<Option<Vec<String>>> {
    let Some(value) = keys.iter().find_map(|key| body.get(*key)) else {
        return Ok(None);
    };
    parse_string_list_value(value, field_name).map(Some)
}

fn parse_string_list_value(
    value: &JsonValue,
    field_name: &'static str,
) -> RoleplayDomainResult<Vec<String>> {
    if let Some(items) = value.as_array() {
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
        return Ok(parsed);
    }
    if let Some(value) = value.as_str() {
        return Ok(value
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_string)
            .collect());
    }
    Err(RoleplayDomainError::invalid(
        "roleplay_invalid_string_array",
        format!("{field_name} must be an array or comma-separated string"),
    ))
}

fn parse_query_string_list_value(
    value: &JsonValue,
    field_name: &'static str,
) -> RoleplayDomainResult<Vec<String>> {
    if let Some(items) = value.as_array() {
        let mut parsed = Vec::new();
        for (index, item) in items.iter().enumerate() {
            let Some(value) = item.as_str() else {
                return Err(RoleplayDomainError::invalid(
                    "roleplay_invalid_string_array",
                    format!("{field_name}[{index}] must be a string"),
                ));
            };
            parsed.extend(
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .map(str::to_string),
            );
        }
        return Ok(parsed);
    }
    parse_string_list_value(value, field_name)
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

fn optional_bool_strict(
    body: &serde_json::Map<String, JsonValue>,
    keys: &[&str],
    field_name: &'static str,
) -> RoleplayDomainResult<Option<bool>> {
    let Some(value) = keys.iter().find_map(|key| body.get(*key)) else {
        return Ok(None);
    };
    value.as_bool().map(Some).ok_or_else(|| {
        RoleplayDomainError::invalid(
            "roleplay_invalid_boolean",
            format!("{field_name} must be a boolean"),
        )
    })
}

fn optional_i64(
    body: &serde_json::Map<String, JsonValue>,
    keys: &[&str],
    field_name: &'static str,
) -> RoleplayDomainResult<Option<i64>> {
    let Some(value) = keys.iter().find_map(|key| body.get(*key)) else {
        return Ok(None);
    };
    value.as_i64().map(Some).ok_or_else(|| {
        RoleplayDomainError::invalid(
            "roleplay_invalid_integer",
            format!("{field_name} must be an integer"),
        )
    })
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

fn validate_roleplay_identifier(label: &'static str, value: &str) -> RoleplayDomainResult<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(RoleplayDomainError::invalid(
            "roleplay_identifier_empty",
            format!("{label} must not be empty"),
        ));
    }
    if trimmed.len() > 256 {
        return Err(RoleplayDomainError::invalid(
            "roleplay_identifier_too_long",
            format!("{label} must be at most 256 characters"),
        ));
    }
    if trimmed.contains('\0') {
        return Err(RoleplayDomainError::invalid(
            "roleplay_identifier_invalid",
            format!("{label} must not contain NUL"),
        ));
    }
    Ok(())
}

fn validate_unique_roleplay_identifiers(
    label: &'static str,
    ids: &[String],
) -> RoleplayDomainResult<()> {
    let mut seen = BTreeSet::new();
    for id in ids {
        validate_roleplay_identifier(label, id)?;
        if !seen.insert(id.as_str()) {
            return Err(RoleplayDomainError::invalid(
                "roleplay_identifier_duplicate",
                format!("{label} contains duplicate id {id}"),
            ));
        }
    }
    Ok(())
}

fn roleplay_chat_layer_bindings_from_body(
    body: &serde_json::Map<String, JsonValue>,
) -> RoleplayDomainResult<Vec<RoleplayChatLayerBinding>> {
    if let Some(raw_layers) = body.get("layers") {
        let Some(items) = raw_layers.as_array() else {
            return Err(RoleplayDomainError::invalid(
                "roleplay_invalid_chat_layers",
                "layers must be an array",
            ));
        };
        return items
            .iter()
            .enumerate()
            .map(|(index, value)| roleplay_chat_layer_binding_from_value(index, value))
            .collect();
    }
    let layer_ids =
        optional_string_list(body, &["layer_ids", "layerIds"], "layer_ids")?.unwrap_or_default();
    Ok(layer_ids
        .into_iter()
        .enumerate()
        .map(|(index, layer_id)| RoleplayChatLayerBinding {
            layer_id,
            priority: i64::try_from(index).unwrap_or(i64::MAX),
            enabled: true,
        })
        .collect())
}

fn roleplay_chat_layer_binding_from_value(
    index: usize,
    value: &JsonValue,
) -> RoleplayDomainResult<RoleplayChatLayerBinding> {
    if let Some(layer_id) = value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Ok(RoleplayChatLayerBinding {
            layer_id: layer_id.to_string(),
            priority: i64::try_from(index).unwrap_or(i64::MAX),
            enabled: true,
        });
    }
    let Some(record) = value.as_object() else {
        return Err(RoleplayDomainError::invalid(
            "roleplay_invalid_chat_layer",
            format!("layers[{index}] must be a string or object"),
        ));
    };
    let layer_id = required_json_string(
        record,
        &["layer_id", "layerId"],
        "roleplay chat layer layer_id",
    )?;
    Ok(RoleplayChatLayerBinding {
        layer_id,
        priority: optional_i64(record, &["priority"], "roleplay chat layer priority")?
            .unwrap_or_else(|| i64::try_from(index).unwrap_or(i64::MAX)),
        enabled: optional_bool_strict(record, &["enabled"], "roleplay chat layer enabled")?
            .unwrap_or(true),
    })
}

fn roleplay_lore_search_layer_ids(
    params: &serde_json::Map<String, JsonValue>,
) -> RoleplayDomainResult<Vec<String>> {
    let mut layer_ids = Vec::new();
    for key in ["layer_id", "layerId", "layer_ids", "layerIds"] {
        if let Some(value) = params.get(key) {
            layer_ids.extend(parse_query_string_list_value(value, "layer_ids")?);
        }
    }
    let mut seen = BTreeSet::new();
    Ok(layer_ids
        .into_iter()
        .filter(|layer_id| seen.insert(layer_id.clone()))
        .collect())
}

fn normalized_lore_search_page_value(
    params: &serde_json::Map<String, JsonValue>,
    keys: &[&str],
    fallback: u32,
    min: u32,
    max: u32,
    field_name: &'static str,
) -> RoleplayDomainResult<u32> {
    let Some(value) = keys.iter().find_map(|key| params.get(*key)) else {
        return Ok(fallback);
    };
    let Some(parsed) = query_param_integer_value(value, field_name)? else {
        return Ok(fallback);
    };
    let clamped = parsed.clamp(i64::from(min), i64::from(max));
    u32::try_from(clamped).map_err(|_| {
        RoleplayDomainError::invalid(
            "roleplay_invalid_integer",
            format!("{field_name} must fit in an unsigned 32-bit integer"),
        )
    })
}

fn query_param_integer_value(
    value: &JsonValue,
    field_name: &'static str,
) -> RoleplayDomainResult<Option<i64>> {
    if value.is_null() {
        return Ok(None);
    }
    if let Some(items) = value.as_array() {
        let Some(first) = items.first() else {
            return Ok(None);
        };
        return query_param_integer_value(first, field_name);
    }
    if let Some(number) = value.as_i64() {
        return Ok(Some(number));
    }
    if let Some(number) = value.as_u64() {
        return i64::try_from(number).map(Some).map_err(|_| {
            RoleplayDomainError::invalid(
                "roleplay_invalid_integer",
                format!("{field_name} must fit in a signed 64-bit integer"),
            )
        });
    }
    if value.as_f64().is_some() {
        return Err(RoleplayDomainError::invalid(
            "roleplay_invalid_integer",
            format!("{field_name} must be an integer"),
        ));
    }
    if let Some(text) = value.as_str() {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        return trimmed.parse::<i64>().map(Some).map_err(|_| {
            RoleplayDomainError::invalid(
                "roleplay_invalid_integer",
                format!("{field_name} must be an integer"),
            )
        });
    }
    Err(RoleplayDomainError::invalid(
        "roleplay_invalid_integer",
        format!("{field_name} must be an integer"),
    ))
}

fn empty_scene_state(session_id: &str) -> RoleplaySceneState {
    RoleplaySceneState {
        session_id: session_id.to_string(),
        location: None,
        characters_present: Vec::new(),
        active_threads: Vec::new(),
        notes: None,
        updated_at: None,
    }
}

fn parse_scene_state_record(
    session_id: &str,
    value_json: &str,
    record_updated_at: Option<&str>,
) -> RoleplaySceneState {
    let Ok(mut state) = serde_json::from_str::<RoleplaySceneState>(value_json) else {
        return empty_scene_state(session_id);
    };
    state.session_id = session_id.to_string();
    state.location = normalize_optional_text(state.location);
    state.characters_present = normalize_string_tags(state.characters_present);
    state.active_threads = normalize_string_tags(state.active_threads);
    state.notes = normalize_optional_text(state.notes);
    state.updated_at = normalize_optional_text(state.updated_at).or_else(|| {
        record_updated_at.and_then(|value| normalize_optional_text(Some(value.to_string())))
    });
    state
}

fn optional_nullable_string(
    body: &serde_json::Map<String, JsonValue>,
    keys: &[&str],
    field_name: &'static str,
) -> RoleplayDomainResult<Option<String>> {
    let Some(value) = keys.iter().find_map(|key| body.get(*key)) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let Some(text) = value.as_str() else {
        return Err(RoleplayDomainError::invalid(
            "roleplay_invalid_string",
            format!("{field_name} must be a string or null"),
        ));
    };
    Ok(normalize_optional_text(Some(text.to_string())))
}

fn required_normalized_string_list(
    body: &serde_json::Map<String, JsonValue>,
    keys: &[&str],
    field_name: &'static str,
) -> RoleplayDomainResult<Vec<String>> {
    let Some(value) = keys.iter().find_map(|key| body.get(*key)) else {
        return Ok(Vec::new());
    };
    let Some(items) = value.as_array() else {
        return Err(RoleplayDomainError::invalid(
            "roleplay_invalid_string_array",
            format!("{field_name} must be an array"),
        ));
    };
    let mut tags = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let Some(text) = item.as_str() else {
            return Err(RoleplayDomainError::invalid(
                "roleplay_invalid_string_array",
                format!("{field_name}[{index}] must be a string"),
            ));
        };
        if let Some(normalized) = normalize_optional_text(Some(text.to_string())) {
            tags.push(normalized);
        }
    }
    Ok(normalize_string_tags(tags))
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn normalize_string_tags(values: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .filter_map(|value| normalize_optional_text(Some(value)))
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

fn required_lifecycle_source(
    source: Option<RoleplaySessionLifecycleSession>,
) -> RoleplayDomainResult<RoleplaySessionLifecycleSession> {
    source.ok_or_else(|| {
        RoleplayDomainError::invalid(
            "roleplay_session_lifecycle_source_required",
            "source session is required for this roleplay session lifecycle action",
        )
    })
}

fn required_lifecycle_metadata(
    metadata: Option<RoleplaySessionMetadata>,
) -> RoleplayDomainResult<RoleplaySessionMetadata> {
    metadata.ok_or_else(|| {
        RoleplayDomainError::invalid(
            "roleplay_session_lifecycle_metadata_required",
            "current roleplay session metadata is required for this lifecycle action",
        )
    })
}

fn chat_layer_update_from_active_layers(
    chat_id: &str,
    active_layer_ids: &[String],
    enabled: bool,
) -> Option<RoleplayChatLayerUpdatePlan> {
    if active_layer_ids.is_empty() {
        return None;
    }
    Some(RoleplayChatLayerUpdatePlan {
        chat_id: chat_id.to_string(),
        layers: active_layer_ids
            .iter()
            .enumerate()
            .map(|(index, layer_id)| RoleplayChatLayerBinding {
                layer_id: layer_id.clone(),
                priority: i64::try_from(index).unwrap_or(i64::MAX),
                enabled,
            })
            .collect(),
    })
}

fn stable_roleplay_record_id(prefix: &str, raw: &str) -> String {
    let sanitized = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | ':' | '-') {
                ch
            } else {
                '_'
            }
        })
        .take(160)
        .collect::<String>();
    format!("{prefix}:{sanitized}")
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
            request_id: None,
            body: serde_json::json!({}),
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
            request_id: None,
            body: serde_json::json!({}),
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
            request_id: None,
            body: serde_json::json!({}),
            slots: vec![user, assistant],
            active_branch_id: Some("branch-main".to_string()),
            branches: vec![branch("branch-main", Some("msg-1"))],
        })
        .expect_err("user branch head should fail");

        assert_eq!(error.reason_code, "roleplay_terminal_slot_not_assistant");
    }

    #[test]
    fn plans_assistant_alternative_variant_write_ids_and_lineage() {
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
            request_id: Some("request-1".to_string()),
            body: serde_json::json!({}),
            slots: vec![assistant],
            active_branch_id: Some("branch-main".to_string()),
            branches: vec![branch("branch-main", Some("msg-2"))],
        })
        .expect("variant write plan");
        let write = plan.variant_write.expect("variant write");
        assert_eq!(write.slot_id, "slot-2");
        assert_eq!(write.variant_id, "variant:slot-2:request-1");
        assert_eq!(write.message_id, "message:variant:slot-2:request-1");
        assert_eq!(write.source, "alternate");
        assert_eq!(write.ordinal, 1);
        assert_eq!(write.branch_id.as_deref(), Some("branch-main"));
        assert_eq!(write.previous_message_id.as_deref(), Some("msg-1"));
    }

    #[test]
    fn plans_assistant_alternative_explicit_variant_ids() {
        let assistant = slot(
            "slot-2",
            "msg-2",
            "assistant",
            Some("msg-1"),
            None,
            "2026-07-07T00:01:00Z",
        );
        let plan = plan_assistant_alternative(RoleplayAssistantAlternativePlanInput {
            session_id: "session-rp".to_string(),
            requested_slot_id: Some("slot-2".to_string()),
            request_id: Some("request-1".to_string()),
            body: serde_json::json!({
                "variantId": "variant-custom",
                "message_id": "message-custom"
            }),
            slots: vec![assistant],
            active_branch_id: None,
            branches: vec![],
        })
        .expect("explicit variant write plan");
        let write = plan.variant_write.expect("variant write");
        assert_eq!(write.variant_id, "variant-custom");
        assert_eq!(write.message_id, "message-custom");
    }

    #[test]
    fn rejects_assistant_alternative_variant_write_conflicts() {
        let mut assistant = slot(
            "slot-2",
            "msg-2",
            "assistant",
            Some("msg-1"),
            None,
            "2026-07-07T00:01:00Z",
        );
        assistant.alternates = vec![variant(
            "slot-2",
            "variant-custom",
            "message-custom",
            "assistant",
            "active",
            1,
            None,
            Some("msg-1"),
        )];
        let duplicate_variant = plan_assistant_alternative(RoleplayAssistantAlternativePlanInput {
            session_id: "session-rp".to_string(),
            requested_slot_id: Some("slot-2".to_string()),
            request_id: Some("request-1".to_string()),
            body: serde_json::json!({
                "variantId": "variant-custom",
                "messageId": "message-new"
            }),
            slots: vec![assistant.clone()],
            active_branch_id: None,
            branches: vec![],
        })
        .expect_err("duplicate variant rejected");
        assert_eq!(
            duplicate_variant.reason_code,
            "roleplay_assistant_alternative_variant_conflict"
        );

        let duplicate_message = plan_assistant_alternative(RoleplayAssistantAlternativePlanInput {
            session_id: "session-rp".to_string(),
            requested_slot_id: Some("slot-2".to_string()),
            request_id: Some("request-1".to_string()),
            body: serde_json::json!({
                "variantId": "variant-new",
                "messageId": "message-custom"
            }),
            slots: vec![assistant],
            active_branch_id: None,
            branches: vec![],
        })
        .expect_err("duplicate message rejected");
        assert_eq!(
            duplicate_message.reason_code,
            "roleplay_assistant_alternative_message_conflict"
        );
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
    fn plans_roleplay_session_create_defaults_and_references() {
        let plan = plan_session_lifecycle(RoleplaySessionLifecyclePlanInput {
            action: "create".to_string(),
            now: "2026-07-07T03:00:00Z".to_string(),
            body: serde_json::json!({
                "profileId": "profile-rp",
                "playerPersonaId": "persona-1",
                "characterId": "character-1",
                "activeLayerIds": ["world"]
            }),
            fallback_session_id: Some("session-created".to_string()),
            registry_agent_id: Some("agent-rp".to_string()),
            source_session: None,
            current_metadata: None,
            player_persona: Some(persona("Player", "", "")),
            character: Some(character("Guide", "")),
            available_layer_ids: Some(vec!["world".to_string()]),
            source_chat_layers: vec![],
        })
        .expect("create plan");

        assert_eq!(plan.session_id, "session-created");
        assert_eq!(plan.agent_id, "agent-rp");
        assert!(plan.runtime.create_session);
        assert_eq!(
            plan.metadata.display_name.as_deref(),
            Some("Roleplay Session")
        );
        assert_eq!(plan.metadata.active_layer_ids, vec!["world".to_string()]);
        assert_eq!(
            plan.chat_layer_update
                .as_ref()
                .expect("chat layer update")
                .layers[0]
                .layer_id,
            "world"
        );
    }

    #[test]
    fn plans_roleplay_session_archive_and_restore_transitions() {
        let archive = plan_session_lifecycle(RoleplaySessionLifecyclePlanInput {
            action: "archive".to_string(),
            now: "2026-07-07T03:00:00Z".to_string(),
            body: serde_json::json!({}),
            fallback_session_id: None,
            registry_agent_id: None,
            source_session: Some(lifecycle_session("active")),
            current_metadata: Some(metadata(vec![])),
            player_persona: None,
            character: None,
            available_layer_ids: None,
            source_chat_layers: vec![],
        })
        .expect("archive plan");
        assert!(archive.metadata.archived);
        assert!(archive.runtime.archive_session);
        assert!(!archive.runtime.ensure_configured_session);

        let mut archived_session = lifecycle_session("archived");
        archived_session.updated_at = "2026-07-07T03:00:00Z".to_string();
        let mut archived_metadata = metadata(vec![]);
        archived_metadata.archived = true;
        let restore = plan_session_lifecycle(RoleplaySessionLifecyclePlanInput {
            action: "restore".to_string(),
            now: "2026-07-07T04:00:00Z".to_string(),
            body: serde_json::json!({}),
            fallback_session_id: None,
            registry_agent_id: None,
            source_session: Some(archived_session),
            current_metadata: Some(archived_metadata),
            player_persona: None,
            character: None,
            available_layer_ids: None,
            source_chat_layers: vec![],
        })
        .expect("restore plan");
        assert!(!restore.metadata.archived);
        assert!(restore.runtime.ensure_configured_session);
        assert!(!restore.runtime.archive_session);
    }

    #[test]
    fn plans_roleplay_session_fork_metadata_branch_and_layers() {
        let plan = plan_session_lifecycle(RoleplaySessionLifecyclePlanInput {
            action: "fork".to_string(),
            now: "2026-07-07T05:00:00Z".to_string(),
            body: serde_json::json!({
                "messageId": "message-1",
                "sessionId": "session-forked",
                "displayName": "Forked Session",
                "branchLabel": "Fork point"
            }),
            fallback_session_id: None,
            registry_agent_id: None,
            source_session: Some(lifecycle_session("active")),
            current_metadata: Some(metadata(vec!["world".to_string()])),
            player_persona: Some(persona("Player", "", "")),
            character: Some(character("Guide", "")),
            available_layer_ids: Some(vec!["world".to_string()]),
            source_chat_layers: vec![RoleplayChatLayerBinding {
                layer_id: "world".to_string(),
                priority: 7,
                enabled: true,
            }],
        })
        .expect("fork plan");

        assert!(plan.runtime.create_session);
        assert_eq!(plan.session_id, "session-forked");
        assert_eq!(plan.metadata.session_id, "session-forked");
        assert_eq!(
            plan.metadata.display_name.as_deref(),
            Some("Forked Session")
        );
        assert!(!plan.metadata.archived);
        let fork = plan.fork.expect("fork plan details");
        assert_eq!(fork.source_session_id, "session-rp");
        assert_eq!(fork.source_message_id, "message-1");
        assert_eq!(fork.branch_id, "branch:session-forked:fork:message-1");
        assert_eq!(fork.branch_label, "Fork point");
        assert_eq!(
            plan.chat_layer_update.expect("layer copy").layers[0].priority,
            7
        );
    }

    #[test]
    fn rejects_invalid_roleplay_session_lifecycle_inputs() {
        let missing_layer = plan_session_lifecycle(RoleplaySessionLifecyclePlanInput {
            action: "fork".to_string(),
            now: "2026-07-07T05:00:00Z".to_string(),
            body: serde_json::json!({"messageId": "message-1"}),
            fallback_session_id: Some("session-forked".to_string()),
            registry_agent_id: None,
            source_session: Some(lifecycle_session("active")),
            current_metadata: Some(metadata(vec!["missing".to_string()])),
            player_persona: Some(persona("Player", "", "")),
            character: Some(character("Guide", "")),
            available_layer_ids: Some(vec!["world".to_string()]),
            source_chat_layers: vec![],
        })
        .expect_err("missing layer rejected");
        assert_eq!(
            missing_layer.reason_code,
            "roleplay_lore_layer_reference_invalid"
        );

        let source_archived = plan_session_lifecycle(RoleplaySessionLifecyclePlanInput {
            action: "fork".to_string(),
            now: "2026-07-07T05:00:00Z".to_string(),
            body: serde_json::json!({"messageId": "message-1"}),
            fallback_session_id: Some("session-forked".to_string()),
            registry_agent_id: None,
            source_session: Some(lifecycle_session("archived")),
            current_metadata: Some(metadata(vec![])),
            player_persona: Some(persona("Player", "", "")),
            character: Some(character("Guide", "")),
            available_layer_ids: None,
            source_chat_layers: vec![],
        })
        .expect_err("archived source rejected");
        assert_eq!(
            source_archived.reason_code,
            "roleplay_session_fork_source_archived"
        );
    }

    #[test]
    fn plans_chat_layer_binding_metadata_and_write_side_effects() {
        let plan = plan_chat_layer_binding(RoleplayChatLayerBindingPlanInput {
            now: "2026-07-07T06:00:00Z".to_string(),
            body: serde_json::json!({
                "chatId": "session-rp",
                "layers": [
                    "world",
                    {"layerId": "scene", "priority": 8, "enabled": false},
                    {"layer_id": "notes"}
                ]
            }),
            current_metadata: Some(metadata(vec!["old".to_string()])),
            current_chat_layers: vec![],
            available_layer_ids: Some(vec![
                "world".to_string(),
                "scene".to_string(),
                "notes".to_string(),
            ]),
        })
        .expect("chat layer binding plan");

        assert_eq!(plan.chat_layers_write.chat_id, "session-rp");
        assert_eq!(plan.chat_layers_write.layers[1].priority, 8);
        assert!(!plan.chat_layers_write.layers[1].enabled);
        assert_eq!(
            plan.active_layer_ids,
            vec!["world".to_string(), "notes".to_string()]
        );
        assert_eq!(
            plan.metadata_patch
                .expect("metadata patch")
                .active_layer_ids,
            vec!["world".to_string(), "notes".to_string()]
        );
        assert!(plan.chat_layers_changed);
        assert!(plan.active_layer_ids_changed);
        assert!(!plan.no_op);
    }

    #[test]
    fn plans_chat_layer_binding_noop_when_write_and_metadata_match() {
        let current_layers = vec![
            RoleplayChatLayerBinding {
                layer_id: "world".to_string(),
                priority: 0,
                enabled: true,
            },
            RoleplayChatLayerBinding {
                layer_id: "scene".to_string(),
                priority: 1,
                enabled: true,
            },
        ];
        let plan = plan_chat_layer_binding(RoleplayChatLayerBindingPlanInput {
            now: "2026-07-07T06:00:00Z".to_string(),
            body: serde_json::json!({
                "session_id": "session-rp",
                "layer_ids": "world, scene"
            }),
            current_metadata: Some(metadata(vec!["world".to_string(), "scene".to_string()])),
            current_chat_layers: current_layers,
            available_layer_ids: Some(vec!["world".to_string(), "scene".to_string()]),
        })
        .expect("chat layer binding no-op plan");

        assert!(plan.metadata_patch.is_none());
        assert!(!plan.chat_layers_changed);
        assert!(!plan.active_layer_ids_changed);
        assert!(plan.no_op);
    }

    #[test]
    fn rejects_invalid_chat_layer_binding_inputs() {
        let duplicate = plan_chat_layer_binding(RoleplayChatLayerBindingPlanInput {
            now: "2026-07-07T06:00:00Z".to_string(),
            body: serde_json::json!({
                "chat_id": "session-rp",
                "layer_ids": ["world", "world"]
            }),
            current_metadata: Some(metadata(vec![])),
            current_chat_layers: vec![],
            available_layer_ids: Some(vec!["world".to_string()]),
        })
        .expect_err("duplicate layer rejected");
        assert_eq!(duplicate.reason_code, "roleplay_identifier_duplicate");

        let missing_layer = plan_chat_layer_binding(RoleplayChatLayerBindingPlanInput {
            now: "2026-07-07T06:00:00Z".to_string(),
            body: serde_json::json!({
                "chat_id": "session-rp",
                "layers": [{"layer_id": "missing"}]
            }),
            current_metadata: Some(metadata(vec![])),
            current_chat_layers: vec![],
            available_layer_ids: Some(vec!["world".to_string()]),
        })
        .expect_err("unknown layer rejected");
        assert_eq!(
            missing_layer.reason_code,
            "roleplay_lore_layer_reference_invalid"
        );

        let bad_enabled = plan_chat_layer_binding(RoleplayChatLayerBindingPlanInput {
            now: "2026-07-07T06:00:00Z".to_string(),
            body: serde_json::json!({
                "chat_id": "session-rp",
                "layers": [{"layer_id": "world", "enabled": "yes"}]
            }),
            current_metadata: Some(metadata(vec![])),
            current_chat_layers: vec![],
            available_layer_ids: Some(vec!["world".to_string()]),
        })
        .expect_err("bad enabled rejected");
        assert_eq!(bad_enabled.reason_code, "roleplay_invalid_boolean");
    }

    #[test]
    fn normalizes_lore_search_controls() {
        let controls = normalize_lore_search_controls(RoleplayLoreSearchControlsInput {
            params: serde_json::json!({
                "layer_id": ["world, scene", "world"],
                "layerIds": "notes",
                "limit": "500",
                "offset": "-8"
            }),
        })
        .expect("search controls");

        assert_eq!(
            controls.explicit_layer_ids,
            vec![
                "world".to_string(),
                "scene".to_string(),
                "notes".to_string()
            ]
        );
        assert_eq!(controls.page.limit, 200);
        assert_eq!(controls.page.offset, 0);
    }

    #[test]
    fn normalizes_lore_search_control_defaults_and_rejects_invalid_values() {
        let defaults = normalize_lore_search_controls(RoleplayLoreSearchControlsInput {
            params: serde_json::json!({}),
        })
        .expect("default search controls");
        assert!(defaults.explicit_layer_ids.is_empty());
        assert_eq!(defaults.page.limit, 50);
        assert_eq!(defaults.page.offset, 0);

        let bad_limit = normalize_lore_search_controls(RoleplayLoreSearchControlsInput {
            params: serde_json::json!({"limit": "many"}),
        })
        .expect_err("bad limit rejected");
        assert_eq!(bad_limit.reason_code, "roleplay_invalid_integer");

        let bad_layer = normalize_lore_search_controls(RoleplayLoreSearchControlsInput {
            params: serde_json::json!({"layer_id": ["valid", "\0"]}),
        })
        .expect_err("bad layer rejected");
        assert_eq!(bad_layer.reason_code, "roleplay_identifier_invalid");
    }

    #[test]
    fn reads_scene_state_defaults_and_normalizes_records() {
        let empty = read_scene_state(RoleplaySceneStateReadInput {
            session_id: "session-rp".to_string(),
            record_value_json: None,
            record_updated_at: None,
            revision: None,
        })
        .expect("empty scene state");
        assert_eq!(empty.state.session_id, "session-rp");
        assert!(empty.state.characters_present.is_empty());
        assert!(empty.state.active_threads.is_empty());

        let parsed = read_scene_state(RoleplaySceneStateReadInput {
            session_id: "session-rp".to_string(),
            record_value_json: Some(
                serde_json::json!({
                    "sessionId": "other",
                    "location": " Garden ",
                    "charactersPresent": [" elara ", "", "elara"],
                    "activeThreads": ["locket"],
                    "notes": "  rain  "
                })
                .to_string(),
            ),
            record_updated_at: Some("2026-07-07T07:00:00Z".to_string()),
            revision: Some(7),
        })
        .expect("parsed scene state");
        assert_eq!(parsed.state.session_id, "session-rp");
        assert_eq!(parsed.state.location.as_deref(), Some("Garden"));
        assert_eq!(parsed.state.characters_present, vec!["elara".to_string()]);
        assert_eq!(
            parsed.state.updated_at.as_deref(),
            Some("2026-07-07T07:00:00Z")
        );
        assert_eq!(parsed.revision, Some(7));

        let corrupt = read_scene_state(RoleplaySceneStateReadInput {
            session_id: "session-rp".to_string(),
            record_value_json: Some("not json".to_string()),
            record_updated_at: Some("2026-07-07T07:00:00Z".to_string()),
            revision: Some(8),
        })
        .expect("corrupt scene state falls back");
        assert!(corrupt.state.location.is_none());
        assert_eq!(corrupt.revision, Some(8));
    }

    #[test]
    fn plans_scene_state_update_merge_and_normalization() {
        let plan = plan_scene_state_update(RoleplaySceneStateUpdateInput {
            session_id: "session-rp".to_string(),
            current: Some(RoleplaySceneState {
                session_id: "session-rp".to_string(),
                location: Some("Garden".to_string()),
                characters_present: vec!["elara".to_string()],
                active_threads: vec!["locket".to_string()],
                notes: Some("rain".to_string()),
                updated_at: Some("old".to_string()),
            }),
            now: "2026-07-07T08:00:00Z".to_string(),
            body: serde_json::json!({
                "location": null,
                "charactersPresent": [" elara ", " katheryn ", "elara"],
                "activeThreads": ["locket", "", "garden promise"],
                "notes": "  warmer after rain  "
            }),
        })
        .expect("scene state update plan");

        assert!(plan.state.location.is_none());
        assert_eq!(
            plan.state.characters_present,
            vec!["elara".to_string(), "katheryn".to_string()]
        );
        assert_eq!(
            plan.state.active_threads,
            vec!["locket".to_string(), "garden promise".to_string()]
        );
        assert_eq!(plan.state.notes.as_deref(), Some("warmer after rain"));
        assert_eq!(
            plan.state.updated_at.as_deref(),
            Some("2026-07-07T08:00:00Z")
        );
        assert!(plan.value_json.contains("charactersPresent"));
    }

    #[test]
    fn rejects_invalid_scene_state_updates() {
        let bad_array = plan_scene_state_update(RoleplaySceneStateUpdateInput {
            session_id: "session-rp".to_string(),
            current: None,
            now: "2026-07-07T08:00:00Z".to_string(),
            body: serde_json::json!({"charactersPresent": "elara"}),
        })
        .expect_err("charactersPresent string rejected");
        assert_eq!(bad_array.reason_code, "roleplay_invalid_string_array");

        let bad_note = plan_scene_state_update(RoleplaySceneStateUpdateInput {
            session_id: "session-rp".to_string(),
            current: None,
            now: "2026-07-07T08:00:00Z".to_string(),
            body: serde_json::json!({"notes": ["nope"]}),
        })
        .expect_err("notes array rejected");
        assert_eq!(bad_note.reason_code, "roleplay_invalid_string");
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
    fn host_plans_mandatory_explore_and_capture_without_ts_decisions() {
        let start = narrator_start(
            "She reveals a silver locket engraved with a serpent-and-rose crest.",
            false,
            None,
        );
        let RoleplayNarratorDirective::ToolBatch { requests } = &start.directive else {
            panic!("start must issue the mandatory explore tool batch");
        };

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

        let capture_receipt = advance_tools(
            start,
            vec![RoleplayNarratorToolObservation {
                tool_name: "list_lore_layers".to_string(),
                ok: true,
                summary: "layers".to_string(),
                details_json: Some(serde_json::json!({
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
                })),
            }],
        );
        assert_eq!(
            capture_receipt.phase,
            RoleplayNarratorPhaseKind::PreludeCapture
        );
        let RoleplayNarratorDirective::ToolBatch { requests } = &capture_receipt.directive else {
            panic!("capture phase must issue a tool batch");
        };
        let capture = &requests[0];

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
    fn host_receipts_restart_cleanly_and_finish_after_final_compose() {
        let start = narrator_start("Continue the moonlit library scene.", false, None);
        assert_eq!(start.phase, RoleplayNarratorPhaseKind::PreludeExplore);
        assert_eq!(
            start.activity.as_ref().map(|activity| &activity.phase),
            Some(&RoleplayNarratorActivityPhase::Exploring)
        );
        let serialized = serde_json::to_string(&start).expect("serialize receipt");
        let restored: RoleplayNarratorTurnReceipt =
            serde_json::from_str(&serialized).expect("restore receipt");
        let explore = advance_tools(
            restored,
            vec![
                RoleplayNarratorToolObservation {
                    tool_name: "recall_lore".to_string(),
                    ok: true,
                    summary: "Relevant lore found.".to_string(),
                    details_json: Some(serde_json::json!({
                        "ok": true,
                        "operation": "recall_lore",
                        "action": "read",
                        "result": {
                            "entries": [{
                                "record": {
                                    "record_id": "moonlit-garden",
                                    "title": "Moonlit Garden",
                                    "body": "Night-blooming orchids glow beside the missing locket's path."
                                },
                                "score": 0.92,
                                "token_estimate": 18
                            }],
                            "entries_considered": 1,
                            "tokens_consumed": 18
                        }
                    })),
                },
                RoleplayNarratorToolObservation {
                    tool_name: "search_lore".to_string(),
                    ok: true,
                    summary: "Agentic search found a scene-specific entry.".to_string(),
                    details_json: Some(serde_json::json!({
                        "ok": true,
                        "operation": "search_lore",
                        "action": "read",
                        "result": {
                            "records": [{
                                "record_id": "silver-locket",
                                "title": "Silver Locket",
                                "body": "The serpent-and-rose crest belongs to the old city wardens."
                            }]
                        }
                    })),
                },
            ],
        );

        assert_eq!(explore.phase, RoleplayNarratorPhaseKind::Explore);
        let RoleplayNarratorDirective::ProviderPhase {
            instructions,
            allowed_tools,
            output_mode,
            ..
        } = &explore.directive
        else {
            panic!("explore must issue a provider directive");
        };
        assert!(instructions.contains("Mandatory explore tool results"));
        assert!(allowed_tools.contains(&"recall_lore".to_string()));
        assert_eq!(*output_mode, RoleplayNarratorOutputMode::Internal);
        assert_eq!(explore.state.relevant_lore.len(), 2);
        assert_eq!(explore.state.relevant_lore[0].source_id, "moonlit-garden");
        assert_eq!(
            explore.state.relevant_lore[0].source_kind,
            "roleplay_lore_recall"
        );
        assert_eq!(explore.state.relevant_lore[1].source_id, "silver-locket");
        assert_eq!(
            explore.state.relevant_lore[1].source_kind,
            "roleplay_lore_search"
        );

        let compose = advance_provider(explore, "{\"location\":\"moonlit library\"}");
        assert_eq!(compose.phase, RoleplayNarratorPhaseKind::Compose);
        let RoleplayNarratorDirective::ProviderPhase {
            instructions,
            allowed_tools,
            output_mode,
            ..
        } = &compose.directive
        else {
            panic!("compose must issue a provider directive");
        };
        assert!(instructions.contains("Scene brief:"));
        assert!(instructions.contains("moonlit library"));
        assert!(instructions.contains("Relevant lore gathered during explore:"));
        assert!(instructions.contains("Moonlit Garden"));
        assert!(instructions.contains("Night-blooming orchids glow"));
        assert!(instructions.contains("Silver Locket"));
        assert!(instructions.contains("old city wardens"));
        assert_eq!(*output_mode, RoleplayNarratorOutputMode::Final);
        assert_eq!(
            allowed_tools.as_slice(),
            vec![
                "get_scene_state".to_string(),
                "update_scene_state".to_string()
            ]
        );

        let done = advance_provider(compose, "Final prose.");
        assert_eq!(done.phase, RoleplayNarratorPhaseKind::Done);
        assert!(done.terminal);
        assert!(matches!(done.directive, RoleplayNarratorDirective::Done));
        assert_eq!(
            done.activity.as_ref().map(|activity| &activity.phase),
            Some(&RoleplayNarratorActivityPhase::Idle)
        );
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
        let start = narrator_start("Continue the scene.", false, Some(config));
        let explore = advance_tools(start, vec![]);
        let draft = advance_provider(explore, "scene brief");
        assert_eq!(draft.phase, RoleplayNarratorPhaseKind::ComposeDraft);
        let draft_instructions = provider_instructions(&draft);
        assert!(draft_instructions.contains("- tone: wry"));
        assert!(draft_instructions.contains("Keep images tactile."));
        assert!(draft_instructions.contains("A crisp reference line."));

        let review = advance_provider(draft, "draft one");
        assert_eq!(review.phase, RoleplayNarratorPhaseKind::Review);
        assert!(provider_instructions(&review).contains("Draft:"));
        assert!(provider_instructions(&review).contains("draft one"));

        let revised_draft = advance_provider(review, "revise for continuity error");
        assert_eq!(revised_draft.phase, RoleplayNarratorPhaseKind::ComposeDraft);
        assert_eq!(revised_draft.state.review_cycle, 1);
        assert!(provider_instructions(&revised_draft).contains("revise for continuity error"));

        let second_review = advance_provider(revised_draft, "draft two");
        let final_compose = advance_provider(second_review, "revise one more thing");
        assert_eq!(final_compose.phase, RoleplayNarratorPhaseKind::Compose);
        assert_eq!(final_compose.state.review_cycle, 2);
        assert!(provider_instructions(&final_compose).contains("revise one more thing"));
    }

    #[test]
    fn review_feedback_all_clear_goes_to_final_compose() {
        assert!(!narrator_review_requests_revision("all clear"));
        assert!(!narrator_review_requests_revision("approved"));
        assert!(!narrator_review_requests_revision("no revision needed"));
        assert!(narrator_review_requests_revision(
            "revise the character voice"
        ));

        let mut start = narrator_start("Continue.", true, None);
        start.state.max_review_cycles = 4;
        start.receipt_id = narrator_receipt_id(&start).expect("receipt id");
        let draft = advance_provider(advance_tools(start, vec![]), "brief");
        let review = advance_provider(draft, "draft");
        let final_compose = advance_provider(review, "all clear");

        assert_eq!(final_compose.phase, RoleplayNarratorPhaseKind::Compose);
        assert_eq!(final_compose.state.review_cycle, 1);
    }

    #[test]
    fn host_rejects_stale_receipts_and_mismatched_outcomes() {
        let start = narrator_start("Continue.", false, None);
        let mismatch = advance_narrator_turn(RoleplayNarratorAdvanceInput {
            receipt: start.clone(),
            outcome: RoleplayNarratorPhaseOutcome::ProviderPhaseCompleted {
                output_text: "not a tool result".to_string(),
            },
        })
        .expect_err("tool phase rejects provider outcome");
        assert_eq!(mismatch.reason_code, "roleplay_narrator_outcome_mismatch");

        let mut visibility_tampered = advance_tools(start.clone(), vec![]);
        if let RoleplayNarratorDirective::ProviderPhase { output_mode, .. } =
            &mut visibility_tampered.directive
        {
            *output_mode = RoleplayNarratorOutputMode::Final;
        }
        let invalid_visibility = advance_narrator_turn(RoleplayNarratorAdvanceInput {
            receipt: visibility_tampered,
            outcome: RoleplayNarratorPhaseOutcome::ProviderPhaseCompleted {
                output_text: "brief".to_string(),
            },
        })
        .expect_err("tampered output visibility");
        assert_eq!(
            invalid_visibility.reason_code,
            "roleplay_narrator_receipt_invalid"
        );

        let mut tampered = start;
        tampered.state.pending_text.push_str(" altered");
        let invalid = advance_narrator_turn(RoleplayNarratorAdvanceInput {
            receipt: tampered,
            outcome: RoleplayNarratorPhaseOutcome::ToolBatchCompleted {
                observations: vec![],
            },
        })
        .expect_err("tampered receipt");
        assert_eq!(invalid.reason_code, "roleplay_narrator_receipt_invalid");
    }

    fn narrator_start(
        pending_text: &str,
        review_enabled: bool,
        narrator_config: Option<RoleplayNarratorConfig>,
    ) -> RoleplayNarratorTurnReceipt {
        start_narrator_turn(RoleplayNarratorStartInput {
            wake_id: "wake-rp".to_string(),
            session_id: "session-rp".to_string(),
            profile_id: "profile-rp".to_string(),
            pending_text: pending_text.to_string(),
            narrator_config,
            review_enabled,
            max_review_cycles: None,
        })
        .expect("start narrator")
    }

    fn advance_tools(
        receipt: RoleplayNarratorTurnReceipt,
        observations: Vec<RoleplayNarratorToolObservation>,
    ) -> RoleplayNarratorTurnReceipt {
        advance_narrator_turn(RoleplayNarratorAdvanceInput {
            receipt,
            outcome: RoleplayNarratorPhaseOutcome::ToolBatchCompleted { observations },
        })
        .expect("advance tool batch")
    }

    fn advance_provider(
        receipt: RoleplayNarratorTurnReceipt,
        output_text: &str,
    ) -> RoleplayNarratorTurnReceipt {
        advance_narrator_turn(RoleplayNarratorAdvanceInput {
            receipt,
            outcome: RoleplayNarratorPhaseOutcome::ProviderPhaseCompleted {
                output_text: output_text.to_string(),
            },
        })
        .expect("advance provider phase")
    }

    fn provider_instructions(receipt: &RoleplayNarratorTurnReceipt) -> &str {
        let RoleplayNarratorDirective::ProviderPhase { instructions, .. } = &receipt.directive
        else {
            panic!("expected provider directive");
        };
        instructions
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

    fn lifecycle_session(status: &str) -> RoleplaySessionLifecycleSession {
        RoleplaySessionLifecycleSession {
            session_id: "session-rp".to_string(),
            agent_id: "agent-rp".to_string(),
            profile_id: "profile-rp".to_string(),
            kind: "full".to_string(),
            status: status.to_string(),
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
