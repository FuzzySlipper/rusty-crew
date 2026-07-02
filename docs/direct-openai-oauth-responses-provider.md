# Direct OpenAI OAuth Responses Provider

Status: implemented green-path note for task #3972/#3978
Date: 2026-07-02

Rusty Crew supports a direct OpenAI OAuth-backed Responses provider so the
Responses brain does not need to route through den-router for the ChatGPT/Codex
auth path. This is the green path for `openai-responses` live profiles.

The direct provider shape is:

- model provider: `providerKind=openai`, `protocol=responses`
- credential: typed `openai_oauth` provider secret envelope
- request base URL: `https://chatgpt.com/backend-api/codex`
- profile brain module: `openai-responses`
- live mode: `RUSTY_CREW_OPENAI_RESPONSES_LIVE=1`
- native required: `RUSTY_CREW_OPENAI_RESPONSES_REQUIRE_NATIVE=1`

`RUSTY_CREW_OPENAI_RESPONSES_ALLOW_NO_KEY=1` is not part of the direct OpenAI
OAuth path. That flag exists only for explicitly configured API-key-compatible
proxy endpoints that handle credentials outside Rusty Crew. Direct OAuth
providers get their credentials from the provider secret envelope and should not
depend on no-key mode.

Codex is useful here because its Rust source is an available reference for the
sanctioned OAuth and Responses behavior, but Rusty Crew implements its own
provider, credential envelopes, API routes, tests, and diagnostics.

Do not vendor or depend on Codex crates for this work. The local Codex checkout is
Apache-2.0 licensed reference material. Any direct code copy would need explicit
license/NOTICE review first.

## Reference Sources

The current audit used these files from `/home/research/codex`:

- `codex-rs/login/src/server.rs`
- `codex-rs/login/src/pkce.rs`
- `codex-rs/login/src/token_data.rs`
- `codex-rs/login/src/auth/default_client.rs`
- `codex-rs/login/src/auth/manager.rs`
- `codex-rs/model-provider-info/src/lib.rs`
- `codex-rs/model-provider/src/auth.rs`
- `codex-rs/model-provider/src/bearer_auth_provider.rs`
- `codex-rs/codex-api/src/endpoint/responses.rs`
- `codex-rs/codex-api/src/provider.rs`

## OAuth Flow

Codex defaults:

- Issuer: `https://auth.openai.com`
- Authorization endpoint: `{issuer}/oauth/authorize`
- Token endpoint: `{issuer}/oauth/token`
- Revoke endpoint: `https://auth.openai.com/oauth/revoke`
- Client id: `app_EMoamEEZ73f0CkXaXp7hrann`
- Client id override: `CODEX_APP_SERVER_LOGIN_CLIENT_ID`
- Originator default: `codex_cli_rs`
- Refresh URL override: `CODEX_REFRESH_TOKEN_URL_OVERRIDE`
- Revoke URL override: `CODEX_REVOKE_TOKEN_URL_OVERRIDE`

Rusty Crew should use Rusty Crew-owned config names, but keep the same default
endpoint and client-id semantics unless testing proves OpenAI requires a
different registered client id.

PKCE requirements:

- Generate 64 random bytes.
- `code_verifier` is base64url without padding.
- `code_challenge` is base64url without padding of `SHA256(code_verifier)`.
- `code_challenge_method=S256`.
- Generate a random `state` and validate it exactly on callback.

Authorization URL query:

- `response_type=code`
- `client_id=<client id>`
- `redirect_uri=<Crew-configured registered callback URI>`
- `scope=openid profile email offline_access api.connectors.read api.connectors.invoke`
- `code_challenge=<pkce challenge>`
- `code_challenge_method=S256`
- `id_token_add_organizations=true`
- `codex_cli_simplified_flow=true`
- `state=<state>`
- `originator=<originator>`
- optional `allowed_workspace_id=<comma-separated workspace ids>`

