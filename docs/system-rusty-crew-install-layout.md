# `/home/system/rusty-crew` Fresh Install Layout

Status: design for task 3503

Date: 2026-06-27

## Purpose

Rusty Crew will move from the current local runtime root
`/home/agents/rusty-crew` to a fresh infrastructure-owned root:

```text
/home/system/rusty-crew
```

This is not a migration of the current SQLite install. The new root should be
generated deliberately so legacy `/home/agents` paths, stale `service.json`
plumbing, and copied secret values do not sneak into the PostgreSQL deployment.

The old root remains rollback material until Patch decides it can be archived.

## Directory Layout

```text
/home/system/rusty-crew/
  config/
    service.env
    service.json
    profiles/
    skills/
  data/
    engine/
    generated/
    governance/
  logs/
  run/
  artifacts/
  backups/
  site/
  README.md
```

### `config/`

Service-owned configuration files.

- `service.env`: generated fresh for the new root.
- `service.json`: generated fresh for the new root.
- `profiles/`: selected profile asset directories/files.
- `skills/`: selected service-wide skill directories.

Do not copy the current `/home/agents/rusty-crew/config/service.env` wholesale.
It contains live adapter tokens and old path values.

### `data/`

Runtime data local to the service instance.

For PostgreSQL cutover, durable coordination state lives in PostgreSQL rather
than a local SQLite database. The local `data/engine/` directory remains useful
for backend-neutral engine scratch files, generated diagnostics, or future
non-database state, but the full-service smoke must verify that no
`coordination.sqlite3` file is created when the backend is PostgreSQL.

`data/governance/` may hold generated governance snapshots such as curator
state if the feature still uses file state after the backend work. Any such
file state should be explicitly inventoried during cutover.

### `logs/`

Service logs and operator-readable run output.

### `run/`

Volatile runtime files such as lock files and PID/status crumbs. This directory
must be safe to clear while the service is stopped.

### `artifacts/`

Generated task/tool artifacts owned by this service instance.

### `backups/`

Operator backups and pre-cutover snapshots. The old `/home/agents/rusty-crew`
root should be backed up before the final service switch, but the backup should
not be imported into the fresh PostgreSQL database during the first cutover.

### `site/`

Static frontend assets served by the Rusty Crew service. The final cutover can
copy a built Rusty View bundle here, but it should be treated as a deployable
frontend artifact, not runtime state.

## Generated `service.env`

The fresh env file should use the new root consistently:

```text
RUSTY_CREW_DATA_DIR=/home/system/rusty-crew
RUSTY_CREW_CONFIG_DIR=/home/system/rusty-crew/config
RUSTY_CREW_ENGINE_DATA_DIR=/home/system/rusty-crew/data/engine
RUSTY_CREW_LOG_DIR=/home/system/rusty-crew/logs
RUSTY_CREW_RUN_DIR=/home/system/rusty-crew/run
RUSTY_CREW_ARTIFACT_DIR=/home/system/rusty-crew/artifacts
RUSTY_CREW_BACKUP_DIR=/home/system/rusty-crew/backups
RUSTY_CREW_STATIC_DIR=/home/system/rusty-crew/site
RUSTY_CREW_DEFAULT_WORKDIR=/home

RUSTY_CREW_ADMIN_HOST=0.0.0.0
RUSTY_CREW_ADMIN_PORT=9347
RUSTY_CREW_ADMIN_ALLOW_LAN=true
RUSTY_CREW_ADMIN_AUTH_MODE=none

RUSTY_CREW_STORAGE_BACKEND=postgres
RUSTY_CREW_POSTGRES_DATABASE_URL_ENV=RUSTY_CREW_DATABASE_URL
RUSTY_CREW_POSTGRES_SCHEMA=rusty_crew
RUSTY_CREW_POSTGRES_MAX_CONNECTIONS=10
RUSTY_CREW_POSTGRES_STATEMENT_TIMEOUT_MS=30000

RUSTY_CREW_SCHEDULER_TICK_INTERVAL_MS=1000
RUSTY_CREW_WAKE_DISPATCH_INTERVAL_MS=250
```

The database URL value must come from a secret env source such as:

```text
/home/system/database/rusty-crew-postgres.env
```

Do not write the actual database URL into committed docs, Den messages, or
non-secret config.

