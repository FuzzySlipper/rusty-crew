//! In-process coordination bus.

use rusty_crew_core_protocol::{
    AgentId, AgentMessage, CoreError, CoreErrorKind, CoreEvent, CoreResult,
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct CoreBus {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    next_subscription: AtomicU64,
    subscribers: Mutex<Vec<Subscriber>>,
}

#[derive(Debug)]
struct Subscriber {
    id: u64,
    sender: Sender<CoreEvent>,
}

impl CoreBus {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                next_subscription: AtomicU64::new(1),
                subscribers: Mutex::new(Vec::new()),
            }),
        }
    }

    pub fn subscribe(&self) -> CoreResult<(u64, Receiver<CoreEvent>)> {
        let id = self.inner.next_subscription.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = mpsc::channel();
        self.inner
            .subscribers
            .lock()
            .map_err(|_| CoreError::new(CoreErrorKind::InternalError, "subscriber lock poisoned"))?
            .push(Subscriber { id, sender });
        Ok((id, receiver))
    }

    pub fn unsubscribe(&self, id: u64) -> CoreResult<()> {
        let mut subscribers = self.inner.subscribers.lock().map_err(|_| {
            CoreError::new(CoreErrorKind::InternalError, "subscriber lock poisoned")
        })?;
        subscribers.retain(|subscriber| subscriber.id != id);
        Ok(())
    }

    pub fn route_message(
        &self,
        from: AgentId,
        to: AgentId,
        body: impl Into<String>,
    ) -> CoreResult<()> {
        self.publish(CoreEvent::AgentMessageRouted {
            message: AgentMessage {
                from,
                to,
                body: body.into(),
                correlation_id: None,
            },
        })
    }

    pub fn publish(&self, event: CoreEvent) -> CoreResult<()> {
        let mut subscribers = self.inner.subscribers.lock().map_err(|_| {
            CoreError::new(CoreErrorKind::InternalError, "subscriber lock poisoned")
        })?;

        subscribers.retain(|subscriber| subscriber.sender.send(event.clone()).is_ok());
        Ok(())
    }
}

impl Default for CoreBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_messages_to_subscribers() {
        let bus = CoreBus::new();
        let (_id, events) = bus.subscribe().unwrap();

        bus.route_message(AgentId::new("planner"), AgentId::new("coder"), "hello")
            .unwrap();

        let event = events.recv().unwrap();
        assert!(matches!(event, CoreEvent::AgentMessageRouted { .. }));
    }
}
