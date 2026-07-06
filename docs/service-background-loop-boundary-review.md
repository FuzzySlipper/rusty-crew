# Service Background Loop Boundary Review

Date: 2026-07-06

Task: `#4337`

## Current Slice

`ts/packages/brain-island/src/service-background-loops.ts` owns the timer
composition for service background loops. `service-app.ts` now adapts concrete
service state into explicit callbacks and interval configuration.

This first slice moves timer ownership out of the large app file without moving
the actual scheduler, wake, Den, or Telegram behaviors.

## Service-Host Composition

These concerns are process composition and should continue moving toward the
service-host layer over later slices:

- timer creation and cancellation policy;
- startup/shutdown sequencing;
- concrete adapter lifecycle ownership;
- background-loop failure projection to service diagnostics.

The new module is intentionally small enough that `service-host` can eventually
own it or wrap it without pulling brain behavior along.

## Brain-Island Ports And Executors

The following remain brain-island/service-app callback responsibilities for now:

- scheduler heartbeat execution;
- queued wake draining;
- Den runtime heartbeat and delivery polling;
- Telegram outbound drain;
- translating background failures into service events.

These are still close to the current service state, but they now cross an
explicit callback boundary rather than being hidden inside timer setup.

## Rust Authority

The extraction does not move deterministic authority. These remain Rust/storage
domain candidates:

- scheduler tick ownership and persisted run state;
- wake dispatch decisions;
- queue TTL and expiration;
- lifecycle state transitions for sessions/workers/delegations.

Future slices should move authority only when the Rust API can own the behavior
directly, not by recreating policy in another TypeScript module.
