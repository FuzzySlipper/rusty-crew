import { readFileSync } from "node:fs";

import type { BodyState, CoreEvent } from "@rusty-crew/contracts";
import type { TSchema } from "typebox";
import { Value } from "typebox/value";

import {
  BridgeValidationError,
  bridgeValidationEnabled,
  validateBridgeValue,
} from "./bridge-validation.js";
import { assertBridgeValidationCoverageRatchet } from "./bridge-validation-coverage.js";
import {
  actionBatchReceiptSchema,
  brainActionBatchSchema,
  brainEventEnvelopeSchema,
  brainWakeRequestSchema,
  eventReceiptSchema,
  openAiResponsesBrainRunInputSchema,
  rawBodyStateSchema,
  rawBufferedBrainRunDrainSchema,
  rawContextCompactionArtifactSchema,
  rawModelProviderRefreshImpactSchema,
  rawMemoryGovernanceDecisionRecordSchema,
  rawMemoryProposalRecordSchema,
  rawMemorySpaceDescriptorSchema,
  rawModelProviderRecordSchema,
  rawOpenAiResponsesBrainRunResultSchema,
  rawProfileRegistryRecordSchema,
  rawSessionActivityDigestSchema,
  rawSessionStateArraySchema,
} from "./bridge-validation-schemas.js";
import {
  loadNativeBridge,
  nativeManifestOperationNames,
  nativeManifestVersion,
  nativeWireShapeFingerprint,
  roundTripNativeBridgeFixture,
  type ChatCompletionsBrainRunInput,
  type NativeBridgeRoundTripFixtureName,
  type OpenAiResponsesBrainRunInput,
} from "./index.js";
import { toNativeOpenAiResponsesBrainRunInput } from "./brain-provider-input-wire.js";
import { toNativeChatCompletionsBrainRunInput } from "./brain-run-wire.js";
import {
  toCoreEvent,
  toNativeCoreEvent,
  type RawCoreEvent,
} from "./event-body-wire.js";

const validationEnv = { RUSTY_CREW_BRIDGE_VALIDATE: "1" };

const coordinationEvents: CoreEvent[] = [
  {
    type: "agent_message_delivery_observed",
    receipt: { marker: "delivery" } as unknown as Extract<
      CoreEvent,
      { type: "agent_message_delivery_observed" }
    >["receipt"],
  },
  {
    type: "agent_round_observed",
    round: { marker: "round" } as unknown as Extract<
      CoreEvent,
      { type: "agent_round_observed" }
    >["round"],
  },
];
for (const event of coordinationEvents) {
  const roundTripped = toCoreEvent(toNativeCoreEvent(event) as RawCoreEvent);
  if (roundTripped.type !== event.type) {
    throw new Error(
      `coordination event wire round trip changed ${event.type} to ${roundTripped.type}`,
    );
  }
}

interface RustBridgeValidationFixtureFile {
  manifest_version: number;
  operation_count: number;
  fixtures: Array<{
    name: string;
    operation: string;
    value: unknown;
  }>;
}

const rustFixtures = JSON.parse(
  readFileSync(
    new URL("../bridge-validation-rust-fixtures.json", import.meta.url),
    "utf8",
  ),
) as RustBridgeValidationFixtureFile;

const rustFixtureValues = new Map(
  rustFixtures.fixtures.map((fixture) => [fixture.name, fixture.value]),
);

if (rustFixtures.manifest_version !== nativeManifestVersion) {
  throw new Error(
    `Rust fixture manifest version ${rustFixtures.manifest_version} does not match TS native manifest version ${nativeManifestVersion}`,
  );
}

if (rustFixtures.operation_count !== nativeManifestOperationNames.length) {
  throw new Error(
    `Rust fixture operation count ${rustFixtures.operation_count} does not match TS manifest operation count ${nativeManifestOperationNames.length}`,
  );
}
assertBridgeValidationCoverageRatchet(rustFixtures);

