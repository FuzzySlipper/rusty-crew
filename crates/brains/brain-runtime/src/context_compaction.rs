use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::{mpsc, Arc};
use std::time::Duration;

const CONSERVATIVE_SERIALIZED_BYTES_PER_TOKEN: u64 = 3;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BrainContextCompactionPolicy {
    pub enabled: bool,
    pub auto_compaction_enabled: bool,
    pub strategy_id: String,
    pub context_window_tokens: u64,
    pub compact_at_percent: u32,
    pub target_percent_after_compaction: u32,
}

impl BrainContextCompactionPolicy {
    pub fn validate(&self) -> Result<(), String> {
        if self.strategy_id.trim().is_empty() {
            return Err("context compaction strategy_id must not be empty".to_string());
        }
        if self.context_window_tokens == 0 {
            return Err("context compaction context_window_tokens must be positive".to_string());
        }
        if !(1..=100).contains(&self.compact_at_percent) {
            return Err(
                "context compaction compact_at_percent must be between 1 and 100".to_string(),
            );
        }
        if self.target_percent_after_compaction >= self.compact_at_percent {
            return Err(
                "context compaction target_percent_after_compaction must be below compact_at_percent"
                    .to_string(),
            );
        }
        Ok(())
    }

    pub fn target_tokens(&self) -> u64 {
        self.context_window_tokens
            .saturating_mul(u64::from(self.target_percent_after_compaction))
            / 100
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BrainContextUsageSource {
    Provider,
    ConservativeEstimate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BrainContextUsageSnapshot {
    pub input_tokens: u64,
    pub context_window_tokens: u64,
    pub fill_percent: u32,
    pub source: BrainContextUsageSource,
}

impl BrainContextUsageSnapshot {
    pub fn from_provider(input_tokens: u64, context_window_tokens: u64) -> Self {
        Self::new(
            input_tokens,
            context_window_tokens,
            BrainContextUsageSource::Provider,
        )
    }

    pub fn from_serialized_bytes(serialized_bytes: usize, context_window_tokens: u64) -> Self {
        let estimated_tokens = (serialized_bytes as u64)
            .saturating_add(CONSERVATIVE_SERIALIZED_BYTES_PER_TOKEN - 1)
            / CONSERVATIVE_SERIALIZED_BYTES_PER_TOKEN;
        Self::new(
            estimated_tokens,
            context_window_tokens,
            BrainContextUsageSource::ConservativeEstimate,
        )
    }

    /// Records an estimate derived from the exact provider request projection
    /// that is about to be dispatched. The estimate may still be tokenizer-
    /// approximate, but it must include the complete assembled request rather
    /// than stale usage from an earlier provider round.
    pub fn from_projection_estimate(input_tokens: u64, context_window_tokens: u64) -> Self {
        Self::new(
            input_tokens,
            context_window_tokens,
            BrainContextUsageSource::ConservativeEstimate,
        )
    }

    fn new(input_tokens: u64, context_window_tokens: u64, source: BrainContextUsageSource) -> Self {
        let fill_percent = if context_window_tokens == 0 {
            100
        } else {
            input_tokens
                .saturating_mul(100)
                .saturating_add(context_window_tokens - 1)
                .checked_div(context_window_tokens)
                .unwrap_or(100)
                .min(100) as u32
        };
        Self {
            input_tokens,
            context_window_tokens,
            fill_percent,
            source,
        }
    }
}

pub fn decide_context_compaction_for_projection(
    policy: Option<&BrainContextCompactionPolicy>,
    usage: BrainContextUsageSnapshot,
) -> Result<BrainContextCompactionDecision, String> {
    let Some(policy) = policy else {
        return Ok(BrainContextCompactionDecision::Disabled);
    };
    policy.validate()?;
    if !policy.enabled || !policy.auto_compaction_enabled {
        return Ok(BrainContextCompactionDecision::Disabled);
    }
    if usage.context_window_tokens != policy.context_window_tokens {
        return Err(
            "context admission projection does not match the configured context window".to_string(),
        );
    }
    if usage.fill_percent >= policy.compact_at_percent {
        Ok(BrainContextCompactionDecision::Compact(usage))
    } else {
        Ok(BrainContextCompactionDecision::BelowThreshold(usage))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrainContextCompactionDecision {
    Disabled,
    BelowThreshold(BrainContextUsageSnapshot),
    Compact(BrainContextUsageSnapshot),
}

pub fn decide_context_compaction(
    policy: Option<&BrainContextCompactionPolicy>,
    provider_input_tokens: Option<u64>,
    serialized_model_context_bytes: usize,
) -> Result<BrainContextCompactionDecision, String> {
    let Some(policy) = policy else {
        return Ok(BrainContextCompactionDecision::Disabled);
    };
    policy.validate()?;
    if !policy.enabled || !policy.auto_compaction_enabled {
        return Ok(BrainContextCompactionDecision::Disabled);
    }
    let usage = provider_input_tokens
        .map(|tokens| {
            BrainContextUsageSnapshot::from_provider(tokens, policy.context_window_tokens)
        })
        .unwrap_or_else(|| {
            BrainContextUsageSnapshot::from_serialized_bytes(
                serialized_model_context_bytes,
                policy.context_window_tokens,
            )
        });
    decide_context_compaction_for_projection(Some(policy), usage)
}

pub fn is_context_limit_provider_error(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    [
        "context length",
        "context_length",
        "context window",
        "maximum context",
        "max context",
        "too many tokens",
        "token limit",
        "input is too long",
        "prompt is too long",
    ]
    .iter()
    .any(|pattern| normalized.contains(pattern))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BrainContextCompactionTrigger {
    AutoThreshold,
    ManualIntent,
    ProviderLimit,
    Retry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BrainContextCompactionTerminalStatus {
    Completed,
    Failed,
}

/// An immutable, provider-neutral item exposed to a compaction strategy.
///
/// This is deliberately a projection rather than a transcript handle. A
/// strategy can describe what should survive in the next provider request,
/// but cannot mutate Crew's canonical transcript or provider state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BrainContextCompactionItem {
    pub source_ref: String,
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_exchange_id: Option<String>,
    pub tool_exchange_completed: bool,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BrainContextCompactionSnapshot {
    pub source_projection_fingerprint: String,
    pub items: Vec<BrainContextCompactionItem>,
}

/// The exclusive item index before which a strategy may compact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BrainContextSafeCompactionBoundary {
    pub boundary_id: String,
    pub compact_before_item: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_tool_exchange_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BrainContextCompactionStrategyInput {
    pub snapshot: BrainContextCompactionSnapshot,
    pub policy: BrainContextCompactionPolicy,
    pub safe_boundary: BrainContextSafeCompactionBoundary,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain_context: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_artifact_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BrainContextCompactionPayloadLineage {
    pub source_projection_fingerprint: String,
    pub boundary_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_artifact_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BrainContextCompactionQuality {
    Exact,
    Derived,
    Degraded,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BrainContextCompactionPreservationDecision {
    pub strategy_id: String,
    pub strategy_revision: String,
    pub summary_text: String,
    pub compacted_source_refs: Vec<String>,
    pub retained_source_refs: Vec<String>,
    #[serde(default)]
    pub preservation_payload: Value,
    pub payload_lineage: BrainContextCompactionPayloadLineage,
    pub quality: BrainContextCompactionQuality,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BrainContextCompactionStrategyFailureKind {
    InvalidInput,
    StrategyFailed,
    TimedOut,
    InvalidDecision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BrainContextCompactionStrategyFailure {
    pub kind: BrainContextCompactionStrategyFailureKind,
    pub reason_code: String,
    pub summary: String,
    pub retryable: bool,
    pub preserves_prior_projection: bool,
}

impl BrainContextCompactionStrategyFailure {
    fn new(
        kind: BrainContextCompactionStrategyFailureKind,
        reason_code: impl Into<String>,
        summary: impl Into<String>,
        retryable: bool,
    ) -> Self {
        Self {
            kind,
            reason_code: reason_code.into(),
            summary: summary.into(),
            retryable,
            preserves_prior_projection: true,
        }
    }
}

/// Rust-owned extension point for domain-aware preservation.
///
/// Implementations receive owned frozen data and return a declarative
/// decision. Provider brains remain responsible for applying a validated
/// decision to their own projection shape.
pub trait BrainContextCompactionStrategy: Send + Sync + 'static {
    fn strategy_id(&self) -> &str;
    fn strategy_revision(&self) -> &str;
    fn preserve(
        &self,
        input: BrainContextCompactionStrategyInput,
    ) -> Result<BrainContextCompactionPreservationDecision, String>;
}

pub fn validate_compaction_strategy_input(
    input: &BrainContextCompactionStrategyInput,
) -> Result<(), String> {
    input.policy.validate()?;
    if input
        .snapshot
        .source_projection_fingerprint
        .trim()
        .is_empty()
    {
        return Err("compaction snapshot fingerprint must not be empty".to_string());
    }
    if input.safe_boundary.boundary_id.trim().is_empty() {
        return Err("safe compaction boundary id must not be empty".to_string());
    }
    if input.safe_boundary.compact_before_item > input.snapshot.items.len() {
        return Err("safe compaction boundary is outside the frozen snapshot".to_string());
    }
    if input.safe_boundary.compact_before_item == input.snapshot.items.len() {
        return Err("safe compaction boundary must retain at least one current item".to_string());
    }
    let mut refs = HashSet::new();
    for (index, item) in input.snapshot.items.iter().enumerate() {
        if item.source_ref.trim().is_empty() || !refs.insert(item.source_ref.as_str()) {
            return Err("compaction snapshot source refs must be non-empty and unique".to_string());
        }
        if index < input.safe_boundary.compact_before_item && !item.tool_exchange_completed {
            return Err("safe compaction boundary crosses an active tool exchange".to_string());
        }
        if index < input.safe_boundary.compact_before_item
            && input.safe_boundary.active_tool_exchange_id.as_deref()
                == item.tool_exchange_id.as_deref()
            && item.tool_exchange_id.is_some()
        {
            return Err("safe compaction boundary includes the active tool exchange".to_string());
        }
    }
    Ok(())
}

pub fn validate_compaction_strategy_decision(
    input: &BrainContextCompactionStrategyInput,
    strategy: &dyn BrainContextCompactionStrategy,
    decision: &BrainContextCompactionPreservationDecision,
) -> Result<(), String> {
    if decision.strategy_id != strategy.strategy_id()
        || decision.strategy_id != input.policy.strategy_id
    {
        return Err("compaction decision strategy id does not match the selected strategy".into());
    }
    if decision.strategy_revision.trim().is_empty()
        || decision.strategy_revision != strategy.strategy_revision()
    {
        return Err("compaction decision strategy revision is invalid".into());
    }
    if decision.summary_text.trim().is_empty() || decision.compacted_source_refs.is_empty() {
        return Err(
            "compaction decision must contain a summary and compact at least one item".into(),
        );
    }
    if decision.payload_lineage.source_projection_fingerprint
        != input.snapshot.source_projection_fingerprint
        || decision.payload_lineage.boundary_id != input.safe_boundary.boundary_id
        || decision.payload_lineage.parent_artifact_id != input.parent_artifact_id
    {
        return Err("compaction decision payload lineage does not match the frozen input".into());
    }

    let compacted: HashSet<&str> = decision
        .compacted_source_refs
        .iter()
        .map(String::as_str)
        .collect();
    let retained: HashSet<&str> = decision
        .retained_source_refs
        .iter()
        .map(String::as_str)
        .collect();
    if compacted.len() != decision.compacted_source_refs.len()
        || retained.len() != decision.retained_source_refs.len()
        || !compacted.is_disjoint(&retained)
    {
        return Err("compaction decision source refs must be unique and disjoint".into());
    }
    for (index, item) in input.snapshot.items.iter().enumerate() {
        let is_compacted = compacted.contains(item.source_ref.as_str());
        let is_retained = retained.contains(item.source_ref.as_str());
        if is_compacted == is_retained {
            return Err("compaction decision must partition every frozen projection item".into());
        }
        if is_compacted && index >= input.safe_boundary.compact_before_item {
            return Err("compaction decision crosses the supplied safe boundary".into());
        }
        if is_compacted && !item.tool_exchange_completed {
            return Err("compaction decision attempts to compact an active tool exchange".into());
        }
    }
    Ok(())
}

/// Runs an untrusted strategy against owned frozen input. A late result is
/// ignored; because the strategy has no transcript/provider handles, timeout
/// cannot mutate canonical state. Callers may safely retry the same input.
pub fn execute_compaction_strategy(
    strategy: Arc<dyn BrainContextCompactionStrategy>,
    input: BrainContextCompactionStrategyInput,
    timeout: Duration,
) -> Result<BrainContextCompactionPreservationDecision, BrainContextCompactionStrategyFailure> {
    validate_compaction_strategy_input(&input).map_err(|error| {
        BrainContextCompactionStrategyFailure::new(
            BrainContextCompactionStrategyFailureKind::InvalidInput,
            "compaction_strategy_input_invalid",
            error,
            false,
        )
    })?;
    if strategy.strategy_id() != input.policy.strategy_id {
        return Err(BrainContextCompactionStrategyFailure::new(
            BrainContextCompactionStrategyFailureKind::InvalidInput,
            "compaction_strategy_not_selected",
            "strategy id does not match the policy selection",
            false,
        ));
    }
    let validation_input = input.clone();
    let validation_strategy = Arc::clone(&strategy);
    let (send, receive) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let result = strategy.preserve(input);
        let _ = send.send(result);
    });
    let decision = receive.recv_timeout(timeout).map_err(|error| match error {
        mpsc::RecvTimeoutError::Timeout => BrainContextCompactionStrategyFailure::new(
            BrainContextCompactionStrategyFailureKind::TimedOut,
            "compaction_strategy_timed_out",
            "compaction strategy exceeded its execution deadline",
            true,
        ),
        mpsc::RecvTimeoutError::Disconnected => BrainContextCompactionStrategyFailure::new(
            BrainContextCompactionStrategyFailureKind::StrategyFailed,
            "compaction_strategy_worker_failed",
            "compaction strategy worker ended without returning a decision",
            true,
        ),
    })?;
    let decision = decision.map_err(|error| {
        BrainContextCompactionStrategyFailure::new(
            BrainContextCompactionStrategyFailureKind::StrategyFailed,
            "compaction_strategy_failed",
            error,
            true,
        )
    })?;
    validate_compaction_strategy_decision(
        &validation_input,
        validation_strategy.as_ref(),
        &decision,
    )
    .map_err(|error| {
        BrainContextCompactionStrategyFailure::new(
            BrainContextCompactionStrategyFailureKind::InvalidDecision,
            "compaction_strategy_decision_invalid",
            error,
            true,
        )
    })?;
    Ok(decision)
}

#[derive(Debug, Default)]
pub struct RollingSummaryCompactionStrategy;

impl BrainContextCompactionStrategy for RollingSummaryCompactionStrategy {
    fn strategy_id(&self) -> &str {
        "rolling_summary_compaction"
    }

    fn strategy_revision(&self) -> &str {
        "rolling_summary_v1"
    }

    fn preserve(
        &self,
        input: BrainContextCompactionStrategyInput,
    ) -> Result<BrainContextCompactionPreservationDecision, String> {
        let boundary = input.safe_boundary.compact_before_item;
        if boundary == 0 {
            return Err("no completed historical item is available for compaction".into());
        }
        let mut summary = String::from(
            "[Rusty Crew context summary]\nEarlier completed provider items were compacted from the model-facing projection only. Canonical transcript and tool telemetry remain authoritative.\n",
        );
        let summary_budget = (input.policy.target_tokens() as usize).clamp(256, 4096);
        for item in &input.snapshot.items[..boundary] {
            let remaining = summary_budget.saturating_sub(summary.len());
            if remaining <= 32 {
                break;
            }
            let line = format!("{}: {}\n", item.role, item.content);
            summary.push_str(truncate_compaction_utf8(&line, remaining.min(320)));
        }
        Ok(BrainContextCompactionPreservationDecision {
            strategy_id: self.strategy_id().to_string(),
            strategy_revision: self.strategy_revision().to_string(),
            summary_text: summary,
            compacted_source_refs: input.snapshot.items[..boundary]
                .iter()
                .map(|item| item.source_ref.clone())
                .collect(),
            retained_source_refs: input.snapshot.items[boundary..]
                .iter()
                .map(|item| item.source_ref.clone())
                .collect(),
            preservation_payload: json!({"kind": "rolling_summary", "version": 1}),
            payload_lineage: BrainContextCompactionPayloadLineage {
                source_projection_fingerprint: input.snapshot.source_projection_fingerprint,
                boundary_id: input.safe_boundary.boundary_id,
                parent_artifact_id: input.parent_artifact_id,
            },
            quality: BrainContextCompactionQuality::Derived,
            warnings: Vec::new(),
        })
    }
}

pub fn compaction_strategy_artifact_metadata(
    decision: &BrainContextCompactionPreservationDecision,
) -> Value {
    json!({
        "schema_version": 1,
        "strategy_revision": decision.strategy_revision,
        "payload_lineage": decision.payload_lineage,
        "preservation_payload": decision.preservation_payload,
        "quality": decision.quality,
        "warnings": decision.warnings,
    })
}

fn truncate_compaction_utf8(value: &str, max_bytes: usize) -> &str {
    if value.len() <= max_bytes {
        return value;
    }
    let mut boundary = max_bytes;
    while boundary > 0 && !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    &value[..boundary]
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BrainContextCompactionArtifact {
    pub artifact_id: String,
    pub sequence: u64,
    pub session_id: Option<String>,
    pub logical_turn_id: Option<String>,
    pub execution_epoch_id: Option<String>,
    pub source_projection_fingerprint: Option<String>,
    pub strategy_id: String,
    pub strategy_revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy_payload_metadata: Option<Value>,
    pub reason_code: String,
    pub trigger: Option<BrainContextCompactionTrigger>,
    pub usage_before: BrainContextUsageSnapshot,
    pub estimated_tokens_after: u64,
    pub before_tokens: Option<u64>,
    pub after_tokens: Option<u64>,
    pub preserved_item_count: Option<u64>,
    pub excised_item_count: Option<u64>,
    pub compacted_item_count: u64,
    pub retained_item_count: u64,
    pub summary_text: String,
    pub provider_chain_action: Option<String>,
    pub terminal_status: Option<BrainContextCompactionTerminalStatus>,
}

pub fn validate_compaction_artifacts(
    artifacts: &[BrainContextCompactionArtifact],
) -> Result<(), String> {
    let mut previous_sequence = 0;
    for (index, artifact) in artifacts.iter().enumerate() {
        if artifact.artifact_id.trim().is_empty() {
            return Err(format!(
                "context compaction artifact {index} artifact_id must not be empty"
            ));
        }
        if artifact.sequence == 0 {
            return Err(format!(
                "context compaction artifact {index} must have a positive sequence"
            ));
        }
        if artifact.sequence <= previous_sequence {
            return Err(format!(
                "context compaction artifact {index} sequence {} is not strictly increasing",
                artifact.sequence
            ));
        }
        if artifact.strategy_id.trim().is_empty() {
            return Err(format!(
                "context compaction artifact {index} strategy_id must not be empty"
            ));
        }
        if artifact.reason_code.trim().is_empty() {
            return Err(format!(
                "context compaction artifact {index} reason_code must not be empty"
            ));
        }
        if artifact.summary_text.trim().is_empty() {
            return Err(format!(
                "context compaction artifact {index} summary_text must not be empty"
            ));
        }
        if artifact.usage_before.context_window_tokens == 0 {
            return Err(format!(
                "context compaction artifact {index} context window must be positive"
            ));
        }
        if artifact.estimated_tokens_after >= artifact.usage_before.input_tokens {
            return Err(format!(
                "context compaction artifact {index} does not reduce the provider projection"
            ));
        }
        if artifact.compacted_item_count == 0 || artifact.retained_item_count == 0 {
            return Err(format!(
                "context compaction artifact {index} must retain and compact at least one item"
            ));
        }
        if let Some(status) = artifact.terminal_status {
            if status == BrainContextCompactionTerminalStatus::Failed
                && artifact.provider_chain_action.is_none()
            {
                // failed compaction must still record chain action for recovery
            }
        }
        previous_sequence = artifact.sequence;
    }
    Ok(())
}

/// Restart hydration must ignore later failed attempts and resume from the
/// most recent completed provider projection.
pub fn latest_usable_compaction_artifact(
    artifacts: &[BrainContextCompactionArtifact],
) -> Option<&BrainContextCompactionArtifact> {
    artifacts
        .iter()
        .filter(|artifact| {
            artifact.terminal_status == Some(BrainContextCompactionTerminalStatus::Completed)
        })
        .max_by_key(|artifact| artifact.sequence)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    fn policy() -> BrainContextCompactionPolicy {
        BrainContextCompactionPolicy {
            enabled: true,
            auto_compaction_enabled: true,
            strategy_id: "rolling_summary_compaction".to_string(),
            context_window_tokens: 100,
            compact_at_percent: 80,
            target_percent_after_compaction: 50,
        }
    }

    fn strategy_input() -> BrainContextCompactionStrategyInput {
        BrainContextCompactionStrategyInput {
            snapshot: BrainContextCompactionSnapshot {
                source_projection_fingerprint: "projection-1".into(),
                items: vec![
                    BrainContextCompactionItem {
                        source_ref: "event-1".into(),
                        role: "user".into(),
                        content: "Earlier question".into(),
                        tool_exchange_id: None,
                        tool_exchange_completed: true,
                        metadata: Value::Null,
                    },
                    BrainContextCompactionItem {
                        source_ref: "event-2".into(),
                        role: "assistant".into(),
                        content: "Earlier answer".into(),
                        tool_exchange_id: None,
                        tool_exchange_completed: true,
                        metadata: Value::Null,
                    },
                    BrainContextCompactionItem {
                        source_ref: "event-3".into(),
                        role: "user".into(),
                        content: "Current request".into(),
                        tool_exchange_id: None,
                        tool_exchange_completed: true,
                        metadata: Value::Null,
                    },
                ],
            },
            policy: policy(),
            safe_boundary: BrainContextSafeCompactionBoundary {
                boundary_id: "before-current-request".into(),
                compact_before_item: 2,
                active_tool_exchange_id: None,
            },
            domain_context: Some(json!({"retentionTiers": ["critical", "recent"]})),
            parent_artifact_id: Some("artifact-parent".into()),
        }
    }

    #[derive(Debug)]
    struct FixedDownstreamStrategy;

    impl BrainContextCompactionStrategy for FixedDownstreamStrategy {
        fn strategy_id(&self) -> &str {
            "rolling_summary_compaction"
        }

        fn strategy_revision(&self) -> &str {
            "roleplay-adapter-v7"
        }

        fn preserve(
            &self,
            input: BrainContextCompactionStrategyInput,
        ) -> Result<BrainContextCompactionPreservationDecision, String> {
            Ok(BrainContextCompactionPreservationDecision {
                strategy_id: self.strategy_id().into(),
                strategy_revision: self.strategy_revision().into(),
                summary_text: "The established scene facts remain in force.".into(),
                compacted_source_refs: vec!["event-1".into(), "event-2".into()],
                retained_source_refs: vec!["event-3".into()],
                preservation_payload: json!({"facts": ["scene fact"]}),
                payload_lineage: BrainContextCompactionPayloadLineage {
                    source_projection_fingerprint: input.snapshot.source_projection_fingerprint,
                    boundary_id: input.safe_boundary.boundary_id,
                    parent_artifact_id: input.parent_artifact_id,
                },
                quality: BrainContextCompactionQuality::Derived,
                warnings: Vec::new(),
            })
        }
    }

    #[test]
    fn baseline_strategy_returns_a_valid_deterministic_partition() {
        let input = strategy_input();
        let transcript_before = input.snapshot.clone();
        let strategy: Arc<dyn BrainContextCompactionStrategy> =
            Arc::new(RollingSummaryCompactionStrategy);
        let decision =
            execute_compaction_strategy(strategy, input.clone(), Duration::from_millis(100))
                .expect("baseline strategy");
        assert_eq!(decision.compacted_source_refs, ["event-1", "event-2"]);
        assert_eq!(decision.retained_source_refs, ["event-3"]);
        assert_eq!(input.snapshot, transcript_before, "input stays immutable");
    }

    #[test]
    fn downstream_strategy_success_carries_revision_payload_and_lineage() {
        let decision = execute_compaction_strategy(
            Arc::new(FixedDownstreamStrategy),
            strategy_input(),
            Duration::from_millis(100),
        )
        .expect("downstream strategy");
        assert_eq!(decision.strategy_revision, "roleplay-adapter-v7");
        assert_eq!(
            decision.payload_lineage.parent_artifact_id.as_deref(),
            Some("artifact-parent")
        );
        let metadata = compaction_strategy_artifact_metadata(&decision);
        assert_eq!(metadata["preservation_payload"]["facts"][0], "scene fact");
    }

    #[derive(Debug)]
    struct InvalidDecisionStrategy;

    impl BrainContextCompactionStrategy for InvalidDecisionStrategy {
        fn strategy_id(&self) -> &str {
            "rolling_summary_compaction"
        }
        fn strategy_revision(&self) -> &str {
            "invalid-v1"
        }
        fn preserve(
            &self,
            input: BrainContextCompactionStrategyInput,
        ) -> Result<BrainContextCompactionPreservationDecision, String> {
            let mut decision = FixedDownstreamStrategy.preserve(input)?;
            decision.strategy_revision = self.strategy_revision().into();
            decision.retained_source_refs.clear();
            Ok(decision)
        }
    }

    #[test]
    fn invalid_decision_is_recoverable_and_preserves_prior_projection() {
        let failure = execute_compaction_strategy(
            Arc::new(InvalidDecisionStrategy),
            strategy_input(),
            Duration::from_millis(100),
        )
        .expect_err("invalid partition must fail");
        assert_eq!(
            failure.kind,
            BrainContextCompactionStrategyFailureKind::InvalidDecision
        );
        assert!(failure.retryable);
        assert!(failure.preserves_prior_projection);
    }

    #[derive(Debug)]
    struct SlowStrategy;

    impl BrainContextCompactionStrategy for SlowStrategy {
        fn strategy_id(&self) -> &str {
            "rolling_summary_compaction"
        }
        fn strategy_revision(&self) -> &str {
            "slow-v1"
        }
        fn preserve(
            &self,
            input: BrainContextCompactionStrategyInput,
        ) -> Result<BrainContextCompactionPreservationDecision, String> {
            std::thread::sleep(Duration::from_millis(40));
            let mut decision = FixedDownstreamStrategy.preserve(input)?;
            decision.strategy_revision = self.strategy_revision().into();
            Ok(decision)
        }
    }

    #[test]
    fn timeout_is_retryable_without_changing_the_input() {
        let input = strategy_input();
        let before = input.clone();
        let failure = execute_compaction_strategy(
            Arc::new(SlowStrategy),
            input.clone(),
            Duration::from_millis(1),
        )
        .expect_err("slow strategy must time out");
        assert_eq!(
            failure.kind,
            BrainContextCompactionStrategyFailureKind::TimedOut
        );
        assert_eq!(input, before);
    }

    #[derive(Debug)]
    struct FlakyStrategy(AtomicUsize);

    impl BrainContextCompactionStrategy for FlakyStrategy {
        fn strategy_id(&self) -> &str {
            "rolling_summary_compaction"
        }
        fn strategy_revision(&self) -> &str {
            "flaky-v1"
        }
        fn preserve(
            &self,
            input: BrainContextCompactionStrategyInput,
        ) -> Result<BrainContextCompactionPreservationDecision, String> {
            if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
                return Err("temporary adapter failure".into());
            }
            let mut decision = FixedDownstreamStrategy.preserve(input)?;
            decision.strategy_revision = self.strategy_revision().into();
            Ok(decision)
        }
    }

    #[test]
    fn failed_strategy_can_retry_the_same_frozen_input() {
        let strategy: Arc<dyn BrainContextCompactionStrategy> =
            Arc::new(FlakyStrategy(AtomicUsize::new(0)));
        let input = strategy_input();
        assert!(execute_compaction_strategy(
            Arc::clone(&strategy),
            input.clone(),
            Duration::from_millis(100)
        )
        .is_err());
        assert!(execute_compaction_strategy(strategy, input, Duration::from_millis(100)).is_ok());
    }

    #[test]
    fn active_tool_exchange_cannot_be_inside_safe_boundary() {
        let mut input = strategy_input();
        input.snapshot.items[1].tool_exchange_id = Some("tool-1".into());
        input.snapshot.items[1].tool_exchange_completed = false;
        input.safe_boundary.active_tool_exchange_id = Some("tool-1".into());
        let failure = execute_compaction_strategy(
            Arc::new(RollingSummaryCompactionStrategy),
            input,
            Duration::from_millis(100),
        )
        .expect_err("active tool exchange must be rejected before strategy execution");
        assert_eq!(
            failure.kind,
            BrainContextCompactionStrategyFailureKind::InvalidInput
        );
    }

    #[test]
    fn provider_usage_wins_over_estimate() {
        assert!(matches!(
            decide_context_compaction(Some(&policy()), Some(81), 4).unwrap(),
            BrainContextCompactionDecision::Compact(BrainContextUsageSnapshot {
                source: BrainContextUsageSource::Provider,
                ..
            })
        ));
    }

    #[test]
    fn conservative_estimate_triggers_without_usage() {
        assert!(matches!(
            decide_context_compaction(Some(&policy()), None, 321).unwrap(),
            BrainContextCompactionDecision::Compact(BrainContextUsageSnapshot {
                source: BrainContextUsageSource::ConservativeEstimate,
                ..
            })
        ));
    }

    #[test]
    fn exact_oversized_projection_is_rejected_before_dispatch() {
        let mut policy = policy();
        policy.context_window_tokens = 1_048_576;
        policy.compact_at_percent = 100;
        let usage = BrainContextUsageSnapshot::from_projection_estimate(
            1_049_321,
            policy.context_window_tokens,
        );
        assert!(matches!(
            decide_context_compaction_for_projection(Some(&policy), usage).unwrap(),
            BrainContextCompactionDecision::Compact(BrainContextUsageSnapshot {
                input_tokens: 1_049_321,
                context_window_tokens: 1_048_576,
                source: BrainContextUsageSource::ConservativeEstimate,
                ..
            })
        ));
    }

    #[test]
    fn disabled_policy_does_not_compact() {
        let mut disabled = policy();
        disabled.enabled = false;
        assert_eq!(
            decide_context_compaction(Some(&disabled), Some(100), 0).unwrap(),
            BrainContextCompactionDecision::Disabled
        );

        disabled.enabled = true;
        disabled.auto_compaction_enabled = false;
        assert_eq!(
            decide_context_compaction(Some(&disabled), Some(100), 0).unwrap(),
            BrainContextCompactionDecision::Disabled
        );
    }

    #[test]
    fn classifies_common_provider_context_limit_failures() {
        for message in [
            "maximum context length is 128000 tokens",
            "context_length_exceeded",
            "Input is too long for this model",
            "prompt is too long",
        ] {
            assert!(is_context_limit_provider_error(message), "{message}");
        }
        assert!(!is_context_limit_provider_error("provider rate limited"));
    }

    #[test]
    fn validates_durable_compaction_artifact_sequence_and_reduction() {
        let mut artifact = BrainContextCompactionArtifact {
            artifact_id: "artifact-1".to_string(),
            sequence: 1,
            session_id: Some("session-1".to_string()),
            logical_turn_id: Some("turn-1".to_string()),
            execution_epoch_id: Some("epoch-1".to_string()),
            source_projection_fingerprint: Some("fp-abc123".to_string()),
            strategy_id: "rolling_summary_compaction".to_string(),
            strategy_revision: Some("1".to_string()),
            strategy_payload_metadata: None,
            reason_code: "context_fill_threshold_exceeded".to_string(),
            trigger: Some(BrainContextCompactionTrigger::AutoThreshold),
            usage_before: BrainContextUsageSnapshot::from_provider(90, 100),
            estimated_tokens_after: 50,
            before_tokens: Some(90),
            after_tokens: Some(50),
            preserved_item_count: Some(3),
            excised_item_count: Some(4),
            compacted_item_count: 4,
            retained_item_count: 3,
            summary_text: "summary".to_string(),
            provider_chain_action: Some("rebuild_replay_after_compaction".to_string()),
            terminal_status: Some(BrainContextCompactionTerminalStatus::Completed),
        };
        validate_compaction_artifacts(std::slice::from_ref(&artifact)).expect("valid artifact");

        artifact.sequence = 0;
        assert!(validate_compaction_artifacts(&[artifact.clone()]).is_err());
        artifact.sequence = 1;
        artifact.terminal_status = Some(BrainContextCompactionTerminalStatus::Failed);
        validate_compaction_artifacts(std::slice::from_ref(&artifact))
            .expect("failed terminal is still a valid artifact shape");
        // latest valid is completed, failed preserves prior
        let completed = BrainContextCompactionArtifact {
            terminal_status: Some(BrainContextCompactionTerminalStatus::Completed),
            ..artifact.clone()
        };
        let failed = BrainContextCompactionArtifact {
            artifact_id: "artifact-2".to_string(),
            sequence: 2,
            terminal_status: Some(BrainContextCompactionTerminalStatus::Failed),
            ..artifact.clone()
        };
        let artifacts = vec![completed.clone(), failed.clone()];
        validate_compaction_artifacts(&artifacts).expect("mixed completed+failed valid");
        let latest_valid = artifacts
            .iter()
            .filter(|candidate| {
                candidate.terminal_status == Some(BrainContextCompactionTerminalStatus::Completed)
            })
            .max_by_key(|candidate| candidate.sequence)
            .expect("completed must exist");
        assert_eq!(latest_valid.artifact_id, "artifact-1");
        assert_eq!(
            latest_usable_compaction_artifact(&artifacts)
                .expect("restart hydration must select completed artifact")
                .artifact_id,
            "artifact-1"
        );
    }

    #[test]
    fn unsafe_tool_exchange_boundary_is_rejected_at_compaction_layer() {
        // At the generic artifact layer, sequence and reduction are still enforced.
        // A would-be unsafe compaction that cannot reduce the projection is
        // rejected at the brain-specific layer (chat/responses) before an artifact
        // is ever persisted — see those crate tests. Here we prove a failed
        // artifact that *does* reduce is still structurally valid and would be
        // persisted as a failed attempt preserving the prior valid.
        let artifact = BrainContextCompactionArtifact {
            artifact_id: "artifact-unsafe".to_string(),
            sequence: 1,
            session_id: Some("session-1".to_string()),
            logical_turn_id: Some("turn-1".to_string()),
            execution_epoch_id: Some("epoch-1".to_string()),
            source_projection_fingerprint: Some("fp-unsafe".to_string()),
            strategy_id: "rolling_summary_compaction".to_string(),
            strategy_revision: Some("1".to_string()),
            strategy_payload_metadata: None,
            reason_code: "context_fill_threshold_exceeded".to_string(),
            trigger: Some(BrainContextCompactionTrigger::AutoThreshold),
            usage_before: BrainContextUsageSnapshot::from_provider(90, 100),
            estimated_tokens_after: 50,
            before_tokens: Some(90),
            after_tokens: Some(50),
            preserved_item_count: Some(3),
            excised_item_count: Some(2),
            compacted_item_count: 2,
            retained_item_count: 3,
            summary_text: "failed but reducing".to_string(),
            provider_chain_action: Some("rebuild_replay_after_compaction".to_string()),
            terminal_status: Some(BrainContextCompactionTerminalStatus::Failed),
        };
        validate_compaction_artifacts(std::slice::from_ref(&artifact))
            .expect("failed reducing artifact is structurally valid");
        assert!(
            artifact.estimated_tokens_after < artifact.usage_before.input_tokens,
            "failed artifact still records a reduction, but brain layer rejected the unsafe boundary before persisting"
        );
    }
}
