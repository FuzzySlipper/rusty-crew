import { readFile } from "node:fs/promises";
import { join } from "node:path";
import type {
  BrainModelConfig,
  ProfileId,
  ResourceLimits,
} from "@rusty-crew/contracts";
import {
  selectToolProfile,
  type ProfileToolPolicy,
  type SessionToolConstraints,
  type ToolProfileSelection,
} from "./tool-profile-selection.js";
import { defaultToolRegistry, type ToolRegistry } from "./tool-registry.js";

export type ProfileLoadErrorCode =
  | "profile_not_found"
  | "invalid_profile_json"
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
}

export interface ProfilePromptFragments {
  system?: string;
  instructions?: string[];
}

export interface ProfileConfig {
  profileId: ProfileId;
  displayName?: string;
  modelConfig: BrainModelConfig;
  runtime?: ProfileRuntimeConfig;
  toolPolicy?: ProfileToolPolicy;
  prompt?: ProfilePromptFragments;
  skills?: string[];
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

export interface LoadProfileContextInput {
  profilesDir: string;
  skillsDir?: string;
  profileId: ProfileId;
  registry?: ToolRegistry;
  session?: SessionToolConstraints;
  catalogId?: string;
}

export async function loadProfileContext(
  input: LoadProfileContextInput,
): Promise<LoadedProfileContext> {
  const registry = input.registry ?? defaultToolRegistry;
  const profile = await loadProfileConfig(input.profilesDir, input.profileId);
  const skills = await Promise.all(
    (profile.skills ?? []).map((slug) =>
      loadSkill(input.skillsDir ?? join(input.profilesDir, "skills"), slug),
    ),
  );
  const toolSelection = selectToolProfile({
    profileId: profile.profileId,
    policy: profile.toolPolicy ?? {},
    session: input.session,
    registry,
    catalogId: input.catalogId,
  });

  return {
    profile,
    skills,
    toolSelection,
  };
}

export async function loadProfileConfig(
  profilesDir: string,
  profileId: ProfileId,
): Promise<ProfileConfig> {
  const profilePath = join(profilesDir, `${profileId}.json`);
  let raw: string;
  try {
    raw = await readFile(profilePath, "utf8");
  } catch (error) {
    throw new ProfileLoadError(
      "profile_not_found",
      `profile ${profileId} was not found at ${profilePath}`,
      error,
    );
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

  return validateProfileConfig(parsed, profileId, profilePath);
}

export async function loadSkill(
  skillsDir: string,
  slug: string,
): Promise<LoadedSkill> {
  const sourcePath = join(skillsDir, `${slug}.md`);
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
    slug,
    title: optionalString(frontmatter.title),
    summary: optionalString(frontmatter.summary),
    tags: stringList(frontmatter.tags),
    bodyMarkdown,
    sourcePath,
  };
}

function validateProfileConfig(
  parsed: unknown,
  profileId: ProfileId,
  profilePath: string,
): ProfileConfig {
  if (!isRecord(parsed)) {
    throw invalidProfile(
      profileId,
      profilePath,
      "profile root must be an object",
    );
  }
  const rawProfileId = optionalString(parsed.profileId) ?? profileId;
  if (rawProfileId !== profileId) {
    throw invalidProfile(
      profileId,
      profilePath,
      `profileId ${rawProfileId} does not match requested ${profileId}`,
    );
  }
  const modelConfig = parsed.modelConfig;
  if (!isRecord(modelConfig)) {
    throw invalidProfile(profileId, profilePath, "modelConfig is required");
  }
  const provider = requiredString(modelConfig.provider);
  const modelName = requiredString(modelConfig.modelName);

  return {
    profileId,
    displayName: optionalString(parsed.displayName),
    modelConfig: {
      provider,
      modelName,
      temperatureMilli: optionalNumber(modelConfig.temperatureMilli),
      maxOutputTokens: optionalNumber(modelConfig.maxOutputTokens),
    },
    runtime: isRecord(parsed.runtime)
      ? {
          maxTurns: optionalNumber(parsed.runtime.maxTurns),
          defaultResourceLimits: isRecord(parsed.runtime.defaultResourceLimits)
            ? {
                workdir: optionalString(
                  parsed.runtime.defaultResourceLimits.workdir,
                ),
                maxDurationMs: optionalNumber(
                  parsed.runtime.defaultResourceLimits.maxDurationMs,
                ),
                maxDelegationDepth: optionalNumber(
                  parsed.runtime.defaultResourceLimits.maxDelegationDepth,
                ),
              }
            : undefined,
        }
      : undefined,
    toolPolicy: isRecord(parsed.toolPolicy)
      ? {
          requestedToolsets: stringList(parsed.toolPolicy.requestedToolsets),
          requestedTools: stringList(parsed.toolPolicy.requestedTools),
          deniedTools: stringList(parsed.toolPolicy.deniedTools),
          includeDeprecated:
            typeof parsed.toolPolicy.includeDeprecated === "boolean"
              ? parsed.toolPolicy.includeDeprecated
              : undefined,
        }
      : undefined,
    prompt: isRecord(parsed.prompt)
      ? {
          system: optionalString(parsed.prompt.system),
          instructions: stringList(parsed.prompt.instructions),
        }
      : undefined,
    skills: stringList(parsed.skills),
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
