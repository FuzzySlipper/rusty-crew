use rusty_crew_core_bridge_api::{
    CoreError, CoreErrorKind, CoreResult, RunId, ScheduledJobWireOutput, ScheduledRunWireOutput,
    SchedulerTickWireOutput, SessionId, Unit,
};
use rusty_crew_core_persistence::{
    ScheduledJobRecord, ScheduledJobStatus, ScheduledRunRecord, ScheduledRunStatus,
    ScheduledRunTrigger,
};
use serde_json::Value;

use crate::NativeBridge;

impl NativeBridge {
    pub fn register_scheduled_wake_job(
        &self,
        job_id: String,
        target_session_id: SessionId,
        interval_ms: Option<u64>,
        first_due_at: String,
    ) -> CoreResult<ScheduledJobWireOutput> {
        self.engine()?
            .register_scheduled_wake_job(job_id, target_session_id, interval_ms, first_due_at)
            .map(scheduled_job_wire_output)
    }

    pub fn register_scheduled_host_job(
        &self,
        job_id: String,
        job_kind: String,
        interval_ms: Option<u64>,
        first_due_at: String,
        payload_json: Value,
    ) -> CoreResult<ScheduledJobWireOutput> {
        self.engine()?
            .register_scheduled_host_job(job_id, job_kind, interval_ms, first_due_at, payload_json)
            .map(scheduled_job_wire_output)
    }

