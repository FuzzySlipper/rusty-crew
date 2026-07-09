# Brain Island Package Surface

`@rusty-crew/brain-island` exposes its package root from
`ts/packages/brain-island/src/index.ts`. That root is a compatibility boundary
for service-host and operator smokes, not a place for implementation bodies to
accumulate.

Known local package-root consumers:

- `ts/packages/service-host/src/index.ts`
- `ts/packages/service-host/src/preflight.ts`
- `ts/smokes/operator-surfaces-e2e.ts`

Current root entrypoint shape:

- `src/index.ts` re-exports `local-brain.ts` plus the explicit domain modules
  in `src/package-surface/`.
- `src/local-brain.ts` owns the local/default brain implementation, bridge wake
  executor adapter, and related core brain wake types.
- `src/package-surface/service.ts` owns service app/config/root adapter factory
  exports.
- `src/package-surface/observation.ts` owns activity observation and runtime
  activity observer exports.
- `src/package-surface/diagnostics.ts` owns runtime, adapter, background, tool
  context, and storage-query diagnostics exports.
- `src/package-surface/admin.ts` owns admin routes, admin control, slash command,
  API capability, new-session, and reload-MCP control exports.
- `src/package-surface/background.ts` owns scheduler/cron/background-control and
  delegated cleanup exports.
- `src/package-surface/debug.ts` owns direct-debug and debug API client exports.
- `src/package-surface/brain.ts` owns pi-agent, bridge wake, brain module,
  tool-session, and mid-turn exports.
- `src/package-surface/tools.ts` owns local code, delegation, completion,
  coordination, web, skills, planning, patch, tool registry, and tool-profile
  exports.
- `src/package-surface/memory.ts` owns Den/dense memory tools, memory spaces,
  curator, capture proposal, session digest, and memory review exports.
  Exporting these wrappers does not make TypeScript the durable memory policy
  owner; see `docs/typescript-memory-surface-inventory-2026-07-08.md` for the
  current Rust policy/storage boundary.
- `src/package-surface/roleplay.ts` owns lore memory, scene state, and roleplay
  narrator exports.
- `src/package-surface/mcp-browser.ts` owns MCP integration/telemetry and browser
  tool/session exports.
- `src/package-surface/profile-context.ts` owns profile loading, profile registry
  import/export/admin helpers, runtime config validation, role assembly, context
  strategy, context estimate, and context compaction exports.

The tool/profile/prompt cluster has an additional Rust-authority inventory in
`docs/typescript-tool-profile-prompt-surface-inventory-2026-07-08.md`. That note
classifies local code, patch, web, browser, skills, MCP, profile loading,
profile role assembly, context strategy/estimate, and delegated role assembly
surfaces as execution wrappers, prompt renderers, adapter glue,
provider/client implementations, diagnostic estimators, or temporary policy
facades.

Root surface groups:

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

Final decomposition validation should include:

```bash
npm run typecheck
npm run smoke:brain-island-entrypoint-surface -w @rusty-crew/brain-island
npm run smoke:tool-profile-prompt-authority -w @rusty-crew/brain-island
npm run smoke:operator-surfaces-e2e
npm run service:preflight
```

Domain slices should also run representative smokes for the package-surface
module they touch. The full `smoke:service-host` script may start a real host and
can take longer than the narrow compatibility checks; use narrower service-host
smokes such as `smoke:admin-diagnostics-api` and `smoke:adapter-diagnostics`
when the goal is package-root import compatibility.
