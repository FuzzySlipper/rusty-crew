# Tool Registry Rust Authority Migration

Status: planning note for task 4582

Date: 2026-07-07

Related docs:

- `tool-registry-metadata-ownership`
- `tool-architecture-registry-rules`
- `mcp-registry-integration-and-collision-policy`
- `[doc: rusty-crew/typescript-authority-refactor-catalog-2026-07-07]`

## Purpose

Rusty Crew already has the right long-term split for tools, but the current
implementation still leaves too much authority in TypeScript. The next tool
registry work should finish separating portable tool policy from executable
tool bindings.

Portable metadata is service policy. It describes what a tool is, how it may be
selected, how it collides with other tools, and how operators/debug surfaces can
explain its availability.

Executable binding is brain-island implementation detail. It describes which JS
module or MCP executor can run a validated tool name during a brain turn.

Those two ledgers must stay separate.

## Current State

`crates/core/core-tool-registry` already owns a useful Rust validator for the
shared portable fixture:

- canonical lower-snake-case names;
- aliases;
- duplicate names;
- alias/name collisions;
- capability collisions by category and output shape;
- deprecation replacement rules;
- required public metadata fields.

`fixtures/tool-registry/default-tool-registry-metadata.json` is the current
portable artifact consumed by Rust tests.

Task #4677 moved the default TypeScript registry to consume that portable
artifact directly. `tool-registry.ts` now keeps only the private executable
binding ledger for built-in tools; public descriptors, admin catalog data, and
normal diagnostics are derived from the artifact-backed metadata. The
`implementationModule` field remains binding/debug data only.

TypeScript still owns several policy surfaces:

- `ts/packages/brain-island/src/tool-registry.ts` defines TypeScript-facing
  public metadata types, private executable bindings, validation adapters,
  inventory status, denial reasons, and toolset catalog construction.
- `ts/packages/brain-island/src/tool-profile-selection.ts` owns profile/session
  selection rules and safety-flag denials.
- `ts/packages/brain-island/src/local-tool-profiles.ts` owns local tool profile
  validation against the built-in catalog and default local profile seeding.
- `ts/packages/brain-island/src/mcp-tool-registry-integration.ts` merges MCP
  candidates into the same registry, but validation remains TS-driven.
- `ts/packages/adapter-mcp/src/mcp-discovery.ts` normalizes dynamic MCP tools
  before the Rust validator has a chance to reject collisions or malformed
  metadata.
- `ts/packages/brain-island/src/tool-registry-diagnostics.ts` can expose debug
  executable bindings; that must remain explicitly debug-only and must not
  become a public contract.

## Target Ownership

Rust/codegen owns portable policy:

- public metadata schema;
- tool name, alias, deprecation, replacement, toolset, category, surface,
  safety, output-shape, and version validation;
- inventory status and denial-reason vocabulary;
- local tool profile reference validation;
- MCP normalized metadata validation before exposure;
- collision diagnostics for built-in and dynamic tools;
- portable catalog artifact generation or verification.

TypeScript owns executable capability:

- JS executor modules and factories;
- MCP client calls and source-routing details;
- provider-specific conversion from selected descriptors to model-callable
  tools;
- adapter client calls;
- runtime binding diagnostics guarded as debug data.

The Rust side should never learn `implementationModule`, JS factory names, MCP
client instances, or provider-specific model-callable quirks. TS should never
be the final authority for public tool policy once the metadata has crossed the
validation boundary.

## Migration Slices

### 1. Expand `core-tool-registry` Into The Portable Policy Crate

Add Rust types and validation for the full public catalog shape, including
inventory status and denial reasons. Keep executor binding fields out of this
crate.

Acceptance:

- Rust validates the default portable tool registry without TS execution
  modules.
- Rust exposes or generates the public metadata and diagnostic vocabulary used
  by TS/admin surfaces.
- Adding a new public metadata field requires updating Rust validation and the
  artifact/parity tests.

### 2. Generate Or Import The TS Public Catalog From Rust-Owned Metadata

