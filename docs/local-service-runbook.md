# Local Service Runbook

Status: Local deployment runbook; updated for `/home/system/rusty-crew`

This runbook starts Rusty Crew from the source checkout at `/home/dev/rusty-crew`
while keeping mutable service state under `/home/system/rusty-crew`.

The local machine has two first-class service roots. See
`docs/local-service-topology.md` for the short operational map.

- live agent service: `/home/system/rusty-crew`, port `9347`, PostgreSQL;
- debug/test service: `/home/system/rusty-crew-debug`, port `9348`, SQLite.

Set `RUSTY_CREW_DEPLOYMENT_ROLE=production` for the live service and
`RUSTY_CREW_DEPLOYMENT_ROLE=debug` for the debug service. Operator messaging
uses the separately named clients documented in
`docs/agent-coordination-operator-clients.md`; there is no shared command with a
service URL or debug selector.

Use the debug service for smoke tests, Rusty View live certification, and
disposable LLM-backed experiments.

## Paths

- Source checkout: `/home/dev/rusty-crew`
- Runtime root: `/home/system/rusty-crew`
- Service env file: `/home/system/rusty-crew/config/service.env`
- Runtime config: `/home/system/rusty-crew/config/service.json`
- Engine data: `/home/system/rusty-crew/data/engine`
- Static frontend site: `/home/system/rusty-crew/site`
- Local lock: `/home/system/rusty-crew/run/service.lock`
- Systemd user unit source: `ops/systemd/rusty-crew.service`

Debug/test paths mirror the live layout under `/home/system/rusty-crew-debug`.
The debug unit is `rusty-crew-debug.service`, uses port `9348`, and stores its
SQLite database at
`/home/system/rusty-crew-debug/data/engine/coordination.sqlite3`.

## Codex App-Server Isolation

Live/CLI and debug Codex runtimes use separate app-server processes and
separate history roots. Do not point both Crew controllers at one app-server
socket, and do not give the debug process the live `/home/agent/.codex` home.

| Purpose | Unit | `CODEX_HOME` | Socket |
| --- | --- | --- | --- |
| live and normal CLI history | `codex-app-server-live.service` | `/home/agent/.codex` | `/run/user/1001/codex-app-server-live/app-server.sock` |
| debug and certification history | `codex-app-server.service` | `/home/system/rusty-crew-debug/codex-home` | `/run/user/1001/codex-app-server/app-server.sock` |

Frequent Codex CLI updates must be staged and certified through the debug-only
operator workflow before live promotion. See
[Codex debug update and certification](codex-debug-update-certification.md).

The routine update sequence is:

```bash
cd /home/dev/rusty-crew
npm run codex:debug:update-certify -- --update
npm run codex:live:promote -- --promote
```

When the executable was updated separately, replace `--update` with
`--skip-update`. The first command may restart only the debug app-server and
debug Crew. The second refuses active live work by default, snapshots exact
binding/thread identities, and restarts only the live app-server and live Crew.
Do not update source protocol version strings merely because `codex --version`
changed. See the [0.144.3 live certification](codex-app-server-0.144.3-live-certification.md)
for the first update completed under this contract.

Bootstrap the private debug home without copying live sessions, history, or
state databases:

```bash
install -d -m 700 /home/system/rusty-crew-debug/codex-home
install -m 600 /home/agent/.codex/auth.json \
  /home/system/rusty-crew-debug/codex-home/auth.json
install -m 600 /home/agent/.codex/config.toml \
  /home/system/rusty-crew-debug/codex-home/config.toml
ln -s /home/agent/.codex/skills \
  /home/system/rusty-crew-debug/codex-home/skills
ln -s /home/agent/.codex/plugins \
  /home/system/rusty-crew-debug/codex-home/plugins
ln -s /home/agent/.codex/rules \
  /home/system/rusty-crew-debug/codex-home/rules
```

The copied auth/config files are local secrets and must not be committed. The
linked capability directories are shared intentionally; native session and
rollout state is not. Install the repo-owned units with:

```bash
cp ops/systemd/codex-app-server.service \
  ~/.config/systemd/user/codex-app-server.service
cp ops/systemd/codex-app-server-live.service \
  ~/.config/systemd/user/codex-app-server-live.service
systemctl --user daemon-reload
systemctl --user enable --now \
  codex-app-server.service codex-app-server-live.service
```

After changing a Codex home or replacing an app-server process, restart the
corresponding Crew service so its single controller creates a fresh WebSocket
driver. Verify each runtime reports `observedState: ready`, and prove a newly
created debug thread cannot be read through the live runtime API.

