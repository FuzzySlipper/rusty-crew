# Deployment And Storage

This is the canonical setup guide for a Rusty Crew service. The service runs
from a source checkout today and keeps mutable state, credentials, static site
files, locks, artifacts, and backups in a separate runtime root.

For current-machine operational details and recovery procedures, also see the
[local service runbook](local-service-runbook.md). For repository-level storage
design and backend migration semantics, see
[ADR 0020](adr/0020-storage-backend-abstraction-and-postgresql-readiness.md) and
the [logical export/import contract](logical-storage-export-import-contract.md).

## Choose A Backend

SQLite and PostgreSQL are both first-class backends behind the same Rust-owned
repository boundary.

Use SQLite when one Crew process owns the database and operational simplicity
matters most. It is the default and recommended backend for small/container
deployments, a few agents, and early roleplay installations.

Use PostgreSQL for larger, longer-lived services: dozens of active agents,
multi-user roleplay, high transcript/lore/search volume, online maintenance, or
workloads where write concurrency and operational isolation matter.

Backend choice does not change the HTTP/profile/provider APIs. It does change
backup operations and the storage environment. Rusty Crew does not silently
migrate a SQLite file into PostgreSQL; use a fresh target or an explicit
logical export/import workflow when that contract supports the required data.

## Runtime Layout

Choose one absolute runtime root per service:

```text
<root>/
  config/
    service.env
    service.json             required runtime graph; may be empty
    adapter-secrets.env      optional external-adapter secrets
  data/engine/               SQLite file or local engine artifacts
  site/                      optional static frontend build
  run/                       service lock
  logs/                      reserved; current process logs use journald
  artifacts/
  backups/
```

The current local topology uses:

| Service | Root | Port | Backend |
| --- | --- | --- | --- |
| live | `/home/system/rusty-crew` | `9347` | PostgreSQL, schema `rusty_crew` |
| debug | `/home/system/rusty-crew-debug` | `9348` | SQLite at `data/engine/coordination.sqlite3` |

Never share a root, SQLite file, or PostgreSQL schema between service
processes. Use the debug service for disposable profiles, live-provider smokes,
and Rusty View certification.

## Prerequisites And Build

Install the pinned Node/npm and Rust toolchains, a C/C++ build toolchain for
napi-rs, and PostgreSQL or SQLite command-line clients only when their backup
operations are needed.

```bash
cd /home/dev/rusty-crew
npm ci
npm run build:native
npm run verify:offline
```

The native addon is built with PostgreSQL support by default and can run either
backend at service startup.

## Common Service Environment

Create the runtime directories and start from the example:

```bash
install -d \
  /home/system/rusty-crew/{config,data/engine,site,run,logs,artifacts,backups}
cp ops/systemd/service.env.example \
  /home/system/rusty-crew/config/service.env
chmod 600 /home/system/rusty-crew/config/service.env
```

At minimum, set paths, deployment role, listener, admin auth, and storage:

```dotenv
RUSTY_CREW_DATA_DIR=/home/system/rusty-crew
RUSTY_CREW_DEPLOYMENT_ROLE=production
RUSTY_CREW_CONFIG_DIR=/home/system/rusty-crew/config
RUSTY_CREW_ENGINE_DATA_DIR=/home/system/rusty-crew/data/engine
RUSTY_CREW_LOG_DIR=/home/system/rusty-crew/logs
RUSTY_CREW_RUN_DIR=/home/system/rusty-crew/run
RUSTY_CREW_ARTIFACT_DIR=/home/system/rusty-crew/artifacts
RUSTY_CREW_BACKUP_DIR=/home/system/rusty-crew/backups
RUSTY_CREW_DEFAULT_WORKDIR=/home

RUSTY_CREW_ADMIN_HOST=0.0.0.0
RUSTY_CREW_ADMIN_PORT=9347
RUSTY_CREW_ADMIN_ALLOW_LAN=true
RUSTY_CREW_ADMIN_AUTH_MODE=bearer
RUSTY_CREW_ADMIN_TOKEN=replace-with-local-token

RUSTY_CREW_SCHEDULER_TICK_INTERVAL_MS=1000
RUSTY_CREW_WAKE_DISPATCH_INTERVAL_MS=250
RUSTY_CREW_CHAT_COMPLETIONS_MAX_TOOL_ROUNDS=512
RUSTY_CREW_OPENAI_RESPONSES_MAX_CONTINUATION_ROUNDS=512
```

