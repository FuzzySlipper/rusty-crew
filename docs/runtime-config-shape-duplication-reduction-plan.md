# Runtime Config Shape Duplication Reduction Plan

Status: next-slice plan for task 4646

Date: 2026-07-07

## Context

Runtime/profile config authority is intentionally split, but the shape
definitions are still repeated in several places:

- `crates/core/core-config/src/lib.rs` owns canonical runtime-affecting config
  drafts, profile runtime metadata, create-profile planning inputs, expansion,
  and diagnostics.
- `ts/packages/brain-island/src/service-runtime-config.ts` parses
  service-authored `service.json`, keeps service-only fields, loads profiles,
  calls Rust planning, and applies the resulting graph to the running service.
- `ts/packages/brain-island/src/runtime-config-validation.ts` converts loaded
  TS config/profile data into the native bridge validation input.
- `ts/packages/native-bridge/src/index.ts` hand-maintains the camelCase
  public TS facade, private raw snake_case bridge shapes, and conversion
  helpers around `core-config`.
- `fixtures/runtime-config-parity/` pins one representative valid/invalid
  config family, but it is still fixture-level parity rather than generated
  source-of-truth.

The current guard is valuable: `npm run smoke:runtime-config-parity` proves the
loaded TS config and profile metadata match expected bridge inputs, and the
bridge calls Rust `validateRuntimeConfigDraft`, `planRuntimeConfig`, and
`planCreateProfile`. That is still not enough to stop shape duplication from
growing.

## Ownership Split

### Rust Authority

Rust `core-config` owns config that changes Rust-owned runtime graph behavior:

- brain registrations;
- sessions, session defaults, resource limits, and history windows;
- scheduled jobs and executable scheduled-job gates;
- channel bindings and MCP bindings;
- create-profile runtime defaults and duplicate/reference validation;
- profile runtime metadata reduced from profile files;
- diagnostics and expansion plans.

### TypeScript Glue

TypeScript should keep:

- disk parsing of `service.json` and profile assets;
- service-only config fields not currently part of the Rust runtime graph:
  storage bootstrap, Den observation filters, wake-timeout admin config,
  `mcpServers`, and external adapter/client config;
- profile file discovery and prompt/soul/memory/skill loading;
- applying a Rust-approved runtime graph to the service and native bridge;
- browser/admin route envelopes.

The retained TypeScript shape should be named and treated as a loader/envelope,
not as canonical validation authority.

## Next Reduction Path

1. **Generated native bridge config facade.**
   Generate the raw snake_case TypeScript interfaces, public camelCase
   interfaces, TypeBox validators, and conversion helpers for `core-config`
   bridge operations from Rust-owned metadata or schema output. Start with the
   `RuntimeConfigValidationInput`, `RuntimeConfigPlan`, `CreateProfile*`, and
   `ProfileRegistryMutation*` families.

2. **Loader-envelope split in `service-runtime-config.ts`.**
   Separate `service.json` parsing into two explicit outputs:
   `ServiceRuntimeEnvelope` for TS/service-only fields and
   `RuntimeConfigDraft` for Rust-owned graph fields. The runtime graph draft
   should use generated/native bridge types rather than hand-authored local
   interfaces where possible.

3. **Profile runtime metadata builder isolation.**
   Move the profile-to-`NativeProfileRuntimeMetadata` projection into a small
   module with fixture coverage. That module should be the only place that
   translates rich `ProfileConfig` files into Rust-owned runtime metadata.

4. **Parity ratchet.**
   Extend `smoke:runtime-config-parity` from one representative fixture to a
   ratcheted fixture family that covers every Rust-owned draft object and
   create-profile request branch. The smoke should fail when new
   `core-config` fields appear without either generated TS coverage or an
   explicit documented TS-only reason.

5. **Remove duplicate local authority.**
   Once generated/native types are available, delete or narrow the local
   `RustyCrewConfigured*` interfaces that duplicate Rust graph shapes. Keep
   TS-only fields in an envelope type with documented reasons.

## Immediate Test Tightening

Task 4646 updates `smoke-runtime-config-parity` to assert the existing
snake_case fixtures too:

- `valid/validation-input.camel.json` remains the expected TypeScript public
  facade shape.
- `valid/validation-input.snake.json` now pins the corresponding Rust serde
  shape.
- `valid/create-profile-request.camel.json` and
  `valid/create-profile-request.snake.json` are checked as a pair.

That does not replace generation, but it makes the existing fixture set less
ceremonial and keeps the Rust-facing shape visible until codegen takes over.

## Follow-Up Validation

Each implementation slice should keep these gates green:

```sh
npm run smoke:runtime-config-parity
npm run smoke:bridge-validation
npm run smoke:bridge-native-surface
npm run smoke:bridge-fixture-drift
npm run smoke:bridge-fingerprint-drift
npm run smoke:architecture-boundaries
npm run typecheck
cargo test -p rusty-crew-core-config
```

Run broader `npm run verify:offline` for the slice that deletes hand-authored
native-bridge config mappings.

## Current Loader/Envelope Boundary

Task 4667 split the brain-island runtime config type into two explicit pieces:

- `RustyCrewRuntimeGraphDraft` is the Rust-owned graph-facing shape:
  `profilesDir`, `skillsDir`, `brains`, `sessions`, `scheduledJobs`,
  `channelBindings`, and `mcpBindings`.
- `ServiceRuntimeEnvelope` is the TS/service-host loader envelope:
  `storage`, `denObservation`, and `mcpServers`.

`RustyCrewRuntimeConfig` remains the composed type that existing callers use,
but new runtime-affecting graph fields should land in the graph draft and be
reviewed against `core-config` / generated native bridge coverage. New
process, adapter, or loader-only fields should land in the envelope with a
clear reason they are not part of the Rust runtime graph.

Profile-file projection into `NativeProfileRuntimeMetadata` now lives in
`ts/packages/brain-island/src/profile-runtime-metadata.ts`; the
`smoke:runtime-config-parity` fixture covers that projection through the
validation input fixture.
