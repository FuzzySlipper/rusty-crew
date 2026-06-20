# ADR 0010: Delegation Den Observability And Controls

Status: Accepted

Date: 2026-06-20

## Context

Den needs to show delegated-session lifecycle for operators, but Den must not
become the coordination authority. Rust owns delegated session state,
worker-run status, timeout/cancellation, and parent-child routing.

## Decision

Rust publishes `delegation_lifecycle_observed` events for delegated session
creation, wake request, checkpoint request, terminal completion states, timeout,
and cancellation. Den adapters project these events from the normal event
subscription path. Projection failures mark the adapter degraded and drop the
projection, but they do not block Rust coordination or parent/child routing.

Operator controls are typed bridge calls back into Rust:

- `cancel_delegated_session`
- `request_delegated_checkpoint`
- `drain_delegated_sessions`
- `delegated_session_status`

Den or another adapter may present these controls, but the actual lifecycle
mutation happens inside Rust and then emits lifecycle/session events for
observation.

## Consequences

Den can display delegated creation, wake, checkpoint, completion, failure,
timeout, and cancellation without owning that state. A future Den UI can add
buttons or command tools over the typed bridge operations while preserving the
same coordination boundary.
