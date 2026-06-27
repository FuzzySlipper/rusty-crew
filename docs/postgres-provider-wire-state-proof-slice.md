# PostgreSQL Provider Wire-State Proof Slice

Date: 2026-06-26

## Purpose

Task 3485 adds provider wire-state coverage to the narrow PostgreSQL proof
store. This is still not the full service backend. It proves that the
provider-state repository can preserve the SQLite API contract on PostgreSQL
before the broader runtime is wired to use PostgreSQL for ordinary service
boot.

## Repository Contract

Provider wire state is keyed by:

- `session_id`
- `module_id`
- `strategy_id`

The payload remains provider-owned JSON. Rust persists the opaque payload,
payload version, payload encoding, profile fingerprint, provider fingerprint,
expiry, last wake id, and invalidation metadata. Rust does not interpret the
provider payload beyond JSON round-tripping and byte accounting for
diagnostics.

Wake lookup must never return stale current state:

- missing state returns `Missing`;
- expired state is invalidated and returns `Expired`;
- profile fingerprint mismatch is invalidated and returns `Invalidated`;
- provider fingerprint mismatch is invalidated and returns `Invalidated`;
- looking up another module or strategy for the same session invalidates the
  previous current row and returns `Missing` for the requested key.

## PostgreSQL Proof Table

The proof schema owns `provider_wire_states` with:

- `row_id BIGSERIAL PRIMARY KEY`
- stable key columns: `session_id`, `module_id`, `strategy_id`
- fingerprint columns: `profile_fingerprint`, `provider_fingerprint`
- opaque payload columns: `payload_version`, `payload_json`,
  `payload_encoding`
- lifecycle columns: `created_at`, `updated_at`, `expires_at`,
  `last_wake_id`, `invalidated_at`, `invalidation_reason`

The partial unique index on current rows matches SQLite behavior:

```sql
CREATE UNIQUE INDEX ... ON provider_wire_states(session_id, module_id, strategy_id)
WHERE invalidated_at IS NULL;
```

## Verification

The shared conformance test runs against SQLite and the PostgreSQL proof store.
It covers:

- replacement and supersession;
- wake-time expiry;
- maintenance expiry;
- profile fingerprint invalidation;
- provider fingerprint invalidation;
- explicit clear;
- module and strategy change invalidation;
- diagnostics ordering and payload byte accounting.

Local PostgreSQL verification:

```bash
source /home/system/database/rusty-crew-postgres.env
cargo test -p rusty-crew-core-persistence --features postgres-proof \
  postgres_provider_wire_state_proof_matches_sqlite_conformance_contract \
  -- --ignored --nocapture
```

The proof diagnostics mark repository group `provider_state` as implemented for
this typed conformance surface only. Full service boot must still fail closed
for unsupported PostgreSQL repositories until later integration tasks wire the
backend through the service.
