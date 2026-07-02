use super::super::*;

impl CoordinationStore {
    pub fn query_completion_packets(
        &self,
        query: &CompletionPacketQuery,
    ) -> CoreResult<Vec<CompletionPacketRecord>> {
        let conn = self.conn()?;
        query_completion_packets(&conn, query)
    }

    pub(crate) fn save_completion_packet_in_tx(
        &self,
        tx: &rusqlite::Transaction<'_>,
        sequence: u64,
        packet: &CompletionPacket,
    ) -> CoreResult<()> {
        let packet_json = to_json_text(packet)?;
        let status_json = to_json_text(&packet.status)?;
        tx.execute(
            "INSERT OR REPLACE INTO completion_packets (
                sequence,
                session_id,
                status,
                summary,
                packet_json
            ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                sequence as i64,
                packet.session_id.0,
                status_json,
                packet.summary,
                packet_json,
            ],
        )
        .map_err(|error| persistence_error("save completion packet", error))?;
        Ok(())
    }

    pub fn save_worker_run_requested(&self, record: &WorkerRunRecord) -> CoreResult<()> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT OR REPLACE INTO worker_runs (
                run_id,
                session_id,
                delegated_session_id,
                parent_agent_id,
                profile_id,
                task_id,
                status,
                created_at,
                last_updated_at,
                source_wake_id,
                source_action_index,
                delegation_correlation_id,
                parent_consumption,
                fan_out_group_id,
                fan_out_max_concurrency,
                fan_out_failure_policy,
                worker_pool_work_item_id,
                worker_pool_lease_id,
                worker_pool_member_id,
                worker_pool_claim_token
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
            params![
                record.run_id.0.as_str(),
                record.parent_session_id.0.as_str(),
                record
                    .delegated_session_id
                    .as_ref()
                    .map(|session_id| session_id.0.as_str()),
                record
                    .parent_agent_id
                    .as_ref()
                    .map(|agent_id| agent_id.0.as_str()),
                record.profile_id.0.as_str(),
                record.task_id.as_ref().map(|task_id| task_id.0.as_str()),
                record.status.as_str(),
                record.created_at.as_str(),
                record.last_updated_at.as_str(),
                record.source_wake_id.as_str(),
                record.source_action_index as i64,
                record.delegation_correlation_id.as_deref(),
                parent_consumption_policy_as_str(&record.parent_consumption),
                record.fan_out_group_id.as_deref(),
                record.fan_out_max_concurrency.map(|value| value as i64),
                fan_out_failure_policy_as_str(&record.fan_out_failure_policy),
                record.worker_pool_work_item_id.as_deref(),
                record.worker_pool_lease_id.as_deref(),
                record.worker_pool_member_id.as_deref(),
                record.worker_pool_claim_token.as_deref(),
            ],
        )
        .map_err(|error| persistence_error("save worker run", error))?;
        Ok(())
    }

    pub fn load_worker_run(&self, run_id: &RunId) -> CoreResult<Option<WorkerRunRecord>> {
        let conn = self.conn()?;
        let sql = WORKER_RUN_SELECT.to_string() + " WHERE run_id = ?1";
        conn.query_row(&sql, params![run_id.0.as_str()], row_to_worker_run)
            .optional()
            .map_err(|error| persistence_error("load worker run", error))
    }

    pub fn load_worker_run_by_delegated_session(
        &self,
        delegated_session_id: &SessionId,
    ) -> CoreResult<Option<WorkerRunRecord>> {
        let conn = self.conn()?;
        let sql = WORKER_RUN_SELECT.to_string() + " WHERE delegated_session_id = ?1";
        conn.query_row(
            &sql,
            params![delegated_session_id.0.as_str()],
            row_to_worker_run,
        )
        .optional()
        .map_err(|error| persistence_error("load worker run by delegated session", error))
    }

    pub fn query_worker_runs(&self, query: &WorkerRunQuery) -> CoreResult<Vec<WorkerRunRecord>> {
        let conn = self.conn()?;
        query_worker_runs(&conn, query)
    }

    pub fn update_worker_run_status_by_delegated_session(
        &self,
        delegated_session_id: &SessionId,
        status: WorkerRunStatus,
        now: IsoTimestamp,
    ) -> CoreResult<()> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE worker_runs
             SET status = ?1, last_updated_at = ?2
             WHERE delegated_session_id = ?3",
            params![
                status.as_str(),
                now.as_str(),
                delegated_session_id.0.as_str()
            ],
        )
        .map_err(|error| persistence_error("update worker run status", error))?;
        Ok(())
    }

    pub fn update_worker_run_status(
        &self,
        run_id: &RunId,
        status: WorkerRunStatus,
        now: IsoTimestamp,
    ) -> CoreResult<()> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE worker_runs
             SET status = ?1, last_updated_at = ?2
             WHERE run_id = ?3",
            params![status.as_str(), now.as_str(), run_id.0.as_str()],
        )
        .map_err(|error| persistence_error("update worker run status by run id", error))?;
        Ok(())
    }

    pub fn delegated_completions_for_parent(
        &self,
        parent_session_id: &SessionId,
    ) -> CoreResult<Vec<DelegatedCompletion>> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT
                    worker_runs.run_id,
                    worker_runs.delegated_session_id,
                    worker_runs.task_id,
                    worker_runs.source_wake_id,
                    worker_runs.source_action_index,
                    worker_runs.delegation_correlation_id,
                    worker_runs.parent_consumption,
                    completion_packets.packet_json
                 FROM worker_runs
                 JOIN completion_packets
                    ON completion_packets.session_id = worker_runs.delegated_session_id
                 WHERE worker_runs.session_id = ?1
                 ORDER BY completion_packets.sequence ASC",
            )
            .map_err(|error| persistence_error("prepare delegated completions", error))?;

        let rows = stmt
            .query_map(params![parent_session_id.0.as_str()], |row| {
                let parent_consumption: String = row.get(6)?;
                let packet_json: String = row.get(7)?;
                let packet =
                    from_json_text::<CompletionPacket>(&packet_json).map_err(to_sql_error)?;
                Ok(DelegatedCompletion {
                    run_id: RunId(row.get(0)?),
                    child_session_id: SessionId(row.get(1)?),
                    requested_task_id: row.get::<_, Option<String>>(2)?.map(TaskId),
                    source_wake_id: row.get(3)?,
                    source_action_index: row.get::<_, i64>(4)? as u32,
                    correlation_id: row.get(5)?,
                    parent_consumption: parent_consumption_policy_from_str(&parent_consumption)?,
                    packet,
                })
            })
            .map_err(|error| persistence_error("query delegated completions", error))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| persistence_error("load delegated completions", error))
    }

    pub fn worker_runs_for_fan_out_group(
        &self,
        parent_session_id: &SessionId,
        group_id: &str,
    ) -> CoreResult<Vec<WorkerRunRecord>> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(
                &(WORKER_RUN_SELECT.to_string()
                    + " WHERE session_id = ?1 AND fan_out_group_id = ?2
                        ORDER BY source_wake_id ASC, source_action_index ASC"),
            )
            .map_err(|error| persistence_error("prepare worker runs for fan-out group", error))?;

        let rows = stmt
            .query_map(
                params![parent_session_id.0.as_str(), group_id],
                row_to_worker_run,
            )
            .map_err(|error| persistence_error("query worker runs for fan-out group", error))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| persistence_error("load worker runs for fan-out group", error))
    }

    pub fn fan_out_groups_for_parent(
        &self,
        parent_session_id: &SessionId,
    ) -> CoreResult<Vec<DelegatedFanOutGroup>> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(
                &(WORKER_RUN_SELECT.to_string()
                    + " WHERE session_id = ?1 AND fan_out_group_id IS NOT NULL
                        ORDER BY fan_out_group_id ASC, source_wake_id ASC, source_action_index ASC"),
            )
            .map_err(|error| persistence_error("prepare fan-out groups", error))?;

        let rows = stmt
            .query_map(params![parent_session_id.0.as_str()], row_to_worker_run)
            .map_err(|error| persistence_error("query fan-out groups", error))?;
        let runs = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| persistence_error("load fan-out group runs", error))?;

        Ok(aggregate_fan_out_groups(runs))
    }
}

