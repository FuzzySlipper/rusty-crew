# Curator Mutation Executor Safeguards

Status: Implementation note for task 2972

Date: 2026-06-21

## Purpose

Rusty Crew now has a first curator governance executor for approved skill
mutations. It is intentionally narrow and lives in the TypeScript brain-island
layer because skill files and skill management policy currently live there.

This executor is a bridge between `curator_execute` control receipts and the
existing `skill_manage` primitive. It is not a general filesystem editor,
memory editor, Den writer, or shell executor.

## Implemented Surface

`createCuratorGovernanceExecutor` accepts:

- a skills root;
- a `MemoryCuratorGovernanceStore`;
- optional snapshot root and clock;
- optional scan callback for `request_scan`.

It handles the existing `curator_execute` actions:

- `request_scan`: records a returned candidate batch when a scan callback is
  supplied;
- `preview_candidate`: runs the candidate through `skill_manage` in dry-run
  curator mode;
- `approve_candidate`: records actor, reason, timestamp, and candidate
  fingerprint;
- `apply_candidate`: requires prior approval for non-dry-run application,
  validates the candidate is current, snapshots before mutation, delegates to
  `skill_manage`, and records mutation evidence.

`rollbackCuratorMutation` restores the recorded snapshot for applied mutations.

## Supported Mutations

The first executor supports skill-scoped operations only:

- `skill_patch`: full-content or unique-string replacement through
  `skill_manage`;
- `skill_create`: create a new skill file through `skill_manage`;
- `skill_archive`: archive a skill through `skill_manage delete`;
- `sidecar_write`: write an approved sidecar file through `skill_manage`.

The executor does not yet implement dense-memory merge/prune, pin/unpin state,
or skill restore as a direct candidate operation. Rollback can restore snapshots
for applied candidate mutations.

## Safety Rules

Before approval or apply, the executor checks skill source refs against current
skill content. If the source hash changed, the candidate is stale and denied.

Before non-dry-run apply, the executor writes a snapshot under:

`<skillsDir>/.curator/snapshots/<candidate-and-timestamp>/`

Snapshots include the active skill file and sidecar state needed for rollback.
Sidecar write candidates additionally snapshot the targeted sidecar file.

Non-dry-run apply requires:

- an approval record;
- a matching approval fingerprint;
- a current candidate source hash;
- a supported mutation type;
- successful `skill_manage` execution in curator-approved mode.

## Rollback

Rollback restores from the snapshot record:

- existing skill files are copied back;
- skill files created by a candidate are removed;
- sidecar directories or sidecar files are restored or removed according to
  their snapshot state.

Rollback is recorded on the in-memory mutation record as `rolled_back` or
`rollback_failed`. Durable rollback audit belongs in the future persistence and
admin-route work.

## Current Limits

`MemoryCuratorGovernanceStore` is a smoke/test scaffold, not durable truth.
Future tasks should move candidate, approval, mutation, rollback, and audit
records into Rust-owned or otherwise durable governance persistence.

The executor does not emit observation activity directly yet. Observation and
admin/control integration remain follow-up tasks so that display projection does
not become the coordination authority.
