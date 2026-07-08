# Operator Surfaces E2E Proof

Original task: Den `2965`

Updated for Den `4714` after the Rust authority slices for diagnostics,
storage query catalog, and mutating slash-command control plans.

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
- public admin capability registry at `/v1/admin/capabilities`
- slash command catalog metadata used by Rusty View command discovery
- Rust-owned control-plan metadata for `/new` and `/reload-mcp`
- storage query catalog route at `/v1/admin/storage/query-catalog`
- Rust module-schema-derived `simple_kv.entries` query metadata
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

## 4714 Certification Notes

Deterministic certification:

```bash
npm run smoke:operator-surfaces-e2e
```

The smoke now checks that the same backend truth is visible through:

- `handleAdminDiagnosticsRequest` for health/readiness and `/v1/admin/capabilities`;
- `chatCommandRegistry()` for Rusty View slash command discovery;
- `handleStorageQueryRequest` for storage query catalog discovery;
- guarded admin control for visible audit and observation events;
- the debug API client and debug TUI renderer.

The fixture intentionally keeps adapter diagnostics degraded while
`health.readiness.ready` remains true, proving external projection failures are
visible without blocking durable runtime readiness.

Live debug-service spot check:

```bash
curl -fsS http://127.0.0.1:9348/v1/admin/capabilities
curl -fsS http://127.0.0.1:9348/v1/admin/storage/query-catalog
curl -fsS http://127.0.0.1:9348/v1/admin/diagnostics
curl -fsS http://127.0.0.1:9348/
```

On 2026-07-08 after restarting `rusty-crew-debug.service`, the debug service
reported:

- six slash commands;
- `/reload-mcp` and `admin.control.mcp.reload` both backed by
  `plan_reload_mcp_control`;
- storage query catalog source `rust_bridge_read_model` with seven queries and
  `simple_kv.entries` present;
- diagnostics `ok` with readiness true;
- Rusty View HTML served from the same debug service root.
