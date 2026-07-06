# PostgreSQL Connection Pool And Reconnect Strategy

Task: `rusty-crew#4260`

## Summary

The PostgreSQL backend uses a small synchronous in-process connection pool. This
matches the current blocking Rust persistence implementation without introducing
an async runtime or external pool dependency.

Behavior:

- `EngineStorageConfig::Postgres.max_connections` controls the pool size.
- The default pool size is conservative.
- Checkout reuses idle open clients when available.
- Closed idle clients are discarded and replaced by a fresh connection.
- If a backend connection is killed while an operation is in flight, that
  operation may fail once with a transient PostgreSQL connection error; the lease
  is then discarded and the next checkout reconnects.
- Pool exhaustion fails explicitly instead of silently creating unlimited
  clients.
- Schema incompatibility errors keep their schema/version wording and are not
  classified as transient connection failures.

The current strategy intentionally does not replay arbitrary failed writes. Some
operations are not safely replayable without operation-specific idempotency
proofs. Repository slices can add targeted retry later where the contract proves
the operation is idempotent.

## Diagnostics

`RuntimeStorageDiagnostics.connection_health` reports:

- backend
- status (`healthy`, `degraded`, or `exhausted`)
- max, active, and idle connection counts
- total opened connections
- checkout and reuse counts
- reconnect attempts and successes
- discarded closed connection count
- last pool/connection error

SQLite reports a stable single-connection healthy projection so admin clients can
render one backend-neutral shape.

## Live Test

The live reconnect proof terminates an idle PostgreSQL backend connection,
allows the next operation to observe or discard it, and verifies a subsequent
operation succeeds without restarting the service:

```bash
RUSTY_CREW_POSTGRES_PROOF_DATABASE_URL=... \
  cargo test -p rusty-crew-core-persistence \
  --features postgres-proof \
  postgres_connection_pool_recovers_after_closed_idle_connection \
  -- --ignored --nocapture
```

