import {
  manifestOperationNames,
  type ManifestOperationName,
} from "@rusty-crew/contracts";

import * as bridgeSchemas from "./bridge-validation-schemas.js";

export interface RustBridgeValidationFixtureSummary {
  operation_count: number;
  fixtures: Array<{
    name: string;
    operation: string;
  }>;
}

interface OperationExemptionGroup {
  group: string;
  reason: string;
  operations: readonly ManifestOperationName[];
}

const EXPECTED_MANIFEST_OPERATION_COUNT = 144;
const EXPECTED_TYPEBOX_SCHEMA_EXPORT_COUNT = 27;
const EXPECTED_RUST_FIXTURE_FAMILY_COUNT = 9;
const EXPECTED_MANIFEST_OPERATION_COVERAGE_COUNT = 24;
const EXPECTED_EXEMPT_OPERATION_COUNT = 120;

const RUNTIME_VALIDATED_MANIFEST_OPERATIONS = [
  "create_profile_registry_record",
  "get_model_provider",
  "get_profile_registry_record",
  "list_model_providers",
  "list_profile_registry_records",
  "list_sessions",
  "model_provider_refresh_impact",
  "plan_model_provider_refresh",
  "provider_state_diagnostics",
  "purge_profile",
  "drain_pi_agent_brain_stream",
  "run_openai_responses_brain",
  "start_pi_agent_brain",
  "submit_brain_actions",
  "submit_brain_event",
  "update_profile_registry_record",
  "upsert_model_provider",
  "validate_tool_metadata_policy",
  "validate_local_tool_profile_policy",
  "wake_brain",
] as const satisfies readonly ManifestOperationName[];

const RUST_FIXTURE_BACKED_OPERATIONS = [
  "project_body_state",
  "list_sessions",
  "run_openai_responses_brain",
  "list_profile_registry_records",
  "list_model_providers",
  "model_provider_refresh_impact",
  "list_memory_space_descriptors",
  "list_memory_proposals",
  "record_memory_governance_decision",
] as const satisfies readonly ManifestOperationName[];

const RUST_FIXTURE_FAMILY_NAMES = [
  "body_state_v1",
  "list_sessions_v1",
  "brain_wake_stream_result_v1",
  "profile_registry_record_v1",
  "model_provider_record_v1",
  "model_provider_refresh_impact_v1",
  "memory_space_descriptor_v1",
  "memory_proposal_record_v1",
  "memory_governance_decision_record_v1",
] as const;

