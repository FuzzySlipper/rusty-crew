# PostgreSQL Profile Memory Proof Slice

Status: implemented as a PostgreSQL proof repository slice.

Task: Den `rusty-crew` #3488.

## What This Proves

The PostgreSQL proof store now covers the dense profile memory compatibility
path and its typed memory-space descriptor projection:

- `profile_memories`
- `profile_dense` descriptor projection

The proof store preserves the same Rust API contract used by SQLite:

- `add_profile_memory`
- `replace_profile_memory`
- `remove_profile_memory`
- `get_profile_memory`
- `list_profile_memory`

The shared SQLite/PostgreSQL conformance fixture covers:

- descriptor validation for `profile_dense`;
- profile and user target scopes;
- add, list, get, replace, and remove;
- expected-revision conflicts for replace/remove;
- duplicate-key rejection;
- profile-level record caps;
- key/content validation through `ProfileMemoryCaps`;
- profile isolation and target filtering.

## Storage Boundary

PostgreSQL stores `metadata_json` as JSONB internally, but callers do not use
JSONB syntax. The Rust API remains typed around `ProfileMemoryRecord`,
`ProfileMemoryWrite`, `ProfileMemoryQuery`, and revision-checked operations.

This slice does not introduce TypeScript migrations or raw SQL. Dense profile
memory remains Rust-owned persistence.

## Descriptor Scope

The proof store exposes `memory_space_descriptors()` with `profile_dense`.
This is intentionally a projection for the existing dense-memory compatibility
surface, not a generic memory-space CRUD repository yet.

`profile_dense` uses:

- profile and user scopes;
- expected revision conflict policy;
- candidate governance defaults for write operations;
- manual retention;
- summary prompt policy.

## Diagnostics

`storage_diagnostics()` now reports a `profile_memories` row count.

The `profile_memory` repository-group diagnostic now reports implemented
PostgreSQL proof coverage for `profile_dense` descriptor projection and dense
profile memory conformance. It remains a proof slice and is not yet the full
service backend.

## Verification

Run the normal local proof suite:

```bash
cargo test -p rusty-crew-core-persistence --features postgres-proof
```

Run the live PostgreSQL profile-memory proof after sourcing the local dev
database env:

```bash
source /home/system/database/rusty-crew-postgres.env
cargo test -p rusty-crew-core-persistence --features postgres-proof \
  postgres_profile_memory_proof_matches_sqlite_conformance_contract \
  -- --ignored
```
