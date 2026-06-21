# Delegated Resource Cleanup Loop

Status: Implementation note for task 2977

Date: 2026-06-21

## Purpose

Rusty Crew now exposes a first delegated-resource cleanup loop that can be
called without manual database surgery. Rust remains the authority for delegated
session lifecycle state; TypeScript only wraps the operation for scheduled/admin
hosts and adapter cleanup callbacks.

## Rust-Owned Cleanup

`CoreEngine::cleanup_delegated_resources` returns a
`DelegatedResourceCleanupReport` with:

- `terminal_archived`: delegated sessions whose worker run is already terminal
  but whose session still needed archiving;
- `orphaned_archived`: delegated sessions missing lineage or attached to an
  archived/missing parent;
- `expired_archived`: delegated sessions that exceeded their resource-limit
  duration;
- `resources_released`: reserved for future Rust-owned resource handles.

The existing restart cleanup and explicit timeout cleanup remain in place. The
new public cleanup method composes those paths and adds terminal-session
archival without fabricating completion packets or changing terminal worker-run
outcomes.

## Bridge Surface

The bridge exposes `cleanup_delegated_resources`, and the native TypeScript
wrapper exposes `cleanupDelegatedResources()`.

The report is compact evidence for scheduler/admin callers. It is not a Den
projection and does not rely on external display state.

## TypeScript Loop Wrapper

`runDelegatedResourceCleanup` calls the Rust cleanup operation and then runs
optional adapter cleanup callbacks. Adapter callbacks are the only place where
browser, MCP, or channel resources may be released from TypeScript.

The wrapper can publish `agent_activity.v1` work observations:

- `work_started`;
- `work_completed`;
- `work_failed` on runtime or adapter cleanup failure.

Observation is display-only. Cleanup success/failure is determined by the Rust
operation and adapter callback results, not by whether observation publishing
succeeds.

## Current Limits

Browser, MCP, and channel adapters do not yet share a durable resource registry,
so adapter cleanup is callback-based. Future adapter lifecycle state can plug
into the same wrapper without changing Rust delegated-session cleanup.

The loop intentionally does not:

- create completion packets;
- redeliver queued messages;
- mutate Den tasks/messages/docs as coordination truth;
- release adapter resources except through adapter-provided cleanup APIs.
