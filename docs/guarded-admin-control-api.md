# Guarded Admin Control API

Status: implementation note for task 2953

Rusty Crew now has a framework-neutral guarded admin control route layer in `@rusty-crew/brain-island`.

The route layer maps `POST /v1/admin/control/...` requests into explicit command objects and dispatches them through a configured `AdminControlExecutor`. It does not reach into storage, adapter maps, MCP clients, queues, or runtime internals directly.

Curator status, scan, candidate preview/approve/apply, and rollback commands now
use the same guarded surface. See `curator-admin-control-routes.md`.

Scheduler tick/job controls and delegated cleanup controls also use this
surface. See `background-service-diagnostics-admin-controls.md`.

## Boundary

`handleAdminControlRequest` requires:

- `POST`;
- a valid bearer token;
- an operator identity;
- a configured executor method for the requested control;
- an audit sink that can write a started audit event before side effects.

Unsupported control families fail closed with `failed_precondition` and do not write audit events or call executor methods. Missing or invalid auth fails before body parsing can cause side effects.

## Routes

Initial route families:

- `POST /v1/admin/control/sessions`
- `POST /v1/admin/control/sessions/{session_id}/archive`
- `POST /v1/admin/control/sessions/{session_id}/new`
- `POST /v1/admin/control/delegations/{session_id}/cancel`
- `POST /v1/admin/control/delegations/{session_id}/checkpoint`
- `POST /v1/admin/control/mcp/{session_id}/reload`
- `POST /v1/admin/control/maintenance`
- `POST /v1/admin/control/scheduler/tick`
- `POST /v1/admin/control/scheduler/jobs/{job_id}/run`
- `POST /v1/admin/control/scheduler/jobs/{job_id}/pause`
- `POST /v1/admin/control/scheduler/jobs/{job_id}/resume`
- `POST /v1/admin/control/cleanup/delegated/run`
- `POST /v1/admin/control/shutdown`

Each route becomes an `AdminControlCommand` with:

- command name;
- target runtime IDs;
- actor/operator identity;
- request ID;
- optional idempotency key;
- optional reason/reason code;
- optional Den refs;
- body object for executor-specific fields.

## Audit And Observation

The route writes a `started` audit event before calling the executor. If that write fails, the route fails closed and no executor method is called.

After the executor returns or throws, the route writes a terminal `completed` or `failed` audit event. Executor throws are converted into a failed control outcome with reason code `control_executor_failed`.

If an `AgentActivityObservationProducer` and identity are configured, the route also emits display-only `admin_command_started`, `admin_command_completed`, or `admin_command_failed` breadcrumbs. Observation writes are not runtime authority.

## Redaction

Control responses use the same conservative redaction posture as read-only admin diagnostics:

- secret-like keys such as `authorization`, `bearer`, `credential`, `password`, `secret`, `token`, and `api_key` become `[redacted]`;
- strings are bounded to 2048 characters.

Executors should still avoid returning raw prompts, full tool output, credentials, private adapter handles, or giant payloads.

## Current Executor Model

The route layer intentionally depends on an executor interface instead of implementing Rust calls itself. This lets each command family be wired only when the underlying Rust/bridge control API exists.

Configured executor methods can cover:

- create session;
- archive session;
- archive/create new session;
- cancel delegation;
- request delegated checkpoint;
- reload per-session MCP;
- run explicit maintenance;
- run scheduler tick/job controls;
- run delegated resource cleanup;
- shutdown.

Absent executor methods are explicit unsupported controls.

## Smoke Coverage

`npm run smoke:admin-control-api` verifies:

- missing auth fails closed;
- unsupported controls fail closed before audit/executor side effects;
- successful archive control emits started/completed audit and observation events;
- operator identity and idempotency key are preserved;
- control outcomes are redacted;
- delegated checkpoint requires a parent session ID;
- executor throws become failed control outcomes;
- unavailable audit sink blocks side effects;
- non-POST methods are rejected.