fn query_completion_packets(
    conn: &Connection,
    query: &CompletionPacketQuery,
) -> CoreResult<Vec<CompletionPacketRecord>> {
    let session_id = query.session_id.as_ref().map(|value| value.0.as_str());
    let status_json = query.status.as_ref().map(to_json_text).transpose()?;
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let mut stmt = conn
        .prepare(
            "SELECT sequence, packet_json
             FROM completion_packets
             WHERE (?1 IS NULL OR session_id = ?1)
               AND (?2 IS NULL OR status = ?2)
             ORDER BY sequence ASC
             LIMIT ?3 OFFSET ?4",
        )
        .map_err(|error| persistence_error("prepare query completion packets", error))?;
    let rows = stmt
        .query_map(params![session_id, status_json, limit, offset], |row| {
            let packet_json: String = row.get(1)?;
            Ok(CompletionPacketRecord {
                sequence: row.get::<_, i64>(0)? as u64,
                packet: from_json_text(&packet_json).map_err(to_sql_error)?,
            })
        })
        .map_err(|error| persistence_error("query completion packets", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load queried completion packets", error))
}

fn query_worker_runs(
    conn: &Connection,
    query: &WorkerRunQuery,
) -> CoreResult<Vec<WorkerRunRecord>> {
    let parent_session_id = query
        .parent_session_id
        .as_ref()
        .map(|value| value.0.as_str());
    let delegated_session_id = query
        .delegated_session_id
        .as_ref()
        .map(|value| value.0.as_str());
    let profile_id = query.profile_id.as_ref().map(|value| value.0.as_str());
    let task_id = query.task_id.as_ref().map(|value| value.0.as_str());
    let status = query.status.as_ref().map(WorkerRunStatus::as_str);
    let terminal = query
        .terminal
        .map(|value| if value { 1_i64 } else { 0_i64 });
    let (limit, offset) = query
        .page
        .unwrap_or(QueryPage {
            limit: None,
            offset: None,
        })
        .bounded(100, 1_000);
    let mut stmt = conn
        .prepare(
            &(WORKER_RUN_SELECT.to_string()
                + " WHERE (?1 IS NULL OR session_id = ?1)
               AND (?2 IS NULL OR delegated_session_id = ?2)
               AND (?3 IS NULL OR profile_id = ?3)
               AND (?4 IS NULL OR task_id = ?4)
               AND (?5 IS NULL OR status = ?5)
               AND (
                   ?6 IS NULL
                   OR (?6 = 1 AND status IN ('completed', 'failed', 'blocked', 'exhausted', 'cancelled', 'expired'))
                   OR (?6 = 0 AND status NOT IN ('completed', 'failed', 'blocked', 'exhausted', 'cancelled', 'expired'))
               )
             ORDER BY created_at ASC, run_id ASC
             LIMIT ?7 OFFSET ?8"),
        )
        .map_err(|error| persistence_error("prepare query worker runs", error))?;
    let rows = stmt
        .query_map(
            params![
                parent_session_id,
                delegated_session_id,
                profile_id,
                task_id,
                status,
                terminal,
                limit,
                offset,
            ],
            row_to_worker_run,
        )
        .map_err(|error| persistence_error("query worker runs", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load queried worker runs", error))
}

fn row_to_worker_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkerRunRecord> {
    let status: String = row.get(6)?;
    let fan_out_failure_policy: String = row.get(15)?;
    Ok(WorkerRunRecord {
        run_id: RunId(row.get(0)?),
        parent_session_id: SessionId(row.get(1)?),
        delegated_session_id: row.get::<_, Option<String>>(2)?.map(SessionId),
        parent_agent_id: row.get::<_, Option<String>>(3)?.map(AgentId),
        profile_id: ProfileId(row.get(4)?),
        task_id: row.get::<_, Option<String>>(5)?.map(TaskId),
        status: worker_run_status_from_str(&status)?,
        created_at: row.get(7)?,
        last_updated_at: row.get(8)?,
        source_wake_id: row.get(9)?,
        source_action_index: row.get::<_, i64>(10)? as u32,
        delegation_correlation_id: row.get(11)?,
        parent_consumption: parent_consumption_policy_from_str(&row.get::<_, String>(12)?)?,
        fan_out_group_id: row.get(13)?,
        fan_out_max_concurrency: row.get::<_, Option<i64>>(14)?.map(|value| value as u32),
        fan_out_failure_policy: fan_out_failure_policy_from_str(&fan_out_failure_policy)?,
        worker_pool_work_item_id: row.get(16)?,
        worker_pool_lease_id: row.get(17)?,
        worker_pool_member_id: row.get(18)?,
        worker_pool_claim_token: row.get(19)?,
    })
}

fn worker_run_status_from_str(raw: &str) -> rusqlite::Result<WorkerRunStatus> {
    match raw {
        "requested" => Ok(WorkerRunStatus::Requested),
        "session_created" => Ok(WorkerRunStatus::SessionCreated),
        "wake_requested" => Ok(WorkerRunStatus::WakeRequested),
        "running" => Ok(WorkerRunStatus::Running),
        "checkpoint_waiting" => Ok(WorkerRunStatus::CheckpointWaiting),
        "completed" => Ok(WorkerRunStatus::Completed),
        "failed" => Ok(WorkerRunStatus::Failed),
        "blocked" => Ok(WorkerRunStatus::Blocked),
        "exhausted" => Ok(WorkerRunStatus::Exhausted),
        "cancelled" => Ok(WorkerRunStatus::Cancelled),
        "expired" => Ok(WorkerRunStatus::Expired),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            6,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown worker run status {other}"),
            )),
        )),
    }
}

fn fan_out_failure_policy_as_str(policy: &FanOutFailurePolicy) -> &'static str {
    match policy {
        FanOutFailurePolicy::FailFast => "fail_fast",
        FanOutFailurePolicy::FailSoft => "fail_soft",
    }
}