const nativeBridge = await loadNativeBridge();
assertArrayEqual(
  "loaded native bridge operation inventory",
  nativeBridge.operationNames,
  nativeManifestOperationNames,
);
if (nativeBridge.wireShapeFingerprint !== nativeWireShapeFingerprint) {
  throw new Error(
    `loaded native bridge wire-shape fingerprint ${nativeBridge.wireShapeFingerprint} does not match expected ${nativeWireShapeFingerprint}`,
  );
}

assertBridgeValidationDefaults();

function rustFixture(name: string): unknown {
  const value = rustFixtureValues.get(name);
  if (value === undefined) {
    throw new Error(`missing Rust bridge validation fixture ${name}`);
  }
  return value;
}

function assertBridgeValidationDefaults(): void {
  const cases: Array<{
    label: string;
    env: Parameters<typeof bridgeValidationEnabled>[0];
    expected: boolean;
  }> = [
    { label: "default local", env: {}, expected: true },
    { label: "test", env: { NODE_ENV: "test" }, expected: true },
    {
      label: "production default",
      env: { NODE_ENV: "production" },
      expected: false,
    },
    {
      label: "explicit production on",
      env: {
        NODE_ENV: "production",
        RUSTY_CREW_BRIDGE_VALIDATE: "1",
      },
      expected: true,
    },
    {
      label: "explicit local off",
      env: { RUSTY_CREW_BRIDGE_VALIDATE: "0" },
      expected: false,
    },
  ];

  for (const item of cases) {
    const actual = bridgeValidationEnabled(item.env);
    if (actual !== item.expected) {
      throw new Error(
        `bridge validation default ${item.label} expected ${item.expected}, got ${actual}`,
      );
    }
  }
}

function validateRustFixture(input: {
  name: NativeBridgeRoundTripFixtureName;
  operation: string;
  schema: TSchema;
}): void {
  const value = rustFixture(input.name);
  assertSchemaCoversValueKeys({
    schema: input.schema,
    value,
    path: input.name,
  });
  validateBridgeValue({
    operation: input.operation,
    direction: "rust_to_ts",
    schema: input.schema,
    value,
    env: validationEnv,
  });
  assertRustFixtureMapperRoundTrips(input.name, value);
}

function assertRustFixtureMapperRoundTrips(
  name: NativeBridgeRoundTripFixtureName,
  value: unknown,
): void {
  const roundTripped = roundTripNativeBridgeFixture({ name, value });
  const actual = normalizeRoundTripValue(roundTripped);
  const expected = normalizeRoundTripValue(value);
  const actualJson = JSON.stringify(actual, null, 2);
  const expectedJson = JSON.stringify(expected, null, 2);
  if (actualJson === expectedJson) return;

  throw new Error(
    `Rust fixture ${name} did not round-trip through the TypeScript native-bridge mapper.\nExpected:\n${expectedJson}\nActual:\n${actualJson}`,
  );
}

function normalizeRoundTripValue(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.map(normalizeRoundTripValue);
  }

  if (!isPlainObject(value)) {
    return value ?? undefined;
  }

  const entries = Object.entries(value)
    .map(([key, entryValue]) => [key, normalizeRoundTripValue(entryValue)])
    .filter((entry): entry is [string, unknown] => entry[1] !== undefined)
    .sort(([left], [right]) => left.localeCompare(right));
  return Object.fromEntries(entries);
}

function assertSchemaCoversValueKeys(input: {
  schema: TSchema;
  value: unknown;
  path: string;
}): void {
  const schema = bestMatchingSchema(input.schema, input.value);
  if (schema === undefined) return;

  if (Array.isArray(input.value)) {
    const items = schemaRecord(schema).items;
    if (items && isSchema(items)) {
      input.value.forEach((item, index) =>
        assertSchemaCoversValueKeys({
          schema: items,
          value: item,
          path: `${input.path}[${index}]`,
        }),
      );
    }
    return;
  }

  if (!isPlainObject(input.value)) return;

  const record = schemaRecord(schema);
  if (record.type !== "object") return;
  const properties = schemaProperties(schema);
  if (properties === undefined) return;

  for (const [key, value] of Object.entries(input.value)) {
    const propertySchema = properties[key];
    if (propertySchema === undefined) {
      throw new Error(
        `Rust fixture ${input.path} has key ${key} that is not declared in the TypeScript bridge schema`,
      );
    }
    assertSchemaCoversValueKeys({
      schema: propertySchema,
      value,
      path: `${input.path}.${key}`,
    });
  }
}

