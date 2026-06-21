# Background Governance Observation Projection

Status: Implementation note for task 2979

Date: 2026-06-21

## Purpose

Background services now have a shared observation helper for compact
`agent_activity.v1` projection. The helper keeps scheduler, curator,
background-review, cleanup, and adapter-check breadcrumbs consistent while
preserving the rule that observation is display-only.

## Implemented Surface

`publishBackgroundGovernanceObservation` accepts:

- loop kind: `scheduler`, `curator`, `background_review`, `cleanup`, or
  `adapter_check`;
- phase: `started`, `completed`, `failed`, `degraded`, or `recovered`;
- operator/runtime identity;
- compact summary;
- optional work refs, result refs, reason code, and adapter name.

Non-adapter loops publish work activity:

- `started` -> `work_started`;
- `completed` -> `work_completed`;
- `failed` -> `work_failed`.

Adapter checks publish adapter activity:

- `degraded` -> `adapter_degraded`;
- `recovered` -> `adapter_recovered`.

## Existing Callers And Fit

The route/admin/control layer already emits admin-command audit and observation
events. Background review and delegated cleanup have direct observation hooks.
The governance helper is the shared projection shape for new scheduler,
curator, cleanup, and adapter background loop callers.

Observation payloads should carry result refs such as scheduler run id, curator
report doc slug, cleanup run id, or adapter diagnostic ref. They should not
include raw skill bodies, raw prompts, full tool output, credentials, or large
logs.

## Boundary

Observation is not runtime truth. A scheduler run, curator mutation record, or
cleanup report remains authoritative even if observation publishing is skipped
or degraded.
