import type { ToolDescriptor } from "@rusty-crew/contracts";
import { defaultToolRegistryMetadata } from "./tool-registry-portable-catalog.js";
export { defaultToolRegistryMetadata } from "./tool-registry-portable-catalog.js";

export type ToolCategory =
  | "local"
  | "git"
  | "patch"
  | "web"
  | "browser"
  | "memory"
  | "skills"
  | "mcp"
  | "delegation"
  | "coordination"
  | "planning"
  | "storage"
  | "diagnostics";

export type ToolSurface = "brain" | "mcp" | "admin" | "tui" | "diagnostic";

export type ToolSafetyFlag =
  | "read_only"
  | "writes_files"
  | "executes_process"
  | "workdir_scoped"
  | "network_access"
  | "external_write"
  | "coordination_action";

export type ToolInventoryStatus =
  | "selected"
  | "not_requested"
  | "profile_denied"
  | "session_denied"
  | "resource_denied"
  | "deprecated"
  | "missing"
  | "shadowed"
  | "collision";

export interface ToolDeprecation {
  reason: string;
  since: string;
  replacement?: string;
  sunset?: string;
}

export interface ToolRegistryMetadata {
  name: string;
  description: string;
  category: ToolCategory;
  toolsets: readonly string[];
  surfaces: readonly ToolSurface[];
  safety: readonly ToolSafetyFlag[];
  outputShape: string;
  version: string;
  aliases?: readonly string[];
  deprecated?: ToolDeprecation;
  replacement?: string;
  coexistenceNote?: string;
}

export type ToolRegistryEntry = ToolRegistryMetadata;

export interface ToolExecutableBinding {
  name: string;
  implementationModule: string;
  inventoryTest: string;
}

export interface ToolRegistryValidationIssue {
  severity: "error" | "warning";
  code:
    | "duplicate_name"
    | "alias_collides_with_name"
    | "duplicate_alias"
    | "capability_collision"
    | "implementation_shape_drift"
    | "deprecated_without_replacement"
    | "replacement_without_deprecation"
    | "bad_deprecation"
    | "invalid_replacement"
    | "deprecated_replacement_self_reference"
    | "missing_replacement_tool"
    | "invalid_name"
    | "invalid_alias"
    | "invalid_toolset"
    | "invalid_output_shape"
    | "invalid_version"
    | "missing_metadata"
    | "duplicate_metadata_value"
    | "missing_executable_binding"
    | "orphan_executable_binding"
    | "duplicate_executable_binding";
  toolName?: string;
  otherToolName?: string;
  message: string;
}

export interface ToolRegistryValidation {
  ok: boolean;
  issues: ToolRegistryValidationIssue[];
}

export interface ToolRegistryValidationOptions {
  requireExecutableBindings?: boolean;
}

export interface ToolInventoryRequest {
  requestedToolsets?: readonly string[];
  requestedTools?: readonly string[];
  profileDeniedTools?: readonly string[];
  sessionDeniedTools?: readonly string[];
  resourceDeniedTools?: readonly string[];
  profileDeniedReasons?: Record<string, string>;
  sessionDeniedReasons?: Record<string, string>;
  resourceDeniedReasons?: Record<string, string>;
  includeDeprecated?: boolean;
}

export interface ToolInventoryItem {
  name: string;
  canonicalName?: string;
  entry?: ToolRegistryEntry;
  binding?: ToolExecutableBinding;
  status: ToolInventoryStatus;
  reasons: string[];
}

export interface ToolInventory {
  selectedTools: ToolRegistryEntry[];
  selectedBindings: ToolExecutableBinding[];
  selectedDescriptors: ToolDescriptor[];
  items: ToolInventoryItem[];
}

export interface BuiltInToolCatalogToolset {
  id: string;
  label: string;
  description: string;
  category: ToolCategory | "mixed";
  toolCount: number;
  tools: string[];
}

