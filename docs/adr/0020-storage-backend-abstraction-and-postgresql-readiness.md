# ADR 0020: Storage Backend Abstraction And PostgreSQL Readiness

Status: Proposed for task 3377

Date: 2026-06-25

## Context

Rusty Crew currently uses SQLite through `crates/core/core-persistence`.
That has been the right default for local deployment: it is simple, visible,
portable, and already contained behind Rust-owned typed APIs. The audit for this
task found no TypeScript-owned raw SQL path and no broad SQLite leakage outside
the persistence boundary. `rusqlite` and SQL literals are concentrated in
`core-persistence`.

SQLite must remain a well-supported first-class backend. One of the first
expected deployments outside this machine is a small containerized
Rusty Roleplay service running a couple agents on a SQLite database. SQLite is
not just a bootstrap backend; it is the right backend for simple local,
container, and small-agent deployments.

The pressure is that `core-persistence` has become a very large boundary. It now
owns schema migrations, SQLite connection pragmas, runtime search, maintenance,
sessions, event history, queued messages, scheduler/job state, provider wire
state, transcript/message trees, attachments, profile memory, external bindings,
runtime counters, import tracking, diagnostics, and bridge-facing record shapes.

That containment is good architecture. The size of the single store is the
warning sign: adding roleplay lore, typed memory spaces, module-owned data, and
dozens of active agents will make a future PostgreSQL move harder if every new
feature lands as another SQLite-shaped method on one monolithic store.

There is also a practical scar here: the den-core SQLite-to-PostgreSQL migration
has been painful. Rusty Crew should learn from that, but should not force its
first PostgreSQL validation path through a migration. A development PostgreSQL
service already exists for Rusty Crew on den-k8; the initial local Postgres
exercise should switch the service to a fresh PostgreSQL database and shake out
the backend module as a first-class empty-db path.

## Decision

Rusty Crew will keep SQLite as the embedded/local default and will not require
an immediate PostgreSQL implementation.

Rusty Crew will treat both SQLite and PostgreSQL as first-class storage modules
once PostgreSQL support begins:

- SQLite is the default for local, container, small-agent, and simple roleplay
  deployments.
- PostgreSQL is the scale/concurrency backend for deployments with many active
  agents, multi-user roleplay, large transcript/lore/search workloads, or
  operational needs that exceed SQLite's single-service-writer assumptions.

PostgreSQL readiness should start with backend-neutral structure inside the Rust
persistence boundary:

- add explicit storage backend config with SQLite as the default;
- make backend capabilities first-class service diagnostics;
- split `CoordinationStore` internally into repository modules or traits by
  runtime concern;
- add backend/dialect conformance tests for repository behavior;
- define logical export/import records for portability and future migrations,
  but do not make migration the first local PostgreSQL cutover path;
- introduce PostgreSQL only through a narrow low-risk repository proof slice
  after the config, capability, and repository boundaries exist.

The first PostgreSQL proof slice should not be queues, scheduler/job claims,
transcripts, or runtime search. It should be a comparatively low-risk repository
such as runtime counters, module schema registry records, import batches, or
another simple append/upsert table. The proof should validate connection,
migration, typed repository API parity, diagnostics, and export/import shape
before touching correctness-sensitive runtime coordination.

After SQLite conformance is thoroughly tested and the PostgreSQL proof slice is
credible, the local service should be tested against the existing den-k8
PostgreSQL service as a new empty database. That test is intentionally a clean
backend switch, not a migration of the current SQLite service data.

## Current Audit Summary

SQLite assumptions are mostly contained:

- `rusqlite`, `Connection`, `Transaction`, `params!`, and SQL literals live in
  `crates/core/core-persistence`.
- `core-engine` constructs `CoordinationStore` and calls typed methods.
- The native bridge and TypeScript service code call manifest/bridge methods
  such as `storageDiagnostics`, `runMaintenance`, `searchRuntime`,
  `listProfileMemory`, and `queryRuntimeCounters`.
