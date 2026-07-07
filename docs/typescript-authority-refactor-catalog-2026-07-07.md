# TypeScript Authority Refactor Catalog

Status: exploratory catalog for the Rust authority migration.
Date: 2026-07-07.

This note catalogs TypeScript that should be refactored as Rusty Crew moves away
from a central TypeScript brain island and toward Rust-owned authority. It is a
current-code inventory, not a claim that every TypeScript file is wrong.
TypeScript can remain useful for external adapters, provider SDK glue, route
envelopes, executable tool bindings, and authored configuration. The refactor
target is durable authority code: validation, orchestration, lifecycle,
selection policy, persistence semantics, cross-record consistency, and provider
loops that are easier to make reliable in Rust.

## Current TypeScript Footprint

Measured in the current working tree:

| Package | TypeScript LOC | Refactor posture |
| --- | ---: | --- |
| `ts/packages/brain-island` | 102,377 | Main migration target. Contains service composition, brain modules, route logic, tools, admin/control, memory, roleplay, and 132 smoke scripts. |
| `ts/packages/native-bridge` | 9,212 | Keep as bridge loader/validator for now, but reduce handwritten wire mapping as codegen/fixtures mature. |
| `ts/packages/adapter-den` | 6,016 | Mostly acceptable adapter glue, with channel routing and Den ingress policy that should be checked against Rust-owned route decisions. |
| `ts/packages/service-host` | 2,926 | Intended process-composition home. Should grow only as host glue while authority moves to Rust. |
| `ts/packages/contracts` | 1,872 | Transitional contract surface. Should become generated/checked, not hand-maintained authority. |
| `ts/packages/adapter-telegram` | 1,124 | Acceptable platform adapter, with normalization/routing policy to keep narrow. |
| `ts/packages/adapter-mcp` | 1,121 | Acceptable MCP adapter/executor, but portable metadata and collision policy should be Rust/codegen-validated. |
| `ts/packages/adapter-tui` | 939 | Acceptable diagnostic/UI adapter. |

Largest non-smoke implementation hotspots:

| File | LOC | Why it matters |
| --- | ---: | --- |
| `ts/packages/brain-island/src/service-app.ts` | 11,815 | Central composition, route dispatch, admin mutation orchestration, chat projection, background loops, adapter bridging, wake dispatch glue. |
| `ts/packages/native-bridge/src/index.ts` | 6,220 | Handwritten raw/native mapping layer where field-level bridge drift can hide. |
| `ts/packages/brain-island/src/rusty-view-chat-api.ts` | 3,171 | Chat HTTP contract, parsing, event summaries, mutation request parsing, and response envelopes. |
| `ts/packages/brain-island/src/service-runtime-config.ts` | 2,313 | Runtime graph parsing, expansion, bridge registration, scheduled jobs, sessions, tool resolver construction. |
| `ts/packages/brain-island/src/service-roleplay-routes.ts` | 2,087 | Remaining roleplay session, fork, branch, variant, narrator, and alternative-generation route orchestration. |
| `ts/packages/brain-island/src/roleplay/lore-routes.ts` | 2,017 | Route adapter over Rust storage, but still owns lore request/control normalization and multi-call route semantics. |
| `ts/packages/brain-island/src/api-command-registry.ts` | 1,522 | Route/command/capability catalog with policy-visible metadata. |
| `ts/packages/brain-island/src/tool-registry.ts` | 1,491 | Tool policy metadata, validation, selection inventory, denial/status explanations. |
| `ts/packages/brain-island/src/brain-module.ts` | 1,411 | Brain module registry plus OpenAI Responses bridging/tool output policy. |
| `ts/packages/brain-island/src/admin-control-api.ts` | 1,311 | Admin control command routing and effect envelopes. |

## P0: Move Authority Out First

These areas still make durable decisions in TypeScript. They should be turned
into Rust operations, Rust crates, or generated contracts before further feature
growth.

### 1. Service composition and control orchestration

Current home:

- `ts/packages/brain-island/src/service-app.ts`
- `ts/packages/brain-island/src/service-runtime-config.ts`
- `ts/packages/service-host/src/index.ts`

Authority still in TypeScript:

- startup composition and storage boot gating;
- profile create/update apply ordering;
- model-provider refresh impact application;
- session archive/recreate/rebuild orchestration;
- scheduler/manual run/control wiring;
- dynamic Den delivery channel recording;
- background loop lifecycle and service drain behavior;
- chat event retention/projection helpers;
- cross-surface route dispatch ordering.

Target:

- Rust should own control-plane apply plans and lifecycle effects.
- `service-host` should own process wiring, adapter injection, HTTP listener
  startup, and drain-loop hosting.
- `brain-island` should expose brain/tool/provider adapters and framework-neutral
  handlers, not whole-service orchestration.

