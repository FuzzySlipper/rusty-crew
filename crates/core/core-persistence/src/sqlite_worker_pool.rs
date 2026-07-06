//! SQLite worker-pool repository domain.
//!
//! Worker-pool persistence owns claim fencing, lease lifecycle, terminal
//! completion, and stale-claim expiry. Keeping these operations together makes
//! the no-resurrection invariants easier to audit.

use super::*;

impl CoordinationStore {
    pub fn upsert_worker_pool_member(&self, record: &WorkerPoolMemberRecord) -> CoreResult<()> {
        let capabilities_json = to_json_text(&record.capabilities_json)?;
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO worker_pool_members (
                member_id,
                profile_id,
                agent_id,
                session_id,
                status,
                concurrency_limit,
                active_leases,
                capabilities_json,
                registered_at,
                last_heartbeat_at,
                updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ON CONFLICT(member_id) DO UPDATE SET
                profile_id = excluded.profile_id,
                agent_id = excluded.agent_id,
                session_id = excluded.session_id,
                status = excluded.status,
                concurrency_limit = excluded.concurrency_limit,
                active_leases = excluded.active_leases,
                capabilities_json = excluded.capabilities_json,
                last_heartbeat_at = excluded.last_heartbeat_at,
                updated_at = excluded.updated_at",
            params![
                record.member_id.as_str(),
                record.profile_id.0.as_str(),
                record.agent_id.as_ref().map(|agent_id| agent_id.0.as_str()),
                record
                    .session_id
                    .as_ref()
                    .map(|session_id| session_id.0.as_str()),
                record.status.as_str(),
                record.concurrency_limit as i64,
                record.active_leases as i64,
                capabilities_json,
                record.registered_at.as_str(),
                record.last_heartbeat_at.as_str(),
                record.updated_at.as_str(),
            ],
        )
        .map_err(|error| persistence_error("upsert worker pool member", error))?;
        Ok(())
    }

    pub fn heartbeat_worker_pool_member(
        &self,
        member_id: &str,
        status: WorkerPoolMemberStatus,
        now: &IsoTimestamp,
    ) -> CoreResult<bool> {
        let conn = self.conn()?;
        let rows = conn
            .execute(
                "UPDATE worker_pool_members
                 SET status = ?1, last_heartbeat_at = ?2, updated_at = ?2
                 WHERE member_id = ?3",
                params![status.as_str(), now.as_str(), member_id],
            )
            .map_err(|error| persistence_error("heartbeat worker pool member", error))?;
        Ok(rows > 0)
    }

    pub fn load_worker_pool_member(
        &self,
        member_id: &str,
    ) -> CoreResult<Option<WorkerPoolMemberRecord>> {
        let conn = self.conn()?;
        load_worker_pool_member_from_conn(&conn, member_id)
    }

