# FFI Throughput Pre-napi Baseline

Date: 2026-06-19

This is the first throughput baseline for task 2788. It measures the Rust
native-facade path, not actual Node/Rust napi crossing overhead.

## What This Measures

- `NativeBridge::build_brain_wake_request` creates handle-based wake buffers.
- The harness borrows and releases `body_state`, `system_prompt`, and
  `role_assembly` handles.
- A synthetic high-rate `BrainEvent::TextDelta` stream calls
  `NativeBridge::submit_brain_event`.
- `CoreEngine::submit_brain_event` publishes `CoreEvent::BrainEventObserved`.
- Current core event handling includes synchronous SQLite persistence through
  the bus recorder.

Command:

```sh
RUSTY_CREW_THROUGHPUT_EVENTS=10000 \
RUSTY_CREW_THROUGHPUT_BATCHES=1,16,64 \
RUSTY_CREW_THROUGHPUT_TEXT_BYTES=64 \
cargo run --release -p rusty-crew-core-bridge-node --bin measure_brain_event_throughput
```

## Results

| Producer batch size | Events/sec | p50 submit | p95 submit | p99 submit | RSS delta | CPU ticks delta |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 341.83 | 2.990 ms | 3.083 ms | 3.498 ms | +2,011,136 B | 120 |
| 16 | 346.06 | 2.983 ms | 3.095 ms | 3.650 ms | +1,966,080 B | 114 |
| 64 | 349.46 | 2.978 ms | 3.097 ms | 3.407 ms | +1,970,176 B | 107 |

Wake-buffer hydration for approximately 737 KB of body-state, system-prompt,
and role-assembly payload took 1.4-2.2 ms. The handle path is not the bottleneck
in this baseline.

Producer batch size had little effect because there is not yet a true batched
bridge operation. The harness still calls `submit_brain_event` once per event;
the batch knob currently measures producer chunking only.

## Preliminary Recommendation

Use a hybrid policy unless the true napi measurement disproves it:

- Keep lifecycle events, tool-call boundaries, action receipts, and completion
  packets as durable per-event submissions.
- Treat high-rate text deltas as stream data, not necessarily durable
  coordination facts.
- Add a true batched text-delta operation or a coalescing layer if real napi
  p95 submit latency exceeds 5 ms, if sustained throughput stays below 1,000
  text deltas/sec, or if synchronous persistence remains in the per-delta path.
- Measure persistence separately from FFI before making final manifest
  constraints. This baseline is dominated by durable event handling, not buffer
  hydration.

## Blocked Until 2789

- `@rusty-crew/native-bridge` still does not load a real napi binding.
- These results do not include JavaScript serialization, napi call overhead, or
  callback scheduling.
- `subscribe_events` is not yet delivering events back to TypeScript.
- There is no true batch-submit manifest operation to compare against per-event
  calls.
