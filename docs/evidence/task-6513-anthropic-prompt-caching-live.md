# Task 6513 Anthropic Prompt Caching Live Certificate

Date: 2026-08-01 PDT

## Target

- Crew: `rusty-crew-debug.service` at `http://127.0.0.1:9348`
- Storage: the dedicated debug SQLite database
- Provider alias: `haiku-cache-cert-6513`
- Provider kind: `openrouter`
- Model: `anthropic/claude-haiku-4.5`
- Upstream path: local den-router alias to OpenRouter
- Policy: `automatic_5m`

The local router received a temporary `haiku` route and an
`anthropic/claude-haiku-4.5` alias. Its OpenRouter credential remained in the
router environment and was not copied into Crew or this evidence packet.

## Procedure

1. Build the native bridge and restart only `rusty-crew-debug.service`.
2. Create the typed provider and a disposable profile with a stable system
   prefix above Haiku 4.5's minimum cacheable prefix.
3. Complete two live model turns.
4. Restart Crew without restarting den-router.
5. Complete two more live turns in the same Crew session.
6. Read the Rust chat-completions transport metrics from admin diagnostics.

## Result

Both post-restart turns completed with the requested exact text. Diagnostics
reported the same non-secret sticky identifier for both requests:

```text
rusty-crew-561d2ba7cb085fc0883c5396958592ec0eba69887faa81eebdb7f79ef837b784
```

Observed usage totals:

| Turn | Prompt | Cache Read | Cache Write |
| --- | ---: | ---: | ---: |
| post-restart 1 | 11,969 | 11,947 | 19 |
| post-restart 2 | 11,989 | 11,966 | 20 |

The diagnostics projection also reported:

- `effectiveTransport: rust-chat-completions`
- `promptCachingPolicy: automatic_5m`
- `providerRequestCount: 1` for each turn
- no terminal failure reason

This certifies the typed provider policy, top-level OpenRouter request
serialization, stable session stickiness across turns and a Crew restart, live
cache reads/writes, and operator-visible usage accounting on SQLite.
