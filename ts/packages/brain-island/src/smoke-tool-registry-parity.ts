import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import {
  buildToolRegistryMetadataArtifact,
  stableToolRegistryArtifactJson,
  type ToolRegistryMetadataArtifact,
} from "./tool-registry-artifact.js";
import {
  buildToolInventory,
  defaultToolExecutableBindings,
  defaultToolRegistry,
  defaultToolRegistryMetadata,
  validateToolRegistry,
} from "./tool-registry.js";

const artifactPath = fileURLToPath(
  new URL(
    "../../../../fixtures/tool-registry/default-tool-registry-metadata.json",
    import.meta.url,
  ),
);

const artifactText = await readFile(artifactPath, "utf8");
const artifact = JSON.parse(artifactText) as ToolRegistryMetadataArtifact;
const currentArtifact = buildToolRegistryMetadataArtifact({
  catalogId: "default-local-tools",
  metadata: defaultToolRegistryMetadata,
});

assert.equal(artifact.schemaVersion, 1);
assert.equal(artifact.catalogId, "default-local-tools");
assert.equal(
  artifactText,
  await stableToolRegistryArtifactJson(currentArtifact),
  "shared tool registry artifact drifted; run npm run generate:tool-registry-artifact",
);

const metadataNames = defaultToolRegistryMetadata.map((entry) => entry.name);
const bindingNames = defaultToolExecutableBindings.map(
  (binding) => binding.name,
);
assert.deepEqual(
  [...bindingNames].sort(),
  [...metadataNames].sort(),
  "every default metadata entry must have exactly one executable binding",
);

const validation = validateToolRegistry(
  defaultToolRegistryMetadata,
  defaultToolExecutableBindings,
  { requireExecutableBindings: true },
);
assert.equal(validation.ok, true, JSON.stringify(validation.issues, null, 2));

const missingBindingValidation = validateToolRegistry(
  defaultToolRegistryMetadata,
  defaultToolExecutableBindings.slice(1),
  { requireExecutableBindings: true },
);
assert.equal(
  missingBindingValidation.issues.some(
    (issue) => issue.code === "missing_executable_binding",
  ),
  true,
);

const orphanBindingValidation = validateToolRegistry(
  defaultToolRegistryMetadata,
  [
    ...defaultToolExecutableBindings,
    {
      name: "orphan_tool",
      implementationModule: "./tools.js#orphanTool",
      inventoryTest: "smoke:tool-registry-parity",
    },
  ],
  { requireExecutableBindings: true },
);
assert.equal(
  orphanBindingValidation.issues.some(
    (issue) => issue.code === "orphan_executable_binding",
  ),
  true,
);

const inventory = buildToolInventory(defaultToolRegistry, {
  requestedToolsets: ["local_code_read", "web_research", "browser"],
});
assert.deepEqual(
  inventory.selectedDescriptors,
  inventory.selectedTools.map((tool) => ({
    name: tool.name,
    description: tool.description,
  })),
);

console.log(
  JSON.stringify(
    {
      artifactTools: artifact.tools.length,
      metadataTools: metadataNames.length,
      bindingTools: bindingNames.length,
      selectedDescriptors: inventory.selectedDescriptors.length,
    },
    null,
    2,
  ),
);
