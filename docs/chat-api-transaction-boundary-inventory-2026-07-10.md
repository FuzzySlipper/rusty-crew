# Chat API Transaction Boundary Inventory

Status: implementation inventory for Den task `rusty-crew` #5371

Date: 2026-07-10

## Purpose

This inventory traces the current Rusty View chat routes through
`rusty-view-chat-api.ts` and `service-rusty-view-chat-operations.ts`. It records
the authority that remains after #4681-#4685, #4884/#4885, #4914-#4916, and
#4921/#4922 so later tasks do not recreate operations that already exist in
Rust.

The desired boundary is:

- Rust owns durable identity, ownership, paging truth, conflicts, mutation
  ordering, idempotency, and restart-safe read-model facts.
- TypeScript owns HTTP parsing/envelopes, brain/provider calls, browser-safe
  projection, ephemeral subscriber fan-out, and SSE socket mechanics.
- Appending a browser projection event after a completed Rust domain operation
  remains TypeScript glue under the current architecture. It is not evidence
  that the underlying domain mutation is still TS-owned.

## Classification

- **TS boundary:** intentionally remains TypeScript.
- **Rust complete:** one Rust operation owns the durable semantics; TS only
  validates, invokes, or projects.
- **Mutation residual:** a consistency-sensitive write sequence remains in TS
  and is work for #5375.
- **Read residual:** paging, ownership, merge, or reconstruction truth remains
  in TS and is work for #5379.

## Route Matrix

