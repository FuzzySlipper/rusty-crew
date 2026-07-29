# Active Turn Termination Guard Inventory

Status: implementation audit for task 6363

Date: 2026-07-28 local / 2026-07-29 UTC

## Purpose

This inventory records every known guard that can stop, fail, expire, or lose
an active Rusty Crew brain turn or delegated run. It separates healthy logical
work from bounded operations and no-progress detection.

The target lifecycle rule is simple:

- healthy progress may run indefinitely;
- a finite execution quantum may yield and durably continue, but may not fail
  the logical turn;
- individual provider, tool, browser, subprocess, and message-wait operations
  may remain bounded when their failure is returned as recoverable evidence;
- no-progress policy may request model correction and then pause for operator
  attention;
- only explicit operator/user cancellation or a genuinely terminal provider
  outcome ends an otherwise healthy logical turn.

The current implementation does not yet satisfy that rule. Tasks 6364 through
6373 own the migration. This document is the code-backed baseline and known
limitations list, not permission to preserve the old hard stops.

## Disposition Vocabulary

| Disposition | Meaning |
| --- | --- |
| Retain operation bound | Bound one external or local operation; return its failure to the active turn. |
| Recoverable feedback | Put a typed error in model-visible tool/provider context and allow a corrected attempt. |
| Yield/continue | Persist a continuation checkpoint, release the current execution quantum, and resume the same logical turn. |
| Operator attention | Preserve the turn as paused/attention-required without claiming completion or failure. |
| Explicit cancellation only | No automatic healthy-work deadline; only a user/operator cancellation is terminal. |

## Effective Deployment Snapshot

The live and debug admin diagnostics were read from
`/v1/admin/diagnostics` during this audit.

| Surface | Live `9347` | Debug `9348` |
| --- | --- | --- |
| Whole-wake policy | `wakeTimeout.mode=disabled` | `wakeTimeout.mode=disabled` |
| Session `turnTimeoutMs` | no loaded session override | no loaded session override |
| Provider request deadline | every loaded module reports `disabled` | every loaded module reports `disabled` |
| Chat Completions continuation guard | `512` | `512` |
| Responses continuation guard | `512` | `512` |
| Active buffered runs | one progressing Chat Completions run | none |

The live buffered run had exceeded fifteen minutes while this audit was being
written. It reported `wake_timeout_ms=0`, recent transitions, 2,948 raw stream
items, 429 retained items, 2,519 coalesced deltas, and zero dropped items. That
is direct evidence that disabled wall-clock limits and stream coalescing permit
healthy long work today. It does not remove the latent hard stops below.

Both deployed `service.env` files and `ops/systemd/service.env.example`
currently raise the two continuation variables to the emergency maximum of
`512`:

- `RUSTY_CREW_CHAT_COMPLETIONS_MAX_TOOL_ROUNDS`
- `RUSTY_CREW_OPENAI_RESPONSES_MAX_CONTINUATION_ROUNDS`

Those values reduce immediate failures but are not the target design.

## Whole Logical Turn And Continuation Guards

