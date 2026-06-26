# ADR 0018: Typed Memory Space API And Governance

Status: Proposed for task 3374

Date: 2026-06-25

## Context

Rusty Crew already has dense profile memory: Rust-owned persistence,
revision-checked writes, caps, native bridge methods, prompt injection, and a
profile-gated brain tool. Roleplay lore, session memory, and future user memory
need similar durability, but they should not become one-off table islands or an
untyped generic blob memory system.

The storage strategy recommends one Crew service database per instance, a
Rust-owned storage boundary, module-owned data through a schema registry, and
safe read-only query surfaces. The capture decision producer design recommends
post-wake proposals for durable captures, but its initial target shape is dense
profile memory only.

This ADR defines the broader memory substrate those pieces should converge on.

## Decision

Rusty Crew will model durable service memory as typed memory spaces. A memory
space is a registered domain contract with:

- a stable id;
- a typed record shape;
- allowed scopes and visibility;
- retrieval/search behavior;
- prompt injection policy;
- write operation policy;
- provenance and evidence requirements;
- retention/archive behavior;
- repository/API contracts;
- diagnostics and query catalog integration.

Memory spaces are service-owned Crew memory. They are distinct from Den-owned
memory, Den documents, Den tasks, and local session todo notes.

Dense profile memory maps to the first existing space, `profile_dense`. It
should be adapted into the memory-space model rather than rewritten first.

Roleplay lore should become a typed memory space, not an external lore service
and not a TS-owned schema island.

## Non-Goals

- Do not create an arbitrary raw memory table that accepts any shape.
- Do not make Den memory and Crew service memory interchangeable.
- Do not let TypeScript issue raw SQL or own memory-space migrations.
- Do not auto-inject every memory record into prompts.
- Do not require roleplay lore implementation before the generic registry and
  governance model are proven.

## Memory Space Descriptor

A memory space descriptor contains:

- `space_id`: stable snake_case id, e.g. `profile_dense`.
- `schema_version`: memory-space schema version.
- `module_id`: owning module schema bundle, when applicable.
- `record_shape`: Rust-owned typed record definition or descriptor reference.
- `scope_model`: allowed scope types and id rules.
- `visibility_model`: who/what may read records.
- `retrieval_strategy`: direct lookup, query/search, recency, relevance,
  branch-aware retrieval, or domain-specific retrieval.
- `indexing`: required and optional indexes/search capabilities.
- `prompt_policy`: when records may be auto-injected, summarized, withheld, or
  exposed only through tools.
- `write_policy`: direct, candidate, manual review, curator route, or read-only.
- `operations`: allowed proposal/write operations.
- `provenance_policy`: required evidence refs and source metadata.
- `retention_policy`: archive, tombstone, compact, expire, manual-only, or
  domain-specific rules.
- `conflict_policy`: revision tokens, supersession, merge rules, or immutable
  records.
- `diagnostics`: row counts, stale candidates, retention pressure, search
  health, and governance queue state.
- `export_import`: stable backup/restore format and validation behavior.

Descriptors are Rust-owned. TypeScript may use descriptor projections for tools
and UI, but may not register physical storage.

## Scope Model

Memory scopes are explicit. V1 should support:

- `profile`: memory owned by a profile/agent persona.
- `user`: memory about an external user/participant, within a profile or app
  context.
- `session`: memory scoped to a durable session.
- `conversation_branch`: branch-aware session memory.
- `world`: roleplay world/campaign scope.
- `entity`: roleplay entity/person/place/object scope.
- `project`: optional service-local project/reference scope, distinct from Den
  product data.

Every memory record has one primary scope and may have secondary tags or links.
Cross-scope propagation is a governed operation, not an implicit copy.

## Operations

Memory proposals and repository writes use a common operation vocabulary:

- `add`: create a new record; fails if an equivalent key/id exists.
- `replace`: overwrite a record using an expected revision.
- `merge`: combine structured fields through a space-defined merge policy.
- `supersede`: mark an old record replaced by a new record without deleting
  history.
- `remove`: remove or tombstone a record using an expected revision.
- `archive`: hide from active retrieval while preserving export/audit history.
- `candidate_only`: create a proposal that cannot auto-apply.

Each memory space declares which operations are allowed and which governance
policy applies to each operation.

## Governance Policy

Supported governance modes:

- `read_only`: records are visible but not writable through runtime tools.
- `direct_write`: trusted path may write immediately after validation.
- `candidate`: writes become proposals and require later review.
- `manual_review`: human/operator review required before apply.
- `curator_route`: proposals flow through curator/background governance.
- `auto_apply_threshold`: non-destructive proposals may apply automatically when
  confidence and policy checks pass.

Default policy is conservative:

- `add` may be direct only for narrow trusted sources or high-confidence
  auto-apply once calibrated.
- `replace`, `merge`, `supersede`, `remove`, and `archive` normally route
  through curator/manual review unless a space explicitly proves safety.