    pub fn create_worker_pool_work_item(
        &self,
        record: &WorkerPoolWorkItemRecord,
    ) -> CoreResult<()> {
        let work_json = to_json_text(&record.work_json)?;
        let required_capabilities_json = to_json_text(&record.required_capabilities_json)?;
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO worker_pool_work_items (
                work_item_id,
                requested_profile_id,
                task_id,
                status,
                priority,
                work_json,
                required_capabilities_json,
                created_at,
                updated_at,
                claimed_by_member_id,
                lease_id,
                claim_token,
                claim_deadline_at,
                terminal_at,
                terminal_summary
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                record.work_item_id.as_str(),
                record
                    .requested_profile_id
                    .as_ref()
                    .map(|profile_id| profile_id.0.as_str()),
                record.task_id.as_ref().map(|task_id| task_id.0.as_str()),
                record.status.as_str(),
                record.priority as i64,
                work_json,
                required_capabilities_json,
                record.created_at.as_str(),
                record.updated_at.as_str(),
                record.claimed_by_member_id.as_deref(),
                record.lease_id.as_deref(),
                record.claim_token.as_deref(),
                record.claim_deadline_at.as_deref(),
                record.terminal_at.as_deref(),
                record.terminal_summary.as_deref(),
            ],
        )
        .map_err(|error| persistence_error("create worker pool work item", error))?;
        Ok(())
    }

    pub fn load_worker_pool_work_item(
        &self,
        work_item_id: &str,
    ) -> CoreResult<Option<WorkerPoolWorkItemRecord>> {
        let conn = self.conn()?;
        load_worker_pool_work_item_from_conn(&conn, work_item_id)
    }

    pub fn claim_next_worker_pool_work_item(
        &self,
        request: &WorkerPoolClaimRequest,
    ) -> CoreResult<Result<WorkerPoolClaimRecord, WorkerPoolNoCapacityReason>> {
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start worker pool claim transaction", error))?;

        let Some(member) = load_worker_pool_member_from_tx(&tx, &request.member_id)? else {
            return Ok(Err(WorkerPoolNoCapacityReason::MemberUnavailable));
        };
        if !member.status.can_claim() {
            return Ok(Err(WorkerPoolNoCapacityReason::MemberUnavailable));
        }
        if member.last_heartbeat_at < request.min_heartbeat_at {
            return Ok(Err(WorkerPoolNoCapacityReason::MemberHeartbeatStale));
        }
        if member.active_leases >= member.concurrency_limit {
            return Ok(Err(WorkerPoolNoCapacityReason::MemberAtCapacity));
        }

        let Some(mut work_item) = find_next_worker_pool_work_item_for_claim(&tx, &member)? else {
            return Ok(Err(WorkerPoolNoCapacityReason::NoPendingWork));
        };

        let rows = tx
            .execute(
                "UPDATE worker_pool_work_items
                 SET status = ?1,
                     claimed_by_member_id = ?2,
                     lease_id = ?3,
                     claim_token = ?4,
                     claim_deadline_at = ?5,
                     updated_at = ?6
                 WHERE work_item_id = ?7 AND status = ?8",
                params![
                    WorkerPoolWorkStatus::Claimed.as_str(),
                    member.member_id.as_str(),
                    request.lease_id.as_str(),
                    request.claim_token.as_str(),
                    request.claim_deadline_at.as_str(),
                    request.now.as_str(),
                    work_item.work_item_id.as_str(),
                    WorkerPoolWorkStatus::Pending.as_str(),
                ],
            )
            .map_err(|error| persistence_error("claim worker pool work item", error))?;
        if rows != 1 {
            return Ok(Err(WorkerPoolNoCapacityReason::NoPendingWork));
        }

        let lease = WorkerPoolLeaseRecord {
            lease_id: request.lease_id.clone(),
            work_item_id: work_item.work_item_id.clone(),
            member_id: member.member_id.clone(),
            claim_token: request.claim_token.clone(),
            status: WorkerPoolLeaseStatus::Active,
            claimed_at: request.now.clone(),
            claim_deadline_at: request.claim_deadline_at.clone(),
            terminal_at: None,
        };
        insert_worker_pool_lease(&tx, &lease)?;

        let next_active_leases = member.active_leases.saturating_add(1);
        let next_member_status = if next_active_leases >= member.concurrency_limit {
            WorkerPoolMemberStatus::Busy
        } else {
            member.status
        };
        tx.execute(
            "UPDATE worker_pool_members
             SET active_leases = ?1, status = ?2, updated_at = ?3
             WHERE member_id = ?4",
            params![
                next_active_leases as i64,
                next_member_status.as_str(),
                request.now.as_str(),
                member.member_id.as_str(),
            ],
        )
        .map_err(|error| persistence_error("update worker pool member claim count", error))?;
        insert_worker_pool_event(
            &tx,
            &work_item.work_item_id,
            Some(&lease.lease_id),
            Some(&member.member_id),
            "claimed",
            &serde_json::json!({
                "claim_deadline_at": request.claim_deadline_at,
                "profile_id": member.profile_id.0,
            }),
            &request.now,
        )?;
        tx.commit()
            .map_err(|error| persistence_error("commit worker pool claim", error))?;

        work_item.status = WorkerPoolWorkStatus::Claimed;
        work_item.updated_at = request.now.clone();
        work_item.claimed_by_member_id = Some(member.member_id.clone());
        work_item.lease_id = Some(lease.lease_id.clone());
        work_item.claim_token = Some(lease.claim_token.clone());
        work_item.claim_deadline_at = Some(lease.claim_deadline_at.clone());
        let mut updated_member = member;
        updated_member.active_leases = next_active_leases;
        updated_member.status = next_member_status;
        updated_member.updated_at = request.now.clone();

        Ok(Ok(WorkerPoolClaimRecord {
            member: updated_member,
            work_item,
            lease,
        }))
    }

    pub fn complete_worker_pool_work_item(
        &self,
        request: &WorkerPoolCompletionRequest,
    ) -> CoreResult<bool> {
        if !request.status.is_terminal() || request.status == WorkerPoolWorkStatus::Pending {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "worker pool completion must use a terminal work status",
            ));
        }

        let mut conn = self.conn()?;
        let tx = conn.transaction().map_err(|error| {
            persistence_error("start worker pool completion transaction", error)
        })?;
        let Some(lease) = load_worker_pool_lease_from_tx(&tx, &request.lease_id)? else {
            return Ok(false);
        };
        if lease.status != WorkerPoolLeaseStatus::Active || lease.claim_token != request.claim_token
        {
            return Ok(false);
        }
        let Some(work_item) = load_worker_pool_work_item_from_tx(&tx, &lease.work_item_id)? else {
            return Ok(false);
        };
        if work_item.status.is_terminal() {
            return Ok(false);
        }

        let lease_status = worker_pool_lease_status_for_work_status(request.status)?;
        let rows = tx
            .execute(
                "UPDATE worker_pool_work_items
                 SET status = ?1,
                     updated_at = ?2,
                     terminal_at = ?2,
                     terminal_summary = ?3
                 WHERE work_item_id = ?4
                   AND lease_id = ?5
                   AND claim_token = ?6
                   AND status IN (?7, ?8)",
                params![
                    request.status.as_str(),
                    request.now.as_str(),
                    request.summary.as_deref(),
                    work_item.work_item_id.as_str(),
                    lease.lease_id.as_str(),
                    request.claim_token.as_str(),
                    WorkerPoolWorkStatus::Claimed.as_str(),
                    WorkerPoolWorkStatus::Running.as_str(),
                ],
            )
            .map_err(|error| persistence_error("complete worker pool work item", error))?;
        if rows != 1 {
            return Ok(false);
        }

        tx.execute(
            "UPDATE worker_pool_leases
             SET status = ?1, terminal_at = ?2
             WHERE lease_id = ?3 AND status = ?4",
            params![
                lease_status.as_str(),
                request.now.as_str(),
                lease.lease_id.as_str(),
                WorkerPoolLeaseStatus::Active.as_str(),
            ],
        )
        .map_err(|error| persistence_error("complete worker pool lease", error))?;
        release_worker_pool_member_lease(&tx, &lease.member_id, &request.now)?;
        insert_worker_pool_event(
            &tx,
            &work_item.work_item_id,
            Some(&lease.lease_id),
            Some(&lease.member_id),
            request.status.as_str(),
            &serde_json::json!({ "summary": request.summary }),
            &request.now,
        )?;
        tx.commit()
            .map_err(|error| persistence_error("commit worker pool completion", error))?;
        Ok(true)
    }

    pub fn expire_worker_pool_claims(
        &self,
        stale_before: &IsoTimestamp,
        now: &IsoTimestamp,
    ) -> CoreResult<Vec<WorkerPoolWorkItemRecord>> {
        let mut conn = self.conn()?;
        let tx = conn.transaction().map_err(|error| {
            persistence_error("start worker pool claim expiry transaction", error)
        })?;
        let mut stmt = tx
            .prepare(
                "SELECT
                    work_item_id,
                    requested_profile_id,
                    task_id,
                    status,
                    priority,
                    work_json,
                    required_capabilities_json,
                    created_at,
                    updated_at,
                    claimed_by_member_id,
                    lease_id,
                    claim_token,
                    claim_deadline_at,
                    terminal_at,
                    terminal_summary
                 FROM worker_pool_work_items
                 WHERE status IN (?1, ?2)
                   AND claim_deadline_at IS NOT NULL
                   AND claim_deadline_at < ?3
                 ORDER BY claim_deadline_at ASC, work_item_id ASC",
            )
            .map_err(|error| persistence_error("prepare stale worker pool claims", error))?;
        let stale = stmt
            .query_map(
                params![
                    WorkerPoolWorkStatus::Claimed.as_str(),
                    WorkerPoolWorkStatus::Running.as_str(),
                    stale_before.as_str(),
                ],
                row_to_worker_pool_work_item,
            )
            .map_err(|error| persistence_error("query stale worker pool claims", error))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| persistence_error("load stale worker pool claims", error))?;
        drop(stmt);

        let mut expired = Vec::new();
        for mut item in stale {
            let rows = tx
                .execute(
                    "UPDATE worker_pool_work_items
                     SET status = ?1,
                         updated_at = ?2,
                         terminal_at = ?2,
                         terminal_summary = ?3
                     WHERE work_item_id = ?4
                       AND status IN (?5, ?6)",
                    params![
                        WorkerPoolWorkStatus::Expired.as_str(),
                        now.as_str(),
                        "worker pool claim expired",
                        item.work_item_id.as_str(),
                        WorkerPoolWorkStatus::Claimed.as_str(),
                        WorkerPoolWorkStatus::Running.as_str(),
                    ],
                )
                .map_err(|error| persistence_error("expire worker pool work item", error))?;
            if rows != 1 {
                continue;
            }
            if let Some(lease_id) = item.lease_id.as_deref() {
                tx.execute(
                    "UPDATE worker_pool_leases
                     SET status = ?1, terminal_at = ?2
                     WHERE lease_id = ?3 AND status = ?4",
                    params![
                        WorkerPoolLeaseStatus::Expired.as_str(),
                        now.as_str(),
                        lease_id,
                        WorkerPoolLeaseStatus::Active.as_str(),
                    ],
                )
                .map_err(|error| persistence_error("expire worker pool lease", error))?;
            }
            if let Some(member_id) = item.claimed_by_member_id.as_deref() {
                release_worker_pool_member_lease(&tx, member_id, now)?;
            }
            insert_worker_pool_event(
                &tx,
                &item.work_item_id,
                item.lease_id.as_deref(),
                item.claimed_by_member_id.as_deref(),
                WorkerPoolWorkStatus::Expired.as_str(),
                &serde_json::json!({ "reason": "claim_deadline_expired" }),
                now,
            )?;
            item.status = WorkerPoolWorkStatus::Expired;
            item.updated_at = now.clone();
            item.terminal_at = Some(now.clone());
            item.terminal_summary = Some("worker pool claim expired".to_string());
            expired.push(item);
        }

        tx.commit()
            .map_err(|error| persistence_error("commit worker pool expiry", error))?;
        Ok(expired)
    }
}

