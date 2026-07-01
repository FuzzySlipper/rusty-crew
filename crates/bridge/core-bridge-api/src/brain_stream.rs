use std::sync::mpsc;

use rusty_crew_core_protocol::{
    BrainActionBatch, BrainEventEnvelope, BrainWakeFailure, BrainWakeStreamItem, CoreError,
    CoreErrorKind, CoreResult, SessionId,
};

#[derive(Debug)]
pub struct BrainWakeStream {
    receiver: mpsc::Receiver<CoreResult<BrainWakeStreamItem>>,
}

#[derive(Debug, Clone)]
pub struct BrainWakeStreamSender {
    sender: mpsc::Sender<CoreResult<BrainWakeStreamItem>>,
}

pub fn brain_wake_stream_channel() -> (BrainWakeStreamSender, BrainWakeStream) {
    let (sender, receiver) = mpsc::channel();
    (
        BrainWakeStreamSender { sender },
        BrainWakeStream { receiver },
    )
}

impl BrainWakeStream {
    pub fn from_items(items: impl IntoIterator<Item = BrainWakeStreamItem>) -> Self {
        let (sender, stream) = brain_wake_stream_channel();
        for item in items {
            sender.send(item).expect("local stream receiver is alive");
        }
        stream
    }

    pub fn from_event_batch(events: Vec<BrainEventEnvelope>, batch: BrainActionBatch) -> Self {
        Self::from_items(
            events
                .into_iter()
                .map(BrainWakeStreamItem::event)
                .chain(std::iter::once(BrainWakeStreamItem::actions(batch))),
        )
    }

    pub fn wake_failed(
        wake_id: impl Into<String>,
        session_id: SessionId,
        kind: CoreErrorKind,
        message: impl Into<String>,
    ) -> Self {
        Self::from_items([BrainWakeStreamItem::wake_failed(BrainWakeFailure {
            wake_id: wake_id.into(),
            session_id,
            kind,
            message: message.into(),
        })])
    }

    pub fn recv(&self) -> CoreResult<BrainWakeStreamItem> {
        self.receiver.recv().map_err(|error| {
            CoreError::new(
                CoreErrorKind::BrainUnavailable,
                format!("brain wake stream closed before terminal item: {error}"),
            )
        })?
    }

    pub fn try_recv(&self) -> Option<CoreResult<BrainWakeStreamItem>> {
        match self.receiver.try_recv() {
            Ok(item) => Some(item),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err(CoreError::new(
                CoreErrorKind::BrainUnavailable,
                "brain wake stream closed before terminal item",
            ))),
        }
    }

    pub fn drain_until_terminal(&self) -> CoreResult<Vec<BrainWakeStreamItem>> {
        let mut items = Vec::new();
        loop {
            let item = self.recv()?;
            let terminal = item.is_terminal();
            items.push(item);
            if terminal {
                return Ok(items);
            }
        }
    }
}

impl BrainWakeStreamSender {
    pub fn send(&self, item: BrainWakeStreamItem) -> CoreResult<()> {
        self.sender.send(Ok(item)).map_err(|error| {
            CoreError::new(
                CoreErrorKind::BrainUnavailable,
                format!("brain wake stream receiver closed: {error}"),
            )
        })
    }

    pub fn send_error(&self, error: CoreError) -> CoreResult<()> {
        self.sender.send(Err(error)).map_err(|error| {
            CoreError::new(
                CoreErrorKind::BrainUnavailable,
                format!("brain wake stream receiver closed: {error}"),
            )
        })
    }
}

pub trait BrainWakeStreamProducer {
    fn wake_stream(
        &self,
        request: rusty_crew_core_protocol::BrainWakeRequest,
    ) -> CoreResult<BrainWakeStream>;
}

#[cfg(test)]
mod tests {
    use rusty_crew_core_protocol::{
        AgentMessage, BrainAction, BrainEvent, BrainImplementationHandle, BrainWakeRequest,
        RuntimeBufferHandle,
    };

    use super::*;

    struct FakeDirectRustBrain;

    impl BrainWakeStreamProducer for FakeDirectRustBrain {
        fn wake_stream(&self, request: BrainWakeRequest) -> CoreResult<BrainWakeStream> {
            Ok(BrainWakeStream::from_event_batch(
                vec![
                    BrainEventEnvelope {
                        wake_id: request.wake_id.clone(),
                        session_id: request.session_id.clone(),
                        event: BrainEvent::Started,
                    },
                    BrainEventEnvelope {
                        wake_id: request.wake_id.clone(),
                        session_id: request.session_id.clone(),
                        event: BrainEvent::TextDelta {
                            text: "direct rust brain streamed".to_string(),
                        },
                    },
                    BrainEventEnvelope {
                        wake_id: request.wake_id.clone(),
                        session_id: request.session_id.clone(),
                        event: BrainEvent::ProviderStatus {
                            level: rusty_crew_core_protocol::BrainProviderStatusLevel::Info,
                            message: "provider stream connected".to_string(),
                            metadata_json: None,
                        },
                    },
                    BrainEventEnvelope {
                        wake_id: request.wake_id.clone(),
                        session_id: request.session_id.clone(),
                        event: BrainEvent::Finished,
                    },
                ],
                BrainActionBatch {
                    wake_id: request.wake_id,
                    session_id: request.session_id,
                    actions: vec![BrainAction::SendMessage {
                        message: AgentMessage {
                            from: rusty_crew_core_protocol::AgentId::new("direct-rust"),
                            to: rusty_crew_core_protocol::AgentId::new("operator"),
                            body: "done".to_string(),
                            correlation_id: None,
                            projection: None,
                        },
                    }],
                },
            ))
        }
    }

    #[test]
    fn fake_direct_rust_brain_streams_events_then_actions() {
        let stream = FakeDirectRustBrain
            .wake_stream(BrainWakeRequest {
                brain: BrainImplementationHandle::new(7),
                session_id: SessionId::new("session-1"),
                body_state: RuntimeBufferHandle::new(1),
                system_prompt: RuntimeBufferHandle::new(2),
                role_assembly: RuntimeBufferHandle::new(3),
                wake_id: "wake-1".to_string(),
                provider_state: None,
                provider_state_absence: Some(
                    rusty_crew_core_protocol::ProviderStateAbsenceReason::ModuleDoesNotUseState,
                ),
            })
            .expect("wake stream");

        let items = stream.drain_until_terminal().expect("drain stream");
        assert_eq!(items.len(), 5);
        assert!(matches!(
            items.last(),
            Some(BrainWakeStreamItem::Actions { .. })
        ));
        assert!(items.iter().any(|item| matches!(
            item,
            BrainWakeStreamItem::Event {
                event: BrainEventEnvelope {
                    event: BrainEvent::ProviderStatus { .. },
                    ..
                }
            }
        )));
    }
}
