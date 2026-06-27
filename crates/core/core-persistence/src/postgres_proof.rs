//! Narrow PostgreSQL proof slice for the runtime counter repository.
//!
//! This module is intentionally not the full `CoordinationStore` backend. It
//! exists to prove connection, migration, typed API parity, and diagnostics for
//! one low-risk repository before correctness-sensitive coordination state moves
//! beyond SQLite.

use crate::{
    counter_value, from_json_text, repositories, to_json_text, validate_simple_kv_identity,
    validate_simple_kv_query, validate_simple_kv_write, CoreError, CoreErrorKind, CoreResult,
    IsoTimestamp, QueryPage, RuntimeCounterQuery, RuntimeCounterRecord, RuntimeCounterScope,
    RuntimeRepositoryGroupDiagnostic, RuntimeSearchFilter, RuntimeSearchResult,
    RuntimeSearchRowType, RuntimeStateSummary, RuntimeStorageCapability, RuntimeStorageTableCount,
    SimpleKvCompareAndSwap, SimpleKvDelete, SimpleKvQuery, SimpleKvRecord, SimpleKvScope,
    SimpleKvWrite, COUNTER_BRAIN_TURNS, COUNTER_COMPLETIONS, COUNTER_DELEGATIONS_CANCELLED,
    COUNTER_DELEGATIONS_COMPLETED, COUNTER_DELEGATIONS_CREATED, COUNTER_DELEGATIONS_FAILED,
    COUNTER_DELEGATIONS_TIMED_OUT, COUNTER_MESSAGES, COUNTER_QUEUE_EXPIRATIONS, COUNTER_TOOL_CALLS,
    COUNTER_TOOL_ERRORS, COUNTER_WAKES,
};
use postgres::{Client, NoTls, Row};
use std::sync::{Mutex, MutexGuard};

const POSTGRES_PROOF_SCHEMA_VERSION: i64 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostgresRuntimeCounterProofConfig {
    pub database_url_env: String,
    pub schema: String,
}

