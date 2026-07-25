import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import {
  createServer as createHttpServer,
  type Server,
  type ServerResponse,
} from "node:http";
import { createServer as createTcpServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { startRustyCrewServiceHost } from "@rusty-crew/service-host";

const root = mkdtempSync(join(tmpdir(), "rusty-external-memory-ready-"));
const adminPort = await openPort();
const memoryPort = await openPort();
const providerPort = await openPort();
const token = "external-memory-readiness-token";
const providerRequests: Array<{
  tools?: Array<{ name?: string; function?: { name?: string } }>;
}> = [];
let memoryMode: "healthy" | "hang" = "healthy";
const hangingResponses = new Set<ServerResponse>();

writeRuntimeConfig(root, providerPort);
const providerServer = await startProviderServer(providerPort);
const host = await startRustyCrewServiceHost({
  env: {
    RUSTY_CREW_DATA_DIR: root,
    RUSTY_CREW_ADMIN_HOST: "127.0.0.1",
    RUSTY_CREW_ADMIN_ALLOW_LAN: "false",
    RUSTY_CREW_ADMIN_PORT: String(adminPort),
    RUSTY_CREW_ADMIN_TOKEN: token,
    RUSTY_CREW_DEN_MEMORY_BASE_URL: `http://127.0.0.1:${memoryPort}`,
    RUSTY_CREW_DEN_MEMORY_TIMEOUT_MS: "100",
    RUSTY_CREW_SCHEDULER_TICK_INTERVAL_MS: "10000",
    RUSTY_CREW_WAKE_DISPATCH_INTERVAL_MS: "10000",
  },
});
let memoryServer: Server | undefined;

try {
  const startupSession = (await host.bridge.listSessions()).find(
    (session) => session.sessionId === "memory-session",
  );
  assert.deepEqual(
    startupSession?.toolProfile.tools.map((tool) => tool.name),
    [],
    "a configured but refused endpoint must omit external memory tools during profile loading",
  );

  memoryServer = await startMemoryServer(memoryPort);
  await rebuildProfile();
  await sendMessage("healthy memory wake", "healthy-memory-wake");
  await waitFor(() => providerRequests.length === 1);
  assert.deepEqual(memoryToolNames(providerRequests[0]), [
    "memory_read",
    "memory_recall",
    "memory_search",
  ]);

  memoryMode = "hang";
  await sendMessage("timed out memory wake", "timeout-memory-wake");
  await waitFor(() => providerRequests.length === 2);
  assert.deepEqual(
    memoryToolNames(providerRequests[1]),
    [],
    "wake-time readiness must remove external memory tools before the provider request",
  );

  await rebuildProfile();
  const surfaces = await get("/v1/admin/diagnostics/memory-surfaces");
  assert.equal(surfaces.status, 200, JSON.stringify(surfaces.body));
  const externalMemory = surfaces.body.data.items.find(
    (item: { surfaceId?: string }) => item.surfaceId === "external_memory",
  );
  assert.equal(externalMemory.availability, "unavailable");
  assert.equal(
    externalMemory.availabilityReasonCode,
    "memory_external_dependency_unavailable",
  );
  assert.equal(
    externalMemory.lastSafeError,
    "external memory readiness probe failed (timeout)",
  );
  assert.doesNotMatch(externalMemory.lastSafeError, /127\.0\.0\.1|memoryPort/);

  console.log("external memory readiness smoke passed");
} finally {
  for (const response of hangingResponses) response.destroy();
  if (memoryServer !== undefined) await closeServer(memoryServer);
  await host.stop();
  await closeServer(providerServer);
  rmSync(root, { recursive: true, force: true });
}

async function rebuildProfile(): Promise<void> {
  const response = await post(
    "/v1/admin/control/profiles/memory-profile/rebuild-brain/apply",
    { reason: "external memory readiness smoke" },
  );
  assert.equal(response.status, 200, JSON.stringify(response.body));
  assert.equal(response.body.data.outcome.status, "completed");
}

async function sendMessage(
  body: string,
  idempotencyKey: string,
): Promise<void> {
  const response = await post(
    "/v1/chat/sessions/memory-session/messages",
    {
      actor: { id: "smoke-operator", kind: "human" },
      body,
      client_message_id: idempotencyKey,
    },
    { "Idempotency-Key": idempotencyKey },
  );
  assert.equal(response.status, 202, JSON.stringify(response.body));
}

async function post(
  path: string,
  body: unknown,
  extraHeaders: Record<string, string> = {},
) {
  const response = await fetch(`http://127.0.0.1:${adminPort}${path}`, {
    method: "POST",
    headers: {
      authorization: `Bearer ${token}`,
      "content-type": "application/json",
      ...extraHeaders,
    },
    body: JSON.stringify(body),
  });
  return { status: response.status, body: (await response.json()) as any };
}

async function get(path: string) {
  const response = await fetch(`http://127.0.0.1:${adminPort}${path}`, {
    headers: { authorization: `Bearer ${token}` },
  });
  return { status: response.status, body: (await response.json()) as any };
}

function memoryToolNames(
  request:
    | { tools?: Array<{ name?: string; function?: { name?: string } }> }
    | undefined,
): string[] {
  return (request?.tools ?? [])
    .map((tool) => tool.name ?? tool.function?.name)
    .filter((name): name is string => name?.startsWith("memory_") === true)
    .sort();
}

function startProviderServer(port: number): Promise<Server> {
  const server = createHttpServer(async (request, response) => {
    if (request.method !== "POST" || request.url !== "/v1/chat/completions") {
      response.writeHead(404).end();
      return;
    }
    const chunks: Buffer[] = [];
    for await (const chunk of request) chunks.push(Buffer.from(chunk));
    providerRequests.push(JSON.parse(Buffer.concat(chunks).toString("utf8")));
    response.writeHead(200, { "content-type": "text/event-stream" });
    response.write(
      `data: ${JSON.stringify({
        id: `readiness-${providerRequests.length}`,
        object: "chat.completion.chunk",
        choices: [
          {
            index: 0,
            delta: { role: "assistant", content: "readiness checked" },
            finish_reason: null,
          },
        ],
      })}\n\n`,
    );
    response.write(
      `data: ${JSON.stringify({
        id: `readiness-${providerRequests.length}`,
        object: "chat.completion.chunk",
        choices: [{ index: 0, delta: {}, finish_reason: "stop" }],
      })}\n\n`,
    );
    response.end("data: [DONE]\n\n");
  });
  return listen(server, port);
}

function startMemoryServer(port: number): Promise<Server> {
  const server = createHttpServer(async (request, response) => {
    for await (const _chunk of request) {
      // Drain readiness requests before selecting the fixture behavior.
    }
    if (request.method !== "POST" || request.url !== "/v1/memories/search") {
      response.writeHead(404).end();
      return;
    }
    if (memoryMode === "hang") {
      hangingResponses.add(response);
      response.once("close", () => hangingResponses.delete(response));
      return;
    }
    response.writeHead(200, { "content-type": "application/json" });
    response.end(JSON.stringify({ memories: [], total: 0 }));
  });
  return listen(server, port);
}

function listen(server: Server, port: number): Promise<Server> {
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(port, "127.0.0.1", () => resolve(server));
  });
}

