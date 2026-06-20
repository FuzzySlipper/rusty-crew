# 0004. Rust Owns Wake Scheduling, Bridge Runtime Owns TS Invocation

Date: 2026-06-20

## Status

Accepted for v1 implementation.

## Context

The unified architecture requires Rust to drive activation. Events arrive on
the internal bus, Rust projects body state and evaluates wake thresholds, then
the TypeScript brain island is invoked with a frozen snapshot. TypeScript must
not decide which sessions wake or route coordination messages around the Rust
core.

The current scaffold has the pieces, but not the production loop:

- `DefaultWakeThreshold` can classify wake-worthy events.
- `BodyProjector` can project `BodyState`.
- `RuntimeBufferStore` can build the three-handle wake payload.
- `wakeBrainFromBridgeRequest` can hydrate buffers and call a TS brain.
- `CoreEngine::spawn_delegated_workers` now creates delegated sessions and
  emits `BrainWakeRequested`.
- Native `register_brain_implementation`, `wake_brain`,
  `register_platform_adapter`, `subscribe_events`, and `unsubscribe_events`
  are still unavailable or not implemented.

The main design pressure is crate ownership. Core crates should not depend on
Node or napi callback machinery, but activation decisions must still be
Rust-owned.

## Decision

The v1 scheduler is a Rust-owned body-loop component that lives at the
`core-engine` / runtime boundary. It owns wake eligibility, wake ids, session
lifecycle checks, threshold evaluation, body snapshot projection, and stale
wake suppression.

The native bridge/runtime layer owns the transport callback into TypeScript.
That layer may store registered TS brain implementations and use
`RuntimeBufferStore`, because buffers and napi callbacks are bridge concerns.
It must treat scheduler output as an instruction from Rust, not as an
opportunity to make activation decisions in TS.

In practical terms:

- Rust decides that a wake should happen.
- Rust creates or accepts a durable wake ticket with `wake_id`, `session_id`,
  and the selected brain/profile binding.
- Rust projects the frozen `BodyState`.
- The bridge builds `BrainWakeRequest` buffers and invokes the registered TS
  brain callback.
- TS hydrates buffers, runs the brain/tool loop, submits streamed events and
  the final action batch back to Rust.
- Rust validates and executes actions, then decides whether queued deltas
  require another wake.

## Boundaries

Rust owns:

- bus subscription or equivalent event observation;
- threshold evaluation;
- `BrainWakeRequested` creation and wake idempotency;
- archived/expired session checks;
- body-state projection;
- body-owned next-wake queue policy and TTL;
- action validation/execution.

TypeScript owns:

- brain implementation code;
- LLM provider calls;
- tool execution;
- model and tool registry composition;
- platform adapter protocol translation.

The bridge/runtime owns:

- registered brain callback storage;
- runtime buffer leases;
- napi transport conversion;
- error mapping between callback failure and `CoreError`.

## Consequences

`CoreEngine` should not grow a direct dependency on Node-specific callback
types. If a trait is needed, define it in a transport-neutral Rust crate or
module and implement the napi binding in `core-bridge-node`.

The first production proof may use a deterministic TS brain, but it must use
the same registered-brain and `wake_brain` path that real pi-agent brains use.
Diagnostic helpers such as `projectBodyStateJson` and
`submitBrainActionsJson` remain useful for tests, but they must not be the
core runtime path.

An `EngineRegistry` is not part of this decision. The v1 service has one
engine per service process and many sessions/agents within that engine.
