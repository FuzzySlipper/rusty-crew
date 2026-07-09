use rusty_crew_core_persistence::{
    CoreCoordinationStore, WorkerPoolClaimRecord, WorkerPoolClaimRequest,
    WorkerPoolCompletionRequest, WorkerPoolMemberRecord, WorkerPoolNoCapacityReason,
    WorkerPoolWorkItemRecord, WorkerRunRecord, WorkerRunStatus,
};
use rusty_crew_core_protocol::{
    CoreResult, DelegatedCompletion, DelegatedFanOutGroup, IsoTimestamp, RunId, SessionId,
};

pub(crate) trait DelegationStore {
    fn save_delegated_worker_run_requested(&self, record: &WorkerRunRecord) -> CoreResult<()>;
    fn load_delegated_worker_run(&self, run_id: &RunId) -> CoreResult<Option<WorkerRunRecord>>;
    fn load_delegated_worker_run_by_session(
        &self,
        delegated_session_id: &SessionId,
    ) -> CoreResult<Option<WorkerRunRecord>>;
    fn update_delegated_worker_run_status_by_session(
        &self,
        delegated_session_id: &SessionId,
        status: WorkerRunStatus,
        now: IsoTimestamp,
    ) -> CoreResult<()>;
    fn update_delegated_worker_run_status(
        &self,
        run_id: &RunId,
        status: WorkerRunStatus,
        now: IsoTimestamp,
    ) -> CoreResult<()>;
    fn delegated_completions_for_parent(
        &self,
        parent_session_id: &SessionId,
    ) -> CoreResult<Vec<DelegatedCompletion>>;
    fn delegated_fan_out_groups_for_parent(
        &self,
        parent_session_id: &SessionId,
    ) -> CoreResult<Vec<DelegatedFanOutGroup>>;
}

pub(crate) trait WorkerPoolStore {
    fn load_worker_pool_member(
        &self,
        member_id: &str,
    ) -> CoreResult<Option<WorkerPoolMemberRecord>>;
    fn create_worker_pool_work_item(&self, record: &WorkerPoolWorkItemRecord) -> CoreResult<()>;
    fn claim_next_worker_pool_work_item(
        &self,
        request: &WorkerPoolClaimRequest,
    ) -> CoreResult<Result<WorkerPoolClaimRecord, WorkerPoolNoCapacityReason>>;
    fn complete_worker_pool_work_item(
        &self,
        request: &WorkerPoolCompletionRequest,
    ) -> CoreResult<bool>;
}

impl DelegationStore for CoreCoordinationStore {
    fn save_delegated_worker_run_requested(&self, record: &WorkerRunRecord) -> CoreResult<()> {
        self.save_worker_run_requested(record)
    }

    fn load_delegated_worker_run(&self, run_id: &RunId) -> CoreResult<Option<WorkerRunRecord>> {
        self.load_worker_run(run_id)
    }

    fn load_delegated_worker_run_by_session(
        &self,
        delegated_session_id: &SessionId,
    ) -> CoreResult<Option<WorkerRunRecord>> {
        self.load_worker_run_by_delegated_session(delegated_session_id)
    }

    fn update_delegated_worker_run_status_by_session(
        &self,
        delegated_session_id: &SessionId,
        status: WorkerRunStatus,
        now: IsoTimestamp,
    ) -> CoreResult<()> {
        self.update_worker_run_status_by_delegated_session(delegated_session_id, status, now)
    }

    fn update_delegated_worker_run_status(
        &self,
        run_id: &RunId,
        status: WorkerRunStatus,
        now: IsoTimestamp,
    ) -> CoreResult<()> {
        self.update_worker_run_status(run_id, status, now)
    }

    fn delegated_completions_for_parent(
        &self,
        parent_session_id: &SessionId,
    ) -> CoreResult<Vec<DelegatedCompletion>> {
        self.delegated_completions_for_parent(parent_session_id)
    }

    fn delegated_fan_out_groups_for_parent(
        &self,
        parent_session_id: &SessionId,
    ) -> CoreResult<Vec<DelegatedFanOutGroup>> {
        self.fan_out_groups_for_parent(parent_session_id)
    }
}

