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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BrainContextCompactionArtifact {
    pub sequence: u64,
    pub strategy_id: String,
    pub reason_code: String,
    pub usage_before: BrainContextUsageSnapshot,
    pub estimated_tokens_after: u64,
    pub compacted_item_count: u64,
    pub retained_item_count: u64,
    pub summary_text: String,
    pub provider_chain_action: Option<String>,
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
}
