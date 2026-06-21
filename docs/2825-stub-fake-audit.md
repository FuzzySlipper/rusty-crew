# 2825 Stub And Fake Audit

Status: Audit result for task 2996

Date: 2026-06-20

## Scope

Task 2825 is a visibility registry for load-bearing fakes and silent stubs.
This audit checks the current codebase against the original child tasks and the
stub/fake policy in `docs/stubs-fakes-placeholders-policy.md`.

The parent should remain open so future production-adjacent fakes can be
attached there.

## Results

### 2826: `CoreEngine::now()` Placeholder

Outcome: resolved; original finding is stale.

Current code:

- `CoreEngine::now()` returns `OffsetDateTime::now_utc().format(&Rfc3339)` for
  `ClockConfig::System`.
- `ClockConfig::Fixed` remains the deterministic test seam.
- Regression test `system_clock_writes_rfc3339_timestamps` checks session and
  worker-run timestamps.

Task status: closed.

### 2827: Shutdown No-Op

Outcome: resolved.

Current code:

- `CoreEngine::shutdown()` now iterates active sessions and archives them.
- The returned `archived_sessions` count is real.
- `CoreBus::shutdown_subscribers()` drops all active subscriber senders and
  returns the dropped count, releasing blocked receivers once buffered events
  are drained.
- `CoreEngine::shutdown_with_timeout()` publishes session archive events, closes
  subscribers, and reports `dropped_subscriptions`.
- `NativeBridge::shutdown_engine()` passes through `drain_timeout_ms`, delegates
  to `engine.shutdown_with_timeout()`, and clears local subscription handles.
- The mock bridge delegates to `engine.shutdown()`.

Timeout semantics:

- Shutdown is currently synchronous and in-process. Session archive events are
  published before subscriber senders are dropped, so there are no background
  joins to wait on yet. `drain_timeout_ms` is accepted and passed into the
  engine as the future bounded-join budget for when the engine owns async
  background tasks.

Task status: closed.

### 2828: Resource Limits Not Enforced

Outcome: resolved for the originally dangerous delegation/runtime cases.

Current code:

- `RequestDelegation` validation rejects empty `resource_limits.workdir`.
- `validate_delegation_invariants()` rejects delegation when parent
  `max_delegation_depth` is `Some(0)`.
- delegated sessions carry resolved `ResourceLimits`.
- `expire_delegated_sessions_at()` archives delegated sessions that exceed
  `max_duration_ms` and marks the worker run `expired`.
- TypeScript local code tools consume `workdir` and `maxDurationMs` from session
  resource limits.
- tests cover depth-zero rejection and delegated-session timeout expiry.

Task status: closed.

### 2829: No Scheduler / Activation Loop

Outcome: superseded/resolved by scheduler and wake-path work.

Current code:

- `CoreEngine::route_agent_message()` publishes `AgentMessageRouted`, evaluates
  `DefaultWakeThreshold`, and emits `BrainWakeRequested` for an active
  wake-capable target session.
- `run_scheduler_tick()` claims due scheduled wake jobs and emits
  `BrainWakeRequested`.
- delegated session creation emits `BrainWakeRequested` through the direct
  delegation lifecycle.
- tests cover routed-message wakes, archived-session suppression, and scheduled
  wake ticks.

Remaining production wake host work is tracked in the wake-path capability
tasks and ADRs, not in 2825. The old "no scheduler exists" tracking item should
not stay open.

Task status: closed.

### 3036: Public Failure-Injection Fakes

Outcome: resolved.

Current code:

- `@rusty-crew/adapter-den` exposes production adapter/channel APIs from its
  root entrypoint.
- `@rusty-crew/adapter-den/test-support` exposes
  `createMemoryDenProjectionSink`, `MemoryDenProjectionSink`,
  `createSimulatedDenChannelsTransport`, and `SimulatedDenChannelsTransport`.
- `@rusty-crew/brain-island` exposes production observation/control interfaces,
  producers, executors, and route handlers from its root entrypoint.
- `@rusty-crew/brain-island/test-support` exposes memory observation/admin
  audit/lifecycle sinks.
- Smokes import failure-injection helpers from test-support paths.

ADR 0015 remains the rule: future memory fakes and failure-injection helpers
belong behind explicit test-support exports or smoke/internal modules, not root
production package entrypoints.

Task status: closed.

## Other Hits Reviewed

Search hits for "placeholder" in top-level READMEs and docs describe scaffold
package areas such as codegen or adapter packages. They are not silent runtime
state corruption.

The `not_implemented("wake_brain")` native method is documented by the bridge
surface and production wake path decisions. It is an explicit unavailable
operation, not a silent fake success.

Smoke-file `TODO` strings are fixture content, not production TODO markers.

## Closeout

Known load-bearing cases are either closed, narrowed to a real remaining child
task, or covered by a future cleanup task. The 2825 parent should stay open for
new fakes and placeholders, as requested.
