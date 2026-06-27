# PostgreSQL Queue And Scheduler Claim Semantics

Status: design gate for task 3473

Date: 2026-06-26

## Purpose

This document defines the correctness requirements for porting queued messages
and scheduler/job state to PostgreSQL.

PostgreSQL support makes multi-writer and multi-worker execution possible. That
is useful, but it also makes the dangerous cases easier to trigger:

- old queued messages can be delivered after their context is stale;
- terminal queue rows can be imported or retried as pending work;
- two workers can claim the same scheduled run;
- a stale worker can complete a run after another reconciler already expired it.

The current PostgreSQL proof slices intentionally do not implement these
repositories. `queues_messages` and `scheduler_jobs` must remain unsupported for
PostgreSQL service boot until the tests named here pass against a real
PostgreSQL backend.

## Queue Semantics

Queued messages are recovery state, not a durable work backlog. They preserve
body-owned next-wake messages across bounded restart windows. They must not
become a general replay queue.

### States

Queue state remains:

- `pending`
- `delivered`
- `expired`
- `discarded`
- `cancelled`

Terminal states are `delivered`, `expired`, `discarded`, and `cancelled`.

Terminal state is monotonic. Once a `message_id` is terminal, no normal write,
import, retry, or maintenance path may move that same `message_id` back to
`pending`. Requeueing requires a new `message_id` with fresh provenance.

### TTL-First Reads

Every path that exposes pending queue rows for wake hydration or delivery must
expire stale rows first in the same transaction:

```sql
UPDATE queued_messages
SET state = 'expired',
    terminal_at = $1,
    state_reason = COALESCE(state_reason, 'ttl_expired')
WHERE state = 'pending'
  AND expires_at <= $1
RETURNING *;
```

After that update, eligible reads may only select:

```sql
WHERE state = 'pending'
  AND expires_at > $now
```

The existing Rust body drain marks selected rows `delivered` immediately at a
wake boundary. PostgreSQL should preserve that simple model until there is a
proven need for a separate queue claim lease.

If a future active queue claim state is added, it must include a claim token,
claim owner, claim timestamp, claim expiry, and stale-claim reconciliation. Do
not simulate claims with a plain pending read followed by a later update.

### Concurrent Drain

For multi-worker PostgreSQL delivery, drain must be a single transaction:

1. Expire stale pending rows.
2. Select fresh pending rows with row locks:

   ```sql
   SELECT message_id
   FROM queued_messages
   WHERE state = 'pending'
     AND expires_at > $now
     AND owner_session_id = $session_id
   ORDER BY enqueued_at ASC, message_id ASC
   FOR UPDATE SKIP LOCKED
   LIMIT $limit;
   ```

3. Mark those rows `delivered`, with `terminal_at = $now` and a delivery reason,
   before returning them to the caller.

Row locks are the primary queue concurrency primitive. Advisory locks may be
used for coarse maintenance windows, but they must not replace row-level state
fencing for delivery.

### Purge And Search

Purge is a terminal-row cleanup operation only. It may delete terminal queue
rows whose `terminal_at` is older than the cutoff and must delete matching
runtime-search rows in the same transaction.

Purge must never:

- delete `pending` rows;
- change terminal state;
- recreate search rows;
- create a new pending row as a side effect.

Search visibility is not delivery eligibility. Expired or discarded queue rows
may remain visible to diagnostics or future expired-message pull tools without
being deliverable.

### Import And Export

Logical export records for queue rows must include:

- `message_id`
- owner session and owner agent
- full `AgentMessage`
- `source_sequence`
- `enqueued_at`
- `expires_at`
- `ttl_ms`
- `delivery_attempts`
- `state`
- `terminal_at`
- `state_reason`

Logical import must preserve terminal state. It must not import terminal rows as
`pending`.

If an imported row is `pending` but `expires_at <= import_now`, import it as
`expired` with `terminal_at = import_now` and `state_reason =
'import_expired'`. This preserves inspectability while preventing resurrection.

Upsert/import conflict behavior must be terminal-monotonic:

- terminal existing row plus pending incoming row stays terminal;
- pending existing row plus terminal incoming row becomes terminal;
- terminal existing row plus terminal incoming row may update diagnostics only
  if it does not erase terminal evidence;
- duplicate pending rows are rejected or idempotently preserved only when all
  delivery-relevant fields match.

## Scheduler Semantics

Scheduler/job state remains Rust-owned coordination state. PostgreSQL may allow
multiple claimers, but only one worker may own a run at a time.

The current Rust model has no separate `queued` or `running` run state. A due or
manual run is created directly as `claimed`. Keep that vocabulary unless a
future task explicitly expands the run state machine.

### States

Run state remains:

- `claimed`
- `completed`
- `skipped`
- `failed`
- `expired`
- `cancelled`

Terminal states are `completed`, `skipped`, `failed`, `expired`, and
`cancelled`.

### Due Run Creation

Creating a due run and advancing `scheduled_jobs.next_due_at` must be atomic.
The operation must be idempotent for the same job and scheduled tick.

PostgreSQL should enforce this with a unique key equivalent to:

```text
(job_id, trigger_kind, scheduled_for)
```

for due runs where `scheduled_for` is not null. If the final schema needs a
different idempotency key, document it before implementation.

For high-concurrency due-run materialization, a transaction-scoped advisory lock
may be used per `job_id`:

```sql
SELECT pg_advisory_xact_lock(hashtext($job_id));
```

This lock is optional helper fencing for materialization. It does not replace
run-row claim fencing.

### Claim Fencing