const BRIDGE_OPERATION_EXEMPTION_GROUPS = [
  {
    group: "engine_lifecycle",
    reason:
      "Core engine and brain-registration calls currently use narrow command/receipt shapes; add fixture coverage when their payloads grow beyond startup plumbing.",
    operations: [
      "initialize_engine",
      "shutdown_engine",
      "register_brain_implementation",
      "replace_brain_implementation",
      "unregister_brain_implementation_for_profile",
      "apply_brain_provider_state_output",
    ],
  },
  {
    group: "responses_buffered_and_oauth",
    reason:
      "Buffered Responses stream operations are covered by Rust brain tests and fingerprinted result fixtures only for the aggregate run result; add per-operation fixtures before changing stream item envelopes.",
    operations: [
      "start_openai_responses_brain",
      "drain_openai_responses_brain_stream",
      "submit_openai_responses_tool_output",
      "cancel_openai_responses_brain",
      "exchange_openai_oauth_code",
    ],
  },
  {
    group: "pi_agent_buffered",
    reason:
      "Rust pi-agent start/drain are TypeBox validated; submit/cancel are narrow buffered-run receipts covered by Rust bridge tests until per-operation fixtures are added.",
    operations: ["submit_pi_agent_tool_output", "cancel_pi_agent_brain"],
  },
  {
    group: "config_and_adapter_ingress",
    reason:
      "Runtime config, create-profile planning, adapter registration, and external ingress are validated by focused config/adapter smokes; bridge fixtures should be added before new UI-facing wire fields land.",
    operations: [
      "register_platform_adapter",
      "validate_runtime_config_draft",
      "plan_runtime_config",
      "plan_create_profile",
      "plan_profile_registry_mutation",
      "inject_external_event",
      "inject_den_data_update",
      "enqueue_body_follow_up_message",
    ],
  },
  {
    group: "sessions_scheduler_delegation",
    reason:
      "Session lifecycle, scheduler, and delegation records are active Rust coordination surfaces; fixture coverage is intentionally deferred to a dedicated family expansion.",
    operations: [
      "archive_session",
      "ensure_configured_session",
      "register_scheduled_wake_job",
      "register_scheduled_host_job",
      "list_scheduled_jobs",
      "list_scheduled_runs",
      "claim_scheduled_host_runs",
      "request_scheduled_host_job_run",
      "complete_scheduled_host_run",
      "run_scheduler_tick",
      "request_scheduled_job_run",
      "pause_scheduled_job",
      "resume_scheduled_job",
      "cancel_delegated_session",
      "request_delegated_checkpoint",
      "drain_delegated_sessions",
      "cleanup_delegated_resources",
      "delegated_session_status",
    ],
  },
  {
    group: "conversation_tree",
    reason:
      "Conversation tree/message variant operations are high-value UI records but not yet fixture-backed; keep exact exemptions until their family receives TypeBox fixtures.",
    operations: [
      "save_message_slot",
      "save_message_variant",
      "query_message_slots",
      "query_message_variants",
      "select_active_message_variant",
      "delete_message_variant",
      "reorder_message_variants",
      "save_conversation_branch",
      "query_conversation_branches",
      "get_conversation_branch_state",
      "select_active_conversation_branch",
      "update_conversation_branch_head",
      "save_conversation_snapshot",
      "query_conversation_snapshots",
      "resolve_conversation_jump",
    ],
  },
  {
    group: "attachments_data_bank",
    reason:
      "Attachment and data-bank records are covered by repository/API smokes today; add bridge fixtures before changing browser-facing shapes.",
    operations: [
      "save_attachment",
      "query_attachments",
      "remove_attachment",
      "save_data_bank_scope",
      "query_data_bank_scopes",
      "remove_data_bank_scope",
    ],
  },
  {
    group: "storage_diagnostics",
    reason:
      "Storage/admin diagnostics are backend-neutral operational readbacks; fixture coverage is deferred until the diagnostics family stabilizes.",
    operations: [
      "database_size",
      "storage_schema",
      "storage_diagnostics",
      "run_maintenance",
    ],
  },
  {
    group: "model_secrets",
    reason:
      "Secret readback is intentionally narrow and redacted by callers; do not add fixture payloads that could normalize secret exposure.",
    operations: ["get_model_provider_secret"],
  },
  {
    group: "roleplay_narrator",
    reason:
      "Roleplay/narrator bridge operations are being decomposed separately; exact exemptions keep future shape changes visible until Rust-owned roleplay fixtures land.",
    operations: [
      "plan_roleplay_assistant_alternative",
      "build_roleplay_prompt_context",
      "roleplay_speaker_identity",
      "write_roleplay_character",
      "merge_roleplay_character",
      "write_roleplay_player_persona",
      "merge_roleplay_player_persona",
      "patch_roleplay_session_metadata",
      "normalize_roleplay_narrator_config",
      "roleplay_narrator_mandatory_explore_requests",
      "roleplay_narrator_auto_capture_request",
      "start_roleplay_narrator_turn",
      "next_roleplay_narrator_phase",
      "roleplay_narrator_review_requests_revision",
    ],
  },
  {
    group: "roleplay_lore",
    reason:
      "Lore records and recall traces are active roleplay storage surfaces; fixture coverage should be added as part of the roleplay Rust-boundary migration.",
    operations: [
      "create_lore_layer",
      "get_lore_layer",
      "list_lore_layers",
      "update_lore_layer",
      "archive_lore_layer",
      "set_chat_layers",
      "get_chat_layers",
      "toggle_chat_layer",
      "reorder_chat_layers",
      "add_lore_entry",
      "replace_lore_entry",
      "supersede_lore_entry",
      "tombstone_lore_entry",
      "query_lore_entries",
      "get_lore_entry",
      "lore_entry_provenance_events",
      "add_entry_to_layer",
      "remove_entry_from_layer",
      "set_entry_constant",
      "list_entries_by_layer",
      "recall_lore",
      "capture_lore_fact",
      "promote_lore_entry",
      "get_lore_layer_config",
      "set_lore_layer_config",
      "list_recall_traces",
      "get_recall_trace",
    ],
  },
  {
    group: "local_stores_memory_compaction",
    reason:
      "Simple KV, session memory, digests, and compaction artifacts are covered by storage/API tests today; bridge fixtures are pending a storage-family expansion.",
    operations: [
      "list_simple_kv",
      "put_simple_kv",
      "delete_simple_kv",
      "query_session_memory_records",
      "build_session_memory_prompt_context",
      "save_memory_proposal",
      "save_session_activity_digest",
      "list_session_activity_digests",
      "save_context_compaction_artifact",
      "list_context_compaction_artifacts",
    ],
  },
  {
    group: "subscriptions_and_buffers",
    reason:
      "Event subscription and runtime-buffer lease semantics are protocol/lifecycle concerns; bump MANIFEST_VERSION for breaking changes until buffer fixtures exist.",
    operations: [
      "subscribe_events",
      "unsubscribe_events",
      "get_buffer",
      "release_buffer",
    ],
  },
] as const satisfies readonly OperationExemptionGroup[];

