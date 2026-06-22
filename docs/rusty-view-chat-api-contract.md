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

## Route Families

- `GET /v1/chat/sessions`: list chat-capable sessions.
- `GET /v1/chat/sessions/{session_id}`: open a session and return a bounded
  transcript page plus the latest cursor.
- `GET /v1/chat/sessions/{session_id}/events`: replay historical session
  events after an optional cursor.
- `GET /v1/chat/sessions/{session_id}/stream`: SSE stream for live and replayed
  events. Supports `Last-Event-ID` and an explicit `cursor` query.
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

## Command Support

Rusty has slash command routing and chat command discovery for `/help`,
`/status`, `/session`, `/new`, and `/reload-mcp`.

The chat registry contains command metadata: name, aliases, description,
argument schema, session-kind constraints, read-only versus mutating behavior,
and auth/control requirements.

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

## Implementation Notes

Current Rusty pieces to reuse:

- direct-debug turn enqueueing/wake dispatch as inspiration for send-message;
- Rust-owned session/message/tool-call persistence;
- `routeSlashCommand` and guarded admin control executors for command execution;
- existing brain events and Den observation tool events as sources for chat
  event projection.

The chat API should not scrape Den Web, Den observation, or admin diagnostics as
authoritative transcript state. Those remain diagnostics/observation surfaces.
