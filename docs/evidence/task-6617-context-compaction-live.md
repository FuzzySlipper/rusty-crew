# Task 6617 Context Compaction Live Certification

Date: 2026-08-04

This packet certifies context accounting, automatic compaction, restart
hydration, and compaction failure preservation against the isolated debug
service. It is intentionally separate from the live PostgreSQL service.

## Service

- Base URL: `http://127.0.0.1:9348`
- Unit: `rusty-crew-debug.service`
- Storage: `/home/system/rusty-crew-debug/data/engine/coordination.sqlite3`
- Provider route: local den-router `http://127.0.0.1:18082/v1`
- Model: `deepseek-flash`
- Evidence packet: `/home/system/rusty-crew-debug/evidence/task-6617/msee1ud5/live-results.json`

The certification created secret-free disposable provider and profile records.
The profiles were deleted and the providers were archived after the run.

## Successful Scenario

Profile/session: `task-6617-context-msee1ud5` /
`task-6617-context-msee1ud5-session`

- 12 provider-backed turns completed.
- 12 real terminal tool calls completed successfully.
- Pre-pressure snapshot: 46% fill.
- Pressure boundary: 60% fill, recorded in the durable
  `context_compaction_started` event with provider usage accounting.
- Compaction artifact: `chat-completions:compaction:1`.
- Compaction reduced the provider projection from 9,762 exact input tokens to
  approximately 3,256 serialized-estimate tokens, retaining 11 items after
  compacting 27.
- The post-compaction turn retained both the pre-compaction continuity fact and
  a new continuity marker.
- After restarting `rusty-crew-debug.service`, the same session ID and
  compaction artifact were present, the durable transcript count did not shrink,
  and a new terminal-backed continuity turn completed.

## Failure Scenario

Profile/session: `task-6617-context-failure-msee1ud5` /
`task-6617-context-failure-msee1ud5-session`

The deliberately undersized 12,288-token profile emitted
`context_compaction_started` followed by `context_compaction_failed` when no
completed historical exchange could be compacted without touching the frozen
request/tool context. The service surfaced `logical_turn_attention_required`
and retained a three-message durable transcript plus the prior
`chat_completions_messages` provider projection. No silent success or data loss
was claimed.

## Verification Commands

Deterministic and persistence gates:

```bash
cargo test -p rusty-crew-chat-completions-brain
cargo test -p rusty-crew-openai-responses-brain
cargo test -p rusty-crew-core-persistence --features postgres postgres_context_compaction_artifact_persists_and_reloads -- --ignored
npm run smoke:rusty-view-chat-context -w @rusty-crew/brain-island
```

Focused persistence regression:

```bash
cargo test -p rusty-crew-chat-completions-brain completed_wake_provider_state_preserves_compaction_for_next_wake
```

Live certification:

```bash
npm run smoke:context-compaction-live-debug-service -w @rusty-crew/brain-island
```

The live smoke is debug-service-only by construction: it rejects any base URL
other than port `9348` and any unit other than `rusty-crew-debug.service`.
