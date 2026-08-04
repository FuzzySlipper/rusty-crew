# Task 6617 Context Compaction Live Certification

Date: 2026-08-04

This certification covers context accounting, automatic compaction, restart
hydration, durable transcript continuity, and compaction failure preservation
against the isolated debug service. It is intentionally separate from the live
PostgreSQL service.

## Certification Contract

The live smoke must be run against the exact source revision being reviewed.
The service exposes `RUSTY_CREW_SOURCE_REVISION` through
`GET /v1/admin/healthz`; the smoke fails before creating test data, and again
after restart, unless that value equals `RUSTY_CREW_CONTEXT_CERT_SOURCE_SHA`.
This makes the live packet attributable to the reviewed source rather than to a
possibly stale deployment.

Each run writes a durable packet to:

`/home/system/rusty-crew-debug/evidence/task-6617/<run>/live-results.json`

The packet records `service.sourceRevision`, profile/session IDs, the
compaction artifact ID, complete event references for both scenarios, and
durable transcript counts at the first post-compaction snapshot, pre-restart,
post-restart-hydration, and post-continuation checkpoints. The smoke requires
the pre-restart count to retain the first post-compaction count, then requires
each later durable count to be greater than or equal to the preceding count.
It also requires the new post-restart user message to survive into the next
provider-state write.

## Service

- Base URL: `http://127.0.0.1:9348`
- Unit: `rusty-crew-debug.service`
- Storage: `/home/system/rusty-crew-debug/data/engine/coordination.sqlite3`
- Provider route: local den-router `http://127.0.0.1:18082/v1`
- Model: `deepseek-flash`

The certification creates secret-free disposable provider and profile records.
The profiles are deleted and the providers are archived after the run.

## Scenarios

The successful scenario uses a 16,384-token provider context and real
provider-backed turns with one real terminal call per turn. It crosses the
compaction boundary, verifies exact provider accounting, restarts the debug
service, hydrates the same session and artifact, and completes a
terminal-backed continuity turn. The durable transcript is kept separately
from the compacted provider projection.

The failure scenario uses an undersized context where no completed historical
exchange can be compacted without touching frozen request/tool context. It
requires `context_compaction_started` followed by
`context_compaction_failed`, `logical_turn_attention_required`, retention of
the durable transcript, and preservation of the prior
`chat_completions_messages` provider projection.

## Verification Commands

Deterministic and persistence gates:

```bash
cargo test -p rusty-crew-chat-completions-brain
cargo test -p rusty-crew-openai-responses-brain
cargo test -p rusty-crew-core-persistence --features postgres postgres_context_compaction_artifact_persists_and_reloads -- --ignored
npm run smoke:rusty-view-chat-context -w @rusty-crew/brain-island
```

Focused durable-transcript regression:

```bash
cargo test -p rusty-crew-chat-completions-brain completed_wake_provider_state_preserves_compaction_for_next_wake
```

Exact-head live certification:

```bash
RUSTY_CREW_CONTEXT_CERT_SOURCE_SHA=<exact-head-sha> \
  npm run smoke:context-compaction-live-debug-service -w @rusty-crew/brain-island
```

The live smoke is debug-service-only by construction: it rejects any base URL
other than port `9348` and any unit other than `rusty-crew-debug.service`.
