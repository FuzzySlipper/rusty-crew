# Slash Command E2E Proof

Status: implementation note for task 2963

Rusty Crew now has an end-to-end slash command smoke proving that slash commands are control-plane commands, not tools or prompts.

The proof lives in `npm run smoke:slash-command-e2e`.

## Scenario

A channel-bound full/prime agent receives:

- `/help`
- `/status`
- `/session`
- `/new`
- `/reload-mcp`

The smoke proves:

- all slash commands are intercepted before LLM prompting;
- read-only commands produce bounded diagnostics-backed responses;
- responses project back to the originating channel binding;
- `/new` flows through guarded admin control and creates a fresh session boundary;
- `/new` emits lifecycle audit and observation events;
- `/reload-mcp` flows through guarded admin control and reloads the current session/profile MCP surface;
- `/reload-mcp` leaves another session's MCP surface active;
- admin audit and observation events are emitted.

## Boundary

The proof composes the existing surfaces:

- `routeSlashCommand`
- `buildReadOnlySlashCommandResponse`
- `handleAdminControlRequest`
- `createNewSessionLifecycleExecutor`
- `createReloadMcpControlExecutor`
- `AgentActivityObservationProducer`

No slash command is sent to the LLM. No slash command mutates adapter state directly.

## Smoke Output

Expected smoke output includes:

- projected commands: `help`, `status`, `session`, `new`, `reload-mcp`;
- `llmPrompted: 0`;
- a new current session ID after `/new`;
- lifecycle audit events for `/new`;
- reload audit events for `/reload-mcp`;
- observation event count greater than zero;
- another MCP surface still active after reload.
