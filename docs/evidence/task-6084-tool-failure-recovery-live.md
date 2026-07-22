# Task 6084 Tool Failure Recovery Live Certificate

Date: 2026-07-22

Target: `rusty-crew-debug.service` at `http://127.0.0.1:9348`, using the
deployment's isolated SQLite database and the real `tester-chat` provider. The
production service on port `9347` was not used for this certificate.

## Scenario

The existing `tester` profile was instructed to perform three ordered tool
calls in one wake:

1. `read_file` for a unique nonexistent path;
2. `read_file` for a different unique nonexistent path;
3. `read_file` for `/home/dev/rusty-crew/README.md`;
4. emit an exact completion marker.

The first two calls deliberately produced the same Rust policy key,
`read_file:tool_exception`, while using distinct arguments. The second failed
result delivered this bounded provider-visible guidance:

```text
[Rusty Crew recovery guidance]
Crew observed repeated read_file failure (tool_exception).
Tool failure count this turn: 2.
Recent tool failures: read_file: tool_exception (retryable=true); read_file: tool_exception (retryable=true).
Do not repeat an unchanged call. Correct the arguments, choose an alternative, or explain the unavailable operation to the user. Continue the turn with the best useful result available.
```

## Result

Wake `service-tester-session-1784720492046-1` emitted this tool lifecycle:

```text
sequence 8210  tool_call_started    read_file
sequence 8211  tool_call_failed     read_file
sequence 8218  tool_call_started    read_file
sequence 8219  tool_call_failed     read_file
sequence 8227  tool_call_started    read_file
sequence 8228  tool_call_completed  read_file
sequence 8249  assistant_message_completed (status completed)
sequence 8250  assistant_turn_finished
```

The final assistant text was:

```text
RECOVERED_AFTER_TWO_FAILURES task-6084-live-1784720491
```

There was no failed terminal event and no `repeated_tool_failure` stop. Both
failed calls reached terminal tool lifecycle events, the later successful call
was accepted in the same Rust-owned wake, and the model completed normally.

## Focused Verification

- `cargo test -p rusty-crew-brain-runtime`: passed, 37 tests.
- Chat Completions brain suite: passed, 58 tests.
- OpenAI Responses brain suite: passed, 33 tests.
- Native bridge suite: passed, 24 tests, including both brain submission
  wrappers remaining nonterminal after repeated ordinary tool failures.
- `npm run build:native`: passed before deployment to the debug service.

`npm run verify:offline` passed the complete Rust workspace, protocol and API
artifact checks, TypeScript typecheck, native release build, and all unit
tests. It then stopped at the repository-wide Prettier check, which reports an
existing baseline of 100 unrelated files. None of those files is changed by
task 6084; formatting the task's Rust and Markdown files passes.

## Production Continuity

Before restarting `rusty-crew.service` on port `9347`, the buffered-run
diagnostic reported zero active runs. The current Software Engineer session
was `software-engineer-session-20260722T09464443-1`, idle at cursor 1032.

After the guarded restart, that exact session remained current and accepted a
small no-tool wake. Wake
`service-software-engineer-session-20260722T09464443-1-1784720844568-1`
completed normally with this text:

```text
SOFTWARE_ENGINEER_RUNTIME_RECOVERED task-6084-production-recovery-1784720844
```

The session returned to idle at cursor 1047. It remained the only non-archived
Software Engineer session, and the runtime activity census contained no active
activities after completion. No replacement session was created.
