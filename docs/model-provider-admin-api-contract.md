# Model Provider Admin API Contract

Status: v0 implemented contract for Rusty Crew model-provider administration.

The machine-readable source artifact is
[`model-provider-admin-api-v0.openapi.json`](model-provider-admin-api-v0.openapi.json).
Rusty View and other operator clients should generate protocol types from that
artifact instead of hand-copying route shapes.

## Route Families

- `GET /v1/admin/model-providers`: list configured model providers.
- `POST /v1/admin/model-providers`: create or upsert a model provider.
- `GET /v1/admin/model-providers/{alias}`: read a provider by alias.
- `PATCH /v1/admin/model-providers/{alias}`: update a provider by alias.
- `GET /v1/admin/model-providers/{alias}/oauth/openai/status`: inspect OpenAI
  OAuth login state for one provider.
- `POST /v1/admin/model-providers/{alias}/oauth/openai/start`: create a
  redacted pending OAuth login.
- `POST /v1/admin/model-providers/{alias}/oauth/openai/complete`: exchange or
  test-complete a pending login and store the provider credential.
- `POST /v1/admin/model-providers/{alias}/oauth/openai/clear`: clear the stored
  credential and pending logins for one provider.

## Contract Rules

- Public provider records include both `temperatureMilli` and a projected
  decimal `temperature` when a temperature is configured. `temperatureMilli`
  remains the storage/write millivalue.
- Write bodies may use decimal `temperature` or integer `temperatureMilli`.
- Revision conflicts use `reason_code:
  "model_provider_revision_mismatch"` and return `expectedRevision`,
  `currentRevision`, and the current projected provider when available.
- OAuth pending login responses never expose `codeVerifier`.
- OpenAI OAuth-specific fields remain under the OAuth route schemas; generic
  provider records only expose redacted credential summaries.

## Drift Check

Run:

```bash
npm run smoke:model-provider-admin-contract
```

The smoke checks the OpenAPI artifact against
`model-provider-admin-contract.ts` constants and route behavior around
temperature readback, revision conflicts, and OAuth status/start shape.
