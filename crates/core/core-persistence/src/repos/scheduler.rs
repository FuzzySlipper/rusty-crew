use super::super::*;

pub(crate) fn migrate_v14_add_scheduler_persistence(
    tx: &rusqlite::Transaction<'_>,
) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS scheduled_jobs (
                job_id TEXT PRIMARY KEY,
                job_kind TEXT NOT NULL,
                target_session_id TEXT,
                interval_ms INTEGER,
                next_due_at TEXT,
                payload_json TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                paused_at TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_scheduled_jobs_due
                ON scheduled_jobs(status, next_due_at, job_id);
            CREATE INDEX IF NOT EXISTS idx_scheduled_jobs_kind_status
                ON scheduled_jobs(job_kind, status, job_id);

            CREATE TABLE IF NOT EXISTS scheduled_job_runs (
                run_id TEXT PRIMARY KEY,
                job_id TEXT NOT NULL,
                job_kind TEXT NOT NULL,
                target_session_id TEXT,
                status TEXT NOT NULL,
                trigger_kind TEXT NOT NULL,
                scheduled_for TEXT,
                claimed_at TEXT NOT NULL,
                claim_deadline_at TEXT NOT NULL,
                completed_at TEXT,
                error TEXT,
                output_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                FOREIGN KEY (job_id) REFERENCES scheduled_jobs(job_id)
            );
            CREATE INDEX IF NOT EXISTS idx_scheduled_job_runs_job_created
                ON scheduled_job_runs(job_id, created_at, run_id);
            CREATE INDEX IF NOT EXISTS idx_scheduled_job_runs_status_deadline
                ON scheduled_job_runs(status, claim_deadline_at, run_id);
            CREATE INDEX IF NOT EXISTS idx_scheduled_job_runs_target
                ON scheduled_job_runs(target_session_id, status, created_at);
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 14", error))
}

impl CoordinationStore {
    pub fn upsert_scheduled_job(&self, record: &ScheduledJobRecord) -> CoreResult<()> {
        let conn = self.conn()?;
        save_scheduled_job(&conn, record)
    }

    pub fn load_scheduled_job(&self, job_id: &str) -> CoreResult<Option<ScheduledJobRecord>> {
        let conn = self.conn()?;
        load_scheduled_job(&conn, job_id)
    }

    pub fn query_scheduled_jobs(
        &self,
        query: &ScheduledJobQuery,
    ) -> CoreResult<Vec<ScheduledJobRecord>> {
        let conn = self.conn()?;
        query_scheduled_jobs(&conn, query)
    }

    pub fn pause_scheduled_job(&self, job_id: &str, now: &IsoTimestamp) -> CoreResult<()> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE scheduled_jobs
             SET status = 'paused', paused_at = ?2, updated_at = ?2
             WHERE job_id = ?1 AND status != 'archived'",
            params![job_id, now],
        )
        .map_err(|error| persistence_error("pause scheduled job", error))?;
        Ok(())
    }

    pub fn resume_scheduled_job(
        &self,
        job_id: &str,
        next_due_at: &IsoTimestamp,
        now: &IsoTimestamp,
    ) -> CoreResult<()> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE scheduled_jobs
             SET status = 'active', next_due_at = ?2, paused_at = NULL, updated_at = ?3
             WHERE job_id = ?1 AND status != 'archived'",
            params![job_id, next_due_at, now],
        )
        .map_err(|error| persistence_error("resume scheduled job", error))?;
        Ok(())
    }

    pub fn claim_scheduled_run(
        &self,
        run: &ScheduledRunRecord,
        next_due_at: Option<&IsoTimestamp>,
    ) -> CoreResult<()> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start claim scheduled run", error))?;
        save_scheduled_run_in_tx(&tx, run)?;
        if run.trigger == ScheduledRunTrigger::Due {
            tx.execute(
                "UPDATE scheduled_jobs
                 SET next_due_at = ?2, updated_at = ?3
                 WHERE job_id = ?1 AND status = 'active'",
                params![run.job_id.as_str(), next_due_at, run.updated_at.as_str()],
            )
            .map_err(|error| persistence_error("advance scheduled job", error))?;
        }
        tx.commit()
            .map_err(|error| persistence_error("commit claim scheduled run", error))?;
        Ok(())
    }

    pub fn complete_scheduled_run(
        &self,
        run_id: &RunId,
        status: ScheduledRunStatus,
        completed_at: &IsoTimestamp,
        output_json: &JsonValue,
        error: Option<&str>,
    ) -> CoreResult<()> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE scheduled_job_runs
             SET status = ?2,
                 completed_at = ?3,
                 updated_at = ?3,
                 output_json = ?4,
                 error = ?5
             WHERE run_id = ?1",
            params![
                run_id.0.as_str(),
                scheduled_run_status_as_str(status),
                completed_at,
                to_json_text(output_json)?,
                error,
            ],
        )
        .map_err(|error| persistence_error("complete scheduled run", error))?;
        Ok(())
    }

    pub fn query_scheduled_runs(
        &self,
        query: &ScheduledRunQuery,
    ) -> CoreResult<Vec<ScheduledRunRecord>> {
        let conn = self.conn()?;
        query_scheduled_runs(&conn, query)
    }

    pub fn expire_stale_scheduled_runs(
        &self,
        stale_before: &IsoTimestamp,
        now: &IsoTimestamp,
    ) -> CoreResult<Vec<ScheduledRunRecord>> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start expire stale scheduled runs", error))?;
        let stale = query_scheduled_runs(
            &tx,
            &ScheduledRunQuery {
                status: Some(ScheduledRunStatus::Claimed),
                stale_claim_deadline_before: Some(stale_before.clone()),
                page: None,
                ..ScheduledRunQuery::default()
            },
        )?;
        for run in &stale {
            tx.execute(
                "UPDATE scheduled_job_runs
                 SET status = 'expired',
                     completed_at = ?2,
                     updated_at = ?2,
                     error = 'claim deadline elapsed'
                 WHERE run_id = ?1 AND status = 'claimed'",
                params![run.run_id.0.as_str(), now],
            )
            .map_err(|error| persistence_error("expire stale scheduled run", error))?;
        }
        tx.commit()
            .map_err(|error| persistence_error("commit expire stale scheduled runs", error))?;
        Ok(stale)
    }
}

