# API Capability And Slash Command Catalog Ratchet

Status: active generated-contract guidance

Rusty Crew exposes browser/admin discovery through `GET /v1/admin/capabilities`
and chat command discovery through `GET /v1/chat/commands`. TypeScript
declarations remain the source because handlers and browser projection live at
that boundary, but readbacks, route coverage, command metadata, and OpenAPI
operation metadata are no longer maintained as separate inventories.

The committed review artifacts are:

```text
fixtures/api-capabilities/api-command-capabilities.json
docs/rusty-crew-api-capabilities.openapi.json
```

Generate and check it with:

```bash
npm run codegen:api-capabilities
npm run check:api-capabilities
```

`verify:offline` runs the check. The JSON fixture contains the exact browser
registry projection, route-family coverage, explicit exemptions, and slash
command execution-handler inventory. The OpenAPI 3.1 artifact contains every
public capability operation and precise wire schemas for capability and command
discovery.

Operations marked `x-rusty-crew-contract-detail: capability` intentionally do
not claim an operation-specific request or response shape. Their detailed wire
contracts remain in their owning domain documents. The three discovery
operations are marked `wire` and are suitable for type generation.

## Validation Sources

`npm run smoke:api-command-registry` and the generated-artifact check validate
these boundaries:

- public API capabilities must match a service route table family;
- every service route family must have capability coverage or an explicit
  exemption in `api-capability-coverage.ts`;
- covered route families cannot retain stale exemptions;
- slash command names are inferred from `SLASH_COMMAND_REGISTRY`, and the
  execution-handler map must implement every inferred name exactly once;
- any `rust_plan_operation` declared by a slash command or API capability must
  exist in the generated `manifestOperationNames` list from
  `@rusty-crew/contracts`.
- generated OpenAPI method/path pairs must exactly match public capability
  declarations and operation ids must be unique.

The `/new` command and `admin.control.sessions.new` capability declare
`plan_new_session_control`, which is the Rust-owned planner for archive/create
lifecycle preconditions.

## Adding Routes

When adding a new route family to `SERVICE_API_ROUTE_TABLE`:

- add its public operation declarations to `API_CAPABILITIES` when the route
  should be discoverable by Rusty View or admin clients;
- add a short entry to `SERVICE_ROUTE_CATALOG_EXEMPTIONS` only when the whole
  route family should intentionally stay out of public discovery;
- regenerate the capability artifact.

This makes exemptions visible in CI instead of hiding them in stale docs.

## Rusty View Consumption

Rusty View should keep an explicit `fetch`/SSE transport and generate types with
`openapi-typescript`; it should not generate an HTTP client. The existing chat
OpenAPI references the generated command and capability discovery operations,
so one type-generation input remains sufficient:

```bash
pnpm exec openapi-typescript \
  /home/dev/rusty-crew/docs/rusty-view-chat-api-v0.openapi.json \
  -o libs/protocol/src/generated/openapi.ts
```

See `docs/rusty-view-openapi-consumption.md` for stable operation ids, drift
checks, and the live browser certification scenario.

## Adding Commands

When a slash command mutates runtime state, route it through an admin control
capability. If Rust owns the command plan, set `rustPlanOperation` on the slash
command control descriptor and on the matching API capability. The smoke will
fail if that operation is removed from the bridge manifest without updating the
catalog. Add command execution to the typed `SLASH_COMMAND_HANDLERS` map; its
`Record<SlashCommandName, ...>` contract prevents missing or orphan handlers.
