import {
  mkdir,
  readdir,
  readFile,
  rename,
  rm,
  stat,
  writeFile,
} from "node:fs/promises";
import { basename, dirname, join, resolve, sep } from "node:path";

export interface CuratorPinnedSkill {
  slug: string;
  markerPath: string;
}

export interface CuratorArchivedSkill {
  slug: string;
  archivedAt?: string;
  absorbedInto?: string;
  manifestPath: string;
  skillArchivePath: string;
  sidecarArchivePath?: string;
}

export interface CuratorSkillPinResult {
  slug: string;
  markerPath: string;
  changed: boolean;
}

export interface CuratorSkillUnpinResult {
  slug: string;
  removed: readonly string[];
}

export interface CuratorSkillRestoreResult {
  slug: string;
  skillPath: string;
  sidecarPath?: string;
  manifestPath: string;
}

export async function pinCuratorSkill(input: {
  skillsDir: string;
  slug: string;
  reason?: string;
  operatorId?: string;
  now?: string;
}): Promise<CuratorSkillPinResult> {
  await requireSkill(input.skillsDir, input.slug);
  const markerPath = pinnedMarkerPath(input.skillsDir, input.slug);
  const existed = await pathExists(markerPath);
  await mkdir(dirname(markerPath), { recursive: true });
  await writeFile(
    markerPath,
    [
      `slug: ${input.slug}`,
      `pinnedAt: ${input.now ?? new Date().toISOString()}`,
      input.operatorId ? `operatorId: ${input.operatorId}` : undefined,
      input.reason ? `reason: ${input.reason.replace(/\n/g, " ")}` : undefined,
      "",
    ]
      .filter((line): line is string => line !== undefined)
      .join("\n"),
    "utf8",
  );
  return { slug: input.slug, markerPath, changed: !existed };
}

export async function unpinCuratorSkill(input: {
  skillsDir: string;
  slug: string;
}): Promise<CuratorSkillUnpinResult> {
  const removed: string[] = [];
  for (const markerPath of pinnedMarkerPaths(input.skillsDir, input.slug)) {
    if (!(await pathExists(markerPath))) continue;
    await rm(markerPath, { force: true });
    removed.push(markerPath);
  }
  return { slug: input.slug, removed };
}

export async function listCuratorPinnedSkills(
  skillsDir: string,
): Promise<CuratorPinnedSkill[]> {
  const entries = await safeReadDir(skillsDir);
  const pinned: CuratorPinnedSkill[] = [];
  for (const entry of entries) {
    if (entry.isFile() && entry.name.endsWith(".pinned")) {
      pinned.push({
        slug: entry.name.slice(0, -".pinned".length),
        markerPath: join(skillsDir, entry.name),
      });
    }
    if (entry.isDirectory() && entry.name.endsWith(".d")) {
      const markerPath = join(skillsDir, entry.name, ".pinned");
      if (await pathExists(markerPath)) {
        pinned.push({
          slug: entry.name.slice(0, -".d".length),
          markerPath,
        });
      }
    }
  }
  return pinned.sort((left, right) => left.slug.localeCompare(right.slug));
}

export async function listCuratorArchivedSkills(
  skillsDir: string,
): Promise<CuratorArchivedSkill[]> {
  const archiveDir = archiveRoot(skillsDir);
  const entries = await safeReadDir(archiveDir);
  const archived: CuratorArchivedSkill[] = [];
  for (const entry of entries) {
    if (!entry.isFile() || !entry.name.endsWith(".delete.json")) continue;
    const manifestPath = join(archiveDir, entry.name);
    const manifest = parseArchiveManifest(await readFile(manifestPath, "utf8"));
    if (!manifest) continue;
    archived.push({ ...manifest, manifestPath });
  }
  return archived.sort((left, right) =>
    (right.archivedAt ?? "").localeCompare(left.archivedAt ?? ""),
  );
}

