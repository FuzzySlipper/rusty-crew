# Model Providers

Rusty Crew stores model providers as service-owned database records. A provider
record gives a reusable alias to one model endpoint and its protocol,
generation limits, optional reasoning controls, and credential. Profiles point
to the alias; they do not carry private inline provider fallback configuration.

The machine-readable API contract is
[`model-provider-admin-api-v0.openapi.json`](model-provider-admin-api-v0.openapi.json).
The concise route contract is
[model-provider-admin-api-contract.md](model-provider-admin-api-contract.md).

## Supported Protocols And Brains

Two provider protocols are supported:

| Protocol | Production brain | Provider expectation |
| --- | --- | --- |
| `chat_completions` | `chat-completions` | OpenAI-compatible chat-completions endpoint |
| `responses` | `openai-responses` | OpenAI Responses-compatible endpoint |

The provider protocol is authoritative for the default brain selection. A
profile assigned to `chat_completions` resolves to `chat-completions`; a profile
assigned to `responses` resolves to `openai-responses`. An explicitly configured
brain must be compatible with the protocol or registration fails.

`providerKind` is an explicit classification selected from `custom`, `local`,
`den-router`, `openai`, `openai-compatible`, `openrouter`, `deepseek`, or
`moonshot`; it is not a substitute for `protocol`. `openai` enables the OpenAI
OAuth provider flow and `openrouter` enables guarded Anthropic prompt caching.
The other values are routing/diagnostic labels retained for existing
configurations. The admin API rejects unrecognized values rather than silently
accepting an ambiguous free-form label on create or update. Read responses keep
`providerKind` open as a string so historical records with earlier free-form
values remain visible and can be migrated through the admin API.

Current green paths are:

- an API-key-backed OpenAI-compatible chat-completions service;
- local den-router chat completions, where the router owns upstream secrets;
- an API-key-backed Responses-compatible service;
- direct OpenAI OAuth Responses through the ChatGPT/Codex endpoint.

Provider compatibility still depends on the remote endpoint implementing the
selected wire protocol. Rusty Crew does not translate Responses semantics into
chat completions or vice versa.

## Provider Transport Interruptions

The Chat Completions brain retries the same frozen request when the provider
connection fails before any semantic provider event arrives. This includes a
request-send failure, a response-body I/O failure before the first SSE event,
and explicitly transient HTTP statuses `429`, `502`, `503`, and `504`.

Connection retries use cancellation-aware exponential backoff capped at five
seconds. Transient HTTP responses honor `Retry-After` seconds or HTTP-date
headers, bounded to five minutes; otherwise they use the same exponential
backoff. Neither path has a finite retry count. Distinct degraded and recovered
provider-status events include attempts, backoff, status, and safe timing
metadata. Error bodies are bounded and credential-marked bodies are redacted.
No tool is re-executed by either retry path.

Permanent HTTP statuses, request timeouts, explicit cancellation, malformed
provider protocol, provider-declared errors, and failures after any semantic
stream event remain distinct terminal outcomes. Once provider output has begun,
Crew cannot assume reissuing the request is safe or replay partial output
without duplication. If an error response body itself fails, Crew retains the
already-known HTTP status so a transient status does not become an unrelated
generic transport failure.

Prefer provider/router hot reload or a drained rolling restart when active Crew
turns may be using the endpoint. If an unavoidable restart drops only
not-yet-answered connections, Crew waits for the endpoint to return. Operators
can cancel the turn explicitly if the outage should not be waited out.

## Provider Fields

