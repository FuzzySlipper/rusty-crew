use super::super::*;

pub(crate) fn migrate_v61_add_review_submissions(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "CREATE TABLE review_submissions (
            submission_id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL,
            task_id TEXT NOT NULL,
            commit_sha TEXT NOT NULL,
            submitter_session_id TEXT NOT NULL,
            phase TEXT NOT NULL,
            revision INTEGER NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            record_json TEXT NOT NULL
         );
         CREATE UNIQUE INDEX idx_review_submissions_task_sha_session
            ON review_submissions(task_id, commit_sha, submitter_session_id);
         CREATE INDEX idx_review_submissions_task_phase
            ON review_submissions(task_id, phase, updated_at DESC);
         CREATE INDEX idx_review_submissions_session_phase
            ON review_submissions(submitter_session_id, phase, updated_at DESC);",
    )
    .map_err(|error| persistence_error("apply schema migration 61", error))
}

pub(crate) fn migrate_v62_allow_external_review_submitters(
    tx: &rusqlite::Transaction<'_>,
) -> CoreResult<()> {
    tx.execute_batch(
        "DROP INDEX IF EXISTS idx_review_submissions_task_sha_session;
         DROP INDEX IF EXISTS idx_review_submissions_session_phase;
         ALTER TABLE review_submissions RENAME TO review_submissions_v61;
         CREATE TABLE review_submissions (
            submission_id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL,
            task_id TEXT NOT NULL,
            commit_sha TEXT NOT NULL,
            submitter_session_id TEXT,
            phase TEXT NOT NULL,
            revision INTEGER NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            record_json TEXT NOT NULL
         );
         INSERT INTO review_submissions (
            submission_id, project_id, task_id, commit_sha, submitter_session_id,
            phase, revision, created_at, updated_at, record_json
         )
         SELECT submission_id, project_id, task_id, commit_sha, submitter_session_id,
                phase, revision, created_at, updated_at, record_json
           FROM review_submissions_v61;
         DROP TABLE review_submissions_v61;
         CREATE UNIQUE INDEX idx_review_submissions_task_sha_session
            ON review_submissions(task_id, commit_sha, submitter_session_id);
         CREATE INDEX idx_review_submissions_session_phase
            ON review_submissions(submitter_session_id, phase, updated_at DESC);",
    )
    .map_err(|error| persistence_error("apply schema migration 62", error))
}

impl CoordinationStore {
    pub fn get_review_submission(
        &self,
        submission_id: &str,
    ) -> CoreResult<Option<ReviewSubmissionRecord>> {
        let conn = self.conn()?;
        conn.query_row(
            "SELECT record_json FROM review_submissions WHERE submission_id = ?1",
            params![submission_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| persistence_error("get review submission", error))?
        .map(|raw| decode_review_submission(&raw))
        .transpose()
    }

    pub fn list_review_submissions(&self) -> CoreResult<Vec<ReviewSubmissionRecord>> {
        let conn = self.conn()?;
        let mut statement = conn
            .prepare(
                "SELECT record_json FROM review_submissions ORDER BY updated_at, submission_id",
            )
            .map_err(|error| persistence_error("prepare review submission list", error))?;
        let records = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| persistence_error("query review submissions", error))?
            .map(|raw| {
                let raw =
                    raw.map_err(|error| persistence_error("read review submission row", error))?;
                decode_review_submission(&raw)
            })
            .collect();
        records
    }

    pub fn list_review_submissions_page(
        &self,
        project_id: Option<&str>,
        limit: u32,
        offset: u64,
    ) -> CoreResult<Vec<ReviewSubmissionRecord>> {
        let conn = self.conn()?;
        let mut statement = conn
            .prepare(
                "SELECT record_json FROM review_submissions
                 WHERE (?1 IS NULL OR project_id = ?1)
                 ORDER BY updated_at DESC, submission_id
                 LIMIT ?2 OFFSET ?3",
            )
            .map_err(|error| persistence_error("prepare bounded review submission list", error))?;
        let records = statement
            .query_map(
                params![project_id, i64::from(limit), offset as i64],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| persistence_error("query bounded review submissions", error))?
            .map(|raw| {
                decode_review_submission(&raw.map_err(|error| {
                    persistence_error("read bounded review submission row", error)
                })?)
            })
            .collect();
        records
    }

    pub fn insert_review_submission(
        &self,
        record: &ReviewSubmissionRecord,
    ) -> CoreResult<ReviewSubmissionRecord> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO review_submissions (
                submission_id, project_id, task_id, commit_sha, submitter_session_id,
                phase, revision, created_at, updated_at, record_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                record.submission_id,
                record.project_id.0,
                record.task_id.0,
                record.commit_sha,
                record
                    .submitter_session_id
                    .as_ref()
                    .map(|session| session.0.as_str()),
                review_submission_phase_as_str(record.phase),
                record.revision as i64,
                record.created_at,
                record.updated_at,
                to_json_text(record)?,
            ],
        )
        .map_err(|error| {
            if error.sqlite_error_code() == Some(rusqlite::ErrorCode::ConstraintViolation) {
                CoreError::new(
                    CoreErrorKind::AlreadyExists,
                    format!("review submission {} already exists", record.submission_id),
                )
            } else {
                persistence_error("insert review submission", error)
            }
        })?;
        Ok(record.clone())
    }

    pub fn update_review_submission(
        &self,
        record: &ReviewSubmissionRecord,
        expected_revision: u64,
    ) -> CoreResult<ReviewSubmissionRecord> {
        let conn = self.conn()?;
        let changed = conn
            .execute(
                "UPDATE review_submissions
                    SET phase = ?1, revision = ?2, updated_at = ?3, record_json = ?4
                  WHERE submission_id = ?5 AND revision = ?6",
                params![
                    review_submission_phase_as_str(record.phase),
                    record.revision as i64,
                    record.updated_at,
                    to_json_text(record)?,
                    record.submission_id,
                    expected_revision as i64,
                ],
            )
            .map_err(|error| persistence_error("update review submission", error))?;
        if changed != 1 {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "review submission {} revision mismatch: expected {}",
                    record.submission_id, expected_revision
                ),
            ));
        }
        Ok(record.clone())
    }
}

fn decode_review_submission(raw: &str) -> CoreResult<ReviewSubmissionRecord> {
    from_json_text(raw).map_err(|error| {
        CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("decode review submission: {error}"),
        )
    })
}

pub(crate) fn review_submission_phase_as_str(phase: ReviewSubmissionPhase) -> &'static str {
    match phase {
        ReviewSubmissionPhase::Submitted => "submitted",
        ReviewSubmissionPhase::DenHandoffRecorded => "den_handoff_recorded",
        ReviewSubmissionPhase::GatePending => "gate_pending",
        ReviewSubmissionPhase::GateFailed => "gate_failed",
        ReviewSubmissionPhase::ReviewerDispatchPending => "reviewer_dispatch_pending",
        ReviewSubmissionPhase::ReviewerDispatched => "reviewer_dispatched",
        ReviewSubmissionPhase::DenFinalizationPending => "den_finalization_pending",
        ReviewSubmissionPhase::DenFinalized => "den_finalized",
        ReviewSubmissionPhase::ReplyPending => "reply_pending",
        ReviewSubmissionPhase::Replied => "replied",
        ReviewSubmissionPhase::ReplyTerminal => "reply_terminal",
        ReviewSubmissionPhase::ReviewTerminal => "review_terminal",
        ReviewSubmissionPhase::Superseded => "superseded",
    }
}
