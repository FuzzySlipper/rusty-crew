# 0003. Use Frozen Snapshots For Mid-Turn Deltas

Date: 2026-06-19

## Status

Accepted

## Context

The Rust body loop wakes the TypeScript brain island with a projected
`BodyState`. While the brain is active, new user messages, Den observations, or
adapter events may arrive. Task 2771 tested whether the current
`@earendil-works/pi-agent-core` Agent loop gives us a safe hook for injecting
those deltas into an in-flight inference turn.

The upstream Agent API exposes `steer()` and `followUp()` queues, plus lower
level `getSteeringMessages`, `getFollowUpMessages`, and `prepareNextTurn`
hooks. The package docs and generated types show these hooks are turn-boundary
mechanics:

- `steer()` is drained after the current assistant turn and tool calls finish,
  before a later provider request.
- `followUp()` is drained only after the agent would otherwise stop.
- `shouldStopAfterTurn` exits before polling either queue.
- The provider stream currently in progress is not mutated by these queues.

That makes the hooks useful as transient runtime controls, but unsafe as the
coordination source of truth. A durable or long-lived queue can replay messages
that were intended for an earlier context and effectively resurrect stale
instructions.

## Decision

Use `frozen_snapshot_next_wake` for v1.

- Each brain wake receives a frozen body-state snapshot.
- Mid-turn deltas are not injected into the current provider stream.
- New messages arriving while the brain is active are candidates for the next
  wake only.
- If a queue is used between active wake and next wake, it is owned by the body
  policy, not by pi-agent's internal queues.
- The body queue is aggressively bounded: default TTL is 5 seconds and default
  capacity is 32 messages.
- Expired queued messages are dropped, not replayed.

The shared body contract now carries `BodyState.deltaPolicy` so the brain
island can see the active decision in-band. The TypeScript brain island clears
upstream Agent queues when a Rust wake exits and configures upstream queues as
`one-at-a-time` if they are touched.

## Evidence

The integrated smoke `npm run smoke:mid-turn` simulates a message arriving while
a pi-agent-backed wake is active. It verifies:

- the current wake prompt contains only the frozen snapshot message;
- the mid-turn message is available to the next wake while fresh;
- the same message is dropped after its TTL;
- upstream Agent queues are cleared when the wake exits.

## Consequences

This avoids in-flight context mutation and avoids a durable unbounded queue. It
does mean the active brain may not see a very fresh update until another wake.
That is acceptable for v1 because the Rust body loop owns activation and can
choose whether the new event warrants a follow-up wake.

If later testing proves that immediate steering is required, the replacement
must still preserve the same safety property: body-owned TTL, small cap,
explicit drop behavior, and no replay of expired messages.
