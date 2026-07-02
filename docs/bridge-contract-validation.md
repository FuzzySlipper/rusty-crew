# Bridge Contract Validation

Rusty Crew is moving bridge contract checks in three steps:

1. TypeScript runtime validation at native bridge chokepoints.
2. Rust-authored wire fixtures emitted by `core-bridge-codegen`.
3. Later generated schemas/bindings for full operation families.

The active incremental source is:

```bash
cargo run -p rusty-crew-core-bridge-codegen -- emit-fixtures
```

The checked-in fixture file lives at:

```text
ts/packages/native-bridge/bridge-validation-rust-fixtures.json
```

To update it after intentional protocol shape changes:

```bash
npm run codegen:bridge-fixtures
npm run smoke:bridge-fixture-drift
npm run smoke:bridge-validation
```

`smoke:bridge-fixture-drift` compares the checked-in file with fresh Rust
serialization output. `smoke:bridge-validation` validates those Rust fixtures
against the TypeBox bridge schemas. Together they provide the first CI-capable
drift guard while the full generator matures.

`schemars` remains the preferred future path for full JSON Schema generation,
but this fixture scaffold avoids forcing schema derives through every nested
protocol type before the checker workflow is proven.

## Current Coverage

Checker-backed Rust fixtures currently cover:

| Family | Operations / Shape | TS Validation Surface |
| --- | --- | --- |
| Wake/session/action | `project_body_state_json`, `list_sessions`, Responses-style brain wake stream result | `rawBodyStateSchema`, `rawSessionStateArraySchema`, `rawOpenAiResponsesBrainRunResultSchema` |
| Profile/model admin | profile registry records and model provider records returned by admin bridge methods | `rawProfileRegistryRecordSchema`, `rawModelProviderRecordSchema` |

Runtime validation currently wraps:

- `wakeBrain`
- `submitBrainEvent`
- `submitBrainActions`
- `listSessions`
- `buildBrainWakeRequest` / `buildBrainWakeRequestForSession`
- diagnostic body-state projection and diagnostic action submission
- provider-state diagnostics
- profile registry create/update/list/get record reads
- model provider upsert/list/get record reads
- OpenAI Responses brain run input/result

## Adding A Bridge Family

1. Add a Rust fixture in `crates/bridge/core-bridge-codegen/src/main.rs` using
   the Rust protocol type or a small wrapper struct when the bridge returns an
   envelope.
2. Regenerate fixtures with `npm run codegen:bridge-fixtures`.
3. Add or extend the matching TypeBox schema in
   `ts/packages/native-bridge/src/bridge-validation-schemas.ts`.
4. Validate the fixture in
   `ts/packages/native-bridge/src/smoke-bridge-validation.ts`.
5. Wrap the native bridge parse/cast chokepoint with `validateBridgeValue`.
6. Run `npm run smoke:bridge-fixture-drift`, `npm run smoke:bridge-validation`,
   and the relevant package typecheck/smoke.
