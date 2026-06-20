# Runtime State Query APIs

Status: Implementation contract for task 2876

Date: 2026-06-20

## Purpose

Runtime diagnostics should consume typed persistence APIs, not SQLite tables.
This keeps the engine, Den adapters, admin surfaces, and future debug TUIs away
from storage-specific SQL and schema details.

## Query Surfaces

The persistence boundary exposes typed filters for:

- sessions;
- runtime instances;
- runtime events;
- routed messages;
- completion packets;
- delegated worker runs;
- runtime counters;
- queued-message recovery state;
- runtime text search.

Each query has a bounded default limit and stable sort order. Callers can page
with typed `limit` and `offset` options, but there is no raw SQL passthrough.

## Sort And Paging Rules

Stable ordering is part of the API contract:

- sessions sort by handle;
- instances sort by instance id;
- messages, events, and completion packets sort by sequence;
- worker runs sort by creation timestamp then run id;
- counters sort by scope and counter name;
- queued messages sort by enqueue timestamp then message id.

## Ownership

These APIs expose Rust-owned coordination state only. Den product data remains
owned by Den and should be queried through Den APIs.

Future PostgreSQL support should preserve these query shapes and move backend
differences behind the same persistence boundary.
