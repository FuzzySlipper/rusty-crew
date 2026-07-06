# Core Persistence `lib.rs` Decomposition Plan

Status: active implementation plan for task #4327
Date: 2026-07-06

`crates/core/core-persistence/src/lib.rs` is still the dominant SQLite and
crate-entry file after the first repository split wave. It is smaller than the
older `storage-repository-split-map.md` snapshot, but at roughly 17.5k lines it
still mixes public contract types, backend facade wiring, SQLite migrations,
SQLite repository methods, row mappers, module-schema helpers, and broad tests.

This plan is the current split map for shrinking `lib.rs` while keeping SQLite
first-class. It complements `storage-repository-split-map.md`; where this doc
mentions current line ranges, those are an implementation aid rather than an API
contract.

## Current Shape

Current high-level line map:

| Range | Current content | Preferred owner |
| --- | --- | --- |
| `1-110` | crate docs, module declarations, public re-exports, imports, SQLite constants | `lib.rs` plus extracted modules |
| `112-276` | SQLite schema migration catalog | `sqlite/migrations.rs` |
| `277-323` | store handles and repository-set structs | `store.rs` or `facades.rs` |
| `324-2329` | `CoreCoordinationStore` and concern-facade forwarding methods | `facades.rs`, keeping `lib.rs` as re-export |
| `2340-4843` | public record/query/request/result contracts | `contracts/*` modules, re-exported by `lib.rs` |
| `4844-5834` | primary SQLite `CoordinationStore` methods, including simple_kv, queues, provider state, diagnostics, worker pool methods | SQLite domain modules, phased by concern |
| `5835-5979` | runtime counter repository implementation and module-schema install facade methods | `sqlite/runtime_counters.rs`, `sqlite/module_schema_registry.rs` |
| `5980-6503` | SQLite open/configuration, diagnostics, storage pressure, query plan, migration application helpers | `sqlite/admin.rs` and `sqlite/migrations.rs` |
| `6504-7702` | SQLite schema migration functions | `sqlite/migrations.rs` |
| `7703-7942` | module-schema physical install helpers | `sqlite/module_schema_registry.rs` |
| `7943-8388` | provider wire state SQLite helpers and row mappers | `sqlite/provider_state.rs` |
| `8389-8852` | logical import/export and legacy ID mapping helpers | `sqlite/import_export.rs` |
| `8853-9239` | runtime search, simple_kv, branch/session lookup helpers | `sqlite/runtime_search.rs`, `sqlite/module_data.rs`, shared SQLite helpers |
| `9240-10083` | memory/status/identity/worker parsing helpers and row mappers | domain modules matching the helpers |
| `10084-10162` | DDL column helpers and JSON/error helpers | `sqlite/admin.rs`, `sqlite/json.rs` |
| `10163-end` | large embedded test module | focused module tests after subject modules settle |

Existing `repos/*` files already own a meaningful part of the target structure:

- `repos/sessions.rs`
- `repos/events.rs`
- `repos/queued_messages.rs`
- `repos/scheduler.rs`
- `repos/worker_runs.rs`
- `repos/runtime_counters.rs`
- `repos/service_config.rs`
- `repos/conversations.rs`
- `repos/attachments.rs`
- `repos/memory.rs`
- `repos/roleplay_lore.rs`

Those modules should remain the foundation for backend-neutral conformance and
repository behavior. New SQLite implementation modules should not duplicate
their conformance contracts; they should either use them or make the remaining
SQLite-only behavior easier to attach to them.

## Boundary Rules

Backend-neutral contracts:

- record/query/request/result types that describe Crew storage behavior;
- repository-set facade structs and public concern group names;
- conformance traits and fixtures shared by SQLite and Postgres;
- error/status enums that callers already consume.

SQLite implementation:

- `rusqlite` imports and `params!` usage;
- DDL and migration functions;
- row mappers from `rusqlite::Row`;
- SQLite FTS/query-plan/pragma behavior;
- dynamic identifier whitelists;
- SQLite maintenance and pressure diagnostics.

Crate entrypoint:

