# PostgreSQL High-Volume Repository Coverage Plan

Status: planning output for task 3474

Date: 2026-06-26

## Purpose

This document plans the PostgreSQL repository coverage needed before Rusty Crew
can treat PostgreSQL as production-ready for many-agent or roleplay-heavy
deployments.

Task 3419 proved a narrow runtime-counter slice. That proof does not cover the
high-volume stores that will dominate roleplay and long-lived agent use:

- transcript and conversation tree records;
- dense profile memory, typed memory spaces, and roleplay lore;
- runtime search;
- attachments and data-bank scopes;
- provider wire state.

These repositories should remain unsupported for PostgreSQL service boot until
their API-level contracts and conformance tests are in place.

## Contract Principles

The service API is the portability contract. Neither TypeScript nor frontend
clients should learn SQLite FTS5, PostgreSQL `tsquery`, PostgreSQL `jsonb`
operators, physical table names, or raw SQL fragments.

Stable API contracts should use:

- typed filters;
- explicit cursor/page fields;
- stable sort modes;
- structured result shapes;
- backend capability diagnostics;
- degraded search indicators when ranking/tokenization differs;
- repository conformance tests that assert behavior, not implementation syntax.

Backend-specific behavior may exist behind Rust repositories, but it must not
leak through the bridge, service routes, command registry, or query catalog.

## Runtime Search

Runtime search indexes Rust-owned coordination history and read models. It is
not Den product-data search and should not index every high-volume stream by
default.

### Stable API Contract

Runtime search requests should remain typed:

- `query`: free text string;
- `row_type`: bounded enum;
- `session_id`;
- `agent_id`;
- `instance_id`;
- `task_id`;
- `event_kind`;
- `recorded_after`;
- `recorded_before`;
- `limit`;
- `cursor` or `offset` depending on the final paging contract.

Runtime search results should expose:

- `row_type`;
- `row_key`;
- `session_id`;
- `agent_id`;
- `instance_id`;
- `task_id`;
- `event_kind`;
- `recorded_at`;
- `title`;
- bounded snippet/body preview;
- optional backend-neutral score bucket or rank number;
- `search_backend` diagnostics such as `sqlite_fts5`, `postgres_tsvector`, or
  `degraded_linear_scan` only in diagnostics metadata.

Callers may sort by `relevance` or `recorded_at`, but they must not depend on
identical rank scores across SQLite and PostgreSQL. Cross-backend tests should
assert inclusion, filtering, pagination, and stable ordering for equal
timestamps/keys, not exact FTS rank equivalence.

### PostgreSQL Work

PostgreSQL runtime search should use a dedicated repository/read-model
implementation, likely with `tsvector` columns or search tables maintained in
the same transaction as source record writes.

Do not port search by exposing PostgreSQL syntax through the query catalog.

## Conversation Trees And Transcripts

Conversation state includes branches, active branch state, message slots,
variants, snapshots, and transcript navigation. Roleplay and chat UI workloads
will make this one of the largest stores.

### Stable API Contract

The contract should cover:

- branch list by session and parent branch;
- active branch selection with expected-current conflict detection;
- branch head update with expected-current conflict detection;
- message slot and active variant selection with expected-current conflict
  detection;
- branch timeline paging by session/branch/cursor;
- jump targets by message, branch, snapshot, or cursor;
- snapshot list by session/branch/message;
- bounded transcript window reads for UI and brain reconstruction.

Ordering must be stable through typed fields:

- branch order: `created_at ASC, branch_id ASC`;
- message timeline order: logical sequence/cursor, then message id;
- variant order: slot id plus variant id or explicit ordinal;
- snapshot order: `created_at DESC, snapshot_id DESC` unless the API says
  otherwise.

PostgreSQL must preserve optimistic conflict semantics. Concurrent branch-head,
active-branch, and active-variant updates should either apply once or return a
typed conflict, never silently clobber.

## Attachments And Data-Bank Scopes

Attachments are metadata records with external storage refs, extracted text,
links to transcript/message scopes, and expiry/removal state. Data-bank scopes
group attachment-like resources for user/UI workflows.

### Stable API Contract

Attachment/data-bank APIs should stay typed:

- query by `session_id`, `message_id`, `block_id`, `scope_id`, status, and
  expiry window;
- include/exclude removed records explicitly;
- never return unbounded extracted text by default;
- return byte size, MIME type, storage refs, thumbnail refs, truncation flags,
  and bounded metadata;
- support expiry/removal maintenance through typed retention policy.

PostgreSQL coverage must preserve link consistency transactionally: saving an
attachment with an initial link must not leave only one side of the record.

## Typed Memory, Dense Memory, And Roleplay Lore

Dense profile memory already exists. Future session memory and roleplay lore
should use typed memory-space/module contracts rather than an untyped generic
blob table or a TS-owned lore schema.

### Stable API Contract

Memory-space APIs should expose:

- descriptor/catalog projection;
- space-specific query filters;
- record scopes and visibility;
- revision tokens for replace/remove/supersede;
- provenance/evidence refs;
- governance state for proposals;
- retrieval policy and prompt-policy metadata;
- backend-neutral search/filter fields.

`profile_dense` should remain the compatibility first space. `roleplay_lore`
should be a typed module/memory space with world/entity/canon/visibility
filters and provenance lookup. Roleplay lore search may use backend full-text
support when available, but callers should request `query`, `world_id`,
`entity_id`, `canon_status`, `visibility`, and `limit`, not backend syntax.

PostgreSQL memory/lore implementations should use JSONB where useful, but the
Rust API remains typed records and validated payloads. SQLite remains
first-class by mapping the same records to JSON text and explicit indexes.

## Provider Wire State

Provider wire state is opaque brain-module state used across wakes. It may hold
Responses replay metadata, provider ids, cache handles, or response-chain
continuity. Rust owns namespace, persistence, expiry, invalidation, and wake
selection; brain modules own payload semantics.

### Stable API Contract

The existing provider wire-state contract must remain stable:

- key: `(session_id, module_id, strategy_id)`;
- profile fingerprint;
- provider fingerprint;
- payload version;
- opaque JSON payload and encoding;
- `expires_at`;
- `last_wake_id`;
- invalidation timestamp and reason.

Wake lookup must never return an expired or fingerprint-mismatched current
record. Instead it returns no record and an explicit absence reason.

PostgreSQL provider-state coverage must include expiry and fingerprint
invalidation tests because this state directly affects model continuity and
token/cost behavior for modular brains.

## Required Follow-Up Tasks

The follow-up implementation tasks should stay independent enough to land in
separate slices:

1. Runtime search PostgreSQL contract/conformance and backend implementation.
2. Provider wire-state PostgreSQL repository and invalidation tests.
3. Conversation transcript/tree PostgreSQL repository and conflict tests.
4. Attachments and data-bank PostgreSQL repository and retention tests.
5. Dense profile memory / typed memory-space PostgreSQL repository coverage.
6. Roleplay lore module/memory-space PostgreSQL proof.
7. High-volume repository diagnostics and production-readiness gates.

## Production Readiness Gate

PostgreSQL remains non-production for Rusty Crew until every high-volume
repository group has one of:

- PostgreSQL implementation with shared conformance tests;
- explicit unsupported diagnostics that prevent service workloads from relying
  on that group;
- documented degraded behavior accepted for a specific deployment mode.

The global service must not silently fall back to SQLite or report PostgreSQL
production readiness while these repositories are missing.
