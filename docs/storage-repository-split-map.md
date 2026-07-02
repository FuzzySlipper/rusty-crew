# Storage Repository Split Map

Status: implementation map for task #3922
Date: 2026-07-02

Rusty Crew owns its service data, but `core-persistence` should not present all
durable concerns as one undifferentiated `CoordinationStore`. This map turns
the existing `repositories.rs` catalog into an extraction plan for shrinking
`crates/core/core-persistence/src/lib.rs` and replacing
`postgres_proof.rs` with backend-parametric conformance suites.

Current shape:

- `core-persistence/src/lib.rs`: about 29k lines, owns most SQLite DDL,
  record types, store methods, row mappers, migrations, maintenance, and tests.
- `core-persistence/src/postgres_proof.rs`: about 18k lines, owns a parallel
  PostgreSQL proof store with duplicated repository behavior.
- `core-persistence/src/repositories.rs`: diagnostic catalog only.
- `core-persistence/src/module_schema.rs`: module schema descriptor logic.

## Target Module Layout

Create `crates/core/core-persistence/src/repos/` and move one repository group
at a time. Each module owns its records where they are not already protocol
types, SQLite DDL fragments, row mappers, backend-neutral repository methods,
and focused tests.

```text
src/
  lib.rs                       # store handles, backend open/config, facade exports
  repositories.rs              # runtime diagnostics catalog, kept small
  module_schema.rs             # descriptor validation; later imported by module_data
  repos/
    mod.rs
    storage_admin.rs
    sessions.rs
    events.rs
    queues.rs
    scheduler.rs
    workers.rs
    tool_telemetry.rs
    provider_state.rs
    runtime_search.rs
    conversation.rs
    attachments.rs
    profile_memory.rs
    session_memory.rs
    roleplay_lore.rs
    bindings.rs
    profile_registry.rs
    model_providers.rs
    module_data.rs
    import_export.rs
    runtime_counters.rs
  conformance/
    mod.rs
    harness.rs
    storage_admin.rs
    sessions.rs
    events.rs
    queues.rs
    scheduler.rs
    workers.rs
    provider_state.rs
    profile_registry.rs
    memory.rs
    conversation.rs
    runtime_search.rs
```

`postgres_proof.rs` should become a thin PostgreSQL harness plus any temporary
module adapters that have not yet moved. Its duplicated repository methods
should disappear as each conformance suite is adopted.

## Store Facades

Keep connection/backend ownership centralized, but expose explicit concern
facades so call sites show what kind of data they are touching.

```rust
CoreStore
  .coordination() -> CoordinationRepositorySet
  .service_data() -> ServiceDataRepositorySet
  .conversation() -> ConversationRepositorySet
  .memory() -> MemoryRepositorySet
  .module_data() -> ModuleDataRepositorySet
  .admin() -> StorageAdminRepositorySet
```

`CoordinationStore` may remain as a compatibility type name only during the
same refactor series, but new code should route through the concern facade.
Do not keep a permanent fallback facade that hides the split.

### Coordination Repository Set

Owns lifecycle and coordination truth:

- `sessions`
- `events`
- `queues`
- `scheduler`
- `workers`
- `provider_state` when it is required to continue a brain wake safely
- `runtime_counters` when counters are lifecycle summaries

### Service Data Repository Set

Owns service configuration and profile plumbing:

- `profile_registry`
- `model_providers`
- `bindings`
- service config records when those become database-backed

### Conversation Repository Set

Owns user/chat content:

- `conversation`
- `attachments`
- data-bank scopes
- context compaction artifacts that summarize conversation branches

### Memory Repository Set

Owns Crew-local memory and roleplay lore:

- `profile_memory`
- `session_memory`
- memory proposals/governance
- session activity digests
- `roleplay_lore`

### Module Data Repository Set

Owns module schema registry and module-owned logical stores:

- `module_data`
- `simple_kv`
- module schema install records
- module transfer hooks and logical export/import declarations

### Storage Admin Repository Set

