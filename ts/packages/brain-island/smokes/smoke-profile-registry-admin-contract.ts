import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { apiCapabilityRegistry } from "../src/api-command-registry.js";
import {
  PROFILE_REGISTRY_ADMIN_OPENAPI_PATH,
  PROFILE_REGISTRY_ADMIN_PATHS,
  PROFILE_REGISTRY_ADMIN_REASON_CODES,
  PROFILE_REGISTRY_LIFECYCLE_STATUS_VALUES,
  PROFILE_REGISTRY_MUTATION_PLAN_REQUIRED_FIELDS,
  PROFILE_REGISTRY_RECORD_REQUIRED_FIELDS,
  PROFILE_REGISTRY_RUNTIME_CONFIG_REQUIRED_FIELDS,
  PROFILE_REGISTRY_SESSION_KIND_VALUES,
  PROFILE_REGISTRY_WRITE_KIND_VALUES,
  PROFILE_REGISTRY_WRITE_MODE_VALUES,
  PROFILE_REGISTRY_WRITE_REQUIRED_FIELDS,
  profileRegistryAdminPathToConcrete,
} from "../src/profile-registry-admin-contract.js";
import {
  isProfileRegistryWriteRoute,
  parseProfileRegistryWriteRoute,
} from "../src/service-profile-registry-routes.js";

const contractPath = resolve(
  process.cwd(),
  "../../../",
  PROFILE_REGISTRY_ADMIN_OPENAPI_PATH,
);
const contract = JSON.parse(readFileSync(contractPath, "utf8")) as OpenApiDoc;

assert.equal(contract.openapi, "3.1.0");
assert.equal(contract.info.title, "Rusty Crew Profile Registry Admin API");

for (const path of Object.values(PROFILE_REGISTRY_ADMIN_PATHS)) {
  assert.ok(contract.paths[path], `missing path ${path}`);
  const concretePath = profileRegistryAdminPathToConcrete(path);
  assert.equal(
    isProfileRegistryWriteRoute(concretePath),
    true,
    `route detector rejected ${concretePath}`,
  );
  const route = parseProfileRegistryWriteRoute(concretePath);
  assert.ok(route, `route parser rejected ${concretePath}`);
  assert.equal(route.profileId, "profile-alpha");
}

assert.deepEqual(schema("ProfileRegistryLifecycleStatus").enum, [
  ...PROFILE_REGISTRY_LIFECYCLE_STATUS_VALUES,
]);
assert.deepEqual(schema("ProfileRegistrySessionKind").enum, [
  ...PROFILE_REGISTRY_SESSION_KIND_VALUES,
]);
assert.deepEqual(schema("ProfileRegistryWriteKind").enum, [
  ...PROFILE_REGISTRY_WRITE_KIND_VALUES,
]);
assert.deepEqual(schema("ProfileRegistryWriteMode").enum, [
  ...PROFILE_REGISTRY_WRITE_MODE_VALUES,
]);
assert.deepEqual(
  schema("ProfileRegistryReasonCode").enum,
  Object.values(PROFILE_REGISTRY_ADMIN_REASON_CODES),
);
assert.deepEqual(schema("ProfileRegistryRecord").required, [
  ...PROFILE_REGISTRY_RECORD_REQUIRED_FIELDS,
]);
assert.deepEqual(schema("ProfileRegistryWrite").required, [
  ...PROFILE_REGISTRY_WRITE_REQUIRED_FIELDS,
]);
assert.deepEqual(schema("ProfileRegistryMutationPlan").required, [
  ...PROFILE_REGISTRY_MUTATION_PLAN_REQUIRED_FIELDS,
]);
assert.deepEqual(schema("ProfileRegistryRuntimeConfigEdit").required, [
  ...PROFILE_REGISTRY_RUNTIME_CONFIG_REQUIRED_FIELDS,
]);

const fieldRequest = schema("ProfileRegistryFieldUpdateRequest");
assert.ok(fieldRequest.properties?.expectedRevision);
assert.ok(fieldRequest.properties?.activeRuntimeSettingsJson);
const promptRequest = schema("ProfileRegistryPromptRequest");
assert.ok(promptRequest.properties?.soulMarkdown);
assert.ok(promptRequest.properties?.memoryMarkdown);
const runtimeConfigRequest = schema("ProfileRegistryRuntimeConfigRequest");
assert.ok(runtimeConfigRequest.properties?.providerAlias);
assert.ok(runtimeConfigRequest.properties?.mcpBindings);
assert.ok(schema("ProfileRegistryRuntimeConfigPlan").allOf?.length);
assert.ok(schema("ProfileRegistryLifecycleApplyEnvelope").allOf?.length);

const commandPaths = new Set(
  apiCapabilityRegistry().capabilities.map(
    (capability) => capability.path_template,
  ),
);
for (const path of Object.values(PROFILE_REGISTRY_ADMIN_PATHS)) {
  assert.ok(commandPaths.has(path), `command registry missing ${path}`);
}

console.log(
  JSON.stringify(
    {
      title: contract.info.title,
      paths: Object.values(PROFILE_REGISTRY_ADMIN_PATHS).length,
      reasonCodes: Object.values(PROFILE_REGISTRY_ADMIN_REASON_CODES).length,
    },
    null,
    2,
  ),
);

function schema(name: string): JsonSchema {
  const value = contract.components.schemas[name];
  assert.ok(value, `missing schema ${name}`);
  return value;
}

interface OpenApiDoc {
  openapi: string;
  info: { title: string; version: string };
  paths: Record<string, Record<string, unknown>>;
  components: {
    schemas: Record<string, JsonSchema>;
  };
}

interface JsonSchema {
  type?: string;
  enum?: string[];
  required?: string[];
  allOf?: JsonSchema[];
  properties?: Record<string, JsonSchema>;
}
