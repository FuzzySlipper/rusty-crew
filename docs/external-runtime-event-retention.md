# External Runtime Event Retention

Rusty Crew stores normalized Codex app-server events so current turns can stream
through SSE and recent turns can be inspected after a client reconnect. These
events are not the authority for Codex thread transcripts or external-turn
recovery. Native thread state plus the durable `external_turns` correlation is
the recovery authority.

## Durability Classes

External runtime events have two retention classes.

Durable lifecycle and audit events remain in `external_runtime_events`:

- `thread_lifecycle`
- `turn_lifecycle`
- `usage`
- `compaction`
- `runtime_warning`
- `runtime_status`
- `unknown_native_notification`
- `unsupported_server_request`

High-volume replay and debug events are compactable after their native turn is
terminal:

- `assistant_text_delta`
- `reasoning_delta`
- `plan_delta`
- `item_lifecycle`
- `command_activity`
- `file_activity`
- `mcp_activity`
- `dynamic_tool_activity`

Compaction writes or updates one
`external_runtime_event_checkpoints` row for the native turn before deleting
compactable events in the same database transaction. The checkpoint records the
terminal correlation, covered sequence range, event counts by kind, estimated
payload bytes, cutoff, and checkpoint time. Late compactable events are folded
into the same accumulating checkpoint on a later maintenance pass.

Active and otherwise nonterminal turns are never eligible. Low filesystem
headroom is diagnostic only; it does not reject, cancel, or interrupt a turn.

## Cursor Safety

`external_runtime_event_cursors` owns the monotonic next sequence for each
runtime. Event sequence allocation never derives from the currently retained
maximum. Deleting retained rows therefore cannot reuse an SSE cursor, and a
client reconnecting with an old cursor simply advances across the gap to the
next retained event.

## Service Policy

Retention is disabled unless both settings are present:

```env
RUSTY_CREW_EXTERNAL_EVENT_RETENTION_AGE_DAYS=14
RUSTY_CREW_EXTERNAL_EVENT_RETENTION_TERMINAL_TURN_BATCH_SIZE=100
```

The scheduler supplies the current maintenance timestamp and computes the
cutoff from the configured age. The admin maintenance command can run the same
Rust-owned operation explicitly with:

- `compactTerminalExternalRuntimeEventsBefore`
- `externalRuntimeEventRetentionAt`
- `externalRuntimeEventTerminalTurnBatchSize`

All three explicit command fields are required together. No fallback cutoff or
batch size is applied by Rust.

Filesystem diagnostics use these settings:

```env
RUSTY_CREW_STORAGE_FILESYSTEM_WARNING_FREE_PERCENT=10
RUSTY_CREW_POSTGRES_BACKING_FILESYSTEM_PATH=/path/on-the-postgres-host
```

SQLite derives its backing path from the database file. PostgreSQL cannot infer
server filesystem capacity through the application connection, so its backing
path must be configured only when that path is visible to the Crew process.
Otherwise diagnostics report filesystem headroom as unavailable instead of
mislabeling service-host capacity as database capacity.

## Diagnostics

Storage diagnostics report:

- retained external event rows and estimated JSON bytes;
- checkpoint rows;
- oldest and newest retained cursor/time;
- backing filesystem total/free bytes and free percent;
- the configured warning threshold and whether it is active.

Maintenance reports include the applied cutoff and batch size, inspected and
compacted terminal turns, checkpoints created, events deleted, estimated bytes
reclaimed, and the oldest remaining cursor/time.
