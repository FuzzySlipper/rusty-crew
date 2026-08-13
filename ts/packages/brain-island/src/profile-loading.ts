import { readdir, readFile } from "node:fs/promises";
import { join } from "node:path";
import type {
  BrainModelConfig,
  ExternalMessageDeliveryPolicy,
  ProfileId,
  ResourceLimits,
} from "@rusty-crew/contracts";
import type { BrainModuleId } from "./brain-catalog.js";
import {
  selectToolProfile,
  type ProfileToolPolicy,
  type SessionToolConstraints,
  type ToolProfileSelection,
} from "./tool-profile-selection.js";
import { defaultToolRegistry, type ToolRegistry } from "./tool-registry.js";
import {
  contextStrategyPolicyFromUnknownWithDiagnostics,
  type ContextStrategyPolicy,
} from "./context-strategy.js";
import { isReservedBuiltInSkillSlug } from "./built-in-skills.js";

export const DEFAULT_EXTERNAL_MESSAGE_DELIVERY_POLICY: ExternalMessageDeliveryPolicy =
  "immediate_steer";

export function isExternalMessageDeliveryPolicy(
  value: unknown,
): value is ExternalMessageDeliveryPolicy {
  return value === "immediate_steer" || value === "serial_next_turn";
}

export function parseExternalMessageDeliveryPolicy(
  value: unknown,
  fallback: ExternalMessageDeliveryPolicy = DEFAULT_EXTERNAL_MESSAGE_DELIVERY_POLICY,
): ExternalMessageDeliveryPolicy {
  if (value === undefined) return fallback;
  if (isExternalMessageDeliveryPolicy(value)) return value;
  throw new Error(
    "externalMessageDeliveryPolicy must be immediate_steer or serial_next_turn",
  );
}

export type ProfileLoadErrorCode =
  | "profile_not_found"
  | "invalid_profile_json"
  | "invalid_profile_yaml"
  | "invalid_profile_config"
  | "skill_not_found"
  | "invalid_skill_frontmatter";

export class ProfileLoadError extends Error {
  constructor(
    readonly code: ProfileLoadErrorCode,
    message: string,
    readonly cause?: unknown,
  ) {
    super(message);
    this.name = "ProfileLoadError";
  }
}

export interface ProfileRuntimeConfig {
  maxTurns?: number;
  defaultResourceLimits?: ResourceLimits;
  maxTokensPerTurn?: number;
}

export interface ProfilePromptFragments {
  system?: string;
  instructions?: string[];
  soulMarkdown?: string;
  memoryMarkdown?: string;
}

export interface ProfileMcpConfig {
  bindingId?: string;
  endpointRef?: string;
  serverNames?: string[];
  transport?: string;
  toolProfile?: string;
}

export interface ProfileBackgroundReviewConfig {
  enabled: boolean;
  reviewType?: "memory" | "skills" | "combined";
  schedule?: string;
  memoryNudgeInterval?: number;
  skillNudgeInterval?: number;
  maxTokens?: number;
  maxFindings?: number;
  maxCandidates?: number;
  llmReviewEnabled?: boolean;
  captureModelConfigId?: string;
  captureProviderAlias?: string;
  captureMaxProposals?: number;
  dryRun?: boolean;
}

export interface ProfileMemoryConfig {
  enabled?: boolean;
  denMemory?: boolean;
  denseProfileMemory?: boolean;
  sessionMemory?: boolean;
  sessionMemoryPrompt?: SessionMemoryPromptConfig;
}

export interface SessionMemoryPromptConfig {
  enabled?: boolean;
  maxRecords?: number;
  includeAncestors?: boolean;
  includeSiblings?: boolean;
}

export interface ProfileSessionDefaultsConfig {
  ownerId?: string;
  maxHistoryMessages?: number;
}

export interface ProfileChannelDefaultsConfig {
  wakePolicy?: "subscription" | "manual" | "disabled";
}

export type RoleplayNarratorTone =
  | "whimsical"
  | "dramatic"
  | "matter_of_fact"
  | "lush"
  | "wry";

export type RoleplayNarratorExplicitness =
  | "implied"
  | "suggestive"
  | "romantic"
  | "steamy";

export type RoleplayNarratorPacing =
  | "leisurely"
  | "balanced"
  | "rapid"
  | "breathless";

export type RoleplayNarratorMemoryDepth = "shallow" | "medium" | "deep";

export interface RoleplayNarratorReviewConfig {
  enabled: boolean;
  maxReviewCycles: number;
  checkGravityDrift: boolean;
  checkCharacterVoice: boolean;
  checkContinuity: boolean;
}

export interface RoleplayNarratorConfig {
  tone: RoleplayNarratorTone;
  explicitness: RoleplayNarratorExplicitness;
  pacing: RoleplayNarratorPacing;
  memoryDepth: RoleplayNarratorMemoryDepth;
  stylePrompt?: string;
  exemplar?: string;
  review: RoleplayNarratorReviewConfig;
}

export interface RoleplayMechanicConfig {
  autoMonitor: false;
}

export interface ProfileBrainConfig {
  module?: BrainModuleId;
  strategy?: string;
}

export interface ProfileModelSelectionCompatibilityDiagnostic {
  code:
    | "legacy_provider_alias_selection"
    | "inline_model_config_selection"
    | "legacy_runtime_model_selection";
  message: string;
}

export interface ProfileConfig {
  profileId: ProfileId;
  profileDir?: string;
  profileSkillsDir?: string;
  displayName?: string;
  modelConfigId?: string;
  /** Legacy selection identity retained only while profile files migrate. */
  providerAlias?: string;
  modelSelectionCompatibilityDiagnostics?: ProfileModelSelectionCompatibilityDiagnostic[];
  externalMessageDeliveryPolicy: ExternalMessageDeliveryPolicy;
  modelConfig: BrainModelConfig;
  brain?: ProfileBrainConfig;
  runtime?: ProfileRuntimeConfig;
  localToolProfileId?: string;
  toolPolicy?: ProfileToolPolicy;
  prompt?: ProfilePromptFragments;
  skills?: string[];
  skillsMode?: "listed" | "all";
  mcpConfig?: ProfileMcpConfig;
  backgroundReview?: ProfileBackgroundReviewConfig;
  memoryConfig?: ProfileMemoryConfig;
  roleplayNarrator?: RoleplayNarratorConfig;
  roleplayMechanic?: RoleplayMechanicConfig;
  contextPolicy?: ContextStrategyPolicy;
  sessionDefaults?: ProfileSessionDefaultsConfig;
  channelDefaults?: ProfileChannelDefaultsConfig;
}

