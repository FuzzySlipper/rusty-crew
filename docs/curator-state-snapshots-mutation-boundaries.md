# Curator State, Snapshots, And Mutation Boundaries

Status: Design contract for task 2970

Date: 2026-06-20

## Purpose

Rusty Crew needs curator workflows for skill and memory cleanup, but curator
work must not become a broad hidden mutation path. This document defines the
state model, snapshot/rollback expectations, and mutation boundaries for the
curator tasks that follow.

## Authority Split

Rust owns:

- scheduler/job/run records;
- runtime/session coordination;
- audit-grade state if curator candidates become Rust-persistent records;
- action validation when curator work affects runtime state.

TypeScript owns:

- candidate discovery over skill/profile files;
- static report generation;
- safe file-level previews;
- calling profile/skill tooling where the source state lives;
- bridge/admin route adapters that invoke curator services.

Den/adapters own:

- projection of curator reports and evidence to operators;
- product-data references;
- display-only task/comment/document updates.

No layer may treat Den projection as curator coordination truth.

## State Model

Curator state should be typed and auditable. The first durable model should be
small:

- candidate batch;
- candidate;
- snapshot reference;
- approval record;
- mutation record;
- rollback reference;
- run/report reference.

### Candidate Batch

Represents one discovery pass.

Fields:

- `batch_id`;
- `source`: scheduler run id, admin command id, or `curator_execute` receipt id;
- `scope_type`: profile, skills_root, project, session, or runtime;
- `scope_id`;
- `candidate_kinds`;
- `created_at`;
- `created_by`;
- `dry_run`;
- `candidate_count`;
- `report_ref`;
- `status`: open, superseded, applied, archived, or failed.

### Candidate

Represents one proposed action.

Fields:

- `candidate_id`;
- `batch_id`;
- `kind`: skill_patch, skill_archive, skill_create, sidecar_write,
  dense_memory_prune, dense_memory_merge, or diagnostics_only;
- `source_ref`;
- `target_ref`;
- `summary`;
- `severity`;
- `confidence`;
- `proposed_action`;
- `preview_ref`;
- `fingerprint`;
- `status`: proposed, previewed, approved, rejected, applied, failed,
  superseded, or expired;
- `expires_at`.

Candidates are proposals. They are not mutations.

### Snapshot Reference

Snapshot refs must be compact and inspectable without storing large source
bodies in scheduler records.

Acceptable snapshot refs:

- content hash plus file path for a skill file;
- copied snapshot path inside a curator-owned snapshot directory;
- Den document revision or memory revision id;
- runtime search row/ref for immutable runtime facts.

Snapshots should be taken before mutation, not after.

### Mutation Record

Mutation records describe what was applied.

Fields:

- `mutation_id`;
- `candidate_id`;
- `approved_by`;
- `approved_at`;
- `applied_by`;
- `applied_at`;
- `snapshot_ref`;
- `rollback_ref`;
- `changed_paths`;
- `result_ref`;
- `status`: applied, failed, rolled_back, or rollback_failed;
- `error`.

## Mutation Boundaries

Allowed first mutations:

- archive a skill by moving it to `.archive`;
- patch a skill body/frontmatter through the same validation as
  `skill_manage`;
- write a sidecar file under approved `slug.d/` subdirectories;
- create a skill from validated frontmatter and body;
- mark a candidate rejected/approved/applied.

Deferred mutations:

- deleting without archive;
- broad profile package rewrites;
- raw Den memory deletion;
- dense-memory merge/prune without a dedicated persistence API;
- arbitrary shell/file commands;
- edits outside configured skill/profile roots.

Forbidden behavior:

- mutating from candidate discovery;
- applying a mutation without an approval record unless it is dry-run;
- applying stale candidates whose fingerprint no longer matches source state;
- bypassing pinned/protected skill checks;
- treating `curator_execute` as an arbitrary command runner;
- replaying old curator proposals after source state changed.

## Approval And Confirmation

Destructive or broad changes require approval.

Minimum approval fields:

- actor;
- reason;
- candidate id;
- candidate fingerprint;
- approved action;
- timestamp;
- optional Den/task/comment refs.

`curator_execute` may request approval or application, but the governance
executor must validate that the candidate exists, is current, and is approved
before any non-dry-run mutation.

## Snapshot And Rollback Rules

Before a mutation:

1. Re-read source state.
2. Check the candidate fingerprint.
3. Check pinned/protected rules.
4. Write or reference a before snapshot.
5. Apply through safe primitives.
6. Record changed refs and rollback ref.

Rollback should restore from the snapshot when the storage type supports it.
Rollback is a mutation too and must be audited.

If rollback is not supported for a candidate kind, the preview and approval
surface must say so before apply.

## Relationship To Existing Tools

`skill_manage` remains the low-level governed skill write primitive. It already
validates paths, frontmatter, pinned deletion, and archive-on-delete behavior.

`curator_execute` remains a receipt/control tool. It should operate on candidate
ids and delegate actual effects to a governance executor that can call safe
primitives such as `skill_manage` internally after approval.

The first skill-scoped mutation executor and rollback scaffold are documented in
`curator-mutation-executor-safeguards.md`.

Background memory/skill review produces findings and proposed candidates. It
does not mutate directly.

## Report Shape

The initial static discovery/report surface is implemented and documented in
`curator-candidate-discovery-reporting.md`.

Curator reports should include:

- batch id;
- scope;
- candidate count by kind/severity/status;
- top findings;
- skipped/error counts;
- result refs;
- next available actions;
- warnings about unsupported rollback or stale candidates.

Reports should avoid full skill bodies or memory contents. Use refs and bounded
snippets.

## First Implementation Sequence

1. Candidate discovery/report generation over skills.
2. In-memory or file-backed candidate batch store for smokes.
3. Preview/diff generation for skill candidates.
4. Approval state and receipt flow through `curator_execute`.
5. Snapshot-before-write and archive/patch application via safe skill
   primitives.
6. Admin routes and observation/audit projection.
7. End-to-end proof with dry-run, approval, apply, and rollback evidence.
