# Production Wake Path Contract

Date: 2026-06-20

Status: Implementation contract for task 2830.

## Scope

This contract defines the first production wake lifecycle. It replaces the
diagnostic helper flow used by smoke tests with the real manifest path:
registered brain implementation, Rust-owned scheduling, buffered wake request,
TS brain execution, streamed brain events, returned brain actions, and Rust
action execution.

The unified architecture is authoritative. Rust owns deterministic
coordination. TypeScript owns the brain island, tools, and platform adapters.

## Lifecycle

1. The service initializes one Rust `CoreEngine`.
2. TS registers one or more brain implementations through
   `register_brain_implementation`.
3. Platform adapters inject external events or Den product-data updates, or
   Rust publishes internal bus events such as `AgentMessageRouted`.
4. The Rust scheduler observes bus events.
5. For each affected active session, Rust projects `BodyState` and evaluates
   the wake threshold.
6. If the wake is valid, Rust creates a wake ticket with a stable `wake_id`.
7. The bridge builds a `BrainWakeRequest` with runtime-buffer handles for:
   `body_state`, `system_prompt`, and `role_assembly`.
8. The bridge invokes the registered TS brain implementation.
9. TS hydrates and releases all buffers exactly once.
10. TS submits brain events through `submit_brain_event`.
11. TS submits the final `BrainActionBatch` through `submit_brain_actions`.
12. Rust validates actions against current session state and executes accepted
    actions against the internal bus and coordination store.
13. Rust processes any queued next-wake deltas according to
    `frozen_snapshot_next_wake`.

## Wake Inputs

### body_state

Producer: Rust.

For v1, `body_state` is the JSON serialization of
`CoreEngine::project_body_state(session_id)`. It must include the frozen
session snapshot, pending messages, recent events, and the active
`BodyDeltaPolicy`.

The body state is frozen for the wake. New events that arrive while the brain
is active are candidates for a later wake and must obey body-owned TTL/cap
policy.

### system_prompt

Producer: profile/brain registration assembly, stored or resolved through the
Rust-owned session/brain binding and passed through the bridge as text.

The current scaffold does not yet include full pi-profile prompt assembly.
Until that lands, the first implementation may use a minimal registered prompt
string, but the prompt must be part of the registered profile/brain input and
not hard-coded inside the scheduler.

### role_assembly

Producer: profile/role assembly layer.

For v1 this is a JSON object compatible with `BrainRoleAssembly`:

```json
{
  "instructions": "string",
  "initialMessages": []
}
```

The scheduler must not invent role semantics. It should consume the selected
profile/role assembly output and include it in the buffered wake payload.

## Registration Contract

`register_brain_implementation` records:

- implementation id;
- profile id;
- selected `ToolProfile`;
- model config;
- the callable TS brain implementation or callback handle.

Duplicate implementation ids should fail with `AlreadyExists`. A session wake
must resolve to exactly one registered brain/profile binding or fail with a
typed `BrainUnavailable`/`NotFound` error.

The selected tool profile must become durable enough for later wake/tool
filtering. The current `SessionState` does not retain `ToolProfile`, so
implementation work must add a registry or persisted binding rather than
assuming the profile can be recovered from `SessionState`.

## Scheduler Contract

The scheduler must:

- ignore archived sessions;
- use `DefaultWakeThreshold` or an explicit profile threshold policy;
- handle explicit `BrainWakeRequested` events for delegated sessions;
- generate idempotent `wake_id` values;
- avoid replaying expired queued messages;
- avoid waking sessions without a registered brain/profile binding;
- not require Den to be available.

For the first path, `AgentMessageRouted` and delegated
`BrainWakeRequested` are sufficient trigger events. Den and external adapter
events can use the same machinery after platform adapter registration is real.

## Diagnostic Helper Boundary

The following current helpers may remain for tests and smokes, but cannot be
the production wake loop:

- `createSession`;
- `routeAgentMessage`;
- `projectBodyStateJson`;
- `submitBrainActionsJson`;
- `assertNoBufferLeaks`.

The production proof in task 2838 should fail if it relies on these helpers for
the central wake lifecycle.

## Validation Target

The first end-to-end proof is:

```text
internal trigger event
  -> Rust scheduler
  -> buffered bridge wake
  -> TS brain
  -> submitted brain events
  -> submitted action batch
  -> Rust action execution
```

This proof may use a deterministic TS brain if needed, but it must exercise the
same registered-brain and `wake_brain` path that pi-agent-backed brains will
use.
