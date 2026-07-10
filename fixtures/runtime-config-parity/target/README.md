# Target Runtime Graph Fixtures

These fixtures describe the clean-break endpoint for Den task #5362. They are
intentionally ahead of the current runtime planner until #5373 lands.

- `complete-source.camel.json` is decoded input before graph defaults and
  profile-derived records.
- `complete-plan.camel.json` is the deterministic Rust-owned output.
- `invalid-source.camel.json` lists stable diagnostic codes required from an
  invalid source.

Do not make TypeScript pre-expand the source to satisfy the expected plan.
