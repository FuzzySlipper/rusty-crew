# Runtime Config Parity Fixtures

These fixtures guard the temporary handwritten TypeScript facade for Rust-owned
runtime/profile config validation.

Rust owns the canonical control-plane shapes and validation behavior in
`crates/core/core-config`. TypeScript still loads `service.json` and profile
files, then converts them into bridge-facing validation inputs in
`ts/packages/brain-island/src/runtime-config-validation.ts`.

The native bridge now exposes a Rust-emitted `core-config` facade field
inventory from `ts/packages/native-bridge/src/generated/core-config-facade.ts`.
`smoke:runtime-config-parity` compares these fixtures against that inventory so
new Rust-owned graph fields cannot appear silently.

Keep these fixtures representative and shared:

- `valid/service.json` and `valid/profiles/parity-runner.json` are loaded by
  the TypeScript runtime config loader.
- `valid/validation-input.camel.json` is the expected TypeScript normalized
  bridge input.
- `valid/validation-input.snake.json` is the same validation input in the Rust
  serde/native bridge shape.
- `valid/create-profile-request.camel.json` and
  `valid/create-profile-request.snake.json` cover create-profile request
  branches.
- `valid/profile-registry-runtime-metadata.*.json` covers the profile registry
  metadata branch used by create-profile planning.
- `coverage-manifest.json` records explicit coverage exemptions. Keep the list
  empty when possible; every exemption needs a reason long enough to explain
  why the field is TS-only, obsolete, or intentionally covered elsewhere.
- `invalid/service.json` is accepted by the TypeScript loader but must produce
  structured Rust diagnostics during preflight.

Use `__FIXTURE_ROOT__` only for deterministic path substitution in tests.
