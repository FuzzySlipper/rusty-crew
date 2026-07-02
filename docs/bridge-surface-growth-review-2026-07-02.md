# Bridge Surface Growth Review

Date: 2026-07-02
Task: rusty-crew #3921

## Summary

The bridge manifest is still directionally sound, but it now mixes four
different kinds of operations:

- coordination lifecycle operations that must remain explicit;
- admin/config operations that are good explicit frontend contracts;
- product-like storage CRUD that should be reviewed per storage module;
- diagnostic/runtime-local helpers that should stay named as diagnostics or
  move out of the stable manifest when codegen matures.

Do not collapse operations just because they are CRUD-shaped. Rusty Crew owns
its service storage, and typed explicit operations are often the safer API for
Rusty View and future roleplay clients. Collapse only when a family has schemas,
fixture drift checks, package consumers identified, and live client coverage.

## Keep Explicit

These operations encode Rust-owned coordination or lifecycle authority and
should remain explicit:

- Engine lifecycle: `initialize_engine`, `shutdown_engine`.
- Brain/wake path: `register_brain_implementation`,
  `replace_brain_implementation`,
  `unregister_brain_implementation_for_profile`, `wake_brain`,
  `submit_brain_event`, `submit_brain_actions`,
  `apply_brain_provider_state_output`.
- Adapter ingress and body queueing: `register_platform_adapter`,
  `inject_external_event`, `inject_den_data_update`,
  `enqueue_body_follow_up_message`.
- Durable session setup: `ensure_configured_session`, `list_sessions`.
- Delegation controls: `cancel_delegated_session`,
  `request_delegated_checkpoint`, `drain_delegated_sessions`,
  `cleanup_delegated_resources`, `delegated_session_status`.
- Runtime buffers and subscriptions: `subscribe_events`, `unsubscribe_events`,
  `get_buffer`, `release_buffer`.

These are the bridge surface where Rust's deterministic authority matters. A
generic CRUD envelope would hide lifecycle semantics and make failures harder
to classify.

## Keep Explicit For Now

These are admin/frontend contracts. Keep them explicit until a generated
schema layer covers their whole family and Rusty View has live checks:

- Runtime config planning: `validate_runtime_config_draft`,
  `plan_runtime_config`, `plan_create_profile`.
- Scheduler controls: `register_scheduled_wake_job`,
  `register_scheduled_host_job`, `list_scheduled_jobs`,
  `list_scheduled_runs`, `claim_scheduled_host_runs`,
  `request_scheduled_host_job_run`, `complete_scheduled_host_run`,
  `run_scheduler_tick`, `request_scheduled_job_run`, `pause_scheduled_job`,
  `resume_scheduled_job`.
- Profile/model admin: `list_profile_registry_records`,
  `get_profile_registry_record`. The implementation also exposes create/update
  profile registry and model-provider bridge methods that should be added to
  the manifest or moved behind a deliberate admin service boundary. They are
  now fixture/validation-backed in the TS native bridge, so they are a good
  next manifest alignment target.
- Context and memory artifacts: `save_session_activity_digest`,
  `list_session_activity_digests`, `save_context_compaction_artifact`,
  `list_context_compaction_artifacts`.

## Collapse Candidates

These families are product-like storage surfaces. They should not be collapsed
blindly, but they are the best candidates for a smaller module-shaped API once
their schemas and client usage are better understood:

- Attachments/data bank:
  `save_attachment`, `query_attachments`, `remove_attachment`,
  `save_data_bank_scope`, `query_data_bank_scopes`, `remove_data_bank_scope`.
  Candidate direction: one attachment repository API with typed commands and
  typed queries, not six unrelated manifest entries.
- Simple KV:
  `list_simple_kv`, `put_simple_kv`, `delete_simple_kv`. Candidate direction:
  keep only if it remains an operator/debug primitive; otherwise fold into the
  owning module store and avoid a generic escape hatch.
- Generic conversation variants/branches/snapshots:
  `save_message_slot`, `save_message_variant`, `query_message_slots`,
  `query_message_variants`, `select_active_message_variant`,
  `delete_message_variant`, `reorder_message_variants`,
  `save_conversation_branch`, `query_conversation_branches`,
  `get_conversation_branch_state`, `select_active_conversation_branch`,
  `update_conversation_branch_head`, `save_conversation_snapshot`,
  `query_conversation_snapshots`, `resolve_conversation_jump`.
  Candidate direction: keep conflict-sensitive selection/update operations
  explicit, but consider grouping pure save/query operations under a
  conversation repository facade after branch/variant live behavior is stable.

## Defer Collapse

Roleplay lore is intentionally domain-specific and should remain explicit for
now:

- Layer/chat controls: `create_lore_layer`, `get_lore_layer`,
  `list_lore_layers`, `update_lore_layer`, `archive_lore_layer`,
  `set_chat_layers`, `get_chat_layers`, `toggle_chat_layer`,
  `reorder_chat_layers`.
- Lore entries and recall: `add_lore_entry`, `replace_lore_entry`,
  `supersede_lore_entry`, `tombstone_lore_entry`, `query_lore_entries`,
  `lore_entry_provenance_events`, `add_entry_to_layer`,
  `remove_entry_from_layer`, `set_entry_constant`, `list_entries_by_layer`,
  `recall_lore`, `capture_lore_fact`, `promote_lore_entry`,
  `get_lore_layer_config`, `set_lore_layer_config`, `list_recall_traces`,
  `get_recall_trace`.

Reason: lore is not generic memory with a different label; it has roleplay
semantics, provenance, constants, promotion, traceability, and recall policy.
Collapse only after the roleplay frontend has live coverage and the storage
module boundary is settled.

## Move Or Rename Candidates

- `database_size` and `storage_schema` are diagnostics, not core coordination
  operations. Keep them callable, but classify them as admin diagnostics in
  docs/codegen and consider moving them out of the stable manifest once
  operator/admin API routing is clearer.
- `run_maintenance` is a service maintenance operation. Keep explicit, but
  document it as operator/admin, not as a normal agent-facing coordination verb.
- Manifest metadata no longer names a TS facade package after removing
  `ts/packages/core-bridge`; `ts/packages/native-bridge` is the active TS
  bridge facade/loader.

## Suggested Next Steps

1. Add manifest entries or a deliberate non-manifest classification for current
   profile/model provider methods that exist in `native-bridge` but are not in
   the manifest operation list.
2. Extend fixture/checker coverage to the conversation branch/variant family
   before considering any collapse there.
3. Extend fixture/checker coverage to attachment/data-bank records before
   grouping those APIs.
4. Add an operation classification field or sidecar report for:
   `coordination`, `admin_config`, `admin_diagnostic`, `module_storage`,
   `runtime_local`, and `deprecated_diagnostic`.
5. Do not add compatibility aliases when shrinking the surface. Use clean-break
   edits to callers and smokes.

## Current Decision

No operation removals in this review pass. The next safe work is classification
and schema coverage, not surface collapse.