impl WorkerPoolStore for CoreCoordinationStore {
    fn load_worker_pool_member(
        &self,
        member_id: &str,
    ) -> CoreResult<Option<WorkerPoolMemberRecord>> {
        self.load_worker_pool_member(member_id)
    }

    fn create_worker_pool_work_item(&self, record: &WorkerPoolWorkItemRecord) -> CoreResult<()> {
        self.create_worker_pool_work_item(record)
    }

    fn claim_next_worker_pool_work_item(
        &self,
        request: &WorkerPoolClaimRequest,
    ) -> CoreResult<Result<WorkerPoolClaimRecord, WorkerPoolNoCapacityReason>> {
        self.claim_next_worker_pool_work_item(request)
    }

    fn complete_worker_pool_work_item(
        &self,
        request: &WorkerPoolCompletionRequest,
    ) -> CoreResult<bool> {
        self.complete_worker_pool_work_item(request)
    }
}

pub(crate) fn save_delegated_worker_run_requested(
    store: &impl DelegationStore,
    record: &WorkerRunRecord,
) -> CoreResult<()> {
    store.save_delegated_worker_run_requested(record)
}

pub(crate) fn load_delegated_worker_run(
    store: &impl DelegationStore,
    run_id: &RunId,
) -> CoreResult<Option<WorkerRunRecord>> {
    store.load_delegated_worker_run(run_id)
}

pub(crate) fn load_delegated_worker_run_by_session(
    store: &impl DelegationStore,
    delegated_session_id: &SessionId,
) -> CoreResult<Option<WorkerRunRecord>> {
    store.load_delegated_worker_run_by_session(delegated_session_id)
}

pub(crate) fn update_delegated_worker_run_status_by_session(
    store: &impl DelegationStore,
    delegated_session_id: &SessionId,
    status: WorkerRunStatus,
    now: IsoTimestamp,
) -> CoreResult<()> {
    store.update_delegated_worker_run_status_by_session(delegated_session_id, status, now)
}

pub(crate) fn update_delegated_worker_run_status(
    store: &impl DelegationStore,
    run_id: &RunId,
    status: WorkerRunStatus,
    now: IsoTimestamp,
) -> CoreResult<()> {
    store.update_delegated_worker_run_status(run_id, status, now)
}

pub(crate) fn delegated_completions_for_parent(
    store: &impl DelegationStore,
    parent_session_id: &SessionId,
) -> CoreResult<Vec<DelegatedCompletion>> {
    store.delegated_completions_for_parent(parent_session_id)
}

pub(crate) fn delegated_fan_out_groups_for_parent(
    store: &impl DelegationStore,
    parent_session_id: &SessionId,
) -> CoreResult<Vec<DelegatedFanOutGroup>> {
    store.delegated_fan_out_groups_for_parent(parent_session_id)
}

pub(crate) fn load_worker_pool_member(
    store: &impl WorkerPoolStore,
    member_id: &str,
) -> CoreResult<Option<WorkerPoolMemberRecord>> {
    store.load_worker_pool_member(member_id)
}

pub(crate) fn create_worker_pool_work_item(
    store: &impl WorkerPoolStore,
    record: &WorkerPoolWorkItemRecord,
) -> CoreResult<()> {
    store.create_worker_pool_work_item(record)
}

pub(crate) fn claim_next_worker_pool_work_item(
    store: &impl WorkerPoolStore,
    request: &WorkerPoolClaimRequest,
) -> CoreResult<Result<WorkerPoolClaimRecord, WorkerPoolNoCapacityReason>> {
    store.claim_next_worker_pool_work_item(request)
}