- module declarations;
- public re-exports;
- short crate-level docs;
- no large SQL blocks;
- no broad test module.

## Extraction Order

1. **Inventory slice (#4423):** land this plan and task series.
2. **Backend-neutral contracts (#4424):** move public type definitions into
   named modules such as `contracts/common.rs`, `contracts/conversation.rs`,
   `contracts/memory.rs`, `contracts/runtime.rs`, and `contracts/workers.rs`.
   Keep `pub use` in `lib.rs` so callers do not churn.
3. **SQLite migrations/admin (#4425):** move migration catalog, migration
   application, schema readbacks, startup fail-closed checks, and migration
   tests. Keep connection pooling/opening concerns separate from migration SQL.
4. **SQLite facade/runtime admin (#4426):** move open/configuration,
   `CoordinationStore` runtime diagnostics, table-count diagnostics,
   maintenance, query-plan checks, and pressure-signal helpers.
5. **SQLite repository domains (#4427):** split the remaining SQLite domain
   methods in small sub-slices. Start with lower-risk provider state or
   simple_kv/module-data helpers, then queues/scheduler/workers, then
   conversation/memory/lore domains where tests are heavier.
6. **Tests (#4428):** move the large embedded `tests` module after subject
   code has stable homes. Preserve behavior-based test names.

## Suggested SQLite Module Layout

The target does not need to mirror Postgres file names exactly. Prefer Crew
storage concerns:

```text
src/
  lib.rs                    # crate docs, module declarations, public re-exports
  contracts/
    mod.rs
    common.rs
    conversation.rs
    memory.rs
    runtime.rs
    workers.rs
  store.rs                  # CoreCoordinationStore and repository-set facades
  sqlite/
    mod.rs
    admin.rs                # open/config, diagnostics, maintenance, query plans
    migrations.rs           # catalog + DDL + migration tests
    provider_state.rs
    module_data.rs          # simple_kv and module schema registry install helpers
    import_export.rs
    runtime_search.rs
    workers.rs
    queues.rs
    sessions.rs             # only if useful after repos/sessions.rs reuse
```

Avoid creating a permanent `sqlite/misc.rs`. A temporary module is acceptable
only inside one task if the handoff immediately names the next split.

## Validation Plan

Use focused validation first, then broader gates according to blast radius:

- Contracts-only extraction:
  - `cargo test -p rusty-crew-core-persistence --lib`
  - `cargo test -p rusty-crew-core-persistence --features postgres --lib`
- SQLite migration/admin extraction:
  - `cargo test -p rusty-crew-core-persistence --lib fresh_database_applies_all_schema_migrations -- --nocapture`
  - `cargo test -p rusty-crew-core-persistence --lib version_one_database_migrates_to_current_schema -- --nocapture`
  - `cargo test -p rusty-crew-core-persistence --lib future_schema_version_fails_closed -- --nocapture`
  - `cargo test -p rusty-crew-core-persistence --lib failed_schema_migration_rolls_back_partial_ddl -- --nocapture`
  - `cargo test -p rusty-crew-core-persistence --lib`
- Repository-domain extraction:
  - the matching `repos::<domain>` tests;
  - matching `sqlite_*_conformance_matches_postgres_backend_contract` tests when present;
  - `cargo test -p rusty-crew-core-persistence --features postgres --lib` for shared contract drift.
- Before commit on implementation slices:
  - `cargo fmt --all --check`
  - `cargo clippy -p rusty-crew-core-persistence --features postgres --all-targets -- -D warnings`

When a slice touches only docs/task metadata, no cargo validation is required;
`git diff --check` is enough.

## Guardrails

- Do not make SQLite a legacy path. The debug service and small deployments rely
  on it being a real backend.
- Do not move behavior and change semantics in the same slice.
- Do not expose SQLite-specific types through backend-neutral contracts.
- Do not weaken dynamic SQL whitelisting while moving helpers.
- Keep `rusqlite` and SQL literals inside `core-persistence` unless a future
  Rust storage crate is explicitly approved.
- Preserve Postgres feature builds after contract moves; Postgres imports many
  `lib.rs` contracts and helper exports.
