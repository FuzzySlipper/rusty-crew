import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import type {
  ToolRegistryArtifactTool,
  ToolRegistryMetadataArtifact,
} from "./tool-registry-artifact.js";
import type { ToolRegistryMetadata } from "./tool-registry.js";

const defaultToolRegistryArtifactPath = fileURLToPath(
  new URL(
    "../../../../fixtures/tool-registry/default-tool-registry-metadata.json",
    import.meta.url,
  ),
);

export const defaultToolRegistryMetadata = loadPortableToolCatalog(
  defaultToolRegistryArtifactPath,
);

export function loadPortableToolCatalog(
  artifactPath: string,
): readonly ToolRegistryMetadata[] {
  const artifact = JSON.parse(
    readFileSync(artifactPath, "utf8"),
  ) as ToolRegistryMetadataArtifact;
  assertPortableArtifactEnvelope(artifact, artifactPath);
  return artifact.tools.map(portableToolMetadataFromArtifact);
}

function portableToolMetadataFromArtifact(
  tool: ToolRegistryArtifactTool,
): ToolRegistryMetadata {
  return {
    name: tool.name,
    description: tool.description,
    category: tool.category as ToolRegistryMetadata["category"],
    toolsets: [...tool.toolsets],
    surfaces: [...tool.surfaces] as ToolRegistryMetadata["surfaces"],
    safety: [...tool.safety] as ToolRegistryMetadata["safety"],
    outputShape: tool.output_shape,
    version: tool.version,
    aliases: tool.aliases ? [...tool.aliases] : undefined,
    deprecated: tool.deprecated,
    replacement: tool.replacement,
    coexistenceNote: tool.coexistence_note,
  };
}

function assertPortableArtifactEnvelope(
  artifact: ToolRegistryMetadataArtifact,
  artifactPath: string,
): void {
  if (artifact.schemaVersion !== 1) {
    throw new Error(
      `unsupported portable tool catalog schema in ${artifactPath}: ${artifact.schemaVersion}`,
    );
  }
  if (artifact.catalogId !== "default-local-tools") {
    throw new Error(
      `unexpected portable tool catalog id in ${artifactPath}: ${artifact.catalogId}`,
    );
  }
  if (!Array.isArray(artifact.tools)) {
    throw new Error(`portable tool catalog ${artifactPath} has no tools array`);
  }
}