Good first slices:

1. Move profile/session/provider refresh apply plans behind bridge operations
   that return explicit summaries.
2. Move chat event projection into a Rust projection port once event schemas are
   stable.
3. Move scheduler/background host-run state transitions out of service-app glue.

### 2. Runtime/profile config graph

Current home:

- `ts/packages/brain-island/src/service-runtime-config.ts`
- `ts/packages/brain-island/src/profile-loading.ts`
- `ts/packages/brain-island/src/profile-registry-admin.ts`
- `ts/packages/brain-island/src/service-profile-registry-routes.ts`

Already partially Rust-owned:

- `planRuntimeConfigWithRust`
- `planCreateProfileWithRust`
- bridge registration of sessions, brains, jobs, channel bindings, and MCP
  bindings.

Remaining TypeScript authority:

- profile file metadata parsing and runtime graph expansion;
- default session/tool/resource values;
- derived scheduled jobs from background review;
- derived MCP bindings from profile config;
- tool resolver construction and selected tool descriptors;
- mutable profile-file writes and profile/runtime config synchronization.

Target:

- `core-config` remains the Rust home for runtime-affecting graph validation,
  expansion, and apply plans.
- TypeScript should keep profile file discovery, prompt/soul/skill file loading,
  and provider SDK configuration.
- Generated or fixture-checked contracts should replace duplicate hand-written
  TS shapes as they stabilize.

### 3. Brain module/provider loop authority

Current home:

- `ts/packages/brain-island/src/brain-module.ts`
- `ts/packages/brain-island/src/bridge-wake.ts`
- `ts/packages/brain-island/src/pi-agent-brain.ts`
- `ts/packages/brain-island/src/narrator-brain.ts`
- `crates/brains/openai-responses` through bridge operations

Already moving:

- OpenAI Responses has a Rust brain path and bridge stream operations.
- ADR 0021 makes Rust and TypeScript brain modules peers behind the neutral
  wake/stream/action/provider-state contract.

Remaining TypeScript authority:

- default brain module selection;
- OpenAI Responses tool request preparation and tool-output failure policy;
- provider-state fallback metadata and debug sampling glue;
- local/pi-agent brain fallback behavior;
- roleplay narrator brain execution.

Target:

- New durable provider loops should prefer Rust brain modules under
  `crates/brains/`.
- TypeScript may keep pi-agent integration and JS-only provider/tool glue, but
  should not be the central default for provider semantics that Rust can own.
- Tool-call policy and terminal/failure stream semantics should be enforced in
  Rust or in generated protocol tests, not only in TypeScript helpers.

### 4. Tool registry, tool profile, and MCP catalog policy

Current home:

- `ts/packages/brain-island/src/tool-registry.ts`
- `ts/packages/brain-island/src/tool-profile-selection.ts`
- `ts/packages/brain-island/src/tool-session-selection.ts`
- `ts/packages/brain-island/src/local-tool-profiles.ts`
- `ts/packages/adapter-mcp/src/mcp-discovery.ts`
- `fixtures/tool-registry/default-tool-registry-metadata.json`
- `crates/core/core-tool-registry`

Already documented target:

- Portable metadata and validation move toward Rust/codegen.
- TypeScript keeps executable bindings and JS/MCP tool execution.

Remaining TypeScript authority:

- built-in tool metadata source of truth;
- collision, alias, deprecation, capability, and inventory status validation;
- selected/denied/shadowed/missing tool explanations;
- local tool profile parsing and selection;
- dynamic MCP metadata conversion and schema sanitization.

Target:

- Rust owns canonical portable metadata validation and inventory policy.
- TypeScript emits or consumes generated metadata artifacts and keeps
  `implementationModule`/executor bindings.
- MCP dynamic tool conversion should pass through the same Rust/codegen
  validator before exposure.

### 5. Chat, transcript, branch, variant, attachment, and data-bank route logic

Current home:

- `ts/packages/brain-island/src/rusty-view-chat-api.ts`
- chat helper sections inside `ts/packages/brain-island/src/service-app.ts`
- `docs/chat-authority-boundary-classification-2026-07-06.md`

Already Rust-owned:

- message slots and variants;
- conversation branches and snapshots;
- wake dispatch core path;
- session lifecycle state;
- profile registry mutation planning.

Remaining TypeScript authority:

- chat event projection and replay semantics;
- route-level mutation ordering across branch/slot/variant operations;
- default branch creation and branch-head update orchestration;
- attachment and data-bank request semantics;
- command execution routing and output envelopes;
- event cursor/page semantics that may become durable frontend contract.

Target:

- Rust projection/read-model ports for chat event views and cursor replay.
- Rust domain operations for multi-step transcript mutations where consistency
  matters.
