# PostgreSQL Repository Parity Boundary

Task: `rusty-crew#4259`

## Summary

`CoreCoordinationStore` still intentionally exposes a broad compatibility API
while SQLite and PostgreSQL are both being made first-class. New repository
parity work should move one correctness-sensitive group at a time behind a
crate-internal trait, then keep the public service-facing methods as thin
facades.

The first concrete boundary is runtime counters:

- `RuntimeCounterRepository` lives in `repos/runtime_counters.rs`.
- SQLite `CoordinationStore` and PostgreSQL `PostgresRuntimeCounterProofStore`
  both implement that trait.
- `CoreCoordinationStore` routes runtime-counter query, summary, and reset calls
  through one repository accessor instead of repeated two-arm enum dispatch.
- Summary projection is shared by the trait default so new counter fields cannot
  silently diverge between backends.
- A shared conformance helper runs against SQLite in normal offline tests and
  against PostgreSQL in an ignored live-DB test.

This keeps SQL dialect details inside the persistence crate and gives compiler
pressure for the selected group without exporting a broad public trait surface.

## Continuation Pattern

Use this order for the next groups:

1. Pick one repository group from `repositories.rs`.
2. Define a crate-internal trait in that group's `repos/*` module.
3. Implement the trait for SQLite and PostgreSQL stores.
4. Keep existing public methods as thin wrappers.
5. Move any shared projections or record-shape assembly into a backend-neutral
   helper when both backends currently duplicate it.
6. Add one shared conformance helper that runs offline against SQLite and behind
   the live PostgreSQL env gate for PostgreSQL.
7. Only then collapse `CoreCoordinationStore` enum dispatch for that group.

Avoid a single top-level storage trait until several repository groups have
settled. The useful boundary is the correctness concern, not the whole database.

## Suggested Next Groups

- `provider_state`: small surface, high correctness impact for modular brain
  restart and hot-swap behavior.
- `queues_messages`: more dangerous, but should follow once row-claim and TTL
  semantics are fully captured in the contract.
- `sessions_identities` plus `events_projections`: required for complete
  PostgreSQL service hydration and should be split only if the test contract can
  still prove restart behavior end to end.

