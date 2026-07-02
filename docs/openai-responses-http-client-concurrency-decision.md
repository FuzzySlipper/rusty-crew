# OpenAI Responses Rust Brain HTTP Client Concurrency Decision

Status: phase-1 decision  
Task: 3914  
Date: 2026-07-02

## Decision

Defer migrating the Rust OpenAI Responses brain from `reqwest::blocking` to
async `reqwest` during architecture remediation phase 1.

The current implementation may keep blocking provider HTTP inside the direct
Rust brain crate because the bridge now runs the wake off the Node event loop
and exposes a buffered drain path for Rusty View-visible progress:

- `run_openai_responses_brain_json` runs as a napi worker task.
- `start_openai_responses_brain_json` starts a background Rust wake and stores
  stream items in a wake-id keyed buffer.
- `drain_openai_responses_brain_stream_json` lets TypeScript poll visible
  stream items and terminal actions separately.

This makes blocking `reqwest` a capacity concern rather than a correctness
defect for the current service shape.

## Rationale

The async migration would require introducing a Rust async runtime boundary.
That should not happen as an incidental detail inside `core-bridge-node` or the
brain crate because hidden runtime initialization would become another service
host assumption.

The expected near-term Rust responses brain concurrency is low:

- Most active brains still use the TypeScript pi-agent path.
- Rust responses is currently a targeted provider-loop module under evaluation.
- Wake execution is per-session guarded by `inFlightWakes`.
- Phase-1 smokes only require one or a few concurrent direct Rust wakes.

At that level, one OS worker/thread per active Rust responses wake is acceptable
and simpler to reason about than a partially introduced async runtime.

## Future Async Migration Trigger

Revisit async `reqwest` when one of these becomes true:

- a single service regularly runs many concurrent Rust responses wakes;
- Rust responses becomes the default brain for dozens of profiles;
- field metrics show worker-thread saturation, delayed wake drain polling, or
  poor service responsiveness under concurrent Rust responses load;
- the responses provider loop is deepened to emit provider SSE events as they
  arrive rather than materializing each provider response body before processing.

## Required Shape For A Future Migration

A future migration should define the runtime owner explicitly.

Acceptable shapes:

- a dedicated runtime owned by the Rust brain bridge layer, with clear shutdown
  and timeout semantics;
- an async client encapsulated behind a Rust brain module runner that does not
  leak Tokio/runtime assumptions into core coordination crates;
- a deeper provider streaming interface that sends `BrainWakeStreamItem`s to the
  buffer as provider SSE frames arrive.

Non-goals:

- no async runtime globals hidden in core coordination crates;
- no service-host reliance on an implicitly initialized Tokio runtime;
- no callback-heavy JavaScript payload path as the first fix.

## Validation State

Current phase-1 validation covers the correctness concerns that originally made
blocking HTTP risky:

- admin diagnostics stays responsive during an in-flight fake Rust responses
  wake;
- Rusty View chat SSE receives `assistant_text_delta` before chat POST
  completion through the buffered drain bridge;
- existing responses provider-state field smoke still passes.

Capacity/load validation remains future work.
