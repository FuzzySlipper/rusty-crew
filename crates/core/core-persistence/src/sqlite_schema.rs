//! SQLite schema migration catalog, migration application, and diagnostics helpers.
//!
//! SQLite remains a first-class small/debug deployment backend. This module owns
//! schema-version checks, migration SQL, storage diagnostics, and module-schema
//! registry installation for that backend. Repository-domain SQL stays in the
//! repository modules or the store facade until later decomposition slices.

use super::*;

pub(crate) const CURRENT_SCHEMA_VERSION: i64 = 31;
const MIN_SUPPORTED_SCHEMA_VERSION: i64 = 1;
pub(crate) const SQLITE_BUSY_TIMEOUT_MS: u64 = 5_000;
pub(crate) const SQLITE_WAL_AUTOCHECKPOINT_PAGES: u32 = 1_000;

pub(crate) struct SchemaMigration {
    pub(crate) version: i64,
    pub(crate) description: &'static str,
    pub(crate) apply: fn(&rusqlite::Transaction<'_>) -> CoreResult<()>,
}

pub(crate) const SCHEMA_MIGRATIONS: &[SchemaMigration] = &[
    SchemaMigration {
        version: 1,
        description: "create base coordination tables",
        apply: migrate_v1_create_base_tables,
    },
    SchemaMigration {
        version: 2,
        description: "add delegation and fan-out coordination columns",
        apply: migrate_v2_add_delegation_columns,
    },
    SchemaMigration {
        version: 3,
        description: "add durable agent, instance, and session identity tables",
        apply: migrate_v3_add_identity_tables,
    },
    SchemaMigration {
        version: 4,
        description: "add immutable session configuration snapshots",
        apply: migrate_v4_add_session_config_snapshots,
    },
    SchemaMigration {
        version: 5,
        description: "add event-log query projection indexes",
        apply: migrate_v5_add_event_projection_indexes,
    },
    SchemaMigration {
        version: 6,
        description: "add FTS runtime search index",
        apply: migrate_v6_add_runtime_search_index,
    },
    SchemaMigration {
        version: 7,
        description: "add durable runtime counters",
        apply: repos::runtime_counters::migrate_v7_add_runtime_counters,
    },
    SchemaMigration {
        version: 8,
        description: "add queued message retention state",
        apply: repos::queued_messages::migrate_v8_add_queued_message_retention,
    },
    SchemaMigration {
        version: 9,
        description: "add scale guardrail indexes for runtime diagnostics",
        apply: migrate_v9_add_scale_guardrail_indexes,
    },
    SchemaMigration {
        version: 10,
        description: "add future legacy runtime import metadata",
        apply: migrate_v10_add_legacy_runtime_import_metadata,
    },
    SchemaMigration {
        version: 11,
        description: "add per-agent external channel and MCP bindings",
        apply: migrate_v11_add_external_bindings,
    },
    SchemaMigration {
        version: 12,
        description: "add tool call metadata audit column",
        apply: migrate_v12_add_tool_call_metadata,
    },
    SchemaMigration {
        version: 13,
        description: "add dense profile memory persistence",
        apply: migrate_v13_add_profile_memory,
    },
    SchemaMigration {
        version: 14,
        description: "add scheduler job and run persistence",
        apply: repos::scheduler::migrate_v14_add_scheduler_persistence,
    },
    SchemaMigration {
        version: 15,
        description: "add session history window persistence",
        apply: migrate_v15_add_session_history_window,
    },
    SchemaMigration {
        version: 16,
        description: "add provider wire-state persistence",
        apply: migrate_v16_add_provider_wire_state,
    },
    SchemaMigration {
        version: 17,
        description: "add message slot and variant persistence",
        apply: migrate_v17_add_message_slot_variants,
    },
    SchemaMigration {
        version: 18,
        description: "add conversation tree branches and snapshots",
        apply: migrate_v18_add_conversation_tree,
    },
    SchemaMigration {
        version: 19,
        description: "add generic chat attachments and data-bank scopes",
        apply: migrate_v19_add_chat_attachments,
    },
    SchemaMigration {
        version: 20,
        description: "add module schema installed-version registry",
        apply: migrate_v20_add_module_schema_registry,
    },
    SchemaMigration {
        version: 21,
        description: "add typed memory proposal governance storage",
        apply: migrate_v21_add_memory_proposal_governance,
    },
    SchemaMigration {
        version: 22,
        description: "add DB-backed active profile registry",
        apply: migrate_v22_add_profile_registry,
    },
    SchemaMigration {
        version: 23,
        description: "add session memory record persistence",
        apply: migrate_v23_add_session_memory_records,
    },
    SchemaMigration {
        version: 24,
        description: "add roleplay lore typed memory-space persistence",
        apply: migrate_v24_add_roleplay_lore_records,
    },
    SchemaMigration {
        version: 25,
        description: "add service-level model provider registry",
        apply: migrate_v25_add_model_provider_registry,
    },
    SchemaMigration {
        version: 26,
        description: "add roleplay lore layer and recall scaffolding",
        apply: migrate_v26_add_roleplay_lore_layers,
    },
    SchemaMigration {
        version: 27,
        description: "add DB-backed profile prompt text",
        apply: migrate_v27_add_profile_registry_prompt_text,
    },
    SchemaMigration {
        version: 28,
        description: "add session activity digest persistence",
        apply: migrate_v28_add_session_activity_digests,
    },
    SchemaMigration {
        version: 29,
        description: "add context compaction artifact persistence",
        apply: migrate_v29_add_context_compaction_artifacts,
    },
    SchemaMigration {
        version: 30,
        description: "add optional worker-pool capacity primitives",
        apply: migrate_v30_add_worker_pool_capacity,
    },
    SchemaMigration {
        version: 31,
        description: "link worker runs to optional worker-pool leases",
        apply: migrate_v31_add_worker_run_pool_provenance,
    },
];

pub(crate) fn prepare_migration_metadata(conn: &Connection) -> CoreResult<()> {
    conn.execute_batch(
        "
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                description TEXT NOT NULL DEFAULT '',
                applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            ",
    )
    .map_err(|error| persistence_error("prepare schema migration metadata", error))?;
    add_missing_column(
        conn,
        "schema_migrations",
        "description",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    reject_unsupported_unversioned_schema(conn)
}

pub(crate) fn database_size(conn: &Connection) -> CoreResult<RuntimeDatabaseSize> {
    let page_count = pragma_u64(conn, "page_count")?;
    let page_size_bytes = pragma_u64(conn, "page_size")?;
    let freelist_pages = pragma_u64(conn, "freelist_count")?;
    let database_bytes = page_count.saturating_mul(page_size_bytes);
    let freelist_bytes = freelist_pages.saturating_mul(page_size_bytes);
    let wal_bytes = database_path(conn)?
        .and_then(|path| fs::metadata(format!("{}-wal", path.display())).ok())
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    Ok(RuntimeDatabaseSize {
        database_bytes,
        page_count,
        page_size_bytes,
        freelist_pages,
        freelist_bytes,
        wal_bytes,
    })
}

const SQLITE_WAL_PRESSURE_BYTES: u64 = 64 * 1024 * 1024;
const SQLITE_FREELIST_PRESSURE_PERCENT: u64 = 25;
const SQLITE_ACTIVE_AGENT_WARNING_ROWS: u64 = 32;
const SQLITE_TRANSCRIPT_WARNING_ROWS: u64 = 64;
const SQLITE_MEMORY_LORE_WARNING_ROWS: u64 = 64;
const SQLITE_RUNTIME_SEARCH_WARNING_ROWS: u64 = 64;
const SQLITE_QUEUE_WARNING_ROWS: u64 = 32;
const SQLITE_SCHEDULER_WARNING_ROWS: u64 = 32;
const SQLITE_PROVIDER_STATE_WARNING_ROWS: u64 = 32;

pub(crate) fn sqlite_storage_pressure_signals(
    size: &RuntimeDatabaseSize,
    table_counts: &[RuntimeStorageTableCount],
    index_checks: &[RuntimeQueryPlanCheck],
    search_healthy: bool,
) -> Vec<RuntimeStoragePressureSignal> {
    let row_count = |table: &str| -> u64 {
        table_counts
            .iter()
            .find(|count| count.table == table)
            .map(|count| count.rows)
            .unwrap_or(0)
    };
    let summed_rows =
        |tables: &[&str]| -> u64 { tables.iter().map(|table| row_count(table)).sum() };
    let freelist_percent = size
        .freelist_bytes
        .saturating_mul(100)
        .checked_div(size.database_bytes)
        .unwrap_or(0);
    let failed_query_plans = index_checks
        .iter()
        .filter(|check| !check.uses_index)
        .count() as u64;
    let active_agents = row_count("agent_instances").max(row_count("sessions"));
    let transcript_rows = summed_rows(&[
        "messages",
        "message_slots",
        "message_variants",
        "message_blocks",
        "conversation_branches",
        "conversation_snapshots",
    ]);
    let memory_lore_rows = summed_rows(&[
        "profile_memories",
        "memory_proposals",
        "memory_governance_decisions",
        "data_bank_scopes",
        "attachments",
        "attachment_links",
    ]);
    let scheduler_rows = summed_rows(&["scheduled_jobs", "scheduled_job_runs"]);

    vec![
        storage_pressure_signal(
            "sqlite_wal_bytes",
            size.wal_bytes > SQLITE_WAL_PRESSURE_BYTES,
            "warning",
            size.wal_bytes,
            Some(SQLITE_WAL_PRESSURE_BYTES),
            "WAL growth above the checkpoint threshold suggests maintenance windows are not keeping up.",
        ),
        storage_pressure_signal(
            "sqlite_freelist_percent",
            size.database_bytes > 0 && freelist_percent > SQLITE_FREELIST_PRESSURE_PERCENT,
            "warning",
            freelist_percent,
            Some(SQLITE_FREELIST_PRESSURE_PERCENT),
            "Freelist pressure above 25% after retention suggests export/backup/VACUUM planning.",
        ),
        storage_pressure_signal(
            "sqlite_hot_query_plan_failures",
            failed_query_plans > 0,
            "critical",
            failed_query_plans,
            Some(0),
            "Hot diagnostic query plans should keep index coverage before load grows.",
        ),
        storage_pressure_signal(
            "runtime_search_health",
            !search_healthy,
            "critical",
            if search_healthy { 1 } else { 0 },
            Some(1),
            "Runtime search must remain healthy before transcript/lore/search rows grow.",
        ),
        storage_pressure_signal(
            "active_agent_count",
            active_agents > SQLITE_ACTIVE_AGENT_WARNING_ROWS,
            "warning",
            active_agents,
            Some(SQLITE_ACTIVE_AGENT_WARNING_ROWS),
            "Dozens of active agents increase wake, queue, scheduler, and writer contention pressure.",
        ),
        storage_pressure_signal(
            "conversation_transcript_growth",
            transcript_rows > SQLITE_TRANSCRIPT_WARNING_ROWS,
            "warning",
            transcript_rows,
            Some(SQLITE_TRANSCRIPT_WARNING_ROWS),
            "Large transcript trees are an early PostgreSQL pressure area for multi-user roleplay.",
        ),
        storage_pressure_signal(
            "memory_lore_growth",
            memory_lore_rows > SQLITE_MEMORY_LORE_WARNING_ROWS,
            "warning",
            memory_lore_rows,
            Some(SQLITE_MEMORY_LORE_WARNING_ROWS),
            "Dense memory, lore, attachments, and data-bank rows should stay visible before they become a separate store.",
        ),
        storage_pressure_signal(
            "runtime_search_growth",
            row_count("runtime_search_fts") > SQLITE_RUNTIME_SEARCH_WARNING_ROWS,
            "warning",
            row_count("runtime_search_fts"),
            Some(SQLITE_RUNTIME_SEARCH_WARNING_ROWS),
            "Search row growth is backend-sensitive because SQLite FTS5 and PostgreSQL search are not equivalent.",
        ),
        storage_pressure_signal(
            "queued_message_retention",
            row_count("queued_messages") > SQLITE_QUEUE_WARNING_ROWS,
            "warning",
            row_count("queued_messages"),
            Some(SQLITE_QUEUE_WARNING_ROWS),
            "Queued messages need aggressive TTL/no-resurrection checks when retention volume grows.",
        ),
        storage_pressure_signal(
            "scheduler_row_growth",
            scheduler_rows > SQLITE_SCHEDULER_WARNING_ROWS,
            "warning",
            scheduler_rows,
            Some(SQLITE_SCHEDULER_WARNING_ROWS),
            "Scheduler rows become correctness-sensitive once claims need multi-process concurrency semantics.",
        ),
        storage_pressure_signal(
            "provider_wire_state_growth",
            row_count("provider_wire_states") > SQLITE_PROVIDER_STATE_WARNING_ROWS,
            "warning",
            row_count("provider_wire_states"),
            Some(SQLITE_PROVIDER_STATE_WARNING_ROWS),
            "Provider wire state can hold large opaque payloads and should be monitored before it dominates local storage.",
        ),
        storage_pressure_signal(
            "single_service_writer_assumption",
            false,
            "info",
            1,
            Some(1),
            "SQLite remains the local default while one Rusty Crew service owns writes; independent writer processes should trigger PostgreSQL planning.",
        ),
    ]
}

fn storage_pressure_signal(
    name: &str,
    active: bool,
    severity: &str,
    observed_value: u64,
    threshold_value: Option<u64>,
    detail: &str,
) -> RuntimeStoragePressureSignal {
    RuntimeStoragePressureSignal {
        name: name.to_string(),
        active,
        severity: severity.to_string(),
        observed_value,
        threshold_value,
        detail: detail.to_string(),
    }
}

fn pragma_u64(conn: &Connection, name: &str) -> CoreResult<u64> {
    let value = conn
        .query_row(&format!("PRAGMA {name}"), [], |row| row.get::<_, i64>(0))
        .map_err(|error| persistence_error("read sqlite pragma", error))?;
    Ok(value as u64)
}

fn database_path(conn: &Connection) -> CoreResult<Option<PathBuf>> {
    let path = conn
        .query_row("PRAGMA database_list", [], |row| row.get::<_, String>(2))
        .map_err(|error| persistence_error("read sqlite database path", error))?;
    if path.is_empty() {
        Ok(None)
    } else {
        Ok(Some(PathBuf::from(path)))
    }
}

pub(crate) fn sqlite_table_exists(conn: &Connection, table: &str) -> CoreResult<bool> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type IN ('table', 'view') AND name = ?1)",
        params![table],
        |row| row.get::<_, i64>(0),
    )
    .map(|value| value != 0)
    .map_err(|error| persistence_error("check sqlite table existence", error))
}

