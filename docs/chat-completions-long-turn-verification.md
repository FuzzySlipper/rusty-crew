# Chat Completions long-turn verification

Rusty Crew's native Chat Completions loop permits up to 64 tool continuation
rounds by default. Operators can set a different bounded service-wide value
with `RUSTY_CREW_CHAT_COMPLETIONS_MAX_TOOL_ROUNDS` (1 through 512). The
TypeScript brain host resolves this policy and sends `maxToolRounds` explicitly
across the native boundary. Rust also retains 64 as the defensive default for
direct bridge callers.

The continuation budget is a loop guard, not a time limit. Explicit
cancellation and configured session/service wake-duration policy remain
independent. Repeated calls with the same tool name and arguments continue to
use the separate repeated-call circuit breaker.

The admin diagnostics response reports the effective policy as
`modelProvider.maxContinuationRounds`. Exhaustion terminates with reason code
`chat_completions_continuation_limit_exceeded` and includes the effective
round count in the message.

## Deterministic verification

The Rust loop regression completes twelve distinct tool rounds, and the native
bridge smoke drives the same twelve-round exchange through buffered tool
submission:

```bash
cargo test -p rusty-crew-chat-completions-brain \
  minimal_loop_completes_more_than_eight_distinct_tool_rounds
npm run build:native
npm run smoke:chat-completions-rust-bridge \
  -w @rusty-crew/brain-island
```

## Live verification

Run live certification only against the debug service at
`http://127.0.0.1:9348` and `rusty-crew-debug.service`. Do not use port 9347 or
`rusty-crew.service` for this test. A GoblinBench candidate must declare
`provider_protocol: chat_completions`, the debug service base URL and unit, and
a configured debug-service provider alias.

Task 5977 was motivated by GoblinBench run
`run-20260718-052115-920252a7`: all eight non-GPT Chat Completions candidates
terminated at the old hidden eight-round bridge fallback after completing 23
to 41 batched tool calls.

The repaired path was certified with Grok 4.5 in run
`run-20260718-060809-6166fd2c`. The same `coding.asha-authority-door` scenario
completed normally in 56.9 seconds with ten observed terminal tool calls, five
changed fixture files, a passing fixture CI gate, and a 1.0 ASHA governance
score. The run used `http://127.0.0.1:9348`, `rusty-crew-debug.service`, and
`provider_protocol: chat_completions`; cleanup completed after the run.
