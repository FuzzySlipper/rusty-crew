import {
  manifestOperationNames,
  type ManifestOperationName,
} from "@rusty-crew/contracts";

import * as bridgeSchemas from "./bridge-validation-schemas.js";
import { bridgeWireSchemaArtifact } from "./generated/bridge-wire-schemas.js";
import { directBridgeValidatedOperations } from "./direct-binding-validation.js";

export interface RustBridgeValidationFixtureSummary {
  operation_count: number;
  fixtures: Array<{
    name: string;
    operation: string;
  }>;
}

const EXPECTED_MANIFEST_OPERATION_COUNT = 238;
const EXPECTED_TYPEBOX_SCHEMA_EXPORT_COUNT = 41;
const EXPECTED_RUST_FIXTURE_FAMILY_COUNT = 11;
const EXPECTED_GENERATED_OUTPUT_SCHEMA_COUNT = 154;
const EXPECTED_UNIT_RETURN_OPERATION_COUNT = 13;
const EXPECTED_MANIFEST_OPERATION_COVERAGE_COUNT = 238;
const EXPECTED_EXEMPT_OPERATION_COUNT = 0;

const RUNTIME_VALIDATED_MANIFEST_OPERATIONS = [
  "append_chat_event",
  "apply_curator_governance_write",
  "chat_read_model_page",
  "create_profile_registry_record",
  "drain_brain_run",
  "get_model_provider",
  "get_curator_candidate",
  "get_curator_mutation",
  "get_profile_registry_record",
  "list_context_compaction_artifacts",
  "list_curator_audit_receipts",
  "list_curator_candidates",
  "list_curator_mutations",
  "list_model_providers",
  "list_profile_registry_records",
  "list_session_activity_digests",
  "list_sessions",
  "model_provider_refresh_impact",
  "plan_model_provider_refresh",
  "plan_channel_ingress_route",
  "plan_den_product_ingress_policy",
  "provider_state_diagnostics",
  "purge_profile",
  "query_chat_events",
  "save_context_compaction_artifact",
  "save_session_activity_digest",
  "start_brain_run",
  "submit_brain_host_result",
  "cancel_brain_run",
  "database_size",
  "delete_simple_kv",
  "list_simple_kv",
  "put_simple_kv",
  "run_maintenance",
  "storage_diagnostics",
  "storage_schema",
  "submit_brain_actions",
  "submit_brain_event",
  "update_profile_registry_record",
  "upsert_model_provider",
  "validate_tool_metadata_policy",
  "validate_local_tool_profile_policy",
  "wake_brain",
  ...directBridgeValidatedOperations,
] as const satisfies readonly ManifestOperationName[];

const RUST_FIXTURE_BACKED_OPERATIONS = [
  "project_body_state",
  "list_sessions",
  "drain_brain_run",
  "list_profile_registry_records",
  "list_model_providers",
  "model_provider_refresh_impact",
  "list_memory_space_descriptors",
  "list_memory_proposals",
  "record_memory_governance_decision",
  "save_session_activity_digest",
  "list_session_activity_digests",
  "save_context_compaction_artifact",
  "list_context_compaction_artifacts",
] as const satisfies readonly ManifestOperationName[];

const RUST_FIXTURE_FAMILY_NAMES = [
  "body_state_v1",
  "context_compaction_artifact_v1",
  "list_sessions_v1",
  "buffered_brain_run_drain_v1",
  "profile_registry_record_v1",
  "model_provider_record_v1",
  "model_provider_refresh_impact_v1",
  "memory_space_descriptor_v1",
  "memory_proposal_record_v1",
  "memory_governance_decision_record_v1",
  "session_activity_digest_v1",
] as const;

const UNIT_RETURN_MANIFEST_OPERATIONS = [
  "add_entry_to_layer",
  "apply_brain_provider_state_output",
  "complete_scheduled_host_run",
  "pause_scheduled_job",
  "release_buffer",
  "remove_entry_from_layer",
  "reorder_chat_layers",
  "resume_scheduled_job",
  "save_message_slot",
  "set_chat_layers",
  "set_entry_constant",
  "toggle_chat_layer",
  "unsubscribe_events",
] as const satisfies readonly ManifestOperationName[];

const BRIDGE_COVERAGE_GREENPATH = [
  "Bridge coverage ratchet failed.",
  "Follow docs/bridge-contract-validation.md#adding-a-bridge-family and docs/native-bridge-rust-contract-mapping-migration.md.",
  "Greenpath: manifest operation -> Rust operation name -> native surface check -> Rust fixture or exact exemption -> fingerprint update when fixture-backed -> TypeBox schema/validation -> wrapper mapping.",
  "Useful commands: npm run smoke:bridge-contract-parity && npm run smoke:bridge-native-surface && npm run codegen:bridge-fixtures && npm run codegen:bridge-fingerprint && npm run smoke:bridge-fixture-drift && npm run smoke:bridge-fingerprint-drift && npm run smoke:bridge-validation.",
  "If the operation is intentionally uncovered, add it to exactly one BRIDGE_OPERATION_EXEMPTION_GROUP with a narrow reason; active UI/service families should prefer fixture and schema coverage.",
].join("\n");

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
    ...UNIT_RETURN_MANIFEST_OPERATIONS,
    ...(Object.keys(
      bridgeWireSchemaArtifact.operationSchemaKeys,
    ) as ManifestOperationName[]),
  ]);
  assertEqual(
    "unit-return manifest operation count",
    UNIT_RETURN_MANIFEST_OPERATIONS.length,
    EXPECTED_UNIT_RETURN_OPERATION_COUNT,
  );
  assertEqual(
    "generated Rust bridge output schema count",
    Object.keys(bridgeWireSchemaArtifact.operationSchemaKeys).length,
    EXPECTED_GENERATED_OUTPUT_SCHEMA_COUNT,
  );
  assertEqual(
    "manifest operations with TypeBox validation or Rust fixtures",
    coverage.length,
    EXPECTED_MANIFEST_OPERATION_COVERAGE_COUNT,
  );

  const exemptions: ManifestOperationName[] = [];
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
  throw new Error(
    `${label} expected ${expected}, got ${actual}\n\n${BRIDGE_COVERAGE_GREENPATH}`,
  );
}

function assertEmpty(label: string, values: readonly string[]): void {
  if (values.length === 0) return;
  throw new Error(
    `${label}: ${values.join(", ")}\n\n${BRIDGE_COVERAGE_GREENPATH}`,
  );
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
    `${label} mismatch: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}\n\n${BRIDGE_COVERAGE_GREENPATH}`,
  );
}
