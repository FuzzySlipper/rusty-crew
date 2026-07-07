# Native Bridge Rust Contract Mapping Migration

Status: planning note for task 4587

Date: 2026-07-07

Related docs:

- `bridge-contract-validation`
- `native-bridge-artifact-strategy`
- `core-bridge-node-decomposition-plan`
- `[doc: rusty-crew/typescript-authority-refactor-catalog-2026-07-07]`

## Purpose

The bridge has strong operation-level guardrails, but the field-level mapping
surface is still too hand-written in TypeScript. The next migration should move
more bridge wire-shape authority to Rust/codegen while keeping the TypeScript
native package as the ergonomic Node-facing wrapper.

This is not a request to hide the bridge behind a clever generated client. The
goal is a boring, reviewable bridge where Rust owns operation inventory and wire
shape evidence, and TypeScript owns only wrapper ergonomics plus explicit
runtime validation hooks.

## Current State

Current large surfaces:

- `ts/packages/native-bridge/src/index.ts`: native loader, raw native binding
  interface, wrapper methods, JSON parse/cast helpers, and ergonomic mapping.
- `ts/packages/native-bridge/src/bridge-validation-schemas.ts`: TypeBox
  validation schemas for covered bridge wire families.
- `ts/packages/contracts/src/index.ts`: shared bridge operation inventory,
  protocol aliases, and the wire-shape fingerprint export.
- `crates/bridge/core-bridge-api/bridge-manifest.toml`: operation manifest.
- `crates/bridge/core-bridge-codegen`: manifest checks, native surface checks,
  Rust fixture emission, and wire-shape fingerprint checks.

Current guardrails already cover:

- manifest/Rust/TS operation-name parity;
- generated napi `*Json` method inventory checks;
- Rust fixture drift checks;
- wire-shape fingerprint checks;
- TypeBox validation coverage ratchets;
- exact exemption groups for operations not yet fixture/schema covered.

Remaining risk:

- raw TS binding method declarations can drift from native JSON methods;
- raw/ergonomic mapping code in `native-bridge/src/index.ts` is large enough to
  hide field-level mistakes;
- TypeBox schemas still mirror Rust shapes manually for covered families;
- contract exports and fixtures are checked, but not enough operation families
  are fixture-backed for the UI-heavy surfaces now growing.

## Target Ownership

Rust/codegen owns:

- manifest operation inventory and direction;
- native JSON method inventory checks;
- Rust-authored sample wire fixtures for active operation families;
- wire-shape fingerprints;
- generated or generated-checked raw TS method/signature metadata;
- generated or generated-checked schema artifacts where practical.

TypeScript owns:

- native addon loading and stale-binary error presentation;
- ergonomic wrapper method names and object-friendly convenience shapes;
- runtime validation invocation at bridge chokepoints;
- domain-specific helper functions that adapt Rust wire DTOs to service code.

## Migration Slices

### 1. Generate Or Check The Raw Native Binding Interface

Use `bridge-manifest.toml` plus the generated napi declaration to generate or
check the `NativeBridgeBinding` raw method inventory in
`ts/packages/native-bridge/src/index.ts`.

Acceptance:

- raw `*Json` method names and signatures cannot drift from the manifest/native
  declaration without failing CI;
- the hand-written raw interface shrinks or is moved into generated output;
- stale binary checks still run before bridge use.

### 2. Expand Rust Fixture Families For Active UI Surfaces

Add Rust-authored fixture families for bridge surfaces that are now live UI/API
paths instead of obscure internal plumbing.

Priority families:

- conversation slots, variants, branches, snapshots, and jumps;
- attachments and data-bank scopes;
- roleplay lore records and recall traces;
- scheduler/runtime diagnostics and maintenance reports;
- session activity digests and context-compaction artifacts.

Acceptance:

- fixture family count increases with exact ratchet updates;
- fixture drift and fingerprint checks fail on Rust wire-shape changes;
- covered operation exemptions are removed when fixtures land.

### 3. Generate Or Check TypeScript Validation Schemas

For fixture-backed families, reduce hand-written TypeBox drift by generating
schemas, generating schema tests, or checking schemas against Rust fixture keys
and enum tags more strictly.

Acceptance:

- adding a Rust field to a covered fixture fails unless TS validation/schema
  handling is updated intentionally;
- enum/tag drift is detected for covered families;
- schema coverage count ratchets upward or a deliberate exemption explains why
  generation is not ready.

### 4. Decompose Ergonomic Wrapper Mapping By Operation Family

Split `native-bridge/src/index.ts` so operation-family wrappers live in smaller
modules, mirroring the Rust `core-bridge-node` extraction. This is secondary to
authority movement but makes review safer.

Acceptance:

- `index.ts` retains loader/composition logic rather than all family mapping;
- operation-family wrappers remain covered by validation/fingerprint gates;
- public `@rusty-crew/native-bridge` exports remain compatible.

### 5. Ratchet Bridge Additions

Make bridge operation additions follow an explicit greenpath:

1. manifest operation;
2. Rust operation name;
3. native surface check;
4. fixture or exact exemption;
5. fingerprint update when fixture-backed;
6. TS validation or generated schema;
7. wrapper mapping.

Acceptance:

- CI fails for a new bridge operation without coverage or exact exemption;
- documentation names the commands for adding fixture-backed and exempt
  operation families;
- the exemption count trends down for active UI/service families.

## Implementation Notes

Do not generate a full opaque client that hides the bridge boundary. Generated
pieces should be reviewable artifacts or checks that make hand-written mappings
safer.

Do not treat `MANIFEST_VERSION` as a substitute for fixture/schema coverage on
active UI paths. Version bumps are a coarse compatibility guard for uncovered
families, not a reason to let covered shapes drift.