export interface BuiltInToolCatalogTool {
  name: string;
  label: string;
  description: string;
  category: ToolCategory;
  toolsets: string[];
  surfaces: ToolSurface[];
  safety: ToolSafetyFlag[];
  outputShape: string;
  version: string;
  aliases: string[];
  deprecated?: ToolDeprecation;
  replacement?: string;
  coexistenceNote?: string;
}

export interface BuiltInToolCatalog {
  schemaVersion: 1;
  catalogId: "default-local-tools";
  toolsets: BuiltInToolCatalogToolset[];
  tools: BuiltInToolCatalogTool[];
}

export class ToolRegistry {
  readonly entries: readonly ToolRegistryEntry[];
  readonly bindings: ReadonlyMap<string, ToolExecutableBinding>;
  private readonly byName: Map<string, ToolRegistryEntry>;
  private readonly aliasToName: Map<string, string>;

  constructor(
    entries: readonly ToolRegistryEntry[],
    bindings: readonly ToolExecutableBinding[] = [],
  ) {
    const validation = validateToolRegistry(entries, bindings, {
      requireExecutableBindings: true,
    });
    if (!validation.ok) {
      throw new Error(formatValidationIssues(validation.issues));
    }
    this.entries = [...entries];
    this.bindings = new Map(bindings.map((binding) => [binding.name, binding]));
    this.byName = new Map(entries.map((entry) => [entry.name, entry]));
    this.aliasToName = new Map(
      entries.flatMap((entry) =>
        (entry.aliases ?? []).map((alias) => [alias, entry.name] as const),
      ),
    );
  }

  get(name: string): ToolRegistryEntry | undefined {
    return this.byName.get(name);
  }

  resolve(name: string): ToolRegistryEntry | undefined {
    return (
      this.byName.get(name) ?? this.byName.get(this.aliasToName.get(name) ?? "")
    );
  }

  bindingFor(name: string): ToolExecutableBinding | undefined {
    const canonicalName = this.canonicalName(name);
    return canonicalName ? this.bindings.get(canonicalName) : undefined;
  }

  canonicalName(name: string): string | undefined {
    if (this.byName.has(name)) {
      return name;
    }
    return this.aliasToName.get(name);
  }

  buildInventory(request: ToolInventoryRequest = {}): ToolInventory {
    return buildToolInventory(this, request);
  }
}

export function createToolRegistry(
  entries: readonly ToolRegistryEntry[],
  bindings: readonly ToolExecutableBinding[] = [],
): ToolRegistry {
  return new ToolRegistry(entries, bindings);
}

