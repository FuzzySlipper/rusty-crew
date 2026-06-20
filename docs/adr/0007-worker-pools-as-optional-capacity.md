# ADR 0007: Worker Pools as Optional Capacity

Status: Accepted

Date: 2026-06-20

## Context

Rusty Crew's dominant agent strategy is prime-agent delegation. A prime agent is
a full agent that owns the main work context, performs substantive work itself,
and delegates bounded slices to subagents when that saves context, reduces token
cost, or isolates a narrow task.

Pi-crew started with worker pools and orchestrators near the center of the
runtime model. Rusty Crew should keep the useful capacity concepts from that
history without making every delegation path depend on a pool.

The project glossary defines a worker pool as available worker capacity that can
be leased or assigned. It also defines delegated sessions as worker-like child
sessions created directly from `BrainAction::RequestDelegation`. Those are
different capabilities.

## Decision

Worker pools are optional capacity infrastructure. They are not the primary
delegation mechanism.

Direct delegated sessions remain the baseline:

- A full or prime agent requests delegation.
- Rust creates a bounded delegated session.
- Rust records lineage, resource limits, tool profile, lifecycle state, and
  completion packets.
- Parent completion consumption works without worker-pool availability.

Worker pools may later sit beside or underneath direct delegation when a use case
needs standing capacity, reusable worker identities, throughput control, or
capability matching. A pool lease must produce or bind to normal Rust-owned
sessions and worker-run records rather than becoming a parallel coordination
system.

## Useful Pool Concepts to Keep

The following pi-crew concepts are still useful if introduced deliberately:

- availability: whether a pool member can accept work;
- leases: bounded claims on capacity with explicit release/expiry;
- concurrency: maximum active assignments per pool/profile/project;
- capability matching: selecting capacity by profile, role, tool surface, or
  resource constraints;
- quarantine: excluding unhealthy capacity without deleting history;
- idle/offboarded states: preserving operator visibility while preventing new
  assignments;
- no-capacity diagnostics: typed reasons when a lease cannot be fulfilled.

These concepts should be modeled as capacity/scheduling concerns, not as the
semantic authority for every agent, tool, or delegation decision.

## Rust Versus Adapter/Den Ownership

Rust coordination should own:

- pool member and lease state;
- lease expiry and release;
- capacity matching decisions;
- concurrency counters;
- links from a lease to session/run lineage;
- terminal lifecycle transitions.

Adapters and Den observability should own:

- human-readable projection of pool state;
- operator controls that call typed Rust operations;
- diagnostics and audit display;
- product/task/document context.

Den must not become the internal worker-pool coordination store. Projection
failure should degrade observability, not block Rust routing or lifecycle
handling.

## Future Implementation Tasks

Future work should be split only when a concrete use case needs pool capacity:

- define `WorkerPool`, `PoolMember`, and `PoolLease` protocol/storage records;
- add pool registration and member lifecycle operations;
- add a lease acquisition operation with typed no-capacity diagnostics;
- bind leases to normal sessions and worker-run lineage;
- enforce lease expiry/release without fabricating completion packets;
- add concurrency and capability matching tests;
- add Den projection and operator controls for status, quarantine, drain, and
  release;
- add migration tooling if existing pi-crew/Hermes agents move onto pooled
  capacity.

## Consequences

Current delegation work can continue without a worker pool. Tasks such as
completion routing, tool-profile enforcement, timeout cleanup, fan-out
aggregation, and the prime-agent end-to-end proof remain valid on direct
delegated sessions.

When worker pools are added, they must integrate with the same session,
lineage, resource-limit, and completion-packet structures rather than forcing a
second runtime model.
