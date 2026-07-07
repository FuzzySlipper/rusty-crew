# Core Bridge Node Remaining Extraction Plan

Status: design record for Den task `#4442`

`crates/bridge/core-bridge-node/src/lib.rs` is intentionally still the native
transport entrypoint after `#4328`. The first decomposition moved operation
domain implementation out to modules while preserving the public napi surface.
This note covers the remaining bulk and proposes follow-up slices that protect
the bridge contract instead of optimizing for line count alone.

## Remaining Inventory

Current rough shape after the `#4328` slices:

- `lib.rs`: about 4,870 lines.
- Exported napi methods: 138 `#[napi]` methods.
- Napi object types: 60 `Js*` object structs.
- Wire-only JSON request structs: 10 `Wire*` structs.
- Conversion/parsing/status helpers: about 73 free helper functions.
- Native bridge unit tests: 17 tests plus local test helpers.

One operation-domain cluster remains in `lib.rs`: scheduler/job bridge methods
and their JSON/status mapping helpers. That should be extracted before
conversion-helper work so the rest of the plan is truly about transport
wrappers, types, conversions, and tests.

## Decisions

### Keep Exported Napi Wrappers Explicit

Do not replace `NativeBridgeBinding` methods with generic string dispatch. The
explicit method list is part of the reviewable bridge contract and is guarded by
the native surface inventory.

Splitting `#[napi] impl NativeBridgeBinding` across modules might be possible,
but it should not be assumed. It needs a small proof with d.ts/native-surface
validation before becoming the default cleanup path. Until that proof exists,
keep exported wrappers in `lib.rs`.

### Move Types Before Conversions

Napi object structs and small wire-only request structs are lower risk than
conversion helpers because their shape is already guarded by d.ts and wire
fixture/fingerprint checks. A first type extraction should move them into a
`wire_types.rs` module while keeping names, field names, and serde/napi derives
unchanged.

### Prefer Domain Conversion Modules Over One Dumping Ground

Do not create a large `conversions.rs` as another mega-file. Use a tiny shared
module only for genuinely generic helpers:

- `parse_json`
- `serialize_json`
- `to_napi_error`
- handle conversions
- tiny status/string helpers used in more than one domain

Domain-specific conversions should move near their owned surfaces:

- scheduler status/job/run JSON helpers;
- session/brain/provider-state conversions;
- config/profile/provider conversions;
- storage/admin diagnostics conversions;
- memory/simple-kv/runtime-search conversions;
- conversation/roleplay request wire structs and JSON helpers.

If a conversion is only used by a single exported wrapper and the wrapper must
remain in `lib.rs`, it can temporarily remain in `lib.rs` until the wrapper
split proof is done.

### Split Tests After Helpers Stabilize

Move tests only after type/conversion movement is stable. Unit tests currently
need private crate state and local helper access, so the safe first move is a
file module:

```rust
#[cfg(test)]
mod tests;
```

Then split test support and domain tests only if that improves ownership:

- `test_support.rs` or nested test helper module for common registrations,
  sessions, and fake HTTP server fixtures;
- `responses`-specific tests can move closer to `responses.rs` if they only
  touch responses bridge helpers;
- registry/session/engine tests can remain unit tests so they can inspect
  crate-private state.

Avoid moving tests to integration tests if that would require widening private
APIs only for tests.

## Proposed Follow-Up Sequence

1. Extract scheduler bridge support (`#4443`).
   Move scheduler/job methods and scheduled job/run JSON/status helpers into a
   focused module. This closes the last obvious operation-domain cluster.

2. Extract native wire type definitions (`#4444`).
   Move `Js*` napi object types and `Wire*` request types into `wire_types.rs`.
   Keep names and derives stable. Run native surface and fixture/fingerprint
   checks.

3. Extract shared wire helper primitives (`#4445`).
   Move generic JSON/error/handle helpers into a small module. Do not move
   domain conversions yet.

4. Extract domain conversion helpers (`#4446`).
   Move scheduler, profile/provider, memory/storage, conversation/roleplay,
   event/session/brain/provider-state conversions into focused modules where
   imports stay readable.

5. Split native bridge tests (`#4447`).
   Move the test module to its own file, then optionally split helpers/domain
   tests once the helper modules have settled.

6. Prove or reject exported-wrapper module splitting (`#4448`).
   Create a tiny napi-rs proof that multiple `#[napi] impl NativeBridgeBinding`
   blocks in separate modules keep the same generated native surface. Only if
   the proof passes should wrapper methods be moved by domain.

## Validation Gates

Every implementation slice in this series should run:

```bash
cargo fmt --all --check
cargo test -p rusty-crew-core-bridge-node --lib
cargo clippy -p rusty-crew-core-bridge-node --all-targets -- -D warnings
npm run smoke:bridge-contract-parity
npm run smoke:bridge-native-surface
npm run smoke:bridge-fixture-drift
npm run smoke:bridge-fingerprint-drift
npm run smoke:bridge-validation
```

Slices touching storage, memory, roleplay, or responses should additionally run
the relevant domain crate tests, such as:

```bash
cargo test -p rusty-crew-core-persistence --lib
cargo test -p rusty-crew-openai-responses-brain --lib
```

As with the `#4328` slices, `smoke:bridge-native-surface` may rebuild
`ts/packages/native-bridge/native/index.linux-x64-gnu.node`. That binary is
ignored build output; commit only source changes and any intentional
`index.d.ts` declaration-surface update.

## Non-Goals

- Do not widen Rust visibility just to make tests easier to move.
- Do not introduce generated code by hand.
- Do not change bridge operation names or napi method signatures.
- Do not split wrappers before proving napi-rs keeps the generated surface
  stable across module-local impl blocks.
