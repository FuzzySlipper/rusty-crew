# PostgreSQL Attachments And Data-Bank Proof Slice

Status: implemented as a PostgreSQL proof repository slice.

Task: Den `rusty-crew` #3487.

## What This Proves

The PostgreSQL proof store now covers the attachment/data-bank part of the
conversation repository group:

- `attachments`
- `attachment_links`
- `data_bank_scopes`

The proof store preserves the same typed API shape used by SQLite:

- save attachment plus optional initial link in one transaction;
- query attachments by session, message, block, scope, removed visibility, and
  expiry visibility;
- hydrate attachment links with message, block, scope, metadata, and created
  timestamps;
- remove attachments by status instead of deleting rows;
- save, query, and remove data-bank scopes by typed status.

The same conformance fixture runs against SQLite and PostgreSQL. That keeps the
API backend-neutral while PostgreSQL is still a proof backend rather than the
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
the conversation transcript proof surface from the attachment/data-bank proof
surface instead of claiming attachments are unsupported.

## Verification

Run the normal local proof suite:

```bash
cargo test -p rusty-crew-core-persistence --features postgres-proof
```

Run the live PostgreSQL attachment/data-bank proof after sourcing the local dev
database env:

```bash
source /home/system/database/rusty-crew-postgres.env
cargo test -p rusty-crew-core-persistence --features postgres-proof \
  postgres_attachment_data_bank_proof_matches_sqlite_conformance_contract \
  -- --ignored
```
