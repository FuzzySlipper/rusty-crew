use rusty_crew_core_persistence::{
    CoreCoordinationStore, ProviderWireStateDiagnostic, ProviderWireStateInvalidationReason,
    ProviderWireStateKey, ProviderWireStateWakeLookup, ProviderWireStateWakeResult,
    ProviderWireStateWrite,
};
use rusty_crew_core_protocol::{CoreResult, IsoTimestamp};

pub(crate) trait ProviderStateStore {
    fn load_provider_state_for_wake(
        &self,
        lookup: &ProviderWireStateWakeLookup,
    ) -> CoreResult<ProviderWireStateWakeResult>;
    fn save_provider_state(&self, write: &ProviderWireStateWrite) -> CoreResult<()>;
    fn clear_provider_state(
        &self,
        key: &ProviderWireStateKey,
        now: &IsoTimestamp,
        reason: ProviderWireStateInvalidationReason,
    ) -> CoreResult<()>;
    fn list_provider_state_diagnostics(
        &self,
        limit: u32,
    ) -> CoreResult<Vec<ProviderWireStateDiagnostic>>;
}

impl ProviderStateStore for CoreCoordinationStore {
    fn load_provider_state_for_wake(
        &self,
        lookup: &ProviderWireStateWakeLookup,
    ) -> CoreResult<ProviderWireStateWakeResult> {
        self.load_provider_wire_state_for_wake(lookup)
    }

    fn save_provider_state(&self, write: &ProviderWireStateWrite) -> CoreResult<()> {
        self.save_provider_wire_state(write)
    }

    fn clear_provider_state(
        &self,
        key: &ProviderWireStateKey,
        now: &IsoTimestamp,
        reason: ProviderWireStateInvalidationReason,
    ) -> CoreResult<()> {
        self.clear_provider_wire_state(key, now, reason)
    }

    fn list_provider_state_diagnostics(
        &self,
        limit: u32,
    ) -> CoreResult<Vec<ProviderWireStateDiagnostic>> {
        self.list_provider_wire_state_diagnostics(limit)
    }
}

pub(crate) fn load_provider_state_for_wake(
    store: &impl ProviderStateStore,
    lookup: &ProviderWireStateWakeLookup,
) -> CoreResult<ProviderWireStateWakeResult> {
    store.load_provider_state_for_wake(lookup)
}

pub(crate) fn save_provider_state(
    store: &impl ProviderStateStore,
    write: &ProviderWireStateWrite,
) -> CoreResult<()> {
    store.save_provider_state(write)
}

pub(crate) fn clear_provider_state(
    store: &impl ProviderStateStore,
    key: &ProviderWireStateKey,
    now: &IsoTimestamp,
    reason: ProviderWireStateInvalidationReason,
) -> CoreResult<()> {
    store.clear_provider_state(key, now, reason)
}

pub(crate) fn list_provider_state_diagnostics(
    store: &impl ProviderStateStore,
    limit: u32,
) -> CoreResult<Vec<ProviderWireStateDiagnostic>> {
    store.list_provider_state_diagnostics(limit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_crew_core_protocol::{ProviderStateAbsenceReason, SessionId};
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeProviderStateStore {
        wake_result: Mutex<Option<ProviderWireStateWakeResult>>,
        saved: Mutex<Vec<ProviderWireStateWrite>>,
        clears: Mutex<Vec<(ProviderWireStateKey, ProviderWireStateInvalidationReason)>>,
        diagnostics: Mutex<Vec<ProviderWireStateDiagnostic>>,
    }

    impl ProviderStateStore for FakeProviderStateStore {
        fn load_provider_state_for_wake(
            &self,
            _lookup: &ProviderWireStateWakeLookup,
        ) -> CoreResult<ProviderWireStateWakeResult> {
            Ok(self
                .wake_result
                .lock()
                .unwrap()
                .clone()
                .unwrap_or(ProviderWireStateWakeResult {
                    record: None,
                    absence_reason: Some(ProviderStateAbsenceReason::Missing),
                }))
        }

        fn save_provider_state(&self, write: &ProviderWireStateWrite) -> CoreResult<()> {
            self.saved.lock().unwrap().push(write.clone());
            Ok(())
        }

        fn clear_provider_state(
            &self,
            key: &ProviderWireStateKey,
            _now: &IsoTimestamp,
            reason: ProviderWireStateInvalidationReason,
        ) -> CoreResult<()> {
            self.clears.lock().unwrap().push((key.clone(), reason));
            Ok(())
        }

        fn list_provider_state_diagnostics(
            &self,
            limit: u32,
        ) -> CoreResult<Vec<ProviderWireStateDiagnostic>> {
            let mut diagnostics = self.diagnostics.lock().unwrap().clone();
            diagnostics.truncate(limit as usize);
            Ok(diagnostics)
        }
    }

    #[test]
    fn wake_absence_and_diagnostics_use_fake_provider_state_store() {
        let store = FakeProviderStateStore::default();
        let key = provider_key();
        *store.wake_result.lock().unwrap() = Some(ProviderWireStateWakeResult {
            record: None,
            absence_reason: Some(ProviderStateAbsenceReason::Invalidated),
        });
        store.diagnostics.lock().unwrap().push(diagnostic(&key));

        let loaded = load_provider_state_for_wake(
            &store,
            &ProviderWireStateWakeLookup {
                key: key.clone(),
                profile_fingerprint: "profile-v2".to_string(),
                provider_fingerprint: "provider-v2".to_string(),
                now: "2026-07-09T08:45:00Z".to_string(),
            },
        )
        .unwrap();
        clear_provider_state(
            &store,
            &key,
            &"2026-07-09T08:45:01Z".to_string(),
            ProviderWireStateInvalidationReason::OperatorRequestedClear,
        )
        .unwrap();
        let diagnostics = list_provider_state_diagnostics(&store, 1).unwrap();

        assert_eq!(
            loaded.absence_reason,
            Some(ProviderStateAbsenceReason::Invalidated)
        );
        assert_eq!(diagnostics[0].key, key);
        assert_eq!(
            store.clears.lock().unwrap()[0].1,
            ProviderWireStateInvalidationReason::OperatorRequestedClear
        );
    }

    fn provider_key() -> ProviderWireStateKey {
        ProviderWireStateKey {
            session_id: SessionId::new("prime-session"),
            module_id: "openai-responses".to_string(),
            strategy_id: "response-loop".to_string(),
        }
    }

    fn diagnostic(key: &ProviderWireStateKey) -> ProviderWireStateDiagnostic {
        ProviderWireStateDiagnostic {
            key: key.clone(),
            payload_version: "v1".to_string(),
            payload_bytes: 42,
            created_at: "2026-07-09T08:44:00Z".to_string(),
            updated_at: "2026-07-09T08:44:00Z".to_string(),
            expires_at: Some("2026-07-09T09:44:00Z".to_string()),
            last_wake_id: Some("wake-1".to_string()),
            invalidated_at: None,
            invalidation_reason: None,
        }
    }
}
