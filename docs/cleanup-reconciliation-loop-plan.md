# Cleanup Reconciliation Loop Plan

Status: Design contract for task 2976

Date: 2026-06-20

## Scope

Cleanup and reconciliation loops repair or surface stale runtime state after
normal operation, adapter failures, or service restart. They must be
idempotent, bounded, auditable, and safe to retry.

Cleanup is not a hidden completion system. It may archive, expire, cancel, or
degrade state through typed owner APIs, but it must not fabricate brain
outputs, replay old work, or treat external projection state as runtime truth.

## State Owner Split

Rust-owned cleanup:

- delegated sessions and worker-run terminal reconciliation;
- queued-message expiry and purge;
- scheduler stale claims and terminal run retention;
- runtime buffer leak detection/release policy;
- runtime counters repair/reset scheduling;
- session archival/drain effects.

TypeScript adapter cleanup:

- MCP surface disconnect/archive/reload;
- channel subscription/cursor degradation;
- browser subprocess/resource cleanup when browser support lands;
- Den/observation sink degradation handling;
- platform-specific readback and report formatting.

Den/external cleanup:

- Den task/message/document retention;
- provider channel history;
- external MCP server state.

Rusty Crew can report or project external cleanup needs, but should not assume
ownership of external retention.

## Loop Envelope

Each cleanup loop should:

1. Claim a scheduler run or explicit admin command.
2. Read bounded candidates from the owning state domain.
3. Apply only idempotent transitions.
4. Write an outcome with counts and evidence refs.
5. Emit optional display-only `agent_activity.v1` observation.
6. Leave enough diagnostics for readback.

Every loop needs:

- `max_candidates`;
- `max_runtime`;
- dry-run support for admin/manual execution;
- stable reason codes;
- safe error output;
- changed/skipped/failed counts.

## Runtime Cleanup Domains

### Delegated Sessions

Owner: Rust.

Candidate states:

- delegated session has missing parent;
- delegated session parent is archived;
- delegated session exceeds max duration;
- worker-run record is nonterminal while session is archived;
- worker-run terminal state conflicts with session state.

Allowed transitions:

- archive child delegated session;
- mark worker run `cancelled` or `expired`;
- leave terminal worker-run outcomes untouched;
- emit cleanup audit/observation.

Forbidden transitions:

- create completion packets;
- route new checkpoint messages;
- replay original delegation prompt;
- overwrite completed/failed/blocked/exhausted outcomes.

### Queued Messages

Owner: Rust persistence.

Candidate states:

- pending queue item past `expires_at`;
- terminal item older than retention cutoff.

Allowed transitions:

- mark expired pending item `expired`;
- purge old terminal rows only by explicit retention policy;
- increment queue-expiration counters;
- expose expired rows through query/readback tooling.

Forbidden transitions:

- move expired rows back to pending;
- redeliver terminal rows;
- hydrate expired rows into `BodyState`.

### Scheduler Runs

Owner: Rust scheduler persistence.

Candidate states:

- claimed/running run past `deadline_at`;
- queued run older than job-specific TTL;
- terminal run older than retention cutoff.

Allowed transitions:

- mark stale claimed/running run `expired`;
- leave future queued runs queued;
- purge old terminal runs only by explicit retention policy.

Forbidden transitions:

- silently requeue side-effecting work after a stale claim;
- double-run the same due tick;
- execute a job without a valid claim token.

### Runtime Buffers

Owner: bridge/runtime buffer store.

Candidate states:

- buffer handle leased past wake deadline;
- bridge shutdown with outstanding leases.

Allowed transitions:

- release or mark leaked during shutdown/cleanup;
- emit diagnostic/audit result;
- fail readiness if leaks indicate unsafe runtime state.

Forbidden transitions:

- reuse a leaked handle for a different payload;
- hide repeated leaks from diagnostics.

## Adapter Cleanup Domains

### MCP Surfaces

Owner: TypeScript MCP adapter.

Candidate states:

- archived binding with active client;
- disconnected/degraded client beyond retry budget;
- stale discovered tool surface after reload;
- per-session surface whose session is archived.

Allowed transitions:

- disconnect/archive client;
- mark surface degraded;
- reload from current binding;
- emit adapter diagnostics and observation.

Forbidden transitions:

- mutate another agent/session surface;
- keep stale tools selected after degraded reload;
- treat MCP reload as runtime work completion.

### Channel Subscriptions

Owner: TypeScript channel adapter.

Candidate states:

- subscription disconnected or stale;
- cursor recovery ambiguous;
- binding archived;
- inbound item expired before routing.

Allowed transitions:

- reconnect from committed cursor after idempotency checks;
- mark binding/subscription degraded;
- archive subscription for archived binding;
- store terminal inspection state for expired inbound items.

Forbidden transitions:

- replay messages older than replay window;
- redeliver expired inbound messages;
- create/archive Rust sessions solely from presence changes.

### Browser Resources

Owner: future TypeScript/browser adapter.

Candidate states:

- orphan subprocess;
- stale session/browser handle;
- resource over deadline.

Allowed transitions:

- terminate subprocess;
- mark adapter degraded if termination fails;
- emit bounded diagnostics.

Forbidden transitions:

- kill arbitrary unrelated processes;
- hide resource cleanup failure from diagnostics.

## Completion And Evidence Reconciliation

Checks:

- delegated run completed but missing completion packet;
- completion packet exists for unknown session;
- background run outcome references missing evidence;
- observation publish failed after a completed run.

Allowed responses:

- report mismatch as diagnostic finding;
- link existing evidence refs;
- schedule curator/review follow-up when applicable.

Forbidden responses:

- fabricate missing completion packets;
- delete completed runs to hide mismatch;
- mutate Den evidence as if it were runtime state.

## Observation And Audit

Cleanup loops should emit observation only after authoritative state transition
or dry-run report is known.

Recommended event types:

- `work_started`;
- `work_checkpoint`;
- `work_completed`;
- `work_failed`;
- `adapter_degraded`;
- `adapter_recovered`.

Observation failure is a diagnostics issue. It does not roll back cleanup.

## First Implementation Order

1. Add scheduler job/run persistence and stale-run reconciliation.
2. Expose queued-message expiry/purge through scheduled job executor.
3. Promote existing delegated orphan cleanup into a scheduled/admin callable
   operation. The first implementation is documented in
   `delegated-resource-cleanup-loop.md`.
4. Add adapter cleanup reports for MCP and channel surfaces.
5. Add cleanup diagnostics projection.
6. Prove restart-safe cleanup e2e.

## Open Implementation Notes

- Expired-message pull tooling should query terminal queue rows and never
  redeliver them.
- Adapter cleanup should remain TS-owned until adapter lifecycle state has a
  Rust persistence API that truly owns it.
- Browser cleanup waits until browser resource tracking exists.
