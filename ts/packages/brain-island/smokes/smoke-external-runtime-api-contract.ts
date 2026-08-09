import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { apiCapabilityRegistry } from "../src/api-command-registry.js";
import {
  EXTERNAL_BINDING_RESTORE_API_REASON_CODES,
  EXTERNAL_CONTROL_API_REASON_CODES,
  EXTERNAL_RUNTIME_API_CONTRACT_VERSION,
  EXTERNAL_RUNTIME_API_OPENAPI_PATH,
  EXTERNAL_RUNTIME_API_OPERATIONS,
  EXTERNAL_RUNTIME_API_PATHS,
} from "../src/external-runtime-api-contract.js";

const contract = JSON.parse(
  readFileSync(
    resolve(process.cwd(), "../../../", EXTERNAL_RUNTIME_API_OPENAPI_PATH),
    "utf8",
  ),
) as OpenApiDocument;

assert.equal(contract.openapi, "3.1.0");
assert.equal(EXTERNAL_RUNTIME_API_CONTRACT_VERSION, "0.18.0");
assert.equal(contract.info.version, EXTERNAL_RUNTIME_API_CONTRACT_VERSION);

const capabilityIds = new Set(
  apiCapabilityRegistry().capabilities.map((capability) => capability.id),
);
const operationIds = new Set<string>();
for (const operation of EXTERNAL_RUNTIME_API_OPERATIONS) {
  assert.ok(capabilityIds.has(operation.capabilityId));
  const contractOperation = contract.paths[operation.path]?.[operation.method];
  assert.ok(
    contractOperation,
    `missing ${operation.method.toUpperCase()} ${operation.path}`,
  );
  assert.equal(contractOperation.operationId, operation.operationId);
  assert.equal(
    contractOperation["x-rusty-crew-capability-id"],
    operation.capabilityId,
  );
  assert.equal(contractOperation["x-rusty-crew-contract-detail"], "wire");
  assert.ok(!operationIds.has(operation.operationId));
  operationIds.add(operation.operationId);
  assert.ok(contractOperation.responses.default?.content?.["application/json"]);
  if (operation.requestSchema !== undefined) {
    assert.equal(
      contractOperation.requestBody?.content?.["application/json"]?.schema
        ?.$ref,
      `#/components/schemas/${operation.requestSchema}`,
    );
  }
}

for (const path of Object.values(EXTERNAL_RUNTIME_API_PATHS)) {
  assert.ok(contract.paths[path], `missing external runtime path ${path}`);
}

