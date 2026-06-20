# Runtime Search Contract

Status: Implementation contract for task 2872

Date: 2026-06-20

## Scope

Runtime search indexes Rust-owned coordination history. It does not index Den
product data as if Rusty Crew owned it.

The initial SQLite implementation uses FTS5 behind `CoordinationStore` APIs.
Callers search through typed filters rather than raw SQL or SQLite-specific FTS
details.

## Indexed Rows

The first search index contains:

- routed agent messages;
- immutable session configuration snapshots.

High-volume brain text deltas are intentionally not indexed yet. They should
flow through a retention and compaction design before entering ordinary runtime
search.

## Filters

Search supports:

- free text;
- row type;
- session id;
- agent id;
- instance id;
- task id;
- event kind;
- recorded timestamp range;
- bounded result limits.

## Portability

SQLite FTS5 maps to PostgreSQL text search behind the same API. Metadata filters
should remain plain typed fields so a future storage backend can combine text
search with indexed relational filters without leaking backend-specific syntax
to the engine or TypeScript layer.
