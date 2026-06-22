import { createHash } from "node:crypto";
import type { ResourceLimits } from "@rusty-crew/contracts";
import type { AdapterDiagnosticsProjection } from "./adapter-diagnostics.js";
import type { BrainRoleAssembly } from "./index.js";
import type { LoadedProfileContext } from "./profile-loading.js";
import type {
  ProfileToolPolicy,
  SessionToolConstraints,
  ToolProfileSelection,
} from "./tool-profile-selection.js";
import type {
  ToolInventoryItem,
  ToolInventoryStatus,
  ToolRegistryValidationIssue,
} from "./tool-registry.js";
import type { ToolRegistryDiagnosticsReport } from "./tool-registry-diagnostics.js";

export type ToolContextToolStatus =
  | "selected"
  | "denied"
  | "missing"
  | "collided";

export type ToolContextReasonCode =
  | "selected_by_profile_policy"
  | "not_requested_by_profile_policy"
  | "denied_by_profile_policy"
  | "denied_by_session_policy"
  | "denied_by_resource_policy"
  | "deprecated_tool_hidden"
  | "missing_requested_tool"
  | "alias_shadowed_by_canonical_tool"
  | "registry_collision"
  | "registry_invalid"
  | "mcp_surface_degraded"
  | "adapter_unavailable"
  | "workdir_limited"
  | "resource_limited"
  | "profile_memory_unconfigured"
  | "profile_operational_default_declarative"
  | "den_memory_unavailable"
  | "skill_root_unavailable"
  | "dense_profile_memory_unavailable"
  | "session_search_unavailable"
  | "todo_state_unavailable"
  | "runtime_counter_unavailable";

export interface ToolContextDiagnosticsSession {
  sessionId: string;
  agentId: string;
  profileId: string;
  kind?: string;
}

export interface ToolContextDiagnosticsInput {
  now: string;
  session: ToolContextDiagnosticsSession;
  catalogId?: string;
  toolDiagnostics: ToolRegistryDiagnosticsReport;
  toolSelection?: ToolProfileSelection;
  profileContext?: LoadedProfileContext;
  toolPolicy?: ProfileToolPolicy;
  sessionConstraints?: SessionToolConstraints;
  roleAssembly?: BrainRoleAssembly;
  systemPrompt?: string;
  resourceLimits?: ResourceLimits;
  adapters?: AdapterDiagnosticsProjection;
  memorySkillsPlanning?: ToolContextMemorySkillsPlanningInput;
}

export interface ToolContextDiagnosticsReport {
  generatedAt: string;
  session: ToolContextDiagnosticsSession;
  catalogId: string;
  summary: ToolContextDiagnosticsSummary;
  policy: ToolContextPolicySummary;
  tools: ToolContextDiagnosticTool[];
  context: ToolContextAssemblySummary;
  memorySkillsPlanning: ToolContextMemorySkillsPlanningSummary;
  resources: ToolContextResourceSummary;
  adapters: ToolContextAdapterSummary;
  issues: ToolContextDiagnosticsIssue[];
}

export interface ToolContextDiagnosticsSummary {
  selectedTools: number;
  deniedTools: number;
  missingTools: number;
  collidedTools: number;
  localTools: number;
  mcpTools: number;
  webTools: number;
  browserTools: number;
  memoryTools: number;
  planningTools: number;
}

export interface ToolContextPolicySummary {
  requestedToolsets: readonly string[];
  requestedTools: readonly string[];
  deniedTools: readonly string[];
  sessionDeniedTools: readonly string[];
  resourceDeniedTools: readonly string[];
  readOnly: boolean;
  disallowedSafetyFlags: readonly string[];
  includeDeprecated: boolean;
}

export interface ToolContextDiagnosticTool {
  name: string;
  canonicalName?: string;
  status: ToolContextToolStatus;
  category?: string;
  surfaces: readonly string[];
  toolsets: readonly string[];
  implementationModule?: string;
  outputShape?: string;
  version?: string;
  source: "local" | "mcp" | "registry" | "missing";
  reasonCodes: readonly ToolContextReasonCode[];
  reasons: readonly string[];
}

export interface ToolContextAssemblySummary {
  systemPrompt: TextSurfaceSummary;
  instructions: TextSurfaceSummary;
  initialMessages: {
    count: number;
    totalChars: number;
    roles: readonly string[];
  };
  sections: readonly string[];
  skills: readonly ToolContextSkillSummary[];
  model?: {
    provider: string;
    modelName: string;
  };
  maxTurns?: number;
}

