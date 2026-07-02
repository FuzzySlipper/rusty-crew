# ADR 0022: Crew-Owned Durable Service Storage

Status: Accepted for task 3907

Date: 2026-07-01

## Context

Rusty Crew's earlier architecture language drew a clean line: Rust owns
coordination state, and Den owns product data. That remains correct for Den
products such as Den projects, tasks, documents, comments, notifications, and
Den-owned memory. It is too narrow for Rusty Crew itself.

Rusty Crew is becoming a service runtime for agents and frontends. That service
has durable data of its own:

- sessions, events, body projections, queues, scheduler state, worker runs, and
  completion packets;
- active profile registry records, model-provider aliases, bindings, and
  provider wire state;
- transcripts, conversation branches, message variants, attachments, and
  data-bank scopes;
- dense memory, typed memory spaces, roleplay lore, memory proposals, and
  module-owned records;
- runtime search, tool telemetry, maintenance records, import/export batches,
  diagnostics, and logical storage metadata.

This data should not be pushed into Den as a storage alternative. Crew should
own Crew service data. The architectural problem is not ownership; it is letting
one `CoordinationStore` concept become an undifferentiated everything-store.

ADR 0017, ADR 0018, ADR 0019, and ADR 0020 already point in this direction:
module schema registry, typed memory spaces, DB-backed profile registry, and
SQLite/PostgreSQL readiness all assume a Rust-owned storage boundary. This ADR
names the boundary explicitly.

## Decision

Rusty Crew owns durable Crew service storage.

Den owns Den product/planning/observability data. Rusty Crew may store Den
references such as project ids, task ids, document refs, message refs, or source
metadata, but it must not mirror Den product rows as Crew-owned records.

Rusty Crew owns Crew runtime/service/product-like data needed by agents,
frontends, roleplay modules, and local service operation. This includes
coordination state and higher-level service data such as profiles, transcripts,
memory, lore, provider state, and module records.

The storage implementation must be partitioned by durable concern. The future
shape should make it visible when code is touching:

- coordination/lifecycle state;
- service configuration and profile registry;
- provider state;
- conversation and transcript content;
- memory, lore, and module-owned data;
- telemetry, search, diagnostics, and maintenance state.

Those partitions may share one database backend and one service process. They
should not share one vague ownership concept.

## Storage Classes

### Coordination And Lifecycle

Owned by Rust coordination:

- agents, instances, sessions, and session configs;
- event history and body projections;
- routed agent messages and queued messages;
- scheduler jobs and runs;
- worker/delegated runs, completion packets, fan-out groups;
- coordination counters and lifecycle diagnostics.

This class is closest to the older "coordination state" wording.

### Service Configuration And Profile Registry

Owned by Crew service storage:

- active profile registry records;
- lifecycle status, defaults, profile prompts, profile settings;
- model-provider aliases and selected model/provider config;
- MCP bindings, channel bindings, and derived runtime graph references;
- import/export metadata and file asset refs.

File profiles remain useful as import/export material and templates, not the
only active runtime truth.

### Provider State

Owned by Crew service storage:

- provider wire-state payloads;
- provider-state fingerprints and invalidation metadata;
- expiry and absence reasons;
- module-specific provider-state versions.

Provider state belongs with the brain/module contract, not with Den and not with
adapters.

### Conversation And Transcript Data

Owned by Crew service storage:

- message slots, message variants, selected variants;
- conversation branches, snapshots, branch heads, jump targets;
- attachments and data-bank scopes.

This data is service content for Rusty View, Rusty Roleplay, and future clients.
It is not Den message/thread storage.

### Memory, Lore, And Module Data

Owned by Crew service storage:

- dense profile memory;
- typed memory spaces and proposals;
- roleplay lore layers, entries, recall traces, provenance, capture/promotion
  records;
- module-owned tables registered through Rust-owned descriptors;
- simple KV where it is a Crew module capability.

Den memory remains Den memory. Bridging Den memory into Crew memory must be an
explicit import/proposal flow with provenance.

### Telemetry, Search, Diagnostics, And Maintenance

Owned by Crew service storage:

- runtime search indexes/read models;
- tool telemetry;
- storage diagnostics and query catalog projections;
- maintenance results and logical import/export batches.

These are service inspection and operation paths. They should stay typed and
curated, not arbitrary SQL.

## Backend Policy

SQLite remains a first-class backend for local, container, and small-agent
deployments. PostgreSQL is the scale/concurrency backend for larger deployments.
Both should be treated as first-class where implemented.

Feature code should prefer capability checks over backend-name checks. SQLite
and PostgreSQL do not need identical internal implementations; they need stable
typed repository contracts and conformance tests.

The current local service data is test data. Architecture remediation may reset
or recreate it. Future real deployments still need explicit logical
export/import and migration contracts. The clean-break policy for current test
data does not weaken future portability requirements.

## TypeScript Boundary

TypeScript may:

- call official bridge/service APIs;
- compose admin and Rusty View responses;
- run provider calls and model-callable tools;
- present query catalog results;
- orchestrate import/export workflows through typed APIs.

TypeScript may not:

- open the Crew database directly;
- issue raw SQL through the bridge;
- create, alter, or drop tables;
- own schema migration order;
- register unchecked physical table names at runtime;
- bypass repository invariants for revisions, retention, provenance, or queue
  expiry.

## Den Boundary

Den is:

- a planning/product-data source for Den concepts;
- an observability and notification target;
- an external service integration reached through adapters/tools.

Den is not:

- the storage fallback for Crew profiles, transcripts, memory, lore, or
  provider state;
- the coordination bus;
- required for internal Crew routing once Crew has accepted a runtime event.

Adapters may project Crew facts to Den for visibility and may inject Den updates
as external inputs. Projection failure is degraded observability, not a reason
to route Crew storage through Den.

## Consequences

`core-persistence` must be split into repository/store boundaries. Keeping SQL
inside Rust is good. Keeping every durable concept under one giant
`CoordinationStore` facade is not.

The implementation series should introduce explicit repository modules and, when
useful, store facades for coordination, service configuration, conversation
content, memory/lore/module data, and telemetry/search/diagnostics.

Bridge and service APIs should remain typed. Do not expose arbitrary SQL to
frontends or agents.

## Non-Goals

- Do not introduce Den as the storage home for Crew service data.
- Do not create a separate service for lore/memory just to avoid expanding Crew
  storage.
- Do not make PostgreSQL mandatory for local/small deployments.
- Do not preserve current scratch service data through hidden legacy fallback
  code.
- Do not remove the requirement for future logical export/import when real
  deployments need portability.