The continuation values are temporary compatibility ceilings and should remain
at the implementation maximum. They are not intended as normal turn limits:
healthy long-running work should eventually yield into a durable continuation
instead of failing at a provider/tool-round count. Provider-request and session
turn timeouts may be left unset when explicit user cancellation is the desired
lifecycle policy.

`0.0.0.0` is appropriate for the current trusted-LAN deployment. Use an
appropriate interface and bearer auth for other environments. Explicit
tokenless local development is supported with
`RUSTY_CREW_ADMIN_AUTH_MODE=none`; omitting a bearer token without selecting
that mode is not the same thing.

Do not commit `service.env`, adapter tokens, provider credentials, database
URLs, or OAuth state.

## SQLite Setup

SQLite is selected by default, but an explicit configuration makes deployment
intent clear:

```dotenv
RUSTY_CREW_STORAGE_BACKEND=sqlite
RUSTY_CREW_SQLITE_PATH=coordination.sqlite3
RUSTY_CREW_SQLITE_WAL=true
RUSTY_CREW_SQLITE_BUSY_TIMEOUT_MS=5000
```

A relative `RUSTY_CREW_SQLITE_PATH` resolves under
`RUSTY_CREW_ENGINE_DATA_DIR`; an absolute path is also accepted. WAL mode is on
by default. One Crew process should own writes, and normal inspection should go
through service APIs rather than writer connections from other processes.

SQLite uses the main file plus `-wal` and `-shm` companions. Do not copy only
the main file while the service is running. Use the backup helper, which calls
SQLite's online backup operation:

```bash
ops/scripts/rusty-crew-backup.sh \
  --root /home/system/rusty-crew-debug \
  --backend sqlite
```

## PostgreSQL Setup

Keep the database URL in a separate root-owned or user-private environment
file. The service config names the environment variable; it must not contain
the raw URL itself.

`/home/system/database/rusty-crew-postgres.env`:

```dotenv
RUSTY_CREW_DATABASE_URL=postgresql://user:password@host:5432/rusty_crew
```

`<root>/config/service.env`:

```dotenv
RUSTY_CREW_STORAGE_BACKEND=postgres
RUSTY_CREW_POSTGRES_BOOT_MODE=active
RUSTY_CREW_POSTGRES_DATABASE_URL_ENV=RUSTY_CREW_DATABASE_URL
RUSTY_CREW_POSTGRES_SCHEMA=rusty_crew
RUSTY_CREW_POSTGRES_MAX_CONNECTIONS=10
RUSTY_CREW_POSTGRES_STATEMENT_TIMEOUT_MS=30000
```

`RUSTY_CREW_POSTGRES_BOOT_MODE=active` and a populated URL variable are both
required for full PostgreSQL service startup. `blocked` and `proof_admin` are
development/proof modes, not production deployment modes.

Startup applies the backend's versioned migration ledger and fails closed on
unsupported future schema versions. Use an explicit deployment-scoped schema;
two services using the same database still need different schemas.

The repo's live systemd unit loads the database secret before the service env:

```ini
EnvironmentFile=/home/system/database/rusty-crew-postgres.env
EnvironmentFile=/home/system/rusty-crew/config/service.env
```

Back up PostgreSQL with normal database tooling through the repo helper:

```bash
ops/scripts/rusty-crew-backup.sh \
  --root /home/system/rusty-crew \
  --backend postgres \
  --database-env /home/system/database/rusty-crew-postgres.env
```

