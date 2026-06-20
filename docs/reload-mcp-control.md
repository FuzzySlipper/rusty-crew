# Reload MCP Control

Status: implementation note for task 2957

Rusty Crew wires `/reload-mcp` to the per-session/profile MCP surface reload path through `createReloadMcpControlExecutor` in `@rusty-crew/brain-island`.

The executor is designed to be mounted as `AdminControlExecutor.reloadMcp`, so slash commands and admin controls share the same guarded control path.

## Boundary

Reload is scoped to one MCP binding for one session/profile. The executor:

- resolves the binding for the requested session;
- fails if no binding exists;
- fails if the resolved binding belongs to a different session;
- calls the existing `reloadMcpSurface` helper;
- does not reload global MCP state;
- does not disturb other agents' MCP surfaces.

## Reported Outcome

The control outcome includes:

- binding ID;
- session ID;
- profile ID;
- reload status;
- old/new tool counts;
- added, removed, and unchanged tool names;
- collision count;
- discovery issue count;
- optional server failures;
- duration;
- reason;
- observation publish status when configured.

If reload returns a degraded report, the control outcome is failed with reason code `mcp_reload_degraded`. Missing binding or binding/session mismatch fail before reload.

## Audit And Observation

The executor can emit lifecycle audit phases:

- `reload_started`
- `reloaded`
- `degraded`

When configured with `AgentActivityObservationProducer`, it emits display-only adapter activity:

- `adapter_recovered` for a successful reload;
- `adapter_degraded` for degraded reload.

These observation events do not drive runtime behavior.

## Smoke Coverage

`npm run smoke:reload-mcp-control` verifies:

- `/reload-mcp` flows through the guarded admin control route;
- the executor reloads one session's MCP binding;
- old/new/added/removed tool details are returned;
- another session's MCP surface remains active;
- reload audit phases are emitted;
- observation emits an adapter recovered event;
- missing binding fails before reload;
- binding/session mismatch fails before reload.
