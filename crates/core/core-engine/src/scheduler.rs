use crate::{add_millis_to_iso, CoreEngine};
use rusty_crew_core_body::session_kind_can_wake;
use rusty_crew_core_persistence::{
    CoreCoordinationStore, QueryPage, ScheduledJobQuery, ScheduledJobRecord, ScheduledJobStatus,
    ScheduledRunQuery, ScheduledRunRecord, ScheduledRunStatus, ScheduledRunTrigger,
};
use rusty_crew_core_protocol::{
    CoreError, CoreErrorKind, CoreEvent, CoreResult, IsoTimestamp, RunId, SessionId, SessionStatus,
};
use serde_json::Value as JsonValue;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_SCHEDULED_RUN: AtomicU64 = AtomicU64::new(1);

const SCHEDULED_WAKE_JOB_KIND: &str = "runtime.wake.session";
const SCHEDULER_CLAIM_TTL_MS: u64 = 30_000;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SchedulerTickReport {
    pub stale_runs_expired: u32,
    pub due_runs_claimed: u32,
    pub wakes_requested: u32,
    pub runs_completed: u32,
    pub runs_skipped: u32,
    pub runs_failed: u32,
}

trait SchedulerStore {
    fn upsert_scheduler_job(&self, record: &ScheduledJobRecord) -> CoreResult<()>;
    fn load_scheduler_job(&self, job_id: &str) -> CoreResult<Option<ScheduledJobRecord>>;
    fn query_scheduler_jobs(
        &self,
        query: &ScheduledJobQuery,
    ) -> CoreResult<Vec<ScheduledJobRecord>>;
    fn query_scheduler_runs(
        &self,
        query: &ScheduledRunQuery,
    ) -> CoreResult<Vec<ScheduledRunRecord>>;
    fn expire_stale_scheduler_runs(
        &self,
        stale_before: &IsoTimestamp,
        now: &IsoTimestamp,
    ) -> CoreResult<Vec<ScheduledRunRecord>>;
    fn claim_scheduler_run(
        &self,
        run: &ScheduledRunRecord,
        next_due_at: Option<&IsoTimestamp>,
    ) -> CoreResult<()>;
    fn complete_scheduler_run(
        &self,
        run_id: &RunId,
        status: ScheduledRunStatus,
        completed_at: &IsoTimestamp,
        output_json: &JsonValue,
        error: Option<&str>,
    ) -> CoreResult<()>;
    fn pause_scheduler_job(&self, job_id: &str, now: &IsoTimestamp) -> CoreResult<()>;
    fn resume_scheduler_job(
        &self,
        job_id: &str,
        next_due_at: &IsoTimestamp,
        now: &IsoTimestamp,
    ) -> CoreResult<()>;
}

impl SchedulerStore for CoreCoordinationStore {
    fn upsert_scheduler_job(&self, record: &ScheduledJobRecord) -> CoreResult<()> {
        self.upsert_scheduled_job(record)
    }

    fn load_scheduler_job(&self, job_id: &str) -> CoreResult<Option<ScheduledJobRecord>> {
        self.load_scheduled_job(job_id)
    }

    fn query_scheduler_jobs(
        &self,
        query: &ScheduledJobQuery,
    ) -> CoreResult<Vec<ScheduledJobRecord>> {
        self.query_scheduled_jobs(query)
    }

    fn query_scheduler_runs(
        &self,
        query: &ScheduledRunQuery,
    ) -> CoreResult<Vec<ScheduledRunRecord>> {
        self.query_scheduled_runs(query)
    }

    fn expire_stale_scheduler_runs(
        &self,
        stale_before: &IsoTimestamp,
        now: &IsoTimestamp,
    ) -> CoreResult<Vec<ScheduledRunRecord>> {
        self.expire_stale_scheduled_runs(stale_before, now)
    }

