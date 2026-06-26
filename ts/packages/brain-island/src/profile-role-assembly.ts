import type { BrainRoleAssembly } from "./index.js";
import type {
  NativeSessionMemoryPromptContext,
  NativeSessionMemoryRecord,
} from "@rusty-crew/native-bridge";
import type { LoadedProfileContext } from "./profile-loading.js";

export type DenMemoryPromptMode =
  | "off"
  | "metadata"
  | "candidate"
  | "manual"
  | "permissive";

export interface DenMemoryPromptContext {
  mode: DenMemoryPromptMode;
  guidance?: string;
  projectId?: string;
  profileId?: string;
}

export interface DenseProfileMemoryPromptRecord {
  targetType: "profile" | "user";
  targetId?: string;
  key: string;
  content: string;
  revision?: number;
}

export interface RenderDenseProfileMemoryContextOptions {
  maxRecords?: number;
  maxContentChars?: number;
}

export interface PlanningPromptContext {
  todoContext?: string;
  sessionSearchGuidance?: string;
  counterGuidance?: string;
}

export interface BuildProfileRoleAssemblyOptions {
  systemPromptOverride?: string;
  additionalInstructions?: readonly string[];
  includeToolInventory?: boolean;
  includeSkillBodies?: boolean;
  denMemoryContext?: string;
  denseProfileMemoryContext?: string;
  sessionMemoryContext?: string;
  planningContext?: string;
  todoContext?: string;
}

export interface ProfileRoleAssemblyResult {
  systemPrompt: string;
  roleAssembly: BrainRoleAssembly;
}

export function buildProfileRoleAssembly(
  context: LoadedProfileContext,
  options: BuildProfileRoleAssemblyOptions = {},
): ProfileRoleAssemblyResult {
  const systemPrompt =
    options.systemPromptOverride ??
    context.profile.prompt?.system ??
    `You are ${context.profile.displayName ?? context.profile.profileId} in Rusty Crew.`;

  const instructions = [
    profileHeader(context),
    markdownSection("Profile Soul", context.profile.prompt?.soulMarkdown),
    markdownSection("Profile Memory", context.profile.prompt?.memoryMarkdown),
    instructionSection(context.profile.prompt?.instructions ?? []),
    options.denMemoryContext,
    options.denseProfileMemoryContext,
    options.sessionMemoryContext,
    skillSection(context, options.includeSkillBodies ?? true),
    options.includeToolInventory === false
      ? undefined
      : toolInventorySection(context),
    planningSection(options),
    runtimeSection(context),
    instructionSection(
      options.additionalInstructions ?? [],
      "Additional Instructions",
    ),
  ]
    .filter((section): section is string => Boolean(section))
    .join("\n\n");

  return {
    systemPrompt,
    roleAssembly: {
      instructions,
      initialMessages: [],
    },
  };
}

export function renderDenMemoryContext(
  context: DenMemoryPromptContext,
): string | undefined {
  if (context.mode === "off") {
    return undefined;
  }
  return [
    "# Den Memory",
    "Den Memories are external Den-owned memory. Use memory tools for source records; do not treat this prompt section as authoritative project state.",
    `Mode: ${context.mode}`,
    context.projectId ? `Project: ${context.projectId}` : undefined,
    context.profileId ? `Profile: ${context.profileId}` : undefined,
    context.guidance ? `Guidance: ${context.guidance}` : undefined,
  ]
    .filter((line): line is string => Boolean(line))
    .join("\n");
}

export function renderDenseProfileMemoryContext(
  records: readonly DenseProfileMemoryPromptRecord[],
  options: RenderDenseProfileMemoryContextOptions = {},
): string | undefined {
  if (records.length === 0) {
    return undefined;
  }
  const maxRecords = options.maxRecords ?? 12;
  const maxContentChars = options.maxContentChars ?? 500;
  const rendered = records.slice(0, maxRecords).map((record) => {
    const target =
      record.targetType === "profile"
        ? "profile"
        : `user:${record.targetId ?? "unknown"}`;
    const revision =
      record.revision === undefined ? "" : ` rev ${record.revision}`;
    return `- [${target}] ${record.key}${revision}: ${truncate(record.content, maxContentChars)}`;
  });
  return [
    "# Dense Profile Memory",
    "Compact stable runtime/profile memory. Do not use it for temporary task progress, todos, or Den product facts.",
    ...rendered,
    records.length > maxRecords
      ? `- ${records.length - maxRecords} additional records omitted.`
      : undefined,
  ]
    .filter((line): line is string => Boolean(line))
    .join("\n");
}

export interface RenderSessionMemoryContextOptions {
  maxContentChars?: number;
}

export function renderSessionMemoryContext(
  context: NativeSessionMemoryPromptContext | undefined,
  options: RenderSessionMemoryContextOptions = {},
): string | undefined {
  if (!context || context.records.length === 0) {
    return undefined;
  }
  const maxContentChars = options.maxContentChars ?? 700;
  const rendered = context.records.map((record) => {
    const branch = record.branch_id ? ` branch=${record.branch_id}` : "";
    return `- [${record.shape.shape_id} ${record.record_id} scope=${record.scope.scope_type}:${record.scope.scope_id}${branch}] ${truncate(sessionMemoryRecordText(record), maxContentChars)}`;
  });
  const diagnostics = context.diagnostics;
  return [
    "# Session Memory",
    "Rust-selected branch-aware session memory. Treat these as compact durable summaries, choices, and facts; prefer current conversation evidence when it conflicts.",
    `Session: ${diagnostics.session_id}`,
    diagnostics.active_branch_id
      ? `Active branch: ${diagnostics.active_branch_id}`
      : undefined,
    ...rendered,
    `Diagnostics: selected=${diagnostics.selected_records.length}; excluded wrong_branch=${diagnostics.excluded_counts.wrong_branch}, sibling_branch=${diagnostics.excluded_counts.sibling_branch}, tool_only=${diagnostics.excluded_counts.tool_only}, archived=${diagnostics.excluded_counts.archived}, superseded=${diagnostics.excluded_counts.superseded}, limit_exceeded=${diagnostics.excluded_counts.limit_exceeded}, policy_disabled=${diagnostics.excluded_counts.policy_disabled}; tokens~${diagnostics.token_estimate}`,
  ]
    .filter((line): line is string => Boolean(line))
    .join("\n");
}

