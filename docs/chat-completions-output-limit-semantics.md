# Chat Completions output-limit semantics

Chat Completions `finish_reason` values are provider outcomes, not generic
success signals. Rust owns their interpretation in the native
`chat-completions` brain.

- `stop` completes the provider response normally.
- `tool_calls` continues through the bounded tool loop.
- `length` does not emit `BrainEvent::Finished`. A response with no tool-call
  fragment still terminates with `chat_completions_output_limit_exceeded` when
  no fully parsed tool call is available.
- A tool call remains actionable when the provider reports `length` only when
  its complete arguments parse as a JSON object. The bounded tool loop may then
  execute it and request the next provider turn.
- Pending tool-call fragments are classified only after the provider's terminal
  chunk is read. A missing/empty function name, invalid argument JSON, or
  non-object argument value is retained as a diagnostic rather than aborting
  parsing before the finish reason is observed.
- A malformed response never executes any call from that provider round. The
  Rust brain instead supplies deterministic model-visible feedback and permits
  one provider recovery attempt by default. The feedback says that no tool ran,
  includes the field-level diagnostics, and asks for one complete JSON object.
- Recovery is temporary wake-local context. The malformed assistant fragment
  and runtime-generated feedback are sent to the next provider request but are
  excluded from durable provider history. Successful tool rounds before and
  after recovery remain durable.
- A partial assistant message is replayed only when it contains visible text.
  Reasoning-only fragments remain observable but are not placed in recovery
  request history because some OpenAI-compatible providers reject assistant
  messages without content or executable tool calls.
- Recovery emits a degraded provider status with kind
  `malformed_tool_call_recovery`, the attempt count, the triggering reason code,
  and affected tool names. Partial text, reasoning, and malformed-fragment
  diagnostics remain visible in the event stream.
- Repeated malformed output exhausts the bounded recovery and fails under the
  original stable reason family: `chat_completions_output_limit_exceeded` for a
  `length` finish or `chat_completions_malformed_provider_stream` otherwise.
  The terminal failure states that recovery was exhausted.

The output-limit failure preserves all text, reasoning, and completed tool
events emitted before the terminal provider event. It also emits an info-level
provider status with `finish_reason: length` in structured metadata. Service
chat projection therefore records the partial transcript and a failed terminal
event instead of a successful empty or partial completion.

Rusty Crew does not increase a provider's output-token setting. Provider
configuration remains the operator's authority, and an incomplete provider
result remains visible. The bounded malformed-call recovery is not a generic
hidden continuation: it runs only when a tool fragment is present but unsafe to
execute, records an explicit degraded status, and cannot exceed
`DEFAULT_MAX_MALFORMED_TOOL_CALL_RECOVERIES` without deliberate configuration.

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
