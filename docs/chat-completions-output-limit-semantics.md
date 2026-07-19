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
partial visible text, normal `stop`, normal tool continuation, and the bounded
actionable-tool exception:

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
