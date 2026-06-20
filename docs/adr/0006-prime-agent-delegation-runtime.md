# 0006. Prime-Agent Delegation Runtime Comes Before Worker Pools

Date: 2026-06-20

## Status

Accepted for v1 delegation planning.

## Context

The authoritative architecture in Den document
`rusty-crew-unified-architecture` says Rust owns coordination, activation,
session lifecycle, routing, persistence, and action validation. TypeScript owns
the brain island, tool implementations, model calls, and platform adapters.

The project glossary in Den document `rusty-crew-project-glossary` defines a
prime agent as a full agent that owns the main work context and usually does
real work directly, while delegating bounded slices to subagents for context,
token, or parallelism reasons. It also warns that worker pools are a capability,
not the organizing principle.

The parity audit in Den document
`pi-crew-vs-rusty-crew-parity-2026-06-19` is useful as feature inventory, but it
was written before later Rusty Crew wake-path work landed. Treat its worker and
delegation analysis as grounding material, not as binding design authority.

Pi-crew began with a strong worker-pool/orchestrator center of gravity and later
had to loosen that coupling to support full agents properly. Rusty Crew should
avoid rebuilding that pressure into the architecture.

## Decision

Rusty Crew v1 delegation is direct subagent delegation from a full or prime
agent, not worker-pool leasing.

The first production path is:

1. A full or prime agent wakes through the Rust-owned scheduler.
2. Its TypeScript brain emits `BrainAction::RequestDelegation`.
3. Rust validates the action against session state, resource limits, lineage,
   and delegation depth.
4. Rust creates a bounded `SessionKind::Delegated` child session and durable
   delegation/run state.
5. Rust routes the delegated prompt to the child session.
6. Rust emits or consumes `BrainWakeRequested` for the delegated session through
   the same scheduler and registered-brain bridge path as any other session.
7. The delegated brain returns a completion packet.
8. Rust validates and persists the completion, then routes/project it back to
   the parent session without forcing the parent into an orchestrator-only role.

Worker pools remain a later capacity and scheduling capability. They may lease
standing capacity, optimize throughput, or host specialized workers, but direct
delegated sessions must not depend on a pool existing.

## Porting Rules

Port from pi-crew by responsibility, not by file shape.

First-class now:

- `RequestDelegation` as the brain-to-Rust intent surface;
- Rust-owned delegated session creation;
- lineage from parent wake/session to delegated session;
- resource limits such as max delegation depth;
- prompt routing over the internal bus;
- scheduler-driven delegated wakes;
- completion packet validation and persistence.

Later capabilities:

- worker-pool leasing and capacity management;
- fan-out concurrency controls;
- specialized coder/reviewer/packet-auditor role assemblies;
- idle timeout and drain-mode sophistication;
- operator controls and projection sinks beyond the first Den observability
  path.

Intentionally not ported as the default model:

- TypeScript-owned spawning as the coordination authority;
- orchestrator-only assumptions where every meaningful task must be assigned to
  a temporary worker;
- `WorkerPolicy` as the primary tool gate. Tool enablement remains
  profile-based, with resource limits modeled separately.

## Sequencing Constraints

The implementation chain should keep the design pressure visible:

- `2841` models delegation lineage and session relationships in protocol and
  persistence.
- `2842` extends the delegation request contract for production use.
- `2843` implements the Rust-owned direct delegated-session lifecycle state
  machine.
- `2844` integrates delegated sessions with the production wake scheduler.

`2844` should not pretend to be complete until lineage, request shape, and
lifecycle state are durable enough to avoid duplicate or stale delegated work.

## Consequences

Direct delegation must preserve parent agency. A prime/full agent may delegate a
slice and continue doing work in later wakes; it is not reduced to a dispatcher.

The direct delegated-session lifecycle is the narrow bridge from current smokes
to real production behavior. It can start small, but it needs idempotency and
restart behavior early enough that retried action batches do not create duplicate
children or resurrect stale work.

Worker pools will fit later as a scheduler/capacity layer over the same session,
lineage, wake, and completion primitives. They should not introduce a second
coordination substrate.