- Generated OpenAPI/bridge-backed TS types for public chat envelopes.

### 6. Roleplay domain leftovers

Current home:

- `ts/packages/brain-island/src/service-roleplay-routes.ts`
- `ts/packages/brain-island/src/roleplay/lore-routes.ts`
- `ts/packages/brain-island/src/narrator-brain.ts`
- `ts/packages/brain-island/src/lore-memory-tool.ts`
- `ts/packages/brain-island/src/scene-state-tool.ts`
- `crates/roleplay/roleplay-core`

Already Rust-owned:

- assistant alternative planning;
- prompt context and speaker identity assembly;
- character/persona write and merge validation;
- session metadata patch validation;
- narrator config normalization;
- lore persistence operations.

Remaining TypeScript authority:

- roleplay session create/fork/archive orchestration;
- chat layer binding side effects;
- alternative-generation prompt construction and model call path;
- variant writes, active selection, and branch-head update ordering;
- lore request/control normalization in `roleplay/lore-routes.ts`;
- lore tools and scene state tool behavior.

Target:

- Keep HTTP/browser envelopes in TS.
- Continue expanding `crates/roleplay/roleplay-core` for deterministic roleplay
  validation, prompt/context assembly, branch/variant invariants, lore controls,
  scene state, and generated contract shapes.
- Keep narrator model execution in TS only until a deliberate Rust narrator
  brain module exists.

### 7. Memory, capture, curator, and governance surfaces

Current home:

- `ts/packages/brain-island/src/memory-space-api.ts`
- `ts/packages/brain-island/src/dense-profile-memory-tool.ts`
- `ts/packages/brain-island/src/den-memory-tools.ts`
- `ts/packages/brain-island/src/capture-memory-proposals.ts`
- `ts/packages/brain-island/src/capture-producer-provider.ts`
- `ts/packages/brain-island/src/curator-*.ts`
- `ts/packages/brain-island/src/background-memory-skill-review.ts`
- `docs/memory-surface-boundaries-2026-07-05.md`

Already Rust-owned or intended:

- Crew memory lives in Crew service storage.
- Session memory prompt selection is Rust-selected.
- Den memory remains an external adapter surface, not Crew storage.

Remaining TypeScript authority:

- memory-space admin request semantics;
- governance proposal/decision envelopes;
- capture provider output normalization;
- curator candidate/action/execution safeguards;
- background review trigger shape and scheduling glue;
- dense/lore/session memory tool result shaping.

Target:

- Rust storage and governance repositories should own durable proposal,
  decision, policy, conflict, and retention semantics.
- TypeScript should keep external memory client calls, model-callable tool
  wrappers, and prompt presentation.
- Availability policy should be decided before wake by Rust/tool-selection
  authority where possible, with TS clients treated as dependencies.

### 8. Admin control, diagnostics, slash commands, and API capability catalogs

Current home:

- `ts/packages/brain-island/src/admin-control-api.ts`
- `ts/packages/brain-island/src/admin-diagnostics-api.ts`
- `ts/packages/brain-island/src/api-command-registry.ts`
- `ts/packages/brain-island/src/slash-command-router.ts`
- `ts/packages/brain-island/src/slash-command-responses.ts`
- `ts/packages/brain-island/src/runtime-diagnostics.ts`
- `ts/packages/brain-island/src/storage-query-catalog.ts`

Remaining TypeScript authority:

- command capability catalog as hand-authored policy;
- admin control command routing and mutation dispatch;
- slash-command side effects that call lifecycle/session controls;
- diagnostics projection rules that become operator truth;
- storage query catalog scope and response shapes.

Target:

- TS can keep command parsing/autocomplete and UI/API envelopes.
- Mutation effects should be explicit Rust control-plane commands.
- Capability/catalog metadata should become generated or Rust-validated.
- Diagnostics should be projections from Rust-owned state where the state is
  durable service authority.

### 9. Native bridge hand-written mapping layer

Current home:

- `ts/packages/native-bridge/src/index.ts`
- `ts/packages/native-bridge/src/bridge-validation-schemas.ts`
- `ts/packages/contracts/src/index.ts`
- `crates/bridge/core-bridge-api/bridge-manifest.toml`
- `crates/bridge/core-bridge-codegen`

Already improved:

- operation-name parity checks;
- native surface checks;
- fixture drift checks;
- wire-shape fingerprint checks;
- validation coverage ratchet work.

Remaining TypeScript authority/risk:

- thousands of lines of manual `Raw*` mapping and optional-field conversion;
- field-level bridge drift still possible outside validated fixture/schema
  coverage;
- committed contract shapes still require synchronized edits.

Target:

- Generate more of `contracts`, `native-bridge` raw mappings, validators, and
  fixture schemas from the bridge manifest/Rust types.
