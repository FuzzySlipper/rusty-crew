import {
  mkdir,
  readFile,
  readdir,
  rename,
  stat,
  writeFile,
} from "node:fs/promises";
import { basename, dirname, join, resolve, sep } from "node:path";
import type {
  AgentTool as PiAgentTool,
  AgentToolResult,
} from "@earendil-works/pi-agent-core";
import { Type, type Static } from "typebox";
import {
  loadSkill,
  ProfileLoadError,
  type LoadedSkill,
} from "./profile-loading.js";
import type { PiAgentToolResolver } from "./tool-session-selection.js";

const skillSlugPattern = "^[A-Za-z0-9][A-Za-z0-9_-]*$";
const writableSkillSubdirs = [
  "references",
  "templates",
  "scripts",
  "assets",
] as const;

const listParameters = Type.Object({
  includeInvalid: Type.Optional(Type.Boolean()),
});

const viewParameters = Type.Object({
  slug: Type.String({ pattern: skillSlugPattern }),
  includeBody: Type.Optional(Type.Boolean()),
});

const manageParameters = Type.Object({
  action: Type.Union([
    Type.Literal("create"),
    Type.Literal("patch"),
    Type.Literal("write_file"),
    Type.Literal("delete"),
  ]),
  slug: Type.String({ pattern: skillSlugPattern }),
  content: Type.Optional(Type.String()),
  old_string: Type.Optional(Type.String()),
  new_string: Type.Optional(Type.String()),
  file_path: Type.Optional(Type.String()),
  file_content: Type.Optional(Type.String()),
  absorbed_into: Type.Optional(Type.String()),
  dryRun: Type.Optional(Type.Boolean()),
  provenance: Type.Optional(Type.String()),
});

type ListParams = Static<typeof listParameters>;
type ViewParams = Static<typeof viewParameters>;
type ManageParams = Static<typeof manageParameters>;

export type SkillManageMode = "off" | "profile" | "curator";
export type SkillManageAction = ManageParams["action"];

export interface SkillsToolContext {
  skillsDir?: string;
  allowedSkills?: readonly string[];
  maxBodyChars?: number;
  manageMode?: SkillManageMode;
  curatorApproved?: boolean;
  now?: () => Date;
}

export interface SkillListItem {
  slug: string;
  title?: string;
  summary?: string;
  tags: readonly string[];
  sourcePath: string;
  status: "available" | "invalid";
  error?: string;
}

export interface SkillsToolDetails {
  ok: boolean;
  operation: "list" | "view" | "manage";
  reasonCode?: string;
  skills?: readonly SkillListItem[];
  skill?: SkillListItem & {
    bodyMarkdown?: string;
    bodyChars?: number;
    truncated?: boolean;
  };
  management?: SkillManagementResult;
}

export interface SkillManagementResult {
  action: SkillManageAction;
  slug: string;
  dryRun: boolean;
  skillPath?: string;
  sidecarPath?: string;
  filePath?: string;
  archivePath?: string;
  sidecarArchivePath?: string;
  absorbedInto?: string;
  provenance?: string;
  changed?: boolean;
  oldStringMatches?: number;
}

export function createSkillsToolResolver(
  context: SkillsToolContext,
): PiAgentToolResolver {
  return () => resolveSkillsTools(context);
}

export function resolveSkillsTools(context: SkillsToolContext): PiAgentTool[] {
  const tools: PiAgentTool[] = [
    skillsListTool(context),
    skillViewTool(context),
  ];
  if (context.manageMode && context.manageMode !== "off") {
    tools.push(skillManageTool(context));
  }
  return tools;
}

export function skillsListTool(
  context: SkillsToolContext,
): PiAgentTool<typeof listParameters, SkillsToolDetails> {
  return {
    name: "skills_list",
    label: "List skills",
    description: "List configured skills visible to the current profile.",
    parameters: listParameters,
    execute: async (_toolCallId, params: ListParams) => {
      const root = await validateSkillsRoot(context, "list");
      if (!root.ok) return root.result;
      const slugs = await listSkillSlugs(root.skillsDir, context.allowedSkills);
      const skills = await Promise.all(
        slugs.map((slug) => loadSkillListItem(root.skillsDir, slug)),
      );
      const visible = params.includeInvalid
        ? skills
        : skills.filter((skill) => skill.status === "available");
      return result({
        ok: true,
        operation: "list",
        skills: visible,
      });
    },
  };
}

