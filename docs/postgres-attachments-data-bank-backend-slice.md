# PostgreSQL Attachments And Data-Bank Backend Slice

Status: implemented as a PostgreSQL backend repository slice.

Task: Den `rusty-crew` #3487.

## What This Proves

The PostgreSQL backend store now covers the attachment/data-bank part of the
conversation repository group:

- `attachments`
- `attachment_links`
- `data_bank_scopes`

The backend store preserves the same typed API shape used by SQLite:

- save attachment plus optional initial link in one transaction;
- query attachments by session, message, block, scope, removed visibility, and
  expiry visibility;
- hydrate attachment links with message, block, scope, metadata, and created
  timestamps;
- remove attachments by status instead of deleting rows;
- save, query, and remove data-bank scopes by typed status.

The same conformance fixture runs against SQLite and PostgreSQL. That keeps the
API backend-neutral while PostgreSQL is still a backend rather than the
service default.

## Explicit Bounds

Attachment records still return the API's `extracted_text` field. Callers that
need admin summaries should continue using bounded admin/reporting DTOs rather
than exposing raw attachment payloads by default. The repository tests assert
the truncation flag and metadata round-trip, but this slice does not introduce
a new admin response surface.

Expiry behavior is query-visible through `AttachmentQuery`:

- default queries exclude expired rows when `now` is supplied;
- `include_expired` includes expired rows;
- `expired_only` returns only expired rows.

There is not yet a destructive expiry maintenance operation for attachments.
Removal remains an explicit status transition.

## Diagnostics

`storage_diagnostics()` now reports row counts for:

- `attachments`
- `attachment_links`
- `data_bank_scopes`

The `conversations_attachments` repository-group diagnostic now distinguishes
the conversation transcript backend surface from the attachment/data-bank
backend surface instead of claiming attachments are unsupported.

## Verification

Run the normal local backend suite:

```bash
cargo test -p rusty-crew-core-persistence --features postgres-backend
```

Run the live PostgreSQL attachment/data-bank backend conformance after sourcing the local dev
database env:

```bash
source /home/system/database/rusty-crew-postgres.env
cargo test -p rusty-crew-core-persistence --features postgres-backend \
  postgres_attachment_data_bank_backend_matches_sqlite_conformance_contract \
  -- --ignored
```