| Field | Meaning |
| --- | --- |
| `alias` | Stable lowercase ID referenced by profiles |
| `status` | `active`, `disabled`, or `archived` |
| `protocol` | `chat_completions` or `responses` |
| `providerKind` | Routing/diagnostic label |
| `displayName`, `description` | Operator-facing labels |
| `baseUrl` | Provider API base URL |
| `modelId` | Model identifier sent to the provider |
| `contextWindowTokens` | Declared context capacity used by context diagnostics/policy |
| `maxOutputTokens` | Maximum requested response length |
| `temperature` | Decimal generation temperature |
| `temperatureMilli` | Integer storage form, `temperature * 1000` |
| `reasoningEffort` | Provider-specific reasoning effort string |
| `reasoningFormat` | Provider-specific reasoning/output format string |
| `responsesDialect` | Required Responses wire dialect: `openai_stateful`, `openai_stateless`, `generic_stateless`, `deepseek`, or `meta` |
| `chatCompletionsDialect` | Typed Chat Completions wire dialect: `standard`, `kimi`, `glm`, `qwen`, or `deepseek` |
| `thinkingMode` | `provider_default`, `enabled`, or `disabled` |
| `reasoningHistory` | `provider_default`, `discard`, `preserve_all`, or `tool_calls_only` |
| `reasoningBudgetTokens` | Optional Qwen-only thinking budget |
| `promptCaching` | `disabled`, `automatic_5m`, or `automatic_1h` for typed Anthropic/OpenRouter cache control |
| `credentialSecret` | Typed secret envelope for API key or OAuth material |
| `metadataJson` | Non-secret provider-specific metadata |
| `expectedRevision` | Optimistic concurrency revision for updates |

The API accepts decimal `temperature`, including values below `1`, and
normalizes it to `temperatureMilli`. Readback includes both forms when set. An
omitted or `null` temperature clears the override so the upstream provider
chooses its default. Do not use a negative sentinel and do not send a decimal
value in `temperatureMilli`.

Reasoning values are deliberately strings because provider vocabularies differ.
They are passed only where the selected brain/provider path supports them; they
do not convert a chat-completions endpoint into a Responses endpoint.

The native Responses path maps `reasoningEffort` to `reasoning.effort` and
`maxOutputTokens` to `max_output_tokens`. The native Chat Completions path maps
`reasoningEffort` to `reasoning_effort`. `reasoningFormat` remains diagnostic
metadata until a protocol-specific mapping is configured; `/model` reports a
warning instead of claiming it was applied.

### Responses Dialects

Responses wire behavior is explicit provider configuration. Crew does not infer
it from the provider alias, URL, model ID, or `providerKind`.

| Dialect | Continuation | Provider-specific behavior |
| --- | --- | --- |
| `openai_stateful` | May use `previous_response_id` | Emits OpenAI stateful request extensions |
| `openai_stateless` | Replays complete provider state | Uses the OpenAI request/event shape without server-side chaining |
| `generic_stateless` | Replays complete provider state | Omits provider-specific state and request extensions |
| `deepseek` | Replays complete provider state | Preserves plain-text reasoning items and accepts DeepSeek reasoning/tool SSE events |
| `meta` | Replays complete provider state or may use `previous_response_id` | Uses the Meta Responses wire dialect, includes `summary: []` for reasoning replay, and requests encrypted reasoning for stateless replay |

`openai_stateful` and `meta` may use the `previous-response-chain` brain strategy.
Stateless dialects retain ordinary messages, reasoning, tool calls, and tool
outputs in Crew provider state and replay that state on each request. A write
with `protocol: "responses"` must include `responsesDialect`; a Chat Completions
provider must not include it.

DeepSeek's Responses implementation is stateless. Crew therefore stores the
complete ordered replay projection, places prior provider output before the new
user turn, and strips ephemeral provider item IDs. During tool rounds, Crew
passes the exact streamed `reasoning_text` back before the adjacent function
calls, followed by all function outputs. This ordering is required for both
sequential and parallel calls. DeepSeek's API manages prompt caching
automatically; Crew sends no OpenAI prompt-cache extensions and reports cached
input and reasoning-output token counts from provider usage.

