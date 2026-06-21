# 2824 Architecture Decision Index

Status: Closeout index for task 2824

Date: 2026-06-20

## Purpose

Task 2824 collected open architecture questions from the parity audit. This
index records the final outcome for each question after re-checking current
Rusty Crew code and docs.

The parity audit remains useful as feature inventory, but the unified
architecture and the ADRs below are authoritative for current implementation
planning.

## Decision Map

### Production `wake_brain` And Scheduler Ownership

Outcome: decided.

References:

- `docs/adr/0004-wake-scheduler-ownership.md`
- `docs/production-wake-path-contract.md`
- Task 2988

Decision: Rust owns wake eligibility, lifecycle checks, wake ids, stale-wake
suppression, body projection, queue TTL/cap policy, and action execution. The
bridge/runtime owns the transport callback into TypeScript and runtime buffer
leases.

Implementation continues through the production wake path tasks rather than a
new detached spike.

### Wake Buffer Assembly

Outcome: decided.

References:

- `docs/adr/0013-wake-buffer-assembly-ownership.md`
- `docs/runtime-buffer-ownership.md`
- Task 2989

Decision: Rust produces `body_state`; registered brain/profile input supplies
`system_prompt`; profile/role assembly supplies `role_assembly`; the bridge
owns buffer creation, leases, hydration, and release.

### `ToolProfile` Enforcement

Outcome: decided.

References:

- `docs/adr/0014-tool-profile-enforcement.md`
- `docs/tool-architecture-registry-rules.md`
- Task 2990

Decision: `ToolProfile` is the canonical per-session allowed-tool contract.
Rust owns the selected contract and auditability. TypeScript owns registry
selection, concrete tool implementations, model schema exposure, and defensive
filtering back to the Rust-projected profile.

### Delegation Receiver Replacement

Outcome: decided by delegation ADR set.

References:

- `docs/adr/0006-prime-agent-delegation-runtime.md`
- `docs/adr/0007-worker-pools-as-optional-capacity.md`
- `docs/adr/0008-delegated-session-lifecycle-guardrails.md`
- `docs/adr/0009-bounded-fan-out-delegation.md`
- `docs/adr/0010-delegation-den-observability-and-controls.md`
- Task 2991

Decision: do not port pi-crew's `rusty-core` / delegation receiver as a
TypeScript-owned spawning authority. Rust-owned direct delegated sessions are
the v1 mechanism. Prime/full agents remain first-class; worker pools are later
capacity infrastructure.

### Test Seams And Public Fakes

Outcome: decided, with implementation follow-up.

References:

- `docs/stubs-fakes-placeholders-policy.md`
- `docs/adr/0015-test-seams-and-public-exports.md`
- Den doc `rusty-crew/stubs-fakes-placeholders-policy`
- Task 2992
- Follow-up task 3036

Decision: failure-injection helpers such as `.failNext()` are test-support APIs,
not production APIs. Existing public fakes may remain during scaffolding, but
they should move behind `test-support` exports or smoke/internal modules before
production packaging.

### Timestamp Placeholder

Outcome: obsolete audit item; fixed in current code and documented.

References:

- `docs/adr/0016-runtime-clock-policy.md`
- Task 2993
- Task 2826

Decision: runtime timestamps are RFC3339 UTC. `ClockConfig::System` is the
production clock; `ClockConfig::Fixed` is the deterministic test seam. The old
`system-clock-placeholder` bug is closed as stale against current code.

### Multiple Engines / `EngineRegistry`

Outcome: deferred deliberately.

References:

- `docs/adr/0012-single-engine-service-scope.md`
- Task 2987

Decision: v1 has one `CoreEngine` per service process. Many agents, sessions,
profiles, adapters, brain registrations, and scoped resources live inside that
engine. `EngineRegistry` is only a future expansion point if concrete
in-process tenancy or lifecycle isolation requirements appear.

### Pi-Agent Steering And Follow-Up

Outcome: decided.

References:

- `docs/adr/0003-mid-turn-delta-policy.md`
- `docs/adr/0011-steer-followup-frozen-snapshot.md`
- `docs/queued-message-retention-state.md`
- Task 2994

Decision: do not port pi-agent internal steer/follow-up queues as durable
coordination. Rusty Crew uses frozen wake snapshots and body-owned next-wake
deltas with aggressive TTL/cap behavior. Expired messages are inspectable but
not redeliverable.

## Implementation Deferrals

The following decisions intentionally defer implementation:

- `EngineRegistry`: deferred until real in-process tenancy/isolation need.
- test-support export split: tracked by task 3036.
- full profile prompt/role assembly: should land through production wake and
  profile-loading work; ADR 0013 defines the owner boundary.
- worker pools: deferred by ADR 0007 until direct delegated sessions and prime
  agent flows are solid.

## Closeout

All 2824 open questions now have one of:

- an accepted ADR or design note;
- an implementation follow-up task;
- an explicit obsolete-audit correction.

New implementation work should reference these ADRs rather than treating the
parity audit text as current architecture.
