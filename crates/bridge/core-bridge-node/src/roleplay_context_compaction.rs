//! Roleplay-specific preservation at Crew's native brain integration boundary.
//!
//! This module never owns transcript mutation, safe-boundary selection, or
//! artifact persistence. It receives Crew's frozen projection and returns a
//! declarative preservation decision for the provider-facing RP projection.

use rusty_crew_brain_runtime::{
    BrainContextCompactionPayloadLineage, BrainContextCompactionPreservationDecision,
    BrainContextCompactionQuality, BrainContextCompactionStrategy,
    BrainContextCompactionStrategyInput,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const ROLEPLAY_SCENE_AWARE_COMPACTION_STRATEGY_ID: &str = "roleplay_scene_aware_compaction";
pub const ROLEPLAY_SCENE_AWARE_COMPACTION_STRATEGY_REVISION: &str = "roleplay_scene_aware_v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RoleplaySceneBoundaryReason {
    SceneStarted,
    SceneEnded,
    DirectorBoundary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayCompactionSceneBoundaryV1 {
    pub scene_id: String,
    pub source_refs: Vec<String>,
    pub reason: RoleplaySceneBoundaryReason,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RoleplayCompactionRetentionTier {
    Critical,
    Scene,
    Recent,
    Discardable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayCompactionRetentionV1 {
    pub source_ref: String,
    pub tier: RoleplayCompactionRetentionTier,
    pub reason_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayCompactionDirectorsNoteV1 {
    pub note_id: String,
    pub text: String,
    pub provenance_source_refs: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RoleplayCompactionExtractionKind {
    #[serde(rename = "lore_fact")]
    Lore,
    #[serde(rename = "character_fact")]
    Character,
    #[serde(rename = "scene_fact")]
    Scene,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayCompactionExtractionRequestV1 {
    pub request_id: String,
    pub kind: RoleplayCompactionExtractionKind,
    pub source_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayCompactionDomainContextV1 {
    pub schema_version: u32,
    #[serde(default)]
    pub derive_source_refs: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene_boundary: Option<RoleplayCompactionSceneBoundaryV1>,
    #[serde(default)]
    pub retention_tiers: Vec<RoleplayCompactionRetentionV1>,
    #[serde(default)]
    pub directors_notes: Vec<RoleplayCompactionDirectorsNoteV1>,
    #[serde(default)]
    pub extraction_requests: Vec<RoleplayCompactionExtractionRequestV1>,
}

impl Default for RoleplayCompactionDomainContextV1 {
    fn default() -> Self {
        Self {
            schema_version: 1,
            derive_source_refs: false,
            scene_boundary: None,
            retention_tiers: Vec::new(),
            directors_notes: Vec::new(),
            extraction_requests: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayCompactionScenePayloadV1 {
    pub scene_id: String,
    pub summary: String,
    pub source_refs: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RoleplayCompactionFactKind {
    Lore,
    Character,
    Scene,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RoleplayCompactionFactConfidence {
    Exact,
    Derived,
    Uncertain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayCompactionRetainedFactV1 {
    pub fact_id: String,
    pub kind: RoleplayCompactionFactKind,
    pub text: String,
    pub source_refs: Vec<String>,
    pub confidence: RoleplayCompactionFactConfidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayCompactionPreservedDirectorsNoteV1 {
    pub note_id: String,
    pub text: String,
    pub source_refs: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RoleplayCompactionExtractionStatus {
    Completed,
    Partial,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayCompactionExtractionResultV1 {
    pub request_id: String,
    pub status: RoleplayCompactionExtractionStatus,
    pub fact_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleplayCompactionPreservationPayloadV1 {
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene: Option<RoleplayCompactionScenePayloadV1>,
    pub retained_facts: Vec<RoleplayCompactionRetainedFactV1>,
    pub directors_notes: Vec<RoleplayCompactionPreservedDirectorsNoteV1>,
    pub extraction_results: Vec<RoleplayCompactionExtractionResultV1>,
}

#[derive(Debug, Default)]
pub struct RoleplaySceneAwareCompactionStrategy;

impl BrainContextCompactionStrategy for RoleplaySceneAwareCompactionStrategy {
    fn strategy_id(&self) -> &str {
        ROLEPLAY_SCENE_AWARE_COMPACTION_STRATEGY_ID
    }

    fn strategy_revision(&self) -> &str {
        ROLEPLAY_SCENE_AWARE_COMPACTION_STRATEGY_REVISION
    }

    fn preserve(
        &self,
        input: BrainContextCompactionStrategyInput,
    ) -> Result<BrainContextCompactionPreservationDecision, String> {
        let mut domain = parse_domain_context(input.domain_context.as_ref())?;
        resolve_derived_source_refs(&input, &mut domain)?;
        validate_domain_context(&input, &domain)?;

        let boundary = input.safe_boundary.compact_before_item;
        if boundary == 0 {
            return Err("no completed roleplay history is available for compaction".into());
        }
        let tiers = domain
            .retention_tiers
            .iter()
            .map(|entry| (entry.source_ref.as_str(), entry.tier))
            .collect::<BTreeMap<_, _>>();
        let mut compacted_source_refs = Vec::new();
        let mut retained_source_refs = Vec::new();
        for (index, item) in input.snapshot.items.iter().enumerate() {
            let preserve_verbatim = index >= boundary
                || matches!(
                    tiers.get(item.source_ref.as_str()),
                    Some(
                        RoleplayCompactionRetentionTier::Critical
                            | RoleplayCompactionRetentionTier::Recent
                    )
                );
            if preserve_verbatim {
                retained_source_refs.push(item.source_ref.clone());
            } else {
                compacted_source_refs.push(item.source_ref.clone());
            }
        }
        if compacted_source_refs.is_empty() {
            return Err(
                "roleplay retention tiers leave no completed historical item to compact".into(),
            );
        }

        let item_by_ref = input
            .snapshot
            .items
            .iter()
            .map(|item| (item.source_ref.as_str(), item))
            .collect::<BTreeMap<_, _>>();
        let scene =
            domain
                .scene_boundary
                .as_ref()
                .map(|boundary| RoleplayCompactionScenePayloadV1 {
                    scene_id: boundary.scene_id.clone(),
                    summary: boundary
                        .summary
                        .clone()
                        .filter(|summary| !summary.trim().is_empty())
                        .unwrap_or_else(|| {
                            narrative_excerpt(&boundary.source_refs, &item_by_ref, 900)
                        }),
                    source_refs: boundary.source_refs.clone(),
                });
        let directors_notes = domain
            .directors_notes
            .iter()
            .map(|note| RoleplayCompactionPreservedDirectorsNoteV1 {
                note_id: note.note_id.clone(),
                text: note.text.clone(),
                source_refs: note.provenance_source_refs.clone(),
            })
            .collect::<Vec<_>>();
        let mut retained_facts = Vec::new();
        let mut extraction_results = Vec::new();
        for request in &domain.extraction_requests {
            let text = narrative_excerpt(&request.source_refs, &item_by_ref, 600);
            if text.is_empty() {
                extraction_results.push(RoleplayCompactionExtractionResultV1 {
                    request_id: request.request_id.clone(),
                    status: RoleplayCompactionExtractionStatus::Failed,
                    fact_ids: Vec::new(),
                    reason_code: Some("roleplay_extraction_has_no_narrative_text".to_string()),
                });
                continue;
            }
            let fact_id = stable_fact_id(&request.request_id, &request.source_refs, &text);
            retained_facts.push(RoleplayCompactionRetainedFactV1 {
                fact_id: fact_id.clone(),
                kind: match request.kind {
                    RoleplayCompactionExtractionKind::Lore => RoleplayCompactionFactKind::Lore,
                    RoleplayCompactionExtractionKind::Character => {
                        RoleplayCompactionFactKind::Character
                    }
                    RoleplayCompactionExtractionKind::Scene => RoleplayCompactionFactKind::Scene,
                },
                text,
                source_refs: request.source_refs.clone(),
                confidence: RoleplayCompactionFactConfidence::Derived,
            });
            extraction_results.push(RoleplayCompactionExtractionResultV1 {
                request_id: request.request_id.clone(),
                status: RoleplayCompactionExtractionStatus::Completed,
                fact_ids: vec![fact_id],
                reason_code: None,
            });
        }

        let mut warnings = Vec::new();
        if scene.is_none() {
            warnings.push("roleplay_scene_boundary_unavailable".to_string());
        }
        if directors_notes.is_empty() {
            warnings.push("roleplay_directors_notes_unavailable".to_string());
        }
        if domain.extraction_requests.is_empty() {
            warnings.push("roleplay_lore_extraction_not_requested".to_string());
        } else if extraction_results
            .iter()
            .any(|result| result.status != RoleplayCompactionExtractionStatus::Completed)
        {
            warnings.push("roleplay_lore_extraction_incomplete".to_string());
        }
        if !directors_notes.iter().any(|note| {
            let text = note.text.to_ascii_lowercase();
            text.contains("voice") && text.contains("emotional")
        }) {
            warnings.push("roleplay_voice_or_emotional_arc_unverified".to_string());
        }

        let payload = RoleplayCompactionPreservationPayloadV1 {
            schema_version: 1,
            scene,
            retained_facts,
            directors_notes,
            extraction_results,
        };
        let summary_text = directors_summary(&payload, &input, &compacted_source_refs);
        let quality = if warnings.is_empty() {
            BrainContextCompactionQuality::Derived
        } else {
            BrainContextCompactionQuality::Degraded
        };
        Ok(BrainContextCompactionPreservationDecision {
            strategy_id: self.strategy_id().to_string(),
            strategy_revision: self.strategy_revision().to_string(),
            summary_text,
            compacted_source_refs,
            retained_source_refs,
            preservation_payload: serde_json::to_value(payload)
                .map_err(|error| format!("serialize roleplay preservation payload: {error}"))?,
            payload_lineage: BrainContextCompactionPayloadLineage {
                source_projection_fingerprint: input.snapshot.source_projection_fingerprint,
                boundary_id: input.safe_boundary.boundary_id,
                parent_artifact_id: input.parent_artifact_id,
            },
            quality,
            warnings,
        })
    }
}

fn parse_domain_context(
    value: Option<&Value>,
) -> Result<RoleplayCompactionDomainContextV1, String> {
    let Some(value) = value else {
        return Ok(RoleplayCompactionDomainContextV1::default());
    };
    let domain = serde_json::from_value::<RoleplayCompactionDomainContextV1>(value.clone())
        .map_err(|error| format!("invalid roleplay compaction domain context: {error}"))?;
    if domain.schema_version != 1 {
        return Err(format!(
            "unsupported roleplay compaction domain context schema {}",
            domain.schema_version
        ));
    }
    Ok(domain)
}

fn resolve_derived_source_refs(
    input: &BrainContextCompactionStrategyInput,
    domain: &mut RoleplayCompactionDomainContextV1,
) -> Result<(), String> {
    if !domain.derive_source_refs {
        return Ok(());
    }
    let candidates = input
        .snapshot
        .items
        .iter()
        .take(input.safe_boundary.compact_before_item)
        .filter(|item| matches!(item.role.as_str(), "user" | "assistant"))
        .map(|item| item.source_ref.clone())
        .rev()
        .take(6)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Err(
            "roleplay domain context could not derive provenance before the safe boundary".into(),
        );
    }
    if let Some(scene) = &mut domain.scene_boundary {
        if scene.source_refs.is_empty() {
            scene.source_refs = candidates.clone();
        }
    }
    for note in &mut domain.directors_notes {
        if note.provenance_source_refs.is_empty() {
            note.provenance_source_refs = candidates.clone();
        }
    }
    for request in &mut domain.extraction_requests {
        if request.source_refs.is_empty() {
            request.source_refs = candidates.clone();
        }
    }
    Ok(())
}

fn validate_domain_context(
    input: &BrainContextCompactionStrategyInput,
    domain: &RoleplayCompactionDomainContextV1,
) -> Result<(), String> {
    let known = input
        .snapshot
        .items
        .iter()
        .map(|item| item.source_ref.as_str())
        .collect::<BTreeSet<_>>();
    let mut retention_refs = BTreeSet::new();
    for retention in &domain.retention_tiers {
        validate_source_ref(&known, &retention.source_ref)?;
        if retention.reason_code.trim().is_empty() {
            return Err("roleplay retention reason code must not be empty".into());
        }
        if !retention_refs.insert(retention.source_ref.as_str()) {
            return Err("roleplay retention source refs must be unique".into());
        }
    }
    if let Some(scene) = &domain.scene_boundary {
        if scene.scene_id.trim().is_empty() || scene.source_refs.is_empty() {
            return Err("roleplay scene boundary must identify a scene and sources".into());
        }
        validate_source_refs(&known, &scene.source_refs)?;
    }
    let mut note_ids = BTreeSet::new();
    for note in &domain.directors_notes {
        if note.note_id.trim().is_empty()
            || note.text.trim().is_empty()
            || note.provenance_source_refs.is_empty()
        {
            return Err("roleplay director note requires id, text, and provenance".into());
        }
        if !note_ids.insert(note.note_id.as_str()) {
            return Err("roleplay director note ids must be unique".into());
        }
        validate_source_refs(&known, &note.provenance_source_refs)?;
    }
    let mut request_ids = BTreeSet::new();
    for request in &domain.extraction_requests {
        if request.request_id.trim().is_empty() || request.source_refs.is_empty() {
            return Err("roleplay extraction request requires id and sources".into());
        }
        if !request_ids.insert(request.request_id.as_str()) {
            return Err("roleplay extraction request ids must be unique".into());
        }
        validate_source_refs(&known, &request.source_refs)?;
    }
    Ok(())
}

fn validate_source_refs(known: &BTreeSet<&str>, refs: &[String]) -> Result<(), String> {
    let mut unique = BTreeSet::new();
    for source_ref in refs {
        validate_source_ref(known, source_ref)?;
        if !unique.insert(source_ref.as_str()) {
            return Err("roleplay domain source refs must be unique".into());
        }
    }
    Ok(())
}

fn validate_source_ref(known: &BTreeSet<&str>, source_ref: &str) -> Result<(), String> {
    if !known.contains(source_ref) {
        return Err(format!(
            "roleplay domain context references unknown source {source_ref}"
        ));
    }
    Ok(())
}

fn narrative_excerpt(
    refs: &[String],
    items: &BTreeMap<&str, &rusty_crew_brain_runtime::BrainContextCompactionItem>,
    max_bytes: usize,
) -> String {
    let mut output = String::new();
    for source_ref in refs {
        let Some(item) = items.get(source_ref.as_str()) else {
            continue;
        };
        if matches!(
            item.role.as_str(),
            "tool" | "tool_call" | "tool_result" | "reasoning" | "reasoning_summary"
        ) {
            continue;
        }
        let text = item.content.trim();
        if text.is_empty() {
            continue;
        }
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(item.role.as_str());
        output.push_str(": ");
        output.push_str(text);
        truncate_string(&mut output, max_bytes);
        if output.len() >= max_bytes {
            break;
        }
    }
    output
}

fn directors_summary(
    payload: &RoleplayCompactionPreservationPayloadV1,
    input: &BrainContextCompactionStrategyInput,
    compacted_source_refs: &[String],
) -> String {
    let mut summary = String::from(
        "[Roleplay director context]\nEarlier completed RP projection items were selectively compacted. Canonical transcript and tool telemetry remain authoritative.\n",
    );
    if let Some(scene) = &payload.scene {
        summary.push_str("Scene: ");
        summary.push_str(&scene.scene_id);
        summary.push('\n');
        if !scene.summary.is_empty() {
            summary.push_str(&scene.summary);
            summary.push('\n');
        }
    }
    for note in &payload.directors_notes {
        summary.push_str("Director note: ");
        summary.push_str(note.text.trim());
        summary.push('\n');
    }
    for fact in &payload.retained_facts {
        summary.push_str("Preserved fact: ");
        summary.push_str(fact.text.trim());
        summary.push('\n');
    }
    if payload.scene.is_none() && payload.directors_notes.is_empty() {
        let compacted = compacted_source_refs
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let refs = input
            .snapshot
            .items
            .iter()
            .filter(|item| compacted.contains(&item.source_ref))
            .map(|item| item.source_ref.clone())
            .collect::<Vec<_>>();
        let item_by_ref = input
            .snapshot
            .items
            .iter()
            .map(|item| (item.source_ref.as_str(), item))
            .collect::<BTreeMap<_, _>>();
        let excerpt = narrative_excerpt(&refs, &item_by_ref, 1200);
        if !excerpt.is_empty() {
            summary.push_str("Narrative continuity (degraded):\n");
            summary.push_str(&excerpt);
            summary.push('\n');
        }
    }
    let budget = (input.policy.target_tokens() as usize)
        .saturating_mul(3)
        .clamp(768, 12_288);
    truncate_string(&mut summary, budget);
    summary
}

fn stable_fact_id(request_id: &str, refs: &[String], text: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(request_id.as_bytes());
    for source_ref in refs {
        digest.update([0]);
        digest.update(source_ref.as_bytes());
    }
    digest.update([0]);
    digest.update(text.as_bytes());
    let hash = format!("{:x}", digest.finalize());
    format!("roleplay-fact-{}", &hash[..20])
}

fn truncate_string(value: &mut String, max_bytes: usize) {
    if value.len() <= max_bytes {
        return;
    }
    let mut boundary = max_bytes;
    while boundary > 0 && !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    value.truncate(boundary);
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_crew_brain_runtime::{
        execute_compaction_strategy, BrainContextCompactionItem, BrainContextCompactionPolicy,
        BrainContextCompactionSnapshot, BrainContextCompactionStrategyFailureKind,
        BrainContextSafeCompactionBoundary,
    };
    use std::sync::Arc;
    use std::time::Duration;

    use serde_json::json;

    fn input(domain_context: Value) -> BrainContextCompactionStrategyInput {
        let items = vec![
            item("rp-0", "user", "Elara touched the silver locket."),
            item("rp-1", "assistant", "Mara whispered, \"I still remember.\""),
            item("rp-2", "tool_call", "search_lore({secret arguments})"),
            item("rp-3", "tool_result", "private retrieval telemetry"),
            item("rp-4", "user", "She did not look away."),
            item("rp-5", "assistant", "The silence became a promise."),
        ];
        BrainContextCompactionStrategyInput {
            snapshot: BrainContextCompactionSnapshot {
                source_projection_fingerprint: "rp-projection-1".to_string(),
                items,
            },
            policy: BrainContextCompactionPolicy {
                enabled: true,
                auto_compaction_enabled: true,
                strategy_id: ROLEPLAY_SCENE_AWARE_COMPACTION_STRATEGY_ID.to_string(),
                context_window_tokens: 8_000,
                compact_at_percent: 70,
                target_percent_after_compaction: 45,
            },
            safe_boundary: BrainContextSafeCompactionBoundary {
                boundary_id: "rp-before-4".to_string(),
                compact_before_item: 4,
                active_tool_exchange_id: None,
            },
            domain_context: Some(domain_context),
            parent_artifact_id: Some("prior-rp-artifact".to_string()),
        }
    }

    fn item(source_ref: &str, role: &str, content: &str) -> BrainContextCompactionItem {
        BrainContextCompactionItem {
            source_ref: source_ref.to_string(),
            role: role.to_string(),
            content: content.to_string(),
            tool_exchange_id: None,
            tool_exchange_completed: true,
            metadata: Value::Null,
        }
    }

    fn rich_domain_context() -> Value {
        json!({
            "schemaVersion": 1,
            "sceneBoundary": {
                "sceneId": "the-silver-locket",
                "sourceRefs": ["rp-0", "rp-1"],
                "reason": "scene_ended"
            },
            "retentionTiers": [
                {"sourceRef": "rp-0", "tier": "scene", "reasonCode": "completed_scene"},
                {"sourceRef": "rp-1", "tier": "critical", "reasonCode": "voice_sample"},
                {"sourceRef": "rp-2", "tier": "discardable", "reasonCode": "historic_tool_call"},
                {"sourceRef": "rp-3", "tier": "discardable", "reasonCode": "historic_tool_result"},
                {"sourceRef": "rp-4", "tier": "recent", "reasonCode": "active_scene"}
            ],
            "directorsNotes": [{
                "noteId": "note-locket",
                "text": "Emotional arc: guarded grief to recognition. Voice sample: Mara says I still remember. Symbolic object: the silver locket remains unresolved.",
                "provenanceSourceRefs": ["rp-0", "rp-1"]
            }],
            "extractionRequests": [{
                "requestId": "fact-locket",
                "kind": "lore_fact",
                "sourceRefs": ["rp-0"]
            }]
        })
    }

    #[test]
    fn preserves_scene_tiers_voice_and_lore_provenance() {
        let decision = RoleplaySceneAwareCompactionStrategy
            .preserve(input(rich_domain_context()))
            .expect("roleplay preservation decision");
        assert_eq!(decision.quality, BrainContextCompactionQuality::Derived);
        assert_eq!(decision.compacted_source_refs, vec!["rp-0", "rp-2", "rp-3"]);
        assert_eq!(decision.retained_source_refs, vec!["rp-1", "rp-4", "rp-5"]);
        assert!(!decision.summary_text.contains("search_lore"));
        assert!(!decision
            .summary_text
            .contains("private retrieval telemetry"));
        assert!(decision.summary_text.contains("silver locket"));
        let payload = serde_json::from_value::<RoleplayCompactionPreservationPayloadV1>(
            decision.preservation_payload,
        )
        .expect("typed payload");
        assert_eq!(
            payload.scene.as_ref().map(|scene| scene.scene_id.as_str()),
            Some("the-silver-locket")
        );
        assert_eq!(payload.retained_facts.len(), 1);
        assert_eq!(payload.retained_facts[0].source_refs, vec!["rp-0"]);
        assert_eq!(
            payload.extraction_results[0].status,
            RoleplayCompactionExtractionStatus::Completed
        );
    }

    #[test]
    fn incomplete_domain_evidence_degrades_without_dropping_recent_history() {
        let decision = RoleplaySceneAwareCompactionStrategy
            .preserve(input(json!({"schemaVersion": 1})))
            .expect("degraded preservation decision");
        assert_eq!(decision.quality, BrainContextCompactionQuality::Degraded);
        assert!(decision
            .warnings
            .contains(&"roleplay_scene_boundary_unavailable".to_string()));
        assert!(decision.retained_source_refs.contains(&"rp-4".to_string()));
        assert!(decision.retained_source_refs.contains(&"rp-5".to_string()));
        assert!(!decision.summary_text.contains("secret arguments"));
    }

    #[test]
    fn production_evidence_derives_valid_refs_inside_the_safe_boundary() {
        let decision = RoleplaySceneAwareCompactionStrategy
            .preserve(input(json!({
                "schemaVersion": 1,
                "deriveSourceRefs": true,
                "sceneBoundary": {
                    "sceneId": "session-1",
                    "sourceRefs": [],
                    "reason": "director_boundary",
                    "summary": "The locket promise continues in the orchard."
                },
                "directorsNotes": [{
                    "noteId": "scene:session-1",
                    "text": "Preserve voice and emotional continuity around the locket.",
                    "provenanceSourceRefs": []
                }],
                "extractionRequests": [{
                    "requestId": "lore:locket",
                    "kind": "lore_fact",
                    "sourceRefs": []
                }]
            })))
            .expect("derived production provenance");
        let payload = serde_json::from_value::<RoleplayCompactionPreservationPayloadV1>(
            decision.preservation_payload,
        )
        .expect("typed payload");
        let scene = payload.scene.expect("scene payload");
        assert_eq!(
            scene.summary,
            "The locket promise continues in the orchard."
        );
        assert_eq!(scene.source_refs, vec!["rp-0", "rp-1"]);
        assert_eq!(payload.directors_notes[0].source_refs, vec!["rp-0", "rp-1"]);
        assert_eq!(payload.retained_facts[0].source_refs, vec!["rp-0", "rp-1"]);
        assert!(!decision.compacted_source_refs.contains(&"rp-4".to_string()));
        assert!(!decision.compacted_source_refs.contains(&"rp-5".to_string()));
    }

    #[test]
    fn failed_domain_validation_preserves_the_prior_projection_for_retry() {
        let strategy: Arc<dyn BrainContextCompactionStrategy> =
            Arc::new(RoleplaySceneAwareCompactionStrategy);
        let failure = execute_compaction_strategy(
            strategy,
            input(json!({
                "schemaVersion": 1,
                "retentionTiers": [{
                    "sourceRef": "missing-source",
                    "tier": "critical",
                    "reasonCode": "bad_fixture"
                }]
            })),
            Duration::from_secs(1),
        )
        .expect_err("unknown provenance must fail closed");
        assert_eq!(
            failure.kind,
            BrainContextCompactionStrategyFailureKind::StrategyFailed
        );
        assert_eq!(failure.reason_code, "compaction_strategy_failed");
        assert!(failure.retryable);
        assert!(failure.preserves_prior_projection);
    }
}
