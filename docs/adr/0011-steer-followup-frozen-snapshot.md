# ADR 0011: Do Not Port Direct Steer/Follow-Up Queues

Status: Accepted

Date: 2026-06-20

## Context

Pi-crew had a steer/follow-up bridge that could route channel events directly
into active upstream Agent queues. That made sense in a system where the
TypeScript agent loop owned much of the runtime behavior.

Rusty Crew has a different ownership model:

- Rust owns coordination, session state, routing, queue retention, and wake
  scheduling.
- TypeScript owns brain execution, tool execution, and platform adapters.
- ADR 0003 chose `frozen_snapshot_next_wake` for mid-turn deltas.
- Queued messages are explicitly TTL-bounded recovery state, not an unbounded
  backlog.

Directly porting pi-crew's steer/follow-up bridge would recreate a second
message-routing path inside the TS brain island and could resurrect stale
instructions after the body context that made them valid has expired.

## Decision

Do not port pi-crew's direct `Agent.steer()` / `Agent.followUp()` bridge.

For v1, steer/follow-up behavior is:

- body-owned;
- next-wake only;
- TTL-bounded;
- capacity-bounded;
- expired-message dropping, not replay;
- Rust wake scheduling remains the only activation authority.

The upstream pi-agent queue APIs may still be used as transient implementation
details inside one brain wake, but they are not Rusty Crew coordination state.
The brain island clears upstream queues after wake completion.

## Replacement Shape

The Rusty Crew replacement for steer/follow-up is a body-owned next-wake delta
path:

1. A channel, adapter, or internal event arrives while a brain wake is active.
2. Rust or the body policy decides whether the event should become a future
   wake candidate.
3. If queued, the message receives an enqueue time, expiry time, owner session,
   owner agent, and source/correlation metadata.
4. The active provider stream is not mutated.
5. On the next wake, only non-expired pending messages are projected into the
   frozen `BodyState`.
6. Expired rows become terminal and remain inspectable, but cannot be
   redelivered.

## Guardrails

Any implementation of task 2981 must preserve:

- default TTL near the current body policy of 5 seconds unless explicitly
  overridden by body policy;
- small cap, currently 32 messages by default;
- terminal states for expired, discarded, cancelled, and delivered messages;
- no direct injection into an in-flight provider request;
- no durable dependency on pi-agent's internal steer/follow-up queues;
- query/readback tooling for expired messages that does not move them back to
  pending.

## Reopening Criteria

ADR 0003 can be reopened only with evidence that next-wake deltas are
insufficient for a required workflow and with a replacement design that keeps
the same safety properties:

- Rust-owned authority;
- TTL and cap;
- explicit drop semantics;
- no replay of expired instructions;
- bounded observability.

## Consequences

This preserves Rusty Crew's deterministic runtime boundary and avoids the most
dangerous queue failure mode: stale messages returning as if they were fresh
operator intent.

It may delay very fresh updates until a later wake. That is acceptable for v1
because the scheduler can choose to request another Rust-owned wake when a
fresh event warrants it.