| Operation | Current call path | Classification | Follow-up |
| --- | --- | --- | --- |
| `GET /v1/chat/sessions` | `listSessions` then per-session chat-event scan, optional slot fallback, effective-default projection | **Read residual** | Rust must return one bounded session-summary page with exact counts/cursors. TS keeps query parsing and envelope. |
| `GET /v1/chat/sessions/{session_id}` | Rust event log, Rust durable read-model fallback, slot fallback, then pending body-state messages merged in TS | **Read residual** | Replace the fallback ladder with one Rust read-model/open operation. Pending messages must be represented explicitly, not silently converted into synthetic durable cursors. |
| `GET /v1/chat/sessions/{session_id}/events` | Rust event log, then Rust read model, then slot or pending-message synthetic fallback | **Read residual** | One Rust replay/read-model operation must own cursor, source, `has_more`, and latest-cursor facts. |
| `GET /v1/chat/sessions/{session_id}/stream` | Rust replay log plus Node response/subscriber wiring in `service-chat-stream-routes.ts` | **TS boundary** | Keep SSE framing, connection lifecycle, heartbeat, and subscriber fan-out in TS. Consume only Rust-provided replay/cursor facts. |
| `GET /v1/chat/sessions/{session_id}/context` | profile/provider/config reads plus token estimation and compaction diagnostics | **TS boundary** | Brain/provider diagnostics are boundary aggregation, not durable chat authority. Rust-owned compaction artifacts remain typed inputs. |
| `GET /v1/chat/sessions/{session_id}/tool-calls/{debug_detail_id}` | bounded process-local debug cache lookup | **TS boundary** | Debug cache is intentionally ephemeral and browser-redacted. |
| `GET /v1/chat/sessions/{session_id}/provider-requests/{debug_detail_id}` | bounded process-local provider-request debug cache lookup | **TS boundary** | Provider payload redaction and bounded debug retention remain at the provider boundary. |
| `POST /v1/chat/sessions/{session_id}/messages` | TS receipt map; ensure branch; raw slot save; raw variant save; append inbound event; branch-head update; wake; optional assistant slot persistence | **Mutation residual** | Add one Rust pre-wake chat-ingest transaction with durable idempotency. Wake/provider execution stays TS; completed assistant persistence may continue through the existing atomic slot operation. |
| `GET /v1/chat/sessions/{session_id}/slots` | `queryMessageSlots`, then TS guesses `total` and `nextOffset` | **Read residual** | Return a typed Rust page with exact total/next-offset. |
| `POST /v1/chat/sessions/{session_id}/slots` | ensure active/default branch; roleplay speaker snapshot; `createChatMessageSlot`; append projection event | **Mutation residual** | Fold default-branch ensure plus slot/primary-variant/head mutation into one Rust operation. Speaker snapshot remains an input assembled at the roleplay boundary. |
| `GET /v1/chat/sessions/{session_id}/slots/{slot_id}/variants` | capped 500-slot ownership pre-read; `queryMessageVariants`; guessed total | **Read residual** | Add a session-aware Rust variant page operation. Remove capped ownership scan. |
| `POST /v1/chat/sessions/{session_id}/slots/{slot_id}/variants` | capped slot ownership pre-read; speaker snapshot; `createChatMessageVariant`; append event | **Rust complete with redundant pre-read** | Rust already validates slot/session ownership and allocates ordinal. Remove the pre-read in #5383; do not add another mutation operation. |
| `DELETE /v1/chat/sessions/{session_id}/slots/{slot_id}/variants/{variant_id}` | `deleteChatMessageVariant`; append event | **Rust complete** | No new domain operation. |
| `POST /v1/chat/sessions/{session_id}/slots/{slot_id}/variants/reorder` | `reorderChatMessageVariants`; append event | **Rust complete** | No new domain operation. |
| `POST /v1/chat/sessions/{session_id}/slots/{slot_id}/active-variant` | capped ownership pre-read; `selectActiveChatMessageVariant`; append event | **Rust complete with redundant pre-read** | Rust owns ownership, CAS conflict, active selection, and branch-head effects. Remove the pre-read in #5383. |
| `GET /v1/chat/sessions/{session_id}/tree` | `queryConversationBranches` plus optional `queryConversationSnapshots`, then TS assembles page/active facts | **Read residual** | Add one Rust tree projection/page operation with exact totals and active state. |
| `GET /v1/chat/sessions/{session_id}/jump` | `resolveConversationJump` | **Rust complete** | TS parses target and projects the receipt. |
| `GET /v1/chat/sessions/{session_id}/search` | TS scans up to 500 slots, filters variants, computes snippets/highlights, sorts and pages | **Read residual** | Move current-session search to a Rust typed query over the full dataset. |
| `GET /v1/chat/search` | TS lists sessions, scans 500 slots per session, reconstructs cross-conversation search and paging | **Read residual** | Move cross-conversation search to the same Rust query family with exact totals. |
| `POST /v1/chat/sessions/{session_id}/branches` | `createChatConversationBranch`; append event | **Rust complete** | Rust owns branch identity/ownership/default state. |
| `POST /v1/chat/sessions/{session_id}/branches/active` | `selectActiveConversationBranch`; append event | **Rust complete** | Rust owns expected-active CAS and conflict metadata. |
| `POST /v1/chat/sessions/{session_id}/branches/{branch_id}/head` | `updateConversationBranchHead`; append event | **Rust complete** | Rust owns expected-head CAS and conflict metadata. |
| `POST /v1/chat/sessions/{session_id}/snapshots` | `createChatConversationSnapshot`; append event | **Rust complete** | Rust owns durable snapshot validation and persistence. |
| `GET /v1/chat/sessions/{session_id}/attachments` | `queryAttachments`; TS guesses total/next offset | **Read residual** | Reuse a typed exact-page result; do not recreate #4921 mutation work. |
| `POST /v1/chat/sessions/{session_id}/attachments` | `createChatAttachment`; append upload/update and optional link projection events | **Rust complete** | #4921 owns session-aware create/update/link transaction. |
| `DELETE /v1/chat/sessions/{session_id}/attachments/{attachment_id}` | `removeChatAttachment`; append event | **Rust complete** | #4921 owns session-aware removal. |
| `GET /v1/chat/sessions/{session_id}/data-bank/scopes` | `queryDataBankScopes`; TS guesses total/next offset | **Read residual** | Return a typed exact page; do not recreate #4922 mutation work. |
| `POST /v1/chat/sessions/{session_id}/data-bank/scopes` | `createChatDataBankScope`; append event | **Rust complete** | #4922 owns session-aware create/update. |
| `DELETE /v1/chat/sessions/{session_id}/data-bank/scopes/{scope_id}` | `removeChatDataBankScope`; append event | **Rust complete** | #4922 owns session-aware removal. |
| `GET /v1/chat/sessions/{session_id}/data-bank/scopes/{scope_id}/attachments` | `queryAttachments` scoped by session and scope; guessed total | **Read residual** | Use the same exact attachment page operation. |
| `GET /v1/chat/commands` | generated command registry readback | **TS boundary** | Code-as-config and browser projection stay TS and generated. |
| `GET /v1/chat/commands/{command_name}/autocomplete` | generated command metadata/static provider lookup | **TS boundary** | Command parsing/autocomplete stays TS. |
| `POST /v1/chat/sessions/{session_id}/commands` | parse command, invoke explicit command/control port, project output | **TS boundary** | Mutating effects must continue through Rust control plans; text parsing and response projection stay TS. |

