use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
    if usage.fill_percent >= policy.compact_at_percent {
        Ok(BrainContextCompactionDecision::Compact(usage))
    } else {
        Ok(BrainContextCompactionDecision::BelowThreshold(usage))
    }
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

#[cfg(test)]
mod tests {
    use super::*;

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
