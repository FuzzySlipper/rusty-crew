# Session Memory Space Implementation Design

Status: implementation-ready design for Den task 3402

Date: 2026-06-26

Related:

- `docs/adr/0018-typed-memory-space-api-and-governance.md`
- `docs/rusty-view-chat-api-contract.md`
- `docs/runtime-event-log-and-projections.md`

## Purpose

`session_memory` is Crew-owned durable memory for facts, summaries, choices, and
branch-local conversation state that should survive wakes and restarts. It is a
typed memory space on the Rust-owned memory substrate.

It is not transcript storage. Raw messages, message slots, branch heads, and SSE
chat events remain owned by the existing chat/session persistence surfaces.
`session_memory` stores curated records derived from those surfaces or written by
trusted UI/control paths.

## Descriptor

Initial descriptor:

- `space_id`: `session_memory`
- `module_id`: `runtime_memory`
- scopes: `session`, `conversation_branch`
- visibility: `session_scoped`
- retrieval: `direct_lookup`, `recency`, `branch_aware`, optional
  `query_search`
- prompt policy: mixed per record shape; descriptor-level default remains
  `summary_context`
- write policy:
  - user/UI direct notes may use `direct_write` after validation;
  - LLM/capture-producer outputs use `candidate` or `curator_route`;
  - branch compaction and supersession use `curator_route`;
  - destructive removal/archive starts `manual_review`.
- operations: `read`, `list`, `add`, `replace`, `merge`, `supersede`,
  `archive`, optionally `remove` after governance is proven
- provenance: at least one `wake`, `event`, `transcript`, `ui`, or
  `user_correction` evidence ref; generated records require a durability
  rationale
- conflict policy: expected revision plus supersession chains
- retention: manual/archive/compact; no raw transcript deletion

## Record Shapes

### `session_fact`

A durable fact observed inside one session.

Required fields:

- `record_id`: stable id or generated id
- `content`: compact Markdown fact
- `fact_kind`: enum-like string, e.g. `environment`, `preference`,
  `decision`, `constraint`, `correction`, `open_loop`
- `confidence`: number 0.0-1.0
- `source_summary`: short human-readable evidence summary
- `created_at`
- `updated_at`

Optional fields:

- `subject`: normalized subject/fingerprint for dedupe
- `expires_at`: only for explicitly temporary-but-cross-wake facts
- `supersedes_record_id`
- `tags`
- `metadata_json`

Prompt policy: eligible for summary-context when high confidence and relevant to
the active session/branch. Never injected merely because it exists.

### `session_summary`

A rolling summary of the whole durable session.

Required fields:

- `record_id`
- `summary`: Markdown summary
- `coverage_start`: cursor, message id, wake id, or timestamp
- `coverage_end`: cursor, message id, wake id, or timestamp
- `summary_kind`: `rolling`, `checkpoint`, `compaction`, or `handoff`
- `created_at`
- `updated_at`

Optional fields:

- `token_estimate`
- `source_record_ids`
- `supersedes_record_id`
- `metadata_json`

Prompt policy: strongest auto-context candidate for session resumes, but still
bounded by active session and branch state.

### `branch_summary`

A summary specific to one conversation branch.

Required fields:

- `record_id`
- `summary`
- `branch_id`
- `head_message_id`
- `coverage_start`
- `coverage_end`
- `created_at`
- `updated_at`

Optional fields:

- `parent_branch_id`
- `ancestor_branch_ids`
- `supersedes_record_id`
- `token_estimate`
- `metadata_json`

Prompt policy: active branch summary is auto/summary-context. Ancestor summaries
are eligible as compressed context. Sibling branch summaries are tool-only unless
the user/front-end explicitly switches or attaches them.

### `user_choice`

A durable user choice inside a session or branch.

Required fields:

- `record_id`
- `choice`: compact statement of the selected option
- `choice_kind`: e.g. `model`, `plot`, `workflow`, `configuration`,
  `preference`
- `chosen_at`
- `status`: `active`, `superseded`, `reverted`, or `archived`
- `created_at`
- `updated_at`

Optional fields:

- `alternatives`
- `supersedes_record_id`
- `reverted_by_record_id`
- `metadata_json`

Prompt policy: active choices are summary-context for the matching
session/branch. Superseded/reverted choices are normally diagnostics/tool-only.

## Scope Behavior

`session` scope records belong to the durable Rusty session id. They are visible
from any branch in that session unless the record carries a branch-specific
visibility marker in `metadata_json`.