pub(crate) fn count_diagnostic_table_rows(
    conn: &Connection,
    table: DiagnosticTable,
) -> CoreResult<u64> {
    let count = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM {}", table.as_str()),
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| persistence_error("count rows", error))?
        .unwrap_or(0);
    Ok(count as u64)
}

pub(crate) fn sqlite_storage_capabilities() -> Vec<RuntimeStorageCapability> {
    [
        (
            "transactions",
            true,
            "single-node ACID transactions are supported",
        ),
        (
            "runtime_full_text_search",
            true,
            "runtime search is backed by the service search capability",
        ),
        (
            "json_metadata",
            true,
            "JSON metadata is stored as validated text blobs",
        ),
        (
            "concurrent_writers",
            false,
            "SQLite serializes writers; WAL improves readers but not write concurrency",
        ),
        (
            "online_migrations",
            false,
            "schema migrations run during service startup/open",
        ),
        (
            "advisory_locks",
            false,
            "SQLite backend has no database-native advisory lock capability",
        ),
        (
            "maintenance_checkpoint",
            true,
            "SQLite WAL checkpoint maintenance is available",
        ),
        (
            "maintenance_vacuum_or_optimize",
            true,
            "SQLite PRAGMA optimize maintenance is available",
        ),
        (
            "estimated_table_size",
            true,
            "SQLite table row counts and database/page size diagnostics are available",
        ),
        (
            "query_plan_diagnostics",
            true,
            "SQLite EXPLAIN QUERY PLAN checks are available for hot diagnostic queries",
        ),
        (
            "row_level_claims",
            false,
            "SQLite claims are scoped to a single service process rather than database row locks",
        ),
        (
            "listen_notify",
            false,
            "SQLite backend has no database-native LISTEN/NOTIFY capability",
        ),
        (
            "logical_export_import",
            true,
            "logical export/import bundle contracts and dry-run validation are available; applying records remains repository-gated",
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

pub(crate) fn sqlite_module_schema_capabilities() -> Vec<ModuleSchemaCapability> {
    vec![
        ModuleSchemaCapability::Transactions,
        ModuleSchemaCapability::FullTextSearch,
        ModuleSchemaCapability::JsonDocuments,
    ]
}

#[cfg(feature = "postgres")]
pub(crate) fn postgres_module_schema_capabilities() -> Vec<ModuleSchemaCapability> {
    vec![
        ModuleSchemaCapability::Transactions,
        ModuleSchemaCapability::FullTextSearch,
        ModuleSchemaCapability::JsonDocuments,
    ]
}

pub(crate) fn hot_query_plan_checks(conn: &Connection) -> CoreResult<Vec<RuntimeQueryPlanCheck>> {
    const HOT_QUERIES: &[(&str, &str)] = &[
        (
            "pending_queue_by_agent",
            "SELECT message_id FROM queued_messages
             WHERE owner_agent_id = 'agent-alpha' AND state = 'pending'
             ORDER BY expires_at ASC LIMIT 10",
        ),
        (
            "worker_runs_by_parent_status",
            "SELECT run_id FROM worker_runs
             WHERE session_id = 'session-alpha' AND status = 'running'
             ORDER BY created_at ASC, run_id ASC LIMIT 10",
        ),
        (
            "messages_by_correlation",
            "SELECT sequence FROM agent_messages
             WHERE correlation_id = 'corr-alpha'
             ORDER BY sequence ASC LIMIT 10",
        ),
        (
            "completion_packets_by_session",
            "SELECT sequence FROM completion_packets
             WHERE session_id = 'session-alpha'
             ORDER BY sequence ASC LIMIT 10",
        ),
        (
            "event_session_lookup",
            "SELECT sequence FROM event_session_index
             WHERE session_id = 'session-alpha'
             ORDER BY sequence ASC LIMIT 10",
        ),
    ];

    HOT_QUERIES
        .iter()
        .map(|(name, sql)| query_plan_check(conn, name, sql))
        .collect()
}

fn query_plan_check(
    conn: &Connection,
    name: &'static str,
    sql: &str,
) -> CoreResult<RuntimeQueryPlanCheck> {
    let mut stmt = conn
        .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
        .map_err(|error| persistence_error("prepare hot query plan", error))?;
    let details = stmt
        .query_map([], |row| row.get::<_, String>(3))
        .map_err(|error| persistence_error("run hot query plan", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("read hot query plan", error))?;
    let detail = details.join(" | ");
    let uses_index = detail.contains("USING INDEX") || detail.contains("USING COVERING INDEX");
    Ok(RuntimeQueryPlanCheck {
        name,
        uses_index,
        detail,
    })
}

pub(crate) fn apply_schema_migrations(
    conn: &mut Connection,
    migrations: &[SchemaMigration],
) -> CoreResult<()> {
    validate_migration_catalog(migrations)?;
    let current_version = current_schema_version(conn)?;
    if current_version > CURRENT_SCHEMA_VERSION {
        return Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!(
                "sqlite schema version {current_version} is newer than supported version {CURRENT_SCHEMA_VERSION}"
            ),
        ));
    }
    if current_version > 0 && current_version < MIN_SUPPORTED_SCHEMA_VERSION {
        return Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!(
                "sqlite schema version {current_version} is older than supported version {MIN_SUPPORTED_SCHEMA_VERSION}"
            ),
        ));
    }

    for migration in migrations {
        if migration.version <= current_version {
            conn.execute(
                "UPDATE schema_migrations SET description = ?1 WHERE version = ?2",
                params![migration.description, migration.version],
            )
            .map_err(|error| persistence_error("refresh schema migration metadata", error))?;
            continue;
        }

        let tx = conn
            .transaction()
            .map_err(|error| persistence_error("start schema migration", error))?;
        (migration.apply)(&tx)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, description) VALUES (?1, ?2)",
            params![migration.version, migration.description],
        )
        .map_err(|error| persistence_error("record schema migration", error))?;
        tx.commit()
            .map_err(|error| persistence_error("commit schema migration", error))?;
    }
    Ok(())
}

