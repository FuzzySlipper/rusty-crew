# Delegation Request Contract

Status: v2 production request shape for tasks 2842 and 2849.

`BrainAction::RequestDelegation` is an intent from a brain to the Rust
coordination core. It does not spawn a worker directly. Rust validates the
intent, creates the delegated session, records lineage/run state, routes the
prompt, and schedules the wake.

## Fields

- `profile_id`: required child profile or role profile.
- `task_id`: optional Den/product task reference.
- `prompt`: required routed prompt for the delegated session.
- `expected_output`: optional natural-language output/packet expectation.
- `resource_limits`: optional child limits such as workdir, duration, and
  delegation depth.
- `timeout_ms`: optional requested lifecycle timeout.
- `priority`: optional scheduling hint: `low`, `normal`, or `high`.
- `fan_out_group_id`: optional grouping key for bounded fan-out execution.
- `fan_out_max_concurrency`: optional per-group maximum child count for the
  submitted batch. When provided, Rust rejects a group whose submitted actions
  exceed the bound before creating child sessions.
- `fan_out_failure_policy`: optional per-group failure behavior:
  `fail_soft` keeps siblings running and reports aggregate partial failure;
  `fail_fast` cancels nonterminal siblings after the first non-completed child
  completion.
- `correlation_id`: optional caller-provided delegation correlation id. Rust
  generates one when omitted.
- `parent_consumption`: optional parent behavior hint:
  `await_completion` or `observe_only`.

## Validation

Rust rejects malformed requests before creating child sessions or worker-run
records:

- `profile_id` and `prompt` must be non-empty.
- `expected_output`, `fan_out_group_id`, and `correlation_id` must be non-empty
  when provided.
- `timeout_ms` must be greater than zero when provided.
- `fan_out_max_concurrency` must be greater than zero when provided.
- `resource_limits.max_duration_ms` must be greater than zero when provided.
- All requests in the same fan-out group in one action batch must agree on
  `fan_out_max_concurrency` and `fan_out_failure_policy`.
- A fan-out group whose submitted action count exceeds `fan_out_max_concurrency`
  is rejected before any child session or worker-run record is created.

Depth, timeout execution, and parent consumption semantics are lifecycle
concerns owned by Rust. Fan-out is modelled as direct delegated sessions, not a
worker-pool lease.

## Body Projection

`BodyState.fan_out_groups` summarizes fan-out progress for a parent session
without embedding every sibling transcript in the brain context. Each group
includes totals for pending, completed, failed, blocked, exhausted, cancelled,
and expired child runs plus the group status:

- `in_progress`: at least one child is still nonterminal.
- `completed`: all children completed successfully.
- `partial_failure`: fail-soft group ended with one or more non-success child
  outcomes.
- `failed_fast`: fail-fast group has at least one non-success child outcome;
  Rust cancels nonterminal siblings without fabricating completion packets.
