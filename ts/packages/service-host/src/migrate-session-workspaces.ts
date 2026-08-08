import { constants } from "node:fs";
import {
  chmod,
  copyFile,
  readFile,
  rename,
  stat,
  writeFile,
} from "node:fs/promises";
import { isAbsolute, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export interface SessionWorkspaceMigrationResult {
  readonly config: Record<string, unknown>;
  readonly migratedSessionIds: readonly string[];
}

export function migrateLegacyFullSessionWorkspaces(
  input: unknown,
  explicitWorkspaceCwd: string,
): SessionWorkspaceMigrationResult {
  if (explicitWorkspaceCwd.trim() === "" || !isAbsolute(explicitWorkspaceCwd)) {
    throw new Error("--workspace-cwd must be an explicit absolute path");
  }
  if (!isRecord(input))
    throw new Error("service config root must be an object");
  const sessions = input.sessions;
  if (!Array.isArray(sessions))
    throw new Error("service config sessions must be an array");

  const config = structuredClone(input);
  const clonedSessions = config.sessions as unknown[];
  const migratedSessionIds: string[] = [];
  for (const [index, candidate] of clonedSessions.entries()) {
    if (!isRecord(candidate))
      throw new Error(`sessions[${index}] must be an object`);
    if (candidate.kind !== "full") continue;
    const sessionId = candidate.sessionId;
    if (typeof sessionId !== "string" || sessionId.length === 0) {
      throw new Error(
        `sessions[${index}].sessionId must be a non-empty string`,
      );
    }
    let changed = false;
    if (candidate.workspaceCwd === undefined) {
      candidate.workspaceCwd = explicitWorkspaceCwd;
      changed = true;
    }
    if (
      isRecord(candidate.resourceLimits) &&
      candidate.resourceLimits.workdir !== undefined
    ) {
      delete candidate.resourceLimits.workdir;
      changed = true;
    }
    if (changed) migratedSessionIds.push(sessionId);
  }
  return { config, migratedSessionIds };
}

async function main(argv: readonly string[]): Promise<void> {
  const configPath = requiredArgument(argv, "--config");
  const workspaceCwd = requiredArgument(argv, "--workspace-cwd");
  const write = argv.includes("--write");
  const parsed = JSON.parse(await readFile(configPath, "utf8")) as unknown;
  const result = migrateLegacyFullSessionWorkspaces(parsed, workspaceCwd);
  if (!write) {
    console.log(
      JSON.stringify({
        mode: "dry_run",
        configPath,
        workspaceCwd,
        migratedSessionCount: result.migratedSessionIds.length,
        migratedSessionIds: result.migratedSessionIds,
      }),
    );
    return;
  }

  const backupPath =
    optionalArgument(argv, "--backup") ??
    `${configPath}.pre-workspace-${new Date().toISOString().replaceAll(/[:.]/g, "-")}`;
  await copyFile(configPath, backupPath, constants.COPYFILE_EXCL);
  const sourceStat = await stat(configPath);
  const temporaryPath = `${configPath}.workspace-migration-${process.pid}`;
  await writeFile(
    temporaryPath,
    `${JSON.stringify(result.config, null, 2)}\n`,
    {
      flag: "wx",
      mode: sourceStat.mode,
    },
  );
  await chmod(temporaryPath, sourceStat.mode);
  await rename(temporaryPath, configPath);
  console.log(
    JSON.stringify({
      mode: "written",
      configPath,
      backupPath,
      workspaceCwd,
      migratedSessionCount: result.migratedSessionIds.length,
      migratedSessionIds: result.migratedSessionIds,
    }),
  );
}

function requiredArgument(argv: readonly string[], name: string): string {
  const value = optionalArgument(argv, name);
  if (value === undefined) throw new Error(`${name} is required`);
  return value;
}

function optionalArgument(
  argv: readonly string[],
  name: string,
): string | undefined {
  const index = argv.indexOf(name);
  if (index < 0) return undefined;
  const value = argv[index + 1];
  if (value === undefined || value.startsWith("--"))
    throw new Error(`${name} requires a value`);
  return value;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

if (
  process.argv[1] !== undefined &&
  fileURLToPath(import.meta.url) === resolve(process.argv[1])
) {
  await main(process.argv.slice(2));
}