export interface TextSurfaceSummary {
  present: boolean;
  chars: number;
  lines: number;
  sha256?: string;
}

export interface ToolContextSkillSummary {
  slug: string;
  title?: string;
  summary?: string;
  tags: readonly string[];
  bodyChars: number;
}

export interface ToolContextResourceSummary {
  workdir?: string;
  workdirScoped: boolean;
  maxDurationMs?: number;
  maxDelegationDepth?: number;
  notes: readonly string[];
}

export interface ToolContextAdapterSummary {
  channels: {
    bindings: number;
    degraded: number;
    statuses: readonly string[];
  };
  mcp: {
    surfaces: number;
    degraded: number;
    collisions: number;
    statuses: readonly string[];
    serverNames: readonly string[];
  };
  notes: readonly string[];
}

export interface ToolContextMemorySkillsPlanningInput {
  denMemory?: DenMemoryDiagnosticsInput;
  skills?: SkillRootDiagnosticsInput;
  denseProfileMemory?: DenseProfileMemoryDiagnosticsInput;
  sessionSearch?: SessionSearchDiagnosticsInput;
  todo?: TodoDiagnosticsInput;
  counters?: RuntimeCounterDiagnosticsInput;
}

export interface DenMemoryDiagnosticsInput {
  configured: boolean;
  clientAvailable: boolean;
  mode?: string;
  endpointConfigured?: boolean;
  lastError?: string;
}

export interface SkillRootDiagnosticsInput {
  rootConfigured: boolean;
  rootReadable: boolean;
  profileSkillCount?: number;
  loadedSkillCount?: number;
  pinnedSkillCount?: number;
  protectedSkillCount?: number;
  invalidSkillCount?: number;
  missingSkillCount?: number;
  lastError?: string;
}

export interface DenseProfileMemoryDiagnosticsInput {
  clientAvailable: boolean;
  recordCount?: number;
  maxRecordsPerProfile?: number;
  capReached?: boolean;
  lastError?: string;
}

export interface SessionSearchDiagnosticsInput {
  available: boolean;
  indexedRows?: number;
  lastIndexedAt?: string;
  lastError?: string;
}

export interface TodoDiagnosticsInput {
  available: boolean;
  itemCount?: number;
  blockedItems?: number;
  expiresAt?: string;
  lastError?: string;
}

export interface RuntimeCounterDiagnosticsInput {
  available: boolean;
  resetAllowed?: boolean;
  summary?: {
    brainTurns?: number;
    wakes?: number;
    toolCalls?: number;
    toolErrors?: number;
    messages?: number;
    completions?: number;
    queueExpirations?: number;
  };
  lastError?: string;
}

export interface ToolContextMemorySkillsPlanningSummary {
  denMemory: {
    configured: boolean;
    clientAvailable: boolean;
    mode?: string;
    endpointConfigured?: boolean;
    lastError?: string;
  };
  skills: {
    rootConfigured: boolean;
    rootReadable: boolean;
    profileSkillCount: number;
    loadedSkillCount: number;
    pinnedSkillCount: number;
    protectedSkillCount: number;
    invalidSkillCount: number;
    missingSkillCount: number;
    lastError?: string;
  };
  denseProfileMemory: {
    clientAvailable: boolean;
    recordCount?: number;
    maxRecordsPerProfile?: number;
    capReached: boolean;
    lastError?: string;
  };
  sessionSearch: {
    available: boolean;
    indexedRows?: number;
    lastIndexedAt?: string;
    lastError?: string;
  };
  todo: {
    available: boolean;
    itemCount?: number;
    blockedItems?: number;
    expiresAt?: string;
    lastError?: string;
  };
  counters: {
    available: boolean;
    resetAllowed: boolean;
    summary?: RuntimeCounterDiagnosticsInput["summary"];
    lastError?: string;
  };
  notes: readonly string[];
}

export interface ToolContextDiagnosticsIssue {
  code: ToolContextReasonCode;
  severity: "warning" | "blocked";
  message: string;
  toolName?: string;
}

