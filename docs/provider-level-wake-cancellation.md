# Provider-Level Wake Cancellation

Task: #4475

Rusty Crew has two cancellation layers:

- Service/chat wake timeout: the service stops waiting for a turn, records a
  failed chat terminal event, clears in-flight gating, and keeps the UI from
  staying in a pending state.
- Provider-level cancellation: the brain module asks the provider runtime to
  stop the underlying wake where the module has a real cancellation hook.

The service timeout is authoritative for user-visible wake status. Provider
cancellation is best-effort by module and must not be required before chat
terminal events are recorded.

## Current Module Semantics

### `openai-responses`

Status: cooperatively cancellable for buffered runs.

The TypeScript service passes the wake timeout `AbortSignal` through
`NativeBridgeModule.wakeBrain` to the registered brain executor. The
OpenAI Responses module listens for that signal after creating the buffered
native run and calls `cancelOpenAiResponsesBrain`.

The Rust buffered run registry records:

- `reason_code`, currently `wake_timeout`;
- a human-readable summary;
- `cancelled_at`;
- terminal state with no provider state persistence.

Cancellation clears pending tool requests and submitted tool outputs. The
stream sink and final provider result path ignore cancelled runs, preventing
late provider output from appending to the wake stream after the service has
already recorded timeout terminal chat events.

Limitation: the current live Responses HTTP path uses blocking provider I/O.
Cancellation cannot preempt a single in-flight blocking HTTP request at the
transport layer. It does stop buffered tool waits, terminal drain state, and
late append/persistence drift. A future lower-level Responses transport can add
hard request abort if needed.

### `pi-agent-core`

Status: observation-bounded only.

The service timeout aborts chat event observation and records terminal failed
chat state, but the current pi-agent-core integration does not expose a
service-owned cancellation hook for the live provider/agent loop. The module
therefore may continue provider-side work until its own runtime returns or
fails.

Future work should pass a cancellation primitive into the pi-agent-core adapter
only when the upstream pi runtime supports an explicit abort hook. Do not
simulate provider abort by dropping observation alone; that is already the
service timeout layer.

### `local` / deterministic brains

Status: no provider cancellation needed.

Local deterministic brains run synchronously and do not hold provider resources.
The service timeout layer is sufficient for any accidental long-running local
test path. If future deterministic brains become async or tool-backed, they
should accept the same `BrainWakeOptions.signal` path and fail cooperatively.

## Terminal Event Policy

Service timeout remains represented to chat/API clients as:

- `assistant_message_completed` with `status: failed`;
- `reason_code: wake_timeout`;
- a clear summary naming the timed-out wake and timeout duration;
- `assistant_turn_finished`.

Provider-level cancellation details are diagnostic/module-level details. They
should not replace the service timeout reason unless a future operator stop or
user stop command introduces a different visible reason code.

## In-Flight Gating

Session in-flight gating is released when the service timeout path records its
terminal outcome. For provider modules with cooperative cancellation, the
provider abort request can still be pending at that instant. The module must
ignore late provider output and must not append additional chat events after
the timeout terminal state.

## Validation

Deterministic coverage:

```bash
npm run smoke:openai-responses-cancellation -w @rusty-crew/brain-island
```

This smoke uses the fake Responses client with a short artificial delay,
aborts the wake via `BrainWakeOptions.signal`, and asserts that no late stream
events are submitted after cancellation.

Live timeout behavior remains covered separately by #4478 because it requires a
running debug service and live provider configuration.
