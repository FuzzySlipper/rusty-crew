# Memory Surface Boundaries

Status: design note for Den task 4195
Date: 2026-07-05

## Problem

Rusty Crew now has several useful memory-like surfaces, but they do not all mean
the same thing. A live `asha-planner` test showed the danger: the agent treated
memory tools as if they were Den document/task lookup tools, then hit an
unavailable memory client and lost the useful path.

The fix is not one giant memory abstraction. Rusty Crew needs a small naming and
ownership contract so agents, UIs, and tool-selection diagnostics can tell which
surface answers which question.

## Principles

- Crew-owned memory lives in the Crew service database and is governed by Crew
  storage, prompt, and tool policy.
- Den-owned memory lives in Den services. It is external reference memory, not
  Den documents, Den tasks, Den guidance, or Crew-local runtime memory.
- Model-callable tools should use user-intent names, not backend names. Backend
  provenance belongs in tool metadata, diagnostics, and evidence refs.
- Unavailable external clients should not be exposed as attractive broken tools.
  Operator diagnostics should report the missing dependency; model inventories
  should omit or clearly deny the tool before a turn starts.
- Domain memory can stay domain-specific. Roleplay lore should not be forced
  into a generic blob, but it should still fit the typed memory-space governance
  model.

## Inventory

| Surface | Ownership | Storage | Green-path language | Current access |
| --- | --- | --- | --- | --- |
| Profile soul / prompt memory | Crew | `profile_registry.prompt_*` records in service DB | "Profile instructions" and "profile prompt memory" | Admin profile prompt APIs and role assembly |
| Dense profile memory | Crew | `profile_memories` | "Compact stable profile memory" | `dense_profile_memory` tool, prompt injection, memory-space descriptor `profile_dense` |
| Session memory | Crew | `session_memory_records` | "Branch-aware session memory" | Rust-selected prompt context and memory-space APIs |
| Memory proposals/governance | Crew | `memory_proposals`, `memory_governance_decisions` | "Memory proposal/review queue" | Admin memory-space API and capture producer direction |
| Roleplay lore | Crew | roleplay lore module tables | "Roleplay lore" / "world and character facts" | roleplay lore tools and browser/admin APIs |
| Runtime search | Crew | runtime search index/read model | "Search prior session/runtime records" | typed runtime search/admin query APIs |
| Den memory | Den | Den memory service | "External memory" or "configured memory service" | `memory_recall`, `memory_search`, `memory_read`, `memory_store`, `memory_propose` backed by Den client |
| Den documents/tasks/guidance | Den | Den planning services | "Den documents", "tasks", "guidance" | MCP/project/task/document tools, not memory tools |
| Skill files | Crew filesystem/config | service-wide/profile skill roots | "Skills" | skills tools and prompt assembly |
| Todo/local planning state | Crew | todo/tool state when enabled | "Session todo state" | todo tools; not durable memory |

## Tool Naming Decision

The model-facing external memory tools should remain generic:

- `memory_recall`
- `memory_search`
- `memory_read`
- `memory_store`
- `memory_propose`

Those names describe the agent's intent and avoid teaching the model that every
memory action is a Den document action. The backend should be described as
"configured external memory service" in model-facing descriptions. Den ownership
should remain visible in non-model diagnostics and evidence refs.

Crew-owned typed memory spaces should keep explicit names until the generic
facade is designed:

- `dense_profile_memory` for compact profile records.
- `memory_space_catalog` / `memory_space_read` for typed memory-space browsing.
- `recall_lore`, `search_lore`, `capture_lore_fact`, `promote_lore_entry` for
  roleplay lore.
- Runtime search tools/routes should say "runtime search" rather than memory.

Do not expose both `den_memory_*` and `memory_*` variants. If compatibility
aliases are needed internally, they must be deprecated, hidden from normal tool
selection, and diagnosed as aliases.

## Prompt Language

The role prompt should avoid a `# Den Memory` section. The model should see:

- `# External Memory` for configured memory-service context.
- `# Dense Profile Memory` for compact Crew profile memory.
- `# Session Memory` for Rust-selected branch-aware session memory.
- Domain headings such as `# Roleplay Lore` when lore is injected.

The external memory prompt must explicitly say it is not Den documents, tasks,
projects, or guidance, and should point agents toward project/task/document MCP
tools for those concepts.

## Availability Policy

Tool availability should be decided before a wake when possible.

- If external memory policy mode is `off`, do not expose external memory tools.
- If the external memory client is missing or unhealthy, do not expose external
  memory tools to the model. Surface `memory_client_unavailable` through admin
  diagnostics and tool-selection diagnostics instead.
- If policy mode is `metadata`, expose read tools only when the client is
  available; hide or deny write/propose tools.
- Non-retryable denials such as missing ids or manual-review policy are normal
  tool results and should not kill the turn.
- Read-only Crew memory tools may be exposed if they can return useful local
  data without an external dependency.

This policy prevents agents from selecting a tool that can only say
`memory_client_unavailable`, while preserving clear operator evidence.

## Ownership Boundaries

Crew memory and Den memory may reference each other only through explicit
provenance:

- A Crew memory proposal can cite a Den memory id as evidence.
- A Den memory result can be shown to the agent as external context.
- No path should silently copy Den memory into Crew storage.
- No Crew memory write should use Den as a fallback storage home.

Roleplay lore should stay Crew-local and typed. It may use the memory-space
descriptor/governance model, but it should retain domain APIs for world, entity,
scene, canon status, visibility, capture, promotion, and recall.

## Implementation Plan

1. Rename model-facing prompt/toolset language from Den-specific memory to
   external/configured memory, while keeping backend provenance in diagnostics.
2. Change external memory resolver selection so unavailable clients are hidden
   before model wake and reported through diagnostics instead of broken tools.
3. Add a read-only admin/tool catalog projection that shows each memory surface,
   owner, storage home, prompt policy, tool names, and availability.
4. Align memory-space catalog descriptions with this note, especially
   `profile_dense`, `session_memory`, and roleplay lore.
5. Add live and deterministic tests for a profile that has Den docs/task MCP
   tools plus external memory tools, proving the model distinguishes document
   lookup from memory lookup.
6. Plan the later typed-memory facade only after the above is stable; do not
   collapse roleplay lore or runtime search into generic memory prematurely.

## Acceptance For Future Changes

- Agents must not need to infer backend ownership from a tool name.
- A missing external memory client must be obvious in diagnostics and absent
  from normal model tool selection.
- Den document/task/guidance tools must remain separate from memory tools.
- Roleplay lore and session/profile memory must remain Crew-owned even when
  they cite Den evidence.
