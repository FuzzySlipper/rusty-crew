# Den Assignment, Completion, And Evidence Loop Plan

Status: Planning note for task 2942

Date: 2026-06-20

Related docs:

- `multi-agent-adapter-architecture`
- `outbound-channel-projections`
- `evidence-result-reference-policy`
- `background-services-governance-loop-architecture`
- `adr-0007-worker-pools-as-optional-capacity`
- `adr-0010-delegation-den-observability-and-controls`

## Purpose

Pi-crew had several Den-centered long-tail loops:

- `den-pool-source`
- `den-assignment-runner`
- `den-assignment-loop`
- `den-completion-poster`
- `den-admin-evidence-poster`
- `den-pool-cleanup`
- `worker-pool-groups`
- `den-router-metadata-client`
- delegation/channel breadcrumb sinks

Rusty Crew should not port these as a Den-owned coordination layer. Den product
data stays Den-owned, Rust coordination stays Rust-owned, and adapter code
projects between them without becoming the authority for assignment,
completion, lifecycle, wake, or worker capacity decisions.

## Current Grounding

Rusty Crew already has enough substrate to plan this deliberately:

- normalized channel inbound/outbound/activity projection records;
- Den Channels transport/cursor discipline;
- channel readback as bounded context inspection, not replay;
- runtime completion packets and delegated-session lifecycle events;
- observation activity producer for Den Web display breadcrumbs;
- evidence/result-reference policy for compact `work_ref` and `result_ref`;
- scheduler/background loop contract for bounded, restart-safe loops.

## Boundary Decisions

### Assignments

Den assignments, tasks, and product records are external product context. Rust
may reference them as `work_ref` or binding metadata, but Den assignment records
must not become Rusty Crew's scheduling queue.

Allowed first version:

- adapter reads or receives Den assignment/product references;
- adapter normalizes them into product-data projections or explicit control
  requests;
- Rust links accepted work to sessions, delegation runs, or completion packets;
- adapter projects lifecycle summaries back to Den observation/channel surfaces.

Disallowed:

- adapter decides that an assignment is claimed, complete, retried, or expired
  without a typed Rust or Den-domain operation;
- assignment polling becomes a hidden wake loop;
- Den assignment state replaces Rust session/delegation/worker-run state.

### Completion And Evidence

Completion is Rust-owned when it is runtime completion. Evidence posting is
adapter-owned display/projection after the owning domain has produced an
outcome.

The first implementation should project:

- completion packet refs;
- delegated run/session refs;
- task/assignment refs when available;
- bounded summary text;
- optional Den message/document/task refs;
- projection status and degraded reason.

Projection failure degrades diagnostics. It must not roll back runtime
completion.

### Router Metadata

Router metadata is useful as an adapter-side anti-corruption record:

- binding id;
- agent/session/profile/instance ids;
- Den project/task/assignment refs;
- channel/thread/user refs;
- selected tool profile or MCP surface refs;
- provider cursor/subscription refs;
- status and degraded reason.

It should not contain secrets, raw prompts, full tool outputs, or become a
global "current active agent" singleton.

### Worker Pools

Worker-pool source, cleanup, and group concepts are deferred until a concrete
capacity use case needs them. ADR 0007 says worker pools are optional capacity,
not the primary delegation model.

Useful pool concepts to keep later:

- pool member availability;
- leases with release/expiry;
- concurrency/capability matching;
- quarantine/drain/offboarded status;
- typed no-capacity diagnostics;
- Den/operator projection of pool state.

These should be future Rust coordination and Den projection tasks, not part of
the first Den assignment/completion/evidence loop.

## Proposed Child Task Slices

1. Define Den work-ref and router-metadata projection contracts.

   Add typed records for Den product refs, router metadata, and binding
   provenance that can connect Den tasks/assignments/channels to runtime
   sessions without making Den product data authoritative.

2. Implement completion and evidence projection sink.

   Consume Rust completion/delegation events and produce bounded Den
   observation/channel activity projections with `work_ref` and `result_ref`.
   Preserve failure-as-degraded behavior.

3. Implement assignment/product-data ingestion boundary.

   Add a Den adapter path for bounded assignment/product references that can
   create explicit Rust control/ingress requests or product-data updates.
   Do not poll as a hidden scheduler and do not claim/complete work in adapter
   code.

4. Add router metadata diagnostics and read APIs.

   Expose per-agent/session/profile/channel metadata for operator diagnostics
   without leaking provider secrets or building a global singleton.

5. Prove Den loop e2e without Den authority.

   Smoke one lifecycle where Den product refs enter as context, Rust completes
   or delegates work, and adapter projections emit completion/evidence refs. The
   proof should also show projection failure does not block Rust completion.

6. Defer worker-pool Den source/cleanup behind optional-capacity design.

   Create future tasks only after Rust pool member/lease records exist. Until
   then, keep pi-crew `den-pool-source`, `den-pool-cleanup`, and
   `worker-pool-groups` as inventory, not implementation targets.

## Open Questions

- What Den assignment/product DTOs should be normalized first: task refs,
  assignment refs, or both?
- Should Den product-data ingestion be scheduled by Rust-owned jobs or invoked
  by a Den adapter subscription/webhook first?
- Which observation surface should carry the first completion/evidence proof:
  Den Web `agent_activity.v1`, Den Channels activity, or both?
- Does the first router metadata record live purely in TS adapter memory, or
  should Rust own a persisted binding/product-ref query immediately?

## Done Criteria For The Planning Parent

The parent is complete when the long-tail pi-crew Den loops are either mapped
to implementation slices above or explicitly deferred behind worker-pool
optional-capacity work. It should not remain blocked on the entire background
services parent.
