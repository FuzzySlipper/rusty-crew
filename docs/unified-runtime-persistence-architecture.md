# Unified Runtime Persistence Architecture

Status: Accepted design note for task 2866

Date: 2026-06-20

## Purpose

Rusty Crew's persistence layer is the durable substrate for one service runtime
that may eventually host prime agents, delegated sessions, worker-pool sessions,
pi-crew-style agents, and Hermes-style agents. The near-term implementation
should stay modest, but it must not make SQLite tables, worker-pool concepts, or
legacy runtime assumptions the shape of the whole system.

This note defines the ownership boundary and vocabulary later persistence tasks
should cite.

## Ownership Boundary

Rusty Crew owns runtime coordination state:

- engine startup metadata;
- agents, runtime instances, and sessions;
- session lifecycle, delegation lineage, wake state, and completion packets;
- selected `ToolProfile`s and resource limits;
- routed runtime messages;
- brain event telemetry such as tool-call start/finish;
- append-only coordination events and derived projections;
- queue/retention state needed to avoid resurrecting stale work;
- counters and service diagnostics derived from runtime facts.

Den owns product data:

- projects, tasks, documents, comments, and user notifications;
- product-level message threads;
- Den-specific observability surfaces.

Rust persistence may store Den references such as `project_id`, `task_id`,
notification ids, or source metadata, but it must not mirror Den product rows as
runtime-owned records. Den projection failures must not block internal
coordination once Rust has accepted the runtime event.

## Runtime Entity Vocabulary

### Agent

An agent is a durable logical identity: for example a prime coder, a reviewer,
an imported Hermes profile, or a future pi-crew agent. It is not the same thing
as a live process or one conversation/session.

Future records should be able to store:

- `agent_id`;
- display label;
- profile id or profile source reference;
- source system metadata;
- status and archive timestamps;
- optional default project/task references.

### Runtime Instance

A runtime instance is one active host/binding for an agent identity. It may be a
long-lived prime agent, a service-managed worker, a delegated child, or a future
external process.

Instances should eventually hold:

- `instance_id`;
- `agent_id`;
- host or adapter binding metadata;
- profile snapshot/reference;
- status;
- created, last-seen, degraded, and archived timestamps;
- optional source-system import metadata.

The instance concept prevents dozens of agents from overloading `session_id` as
the only durable identity handle.

### Session

A session is a bounded conversation/work lifecycle owned by Rust coordination.
It can be `full`, `delegated`, or `worker`. A session references an agent and, in
future schema, should also reference a runtime instance when one is known.

Sessions own:

- `session_id`;
- `agent_id`;
- optional `instance_id`;
- `profile_id`;
- session kind and status;
- delegation lineage;
- immutable creation config;
- mutable lifecycle projection;
- selected `ToolProfile`;
- resource limits.

Large prompt/body buffers should not be stored directly in ordinary session
rows. If durable prompt or role assembly snapshots are needed, store compact
references or separate bounded records.

### Delegation

Delegation is session lineage, not a separate runtime authority. Rust owns the
creation action, delegated session id, parent/child relationship, lifecycle
events, fan-out group, completion packet, and parent-consumption policy.

TypeScript tools may request delegation through `BrainAction::RequestDelegation`;
they must not spawn or persist delegated sessions directly.

### Profile And Tool Profile

Profiles describe brain behavior. `ToolProfile` is the selected tool descriptor
contract attached to a session and used by the TS brain island to filter actual
tool implementations.

Rust stores the selected `ToolProfile` for audit and wake context. The
TypeScript registry remains the source of model-callable tool implementation
metadata.

### Adapter

Adapters connect external systems such as Den, MCP, channels, Telegram, or a
future admin service. Adapter state belongs at the platform boundary unless it
is required for Rust coordination. Runtime persistence can store adapter ids and
external correlation references.

### Source System

Source-system metadata is the place for future migration context. Examples:
`rusty_crew`, `pi_crew`, `hermes`, `manual_import`, `test_fixture`.

