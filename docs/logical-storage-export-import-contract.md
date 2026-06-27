# Logical Storage Export/Import Contract

Status: implementation contract for task 3413

Date: 2026-06-26

## Purpose

Rusty Crew migrations and backend portability use service-owned logical records,
not raw database dumps. Raw SQLite or PostgreSQL dump files may be operational
backup tools, but they are not the cross-backend contract.

The first local PostgreSQL exercise remains a clean empty-database backend
switch against the den-k8 development service. It does not migrate the current
SQLite service data.

## Bundle Shape

The Rust-owned `LogicalStorageExportBundle` is versioned and grouped by
repository/module. A bundle records:

- bundle version and export id;
- export timestamp and optional service version;
- source backend, backend label, source instance id, and snapshot ref;
- storage schema version;
- module schema versions and descriptor fingerprints;
- source capability snapshot;
- repository bundles;
- legacy id mappings;
- profile asset refs.

Each `LogicalStorageRepositoryBundle` declares:

- repository id, such as `runtime_counters` or `queues_messages`;
- repository schema version;
- required storage capabilities;
- exported count;
- optional checksum;
- typed records.

Typed records can carry repository-specific payloads, such as
`QueueMessage`, or a bounded typed JSON payload for repositories whose concrete
logical record is not yet specialized.

## Dry-Run Validation

`CoordinationStore::validate_logical_storage_import` validates a bundle without
writing imported records.

The dry-run input declares:

- import batch id;
- target backend label;
- validation timestamp;
- supported capabilities;
- supported repositories.

The report includes:

- source and target backend;
- repository and record counts;
- accepted, unsupported, and refused record counts;
- whether the import batch id is already recorded;
- structured issues with severity, code, repository id, record id, and message.

An import batch id is the idempotency key. If that batch already exists in
`runtime_import_batches`, the dry run reports `import_batch_already_recorded`
and `can_apply()` returns false.

## Queue Safety

Queue import validation is deliberately strict because stale queue rows can
resurrect old work.

For `queues_messages` records:

- `pending` rows whose `expires_at` is not in the future are refused with
  `queue_pending_expired_would_resurrect`;
- `pending` rows with `terminal_at` are refused;
- terminal rows without `terminal_at` are refused;
- queue records outside the `queues_messages` repository are refused.

Future apply/import work may choose to import stale pending rows as terminal
`expired` records, but it must never make expired work deliverable.

## Capability Checks

Repository bundles list required capabilities. Dry-run validation rejects a
repository when the declared target capability set does not include the required
capabilities.

If a target repository allowlist is supplied, repositories outside that allowlist
are rejected as unsupported. This lets a PostgreSQL proof slice validate the
parts it actually owns without pretending to support the full service store.

## Current Boundary

Current implementation covers typed contract definitions and dry-run validation.
It does not apply records.

The `logical_export_import` SQLite diagnostic capability means logical bundle
contracts and dry-run validation are available. It does not mean full
SQLite-to-PostgreSQL migration is production-ready.

Future apply work must add repository-specific import implementations,
post-import count/checksum validation, quiesce/read-only runbook steps, and
backend-specific conformance coverage.