export const profileRuntimeGraphWireFieldPaths = [
  "profiles[].background_review",
  "profiles[].background_review.enabled",
  "profiles[].background_review.review_type",
  "profiles[].background_review.schedule",
  "profiles[].brain",
  "profiles[].brain.module",
  "profiles[].brain.strategy",
  "profiles[].channel_defaults",
  "profiles[].channel_defaults.wake_policy",
  "profiles[].context_policy",
  "profiles[].context_policy.auto_compaction_enabled",
  "profiles[].context_policy.compact_at_percent",
  "profiles[].context_policy.debug_visibility",
  "profiles[].context_policy.enabled",
  "profiles[].context_policy.include_debug_events_in_model_context",
  "profiles[].context_policy.max_context_percent_for_wake",
  "profiles[].context_policy.strategy_config",
  "profiles[].context_policy.strategy_id",
  "profiles[].context_policy.target_percent_after_compaction",
  "profiles[].mcp_config",
  "profiles[].mcp_config.binding_id",
  "profiles[].mcp_config.endpoint_ref",
  "profiles[].mcp_config.server_names",
  "profiles[].mcp_config.tool_profile",
  "profiles[].mcp_config.transport",
  "profiles[].profile_id",
  "profiles[].runtime",
  "profiles[].runtime.default_resource_limits",
  "profiles[].runtime.default_resource_limits.max_delegation_depth",
  "profiles[].runtime.default_resource_limits.max_duration_ms",
  "profiles[].runtime.max_tokens_per_turn",
  "profiles[].session_defaults",
  "profiles[].session_defaults.max_history_messages",
  "profiles[].session_defaults.owner_id",
] as const;

export const profilePromptAssetConfigPaths = [
  "profiles[].prompt",
  "profiles[].prompt.system",
  "profiles[].prompt.instructions",
  "profiles[].prompt.soul_markdown",
  "profiles[].prompt.memory_markdown",
  "profiles[].skills",
  "profiles[].skills_mode",
  "profiles[].profile_skills_dir",
] as const;

export type ProfileConfigSourceFormat = "flat_json" | "directory_yaml";

export interface LoadedProfileConfigSource {
  profile: ProfileConfig;
  profilePath: string;
  profileDir: string;
  sourceFormat: ProfileConfigSourceFormat;
  rawProfileConfig: Record<string, unknown>;
  soulMarkdown?: string;
  memoryMarkdown?: string;
}

export interface LoadedSkill {
  slug: string;
  title?: string;
  summary?: string;
  tags: string[];
  bodyMarkdown: string;
  sourcePath: string;
}

export interface LoadedProfileContext {
  profile: ProfileConfig;
  skills: LoadedSkill[];
  toolSelection: ToolProfileSelection;
}

export type ExternalMemoryToolMode =
  | "off"
  | "metadata"
  | "candidate"
  | "manual"
  | "permissive";

export interface ExternalMemoryToolAvailability {
  configured: boolean;
  clientAvailable: boolean;
  mode: ExternalMemoryToolMode;
  lastError?: string;
}

export interface ToolAvailabilityPlannerInput {
  selectedTools: string[];
  denMemory: ExternalMemoryToolAvailability;
}

export interface ToolAvailabilityOmission {
  toolName: string;
  reasonCode: string;
  message: string;
}

export interface ToolAvailabilityPlan {
  selectedTools: string[];
  omittedTools: ToolAvailabilityOmission[];
  diagnostics: ToolAvailabilityOmission[];
}

export type ToolAvailabilityPlanner = (
  input: ToolAvailabilityPlannerInput,
) => Promise<ToolAvailabilityPlan>;

export interface LoadProfileContextInput {
  profilesDir: string;
  skillsDir?: string;
  profileId: ProfileId;
  registry?: ToolRegistry;
  session?: SessionToolConstraints;
  catalogId?: string;
  extraRequestedToolsets?: readonly string[];
  modelConfigResolver?: (modelConfigId: string) => Promise<BrainModelConfig>;
  /** Compatibility-only resolver for legacy providerAlias profile files. */
  modelProviderResolver?: (alias: string) => Promise<BrainModelConfig>;
  modelSelectionSource?: "normalized" | "legacy";
  toolAvailabilityPlanner?: ToolAvailabilityPlanner;
  externalMemoryAvailability?: ExternalMemoryToolAvailability;
  profilePromptAssets?: {
    soulMarkdown?: string;
    memoryMarkdown?: string;
  };
}

export interface LoadedProfileCuratorDiscoveryContext {
  readonly profile: ProfileConfig;
  readonly skills: LoadedSkill[];
  readonly missingSkillSlugs: string[];
}

