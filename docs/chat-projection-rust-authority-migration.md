# Chat Projection Rust Authority Migration

Status: planning note for task 4583

Date: 2026-07-07

Related docs:

- `chat-authority-boundary-classification-2026-07-06`
- `rusty-view-chat-api-contract`
- `postgres-conversation-transcript-backend-slice`
- `postgres-attachments-data-bank-backend-slice`

## Purpose

Rusty Crew already owns the durable conversation repository surfaces for
message slots, variants, branches, snapshots, attachments, and data-bank scopes.
The remaining drift is that TypeScript still owns too much of the browser-facing
chat read model and too many multi-step route orchestration decisions.

The migration goal is not to move HTTP parsing or SSE transport code into Rust.
The goal is to make TypeScript route handlers validation/delegation shells while
Rust owns stable chat projection, cursor semantics, and consistency-sensitive
transcript domain operations.

## Current State

Rust-owned today:

- `core-persistence` and `core-engine` expose message slot, message variant,
  conversation branch, conversation snapshot, attachment, and data-bank
  operations.
- PostgreSQL and SQLite conformance now cover transcript, branch, attachment,
  and data-bank repository behavior.
- Wake dispatch and session lifecycle are Rust-owned outside the chat routes.

TypeScript still owns important authority:

- `ts/packages/brain-island/src/chat-event-store.ts` persists chat stream events
  to JSONL and owns replay ordering.
- `ts/packages/brain-island/src/service-app.ts` owns chat event projection from
  core/brain events, terminal fallback events, cursor merge logic, stream replay
  snapshots, and subscriber fan-out.
- `ts/packages/brain-island/src/rusty-view-chat-api.ts` owns the public chat
  event union, session summary shaping, open-session fallback projection from
  message slots, event page fallback projection, and many mutation request
  shapes.
- Route handlers still call multiple service/bridge helpers for branch, variant,
  attachment, and data-bank operations and then shape event envelopes in TS.

## Target Ownership

Rust owns:

- durable chat event append/replay cursor semantics;
- chat read-model projection from durable transcript records into stable
  browser-facing event records;
- session summary counts/cursors derived from the read model;
- consistency-sensitive domain operations that update more than one transcript
  concept, such as default branch creation plus branch-head update, variant
  selection plus terminal event projection, and branch/snapshot jump resolution;
- attachment and data-bank domain validation when the operation mutates durable
  conversation state.

TypeScript owns:

- HTTP request parsing and browser envelope compatibility;
- CORS and SSE transport wiring;
- command text parsing/autocomplete;
- adapter/brain/provider execution boundaries;
- calling Rust ports and mapping typed Rust errors into existing admin/chat
  envelopes.

## Migration Slices

### 1. Define A Rust Chat Read-Model Port

Add a Rust-owned projection/read-model operation for Rusty View chat pages. The
first slice should not try to rewrite all SSE internals. It should define the
stable output shape and move at least durable slot-to-event projection and
cursor/page semantics behind a Rust/native bridge operation.

Acceptance:

- TypeScript asks one port for open-session/event-page read models instead of
  reconstructing message slot fallback events itself.
- Cursor parsing, page bounds, `has_more`, and latest cursor behavior are tested
  in Rust.
- Existing Rusty View chat read API smokes keep passing.

### 2. Move Chat Event Append/Replay Storage Out Of JSONL TS Authority

Replace or wrap `ChatEventStore` with a Rust-owned durable event log/replay
port. TS can keep subscriber fan-out, but event id allocation and replay order
should not depend on an in-process map plus JSONL merge.

Acceptance:

- event sequence allocation is Rust-owned and restart-safe;
- replay after `Last-Event-ID` and explicit `cursor` uses the same Rust cursor
  parser as read-page routes;
- duplicate/dropped stream updates are covered by a smoke that exercises
  restart or store reload behavior.

### 3. Move Multi-Step Transcript Mutations Behind Rust Domain Operations

Identify route paths where TS currently orders more than one transcript
operation. Move those to Rust domain operations so consistency rules live beside
the repository transactions.

Candidate operations:

- create default branch and set branch head;
- create message slot plus primary variant plus optional branch head update;
- create alternate variant and optionally select it;
- select/delete/reorder variants with stable active/primary invariants;
- create branch/snapshot/jump flows used by Rusty View and roleplay.

Acceptance:

- at least one multi-step mutation becomes one Rust/native operation with typed
  conflict output;
- TS route code delegates to that operation and maps the result envelope;
- branch/variant conflict smokes still pass.

### 4. Classify Attachment And Data-Bank Route Authority

Attachments and data-bank scopes already have Rust repos. Decide which route
semantics should be Rust domain operations and which should remain TS HTTP glue.
The likely target is Rust-owned durable validation plus TS-owned upload/browser
envelope handling.

Acceptance:

- route-level attachment/data-bank semantics are documented as Rust domain
  operation or TS glue;
- any multi-record mutation moves to Rust or gets an implementation task;
- bounded browser response rules remain documented and smoke-tested.

### 5. Generate Or Ratchet Chat API Contracts

The browser chat API currently has OpenAPI plus TS contract types. As read-model
ports stabilize, add generation or drift checks so Rust, OpenAPI, and TS do not
hand-copy event kinds/envelopes indefinitely.

Acceptance:

- event kind unions and public envelope shapes are generated or checked against
  a single durable contract source;
- unknown/future events remain browser-safe;
- Rusty View chat contract and stream smokes cover the final surface.

## Implementation Notes

Do not preserve old fallback paths as legacy compatibility. This service still
has disposable test data, and the green path should be explicit: Rust-owned
read model and mutation authority, TS-owned route/transport glue.

Do not move SSE socket mechanics into Rust unless a later task proves it is
needed. The immediate risk is not Node writing bytes to a response; it is TS
deciding what the durable chat stream means.
