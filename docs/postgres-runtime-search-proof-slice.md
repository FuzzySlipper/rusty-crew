# PostgreSQL Runtime Search Proof Slice

Status: implementation record for task 3484

Date: 2026-06-26

## Purpose

This records the PostgreSQL runtime-search proof slice. It proves that
PostgreSQL can satisfy the existing typed `RuntimeSearchFilter` /
`RuntimeSearchResult` API without exposing PostgreSQL search syntax to callers.

This is still not a full PostgreSQL `CoordinationStore` backend. The main
service remains fail-closed for PostgreSQL repositories that have not been
ported.

## Implemented Surface

The proof store now owns a `runtime_search_entries` table with:

- row type and row key;
- optional sequence;
- optional session, agent, instance, task, and event-kind metadata;
- recorded timestamp;
- title and body;
- generated `tsvector` search column;
- GIN index for full-text search;
- metadata index for typed filters.

The proof API exposes:

- `upsert_runtime_search_entry`;
- `search_runtime` using the same `RuntimeSearchFilter` shape as SQLite.

Callers provide a plain text `query` plus typed filters. They do not pass
`tsquery`, SQL fragments, table names, or backend-specific ranking controls.

## Contract Coverage

The PostgreSQL proof test covers:

- non-empty query validation;
- row-type filtering;
- session filtering;
- agent filtering;
- event-kind filtering;
- recorded timestamp bounds;
- bounded result limits;
- stable tie ordering by rank, timestamp, row type, and row key;
- proof diagnostics for `runtime_full_text_search` and the `runtime_search`
  repository group.

SQLite remains covered by the existing repository conformance suite for the
same typed runtime-search API.

## Boundary

At the proof-slice level, `runtime_full_text_search` is supported.

At the full service level, PostgreSQL runtime search should remain unsupported
until the full backend wiring and production-readiness diagnostics land. The
proof does not port event history, sessions, queues, transcripts, attachments,
or other source repositories to PostgreSQL.

## Validation

Live proof command:

```bash
set -a
. /home/system/database/rusty-crew-postgres.env
set +a
cargo test -p rusty-crew-core-persistence \
  postgres_runtime_search_proof_matches_typed_search_contract \
  --features postgres-proof -- --ignored --nocapture
```

The test creates a unique schema, migrates proof-owned search tables, inserts
typed search records, verifies query behavior, checks diagnostics, and drops the
schema afterward.
