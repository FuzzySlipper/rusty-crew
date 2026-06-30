import assert from "node:assert/strict";
import {
  existsSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  acquireRustyCrewServiceLock,
  ensureRustyCrewServiceDirectories,
  loadRustyCrewServiceConfig,
  RUSTY_CREW_DEFAULT_ADMIN_HOST,
  RUSTY_CREW_DEFAULT_ADMIN_PORT,
  RUSTY_CREW_DEFAULT_DATA_DIR,
  RUSTY_CREW_DEFAULT_WORKDIR,
} from "./service-config.js";
import { loadRustyCrewRuntimeConfig } from "./service-runtime-config.js";

assert.throws(() => loadRustyCrewServiceConfig({}), /RUSTY_CREW_ADMIN_TOKEN/);

const defaultConfig = loadRustyCrewServiceConfig({
  RUSTY_CREW_ADMIN_TOKEN: "default-token",
});
assert.equal(defaultConfig.paths.dataDir, RUSTY_CREW_DEFAULT_DATA_DIR);
assert.equal(defaultConfig.paths.defaultWorkdir, RUSTY_CREW_DEFAULT_WORKDIR);
assert.equal(
  defaultConfig.paths.staticDir,
  existsSync(join(RUSTY_CREW_DEFAULT_DATA_DIR, "site"))
    ? join(RUSTY_CREW_DEFAULT_DATA_DIR, "site")
    : undefined,
);
assert.equal(defaultConfig.admin.host, RUSTY_CREW_DEFAULT_ADMIN_HOST);
assert.equal(defaultConfig.admin.port, RUSTY_CREW_DEFAULT_ADMIN_PORT);
assert.equal(defaultConfig.admin.allowLan, true);
assert.equal(defaultConfig.admin.authMode, "bearer");
assert.equal(defaultConfig.background.schedulerTickIntervalMs, 1_000);
assert.equal(defaultConfig.background.wakeDispatchIntervalMs, 250);
assert.equal(defaultConfig.denMemory.baseUrl, undefined);
assert.equal(defaultConfig.denMemory.apiMode, "v1");
assert.equal(defaultConfig.denMemory.timeoutMs, 5_000);
assert.equal(defaultConfig.mcp.baseUrl, undefined);
assert.equal(defaultConfig.mcp.requestTimeoutMs, 30_000);
assert.equal(defaultConfig.telegram.enabled, false);
assert.equal(defaultConfig.telegram.adapterId, "telegram-main");
assert.equal(defaultConfig.telegram.pollIntervalMs, 2_000);
assert.equal(defaultConfig.telegram.pollTimeoutSeconds, 20);
assert.equal(defaultConfig.telegram.updateLimit, 50);
assert.equal(defaultConfig.telegram.messageTtlMs, 300_000);
assert.equal(defaultConfig.storage.backend, "sqlite");
assert.equal(defaultConfig.storage.implementationStatus, "active");
assert.equal(defaultConfig.storage.sqlite.path, "coordination.sqlite3");
assert.equal(
  defaultConfig.storage.sqlite.effectivePath,
  join(RUSTY_CREW_DEFAULT_DATA_DIR, "data", "engine", "coordination.sqlite3"),
);
assert.equal(defaultConfig.storage.sqlite.wal, true);
assert.equal(defaultConfig.storage.sqlite.busyTimeoutMs, 5_000);
assert.equal(
  defaultConfig.storage.postgres.databaseUrlEnv,
  "RUSTY_CREW_DATABASE_URL",
);
assert.equal(defaultConfig.storage.postgres.schema, "rusty_crew");
assert.equal(defaultConfig.storage.postgres.bootMode, "blocked");