export function skillViewTool(
  context: SkillsToolContext,
): PiAgentTool<typeof viewParameters, SkillsToolDetails> {
  return {
    name: "skill_view",
    label: "View skill",
    description:
      "View one configured skill by safe slug without exposing unrelated files.",
    parameters: viewParameters,
    execute: async (_toolCallId, params: ViewParams) => {
      const root = await validateSkillsRoot(context, "view");
      if (!root.ok) return root.result;
      if (!isAllowedSkill(params.slug, context.allowedSkills)) {
        return result({
          ok: false,
          operation: "view",
          reasonCode: "skill_not_allowed",
        });
      }

      try {
        const loaded = await loadSkill(root.skillsDir, params.slug);
        const maxBodyChars = context.maxBodyChars ?? 64 * 1024;
        const includeBody = params.includeBody ?? true;
        const bodyMarkdown = includeBody
          ? loaded.bodyMarkdown.slice(0, maxBodyChars)
          : undefined;
        return result({
          ok: true,
          operation: "view",
          skill: {
            ...skillItem(loaded),
            bodyMarkdown,
            bodyChars: loaded.bodyMarkdown.length,
            truncated: includeBody && loaded.bodyMarkdown.length > maxBodyChars,
          },
        });
      } catch (error) {
        return result({
          ok: false,
          operation: "view",
          reasonCode:
            error instanceof ProfileLoadError
              ? error.code
              : "skill_view_failed",
        });
      }
    },
  };
}

export function skillManageTool(
  context: SkillsToolContext,
): PiAgentTool<typeof manageParameters, SkillsToolDetails> {
  return {
    name: "skill_manage",
    label: "Manage skill",
    description:
      "Create, patch, write sidecar files for, or archive configured skills with profile governance.",
    parameters: manageParameters,
    execute: async (_toolCallId, params: ManageParams) => {
      const root = await validateSkillsRoot(context, "manage");
      if (!root.ok) return root.result;
      const policy = validateManagePolicy(context);
      if (!policy.ok) {
        return manageResult(params, {
          ok: false,
          reasonCode: policy.reasonCode,
        });
      }
      if (!isAllowedSkill(params.slug, context.allowedSkills)) {
        return manageResult(params, {
          ok: false,
          reasonCode: "skill_not_allowed",
        });
      }

      try {
        switch (params.action) {
          case "create":
            return await handleManageCreate(root.skillsDir, params);
          case "patch":
            return await handleManagePatch(root.skillsDir, params);
          case "write_file":
            return await handleManageWriteFile(root.skillsDir, params);
          case "delete":
            return await handleManageDelete(root.skillsDir, params, context);
        }
      } catch (error) {
        return manageResult(params, {
          ok: false,
          reasonCode:
            error instanceof SkillManageError
              ? error.reasonCode
              : "skill_manage_failed",
        });
      }
    },
  };
}

async function validateSkillsRoot(
  context: SkillsToolContext,
  operation: SkillsToolDetails["operation"],
): Promise<
  | { ok: true; skillsDir: string }
  | { ok: false; result: AgentToolResult<SkillsToolDetails> }
> {
  if (!context.skillsDir) {
    return {
      ok: false,
      result: result({
        ok: false,
        operation,
        reasonCode: "skills_root_missing",
      }),
    };
  }
  try {
    const rootStat = await stat(context.skillsDir);
    if (!rootStat.isDirectory()) {
      return {
        ok: false,
        result: result({
          ok: false,
          operation,
          reasonCode: "skills_root_not_directory",
        }),
      };
    }
  } catch {
    return {
      ok: false,
      result: result({
        ok: false,
        operation,
        reasonCode: "skills_root_missing",
      }),
    };
  }
  return { ok: true, skillsDir: context.skillsDir };
}

