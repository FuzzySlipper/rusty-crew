import type { ToolDescriptor } from "@rusty-crew/contracts";

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

interface ToolRegistryDefinition extends ToolRegistryMetadata {
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
    | "invalid_name"
    | "missing_metadata"
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

const defaultToolRegistryDefinitions = [
  {
    name: "read_file",
    description:
      "Read a UTF-8 text file. Relative paths resolve from the session workdir; absolute paths are allowed.",
    category: "local",
    toolsets: ["local_code_read", "review_readonly"],
    implementationModule: "./local-code-tools.js#readFileTool",
    surfaces: ["brain"],
    safety: ["read_only"],
    outputShape: "local.file_text.v1",
    version: "1.0.0",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "write_file",
    description:
      "Write a bounded UTF-8 text file. Relative paths resolve from the session workdir; absolute paths are allowed.",
    category: "local",
    toolsets: ["local_code_write"],
    implementationModule: "./local-code-tools.js#writeFileTool",
    surfaces: ["brain"],
    safety: ["writes_files"],
    outputShape: "local.file_write_result.v1",
    version: "1.0.0",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "search_files",
    description:
      "Search files beneath a root and return path matches. Relative roots resolve from the session workdir; absolute roots are allowed.",
    category: "local",
    toolsets: ["local_code_read", "review_readonly"],
    implementationModule: "./local-code-tools.js#searchFilesTool",
    surfaces: ["brain"],
    safety: ["read_only"],
    outputShape: "local.file_search_result.v1",
    version: "1.0.0",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "terminal",
    description: "Run a bounded shell command in the session workdir.",
    category: "local",
    toolsets: ["local_code_write"],
    implementationModule: "./local-code-tools.js#terminalTool",
    surfaces: ["brain"],
    safety: ["executes_process", "writes_files"],
    outputShape: "local.terminal_result.v1",
    version: "1.0.0",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "git_status",
    description:
      "Return concise git working tree status for the session workdir.",
    category: "git",
    toolsets: ["local_code_read", "review_readonly"],
    implementationModule: "./local-code-tools.js#gitStatusTool",
    surfaces: ["brain"],
    safety: ["read_only"],
    outputShape: "git.status_result.v1",
    version: "1.0.0",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "git_diff",
    description: "Return a git diff from the session workdir.",
    category: "git",
    toolsets: ["local_code_read", "review_readonly"],
    implementationModule: "./local-code-tools.js#gitDiffTool",
    surfaces: ["brain"],
    safety: ["read_only"],
    outputShape: "git.diff_result.v1",
    version: "1.0.0",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "patch",
    description:
      "Apply a bounded multi-file patch. Relative paths resolve from the session workdir; absolute paths are allowed.",
    category: "patch",
    toolsets: ["local_code_write"],
    implementationModule: "./patch-tool.js#patchTool",
    surfaces: ["brain"],
    safety: ["writes_files"],
    outputShape: "patch.apply_result.v1",
    version: "1.0.0",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "worker_write",
    description:
      "Write a bounded UTF-8 text file inside a delegated worker workdir.",
    category: "local",
    toolsets: ["worker_code_write"],
    implementationModule: "./local-code-tools.js#workerWriteTool",
    surfaces: ["brain"],
    safety: ["writes_files", "workdir_scoped"],
    outputShape: "local.worker_file_write_result.v1",
    version: "1.0.0",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "worker_patch",
    description:
      "Apply bounded patches inside a delegated worker workdir and report a unified diff.",
    category: "patch",
    toolsets: ["worker_code_write"],
    implementationModule: "./local-code-tools.js#workerPatchTool",
    surfaces: ["brain"],
    safety: ["writes_files", "workdir_scoped"],
    outputShape: "patch.worker_apply_result.v1",
    version: "1.0.0",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "spawn_subagent",
    description:
      "Queue one Rust-owned delegated subagent request through the brain action contract.",
    category: "delegation",
    toolsets: ["delegation_basic"],
    implementationModule: "./delegation-tools.js#spawnSubagentTool",
    surfaces: ["brain"],
    safety: ["coordination_action"],
    outputShape: "delegation.spawn_subagent_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:delegation-tools",
  },
  {
    name: "fan_out_subagents",
    description:
      "Queue a bounded Rust-owned fan-out group of delegated subagent requests.",
    category: "delegation",
    toolsets: ["delegation_basic"],
    implementationModule: "./delegation-tools.js#fanOutSubagentsTool",
    surfaces: ["brain"],
    safety: ["coordination_action"],
    outputShape: "delegation.fan_out_request_actions.v1",
    version: "0.1.0",
    inventoryTest: "smoke:delegation-tools",
  },
  {
    name: "scout_codebase",
    description:
      "Delegate a read-only codebase scouting task and request concise evidence back.",
    category: "delegation",
    toolsets: ["delegation_basic"],
    implementationModule: "./delegation-tools.js#scoutCodebaseTool",
    surfaces: ["brain"],
    safety: ["read_only", "coordination_action"],
    outputShape: "delegation.scout_codebase_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:delegation-tools",
  },
  {
    name: "summarize_files",
    description:
      "Delegate read-only file summarization for bounded context gathering.",
    category: "delegation",
    toolsets: ["delegation_basic"],
    implementationModule: "./delegation-tools.js#summarizeFilesTool",
    surfaces: ["brain"],
    safety: ["read_only", "coordination_action"],
    outputShape: "delegation.summarize_files_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:delegation-tools",
  },
  {
    name: "find_relevant_paths",
    description:
      "Delegate read-only codebase search for paths relevant to a task or question.",
    category: "delegation",
    toolsets: ["delegation_basic"],
    implementationModule: "./delegation-tools.js#findRelevantPathsTool",
    surfaces: ["brain"],
    safety: ["read_only", "coordination_action"],
    outputShape: "delegation.find_relevant_paths_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:delegation-tools",
  },
  {
    name: "send_agent_message",
    description:
      "Route a Rusty Crew internal message to another agent and request a wake.",
    category: "coordination",
    toolsets: ["agent_coordination", "full_agent"],
    implementationModule: "./coordination-tools.js#sendAgentMessageTool",
    surfaces: ["brain"],
    safety: ["coordination_action"],
    outputShape: "coordination.send_agent_message_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:coordination-tools",
  },
  {
    name: "agent_round",
    description:
      "Send a Rusty Crew internal message to another agent, wake it, and wait for one correlated reply.",
    category: "coordination",
    toolsets: ["agent_coordination", "full_agent"],
    implementationModule: "./coordination-tools.js#agentRoundTool",
    surfaces: ["brain"],
    safety: ["coordination_action"],
    outputShape: "coordination.agent_round_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:coordination-tools",
  },
  {
    name: "web_search",
    description:
      "Search the public web through the configured provider and return bounded results.",
    category: "web",
    toolsets: ["web_research"],
    implementationModule: "./web-tools.js#webSearchTool",
    surfaces: ["brain"],
    safety: ["read_only", "network_access"],
    outputShape: "web.search_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "web_extract",
    description:
      "Fetch and extract bounded text from a public HTTP(S) URL with SSRF guardrails.",
    category: "web",
    toolsets: ["web_research"],
    implementationModule: "./web-tools.js#webExtractTool",
    surfaces: ["brain"],
    safety: ["read_only", "network_access"],
    outputShape: "web.extract_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "browser_navigate",
    description:
      "Navigate the session-scoped browser to an allowed public HTTP(S) page.",
    category: "browser",
    toolsets: ["browser"],
    implementationModule: "./browser-tools.js#browserNavigateTool",
    surfaces: ["brain"],
    safety: ["network_access"],
    outputShape: "browser.navigation_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "browser_snapshot",
    description:
      "Return a bounded accessibility snapshot for the session-scoped browser page.",
    category: "browser",
    toolsets: ["browser"],
    implementationModule: "./browser-tools.js#browserSnapshotTool",
    surfaces: ["brain"],
    safety: ["read_only"],
    outputShape: "browser.snapshot_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "browser_click",
    description:
      "Click a ref from the current session-scoped browser snapshot.",
    category: "browser",
    toolsets: ["browser"],
    implementationModule: "./browser-tools.js#browserClickTool",
    surfaces: ["brain"],
    safety: ["external_write"],
    outputShape: "browser.click_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "browser_type",
    description:
      "Type bounded text into a ref from the current session-scoped browser snapshot.",
    category: "browser",
    toolsets: ["browser"],
    implementationModule: "./browser-tools.js#browserTypeTool",
    surfaces: ["brain"],
    safety: ["external_write"],
    outputShape: "browser.type_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "browser_scroll",
    description:
      "Scroll the current session-scoped browser page or a snapshot ref.",
    category: "browser",
    toolsets: ["browser"],
    implementationModule: "./browser-tools.js#browserScrollTool",
    surfaces: ["brain"],
    safety: ["read_only"],
    outputShape: "browser.scroll_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "browser_back",
    description:
      "Navigate back within the current session-scoped browser history.",
    category: "browser",
    toolsets: ["browser"],
    implementationModule: "./browser-tools.js#browserBackTool",
    surfaces: ["brain"],
    safety: ["read_only"],
    outputShape: "browser.back_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "browser_press",
    description:
      "Send a bounded keyboard press to the current session-scoped browser page.",
    category: "browser",
    toolsets: ["browser"],
    implementationModule: "./browser-tools.js#browserPressTool",
    surfaces: ["brain"],
    safety: ["external_write"],
    outputShape: "browser.press_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "browser_console",
    description:
      "Read the bounded console log ring for the current session-scoped browser page.",
    category: "browser",
    toolsets: ["browser"],
    implementationModule: "./browser-tools.js#browserConsoleTool",
    surfaces: ["brain"],
    safety: ["read_only"],
    outputShape: "browser.console_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "browser_vision",
    description:
      "Capture a screenshot artifact reference for the current session-scoped browser page.",
    category: "browser",
    toolsets: ["browser_vision"],
    implementationModule: "./browser-tools.js#browserVisionTool",
    surfaces: ["brain"],
    safety: ["read_only"],
    outputShape: "browser.vision_capture_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "den_memory_recall",
    description:
      "Recall relevant Den-owned memory summaries for the current profile or work context.",
    category: "memory",
    toolsets: ["memory_den_read"],
    implementationModule: "./den-memory-tools.js#denMemoryRecallTool",
    surfaces: ["brain"],
    safety: ["read_only", "network_access"],
    outputShape: "den.memory_recall_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "den_memory_read",
    description: "Read a specific Den-owned memory entry by stable reference.",
    category: "memory",
    toolsets: ["memory_den_read"],
    implementationModule: "./den-memory-tools.js#denMemoryReadTool",
    surfaces: ["brain"],
    safety: ["read_only", "network_access"],
    outputShape: "den.memory_read_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "den_memory_search",
    description:
      "Search Den-owned memories through the configured Den Memories service.",
    category: "memory",
    toolsets: ["memory_den_read"],
    implementationModule: "./den-memory-tools.js#denMemorySearchTool",
    surfaces: ["brain"],
    safety: ["read_only", "network_access"],
    outputShape: "den.memory_search_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "den_memory_store",
    description:
      "Store a new Den-owned memory through the configured Den Memories service.",
    category: "memory",
    toolsets: ["memory_den_write"],
    implementationModule: "./den-memory-tools.js#denMemoryStoreTool",
    surfaces: ["brain"],
    safety: ["network_access", "external_write"],
    outputShape: "den.memory_store_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "den_memory_propose",
    description:
      "Propose a Den-owned memory change for later review or acceptance.",
    category: "memory",
    toolsets: ["memory_den_write"],
    implementationModule: "./den-memory-tools.js#denMemoryProposeTool",
    surfaces: ["brain"],
    safety: ["network_access", "external_write"],
    outputShape: "den.memory_propose_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "dense_profile_memory",
    description:
      "Read or update Rusty Crew dense profile memory through runtime-owned APIs.",
    category: "memory",
    toolsets: ["memory_profile"],
    implementationModule:
      "./dense-profile-memory-tool.js#denseProfileMemoryTool",
    surfaces: ["brain"],
    safety: ["coordination_action"],
    outputShape: "runtime.dense_profile_memory_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "memory_space_catalog",
    description:
      "List Rusty Crew runtime-owned memory spaces without reading Den memory.",
    category: "memory",
    toolsets: ["memory_profile", "memory_space_read"],
    implementationModule: "./memory-space-api.js#memorySpaceCatalogTool",
    surfaces: ["brain", "admin"],
    safety: ["read_only"],
    outputShape: "runtime.memory_space_catalog.v1",
    version: "0.1.0",
    inventoryTest: "smoke:memory-space-api",
  },
  {
    name: "memory_space_read",
    description:
      "Read supported Rusty Crew memory-space records through bounded read-only runtime APIs.",
    category: "memory",
    toolsets: ["memory_profile", "memory_space_read"],
    implementationModule: "./memory-space-api.js#memorySpaceReadTool",
    surfaces: ["brain", "admin"],
    safety: ["read_only"],
    outputShape: "runtime.memory_space_read_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:memory-space-api",
  },
  {
    name: "recall_lore",
    description:
      "Recall scored roleplay lore from active chat layers through runtime-owned APIs.",
    category: "memory",
    toolsets: ["roleplay_lore_read"],
    implementationModule: "./lore-memory-tool.js#recallLoreTool",
    surfaces: ["brain"],
    safety: ["read_only", "coordination_action"],
    outputShape: "roleplay.lore_recall_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:lore-memory-tool",
  },
  {
    name: "capture_lore_fact",
    description:
      "Capture a roleplay fact into an automatic lore layer through runtime-owned APIs.",
    category: "memory",
    toolsets: ["roleplay_lore_write"],
    implementationModule: "./lore-memory-tool.js#captureLoreFactTool",
    surfaces: ["brain"],
    safety: ["coordination_action"],
    outputShape: "roleplay.lore_capture_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:lore-memory-tool",
  },
  {
    name: "promote_lore_entry",
    description:
      "Promote a captured roleplay lore entry into another lore layer.",
    category: "memory",
    toolsets: ["roleplay_lore_write"],
    implementationModule: "./lore-memory-tool.js#promoteLoreEntryTool",
    surfaces: ["brain"],
    safety: ["coordination_action"],
    outputShape: "roleplay.lore_promotion_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:lore-memory-tool",
  },
  {
    name: "search_lore",
    description:
      "Search roleplay lore entries and optionally narrow results to chat or explicit layers.",
    category: "memory",
    toolsets: ["roleplay_lore_read"],
    implementationModule: "./lore-memory-tool.js#searchLoreTool",
    surfaces: ["brain"],
    safety: ["read_only", "coordination_action"],
    outputShape: "roleplay.lore_search_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:lore-memory-tool",
  },
  {
    name: "list_lore_layers",
    description: "List roleplay lore layers available to a profile.",
    category: "memory",
    toolsets: ["roleplay_lore_read"],
    implementationModule: "./lore-memory-tool.js#listLoreLayersTool",
    surfaces: ["brain"],
    safety: ["read_only", "coordination_action"],
    outputShape: "roleplay.lore_layer_list_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:lore-memory-tool",
  },
  {
    name: "manage_lore_layers",
    description:
      "Create, update, archive, toggle, and reorder roleplay lore layers.",
    category: "memory",
    toolsets: ["roleplay_lore_manage"],
    implementationModule: "./lore-memory-tool.js#manageLoreLayersTool",
    surfaces: ["brain"],
    safety: ["coordination_action"],
    outputShape: "roleplay.lore_layer_management_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:lore-memory-tool",
  },
  {
    name: "get_lore_layer_config",
    description: "Read retrieval and capture config for a roleplay lore layer.",
    category: "memory",
    toolsets: ["roleplay_lore_read"],
    implementationModule: "./lore-memory-tool.js#getLoreLayerConfigTool",
    surfaces: ["brain"],
    safety: ["read_only", "coordination_action"],
    outputShape: "roleplay.lore_layer_config_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:lore-memory-tool",
  },
  {
    name: "skills_list",
    description: "List configured skills visible to the current profile.",
    category: "skills",
    toolsets: ["skills_read"],
    implementationModule: "./skills-tools.js#skillsListTool",
    surfaces: ["brain"],
    safety: ["read_only"],
    outputShape: "skills.list_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "skill_view",
    description:
      "View one configured skill by slug without exposing unrelated files.",
    category: "skills",
    toolsets: ["skills_read"],
    implementationModule: "./skills-tools.js#skillViewTool",
    surfaces: ["brain"],
    safety: ["read_only"],
    outputShape: "skills.view_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "skill_manage",
    description:
      "Create, patch, replace, or retire configured skills with governance safeguards.",
    category: "skills",
    toolsets: ["skills_manage"],
    implementationModule: "./skills-tools.js#skillManageTool",
    surfaces: ["brain"],
    safety: ["writes_files", "coordination_action"],
    outputShape: "skills.manage_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "todo",
    description:
      "Read or update bounded session-local planning todos without changing Den tasks.",
    category: "planning",
    toolsets: ["planning_session"],
    implementationModule: "./planning-tools.js#todoTool",
    surfaces: ["brain"],
    safety: ["coordination_action"],
    outputShape: "planning.todo_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "session_search",
    description: "Search Rusty Crew runtime history through typed search APIs.",
    category: "planning",
    toolsets: ["planning_session", "runtime_search"],
    implementationModule: "./planning-tools.js#sessionSearchTool",
    surfaces: ["brain"],
    safety: ["read_only", "coordination_action"],
    outputShape: "runtime.session_search_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "storage_query_catalog",
    description:
      "List curated read-only Rusty Crew storage queries without exposing raw SQL.",
    category: "storage",
    toolsets: ["storage_read", "diagnostics_read"],
    implementationModule: "./storage-query-catalog.js#storageQueryCatalogTool",
    surfaces: ["brain", "admin"],
    safety: ["read_only"],
    outputShape: "storage.query_catalog.v1",
    version: "0.1.0",
    inventoryTest: "smoke:storage-query-catalog",
  },
  {
    name: "storage_query_execute",
    description:
      "Execute one curated read-only Rusty Crew storage query by id; raw SQL is not supported.",
    category: "storage",
    toolsets: ["storage_read", "diagnostics_read"],
    implementationModule: "./storage-query-catalog.js#storageQueryExecuteTool",
    surfaces: ["brain", "admin"],
    safety: ["read_only"],
    outputShape: "storage.query_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:storage-query-catalog",
  },
  {
    name: "channel_readback",
    description:
      "Inspect bounded channel context for the current runtime binding without replaying messages.",
    category: "planning",
    toolsets: ["channels"],
    implementationModule: "./planning-tools.js#channelReadbackTool",
    surfaces: ["brain"],
    safety: ["read_only", "coordination_action"],
    outputShape: "channel.readback_response.v1",
    version: "0.1.0",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "counter_reset",
    description:
      "Reset or rebuild derived runtime counters without deleting runtime facts.",
    category: "planning",
    toolsets: ["planning_privileged", "runtime_counters"],
    implementationModule: "./planning-tools.js#counterResetTool",
    surfaces: ["brain", "admin"],
    safety: ["coordination_action"],
    outputShape: "runtime.counter_reset_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:tool-registry",
  },
  {
    name: "curator_execute",
    description:
      "Request a narrow audited curator/governance action through control APIs.",
    category: "planning",
    toolsets: ["planning_privileged", "curator_governance"],
    implementationModule: "./planning-tools.js#curatorExecuteTool",
    surfaces: ["brain", "admin"],
    safety: ["coordination_action", "external_write"],
    outputShape: "governance.curator_execute_result.v1",
    version: "0.1.0",
    inventoryTest: "smoke:tool-registry",
  },
] satisfies readonly ToolRegistryDefinition[];

export const defaultToolRegistryMetadata =
  defaultToolRegistryDefinitions.map(portableToolMetadata);

export const defaultToolExecutableBindings = defaultToolRegistryDefinitions.map(
  executableToolBinding,
);

export const defaultToolRegistry = createToolRegistry(
  defaultToolRegistryMetadata,
  defaultToolExecutableBindings,
);

function portableToolMetadata(
  definition: ToolRegistryDefinition,
): ToolRegistryMetadata {
  return {
    name: definition.name,
    description: definition.description,
    category: definition.category,
    toolsets: definition.toolsets,
    surfaces: definition.surfaces,
    safety: definition.safety,
    outputShape: definition.outputShape,
    version: definition.version,
    aliases: definition.aliases,
    deprecated: definition.deprecated,
    replacement: definition.replacement,
    coexistenceNote: definition.coexistenceNote,
  };
}

function executableToolBinding(
  definition: ToolRegistryDefinition,
): ToolExecutableBinding {
  return {
    name: definition.name,
    implementationModule: definition.implementationModule,
    inventoryTest: definition.inventoryTest,
  };
}

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