fn row_to_worker_pool_member(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkerPoolMemberRecord> {
    let status: String = row.get(4)?;
    let capabilities_json: String = row.get(7)?;
    Ok(WorkerPoolMemberRecord {
        member_id: row.get(0)?,
        profile_id: ProfileId(row.get(1)?),
        agent_id: row.get::<_, Option<String>>(2)?.map(AgentId),
        session_id: row.get::<_, Option<String>>(3)?.map(SessionId),
        status: worker_pool_member_status_from_str(&status)?,
        concurrency_limit: row.get::<_, i64>(5)? as u32,
        active_leases: row.get::<_, i64>(6)? as u32,
        capabilities_json: parse_json_record(&capabilities_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                7,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        registered_at: row.get(8)?,
        last_heartbeat_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn row_to_worker_pool_work_item(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<WorkerPoolWorkItemRecord> {
    let status: String = row.get(3)?;
    let work_json: String = row.get(5)?;
    let required_capabilities_json: String = row.get(6)?;
    Ok(WorkerPoolWorkItemRecord {
        work_item_id: row.get(0)?,
        requested_profile_id: row.get::<_, Option<String>>(1)?.map(ProfileId),
        task_id: row.get::<_, Option<String>>(2)?.map(TaskId),
        status: worker_pool_work_status_from_str(&status)?,
        priority: row.get::<_, i64>(4)? as i32,
        work_json: parse_json_record(&work_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                5,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        required_capabilities_json: parse_json_record(&required_capabilities_json).map_err(
            |error| {
                rusqlite::Error::FromSqlConversionFailure(
                    6,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            },
        )?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        claimed_by_member_id: row.get(9)?,
        lease_id: row.get(10)?,
        claim_token: row.get(11)?,
        claim_deadline_at: row.get(12)?,
        terminal_at: row.get(13)?,
        terminal_summary: row.get(14)?,
    })
}

fn row_to_worker_pool_lease(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkerPoolLeaseRecord> {
    let status: String = row.get(4)?;
    Ok(WorkerPoolLeaseRecord {
        lease_id: row.get(0)?,
        work_item_id: row.get(1)?,
        member_id: row.get(2)?,
        claim_token: row.get(3)?,
        status: worker_pool_lease_status_from_str(&status)?,
        claimed_at: row.get(5)?,
        claim_deadline_at: row.get(6)?,
        terminal_at: row.get(7)?,
    })
}

fn worker_pool_member_status_from_str(raw: &str) -> rusqlite::Result<WorkerPoolMemberStatus> {
    match raw {
        "available" => Ok(WorkerPoolMemberStatus::Available),
        "busy" => Ok(WorkerPoolMemberStatus::Busy),
        "offline" => Ok(WorkerPoolMemberStatus::Offline),
        "quarantined" => Ok(WorkerPoolMemberStatus::Quarantined),
        "retired" => Ok(WorkerPoolMemberStatus::Retired),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            4,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown worker pool member status {other}"),
            )),
        )),
    }
}

fn worker_pool_work_status_from_str(raw: &str) -> rusqlite::Result<WorkerPoolWorkStatus> {
    match raw {
        "pending" => Ok(WorkerPoolWorkStatus::Pending),
        "claimed" => Ok(WorkerPoolWorkStatus::Claimed),
        "running" => Ok(WorkerPoolWorkStatus::Running),
        "completed" => Ok(WorkerPoolWorkStatus::Completed),
        "failed" => Ok(WorkerPoolWorkStatus::Failed),
        "blocked" => Ok(WorkerPoolWorkStatus::Blocked),
        "exhausted" => Ok(WorkerPoolWorkStatus::Exhausted),
        "cancelled" => Ok(WorkerPoolWorkStatus::Cancelled),
        "expired" => Ok(WorkerPoolWorkStatus::Expired),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            3,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown worker pool work status {other}"),
            )),
        )),
    }
}

fn worker_pool_lease_status_from_str(raw: &str) -> rusqlite::Result<WorkerPoolLeaseStatus> {
    match raw {
        "active" => Ok(WorkerPoolLeaseStatus::Active),
        "completed" => Ok(WorkerPoolLeaseStatus::Completed),
        "failed" => Ok(WorkerPoolLeaseStatus::Failed),
        "blocked" => Ok(WorkerPoolLeaseStatus::Blocked),
        "exhausted" => Ok(WorkerPoolLeaseStatus::Exhausted),
        "cancelled" => Ok(WorkerPoolLeaseStatus::Cancelled),
        "expired" => Ok(WorkerPoolLeaseStatus::Expired),
        "released" => Ok(WorkerPoolLeaseStatus::Released),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            4,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown worker pool lease status {other}"),
            )),
        )),
    }
}