- The existing persistence boundary guard already protects against backend
  leaks into TypeScript and non-persistence Rust crates.

Important SQLite-specific behavior currently inside `core-persistence`:

- WAL pragmas, busy timeout, synchronous mode, and autocheckpoint settings;
- migration metadata and version checks;
- SQLite FTS5 for runtime search;
- JSON stored as text columns and deserialized through Rust types;
- SQLite maintenance with `PRAGMA optimize` and `wal_checkpoint`;
- whitelisted diagnostic table counts and query-plan checks;
- SQLite upsert forms such as `INSERT OR REPLACE`, `INSERT OR IGNORE`, and
  `ON CONFLICT`;
- one mutex-protected `rusqlite::Connection`.

This is acceptable for the current local service model. It is not yet a storage
abstraction.

## Backend Config Shape

Configuration should describe storage intent without leaking secrets or forcing
PostgreSQL onto local users.

Example service config shape:

```json
{
  "storage": {
    "backend": "sqlite",
    "sqlite": {
      "path": "coordination.sqlite3",
      "wal": true,
      "busyTimeoutMs": 5000
    },
    "postgres": {
      "databaseUrlEnv": "RUSTY_CREW_DATABASE_URL",
      "schema": "rusty_crew",
      "maxConnections": 10,
      "statementTimeoutMs": 30000
    }
  }
}
```

Rules:

- omitted `storage` means SQLite in `engine_data_dir/coordination.sqlite3`;
- PostgreSQL connection strings must come from environment variables or secret
  providers, not committed config files;
- service and admin APIs should report the selected backend and capabilities,
  but feature code should prefer capability checks over backend-name checks;
- deployment docs must make it clear that SQLite is single-service-writer local
  storage, while PostgreSQL is the future shared/high-write backend.

The current local PostgreSQL test target is documented in
`[doc: den-network/rusty-crew-postgres-service]`. It provides a development
PostgreSQL service on den-k8 with env at
`/home/system/database/rusty-crew-postgres.env`. That service should be used for
empty-db PostgreSQL module testing once the repository abstraction is ready.

## Capability Gates

Storage diagnostics should expose stable capability names. Initial capabilities
should include:

- `transactions`;
- `json_metadata`;
- `runtime_full_text_search`;
- `concurrent_writers`;
- `advisory_locks`;
- `online_migrations`;
- `maintenance_checkpoint`;
- `maintenance_vacuum_or_optimize`;
- `estimated_table_size`;
- `query_plan_diagnostics`;
- `row_level_claims`;
- `listen_notify`;
- `logical_export_import`.

SQLite and PostgreSQL do not need identical capabilities. They need explicit
capabilities so modules, admin screens, diagnostics, and future migrations know
what is safe.

## Repository Boundaries

The next implementation step should split the persistence code by concern while
preserving current public bridge/service behavior.

Recommended internal repository groups:

- storage admin: backend config, capabilities, diagnostics, maintenance,
  migrations;
- sessions and identities;
- runtime event history and projections;
- internal messages and queued messages;
- scheduler/job state;
- worker/delegated runs and completion packets;
- tool telemetry;
- provider wire state;
- runtime search;
- transcript/conversation trees and attachments;
- profile registry and file asset references;
- typed memory spaces and dense profile memory;
- module schema registry and module-owned tables;
- import/export and legacy id mapping;
- runtime counters.

This can start as Rust modules under `core-persistence` without introducing a
public trait explosion. Introduce traits only where the second backend proof
needs them.

## SQL Portability Risks

### Transcripts And Conversation Trees

Conversation branches, message slots, variants, snapshots, attachments, and data
bank scopes are high-volume and correctness-sensitive. Portability risks include
ordering, branch head updates, large JSON/content payloads, attachment expiry,
and UI pagination semantics. Do not make this the first PostgreSQL slice.

### Memory And Lore

