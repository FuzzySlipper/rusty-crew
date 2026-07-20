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
- Chat Completions provider fields `reasoning_content`, `reasoning`,
  `reasoning_delta`, and `thinking` are transport aliases for the same semantic
  reasoning lane. The mapper records each raw field name in provider-event
  metrics, then maps all four to `chat-completions:reasoning`. Alternating alias
  names therefore do not manufacture false format boundaries.
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
Chat Completions terminal transport metrics also expose `providerEventCounts`,
including per-field keys such as `reasoning_delta:reasoning_content`. These raw
counts explain mapper expansion without retaining an unbounded raw-event log.
Runtime configuration application requires a wake-result observer, so profiles
created or reloaded through the admin API publish the same bounded diagnostics
as profiles registered during service startup.

## Regression Coverage

The brain-runtime coordinator suite presents more than 4,096 one-character
text/reasoning deltas with tool activity and a terminal action. A
Chat-Completions integration regression also presents 5,000 reasoning deltas
whose raw fields alternate across all four supported aliases, with a tool
lifecycle in the middle. It verifies exact content, preserved tool boundaries,
five retained entries, 4,998 coalesced entries, zero drops, the unchanged
4,096-item ceiling, and the unchanged 8 MiB byte bound. Separate tests cover
terminal reserved capacity and typed byte-limit exhaustion provenance.

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

## StepFun Alias Expansion

GoblinBench run `run-20260719-025655-dcb35c7e` exposed the alias multiplier on
the debug service before canonical mapping. The StepFun cell retained 996
reasoning updates and two completed tool calls, but alternating/expanded
reasoning formats consumed the 4,096 nonterminal queue entries and failed with
`stream_items_limit_exceeded` after 194,144 ms. This was not an output-token
limit: the provider alias had `maxOutputTokens=64000`.

The fix does not raise either retention limit. Raw field-specific event counts
remain in terminal transport diagnostics; canonical mapped reasoning deltas
coalesce within each semantic span, while tool, status, action, failure, usage,
and terminal items remain explicit ordering boundaries.

Task 5990 reran StepFun on the same debug-only scenario after the fix:

- GoblinBench run: `run-20260719-062803-47b87bf2`
- runtime duration: 137,017 ms
- completed tool calls before terminal failure: 50
- reasoning updates before terminal failure: 22
- changed fixture files retained by the scorer: 2
- canonical score: 0.43 (5 of 9 gates)
- terminal reason: the provider repeated the same `terminal` tool call more
  than three times

The rerun did not reach `stream_items_limit_exceeded`; it progressed far beyond
the original two tool calls and terminated under the independent repeated-call
safety policy. This is a distinct model-behavior failure, not retention or
output-length exhaustion. Both this run and the original reproduction used
only `rusty-crew-debug.service` on port 9348 and its SQLite database.

### Exact Live StepFun Metrics

The bounded metrics certificate was repeated after wiring runtime-created
profiles into the same wake-observation sink as startup profiles:

- GoblinBench run: `run-20260719-231522-26b565f2`
- scenario: `coding.asha-authority-door`
- provider alias: `stepfun`, configured with `maxOutputTokens=64000`
- service: `rusty-crew-debug.service` on port 9348 with SQLite
- runtime duration: 293,204 ms
- provider requests / tool rounds: 7 / 6
- terminal result: `chat_completions_output_limit_exceeded` after 15 completed
  tool calls; the provider returned `finish_reason=length`
- canonical GoblinBench result: failed with score 0.185 (3 of 9 gates); no
  fixture files changed

The terminal `/v1/admin/diagnostics/provider-state` snapshot reported:

| Retention metric | Exact value |
| --- | ---: |
| raw stream items | 32,685 |
| raw delta items | 32,652 |
| retained stream items | 40 |
| coalesced delta items | 32,645 |
| dropped stream items | 0 |
| retained delta bytes | 519,482 |
| queued delta bytes after terminal drain | 0 |
| max stream items | 4,096 |
| max stream delta bytes | 8,388,608 |

The raw-to-retained stream multiplier was exactly `32685 / 40 = 817.125`.
All 32,652 raw deltas were reasoning. StepFun alternated the two provider field
aliases evenly:

| Provider event count | Exact value |
| --- | ---: |
| `reasoning_delta` | 32,652 |
| `reasoning_delta:reasoning` | 16,326 |
| `reasoning_delta:reasoning_content` | 16,326 |
| `finished` | 7 |
| `tool_call_delta` | 15 |
| `tool_call_finished` | 15 |
| `usage` | 16,358 |

Canonical alias mapping retained seven reasoning spans, one per provider
request, so the delta-only multiplier was `32652 / 7`, approximately
4,664.57 raw deltas per retained reasoning item. The other 33 retained records
were meaningful ordering boundaries: one `started` event, 15 tool starts, 15
tool finishes, one provider-status event, and one terminal `wake_failed` item.
The final stream shape was 39 event items plus the terminal failure. No tool,
status, or terminal boundary was coalesced or dropped, and no raw reasoning
text was persisted in this certificate.
