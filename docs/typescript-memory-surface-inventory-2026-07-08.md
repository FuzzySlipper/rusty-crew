# TypeScript Memory Surface Inventory

Date: 2026-07-08
Task: 4707

This document classifies the remaining TypeScript memory-related production
surface after the first Rust authority migration slices in task series 4585.
It is a boundary inventory, not a new architecture proposal.

The rule of thumb is:

- Rust owns durable Crew memory policy, proposal normalization, storage,
  governance decisions, tool availability policy, and deterministic transition
  checks.
- TypeScript may own model-callable wrappers, external adapter calls,
  filesystem/source discovery, HTTP/admin envelopes, and provider glue.
- TypeScript must not add new hidden memory policy without either calling a
  Rust planner/validator or recording a follow-up migration task.

## Current Rust Policy And Storage Boundaries

| Boundary | Rust owner | TypeScript caller |
| --- | --- | --- |
| Typed memory spaces, proposal validation, governance mode selection | `crates/core/core-protocol/src/memory_space.rs` | `memory-space-api.ts`, `capture-memory-proposals.ts`, background review |
| Profile/session memory, memory proposals, governance decisions | `crates/core/core-persistence` through `core-engine` and native bridge | `memory-space-api.ts`, dense/session memory tools, admin routes |
| Capture proposal normalization | `plan_capture_memory_proposals` native operation | `capture-memory-proposals.ts`, `capture-producer-provider.ts`, `background-memory-skill-review.ts` |
| Curator candidate approval/apply preflight | `plan_curator_governance_transition` native operation | `curator-mutations.ts` |
| Curator candidate lifecycle planning | `plan_curator_lifecycle_transition` native operation | `curator-lifecycle.ts` |
| Background memory auto-mutation guard | `plan_background_memory_auto_mutations` native operation | `background-memory-skill-review.ts` |
| External memory tool availability | `plan_tool_availability` native operation in `core-tool-registry` | `profile-loading.ts`, `service-runtime-config.ts` |
| Roleplay lore deterministic policy and storage | `crates/roleplay/roleplay-core` plus persistence repos | `lore-memory-tool.ts`, `scene-state-tool.ts`, roleplay routes |

## Production TypeScript Classification

| File or surface | Classification | Allowed TypeScript authority | Required Rust boundary |
| --- | --- | --- | --- |
| `memory-space-api.ts` | Crew-memory wrapper and UI/admin route glue | Parse tool/admin inputs, call bridge, render model/admin result text. | Must call native memory-space/proposal/session-memory operations for storage and validation. No raw SQL or local proposal policy. |
| `dense-profile-memory-tool.ts` | Crew-memory wrapper | Present profile memory CRUD as a model-callable tool and map missing bridge/client cases to tool results. | Profile memory CRUD and revision behavior are native bridge operations. Tool availability should be selected before wake when possible. |
| `den-memory-tools.ts` | External-memory adapter wrapper | Call the external Den memory client and shape model-callable results. Preserve benign policy denials as tool output when the tool is intentionally exposed. | Availability and coarse exposure/narrowing are decided by `plan_tool_availability` before wake. Den memory is not Crew storage. |
| `lore-memory-tool.ts` | Roleplay-lore domain wrapper | Present lore read/capture/promote operations as model tools. Translate tool parameters and result text. | Roleplay lore read/write/recall/capture/promotion policy goes through native roleplay/lore operations. Do not fold lore into generic Crew memory. |
| `scene-state-tool.ts` | Roleplay-lore domain wrapper | Present scene state reads and updates as model tools. | Scene state defaults, merge/normalization, and invalid input policy go through native roleplay scene-state operations. |
| `session-search-tool.ts` and runtime-search tools | Runtime-search wrapper | Present search results and model-facing summaries. | Runtime search indexing/query semantics are native storage/query operations. Runtime search is not memory. |
| `capture-memory-proposals.ts` | Provider glue and Rust planner adapter | Convert TS/LLM candidate shapes into the native planner request and expose typed TS helper types. | Accepted proposal envelopes, dedupe keys, target space checks, and diagnostics come from `plan_capture_memory_proposals`. |
| `capture-producer-provider.ts` | Provider glue | Call an LLM/provider and parse minimally bounded JSON from the provider. | Must not create durable proposal envelopes directly; accepted proposals go through `plan_capture_memory_proposals`. |
| `background-memory-skill-review.ts` | Provider glue and review orchestration | Gather diagnostics/skills/digests, call optional provider/capture planner, assemble review findings, publish observation. | Memory proposal creation goes through capture planner; persistence goes through native proposal/governance APIs. Any auto-mutating request must go through `plan_background_memory_auto_mutations` before it can be treated as accepted. Scheduling/lifecycle is outside this file's authority. |
| `curator-candidates.ts` | Discovery and preview reporting | Dry-run discovery of candidate skills/memory review findings, source refs, report rendering. | Discovery must not mutate. Any approval/apply transition must go through `plan_curator_governance_transition`. |
| `curator-lifecycle.ts` | Filesystem/source lifecycle glue | Gather factual source-current, pinned-file, and elapsed-time inputs, then apply Rust-returned lifecycle plans to filesystem curator candidates. | Stale/reactivation/archive/skipped decisions and stable reason codes come from `plan_curator_lifecycle_transition`. Memory-affecting approval/apply still requires Rust transition preflight before mutation. |
| `curator-mutations.ts` | Filesystem mutation executor and Rust transition wrapper | Snapshot files, run skill-management dry-run/apply, rollback snapshots, and persist local curator state. | Preview/approve/apply acceptance, stale/expired/unapproved/approval-fingerprint denial, audit refs, and receipt ids come from `plan_curator_governance_transition`. |
| `curator-admin-control.ts` | UI/admin route glue | Map admin commands to curator executor calls and format outcomes. | Mutating control planning is handled by the admin-control migration series, not by memory policy code. |
| `service-roleplay-routes.ts` and `roleplay/lore-routes.ts` | UI/admin route glue and roleplay domain adapter | HTTP parsing/envelopes, multi-call route composition, result formatting. | Lore/session/branch/variant policy should use roleplay-core planners and persistence; remaining route normalization is tracked by roleplay Rust-authority tasks. |
| `service-app.ts` memory/admin route sections | Composition and UI/admin route glue | Route dispatch, service state wiring, diagnostics envelope assembly. | Durable memory mutation and storage must call native bridge APIs. Do not add memory policy directly in route handlers. |
| `service-runtime-config.ts` memory resolver section | Provider glue and tool resolver composition | Create external-memory client, resolve model-callable tool implementations, pass availability facts to Rust planner. | Tool exposure/narrowing is `plan_tool_availability`; actual Crew storage is native bridge. |
| `profile-loading.ts` tool availability hook | Tool/profile assembly glue | Re-run tool selection with Rust-returned resource denials. | Selection decisions for external memory availability must come from `plan_tool_availability`. |
| `tool-profile-selection.ts` | Transitional tool inventory glue | Build inventory and expose denial reasons. | Public policy should continue moving into `core-tool-registry`; TS may not special-case memory availability itself. |
| `tool-context-diagnostics.ts` | Diagnostics projection | Render availability and selection diagnostics for operators/model context. | Diagnostics must reflect Rust/native policy decisions; they are not policy authority. |

