import {
  buildToolInventory,
  createToolRegistry,
  defaultToolRegistry,
  validateToolRegistry,
  type ToolInventory,
  type ToolInventoryItem,
  type ToolInventoryRequest,
  type ToolInventoryStatus,
  type ToolExecutableBinding,
  type ToolRegistry,
  type ToolRegistryEntry,
  type ToolRegistryValidation,
} from "./tool-registry.js";

export interface ToolRegistryDiagnosticsInput {
  catalogId?: string;
  registry?: ToolRegistry;
  entries?: readonly ToolRegistryEntry[];
  bindings?: readonly ToolExecutableBinding[];
  inventoryRequest?: ToolInventoryRequest;
}

export interface ToolRegistryDiagnosticTool {
  name: string;
  aliases: readonly string[];
  category: string;
  toolsets: readonly string[];
  implementationModule: string;
  outputShape: string;
  version: string;
  status: ToolInventoryStatus | "invalid_registry";
  reasons: readonly string[];
  deprecated: boolean;
  replacement?: string;
}

export interface ToolRegistryDiagnosticsSummary {
  catalogId: string;
  registeredTools: number;
  selectedTools: number;
  deniedTools: number;
  missingTools: number;
  deprecatedTools: number;
  validationErrors: number;
  validationWarnings: number;
}

export interface ToolRegistryDiagnosticsReport {
  catalogId: string;
  validation: ToolRegistryValidation;
  inventory?: ToolInventory;
  summary: ToolRegistryDiagnosticsSummary;
  tools: ToolRegistryDiagnosticTool[];
}

export function buildToolRegistryDiagnostics(
  input: ToolRegistryDiagnosticsInput = {},
): ToolRegistryDiagnosticsReport {
  const catalogId = input.catalogId ?? "default-local-tools";
  const entries =
    input.registry?.entries ?? input.entries ?? defaultToolRegistry.entries;
  const bindings = input.registry
    ? [...input.registry.bindings.values()]
    : (input.bindings ??
      (input.entries === undefined
        ? [...defaultToolRegistry.bindings.values()]
        : []));
  const validation = validateToolRegistry(entries, bindings, {
    requireExecutableBindings: bindings.length > 0,
  });
  const registry =
    input.registry ??
    (validation.ok && bindings.length > 0
      ? createToolRegistry(entries, bindings)
      : undefined);
  const inventory = registry
    ? buildToolInventory(registry, input.inventoryRequest)
    : undefined;
  const inventoryItems = inventory?.items ?? [];
  const tools = entries.map<ToolRegistryDiagnosticTool>((entry) => {
    const item = inventoryItems.find(
      (candidate) => candidate.name === entry.name,
    );
    const binding =
      item?.binding ??
      bindings.find((candidate) => candidate.name === entry.name);
    const replacement = entry.replacement ?? entry.deprecated?.replacement;
    return {
      name: entry.name,
      aliases: entry.aliases ?? [],
      category: entry.category,
      toolsets: entry.toolsets,
      implementationModule:
        binding?.implementationModule ?? "(missing binding)",
      outputShape: entry.outputShape,
      version: entry.version,
      status:
        item?.status ?? (validation.ok ? "not_requested" : "invalid_registry"),
      reasons: item?.reasons ?? [
        "registry validation failed before inventory build",
      ],
      deprecated: Boolean(entry.deprecated),
      replacement,
    };
  });

  return {
    catalogId,
    validation,
    inventory,
    summary: summarizeDiagnostics(catalogId, validation, inventory, entries),
    tools,
  };
}

export function formatToolRegistryDiagnosticsMarkdown(
  report: ToolRegistryDiagnosticsReport,
): string {
  const lines = [
    `# Tool Registry Diagnostics: ${report.catalogId}`,
    "",
    `- registered tools: ${report.summary.registeredTools}`,
    `- selected tools: ${report.summary.selectedTools}`,
    `- denied tools: ${report.summary.deniedTools}`,
    `- missing requested tools: ${report.summary.missingTools}`,
    `- deprecated tools: ${report.summary.deprecatedTools}`,
    `- validation errors: ${report.summary.validationErrors}`,
    `- validation warnings: ${report.summary.validationWarnings}`,
    "",
    "| tool | status | implementation | output shape | notes |",
    "| --- | --- | --- | --- | --- |",
    ...report.tools.map((tool) =>
      [
        tool.name,
        tool.status,
        tool.implementationModule,
        tool.outputShape,
        tool.reasons.join("; "),
      ]
        .map(escapeMarkdownTableCell)
        .join(" | "),
    ),
  ];

  if (report.validation.issues.length > 0) {
    lines.push("", "## Validation Issues", "");
    for (const issue of report.validation.issues) {
      lines.push(`- ${issue.severity}: ${issue.code}: ${issue.message}`);
    }
  }

  return `${lines.join("\n")}\n`;
}

function summarizeDiagnostics(
  catalogId: string,
  validation: ToolRegistryValidation,
  inventory: ToolInventory | undefined,
  entries: readonly ToolRegistryEntry[],
): ToolRegistryDiagnosticsSummary {
  const items = inventory?.items ?? [];
  return {
    catalogId,
    registeredTools: entries.length,
    selectedTools: countStatus(items, "selected"),
    deniedTools:
      countStatus(items, "profile_denied") +
      countStatus(items, "session_denied") +
      countStatus(items, "resource_denied"),
    missingTools: countStatus(items, "missing"),
    deprecatedTools:
      entries.filter((entry) => entry.deprecated).length +
      countStatus(items, "deprecated"),
    validationErrors: validation.issues.filter(
      (issue) => issue.severity === "error",
    ).length,
    validationWarnings: validation.issues.filter(
      (issue) => issue.severity === "warning",
    ).length,
  };
}

function countStatus(
  items: readonly ToolInventoryItem[],
  status: ToolInventoryStatus,
): number {
  return items.filter((item) => item.status === status).length;
}

function escapeMarkdownTableCell(value: unknown): string {
  return String(value).replaceAll("|", "\\|").replaceAll("\n", " ");
}