export const defaultToolExecutableBindings = [
  {
    name: "read_file",
    implementationModule: "./local-code-tools.js#readFileTool",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "write_file",
    implementationModule: "./local-code-tools.js#writeFileTool",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "search_files",
    implementationModule: "./local-code-tools.js#searchFilesTool",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "terminal",
    implementationModule: "./local-code-tools.js#terminalTool",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "git_status",
    implementationModule: "./local-code-tools.js#gitStatusTool",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "git_diff",
    implementationModule: "./local-code-tools.js#gitDiffTool",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "patch",
    implementationModule: "./patch-tool.js#patchTool",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "worker_write",
    implementationModule: "./local-code-tools.js#workerWriteTool",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "worker_patch",
    implementationModule: "./local-code-tools.js#workerPatchTool",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "spawn_subagent",
    implementationModule: "./delegation-tools.js#spawnSubagentTool",
    inventoryTest: "smoke:delegation-tools",
  },
  {
    name: "spawn_subagent_md",
    implementationModule: "./delegation-tools.js#spawnSubagentMarkdownTool",
    inventoryTest: "smoke:delegation-tools",
  },
  {
    name: "fan_out_subagents",
    implementationModule: "./delegation-tools.js#fanOutSubagentsTool",
    inventoryTest: "smoke:delegation-tools",
  },
  {
    name: "fan_out_subagents_md",
    implementationModule: "./delegation-tools.js#fanOutSubagentsMarkdownTool",
    inventoryTest: "smoke:delegation-tools",
  },
  {
    name: "scout_codebase",
    implementationModule: "./delegation-tools.js#scoutCodebaseTool",
    inventoryTest: "smoke:delegation-tools",
  },
  {
    name: "summarize_files",
    implementationModule: "./delegation-tools.js#summarizeFilesTool",
    inventoryTest: "smoke:delegation-tools",
  },
  {
    name: "find_relevant_paths",
    implementationModule: "./delegation-tools.js#findRelevantPathsTool",
    inventoryTest: "smoke:delegation-tools",
  },
  {
    name: "deliver_completion_md",
    implementationModule: "./completion-tools.js#deliverCompletionMarkdownTool",
    inventoryTest: "smoke:completion-tools",
  },
  {
    name: "list_agents",
    implementationModule: "./coordination-tools.js#listAgentsTool",
    inventoryTest: "smoke:coordination-tools",
  },
  {
    name: "send_agent_message",
    implementationModule: "./coordination-tools.js#sendAgentMessageTool",
    inventoryTest: "smoke:coordination-tools",
  },
  {
    name: "agent_round",
    implementationModule: "./coordination-tools.js#agentRoundTool",
    inventoryTest: "smoke:coordination-tools",
  },
  {
    name: "web_search",
    implementationModule: "./web-tools.js#webSearchTool",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "web_extract",
    implementationModule: "./web-tools.js#webExtractTool",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "browser_navigate",
    implementationModule: "./browser-tools.js#browserNavigateTool",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "browser_snapshot",
    implementationModule: "./browser-tools.js#browserSnapshotTool",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "browser_click",
    implementationModule: "./browser-tools.js#browserClickTool",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "browser_type",
    implementationModule: "./browser-tools.js#browserTypeTool",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "browser_scroll",
    implementationModule: "./browser-tools.js#browserScrollTool",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "browser_back",
    implementationModule: "./browser-tools.js#browserBackTool",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "browser_press",
    implementationModule: "./browser-tools.js#browserPressTool",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "browser_console",
    implementationModule: "./browser-tools.js#browserConsoleTool",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "browser_vision",
    implementationModule: "./browser-tools.js#browserVisionTool",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "memory_recall",
    implementationModule: "./den-memory-tools.js#denMemoryRecallTool",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "memory_read",
    implementationModule: "./den-memory-tools.js#denMemoryReadTool",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "memory_search",
    implementationModule: "./den-memory-tools.js#denMemorySearchTool",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "memory_store",
    implementationModule: "./den-memory-tools.js#denMemoryStoreTool",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "memory_propose",
    implementationModule: "./den-memory-tools.js#denMemoryProposeTool",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "dense_profile_memory",
    implementationModule:
      "./dense-profile-memory-tool.js#denseProfileMemoryTool",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "memory_space_catalog",
    implementationModule: "./memory-space-api.js#memorySpaceCatalogTool",
    inventoryTest: "smoke:memory-space-api",
  },
  {
    name: "memory_space_read",
    implementationModule: "./memory-space-api.js#memorySpaceReadTool",
    inventoryTest: "smoke:memory-space-api",
  },
  {
    name: "recall_lore",
    implementationModule: "./lore-memory-tool.js#recallLoreTool",
    inventoryTest: "smoke:lore-memory-tool",
  },
  {
    name: "capture_lore_fact",
    implementationModule: "./lore-memory-tool.js#captureLoreFactTool",
    inventoryTest: "smoke:lore-memory-tool",
  },
  {
    name: "promote_lore_entry",
    implementationModule: "./lore-memory-tool.js#promoteLoreEntryTool",
    inventoryTest: "smoke:lore-memory-tool",
  },
  {
    name: "search_lore",
    implementationModule: "./lore-memory-tool.js#searchLoreTool",
    inventoryTest: "smoke:lore-memory-tool",
  },
  {
    name: "list_lore_layers",
    implementationModule: "./lore-memory-tool.js#listLoreLayersTool",
    inventoryTest: "smoke:lore-memory-tool",
  },
  {
    name: "manage_lore_layers",
    implementationModule: "./lore-memory-tool.js#manageLoreLayersTool",
    inventoryTest: "smoke:lore-memory-tool",
  },
  {
    name: "get_lore_layer_config",
    implementationModule: "./lore-memory-tool.js#getLoreLayerConfigTool",
    inventoryTest: "smoke:lore-memory-tool",
  },
  {
    name: "get_scene_state",
    implementationModule: "./scene-state-tool.js#getSceneStateTool",
    inventoryTest: "smoke:scene-state-tool",
  },
  {
    name: "update_scene_state",
    implementationModule: "./scene-state-tool.js#updateSceneStateTool",
    inventoryTest: "smoke:scene-state-tool",
  },
  {
    name: "skills_list",
    implementationModule: "./skills-tools.js#skillsListTool",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "skill_view",
    implementationModule: "./skills-tools.js#skillViewTool",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "skill_manage",
    implementationModule: "./skills-tools.js#skillManageTool",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "todo",
    implementationModule: "./planning-tools.js#todoTool",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "session_search",
    implementationModule: "./planning-tools.js#sessionSearchTool",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "storage_query_catalog",
    implementationModule: "./storage-query-catalog.js#storageQueryCatalogTool",
    inventoryTest: "smoke:storage-query-catalog",
  },
  {
    name: "storage_query_execute",
    implementationModule: "./storage-query-catalog.js#storageQueryExecuteTool",
    inventoryTest: "smoke:storage-query-catalog",
  },
  {
    name: "channel_readback",
    implementationModule: "./planning-tools.js#channelReadbackTool",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "counter_reset",
    implementationModule: "./planning-tools.js#counterResetTool",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "curator_execute",
    implementationModule: "./planning-tools.js#curatorExecuteTool",
    inventoryTest: "smoke:tool-registry",
  },
] satisfies readonly ToolExecutableBinding[];

