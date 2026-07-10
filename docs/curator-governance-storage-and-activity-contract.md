# Curator Governance Storage And Activity Contract

Status: accepted implementation design for tasks #5391-#5407  
Date: 2026-07-10

## Decision

Curator governance is a Rust-owned storage module with typed repositories for
candidates, approvals, mutations, snapshots, and append-only audit receipts.
SQLite and PostgreSQL implement the same repository contract. The existing
`curator_governance/default/snapshot` `simple_kv` record is neither read nor
migrated; a service upgraded to this contract starts with an empty curator
governance store.

TypeScript retains only the capability boundary that needs Node facilities:
candidate discovery inputs, skill filesystem mutation, creation of snapshot
files, and projection of neutral activity to configured adapters. It cannot
approve a transition or make a mutation durable without a Rust-issued plan and
receipt.

## Stored Models

All identifiers and timestamps are non-empty canonical strings. JSON columns
hold bounded domain payloads, never an aggregate store snapshot.

### Candidate

`curator_candidates` records:

- `candidate_id` primary key;
- `batch_id`, `profile_id`, and optional `session_id` scope;
- `kind`, `summary`, `fingerprint`, and `mutation_json`;
- `source_refs_json` and optional `expires_at`;
- `status`: `proposed`, `previewed`, `approved`, or `applied`;
- `lifecycle_state`: `active`, `stale`, or `archived`;
- optional `lifecycle_reason_code` and transition timestamps;
- monotonic `revision`, `created_at`, and `updated_at`.

Candidate upsert is idempotent only when the candidate ID and fingerprint are
unchanged. Reusing an ID for a different fingerprint is a conflict. State
changes use compare-and-swap against `revision`.

Indexes cover `(profile_id, status, updated_at)`,
`(profile_id, lifecycle_state, updated_at)`, `(batch_id, candidate_id)`, and
`expires_at`.

### Approval

`curator_approvals` records one current approval per candidate plus its audit
history:

- `approval_id` primary key and unique `receipt_id`;
- `candidate_id`, `candidate_revision`, and approved `fingerprint`;
- optional `actor_id`, required `reason`, and `approved_at`;
- optional `superseded_at`.

An approval is valid only while candidate fingerprint and revision match. A
candidate update supersedes the current approval; it never silently carries an
approval to new content.

Indexes cover `(candidate_id, approved_at)` and `(actor_id, approved_at)`.

### Snapshot Reference

`curator_snapshot_refs` stores metadata for filesystem snapshots:

- `snapshot_id` primary key and `candidate_id`;
- `snapshot_root_ref`, a service-relative opaque reference;
- `manifest_json`, containing service-relative paths, existence flags, and
  content hashes;
- `created_at`, optional `verified_at`, and `status` (`prepared`, `consumed`,
  `invalid`, or `purged`).

Rust validates that references are relative, contain no traversal, resolve
under the configured curator snapshot root, and match the candidate/mutation
being planned. Rust does not read or write skill files. Absolute host paths are
not returned by public query APIs.

### Mutation And Rollback

`curator_mutations` records:

- `mutation_id` primary key, unique `receipt_id`, and `candidate_id`;
- `candidate_revision`, `action`, optional `actor_id`, and `reason`;
- `snapshot_id`, `changed_paths_json`, and optional bounded
  `management_json`;
- `status`: `prepared`, `applied`, `failed`, `rollback_prepared`,
  `rolled_back`, or `rollback_failed`;
- optional `error_reason_code`;
- `created_at`, optional `applied_at`, optional `rolled_back_at`, and
  monotonic `revision`.

Apply and rollback are two-phase operations:

1. Rust validates current state and idempotency key, then persists a prepared
   mutation and returns a plan/receipt.
2. TypeScript performs the bounded filesystem operation.
3. TypeScript reports the outcome with the plan receipt and expected revision.
4. Rust atomically finalizes mutation, candidate, snapshot, and audit state.

On restart, `prepared` and `rollback_prepared` records remain diagnosable and
can be reconciled explicitly. They are never inferred as successful.

Indexes cover `(candidate_id, created_at)`, `(status, created_at)`,
`(snapshot_id)`, and `(actor_id, created_at)`.

### Audit Receipt

`curator_audit_receipts` is append-only:

- `sequence` backend-generated monotonic ordering key;
- `receipt_id` primary key;
- optional `correlation_id`, `idempotency_key`, `profile_id`, `session_id`,
  `candidate_id`, and `mutation_id`;
- `activity_kind`, `outcome`, optional `reason_code`, and `summary`;
- optional `actor_id`, `details_json`, and `occurred_at`.