Adapter tokens should also come from a secret env source rather than being
copied from the old `service.env`. The final systemd unit may use multiple
`EnvironmentFile=` entries:

```ini
EnvironmentFile=/home/system/database/rusty-crew-postgres.env
EnvironmentFile=/home/system/rusty-crew/config/service.env
EnvironmentFile=-/home/system/rusty-crew/config/adapter-secrets.env
```

`adapter-secrets.env` is optional in this design because token packaging may be
handled by existing den-services env mirrors or a future secret manager.

## Generated `service.json`

`service.json` should be generated as a clean runtime graph for the new
deployment. It must not be copied wholesale from `/home/agents/rusty-crew`.

Minimum shape:

```json
{
  "profilesDir": "/home/system/rusty-crew/config/profiles",
  "skillsDir": "/home/system/rusty-crew/config/skills",
  "storage": {
    "backend": "postgres",
    "postgres": {
      "databaseUrlEnv": "RUSTY_CREW_DATABASE_URL",
      "schema": "rusty_crew",
      "maxConnections": 10,
      "statementTimeoutMs": 30000
    }
  },
  "brains": [],
  "sessions": [],
  "channelBindings": [],
  "mcpBindings": []
}
```

After the profile registry path is fully PostgreSQL-backed, normal profile
creation should happen through the official admin API and DB-backed registry
instead of by editing `service.json`. If bootstrap agents are required for the
first boot, add only the minimal initial brains/sessions/bindings needed for
the smoke and document why each was seeded.

## What To Regenerate

Regenerate these for `/home/system/rusty-crew`:

- directory tree;
- `config/service.env`;
- `config/service.json`;
- systemd unit or drop-in pointing at the new env files;
- static frontend deployment under `site/`;
- any README/operator note for the fresh root.

## What To Copy Intentionally

Copy only after explicit review:

- selected profile prompt assets from the current install if still needed;
- selected service-wide skills;
- frontend build assets;
- non-secret operator notes.

Do not copy:

- SQLite database/WAL/SHM files;
- `run/service.lock`;
- `logs/service.log`;
- old `service.env` token lines;
- old generated runtime graph entries unless they are intentionally reseeded;
- stale smoke-test profiles that are no longer part of the deployment.

## Systemd Plan

The final cutover should keep one live service named `rusty-crew.service`.
Avoid running parallel `rusty-crew` instances unless a short, explicit smoke
task uses a different port and service name.

The eventual user unit should keep the source checkout as the working
directory, but point state and secrets at `/home/system`:

```ini
[Service]
Type=simple
WorkingDirectory=/home/dev/rusty-crew
EnvironmentFile=/home/system/database/rusty-crew-postgres.env
EnvironmentFile=/home/system/rusty-crew/config/service.env
EnvironmentFile=-/home/system/rusty-crew/config/adapter-secrets.env
ExecStart=/usr/bin/env npm run service:start
Restart=on-failure
RestartSec=5s
KillSignal=SIGTERM
TimeoutStopSec=30s
```

If the final deployment later uses a packaged release instead of the source
checkout, update `WorkingDirectory` and `ExecStart` in a separate deployment
task.

## Cutover/Backout Posture

Before final cutover:

1. Stop the current service.
2. Take a consistent backup/snapshot of `/home/agents/rusty-crew`.
3. Leave the old SQLite root untouched.
4. Start the single `rusty-crew.service` against `/home/system/rusty-crew`.
5. Verify the active backend is PostgreSQL and no SQLite file was created in the
   new root.

Backout is the inverse:

1. Stop the PostgreSQL-backed service.
2. Restore the old systemd env/root pointers to `/home/agents/rusty-crew`.
3. Start the old SQLite-backed service.
4. Verify admin diagnostics and agent/channel behavior.

Do not run both normal services on port `9347`.

## Verification For Task 3509/3510

The fresh install task should verify:

- every expected directory exists with intended ownership/permissions;
- `service.env` contains `/home/system/rusty-crew` paths and no copied secret
  values;
- Postgres secret env is sourced from `/home/system/database`;
- `service.json` points at `/home/system/rusty-crew/config/profiles` and
  `/home/system/rusty-crew/config/skills`;
- startup creates no `coordination.sqlite3` in the new root;
- `/v1/admin/diagnostics/storage` reports active backend `postgres`;
- profile create/start, channel wake, tool calls, memory/lore, and restart
  hydration pass before task 3511 is attempted.