- Domain-critical spaces such as `roleplay_lore` start as candidate/manual
  review for LLM-generated writes.

Governance decisions are recorded with actor, source, evidence refs, policy
mode, confidence, and resulting record revisions.

## Provenance

Durable memory needs evidence. A proposal should include:

- `proposal_id`: stable id or fingerprint.
- `space_id`: target memory space.
- `operation`: operation vocabulary above.
- `scope`: typed target scope.
- `shape`: target record shape id/version.
- `content`: structured content or patch payload.
- `evidence_refs`: wake, event, tool call, transcript, user correction, or
  source-document references.
- `confidence`: 0.0 to 1.0.
- `durability_rationale`: why this is not ephemeral.
- `governance_policy`: requested/selected policy.
- `dedupe_key`: optional fact/entity fingerprint.
- `source`: in-wake tool, capture producer, UI, import, migration, or human.

Applied records retain provenance metadata. Read-only diagnostics and UI should
let operators answer why a memory exists without manual DB spelunking.

## Prompt Injection Policy

Prompt injection is per space and per retrieval result, not global.

Possible policies:

- `auto_context`: small trusted records can be injected automatically.
- `summary_context`: records must be summarized before injection.
- `tool_only`: records are discoverable through tools but not auto-injected.
- `explicit_user_context`: UI/user must attach or select records.
- `never_prompt`: storage/diagnostic only.

`profile_dense` may use `auto_context` because records are compact and capped.
`session_memory` usually uses `summary_context` or branch-aware retrieval.
`roleplay_lore` should use retrieval/prompt assembly rules based on world,
entity, scene, canon status, and visibility.

## Crew Memory vs Den Memory

Crew service memory:

- lives in the Crew service DB;
- is owned by Rusty Crew runtime/storage APIs;
- can be auto-injected into agent prompts when policy allows;
- supports profile/session/roleplay/user runtime behavior;
- is visible through Rusty Crew diagnostics/query catalog surfaces.

Den-owned memory:

- lives in Den services;
- is queried intentionally through Den memory tools;
- can be larger and more document/reference-like;
- follows Den product-data/governance semantics;
- must not be silently copied into Crew service memory.

Bridges are allowed only through explicit import/proposal flows with provenance.
For example, an agent may propose a `profile_dense` record based on Den memory,
but the proposal must cite the Den memory ref and pass Crew memory governance.

## Existing Dense Profile Memory Mapping

Current dense profile memory maps as:

- `space_id`: `profile_dense`
- scope types:
  - `profile`
  - `user`
- record shape:
  - `key`
  - `content`
  - `metadata_json`
  - `revision`
  - `created_at`
  - `updated_at`
- operations:
  - `add`
  - `replace`
  - `remove`
  - `list/read`
- conflict policy:
  - expected revision required for replace/remove
- caps:
  - max records per profile
  - max key bytes
  - max content bytes
- prompt policy:
  - compact auto-injection for current profile/user target when profile config
    enables it
- write policy:
  - tool mode `off`, `read_only`, or `read_write`;
  - capture producer starts as curator/candidate, later high-confidence `add`
    may auto-apply after calibration.

Migration path:

1. Introduce memory-space descriptor projection for the existing dense memory
   API.
2. Add generic proposal types that can target `profile_dense`.
3. Keep existing native bridge methods working as compatibility wrappers.
4. Add generic read/list proposal/query APIs alongside them.
5. Move prompt injection diagnostics to refer to `profile_dense`.
6. Deprecate dense-only capture output once typed proposals are in place.

## Example: `session_memory`

Purpose: durable session facts, summaries, user-visible choices, and branchable
conversation state that should persist across wakes and restarts.

Descriptor:

- `space_id`: `session_memory`
- scopes:
  - `session`
  - `conversation_branch`
- record shapes:
  - `session_fact`
  - `session_summary`
  - `branch_summary`
  - `user_choice`
- retrieval:
  - active session and active branch first;
  - recency and branch ancestry filters;
  - optional full-text search;
- prompt policy:
  - branch summary auto-context;
  - older facts summary-context;
  - raw transcript remains tool/query only;
- write policy:
  - user/UI notes may direct write;
  - LLM-generated summaries are candidate or curator route until proven;
  - branch supersession uses explicit `supersede`.
- retention:
  - active summaries retained;
  - old branch summaries compacted after archive;
  - raw event/transcript retention remains separate runtime policy.

This space should not replace transcript storage. It is the durable summary/fact
layer above transcripts.

## Example: `roleplay_lore`

Purpose: structured roleplay world/lore state inside the Crew DB without an
external lore service.

Descriptor:

- `space_id`: `roleplay_lore`
- owning module: `roleplay_lore`
- scopes:
  - `world`
  - `entity`
  - `session`
  - `conversation_branch`
- record shapes:
  - `world`
  - `entity`
  - `lore_entry`
  - `relationship`
  - `timeline_event`
  - `canon_decision`
