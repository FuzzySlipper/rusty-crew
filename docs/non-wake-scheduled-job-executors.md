# Non-Wake Scheduled Job Executors

Status: Design contract for task 3128

Date: 2026-06-22

## Decision

Rusty Crew should not port pi-crew's `script_only` or `data_collection` cron
jobs as generic script runners.

The scheduler remains Rust-owned. Non-wake jobs may be added only as named,
typed job kinds with bounded payload schemas and explicit executor ownership.
The TypeScript service may host executors for jobs that need Den, channel, MCP,
filesystem, browser, or provider access, but those executors must operate on a
Rust-claimed scheduled run and report the terminal outcome back through typed
scheduler APIs.

The current `scheduledJobs[]` runtime config accepts only executable
`shape: "session_wake"`. Legacy pi-crew shapes may be parsed for migration
awareness, but they must fail closed until a named job kind exists.

## Why Not Generic Scripts

Generic scheduled scripts create the same problems Rusty Crew is trying to
avoid:

- executable strings in config become a second tool/executor system;
- secrets and environment assumptions become implicit;
- output shape drifts per script;
- retry or restart can duplicate side effects;
- observation and run history become afterthoughts;
- agent-authored scripts are easy to forget and duplicate.

For local development, operators can still run shell commands manually or build
small tools. Scheduled runtime work should be auditable and typed.

## Executor Boundary

Rust owns:

- due-time calculation and persisted next due time;
- run creation, claim, stale-claim expiry, and terminal run history;
- Rust-only job kinds such as cleanup, reconciliation, wake creation, and queue
  expiry;
- restart behavior and duplicate-run prevention.

TypeScript host owns:

- adapter-backed jobs that need Den Gateway, channel, MCP, browser, filesystem,
  profile, skill, or model-provider access;
- transforming bounded job payloads into adapter calls;
- compact observations and result references for operator visibility.

Both sides share one rule: only a claimed run may execute, and every executor
must complete the run with a bounded output object.

## Replacement For pi-crew Shapes

`script_only` should be replaced by one of:

- a named Rust job kind when the effect is runtime coordination;
- a named TS host job kind when the effect needs adapters or files;
- a normal model-callable/admin tool if it is an operator action rather than
  scheduled maintenance;
- an external service outside Rusty Crew if arbitrary shell orchestration is the
  real requirement.

`data_collection` should be replaced by named diagnostics/reporting jobs such
as:

- `runtime.diagnostics.snapshot`;
- `runtime.review.memory_skills`;
- `runtime.curator.scan`;
- future adapter-specific collectors, for example
  `adapter.den_channels.snapshot`, if the payload and output schemas are
  explicit.

## Required Job-Kind Contract

Before enabling a non-wake job kind, define:

- stable `job_kind` string;
- owner: Rust executor or TS host executor;
- payload schema version and maximum serialized size;
- maximum runtime and candidate count;
- retry/backoff/stale-run policy;
- dry-run behavior;
- terminal output shape;
- result reference policy;
- observation behavior;
- admin controls allowed for the job;
- tests proving restart does not duplicate side effects.

Payloads must not contain secrets, raw prompts, unbounded command strings, or
large tool outputs.

## Host Executor Contract

When TS host executors are needed, add a small scheduler-host-executor layer
rather than embedding execution in cron config loading.

The host executor loop should:

1. ask Rust for claimable host runs for supported job kinds;
2. execute at most one bounded unit per claimed run;
3. publish compact observation events when configured;
4. complete the run through a typed bridge method;
5. leave stale claims to Rust reconciliation after crashes.

This requires bridge support beyond the current query diagnostics: claim host
run, complete host run, fail host run, and optionally cancel claimed run.

## Implementation Tasks

Future work should be split this way:

- Define typed host-run claim and completion bridge APIs.
- Add a scheduler host executor registry keyed by `job_kind`.
- Implement `runtime.diagnostics.snapshot` as the first TS host executor.
- Implement `runtime.review.memory_skills` only after its output refs and
  curator handoff are stable.
- Keep `script_only` and `data_collection` rejected until a migration adapter
  can map each old job to a named Rusty Crew job kind.

## Compatibility

For pi-crew migration, old cron config should be converted explicitly:

- `session_wake` maps directly to Rusty Crew `scheduledJobs[]`.
- `script_only` must map to an approved named job kind or remain unsupported.
- `data_collection` must map to an approved diagnostics/reporting job kind or
  remain unsupported.

Unsupported shapes should produce clear validation errors that mention the
needed replacement path. Silent no-op migration is not acceptable.
