# Roleplay Boundary And Rust Migration Plan

Status: active boundary note for task 4507.

Rusty Crew roleplay started as browser/API glue around existing chat, lore, and
profile surfaces. It has now grown enough that the default home cannot be a
single TypeScript route file. New roleplay functionality should be sorted by
authority before it lands.

## Boundary Rule

Roleplay TypeScript is allowed to own:

- HTTP route dispatch, request-envelope mapping, CORS/browser compatibility,
  and service JSON error envelopes.
- Calls into the existing bridge and service ports.
- Brain/provider glue that has to compose with the TypeScript brain island,
  including narrator brain integration and wake-triggered generation.
- Transitional adapters while a Rust-owned roleplay contract is being built.

Roleplay TypeScript should not become the durable authority for:

- Lore, character, persona, session, scene, or alternate-message invariants.
- Prompt/context assembly rules that should be deterministic and unit-tested
  without Node.
- Branch/variant selection semantics.
- Roleplay memory/scene state storage semantics.
- Import/export or validation of roleplay-specific data shapes.

Those should move toward Rust crates under `crates/roleplay/` or existing
Rust-owned storage modules when the concern is persistence.

## Current Inventory

Current roleplay route surface after the first extraction:

| Area | Current home | Intended authority |
| --- | --- | --- |
| Lore browser API | `ts/packages/brain-island/src/roleplay/lore-routes.ts` | TS route adapter over Rust persistence |
| Lore storage/query semantics | `crates/core/core-persistence` bridge operations | Rust |
| Character/persona admin API | `service-roleplay-routes.ts` | Rust roleplay domain plus TS route adapter |
| Session metadata API | `service-roleplay-routes.ts` | Rust roleplay domain plus TS route adapter |
| Prompt context and speaker identity snapshots | `service-roleplay-routes.ts` | Rust deterministic assembly, with TS brain glue |
| Assistant alternatives and branch-head selection | `service-roleplay-routes.ts` | Rust deterministic variant/branch semantics, TS generation glue |
| Narrator config API | `service-roleplay-routes.ts` | Rust config/domain validation, TS route adapter |
| Narrator brain execution | `narrator-brain.ts` | TS brain module until a Rust brain module is deliberately built |

## First Extraction

Task 4507 moved the lore browser API out of
`service-roleplay-routes.ts` into `roleplay/lore-routes.ts`.

This is intentionally not a claim that lore authority belongs in TypeScript.
The extracted module is route glue:

- it parses browser/admin HTTP routes;
- it maps camelCase/snake_case request bodies to bridge write shapes;
- it returns browser-safe projections;
- it delegates storage, query, promotion, and layer-link behavior to Rust-backed
  bridge operations.

The only cross-domain callback is `upsertSessionMetadata`, used when chat layer
bindings update a roleplay session's active layer ids. That dependency is
explicit so the lore route does not import the broader session API and form a
hidden route cycle.

## Rust Crate Direction

Prefer these crate boundaries when moving roleplay authority out of TypeScript:

1. `crates/roleplay/roleplay-core`
   - deterministic domain types;
   - lore trigger/control normalization;
   - character/persona/session validation;
   - speaker identity and prompt context assembly;
   - assistant-alternative prompt/branch invariants.
2. `crates/roleplay/roleplay-contracts`
   - stable request/response DTOs if OpenAPI/bridge codegen needs a roleplay
     contract home instead of hand-maintained TS interfaces.
3. Existing `crates/core/core-persistence`
   - durable SQL-backed storage repositories for lore, roleplay memory, scene
     state, variants, and related module-owned records.

Avoid creating a Rust crate that is only a parked duplicate of TypeScript. A
Rust roleplay crate should own behavior that is wired into either bridge
operations, persistence repositories, or brain wake/request construction.

## Next Slices

Good follow-up slices, in priority order:

1. Move roleplay alternatives and branch-head semantics behind a Rust-owned
   deterministic helper or bridge operation. TS may still invoke the model for
   generated text, but variant selection rules should not live in route glue.
2. Move prompt context and speaker identity snapshot assembly to Rust so role
   context can be tested without a service host or Node runtime.
3. Move character/persona/session metadata validation into a Rust roleplay
   domain crate, leaving TS as JSON route mapping.
4. Move narrator config validation into the same Rust domain crate before more
   roleplay-specific runtime knobs accumulate.

## Non-Goals

- Do not create a separate service for roleplay.
- Do not put model provider SDK calls in Rust merely because roleplay uses them.
- Do not move browser CORS/envelope compatibility into Rust unless the whole
  admin route layer moves there.
- Do not preserve internal TypeScript import compatibility during cleanup; this
  project is still in a clean-break remediation window.
