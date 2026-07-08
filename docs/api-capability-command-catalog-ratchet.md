# API Capability And Slash Command Catalog Ratchet

Status: implementation note for task 4711

Rusty Crew exposes browser/admin discovery through `GET /v1/admin/capabilities`
and chat command discovery through `GET /v1/chat/commands`. Those catalogs are
still TypeScript presentation data, but they are no longer allowed to drift
silently from the service route table or Rust bridge manifest.

## Validation Sources

`npm run smoke:api-command-registry` validates three boundaries:

- public API capabilities must match a service route table family;
- every service route family must have a representative capability or an
  explicit catalog exemption in the smoke;
- any `rust_plan_operation` declared by a slash command or API capability must
  exist in the generated `manifestOperationNames` list from
  `@rusty-crew/contracts`.

The `/new` command and `admin.control.sessions.new` capability declare
`plan_new_session_control`, which is the Rust-owned planner for archive/create
lifecycle preconditions.

## Adding Routes

When adding a new route family to `SERVICE_API_ROUTE_TABLE`, also update the
catalog smoke:

- add a representative capability path to `serviceRouteFamilyCoverage()` when
  the route should be discoverable by Rusty View or admin clients;
- add a short explicit entry to `serviceRouteCatalogExemptions()` only when the
  route should intentionally stay out of public discovery.

This makes exemptions visible in CI instead of hiding them in stale docs.

## Adding Commands

When a slash command mutates runtime state, route it through an admin control
capability. If Rust owns the command plan, set `rustPlanOperation` on the slash
command control descriptor and on the matching API capability. The smoke will
fail if that operation is removed from the bridge manifest without updating the
catalog.
