import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { nativeMappingInventory } from "./generated/native-mapping-inventory.js";

const sourcePath = fileURLToPath(new URL("./index.ts", import.meta.url));
const source = readFileSync(sourcePath, "utf8");
const memoryWrapperSourcePath = fileURLToPath(
  new URL("./memory-wrappers.ts", import.meta.url),
);
const bridgeSources = `${source}\n${readFileSync(memoryWrapperSourcePath, "utf8")}`;
const memory = nativeMappingInventory.families.memory;
const brainProvider = nativeMappingInventory.families.brainProvider;
const roleplay = nativeMappingInventory.families.roleplay;
const conversation = nativeMappingInventory.families.conversation;
const profileRegistry = nativeMappingInventory.families.profileRegistry;
const modelProvider = nativeMappingInventory.families.modelProvider;

const nativeBridgeBinding = extractInterface("NativeBridgeBinding");
assertRawMethods("memory", memory.rawMethods);
assertRawMethods("brain/provider", brainProvider.rawMethods);
assertRawMethods("roleplay", roleplay.rawMethods);
assertRawMethods("conversation", conversation.rawMethods);
assertRawMethods("profile registry", profileRegistry.rawMethods);
assertRawMethods("model provider", modelProvider.rawMethods);

assertGeneratedDtoFieldsNonEmpty("memory", memory.dtoFields);
assertWrapperCalls("memory", memory.passthroughWrappers, memory.rawMethods);
assertJsonInputWrappers(
  "memory",
  memory.jsonInputWrappers,
  memory.jsonInputRawMethods,
);
assertDirectNativeMethods("memory", memory.directNativeMethods);
assertGeneratedDtoFieldsNonEmpty("brain/provider", brainProvider.dtoFields);
assertWrapperCalls(
  "brain/provider",
  brainProvider.passthroughWrappers,
  brainProvider.rawMethods,
);
assertDirectNativeMethods("brain/provider", brainProvider.directNativeMethods);
assertNamedBrainProviderInterfaces();
assertRawReads("toBrainWakeStreamItem", "item", [
  "item.event.wake_id",
  "item.event.session_id",
  "item.event.event",
  "item.batch.wake_id",
  "item.batch.session_id",
  "item.batch.actions",
  "item.failure.wake_id",
  "item.failure.session_id",
  "item.failure.kind",
  "item.failure.message",
]);
assertRawReads("toRawBrainWakeStreamItem", "item", [
  "item.event.wakeId",
  "item.event.sessionId",
  "item.event.event",
  "item.batch.wakeId",
  "item.batch.sessionId",
  "item.batch.actions",
  "item.failure.wakeId",
  "item.failure.sessionId",
  "item.failure.kind",
  "item.failure.message",
]);
assertRawReads("toBrainWakeProviderStateOutput", "output", [
  "output.state.module_id",
  "output.state.strategy_id",
  "output.state.profile_fingerprint",
  "output.state.provider_fingerprint",
  "output.state.payload_version",
  "output.state.payload",
  "output.state.ttl_ms",
  "output.reason",
]);
assertRawReads("toRawBrainWakeProviderStateOutput", "output", [
  "output.state.moduleId",
  "output.state.strategyId",
  "output.state.profileFingerprint",
  "output.state.providerFingerprint",
  "output.state.payloadVersion",
  "output.state.payload",
  "output.state.ttlMs",
  "output.reason",
]);
assertRawReads("toToolCallMetadata", "metadata", [
  "metadata.source",
  "metadata.adapter_id",
  "metadata.binding_id",
  "metadata.server_names",
  "metadata.profile_id",
  "metadata.tool_profile_key",
  "metadata.source_tool_name",
  "metadata.catalog_revision",
  "metadata.debug_detail_id",
  "metadata.policy",
]);
assertRawReads("toNativeProviderStateDiagnostic", "raw", [
  ...brainProvider.dtoFields.NativeProviderStateDiagnostic.map(
    (field) => `raw.${field}`,
  ),
]);
assertGeneratedDtoFieldsNonEmpty("roleplay", roleplay.dtoFields);
assertWrapperCalls(
  "roleplay",
  roleplay.passthroughWrappers,
  roleplay.rawMethods,
);
assertJsonInputWrappers(
  "roleplay",
  roleplay.jsonInputWrappers,
  roleplay.jsonInputRawMethods,
);
assertGeneratedDtoFieldsNonEmpty("conversation", conversation.dtoFields);
assertNamedConversationInterfaces();
assertPassthroughWrappers(
  "conversation",
  conversation.passthroughWrappers,
  conversation.rawMethods,
);
assertDtoFields(profileRegistry.dtoFields);
assertDtoFields(modelProvider.dtoFields);

