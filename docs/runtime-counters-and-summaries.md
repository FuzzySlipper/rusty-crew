# Runtime Counters And Summaries

Status: Implementation contract for task 2873

Date: 2026-06-20

## Scope

Runtime counters are lightweight projections for health and activity views. They
are not the source of truth for runtime behavior. The source of truth remains
the append-only event log and durable session/run records.

Counters are updated when runtime facts are persisted, so overview queries do
not need to scan raw JSON event history or debug telemetry tables.

## Counter Scopes

Counters are stored by:

- runtime;
- agent;
- runtime instance;
- session.

This keeps overview reads cheap for both service-wide health and per-agent or
per-session activity.

## Current Counters

The initial projection covers:

- brain turns;
- accepted actions;
- wakes;
- tool calls;
- tool errors;
- delegation lifecycle outcomes;
- routed messages;
- completion packets;
- queue expirations, reserved for the queued-message retention task.

## Ownership

Counters are derived state. If a counter is wrong, the repair path is to rebuild
it from owned runtime facts, not to treat the counter table as authoritative.

Detailed debugging still belongs in event history, tool-call history, completion
packets, worker-run records, and future telemetry tables. Counters should stay
small enough for frequent UI and agent overview reads.