PostgreSQL scheduler claim creation must be one transaction:

1. Lock the active due job row or manual request row.
2. Insert the `scheduled_job_runs` row with status `claimed`, `claimed_at`,
   `claim_deadline_at`, and a claim token or equivalent fencing value.
3. Advance `scheduled_jobs.next_due_at` for due runs.
4. Return the claimed run.

If the implementation changes to a queued-run model, queued run claiming must
use row locks:

```sql
WITH candidate AS (
    SELECT run_id
    FROM scheduled_job_runs
    WHERE status = 'queued'
      AND scheduled_for <= $now
    ORDER BY scheduled_for ASC, run_id ASC
    FOR UPDATE SKIP LOCKED
    LIMIT 1
)
UPDATE scheduled_job_runs AS run
SET status = 'claimed',
    claim_token = $token,
    claimed_by = $worker_id,
    claimed_at = $now,
    claim_deadline_at = $deadline,
    updated_at = $now
FROM candidate
WHERE run.run_id = candidate.run_id
RETURNING run.*;
```

The current schema does not store `claim_token` or `claimed_by`. PostgreSQL
multi-worker scheduler work must either add those fields or document an
equivalent fencing mechanism before enabling concurrent claims.

### Completion Fencing

Completing a run must only succeed for the current claim holder.

With claim tokens, completion should look like:

```sql
UPDATE scheduled_job_runs
SET status = $terminal_status,
    completed_at = $now,
    updated_at = $now,
    output_json = $output_json,
    error = $error
WHERE run_id = $run_id
  AND status = 'claimed'
  AND claim_token = $claim_token
RETURNING *;
```

If this update returns no rows, the worker must treat completion as rejected and
emit diagnostics. It must not insert a replacement completion row or silently
overwrite terminal state.

### Stale Claim Reconciliation

Startup and scheduled reconciliation must scan `claimed` rows whose
`claim_deadline_at` is before the reconciliation timestamp.

The first PostgreSQL implementation should preserve the existing conservative
policy: stale claimed runs become `expired` with a reason equivalent to
`claim deadline elapsed`. They should not be silently returned to a runnable
state, because duplicate side effects are worse than a missed tick.

Retry/requeue policy may be added later per job kind, but it must be explicit in
the job policy and covered by tests before enabling.

## Diagnostics And Capabilities

The PostgreSQL diagnostics surface must keep these groups unsupported until the
required tests pass:

- `queues_messages`
- `scheduler_jobs`

The `row_level_claims` capability must stay `supported: false` until both queue
and scheduler claim tests pass against PostgreSQL.

When implemented, diagnostics should expose:

- `row_level_claims`: safe `FOR UPDATE SKIP LOCKED` or equivalent row fencing;
- `claim_token_fencing`: stale workers cannot complete current or terminal runs;
- `queue_no_resurrection_guard`: terminal queue rows cannot become pending;
- `queue_ttl_first_reads`: pending reads expire stale rows first;
- `scheduler_stale_claim_reconcile`: stale claims fail closed to terminal state;
- `advisory_locks`: available only as an optional materialization/maintenance
  helper;
- `logical_export_import`: queue import/export preserves terminal state and
  expiry.

`RUSTY_CREW_POSTGRES_BOOT_MODE=proof_admin` remains an admin/proof mode only. It
must not accept runtime workloads that require queue or scheduler repositories.

## Required Tests Before Implementation Is Accepted

Queue tests:

- `postgres_queue_expire_before_pending_read_no_resurrection`
- `postgres_queue_terminal_state_is_monotonic`
- `postgres_queue_concurrent_drain_skip_locked_delivers_once`
- `postgres_queue_purge_terminal_preserves_nonterminal_and_search_consistency`
- `postgres_queue_import_preserves_terminal_and_expires_stale_pending`
- `postgres_queue_restart_hydration_expires_before_wake_projection`
- shared conformance: `queued_message_ttl_no_resurrection_contract`

Scheduler tests:

- `postgres_scheduler_due_run_materialization_is_idempotent`
- `postgres_scheduler_concurrent_due_claim_creates_one_run_per_tick`
- `postgres_scheduler_completion_requires_current_claim_fence`
- `postgres_scheduler_stale_claim_reconcile_expires_not_requeues`
- `postgres_scheduler_terminal_runs_never_requeued`
- `postgres_scheduler_advisory_lock_does_not_replace_row_fencing`
- shared conformance: `scheduler_claim_and_expiry_contract`

Diagnostics and boot tests:

- `postgres_storage_diagnostics_marks_queue_scheduler_unsupported_until_claim_tests_pass`
- `postgres_storage_diagnostics_marks_row_level_claims_supported_only_after_queue_scheduler_conformance`
- `postgres_service_boot_postgres_fails_closed_when_queue_scheduler_unsupported`

The concurrent tests must run against a real PostgreSQL backend, not only an
in-process mock or SQLite compatibility path.

## Implementation Gates

Do not port queue or scheduler repositories as a casual follow-on to the
existing PostgreSQL proof slices.

Before enabling either group:

1. Add explicit PostgreSQL schema for the group.
2. Add repository methods with transactions around every claim/drain/complete
   operation.
3. Add shared conformance tests that cover SQLite and PostgreSQL where behavior
   is backend-neutral.
4. Add PostgreSQL-only concurrent claim tests.
5. Keep service boot fail-closed until diagnostics report the required group as
   implemented.
6. Keep logical import/export separate from raw SQLite-to-PostgreSQL migration.

Queue and scheduler implementations may land independently, but
`row_level_claims` must not become globally supported until both have the
required coverage.