An idempotency key is unique within its operation kind. Repeating the same key
and canonical input returns the original receipt. Reusing the key with changed
input returns `curator_idempotency_conflict`.

Indexes cover `(candidate_id, sequence)`, `(mutation_id, sequence)`,
`(profile_id, sequence)`, `(session_id, sequence)`,
`(activity_kind, sequence)`, and `(occurred_at, sequence)`.

## Repository Operations

The Rust repository surface is narrow and transactional:

- `upsert_candidate_batch`;
- `get_curator_candidate` and `list_curator_candidates`;
- `record_candidate_preview`;
- `approve_curator_candidate`;
- `transition_curator_candidate_lifecycle`;
- `prepare_curator_mutation` and `finalize_curator_mutation`;
- `prepare_curator_rollback` and `finalize_curator_rollback`;
- `get_curator_mutation` and `list_curator_mutations`;
- `list_curator_audit_receipts`;
- `purge_curator_profile` and `purge_curator_session`.

Writes return the resulting record plus its audit receipt. Multi-record state
changes and receipt insertion share one transaction. PostgreSQL uses row locks
for compare-and-swap transitions; SQLite uses an immediate write transaction.

## Query And Retention Contract

List operations use a bounded `limit` (default 50, maximum 200) and an opaque
cursor based on stable `(occurred_at, sequence)` or `(updated_at, id)` ordering.
They return `items`, `next_cursor`, and an exact `has_more`; exact totals are
optional and are not synthesized from a page.

Candidates, approvals, mutations, and audit receipts are retained by default.
Maintenance may purge:

- expired, never-approved candidates after a configured grace period;
- filesystem snapshots only after their mutation history is terminal and a
  configured retention period has elapsed;
- profile/session scoped records only through explicit destructive profile or
  session purge operations.

Purging a snapshot marks its reference `purged` before filesystem deletion is
requested. Audit receipts remain after payload retention unless an explicit
future policy introduces audited compaction.

## Reason Codes

The stable reason-code family includes:

- `curator_candidate_not_found`;
- `curator_candidate_revision_conflict`;
- `curator_candidate_fingerprint_conflict`;
- `curator_candidate_stale`;
- `curator_candidate_expired`;
- `curator_candidate_archived`;
- `curator_candidate_not_approved`;
- `curator_approval_stale`;
- `curator_mutation_not_found`;
- `curator_mutation_already_terminal`;
- `curator_mutation_noop`;
- `curator_snapshot_ref_invalid`;
- `curator_snapshot_unavailable`;
- `curator_idempotency_conflict`;
- `curator_filesystem_mutation_failed`;
- `curator_rollback_failed`.

Reason codes cross the bridge unchanged. Human summaries may evolve.

## Neutral Activity Contract

Rust emits `CuratorActivity` only after the corresponding durable transition
has committed. Every event contains:

- `receipt_id`, `correlation_id`, and monotonically ordered `sequence`;
- optional profile, session, candidate, mutation, and actor references;
- `phase`, `outcome`, optional `reason_code`, `summary`, and `occurred_at`;
- bounded details safe for operator diagnostics.

Phases are `candidate_discovered`, `review_routed`, `candidate_previewed`,
`candidate_approved`, `candidate_denied`, `mutation_prepared`,
`mutation_applied`, `mutation_failed`, `rollback_prepared`,
`rollback_completed`, `rollback_failed`, `candidate_staled`,
`candidate_archived`, and `candidate_reactivated`.

The contract has no Den URL, token, visibility vocabulary, or adapter-specific
payload. The TS observation adapter maps successful lifecycle/mutation events
to work checkpoints/completions and failures or stale conflicts to visible
failed/waiting activity. `receipt_id` becomes the result reference and
`correlation_id` becomes the run reference.

Projection is best-effort and occurs after the Rust commit. Adapter failure is
recorded in adapter diagnostics and may emit an adapter-degraded observation;
it cannot roll back, reject, or delay curator governance.

## Clean Break And Removal

Implementation removes the production `NativeCuratorGovernanceStore` snapshot
load/save path and the production use of `MemoryCuratorGovernanceStore` as an
authority. In-memory stores may remain only in isolated tests and smokes.
`FileCuratorGovernanceStore` is test-only or removed. No compatibility read,
dual-write, lazy conversion, or fallback to `simple_kv` is permitted.

The compiled module registry advertises the curator stores and query catalog.
Both backend migrations are additive to an empty curator module and are tested
for schema parity before service wiring changes.
