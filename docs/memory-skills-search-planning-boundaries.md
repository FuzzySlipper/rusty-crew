# Memory, Skills, Search, And Planning Authority Boundaries

Task: Den `2896`

This note grounds the `2818` memory, skills, session-search, and planning tool
slice. It applies the existing Rusty Crew rule: Rust owns deterministic
coordination authority and durable runtime facts; TypeScript owns
model-callable tool expression, adapter clients, and profile/role assembly.

## Boundary Summary

| Surface | Authority | Tool execution | Durable store |
| --- | --- | --- | --- |
| Den Memories | Den | TypeScript adapter/tool client | Den Memories service |
| Dense profile memory | Rusty Crew runtime/profile state | TypeScript tool wrapper over Rust APIs | Rust coordination persistence, if durable |
| Skills | Profile/skill package state | TypeScript profile/tool layer | Filesystem or profile package store |
| Session search | Rusty Crew runtime history | TypeScript tool wrapper over Rust query APIs | Rust coordination persistence/search index |
| Todo planning | Session-local planning state | TypeScript tool wrapper over bounded runtime APIs | Volatile or Rust-owned bounded session state |
| Runtime counters | Derived runtime projection | TypeScript reset/request wrapper over Rust APIs | Rust coordination persistence |
| Curator execute | Governance/control loop | Narrow TypeScript control wrapper | Audited governance/control records |

No tool in this slice should inspect SQLite files, bypass the canonical tool
registry, mutate Den product data as if it were runtime state, or use Den
observability as lifecycle authority.

## Den Memories

Den Memories are Den-owned external memory. Rusty Crew may call them, but it
does not own their source of truth.

Implementation boundary:

- `adapter-den` owns the Den Memories HTTP client and request/response
  normalization.
- `brain-island` owns model-callable tools such as `den_memory_recall`,
  `den_memory_read`, `den_memory_search`, `den_memory_store`, and
  `den_memory_propose`.
- The canonical tool registry owns names, categories, toolsets, safety flags,
  and diagnostics for these tools.
- Rust should only see Den memory tool use as tool telemetry/events and selected
  tool descriptors.

Policy:

- Read/search/recall can be ordinary read tools.
- Store/propose are external-write tools and must be profile-selected
  deliberately.
- Den memory failures should degrade tool results/diagnostics, not runtime
  scheduling.
- Den Memories must not be mirrored into Rust coordination persistence except as
  bounded tool-call telemetry.

## Dense Profile Memory

Dense profile memory is Rusty Crew runtime/profile state. It is not Den
Memories with another name.

Implementation boundary:

- Rust owns durable dense profile memory records if the feature is enabled.
- Access should flow through typed bridge/persistence APIs, not raw SQL.
- TypeScript may expose `dense_profile_memory` as a model-callable tool, but the
  tool is a wrapper over Rust-owned state.

Policy:

- Scope records by profile and, where needed, runtime/project identity.
- Keep writes auditable and bounded.
- Treat dense profile memory as runtime assistance, not a substitute for Den
  product/task truth.
- If summarization or compaction is added later, the resulting summary must
  remain rebuildable or clearly marked as derived state.

## Skills

Skills are profile/skill package state loaded into role assembly. They are
closer to source/configuration than runtime memory.

Implementation boundary:

- TypeScript profile loading owns skill discovery and view behavior.
- `skills_list` and `skill_view` are read-only brain tools over configured skill
  roots.
- `skill_manage` is a governed write tool, not an arbitrary filesystem editor.

Governance requirements:

- Respect configured skills roots and reject path traversal.
- Preserve `.pinned` protection.
- Support delete only with explicit semantics, including `absorbed_into` when a
  skill is intentionally merged into another.
- Prefer patch/write operations that validate frontmatter and stable metadata.
- Record management actions in tool telemetry and future audit hooks.

Skills should not become a hidden prompt injection channel. Role assembly should
continue to make selected skills explainable through context diagnostics.

## Session Search

`session_search` searches Rust-owned runtime history. It does not search Den
tasks, Den documents, Den memories, or arbitrary product data.

Implementation boundary:

- Rust persistence owns runtime search indexing behind typed
  `CoordinationStore` APIs.
- SQLite FTS5 is an implementation detail behind that API.
- TypeScript exposes a brain tool that calls the typed runtime search surface.

Scope:

- Routed agent messages.
- Session configuration snapshots.
- Future runtime event/search rows that have an explicit retention design.

Out of scope:

- High-volume brain text deltas until retention/compaction is designed.
- Direct SQL/FTS query strings from TypeScript.
- Den product data search.

## Todo Planning

`todo` is bounded session planning state. It is not Den task truth and should
not compete with Den tasks.

Implementation boundary:

- TypeScript owns the model-callable planning tool shape and role-assembly
  rendering.
- Rust should own durable or restart-safe todo state if it needs to survive
  wakes/restarts.
- A volatile in-memory implementation is acceptable for early proofs if clearly
  scoped and paired with follow-up tasks before treating it as durable.

Policy:

- Scope todos to session/run context.
- Bound item count and text size.
- Prefer statuses such as `planned`, `in_progress`, `blocked`, and `done`.
- Do not sync to Den tasks unless a future explicit bridge says so.

## Runtime Counters And Counter Reset

Runtime counters are derived projections for health/debug views. They are not
runtime truth.

Implementation boundary:

- Rust owns counter persistence/rebuild/reset APIs.
- TypeScript may expose `counter_reset` as a privileged planning/debug tool only
  through explicit profile selection and control policy.

Policy:

- Reset should rebuild or clear derived counters from owned runtime facts.
- Reset must not delete event logs, messages, completion packets, tool
  telemetry, or session records.
- Counter reset is a coordination-action tool, not an ordinary model helper.

## Curator Execute

`curator_execute` is a governance/control affordance. It must not become a side
channel around future curator/governance loops.

Implementation boundary:

- TypeScript may expose a narrow model-callable wrapper.
- Actual effects must route through explicit governance/control APIs with audit
  evidence.
- The tool should return command receipts, not silently mutate state.

Policy:

- Require narrow command names and typed arguments.
- Require actor/session identity.
- Emit audit and observation records when configured.
- Reject broad arbitrary command execution.
- Prefer read-only dry-run/evaluate modes before write modes.

## Registry And Diagnostics Rules

Every tool from this slice must be registered in the canonical TypeScript tool
registry before it is injected into a brain. Tool diagnostics should explain:

- selected versus denied tools;
- read-only/resource denials;
- external-write/governance requirements;
- missing Den Memory client configuration;
- unavailable Rust search/counter APIs;
- skill root write protections;
- todo durability mode.

These diagnostics are the supported way to debug why an agent has or lacks a
memory, skill, search, or planning capability.

## Implementation Order

1. Define this boundary note.
2. Add Den Memories client in `adapter-den`.
3. Add read/write Den memory tools in `brain-island`.
4. Register all memory/skill/search/planning tools through the canonical
   registry.
5. Add dense profile memory, session search, todo, and counter APIs behind Rust
   persistence or clearly bounded volatile state.
6. Add skill read tools, then governed `skill_manage`.
7. Add context assembly/diagnostics for selected memory, skills, todo, and
   planning state.
8. Prove the memory/skills and planning/search/counter surfaces through real
   brain wakes.
