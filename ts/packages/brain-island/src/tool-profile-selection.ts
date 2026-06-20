import type {
  BrainImplementationId,
  BrainImplementationRegistration,
  BrainModelConfig,
  ExternalEventPayload,
  ProfileId,
  ToolProfile,
} from "@rusty-crew/contracts";
import {
  defaultToolRegistry,
  type ToolInventory,
  type ToolRegistry,
  type ToolSafetyFlag,
} from "./tool-registry.js";

export interface ProfileToolPolicy {
  requestedToolsets?: readonly string[];
  requestedTools?: readonly string[];
  deniedTools?: readonly string[];
  includeDeprecated?: boolean;
}

export interface SessionToolConstraints {
  deniedTools?: readonly string[];
  resourceDeniedTools?: readonly string[];
  readOnly?: boolean;
  disallowedSafetyFlags?: readonly ToolSafetyFlag[];
}

export interface ToolProfileSelectionInput {
  profileId: ProfileId;
  policy: ProfileToolPolicy;
  session?: SessionToolConstraints;
  registry?: ToolRegistry;
  catalogId?: string;
}

export interface ToolProfileSelection {
  profileId: ProfileId;
  catalogId: string;
  inventory: ToolInventory;
  toolProfile: ToolProfile;
}

export interface BrainRegistrationFromToolProfileInput extends ToolProfileSelectionInput {
  implementationId: BrainImplementationId;
  modelConfig: BrainModelConfig;
}

export function selectToolProfile(
  input: ToolProfileSelectionInput,
): ToolProfileSelection {
  const registry = input.registry ?? defaultToolRegistry;
  const resourceDeniedTools = new Set(input.session?.resourceDeniedTools ?? []);
  for (const entry of registry.entries) {
    if (input.session?.readOnly && !entry.safety.includes("read_only")) {
      resourceDeniedTools.add(entry.name);
    }
    if (
      input.session?.disallowedSafetyFlags?.some((flag) =>
        entry.safety.includes(flag),
      )
    ) {
      resourceDeniedTools.add(entry.name);
    }
  }

  const inventory = registry.buildInventory({
    requestedToolsets: input.policy.requestedToolsets,
    requestedTools: input.policy.requestedTools,
    profileDeniedTools: input.policy.deniedTools,
    sessionDeniedTools: input.session?.deniedTools,
    resourceDeniedTools: [...resourceDeniedTools],
    includeDeprecated: input.policy.includeDeprecated,
  });

  return {
    profileId: input.profileId,
    catalogId: input.catalogId ?? "default-local-tools",
    inventory,
    toolProfile: {
      tools: inventory.selectedDescriptors,
    },
  };
}

export function buildBrainRegistrationFromToolProfile(
  input: BrainRegistrationFromToolProfileInput,
): BrainImplementationRegistration {
  const selection = selectToolProfile(input);
  return {
    implementationId: input.implementationId,
    profileId: input.profileId,
    toolProfile: selection.toolProfile,
    modelConfig: input.modelConfig,
  };
}

export function createToolCatalogChangedPayload(
  catalogId: string,
): ExternalEventPayload {
  return {
    type: "tool_catalog_changed",
    catalogId,
  };
}