fn worker_pool_lease_status_for_work_status(
    status: WorkerPoolWorkStatus,
) -> CoreResult<WorkerPoolLeaseStatus> {
    match status {
        WorkerPoolWorkStatus::Completed => Ok(WorkerPoolLeaseStatus::Completed),
        WorkerPoolWorkStatus::Failed => Ok(WorkerPoolLeaseStatus::Failed),
        WorkerPoolWorkStatus::Blocked => Ok(WorkerPoolLeaseStatus::Blocked),
        WorkerPoolWorkStatus::Exhausted => Ok(WorkerPoolLeaseStatus::Exhausted),
        WorkerPoolWorkStatus::Cancelled => Ok(WorkerPoolLeaseStatus::Cancelled),
        WorkerPoolWorkStatus::Expired => Ok(WorkerPoolLeaseStatus::Expired),
        WorkerPoolWorkStatus::Pending
        | WorkerPoolWorkStatus::Claimed
        | WorkerPoolWorkStatus::Running => Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "worker pool lease terminal status requires terminal work status",
        )),
    }
}

fn load_worker_pool_member_from_conn(
    conn: &Connection,
    member_id: &str,
) -> CoreResult<Option<WorkerPoolMemberRecord>> {
    conn.query_row(
        "SELECT
            member_id,
            profile_id,
            agent_id,
            session_id,
            status,
            concurrency_limit,
            active_leases,
            capabilities_json,
            registered_at,
            last_heartbeat_at,
            updated_at
         FROM worker_pool_members
         WHERE member_id = ?1",
        params![member_id],
        row_to_worker_pool_member,
    )
    .optional()
    .map_err(|error| persistence_error("load worker pool member", error))
}