    fn claim_scheduler_run(
        &self,
        run: &ScheduledRunRecord,
        next_due_at: Option<&IsoTimestamp>,
    ) -> CoreResult<()> {
        self.claim_scheduled_run(run, next_due_at)
    }

    fn complete_scheduler_run(
        &self,
        run_id: &RunId,
        status: ScheduledRunStatus,
        completed_at: &IsoTimestamp,
        output_json: &JsonValue,
        error: Option<&str>,
    ) -> CoreResult<()> {
        self.complete_scheduled_run(run_id, status, completed_at, output_json, error)
    }

    fn pause_scheduler_job(&self, job_id: &str, now: &IsoTimestamp) -> CoreResult<()> {
        self.pause_scheduled_job(job_id, now)
    }

    fn resume_scheduler_job(
        &self,
        job_id: &str,
        next_due_at: &IsoTimestamp,
        now: &IsoTimestamp,
    ) -> CoreResult<()> {
        self.resume_scheduled_job(job_id, next_due_at, now)
    }
}

impl CoreEngine {
    pub fn register_scheduled_wake_job(
        &self,
        job_id: impl Into<String>,
        target_session_id: SessionId,
        interval_ms: Option<u64>,
        first_due_at: IsoTimestamp,
    ) -> CoreResult<ScheduledJobRecord> {
        let session = self.sessions.get_session(&target_session_id)?;
        if !session_kind_can_wake(&session.kind) {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!(
                    "session {} cannot be woken by scheduler",
                    session.session_id
                ),
            ));
        }
        let now = self.now();
        let record = ScheduledJobRecord {
            job_id: job_id.into(),
            job_kind: SCHEDULED_WAKE_JOB_KIND.to_string(),
            target_session_id: Some(target_session_id),
            interval_ms,
            next_due_at: Some(first_due_at),
            payload_json: serde_json::json!({}),
            status: ScheduledJobStatus::Active,
            created_at: now.clone(),
            updated_at: now,
            paused_at: None,
        };
        SchedulerStore::upsert_scheduler_job(&self.store, &record)?;
        Ok(record)
    }

    pub fn register_scheduled_host_job(
        &self,
        job_id: impl Into<String>,
        job_kind: impl Into<String>,
        interval_ms: Option<u64>,
        first_due_at: IsoTimestamp,
        payload_json: serde_json::Value,
    ) -> CoreResult<ScheduledJobRecord> {
        let job_kind = job_kind.into();
        if job_kind.trim().is_empty() || job_kind == SCHEDULED_WAKE_JOB_KIND {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "scheduled host job requires a non-wake job kind",
            ));
        }
        let now = self.now();
        let record = ScheduledJobRecord {
            job_id: job_id.into(),
            job_kind,
            target_session_id: None,
            interval_ms,
            next_due_at: Some(first_due_at),
            payload_json,
            status: ScheduledJobStatus::Active,
            created_at: now.clone(),
            updated_at: now,
            paused_at: None,
        };
        SchedulerStore::upsert_scheduler_job(&self.store, &record)?;
        Ok(record)
    }

    pub fn list_scheduled_jobs(
        &self,
        status: Option<ScheduledJobStatus>,
        job_kind: Option<String>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> CoreResult<Vec<ScheduledJobRecord>> {
        SchedulerStore::query_scheduler_jobs(
            &self.store,
            &ScheduledJobQuery {
                status,
                job_kind,
                page: Some(QueryPage { limit, offset }),
                ..ScheduledJobQuery::default()
            },
        )
    }

    pub fn list_scheduled_runs(
        &self,
        job_id: Option<String>,
        status: Option<ScheduledRunStatus>,
        trigger: Option<ScheduledRunTrigger>,
        target_session_id: Option<SessionId>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> CoreResult<Vec<ScheduledRunRecord>> {
        SchedulerStore::query_scheduler_runs(
            &self.store,
            &ScheduledRunQuery {
                job_id,
                status,
                trigger,
                target_session_id,
                page: Some(QueryPage { limit, offset }),
                ..ScheduledRunQuery::default()
            },
        )
    }

    pub fn claim_scheduled_host_runs(
        &self,
        supported_job_kinds: Vec<String>,
        limit: Option<u32>,
    ) -> CoreResult<Vec<ScheduledRunRecord>> {
        let _guard = self.scheduler_tick_lock.lock().map_err(|_| {
            CoreError::new(CoreErrorKind::InternalError, "scheduler tick lock poisoned")
        })?;
        let supported_job_kinds = normalized_supported_host_job_kinds(supported_job_kinds)?;
        let now = self.now();
        SchedulerStore::expire_stale_scheduler_runs(&self.store, &now, &now)?;
        let mut claimed = Vec::new();
        let max_claims = limit.unwrap_or(10).clamp(1, 100);
        for job_kind in supported_job_kinds {
            if claimed.len() >= max_claims as usize {
                break;
            }
            let remaining = max_claims.saturating_sub(claimed.len() as u32);
            let due_jobs = SchedulerStore::query_scheduler_jobs(
                &self.store,
                &ScheduledJobQuery {
                    status: Some(ScheduledJobStatus::Active),
                    job_kind: Some(job_kind),
                    due_at_or_before: Some(now.clone()),
                    page: Some(QueryPage {
                        limit: Some(remaining),
                        offset: None,
                    }),
                },
            )?;
            for job in due_jobs {
                claimed.push(self.claim_scheduled_run(
                    &job,
                    ScheduledRunTrigger::Due,
                    job.next_due_at.clone(),
                )?);
            }
        }
        Ok(claimed)
    }

    pub fn request_scheduled_host_job_run(
        &self,
        job_id: &str,
        supported_job_kinds: Vec<String>,
    ) -> CoreResult<Option<ScheduledRunRecord>> {
        let supported_job_kinds = normalized_supported_host_job_kinds(supported_job_kinds)?;
        let Some(job) = SchedulerStore::load_scheduler_job(&self.store, job_id)? else {
            return Ok(None);
        };
        if job.status == ScheduledJobStatus::Archived {
            return Ok(None);
        }
        if !supported_job_kinds.contains(&job.job_kind) {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("scheduled job kind {} is not host-supported", job.job_kind),
            ));
        }
        self.claim_scheduled_run(&job, ScheduledRunTrigger::Manual, None)
            .map(Some)
    }

    pub fn complete_scheduled_host_run(
        &self,
        run_id: &RunId,
        status: ScheduledRunStatus,
        output_json: serde_json::Value,
        error: Option<String>,
    ) -> CoreResult<()> {
        if !matches!(
            status,
            ScheduledRunStatus::Completed
                | ScheduledRunStatus::Skipped
                | ScheduledRunStatus::Failed
                | ScheduledRunStatus::Cancelled
        ) {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "scheduled host run completion requires a terminal host status",
            ));
        }
        SchedulerStore::complete_scheduler_run(
            &self.store,
            run_id,
            status,
            &self.now(),
            &output_json,
            error.as_deref(),
        )
    }

    pub fn pause_scheduled_job(&self, job_id: &str) -> CoreResult<()> {
        SchedulerStore::pause_scheduler_job(&self.store, job_id, &self.now())
    }

    pub fn resume_scheduled_job(&self, job_id: &str, next_due_at: IsoTimestamp) -> CoreResult<()> {
        SchedulerStore::resume_scheduler_job(&self.store, job_id, &next_due_at, &self.now())
    }

    pub fn request_scheduled_job_run(
        &self,
        job_id: &str,
    ) -> CoreResult<Option<ScheduledRunRecord>> {
        let Some(job) = SchedulerStore::load_scheduler_job(&self.store, job_id)? else {
            return Ok(None);
        };
        if job.status == ScheduledJobStatus::Archived {
            return Ok(None);
        }
        let run = self.claim_scheduled_run(&job, ScheduledRunTrigger::Manual, None)?;
        self.finish_scheduler_run(run)
    }

    pub fn run_scheduler_tick(&self) -> CoreResult<SchedulerTickReport> {
        let _guard = self.scheduler_tick_lock.lock().map_err(|_| {
            CoreError::new(CoreErrorKind::InternalError, "scheduler tick lock poisoned")
        })?;
        let now = self.now();
        let stale_runs = SchedulerStore::expire_stale_scheduler_runs(&self.store, &now, &now)?;
        let due_jobs = SchedulerStore::query_scheduler_jobs(
            &self.store,
            &ScheduledJobQuery {
                status: Some(ScheduledJobStatus::Active),
                job_kind: Some(SCHEDULED_WAKE_JOB_KIND.to_string()),
                due_at_or_before: Some(now.clone()),
                page: None,
            },
        )?;
        let mut report = SchedulerTickReport {
            stale_runs_expired: stale_runs.len() as u32,
            ..SchedulerTickReport::default()
        };
        for job in due_jobs {
            let run =
                self.claim_scheduled_run(&job, ScheduledRunTrigger::Due, job.next_due_at.clone())?;
            report.due_runs_claimed += 1;
            if let Some(run) = self.finish_scheduler_run(run)? {
                match run.status {
                    ScheduledRunStatus::Completed => {
                        report.runs_completed += 1;
                        report.wakes_requested += 1;
                    }
                    ScheduledRunStatus::Skipped => report.runs_skipped += 1,
                    ScheduledRunStatus::Failed => report.runs_failed += 1,
                    _ => {}
                }
            }
        }
        Ok(report)
    }

    fn claim_scheduled_run(
        &self,
        job: &ScheduledJobRecord,
        trigger: ScheduledRunTrigger,
        scheduled_for: Option<IsoTimestamp>,
    ) -> CoreResult<ScheduledRunRecord> {
        let now = self.now();
        claim_scheduled_run_in_store(&self.store, job, trigger, scheduled_for, &now)
    }

    fn finish_scheduler_run(
        &self,
        mut run: ScheduledRunRecord,
    ) -> CoreResult<Option<ScheduledRunRecord>> {
        if run.job_kind != SCHEDULED_WAKE_JOB_KIND {
            let now = self.now();
            run.status = ScheduledRunStatus::Skipped;
            run.completed_at = Some(now.clone());
            run.updated_at = now.clone();
            run.error = Some(format!("unsupported scheduled job kind {}", run.job_kind));
            run.output_json = serde_json::json!({ "wake_requested": false });
            SchedulerStore::complete_scheduler_run(
                &self.store,
                &run.run_id,
                run.status,
                &now,
                &run.output_json,
                run.error.as_deref(),
            )?;
            return Ok(Some(run));
        }
        let Some(session_id) = &run.target_session_id else {
            let now = self.now();
            run.status = ScheduledRunStatus::Failed;
            run.completed_at = Some(now.clone());
            run.updated_at = now.clone();
            run.error = Some("scheduled wake job has no target session".to_string());
            run.output_json = serde_json::json!({ "wake_requested": false });
            SchedulerStore::complete_scheduler_run(
                &self.store,
                &run.run_id,
                run.status,
                &now,
                &run.output_json,
                run.error.as_deref(),
            )?;
            return Ok(Some(run));
        };
        let session = match self.sessions.get_session(session_id) {
            Ok(session) => session,
            Err(error) if error.kind == CoreErrorKind::NotFound => {
                let now = self.now();
                run.status = ScheduledRunStatus::Skipped;
                run.completed_at = Some(now.clone());
                run.updated_at = now.clone();
                run.error = Some(format!("target session {session_id} not found"));
                run.output_json = serde_json::json!({ "wake_requested": false });
                SchedulerStore::complete_scheduler_run(
                    &self.store,
                    &run.run_id,
                    run.status,
                    &now,
                    &run.output_json,
                    run.error.as_deref(),
                )?;
                return Ok(Some(run));
            }
            Err(error) => return Err(error),
        };
        let now = self.now();
        if session.status == SessionStatus::Archived || !session_kind_can_wake(&session.kind) {
            run.status = ScheduledRunStatus::Skipped;
            run.completed_at = Some(now.clone());
            run.updated_at = now.clone();
            run.error = Some(format!(
                "target session {} is not wakeable",
                session.session_id
            ));
            run.output_json = serde_json::json!({ "wake_requested": false });
            SchedulerStore::complete_scheduler_run(
                &self.store,
                &run.run_id,
                run.status,
                &now,
                &run.output_json,
                run.error.as_deref(),
            )?;
            return Ok(Some(run));
        }
        self.bus.publish(CoreEvent::BrainWakeRequested {
            session_id: session.session_id.clone(),
        })?;
        run.status = ScheduledRunStatus::Completed;
        run.completed_at = Some(now.clone());
        run.updated_at = now.clone();
        run.output_json = serde_json::json!({
            "wake_requested": true,
            "session_id": session.session_id.0,
        });
        SchedulerStore::complete_scheduler_run(
            &self.store,
            &run.run_id,
            run.status,
            &now,
            &run.output_json,
            None,
        )?;
        Ok(Some(run))
    }
}

