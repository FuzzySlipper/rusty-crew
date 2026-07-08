use rusty_crew_core_protocol::{BrainWakeProviderStateOutput, BrainWakeStreamItem};
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::Mutex;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

pub type BrainRuntimeResult<T> = Result<T, BrainRuntimeError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrainRuntimeError {
    DuplicateWake {
        module_label: &'static str,
        wake_id: String,
    },
    WakeNotFound {
        module_label: &'static str,
        wake_id: String,
    },
    RegistryPoisoned {
        module_label: &'static str,
    },
}

impl BrainRuntimeError {
    pub fn is_invalid_argument(&self) -> bool {
        matches!(
            self,
            BrainRuntimeError::DuplicateWake { .. } | BrainRuntimeError::WakeNotFound { .. }
        )
    }
}

impl fmt::Display for BrainRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BrainRuntimeError::DuplicateWake {
                module_label,
                wake_id,
            } => write!(
                formatter,
                "{module_label} buffered wake {wake_id} already exists"
            ),
            BrainRuntimeError::WakeNotFound {
                module_label,
                wake_id,
            } => write!(
                formatter,
                "{module_label} buffered wake {wake_id} was not found"
            ),
            BrainRuntimeError::RegistryPoisoned { module_label } => {
                write!(
                    formatter,
                    "{module_label} buffered run registry is poisoned"
                )
            }
        }
    }
}

impl std::error::Error for BrainRuntimeError {}