export async function restoreCuratorArchivedSkill(input: {
  skillsDir: string;
  slug: string;
  manifestPath?: string;
}): Promise<CuratorSkillRestoreResult> {
  const archived = input.manifestPath
    ? await archivedSkillFromManifest(input.skillsDir, input.manifestPath)
    : await latestArchivedSkill(input.skillsDir, input.slug);
  if (!archived || archived.slug !== input.slug) {
    throw new Error("curator_archive_not_found");
  }
  const skillPath = skillPathFor(input.skillsDir, input.slug);
  if (await pathExists(skillPath)) {
    throw new Error("curator_restore_target_exists");
  }
  const skillArchivePath = safeArchivePath(
    input.skillsDir,
    archived.skillArchivePath,
  );
  await mkdir(dirname(skillPath), { recursive: true });
  await rename(skillArchivePath, skillPath);

  let sidecarPath: string | undefined;
  if (
    archived.sidecarArchivePath &&
    (await pathExists(
      safeArchivePath(input.skillsDir, archived.sidecarArchivePath),
    ))
  ) {
    sidecarPath = join(input.skillsDir, `${input.slug}.d`);
    if (await pathExists(sidecarPath)) {
      throw new Error("curator_restore_sidecar_target_exists");
    }
    await rename(
      safeArchivePath(input.skillsDir, archived.sidecarArchivePath),
      sidecarPath,
    );
  }
  return {
    slug: input.slug,
    skillPath,
    sidecarPath,
    manifestPath: archived.manifestPath,
  };
}

async function latestArchivedSkill(
  skillsDir: string,
  slug: string,
): Promise<CuratorArchivedSkill | undefined> {
  const archived = await listCuratorArchivedSkills(skillsDir);
  return archived.find((item) => item.slug === slug);
}

async function archivedSkillFromManifest(
  skillsDir: string,
  manifestPath: string,
): Promise<CuratorArchivedSkill | undefined> {
  const safeManifestPath = safeArchivePath(skillsDir, manifestPath);
  const manifest = parseArchiveManifest(
    await readFile(safeManifestPath, "utf8"),
  );
  return manifest ? { ...manifest, manifestPath: safeManifestPath } : undefined;
}

function parseArchiveManifest(
  content: string,
): Omit<CuratorArchivedSkill, "manifestPath"> | undefined {
  const parsed = JSON.parse(content) as Record<string, unknown>;
  const slug = stringValue(parsed.slug);
  const skillArchivePath = stringValue(parsed.skillArchivePath);
  if (!slug || !skillArchivePath) return undefined;
  return {
    slug,
    archivedAt: stringValue(parsed.archivedAt),
    absorbedInto: stringValue(parsed.absorbed_into),
    skillArchivePath,
    sidecarArchivePath: stringValue(parsed.sidecarArchivePath),
  };
}

async function requireSkill(skillsDir: string, slug: string): Promise<void> {
  await stat(skillPathFor(skillsDir, slug));
}

function skillPathFor(skillsDir: string, slug: string): string {
  return join(skillsDir, `${slug}.md`);
}

function pinnedMarkerPath(skillsDir: string, slug: string): string {
  return join(skillsDir, `${slug}.pinned`);
}

function pinnedMarkerPaths(skillsDir: string, slug: string): string[] {
  return [
    pinnedMarkerPath(skillsDir, slug),
    join(skillsDir, `${slug}.d`, ".pinned"),
    join(skillsDir, slug, ".pinned"),
  ];
}

function archiveRoot(skillsDir: string): string {
  return join(skillsDir, ".archive");
}

function safeArchivePath(skillsDir: string, path: string): string {
  const root = resolve(archiveRoot(skillsDir));
  const target = resolve(path);
  if (target === root || target.startsWith(`${root}${sep}`)) return target;
  throw new Error("curator_archive_path_outside_root");
}

async function safeReadDir(path: string) {
  try {
    return await readdir(path, { withFileTypes: true });
  } catch {
    return [];
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

function stringValue(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value : undefined;
}
