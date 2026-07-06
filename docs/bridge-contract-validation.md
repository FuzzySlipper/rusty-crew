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
npm run codegen:bridge-fingerprint
# Update ts/packages/contracts/src/index.ts bridgeWireShapeFingerprint
# to match crates/bridge/core-bridge-api/bridge-wire-shape-fingerprint.txt.
npm run smoke:bridge-fixture-drift
npm run smoke:bridge-fingerprint-drift
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

The native bridge loader also asserts `manifestVersion`, exact operation
inventory, and the checked-in wire-shape fingerprint when a co-located `.node`
binary is loaded. A stale committed binary now fails during
`loadNativeBridge()` with a `NativeBridgeContractError` instead of failing later
as a missing native function or stale payload shape.

## Wire-Shape Versioning Policy

The operation inventory catches stale binaries when an operation is added,
removed, or reordered. It does not catch a stale native addon when an existing
operation keeps the same name but changes a payload shape. The intended next
guard is a generated **wire-shape fingerprint**:

1. `core-bridge-codegen` emits a deterministic SHA-256 over the Rust-authored
   bridge validation fixture output plus the ordered manifest operation list.
2. The expected fingerprint is checked into the repo beside the bridge
   manifest/fixtures and exported through the TypeScript contracts package.
3. `core-bridge-api` includes the same checked-in fingerprint, and
   `core-bridge-node` exposes it through `NativeBridgeBinding`.
4. `loadNativeBridge()` asserts native `manifestVersion`, operation names, and
   wire-shape fingerprint before returning a usable bridge module.
5. `verify:offline` runs a fingerprint drift check so a Rust wire-shape change
   fails until fixtures and the checked-in fingerprint are regenerated.

The checked-in fingerprint file is:

```text
crates/bridge/core-bridge-api/bridge-wire-shape-fingerprint.txt
```

Regenerate it with:

```bash
npm run codegen:bridge-fixtures
npm run codegen:bridge-fingerprint
```

Then copy the generated value into the `bridgeWireShapeFingerprint` export in
`ts/packages/contracts/src/index.ts`, rebuild the native bridge, and run:

```bash
npm run smoke:bridge-fingerprint-drift
npm run smoke:bridge-validation
```

This fingerprint is a stale-binary guard, not a replacement for schema
coverage. It protects only the wire families represented in the Rust fixture
file. When a bridge family is not fixture-backed yet, a wire-shape change must
also extend fixture coverage or explicitly bump `MANIFEST_VERSION`.

`MANIFEST_VERSION` remains a coarse compatibility version. Bump it when a
change alters bridge semantics that are not captured by the fixture fingerprint,
including:

- buffer lease/ownership rules;
- error-channel shape or retryability semantics;
- operation direction or lifecycle ordering;
- native loader compatibility assumptions;
- any intentionally breaking change to an uncovered bridge family.

`schemars` remains the preferred future path for full JSON Schema generation,
but this fixture scaffold avoids forcing schema derives through every nested
protocol type before the checker workflow is proven.

## Current Coverage

Checker-backed Rust fixtures currently cover:

| Family | Operations / Shape | TS Validation Surface |
| --- | --- | --- |
| Wake/session/action | `project_body_state`, `list_sessions`, Responses-style brain wake stream result | `rawBodyStateSchema`, `rawSessionStateArraySchema`, `rawOpenAiResponsesBrainRunResultSchema` |
| Profile/model admin | profile registry records and model provider records returned by admin bridge methods | `rawProfileRegistryRecordSchema`, `rawModelProviderRecordSchema` |
| Memory/governance | memory-space descriptors, memory proposal records, and governance decision records | `rawMemorySpaceDescriptorSchema`, `rawMemoryProposalRecordSchema`, `rawMemoryGovernanceDecisionRecordSchema` |

Not yet fixture-backed:

- conversation/message variant records and branch/snapshot/jump records;
- attachment and data-bank records;
- scheduler/runtime diagnostics and maintenance reports;
- roleplay lore records and recall traces;
- runtime search/counter records;
- session activity digests and context-compaction artifact records.

Until those families are covered, bump `MANIFEST_VERSION` for any breaking or
renaming change to their Rust/TS wire keys, enum tags, required fields, or
return envelope semantics. Add fixture coverage instead of relying on a version
bump when the family becomes active across a user-facing UI or live agent path.

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
5. After the wire-shape fingerprint guard lands, regenerate the fingerprint and
   commit it with the fixture change.
6. Add or extend the matching TypeBox schema in
   `ts/packages/native-bridge/src/bridge-validation-schemas.ts`.
7. Validate the fixture in
   `ts/packages/native-bridge/src/smoke-bridge-validation.ts`.
8. Wrap the native bridge parse/cast chokepoint with `validateBridgeValue`.
9. Run `npm run smoke:bridge-contract-parity`,
   `npm run smoke:bridge-native-surface`, `npm run smoke:bridge-fixture-drift`,
   `npm run smoke:bridge-validation`, and the relevant package typecheck/smoke.