## First Setup

From `/home/dev/rusty-crew`:

```bash
mkdir -p /home/system/rusty-crew/config
cp ops/systemd/service.env.example /home/system/rusty-crew/config/service.env
chmod 600 /home/system/rusty-crew/config/service.env
```

Edit `/home/system/rusty-crew/config/service.env` and set
`RUSTY_CREW_ADMIN_TOKEN` to a local token when using bearer auth.

The local deployment intentionally binds admin HTTP on the trusted LAN:

```text
RUSTY_CREW_ADMIN_HOST=0.0.0.0
RUSTY_CREW_ADMIN_PORT=9347
RUSTY_CREW_ADMIN_ALLOW_LAN=true
RUSTY_CREW_ADMIN_AUTH_MODE=bearer
RUSTY_CREW_SCHEDULER_TICK_INTERVAL_MS=1000
RUSTY_CREW_WAKE_DISPATCH_INTERVAL_MS=250
RUSTY_CREW_CHAT_COMPLETIONS_WORK_QUANTUM_TOOL_ROUNDS=64
RUSTY_CREW_OPENAI_RESPONSES_WORK_QUANTUM_CONTINUATION_ROUNDS=64
RUSTY_CREW_CHAT_COMPLETIONS_NO_PROGRESS_ATTENTION_THRESHOLD=3
RUSTY_CREW_OPENAI_RESPONSES_NO_PROGRESS_ATTENTION_THRESHOLD=3
```

The work-quantum values control how often a healthy turn durably yields. The
no-progress values control when equivalent failed or malformed work pauses for
operator attention. None caps successful work or substitutes for explicit
cancellation.

For trusted development on this machine/LAN, tokenless admin can be enabled
explicitly:

```text
RUSTY_CREW_ADMIN_AUTH_MODE=none
# RUSTY_CREW_ADMIN_TOKEN may be omitted
```

Do not use tokenless mode outside the trusted local field-test environment.

## Direct Run

Use direct execution before installing the user service:

```bash
cd /home/dev/rusty-crew
npm run build:native
npm run typecheck
npm run smoke:service-config
npm run smoke:service-host
set -a
. /home/system/rusty-crew/config/service.env
set +a
npm run service:start
```

`npm run build:native` writes generated runtime artifacts under
`ts/packages/native-bridge/native/`. Those files are expected local build output
and should not be committed except for the tracked declaration surface
`index.d.ts`; see `docs/native-bridge-artifact-strategy.md`.

Health is intentionally shallow and does not require auth:

```bash
curl http://127.0.0.1:9347/v1/admin/healthz
curl http://127.0.0.1:9348/v1/admin/healthz
```

The browser diagnostics panel is served from the same host:

```text
http://127.0.0.1:9347/admin
http://127.0.0.1:9348/admin
```

Enter the local admin token from
`/home/system/rusty-crew/config/service.env` when the page prompts for it. In
`RUSTY_CREW_ADMIN_AUTH_MODE=none`, the token box is hidden and the page reads
diagnostics directly.

## Reset And Recreate Local Test State

During the architecture-remediation window, local Rusty Crew state is disposable
test data. Prefer resetting current-shape state and recreating it through the
official APIs over preserving old `/home/agents/rusty-crew` data or adding
compatibility fallback reads.

Stop the service first:

```bash
systemctl --user stop rusty-crew.service
```

If using direct-run testing, stop the foreground process with `Ctrl-C`.

For SQLite test roots, archive or remove only stopped-service mutable state:

```bash
mv /home/system/rusty-crew/data \
  /home/system/rusty-crew/data.reset.$(date +%Y%m%d%H%M%S)
rm -rf /home/system/rusty-crew/run/*
```

For the shared local PostgreSQL service, reset the configured disposable schema
instead of trying to migrate scratch data. Source the local secret env only in
the shell where you run `psql`; do not copy the URL into docs or commits.

```bash
set -a
. /home/system/database/rusty-crew-postgres.env
. /home/system/rusty-crew/config/service.env
set +a
psql "$RUSTY_CREW_DATABASE_URL" \
  -v schema="${RUSTY_CREW_POSTGRES_SCHEMA:-rusty_crew}" \
  -c 'DROP SCHEMA IF EXISTS :"schema" CASCADE;' \
  -c 'CREATE SCHEMA :"schema";'
rm -rf /home/system/rusty-crew/run/*
```

Restart after the reset:

```bash
systemctl --user start rusty-crew.service
```

Recreate runtime state through API/UI paths:

