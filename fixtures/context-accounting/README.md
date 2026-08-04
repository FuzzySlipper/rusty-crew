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

The fixture suite must not point at a running live or debug service. Real
provider and debug-service evidence belongs to the separate live certification
task.
