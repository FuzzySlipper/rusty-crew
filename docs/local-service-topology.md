# Local Service Topology

The local Rusty Crew machine runs two services on purpose.

## Live Service

- Unit: `rusty-crew.service`
- Root: `/home/system/rusty-crew`
- Port: `9347`
- Storage: PostgreSQL, schema `rusty_crew`
- Purpose: long-lived local agents, channel participation, and project use

Do not use the live service for noisy smoke tests or disposable live
certification profiles unless the test explicitly needs the production-like
PostgreSQL instance.

## Debug Service

- Unit: `rusty-crew-debug.service`
- Root: `/home/system/rusty-crew-debug`
- Port: `9348`
- Storage: SQLite
- SQLite database:
  `/home/system/rusty-crew-debug/data/engine/coordination.sqlite3`
- Purpose: smoke tests, Rusty View live certification, roleplay quality spikes,
  and disposable frontend/LLM experiments

The debug service is intentionally resettable. Test profiles, temporary chat
sessions, and synthetic providers should land here so the live service can stay
usable as an actual agent runtime.

## Default Test Target

Use this for live tests and local debug clients:

```bash
export RUSTY_CREW_DEBUG_ADMIN_BASE_URL=http://127.0.0.1:9348
export RUSTY_CREW_ADMIN_BASE_URL="$RUSTY_CREW_DEBUG_ADMIN_BASE_URL"
```

For Rusty View live certification:

```bash
export RV_LIVE_BACKEND_URL=http://127.0.0.1:9348
```

The `tester-chat` provider is seeded on the debug service and points at local
den-router:

```text
alias: tester-chat
protocol: chat_completions
providerKind: den-router
modelId: deepseek-flash
baseUrl: http://127.0.0.1:18082/v1
```

Production pi-agent hosts always use the live Rust provider path. A silent
provider request fails visibly after 30 seconds by default; this is an HTTP
stream-idle ceiling, not a whole-turn or tool-loop ceiling:

```text
RUSTY_CREW_PI_AGENT_STREAM_IDLE_TIMEOUT_MS=30000
```

Deterministic provider clients are available only through explicit smoke/test
support and cannot be selected by deployed profiles.

## Health Checks

```bash
curl -fsS http://127.0.0.1:9347/v1/admin/diagnostics/storage | jq '.data.backend'
curl -fsS http://127.0.0.1:9348/v1/admin/diagnostics/storage | jq '.data.backend'
```

Expected results:

- live: `postgres`
- debug: `sqlite`
