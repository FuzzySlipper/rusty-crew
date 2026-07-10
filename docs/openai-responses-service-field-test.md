# OpenAI Responses Service Field Test

Status: active smoke/field-test note for task #3329

The `openai-responses` catalog entry always uses the direct Rust Responses
brain through the generic native bridge run operations. Production hosts always
construct live clients. Deterministic clients exist only in explicit smoke/test
support, so deployed profiles cannot silently fall back to fake output.

For OpenAI OAuth-backed profiles, the green path is a direct Rusty Crew provider
alias whose credential is a typed `openai_oauth` secret envelope. That path does
not require `OPENAI_API_KEY` and must not rely on
`RUSTY_CREW_OPENAI_RESPONSES_ALLOW_NO_KEY=1`.

The host reports a configured Responses stream idle budget, defaulting to 120
seconds, so operators can see what first-token/read window the profile expects:

```env
RUSTY_CREW_OPENAI_RESPONSES_STREAM_IDLE_TIMEOUT_MS=120000
```

## Deterministic Service Smoke

```bash
npm run smoke:openai-responses-tool-bridge
```

This exercises explicit fake provider protocol/tool fixtures. It does not
describe a selectable deployed-service mode.

## Optional API-Key Live Provider Field Test

API-key live OpenAI calls are opt-in:

```bash
OPENAI_API_KEY=... npm --workspace @rusty-crew/brain-island run smoke:responses-service-live-field-test
```

If a profile sets `modelConfig.apiKeyEnv`, that environment variable is used
instead of `OPENAI_API_KEY`. `modelConfig.baseUrl` defaults to
`https://api.openai.com/v1` when omitted.

## Direct OpenAI OAuth Field Test

For ChatGPT/Codex OAuth credentials, configure a provider through the admin API
and complete the OpenAI login flow so the provider has a redacted
`openai_oauth` credential. The live service may use a longer stream idle budget
for certification:

```bash
RUSTY_CREW_OPENAI_RESPONSES_STREAM_IDLE_TIMEOUT_MS=300000 \
npm run service:start
```

Useful readbacks:

- `GET /v1/admin/model-providers/:alias/oauth/openai/status`
- `GET /v1/chat/sessions/:sessionId/context`
- `GET /v1/admin/diagnostics/provider-state`

The direct OAuth profile should report `clientMode: "live"` and
`credential.kind: "openai_oauth"` without any no-key env flag.

## Optional den-router Compatibility/Proxy Test

Local den-router can still be used as an explicit proxy endpoint when its
OAuth-backed `gpt` route is available. This is compatibility coverage, not the
Rusty Crew OpenAI OAuth certification path:

```bash
RUSTY_CREW_OPENAI_RESPONSES_ALLOW_NO_KEY=1 \
RUSTY_CREW_OPENAI_RESPONSES_BASE_URL=http://127.0.0.1:18082/v1 \
RUSTY_CREW_OPENAI_RESPONSES_MODEL=gpt \
npm run smoke:responses-service-live-field-test
```

Use `RUSTY_CREW_OPENAI_RESPONSES_ALLOW_NO_KEY=1` only when the configured
endpoint handles credentials itself and the profile is deliberately testing that
proxy behavior.

Expected behavior:

- the profile uses the same service/profile/provider-state path as the
  deterministic smoke;
- provider-state diagnostics start as `missing`, become `valid` after the first
  wake, survive restart, and update after the second wake;
- `/v1/admin/diagnostics/provider-state` reports
  `modelProvider.clientMode: "live"` for the Responses profile;
- the same diagnostic reports the effective
  `modelProvider.streamIdleTimeoutMs`. Provider/router transports can still
  surface lower-level idle failures before that budget when they do not open the
  SSE stream or send heartbeat/data bytes;
- production profiles always use the live Rust host; no live/fake toggle exists.

Explicit deterministic smoke support can report
`modelProvider.clientMode: "fake"`; deployed service configuration cannot.
