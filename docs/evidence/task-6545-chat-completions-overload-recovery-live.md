# Task 6545 Chat Completions Overload Recovery

Date: 2026-08-02 (America/Los_Angeles)

## Behavior

The Rust Chat Completions brain now retries HTTP 429, 502, 503, and 504 when the
attempt produced no semantic provider output. It reuses the frozen request,
keeps the existing wake and logical turn, honors bounded `Retry-After` seconds
or HTTP dates, and otherwise uses cancellation-aware exponential backoff. A
successful retry emits stable degraded and recovered provider status events.

Permanent HTTP errors, provider protocol errors, cancellation, configured
request deadlines, and failures after semantic provider output remain
terminal. Error bodies are bounded to 4 KiB and credential-bearing bodies are
redacted; a failed body read does not discard the known HTTP status.

## Deterministic Verification

```text
cargo test -p rusty-crew-chat-completions-brain --locked
82 passed; 0 failed; 1 ignored

npm run verify:offline
passed
```

The focused suite covers all four retryable statuses, permanent status
rejection, byte-for-byte request identity across retry, cancellation during a
server-directed delay, bounded seconds/date `Retry-After` parsing, truncated
error bodies, redaction, zero-event response-stream interruption, stable event
metadata, and single output emission.

## Live Rusty View Certification

Only `rusty-crew-debug.service` was restarted with the rebuilt native module.
The SQLite debug service at `http://127.0.0.1:9348` used a disposable provider
alias and isolated profile. A local proxy returned this response to the first
real provider request:

```text
HTTP 429 Too Many Requests
Retry-After: 1
{"error":{"message":"The engine is currently overloaded, please try again later","type":"engine_overloaded_error"}}
```

The proxy forwarded the next identical 38,079-byte request to local den-router
model `kimi-k2.7`, which returned a real streamed model response. Crew's durable
events for that turn contained:

```json
{
  "retry": {
    "attempt": 1,
    "backoff_ms": 1000,
    "reason_code": "chat_completions_provider_overload_retry",
    "retry_after_ms": 1000,
    "status": 429
  },
  "recovered": {
    "attempts": 1,
    "last_status": 429,
    "reason_code": "chat_completions_provider_overload_recovered"
  },
  "assistant_terminals": [
    {
      "status": "completed",
      "summary": "OVERLOAD RECOVERY COMPLETE"
    }
  ],
  "failed_terminals": []
}
```

The broker run passed in Chromium:

```text
run_id: rusty-view-20260802T073411.119209180Z-2748455
status: passed
scenario: @provider-overload-recovery
result: 1 passed (10.7s)
```

The inspected final screenshot showed one user message, one assistant message
with `OVERLOAD RECOVERY COMPLETE`, an idle completed session, and the connected
event inspector. `visible-transcript.txt` contained the same single exchange;
`page-errors.json` was empty.

Evidence roots:

- broker index:
  `/home/agent/.cache/den-playwright/runs/rusty-view/rusty-view-20260802T073411.119209180Z-2748455/run-index.json`
- rendered artifacts:
  `/home/dev/rusty-view/dist/.playwright/rusty-view-e2e/2749425/live-provider-overload-rec-fe5ee--provider-overload-recovery-chromium/live-artifacts/`

The temporary Rusty View spec was removed, both disposable profiles were
hard-deleted, the disposable provider was archived, and the proxy was stopped.
The Rusty View worktree was clean afterward. The production Crew service on
port 9347 was not restarted or reconfigured.