const root = mkdtempSync(join(tmpdir(), "rusty-crew-service-config-"));
try {
  const config = loadRustyCrewServiceConfig({
    RUSTY_CREW_DATA_DIR: root,
    RUSTY_CREW_DEFAULT_WORKDIR: join(root, "work"),
    RUSTY_CREW_ADMIN_PORT: "19447",
    RUSTY_CREW_ADMIN_TOKEN: "local-token",
    RUSTY_CREW_SCHEDULER_TICK_INTERVAL_MS: "2000",
    RUSTY_CREW_WAKE_DISPATCH_INTERVAL_MS: "500",
    RUSTY_CREW_DEN_MEMORY_BASE_URL: "http://127.0.0.1:19999",
    RUSTY_CREW_DEN_MEMORY_TOKEN: "memory-token",
    RUSTY_CREW_DEN_MEMORY_API_MODE: "den-memories-v0",
    RUSTY_CREW_DEN_MEMORY_TIMEOUT_MS: "7500",
    RUSTY_CREW_DEN_MEMORY_RECALL_PATH: "/memory/recall",
    RUSTY_CREW_MCP_BASE_URL: "http://127.0.0.1:5199/mcp",
    RUSTY_CREW_MCP_REQUEST_TIMEOUT_MS: "12000",
    RUSTY_CREW_TELEGRAM_ENABLED: "true",
    RUSTY_CREW_TELEGRAM_BOT_TOKEN: "telegram-token",
    RUSTY_CREW_TELEGRAM_API_BASE_URL: "http://127.0.0.1:19998",
    RUSTY_CREW_TELEGRAM_POLL_INTERVAL_MS: "3000",
    RUSTY_CREW_TELEGRAM_POLL_TIMEOUT_SECONDS: "0",
    RUSTY_CREW_TELEGRAM_UPDATE_LIMIT: "10",
    RUSTY_CREW_TELEGRAM_MESSAGE_TTL_MS: "60000",
    RUSTY_CREW_TELEGRAM_ADAPTER_ID: "telegram-field",
    RUSTY_CREW_SQLITE_PATH: "local.sqlite3",
    RUSTY_CREW_SQLITE_WAL: "false",
    RUSTY_CREW_SQLITE_BUSY_TIMEOUT_MS: "2500",
    RUSTY_CREW_POSTGRES_DATABASE_URL_ENV: "RUSTY_CREW_TEST_DATABASE_URL",
    RUSTY_CREW_POSTGRES_SCHEMA: "rusty_crew_test",
    RUSTY_CREW_POSTGRES_MAX_CONNECTIONS: "4",
    RUSTY_CREW_POSTGRES_STATEMENT_TIMEOUT_MS: "15000",
  });

  assert.equal(config.paths.configDir, join(root, "config"));
  assert.equal(
    config.paths.serviceConfigFile,
    join(root, "config", "service.json"),
  );
  assert.equal(config.paths.engineDataDir, join(root, "data", "engine"));
  assert.equal(config.paths.logDir, join(root, "logs"));
  assert.equal(config.paths.runDir, join(root, "run"));
  assert.equal(config.paths.artifactDir, join(root, "artifacts"));
  assert.equal(config.paths.backupDir, join(root, "backups"));
  assert.equal(config.paths.defaultWorkdir, join(root, "work"));
  assert.equal(config.paths.staticDir, undefined);
  assert.equal(config.admin.authMode, "bearer");
  assert.equal(config.admin.token, "local-token");
  assert.equal(config.background.schedulerTickIntervalMs, 2_000);
  assert.equal(config.background.wakeDispatchIntervalMs, 500);
  assert.equal(config.denMemory.baseUrl, "http://127.0.0.1:19999");
  assert.equal(config.denMemory.bearerToken, "memory-token");
  assert.equal(config.denMemory.apiMode, "den-memories-v0");
  assert.equal(config.denMemory.timeoutMs, 7_500);
  assert.equal(config.denMemory.paths.recall, "/memory/recall");
  assert.equal(config.mcp.baseUrl, "http://127.0.0.1:5199/mcp");
  assert.equal(config.mcp.requestTimeoutMs, 12_000);
  assert.equal(config.telegram.enabled, true);
  assert.equal(config.telegram.botToken, "telegram-token");
  assert.equal(config.telegram.apiBaseUrl, "http://127.0.0.1:19998");
  assert.equal(config.telegram.pollIntervalMs, 3_000);
  assert.equal(config.telegram.pollTimeoutSeconds, 0);
  assert.equal(config.telegram.updateLimit, 10);
  assert.equal(config.telegram.messageTtlMs, 60_000);
  assert.equal(config.telegram.adapterId, "telegram-field");
  assert.equal(config.storage.backend, "sqlite");
  assert.equal(config.storage.sqlite.path, "local.sqlite3");
  assert.equal(
    config.storage.sqlite.effectivePath,
    join(root, "data", "engine", "local.sqlite3"),
  );
  assert.equal(config.storage.sqlite.wal, false);
  assert.equal(config.storage.sqlite.busyTimeoutMs, 2_500);
  assert.equal(
    config.storage.postgres.databaseUrlEnv,
    "RUSTY_CREW_TEST_DATABASE_URL",
  );
  assert.equal(config.storage.postgres.schema, "rusty_crew_test");
  assert.equal(config.storage.postgres.maxConnections, 4);
  assert.equal(config.storage.postgres.statementTimeoutMs, 15_000);

  const noAuth = loadRustyCrewServiceConfig({
    RUSTY_CREW_DATA_DIR: root,
    RUSTY_CREW_ADMIN_AUTH_MODE: "none",
  });
  assert.equal(noAuth.admin.authMode, "none");
  assert.equal(noAuth.admin.token, undefined);

  const defaultSiteDir = join(root, "site");
  mkdirSync(defaultSiteDir);
  const defaultSite = loadRustyCrewServiceConfig({
    RUSTY_CREW_DATA_DIR: root,
    RUSTY_CREW_ADMIN_AUTH_MODE: "none",
  });
  assert.equal(defaultSite.paths.staticDir, defaultSiteDir);

  const customSiteDir = join(root, "custom-site");
  const customSite = loadRustyCrewServiceConfig({
    RUSTY_CREW_DATA_DIR: root,
    RUSTY_CREW_ADMIN_AUTH_MODE: "none",
    RUSTY_CREW_STATIC_DIR: customSiteDir,
  });
  assert.equal(customSite.paths.staticDir, customSiteDir);

  ensureRustyCrewServiceDirectories(config);
  writeFileSync(
    config.paths.serviceConfigFile,
    JSON.stringify(
      {
        storage: {
          backend: "sqlite",
          sqlite: {
            path: "runtime.sqlite3",
            wal: true,
            busyTimeoutMs: 3000,
          },
          postgres: {
            databaseUrlEnv: "RUSTY_CREW_RUNTIME_DATABASE_URL",
            schema: "rusty_runtime",
            maxConnections: 3,
            statementTimeoutMs: 12000,
          },
        },
      },
      null,
      2,
    ),
  );
  const runtimeConfig = await loadRustyCrewRuntimeConfig(config);
  assert(runtimeConfig.storage);
  assert.equal(runtimeConfig.storage.backend, "sqlite");
  assert.equal(runtimeConfig.storage.sqlite.path, "runtime.sqlite3");
  assert.equal(
    runtimeConfig.storage.sqlite.effectivePath,
    join(root, "data", "engine", "runtime.sqlite3"),
  );
  assert.equal(
    runtimeConfig.storage.postgres.databaseUrlEnv,
    "RUSTY_CREW_RUNTIME_DATABASE_URL",
  );
  assert.equal(runtimeConfig.storage.postgres.schema, "rusty_runtime");
  assert.equal(runtimeConfig.storage.postgres.bootMode, "blocked");

  writeFileSync(
    config.paths.serviceConfigFile,
    JSON.stringify({ storage: { backend: "mysql" } }),
  );
  await assert.rejects(
    () => loadRustyCrewRuntimeConfig(config),
    /storage.backend/,
  );

  writeFileSync(
    config.paths.serviceConfigFile,
    JSON.stringify({ storage: { backend: "postgres" } }),
  );
  const blockedPostgresRuntimeConfig = await loadRustyCrewRuntimeConfig(config);
  assert.equal(blockedPostgresRuntimeConfig.storage?.backend, "postgres");
  assert.equal(
    blockedPostgresRuntimeConfig.storage?.implementationStatus,
    "blocked_unimplemented",
  );
  assert.equal(
    blockedPostgresRuntimeConfig.storage?.postgres.bootMode,
    "blocked",
  );

  writeFileSync(
    config.paths.serviceConfigFile,
    JSON.stringify({
      storage: {
        backend: "postgres",
        postgres: { bootMode: "proof_admin" },
      },
    }),
  );
  const proofAdminRuntimeConfig = await loadRustyCrewRuntimeConfig(config);
  assert.equal(proofAdminRuntimeConfig.storage?.backend, "postgres");
  assert.equal(
    proofAdminRuntimeConfig.storage?.implementationStatus,
    "proof_admin_only",
  );
  assert.equal(
    proofAdminRuntimeConfig.storage?.postgres.bootMode,
    "proof_admin",
  );

  for (const path of [
    config.paths.configDir,
    config.paths.engineDataDir,
    config.paths.logDir,
    config.paths.runDir,
    config.paths.artifactDir,
    config.paths.backupDir,
  ]) {
    assert.equal(existsSync(path), true, `${path} should exist`);
  }

  const lock = acquireRustyCrewServiceLock(config);
  assert.equal(existsSync(lock.lockFile), true);
  const lockContents = JSON.parse(readFileSync(lock.lockFile, "utf8")) as {
    pid?: number;
  };
  assert.equal(lockContents.pid, process.pid);
  assert.throws(
    () => acquireRustyCrewServiceLock(config),
    /service lock already exists/,
  );
  lock.release();
  lock.release();
  assert.equal(existsSync(lock.lockFile), false);

  assert.throws(
    () =>
      loadRustyCrewServiceConfig({
        RUSTY_CREW_DATA_DIR: root,
        RUSTY_CREW_ADMIN_HOST: "0.0.0.0",
        RUSTY_CREW_ADMIN_ALLOW_LAN: "false",
      }),
    /ADMIN_ALLOW_LAN/,
  );

  const loopback = loadRustyCrewServiceConfig({
    RUSTY_CREW_DATA_DIR: root,
    RUSTY_CREW_ADMIN_HOST: "127.0.0.1",
    RUSTY_CREW_ADMIN_ALLOW_LAN: "false",
    RUSTY_CREW_ADMIN_TOKEN: "loopback-token",
  });
  assert.equal(loopback.admin.allowLan, false);

  assert.throws(
    () =>
      loadRustyCrewServiceConfig({
        RUSTY_CREW_DATA_DIR: root,
        RUSTY_CREW_ADMIN_AUTH_MODE: "none",
        RUSTY_CREW_DEN_MEMORY_TOKEN: "token-without-endpoint",
      }),
    /DEN_MEMORY_BASE_URL/,
  );

  assert.throws(
    () =>
      loadRustyCrewServiceConfig({
        RUSTY_CREW_DATA_DIR: root,
        RUSTY_CREW_ADMIN_AUTH_MODE: "none",
        RUSTY_CREW_DEN_MEMORY_BASE_URL: "not a url",
      }),
    /DEN_MEMORY_BASE_URL/,
  );

  assert.throws(
    () =>
      loadRustyCrewServiceConfig({
        RUSTY_CREW_DATA_DIR: root,
        RUSTY_CREW_ADMIN_AUTH_MODE: "none",
        RUSTY_CREW_DEN_MEMORY_API_MODE: "den-memories-v0",
      }),
    /DEN_MEMORY_BASE_URL/,
  );

  assert.throws(
    () =>
      loadRustyCrewServiceConfig({
        RUSTY_CREW_DATA_DIR: root,
        RUSTY_CREW_ADMIN_AUTH_MODE: "none",
        RUSTY_CREW_DEN_MEMORY_API_MODE: "unknown",
      }),
    /DEN_MEMORY_API_MODE/,
  );

  assert.throws(
    () =>
      loadRustyCrewServiceConfig({
        RUSTY_CREW_DATA_DIR: root,
        RUSTY_CREW_ADMIN_AUTH_MODE: "none",
        RUSTY_CREW_MCP_BASE_URL: "config://mcp/runner",
      }),
    /MCP_BASE_URL/,
  );

  assert.throws(
    () =>
      loadRustyCrewServiceConfig({
        RUSTY_CREW_DATA_DIR: root,
        RUSTY_CREW_ADMIN_AUTH_MODE: "none",
        RUSTY_CREW_TELEGRAM_ENABLED: "true",
      }),
    /TELEGRAM_BOT_TOKEN/,
  );

  assert.throws(
    () =>
      loadRustyCrewServiceConfig({
        RUSTY_CREW_DATA_DIR: root,
        RUSTY_CREW_ADMIN_AUTH_MODE: "none",
        RUSTY_CREW_TELEGRAM_API_BASE_URL: "file:///tmp/bot",
      }),
    /TELEGRAM_API_BASE_URL/,
  );

  assert.throws(
    () =>
      loadRustyCrewServiceConfig({
        RUSTY_CREW_DATA_DIR: root,
        RUSTY_CREW_ADMIN_AUTH_MODE: "none",
        RUSTY_CREW_TELEGRAM_UPDATE_LIMIT: "101",
      }),
    /TELEGRAM_UPDATE_LIMIT/,
  );

  assert.throws(
    () =>
      loadRustyCrewServiceConfig({
        RUSTY_CREW_DATA_DIR: root,
        RUSTY_CREW_ADMIN_AUTH_MODE: "none",
        RUSTY_CREW_STORAGE_BACKEND: "mysql",
      }),
    /STORAGE_BACKEND/,
  );

  const blockedPostgresServiceConfig = loadRustyCrewServiceConfig({
    RUSTY_CREW_DATA_DIR: root,
    RUSTY_CREW_ADMIN_AUTH_MODE: "none",
    RUSTY_CREW_STORAGE_BACKEND: "postgres",
    RUSTY_CREW_POSTGRES_DATABASE_URL_ENV: "RUSTY_CREW_DATABASE_URL",
  });
  assert.equal(blockedPostgresServiceConfig.storage.backend, "postgres");
  assert.equal(
    blockedPostgresServiceConfig.storage.implementationStatus,
    "blocked_unimplemented",
  );
  assert.equal(
    blockedPostgresServiceConfig.storage.postgres.bootMode,
    "blocked",
  );

  const proofAdminServiceConfig = loadRustyCrewServiceConfig({
    RUSTY_CREW_DATA_DIR: root,
    RUSTY_CREW_ADMIN_AUTH_MODE: "none",
    RUSTY_CREW_STORAGE_BACKEND: "postgres",
    RUSTY_CREW_POSTGRES_BOOT_MODE: "proof_admin",
    RUSTY_CREW_POSTGRES_DATABASE_URL_ENV: "RUSTY_CREW_DATABASE_URL",
  });
  assert.equal(proofAdminServiceConfig.storage.backend, "postgres");
  assert.equal(
    proofAdminServiceConfig.storage.implementationStatus,
    "proof_admin_only",
  );
  assert.equal(
    proofAdminServiceConfig.storage.postgres.bootMode,
    "proof_admin",
  );

  assert.throws(
    () =>
      loadRustyCrewServiceConfig({
        RUSTY_CREW_DATA_DIR: root,
        RUSTY_CREW_ADMIN_AUTH_MODE: "none",
        RUSTY_CREW_POSTGRES_DATABASE_URL_ENV: "postgres://unsafe",
      }),
    /not a raw URL/,
  );
} finally {
  rmSync(root, { recursive: true, force: true });
}

console.log("service config smoke passed");
