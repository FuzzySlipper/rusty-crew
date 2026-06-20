# Operator Surfaces E2E Proof

Task: Den `2965`

Rusty Crew now has an end-to-end smoke proving that admin diagnostics, health/readiness, the debug API client, the debug TUI renderer, direct-debug context, and guarded admin control all work over the same multi-agent diagnostics projection.

## Scenario

`npm run smoke:operator-surfaces-e2e` builds a two-agent runtime projection:

- `agent-alpha` / `session-alpha`
- `agent-beta` / `session-beta`
- distinct channel bindings
- distinct MCP surfaces
- degraded beta channel projection
- degraded beta MCP surface
- observation writer unavailable
- queue TTL pressure
- persistence/search health

The runtime health is degraded, but readiness remains true. This proves adapter/observation degradation is visible to operators without blocking internal routing.

## Covered Surfaces

The proof exercises:

- admin diagnostics overview
- readiness/liveness projection
- debug API client sessions/channels/MCP calls
- direct-debug context loading
- debug TUI state loading and rendering
- TUI keyboard reducer
- guarded admin control auth
- admin control audit events
- admin control observation events

## Boundary Notes

The smoke lives at the root under `ts/smokes` because it composes `@rusty-crew/brain-island` and `@rusty-crew/adapter-tui`. Keeping the proof at the root avoids forcing a package dependency between those two layers just for test composition.

The TUI continues to consume a structural API-client contract. A real debug API client satisfies that contract, but `adapter-tui` does not import the brain runtime package directly.
