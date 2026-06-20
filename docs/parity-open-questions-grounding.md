# Parity Open Questions Current Grounding

Date: 2026-06-20

Status: Task 2986 grounding note.

## Purpose

The parity audit `pi-crew-vs-rusty-crew-parity-2026-06-19` is useful as a
baseline, but it predates several Rusty Crew commits. This note maps the open
questions from task 2824 to current code reality before implementation work
uses the audit as if it were still exact.

The authoritative architecture remains the Den document
`rusty-crew-unified-architecture`: Rust owns deterministic coordination,
TypeScript owns the brain island and platform adapters, and Den is product
data plus observability rather than the coordination bus.

## Current Findings

### Wake brain and scheduler

Still true: production `wake_brain` is not wired through the native bridge.
`NativeBridge::wake_brain` validates the three runtime buffer handles, then
returns `not_implemented("wake_brain")`. The TypeScript native facade also
exports `wakeBrain` as unavailable.

Stale or changed: the audit says `CoreEngine` never emits
`BrainWakeRequested`. Current `CoreEngine::spawn_delegated_workers` now creates
delegated sessions and publishes `BrainWakeRequested` for wake-capable delegated
sessions. The remaining gap is the production scheduler/body loop that observes
events, evaluates thresholds, builds wake payloads, and dispatches through the
registered brain implementation.

Tracked by: tasks 2988, 2830, 2831, 2832, 2833, and 2838.

### Bridge wake buffers

Still true: the runtime buffer ownership protocol exists and is tested.
`RuntimeBufferStore::build_brain_wake_request` leases `body_state`,
`system_prompt`, and `role_assembly` buffers, and
`wakeBrainFromBridgeRequest` hydrates and releases each handle exactly once.

Still open: production code has not yet decided the single owner that produces
the three wake payloads. The diagnostic helper `project_body_state_json`
serializes `BodyState`, but the production wake path needs an explicit builder:
Rust projects the body state; profile/role assembly must be produced from the
registered brain/profile configuration without inventing a second buffer
protocol.

Tracked by: tasks 2989, 2830, 2832, and 2836.

### ToolProfile enforcement

Partly resolved by task 2846: `SessionState` now retains a `ToolProfile`, the
native bridge mirrors registered brain profile tools into the Rust engine, and
delegated sessions resolve their tool surface from the requested profile.
The TS pi-agent brain accepts a resolver for concrete tools and filters its
result back to the `ToolProfile` descriptors projected by Rust.

The remaining design direction is still profile-based tool enablement, not a
runtime `WorkerPolicy` allow/deny model. Future work should continue to keep
Rust as the canonical selected profile/session binding while TS supplies only
the concrete tools named by that selected profile.

Tracked by: tasks 2990 and the 2815 children, especially 2855, 2858, 2861,
2862, and 2864.

### Delegation receiver / rusty-core replacement

Changed: the old self-addressed string receiver concern is no longer the whole
story. `RequestDelegation` now causes Rust to create delegated sessions, persist
requested worker-run records, route the prompt to the delegated agent, and emit
`BrainWakeRequested`.

Still open: delegated sessions do not yet wake through the production
scheduler and bridge path. The runtime model should stay prime-agent plus
subagent delegation first, with worker pools as a later capability rather than
the central abstraction.

Tracked by: tasks 2991, 2840, 2843, 2844, and 2852.

### MemoryDenProjectionSink.failNext and test seams

Still true: `MemoryDenProjectionSink.failNext` is part of the public TS
interface exported from `adapter-den`. It is useful for smoke tests, but the
package boundary should distinguish production adapter exports from test-only
failure injection seams.

Tracked by: tasks 2992 and 2995.

### CoreEngine::now placeholder

Still true: `CoreEngine::now()` returns the literal
`system-clock-placeholder` for `ClockConfig::System`. This will corrupt
non-test session timestamps and worker-run timestamps.

Tracked by: tasks 2993 and 2826.

### Multiple engines per process

Current code has two different scopes:

- `CoreEngine` supports allocating multiple `EngineHandle` values through a
  process-global atomic.
- `NativeBridge` stores a single `Option<CoreEngine>` and rejects a second
  `initialize_engine` call.

The unified architecture says Rusty Crew is a single-process fleet: one runtime
service manages many full and worker sessions through one in-process bus. The
near-term architecture should keep one engine per service process and host many
agents/sessions inside it. An `EngineRegistry` is a future expansion point only
if a concrete need appears for hard in-process tenancy boundaries.

Tracked by: task 2987.

### Steering and follow-up

Already decided for v1: ADR 0003 accepts frozen snapshots plus a body-owned
next-wake queue with aggressive TTL. pi-agent steering/follow-up queues may be
used as transient turn-boundary mechanics, but they are not durable
coordination state and must not replay expired instructions.

Tracked by: task 2994 and the queue behavior design note.

## Implementation Sequencing

The dependency chain `2986 -> 2988 -> 2830 -> 2831 -> 2832 -> 2833 -> 2838`
is the right spine. After `2838` proves the production wake path, higher-level
capabilities such as delegated-session integration and first local-tool proof
can build on real activation instead of diagnostic bridge helpers.
