# ADR 0016: Runtime Clock Uses RFC3339 UTC With Fixed Test Injection

Status: Accepted

Date: 2026-06-20

## Context

The parity audit flagged `CoreEngine::now()` returning the literal
`system-clock-placeholder` for `ClockConfig::System`. That would corrupt
session timestamps, worker-run records, queued-message expiry, diagnostics, and
any sorted runtime history.

Current code has moved on: `ClockConfig::System` now formats
`OffsetDateTime::now_utc()` as RFC3339, while `ClockConfig::Fixed` remains the
deterministic test seam. A regression test named
`system_clock_writes_rfc3339_timestamps` verifies session and worker-run
timestamps are parseable and not the placeholder string.

## Decision

Runtime timestamps use RFC3339 UTC strings.

`ClockConfig::System` is the production clock. `ClockConfig::Fixed` is the
accepted deterministic injection seam for tests, smokes, and deterministic
bridge proofs.

Any placeholder timestamp that can enter persistence, events, queues, or
operator-visible diagnostics is production-blocking and must have a linked Den
task before handoff.

## Serialization Rules

- Persist timestamps as RFC3339 UTC strings.
- Use the same format for bridge contracts and TypeScript contract types.
- Preserve fixed-clock tests for deterministic assertions.
- Prefer passing timestamps from the Rust engine into records rather than
  letting unrelated layers invent wall-clock strings.

## Consequences

Task 2826 is stale as a bug report against current code and can be closed as
fixed by implementation. The policy remains important: future fake clocks are
allowed only through explicit test injection, not as placeholder production
values.
