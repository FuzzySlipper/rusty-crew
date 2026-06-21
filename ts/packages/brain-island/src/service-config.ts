import { closeSync, mkdirSync, openSync, rmSync, writeFileSync } from "node:fs";
import { isAbsolute, join, resolve } from "node:path";
import {
  loadDenSuccessorGatewayConfig,
  type DenSuccessorGatewayConfig,
  type DenSuccessorGatewayEnv,
  type DenMemoryClientPaths,
} from "@rusty-crew/adapter-den";

export interface RustyCrewServiceEnv extends DenSuccessorGatewayEnv {
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
  RUSTY_CREW_ADMIN_AUTH_MODE?: string;
  RUSTY_CREW_ADMIN_TOKEN?: string;
  RUSTY_CREW_SCHEDULER_TICK_INTERVAL_MS?: string;
  RUSTY_CREW_WAKE_DISPATCH_INTERVAL_MS?: string;
  RUSTY_CREW_DEN_RUNTIME_HEARTBEAT_INTERVAL_MS?: string;
  RUSTY_CREW_DEN_DELIVERY_POLL_INTERVAL_MS?: string;
  RUSTY_CREW_DEN_CONVERSATION_PROJECT_ID?: string;
  RUSTY_CREW_DEN_MEMORY_BASE_URL?: string;
  RUSTY_CREW_DEN_MEMORY_TOKEN?: string;
  RUSTY_CREW_DEN_MEMORY_BEARER_TOKEN?: string;
  RUSTY_CREW_DEN_MEMORY_TIMEOUT_MS?: string;
  RUSTY_CREW_DEN_MEMORY_READ_PATH?: string;
  RUSTY_CREW_DEN_MEMORY_SEARCH_PATH?: string;
  RUSTY_CREW_DEN_MEMORY_RECALL_PATH?: string;
  RUSTY_CREW_DEN_MEMORY_STORE_PATH?: string;
  RUSTY_CREW_DEN_MEMORY_PROPOSE_PATH?: string;
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
  authMode: "bearer" | "none";
  token?: string;
}

export interface RustyCrewBackgroundConfig {
  schedulerTickIntervalMs: number;
  wakeDispatchIntervalMs: number;
  denRuntimeHeartbeatIntervalMs: number;
  denDeliveryPollIntervalMs: number;
}

export interface RustyCrewDenMemoryConfig {
  baseUrl?: string;
  bearerToken?: string;
  timeoutMs: number;
  paths: Partial<DenMemoryClientPaths>;
}

export interface RustyCrewServiceConfig {
  paths: RustyCrewServicePaths;
  admin: RustyCrewAdminConfig;
  background: RustyCrewBackgroundConfig;
  denConversationProjectId: string;
  denMemory: RustyCrewDenMemoryConfig;
  denSuccessorGateway?: DenSuccessorGatewayConfig;
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
    authMode: parseAuthMode(env.RUSTY_CREW_ADMIN_AUTH_MODE),
    token: normalizeOptional(env.RUSTY_CREW_ADMIN_TOKEN),
  };
  const background: RustyCrewBackgroundConfig = {
    schedulerTickIntervalMs: parseNonNegativeInteger(
      env.RUSTY_CREW_SCHEDULER_TICK_INTERVAL_MS,
      1_000,
      "RUSTY_CREW_SCHEDULER_TICK_INTERVAL_MS",
    ),
    wakeDispatchIntervalMs: parseNonNegativeInteger(
      env.RUSTY_CREW_WAKE_DISPATCH_INTERVAL_MS,
      250,
      "RUSTY_CREW_WAKE_DISPATCH_INTERVAL_MS",
    ),
    denRuntimeHeartbeatIntervalMs: parseNonNegativeInteger(
      env.RUSTY_CREW_DEN_RUNTIME_HEARTBEAT_INTERVAL_MS,
      30_000,
      "RUSTY_CREW_DEN_RUNTIME_HEARTBEAT_INTERVAL_MS",
    ),
    denDeliveryPollIntervalMs: parseNonNegativeInteger(
      env.RUSTY_CREW_DEN_DELIVERY_POLL_INTERVAL_MS,
      2_000,
      "RUSTY_CREW_DEN_DELIVERY_POLL_INTERVAL_MS",
    ),
  };

  const denSuccessorGateway = loadDenSuccessorGatewayConfig(env);
  const denConversationProjectId =
    normalizeOptional(env.RUSTY_CREW_DEN_CONVERSATION_PROJECT_ID) ??
    "rusty-crew";
  const denMemory = loadRustyCrewDenMemoryConfig(env);

  validateRustyCrewServiceConfig({
    paths,
    admin,
    background,
    denConversationProjectId,
    denMemory,
    denSuccessorGateway,
  });
  return {
    paths,
    admin,
    background,
    denConversationProjectId,
    denMemory,
    denSuccessorGateway,
  };
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

  if (config.admin.authMode === "bearer" && !config.admin.token) {
    throw new Error(
      "RUSTY_CREW_ADMIN_TOKEN is required when RUSTY_CREW_ADMIN_AUTH_MODE=bearer",
    );
  }

  validateDenMemoryConfig(config.denMemory);
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

