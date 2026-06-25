# OpenAI Responses Service Field Test

Status: active smoke/field-test note for task #3329

The `openai-responses` brain module uses the direct Rust Responses brain path
through the native bridge when the service provides a native bridge. The
deterministic fake client remains the default so local service smokes and CI do
not require OpenAI credentials.

## Deterministic Service Smoke

```bash
npm run smoke:responses-service-field-test
```

This starts a temporary service host, runs a configured `openai-responses`
profile through the service debug-turn path, verifies provider-state
diagnostics, restarts the host, and verifies provider-state hydration on the
second wake.

## Optional Live Provider Field Test

Live OpenAI calls are opt-in:

```bash
OPENAI_API_KEY=... npm --workspace @rusty-crew/brain-island run smoke:responses-service-live-field-test
```

If a profile sets `modelConfig.apiKeyEnv`, that environment variable is used
instead of `OPENAI_API_KEY`. `modelConfig.baseUrl` defaults to
`https://api.openai.com/v1` when omitted.

Local den-router can be used without an API key when its OAuth-backed `gpt`
route is available:

```bash
RUSTY_CREW_OPENAI_RESPONSES_LIVE=1 \
RUSTY_CREW_OPENAI_RESPONSES_REQUIRE_NATIVE=1 \
RUSTY_CREW_OPENAI_RESPONSES_ALLOW_NO_KEY=1 \
RUSTY_CREW_OPENAI_RESPONSES_BASE_URL=http://127.0.0.1:18082/v1 \
RUSTY_CREW_OPENAI_RESPONSES_MODEL=gpt \
npm run smoke:responses-service-field-test
```

Expected behavior:

- the profile uses the same service/profile/provider-state path as the
  deterministic smoke;
- provider-state diagnostics start as `missing`, become `valid` after the first
  wake, survive restart, and update after the second wake;
- live mode is never enabled unless `RUSTY_CREW_OPENAI_RESPONSES_LIVE=1` is set
  by the command/environment.
