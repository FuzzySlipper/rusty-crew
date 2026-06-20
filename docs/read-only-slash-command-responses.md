# Read-Only Slash Command Responses

Status: implementation note for task 2956

Rusty Crew now has diagnostics-backed response builders for read-only slash commands in `@rusty-crew/brain-island`.

`buildReadOnlySlashCommandResponse` turns the shared diagnostics projection into bounded, channel-friendly response payloads for:

- `/help`
- `/status`
- `/session`

## Boundary

The response builder is separate from the slash command router.

- The router decides whether text is a command and whether it should be intercepted.
- The response builder formats read-only diagnostics for channel surfaces.
- Mutating commands still return explicit control request drafts and run through guarded control APIs.

Responses do not include raw logs, secrets, prompt dumps, full tool output, private adapter handles, or unbounded payloads.

## `/help`

`/help` lists available commands for the current surface and includes availability caveats:

- slash commands are intercepted before the LLM;
- control commands require an authorized full/prime session by default;
- responses are bounded.

Surface-specific `allowedCommands` options are respected.

## `/status`

`/status` summarizes service/session health using runtime diagnostics and health projection:

- aggregate health;
- readiness;
- current session status;
- pending/expired queue counts;
- active sessions;
- degraded channel/MCP counts;
- recent error count;
- bounded issue summaries.

## `/session`

`/session` summarizes the current session:

- session, agent, profile, and kind;
- current diagnostics status and stale flag;
- brain turn count and tool count;
- channel binding count, presence, and status;
- MCP surface count, status, and collision count;
- bounded current-session issues and adapter errors.

If the session is missing from diagnostics, the response says so explicitly rather than guessing from channel state.

## Smoke Coverage

`npm run smoke:slash-command-responses` verifies:

- `/help` lists available commands and honors surface limits;
- `/status` reports readiness, queue counts, adapter degradation, and bounded issues;
- `/session` reports session identity, channel presence, MCP status, and tool count;
- missing session diagnostics are reported explicitly.
