# FFI Throughput True napi Measurement

Date: 2026-06-19

This measurement follows the task 2789 path through Node and the real
`@rusty-crew/native-bridge` napi binding. It is comparable to
`docs/ffi-throughput-pre-napi.md`, but the synthetic brain event stream now
crosses the Node/Rust FFI boundary before entering `CoreEngine`.

## What This Measures

- `@rusty-crew/native-bridge` loads
  `native/index.linux-x64-gnu.node`.
- The TypeScript package initializes a real Rust `CoreEngine` through napi.
- A synthetic high-rate `BrainEvent::TextDelta` stream calls
  `submit_brain_event` through the package API.
- Each event is durably observed by the core bus recorder, including the
  current synchronous SQLite persistence path.
- The harness records per-submit latency, event rate, RSS delta, and Node
  process CPU usage split into user/system time.

Command:

```sh
RUSTY_CREW_THROUGHPUT_EVENTS=10000 \
RUSTY_CREW_THROUGHPUT_BATCHES=1,16,64 \
RUSTY_CREW_THROUGHPUT_TEXT_BYTES=64 \
npm run measure:napi
```

## Results

| Producer batch size | Events/sec | p50 submit | p95 submit | p99 submit | Max submit | RSS delta | CPU user | CPU system |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 345.22 | 2.984 ms | 3.079 ms | 4.491 ms | 7.827 ms | -262,144 B | 438.963 ms | 854.741 ms |
| 16 | 358.97 | 2.969 ms | 3.084 ms | 4.430 ms | 10.889 ms | +917,504 B | 864.920 ms | 250.170 ms |
| 64 | 348.88 | 2.977 ms | 3.107 ms | 3.419 ms | 7.077 ms | +1,310,720 B | 737.411 ms | 611.813 ms |

The true napi path is effectively tied with the pre-napi Rust-facade baseline:
roughly 345-359 events/sec with p95 submit latency around 3.1 ms. Producer
batch size still has little effect because it does not change the manifest
operation; each text delta is still submitted and persisted individually.

Memory remained flat for this workload. RSS moved between -256 KiB and +1.25
MiB over 10,000 events, which is below the noise floor for this process shape.

CPU is not saturated. Total Node process CPU time was about 1.1-1.35 seconds
over 27.9-29.0 seconds of wall time, or roughly 4-5 percent of one core. That
means the observed throughput limit is dominated by synchronous durable event
handling rather than JavaScript dispatch or napi crossing overhead.

## Decision

Use a hybrid event policy.

- Keep lifecycle events as per-event durable manifest calls:
  `started`, `finished`, tool-call boundaries, action receipts, and completion
  packets.
- Coalesce high-rate `text_delta` streams before durable submission whenever
  expected stream rate is above 250 deltas/sec, when more than one brain stream
  is active, or when payloads are small token-sized fragments.
- Flush coalesced text deltas at the first of 16-64 deltas, 20-50 ms, 16 KiB of
  accumulated UTF-8 text, `finished`, or a tool-call boundary.
- Treat the current per-event napi path as acceptable for low-rate debugging,
  tests, and lifecycle facts, but not as the steady-state path for streamed
  model output.

The important result is that a true batch/coalescing layer should target the
persistence/event-recording path, not the FFI boundary alone. napi overhead is
not the limiting factor at this stage.

## Follow-Up Constraints

- Add a manifest operation or bridge-side coalescer for text stream chunks
  before wiring production model token streams into durable per-delta
  persistence.
- Keep `submit_brain_event` available for exact lifecycle facts and for
  low-volume text streams.
- Re-measure after persistence is made asynchronous, append-only, or batched;
  if p95 submit latency drops below 1 ms and sustained throughput exceeds 1,000
  deltas/sec, the batching thresholds can be relaxed.
