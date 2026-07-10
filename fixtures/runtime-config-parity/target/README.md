# Target Runtime Graph Fixtures

These fixtures describe the clean-break endpoint for Den task #5362.
`core-config::plan_runtime_graph` consumes them directly in its #5373 tests.
The service does not use that planner until the bridge and TS adoption slices.

- `complete-source.camel.json` is decoded input before graph defaults and
  profile-derived records.
- `complete-plan.camel.json` is the deterministic Rust-owned output.
- `invalid-source.camel.json` lists stable diagnostic codes required from an
  invalid source.

Do not make TypeScript pre-expand the source to satisfy the expected plan.

The `*.snake.json` files are generated Rust-wire companions:

```sh
npm run codegen:runtime-config-target-fixtures
npm run check:runtime-config-target-fixtures
```