fn fan_out_failure_policy_from_str(raw: &str) -> rusqlite::Result<FanOutFailurePolicy> {
    match raw {
        "fail_fast" => Ok(FanOutFailurePolicy::FailFast),
        "fail_soft" => Ok(FanOutFailurePolicy::FailSoft),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            15,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown fan-out failure policy {other}"),
            )),
        )),
    }
}

pub(crate) fn aggregate_fan_out_groups(
    mut runs: Vec<WorkerRunRecord>,
) -> Vec<DelegatedFanOutGroup> {
    runs.sort_by(|left, right| {
        left.fan_out_group_id
            .cmp(&right.fan_out_group_id)
            .then_with(|| left.source_wake_id.cmp(&right.source_wake_id))
            .then_with(|| left.source_action_index.cmp(&right.source_action_index))
    });

    let mut groups = Vec::new();
    let mut index = 0;
    while index < runs.len() {
        let Some(group_id) = runs[index].fan_out_group_id.clone() else {
            index += 1;
            continue;
        };
        let mut group_runs = Vec::new();
        while index < runs.len() && runs[index].fan_out_group_id.as_deref() == Some(&group_id) {
            group_runs.push(runs[index].clone());
            index += 1;
        }
        groups.push(aggregate_fan_out_group(group_id, &group_runs));
    }
    groups
}

