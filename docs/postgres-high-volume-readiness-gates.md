# PostgreSQL High-Volume Readiness Gates

Status: implemented diagnostics/readiness slice.

Task: Den `rusty-crew` #3490.

## Purpose

Rusty Crew has PostgreSQL proof slices for several high-volume repositories, but
PostgreSQL is not yet the full service coordination backend. The admin
diagnostics surface must make that obvious instead of letting operators or UI
clients infer production readiness from the presence of a database URL.

## Service Boot Gate

Full PostgreSQL service boot remains fail-closed:

- `RUSTY_CREW_STORAGE_BACKEND=postgres` with default `blocked` boot mode throws
  during service config validation.
- `storage.backend = "postgres"` in runtime config also throws unless
  `storage.postgres.bootMode = "proof_admin"`.
- `proof_admin` is a bounded diagnostics/proof mode, not a production backend.

The service does not silently fall back to SQLite when PostgreSQL is selected.
SQLite remains the active full-service backend until PostgreSQL repository
coverage is explicitly wired behind the service backend.

## Admin Diagnostics

`/v1/admin/diagnostics/storage` now exposes PostgreSQL readiness and coverage
metadata:

- `postgres.productionReadiness.ready`
- `postgres.productionReadiness.status`
- `postgres.productionReadiness.reasonCodes`
- `postgres.productionReadiness.blockers`
- per-repository `coverageStatus`
- `postgres.search`
- `postgres.moduleOwnedStores`

Coverage statuses are backend-neutral UI categories:

- `implemented`: ready for selected deployment mode;
- `proof`: Rust proof/conformance exists, but service backend wiring is not
  production-active;
- `degraded`: partial proof exists, but related stores are missing;
- `unsupported`: no PostgreSQL repository coverage for that group/store.

## Current PostgreSQL Coverage

Proof coverage exists for:

- runtime counters;
- simple-kv module-owned table;
- runtime search;
- provider wire state;
- conversation transcript/tree records;
- attachments and data-bank scopes;
- dense profile memory / `profile_dense` compatibility.
- roleplay lore typed module/memory-space records.

Unsupported or degraded coverage remains visible for:

- queues and messages;
- scheduler jobs;
- worker runs and completions;
- sessions/identities;
- events/projections;
- bindings;
- profile registry;
- import/export;
- generic typed memory spaces beyond the proofed `profile_dense` and
  `roleplay_lore` shapes.

`profile_dense` being proofed does not imply generic typed memory spaces are
implemented. `roleplay_lore` is proofed as a module-owned typed memory space,
but still does not make PostgreSQL a production service backend.

## Search Diagnostics

PostgreSQL search diagnostics report:

- `backend: "postgres_tsvector"`;
- `status: "proof"` in proof-admin mode;
- `status: "unsupported"` and `degraded: true` when full service Postgres boot
  is blocked.

No admin or tool surface exposes `tsquery`, SQL, JSONB operators, table names,
or backend-specific query fragments as the portability contract.

## Verification

```bash
npm run typecheck
npm run -w @rusty-crew/brain-island smoke:admin-diagnostics-api
npm run -w @rusty-crew/brain-island smoke:service-config
```
