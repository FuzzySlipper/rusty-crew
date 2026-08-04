use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const CONTEXT_ACCOUNTING_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContextMeasurementSource {
    Provider,
    Tokenizer,
    SerializedEstimate,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContextMeasurementQuality {
    Exact,
    Approximate,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContextTokenMeasurement {
    pub tokens: Option<u64>,
    pub source: ContextMeasurementSource,
    pub quality: ContextMeasurementQuality,
    pub estimator_id: Option<String>,
    pub measured_at: Option<String>,
}

impl ContextTokenMeasurement {
    pub fn unavailable() -> Self {
        Self {
            tokens: None,
            source: ContextMeasurementSource::Unavailable,
            quality: ContextMeasurementQuality::Unavailable,
            estimator_id: None,
            measured_at: None,
        }
    }

    pub fn provider(tokens: u64, measured_at: Option<String>) -> Self {
        Self {
            tokens: Some(tokens),
            source: ContextMeasurementSource::Provider,
            quality: ContextMeasurementQuality::Exact,
            estimator_id: None,
            measured_at,
        }
    }

    pub fn estimate(
        tokens: u64,
        source: ContextMeasurementSource,
        estimator_id: String,
        measured_at: Option<String>,
    ) -> Result<Self, String> {
        if !matches!(
            source,
            ContextMeasurementSource::Tokenizer | ContextMeasurementSource::SerializedEstimate
        ) {
            return Err(
                "estimated context tokens require tokenizer or serialized_estimate source"
                    .to_string(),
            );
        }
        if estimator_id.trim().is_empty() {
            return Err("estimated context tokens require a non-empty estimator_id".to_string());
        }
        Ok(Self {
            tokens: Some(tokens),
            source,
            quality: ContextMeasurementQuality::Approximate,
            estimator_id: Some(estimator_id),
            measured_at,
        })
    }

    pub fn validate(&self, field: &str) -> Result<(), String> {
        match (self.tokens, self.source, self.quality) {
            (
                None,
                ContextMeasurementSource::Unavailable,
                ContextMeasurementQuality::Unavailable,
            ) => {
                if self.estimator_id.is_some() {
                    return Err(format!(
                        "{field}.estimator_id must be absent when unavailable"
                    ));
                }
            }
            (None, _, _) => {
                return Err(format!(
                    "{field} must use unavailable source and quality when tokens are unknown"
                ));
            }
            (Some(_), ContextMeasurementSource::Unavailable, _)
            | (Some(_), _, ContextMeasurementQuality::Unavailable) => {
                return Err(format!("{field} cannot mark known tokens as unavailable"));
            }
            (Some(_), ContextMeasurementSource::Provider, _) => {
                if self.estimator_id.is_some() {
                    return Err(format!(
                        "{field}.estimator_id must be absent for provider measurements"
                    ));
                }
            }
            (Some(_), ContextMeasurementSource::Tokenizer, _)
            | (Some(_), ContextMeasurementSource::SerializedEstimate, _) => {
                if self
                    .estimator_id
                    .as_deref()
                    .is_none_or(|value| value.trim().is_empty())
                {
                    return Err(format!(
                        "{field}.estimator_id is required for estimated measurements"
                    ));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContextSizeMeasurement {
    pub bytes: Option<u64>,
    pub source: ContextMeasurementSource,
    pub quality: ContextMeasurementQuality,
    pub estimator_id: Option<String>,
    pub measured_at: Option<String>,
}

impl ContextSizeMeasurement {
    pub fn unavailable() -> Self {
        Self {
            bytes: None,
            source: ContextMeasurementSource::Unavailable,
            quality: ContextMeasurementQuality::Unavailable,
            estimator_id: None,
            measured_at: None,
        }
    }

    pub fn measured(
        bytes: u64,
        source: ContextMeasurementSource,
        quality: ContextMeasurementQuality,
        estimator_id: Option<String>,
        measured_at: Option<String>,
    ) -> Self {
        Self {
            bytes: Some(bytes),
            source,
            quality,
            estimator_id,
            measured_at,
        }
    }

    pub fn validate(&self, field: &str) -> Result<(), String> {
        match (self.bytes, self.source, self.quality) {
            (
                None,
                ContextMeasurementSource::Unavailable,
                ContextMeasurementQuality::Unavailable,
            ) => {
                if self.estimator_id.is_some() {
                    return Err(format!(
                        "{field}.estimator_id must be absent when unavailable"
                    ));
                }
            }
            (None, _, _) => {
                return Err(format!(
                    "{field} must use unavailable source and quality when bytes are unknown"
                ));
            }
            (Some(_), ContextMeasurementSource::Unavailable, _)
            | (Some(_), _, ContextMeasurementQuality::Unavailable) => {
                return Err(format!("{field} cannot mark known bytes as unavailable"));
            }
            (Some(_), ContextMeasurementSource::Provider, _) => {
                if self.estimator_id.is_some() {
                    return Err(format!(
                        "{field}.estimator_id must be absent for provider measurements"
                    ));
                }
            }
            (Some(_), ContextMeasurementSource::Tokenizer, _)
            | (Some(_), ContextMeasurementSource::SerializedEstimate, _) => {
                if self
                    .estimator_id
                    .as_deref()
                    .is_none_or(|value| value.trim().is_empty())
                {
                    return Err(format!(
                        "{field}.estimator_id is required for estimated measurements"
                    ));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContextProviderProtocol {
    ChatCompletions,
    Responses,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContextProviderDescriptor {
    pub protocol: ContextProviderProtocol,
    pub provider_alias: Option<String>,
    pub model_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContextProjectionSegment {
    pub name: String,
    pub included: bool,
    pub tokens: ContextTokenMeasurement,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContextProtocolProjection {
    ChatCompletions {
        message_count: Option<u64>,
        tool_schema_count: Option<u64>,
        reasoning_policy: Option<String>,
    },
    Responses {
        chain_strategy: Option<String>,
        replay_item_count: Option<u64>,
        response_lineage_fingerprint: Option<String>,
    },
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContextPromptProjection {
    pub input_tokens: ContextTokenMeasurement,
    pub context_window_tokens: ContextTokenMeasurement,
    pub protocol_projection: ContextProtocolProjection,
    pub segments: Vec<ContextProjectionSegment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContextReservedOutput {
    pub response_tokens: ContextTokenMeasurement,
    pub safety_margin_tokens: ContextTokenMeasurement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContextAdmissionState {
    Admitted,
    NearThreshold,
    RequiresCompaction,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContextAdmission {
    pub state: ContextAdmissionState,
    pub fill_percent: Option<u32>,
    pub usable_input_tokens: ContextTokenMeasurement,
    pub compact_at_percent: Option<u32>,
    pub max_context_percent_for_wake: Option<u32>,
    pub reason_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContextTokenUsageTotals {
    pub input_tokens: ContextTokenMeasurement,
    pub cached_input_tokens: ContextTokenMeasurement,
    pub cache_write_input_tokens: ContextTokenMeasurement,
    pub output_tokens: ContextTokenMeasurement,
    pub reasoning_tokens: ContextTokenMeasurement,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContextProviderUsage {
    pub current_request: ContextTokenUsageTotals,
    pub logical_wake: ContextTokenUsageTotals,
    pub request_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContextDurableTranscript {
    pub event_count: Option<u64>,
    pub message_count: Option<u64>,
    pub serialized_size: ContextSizeMeasurement,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContextProviderState {
    pub state_kind: Option<String>,
    pub item_count: Option<u64>,
    pub serialized_size: ContextSizeMeasurement,
    pub lineage_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContextCompactionPhase {
    Idle,
    Started,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContextCompactionProjection {
    pub strategy_id: Option<String>,
    pub strategy_revision: Option<String>,
    pub enabled: bool,
    pub auto_compaction_enabled: bool,
    pub phase: ContextCompactionPhase,
    pub last_artifact_id: Option<String>,
    pub last_sequence: Option<u64>,
    pub trigger_reason: Option<String>,
    pub input_tokens_before: ContextTokenMeasurement,
    pub input_tokens_after: ContextTokenMeasurement,
    pub compacted_item_count: Option<u64>,
    pub retained_item_count: Option<u64>,
    pub provider_chain_action: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContextDiagnosticSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContextAccountingDiagnostic {
    pub severity: ContextDiagnosticSeverity,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContextAccountingSnapshot {
    pub schema_version: u32,
    pub session_id: Option<String>,
    pub wake_id: Option<String>,
    pub logical_turn_id: Option<String>,
    pub execution_epoch_id: Option<String>,
    pub measured_at: Option<String>,
    pub provider: ContextProviderDescriptor,
    pub prompt_projection: ContextPromptProjection,
    pub reserved_output: ContextReservedOutput,
    pub admission: ContextAdmission,
    pub provider_usage: ContextProviderUsage,
    pub durable_transcript: ContextDurableTranscript,
    pub provider_state: ContextProviderState,
    pub compaction: ContextCompactionProjection,
    pub diagnostics: Vec<ContextAccountingDiagnostic>,
}

impl ContextAccountingSnapshot {
    pub fn unavailable(provider: ContextProviderDescriptor) -> Self {
        Self {
            schema_version: CONTEXT_ACCOUNTING_SCHEMA_VERSION,
            session_id: None,
            wake_id: None,
            logical_turn_id: None,
            execution_epoch_id: None,
            measured_at: None,
            provider,
            prompt_projection: ContextPromptProjection {
                input_tokens: ContextTokenMeasurement::unavailable(),
                context_window_tokens: ContextTokenMeasurement::unavailable(),
                protocol_projection: ContextProtocolProjection::Unknown,
                segments: Vec::new(),
            },
            reserved_output: ContextReservedOutput {
                response_tokens: ContextTokenMeasurement::unavailable(),
                safety_margin_tokens: ContextTokenMeasurement::unavailable(),
            },
            admission: ContextAdmission {
                state: ContextAdmissionState::Unavailable,
                fill_percent: None,
                usable_input_tokens: ContextTokenMeasurement::unavailable(),
                compact_at_percent: None,
                max_context_percent_for_wake: None,
                reason_code: Some("context_window_unavailable".to_string()),
            },
            provider_usage: ContextProviderUsage {
                current_request: ContextTokenUsageTotals::unavailable(),
                logical_wake: ContextTokenUsageTotals::unavailable(),
                request_count: 0,
            },
            durable_transcript: ContextDurableTranscript {
                event_count: None,
                message_count: None,
                serialized_size: ContextSizeMeasurement::unavailable(),
            },
            provider_state: ContextProviderState {
                state_kind: None,
                item_count: None,
                serialized_size: ContextSizeMeasurement::unavailable(),
                lineage_fingerprint: None,
            },
            compaction: ContextCompactionProjection {
                strategy_id: None,
                strategy_revision: None,
                enabled: false,
                auto_compaction_enabled: false,
                phase: ContextCompactionPhase::Idle,
                last_artifact_id: None,
                last_sequence: None,
                trigger_reason: None,
                input_tokens_before: ContextTokenMeasurement::unavailable(),
                input_tokens_after: ContextTokenMeasurement::unavailable(),
                compacted_item_count: None,
                retained_item_count: None,
                provider_chain_action: None,
            },
            diagnostics: vec![ContextAccountingDiagnostic {
                severity: ContextDiagnosticSeverity::Info,
                code: "context_accounting_unavailable".to_string(),
                message: "Provider projection accounting is not available yet.".to_string(),
            }],
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != CONTEXT_ACCOUNTING_SCHEMA_VERSION {
            return Err(format!(
                "unsupported context accounting schema version {}",
                self.schema_version
            ));
        }
        self.prompt_projection
            .input_tokens
            .validate("prompt_projection.input_tokens")?;
        self.prompt_projection
            .context_window_tokens
            .validate("prompt_projection.context_window_tokens")?;
        for (index, segment) in self.prompt_projection.segments.iter().enumerate() {
            if segment.name.trim().is_empty() {
                return Err(format!(
                    "prompt_projection.segments[{index}].name must not be empty"
                ));
            }
            segment
                .tokens
                .validate(&format!("prompt_projection.segments[{index}].tokens"))?;
        }
        self.reserved_output
            .response_tokens
            .validate("reserved_output.response_tokens")?;
        self.reserved_output
            .safety_margin_tokens
            .validate("reserved_output.safety_margin_tokens")?;
        self.admission
            .usable_input_tokens
            .validate("admission.usable_input_tokens")?;
        self.provider_usage
            .current_request
            .validate("provider_usage.current_request")?;
        self.provider_usage
            .logical_wake
            .validate("provider_usage.logical_wake")?;
        self.durable_transcript
            .serialized_size
            .validate("durable_transcript.serialized_size")?;
        self.provider_state
            .serialized_size
            .validate("provider_state.serialized_size")?;
        self.compaction
            .input_tokens_before
            .validate("compaction.input_tokens_before")?;
        self.compaction
            .input_tokens_after
            .validate("compaction.input_tokens_after")?;
        if let Some(percent) = self.admission.fill_percent {
            if percent > 100 {
                return Err("admission.fill_percent must be between 0 and 100".to_string());
            }
        }
        for (name, percent) in [
            ("compact_at_percent", self.admission.compact_at_percent),
            (
                "max_context_percent_for_wake",
                self.admission.max_context_percent_for_wake,
            ),
        ] {
            if percent.is_some_and(|value| !(1..=100).contains(&value)) {
                return Err(format!("admission.{name} must be between 1 and 100"));
            }
        }
        Ok(())
    }
}

impl ContextTokenUsageTotals {
    pub fn unavailable() -> Self {
        Self {
            input_tokens: ContextTokenMeasurement::unavailable(),
            cached_input_tokens: ContextTokenMeasurement::unavailable(),
            cache_write_input_tokens: ContextTokenMeasurement::unavailable(),
            output_tokens: ContextTokenMeasurement::unavailable(),
            reasoning_tokens: ContextTokenMeasurement::unavailable(),
        }
    }

    pub fn provider(
        input_tokens: u64,
        cached_input_tokens: u64,
        cache_write_input_tokens: u64,
        output_tokens: u64,
        reasoning_tokens: u64,
        measured_at: Option<String>,
    ) -> Self {
        Self {
            input_tokens: ContextTokenMeasurement::provider(input_tokens, measured_at.clone()),
            cached_input_tokens: ContextTokenMeasurement::provider(
                cached_input_tokens,
                measured_at.clone(),
            ),
            cache_write_input_tokens: ContextTokenMeasurement::provider(
                cache_write_input_tokens,
                measured_at.clone(),
            ),
            output_tokens: ContextTokenMeasurement::provider(output_tokens, measured_at.clone()),
            reasoning_tokens: ContextTokenMeasurement::provider(reasoning_tokens, measured_at),
        }
    }

    fn validate(&self, prefix: &str) -> Result<(), String> {
        self.input_tokens
            .validate(&format!("{prefix}.input_tokens"))?;
        self.cached_input_tokens
            .validate(&format!("{prefix}.cached_input_tokens"))?;
        self.cache_write_input_tokens
            .validate(&format!("{prefix}.cache_write_input_tokens"))?;
        self.output_tokens
            .validate(&format!("{prefix}.output_tokens"))?;
        self.reasoning_tokens
            .validate(&format!("{prefix}.reasoning_tokens"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> ContextAccountingSnapshot {
        let provider = ContextTokenMeasurement::provider(100, Some("2026-08-04T00:00:00Z".into()));
        let capacity = ContextTokenMeasurement::provider(1_000, None);
        let unavailable = ContextTokenMeasurement::unavailable();
        ContextAccountingSnapshot {
            schema_version: CONTEXT_ACCOUNTING_SCHEMA_VERSION,
            session_id: Some("session".into()),
            wake_id: Some("wake".into()),
            logical_turn_id: Some("turn".into()),
            execution_epoch_id: Some("epoch".into()),
            measured_at: Some("2026-08-04T00:00:00Z".into()),
            provider: ContextProviderDescriptor {
                protocol: ContextProviderProtocol::ChatCompletions,
                provider_alias: Some("provider".into()),
                model_id: Some("model".into()),
            },
            prompt_projection: ContextPromptProjection {
                input_tokens: provider.clone(),
                context_window_tokens: capacity,
                protocol_projection: ContextProtocolProjection::ChatCompletions {
                    message_count: Some(2),
                    tool_schema_count: Some(1),
                    reasoning_policy: Some("preserve".into()),
                },
                segments: vec![ContextProjectionSegment {
                    name: "history".into(),
                    included: true,
                    tokens: provider.clone(),
                }],
            },
            reserved_output: ContextReservedOutput {
                response_tokens: ContextTokenMeasurement::provider(100, None),
                safety_margin_tokens: ContextTokenMeasurement::provider(20, None),
            },
            admission: ContextAdmission {
                state: ContextAdmissionState::Admitted,
                fill_percent: Some(10),
                usable_input_tokens: ContextTokenMeasurement::provider(880, None),
                compact_at_percent: Some(80),
                max_context_percent_for_wake: Some(95),
                reason_code: None,
            },
            provider_usage: ContextProviderUsage {
                current_request: ContextTokenUsageTotals {
                    input_tokens: provider.clone(),
                    cached_input_tokens: ContextTokenMeasurement::provider(40, None),
                    cache_write_input_tokens: unavailable.clone(),
                    output_tokens: ContextTokenMeasurement::provider(20, None),
                    reasoning_tokens: unavailable.clone(),
                },
                logical_wake: ContextTokenUsageTotals {
                    input_tokens: ContextTokenMeasurement::provider(220, None),
                    cached_input_tokens: ContextTokenMeasurement::provider(40, None),
                    cache_write_input_tokens: unavailable.clone(),
                    output_tokens: ContextTokenMeasurement::provider(20, None),
                    reasoning_tokens: unavailable.clone(),
                },
                request_count: 1,
            },
            durable_transcript: ContextDurableTranscript {
                event_count: Some(4),
                message_count: Some(2),
                serialized_size: ContextSizeMeasurement::measured(
                    400,
                    ContextMeasurementSource::SerializedEstimate,
                    ContextMeasurementQuality::Approximate,
                    Some("json_bytes_v1".into()),
                    None,
                ),
            },
            provider_state: ContextProviderState {
                state_kind: Some("chat_completions_messages".into()),
                item_count: Some(2),
                serialized_size: ContextSizeMeasurement::unavailable(),
                lineage_fingerprint: Some("fingerprint".into()),
            },
            compaction: ContextCompactionProjection {
                strategy_id: Some("rolling_summary_compaction".into()),
                strategy_revision: Some("1".into()),
                enabled: true,
                auto_compaction_enabled: true,
                phase: ContextCompactionPhase::Idle,
                last_artifact_id: None,
                last_sequence: None,
                trigger_reason: None,
                input_tokens_before: unavailable.clone(),
                input_tokens_after: unavailable,
                compacted_item_count: None,
                retained_item_count: None,
                provider_chain_action: None,
            },
            diagnostics: vec![ContextAccountingDiagnostic {
                severity: ContextDiagnosticSeverity::Info,
                code: "fixture".into(),
                message: "no raw provider payload".into(),
            }],
        }
    }

    #[test]
    fn known_zero_is_distinct_from_unavailable() {
        let zero = ContextTokenMeasurement::provider(0, None);
        assert_eq!(zero.tokens, Some(0));
        assert_eq!(ContextTokenMeasurement::unavailable().tokens, None);
        assert!(zero.validate("zero").is_ok());
        assert!(ContextTokenMeasurement::unavailable()
            .validate("unknown")
            .is_ok());
    }

    #[test]
    fn rejects_measurement_that_hides_unknown_as_exact() {
        let invalid = ContextTokenMeasurement {
            tokens: None,
            source: ContextMeasurementSource::Provider,
            quality: ContextMeasurementQuality::Exact,
            estimator_id: None,
            measured_at: None,
        };
        let error = invalid.validate("input").expect_err("invalid measurement");
        assert!(error.contains("unknown"));
    }

    #[test]
    fn validates_full_snapshot_and_keeps_usage_dimensions_separate() {
        let value = snapshot();
        value.validate().expect("valid fixture");
        assert_ne!(
            value.prompt_projection.input_tokens.tokens,
            value.provider_usage.logical_wake.input_tokens.tokens
        );
        let json = serde_json::to_value(&value).expect("snapshot json");
        assert_eq!(json["schemaVersion"], CONTEXT_ACCOUNTING_SCHEMA_VERSION);
        assert_eq!(json["promptProjection"]["inputTokens"]["tokens"], 100);
        assert_eq!(
            json["providerUsage"]["logicalWake"]["inputTokens"]["tokens"],
            220
        );
        assert_eq!(json["providerState"]["lineageFingerprint"], "fingerprint");
        assert!(json.get("authorization").is_none());
    }

    #[test]
    fn estimate_requires_an_estimator_identity() {
        let error = ContextTokenMeasurement::estimate(
            10,
            ContextMeasurementSource::SerializedEstimate,
            " ".into(),
            None,
        )
        .expect_err("missing estimator");
        assert!(error.contains("estimator_id"));
    }
}
