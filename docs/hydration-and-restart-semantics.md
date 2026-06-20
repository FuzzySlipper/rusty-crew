# Hydration And Restart Semantics

Status: Implementation contract for task 2874

Date: 2026-06-20

## Hydrated On Restart

Rusty Crew service restart hydrates Rust-owned runtime state from the
coordination store:

- durable agent, runtime instance, and session identity records;
- session state and immutable session configuration snapshots;
- append-only event history into the in-process bus;
- routed message history derived from event history;
- delegated run records and completion packets;
- runtime search rows and counters as persisted projections.

Multiple full agents and delegated sessions are expected to hydrate in the same
engine process without requiring a worker pool.

## Not Automatically Resumed

Restart must not invent new work.

Persisted `BrainWakeRequested` events remain visible as history, but an in-flight
brain call is not resumed unless a later scheduler contract explicitly owns a
restart-safe wake lease. Restart should not duplicate wake events, rerun old
action batches, or redeliver expired queued messages.

## Cleanup

On startup the engine performs bounded cleanup:

- delegated sessions whose parent is archived or missing are archived;
- matching worker-run records are moved to terminal cancelled/expired states;
- no fake completion packets are created for cleanup;
- normal active full agents and delegated sessions are left intact.

## Queue TTL Interaction

Queued messages are not yet durable runtime state. When a durable queue is added,
each message must carry body-owned TTL metadata, expiry state, and a query path
for expired messages. Hydration must never treat an expired queue item as a
fresh pending message.

The intended rule is: hydrate fresh queue items only, mark expired items
non-deliverable, and expose expired items through an operator/agent inspection
tool rather than replaying them.

## Idempotence

Restart should be observationally stable. Opening an engine against an already
valid store must not increase event counts, runtime counters, or search rows.
Projection repair is allowed only through explicit maintenance/rebuild tooling.