export async function loadProfileContext(
  input: LoadProfileContextInput,
): Promise<LoadedProfileContext> {
  const registry = input.registry ?? defaultToolRegistry;
  let profile = await loadProfileConfig(input.profilesDir, input.profileId);
  if (input.profilePromptAssets !== undefined) {
    profile = {
      ...profile,
      prompt: {
        ...(profile.prompt ?? {}),
        soulMarkdown: input.profilePromptAssets.soulMarkdown,
        memoryMarkdown: input.profilePromptAssets.memoryMarkdown,
      },
    };
  }
  const modelSelectionSource =
    input.modelSelectionSource ?? runtimeModelSelectionSource();
  if (modelSelectionSource === "legacy") {
    const legacyAlias = profile.providerAlias ?? profile.modelConfigId;
    if (
      legacyAlias === undefined ||
      input.modelProviderResolver === undefined
    ) {
      throw invalidProfile(
        input.profileId,
        input.profileId,
        "legacy model selection requires providerAlias or modelConfigId and a legacy resolver",
      );
    }
    profile = {
      ...profile,
      modelConfig: await input.modelProviderResolver(legacyAlias),
      modelSelectionCompatibilityDiagnostics: [
        ...(profile.modelSelectionCompatibilityDiagnostics ?? []),
        {
          code: "legacy_runtime_model_selection",
          message: `runtime rollback flag selected the compatibility provider view for ${legacyAlias}`,
        },
      ],
    };
  } else if (profile.modelConfigId !== undefined) {
    if (input.modelConfigResolver === undefined) {
      throw invalidProfile(
        input.profileId,
        input.profileId,
        `modelConfigId ${profile.modelConfigId} requires a model configuration resolver`,
      );
    }
    profile = {
      ...profile,
      modelConfig: await input.modelConfigResolver(profile.modelConfigId),
    };
  } else if (profile.providerAlias !== undefined) {
    if (input.modelProviderResolver === undefined) {
      throw invalidProfile(
        input.profileId,
        input.profileId,
        `providerAlias ${profile.providerAlias} requires a model provider resolver`,
      );
    }
    profile = {
      ...profile,
      modelConfig: await input.modelProviderResolver(profile.providerAlias),
    };
  }
  const skills = await loadProfileSkills(
    profile,
    input.skillsDir ?? join(input.profilesDir, "skills"),
  );
  let toolSelection = selectToolProfile({
    profileId: profile.profileId,
    policy: withExtraRequestedToolsets(
      profile.toolPolicy ?? {},
      input.extraRequestedToolsets,
    ),
    session: input.session,
    registry,
    catalogId: input.catalogId,
  });
  if (
    input.toolAvailabilityPlanner !== undefined &&
    input.externalMemoryAvailability !== undefined
  ) {
    const availability = await input.toolAvailabilityPlanner({
      selectedTools: toolSelection.toolProfile.tools.map((tool) => tool.name),
      denMemory: input.externalMemoryAvailability,
    });
    if (availability.omittedTools.length > 0) {
      const deniedTools = [
        ...(input.session?.resourceDeniedTools ?? []),
        ...availability.omittedTools.map((omission) => omission.toolName),
      ];
      const deniedReasons = Object.fromEntries(
        availability.omittedTools.map((omission) => [
          omission.toolName,
          `${omission.reasonCode}: ${omission.message}`,
        ]),
      );
      toolSelection = selectToolProfile({
        profileId: profile.profileId,
        policy: withExtraRequestedToolsets(
          profile.toolPolicy ?? {},
          input.extraRequestedToolsets,
        ),
        session: {
          ...input.session,
          resourceDeniedTools: deniedTools,
          resourceDeniedReasons: {
            ...(input.session?.resourceDeniedReasons ?? {}),
            ...deniedReasons,
          },
        },
        registry,
        catalogId: input.catalogId,
      });
    }
  }

  return {
    profile,
    skills,
    toolSelection,
  };
}

export function runtimeModelSelectionSource(): "normalized" | "legacy" {
  const value = process.env.RUSTY_CREW_MODEL_SELECTION_SOURCE?.trim();
  if (value === undefined || value === "" || value === "normalized") {
    return "normalized";
  }
  if (value === "legacy") return "legacy";
  throw new Error(
    `RUSTY_CREW_MODEL_SELECTION_SOURCE must be normalized or legacy, got ${value}`,
  );
}

export async function loadProfileCuratorDiscoveryContext(input: {
  profilesDir: string;
  skillsDir?: string;
  profileId: ProfileId;
}): Promise<LoadedProfileCuratorDiscoveryContext> {
  const profile = await loadProfileConfig(input.profilesDir, input.profileId);
  const roots = skillRoots(
    profile,
    input.skillsDir ?? join(input.profilesDir, "skills"),
  );
  const slugs =
    profile.skillsMode === "all"
      ? await listSkillSlugsAcrossRoots(roots)
      : (profile.skills ?? []).filter(
          (slug) => !isReservedBuiltInSkillSlug(slug),
        );
  const skills: LoadedSkill[] = [];
  const missingSkillSlugs: string[] = [];
  for (const slug of slugs) {
    try {
      skills.push(await loadSkillFromRoots(roots, slug));
    } catch (error) {
      if (
        error instanceof ProfileLoadError &&
        error.code === "skill_not_found"
      ) {
        missingSkillSlugs.push(slug);
        continue;
      }
      throw error;
    }
  }
  return {
    profile,
    skills,
    missingSkillSlugs,
  };
}

export async function loadProfileConfig(
  profilesDir: string,
  profileId: ProfileId,
): Promise<ProfileConfig> {
  return (await loadProfileConfigWithSource(profilesDir, profileId)).profile;
}

export async function loadProfileConfigWithSource(
  profilesDir: string,
  profileId: ProfileId,
): Promise<LoadedProfileConfigSource> {
  const profilePath = join(profilesDir, `${profileId}.json`);
  let raw: string;
  try {
    raw = await readFile(profilePath, "utf8");
  } catch (error) {
    if (isNodeError(error) && error.code === "ENOENT") {
      return loadProfileDirectoryConfig(profilesDir, profileId, error);
    }
    throw error;
  }

  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch (error) {
    throw new ProfileLoadError(
      "invalid_profile_json",
      `profile ${profileId} is not valid JSON`,
      error,
    );
  }

  if (!isRecord(parsed)) {
    throw invalidProfile(
      profileId,
      profilePath,
      "profile root must be an object",
    );
  }
  const retiredField =
    isRecord(parsed.runtime) &&
    Object.hasOwn(parsed.runtime, "maxTurnDurationMs")
      ? "runtime.maxTurnDurationMs"
      : isRecord(parsed.runtimeConfig) &&
          Object.hasOwn(parsed.runtimeConfig, "maxTurnDurationMs")
        ? "runtimeConfig.maxTurnDurationMs"
        : isRecord(parsed.sessionDefaults) &&
            Object.hasOwn(parsed.sessionDefaults, "turnTimeoutMs")
          ? "sessionDefaults.turnTimeoutMs"
          : undefined;
  if (retiredField !== undefined) {
    throw invalidProfile(
      profileId,
      profilePath,
      `${retiredField} is retired; profile turns have no finite lifetime`,
    );
  }

  return {
    profile: validateProfileConfig(parsed, profileId, profilePath, {
      profileDir: profilesDir,
    }),
    profilePath,
    profileDir: profilesDir,
    sourceFormat: "flat_json",
    rawProfileConfig: parsed,
  };
}

