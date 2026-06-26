# PostgreSQL Runtime Counter Proof Slice

Status: implemented as an optional proof slice for task 3414.

Design source: ADR 0020, `storage-backend-abstraction-and-postgresql-readiness`.

## Repository Choice

The first PostgreSQL proof repository is `runtime_counters`.

Reasons:

- runtime counters are diagnostic state, not lifecycle truth;
- the repository already has a small typed API: increment, query, reset, and summary;
- the SQL shape is simple enough to prove connection, migration, upsert, paging, and diagnostics;
- it avoids correctness-sensitive queue TTL, scheduler claim, transcript, and runtime search semantics.

This proof does not make PostgreSQL a full `CoordinationStore` backend.
`RUSTY_CREW_STORAGE_BACKEND=postgres` should continue to fail closed in the
service until the full coordination repository set has a backend implementation.

## Implementation

The proof lives behind the Rust feature `postgres-proof`:

- `crates/core/core-persistence/src/postgres_proof.rs`
- public type: `PostgresRuntimeCounterProofStore`
- config type: `PostgresRuntimeCounterProofConfig`

The store can connect through an environment-variable reference:

```rust
let config = PostgresRuntimeCounterProofConfig {
    database_url_env: "RUSTY_CREW_DATABASE_URL".to_string(),
    schema: "rusty_crew_counter_proof".to_string(),
};
let store = PostgresRuntimeCounterProofStore::connect_from_env(&config)?;
```

The migration creates only proof-owned objects:

- `<schema>.rusty_crew_storage_metadata`
- `<schema>.runtime_counters`

No SQLite data migration is attempted or required.

## Local Test

The ignored integration test uses the den-k8 development PostgreSQL service when
the database URL env is available.

```bash
set -a
. /home/system/database/rusty-crew-postgres.env
set +a
cargo test -p rusty-crew-core-persistence --features postgres-proof \
  postgres_runtime_counter_proof_matches_typed_counter_contract -- --ignored --nocapture
```

The test creates a unique temporary schema and drops it afterward.

## Out Of Scope

- queued messages and TTL/no-resurrection behavior;
- scheduler/job claim semantics;
- conversation/transcript trees;
- runtime search;
- full service startup with PostgreSQL selected;
- logical export/import or migration from the current SQLite database.
