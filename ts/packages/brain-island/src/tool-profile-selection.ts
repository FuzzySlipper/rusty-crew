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
  resourceDeniedReasons?: Record<string, string>;
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

const DELEGATION_DEPTH_EXHAUSTED_REASON =
  "delegation_depth_exhausted: session max delegation depth is zero";

export function resourceDeniedToolsForLimits(
  limits: { maxDelegationDepth?: number | null } | undefined,
  registry: ToolRegistry = defaultToolRegistry,
): ReadonlyMap<string, string> {
  if (limits?.maxDelegationDepth !== 0) {
    return new Map();
  }
  return new Map(
    registry.entries
      .filter((entry) => entry.category === "delegation")
      .map((entry) => [entry.name, DELEGATION_DEPTH_EXHAUSTED_REASON]),
  );
}

export function effectiveToolSelectionForResourceLimits(
  selection: ToolProfileSelection,
  limits: { maxDelegationDepth?: number | null } | undefined,
  registry: ToolRegistry = defaultToolRegistry,
): ToolProfileSelection {
  const denied = resourceDeniedToolsForLimits(limits, registry);
  if (denied.size === 0) {
    return selection;
  }
  const selectedTools = selection.inventory.selectedTools.filter(
    (entry) => !denied.has(entry.name),
  );
  const selectedNames = new Set(selectedTools.map((entry) => entry.name));
  return {
    ...selection,
    inventory: {
      selectedTools,
      selectedBindings: selection.inventory.selectedBindings.filter((binding) =>
        selectedNames.has(binding.name),
      ),
      selectedDescriptors: selection.inventory.selectedDescriptors.filter(
        (descriptor) => selectedNames.has(descriptor.name),
      ),
      items: selection.inventory.items.map((item) => {
        const reason = denied.get(item.canonicalName ?? item.name);
        if (item.status !== "selected" || reason === undefined) {
          return item;
        }
        return {
          ...item,
          status: "resource_denied" as const,
          reasons: [reason],
        };
      }),
    },
    toolProfile: {
      tools: selection.toolProfile.tools.filter((tool) =>
        selectedNames.has(tool.name),
      ),
    },
  };
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
    resourceDeniedReasons: input.session?.resourceDeniedReasons,
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
