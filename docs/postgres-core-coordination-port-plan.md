# PostgreSQL Core Coordination Port Plan

Task: `rusty-crew#3505`

## Summary

The full-service backend selector can now fail closed before SQLite opens when
PostgreSQL is selected for normal service boot. The next blocker is the Rust
core coordination store boundary: `CoreEngine` still owns a concrete
`CoordinationStore`, and `CoordinationStore` is still a SQLite implementation
with a large public method surface.

The existing PostgreSQL proof store covers useful module and admin slices, but
it does not yet cover the correctness-sensitive repositories needed for normal
agent runtime:

- session hydration and identity/config snapshots
- event log hydration
- queued messages, expiry, and terminal-message safety
- scheduled jobs and run claims
- worker/delegated run lifecycle
- tool telemetry
- storage diagnostics row counts for the core groups

This means task `3505` should be implemented as a small series rather than a
single broad edit.

## Current Engine Store Call Surface

The engine currently calls these persistence methods directly on
`CoordinationStore`:

```text
add_profile_memory
build_session_memory_prompt_context
claim_scheduled_run
clear_provider_wire_state
complete_scheduled_run
count_rows
database_size
delegated_completions_for_parent
expire_queued_messages_at
expire_stale_scheduled_runs
fan_out_groups_for_parent
get_profile_memory
get_profile_registry_record
list_memory_proposals
list_profile_memory
list_profile_registry_records
list_provider_wire_state_diagnostics
list_simple_kv
load_provider_wire_state_for_wake
load_queued_messages
load_scheduled_job
load_sessions
load_tool_call_history
load_worker_run
pause_scheduled_job
query_attachments
query_conversation_branches
query_conversation_snapshots
query_data_bank_scopes
query_message_slots
query_message_variants
query_runtime_counters
query_scheduled_jobs
query_scheduled_runs
query_session_memory_records
remove_attachment
remove_data_bank_scope
remove_profile_memory
replace_profile_memory
reset_runtime_counters
resolve_conversation_jump
run_maintenance
runtime_summary
save_attachment
save_conversation_branch
save_conversation_snapshot
save_data_bank_scope
save_event
save_message_slot
save_message_variant
save_queued_message
save_session
save_session_with_config
save_worker_run_requested
search_runtime
select_active_conversation_branch
select_active_message_variant
storage_diagnostics
storage_schema
update_conversation_branch_head
update_worker_run_status_by_delegated_session
upsert_scheduled_job
```

That surface is too large to duplicate blindly. The port should first carve a
backend-neutral repository trait/facade for the engine-facing subset, then move
SQLite behind that facade before adding PostgreSQL.

## Recommended Implementation Sequence

1. Define the engine-facing storage trait and wrap SQLite behind it.
   Keep the public semantics unchanged and add a conformance harness that can
   run the same repository tests against any backend.

2. Add PostgreSQL schema and repositories for sessions, durable identities,
   immutable session configs, and event log hydration.
   These are required before the engine can restart from Postgres.

3. Add PostgreSQL queue/message repositories with explicit TTL and terminal
   state tests.
   Startup/import paths must discard expired messages and must not resurrect
   terminal messages.

4. Add scheduler and worker/delegation repositories.
   Scheduled run claims need PostgreSQL row-level claim semantics, not
   SQLite-style single-writer assumptions.

5. Add telemetry, runtime counters, maintenance, and diagnostics row counts.
   These should expose backend-neutral admin diagnostics so the operator can see
   which backend is active and which groups are healthy.

6. Wire `CoreEngine` initialization to accept the selected backend store through
   the selector.
   Full-service PostgreSQL readiness should become true only after all
   correctness-sensitive groups used by normal service boot are implemented and
   passing conformance tests.

## Safety Notes

- SQLite remains first-class and should use the same trait/conformance suite.
- PostgreSQL must fail closed for any missing repository group.
- Queue correctness is a hard gate: expired or terminal records must not be
  reintroduced during startup, import, hydration, or retry.
- The cutover plan intentionally starts with a fresh PostgreSQL database; this
  avoids raw SQLite-to-Postgres migration coupling while both backends are made
  first-class.

