//! Local coordination-state persistence.
//!
//! This store is for Rust-owned coordination state only. Den task, project, and
//! document data remains Den product data and is not mirrored here.

use rusqlite::{params, Connection, OptionalExtension};
use rusty_crew_core_protocol::{
    AgentId, BrainEvent, CompletionPacket, CoreError, CoreErrorKind, CoreEvent, CoreEventKind,
    CoreResult, DelegatedCompletion, DelegatedFanOutGroup, DelegationLineage, FanOutFailurePolicy,
    FanOutGroupStatus, IsoTimestamp, ParentConsumptionPolicy, ProfileId, ResourceLimits, RunId,
    SessionHandle, SessionId, SessionKind, SessionState, SessionStatus, TaskId, ToolProfile,
};
use serde::{de::DeserializeOwned, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const DB_FILE_NAME: &str = "coordination.sqlite3";

#[derive(Debug, Clone)]
pub struct CoordinationStore {
    conn: Arc<Mutex<Connection>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedEvent {
    pub sequence: u64,
    pub event: CoreEvent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCallRecord {
    pub sequence: u64,
    pub session_id: SessionId,
    pub wake_id: Option<String>,
    pub tool_name: String,
    pub phase: ToolCallPhase,
    pub is_error: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCallPhase {
    Started,
    Finished,
}

impl ToolCallPhase {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Finished => "finished",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerRunStatus {
    Requested,
    SessionCreated,
    WakeRequested,
    Running,
    CheckpointWaiting,
    Completed,
    Failed,
    Blocked,
    Exhausted,
    Cancelled,
    Expired,
}

impl WorkerRunStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::SessionCreated => "session_created",
            Self::WakeRequested => "wake_requested",
            Self::Running => "running",
            Self::CheckpointWaiting => "checkpoint_waiting",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Blocked => "blocked",
            Self::Exhausted => "exhausted",
            Self::Cancelled => "cancelled",
            Self::Expired => "expired",
        }
    }

    pub const fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed
                | Self::Failed
                | Self::Blocked
                | Self::Exhausted
                | Self::Cancelled
                | Self::Expired
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerRunRecord {
    pub run_id: RunId,
    pub parent_session_id: SessionId,
    pub delegated_session_id: Option<SessionId>,
    pub parent_agent_id: Option<AgentId>,
    pub profile_id: ProfileId,
    pub task_id: Option<TaskId>,
    pub status: WorkerRunStatus,
    pub created_at: IsoTimestamp,
    pub last_updated_at: IsoTimestamp,
    pub source_wake_id: String,
    pub source_action_index: u32,
    pub delegation_correlation_id: Option<String>,
    pub parent_consumption: ParentConsumptionPolicy,
    pub fan_out_group_id: Option<String>,
    pub fan_out_max_concurrency: Option<u32>,
    pub fan_out_failure_policy: FanOutFailurePolicy,
}

impl CoordinationStore {
    pub fn open(engine_data_dir: impl AsRef<Path>) -> CoreResult<Self> {
        fs::create_dir_all(engine_data_dir.as_ref())
            .map_err(|error| persistence_error("create coordination data directory", error))?;
        Self::open_file(engine_data_dir.as_ref().join(DB_FILE_NAME))
    }

    pub fn open_file(path: impl AsRef<Path>) -> CoreResult<Self> {
        let conn = Connection::open(path.as_ref())
            .map_err(|error| persistence_error("open sqlite", error))?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn save_session(&self, state: &SessionState) -> CoreResult<()> {
        let kind_json = to_json_text(&state.kind)?;
        let status_json = to_json_text(&state.status)?;
        let resource_limits_json = to_json_text(&state.resource_limits)?;
        let tool_profile_json = to_json_text(&state.tool_profile)?;
        let delegation_json = state.delegation.as_ref().map(to_json_text).transpose()?;
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO sessions (
                session_id,
                handle,
                agent_id,
                profile_id,
                kind_json,
                delegation_json,
                resource_limits_json,
                tool_profile_json,
                status_json,
                brain_turn_count,
                created_at,
                last_active_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(session_id) DO UPDATE SET
                handle = excluded.handle,
                agent_id = excluded.agent_id,
                profile_id = excluded.profile_id,
                kind_json = excluded.kind_json,
                delegation_json = excluded.delegation_json,
                resource_limits_json = excluded.resource_limits_json,
                tool_profile_json = excluded.tool_profile_json,
                status_json = excluded.status_json,
                brain_turn_count = excluded.brain_turn_count,
                last_active_at = excluded.last_active_at",
            params![
                state.session_id.0,
                state.handle.get() as i64,
                state.agent_id.0,
                state.profile_id.0,
                kind_json,
                delegation_json,
                resource_limits_json,
                tool_profile_json,
                status_json,
                state.brain_turn_count as i64,
                state.created_at,
                state.last_active_at,
            ],
        )
        .map_err(|error| persistence_error("save session", error))?;
        Ok(())
    }

    pub fn load_sessions(&self) -> CoreResult<Vec<SessionState>> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT
                    session_id,
                    handle,
                    agent_id,
                    profile_id,
                    kind_json,
                    delegation_json,
                    resource_limits_json,
                    tool_profile_json,
                    status_json,
                    brain_turn_count,
                    created_at,
                    last_active_at
                FROM sessions
                ORDER BY handle ASC",
            )
            .map_err(|error| persistence_error("prepare load sessions", error))?;

        let rows = stmt
            .query_map([], |row| {
                let kind_json: String = row.get(4)?;
                let delegation_json: Option<String> = row.get(5)?;
                let resource_limits_json: Option<String> = row.get(6)?;
                let tool_profile_json: Option<String> = row.get(7)?;
                let status_json: String = row.get(8)?;
                let kind = from_json_text::<SessionKind>(&kind_json).map_err(to_sql_error)?;
                let delegation = delegation_json
                    .as_deref()
                    .map(from_json_text::<DelegationLineage>)
                    .transpose()
                    .map_err(to_sql_error)?;
                let resource_limits = resource_limits_json
                    .as_deref()
                    .map(from_json_text::<ResourceLimits>)
                    .transpose()
                    .map_err(to_sql_error)?
                    .unwrap_or(ResourceLimits {
                        workdir: None,
                        max_duration_ms: None,
                        max_delegation_depth: None,
                    });
                let tool_profile = tool_profile_json
                    .as_deref()
                    .map(from_json_text::<ToolProfile>)
                    .transpose()
                    .map_err(to_sql_error)?
                    .unwrap_or(ToolProfile { tools: Vec::new() });
                let status = from_json_text::<SessionStatus>(&status_json).map_err(to_sql_error)?;
                Ok(SessionState {
                    session_id: SessionId(row.get(0)?),
                    handle: SessionHandle::new(row.get::<_, i64>(1)? as u64),
                    agent_id: rusty_crew_core_protocol::AgentId(row.get(2)?),
                    profile_id: ProfileId(row.get(3)?),
                    kind,
                    delegation,
                    resource_limits,
                    tool_profile,
                    status,
                    brain_turn_count: row.get::<_, i64>(9)? as u32,
                    created_at: row.get(10)?,
                    last_active_at: row.get(11)?,
                })
            })
            .map_err(|error| persistence_error("query sessions", error))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| persistence_error("load sessions", error))
    }

    pub fn save_event(&self, sequence: u64, event: &CoreEvent) -> CoreResult<()> {
        if !should_persist_event(event) {
            return Ok(());
        }

        let event_kind = format!("{:?}", CoreEventKind::of(event));
        let event_json = to_json_text(event)?;
        let conn = self.conn()?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|error| persistence_error("begin save event", error))?;
        tx.execute(
            "INSERT OR REPLACE INTO event_history (sequence, event_kind, event_json)
             VALUES (?1, ?2, ?3)",
            params![sequence as i64, event_kind, event_json],
        )
        .map_err(|error| persistence_error("save event history", error))?;

        match event {
            CoreEvent::AgentMessageRouted { message } => {
                let message_json = to_json_text(message)?;
                tx.execute(
                    "INSERT OR REPLACE INTO agent_messages (
                        sequence,
                        from_agent,
                        to_agent,
                        body,
                        correlation_id,
                        message_json
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        sequence as i64,
                        message.from.0,
                        message.to.0,
                        message.body,
                        message.correlation_id,
                        message_json,
                    ],
                )
                .map_err(|error| persistence_error("save message history", error))?;
            }
            CoreEvent::CompletionPacketDelivered { packet } => {
                self.save_completion_packet_in_tx(&tx, sequence, packet)?;
            }
            CoreEvent::BrainEventObserved {
                session_id,
                wake_id,
                event,
            } => {
                self.save_tool_call_in_tx(&tx, sequence, session_id, wake_id.as_deref(), event)?;
            }
            _ => {}
        }

        tx.commit()
            .map_err(|error| persistence_error("commit save event", error))?;
        Ok(())
    }

    pub fn load_event_history(&self) -> CoreResult<Vec<PersistedEvent>> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare("SELECT sequence, event_json FROM event_history ORDER BY sequence ASC")
            .map_err(|error| persistence_error("prepare load event history", error))?;

        let rows = stmt
            .query_map([], |row| {
                let event_json: String = row.get(1)?;
                let event = from_json_text::<CoreEvent>(&event_json).map_err(to_sql_error)?;
                Ok(PersistedEvent {
                    sequence: row.get::<_, i64>(0)? as u64,
                    event,
                })
            })
            .map_err(|error| persistence_error("query event history", error))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| persistence_error("load event history", error))
    }

    pub fn load_tool_call_history(&self) -> CoreResult<Vec<ToolCallRecord>> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT sequence, session_id, wake_id, tool_name, phase, is_error
                 FROM tool_call_history
                 ORDER BY sequence ASC",
            )
            .map_err(|error| persistence_error("prepare tool call history", error))?;

        let rows = stmt
            .query_map([], |row| {
                let phase: String = row.get(4)?;
                Ok(ToolCallRecord {
                    sequence: row.get::<_, i64>(0)? as u64,
                    session_id: SessionId(row.get(1)?),
                    wake_id: row.get(2)?,
                    tool_name: row.get(3)?,
                    phase: tool_call_phase_from_str(&phase)?,
                    is_error: row.get::<_, Option<i64>>(5)?.map(|value| value != 0),
                })
            })
            .map_err(|error| persistence_error("query tool call history", error))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| persistence_error("load tool call history", error))
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
                fan_out_failure_policy
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
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
            ],
        )
        .map_err(|error| persistence_error("save worker run", error))?;
        Ok(())
    }

    pub fn load_worker_run(&self, run_id: &RunId) -> CoreResult<Option<WorkerRunRecord>> {
        let conn = self.conn()?;
        conn.query_row(
            "SELECT
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
                fan_out_failure_policy
             FROM worker_runs
             WHERE run_id = ?1",
            params![run_id.0.as_str()],
            row_to_worker_run,
        )
        .optional()
        .map_err(|error| persistence_error("load worker run", error))
    }

    pub fn load_worker_run_by_delegated_session(
        &self,
        delegated_session_id: &SessionId,
    ) -> CoreResult<Option<WorkerRunRecord>> {
        let conn = self.conn()?;
        conn.query_row(
            "SELECT
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
                fan_out_failure_policy
             FROM worker_runs
             WHERE delegated_session_id = ?1",
            params![delegated_session_id.0.as_str()],
            row_to_worker_run,
        )
        .optional()
        .map_err(|error| persistence_error("load worker run by delegated session", error))
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
                "SELECT
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
                    fan_out_failure_policy
                 FROM worker_runs
                 WHERE session_id = ?1 AND fan_out_group_id = ?2
                 ORDER BY source_wake_id ASC, source_action_index ASC",
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
                "SELECT
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
                    fan_out_failure_policy
                 FROM worker_runs
                 WHERE session_id = ?1 AND fan_out_group_id IS NOT NULL
                 ORDER BY fan_out_group_id ASC, source_wake_id ASC, source_action_index ASC",
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

    pub fn count_rows(&self, table: &str) -> CoreResult<u64> {
        if !matches!(
            table,
            "sessions"
                | "event_history"
                | "agent_messages"
                | "completion_packets"
                | "worker_runs"
                | "tool_call_history"
        ) {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("unsupported persistence table {table}"),
            ));
        }

        let conn = self.conn()?;
        let count = conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get::<_, i64>(0)
            })
            .optional()
            .map_err(|error| persistence_error("count rows", error))?
            .unwrap_or(0);
        Ok(count as u64)
    }

    fn migrate(&self) -> CoreResult<()> {
        let conn = self.conn()?;
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE IF NOT EXISTS sessions (
                session_id TEXT PRIMARY KEY,
                handle INTEGER NOT NULL UNIQUE,
                agent_id TEXT NOT NULL,
                profile_id TEXT NOT NULL,
                kind_json TEXT NOT NULL,
                delegation_json TEXT,
                resource_limits_json TEXT,
                tool_profile_json TEXT,
                status_json TEXT NOT NULL,
                brain_turn_count INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                last_active_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS event_history (
                sequence INTEGER PRIMARY KEY,
                event_kind TEXT NOT NULL,
                event_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS agent_messages (
                sequence INTEGER PRIMARY KEY,
                from_agent TEXT NOT NULL,
                to_agent TEXT NOT NULL,
                body TEXT NOT NULL,
                correlation_id TEXT,
                message_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS worker_runs (
                run_id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                delegated_session_id TEXT,
                parent_agent_id TEXT,
                profile_id TEXT NOT NULL,
                task_id TEXT,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                last_updated_at TEXT NOT NULL,
                source_wake_id TEXT NOT NULL,
                source_action_index INTEGER NOT NULL,
                delegation_correlation_id TEXT,
                parent_consumption TEXT NOT NULL DEFAULT 'await_completion',
                fan_out_group_id TEXT,
                fan_out_max_concurrency INTEGER,
                fan_out_failure_policy TEXT NOT NULL DEFAULT 'fail_soft'
            );

            CREATE TABLE IF NOT EXISTS completion_packets (
                sequence INTEGER PRIMARY KEY,
                session_id TEXT NOT NULL,
                status TEXT NOT NULL,
                summary TEXT NOT NULL,
                packet_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS tool_call_history (
                sequence INTEGER PRIMARY KEY,
                session_id TEXT NOT NULL,
                wake_id TEXT,
                tool_name TEXT NOT NULL,
                phase TEXT NOT NULL,
                is_error INTEGER
            );

            INSERT OR IGNORE INTO schema_migrations (version) VALUES (1);
            ",
        )
        .map_err(|error| persistence_error("migrate sqlite", error))?;
        add_missing_column(&conn, "sessions", "delegation_json", "TEXT")?;
        add_missing_column(&conn, "sessions", "resource_limits_json", "TEXT")?;
        add_missing_column(&conn, "sessions", "tool_profile_json", "TEXT")?;
        add_missing_column(&conn, "worker_runs", "delegated_session_id", "TEXT")?;
        add_missing_column(&conn, "worker_runs", "parent_agent_id", "TEXT")?;
        add_missing_column(&conn, "worker_runs", "delegation_correlation_id", "TEXT")?;
        add_missing_column(
            &conn,
            "worker_runs",
            "parent_consumption",
            "TEXT NOT NULL DEFAULT 'await_completion'",
        )?;
        add_missing_column(&conn, "worker_runs", "fan_out_group_id", "TEXT")?;
        add_missing_column(&conn, "worker_runs", "fan_out_max_concurrency", "INTEGER")?;
        add_missing_column(
            &conn,
            "worker_runs",
            "fan_out_failure_policy",
            "TEXT NOT NULL DEFAULT 'fail_soft'",
        )?;
        Ok(())
    }

    fn save_completion_packet_in_tx(
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

    fn save_tool_call_in_tx(
        &self,
        tx: &rusqlite::Transaction<'_>,
        sequence: u64,
        session_id: &SessionId,
        wake_id: Option<&str>,
        event: &BrainEvent,
    ) -> CoreResult<()> {
        let (tool_name, phase, is_error) = match event {
            BrainEvent::ToolCallStarted { tool_name } => (tool_name, ToolCallPhase::Started, None),
            BrainEvent::ToolCallFinished {
                tool_name,
                is_error,
            } => (tool_name, ToolCallPhase::Finished, Some(*is_error)),
            _ => return Ok(()),
        };
        tx.execute(
            "INSERT OR REPLACE INTO tool_call_history (
                sequence,
                session_id,
                wake_id,
                tool_name,
                phase,
                is_error
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                sequence as i64,
                session_id.0,
                wake_id,
                tool_name,
                phase.as_str(),
                is_error.map(|value| if value { 1_i64 } else { 0_i64 }),
            ],
        )
        .map_err(|error| persistence_error("save tool call history", error))?;
        Ok(())
    }

    fn conn(&self) -> CoreResult<std::sync::MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|_| CoreError::new(CoreErrorKind::InternalError, "sqlite lock poisoned"))
    }
}