Owns backend and operational readbacks:

- migrations/schema version
- diagnostics
- maintenance
- runtime search diagnostics if not owned by `runtime_search`
- import/export batch orchestration

## Table Ownership

### `repos/storage_admin.rs`

Tables:

- `schema_migrations`
- backend metadata tables such as PostgreSQL `rusty_crew_storage_metadata`

Owns:

- `open`/migration helpers after connection construction
- `count_rows`, `database_size`, `storage_diagnostics`, `storage_schema`
- `run_maintenance`
- migration helper primitives such as `add_missing_column`
- diagnostic table whitelist

Tests:

- fresh database applies all schema migrations
- version-one migration
- future schema version fails closed
- failed schema migration rolls back partial DDL
- diagnostic table names are whitelisted
- maintenance guardrails for retention, size, and hot indexes

Backend coverage:

- SQLite required immediately.
- PostgreSQL required immediately because this module decides whether Postgres
  can be selected and diagnosed.

### `repos/sessions.rs`

Tables:

- `sessions`
- `agents`
- `agent_instances`
- `session_identity`
- `session_configs`

Owns:

- session save/load/hydration
- durable agent identities
- agent instances
- session identity records
- immutable session config snapshots
- session/profile/agent index DDL

Tests:

- session persistence contract
- saving session projects durable identity records
- explicit identity records round trip source and Den references
- session config snapshot is immutable creation context

Backend coverage:

- SQLite and PostgreSQL conformance immediately. Session hydration is the first
  thing a service restart trusts.

### `repos/events.rs`

Tables:

- `event_history`
- `event_session_index`
- `event_agent_index`
- `event_instance_index`
- `event_correlation_index`
- `event_wake_index`

Owns:

- event persistence/filtering
- event index projection
- `should_persist_event`
- event row mappers

Tests:

- event ordering projection contract
- event log projection indexes support typed queries
- runtime state query APIs filter/page without raw SQL

Backend coverage:

- SQLite immediately.
- PostgreSQL immediately for event history and typed query parity.

### `repos/queues.rs`

Tables:

- `queued_messages`
- `agent_messages`

Owns:

- queued message save/load/expire
- internal agent message persistence
- TTL and terminal-state filtering
- queue row claim rules once implemented

Tests:

- queued message TTL no-resurrection contract
- queued message expiry is queryable without redelivery
- logical import dry run refuses queue resurrection risks

Backend coverage:

- SQLite immediately.
- PostgreSQL immediately with claim semantics documented. If the first
  PostgreSQL version does not support multi-writer claims, diagnostics must say
  so explicitly.

### `repos/scheduler.rs`

Tables:

- `scheduled_jobs`
- `scheduled_job_runs`

Owns:

- job upsert/load/query/pause/resume
- run claim/complete/query
- stale claim expiry

Tests:

- scheduler claim and expiry contract
- scheduled jobs claim runs and reconcile stale claims

Backend coverage:

- SQLite immediately.
- PostgreSQL immediately. This is one of the core reasons PostgreSQL exists for
  larger deployments, so claim behavior must be real rather than proof-only.

### `repos/workers.rs`

Tables:

- `worker_runs`
- `completion_packets`
- `worker_pool_members`
- `worker_pool_work_items`
- `worker_pool_leases`
- `worker_pool_events`

Owns:

- worker run lifecycle
- completion packet queries
- delegated completion aggregation
- fan-out group aggregation
- worker pool member/work/lease/claim/completion/expiry state

Tests:

- worker completion routing and fan-out accounting
- worker pool member registration, claim, and completion
- stale member cannot claim
- claim token fences terminal completion
- expired claims are terminal and not resurrected

Backend coverage:

- SQLite immediately.
- PostgreSQL conformance for worker runs and pool claims before worker pools are
  treated as production-ready on Postgres.

### `repos/tool_telemetry.rs`

Tables:

- `tool_call_history`

Owns:

- tool call record persistence
- tool call history readback
- runtime tool counters if they are stored as telemetry rather than lifecycle
  summaries

