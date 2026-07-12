import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { apiCapabilityRegistry } from "../src/api-command-registry.js";
import {
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
assert.equal(contract.info.version, "0.1.0");

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
assert.ok(schema("ExternalThreadTurnProjection").properties?.items);
assert.ok(schema("ExternalThreadItemProjection").properties?.text);
assert.ok(schema("ExternalRuntimeRegistration").properties?.runtimeId);
assert.deepEqual(
  Object.keys(schema("DenRuntimeReference").properties ?? {}).sort(),
  ["project_id", "task_id"],
);
assert.ok(schema("AgentMessageDeliveryReceipt").properties?.request);
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
}

interface JsonSchema {
  enum?: string[];
  required?: string[];
  properties?: Record<string, unknown>;
}