- model providers: `POST /v1/admin/model-providers?refresh=apply`;
- local tool profiles: read/seed through `GET /v1/admin/local-tool-profiles`;
- profiles/sessions/brain registration:
  `POST /v1/admin/control/profiles`;
- MCP bindings: configure through the profile/control API path rather than
  hand-editing old service files;
- live test profile: follow `docs/live-test-profile-setup.md`.

After recreation, verify the service and Rusty View see current state:

```bash
curl http://127.0.0.1:9347/v1/admin/healthz
curl -H "authorization: Bearer $RUSTY_CREW_ADMIN_TOKEN" \
  http://127.0.0.1:9347/v1/admin/readyz
curl -H "authorization: Bearer $RUSTY_CREW_ADMIN_TOKEN" \
  http://127.0.0.1:9347/v1/admin/diagnostics
```

In `RUSTY_CREW_ADMIN_AUTH_MODE=none`, omit the authorization header.

## Static Frontend

Rusty Crew can serve a static frontend from the same origin as the service API.
When `/home/system/rusty-crew/site` exists, it is used as the default site
directory. `RUSTY_CREW_STATIC_DIR` can point at a different directory while
developing or testing a frontend build.

Deployment is intentionally file-copy simple:

```bash
mkdir -p /home/system/rusty-crew/site
cp -a /home/dev/rusty-view/dist/apps/debug-chat/browser/. /home/system/rusty-crew/site/
```

With a site directory present, `/` serves the frontend app and `/v1/*` remains
API-only. Unknown non-API paths fall back to `index.html` for client-side
routing. The built-in Rusty Crew diagnostics panel remains available at
`/admin`.

## Direct LLM Field Test

The first provider-backed service test should use the direct-debug CLI rather
than Den Channels or Telegram. It sends one message through the service HTTP API;
the service routes it into Rust, consumes the Rust `brain_wake_requested` event,
dispatches the registered brain, and returns the completion summary:

```bash
cd /home/dev/rusty-crew
npm run service:debug-turn -- field-prime-session "Reply with one sentence from the live Rusty Crew service."
```

When `RUSTY_CREW_ADMIN_AUTH_MODE=none`, no token is required. In bearer mode,
export `RUSTY_CREW_ADMIN_TOKEN` before running the CLI.

Profiles with centralized model-provider aliases use the selected brain module
and provider protocol from the session context. Direct OpenAI OAuth Responses
profiles should use an `openai-responses` brain with a provider alias whose
credential kind is `openai_oauth`; den-router is only an explicit proxy/provider
choice, not the default OpenAI OAuth path.

Useful model/brain readbacks:

```bash
curl http://127.0.0.1:9347/v1/chat/sessions/<session-id>/context
curl http://127.0.0.1:9347/v1/admin/model-providers/<alias>/oauth/openai/status
```

## Background Heartbeat

The service owns two lightweight TypeScript timers over the typed Rust bridge:

- `RUSTY_CREW_SCHEDULER_TICK_INTERVAL_MS` calls `runSchedulerTick` and explicit
  queue expiry maintenance. Set to `0` to disable.
- `RUSTY_CREW_WAKE_DISPATCH_INTERVAL_MS` drains Rust `brain_wake_requested`
  events and calls the registered brain runtime. Set to `0` to disable.

Diagnostics recent events record scheduler activity, skipped wakes, failed
wakes, and dispatched wakes.

Readiness and diagnostics require the local bearer token:

```bash
curl -H "authorization: Bearer $RUSTY_CREW_ADMIN_TOKEN" \
  http://127.0.0.1:9347/v1/admin/readyz

curl -H "authorization: Bearer $RUSTY_CREW_ADMIN_TOKEN" \
  http://127.0.0.1:9347/v1/admin/diagnostics
```

## User Service

Install the service for the current user:

```bash
mkdir -p ~/.config/systemd/user
cp /home/dev/rusty-crew/ops/systemd/rusty-crew.service \
  ~/.config/systemd/user/rusty-crew.service
systemctl --user daemon-reload
systemctl --user enable --now rusty-crew.service
```

Inspect:

```bash
systemctl --user status rusty-crew.service
journalctl --user -u rusty-crew.service -f
```

Logs stay in the user journal for the current source-run deployment. The repo
does not write or rotate separate service log files yet; journald retention is
the active policy for both live and debug services. The unit identifiers are
deliberately different:

```bash
# Live service, durable agent infrastructure on PostgreSQL.
journalctl --user -u rusty-crew.service --since today
journalctl --user -t rusty-crew-live -f

# Debug/test service, noisy SQLite-backed smoke target.
journalctl --user -u rusty-crew-debug.service --since -2h
journalctl --user -t rusty-crew-debug -f
```

