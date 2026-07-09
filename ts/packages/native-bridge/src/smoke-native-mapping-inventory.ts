import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { nativeMappingInventory } from "./generated/native-mapping-inventory.js";

const sourcePath = fileURLToPath(new URL("./index.ts", import.meta.url));
const source = readFileSync(sourcePath, "utf8");
const modelProvider = nativeMappingInventory.families.modelProvider;

const nativeBridgeBinding = extractInterface("NativeBridgeBinding");
for (const method of modelProvider.rawMethods) {
  assert(
    nativeBridgeBinding.includes(`${method}(`),
    `NativeBridgeBinding is missing generated-checked model provider raw method ${method}`,
  );
}

for (const [interfaceName, fields] of Object.entries(modelProvider.dtoFields)) {
  const block = extractInterface(interfaceName);
  for (const field of fields) {
    assert(
      new RegExp(`\\b${escapeRegExp(field)}[?:]?`).test(block),
      `${interfaceName} is missing generated-checked raw field ${field}`,
    );
  }
}

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
    field === "affected_profiles"
      ? "affectedProfiles"
      : snakeToCamel(field),
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

function assertRawReads(functionName: string, parameter: string, reads: string[]) {
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
