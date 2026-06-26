import assert from "node:assert/strict";
import {
  buildToolInventory,
  createToolRegistry,
  defaultToolRegistry,
  validateToolRegistry,
} from "./tool-registry.js";
import type {
  ToolExecutableBinding,
  ToolRegistryEntry,
} from "./tool-registry.js";

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
assert.deepEqual(
  readInventory.selectedBindings.map((binding) => binding.name),
  readInventory.selectedTools.map((tool) => tool.name),
);
assert.equal(
  Object.hasOwn(defaultToolRegistry.entries[0]!, "implementationModule"),
  false,
);
assert.equal(
  defaultToolRegistry.bindingFor("read_file")?.implementationModule,
  "./local-code-tools.js#readFileTool",
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

const memoryInventory = defaultToolRegistry.buildInventory({
  requestedToolsets: [
    "memory_den_read",
    "memory_den_write",
    "memory_profile",
    "skills_read",
    "planning_session",
  ],
  resourceDeniedTools: [
    "den_memory_store",
    "den_memory_propose",
    "dense_profile_memory",
    "session_search",
  ],
  resourceDeniedReasons: {
    den_memory_store: "Den Memories write endpoint is not configured",
    den_memory_propose: "Den Memories proposal endpoint is not configured",
    dense_profile_memory: "dense profile memory persistence is unavailable",
    session_search: "runtime search API is unavailable",
  },
});
assert.deepEqual(
  memoryInventory.selectedTools.map((tool) => tool.name),
  [
    "den_memory_recall",
    "den_memory_read",
    "den_memory_search",
    "memory_space_catalog",
    "memory_space_read",
    "skills_list",
    "skill_view",
    "todo",
  ],
);
assert.equal(
  memoryInventory.items.find((item) => item.name === "den_memory_store")
    ?.status,
  "resource_denied",
);
assert.equal(
  memoryInventory.items.find((item) => item.name === "den_memory_store")
    ?.reasons[0],
  "Den Memories write endpoint is not configured",
);
assert.equal(
  memoryInventory.items.find((item) => item.name === "counter_reset")?.status,
  "not_requested",
);

const webBrowserInventory = defaultToolRegistry.buildInventory({
  requestedToolsets: ["web_research", "browser", "browser_vision"],
  resourceDeniedTools: [
    "web_search",
    "web_extract",
    "browser_navigate",
    "browser_vision",
  ],
  resourceDeniedReasons: {
    web_search: "web search provider is not configured",
    web_extract: "web extraction network policy denied the URL",
    browser_navigate: "browser binary is not configured",
    browser_vision: "browser screenshot artifact store is not configured",
  },
});
assert.deepEqual(
  webBrowserInventory.selectedTools.map((tool) => tool.name),
  [
    "browser_snapshot",
    "browser_click",
    "browser_type",
    "browser_scroll",
    "browser_back",
    "browser_press",
    "browser_console",
  ],
);
assert.equal(
  webBrowserInventory.items.find((item) => item.name === "web_search")?.status,
  "resource_denied",
);
assert.equal(
  webBrowserInventory.items.find((item) => item.name === "web_search")
    ?.reasons[0],
  "web search provider is not configured",
);
assert.equal(
  webBrowserInventory.items.find((item) => item.name === "web_extract")
    ?.reasons[0],
  "web extraction network policy denied the URL",
);
assert.equal(
  webBrowserInventory.items.find((item) => item.name === "browser_navigate")
    ?.reasons[0],
  "browser binary is not configured",
);
assert.equal(
  webBrowserInventory.items.find((item) => item.name === "browser_vision")
    ?.reasons[0],
  "browser screenshot artifact store is not configured",
);

const aliasEntry = { ...entry("read_file"), aliases: ["cat_file"] };
const aliasRegistry = createToolRegistry([aliasEntry], [binding(aliasEntry)]);
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
const missingBinding = validateToolRegistry([entry("missing_binding")], [], {
  requireExecutableBindings: true,
});
const orphanBinding = validateToolRegistry(
  [],
  [binding(entry("orphan_tool"))],
  {
    requireExecutableBindings: true,
  },
);

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
assert.equal(
  missingBinding.issues.find(
    (issue) => issue.code === "missing_executable_binding",
  )?.toolName,
  "missing_binding",
);
assert.equal(
  orphanBinding.issues.find(
    (issue) => issue.code === "orphan_executable_binding",
  )?.toolName,
  "orphan_tool",
);

console.log(
  JSON.stringify(
    {
      registeredTools: defaultToolRegistry.entries.length,
      readSelected: readInventory.selectedTools.map((tool) => tool.name),
      memorySelected: memoryInventory.selectedTools.map((tool) => tool.name),
      webBrowserSelected: webBrowserInventory.selectedTools.map(
        (tool) => tool.name,
      ),
      writeStatuses: Object.fromEntries(
        writeInventory.items.map((item) => [item.name, item.status]),
      ),
      validationErrors: invalid.issues.length,
      selectedBindings: readInventory.selectedBindings.length,
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
    surfaces: ["brain"],
    safety: ["read_only"],
    outputShape: `shape.${name}.v1`,
    version: "1.0.0",
  };
}

function binding(entry: ToolRegistryEntry): ToolExecutableBinding {
  return {
    name: entry.name,
    implementationModule: `./tools.js#${entry.name}`,
    inventoryTest: "smoke:tool-registry",
  };
}