The helper writes a custom-format `pg_dump` and a SHA-256 file. Raw dumps are
operational backups; logical bundles are the backend-portable format.

## Runtime Graph

`<root>/config/service.json` is required by the current source-run service.
For a new deployment, create the minimal empty runtime graph below; profiles
and providers can then be created through admin APIs. Service-wide runtime
policy and configured sessions may also be declared here.

```json
{
  "profilesDir": "/home/system/rusty-crew/config/profiles",
  "wakeTimeout": { "mode": "disabled" },
  "brains": [],
  "sessions": []
}
```

Profiles and model providers are database-backed administration surfaces. Use
their official APIs/UI instead of adding inline provider fallback blocks to
`service.json`. Runtime config can be applied without restarting through the
validated draft/apply API or reloaded from disk:

```bash
curl -X POST \
  -H "Authorization: Bearer $RUSTY_CREW_ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"reason":"operator config update"}' \
  http://127.0.0.1:9347/v1/admin/control/config/reload
```

Configured session IDs are durable. Reload and restart reuse them; creating a
new session is an explicit API/UI operation.

## Preflight And Direct Run

Source the service environment and, for PostgreSQL, the database secret:

```bash
set -a
. /home/system/database/rusty-crew-postgres.env  # PostgreSQL only
. /home/system/rusty-crew/config/service.env
set +a

cd /home/dev/rusty-crew
npm run service:preflight
npm run service:start
```

Preflight validates the storage selection, PostgreSQL boot mode and URL
presence, paths, listener config, and service lock before startup.

## User Systemd Service

The repo units describe the current live/debug paths. Copy and edit a unit if
your root, secret file, or source checkout differs.

```bash
install -d ~/.config/systemd/user
cp ops/systemd/rusty-crew.service ~/.config/systemd/user/
cp ops/systemd/rusty-crew-debug.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now rusty-crew.service
systemctl --user enable --now rusty-crew-debug.service
```

Inspect the service through systemd and journald:

```bash
systemctl --user status rusty-crew.service
journalctl --user -u rusty-crew.service -f
```

The source-run unit uses `Type=notify`, a 45-second watchdog, preflight before
start, and restart-on-failure. Logs currently live in journald.

## Health, Diagnostics, And Static UI

Shallow health is unauthenticated. Readiness and diagnostics follow admin auth:

```bash
curl http://127.0.0.1:9347/v1/admin/healthz
curl -H "Authorization: Bearer $RUSTY_CREW_ADMIN_TOKEN" \
  http://127.0.0.1:9347/v1/admin/readyz
curl -H "Authorization: Bearer $RUSTY_CREW_ADMIN_TOKEN" \
  http://127.0.0.1:9347/v1/admin/diagnostics/storage
```

The built-in diagnostics page is `/admin`. If `<root>/site` exists, Crew serves
that static frontend at `/` and keeps `/v1/*` for APIs. This allows Rusty View
or another frontend to be deployed beside the service without a separate API
origin.

## Backend Verification

After setup or a backend change, verify the effective backend instead of
assuming the environment was loaded:

```bash
curl -fsS \
  -H "Authorization: Bearer $RUSTY_CREW_ADMIN_TOKEN" \
  http://127.0.0.1:9347/v1/admin/diagnostics/storage \
  | jq '.data.backend'
```

Expected values are `sqlite` or `postgres`. Also confirm the two services do
not show each other's profiles or sessions; shared rows are a deployment error,
not synchronization behavior.

## Further Detail

- [Local service topology](local-service-topology.md)
- [PostgreSQL and SQLite storage runbook](postgresql-storage-deployment-runbook.md)
- [SQLite small-roleplay proof](sqlite-small-roleplay-deployment-storage-proof.md)
- [PostgreSQL high-volume readiness gates](postgres-high-volume-readiness-gates.md)
- [Read-only admin diagnostics](read-only-admin-diagnostics-api.md)
- [Live deliverable certification](live-deliverable-certification.md)