async function listSkillSlugs(
  skillsDir: string,
  allowedSkills: readonly string[] | undefined,
): Promise<string[]> {
  if (allowedSkills) {
    return [...allowedSkills].filter(isSafeSkillSlug).sort();
  }
  const entries = await readdir(skillsDir, { withFileTypes: true });
  return entries
    .filter((entry) => entry.isFile() && entry.name.endsWith(".md"))
    .map((entry) => basename(entry.name, ".md"))
    .filter(isSafeSkillSlug)
    .sort();
}

async function loadSkillListItem(
  skillsDir: string,
  slug: string,
): Promise<SkillListItem> {
  try {
    return skillItem(await loadSkill(skillsDir, slug));
  } catch (error) {
    return {
      slug,
      tags: [],
      sourcePath: join(skillsDir, `${slug}.md`),
      status: "invalid",
      error:
        error instanceof Error ? error.message : "skill metadata is invalid",
    };
  }
}

function skillItem(skill: LoadedSkill): SkillListItem {
  return {
    slug: skill.slug,
    title: skill.title,
    summary: skill.summary,
    tags: skill.tags,
    sourcePath: skill.sourcePath,
    status: "available",
  };
}

function isAllowedSkill(
  slug: string,
  allowedSkills: readonly string[] | undefined,
): boolean {
  return (
    isSafeSkillSlug(slug) && (!allowedSkills || allowedSkills.includes(slug))
  );
}

function isSafeSkillSlug(slug: string): boolean {
  return new RegExp(skillSlugPattern).test(slug);
}

async function handleManageCreate(
  skillsDir: string,
  params: ManageParams,
): Promise<AgentToolResult<SkillsToolDetails>> {
  const content = requiredParam(params.content, "missing_content");
  validateSkillMarkdown(content, params.slug);
  const skillPath = skillMarkdownPath(skillsDir, params.slug);
  if (await pathExists(skillPath)) {
    return manageResult(params, {
      ok: false,
      reasonCode: "skill_already_exists",
      management: { skillPath },
    });
  }
  if (params.dryRun) {
    return manageResult(params, {
      ok: true,
      management: { skillPath, changed: false },
    });
  }
  await mkdir(dirname(skillPath), { recursive: true });
  await writeFile(skillPath, content, "utf8");
  return manageResult(params, {
    ok: true,
    management: { skillPath, changed: true },
  });
}

async function handleManagePatch(
  skillsDir: string,
  params: ManageParams,
): Promise<AgentToolResult<SkillsToolDetails>> {
  const skillPath = skillMarkdownPath(skillsDir, params.slug);
  const existing = await readExistingSkill(skillPath);
  const fullContent = params.content;
  let nextContent: string;
  let oldStringMatches: number | undefined;

  if (fullContent !== undefined) {
    validateSkillMarkdown(fullContent, params.slug);
    nextContent = fullContent;
  } else {
    const oldString = requiredParam(params.old_string, "missing_old_string");
    const newString = params.new_string ?? "";
    oldStringMatches = countOccurrences(existing, oldString);
    if (oldStringMatches === 0) {
      return manageResult(params, {
        ok: false,
        reasonCode: "old_string_not_found",
        management: { skillPath, oldStringMatches },
      });
    }
    if (oldStringMatches > 1) {
      return manageResult(params, {
        ok: false,
        reasonCode: "old_string_not_unique",
        management: { skillPath, oldStringMatches },
      });
    }
    nextContent = existing.replace(oldString, newString);
    validateSkillMarkdown(nextContent, params.slug);
  }

  const changed = nextContent !== existing;
  if (params.dryRun || !changed) {
    return manageResult(params, {
      ok: true,
      management: { skillPath, changed: false, oldStringMatches },
    });
  }
  await writeFile(skillPath, nextContent, "utf8");
  return manageResult(params, {
    ok: true,
    management: { skillPath, changed: true, oldStringMatches },
  });
}