assertRawReads("toNativeProfileRegistryWrite", "write", [
  ...profileRegistry.dtoFields.RawProfileRegistryWrite.filter(
    (field) =>
      field !== "source_asset_refs" &&
      field !== "derived_runtime_refs" &&
      field !== "import_export",
  ).map((field) => `write.${field}`),
  "write.source_asset_refs",
  "write.derived_runtime_refs",
  "write.import_export",
]);
assertNativeReads("toRawProfileRegistryWrite", "write", [
  ...profileRegistry.dtoFields.RawProfileRegistryWrite.filter(
    (field) =>
      field !== "source_asset_refs" &&
      field !== "derived_runtime_refs" &&
      field !== "import_export",
  ),
  "source_asset_refs",
  "derived_runtime_refs",
  "import_export",
]);
assertNativeReads("toRawProfileRegistryUpdate", "update", [
  ...profileRegistry.dtoFields.RawProfileRegistryUpdate.filter(
    (field) => field !== "write",
  ),
]);
assertNativeReads("toRawProfileRegistryMutationRequest", "request", [
  ...profileRegistry.dtoFields.RawProfileRegistryMutationRequest.filter(
    (field) => field !== "current",
  ),
]);
assertRawReads("toNativeProfileRegistryMutationPlan", "plan", [
  ...profileRegistry.dtoFields.RawProfileRegistryMutationPlan.filter(
    (field) =>
      field !== "current" &&
      field !== "next" &&
      field !== "next_write" &&
      field !== "diagnostics" &&
      field !== "implications",
  ).map((field) => `plan.${field}`),
  "plan.current",
  "plan.next",
  "plan.next_write",
  "plan.diagnostics",
  ...profileRegistry.dtoFields.RawProfileRegistryMutationImplications.map(
    (field) => `plan.implications.${field}`,
  ),
]);
assertRawReads("toNativeProfileRegistryRecord", "record", [
  ...profileRegistry.dtoFields.RawProfileRegistryRecord.filter(
    (field) =>
      field !== "source_asset_refs" &&
      field !== "derived_runtime_refs" &&
      field !== "import_export",
  ).map((field) => `record.${field}`),
  "record.source_asset_refs",
  "record.derived_runtime_refs",
  "record.import_export",
]);
assertNativeReads("toRawProfileRegistryRecord", "record", [
  ...profileRegistry.dtoFields.RawProfileRegistryRecord.filter(
    (field) =>
      field !== "source_asset_refs" &&
      field !== "derived_runtime_refs" &&
      field !== "import_export",
  ),
  "source_asset_refs",
  "derived_runtime_refs",
  "import_export",
]);
assertRawReads("toNativeProfilePurgeReport", "report", [
  ...profileRegistry.dtoFields.RawProfilePurgeReport.filter(
    (field) => field !== "table_counts",
  ).map((field) => `report.${field}`),
  "report.table_counts",
  ...profileRegistry.dtoFields.RawProfilePurgeTableCount.map(
    (field) => `count.${field}`,
  ),
]);
assertRawReads("toNativeProfileRegistryAssetRef", "ref", [
  ...profileRegistry.dtoFields.RawProfileRegistrySourceAssetRef.map(
    (field) => `ref.${field}`,
  ),
]);
assertNativeReads("toRawProfileRegistryAssetRef", "ref", [
  ...profileRegistry.dtoFields.RawProfileRegistrySourceAssetRef,
]);
assertRawReads("toNativeProfileRegistryRuntimeRef", "ref", [
  ...profileRegistry.dtoFields.RawProfileRegistryDerivedRuntimeRef.map(
    (field) => `ref.${field}`,
  ),
]);
assertNativeReads("toRawProfileRegistryRuntimeRef", "ref", [
  ...profileRegistry.dtoFields.RawProfileRegistryDerivedRuntimeRef,
]);
assertRawReads("toNativeProfileRegistryImportExport", "metadata", [
  ...profileRegistry.dtoFields.RawProfileRegistryImportExportMetadata.map(
    (field) => `metadata.${field}`,
  ),
]);
assertNativeReads("toRawProfileRegistryImportExport", "metadata", [
  ...profileRegistry.dtoFields.RawProfileRegistryImportExportMetadata,
]);