The `deepseek` dialect follows DeepSeek's
[Responses API compatibility guide](https://api-docs.deepseek.com/guides/responses_api/).
That guide currently lists `deepseek-v4-flash` as the supported Responses model.
Provider support is explicit configuration and is never inferred from this
model name or the DeepSeek endpoint.

### Chat Completions Reasoning Dialects

Chat Completions reasoning controls are explicit provider configuration. Crew
does not infer them from model names, endpoint URLs, or `providerKind`.

All history policies retain ordinary ordered user, assistant, tool-call, and
tool-result messages across wakes. `provider_default` strips historical
`reasoning_content` without sending a vendor history control. `discard` strips
all historical reasoning and emits a dialect-specific clear control where one
exists. `preserve_all` retains exact structured reasoning with the ordinary
history and emits a dialect's preservation control where one exists.
`tool_calls_only` retains reasoning only on assistant messages containing tool
calls; it is available only with the `deepseek` dialect.

| Dialect | Thinking control | Historical reasoning control | Budget | Assistant history |
| --- | --- | --- | --- | --- |
| `standard` | none | none | unsupported | Omits vendor extensions; non-default dialect settings are rejected |
| `kimi` | `thinking.type` | `thinking.keep: null \| "all"` | unsupported | Replays exact `reasoning_content` during tool loops and, with `preserve_all`, across wakes |
| `glm` | `thinking.type` | `thinking.clear_thinking: true \| false` | unsupported | Uses exact structured `reasoning_content` history when preserved |
| `qwen` | `enable_thinking` | `preserve_thinking` | `thinking_budget` | Uses exact structured `reasoning_content` history when preserved |
| `deepseek` | `thinking.type` | message filtering only | unsupported | With `tool_calls_only`, replays exact reasoning on every assistant tool-call message across later requests and drops non-tool reasoning |

Vendor settings fail closed when combined with `standard` or a non-chat
protocol. `reasoningBudgetTokens` additionally requires the `qwen` dialect and
`thinkingMode: "enabled"`. Disabling thinking cannot be combined with an
explicit history policy.

DeepSeek's current thinking-mode tool contract requires exact
`reasoning_content` from assistant tool-call messages in every later request;
omitting it can produce HTTP 400. Configure `chatCompletionsDialect:
"deepseek"`, `thinkingMode: "enabled"`, and `reasoningHistory:
"tool_calls_only"` for that contract. `discard` remains available for legacy
DeepSeek reasoning endpoints that reject historical reasoning. Crew does not
infer either behavior from a DeepSeek-looking model ID or URL.

For Kimi thinking models, Crew rejects an explicit temperature and requires
`maxOutputTokens >= 16000`; it never silently changes either value. Kimi K2.7
always preserves thinking, so configure `reasoningHistory: "preserve_all"` and
retain every assistant `reasoning_content` value exactly. These constraints
follow Moonshot's tool-loop guidance and are enforced at provider-write and
Rust brain boundaries.

### Anthropic Prompt Caching Through OpenRouter

Prompt caching is disabled unless the provider record selects `promptCaching`.
The enabled policies are accepted only for `chat_completions` records with
`providerKind: "openrouter"` and an `anthropic/*` model ID. Unsupported
combinations fail during the provider write rather than silently omitting the
directive.

`automatic_5m` sends top-level `cache_control: {"type":"ephemeral"}`.
`automatic_1h` additionally sends `"ttl":"1h"`. Both policies send a stable,
non-secret OpenRouter `session_id` derived from the Crew session identity so
sticky routing survives later wakes and Crew restarts. Diagnostics report the
effective policy, sticky identifier, prompt tokens, cached prompt tokens, and
cache-write prompt tokens.

The message/tool prefix remains in normal deterministic request order; Crew
does not insert cache markers into individual content blocks. The disabled
policy emits neither `cache_control` nor `session_id`.

## Admin API

Provider administration uses these routes:

- `GET /v1/admin/model-providers`
- `POST /v1/admin/model-providers`
- `GET /v1/admin/model-providers/{alias}`
- `PATCH /v1/admin/model-providers/{alias}`
- `GET /v1/admin/model-providers/{alias}/oauth/openai/status`
- `POST /v1/admin/model-providers/{alias}/oauth/openai/start`
- `POST /v1/admin/model-providers/{alias}/oauth/openai/complete`
- `POST /v1/admin/model-providers/{alias}/oauth/openai/clear`

The examples below assume tokenless trusted-local admin. In bearer mode add:

```bash
-H "Authorization: Bearer $RUSTY_CREW_ADMIN_TOKEN"
```

## Chat-Completions Example

```bash
export CREW=http://127.0.0.1:9348

curl -fsS -X POST "$CREW/v1/admin/model-providers?refresh=apply" \
  -H "Content-Type: application/json" \
  --data-binary @- <<'JSON' | jq .
{
  "alias": "local-chat",
  "status": "active",
  "protocol": "chat_completions",
  "providerKind": "den-router",
  "displayName": "Local Chat",
  "baseUrl": "http://127.0.0.1:18082/v1",
  "modelId": "deepseek-flash",
  "contextWindowTokens": 128000,
  "maxOutputTokens": 4096,
  "temperature": 0.5,
  "metadataJson": {
    "credential_owner": "den-router"
  }
}
JSON
```

This no-secret example is specific to a proxy that owns upstream credentials.

For an OpenAI-compatible chat-completions provider that accepts image content
parts, opt in explicitly through non-secret metadata:

```json
{
  "metadataJson": {
    "narrator_image_input": {
      "supported": true,
      "max_images": 4,
      "max_image_bytes": 10485760,
      "max_total_bytes": 20971520
    }
  }
}
```

Crew treats image input as unsupported when this declaration is absent. The
optional bounds may only lower Crew's hard limits. This capability applies to
opted-in Roleplay attachment links and does not make generated images part of
ordinary transcript or provider-state history.

For a normal API-key provider, include a typed secret:

```json
{
  "credentialSecret": {
    "kind": "api_key",
    "version": 1,
    "value": "provider-api-key"
  }
}
```

The legacy top-level `apiKey`/`secret` input remains accepted by the current
API, but new clients should use `credentialSecret` so credential type is
explicit.

For a Kimi K2.7 thinking provider, omit temperature and select the typed wire
policy explicitly:

```json
{
  "alias": "kimi-k2.7",
  "status": "active",
  "protocol": "chat_completions",
  "providerKind": "moonshot",
  "baseUrl": "https://api.moonshot.ai/v1",
  "modelId": "kimi-k2.7-code",
  "contextWindowTokens": 262144,
  "maxOutputTokens": 32768,
  "chatCompletionsDialect": "kimi",
  "thinkingMode": "provider_default",
  "reasoningHistory": "preserve_all"
}
```

## Responses Example

For a standard API-key Responses endpoint:

```bash
curl -fsS -X POST "$CREW/v1/admin/model-providers?refresh=apply" \
  -H "Content-Type: application/json" \
  --data-binary @- <<'JSON' | jq .
{
  "alias": "openai-responses-api",
  "status": "active",
  "protocol": "responses",
  "responsesDialect": "openai_stateless",
  "providerKind": "openai",
  "baseUrl": "https://api.openai.com/v1",
  "modelId": "your-responses-model",
  "contextWindowTokens": 200000,
  "maxOutputTokens": 8192,
  "reasoningEffort": "medium",
  "credentialSecret": {
    "kind": "api_key",
    "version": 1,
    "value": "provider-api-key"
  }
}
JSON
```

Use a model ID actually available to the credential. Model availability is a
provider/account concern and is not inferred from Rusty Crew's provider
catalog.

For DeepSeek's direct Responses endpoint, use the dedicated stateless dialect:

```json
{
  "alias": "deepseek-flash-responses",
  "status": "active",
  "protocol": "responses",
  "responsesDialect": "deepseek",
  "providerKind": "deepseek",
  "baseUrl": "https://api.deepseek.com",
  "modelId": "deepseek-v4-flash",
  "reasoningEffort": "medium",
  "credentialSecret": {
    "kind": "api_key",
    "version": 1,
    "value": "provider-api-key"
  }
}
```

DeepSeek Responses is stateless: Crew does not send `previous_response_id`,
`conversation`, `store`, or OpenAI-only include/cache/service-tier fields. It
replays plain-text reasoning content together with visible messages and tool
history. DeepSeek may report cache-hit input tokens in usage; those metrics are
observational and do not change Crew's continuation strategy.

## Direct OpenAI OAuth

Direct OpenAI OAuth is the green path for ChatGPT/Codex-authenticated Responses
use without den-router. First create an active provider with:

```json
{
  "alias": "gpt-oauth",
  "status": "active",
  "protocol": "responses",
  "responsesDialect": "openai_stateful",
  "providerKind": "openai",
  "baseUrl": "https://chatgpt.com/backend-api/codex",
  "modelId": "gpt"
}
```

Then use the provider's OAuth routes:

1. `POST .../oauth/openai/start` returns a pending login and authorization URL.
2. Open the URL and complete login.
3. `POST .../oauth/openai/complete` with the final `callbackUrl` from the
   browser.
4. Confirm `GET .../oauth/openai/status` reports a stored credential.

The default registered callback is localhost. A remote/LAN operator can paste
the complete localhost callback URL back to Crew; Crew validates its state and
uses the original PKCE verifier. Do not paste token bundles or verifier data
into provider metadata.

See [direct-openai-oauth-responses-provider.md](direct-openai-oauth-responses-provider.md)
for the full callback, refresh, account-header, and diagnostic contract.

## Credential Handling

Provider secrets live in Crew storage as typed envelopes. Public provider
readback exposes only a redacted summary such as credential kind, whether a
secret exists, and non-secret account metadata. It never returns API keys,
access tokens, refresh tokens, ID tokens, or PKCE verifier material.

The database and its backups are still secret-bearing infrastructure. Protect
them accordingly. Do not put real credentials in `service.json`, repo docs,
profile files, shell history, or committed fixtures.

Use `clearSecret: true` to remove a credential. A write cannot set and clear a
secret in the same request.

## Updating A Provider

Provider writes use optimistic revisions. Read the current provider, include
its `revision` as `expectedRevision`, and patch it:

```bash
current=$(curl -fsS "$CREW/v1/admin/model-providers/local-chat")
revision=$(jq -r '.data.revision' <<<"$current")

curl -fsS -X PATCH \
  "$CREW/v1/admin/model-providers/local-chat?refresh=apply" \
  -H "Content-Type: application/json" \
  --data-binary @- <<JSON | jq .
{
  "modelId": "deepseek-flash",
  "protocol": "chat_completions",
  "providerKind": "den-router",
  "baseUrl": "http://127.0.0.1:18082/v1",
  "temperature": 0.5,
  "expectedRevision": $revision
}
JSON
```

The current `PATCH` route accepts the full `ModelProviderWrite` shape rather
than JSON Merge Patch semantics. Read the latest record and preserve fields you
do not intend to change; omitted optional fields may be cleared and omitted
defaulted fields may return to their defaults.

A stale write returns HTTP `409` with reason
`model_provider_revision_mismatch`, the expected/current revisions, and the
current provider. Clients should reconcile and retry intentionally rather than
blindly overwriting another edit.

The `refresh` mode may be supplied in the query or body:

- `none`: persist only;
- `plan`: report affected profiles/sessions without rebuilding;
- `apply`: persist and apply the guarded runtime refresh.

Use `refresh=apply` for normal operator edits that should affect running
profiles. Runtime rebuild preserves the durable session identity and transcript
while replacing provider/brain runtime state according to the brain's provider
state policy.

## Assigning A Provider To A Profile

Create profiles through the official profile control API and supply only the
provider alias:

```bash
curl -fsS -X POST "$CREW/v1/admin/control/profiles" \
  -H "Content-Type: application/json" \
  --data-binary @- <<'JSON' | jq .
{
  "profileId": "research-prime",
  "displayName": "Research Prime",
  "providerAlias": "local-chat",
  "kind": "full",
  "localToolProfileId": "full_agent"
}
JSON
```

Crew derives the agent, session, and implementation IDs and selects the brain
from provider protocol. Frontends should not ask operators to invent those IDs
or duplicate endpoint/model fields inside each profile.

## Diagnostics And Validation

Read back a provider and its runtime effect rather than assuming a write was
applied:

```bash
curl -fsS "$CREW/v1/admin/model-providers/local-chat" | jq .
curl -fsS "$CREW/v1/chat/sessions/research-prime-session/context" | jq .
```

Useful contract checks:

```bash
npm run smoke:model-provider-admin-contract
npm run smoke:brain-catalog
```

Live certification must use a real provider through the debug service. The
repeatable setup is documented in
[live-test-profile-setup.md](live-test-profile-setup.md).