export const defaultToolRegistry = createToolRegistry(
  defaultToolRegistryMetadata,
  defaultToolExecutableBindings,
);

export function validateToolRegistry(
  entries: readonly ToolRegistryEntry[],
  bindings: readonly ToolExecutableBinding[] = [],
  options: ToolRegistryValidationOptions = {},
): ToolRegistryValidation {
  const issues: ToolRegistryValidationIssue[] = [];
  const byName = new Map<string, ToolRegistryEntry>();
  const aliasOwners = new Map<string, string>();
  const capabilityOwners = new Map<string, ToolRegistryEntry>();
  const bindingsByName = new Map<string, ToolExecutableBinding>();
  const implementationShapes = new Map<
    string,
    {
      binding: ToolExecutableBinding;
      entry: ToolRegistryEntry;
    }
  >();

  for (const entry of entries) {
    validateEntryMetadata(entry, issues);

    const existing = byName.get(entry.name);
    if (existing) {
      issues.push({
        severity: "error",
        code: "duplicate_name",
        toolName: entry.name,
        otherToolName: existing.name,
        message: `duplicate tool name ${entry.name}`,
      });
    } else {
      byName.set(entry.name, entry);
    }

    for (const alias of entry.aliases ?? []) {
      const aliasOwner = aliasOwners.get(alias);
      if (aliasOwner) {
        issues.push({
          severity: "error",
          code: "duplicate_alias",
          toolName: entry.name,
          otherToolName: aliasOwner,
          message: `alias ${alias} is used by multiple tools`,
        });
      } else {
        aliasOwners.set(alias, entry.name);
      }
    }
  }

  for (const binding of bindings) {
    const existing = bindingsByName.get(binding.name);
    if (existing) {
      issues.push({
        severity: "error",
        code: "duplicate_executable_binding",
        toolName: binding.name,
        otherToolName: existing.name,
        message: `duplicate executable binding for ${binding.name}`,
      });
    } else {
      bindingsByName.set(binding.name, binding);
    }
    if (!byName.has(binding.name)) {
      issues.push({
        severity: "error",
        code: "orphan_executable_binding",
        toolName: binding.name,
        message: `executable binding ${binding.name} has no portable metadata`,
      });
    }
  }

  if (options.requireExecutableBindings ?? bindings.length > 0) {
    for (const entry of entries) {
      if (!bindingsByName.has(entry.name)) {
        issues.push({
          severity: "error",
          code: "missing_executable_binding",
          toolName: entry.name,
          message: `tool metadata ${entry.name} has no executable binding`,
        });
      }
    }
  }

  for (const entry of entries) {
    for (const alias of entry.aliases ?? []) {
      const canonical = byName.get(alias);
      if (canonical && canonical.name !== entry.name) {
        issues.push({
          severity: "error",
          code: "alias_collides_with_name",
          toolName: entry.name,
          otherToolName: canonical.name,
          message: `alias ${alias} collides with canonical tool ${canonical.name}`,
        });
      }
    }

    if (
      entry.deprecated &&
      !entry.replacement &&
      !entry.deprecated.replacement
    ) {
      issues.push({
        severity: "error",
        code: "deprecated_without_replacement",
        toolName: entry.name,
        message: `deprecated tool ${entry.name} needs a replacement or sunset note`,
      });
    }

    const capabilityKey = `${entry.category}:${entry.outputShape}`;
    const capabilityOwner = capabilityOwners.get(capabilityKey);
    if (
      capabilityOwner &&
      capabilityOwner.name !== entry.name &&
      !entry.coexistenceNote &&
      !capabilityOwner.coexistenceNote
    ) {
      issues.push({
        severity: "error",
        code: "capability_collision",
        toolName: entry.name,
        otherToolName: capabilityOwner.name,
        message: `${entry.name} and ${capabilityOwner.name} both claim ${capabilityKey}`,
      });
    } else {
      capabilityOwners.set(capabilityKey, entry);
    }

    const binding = bindingsByName.get(entry.name);
    if (!binding) {
      continue;
    }
    const implementationOwner = implementationShapes.get(
      binding.implementationModule,
    );
    if (
      implementationOwner &&
      implementationOwner.entry.name !== entry.name &&
      implementationOwner.entry.outputShape !== entry.outputShape
    ) {
      issues.push({
        severity: "error",
        code: "implementation_shape_drift",
        toolName: entry.name,
        otherToolName: implementationOwner.entry.name,
        message: `${entry.name} and ${implementationOwner.entry.name} share implementation module with different output shapes`,
      });
    } else {
      implementationShapes.set(binding.implementationModule, {
        binding,
        entry,
      });
    }
  }

  return {
    ok: !issues.some((issue) => issue.severity === "error"),
    issues,
  };
}