fn validate_migration_catalog(migrations: &[SchemaMigration]) -> CoreResult<()> {
    for (index, migration) in migrations.iter().enumerate() {
        let expected = (index as i64) + 1;
        if migration.version != expected {
            return Err(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!(
                    "invalid schema migration catalog: expected version {expected}, found {}",
                    migration.version
                ),
            ));
        }
    }
    Ok(())
}

pub(crate) fn current_schema_version(conn: &Connection) -> CoreResult<i64> {
    conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get::<_, i64>(0),
    )
    .map_err(|error| persistence_error("read schema version", error))
}

pub(crate) fn load_schema_migration_records(
    conn: &Connection,
) -> CoreResult<Vec<SchemaMigrationRecord>> {
    let mut stmt = conn
        .prepare(
            "SELECT version, description, applied_at
             FROM schema_migrations
             ORDER BY version ASC",
        )
        .map_err(|error| persistence_error("prepare schema migration records", error))?;
    let rows = stmt
        .query_map([], |row| {
            Ok(SchemaMigrationRecord {
                version: row.get(0)?,
                description: row.get(1)?,
                applied_at: row.get(2)?,
            })
        })
        .map_err(|error| persistence_error("query schema migration records", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load schema migration records", error))
}

fn reject_unsupported_unversioned_schema(conn: &Connection) -> CoreResult<()> {
    let has_runtime_tables = runtime_table_names(conn)?.iter().any(|table| {
        matches!(
            table.as_str(),
            "sessions"
                | "agents"
                | "agent_instances"
                | "session_configs"
                | "session_identity"
                | "event_history"
                | "agent_messages"
                | "worker_runs"
                | "completion_packets"
                | "tool_call_history"
                | "runtime_counters"
                | "queued_messages"
                | "runtime_search_fts"
                | "runtime_import_batches"
                | "legacy_id_mappings"
                | "profile_registry"
                | "profile_memories"
                | "provider_wire_states"
                | "channel_bindings"
                | "mcp_bindings"
        )
    });
    let has_migration_records = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM schema_migrations)",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| persistence_error("check schema migration records", error))?
        != 0;

    if has_runtime_tables && !has_migration_records {
        return Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            "unsupported unversioned sqlite coordination schema",
        ));
    }
    Ok(())
}

fn runtime_table_names(conn: &Connection) -> CoreResult<Vec<String>> {
    let mut stmt = conn
        .prepare(
            "SELECT name FROM sqlite_schema
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
             ORDER BY name ASC",
        )
        .map_err(|error| persistence_error("prepare sqlite table names", error))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| persistence_error("query sqlite table names", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("read sqlite table names", error))
}

fn migrate_v1_create_base_tables(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "

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

            CREATE TABLE IF NOT EXISTS tool_call_history (
                sequence INTEGER PRIMARY KEY,
                session_id TEXT NOT NULL,
                wake_id TEXT,
                tool_name TEXT NOT NULL,
                phase TEXT NOT NULL,
                is_error INTEGER,
                metadata_json TEXT
            );
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 1", error))
}

fn migrate_v2_add_delegation_columns(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    add_missing_column(tx, "sessions", "delegation_json", "TEXT")?;
    add_missing_column(tx, "sessions", "resource_limits_json", "TEXT")?;
    add_missing_column(tx, "sessions", "tool_profile_json", "TEXT")?;
    add_missing_column(tx, "worker_runs", "delegated_session_id", "TEXT")?;
    add_missing_column(tx, "worker_runs", "parent_agent_id", "TEXT")?;
    add_missing_column(tx, "worker_runs", "delegation_correlation_id", "TEXT")?;
    add_missing_column(
        tx,
        "worker_runs",
        "parent_consumption",
        "TEXT NOT NULL DEFAULT 'await_completion'",
    )?;
    add_missing_column(tx, "worker_runs", "fan_out_group_id", "TEXT")?;
    add_missing_column(tx, "worker_runs", "fan_out_max_concurrency", "INTEGER")?;
    add_missing_column(
        tx,
        "worker_runs",
        "fan_out_failure_policy",
        "TEXT NOT NULL DEFAULT 'fail_soft'",
    )
}

fn migrate_v3_add_identity_tables(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS agents (
                agent_id TEXT PRIMARY KEY,
                display_label TEXT NOT NULL,
                profile_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                status TEXT NOT NULL,
                source_system TEXT,
                source_external_id TEXT,
                den_project_id TEXT,
                den_task_id TEXT,
                created_at TEXT NOT NULL,
                archived_at TEXT
            );

            CREATE TABLE IF NOT EXISTS agent_instances (
                instance_id TEXT PRIMARY KEY,
                agent_id TEXT NOT NULL,
                display_label TEXT NOT NULL,
                profile_id TEXT NOT NULL,
                status TEXT NOT NULL,
                source_system TEXT,
                source_external_id TEXT,
                den_project_id TEXT,
                den_task_id TEXT,
                created_at TEXT NOT NULL,
                last_active_at TEXT NOT NULL,
                archived_at TEXT
            );

            CREATE TABLE IF NOT EXISTS session_identity (
                session_id TEXT PRIMARY KEY,
                instance_id TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                profile_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                status TEXT NOT NULL,
                source_system TEXT,
                source_external_id TEXT,
                den_project_id TEXT,
                den_task_id TEXT,
                created_at TEXT NOT NULL,
                last_active_at TEXT NOT NULL,
                archived_at TEXT
            );
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 3", error))
}