export function parseProfileConfigDraft(input: {
  profilesDir: string;
  profileId: ProfileId;
  profileConfig: unknown;
  soulMarkdown?: string;
  memoryMarkdown?: string;
}): ProfileConfig {
  return validateProfileConfig(
    input.profileConfig,
    input.profileId,
    join(input.profilesDir, `${input.profileId}.json`),
    {
      profileDir: input.profilesDir,
      soulMarkdown: input.soulMarkdown,
      memoryMarkdown: input.memoryMarkdown,
    },
  );
}

async function loadProfileDirectoryConfig(
  profilesDir: string,
  profileId: ProfileId,
  jsonError: unknown,
): Promise<LoadedProfileConfigSource> {
  const profileDir = join(profilesDir, profileId);
  const profilePath = join(profileDir, "profile.yaml");
  let raw: string;
  try {
    raw = await readFile(profilePath, "utf8");
  } catch (error) {
    throw new ProfileLoadError(
      "profile_not_found",
      `profile ${profileId} was not found at ${join(profilesDir, `${profileId}.json`)} or ${profilePath}`,
      jsonError ?? error,
    );
  }

  let parsed: Record<string, unknown>;
  try {
    parsed = parseSimpleYaml(raw);
  } catch (error) {
    throw new ProfileLoadError(
      "invalid_profile_yaml",
      `profile ${profileId} is not valid supported YAML`,
      error,
    );
  }

  const [soulMarkdown, memoryMarkdown] = await Promise.all([
    readOptionalProfileMarkdown(profileDir, "soul.md"),
    readOptionalProfileMarkdown(profileDir, "memory.md"),
  ]);
  return {
    profile: validateProfileConfig(parsed, profileId, profilePath, {
      profileDir,
      soulMarkdown,
      memoryMarkdown,
    }),
    profilePath,
    profileDir,
    sourceFormat: "directory_yaml",
    rawProfileConfig: parsed,
    soulMarkdown,
    memoryMarkdown,
  };
}

async function loadProfileSkills(
  profile: ProfileConfig,
  globalSkillsDir: string,
): Promise<LoadedSkill[]> {
  const roots = skillRoots(profile, globalSkillsDir);
  const slugs =
    profile.skillsMode === "all"
      ? await listSkillSlugsAcrossRoots(roots)
      : (profile.skills ?? []).filter(
          (slug) => !isReservedBuiltInSkillSlug(slug),
        );
  return Promise.all(slugs.map((slug) => loadSkillFromRoots(roots, slug)));
}

function skillRoots(profile: ProfileConfig, globalSkillsDir: string): string[] {
  return [profile.profileSkillsDir, globalSkillsDir].filter(
    (root): root is string => root !== undefined,
  );
}

async function loadSkillFromRoots(
  skillsDirs: readonly string[],
  slug: string,
): Promise<LoadedSkill> {
  let firstError: unknown;
  for (const skillsDir of skillsDirs) {
    try {
      return await loadSkill(skillsDir, slug);
    } catch (error) {
      if (
        error instanceof ProfileLoadError &&
        error.code === "skill_not_found"
      ) {
        firstError ??= error;
        continue;
      }
      throw error;
    }
  }
  throw (
    firstError ??
    new ProfileLoadError("skill_not_found", `skill ${slug} was not found`)
  );
}

async function listSkillSlugsAcrossRoots(
  skillsDirs: readonly string[],
): Promise<string[]> {
  const slugs = new Set<string>();
  for (const skillsDir of skillsDirs) {
    for (const slug of await listAvailableSkillSlugs(skillsDir)) {
      if (!isReservedBuiltInSkillSlug(slug)) {
        slugs.add(slug);
      }
    }
  }
  return [...slugs].sort();
}

export async function loadSkill(
  skillsDir: string,
  slug: string,
): Promise<LoadedSkill> {
  const sourcePath = await resolveSkillSourcePath(skillsDir, slug);
  let raw: string;
  try {
    raw = await readFile(sourcePath, "utf8");
  } catch (error) {
    throw new ProfileLoadError(
      "skill_not_found",
      `skill ${slug} was not found at ${sourcePath}`,
      error,
    );
  }
  const { frontmatter, bodyMarkdown } = parseMarkdownFrontmatter(raw, slug);
  return {
    slug: optionalString(frontmatter.name) ?? slug,
    title:
      optionalString(frontmatter.title) ?? optionalString(frontmatter.name),
    summary:
      optionalString(frontmatter.summary) ??
      optionalString(frontmatter.description),
    tags: stringList(frontmatter.tags),
    bodyMarkdown,
    sourcePath,
  };
}

export async function listAvailableSkillSlugs(
  skillsDir: string,
): Promise<string[]> {
  return [...new Set(await collectSkillSlugs(skillsDir, 0))].sort();
}

async function collectSkillSlugs(
  dir: string,
  depth: number,
): Promise<string[]> {
  if (depth > 4) return [];
  let entries: Array<{
    isDirectory(): boolean;
    isFile(): boolean;
    name: string;
  }>;
  try {
    entries = await readdir(dir, { withFileTypes: true });
  } catch (error) {
    if (isNodeError(error) && error.code === "ENOENT") return [];
    throw error;
  }

  const slugs: string[] = [];
  for (const entry of entries) {
    if (entry.name.startsWith(".")) continue;
    if (depth === 0 && entry.isFile() && entry.name.endsWith(".md")) {
      const slug = entry.name.slice(0, -".md".length);
      if (isSafeSkillSlug(slug)) slugs.push(slug);
      continue;
    }
    if (!entry.isDirectory()) continue;
    const child = join(dir, entry.name);
    if (
      isSafeSkillSlug(entry.name) &&
      (await firstReadablePath([join(child, "SKILL.md")])) !== undefined
    ) {
      slugs.push(entry.name);
    }
    slugs.push(...(await collectSkillSlugs(child, depth + 1)));
  }
  return slugs;
}

async function resolveSkillSourcePath(
  skillsDir: string,
  slug: string,
): Promise<string> {
  const directCandidates = [
    join(skillsDir, `${slug}.md`),
    join(skillsDir, slug, "SKILL.md"),
  ];
  const direct = await firstReadablePath(directCandidates);
  if (direct !== undefined) return direct;

  if (slug.includes("/")) return directCandidates[0]!;

  const matches = await findSkillDirectoryMatches(skillsDir, slug);
  if (matches.length === 1) return matches[0]!;
  if (matches.length > 1) {
    throw new ProfileLoadError(
      "invalid_profile_config",
      `skill ${slug} is ambiguous in ${skillsDir}: ${matches.join(", ")}`,
    );
  }
  return directCandidates[0]!;
}

