# Task 6537 Chat Completions Transport Recovery

Date: 2026-08-01 (America/Los_Angeles)

## Incident Causality

The two failures reported for
`software-engineer-session-20260801T08321219-1` coincided with deliberate
restarts of `den-router.service` while task 6513 added the local Haiku route.
The first router restart began at 20:37:13 PDT after accepting the provider
request at 20:37:11. The second began at 20:38:00 after accepting the retrying
wake at 20:37:22. Both Crew failures were therefore pre-response transport
interruptions caused by operator maintenance, not model output or an upstream
HTTP error.

## Focused Verification

```text
cargo test -p rusty-crew-chat-completions-brain
74 passed; 0 failed; 1 ignored
```

The focused tests prove:

- a connection-refused request is retried until the same endpoint returns;
- retry backoff is interruptible by explicit cancellation;
- HTTP 503 remains a distinct, non-retried provider response;
- degraded and recovered status events carry stable reason codes;
- successful provider output is emitted once after recovery.

## Live Debug Certification

The native bridge was rebuilt and only `rusty-crew-debug.service` was restarted.
The test used the SQLite debug service at `http://127.0.0.1:9348` and a disposable
local provider port. The provider port was unavailable when the turn began and
started serving a valid Chat Completions SSE response after 1.2 seconds.

Observed result:

```json
{
  "profileId": "task-6537-profile-msbdq2ko",
  "sessionId": "task-6537-profile-msbdq2ko-session",
  "marker": "TRANSPORT_RECOVERED_MSBDQ2KO",
  "reasonCodes": [
    "chat_completions_provider_transport_retry",
    "chat_completions_provider_transport_retry",
    "chat_completions_provider_transport_retry",
    "chat_completions_provider_transport_recovered"
  ],
  "completedAssistantMessages": 1
}
```

The disposable profile was hard-deleted and its test provider was archived.
The production service on port 9347 and `den-router.service` were not restarted
or reconfigured during this certification.

## Safety Boundary

Only request-send failures before an HTTP response are retried indefinitely.
Backoff is cancellation-aware and capped at five seconds. HTTP errors and
failures after response streaming starts remain terminal because replaying an
ambiguous partial response could duplicate provider output or side effects.

This was the boundary certified by task 6537. Task 6545 later expanded it to
retry HTTP 429, 502, 503, and 504 responses before semantic provider output;
see `docs/evidence/task-6545-chat-completions-overload-recovery-live.md` for the
current policy and live proof.