export function buildToolContextDiagnosticsReport(
  input: ToolContextDiagnosticsInput,
): ToolContextDiagnosticsReport {
  const policy = policySummary(input);
  const tools = buildToolReports(input);
  const context = assemblySummary(input);
  const memorySkillsPlanning = memorySkillsPlanningSummary(input);
  const resources = resourceSummary(input);
  const adapters = adapterSummary(input);
  const issues = [
    ...toolIssues(tools),
    ...profileConfigIssues(input, memorySkillsPlanning),
    ...memorySkillsPlanningIssues(memorySkillsPlanning),
    ...resourceIssues(resources),
    ...adapterIssues(adapters),
  ];

  return {
    generatedAt: input.now,
    session: input.session,
    catalogId:
      input.catalogId ??
      input.toolSelection?.catalogId ??
      input.toolDiagnostics.catalogId,
    summary: summarizeTools(tools),
    policy,
    tools,
    context,
    memorySkillsPlanning,
    resources,
    adapters,
    issues,
  };
}

export function formatToolContextDiagnosticsMarkdown(
  report: ToolContextDiagnosticsReport,
): string {
  const lines = [
    `# Tool and Context Diagnostics: ${report.session.sessionId}`,
    "",
    `- profile: ${report.session.profileId}`,
    `- catalog: ${report.catalogId}`,
    `- selected tools: ${report.summary.selectedTools}`,
    `- denied tools: ${report.summary.deniedTools}`,
    `- missing tools: ${report.summary.missingTools}`,
    `- collided tools: ${report.summary.collidedTools}`,
    `- instruction chars: ${report.context.instructions.chars}`,
    `- system prompt sha256: ${report.context.systemPrompt.sha256 ?? "none"}`,
    "",
    "## Policy",
    "",
    `- requested toolsets: ${joinOrNone(report.policy.requestedToolsets)}`,
    `- requested tools: ${joinOrNone(report.policy.requestedTools)}`,
    `- profile denied tools: ${joinOrNone(report.policy.deniedTools)}`,
    `- session denied tools: ${joinOrNone(report.policy.sessionDeniedTools)}`,
    `- resource denied tools: ${joinOrNone(report.policy.resourceDeniedTools)}`,
    `- read only: ${report.policy.readOnly}`,
    "",
    "## Tools",
    "",
    "| tool | status | category | source | reasons |",
    "| --- | --- | --- | --- | --- |",
    ...report.tools.map((tool) =>
      [
        tool.name,
        tool.status,
        tool.category ?? "unknown",
        tool.source,
        tool.reasons.join("; "),
      ]
        .map(escapeMarkdownTableCell)
        .join(" | "),
    ),
    "",
    "## Context",
    "",
    `- sections: ${joinOrNone(report.context.sections)}`,
    `- initial messages: ${report.context.initialMessages.count}`,
    `- skills: ${joinOrNone(report.context.skills.map((skill) => skill.slug))}`,
    "",
    "## Memory Skills Planning",
    "",
    `- den memory: ${statusWord(report.memorySkillsPlanning.denMemory.clientAvailable)}`,
    `- den memory mode: ${report.memorySkillsPlanning.denMemory.mode ?? "none"}`,
    `- skill root readable: ${report.memorySkillsPlanning.skills.rootReadable}`,
    `- skill counts: loaded ${report.memorySkillsPlanning.skills.loadedSkillCount} / profile ${report.memorySkillsPlanning.skills.profileSkillCount}`,
    `- pinned skills: ${report.memorySkillsPlanning.skills.pinnedSkillCount}`,
    `- dense profile memory records: ${report.memorySkillsPlanning.denseProfileMemory.recordCount ?? "unknown"}`,
    `- session search rows: ${report.memorySkillsPlanning.sessionSearch.indexedRows ?? "unknown"}`,
    `- todo items: ${report.memorySkillsPlanning.todo.itemCount ?? "unknown"}`,
    `- counter reset allowed: ${report.memorySkillsPlanning.counters.resetAllowed}`,
    `- notes: ${joinOrNone(report.memorySkillsPlanning.notes)}`,
    "",
    "## Resources And Adapters",
    "",
    `- workdir: ${report.resources.workdir ?? "none"}`,
    `- resource notes: ${joinOrNone(report.resources.notes)}`,
    `- channel statuses: ${joinOrNone(report.adapters.channels.statuses)}`,
    `- mcp statuses: ${joinOrNone(report.adapters.mcp.statuses)}`,
  ];

  if (report.issues.length > 0) {
    lines.push("", "## Issues", "");
    for (const issue of report.issues) {
      lines.push(`- ${issue.severity}: ${issue.code}: ${issue.message}`);
    }
  }

  return `${lines.join("\n")}\n`;
}

