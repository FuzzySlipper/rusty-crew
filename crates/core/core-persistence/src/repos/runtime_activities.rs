use super::super::*;

const DEFAULT_ACTIVITY_LIMIT: u32 = 500;
const MAX_ACTIVITY_LIMIT: u32 = 5_000;

pub(crate) fn migrate_v56_add_runtime_activities(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
        CREATE TABLE runtime_activities (
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
            revision INTEGER NOT NULL,
            record_json TEXT NOT NULL
        );
        CREATE INDEX idx_runtime_activities_status_progress
            ON runtime_activities(status, last_progress_at DESC, activity_id);
        CREATE INDEX idx_runtime_activities_session_status
            ON runtime_activities(session_id, status, last_progress_at DESC);
        CREATE INDEX idx_runtime_activities_wake
            ON runtime_activities(wake_id, activity_id);
        CREATE INDEX idx_runtime_activities_parent
            ON runtime_activities(parent_activity_id, status, activity_id);
        ",
    )
    .map_err(|error| persistence_error("apply schema migration 56", error))
}

impl CoordinationStore {
    pub fn get_runtime_activity(
        &self,
        activity_id: &RuntimeActivityId,
    ) -> CoreResult<Option<RuntimeActivityRecord>> {
        let conn = self.conn()?;
        conn.query_row(
            "SELECT record_json FROM runtime_activities WHERE activity_id = ?1",
            params![activity_id.0],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| persistence_error("get runtime activity", error))?
        .map(|raw| {
            from_json_text(&raw).map_err(|error| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    format!("decode runtime activity: {error}"),
                )
            })
        })
        .transpose()
    }

    pub fn insert_runtime_activity(
        &self,
        record: &RuntimeActivityRecord,
    ) -> CoreResult<RuntimeActivityRecord> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO runtime_activities (
                activity_id, service_instance_id, parent_activity_id, kind, status,
                session_id, wake_id, started_at, last_progress_at, terminal_at,
                revision, record_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                record.activity_id.0,
                record.service_instance_id,
                record.parent_activity_id.as_ref().map(|id| id.0.as_str()),
                runtime_activity_kind_as_str(record.kind),
                runtime_activity_status_as_str(record.status),
                record.session_id.as_ref().map(|id| id.0.as_str()),
                record.wake_id,
                record.started_at,
                record.last_progress_at,
                record.terminal_at,
                record.revision as i64,
                to_json_text(record)?,
            ],
        )
        .map_err(|error| {
            if error.sqlite_error_code() == Some(rusqlite::ErrorCode::ConstraintViolation) {
                CoreError::new(
                    CoreErrorKind::AlreadyExists,
                    format!("runtime activity {} already exists", record.activity_id.0),
                )
            } else {
                persistence_error("insert runtime activity", error)
            }
        })?;
        Ok(record.clone())
    }

    pub fn update_runtime_activity(
        &self,
        record: &RuntimeActivityRecord,
        expected_revision: u64,
    ) -> CoreResult<RuntimeActivityRecord> {
        let conn = self.conn()?;
        let changed = conn
            .execute(
                "UPDATE runtime_activities
                 SET status = ?1, last_progress_at = ?2, terminal_at = ?3,
                     revision = ?4, record_json = ?5
                 WHERE activity_id = ?6 AND revision = ?7",
                params![
                    runtime_activity_status_as_str(record.status),
                    record.last_progress_at,
                    record.terminal_at,
                    record.revision as i64,
                    to_json_text(record)?,
                    record.activity_id.0,
                    expected_revision as i64,
                ],
            )
            .map_err(|error| persistence_error("update runtime activity", error))?;
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
        let conn = self.conn()?;
        let status = status.map(runtime_activity_status_as_str);
        let limit = limit
            .unwrap_or(DEFAULT_ACTIVITY_LIMIT)
            .clamp(1, MAX_ACTIVITY_LIMIT);
        let mut stmt = conn
            .prepare(
                "SELECT record_json FROM runtime_activities
                 WHERE (?1 IS NULL OR status = ?1)
                 ORDER BY last_progress_at DESC, activity_id ASC
                 LIMIT ?2",
            )
            .map_err(|error| persistence_error("prepare runtime activity query", error))?;
        let rows = stmt
            .query_map(params![status, limit], |row| row.get::<_, String>(0))
            .map_err(|error| persistence_error("query runtime activities", error))?;
        rows.map(|row| {
            row.map_err(|error| persistence_error("read runtime activity", error))
                .and_then(|raw| {
                    from_json_text(&raw).map_err(|error| {
                        CoreError::new(
                            CoreErrorKind::PersistenceFailure,
                            format!("decode runtime activity: {error}"),
                        )
                    })
                })
        })
        .collect()
    }

    pub fn list_runtime_activities_for_session(
        &self,
        session_id: &SessionId,
        limit: Option<u32>,
    ) -> CoreResult<Vec<RuntimeActivityRecord>> {
        let conn = self.conn()?;
        let limit = limit
            .unwrap_or(DEFAULT_ACTIVITY_LIMIT)
            .clamp(1, MAX_ACTIVITY_LIMIT);
        let mut stmt = conn
            .prepare(
                "SELECT record_json FROM runtime_activities
                 WHERE session_id = ?1
                 ORDER BY last_progress_at DESC, activity_id ASC
                 LIMIT ?2",
            )
            .map_err(|error| persistence_error("prepare session runtime activity query", error))?;
        let rows = stmt
            .query_map(params![session_id.0, limit], |row| row.get::<_, String>(0))
            .map_err(|error| persistence_error("query session runtime activities", error))?;
        rows.map(|row| {
            row.map_err(|error| persistence_error("read session runtime activity", error))
                .and_then(|raw| {
                    from_json_text(&raw).map_err(|error| {
                        CoreError::new(
                            CoreErrorKind::PersistenceFailure,
                            format!("decode session runtime activity: {error}"),
                        )
                    })
                })
        })
        .collect()
    }

    pub fn interrupt_runtime_activities_from_other_instances(
        &self,
        current_service_instance_id: &str,
        now: &IsoTimestamp,
    ) -> CoreResult<Vec<RuntimeActivityRecord>> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("begin runtime activity interruption", error))?;
        let mut stmt = tx
            .prepare(
                "SELECT record_json FROM runtime_activities
                 WHERE status = ?1 AND service_instance_id <> ?2
                 ORDER BY last_progress_at DESC, activity_id ASC",
            )
            .map_err(|error| {
                persistence_error("prepare runtime activity interruption query", error)
            })?;
        let active = stmt
            .query_map(
                params![
                    runtime_activity_status_as_str(RuntimeActivityStatus::Active),
                    current_service_instance_id
                ],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| persistence_error("query runtime activities to interrupt", error))?
            .map(|row| {
                row.map_err(|error| persistence_error("read runtime activity to interrupt", error))
                    .and_then(|raw| {
                        from_json_text(&raw).map_err(|error| {
                            CoreError::new(
                                CoreErrorKind::PersistenceFailure,
                                format!("decode runtime activity to interrupt: {error}"),
                            )
                        })
                    })
            })
            .collect::<CoreResult<Vec<RuntimeActivityRecord>>>()?;
        drop(stmt);
        let mut interrupted = Vec::new();
        for mut record in active {
            let expected_revision = record.revision;
            interrupt_runtime_activity_record(&mut record, now);
            let changed = tx
                .execute(
                    "UPDATE runtime_activities
                     SET status = ?1, last_progress_at = ?2, terminal_at = ?3,
                         revision = ?4, record_json = ?5
                     WHERE activity_id = ?6 AND revision = ?7 AND status = ?8",
                    params![
                        runtime_activity_status_as_str(record.status),
                        record.last_progress_at,
                        record.terminal_at,
                        record.revision as i64,
                        to_json_text(&record)?,
                        record.activity_id.0,
                        expected_revision as i64,
                        runtime_activity_status_as_str(RuntimeActivityStatus::Active),
                    ],
                )
                .map_err(|error| persistence_error("interrupt runtime activity", error))?;
            if changed != 1 {
                return Err(CoreError::new(
                    CoreErrorKind::ActionRejected,
                    format!(
                        "runtime activity {} changed during restart interruption",
                        record.activity_id.0
                    ),
                ));
            }
            interrupted.push(record);
        }
        tx.commit()
            .map_err(|error| persistence_error("commit runtime activity interruption", error))?;
        Ok(interrupted)
    }
}