assertRawReads("toNativeModelProviderRecord", "record", [
  ...modelProvider.dtoFields.RawModelProviderRecord.filter(
    (field) => field !== "credential",
  ).map((field) => `record.${field}`),
  ...modelProvider.dtoFields.RawModelProviderCredential.map(
    (field) => `record.credential.${field}`,
  ),
]);
assertNativeReads("toRawModelProviderRecord", "record", [
  ...modelProvider.dtoFields.RawModelProviderRecord.filter(
    (field) => field !== "credential",
  ),
  ...modelProvider.dtoFields.RawModelProviderCredential.map(
    (field) => `credential.${field}`,
  ),
]);
assertRawReads("toNativeModelProviderRefreshImpact", "impact", [
  "impact.provider_alias",
  "impact.affected_profiles",
  ...modelProvider.dtoFields.RawModelProviderAffectedProfile.map(
    (field) => `profile.${field}`,
  ),
]);
assertRawReads("toRawModelProviderRefreshImpact", "impact", [
  ...modelProvider.dtoFields.RawModelProviderRefreshImpact.map((field) =>
    field === "affected_profiles" ? "affectedProfiles" : snakeToCamel(field),
  ).map((field) => `impact.${field}`),
]);
assertRawReads("toNativeModelProviderRefreshPlan", "plan", [
  "plan.provider_alias",
  "plan.mode",
  "plan.affected_profiles",
  "plan.actions",
  ...modelProvider.dtoFields.RawModelProviderAffectedProfile.map(
    (field) => `profile.${field}`,
  ),
  ...modelProvider.dtoFields.RawModelProviderRefreshProfileAction.map(
    (field) => `action.${field}`,
  ),
]);

console.log("native mapping inventory smoke passed");

function assertGeneratedDtoFieldsNonEmpty(
  label: string,
  dtoFields: Record<string, readonly string[]>,
) {
  for (const [dtoName, fields] of Object.entries(dtoFields)) {
    assert(
      fields.length > 0,
      `${label} generated DTO ${dtoName} has no fields`,
    );
  }
}

function assertNamedConversationInterfaces() {
  const interfaceFieldMap: Record<string, readonly string[]> = {
    NativeChatReadModelEvent: conversation.dtoFields.ChatReadModelEvent,
    NativeChatReadModelPage: conversation.dtoFields.ChatReadModelPage,
    NativeChatEventLogEvent: conversation.dtoFields.ChatEventLogEvent,
    NativeChatEventLogPage: conversation.dtoFields.ChatEventLogPage,
  };
  for (const interfaceName of conversation.namedTypeScriptInterfaces) {
    const fields = interfaceFieldMap[interfaceName];
    assert(fields, `missing generated field map for ${interfaceName}`);
    const block = extractInterface(interfaceName);
    for (const field of fields) {
      assert(
        new RegExp(`\\b${escapeRegExp(field)}[?:]?`).test(block),
        `${interfaceName} is missing generated-checked conversation field ${field}`,
      );
    }
  }
}

function assertNamedBrainProviderInterfaces() {
  for (const interfaceName of brainProvider.namedTypeScriptInterfaces) {
    assert(
      source.includes(`interface ${interfaceName}`) ||
        source.includes(`type ${interfaceName}`),
      `brain/provider expected named interface ${interfaceName} in native bridge source`,
    );
  }
}

function assertRawMethods(label: string, methods: readonly string[]) {
  for (const method of methods) {
    assert(
      nativeBridgeBinding.includes(`${method}(`),
      `NativeBridgeBinding is missing generated-checked ${label} raw method ${method}`,
    );
  }
}

function assertDirectNativeMethods(label: string, methods: readonly string[]) {
  const moduleInterface = extractInterface("NativeBridgeModule");
  for (const method of methods) {
    assert(
      nativeBridgeBinding.includes(`${method}(`),
      `NativeBridgeBinding is missing generated-checked ${label} direct method ${method}`,
    );
    assert(
      moduleInterface.includes(`${method}(`),
      `NativeBridgeModule is missing generated-checked ${label} direct wrapper ${method}`,
    );
    assert(
      hasBindingCall(method),
      `${label} direct wrapper ${method} must call the matching native method`,
    );
  }
}

function assertPassthroughWrappers(
  label: string,
  wrappers: readonly string[],
  rawMethods: readonly string[],
) {
  assertWrapperCalls(label, wrappers, rawMethods);
  assertJsonInputWrappers(label, wrappers, rawMethods);
}

