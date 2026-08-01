# Task 6495 External Runtime Event Retention Certification

Date: 2026-08-01 (America/Los_Angeles)

## Debug SQLite Target

- Service: `rusty-crew-debug.service`
- API: `http://127.0.0.1:9348`
- Deployment role: `debug`
- Storage: SQLite at
  `/home/system/rusty-crew-debug/data/engine/coordination.sqlite3`
- Retention age: 14 days
- Terminal-turn batch size: 100
- Filesystem warning threshold: 10 percent free

The service had zero active sessions before restart. It was restarted from the
task checkout after the native release build and migrated from schema 57 to
schema 58. The first diagnostics read reported:

```text
external runtime events retained: 47,396
estimated retained event bytes: 24,566,456
terminal-turn checkpoints: 700
oldest retained cursor: 1 at 2026-07-11T09:32:30.964Z
newest retained cursor: 79,447 at 2026-08-01T13:07:36.355Z
filesystem free: 49 percent
filesystem warning threshold: 10 percent
filesystem warning active: false
```

The scheduler created the 700 terminal-turn checkpoints while applying the
configured policy. An explicit guarded maintenance request then returned the
same Rust-owned policy without applying hidden defaults:

```text
enabled: true
cutoff: 2026-07-18T13:07:55Z
terminal turn batch size: 100
terminal turns inspected: 0
terminal turns compacted: 0
events deleted: 0
oldest retained cursor: 1
```

The zero deletion result is expected because the scheduler had already drained
all eligible batches. The checkpoint table and cursor allocator remained
available in diagnostics after service restart. After the bounded scheduler
batches reached steady state, diagnostics reported 22,016 retained events,
10,275,023 estimated retained bytes, and 1,365 checkpoints. The newest cursor
remained 79,447 rather than being renumbered after deletion.

## Isolated PostgreSQL Target

The full disposable-schema PostgreSQL conformance lane ran against the local
test server rather than either Crew service database:

```bash
RUSTY_CREW_POSTGRES_BACKEND_DATABASE_URL=postgres://rusty_crew@127.0.0.1:5433/rusty_crew_dev \
  npm run test:postgres-backend
```

Result:

```text
postgres migration catalog: 2 passed
postgres isolated integration cases: 34 passed
failed: 0
```

The external-runtime lifecycle case exercises the PostgreSQL schema migration,
terminal-only compaction, checkpoint creation, retained lifecycle events,
monotonic cursor allocation, and restart-safe paging contract through a unique
throwaway schema that is dropped after the test.

## Deterministic Coverage

The SQLite focused contract proves that:

- active turns are never compacted;
- partial retention policies are rejected;
- terminal compactable events are deleted only with a checkpoint;
- lifecycle events remain replayable;
- old cursors can resume after compaction;
- sequence allocation remains monotonic before and after reopen;
- filesystem headroom warnings are diagnostic and do not reject event writes.

The production service on port 9347 was not changed or restarted. Retention is
enabled only on the debug service until this task completes review.