fn load_worker_pool_member_from_tx(
    tx: &rusqlite::Transaction<'_>,
    member_id: &str,
) -> CoreResult<Option<WorkerPoolMemberRecord>> {
    tx.query_row(
        "SELECT
            member_id,
            profile_id,
            agent_id,
            session_id,
            status,
            concurrency_limit,
            active_leases,
            capabilities_json,
            registered_at,
            last_heartbeat_at,
            updated_at
         FROM worker_pool_members
         WHERE member_id = ?1",
        params![member_id],
        row_to_worker_pool_member,
    )
    .optional()
    .map_err(|error| persistence_error("load worker pool member in tx", error))
}

fn load_worker_pool_work_item_from_conn(
    conn: &Connection,
    work_item_id: &str,
) -> CoreResult<Option<WorkerPoolWorkItemRecord>> {
    conn.query_row(
        WORKER_POOL_WORK_ITEM_SELECT_BY_ID,
        params![work_item_id],
        row_to_worker_pool_work_item,
    )
    .optional()
    .map_err(|error| persistence_error("load worker pool work item", error))
}

fn load_worker_pool_work_item_from_tx(
    tx: &rusqlite::Transaction<'_>,
    work_item_id: &str,
) -> CoreResult<Option<WorkerPoolWorkItemRecord>> {
    tx.query_row(
        WORKER_POOL_WORK_ITEM_SELECT_BY_ID,
        params![work_item_id],
        row_to_worker_pool_work_item,
    )
    .optional()
    .map_err(|error| persistence_error("load worker pool work item in tx", error))
}