#[derive(Debug, Clone, Serialize)]
pub struct BufferedNeutralPendingToolRequest {
    pub call_id: String,
    pub provider_item_id: Option<String>,
    pub name: String,
    pub arguments_json: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BufferedNeutralToolOutput {
    pub output: String,
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BufferedNeutralCancellation {
    pub reason_code: String,
    pub summary: String,
    pub cancelled_at: String,
}

#[derive(Debug)]
pub struct BufferedNeutralRun<Metrics, SecretUpdate> {
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

    pub fn queue_pending_tool_request(&mut self, request: BufferedNeutralPendingToolRequest) {
        self.pending_tool_requests.push_back(request);
    }

    pub fn drain_pending_tool_requests(&mut self) -> Vec<BufferedNeutralPendingToolRequest> {
        self.pending_tool_requests.drain(..).collect()
    }

    pub fn submit_tool_output(&mut self, call_id: String, output: BufferedNeutralToolOutput) {
        self.submitted_tool_outputs.insert(call_id, output);
    }

    pub fn take_submitted_tool_output(
        &mut self,
        call_id: &str,
    ) -> Option<BufferedNeutralToolOutput> {
        self.submitted_tool_outputs.remove(call_id)
    }
}

pub struct BufferedNeutralRunRegistry<Metrics, SecretUpdate> {
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
    ) -> BrainRuntimeResult<()> {
        let mut runs = self.lock_runs()?;
        if runs.contains_key(&wake_id) {
            return Err(BrainRuntimeError::DuplicateWake {
                module_label: self.module_label,
                wake_id,
            });
        }
        runs.insert(wake_id, run);
        Ok(())
    }

    pub fn with_run_mut<R>(
        &self,
        wake_id: &str,
        operation: impl FnOnce(&mut BufferedNeutralRun<Metrics, SecretUpdate>) -> R,
    ) -> BrainRuntimeResult<R> {
        let mut runs = self.lock_runs()?;
        let run = runs
            .get_mut(wake_id)
            .ok_or_else(|| BrainRuntimeError::WakeNotFound {
                module_label: self.module_label,
                wake_id: wake_id.to_string(),
            })?;
        Ok(operation(run))
    }

    pub fn remove(
        &self,
        wake_id: &str,
    ) -> BrainRuntimeResult<Option<BufferedNeutralRun<Metrics, SecretUpdate>>> {
        let mut runs = self.lock_runs()?;
        Ok(runs.remove(wake_id))
    }

    pub fn contains(&self, wake_id: &str) -> BrainRuntimeResult<bool> {
        let runs = self.lock_runs()?;
        Ok(runs.contains_key(wake_id))
    }

    fn lock_runs(
        &self,
    ) -> BrainRuntimeResult<
        std::sync::MutexGuard<'_, HashMap<String, BufferedNeutralRun<Metrics, SecretUpdate>>>,
    > {
        self.runs
            .lock()
            .map_err(|_| BrainRuntimeError::RegistryPoisoned {
                module_label: self.module_label,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_wake_ids_are_rejected_per_registry() {
        let registry: BufferedNeutralRunRegistry<String, String> =
            BufferedNeutralRunRegistry::new("test-module");

        registry
            .insert("wake-1".to_string(), BufferedNeutralRun::new(10_000))
            .expect("first insert");
        let error = registry
            .insert("wake-1".to_string(), BufferedNeutralRun::new(10_000))
            .expect_err("duplicate insert should fail");

        assert_eq!(
            error,
            BrainRuntimeError::DuplicateWake {
                module_label: "test-module",
                wake_id: "wake-1".to_string(),
            }
        );
        assert!(error.is_invalid_argument());
    }

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
                run.queue_pending_tool_request(BufferedNeutralPendingToolRequest {
                    call_id: "responses-call".to_string(),
                    provider_item_id: None,
                    name: "responses_tool".to_string(),
                    arguments_json: "{}".to_string(),
                });
            })
            .expect("responses mutate");
        pi_agent
            .with_run_mut("wake-1", |run| {
                run.queue_pending_tool_request(BufferedNeutralPendingToolRequest {
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

    #[test]
    fn run_timeout_uses_started_at_and_budget() {
        let mut run: BufferedNeutralRun<(), ()> = BufferedNeutralRun::new(100);
        assert!(!run.is_timed_out());

        run.started_at = OffsetDateTime::now_utc() - time::Duration::milliseconds(101);

        assert!(run.is_timed_out());
    }

    #[test]
    fn cancellation_marks_terminal_and_clears_tool_state() {
        let mut run: BufferedNeutralRun<(), ()> = BufferedNeutralRun::new(10_000);
        run.queue_pending_tool_request(BufferedNeutralPendingToolRequest {
            call_id: "call-1".to_string(),
            provider_item_id: Some("provider-1".to_string()),
            name: "tool".to_string(),
            arguments_json: "{}".to_string(),
        });
        run.submit_tool_output(
            "call-1".to_string(),
            BufferedNeutralToolOutput {
                output: "done".to_string(),
                is_error: false,
            },
        );

        run.cancel("user_cancelled".to_string(), "Stopped by user".to_string());

        assert!(run.terminal);
        assert!(run.is_cancelled());
        assert!(run.pending_tool_requests.is_empty());
        assert!(run.submitted_tool_outputs.is_empty());
        assert_eq!(run.error.as_deref(), Some("Stopped by user"));
    }

    #[test]
    fn pending_tool_requests_drain_in_fifo_order() {
        let mut run: BufferedNeutralRun<(), ()> = BufferedNeutralRun::new(10_000);
        run.queue_pending_tool_request(BufferedNeutralPendingToolRequest {
            call_id: "first".to_string(),
            provider_item_id: None,
            name: "tool_a".to_string(),
            arguments_json: "{}".to_string(),
        });
        run.queue_pending_tool_request(BufferedNeutralPendingToolRequest {
            call_id: "second".to_string(),
            provider_item_id: None,
            name: "tool_b".to_string(),
            arguments_json: "{}".to_string(),
        });

        let drained = run.drain_pending_tool_requests();

        assert_eq!(
            drained
                .iter()
                .map(|request| request.call_id.as_str())
                .collect::<Vec<_>>(),
            vec!["first", "second"]
        );
        assert!(run.pending_tool_requests.is_empty());
    }

    #[test]
    fn submitted_tool_output_is_delivered_once() {
        let mut run: BufferedNeutralRun<(), ()> = BufferedNeutralRun::new(10_000);
        run.submit_tool_output(
            "call-1".to_string(),
            BufferedNeutralToolOutput {
                output: "tool result".to_string(),
                is_error: false,
            },
        );

        assert_eq!(
            run.take_submitted_tool_output("call-1"),
            Some(BufferedNeutralToolOutput {
                output: "tool result".to_string(),
                is_error: false,
            })
        );
        assert_eq!(run.take_submitted_tool_output("call-1"), None);
    }

    #[test]
    fn terminal_runs_can_be_removed() {
        let registry: BufferedNeutralRunRegistry<(), ()> =
            BufferedNeutralRunRegistry::new("test-module");
        registry
            .insert("wake-1".to_string(), BufferedNeutralRun::new(10_000))
            .expect("insert");
        registry
            .with_run_mut("wake-1", |run| {
                run.terminal = true;
            })
            .expect("mark terminal");

        let removed = registry.remove("wake-1").expect("remove");

        assert!(removed.is_some());
        assert!(!registry.contains("wake-1").expect("contains"));
    }
}
