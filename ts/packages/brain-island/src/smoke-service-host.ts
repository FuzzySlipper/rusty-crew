import assert from "node:assert/strict";
import { existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { mkdtempSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createDebugApiClient } from "./debug-api-client.js";
import { startRustyCrewServiceHost } from "./service-host.js";

const root = mkdtempSync(join(tmpdir(), "rusty-crew-service-host-"));
const port = await openPort();
const token = "local-field-test-token";
writeRuntimeConfig(root);
let host = await startHost(root, port, token);

try {
  assert.equal(existsSync(join(root, "data", "engine")), true);
  assert.equal(existsSync(join(root, "run", "service.lock")), true);

  const health = await get("/v1/admin/healthz");
  assert.equal(health.status, 200);
  assert.equal(health.body.ok, true);

  const unauthenticatedReady = await get("/v1/admin/readyz");
  assert.equal(unauthenticatedReady.status, 401);
  assert.equal(unauthenticatedReady.body.ok, false);

  const ready = await get("/v1/admin/readyz", token);
  assert.equal(ready.status, 200);
  assert.equal(ready.body.ok, true);

  const diagnostics = await get("/v1/admin/diagnostics", token);
  assert.equal(diagnostics.status, 200);
  assert.equal(diagnostics.body.ok, true);
  assert.equal(
    diagnostics.body.data.overview.persistence.tableCounts.sessions,
    1,
  );

  const recentEvents = await get("/v1/admin/events/recent", token);
  assert.match(
    recentEvents.body.data.items[0]?.summary,
    /1 brains registered, 1 sessions created/,
  );

  const unsupported = await post("/v1/admin/control/maintenance", token, {
    reason: "smoke",
  });
  assert.equal(unsupported.status, 412);
  assert.equal(unsupported.body.ok, false);
  assert.equal(unsupported.body.error.reason_code, "unsupported_control");

  const created = await post("/v1/admin/control/sessions", token, {
    sessionId: "field-session-two",
    agentId: "field-agent",
    profileId: "field-profile",
    kind: "full",
    reason: "smoke",
  });
  assert.equal(created.status, 200);
  assert.equal(created.body.ok, true);
  assert.equal(
    created.body.data.outcome.affectedIds.sessionId,
    "field-session-two",
  );

  const afterCreate = await get("/v1/admin/diagnostics", token);
  assert.equal(
    afterCreate.body.data.overview.persistence.tableCounts.sessions,
    2,
  );

  const schedulerTick = await post("/v1/admin/control/scheduler/tick", token, {
    reason: "smoke",
  });
  assert.equal(schedulerTick.status, 200);
  assert.equal(schedulerTick.body.ok, true);

  const client = createDebugApiClient({
    baseUrl: `http://127.0.0.1:${port}`,
    bearerToken: token,
  });
  const debugContext = await client.directDebugContext({
    sessionId: "field-session",
    includePromptText: true,
  });
  assert.equal(debugContext.source, "direct_debug");
  assert.equal(debugContext.session.sessionId, "field-session");
  assert.equal(debugContext.selectedTools[0]?.name, "read_file");

  const directTurn = await client.requestDirectDebugTurn({
    sessionId: "field-session",
    actorId: "local-operator",
    body: "Exercise direct debug over the service host.",
  });
  assert.equal(directTurn.status, "accepted");

  await host.stop();
  host = await startHost(root, port, token);

  const restartedDiagnostics = await get("/v1/admin/diagnostics", token);
  assert.equal(
    restartedDiagnostics.body.data.overview.persistence.tableCounts.sessions,
    2,
  );
} finally {
  await host.stop();
  assert.equal(existsSync(join(root, "run", "service.lock")), false);
  rmSync(root, { recursive: true, force: true });
}

console.log("service host smoke passed");

async function get(path: string, bearer?: string) {
  const response = await fetch(`http://127.0.0.1:${port}${path}`, {
    headers: bearer ? { authorization: `Bearer ${bearer}` } : undefined,
  });
  return {
    status: response.status,
    body: (await response.json()) as any,
  };
}

async function post(path: string, bearer: string, body: unknown) {
  const response = await fetch(`http://127.0.0.1:${port}${path}`, {
    method: "POST",
    headers: {
      authorization: `Bearer ${bearer}`,
      "content-type": "application/json",
    },
    body: JSON.stringify(body),
  });
  return {
    status: response.status,
    body: (await response.json()) as any,
  };
}

function openPort(): Promise<number> {
  return new Promise((resolveOpenPort, rejectOpenPort) => {
    const server = createServer();
    server.once("error", rejectOpenPort);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      if (!address || typeof address === "string") {
        server.close();
        rejectOpenPort(new Error("failed to discover open TCP port"));
        return;
      }
      const port = address.port;
      server.close((error) => {
        if (error) rejectOpenPort(error);
        else resolveOpenPort(port);
      });
    });
  });
}

async function startHost(root: string, port: number, token: string) {
  return startRustyCrewServiceHost({
    env: {
      RUSTY_CREW_DATA_DIR: root,
      RUSTY_CREW_ADMIN_HOST: "127.0.0.1",
      RUSTY_CREW_ADMIN_ALLOW_LAN: "false",
      RUSTY_CREW_ADMIN_PORT: String(port),
      RUSTY_CREW_ADMIN_TOKEN: token,
    },
    now: () => "2026-06-21T03:30:00.000Z",
  });
}

function writeRuntimeConfig(root: string): void {
  const configDir = join(root, "config");
  const profilesDir = join(configDir, "profiles");
  mkdirSync(profilesDir, { recursive: true });
  writeFileSync(
    join(configDir, "service.json"),
    JSON.stringify(
      {
        profilesDir,
        brains: [{ profileId: "field-profile" }],
        sessions: [
          {
            sessionId: "field-session",
            agentId: "field-agent",
            profileId: "field-profile",
            kind: "full",
          },
        ],
      },
      null,
      2,
    ),
  );
  writeFileSync(
    join(profilesDir, "field-profile.json"),
    JSON.stringify(
      {
        profileId: "field-profile",
        modelConfig: {
          provider: "local",
          modelName: "deterministic",
        },
        toolPolicy: {
          requestedTools: ["read_file"],
        },
      },
      null,
      2,
    ),
  );
}
