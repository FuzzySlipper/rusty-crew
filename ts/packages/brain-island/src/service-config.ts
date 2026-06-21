import { closeSync, mkdirSync, openSync, rmSync, writeFileSync } from "node:fs";
import { isAbsolute, join, resolve } from "node:path";

export interface RustyCrewServiceEnv {
  RUSTY_CREW_DATA_DIR?: string;
  RUSTY_CREW_CONFIG_DIR?: string;
  RUSTY_CREW_ENGINE_DATA_DIR?: string;
  RUSTY_CREW_LOG_DIR?: string;
  RUSTY_CREW_RUN_DIR?: string;
  RUSTY_CREW_ARTIFACT_DIR?: string;
  RUSTY_CREW_BACKUP_DIR?: string;
  RUSTY_CREW_ADMIN_HOST?: string;
  RUSTY_CREW_ADMIN_PORT?: string;
  RUSTY_CREW_ADMIN_ALLOW_LAN?: string;
  RUSTY_CREW_ADMIN_TOKEN?: string;
}

export interface RustyCrewServicePaths {
  dataDir: string;
  configDir: string;
  serviceConfigFile: string;
  engineDataDir: string;
  logDir: string;
  runDir: string;
  artifactDir: string;
  backupDir: string;
  lockFile: string;
}

export interface RustyCrewAdminConfig {
  host: string;
  port: number;
  allowLan: boolean;
  token?: string;
}

export interface RustyCrewServiceConfig {
  paths: RustyCrewServicePaths;
  admin: RustyCrewAdminConfig;
}

export interface RustyCrewServiceLock {
  readonly lockFile: string;
  release(): void;
}

export const RUSTY_CREW_DEFAULT_DATA_DIR = "/home/agents/rusty-crew";
export const RUSTY_CREW_DEFAULT_ADMIN_HOST = "0.0.0.0";
export const RUSTY_CREW_DEFAULT_ADMIN_PORT = 9347;

export function loadRustyCrewServiceConfig(
  env: RustyCrewServiceEnv = process.env,
): RustyCrewServiceConfig {
  const dataDir = resolvePath(
    env.RUSTY_CREW_DATA_DIR,
    RUSTY_CREW_DEFAULT_DATA_DIR,
  );
  const paths: RustyCrewServicePaths = {
    dataDir,
    configDir: resolvePath(env.RUSTY_CREW_CONFIG_DIR, join(dataDir, "config")),
    serviceConfigFile: "",
    engineDataDir: resolvePath(
      env.RUSTY_CREW_ENGINE_DATA_DIR,
      join(dataDir, "data", "engine"),
    ),
    logDir: resolvePath(env.RUSTY_CREW_LOG_DIR, join(dataDir, "logs")),
    runDir: resolvePath(env.RUSTY_CREW_RUN_DIR, join(dataDir, "run")),
    artifactDir: resolvePath(
      env.RUSTY_CREW_ARTIFACT_DIR,
      join(dataDir, "artifacts"),
    ),
    backupDir: resolvePath(env.RUSTY_CREW_BACKUP_DIR, join(dataDir, "backups")),
    lockFile: "",
  };
  paths.serviceConfigFile = join(paths.configDir, "service.json");
  paths.lockFile = join(paths.runDir, "service.lock");

  const admin: RustyCrewAdminConfig = {
    host: normalizeHost(
      env.RUSTY_CREW_ADMIN_HOST ?? RUSTY_CREW_DEFAULT_ADMIN_HOST,
    ),
    port: parsePort(
      env.RUSTY_CREW_ADMIN_PORT,
      RUSTY_CREW_DEFAULT_ADMIN_PORT,
      "RUSTY_CREW_ADMIN_PORT",
    ),
    allowLan: parseBoolean(
      env.RUSTY_CREW_ADMIN_ALLOW_LAN,
      true,
      "RUSTY_CREW_ADMIN_ALLOW_LAN",
    ),
    token: normalizeOptional(env.RUSTY_CREW_ADMIN_TOKEN),
  };

  validateRustyCrewServiceConfig({ paths, admin });
  return { paths, admin };
}

export function validateRustyCrewServiceConfig(
  config: RustyCrewServiceConfig,
): void {
  for (const [name, path] of Object.entries(config.paths)) {
    if (!path.trim()) {
      throw new Error(`service path ${name} must not be empty`);
    }
    if (!isAbsolute(path)) {
      throw new Error(`service path ${name} must be absolute: ${path}`);
    }
  }

  if (!isLoopbackHost(config.admin.host) && !config.admin.allowLan) {
    throw new Error(
      "RUSTY_CREW_ADMIN_ALLOW_LAN must be true when binding admin HTTP to a non-loopback host",
    );
  }
}

export function ensureRustyCrewServiceDirectories(
  config: RustyCrewServiceConfig,
): void {
  for (const path of [
    config.paths.dataDir,
    config.paths.configDir,
    config.paths.engineDataDir,
    config.paths.logDir,
    config.paths.runDir,
    config.paths.artifactDir,
    config.paths.backupDir,
  ]) {
    mkdirSync(path, { recursive: true, mode: 0o750 });
  }
}

export function acquireRustyCrewServiceLock(
  config: RustyCrewServiceConfig,
): RustyCrewServiceLock {
  ensureRustyCrewServiceDirectories(config);
  let fd: number;
  try {
    fd = openSync(config.paths.lockFile, "wx", 0o640);
  } catch (error) {
    if (isNodeError(error) && error.code === "EEXIST") {
      throw new Error(
        `rusty-crew service lock already exists at ${config.paths.lockFile}`,
      );
    }
    throw error;
  }

  let released = false;
  writeFileSync(
    fd,
    JSON.stringify(
      {
        pid: process.pid,
        createdAt: new Date().toISOString(),
      },
      null,
      2,
    ),
  );
  closeSync(fd);

  return {
    lockFile: config.paths.lockFile,
    release() {
      if (released) return;
      released = true;
      rmSync(config.paths.lockFile, { force: true });
    },
  };
}

function resolvePath(input: string | undefined, fallback: string): string {
  const value = normalizeOptional(input) ?? fallback;
  return resolve(value);
}

function normalizeHost(input: string): string {
  const host = input.trim();
  if (!host) throw new Error("RUSTY_CREW_ADMIN_HOST must not be empty");
  return host;
}

function parsePort(
  input: string | undefined,
  fallback: number,
  name: string,
): number {
  const value = normalizeOptional(input);
  if (value === undefined) return fallback;
  if (!/^[0-9]+$/.test(value)) {
    throw new Error(`${name} must be an integer port`);
  }
  const port = Number(value);
  if (!Number.isSafeInteger(port) || port < 1 || port > 65535) {
    throw new Error(`${name} must be between 1 and 65535`);
  }
  return port;
}

function parseBoolean(
  input: string | undefined,
  fallback: boolean,
  name: string,
): boolean {
  const value = normalizeOptional(input);
  if (value === undefined) return fallback;
  switch (value.toLowerCase()) {
    case "1":
    case "true":
    case "yes":
    case "on":
      return true;
    case "0":
    case "false":
    case "no":
    case "off":
      return false;
    default:
      throw new Error(`${name} must be a boolean`);
  }
}

function normalizeOptional(input: string | undefined): string | undefined {
  const value = input?.trim();
  return value ? value : undefined;
}

function isLoopbackHost(host: string): boolean {
  return host === "127.0.0.1" || host === "::1" || host === "localhost";
}

function isNodeError(error: unknown): error is NodeJS.ErrnoException {
  return error instanceof Error && "code" in error;
}