async function firstReadablePath(
  candidates: readonly string[],
): Promise<string | undefined> {
  for (const candidate of candidates) {
    try {
      await readFile(candidate, "utf8");
      return candidate;
    } catch (error) {
      if (!isNodeError(error) || error.code !== "ENOENT") throw error;
    }
  }
  return undefined;
}

async function findSkillDirectoryMatches(
  skillsDir: string,
  slug: string,
): Promise<string[]> {
  const matches: string[] = [];
  await collectSkillDirectoryMatches(skillsDir, slug, matches, 0);
  return matches.sort();
}

async function collectSkillDirectoryMatches(
  dir: string,
  slug: string,
  matches: string[],
  depth: number,
): Promise<void> {
  if (depth > 4) return;
  let entries: Array<{ isDirectory(): boolean; name: string }>;
  try {
    entries = await readdir(dir, { withFileTypes: true });
  } catch (error) {
    if (isNodeError(error) && error.code === "ENOENT") return;
    throw error;
  }

  for (const entry of entries) {
    if (!entry.isDirectory() || entry.name.startsWith(".")) continue;
    const child = join(dir, entry.name);
    if (entry.name === slug) {
      const sourcePath = await firstReadablePath([join(child, "SKILL.md")]);
      if (sourcePath !== undefined) {
        matches.push(sourcePath);
      }
    }
    await collectSkillDirectoryMatches(child, slug, matches, depth + 1);
  }
}

function validateProfileConfig(
  parsed: unknown,
  profileId: ProfileId,
  profilePath: string,
  fragments: Pick<ProfilePromptFragments, "soulMarkdown" | "memoryMarkdown"> & {
    profileDir?: string;
  } = {},
): ProfileConfig {
  if (!isRecord(parsed)) {
    throw invalidProfile(
      profileId,
      profilePath,
      "profile root must be an object",
    );
  }
  const rawProfileId =
    optionalString(parsed.profileId) ??
    optionalString(parsed.profileIdentity) ??
    profileId;
  if (rawProfileId !== profileId) {
    throw invalidProfile(
      profileId,
      profilePath,
      `profileId ${rawProfileId} does not match requested ${profileId}`,
    );
  }
  const providerAlias = optionalString(parsed.providerAlias);
  const modelConfigId = optionalString(parsed.modelConfigId);
  const modelConfig = parsed.modelConfig;
  if (
    modelConfigId === undefined &&
    !isRecord(modelConfig) &&
    providerAlias === undefined
  ) {
    throw invalidProfile(profileId, profilePath, "modelConfigId is required");
  }
  const resolvedModelConfig = isRecord(modelConfig)
    ? modelConfigFromProfileObject(modelConfig)
    : {
        provider:
          modelConfigId === undefined
            ? "unresolved-provider-alias"
            : "unresolved-model-configuration",
        modelName: modelConfigId ?? providerAlias!,
      };
  const modelSelectionCompatibilityDiagnostics = [
    ...(modelConfigId === undefined && providerAlias !== undefined
      ? [
          {
            code: "legacy_provider_alias_selection" as const,
            message: `providerAlias ${providerAlias} is compatibility-only; migrate the profile to modelConfigId`,
          },
        ]
      : []),
    ...(modelConfigId === undefined && isRecord(modelConfig)
      ? [
          {
            code: "inline_model_config_selection" as const,
            message:
              "inline modelConfig is compatibility-only; migrate the profile to a registered modelConfigId",
          },
        ]
      : []),
  ];
  const runtimeConfig = isRecord(parsed.runtimeConfig)
    ? parsed.runtimeConfig
    : undefined;

  return {
    profileId,
    profileDir: fragments.profileDir,
    profileSkillsDir:
      fragments.profileDir === undefined
        ? undefined
        : join(fragments.profileDir, "skills"),
    displayName: optionalString(parsed.displayName),
    modelConfigId,
    providerAlias,
    ...(modelSelectionCompatibilityDiagnostics.length === 0
      ? {}
      : { modelSelectionCompatibilityDiagnostics }),
    externalMessageDeliveryPolicy: profileExternalMessageDeliveryPolicy(
      parsed.externalMessageDeliveryPolicy,
      profileId,
      profilePath,
    ),
    modelConfig: resolvedModelConfig,
    brain: isRecord(parsed.brain)
      ? {
          module: brainModuleId(parsed.brain.module, profileId, profilePath),
          strategy: optionalString(parsed.brain.strategy),
        }
      : undefined,
    runtime: isRecord(parsed.runtime)
      ? {
          maxTurns: optionalNumber(parsed.runtime.maxTurns),
          maxTokensPerTurn: optionalNumber(parsed.runtime.maxTokensPerTurn),
          defaultResourceLimits: isRecord(parsed.runtime.defaultResourceLimits)
            ? {
                maxDurationMs: optionalNumber(
                  parsed.runtime.defaultResourceLimits.maxDurationMs,
                ),
                maxDelegationDepth: optionalNumber(
                  parsed.runtime.defaultResourceLimits.maxDelegationDepth,
                ),
              }
            : undefined,
        }
      : runtimeConfig
        ? {
            maxTurns: optionalNumber(runtimeConfig.maxIterations),
            maxTokensPerTurn: optionalNumber(runtimeConfig.maxTokensPerTurn),
            defaultResourceLimits: {
              maxDurationMs: optionalNumber(runtimeConfig.maxDurationMs),
            },
          }
        : undefined,
    localToolProfileId: optionalString(parsed.localToolProfileId),
    toolPolicy: isRecord(parsed.toolPolicy)
      ? profileToolPolicy(parsed.toolPolicy)
      : undefined,
    prompt: isRecord(parsed.prompt)
      ? {
          system: optionalString(parsed.prompt.system),
          instructions: stringList(parsed.prompt.instructions),
          soulMarkdown:
            fragments.soulMarkdown ??
            optionalString(parsed.prompt.soulMarkdown),
          memoryMarkdown:
            fragments.memoryMarkdown ??
            optionalString(parsed.prompt.memoryMarkdown),
        }
      : fragments.soulMarkdown || fragments.memoryMarkdown
        ? {
            soulMarkdown: fragments.soulMarkdown,
            memoryMarkdown: fragments.memoryMarkdown,
          }
        : undefined,
    skills: parsed.skills === "all" ? [] : stringList(parsed.skills),
    skillsMode: parsed.skills === "all" ? "all" : "listed",
    mcpConfig: isRecord(parsed.mcpConfig)
      ? {
          bindingId: optionalString(parsed.mcpConfig.bindingId),
          endpointRef: optionalString(parsed.mcpConfig.endpointRef),
          serverNames: stringList(parsed.mcpConfig.serverNames),
          transport: optionalString(parsed.mcpConfig.transport),
          toolProfile: optionalString(parsed.mcpConfig.toolProfile),
        }
      : undefined,
    backgroundReview: isRecord(parsed.backgroundReview)
      ? profileBackgroundReviewConfig(parsed.backgroundReview)
      : undefined,
    memoryConfig: isRecord(parsed.memoryConfig)
      ? profileMemoryConfig(parsed.memoryConfig)
      : undefined,
    roleplayNarrator: isRecord(parsed.roleplayNarrator)
      ? roleplayNarratorConfig(parsed.roleplayNarrator)
      : undefined,
    roleplayMechanic: isRecord(parsed.roleplayMechanic)
      ? roleplayMechanicConfig(parsed.roleplayMechanic, profileId, profilePath)
      : undefined,
    contextPolicy: isRecord(parsed.contextPolicy)
      ? profileContextPolicy(parsed.contextPolicy, profileId, profilePath)
      : isRecord(parsed.context_policy)
        ? profileContextPolicy(parsed.context_policy, profileId, profilePath)
        : undefined,
    sessionDefaults: isRecord(parsed.sessionDefaults)
      ? {
          ownerId: optionalString(parsed.sessionDefaults.ownerId),
          maxHistoryMessages: optionalNumber(
            parsed.sessionDefaults.maxHistoryMessages,
          ),
        }
      : undefined,
    channelDefaults: isRecord(parsed.channelDefaults)
      ? {
          wakePolicy: profileWakePolicy(
            parsed.channelDefaults.wakePolicy,
            profileId,
            profilePath,
          ),
        }
      : undefined,
  };
}

