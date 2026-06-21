import assert from "node:assert/strict";
import { createServer } from "node:http";
import { once } from "node:events";
import {
  createDenSuccessorGatewayClient,
  loadDenSuccessorGatewayConfig,
} from "./successor-gateway.js";

const requests: {
  method: string;
  path: string;
  auth?: string;
  migrated?: string;
  idempotencyKey?: string;
  body?: unknown;
}[] = [];

const server = createServer((request, response) => {
  let body = "";
  request.setEncoding("utf8");
  request.on("data", (chunk) => {
    body += chunk;
  });
  request.on("end", () => {
    requests.push({
      method: request.method ?? "GET",
      path: request.url ?? "/",
      auth: request.headers.authorization,
      migrated: request.headers["x-den-migrated-functions"]?.toString(),
      idempotencyKey: request.headers["idempotency-key"]?.toString(),
      body: body ? (JSON.parse(body) as unknown) : undefined,
    });

    response.setHeader("Content-Type", "application/json");
    if (request.url === "/health") {
      response.end(JSON.stringify({ status: "ok", service_name: "gateway" }));
      return;
    }
    if (request.url === "/v1/delivery/intents?state=pending") {
      response.end(
        JSON.stringify([
          {
            id: 7,
            target_identity: {
              profile: "field-prime",
              instance_id: "field-prime@rusty-crew",
            },
            state: "pending",
            idempotency_key: "wake:channel:field-prime:nonce",
            created_at: "2026-06-21T12:00:00Z",
            expires_at: "2026-06-21T12:05:00Z",
          },
        ]),
      );
      return;
    }
    if (request.url === "/v1/runtime/instances") {
      response.statusCode = 201;
      response.end(
        JSON.stringify({
          instance_id: "field-prime@rusty-crew",
          profile_identity: "field-prime",
          host: "den-k8plus",
          state: "starting",
          started_at: "2026-06-21T12:00:00Z",
        }),
      );
      return;
    }
    if (request.url === "/v1/runtime/instances/field-prime%40rusty-crew") {
      response.end(
        JSON.stringify({
          instance_id: "field-prime@rusty-crew",
          profile_identity: "field-prime",
          host: "den-k8plus",
          state: "active",
          started_at: "2026-06-21T12:00:00Z",
        }),
      );
      return;
    }
    if (
      request.url === "/v1/runtime/instances/field-prime%40rusty-crew/heartbeat"
    ) {
      response.end(
        JSON.stringify({
          instance_id: "field-prime@rusty-crew",
          profile_identity: "field-prime",
          host: "den-k8plus",
          state: "active",
          started_at: "2026-06-21T12:00:00Z",
          last_heartbeat_at: "2026-06-21T12:00:01Z",
        }),
      );
      return;
    }
    if (request.url === "/v1/delivery/intents/7/claim") {
      response.end(
        JSON.stringify({
          id: 7,
          target_identity: {
            profile: "field-prime",
            instance_id: "field-prime@rusty-crew",
          },
          state: "claimed",
          idempotency_key: "wake:channel:field-prime:nonce",
          created_at: "2026-06-21T12:00:00Z",
          expires_at: "2026-06-21T12:05:00Z",
        }),
      );
      return;
    }
    if (
      request.url ===
      "/v1/conversation/channels?project_id=rusty-crew&kind=agent_channel&limit=25"
    ) {
      response.end(
        JSON.stringify([
          {
            id: 42,
            slug: "rusty-crew-field-test",
            display_name: "field-prime",
            kind: "agent_channel",
            project_id: "rusty-crew",
            created_by: "rusty-crew",
            visibility: "normal",
            settings: {},
            created_at: "2026-06-21T12:00:00Z",
            updated_at: "2026-06-21T12:00:00Z",
          },
        ]),
      );
      return;
    }
    if (request.url === "/v1/conversation/channels") {
      response.statusCode = 201;
      response.end(
        JSON.stringify({
          id: 43,
          slug: "rusty-crew-field-test-2",
          display_name: "field-prime 2",
          kind: "agent_channel",
          project_id: "rusty-crew",
          created_by: "rusty-crew",
          visibility: "normal",
          settings: {},
          created_at: "2026-06-21T12:00:00Z",
          updated_at: "2026-06-21T12:00:00Z",
        }),
      );
      return;
    }
    if (request.url === "/v1/conversation/channels/42/messages?limit=5") {
      response.end(
        JSON.stringify([
          {
            id: 77,
            channel_id: 42,
            sender_type: "human",
            sender_identity: "operator",
            body: "wake the agent",
            message_kind: "message",
            source_kind: "conversation",
            created_at: "2026-06-21T12:00:00Z",
          },
        ]),
      );
      return;
    }
    response.end(JSON.stringify({ accepted: true }));
  });
});

server.listen(0, "127.0.0.1");
await once(server, "listening");
const address = server.address();
assert.equal(typeof address, "object");
if (typeof address !== "object" || address === null) {
  throw new Error("expected local smoke server address");
}

