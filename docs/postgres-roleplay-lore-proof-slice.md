# PostgreSQL Roleplay Lore Proof Slice

Status: implemented proof slice for task 3489.

## Purpose

Roleplay lore should stay inside Crew-owned storage instead of becoming an
external lore database or a TypeScript-owned table island. This slice proves the
first typed `roleplay_lore` module/memory-space repository on SQLite and
PostgreSQL.

## Scope

The proof stores roleplay lore as typed Rust records with:

- `world`, `entity`, `lore_entry`, `relationship`, `timeline_event`, and
  `provenance_event` descriptor shapes;
- world, entity, session, and conversation-branch links;
- canon status and visibility filters;
- revision-checked replace;
- supersede history;
- tombstone retention;
- provenance events tied to evidence refs;
- backend-neutral bounded search.

SQLite uses ordinary text/JSON columns and `LIKE` search for the proof. The
PostgreSQL proof table uses `JSONB` and an internal generated `tsvector`, but
callers still send only typed filters plus plain query text. No `tsquery`, SQL,
JSONB operator, or physical table name is part of the caller contract.

## Tables

SQLite:

- `module_roleplay_lore_records`
- `module_roleplay_lore_provenance_events`

PostgreSQL proof schema:

- `module_roleplay_lore_records`
- `module_roleplay_lore_provenance_events`

## Diagnostics

PostgreSQL storage diagnostics now report `roleplay_lore` as a proof
module-owned store. This does not make PostgreSQL a production service backend;
full service boot remains blocked until required correctness-sensitive
repository groups are implemented or explicitly unsupported for a deployment
mode.

## Verification

```bash
cargo test -p rusty-crew-core-persistence \
  sqlite_roleplay_lore_conformance_matches_postgres_proof_contract \
  --features postgres-proof

source /home/system/database/rusty-crew-postgres.env
cargo test -p rusty-crew-core-persistence \
  postgres_roleplay_lore_proof_matches_sqlite_conformance_contract \
  --features postgres-proof -- --ignored --nocapture
```