function profileExternalMessageDeliveryPolicy(
  value: unknown,
  profileId: ProfileId,
  profilePath: string,
): ExternalMessageDeliveryPolicy {
  if (value === undefined) return DEFAULT_EXTERNAL_MESSAGE_DELIVERY_POLICY;
  if (isExternalMessageDeliveryPolicy(value)) return value;
  throw invalidProfile(
    profileId,
    profilePath,
    "externalMessageDeliveryPolicy must be immediate_steer or serial_next_turn",
  );
}

function brainModuleId(
  input: unknown,
  profileId: ProfileId,
  profilePath: string,
): BrainModuleId | undefined {
  const value = optionalString(input);
  if (value === undefined) return undefined;
  if (!/^[A-Za-z0-9][A-Za-z0-9_.-]{0,79}$/.test(value)) {
    throw invalidProfile(
      profileId,
      profilePath,
      "brain.module must start with a letter or number and contain only letters, numbers, underscore, dot, or hyphen",
    );
  }
  return value;
}

async function readOptionalProfileMarkdown(
  profileDir: string,
  filename: string,
): Promise<string | undefined> {
  try {
    const raw = await readFile(join(profileDir, filename), "utf8");
    const trimmed = raw.trim();
    return trimmed ? trimmed : undefined;
  } catch (error) {
    if (isNodeError(error) && error.code === "ENOENT") {
      return undefined;
    }
    throw error;
  }
}

function profileToolPolicy(raw: Record<string, unknown>): ProfileToolPolicy {
  if (optionalString(raw.mode) === "allow_all") {
    return {
      requestedToolsets: allDefaultToolsets(),
      deniedTools: stringList(raw.deniedTools),
      includeDeprecated:
        typeof raw.includeDeprecated === "boolean"
          ? raw.includeDeprecated
          : undefined,
    };
  }
  return {
    requestedToolsets: stringList(raw.requestedToolsets),
    requestedTools: stringList(raw.requestedTools),
    deniedTools: stringList(raw.deniedTools),
    includeDeprecated:
      typeof raw.includeDeprecated === "boolean"
        ? raw.includeDeprecated
        : undefined,
  };
}

function profileBackgroundReviewConfig(
  raw: Record<string, unknown>,
): ProfileBackgroundReviewConfig {
  return {
    enabled: raw.enabled === true,
    reviewType: backgroundReviewType(raw.reviewType),
    schedule: optionalString(raw.schedule),
    memoryNudgeInterval: optionalNumber(raw.memoryNudgeInterval),
    skillNudgeInterval: optionalNumber(raw.skillNudgeInterval),
    maxTokens: optionalNumber(raw.maxTokens),
    maxFindings: optionalNumber(raw.maxFindings),
    maxCandidates: optionalNumber(raw.maxCandidates),
    llmReviewEnabled:
      typeof raw.llmReviewEnabled === "boolean"
        ? raw.llmReviewEnabled
        : undefined,
    captureModelConfigId: optionalString(raw.captureModelConfigId),
    captureProviderAlias: optionalString(raw.captureProviderAlias),
    captureMaxProposals: optionalNumber(raw.captureMaxProposals),
    dryRun: typeof raw.dryRun === "boolean" ? raw.dryRun : undefined,
  };
}

function backgroundReviewType(
  value: unknown,
): ProfileBackgroundReviewConfig["reviewType"] {
  if (value === "memory" || value === "skills" || value === "combined") {
    return value;
  }
  return undefined;
}

function profileMemoryConfig(
  raw: Record<string, unknown>,
): ProfileMemoryConfig {
  return {
    enabled: typeof raw.enabled === "boolean" ? raw.enabled : undefined,
    denMemory: typeof raw.denMemory === "boolean" ? raw.denMemory : undefined,
    denseProfileMemory:
      typeof raw.denseProfileMemory === "boolean"
        ? raw.denseProfileMemory
        : undefined,
    sessionMemory:
      typeof raw.sessionMemory === "boolean" ? raw.sessionMemory : undefined,
    sessionMemoryPrompt: isRecord(raw.sessionMemoryPrompt)
      ? sessionMemoryPromptConfig(raw.sessionMemoryPrompt)
      : undefined,
  };
}

