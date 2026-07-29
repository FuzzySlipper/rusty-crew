# Chat Completions long-turn verification

Rusty Crew's native Chat Completions loop durably yields after 64 completed tool
rounds by default. Operators can set another positive scheduling quantum with
`RUSTY_CREW_CHAT_COMPLETIONS_WORK_QUANTUM_TOOL_ROUNDS`. TypeScript resolves the
policy and sends `workQuantumToolRounds` across the native boundary; Rust keeps
64 as the defensive default for direct bridge callers.

Crossing the quantum is not terminal. Rust atomically persists the logical-turn
checkpoint, publishes the next wake, and resumes messages, reasoning, tool
calls/results, diagnostics, and cancellation identity in another execution
epoch. No finite healthy-progress round limit remains. Repeated identical calls
still use the separate no-progress policy.

Admin diagnostics report the effective policy as
`modelProvider.workQuantumToolRounds`. Yielded epochs report `continuing`; they
do not emit a failed or completed assistant-turn terminal.

## Deterministic verification

The Rust loop regression crosses multiple one-round quanta before completion,
and the native bridge smoke drives twelve rounds through buffered tool
submission:

```bash
cargo test -p rusty-crew-chat-completions-brain \
  minimal_loop_yields_and_resumes_beyond_each_work_quantum
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
