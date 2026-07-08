import { fileURLToPath } from "node:url";

import {
  acquireRustyCrewServiceLock,
  loadRustyCrewServiceConfig,
} from "@rusty-crew/brain-island";
import {
  assertServiceHostStorageBootReady,
  preflightServiceHostStorageBoot,
} from "./storage-preflight.js";

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback;
}

export function runRustyCrewServicePreflight(): void {
  const env = process.env;
  const config = loadRustyCrewServiceConfig(env);
  const storage = preflightServiceHostStorageBoot(config, env);
  assertServiceHostStorageBootReady(storage);
  const lock = acquireRustyCrewServiceLock(config);
  lock.release();
  console.log(
    JSON.stringify(
      {
        ok: true,
        dataDir: config.paths.dataDir,
        runDir: config.paths.runDir,
        lockFile: config.paths.lockFile,
        adminHost: config.admin.host,
        adminPort: config.admin.port,
        storageBackend: config.storage.backend,
        storage,
      },
      null,
      2,
    ),
  );
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  try {
    runRustyCrewServicePreflight();
  } catch (error) {
    console.error(errorMessage(error, "rusty-crew service preflight failed"));
    process.exit(1);
  }
}