## Remaining TypeScript Policy Owners

The following TypeScript surfaces still contain policy-adjacent logic. They are
allowed for now only under the listed constraint.

| Surface | Current policy risk | Disposition |
| --- | --- | --- |
| `background-memory-skill-review.ts` | Review finding heuristics and provider-gating order are still TS-owned. | Acceptable as provider/review orchestration. Durable proposal shape is Rust-planned, and any future auto-mutating request must pass `plan_background_memory_auto_mutations`; provider/model output is not directly trusted as a durable mutation instruction. |
| `service-roleplay-routes.ts` and `roleplay/lore-routes.ts` | Route-level multi-call semantics and some request normalization remain TS-owned. | Covered by the roleplay Rust-authority migration track. Do not add generic memory behavior here. |
| `tool-profile-selection.ts` | General tool inventory remains TS-built after Rust availability planning. | Covered by the tool registry Rust-authority migration. External memory availability is already Rust-planned. |

Task 4708 is the immediate ratchet for the most confusing production case: a
profile with Den document/task/guidance MCP tools and external memory tools.
That task should prove Den docs/tasks use MCP document/task tools, while memory
questions use memory tools only when the external memory policy exposes them.

## Rules For New TypeScript Memory Wrappers

1. Name the surface category in the file doc or nearby docs before adding a new
   memory-adjacent wrapper.
2. If it persists, approves, rejects, promotes, archives, dedupes, selects, or
   enforces availability, call a Rust/native planner or repository.
3. If it calls Den, MCP, a provider, the filesystem, or HTTP, keep that adapter
   call in TypeScript but pass only factual dependency/source state into Rust.
4. Do not use Den memory as fallback Crew storage.
5. Do not describe Den document/task/guidance tools as memory tools.
6. Do not fold roleplay lore or runtime search into generic memory without a
   new architecture decision.

## Suggested Validation

- `cargo test -p rusty-crew-core-protocol`
- `cargo test -p rusty-crew-core-tool-registry`
- `npm run smoke:profile-loading -w @rusty-crew/brain-island`
- `npm run smoke:tool-profile-selection -w @rusty-crew/brain-island`
- `npm run smoke:capture-memory-proposals -w @rusty-crew/brain-island`
- `npm run smoke:background-memory-skill-review -w @rusty-crew/brain-island`
- `npm run smoke:curator-lifecycle -w @rusty-crew/brain-island`
- `npm run smoke:curator-mutations -w @rusty-crew/brain-island`
- `npm run smoke:curator-review-e2e -w @rusty-crew/brain-island`
