# Background Service Diagnostics And Admin Controls

Status: Implementation note for task 2982

Date: 2026-06-21

## Purpose

Rusty Crew now exposes background-service state through the read-only admin
diagnostics surface and exposes scheduler/cleanup controls through the guarded
admin control route layer.

This makes scheduler, curator, review, and cleanup state inspectable without
direct process or database access. Mutating actions still dispatch through
configured executors and fail closed when the host has not wired the owner API.

## Diagnostics

`buildBackgroundServiceDiagnosticsProjection` summarizes:

- scheduler jobs, paused jobs, stale runs, recent errors, next due time;
- curator availability, candidates, mutations, recent errors;
- background-review enablement and recent findings;
- cleanup loop archive counts, adapter release/degradation counts, recent
  errors.

`handleAdminDiagnosticsRequest` exposes the projection at:

`GET /v1/admin/diagnostics/background`

The projection reports a health level, compact summary counts, details for each
background domain, and issue records. It is read-only and redacted by the normal
admin diagnostics envelope.

## Controls

The guarded admin control parser now recognizes:

- `POST /v1/admin/control/scheduler/tick`
- `POST /v1/admin/control/scheduler/jobs/:jobId/run`
- `POST /v1/admin/control/scheduler/jobs/:jobId/pause`
- `POST /v1/admin/control/scheduler/jobs/:jobId/resume`
- `POST /v1/admin/control/cleanup/delegated/run`

Curator controls remain documented in `curator-admin-control-routes.md`.

`createBackgroundAdminControlExecutor` maps those routes onto host-provided
scheduler and delegated-cleanup functions. The route layer remains
framework-neutral and executor-injected, so unsupported controls return
`unsupported_control` before audit/executor side effects.

## Boundary

Diagnostics are display/readback state, not coordination truth. Scheduler run
records, curator records, review results, and cleanup reports remain the
authoritative state.

Controls must not edit tables directly. Hosts should wire them to typed Rust or
adapter APIs such as scheduler tick/job methods and
`runDelegatedResourceCleanup`.
