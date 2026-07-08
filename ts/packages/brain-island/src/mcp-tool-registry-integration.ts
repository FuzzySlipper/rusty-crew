import type { ExternalEventPayload } from "@rusty-crew/contracts";
import type {
  NativeBridgeModule,
  NativeToolMetadataPolicyDiagnostic,
  NativeToolMetadataPolicyTool,
} from "@rusty-crew/native-bridge";
import type { McpRegistryCandidate } from "./service-adapter-ports.js";
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
  type ToolRegistryMetadata,
  type ToolRegistryValidation,
  type ToolRegistryValidationIssue,
} from "./tool-registry.js";
import { createToolCatalogChangedPayload } from "./tool-profile-selection.js";

export type McpNameCollisionPolicy = "fail" | "prefix_source";
export type PortableToolMetadataPolicyValidator = (
  entries: readonly ToolRegistryMetadata[],
) => Promise<ToolRegistryValidation> | ToolRegistryValidation;

export interface McpRegistryIntegrationInput {
  catalogId: string;
  candidates: readonly McpRegistryCandidate[];
  metadataPolicyValidator: PortableToolMetadataPolicyValidator;
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

export async function integrateMcpToolsWithRegistry(
  input: McpRegistryIntegrationInput,
): Promise<McpRegistryIntegrationReport> {
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
  const metadataValidation = await input.metadataPolicyValidator(entries);
  const bindingValidation = validateToolRegistry(entries, bindings, {
    requireExecutableBindings: true,
  });
  const validation = combineToolRegistryValidation(
    metadataValidation,
    bindingValidation,
  );
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

export function createBridgeToolMetadataPolicyValidator(
  bridge: Pick<NativeBridgeModule, "validateToolMetadataPolicy">,
): PortableToolMetadataPolicyValidator {
  return async (entries) => {
    const result = await bridge.validateToolMetadataPolicy({
      tools: entries.map(nativeToolMetadataPolicyTool),
    });
    return {
      ok: result.ok,
      issues: result.diagnostics.map(toolMetadataDiagnosticIssue),
    };
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

function nativeToolMetadataPolicyTool(
  entry: ToolRegistryMetadata,
): NativeToolMetadataPolicyTool {
  return {
    name: entry.name,
    description: entry.description,
    aliases: entry.aliases ? [...entry.aliases] : undefined,
    category: entry.category,
    toolsets: [...entry.toolsets],
    surfaces: [...entry.surfaces],
    safety: [...entry.safety],
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
  };
}

function toolMetadataDiagnosticIssue(
  diagnostic: NativeToolMetadataPolicyDiagnostic,
): ToolRegistryValidationIssue {
  return {
    severity: diagnostic.severity === "error" ? "error" : "warning",
    code: diagnostic.code as ToolRegistryValidationIssue["code"],
    toolName: diagnostic.tool_name,
    otherToolName: diagnostic.other_tool_name,
    message: diagnostic.message,
  };
}

function combineToolRegistryValidation(
  ...validations: readonly ToolRegistryValidation[]
): ToolRegistryValidation {
  const issues = validations.flatMap((validation) => validation.issues);
  return {
    ok: issues.every((issue) => issue.severity !== "error"),
    issues,
  };
}
