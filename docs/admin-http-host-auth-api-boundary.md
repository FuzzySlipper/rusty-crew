# Admin HTTP Host, Auth, And API Boundary

Status: Design contract for task 2951

Date: 2026-06-20

Depends on: `operator-control-plane-and-observation-architecture`

## Decision

Rusty Crew v1 admin HTTP should be a **TS-hosted service over typed Rust bridge
and diagnostics APIs**.

Rust remains lifecycle and coordination authority. TypeScript owns HTTP hosting,
auth integration, route shaping, JSON response ergonomics, and future Den/Gateway
deployment integration. This matches the broader architecture: Rust owns
deterministic runtime state; TS owns expression, projection, and platform
integration.

This is a hybrid boundary, not a TS authority transfer.

## Host Ownership

The admin host lives in the TS service layer for v1 because:

- the current bridge is already the supported TS/Rust boundary;
- Den, channel, MCP, observation, and future Gateway auth integration are
  TS-side adapter concerns;
- admin response shaping is projection work;
- Rust should not learn HTTP framework, auth proxy, or browser/debug UI details
  before diagnostics/control APIs stabilize.

Rust responsibilities:

- expose typed read/control APIs through core/bridge surfaces;
- validate lifecycle invariants;
- return structured errors and affected runtime IDs;
- emit durable/auditable runtime facts when a control command changes state.

TS admin host responsibilities:

- bind/listen according to deployment config;
- authenticate requests;
- authorize read versus control routes;
- call only typed bridge/control APIs;
- produce stable JSON response envelopes;
- redact sensitive data;
- write observation/audit events for admin lifecycle where configured.

## Bind And Deployment Posture

The initial local-machine deployment binds on the local network because the
operator normally reaches this host over SSH/LAN:

- host: `0.0.0.0`
- port: explicit config or the documented local default

This is a trusted-environment deployment posture, not a browser/public internet
posture. Other deployments can still choose loopback-only:

- host: `127.0.0.1`
- port: explicit config or a documented development default

When LAN binding is disabled by config, startup must fail closed if a
non-loopback bind host is requested. The local service config uses:

- `RUSTY_CREW_ADMIN_HOST=0.0.0.0`
- `RUSTY_CREW_ADMIN_ALLOW_LAN=true`

If `RUSTY_CREW_ADMIN_ALLOW_LAN=false`, a non-loopback host must fail closed
rather than quietly exposing admin routes contrary to the operator's config.

Future Den/Gateway deployment can proxy read routes to browser clients, but
write/control routes must not be exposed to untrusted browser contexts without
separate auth and CSRF posture.

## Authentication And Authorization

V1 auth uses bearer tokens.

Recommended config:

- `RUSTY_CREW_ADMIN_TOKEN` for local/dev single-token mode;
- future replacement with Den/Gateway-issued service tokens;
- optional separate read/write tokens if needed by deployment.

Rules:

- control routes require an authenticated bearer token with write/admin-control
  authorization;
- read-only diagnostics routes require bearer auth unless a local development
  config explicitly enables unauthenticated loopback reads;
- LAN bind must never allow unauthenticated diagnostics;
- health liveness may be unauthenticated only if it returns shallow process
  status and no runtime details;
- readiness, degraded status, diagnostics, sessions, queues, tools, and errors
  are authenticated read routes;
- invalid or missing auth fails before any body parsing that could trigger side
  effects.

Bearer tokens must not be logged, echoed in errors, emitted to observation, or
stored in runtime persistence.

## Route Versioning

Admin routes use `/v1/admin/...`.

Read-only route families:

- `GET /v1/admin/healthz`
- `GET /v1/admin/readyz`
- `GET /v1/admin/diagnostics`
- `GET /v1/admin/diagnostics/sessions`
- `GET /v1/admin/diagnostics/agents`
- `GET /v1/admin/diagnostics/delegations`
- `GET /v1/admin/diagnostics/queues`
- `GET /v1/admin/diagnostics/tools`
- `GET /v1/admin/diagnostics/mcp`
- `GET /v1/admin/diagnostics/channels`
- `GET /v1/admin/diagnostics/persistence`
- `GET /v1/admin/diagnostics/observation`
- `GET /v1/admin/events/recent`