Source metadata should be additive and optional. Do not bake pi-crew worker-pool
or Hermes per-profile SQLite assumptions into core runtime tables.

## Facts Versus Projections

Append-only facts are the durable source of what happened. Projections are
indexed views used for query speed, diagnostics, and hydration.

Append-only facts:

- `CoreEvent`s in sequence order;
- routed agent messages;
- brain wake requests and accepted action batches;
- tool-call start/finish/error events;
- delegation lifecycle events;
- completion packets;
- queue enqueue/expire/discard facts when queue persistence lands.

Mutable projections:

- current session state;
- current agent/instance status;
- worker/delegated run status;
- message/search indexes;
- tool-call history table;
- counters and health summaries;
- queue pending/expired views.

Projection rows can be rebuilt from facts when feasible. If a projection cannot
be rebuilt exactly, document the non-replayable input and keep the projection's
update path behind a persistence API.

## Event Log Contract

The event log should remain the durable ordering backbone. Later event-log work
should index or project by:

- sequence;
- timestamp;
- event kind;
- session id;
- agent id;
- instance id;
- wake id;
- correlation id;
- source system;
- project/task reference when present.

High-volume text deltas may need retention, compaction, or snapshotting. That
policy should be explicit rather than hidden inside a query helper.

## SQLite Isolation And Portability

SQLite is the current implementation detail, not the architecture. It remains a
good default while the runtime is local and the persistence module stays
isolated.

Rules:

- `rusqlite` imports stay inside `crates/core/core-persistence`.
- Coordination/business logic should call repository-style methods, not SQL.
- Dynamic SQL helpers must whitelist table/index names.
- SQLite-specific features such as FTS5, WAL, pragmas, and `rowid` behavior
  need named helper boundaries and PostgreSQL mapping notes.
- Schema migration metadata must be explicit enough for support/debug output.
- No adapter or TS package should inspect SQLite files directly.

If the runtime later needs PostgreSQL, the migration should replace
`core-persistence` internals and query implementations, not the engine/body/tool
contracts.

## Queue And Message Retention

Queue persistence must be designed to avoid message resurrection. A queued
message is not simply "undelivered text"; it needs:

- enqueue timestamp;
- expiry timestamp;
- body-selected TTL/cap policy;
- delivery attempts;
- terminal state such as delivered, expired, discarded, or cancelled;
- query path for expired messages that does not redeliver them.

Expired messages should be inspectable by a future operator/agent pull tool, but
hydration must not requeue them as fresh work.

## Future Migration Metadata

Migration metadata should be carried in small provenance records or columns:

- source system;
- legacy id;
- imported-at timestamp;
- import batch id;
- source profile/session path;
- compatibility notes;
- whether the imported record remains externally owned.

Pi-crew worker-pool history and Hermes per-profile SQLite history should map
into provenance and import tables later. They should not dictate near-term
runtime table names or session semantics.

## Near-Term Implementation Sequence

Later tasks should build in this order:

1. Harden persistence APIs and database portability boundaries.
2. Add versioned migrations and schema compatibility checks.
3. Model durable agent, instance, and session identity.
4. Persist complete session configuration and constraints.
5. Define append-only runtime event/query projection contracts.
6. Add searchable message/session history through an abstracted FTS interface.
7. Add counters and lightweight state summaries.
8. Define multi-agent hydration/restart semantics.
9. Add safe queue retention and expired-message query state.
10. Expose bounded runtime query APIs for service/admin diagnostics.
11. Add scale, retention, and maintenance guardrails.
12. Define migration metadata for future pi-crew/Hermes imports.
13. Prove the model with a multi-agent restart and search scenario.

## Non-Goals For The Current Slice

- Migrating pi-crew or Hermes data now.
- Replacing SQLite now.
- Introducing a worker-pool-first persistence model.
- Mirroring Den task/project/document rows.
- Storing large prompt/body buffers in hot session rows without a separate
  retention strategy.
