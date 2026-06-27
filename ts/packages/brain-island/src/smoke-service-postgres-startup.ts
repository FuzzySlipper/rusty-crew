import assert from "node:assert/strict";
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
  console.log("service postgres startup smoke passed");
} finally {
  await host?.stop();
  rmSync(root, { recursive: true, force: true });
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