function assertWrapperCalls(
  label: string,
  wrappers: readonly string[],
  rawMethods: readonly string[],
) {
  assert.equal(
    wrappers.length,
    rawMethods.length,
    `${label} wrapper/raw method inventory length mismatch`,
  );
  const moduleInterface = extractInterface("NativeBridgeModule");
  for (const [index, wrapper] of wrappers.entries()) {
    const rawMethod = rawMethods[index];
    assert(
      moduleInterface.includes(`${wrapper}(`),
      `NativeBridgeModule is missing generated-checked ${label} wrapper ${wrapper}`,
    );
    assert(
      hasBindingCall(rawMethod),
      `${label} wrapper ${wrapper} must call generated-checked raw method ${rawMethod}`,
    );
  }
}

function assertJsonInputWrappers(
  label: string,
  wrappers: readonly string[],
  rawMethods: readonly string[],
) {
  assert.equal(
    wrappers.length,
    rawMethods.length,
    `${label} JSON wrapper/raw method inventory length mismatch`,
  );
  for (const [index, wrapper] of wrappers.entries()) {
    const rawMethod = rawMethods[index];
    const callIndex = bindingCallIndex(rawMethod);
    assert.notEqual(
      callIndex,
      -1,
      `${label} wrapper ${wrapper} must call generated-checked raw method ${rawMethod}`,
    );
    const callWindow = bridgeSources.slice(callIndex, callIndex + 240);
    assert(
      callWindow.includes("JSON.stringify("),
      `${label} wrapper ${wrapper} must pass input through ${rawMethod} with JSON.stringify`,
    );
  }
}

function hasBindingCall(method: string): boolean {
  return bindingCallIndex(method) >= 0;
}

function bindingCallIndex(method: string): number {
  const direct = bridgeSources.indexOf(`binding.${method}(`);
  if (direct >= 0) return direct;
  const pattern = new RegExp(
    `binding\\s*\\.\\s*${escapeRegExp(method)}\\s*\\(`,
  );
  const match = pattern.exec(bridgeSources);
  return match?.index ?? -1;
}

function assertDtoFields(dtoFields: Record<string, readonly string[]>) {
  for (const [interfaceName, fields] of Object.entries(dtoFields)) {
    const block = extractInterface(interfaceName);
    for (const field of fields) {
      assert(
        new RegExp(`\\b${escapeRegExp(field)}[?:]?`).test(block),
        `${interfaceName} is missing generated-checked raw field ${field}`,
      );
    }
  }
}

function assertRawReads(
  functionName: string,
  parameter: string,
  reads: string[],
) {
  const block = extractFunction(functionName);
  for (const read of reads) {
    assert(
      block.includes(read),
      `${functionName} must read generated-checked raw field ${read} from ${parameter}`,
    );
  }
}

function assertNativeReads(
  functionName: string,
  parameter: string,
  rawFields: string[],
) {
  const block = extractFunction(functionName);
  for (const rawField of rawFields) {
    const nativeRead = rawField
      .split(".")
      .map((part) => snakeToCamel(part))
      .join(".");
    assert(
      block.includes(`${parameter}.${nativeRead}`),
      `${functionName} must read generated-checked native field ${parameter}.${nativeRead} for raw field ${rawField}`,
    );
  }
}

function extractInterface(name: string): string {
  const marker = `interface ${name}`;
  const start = source.indexOf(marker);
  assert.notEqual(start, -1, `missing interface ${name}`);
  return extractBracedBlock(source, source.indexOf("{", start));
}

function extractFunction(name: string): string {
  const marker = `function ${name}`;
  const start = source.indexOf(marker);
  assert.notEqual(start, -1, `missing function ${name}`);
  return extractBracedBlock(source, source.indexOf("{", start));
}

function extractBracedBlock(text: string, openBrace: number): string {
  assert(openBrace >= 0, "missing opening brace");
  let depth = 0;
  for (let index = openBrace; index < text.length; index += 1) {
    const char = text[index];
    if (char === "{") depth += 1;
    if (char === "}") {
      depth -= 1;
      if (depth === 0) return text.slice(openBrace, index + 1);
    }
  }
  throw new Error("missing closing brace");
}

function snakeToCamel(value: string): string {
  return value.replace(/_([a-z])/g, (_match, letter: string) =>
    letter.toUpperCase(),
  );
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
