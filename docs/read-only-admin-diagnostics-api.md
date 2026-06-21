# Read-Only Admin Diagnostics API

Status: implementation note for task 2952

Rusty Crew now has a read-only admin diagnostics route layer in `@rusty-crew/brain-island`.

The route layer is not an HTTP listener. It accepts a method/path request shape and returns the stable admin response envelope defined by `admin-http-host-auth-api-boundary`. A later TS HTTP host can mount it without copying diagnostics shaping logic or choosing a framework too early.

## Boundary

`handleAdminDiagnosticsRequest` only supports `GET` routes. It reads from:

- `RuntimeDiagnosticsProjection`;
- `RuntimeHealthProjection`;
- optional bounded recent event summaries.

It does not read storage directly, mutate runtime state, run maintenance, reload adapters, redeliver queues, or inspect private adapter objects. Mutating routes belong to guarded control endpoints.

## Routes

Implemented read-only route families:

- `GET /v1/admin/healthz`
- `GET /v1/admin/readyz`
- `GET /v1/admin/diagnostics`
- `GET /v1/admin/diagnostics/overview`
- `GET /v1/admin/diagnostics/sessions`
- `GET /v1/admin/diagnostics/agents`
- `GET /v1/admin/diagnostics/delegations`
- `GET /v1/admin/diagnostics/queues`
- `GET /v1/admin/diagnostics/tools`
- `GET /v1/admin/diagnostics/mcp`
- `GET /v1/admin/diagnostics/channels`
- `GET /v1/admin/diagnostics/persistence`
- `GET /v1/admin/diagnostics/observation`
- `GET /v1/admin/diagnostics/background`
- `GET /v1/admin/diagnostics/metrics`
- `GET /v1/admin/events/recent`

Unknown routes return a stable `not_found` error envelope. Non-GET methods return `method_not_allowed`.

## Pagination And Filters

List routes return:

```ts
interface AdminPage<T> {
  items: T[];
  total: number;
  limit: number;
  offset: number;
  nextOffset?: number;
}
```

Supported initial filters:

- sessions: `status`, `agent_id`, `profile_id`;
- tools: `catalog_id`, `invalid`;
- MCP surfaces: `status`;
- channel bindings: `status`.

`limit` and `offset` are bounded. Metrics allow a larger cap than ordinary list routes because metrics are compact numeric samples.

## Redaction

All success payloads pass through a conservative redactor:

- secret-like keys such as `authorization`, `bearer`, `credential`, `password`, `secret`, `token`, and `api_key` become `[redacted]`;
- strings are bounded to 2048 characters.

The redactor is a guardrail, not a license to put raw prompts, full tool outputs, credentials, or private adapter internals into diagnostics inputs.

## Smoke Coverage

`npm run smoke:admin-diagnostics-api` verifies:

- stable success and error envelopes;
- readiness and overview routes;
- sessions, agents, channels, tools, metrics, and recent events routes;
- background service diagnostics route;
- pagination and filters;
- method rejection and unknown route handling;
- secret-like field redaction.
