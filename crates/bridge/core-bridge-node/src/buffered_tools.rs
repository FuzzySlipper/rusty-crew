use rusty_crew_core_protocol::{BrainWakeProviderStateOutput, BrainWakeStreamItem};
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct BufferedNeutralPendingToolRequest {
    pub call_id: String,
    pub provider_item_id: Option<String>,
    pub name: String,
    pub arguments_json: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct BufferedNeutralToolOutput {
    pub output: String,
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct BufferedNeutralCancellation {
    pub reason_code: String,
    pub summary: String,
    pub cancelled_at: String,
}

#[derive(Debug)]
pub(crate) struct BufferedNeutralRun<Metrics, SecretUpdate> {
    pub items: VecDeque<BrainWakeStreamItem>,
    pub pending_tool_requests: VecDeque<BufferedNeutralPendingToolRequest>,
    pub submitted_tool_outputs: HashMap<String, BufferedNeutralToolOutput>,
    pub terminal: bool,
    pub started_at: OffsetDateTime,
    pub wake_timeout_ms: u64,
    pub provider_state: Option<BrainWakeProviderStateOutput>,
    pub transport_metrics: Option<Metrics>,
    pub credential_secret_update: Option<SecretUpdate>,
    pub error: Option<String>,
    pub cancellation: Option<BufferedNeutralCancellation>,
}

impl<Metrics, SecretUpdate> BufferedNeutralRun<Metrics, SecretUpdate> {
    pub fn new(wake_timeout_ms: u64) -> Self {
        Self {
            items: VecDeque::new(),
            pending_tool_requests: VecDeque::new(),
            submitted_tool_outputs: HashMap::new(),
            terminal: false,
            started_at: OffsetDateTime::now_utc(),
            wake_timeout_ms,
            provider_state: None,
            transport_metrics: None,
            credential_secret_update: None,
            error: None,
            cancellation: None,
        }
    }

    pub fn is_timed_out(&self) -> bool {
        let elapsed = OffsetDateTime::now_utc() - self.started_at;
        elapsed.whole_milliseconds() as u64 > self.wake_timeout_ms
    }

    pub fn cancel(&mut self, reason_code: String, summary: String) {
        if self.cancellation.is_none() {
            self.cancellation = Some(BufferedNeutralCancellation {
                reason_code,
                summary: summary.clone(),
                cancelled_at: OffsetDateTime::now_utc()
                    .format(&Rfc3339)
                    .unwrap_or_else(|_| "unknown".to_string()),
            });
        }
        self.pending_tool_requests.clear();
        self.submitted_tool_outputs.clear();
        self.error = Some(summary);
        self.terminal = true;
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_some()
    }
}

pub(crate) struct BufferedNeutralRunRegistry<Metrics, SecretUpdate> {
    module_label: &'static str,
    runs: Mutex<HashMap<String, BufferedNeutralRun<Metrics, SecretUpdate>>>,
}

impl<Metrics, SecretUpdate> BufferedNeutralRunRegistry<Metrics, SecretUpdate> {
    pub fn new(module_label: &'static str) -> Self {
        Self {
            module_label,
            runs: Mutex::new(HashMap::new()),
        }
    }

    pub fn insert(
        &self,
        wake_id: String,
        run: BufferedNeutralRun<Metrics, SecretUpdate>,
    ) -> napi::Result<()> {
        let mut runs = self.lock_runs()?;
        if runs.contains_key(&wake_id) {
            return Err(napi::Error::new(
                napi::Status::InvalidArg,
                format!(
                    "{} buffered wake {wake_id} already exists",
                    self.module_label
                ),
            ));
        }
        runs.insert(wake_id, run);
        Ok(())
    }

    pub fn with_run_mut<R>(
        &self,
        wake_id: &str,
        operation: impl FnOnce(&mut BufferedNeutralRun<Metrics, SecretUpdate>) -> R,
    ) -> napi::Result<R> {
        let mut runs = self.lock_runs()?;
        let run = runs.get_mut(wake_id).ok_or_else(|| {
            napi::Error::new(
                napi::Status::InvalidArg,
                format!(
                    "{} buffered wake {wake_id} was not found",
                    self.module_label
                ),
            )
        })?;
        Ok(operation(run))
    }

    pub fn remove(
        &self,
        wake_id: &str,
    ) -> napi::Result<Option<BufferedNeutralRun<Metrics, SecretUpdate>>> {
        let mut runs = self.lock_runs()?;
        Ok(runs.remove(wake_id))
    }

    fn lock_runs(
        &self,
    ) -> napi::Result<
        std::sync::MutexGuard<'_, HashMap<String, BufferedNeutralRun<Metrics, SecretUpdate>>>,
    > {
        self.runs.lock().map_err(|_| {
            napi::Error::new(
                napi::Status::GenericFailure,
                format!("{} buffered run registry is poisoned", self.module_label),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registries_isolate_identical_wake_ids_for_different_brain_modules() {
        let responses: BufferedNeutralRunRegistry<String, String> =
            BufferedNeutralRunRegistry::new("OpenAI Responses");
        let pi_agent: BufferedNeutralRunRegistry<String, String> =
            BufferedNeutralRunRegistry::new("pi-agent");

        responses
            .insert("wake-1".to_string(), BufferedNeutralRun::new(10_000))
            .expect("responses insert");
        pi_agent
            .insert("wake-1".to_string(), BufferedNeutralRun::new(10_000))
            .expect("pi insert");

        responses
            .with_run_mut("wake-1", |run| {
                run.pending_tool_requests
                    .push_back(BufferedNeutralPendingToolRequest {
                        call_id: "responses-call".to_string(),
                        provider_item_id: None,
                        name: "responses_tool".to_string(),
                        arguments_json: "{}".to_string(),
                    });
            })
            .expect("responses mutate");
        pi_agent
            .with_run_mut("wake-1", |run| {
                run.pending_tool_requests
                    .push_back(BufferedNeutralPendingToolRequest {
                        call_id: "pi-call".to_string(),
                        provider_item_id: None,
                        name: "pi_tool".to_string(),
                        arguments_json: "{}".to_string(),
                    });
            })
            .expect("pi mutate");

        responses
            .with_run_mut("wake-1", |run| {
                assert_eq!(
                    run.pending_tool_requests
                        .front()
                        .map(|request| request.call_id.as_str()),
                    Some("responses-call")
                );
            })
            .expect("responses inspect");
        pi_agent
            .with_run_mut("wake-1", |run| {
                assert_eq!(
                    run.pending_tool_requests
                        .front()
                        .map(|request| request.call_id.as_str()),
                    Some("pi-call")
                );
            })
            .expect("pi inspect");
    }
}