| Guard | Current owner and surface | Default / maximum | Progress signal | Current terminal behavior | Persistence and restart | Intended disposition |
| --- | --- | --- | --- | --- | --- | --- |
| Service wake timeout | TypeScript `service-wake-dispatch.ts`, `wake-timeout.ts`, and `buffered-brain-host.ts`; session/profile `turnTimeoutMs`, profile `runtime.maxTurnDurationMs`, or service wake policy | disabled when absent; config validator caps configured turn timeout at 24 hours | elapsed wall time only | aborts observation/provider hook and records `wake_timeout` | no logical-turn continuation checkpoint | Remove as a healthy-turn terminal. Preserve explicit cancel; use yield/attention for real no-progress. Tasks 6365 and 6372. |
| Buffered coordinator wake timeout | Rust `brain-runtime::BufferedBrainTurnCoordinator::timeout_if_due_at` | optional milliseconds from the same effective wake policy | elapsed wall time only | transitions run to `TimedOut`, clears pending tool state, reason `wake_timeout` | active registry is process memory | Delete duplicated terminal authority when durable continuation lands. Tasks 6365 and 6372. |
| Chat Completions tool rounds | Rust `chat-completions`; TS `chat-completions-continuation-policy.ts` supplies config | 64 default; 512 hard maximum | counts tool rounds, even when each round makes progress | `chat_completions_continuation_limit_exceeded`; provider state is omitted on this branch | loop messages/counters are wake-local; only selected terminal paths emit provider state | Replace with a bounded execution quantum and durable yield/continue. Task 6366. |
| Responses continuation rounds | Rust `openai-responses`; TS `responses-continuation-policy.ts` supplies config | 64 default; 512 hard maximum | counts continuation requests, even when each round makes progress | `responses_continuation_limit_exceeded` | continuation items, response id, usage, and repeated-call map are wake-local until terminal provider-state output | Replace with the same durable execution quantum. Task 6367. |
| Chat repeated identical tool calls | Rust `chat-completions`, key `(name, arguments_json)` | profile loop config, currently 3 | no state/result/progress comparison | generic failed wake after the fourth identical call | counter is wake-local | Make progress-aware; send correction feedback, then pause for attention only after demonstrated no progress. Task 6368. |
| Responses repeated identical function calls | Rust `openai-responses`, `MAX_REPEATED_FUNCTION_CALLS` | fixed at 3 | no state/result/progress comparison | `responses_repeated_function_call` | map is wake-local | Same progress-aware no-progress policy as Chat Completions. Task 6368. |
| Malformed Chat Completions tool calls | Rust `chat-completions` | one recovery by default | detects provider-output correction, not task progress | after recovery exhaustion: `chat_completions_malformed_provider_stream` or `chat_completions_output_limit_exceeded` | malformed fragments and generated correction text intentionally are not durable; earlier completed rounds can be | Keep model-visible correction. Exhaustion should preserve state and request attention instead of killing the logical turn. Task 6368. |
| Chat `finish_reason=length` | Rust `chat-completions` | provider-controlled output limit | provider operation ended with partial output | `chat_completions_output_limit_exceeded` unless a complete actionable tool call exists | partial events are visible; provider state is missing on the direct no-action branch | Treat as a bounded provider operation. Preserve partial progress, compact or continue when possible, otherwise pause with actionable diagnostics. Tasks 6366 and 6368. |

## Provider And Tool Operation Guards

| Guard | Current owner and surface | Default / maximum | Progress signal | Current terminal behavior | Persistence and restart | Intended disposition |
| --- | --- | --- | --- | --- | --- | --- |
| Provider HTTP request deadline | Rust live clients for both brains; provider `requestTimeoutMs` diagnostics | disabled by default; optional configured milliseconds | bounds one request but currently ignores stream progress within that deadline | failed wake with `provider_request_timeout` | earlier Chat tool rounds may emit provider state; active operation is not resumable after restart | Retain only as an explicit operation bound. Retry or yield the logical turn; never confuse it with turn lifetime. Tasks 6366 and 6367. |
| Provider connect timeout | Rust `reqwest` clients | fixed 10 seconds | connection establishment only | transport/provider failure reaches failed wake | no in-flight connection persistence | Retain operation bound; classify retryability and continue or request attention. |
| Provider transport/stream/protocol errors | Rust brain clients | provider/event dependent | some branches discard already observed progress from the decision | Chat uses a generic provider-stream failed wake; Responses uses `provider_transport_error`, `provider_stream_closed_before_complete`, `provider_protocol_error`, `provider_response_failed`, or `provider_response_incomplete` | earlier durable state varies by branch; no restartable in-flight request | Explicit provider terminal rejection may pause with evidence. Transient transport/close errors retry or yield. Protocol errors pause for operator attention. Tasks 6366 and 6367. |
| Explicit provider cancellation | Rust cancellation tokens plus TS dispatch signal | no automatic default | explicit operator/user/service cancellation signal | `provider_request_cancelled` or cancelled buffered run | terminal cancellation is visible; no continuation expected | Retain as terminal explicit cancellation. Remove wake-timeout masquerading as cancellation. |
| Chat tool cancellation or timeout | Rust Chat loop consumes TS host output flags | tool-specific | does not distinguish user cancellation from one tool operation timeout | either condition immediately emits failed wake | provider state omitted on this branch | Explicit cancel remains terminal. Tool timeout becomes model-visible recoverable output. Task 6368. |
| Ordinary tool denial/failure | Rust `brain-runtime::tool_policy` plus TS executor | consecutive/repeated guidance thresholds | keys include tool/reason/detail and reset on success | returns `isError=true` plus guidance; does not first-strike terminate | tool result is in the current provider loop; durable history depends on brain | Retain current nonterminal feedback. Thresholds guide alternatives; they must not become arbitrary turn ceilings. |
| Local command timeout | Rust tool policy plan, TS local-code executor | 30 seconds absent a resource value; configured value clamped 1 second to 24 hours | one subprocess operation | model-visible tool error, except the Chat timeout flag currently kills the wake | no resumable subprocess | Retain operation bound; remove whole-turn consequence. |
| Browser startup/CDP/load/idle bounds | Rust tool policy metadata, TS browser executor | operation-specific bounded defaults and clamps | operation or browser-resource activity | model-visible tool/resource error | browser resource lifecycle is separate from logical turn state | Retain operation/resource bounds; return typed recovery evidence. |

