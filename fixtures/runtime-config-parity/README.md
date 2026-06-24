# Runtime Config Parity Fixtures

These fixtures guard the temporary handwritten TypeScript facade for Rust-owned
runtime/profile config validation.

Rust owns the canonical control-plane shapes and validation behavior in
`crates/core/core-config`. TypeScript still loads `service.json` and profile
files, then converts them into bridge-facing validation inputs in
`ts/packages/brain-island/src/runtime-config-validation.ts`.

Until bridge manifest/codegen can generate the TypeScript validator facade
types, keep these fixtures representative and shared:

- `valid/service.json` and `valid/profiles/parity-runner.json` are loaded by
  the TypeScript runtime config loader.
- `valid/validation-input.camel.json` is the expected TypeScript normalized
  bridge input.
- `valid/validation-input.snake.json` is the same validation input in the Rust
  serde/native bridge shape.
- `invalid/service.json` is accepted by the TypeScript loader but must produce
  structured Rust diagnostics during preflight.

Use `__FIXTURE_ROOT__` only for deterministic path substitution in tests.
