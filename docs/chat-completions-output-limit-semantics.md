# Chat Completions output-limit semantics

Chat Completions `finish_reason` values are provider outcomes, not generic
success signals. Rust owns their interpretation in the native
`chat-completions` brain.

- `stop` completes the provider response normally.
- `tool_calls` continues through the bounded tool loop.
- `length` does not emit `BrainEvent::Finished`. If no fully parsed tool call
  is available, the wake terminates with
  `chat_completions_output_limit_exceeded`.
- A tool call remains actionable when the provider reports `length` only when
  its complete arguments parse as a JSON object. The bounded tool loop may then
  execute it and request the next provider turn. Truncated, malformed, or
  non-object arguments terminate as an output-limit failure without invoking
  the tool.
- Pending tool-call fragments are classified only after the provider's terminal
  chunk is read. A missing/empty function name, invalid argument JSON, or
  non-object argument value is retained as a diagnostic rather than aborting
  parsing before the finish reason is observed.
- If malformed fragments accompany a non-`length` finish, the wake fails with
  `chat_completions_malformed_provider_stream`. The provider-status event names
  the fragment index and includes field-level diagnostics. No call from that
  malformed response is executed.

The output-limit failure preserves all text, reasoning, and completed tool
events emitted before the terminal provider event. It also emits an info-level
provider status with `finish_reason: length` in structured metadata. Service
chat projection therefore records the partial transcript and a failed terminal
event instead of a successful empty or partial completion.

Rusty Crew does not increase a provider's output-token setting or start a
hidden continuation for this condition. Provider configuration remains the
operator's authority, and an incomplete provider result remains visible.

## Verification

Focused Rust regressions cover reasoning-only truncation, truncation after
partial visible text, missing-name and malformed-argument terminal fragments,
normal multi-chunk tool names, normal `stop`, normal tool continuation, and the
bounded actionable-tool exception:

```bash
cargo test -p rusty-crew-chat-completions-brain
```

Task 5982 was certified against MiMo Pro through the debug-only SQLite service
at `http://127.0.0.1:9348`. GoblinBench run
`run-20260719-001537-bfd3cefb` reproduced `finish_reason: length` after 112.9
seconds. Rusty Crew rejected the chat wake with
`chat_completions_output_limit_exceeded`, retained the partial response, 40
completed tool calls, 10 reasoning updates, and the provider finish-reason
status, and did not accept the empty patch as a successful turn.

The live command used the existing provider output limit without an override:

```bash
cd /home/dev/goblinbench
python3 scripts/gb-run.py \
  --scenario coding.asha-authority-door \
  --candidates candidates.rusty-crew-native-nongpt-asha.json \
  --candidate rusty-crew-native-mimo-pro \
  --label task-5982-output-limit-live
```

Task 5988 reran the same MiMo Pro candidate after terminal fragment
classification landed. GoblinBench run `run-20260719-061434-09555fbe` used
only `rusty-crew-debug.service` on port 9348 and completed the Crew turn after
386,292 ms with 5 changed files. The earlier missing-name parser crash did not
recur. The deterministic scorer retained a separate model-quality result of
0.69 (7 of 9 gates); the runtime itself completed normally. Real-SSE unit
fixtures retain direct coverage of the typed `length` and malformed non-length
terminal branches because provider output is nondeterministic between live
runs.