Rusty Crew's default direct OpenAI client uses the registered callback
`http://localhost:1455/auth/callback`. Rusty View and other frontends must not
derive `redirect_uri` from the browser origin (for example a LAN
`http://192.168.x.x:9347/...` URL) unless the operator has configured a
separate OpenAI OAuth client registration and enabled redirect URI overrides in
Crew service config. The provider OAuth status/start API exposes the configured
redirect URI and whether overrides are allowed.

For LAN or remote-operator use with the default registered localhost callback:

1. Start the OAuth login through Crew and open the returned authorization URL.
2. Complete the OpenAI login in the browser.
3. If the browser lands on a localhost callback that the operator machine is
   not serving, copy the final callback URL from the address bar.
4. POST that full URL to the Crew completion route as `callbackUrl`. Crew
   extracts `code` and `state`, finds the pending login for that provider, and
   exchanges the code using the original registered redirect URI and private
   PKCE verifier.

This path avoids asking users to paste internal pending ids, verifier material,
token bundles, or secret JSON.

Authorization-code exchange:

- POST `{issuer}/oauth/token`
- Content type: `application/x-www-form-urlencoded`
- Body:
  - `grant_type=authorization_code`
  - `code=<authorization code>`
  - `redirect_uri=<same callback uri>`
  - `client_id=<client id>`
  - `code_verifier=<pkce verifier>`
- Successful response contains `id_token`, `access_token`, and `refresh_token`.

Optional API-key-style token exchange:

- POST `{issuer}/oauth/token`
- Content type: `application/x-www-form-urlencoded`
- Body:
  - `grant_type=urn:ietf:params:oauth:grant-type:token-exchange`
  - `client_id=<client id>`
  - `requested_token=openai-api-key`
  - `subject_token=<id_token>`
  - `subject_token_type=urn:ietf:params:oauth:token-type:id_token`
- Successful response contains `access_token`.

Codex obtains this API-key-style access token during login and persists it
alongside the OAuth token bundle. Its model request path still supports
ChatGPT/Codex bearer auth from the ChatGPT access token. Rusty Crew should store
the exchanged token as optional credential material, not make it the only direct
Responses path.

Refresh flow:

- POST token endpoint.
- Content type: `application/json`.
- JSON body:
  - `client_id=<client id>`
  - `grant_type=refresh_token`
  - `refresh_token=<refresh token>`
- Successful response may include any of `id_token`, `access_token`, and
  `refresh_token`; update only the returned fields.
- Mark `last_refresh_at` on successful refresh.
- Permanent refresh failure codes include `refresh_token_expired`,
  `refresh_token_reused`, and `refresh_token_invalidated`.

Refresh policy:

- Refresh if access-token JWT expiration is within 5 minutes.
- Refresh if `last_refresh_at` is older than 8 days.
- A provider request that receives an authentication failure should also be able
  to force one refresh and retry once.

## Responses Provider Behavior

Codex treats OpenAI/Codex as Responses-only. Its Chat wire API has been removed.

Default provider URLs:

- API-key auth: `https://api.openai.com/v1`
- ChatGPT/Codex OAuth auth: `https://chatgpt.com/backend-api/codex`

Request behavior:

- Path: `responses`
- Method: `POST`
- Streaming header: `Accept: text/event-stream`
- Auth header: `Authorization: Bearer <access token>`
- Workspace header when available: `ChatGPT-Account-ID: <account id>`
- FedRAMP routing header when applicable: `X-OpenAI-Fedramp: true`
- Provider header: Codex sends a `version` header. Rusty Crew should send a
  Rusty Crew-owned version/client header, not impersonate Codex.
- Codex default stream idle timeout is 300,000 ms.
- Codex allows `OPENAI_ORGANIZATION` and `OPENAI_PROJECT` env-backed headers for
  the OpenAI provider. Rusty Crew should model those explicitly if needed rather
  than silently reading old Codex env names.

## Rusty Crew Provider Metadata

Provider metadata should contain routing and non-secret behavior:

