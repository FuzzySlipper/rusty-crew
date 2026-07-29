//! Provider-neutral no-progress classification for durable brain turns.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BrainProgressResultClass {
    Succeeded,
    Failed,
    MalformedProviderOutput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BrainProgressSample {
    pub intent_fingerprint: String,
    pub result_fingerprint: String,
    pub state_fingerprint: String,
    pub assistant_progress_fingerprint: String,
    pub result_class: BrainProgressResultClass,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BrainNoProgressState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_sample: Option<BrainProgressSample>,
    #[serde(default)]
    pub consecutive_no_progress_samples: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrainProgressDisposition {
    Progress,
    Correction { consecutive_samples: u32 },
    AttentionRequired { consecutive_samples: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrainNoProgressPolicy {
    attention_threshold: u32,
}

impl BrainNoProgressPolicy {
    pub fn new(attention_threshold: u32) -> Result<Self, &'static str> {
        if attention_threshold < 2 {
            return Err("no-progress attention threshold must be at least 2");
        }
        Ok(Self {
            attention_threshold,
        })
    }

    pub const fn attention_threshold(self) -> u32 {
        self.attention_threshold
    }

    pub fn observe(
        self,
        state: &mut BrainNoProgressState,
        sample: BrainProgressSample,
    ) -> BrainProgressDisposition {
        let equivalent_failed_sample = sample.result_class != BrainProgressResultClass::Succeeded
            && state
                .previous_sample
                .as_ref()
                .is_some_and(|previous| previous == &sample);

        state.consecutive_no_progress_samples = if equivalent_failed_sample {
            state.consecutive_no_progress_samples.saturating_add(1)
        } else {
            0
        };
        state.previous_sample = Some(sample);

        match state.consecutive_no_progress_samples {
            0 => BrainProgressDisposition::Progress,
            count if count >= self.attention_threshold => {
                BrainProgressDisposition::AttentionRequired {
                    consecutive_samples: count,
                }
            }
            count => BrainProgressDisposition::Correction {
                consecutive_samples: count,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(result_class: BrainProgressResultClass, result: &str) -> BrainProgressSample {
        BrainProgressSample {
            intent_fingerprint: "intent".to_string(),
            result_fingerprint: result.to_string(),
            state_fingerprint: "state".to_string(),
            assistant_progress_fingerprint: "assistant".to_string(),
            result_class,
        }
    }

    #[test]
    fn successful_repetition_is_progress() {
        let policy = BrainNoProgressPolicy::new(2).unwrap();
        let mut state = BrainNoProgressState::default();
        assert_eq!(
            policy.observe(
                &mut state,
                sample(BrainProgressResultClass::Succeeded, "same")
            ),
            BrainProgressDisposition::Progress
        );
        assert_eq!(
            policy.observe(
                &mut state,
                sample(BrainProgressResultClass::Succeeded, "same")
            ),
            BrainProgressDisposition::Progress
        );
        assert_eq!(state.consecutive_no_progress_samples, 0);
    }

    #[test]
    fn equivalent_failures_correct_then_require_attention() {
        let policy = BrainNoProgressPolicy::new(2).unwrap();
        let mut state = BrainNoProgressState::default();
        assert_eq!(
            policy.observe(&mut state, sample(BrainProgressResultClass::Failed, "same")),
            BrainProgressDisposition::Progress
        );
        assert_eq!(
            policy.observe(&mut state, sample(BrainProgressResultClass::Failed, "same")),
            BrainProgressDisposition::Correction {
                consecutive_samples: 1
            }
        );
        assert_eq!(
            policy.observe(&mut state, sample(BrainProgressResultClass::Failed, "same")),
            BrainProgressDisposition::AttentionRequired {
                consecutive_samples: 2
            }
        );
    }

    #[test]
    fn changed_result_or_state_resets_no_progress() {
        let policy = BrainNoProgressPolicy::new(3).unwrap();
        let mut state = BrainNoProgressState::default();
        policy.observe(&mut state, sample(BrainProgressResultClass::Failed, "one"));
        policy.observe(&mut state, sample(BrainProgressResultClass::Failed, "one"));
        assert_eq!(state.consecutive_no_progress_samples, 1);
        assert_eq!(
            policy.observe(&mut state, sample(BrainProgressResultClass::Failed, "two")),
            BrainProgressDisposition::Progress
        );
        assert_eq!(state.consecutive_no_progress_samples, 0);
    }
}