assert.equal(
  contract.paths[EXTERNAL_RUNTIME_API_PATHS.stream]?.get?.responses["200"]
    ?.content?.["text/event-stream"]?.["x-rusty-crew-event-schema"]?.$ref,
  "#/components/schemas/NormalizedExternalRuntimeEvent",
);
assert.deepEqual(schema("ExternalThreadPage").required, [
  "items",
  "nextCursor",
  "backwardsCursor",
]);
assert.ok(schema("ExternalThreadProjection").properties?.turns);
assert.ok(
  schema("ExternalThreadProjection").required?.includes("effectiveModel"),
);
assert.deepEqual(
  propertySchema("ExternalThreadProjection", "effectiveModel").type,
  ["string", "null"],
);
assert.ok(schema("ExternalThreadTurnProjection").properties?.items);
assert.ok(schema("ExternalThreadTurnProjection").properties?.error);
assert.deepEqual(
  propertySchema("ExternalThreadTurnProjection", "statusSource").enum,
  ["native", "crew_terminal"],
);
assert.ok(schema("ExternalThreadItemProjection").properties?.text);
assert.deepEqual(
  propertySchema("ExternalThreadItemProjection", "messagePhase").enum,
  ["commentary", "final_answer", "unknown"],
);
assert.ok(schema("ExternalRuntimeCommandCatalog").properties?.models);
assert.ok(schema("ExternalRuntimeCommandExecutionResult").properties?.receipt);
assert.ok(schema("ExternalRuntimeEventPayload").properties?.settings);
assert.ok(schema("ExternalRuntimeEventPayload").properties?.usage);
assert.deepEqual(
  propertySchema("ExternalRuntimeEventPayload", "predecessorLifecycle").enum,
  ["retained", "archived"],
);
assert.equal(
  propertySchema("ExternalRuntimeEventPayload", "movedRouteCount").minimum,
  0,
);
assert.deepEqual(propertySchema("ExternalRuntimeEventPayload", "error"), {
  $ref: "#/components/schemas/ExternalThreadTurnErrorProjection",
});
assert.equal(
  contract.paths[EXTERNAL_RUNTIME_API_PATHS.commands]?.get?.operationId,
  "listExternalBindingCommands",
);
assert.equal(
  contract.paths[EXTERNAL_RUNTIME_API_PATHS.commands]?.post?.operationId,
  "executeExternalBindingCommand",
);
assert.equal(
  contract.paths[EXTERNAL_RUNTIME_API_PATHS.bindingMetadata]?.post?.operationId,
  "writeExternalBindingMetadata",
);
assert.deepEqual(
  contract.paths[EXTERNAL_RUNTIME_API_PATHS.controls]?.post?.[
    "x-rusty-crew-error-reason-codes"
  ],
  EXTERNAL_CONTROL_API_REASON_CODES,
);
assert.deepEqual(schema("ExternalBindingMetadataWrite").required, [
  "expectedRevision",
  "label",
  "taskRef",
]);
assert.equal(
  contract.paths[EXTERNAL_RUNTIME_API_PATHS.bindingRestore]?.post?.operationId,
  "restoreExternalBinding",
);
assert.deepEqual(
  contract.paths[EXTERNAL_RUNTIME_API_PATHS.bindingRestore]?.post?.[
    "x-rusty-crew-error-reason-codes"
  ],
  EXTERNAL_BINDING_RESTORE_API_REASON_CODES,
);
assert.deepEqual(schema("ExternalBindingRestoreWrite").required, [
  "expectedBindingRevision",
  "expectedSessionId",
  "expectedAgentId",
  "expectedProfileId",
  "expectedNativeThreadId",
]);
assert.ok(schema("ExternalAgentBindingRestoreReceipt").properties?.binding);
assert.ok(schema("ExternalAgentBinding").properties?.label);
assert.deepEqual(
  propertySchema("ExternalRuntimeEventPayload", "messagePhase").enum,
  ["commentary", "final_answer", "unknown"],
);
assert.deepEqual(
  propertySchema("ExternalThreadDeleteReceipt", "outcome").enum,
  ["applied", "already_deleted"],
);
assert.equal(
  propertySchema("ExternalThreadDeleteReceipt", "nativeDeleted").const,
  true,
);
assert.ok(schema("ExternalRuntimeRegistration").properties?.runtimeId);
assert.deepEqual(schema("ExternalRuntimeCompatibilityState").enum, [
  "unassessed",
  "compatible_uncertified",
  "certified",
  "incompatible",
]);
assert.deepEqual(
  propertySchema("ExternalRuntimeControllerStatus", "compatibilityDiagnostic")
    .enum,
  [
    "certified",
    "compatible_uncertified",
    "incompatible",
    "probe_failed",
    "disconnected",
  ],
);
assert.ok(schema("ExternalRuntimeCertificationRecord").properties?.revision);
assert.ok(schema("ExternalRuntimePromotionReadiness").properties?.activeTurns);
assert.equal(
  contract.paths[EXTERNAL_RUNTIME_API_PATHS.promotionReadiness]?.get
    ?.parameters?.[0]?.required,
  true,
);
assert.deepEqual(schema("ExternalRuntimeCertificationWrite").required, [
  "certificationId",
  "idempotencyKey",
  "runtimeId",
  "evidenceSummary",
]);
assert.ok(schema("ExternalRuntimeRegistration").properties?.observedCliVersion);
assert.ok(
  schema("ExternalRuntimeRegistration").properties?.consumedContractRevision,
);
assert.equal(
  schema("ExternalRuntimeRegistration").properties?.expectedCliVersion,
  undefined,
);
assert.ok(
  schema("ExternalRuntimeControllerStatus").properties?.bindingResumeFailures,
);
assert.ok(
  schema("ExternalRuntimeControllerStatus").properties?.lastCompatibilityProbe,
);
assert.deepEqual(
  propertySchema("ExternalRuntimeControllerStatus", "recovery").properties
    ?.phase?.enum,
  ["idle", "scheduled", "attempting", "succeeded", "failed"],
);
assert.ok(schema("ExternalRuntimeCompatibilityProbeReport").properties?.steps);
assert.deepEqual(
  Object.keys(schema("DenRuntimeReference").properties ?? {}).sort(),
  ["project_id", "task_id"],
);
assert.ok(schema("AgentMessageDeliveryReceipt").properties?.request);
assert.ok(schema("ExternalTurnCorrelation").properties?.terminalReasonCode);
assert.ok(schema("AgentCorrelatedRound").properties?.status);
assert.ok(schema("ExternalInteractionRecord").properties?.allowedResponses);
assert.deepEqual(schema("ExternalCollaborationMode").enum, ["plan"]);
assert.deepEqual(
  schema("ExternalBindingMessageWrite").properties?.collaborationMode,
  { $ref: "#/components/schemas/ExternalCollaborationMode" },
);
assert.equal(
  contract["x-rusty-crew-generated"].native_payload_policy,
  "bounded_raw_detail_only",
);

console.log(
  JSON.stringify({
    artifact: EXTERNAL_RUNTIME_API_OPENAPI_PATH,
    paths: Object.keys(contract.paths).length,
    operations: operationIds.size,
    generatedCoreSchemas: Object.keys(contract.components.schemas).length,
    sseEvent: "NormalizedExternalRuntimeEvent",
  }),
);

function schema(name: string): JsonSchema {
  const value = contract.components.schemas[name];
  assert.ok(value, `missing schema ${name}`);
  return value;
}

function propertySchema(schemaName: string, propertyName: string): JsonSchema {
  const value = schema(schemaName).properties?.[propertyName];
  assert.ok(
    typeof value === "object" && value !== null,
    `missing schema ${schemaName}.${propertyName}`,
  );
  return value as JsonSchema;
}

interface OpenApiDocument {
  openapi: string;
  info: { version: string };
  paths: Record<string, Partial<Record<"get" | "post", Operation>>>;
  components: { schemas: Record<string, JsonSchema> };
  "x-rusty-crew-generated": { native_payload_policy: string };
}

interface Operation {
  operationId: string;
  requestBody?: {
    content?: Record<string, { schema?: { $ref?: string } }>;
  };
  responses: Record<
    string,
    {
      content?: Record<
        string,
        {
          "x-rusty-crew-event-schema"?: { $ref?: string };
        }
      >;
    }
  >;
  "x-rusty-crew-capability-id": string;
  "x-rusty-crew-contract-detail": string;
  "x-rusty-crew-error-reason-codes"?: readonly string[];
}

interface JsonSchema {
  const?: unknown;
  enum?: string[];
  type?: string | string[];
  required?: string[];
  properties?: Record<string, unknown>;
}
