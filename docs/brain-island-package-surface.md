# Brain Island Package Surface

`@rusty-crew/brain-island` exposes its package root from
`ts/packages/brain-island/src/index.ts`. That root is a compatibility boundary
for service-host and operator smokes, not a place for implementation bodies to
accumulate.

Known local package-root consumers:

- `ts/packages/service-host/src/index.ts`
- `ts/packages/service-host/src/preflight.ts`
- `ts/smokes/operator-surfaces-e2e.ts`

Current root surface groups:

- Core brain runtime: local brain helpers, pi-agent brain construction, bridge
  wake helpers.
- Service/config: service app creation, service config loading, defaults, lock
  helpers.
- Tools: local code, delegation, completion, coordination, memory, web, skills,
  browser, MCP, registry, and profile selection tools.
- Diagnostics and observation: runtime health, runtime diagnostics, adapter
  diagnostics, tool context diagnostics, activity observation.
- Admin and commands: admin diagnostics/control routes, slash command routing,
  new-session and reload-MCP control paths, API command registries.
- Profiles/config/context: profile loading, profile registry import/export/admin
  helpers, runtime config validation, role assembly, context strategy and
  context estimate helpers.
- Memory/roleplay: memory space APIs, dense profile memory, Den memory tools,
  lore memory, scene state, roleplay narrator brain.
- Browser/debug/background: debug API/direct debug, browser tools, scheduled host
  executors, background governance/review loops, curator lifecycle and mutation
  helpers.

Guardrail:

```bash
npm run smoke:brain-island-entrypoint-surface -w @rusty-crew/brain-island
```

This smoke intentionally checks representative runtime root exports from each
group. Type-only exports are guarded by `npm run typecheck`; implementation
slices should run both before changing the root export layout.
