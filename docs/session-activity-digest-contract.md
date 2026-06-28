# Session Activity Digest Contract

Status: implementation contract for capture decision producer Phase 1

Related tasks:

- #3590 Implement Capture Decision Producer Digest And Proposal Pipeline
- #3592 Define Session Activity Digest Contract

## Purpose

Session activity digests are bounded per-wake summaries used by scheduled
background review to discover durable memory candidates after a wake finishes.
They are not raw transcripts. TypeScript builds them from the warm post-wake
event stream because service-host already has the wake source, profile/session
context, observed events, and completion summary in hand. Rust persists them and
owns query, retention, duplicate, and backend portability behavior.

## Contract Types

The shared TypeScript contract is `SessionActivityDigest` in
`ts/packages/contracts/src/index.ts`.

The Rust protocol contract is `SessionActivityDigest` in
`crates/core/core-protocol/src/memory_space.rs`.

Fields:

- `digest_id`: stable identifier for this digest. Phase 1 should derive this
  deterministically from profile id, session id, and wake id.
- `profile_id`: profile that produced the wake.
- `session_id`: runtime session that produced the wake.
- `wake_id`: wake identifier.
- `source`: wake source, such as channel, direct debug, scheduled wake, or API.
- `summary_text`: bounded human-readable digest for the capture prompt.
- `event_counts_json`: object keyed by event kind with numeric counts.
- `tool_calls_json`: bounded structured summaries of tool calls and outcomes.
- `signals_json`: bounded structured summaries of durable-memory signals, such
  as corrections, provider degraded status, repeated tool failure, or explicit
  completion evidence.
- `completion_summary`: optional terminal completion summary.
- `allowed_capture_spaces`: memory spaces the first capture pass may propose.
- `created_at`: timestamp when the digest was built.
- `retention_until`: optional expiry timestamp for pruning.
- `reviewed_at`: optional timestamp once a scheduled review has consumed it.

## Duplicate Policy

One digest should exist for a given profile/session/wake tuple. Duplicate saves
for the same tuple should be deterministic and idempotent: either replace the
same `digest_id` with equivalent content or return the existing record. The
first implementation should not create multiple records for replayed wake
events.

## Retention Policy

Digests are review input, not permanent memory. Phase 1 should keep retention
bounded by count and/or age. The default can be conservative, such as retaining
recent unreviewed digests plus a short reviewed history for diagnostics. Long
term facts belong in memory proposals and memory records, not in digest storage.

## Capture Target Policy

The first implementation validates `profile_dense` only. Capture producer output
should still use typed memory proposal fields so later phases can enable
`session_memory` and `roleplay_lore` through policy without rewriting the
producer.

Initial allowed target:

- `profile_dense`

Future gated targets:

- `session_memory`
- `roleplay_lore`

## Non-Goals

- Do not store full raw transcripts as the digest default.
- Do not auto-apply proposals in the digest phase.
- Do not make the capture producer a full brain wake with tools or provider
  wire state.
