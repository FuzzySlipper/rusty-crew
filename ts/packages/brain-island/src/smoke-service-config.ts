import assert from "node:assert/strict";
import { existsSync, readFileSync, rmSync } from "node:fs";
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

const defaultConfig = loadRustyCrewServiceConfig({});
assert.equal(defaultConfig.paths.dataDir, RUSTY_CREW_DEFAULT_DATA_DIR);
assert.equal(defaultConfig.admin.host, RUSTY_CREW_DEFAULT_ADMIN_HOST);
assert.equal(defaultConfig.admin.port, RUSTY_CREW_DEFAULT_ADMIN_PORT);
assert.equal(defaultConfig.admin.allowLan, true);

const root = mkdtempSync(join(tmpdir(), "rusty-crew-service-config-"));
try {
  const config = loadRustyCrewServiceConfig({
    RUSTY_CREW_DATA_DIR: root,
    RUSTY_CREW_ADMIN_PORT: "19447",
    RUSTY_CREW_ADMIN_TOKEN: "local-token",
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
  assert.equal(config.admin.token, "local-token");

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
  });
  assert.equal(loopback.admin.allowLan, false);
} finally {
  rmSync(root, { recursive: true, force: true });
}

console.log("service config smoke passed");
