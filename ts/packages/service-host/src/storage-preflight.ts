import type {
  RustyCrewServiceConfig,
  RustyCrewServiceEnv,
} from "@rusty-crew/brain-island";

export interface ServiceHostStorageBootPreflight {
  backend: RustyCrewServiceConfig["storage"]["backend"];
  implementationStatus: RustyCrewServiceConfig["storage"]["implementationStatus"];
  ready: boolean;
  blockers: string[];
  sqlite: {
    effectivePath: string;
    wal: boolean;
    busyTimeoutMs: number;
  };
  postgres: {
    bootMode: RustyCrewServiceConfig["storage"]["postgres"]["bootMode"];
    databaseUrlEnv: string;
    databaseUrlPresent: boolean;
    schema: string;
    maxConnections: number;
    statementTimeoutMs: number;
  };
}

export function preflightServiceHostStorageBoot(
  config: RustyCrewServiceConfig,
  env: RustyCrewServiceEnv,
): ServiceHostStorageBootPreflight {
  const blockers: string[] = [];
  const storage = config.storage;
  const databaseUrl = env[storage.postgres.databaseUrlEnv];

  if (storage.backend === "postgres") {
    if (storage.postgres.bootMode !== "active") {
      blockers.push(
        `storage.backend=postgres requires storage.postgres.bootMode=active for full service startup; current mode is ${storage.postgres.bootMode}`,
      );
    }
    if (databaseUrl === undefined || databaseUrl.trim() === "") {
      blockers.push(
        `storage.backend=postgres requires ${storage.postgres.databaseUrlEnv} to be set`,
      );
    }
  }

  return {
    backend: storage.backend,
    implementationStatus: storage.implementationStatus,
    ready: blockers.length === 0,
    blockers,
    sqlite: {
      effectivePath: storage.sqlite.effectivePath,
      wal: storage.sqlite.wal,
      busyTimeoutMs: storage.sqlite.busyTimeoutMs,
    },
    postgres: {
      bootMode: storage.postgres.bootMode,
      databaseUrlEnv: storage.postgres.databaseUrlEnv,
      databaseUrlPresent:
        databaseUrl !== undefined && databaseUrl.trim() !== "",
      schema: storage.postgres.schema,
      maxConnections: storage.postgres.maxConnections,
      statementTimeoutMs: storage.postgres.statementTimeoutMs,
    },
  };
}

export function assertServiceHostStorageBootReady(
  report: ServiceHostStorageBootPreflight,
): void {
  if (report.ready) return;
  throw new Error(
    `service-host storage preflight failed: ${report.blockers.join("; ")}`,
  );
}