function bestMatchingSchema(
  schema: TSchema,
  value: unknown,
): TSchema | undefined {
  const record = schemaRecord(schema);
  const branches = unionBranches(record);
  if (branches.length === 0) return schema;
  return branches.find((branch) => Value.Check(branch, value));
}

function unionBranches(record: Record<string, unknown>): TSchema[] {
  const branches = record.anyOf ?? record.oneOf ?? record.allOf;
  if (!Array.isArray(branches)) return [];
  return branches.filter(isSchema);
}

function schemaProperties(
  schema: TSchema,
): Record<string, TSchema> | undefined {
  const properties = schemaRecord(schema).properties;
  if (!isPlainObject(properties)) return undefined;
  const entries = Object.entries(properties).filter(
    (entry): entry is [string, TSchema] => isSchema(entry[1]),
  );
  return Object.fromEntries(entries);
}

function schemaRecord(schema: TSchema): Record<string, unknown> {
  return schema as unknown as Record<string, unknown>;
}

function isSchema(value: unknown): value is TSchema {
  return isPlainObject(value);
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function assertArrayEqual(
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
  const firstDiff = firstArrayDifference(actual, expected);
  throw new Error(
    `${label} mismatch at index ${firstDiff ?? "unknown"}: actual=${JSON.stringify(actual)} expected=${JSON.stringify(expected)}`,
  );
}

function firstArrayDifference(
  actual: readonly string[],
  expected: readonly string[],
): number | undefined {
  const length = Math.max(actual.length, expected.length);
  for (let index = 0; index < length; index += 1) {
    if (actual[index] !== expected[index]) return index;
  }
  return undefined;
}

const bodyState: BodyState = {
  session: {
    handle: 1 as BodyState["session"]["handle"],
    sessionId: "validation-session" as BodyState["session"]["sessionId"],
    agentId: "validation-agent" as BodyState["session"]["agentId"],
    profileId: "validation-profile" as BodyState["session"]["profileId"],
    kind: "full",
    resourceLimits: {},
    toolProfile: { tools: [] },
    status: "idle",
    brainTurnCount: 0,
    createdAt: "2026-07-02T00:00:00.000Z",
    lastActiveAt: "2026-07-02T00:00:00.000Z",
  },
  pendingMessages: [],
  recentEvents: [],
  childCompletions: [],
  fanOutGroups: [],
  deltaPolicy: {
    mode: "frozen_snapshot_next_wake",
    queueOwner: "body",
    queuedMessageTtlMs: 30_000,
    maxQueuedMessages: 20,
  },
};

const input: OpenAiResponsesBrainRunInput = {
  wakeId: "validation-wake",
  sessionId: "validation-session" as OpenAiResponsesBrainRunInput["sessionId"],
  bodyState,
  config: {
    model: "gpt",
    responsesDialect: "openai_stateless",
    providerRequestTimeoutMs: 30_000,
  },
  client: { mode: "fake" },
};

const compactionDomainContext = {
  schemaVersion: 1,
  retentionTiers: [],
  directorsNotes: [],
  extractionRequests: [],
};
const nativeResponsesInput = toNativeOpenAiResponsesBrainRunInput({
  ...input,
  compactionDomainContext,
}) as { compactionDomainContext?: unknown };
if (nativeResponsesInput.compactionDomainContext !== compactionDomainContext) {
  throw new Error(
    "OpenAI Responses wire mapping dropped compactionDomainContext",
  );
}
const nativeChatInput = toNativeChatCompletionsBrainRunInput({
  wakeId: "validation-chat-wake",
  sessionId: "validation-session" as ChatCompletionsBrainRunInput["sessionId"],
  messages: [{ role: "user", content: "Continue the scene." }],
  compaction_domain_context: compactionDomainContext,
  config: { model: "fake-model" },
  client: { mode: "fake" },
}) as { compactionDomainContext?: unknown };
if (nativeChatInput.compactionDomainContext !== compactionDomainContext) {
  throw new Error(
    "Chat Completions wire mapping dropped compaction_domain_context alias",
  );
}

validateBridgeValue<OpenAiResponsesBrainRunInput>({
  operation: "run_openai_responses_brain",
  direction: "ts_to_rust",
  schema: openAiResponsesBrainRunInputSchema,
  value: input,
  env: validationEnv,
});

validateBridgeValue({
  operation: "run_openai_responses_brain",
  direction: "rust_to_ts",
  schema: rawOpenAiResponsesBrainRunResultSchema,
  value: {
    stream: [
      {
        type: "event",
        event: {
          wake_id: "validation-wake",
          session_id: "validation-session",
          event: { type: "started" },
        },
      },
      {
        type: "actions",
        batch: {
          wake_id: "validation-wake",
          session_id: "validation-session",
          actions: [],
        },
      },
    ],
    provider_state: { type: "unchanged" },
  },
  env: validationEnv,
});

validateBridgeValue({
  operation: "wake_brain",
  direction: "ts_to_rust",
  schema: brainWakeRequestSchema,
  value: {
    brain: 1,
    sessionId: "validation-session",
    bodyState: 2,
    systemPrompt: 3,
    roleAssembly: 4,
    wakeId: "validation-wake",
  },
  env: validationEnv,
});

validateBridgeValue({
  operation: "submit_brain_event",
  direction: "ts_to_rust",
  schema: brainEventEnvelopeSchema,
  value: {
    wakeId: "validation-wake",
    sessionId: "validation-session",
    event: { type: "text_delta", text: "hello" },
  },
  env: validationEnv,
});

validateBridgeValue({
  operation: "submit_brain_actions",
  direction: "ts_to_rust",
  schema: brainActionBatchSchema,
  value: {
    wakeId: "validation-wake",
    sessionId: "validation-session",
    actions: [
      {
        type: "send_message",
        message: {
          from: "validation-agent",
          to: "operator",
          body: "hello",
        },
      },
    ],
  },
  env: validationEnv,
});

validateBridgeValue({
  operation: "submit_brain_event",
  direction: "rust_to_ts",
  schema: eventReceiptSchema,
  value: { accepted: true, sequence: 1 },
  env: validationEnv,
});

validateBridgeValue({
  operation: "submit_brain_actions",
  direction: "rust_to_ts",
  schema: actionBatchReceiptSchema,
  value: {
    wakeId: "validation-wake",
    acceptedActions: 1,
    rejectedActions: [],
  },
  env: validationEnv,
});

validateBridgeValue({
  operation: "list_sessions",
  direction: "rust_to_ts",
  schema: rawSessionStateArraySchema,
  value: [
    {
      handle: 1,
      session_id: "validation-session",
      agent_id: "validation-agent",
      profile_id: "validation-profile",
      kind: "full",
      resource_limits: {},
      tool_profile: { tools: [] },
      status: "idle",
      brain_turn_count: 0,
      created_at: "2026-07-02T00:00:00.000Z",
      last_active_at: "2026-07-02T00:00:00.000Z",
    },
  ],
  env: validationEnv,
});

validateBridgeValue({
  operation: "project_body_state_json",
  direction: "rust_to_ts",
  schema: rawBodyStateSchema,
  value: {
    session: {
      handle: 1,
      session_id: "validation-session",
      agent_id: "validation-agent",
      profile_id: "validation-profile",
      kind: "full",
      resource_limits: {},
      tool_profile: { tools: [] },
      status: "idle",
      brain_turn_count: 0,
      created_at: "2026-07-02T00:00:00.000Z",
      last_active_at: "2026-07-02T00:00:00.000Z",
    },
    pending_messages: [],
    recent_events: [],
    child_completions: [],
    fan_out_groups: [],
    delta_policy: {
      mode: "frozen_snapshot_next_wake",
      queue_owner: "body",
      queued_message_ttl_ms: 30_000,
      max_queued_messages: 20,
    },
  },
  env: validationEnv,
});

validateRustFixture({
  operation: "rust_fixture_project_body_state_json",
  schema: rawBodyStateSchema,
  name: "body_state_v1",
});

validateRustFixture({
  operation: "rust_fixture_list_sessions",
  schema: rawSessionStateArraySchema,
  name: "list_sessions_v1",
});

validateRustFixture({
  operation: "rust_fixture_drain_brain_run",
  schema: rawBufferedBrainRunDrainSchema,
  name: "buffered_brain_run_drain_v1",
});

validateRustFixture({
  operation: "rust_fixture_profile_registry_record",
  schema: rawProfileRegistryRecordSchema,
  name: "profile_registry_record_v1",
});

validateRustFixture({
  operation: "rust_fixture_model_provider_record",
  schema: rawModelProviderRecordSchema,
  name: "model_provider_record_v1",
});

validateRustFixture({
  operation: "rust_fixture_model_provider_refresh_impact",
  schema: rawModelProviderRefreshImpactSchema,
  name: "model_provider_refresh_impact_v1",
});

validateRustFixture({
  operation: "rust_fixture_memory_space_descriptor",
  schema: rawMemorySpaceDescriptorSchema,
  name: "memory_space_descriptor_v1",
});

validateRustFixture({
  operation: "rust_fixture_memory_proposal_record",
  schema: rawMemoryProposalRecordSchema,
  name: "memory_proposal_record_v1",
});

validateRustFixture({
  operation: "rust_fixture_memory_governance_decision_record",
  schema: rawMemoryGovernanceDecisionRecordSchema,
  name: "memory_governance_decision_record_v1",
});

validateRustFixture({
  operation: "rust_fixture_session_activity_digest",
  schema: rawSessionActivityDigestSchema,
  name: "session_activity_digest_v1",
});

validateRustFixture({
  operation: "rust_fixture_context_compaction_artifact",
  schema: rawContextCompactionArtifactSchema,
  name: "context_compaction_artifact_v1",
});

try {
  validateBridgeValue({
    operation: "run_openai_responses_brain",
    direction: "rust_to_ts",
    schema: rawOpenAiResponsesBrainRunResultSchema,
    value: { provider_state: { type: "unchanged" } },
    env: validationEnv,
  });
  throw new Error("expected bridge validation to reject missing stream");
} catch (error) {
  if (!(error instanceof BridgeValidationError)) throw error;
  if (
    !error.message.includes("run_openai_responses_brain") ||
    !error.message.includes("rust_to_ts")
  ) {
    throw new Error(`unexpected validation error message: ${error.message}`);
  }
}

try {
  validateBridgeValue({
    operation: "submit_brain_actions",
    direction: "ts_to_rust",
    schema: brainActionBatchSchema,
    value: {
      wakeId: "validation-wake",
      sessionId: "validation-session",
      actions: [{ type: "request_delegation", prompt: "missing profile" }],
    },
    env: validationEnv,
  });
  throw new Error("expected bridge validation to reject malformed action");
} catch (error) {
  if (!(error instanceof BridgeValidationError)) throw error;
  if (
    !error.message.includes("submit_brain_actions") ||
    !error.message.includes("ts_to_rust")
  ) {
    throw new Error(`unexpected validation error message: ${error.message}`);
  }
}

try {
  validateBridgeValue({
    operation: "list_sessions",
    direction: "rust_to_ts",
    schema: rawSessionStateArraySchema,
    value: [{ session_id: "missing required fields" }],
    env: validationEnv,
  });
  throw new Error("expected bridge validation to reject malformed session");
} catch (error) {
  if (!(error instanceof BridgeValidationError)) throw error;
  if (
    !error.message.includes("list_sessions") ||
    !error.message.includes("rust_to_ts")
  ) {
    throw new Error(`unexpected validation error message: ${error.message}`);
  }
}

console.log("bridge validation smoke passed");
