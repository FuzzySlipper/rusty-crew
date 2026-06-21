# Curator Admin Control Routes

Status: Implementation note for task 2973

Date: 2026-06-21

## Purpose

Curator operations now route through the shared guarded admin control surface.
This keeps curator controls aligned with the same authorization, audit, and
observation path used by session, delegation, MCP, maintenance, and shutdown
commands.

The route layer still does not own curator truth. It parses an admin request,
authenticates the operator, writes admin-control audit events, publishes
display-only observation when configured, and dispatches to an injected
executor.

## Routes

All routes are `POST /v1/admin/control/...` and require the admin bearer token
plus operator identity configured by `handleAdminControlRequest`.

Implemented curator routes:

- `/v1/admin/control/curator/status`
- `/v1/admin/control/curator/run`
- `/v1/admin/control/curator/candidates/:candidateId/preview`
- `/v1/admin/control/curator/candidates/:candidateId/approve`
- `/v1/admin/control/curator/candidates/:candidateId/apply`
- `/v1/admin/control/curator/mutations/:mutationId/rollback`

`curator/run` accepts `scopeType`, `scopeId`, and `dryRun` in the request body.
Approve/apply routes use the normal admin-control `reason` body field.

## Executor Adapter

`createCuratorAdminControlExecutor` maps admin commands onto:

- a configured `CuratorExecuteRequest` executor for scan, preview, approve, and
  apply;
- an optional status provider;
- an optional rollback function.

Hosts can mount the returned methods into `AdminControlExecutor` alongside the
other runtime controls. If the host does not configure one of the executor
methods, the existing admin route fails closed with `unsupported_control`.

## Current Support

The current curator executor supports skill-scoped mutation candidates and
rollback scaffolding. The admin routes expose that support without adding new
mutation authority.

Not yet implemented:

- durable curator persistence;
- pause/resume state;
- archived candidate listing;
- direct restore, pin, and unpin controls;
- richer status from Rust-owned scheduler/governance storage.

Those should be added behind the same admin-control route layer once the owning
state exists.
