import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { nativeMappingInventory } from "./generated/native-mapping-inventory.js";

const sourcePath = fileURLToPath(new URL("./index.ts", import.meta.url));
const source = readFileSync(sourcePath, "utf8");
const profileRegistry = nativeMappingInventory.families.profileRegistry;
const modelProvider = nativeMappingInventory.families.modelProvider;

const nativeBridgeBinding = extractInterface("NativeBridgeBinding");
assertRawMethods("profile registry", profileRegistry.rawMethods);
assertRawMethods("model provider", modelProvider.rawMethods);

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

function assertRawMethods(label: string, methods: readonly string[]) {
  for (const method of methods) {
    assert(
      nativeBridgeBinding.includes(`${method}(`),
      `NativeBridgeBinding is missing generated-checked ${label} raw method ${method}`,
    );
  }
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