fn find_next_worker_pool_work_item_for_claim(
    tx: &rusqlite::Transaction<'_>,
    member: &WorkerPoolMemberRecord,
) -> CoreResult<Option<WorkerPoolWorkItemRecord>> {
    tx.query_row(
        "SELECT
            work_item_id,
            requested_profile_id,
            task_id,
            status,
            priority,
            work_json,
            required_capabilities_json,
            created_at,
            updated_at,
            claimed_by_member_id,
            lease_id,
            claim_token,
            claim_deadline_at,
            terminal_at,
            terminal_summary
         FROM worker_pool_work_items
         WHERE status = ?1
           AND (requested_profile_id IS NULL OR requested_profile_id = ?2)
         ORDER BY priority ASC, created_at ASC, work_item_id ASC
         LIMIT 1",
        params![
            WorkerPoolWorkStatus::Pending.as_str(),
            member.profile_id.0.as_str()
        ],
        row_to_worker_pool_work_item,
    )
    .optional()
    .map_err(|error| persistence_error("find next worker pool work item", error))
}

fn load_worker_pool_lease_from_tx(
    tx: &rusqlite::Transaction<'_>,
    lease_id: &str,
) -> CoreResult<Option<WorkerPoolLeaseRecord>> {
    tx.query_row(
        "SELECT
            lease_id,
            work_item_id,
            member_id,
            claim_token,
            status,
            claimed_at,
            claim_deadline_at,
            terminal_at
         FROM worker_pool_leases
         WHERE lease_id = ?1",
        params![lease_id],
        row_to_worker_pool_lease,
    )
    .optional()
    .map_err(|error| persistence_error("load worker pool lease", error))
}