Treat `rusty-crew.service` / `rusty-crew-live` as the operational source of
truth for live agent health. Debug logs are for smoke and reproduction traffic
only; do not use them to infer live-service health. The debug unit has a lower
per-unit journal burst limit than live so noisy test loops cannot dominate the
user journal. If future deployments need file logs for shipping or long
retention, add explicit logrotate assets then; until that exists,
`RUSTY_CREW_LOG_DIR` is reserved for future service-owned artifacts, not the
primary process log.

Restart:

```bash
systemctl --user restart rusty-crew.service
```

Stop:

```bash
systemctl --user stop rusty-crew.service
```

Disable and remove:

```bash
systemctl --user disable --now rusty-crew.service
rm -f ~/.config/systemd/user/rusty-crew.service
systemctl --user daemon-reload
```

## Runtime Config

`/home/system/rusty-crew/config/service.json` is required by the current
source-run service. For a new deployment, create the minimal empty runtime
graph below before the first start.

Minimal shape:

```json
{
  "profilesDir": "/home/system/rusty-crew/config/profiles",
  "wakeTimeout": {
    "mode": "disabled"
  },
  "brains": [{ "profileId": "prime" }],
  "sessions": [
    {
      "sessionId": "prime-session",
      "agentId": "prime",
      "profileId": "prime",
      "kind": "full"
    }
  ]
}
```

Profile files live at `${profilesDir}/${profileId}.json`.

`wakeTimeout` controls the service-side ceiling for one dispatched brain turn.
Use `{ "mode": "disabled" }` for no service-wide ceiling. To set a visible
service default, use `{ "mode": "default", "defaultMs": 600000 }`. Explicit
session `turnTimeoutMs`, profile `runtime.maxTurnDurationMs`, and profile
`sessionDefaults.turnTimeoutMs` override the service default.

### Create Profile API

Frontends and operators should create new profile identities through the
official control path instead of hand-editing `service.json` plumbing:

```bash
curl -X POST \
  -H "authorization: Bearer $RUSTY_CREW_ADMIN_TOKEN" \
  -H "content-type: application/json" \
  -d '{"profileId":"field-prime","displayName":"Field Prime"}' \
  http://127.0.0.1:9347/v1/admin/control/profiles
```

Only identity-bearing fields are required. The service defaults `agentId` to the
profile id, `sessionId` to `${agentId}-session`, `implementationId` to
`${profileId}-brain`, and creates a minimal profile JSON with a local
deterministic model and default MCP profile binding. The endpoint then updates
`service.json`, applies runtime config, registers the brain, and creates the
configured session without a service restart.

Optional request fields:

- `displayName`
- `agentId`
- `sessionId`
- `implementationId`
- `kind` (`full`, `worker`, or `delegated`; default `full`)
- `mcpToolProfile`
- `modelConfig`

Profile-authored content such as `soul.md`, memory files, profile-local skills,
and later editable profile settings should be managed by profile editor flows.
Channel joins are deliberately not implicit; use explicit channel
join/create/archive controls for that.

### Durable Sessions

Configured sessions are durable identities. A service restart must reuse the
same configured session ID rather than creating a replacement session. If
shutdown archived the configured session, startup reactivates that same session
after expiring pending queued messages that are past their TTL.

Creating a new session is an explicit operator action, such as a future `/new`
command or a typed create-session admin control. Do not add a new `sessions[]`
entry and rely on hot reload as an implicit `/new`.

### Hot Reload

Runtime config edits should normally be applied without restarting the service:

```bash
curl -X POST \
  -H "authorization: Bearer $RUSTY_CREW_ADMIN_TOKEN" \
  -H "content-type: application/json" \
  -d '{"reason":"operator edited service config"}' \
  http://127.0.0.1:9347/v1/admin/control/config/reload
```

In `RUSTY_CREW_ADMIN_AUTH_MODE=none`, omit the authorization header.

Reload rereads `service.json`, reconciles brain registrations, reuses or
reactivates already-known configured sessions, and rebuilds MCP surface
diagnostics from the configured MCP bindings. It reports configured sessions
that are missing instead of creating them implicitly. Use an explicit create
operation first when a truly new session is desired.

Reload and restart must not resurrect expired or terminal queued messages.
Pending queued messages are subject to the same body-policy TTL enforced by the
background heartbeat.

## Guardrails

