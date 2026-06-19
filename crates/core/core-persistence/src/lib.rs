//! Local coordination-state persistence.
//!
//! This store is for Rust-owned coordination state only. Den task, project, and
//! document data remains Den product data and is not mirrored here.

use rusqlite::{params, Connection, OptionalExtension};
use rusty_crew_core_protocol::{
    BrainAction, BrainActionBatch, CompletionPacket, CoreError, CoreErrorKind, CoreEvent,
    CoreEventKind, CoreResult, IsoTimestamp, ProfileId, RunId, SessionHandle, SessionId,
    SessionKind, SessionState, SessionStatus, TaskId,
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
pub enum WorkerRunStatus {
    Requested,
}

impl WorkerRunStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Requested => "requested",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerRunRecord {
    pub run_id: RunId,
    pub session_id: SessionId,
    pub profile_id: ProfileId,
    pub task_id: Option<TaskId>,
    pub status: WorkerRunStatus,
    pub created_at: IsoTimestamp,
    pub last_updated_at: IsoTimestamp,
    pub source_wake_id: String,
    pub source_action_index: u32,
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
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO sessions (
                session_id,
                handle,
                agent_id,
                profile_id,
                kind_json,
                status_json,
                brain_turn_count,
                created_at,
                last_active_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(session_id) DO UPDATE SET
                handle = excluded.handle,
                agent_id = excluded.agent_id,
                profile_id = excluded.profile_id,
                kind_json = excluded.kind_json,
                status_json = excluded.status_json,
                brain_turn_count = excluded.brain_turn_count,
                last_active_at = excluded.last_active_at",
            params![
                state.session_id.0,
                state.handle.get() as i64,
                state.agent_id.0,
                state.profile_id.0,
                kind_json,
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
                let status_json: String = row.get(5)?;
                let kind = from_json_text::<SessionKind>(&kind_json).map_err(to_sql_error)?;
                let status = from_json_text::<SessionStatus>(&status_json).map_err(to_sql_error)?;
                Ok(SessionState {
                    session_id: SessionId(row.get(0)?),
                    handle: SessionHandle::new(row.get::<_, i64>(1)? as u64),
                    agent_id: rusty_crew_core_protocol::AgentId(row.get(2)?),
                    profile_id: ProfileId(row.get(3)?),
                    kind,
                    status,
                    brain_turn_count: row.get::<_, i64>(6)? as u32,
                    created_at: row.get(7)?,
                    last_active_at: row.get(8)?,
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

    pub fn save_worker_runs_requested(
        &self,
        batch: &BrainActionBatch,
        now: IsoTimestamp,
    ) -> CoreResult<()> {
        let conn = self.conn()?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|error| persistence_error("begin save worker runs", error))?;

        for (index, action) in batch.actions.iter().enumerate() {
            let BrainAction::RequestDelegation {
                profile_id,
                task_id,
                ..
            } = action
            else {
                continue;
            };

            let record = WorkerRunRecord {
                run_id: RunId::new(format!("{}:{index}", batch.wake_id)),
                session_id: batch.session_id.clone(),
                profile_id: profile_id.clone(),
                task_id: task_id.clone(),
                status: WorkerRunStatus::Requested,
                created_at: now.clone(),
                last_updated_at: now.clone(),
                source_wake_id: batch.wake_id.clone(),
                source_action_index: index as u32,
            };
            tx.execute(
                "INSERT OR REPLACE INTO worker_runs (
                    run_id,
                    session_id,
                    profile_id,
                    task_id,
                    status,
                    created_at,
                    last_updated_at,
                    source_wake_id,
                    source_action_index
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    record.run_id.0,
                    record.session_id.0,
                    record.profile_id.0,
                    record.task_id.map(|task_id| task_id.0),
                    record.status.as_str(),
                    record.created_at,
                    record.last_updated_at,
                    record.source_wake_id,
                    record.source_action_index as i64,
                ],
            )
            .map_err(|error| persistence_error("save worker run", error))?;
        }

        tx.commit()
            .map_err(|error| persistence_error("commit worker runs", error))?;
        Ok(())
    }

    pub fn count_rows(&self, table: &str) -> CoreResult<u64> {
        if !matches!(
            table,
            "sessions" | "event_history" | "agent_messages" | "completion_packets" | "worker_runs"
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
                profile_id TEXT NOT NULL,
                task_id TEXT,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                last_updated_at TEXT NOT NULL,
                source_wake_id TEXT NOT NULL,
                source_action_index INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS completion_packets (
                sequence INTEGER PRIMARY KEY,
                session_id TEXT NOT NULL,
                status TEXT NOT NULL,
                summary TEXT NOT NULL,
                packet_json TEXT NOT NULL
            );

            INSERT OR IGNORE INTO schema_migrations (version) VALUES (1);
            ",
        )
        .map_err(|error| persistence_error("migrate sqlite", error))?;
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