    pub fn list_scheduled_jobs(
        &self,
        status: Option<String>,
        job_kind: Option<String>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> CoreResult<Vec<ScheduledJobWireOutput>> {
        let status = status
            .as_deref()
            .map(scheduled_job_status_from_str)
            .transpose()?;
        self.engine()?
            .list_scheduled_jobs(status, job_kind, limit, offset)
            .map(|jobs| jobs.into_iter().map(scheduled_job_wire_output).collect())
    }

    pub fn list_scheduled_runs(
        &self,
        job_id: Option<String>,
        status: Option<String>,
        trigger: Option<String>,
        target_session_id: Option<String>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> CoreResult<Vec<ScheduledRunWireOutput>> {
        let status = status
            .as_deref()
            .map(scheduled_run_status_from_str)
            .transpose()?;
        let trigger = trigger
            .as_deref()
            .map(scheduled_run_trigger_from_str)
            .transpose()?;
        self.engine()?
            .list_scheduled_runs(
                job_id,
                status,
                trigger,
                target_session_id.map(SessionId::new),
                limit,
                offset,
            )
            .map(|runs| runs.into_iter().map(scheduled_run_wire_output).collect())
    }

    pub fn claim_scheduled_host_runs(
        &self,
        supported_job_kinds: Vec<String>,
        limit: Option<u32>,
    ) -> CoreResult<Vec<ScheduledRunWireOutput>> {
        self.engine()?
            .claim_scheduled_host_runs(supported_job_kinds, limit)
            .map(|runs| runs.into_iter().map(scheduled_run_wire_output).collect())
    }

    pub fn request_scheduled_host_job_run(
        &self,
        job_id: String,
        supported_job_kinds: Vec<String>,
    ) -> CoreResult<Option<ScheduledRunWireOutput>> {
        self.engine()?
            .request_scheduled_host_job_run(&job_id, supported_job_kinds)
            .map(|run| run.map(scheduled_run_wire_output))
    }

    pub fn complete_scheduled_host_run(
        &self,
        run_id: RunId,
        status: String,
        output_json: Value,
        error: Option<String>,
    ) -> CoreResult<Unit> {
        let status = scheduled_run_status_from_str(&status)?;
        self.engine()?
            .complete_scheduled_host_run(&run_id, status, output_json, error)?;
        Ok(Unit)
    }

    pub fn run_scheduler_tick(&self) -> CoreResult<SchedulerTickWireOutput> {
        self.engine()?
            .run_scheduler_tick()
            .map(|report| SchedulerTickWireOutput {
                stale_runs_expired: report.stale_runs_expired,
                due_runs_claimed: report.due_runs_claimed,
                wakes_requested: report.wakes_requested,
                runs_completed: report.runs_completed,
                runs_skipped: report.runs_skipped,
                runs_failed: report.runs_failed,
            })
    }

    pub fn request_scheduled_job_run(
        &self,
        job_id: String,
    ) -> CoreResult<Option<ScheduledRunWireOutput>> {
        self.engine()?
            .request_scheduled_job_run(&job_id)
            .map(|run| run.map(scheduled_run_wire_output))
    }

    pub fn pause_scheduled_job(&self, job_id: String) -> CoreResult<Unit> {
        self.engine()?.pause_scheduled_job(&job_id)?;
        Ok(Unit)
    }

    pub fn resume_scheduled_job(&self, job_id: String, next_due_at: String) -> CoreResult<Unit> {
        self.engine()?.resume_scheduled_job(&job_id, next_due_at)?;
        Ok(Unit)
    }
}

fn scheduled_job_wire_output(record: ScheduledJobRecord) -> ScheduledJobWireOutput {
    ScheduledJobWireOutput {
        job_id: record.job_id,
        job_kind: record.job_kind,
        target_session_id: record.target_session_id.map(|session_id| session_id.0),
        interval_ms: record.interval_ms,
        next_due_at: record.next_due_at,
        status: scheduled_job_status_as_str(record.status).to_owned(),
        created_at: record.created_at,
        updated_at: record.updated_at,
        paused_at: record.paused_at,
    }
}

fn scheduled_run_wire_output(record: ScheduledRunRecord) -> ScheduledRunWireOutput {
    ScheduledRunWireOutput {
        run_id: record.run_id.0,
        job_id: record.job_id,
        job_kind: record.job_kind,
        target_session_id: record.target_session_id.map(|session_id| session_id.0),
        status: scheduled_run_status_as_str(record.status).to_owned(),
        trigger: scheduled_run_trigger_as_str(record.trigger).to_owned(),
        scheduled_for: record.scheduled_for,
        claimed_at: record.claimed_at,
        claim_deadline_at: record.claim_deadline_at,
        completed_at: record.completed_at,
        error: record.error,
        output: record.output_json,
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

fn scheduled_job_status_as_str(status: ScheduledJobStatus) -> &'static str {
    match status {
        ScheduledJobStatus::Active => "active",
        ScheduledJobStatus::Paused => "paused",
        ScheduledJobStatus::Archived => "archived",
    }
}

fn scheduled_job_status_from_str(raw: &str) -> CoreResult<ScheduledJobStatus> {
    match raw {
        "active" => Ok(ScheduledJobStatus::Active),
        "paused" => Ok(ScheduledJobStatus::Paused),
        "archived" => Ok(ScheduledJobStatus::Archived),
        other => Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("unknown scheduled job status {other}"),
        )),
    }
}

fn scheduled_run_status_as_str(status: ScheduledRunStatus) -> &'static str {
    match status {
        ScheduledRunStatus::Claimed => "claimed",
        ScheduledRunStatus::Completed => "completed",
        ScheduledRunStatus::Skipped => "skipped",
        ScheduledRunStatus::Failed => "failed",
        ScheduledRunStatus::Expired => "expired",
        ScheduledRunStatus::Cancelled => "cancelled",
    }
}

fn scheduled_run_status_from_str(raw: &str) -> CoreResult<ScheduledRunStatus> {
    match raw {
        "claimed" => Ok(ScheduledRunStatus::Claimed),
        "completed" => Ok(ScheduledRunStatus::Completed),
        "skipped" => Ok(ScheduledRunStatus::Skipped),
        "failed" => Ok(ScheduledRunStatus::Failed),
        "expired" => Ok(ScheduledRunStatus::Expired),
        "cancelled" => Ok(ScheduledRunStatus::Cancelled),
        other => Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("unknown scheduled run status {other}"),
        )),
    }
}

fn scheduled_run_trigger_as_str(trigger: ScheduledRunTrigger) -> &'static str {
    match trigger {
        ScheduledRunTrigger::Due => "due",
        ScheduledRunTrigger::Manual => "manual",
    }
}

fn scheduled_run_trigger_from_str(raw: &str) -> CoreResult<ScheduledRunTrigger> {
    match raw {
        "due" => Ok(ScheduledRunTrigger::Due),
        "manual" => Ok(ScheduledRunTrigger::Manual),
        other => Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("unknown scheduled run trigger {other}"),
        )),
    }
}