Tests:

- tool history load after event persistence
- telemetry loss is degraded observability, not coordination failure

Backend coverage:

- SQLite first.
- PostgreSQL can follow after coordination-critical groups.

### `repos/provider_state.rs`

Tables:

- `provider_wire_states`

Owns:

- provider wire state save/load/clear
- wake lookup and expiry
- strategy/fingerprint invalidation
- diagnostics

Tests:

- provider wire state expiry contract
- replaces current record and preserves payload version
- withholds expired and fingerprint-stale records
- clear and strategy change remove current state
- maintenance marks expired current records

Backend coverage:

- SQLite and PostgreSQL immediately. Direct Responses profiles depend on this
  state for correct restart and response replay behavior.

### `repos/runtime_search.rs`

Tables:

- `runtime_search_fts` on SQLite
- PostgreSQL `runtime_search_entries` and indexes

Owns:

- indexed runtime search write/query
- search filter validation
- backend search capability diagnostics

Tests:

- runtime search contract
- runtime search indexes messages and session configs
- no arbitrary SQL exposed outside persistence

Backend coverage:

- SQLite immediately.
- PostgreSQL conformance can start with exact/ILIKE semantics if full-text
  ranking is not ready, but diagnostics must report capability differences.

### `repos/conversation.rs`

Tables:

- `message_slots`
- `messages`
- `message_blocks`
- `message_variants`
- `conversation_branches`
- `conversation_branch_state`
- `conversation_snapshots`
- `context_compaction_artifacts`

Owns:

- durable message slots and variants
- active variant selection/conflicts
- branch state/head selection
- snapshots/jump targets
- context compaction artifacts

Tests:

- conversation branch/message contract
- message slots persist variants and active selection conflicts
- branch tree snapshots and jump targets persist
- context compaction artifacts preserve raw message history

Backend coverage:

- SQLite immediately.
- PostgreSQL conformance before Rusty View certification is considered stable
  on Postgres, because visible chat state depends on this group.

### `repos/attachments.rs`

Tables:

- `attachments`
- `attachment_links`
- `data_bank_scopes`

Owns:

- attachment save/query/remove
- attachment link validation
- data-bank scope save/query/remove
- expiry/status handling

Tests:

- attachments and data-bank scopes persist across reopen
- links preserve session/message/block/scope targets

Backend coverage:

- SQLite first.
- PostgreSQL after conversation conformance.

### `repos/profile_memory.rs`

Tables:

- `profile_memories`
- `memory_proposals`
- `memory_governance_decisions`

Owns:

- profile memory CRUD/revisions
- memory proposal lifecycle
- governance decision records
- memory operation policy/cap diagnostics for profile scopes

Tests:

- dense profile memory revision contract
- profile memory supports caps/revisions/profile isolation
- memory proposals persist governance state without direct mutation
- applied proposals create/update records

Backend coverage:

- SQLite immediately.
- PostgreSQL conformance before dense memory is enabled for multi-agent service
  usage on Postgres.

### `repos/session_memory.rs`

Tables:

- `session_memory_records`
- `session_activity_digests`
- `context_compaction_artifacts` if not fully owned by conversation

Owns:

- session/branch memory CRUD
- supersede/archive/replace semantics
- prompt-context assembly and diagnostics
- session activity digests

Tests:

- session memory round trip and isolation
- validates branch membership
- replace/supersede/archive enforce revisions
- compaction archives records without touching message history
- branch-aware memory ordering/exclusion
- prompt context reports policy status and exclusions

Backend coverage:

- SQLite immediately.
- PostgreSQL conformance before roleplay or long-lived chat sessions use this
  module on Postgres.

### `repos/roleplay_lore.rs`

Tables:

- `module_roleplay_lore_records`
- `module_roleplay_lore_provenance_events`
- `module_roleplay_lore_layers`
- `module_roleplay_lore_layer_entries`
- `module_roleplay_chat_layers`
- `module_roleplay_lore_recall_traces`
- `module_roleplay_lore_layer_config`
- `module_roleplay_lore_records_fts` and triggers on SQLite

