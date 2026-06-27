# PostgreSQL Full-Service Gap Audit

Status: audit for task 3502

Date: 2026-06-27

## Summary

The live Rusty Crew service still runs on SQLite. PostgreSQL is parsed in
service/runtime config, but full service boot is deliberately blocked. The
current PostgreSQL implementation lives behind the `postgres-proof` Cargo
feature in `crates/core/core-persistence/src/postgres_proof.rs`; it has grown
beyond its original runtime-counter proof, but it is still not wired as the
coordination backend used by `core-engine` or the TypeScript service host.

The cutover path should treat PostgreSQL as a first-class backend only after
three gaps close:

1. A backend-neutral storage facade exists for full service boot.
2. Missing correctness-sensitive repositories are implemented in PostgreSQL.
3. Diagnostics and smokes prove the service is using PostgreSQL, not falling
   back to SQLite.

## Current Live Deployment

- User systemd unit: `/home/agent/.config/systemd/user/rusty-crew.service`
- Current service root: `/home/agents/rusty-crew`
- Current repo working directory: `/home/dev/rusty-crew`
- Current env file: `/home/agents/rusty-crew/config/service.env`
- Current storage: omitted `storage` config, so SQLite defaults to
  `/home/agents/rusty-crew/data/engine/coordination.sqlite3`
- Observed live diagnostics:
  - `/v1/admin/diagnostics/persistence` succeeds with SQLite-style table counts
    and database byte size.
  - `/v1/admin/diagnostics/storage` is unavailable on the currently deployed
    process, so the live service should be redeployed before relying on newer
    storage diagnostics.

The target deployment requested by Patch is a fresh root:

```text
/home/system/rusty-crew
```

The current SQLite data should remain untouched and available for rollback. The
first PostgreSQL service should use a fresh database, not a SQLite migration.

## Full-Service Boot Blockers

PostgreSQL config is parsed in both service env and runtime config:

- `ts/packages/brain-island/src/service-config.ts`
- `ts/packages/brain-island/src/service-runtime-config.ts`

Both validators intentionally reject full PostgreSQL boot:

```text
RUSTY_CREW_STORAGE_BACKEND=postgres is parsed but not implemented for full
service boot; set RUSTY_CREW_POSTGRES_BOOT_MODE=proof_admin only for bounded
storage-admin diagnostics smoke mode
```

Rust service construction is still SQLite-only:

- `crates/core/core-engine/src/lib.rs` calls `CoordinationStore::open(...)`.
- `CoordinationStore` wraps one `rusqlite::Connection`.
- `CoordinationStore::open` and `CoordinationStore::open_file` are SQLite path
  constructors.

There is no full-service enum/trait facade such as `CoordinationBackend` that
can hold SQLite or PostgreSQL behind the same public repository APIs.

## Repository Coverage Inventory

The canonical repository group catalog is
`crates/core/core-persistence/src/repositories.rs`.

| Repository group | Full-service need | PostgreSQL state | Notes |
| --- | --- | --- | --- |
| `storage_admin` | schema/migrations/diagnostics | proof-only | Postgres proof store has schema metadata and diagnostics, but not full service selector wiring. |
| `sessions_identities` | sessions, durable agents, instances, restart hydration | missing | Full engine hydration depends on SQLite `sessions`, `agent_identities`, and instance/config tables. This is a hard blocker. |
| `events_projections` | core event log, indexes, body projection/search rows | missing | Runtime events are central to restart/debug/search behavior. Needs Postgres schema and API coverage. |
| `queues_messages` | queued messages, TTL, terminal purge, internal agent messages | missing | Hard blocker because stale messages must not resurrect. Needs claim/expiry semantics and conformance. |
| `scheduler_jobs` | scheduled jobs, run claims, stale claim expiry | missing | Hard blocker if scheduler is enabled. Needs row-level claim semantics. |
| `worker_runs_completions` | worker runs, completion packets, delegation evidence | missing | Needed for delegated work and durable completion packets. |
| `tool_telemetry` | tool call history/counters | missing | Degraded observability if absent, but current admin/tool views expect it. |
| `provider_state` | provider wire state for modular/Responses brains | proof-only | Proof store implements typed provider-state API and tests; not wired as backend. |
| `runtime_search` | runtime search read model | proof-only | Proof store uses Postgres `tsvector`; service still uses SQLite FTS path. |
| `conversations_attachments` | branches, slots, variants, snapshots, attachments, data-bank scopes | proof-only | Proof store implements conformance for conversations and attachments/data bank; not wired as backend. |
| `profile_memory` | dense profile memory and memory governance/proposals | partial proof | Profile dense memory is proofed. Memory proposals/governance/session memory remain SQLite/full-service gaps. |
| `bindings` | MCP, channel, adapter bindings | missing | Needed for restartable profile/channel/MCP state without file edits. |
| `profile_registry` | official create-profile registry records | missing | Current profile registry is SQLite migration v22. Postgres proof store does not implement profile registry. |
| `module_schema_registry` | installed module descriptors and module-owned tables | partial proof | Simple KV and roleplay lore proof tables exist; full module schema registry/install diagnostics need service wiring. |
| `import_export` | import batches, legacy mappings, logical import/export validation | missing | Needed before any migration/import path, but not required for fresh empty first boot if fail-closed. |
| `runtime_counters` | durable counters/summaries | proof-only | Original Postgres proof surface; not wired as backend. |