Dense profile memory and future roleplay lore will need typed JSON/content,
provenance, revision checks, search, visibility, retention, and governance.
This should be DB-backed service data, but the repository API must stay typed so
SQLite text JSON can map to PostgreSQL `jsonb` later.

### Search

SQLite FTS5 and PostgreSQL search are not equivalent. The service should expose
a runtime search API and capability metadata, not FTS query syntax. Search
ranking and token behavior may differ by backend; tests should assert stable
result contracts only where necessary.

### Scheduler And Job State

Scheduler claims need stronger concurrency semantics than ordinary upserts once
multiple processes or worker executors are involved. PostgreSQL can eventually
use row locks or advisory locks, but the current SQLite implementation assumes
one service process. This area should wait until backend capabilities include
claim semantics.

### Queues

Queued messages are dangerous because old messages can be resurrected if
migration, retry, or import semantics are loose. Export/import must preserve
state, expiry, terminal status, and purge markers. Any queue backend work must
include aggressive TTL/no-resurrection tests.

### Provider Wire State

Provider wire state may hold large opaque payloads and expiration/invalidation
metadata. Portability should keep payload encoding explicit and make expiry
behavior deterministic across backends.

## Export And Import

SQLite-to-PostgreSQL migration, when needed, should use a service-owned logical
export/import format as the durable portability contract. Raw SQLite dumps or
PostgreSQL dumps can remain operational tools, but they should not be the
cross-backend contract.

This is not the first local PostgreSQL validation path. For the current local
service, the planned cutover test is:

1. keep the existing SQLite data untouched;
2. configure the service to use a fresh PostgreSQL database from
   `[doc: den-network/rusty-crew-postgres-service]`;
3. run normal service/profile/agent/channel/scheduler/storage diagnostics
   against that empty database;
4. compare behavior with the SQLite module through the same conformance and
   smoke tests.

That path exercises both storage modules as first-class implementations without
turning migration correctness into the first blocker.

An export bundle should include:

- source backend and storage capability snapshot;
- schema version and module schema versions;
- typed records grouped by repository;
- stable ids and legacy id mappings;
- export timestamp and service version;
- checksums/counts per repository;
- queue expiration and terminal state metadata;
- optional file asset references or bundled assets for profile exports.

Import should support:

- dry-run validation;
- capability checks before applying records;
- idempotency through import batch ids;
- quiesced or read-only migration windows for live services;
- count/checksum validation after apply;
- explicit refusal when queue/message TTL semantics would resurrect expired
  work.

## Implementation Plan

1. Add storage backend config parsing and runtime projection while preserving
   the SQLite default.
2. Split `core-persistence` into internal repository modules and define backend
   capability contracts per repository group.
3. Add repository conformance fixtures that verify current SQLite behavior
   through backend-neutral APIs.
4. Add a narrow PostgreSQL proof slice for a low-risk repository, tested against
   a fresh PostgreSQL database.
5. Exercise the local service against the den-k8 PostgreSQL service as an
   empty-db backend switch, not a migration.
6. Define logical export/import records and dry-run validation for future
   migrations and portability.
7. Add PostgreSQL diagnostics, deployment notes, and operator runbook material.
8. Add load/scale fixtures that decide when a deployment should move from
   SQLite to PostgreSQL.

This order keeps SQLite strong for small deployments, gives PostgreSQL a real
first-class empty-db path, and avoids making migration pain the first proof of
the backend abstraction.

## Non-Goals

- Do not move to PostgreSQL immediately just because future scale may need it.
- Do not treat SQLite as a temporary stub; it must remain supported for small
  container and roleplay deployments.
- Do not introduce TypeScript-owned SQL or a second schema owner.
- Do not expose arbitrary SQL as an admin or agent green path.
- Do not make modules create unchecked tables at runtime.
- Do not require the first local PostgreSQL validation to migrate existing
  SQLite data.
- Do not port correctness-sensitive queue/scheduler/transcript data before the
  backend abstraction and export/import path exist.