pub fn coordination_db_path(engine_data_dir: impl AsRef<Path>) -> PathBuf {
    engine_data_dir.as_ref().join(DB_FILE_NAME)
}

fn should_persist_event(event: &CoreEvent) -> bool {
    !matches!(
        event,
        CoreEvent::DenDataUpdated { .. } | CoreEvent::ExternalEventInjected { .. }
    )
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

fn tool_call_phase_from_str(raw: &str) -> rusqlite::Result<ToolCallPhase> {
    match raw {
        "started" => Ok(ToolCallPhase::Started),
        "finished" => Ok(ToolCallPhase::Finished),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            4,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unsupported tool call phase {other}"),
            )),
        )),
    }
}

fn parent_consumption_policy_as_str(policy: &ParentConsumptionPolicy) -> &'static str {
    match policy {
        ParentConsumptionPolicy::AwaitCompletion => "await_completion",
        ParentConsumptionPolicy::ObserveOnly => "observe_only",
    }
}

fn parent_consumption_policy_from_str(raw: &str) -> rusqlite::Result<ParentConsumptionPolicy> {
    match raw {
        "await_completion" => Ok(ParentConsumptionPolicy::AwaitCompletion),
        "observe_only" => Ok(ParentConsumptionPolicy::ObserveOnly),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            12,
            rusqlite::types::Type::Text,
            Box::new(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown parent consumption policy {other}"),
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

fn aggregate_fan_out_groups(mut runs: Vec<WorkerRunRecord>) -> Vec<DelegatedFanOutGroup> {
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

fn add_missing_column(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> CoreResult<()> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| persistence_error("prepare table info", error))?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| persistence_error("query table info", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("read table info", error))?;

    if columns.iter().any(|existing| existing == column) {
        return Ok(());
    }

    conn.execute(
        &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
        [],
    )
    .map_err(|error| persistence_error("add missing sqlite column", error))?;
    Ok(())
}

fn to_json_text<T: Serialize>(value: &T) -> CoreResult<String> {
    serde_json::to_string(value)
        .map_err(|error| persistence_error("serialize coordination record", error))
}

fn from_json_text<T: DeserializeOwned>(value: &str) -> Result<T, serde_json::Error> {
    serde_json::from_str(value)
}

fn to_sql_error(error: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}

fn persistence_error(context: &str, error: impl std::error::Error) -> CoreError {
    CoreError::new(
        CoreErrorKind::PersistenceFailure,
        format!("{context}: {error}"),
    )
}
