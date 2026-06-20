import assert from "node:assert/strict";
import {
  buildToolInventory,
  createToolRegistry,
  defaultToolRegistry,
  validateToolRegistry,
} from "./tool-registry.js";
import type { ToolRegistryEntry } from "./tool-registry.js";

const readInventory = buildToolInventory(defaultToolRegistry, {
  requestedToolsets: ["local_code_read"],
});

assert.deepEqual(
  readInventory.selectedTools.map((tool) => tool.name),
  ["read_file", "search_files", "git_status", "git_diff"],
);
assert.deepEqual(
  readInventory.selectedDescriptors.map((tool) => tool.name),
  readInventory.selectedTools.map((tool) => tool.name),
);
assert.equal(
  readInventory.items.find((item) => item.name === "terminal")?.status,
  "not_requested",
);

const writeInventory = defaultToolRegistry.buildInventory({
  requestedToolsets: ["local_code_write"],
  profileDeniedTools: ["terminal"],
  resourceDeniedTools: ["patch"],
});

assert.equal(
  writeInventory.items.find((item) => item.name === "terminal")?.status,
  "profile_denied",
);
assert.equal(
  writeInventory.items.find((item) => item.name === "patch")?.status,
  "resource_denied",
);
assert.equal(
  writeInventory.items.find((item) => item.name === "write_file")?.status,
  "selected",
);

const aliasRegistry = createToolRegistry([
  {
    ...entry("read_file"),
    aliases: ["cat_file"],
  },
]);
const aliasInventory = aliasRegistry.buildInventory({
  requestedTools: ["cat_file", "missing_tool"],
});
assert.equal(
  aliasInventory.items.find((item) => item.name === "cat_file")?.status,
  "shadowed",
);
assert.equal(
  aliasInventory.items.find((item) => item.name === "read_file")?.status,
  "selected",
);
assert.equal(
  aliasInventory.items.find((item) => item.name === "missing_tool")?.status,
  "missing",
);

const invalid = validateToolRegistry([
  entry("read_file"),
  entry("read_file"),
  {
    ...entry("read_alias"),
    aliases: ["read_file"],
  },
  {
    ...entry("read_duplicate_shape"),
    outputShape: "shape.read_file.v1",
  },
  {
    ...entry("deprecated_without_replacement"),
    deprecated: {
      reason: "testing validation",
      since: "0.1.0",
    },
  },
]);

assert.equal(invalid.ok, false);
assert.deepEqual(
  invalid.issues
    .filter((issue) => issue.severity === "error")
    .map((issue) => issue.code)
    .sort(),
  [
    "alias_collides_with_name",
    "capability_collision",
    "deprecated_without_replacement",
    "duplicate_name",
  ],
);

console.log(
  JSON.stringify(
    {
      registeredTools: defaultToolRegistry.entries.length,
      readSelected: readInventory.selectedTools.map((tool) => tool.name),
      writeStatuses: Object.fromEntries(
        writeInventory.items.map((item) => [item.name, item.status]),
      ),
      validationErrors: invalid.issues.length,
    },
    null,
    2,
  ),
);

function entry(name: string): ToolRegistryEntry {
  return {
    name,
    description: `${name} description`,
    category: "local",
    toolsets: ["local_code_read"],
    implementationModule: `./tools.js#${name}`,
    surfaces: ["brain"],
    safety: ["read_only"],
    outputShape: `shape.${name}.v1`,
    version: "1.0.0",
    inventoryTest: "smoke:tool-registry",
  };
}
