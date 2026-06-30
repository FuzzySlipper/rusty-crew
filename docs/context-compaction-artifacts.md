# Context Compaction Artifacts

Context compaction writes derived artifact records. It does not delete or
rewrite raw transcript storage.

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

## Readback

Rust persistence exposes:

- `save_context_compaction_artifact`
- `list_context_compaction_artifacts`

The chat `/context` diagnostics route reports the latest artifact metadata for
the session without returning `summary_text` by default. Full artifact readback
is available through the Rust/bridge persistence API for admin and debugging
surfaces.
