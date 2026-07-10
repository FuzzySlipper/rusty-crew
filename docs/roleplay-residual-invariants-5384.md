# Roleplay Residual Invariants

Task: `rusty-crew#5384`

## Proven Gap Closed

Generated alternatives previously used three independent durable operations:

1. save the alternate message variant;
2. select it as the slot's active variant;
3. advance the selected message's conversation branch head.

Manual selection used the final two operations. A failure between calls could
leave an orphan generated alternate, a selected slot with an old branch head,
or a branch head that did not match the active variant.

`apply_roleplay_alternative` now owns this invariant in one SQLite/PostgreSQL
transaction. It optionally creates an alternate with Rust-owned ordinal
allocation, checks the requested active-variant expectation, selects the
variant, derives the selected message and branch, and advances that branch head.
On conflict, a newly requested variant is not persisted.

The result is a typed receipt containing the created variant, selected slot,
updated branch, and any active-variant conflict. The roleplay route invokes this
single operation for generated alternatives and manual selection. Provider
generation remains outside the transaction and does not append a normal chat
assistant message.

## Boundaries Confirmed

- Lore query control normalization, paging truth, capture, promotion, layer
  mutation, and recall are already Rust-owned repository/domain operations.
- Scene-state read and merge planning are already Rust-owned. TypeScript retains
  HTTP body adaptation and tool/provider execution.
- Manual alternate text parsing and model generation remain TypeScript boundary
  work. Moving them would not add a deterministic invariant.

## Evidence

- `roleplay_alternative_creation_selection_and_branch_head_are_atomic` proves
  creation, selection, branch-head advancement, and rollback on conflict.
- Bridge contract, fingerprint, and native-surface checks cover the typed
  operation.
- Focused roleplay service smokes verify alternatives remain outside normal chat
  append behavior.
