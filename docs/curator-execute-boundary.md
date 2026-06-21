# Curator Execute Boundary

Status: Implementation note for task 2907

Date: 2026-06-20

## Purpose

`curator_execute` is the first model-callable governance affordance for curator
work. It is intentionally narrow: the tool requests auditable curator actions
through an injected governance executor and returns receipts. It does not
directly edit skill files, Den data, memory records, runtime persistence, or
profile packages.

## Boundary

TypeScript brain island owns:

- the model-callable tool schema;
- parameter validation;
- fail-closed behavior when no executor is configured;
- confirmation checks for non-dry-run apply operations;
- returning bounded receipts to the brain.

Governance/control code owns:

- candidate storage;
- approval state;
- mutation snapshots and rollback refs;
- audit and observation records;
- actual mutation execution.

Rust owns:

- runtime/session coordination;
- scheduled job and wake authority;
- any future Rust-side governance persistence or runtime event validation.

## Supported First Actions

The first tool supports four actions:

- `request_scan`: ask the curator/governance layer to discover candidates for a
  scoped target.
- `preview_candidate`: request a preview/diff/report for one candidate.
- `approve_candidate`: record approval intent for one candidate with a reason.
- `apply_candidate`: request application of one candidate. Non-dry-run apply
  requires explicit `confirm: true` unless the executor context deliberately
  disables that guard.

All actions require an injected `CuratorExecuteContext.executor`. Without one,
the tool returns `curator_executor_unavailable`.

## Guardrails

- No arbitrary command names.
- No arbitrary file paths or SQL.
- No mutation without a configured executor.
- Candidate actions require `candidateId`.
- Approval/apply actions require `reason`.
- Non-dry-run apply requires confirmation by default.
- Profiles may restrict allowed actions with `allowedActions`.

## Relationship To `skill_manage`

`skill_manage` is a governed skill write tool for profile-approved contexts.
`curator_execute` is a control receipt tool for curator candidates and
governance workflows. It should not duplicate low-level file-edit behavior.

When curator persistence lands, `curator_execute` should operate on candidate
ids, snapshots, approvals, and rollback refs. The underlying mutation may call
the same safe skill-management primitives, but only after governance approval.

The first injected governance executor for skill-scoped curator mutations is
documented in `curator-mutation-executor-safeguards.md`.

## Follow-On Work

The current executor interface is intentionally injectable. Future curator work
should add:

- state, snapshot, approval, and mutation rules from
  `curator-state-snapshots-mutation-boundaries.md`;
- durable candidate and run state;
- report generation;
- snapshot/rollback records;
- admin routes and operator controls;
- observation/audit projection;
- end-to-end proof with a real governance executor.
