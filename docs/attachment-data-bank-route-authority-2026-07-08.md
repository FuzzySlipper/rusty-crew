# Attachment And Data-Bank Route Authority

Status: classification for Den task `rusty-crew` #4684.

Date: 2026-07-08

Related:

- `docs/chat-projection-rust-authority-migration.md`
- `docs/chat-authority-boundary-classification-2026-07-06.md`
- `docs/postgres-attachments-data-bank-backend-slice.md`
- Den tasks #4921 and #4922

## Summary

Attachments and data-bank scopes already have Rust-owned repository surfaces for
SQLite and PostgreSQL. The route boundary should not move browser upload
parsing, HTTP envelopes, or SSE/socket mechanics into Rust. It should move
session-aware durable mutation semantics into Rust domain operations so
TypeScript remains a thin route and adapter layer.

The current repository operations are necessary but not sufficient as chat
domain operations: the raw `save_*` methods can upsert global ids, and the raw
`remove_*` methods operate by global id before TypeScript checks the returned
session. Those are acceptable low-level repository primitives, but not the
right authority surface for browser chat routes.

## Route Classification

| Route surface | Durable authority target | TypeScript responsibility | Notes |
| --- | --- | --- | --- |
| list attachments | Rust repository/read model | parse query params, map page envelope | `queryAttachments` is already Rust-backed. TS currently derives `total` as `offset + items.length`; keep this compatibility unless a later contract adds exact totals. |
| create/update/link attachment | Rust chat-domain operation | parse JSON/upload metadata, generate request-scoped fallback id, append browser chat events, map typed result to API envelope | Requires #4921. Rust should decide `created`/`updated`/`linked` and validate same-session id/link ownership transactionally. |
| remove attachment | Rust chat-domain operation | parse path, append browser chat event, map typed not-found/conflict errors | Requires #4921. The operation must include `session_id` in the mutation predicate so a wrong-session request cannot tombstone another session's attachment. |
| list data-bank scopes | Rust repository/read model | parse query params, map page envelope | `queryDataBankScopes` is already Rust-backed. Exact totals are not part of the current browser contract. |
| create/update data-bank scope | Rust chat-domain operation | parse JSON, generate request-scoped fallback id, append browser chat event, map typed result to API envelope | Requires #4922. Rust should decide `created` vs `updated` and reject cross-session `scope_id` collisions. |
| remove data-bank scope | Rust chat-domain operation | parse path, append browser chat event, map typed not-found/conflict errors | Requires #4922. The operation must include `session_id` in the mutation predicate. |

## Rust-Owned Semantics

Rust should own the following semantics because they mutate durable conversation
state or depend on durable ownership invariants:

- attachment and data-bank id ownership by session;
- create/update status selection based on transaction-local existing state;
- optional attachment link persistence in the same transaction as attachment
  persistence;
- attachment link target validation when the target id is durable chat state;
- status removal by `(id, session_id)`, not by global id plus a post-hoc TS
  session check;
- cross-backend behavior parity for SQLite and PostgreSQL.

The low-level repositories may continue to expose raw save/query/remove methods
for internal use and conformance tests. Browser-facing chat routes should move
to the session-aware domain operations from #4921 and #4922.

## TypeScript-Owned Glue

TypeScript should continue to own:

- HTTP path/query parsing and response envelopes;
- browser upload or storage-reference metadata parsing;
- request-scoped fallback id generation where the browser omits an id;
- CORS and SSE transport behavior;
- appending Rusty View chat events after a successful domain operation;
- bounded browser response compatibility for attachment metadata and extracted
  text.

This keeps the route layer close to the browser contract without letting it
become the authority for durable chat invariants.

## Current Gaps Split Out

#4921 covers attachment create/update/link/remove as session-aware Rust
chat-domain operations. It should remove the current capped TS pre-read used to
infer `created` vs `updated`, reject cross-session attachment id collisions, and
prevent wrong-session deletion from mutating another session.

#4922 covers data-bank scope create/update/remove as session-aware Rust
chat-domain operations. It should remove the current capped TS pre-read used to
infer `created` vs `updated`, reject cross-session scope id collisions, and
prevent wrong-session deletion from mutating another session.

Both follow-ups should preserve the existing browser API envelope shape while
moving the durable decisions behind native bridge operations.
