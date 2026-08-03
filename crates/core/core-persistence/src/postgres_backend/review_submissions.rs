use super::*;
use crate::repos::review_submissions::review_submission_phase_as_str;

pub(super) fn apply_postgres_review_submissions(
    tx: &mut Transaction<'_>,
    schema: &str,
) -> CoreResult<()> {
    tx.batch_execute(&format!(
        "CREATE TABLE {schema}.review_submissions (
            submission_id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL,
            task_id TEXT NOT NULL,
            commit_sha TEXT NOT NULL,
            submitter_session_id TEXT NOT NULL,
            phase TEXT NOT NULL,
            revision BIGINT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            record_json TEXT NOT NULL
         );
         CREATE UNIQUE INDEX review_submissions_task_sha_session_idx
            ON {schema}.review_submissions(task_id, commit_sha, submitter_session_id);
         CREATE INDEX review_submissions_task_phase_idx
            ON {schema}.review_submissions(task_id, phase, updated_at DESC);
         CREATE INDEX review_submissions_session_phase_idx
            ON {schema}.review_submissions(submitter_session_id, phase, updated_at DESC);"
    ))
    .map_err(|error| postgres_error("create PostgreSQL review submissions table", error))
}

impl PostgresBackendStore {
    pub fn get_review_submission(
        &self,
        submission_id: &str,
    ) -> CoreResult<Option<ReviewSubmissionRecord>> {
        let schema = self.quoted_schema();
        self.client()?
            .query_opt(
                &format!(
                    "SELECT record_json FROM {schema}.review_submissions WHERE submission_id = $1"
                ),
                &[&submission_id],
            )
            .map_err(|error| postgres_error("get PostgreSQL review submission", error))?
            .map(|row| decode_review_submission(row.get(0)))
            .transpose()
    }

    pub fn list_review_submissions(&self) -> CoreResult<Vec<ReviewSubmissionRecord>> {
        let schema = self.quoted_schema();
        self.client()?
            .query(
                &format!(
                    "SELECT record_json FROM {schema}.review_submissions ORDER BY updated_at, submission_id"
                ),
                &[],
            )
            .map_err(|error| postgres_error("list PostgreSQL review submissions", error))?
            .into_iter()
            .map(|row| decode_review_submission(row.get(0)))
            .collect()
    }

    pub fn insert_review_submission(
        &self,
        record: &ReviewSubmissionRecord,
    ) -> CoreResult<ReviewSubmissionRecord> {
        let schema = self.quoted_schema();
        let phase = review_submission_phase_as_str(record.phase);
        let revision = record.revision as i64;
        let record_json = to_json_text(record)?;
        self.client()?
            .execute(
                &format!(
                    "INSERT INTO {schema}.review_submissions (
                        submission_id, project_id, task_id, commit_sha, submitter_session_id,
                        phase, revision, created_at, updated_at, record_json
                     ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"
                ),
                &[
                    &record.submission_id,
                    &record.project_id.0,
                    &record.task_id.0,
                    &record.commit_sha,
                    &record.submitter_session_id.0,
                    &phase,
                    &revision,
                    &record.created_at,
                    &record.updated_at,
                    &record_json,
                ],
            )
            .map_err(|error| {
                if error.code() == Some(&postgres::error::SqlState::UNIQUE_VIOLATION) {
                    CoreError::new(
                        CoreErrorKind::AlreadyExists,
                        format!("review submission {} already exists", record.submission_id),
                    )
                } else {
                    postgres_error("insert PostgreSQL review submission", error)
                }
            })?;
        Ok(record.clone())
    }

    pub fn update_review_submission(
        &self,
        record: &ReviewSubmissionRecord,
        expected_revision: u64,
    ) -> CoreResult<ReviewSubmissionRecord> {
        let schema = self.quoted_schema();
        let phase = review_submission_phase_as_str(record.phase);
        let revision = record.revision as i64;
        let expected_revision = expected_revision as i64;
        let record_json = to_json_text(record)?;
        let changed = self
            .client()?
            .execute(
                &format!(
                    "UPDATE {schema}.review_submissions
                        SET phase = $1, revision = $2, updated_at = $3, record_json = $4
                      WHERE submission_id = $5 AND revision = $6"
                ),
                &[
                    &phase,
                    &revision,
                    &record.updated_at,
                    &record_json,
                    &record.submission_id,
                    &expected_revision,
                ],
            )
            .map_err(|error| postgres_error("update PostgreSQL review submission", error))?;
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
            format!("decode PostgreSQL review submission: {error}"),
        )
    })
}