Owns:

- lore records/revisions/tombstones
- provenance events
- layer creation/config/archive/linking
- chat layer ordering/toggles
- recall and recall traces
- FTS/search capability differences

Tests:

- roleplay lore layers/configs/entries/chat links round trip
- FTS triggers track record changes
- recall traces persist and are queryable

Backend coverage:

- SQLite immediately because first roleplay deployments use SQLite.
- PostgreSQL conformance before the local shared service switches roleplay lore
  workloads to Postgres.

### `repos/bindings.rs`

Tables:

- `channel_bindings`
- `mcp_bindings`

Owns:

- channel binding save/query
- MCP binding save/query
- binding diagnostics
- profile/session/agent scoping validation

Tests:

- external bindings are scoped per agent without secret material
- MCP/channel binding query filters

Backend coverage:

- SQLite and PostgreSQL immediately. Bindings are startup-critical service
  configuration.

### `repos/profile_registry.rs`

Tables:

- `profile_registry`

Owns:

- profile create/update/get/list
- lifecycle/revision semantics
- asset refs and registry metadata

Tests:

- profile registry supports lifecycle revisions and asset refs
- official create-profile path can hydrate without file-only plumbing

Backend coverage:

- SQLite and PostgreSQL immediately. Rusty View profile management depends on
  this group.

### `repos/model_providers.rs`

Tables:

- `model_providers`

Owns:

- provider upsert/get/list
- provider secret retrieval and redacted credential metadata
- optimistic revision checks
- protocol/provider-kind filtering

Tests:

- model provider secret envelope contract
- OpenAI OAuth/API-key envelope round trip
- revision mismatch behavior

Backend coverage:

- SQLite and PostgreSQL immediately. Provider selection is now part of normal
  profile creation and live testing.

### `repos/module_data.rs`

Tables:

- `module_schema_versions`
- generated module tables such as `module_simple_kv_entries`

Owns:

- installed module schema records
- module schema registry diagnostics that require runtime state
- generated logical store DDL
- simple KV repository
- module transfer hook metadata

Tests:

- module schema registry tracks fresh install and existing descriptor
- rejects upgrade without migration implementation
- rejects same-version fingerprint change
- rejects missing required capability
- simple KV repository round trips revisions and expiry
- storage schema diagnostics project installed module registry

Backend coverage:

- SQLite immediately for `simple_kv`.
- PostgreSQL conformance for `simple_kv` already exists in proof form and
  should become the first backend-parametric module-data suite.

### `repos/import_export.rs`

Tables:

- `runtime_import_batches`
- `legacy_id_mappings`

Owns:

- logical import/export bundles
- import dry-run validation
- legacy id mapping
- queue no-resurrection validation during import

Tests:

- legacy import metadata maps pi-crew and Hermes ids without runtime coupling
- logical storage import dry run validates capabilities/idempotency
- logical import refuses queue resurrection risks

Backend coverage:

- SQLite first.
- PostgreSQL after conformance harness exists; cross-backend movement should be
  logical, not raw file copying.

### `repos/runtime_counters.rs`

Tables:

- `runtime_counters`

Owns:

- counter increment/query/reset
- runtime summary
- counter scope serialization

Tests:

- runtime counters contract
- increment by scope without scanning history
- reset zeroes selected derived rows

Backend coverage:

- SQLite and PostgreSQL immediately. This is the best first extraction because
  it is small, already has proof-store methods, and has an existing shared
  conformance contract.

## Public Facade Plan

1. Introduce `CoreStore` or `RuntimeStore` as the explicit backend owner. It
   holds a backend enum/connection handle and creates concern facades by
   reference.
2. Keep `CoordinationStore` as the SQLite implementation detail during the
   first extraction, but do not add new broad methods to it.
