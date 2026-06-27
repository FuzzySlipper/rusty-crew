# PostgreSQL Conversation Transcript Proof Slice

Date: 2026-06-27

## Purpose

Task 3486 adds PostgreSQL proof coverage for conversation transcript storage.
This is a typed proof slice, not the full Rusty Crew PostgreSQL service
backend. It proves that branch and transcript operations can preserve SQLite
API behavior on PostgreSQL before the broader storage backend is selectable for
ordinary service boot.

## Covered Tables

The proof schema now owns:

- `message_slots`
- `messages`
- `message_blocks`
- `message_variants`
- `conversation_branches`
- `conversation_branch_state`
- `conversation_snapshots`

The implementation covers:

- message slot writes and bounded slot queries;
- message variant writes, durable message/block hydration, and variant queries;
- active variant selection with optimistic conflict reporting;
- conversation branch writes and stable branch listing;
- active branch state reads and selection;
- branch head updates;
- snapshot writes and queries;
- jump resolution for message, branch, snapshot, and cursor targets.

## Conflict Semantics

The PostgreSQL proof keeps the same typed conflict behavior as SQLite:

- active branch selection with a stale expectation returns
  `ActiveBranchConflict`;
- branch head updates with a stale expectation return `BranchHeadConflict`;
- active variant selection with a stale expectation returns
  `ActiveVariantConflict`.

For PostgreSQL, existing row updates use transaction-scoped row locks via
`FOR UPDATE`. Initial active-branch selection uses insert-once semantics so two
connections racing from an absent state cannot both silently succeed.

## Repository Group Status

The repository catalog group is `conversations_attachments`, but this task only
implements the conversation transcript/tree portion. PostgreSQL diagnostics
therefore report this group as partially implemented:

- conversation branches/messages/variants/snapshots/jumps are proofed;
- attachments and data-bank scopes remain unsupported.

Future attachment/data-bank tasks must not treat this slice as full completion
of the combined repository group.

## Verification

Local tests:

```bash
cargo test -p rusty-crew-core-persistence --features postgres-proof
cargo clippy -p rusty-crew-core-persistence --all-targets --features postgres-proof -- -D warnings
```

Live PostgreSQL proof:

```bash
source /home/system/database/rusty-crew-postgres.env
cargo test -p rusty-crew-core-persistence --features postgres-proof \
  postgres_conversation -- --ignored --nocapture
```

The live test creates a unique proof schema, runs shared SQLite/PostgreSQL
conversation conformance, runs cross-connection conflict checks, and drops the
schema afterward.
