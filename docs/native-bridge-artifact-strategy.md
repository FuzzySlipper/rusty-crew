# Native Bridge Artifact Strategy

Status: active convention

The native bridge addon is build output, not source. Fresh checkouts produce it
with:

```sh
npm run build:native
```

The repository commits only the generated declaration surface:

- `ts/packages/native-bridge/native/index.d.ts`

The repository intentionally does not commit generated native runtime artifacts:

- `ts/packages/native-bridge/native/index.js`
- `ts/packages/native-bridge/native/index.linux-x64-gnu.node`

## Why

The `.node` addon is a large platform-specific binary. Local native builds
rewrite it frequently, which creates dirty worktrees that look like source
changes. Committing only `index.d.ts` keeps the native surface visible to CI and
review while treating the loader and binary as reproducible build artifacts.

## Validation

`npm run smoke:bridge-native-surface` checks the generated declaration surface
against the Rust bridge manifest/codegen expectation using an already-built
addon. Use `npm run verify:bridge-native-surface` for the one-step local command
that builds the addon and then runs the surface check. The full
`npm run verify:offline` lane builds once before all native checks, avoiding a
second incremental native build inside the smoke itself.

`npm run smoke:native-artifact-tracking` asserts that the declaration file is
the only tracked file under `ts/packages/native-bridge/native/` and that the
loader/binary outputs are ignored.

`npm run smoke:bridge-validation` loads the built addon and validates runtime
bridge fixtures. Run `npm run build:native` first when invoking native smokes
directly.

## Agent Guidance

If a native smoke or local service rebuild leaves files under
`ts/packages/native-bridge/native/`, treat them as build output unless the task
explicitly says to update generated native surface declarations.

Do not stage `.node` or generated loader `.js` files. If the bridge surface
changes, commit the relevant Rust/TypeScript source plus the updated
`index.d.ts`, bridge fixture, or fingerprint files required by the drift gates.
