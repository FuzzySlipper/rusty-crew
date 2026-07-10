# Roleplay Authority And Storage Inventory

Task: #5372  
Date: 2026-07-09  
Status: implementation inventory for #5376, #5380, and #5384

## Boundary

Rust already owns the deterministic roleplay record planners, session lifecycle
plan, chat-layer binding plan, assistant-alternative plan, lore control
normalization, scene-state merge, narrator FSM, typed lore repositories, and
typed conversation tree stores. This series must consume those surfaces rather
than recreate them.

TypeScript intentionally keeps HTTP parsing and envelopes, provider text
generation, prompt and imported asset decoding, external tool invocation, SSE
projection, and browser-facing response shaping.

## Route Ownership Matrix

| Route family | Current durable writes | Current authority | Required change |
| --- | --- | --- | --- |
| profile characters | Rust validates/normalizes, TS writes `character:*` JSON through `simple_kv` | split incorrectly | typed character repository and CRUD operations |
| profile player personas | Rust validates/normalizes, TS writes `player_persona:*` JSON through `simple_kv` | split incorrectly | typed persona repository and CRUD operations |
| roleplay sessions | Rust plans lifecycle and metadata, Rust session store plus TS `simple_kv` metadata | split incorrectly | typed session metadata repository and cohesive lifecycle apply operations |
| session chat layers | typed lore tables plus TS session metadata write | two-store invariant | one atomic roleplay session/layer operation |
| ST packet import | character/persona/session/import `simple_kv`, typed lore, session, branch, slot, and variant stores | multi-domain TS orchestration | typed import receipt plus transactional import application boundaries |
| narrator config | profile JSON plus config reload | TS adapter/config | remain TS; Rust narrator FSM owns runtime phase decisions |
| prompt stack and speaker identity | typed Rust projection from fetched records | Rust semantics, TS fetch/adapter | remain split at the current boundary |
| lore layer/entry CRUD | typed Rust lore repositories | Rust | no storage migration |
| lore search/detail | typed Rust query plus TS scope fan-out and browser projection | Rust query truth, TS projection | retain unless a measured query operation removes N+1 reads |
| lore promotion/capture | typed Rust operations | Rust | retain; TS only resolves HTTP scope and formats results |
| alternatives | typed conversation records with Rust alternative planner | mostly Rust | close only selection/branch-head atomicity gap |
| session fork | Rust lifecycle planner followed by many session/branch/slot/variant writes | split incorrectly | cohesive transactional fork operation |
| scene state | Rust merge/validation and typed store | Rust | no migration |

## Generic Storage Inventory

`service-roleplay-routes.ts` is the only production roleplay owner using the
generic JSON helpers `putRoleplayJson`, `listRoleplayJson`, and
`getRoleplayJson`. The active key spaces are:

| Scope | Keys | Replacement |
| --- | --- | --- |
| `roleplay_profile:<profile_id>` | `character:<character_id>` | `module_roleplay_characters` |
| `roleplay_profile:<profile_id>` | `player_persona:<persona_id>` | `module_roleplay_player_personas` |
| `roleplay_session:<session_id>` | `metadata` | `module_roleplay_sessions` |
| `roleplay_import:<profile_id>` | `st-packet:<import_id>` and child character/persona provenance keys | `module_roleplay_imports` plus typed source refs |

There is no compatibility read or dual-write requirement. Once typed routes
land, delete the generic helpers and all production roleplay `simple_kv`
scopes. Scratch service data may be discarded.

## Typed Repository Schema Plan

### Characters

Primary key `character_id`; indexed by `(profile_id, status, updated_at)` and
`(profile_id, name)`. Store every `RoleplayCharacter` field as typed columns
where scalar and JSON only for ordered lists and explicitly open metadata.
Include `revision`, `created_at`, and `updated_at` for guarded writes.

### Player Personas

Primary key `persona_id`; indexed by `(profile_id, status, updated_at)` and
`(profile_id, display_name)`. Preserve avatar refs, description, notes,
revision, and timestamps as typed fields.

### Session Metadata

Primary key and foreign ownership key `session_id`; index `profile_id`,
`archived`, and `updated_at`. Store display name, selected character/persona,
ordered active layer ids, revision, and timestamps. Character/persona and layer
references are validated by the roleplay domain operation before commit.

### Import Receipts

Primary key `import_id`; index `(profile_id, imported_at)`. Store source kind,
source/provenance JSON, created character/persona/layer/session ids, typed
counts, status, failure reason, and timestamps. A completed receipt is written
in the same transaction as its selected typed roleplay mutations; partial
imports must roll back instead of producing a completed receipt.

SQLite and PostgreSQL use the same logical records and conflict semantics.
Tables belong to the roleplay module in storage ownership metadata and profile
purge removes character, persona, session metadata, and import rows. Session
purge removes only that session's metadata and import references that are
explicitly session-owned.

## Proven Atomicity Gaps

1. **Create session:** `createSession`, session metadata JSON write, then chat
   layer write can leave a runtime session without roleplay metadata/layers.
2. **Update metadata/chat layers:** chat layers are written before metadata;
   the second failure leaves two projections disagreeing.
3. **Archive/restore:** runtime lifecycle and metadata writes are separate and
   can disagree after either ordering fails.
4. **Fork:** target session, metadata, layers, branch, every slot/variant,
   branch head, and active branch are separate writes. Any mid-loop failure
   leaves a visible partial fork.
5. **Generated alternative:** variant save, active-variant selection, and
   branch-head update are separate. Selection can succeed while branch head
   remains stale.
6. **Manual alternative selection:** active variant and branch head are two
   writes with the same partial-failure risk.
7. **Chat-layer route:** typed chat layers and session metadata are separate
   writes.
8. **ST import:** character, persona, lore, session, transcript tree, and final
   receipt can partially succeed. The receipt currently cannot prove atomic
   completion.

Character/persona single-record CRUD becomes atomic simply by moving planning
and persistence into one Rust operation. Lore entry create/replace/promote,
layer membership updates, scene-state writes, and landed conversation-tree
domain mutations are already cohesive and must not be duplicated.

## Implementation Order

1. Add typed repositories and direct CRUD/query operations for characters,
   personas, session metadata, and import receipts (#5376).
2. Replace every generic roleplay read/write and delete the helpers.
3. Add transactional session lifecycle, chat-layer synchronization, and fork
   application operations where the repositories share a backend (#5380).
4. Add one generated/manual alternative selection operation that commits slot
   selection and branch-head movement together; retain provider generation in
   TS (#5384).
5. Treat cross-domain ST import as an explicit transaction command. If a
   backend transaction cannot span a required repository, fail before writes;
   do not compensate through best-effort cleanup.

## Explicit TypeScript Remainder

- route matching, query/body decoding, status codes, CORS, and envelopes;
- Rust DTO camel/snake conversion at the generated bridge edge;
- SillyTavern packet decoding and source-asset parsing;
- narrator and alternative provider invocation and prompt construction;
- prompt stack source gathering and browser-safe result projection;
- SSE/activity projection and external adapter calls;
- pagination presentation derived from typed Rust query receipts.

Line count alone is not a migration trigger. TS route code may remain large
when it is visibly adapter or projection code and does not own durable truth or
deterministic cross-record decisions.

