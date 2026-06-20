import type { McpRegistryCandidate } from "@rusty-crew/adapter-mcp";
import type { ExternalEventPayload } from "@rusty-crew/contracts";
import {
  buildToolInventory,
  createToolRegistry,
  defaultToolRegistry,
  validateToolRegistry,
  type ToolInventory,
  type ToolInventoryRequest,
  type ToolRegistry,
  type ToolRegistryEntry,
  type ToolRegistryValidation,
} from "./tool-registry.js";
import { createToolCatalogChangedPayload } from "./tool-profile-selection.js";

export type McpNameCollisionPolicy = "fail" | "prefix_source";

export interface McpRegistryIntegrationInput {
  catalogId: string;
  candidates: readonly McpRegistryCandidate[];
  baseEntries?: readonly ToolRegistryEntry[];
  inventoryRequest?: ToolInventoryRequest;
  nameCollisionPolicy?: McpNameCollisionPolicy;
  unavailableTools?: readonly string[];
}

export interface McpToolRegistryEntry extends ToolRegistryEntry {
  mcpSource: McpRegistryCandidate["source"];
  mcpAnnotations: McpRegistryCandidate["annotations"];
  mcpOutputSchema?: McpRegistryCandidate["outputSchema"];
}

export interface McpRegistryIntegrationReport {
  catalogId: string;
  entries: readonly ToolRegistryEntry[];
  mcpEntries: readonly McpToolRegistryEntry[];
  validation: ToolRegistryValidation;
  registry?: ToolRegistry;
  inventory?: ToolInventory;
  catalogChangedPayload: ExternalEventPayload;
  collisionPolicy: McpNameCollisionPolicy;
}

export function integrateMcpToolsWithRegistry(
  input: McpRegistryIntegrationInput,
): McpRegistryIntegrationReport {
  const baseEntries = input.baseEntries ?? defaultToolRegistry.entries;
  const policy = input.nameCollisionPolicy ?? "fail";
  const baseNames = new Set(baseEntries.map((entry) => entry.name));
  const unavailable = new Set(input.unavailableTools ?? []);
  const inventoryRequest = {
    ...input.inventoryRequest,
    resourceDeniedTools: [
      ...(input.inventoryRequest?.resourceDeniedTools ?? []),
      ...unavailable,
    ],
  } satisfies ToolInventoryRequest;
  const mcpEntries = input.candidates.map((candidate) =>
    mcpCandidateToRegistryEntry(candidate, {
      name:
        policy === "prefix_source" && baseNames.has(candidate.name)
          ? prefixedMcpToolName(candidate)
          : candidate.name,
    }),
  );
  const entries = [...baseEntries, ...mcpEntries];
  const validation = validateToolRegistry(entries);
  const registry = validation.ok ? createToolRegistry(entries) : undefined;
  const inventory = registry
    ? buildToolInventory(registry, inventoryRequest)
    : undefined;

  return {
    catalogId: input.catalogId,
    entries,
    mcpEntries,
    validation,
    registry,
    inventory,
    catalogChangedPayload: createToolCatalogChangedPayload(input.catalogId),
    collisionPolicy: policy,
  };
}

export function mcpCandidateToRegistryEntry(
  candidate: McpRegistryCandidate,
  options: { name?: string } = {},
): McpToolRegistryEntry {
  return {
    name: options.name ?? candidate.name,
    description: candidate.description,
    category: candidate.category,
    toolsets: candidate.toolsets,
    implementationModule: `${candidate.implementationModule}:${candidate.source.bindingId}:${candidate.source.sourceToolName}`,
    surfaces: candidate.surfaces,
    safety: candidate.safety,
    outputShape: candidate.outputShape,
    version: candidate.version,
    inventoryTest: candidate.inventoryTest,
    coexistenceNote: candidate.coexistenceNote,
    mcpSource: candidate.source,
    mcpAnnotations: candidate.annotations,
    mcpOutputSchema: candidate.outputSchema,
  };
}

function prefixedMcpToolName(candidate: McpRegistryCandidate): string {
  const source = candidate.source.serverNames.join("_");
  const prefix = source
    .replace(/[^A-Za-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "")
    .toLowerCase();
  return `${prefix || "mcp"}_${candidate.name}`;
}