function buildToolReports(
  input: ToolContextDiagnosticsInput,
): ToolContextDiagnosticTool[] {
  const collisionIssues = collisionIssuesByTool(
    input.toolDiagnostics.validation.issues,
  );
  const inventoryItems =
    input.toolSelection?.inventory.items ??
    input.toolDiagnostics.inventory?.items;
  const byName = new Map(
    (inventoryItems ?? []).map((item) => [item.name, item] as const),
  );
  const reports: ToolContextDiagnosticTool[] = [];

  for (const diagnostic of input.toolDiagnostics.tools) {
    const item = byName.get(diagnostic.name);
    const collisionReasons = collisionIssues.get(diagnostic.name) ?? [];
    const status =
      collisionReasons.length > 0
        ? "collided"
        : mapToolStatus(item?.status ?? diagnostic.status);
    reports.push({
      name: diagnostic.name,
      canonicalName: item?.canonicalName,
      status,
      category: diagnostic.category,
      surfaces: item?.entry?.surfaces ?? [],
      toolsets: diagnostic.toolsets,
      implementationModule: diagnostic.implementationModule,
      outputShape: diagnostic.outputShape,
      version: diagnostic.version,
      source: diagnostic.category === "mcp" ? "mcp" : "local",
      reasonCodes: [
        ...reasonCodesForInventoryStatus(item?.status ?? diagnostic.status),
        ...(collisionReasons.length > 0
          ? (["registry_collision"] satisfies ToolContextReasonCode[])
          : []),
      ],
      reasons: [...(item?.reasons ?? diagnostic.reasons), ...collisionReasons],
    });
  }

  for (const item of inventoryItems ?? []) {
    if (input.toolDiagnostics.tools.some((tool) => tool.name === item.name)) {
      continue;
    }
    const status = mapToolStatus(item.status);
    reports.push({
      name: item.name,
      canonicalName: item.canonicalName,
      status,
      category: item.entry?.category,
      surfaces: item.entry?.surfaces ?? [],
      toolsets: item.entry?.toolsets ?? [],
      implementationModule: item.entry?.implementationModule,
      outputShape: item.entry?.outputShape,
      version: item.entry?.version,
      source: item.entry?.category === "mcp" ? "mcp" : "missing",
      reasonCodes: reasonCodesForInventoryStatus(item.status),
      reasons: item.reasons,
    });
  }

  return reports.sort((left, right) => {
    const rank = statusRank(left.status) - statusRank(right.status);
    return rank === 0 ? left.name.localeCompare(right.name) : rank;
  });
}

function mapToolStatus(
  status: ToolInventoryStatus | "invalid_registry",
): ToolContextToolStatus {
  switch (status) {
    case "selected":
      return "selected";
    case "missing":
      return "missing";
    case "collision":
    case "invalid_registry":
      return "collided";
    case "not_requested":
    case "profile_denied":
    case "session_denied":
    case "resource_denied":
    case "deprecated":
    case "shadowed":
      return "denied";
  }
}

function reasonCodesForInventoryStatus(
  status: ToolInventoryStatus | "invalid_registry",
): ToolContextReasonCode[] {
  switch (status) {
    case "selected":
      return ["selected_by_profile_policy"];
    case "not_requested":
      return ["not_requested_by_profile_policy"];
    case "profile_denied":
      return ["denied_by_profile_policy"];
    case "session_denied":
      return ["denied_by_session_policy"];
    case "resource_denied":
      return ["denied_by_resource_policy"];
    case "deprecated":
      return ["deprecated_tool_hidden"];
    case "missing":
      return ["missing_requested_tool"];
    case "shadowed":
      return ["alias_shadowed_by_canonical_tool"];
    case "collision":
      return ["registry_collision"];
    case "invalid_registry":
      return ["registry_invalid"];
  }
}

function collisionIssuesByTool(
  issues: readonly ToolRegistryValidationIssue[],
): Map<string, string[]> {
  const collisions = new Map<string, string[]>();
  for (const issue of issues) {
    if (!isCollisionIssue(issue)) {
      continue;
    }
    addCollision(collisions, issue.toolName, issue.message);
    addCollision(collisions, issue.otherToolName, issue.message);
  }
  return collisions;
}

function isCollisionIssue(issue: ToolRegistryValidationIssue): boolean {
  return [
    "duplicate_name",
    "alias_collides_with_name",
    "duplicate_alias",
    "capability_collision",
    "implementation_shape_drift",
  ].includes(issue.code);
}

