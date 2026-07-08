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
| Narrator config API | `crates/roleplay/roleplay-core` via `normalize_roleplay_narrator_config` | Rust deterministic validation/defaulting, with TS route adapter and profile-file persistence |
| Narrator brain execution | `narrator-brain.ts` over `crates/roleplay/roleplay-core` FSM bridge operations | TS executor for phase/tool wake plumbing; Rust owns deterministic narrator sequencing, instruction construction, allowed tool sets, mandatory prelude planning, auto-capture planning, and review decisions |

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
4. Keep narrator config validation/defaulting expansion inside
   `normalize_roleplay_narrator_config` before more roleplay-specific runtime
   knobs accumulate.
5. Live certify the Rust-owned narrator FSM through the debug service and Rusty
   View, then continue moving any newly discovered deterministic roleplay
   behavior into `roleplay-core` instead of expanding the TS executor.

## Task 4584 Follow-Up Series

The TypeScript authority refactor catalog reopened this area after several Rust
roleplay slices landed. Treat the remaining work as implementation slices, not
as permission for another broad TS route file.

### 1. Session Lifecycle Planning

Move deterministic roleplay session create, fork, archive, and restore planning
into `roleplay-core`.

TS may still gather current records, call profile/session bridge APIs, and map
browser envelopes. Rust should own:

- required identifiers and default names;
- archived/restored status transitions;
- fork source validation inputs;
- copied metadata shape and layer/reference invariants;
- stable reason codes for lifecycle rejection.

### 2. Chat Layer Binding Side Effects

Chat layer binding currently crosses lore routes and session metadata. Move the
deterministic write plan into Rust so a browser layer update produces one
validated plan describing lore-layer writes plus the session metadata patch.

The persistence calls can remain bridge operations, but TS should not decide
whether a layer binding changes the active session layer list.

### 3. Alternative And Branch/Variant Invariants

`plan_roleplay_assistant_alternative` already gives the terminal-slot and
branch-head logic a Rust home. Continue expanding that boundary so TS supplies
records and generated text, while Rust owns:

- terminal slot choice;
- alternate variant identity and metadata;
- whether a generation may append to normal chat;
- active selection and branch-head update plan;
- conflict/retry reason codes.

### 4. Lore Control Normalization

`roleplay/lore-routes.ts` is route glue over Rust persistence, but it still
normalizes browser request semantics for search, promotion, layer entry context,
and scoped pagination. Move deterministic request/control normalization into
`roleplay-core` or a roleplay contract crate.

Keep HTTP parsing and response projection in TS. Do not move SQL-backed lore
queries out of `core-persistence`.

### 5. Scene State Tool Domain

The scene state brain tools currently own their state shape and merge behavior
in TS. Move the deterministic scene-state record shape, update/merge rules, and
tool result normalization into Rust-owned roleplay domain code or a persistence
repo if the state becomes durable module data.

TS should remain the brain tool adapter that calls the Rust/domain operation.

### 6. Certification And Ratchets

Every roleplay slice should include a Rust unit test for the moved invariant and
a browser/API or tool smoke proving the TS route/tool still behaves the same.
Larger behavior changes should be live-certified through the debug Rusty Crew
service and Rusty View, especially narrator and alternative-generation flows.

## Task 4584 Certification Matrix

The task 4584 follow-up series is the first concrete ratchet for this
boundary. Future roleplay work should keep this shape: Rust unit coverage proves
the deterministic invariant without Node, and a TS smoke proves the route/tool
adapter still presents the expected service behavior.

| Slice | Rust-owned invariant | Rust coverage | Adapter/behavior smoke |
| --- | --- | --- | --- |
| #4686 session lifecycle | create/fork/archive/restore validation, defaulting, metadata copy shape, status transitions | `cargo test -p rusty-crew-roleplay-core` tests for `plans_roleplay_session_create_defaults_and_references`, `plans_roleplay_session_fork_metadata_branch_and_layers`, `plans_roleplay_session_archive_and_restore_transitions`, and invalid lifecycle inputs | `npm run smoke:roleplay-browser-api -w @rusty-crew/brain-island` |
| #4687 chat layer binding | lore-layer write metadata plus active session layer patch planning | `cargo test -p rusty-crew-roleplay-core` tests for `plans_chat_layer_binding_metadata_and_write_side_effects`, no-op planning, and invalid inputs | `npm run smoke:roleplay-browser-api -w @rusty-crew/brain-island` |
| #4688 alternatives and variants | terminal assistant slot, alternate variant ids, active selection, branch-head updates, and no normal chat append side effect | `cargo test -p rusty-crew-roleplay-core` tests for `plans_assistant_alternative_explicit_variant_ids`, `plans_assistant_alternative_variant_write_ids_and_lineage`, conflict rejection, current branch-head planning, and stale/user-slot rejection | `npm run smoke:roleplay-browser-api -w @rusty-crew/brain-island` |
| #4689 lore controls | search layer filters, scoped/unscoped paging controls, and invalid query reason codes | `cargo test -p rusty-crew-roleplay-core` tests for `normalizes_lore_search_controls` and invalid pagination/default behavior | `npm run smoke:roleplay-browser-api -w @rusty-crew/brain-island` |
| #4690 scene state tool | state read defaults, persisted record normalization, update merge rules, tag/value cleanup, and invalid tool input reason codes | `cargo test -p rusty-crew-roleplay-core` tests for scene-state read defaults, update merge/normalization, and invalid updates | `npm run smoke:scene-state-tool -w @rusty-crew/brain-island` |

Run bridge checks whenever a roleplay slice adds or changes a bridge operation:

```sh
npm run build:native
npm run smoke:bridge-contract-parity
npm run smoke:bridge-native-surface
npm run smoke:bridge-validation
cargo test -p rusty-crew-core-bridge-node
```

Run the full roleplay deterministic handoff set before claiming a roleplay Rust
migration milestone:

```sh
cargo fmt --all --check
cargo test -p rusty-crew-roleplay-core
npm run typecheck
npm run smoke:roleplay-browser-api -w @rusty-crew/brain-island
npm run smoke:scene-state-tool -w @rusty-crew/brain-island
npm run smoke:roleplay-narrator-fsm-bridge -w @rusty-crew/brain-island
```

Live certification through Rusty View is mandatory when the change affects
provider text generation, streamed roleplay chat projection, narrator phase
sequencing as observed by a user, or generated-alternative selection/swipe
behavior. Use the debug service rather than the durable live service unless the
task is explicitly about production deployment state, and record evidence using
`docs/live-deliverable-certification.md`.

The #4687-#4690 slices moved deterministic planners and tool/domain
normalization only. They should not require live provider evidence by
themselves unless a reviewer finds that the visible generated-alternative or
narrator behavior changed.

## Non-Goals

- Do not create a separate service for roleplay.
- Do not put model provider SDK calls in Rust merely because roleplay uses them.
- Do not move browser CORS/envelope compatibility into Rust unless the whole
  admin route layer moves there.
- Do not preserve internal TypeScript import compatibility during cleanup; this
  project is still in a clean-break remediation window.
