use rusty_crew_core_body::apply_history_window;
use rusty_crew_core_persistence::{
    CoreCoordinationStore, QueuedMessageFilter, QueuedMessageRecord, QueuedMessageState,
};
use rusty_crew_core_protocol::{CoreResult, IsoTimestamp, SessionId};

pub(crate) trait BodyQueueStore {
    fn save_body_follow_up_message(&self, record: &QueuedMessageRecord) -> CoreResult<()>;
    fn expire_body_follow_up_messages_at(
        &self,
        now: &IsoTimestamp,
    ) -> CoreResult<Vec<QueuedMessageRecord>>;
    fn load_body_follow_up_messages(
        &self,
        filter: &QueuedMessageFilter,
    ) -> CoreResult<Vec<QueuedMessageRecord>>;
}

impl BodyQueueStore for CoreCoordinationStore {
    fn save_body_follow_up_message(&self, record: &QueuedMessageRecord) -> CoreResult<()> {
        self.save_queued_message(record)
    }

    fn expire_body_follow_up_messages_at(
        &self,
        now: &IsoTimestamp,
    ) -> CoreResult<Vec<QueuedMessageRecord>> {
        self.expire_queued_messages_at(now)
    }

    fn load_body_follow_up_messages(
        &self,
        filter: &QueuedMessageFilter,
    ) -> CoreResult<Vec<QueuedMessageRecord>> {
        self.load_queued_messages(filter)
    }
}

pub(crate) fn save_body_follow_up_message(
    store: &impl BodyQueueStore,
    record: &QueuedMessageRecord,
) -> CoreResult<()> {
    store.save_body_follow_up_message(record)
}

pub(crate) fn drain_follow_up_queue_for_wake(
    store: &impl BodyQueueStore,
    now: &IsoTimestamp,
    session_id: &SessionId,
    max_delivered_messages: Option<u32>,
) -> CoreResult<Vec<QueuedMessageRecord>> {
    store.expire_body_follow_up_messages_at(now)?;
    let pending = store.load_body_follow_up_messages(&pending_filter(session_id))?;
    let delivered_ids = apply_history_window(
        pending
            .iter()
            .map(|record| record.message_id.clone())
            .collect::<Vec<_>>(),
        max_delivered_messages,
    )
    .into_iter()
    .collect::<std::collections::HashSet<_>>();
    let mut delivered = Vec::new();
    for mut record in pending {
        let include_in_wake = delivered_ids.contains(&record.message_id);
        record.state = if include_in_wake {
            QueuedMessageState::Delivered
        } else {
            QueuedMessageState::Discarded
        };
        record.delivery_attempts += 1;
        record.terminal_at = Some(now.clone());
        record.state_reason = Some(if include_in_wake {
            "delivered_for_wake".to_string()
        } else {
            "history_window_exceeded".to_string()
        });
        store.save_body_follow_up_message(&record)?;
        if include_in_wake {
            delivered.push(record);
        }
    }
    Ok(delivered)
}

pub(crate) fn enforce_follow_up_queue_cap(
    store: &impl BodyQueueStore,
    now: &IsoTimestamp,
    session_id: &SessionId,
    max_queued_messages: u32,
) -> CoreResult<()> {
    let pending = store.load_body_follow_up_messages(&pending_filter(session_id))?;
    let overflow = pending.len().saturating_sub(max_queued_messages as usize);
    if overflow == 0 {
        return Ok(());
    }
    for mut record in pending.into_iter().take(overflow) {
        record.state = QueuedMessageState::Discarded;
        record.terminal_at = Some(now.clone());
        record.state_reason = Some("queue_cap_exceeded".to_string());
        store.save_body_follow_up_message(&record)?;
    }
    Ok(())
}

