# Persistence Boundary And Portability Contract

Status: Implementation contract for task 2867

Date: 2026-06-20

## Scope

Rusty Crew currently uses SQLite for local coordination persistence. SQLite is
an implementation detail of `crates/core/core-persistence`, not an architectural
dependency for the engine, body projector, bridge, adapters, or TypeScript brain
island.

This contract turns the persistence boundary into an explicit rule.

## Boundary Rules

- `rusqlite` imports must stay inside `crates/core/core-persistence`.
- SQL literals must stay inside `crates/core/core-persistence`.
- Other crates consume `CoordinationStore` methods and protocol structs.
- TypeScript packages must not inspect SQLite files or issue raw SQL.
- Dynamic SQL helpers must use typed whitelists or closed enums.
- Den product data is not mirrored into runtime persistence.
- Future storage engines must preserve the public persistence API shape rather
  than forcing SQL details into coordination logic.

## Repository Surface

The near-term repository surface is `CoordinationStore`. It groups operations by
runtime concern:

- sessions: save/load session state;
- event log: save/load ordered `CoreEvent` history;
- messages: project routed runtime messages;
- delegated runs: save/update/query worker/delegated run records;
- completion packets: persist and query delivered completions;
- tool telemetry: persist and query tool-call history;
- diagnostics: bounded whitelisted row counts.

Future tasks may split this into smaller traits or modules, but the split should
remain inside `core-persistence` unless a caller genuinely needs a new stable
runtime query contract.

## SQLite-Specific Features

Allowed SQLite-specific features:

- WAL mode and connection pragmas;
- `CREATE TABLE IF NOT EXISTS` for current bootstrapping;
- FTS5 for future search, behind a search API;
- whitelisted diagnostic table counts;
- SQLite JSON text columns for compact protocol snapshots.

PostgreSQL mapping notes:

- WAL pragmas become deployment/connection-pool configuration.
- FTS5 maps to PostgreSQL `tsvector`/GIN-backed search behind the same search
  API.
- JSON text columns can map to `jsonb` if useful, but callers must not rely on
  SQL JSON operators.
- `INSERT OR REPLACE`/`ON CONFLICT` usage must stay hidden behind repository
  methods.

## Guard Test

`core-persistence` owns a boundary test that scans Rust and TypeScript source
for persistence backend leaks. It intentionally allows:

- SQL and `rusqlite` inside `crates/core/core-persistence`;
- documentation references;
- Cargo dependency declarations.

If a future task needs SQL elsewhere, add a design note first. Most cases should
instead add a method to `CoordinationStore` or a typed query API.