function profileContextPolicy(
  raw: Record<string, unknown>,
  profileId: ProfileId,
  profilePath: string,
): ContextStrategyPolicy {
  const result = contextStrategyPolicyFromUnknownWithDiagnostics(raw);
  const errors = result.diagnostics.filter(
    (diagnostic) => diagnostic.severity === "error",
  );
  if (errors.length > 0) {
    throw invalidProfile(
      profileId,
      profilePath,
      errors.map((diagnostic) => diagnostic.message).join("; "),
    );
  }
  return result.policy;
}

export function sessionMemoryPromptConfig(
  raw: Record<string, unknown>,
): SessionMemoryPromptConfig {
  return {
    enabled: typeof raw.enabled === "boolean" ? raw.enabled : undefined,
    maxRecords: optionalNumber(raw.maxRecords),
    includeAncestors:
      typeof raw.includeAncestors === "boolean"
        ? raw.includeAncestors
        : undefined,
    includeSiblings:
      typeof raw.includeSiblings === "boolean"
        ? raw.includeSiblings
        : undefined,
  };
}

function roleplayNarratorConfig(
  raw: Record<string, unknown>,
): RoleplayNarratorConfig {
  const review = isRecord(raw.review) ? raw.review : {};
  return {
    tone: narratorTone(raw.tone) ?? "lush",
    explicitness: narratorExplicitness(raw.explicitness) ?? "romantic",
    pacing: narratorPacing(raw.pacing) ?? "balanced",
    memoryDepth: narratorMemoryDepth(raw.memoryDepth) ?? "medium",
    stylePrompt: optionalString(raw.stylePrompt ?? raw.style_prompt),
    exemplar: optionalString(raw.exemplar ?? raw.styleExemplar),
    review: {
      enabled: review.enabled === true,
      maxReviewCycles: optionalNumber(review.maxReviewCycles) ?? 1,
      checkGravityDrift:
        typeof review.checkGravityDrift === "boolean"
          ? review.checkGravityDrift
          : true,
      checkCharacterVoice:
        typeof review.checkCharacterVoice === "boolean"
          ? review.checkCharacterVoice
          : true,
      checkContinuity:
        typeof review.checkContinuity === "boolean"
          ? review.checkContinuity
          : true,
    },
  };
}

function roleplayMechanicConfig(
  raw: Record<string, unknown>,
  profileId: ProfileId,
  profilePath: string,
): RoleplayMechanicConfig {
  const autoMonitor = isRecord(raw.autoMonitor)
    ? raw.autoMonitor.enabled
    : (raw.autoMonitor ?? raw.auto_monitor);
  if (autoMonitor !== undefined && autoMonitor !== false) {
    throw invalidProfile(
      profileId,
      profilePath,
      "roleplayMechanic.autoMonitor is not available and must be false",
    );
  }
  return { autoMonitor: false };
}

function narratorTone(value: unknown): RoleplayNarratorTone | undefined {
  return value === "whimsical" ||
    value === "dramatic" ||
    value === "matter_of_fact" ||
    value === "lush" ||
    value === "wry"
    ? value
    : undefined;
}

function narratorExplicitness(
  value: unknown,
): RoleplayNarratorExplicitness | undefined {
  return value === "implied" ||
    value === "suggestive" ||
    value === "romantic" ||
    value === "steamy"
    ? value
    : undefined;
}

function narratorPacing(value: unknown): RoleplayNarratorPacing | undefined {
  return value === "leisurely" ||
    value === "balanced" ||
    value === "rapid" ||
    value === "breathless"
    ? value
    : undefined;
}

function narratorMemoryDepth(
  value: unknown,
): RoleplayNarratorMemoryDepth | undefined {
  return value === "shallow" || value === "medium" || value === "deep"
    ? value
    : undefined;
}

function profileWakePolicy(
  value: unknown,
  profileId: ProfileId,
  profilePath: string,
): ProfileChannelDefaultsConfig["wakePolicy"] {
  if (value === undefined) return undefined;
  if (value === "subscription" || value === "manual" || value === "disabled") {
    return value;
  }
  throw invalidProfile(
    profileId,
    profilePath,
    "channelDefaults.wakePolicy must be subscription, manual, or disabled",
  );
}

function allDefaultToolsets(): string[] {
  return [
    ...new Set(defaultToolRegistry.entries.flatMap((entry) => entry.toolsets)),
  ].sort();
}

function withExtraRequestedToolsets(
  policy: ProfileToolPolicy,
  extraRequestedToolsets: readonly string[] | undefined,
): ProfileToolPolicy {
  if (!extraRequestedToolsets || extraRequestedToolsets.length === 0) {
    return policy;
  }
  return {
    ...policy,
    requestedToolsets: [
      ...new Set([
        ...(policy.requestedToolsets ?? []),
        ...extraRequestedToolsets,
      ]),
    ].sort(),
  };
}

function temperatureToMilli(value: number | undefined): number | undefined {
  if (value === undefined) return undefined;
  return Math.round(value * 1_000);
}

function modelConfigFromProfileObject(
  modelConfig: Record<string, unknown>,
): BrainModelConfig {
  const provider = requiredString(modelConfig.provider);
  const modelName = requiredString(modelConfig.modelName ?? modelConfig.model);
  const temperatureMilli =
    optionalNumber(modelConfig.temperatureMilli) ??
    temperatureToMilli(optionalNumber(modelConfig.temperature));
  return {
    provider,
    modelName,
    baseUrl: optionalString(modelConfig.baseUrl),
    api: optionalString(modelConfig.api),
    apiKeyEnv: optionalString(modelConfig.apiKeyEnv),
    credentialKind: optionalString(modelConfig.credentialKind) as
      | BrainModelConfig["credentialKind"]
      | undefined,
    temperatureMilli,
    reasoningEffort: optionalString(modelConfig.reasoningEffort),
    reasoningFormat: optionalString(modelConfig.reasoningFormat),
    responsesDialect: optionalString(
      modelConfig.responsesDialect,
    ) as BrainModelConfig["responsesDialect"],
    chatCompletionsDialect: optionalString(
      modelConfig.chatCompletionsDialect,
    ) as BrainModelConfig["chatCompletionsDialect"],
    thinkingMode: optionalString(
      modelConfig.thinkingMode,
    ) as BrainModelConfig["thinkingMode"],
    reasoningHistory: optionalString(
      modelConfig.reasoningHistory,
    ) as BrainModelConfig["reasoningHistory"],
    promptCaching: optionalString(
      modelConfig.promptCaching,
    ) as BrainModelConfig["promptCaching"],
    reasoningBudgetTokens: optionalNumber(modelConfig.reasoningBudgetTokens),
    maxOutputTokens:
      optionalNumber(modelConfig.maxOutputTokens) ??
      optionalNumber(modelConfig.maxTokens),
  };
}

