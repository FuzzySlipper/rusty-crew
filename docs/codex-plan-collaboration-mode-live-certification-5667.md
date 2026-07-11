# Codex Plan Collaboration Mode Live Certification

Task: #5667  
Date: 2026-07-11  
Verdict: **passed**

## Command

The certification used a disposable SQLite Crew engine, the browser-safe HTTP
routes, the supervised Codex app-server Unix WebSocket, normal Codex OAuth, and
the real provider:

```bash
CODEX_APP_SERVER_RESTART_SERVICE=1 \
CODEX_APP_SERVER_SERVICE_LIVE_TIMEOUT_MS=420000 \
  npm run smoke:external-runtime-service-live -w @rusty-crew/brain-island
```

## Plan Interaction Evidence

The HTTP message write selected `collaborationMode: "plan"`. Rust persisted
that selection on the external turn request, and the controller passed an
advertised Codex Plan preset to `turn/start` while retaining
`approvalPolicy: "never"` and `sandboxPolicy: { "type": "dangerFullAccess" }`.
The advertised Plan mask used the active thread model because Codex 0.144.1
reports `model: null` for the built-in Plan preset.

The live model called `request_user_input` with question ID
`certification_color`. Crew exposed the pending request through:

```text
GET /v1/external-interactions
```

The durable turn remained in `waiting_interaction`, and no terminal runtime
event existed before the browser-shaped resolution was submitted:

```json
{
  "expectedRevision": 0,
  "idempotencyKey": "external-service-live-plan-input-resolution",
  "result": {
    "answers": {
      "certification_color": {
        "answers": ["blue"]
      }
    }
  }
}
```

The resolution route was:

```text
POST /v1/external-interactions/codex-service-live%3A30/resolve
```

It resumed the same native turn
`019f5118-3ed4-78c2-8895-b45449c45d89`, which completed with the streamed
assistant text `PLAN_MODE_INPUT_OK:blue`.

## Companion Evidence

The same run also passed ordered queued turns, idempotent delivery replay,
Codex-to-Codex correlated round messaging, SSE replay, steer, interrupt,
app-server replacement, exact-thread restart/resume, and controller lease
generation advancement from 1 to 2. The primary retained native thread was
`019f5118-26a1-7be2-a7f9-4a2982a37f81`.

This command is the rerunnable service-level artifact for Rusty View. A browser
client should send the optional typed collaboration mode on the normal external
binding message route, render pending interactions from the interaction list,
and post the answer map to the interaction resolution route.
