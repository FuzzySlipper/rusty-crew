# Slash Command Router Control Plane

Status: implementation note for task 2954

Rusty Crew now has a frontend/channel-independent slash command router in `@rusty-crew/brain-island`.

The router is a pure interception layer. Channel adapters can call `routeSlashCommand` after binding resolution and before routing ordinary user text to the LLM/runtime.

## Boundary

Slash commands are not model-callable tools and should not appear in tool inventory. They are control-plane commands.

The router:

- recognizes slash command text;
- returns `pass_through` for ordinary messages;
- intercepts known and unknown slash commands before LLM prompting;
- returns structured responses that adapters can format;
- emits explicit control request drafts for mutating commands;
- does not mutate runtime, persistence, queues, adapters, MCP clients, or channel bindings.

Lifecycle operations still need to flow through guarded control APIs and Rust-owned runtime control.

## Commands

Initial commands:

- `/help`
- `/status`
- `/session`
- `/new`
- `/reload-mcp`

`/help`, `/status`, and `/session` currently return structured read-only response shells. Later command response work can enrich these with diagnostics data without changing the interception boundary.

`/new` returns a `new_session` control request draft targeting the current session. It represents archive-and-create semantics, not in-place context clearing.

`/reload-mcp` returns a `reload_mcp` control request draft targeting the current session.

Unknown slash commands are intercepted and return command help rather than reaching the LLM as normal text.

## Authorization Rule

By default, commands operate only for full sessions whose profile is `prime`.

Options can explicitly allow:

- additional prime profile names;
- read-only commands for non-prime sessions;
- control commands for worker sessions;
- a reduced allowed-command set for a specific surface.

Denied commands return a structured denied response and no control request.

## Adapter Use

Expected adapter flow:

1. Resolve channel binding to a concrete session/profile/agent.
2. Apply duplicate/TTL checks.
3. Call `routeSlashCommand`.
4. If `pass_through`, route message normally.
5. If intercepted with a read-only response, format and send the response.
6. If intercepted with a control request, submit it to the guarded control API or equivalent control executor.

Adapters should not run slash commands by mutating their own private state.

## Smoke Coverage

`npm run smoke:slash-command-router` verifies:

- ordinary text passes through;
- `/help`, `/status`, and `/session` are intercepted;
- `/new` produces a `new_session` control request;
- `/reload-mcp` produces a `reload_mcp` control request;
- worker control commands are denied by default;
- explicit options can allow worker controls or non-prime reads;
- unknown commands are intercepted and rendered as command help;
- per-surface allowed command limits deny unavailable commands.