Change TS so the portable catalog is either generated from Rust/codegen output
or imported from a Rust-validated artifact. TS should keep a separate
`ToolExecutableBinding[]` keyed by canonical tool name.

Current implementation: the default TS catalog imports
`fixtures/tool-registry/default-tool-registry-metadata.json` through
`tool-registry-portable-catalog.ts`. Rust validates that artifact in
`cargo test -p rusty-crew-core-tool-registry`; TS parity smokes verify fixture
formatting, one binding per public metadata entry, and that public metadata does
not expose executable implementation modules. New built-in tools should add or
modify portable metadata in the fixture, then add the matching TS executable
binding.

Acceptance:

- every executable binding points at a Rust/codegen-validated canonical name;
- every selected public descriptor comes from the portable metadata catalog;
- `implementationModule` remains private binding/debug data;
- public diagnostics exclude implementation modules unless an explicit debug
  option is requested.

### 3. Move Local Tool Profile Validation To The Portable Policy Boundary

Local tool profiles are currently DB-backed records, but their toolset/tool
references are validated in TS. Move the reference and denial-shape validation
to the Rust/codegen policy surface so profile create/update and brain assembly
use the same catalog authority.

Acceptance:

- local tool profiles reject unknown built-in toolsets/tools through the
  Rust/codegen validator;
- local tool profiles continue to reject dynamic `mcp:` toolsets;
- seeded system profiles are validated by the same path as user-created
  profiles;
- profile APIs return stable reason codes from the portable policy vocabulary.

Current implementation: `core-tool-registry` owns
`LocalToolProfileValidationInput` and the stable validation result vocabulary.
The native bridge exposes it as `validate_local_tool_profile_policy`, and
`local-tool-profiles.ts` calls that Rust policy for seeded, created, and updated
profiles. TypeScript still owns DB route plumbing and catalog projection, but no
longer makes the final local tool profile reference decision.

### 4. Route Dynamic MCP Metadata Through The Same Gate

`adapter-mcp` should keep discovering tools and building execution candidates,
but the normalized public metadata must pass through the canonical validator
before it is merged into the session inventory.

Acceptance:

- MCP tools cannot shadow local tools unless an explicit source-prefix policy
  changes the exposed canonical name before validation;
- duplicate MCP exposed names fail closed;
- invalid dynamic metadata is reported in MCP/tool diagnostics and is not
  exposed to the model;
- source routing remains TS binding data, not public portable metadata.

Current implementation: dynamic MCP candidates stay TS-discovered and
TS-executed, but `integrateMcpToolsWithRegistry` now requires a portable
metadata policy validator. The service and reload paths provide
`createBridgeToolMetadataPolicyValidator(state.bridge)`, which calls the native
`validate_tool_metadata_policy` operation backed by `core-tool-registry`. MCP
source routing, annotations, schemas, and executor module strings remain private
binding data outside the Rust metadata payload.

### 5. Ratchet Fixtures And Smokes Around The New Source

The current fixture checks should survive the migration and become stricter.
New tools should require portable metadata, executable binding, and registry
test evidence unless explicitly exempted by a reviewed task.

Acceptance:

- Rust tests validate the portable catalog and representative bad fixtures;
- TS parity smokes verify executable bindings are keyed only by validated names;
- MCP smokes cover local-name collision, duplicate dynamic names, prefix policy,
  unavailable dynamic tools, and diagnostics;
- local tool profile smokes cover unknown toolset/tool rejection through the
  Rust/codegen policy path;
- CI fails if generated artifacts drift.

## Implementation Notes

Prefer small codegen or artifact boundaries over another hand-maintained mirror.
Until codegen is fully in place, the fixture can remain the bridge as long as
both Rust and TS assert it is current.

Do not preserve legacy fallback behavior. If a profile or MCP surface provides
tool metadata that cannot pass the portable validator, the right result is a
diagnostic and no model exposure.

Do not move ordinary tool execution into Rust as part of this work. The purpose
is authority over metadata and policy, not reimplementing browser, web, memory,
MCP, or local code tools.
