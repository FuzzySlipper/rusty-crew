import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { rmSync } from "node:fs";
import { mkdtempSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { startRustyCrewServiceHost } from "./service-host.js";

const databaseUrl = process.env.RUSTY_CREW_DATABASE_URL;
assert.ok(
  databaseUrl && databaseUrl.trim(),
  "RUSTY_CREW_DATABASE_URL must be set for service PostgreSQL startup smoke",
);

const root = mkdtempSync(join(tmpdir(), "rusty-crew-service-postgres-"));
const port = await openPort();
const schema = `rcsvc_${process.pid}_${Date.now()}`;

let host: Awaited<ReturnType<typeof startRustyCrewServiceHost>> | undefined;
try {
  host = await startRustyCrewServiceHost({
    env: {
      RUSTY_CREW_DATA_DIR: root,
      RUSTY_CREW_ADMIN_HOST: "127.0.0.1",
      RUSTY_CREW_ADMIN_ALLOW_LAN: "false",
      RUSTY_CREW_ADMIN_PORT: String(port),
      RUSTY_CREW_ADMIN_AUTH_MODE: "none",
      RUSTY_CREW_STORAGE_BACKEND: "postgres",
      RUSTY_CREW_POSTGRES_BOOT_MODE: "active",
      RUSTY_CREW_POSTGRES_DATABASE_URL_ENV: "RUSTY_CREW_DATABASE_URL",
      RUSTY_CREW_POSTGRES_SCHEMA: schema,
      RUSTY_CREW_DATABASE_URL: databaseUrl,
    },
  });
  const response = await fetch(`${host.url}/v1/admin/diagnostics/storage`);
  assert.equal(response.status, 200);
  const envelope = (await response.json()) as {
    ok: boolean;
    data?: {
      backend: string;
      configuredBackend?: string;
      activeCoordinationBackend?: string;
      selectorStatus?: string;
      implementationStatus?: string;
      postgres?: {
        bootMode: string;
        implementationStatus: string;
        productionReadiness?: {
          ready: boolean;
          status: string;
        };
      };
    };
  };
  assert.equal(envelope.ok, true);
  assert.equal(envelope.data?.backend, "postgres");
  assert.equal(envelope.data?.configuredBackend, "postgres");
  assert.equal(envelope.data?.activeCoordinationBackend, "postgres");
  assert.equal(envelope.data?.selectorStatus, "active");
  assert.equal(envelope.data?.implementationStatus, "active");
  assert.equal(envelope.data?.postgres?.bootMode, "active");
  assert.equal(envelope.data?.postgres?.implementationStatus, "active");
  assert.equal(envelope.data?.postgres?.productionReadiness?.ready, true);
  assert.equal(
    envelope.data?.postgres?.productionReadiness?.status,
    "ready",
  );

  const profileCreate = await postJson(`${host.url}/v1/admin/control/profiles`, {
    profileId: "postgres-created-profile",
    displayName: "Postgres Created Profile",
  });
  assert.equal(profileCreate.status, 200, JSON.stringify(profileCreate.body));
  const profileCreateEnvelope = profileCreate.body as {
    ok: boolean;
    data?: {
      outcome?: {
        result?: {
          profileId?: string;
          sessionId?: string;
          registryRecord?: {
            profileId?: string;
            lifecycleStatus?: string;
            revision?: number;
          };
        };
      };
    };
  };
  assert.equal(profileCreateEnvelope.ok, true);
  assert.equal(
    profileCreateEnvelope.data?.outcome?.result?.profileId,
    "postgres-created-profile",
  );
  assert.equal(
    profileCreateEnvelope.data?.outcome?.result?.sessionId,
    "postgres-created-profile-session",
  );
  assert.equal(
    profileCreateEnvelope.data?.outcome?.result?.registryRecord?.profileId,
    "postgres-created-profile",
  );
  assert.equal(
    profileCreateEnvelope.data?.outcome?.result?.registryRecord
      ?.lifecycleStatus,
    "active",
  );
  assert.equal(
    profileCreateEnvelope.data?.outcome?.result?.registryRecord?.revision,
    1,
  );
  assert.equal(
    existsSync(join(root, "config", "profiles", "postgres-created-profile.json")),
    true,
  );
  const createdProfileConfig = JSON.parse(
    readFileSync(
      join(root, "config", "profiles", "postgres-created-profile.json"),
      "utf8",
    ),
  ) as { displayName?: string; mcpConfig?: { toolProfile?: string } };
  assert.equal(createdProfileConfig.displayName, "Postgres Created Profile");
  assert.equal(
    createdProfileConfig.mcpConfig?.toolProfile,
    "postgres-created-profile",
  );

  const registryResponse = await fetch(
    `${host.url}/v1/admin/profiles/registry?limit=10`,
  );
  assert.equal(registryResponse.status, 200);
  const registryEnvelope = (await registryResponse.json()) as {
    ok: boolean;
    data?: {
      items?: Array<{
        source?: string;
        profileId?: string;
        lifecycleStatus?: string;
        fallbackStatus?: string;
      }>;
    };
  };
  assert.equal(registryEnvelope.ok, true);
  const registryRecord = registryEnvelope.data?.items?.find(
    (record) => record.profileId === "postgres-created-profile",
  );
  assert.equal(
    registryRecord?.source,
    "registry",
    JSON.stringify(registryEnvelope),
  );
  assert.equal(registryRecord?.lifecycleStatus, "active");
  assert.equal(registryRecord?.fallbackStatus, "registry_authoritative");

  const catalogResponse = await fetch(
    `${host.url}/v1/admin/storage/query-catalog`,
  );
  assert.equal(catalogResponse.status, 200);
  const catalogEnvelope = (await catalogResponse.json()) as {
    ok: boolean;
    data?: { items?: Array<{ id?: string; readOnly?: boolean }> };
  };
  assert.equal(catalogEnvelope.ok, true);
  assert.equal(
    catalogEnvelope.data?.items?.some(
      (query) => query.id === "storage.table_counts" && query.readOnly,
    ),
    true,
    JSON.stringify(catalogEnvelope),
  );

  const tableCounts = await postJson(
    `${host.url}/v1/admin/storage/query/storage.table_counts`,
    { limit: 50 },
  );
  assert.equal(tableCounts.status, 200);
  const tableCountsEnvelope = tableCounts.body as {
    ok: boolean;
    data?: {
      items?: Array<{ table?: string; rows?: number }>;
      data?: { backend?: string };
    };
  };
  assert.equal(tableCountsEnvelope.ok, true);
  assert.equal(
    tableCountsEnvelope.data?.data?.backend,
    "postgres",
    JSON.stringify(tableCountsEnvelope),
  );
  assert.equal(
    tableCountsEnvelope.data?.items?.some(
      (row) => row.table === "profile_registry" && (row.rows ?? 0) >= 1,
    ),
    true,
  );
  assert.equal(existsSync(join(root, "data", "coordination.sqlite3")), false);
  assert.equal(existsSync(join(root, "coordination.sqlite3")), false);
  console.log("service postgres startup smoke passed");
} finally {
  await host?.stop();
  rmSync(root, { recursive: true, force: true });
}

async function postJson(
  url: string,
  body: unknown,
): Promise<{ status: number; body: unknown }> {
  const response = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  return {
    status: response.status,
    body: await response.json(),
  };
}

async function openPort(): Promise<number> {
  const server = createServer();
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => resolve());
  });
  const address = server.address();
  assert.ok(address && typeof address === "object");
  const port = address.port;
  await new Promise<void>((resolve, reject) => {
    server.close((error) => (error ? reject(error) : resolve()));
  });
  return port;
}
