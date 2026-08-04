# Context Accounting Migration Inventory

Status: Rust-owned accounting is authoritative for provider requests, wake
admission, compaction decisions, durable artifacts, and restart hydration.

This inventory records the remaining TypeScript estimator deliberately kept for
the Rusty View compatibility surface. It is not a second wake-policy
authority. The checked-in inventory is validated by
`npm run check:context-accounting-migration` and is part of `verify:ts` so a
call-site or boundary drift cannot quietly become a new policy path.

## Authority Boundary

The Rust contract lives in
`crates/brains/brain-runtime/src/context_accounting.rs`. The chat-completions
and Responses brains produce the native snapshot after provider operations.
Consumers that make admission, compaction, provider-state, or restart
decisions must use that snapshot and its Rust-owned policy inputs.

The TypeScript compatibility estimator is
`ts/packages/brain-island/src/context-estimate.ts`. Its estimator identity is
`fallback_chars_words_v1`, and its method is
`approximate_chars_div4_and_words_4over3_from_chat_events`. It may describe
approximate UI/debug values while a native snapshot is not available. It must
not reject a wake, trigger compaction, mutate provider state, or select a
strategy.

## Current Call-Site Inventory

| Path | Current use | Migration status |
| --- | --- | --- |
| `ts/packages/brain-island/src/context-estimate.ts` | Compatibility estimator implementation and exported package helper | Retain until all consumers have native snapshot coverage; never use as policy |
| `ts/packages/brain-island/src/service-rusty-view-chat-operations.ts` | Builds legacy route fields and diagnostic notes when no native snapshot is available | Compatibility-only; prefer native snapshot and remove fallback fields after downstream Rusty View migration |
| `ts/packages/brain-island/src/package-surface/profile-context.ts` | Re-exports estimator helpers for the current package surface | Remove with the compatibility route, not by adding new consumers |
| `ts/packages/brain-island/smokes/smoke-context-estimate.ts` | Deterministic contract coverage for the fallback estimator | Retain as compatibility coverage until removal |
| `ts/smokes/brain-island/smoke-rusty-view-chat-context.ts` | Redacted route compatibility fixture containing fallback output | Replace with native snapshot assertions before removing the legacy route shape |
| `fixtures/external-cassettes/rusty-view-chat-api/roleplay-turn-readback.redacted.json` | Historical redacted cassette | Keep as a historical fixture; it is not a runtime policy input |
| `crates/core/core-persistence/src/sqlite_integration_tests.rs` | Historical artifact metadata still names `fallback_chars_words_v1` to prove raw persistence does not rewrite old records | Keep only as historical fixture data; migrate the metadata in a later storage-contract task, and never use it for admission or compaction |
| `ts/packages/brain-island/src/tool-profile-prompt-authority.ts` | Classifies the estimator as `diagnostic_estimator` | Keep the classification until the compatibility surface is removed |

The old estimator is intentionally absent from `crates/brains/**`. There is no
TypeScript fallback for Rust wake admission or compaction policy.

## Removal Gates

The estimator and legacy route fields can be removed only after all of these
are true:

1. Rust snapshots are present for both chat-completions and Responses in every
   supported chat read path, including the pre-first-request unavailable case.
2. Rusty View reads the native snapshot fields and no longer requires the
   approximate compatibility fields.
3. Roleplay and external chat consumers have migrated their contracts and
   redacted cassettes.
4. Deterministic Offline and Postgres coverage, plus a live debug-service
   provider run, prove the native source/quality pair and restart hydration.
5. A repository search shows no runtime import or route serialization of
   `fallback_chars_words_v1`; historical documentation may retain the marker
   only when it explicitly labels the reference as historical.

Until those gates are met, this estimator is a bounded compatibility adapter,
not permission to add new TypeScript context policy.
