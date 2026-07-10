# Bridge Contract Validation

Rusty Crew is moving bridge contract checks in three steps:

1. Manifest/Rust/TypeScript operation-inventory parity.
2. Generated napi `*Json` method surface coverage against the manifest.
3. TypeScript runtime validation at native bridge chokepoints.
4. Rust-authored wire fixtures emitted by `core-bridge-codegen`.
5. Rust-emitted helper modules for operation families as their shapes stabilize.
6. Later generated schemas/bindings for full operation families.

The active incremental source is:

```bash
cargo run -p rusty-crew-core-bridge-codegen -- emit-fixtures
```

The native bridge `core-config` request facade is also Rust-emitted. It keeps
runtime-config validation and create-profile request serialization out of the
hand-authored native bridge mapping layer:

```bash
npm run codegen:core-config-facade
npm run smoke:core-config-facade-drift
```

The generated file lives at:

```text
ts/packages/native-bridge/src/generated/core-config-facade.ts
```

Do not edit that file manually. If `crates/core/core-config` changes one of the
covered request shapes, regenerate the facade and keep the drift smoke green.

## Generated Neutral Protocol Contracts

Selected Rust-owned protocol families now derive `schemars::JsonSchema` and
generate the public TypeScript declarations consumed through
`@rusty-crew/contracts`:

```text
ts/packages/contracts/src/generated/core-protocol.ts
ts/packages/contracts/src/generated/core-protocol.schema.json
```

The selected closure covers session/resource/delegation records, brain
wake/event/action/provider-state records, core events and errors, and the
memory-space/proposal/activity/compaction families. Memory records preserve
their direct Rust `snake_case` JSON names; native-bridge DTOs use the existing
camel-case ergonomic projection. Branded identifiers remain a thin TypeScript
typing layer over the generated wire primitives.

Regenerate and check them with:

```bash
npm run codegen:protocol-contracts
npm run check:protocol-contracts
```

The check regenerates in memory and compares both committed artifacts exactly.
Unsupported schema constructs fail generation rather than emitting `any`.
Adapter-composed channel/MCP views and bridge-only scheduler presentation
types remain authored because they are not direct `core-protocol` serde DTOs.
Their Rust config and persistence inputs continue to be covered by the native
mapping and fixture inventories.

The checked-in fixture file lives at:

```text
ts/packages/native-bridge/bridge-validation-rust-fixtures.json
```

To update it after intentional protocol shape changes:

```bash
npm run smoke:bridge-contract-parity
npm run codegen:bridge-contracts
npm run check:bridge-contracts
npm run smoke:bridge-fixture-drift
npm run smoke:bridge-fingerprint-drift
npm run smoke:bridge-validation
```

`smoke:bridge-contract-parity` compares:

- `crates/bridge/core-bridge-api/bridge-manifest.toml` `[[operation]]`
  names;
- the build-generated `core_bridge_api::OPERATION_NAMES`;
- `ts/packages/contracts/src/generated/bridge-manifest.ts`.

The manifest stays grouped for readability and is the canonical ordered source.
`core-bridge-api/build.rs` generates the Rust operation slice at compile time,
and `core-bridge-codegen` emits the committed TypeScript operation metadata,
name union, ordered runtime list, manifest version, and wire fingerprint.
`@rusty-crew/native-bridge` imports that generated artifact through the
contracts package; there is no separately authored Rust or TypeScript list.

`smoke:bridge-native-surface` parses the generated napi declaration file at
`ts/packages/native-bridge/native/index.d.ts`, derives operation names from
`NativeBridgeBinding` methods ending in `Json`, and fails when any such method
is missing from the manifest. This catches drift where a Rust `#[napi]`
JSON-string operation is exported without being inventoried in the bridge
contract.

`smoke:bridge-fixture-drift` compares the checked-in file with fresh Rust
serialization output. `smoke:bridge-validation` validates those Rust fixtures
against the TypeBox bridge schemas, asserts that each object key present in a
Rust fixture is explicitly declared by the matching TypeBox schema, and enforces
the bridge coverage ratchet in
`ts/packages/native-bridge/src/bridge-validation-coverage.ts`.

The ratchet currently pins:

- manifest operations: 192;
- exported TypeBox bridge schemas: 41;
- Rust fixture families: 11;
- manifest operations with TypeBox runtime validation and/or Rust fixtures: 34;
- explicit operation exemptions: 158.

