# Runtime Database Scale And Maintenance

Status: Implementation contract for task 2877

Date: 2026-06-20

## Purpose

Rusty Crew starts with SQLite because the Rust-owned runtime state is contained
behind `core-persistence`. That boundary keeps the storage choice reversible:
engine, service, Den adapters, and diagnostics surfaces should call typed
persistence APIs rather than embedding SQL or SQLite assumptions.

This document defines the first guardrails for running many agents from one
service process without discovering database pressure only after the file has
grown into a multi-GB artifact.

## SQLite Operating Settings

Every `CoordinationStore` connection applies:

- WAL journal mode;
- foreign keys enabled;
- `synchronous = NORMAL`;
- in-memory temporary storage;
- a bounded busy timeout;
- a bounded WAL autocheckpoint page target.

These settings assume one local runtime service owns writes and that callers
coordinate through the service instead of opening independent writer-heavy
connections. If Rusty Crew later supports multiple service processes against
one state store, that should trigger a PostgreSQL design pass rather than
stretching SQLite into a distributed coordination layer.

## Hot Path Index Budget

Schema version 9 adds explicit indexes for the first expected service hot paths:

- sessions by agent/profile;
- agent instances by agent/status;
- agent messages by sender, recipient, and correlation;
- completion packets by session;
- worker runs by parent/status, delegated session, and profile/task;
- tool calls by session;
- existing event projection and queued-message indexes.

The persistence crate owns query-plan checks through
`CoordinationStore::hot_query_plan_checks`. Tests assert that representative
queue, worker-run, message, completion, and event lookups use indexes. This is
not a substitute for production telemetry, but it gives future migrations a
tripwire if a query shape regresses into a full scan.

## Retention Rules

Retention is explicit maintenance, not ambient cleanup. The current
`RuntimeMaintenancePolicy` can:

- expire pending queued messages at a caller-provided timestamp;
- purge terminal queued messages older than a caller-provided cutoff;
- remove purged queue rows from search at the same time;
- run SQLite optimize and WAL checkpoint steps.

The event log remains append-only for now. Deleting or compacting
`event_history` must be a separate policy because projections, search rows,
counters, and replay behavior all depend on event facts. High-volume future
delta streams should use summarized derived rows before they enter ordinary
runtime search.

## Size And Maintenance Signals

`CoordinationStore::database_size` reports page count, page size, freelist
bytes, database bytes, and WAL bytes. `run_maintenance` returns before/after
snapshots so operators can tell whether retention and checkpointing changed the
pressure.

`CoordinationStore::storage_diagnostics` exposes a boolean `pressure` summary
and a structured `pressure_signals` list. The signal list is guidance for
operators and admin UIs; it is not an automatic backend migration trigger.
SQLite remains the default for local, container, and small roleplay
deployments.

Useful warning signals:

- WAL grows faster than maintenance windows can checkpoint;
- freelist bytes stay high after retention, suggesting a backup/export or VACUUM
  window is needed;
- event and message history grows faster than search/query surfaces can page;
- query-plan checks still pass, but real diagnostics are slow because working
  sets no longer fit local cache;
- multiple independent processes need concurrent writes;
- profiles or tenants need hard operational isolation.

Any of those should start a PostgreSQL migration plan. The typed persistence
APIs should remain the contract; PostgreSQL should replace the backend, not
leak a second query language through the service.

Current SQLite warning thresholds are deliberately small enough for fixtures
and local diagnostics to exercise them before a production service is in pain:

| Signal | Warning threshold | Why it matters |
| --- | ---: | --- |
| `sqlite_wal_bytes` | 64 MiB | WAL growth above this suggests checkpoint/maintenance windows are falling behind. |
| `sqlite_freelist_percent` | 25% | Persistent freelist pressure suggests export/backup/VACUUM planning. |
| `sqlite_hot_query_plan_failures` | 0 failures | Hot queue/message/search diagnostics should stay indexed. |
| `runtime_search_health` | healthy FTS table required | Search is backend-sensitive and must stay valid before growth. |
| `active_agent_count` | 32 sessions/agent instances | Dozens of active agents increase wake, queue, scheduler, and writer pressure. |
| `conversation_transcript_growth` | 64 transcript/tree rows | Multi-user roleplay transcripts are an expected PostgreSQL pressure area. |
| `memory_lore_growth` | 64 memory/lore-like rows | Lore, profile memory, data-bank, and attachment growth should remain visible in one store. |
| `runtime_search_growth` | 64 search rows | SQLite FTS5 and PostgreSQL search are not equivalent, so growth needs early visibility. |
| `queued_message_retention` | 32 queue rows | Queues need aggressive TTL/no-resurrection checks as retained rows grow. |
| `scheduler_row_growth` | 32 scheduler rows | Scheduler claims become more sensitive when multiple writers are needed. |
| `provider_wire_state_growth` | 32 provider-state rows | Provider wire state may contain large opaque payloads and expiry semantics. |
| `single_service_writer_assumption` | informational | Independent writer processes should trigger PostgreSQL planning. |

## Backup And Export

SQLite backup/export should be service-owned and quiesced or snapshot-based.
Callers should not copy the main database file without its WAL/shm files while
the service is active. A future backup tool should report the same
`RuntimeDatabaseSize` fields before and after export so the operator can see
what was captured.

## Scale Fixture

The persistence tests include two scale-oriented guardrails. The maintenance
fixture creates many sessions, worker runs, routed messages, queued messages,
search rows, counters, and then runs maintenance. It verifies:

- expired queue rows can be removed without redelivering them;
- purged queue rows leave runtime search;
- one fresh queued message remains deliverable;
- size reporting is populated;
- hot query-plan checks keep index coverage.

The backend-pressure fixture extends this toward expected PostgreSQL pressure
areas: dozens of active sessions, larger transcript trees, dense
memory/lore-like records, runtime search rows, scheduler rows, queued messages,
and provider wire state. It asserts that the named pressure signals activate and
that expired queue rows are still not resurrected after maintenance.

These fixtures are intentionally guardrails, not benchmarks. If production
behavior starts approaching the warning signals above, add a dedicated benchmark
or replay corpus before tuning indexes further or switching backends.