3. Move public methods from `CoreCoordinationStore` into concern facades in the
   same order as repository extraction. The old enum can delegate during the
   series, but each moved method should have exactly one owning repository
   module.
4. Keep TypeScript/native bridge APIs typed. Do not expose arbitrary SQL,
   table names, or backend query syntax.
5. Keep `module_schema.rs` descriptor validation separate from runtime module
   install state. `repos/module_data.rs` owns the runtime tables and
   persistence methods.

## Extraction Order

1. `runtime_counters`
2. `module_data` simple KV and installed schema records
3. `storage_admin`
4. `bindings`, `profile_registry`, `model_providers`
5. `provider_state`
6. `sessions`
7. `events`
8. `queues`
9. `scheduler`
10. `workers`
11. `conversation`
12. `attachments`
13. `profile_memory`
14. `session_memory`
15. `roleplay_lore`
16. `runtime_search`
17. `import_export`
18. final removal of broad `CoordinationStore` delegating surface

Rationale:

- Start with small isolated groups that already have conformance coverage or
  low coordination risk.
- Move startup-critical service data before coordination-critical queue and
  scheduler code.
- Move queue/scheduler/worker code only after the facade shape is boring and
  tests can focus on no-resurrection/claim behavior.
- Move conversation/memory/lore after profile/provider/binding storage is
  already split, because those modules have larger product-facing APIs.

## Conformance Plan

Create backend-parametric tests with a shared harness:

```rust
trait RepositoryConformanceBackend {
    fn label(&self) -> &'static str;
    fn with_store<F>(&self, test_name: &str, test: F)
    where
        F: FnOnce(&CoreStore);
}
```

The actual harness can refine this shape, but each suite must run against:

- SQLite file store always.
- SQLite facade path if it remains distinct during the refactor.
- PostgreSQL when `postgres-proof`/env configuration is enabled.

First conformance suites to extract:

1. `runtime_counters`
2. `module_data::simple_kv`
3. `provider_state`
4. `profile_registry` and `model_providers`
5. `queues` no-resurrection
6. `scheduler` claim/expiry
7. `conversation` branch/message persistence

`postgres_proof.rs` should shrink each time a conformance suite lands. Do not
keep copy-pasted PostgreSQL behavior after a module owns its backend-neutral
contract.

## Existing Inline Tests To Move

Move tests with their modules. The large inline test module in `lib.rs` should
be drained in groups rather than split alphabetically.

Low-risk first moves:

- `runtime_counters_contract`
- `runtime_counters_increment_by_scope_without_scanning_history`
- `runtime_counter_reset_zeroes_selected_derived_rows`
- `simple_kv_repository_round_trips_revisions_and_expiry`
- `module_schema_registry_tracks_fresh_install_and_existing_descriptor`
- `storage_schema_diagnostics_project_installed_module_registry`

High-risk moves:

- `queued_message_ttl_no_resurrection_contract`
- `queued_message_expiry_is_queryable_without_redelivery`
- `scheduled_jobs_claim_runs_and_reconcile_stale_claims`
- worker pool claim/completion/expiry tests
- conversation branch/message tests
- session memory branch-aware prompt-context tests

## Backend Coverage Rules

- SQLite is first-class for every repository group.
- PostgreSQL is first-class for startup-critical, coordination-critical, and
  profile/provider/conversation groups before the local shared service relies on
  them.
- If PostgreSQL lacks an implementation for a module, the module diagnostics
  must report the gap rather than pretending parity.
- Backend differences such as FTS implementation, row claim behavior, advisory
  locks, and size estimates belong in explicit capability diagnostics.
- No Crew data should move into Den as a storage fallback.

## Governance Notes

After the first modules exist, update `governance/ownership.toml` so
`rusty-crew-core-persistence` may not depend on engine, bridge, brain, adapter,
or service-host crates. The persistence crate should depend on protocol/domain
types and backend libraries only.

Future `core-engine` extraction should wait until the storage facades are
stable enough that engine modules can depend on concern-specific stores instead
of the broad persistence surface.
