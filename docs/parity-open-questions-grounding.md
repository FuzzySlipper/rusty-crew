# Parity Open Questions Closeout

Date: 2026-06-20 (grounding snapshot); closeout update 2026-07-05.

Status: Superseded closeout for task 2986. The findings below were a 2026-06-20
grounding snapshot that mapped the 2824 open questions against code reality
before implementation. Every item has since been resolved by an accepted ADR
and/or landed code. Treat this file as a historical closeout, not current
architecture. For active policy use `docs/README.md`, the `docs/adr/` trail,
and the Den document `rusty-crew-unified-architecture`.

## Purpose

The parity audit `pi-crew-vs-rusty-crew-parity-2026-06-19` was useful as a
baseline but predated several Rusty Crew commits. This note originally mapped
the open questions from task 2824 to current code reality so implementation
work would not treat the audit as if it were still exact. The 2824 questions
are now closed; see `2824-architecture-decision-index.md` for the per-question
outcome map.

## Findings At Closeout

### Wake brain and scheduler — landed

The production wake path is wired. `CoreEngine::route_agent_message()` and
`run_scheduler_tick()` evaluate `DefaultWakeThreshold` and emit
`BrainWakeRequested` for active wake-capable sessions; delegated session
creation emits `BrainWakeRequested` through the direct delegation lifecycle.
Brain implementations are registered through `register_brain_implementation`;
`wakeBrainFromBridgeRequest` (`ts/packages/brain-island/src/bridge-wake.ts`)
hydrates the three runtime-buffer handles, calls the registered brain's
`wake`, and releases each handle exactly once; brain events and action
batches return through `submit_brain_event` / `submit_brain_actions`.

Note: the `NativeBridge::wake_brain` manifest operation itself still returns
`not_implemented("wake_brain")` — it validates the handle/buffer request shape
only, because the transport-neutral callback story is waiting on bridge
codegen. This is an intentional explicit-unavailable operation documented by
the bridge surface decisions, not a missing wake path. The live wake path runs
through the registered-brain executor binding and the Rust `openai-responses`
brain (`run_openai_responses_brain_json`), see ADR 0021.

Decision references: `adr/0004-wake-scheduler-ownership.md`,
`adr/0013-wake-buffer-assembly-ownership.md`, `production-wake-path-contract.md`.

### Bridge wake buffers — decided and landed

The runtime buffer ownership protocol is implemented and tested.
`RuntimeBufferStore::build_brain_wake_request` leases `body_state`,
`system_prompt`, and `role_assembly` buffers; `wakeBrainFromBridgeRequest`
hydrates and releases each handle exactly once. The single-owner question is
decided by ADR 0013: Rust produces `body_state`; registered brain/profile input
supplies `system_prompt`; profile/role assembly supplies `role_assembly`; the
bridge owns buffer creation, leases, hydration, and release.

Decision reference: `adr/0013-wake-buffer-assembly-ownership.md`,
`runtime-buffer-ownership.md`.

### ToolProfile enforcement — decided and landed

`SessionState` retains a `ToolProfile`; the native bridge mirrors registered
brain profile tools into the Rust engine; delegated sessions resolve their
tool surface from the requested profile; the TS pi-agent brain accepts a
resolver for concrete tools and filters its result back to the
Rust-projected `ToolProfile`. Tool availability is profile-based, not a
runtime `WorkerPolicy` allow/deny model.

Decision reference: `adr/0014-tool-profile-enforcement.md`,
`tool-architecture-registry-rules.md`.

### Delegation receiver / rusty-core replacement — decided and landed

`RequestDelegation` causes Rust to create delegated sessions, persist
requested worker-run records, route the prompt to the delegated agent, and
emit `BrainWakeRequested`. Delegated sessions wake through the same production
scheduler and bridge path as full sessions. The runtime model is prime-agent
plus subagent delegation first; worker pools are a later capacity layer, not
the central abstraction.

Decision references: `adr/0006`–`0010`, `end-to-end-delegated-slice.md`
(run locally with `npm run build:native` and `npm run smoke:delegated-slice`).

### MemoryDenProjectionSink.failNext and test seams — decided and landed

Failure-injection helpers are behind explicit test-support exports, not root
production entrypoints. `@rusty-crew/adapter-den/test-support` exposes
`createMemoryDenProjectionSink`, `MemoryDenProjectionSink`,
`createSimulatedDenChannelsTransport`, and `SimulatedDenChannelsTransport`;
`@rusty-crew/brain-island/test-support` exposes memory observation/admin
audit/lifecycle sinks. Smokes import failure-injection helpers from
test-support paths.

Decision references: `adr/0015-test-seams-and-public-exports.md`,
`stubs-fakes-placeholders-policy.md`. Closeout audit: `2825-stub-fake-audit.md`
(task 3036 resolved).

### CoreEngine::now placeholder — fixed

Resolved. `CoreEngine::now()` returns `OffsetDateTime::now_utc()` formatted as
RFC3339 for `ClockConfig::System`; `ClockConfig::Fixed` remains the
deterministic test seam. The old `system-clock-placeholder` bug is closed as
stale against current code.

Decision reference: `adr/0016-runtime-clock-policy.md`. Closeout audit:
`2825-stub-fake-audit.md` (item 2826).

### Multiple engines per process — decided

v1 has one `CoreEngine` per service process. Many agents, sessions, profiles,
adapters, brain registrations, and scoped resources live inside that engine.
`EngineRegistry` is only a future expansion point if concrete in-process
tenancy or lifecycle isolation requirements appear.

Decision reference: `adr/0012-single-engine-service-scope.md`.

### Steering and follow-up — decided

v1 uses frozen wake snapshots plus body-owned next-wake deltas with aggressive
TTL/cap behavior. pi-agent steering/follow-up queues may be used as transient
turn-boundary mechanics but are not durable coordination state; expired
messages are inspectable but not redeliverable.

Decision references: `adr/0003-mid-turn-delta-policy.md`,
`adr/0011-steer-followup-frozen-snapshot.md`, `queued-message-retention-state.md`.

## Implementation Sequencing — complete

The dependency chain `2986 -> 2988 -> 2830 -> 2831 -> 2832 -> 2833 -> 2838` has
been worked through to a real registered-brain wake path. Higher-level
capabilities (delegated-session integration, local-tool proof, Rust
`openai-responses` brain) build on real activation rather than diagnostic
bridge helpers.
