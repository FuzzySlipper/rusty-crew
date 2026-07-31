use crate::{
    BrainRuntimeError, BrainRuntimeResult, BufferedBrainHostToolResult,
    BufferedBrainHostTurnDisposition, BufferedBrainStreamRetentionMetrics,
    BufferedBrainToolFailurePolicy, BufferedBrainToolPolicyDecision,
    BufferedBrainTurnCleanupReport, BufferedBrainTurnDiagnostic, BufferedNeutralCancellation,
    BufferedNeutralPendingToolRequest, BufferedNeutralToolOutput, BufferedNeutralToolOutputPoll,
};
use rusty_crew_core_protocol::{
    BrainEvent, BrainWakeProviderStateOutput, BrainWakeStreamItem, SessionId,
};
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::Mutex;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BufferedBrainTurnPhase {
    Created,
    Running,
    AwaitingHostTools,
    Yielded,
    AttentionRequired,
    Completed,
    Failed,
    Cancelled,
}

impl BufferedBrainTurnPhase {
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Yielded
                | Self::AttentionRequired
                | Self::Completed
                | Self::Failed
                | Self::Cancelled
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BufferedBrainTurnLimits {
    pub max_stream_items: usize,
    pub max_stream_delta_bytes: usize,
    pub max_pending_tool_requests: usize,
    pub max_tool_results: usize,
    pub max_tool_output_bytes: usize,
}

impl Default for BufferedBrainTurnLimits {
    fn default() -> Self {
        Self {
            max_stream_items: 4_096,
            max_stream_delta_bytes: 8 * 1_024 * 1_024,
            max_pending_tool_requests: 128,
            max_tool_results: 1_024,
            max_tool_output_bytes: 64 * 1_024,
        }
    }
}

impl BufferedBrainTurnLimits {
    fn validate(self) -> Result<Self, BufferedBrainTurnError> {
        for (name, value) in [
            ("max_stream_items", self.max_stream_items),
            ("max_stream_delta_bytes", self.max_stream_delta_bytes),
            ("max_pending_tool_requests", self.max_pending_tool_requests),
            ("max_tool_results", self.max_tool_results),
            ("max_tool_output_bytes", self.max_tool_output_bytes),
        ] {
            if value == 0 {
                return Err(BufferedBrainTurnError::InvalidLimit { name });
            }
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BufferedBrainTurnError {
    InvalidLimit {
        name: &'static str,
    },
    InvalidTransition {
        phase: BufferedBrainTurnPhase,
        operation: &'static str,
    },
    WakeIdentityMismatch {
        expected: String,
        found: String,
    },
    SessionIdentityMismatch {
        expected: String,
        found: String,
    },
    DuplicateToolRequest {
        call_id: String,
    },
    UnknownToolRequest {
        call_id: String,
    },
    ConflictingToolResult {
        call_id: String,
    },
    BufferLimitExceeded {
        buffer: &'static str,
        limit: usize,
    },
    ToolOutputTooLarge {
        call_id: String,
        actual_bytes: usize,
        limit_bytes: usize,
    },
    SequenceExhausted,
}

impl fmt::Display for BufferedBrainTurnError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimit { name } => write!(formatter, "{name} must be greater than zero"),
            Self::InvalidTransition { phase, operation } => {
                write!(formatter, "cannot {operation} while turn is {phase:?}")
            }
            Self::WakeIdentityMismatch { expected, found } => write!(
                formatter,
                "brain stream wake id mismatch: expected {expected}, found {found}"
            ),
            Self::SessionIdentityMismatch { expected, found } => write!(
                formatter,
                "brain stream session id mismatch: expected {expected}, found {found}"
            ),
            Self::DuplicateToolRequest { call_id } => {
                write!(formatter, "tool request {call_id} is already registered")
            }
            Self::UnknownToolRequest { call_id } => {
                write!(formatter, "tool request {call_id} is not pending")
            }
            Self::ConflictingToolResult { call_id } => write!(
                formatter,
                "tool request {call_id} already has a different accepted result"
            ),
            Self::BufferLimitExceeded { buffer, limit } => {
                write!(formatter, "{buffer} exceeded configured limit {limit}")
            }
            Self::ToolOutputTooLarge {
                call_id,
                actual_bytes,
                limit_bytes,
            } => write!(
                formatter,
                "tool result {call_id} is {actual_bytes} bytes; limit is {limit_bytes}"
            ),
            Self::SequenceExhausted => write!(formatter, "brain stream sequence exhausted"),
        }
    }
}

impl std::error::Error for BufferedBrainTurnError {}

fn stream_delta_bytes(item: &BrainWakeStreamItem) -> Option<usize> {
    match item {
        BrainWakeStreamItem::Event { event } => match &event.event {
            BrainEvent::TextDelta { text } | BrainEvent::ReasoningDelta { text, .. } => {
                Some(text.len())
            }
            _ => None,
        },
        BrainWakeStreamItem::Actions { .. } | BrainWakeStreamItem::WakeFailed { .. } => None,
    }
}

fn coalesce_adjacent_delta(
    stream_items: &mut VecDeque<SequencedBrainWakeStreamItem>,
    incoming: &BrainWakeStreamItem,
) -> Option<u64> {
    let last = stream_items.back_mut()?;
    let (
        BrainWakeStreamItem::Event { event: last_event },
        BrainWakeStreamItem::Event {
            event: incoming_event,
        },
    ) = (&mut last.item, incoming)
    else {
        return None;
    };
    match (&mut last_event.event, &incoming_event.event) {
        (
            BrainEvent::TextDelta { text },
            BrainEvent::TextDelta {
                text: incoming_text,
            },
        ) => {
            text.push_str(incoming_text);
            Some(last.sequence)
        }
        (
            BrainEvent::ReasoningDelta { text, format },
            BrainEvent::ReasoningDelta {
                text: incoming_text,
                format: incoming_format,
            },
        ) if format == incoming_format => {
            text.push_str(incoming_text);
            Some(last.sequence)
        }
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SequencedBrainWakeStreamItem {
    pub sequence: u64,
    pub item: BrainWakeStreamItem,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BufferedBrainTurnTerminal {
    pub reason_code: String,
    pub summary: String,
    pub occurred_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BufferedBrainToolResultReceipt {
    Accepted,
    Duplicate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BufferedBrainTurnDrain {
    pub wake_id: String,
    pub phase: BufferedBrainTurnPhase,
    pub items: Vec<SequencedBrainWakeStreamItem>,
    pub remaining_stream_item_count: usize,
    pub terminal: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BufferedBrainHostToolSubmission {
    pub receipt: BufferedBrainToolResultReceipt,
    pub decision: BufferedBrainToolPolicyDecision,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferedBrainHostToolOutput {
    pub output: BufferedNeutralToolOutput,
    pub turn_disposition: Option<BufferedBrainHostTurnDisposition>,
}

#[derive(Debug)]
pub struct BufferedBrainTurnCoordinator {
    module_id: String,
    wake_id: String,
    session_id: SessionId,
    phase: BufferedBrainTurnPhase,
    limits: BufferedBrainTurnLimits,
    created_at: OffsetDateTime,
    started_at: Option<OffsetDateTime>,
    last_transition_at: OffsetDateTime,
    next_stream_sequence: u64,
    stream_items: VecDeque<SequencedBrainWakeStreamItem>,
    raw_stream_item_count: usize,
    raw_delta_item_count: usize,
    retained_stream_item_count: usize,
    coalesced_delta_item_count: usize,
    dropped_stream_item_count: usize,
    retained_delta_bytes: usize,
    queued_delta_bytes: usize,
    pending_tool_requests: HashMap<String, BufferedNeutralPendingToolRequest>,
    undelivered_tool_request_ids: VecDeque<String>,
    submitted_tool_outputs: HashMap<String, BufferedNeutralToolOutput>,
    submitted_tool_dispositions: HashMap<String, Option<BufferedBrainHostTurnDisposition>>,
    accepted_tool_outputs: HashMap<String, BufferedNeutralToolOutput>,
    accepted_host_tool_results: HashMap<String, BufferedBrainHostToolResult>,
    accepted_host_tool_decisions: HashMap<String, BufferedBrainToolPolicyDecision>,
    tool_failure_policy: BufferedBrainToolFailurePolicy,
    provider_state_output: Option<BrainWakeProviderStateOutput>,
    terminal: Option<BufferedBrainTurnTerminal>,
}

impl BufferedBrainTurnCoordinator {
    pub fn new(
        module_id: impl Into<String>,
        wake_id: impl Into<String>,
        session_id: SessionId,
        limits: BufferedBrainTurnLimits,
    ) -> Result<Self, BufferedBrainTurnError> {
        Self::new_at(
            module_id,
            wake_id,
            session_id,
            limits,
            OffsetDateTime::now_utc(),
        )
    }

    pub fn new_at(
        module_id: impl Into<String>,
        wake_id: impl Into<String>,
        session_id: SessionId,
        limits: BufferedBrainTurnLimits,
        now: OffsetDateTime,
    ) -> Result<Self, BufferedBrainTurnError> {
        Ok(Self {
            module_id: module_id.into(),
            wake_id: wake_id.into(),
            session_id,
            phase: BufferedBrainTurnPhase::Created,
            limits: limits.validate()?,
            created_at: now,
            started_at: None,
            last_transition_at: now,
            next_stream_sequence: 1,
            stream_items: VecDeque::new(),
            raw_stream_item_count: 0,
            raw_delta_item_count: 0,
            retained_stream_item_count: 0,
            coalesced_delta_item_count: 0,
            dropped_stream_item_count: 0,
            retained_delta_bytes: 0,
            queued_delta_bytes: 0,
            pending_tool_requests: HashMap::new(),
            undelivered_tool_request_ids: VecDeque::new(),
            submitted_tool_outputs: HashMap::new(),
            submitted_tool_dispositions: HashMap::new(),
            accepted_tool_outputs: HashMap::new(),
            accepted_host_tool_results: HashMap::new(),
            accepted_host_tool_decisions: HashMap::new(),
            tool_failure_policy: BufferedBrainToolFailurePolicy::default(),
            provider_state_output: None,
            terminal: None,
        })
    }

    pub fn module_id(&self) -> &str {
        &self.module_id
    }

    pub fn wake_id(&self) -> &str {
        &self.wake_id
    }

    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    pub fn phase(&self) -> BufferedBrainTurnPhase {
        self.phase
    }

    pub fn created_at(&self) -> OffsetDateTime {
        self.created_at
    }

    pub fn started_at(&self) -> Option<OffsetDateTime> {
        self.started_at
    }

    pub fn last_transition_at(&self) -> OffsetDateTime {
        self.last_transition_at
    }

    pub fn terminal(&self) -> Option<&BufferedBrainTurnTerminal> {
        self.terminal.as_ref()
    }

    pub fn is_cancelled(&self) -> bool {
        self.phase == BufferedBrainTurnPhase::Cancelled
    }

    pub fn has_error(&self) -> bool {
        matches!(
            self.phase,
            BufferedBrainTurnPhase::Failed | BufferedBrainTurnPhase::Cancelled
        )
    }

    pub fn cancellation(&self) -> Option<BufferedNeutralCancellation> {
        (self.phase == BufferedBrainTurnPhase::Cancelled).then(|| {
            let terminal = self
                .terminal
                .as_ref()
                .expect("cancelled turn must retain terminal details");
            BufferedNeutralCancellation {
                reason_code: terminal.reason_code.clone(),
                summary: terminal.summary.clone(),
                cancelled_at: terminal.occurred_at.clone(),
            }
        })
    }

    pub fn provider_state_output(&self) -> Option<&BrainWakeProviderStateOutput> {
        self.provider_state_output.as_ref()
    }

    pub fn pending_tool_request_count(&self) -> usize {
        self.pending_tool_requests.len()
    }

    pub fn submitted_tool_output_count(&self) -> usize {
        self.submitted_tool_outputs.len()
    }

    pub fn tool_failure_policy(&self) -> &BufferedBrainToolFailurePolicy {
        &self.tool_failure_policy
    }

    pub fn queued_stream_item_count(&self) -> usize {
        self.stream_items.len()
    }

    pub fn stream_retention_metrics(&self) -> BufferedBrainStreamRetentionMetrics {
        BufferedBrainStreamRetentionMetrics {
            raw_stream_item_count: self.raw_stream_item_count,
            raw_delta_item_count: self.raw_delta_item_count,
            retained_stream_item_count: self.retained_stream_item_count,
            coalesced_delta_item_count: self.coalesced_delta_item_count,
            dropped_stream_item_count: self.dropped_stream_item_count,
            retained_delta_bytes: self.retained_delta_bytes,
            queued_delta_bytes: self.queued_delta_bytes,
            max_stream_items: self.limits.max_stream_items,
            max_stream_delta_bytes: self.limits.max_stream_delta_bytes,
        }
    }

    pub fn start(&mut self) -> Result<(), BufferedBrainTurnError> {
        self.start_at(OffsetDateTime::now_utc())
    }

    pub fn start_at(&mut self, now: OffsetDateTime) -> Result<(), BufferedBrainTurnError> {
        self.require_phase(BufferedBrainTurnPhase::Created, "start")?;
        self.phase = BufferedBrainTurnPhase::Running;
        self.started_at = Some(now);
        self.record_transition_at(now);
        Ok(())
    }

    pub fn enqueue_stream_item(
        &mut self,
        item: BrainWakeStreamItem,
    ) -> Result<u64, BufferedBrainTurnError> {
        self.enqueue_stream_item_at(item, OffsetDateTime::now_utc())
    }

    pub fn enqueue_provider_stream_item(
        &mut self,
        item: BrainWakeStreamItem,
    ) -> Result<u64, BufferedBrainTurnError> {
        match self.enqueue_stream_item(item) {
            Ok(sequence) => Ok(sequence),
            Err(error @ BufferedBrainTurnError::BufferLimitExceeded { .. }) => Err(error),
            Err(error) => {
                if !self.phase.is_terminal() {
                    let _ = self.fail(
                        "provider_stream_enqueue_failed",
                        format!("provider stream item was rejected: {error}"),
                    );
                }
                Err(error)
            }
        }
    }

    pub fn enqueue_stream_item_at(
        &mut self,
        item: BrainWakeStreamItem,
        now: OffsetDateTime,
    ) -> Result<u64, BufferedBrainTurnError> {
        self.require_phase(BufferedBrainTurnPhase::Running, "enqueue stream item")?;
        self.validate_stream_identity(&item)?;
        let delta_bytes = stream_delta_bytes(&item);
        if let Some(delta_bytes) = delta_bytes {
            let Some(next_queued_delta_bytes) = self.queued_delta_bytes.checked_add(delta_bytes)
            else {
                return Err(BufferedBrainTurnError::BufferLimitExceeded {
                    buffer: "stream_delta_bytes",
                    limit: self.limits.max_stream_delta_bytes,
                });
            };
            // An atomic provider delta can itself exceed the retention target. Admit one
            // only after the host has drained the queue; otherwise signal backpressure.
            if next_queued_delta_bytes > self.limits.max_stream_delta_bytes
                && !self.stream_items.is_empty()
            {
                return Err(BufferedBrainTurnError::BufferLimitExceeded {
                    buffer: "stream_delta_bytes",
                    limit: self.limits.max_stream_delta_bytes,
                });
            }
            if let Some(sequence) = coalesce_adjacent_delta(&mut self.stream_items, &item) {
                self.raw_stream_item_count = self.raw_stream_item_count.saturating_add(1);
                self.raw_delta_item_count = self.raw_delta_item_count.saturating_add(1);
                self.coalesced_delta_item_count = self.coalesced_delta_item_count.saturating_add(1);
                self.retained_delta_bytes = self.retained_delta_bytes.saturating_add(delta_bytes);
                self.queued_delta_bytes = next_queued_delta_bytes;
                self.record_transition_at(now);
                return Ok(sequence);
            }
        }

        let terminal_item = item.is_terminal();
        if !terminal_item && self.stream_items.len() >= self.limits.max_stream_items {
            return Err(BufferedBrainTurnError::BufferLimitExceeded {
                buffer: "stream_items",
                limit: self.limits.max_stream_items,
            });
        }

        let sequence = self.next_stream_sequence;
        self.next_stream_sequence = self
            .next_stream_sequence
            .checked_add(1)
            .ok_or(BufferedBrainTurnError::SequenceExhausted)?;
        let terminal_phase = match &item {
            BrainWakeStreamItem::Actions { .. } => Some(BufferedBrainTurnPhase::Completed),
            BrainWakeStreamItem::WakeFailed { failure } => {
                self.terminal = Some(BufferedBrainTurnTerminal {
                    reason_code: failure
                        .reason_code
                        .clone()
                        .unwrap_or_else(|| failure.kind.reason_code().to_string()),
                    summary: failure.message.clone(),
                    occurred_at: format_rfc3339(now),
                });
                Some(BufferedBrainTurnPhase::Failed)
            }
            BrainWakeStreamItem::Event { .. } => None,
        };
        self.stream_items
            .push_back(SequencedBrainWakeStreamItem { sequence, item });
        self.raw_stream_item_count = self.raw_stream_item_count.saturating_add(1);
        self.retained_stream_item_count = self.retained_stream_item_count.saturating_add(1);
        if let Some(delta_bytes) = delta_bytes {
            self.raw_delta_item_count = self.raw_delta_item_count.saturating_add(1);
            self.retained_delta_bytes = self.retained_delta_bytes.saturating_add(delta_bytes);
            self.queued_delta_bytes = self.queued_delta_bytes.saturating_add(delta_bytes);
        }
        if let Some(phase) = terminal_phase {
            self.phase = phase;
            if phase == BufferedBrainTurnPhase::Completed {
                self.terminal = Some(BufferedBrainTurnTerminal {
                    reason_code: "completed".to_string(),
                    summary: "brain turn completed".to_string(),
                    occurred_at: format_rfc3339(now),
                });
            }
        }
        self.record_transition_at(now);
        Ok(sequence)
    }

    pub fn queue_tool_request(
        &mut self,
        request: BufferedNeutralPendingToolRequest,
    ) -> Result<(), BufferedBrainTurnError> {
        self.queue_tool_request_at(request, OffsetDateTime::now_utc())
    }

    pub fn queue_tool_request_at(
        &mut self,
        request: BufferedNeutralPendingToolRequest,
        now: OffsetDateTime,
    ) -> Result<(), BufferedBrainTurnError> {
        if !matches!(
            self.phase,
            BufferedBrainTurnPhase::Running | BufferedBrainTurnPhase::AwaitingHostTools
        ) {
            return Err(self.invalid_transition("queue tool request"));
        }
        if self.pending_tool_requests.contains_key(&request.call_id)
            || self.accepted_tool_outputs.contains_key(&request.call_id)
        {
            return Err(BufferedBrainTurnError::DuplicateToolRequest {
                call_id: request.call_id,
            });
        }
        if self.pending_tool_requests.len() >= self.limits.max_pending_tool_requests {
            return Err(BufferedBrainTurnError::BufferLimitExceeded {
                buffer: "pending_tool_requests",
                limit: self.limits.max_pending_tool_requests,
            });
        }
        let call_id = request.call_id.clone();
        self.pending_tool_requests.insert(call_id.clone(), request);
        self.undelivered_tool_request_ids.push_back(call_id);
        self.phase = BufferedBrainTurnPhase::AwaitingHostTools;
        self.record_transition_at(now);
        Ok(())
    }

    pub fn drain_host_tool_requests(
        &mut self,
        max_requests: usize,
    ) -> Vec<BufferedNeutralPendingToolRequest> {
        let mut requests = Vec::new();
        for _ in 0..max_requests.max(1) {
            let Some(call_id) = self.undelivered_tool_request_ids.pop_front() else {
                break;
            };
            if let Some(request) = self.pending_tool_requests.get(&call_id) {
                requests.push(request.clone());
            }
        }
        if !requests.is_empty() {
            self.record_transition_at(OffsetDateTime::now_utc());
        }
        requests
    }

    pub fn submit_tool_output(
        &mut self,
        call_id: &str,
        output: BufferedNeutralToolOutput,
    ) -> Result<BufferedBrainToolResultReceipt, BufferedBrainTurnError> {
        self.submit_tool_output_at(call_id, output, OffsetDateTime::now_utc())
    }

    pub fn submit_host_tool_result(
        &mut self,
        call_id: &str,
        result: BufferedBrainHostToolResult,
    ) -> Result<BufferedBrainHostToolSubmission, BufferedBrainTurnError> {
        self.submit_host_tool_result_at(call_id, result, OffsetDateTime::now_utc())
    }

    pub fn submit_host_tool_result_at(
        &mut self,
        call_id: &str,
        result: BufferedBrainHostToolResult,
        now: OffsetDateTime,
    ) -> Result<BufferedBrainHostToolSubmission, BufferedBrainTurnError> {
        if let Some(accepted) = self.accepted_host_tool_results.get(call_id) {
            return if accepted == &result {
                Ok(BufferedBrainHostToolSubmission {
                    receipt: BufferedBrainToolResultReceipt::Duplicate,
                    decision: self
                        .accepted_host_tool_decisions
                        .get(call_id)
                        .expect("accepted host result must retain its policy decision")
                        .clone(),
                })
            } else {
                Err(BufferedBrainTurnError::ConflictingToolResult {
                    call_id: call_id.to_string(),
                })
            };
        }
        if self.phase != BufferedBrainTurnPhase::AwaitingHostTools {
            return Err(self.invalid_transition("submit host tool result"));
        }
        let request = self.pending_tool_requests.get(call_id).ok_or_else(|| {
            BufferedBrainTurnError::UnknownToolRequest {
                call_id: call_id.to_string(),
            }
        })?;
        let decision = self.tool_failure_policy.record_result(
            &request.name,
            &result,
            self.limits.max_tool_output_bytes,
        );
        self.submit_tool_output_at(call_id, decision.provider_output.clone(), now)?;
        self.submitted_tool_dispositions
            .insert(call_id.to_string(), result.turn_disposition);
        self.accepted_host_tool_results
            .insert(call_id.to_string(), result);
        self.accepted_host_tool_decisions
            .insert(call_id.to_string(), decision.clone());
        Ok(BufferedBrainHostToolSubmission {
            receipt: BufferedBrainToolResultReceipt::Accepted,
            decision,
        })
    }

    pub fn submit_tool_output_at(
        &mut self,
        call_id: &str,
        output: BufferedNeutralToolOutput,
        now: OffsetDateTime,
    ) -> Result<BufferedBrainToolResultReceipt, BufferedBrainTurnError> {
        if let Some(accepted) = self.accepted_tool_outputs.get(call_id) {
            return if accepted == &output {
                Ok(BufferedBrainToolResultReceipt::Duplicate)
            } else {
                Err(BufferedBrainTurnError::ConflictingToolResult {
                    call_id: call_id.to_string(),
                })
            };
        }
        if self.phase != BufferedBrainTurnPhase::AwaitingHostTools {
            return Err(self.invalid_transition("submit tool output"));
        }
        if !self.pending_tool_requests.contains_key(call_id) {
            return Err(BufferedBrainTurnError::UnknownToolRequest {
                call_id: call_id.to_string(),
            });
        }
        let output_bytes = output.output.len();
        if output_bytes > self.limits.max_tool_output_bytes {
            return Err(BufferedBrainTurnError::ToolOutputTooLarge {
                call_id: call_id.to_string(),
                actual_bytes: output_bytes,
                limit_bytes: self.limits.max_tool_output_bytes,
            });
        }
        if self.submitted_tool_outputs.len() >= self.limits.max_tool_results {
            return Err(BufferedBrainTurnError::BufferLimitExceeded {
                buffer: "tool_results",
                limit: self.limits.max_tool_results,
            });
        }

        self.pending_tool_requests.remove(call_id);
        self.submitted_tool_outputs
            .insert(call_id.to_string(), output.clone());
        self.accepted_tool_outputs
            .insert(call_id.to_string(), output);
        if self.pending_tool_requests.is_empty() {
            self.phase = BufferedBrainTurnPhase::Running;
        }
        self.record_transition_at(now);
        Ok(BufferedBrainToolResultReceipt::Accepted)
    }

    pub fn take_submitted_tool_output(
        &mut self,
        call_id: &str,
    ) -> Option<BufferedNeutralToolOutput> {
        let output = self.submitted_tool_outputs.remove(call_id);
        if output.is_some() {
            self.record_transition_at(OffsetDateTime::now_utc());
        }
        output
    }

    pub fn poll_submitted_tool_output(&mut self, call_id: &str) -> BufferedNeutralToolOutputPoll {
        match self.take_submitted_tool_output(call_id) {
            Some(output) => BufferedNeutralToolOutputPoll::Ready(output),
            None => BufferedNeutralToolOutputPoll::Pending,
        }
    }

    pub fn poll_submitted_host_tool_output(
        &mut self,
        call_id: &str,
    ) -> Option<BufferedBrainHostToolOutput> {
        let output = self.take_submitted_tool_output(call_id)?;
        let turn_disposition = self.submitted_tool_dispositions.remove(call_id).flatten();
        Some(BufferedBrainHostToolOutput {
            output,
            turn_disposition,
        })
    }

    pub fn set_provider_state_output(
        &mut self,
        output: BrainWakeProviderStateOutput,
    ) -> Result<(), BufferedBrainTurnError> {
        if matches!(
            self.phase,
            BufferedBrainTurnPhase::Created | BufferedBrainTurnPhase::Cancelled
        ) {
            return Err(self.invalid_transition("set provider state output"));
        }
        self.provider_state_output = Some(output);
        self.record_transition_at(OffsetDateTime::now_utc());
        Ok(())
    }

    pub fn yield_turn(&mut self) -> Result<(), BufferedBrainTurnError> {
        self.yield_turn_at(OffsetDateTime::now_utc())
    }

    pub fn yield_turn_at(&mut self, now: OffsetDateTime) -> Result<(), BufferedBrainTurnError> {
        self.require_phase(BufferedBrainTurnPhase::Running, "yield")?;
        self.phase = BufferedBrainTurnPhase::Yielded;
        self.terminal = Some(BufferedBrainTurnTerminal {
            reason_code: "work_quantum_reached".to_string(),
            summary: "brain execution epoch yielded for durable continuation".to_string(),
            occurred_at: format_rfc3339(now),
        });
        self.record_transition_at(now);
        Ok(())
    }

    pub fn require_attention(
        &mut self,
        reason_code: impl Into<String>,
        summary: impl Into<String>,
    ) -> Result<(), BufferedBrainTurnError> {
        self.require_attention_at(reason_code, summary, OffsetDateTime::now_utc())
    }

    pub fn require_attention_at(
        &mut self,
        reason_code: impl Into<String>,
        summary: impl Into<String>,
        now: OffsetDateTime,
    ) -> Result<(), BufferedBrainTurnError> {
        self.require_phase(
            BufferedBrainTurnPhase::Running,
            "require operator attention",
        )?;
        self.phase = BufferedBrainTurnPhase::AttentionRequired;
        self.terminal = Some(BufferedBrainTurnTerminal {
            reason_code: reason_code.into(),
            summary: summary.into(),
            occurred_at: format_rfc3339(now),
        });
        self.record_transition_at(now);
        Ok(())
    }

    pub fn fail(
        &mut self,
        reason_code: impl Into<String>,
        summary: impl Into<String>,
    ) -> Result<(), BufferedBrainTurnError> {
        self.fail_at(reason_code, summary, OffsetDateTime::now_utc())
    }

    pub fn fail_at(
        &mut self,
        reason_code: impl Into<String>,
        summary: impl Into<String>,
        now: OffsetDateTime,
    ) -> Result<(), BufferedBrainTurnError> {
        self.require_active("fail")?;
        self.transition_terminal(
            BufferedBrainTurnPhase::Failed,
            reason_code.into(),
            summary.into(),
            now,
        );
        Ok(())
    }

    pub fn cancel(
        &mut self,
        reason_code: impl Into<String>,
        summary: impl Into<String>,
    ) -> Result<(), BufferedBrainTurnError> {
        self.cancel_at(reason_code, summary, OffsetDateTime::now_utc())
    }

    pub fn cancel_at(
        &mut self,
        reason_code: impl Into<String>,
        summary: impl Into<String>,
        now: OffsetDateTime,
    ) -> Result<(), BufferedBrainTurnError> {
        if self.phase == BufferedBrainTurnPhase::Cancelled {
            return Ok(());
        }
        if self.phase == BufferedBrainTurnPhase::AttentionRequired {
            self.transition_terminal(
                BufferedBrainTurnPhase::Cancelled,
                reason_code.into(),
                summary.into(),
                now,
            );
            return Ok(());
        }
        self.require_active("cancel")?;
        self.transition_terminal(
            BufferedBrainTurnPhase::Cancelled,
            reason_code.into(),
            summary.into(),
            now,
        );
        Ok(())
    }

    pub fn drain_stream(&mut self, max_items: usize) -> BufferedBrainTurnDrain {
        let mut items = Vec::new();
        for _ in 0..max_items.max(1) {
            let Some(item) = self.stream_items.pop_front() else {
                break;
            };
            if let Some(delta_bytes) = stream_delta_bytes(&item.item) {
                self.queued_delta_bytes = self.queued_delta_bytes.saturating_sub(delta_bytes);
            }
            let terminal = item.item.is_terminal();
            items.push(item);
            if terminal {
                break;
            }
        }
        if !items.is_empty() {
            self.record_transition_at(OffsetDateTime::now_utc());
        }
        BufferedBrainTurnDrain {
            wake_id: self.wake_id.clone(),
            phase: self.phase,
            items,
            remaining_stream_item_count: self.stream_items.len(),
            terminal: self.phase.is_terminal() && self.stream_items.is_empty(),
        }
    }

    fn validate_stream_identity(
        &self,
        item: &BrainWakeStreamItem,
    ) -> Result<(), BufferedBrainTurnError> {
        let (wake_id, session_id) = match item {
            BrainWakeStreamItem::Event { event } => (&event.wake_id, &event.session_id),
            BrainWakeStreamItem::Actions { batch } => (&batch.wake_id, &batch.session_id),
            BrainWakeStreamItem::WakeFailed { failure } => (&failure.wake_id, &failure.session_id),
        };
        if wake_id != &self.wake_id {
            return Err(BufferedBrainTurnError::WakeIdentityMismatch {
                expected: self.wake_id.clone(),
                found: wake_id.clone(),
            });
        }
        if session_id != &self.session_id {
            return Err(BufferedBrainTurnError::SessionIdentityMismatch {
                expected: self.session_id.to_string(),
                found: session_id.to_string(),
            });
        }
        Ok(())
    }

    fn require_phase(
        &self,
        phase: BufferedBrainTurnPhase,
        operation: &'static str,
    ) -> Result<(), BufferedBrainTurnError> {
        if self.phase != phase {
            return Err(self.invalid_transition(operation));
        }
        Ok(())
    }

    fn require_active(&self, operation: &'static str) -> Result<(), BufferedBrainTurnError> {
        if !matches!(
            self.phase,
            BufferedBrainTurnPhase::Created
                | BufferedBrainTurnPhase::Running
                | BufferedBrainTurnPhase::AwaitingHostTools
        ) {
            return Err(self.invalid_transition(operation));
        }
        Ok(())
    }

    fn invalid_transition(&self, operation: &'static str) -> BufferedBrainTurnError {
        BufferedBrainTurnError::InvalidTransition {
            phase: self.phase,
            operation,
        }
    }

    fn transition_terminal(
        &mut self,
        phase: BufferedBrainTurnPhase,
        reason_code: String,
        summary: String,
        now: OffsetDateTime,
    ) {
        debug_assert!(phase.is_terminal());
        self.phase = phase;
        self.pending_tool_requests.clear();
        self.undelivered_tool_request_ids.clear();
        self.submitted_tool_outputs.clear();
        self.submitted_tool_dispositions.clear();
        self.terminal = Some(BufferedBrainTurnTerminal {
            reason_code,
            summary,
            occurred_at: format_rfc3339(now),
        });
        self.record_transition_at(now);
    }

    fn record_transition_at(&mut self, now: OffsetDateTime) {
        self.last_transition_at = now;
    }
}

#[derive(Debug)]
pub struct BufferedBrainTurnRun<Payload> {
    pub coordinator: BufferedBrainTurnCoordinator,
    pub payload: Payload,
}

impl<Payload> BufferedBrainTurnRun<Payload> {
    pub fn new(coordinator: BufferedBrainTurnCoordinator, payload: Payload) -> Self {
        Self {
            coordinator,
            payload,
        }
    }
}

#[derive(Debug)]
pub struct BufferedBrainTurnRegistry<Payload> {
    module_label: &'static str,
    runs: Mutex<HashMap<String, BufferedBrainTurnRun<Payload>>>,
}

impl<Payload> BufferedBrainTurnRegistry<Payload> {
    pub fn new(module_label: &'static str) -> Self {
        Self {
            module_label,
            runs: Mutex::new(HashMap::new()),
        }
    }

    pub fn insert(&self, run: BufferedBrainTurnRun<Payload>) -> BrainRuntimeResult<()> {
        let wake_id = run.coordinator.wake_id().to_string();
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
        operation: impl FnOnce(&mut BufferedBrainTurnRun<Payload>) -> R,
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
    ) -> BrainRuntimeResult<Option<BufferedBrainTurnRun<Payload>>> {
        Ok(self.lock_runs()?.remove(wake_id))
    }

    pub fn diagnostics(&self) -> BrainRuntimeResult<Vec<BufferedBrainTurnDiagnostic>> {
        let now = OffsetDateTime::now_utc();
        let runs = self.lock_runs()?;
        let mut diagnostics = runs
            .values()
            .map(|run| {
                let coordinator = &run.coordinator;
                BufferedBrainTurnDiagnostic {
                    module_label: self.module_label.to_string(),
                    wake_id: coordinator.wake_id().to_string(),
                    session_id: coordinator.session_id().0.clone(),
                    agent_id: None,
                    profile_id: None,
                    phase: buffered_brain_turn_phase_name(coordinator.phase()).to_string(),
                    queued_stream_item_count: coordinator.queued_stream_item_count(),
                    stream_retention_metrics: coordinator.stream_retention_metrics(),
                    pending_tool_request_count: coordinator.pending_tool_request_count(),
                    submitted_tool_output_count: coordinator.submitted_tool_output_count(),
                    age_ms: elapsed_ms(coordinator.created_at(), now),
                    terminal: coordinator.phase().is_terminal(),
                    cancelled: coordinator.is_cancelled(),
                    has_error: coordinator.has_error(),
                    started_at: format_rfc3339(
                        coordinator.started_at().unwrap_or(coordinator.created_at()),
                    ),
                    last_transition_at: format_rfc3339(coordinator.last_transition_at()),
                }
            })
            .collect::<Vec<_>>();
        diagnostics.sort_by(|left, right| left.wake_id.cmp(&right.wake_id));
        Ok(diagnostics)
    }

    pub fn cleanup(
        &self,
        reason_code: &str,
        summary: &str,
    ) -> BrainRuntimeResult<BufferedBrainTurnCleanupReport> {
        let mut runs = self.lock_runs()?;
        let active_runs = runs.len();
        let terminal_runs = runs
            .values()
            .filter(|run| run.coordinator.phase().is_terminal())
            .count();
        let mut cancelled_nonterminal_runs = 0;
        for run in runs.values_mut() {
            if !run.coordinator.phase().is_terminal() {
                let _ = run.coordinator.cancel(reason_code, summary);
                cancelled_nonterminal_runs += 1;
            }
        }
        runs.clear();
        Ok(BufferedBrainTurnCleanupReport {
            module_label: self.module_label.to_string(),
            active_runs,
            terminal_runs,
            cancelled_nonterminal_runs,
            removed_runs: active_runs,
        })
    }

    fn lock_runs(
        &self,
    ) -> BrainRuntimeResult<std::sync::MutexGuard<'_, HashMap<String, BufferedBrainTurnRun<Payload>>>>
    {
        self.runs
            .lock()
            .map_err(|_| BrainRuntimeError::RegistryPoisoned {
                module_label: self.module_label,
            })
    }
}

fn buffered_brain_turn_phase_name(phase: BufferedBrainTurnPhase) -> &'static str {
    match phase {
        BufferedBrainTurnPhase::Created => "created",
        BufferedBrainTurnPhase::Running => "running",
        BufferedBrainTurnPhase::AwaitingHostTools => "awaiting_host_tools",
        BufferedBrainTurnPhase::Yielded => "yielded",
        BufferedBrainTurnPhase::AttentionRequired => "attention_required",
        BufferedBrainTurnPhase::Completed => "completed",
        BufferedBrainTurnPhase::Failed => "failed",
        BufferedBrainTurnPhase::Cancelled => "cancelled",
    }
}

fn elapsed_ms(start: OffsetDateTime, end: OffsetDateTime) -> u64 {
    (end - start).whole_milliseconds().max(0) as u64
}

fn format_rfc3339(timestamp: OffsetDateTime) -> String {
    timestamp
        .format(&Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_crew_core_protocol::{
        BrainActionBatch, BrainEvent, BrainEventEnvelope, BrainWakeFailure, CoreErrorKind,
    };

    fn session_id(value: &str) -> SessionId {
        SessionId(value.to_string())
    }

    fn coordinator() -> BufferedBrainTurnCoordinator {
        BufferedBrainTurnCoordinator::new_at(
            "chat-completions",
            "wake-1",
            session_id("session-1"),
            BufferedBrainTurnLimits::default(),
            OffsetDateTime::UNIX_EPOCH,
        )
        .expect("coordinator")
    }

    fn event_item(wake_id: &str, session: &str) -> BrainWakeStreamItem {
        BrainWakeStreamItem::event(BrainEventEnvelope {
            wake_id: wake_id.to_string(),
            session_id: session_id(session),
            event: BrainEvent::Started,
        })
    }

    fn brain_event_item(event: BrainEvent) -> BrainWakeStreamItem {
        BrainWakeStreamItem::event(BrainEventEnvelope {
            wake_id: "wake-1".to_string(),
            session_id: session_id("session-1"),
            event,
        })
    }

    fn actions_item() -> BrainWakeStreamItem {
        BrainWakeStreamItem::actions(BrainActionBatch {
            wake_id: "wake-1".to_string(),
            session_id: session_id("session-1"),
            actions: Vec::new(),
        })
    }

    fn tool_request(call_id: &str) -> BufferedNeutralPendingToolRequest {
        BufferedNeutralPendingToolRequest {
            call_id: call_id.to_string(),
            provider_item_id: None,
            name: "read_file".to_string(),
            arguments_json: "{}".to_string(),
        }
    }

    fn tool_output(value: &str) -> BufferedNeutralToolOutput {
        BufferedNeutralToolOutput {
            output: value.to_string(),
            is_error: false,
            state_fingerprint: String::new(),
        }
    }

    #[test]
    fn only_created_turn_can_start() {
        let mut turn = coordinator();
        turn.start_at(OffsetDateTime::UNIX_EPOCH).expect("start");
        let error = turn.start().expect_err("second start must fail");
        assert_eq!(
            error,
            BufferedBrainTurnError::InvalidTransition {
                phase: BufferedBrainTurnPhase::Running,
                operation: "start",
            }
        );
    }

    #[test]
    fn stream_items_are_sequenced_and_terminal_once() {
        let mut turn = coordinator();
        turn.start().expect("start");
        assert_eq!(
            turn.enqueue_stream_item(event_item("wake-1", "session-1")),
            Ok(1)
        );
        assert_eq!(turn.enqueue_stream_item(actions_item()), Ok(2));
        assert_eq!(turn.phase(), BufferedBrainTurnPhase::Completed);

        let error = turn
            .enqueue_stream_item(event_item("wake-1", "session-1"))
            .expect_err("terminal turn rejects more stream items");
        assert!(matches!(
            error,
            BufferedBrainTurnError::InvalidTransition {
                phase: BufferedBrainTurnPhase::Completed,
                ..
            }
        ));

        let first = turn.drain_stream(1);
        assert_eq!(first.items.len(), 1);
        assert!(!first.terminal);
        let second = turn.drain_stream(8);
        assert_eq!(second.items.len(), 1);
        assert!(second.terminal);
        assert!(turn.drain_stream(8).terminal);
    }

    #[test]
    fn stream_identity_is_enforced() {
        let mut turn = coordinator();
        turn.start().expect("start");
        assert!(matches!(
            turn.enqueue_stream_item(event_item("other", "session-1")),
            Err(BufferedBrainTurnError::WakeIdentityMismatch { .. })
        ));
        assert!(matches!(
            turn.enqueue_stream_item(event_item("wake-1", "other")),
            Err(BufferedBrainTurnError::SessionIdentityMismatch { .. })
        ));
    }

    #[test]
    fn provider_stream_enqueue_failure_is_terminal_and_visible() {
        let mut turn = coordinator();
        turn.start().expect("start");
        let error = turn
            .enqueue_provider_stream_item(event_item("other", "session-1"))
            .expect_err("mismatched provider stream identity must fail");
        assert!(matches!(
            error,
            BufferedBrainTurnError::WakeIdentityMismatch { .. }
        ));
        assert_eq!(turn.phase(), BufferedBrainTurnPhase::Failed);
        assert_eq!(
            turn.terminal()
                .map(|terminal| terminal.reason_code.as_str()),
            Some("provider_stream_enqueue_failed")
        );
        assert!(turn
            .terminal()
            .expect("terminal")
            .summary
            .contains("wake id mismatch"));
    }

    #[test]
    fn empty_successful_tool_output_is_ready_exactly_once() {
        let mut turn = coordinator();
        turn.start().expect("start");
        turn.queue_tool_request(tool_request("call-empty"))
            .expect("queue");
        turn.submit_tool_output("call-empty", tool_output(""))
            .expect("submit empty output");
        assert_eq!(
            turn.poll_submitted_tool_output("call-empty"),
            BufferedNeutralToolOutputPoll::Ready(tool_output(""))
        );
        assert_eq!(
            turn.poll_submitted_tool_output("call-empty"),
            BufferedNeutralToolOutputPoll::Pending
        );
    }

    #[test]
    fn wake_failed_terminal_uses_core_error_reason_code() {
        let mut turn = coordinator();
        turn.start().expect("start");
        turn.enqueue_stream_item(BrainWakeStreamItem::wake_failed(BrainWakeFailure {
            wake_id: "wake-1".to_string(),
            session_id: SessionId::new("session-1"),
            kind: CoreErrorKind::AdapterUnavailable,
            reason_code: None,
            message: "adapter offline".to_string(),
        }))
        .expect("enqueue failure");
        assert_eq!(
            turn.terminal()
                .map(|terminal| terminal.reason_code.as_str()),
            Some("adapter_unavailable")
        );
    }

    #[test]
    fn wake_failed_terminal_preserves_specific_reason_code() {
        let mut turn = coordinator();
        turn.start().expect("start");
        turn.enqueue_stream_item(BrainWakeStreamItem::wake_failed(BrainWakeFailure {
            wake_id: "wake-1".to_string(),
            session_id: SessionId::new("session-1"),
            kind: CoreErrorKind::BrainUnavailable,
            reason_code: Some("provider_request_timeout".to_string()),
            message: "provider request timed out".to_string(),
        }))
        .expect("enqueue failure");
        assert_eq!(
            turn.terminal()
                .map(|terminal| terminal.reason_code.as_str()),
            Some("provider_request_timeout")
        );
    }

    #[test]
    fn tool_requests_pause_and_results_resume_the_turn() {
        let mut turn = coordinator();
        turn.start().expect("start");
        turn.queue_tool_request(tool_request("call-1"))
            .expect("queue first");
        turn.queue_tool_request(tool_request("call-2"))
            .expect("queue second");
        assert_eq!(turn.phase(), BufferedBrainTurnPhase::AwaitingHostTools);
        assert_eq!(turn.drain_host_tool_requests(8).len(), 2);
        assert!(turn.drain_host_tool_requests(8).is_empty());

        assert_eq!(
            turn.submit_tool_output("call-1", tool_output("one")),
            Ok(BufferedBrainToolResultReceipt::Accepted)
        );
        assert_eq!(turn.phase(), BufferedBrainTurnPhase::AwaitingHostTools);
        assert_eq!(
            turn.submit_tool_output("call-2", tool_output("two")),
            Ok(BufferedBrainToolResultReceipt::Accepted)
        );
        assert_eq!(turn.phase(), BufferedBrainTurnPhase::Running);
        assert_eq!(
            turn.take_submitted_tool_output("call-1"),
            Some(tool_output("one"))
        );
    }

    #[test]
    fn tool_result_submission_is_idempotent_but_conflicts_fail() {
        let mut turn = coordinator();
        turn.start().expect("start");
        turn.queue_tool_request(tool_request("call-1"))
            .expect("queue");
        assert_eq!(
            turn.submit_tool_output("call-1", tool_output("same")),
            Ok(BufferedBrainToolResultReceipt::Accepted)
        );
        assert_eq!(
            turn.submit_tool_output("call-1", tool_output("same")),
            Ok(BufferedBrainToolResultReceipt::Duplicate)
        );
        assert_eq!(
            turn.submit_tool_output("call-1", tool_output("different")),
            Err(BufferedBrainTurnError::ConflictingToolResult {
                call_id: "call-1".to_string(),
            })
        );
    }

    #[test]
    fn host_tool_policy_returns_single_denial_to_provider_without_stopping() {
        let mut turn = coordinator();
        turn.start().expect("start");
        turn.queue_tool_request(tool_request("call-1"))
            .expect("queue");
        let submission = turn
            .submit_host_tool_result(
                "call-1",
                BufferedBrainHostToolResult::denied(
                    "manual review required",
                    "memory_manual_review_required",
                ),
            )
            .expect("submit denial");
        assert_eq!(turn.phase(), BufferedBrainTurnPhase::Running);
        assert!(submission.decision.provider_output.is_error);
        assert!(submission.decision.recovery_guidance.is_none());
        assert_eq!(
            turn.take_submitted_tool_output("call-1"),
            Some(submission.decision.provider_output)
        );
    }

    #[test]
    fn host_tool_turn_disposition_survives_until_provider_poll_exactly_once() {
        let mut turn = coordinator();
        turn.start().expect("start");
        turn.queue_tool_request(tool_request("call-1"))
            .expect("queue");
        let mut result = BufferedBrainHostToolResult::succeeded("completion accepted");
        result.turn_disposition = Some(BufferedBrainHostTurnDisposition::CompleteTurn);

        turn.submit_host_tool_result("call-1", result)
            .expect("submit completion result");

        let ready = turn
            .poll_submitted_host_tool_output("call-1")
            .expect("provider output");
        assert_eq!(ready.output.output, "completion accepted");
        assert_eq!(
            ready.turn_disposition,
            Some(BufferedBrainHostTurnDisposition::CompleteTurn)
        );
        assert!(turn.poll_submitted_host_tool_output("call-1").is_none());
    }

    #[test]
    fn repeated_host_failure_returns_guidance_and_later_success_can_complete() {
        let mut turn = coordinator();
        turn.start().expect("start");
        let failure = BufferedBrainHostToolResult::failed(
            "Tool den_get_document is unavailable",
            "tool_unavailable",
            false,
        );

        turn.queue_tool_request(tool_request("call-1"))
            .expect("first request");
        let first = turn
            .submit_host_tool_result("call-1", failure.clone())
            .expect("first failure");
        assert!(first.decision.recovery_guidance.is_none());
        assert_eq!(turn.phase(), BufferedBrainTurnPhase::Running);

        turn.queue_tool_request(tool_request("call-2"))
            .expect("second request");
        let second = turn
            .submit_host_tool_result("call-2", failure.clone())
            .expect("second failure");
        let guidance = second
            .decision
            .recovery_guidance
            .as_ref()
            .expect("recovery guidance");
        assert_eq!(guidance.reason_code, "repeated_tool_failure_guidance");
        assert!(guidance
            .guidance
            .contains("Tool failure count this turn: 2."));
        assert!(guidance.guidance.contains("read_file: tool_unavailable"));
        assert_eq!(
            second.decision.provider_output.output,
            "Tool den_get_document is unavailable"
        );
        assert_eq!(turn.phase(), BufferedBrainTurnPhase::Running);
        assert!(turn.terminal().is_none());

        let duplicate = turn
            .submit_host_tool_result("call-2", failure)
            .expect("idempotent duplicate after terminal");
        assert_eq!(duplicate.receipt, BufferedBrainToolResultReceipt::Duplicate);
        assert_eq!(turn.tool_failure_policy().total_failures(), 2);

        turn.queue_tool_request(tool_request("call-3"))
            .expect("corrected request");
        let success = turn
            .submit_host_tool_result(
                "call-3",
                BufferedBrainHostToolResult::succeeded("corrected call succeeded"),
            )
            .expect("corrected success");
        assert!(success.decision.recovery_guidance.is_none());
        assert_eq!(turn.phase(), BufferedBrainTurnPhase::Running);
        assert_eq!(
            turn.take_submitted_tool_output("call-3"),
            Some(success.decision.provider_output)
        );
        turn.enqueue_stream_item(actions_item())
            .expect("assistant can finish after recovery");
        assert_eq!(turn.phase(), BufferedBrainTurnPhase::Completed);
    }

    #[test]
    fn duplicate_and_unknown_tool_requests_fail_closed() {
        let mut turn = coordinator();
        turn.start().expect("start");
        turn.queue_tool_request(tool_request("call-1"))
            .expect("queue");
        assert_eq!(
            turn.queue_tool_request(tool_request("call-1")),
            Err(BufferedBrainTurnError::DuplicateToolRequest {
                call_id: "call-1".to_string(),
            })
        );
        assert_eq!(
            turn.submit_tool_output("missing", tool_output("nope")),
            Err(BufferedBrainTurnError::UnknownToolRequest {
                call_id: "missing".to_string(),
            })
        );
    }

    #[test]
    fn cancellation_is_idempotent_and_clears_host_work() {
        let mut turn = coordinator();
        turn.start().expect("start");
        turn.queue_tool_request(tool_request("call-1"))
            .expect("queue");
        turn.cancel("user_cancelled", "stopped").expect("cancel");
        turn.cancel("user_cancelled", "stopped").expect("repeat");
        assert_eq!(turn.phase(), BufferedBrainTurnPhase::Cancelled);
        assert_eq!(turn.pending_tool_request_count(), 0);
        assert_eq!(turn.submitted_tool_output_count(), 0);
        assert_eq!(
            turn.terminal()
                .map(|terminal| terminal.reason_code.as_str()),
            Some("user_cancelled")
        );
    }

    #[test]
    fn operator_attention_can_always_be_cancelled_explicitly() {
        let mut turn = coordinator();
        turn.start().expect("start");
        turn.require_attention("tool_no_progress", "operator decision required")
            .expect("attention");

        turn.cancel("user_cancelled", "stopped while paused")
            .expect("cancel attention");

        assert_eq!(turn.phase(), BufferedBrainTurnPhase::Cancelled);
        assert_eq!(turn.pending_tool_request_count(), 0);
        assert_eq!(
            turn.terminal()
                .map(|terminal| terminal.reason_code.as_str()),
            Some("user_cancelled")
        );
    }

    #[test]
    fn first_terminal_transition_wins_after_completion_or_cancellation() {
        let mut completed = coordinator();
        completed.start().expect("start");
        completed
            .enqueue_stream_item(actions_item())
            .expect("complete");
        assert!(matches!(
            completed.cancel("late_cancel", "too late"),
            Err(BufferedBrainTurnError::InvalidTransition {
                phase: BufferedBrainTurnPhase::Completed,
                ..
            })
        ));
        assert_eq!(completed.phase(), BufferedBrainTurnPhase::Completed);

        let mut cancelled = coordinator();
        cancelled
            .start_at(OffsetDateTime::UNIX_EPOCH)
            .expect("start");
        cancelled
            .cancel_at("user_cancelled", "stopped", OffsetDateTime::UNIX_EPOCH)
            .expect("cancel");
        assert_eq!(cancelled.phase(), BufferedBrainTurnPhase::Cancelled);
    }

    #[test]
    fn stream_pending_tool_and_output_limits_apply_recoverable_backpressure() {
        let limits = BufferedBrainTurnLimits {
            max_stream_items: 1,
            max_stream_delta_bytes: 1_024,
            max_pending_tool_requests: 1,
            max_tool_results: 1,
            max_tool_output_bytes: 3,
        };
        let mut stream_turn = BufferedBrainTurnCoordinator::new(
            "chat-completions",
            "wake-1",
            session_id("session-1"),
            limits,
        )
        .expect("coordinator");
        stream_turn.start().expect("start");
        stream_turn
            .enqueue_stream_item(event_item("wake-1", "session-1"))
            .expect("first item");
        assert!(matches!(
            stream_turn.enqueue_stream_item(event_item("wake-1", "session-1")),
            Err(BufferedBrainTurnError::BufferLimitExceeded {
                buffer: "stream_items",
                ..
            })
        ));
        assert_eq!(stream_turn.phase(), BufferedBrainTurnPhase::Running);
        assert_eq!(stream_turn.drain_stream(1).items.len(), 1);
        stream_turn
            .enqueue_stream_item(event_item("wake-1", "session-1"))
            .expect("retry after drain");

        let mut tool_turn = BufferedBrainTurnCoordinator::new(
            "chat-completions",
            "wake-1",
            session_id("session-1"),
            limits,
        )
        .expect("coordinator");
        tool_turn.start().expect("start");
        tool_turn
            .queue_tool_request(tool_request("call-1"))
            .expect("first request");
        assert!(matches!(
            tool_turn.queue_tool_request(tool_request("call-2")),
            Err(BufferedBrainTurnError::BufferLimitExceeded {
                buffer: "pending_tool_requests",
                ..
            })
        ));
        assert_eq!(tool_turn.phase(), BufferedBrainTurnPhase::AwaitingHostTools);
        tool_turn
            .submit_tool_output("call-1", tool_output("one"))
            .expect("first result");
        tool_turn
            .queue_tool_request(tool_request("call-2"))
            .expect("request after host result");
        assert!(matches!(
            tool_turn.submit_tool_output("call-2", tool_output("two")),
            Err(BufferedBrainTurnError::BufferLimitExceeded {
                buffer: "tool_results",
                ..
            })
        ));
        assert!(tool_turn.take_submitted_tool_output("call-1").is_some());
        tool_turn
            .submit_tool_output("call-2", tool_output("two"))
            .expect("result retry after provider consumption");

        let mut output_turn = BufferedBrainTurnCoordinator::new(
            "chat-completions",
            "wake-1",
            session_id("session-1"),
            limits,
        )
        .expect("coordinator");
        output_turn.start().expect("start");
        output_turn
            .queue_tool_request(tool_request("call-1"))
            .expect("request");
        assert!(matches!(
            output_turn.submit_tool_output("call-1", tool_output("four")),
            Err(BufferedBrainTurnError::ToolOutputTooLarge { .. })
        ));
    }

    #[test]
    fn fine_grained_deltas_coalesce_without_crowding_out_tools_or_terminal() {
        let mut turn = BufferedBrainTurnCoordinator::new(
            "chat-completions",
            "wake-1",
            session_id("session-1"),
            BufferedBrainTurnLimits {
                max_stream_items: 16,
                max_stream_delta_bytes: 8 * 1_024,
                ..BufferedBrainTurnLimits::default()
            },
        )
        .expect("coordinator");
        turn.start().expect("start");

        for _ in 0..2_500 {
            turn.enqueue_stream_item(brain_event_item(BrainEvent::ReasoningDelta {
                text: "r".to_string(),
                format: Some("provider-reasoning".to_string()),
            }))
            .expect("reasoning delta");
        }
        turn.enqueue_stream_item(brain_event_item(BrainEvent::ToolCallStarted {
            tool_name: "read_file".to_string(),
            metadata: None,
        }))
        .expect("tool start");
        turn.enqueue_stream_item(brain_event_item(BrainEvent::ToolCallFinished {
            tool_name: "read_file".to_string(),
            is_error: false,
            metadata: None,
        }))
        .expect("tool finish");
        for _ in 0..2_500 {
            turn.enqueue_stream_item(brain_event_item(BrainEvent::TextDelta {
                text: "t".to_string(),
            }))
            .expect("text delta");
        }
        turn.enqueue_stream_item(actions_item())
            .expect("terminal actions");

        let metrics = turn.stream_retention_metrics();
        assert_eq!(metrics.raw_stream_item_count, 5_003);
        assert_eq!(metrics.raw_delta_item_count, 5_000);
        assert_eq!(metrics.retained_stream_item_count, 5);
        assert_eq!(metrics.coalesced_delta_item_count, 4_998);
        assert_eq!(metrics.dropped_stream_item_count, 0);
        assert_eq!(metrics.retained_delta_bytes, 5_000);
        assert_eq!(metrics.queued_delta_bytes, 5_000);

        let drain = turn.drain_stream(16);
        assert!(drain.terminal);
        assert_eq!(drain.items.len(), 5);
        assert!(matches!(
            &drain.items[0].item,
            BrainWakeStreamItem::Event { event }
                if matches!(&event.event, BrainEvent::ReasoningDelta { text, format }
                    if text.len() == 2_500 && format.as_deref() == Some("provider-reasoning"))
        ));
        assert!(matches!(
            &drain.items[1].item,
            BrainWakeStreamItem::Event { event }
                if matches!(&event.event, BrainEvent::ToolCallStarted { .. })
        ));
        assert!(matches!(
            &drain.items[2].item,
            BrainWakeStreamItem::Event { event }
                if matches!(&event.event, BrainEvent::ToolCallFinished { .. })
        ));
        assert!(matches!(
            &drain.items[3].item,
            BrainWakeStreamItem::Event { event }
                if matches!(&event.event, BrainEvent::TextDelta { text } if text.len() == 2_500)
        ));
        assert!(matches!(
            &drain.items[4].item,
            BrainWakeStreamItem::Actions { .. }
        ));
        assert_eq!(turn.stream_retention_metrics().queued_delta_bytes, 0);
    }

    #[test]
    fn terminal_item_has_reserved_capacity_beyond_nonterminal_limit() {
        let mut turn = BufferedBrainTurnCoordinator::new(
            "chat-completions",
            "wake-1",
            session_id("session-1"),
            BufferedBrainTurnLimits {
                max_stream_items: 1,
                ..BufferedBrainTurnLimits::default()
            },
        )
        .expect("coordinator");
        turn.start().expect("start");
        turn.enqueue_stream_item(event_item("wake-1", "session-1"))
            .expect("nonterminal item");
        turn.enqueue_stream_item(actions_item())
            .expect("reserved terminal item");

        assert_eq!(turn.queued_stream_item_count(), 2);
        assert!(turn.drain_stream(2).terminal);

        let mut failed_turn = BufferedBrainTurnCoordinator::new(
            "chat-completions",
            "wake-1",
            session_id("session-1"),
            BufferedBrainTurnLimits {
                max_stream_items: 1,
                ..BufferedBrainTurnLimits::default()
            },
        )
        .expect("coordinator");
        failed_turn.start().expect("start");
        failed_turn
            .enqueue_stream_item(event_item("wake-1", "session-1"))
            .expect("nonterminal item");
        failed_turn
            .enqueue_stream_item(BrainWakeStreamItem::wake_failed(BrainWakeFailure {
                wake_id: "wake-1".to_string(),
                session_id: session_id("session-1"),
                kind: CoreErrorKind::BrainUnavailable,
                reason_code: Some("provider_failed".to_string()),
                message: "provider failed".to_string(),
            }))
            .expect("reserved failure item");
        let failed_drain = failed_turn.drain_stream(2);
        assert!(failed_drain.terminal);
        assert!(matches!(
            &failed_drain.items[1].item,
            BrainWakeStreamItem::WakeFailed { .. }
        ));
    }

    #[test]
    fn reasoning_deltas_with_different_formats_remain_distinct() {
        let mut turn = coordinator();
        turn.start().expect("start");
        for format in ["summary", "analysis"] {
            turn.enqueue_stream_item(brain_event_item(BrainEvent::ReasoningDelta {
                text: format.to_string(),
                format: Some(format.to_string()),
            }))
            .expect("reasoning delta");
        }
        turn.enqueue_stream_item(actions_item())
            .expect("terminal actions");

        let drain = turn.drain_stream(8);
        assert!(drain.terminal);
        assert_eq!(drain.items.len(), 3);
        assert_eq!(
            turn.stream_retention_metrics().coalesced_delta_item_count,
            0
        );
    }

    #[test]
    fn delta_byte_pressure_drains_and_retries_without_terminating() {
        let mut turn = BufferedBrainTurnCoordinator::new(
            "chat-completions",
            "wake-1",
            session_id("session-1"),
            BufferedBrainTurnLimits {
                max_stream_delta_bytes: 4,
                ..BufferedBrainTurnLimits::default()
            },
        )
        .expect("coordinator");
        turn.start().expect("start");
        turn.enqueue_stream_item(brain_event_item(BrainEvent::TextDelta {
            text: "abc".to_string(),
        }))
        .expect("first delta");
        assert_eq!(
            turn.enqueue_stream_item(brain_event_item(BrainEvent::TextDelta {
                text: "de".to_string(),
            })),
            Err(BufferedBrainTurnError::BufferLimitExceeded {
                buffer: "stream_delta_bytes",
                limit: 4,
            })
        );

        assert_eq!(turn.phase(), BufferedBrainTurnPhase::Running);
        assert!(turn.terminal().is_none());
        let metrics = turn.stream_retention_metrics();
        assert_eq!(metrics.raw_stream_item_count, 1);
        assert_eq!(metrics.raw_delta_item_count, 1);
        assert_eq!(metrics.retained_stream_item_count, 1);
        assert_eq!(metrics.coalesced_delta_item_count, 0);
        assert_eq!(metrics.dropped_stream_item_count, 0);
        assert_eq!(metrics.retained_delta_bytes, 3);
        assert_eq!(metrics.queued_delta_bytes, 3);

        let first = turn.drain_stream(8);
        assert!(!first.terminal);
        assert_eq!(first.items.len(), 1);
        turn.enqueue_stream_item(brain_event_item(BrainEvent::TextDelta {
            text: "de".to_string(),
        }))
        .expect("retry after drain");
        assert_eq!(turn.stream_retention_metrics().retained_delta_bytes, 5);
        assert_eq!(turn.stream_retention_metrics().queued_delta_bytes, 2);
    }

    #[test]
    fn provider_state_is_recorded_but_not_on_cancelled_turns() {
        let mut turn = coordinator();
        turn.start().expect("start");
        turn.set_provider_state_output(BrainWakeProviderStateOutput::Unchanged)
            .expect("state");
        assert!(matches!(
            turn.provider_state_output(),
            Some(BrainWakeProviderStateOutput::Unchanged)
        ));
        turn.cancel("cancelled", "stopped").expect("cancel");
        assert!(matches!(
            turn.set_provider_state_output(BrainWakeProviderStateOutput::Unchanged),
            Err(BufferedBrainTurnError::InvalidTransition {
                phase: BufferedBrainTurnPhase::Cancelled,
                ..
            })
        ));
    }

    #[test]
    fn wake_failure_stream_item_sets_failed_terminal_details() {
        let mut turn = coordinator();
        turn.start().expect("start");
        turn.enqueue_stream_item(BrainWakeStreamItem::wake_failed(BrainWakeFailure {
            wake_id: "wake-1".to_string(),
            session_id: session_id("session-1"),
            kind: CoreErrorKind::AdapterUnavailable,
            reason_code: None,
            message: "provider unavailable".to_string(),
        }))
        .expect("failure item");
        assert_eq!(turn.phase(), BufferedBrainTurnPhase::Failed);
        assert_eq!(
            turn.terminal().map(|terminal| terminal.summary.as_str()),
            Some("provider unavailable")
        );
    }

    #[test]
    fn zero_limits_are_rejected() {
        let error = BufferedBrainTurnCoordinator::new(
            "chat-completions",
            "wake-1",
            session_id("session-1"),
            BufferedBrainTurnLimits {
                max_stream_items: 0,
                ..BufferedBrainTurnLimits::default()
            },
        )
        .expect_err("zero limit");
        assert_eq!(
            error,
            BufferedBrainTurnError::InvalidLimit {
                name: "max_stream_items"
            }
        );
    }

    #[test]
    fn typed_registry_rejects_duplicates_and_cleans_active_runs() {
        let registry = BufferedBrainTurnRegistry::new("chat-completions");
        let mut first = coordinator();
        first.start().expect("start");
        registry
            .insert(BufferedBrainTurnRun::new(first, "first"))
            .expect("insert");

        let duplicate = registry
            .insert(BufferedBrainTurnRun::new(coordinator(), "duplicate"))
            .expect_err("duplicate wake");
        assert!(matches!(
            duplicate,
            BrainRuntimeError::DuplicateWake { wake_id, .. } if wake_id == "wake-1"
        ));

        let diagnostics = registry.diagnostics().expect("diagnostics");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].wake_id, "wake-1");
        assert!(!diagnostics[0].terminal);

        let cleanup = registry
            .cleanup("service_shutdown", "service stopping")
            .expect("cleanup");
        assert_eq!(cleanup.active_runs, 1);
        assert_eq!(cleanup.cancelled_nonterminal_runs, 1);
        assert_eq!(cleanup.removed_runs, 1);
        assert!(registry
            .diagnostics()
            .expect("empty diagnostics")
            .is_empty());
    }
}