fn aggregate_fan_out_group(group_id: String, runs: &[WorkerRunRecord]) -> DelegatedFanOutGroup {
    let mut group = DelegatedFanOutGroup {
        group_id,
        total: runs.len() as u32,
        pending: 0,
        completed: 0,
        failed: 0,
        blocked: 0,
        exhausted: 0,
        cancelled: 0,
        expired: 0,
        max_concurrency: runs.iter().find_map(|run| run.fan_out_max_concurrency),
        failure_policy: runs
            .iter()
            .find(|run| run.fan_out_failure_policy == FanOutFailurePolicy::FailFast)
            .map(|run| run.fan_out_failure_policy.clone())
            .unwrap_or(FanOutFailurePolicy::FailSoft),
        status: FanOutGroupStatus::InProgress,
    };

    for run in runs {
        match run.status {
            WorkerRunStatus::Requested
            | WorkerRunStatus::SessionCreated
            | WorkerRunStatus::WakeRequested
            | WorkerRunStatus::Running
            | WorkerRunStatus::CheckpointWaiting => group.pending += 1,
            WorkerRunStatus::Completed => group.completed += 1,
            WorkerRunStatus::Failed => group.failed += 1,
            WorkerRunStatus::Blocked => group.blocked += 1,
            WorkerRunStatus::Exhausted => group.exhausted += 1,
            WorkerRunStatus::Cancelled => group.cancelled += 1,
            WorkerRunStatus::Expired => group.expired += 1,
        }
    }

    let non_success =
        group.failed + group.blocked + group.exhausted + group.cancelled + group.expired;
    group.status = if group.pending > 0 {
        if group.failure_policy == FanOutFailurePolicy::FailFast && non_success > 0 {
            FanOutGroupStatus::FailedFast
        } else {
            FanOutGroupStatus::InProgress
        }
    } else if non_success == 0 {
        FanOutGroupStatus::Completed
    } else if group.failure_policy == FanOutFailurePolicy::FailFast {
        FanOutGroupStatus::FailedFast
    } else {
        FanOutGroupStatus::PartialFailure
    };

    group
}