export function assertValidToolRegistry(
  entries: readonly ToolRegistryEntry[],
  bindings: readonly ToolExecutableBinding[] = [],
): void {
  const validation = validateToolRegistry(entries, bindings, {
    requireExecutableBindings: bindings.length > 0,
  });
  if (!validation.ok) {
    throw new Error(formatValidationIssues(validation.issues));
  }
}

export function buildToolInventory(
  registry: ToolRegistry,
  request: ToolInventoryRequest = {},
): ToolInventory {
  const requestedToolsets = new Set(request.requestedToolsets ?? []);
  const requestedNames = new Set(request.requestedTools ?? []);
  const profileDenied = new Set(request.profileDeniedTools ?? []);
  const sessionDenied = new Set(request.sessionDeniedTools ?? []);
  const resourceDenied = new Set(request.resourceDeniedTools ?? []);
  const requestedCanonicalNames = new Set<string>();
  const aliasItems: ToolInventoryItem[] = [];
  const missingItems: ToolInventoryItem[] = [];

  for (const requestedName of requestedNames) {
    const canonicalName = registry.canonicalName(requestedName);
    if (!canonicalName) {
      missingItems.push({
        name: requestedName,
        status: "missing",
        reasons: [`requested tool ${requestedName} is not registered`],
      });
      continue;
    }
    requestedCanonicalNames.add(canonicalName);
    if (canonicalName !== requestedName) {
      aliasItems.push({
        name: requestedName,
        canonicalName,
        entry: registry.get(canonicalName),
        binding: registry.bindingFor(canonicalName),
        status: "shadowed",
        reasons: [
          `${requestedName} resolves to canonical tool ${canonicalName}`,
        ],
      });
    }
  }

  const items = registry.entries.map<ToolInventoryItem>((entry) => {
    const requestedByName = requestedCanonicalNames.has(entry.name);
    const requestedByToolset = entry.toolsets.some((toolset) =>
      requestedToolsets.has(toolset),
    );
    const requested = requestedByName || requestedByToolset;

    if (!requested) {
      return {
        name: entry.name,
        canonicalName: entry.name,
        entry,
        binding: registry.bindingFor(entry.name),
        status: "not_requested",
        reasons: ["not requested by profile toolsets or explicit tool names"],
      };
    }

    const denialStatus = firstDenialStatus(entry.name, {
      profileDenied,
      sessionDenied,
      resourceDenied,
    });
    if (denialStatus) {
      const reason = denialReason(entry.name, denialStatus, request);
      return {
        name: entry.name,
        canonicalName: entry.name,
        entry,
        binding: registry.bindingFor(entry.name),
        status: denialStatus,
        reasons: [reason],
      };
    }

    if (entry.deprecated && !request.includeDeprecated) {
      return {
        name: entry.name,
        canonicalName: entry.name,
        entry,
        binding: registry.bindingFor(entry.name),
        status: "deprecated",
        reasons: [
          (entry.replacement ?? entry.deprecated.replacement)
            ? `${entry.name} is deprecated; use ${
                entry.replacement ?? entry.deprecated.replacement
              }`
            : `${entry.name} is deprecated`,
        ],
      };
    }

    return {
      name: entry.name,
      canonicalName: entry.name,
      entry,
      binding: registry.bindingFor(entry.name),
      status: "selected",
      reasons: [
        requestedByName
          ? "requested explicitly by profile"
          : `requested through toolset ${entry.toolsets.find((toolset) =>
              requestedToolsets.has(toolset),
            )}`,
      ],
    };
  });

  const allItems = [...items, ...aliasItems, ...missingItems].sort(
    (left, right) => left.name.localeCompare(right.name),
  );
  const selectedTools = items
    .filter((item) => item.status === "selected" && item.entry)
    .map((item) => item.entry!);
  const selectedBindings = items
    .filter((item) => item.status === "selected" && item.binding)
    .map((item) => item.binding!);

  return {
    selectedTools,
    selectedBindings,
    selectedDescriptors: selectedTools.map(toToolDescriptor),
    items: allItems,
  };
}