- Keep TS loader, runtime validation toggles, and ergonomic wrapper surface.
- Treat all new bridge operations as requiring schema or documented exemption.

## P1: Thin TypeScript, But Do Not Automatically Move All Of It To Rust

These areas should be decomposed or guarded, but parts of them are valid TS
adapter/config code.

### Platform adapters

Current homes:

- `ts/packages/adapter-den`
- `ts/packages/adapter-telegram`
- `ts/packages/adapter-mcp`
- `ts/packages/adapter-tui`

Keep in TS:

- HTTP/SSE/WebSocket/client SDK calls;
- external message normalization;
- external transport retries and auth headers;
- model-callable wrappers for JS-only adapter clients;
- TUI/debug surface rendering.

Audit/refactor candidates:

- `adapter-den/src/channel-routing.ts` currently resolves route candidates,
  mention disambiguation, and ambiguity status in TS. If channel routing becomes
  durable coordination policy, move route resolution into Rust or make TS call a
  Rust route-decision operation.
- `adapter-den/src/den-product-ingress.ts` denies lifecycle operations and
  maps product refs to work refs. That policy is fine as anti-corruption glue
  only if Rust remains authoritative for lifecycle effects.
- `adapter-mcp/src/mcp-discovery.ts` owns model-tool naming, schema wrapping,
  and argument normalization. Keep executor glue in TS, but validate portable
  metadata/collision policy through Rust/codegen.
- `adapter-telegram/src/index.ts` owns external update normalization. Keep it
  as adapter code, but avoid letting it decide internal wake/lifecycle policy.

### Local and browser/web tools

Current homes:

- `local-code-tools.ts`
- `patch-tool.ts`
- `web-tools.ts`
- `browser-tools.ts`
- `browser-session-manager.ts`
- `skills-tools.ts`
- `mcp-brain-tools.ts`

Keep in TS:

- Node subprocess/file/browser/client execution;
- JS package integration;
- model-callable wrapper presentation.

Move or guard:

- capability/safety metadata;
- session/profile/resource denial policy;
- durable telemetry/diagnostic schema;
- tool result lifecycle effects that mutate Rust-owned state.

### Profile and prompt assembly

Current homes:

- `profile-loading.ts`
- `profile-role-assembly.ts`
- `context-strategy.ts`
- `context-estimate.ts`
- `delegated-role-assembly.ts`

Keep in TS:

- loading Markdown/YAML/profile assets;
- prompt rendering and role text composition while pi-agent remains TS;
- JS-side provider prompt quirks.

Move or validate in Rust:

- runtime-affecting profile defaults;
- tool availability policy;
- branch/session memory selection policy;
- delegated role invariants if they affect lifecycle or authority rather than
  prompt wording.

## P2: Mostly Contract/Test Debt

These are not the main authority risk, but they make migration harder:

- 132 smoke scripts under `brain-island/src/` blur production dependency
  boundaries. Relocate smokes out of `src/`, declare real dependencies, and
  remove boundary-check skips.
- `ts/packages/contracts/src/index.ts` should trend toward generated contracts,
  especially for bridge and public API envelopes.
- `api-command-registry.ts`, API OpenAPI files, and route contract tests should
  share generated schema sources where possible.
- `service-host` smoke scripts currently still reach into `brain-island`
  internals; keep host smokes at package boundaries.

## Recommended Migration Order

1. Finish service composition decomposition enough that `service-host` owns host
   lifecycle and `brain-island` stops being the runtime service shell.
2. Add Rust/codegen ownership for portable tool metadata and inventory policy,
   leaving executable bindings in TS.
3. Move chat projection and multi-step transcript/branch mutations behind Rust
   projection/domain operations.
4. Continue roleplay migration in `crates/roleplay/roleplay-core`, especially
   lore controls, scene state, variant/branch invariants, and alternative
   generation orchestration.
5. Push memory governance/proposal/curator semantics into Rust-owned storage and
   policy ports; keep external Den memory as an adapter.
6. Reduce `native-bridge/src/index.ts` by generating raw mappings and validators
   from the bridge manifest.
7. Prefer Rust brain modules for new durable provider loops, while leaving
   pi-agent and JS-only provider/tool glue in TS until replaced deliberately.

## Acceptance Test For Future Refactors

For each TypeScript module touched, classify the remaining code as one of:

- route envelope or UI/API compatibility;
- external adapter/client SDK glue;
- executable tool binding;
- prompt/profile rendering;
- generated or validated contract surface;
- temporary adapter over Rust-owned authority.

If a module still decides lifecycle, validation, selection, storage semantics,
multi-record mutation ordering, wake behavior, or provider-loop semantics, it is
still authority code and should either move to Rust or be backed by an explicit
Rust operation with tests.