impl Default for PostgresRuntimeCounterProofConfig {
    fn default() -> Self {
        Self {
            database_url_env: "RUSTY_CREW_DATABASE_URL".to_string(),
            schema: "rusty_crew".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostgresRuntimeCounterProofDiagnostics {
    pub backend: String,
    pub backend_label: String,
    pub schema: String,
    pub proof_repository: String,
    pub schema_version: i64,
    pub table_counts: Vec<RuntimeStorageTableCount>,
    pub capabilities: Vec<RuntimeStorageCapability>,
    pub repository_groups: Vec<RuntimeRepositoryGroupDiagnostic>,
}

pub struct PostgresRuntimeCounterProofStore {
    schema: String,
    client: Mutex<Client>,
}

impl PostgresRuntimeCounterProofStore {
    pub fn connect_from_env(config: &PostgresRuntimeCounterProofConfig) -> CoreResult<Self> {
        let database_url = std::env::var(&config.database_url_env).map_err(|error| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!(
                    "load PostgreSQL proof database URL from env {}: {error}",
                    config.database_url_env
                ),
            )
        })?;
        Self::connect(&database_url, &config.schema)
    }

    pub fn connect(database_url: &str, schema: &str) -> CoreResult<Self> {
        validate_postgres_identifier("postgres schema", schema)?;
        let client = Client::connect(database_url, NoTls)
            .map_err(|error| postgres_error("connect PostgreSQL runtime counter proof", error))?;
        let store = Self {
            schema: schema.to_string(),
            client: Mutex::new(client),
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn increment_counter(
        &self,
        scope: &RuntimeCounterScope,
        counter_name: &str,
        amount: u64,
        now: &IsoTimestamp,
    ) -> CoreResult<()> {
        if amount == 0 {
            return Ok(());
        }
        validate_counter_amount(amount)?;
        let (scope_type, scope_id) = runtime_counter_scope_parts(scope);
        let schema = self.quoted_schema();
        self.client()?
            .execute(
                &format!(
                    "INSERT INTO {schema}.runtime_counters (
                    scope_type,
                    scope_id,
                    counter_name,
                    value,
                    updated_at
                ) VALUES ($1, $2, $3, $4, $5)
                ON CONFLICT(scope_type, scope_id, counter_name) DO UPDATE SET
                    value = runtime_counters.value + EXCLUDED.value,
                    updated_at = EXCLUDED.updated_at"
                ),
                &[&scope_type, &scope_id, &counter_name, &(amount as i64), now],
            )
            .map_err(|error| postgres_error("increment PostgreSQL runtime counter", error))?;
        Ok(())
    }

    pub fn runtime_counters(
        &self,
        scope: Option<&RuntimeCounterScope>,
    ) -> CoreResult<Vec<RuntimeCounterRecord>> {
        self.query_runtime_counters(&RuntimeCounterQuery {
            scope: scope.cloned(),
            counter_name: None,
            page: None,
        })
    }

    pub fn query_runtime_counters(
        &self,
        query: &RuntimeCounterQuery,
    ) -> CoreResult<Vec<RuntimeCounterRecord>> {
        let scope_parts = query.scope.as_ref().map(runtime_counter_scope_parts);
        let scope_type = scope_parts.as_ref().map(|(scope_type, _)| *scope_type);
        let scope_id = scope_parts.as_ref().map(|(_, scope_id)| scope_id.as_str());
        let counter_name = query.counter_name.as_deref();
        let (limit, offset) = query
            .page
            .unwrap_or(QueryPage {
                limit: None,
                offset: None,
            })
            .bounded(200, 5_000);
        let schema = self.quoted_schema();
        let rows = self
            .client()?
            .query(
                &format!(
                    "SELECT scope_type, scope_id, counter_name, value, updated_at
                     FROM {schema}.runtime_counters
                     WHERE ($1::text IS NULL OR scope_type = $1)
                       AND ($2::text IS NULL OR scope_id = $2)
                       AND ($3::text IS NULL OR counter_name = $3)
                     ORDER BY scope_type ASC, scope_id ASC, counter_name ASC
                     LIMIT $4 OFFSET $5"
                ),
                &[&scope_type, &scope_id, &counter_name, &limit, &offset],
            )
            .map_err(|error| postgres_error("query PostgreSQL runtime counters", error))?;
        rows.iter().map(row_to_runtime_counter).collect()
    }

    pub fn reset_runtime_counters(
        &self,
        query: &RuntimeCounterQuery,
        now: IsoTimestamp,
    ) -> CoreResult<u64> {
        let scope_parts = query.scope.as_ref().map(runtime_counter_scope_parts);
        let scope_type = scope_parts.as_ref().map(|(scope_type, _)| *scope_type);
        let scope_id = scope_parts.as_ref().map(|(_, scope_id)| scope_id.as_str());
        let counter_name = query.counter_name.as_deref();
        let schema = self.quoted_schema();
        self.client()?
            .execute(
                &format!(
                    "UPDATE {schema}.runtime_counters
                     SET value = 0, updated_at = $4
                     WHERE ($1::text IS NULL OR scope_type = $1)
                       AND ($2::text IS NULL OR scope_id = $2)
                       AND ($3::text IS NULL OR counter_name = $3)"
                ),
                &[&scope_type, &scope_id, &counter_name, &now],
            )
            .map_err(|error| postgres_error("reset PostgreSQL runtime counters", error))
    }

    pub fn runtime_summary(&self, scope: &RuntimeCounterScope) -> CoreResult<RuntimeStateSummary> {
        let counters = self.runtime_counters(Some(scope))?;
        Ok(RuntimeStateSummary {
            scope: scope.clone(),
            brain_turns: counter_value(&counters, COUNTER_BRAIN_TURNS),
            wakes: counter_value(&counters, COUNTER_WAKES),
            tool_calls: counter_value(&counters, COUNTER_TOOL_CALLS),
            tool_errors: counter_value(&counters, COUNTER_TOOL_ERRORS),
            delegations_created: counter_value(&counters, COUNTER_DELEGATIONS_CREATED),
            delegations_completed: counter_value(&counters, COUNTER_DELEGATIONS_COMPLETED),
            delegations_failed: counter_value(&counters, COUNTER_DELEGATIONS_FAILED),
            delegations_timed_out: counter_value(&counters, COUNTER_DELEGATIONS_TIMED_OUT),
            delegations_cancelled: counter_value(&counters, COUNTER_DELEGATIONS_CANCELLED),
            messages: counter_value(&counters, COUNTER_MESSAGES),
            completions: counter_value(&counters, COUNTER_COMPLETIONS),
            queue_expirations: counter_value(&counters, COUNTER_QUEUE_EXPIRATIONS),
        })
    }

    pub fn get_simple_kv(
        &self,
        scope: &SimpleKvScope,
        key: &str,
        now: Option<&IsoTimestamp>,
    ) -> CoreResult<Option<SimpleKvRecord>> {
        validate_simple_kv_identity(scope, key)?;
        let schema = self.quoted_schema();
        let row = self
            .client()?
            .query_opt(
                &format!(
                    "SELECT scope_type, scope_id, entry_key, value_json, revision,
                            created_at, updated_at, expires_at
                     FROM {schema}.module_simple_kv_entries
                     WHERE scope_type = $1
                       AND scope_id = $2
                       AND entry_key = $3
                       AND (expires_at IS NULL OR $4::text IS NULL OR expires_at > $4)"
                ),
                &[&scope.scope_type, &scope.scope_id, &key, &now],
            )
            .map_err(|error| postgres_error("get PostgreSQL simple_kv entry", error))?;
        row.as_ref().map(row_to_simple_kv).transpose()
    }

    pub fn list_simple_kv(&self, query: &SimpleKvQuery) -> CoreResult<Vec<SimpleKvRecord>> {
        validate_simple_kv_query(query)?;
        let (limit, offset) = query
            .page
            .unwrap_or(QueryPage {
                limit: None,
                offset: None,
            })
            .bounded(100, 1_000);
        let key_prefix = query
            .key_prefix
            .as_ref()
            .map(|prefix| postgres_like_prefix(prefix));
        let schema = self.quoted_schema();
        let rows = self
            .client()?
            .query(
                &format!(
                    "SELECT scope_type, scope_id, entry_key, value_json, revision,
                            created_at, updated_at, expires_at
                     FROM {schema}.module_simple_kv_entries
                     WHERE scope_type = $1
                       AND scope_id = $2
                       AND ($3::text IS NULL OR entry_key LIKE $3 ESCAPE '\\')
                       AND (
                            ($4 AND expires_at IS NOT NULL AND $5::text IS NOT NULL AND expires_at <= $5)
                            OR
                            (NOT $4 AND ($6 OR expires_at IS NULL OR $5::text IS NULL OR expires_at > $5))
                       )
                     ORDER BY entry_key ASC
                     LIMIT $7 OFFSET $8"
                ),
                &[
                    &query.scope.scope_type,
                    &query.scope.scope_id,
                    &key_prefix,
                    &query.expired_only,
                    &query.now,
                    &query.include_expired,
                    &limit,
                    &offset,
                ],
            )
            .map_err(|error| postgres_error("list PostgreSQL simple_kv entries", error))?;
        rows.iter().map(row_to_simple_kv).collect()
    }

    pub fn put_simple_kv(&self, write: &SimpleKvWrite) -> CoreResult<SimpleKvRecord> {
        validate_simple_kv_write(write)?;
        let existing = self.get_simple_kv(&write.scope, &write.key, None)?;
        match existing {
            Some(existing) => self.update_simple_kv(write, existing.revision + 1),
            None => self.insert_simple_kv(write),
        }
    }

    pub fn compare_and_swap_simple_kv(
        &self,
        compare_and_swap: &SimpleKvCompareAndSwap,
    ) -> CoreResult<SimpleKvRecord> {
        validate_simple_kv_write(&compare_and_swap.write)?;
        let existing = self
            .get_simple_kv(
                &compare_and_swap.write.scope,
                &compare_and_swap.write.key,
                None,
            )?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    format!(
                        "simple_kv entry {}/{} not found",
                        compare_and_swap.write.scope.scope_id, compare_and_swap.write.key
                    ),
                )
            })?;
        if existing.revision != compare_and_swap.expected_revision {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "simple_kv revision mismatch for {}/{}: expected {}, found {}",
                    compare_and_swap.write.scope.scope_id,
                    compare_and_swap.write.key,
                    compare_and_swap.expected_revision,
                    existing.revision
                ),
            ));
        }
        self.update_simple_kv(&compare_and_swap.write, existing.revision + 1)
    }

    pub fn delete_simple_kv(&self, delete: &SimpleKvDelete) -> CoreResult<SimpleKvRecord> {
        validate_simple_kv_identity(&delete.scope, &delete.key)?;
        let existing = self
            .get_simple_kv(&delete.scope, &delete.key, None)?
            .ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::NotFound,
                    format!(
                        "simple_kv entry {}/{} not found",
                        delete.scope.scope_id, delete.key
                    ),
                )
            })?;
        if existing.revision != delete.expected_revision {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "simple_kv revision mismatch for {}/{}: expected {}, found {}",
                    delete.scope.scope_id, delete.key, delete.expected_revision, existing.revision
                ),
            ));
        }
        let schema = self.quoted_schema();
        self.client()?
            .execute(
                &format!(
                    "DELETE FROM {schema}.module_simple_kv_entries
                     WHERE scope_type = $1 AND scope_id = $2 AND entry_key = $3"
                ),
                &[
                    &delete.scope.scope_type,
                    &delete.scope.scope_id,
                    &delete.key,
                ],
            )
            .map_err(|error| postgres_error("delete PostgreSQL simple_kv entry", error))?;
        Ok(existing)
    }

    pub fn expire_simple_kv(&self, now: &IsoTimestamp) -> CoreResult<u64> {
        let schema = self.quoted_schema();
        let changed = self
            .client()?
            .execute(
                &format!(
                    "DELETE FROM {schema}.module_simple_kv_entries
                     WHERE expires_at IS NOT NULL AND expires_at <= $1"
                ),
                &[now],
            )
            .map_err(|error| postgres_error("expire PostgreSQL simple_kv entries", error))?;
        Ok(changed)
    }

    pub fn storage_diagnostics(&self) -> CoreResult<PostgresRuntimeCounterProofDiagnostics> {
        Ok(PostgresRuntimeCounterProofDiagnostics {
            backend: "postgres".to_string(),
            backend_label: "PostgreSQL runtime-counter proof slice".to_string(),
            schema: self.schema.clone(),
            proof_repository: "runtime_counters,module_simple_kv_entries,runtime_search"
                .to_string(),
            schema_version: self.schema_version()?,
            table_counts: vec![
                RuntimeStorageTableCount {
                    table: "runtime_counters".to_string(),
                    rows: self.runtime_counter_rows()?,
                },
                RuntimeStorageTableCount {
                    table: "module_simple_kv_entries".to_string(),
                    rows: self.simple_kv_rows()?,
                },
                RuntimeStorageTableCount {
                    table: "runtime_search_entries".to_string(),
                    rows: self.runtime_search_rows()?,
                },
            ],
            capabilities: postgres_proof_capabilities(),
            repository_groups: postgres_proof_repository_groups(),
        })
    }

    pub fn upsert_runtime_search_entry(&self, entry: &RuntimeSearchResult) -> CoreResult<()> {
        let schema = self.quoted_schema();
        let row_type = runtime_search_row_type_as_str(entry.row_type);
        let event_kind = entry.event_kind.as_ref().map(|kind| format!("{kind:?}"));
        self.client()?
            .execute(
                &format!(
                    "INSERT INTO {schema}.runtime_search_entries (
                        row_type,
                        row_key,
                        sequence,
                        session_id,
                        agent_id,
                        instance_id,
                        task_id,
                        event_kind,
                        recorded_at,
                        title,
                        body
                     ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                     ON CONFLICT(row_type, row_key) DO UPDATE SET
                        sequence = EXCLUDED.sequence,
                        session_id = EXCLUDED.session_id,
                        agent_id = EXCLUDED.agent_id,
                        instance_id = EXCLUDED.instance_id,
                        task_id = EXCLUDED.task_id,
                        event_kind = EXCLUDED.event_kind,
                        recorded_at = EXCLUDED.recorded_at,
                        title = EXCLUDED.title,
                        body = EXCLUDED.body"
                ),
                &[
                    &row_type,
                    &entry.row_key,
                    &entry.sequence.map(|value| value as i64),
                    &entry.session_id.as_ref().map(|value| value.0.as_str()),
                    &entry.agent_id.as_ref().map(|value| value.0.as_str()),
                    &entry.instance_id.as_ref().map(|value| value.0.as_str()),
                    &entry.task_id.as_ref().map(|value| value.0.as_str()),
                    &event_kind,
                    &entry.recorded_at,
                    &entry.title,
                    &entry.body,
                ],
            )
            .map_err(|error| postgres_error("upsert PostgreSQL runtime search entry", error))?;
        Ok(())
    }

    pub fn search_runtime(
        &self,
        filter: &RuntimeSearchFilter,
    ) -> CoreResult<Vec<RuntimeSearchResult>> {
        if filter.query.trim().is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "runtime search query must be non-empty",
            ));
        }
        let row_type = filter.row_type.map(runtime_search_row_type_as_str);
        let session_id = filter.session_id.as_ref().map(|value| value.0.as_str());
        let agent_id = filter.agent_id.as_ref().map(|value| value.0.as_str());
        let instance_id = filter.instance_id.as_ref().map(|value| value.0.as_str());
        let task_id = filter.task_id.as_ref().map(|value| value.0.as_str());
        let event_kind = filter.event_kind.as_ref().map(|kind| format!("{kind:?}"));
        let recorded_after = filter.recorded_after.as_deref();
        let recorded_before = filter.recorded_before.as_deref();
        let limit = filter.limit.unwrap_or(50).clamp(1, 200) as i64;
        let query = filter.query.trim();
        let schema = self.quoted_schema();
        let rows = self
            .client()?
            .query(
                &format!(
                    "SELECT
                        row_type,
                        row_key,
                        sequence,
                        session_id,
                        agent_id,
                        instance_id,
                        task_id,
                        event_kind,
                        recorded_at,
                        title,
                        body
                     FROM {schema}.runtime_search_entries
                     WHERE search_vector @@ plainto_tsquery('simple', $1)
                       AND ($2::text IS NULL OR row_type = $2)
                       AND ($3::text IS NULL OR session_id = $3)
                       AND ($4::text IS NULL OR agent_id = $4)
                       AND ($5::text IS NULL OR instance_id = $5)
                       AND ($6::text IS NULL OR task_id = $6)
                       AND ($7::text IS NULL OR event_kind = $7)
                       AND ($8::text IS NULL OR recorded_at >= $8)
                       AND ($9::text IS NULL OR recorded_at <= $9)
                     ORDER BY
                       ts_rank(search_vector, plainto_tsquery('simple', $1)) DESC,
                       recorded_at ASC,
                       row_type ASC,
                       row_key ASC
                     LIMIT $10"
                ),
                &[
                    &query,
                    &row_type,
                    &session_id,
                    &agent_id,
                    &instance_id,
                    &task_id,
                    &event_kind,
                    &recorded_after,
                    &recorded_before,
                    &limit,
                ],
            )
            .map_err(|error| postgres_error("query PostgreSQL runtime search", error))?;
        rows.iter().map(row_to_runtime_search_result).collect()
    }

    #[cfg(test)]
    fn drop_schema_for_test(&self) -> CoreResult<()> {
        let schema = self.quoted_schema();
        self.client()?
            .batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
            .map_err(|error| postgres_error("drop PostgreSQL proof schema", error))
    }

    fn migrate(&self) -> CoreResult<()> {
        let schema = self.quoted_schema();
        self.client()?
            .batch_execute(&format!(
                "CREATE SCHEMA IF NOT EXISTS {schema};
                 CREATE TABLE IF NOT EXISTS {schema}.rusty_crew_storage_metadata (
                    metadata_key TEXT PRIMARY KEY,
                    metadata_value TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                 );
                 INSERT INTO {schema}.rusty_crew_storage_metadata (
                    metadata_key,
                    metadata_value,
                    updated_at
                 ) VALUES (
                    'runtime_counter_proof_schema_version',
                    '{POSTGRES_PROOF_SCHEMA_VERSION}',
                    '2026-06-26T00:00:00Z'
                 )
                 ON CONFLICT(metadata_key) DO UPDATE SET
                    metadata_value = EXCLUDED.metadata_value,
                    updated_at = EXCLUDED.updated_at;
                 CREATE TABLE IF NOT EXISTS {schema}.runtime_counters (
                    scope_type TEXT NOT NULL,
                    scope_id TEXT NOT NULL,
                    counter_name TEXT NOT NULL,
                    value BIGINT NOT NULL DEFAULT 0,
                    updated_at TEXT NOT NULL,
                    PRIMARY KEY(scope_type, scope_id, counter_name)
                 );
                 CREATE INDEX IF NOT EXISTS runtime_counters_scope_idx
                    ON {schema}.runtime_counters(scope_type, scope_id);
                 CREATE TABLE IF NOT EXISTS {schema}.module_simple_kv_entries (
                    scope_type TEXT NOT NULL,
                    scope_id TEXT NOT NULL,
                    entry_key TEXT NOT NULL,
                    value_json TEXT NOT NULL,
                    revision BIGINT NOT NULL CHECK (revision > 0),
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    expires_at TEXT,
                    PRIMARY KEY(scope_type, scope_id, entry_key)
                 );
                 CREATE INDEX IF NOT EXISTS module_simple_kv_entries_scope_key_idx
                    ON {schema}.module_simple_kv_entries(scope_type, scope_id, entry_key);
                 CREATE INDEX IF NOT EXISTS module_simple_kv_entries_expires_at_idx
                    ON {schema}.module_simple_kv_entries(expires_at);
                 CREATE TABLE IF NOT EXISTS {schema}.runtime_search_entries (
                    row_type TEXT NOT NULL,
                    row_key TEXT NOT NULL,
                    sequence BIGINT,
                    session_id TEXT,
                    agent_id TEXT,
                    instance_id TEXT,
                    task_id TEXT,
                    event_kind TEXT,
                    recorded_at TEXT NOT NULL,
                    title TEXT NOT NULL,
                    body TEXT NOT NULL,
                    search_vector TSVECTOR GENERATED ALWAYS AS (
                        to_tsvector('simple', coalesce(title, '') || ' ' || coalesce(body, ''))
                    ) STORED,
                    PRIMARY KEY(row_type, row_key)
                 );
                 CREATE INDEX IF NOT EXISTS runtime_search_entries_vector_idx
                    ON {schema}.runtime_search_entries USING GIN(search_vector);
                 CREATE INDEX IF NOT EXISTS runtime_search_entries_metadata_idx
                    ON {schema}.runtime_search_entries(
                        row_type,
                        session_id,
                        agent_id,
                        instance_id,
                        task_id,
                        event_kind,
                        recorded_at
                    );"
            ))
            .map_err(|error| postgres_error("migrate PostgreSQL runtime counter proof", error))
    }

    fn insert_simple_kv(&self, write: &SimpleKvWrite) -> CoreResult<SimpleKvRecord> {
        let value_json = to_json_text(&write.value_json)?;
        let schema = self.quoted_schema();
        self.client()?
            .execute(
                &format!(
                    "INSERT INTO {schema}.module_simple_kv_entries (
                        scope_type,
                        scope_id,
                        entry_key,
                        value_json,
                        revision,
                        created_at,
                        updated_at,
                        expires_at
                     ) VALUES ($1, $2, $3, $4, 1, $5, $5, $6)"
                ),
                &[
                    &write.scope.scope_type,
                    &write.scope.scope_id,
                    &write.key,
                    &value_json,
                    &write.now,
                    &write.expires_at,
                ],
            )
            .map_err(|error| postgres_error("insert PostgreSQL simple_kv entry", error))?;
        Ok(SimpleKvRecord {
            scope: write.scope.clone(),
            key: write.key.clone(),
            value_json: write.value_json.clone(),
            revision: 1,
            created_at: write.now.clone(),
            updated_at: write.now.clone(),
            expires_at: write.expires_at.clone(),
        })
    }

    fn update_simple_kv(&self, write: &SimpleKvWrite, revision: u64) -> CoreResult<SimpleKvRecord> {
        validate_counter_amount(revision)?;
        let existing = self.get_simple_kv(&write.scope, &write.key, None)?;
        let created_at = existing
            .as_ref()
            .map(|record| record.created_at.clone())
            .unwrap_or_else(|| write.now.clone());
        let value_json = to_json_text(&write.value_json)?;
        let schema = self.quoted_schema();
        self.client()?
            .execute(
                &format!(
                    "UPDATE {schema}.module_simple_kv_entries
                     SET value_json = $4,
                         revision = $5,
                         updated_at = $6,
                         expires_at = $7
                     WHERE scope_type = $1
                       AND scope_id = $2
                       AND entry_key = $3"
                ),
                &[
                    &write.scope.scope_type,
                    &write.scope.scope_id,
                    &write.key,
                    &value_json,
                    &(revision as i64),
                    &write.now,
                    &write.expires_at,
                ],
            )
            .map_err(|error| postgres_error("update PostgreSQL simple_kv entry", error))?;
        Ok(SimpleKvRecord {
            scope: write.scope.clone(),
            key: write.key.clone(),
            value_json: write.value_json.clone(),
            revision,
            created_at,
            updated_at: write.now.clone(),
            expires_at: write.expires_at.clone(),
        })
    }

    fn schema_version(&self) -> CoreResult<i64> {
        let schema = self.quoted_schema();
        let row = self
            .client()?
            .query_one(
                &format!(
                    "SELECT metadata_value
                     FROM {schema}.rusty_crew_storage_metadata
                     WHERE metadata_key = 'runtime_counter_proof_schema_version'"
                ),
                &[],
            )
            .map_err(|error| postgres_error("load PostgreSQL proof schema version", error))?;
        let raw: String = row.get(0);
        raw.parse::<i64>().map_err(|error| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("parse PostgreSQL proof schema version: {error}"),
            )
        })
    }

    fn runtime_counter_rows(&self) -> CoreResult<u64> {
        let schema = self.quoted_schema();
        let row = self
            .client()?
            .query_one(
                &format!("SELECT COUNT(*) FROM {schema}.runtime_counters"),
                &[],
            )
            .map_err(|error| postgres_error("count PostgreSQL runtime counters", error))?;
        let rows: i64 = row.get(0);
        Ok(rows as u64)
    }

    fn simple_kv_rows(&self) -> CoreResult<u64> {
        let schema = self.quoted_schema();
        let row = self
            .client()?
            .query_one(
                &format!("SELECT COUNT(*) FROM {schema}.module_simple_kv_entries"),
                &[],
            )
            .map_err(|error| postgres_error("count PostgreSQL simple_kv entries", error))?;
        let rows: i64 = row.get(0);
        Ok(rows as u64)
    }

    fn runtime_search_rows(&self) -> CoreResult<u64> {
        let schema = self.quoted_schema();
        let row = self
            .client()?
            .query_one(
                &format!("SELECT COUNT(*) FROM {schema}.runtime_search_entries"),
                &[],
            )
            .map_err(|error| postgres_error("count PostgreSQL runtime search entries", error))?;
        let rows: i64 = row.get(0);
        Ok(rows as u64)
    }

    fn quoted_schema(&self) -> String {
        quote_postgres_identifier(&self.schema)
    }

    fn client(&self) -> CoreResult<MutexGuard<'_, Client>> {
        self.client.lock().map_err(|_| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "PostgreSQL proof connection mutex poisoned",
            )
        })
    }
}