## Buffered Runtime And Retention Guards

`BufferedBrainTurnLimits::default()` currently defines:

| Buffer | Default | Current overflow behavior | Intended disposition |
| --- | ---: | --- | --- |
| queued stream items | 4,096 | terminal `stream_items_limit_exceeded` | coalesce and drain; shed only replayable diagnostics; apply flow control or yield without ending the turn |
| cumulative retained delta bytes | 8 MiB | terminal `stream_delta_bytes_limit_exceeded` | checkpoint/spill or compact semantic output; preserve a debug reference; yield rather than fail |
| pending tool requests | 128 | terminal `pending_tool_requests_limit_exceeded` | backpressure or yield; never silently lose a request |
| submitted tool results | 1,024 | terminal `tool_results_limit_exceeded` | consume/compact/checkpoint results; yield rather than fail |
| one tool output | 64 KiB | rejects the output as `ToolOutputTooLarge` | return truncation/spill metadata as a model-visible tool result; do not terminate the turn |
| stream sequence | `u64` sequence space | `SequenceExhausted`, normally surfaced as a run error | preserve monotonic identity across continuation epochs; practically unreachable but must not produce an unreported dead turn |

Stream coalescing is already effective and should remain. The design error is
using retention pressure as a logical-turn terminal condition. Task 6365 owns
durable run state, while 6366 through 6368 adapt each producer and no-progress
policy.

## Context And History Bounds

- `maxHistoryMessages` selects a bounded prompt-history window. It does not
  currently terminate a turn and may remain a context-selection policy.
- Profile `runtime.maxTurns` is currently descriptive prompt/runtime metadata,
  not an enforced active-turn counter. It must not become a hidden hard stop.
- Profile `runtime.maxTokensPerTurn` and provider `maxOutputTokens` bound one
  provider response, not the lifetime of the logical turn.
- Context strategy and compaction are advisory/durable surfaces. Context
  pressure should cause compaction or a continuation yield, not a failed turn.
- Provider context-window and output-token limits are operation constraints.
  Their partial output, usage, and provider reason must survive into the
  logical-turn checkpoint.
- There is no durable checkpoint today that can resume midway through a
  Chat/Responses provider-and-tool loop after service restart.

## Delegated Runs, Messaging, And External Codex