function denialReason(
  name: string,
  status: Extract<
    ToolInventoryStatus,
    "profile_denied" | "session_denied" | "resource_denied"
  >,
  request: ToolInventoryRequest,
): string {
  const explicit =
    status === "profile_denied"
      ? request.profileDeniedReasons?.[name]
      : status === "session_denied"
        ? request.sessionDeniedReasons?.[name]
        : request.resourceDeniedReasons?.[name];
  return explicit ?? `${name} was denied by ${status.replace("_", " ")}`;
}

export function toToolDescriptor(entry: ToolRegistryEntry): ToolDescriptor {
  return {
    name: entry.name,
    description: entry.description,
  };
}

export function buildBuiltInToolCatalog(
  registry: ToolRegistry = defaultToolRegistry,
): BuiltInToolCatalog {
  const tools = registry.entries
    .filter((entry) => entry.surfaces.includes("brain"))
    .map<BuiltInToolCatalogTool>((entry) => ({
      name: entry.name,
      label: humanizeIdentifier(entry.name),
      description: entry.description,
      category: entry.category,
      toolsets: entry.toolsets.filter(
        (toolset) => !isDynamicMcpToolset(toolset),
      ),
      surfaces: [...entry.surfaces],
      safety: [...entry.safety],
      outputShape: entry.outputShape,
      version: entry.version,
      aliases: [...(entry.aliases ?? [])],
      deprecated: entry.deprecated,
      replacement: entry.replacement,
      coexistenceNote: entry.coexistenceNote,
    }))
    .filter((tool) => tool.toolsets.length > 0)
    .sort((left, right) => left.name.localeCompare(right.name));

  const toolsetsById = new Map<
    string,
    { categories: ToolCategory[]; tools: string[] }
  >();
  for (const tool of tools) {
    for (const toolset of tool.toolsets) {
      const aggregate = toolsetsById.get(toolset) ?? {
        categories: [],
        tools: [],
      };
      aggregate.categories.push(tool.category);
      aggregate.tools.push(tool.name);
      toolsetsById.set(toolset, aggregate);
    }
  }

  const toolsets = [...toolsetsById.entries()]
    .map<BuiltInToolCatalogToolset>(([id, aggregate]) => {
      const toolsForSet = [...new Set(aggregate.tools)].sort();
      const category = dominantCategory(aggregate.categories);
      return {
        id,
        label: humanizeIdentifier(id),
        description: toolsetDescription(id, category, toolsForSet.length),
        category,
        toolCount: toolsForSet.length,
        tools: toolsForSet,
      };
    })
    .sort((left, right) => left.id.localeCompare(right.id));

  return {
    schemaVersion: 1,
    catalogId: "default-local-tools",
    toolsets,
    tools,
  };
}