fn postgres_proof_capabilities() -> Vec<RuntimeStorageCapability> {
    [
        (
            "transactions",
            true,
            "PostgreSQL transactions are available for the proof repository",
        ),
        (
            "json_metadata",
            true,
            "PostgreSQL can store JSON metadata, though runtime counters do not need it",
        ),
        (
            "concurrent_writers",
            true,
            "PostgreSQL supports concurrent writers for this proof repository",
        ),
        (
            "estimated_table_size",
            true,
            "the proof slice exposes row counts for proof-owned tables",
        ),
        (
            "row_level_claims",
            false,
            "not exercised by the runtime counter proof slice",
        ),
        (
            "runtime_full_text_search",
            true,
            "PostgreSQL runtime search proof uses tsvector behind the typed RuntimeSearchFilter API",
        ),
        (
            "logical_export_import",
            false,
            "logical cross-backend export/import remains future work",
        ),
    ]
    .into_iter()
    .map(|(name, supported, detail)| RuntimeStorageCapability {
        name: name.to_string(),
        supported,
        detail: detail.to_string(),
    })
    .collect()
}

fn postgres_proof_repository_groups() -> Vec<RuntimeRepositoryGroupDiagnostic> {
    repositories::core_repository_group_diagnostics()
        .into_iter()
        .map(|mut group| {
            if group.group_id == "storage_admin" {
                group.notes.insert(
                    0,
                    "PostgreSQL proof status: implemented for proof-owned migrations and storage diagnostics only.".to_string(),
                );
            } else if group.group_id == "module_schema_registry" {
                group.notes.insert(
                    0,
                    "PostgreSQL proof status: implemented only for the simple_kv module-owned data table, not the full module registry.".to_string(),
                );
            } else if group.group_id == "runtime_counters" {
                group.notes.insert(
                    0,
                    "PostgreSQL proof status: implemented for the runtime-counter proof repository.".to_string(),
                );
            } else if group.group_id == "runtime_search" {
                group.notes.insert(
                    0,
                    "PostgreSQL proof status: implemented for runtime search entries through the typed search API; not yet wired as the full service backend.".to_string(),
                );
            } else {
                group.notes.insert(
                    0,
                    "PostgreSQL proof status: unsupported; full service boot must fail closed before using this repository group.".to_string(),
                );
            }
            group
        })
        .collect()
}

