import type { BrainRoleAssembly } from "./index.js";
import type { LoadedProfileContext } from "./profile-loading.js";

export interface BuildProfileRoleAssemblyOptions {
  systemPromptOverride?: string;
  additionalInstructions?: readonly string[];
  includeToolInventory?: boolean;
  includeSkillBodies?: boolean;
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
    instructionSection(context.profile.prompt?.instructions ?? []),
    skillSection(context, options.includeSkillBodies ?? true),
    options.includeToolInventory === false
      ? undefined
      : toolInventorySection(context),
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
