# Scheduled Job Shapes And Host Executors

Status: Design contract for task 2969

Date: 2026-06-20

## Scope

This note defines the first Rusty Crew scheduled job catalog. Jobs are narrow,
typed, auditable, and safe for admin/cron controls. They build on the
Rust-owned scheduler persistence model and the background services governance
loop architecture.

External script execution is not part of the v1 scheduled job catalog.

## Common Job Payload Envelope

Every job payload should include:

- `schema_version`
- `max_candidates`
- optional `dry_run`
- optional `reason`
- optional `result_visibility`
- optional `observation_required`

Payloads must be bounded, JSON-serializable, and free of secrets, raw prompts,
large tool outputs, or executable command strings.

## Common Run Output

Every executor reports:

- `outcome`: completed, skipped, failed, expired, cancelled, or blocked;
- `summary`;
- `candidate_count`;
- `changed_count`;
- `result_refs`;
- `safe_error`, when failed or blocked;
- optional `next_recommendation`.

`result_refs` should use the evidence/result-reference policy rather than
embedding bulky evidence.

## Executor Placement

Rust executor:

- pure runtime coordination;
- cleanup/reconciliation over Rust-owned persistence;
- scheduler stale-run reconciliation;
- queued-message expiry;
- wake request creation.

TypeScript host executor:

- Den/channel/MCP/browser/filesystem adapter access;
- skill/profile file inspection;
- report formatting;
- LLM-backed review through normal brain/provider paths.

Both executor types operate on claimed scheduler runs. Neither type invents job
truth outside the Rust scheduler run record.

## Built-In Jobs

### `runtime.cleanup.delegations`

Purpose: reconcile delegated sessions and worker-run records.

Executor: Rust.

Scope: runtime, agent, session, or profile.

Payload:

- `max_candidates`
- `include_orphans`
- `include_expired`
- `max_duration_ms`
- `dry_run`

Effects:

- archive orphan/expired delegated sessions through typed Rust operations;
- mark nonterminal worker runs cancelled or expired;
- never create completion packets.

Result refs:

- session ids;
- run ids;
- cleanup audit ids when available.

### `runtime.cleanup.queued_messages`

Purpose: expire and optionally purge queued-message recovery state.

Executor: Rust.

Payload:

- `expire_before`
- `purge_terminal_before`
- `max_candidates`
- `dry_run`

Effects:

- mark expired messages terminal;
- optionally purge old terminal rows by explicit retention policy;
- never move expired rows back to pending.

Result refs:

- queue message ids;
- aggregate count refs.

### `runtime.cleanup.scheduler_runs`

Purpose: reconcile stale scheduler claims and old run records.

Executor: Rust.

Payload:

- `expire_claims_before`
- `purge_terminal_before`
- `max_candidates`
- `dry_run`

Effects:

- mark stale claimed/running runs expired;
- optionally purge old terminal runs;
- do not requeue side-effecting work by default.

Result refs:

- scheduled run ids.

### `runtime.diagnostics.snapshot`

Purpose: produce a bounded diagnostics/health snapshot for operators.

Executor: TypeScript host, optionally Rust-only once native diagnostics are
complete.

Payload:

- `include_sessions`
- `include_adapters`
- `include_tools`
- `include_persistence`
- `max_items`

Effects:

- no runtime mutation;
- may emit an observation event with a compact result ref.

Result refs:

- diagnostics bundle id;
- observation event id.

### `runtime.review.memory_skills`

Purpose: run static/bounded review over memory, skill, and planning surfaces.

Executor: TypeScript host.

Payload:

- `profile_id`
- `skills_root`
- `include_dense_profile_memory`
- `include_den_memory_diagnostics`
- `max_candidates`
- `llm_review_enabled`
- `dry_run`

Effects:

- produces findings and proposed actions;
- may create curator candidates;
- must not mutate memory or skill files directly.

Result refs:

- finding ids;
- curator candidate batch id;
- diagnostics report id.

### `runtime.curator.scan`

Purpose: discover curator candidates and produce a report.

Executor: TypeScript host.

Payload:

- `scope_type`
- `scope_id`
- `skills_root`
- `profile_root`
- `candidate_kinds`
- `max_candidates`
- `dry_run`

Effects:

- create candidate batch records after curator persistence exists;
- produce report output;
- no mutation without later approval.

Result refs:

- curator candidate batch id;
- report document/ref id;
- observation event id.

### `runtime.wake.profile`

Purpose: request a normal Rust-owned wake for a profile/session/agent with a
bounded reason.

Executor: Rust to request wake; TypeScript only participates later through the
normal registered brain path.

Payload:

- `target_type`: session, agent, or profile;
- `target_id`;
- `reason`;
- `correlation_id`;
- optional bounded `message_body`;
- `max_wakes`.

Effects:

- publish or persist a wake request through Rust scheduler authority;
- optionally route a typed bounded agent message;
- never invoke a TS brain directly.

Result refs:

- wake ids;
- routed message ids or event sequences.

## Excluded From V1

The first scheduler should not include:

- arbitrary shell/script execution;
- unbounded web/browser scraping;
- raw SQL jobs;
- direct Den task mutation jobs;
- direct LLM review jobs that do not use normal brain/provider paths;
- hidden worker-pool assignment loops.

If external script execution is later needed, it requires a separate design
with sandboxing, environment allowlists, resource limits, audit, and explicit
operator approval.

## Admin Control Compatibility

Admin/cron controls should support:

- enable/disable job;
- pause/resume with reason;
- run now with reason;
- dry-run now;
- show last outcome;
- show next due time;
- show active claim/deadline;
- cancel queued/claimed run.

Controls call typed scheduler APIs. They do not edit tables directly.

## Observation

Each job may emit compact `agent_activity.v1` events:

- `background_job_started`
- `background_job_completed`
- `background_job_failed`
- `background_job_skipped`

Observation is display-only. The scheduler run record remains the authoritative
job outcome.