Mutating control route families:

- `POST /v1/admin/control/sessions`
- `POST /v1/admin/control/sessions/{session_id}/archive`
- `POST /v1/admin/control/sessions/{session_id}/new`
- `POST /v1/admin/control/delegations/{session_id}/cancel`
- `POST /v1/admin/control/delegations/{session_id}/checkpoint`
- `POST /v1/admin/control/mcp/{session_id}/reload`
- `POST /v1/admin/control/maintenance`
- `POST /v1/admin/control/shutdown`

Control endpoints should be command-shaped. The TS route maps JSON into an
explicit typed request and calls the corresponding Rust control/bridge API.

## Response Envelope

All routes return a stable envelope:

```json
{
  "ok": true,
  "data": {},
  "meta": {
    "request_id": "req_...",
    "schema_version": 1
  }
}
```

Errors return:

```json
{
  "ok": false,
  "error": {
    "code": "unauthorized|forbidden|invalid_input|not_found|conflict|failed_precondition|internal_error",
    "reason_code": "machine_specific_reason",
    "message": "Human-readable summary.",
    "retryable": false
  },
  "meta": {
    "request_id": "req_...",
    "schema_version": 1
  }
}
```

Route handlers should preserve Rust `CoreErrorKind` where possible, but should
not leak stack traces, raw prompts, full tool outputs, bearer tokens, or private
adapter internals.

## Read Versus Control

Read-only routes:

- call diagnostics projection APIs;
- may page/filter/sort;
- must not call lifecycle methods;
- must not run maintenance implicitly;
- must not redeliver queued work;
- must not derive authoritative state from observation or channel prose.

Control routes:

- use `POST`;
- require write auth;
- validate target IDs and request body before side effects;
- call Rust-owned control APIs;
- return affected runtime IDs and final state summary;
- emit audit/observation events when configured;
- should accept an optional idempotency key for operations where duplicate
  clicks/retries could be harmful.

## Control Request Shape

Every mutating route should map to an explicit command type with:

- command name;
- target runtime IDs;
- optional Den refs;
- actor/auth identity;
- reason string or reason code;
- request ID;
- idempotency key when available.

The admin host must not reach into SQLite, adapter maps, MCP clients, or channel
bindings directly. If a needed operation has no Rust/bridge API yet, add that
API deliberately before implementing the route.

## Audit And Observation

Admin control outcomes should produce two different kinds of records:

1. **Runtime facts** when executable state changed. These are Rust-owned events
   or persisted state updates.
2. **Observation/audit projections** for human-visible breadcrumbs. These are
   display-only `agent_activity.v1` or future admin-audit events and must not
   drive runtime behavior.

Observation payloads should cite `work_ref`/`result_ref`/runtime IDs and avoid
copying full state machines.

## Security Guardrails

- Default loopback bind.
- LAN bind requires double opt-in.
- Bearer auth required for all control routes.
- Auth checked before side effects.
- Mutating routes use `POST`, not `GET`.
- No CORS wildcard on authenticated admin routes.
- No browser-exposed write route without CSRF posture.
- Redact secrets from diagnostics and errors.
- Limit request body size.
- Add rate limits or debounce for expensive diagnostics and control endpoints
  before non-local deployment.

## Implementation Guidance

Later tasks should proceed in this order:

1. Implement diagnostics projection core.
2. Add a TS admin host package or module over the native bridge.
3. Implement authenticated read-only diagnostics routes.
4. Implement guarded control routes one command family at a time.
5. Add debug API client and TUI over the same read/control API.
6. Prove multi-agent diagnostics and control behavior end to end.

Do not add admin-only runtime truth. If the admin UI needs a state field, add it
to the shared diagnostics projection or a typed Rust control response.
