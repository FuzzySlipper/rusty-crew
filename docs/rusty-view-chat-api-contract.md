# Rusty View Chat API Contract

Status: v0 implemented contract for the Rusty View chat support tasks.

Rusty View needs a durable browser-facing chat protocol over Rusty Crew sessions.
The existing admin diagnostics and direct-debug endpoints are useful references,
but they are not the stable chat contract. This document defines the intended
Rusty-owned surface before the implementation tasks wire it to runtime state.

The machine-readable source artifact is
[`rusty-view-chat-api-v0.openapi.json`](rusty-view-chat-api-v0.openapi.json).
Frontend protocol types should be generated from that artifact or from a later
Rust-derived replacement. Rusty View should not hand-copy backend shapes.

## Contract Source And Ratchets

For browser-facing chat HTTP/SSE envelopes, the current source of truth is the
OpenAPI artifact:

```bash
docs/rusty-view-chat-api-v0.openapi.json
```

The TypeScript constants in
`ts/packages/brain-island/src/rusty-view-chat-contract.ts` are ratchets against
that OpenAPI file for route paths, event kind names, and required core event
fields. They are not an independent source of truth.

Rust-owned native/domain request and result shapes remain governed by the bridge
manifest and its fingerprint/fixture checks:

```bash
cargo run -p rusty-crew-core-bridge-codegen -- check-contracts ts/packages/contracts/src/index.ts
cargo run -p rusty-crew-core-bridge-codegen -- check-fingerprint crates/bridge/core-bridge-api/bridge-wire-shape-fingerprint.txt ts/packages/contracts/src/index.ts
cargo run -p rusty-crew-core-bridge-codegen -- check-native-surface ts/packages/native-bridge/native/index.d.ts ts/packages/native-bridge/src/index.ts
```

Run these chat contract checks whenever routes, event kinds, cursor fields,
debug-detail references, or public chat mutation envelopes change:

```bash
npm run smoke:rusty-view-chat-contract
npm run smoke:rusty-view-chat-read-api
npm run smoke -- bridge-validation
```

`smoke:rusty-view-chat-contract` must fail when the OpenAPI artifact, TS route
constants, capability registry paths, cursor-bearing page envelopes, debug detail
schemas, or chat mutation conflict envelopes drift.

## Route Families

- `GET /v1/chat/sessions`: list chat-capable sessions.
- `GET /v1/chat/sessions/{session_id}`: open a session and return a bounded
  transcript page plus the latest cursor.
- `GET /v1/chat/sessions/{session_id}/events`: replay historical session
  events after an optional cursor.
- `GET /v1/chat/sessions/{session_id}/stream`: SSE stream for live and replayed
  events. Supports `Last-Event-ID` and an explicit `cursor` query. During an
  in-flight wake, assistant text deltas and tool lifecycle events are appended
  and flushed as the service observes them; clients do not need to wait for the
  whole wake to finish before rendering progress.
- `GET /v1/chat/sessions/{session_id}/tool-calls/{debug_detail_id}`: debug-only
  bounded raw tool-call inspection. Normal chat/SSE tool events stay browser-safe
  and expose only `debug_detail_id`/metadata references; clients call this route
  on demand for redacted arguments, partial updates, final result, error, and
  retention limits.
- `GET /v1/chat/sessions/{session_id}/provider-requests/{debug_detail_id}`:
  debug-only bounded provider request inspection. Provider status events expose
  `provider_request_debug_detail_id` metadata; clients call this route on demand
  to inspect the cached, redacted provider-facing prompt/tool payload.
- `POST /v1/chat/sessions/{session_id}/messages`: append a user message and
  request an agent wake.
- `GET /v1/chat/commands`: discover slash/debug commands.
- `POST /v1/chat/sessions/{session_id}/commands`: execute a chat command using
  the same guarded control paths as admin/slash surfaces.

## Event Log Rules

Every event emitted to Rusty View has:

- `event_id`: stable replay id suitable for SSE `id:`.
- `session_id`: target Rusty session.
- `sequence_id`: monotonic session-local integer cursor.
- `created_at`: RFC3339 timestamp.
- `kind`: closed known kind with safe handling for future unknowns.
- `payload`: event-specific object.

Initial known event kinds:

- `session_snapshot`
- `message_created`
- `assistant_turn_started`
- `assistant_text_delta`
- `assistant_reasoning_delta`
- `phase_change`
- `provider_status`
- `assistant_message_completed`
- `assistant_turn_finished`
- `tool_call_started`
- `tool_call_completed`
- `tool_call_failed`
- `command_started`
- `command_completed`
- `command_failed`
- `stream_error`
- `unknown`

Unknown future event kinds must not crash Rusty View. Debug Chat should render
them generically from `payload.summary` or raw JSON.

Tool lifecycle payloads may include `debug_detail_id` plus `metadata` for
inspection affordances. They must not include raw arguments, raw partial
updates, raw final results, credentials, or full stack traces inline.

## Command Support

Rusty has slash command routing and chat command discovery for `/help`,
`/status`, `/session`, `/new`, and `/reload-mcp`.

The chat registry contains command metadata: name, aliases, description,
argument schema, session-kind constraints, read-only versus mutating behavior,
auth/control requirements, and the backing admin control command when a slash
command executes through the guarded control plane.

The registry is derived from the canonical API/command registry in
`api-command-registry.ts`. Public chat OpenAPI paths are smoke-checked against
that same registry, and the read-only admin surface exposes the broader route
inventory at `GET /v1/admin/capabilities`.

`/new` keeps archive-and-create semantics. It never clears context in place and
never creates a new session implicitly from a normal message.

## Browser Boundary

Chat routes are browser-facing and distinct from admin/control routes. They must
support CORS and SSE headers deliberately without broadening admin mutability.
No endpoint should expose bearer tokens, profile secrets, full prompts, or full
tool payloads unless a separate debug route explicitly asks for them.

Auth posture:

- Household/LAN development may run with `RUSTY_CREW_ADMIN_AUTH_MODE=none`.
  In that mode Rusty View can connect to `/v1/chat/*` without a bearer token.
- Protected/local-token mode uses the existing bearer token requirement. Rusty
  View should send `Authorization: Bearer <token>` for chat requests when the
  service is configured with `RUSTY_CREW_ADMIN_AUTH_MODE=bearer`.
- Normal messages never create sessions implicitly. A new session remains an
  explicit command/control action such as `/new`.

CORS posture:

- CORS and `OPTIONS` preflight support are intentionally limited to
  `/v1/chat/*`.
- Chat preflight allows `GET`, `POST`, and `OPTIONS`, plus `authorization`,
  `content-type`, `idempotency-key`, `last-event-id`, and `x-request-id`
  request headers.
- SSE responses from `/v1/chat/sessions/{session_id}/stream` include the same
  chat CORS headers and keep admin/control routes outside this browser surface.
- Default tool lifecycle payloads are browser-safe: tool name, status/error
  state, wake id, and metadata. Raw tool arguments and results require a
  deliberate future debug mode and are not part of the default stream contract.

## Implementation Notes

Current Rusty pieces to reuse:

- direct-debug turn enqueueing/wake dispatch as inspiration for send-message;
- Rust-owned session/message/tool-call persistence;
- `routeSlashCommand` and guarded admin control executors for command execution;
- existing brain events and Den observation tool events as sources for chat
  event projection.

The chat API should not scrape Den Web, Den observation, or admin diagnostics as
authoritative transcript state. Those remain diagnostics/observation surfaces.