fn pending_filter(session_id: &SessionId) -> QueuedMessageFilter {
    QueuedMessageFilter {
        state: Some(QueuedMessageState::Pending),
        owner_session_id: Some(session_id.clone()),
        owner_agent_id: None,
        limit: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_crew_core_protocol::{AgentId, AgentMessage};
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeBodyQueueStore {
        records: Mutex<Vec<QueuedMessageRecord>>,
    }

    impl FakeBodyQueueStore {
        fn with(records: Vec<QueuedMessageRecord>) -> Self {
            Self {
                records: Mutex::new(records),
            }
        }

        fn records(&self) -> Vec<QueuedMessageRecord> {
            self.records.lock().unwrap().clone()
        }
    }

    impl BodyQueueStore for FakeBodyQueueStore {
        fn save_body_follow_up_message(&self, record: &QueuedMessageRecord) -> CoreResult<()> {
            let mut records = self.records.lock().unwrap();
            if let Some(existing) = records
                .iter_mut()
                .find(|existing| existing.message_id == record.message_id)
            {
                *existing = record.clone();
            } else {
                records.push(record.clone());
            }
            Ok(())
        }

        fn expire_body_follow_up_messages_at(
            &self,
            _now: &IsoTimestamp,
        ) -> CoreResult<Vec<QueuedMessageRecord>> {
            Ok(Vec::new())
        }

        fn load_body_follow_up_messages(
            &self,
            filter: &QueuedMessageFilter,
        ) -> CoreResult<Vec<QueuedMessageRecord>> {
            let records = self.records.lock().unwrap();
            let mut matched = records
                .iter()
                .filter(|record| {
                    filter
                        .state
                        .as_ref()
                        .is_none_or(|state| record.state == *state)
                        && filter.owner_session_id.as_ref().is_none_or(|session_id| {
                            record.owner_session_id.as_ref() == Some(session_id)
                        })
                        && filter
                            .owner_agent_id
                            .as_ref()
                            .is_none_or(|agent_id| &record.owner_agent_id == agent_id)
                })
                .cloned()
                .collect::<Vec<_>>();
            if let Some(limit) = filter.limit {
                matched.truncate(limit as usize);
            }
            Ok(matched)
        }
    }

    #[test]
    fn drain_follow_up_queue_uses_fake_store_and_history_window() {
        let session_id = SessionId::new("prime-session");
        let store = FakeBodyQueueStore::with(
            (1..=4)
                .map(|index| queued_record(&session_id, index))
                .collect(),
        );
        let now = "2026-07-07T13:00:00Z".to_string();

        let delivered = drain_follow_up_queue_for_wake(&store, &now, &session_id, Some(2)).unwrap();

        assert_eq!(
            delivered
                .iter()
                .map(|record| record.message_id.as_str())
                .collect::<Vec<_>>(),
            vec!["queued-3", "queued-4"]
        );
        let records = store.records();
        assert_eq!(
            records
                .iter()
                .map(|record| (
                    record.message_id.as_str(),
                    record.state,
                    record.terminal_at.as_deref(),
                    record.state_reason.as_deref(),
                    record.delivery_attempts,
                ))
                .collect::<Vec<_>>(),
            vec![
                (
                    "queued-1",
                    QueuedMessageState::Discarded,
                    Some("2026-07-07T13:00:00Z"),
                    Some("history_window_exceeded"),
                    1,
                ),
                (
                    "queued-2",
                    QueuedMessageState::Discarded,
                    Some("2026-07-07T13:00:00Z"),
                    Some("history_window_exceeded"),
                    1,
                ),
                (
                    "queued-3",
                    QueuedMessageState::Delivered,
                    Some("2026-07-07T13:00:00Z"),
                    Some("delivered_for_wake"),
                    1,
                ),
                (
                    "queued-4",
                    QueuedMessageState::Delivered,
                    Some("2026-07-07T13:00:00Z"),
                    Some("delivered_for_wake"),
                    1,
                ),
            ]
        );
    }

    #[test]
    fn enforce_follow_up_queue_cap_uses_fake_store() {
        let session_id = SessionId::new("prime-session");
        let store = FakeBodyQueueStore::with(
            (1..=4)
                .map(|index| queued_record(&session_id, index))
                .collect(),
        );
        let now = "2026-07-07T13:00:00Z".to_string();

        enforce_follow_up_queue_cap(&store, &now, &session_id, 2).unwrap();

        let records = store.records();
        assert_eq!(
            records
                .iter()
                .map(|record| (
                    record.message_id.as_str(),
                    record.state,
                    record.state_reason.as_deref(),
                ))
                .collect::<Vec<_>>(),
            vec![
                (
                    "queued-1",
                    QueuedMessageState::Discarded,
                    Some("queue_cap_exceeded"),
                ),
                (
                    "queued-2",
                    QueuedMessageState::Discarded,
                    Some("queue_cap_exceeded"),
                ),
                ("queued-3", QueuedMessageState::Pending, None),
                ("queued-4", QueuedMessageState::Pending, None),
            ]
        );
    }

    fn queued_record(session_id: &SessionId, index: u32) -> QueuedMessageRecord {
        QueuedMessageRecord {
            message_id: format!("queued-{index}"),
            owner_session_id: Some(session_id.clone()),
            owner_agent_id: AgentId::new("prime"),
            message: AgentMessage {
                from: AgentId::new("operator"),
                to: AgentId::new("prime"),
                body: format!("body {index}"),
                correlation_id: None,
                projection: None,
            },
            source_sequence: None,
            enqueued_at: "2026-07-07T12:59:00Z".to_string(),
            expires_at: "2026-07-07T13:10:00Z".to_string(),
            ttl_ms: 600_000,
            delivery_attempts: 0,
            state: QueuedMessageState::Pending,
            terminal_at: None,
            state_reason: None,
        }
    }
}
