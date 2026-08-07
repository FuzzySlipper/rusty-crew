# Context Compaction Artifacts

Context compaction writes derived artifact records. It does not delete or
rewrite raw transcript storage.

There are two artifact lifetimes. Mid-turn artifacts are embedded atomically in
the brain continuation checkpoint with the replacement model projection; see
[`mid-turn-context-compaction.md`](mid-turn-context-compaction.md). The records
described below are session-level derived artifacts for cross-turn readback and
future context strategies. They are not a competing mid-turn lifecycle store.

## Shape

`ContextCompactionArtifact` records:

- `artifact_id`: stable lowercase snake_case id for the derived artifact.
- `session_id` and optional `branch_id`: the source conversation scope.
- `strategy_id`: the context strategy that created the artifact, such as
  `rolling_summary_compaction`.
- `source_refs_json`: structured provenance for source message slots, variants,
  cursors, wake ids, branches, or other transcript references.
- `provider_metadata_json`: provider/model metadata used to generate the
  summary.
- `estimate_before_json` and optional `estimate_after_json`: token/context
  estimates around the compaction operation.
- `summary_text`: the derived summary content.
- `enters_future_context`: whether a strategy may project this artifact into
  future model context.
- `context_policy`: the projection policy selected by the strategy.
- `metadata_json`, `created_at`, and `updated_at`: implementation metadata and
  audit timestamps.

## Why Not Session Memory

Session memory is agent-facing memory. It can hold facts, summaries, or branch
notes that a memory policy selects for future context.

Compaction artifacts are lower-level evidence. They need provider metadata,
source transcript references, estimate before/after data, and strategy status.
Packing those fields into session memory would either leak implementation
details into model-facing memory or lose provenance needed for debugging.

A later strategy may choose to create session memory from an artifact, or select
an artifact directly for context. The artifact itself remains a durable derived
record beside the transcript, not a replacement for transcript history.

## Provenance and Intent Lineage (v63)

`ContextCompactionArtifact` now also records durable provenance beyond
`artifact_id` for restart hydration and audit:

- `strategy_revision`, `logical_turn_id`, `execution_epoch_id`,
  `source_projection_fingerprint`, `trigger`
  (`auto_threshold`|`manual_intent`|`provider_limit`|`retry`),
  `before_tokens`/`after_tokens`, `preserved_item_count`/`excised_item_count`,
  `intent_key` (idempotent intent for manual/auto), `terminal_status`
  (`completed`|`failed`) and `provider_chain_action`
  (`rebuild_replay_after_compaction`).

`intent_key` is the idempotent key for `session+projection+intent`:
`INSERT … ON CONFLICT(artifact_id) DO UPDATE` keeps last-writer-wins and is
safe under concurrent retry (same `artifact_id` never duplicates). Postgres
persists the full JSON record, SQLite persists typed columns via `v63`.

Mid-turn in-memory `BrainContextCompactionArtifact` mirrors this provenance
(`artifact_id`, `trigger`, `terminal_status`, before/after, preserved/excised)
and is validated alongside `sequence` monotonicity and reduction.

## Retention and Hydration

- Raw transcript/tool telemetry is never deleted by compaction; only the
  next model projection is smaller.
- `latest_only` + `session_id` returns the last `completed` artifact;
  a `failed` artifact preserves the prior valid projection and records a
  durable `terminal_status=failed` with `attention` (no destructive reset).
- Service restart hydrates the last valid provider projection + lineage from
  `context_compaction_artifacts` + `logical_brain_turn_checkpoints`
  (opaque `BrainContinuationPayload`). No new session is created and no
  transcript is silently discarded.
- `enters_future_context=false` artifacts are kept for audit but never
  projected.
- Retention: artifacts are retained for `external_event_retention_age_days`
  (14d) via the normal external event retention path; manual cleanup is
  `DELETE FROM context_compaction_artifacts WHERE session_id=?` or
  `POST /v1/admin/compaction-artifacts/cleanup` when exposed. No provider
  is required to be hydratable exactly — if a provider cannot rebuild a
  chain from the compacted projection, the artifact records `failed` and the
  prior projection remains usable (recorded as known limitation, not silent
  fallback).

## Readback

Rust persistence exposes:

- `save_context_compaction_artifact`
- `list_context_compaction_artifacts`

The chat `/context` diagnostics route reports the latest artifact metadata for
the session without returning `summary_text` by default. Full artifact readback
is available through the Rust/bridge persistence API for admin and debugging
surfaces.
