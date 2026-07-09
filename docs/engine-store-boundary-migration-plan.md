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

## Landed Slices

### Body queue store port

Task #5300's initial slice extracted the body follow-up queue behavior behind
`BodyQueueStore` in `crates/core/core-engine/src/body_queue.rs`.

The port is intentionally narrow:

- save a body follow-up message;
- expire pending follow-up messages at the current engine time;
- load follow-up messages by queue filter.

`CoreCoordinationStore` implements the port, while the body-queue unit tests use
a fake in-memory implementation. That proves the first engine behavior slice can
be tested without constructing a SQLite or Postgres coordination store.

This is a migrated slice for body follow-up queue behavior only. `CoreEngine`
still stores `CoreCoordinationStore` directly and still reaches the concrete
store for the other domains listed above.

### Bootstrap and session lifecycle store ports

Task #5310 extracted startup hydration, event replay/recording, and session
state/config persistence behind engine-owned ports in
`crates/core/core-engine/src/session_store.rs`.

The slice is split into two deliberately small traits:

- `EngineBootstrapStore` loads persisted sessions, loads persisted bus history,
  and records new bus events.
- `SessionLifecycleStore` saves session state and session state plus config.

`CoreCoordinationStore` implements both ports as thin delegates to
`core-persistence`. The fake-backed unit test proves bootstrap replay and
session/config persistence without constructing a concrete database.

Delegated-worker status and worker-pool persistence remain outside this slice;
only the delegated session state saves now route through the session lifecycle
port. The worker-specific store calls are left for the delegation/worker-pool
extraction.

## Remaining Extraction Tasks

Continue in domain-sized patches rather than one monolithic trait:

1. Extract scheduler store ports for job/run query, claim, completion,
   pause/resume, and stale-run expiration.
2. Extract delegation and worker-pool store ports for worker runs, fan-out
   groups, completions, pool members, and work-item lifecycle.
3. Extract provider-state store ports for wake lookup, persistence,
   invalidation, diagnostics, and cleanup.
4. Extract chat store ports for conversation branches, snapshots, slots,
   variants, attachments, data-bank scopes, read models, and event logs.
5. Extract roleplay lore store ports for lore layers, entries, recall,
   capture, promotion, provenance, and recall traces.
6. Extract memory store ports for profile/session memory, proposals,
   governance, activity digests, compaction artifacts, and prompt context.
7. Extract runtime admin store ports for profile/model admin, runtime counters,
   diagnostics, maintenance, service/module data, runtime search, and simple
   key/value state.

Each extraction should leave behind a fake-backed engine unit test for at least
one behavior in the extracted domain, not just a trait wrapper over the concrete
store.

## Config Ownership

`ClockConfig`, `EngineConfig`, and `EngineStorageConfig` are config-domain
types owned by `core-config`. `core-bridge-api` re-exports those selected
config types for the stable bridge-facing surface, while the bridge manifest
names `core_config::EngineConfig` as the `initialize_engine` input.

`core-protocol` should not regain runtime engine config policy types. It should
continue to own transport-free IDs, events, actions, and payload records. Engine
config validation and defaults, including the Postgres default schema, belong in
`core-config`; bridge and native layers may translate JS/wire inputs but should
not become the policy owner.

## Clock Policy

Engine behavior must use the engine clock (`CoreEngine::now`) when it affects
state transitions, timestamps, or generated IDs. Persistence internals may still
use wall-clock timestamps for storage-private uniqueness, temporary object names,
or diagnostics where the value is not part of engine behavior.

The 2026-07-07 cleanup moved queued follow-up message IDs and scheduler run IDs
off `SystemTime::now()` and onto the injected engine clock plus atomic sequence
suffixes.