const WORKER_RUN_SELECT: &str = "SELECT
    run_id,
    session_id,
    delegated_session_id,
    parent_agent_id,
    profile_id,
    task_id,
    status,
    created_at,
    last_updated_at,
    source_wake_id,
    source_action_index,
    delegation_correlation_id,
    parent_consumption,
    fan_out_group_id,
    fan_out_max_concurrency,
    fan_out_failure_policy,
    worker_pool_work_item_id,
    worker_pool_lease_id,
    worker_pool_member_id,
    worker_pool_claim_token
 FROM worker_runs";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_run_repo_aggregates_fan_out_status() {
        let runs = vec![
            worker_run(
                "run-1",
                WorkerRunStatus::Completed,
                FanOutFailurePolicy::FailSoft,
            ),
            worker_run(
                "run-2",
                WorkerRunStatus::Failed,
                FanOutFailurePolicy::FailSoft,
            ),
        ];
        let groups = aggregate_fan_out_groups(runs);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].total, 2);
        assert_eq!(groups[0].completed, 1);
        assert_eq!(groups[0].failed, 1);
        assert_eq!(groups[0].status, FanOutGroupStatus::PartialFailure);
    }

    fn worker_run(
        run_id: &str,
        status: WorkerRunStatus,
        failure_policy: FanOutFailurePolicy,
    ) -> WorkerRunRecord {
        WorkerRunRecord {
            run_id: RunId::new(run_id),
            parent_session_id: SessionId::new("parent"),
            delegated_session_id: Some(SessionId::new(format!("{run_id}-child"))),
            parent_agent_id: Some(AgentId::new("parent-agent")),
            profile_id: ProfileId::new("worker-profile"),
            task_id: None,
            status,
            created_at: "2026-07-02T00:00:00Z".to_string(),
            last_updated_at: "2026-07-02T00:00:00Z".to_string(),
            source_wake_id: "wake".to_string(),
            source_action_index: if run_id == "run-1" { 0 } else { 1 },
            delegation_correlation_id: Some("corr".to_string()),
            parent_consumption: ParentConsumptionPolicy::AwaitCompletion,
            fan_out_group_id: Some("group-alpha".to_string()),
            fan_out_max_concurrency: Some(2),
            fan_out_failure_policy: failure_policy,
            worker_pool_work_item_id: None,
            worker_pool_lease_id: None,
            worker_pool_member_id: None,
            worker_pool_claim_token: None,
        }
    }
}
