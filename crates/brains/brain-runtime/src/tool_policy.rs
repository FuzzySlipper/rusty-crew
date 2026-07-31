use crate::BufferedNeutralToolOutput;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BufferedBrainHostToolStatus {
    Succeeded,
    Denied,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BufferedBrainHostTurnDisposition {
    CompleteTurn,
    SuspendExternal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BufferedBrainHostToolResult {
    pub status: BufferedBrainHostToolStatus,
    pub output_text: String,
    pub reason_code: Option<String>,
    pub retryable: bool,
    pub action: Option<String>,
    pub summary: Option<String>,
    pub debug_detail_id: Option<String>,
    pub state_fingerprint: Option<String>,
    pub turn_disposition: Option<BufferedBrainHostTurnDisposition>,
}

impl BufferedBrainHostToolResult {
    pub fn succeeded(output_text: impl Into<String>) -> Self {
        Self {
            status: BufferedBrainHostToolStatus::Succeeded,
            output_text: output_text.into(),
            reason_code: None,
            retryable: false,
            action: None,
            summary: None,
            debug_detail_id: None,
            state_fingerprint: None,
            turn_disposition: None,
        }
    }

    pub fn denied(output_text: impl Into<String>, reason_code: impl Into<String>) -> Self {
        Self {
            status: BufferedBrainHostToolStatus::Denied,
            output_text: output_text.into(),
            reason_code: Some(reason_code.into()),
            retryable: false,
            action: Some("denied".to_string()),
            summary: None,
            debug_detail_id: None,
            state_fingerprint: None,
            turn_disposition: None,
        }
    }

    pub fn failed(
        output_text: impl Into<String>,
        reason_code: impl Into<String>,
        retryable: bool,
    ) -> Self {
        Self {
            status: BufferedBrainHostToolStatus::Failed,
            output_text: output_text.into(),
            reason_code: Some(reason_code.into()),
            retryable,
            action: Some("failed".to_string()),
            summary: None,
            debug_detail_id: None,
            state_fingerprint: None,
            turn_disposition: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BufferedBrainToolFailure {
    pub tool_name: String,
    pub reason_code: String,
    pub retryable: bool,
    pub action: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BufferedBrainToolRecoveryGuidance {
    pub reason_code: String,
    pub reason: String,
    pub total_failures: usize,
    pub recent_failures: Vec<BufferedBrainToolFailure>,
    pub guidance: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BufferedBrainToolOutputTruncation {
    pub original_chars: usize,
    pub original_bytes: usize,
    pub output_chars: usize,
    pub output_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BufferedBrainToolPolicyDecision {
    pub provider_output: BufferedNeutralToolOutput,
    pub failure: Option<BufferedBrainToolFailure>,
    pub recovery_guidance: Option<BufferedBrainToolRecoveryGuidance>,
    pub truncation: Option<BufferedBrainToolOutputTruncation>,
    pub debug_detail_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BufferedBrainToolFailurePolicyConfig {
    pub repeated_failure_guidance_threshold: usize,
    pub consecutive_failure_guidance_threshold: usize,
    pub recent_failure_limit: usize,
    pub max_output_chars: usize,
}

impl Default for BufferedBrainToolFailurePolicyConfig {
    fn default() -> Self {
        Self {
            repeated_failure_guidance_threshold: 2,
            consecutive_failure_guidance_threshold: 3,
            recent_failure_limit: 5,
            max_output_chars: 20_000,
        }
    }
}

#[derive(Debug)]
pub struct BufferedBrainToolFailurePolicy {
    config: BufferedBrainToolFailurePolicyConfig,
    total_failures: usize,
    consecutive_failures: usize,
    failures_by_key: HashMap<String, usize>,
    recent_failures: VecDeque<BufferedBrainToolFailure>,
}

impl Default for BufferedBrainToolFailurePolicy {
    fn default() -> Self {
        Self::new(BufferedBrainToolFailurePolicyConfig::default())
    }
}

impl BufferedBrainToolFailurePolicy {
    pub fn new(config: BufferedBrainToolFailurePolicyConfig) -> Self {
        assert!(config.repeated_failure_guidance_threshold > 0);
        assert!(config.consecutive_failure_guidance_threshold > 0);
        assert!(config.recent_failure_limit > 0);
        assert!(config.max_output_chars > 0);
        Self {
            config,
            total_failures: 0,
            consecutive_failures: 0,
            failures_by_key: HashMap::new(),
            recent_failures: VecDeque::new(),
        }
    }

    pub fn total_failures(&self) -> usize {
        self.total_failures
    }

    pub fn consecutive_failures(&self) -> usize {
        self.consecutive_failures
    }

    pub fn record_result(
        &mut self,
        tool_name: &str,
        result: &BufferedBrainHostToolResult,
        max_output_bytes: usize,
    ) -> BufferedBrainToolPolicyDecision {
        let failure = self.failure_from_result(tool_name, result);
        let recovery_guidance = self.record_failure(failure.as_ref());
        let (output, truncation) = bound_tool_output(
            &result.output_text,
            self.config.max_output_chars,
            max_output_bytes,
        );
        BufferedBrainToolPolicyDecision {
            provider_output: BufferedNeutralToolOutput {
                output,
                is_error: failure.is_some(),
                state_fingerprint: result.state_fingerprint.clone().unwrap_or_default(),
            },
            failure,
            recovery_guidance,
            truncation,
            debug_detail_id: result.debug_detail_id.clone(),
        }
    }

    fn failure_from_result(
        &self,
        tool_name: &str,
        result: &BufferedBrainHostToolResult,
    ) -> Option<BufferedBrainToolFailure> {
        if result.status == BufferedBrainHostToolStatus::Succeeded {
            return None;
        }
        let default_reason = match result.status {
            BufferedBrainHostToolStatus::Succeeded => unreachable!(),
            BufferedBrainHostToolStatus::Denied => "tool_denied",
            BufferedBrainHostToolStatus::Failed => "tool_reported_unsuccessful_result",
        };
        let reason_code = result.reason_code.as_deref().unwrap_or(default_reason);
        let detail = result.summary.clone().unwrap_or_else(|| {
            [
                format!("{tool_name} returned an unsuccessful result"),
                result
                    .action
                    .as_ref()
                    .map(|action| format!("action={action}"))
                    .unwrap_or_default(),
                format!("reason={reason_code}"),
                format!("retryable={}", result.retryable),
            ]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
        });
        Some(BufferedBrainToolFailure {
            tool_name: tool_name.to_string(),
            reason_code: reason_code.to_string(),
            retryable: result.retryable,
            action: result.action.clone(),
            detail,
        })
    }

    fn record_failure(
        &mut self,
        failure: Option<&BufferedBrainToolFailure>,
    ) -> Option<BufferedBrainToolRecoveryGuidance> {
        let Some(failure) = failure else {
            self.consecutive_failures = 0;
            return None;
        };
        self.total_failures += 1;
        self.consecutive_failures += 1;
        self.recent_failures.push_back(failure.clone());
        while self.recent_failures.len() > self.config.recent_failure_limit {
            self.recent_failures.pop_front();
        }
        let key = format!("{}:{}", failure.tool_name, failure.reason_code);
        let key_count = self.failures_by_key.entry(key).or_default();
        *key_count += 1;

        if *key_count >= self.config.repeated_failure_guidance_threshold {
            return Some(self.recovery_guidance(
                "repeated_tool_failure_guidance",
                format!(
                    "repeated {} failure ({})",
                    failure.tool_name, failure.reason_code
                ),
            ));
        }
        if self.consecutive_failures >= self.config.consecutive_failure_guidance_threshold {
            return Some(self.recovery_guidance(
                "consecutive_tool_failure_guidance",
                format!("{} consecutive tool failures", self.consecutive_failures),
            ));
        }
        None
    }

    fn recovery_guidance(
        &self,
        reason_code: &str,
        reason: String,
    ) -> BufferedBrainToolRecoveryGuidance {
        let recent_failures = self.recent_failures.iter().cloned().collect::<Vec<_>>();
        let recent = recent_failures
            .iter()
            .map(|failure| {
                format!(
                    "{}: {} (retryable={})",
                    failure.tool_name, failure.reason_code, failure.retryable
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        let guidance = [
            Some(format!("Crew observed {reason}.")),
            Some(format!(
                "Tool failure count this turn: {}.",
                self.total_failures
            )),
            (!recent.is_empty()).then(|| format!("Recent tool failures: {recent}.")),
            Some(
                "Do not repeat an unchanged call. Correct the arguments, choose an alternative, or explain the unavailable operation to the user. Continue the turn with the best useful result available."
                    .to_string(),
            ),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n");
        BufferedBrainToolRecoveryGuidance {
            reason_code: reason_code.to_string(),
            reason,
            total_failures: self.total_failures,
            recent_failures,
            guidance,
        }
    }
}

fn bound_tool_output(
    input: &str,
    max_chars: usize,
    max_bytes: usize,
) -> (String, Option<BufferedBrainToolOutputTruncation>) {
    let original_chars = input.chars().count();
    let original_bytes = input.len();
    if original_chars <= max_chars && original_bytes <= max_bytes {
        return (input.to_string(), None);
    }

    let mut kept = String::new();
    let mut kept_chars = 0;
    for character in input.chars() {
        if kept_chars >= max_chars || kept.len() + character.len_utf8() > max_bytes {
            break;
        }
        kept.push(character);
        kept_chars += 1;
    }
    let omitted_chars = original_chars.saturating_sub(kept_chars);
    let suffix = format!("\n[truncated {omitted_chars} chars]");
    while !kept.is_empty() && kept.len() + suffix.len() > max_bytes {
        kept.pop();
    }
    if suffix.len() <= max_bytes {
        kept.push_str(&suffix);
    }
    let truncation = BufferedBrainToolOutputTruncation {
        original_chars,
        original_bytes,
        output_chars: kept.chars().count(),
        output_bytes: kept.len(),
    };
    (kept, Some(truncation))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_nonretryable_and_denied_failures_do_not_stop() {
        let mut policy = BufferedBrainToolFailurePolicy::default();
        let missing = policy.record_result(
            "memory_read",
            &BufferedBrainHostToolResult::failed("missing ref", "missing_memory_ref", false),
            64 * 1_024,
        );
        assert!(missing.failure.is_some());
        assert!(missing.recovery_guidance.is_none());

        let manual_review = policy.record_result(
            "memory_store",
            &BufferedBrainHostToolResult::denied(
                "manual review required",
                "memory_manual_review_required",
            ),
            64 * 1_024,
        );
        assert!(manual_review.failure.is_some());
        assert!(manual_review.recovery_guidance.is_none());
    }

    #[test]
    fn repeated_failure_key_adds_recovery_guidance_without_stopping() {
        let mut policy = BufferedBrainToolFailurePolicy::default();
        let unavailable =
            BufferedBrainHostToolResult::failed("not available", "tool_unavailable", false);
        assert!(policy
            .record_result("den_get_document", &unavailable, 64 * 1_024)
            .recovery_guidance
            .is_none());
        let second = policy.record_result("den_get_document", &unavailable, 64 * 1_024);
        assert_eq!(
            second
                .recovery_guidance
                .as_ref()
                .map(|guidance| guidance.reason_code.as_str()),
            Some("repeated_tool_failure_guidance")
        );
        assert!(second
            .recovery_guidance
            .expect("recovery guidance")
            .guidance
            .contains("den_get_document: tool_unavailable"));
        assert!(second.provider_output.is_error);
        assert_eq!(second.provider_output.output, "not available");
    }

    #[test]
    fn three_distinct_consecutive_failures_add_recovery_guidance() {
        let mut policy = BufferedBrainToolFailurePolicy::default();
        for (index, reason) in ["bad_args", "tool_exception", "dependency_down"]
            .into_iter()
            .enumerate()
        {
            let decision = policy.record_result(
                &format!("tool-{index}"),
                &BufferedBrainHostToolResult::failed("failed", reason, true),
                64 * 1_024,
            );
            if index < 2 {
                assert!(decision.recovery_guidance.is_none());
            } else {
                assert_eq!(
                    decision
                        .recovery_guidance
                        .map(|guidance| guidance.reason_code),
                    Some("consecutive_tool_failure_guidance".to_string())
                );
            }
        }
    }

    #[test]
    fn success_resets_only_the_consecutive_counter() {
        let mut policy = BufferedBrainToolFailurePolicy::default();
        let failure = BufferedBrainHostToolResult::failed("failed", "tool_exception", true);
        policy.record_result("first", &failure, 64 * 1_024);
        assert_eq!(policy.consecutive_failures(), 1);
        policy.record_result(
            "second",
            &BufferedBrainHostToolResult::succeeded("ok"),
            64 * 1_024,
        );
        assert_eq!(policy.consecutive_failures(), 0);
        assert_eq!(policy.total_failures(), 1);
    }

    #[test]
    fn missing_args_unavailable_and_exception_preserve_reason_and_retryability() {
        let mut policy = BufferedBrainToolFailurePolicy::default();
        for (tool, reason, retryable) in [
            ("bad_args", "tool_preparation_failed", false),
            ("missing", "tool_unavailable", false),
            ("throws", "tool_exception", true),
        ] {
            let decision = policy.record_result(
                tool,
                &BufferedBrainHostToolResult::failed("failed", reason, retryable),
                64 * 1_024,
            );
            let failure = decision.failure.expect("failure");
            assert_eq!(failure.reason_code, reason);
            assert_eq!(failure.retryable, retryable);
        }
    }

    #[test]
    fn large_unicode_output_is_bounded_by_chars_and_bytes() {
        let mut policy =
            BufferedBrainToolFailurePolicy::new(BufferedBrainToolFailurePolicyConfig {
                max_output_chars: 10,
                ..BufferedBrainToolFailurePolicyConfig::default()
            });
        let decision = policy.record_result(
            "read_file",
            &BufferedBrainHostToolResult::succeeded("abcdefghijklmno"),
            64,
        );
        assert!(decision.provider_output.output.starts_with("abcdefghij"));
        assert!(decision
            .provider_output
            .output
            .contains("[truncated 5 chars]"));
        assert!(decision.truncation.is_some());

        let unicode = policy.record_result(
            "read_file",
            &BufferedBrainHostToolResult::succeeded("éééééééééé"),
            12,
        );
        assert!(unicode.provider_output.output.len() <= 12);
        assert!(unicode.truncation.is_some());
    }

    #[test]
    fn repeated_failure_guidance_does_not_mutate_provider_output() {
        let mut policy =
            BufferedBrainToolFailurePolicy::new(BufferedBrainToolFailurePolicyConfig {
                max_output_chars: 1_024,
                ..BufferedBrainToolFailurePolicyConfig::default()
            });
        let failure = BufferedBrainHostToolResult::failed(
            "x".repeat(2_000),
            "tool_reported_unsuccessful_result",
            true,
        );
        policy.record_result("patch", &failure, 4_096);
        let second = policy.record_result("patch", &failure, 4_096);
        assert!(second.truncation.is_some());
        assert!(second
            .provider_output
            .output
            .starts_with(&"x".repeat(1_024)));
        assert!(!second.provider_output.output.contains("recovery guidance"));
        assert!(second
            .recovery_guidance
            .expect("recovery guidance metadata")
            .guidance
            .contains("Do not repeat an unchanged call"));
    }
}
