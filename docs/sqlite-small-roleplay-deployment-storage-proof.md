# SQLite Small Roleplay Deployment Storage Proof

Status: implemented for task 3418.

Design source: ADR 0020, `storage-backend-abstraction-and-postgresql-readiness`.

## Purpose

SQLite remains the default storage backend for local, container, and small-agent
deployments. It is not a bootstrap-only fallback.

The first expected Rusty Roleplay deployment path is a small container with a
couple of agents and a visible SQLite file under the service data directory.
That path should stay boring and supportable while PostgreSQL grows as the
larger multi-agent/multi-user backend.

## Proof Fixture

The focused proof is:

```bash
cargo test -p rusty-crew-core-persistence \
  sqlite_small_roleplay_deployment_storage_proof
```

The fixture uses `CoordinationStore::open` with a temporary data directory,
matching the small service/container shape where the database file is created
under an engine data directory.

It covers early roleplay storage surfaces:

- profile registry plus session/config persistence;
- conversation branch, message slot, and primary transcript message writes;
- dense profile/user memory;
- runtime message search;
- provider wire-state persistence and wake lookup;
- scheduled maintenance job persistence;
- storage diagnostics, search health, repository group projection, WAL
  checkpoint maintenance, and optimize maintenance.

The test also asserts WAL size stays bounded below the current pressure
threshold after maintenance.

## Deployment Assumptions

Recommended SQLite posture for small roleplay/container deployments:

- run one Rusty Crew service process as the SQLite writer;
- store the database in the service data directory, for example
  `/home/agents/rusty-crew/data/engine/coordination.sqlite3`;
- keep WAL enabled unless there is a specific filesystem reason not to;
- expose storage diagnostics in the admin UI so WAL bytes, search health,
  maintenance capability, and table counts stay visible;
- run periodic maintenance with WAL checkpoint and SQLite optimize;
- keep transcript, profile memory, provider state, and scheduler data in Rusty
  Crew storage rather than an external lore database for the small deployment
  path.

## Scale Boundary

SQLite is the right default for a single container with a couple of agents. Move
toward PostgreSQL when the deployment needs multiple service writers, many
active agents, many simultaneous users, large transcript/lore workloads, or
operational database controls beyond a single visible local file.

Queues and scheduler claims remain correctness-sensitive. Do not treat SQLite
fixtures as permission to loosen TTL, terminal-state, or claim semantics during
future backend work.
