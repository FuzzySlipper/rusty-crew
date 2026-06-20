# Scheduler Cron Persistence Model

Status: Design contract for task 2967

Date: 2026-06-20

## Scope

Rusty Crew scheduler/cron persistence is the durable substrate for scheduled
runtime work. It should let the service claim due jobs, request Rust-owned
wakes or internal events, survive restart without duplicate runs, and expose
bounded diagnostics.

This design does not port pi-crew cron scripts directly. Script execution is
not the default first implementation.

## Ownership

Rust owns:

- job definitions that affect runtime coordination;
- schedule state and due-time indexes;
- run records, claims, retries, and terminal outcomes;
- stale-run reconciliation;
- manual run requests;
- wake/internal-event emission from claimed scheduler work.

Host/service config owns:

- which built-in job kinds are installed for a deployment;
- adapter credentials and network endpoints;
- optional TS host executors for platform-specific jobs;
- default enablement choices.

TypeScript host executors may execute a claimed job step when the job needs
Den, channel, MCP, filesystem, or LLM logic, but the executor receives a typed
claim and reports an outcome to Rust.

## Job Definition Table

Proposed table: `scheduled_jobs`

Fields:

- `job_id TEXT PRIMARY KEY`
- `job_kind TEXT NOT NULL`
- `display_name TEXT`
- `scope_type TEXT NOT NULL`
- `scope_id TEXT NOT NULL`
- `schedule_kind TEXT NOT NULL`
- `schedule_expr TEXT`
- `interval_seconds INTEGER`
- `enabled INTEGER NOT NULL`
- `paused_reason TEXT`
- `next_due_at TEXT`
- `jitter_seconds INTEGER`
- `max_runtime_seconds INTEGER NOT NULL`
- `max_attempts INTEGER NOT NULL`
- `backoff_seconds INTEGER NOT NULL`
- `payload_json TEXT NOT NULL`
- `last_run_id TEXT`
- `last_outcome TEXT`
- `last_error TEXT`
- `created_at TEXT NOT NULL`
- `updated_at TEXT NOT NULL`

Indexes:

- `(enabled, next_due_at)`
- `(scope_type, scope_id)`
- `(job_kind, enabled)`

`payload_json` must stay bounded and schema-versioned by `job_kind`; it should
not contain secrets or large prompts.

## Run Record Table

Proposed table: `scheduled_job_runs`

Fields:

- `run_id TEXT PRIMARY KEY`
- `job_id TEXT NOT NULL`
- `job_kind TEXT NOT NULL`
- `scope_type TEXT NOT NULL`
- `scope_id TEXT NOT NULL`
- `trigger_type TEXT NOT NULL`
- `trigger_ref TEXT`
- `attempt INTEGER NOT NULL`
- `state TEXT NOT NULL`
- `claim_token TEXT`
- `claimed_by TEXT`
- `claimed_at TEXT`
- `started_at TEXT`
- `deadline_at TEXT`
- `finished_at TEXT`
- `outcome TEXT`
- `error_kind TEXT`
- `error_message TEXT`
- `payload_json TEXT NOT NULL`
- `result_ref_json TEXT`
- `created_at TEXT NOT NULL`
- `updated_at TEXT NOT NULL`

Indexes:

- `(job_id, created_at)`
- `(state, deadline_at)`
- `(scope_type, scope_id, created_at)`
- `(trigger_type, created_at)`

Run states:

- `queued`
- `claimed`
- `running`
- `completed`
- `skipped`
- `failed`
- `expired`
- `cancelled`

Terminal states are `completed`, `skipped`, `failed`, `expired`, and
`cancelled`.

## Manual Run Requests

Manual runs should be represented as `scheduled_job_runs` with
`trigger_type = manual`. The request should include:

- actor identity;
- reason;
- optional run-after timestamp;
- optional bounded override payload;
- optional dry-run flag.

Manual runs still obey job enablement, scope, max runtime, and claim semantics
unless an admin control explicitly overrides them with audit.

## Claim Protocol

The scheduler loop should:

1. Query due enabled jobs by `next_due_at <= now`.
2. Create one queued run per due job using an idempotent key or transaction.
3. Advance `next_due_at` before or inside the same transaction that creates the
   run.
4. Claim queued runs with a `claim_token`, `claimed_by`, `claimed_at`, and
   `deadline_at`.
5. Execute exactly one bounded step.
6. Complete the run with terminal outcome and result refs.

Only claimed runs may execute. If a host crashes after claim, stale-run
reconciliation marks the run `expired` or returns it to `queued` according to
job policy. The first implementation should prefer `expired` plus next regular
tick over silent retry to avoid duplicate side effects.

## Wake And Event Emission

Scheduled work may request runtime activation only through Rust coordination:

- publish an internal event;
- create a durable wake request/ticket;
- route a typed agent message;
- call a typed cleanup/reconciliation operation.

The scheduler must not invoke a TS brain directly because a job is due. It
should hand a wake request to the same Rust-owned wake scheduler path used by
runtime events.

## Built-In Job Kinds

Initial job kinds should be narrow:

- `runtime.cleanup.delegations`
- `runtime.cleanup.queued_messages`
- `runtime.cleanup.scheduler_runs`
- `runtime.review.memory_skills`
- `runtime.curator.scan`
- `runtime.diagnostics.snapshot`

Each job kind needs:

- payload schema;
- max candidates per pass;
- max runtime;
- retry/backoff policy;
- result ref shape;
- observation/audit behavior;
- whether TS host execution is required.

## Restart Semantics

On startup:

- do not rerun `running` or `claimed` work blindly;
- mark claims past `deadline_at` as `expired` with reason
  `scheduler_claim_stale`;
- leave future `queued` work queued;
- never duplicate a run for the same due tick;
- do not emit fake completion packets;
- run projection/diagnostic repair only through explicit maintenance tooling.

Opening an engine against an already valid store must not increase job/run
counts. Scheduler catch-up should be explicit and bounded by job policy.

## Diagnostics

Scheduler diagnostics should use typed query APIs rather than raw SQL and
report:

- due job count;
- enabled/paused job counts;
- active/expired/failed run counts;
- oldest queued run age;
- stale claim count;
- last outcome by job kind;
- degraded host executors.

Observation events are display-only and should follow `agent_activity.v1`.

## Migration Plan

Implementation should add a new schema migration with:

1. `scheduled_jobs`
2. `scheduled_job_runs`
3. typed query structs and bounded paging
4. claim/complete/reconcile APIs on `CoordinationStore`
5. CoreEngine pass-through APIs only where runtime coordination needs them
6. native bridge methods only after TS host executors need them

SQLite remains acceptable for the first implementation because the scheduler
surface is isolated behind the persistence boundary. Future PostgreSQL support
should preserve the typed API shape.
