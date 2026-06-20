# Debug TUI State Model And Renderer

Task: Den `2960`

Rusty Crew now has a debug TUI state model and bounded text renderer in `@rusty-crew/adapter-tui`.

## Scope

The TUI layer consumes the supported debug/admin API client surface. It does not inspect databases, adapter internals, or runtime private state directly.

The state loader collects:

- diagnostics overview and health
- sessions
- channel bindings
- MCP surfaces
- tool catalogs
- queue/TTL state
- persistence/search health
- observation status
- metrics
- recent events
- optional direct-debug context for one active session

## Renderer

The renderer is intentionally plain text for this stage. It produces bounded terminal output with tabs for:

- overview
- sessions
- channels
- MCP
- tools
- queues
- persistence
- observation
- events
- direct-debug context, when loaded

Degraded and blocked state is rendered with explicit badges. Tables include stable headers and selected-row markers.

## Navigation

The reducer handles:

- `ArrowLeft` / `ArrowRight` / `Tab` / `Shift+Tab` for tab movement
- `ArrowUp` / `ArrowDown` for row movement
- `r` to request refresh
- `q` to request quit

The model is read-only by default. Future controls should route through admin control APIs instead of mutating runtime state directly.

## Verification

`npm run smoke:debug-tui` covers loading through a fake `DebugApiClient`, degraded overview rendering, session navigation, direct-debug context rendering, refresh/quit state flags, and raw-prompt omission.