async function handleManageWriteFile(
  skillsDir: string,
  params: ManageParams,
): Promise<AgentToolResult<SkillsToolDetails>> {
  const skillPath = skillMarkdownPath(skillsDir, params.slug);
  await readExistingSkill(skillPath);
  const filePath = requiredParam(params.file_path, "missing_file_path");
  const fileContent = requiredParam(
    params.file_content,
    "missing_file_content",
  );
  const sidecarPath = skillSidecarPath(skillsDir, params.slug);
  const targetPath = safeSidecarFilePath(sidecarPath, filePath);
  if (params.dryRun) {
    return manageResult(params, {
      ok: true,
      management: {
        skillPath,
        sidecarPath,
        filePath: targetPath,
        changed: false,
      },
    });
  }
  await mkdir(dirname(targetPath), { recursive: true });
  await writeFile(targetPath, fileContent, "utf8");
  return manageResult(params, {
    ok: true,
    management: { skillPath, sidecarPath, filePath: targetPath, changed: true },
  });
}

async function handleManageDelete(
  skillsDir: string,
  params: ManageParams,
  context: SkillsToolContext,
): Promise<AgentToolResult<SkillsToolDetails>> {
  if (params.absorbed_into === undefined) {
    return manageResult(params, {
      ok: false,
      reasonCode: "missing_absorbed_into",
    });
  }
  const skillPath = skillMarkdownPath(skillsDir, params.slug);
  await readExistingSkill(skillPath);
  const pinned = await findPinnedMarker(skillsDir, params.slug);
  if (pinned) {
    return manageResult(params, {
      ok: false,
      reasonCode: "skill_pinned",
      management: { skillPath, filePath: pinned },
    });
  }

  const archive = archivePaths(
    skillsDir,
    params.slug,
    context.now?.() ?? new Date(),
  );
  const sidecarPath = skillSidecarPath(skillsDir, params.slug);
  const sidecarExists = await pathExists(sidecarPath);
  if (params.dryRun) {
    return manageResult(params, {
      ok: true,
      management: {
        skillPath,
        sidecarPath: sidecarExists ? sidecarPath : undefined,
        archivePath: archive.skillArchivePath,
        sidecarArchivePath: sidecarExists
          ? archive.sidecarArchivePath
          : undefined,
        absorbedInto: params.absorbed_into || undefined,
        changed: false,
      },
    });
  }

  await mkdir(archive.archiveDir, { recursive: true });
  await rename(skillPath, archive.skillArchivePath);
  if (sidecarExists) {
    await rename(sidecarPath, archive.sidecarArchivePath);
  }
  await writeFile(
    archive.manifestPath,
    `${JSON.stringify(
      {
        slug: params.slug,
        action: "delete",
        archivedAt: archive.timestamp,
        absorbed_into: params.absorbed_into,
        provenance: params.provenance,
        skillArchivePath: archive.skillArchivePath,
        sidecarArchivePath: sidecarExists
          ? archive.sidecarArchivePath
          : undefined,
      },
      null,
      2,
    )}\n`,
    "utf8",
  );
  return manageResult(params, {
    ok: true,
    management: {
      skillPath,
      sidecarPath: sidecarExists ? sidecarPath : undefined,
      archivePath: archive.skillArchivePath,
      sidecarArchivePath: sidecarExists
        ? archive.sidecarArchivePath
        : undefined,
      absorbedInto: params.absorbed_into || undefined,
      changed: true,
    },
  });
}

function validateManagePolicy(
  context: SkillsToolContext,
): { ok: true } | { ok: false; reasonCode: string } {
  const mode = context.manageMode ?? "off";
  if (mode === "off") {
    return { ok: false, reasonCode: "skill_manage_disabled" };
  }
  if (mode === "curator" && context.curatorApproved !== true) {
    return { ok: false, reasonCode: "skill_manage_requires_curator" };
  }
  return { ok: true };
}

function skillMarkdownPath(skillsDir: string, slug: string): string {
  return join(skillsDir, `${slug}.md`);
}

function skillSidecarPath(skillsDir: string, slug: string): string {
  return join(skillsDir, `${slug}.d`);
}