## Existing PostgreSQL Proof Coverage

`crates/core/core-persistence/src/postgres_proof.rs` already contains useful
PostgreSQL implementations and conformance tests for:

- storage/admin metadata and row-count diagnostics;
- runtime counters;
- simple KV module-owned data;
- runtime search through `tsvector`;
- provider wire state with current-record and expiry semantics;
- conversation message slots, variants, branches, branch state, snapshots, and
  jump resolution;
- attachments and data-bank scopes;
- profile dense memory;
- roleplay lore records and provenance events;
- a conversation concurrency proof across multiple Postgres connections.

These should be promoted carefully rather than discarded. The file name and
types are now misleading: `PostgresRuntimeCounterProofStore` is effectively a
Postgres repository prototype for several groups.

## Required Implementation Work

### 1. Backend Facade

Create a backend-neutral storage interface that can hold SQLite and PostgreSQL
implementations behind the same public API. Avoid forcing TypeScript or
`core-engine` to branch on raw backend names.

Likely shape:

- keep `CoordinationStore` as the public type if possible, but make it an enum
  or facade over `SqliteCoordinationStore` and `PostgresCoordinationStore`;
- preserve existing method names so bridge/native surface churn is minimized;
- keep backend-specific SQL in `core-persistence`;
- expose backend diagnostics through the existing storage diagnostics projection.

### 2. Promote Postgres Proof Store

Rename/extract `PostgresRuntimeCounterProofStore` into an internal
PostgreSQL backend module. Keep proof/conformance tests, but remove
proof-only naming from the implementation path once full service semantics land.

### 3. Fill Missing Core Repositories

Implement the missing correctness-sensitive repositories before unblocking full
service boot:

- sessions and identities;
- event log and projections;
- queued messages/internal agent messages with TTL and terminal safety;
- scheduler jobs and run claims;
- worker runs and completion packets;
- profile registry;
- MCP/channel/adapter bindings;
- memory proposals/governance/session memory if the active service uses them;
- import/export records or fail-closed deployment checks if omitted.

### 4. Selector And Fail-Closed Readiness

Only remove the full-service Postgres boot block once readiness checks can prove
the selected deployment mode has every required repository implemented.

Required behavior:

- omitted storage remains SQLite;
- `storage.backend=postgres` reads the URL only from the configured env var;
- missing env/schema/migration/coverage fails closed;
- diagnostics state active/configured backend and repository readiness;
- no hidden fallback to SQLite is allowed after selecting Postgres.

### 5. Fresh `/home/system/rusty-crew` Install

The new install should be generated from intentional config, not copied from
`/home/agents/rusty-crew`.

Minimum planned root:

```text
/home/system/rusty-crew/
  config/
  data/
  logs/
  run/
  artifacts/
  backups/
  site/
```

Profiles/skills may live under `config/` to match the current service shape,
but the deployment task should explicitly decide what is regenerated versus
copied.

## Tests Needed Before Cutover

SQLite must stay green:

```bash
cargo test -p rusty-crew-core-persistence
npm run typecheck
```

PostgreSQL must be proved with the local secret env sourced by the operator
environment:

```bash
source /home/system/database/rusty-crew-postgres.env
cargo test -p rusty-crew-core-persistence --features postgres-proof -- --ignored --nocapture
```

Full-service smoke must then run against a PostgreSQL-configured process rooted
at `/home/system/rusty-crew`, proving:

- storage diagnostics report active backend `postgres`;
- profile create/list/export works;
- profile start/session hydration survives restart;
- channel wake and queue TTL/terminal behavior works;
- scheduler claims do not double-run;
- tool call history records;
- provider wire state persists/clears/expires;
- conversations, memory, roleplay lore, and attachments/data bank use Postgres;
- no SQLite database file is created in the new service root.

## Task Mapping

- `3504`: backend selector/facade and fail-closed readiness.
- `3505`: sessions/events/queues/scheduler/worker completions/tool telemetry.
- `3506`: profile registry and runtime config stores.
- `3507`: conversations, memory, lore, attachments, provider state.
- `3508`: backend-neutral diagnostics and query tooling.
- `3509`: fresh `/home/system/rusty-crew` install assets.
- `3510`: full PostgreSQL conformance and smoke suite.
- `3511`: final one-service cutover, intentionally left for Patch review.
