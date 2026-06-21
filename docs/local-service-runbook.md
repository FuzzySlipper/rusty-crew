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
`RUSTY_CREW_ADMIN_TOKEN` to a local token.

The local deployment intentionally binds admin HTTP on the trusted LAN:

```text
RUSTY_CREW_ADMIN_HOST=0.0.0.0
RUSTY_CREW_ADMIN_PORT=9347
RUSTY_CREW_ADMIN_ALLOW_LAN=true
```

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
- Unsupported admin controls should return `unsupported_control` until a typed
  bridge/runtime API exists.

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