export function assertBridgeValidationCoverageRatchet(
  rustFixtures: RustBridgeValidationFixtureSummary,
): void {
  assertEqual(
    "manifest operation count",
    manifestOperationNames.length,
    EXPECTED_MANIFEST_OPERATION_COUNT,
  );
  assertEqual(
    "Rust fixture operation count",
    rustFixtures.operation_count,
    EXPECTED_MANIFEST_OPERATION_COUNT,
  );

  const schemaExportNames = Object.keys(bridgeSchemas)
    .filter((name) => name.endsWith("Schema"))
    .sort();
  assertEqual(
    "TypeBox bridge schema export count",
    schemaExportNames.length,
    EXPECTED_TYPEBOX_SCHEMA_EXPORT_COUNT,
  );

  const fixtureFamilyNames = rustFixtures.fixtures
    .map((fixture) => fixture.name)
    .sort();
  assertEqual(
    "Rust fixture family count",
    fixtureFamilyNames.length,
    EXPECTED_RUST_FIXTURE_FAMILY_COUNT,
  );
  assertStringArrayEqual(
    "Rust fixture family names",
    fixtureFamilyNames,
    [...RUST_FIXTURE_FAMILY_NAMES].sort(),
  );

  const coverage = sortedUnique([
    ...RUNTIME_VALIDATED_MANIFEST_OPERATIONS,
    ...RUST_FIXTURE_BACKED_OPERATIONS,
  ]);
  assertEqual(
    "manifest operations with TypeBox validation or Rust fixtures",
    coverage.length,
    EXPECTED_MANIFEST_OPERATION_COVERAGE_COUNT,
  );

  const exemptions = sortedUnique(
    BRIDGE_OPERATION_EXEMPTION_GROUPS.flatMap((group) => group.operations),
  );
  assertEqual(
    "bridge manifest operation exemptions",
    exemptions.length,
    EXPECTED_EXEMPT_OPERATION_COUNT,
  );

  const manifest = new Set<string>(manifestOperationNames);
  const covered = new Set<string>(coverage);
  const exempted = new Set<string>(exemptions);
  const invalidCoverage = coverage.filter(
    (operation) => !manifest.has(operation),
  );
  const invalidExemptions = exemptions.filter(
    (operation) => !manifest.has(operation),
  );
  const overlappingExemptions = exemptions.filter((operation) =>
    covered.has(operation),
  );
  const missing = manifestOperationNames.filter(
    (operation) => !covered.has(operation) && !exempted.has(operation),
  );

  assertEmpty("coverage entries missing from manifest", invalidCoverage);
  assertEmpty("exemptions missing from manifest", invalidExemptions);
  assertEmpty("operations both covered and exempted", overlappingExemptions);
  assertEmpty("manifest operations needing coverage or exemption", missing);
}

function sortedUnique(values: readonly string[]): string[] {
  return [...new Set(values)].sort();
}

function assertEqual(label: string, actual: number, expected: number): void {
  if (actual === expected) return;
  throw new Error(`${label} expected ${expected}, got ${actual}`);
}

function assertEmpty(label: string, values: readonly string[]): void {
  if (values.length === 0) return;
  throw new Error(`${label}: ${values.join(", ")}`);
}

function assertStringArrayEqual(
  label: string,
  actual: readonly string[],
  expected: readonly string[],
): void {
  if (
    actual.length === expected.length &&
    actual.every((value, index) => value === expected[index])
  ) {
    return;
  }
  throw new Error(
    `${label} mismatch: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`,
  );
}