pub(crate) fn complete_worker_pool_work_item(
    store: &impl WorkerPoolStore,
    request: &WorkerPoolCompletionRequest,
) -> CoreResult<bool> {
    store.complete_worker_pool_work_item(request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_crew_core_protocol::{
        AgentId, FanOutFailurePolicy, ParentConsumptionPolicy, ProfileId,
    };
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeDelegationStore {
        runs: Mutex<Vec<WorkerRunRecord>>,
    }

    impl DelegationStore for FakeDelegationStore {
        fn save_delegated_worker_run_requested(&self, record: &WorkerRunRecord) -> CoreResult<()> {
            self.runs.lock().unwrap().push(record.clone());
            Ok(())
        }

        fn load_delegated_worker_run(&self, run_id: &RunId) -> CoreResult<Option<WorkerRunRecord>> {
            Ok(self
                .runs
                .lock()
                .unwrap()
                .iter()
                .find(|run| &run.run_id == run_id)
                .cloned())
        }

        fn load_delegated_worker_run_by_session(
            &self,
            delegated_session_id: &SessionId,
        ) -> CoreResult<Option<WorkerRunRecord>> {
            Ok(self
                .runs
                .lock()
                .unwrap()
                .iter()
                .find(|run| run.delegated_session_id.as_ref() == Some(delegated_session_id))
                .cloned())
        }

        fn update_delegated_worker_run_status_by_session(
            &self,
            delegated_session_id: &SessionId,
            status: WorkerRunStatus,
            now: IsoTimestamp,
        ) -> CoreResult<()> {
            let mut runs = self.runs.lock().unwrap();
            if let Some(run) = runs
                .iter_mut()
                .find(|run| run.delegated_session_id.as_ref() == Some(delegated_session_id))
            {
                run.status = status;
                run.last_updated_at = now;
            }
            Ok(())
        }

        fn update_delegated_worker_run_status(
            &self,
            run_id: &RunId,
            status: WorkerRunStatus,
            now: IsoTimestamp,
        ) -> CoreResult<()> {
            let mut runs = self.runs.lock().unwrap();
            if let Some(run) = runs.iter_mut().find(|run| &run.run_id == run_id) {
                run.status = status;
                run.last_updated_at = now;
            }
            Ok(())
        }

        fn delegated_completions_for_parent(
            &self,
            _parent_session_id: &SessionId,
        ) -> CoreResult<Vec<DelegatedCompletion>> {
            Ok(Vec::new())
        }

        fn delegated_fan_out_groups_for_parent(
            &self,
            _parent_session_id: &SessionId,
        ) -> CoreResult<Vec<DelegatedFanOutGroup>> {
            Ok(Vec::new())
        }
    }

    #[test]
    fn worker_run_status_update_uses_fake_delegation_store() {
        let store = FakeDelegationStore::default();
        let delegated_session_id = SessionId::new("prime-session:worker");
        let run = worker_run_record(&delegated_session_id);

        save_delegated_worker_run_requested(&store, &run).unwrap();
        update_delegated_worker_run_status_by_session(
            &store,
            &delegated_session_id,
            WorkerRunStatus::WakeRequested,
            "2026-07-09T08:30:00Z".to_string(),
        )
        .unwrap();

        let loaded = load_delegated_worker_run_by_session(&store, &delegated_session_id)
            .unwrap()
            .unwrap();
        assert_eq!(loaded.status, WorkerRunStatus::WakeRequested);
        assert_eq!(loaded.last_updated_at, "2026-07-09T08:30:00Z");
    }

    fn worker_run_record(delegated_session_id: &SessionId) -> WorkerRunRecord {
        WorkerRunRecord {
            run_id: RunId::new("wake-1:0"),
            parent_session_id: SessionId::new("prime-session"),
            delegated_session_id: Some(delegated_session_id.clone()),
            parent_agent_id: Some(AgentId::new("prime")),
            profile_id: ProfileId::new("worker-profile"),
            task_id: None,
            status: WorkerRunStatus::Requested,
            created_at: "2026-07-09T08:29:00Z".to_string(),
            last_updated_at: "2026-07-09T08:29:00Z".to_string(),
            source_wake_id: "wake-1".to_string(),
            source_action_index: 0,
            delegation_correlation_id: None,
            parent_consumption: ParentConsumptionPolicy::AwaitCompletion,
            fan_out_group_id: None,
            fan_out_max_concurrency: None,
            fan_out_failure_policy: FanOutFailurePolicy::FailSoft,
            worker_pool_work_item_id: None,
            worker_pool_lease_id: None,
            worker_pool_member_id: None,
            worker_pool_claim_token: None,
        }
    }
}
