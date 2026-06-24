import type { ToolRegistryMetadata } from "./tool-registry.js";
import { format } from "prettier";

export interface ToolRegistryMetadataArtifact {
  schemaVersion: 1;
  catalogId: string;
  tools: ToolRegistryArtifactTool[];
}

export interface ToolRegistryArtifactTool {
  name: string;
  description: string;
  aliases?: readonly string[];
  category: string;
  toolsets: readonly string[];
  surfaces: readonly string[];
  safety: readonly string[];
  output_shape: string;
  version: string;
  deprecated?: {
    reason: string;
    since: string;
    replacement?: string;
    sunset?: string;
  };
  replacement?: string;
  coexistence_note?: string;
}

export function buildToolRegistryMetadataArtifact(input: {
  catalogId: string;
  metadata: readonly ToolRegistryMetadata[];
}): ToolRegistryMetadataArtifact {
  return {
    schemaVersion: 1,
    catalogId: input.catalogId,
    tools: input.metadata.map((entry) => ({
      name: entry.name,
      description: entry.description,
      aliases: optionalReadonlyArray(entry.aliases),
      category: entry.category,
      toolsets: entry.toolsets,
      surfaces: entry.surfaces,
      safety: entry.safety,
      output_shape: entry.outputShape,
      version: entry.version,
      deprecated: entry.deprecated
        ? {
            reason: entry.deprecated.reason,
            since: entry.deprecated.since,
            replacement: entry.deprecated.replacement,
            sunset: entry.deprecated.sunset,
          }
        : undefined,
      replacement: entry.replacement,
      coexistence_note: entry.coexistenceNote,
    })),
  };
}

export async function stableToolRegistryArtifactJson(
  artifact: ToolRegistryMetadataArtifact,
): Promise<string> {
  return format(JSON.stringify(removeUndefined(artifact)), {
    filepath: "default-tool-registry-metadata.json",
  });
}

function optionalReadonlyArray<T>(
  values: readonly T[] | undefined,
): readonly T[] | undefined {
  return values && values.length > 0 ? values : undefined;
}

function removeUndefined(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.map(removeUndefined);
  }
  if (value === null || typeof value !== "object") {
    return value;
  }
  return Object.fromEntries(
    Object.entries(value)
      .filter(([, nested]) => nested !== undefined)
      .map(([key, nested]) => [key, removeUndefined(nested)]),
  );
}
