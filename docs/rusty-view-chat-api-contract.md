# Rusty View Chat API Contract

Status: v0 contract spike for task 3171.

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

Rusty already has slash command routing for `/help`, `/status`, `/session`,
`/new`, and `/reload-mcp`. That is not yet a command registry API.

The chat contract adds a discoverable registry containing command metadata:
name, aliases, description, argument schema, session-kind constraints,
read-only versus mutating behavior, and auth/control requirements.

`/new` keeps archive-and-create semantics. It never clears context in place and
never creates a new session implicitly from a normal message.

## Browser Boundary

Chat routes are browser-facing and distinct from admin/control routes. They must
support CORS and SSE headers deliberately without broadening admin mutability.
No endpoint should expose bearer tokens, profile secrets, full prompts, or full
tool payloads unless a separate debug route explicitly asks for them.

## Implementation Notes

Current Rusty pieces to reuse:

- direct-debug turn enqueueing/wake dispatch as inspiration for send-message;
- Rust-owned session/message/tool-call persistence;
- `routeSlashCommand` and guarded admin control executors for command execution;
- existing brain events and Den observation tool events as sources for chat
  event projection.

The chat API should not scrape Den Web, Den observation, or admin diagnostics as
authoritative transcript state. Those remain diagnostics/observation surfaces.