function addCollision(
  collisions: Map<string, string[]>,
  toolName: string | undefined,
  message: string,
): void {
  if (!toolName) {
    return;
  }
  collisions.set(toolName, [...(collisions.get(toolName) ?? []), message]);
}

function assemblySummary(
  input: ToolContextDiagnosticsInput,
): ToolContextAssemblySummary {
  const profile = input.profileContext?.profile;
  const instructions = input.roleAssembly?.instructions;
  const messages = input.roleAssembly?.initialMessages ?? [];
  const messageRoles = messages.map((message) =>
    "role" in message && typeof message.role === "string"
      ? message.role
      : "unknown",
  );
  const messageChars = messages.reduce(
    (sum, message) => sum + JSON.stringify(message).length,
    0,
  );

  return {
    systemPrompt: textSummary(input.systemPrompt),
    instructions: textSummary(instructions),
    initialMessages: {
      count: messages.length,
      totalChars: messageChars,
      roles: uniqueSorted(messageRoles),
    },
    sections: sectionHeadings(instructions),
    skills:
      input.profileContext?.skills.map((skill) => ({
        slug: skill.slug,
        title: skill.title,
        summary: skill.summary,
        tags: skill.tags,
        bodyChars: skill.bodyMarkdown.length,
      })) ?? [],
    model: profile
      ? {
          provider: profile.modelConfig.provider,
          modelName: profile.modelConfig.modelName,
        }
      : undefined,
    maxTurns: profile?.runtime?.maxTurns,
  };
}

function textSummary(text: string | undefined): TextSurfaceSummary {
  return {
    present: Boolean(text),
    chars: text?.length ?? 0,
    lines: text ? text.split(/\r?\n/).length : 0,
    sha256: text ? createHash("sha256").update(text).digest("hex") : undefined,
  };
}