export function renderPlanningContext(
  context: PlanningPromptContext,
): string | undefined {
  const sections = [
    context.todoContext,
    context.sessionSearchGuidance
      ? ["# Session Search", context.sessionSearchGuidance].join("\n")
      : undefined,
    context.counterGuidance
      ? ["# Runtime Counters", context.counterGuidance].join("\n")
      : undefined,
  ].filter((section): section is string => Boolean(section));

  if (sections.length === 0) {
    return undefined;
  }
  return ["# Planning Context", ...sections].join("\n\n");
}

function profileHeader(context: LoadedProfileContext): string {
  return [
    "# Profile",
    `Profile ID: ${context.profile.profileId}`,
    context.profile.displayName
      ? `Display name: ${context.profile.displayName}`
      : undefined,
    `Model: ${context.profile.modelConfig.provider}/${context.profile.modelConfig.modelName}`,
  ]
    .filter((line): line is string => Boolean(line))
    .join("\n");
}

function markdownSection(
  title: string,
  body: string | undefined,
): string | undefined {
  if (!body?.trim()) {
    return undefined;
  }
  return [`# ${title}`, body.trim()].join("\n");
}

function sessionMemoryRecordText(record: NativeSessionMemoryRecord): string {
  const content = record.content;
  if (typeof content === "string") {
    return content;
  }
  if (content && typeof content === "object" && !Array.isArray(content)) {
    const map = content as Record<string, unknown>;
    for (const key of [
      "summary",
      "choice",
      "decision",
      "fact",
      "content",
      "text",
      "value",
      "note",
    ]) {
      const value = map[key];
      if (typeof value === "string" && value.trim()) {
        return value.trim();
      }
    }
    const title = typeof map.title === "string" ? map.title.trim() : "";
    const summary = typeof map.summary === "string" ? map.summary.trim() : "";
    if (title || summary) {
      return [title, summary].filter(Boolean).join(": ");
    }
  }
  return JSON.stringify(content);
}

function instructionSection(
  instructions: readonly string[],
  title = "Profile Instructions",
): string | undefined {
  if (instructions.length === 0) {
    return undefined;
  }
  return [
    `# ${title}`,
    ...instructions.map((instruction) => `- ${instruction}`),
  ].join("\n");
}

function skillSection(
  context: LoadedProfileContext,
  includeBodies: boolean,
): string | undefined {
  if (context.skills.length === 0) {
    return undefined;
  }
  return [
    "# Selected Skills",
    ...context.skills.map((skill) =>
      [
        `## ${skill.title ?? skill.slug}`,
        skill.summary ? `Summary: ${skill.summary}` : undefined,
        skill.tags.length ? `Tags: ${skill.tags.join(", ")}` : undefined,
        includeBodies ? skill.bodyMarkdown : undefined,
      ]
        .filter((line): line is string => Boolean(line))
        .join("\n"),
    ),
  ].join("\n\n");
}

function toolInventorySection(
  context: LoadedProfileContext,
): string | undefined {
  const selected = context.toolSelection.inventory.selectedTools.map(
    (tool) => `- ${tool.name}: ${tool.description}`,
  );
  const denied = context.toolSelection.inventory.items
    .filter((item) =>
      [
        "profile_denied",
        "session_denied",
        "resource_denied",
        "deprecated",
      ].includes(item.status),
    )
    .map(
      (item) => `- ${item.name}: ${item.status} (${item.reasons.join("; ")})`,
    );

  if (selected.length === 0 && denied.length === 0) {
    return undefined;
  }

  return [
    "# Tool Inventory",
    selected.length ? "Selected tools:" : undefined,
    ...selected,
    denied.length ? "Unavailable tools:" : undefined,
    ...denied,
  ]
    .filter((line): line is string => Boolean(line))
    .join("\n");
}

function runtimeSection(context: LoadedProfileContext): string | undefined {
  const runtime = context.profile.runtime;
  if (!runtime) {
    return undefined;
  }
  const limits = runtime.defaultResourceLimits;
  const lines = [
    "# Runtime",
    runtime.maxTurns === undefined
      ? undefined
      : `Max turns: ${runtime.maxTurns}`,
    limits?.workdir ? `Workdir: ${limits.workdir}` : undefined,
    limits?.maxDurationMs === undefined
      ? undefined
      : `Max duration ms: ${limits.maxDurationMs}`,
    limits?.maxDelegationDepth === undefined
      ? undefined
      : `Max delegation depth: ${limits.maxDelegationDepth}`,
  ].filter((line): line is string => Boolean(line));
  return lines.length > 1 ? lines.join("\n") : undefined;
}

function planningSection(
  options: BuildProfileRoleAssemblyOptions,
): string | undefined {
  return [options.planningContext, options.todoContext]
    .filter((section): section is string => Boolean(section))
    .join("\n\n");
}

function truncate(value: string, maxChars: number): string {
  if (value.length <= maxChars) {
    return value;
  }
  return `${value.slice(0, Math.max(0, maxChars - 1))}...`;
}