```json
{
  "auth_kind": "openai_oauth",
  "oauth": {
    "issuer": "https://auth.openai.com",
    "client_id": "app_EMoamEEZ73f0CkXaXp7hrann",
    "originator": "rusty_crew",
    "scopes": [
      "openid",
      "profile",
      "email",
      "offline_access",
      "api.connectors.read",
      "api.connectors.invoke"
    ],
    "allowed_workspace_ids": []
  },
  "request": {
    "base_url": "https://chatgpt.com/backend-api/codex",
    "stream_idle_timeout_ms": 300000,
    "supports_streaming": true,
    "supports_websockets": false
  }
}
```

The provider record should still use first-class columns for existing common
fields:

- `provider_kind=openai`
- `protocol=responses`
- `model_id=<selected model>`
- `context_size`, `max_output_tokens`, `temperature_milli`,
  `reasoning_effort`, and `reasoning_format`

Direct OpenAI OAuth should be a first-class provider auth kind, not an implicit
meaning of `base_url` or `provider_kind`.

## Provider Secret JSON

The existing `model_providers.secret_ciphertext` slot is an opaque string today.
For direct OAuth, make it a typed JSON envelope. Diagnostics and readback APIs
must never return token values.

Recommended API-key envelope:

```json
{
  "version": 1,
  "kind": "api_key",
  "api_key": "sk-..."
}
```

Recommended OpenAI OAuth envelope:

```json
{
  "version": 1,
  "kind": "openai_oauth",
  "issuer": "https://auth.openai.com",
  "client_id": "app_EMoamEEZ73f0CkXaXp7hrann",
  "id_token": "<jwt>",
  "access_token": "<jwt>",
  "refresh_token": "<refresh token>",
  "exchanged_api_token": "<optional token-exchange access token>",
  "last_refresh_at": "2026-07-02T00:00:00Z",
  "account_id": "<chatgpt workspace/account id>",
  "email": "<optional email from id token>",
  "plan_type": "<optional plan>",
  "is_fedramp_account": false,
  "access_token_expires_at": "2026-07-02T01:00:00Z"
}
```

Implementation notes:

- Parse ID-token claims enough to populate non-secret diagnostics:
  `email`, `plan_type`, `account_id`, `is_fedramp_account`.
- Parse access-token JWT expiration enough to decide proactive refresh.
- Secret updates should use a dedicated provider-secret update path so token
  refreshes do not fight normal provider edit revisions.
- New raw `apiKey` convenience writes are normalized into the `api_key` envelope
  at storage time so existing provider forms keep working while the DB stops
  accumulating raw secret strings. Stored legacy raw strings are still
  interpreted as API keys by the resolver until they are rewritten.

## Admin API Contract Notes

For model-provider create/update routes:

- `apiKey` remains a convenience input for API-key providers. The service stores
  it as an `api_key` envelope.
- `credentialSecret` is the explicit typed input. Use `{ "kind": "api_key",
  "value": "..." }` for API-key providers or `{ "kind": "openai_oauth", ... }`
  for completed OpenAI OAuth credentials.
- `clearSecret: true` clears stored credentials and must not be combined with
  `apiKey`, `secret`, or `credentialSecret`.
- OpenAI OAuth UI should normally start an OAuth login attempt instead of asking
  users to paste token bundles. The completed callback path will write the
  `openai_oauth` envelope.
- Remote/LAN UIs should complete fixed-redirect logins by submitting the pasted
  callback URL as `callbackUrl`, not by inventing a LAN redirect URI.
- Provider read/list routes expose only `credential.hasSecret`,
  `credential.secretRef`, `credential.updatedAt`, and `credential.kind`. They
  must not expose `credentialSecret`, token values, or raw secret JSON.
- Existing TypeScript pi-brain provider resolution unwraps only `api_key`
  envelopes. `openai_oauth` envelopes require the direct Rust Responses brain
  provider path.

## Login Attempt State

