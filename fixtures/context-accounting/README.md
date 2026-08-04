# Context Accounting Fixtures

These fixtures are deterministic, non-live evidence for the versioned Rust
`ContextAccountingSnapshot` contract. They deliberately include both known and
unknown measurements so tests cannot turn missing provider data into zero.

- `chat-completions-provider.json` covers a provider-reported request with
  system, history, tool-schema, and reasoning segments.
- `responses-chain.json` covers Responses replay lineage and logical-wake
  aggregate usage.
- `unavailable.json` covers a provider that has not produced an accounting
  projection yet.
- `compaction-matrix.json` is the checked-in executable coverage catalog for
  provider projection variants, admission and compaction decisions, duplicate
  and failed recovery, restart hydration, stream ordering, and schema ratchets.
  Each case carries concrete deterministic input/expected values and `testRefs`
  to the Rust regression that exercises it. `check:context-accounting-fixtures`
  verifies that every catalog reference still resolves, while the Rust fixture
  test executes the snapshot, policy, lineage, ordering, and diagnostic cases.
- `schema-fingerprint.json` is the field-level ratchet for the Rust snapshot
  mapper. A removed or renamed field must be handled as an intentional
  contract change rather than silently disappearing from fixtures.

The matrix uses explicit `sourceQuality` labels. Provider-reported values are
`provider/exact`; tokenizer and serialized estimates remain approximate; and
unknown values are `unavailable/unavailable`. Tests reject contradictory pairs
at the Rust contract boundary.

The fixture suite must not point at a running live or debug service. Real
provider and debug-service evidence belongs to the separate live certification
task.
