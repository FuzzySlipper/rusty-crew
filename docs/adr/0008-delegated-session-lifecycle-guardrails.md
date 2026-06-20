# ADR 0008: Delegated Session Lifecycle Guardrails

Status: Accepted

Date: 2026-06-20

## Context

Delegated sessions are Rust-owned child sessions created from
`BrainAction::RequestDelegation`. They are the baseline subagent mechanism for
prime and full agents, independent of worker pools.

Pi-crew accumulated checkpoint, timeout, drain, cancellation, and orphan cleanup
behavior around worker runtime code. Rusty Crew needs the same safety properties,
but the guardrails should attach to Rust-owned sessions, worker-run records, and
resource limits rather than to a pool-first worker abstraction.

## Decision

Delegated lifecycle guardrails are coordinated by Rust:

- checkpoint requests are routed as explicit parent-to-child messages and wake
  the delegated session;
- parent archive cancels nonterminal delegated children;
- direct delegated cancellation archives the child and marks the worker run
  `cancelled`;
- max-duration expiry archives the child and marks the worker run `expired`;
- restart cleanup archives delegated sessions whose parent is missing or
  archived;
- shutdown drain archives active sessions and reuses the same cancellation path
  for nonterminal delegated children.

These guardrails never fabricate completion packets. A `CompletionPacket` is
only persisted when a brain delivers `BrainAction::DeliverCompletion`.

## Checkpoint Semantics

A checkpoint request is a prompt for progress, not a terminal state. Rust
validates that the requested child belongs to the parent, routes an
`AgentMessage` to the child with a `checkpoint:<child-session-id>` correlation
id, and schedules the delegated session for wake.

The child may answer with normal messages or eventually deliver a completion
packet. Rust does not invent a checkpoint response packet.

## Terminal Run States

Worker-run states `completed`, `failed`, `blocked`, `exhausted`, `cancelled`,
and `expired` are terminal for lifecycle cleanup. Conservative cleanup must not
overwrite an existing terminal outcome.

Session archival and worker-run terminal state are related but distinct:
completion may terminate the run while leaving the session record available for
inspection; cancellation, timeout, orphan cleanup, and shutdown drain archive
the session because no more work should be scheduled.

## Drain And Cleanup

Drain is conservative:

- active sessions are archived during shutdown;
- nonterminal delegated children are cancelled when their parent is archived;
- cleanup does not replay pending messages;
- cleanup does not create completion packets;
- Den projection is observability only and not required for cleanup.

Future operator controls for drain/cancel/status should call typed Rust
operations and project the result outward rather than editing Den state as if it
were coordination state.
