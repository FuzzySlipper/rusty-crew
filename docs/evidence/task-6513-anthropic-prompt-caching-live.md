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

## Rereview Certificate: Initial Write, Subsequent Read, And Exact Cost

The rereview used a fresh disposable profile and session so the first request
could not inherit an earlier Crew conversation cache:

- Provider alias: `haiku-cache-cert-6513-r2`
- Profile: `task-6513-haiku-r3`
- Session: `task-6513-haiku-r3-session`
- Local tool profile: `full_agent` (58 stable tool definitions)
- Model: `anthropic/claude-haiku-4.5`
- Provider kind: `openrouter`
- Policy: `automatic_5m`

The full tool catalog supplied a stable prefix above Anthropic's minimum
cacheable size. Two sequential real provider turns completed with exact marker
responses:

| Phase | Generation ID | Prompt | Cache Read | Cache Write | Exact Cost |
| --- | --- | ---: | ---: | ---: | ---: |
| initial cache creation | `gen-1785659509-oHYCd67huAWHemwEfzOq` | 12,119 | 0 | 12,116 | `$0.015213` |
| subsequent cache read | `gen-1785659513-NiIdS32zSxkJvEPvaoNH` | 12,159 | 12,116 | 40 | `$0.0013296` |

The provider's final SSE usage objects were captured by a disposable
metadata-only proxy between debug Crew and the local den-router. The proxy
recorded only generation id, model, and usage/cost fields; it did not record
request bodies, headers, or credentials. The exact provider cost breakdowns
were:

```text
initial: prompt $0.015148 + completion $0.000065 = $0.015213
cached:  prompt $0.0012646 + completion $0.000065 = $0.0013296
total:                                            $0.0165426
```

Crew readback independently matched the provider usage. The
`GET /v1/admin/diagnostics/provider-state` entry for
`task-6513-haiku-r3` reported:

- initial wake: `promptTokens: 12119`, `cachedPromptTokens: 0`,
  `cacheWritePromptTokens: 12116`
- subsequent wake: `promptTokens: 12159`, `cachedPromptTokens: 12116`,
  `cacheWritePromptTokens: 40`
- both wakes: `effectiveTransport: rust-chat-completions`,
  `promptCachingPolicy: automatic_5m`, `providerRequestCount: 1`, and
  `openrouterSessionId:
  rusty-crew-d706d76cc86cd49fd33425eaa157c8612bfe924e4dd04554ee2fdbaac911aba4`

The redacted provider-request cache supplied the corresponding wire proof:

- initial detail `providerdbg_c3ec44ca8c3952323e21a4af`, SHA-256
  `3b25037ae11b12cdc88463917133418884a32d54f1ce24945adc7963d65c83eb`
- read detail `providerdbg_6eb8865dce763cc93daea987`, SHA-256
  `d5ba4911eed489a1ff1eb9f778d7c9675105f9ad68c439767a042534df736300`
- both serialized `cache_control: {"type":"ephemeral"}`, the exact model,
  and the same non-secret hashed session id shown above

This closes the original certificate's evidence gap: the live record now shows
the first cache creation, the next cache hit, the provider's exact billing
amounts, and the Crew diagnostics and request-cache sources used for readback.
