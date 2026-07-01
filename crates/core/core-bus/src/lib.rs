//! In-process coordination bus.

use rusty_crew_core_protocol::{
    AgentId, AgentMessage, CoreError, CoreErrorKind, CoreEvent, CoreEventKind, CoreResult,
    EventSubscription, SessionId,
};
use std::collections::VecDeque;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

const DEFAULT_HISTORY_LIMIT: usize = 1024;

pub type EventRecorder = Arc<dyn Fn(u64, &CoreEvent) -> CoreResult<()> + Send + Sync>;

#[derive(Clone)]
pub struct CoreBus {
    inner: Arc<Inner>,
}

struct Inner {
    next_subscription: AtomicU64,
    next_sequence: AtomicU64,
    subscribers: Mutex<Vec<Subscriber>>,
    history: Mutex<VecDeque<SequencedEvent>>,
    history_limit: usize,
    recorder: Option<EventRecorder>,
}

impl fmt::Debug for CoreBus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CoreBus")
            .field("history_limit", &self.inner.history_limit)
            .field("has_recorder", &self.inner.recorder.is_some())
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
struct Subscriber {
    id: u64,
    filter: EventSubscription,
    sender: Sender<CoreEvent>,
}

#[derive(Debug, Clone)]
pub struct SequencedEvent {
    pub sequence: u64,
    pub event: CoreEvent,
}

impl CoreBus {
    pub fn new() -> Self {
        Self::with_history_and_recorder(Vec::new(), None)
    }

    pub fn with_history(history: Vec<SequencedEvent>) -> Self {
        Self::with_history_and_recorder(history, None)
    }

    pub fn with_history_and_recorder(
        history: Vec<SequencedEvent>,
        recorder: Option<EventRecorder>,
    ) -> Self {
        let next_sequence = history
            .iter()
            .map(|entry| entry.sequence)
            .max()
            .unwrap_or(0)
            + 1;

        Self {
            inner: Arc::new(Inner {
                next_subscription: AtomicU64::new(1),
                next_sequence: AtomicU64::new(next_sequence),
                subscribers: Mutex::new(Vec::new()),
                history: Mutex::new(history.into_iter().collect()),
                history_limit: DEFAULT_HISTORY_LIMIT,
                recorder,
            }),
        }
    }

    pub fn subscribe(&self, filter: EventSubscription) -> CoreResult<(u64, Receiver<CoreEvent>)> {
        let id = self.inner.next_subscription.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = mpsc::channel();
        self.inner
            .subscribers
            .lock()
            .map_err(|_| CoreError::new(CoreErrorKind::InternalError, "subscriber lock poisoned"))?
            .push(Subscriber { id, filter, sender });
        Ok((id, receiver))
    }

    pub fn unsubscribe(&self, id: u64) -> CoreResult<()> {
        let mut subscribers = self.inner.subscribers.lock().map_err(|_| {
            CoreError::new(CoreErrorKind::InternalError, "subscriber lock poisoned")
        })?;
        subscribers.retain(|subscriber| subscriber.id != id);
        Ok(())
    }

    pub fn shutdown_subscribers(&self) -> CoreResult<u32> {
        let mut subscribers = self.inner.subscribers.lock().map_err(|_| {
            CoreError::new(CoreErrorKind::InternalError, "subscriber lock poisoned")
        })?;
        let dropped = subscribers.len() as u32;
        subscribers.clear();
        Ok(dropped)
    }

    pub fn route_message(
        &self,
        from: AgentId,
        to: AgentId,
        body: impl Into<String>,
    ) -> CoreResult<()> {
        self.route_agent_message(AgentMessage {
            from,
            to,
            body: body.into(),
            correlation_id: None,
            projection: None,
        })
    }

    pub fn route_agent_message(&self, message: AgentMessage) -> CoreResult<()> {
        self.publish(CoreEvent::AgentMessageRouted { message })?;
        Ok(())
    }

    pub fn publish(&self, event: CoreEvent) -> CoreResult<u64> {
        let sequence = self.inner.next_sequence.fetch_add(1, Ordering::Relaxed);
        if let Some(recorder) = &self.inner.recorder {
            recorder(sequence, &event)?;
        }

        {
            let mut history = self.inner.history.lock().map_err(|_| {
                CoreError::new(CoreErrorKind::InternalError, "history lock poisoned")
            })?;
            history.push_back(SequencedEvent {
                sequence,
                event: event.clone(),
            });
            while history.len() > self.inner.history_limit {
                history.pop_front();
            }
        }

        let mut subscribers = self.inner.subscribers.lock().map_err(|_| {
            CoreError::new(CoreErrorKind::InternalError, "subscriber lock poisoned")
        })?;

        subscribers.retain(|subscriber| {
            !event_matches_filter(&event, &subscriber.filter)
                || subscriber.sender.send(event.clone()).is_ok()
        });
        Ok(sequence)
    }

    pub fn recent_events_for_session(
        &self,
        session_id: &SessionId,
        limit: usize,
    ) -> CoreResult<Vec<CoreEvent>> {
        let history =
            self.inner.history.lock().map_err(|_| {
                CoreError::new(CoreErrorKind::InternalError, "history lock poisoned")
            })?;

        Ok(history
            .iter()
            .rev()
            .filter(|entry| event_mentions_session(&entry.event, session_id))
            .take(limit)
            .map(|entry| entry.event.clone())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect())
    }

    pub fn pending_messages_for_agent(&self, agent_id: &AgentId) -> CoreResult<Vec<AgentMessage>> {
        let history =
            self.inner.history.lock().map_err(|_| {
                CoreError::new(CoreErrorKind::InternalError, "history lock poisoned")
            })?;

        Ok(history
            .iter()
            .filter_map(|entry| match &entry.event {
                CoreEvent::AgentMessageRouted { message } if &message.to == agent_id => {
                    Some(message.clone())
                }
                _ => None,
            })
            .collect())
    }
}

