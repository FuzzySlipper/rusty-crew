use super::*;
use crate::repos::runtime_activities::runtime_activity_status_as_str;

const DEFAULT_ACTIVITY_LIMIT: u32 = 500;
const MAX_ACTIVITY_LIMIT: u32 = 5_000;

pub(super) fn apply_postgres_runtime_activities(
    tx: &mut Transaction<'_>,
    schema: &str,
) -> CoreResult<()> {
    tx.batch_execute(&format!(
        "CREATE TABLE {schema}.runtime_activities (
            activity_id TEXT PRIMARY KEY,
            service_instance_id TEXT NOT NULL,
            parent_activity_id TEXT,
            kind TEXT NOT NULL,
            status TEXT NOT NULL,
            session_id TEXT,
            wake_id TEXT,
            started_at TEXT NOT NULL,
            last_progress_at TEXT NOT NULL,
            terminal_at TEXT,
            revision BIGINT NOT NULL,
            record_json TEXT NOT NULL
         );
         CREATE INDEX runtime_activities_status_progress_idx
            ON {schema}.runtime_activities(status, last_progress_at DESC, activity_id);
         CREATE INDEX runtime_activities_session_status_idx
            ON {schema}.runtime_activities(session_id, status, last_progress_at DESC);
         CREATE INDEX runtime_activities_wake_idx
            ON {schema}.runtime_activities(wake_id, activity_id);
         CREATE INDEX runtime_activities_parent_idx
            ON {schema}.runtime_activities(parent_activity_id, status, activity_id);"
    ))
    .map_err(|error| postgres_error("create PostgreSQL runtime activity table", error))
}

impl PostgresBackendStore {
    pub fn get_runtime_activity(
        &self,
        activity_id: &RuntimeActivityId,
    ) -> CoreResult<Option<RuntimeActivityRecord>> {
        let schema = self.quoted_schema();
        self.client()?
            .query_opt(
                &format!(
                    "SELECT record_json FROM {schema}.runtime_activities WHERE activity_id = $1"
                ),
                &[&activity_id.0],
            )
            .map_err(|error| postgres_error("get PostgreSQL runtime activity", error))?
            .map(|row| {
                from_json_text::<RuntimeActivityRecord>(row.get(0)).map_err(|error| {
                    CoreError::new(
                        CoreErrorKind::PersistenceFailure,
                        format!("decode PostgreSQL runtime activity: {error}"),
                    )
                })
            })
            .transpose()
    }

    pub fn insert_runtime_activity(
        &self,
        record: &RuntimeActivityRecord,
    ) -> CoreResult<RuntimeActivityRecord> {
        let schema = self.quoted_schema();
        let parent_activity_id = record.parent_activity_id.as_ref().map(|id| id.0.as_str());
        let session_id = record.session_id.as_ref().map(|id| id.0.as_str());
        let kind = crate::repos::runtime_activities::runtime_activity_kind_as_str(record.kind);
        let status = runtime_activity_status_as_str(record.status);
        let revision = record.revision as i64;
        let record_json = to_json_text(record)?;
        self.client()?
            .execute(
                &format!(
                    "INSERT INTO {schema}.runtime_activities (
                        activity_id, service_instance_id, parent_activity_id, kind, status,
                        session_id, wake_id, started_at, last_progress_at, terminal_at,
                        revision, record_json
                     ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)"
                ),
                &[
                    &record.activity_id.0,
                    &record.service_instance_id,
                    &parent_activity_id,
                    &kind,
                    &status,
                    &session_id,
                    &record.wake_id,
                    &record.started_at,
                    &record.last_progress_at,
                    &record.terminal_at,
                    &revision,
                    &record_json,
                ],
            )
            .map_err(|error| {
                if error.code() == Some(&postgres::error::SqlState::UNIQUE_VIOLATION) {
                    CoreError::new(
                        CoreErrorKind::AlreadyExists,
                        format!("runtime activity {} already exists", record.activity_id.0),
                    )
                } else {
                    postgres_error("insert PostgreSQL runtime activity", error)
                }
            })?;
        Ok(record.clone())
    }

    pub fn update_runtime_activity(
        &self,
        record: &RuntimeActivityRecord,
        expected_revision: u64,
    ) -> CoreResult<RuntimeActivityRecord> {
        let schema = self.quoted_schema();
        let status = runtime_activity_status_as_str(record.status);
        let revision = record.revision as i64;
        let expected_revision = expected_revision as i64;
        let record_json = to_json_text(record)?;
        let changed = self
            .client()?
            .execute(
                &format!(
                    "UPDATE {schema}.runtime_activities
                     SET status = $1, last_progress_at = $2, terminal_at = $3,
                         revision = $4, record_json = $5
                     WHERE activity_id = $6 AND revision = $7"
                ),
                &[
                    &status,
                    &record.last_progress_at,
                    &record.terminal_at,
                    &revision,
                    &record_json,
                    &record.activity_id.0,
                    &expected_revision,
                ],
            )
            .map_err(|error| postgres_error("update PostgreSQL runtime activity", error))?;
        if changed == 0 {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "runtime activity {} revision mismatch or missing: expected {}",
                    record.activity_id.0, expected_revision
                ),
            ));
        }
        Ok(record.clone())
    }

    pub fn list_runtime_activities(
        &self,
        status: Option<RuntimeActivityStatus>,
        limit: Option<u32>,
    ) -> CoreResult<Vec<RuntimeActivityRecord>> {
        let schema = self.quoted_schema();
        let status = status.map(runtime_activity_status_as_str);
        let limit = i64::from(
            limit
                .unwrap_or(DEFAULT_ACTIVITY_LIMIT)
                .clamp(1, MAX_ACTIVITY_LIMIT),
        );
        self.client()?
            .query(
                &format!(
                    "SELECT record_json FROM {schema}.runtime_activities
                     WHERE ($1::text IS NULL OR status = $1)
                     ORDER BY last_progress_at DESC, activity_id ASC
                     LIMIT $2"
                ),
                &[&status, &limit],
            )
            .map_err(|error| postgres_error("query PostgreSQL runtime activities", error))?
            .iter()
            .map(|row| {
                from_json_text::<RuntimeActivityRecord>(row.get(0)).map_err(|error| {
                    CoreError::new(
                        CoreErrorKind::PersistenceFailure,
                        format!("decode PostgreSQL runtime activity: {error}"),
                    )
                })
            })
            .collect()
    }

    pub fn interrupt_runtime_activities_from_other_instances(
        &self,
        current_service_instance_id: &str,
        now: &IsoTimestamp,
    ) -> CoreResult<Vec<RuntimeActivityRecord>> {
        let active = self.list_runtime_activities(Some(RuntimeActivityStatus::Active), None)?;
        let mut interrupted = Vec::new();
        for mut record in active {
            if record.service_instance_id == current_service_instance_id {
                continue;
            }
            let expected_revision = record.revision;
            record.status = RuntimeActivityStatus::Interrupted;
            record.phase = "restart_interrupted".to_string();
            record.reason_code = Some("restart_interrupted".to_string());
            record.summary = Some("service restart interrupted unfinished runtime activity".into());
            record.last_progress_at = now.clone();
            record.terminal_at = Some(now.clone());
            record.revision += 1;
            interrupted.push(self.update_runtime_activity(&record, expected_revision)?);
        }
        Ok(interrupted)
    }
}