fn save_scheduled_job(conn: &Connection, record: &ScheduledJobRecord) -> CoreResult<()> {
    conn.execute(
        "INSERT OR REPLACE INTO scheduled_jobs (
            job_id,
            job_kind,
            target_session_id,
            interval_ms,
            next_due_at,
            payload_json,
            status,
            created_at,
            updated_at,
            paused_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            record.job_id.as_str(),
            record.job_kind.as_str(),
            record
                .target_session_id
                .as_ref()
                .map(|session_id| session_id.0.as_str()),
            record.interval_ms.map(|value| value as i64),
            record.next_due_at.as_deref(),
            to_json_text(&record.payload_json)?,
            scheduled_job_status_as_str(record.status),
            record.created_at.as_str(),
            record.updated_at.as_str(),
            record.paused_at.as_deref(),
        ],
    )
    .map_err(|error| persistence_error("save scheduled job", error))?;
    Ok(())
}

fn load_scheduled_job(conn: &Connection, job_id: &str) -> CoreResult<Option<ScheduledJobRecord>> {
    conn.query_row(
        "SELECT
            job_id,
            job_kind,
            target_session_id,
            interval_ms,
            next_due_at,
            payload_json,
            status,
            created_at,
            updated_at,
            paused_at
         FROM scheduled_jobs
         WHERE job_id = ?1",
        params![job_id],
        row_to_scheduled_job,
    )
    .optional()
    .map_err(|error| persistence_error("load scheduled job", error))
}