fn migrate_v4_add_session_config_snapshots(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS session_configs (
                session_id TEXT PRIMARY KEY,
                profile_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                resource_limits_json TEXT NOT NULL,
                tool_profile_json TEXT NOT NULL,
                config_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 4", error))
}

fn migrate_v5_add_event_projection_indexes(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    add_missing_column(
        tx,
        "event_history",
        "recorded_at",
        "TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP",
    )?;
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS event_session_index (
                sequence INTEGER NOT NULL,
                session_id TEXT NOT NULL,
                PRIMARY KEY (sequence, session_id)
            );
            CREATE INDEX IF NOT EXISTS idx_event_session_index_session
                ON event_session_index(session_id, sequence);

            CREATE TABLE IF NOT EXISTS event_agent_index (
                sequence INTEGER NOT NULL,
                agent_id TEXT NOT NULL,
                PRIMARY KEY (sequence, agent_id)
            );
            CREATE INDEX IF NOT EXISTS idx_event_agent_index_agent
                ON event_agent_index(agent_id, sequence);

            CREATE TABLE IF NOT EXISTS event_instance_index (
                sequence INTEGER NOT NULL,
                instance_id TEXT NOT NULL,
                PRIMARY KEY (sequence, instance_id)
            );
            CREATE INDEX IF NOT EXISTS idx_event_instance_index_instance
                ON event_instance_index(instance_id, sequence);

            CREATE TABLE IF NOT EXISTS event_correlation_index (
                sequence INTEGER NOT NULL,
                correlation_id TEXT NOT NULL,
                PRIMARY KEY (sequence, correlation_id)
            );
            CREATE INDEX IF NOT EXISTS idx_event_correlation_index_correlation
                ON event_correlation_index(correlation_id, sequence);

            CREATE TABLE IF NOT EXISTS event_wake_index (
                sequence INTEGER NOT NULL,
                source_wake_id TEXT NOT NULL,
                PRIMARY KEY (sequence, source_wake_id)
            );
            CREATE INDEX IF NOT EXISTS idx_event_wake_index_wake
                ON event_wake_index(source_wake_id, sequence);
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 5", error))
}

fn migrate_v6_add_runtime_search_index(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE VIRTUAL TABLE IF NOT EXISTS runtime_search_fts USING fts5(
                row_type UNINDEXED,
                row_key UNINDEXED,
                sequence UNINDEXED,
                session_id UNINDEXED,
                agent_id UNINDEXED,
                instance_id UNINDEXED,
                task_id UNINDEXED,
                event_kind UNINDEXED,
                recorded_at UNINDEXED,
                title,
                body
            );
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 6", error))
}

fn migrate_v9_add_scale_guardrail_indexes(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE INDEX IF NOT EXISTS idx_sessions_agent_profile_handle
                ON sessions(agent_id, profile_id, handle);
            CREATE INDEX IF NOT EXISTS idx_sessions_profile_handle
                ON sessions(profile_id, handle);
            CREATE INDEX IF NOT EXISTS idx_agent_instances_agent_status
                ON agent_instances(agent_id, status, instance_id);
            CREATE INDEX IF NOT EXISTS idx_agent_messages_from_sequence
                ON agent_messages(from_agent, sequence);
            CREATE INDEX IF NOT EXISTS idx_agent_messages_to_sequence
                ON agent_messages(to_agent, sequence);
            CREATE INDEX IF NOT EXISTS idx_agent_messages_correlation_sequence
                ON agent_messages(correlation_id, sequence);
            CREATE INDEX IF NOT EXISTS idx_completion_packets_session_sequence
                ON completion_packets(session_id, sequence);
            CREATE INDEX IF NOT EXISTS idx_worker_runs_parent_status_created
                ON worker_runs(session_id, status, created_at, run_id);
            CREATE INDEX IF NOT EXISTS idx_worker_runs_delegated_session
                ON worker_runs(delegated_session_id);
            CREATE INDEX IF NOT EXISTS idx_worker_runs_profile_task_created
                ON worker_runs(profile_id, task_id, created_at, run_id);
            CREATE INDEX IF NOT EXISTS idx_tool_call_history_session_sequence
                ON tool_call_history(session_id, sequence);
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 9", error))
}

fn migrate_v10_add_legacy_runtime_import_metadata(
    tx: &rusqlite::Transaction<'_>,
) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS runtime_import_batches (
                import_batch_id TEXT PRIMARY KEY,
                source_system TEXT NOT NULL,
                source_label TEXT NOT NULL,
                source_snapshot_ref TEXT,
                notes TEXT,
                imported_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_runtime_import_batches_source
                ON runtime_import_batches(source_system, imported_at);

            CREATE TABLE IF NOT EXISTS legacy_id_mappings (
                import_batch_id TEXT NOT NULL,
                source_system TEXT NOT NULL,
                legacy_kind TEXT NOT NULL,
                legacy_id TEXT NOT NULL,
                rusty_kind TEXT NOT NULL,
                rusty_id TEXT NOT NULL,
                provenance_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                PRIMARY KEY (source_system, legacy_kind, legacy_id),
                FOREIGN KEY (import_batch_id)
                    REFERENCES runtime_import_batches(import_batch_id)
            );
            CREATE INDEX IF NOT EXISTS idx_legacy_id_mappings_batch
                ON legacy_id_mappings(import_batch_id, legacy_kind);
            CREATE INDEX IF NOT EXISTS idx_legacy_id_mappings_rusty
                ON legacy_id_mappings(rusty_kind, rusty_id);
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 10", error))
}

fn migrate_v11_add_external_bindings(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS channel_bindings (
                binding_id TEXT PRIMARY KEY,
                adapter_id TEXT NOT NULL,
                provider TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                instance_id TEXT,
                session_id TEXT,
                profile_id TEXT NOT NULL,
                external_channel_id TEXT NOT NULL,
                external_thread_id TEXT,
                external_user_id TEXT,
                provider_subscription_id TEXT,
                cursor TEXT,
                membership_state TEXT,
                presence_state TEXT,
                status TEXT NOT NULL,
                degraded_reason TEXT,
                provenance_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_channel_bindings_agent_provider
                ON channel_bindings(agent_id, provider, status);
            CREATE INDEX IF NOT EXISTS idx_channel_bindings_profile_agent
                ON channel_bindings(profile_id, agent_id, status);
            CREATE INDEX IF NOT EXISTS idx_channel_bindings_session
                ON channel_bindings(session_id, status);
            CREATE INDEX IF NOT EXISTS idx_channel_bindings_external
                ON channel_bindings(provider, external_channel_id, external_thread_id);

            CREATE TABLE IF NOT EXISTS mcp_bindings (
                binding_id TEXT PRIMARY KEY,
                adapter_id TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                instance_id TEXT,
                session_id TEXT,
                profile_id TEXT NOT NULL,
                server_names_json TEXT NOT NULL,
                endpoint_ref TEXT NOT NULL,
                transport TEXT NOT NULL,
                tool_profile_key TEXT NOT NULL,
                discovered_tool_revision TEXT,
                status TEXT NOT NULL,
                degraded_reason TEXT,
                diagnostics_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_mcp_bindings_agent_profile
                ON mcp_bindings(agent_id, profile_id, status);
            CREATE INDEX IF NOT EXISTS idx_mcp_bindings_session
                ON mcp_bindings(session_id, status);
            CREATE INDEX IF NOT EXISTS idx_mcp_bindings_adapter
                ON mcp_bindings(adapter_id, status);
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 11", error))
}

fn migrate_v12_add_tool_call_metadata(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    add_missing_column(tx, "tool_call_history", "metadata_json", "TEXT")
}

fn migrate_v13_add_profile_memory(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS profile_memories (
                profile_id TEXT NOT NULL,
                target_type TEXT NOT NULL,
                target_id TEXT NOT NULL,
                memory_key TEXT NOT NULL,
                content TEXT NOT NULL,
                metadata_json TEXT NOT NULL,
                revision INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (profile_id, target_type, target_id, memory_key)
            );
            CREATE INDEX IF NOT EXISTS idx_profile_memories_profile_updated
                ON profile_memories(profile_id, updated_at DESC);
            CREATE INDEX IF NOT EXISTS idx_profile_memories_target
                ON profile_memories(profile_id, target_type, target_id, memory_key);
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 13", error))
}

fn migrate_v15_add_session_history_window(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    add_missing_column(tx, "sessions", "history_window_json", "TEXT")
}

fn migrate_v16_add_provider_wire_state(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS provider_wire_states (
                row_id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                module_id TEXT NOT NULL,
                strategy_id TEXT NOT NULL,
                profile_fingerprint TEXT NOT NULL,
                provider_fingerprint TEXT NOT NULL,
                payload_version TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                payload_encoding TEXT NOT NULL DEFAULT 'json',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                expires_at TEXT,
                last_wake_id TEXT,
                invalidated_at TEXT,
                invalidation_reason TEXT
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_provider_wire_states_current
                ON provider_wire_states(session_id, module_id, strategy_id)
                WHERE invalidated_at IS NULL;
            CREATE INDEX IF NOT EXISTS idx_provider_wire_states_session_current
                ON provider_wire_states(session_id, invalidated_at);
            CREATE INDEX IF NOT EXISTS idx_provider_wire_states_expiry
                ON provider_wire_states(invalidated_at, expires_at);
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 16", error))
}

fn migrate_v17_add_message_slot_variants(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS message_slots (
                slot_id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                primary_variant_id TEXT NOT NULL,
                active_variant_id TEXT,
                metadata_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                version INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_message_slots_session_slot
                ON message_slots(session_id, slot_id);
            CREATE INDEX IF NOT EXISTS idx_message_slots_active_variant
                ON message_slots(active_variant_id);

            CREATE TABLE IF NOT EXISTS messages (
                message_id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                author_id TEXT NOT NULL,
                author_role TEXT NOT NULL,
                status TEXT NOT NULL,
                body TEXT NOT NULL,
                metadata_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_messages_session_created
                ON messages(session_id, created_at, message_id);

            CREATE TABLE IF NOT EXISTS message_blocks (
                block_id TEXT PRIMARY KEY,
                message_id TEXT NOT NULL,
                ordinal INTEGER NOT NULL,
                kind TEXT NOT NULL,
                content_json TEXT NOT NULL,
                render_policy_json TEXT,
                metadata_json TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_message_blocks_message_ordinal
                ON message_blocks(message_id, ordinal);

            CREATE TABLE IF NOT EXISTS message_variants (
                variant_id TEXT PRIMARY KEY,
                slot_id TEXT NOT NULL,
                source TEXT NOT NULL,
                ordinal INTEGER NOT NULL,
                status TEXT NOT NULL,
                message_id TEXT NOT NULL,
                metadata_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                FOREIGN KEY (slot_id) REFERENCES message_slots(slot_id),
                FOREIGN KEY (message_id) REFERENCES messages(message_id)
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_message_variants_slot_ordinal
                ON message_variants(slot_id, ordinal);
            CREATE INDEX IF NOT EXISTS idx_message_variants_slot_status
                ON message_variants(slot_id, status, ordinal);
            CREATE INDEX IF NOT EXISTS idx_message_variants_message
                ON message_variants(message_id);
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 17", error))
}

fn migrate_v18_add_conversation_tree(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    add_missing_column_tx(tx, "messages", "branch_id", "TEXT")?;
    add_missing_column_tx(tx, "messages", "parent_message_id", "TEXT")?;
    add_missing_column_tx(tx, "messages", "previous_message_id", "TEXT")?;
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS conversation_branches (
                branch_id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                parent_branch_id TEXT,
                parent_message_id TEXT,
                origin_message_id TEXT,
                head_message_id TEXT,
                label TEXT,
                metadata_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                version INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_conversation_branches_session_branch
                ON conversation_branches(session_id, branch_id);
            CREATE INDEX IF NOT EXISTS idx_conversation_branches_parent_branch
                ON conversation_branches(parent_branch_id);
            CREATE INDEX IF NOT EXISTS idx_conversation_branches_parent_message
                ON conversation_branches(parent_message_id);
            CREATE INDEX IF NOT EXISTS idx_conversation_branches_session_created
                ON conversation_branches(session_id, created_at, branch_id);

            CREATE TABLE IF NOT EXISTS conversation_branch_state (
                session_id TEXT PRIMARY KEY,
                active_branch_id TEXT,
                updated_at TEXT NOT NULL,
                version INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS conversation_snapshots (
                snapshot_id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                branch_id TEXT,
                message_id TEXT,
                cursor TEXT,
                label TEXT,
                summary TEXT,
                source TEXT NOT NULL,
                metadata_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_conversation_snapshots_session_message
                ON conversation_snapshots(session_id, message_id);
            CREATE INDEX IF NOT EXISTS idx_conversation_snapshots_session_branch
                ON conversation_snapshots(session_id, branch_id, created_at);
            CREATE INDEX IF NOT EXISTS idx_conversation_snapshots_session_created
                ON conversation_snapshots(session_id, created_at, snapshot_id);

            CREATE INDEX IF NOT EXISTS idx_messages_session_branch
                ON messages(session_id, branch_id);
            CREATE INDEX IF NOT EXISTS idx_messages_parent_message
                ON messages(parent_message_id);
            CREATE INDEX IF NOT EXISTS idx_messages_branch_created
                ON messages(branch_id, created_at, message_id);
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 18", error))
}

fn migrate_v19_add_chat_attachments(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS attachments (
                attachment_id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                status TEXT NOT NULL,
                filename TEXT NOT NULL,
                mime_type TEXT NOT NULL,
                byte_size INTEGER NOT NULL,
                storage_url TEXT,
                download_url TEXT,
                thumbnail_url TEXT,
                extracted_text TEXT,
                extracted_text_truncated INTEGER NOT NULL DEFAULT 0,
                metadata_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                expires_at TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_attachments_session_status
                ON attachments(session_id, status, created_at, attachment_id);
            CREATE INDEX IF NOT EXISTS idx_attachments_expiry
                ON attachments(expires_at);

            CREATE TABLE IF NOT EXISTS attachment_links (
                link_id TEXT PRIMARY KEY,
                attachment_id TEXT NOT NULL,
                session_id TEXT NOT NULL,
                message_id TEXT,
                block_id TEXT,
                scope_id TEXT,
                metadata_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY (attachment_id) REFERENCES attachments(attachment_id)
            );
            CREATE INDEX IF NOT EXISTS idx_attachment_links_attachment
                ON attachment_links(attachment_id, created_at, link_id);
            CREATE INDEX IF NOT EXISTS idx_attachment_links_session_message
                ON attachment_links(session_id, message_id);
            CREATE INDEX IF NOT EXISTS idx_attachment_links_session_scope
                ON attachment_links(session_id, scope_id);

            CREATE TABLE IF NOT EXISTS data_bank_scopes (
                scope_id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                status TEXT NOT NULL,
                label TEXT,
                description TEXT,
                metadata_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_data_bank_scopes_session_status
                ON data_bank_scopes(session_id, status, created_at, scope_id);
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 19", error))
}

fn migrate_v20_add_module_schema_registry(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS module_schema_versions (
                module_id TEXT PRIMARY KEY,
                installed_version INTEGER NOT NULL,
                descriptor_fingerprint TEXT NOT NULL,
                installed_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_module_schema_versions_version
                ON module_schema_versions(installed_version, module_id);
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 20", error))
}

fn migrate_v21_add_memory_proposal_governance(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS memory_proposals (
                proposal_id TEXT PRIMARY KEY,
                space_id TEXT NOT NULL,
                operation TEXT NOT NULL,
                scope_type TEXT NOT NULL,
                scope_id TEXT NOT NULL,
                shape_id TEXT NOT NULL,
                shape_version INTEGER NOT NULL,
                envelope_json TEXT NOT NULL,
                status TEXT NOT NULL,
                selected_governance_mode TEXT NOT NULL,
                source TEXT NOT NULL,
                dedupe_key TEXT,
                duplicate_of TEXT,
                resulting_revision INTEGER,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                decided_at TEXT,
                applied_at TEXT
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_memory_proposals_dedupe
                ON memory_proposals(space_id, dedupe_key)
                WHERE dedupe_key IS NOT NULL;
            CREATE INDEX IF NOT EXISTS idx_memory_proposals_status
                ON memory_proposals(status, updated_at DESC, proposal_id);
            CREATE INDEX IF NOT EXISTS idx_memory_proposals_space_status
                ON memory_proposals(space_id, status, updated_at DESC, proposal_id);

            CREATE TABLE IF NOT EXISTS memory_governance_decisions (
                decision_id TEXT PRIMARY KEY,
                proposal_id TEXT NOT NULL,
                decision TEXT NOT NULL,
                actor TEXT NOT NULL,
                source TEXT NOT NULL,
                evidence_refs_json TEXT NOT NULL,
                policy_mode TEXT NOT NULL,
                confidence REAL,
                message TEXT,
                resulting_revision INTEGER,
                decided_at TEXT NOT NULL,
                FOREIGN KEY (proposal_id) REFERENCES memory_proposals(proposal_id)
            );
            CREATE INDEX IF NOT EXISTS idx_memory_governance_decisions_proposal
                ON memory_governance_decisions(proposal_id, decided_at, decision_id);
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 21", error))
}

fn migrate_v22_add_profile_registry(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS profile_registry (
                profile_id TEXT PRIMARY KEY,
                lifecycle_status TEXT NOT NULL,
                display_name TEXT,
                summary TEXT,
                default_session_kind TEXT,
                agent_id TEXT,
                owner_id TEXT,
                prompt_soul_markdown TEXT,
                prompt_memory_markdown TEXT,
                active_runtime_settings_json TEXT NOT NULL,
                source_asset_refs_json TEXT NOT NULL,
                derived_runtime_refs_json TEXT NOT NULL,
                import_export_json TEXT NOT NULL,
                revision INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_profile_registry_lifecycle
                ON profile_registry(lifecycle_status, updated_at DESC, profile_id);
            CREATE INDEX IF NOT EXISTS idx_profile_registry_updated
                ON profile_registry(updated_at DESC, profile_id);
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 22", error))
}

fn migrate_v27_add_profile_registry_prompt_text(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    add_missing_column_tx(tx, "profile_registry", "prompt_soul_markdown", "TEXT")?;
    add_missing_column_tx(tx, "profile_registry", "prompt_memory_markdown", "TEXT")
}

fn migrate_v28_add_session_activity_digests(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS session_activity_digests (
                digest_id TEXT PRIMARY KEY,
                profile_id TEXT NOT NULL,
                session_id TEXT NOT NULL,
                wake_id TEXT NOT NULL,
                source TEXT NOT NULL,
                summary_text TEXT NOT NULL,
                event_counts_json TEXT NOT NULL,
                tool_calls_json TEXT NOT NULL,
                signals_json TEXT NOT NULL,
                completion_summary TEXT,
                allowed_capture_spaces_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                retention_until TEXT,
                reviewed_at TEXT
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_session_activity_digests_wake
                ON session_activity_digests(profile_id, session_id, wake_id);
            CREATE INDEX IF NOT EXISTS idx_session_activity_digests_profile_review
                ON session_activity_digests(profile_id, reviewed_at, created_at DESC, digest_id);
            CREATE INDEX IF NOT EXISTS idx_session_activity_digests_session
                ON session_activity_digests(session_id, created_at DESC, digest_id);
        ",
    )
    .map_err(|error| persistence_error("apply schema migration 28", error))
}

fn migrate_v29_add_context_compaction_artifacts(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS context_compaction_artifacts (
                artifact_id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                branch_id TEXT,
                strategy_id TEXT NOT NULL,
                source_refs_json TEXT NOT NULL,
                provider_metadata_json TEXT NOT NULL,
                estimate_before_json TEXT NOT NULL,
                estimate_after_json TEXT,
                summary_text TEXT NOT NULL,
                enters_future_context INTEGER NOT NULL,
                context_policy TEXT NOT NULL,
                metadata_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_context_compaction_session_latest
                ON context_compaction_artifacts(session_id, created_at DESC, artifact_id);
            CREATE INDEX IF NOT EXISTS idx_context_compaction_branch_latest
                ON context_compaction_artifacts(session_id, branch_id, created_at DESC, artifact_id);
            CREATE INDEX IF NOT EXISTS idx_context_compaction_strategy_latest
                ON context_compaction_artifacts(session_id, strategy_id, created_at DESC, artifact_id);
            CREATE INDEX IF NOT EXISTS idx_context_compaction_future_context
                ON context_compaction_artifacts(session_id, enters_future_context, created_at DESC, artifact_id);
        ",
    )
    .map_err(|error| persistence_error("apply schema migration 29", error))
}

fn migrate_v30_add_worker_pool_capacity(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS worker_pool_members (
                member_id TEXT PRIMARY KEY,
                profile_id TEXT NOT NULL,
                agent_id TEXT,
                session_id TEXT,
                status TEXT NOT NULL,
                concurrency_limit INTEGER NOT NULL CHECK (concurrency_limit > 0),
                active_leases INTEGER NOT NULL DEFAULT 0 CHECK (active_leases >= 0),
                capabilities_json TEXT NOT NULL,
                registered_at TEXT NOT NULL,
                last_heartbeat_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_worker_pool_members_status_heartbeat
                ON worker_pool_members(status, last_heartbeat_at, member_id);
            CREATE INDEX IF NOT EXISTS idx_worker_pool_members_profile_status
                ON worker_pool_members(profile_id, status, member_id);

            CREATE TABLE IF NOT EXISTS worker_pool_work_items (
                work_item_id TEXT PRIMARY KEY,
                requested_profile_id TEXT,
                task_id TEXT,
                status TEXT NOT NULL,
                priority INTEGER NOT NULL DEFAULT 100,
                work_json TEXT NOT NULL,
                required_capabilities_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                claimed_by_member_id TEXT,
                lease_id TEXT,
                claim_token TEXT,
                claim_deadline_at TEXT,
                terminal_at TEXT,
                terminal_summary TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_worker_pool_work_items_pending
                ON worker_pool_work_items(status, priority, created_at, work_item_id);
            CREATE INDEX IF NOT EXISTS idx_worker_pool_work_items_claim_deadline
                ON worker_pool_work_items(status, claim_deadline_at, work_item_id);
            CREATE INDEX IF NOT EXISTS idx_worker_pool_work_items_member_status
                ON worker_pool_work_items(claimed_by_member_id, status, work_item_id);

            CREATE TABLE IF NOT EXISTS worker_pool_leases (
                lease_id TEXT PRIMARY KEY,
                work_item_id TEXT NOT NULL,
                member_id TEXT NOT NULL,
                claim_token TEXT NOT NULL,
                status TEXT NOT NULL,
                claimed_at TEXT NOT NULL,
                claim_deadline_at TEXT NOT NULL,
                terminal_at TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_worker_pool_leases_member_status
                ON worker_pool_leases(member_id, status, claimed_at, lease_id);
            CREATE INDEX IF NOT EXISTS idx_worker_pool_leases_work_item
                ON worker_pool_leases(work_item_id, status, lease_id);

            CREATE TABLE IF NOT EXISTS worker_pool_events (
                sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                work_item_id TEXT,
                lease_id TEXT,
                member_id TEXT,
                event_type TEXT NOT NULL,
                event_json TEXT NOT NULL,
                recorded_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_worker_pool_events_work_item
                ON worker_pool_events(work_item_id, sequence);
            CREATE INDEX IF NOT EXISTS idx_worker_pool_events_member
                ON worker_pool_events(member_id, sequence);
        ",
    )
    .map_err(|error| persistence_error("apply schema migration 30", error))
}

fn migrate_v31_add_worker_run_pool_provenance(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    add_missing_column_tx(tx, "worker_runs", "worker_pool_work_item_id", "TEXT")?;
    add_missing_column_tx(tx, "worker_runs", "worker_pool_lease_id", "TEXT")?;
    add_missing_column_tx(tx, "worker_runs", "worker_pool_member_id", "TEXT")?;
    add_missing_column_tx(tx, "worker_runs", "worker_pool_claim_token", "TEXT")?;
    tx.execute_batch(
        "
            CREATE INDEX IF NOT EXISTS idx_worker_runs_pool_lease
                ON worker_runs(worker_pool_lease_id);
            CREATE INDEX IF NOT EXISTS idx_worker_runs_pool_member
                ON worker_runs(worker_pool_member_id, status);
        ",
    )
    .map_err(|error| persistence_error("apply schema migration 31", error))
}

fn migrate_v23_add_session_memory_records(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS session_memory_records (
                record_id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                scope_type TEXT NOT NULL,
                scope_id TEXT NOT NULL,
                branch_id TEXT,
                shape_id TEXT NOT NULL,
                shape_version INTEGER NOT NULL,
                status TEXT NOT NULL,
                revision INTEGER NOT NULL,
                content_json TEXT NOT NULL,
                evidence_refs_json TEXT NOT NULL,
                source TEXT NOT NULL,
                confidence REAL NOT NULL,
                durability_rationale TEXT NOT NULL,
                supersedes_record_id TEXT,
                superseded_by_record_id TEXT,
                archived_at TEXT,
                archive_reason TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                FOREIGN KEY (session_id) REFERENCES sessions(session_id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_session_memory_session_status_updated
                ON session_memory_records(session_id, status, updated_at DESC, record_id);
            CREATE INDEX IF NOT EXISTS idx_session_memory_branch_status_updated
                ON session_memory_records(branch_id, status, updated_at DESC, record_id)
                WHERE branch_id IS NOT NULL;
            CREATE INDEX IF NOT EXISTS idx_session_memory_scope
                ON session_memory_records(scope_type, scope_id, updated_at DESC, record_id);
            CREATE INDEX IF NOT EXISTS idx_session_memory_shape
                ON session_memory_records(shape_id, shape_version, updated_at DESC, record_id);
            CREATE INDEX IF NOT EXISTS idx_session_memory_supersedes
                ON session_memory_records(supersedes_record_id)
                WHERE supersedes_record_id IS NOT NULL;
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 23", error))
}

fn migrate_v24_add_roleplay_lore_records(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS module_roleplay_lore_records (
                record_id TEXT PRIMARY KEY,
                world_id TEXT NOT NULL,
                entity_id TEXT,
                session_id TEXT,
                branch_id TEXT,
                shape_id TEXT NOT NULL,
                shape_version INTEGER NOT NULL,
                canon_status TEXT NOT NULL,
                visibility TEXT NOT NULL,
                status TEXT NOT NULL,
                revision INTEGER NOT NULL,
                title TEXT NOT NULL,
                body TEXT NOT NULL,
                content_json TEXT NOT NULL,
                evidence_refs_json TEXT NOT NULL,
                source TEXT NOT NULL,
                confidence REAL NOT NULL,
                durability_rationale TEXT NOT NULL,
                supersedes_record_id TEXT,
                superseded_by_record_id TEXT,
                tombstoned_at TEXT,
                tombstone_reason TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_roleplay_lore_world_status_updated
                ON module_roleplay_lore_records(world_id, status, updated_at DESC, record_id);
            CREATE INDEX IF NOT EXISTS idx_roleplay_lore_entity
                ON module_roleplay_lore_records(world_id, entity_id, canon_status, visibility, updated_at DESC, record_id)
                WHERE entity_id IS NOT NULL;
            CREATE INDEX IF NOT EXISTS idx_roleplay_lore_shape
                ON module_roleplay_lore_records(shape_id, shape_version, updated_at DESC, record_id);
            CREATE INDEX IF NOT EXISTS idx_roleplay_lore_supersedes
                ON module_roleplay_lore_records(supersedes_record_id)
                WHERE supersedes_record_id IS NOT NULL;

            CREATE TABLE IF NOT EXISTS module_roleplay_lore_provenance_events (
                event_id TEXT PRIMARY KEY,
                record_id TEXT NOT NULL,
                world_id TEXT NOT NULL,
                evidence_refs_json TEXT NOT NULL,
                source TEXT NOT NULL,
                actor TEXT NOT NULL,
                note TEXT,
                created_at TEXT NOT NULL,
                FOREIGN KEY (record_id) REFERENCES module_roleplay_lore_records(record_id)
            );
            CREATE INDEX IF NOT EXISTS idx_roleplay_lore_provenance_record
                ON module_roleplay_lore_provenance_events(record_id, created_at, event_id);
            CREATE INDEX IF NOT EXISTS idx_roleplay_lore_provenance_world
                ON module_roleplay_lore_provenance_events(world_id, created_at, event_id);
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 24", error))
}

fn migrate_v25_add_model_provider_registry(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS model_providers (
                alias TEXT PRIMARY KEY,
                status TEXT NOT NULL,
                protocol TEXT NOT NULL,
                provider_kind TEXT NOT NULL,
                display_name TEXT,
                description TEXT,
                base_url TEXT,
                model_id TEXT NOT NULL,
                context_window_tokens INTEGER,
                max_output_tokens INTEGER,
                temperature_milli INTEGER,
                reasoning_effort TEXT,
                reasoning_format TEXT,
                secret_ciphertext TEXT,
                secret_updated_at TEXT,
                metadata_json TEXT NOT NULL,
                revision INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_model_providers_status
                ON model_providers(status, updated_at DESC, alias);
            CREATE INDEX IF NOT EXISTS idx_model_providers_protocol
                ON model_providers(protocol, provider_kind, alias);
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 25", error))
}

fn migrate_v26_add_roleplay_lore_layers(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
    tx.execute_batch(
        "
            CREATE TABLE IF NOT EXISTS module_roleplay_lore_layers (
                layer_id TEXT PRIMARY KEY,
                profile_id TEXT NOT NULL,
                name TEXT NOT NULL,
                description TEXT,
                purpose TEXT NOT NULL DEFAULT 'mixed',
                write_policy TEXT NOT NULL DEFAULT 'manual',
                is_archived INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_roleplay_lore_layers_profile
                ON module_roleplay_lore_layers(profile_id, is_archived, name);

            CREATE TABLE IF NOT EXISTS module_roleplay_lore_layer_entries (
                layer_id TEXT NOT NULL,
                record_id TEXT NOT NULL,
                is_constant INTEGER NOT NULL DEFAULT 0,
                priority INTEGER NOT NULL DEFAULT 0,
                added_at TEXT NOT NULL,
                PRIMARY KEY(layer_id, record_id),
                FOREIGN KEY (layer_id) REFERENCES module_roleplay_lore_layers(layer_id) ON DELETE CASCADE,
                FOREIGN KEY (record_id) REFERENCES module_roleplay_lore_records(record_id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_roleplay_lore_layer_entries_record
                ON module_roleplay_lore_layer_entries(record_id, layer_id);

            CREATE TABLE IF NOT EXISTS module_roleplay_chat_layers (
                chat_id TEXT NOT NULL,
                layer_id TEXT NOT NULL,
                priority INTEGER NOT NULL DEFAULT 0,
                enabled INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL,
                PRIMARY KEY(chat_id, layer_id),
                FOREIGN KEY (layer_id) REFERENCES module_roleplay_lore_layers(layer_id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_roleplay_chat_layers_enabled
                ON module_roleplay_chat_layers(chat_id, enabled, priority, layer_id);

            CREATE TABLE IF NOT EXISTS module_roleplay_lore_recall_traces (
                trace_id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                layer_ids TEXT NOT NULL,
                query_text TEXT,
                active_subjects TEXT,
                excluded_subjects TEXT,
                config_snapshot TEXT NOT NULL,
                entries_considered INTEGER NOT NULL,
                entries_returned INTEGER NOT NULL,
                token_budget INTEGER,
                tokens_consumed INTEGER,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_roleplay_lore_recall_traces_session
                ON module_roleplay_lore_recall_traces(session_id, created_at DESC, trace_id);

            CREATE TABLE IF NOT EXISTS module_roleplay_lore_layer_config (
                config_id TEXT PRIMARY KEY,
                layer_id TEXT NOT NULL UNIQUE,
                fts_weight REAL NOT NULL DEFAULT 1.0,
                subject_weight REAL NOT NULL DEFAULT 1.0,
                canon_weight REAL NOT NULL DEFAULT 0.5,
                tag_boost_weight REAL NOT NULL DEFAULT 0.5,
                recency_weight REAL NOT NULL DEFAULT 0.2,
                default_token_budget INTEGER NOT NULL DEFAULT 4000,
                constant_token_reserve INTEGER NOT NULL DEFAULT 500,
                min_relevance_score REAL NOT NULL DEFAULT 0.3,
                max_constants INTEGER NOT NULL DEFAULT 5,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                FOREIGN KEY (layer_id) REFERENCES module_roleplay_lore_layers(layer_id) ON DELETE CASCADE
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS module_roleplay_lore_records_fts USING fts5(
                record_id UNINDEXED,
                title,
                body,
                content_json,
                content='module_roleplay_lore_records',
                content_rowid='rowid'
            );

            CREATE TRIGGER IF NOT EXISTS roleplay_lore_fts_ai
            AFTER INSERT ON module_roleplay_lore_records BEGIN
                INSERT INTO module_roleplay_lore_records_fts(rowid, record_id, title, body, content_json)
                VALUES (new.rowid, new.record_id, new.title, new.body, new.content_json);
            END;

            CREATE TRIGGER IF NOT EXISTS roleplay_lore_fts_ad
            AFTER DELETE ON module_roleplay_lore_records BEGIN
                INSERT INTO module_roleplay_lore_records_fts(
                    module_roleplay_lore_records_fts,
                    rowid,
                    record_id,
                    title,
                    body,
                    content_json
                )
                VALUES ('delete', old.rowid, old.record_id, old.title, old.body, old.content_json);
            END;

            CREATE TRIGGER IF NOT EXISTS roleplay_lore_fts_au
            AFTER UPDATE ON module_roleplay_lore_records BEGIN
                INSERT INTO module_roleplay_lore_records_fts(
                    module_roleplay_lore_records_fts,
                    rowid,
                    record_id,
                    title,
                    body,
                    content_json
                )
                VALUES ('delete', old.rowid, old.record_id, old.title, old.body, old.content_json);
                INSERT INTO module_roleplay_lore_records_fts(rowid, record_id, title, body, content_json)
                VALUES (new.rowid, new.record_id, new.title, new.body, new.content_json);
            END;

            INSERT INTO module_roleplay_lore_records_fts(module_roleplay_lore_records_fts)
            VALUES ('rebuild');
            ",
    )
    .map_err(|error| persistence_error("apply schema migration 26", error))
}

fn apply_module_schema_migration_in_tx(
    tx: &rusqlite::Transaction<'_>,
    bundle: &ModuleSchemaBundle,
    installed_version: Option<u32>,
) -> CoreResult<()> {
    match bundle.module_id.as_str() {
        "simple_kv" => apply_simple_kv_module_schema_in_tx(tx, bundle, installed_version),
        module_id => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("module {module_id} has no registered migration implementation"),
        )),
    }
}

fn apply_simple_kv_module_schema_in_tx(
    tx: &rusqlite::Transaction<'_>,
    bundle: &ModuleSchemaBundle,
    _installed_version: Option<u32>,
) -> CoreResult<()> {
    if bundle.schema_version != 1 {
        return Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!(
                "simple_kv schema version {} has no migration implementation",
                bundle.schema_version
            ),
        ));
    }
    let table = bundle
        .tables
        .iter()
        .find(|table| table.table_name.as_str() == "entries")
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "simple_kv descriptor is missing entries table",
            )
        })?
        .physical_name(&bundle.module_id)?;
    let index = bundle
        .indexes
        .iter()
        .find(|index| {
            index.table_name.as_str() == "entries" && index.purpose.as_str() == "scope_key"
        })
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "simple_kv descriptor is missing scope_key index",
            )
        })?
        .physical_name(&bundle.module_id)?;
    let expiry_index = bundle
        .indexes
        .iter()
        .find(|index| {
            index.table_name.as_str() == "entries" && index.purpose.as_str() == "expires_at"
        })
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "simple_kv descriptor is missing expires_at index",
            )
        })?
        .physical_name(&bundle.module_id)?;
    let table = sqlite_identifier(&table)?;
    let index = sqlite_identifier(&index)?;
    let expiry_index = sqlite_identifier(&expiry_index)?;
    tx.execute_batch(&format!(
        "
            CREATE TABLE IF NOT EXISTS {table} (
                scope_type TEXT NOT NULL,
                scope_id TEXT NOT NULL,
                entry_key TEXT NOT NULL,
                value_json TEXT NOT NULL,
                revision INTEGER NOT NULL CHECK (revision > 0),
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                expires_at TEXT,
                PRIMARY KEY (scope_type, scope_id, entry_key)
            );
            CREATE UNIQUE INDEX IF NOT EXISTS {index}
                ON {table}(scope_type, scope_id, entry_key);
            CREATE INDEX IF NOT EXISTS {expiry_index}
                ON {table}(expires_at)
                WHERE expires_at IS NOT NULL;
            "
    ))
    .map_err(|error| persistence_error("apply simple_kv module schema", error))
}

fn sqlite_identifier(identifier: &str) -> CoreResult<String> {
    if identifier.is_empty()
        || !identifier.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
    {
        return Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("unsafe sqlite identifier {identifier:?}"),
        ));
    }
    Ok(identifier.to_string())
}

pub(crate) fn install_module_schema_registry_in_tx(
    tx: &rusqlite::Transaction<'_>,
    registry: &ModuleSchemaRegistry,
    supported_capabilities: &[ModuleSchemaCapability],
    now: &IsoTimestamp,
) -> CoreResult<Vec<InstalledModuleSchemaRecord>> {
    registry.validate()?;
    registry.validate_capabilities(supported_capabilities)?;

    let mut installed = Vec::new();
    for bundle in registry.bundles() {
        let module_id = bundle.module_id.as_str();
        let descriptor_fingerprint = bundle.descriptor_fingerprint()?;
        let existing = load_installed_module_schema_record(tx, module_id)?;
        if let Some(existing) = existing {
            validate_version_progression(Some(existing.installed_version), bundle.schema_version)?;
            apply_module_schema_migration_in_tx(tx, bundle, Some(existing.installed_version))?;
            if existing.installed_version == bundle.schema_version {
                if existing.descriptor_fingerprint != descriptor_fingerprint {
                    return Err(CoreError::new(
                        CoreErrorKind::ActionRejected,
                        format!(
                            "module {module_id} descriptor fingerprint changed without a schema version bump"
                        ),
                    ));
                }
                installed.push(existing);
                continue;
            }
        } else {
            validate_version_progression(None, bundle.schema_version)?;
            apply_module_schema_migration_in_tx(tx, bundle, None)?;
        }

        tx.execute(
            "INSERT INTO module_schema_versions (
                module_id,
                installed_version,
                descriptor_fingerprint,
                installed_at,
                updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?4)
            ON CONFLICT(module_id) DO UPDATE SET
                installed_version = excluded.installed_version,
                descriptor_fingerprint = excluded.descriptor_fingerprint,
                updated_at = excluded.updated_at",
            params![
                module_id,
                bundle.schema_version as i64,
                descriptor_fingerprint.as_str(),
                now.as_str(),
            ],
        )
        .map_err(|error| persistence_error("upsert module schema version", error))?;
        installed.push(
            load_installed_module_schema_record(tx, module_id)?.ok_or_else(|| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    format!("module {module_id} schema version missing after install"),
                )
            })?,
        );
    }

    Ok(installed)
}

pub(crate) fn load_installed_module_schema_records(
    conn: &Connection,
) -> CoreResult<Vec<InstalledModuleSchemaRecord>> {
    let mut stmt = conn
        .prepare(
            "SELECT module_id, installed_version, descriptor_fingerprint, installed_at, updated_at
             FROM module_schema_versions
             ORDER BY module_id ASC",
        )
        .map_err(|error| persistence_error("prepare installed module schema records", error))?;
    let rows = stmt
        .query_map([], row_to_installed_module_schema_record)
        .map_err(|error| persistence_error("query installed module schema records", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| persistence_error("load installed module schema records", error))
}

pub(crate) fn storage_schema_for_registry(
    conn: &Connection,
    registry: &ModuleSchemaRegistry,
    supported_capabilities: &[ModuleSchemaCapability],
) -> CoreResult<RuntimeModuleSchemaRegistryDiagnostics> {
    let installed = load_installed_module_schema_records(conn)?;
    module_schema_registry_diagnostics(registry, &installed, supported_capabilities)
}

fn load_installed_module_schema_record(
    conn: &Connection,
    module_id: &str,
) -> CoreResult<Option<InstalledModuleSchemaRecord>> {
    conn.query_row(
        "SELECT module_id, installed_version, descriptor_fingerprint, installed_at, updated_at
         FROM module_schema_versions
         WHERE module_id = ?1",
        params![module_id],
        row_to_installed_module_schema_record,
    )
    .optional()
    .map_err(|error| persistence_error("load installed module schema record", error))
}

fn row_to_installed_module_schema_record(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<InstalledModuleSchemaRecord> {
    let raw_module_id: String = row.get(0)?;
    let installed_version: i64 = row.get(1)?;
    if installed_version <= 0 || installed_version > i64::from(u32::MAX) {
        return Err(rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Integer,
            Box::new(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("invalid installed module schema version {installed_version}"),
            )),
        ));
    }
    let module_id = ModuleId::new(raw_module_id).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(InstalledModuleSchemaRecord {
        module_id,
        installed_version: installed_version as u32,
        descriptor_fingerprint: row.get(2)?,
        installed_at: row.get(3)?,
        updated_at: row.get(4)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db_path(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "rusty-crew-sqlite-schema-{label}-{}-{unique}.sqlite3",
            std::process::id()
        ))
    }

    fn remove_temp_db(db_path: &Path) {
        let _ = fs::remove_file(db_path);
        let _ = fs::remove_file(format!("{}-wal", db_path.display()));
        let _ = fs::remove_file(format!("{}-shm", db_path.display()));
    }

    fn table_has_column(db_path: &Path, table: &str, column: &str) -> bool {
        let conn = Connection::open(db_path).unwrap();
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .unwrap();
        stmt.query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .iter()
            .any(|value| value == column)
    }

    fn table_exists(db_path: &Path, table: &str) -> bool {
        let conn = Connection::open(db_path).unwrap();
        conn.query_row(
            "SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = ?1",
            params![table],
            |_| Ok(()),
        )
        .optional()
        .unwrap()
        .is_some()
    }

    fn index_exists(db_path: &Path, index: &str) -> bool {
        let conn = Connection::open(db_path).unwrap();
        conn.query_row(
            "SELECT 1 FROM sqlite_schema WHERE type = 'index' AND name = ?1",
            params![index],
            |_| Ok(()),
        )
        .optional()
        .unwrap()
        .is_some()
    }

    #[test]
    fn diagnostic_table_names_are_whitelisted() {
        for table in DiagnosticTable::ALL {
            assert_eq!(DiagnosticTable::parse(table.as_str()).unwrap(), *table);
        }
        let error = DiagnosticTable::parse("sessions; DROP TABLE sessions").unwrap_err();
        assert_eq!(error.kind, CoreErrorKind::InvalidInput);
    }

    #[test]
    fn fresh_database_applies_all_schema_migrations() {
        let db_path = temp_db_path("fresh-schema");
        let store = CoordinationStore::open_file(&db_path).unwrap();

        assert_eq!(store.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
        assert_eq!(
            store.schema_migrations().unwrap().len(),
            SCHEMA_MIGRATIONS.len()
        );
        assert_eq!(store.count_rows("sessions").unwrap(), 0);
        assert!(table_exists(&db_path, "module_simple_kv_entries"));
        assert!(table_exists(&db_path, "profile_registry"));
        assert!(table_exists(&db_path, "session_memory_records"));
        assert!(table_exists(&db_path, "module_roleplay_lore_layers"));
        assert!(table_exists(&db_path, "module_roleplay_lore_layer_entries"));
        assert!(table_exists(&db_path, "module_roleplay_chat_layers"));
        assert!(table_exists(&db_path, "module_roleplay_lore_recall_traces"));
        assert!(table_exists(&db_path, "module_roleplay_lore_layer_config"));
        assert!(table_exists(&db_path, "module_roleplay_lore_records_fts"));
        assert!(index_exists(
            &db_path,
            "idx_module_simple_kv_entries_scope_key"
        ));
        assert!(index_exists(&db_path, "idx_profile_registry_lifecycle"));
        assert!(index_exists(
            &db_path,
            "idx_session_memory_session_status_updated"
        ));
        assert!(index_exists(&db_path, "idx_roleplay_lore_layers_profile"));
        assert!(index_exists(
            &db_path,
            "idx_roleplay_lore_layer_entries_record"
        ));
        assert!(index_exists(&db_path, "idx_roleplay_chat_layers_enabled"));
        assert!(index_exists(
            &db_path,
            "idx_roleplay_lore_recall_traces_session"
        ));
        assert!(index_exists(
            &db_path,
            "idx_module_simple_kv_entries_expires_at"
        ));
        let installed = store.installed_module_schemas().unwrap();
        assert_eq!(installed.len(), 1);
        assert_eq!(installed[0].module_id.as_str(), "simple_kv");
        assert_eq!(installed[0].installed_version, 1);

        remove_temp_db(&db_path);
    }

    #[test]
    fn version_one_database_migrates_to_current_schema() {
        let db_path = temp_db_path("migrated-schema");
        {
            let mut conn = Connection::open(&db_path).unwrap();
            prepare_migration_metadata(&conn).unwrap();
            apply_schema_migrations(&mut conn, &SCHEMA_MIGRATIONS[..1]).unwrap();
        }

        let store = CoordinationStore::open_file(&db_path).unwrap();

        assert_eq!(store.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
        assert!(table_has_column(&db_path, "sessions", "tool_profile_json"));
        assert!(table_has_column(
            &db_path,
            "worker_runs",
            "fan_out_failure_policy"
        ));
        for table in [
            "agents",
            "agent_instances",
            "session_configs",
            "session_identity",
            "event_session_index",
            "event_agent_index",
            "runtime_search_fts",
            "runtime_counters",
            "queued_messages",
            "runtime_import_batches",
            "legacy_id_mappings",
            "profile_memories",
            "scheduled_jobs",
            "scheduled_job_runs",
            "provider_wire_states",
            "message_slots",
            "message_variants",
            "messages",
            "message_blocks",
            "channel_bindings",
            "mcp_bindings",
            "module_schema_versions",
            "memory_proposals",
            "memory_governance_decisions",
            "profile_registry",
            "session_memory_records",
            "module_roleplay_lore_layers",
            "module_roleplay_lore_layer_entries",
            "module_roleplay_chat_layers",
            "module_roleplay_lore_recall_traces",
            "module_roleplay_lore_layer_config",
            "module_roleplay_lore_records_fts",
            "module_simple_kv_entries",
        ] {
            assert!(table_exists(&db_path, table), "missing table {table}");
        }
        for index in [
            "idx_worker_runs_parent_status_created",
            "idx_profile_memories_profile_updated",
            "idx_scheduled_jobs_due",
            "idx_scheduled_job_runs_status_deadline",
            "idx_provider_wire_states_current",
            "idx_channel_bindings_external",
            "idx_mcp_bindings_agent_profile",
            "idx_module_simple_kv_entries_scope_key",
            "idx_module_simple_kv_entries_expires_at",
            "idx_memory_proposals_dedupe",
            "idx_memory_governance_decisions_proposal",
            "idx_profile_registry_lifecycle",
            "idx_roleplay_lore_layers_profile",
            "idx_roleplay_lore_layer_entries_record",
            "idx_roleplay_chat_layers_enabled",
            "idx_roleplay_lore_recall_traces_session",
        ] {
            assert!(index_exists(&db_path, index), "missing index {index}");
        }

        remove_temp_db(&db_path);
    }

    #[test]
    fn future_schema_version_fails_closed() {
        let db_path = temp_db_path("future-schema");
        {
            let conn = Connection::open(&db_path).unwrap();
            prepare_migration_metadata(&conn).unwrap();
            conn.execute(
                "INSERT INTO schema_migrations (version, description) VALUES (?1, ?2)",
                params![CURRENT_SCHEMA_VERSION + 1, "future migration"],
            )
            .unwrap();
        }

        let error = CoordinationStore::open_file(&db_path).unwrap_err();

        assert_eq!(error.kind, CoreErrorKind::PersistenceFailure);
        assert!(error.message.contains("newer than supported"));

        remove_temp_db(&db_path);
    }

    #[test]
    fn failed_schema_migration_rolls_back_partial_ddl() {
        fn create_then_fail(tx: &rusqlite::Transaction<'_>) -> CoreResult<()> {
            tx.execute_batch("CREATE TABLE partial_migration_marker (id INTEGER PRIMARY KEY);")
                .map_err(|error| persistence_error("create partial migration marker", error))?;
            Err(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "intentional migration failure",
            ))
        }

        let db_path = temp_db_path("rollback-schema");
        let mut conn = Connection::open(&db_path).unwrap();
        prepare_migration_metadata(&conn).unwrap();
        let failing_migrations = [SchemaMigration {
            version: 1,
            description: "create table then fail",
            apply: create_then_fail,
        }];

        let error = apply_schema_migrations(&mut conn, &failing_migrations).unwrap_err();

        assert_eq!(error.kind, CoreErrorKind::PersistenceFailure);
        assert!(!table_exists(&db_path, "partial_migration_marker"));
        assert_eq!(current_schema_version(&conn).unwrap(), 0);

        drop(conn);
        remove_temp_db(&db_path);
    }
}