fn runtime_counter_scope_parts(scope: &RuntimeCounterScope) -> (&'static str, String) {
    match scope {
        RuntimeCounterScope::Runtime => ("runtime", "_global".to_string()),
        RuntimeCounterScope::Agent(agent_id) => ("agent", agent_id.0.clone()),
        RuntimeCounterScope::Instance(instance_id) => ("instance", instance_id.0.clone()),
        RuntimeCounterScope::Session(session_id) => ("session", session_id.0.clone()),
    }
}

fn runtime_counter_scope_from_parts(
    scope_type: &str,
    scope_id: &str,
) -> CoreResult<RuntimeCounterScope> {
    match scope_type {
        "runtime" if scope_id == "_global" => Ok(RuntimeCounterScope::Runtime),
        "agent" => Ok(RuntimeCounterScope::Agent(crate::AgentId::new(scope_id))),
        "instance" => Ok(RuntimeCounterScope::Instance(crate::AgentInstanceId::new(
            scope_id,
        ))),
        "session" => Ok(RuntimeCounterScope::Session(crate::SessionId::new(
            scope_id,
        ))),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("unknown runtime counter scope {other}:{scope_id}"),
        )),
    }
}

fn row_to_runtime_counter(row: &Row) -> CoreResult<RuntimeCounterRecord> {
    let scope_type: String = row.get(0);
    let scope_id: String = row.get(1);
    let value: i64 = row.get(3);
    if value < 0 {
        return Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("runtime counter value must not be negative: {value}"),
        ));
    }
    Ok(RuntimeCounterRecord {
        scope: runtime_counter_scope_from_parts(&scope_type, &scope_id)?,
        counter_name: row.get(2),
        value: value as u64,
        updated_at: row.get(4),
    })
}

