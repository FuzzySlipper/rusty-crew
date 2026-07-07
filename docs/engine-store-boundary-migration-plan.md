# Engine Store Boundary Migration Plan

Status: active migration plan

Rusty Crew's engine currently owns deterministic coordination, but
`CoreEngine` still holds `CoreCoordinationStore` directly. That was useful while
the storage backend work was moving quickly, but it now makes engine unit tests
pay for a concrete SQLite/Postgres store even when the behavior under test only
needs a narrow repository surface.

This document records the staged green path for moving engine authority behind
domain-sized store traits without hiding backend-specific storage
responsibilities.

## Current Engine Store Usage

As of 2026-07-07, `core-engine` reaches into these persistence surfaces:

- session lifecycle: `load_sessions`, `save_session`,
  `save_session_with_config`;
- event replay/recording: `load_event_history`, `save_event`;
- body follow-up queue: `save_queued_message`, `expire_queued_messages_at`,
  `load_queued_messages`;
- delegated completion and worker lifecycle:
  `delegated_completions_for_parent`, `fan_out_groups_for_parent`,
  `save_worker_run_requested`, `load_worker_run`,
  `load_worker_run_by_delegated_session`,
  `update_worker_run_status_by_delegated_session`,
  `create_worker_pool_work_item`, `complete_worker_pool_work_item`,
  `load_worker_pool_member`;
- scheduler: scheduled job/run query, claim, pause/resume, expiration, and
  completion calls;
- provider wire state: wake lookup, clear, diagnostics;
- profile/model admin: profile registry and model provider repositories through
  the `admin` facade;
- conversation/chat storage: conversation branches, snapshots, message slots,
  variants, attachments, and data bank scopes;
- roleplay lore: layers, entries, recall, capture, promotion, provenance, and
  recall traces;
- memory spaces: profile memory, session memory, proposals, governance
  decisions, activity digests, compaction artifacts, and memory prompt context;
- runtime service/module data: runtime counters, runtime search, diagnostics,
  maintenance, module data, service data, and simple key/value storage.

That is too wide for a single helpful fake. A monolithic trait would mostly
copy the concrete store API and make every test fake enormous.

## Boundary Shape

Prefer several domain-sized ports owned by `core-engine`, implemented by an
adapter over `CoreCoordinationStore` in `core-persistence` or a small
engine-local adapter module:

- `EngineBootstrapStore`: session replay and event replay/recording.
- `SessionLifecycleStore`: session config/state persistence and archive/reactive
  support.
- `BodyQueueStore`: queued follow-up messages and expiration.
- `SchedulerStore`: scheduled job/run lifecycle.
- `DelegationStore`: delegated runs, worker pools, fan-out, completions.
- `ProviderStateStore`: provider wire state hydration/diagnostics.
- `ChatStore`: conversation, message slot/variant, attachment, and data-bank
  operations.
- `RoleplayLoreStore`: roleplay lore layers, entries, recall, and provenance.
- `MemoryStore`: profile/session memory, proposals, governance, digests, and
  compaction artifacts.
- `RuntimeAdminStore`: admin/config, runtime counters, service/module data,
  diagnostics, and maintenance.

The first extraction should be `BodyQueueStore` or `SchedulerStore`; both have
small method sets and already have engine behavior tests that can move from
SQLite-backed fixtures to fake stores.

## Config Ownership

`ClockConfig`, `EngineConfig`, and `EngineStorageConfig` currently live in
`core-protocol` because they cross the bridge API. Moving them directly into
`core-config` would create an awkward dependency direction: `core-config`
already depends on `core-protocol` for protocol IDs and config payload types,
while `core-bridge-api` currently re-exports `core-protocol` as the stable
bridge-facing surface.

The staged path is:

1. Add config-domain equivalents in `core-config` once bridge codegen can export
   more than `core-protocol`.
2. Keep wire-compatible protocol aliases or conversion types during the same
   breaking-change window as the bridge manifest/codegen cleanup.
3. Move validation and defaults into `core-config`; leave transport-free IDs and
   event/action payloads in `core-protocol`.
4. Remove the protocol-owned engine config types after TypeScript and bridge
   callers consume the config crate surface.

Until then, treating engine config types as protocol-owned is accepted debt, not
an invitation to add more service config into `core-protocol`.

## Clock Policy

Engine behavior must use the engine clock (`CoreEngine::now`) when it affects
state transitions, timestamps, or generated IDs. Persistence internals may still
use wall-clock timestamps for storage-private uniqueness, temporary object names,
or diagnostics where the value is not part of engine behavior.

The 2026-07-07 cleanup moved queued follow-up message IDs and scheduler run IDs
off `SystemTime::now()` and onto the injected engine clock plus atomic sequence
suffixes.