fn claim_scheduled_run_in_store(
    store: &impl SchedulerStore,
    job: &ScheduledJobRecord,
    trigger: ScheduledRunTrigger,
    scheduled_for: Option<IsoTimestamp>,
    now: &IsoTimestamp,
) -> CoreResult<ScheduledRunRecord> {
    let claim_deadline_at = add_millis_to_iso(now, SCHEDULER_CLAIM_TTL_MS)?;
    let run = ScheduledRunRecord {
        run_id: next_scheduled_run_id(&job.job_id, now),
        job_id: job.job_id.clone(),
        job_kind: job.job_kind.clone(),
        target_session_id: job.target_session_id.clone(),
        status: ScheduledRunStatus::Claimed,
        trigger,
        scheduled_for,
        claimed_at: now.clone(),
        claim_deadline_at,
        completed_at: None,
        error: None,
        output_json: serde_json::json!({}),
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    let next_due_at = if trigger == ScheduledRunTrigger::Due {
        job.interval_ms
            .map(|interval_ms| add_millis_to_iso(now, interval_ms))
            .transpose()?
    } else {
        None
    };
    store.claim_scheduler_run(&run, next_due_at.as_ref())?;
    Ok(run)
}

fn next_scheduled_run_id(job_id: &str, now: &IsoTimestamp) -> RunId {
    let sequence = NEXT_SCHEDULED_RUN.fetch_add(1, Ordering::Relaxed);
    RunId::new(format!(
        "scheduled:{job_id}:{}:{sequence}",
        crate::sanitized_clock_key(now)
    ))
}

fn normalized_supported_host_job_kinds(job_kinds: Vec<String>) -> CoreResult<Vec<String>> {
    let mut normalized = Vec::new();
    for job_kind in job_kinds {
        let job_kind = job_kind.trim().to_string();
        if job_kind.is_empty() || job_kind == SCHEDULED_WAKE_JOB_KIND {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "host scheduler claims require non-wake job kinds",
            ));
        }
        if !normalized.contains(&job_kind) {
            normalized.push(job_kind);
        }
    }
    if normalized.is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "host scheduler claims require at least one supported job kind",
        ));
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeSchedulerStore {
        claimed_run: Mutex<Option<ScheduledRunRecord>>,
        claimed_next_due_at: Mutex<Option<IsoTimestamp>>,
    }

    impl SchedulerStore for FakeSchedulerStore {
        fn upsert_scheduler_job(&self, _record: &ScheduledJobRecord) -> CoreResult<()> {
            Ok(())
        }

        fn load_scheduler_job(&self, _job_id: &str) -> CoreResult<Option<ScheduledJobRecord>> {
            Ok(None)
        }

        fn query_scheduler_jobs(
            &self,
            _query: &ScheduledJobQuery,
        ) -> CoreResult<Vec<ScheduledJobRecord>> {
            Ok(Vec::new())
        }

        fn query_scheduler_runs(
            &self,
            _query: &ScheduledRunQuery,
        ) -> CoreResult<Vec<ScheduledRunRecord>> {
            Ok(Vec::new())
        }

        fn expire_stale_scheduler_runs(
            &self,
            _stale_before: &IsoTimestamp,
            _now: &IsoTimestamp,
        ) -> CoreResult<Vec<ScheduledRunRecord>> {
            Ok(Vec::new())
        }

        fn claim_scheduler_run(
            &self,
            run: &ScheduledRunRecord,
            next_due_at: Option<&IsoTimestamp>,
        ) -> CoreResult<()> {
            *self.claimed_run.lock().unwrap() = Some(run.clone());
            *self.claimed_next_due_at.lock().unwrap() = next_due_at.cloned();
            Ok(())
        }

        fn complete_scheduler_run(
            &self,
            _run_id: &RunId,
            _status: ScheduledRunStatus,
            _completed_at: &IsoTimestamp,
            _output_json: &JsonValue,
            _error: Option<&str>,
        ) -> CoreResult<()> {
            Ok(())
        }

        fn pause_scheduler_job(&self, _job_id: &str, _now: &IsoTimestamp) -> CoreResult<()> {
            Ok(())
        }

        fn resume_scheduler_job(
            &self,
            _job_id: &str,
            _next_due_at: &IsoTimestamp,
            _now: &IsoTimestamp,
        ) -> CoreResult<()> {
            Ok(())
        }
    }

    #[test]
    fn claim_scheduled_run_uses_fake_store_and_engine_clock() {
        let store = FakeSchedulerStore::default();
        let job = scheduled_wake_job();
        let now = "2026-07-07T13:00:00Z".to_string();

        let run = claim_scheduled_run_in_store(
            &store,
            &job,
            ScheduledRunTrigger::Due,
            job.next_due_at.clone(),
            &now,
        )
        .unwrap();

        assert!(run
            .run_id
            .0
            .starts_with("scheduled:wake-prime:2026_07_07T13_00_00Z:"));
        assert_eq!(run.job_id, "wake-prime");
        assert_eq!(run.status, ScheduledRunStatus::Claimed);
        assert_eq!(run.scheduled_for, Some("2026-07-07T12:59:00Z".to_string()));
        assert_eq!(run.claimed_at, "2026-07-07T13:00:00Z");
        assert_eq!(run.claim_deadline_at, "2026-07-07T13:00:30Z");
        assert_eq!(
            store.claimed_run.lock().unwrap().as_ref().unwrap().run_id,
            run.run_id
        );
        assert_eq!(
            *store.claimed_next_due_at.lock().unwrap(),
            Some("2026-07-07T13:01:00Z".to_string())
        );
    }

    fn scheduled_wake_job() -> ScheduledJobRecord {
        ScheduledJobRecord {
            job_id: "wake-prime".to_string(),
            job_kind: SCHEDULED_WAKE_JOB_KIND.to_string(),
            target_session_id: Some(SessionId::new("prime-session")),
            interval_ms: Some(60_000),
            next_due_at: Some("2026-07-07T12:59:00Z".to_string()),
            payload_json: serde_json::json!({}),
            status: ScheduledJobStatus::Active,
            created_at: "2026-07-07T12:00:00Z".to_string(),
            updated_at: "2026-07-07T12:00:00Z".to_string(),
            paused_at: None,
        }
    }
}