function isDynamicMcpToolset(toolset: string): boolean {
  return toolset.startsWith("mcp:");
}

function humanizeIdentifier(identifier: string): string {
  return identifier
    .split(/[_:-]+/g)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ");
}

function dominantCategory(
  categories: readonly ToolCategory[],
): ToolCategory | "mixed" {
  const unique = [...new Set(categories)];
  if (unique.length === 1) {
    return unique[0]!;
  }
  return "mixed";
}

function toolsetDescription(
  id: string,
  category: ToolCategory | "mixed",
  toolCount: number,
): string {
  const noun = toolCount === 1 ? "tool" : "tools";
  if (category === "mixed") {
    return `${humanizeIdentifier(id)} built-in tool policy set with ${toolCount} ${noun}.`;
  }
  return `${humanizeIdentifier(id)} built-in ${category} tool policy set with ${toolCount} ${noun}.`;
}

function validateEntryMetadata(
  entry: ToolRegistryEntry,
  issues: ToolRegistryValidationIssue[],
): void {
  if (!/^[a-z][a-z0-9_]*$/.test(entry.name)) {
    issues.push({
      severity: "error",
      code: "invalid_name",
      toolName: entry.name,
      message: `tool name ${entry.name} must be lower snake case`,
    });
  }
  const missingFields = [
    ["description", entry.description],
    ["category", entry.category],
    ["outputShape", entry.outputShape],
    ["version", entry.version],
  ].filter(([, value]) => typeof value !== "string" || value.trim() === "");
  if (
    missingFields.length > 0 ||
    entry.toolsets.length === 0 ||
    entry.surfaces.length === 0
  ) {
    issues.push({
      severity: "error",
      code: "missing_metadata",
      toolName: entry.name,
      message: `tool ${entry.name} is missing required metadata`,
    });
  }
}

function firstDenialStatus(
  name: string,
  sets: {
    profileDenied: ReadonlySet<string>;
    sessionDenied: ReadonlySet<string>;
    resourceDenied: ReadonlySet<string>;
  },
):
  | Extract<
      ToolInventoryStatus,
      "profile_denied" | "session_denied" | "resource_denied"
    >
  | undefined {
  if (sets.profileDenied.has(name)) {
    return "profile_denied";
  }
  if (sets.sessionDenied.has(name)) {
    return "session_denied";
  }
  if (sets.resourceDenied.has(name)) {
    return "resource_denied";
  }
  return undefined;
}

function formatValidationIssues(
  issues: readonly ToolRegistryValidationIssue[],
): string {
  return issues.map((issue) => issue.message).join("; ");
}
