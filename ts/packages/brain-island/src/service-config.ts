import {
  closeSync,
  existsSync,
  readFileSync,
  mkdirSync,
  openSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { isAbsolute, join, resolve } from "node:path";
import { randomUUID } from "node:crypto";
import {
  loadDenSuccessorGatewayConfig,
  type DenSuccessorGatewayConfig,
  type DenSuccessorGatewayEnv,
  type DenMemoryApiMode,
  type DenMemoryClientPaths,
} from "@rusty-crew/adapter-den";

export interface RustyCrewServiceEnv extends DenSuccessorGatewayEnv {
  [key: string]: string | undefined;
  RUSTY_CREW_DATA_DIR?: string;
  RUSTY_CREW_CONFIG_DIR?: string;
  RUSTY_CREW_ENGINE_DATA_DIR?: string;
  RUSTY_CREW_LOG_DIR?: string;
  RUSTY_CREW_RUN_DIR?: string;
  RUSTY_CREW_ARTIFACT_DIR?: string;
  RUSTY_CREW_BACKUP_DIR?: string;
  RUSTY_CREW_STATIC_DIR?: string;
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
  RUSTY_CREW_DEN_MEMORY_API_MODE?: string;
  RUSTY_CREW_DEN_MEMORY_TIMEOUT_MS?: string;
  RUSTY_CREW_DEN_MEMORY_READ_PATH?: string;
  RUSTY_CREW_DEN_MEMORY_SEARCH_PATH?: string;
  RUSTY_CREW_DEN_MEMORY_RECALL_PATH?: string;
  RUSTY_CREW_DEN_MEMORY_STORE_PATH?: string;
  RUSTY_CREW_DEN_MEMORY_PROPOSE_PATH?: string;
  RUSTY_CREW_MCP_BASE_URL?: string;
  RUSTY_CREW_MCP_REQUEST_TIMEOUT_MS?: string;
  RUSTY_CREW_TELEGRAM_ENABLED?: string;
  RUSTY_CREW_TELEGRAM_BOT_TOKEN?: string;
  RUSTY_CREW_TELEGRAM_API_BASE_URL?: string;
  RUSTY_CREW_TELEGRAM_POLL_INTERVAL_MS?: string;
  RUSTY_CREW_TELEGRAM_POLL_TIMEOUT_SECONDS?: string;
  RUSTY_CREW_TELEGRAM_UPDATE_LIMIT?: string;
  RUSTY_CREW_TELEGRAM_MESSAGE_TTL_MS?: string;
  RUSTY_CREW_TELEGRAM_ADAPTER_ID?: string;
  RUSTY_CREW_STORAGE_BACKEND?: string;
  RUSTY_CREW_SQLITE_PATH?: string;
  RUSTY_CREW_SQLITE_WAL?: string;
  RUSTY_CREW_SQLITE_BUSY_TIMEOUT_MS?: string;
  RUSTY_CREW_POSTGRES_DATABASE_URL_ENV?: string;
  RUSTY_CREW_POSTGRES_SCHEMA?: string;
  RUSTY_CREW_POSTGRES_BOOT_MODE?: string;
  RUSTY_CREW_POSTGRES_MAX_CONNECTIONS?: string;
  RUSTY_CREW_POSTGRES_STATEMENT_TIMEOUT_MS?: string;
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
  staticDir?: string;
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
  apiMode: DenMemoryApiMode;
  timeoutMs: number;
  paths: Partial<DenMemoryClientPaths>;
}

export interface RustyCrewMcpConfig {
  baseUrl?: string;
  requestTimeoutMs: number;
}

export interface RustyCrewTelegramConfig {
  enabled: boolean;
  adapterId: string;
  botToken?: string;
  apiBaseUrl?: string;
  pollIntervalMs: number;
  pollTimeoutSeconds: number;
  updateLimit: number;
  messageTtlMs: number;
}

export type RustyCrewStorageBackend = "sqlite" | "postgres";

export interface RustyCrewSqliteStorageConfig {
  path: string;
  wal: boolean;
  busyTimeoutMs: number;
  effectivePath: string;
}

export interface RustyCrewPostgresStorageConfig {
  databaseUrlEnv: string;
  schema: string;
  bootMode: "blocked" | "proof_admin" | "active";
  maxConnections: number;
  statementTimeoutMs: number;
}

export interface RustyCrewStorageConfig {
  backend: RustyCrewStorageBackend;
  sqlite: RustyCrewSqliteStorageConfig;
  postgres: RustyCrewPostgresStorageConfig;
  implementationStatus: "active" | "blocked_unimplemented" | "proof_admin_only";
}

export interface RustyCrewServiceConfig {
  paths: RustyCrewServicePaths;
  admin: RustyCrewAdminConfig;
  background: RustyCrewBackgroundConfig;
  denConversationProjectId: string;
  denMemory: RustyCrewDenMemoryConfig;
  mcp: RustyCrewMcpConfig;
  telegram: RustyCrewTelegramConfig;
  storage: RustyCrewStorageConfig;
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
    staticDir: resolveStaticDir(env.RUSTY_CREW_STATIC_DIR, dataDir),
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
  const mcp = loadRustyCrewMcpConfig(env);
  const telegram = loadRustyCrewTelegramConfig(env);
  const storage = loadRustyCrewStorageConfig(env, paths);

  validateRustyCrewServiceConfig({
    paths,
    admin,
    background,
    denConversationProjectId,
    denMemory,
    mcp,
    telegram,
    storage,
    denSuccessorGateway,
  });
  return {
    paths,
    admin,
    background,
    denConversationProjectId,
    denMemory,
    mcp,
    telegram,
    storage,
    denSuccessorGateway,
  };
}

export function validateRustyCrewServiceConfig(
  config: RustyCrewServiceConfig,
): void {
  for (const [name, path] of Object.entries(config.paths)) {
    if (path === undefined) continue;
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
  validateMcpConfig(config.mcp);
  validateTelegramConfig(config.telegram);
  validateStorageConfig(config.storage);
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

function resolveStaticDir(
  input: string | undefined,
  dataDir: string,
): string | undefined {
  const configured = normalizeOptional(input);
  if (configured !== undefined) return resolve(configured);
  const defaultStaticDir = join(dataDir, "site");
  return existsSync(defaultStaticDir) ? defaultStaticDir : undefined;
}

export function acquireRustyCrewServiceLock(
  config: RustyCrewServiceConfig,
): RustyCrewServiceLock {
  ensureRustyCrewServiceDirectories(config);
  const ownerToken = randomUUID();
  let fd: number;
  try {
    fd = openSync(config.paths.lockFile, "wx", 0o640);
  } catch (error) {
    if (isNodeError(error) && error.code === "EEXIST") {
      if (clearStaleRustyCrewServiceLock(config.paths.lockFile)) {
        fd = openSync(config.paths.lockFile, "wx", 0o640);
      } else {
        throw new Error(
          `rusty-crew service lock already exists at ${config.paths.lockFile}`,
        );
      }
    } else {
      throw error;
    }
  }

  let released = false;
  writeFileSync(
    fd,
    JSON.stringify(
      {
        pid: process.pid,
        ownerToken,
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
      if (lockFileBelongsToCurrentProcess(config.paths.lockFile, ownerToken)) {
        rmSync(config.paths.lockFile, { force: true });
      }
    },
  };
}

function clearStaleRustyCrewServiceLock(lockFile: string): boolean {
  const lock = readServiceLockFile(lockFile);
  if (lock === undefined) {
    rmSync(lockFile, { force: true });
    return true;
  }
  if (lock.pid === process.pid) return false;
  if (!processIsAlive(lock.pid)) {
    rmSync(lockFile, { force: true });
    return true;
  }
  if (!processLooksLikeRustyCrewService(lock.pid)) {
    rmSync(lockFile, { force: true });
    return true;
  }
  return false;
}

function lockFileBelongsToCurrentProcess(
  lockFile: string,
  ownerToken: string,
): boolean {
  const lock = readServiceLockFile(lockFile);
  return lock?.pid === process.pid && lock.ownerToken === ownerToken;
}

function readServiceLockFile(
  lockFile: string,
): { pid: number; ownerToken?: string } | undefined {
  try {
    const parsed = JSON.parse(readFileSync(lockFile, "utf8")) as unknown;
    if (
      typeof parsed !== "object" ||
      parsed === null ||
      Array.isArray(parsed)
    ) {
      return undefined;
    }
    const record = parsed as Record<string, unknown>;
    const pid = record.pid;
    if (typeof pid !== "number" || !Number.isSafeInteger(pid) || pid <= 0) {
      return undefined;
    }
    return {
      pid,
      ownerToken:
        typeof record.ownerToken === "string" ? record.ownerToken : undefined,
    };
  } catch {
    return undefined;
  }
}

function processIsAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    if (isNodeError(error) && error.code === "ESRCH") return false;
    return true;
  }
}

function processLooksLikeRustyCrewService(pid: number): boolean {
  try {
    const cmdline = readFileSync(`/proc/${pid}/cmdline`, "utf8").replace(
      /\0/g,
      " ",
    );
    return (
      cmdline.includes("rusty-crew") &&
      (cmdline.includes("service-host.ts") || cmdline.includes("service:start"))
    );
  } catch {
    return true;
  }
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

function parseStorageBackend(
  input: string | undefined,
): RustyCrewStorageBackend {
  const value = normalizeOptional(input);
  if (value === undefined || value === "sqlite") return "sqlite";
  if (value === "postgres" || value === "postgresql") return "postgres";
  throw new Error("RUSTY_CREW_STORAGE_BACKEND must be sqlite or postgres");
}

function parsePostgresBootMode(
  input: string | undefined,
): "blocked" | "proof_admin" | "active" {
  const value = normalizeOptional(input);
  if (value === undefined || value === "blocked") return "blocked";
  if (value === "proof_admin" || value === "proof-admin") return "proof_admin";
  if (value === "active") return "active";
  throw new Error(
    "RUSTY_CREW_POSTGRES_BOOT_MODE must be blocked, proof_admin, or active",
  );
}

function normalizeOptional(input: string | undefined): string | undefined {
  const value = input?.trim();
  return value ? value : undefined;
}

function loadRustyCrewStorageConfig(
  env: RustyCrewServiceEnv,
  paths: RustyCrewServicePaths,
): RustyCrewStorageConfig {
  const sqlitePath =
    normalizeOptional(env.RUSTY_CREW_SQLITE_PATH) ?? "coordination.sqlite3";
  const backend = parseStorageBackend(env.RUSTY_CREW_STORAGE_BACKEND);
  const postgresBootMode = parsePostgresBootMode(
    env.RUSTY_CREW_POSTGRES_BOOT_MODE,
  );
  return {
    backend,
    sqlite: {
      path: sqlitePath,
      wal: parseBoolean(
        env.RUSTY_CREW_SQLITE_WAL,
        true,
        "RUSTY_CREW_SQLITE_WAL",
      ),
      busyTimeoutMs: parsePositiveInteger(
        env.RUSTY_CREW_SQLITE_BUSY_TIMEOUT_MS,
        5_000,
        "RUSTY_CREW_SQLITE_BUSY_TIMEOUT_MS",
      ),
      effectivePath: isAbsolute(sqlitePath)
        ? sqlitePath
        : join(paths.engineDataDir, sqlitePath),
    },
    postgres: {
      databaseUrlEnv:
        normalizeOptional(env.RUSTY_CREW_POSTGRES_DATABASE_URL_ENV) ??
        "RUSTY_CREW_DATABASE_URL",
      schema: normalizeOptional(env.RUSTY_CREW_POSTGRES_SCHEMA) ?? "rusty_crew",
      bootMode: postgresBootMode,
      maxConnections: parsePositiveInteger(
        env.RUSTY_CREW_POSTGRES_MAX_CONNECTIONS,
        10,
        "RUSTY_CREW_POSTGRES_MAX_CONNECTIONS",
      ),
      statementTimeoutMs: parsePositiveInteger(
        env.RUSTY_CREW_POSTGRES_STATEMENT_TIMEOUT_MS,
        30_000,
        "RUSTY_CREW_POSTGRES_STATEMENT_TIMEOUT_MS",
      ),
    },
    implementationStatus:
      backend === "sqlite"
        ? "active"
        : postgresBootMode === "active"
          ? "active"
          : postgresBootMode === "proof_admin"
            ? "proof_admin_only"
            : "blocked_unimplemented",
  };
}

function validateStorageConfig(config: RustyCrewStorageConfig): void {
  if (!config.sqlite.path.trim()) {
    throw new Error("RUSTY_CREW_SQLITE_PATH must not be empty");
  }
  if (
    !config.sqlite.effectivePath.trim() ||
    !isAbsolute(config.sqlite.effectivePath)
  ) {
    throw new Error("RUSTY_CREW_SQLITE_PATH must resolve to an absolute path");
  }
  if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(config.postgres.databaseUrlEnv)) {
    throw new Error(
      "RUSTY_CREW_POSTGRES_DATABASE_URL_ENV must be an environment variable name, not a raw URL",
    );
  }
  if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(config.postgres.schema)) {
    throw new Error(
      "RUSTY_CREW_POSTGRES_SCHEMA must be a PostgreSQL identifier",
    );
  }
}

function loadRustyCrewDenMemoryConfig(
  env: RustyCrewServiceEnv,
): RustyCrewDenMemoryConfig {
  return {
    baseUrl: normalizeOptional(env.RUSTY_CREW_DEN_MEMORY_BASE_URL),
    bearerToken:
      normalizeOptional(env.RUSTY_CREW_DEN_MEMORY_BEARER_TOKEN) ??
      normalizeOptional(env.RUSTY_CREW_DEN_MEMORY_TOKEN),
    apiMode: parseDenMemoryApiMode(env.RUSTY_CREW_DEN_MEMORY_API_MODE),
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

function parseDenMemoryApiMode(input: string | undefined): DenMemoryApiMode {
  const value = normalizeOptional(input);
  if (value === undefined || value === "v1") return "v1";
  if (value === "den-memories-v0") return "den-memories-v0";
  throw new Error(
    "RUSTY_CREW_DEN_MEMORY_API_MODE must be v1 or den-memories-v0",
  );
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
    config.apiMode !== "v1" ||
    config.timeoutMs !== 5_000 ||
    Object.values(config.paths).some((value) => value !== undefined),
  );
}

function loadRustyCrewMcpConfig(env: RustyCrewServiceEnv): RustyCrewMcpConfig {
  return {
    baseUrl: normalizeOptional(env.RUSTY_CREW_MCP_BASE_URL),
    requestTimeoutMs: parsePositiveInteger(
      env.RUSTY_CREW_MCP_REQUEST_TIMEOUT_MS,
      30_000,
      "RUSTY_CREW_MCP_REQUEST_TIMEOUT_MS",
    ),
  };
}

function validateMcpConfig(config: RustyCrewMcpConfig): void {
  if (!config.baseUrl) return;
  try {
    const url = new URL(config.baseUrl);
    if (url.protocol !== "http:" && url.protocol !== "https:") {
      throw new Error("protocol must be http or https");
    }
  } catch (error) {
    throw new Error("RUSTY_CREW_MCP_BASE_URL must be a valid HTTP(S) URL", {
      cause: error,
    });
  }
}

function loadRustyCrewTelegramConfig(
  env: RustyCrewServiceEnv,
): RustyCrewTelegramConfig {
  const botToken = normalizeOptional(env.RUSTY_CREW_TELEGRAM_BOT_TOKEN);
  return {
    enabled: parseBoolean(
      env.RUSTY_CREW_TELEGRAM_ENABLED,
      false,
      "RUSTY_CREW_TELEGRAM_ENABLED",
    ),
    adapterId:
      normalizeOptional(env.RUSTY_CREW_TELEGRAM_ADAPTER_ID) ?? "telegram-main",
    botToken,
    apiBaseUrl: normalizeOptional(env.RUSTY_CREW_TELEGRAM_API_BASE_URL),
    pollIntervalMs: parsePositiveInteger(
      env.RUSTY_CREW_TELEGRAM_POLL_INTERVAL_MS,
      2_000,
      "RUSTY_CREW_TELEGRAM_POLL_INTERVAL_MS",
    ),
    pollTimeoutSeconds: parseNonNegativeInteger(
      env.RUSTY_CREW_TELEGRAM_POLL_TIMEOUT_SECONDS,
      20,
      "RUSTY_CREW_TELEGRAM_POLL_TIMEOUT_SECONDS",
    ),
    updateLimit: parsePositiveInteger(
      env.RUSTY_CREW_TELEGRAM_UPDATE_LIMIT,
      50,
      "RUSTY_CREW_TELEGRAM_UPDATE_LIMIT",
    ),
    messageTtlMs: parsePositiveInteger(
      env.RUSTY_CREW_TELEGRAM_MESSAGE_TTL_MS,
      5 * 60 * 1_000,
      "RUSTY_CREW_TELEGRAM_MESSAGE_TTL_MS",
    ),
  };
}

function validateTelegramConfig(config: RustyCrewTelegramConfig): void {
  if (config.enabled && !config.botToken) {
    throw new Error(
      "RUSTY_CREW_TELEGRAM_BOT_TOKEN is required when RUSTY_CREW_TELEGRAM_ENABLED=true",
    );
  }
  if (config.updateLimit > 100) {
    throw new Error("RUSTY_CREW_TELEGRAM_UPDATE_LIMIT must be at most 100");
  }
  if (config.apiBaseUrl !== undefined) {
    try {
      const url = new URL(config.apiBaseUrl);
      if (url.protocol !== "http:" && url.protocol !== "https:") {
        throw new Error("protocol must be http or https");
      }
    } catch (error) {
      throw new Error(
        "RUSTY_CREW_TELEGRAM_API_BASE_URL must be a valid HTTP(S) URL",
        {
          cause: error,
        },
      );
    }
  }
}

function isLoopbackHost(host: string): boolean {
  return host === "127.0.0.1" || host === "::1" || host === "localhost";
}

function isNodeError(error: unknown): error is NodeJS.ErrnoException {
  return error instanceof Error && "code" in error;
}