pub(crate) fn interrupt_runtime_activity_record(
    record: &mut RuntimeActivityRecord,
    now: &IsoTimestamp,
) {
    record.status = RuntimeActivityStatus::Interrupted;
    record.phase = "restart_interrupted".to_string();
    record.reason_code = Some("restart_interrupted".to_string());
    record.summary = Some("service restart interrupted unfinished runtime activity".into());
    record.last_progress_at = now.clone();
    record.terminal_at = Some(now.clone());
    record.revision += 1;
}

pub(crate) fn runtime_activity_kind_as_str(kind: RuntimeActivityKind) -> &'static str {
    match kind {
        RuntimeActivityKind::Dispatch => "dispatch",
        RuntimeActivityKind::Wake => "wake",
        RuntimeActivityKind::ProviderRequest => "provider_request",
        RuntimeActivityKind::ToolCall => "tool_call",
        RuntimeActivityKind::Subprocess => "subprocess",
        RuntimeActivityKind::Browser => "browser",
        RuntimeActivityKind::ExternalTurn => "external_turn",
    }
}

pub(crate) fn runtime_activity_status_as_str(status: RuntimeActivityStatus) -> &'static str {
    match status {
        RuntimeActivityStatus::Active => "active",
        RuntimeActivityStatus::Completed => "completed",
        RuntimeActivityStatus::Failed => "failed",
        RuntimeActivityStatus::Cancelled => "cancelled",
        RuntimeActivityStatus::Interrupted => "interrupted",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    #[test]
    fn sqlite_runtime_activity_lifecycle_is_revisioned_and_restart_safe() {
        let path = temp_db_path();
        let store = CoordinationStore::open_file(&path).unwrap();
        let record = test_record("sqlite-instance");
        assert_eq!(store.insert_runtime_activity(&record).unwrap(), record);
        assert_eq!(
            store.insert_runtime_activity(&record).unwrap_err().kind,
            CoreErrorKind::AlreadyExists
        );

        let mut progressed = record.clone();
        progressed.phase = "provider_stream".into();
        progressed.last_progress_at = "2026-07-22T00:00:01Z".into();
        progressed.revision = 2;
        assert_eq!(
            store.update_runtime_activity(&progressed, 1).unwrap(),
            progressed
        );
        assert_eq!(
            store
                .update_runtime_activity(&progressed, 1)
                .unwrap_err()
                .kind,
            CoreErrorKind::ActionRejected
        );

        let interrupted = store
            .interrupt_runtime_activities_from_other_instances(
                "new-instance",
                &"2026-07-22T00:00:02Z".into(),
            )
            .unwrap();
        assert_eq!(interrupted.len(), 1);
        assert_eq!(interrupted[0].status, RuntimeActivityStatus::Interrupted);
        assert!(store
            .list_runtime_activities(Some(RuntimeActivityStatus::Active), None)
            .unwrap()
            .is_empty());
        drop(store);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn sqlite_restart_interruption_exhausts_more_than_default_query_limit() {
        let path = temp_db_path();
        let store = CoordinationStore::open_file(&path).unwrap();
        for index in 0..=DEFAULT_ACTIVITY_LIMIT {
            store
                .insert_runtime_activity(&test_record_with_id(
                    "old-instance",
                    &format!("wake:old-{index}"),
                ))
                .unwrap();
        }
        store
            .insert_runtime_activity(&test_record_with_id("current-instance", "wake:current"))
            .unwrap();

        let interrupted = store
            .interrupt_runtime_activities_from_other_instances(
                "current-instance",
                &"2026-07-22T00:00:02Z".into(),
            )
            .unwrap();
        assert_eq!(interrupted.len(), DEFAULT_ACTIVITY_LIMIT as usize + 1);
        assert!(interrupted.iter().all(|record| {
            record.status == RuntimeActivityStatus::Interrupted
                && record.reason_code.as_deref() == Some("restart_interrupted")
        }));
        let active = store
            .list_runtime_activities(Some(RuntimeActivityStatus::Active), None)
            .unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].activity_id.0, "wake:current");
        drop(store);
        let _ = fs::remove_file(path);
    }

    fn test_record(service_instance_id: &str) -> RuntimeActivityRecord {
        test_record_with_id(service_instance_id, "wake:test")
    }

    fn test_record_with_id(service_instance_id: &str, activity_id: &str) -> RuntimeActivityRecord {
        RuntimeActivityRecord {
            activity_id: RuntimeActivityId::new(activity_id),
            service_instance_id: service_instance_id.into(),
            parent_activity_id: Some(RuntimeActivityId::new("dispatch:test")),
            kind: RuntimeActivityKind::Wake,
            owner: rusty_crew_core_protocol::RuntimeActivityOwner::RustBrain,
            status: RuntimeActivityStatus::Active,
            agent_id: None,
            profile_id: None,
            session_id: None,
            wake_id: Some("test".into()),
            phase: "running".into(),
            summary: None,
            provider_alias: None,
            model_config_id: None,
            endpoint_id: None,
            model: None,
            tool_name: None,
            process_id: None,
            debug_detail_id: None,
            reason_code: None,
            started_at: "2026-07-22T00:00:00Z".into(),
            last_progress_at: "2026-07-22T00:00:00Z".into(),
            terminal_at: None,
            revision: 1,
        }
    }

    fn temp_db_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "rusty-crew-runtime-activities-{}-{}.sqlite3",
            std::process::id(),
            time::OffsetDateTime::now_utc().unix_timestamp_nanos()
        ))
    }
}
