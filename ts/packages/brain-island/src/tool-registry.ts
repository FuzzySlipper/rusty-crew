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
  | "planning"
  | "diagnostics";

export type ToolSurface = "brain" | "mcp" | "admin" | "tui" | "diagnostic";

export type ToolSafetyFlag =
  | "read_only"
  | "writes_files"
  | "executes_process"
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

export interface ToolRegistryEntry {
  name: string;
  description: string;
  category: ToolCategory;
  toolsets: readonly string[];
  implementationModule: string;
  surfaces: readonly ToolSurface[];
  safety: readonly ToolSafetyFlag[];
  outputShape: string;
  version: string;
  aliases?: readonly string[];
  deprecated?: ToolDeprecation;
  replacement?: string;
  inventoryTest: string;
  coexistenceNote?: string;
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
    | "missing_metadata";
  toolName?: string;
  otherToolName?: string;
  message: string;
}

export interface ToolRegistryValidation {
  ok: boolean;
  issues: ToolRegistryValidationIssue[];
}

export interface ToolInventoryRequest {
  requestedToolsets?: readonly string[];
  requestedTools?: readonly string[];
  profileDeniedTools?: readonly string[];
  sessionDeniedTools?: readonly string[];
  resourceDeniedTools?: readonly string[];
  includeDeprecated?: boolean;
}

export interface ToolInventoryItem {
  name: string;
  canonicalName?: string;
  entry?: ToolRegistryEntry;
  status: ToolInventoryStatus;
  reasons: string[];
}

export interface ToolInventory {
  selectedTools: ToolRegistryEntry[];
  selectedDescriptors: ToolDescriptor[];
  items: ToolInventoryItem[];
}

export class ToolRegistry {
  readonly entries: readonly ToolRegistryEntry[];
  private readonly byName: Map<string, ToolRegistryEntry>;
  private readonly aliasToName: Map<string, string>;

  constructor(entries: readonly ToolRegistryEntry[]) {
    const validation = validateToolRegistry(entries);
    if (!validation.ok) {
      throw new Error(formatValidationIssues(validation.issues));
    }
    this.entries = [...entries];
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
): ToolRegistry {
  return new ToolRegistry(entries);
}

export const defaultToolRegistry = createToolRegistry([
  {
    name: "read_file",
    description: "Read a UTF-8 text file from the session workdir.",
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
    description: "Write a bounded UTF-8 text file inside the session workdir.",
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
      "Search files beneath the session workdir and return path matches.",
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
    description: "Apply a bounded multi-file patch and report a unified diff.",
    category: "patch",
    toolsets: ["local_code_write"],
    implementationModule: "./patch-tool.js#patchTool",
    surfaces: ["brain"],
    safety: ["writes_files"],
    outputShape: "patch.apply_result.v1",
    version: "1.0.0",
    inventoryTest: "smoke:tool-registry",
  },
] satisfies readonly ToolRegistryEntry[]);

export function validateToolRegistry(
  entries: readonly ToolRegistryEntry[],
): ToolRegistryValidation {
  const issues: ToolRegistryValidationIssue[] = [];
  const byName = new Map<string, ToolRegistryEntry>();
  const aliasOwners = new Map<string, string>();
  const capabilityOwners = new Map<string, ToolRegistryEntry>();
  const implementationShapes = new Map<string, ToolRegistryEntry>();

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

    const implementationOwner = implementationShapes.get(
      entry.implementationModule,
    );
    if (
      implementationOwner &&
      implementationOwner.name !== entry.name &&
      implementationOwner.outputShape !== entry.outputShape
    ) {
      issues.push({
        severity: "error",
        code: "implementation_shape_drift",
        toolName: entry.name,
        otherToolName: implementationOwner.name,
        message: `${entry.name} and ${implementationOwner.name} share implementation module with different output shapes`,
      });
    } else {
      implementationShapes.set(entry.implementationModule, entry);
    }
  }

  return {
    ok: !issues.some((issue) => issue.severity === "error"),
    issues,
  };
}

export function assertValidToolRegistry(
  entries: readonly ToolRegistryEntry[],
): void {
  const validation = validateToolRegistry(entries);
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
      return {
        name: entry.name,
        canonicalName: entry.name,
        entry,
        status: denialStatus,
        reasons: [
          `${entry.name} was denied by ${denialStatus.replace("_", " ")}`,
        ],
      };
    }

    if (entry.deprecated && !request.includeDeprecated) {
      return {
        name: entry.name,
        canonicalName: entry.name,
        entry,
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

  return {
    selectedTools,
    selectedDescriptors: selectedTools.map(toToolDescriptor),
    items: allItems,
  };
}

export function toToolDescriptor(entry: ToolRegistryEntry): ToolDescriptor {
  return {
    name: entry.name,
    description: entry.description,
  };
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
    ["implementationModule", entry.implementationModule],
    ["outputShape", entry.outputShape],
    ["version", entry.version],
    ["inventoryTest", entry.inventoryTest],
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
