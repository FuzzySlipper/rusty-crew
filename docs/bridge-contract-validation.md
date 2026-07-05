# Bridge Contract Validation

Rusty Crew is moving bridge contract checks in three steps:

1. Manifest/Rust/TypeScript operation-inventory parity.
2. Generated napi `*Json` method surface coverage against the manifest.
3. TypeScript runtime validation at native bridge chokepoints.
4. Rust-authored wire fixtures emitted by `core-bridge-codegen`.
5. Later generated schemas/bindings for full operation families.

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
npm run smoke:bridge-contract-parity
npm run codegen:bridge-fixtures
npm run smoke:bridge-fixture-drift
npm run smoke:bridge-validation
```

`smoke:bridge-contract-parity` compares:

- `crates/bridge/core-bridge-api/bridge-manifest.toml` `[[operation]]`
  names;
- `core_bridge_api::OPERATION_NAMES`;
- `ts/packages/contracts/src/index.ts` `manifestOperationNames`.

The manifest may stay grouped for readability, but it must contain the exact
same operation set as Rust. The TypeScript contract list must match Rust in
both membership and order, and `@rusty-crew/native-bridge` imports that list
instead of keeping a fourth operation-name mirror.

`smoke:bridge-native-surface` parses the generated napi declaration file at
`ts/packages/native-bridge/native/index.d.ts`, derives operation names from
`NativeBridgeBinding` methods ending in `Json`, and fails when any such method
is missing from the manifest. This catches drift where a Rust `#[napi]`
JSON-string operation is exported without being inventoried in the bridge
contract.

`smoke:bridge-fixture-drift` compares the checked-in file with fresh Rust
serialization output. `smoke:bridge-validation` validates those Rust fixtures
against the TypeBox bridge schemas and asserts that each object key present in
a Rust fixture is explicitly declared by the matching TypeBox schema. Together
they provide the first CI-capable drift guard while the full generator matures:
adding a field to a covered Rust protocol shape fails until the TS schema is
updated, even when runtime validation remains permissive for forward
compatibility.

The native bridge loader also asserts `manifestVersion` and exact operation
inventory when a co-located `.node` binary is loaded. A stale committed binary
now fails during `loadNativeBridge()` with a `NativeBridgeContractError`
instead of failing later as a missing native function.

`schemars` remains the preferred future path for full JSON Schema generation,
but this fixture scaffold avoids forcing schema derives through every nested
protocol type before the checker workflow is proven.

## Current Coverage

Checker-backed Rust fixtures currently cover:

| Family | Operations / Shape | TS Validation Surface |
| --- | --- | --- |
| Wake/session/action | `project_body_state`, `list_sessions`, Responses-style brain wake stream result | `rawBodyStateSchema`, `rawSessionStateArraySchema`, `rawOpenAiResponsesBrainRunResultSchema` |
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

1. Add or update the operation in
   `crates/bridge/core-bridge-api/bridge-manifest.toml`,
   `core_bridge_api::OPERATION_NAMES`, and
   `ts/packages/contracts/src/index.ts` `manifestOperationNames`.
2. Run `npm run smoke:bridge-contract-parity` and
   `npm run smoke:bridge-native-surface` before touching runtime code.
3. Add a Rust fixture in `crates/bridge/core-bridge-codegen/src/main.rs` using
   the Rust protocol type or a small wrapper struct when the bridge returns an
   envelope.
4. Regenerate fixtures with `npm run codegen:bridge-fixtures`.
5. Add or extend the matching TypeBox schema in
   `ts/packages/native-bridge/src/bridge-validation-schemas.ts`.
6. Validate the fixture in
   `ts/packages/native-bridge/src/smoke-bridge-validation.ts`.
7. Wrap the native bridge parse/cast chokepoint with `validateBridgeValue`.
8. Run `npm run smoke:bridge-contract-parity`,
   `npm run smoke:bridge-native-surface`, `npm run smoke:bridge-fixture-drift`,
   `npm run smoke:bridge-validation`, and the relevant package typecheck/smoke.
