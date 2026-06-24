# Tool Registry Metadata Ownership

Status: design decision for task 3231

Date: 2026-06-24

Related docs:

- `tool-architecture-registry-rules`
- `mcp-registry-integration-and-collision-policy`
- `[doc: rusty-crew/brain-island-rust-ownership-audit-2026-06-23]`

## Decision

Rusty Crew should adopt a two-layer tool registry:

1. Canonical tool policy metadata should move toward a Rust/codegen-owned
   contract and validator.
2. TypeScript should remain the owner of executable tool bindings, JavaScript
   callable wrappers, MCP client routing, and brain-module presentation.

Do not move the current TypeScript registry wholesale into Rust. The registry
currently mixes stable policy metadata with `implementationModule` executor
pointers. Moving that shape directly would teach Rust about Node module wiring
and make the bridge less neutral.

The next implementation should separate metadata from executable binding first,
then add a Rust-owned validator or codegen-owned schema for the portable
metadata shape.

## Why

The tool registry is no longer just a brain-island convenience. It is becoming
control-plane data:

- profile creation and profile diagnostics need stable tool discovery;
- MCP exposure must not shadow or duplicate local tools;
- admin and Rusty View surfaces need a public list of commands/tools with
  safety and deprecation metadata;
- session audit needs a durable explanation for selected, denied, missing, and
  deprecated tools;
- duplicate-tool prevention should not depend on each TypeScript caller
  remembering to run the same helper.

Those are Rusty Crew service invariants. They fit Rust/codegen ownership better
than a TS-only helper once frontend APIs, MCP exposure, and profile mutation all
depend on the same registry.

At the same time, actual tool execution belongs in TypeScript. Local code
tools, Den memory tools, browser/web tools, skills, MCP tools, and delegation
wrappers depend on Node, JS package surfaces, adapter clients, and brain-module
translation. Rust should validate what a session is allowed to see and audit
what happened; it should not execute ordinary brain tools.

## Ownership Split

Rust/codegen-owned portable metadata:

- canonical `name`;
- `description`;
- `aliases`;
- `category`;
- `toolsets`;
- `surfaces`;
- `safety` flags;
- `outputShape`;
- `version`;
- `deprecated` and `replacement`;
- collision/coexistence metadata;
- inventory status and denial reason shape;
- validation rules for name format, alias collisions, capability collisions,
  deprecated replacements, and duplicate capability claims.

TypeScript-owned executable binding:

- `implementationModule`;
- JS executor factories and resolvers;
- provider-specific model-callable tool conversion;
- MCP transport clients and binding-specific execution routing;
- adapter-specific source metadata needed for calls;
- brain-module presentation quirks.

MCP dynamic tools should be TS-discovered and TS-executed, but their converted
metadata should pass through the same canonical validator before selection or
public exposure.

## Suggested Rust Home

Use a dedicated `core-tool-registry` crate when implementation begins.

The registry is more than protocol shape because it owns validation behavior,
collision policy, and inventory explanations. Putting all of that in
`core-protocol` would make protocol carry policy logic. Putting it in
`core-engine` would make it harder for config/profile APIs and future codegen
checks to reuse without pulling in engine composition.

`core-tool-registry` can depend on `core-protocol` for shared descriptor types
or export generated protocol-safe structs through the bridge. Config/profile
validation can call into it when official profile creation and refresh APIs are
implemented.

## Migration Shape

Start with a generated or exported metadata artifact that excludes executor
pointers. The current TypeScript registry can remain the authoring location for
the first pass if it produces:

- portable metadata JSON;
- TS executable binding records keyed by canonical tool name;
- a smoke test that proves every executable binding has metadata;
- a Rust validation smoke/test that proves the portable metadata satisfies the
canonical rules.

After that, the source of truth can move to a manifest or Rust-owned schema
without changing executable tool wiring.

Current guardrail artifact:

- fixture:
  `fixtures/tool-registry/default-tool-registry-metadata.json`;
- regenerate after registry metadata changes:
  `npm run generate:tool-registry-artifact`;
- check TS metadata/binding/descriptor parity:
  `npm run smoke:tool-registry-parity`;
- check Rust canonical validator parity:
  `cargo test -p rusty-crew-core-tool-registry`.

## Invariants Until Migration

While TypeScript remains the practical source of truth, it must continue to
enforce these invariants:

- canonical names are lower snake case and unique;
- aliases cannot duplicate another alias or collide with another canonical
  name;
- deprecated tools must have a replacement or explicit sunset path;
- active tools cannot share `(category, outputShape)` without an explicit
  coexistence note;
- requested aliases resolve to canonical names and appear as shadowed inventory
  entries, not separate selected tools;
- profile, session, and resource denials remain visible in inventory;
- MCP tools fail closed on name collisions unless an explicit source-prefix
  policy is selected;
- unavailable MCP tools appear as resource denials, not deprecations;
- selected tool descriptors given to Rust match the selected inventory;
- public diagnostics should expose canonical metadata and source information,
  but should not treat `implementationModule` as a public contract.

## Follow-Up Tasks

The implementation should be split into scoped tasks:

1. Define the portable canonical metadata shape and Rust validator.
2. Split the TS registry into metadata records and executable binding records.
3. Route MCP dynamic metadata through the canonical validator.
4. Update diagnostics/admin APIs to expose public metadata without executor
   pointers.
5. Add parity guardrails so Rust, TS, and generated artifacts cannot drift.
