# 0002. Use Hybrid Brain Event Submission

Date: 2026-06-19

## Status

Accepted

## Context

The Rust core receives streamed brain events from the TypeScript brain island
through the native bridge. The open question was whether `BrainEvent::TextDelta`
should cross FFI one event at a time, or whether the bridge/manifest needs a
batching policy.

Task 2789 measured the real Node-to-Rust napi path with 10,000 synthetic
64-byte text deltas. The binding sustained 345-359 events/sec with p95 submit
latency around 3.1 ms. This matches the pre-napi Rust-facade baseline closely,
so napi crossing overhead is not the current bottleneck. The limit is the
synchronous durable event path behind `CoreEngine::submit_brain_event`.

## Decision

Use hybrid submission:

- Submit lifecycle and coordination facts per event.
- Coalesce high-rate text deltas before durable submission.
- Flush text coalescing after 16-64 deltas, 20-50 ms, 16 KiB of text, a
  `finished` event, or a tool-call boundary.

Use the per-event path only for low-rate text streams, diagnostics, and tests
until persistence is reworked.

## Consequences

The v1 manifest can keep `submit_brain_event` stable for exact facts, but it
must not imply that every model token should become an independent durable
event. A future `submit_brain_text_batch` operation or bridge-local coalescer is
needed before production model token streams are routed through this path.

The thresholds are operational constraints, not permanent API law. Re-measure
after persistence is batched or made asynchronous; if p95 submit latency drops
below 1 ms and sustained throughput exceeds 1,000 deltas/sec, the thresholds can
be revisited.