function sectionHeadings(text: string | undefined): readonly string[] {
  return (text ?? "")
    .split(/\r?\n/)
    .filter((line) => line.startsWith("#"))
    .map((line) => line.replace(/^#+\s*/, "").trim())
    .filter(Boolean)
    .slice(0, 20);
}

function resourceSummary(
  input: ToolContextDiagnosticsInput,
): ToolContextResourceSummary {
  const limits =
    input.resourceLimits ??
    input.profileContext?.profile.runtime?.defaultResourceLimits;
  const notes = [
    limits?.workdir ? "workdir scope is configured" : "workdir scope missing",
    limits?.maxDurationMs === undefined
      ? "duration limit missing"
      : "duration limit configured",
    limits?.maxDelegationDepth === undefined
      ? "delegation depth limit missing"
      : "delegation depth limit configured",
    input.sessionConstraints?.readOnly
      ? "session is read-only; write/process tools are resource denied"
      : undefined,
  ].filter((note): note is string => Boolean(note));

  return {
    workdir: limits?.workdir,
    workdirScoped: Boolean(limits?.workdir),
    maxDurationMs: limits?.maxDurationMs,
    maxDelegationDepth: limits?.maxDelegationDepth,
    notes,
  };
}

function memorySkillsPlanningSummary(
  input: ToolContextDiagnosticsInput,
): ToolContextMemorySkillsPlanningSummary {
  const supplied = input.memorySkillsPlanning;
  const profileSkillCount = input.profileContext?.profile.skills?.length ?? 0;
  const loadedSkillCount = input.profileContext?.skills.length ?? 0;
  const denMemory = {
    configured: supplied?.denMemory?.configured ?? false,
    clientAvailable: supplied?.denMemory?.clientAvailable ?? false,
    mode: supplied?.denMemory?.mode,
    endpointConfigured: supplied?.denMemory?.endpointConfigured,
    lastError: supplied?.denMemory?.lastError,
  };
  const skills = {
    rootConfigured: supplied?.skills?.rootConfigured ?? loadedSkillCount > 0,
    rootReadable: supplied?.skills?.rootReadable ?? loadedSkillCount > 0,
    profileSkillCount: supplied?.skills?.profileSkillCount ?? profileSkillCount,
    loadedSkillCount: supplied?.skills?.loadedSkillCount ?? loadedSkillCount,
    pinnedSkillCount: supplied?.skills?.pinnedSkillCount ?? 0,
    protectedSkillCount: supplied?.skills?.protectedSkillCount ?? 0,
    invalidSkillCount: supplied?.skills?.invalidSkillCount ?? 0,
    missingSkillCount: supplied?.skills?.missingSkillCount ?? 0,
    lastError: supplied?.skills?.lastError,
  };
  const denseProfileMemory = {
    clientAvailable:
      supplied?.denseProfileMemory?.clientAvailable ??
      hasTool(input, "dense_profile_memory"),
    recordCount: supplied?.denseProfileMemory?.recordCount,
    maxRecordsPerProfile: supplied?.denseProfileMemory?.maxRecordsPerProfile,
    capReached: supplied?.denseProfileMemory?.capReached ?? false,
    lastError: supplied?.denseProfileMemory?.lastError,
  };
  const sessionSearch = {
    available:
      supplied?.sessionSearch?.available ?? hasTool(input, "session_search"),
    indexedRows: supplied?.sessionSearch?.indexedRows,
    lastIndexedAt: supplied?.sessionSearch?.lastIndexedAt,
    lastError: supplied?.sessionSearch?.lastError,
  };
  const todo = {
    available: supplied?.todo?.available ?? hasTool(input, "todo"),
    itemCount: supplied?.todo?.itemCount,
    blockedItems: supplied?.todo?.blockedItems,
    expiresAt: supplied?.todo?.expiresAt,
    lastError: supplied?.todo?.lastError,
  };
  const counters = {
    available: supplied?.counters?.available ?? hasTool(input, "counter_reset"),
    resetAllowed: supplied?.counters?.resetAllowed ?? false,
    summary: supplied?.counters?.summary,
    lastError: supplied?.counters?.lastError,
  };
  const notes = [
    denMemory.lastError ? `den memory: ${denMemory.lastError}` : undefined,
    skills.lastError ? `skills: ${skills.lastError}` : undefined,
    denseProfileMemory.lastError
      ? `dense profile memory: ${denseProfileMemory.lastError}`
      : undefined,
    sessionSearch.lastError
      ? `session search: ${sessionSearch.lastError}`
      : undefined,
    todo.lastError ? `todo: ${todo.lastError}` : undefined,
    counters.lastError ? `counters: ${counters.lastError}` : undefined,
    denseProfileMemory.capReached
      ? "dense profile memory cap reached"
      : undefined,
    skills.pinnedSkillCount > 0
      ? `${skills.pinnedSkillCount} pinned skills`
      : undefined,
  ].filter((note): note is string => Boolean(note));

  return {
    denMemory,
    skills,
    denseProfileMemory,
    sessionSearch,
    todo,
    counters,
    notes,
  };
}

function adapterSummary(
  input: ToolContextDiagnosticsInput,
): ToolContextAdapterSummary {
  const channels =
    input.adapters?.channels.bindings.filter((binding) =>
      matchesSession(input, binding.sessionId, binding.profileId),
    ) ?? [];
  const mcp =
    input.adapters?.mcp.surfaces.filter((surface) =>
      matchesSession(input, surface.sessionId, surface.profileId),
    ) ?? [];
  const notes = [
    channels.length === 0
      ? "no channel binding diagnostics for session"
      : undefined,
    mcp.length === 0 ? "no mcp surface diagnostics for session" : undefined,
    ...mcp.flatMap((surface) =>
      surface.lastError
        ? [`mcp ${surface.bindingId}: ${surface.lastError}`]
        : [],
    ),
    ...channels.flatMap((binding) =>
      binding.lastError
        ? [`channel ${binding.bindingId}: ${binding.lastError}`]
        : [],
    ),
  ].filter((note): note is string => Boolean(note));

  return {
    channels: {
      bindings: channels.length,
      degraded: channels.filter((binding) => binding.status === "degraded")
        .length,
      statuses: uniqueSorted(channels.map((binding) => binding.status)),
    },
    mcp: {
      surfaces: mcp.length,
      degraded: mcp.filter((surface) => surface.status === "degraded").length,
      collisions: mcp.reduce((sum, surface) => sum + surface.collisionCount, 0),
      statuses: uniqueSorted(mcp.map((surface) => surface.status)),
      serverNames: uniqueSorted(mcp.flatMap((surface) => surface.serverNames)),
    },
    notes,
  };
}

function hasTool(
  input: ToolContextDiagnosticsInput,
  toolName: string,
): boolean {
  const inventoryItems =
    input.toolSelection?.inventory.items ??
    input.toolDiagnostics.inventory?.items ??
    [];
  return inventoryItems.some(
    (item) => item.name === toolName && item.status === "selected",
  );
}

function matchesSession(
  input: ToolContextDiagnosticsInput,
  sessionId: string | undefined,
  profileId: string,
): boolean {
  return (
    sessionId === input.session.sessionId ||
    (sessionId === undefined && profileId === input.session.profileId)
  );
}

function policySummary(
  input: ToolContextDiagnosticsInput,
): ToolContextPolicySummary {
  const policy = input.toolPolicy ?? input.profileContext?.profile.toolPolicy;
  return {
    requestedToolsets: policy?.requestedToolsets ?? [],
    requestedTools: policy?.requestedTools ?? [],
    deniedTools: policy?.deniedTools ?? [],
    sessionDeniedTools: input.sessionConstraints?.deniedTools ?? [],
    resourceDeniedTools: input.sessionConstraints?.resourceDeniedTools ?? [],
    readOnly: input.sessionConstraints?.readOnly ?? false,
    disallowedSafetyFlags:
      input.sessionConstraints?.disallowedSafetyFlags ?? [],
    includeDeprecated: policy?.includeDeprecated ?? false,
  };
}

function summarizeTools(
  tools: readonly ToolContextDiagnosticTool[],
): ToolContextDiagnosticsSummary {
  return {
    selectedTools: countStatus(tools, "selected"),
    deniedTools: countStatus(tools, "denied"),
    missingTools: countStatus(tools, "missing"),
    collidedTools: countStatus(tools, "collided"),
    localTools: countCategory(tools, "local"),
    mcpTools: countCategory(tools, "mcp"),
    webTools: countCategory(tools, "web"),
    browserTools: countCategory(tools, "browser"),
    memoryTools: countCategory(tools, "memory"),
    planningTools: countCategory(tools, "planning"),
  };
}

function toolIssues(
  tools: readonly ToolContextDiagnosticTool[],
): ToolContextDiagnosticsIssue[] {
  return tools.flatMap((tool) => {
    if (tool.status === "selected" || tool.status === "denied") {
      return [];
    }
    const code =
      tool.status === "missing"
        ? "missing_requested_tool"
        : "registry_collision";
    return [
      {
        code,
        severity: tool.status === "collided" ? "blocked" : "warning",
        message: `${tool.name}: ${tool.reasons.join("; ")}`,
        toolName: tool.name,
      },
    ] satisfies ToolContextDiagnosticsIssue[];
  });
}

function profileConfigIssues(
  input: ToolContextDiagnosticsInput,
  memorySkillsPlanning: ToolContextMemorySkillsPlanningSummary,
): ToolContextDiagnosticsIssue[] {
  const profile = input.profileContext?.profile;
  if (!profile) return [];
  const issues: ToolContextDiagnosticsIssue[] = [];
  const memoryConfig = profile.memoryConfig;
  const wantsDenMemory =
    memoryConfig?.denMemory === true ||
    (memoryConfig?.enabled === true && memoryConfig.denMemory !== false);
  const wantsDenseProfileMemory =
    memoryConfig?.denseProfileMemory === true ||
    (memoryConfig?.enabled === true &&
      memoryConfig.denseProfileMemory !== false);
  if (wantsDenMemory && !memorySkillsPlanning.denMemory.configured) {
    issues.push({
      code: "profile_memory_unconfigured",
      severity: "warning",
      message:
        "profile memoryConfig requests Den memory, but service Den memory is not configured",
      toolName: "den_memory_recall",
    });
  }
  if (
    wantsDenseProfileMemory &&
    !memorySkillsPlanning.denseProfileMemory.clientAvailable
  ) {
    issues.push({
      code: "profile_memory_unconfigured",
      severity: "warning",
      message:
        "profile memoryConfig requests dense profile memory, but the dense memory client is unavailable",
      toolName: "dense_profile_memory",
    });
  }

  const declarativeNotes = [
    profile.sessionDefaults
      ? "sessionDefaults are preserved for diagnostics; service sessions still come from runtime config"
      : undefined,
    profile.channelDefaults?.wakePolicy
      ? "channelDefaults.wakePolicy is declarative; a runtime channel binding is still required"
      : undefined,
    profile.runtime?.maxTokensPerTurn
      ? "runtimeConfig.maxTokensPerTurn is preserved; modelConfig.maxTokens/maxOutputTokens drives current model output limits"
      : undefined,
    profile.runtime?.maxTurnDurationMs
      ? "runtimeConfig.maxTurnDurationMs is preserved for diagnostics but is not yet enforced as a per-turn timeout"
      : undefined,
  ].filter((note): note is string => Boolean(note));
  for (const note of declarativeNotes) {
    issues.push({
      code: "profile_operational_default_declarative",
      severity: "warning",
      message: note,
    });
  }
  return issues;
}

function memorySkillsPlanningIssues(
  summary: ToolContextMemorySkillsPlanningSummary,
): ToolContextDiagnosticsIssue[] {
  const issues: ToolContextDiagnosticsIssue[] = [];
  if (summary.denMemory.configured && !summary.denMemory.clientAvailable) {
    issues.push({
      code: "den_memory_unavailable",
      severity: "warning",
      message: `Den memory is configured but unavailable${detail(summary.denMemory.lastError)}`,
      toolName: "den_memory_recall",
    });
  }
  if (
    summary.skills.rootConfigured &&
    (!summary.skills.rootReadable || summary.skills.missingSkillCount > 0)
  ) {
    issues.push({
      code: "skill_root_unavailable",
      severity: summary.skills.rootReadable ? "warning" : "blocked",
      message: `skill root has ${summary.skills.missingSkillCount} missing and ${summary.skills.invalidSkillCount} invalid skills${detail(summary.skills.lastError)}`,
      toolName: "skills_list",
    });
  }
  if (!summary.denseProfileMemory.clientAvailable) {
    issues.push({
      code: "dense_profile_memory_unavailable",
      severity: "warning",
      message: `dense profile memory client unavailable${detail(summary.denseProfileMemory.lastError)}`,
      toolName: "dense_profile_memory",
    });
  }
  if (!summary.sessionSearch.available) {
    issues.push({
      code: "session_search_unavailable",
      severity: "warning",
      message: `session search unavailable${detail(summary.sessionSearch.lastError)}`,
      toolName: "session_search",
    });
  }
  if (!summary.todo.available) {
    issues.push({
      code: "todo_state_unavailable",
      severity: "warning",
      message: `todo state unavailable${detail(summary.todo.lastError)}`,
      toolName: "todo",
    });
  }
  if (!summary.counters.available) {
    issues.push({
      code: "runtime_counter_unavailable",
      severity: "warning",
      message: `runtime counters unavailable${detail(summary.counters.lastError)}`,
      toolName: "counter_reset",
    });
  }
  return issues;
}

function detail(value: string | undefined): string {
  return value ? `: ${value}` : "";
}

function resourceIssues(
  resources: ToolContextResourceSummary,
): ToolContextDiagnosticsIssue[] {
  const issues: ToolContextDiagnosticsIssue[] = [];
  if (!resources.workdirScoped) {
    issues.push({
      code: "workdir_limited",
      severity: "warning",
      message: "session has no workdir scope configured",
    });
  }
  if (
    resources.maxDurationMs === undefined ||
    resources.maxDelegationDepth === undefined
  ) {
    issues.push({
      code: "resource_limited",
      severity: "warning",
      message: "session resource limits are incomplete",
    });
  }
  return issues;
}

function adapterIssues(
  adapters: ToolContextAdapterSummary,
): ToolContextDiagnosticsIssue[] {
  const issues: ToolContextDiagnosticsIssue[] = [];
  if (adapters.mcp.degraded > 0 || adapters.mcp.collisions > 0) {
    issues.push({
      code: "mcp_surface_degraded",
      severity: "warning",
      message: `${adapters.mcp.degraded} MCP surfaces degraded with ${adapters.mcp.collisions} collisions`,
    });
  }
  if (adapters.notes.length > 0) {
    issues.push({
      code: "adapter_unavailable",
      severity: "warning",
      message: adapters.notes.join("; "),
    });
  }
  return issues;
}

function countStatus(
  tools: readonly ToolContextDiagnosticTool[],
  status: ToolContextToolStatus,
): number {
  return tools.filter((tool) => tool.status === status).length;
}

function countCategory(
  tools: readonly ToolContextDiagnosticTool[],
  category: string,
): number {
  return tools.filter((tool) => tool.category === category).length;
}

function statusRank(status: ToolContextToolStatus): number {
  return {
    collided: 0,
    missing: 1,
    denied: 2,
    selected: 3,
  }[status];
}

function uniqueSorted(values: readonly string[]): readonly string[] {
  return [...new Set(values)].sort();
}

function joinOrNone(values: readonly string[]): string {
  return values.length > 0 ? values.join(", ") : "none";
}

function statusWord(ok: boolean): string {
  return ok ? "available" : "unavailable";
}

function escapeMarkdownTableCell(value: unknown): string {
  return String(value).replaceAll("|", "\\|").replaceAll("\n", " ");
}
