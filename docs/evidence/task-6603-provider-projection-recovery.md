# Task 6603 provider projection admission and recovery

Date: 2026-08-08

Task 6603 closes the remaining admission and automatic-recovery gap on top of
the context accounting, persistence, manual compaction, API, fixture, and live
certification foundations delivered by tasks 6610-6617 and 6624.

## Exact request authority

Chat Completions and Responses now assemble the complete provider request
before dispatch and derive admission from that exact serialized projection.
The projection includes provider messages/items, system instructions, tools,
reasoning history, image request data, replay input, and previous-response
chain usage. A preflight `context_accounting_snapshot` is emitted before the
decision. Stale usage from an earlier provider request is not an admission
input.

Both brains have an exact incident regression whose assembled request measures
1,049,321 estimated input tokens against a 1,048,576-token context window. The
tests require a durable compaction continuation and zero provider-client
requests. The executable fixture matrix records this boundary and retains the
existing normal, reasoning, tool, image, replay, and previous-response-chain
projection coverage.

## Provider rejection recovery

A recognized provider context-limit response invokes the normal compaction
implementation against the rejected request projection. Successful recovery
writes the typed artifact with `provider_limit` trigger and
`provider_context_limit_recovery` reason, invalidates stateful provider-chain
state when necessary, and yields a continuation for the same session and
logical wake. It does not add a transcript message or clear raw history.

Provider-limit recovery is bounded to two successful compactions for one
logical turn. A further equivalent rejection preserves the last valid
projection and yields actionable attention. Chat Completions and Responses
tests cover successful resume, repeated rejection convergence, unsafe-boundary
attention, unchanged raw history, and artifact bounds.

## Persistence and compatibility

The continuation and artifact formats are unchanged and continue through the
existing SQLite/PostgreSQL compaction persistence and restart-hydration paths.
The compatibility estimator in
`service-rusty-view-chat-operations.ts` remains explicitly diagnostic-only;
`docs/context-accounting-migration-inventory.md` prohibits it from wake
admission, compaction, provider-state, or restart decisions.

Deterministic verification:

- Chat Completions: 91 passed, 1 ignored
- Responses: 58 passed, 1 ignored
- fixture catalog: 18 executable cases, 7 production probes
- fixture matrix Rust tests: 2 passed
- exact no-dispatch tests pass for both provider protocols
- bounded repeated-rejection tests pass for both provider protocols

Live debug-service certification reuses the task 6617 automatic/restart smoke
and the task 6624 manual-compaction public path so both operations are proved on
the same Rust-owned implementation rather than a synthetic test compactor.