impl Default for CoreBus {
    fn default() -> Self {
        Self::new()
    }
}

pub fn event_matches_filter(event: &CoreEvent, filter: &EventSubscription) -> bool {
    if !filter.event_kinds.is_empty() && !filter.event_kinds.contains(&CoreEventKind::of(event)) {
        return false;
    }

    if let Some(session_id) = &filter.session_id {
        if !event_mentions_session(event, session_id) {
            return false;
        }
    }

    if let Some(agent_id) = &filter.agent_id {
        if !event_mentions_agent(event, agent_id) {
            return false;
        }
    }

    if let Some(adapter_id) = &filter.adapter_id {
        if !event_mentions_adapter(event, adapter_id) {
            return false;
        }
    }

    true
}

fn event_mentions_session(event: &CoreEvent, session_id: &SessionId) -> bool {
    match event {
        CoreEvent::SessionCreated { state } => &state.session_id == session_id,
        CoreEvent::SessionArchived {
            session_id: archived,
        } => archived == session_id,
        CoreEvent::BrainWakeRequested { session_id: wake } => wake == session_id,
        CoreEvent::BrainEventObserved {
            session_id: observed,
            ..
        } => observed == session_id,
        CoreEvent::BrainActionsAccepted {
            session_id: accepted,
            ..
        } => accepted == session_id,
        CoreEvent::CompletionPacketDelivered { packet } => &packet.session_id == session_id,
        CoreEvent::DelegationLifecycleObserved { lifecycle } => {
            &lifecycle.parent_session_id == session_id
                || &lifecycle.delegated_session_id == session_id
        }
        CoreEvent::AgentMessageRouted { .. }
        | CoreEvent::ExternalEventInjected { .. }
        | CoreEvent::DenDataUpdated { .. } => false,
    }
}

fn event_mentions_agent(event: &CoreEvent, agent_id: &AgentId) -> bool {
    match event {
        CoreEvent::SessionCreated { state } => &state.agent_id == agent_id,
        CoreEvent::AgentMessageRouted { message } => {
            &message.from == agent_id || &message.to == agent_id
        }
        CoreEvent::SessionArchived { .. }
        | CoreEvent::DelegationLifecycleObserved { .. }
        | CoreEvent::ExternalEventInjected { .. }
        | CoreEvent::DenDataUpdated { .. }
        | CoreEvent::BrainWakeRequested { .. }
        | CoreEvent::BrainEventObserved { .. }
        | CoreEvent::BrainActionsAccepted { .. }
        | CoreEvent::CompletionPacketDelivered { .. } => false,
    }
}

fn event_mentions_adapter(
    event: &CoreEvent,
    adapter_id: &rusty_crew_core_protocol::AdapterId,
) -> bool {
    match event {
        CoreEvent::ExternalEventInjected { event } => &event.adapter_id == adapter_id,
        CoreEvent::SessionCreated { .. }
        | CoreEvent::SessionArchived { .. }
        | CoreEvent::AgentMessageRouted { .. }
        | CoreEvent::DelegationLifecycleObserved { .. }
        | CoreEvent::DenDataUpdated { .. }
        | CoreEvent::BrainWakeRequested { .. }
        | CoreEvent::BrainEventObserved { .. }
        | CoreEvent::BrainActionsAccepted { .. }
        | CoreEvent::CompletionPacketDelivered { .. } => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_messages_to_subscribers() {
        let bus = CoreBus::new();
        let (_id, events) = bus
            .subscribe(EventSubscription {
                event_kinds: Vec::new(),
                session_id: None,
                agent_id: None,
                adapter_id: None,
            })
            .unwrap();

        bus.route_message(AgentId::new("planner"), AgentId::new("coder"), "hello")
            .unwrap();

        let event = events.recv().unwrap();
        let CoreEvent::AgentMessageRouted { message } = event else {
            panic!("expected routed message");
        };
        assert_eq!(message.body, "hello");
        assert_eq!(message.projection, None);
    }

    #[test]
    fn routes_full_messages_with_projection_hints() {
        let bus = CoreBus::new();
        let (_id, events) = bus
            .subscribe(EventSubscription {
                event_kinds: vec![CoreEventKind::AgentMessageRouted],
                session_id: None,
                agent_id: None,
                adapter_id: None,
            })
            .unwrap();

        bus.route_agent_message(AgentMessage {
            from: AgentId::new("planner"),
            to: AgentId::new("operator"),
            body: "ready".to_string(),
            correlation_id: Some("handoff-1".to_string()),
            projection: Some(rusty_crew_core_protocol::AgentMessageProjectionHint {
                visibility: rusty_crew_core_protocol::ProjectionVisibility::Observation,
                target_ref: Some(rusty_crew_core_protocol::ProjectionRef {
                    system: "den".to_string(),
                    kind: "project".to_string(),
                    id: "rusty-crew".to_string(),
                }),
                work_ref: Some(rusty_crew_core_protocol::ProjectionRef {
                    system: "den".to_string(),
                    kind: "task".to_string(),
                    id: "3875".to_string(),
                }),
                reason: Some("projection proof".to_string()),
            }),
        })
        .unwrap();

        let event = events.recv().unwrap();
        let CoreEvent::AgentMessageRouted { message } = event else {
            panic!("expected routed message");
        };
        let projection = message.projection.expect("projection hint");
        assert_eq!(projection.target_ref.expect("target ref").id, "rusty-crew");
        assert_eq!(projection.work_ref.expect("work ref").id, "3875");
    }
}
