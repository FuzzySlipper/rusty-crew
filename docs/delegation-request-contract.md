# Delegation Request Contract

Status: v1 production request shape for task 2842.

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
- `fan_out_group_id`: optional grouping key for future fan-out execution.
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
- `resource_limits.max_duration_ms` must be greater than zero when provided.

Depth, timeout execution, fan-out concurrency, and parent consumption semantics
are lifecycle concerns for the follow-up state-machine work. This contract only
defines the request shape and early structural validation.
