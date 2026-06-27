# PostgreSQL And SQLite Storage Deployment Runbook

Status: operator runbook for task 3415

Date: 2026-06-26

## Defaults

SQLite is the default Rusty Crew storage backend. It is first-class for local,
container, small-agent, and early Rusty Roleplay deployments.

Use SQLite when:

- one Rusty Crew service process owns writes;
- the deployment is local or containerized;
- the service runs a couple agents;
- roleplay is small enough that transcript/lore/search growth is visible and
  maintainable;
- operational simplicity matters more than multi-writer concurrency.

The expected local file lives under the service data directory, for example:

```text
/home/agents/rusty-crew/data/engine/coordination.sqlite3
```

SQLite WAL mode also creates `coordination.sqlite3-wal` and
`coordination.sqlite3-shm`.

## SQLite Operating Assumptions

SQLite assumes a single service writer. Other processes should not open
writer-heavy connections to the Crew database. Use service APIs and admin
diagnostics instead of manual DB spelunking for normal operations.

Do not copy only the main SQLite database file while the service is active. WAL
mode means the WAL and SHM files matter too. Backup/export should be quiesced,
snapshot-based, or service-owned.

For the first small Rusty Roleplay deployment, SQLite is the recommended
backend. Keep lore and memory inside Crew storage through typed memory/module
repositories rather than adding an external lore DB service.

## When To Consider PostgreSQL

Start PostgreSQL planning when diagnostics show:

- WAL growth beyond maintenance windows;
- persistent freelist pressure after retention;
- hot query-plan failures;
- runtime search health problems;
- dozens of active agents or agent instances;
- multi-user roleplay traffic;
- large transcript, lore, attachment, or runtime-search workloads;
- provider wire-state growth from Responses-style brains;
- independent service processes need concurrent writes;
- online maintenance or operational isolation becomes important.

Storage diagnostics and admin screens are the green path for seeing this
pressure. The pressure signals are guidance for operators, not automatic
migration triggers.

## PostgreSQL Posture

PostgreSQL is the future scale/concurrency backend, but the full Rusty Crew
service is not production-ready on PostgreSQL yet.

Current PostgreSQL proof coverage is intentionally narrow:

- storage/admin diagnostics proof;
- runtime-counter proof;
- simple key/value module proof.

Unsupported high-risk surfaces include queues/messages, scheduler claims,
transcripts, attachments, runtime search, profile memory, roleplay lore, and
provider wire state unless later tasks explicitly implement them.

The service must fail closed rather than silently falling back to SQLite or
advertising PostgreSQL production readiness.

## PostgreSQL Config

The intended config shape is:

```json
{
  "storage": {
    "backend": "sqlite",
    "sqlite": {
      "path": "coordination.sqlite3",
      "wal": true,
      "busyTimeoutMs": 5000
    },
    "postgres": {
      "databaseUrlEnv": "RUSTY_CREW_DATABASE_URL",
      "schema": "rusty_crew",
      "maxConnections": 10,
      "statementTimeoutMs": 30000
    }
  }
}
```

Rules:

- omitted storage config means SQLite in the engine data directory;
- PostgreSQL connection strings come from environment variables or secret
  providers;
- secrets are never copied into repo docs, committed config, or Den task prose;
- schema names should be explicit and deployment-scoped;
- pool sizes and statement timeouts should be conservative until measured.

The local den-k8 development PostgreSQL service is documented in Den at
`den-network/rusty-crew-postgres-service`. Use that doc for host/env details.
Do not copy its secret values into this repository.

## Fresh PostgreSQL Test Path

The first local PostgreSQL service exercise should use a fresh empty database:

1. Leave the current SQLite service data untouched.
2. Configure PostgreSQL using the den-k8 service env.
3. Boot only in a mode whose repository coverage is implemented.
4. Run storage diagnostics, admin smokes, and repository conformance tests.
5. Confirm unsupported repositories are visible and fail closed.

This path proves PostgreSQL as a first-class empty backend. It is separate from
SQLite-to-PostgreSQL migration.

## Migration And Logical Export/Import

Future migration uses the logical storage bundle contract in
`docs/logical-storage-export-import-contract.md`.

Migration flow should be:

1. Quiesce the source service or enter a read-only maintenance window.
2. Export a service-owned logical bundle grouped by repository/module.
3. Run dry-run import validation against the target backend.
4. Review unsupported repositories, missing capabilities, count/checksum
   mismatches, and queue safety refusals.
5. Apply only after every required repository has an implementation.
6. Run post-import count/checksum validation and service smoke tests.
7. Keep source data untouched until the target deployment is proven.

Queue safety is mandatory. Pending queue rows that are already expired must not
be imported as deliverable work, and terminal rows must preserve terminal state
so old messages cannot resurrect.

## Backups

SQLite backup should capture the database plus WAL/SHM consistently or use a
service-owned export/snapshot route.

PostgreSQL backups should use normal PostgreSQL operational tooling for
database-level backup, plus logical export bundles when portability across
backends or schema modules is the goal.

Raw dumps are operational backups. Logical bundles are portability/migration
contracts.
