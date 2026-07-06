# PostgreSQL Fresh Backend Exercise Against den-k8

Status: exercise record for task 3419

Date: 2026-06-26

Design sources:

- ADR 0020, `storage-backend-abstraction-and-postgresql-readiness`
- Den doc `den-network/rusty-crew-postgres-service`
- `docs/postgres-runtime-counter-backend-slice.md`

## Result

The den-k8 PostgreSQL development service was exercised through the implemented
PostgreSQL runtime-counter backend slice. This is fresh-backend conformance, not a
SQLite migration and not a full service cutover.

The backend used the existing PostgreSQL env file:

```bash
set -a
. /home/system/database/rusty-crew-postgres.env
set +a
cargo test -p rusty-crew-core-persistence --features postgres-backend \
  postgres_runtime_counter_backend_matches_typed_counter_contract -- --ignored --nocapture
```

The test passed. It creates a unique temporary schema, migrates only backend-owned
objects, exercises typed runtime-counter increment/query/reset/summary behavior,
checks PostgreSQL backend diagnostics, and drops the schema afterward.

No SQLite service data was migrated or mutated.

## Service Cutover Status

The main Rusty Crew service is not ready to run with PostgreSQL as the selected
coordination backend. That is intentional at this stage.

`RUSTY_CREW_STORAGE_BACKEND=postgres` remains parsed but rejected by the service
configuration layer. The service smoke covers this fail-closed behavior so a
configured PostgreSQL backend cannot silently fall back to SQLite or pretend to
cover repositories that have not been ported.

## Implemented PostgreSQL Surface

Current implemented PostgreSQL surface:

- runtime counter backend store;
- backend-owned migration table;
- backend-owned `runtime_counters` table;
- runtime counter typed API parity for increment, query, reset, and summary;
- backend diagnostics with backend label, schema version, table counts,
  capabilities, and the `runtime_counters` repository group.

Current unsupported surface:

- full `CoordinationStore` boot path;
- session and identity records;
- event history and projections;
- internal messages and queued messages;
- scheduler and job claims;
- worker/delegated runs and completion packets;
- tool telemetry;
- provider wire state;
- runtime search;
- transcript/conversation trees and attachments;
- profile registry and dense memory;
- module schema registry beyond the backend-owned metadata table;
- import/export and migration.

## Production Readiness Boundary

PostgreSQL should not be treated as production-ready for Rusty Crew until the
full coordination repository set has either:

- PostgreSQL implementations with shared conformance coverage; or
- explicit unsupported capability diagnostics that keep the service from
  accepting workloads that would need those repositories.

Correctness-sensitive areas need dedicated tests before porting:

- queue TTL, terminal states, purge markers, and no-resurrection behavior;
- scheduler claim semantics under concurrent workers;
- transcript ordering, branch heads, attachments, and pagination;
- runtime search contracts across SQLite FTS5 and PostgreSQL search;
- provider wire state expiry and fingerprint invalidation.

## Follow-Up Shape

Follow-up implementation should keep the current rule: SQLite remains the local
default and PostgreSQL expands through typed repository coverage. The next useful
PostgreSQL slices are storage-admin diagnostics/module metadata and another
low-risk repository before queues, scheduler claims, transcripts, or search.
