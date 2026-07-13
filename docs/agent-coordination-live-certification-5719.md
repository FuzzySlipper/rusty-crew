# Agent Coordination Live Certification (#5719)

Date: 2026-07-12 (America/Los_Angeles)

The coordination capability has separate debug and production certifications.
Neither accepts a service URL, endpoint, port, or deployment selector. Both
create disposable direct-brain and managed Codex identities, use real model tool
calls, and hard-delete their profiles, sessions, bindings, and native threads.

## Debug

Command:

```bash
npm run smoke:coordination-live:debug -w @rusty-crew/capability-harness
```

Artifact root:

```text
/tmp/rusty-crew-coordination-debug-1783911007445
```

Run ID: `coordination-debug-1783911007445-86ee4c09`

The run certified:

- direct and managed Codex `list_agents` calls against the same-service Rust
  directory;
- direct brain to managed Codex, managed Codex to direct brain, and managed
  Codex to managed Codex correlated rounds;
- durable delivery, round, native thread, native turn, and tool-call IDs;
- duplicate delivery idempotency;
- a pending round across `rusty-crew-debug.service` restart;
- terminal `agent_round_timeout` expiry without resurrection;
- native-thread deletion and profile hard-delete cleanup.

## Production

Command:

```bash
npm run smoke:coordination-live:production -w @rusty-crew/capability-harness
```

Artifact root:

```text
/tmp/rusty-crew-coordination-production-1783911318127
```

Run ID: `coordination-production-1783911318127-253e7321`

The production run exercised the same two directory tools and three routing
directions. It asserted duplicate idempotency and terminal TTL expiry without
restarting `rusty-crew.service`. Cleanup waited for its own intentionally late
direct wake, deleted both disposable native threads and profiles, verified its
agent and binding IDs were absent, and verified every non-certification agent
and binding present before the run remained present afterward.

The assertions use durable identities, lifecycle/tool events, reply
correlation, terminal states, and cleanup state. They do not compare model prose.