The PKCE verifier and state are short-lived pending login material, not provider
credentials. They should not be written into the permanent provider secret.

Recommended shape:

- Store pending login attempts in a small DB-backed table or service storage
  repository with a strict TTL.
- Fields: attempt id, provider alias, code verifier, state, redirect uri,
  issuer, client id, allowed workspace ids, created at, expires at.
- Complete, cancel, and expiry paths must delete the verifier/state.
- Admin readback may expose attempt status and expiration, never verifier/state.

An in-memory-only attempt store would be simpler but makes browser login fragile
across service restarts. Since Rusty Crew already needs profile/provider admin
APIs, DB-backed TTL state is the better default.

## Difference From den-router Proxy Usage

The den-router path was useful during early testing because it hid OpenAI auth
outside Rusty Crew. It is now optional compatibility/proxy behavior, not the
default certification path for Rusty Crew's Responses brain.

Direct provider path:

- Rusty Crew owns provider setup, OAuth login attempt state, token refresh,
  provider diagnostics, and request construction.
- Responses requests go directly to the OpenAI/ChatGPT Codex backend.
- Rusty Crew can debug provider state without asking den-router to explain a
  proxy failure.
- Rusty View can expose provider setup and status through Rusty Crew admin APIs.

den-router proxy path after certification:

- Keep only as a generic OpenAI-compatible provider option when a profile
  explicitly selects a proxy URL and credential behavior.
- Remove any special assumption that `gpt`/den-router is the canonical
  Responses route.
- Remove live-test wording that suggests den-router is required for OpenAI OAuth
  Responses.
- Do not block Rusty Crew OpenAI Responses certification on den-router proxy
  repairs. Track den-router failures in focused Den tasks for den-router.

## Diagnostics And Operator Readback

Direct OAuth status is visible through Rusty Crew admin APIs:

- `GET /v1/admin/model-providers/:alias/oauth/openai/status` reports the
  configured redirect URI, whether LAN redirect overrides are allowed, pending
  login summaries, redacted credential presence, and redacted OAuth summary
  fields such as account id/email when available.
- `GET /v1/chat/sessions/:sessionId/context` reports the selected model
  provider, brain module, protocol, provider kind, and redacted credential
  status for a session.
- Provider list/read routes expose credential metadata such as
  `credential.kind`, `credential.hasSecret`, and `credential.updatedAt`; they
  must never return token values or raw secret JSON.

Common direct OAuth failures should be diagnosed in Rusty Crew first:

- missing secret envelope for a provider configured as `openai_oauth`;
- expired or missing pending login during callback completion;
- callback state mismatch;
- refresh-token expiration, reuse, or invalidation;
- provider response authentication failure after one forced refresh retry;
- non-streaming or idle provider response before the configured stream idle
  budget.

## Validation Plan

Deterministic tests:

- Fake OAuth server for authorize URL generation, callback state validation,
  token exchange, token refresh, token-exchange API-token response, revoke, and
  permanent refresh failure classification.
- Credential-envelope round-trip tests for API-key and OpenAI OAuth providers.
- Redaction tests for diagnostics/readback.
- Provider request tests proving base URL, path, SSE accept header, bearer
  header, account header, and stream idle timeout.

Live certification:

- Configure a direct OpenAI OAuth provider through the admin API.
- Assign it to a test profile.
- Run the unchanged Rusty View long Responses scenario that previously exposed
  den-router stalling.
- Certify that live chat streams initial output, tool-call activity, reasoning
  visibility where applicable, and terminal events without changing the test to
  avoid the failure.

Latest direct-path evidence:

- Rusty View broker run:
  `/home/agent/.cache/den-playwright/runs/rusty-view/rusty-view-20260702T105956.010098079Z-3045777/run-index.json`
- Session: `responses-live-cert-session-20260702T10594590-1`
- Result: long live Responses stream completed with 7,282 raw events, preserved
  beginning/end replay from cursor `0`, and did not depend on den-router.