- retrieval:
  - world/entity/canon/visibility filters;
  - scene-aware relevance;
  - optional full-text search;
  - provenance lookup for contested facts;
- prompt policy:
  - selected canon lore can be auto-context after retrieval;
  - contested/draft/private lore is explicit-user-context or tool-only;
  - long lore is summarized before prompt injection;
- write policy:
  - UI/human canon edits may direct write with revision checks;
  - LLM suggestions are candidate/manual review by default;
  - `supersede` is preferred over destructive replacement for canon changes;
- retention:
  - tombstone deleted lore until export/compaction;
  - provenance events compact into summaries by policy;
  - archived worlds remain exportable.

Roleplay code in TypeScript may handle prompt assembly and UI flows. Rust owns
stored shapes, revisions, search/query APIs, provenance writes, and migrations.

## Capture Decision Producer Alignment

The capture producer should emit typed memory proposals rather than
dense-profile-only outputs.

Near term, the producer may still target `profile_dense` because it exists. The
output shape should move toward:

- `space_id`
- `operation`
- `scope`
- `shape`
- `content`
- `evidence_refs`
- `confidence`
- `durability_rationale`
- `governance_policy`
- `dedupe_key`

Compatibility with the dense-only producer shape is operation-preserving:

- `dense_memory_add` -> `profile_dense` `add`
- `dense_memory_replace` -> `profile_dense` `replace`
- `dense_memory_remove` -> `profile_dense` `remove`

The compatibility mapper fills the `profile_dense_item` shape and profile scope,
but generated outputs remain proposals. The mapper must not call dense profile
CRUD directly, and approval/apply decisions must stay in the typed governance
path.

Mapping examples:

- user correction about profile environment -> `profile_dense` `add`
- repeated session-level decision -> `session_memory` `add` or `merge`
- proposed character/world fact -> `roleplay_lore` `candidate_only`
- contradiction of old fact -> `supersede` proposal through curator
- stale/incorrect memory -> `remove` proposal through curator/manual review

The first implementation should keep all LLM-generated proposals in curator
review until enough real proposal data exists to calibrate auto-apply.

The capture producer is a Crew memory producer. It may cite Den memory as
evidence through an explicit `den_memory` evidence ref, but it must not write to
Den memory and must not silently copy Den-owned memory into Crew storage.

## API And Bridge Shape

V1 should expose memory-space metadata and proposal/read APIs without forcing
all spaces into one generic blob contract.

Read surfaces:

- `GET /v1/admin/memory/spaces`
- `GET /v1/admin/memory/spaces/{space_id}`
- `POST /v1/admin/memory/query/{space_id}`
- agent read tools over the same query catalog/profile-gated tool registry

Mutation/proposal surfaces:

- Rust bridge types for `MemoryProposal`, `MemoryOperation`, `MemoryScope`,
  `MemoryEvidenceRef`, and `MemoryGovernancePolicy`.
- A proposal repository/API that stores proposals and their review/apply state.
- Space-specific repository APIs for applied records.
- Compatibility wrappers for current dense profile memory CRUD.

Admin/user UI should favor proposals and review flows over raw mutation routes.
Direct writes are still needed for trusted UI/import paths, but those paths
must be typed and audited.

## First Implementation Slice

The first slice should be small and compatible:

1. Add Rust/TS contract types for memory spaces, scopes, operations, proposals,
   governance modes, and evidence refs.
2. Register `profile_dense` as a descriptor over current dense profile memory.
3. Add admin/read-only memory-space catalog projection.
4. Add typed proposal records that can target `profile_dense`, but route all
   generated proposals to curator/manual review initially.
5. Adapt capture producer output design from dense-only kinds to typed memory
   proposals.
6. Add smokes for descriptor projection, proposal validation, profile_dense
   compatibility, and denial of invalid operations/scopes.

`session_memory` and `roleplay_lore` should follow as separate tasks after the
generic descriptor/proposal path is proven.

## Consequences

Positive:

- Roleplay lore gets domain-specific shape without becoming an external
  service.
- Existing dense profile memory survives as the first concrete space.
- Capture producer output can grow beyond dense memory without redesign.
- Frontends and agents get one governance vocabulary for memory proposals.
- Den memory remains clearly separate from Crew runtime memory.

Costs:

- More ceremony than a simple key/value memory store.
- Each durable memory domain needs Rust-owned descriptors and repositories.
- Auto-apply requires real calibration data and conservative defaults.
- Prompt injection becomes a policy surface that needs diagnostics.

## Deferred Decisions

- Exact route names for non-admin user-facing memory APIs.
- Whether proposal storage is generic across all spaces from day one or starts
  with `profile_dense`.
- How roleplay canon review UI maps to curator governance.
- Whether memory-space descriptors are compiled Rust only or later generated
  from manifests.
- How much Den memory import/proposal tooling is exposed to agents.