function closeServer(server: Server): Promise<void> {
  return new Promise((resolve, reject) => {
    server.close((error) => (error ? reject(error) : resolve()));
    server.closeAllConnections();
  });
}

function openPort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const server = createTcpServer();
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      if (!address || typeof address === "string") {
        server.close();
        reject(new Error("failed to allocate test port"));
        return;
      }
      const port = address.port;
      server.close((error) => (error ? reject(error) : resolve(port)));
    });
  });
}

function writeRuntimeConfig(dataDir: string, modelPort: number): void {
  const configDir = join(dataDir, "config");
  const profilesDir = join(configDir, "profiles");
  mkdirSync(profilesDir, { recursive: true });
  writeFileSync(
    join(configDir, "service.json"),
    JSON.stringify(
      {
        profilesDir,
        brains: [{ profileId: "memory-profile" }],
        sessions: [
          {
            sessionId: "memory-session",
            agentId: "memory-agent",
            profileId: "memory-profile",
            kind: "full",
          },
        ],
      },
      null,
      2,
    ),
  );
  writeFileSync(
    join(profilesDir, "memory-profile.json"),
    JSON.stringify(
      {
        profileId: "memory-profile",
        modelConfig: {
          provider: "local",
          modelName: "readiness-model",
          baseUrl: `http://127.0.0.1:${modelPort}/v1`,
        },
        toolPolicy: {
          requestedTools: [
            "memory_recall",
            "memory_read",
            "memory_search",
            "memory_store",
            "memory_propose",
          ],
        },
      },
      null,
      2,
    ),
  );
}

async function waitFor(predicate: () => boolean): Promise<void> {
  const deadline = Date.now() + 5_000;
  while (!predicate()) {
    if (Date.now() >= deadline) throw new Error("timed out waiting for wake");
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
}