function parseMarkdownFrontmatter(
  raw: string,
  slug: string,
): { frontmatter: Record<string, unknown>; bodyMarkdown: string } {
  if (!raw.startsWith("---\n")) {
    return { frontmatter: {}, bodyMarkdown: raw.trim() };
  }
  const end = raw.indexOf("\n---", 4);
  if (end === -1) {
    throw new ProfileLoadError(
      "invalid_skill_frontmatter",
      `skill ${slug} has an unterminated frontmatter block`,
    );
  }
  const frontmatterRaw = raw.slice(4, end).trim();
  const bodyMarkdown = raw.slice(end + 4).trim();
  return {
    frontmatter: parseSimpleFrontmatter(frontmatterRaw, slug),
    bodyMarkdown,
  };
}

function parseSimpleFrontmatter(
  raw: string,
  slug: string,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  let currentListKey: string | undefined;
  for (const line of raw.split(/\r?\n/)) {
    if (line.trim() === "") {
      continue;
    }
    const listItem = line.match(/^\s*-\s+(.+)$/);
    if (listItem && currentListKey) {
      const existing = result[currentListKey];
      if (Array.isArray(existing)) {
        existing.push(unquote(listItem[1]!));
        continue;
      }
    }
    const match = line.match(/^([A-Za-z][A-Za-z0-9_-]*):(?:\s*(.*))?$/);
    if (!match) {
      throw new ProfileLoadError(
        "invalid_skill_frontmatter",
        `skill ${slug} has unsupported frontmatter line: ${line}`,
      );
    }
    const key = match[1]!;
    const value = match[2] ?? "";
    if (value.trim() === "") {
      result[key] = [];
      currentListKey = key;
    } else {
      result[key] = unquote(value.trim());
      currentListKey = undefined;
    }
  }
  return result;
}

function parseSimpleYaml(raw: string): Record<string, unknown> {
  const lines = raw.split(/\r?\n/);
  const root: Record<string, unknown> = {};
  const stack: Array<{
    indent: number;
    value: Record<string, unknown> | unknown[];
  }> = [{ indent: -1, value: root }];

  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index]!;
    if (!line.trim() || line.trimStart().startsWith("#")) {
      continue;
    }
    const indent = leadingSpaces(line);
    const trimmed = line.trim();
    while (stack.length > 1 && indent <= stack.at(-1)!.indent) {
      stack.pop();
    }
    const parent = stack.at(-1)!.value;
    if (trimmed.startsWith("- ")) {
      if (!Array.isArray(parent)) {
        throw new ProfileLoadError(
          "invalid_profile_yaml",
          `unsupported list item outside list: ${line}`,
        );
      }
      parent.push(parseYamlScalar(trimmed.slice(2).trim()));
      continue;
    }

    if (Array.isArray(parent)) {
      throw new ProfileLoadError(
        "invalid_profile_yaml",
        `unsupported mapping inside list: ${line}`,
      );
    }
    const match = trimmed.match(/^([A-Za-z][A-Za-z0-9_-]*):(?:\s*(.*))?$/);
    if (!match) {
      throw new ProfileLoadError(
        "invalid_profile_yaml",
        `unsupported profile YAML line: ${line}`,
      );
    }
    const key = match[1]!;
    const rest = match[2]?.trim() ?? "";
    if (rest) {
      parent[key] = parseYamlScalar(rest);
      continue;
    }

    const child: Record<string, unknown> | unknown[] =
      nextContentLine(lines, index + 1)
        ?.trim()
        .startsWith("- ") === true
        ? []
        : {};
    parent[key] = child;
    stack.push({ indent, value: child });
  }

  return root;
}

function parseYamlScalar(raw: string): unknown {
  if (raw === "[]") return [];
  if (raw === "true") return true;
  if (raw === "false") return false;
  if (/^-?\d+(?:\.\d+)?$/.test(raw)) return Number(raw);
  const quoted = raw.match(/^(['"])(.*)\1$/);
  return quoted ? quoted[2]! : raw;
}

function nextContentLine(
  lines: readonly string[],
  startIndex: number,
): string | undefined {
  for (let index = startIndex; index < lines.length; index += 1) {
    const line = lines[index]!;
    if (line.trim() && !line.trimStart().startsWith("#")) {
      return line;
    }
  }
  return undefined;
}

function leadingSpaces(line: string): number {
  return line.length - line.trimStart().length;
}

function invalidProfile(
  profileId: ProfileId,
  profilePath: string,
  message: string,
): ProfileLoadError {
  return new ProfileLoadError(
    "invalid_profile_config",
    `profile ${profileId} at ${profilePath}: ${message}`,
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isSafeSkillSlug(slug: string): boolean {
  return /^[A-Za-z0-9][A-Za-z0-9_-]*$/.test(slug);
}

function isNodeError(error: unknown): error is NodeJS.ErrnoException {
  return error instanceof Error && "code" in error;
}

function requiredString(value: unknown): string {
  if (typeof value !== "string" || value.trim() === "") {
    throw new ProfileLoadError(
      "invalid_profile_config",
      "expected a non-empty string",
    );
  }
  return value;
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() !== "" ? value : undefined;
}

function optionalNumber(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value)
    ? value
    : undefined;
}

function stringList(value: unknown): string[] {
  if (!Array.isArray(value)) {
    return [];
  }
  return value.filter(
    (item): item is string => typeof item === "string" && item.trim() !== "",
  );
}

function unquote(value: string): string {
  return value.replace(/^['"]|['"]$/g, "");
}
