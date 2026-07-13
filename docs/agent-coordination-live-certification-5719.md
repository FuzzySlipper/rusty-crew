# Agent Coordination Live Certification (#5719)

Date: 2026-07-12 (America/Los_Angeles)

Durable evidence rerun: 2026-07-12 22:03-22:06 PDT

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
docs/evidence/agent-coordination-5719/debug
```

Run ID: `coordination-debug-1783918992360-62137418`

Artifact hashes:

```text
d9f62dbfac5472bad8d720f0ab5c040dae42dc4438af4966e963b9057920843f  debug-snapshot.json
f57980cc0523e8706c6a71c674b3117ed1aea6417e367febb5006dc584134121  evidence-packet.json
74c549f4809ffdeade5b34659c5475d4858ba6fd38cf350b6cbc0cfba3a70e17  scenario-summary.md
```

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
docs/evidence/agent-coordination-5719/production
```

Run ID: `coordination-production-1783919070111-51c8aae4`

Artifact hashes:

```text
77ecd90fd0304188d19682e09ab1a6f65215fd7f641d3b1b879ec442822be2a2  debug-snapshot.json
479ad5723c74244089c6fe9062d74bf4ec26b94614cc9c6cb2adfbf014ee4937  evidence-packet.json
dfde6cc28670d33aafb4819ecc9ce3baa27842c06ba901028eeb389cd2c141b7  scenario-summary.md
```

The production run exercised the same two directory tools and three routing
directions. It asserted duplicate idempotency and terminal TTL expiry without
restarting `rusty-crew.service`. Cleanup waited for its own intentionally late
direct wake, deleted both disposable native threads and profiles, verified its
agent and binding IDs were absent, and verified every non-certification agent
and binding present before the run remained present afterward.

The assertions use durable identities, lifecycle/tool events, reply
correlation, terminal states, and cleanup state. They do not compare model prose.

## Artifact Inspection

The committed packets and redacted snapshots were inspected after both runs.
For debug, `scenarioPassedByRuntime` is true for `direct-pi-agent` and
`codex-app-server`; the controller advanced from generation 37 to 38, the
pending round survived restart, and its terminal state is `expired`. For
production, both runtime results also pass; controller generation remained 12,
the pending round expired without a service restart, and cleanup preserved all
pre-existing agents and bindings. Each directory includes `sha256sums.txt` so a
reviewer can verify the three evidence files directly.
