# ADR 0015: Keep Failure Injection Out Of Production Entry Points

Status: Accepted

Date: 2026-06-20

## Context

Current TypeScript packages expose several useful memory fakes and
failure-injection helpers:

- `MemoryDenProjectionSink.failNext` in `@rusty-crew/adapter-den`;
- `MemoryAgentActivityObservationSink.failNext` in the brain island;
- `MemoryAdminControlAuditSink.failNext` in the brain island;
- Den channel fake transport helpers such as `failNextOpen`.

These are valuable for smoke tests because they prove degraded projection,
audit, and transport behavior. The risk is package-boundary drift: production
code can import a fake or call failure injection as if it were part of the
runtime API.

## Decision

Failure-injection helpers are test-support APIs, not production APIs.

Production entrypoints may export narrow sink and adapter interfaces. They
should not export `.failNext()` helpers, fake transports, or intentionally
memory-only implementations as ordinary runtime constructors once packaging
hardens.

During scaffolding, existing public fakes may remain to avoid breaking smokes,
but each production-adjacent fake export must be tracked by a Den task and
moved behind a test-support boundary before Rusty Crew is packaged for normal
service use.

## Boundary Rules

Production exports:

- interfaces such as `DenProjectionSink` and `AgentActivityObservationSink`;
- real adapter constructors;
- pure projection/formatting helpers;
- explicit diagnostic helpers that do not mutate runtime state.

Test-support exports:

- memory sinks with failure injection;
- fake transports and fake CDP/network providers;
- fixtures and deterministic smoke harness builders;
- helpers that intentionally simulate broken dependencies.

Fakes in smoke files do not need package exports. Shared fakes should use an
explicit `test-support` entrypoint or module name so imports make their purpose
obvious.

## Implementation Direction

Add a dedicated test-support export split for TS packages that currently expose
memory fakes through their root export. Existing smokes can migrate first.
Production imports should then be updated to use only production interfaces and
constructors.

## Consequences

This keeps failure injection available without blessing it as runtime API.
It also aligns with the broader stubs/fakes policy: useful scaffolding is fine,
but production-adjacent fake behavior needs visible tracking and a removal or
isolation path.