`conversation_branch` scope records belong to a branch id and must also carry or
resolve to a session id. Branch records are retrieved by active branch first,
then ancestor branches, then session-scope records. Sibling branches are excluded
from automatic prompt context by default.

Cross-scope promotion is explicit governance:

- branch fact -> session fact requires `merge` or `supersede`;
- session summary -> branch summary is a new derived record with provenance;
- user choice on one branch does not become global until explicitly promoted.

## Branch-Aware Retrieval

The initial repository query should accept:

- `session_id`
- optional `active_branch_id`
- optional `include_ancestors`
- optional `include_siblings`, default false
- optional `shape_id`
- optional `prompt_context_only`
- `limit` and `offset`

Ordering:

1. active branch records, newest valid summaries first;
2. ancestor branch summaries/facts by branch ancestry distance, then recency;
3. session-scope summaries, choices, and facts by prompt priority and recency;
4. optional sibling/tool-only records only when explicitly requested.

The repository may start with deterministic SQL ordering and no semantic search.
Full-text or relevance scoring can be added later as an indexing capability.

## Prompt Policy Diagnostics

Prompt assembly should expose a diagnostic projection for `session_memory`:

- descriptor id and schema version;
- active session id and active branch id;
- selected record ids and shape ids;
- excluded counts by reason: `wrong_branch`, `sibling_branch`, `tool_only`,
  `archived`, `superseded`, `limit_exceeded`, `policy_disabled`;
- token/character estimate;
- whether the result was summary-context or tool-only.

This diagnostic belongs in Rusty Crew service/admin diagnostics and should not
depend on Den observation being healthy.

## Transcript Boundary

`session_memory` complements these existing surfaces:

- `message_slots`: raw durable chat message/variant records;
- `conversation_branches`: branch topology and branch heads;
- `event_history`: runtime fact log and replay source;
- Rusty View chat SSE/read APIs: browser-facing transcript/event projection.

It must not:

- duplicate raw message bodies as a general transcript store;
- delete or mutate transcript history during memory compaction;
- become the source of truth for branch heads or message-slot variants;
- rely on Den memory or Den documents for runtime session state.

Compaction writes summary records and may archive/supersede old
`session_memory` summaries. Transcript retention remains a separate runtime
policy.

## API And Repository Shape

Rust-owned repository methods should be explicit rather than raw generic SQL:

- `list_session_memory_descriptors` through the existing memory-space catalog;
- `add_session_memory_record`;
- `replace_session_memory_record`;
- `merge_session_memory_record`;
- `supersede_session_memory_record`;
- `archive_session_memory_record`;
- `query_session_memory_records`;
- `build_session_memory_prompt_context`.

The first implementation may use a single typed `session_memory_records` table
with JSON content validated by Rust shape descriptors. If a later backend needs
space-specific physical tables, the repository API should stay stable.

TypeScript may:

- submit typed proposals;
- call read/query/prompt-context APIs;
- render diagnostics;
- assemble brain prompts from Rust-returned prompt context.

TypeScript may not own migrations or issue raw SQL for this space.

## Test Strategy

Rust unit/integration coverage:

- descriptor declares all four shapes and allowed scopes;
- invalid scope/operation/shape proposal is rejected;
- session-scope records are isolated by session id;
- branch-scope records require a branch id that belongs to the session;
- expected revision rejects stale replace/archive/supersede;
- branch-aware retrieval returns active branch, ancestors, then session records;
- sibling branch records are excluded unless explicitly requested;
- archived/superseded records are excluded from prompt context but readable in
  diagnostics/history mode.

TypeScript/API smokes:

- memory-space catalog shows `session_memory` with all shapes;
- admin read/query routes return bounded `session_memory` records;
- prompt-policy diagnostics explain selected/excluded records;
- capture producer typed proposal for `session_memory` remains pending review
  until governance applies it;
- Rusty View/chat branch setup can create two branches and prove prompt context
  follows the active branch.

## Implementation Task Breakdown

1. Expand `session_memory` descriptor shapes and operation policy in
   `core-protocol`.
2. Add Rust persistence repository and migration for `session_memory_records`.
3. Add branch-aware query and prompt-context selection in Rust.
4. Expose native bridge and TS facade methods for session memory query and
   prompt diagnostics.
5. Extend memory-space admin/read tools to support `session_memory`.
6. Wire prompt assembly to include bounded `session_memory` summary context when
   enabled for the profile/session.
7. Add capture/governance apply path from typed proposals into
   `session_memory` records.
8. Add retention/compaction/archive policy hooks after real usage data exists.
