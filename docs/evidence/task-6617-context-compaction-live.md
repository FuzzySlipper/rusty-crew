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
- Evidence packet: `/home/system/rusty-crew-debug/evidence/task-6617/msej5byp/live-results.json`.

The certification created secret-free disposable provider and profile records.
The profiles were deleted and the providers were archived after the run.

## Successful Scenario

Profile/session: `task-6617-context-msej5byp` /
`task-6617-context-msej5byp-session`

- The certification now uses a 16,384-token provider context and runs up to 20
  provider-backed turns with one real terminal call per turn, so the pressure
  boundary is reached without making fixed prompt overhead dominate the target.
- The smoke records the first provider accounting snapshot after
  `context_compaction_completed`. It requires `provider/exact` provenance for
  both the prompt projection and current request, admission below the 60%
  threshold, and fewer prompt tokens than the pressured request.
- Latest successful packet: `/home/system/rusty-crew-debug/evidence/task-6617/msej5byp/live-results.json`.

The run crossed the boundary after 20 real terminal-backed turns: the
pressured request was 9,819 exact provider input tokens at 60%, and the first
accounting snapshot after `context_compaction_completed` was 9,113 exact
provider input tokens at 56% and near-threshold. The artifact estimate was 3,248
serialized-estimate tokens after compacting 27 items and retaining 11; that
estimate is reported as artifact metadata, not substituted for the next
provider request.

The service restarted before the continuity turn. Hydration retained the same
artifact, the reduced projection, and the durable transcript; the post-restart
turn recalled the pre-compaction fact, completed one real terminal call, and
triggered a second compaction artifact. Its authoritative provider snapshot was
9,406 exact input tokens at 58%, with 28 items compacted and 11 retained. The
separate failure scenario emitted `context_compaction_failed`, retained three
durable messages, and preserved the prior chat-completions projection.
- The post-compaction turn retained both the pre-compaction continuity fact and
  a new continuity marker.
- After restarting `rusty-crew-debug.service`, the same session ID and
  compaction artifact were present, the durable transcript count did not shrink,
  and a new terminal-backed continuity turn completed.

## Failure Scenario

Profile/session: `task-6617-context-failure-msej5byp` /
`task-6617-context-failure-msej5byp-session`

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
