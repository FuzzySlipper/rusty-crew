# Chat Completions output-limit semantics

Chat Completions `finish_reason` values are provider outcomes, not generic
success signals. Rust owns their interpretation in the native
`chat-completions` brain.

- `stop` completes the provider response normally.
- `tool_calls` continues through the bounded tool loop.
- `length` does not emit `BrainEvent::Finished`. A response with no actionable
  tool call preserves its partial text and reasoning, checkpoints the same
  logical turn, and yields for another provider request. It is not a terminal
  wake failure.
- A tool call remains actionable when the provider reports `length` only when
  the provider supplied an argument string and its complete contents parse as
  a JSON object. Missing, null, non-string, empty, and whitespace-only argument
  values are malformed; Crew never substitutes `{}` or otherwise repairs them.
  The bounded tool loop may execute only the validated call and then request
  the next provider turn.
- Pending tool-call fragments are classified only after the provider's terminal
  chunk is read. A missing/empty function name, invalid argument JSON, or
  non-object argument value is retained as a diagnostic rather than aborting
  parsing before the finish reason is observed. `[DONE]` is also a terminal
  boundary: pending fragments are classified before it can complete a stream.
  Missing or invalid tool-call indices become malformed-call diagnostics and
  enter the same bounded recovery path instead of becoming generic transport
  failures.
- A malformed response never executes any call from that provider round. The
  Rust brain instead supplies deterministic model-visible feedback, checkpoints
  the same logical turn, and yields for recovery. The feedback says that no
  tool ran, includes the field-level diagnostics, and asks for one complete
  JSON object.
- Recovery is temporary wake-local context. The malformed assistant fragment
  and runtime-generated feedback are sent to the next provider request but are
  excluded from durable provider history. Successful tool rounds before and
  after recovery remain durable, including when recovery is exhausted or the
  recovery provider request itself fails. A persisted replacement contains
  only completed assistant tool-call and tool-result messages, never malformed
  fragments or synthetic recovery feedback.
- A partial assistant message is replayed only when it contains visible text.
  Reasoning-only fragments remain observable and checkpointed for duplicate
  suppression, but are not placed in provider request history because some
  OpenAI-compatible providers reject assistant messages without content or
  executable tool calls.
- Recovery emits a degraded provider status with kind
  `malformed_tool_call_recovery`, the attempt count, the triggering reason code,
  and affected tool names. Partial text, reasoning, and malformed-fragment
  diagnostics remain visible in the event stream.
- Repeated equivalent malformed output receives model-visible correction. If
  intent, result, durable state, and assistant progress remain unchanged for
  the configured no-progress threshold, Rust checkpoints the same logical turn
  as `attention_required` instead of failing it.

Output-limit continuation preserves all text, reasoning, and completed tool
events emitted before the provider boundary. It emits an info-level finish
status plus a degraded `output_limit_continuation` status, then projects a
logical-turn yield without fabricating a user message or completed assistant
message. Replayed provider prefixes are suppressed from the transcript. A
completed, valid tool call at the boundary executes once before continuation;
partial or malformed calls never execute.

Rusty Crew does not increase a provider's output-token setting. Provider
configuration remains the operator's authority, and an incomplete provider
result remains visible. Both ordinary output continuation and malformed-call
recovery use the shared progress policy. Equivalent repetition pauses as
durable operator attention with retry and cancel actions; advancing output may
continue for as many provider requests as the logical turn needs.

## Verification

Focused Rust regressions cover reasoning-only truncation, truncation after
partial visible text, successful recovery from truncated and malformed tool
arguments, bounded recovery exhaustion, durable preservation of earlier tool
rounds, normal multi-chunk tool names, normal `stop`, normal tool continuation,
and the actionable-tool exception:

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

Task 6056 forced two truncated `write_file` calls through the debug-only
SQLite service and its `tester-chat` live provider. The malformed calls did not
execute, the first emitted `malformed_tool_call_recovery`, and the repeated
truncation failed with `chat_completions_output_limit_exceeded`. A subsequent
short `write_file` turn in the same session completed and returned
`RECOVERY_SESSION_OK`, proving the failed turn did not strand the session. The
disposable profile and temporary files were removed after certification.

Task 6407 replaced the remaining terminal output-exhaustion behavior and
certified it with a deliberately small provider request budget. See
`docs/evidence/task-6407-output-exhaustion-live.md`.
