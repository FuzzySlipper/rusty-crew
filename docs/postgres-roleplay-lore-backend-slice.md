# PostgreSQL Roleplay Lore Backend Slice

Status: implemented backend slice for task 3489.

## Purpose

Roleplay lore should stay inside Crew-owned storage instead of becoming an
external lore database or a TypeScript-owned table island. This slice proves the
first typed `roleplay_lore` module/memory-space repository on SQLite and
PostgreSQL.

## Scope

The backend stores roleplay lore as typed Rust records with:

- `world`, `entity`, `lore_entry`, `relationship`, `timeline_event`, and
  `provenance_event` descriptor shapes;
- world, entity, session, and conversation-branch links;
- canon status and visibility filters;
- revision-checked replace;
- supersede history;
- tombstone retention;
- provenance events tied to evidence refs;
- backend-neutral bounded search.

SQLite uses ordinary text/JSON columns and `LIKE` search for the backend. The
PostgreSQL backend table uses `JSONB` and an internal generated `tsvector`, but
callers still send only typed filters plus plain query text. No `tsquery`, SQL,
JSONB operator, or physical table name is part of the caller contract.

## Tables

SQLite:

- `module_roleplay_lore_records`
- `module_roleplay_lore_provenance_events`

PostgreSQL backend schema:

- `module_roleplay_lore_records`
- `module_roleplay_lore_provenance_events`

## Diagnostics

PostgreSQL storage diagnostics now report `roleplay_lore` as an implemented
module-owned store. This does not make PostgreSQL a production service backend;
full service boot remains blocked until required correctness-sensitive
repository groups are implemented or explicitly unsupported for a deployment
mode.

## Verification

```bash
cargo test -p rusty-crew-core-persistence \
  sqlite_roleplay_lore_conformance_matches_postgres_backend_contract \
  --features postgres-backend

source /home/system/database/rusty-crew-postgres.env
cargo test -p rusty-crew-core-persistence \
  postgres_roleplay_lore_backend_matches_sqlite_conformance_contract \
  --features postgres-backend -- --ignored --nocapture
```
