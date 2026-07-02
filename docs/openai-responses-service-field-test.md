# OpenAI Responses Service Field Test

Status: active smoke/field-test note for task #3329

The `openai-responses` brain module uses the direct Rust Responses brain path
through the native bridge when the service provides a native bridge. The
deterministic fake client remains the default so local service smokes and CI do
not require OpenAI credentials.

Fake mode is smoke-only. A deployed service, field certification run, or user
profile that is meant to talk to a real provider should use a model provider
alias with `protocol=responses` and set live mode explicitly:

```env
RUSTY_CREW_OPENAI_RESPONSES_LIVE=1
RUSTY_CREW_OPENAI_RESPONSES_REQUIRE_NATIVE=1
```

For OpenAI OAuth-backed profiles, the green path is a direct Rusty Crew provider
alias whose credential is a typed `openai_oauth` secret envelope. That path does
not require `OPENAI_API_KEY` and must not rely on
`RUSTY_CREW_OPENAI_RESPONSES_ALLOW_NO_KEY=1`.

Live mode reports a configured Responses stream idle budget, defaulting to 120
seconds, so operators can see what first-token/read window the profile expects:

```env
RUSTY_CREW_OPENAI_RESPONSES_STREAM_IDLE_TIMEOUT_MS=120000
```

Do not certify a live profile while these settings are absent; otherwise the
service is allowed to use the deterministic fake client for smoke coverage.

## Deterministic Service Smoke

```bash
npm run smoke:responses-service-field-test
```

This starts a temporary service host, runs a configured `openai-responses`
profile through the service debug-turn path, verifies provider-state
diagnostics, restarts the host, and verifies provider-state hydration on the
second wake.

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
`openai_oauth` credential. The live service then needs only:

```bash
RUSTY_CREW_OPENAI_RESPONSES_LIVE=1 \
RUSTY_CREW_OPENAI_RESPONSES_REQUIRE_NATIVE=1 \
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
RUSTY_CREW_OPENAI_RESPONSES_LIVE=1 \
RUSTY_CREW_OPENAI_RESPONSES_REQUIRE_NATIVE=1 \
RUSTY_CREW_OPENAI_RESPONSES_ALLOW_NO_KEY=1 \
RUSTY_CREW_OPENAI_RESPONSES_BASE_URL=http://127.0.0.1:18082/v1 \
RUSTY_CREW_OPENAI_RESPONSES_MODEL=gpt \
npm run smoke:responses-service-field-test
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
- live mode is never enabled unless `RUSTY_CREW_OPENAI_RESPONSES_LIVE=1` is set
  by the command/environment.

In deterministic smoke mode the same diagnostics route reports
`modelProvider.clientMode: "fake"`. That is expected only for local tests and
CI-style smoke runs.