try {
  const config = loadDenSuccessorGatewayConfig({
    DEN_SUCCESSOR_GATEWAY_URL: `http://127.0.0.1:${address.port}`,
    DEN_SUCCESSOR_DELIVERY_TOKEN: "delivery-token",
    DEN_SUCCESSOR_RUNTIME_TOKEN: "runtime-token",
    DEN_SUCCESSOR_OBSERVATION_WRITE_TOKEN: "observation-write-token",
    DEN_SUCCESSOR_CONVERSATION_WRITE_TOKEN: "conversation-write-token",
    DEN_SUCCESSOR_CONVERSATION_READ_TOKEN: "conversation-read-token",
  });
  assert.ok(config);
  const client = createDenSuccessorGatewayClient(config);

  const health = await client.health();
  assert.equal(health.status, "ok");

  await client.createObservationActivityEvent({
    source_domain: "runtime",
    event_type: "adapter_connected",
    agent_identity: {
      profile: "field-prime",
      instance_id: "field-prime@rusty-crew",
      session_key: "session-1",
    },
    runtime_instance_id: "field-prime@rusty-crew",
    payload: {
      kind: "agent_activity.v1",
      schema_version: 1,
      summary: "field-prime connected",
      severity: "info",
      visibility: "agent",
      adapter: "pi-crew",
      surface: "gateway",
      session_key: "session-1",
    },
  });

  const registered = await client.registerRuntimeInstance({
    instance_id: "field-prime@rusty-crew",
    profile_identity: "field-prime",
    host: "den-k8plus",
  });
  assert.equal(registered.state, "starting");
  const heartbeated = await client.heartbeatRuntimeInstance(
    "field-prime@rusty-crew",
  );
  assert.equal(heartbeated.state, "active");
  const runtimeInstance = await client.getRuntimeInstance(
    "field-prime@rusty-crew",
  );
  assert.equal(runtimeInstance.instance_id, "field-prime@rusty-crew");

  const pending = await client.listDeliveryIntents("pending");
  assert.equal(pending[0]?.id, 7);
  await client.claimDeliveryIntent({
    id: 7,
    claimToken: "claim-token",
    claimedBy: {
      profile: "field-prime",
      instance_id: "field-prime@rusty-crew",
    },
  });

  await client.appendConversationMessage({
    channelId: 42,
    idempotencyKey: "channel_outbound:field-prime:nonce",
    message: {
      sender_type: "agent",
      sender_identity: "field-prime",
      body: "hello from rusty crew",
      message_kind: "message",
      source_kind: "rusty-crew",
      profile_identity: "field-prime",
      session_id: "session-1",
    },
  });
  const channels = await client.listConversationChannels({
    projectId: "rusty-crew",
    kind: "agent_channel",
    limit: 25,
  });
  assert.equal(channels[0]?.slug, "rusty-crew-field-test");
  const createdChannel = await client.createConversationChannel({
    slug: "rusty-crew-field-test-2",
    display_name: "field-prime 2",
    kind: "agent_channel",
    project_id: "rusty-crew",
    created_by: "rusty-crew",
    visibility: "normal",
  });
  assert.equal(createdChannel.id, 43);
  const messages = await client.listConversationMessages({
    channelId: 42,
    limit: 5,
  });
  assert.equal(messages[0]?.body, "wake the agent");

  assert.deepEqual(
    requests.map((request) => `${request.method} ${request.path}`),
    [
      "GET /health",
      "POST /v1/observation/activity-events",
      "POST /v1/runtime/instances",
      "POST /v1/runtime/instances/field-prime%40rusty-crew/heartbeat",
      "GET /v1/runtime/instances/field-prime%40rusty-crew",
      "GET /v1/delivery/intents?state=pending",
      "POST /v1/delivery/intents/7/claim",
      "POST /v1/conversation/channels/42/messages",
      "GET /v1/conversation/channels?project_id=rusty-crew&kind=agent_channel&limit=25",
      "POST /v1/conversation/channels",
      "GET /v1/conversation/channels/42/messages?limit=5",
    ],
  );
  assert.equal(
    requests[1]?.auth,
    "Bearer observation-write-token",
    "observation write token should be used only for observation writes",
  );
  assert.equal(requests[2]?.auth, "Bearer runtime-token");
  assert.equal(requests[3]?.auth, "Bearer runtime-token");
  assert.equal(requests[4]?.auth, "Bearer runtime-token");
  assert.equal(requests[5]?.auth, "Bearer delivery-token");
  assert.equal(requests[6]?.auth, "Bearer delivery-token");
  assert.equal(requests[7]?.auth, "Bearer conversation-write-token");
  assert.equal(requests[8]?.auth, "Bearer conversation-read-token");
  assert.equal(requests[9]?.auth, "Bearer conversation-write-token");
  assert.equal(requests[10]?.auth, "Bearer conversation-read-token");
  assert.equal(
    requests[7]?.idempotencyKey,
    "channel_outbound:field-prime:nonce",
  );
  assert.equal(
    requests.slice(1).every((request) => request.migrated === "true"),
    true,
  );

  console.log(
    JSON.stringify(
      {
        gatewayHealth: health.status,
        requests: requests.length,
        deliveryIntent: pending[0]?.state,
        runtimeState: heartbeated.state,
        conversationPath: requests[7]?.path,
        channelListPath: requests[8]?.path,
        readbackPath: requests[10]?.path,
      },
      null,
      2,
    ),
  );
} finally {
  server.close();
}
