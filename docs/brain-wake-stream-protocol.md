# Brain Wake Stream Protocol

Status: implemented protocol note for task #3294
Date: 2026-06-24

Rusty Crew's brain module boundary is the wake contract, not a required
implementation language. TypeScript bridge-backed brains and future direct Rust
brains both map onto the same wake stream vocabulary.

## Stream Items

`BrainWakeStreamItem` is the transport-neutral sequence emitted by a brain wake:

- `event`: one `BrainEventEnvelope`.
- `actions`: the terminal `BrainActionBatch` for a successful wake.
- `wake_failed`: the terminal failure record for a wake that could not produce
  actions.

A valid stream ends with exactly one terminal item: `actions` or `wake_failed`.
Direct Rust brain modules should return a `BrainWakeStream`. TypeScript
bridge-backed brains may still return the legacy `{ events, actions }` result;
the native bridge wrapper adapts that into the same `event* -> actions` stream
shape before submitting events/actions to Rust.

## Event Mapping

The current event vocabulary covers:

- `started`: provider/model wake began.
- `text_delta`: visible model text.
- `tool_call_started`: model-callable tool invocation began.
- `tool_call_finished`: model-callable tool invocation completed; `is_error`
  marks tool failure.
- `provider_status`: provider-loop status that is not a tool call, including
  degraded transport, retry, recovery, or provider stream errors that do not
  immediately fail the wake.
- `finished`: provider/model wake finished producing stream events.

Provider failures that abort the wake map to the terminal `wake_failed` stream
item. Provider failures that are recovered or informational map to
`provider_status` with level `info`, `degraded`, or `error`.

## Ownership

Rust owns wake dispatch, stream item ingestion, action validation, and
coordination effects. Brain modules own provider request construction and
provider response parsing.

The stream protocol must not expose Rust coordination internals to brain crates.
Future Rust brain crates should depend only on approved wake/protocol surfaces,
not `core-engine`, `core-session`, `core-bus`, `core-body`, or
`core-persistence`.
