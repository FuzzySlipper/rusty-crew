import assert from "node:assert/strict";
import { existsSync, mkdirSync, readFileSync, rmSync } from "node:fs";
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
} from "./service-config.js";

assert.throws(() => loadRustyCrewServiceConfig({}), /RUSTY_CREW_ADMIN_TOKEN/);

const defaultConfig = loadRustyCrewServiceConfig({
  RUSTY_CREW_ADMIN_TOKEN: "default-token",
});
assert.equal(defaultConfig.paths.dataDir, RUSTY_CREW_DEFAULT_DATA_DIR);
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

const root = mkdtempSync(join(tmpdir(), "rusty-crew-service-config-"));
try {
  const config = loadRustyCrewServiceConfig({
    RUSTY_CREW_DATA_DIR: root,
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
} finally {
  rmSync(root, { recursive: true, force: true });
}

console.log("service config smoke passed");