- Do not run two Rusty Crew service processes against the same runtime root.
  Startup creates `/home/system/rusty-crew/run/service.lock` or
  `/home/system/rusty-crew-debug/run/service.lock` and fails if a live Rusty
  Crew process still owns the lock.
- Service startup clears stale lock files whose recorded PID is gone or no
  longer looks like a Rusty Crew service. The systemd templates run
  `npm run service:preflight` before `service:start` so hard-kill stale locks
  are handled before systemd enters a restart loop.
- The live and debug source-run systemd units use `Type=notify`,
  `NotifyAccess=all`, and `WatchdogSec=45s`. `service:start` reports ready with
  `systemd-notify` only after the HTTP host is listening, then sends watchdog
  heartbeats while `/v1/admin/healthz` responds healthy. `NotifyAccess=all` is
  intentional for the npm/tsx source-run shape; a later packaged binary unit can
  tighten this once the Node service process is the direct systemd main PID.
- If preflight still reports an existing lock, inspect the lock file before
  removing it. It records the pid and creation time.
- Do not copy only the SQLite main database file while the service is running.
  Backup/export should be quiesced or service-owned.
- Read-only admin routes must not trigger maintenance, queue delivery, or any
  runtime mutation.
- Restart and reload must reuse configured session IDs. New session IDs should
  come only from explicit create operations.
- Unsupported admin controls should return `unsupported_control` until a typed
  bridge/runtime API exists.

## Maintenance

Runtime database size is exposed through authenticated diagnostics:

```bash
curl -H "authorization: Bearer $RUSTY_CREW_ADMIN_TOKEN" \
  http://127.0.0.1:9347/v1/admin/diagnostics/persistence
```

Explicit maintenance is a guarded control route. It can expire/purge queue rows
when given timestamp cutoffs and can run SQLite optimize/WAL checkpoint work:

```bash
curl -X POST \
  -H "authorization: Bearer $RUSTY_CREW_ADMIN_TOKEN" \
  -H "content-type: application/json" \
  -d '{"runWalCheckpoint":true,"runOptimize":true,"reason":"manual local maintenance"}' \
  http://127.0.0.1:9347/v1/admin/control/maintenance
```

Do not rely on diagnostics reads to run maintenance. Reads must stay inert.

Warning signals that should trigger a PostgreSQL or retention design pass:

- WAL bytes keep growing faster than checkpoints can reduce them.
- Freelist bytes remain high after retention.
- Event/message tables grow faster than diagnostic pages remain usable.
- Multiple service processes need concurrent writes to the same runtime store.
- Agents/profiles require hard operational isolation.

## Backup

Repo-owned backup helpers live in `ops/scripts` and `ops/systemd`.

Live service (`/home/system/rusty-crew`) uses PostgreSQL. Run a manual dump with:

```bash
ops/scripts/rusty-crew-backup.sh \
  --root /home/system/rusty-crew \
  --backend postgres \
  --database-env /home/system/database/rusty-crew-postgres.env
```

Debug service (`/home/system/rusty-crew-debug`) uses SQLite. Run an online
SQLite backup with:

```bash
ops/scripts/rusty-crew-backup.sh \
  --root /home/system/rusty-crew-debug \
  --backend sqlite
```

The script writes the backup and a `.sha256` file under each service root's
`RUSTY_CREW_BACKUP_DIR`.

Install optional user timers:

```bash
mkdir -p ~/.config/systemd/user
cp ops/systemd/rusty-crew-backup.service \
  ops/systemd/rusty-crew-backup.timer \
  ops/systemd/rusty-crew-debug-backup.service \
  ops/systemd/rusty-crew-debug-backup.timer \
  ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now rusty-crew-backup.timer
systemctl --user enable --now rusty-crew-debug-backup.timer
```

Stopped-service root snapshots are still useful before major local cutovers:

```bash
systemctl --user stop rusty-crew.service
tar -C /home/system -czf /home/system/rusty-crew-field-test.tgz rusty-crew
```

If using direct-run testing, stop the foreground process first. Do not copy only
`coordination.sqlite3` while the service is active; SQLite WAL mode also uses
`coordination.sqlite3-wal` and `coordination.sqlite3-shm`.

## Rollback

For first field tests, rollback is simply stopping the process and leaving the
runtime root intact for inspection:

```bash
systemctl --user stop rusty-crew.service
```

If direct-run testing was used, stop the foreground process with `Ctrl-C`.

Archive the runtime root only after confirming the service is stopped:

```bash
tar -C /home/system -czf /home/system/rusty-crew-field-test.tgz rusty-crew
```