fn query_scheduled_jobs(
    conn: &Connection,
    query: &ScheduledJobQuery,
) -> CoreResult<Vec<ScheduledJobRecord>> {
    let status = query.status.map(scheduled_job_status_as_str);
    let job_kind = query.job_kind.as_deref();
    let due_at_or_before = query.due_at_or_before.as_deref();
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let mut stmt = conn
        .prepare(
            "SELECT
                job_id,
                job_kind,
                target_session_id,
                interval_ms,
                next_due_at,
                payload_json,
                status,
                created_at,
                updated_at,
                paused_at
             FROM scheduled_jobs
             WHERE (?1 IS NULL OR status = ?1)
               AND (?2 IS NULL OR job_kind = ?2)
               AND (?3 IS NULL OR (next_due_at IS NOT NULL AND next_due_at <= ?3))
             ORDER BY COALESCE(next_due_at, created_at) ASC, job_id ASC
             LIMIT ?4 OFFSET ?5",
        )
        .map_err(|error| persistence_error("prepare scheduled jobs query", error))?;
    let rows = stmt
        .query_map(
            params![status, job_kind, due_at_or_before, limit, offset],
            row_to_scheduled_job,
        )
        .map_err(|error| persistence_error("query scheduled jobs", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load scheduled jobs", error))
}

fn row_to_scheduled_job(row: &rusqlite::Row<'_>) -> rusqlite::Result<ScheduledJobRecord> {
    let payload_json: String = row.get(5)?;
    let status: String = row.get(6)?;
    Ok(ScheduledJobRecord {
        job_id: row.get(0)?,
        job_kind: row.get(1)?,
        target_session_id: row.get::<_, Option<String>>(2)?.map(SessionId),
        interval_ms: row.get::<_, Option<i64>>(3)?.map(|value| value as u64),
        next_due_at: row.get(4)?,
        payload_json: from_json_text(&payload_json).map_err(to_sql_error)?,
        status: scheduled_job_status_from_str(&status)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        paused_at: row.get(9)?,
    })
}

fn save_scheduled_run_in_tx(
    tx: &rusqlite::Transaction<'_>,
    run: &ScheduledRunRecord,
) -> CoreResult<()> {
    tx.execute(
        "INSERT INTO scheduled_job_runs (
            run_id,
            job_id,
            job_kind,
            target_session_id,
            status,
            trigger_kind,
            scheduled_for,
            claimed_at,
            claim_deadline_at,
            completed_at,
            error,
            output_json,
            created_at,
            updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        params![
            run.run_id.0.as_str(),
            run.job_id.as_str(),
            run.job_kind.as_str(),
            run.target_session_id
                .as_ref()
                .map(|session_id| session_id.0.as_str()),
            scheduled_run_status_as_str(run.status),
            scheduled_run_trigger_as_str(run.trigger),
            run.scheduled_for.as_deref(),
            run.claimed_at.as_str(),
            run.claim_deadline_at.as_str(),
            run.completed_at.as_deref(),
            run.error.as_deref(),
            to_json_text(&run.output_json)?,
            run.created_at.as_str(),
            run.updated_at.as_str(),
        ],
    )
    .map_err(|error| persistence_error("save scheduled run", error))?;
    Ok(())
}

fn query_scheduled_runs(
    conn: &Connection,
    query: &ScheduledRunQuery,
) -> CoreResult<Vec<ScheduledRunRecord>> {
    let job_id = query.job_id.as_deref();
    let status = query.status.map(scheduled_run_status_as_str);
    let trigger = query.trigger.map(scheduled_run_trigger_as_str);
    let target_session_id = query
        .target_session_id
        .as_ref()
        .map(|session_id| session_id.0.as_str());
    let stale_before = query.stale_claim_deadline_before.as_deref();
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let mut stmt = conn
        .prepare(
            "SELECT
                run_id,
                job_id,
                job_kind,
                target_session_id,
                status,
                trigger_kind,
                scheduled_for,
                claimed_at,
                claim_deadline_at,
                completed_at,
                error,
                output_json,
                created_at,
                updated_at
             FROM scheduled_job_runs
             WHERE (?1 IS NULL OR job_id = ?1)
               AND (?2 IS NULL OR status = ?2)
               AND (?3 IS NULL OR trigger_kind = ?3)
               AND (?4 IS NULL OR target_session_id = ?4)
               AND (?5 IS NULL OR claim_deadline_at < ?5)
             ORDER BY created_at ASC, run_id ASC
             LIMIT ?6 OFFSET ?7",
        )
        .map_err(|error| persistence_error("prepare scheduled runs query", error))?;
    let rows = stmt
        .query_map(
            params![
                job_id,
                status,
                trigger,
                target_session_id,
                stale_before,
                limit,
                offset,
            ],
            row_to_scheduled_run,
        )
        .map_err(|error| persistence_error("query scheduled runs", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load scheduled runs", error))
}

fn row_to_scheduled_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<ScheduledRunRecord> {
    let status: String = row.get(4)?;
    let trigger: String = row.get(5)?;
    let output_json: String = row.get(11)?;
    Ok(ScheduledRunRecord {
        run_id: RunId(row.get(0)?),
        job_id: row.get(1)?,
        job_kind: row.get(2)?,
        target_session_id: row.get::<_, Option<String>>(3)?.map(SessionId),
        status: scheduled_run_status_from_str(&status)?,
        trigger: scheduled_run_trigger_from_str(&trigger)?,
        scheduled_for: row.get(6)?,
        claimed_at: row.get(7)?,
        claim_deadline_at: row.get(8)?,
        completed_at: row.get(9)?,
        error: row.get(10)?,
        output_json: from_json_text(&output_json).map_err(to_sql_error)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

fn scheduled_job_status_as_str(status: ScheduledJobStatus) -> &'static str {
    match status {
        ScheduledJobStatus::Active => "active",
        ScheduledJobStatus::Paused => "paused",
        ScheduledJobStatus::Archived => "archived",
    }
}

fn scheduled_job_status_from_str(raw: &str) -> rusqlite::Result<ScheduledJobStatus> {
    match raw {
        "active" => Ok(ScheduledJobStatus::Active),
        "paused" => Ok(ScheduledJobStatus::Paused),
        "archived" => Ok(ScheduledJobStatus::Archived),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::<dyn std::error::Error + Send + Sync>::from(format!(
                "unknown scheduled job status {other}",
            )),
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

fn scheduled_run_status_from_str(raw: &str) -> rusqlite::Result<ScheduledRunStatus> {
    match raw {
        "claimed" => Ok(ScheduledRunStatus::Claimed),
        "completed" => Ok(ScheduledRunStatus::Completed),
        "skipped" => Ok(ScheduledRunStatus::Skipped),
        "failed" => Ok(ScheduledRunStatus::Failed),
        "expired" => Ok(ScheduledRunStatus::Expired),
        "cancelled" => Ok(ScheduledRunStatus::Cancelled),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::<dyn std::error::Error + Send + Sync>::from(format!(
                "unknown scheduled run status {other}",
            )),
        )),
    }
}

fn scheduled_run_trigger_as_str(trigger: ScheduledRunTrigger) -> &'static str {
    match trigger {
        ScheduledRunTrigger::Due => "due",
        ScheduledRunTrigger::Manual => "manual",
    }
}

fn scheduled_run_trigger_from_str(raw: &str) -> rusqlite::Result<ScheduledRunTrigger> {
    match raw {
        "due" => Ok(ScheduledRunTrigger::Due),
        "manual" => Ok(ScheduledRunTrigger::Manual),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::<dyn std::error::Error + Send + Sync>::from(format!(
                "unknown scheduled run trigger {other}",
            )),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn scheduler_repo_claims_due_run_and_expires_stale_claim() {
        let db_path = std::env::temp_dir().join(format!(
            "rusty-crew-scheduler-repo-{}.sqlite3",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = CoordinationStore::open_file(&db_path).unwrap();
        store
            .upsert_scheduled_job(&ScheduledJobRecord {
                job_id: "wake-alpha".to_string(),
                job_kind: "brain_wake".to_string(),
                target_session_id: Some(SessionId::new("session-alpha")),
                interval_ms: Some(60_000),
                next_due_at: Some("2026-07-02T00:01:00Z".to_string()),
                payload_json: JsonValue::Null,
                status: ScheduledJobStatus::Active,
                created_at: "2026-07-02T00:00:00Z".to_string(),
                updated_at: "2026-07-02T00:00:00Z".to_string(),
                paused_at: None,
            })
            .unwrap();
        store
            .claim_scheduled_run(
                &ScheduledRunRecord {
                    run_id: RunId::new("wake-alpha:0"),
                    job_id: "wake-alpha".to_string(),
                    job_kind: "brain_wake".to_string(),
                    target_session_id: Some(SessionId::new("session-alpha")),
                    status: ScheduledRunStatus::Claimed,
                    trigger: ScheduledRunTrigger::Due,
                    scheduled_for: Some("2026-07-02T00:01:00Z".to_string()),
                    claimed_at: "2026-07-02T00:01:00Z".to_string(),
                    claim_deadline_at: "2026-07-02T00:02:00Z".to_string(),
                    completed_at: None,
                    error: None,
                    output_json: JsonValue::Null,
                    created_at: "2026-07-02T00:01:00Z".to_string(),
                    updated_at: "2026-07-02T00:01:00Z".to_string(),
                },
                Some(&"2026-07-02T00:03:00Z".to_string()),
            )
            .unwrap();

        assert_eq!(
            store
                .load_scheduled_job("wake-alpha")
                .unwrap()
                .unwrap()
                .next_due_at,
            Some("2026-07-02T00:03:00Z".to_string())
        );

        let stale = store
            .expire_stale_scheduled_runs(
                &"2026-07-02T00:02:30Z".to_string(),
                &"2026-07-02T00:02:31Z".to_string(),
            )
            .unwrap();
        let expired = store
            .query_scheduled_runs(&ScheduledRunQuery {
                status: Some(ScheduledRunStatus::Expired),
                ..ScheduledRunQuery::default()
            })
            .unwrap();

        assert_eq!(stale.len(), 1);
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].error.as_deref(), Some("claim deadline elapsed"));

        drop(store);
        let _ = std::fs::remove_file(db_path);
    }
}
