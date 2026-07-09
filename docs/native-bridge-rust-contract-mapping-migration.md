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

### Current Guardrail Ratchets

As of the task 4696 ratchet pass, the bridge validation gate pins:

- manifest operations: 171;
- exported TypeBox bridge schemas: 36;
- Rust fixture families: 11;
- manifest operations with TypeBox runtime validation and/or Rust fixtures: 31;
- explicit operation exemptions: 140.

New bridge operations must either become runtime validated or fixture-backed,
or be added to exactly one documented exemption group in
`ts/packages/native-bridge/src/bridge-validation-coverage.ts`. Active UI and
service families should prefer fixture/schema coverage instead of growing the
exemption count.

The full bridge gate is:

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

### 1. Generate Or Check The Raw Native Binding Interface

Use `bridge-manifest.toml` plus the generated napi declaration to generate or
check the `NativeBridgeBinding` raw method inventory in
`ts/packages/native-bridge/src/index.ts`.

Acceptance:

- raw `*Json` method names and signatures cannot drift from the manifest/native
  declaration without failing CI;
- the hand-written raw interface shrinks or is moved into generated output;
- stale binary checks still run before bridge use.

Current implementation:

- `smoke:bridge-native-surface` compares the generated napi declaration against
  the TypeScript raw `NativeBridgeBinding` interface for operation names,
  parameter counts, method names, and return-kind shape.
- Task #5302 added `codegen:native-mapping-inventory` and
  `check:native-mapping-inventory` for generated-check mapper coverage. The
  first covered families are model providers, profile registry, and
  conversation/chat read models, roleplay, memory/compaction, and
  brain/provider wake runtime DTOs.

### 1a. Generated-Check Model Provider Mapper Inventory

`cargo run -p rusty-crew-core-bridge-codegen --
emit-native-mapping-inventory` now emits
`ts/packages/native-bridge/src/generated/native-mapping-inventory.ts`.

The artifact currently covers these bridge families:

- model-provider manifest operations:
  `upsert_model_provider`, `list_model_providers`, `get_model_provider`,
  `get_model_provider_secret`, `model_provider_refresh_impact`, and
  `plan_model_provider_refresh`;
- profile-registry manifest operations:
  `plan_profile_registry_mutation`, `create_profile_registry_record`,
  `update_profile_registry_record`, `list_profile_registry_records`,
  `get_profile_registry_record`, and `purge_profile`;
- conversation/chat manifest operations for message slots, message variants,
  chat read-model pages, chat event logs, conversation branches, branch state,
  snapshots, jumps, attachments, and data-bank scopes;
- roleplay manifest operations for lore/layer/recall persistence, scene-state
  planning, prompt-context assembly, speaker identity, character/persona and
  session metadata normalization, narrator phase planning, and assistant
  alternatives;
- memory manifest operations for session memory, memory proposals, governance
  decisions, activity digests, and context compaction artifacts;
- brain/provider manifest operations for wake execution, provider-state
  persistence, OpenAI Responses and pi-agent buffered run control, provider
  diagnostics, and buffered run cleanup;
- raw method names derived from those operations;
- passthrough wrapper names for conversation operations that intentionally keep
  Rust's persistence-contract JSON as the bridge authority instead of adding
  TypeScript mapper code;
- passthrough wrapper names plus JSON-body wrapper subsets for roleplay
  operations, including the non-JSON scalar
  `roleplayNarratorReviewRequestsRevision` native method exception;
- passthrough wrapper names plus JSON-body wrapper subsets for memory methods,
  and direct native method names for the profile-memory napi surface that is
  not represented as manifest JSON operations;
- raw/native method names, wrapper names, and direct native helper names for
  brain/provider wake surfaces whose implementation mixes manifest operations,
  native JSON methods, and runtime-buffer helper calls;
- named TypeScript interface inventories for Rusty View chat read-model and
  event-log DTOs;
- raw DTO field inventories for model provider records, credentials, refresh
  impacts, affected profiles, refresh plans, and refresh actions;
- raw DTO field inventories for profile registry records, writes, updates,
  mutation requests/plans, mutation implications, source asset refs, derived
  runtime refs, import/export metadata, purge reports, and purge table counts.
- Rust persistence-contract DTO field inventories for conversation branches,
  snapshots, message slots, variants, durable messages, read-model pages, chat
  events, attachments, attachment links, and data-bank scopes.
- Rust persistence-contract and roleplay-core DTO field inventories for lore
  records/layers/queries/recall traces, scene state, narrator configuration and
  phase plans, prompt context, speaker identity, character/persona/session
  metadata, and assistant alternative planning.
- Rust persistence/protocol DTO field inventories for profile memory, session
  memory, memory proposal records and queries, governance decisions, session
  activity digests, and context compaction artifacts.
- Rust protocol/native-boundary DTO field inventories for brain registrations,
  brain model/strategy metadata, wake requests, wake stream items, brain events
  and actions, provider-state input/output/diagnostics, tool-call metadata,
  completion packets, runtime-buffer views, and buffered brain run diagnostics
  and cleanup reports.

All DTO field inventories are derived from Rust-authored sample serialization.

`npm run check:native-mapping-inventory` does two checks:

1. The generated artifact has not drifted from Rust codegen output.
2. `@rusty-crew/native-bridge` smoke coverage verifies that
   `native-bridge/src/index.ts` still declares the raw methods/interfaces and
   that covered converter functions read the generated-checked fields. For
   conversation/chat and roleplay operations, the same smoke verifies the
   intentionally raw passthrough wrappers call their matching bridge methods,
   and JSON-body wrappers call those methods with `JSON.stringify`.

This is not a full client generator. It is a generated-check inventory that
keeps the handwritten ergonomic wrapper reviewable while making field omissions
fail loudly.

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
