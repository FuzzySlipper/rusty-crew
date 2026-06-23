# Local Service Runbook

Status: Initial local deployment runbook for task 3066

This runbook starts Rusty Crew from the source checkout at `/home/dev/rusty-crew`
while keeping mutable service state under `/home/agents/rusty-crew`.

## Paths

- Source checkout: `/home/dev/rusty-crew`
- Runtime root: `/home/agents/rusty-crew`
- Service env file: `/home/agents/rusty-crew/config/service.env`
- Runtime config: `/home/agents/rusty-crew/config/service.json`
- Engine data: `/home/agents/rusty-crew/data/engine`
- Static frontend site: `/home/agents/rusty-crew/site`
- Local lock: `/home/agents/rusty-crew/run/service.lock`
- Systemd user unit source: `ops/systemd/rusty-crew.service`

## First Setup

From `/home/dev/rusty-crew`:

```bash
mkdir -p /home/agents/rusty-crew/config
cp ops/systemd/service.env.example /home/agents/rusty-crew/config/service.env
chmod 600 /home/agents/rusty-crew/config/service.env
```

Edit `/home/agents/rusty-crew/config/service.env` and set
`RUSTY_CREW_ADMIN_TOKEN` to a local token when using bearer auth.

The local deployment intentionally binds admin HTTP on the trusted LAN:

```text
RUSTY_CREW_ADMIN_HOST=0.0.0.0
RUSTY_CREW_ADMIN_PORT=9347
RUSTY_CREW_ADMIN_ALLOW_LAN=true
RUSTY_CREW_ADMIN_AUTH_MODE=bearer
RUSTY_CREW_SCHEDULER_TICK_INTERVAL_MS=1000
RUSTY_CREW_WAKE_DISPATCH_INTERVAL_MS=250
```

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
. /home/agents/rusty-crew/config/service.env
set +a
npm run service:start
```

Health is intentionally shallow and does not require auth:

```bash
curl http://127.0.0.1:9347/v1/admin/healthz
```

The browser diagnostics panel is served from the same host:

```text
http://127.0.0.1:9347/admin
```

Enter the local admin token from
`/home/agents/rusty-crew/config/service.env` when the page prompts for it. In
`RUSTY_CREW_ADMIN_AUTH_MODE=none`, the token box is hidden and the page reads
diagnostics directly.

## Static Frontend

Rusty Crew can serve a static frontend from the same origin as the service API.
When `/home/agents/rusty-crew/site` exists, it is used as the default site
directory. `RUSTY_CREW_STATIC_DIR` can point at a different directory while
developing or testing a frontend build.

Deployment is intentionally file-copy simple:

```bash
mkdir -p /home/agents/rusty-crew/site
cp -a /home/dev/rusty-view/dist/apps/debug-chat/browser/. /home/agents/rusty-crew/site/
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

Profiles with `modelConfig.provider=den-router` use the local den-router-backed
Pi agent path. Profiles with other providers currently use the deterministic
local service brain.

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

`/home/agents/rusty-crew/config/service.json` is optional. If absent, the
service starts with an empty runtime shell.

Minimal shape:

```json
{
  "profilesDir": "/home/agents/rusty-crew/config/profiles",
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
  Startup creates `/home/agents/rusty-crew/run/service.lock` and fails if it is
  already present.
- If the process dies hard, inspect the lock file before removing it. It records
  the pid and creation time.
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

For now, prefer stopped-service backup for local field tests:

```bash
systemctl --user stop rusty-crew.service
tar -C /home/agents -czf /home/agents/rusty-crew-field-test.tgz rusty-crew
```

If using direct-run testing, stop the foreground process first. Do not copy only
`coordination.sqlite3` while the service is active; SQLite WAL mode also uses
`coordination.sqlite3-wal` and `coordination.sqlite3-shm`.

Future backup tooling should be service-owned or quiesced and should report the
same database size fields before and after export.

## Rollback

For first field tests, rollback is simply stopping the process and leaving the
runtime root intact for inspection:

```bash
systemctl --user stop rusty-crew.service
```

If direct-run testing was used, stop the foreground process with `Ctrl-C`.

Archive the runtime root only after confirming the service is stopped:

```bash
tar -C /home/agents -czf /home/agents/rusty-crew-field-test.tgz rusty-crew
```