function safeSidecarFilePath(
  sidecarPath: string,
  relativePath: string,
): string {
  const normalized = relativePath.replace(/\\/g, "/");
  const topLevel = normalized.split("/")[0];
  if (
    normalized.startsWith("/") ||
    normalized.includes("..") ||
    !writableSkillSubdirs.includes(
      topLevel as (typeof writableSkillSubdirs)[number],
    )
  ) {
    throw new SkillManageError("invalid_file_path");
  }
  const root = resolve(sidecarPath);
  const target = resolve(root, normalized);
  if (target !== root && target.startsWith(`${root}${sep}`)) {
    return target;
  }
  throw new SkillManageError("invalid_file_path");
}

async function readExistingSkill(skillPath: string): Promise<string> {
  try {
    return await readFile(skillPath, "utf8");
  } catch {
    throw new SkillManageError("skill_not_found");
  }
}

async function pathExists(path: string): Promise<boolean> {
  try {
    await stat(path);
    return true;
  } catch {
    return false;
  }
}

async function findPinnedMarker(
  skillsDir: string,
  slug: string,
): Promise<string | undefined> {
  const candidates = [
    join(skillsDir, `${slug}.pinned`),
    join(skillSidecarPath(skillsDir, slug), ".pinned"),
    join(skillsDir, slug, ".pinned"),
  ];
  for (const candidate of candidates) {
    if (await pathExists(candidate)) return candidate;
  }
  return undefined;
}

function archivePaths(
  skillsDir: string,
  slug: string,
  now: Date,
): {
  archiveDir: string;
  timestamp: string;
  skillArchivePath: string;
  sidecarArchivePath: string;
  manifestPath: string;
} {
  const timestamp = now.toISOString().replace(/[:.]/g, "-");
  const archiveDir = join(skillsDir, ".archive");
  const prefix = `${slug}-${timestamp}`;
  return {
    archiveDir,
    timestamp,
    skillArchivePath: join(archiveDir, `${prefix}.md`),
    sidecarArchivePath: join(archiveDir, `${prefix}.d`),
    manifestPath: join(archiveDir, `${prefix}.delete.json`),
  };
}

function validateSkillMarkdown(content: string, slug: string): void {
  if (!content.startsWith("---\n")) return;
  const end = content.indexOf("\n---", 4);
  if (end === -1) {
    throw new SkillManageError("invalid_skill_frontmatter");
  }
  const frontmatter = content.slice(4, end).trim();
  for (const line of frontmatter.split(/\r?\n/)) {
    if (line.trim() === "" || /^\s*-\s+.+$/.test(line)) {
      continue;
    }
    if (!/^([A-Za-z][A-Za-z0-9_-]*):(?:\s*(.*))?$/.test(line)) {
      throw new SkillManageError("invalid_skill_frontmatter");
    }
  }
  if (!isSafeSkillSlug(slug)) {
    throw new SkillManageError("invalid_skill_slug");
  }
}

function requiredParam(value: string | undefined, reasonCode: string): string {
  if (value === undefined || value.length === 0) {
    throw new SkillManageError(reasonCode);
  }
  return value;
}

function countOccurrences(haystack: string, needle: string): number {
  if (needle.length === 0) return 0;
  return haystack.split(needle).length - 1;
}

class SkillManageError extends Error {
  constructor(readonly reasonCode: string) {
    super(reasonCode);
    this.name = "SkillManageError";
  }
}

function manageResult(
  params: Pick<ManageParams, "action" | "slug" | "dryRun" | "provenance">,
  details: {
    ok: boolean;
    reasonCode?: string;
    management?: Partial<SkillManagementResult>;
  },
): AgentToolResult<SkillsToolDetails> {
  return result({
    ok: details.ok,
    operation: "manage",
    reasonCode: details.reasonCode,
    management: {
      action: params.action,
      slug: params.slug,
      dryRun: params.dryRun ?? false,
      provenance: params.provenance,
      ...details.management,
    },
  });
}

function result(
  details: SkillsToolDetails,
): AgentToolResult<SkillsToolDetails> {
  return {
    content: [{ type: "text", text: JSON.stringify(details, null, 2) }],
    details,
  };
}
