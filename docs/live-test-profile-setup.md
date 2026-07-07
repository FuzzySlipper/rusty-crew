# Live-Test Profile Setup

This is the repeatable Rusty Crew setup path for Rusty View live certification.
It is test infrastructure, not production profile guidance.

The goal is to recreate a minimal `tester` profile after a clean storage reset
without relying on preserved DB rows or hand-edited service config. Use the
official admin APIs so the same path exercises provider aliases, local tool
profiles, create-profile defaults, runtime config refresh, brain registration,
and session creation.

Assume the Rusty Crew debug/test service is reachable at:

```bash
export CREW=${RUSTY_CREW_DEBUG_ADMIN_BASE_URL:-http://127.0.0.1:9348}
```

The long-lived local agent service is on port `9347` and uses PostgreSQL. Do
not run noisy live-certification setup against it unless the task explicitly
requires testing the production-like service. The debug service at port `9348`
uses SQLite and is the default target for disposable profiles, providers, and
chat sessions. See `docs/local-service-topology.md`.

If the local service requires an admin token, add:

```bash
curl ... -H "Authorization: Bearer <token>"
```

to the curl examples. Do not commit tokens or provider secrets.

## 1. Confirm The Service Is Ready

```bash
curl -fsS "$CREW/v1/admin/healthz" | jq .
```

The service should be ready enough for admin/profile work. During the
architecture remediation window, it is acceptable to restart the service or
reset local service data before this setup.

For Rust pi-agent live certification, the debug service must run the live Rust
brain path rather than the deterministic fake bridge path:

```bash
RUSTY_CREW_PI_AGENT_LIVE=1
RUSTY_CREW_PI_AGENT_STREAM_IDLE_TIMEOUT_MS=300000
```

These values belong in the local debug service environment file, normally
`/home/system/rusty-crew-debug/config/service.env`, and require a debug service
restart. The idle timeout is intentionally long for live certification because
real providers can spend meaningful time in reasoning or tool loops.

## 2. Create The Default Live Chat Provider

The default no-secret provider points at local den-router. The router owns
upstream credentials; Rusty Crew stores no secret for this alias. The
`den-router` provider convention supplies the expected non-secret API-key value
inside the brain module.

```bash
curl -fsS -X POST "$CREW/v1/admin/model-providers?refresh=apply" \
  -H "content-type: application/json" \
  --data-binary @- <<'JSON' | jq .
{
  "alias": "tester-chat",
  "status": "active",
  "protocol": "chat_completions",
  "providerKind": "den-router",
  "displayName": "Tester Chat",
  "description": "Live Rusty View certification chat-completions provider through local den-router.",
  "baseUrl": "http://127.0.0.1:18082/v1",
  "modelId": "deepseek-flash",
  "contextWindowTokens": 128000,
  "maxOutputTokens": 2048,
  "temperature": 0.2,
  "metadataJson": {
    "purpose": "rusty_view_live_certification",
    "credential_owner": "den-router"
  }
}
JSON
```

If `deepseek-flash` is unavailable on the target machine, select another
healthy non-fake den-router model and keep the alias `tester-chat`.

## 3. Seed And Inspect Local Tool Profiles

Default local tool profiles are seeded when the collection is read. The live
tester profile should use `full_agent` so tool/activity display can be tested
without creating a one-off tool policy.

```bash
curl -fsS "$CREW/v1/admin/local-tool-profiles" \
  | jq '.data.items[] | select(.id == "full_agent")'
```

Expected: `full_agent` exists, is enabled, and is read-only/system-owned.

## 4. Create The Tester Profile

Use the create-profile control route. The backend sets session id,
implementation id, agent id, runtime config entries, and registry records. Do
not manually invent those ids in Rusty View or local config.

```bash
curl -fsS -X POST "$CREW/v1/admin/control/profiles" \
  -H "content-type: application/json" \
  --data-binary @- <<'JSON' | jq .
{
  "profileId": "tester",
  "displayName": "Live Tester",
  "providerAlias": "tester-chat",
  "kind": "full",
  "localToolProfileId": "full_agent"
}
JSON
```

Expected result:

- profile id: `tester`;
- session id: normally derived by the backend, such as `tester-session`;
- brain module: `pi-agent-core` because `tester-chat` uses
  `chat_completions`;
- local tool profile: `full_agent`.

Read back the profile:

```bash
curl -fsS "$CREW/v1/admin/control/profiles/tester/read" \
  | jq .
```

## 5. Optional Reasoning/Responses Provider

When a reasoning-capable OpenAI Responses profile is needed, use the direct
OpenAI OAuth provider path documented in
`docs/direct-openai-oauth-responses-provider.md`. The green path is a model
provider with:

- `providerKind`: `openai`;
- `protocol`: `responses`;
- `baseUrl`: `https://chatgpt.com/backend-api/codex`;
- a typed `openai_oauth` credential stored through the provider OAuth admin
  flow;
- live service env:
  `RUSTY_CREW_OPENAI_RESPONSES_LIVE=1`,
  `RUSTY_CREW_OPENAI_RESPONSES_REQUIRE_NATIVE=1`.

After the OAuth provider is active, create a second profile such as
`tester-reasoning` using the same create-profile route with
`providerAlias` set to that Responses alias. The backend should default the
brain module to `openai-responses`.

Do not use the deterministic fake Responses mode for live Rusty View
certification.

## 6. Rusty View Live Variables

Point Rusty View live scenarios at the recreated profile:

```bash
export RV_LIVE_BACKEND_URL="$CREW"
export RV_LIVE_PROFILE=tester
export RV_LIVE_MIN_STREAMING_MS=15000
```

Run live certification through the broker as described in
`docs/live-deliverable-certification.md` and
`../rusty-view/docs/live-testing.md`.

If a manual Rusty View browser run is needed, make sure the app is opened with
the debug backend override:

```text
http://127.0.0.1:<rusty-view-port>/?api=http%3A%2F%2F127.0.0.1%3A9348
```

Without the `api` query parameter, the Rusty View dev app may derive a backend
from its served origin and accidentally talk to the live service on port `9347`.

## Long-Streaming Prompt Pattern

Use a prompt that asks for real analysis rather than artificial delay:

```text
Review the current Rusty Crew testing architecture from the perspective of a
future frontend developer. Compare unit tests, smokes, and live Rusty View
certification. Give a structured answer with at least five concrete risks, five
recommended improvements, and a short checklist for deciding which test layer a
new feature needs. Think carefully and include enough detail that I can inspect
streaming while the answer is still in progress.
```

If the answer completes too quickly for the scenario, make the prompt more
substantive. Do not add fake delay hooks or fake slow profiles just to satisfy
timing.

## Reset Notes

This setup is intentionally disposable. If local storage is reset, rerun these
API calls. If a provider update affects an active tester profile, use
`?refresh=apply` on the model-provider write or run the runtime rebuild/admin
refresh flow before live testing.
