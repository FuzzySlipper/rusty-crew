# Buffered Brain Stream Retention

Rust's buffered brain coordinator owns stream retention for both the
`chat-completions` and `openai-responses` brain modules. Provider transports may
emit a large number of fine-grained text and reasoning deltas, but transport
chunking must not determine whether a turn can retain tool, status, action, or
terminal records.

## Retention Rules

- Adjacent `text_delta` events are retained as one event with concatenated
  text.
- Adjacent `reasoning_delta` events are retained as one event only when their
  reasoning formats match.
- Phase, provider-status, tool, and terminal items are ordering boundaries and
  are never coalesced into deltas.
- `max_stream_items` limits queued nonterminal items. The default is 4,096.
- One terminal `actions` or `wake_failed` item has reserved capacity beyond the
  nonterminal item limit. A terminal result therefore cannot be crowded out by
  earlier stream volume.
- Accepted text and reasoning content has a separate cumulative UTF-8 byte
  limit. The default is 8 MiB per turn. Exceeding it fails the turn with
  `stream_delta_bytes_limit_exceeded`; it does not silently truncate output.

These rules keep final text and reasoning content intact while bounding queue
shape and total retained delta content. Cancellation, timeout, tool-result, and
continuation policies remain independent coordinator concerns.

## Diagnostics

Every buffered drain exposes `stream_retention_metrics`, and active-run admin
diagnostics expose the same counters:

| Field | Meaning |
| --- | --- |
| `raw_stream_item_count` | Valid provider stream items presented to the coordinator |
| `raw_delta_item_count` | Presented text and reasoning delta items |
| `retained_stream_item_count` | Physical queue entries created over the turn |
| `coalesced_delta_item_count` | Delta items appended to an adjacent retained delta |
| `dropped_stream_item_count` | Items rejected by a hard retention limit |
| `retained_delta_bytes` | Cumulative accepted text/reasoning bytes for the turn |
| `queued_delta_bytes` | Accepted delta bytes still waiting in the queue |
| `max_stream_items` | Configured nonterminal queue-entry ceiling |
| `max_stream_delta_bytes` | Configured cumulative delta-byte ceiling |

The service also retains the final metrics with its recent wake diagnostics, so
completed and failed turns can be distinguished from active queue pressure.

## Regression Coverage

The brain-runtime coordinator suite presents more than 4,096 one-character
text/reasoning deltas with tool activity and a terminal action. It verifies
normal completion, exact concatenated content, preserved ordering, bounded
queue entries, and zero dropped items. Separate tests cover terminal reserved
capacity and typed byte-limit exhaustion provenance.

Live provider certification belongs on the debug service at
`http://127.0.0.1:9348`. The live service on port 9347 is not a test target.

## Live Certification

Task 5981 was certified on 2026-07-18 after rebuilding the native addon and
restarting only `rusty-crew-debug.service`:

- GoblinBench run: `run-20260718-170117-a2b317f9`
- scenario: `coding.asha-authority-door`
- affected provider alias: `deepseek-flash`
- harness: `rusty-crew-native` using `chat_completions`
- runtime result: completed in 147,010 ms without
  `stream_items_limit_exceeded`
- canonical GoblinBench result: failed with score 0.69 and 8 of 9 gates
  passing
- distinct model-quality failure: the authority-behavior gate expected
  `UnknownEntity(999)`, while the generated implementation returned
  `MissingDoor(999)`

The stream-retention runtime objective passed: the wake reached a normal
provider completion after the agent used local tools and changed the expected
Rust and TypeScript fixture surfaces. The overall scenario did not pass because
of the separate error-variant mismatch above. GoblinBench used its disposable
debug profile/session lifecycle and did not contact the live service or its
PostgreSQL database.