| Guard | Current semantics | Classification and target |
| --- | --- | --- |
| Delegated `resourceLimits.maxDurationMs` | Rust `core-engine::expire_delegated_sessions_at` compares wall time since session creation and archives a nonterminal delegated session as expired. Protocol maximum is 30 days. | This can kill healthy worker progress and must become a renewable lease/quantum or an explicitly named operator cap. It is not a general turn deadline. |
| Worker claim/queue lease TTL | Claims expire for crash recovery and reassignment. | Retain as a renewable ownership lease. Lease expiry must not erase durable logical progress or imply task failure by itself. |
| Agent message TTL and `agent_round` wait timeout | Bounds delivery relevance and how long a caller waits for a correlated reply. | Retain as message/wait operation policy. It must not cap the recipient agent's active turn. Late replies need explicit expired/unroutable evidence. |
| External Codex RPC/probe timeout | Codex driver defaults requests to 30 seconds and compatibility probes to 15 seconds. | Retain as controller operation bounds. Transport uncertainty must reconcile after reconnect rather than falsely declaring the native Codex turn terminal. |
| External Codex native turn | Codex app-server owns context, compaction, provider loop, and terminal notifications. Crew owns durable binding/correlation and explicit interrupt. | Crew must not add a lifetime ceiling. Only native `completed`/`failed`/`interrupted` evidence or explicit Crew interrupt is terminal. |

## Restart Gap

The shared `BufferedBrainTurnRegistry` is an in-process `Mutex<HashMap<...>>`.
Service shutdown cancels nonterminal runs and clears them. The registry owns
queued stream items, pending tool requests, submitted tool outputs, provider
loop payload, cancellation, and terminal state, but none of that active state
is hydrated after restart.

This is the main prerequisite for replacing ceilings with continuation:

1. Rust must define a serializable logical-turn checkpoint and execution-epoch
   identity.
2. Checkpoints must include brain-specific provider history/continuation state,
   no-progress evidence, pending neutral tool work, accepted tool results,
   transcript cursor, and retained diagnostics references.
3. Yield must atomically persist the checkpoint before relinquishing the
   current execution epoch.
4. Restart hydration must claim and resume the same logical turn exactly once.
5. Explicit cancellation must invalidate all resumable epochs.

Task 6364 owns the contract design. Task 6365 owns Rust persistence,
coordination, claim, and restart hydration.

## Known Limitations Until The Campaign Lands

- Raising continuation ceilings to 512 only postpones failure; it does not make
  a long turn durable.
- A service restart still cancels active buffered brain runs rather than
  resuming them.
- Repeated-call guards can fail useful work without proving lack of progress.
- Retention pressure can still fail a turn despite successful stream
  coalescing.
- Delegated `maxDurationMs` measures age, not inactivity or progress.
- Provider/tool operation failures are inconsistently separated from logical
  turn failure.
- current diagnostics report active runs and effective policies, but there is
  no first-class `yielded`, `continuation_pending`, `attention_required`, or
  resumed-epoch lifecycle projection yet.

## Implementation Ownership

| Task | Scope |
| --- | --- |
| 6364 | durable continuation/checkpoint, progress, attention, and cancellation contract |
| 6365 | Rust logical-turn authority, persistence, claiming, restart hydration |
| 6366 | Chat Completions quantum and recoverable provider/tool outcomes |
| 6367 | Responses quantum and recoverable provider/tool outcomes |
| 6368 | shared progress-aware no-progress policy and buffer-pressure semantics |
| 6369 | diagnostics, chat/activity projection, and operator controls |
| 6371 | deterministic, restart, and live long-turn certification |
| 6372 | delete legacy timeout/ceiling paths and configuration surfaces |
| 6373 | temporary deployed 512-ceiling mitigation; remove after 6372 |

## Source Map

- `crates/brains/brain-runtime/src/coordinator.rs`
- `crates/brains/brain-runtime/src/tool_policy.rs`
- `crates/brains/chat-completions/src/lib.rs`
- `crates/brains/openai-responses/src/lib.rs`
- `crates/core/core-config/src/lib.rs`
- `crates/core/core-engine/src/delegation.rs`
- `crates/core/core-protocol/src/types.rs`
- `crates/core/core-tool-registry/src/lib.rs`
- `ts/packages/brain-island/src/service-wake-dispatch.ts`
- `ts/packages/brain-island/src/wake-timeout.ts`
- `ts/packages/brain-island/src/chat-completions-continuation-policy.ts`
- `ts/packages/brain-island/src/responses-continuation-policy.ts`
- `ts/packages/brain-island/src/service-external-runtime.ts`
- `ts/packages/external-runtime-codex/src/driver.ts`
