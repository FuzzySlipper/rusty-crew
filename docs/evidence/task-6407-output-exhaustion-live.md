# Task 6407 output-exhaustion live certification

Date: 2026-07-29 PDT

The certification targeted only `rusty-crew-debug.service` at
`http://127.0.0.1:9348`, backed by its disposable SQLite database. The native
addon was rebuilt from the task checkout and the debug service was restarted.
The production service on port 9347 was not touched.

## Scenario

The existing live Chat Completions provider `tester-chat` was temporarily
changed from `maxOutputTokens=2048` to `maxOutputTokens=128`. A disposable full
profile using `code_read` was asked for a numbered 30-item distributed-service
checklist with a terminal marker and explicitly told not to call tools. The
provider configuration was restored and the disposable profile was deleted in
cleanup.

The first run exposed a real compatibility defect: a reasoning-only truncated
chunk had been inserted into provider history as an assistant message with no
content. DeepSeek rejected that replay with `Invalid assistant message: content
or tool_calls must be set`. The implementation was corrected so reasoning-only
progress remains observable and participates in duplicate suppression without
becoming an empty assistant history message.

## Accepted result

```json
{
  "profileId": "task-6407-output-limit-1785383662",
  "sessionId": "task-6407-output-limit-1785383662-session",
  "operatorState": "completed",
  "eventCount": 390,
  "outputContinuationStatuses": 7,
  "yieldingEvents": 7,
  "queuedEvents": 7,
  "completedTerminals": 1,
  "failedTerminals": 0,
  "diagnostic": {
    "logicalTurnId": "turn:c6aaf1b35e590771ca2aaa4d85bf1e42b4851f36570b2de76d8168c029f7b88d",
    "continuationCount": 8,
    "providerRequestTotal": 8,
    "toolRoundTotal": 0
  }
}
```

Seven separate provider requests exhausted their per-request output budget.
Each emitted visible continuation status, yielded, and queued the same logical
turn. The eighth request completed it. Chat projection produced exactly one
completed terminal and no failed terminal.

## Deterministic support

The Rust suites additionally cover partial visible text, reasoning-only
truncation, replay-prefix suppression, a valid completed tool call at the
limit, malformed tool arguments, repeated equivalent truncation attention,
continuation checkpoint restore, and explicit cancellation:

```bash
cargo test -p rusty-crew-chat-completions-brain
cargo test -p rusty-crew-openai-responses-brain
npm run verify:offline
```
