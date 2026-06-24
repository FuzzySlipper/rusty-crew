import type { McpRegistryCandidate } from "@rusty-crew/adapter-mcp";
import type { ExternalEventPayload } from "@rusty-crew/contracts";
import {
  buildToolInventory,
  createToolRegistry,
  defaultToolRegistry,
  validateToolRegistry,
  type ToolExecutableBinding,
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
  baseBindings?: readonly ToolExecutableBinding[];
  inventoryRequest?: ToolInventoryRequest;
  nameCollisionPolicy?: McpNameCollisionPolicy;
  unavailableTools?: readonly string[];
}

export type McpToolRegistryEntry = ToolRegistryEntry;

export interface McpToolExecutableBinding extends ToolExecutableBinding {
  mcpSource: McpRegistryCandidate["source"];
  mcpAnnotations: McpRegistryCandidate["annotations"];
  mcpOutputSchema?: McpRegistryCandidate["outputSchema"];
}

export interface McpRegistryIntegrationReport {
  catalogId: string;
  entries: readonly ToolRegistryEntry[];
  bindings: readonly ToolExecutableBinding[];
  mcpEntries: readonly McpToolRegistryEntry[];
  mcpBindings: readonly McpToolExecutableBinding[];
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
  const baseBindings = input.baseBindings ?? [
    ...defaultToolRegistry.bindings.values(),
  ];
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
  const mcpBindings = input.candidates.map((candidate, index) =>
    mcpCandidateToExecutableBinding(candidate, mcpEntries[index]!.name),
  );
  const entries = [...baseEntries, ...mcpEntries];
  const bindings = [...baseBindings, ...mcpBindings];
  const validation = validateToolRegistry(entries, bindings, {
    requireExecutableBindings: true,
  });
  const registry = validation.ok
    ? createToolRegistry(entries, bindings)
    : undefined;
  const inventory = registry
    ? buildToolInventory(registry, inventoryRequest)
    : undefined;

  return {
    catalogId: input.catalogId,
    entries,
    bindings,
    mcpEntries,
    mcpBindings,
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
    surfaces: candidate.surfaces,
    safety: candidate.safety,
    outputShape: candidate.outputShape,
    version: candidate.version,
    coexistenceNote: candidate.coexistenceNote,
  };
}

export function mcpCandidateToExecutableBinding(
  candidate: McpRegistryCandidate,
  name: string = candidate.name,
): McpToolExecutableBinding {
  return {
    name,
    implementationModule: `${candidate.implementationModule}:${candidate.source.bindingId}:${candidate.source.sourceToolName}`,
    inventoryTest: candidate.inventoryTest,
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