fn insert_worker_pool_lease(
    tx: &rusqlite::Transaction<'_>,
    lease: &WorkerPoolLeaseRecord,
) -> CoreResult<()> {
    tx.execute(
        "INSERT INTO worker_pool_leases (
            lease_id,
            work_item_id,
            member_id,
            claim_token,
            status,
            claimed_at,
            claim_deadline_at,
            terminal_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            lease.lease_id.as_str(),
            lease.work_item_id.as_str(),
            lease.member_id.as_str(),
            lease.claim_token.as_str(),
            lease.status.as_str(),
            lease.claimed_at.as_str(),
            lease.claim_deadline_at.as_str(),
            lease.terminal_at.as_deref(),
        ],
    )
    .map_err(|error| persistence_error("insert worker pool lease", error))?;
    Ok(())
}

fn release_worker_pool_member_lease(
    tx: &rusqlite::Transaction<'_>,
    member_id: &str,
    now: &IsoTimestamp,
) -> CoreResult<()> {
    tx.execute(
        "UPDATE worker_pool_members
         SET active_leases = CASE WHEN active_leases > 0 THEN active_leases - 1 ELSE 0 END,
             status = CASE
                WHEN status IN (?1, ?2) THEN ?1
                ELSE status
             END,
             updated_at = ?3
         WHERE member_id = ?4",
        params![
            WorkerPoolMemberStatus::Available.as_str(),
            WorkerPoolMemberStatus::Busy.as_str(),
            now.as_str(),
            member_id,
        ],
    )
    .map_err(|error| persistence_error("release worker pool member lease", error))?;
    Ok(())
}

fn insert_worker_pool_event(
    tx: &rusqlite::Transaction<'_>,
    work_item_id: &str,
    lease_id: Option<&str>,
    member_id: Option<&str>,
    event_type: &str,
    event_json: &JsonValue,
    recorded_at: &IsoTimestamp,
) -> CoreResult<()> {
    let event_json = to_json_text(event_json)?;
    tx.execute(
        "INSERT INTO worker_pool_events (
            work_item_id,
            lease_id,
            member_id,
            event_type,
            event_json,
            recorded_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            work_item_id,
            lease_id,
            member_id,
            event_type,
            event_json,
            recorded_at.as_str(),
        ],
    )
    .map_err(|error| persistence_error("insert worker pool event", error))?;
    Ok(())
}

const WORKER_POOL_WORK_ITEM_SELECT_BY_ID: &str = "SELECT
    work_item_id,
    requested_profile_id,
    task_id,
    status,
    priority,
    work_json,
    required_capabilities_json,
    created_at,
    updated_at,
    claimed_by_member_id,
    lease_id,
    claim_token,
    claim_deadline_at,
    terminal_at,
    terminal_summary
 FROM worker_pool_work_items
 WHERE work_item_id = ?1";
