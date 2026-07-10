# Roleplay Session Projection Atomicity

Task: `rusty-crew#5380`

## Old Multi-Call Path

The browser roleplay routes accepted a Rust lifecycle or metadata plan and then
applied its durable projection through two independent bridge calls:

1. `setChatLayers(...)` replaced the complete chat-layer binding set.
2. `putRoleplaySessionMetadata(...)` wrote the typed session metadata revision.

Create and fork performed the calls in metadata-then-layers order. Metadata
update performed them in layers-then-metadata order. Either order admitted a
partial durable state when layer validation, persistence, or revision checking
failed on the second call.

## Rust-Owned Boundary

`apply_roleplay_session_projection` now accepts one
`RoleplaySessionProjectionWrite` and commits the metadata revision and optional
complete chat-layer replacement in one backend transaction. The operation:

- requires `chat_layers.chat_id` to match `metadata.record.session_id`;
- validates every referenced lore layer before commit;
- returns the committed metadata revision and projected layer records;
- rejects stale metadata revisions with `ActionRejected`;
- rolls back both record families when either write fails.

SQLite and PostgreSQL implement the same repository contract. TypeScript calls
the operation once for create, update, archive, restore, and fork projection
application.

## Deliberate Scope

This transaction owns the roleplay session projection, not the whole runtime
lifecycle or transcript fork. Session-registry mutation and transcript branch,
slot, and variant copying remain separate domains. Claiming those as atomic
would require a broader engine operation and backend transaction spanning the
core session repository and conversation tree; `#5384` owns the proven
conversation/variant selection residue. This task does not add a fallback or
claim that sequential transcript copying is atomic.

## Evidence

- SQLite rollback and stale-revision coverage:
  `roleplay_session_projection_rolls_back_metadata_when_layers_fail`.
- PostgreSQL live rollback coverage:
  `postgres_typed_roleplay_records_match_revision_contract`.
- Bridge contract parity and native-surface checks cover the single operation.
