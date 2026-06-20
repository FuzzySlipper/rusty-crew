# Runtime Event Log And Projections

Status: Implementation contract for task 2871

Date: 2026-06-20

## Contract

`event_history` is the append-only runtime fact log. Projection tables are
derived indexes that make common queries cheap and can be rebuilt from event
JSON if their shape changes.

The current persisted fact row owns:

- monotonic runtime sequence;
- event kind;
- recorded timestamp;
- full serialized `CoreEvent`.

Projection indexes currently cover:

- session ids;
- agent ids;
- runtime instance ids;
- correlation ids;
- source wake ids.

## Facts Versus Projections

Facts are records that should survive replay exactly: session creation/archive,
routed messages, delegation lifecycle events, brain wake/action observations,
completion packets, and accepted tool-call telemetry.

Projection tables are query aids. They should not become the source of truth for
runtime behavior. If a projection is wrong, the repair path is to truncate and
rebuild it from `event_history`, not to mutate the fact log.

## Replay And Hydration

Hydration currently loads persisted `CoreEvent` history back into the in-memory
bus in sequence order. Future replay work can use the same fact log to rebuild
projection tables, body summaries, search indexes, and retention views.

Runtime-specific Den product events remain excluded from coordination
persistence. Den product state should be read from Den rather than mirrored into
Rusty Crew event history.

## Retention

High-volume events, especially text deltas, should be compacted through derived
projection or summary tables rather than by editing historical fact rows in
place. If retention needs to delete old facts, it should do so by an explicit
maintenance policy that also deletes/rebuilds affected projections.
