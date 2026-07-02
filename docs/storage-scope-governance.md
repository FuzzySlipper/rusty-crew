# Storage Scope Governance

Status: initial governance design for task 3909

Date: 2026-07-01

## Purpose

Rusty Crew owns Crew service storage, but ownership must be partitioned. This
document defines the storage-scope groups that should guide the
`core-persistence` split and future boundary smokes.

This is a planning and governance document. It does not migrate tables by
itself.

## Scope Groups

The companion machine-readable draft is `governance/storage-scope.toml`.

### `storage_admin`

Owns schema versioning, migrations, backend config projection, storage
capability diagnostics, database size, maintenance, logical import/export batch
metadata, and repository/module diagnostics.

### `sessions_identities`

Owns durable agents, agent instances, sessions, session identity links, and
immutable session config snapshots.

### `events_projections`

Owns append-only core events, event history, body projections, and restart
hydration read models.

### `queues_messages`

Owns queued messages, internal agent messages, TTL expiry, terminal/purge state,
and no-resurrection semantics.

### `scheduler_jobs`

Owns scheduled jobs, scheduled runs, claim/complete/expire behavior, and manual
run requests.

### `worker_runs_completions`

Owns delegated/worker run state, completion packets, delegation lineage, and
fan-out group state.

### `provider_state`

Owns provider wire-state payloads, provider-state diagnostics, fingerprints,
expiry, and invalidation metadata.

### `profile_registry`

Owns active profile registry records, DB-backed prompt fields, file asset refs,
model-provider aliases, lifecycle state, and profile import/export metadata.

### `bindings`

Owns MCP, channel, adapter, and external binding records that must survive
restart and remain scoped by profile/session/agent.

### `conversations_attachments`

Owns message slots, variants, conversation branches, snapshots, jump targets,
attachments, and data-bank scopes.

### `memory_lore_modules`

Owns dense/profile memory, typed memory spaces, memory proposals/governance,
roleplay lore, module schema registry records, and generated module-owned
tables.

### `telemetry_search`

Owns tool call history, runtime counters, runtime search indexes/read models,
query catalog projections, and user/agent readback surfaces.

## Table Prefix Rules

Raw core tables should be listed in exactly one scope group.

Generated module tables must use the module schema registry naming rules:

- `module_<module_id>_<table_name>`;
- `idx_module_<module_id>_<table_name>_<purpose>`;
- `trg_module_<module_id>_<table_name>_<purpose>`;
- `module_<module_id>_<store_name>_search`.

Module ids, store names, table names, index purposes, and trigger purposes must
use validated lowercase ASCII snake_case identifiers. Dynamic user input must
not become a physical table/index name.

## Ownership Rules

- SQL stays inside Rust persistence/storage crates.
- TypeScript packages do not open the Crew database and do not issue raw SQL.
- Platform adapters do not own Crew storage schema.
- Den is not the fallback storage home for Crew service data.
- Module-owned physical tables are declared through Rust-owned descriptors.
- Query catalog entries are read-only, curated, and capability-gated.
- Queue/message imports must not resurrect expired or terminal work.
- Backend-specific features are hidden behind repository contracts and
  capability diagnostics.

## Mechanical Checks To Add

Future boundary smokes should be able to check:

1. Every raw table in migrations is assigned to one storage scope group.
2. Every generated `module_` table belongs to a registered module descriptor.
3. No SQL-like dependency leaks outside `crates/core/core-persistence` or a
   future approved storage crate.
4. Dynamic SQL helpers use whitelisted table/index names.
5. `governance/storage-scope.toml` agrees with repository diagnostics exposed by
   `repositories.rs`.
6. TypeScript packages do not import SQLite/Postgres client libraries or shell
   out to inspect the Crew DB.
7. Bridge operation additions that expose storage data point at a typed
   repository/query-catalog contract.

## Implementation Notes

The first implementation task should use this document to turn
`repositories.rs` into a concrete module split map. The check can start
lightweight: inventory tables and fail on unassigned raw names. It can grow as
the repository modules become real.