fn row_to_simple_kv(row: &Row) -> CoreResult<SimpleKvRecord> {
    let value_json: String = row.get(3);
    let revision: i64 = row.get(4);
    if revision <= 0 {
        return Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("invalid simple_kv revision {revision}"),
        ));
    }
    Ok(SimpleKvRecord {
        scope: SimpleKvScope {
            scope_type: row.get(0),
            scope_id: row.get(1),
        },
        key: row.get(2),
        value_json: from_json_text(&value_json).map_err(|error| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("parse PostgreSQL simple_kv value_json: {error}"),
            )
        })?,
        revision: revision as u64,
        created_at: row.get(5),
        updated_at: row.get(6),
        expires_at: row.get(7),
    })
}

fn row_to_runtime_search_result(row: &Row) -> CoreResult<RuntimeSearchResult> {
    let row_type: String = row.get(0);
    let sequence: Option<i64> = row.get(2);
    let event_kind: Option<String> = row.get(7);
    Ok(RuntimeSearchResult {
        row_type: runtime_search_row_type_from_str(&row_type)?,
        row_key: row.get(1),
        sequence: sequence.map(|value| value as u64),
        session_id: row.get::<_, Option<String>>(3).map(crate::SessionId),
        agent_id: row.get::<_, Option<String>>(4).map(crate::AgentId),
        instance_id: row.get::<_, Option<String>>(5).map(crate::AgentInstanceId),
        task_id: row.get::<_, Option<String>>(6).map(crate::TaskId),
        event_kind: event_kind
            .as_deref()
            .map(core_event_kind_from_debug_str)
            .transpose()?,
        recorded_at: row.get(8),
        title: row.get(9),
        body: row.get(10),
    })
}

fn runtime_search_row_type_as_str(row_type: RuntimeSearchRowType) -> &'static str {
    match row_type {
        RuntimeSearchRowType::Message => "message",
        RuntimeSearchRowType::QueueMessage => "queue_message",
        RuntimeSearchRowType::Session => "session",
    }
}

fn runtime_search_row_type_from_str(raw: &str) -> CoreResult<RuntimeSearchRowType> {
    match raw {
        "message" => Ok(RuntimeSearchRowType::Message),
        "queue_message" => Ok(RuntimeSearchRowType::QueueMessage),
        "session" => Ok(RuntimeSearchRowType::Session),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("unknown runtime search row type {other}"),
        )),
    }
}

fn core_event_kind_from_debug_str(raw: &str) -> CoreResult<crate::CoreEventKind> {
    match raw {
        "AgentMessageRouted" => Ok(crate::CoreEventKind::AgentMessageRouted),
        "SessionCreated" => Ok(crate::CoreEventKind::SessionCreated),
        "SessionArchived" => Ok(crate::CoreEventKind::SessionArchived),
        "BrainWakeRequested" => Ok(crate::CoreEventKind::BrainWakeRequested),
        "BrainEventObserved" => Ok(crate::CoreEventKind::BrainEventObserved),
        "BrainActionsAccepted" => Ok(crate::CoreEventKind::BrainActionsAccepted),
        "DelegationLifecycleObserved" => Ok(crate::CoreEventKind::DelegationLifecycleObserved),
        "CompletionPacketDelivered" => Ok(crate::CoreEventKind::CompletionPacketDelivered),
        "ExternalEventInjected" => Ok(crate::CoreEventKind::ExternalEventInjected),
        "DenDataUpdated" => Ok(crate::CoreEventKind::DenDataUpdated),
        other => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("unknown runtime search event kind {other}"),
        )),
    }
}

fn postgres_like_prefix(prefix: &str) -> String {
    let mut escaped = String::new();
    for character in prefix.chars() {
        match character {
            '%' | '_' | '\\' => {
                escaped.push('\\');
                escaped.push(character);
            }
            _ => escaped.push(character),
        }
    }
    escaped.push('%');
    escaped
}

fn validate_counter_amount(amount: u64) -> CoreResult<()> {
    if amount > i64::MAX as u64 {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "runtime counter increment exceeds PostgreSQL BIGINT range",
        ));
    }
    Ok(())
}

fn validate_postgres_identifier(label: &str, value: &str) -> CoreResult<()> {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("{label} must not be empty"),
        ));
    };
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("{label} must start with an ASCII letter or underscore"),
        ));
    }
    if !chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric()) {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("{label} must contain only ASCII letters, digits, or underscores"),
        ));
    }
    Ok(())
}

fn quote_postgres_identifier(identifier: &str) -> String {
    format!("\"{identifier}\"")
}

