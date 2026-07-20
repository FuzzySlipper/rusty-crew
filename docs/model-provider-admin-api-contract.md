# Model Provider Admin API Contract

Status: v0.3 implemented contract for Rusty Crew model-provider and shared
credential administration.

The machine-readable source artifact is
[`model-provider-admin-api-v0.openapi.json`](model-provider-admin-api-v0.openapi.json).
Rusty View and other operator clients should generate protocol types from that
artifact instead of hand-copying route shapes.

## Route Families

- `GET /v1/admin/model-providers`: list configured model providers.
- `POST /v1/admin/model-providers`: create or upsert a model provider.
- `GET /v1/admin/model-providers/{alias}`: read a provider by alias.
- `PATCH /v1/admin/model-providers/{alias}`: update a provider by alias.
- `POST /v1/admin/model-providers/{alias}/credential/link`: link an alias to a
  shared service credential.
- `POST /v1/admin/model-providers/{alias}/credential/unlink`: unlink an alias
  without changing or deleting the shared credential.
- `GET|POST /v1/admin/service-credentials`: list redacted credentials or create
  one credential identity.
- `GET|PATCH|DELETE /v1/admin/service-credentials/{credentialId}`: read, update,
  or delete an unlinked credential with revision protection.
- `GET /v1/admin/service-credentials/{credentialId}/impact`: report linked
  aliases and whether clear/delete are currently allowed.
- `POST /v1/admin/service-credentials/{credentialId}/clear`: clear an unlinked
  credential secret without deleting its identity.
- `POST /v1/admin/service-credentials/{credentialId}/providers/{alias}/link|unlink`:
  explicit credential-oriented alias association operations.
- `GET|POST /v1/admin/service-credentials/{credentialId}/oauth/openai/{status|start|complete|clear}`:
  manage one OpenAI OAuth login independent of model aliases.
- `GET /v1/admin/model-providers/{alias}/oauth/openai/status`: inspect OpenAI
  OAuth login state through the compatibility facade for one provider.
- `POST /v1/admin/model-providers/{alias}/oauth/openai/start`: create a
  redacted pending OAuth login.
- `POST /v1/admin/model-providers/{alias}/oauth/openai/complete`: exchange or
  test-complete a pending login, store it under the alias credential identity,
  and link the alias.
- `POST /v1/admin/model-providers/{alias}/oauth/openai/clear`: unlink the alias;
  it does not destroy a possibly shared credential.

## Contract Rules

- Public provider records include both `temperatureMilli` and a projected
  decimal `temperature` when a temperature is configured. `temperatureMilli`
  remains the storage/write millivalue.
- Write bodies may use decimal `temperature` or integer `temperatureMilli`.
- Chat Completions vendor controls use the typed `chatCompletionsDialect`,
  `thinkingMode`, `reasoningHistory`, and optional `reasoningBudgetTokens`
  fields. Standard endpoints reject vendor-only combinations.
- Revision conflicts use `reason_code:
  "model_provider_revision_mismatch"` and return `expectedRevision`,
  `currentRevision`, and the current projected provider when available.
- OAuth pending login responses never expose `codeVerifier`.
- Credential records are always redacted. Raw API keys, access tokens, refresh
  tokens, ID tokens, and exchanged API tokens never appear in API responses.
- Credential clear and delete fail with `service_credential_linked` until all
  aliases are explicitly unlinked. There is no force-delete bypass.
- Credential and provider mutations use separate revisions. OAuth completion
  updates the credential revision once; every linked alias reads the same
  refreshed secret envelope without duplicating auth state.
- The preferred workflow is credential first: create one credential, complete
  OAuth once, and link any number of compatible OpenAI Responses aliases.
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
