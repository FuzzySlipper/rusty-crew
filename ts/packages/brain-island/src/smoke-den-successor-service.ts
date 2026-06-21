import assert from "node:assert/strict";
import { once } from "node:events";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { createServer as createHttpServer } from "node:http";
import { createServer as createTcpServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { startRustyCrewServiceHost } from "./service-host.js";

const root = mkdtempSync(join(tmpdir(), "rusty-crew-den-successor-service-"));
const adminPort = await openPort();
const gatewayRequests: {
  method: string;
  path: string;
  auth?: string;
  migrated?: string;
  body?: unknown;
}[] = [];
let deliveryListCount = 0;
const gateway = createHttpServer((request, response) => {
  let body = "";
  request.setEncoding("utf8");
  request.on("data", (chunk) => {
    body += chunk;
  });
  request.on("end", () => {
    gatewayRequests.push({
      method: request.method ?? "GET",
      path: request.url ?? "/",
      auth: request.headers.authorization,
      migrated: request.headers["x-den-migrated-functions"]?.toString(),
      body: body ? (JSON.parse(body) as unknown) : undefined,
    });
    response.setHeader("Content-Type", "application/json");
    if (request.url === "/health") {
      response.end(JSON.stringify({ status: "ok", service_name: "gateway" }));
      return;
    }
    if (request.url === "/v1/runtime/instances") {
      response.statusCode = 201;
      response.end(
        JSON.stringify({
          instance_id: "field-agent@rusty-crew",
          profile_identity: "field-profile",
          host: "test-host",
          state: "starting",
          started_at: "2026-06-21T12:00:00Z",
        }),
      );
      return;
    }
    if (
      request.url === "/v1/runtime/instances/field-agent%40rusty-crew/heartbeat"
    ) {
      response.end(
        JSON.stringify({
          instance_id: "field-agent@rusty-crew",
          profile_identity: "field-profile",
          host: "test-host",
          state: "active",
          started_at: "2026-06-21T12:00:00Z",
          last_heartbeat_at: "2026-06-21T12:00:01Z",
        }),
      );
      return;
    }
    if (request.url === "/v1/delivery/intents?state=pending") {
      deliveryListCount += 1;
      response.end(
        JSON.stringify(
          deliveryListCount === 1
            ? [
                {
                  id: 91,
                  target_identity: {
                    profile: "field-profile",
                    instance_id: "field-agent@rusty-crew",
                  },
                  state: "pending",
                  idempotency_key: "wake:ch42:field-profile:nonce",
                  source_ref: "wake://field-profile?body=hello%20from%20den",
                  channel_message_id: 7,
                  created_at: "2026-06-21T12:00:00Z",
                  expires_at: "2026-06-21T12:05:00Z",
                },
              ]
            : [],
        ),
      );
      return;
    }
    if (request.url === "/v1/delivery/intents/91/claim") {
      response.end(
        JSON.stringify({
          id: 91,
          target_identity: {
            profile: "field-profile",
            instance_id: "field-agent@rusty-crew",
          },
          state: "claimed",
          idempotency_key: "wake:ch42:field-profile:nonce",
          created_at: "2026-06-21T12:00:00Z",
          expires_at: "2026-06-21T12:05:00Z",
        }),
      );
      return;
    }
    if (request.url === "/v1/delivery/intents/91/events") {
      response.end(JSON.stringify({ accepted: true }));
      return;
    }
    if (request.url === "/v1/conversation/channels/42/messages") {
      response.statusCode = 201;
      response.end(JSON.stringify({ id: 92, channel_id: 42 }));
      return;
    }
    response.end(JSON.stringify({ accepted: true }));
  });
});
gateway.listen(0, "127.0.0.1");
await once(gateway, "listening");
const gatewayAddress = gateway.address();
assert.equal(typeof gatewayAddress, "object");
if (typeof gatewayAddress !== "object" || gatewayAddress === null) {
  throw new Error("expected fake Gateway address");
}

writeRuntimeConfig(root);
const host = await startRustyCrewServiceHost({
  env: {
    RUSTY_CREW_DATA_DIR: root,
    RUSTY_CREW_ADMIN_HOST: "127.0.0.1",
    RUSTY_CREW_ADMIN_ALLOW_LAN: "false",
    RUSTY_CREW_ADMIN_PORT: String(adminPort),
    RUSTY_CREW_ADMIN_AUTH_MODE: "none",
    DEN_SUCCESSOR_GATEWAY_URL: `http://127.0.0.1:${gatewayAddress.port}`,
    DEN_SUCCESSOR_RUNTIME_TOKEN: "runtime-token",
    DEN_SUCCESSOR_OBSERVATION_WRITE_TOKEN: "observation-write-token",
    DEN_SUCCESSOR_DELIVERY_TOKEN: "delivery-token",
    DEN_SUCCESSOR_CONVERSATION_WRITE_TOKEN: "conversation-write-token",
    RUSTY_CREW_DEN_RUNTIME_HEARTBEAT_INTERVAL_MS: "0",
    RUSTY_CREW_DEN_DELIVERY_POLL_INTERVAL_MS: "50",
  },
});

try {
  assert.deepEqual(gatewayRequests.slice(0, 4).map(requestKey), [
    "GET /health",
    "POST /v1/runtime/instances",
    "POST /v1/runtime/instances/field-agent%40rusty-crew/heartbeat",
    "POST /v1/observation/activity-events",
  ]);
  assert.equal(gatewayRequests[1]?.auth, "Bearer runtime-token");
  assert.equal(gatewayRequests[2]?.auth, "Bearer runtime-token");
  assert.equal(gatewayRequests[3]?.auth, "Bearer observation-write-token");
  assert.equal(gatewayRequests[1]?.migrated, "true");
  assert.equal(gatewayRequests[2]?.migrated, "true");
  assert.equal(gatewayRequests[3]?.migrated, "true");
  assert.equal(
    (gatewayRequests[3]?.body as { event_type?: string } | undefined)
      ?.event_type,
    "adapter_connected",
  );

  await waitUntil(
    () =>
      gatewayRequests.some(
        (request) =>
          request.method === "POST" &&
          request.path === "/v1/conversation/channels/42/messages",
      ),
    "delivery intent completion was projected to Conversation",
  );
  assert.equal(
    gatewayRequests.find(
      (request) => request.path === "/v1/delivery/intents/91/claim",
    )?.auth,
    "Bearer delivery-token",
  );
  assert.equal(
    gatewayRequests.find(
      (request) => request.path === "/v1/conversation/channels/42/messages",
    )?.auth,
    "Bearer conversation-write-token",
  );
  const lifecycleEvents = gatewayRequests
    .filter((request) => request.path === "/v1/delivery/intents/91/events")
    .map(
      (request) =>
        (request.body as { event_type?: string } | undefined)?.event_type,
    );
  assert.deepEqual(lifecycleEvents, ["running", "completed"]);

  const diagnostics = await fetch(
    `http://127.0.0.1:${adminPort}/v1/admin/events/recent`,
  );
  const body = (await diagnostics.json()) as {
    data: { items: { eventType: string; summary: string }[] };
  };
  assert.equal(
    body.data.items.some(
      (event) => event.eventType === "den_successor_gateway_connected",
    ),
    true,
  );

  console.log(
    JSON.stringify(
      {
        gatewayRequests: gatewayRequests.length,
        startupEvent: body.data.items[0]?.eventType,
        lifecycleEvents,
      },
      null,
      2,
    ),
  );
} finally {
  await host.stop();
  gateway.close();
  rmSync(root, { recursive: true, force: true });
}

function requestKey(request: { method: string; path: string }): string {
  return `${request.method} ${request.path}`;
}

async function waitUntil(
  predicate: () => boolean,
  description: string,
  timeoutMs = 3_000,
): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (predicate()) return;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  assert.fail(`timed out waiting for ${description}`);
}

function openPort(): Promise<number> {
  return new Promise((resolveOpenPort, rejectOpenPort) => {
    const server = createTcpServer();
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

function writeRuntimeConfig(targetRoot: string): void {
  const configDir = join(targetRoot, "config");
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
        channelBindings: [],
        mcpBindings: [],
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
        toolPolicy: { requestedTools: [] },
      },
      null,
      2,
    ),
  );
}
