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
| Assistant alternative terminal-slot and branch-head planning | `crates/roleplay/roleplay-core` via `plan_roleplay_assistant_alternative` | Rust deterministic domain |
| Character/persona admin API | `crates/roleplay/roleplay-core` via write/merge validators | Rust deterministic validation, with TS route adapter and persistence calls |
| Session metadata API | `crates/roleplay/roleplay-core` via metadata patch validator | Rust deterministic validation/reference checks, with TS route adapter and persistence calls |
| Prompt context and speaker identity snapshots | `crates/roleplay/roleplay-core` via `build_roleplay_prompt_context` and `roleplay_speaker_identity` | Rust deterministic assembly, with TS record fetching/brain glue |
| Assistant alternative persistence and selection routes | `service-roleplay-routes.ts` | TS route adapter over Rust domain planning and persistence bridge operations |
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

1. Continue expanding assistant alternative ownership beyond the current
   `plan_roleplay_assistant_alternative` helper as needed. TS may still invoke
   the model for generated text, but terminal-slot, branch-head, and
   no-normal-chat-append invariants now have a Rust domain home.
2. Keep prompt context and speaker identity expansion inside the current
   `roleplay-core` helpers so role context remains testable without a service
   host or Node runtime.
3. Keep character/persona/session metadata expansion inside the current
   `roleplay-core` write/merge/patch helpers so required fields, archived
   references, layer references, and status transitions stay testable without
   Node.
4. Move narrator config validation into the same Rust domain crate before more
   roleplay-specific runtime knobs accumulate.

## Non-Goals

- Do not create a separate service for roleplay.
- Do not put model provider SDK calls in Rust merely because roleplay uses them.
- Do not move browser CORS/envelope compatibility into Rust unless the whole
  admin route layer moves there.
- Do not preserve internal TypeScript import compatibility during cleanup; this
  project is still in a clean-break remediation window.