Together these provide the CI-capable drift guard while the full generator
matures: adding a field to a covered Rust protocol shape fails until the TS
schema is updated, and adding a bridge operation fails until it is covered by
validation/fixtures or deliberately added to the exact exemption list with a
rationale.

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
npm run codegen:bridge-contracts
```

That command updates the fixtures, fingerprint text, and generated contracts
module together in dependency order. Then rebuild the native bridge and run:

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

`schemars` is now the active path for selected neutral protocol families. The
fixture scaffold remains necessary for bridge projections that reshape Rust
records or combine types owned by more than one Rust crate.

## Current Coverage

The current counts above are intentionally asserted in code, not only in this
document. To update them after intentional bridge work:

1. Add or adjust TypeBox schemas and/or Rust fixtures when possible.
2. If a family remains uncovered, add an exact operation exemption in
   `bridge-validation-coverage.ts` with a short reason and preserve the
   `MANIFEST_VERSION` bump rule for breaking changes.
3. Regenerate fixtures/fingerprint when Rust fixture shapes change.
4. Run the bridge gate:

```bash
npm run smoke:bridge-contract-parity
npm run smoke:bridge-native-surface
npm run smoke:bridge-fixture-drift
npm run smoke:bridge-fingerprint-drift
npm run smoke:bridge-validation
```

Checker-backed Rust fixtures currently cover:

| Family | Operations / Shape | TS Validation Surface |
| --- | --- | --- |
| Wake/session/action | `project_body_state`, `list_sessions`, Responses-style brain wake stream result | `rawBodyStateSchema`, `rawSessionStateArraySchema`, `rawOpenAiResponsesBrainRunResultSchema` |
| Profile/model admin | profile registry records and model provider records returned by admin bridge methods | `rawProfileRegistryRecordSchema`, `rawModelProviderRecordSchema` |
| Memory/governance | memory-space descriptors, memory proposal records, and governance decision records | `rawMemorySpaceDescriptorSchema`, `rawMemoryProposalRecordSchema`, `rawMemoryGovernanceDecisionRecordSchema` |
| Memory activity/context | session activity digest records and context-compaction artifact records | `rawSessionActivityDigestSchema`, `rawContextCompactionArtifactSchema` |

Not yet fixture-backed:

- conversation/message variant records and branch/snapshot/jump records;
- attachment and data-bank records;
- scheduler/runtime diagnostics and maintenance reports;
- roleplay lore records and recall traces;
- runtime search/counter records;

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
- session activity digest save/list inputs and outputs
- context-compaction artifact save/list inputs and outputs

Bridge validation defaults are now fail-safe for development, test, and local
service runs. `RUSTY_CREW_BRIDGE_VALIDATE=1` still forces validation on, and
`RUSTY_CREW_BRIDGE_VALIDATE=0` forces it off. With no explicit setting,
validation is enabled unless `NODE_ENV=production`. Production deployments that
need to skip runtime validation should set either `NODE_ENV=production` or the
explicit bridge validation opt-out.

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
5. Regenerate the fingerprint with `npm run codegen:bridge-fingerprint`, then
   copy the generated value into the `bridgeWireShapeFingerprint` export in
   `ts/packages/contracts/src/index.ts`.
6. Add or extend the matching TypeBox schema in
   `ts/packages/native-bridge/src/bridge-validation-schemas.ts`.
7. Validate the fixture in
   `ts/packages/native-bridge/src/smoke-bridge-validation.ts`.
8. Wrap the native bridge parse/cast chokepoint with `validateBridgeValue`.
9. Add the operation to either `RUNTIME_VALIDATED_MANIFEST_OPERATIONS` or
   `RUST_FIXTURE_BACKED_OPERATIONS` in
   `ts/packages/native-bridge/src/bridge-validation-coverage.ts`. If the
   family is intentionally uncovered, add every operation to exactly one
   `BRIDGE_OPERATION_EXEMPTION_GROUP` with a narrow reason instead.
10. Update the exact ratchets in `bridge-validation-coverage.ts`.
11. Run the full bridge gate:

```bash
npm run typecheck
npm run smoke:bridge-contract-parity
npm run smoke:bridge-native-surface
npm run smoke:bridge-fixture-drift
npm run smoke:bridge-fingerprint-drift
npm run smoke:bridge-validation
npm run format
git diff --check
```
