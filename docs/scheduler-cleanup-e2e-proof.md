# Scheduler And Cleanup E2E Proof

Task: `2983`

## Purpose

`smoke:scheduler-cleanup-e2e` proves that the Rust-owned scheduler and delegated cleanup loop can be driven through the real native bridge and projected through the TypeScript brain-island diagnostics/observation surfaces.

The proof intentionally avoids a mock scheduler. TypeScript may request operator/admin actions, but durable coordination stays in Rust.

## Covered Behavior

- A scheduled wake job is registered for an existing wakeable session through `register_scheduled_wake_job`.
- A real scheduler tick claims the due job, requests one `brain_wake_requested` event, completes the run, and advances the job so an immediate second tick does not double-run.
- A delegated session is created through normal brain actions, completed, then archived by `cleanup_delegated_resources`.
- Background governance observations publish scheduler and cleanup completion evidence through the agent activity observation contract.
- Background diagnostics summarize scheduler job counts, stale runs, curator availability, and cleanup archive counts.
- Reinitializing the engine against the same data directory and clock does not resurrect the completed scheduled run or re-archive the completed delegated session.

## Bridge Surface

The scheduler remains Rust-owned. The bridge exposes only compact operator controls:

- `register_scheduled_wake_job`
- `run_scheduler_tick`
- `request_scheduled_job_run`
- `pause_scheduled_job`
- `resume_scheduled_job`

These return `ScheduledJobSummary`, `ScheduledRunSummary`, or `SchedulerTickReport` DTOs instead of persistence records.

## Verification

Run:

```bash
npm run build:native
npm run smoke:scheduler-cleanup-e2e
```

The smoke expects:

- `dueRunsClaimed === 1`
- `wakesRequested === 1`
- `runsCompleted === 1`
- one `scheduled_job_runs` row
- one terminal delegated session archived by cleanup
- post-restart `dueRunsClaimed === 0`
- post-restart cleanup archive lists are empty
