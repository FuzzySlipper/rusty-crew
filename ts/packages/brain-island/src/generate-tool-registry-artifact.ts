import { mkdir, writeFile } from "node:fs/promises";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";
import {
  buildToolRegistryMetadataArtifact,
  stableToolRegistryArtifactJson,
} from "./tool-registry-artifact.js";
import { defaultToolRegistryMetadata } from "./tool-registry.js";

const artifactPath = fileURLToPath(
  new URL(
    "../../../../fixtures/tool-registry/default-tool-registry-metadata.json",
    import.meta.url,
  ),
);

const artifact = buildToolRegistryMetadataArtifact({
  catalogId: "default-local-tools",
  metadata: defaultToolRegistryMetadata,
});

await mkdir(dirname(artifactPath), { recursive: true });
await writeFile(
  artifactPath,
  await stableToolRegistryArtifactJson(artifact),
  "utf8",
);

console.log(
  JSON.stringify(
    {
      artifactPath,
      catalogId: artifact.catalogId,
      tools: artifact.tools.length,
    },
    null,
    2,
  ),
);