fn postgres_error(context: &str, error: postgres::Error) -> CoreError {
    CoreError::new(
        CoreErrorKind::PersistenceFailure,
        format!("{context}: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CoordinationStore, COUNTER_MESSAGES, COUNTER_WAKES};
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    trait SimpleKvConformanceStore {
        fn put_simple_kv(&self, write: &SimpleKvWrite) -> CoreResult<SimpleKvRecord>;
        fn compare_and_swap_simple_kv(
            &self,
            compare_and_swap: &SimpleKvCompareAndSwap,
        ) -> CoreResult<SimpleKvRecord>;
        fn get_simple_kv(
            &self,
            scope: &SimpleKvScope,
            key: &str,
            now: Option<&IsoTimestamp>,
        ) -> CoreResult<Option<SimpleKvRecord>>;
        fn list_simple_kv(&self, query: &SimpleKvQuery) -> CoreResult<Vec<SimpleKvRecord>>;
        fn delete_simple_kv(&self, delete: &SimpleKvDelete) -> CoreResult<SimpleKvRecord>;
        fn expire_simple_kv(&self, now: &IsoTimestamp) -> CoreResult<u64>;
    }

    impl SimpleKvConformanceStore for CoordinationStore {
        fn put_simple_kv(&self, write: &SimpleKvWrite) -> CoreResult<SimpleKvRecord> {
            CoordinationStore::put_simple_kv(self, write)
        }

        fn compare_and_swap_simple_kv(
            &self,
            compare_and_swap: &SimpleKvCompareAndSwap,
        ) -> CoreResult<SimpleKvRecord> {
            CoordinationStore::compare_and_swap_simple_kv(self, compare_and_swap)
        }

        fn get_simple_kv(
            &self,
            scope: &SimpleKvScope,
            key: &str,
            now: Option<&IsoTimestamp>,
        ) -> CoreResult<Option<SimpleKvRecord>> {
            CoordinationStore::get_simple_kv(self, scope, key, now)
        }

        fn list_simple_kv(&self, query: &SimpleKvQuery) -> CoreResult<Vec<SimpleKvRecord>> {
            CoordinationStore::list_simple_kv(self, query)
        }

        fn delete_simple_kv(&self, delete: &SimpleKvDelete) -> CoreResult<SimpleKvRecord> {
            CoordinationStore::delete_simple_kv(self, delete)
        }

        fn expire_simple_kv(&self, now: &IsoTimestamp) -> CoreResult<u64> {
            CoordinationStore::expire_simple_kv(self, now)
        }
    }

    impl SimpleKvConformanceStore for PostgresRuntimeCounterProofStore {
        fn put_simple_kv(&self, write: &SimpleKvWrite) -> CoreResult<SimpleKvRecord> {
            PostgresRuntimeCounterProofStore::put_simple_kv(self, write)
        }

        fn compare_and_swap_simple_kv(
            &self,
            compare_and_swap: &SimpleKvCompareAndSwap,
        ) -> CoreResult<SimpleKvRecord> {
            PostgresRuntimeCounterProofStore::compare_and_swap_simple_kv(self, compare_and_swap)
        }

        fn get_simple_kv(
            &self,
            scope: &SimpleKvScope,
            key: &str,
            now: Option<&IsoTimestamp>,
        ) -> CoreResult<Option<SimpleKvRecord>> {
            PostgresRuntimeCounterProofStore::get_simple_kv(self, scope, key, now)
        }

        fn list_simple_kv(&self, query: &SimpleKvQuery) -> CoreResult<Vec<SimpleKvRecord>> {
            PostgresRuntimeCounterProofStore::list_simple_kv(self, query)
        }

        fn delete_simple_kv(&self, delete: &SimpleKvDelete) -> CoreResult<SimpleKvRecord> {
            PostgresRuntimeCounterProofStore::delete_simple_kv(self, delete)
        }

        fn expire_simple_kv(&self, now: &IsoTimestamp) -> CoreResult<u64> {
            PostgresRuntimeCounterProofStore::expire_simple_kv(self, now)
        }
    }

    #[test]
    fn validates_schema_identifiers_before_connecting() {
        let error = match PostgresRuntimeCounterProofStore::connect(
            "postgres://example.invalid/db",
            "bad-schema",
        ) {
            Ok(_) => panic!("invalid schema unexpectedly connected"),
            Err(error) => error,
        };
        assert_eq!(error.kind, CoreErrorKind::InvalidInput);
    }

    #[test]
    fn postgres_proof_repository_groups_mark_unsupported_service_repositories() {
        let groups = postgres_proof_repository_groups();
        assert!(groups.iter().any(
            |group| group.group_id == "storage_admin" && group.notes[0].contains("implemented")
        ));
        assert!(groups
            .iter()
            .any(|group| group.group_id == "runtime_counters"
                && group.notes[0].contains("implemented")));
        assert!(groups
            .iter()
            .any(|group| group.group_id == "queues_messages"
                && group.notes[0].contains("unsupported")));
        assert!(groups
            .iter()
            .any(|group| group.group_id == "scheduler_jobs"
                && group.notes[0].contains("unsupported")));
        assert!(groups
            .iter()
            .any(|group| group.group_id == "module_schema_registry"
                && group.notes[0].contains("simple_kv")));
        assert!(groups.iter().any(|group| group.group_id == "runtime_search"
            && group.notes[0].contains("runtime search entries")));
    }

    #[test]
    fn sqlite_simple_kv_conformance_matches_postgres_proof_contract() {
        let db_path = temp_sqlite_path("sqlite-simple-kv-conformance");
        let store = CoordinationStore::open_file(&db_path).unwrap();
        simple_kv_conformance(&store);
        let _ = fs::remove_file(db_path);
    }

    #[test]
    #[ignore = "requires local PostgreSQL dev database env; source /home/system/database/rusty-crew-postgres.env or set RUSTY_CREW_DATABASE_URL"]
    fn postgres_runtime_counter_proof_matches_typed_counter_contract() {
        let Some(database_url) = postgres_test_database_url() else {
            eprintln!("skipping PostgreSQL proof; no database URL env is set");
            return;
        };
        let schema = unique_schema("rusty_crew_counter_proof");
        let store = PostgresRuntimeCounterProofStore::connect(&database_url, &schema).unwrap();

        store
            .increment_counter(
                &RuntimeCounterScope::Runtime,
                COUNTER_MESSAGES,
                2,
                &"2026-06-26T00:00:00Z".to_string(),
            )
            .unwrap();
        store
            .increment_counter(
                &RuntimeCounterScope::Session(crate::SessionId::new("session-alpha")),
                COUNTER_WAKES,
                1,
                &"2026-06-26T00:00:01Z".to_string(),
            )
            .unwrap();

        let runtime_summary = store
            .runtime_summary(&RuntimeCounterScope::Runtime)
            .unwrap();
        let session_summary = store
            .runtime_summary(&RuntimeCounterScope::Session(crate::SessionId::new(
                "session-alpha",
            )))
            .unwrap();
        assert_eq!(runtime_summary.messages, 2);
        assert_eq!(session_summary.wakes, 1);

        let messages = store
            .query_runtime_counters(&RuntimeCounterQuery {
                scope: Some(RuntimeCounterScope::Runtime),
                counter_name: Some(COUNTER_MESSAGES.to_string()),
                page: None,
            })
            .unwrap();
        assert_eq!(messages[0].value, 2);
        assert_eq!(
            store
                .reset_runtime_counters(
                    &RuntimeCounterQuery {
                        scope: Some(RuntimeCounterScope::Runtime),
                        counter_name: Some(COUNTER_MESSAGES.to_string()),
                        page: None,
                    },
                    "2026-06-26T00:00:02Z".to_string(),
                )
                .unwrap(),
            1
        );
        assert_eq!(
            store
                .runtime_summary(&RuntimeCounterScope::Runtime)
                .unwrap()
                .messages,
            0
        );

        let diagnostics = store.storage_diagnostics().unwrap();
        assert_eq!(diagnostics.backend, "postgres");
        assert_eq!(diagnostics.schema_version, POSTGRES_PROOF_SCHEMA_VERSION);
        assert_eq!(diagnostics.repository_groups[0].group_id, "storage_admin");
        assert_eq!(diagnostics.table_counts[0].rows, 2);

        store.drop_schema_for_test().unwrap();
    }

    #[test]
    #[ignore = "requires local PostgreSQL dev database env; source /home/system/database/rusty-crew-postgres.env or set RUSTY_CREW_DATABASE_URL"]
    fn postgres_simple_kv_proof_matches_sqlite_conformance_contract() {
        let Some(database_url) = postgres_test_database_url() else {
            eprintln!("skipping PostgreSQL simple_kv proof; no database URL env is set");
            return;
        };
        let schema = unique_schema("rusty_crew_simple_kv_proof");
        let store = PostgresRuntimeCounterProofStore::connect(&database_url, &schema).unwrap();
        simple_kv_conformance(&store);

        let diagnostics = store.storage_diagnostics().unwrap();
        assert_eq!(
            diagnostics
                .table_counts
                .iter()
                .find(|count| count.table == "module_simple_kv_entries")
                .map(|count| count.rows),
            Some(0)
        );
        assert!(diagnostics.repository_groups.iter().any(|group| {
            group.group_id == "module_schema_registry" && group.notes[0].contains("simple_kv")
        }));

        store.drop_schema_for_test().unwrap();
    }

    #[test]
    #[ignore = "requires local PostgreSQL dev database env; source /home/system/database/rusty-crew-postgres.env or set RUSTY_CREW_DATABASE_URL"]
    fn postgres_runtime_search_proof_matches_typed_search_contract() {
        let Some(database_url) = postgres_test_database_url() else {
            eprintln!("skipping PostgreSQL runtime search proof; no database URL env is set");
            return;
        };
        let schema = unique_schema("rusty_crew_runtime_search_proof");
        let store = PostgresRuntimeCounterProofStore::connect(&database_url, &schema).unwrap();
        runtime_search_conformance(&store);

        let diagnostics = store.storage_diagnostics().unwrap();
        assert_eq!(
            diagnostics
                .table_counts
                .iter()
                .find(|count| count.table == "runtime_search_entries")
                .map(|count| count.rows),
            Some(5)
        );
        assert!(diagnostics.capabilities.iter().any(|capability| {
            capability.name == "runtime_full_text_search" && capability.supported
        }));
        assert!(diagnostics.repository_groups.iter().any(|group| {
            group.group_id == "runtime_search" && group.notes[0].contains("implemented")
        }));

        store.drop_schema_for_test().unwrap();
    }

    fn simple_kv_conformance(store: &dyn SimpleKvConformanceStore) {
        let scope = SimpleKvScope {
            scope_type: "profile".to_string(),
            scope_id: "rusty-crew-runner".to_string(),
        };
        let first = store
            .put_simple_kv(&SimpleKvWrite {
                scope: scope.clone(),
                key: "tone".to_string(),
                value_json: json!({"style": "steady"}),
                now: "2026-06-26T00:00:00Z".to_string(),
                expires_at: None,
            })
            .unwrap();
        assert_eq!(first.revision, 1);
        assert_eq!(first.value_json, json!({"style": "steady"}));

        let fetched = store
            .get_simple_kv(&scope, "tone", Some(&"2026-06-26T00:01:00Z".to_string()))
            .unwrap()
            .unwrap();
        assert_eq!(fetched, first);

        let second = store
            .put_simple_kv(&SimpleKvWrite {
                scope: scope.clone(),
                key: "tone".to_string(),
                value_json: json!({"style": "direct"}),
                now: "2026-06-26T00:02:00Z".to_string(),
                expires_at: Some("2026-06-26T01:00:00Z".to_string()),
            })
            .unwrap();
        assert_eq!(second.revision, 2);
        assert_eq!(second.created_at, first.created_at);

        let stale = store
            .compare_and_swap_simple_kv(&SimpleKvCompareAndSwap {
                write: SimpleKvWrite {
                    scope: scope.clone(),
                    key: "tone".to_string(),
                    value_json: json!({"style": "stale"}),
                    now: "2026-06-26T00:03:00Z".to_string(),
                    expires_at: None,
                },
                expected_revision: 1,
            })
            .unwrap_err();
        assert_eq!(stale.kind, CoreErrorKind::ActionRejected);

        let third = store
            .compare_and_swap_simple_kv(&SimpleKvCompareAndSwap {
                write: SimpleKvWrite {
                    scope: scope.clone(),
                    key: "tone".to_string(),
                    value_json: json!({"style": "precise"}),
                    now: "2026-06-26T00:04:00Z".to_string(),
                    expires_at: Some("2026-06-26T00:05:00Z".to_string()),
                },
                expected_revision: 2,
            })
            .unwrap();
        assert_eq!(third.revision, 3);

        store
            .put_simple_kv(&SimpleKvWrite {
                scope: scope.clone(),
                key: "working_set".to_string(),
                value_json: json!(["a", "b"]),
                now: "2026-06-26T00:04:30Z".to_string(),
                expires_at: None,
            })
            .unwrap();

        let visible = store
            .list_simple_kv(&SimpleKvQuery {
                scope: scope.clone(),
                key_prefix: None,
                include_expired: false,
                expired_only: false,
                now: Some("2026-06-26T00:04:45Z".to_string()),
                page: None,
            })
            .unwrap();
        assert_eq!(
            visible
                .iter()
                .map(|record| record.key.as_str())
                .collect::<Vec<_>>(),
            vec!["tone", "working_set"]
        );

        let prefixed = store
            .list_simple_kv(&SimpleKvQuery {
                scope: scope.clone(),
                key_prefix: Some("work".to_string()),
                include_expired: false,
                expired_only: false,
                now: Some("2026-06-26T00:04:45Z".to_string()),
                page: None,
            })
            .unwrap();
        assert_eq!(prefixed.len(), 1);
        assert_eq!(prefixed[0].key, "working_set");

        assert!(store
            .get_simple_kv(&scope, "tone", Some(&"2026-06-26T00:05:01Z".to_string()))
            .unwrap()
            .is_none());
        let with_expired = store
            .list_simple_kv(&SimpleKvQuery {
                scope: scope.clone(),
                key_prefix: None,
                include_expired: true,
                expired_only: false,
                now: Some("2026-06-26T00:05:01Z".to_string()),
                page: None,
            })
            .unwrap();
        assert_eq!(with_expired.len(), 2);
        let expired_only = store
            .list_simple_kv(&SimpleKvQuery {
                scope: scope.clone(),
                key_prefix: None,
                include_expired: true,
                expired_only: true,
                now: Some("2026-06-26T00:05:01Z".to_string()),
                page: None,
            })
            .unwrap();
        assert_eq!(expired_only.len(), 1);
        assert_eq!(expired_only[0].key, "tone");

        assert_eq!(
            store
                .delete_simple_kv(&SimpleKvDelete {
                    scope: scope.clone(),
                    key: "working_set".to_string(),
                    expected_revision: 1,
                })
                .unwrap()
                .key,
            "working_set"
        );
        assert_eq!(
            store
                .expire_simple_kv(&"2026-06-26T00:05:01Z".to_string())
                .unwrap(),
            1
        );
        assert!(store
            .list_simple_kv(&SimpleKvQuery {
                scope,
                key_prefix: None,
                include_expired: true,
                expired_only: false,
                now: None,
                page: None,
            })
            .unwrap()
            .is_empty());
    }

    fn runtime_search_conformance(store: &PostgresRuntimeCounterProofStore) {
        for entry in runtime_search_fixture() {
            store.upsert_runtime_search_entry(&entry).unwrap();
        }

        let session = store
            .search_runtime(&RuntimeSearchFilter {
                query: "tools".to_string(),
                row_type: Some(RuntimeSearchRowType::Session),
                session_id: Some(crate::SessionId::new("session-alpha")),
                agent_id: None,
                instance_id: None,
                task_id: None,
                event_kind: Some(crate::CoreEventKind::SessionCreated),
                recorded_after: None,
                recorded_before: None,
                limit: Some(10),
            })
            .unwrap();
        assert_eq!(session.len(), 1);
        assert_eq!(session[0].row_type, RuntimeSearchRowType::Session);

        let beta_message = store
            .search_runtime(&RuntimeSearchFilter {
                query: "needle".to_string(),
                row_type: Some(RuntimeSearchRowType::Message),
                session_id: None,
                agent_id: Some(crate::AgentId::new("agent-beta")),
                instance_id: None,
                task_id: None,
                event_kind: Some(crate::CoreEventKind::AgentMessageRouted),
                recorded_after: Some("2026-06-26T00:00:00Z".to_string()),
                recorded_before: Some("2026-06-26T00:10:00Z".to_string()),
                limit: Some(10),
            })
            .unwrap();
        assert_eq!(beta_message.len(), 1);
        assert_eq!(beta_message[0].row_key, "message:1:agent-beta");
        assert_eq!(beta_message[0].sequence, Some(1));

        let queued = store
            .search_runtime(&RuntimeSearchFilter {
                query: "needle".to_string(),
                row_type: Some(RuntimeSearchRowType::QueueMessage),
                session_id: Some(crate::SessionId::new("session-alpha")),
                agent_id: Some(crate::AgentId::new("agent-alpha")),
                instance_id: None,
                task_id: None,
                event_kind: None,
                recorded_after: None,
                recorded_before: None,
                limit: Some(10),
            })
            .unwrap();
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].row_key, "queue-search-conformance");

        let stable_ties = store
            .search_runtime(&RuntimeSearchFilter {
                query: "tie".to_string(),
                row_type: Some(RuntimeSearchRowType::Message),
                session_id: None,
                agent_id: Some(crate::AgentId::new("agent-alpha")),
                instance_id: None,
                task_id: None,
                event_kind: Some(crate::CoreEventKind::AgentMessageRouted),
                recorded_after: None,
                recorded_before: None,
                limit: Some(1),
            })
            .unwrap();
        assert_eq!(stable_ties.len(), 1);
        assert_eq!(stable_ties[0].row_key, "message:2:agent-alpha");

        let empty_query = store
            .search_runtime(&RuntimeSearchFilter {
                query: "   ".to_string(),
                row_type: None,
                session_id: None,
                agent_id: None,
                instance_id: None,
                task_id: None,
                event_kind: None,
                recorded_after: None,
                recorded_before: None,
                limit: None,
            })
            .unwrap_err();
        assert_eq!(empty_query.kind, CoreErrorKind::InvalidInput);
    }

    fn runtime_search_fixture() -> Vec<RuntimeSearchResult> {
        vec![
            RuntimeSearchResult {
                row_type: RuntimeSearchRowType::Session,
                row_key: "session-alpha".to_string(),
                sequence: None,
                session_id: Some(crate::SessionId::new("session-alpha")),
                agent_id: Some(crate::AgentId::new("agent-alpha")),
                instance_id: Some(crate::AgentInstanceId::new("instance:session-alpha")),
                task_id: Some(crate::TaskId::new("task-search")),
                event_kind: Some(crate::CoreEventKind::SessionCreated),
                recorded_at: "2026-06-26T00:00:00Z".to_string(),
                title: "session session-alpha".to_string(),
                body: "agent agent-alpha profile runner kind full tools shell den".to_string(),
            },
            RuntimeSearchResult {
                row_type: RuntimeSearchRowType::Message,
                row_key: "message:1:agent-beta".to_string(),
                sequence: Some(1),
                session_id: None,
                agent_id: Some(crate::AgentId::new("agent-beta")),
                instance_id: None,
                task_id: None,
                event_kind: Some(crate::CoreEventKind::AgentMessageRouted),
                recorded_at: "2026-06-26T00:01:00Z".to_string(),
                title: "agent message".to_string(),
                body: "needle event search beta".to_string(),
            },
            RuntimeSearchResult {
                row_type: RuntimeSearchRowType::QueueMessage,
                row_key: "queue-search-conformance".to_string(),
                sequence: Some(1),
                session_id: Some(crate::SessionId::new("session-alpha")),
                agent_id: Some(crate::AgentId::new("agent-alpha")),
                instance_id: Some(crate::AgentInstanceId::new("instance:session-alpha")),
                task_id: None,
                event_kind: Some(crate::CoreEventKind::AgentMessageRouted),
                recorded_at: "2026-06-26T00:02:00Z".to_string(),
                title: "queued message pending".to_string(),
                body: "needle queue search".to_string(),
            },
            RuntimeSearchResult {
                row_type: RuntimeSearchRowType::Message,
                row_key: "message:2:agent-alpha".to_string(),
                sequence: Some(2),
                session_id: None,
                agent_id: Some(crate::AgentId::new("agent-alpha")),
                instance_id: None,
                task_id: None,
                event_kind: Some(crate::CoreEventKind::AgentMessageRouted),
                recorded_at: "2026-06-26T00:03:00Z".to_string(),
                title: "agent message".to_string(),
                body: "tie search alpha".to_string(),
            },
            RuntimeSearchResult {
                row_type: RuntimeSearchRowType::Message,
                row_key: "message:3:agent-alpha".to_string(),
                sequence: Some(3),
                session_id: None,
                agent_id: Some(crate::AgentId::new("agent-alpha")),
                instance_id: None,
                task_id: None,
                event_kind: Some(crate::CoreEventKind::AgentMessageRouted),
                recorded_at: "2026-06-26T00:04:00Z".to_string(),
                title: "agent message".to_string(),
                body: "tie search alpha".to_string(),
            },
        ]
    }

    fn postgres_test_database_url() -> Option<String> {
        for key in [
            "RUSTY_CREW_POSTGRES_PROOF_DATABASE_URL",
            "RUSTY_CREW_DATABASE_URL",
            "RUSTY_CREW_APP_DATABASE_URL",
        ] {
            if let Ok(value) = std::env::var(key) {
                if !value.trim().is_empty() {
                    return Some(value);
                }
            }
        }
        None
    }

    fn unique_schema(prefix: &str) -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after unix epoch")
            .as_nanos();
        format!("{prefix}_{}_{}", std::process::id(), nanos)
    }

    fn temp_sqlite_path(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("rusty_crew_{label}_{nanos}.sqlite3"))
    }
}
