# Stubs, Fakes, And Placeholders Policy

Status: Accepted policy for task 2995

Date: 2026-06-20

## Purpose

Rusty Crew is being scaffolded in layers. Temporary stubs, fakes, and
placeholders are acceptable when they make a narrow slice testable, but they
must not disappear into the codebase as if they were production behavior.

This policy defines when they are acceptable and what tracking is required.

## Definitions

Stub: a deliberate incomplete implementation that returns a fixed value,
`not_implemented`, or minimal placeholder result.

Fake: an in-memory or deterministic implementation used for tests, smokes, or
local proof paths.

Failure-injection seam: a test helper that forces a fake or sink to fail, such
as `failNext`.

Placeholder: a temporary value used in a production-adjacent path, such as a
fake timestamp, synthetic id, or hard-coded prompt.

## Acceptable Uses

Stubs and fakes are acceptable when:

- the code path is test-only or smoke-only;
- the production caller receives an explicit typed unavailable/not-implemented
  error;
- the fake is injected through a narrow interface rather than hidden in runtime
  code;
- the failure mode is documented in the task or code marker;
- a Den task exists before merge or handoff for production-adjacent behavior
  that still needs implementation.

They block activation/runtime use when they would:

- corrupt persisted state;
- emit invalid timestamps or ids;
- drop, replay, or resurrect messages silently;
- bypass Rust-owned coordination, session, or `ToolProfile` authority;
- pretend an external side effect succeeded;
- make health/readiness look production-ready when a dependency is missing.

## Tracking Rule

Every intentional production-adjacent stub, fake, placeholder, or public
failure-injection seam must have a linked Den task before handoff.

The Den task should include:

- code location;
- failure mode;
- expected replacement shape;
- acceptance criteria;
- whether human design input is required.

Pure unit-test fakes do not need individual Den tasks when they live in test
files or a clearly named test-support module and cannot be imported through the
production package entrypoint.

## Code Marker Convention

Use a short marker only when the incomplete behavior is visible from production
or package exports:

```text
TODO(rusty-crew#1234): replace placeholder timestamp with injected clock; fake
timestamps corrupt persisted ordering.
```

For Rust, prefer a normal comment near the incomplete branch. For TypeScript,
prefer a comment near the exported fake or placeholder. Avoid noisy markers for
small local test doubles inside smoke files.

## Public Export Policy

Production package entrypoints should export production interfaces and
constructors. Test fakes and failure-injection helpers should move to one of:

- a `test-support` entrypoint;
- a smoke/test file;
- an internal module not exported from the package root;
- an explicit development-only package.

During scaffolding, public fakes may remain temporarily if existing smokes need
them, but each public failure-injection seam needs a Den task to move or rename
it before production packaging.

Acceptable public interfaces:

- minimal sink/client interfaces such as `DenProjectionSink`;
- production adapters and transport interfaces;
- deterministic builders that do not expose failure injection.

Test-support-only interfaces:

- `Memory*Sink` implementations with `.failNext()`;
- fake transports with `failNextOpen()`;
- fake browser/CDP/network providers that do not represent production
  dependencies;
- fake clocks except through the accepted `ClockConfig::Fixed` test seam.

## Clock And Time Placeholders

Time placeholders are production-blocking if they enter persisted state or
observable runtime events. `ClockConfig::Fixed` is the accepted deterministic
test seam. `ClockConfig::System` must produce parseable RFC3339 UTC timestamps.

## Queue Placeholders

Queue fakes are production-blocking if they can redeliver expired work. Any
queued-message fake must preserve TTL, cap, terminal states, and inspection
without redelivery.

## Review Checklist

Before handoff, ask:

1. Can this behavior run in production?
2. If yes, does it produce truthful state and side effects?
3. If no, is the unavailable/error path explicit?
4. Is the fake/stub hidden behind test support or linked to a Den task?
5. Would an expired message, stale wake, or placeholder timestamp be able to
   affect later runtime behavior?