## Mutation Work Authorized For #5375

Only two related mutation paths remain:

### 1. Pre-Wake Chat Ingest

Current TS sequence in `submitRustyViewChatMessage`:

1. consult a process-local receipt map;
2. ensure or select the default branch;
3. save a message slot;
4. save its primary variant;
5. append the inbound browser event;
6. update branch head;
7. invoke the wake;
8. optionally persist the completed assistant turn.

Steps 2-4 and 6 are one durable conversation transaction. Durable
idempotency must cover that transaction so a restart or duplicate request does
not create a second user message or wake. The Rust operation should accept the
fully assembled message/speaker metadata, create or select the default branch,
insert slot and primary variant, advance the branch head, and return a typed
receipt indicating `created` or `duplicate`.

The provider wake cannot be part of a database transaction. TS should invoke it
only for a newly created receipt. The existing `createChatMessageSlot`
transaction remains the correct operation for persisting a completed assistant
turn once the wake has produced text.

### 2. Explicit Slot Creation With Default Branch

`createRustyViewMessageSlot` currently ensures the branch in one operation and
creates the slot/variant/branch-head update in another. Extend the same Rust
transaction family so default-branch ensure and slot creation are atomic. Do
not create a second wrapper around `createChatMessageSlot`; evolve that
operation or share its repository transaction.

No new mutation operation is authorized for variant deletion/reorder/select,
branches, snapshots, attachments, or data-bank scopes. Those operations already
own their durable invariants in Rust.

## Read Work Authorized For #5379

The inventory confirms these cohesive Rust read/query families:

1. **Session summary/open/replay projection:** exact event/message counts,
   durable latest cursor, source selection, pending-message representation, and
   restart-safe `has_more` behavior.
2. **Session-aware exact pages:** slots, variants, attachments, and data-bank
   scopes return items plus total/next-offset from the queried dataset.
3. **Conversation tree projection:** branches, snapshots, active branch, exact
   totals, and deterministic ordering in one operation.
4. **Transcript search:** current-session and cross-conversation search over
   the full selected dataset with Rust-owned filtering, snippet/highlight
   offsets, ordering, totals, and paging.

These are read-model operations, not new persistence repositories. Existing
SQLite/PostgreSQL repository primitives remain the storage substrate.

## Backend Transaction Requirements

- SQLite and PostgreSQL must use one transaction for default-branch ensure,
  slot insert, primary variant insert, branch-head update, and idempotency
  receipt creation.
- A conflict or validation error must leave no slot, variant, head update, or
  receipt behind.
- Replaying the same idempotency key returns the original receipt and does not
  create another durable message.
- Concurrent calls with the same key produce one created result and one
  duplicate result.
- Concurrent calls with different keys preserve branch-head consistency under
  the operation's explicit CAS policy.
- Read operations must compute `total`, `has_more`, and cursors from the full
  query, not from the current bounded page.

## Explicit SSE-Only TypeScript Concerns

The following stay in TypeScript and must not be used to justify another Rust
transport layer:

- HTTP auth, CORS, query/header parsing, and browser envelopes;
- `text/event-stream` framing, heartbeat writes, disconnect cleanup, and
  backpressure handling;
- ephemeral in-process subscriber registration and fan-out of already committed
  Rust events;
- `Last-Event-ID` transport precedence over a query cursor;
- browser-safe unknown-event projection and debug-detail redaction;
- provider/tool delta streaming while a wake is in progress.

Rust remains authoritative for event allocation, durable replay ordering,
cursor validation, and retention boundaries consumed by that transport.

## Implementation Status

- #5375 evolved the existing chat-slot transaction to own default-branch
  selection, parent/head inheritance, slot and primary-variant writes,
  branch-head advancement, and a durable session-scoped idempotency receipt.
  TypeScript no longer keeps a process-local chat receipt map or orders those
  durable writes itself.
- #5379 added backend-neutral exact-page contracts for slots, variants,
  attachments, and data-bank scopes; one conversation-tree projection; SQL
  transcript search over the full selected dataset; and unified session
  summary/open/replay facts with explicit `event_log`, `message_slots`,
  `pending_messages`, or `empty` source selection. SQLite and PostgreSQL share
  conformance coverage, including restart readback.
- #5383 remains responsible for switching the HTTP route layer to these typed
  calls and deleting the superseded TypeScript fallback ladders, scans, and
  capped ownership pre-reads. SSE framing and live subscriber fan-out remain
  TypeScript transport concerns.
