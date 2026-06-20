# ADR 0009: Bounded Fan-Out Delegation

Status: Accepted

Date: 2026-06-20

## Context

Prime agents need a way to split work across parallel delegated sessions while
keeping worker pools optional. Pi-crew had fan-out behavior tied to subagent and
worker-pool assumptions; Rusty Crew should preserve the useful coordination
shape without making a pool the default runtime model.

## Decision

Fan-out is modelled as a group of direct `BrainAction::RequestDelegation`
actions from one parent session:

- `fan_out_group_id` correlates sibling delegated runs.
- `fan_out_max_concurrency` bounds the number of sibling requests accepted in a
  single action batch.
- `fan_out_failure_policy` controls whether the group is fail-soft or fail-fast.
- Rust persists the group metadata on worker-run records and projects aggregate
  progress into `BodyState.fan_out_groups`.

The aggregate body state reports totals for pending, completed, failed,
blocked, exhausted, cancelled, and expired runs plus a compact group status. It
does not embed all sibling transcripts in the parent context.

## Failure Semantics

Fail-soft groups continue to allow pending siblings to run after one child
reports a non-success completion. When the group has no pending children and at
least one non-success outcome, the aggregate status is `partial_failure`.

Fail-fast groups cancel nonterminal siblings after the first non-completed child
completion. Cancellation archives the sibling delegated session and marks its
worker-run state `cancelled`; Rust does not fabricate completion packets for
cancelled siblings.

## Consequences

Fan-out works without any worker-pool leasing service. A future scheduler can
add rolling concurrency windows, retries, or pool-backed placement behind the
same group metadata, but the parent-facing contract stays centered on direct
delegated sessions and compact aggregate progress.
