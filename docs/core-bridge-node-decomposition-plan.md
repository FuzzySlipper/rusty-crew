# Core Bridge Node Decomposition Plan

Status: implemented record for Den task `#4328`

`crates/bridge/core-bridge-node/src/lib.rs` is the native Node transport
boundary. It must stay explicit: bridge operation names, napi wrappers, and wire
shape mappings should remain boring and reviewable. This plan decomposes the
current large file without changing the native surface or adding generic
dispatch.

## Current Shape

Approximate current layout:

- lines 1-88: imports and `NativeBridge` state.
- lines 89-1185: internal `NativeBridge` implementation.
- lines 1188-1525: internal registries and validation helpers.
- lines 1533-2204: napi object types and bridge wire helper structs.
- lines 2215-2559: OpenAI responses brain bridge support.
- lines 2561-4544: exported `NativeBridgeBinding` napi wrapper methods.
- lines 4540-6067: mapping/conversion helpers.
- lines 6094-end: native bridge tests.

The exported operation inventory is stable through the manifest and must stay
aligned with `OPERATION_NAMES`, `native/index.d.ts`, bridge fixtures, and the
wire-shape fingerprint.

## Target Modules

Keep `lib.rs` as the public transport entrypoint:

- top-level imports and module declarations;
- `NativeBridge` struct and small constructor/default glue;
- `NativeBridgeBinding` napi class declaration and explicit wrapper methods;
- unavoidable napi object type definitions until a later codegen/design pass
  proves a better home;
- test module declarations.

Move implementation/support into focused modules:

| Module | Owns | Task |
| --- | --- | --- |
| `registries.rs` | brain implementation registry, platform adapter registry, subscription registry, registration validation | `#4435` |
| `engine.rs` or `lifecycle.rs` | manifest metadata, initialize/shutdown, runtime buffers | `#4436` |
| `sessions.rs` | create/ensure/archive/list sessions, session config conversion | `#4437` |
| `events.rs` | route/inject/subscribe/drain events, brain event/action submission | `#4437` |
| `delegation.rs` | delegated cancellation/checkpoint/drain/status/cleanup | `#4437` |
| `config_profiles.rs` | runtime config planning, profile registry, model providers, brain registration conversion | `#4438` |
| `storage_admin.rs` | row counts, database size, storage diagnostics, schema, maintenance, runtime search/counters/simple-kv | `#4439` |
| `memory.rs` | profile memory, session memory, memory proposals, compaction artifacts, governance decisions | `#4439` |
| `conversation.rs` | message slots, variants, branches, snapshots, attachments, data-bank scopes | `#4439` |
| `roleplay.rs` | lore records, lore layers, chat layers, recall, traces | `#4439` |
| `responses.rs` | OpenAI responses brain run/OAuth/buffered-tool support and provider-state mappings | `#4440` |
| `conversions.rs` | only genuinely shared conversion helpers after domain modules stop needing them locally | final cleanup |

Prefer moving helper code toward the domain that uses it. Create a shared
conversion module only for helpers used by multiple domains after extraction.

## Review Boundaries

The first implementation slice is `#4435`: extract registries and registration
validation. It is intentionally low-risk because it does not move napi object
definitions or exported binding methods.

Later slices should move one operation domain at a time. Each slice should keep
the public `NativeBridgeBinding` method names and signatures stable, then move
only the private implementation it delegates to.

## Final Outcome

The `#4435`-`#4440` implementation slices moved the operation-domain code into
focused Rust modules:

- `registries.rs`
- `engine.rs`
- `sessions.rs`
- `events.rs`
- `delegation.rs`
- `config_profiles.rs`
- `storage_admin.rs`
- `memory.rs`
- `conversation.rs`
- `roleplay.rs`
- `responses.rs`

`lib.rs` remains the native transport entrypoint. It still owns module
declarations, the `NativeBridge` state constructor, napi object definitions,
explicit `NativeBridgeBinding` exported wrappers, bridge wire conversion
helpers, and native bridge tests. That remaining shape is deliberate for this
series: the exported napi surface stays obvious, and the next conversion/test
split should be driven by bridge contract/codegen safety rather than line count
alone.

Follow-up `#4442` tracks the remaining conversion helper, napi object type, and
test module extraction design.

## Validation Gates

Run focused validation after each slice:

```bash
cargo test -p rusty-crew-core-bridge-node --lib
cargo clippy -p rusty-crew-core-bridge-node --all-targets -- -D warnings
cargo fmt --all --check
```

Run bridge surface validation before marking larger slices review-ready:

```bash
npm run smoke:bridge-contract-parity
npm run smoke:bridge-native-surface
npm run smoke:bridge-fixture-drift
npm run smoke:bridge-fingerprint-drift
npm run smoke:bridge-validation
```

If `smoke:bridge-native-surface` rebuilds
`ts/packages/native-bridge/native/index.linux-x64-gnu.node`, do not commit that
binary unless the task explicitly intends to update native artifacts. The
checked-in declaration surface and bridge validation fixture/fingerprint files
remain CI-visible drift gates.

## Non-Goals

- Do not add convenience operations.
- Do not replace explicit napi wrappers with stringly generic dispatch.
- Do not move coordination authority into the native bridge.
- Do not edit generated bridge artifacts unless a validation gate requires an
  intentional artifact update.