function parseNonNegativeInteger(
  input: string | undefined,
  fallback: number,
  name: string,
): number {
  const value = normalizeOptional(input);
  if (value === undefined) return fallback;
  if (!/^[0-9]+$/.test(value)) {
    throw new Error(`${name} must be an integer`);
  }
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < 0) {
    throw new Error(`${name} must be a non-negative integer`);
  }
  return parsed;
}

function parsePositiveInteger(
  input: string | undefined,
  fallback: number,
  name: string,
): number {
  const parsed = parseNonNegativeInteger(input, fallback, name);
  if (parsed < 1) {
    throw new Error(`${name} must be a positive integer`);
  }
  return parsed;
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

function parseAuthMode(
  input: string | undefined,
): RustyCrewAdminConfig["authMode"] {
  const value = normalizeOptional(input);
  if (value === undefined) return "bearer";
  switch (value.toLowerCase()) {
    case "bearer":
      return "bearer";
    case "none":
      return "none";
    default:
      throw new Error("RUSTY_CREW_ADMIN_AUTH_MODE must be bearer or none");
  }
}

function normalizeOptional(input: string | undefined): string | undefined {
  const value = input?.trim();
  return value ? value : undefined;
}

function loadRustyCrewDenMemoryConfig(
  env: RustyCrewServiceEnv,
): RustyCrewDenMemoryConfig {
  return {
    baseUrl: normalizeOptional(env.RUSTY_CREW_DEN_MEMORY_BASE_URL),
    bearerToken:
      normalizeOptional(env.RUSTY_CREW_DEN_MEMORY_BEARER_TOKEN) ??
      normalizeOptional(env.RUSTY_CREW_DEN_MEMORY_TOKEN),
    timeoutMs: parsePositiveInteger(
      env.RUSTY_CREW_DEN_MEMORY_TIMEOUT_MS,
      5_000,
      "RUSTY_CREW_DEN_MEMORY_TIMEOUT_MS",
    ),
    paths: {
      read: normalizeOptional(env.RUSTY_CREW_DEN_MEMORY_READ_PATH),
      search: normalizeOptional(env.RUSTY_CREW_DEN_MEMORY_SEARCH_PATH),
      recall: normalizeOptional(env.RUSTY_CREW_DEN_MEMORY_RECALL_PATH),
      store: normalizeOptional(env.RUSTY_CREW_DEN_MEMORY_STORE_PATH),
      propose: normalizeOptional(env.RUSTY_CREW_DEN_MEMORY_PROPOSE_PATH),
    },
  };
}

function validateDenMemoryConfig(config: RustyCrewDenMemoryConfig): void {
  if (!config.baseUrl && hasDenMemorySettings(config)) {
    throw new Error(
      "RUSTY_CREW_DEN_MEMORY_BASE_URL is required when Den memory token, timeout, or paths are configured",
    );
  }
  if (!config.baseUrl) return;
  try {
    new URL(config.baseUrl);
  } catch (error) {
    throw new Error("RUSTY_CREW_DEN_MEMORY_BASE_URL must be a valid URL", {
      cause: error,
    });
  }
}

function hasDenMemorySettings(config: RustyCrewDenMemoryConfig): boolean {
  return Boolean(
    config.bearerToken ||
    config.timeoutMs !== 5_000 ||
    Object.values(config.paths).some((value) => value !== undefined),
  );
}

function isLoopbackHost(host: string): boolean {
  return host === "127.0.0.1" || host === "::1" || host === "localhost";
}

function isNodeError(error: unknown): error is NodeJS.ErrnoException {
  return error instanceof Error && "code" in error;
}
